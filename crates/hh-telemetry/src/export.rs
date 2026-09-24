//! The §5h.1 §7 export path — `export_*` lowers a durable prefix (or a view)
//! under a [`SinkPolicy`], and [`deliver`] appends the *only* ledger write an
//! exporter may make: `measurement.export.delivered` (§2.2 — kernel-appended,
//! provenance-mandatory, content-free ⇒ audit-grade; the row confirms by
//! readback, never by trusting side state).
//!
//! The lowering is honest loss: sampled-out rows, content members the policy
//! doesn't permit, redacted/pseudonymized leaves and `max_field_bytes`
//! truncations each record a `loss[]` entry — a `TelemetryLossReport` the
//! delivery row references by digest (`loss_report_ref`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_digest;
use hh_ledger::event::{Event, EventEnvelope, Producer, SeqRange};
use hh_ledger::store::{Lease, Store};
use hh_ledger::views::{View, ViewKind};
use hh_wire::json::Json;

use crate::errors::TelemetryError;
use crate::events::{export_delivered_payload, ExportDelivered};
use crate::sinks::{ContentClass, Redaction, SamplingMode, SinkPolicy};

/// The `idp/1` domain for `loss_report_ref` digests.
pub const LOSS_REPORT_DOMAIN: &str = "telemetry.loss_report";

/// The `idp/1` domain for deterministic sampling decisions (`ratio` keeps the
/// row iff `H(event_id)[0:8] mod 1e6 < ratio_ppm` — reproducible across
/// rebuilds, never a live RNG).
const SAMPLE_DOMAIN: &str = "telemetry.sample";

/// A lowering loss entry — one per dropped/reduced thing.
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// What happened (`sampled_out`, `content_class_withheld`,
    /// `field_truncated`, `member_dropped`, `pseudonymized`).
    pub kind: &'static str,
    /// The affected row/member.
    pub detail: String,
}

impl LossEntry {
    /// The canonical form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind)),
            ("detail", Json::str(&self.detail)),
        ])
    }
}

/// What `export_*` produced — the rows a sink receives plus the loss record.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportBatch {
    /// The lowered rows (canonical JSON — the sink's wire form).
    pub rows: Vec<Json>,
    /// The lowering loss entries.
    pub loss: Vec<LossEntry>,
    /// The content classes the batch actually carries.
    pub classes: BTreeSet<ContentClass>,
    /// The durable seq range covered (from/to of the folded prefix).
    pub seq_range: (u64, u64),
}

impl ExportBatch {
    /// The `TelemetryLossReport` digest — `sha256:`-ref over the canonical
    /// loss list (absent members never serialize as the empty list; a zero-loss
    /// delivery carries no `loss_report_ref`).
    pub fn loss_report_ref(&self) -> Option<String> {
        if self.loss.is_empty() {
            return None;
        }
        let report = Json::Arr(self.loss.iter().map(LossEntry::to_json).collect());
        Some(format!(
            "sha256:{}",
            idp_digest(LOSS_REPORT_DOMAIN, report.to_canonical_string().as_bytes())
        ))
    }
}

/// The deterministic keep decision — `H(domain, key)[0:8]` as a u32 ppm below
/// `ratio_ppm` keeps the row. `parent_based` keys on the trace root (the whole
/// run's rows share the decision).
fn sampled_keep(mode: SamplingMode, ratio_ppm: i64, event_id: &str, run_id: &str) -> bool {
    let key = match mode {
        SamplingMode::Ratio => event_id,
        SamplingMode::ParentBased => run_id,
        SamplingMode::All => return true,
    };
    let h = idp_digest(SAMPLE_DOMAIN, key.as_bytes());
    let v = u32::from_str_radix(&h[..8], 16).unwrap_or(0) % hh_budget::quantity::PPM_SCALE as u32;
    (v as i64) < ratio_ppm
}

