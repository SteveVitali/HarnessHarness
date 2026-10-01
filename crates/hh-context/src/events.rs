//! Ledger payload builders for the context/memory classes (§5c.1/§5c.3 —
//! every row is a canonical record: `context.assembled`,
//! `context.artefact.delivered`, `context.artefact.activated`,
//! `context.retrieval.completed`, `context.memory.written`,
//! `context.memory.invalidated`, `context.memory.read`; the
//! `control.decision` row `ContextWindowExceeded` emits).
//!
//! The builders produce the *payload* — the caller appends through
//! `hh-ledger`'s append path (the store/assembler never mint `Event`s
//! themselves; `EventSink` is the injection seam). Large `items[]` lists are
//! offloaded by the caller via `itemize`/`itemize_offloaded` (the payload
//! carries content addresses, never raw bulk — spec §5c.1's
//! "offload lists under a size threshold" rule).

use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

use crate::codec::label_json;
use crate::memory::{MemoryVersion, WriteContext};
use crate::plan::{ContextPlan, PlannedItem};
use crate::retrieve::RetrievalReport;
use crate::vocab::RevocationReason;

/// The payload members `context.assembled` carries when `items[]` is
/// offloaded past `INLINE_ITEMS_LIMIT` (§5c.1; the offload contract).
pub const INLINE_ITEMS_LIMIT: usize = 256;

/// `EventSink` — the sink `assemble`/`retrieve` emit payload rows into
/// (same shape as `hh-gateway`'s: `class + payload`; the *caller* owns
/// `append` — I-PROV).
pub trait EventSink {
    /// Record `class` + `payload`.
    fn emit(&mut self, class: &str, payload: Json);
}

/// A collecting sink (tests + the kernel's drain path).
#[derive(Debug, Default)]
pub struct CollectSink {
    /// `(class, payload)` in emit order.
    pub events: Vec<(String, Json)>,
}

impl EventSink for CollectSink {
    fn emit(&mut self, class: &str, payload: Json) {
        self.events.push((class.to_string(), payload));
    }
}

