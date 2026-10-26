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
//! R4.7c (ADR-0346 revisit trigger (a); BL-31's live-sink member): the
//! sink's wire widens from the loopback batch POST to OTLP/HTTP (JSON) —
//! `SinkWire::OtlpTraces` folds the same durable delta through the
//! `gen_ai` convention lowering into an OTLP `ExportTraceServiceRequest`
//! for a real local OTel-compatible collector. The mediation contract is
//! unchanged — the same `net_egress` request, the same durable ordering —
//! and the delivery row now requires the sink's acknowledgement: a
//! forwarded non-2xx answer is [`PollOutcome::NotAcknowledged`], never a
//! delivered row (USD-0 loopback only; a collector-absent leg is the leg
//! runner's honest `skipped(<reason>)`, never a fabricated delivery).
//!
//! An unlisted sink is an egress fact, not a telemetry event: a refused
//! destination leaves `security.egress.decided{deny}` rows and *no*
//! `measurement.export.delivered` — the subscriber reports the refusal,
//! never a fabricated delivery.

use std::collections::BTreeSet;

use hharness_containment::egress::EgressRequest;
use hharness_containment::policy::EgressProtocol;
use hharness_env::egress::{EgressError, EgressMediator, MediatedOutcome};
use hharness_env::events::ScopeChain;
use hharness_ledger::event::{Cursor, Direction, EventEnvelope};
use hharness_telemetry::events::{export_delivered_payload, ExportDelivered};
use hharness_telemetry::export::{
    cap_strings, export_events, sampled_keep, ExportBatch, LossEntry,
};
use hharness_telemetry::sinks::{ContentClass, SamplingMode, SinkPolicy};
use hharness_telemetry::{genai, otlp};
use hharness_wire::json::Json;

/// The sink's wire coordinate — `net_egress` destination triple plus the
/// dialect the body is spelled in.
#[derive(Debug, Clone, PartialEq)]
pub struct SinkDestination {
    /// The destination host as spelled (`host_raw` — the mediator
    /// normalizes).
    pub host: String,
    /// The destination port (loopback only at this stage — a live sink
    /// stays `127.0.0.1`, USD 0).
    pub port: u16,
    /// The request path the delivery POSTs to (`/v1/traces` for an
    /// OTLP/HTTP collector).
    pub path: String,
    /// The wire dialect (the batch `Json::Arr` or the OTLP trace
    /// request).
    pub wire: SinkWire,
}

/// One wire fold's payload: the request `body`, the content `classes`
/// the delivery row declares, and the `loss_report_ref` the batch
/// carries (None when the fold lost nothing).
type WireFold = (String, BTreeSet<ContentClass>, Option<String>);

