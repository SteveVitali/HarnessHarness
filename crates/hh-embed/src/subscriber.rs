//! `subscriber` — the R2.14 exporter-as-subscriber seam (§5h.1 §6;
//! ADR-0061 D5; DF-S1.14-4): a sink is a `net_egress` destination under
//! EP3 mediation, so the exporter never opens its own wire — it tails the
//! run's durable prefix through `read`, lowers the delta under its
//! [`SinkPolicy`], hands the batch to the caller's [`EgressMediator`] as
//! one `net_egress` request, and — only on a forwarded outcome — appends
//! `measurement.export.delivered` through the canonical
//! `commit_kernel_row_for` path (kernel-provenance, audit-grade; the run's
//! *persisted* writer-lease generation, so a closed run still exports —
//! the same append site `kernel.bundle{deliver_sink}` exercises).
//!
//! An unlisted sink is an egress fact, not a telemetry event: a refused
//! destination leaves `security.egress.decided{deny}` rows and *no*
//! `measurement.export.delivered` — the subscriber reports the refusal,
//! never a fabricated delivery (fixture-verified ceiling: loopback
//! fixtures only; no live backend claim).

use std::collections::BTreeSet;

use hh_containment::egress::EgressRequest;
use hh_containment::policy::EgressProtocol;
use hh_env::egress::{EgressError, EgressMediator, MediatedOutcome};
use hh_env::events::ScopeChain;
use hh_ledger::event::{Cursor, Direction};
use hh_telemetry::events::{export_delivered_payload, ExportDelivered};
use hh_telemetry::export::export_events;
use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

/// The fixture sink's wire coordinate — `net_egress` destination triple.
#[derive(Debug, Clone, PartialEq)]
pub struct SinkDestination {
    /// The destination host as spelled (`host_raw` — the mediator
    /// normalizes).
    pub host: String,
    /// The destination port (loopback fixture ports only — the
    /// fixture-verified ceiling).
    pub port: u16,
    /// The request path the batch POSTs to.
    pub path: String,
}

/// The exporter participant's own egress attribution — its `net_egress`
/// request is attributed like any other (the subscriber is a participant,
/// never a trusted side channel; `token` resolves through the run's
/// `TokenResolver` exactly as a tool call's would).
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriberEgress {
    /// The attribution token.
    pub token: String,
    /// The claimed effect the token binds.
    pub effect_id: String,
    /// The environment handle the subscription runs under.
    pub env_handle: String,
    /// The decorating tool-call coordinate.
    pub tool_call_id: String,
}

/// The subscription — the tailing state plus the sink declaration.
#[derive(Debug, Clone)]
pub struct ExportSubscriber {
    /// The run it tails.
    pub run_id: String,
    /// The sink policy (`content_classes`/`sampling`/`redaction`/rate
    /// limits/`requires_consent` — the §5h.1 §7 contract verbatim).
    pub policy: SinkPolicy,
    /// The granted `sink_id` set (`requires_consent` gates on it —
    /// `ConsentMissing`, never a silent send).
    pub consents: BTreeSet<String>,
    /// The last exported durable seq (the tail resumes at
    /// `watermark + 1` — the R2.2 `read` tail over the compacted
    /// prefix).
    pub watermark: u64,
    /// The destination the sink's wire goes to.
    pub destination: SinkDestination,
    /// The subscriber's egress attribution.
    pub egress: SubscriberEgress,
}

/// What a `poll` produced.
#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    /// No new durable rows at the watermark — nothing lowered, nothing
    /// sent, nothing appended.
    Quiet,
    /// The batch forwarded through the mediator and the
    /// `measurement.export.delivered` row is durable.
    Delivered {
        /// The delivered row's seq.
        seq: u64,
        /// The `seq_range` the delivery covers.
        seq_range: (u64, u64),
        /// The `loss_report_ref` when the lowering carried loss.
        loss_report_ref: Option<String>,
    },
    /// The egress decision refused (`deny`) — the `security.egress.*`
    /// rows are the durable record; no `measurement.export.delivered`
    /// was appended.
    Refused {
        /// The decided row's event id.
        decided_ref: String,
        /// The refusal detail.
        detail: String,
    },
}

/// The subscriber's failure modes — typed, never a warning.
#[derive(Debug)]
pub enum SubscriberError {
    /// The durable tail failed (`read` — unknown run/cursor).
    Ledger {
        /// What failed.
        detail: String,
    },
    /// The lowering refused (`SinkPolicy` invalid, consent missing,
    /// rate-limited — the `deliver` pre-wire gates).
    Telemetry {
        /// What failed.
        detail: String,
    },
    /// The mediated path errored (ledger/budget/transport — the
    /// `decided{allow}` row may already be durable; never retried
    /// silently — the caller decides).
    Egress(EgressError),
}