/// `context.assembled` — §5c.1's payload:
/// `{plan_id, model_call_id, derived_from{run_id, seq, view_hash}, layout_ref,
///  policy_ref, estimator_ref, reserved, occupancy_estimate, context_label,
///  items[] | items_ref, omitted[], pending_expansions[], static_hash,
///  compilation_stamp_hash, compaction_state, assembly_ms{measured_at}}`.
/// `items[]` inlines below `INLINE_ITEMS_LIMIT`, else `items_ref` carries the
/// blob address (the caller performs the `put_blob`).
pub fn assembled_payload(
    plan: &ContextPlan,
    layout_ref: &str,
    policy_ref: &str,
    compaction_state: &str,
    assembly_ms: u64,
    items_blob_ref: Option<String>,
) -> Json {
    let item_json = |it: &PlannedItem, slot: &str| {
        let mut v = vec![
            ("candidate_id", Json::str(it.candidate_id.clone())),
            ("context_item_id", Json::str(it.context_item_id.clone())),
            ("delivery_id", Json::str(it.delivery_id.clone())),
            ("slot_id", Json::str(slot)),
            ("authority", Json::str(it.authority.as_str())),
            ("label", label_json(&it.label)),
            ("tokens", Json::Int(it.tokens as i64)),
            ("state", Json::str(it.state.as_str())),
            (
                "delivered_by_reference",
                Json::Bool(it.delivered_by_reference),
            ),
        ];
        if let Some(a) = &it.artefact_id {
            v.push(("artefact_id", Json::str(a.clone())));
        }
        if let Some(d) = &it.derived_from {
            v.push(("derived_from", Json::str(d.clone())));
        }
        Json::obj(v)
    };
    let mut all_items = Vec::new();
    for f in &plan.slots {
        for it in &f.items {
            all_items.push(item_json(it, &f.slot_id));
        }
    }
    let pending: Vec<&PlannedItem> = plan
        .slots
        .iter()
        .flat_map(|f| f.items.iter())
        .filter(|i| i.delivered_by_reference)
        .collect();
    let mut v = vec![
        ("plan_id", Json::str(plan.plan_id.clone())),
        ("model_call_id", Json::str(plan.model_call_id.clone())),
        (
            "derived_from",
            Json::obj([
                ("run_id", Json::str(plan.derived_from.run_id.clone())),
                ("seq", Json::Int(plan.derived_from.seq as i64)),
                ("view_hash", Json::str(plan.derived_from.view_hash.clone())),
            ]),
        ),
        ("layout_ref", Json::str(layout_ref)),
        ("policy_ref", Json::str(policy_ref)),
        ("estimator_ref", Json::str(plan.estimator_ref.clone())),
        ("reserved", Json::Int(plan.reserved as i64)),
        (
            "occupancy_estimate",
            Json::Int(plan.occupancy_estimate as i64),
        ),
        ("context_label", label_json(&plan.context_label)),
        (
            "omitted",
            Json::Arr(
                plan.omitted
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("candidate_id", Json::str(o.candidate_id.clone())),
                            ("reason", Json::str(o.reason.as_str())),
                            ("evidence", Json::str(o.evidence.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "pending_expansions",
            Json::Arr(
                pending
                    .iter()
                    .map(|i| Json::str(i.delivery_id.clone()))
                    .collect(),
            ),
        ),
        ("static_hash", Json::str(plan.static_hash.clone())),
        ("compaction_state", Json::str(compaction_state)),
        (
            "assembly_ms",
            Json::obj([
                ("value", Json::Int(assembly_ms as i64)),
                ("measured_at", Json::str("runtime")),
            ]),
        ),
    ];
    if all_items.len() <= INLINE_ITEMS_LIMIT && items_blob_ref.is_none() {
        v.push(("items", Json::Arr(all_items)));
    } else {
        v.push((
            "items_ref",
            Json::str(items_blob_ref.unwrap_or_else(|| "unwritten".to_string())),
        ));
    }
    Json::obj(v)
}

/// `context.artefact.delivered` — `{artefact_id, delivery_id, kind,
/// rendering_ref, by_reference}` (§5c.1).
pub fn artefact_delivered_payload(
    artefact_id: &str,
    delivery_id: &str,
    kind: &str,
    rendering_ref: Option<&str>,
    by_reference: bool,
) -> Json {
    let mut v = vec![
        ("artefact_id", Json::str(artefact_id)),
        ("delivery_id", Json::str(delivery_id)),
        ("kind", Json::str(kind)),
        ("by_reference", Json::Bool(by_reference)),
    ];
    if let Some(r) = rendering_ref {
        v.push(("rendering_ref", Json::str(r)));
    }
    Json::obj(v)
}

/// `context.artefact.activated` — `{artefact_id, delivery_id, detector,
/// signal}` (§5c.1 — the activation trace; `detector` is the deterministic
/// detector at C0).
pub fn artefact_activated_payload(
    artefact_id: &str,
    delivery_id: &str,
    detector: &str,
    signal: &str,
) -> Json {
    Json::obj([
        ("artefact_id", Json::str(artefact_id)),
        ("delivery_id", Json::str(delivery_id)),
        ("detector", Json::str(detector)),
        ("signal", Json::str(signal)),
    ])
}

/// `context.retrieval.completed` — `{model_call_id, request_hash, query_kind,
/// layers, watermark, report}` (§5c.3 P7).
pub fn retrieval_completed_payload(
    model_call_id: &str,
    request_hash: &str,
    query_kind: &str,
    layers: &[String],
    watermark: (String, u64),
    report: &RetrievalReport,
) -> Json {
    Json::obj([
        ("model_call_id", Json::str(model_call_id)),
        ("request_hash", Json::str(request_hash)),
        ("query_kind", Json::str(query_kind)),
        (
            "layers",
            Json::Arr(layers.iter().map(|l| Json::str(l.clone())).collect()),
        ),
        (
            "watermark",
            Json::obj([
                ("run_id", Json::str(watermark.0.clone())),
                ("seq", Json::Int(watermark.1 as i64)),
            ]),
        ),
        ("report", report.to_json()),
    ])
}

/// `context.memory.read` — `{query_ref, until_seq, delivered[], withheld[],
/// filter_order_attestation}` (§5c.3; the `filter_order_attestation` records
/// `validity→authority→readers→rank`).
pub fn memory_read_payload(
    query_ref: &str,
    until_seq: u64,
    delivered: &[String],
    withheld: &[crate::lifecycle::Withheld],
) -> Json {
    Json::obj([
        ("query_ref", Json::str(query_ref)),
        ("until_seq", Json::Int(until_seq as i64)),
        (
            "delivered",
            Json::Arr(delivered.iter().map(|d| Json::str(d.clone())).collect()),
        ),
        (
            "withheld",
            Json::Arr(
                withheld
                    .iter()
                    .map(|w| {
                        Json::obj([
                            ("version_id", Json::str(w.version_id.clone())),
                            ("state", Json::str(w.state.as_str())),
                            ("reason", Json::str(w.reason.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "filter_order_attestation",
            Json::str("validity,authority,readers,rank"),
        ),
    ])
}

/// `context.memory.written` — `{memory_id, version_id, kind, scope, label,
/// contract_hash, justifications, supersedes?, lease_generation, store}`
/// (§5c.3; `contract_hash` is the contract's `idp` digest).
pub fn memory_written_payload(v: &MemoryVersion, ctx: &WriteContext) -> Json {
    let contract_hash =
        hh_identity::idp::idp_id("memory_contract.1", serde_json_contract(v).as_bytes());
    let mut m = vec![
        ("memory_id", Json::str(v.semantic_id.clone())),
        ("version_id", Json::str(v.version_id.clone())),
        ("kind", Json::str(v.kind.as_str())),
        ("scope", Json::str(v.scope.as_str())),
        ("label", label_json(&v.label)),
        ("contract_hash", Json::str(contract_hash)),
        (
            "justifications",
            Json::Arr(
                v.justifications
                    .iter()
                    .map(|j| {
                        Json::obj([
                            ("kind", Json::str(j.kind.as_str())),
                            ("ref", Json::str(crate::memory::vref_str(&j.ref_))),
                            ("at", Json::str(crate::codec::event_ref_str(&j.at))),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("lease_generation", Json::Int(ctx.lease_generation as i64)),
        ("store", Json::str("memory")),
    ];
    if let Some(s) = &v.supersedes_claim {
        m.push((
            "supersedes",
            Json::obj([
                ("version_id", Json::str(s.version_id.clone())),
                ("reason", Json::str(s.reason.as_str())),
            ]),
        ));
    }
    Json::obj(m)
}

fn serde_json_contract(v: &MemoryVersion) -> String {
    // The contract's canonical form (the body member).
    match v.body_json() {
        Json::Obj(m) => m
            .get("contract")
            .map(|c| c.to_canonical_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `context.memory.invalidated` — `{version_id, reason, by, replacement?,
/// fired_stamp?}` (§5c.4).
pub fn memory_invalidated_payload(
    version_id: &str,
    reason: RevocationReason,
    revoker: &ProvenanceRecord,
    replacement: Option<String>,
) -> Json {
    let mut v = vec![
        ("version_id", Json::str(version_id)),
        ("reason", Json::str(reason.as_str())),
        (
            "by",
            Json::str(crate::memory::render_origin(&revoker.origin)),
        ),
    ];
    if let Some(r) = replacement {
        v.push(("replacement", Json::str(r)));
    }
    Json::obj(v)
}

/// The `control.decision` payload `ContextWindowExceeded` produces at
/// Stage 0/1 (§5c.1: `ContextWindowExceeded → CompactionRequired` handled as
/// stop — the typed error rides the control plane's closed vocabulary; the
/// `reason` member is `compaction_required`).
pub fn stop_decision_payload(required_tokens: u64, cap: u64) -> Json {
    Json::obj([
        ("kind", Json::str("stop")),
        ("reason", Json::str("compaction_required")),
        ("decider", Json::str("context.builder")),
        (
            "triggered_by",
            Json::Arr(vec![Json::obj([
                ("required_tokens", Json::Int(required_tokens as i64)),
                ("cap", Json::Int(cap as i64)),
            ])]),
        ),
    ])
}

/// `context.compaction.started` — `{trigger, requirement,
/// strategy_variant_ref?, pipeline_index, proposal_id?, occupancy_before}`
/// (§5c.2; emitted once per `compact` run before the ladder walks).
pub fn compaction_started(
    trigger: &crate::compact::CompactionTrigger,
    occupancy_before: u64,
    assessment: &crate::compact::Assessment,
) -> Json {
    Json::obj([
        ("trigger", Json::str(trigger.as_str())),
        (
            "requirement",
            Json::str(match assessment.requirement {
                crate::compact::Requirement::None => "none",
                crate::compact::Requirement::Soft => "soft",
                crate::compact::Requirement::Hard => "hard",
            }),
        ),
        ("occupancy_before", Json::Int(occupancy_before as i64)),
        (
            "target_reclaim",
            Json::Int(assessment.target_reclaim as i64),
        ),
        ("min_reclaim", Json::Int(assessment.min_reclaim as i64)),
    ])
}

/// `context.compaction.completed` — the `CompactionRecord` row (§5c.2):
/// `{compaction_id, variant_ref, trigger, requirement, status, ops_applied[],
/// forgotten[], summary_ref?, context_label_after, tokens_freed,
/// derived_from[], summariser_usage?, duration_ms, pipeline_index,
/// fallback_variant?}` — plus `accounting{charged_to: subject, attribution:
/// harness_overhead.compaction}` (AC-R-2.4.2-7; `model_calls: 0` at C0 — a
/// summariser call would post `control.budget.consumed` before dispatch).
pub fn compaction_completed(record: &crate::compact::CompactionRecord) -> Json {
    let mut v = vec![
        ("compaction_id", Json::str(record.compaction_id.clone())),
        ("variant_ref", Json::str(record.variant_ref.clone())),
        ("trigger", Json::str(record.trigger.as_str())),
        (
            "requirement",
            Json::str(match record.requirement {
                crate::compact::Requirement::None => "none",
                crate::compact::Requirement::Soft => "soft",
                crate::compact::Requirement::Hard => "hard",
            }),
        ),
        (
            "status",
            Json::str(match record.status {
                crate::compact::CompactionStatus::Applied => "applied",
                crate::compact::CompactionStatus::Ineffective => "ineffective",
                crate::compact::CompactionStatus::Failed => "failed",
            }),
        ),
        (
            "ops_applied",
            Json::Arr(
                record
                    .ops_applied
                    .iter()
                    .map(|o| Json::str(o.kind()))
                    .collect(),
            ),
        ),
        (
            "forgotten",
            Json::Arr(
                record
                    .forgotten
                    .iter()
                    .map(|f| Json::str(f.clone()))
                    .collect(),
            ),
        ),
        ("tokens_freed", Json::Int(record.tokens_freed as i64)),
        // §5c.2 `context.compaction.completed{…, label_after, reclaimed}` —
        // the post-compaction join (I-LABEL) is part of the durable record.
        ("label_after", label_json(&record.context_label_after)),
        ("reclaimed", Json::Int(record.tokens_freed as i64)),
        (
            "derived_from",
            Json::Arr(
                record
                    .derived_from
                    .iter()
                    .map(|f| Json::str(f.clone()))
                    .collect(),
            ),
        ),
        ("pipeline_index", Json::Int(record.pipeline_index as i64)),
        ("duration_ms", Json::Int(record.duration_ms as i64)),
        (
            "accounting",
            Json::obj([
                ("charged_to", Json::str("subject")),
                ("attribution", Json::str("harness_overhead.compaction")),
                ("model_calls", Json::Int(0)),
            ]),
        ),
    ];
    if let Some(s) = &record.summary_ref {
        v.push(("summary_ref", Json::str(s.clone())));
    }
    if let Some(f) = &record.fallback_variant {
        v.push(("fallback_variant", Json::str(f.clone())));
    }
    if let Some(u) = &record.summariser_usage {
        v.push(("summariser_usage", u.clone()));
    }
    Json::obj(v)
}