/// The sink's wire dialect — how the durable delta is spelled on the
/// wire. `BatchJson` is the landed Stage-1 form; `OtlpTraces` is the
/// R4.7c live-sink form — both ride the same `net_egress` request and
/// the same delivered-on-acknowledgement rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SinkWire {
    /// `Json::Arr(batch.rows)` — the landed loopback-fixture body.
    #[default]
    BatchJson,
    /// An OTLP `ExportTraceServiceRequest` document — the
    /// convention-lowered spans (R4.7c; ADR-0346 trigger (a)).
    OtlpTraces {
        /// The `service.name` the collector attributes the spans to.
        service_name: String,
    },
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
    /// The wire fired (`decided{allow}` + the `network.calls` charge
    /// are durable) but the sink did not acknowledge the delivery — no
    /// `measurement.export.delivered` is appended (R4.7c: delivered
    /// means sink-acknowledged — a 2xx wire answer — never merely
    /// forwarded).
    NotAcknowledged {
        /// The decided row's event id.
        decided_ref: String,
        /// The wire's HTTP status.
        status: u16,
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
        // 2. The §5h.1 §7 policy gates — consent/rate-limit run before
        //    any wire, on both dialects (the mediation contract is
        //    unchanged).
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
        let page_lo = page.events.iter().map(|e| e.seq).min().unwrap_or(0);
        let page_hi = page.events.iter().map(|e| e.seq).max().unwrap_or(0);
        // 3. The wire fold — the destination's dialect spells the same
        //    durable delta. `None` means the delta is entirely outside
        //    the wire's projection (no span, no loss — an OTLP sink
        //    tailing a model-free prefix): consumed quietly, never
        //    sent, never claimed — the watermark still advances.
        let Some((body, classes, loss_report_ref)) = self.wire_fold(&page.events)? else {
            self.watermark = page_hi;
            return Ok(PollOutcome::Quiet);
        };
        // 4. The egress leg — one `net_egress` request carrying the
        //    wire body (content-free under the policy's own gates —
        //    the mediator scans it for sentinels like any other
        //    payload).
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
            body: Some(body),
            body_readers: None,
            credential_sentinels: vec![],
        };
        match med.handle(&req, chain).map_err(SubscriberError::Egress)? {
            MediatedOutcome::Forwarded {
                decided_ref,
                response,
                ..
            } => {
                // A delivered row requires the sink's acknowledgement
                // (R4.7c) — forwarded-but-not-acknowledged is durable
                // egress, never a claimed delivery.
                if !(200..300).contains(&response.status) {
                    return Ok(PollOutcome::NotAcknowledged {
                        decided_ref,
                        status: response.status,
                    });
                }
            }
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
        // 5. The delivery row — kernel-provenance, audit-grade, on the
        //    run's persisted writer-lease generation (the S3.1 append
        //    site; works on closed runs).
        let seq_range = (page_lo, page_hi);
        let record = ExportDelivered {
            sink_id: self.policy.sink_id.clone(),
            view_kind: "events".to_string(),
            seq_range,
            content_classes: classes,
            loss_report_ref,
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
        self.watermark = page_hi;
        Ok(PollOutcome::Delivered {
            seq: env.seq,
            seq_range,
            loss_report_ref: record.loss_report_ref,
        })
    }

    /// `wire_fold(events)` — the destination's dialect over the durable
    /// delta: `Some((body, classes, loss_report_ref))` to send, or
    /// `None` when the delta is entirely outside the wire's projection
    /// (consumed quietly — the caller advances the watermark). Both
    /// dialects run the policy's gates: `BatchJson` inside
    /// `export_events`; `OtlpTraces` here — the same `sampled_keep`
    /// decision, the same `max_field_bytes` cap (CC1 — one spelling per
    /// rule), and the declared content classes withhold the one
    /// free-text-adjacent convention member
    /// (`gen_ai.evaluation.explanation` — named `content_class_withheld`
    /// loss, never silent).
    fn wire_fold(&self, events: &[EventEnvelope]) -> Result<Option<WireFold>, SubscriberError> {
        match &self.destination.wire {
            SinkWire::BatchJson => {
                let batch = export_events(&self.policy, events).map_err(|e| {
                    SubscriberError::Telemetry {
                        detail: format!("export_events: {e}"),
                    }
                })?;
                Ok(Some((
                    Json::Arr(batch.rows.clone()).to_canonical_string(),
                    batch.classes.clone(),
                    batch.loss_report_ref(),
                )))
            }
            SinkWire::OtlpTraces { service_name } => {
                self.policy
                    .validate()
                    .map_err(|e| SubscriberError::Telemetry {
                        detail: format!("sink policy: {e}"),
                    })?;
                let mut loss = Vec::new();
                let mut refs = Vec::with_capacity(events.len());
                let mut ts_by_seq = std::collections::BTreeMap::new();
                for env in events {
                    ts_by_seq.insert(env.seq, env.ts.as_str());
                    if let SamplingMode::Ratio | SamplingMode::ParentBased =
                        self.policy.sampling.mode
                    {
                        let ratio = self.policy.sampling.ratio_ppm.unwrap_or(0);
                        if !sampled_keep(
                            self.policy.sampling.mode,
                            ratio,
                            &env.event_id,
                            &env.run_id,
                        ) {
                            loss.push(LossEntry {
                                kind: "sampled_out",
                                detail: env.event_id.clone(),
                            });
                            continue;
                        }
                    }
                    refs.push((
                        env.seq,
                        env.run_id.as_str(),
                        env.event_id.as_str(),
                        env.class.as_str(),
                        &env.payload,
                    ));
                }
                let export = genai::lower_run_refs(&refs);
                if export.spans.is_empty() && export.loss.is_empty() {
                    // The delta is entirely outside the span projection
                    // — consumed, never sent, never claimed.
                    return Ok(None);
                }
                loss.extend(export.loss);
                let mut pairs = Vec::with_capacity(export.spans.len());
                for s in &export.spans {
                    let mut s = s.clone();
                    let event_id = s
                        .get("attributes")
                        .and_then(|a| a.get(hharness_telemetry::lift::hh_attr::EVENT_ID))
                        .and_then(Json::as_str)
                        .unwrap_or("span")
                        .to_string();
                    if let Json::Obj(m) = &mut s {
                        if let Some(Json::Obj(a)) = m.get_mut("attributes") {
                            if !self.policy.content_classes.contains(&ContentClass::Content)
                                && a.remove("gen_ai.evaluation.explanation").is_some()
                            {
                                loss.push(LossEntry {
                                    kind: "content_class_withheld",
                                    detail: format!("{event_id}.gen_ai.evaluation.explanation"),
                                });
                            }
                            let capped = cap_strings(
                                self.policy.max_field_bytes,
                                &event_id,
                                "attributes",
                                Json::Obj(a.clone()),
                                &mut loss,
                            );
                            m.insert("attributes".to_string(), capped);
                        }
                    }
                    // `seq` names the source event's anchor `ts`; a
                    // span without it anchors at "" — `otlp` names the
                    // loss, never guesses an instant.
                    let ts = s
                        .get("seq")
                        .and_then(Json::as_int)
                        .and_then(|seq| ts_by_seq.get(&(seq as u64)))
                        .copied()
                        .unwrap_or("")
                        .to_string();
                    pairs.push((s, ts));
                }
                let (doc, mut wire_loss) = otlp::trace_service_request(&pairs, service_name);
                loss.append(&mut wire_loss);
                let batch = ExportBatch {
                    rows: Vec::new(),
                    loss,
                    classes: self.policy.content_classes.clone(),
                    seq_range: (0, 0),
                };
                let loss_report_ref = batch.loss_report_ref();
                Ok(Some((
                    doc.to_canonical_string(),
                    batch.classes,
                    loss_report_ref,
                )))
            }
        }
    }
}