impl ExportSubscriber {
    /// `poll(med, chain)` — one tail step: `read` the delta at the
    /// watermark, lower it under the policy, run the sink's
    /// `net_egress` request through the caller's mediator (the only
    /// wire — the mediator's `decided`/`egress` rows precede any
    /// outcome), and append `measurement.export.delivered` on a
    /// forwarded outcome.
    ///
    /// `med` mediates the *exported* run (`med.run_id == self.run_id` —
    /// a mismatched mediator is a configuration error, refused).
    pub fn poll(
        &mut self,
        med: &mut EgressMediator<'_>,
        chain: &ScopeChain,
    ) -> Result<PollOutcome, SubscriberError> {
        if med.run_id != self.run_id {
            return Err(SubscriberError::Ledger {
                detail: format!(
                    "mediator run {} != subscription run {}",
                    med.run_id, self.run_id
                ),
            });
        }
        // 1. The durable tail — `read` from the watermark (inclusive
        //    cursor at the next seq; compacted prefixes read through
        //    the R2.2 tail).
        let page = med
            .store
            .read(
                &self.run_id,
                Cursor::Seq(self.watermark.saturating_add(1)),
                None,
                Direction::Fwd,
                10_000,
            )
            .map_err(|e| SubscriberError::Ledger {
                detail: format!("read tail: {e}"),
            })?;
        if page.events.is_empty() {
            return Ok(PollOutcome::Quiet);
        }
        // 2. The §5h.1 §7 lowering — consent/rate-limit gates inside
        //    `deliver`'s pre-wire contract run here too (the consent
        //    check is explicit; the rate cap applies to the batch).
        if self.policy.requires_consent && !self.consents.contains(&self.policy.sink_id) {
            return Err(SubscriberError::Telemetry {
                detail: format!("consent missing for sink {}", self.policy.sink_id),
            });
        }
        if let Some(rl) = &self.policy.rate_limit {
            if page.events.len() as u64 > rl.events_per_sec {
                return Err(SubscriberError::Telemetry {
                    detail: format!(
                        "rate_limited: {} rows > {} events_per_sec",
                        page.events.len(),
                        rl.events_per_sec
                    ),
                });
            }
        }
        let batch =
            export_events(&self.policy, &page.events).map_err(|e| SubscriberError::Telemetry {
                detail: format!("export_events: {e}"),
            })?;
        // 3. The egress leg — one `net_egress` request carrying the
        //    canonical batch (the body is content-free under the
        //    policy's own content-class gate — the mediator scans it
        //    for sentinels like any other payload).
        let req = EgressRequest {
            token: self.egress.token.clone(),
            effect_id: Some(self.egress.effect_id.clone()),
            tool_call_id: self.egress.tool_call_id.clone(),
            env_handle: self.egress.env_handle.clone(),
            protocol: EgressProtocol::Http,
            host_raw: self.destination.host.clone(),
            resolved_addrs: vec![],
            port: self.destination.port,
            method: Some("POST".to_string()),
            path: Some(self.destination.path.clone()),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Some(Json::Arr(batch.rows.clone()).to_canonical_string()),
            body_readers: None,
            credential_sentinels: vec![],
        };
        match med.handle(&req, chain).map_err(SubscriberError::Egress)? {
            MediatedOutcome::Forwarded { .. } => {}
            MediatedOutcome::Refused {
                decided_ref,
                reason,
                ..
            } => {
                return Ok(PollOutcome::Refused {
                    decided_ref,
                    detail: format!("egress denied: {}", reason.as_str()),
                });
            }
            MediatedOutcome::RefusedCredential {
                decided_ref, code, ..
            } => {
                return Ok(PollOutcome::Refused {
                    decided_ref,
                    detail: format!("credential refused: {code:?}"),
                });
            }
            MediatedOutcome::Asked {
                decided_ref,
                permission_id,
                ..
            } => {
                return Ok(PollOutcome::Refused {
                    decided_ref,
                    detail: format!("egress asked (pending {permission_id})"),
                });
            }
        }
        // 4. The delivery row — kernel-provenance, audit-grade, on the
        //    run's persisted writer-lease generation (the S3.1 append
        //    site; works on closed runs).
        let record = ExportDelivered {
            sink_id: self.policy.sink_id.clone(),
            view_kind: "events".to_string(),
            seq_range: batch.seq_range,
            content_classes: batch.classes.clone(),
            loss_report_ref: batch.loss_report_ref(),
        };
        let env = med
            .store
            .commit_kernel_row_for(
                "kernel:exporter",
                &self.run_id,
                "measurement.export.delivered",
                export_delivered_payload(&record),
                vec![],
                vec![],
            )
            .map_err(|e| SubscriberError::Ledger {
                detail: format!("delivered append: {e}"),
            })?;
        self.watermark = batch.seq_range.1;
        Ok(PollOutcome::Delivered {
            seq: env.seq,
            seq_range: batch.seq_range,
            loss_report_ref: record.loss_report_ref,
        })
    }
}