/// The members a sink at each level receives. `content` adds the payload;
/// `diagnostic` adds the kernel-internal stamps. L3 `diagnostic` never
/// combines with `content` (the policy refuses the mix — see
/// `SinkPolicy::validate`).
fn lowered_event(policy: &SinkPolicy, env: &EventEnvelope, loss: &mut Vec<LossEntry>) -> Json {
    let classes = &policy.content_classes;
    let mut m = BTreeMap::new();
    // L0 — accounting: identity + ordering only.
    if classes.contains(&ContentClass::Accounting) {
        m.insert("event_id".into(), Json::str(&env.event_id));
        m.insert("class".into(), Json::str(&env.class));
        m.insert("seq".into(), Json::Int(env.seq as i64));
        m.insert("ts".into(), Json::str(&env.ts));
    }
    // L1 — structural: scope/parent/links/digests/provenance label, no content.
    if classes.contains(&ContentClass::Structural) {
        m.insert("run_id".into(), Json::str(&env.run_id));
        let mut sc = BTreeMap::new();
        for (k, v) in [
            ("turn_id", &env.scope.turn_id),
            ("model_call_id", &env.scope.model_call_id),
            ("tool_call_id", &env.scope.tool_call_id),
            ("effect_id", &env.scope.effect_id),
            ("child_run_id", &env.scope.child_run_id),
            ("branch_id", &env.scope.branch_id),
        ] {
            if let Some(v) = v {
                sc.insert(k.into(), Json::str(v));
            }
        }
        m.insert("scope".into(), Json::Obj(sc));
        m.insert("parent_event_id".into(), Json::str(&env.parent_event_id));
        m.insert(
            "causes".into(),
            Json::Arr(
                env.causes
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("run_id", Json::str(&c.run_id)),
                            ("event_id", Json::str(&c.event_id)),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert("hash".into(), Json::str(&env.hash));
        m.insert("prev_hash".into(), Json::str(&env.prev_hash));
        if let Some(p) = &env.provenance {
            m.insert("provenance".into(), p.to_json());
        }
    }
    // L2 — content: the payload under redaction + the byte cap.
    if classes.contains(&ContentClass::Content) {
        m.insert(
            "payload".into(),
            lower_payload(policy, &env.payload, &env.event_id, "payload", loss),
        );
    }
    // L3 — diagnostic: the kernel stamps (never content members — the policy
    // refuses the `{content, diagnostic}` mix).
    if classes.contains(&ContentClass::Diagnostic) {
        m.insert(
            "producer".into(),
            Json::obj([
                ("component_class", Json::str(&env.producer.component_class)),
                (
                    "component_variant_ref",
                    Json::str(&env.producer.component_variant_ref),
                ),
                ("participant_ref", Json::str(&env.producer.participant_ref)),
            ]),
        );
        m.insert("plane".into(), Json::str(env.plane.as_str()));
        m.insert(
            "schema_version".into(),
            Json::Int(env.schema_version as i64),
        );
        m.insert(
            "lease_generation".into(),
            Json::Int(env.lease_generation as i64),
        );
    }
    Json::Obj(m)
}

/// The payload under the policy — redaction first, then the byte cap on every
/// string leaf.
fn lower_payload(
    policy: &SinkPolicy,
    payload: &Json,
    event_id: &str,
    path: &str,
    loss: &mut Vec<LossEntry>,
) -> Json {
    let redacted = match policy.redaction {
        Redaction::None => payload.clone(),
        Redaction::Allowlist => allowlist(payload, event_id, path, loss),
        Redaction::Pseudonymize => pseudonymize(payload, event_id, path, loss),
    };
    cap_strings(policy.max_field_bytes, event_id, path, redacted, loss)
}

/// The closed member-name predicate `allowlist`/`pseudonymize` share — the
/// content-free telemetry shape (`*_id`, `*_ref`, `*_ms`, `*_bytes`,
/// `*_count`, `*_ppm`, `*_hash`, `*_digest`, closed tags and counters). A
/// member outside it is content.
fn safe_member(name: &str) -> bool {
    const SAFE: &[&str] = &[
        "kind",
        "class",
        "status",
        "decision",
        "decider",
        "outcome",
        "verdict",
        "reason",
        "stop_reason",
        "currency",
        "unit",
        "convention",
        "normalizer_ref",
        "measured_at",
        "usage",
        "timing",
    ];
    SAFE.contains(&name)
        || name.ends_with("_id")
        || name.ends_with("_ref")
        || name.ends_with("_ms")
        || name.ends_with("_bytes")
        || name.ends_with("_count")
        || name.ends_with("_ppm")
        || name.ends_with("_hash")
        || name.ends_with("_digest")
        || name.ends_with("_no")
}

/// `allowlist` — keep safe-named members (recursively); drop the rest with a
/// loss entry per dropped member.
fn allowlist(payload: &Json, event_id: &str, path: &str, loss: &mut Vec<LossEntry>) -> Json {
    match payload {
        Json::Obj(m) => {
            let mut out = BTreeMap::new();
            for (k, v) in m {
                let p = format!("{path}.{k}");
                if safe_member(k) {
                    out.insert(k.clone(), allowlist(v, event_id, &p, loss));
                } else {
                    loss.push(LossEntry {
                        kind: "member_dropped",
                        detail: format!("{event_id}:{p}"),
                    });
                }
            }
            Json::Obj(out)
        }
        Json::Arr(items) => Json::Arr(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| allowlist(v, event_id, &format!("{path}[{i}]"), loss))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `pseudonymize` — keep the shape and the safe members verbatim; replace
/// other string leaves with `pseudo:<H>` (deterministic — the same leaf
/// pseudonymizes identically across exports).
fn pseudonymize(payload: &Json, event_id: &str, path: &str, loss: &mut Vec<LossEntry>) -> Json {
    match payload {
        Json::Obj(m) => Json::Obj(
            m.iter()
                .map(|(k, v)| {
                    let p = format!("{path}.{k}");
                    if safe_member(k) {
                        (k.clone(), pseudonymize(v, event_id, &p, loss))
                    } else {
                        (k.clone(), pseudo_leaf(v, event_id, &p, loss))
                    }
                })
                .collect(),
        ),
        other => pseudo_leaf(other, event_id, path, loss),
    }
}

fn pseudo_leaf(v: &Json, event_id: &str, path: &str, loss: &mut Vec<LossEntry>) -> Json {
    match v {
        Json::Str(s) => {
            loss.push(LossEntry {
                kind: "pseudonymized",
                detail: format!("{event_id}:{path}"),
            });
            Json::str(format!(
                "pseudo:{}",
                &idp_digest("telemetry.pseudonym", s.as_bytes())[..16]
            ))
        }
        Json::Arr(items) => Json::Arr(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| pseudo_leaf(v, event_id, &format!("{path}[{i}]"), loss))
                .collect(),
        ),
        Json::Obj(m) => Json::Obj(
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        pseudo_leaf(v, event_id, &format!("{path}.{k}"), loss),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `max_field_bytes` — a string leaf over the cap truncates with a loss entry
/// (the marker keeps the truncation visible downstream).
fn cap_strings(max: u64, event_id: &str, path: &str, v: Json, loss: &mut Vec<LossEntry>) -> Json {
    match v {
        Json::Str(s) if s.len() as u64 > max => {
            loss.push(LossEntry {
                kind: "field_truncated",
                detail: format!("{event_id}:{path} {} bytes", s.len()),
            });
            let keep = max.saturating_sub(16) as usize;
            Json::str(format!(
                "{}…(+{} bytes)",
                &s[..keep.min(s.len())],
                s.len() - keep.min(s.len())
            ))
        }
        Json::Arr(items) => Json::Arr(
            items
                .into_iter()
                .enumerate()
                .map(|(i, v)| cap_strings(max, event_id, &format!("{path}[{i}]"), v, loss))
                .collect(),
        ),
        Json::Obj(m) => Json::Obj(
            m.into_iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        cap_strings(max, event_id, &format!("{path}.{k}"), v, loss),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// `export_events(policy, events)` — the per-event lowering over a prefix.
/// `content_classes` gates which members a row carries; `sampling` drops rows
/// deterministically (`ratio`/`parent_based` per row-id / run-id hash — never
/// a live RNG, so a rebuilt export is identical).
pub fn export_events(
    policy: &SinkPolicy,
    events: &[EventEnvelope],
) -> Result<ExportBatch, TelemetryError> {
    policy.validate()?;
    let mut rows = Vec::new();
    let mut loss = Vec::new();
    let mut lo = u64::MAX;
    let mut hi = 0u64;
    for env in events {
        lo = lo.min(env.seq);
        hi = hi.max(env.seq);
        if let SamplingMode::Ratio | SamplingMode::ParentBased = policy.sampling.mode {
            let ratio = policy.sampling.ratio_ppm.unwrap_or(0);
            if !sampled_keep(policy.sampling.mode, ratio, &env.event_id, &env.run_id) {
                loss.push(LossEntry {
                    kind: "sampled_out",
                    detail: env.event_id.clone(),
                });
                continue;
            }
        }
        rows.push(lowered_event(policy, env, &mut loss));
    }
    Ok(ExportBatch {
        rows,
        loss,
        classes: policy.content_classes.clone(),
        seq_range: (if lo == u64::MAX { 0 } else { lo }, hi),
    })
}

/// `export_view(policy, view)` — a whole-view export (a `metric_view` export
/// refuses `sampling` below `all`: a scorecard is never computed from a
/// sample — ADR-0044 D3; AC-R-2.9.1-11).
pub fn export_view(policy: &SinkPolicy, view: &View) -> Result<ExportBatch, TelemetryError> {
    policy.validate()?;
    if view.kind == ViewKind::MetricView && policy.sampling.mode != SamplingMode::All {
        return Err(TelemetryError::SamplingForbidden {
            sink_id: policy.sink_id.clone(),
            view_kind: view.kind.as_str(),
        });
    }
    Ok(ExportBatch {
        rows: vec![view.payload.clone()],
        loss: Vec::new(),
        classes: policy.content_classes.clone(),
        seq_range: (0, view.derived_from_seq.unwrap_or(0)),
    })
}

/// `deliver(store, run, lease, policy, consents, view_kind, batch)` — the only
/// ledger write an exporter may make: append
/// `measurement.export.delivered{sink_id, view_kind, seq_range,
/// content_classes[], loss_report_ref}` (kernel producer + provenance — the
/// class is audit-grade/kernel-origin). `requires_consent` sinks need the
/// manifest's grant (`consents` is the granted `sink_id` set — the manifest
/// consent member's Stage-1 seam; absent ⇒ `ConsentMissing`, never a silent
/// send).
///
/// `rate_limit.events_per_sec` bounds a single delivery's row count at Stage
/// 1 (one delivery spans a ≤1s window — multi-window pacing lands with the
/// sink bindings).
pub fn deliver(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    policy: &SinkPolicy,
    consents: &BTreeSet<String>,
    view_kind: &str,
    batch: &ExportBatch,
) -> Result<SeqRange, TelemetryError> {
    if policy.requires_consent && !consents.contains(&policy.sink_id) {
        return Err(TelemetryError::ConsentMissing {
            sink_id: policy.sink_id.clone(),
        });
    }
    if let Some(rl) = &policy.rate_limit {
        if batch.rows.len() as u64 > rl.events_per_sec {
            return Err(TelemetryError::RateLimited {
                sink_id: policy.sink_id.clone(),
                rows: batch.rows.len() as u64,
                limit: rl.events_per_sec,
            });
        }
    }
    let record = ExportDelivered {
        sink_id: policy.sink_id.clone(),
        view_kind: view_kind.to_string(),
        seq_range: batch.seq_range,
        content_classes: batch.classes.clone(),
        loss_report_ref: batch.loss_report_ref(),
    };
    let ev = Event {
        event_id: store.alloc_id("event"),
        class: "measurement.export.delivered".to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:exporter"),
        scope: Default::default(),
        parent_event_id: store
            .head_event_id(run_id)
            .unwrap_or_else(|_| hh_ledger::ids::ROOT_EVENT.to_string()),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel(
            "kernel:exporter",
            0,
        )),
        content_kind: None,
        payload: export_delivered_payload(&record),
    };
    store
        .append(run_id, lease, vec![ev])
        .map_err(|e| TelemetryError::UnknownSinkMember {
            detail: format!("export.delivered append refused: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::Sampling;
    use std::collections::BTreeMap;

    fn env(seq: u64, class: &str, payload: Json) -> EventEnvelope {
        EventEnvelope {
            event_id: format!("e{seq}"),
            run_id: "r1".into(),
            seq,
            ts: "2026-01-01T00:00:00.000Z".into(),
            hlc: None,
            plane: hh_ledger::event::EventPlane::Lifecycle,
            class: class.into(),
            schema_version: 1,
            producer: Producer::kernel("kernel:test"),
            participant_class: hh_ledger::manifest::ParticipantClass::Native,
            observability_level: [hh_ledger::manifest::ObservabilityLevel::Ledger]
                .into_iter()
                .collect(),
            durability: hh_ledger::classes::Durability::Ledger,
            scope: Default::default(),
            lease_generation: 0,
            parent_event_id: hh_ledger::ids::ROOT_EVENT.into(),
            causes: Vec::new(),
            refs: Vec::new(),
            ir_refs: Vec::new(),
            surface_ids: BTreeMap::new(),
            provenance: None,
            prev_hash: hh_ledger::ids::GENESIS_HASH.into(),
            payload,
            hash: format!("h{seq}"),
        }
    }

    #[test]
    fn accounting_sink_carries_no_content() {
        let p = SinkPolicy::accounting("s0");
        let e = env(
            1,
            "model.call.requested",
            Json::obj([("prompt", Json::str("secret words"))]),
        );
        let b = export_events(&p, &[e]).unwrap();
        let row = b.rows[0].to_canonical_string();
        assert!(!row.contains("secret words"));
        assert!(!row.contains("payload"));
        assert_eq!(b.seq_range, (1, 1));
    }

    #[test]
    fn ratio_sampling_is_deterministic_and_reports_loss() {
        let mut p = SinkPolicy::accounting("s1");
        p.sampling = Sampling {
            mode: SamplingMode::Ratio,
            ratio_ppm: Some(1), // 1e-6 — almost everything drops
        };
        let events: Vec<_> = (0..50)
            .map(|i| env(i, "control.budget.consumed", Json::Null))
            .collect();
        let b1 = export_events(&p, &events).unwrap();
        let b2 = export_events(&p, &events).unwrap();
        assert_eq!(b1, b2); // deterministic — a rebuilt export is identical
        assert!(b1.rows.len() < 50);
        assert!(b1.loss.iter().any(|l| l.kind == "sampled_out"));
        assert!(b1.loss_report_ref().is_some());
    }

    #[test]
    fn max_field_bytes_truncates_with_loss() {
        let mut p = SinkPolicy::accounting("s2");
        p.content_classes.insert(ContentClass::Content);
        p.requires_consent = true;
        p.max_field_bytes = 24;
        let e = env(
            1,
            "context.observation.recorded",
            Json::obj([("body", Json::str("x".repeat(500)))]),
        );
        let b = export_events(&p, &[e]).unwrap();
        assert!(b.loss.iter().any(|l| l.kind == "field_truncated"));
        let s = b.rows[0].to_canonical_string();
        assert!(s.contains("bytes)") && !s.contains(&"x".repeat(500)));
    }

    #[test]
    fn consent_gate_refuses_an_ungranted_content_sink() {
        let mut p = SinkPolicy::accounting("s3");
        p.content_classes.insert(ContentClass::Content);
        p.requires_consent = true;
        // The delivery path refuses before any append — the runtime half of
        // AC-R-2.9.1-8.
        let batch = ExportBatch {
            rows: vec![],
            loss: vec![],
            classes: p.content_classes.clone(),
            seq_range: (0, 0),
        };
        let dir = std::env::temp_dir().join(format!("hh-tel-exp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::open_test(&dir, 1_000).unwrap();
        let (run, lease) = s
            .open_run(
                hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
                "writer",
            )
            .unwrap();
        assert!(matches!(
            deliver(
                &mut s,
                &run,
                &lease,
                &p,
                &BTreeSet::new(),
                "event_batch",
                &batch
            ),
            Err(TelemetryError::ConsentMissing { .. })
        ));
        let granted: BTreeSet<String> = ["s3".to_string()].into_iter().collect();
        assert!(deliver(&mut s, &run, &lease, &p, &granted, "event_batch", &batch).is_ok());
        let readback = crate::views::sink_deliveries(
            &s.read(
                &run,
                hh_ledger::event::Cursor::Seq(0),
                None,
                hh_ledger::event::Direction::Fwd,
                1000,
            )
            .unwrap()
            .events,
        );
        assert_eq!(readback.len(), 1);
        assert_eq!(readback[0].sink_id, "s3");
    }
}
