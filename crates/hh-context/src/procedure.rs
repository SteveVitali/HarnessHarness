//! §5c.5 — the context-plane half of the C0/Stage-2 procedure slice
//! (R-2.4.5⁰): the `ProcedureSelector` (C0 = `index_all_under_budget`, OQ-198's
//! fixed default — capped by the `procedures` slot's `budget_share`), the
//! `procedure_index`/`procedure_body` candidate projections, the deterministic
//! `activated` expansion chain, and the deterministic `followed` detector's
//! evidence half.
//!
//! The chain (AC-R-2.4.5-2): a `procedure_index` candidate delivers
//! `handle_only` — `context.artefact.delivered{kind: procedure_index,
//! by_reference: true}` fires at assemble; the model asking for the body runs
//! [`activate`], which emits `context.artefact.activated{detector:
//! deterministic, signal: loaded}` and produces the expanded
//! `procedure_body` candidate the *next* assemble admits; a tool call naming a
//! capability in the procedure's `allowed_capabilities` with the index's
//! `delivery_id` in its `causes[]` is the deterministic `followed` signal —
//! [`detect_followed`] returns the evidence the caller ledger-records as
//! `verification.artefact.followed` (the verdict row is the verification
//! plane's; this module returns the deterministic evidence).

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::events::EventSink;
use crate::plan::{Candidate, ContextPlan, Estimate};
use crate::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};

/// `context.procedure.selected{method, candidates[], delivered[]}` — the
/// `ProcedureSelector` event (§5c.5). The C0 method is the fixed
/// `index_all_under_budget` (OQ-198): every procedure whose checkable
/// preconditions pass, in id order, until the slot's `budget_share` runs out.
pub const SELECT_METHOD: &str = "index_all_under_budget";

/// One selectable procedure index: what the selector ranks and budgets.
#[derive(Debug, Clone)]
pub struct ProcedureIndexEntry {
    /// The procedure's semantic id (the `procedure_index` artefact id).
    pub procedure_id: String,
    /// The index's estimated token cost.
    pub estimate: Estimate,
    /// The label the index delivers at (the index is a projection — `⊔` of
    /// the profile's `index` block, capped `external` for lifted text).
    pub label: hh_provenance::label::Label,
    /// The index's provenance record.
    pub provenance: hh_provenance::record::ProvenanceRecord,
    /// Whether the run's checkable preconditions passed — `check_preconditions`
    /// (hh-hir) decides before selection; a violated precondition omits the
    /// procedure (`OmissionReason::Precondition`).
    pub preconditions_passed: bool,
    /// The `PreconditionUncheckable` notes counted on the selection record.
    pub uncheckable: u64,
    /// The compiled identity (`procedure_index:<semantic_id>` is the C0 form).
    pub index_item_id: String,
}

/// `select_procedures(indexes, budget_share_tokens, sink)` — the C0
/// `ProcedureSelector`: index-all under the slot's budget share, in
/// `procedure_id` order (deterministic); emits `context.procedure.selected`.
/// Precondition-violated entries are not candidates (the omission lands at
/// assemble with `reason: precondition`).
pub fn select_procedures(
    indexes: &[ProcedureIndexEntry],
    budget_share_tokens: u64,
    sink: &mut dyn EventSink,
) -> Vec<Candidate> {
    let mut sorted: Vec<&ProcedureIndexEntry> =
        indexes.iter().filter(|e| e.preconditions_passed).collect();
    sorted.sort_by(|a, b| a.procedure_id.cmp(&b.procedure_id));
    let mut spent = 0u64;
    let mut delivered = Vec::new();
    let mut candidates = Vec::new();
    for e in &sorted {
        candidates.push(e.procedure_id.clone());
        if spent + e.estimate.tokens > budget_share_tokens {
            continue;
        }
        spent += e.estimate.tokens;
        delivered.push(procedure_index_candidate(e));
    }
    sink.emit(
        "context.procedure.selected",
        Json::obj([
            ("method", Json::str(SELECT_METHOD)),
            (
                "candidates",
                Json::Arr(candidates.iter().map(|c| Json::str(c.clone())).collect()),
            ),
            (
                "delivered",
                Json::Arr(
                    delivered
                        .iter()
                        .map(|c| Json::str(c.candidate_id.clone()))
                        .collect(),
                ),
            ),
        ]),
    );
    let mut out: Vec<Candidate> = delivered;
    out.sort_by(|a, b| a.candidate_id.cmp(&b.candidate_id));
    out
}

/// The `procedure_index` candidate — `handle_only` (the index is a handle the
/// model expands via [`activate`]); `optional(procedure_body)`-class retention
/// … the index itself rides the `procedures` slot at `memory_index` priority
/// (evictable; the body is the `procedure_body` class).
pub fn procedure_index_candidate(e: &ProcedureIndexEntry) -> Candidate {
    Candidate {
        candidate_id: format!("procedure_index:{}", e.procedure_id),
        context_item_id: Some(e.index_item_id.clone()),
        kind: CandidateKind::ProcedureIndex,
        state: CandidateState::HandleOnly,
        retention: Retention::Optional(PriorityClass::MemoryIndex),
        estimate: e.estimate.clone(),
        source_event: None,
        source_seq: 0,
        label: e.label.clone(),
        provenance: e.provenance.clone(),
        validity: hh_hir::records::Validity::open_from(0),
        readers: None,
        slot_hint: Some("procedures".into()),
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: Some(e.index_item_id.clone()),
        handle: Some(crate::plan::OffloadHandle {
            content_address: e.index_item_id.clone(),
            media_type: "application/x-procedure-index".into(),
            size: e.estimate.tokens,
            label: e.label.clone(),
            excerpt_report: crate::plan::ExcerptReport {
                truncated_by: None,
                total_lines: 0,
                total_bytes: 0,
                retained_range: (0, 0),
            },
            read_capability: format!("read_procedure:{}", e.procedure_id),
        }),
    }
}

/// The `procedure_body` candidate — produced by [`activate`]; `expanded`,
/// `optional(procedure_body)` retention (the kernel eviction order's
/// `procedure_body` class), `procedures` slot.
pub fn procedure_body_candidate(
    e: &ProcedureIndexEntry,
    body_estimate: Estimate,
    activation_seq: u64,
) -> Candidate {
    Candidate {
        candidate_id: format!("procedure_body:{}", e.procedure_id),
        context_item_id: Some(format!("procedure_body:{}", e.procedure_id)),
        kind: CandidateKind::ProcedureBody,
        state: CandidateState::Expanded,
        retention: Retention::Optional(PriorityClass::ProcedureBody),
        estimate: body_estimate,
        source_event: None,
        source_seq: activation_seq,
        label: e.label.clone(),
        provenance: e.provenance.clone(),
        validity: hh_hir::records::Validity::open_from(0),
        readers: None,
        slot_hint: Some("procedures".into()),
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: Some(format!("procedure_body:{}", e.procedure_id)),
        handle: None,
    }
}

/// `activate(plan, delivery_id, sink)` — the deterministic expansion: the
/// model expanded a handle-only `procedure_index` delivery. Emits
/// `context.artefact.activated{detector: deterministic, signal: loaded}` and
/// returns the activated index's `procedure_id` — the caller produces the
/// `procedure_body` candidate for the next assemble (the body never lands in
/// the same call's plan — I-DET).
pub fn activate(plan: &ContextPlan, delivery_id: &str, sink: &mut dyn EventSink) -> Option<String> {
    let item = plan
        .slots
        .iter()
        .flat_map(|s| s.items.iter())
        .find(|i| i.delivery_id == delivery_id && i.delivered_by_reference)?;
    let artefact_id = item.artefact_id.clone()?;
    sink.emit(
        "context.artefact.activated",
        crate::events::artefact_activated_payload(
            &artefact_id,
            delivery_id,
            "deterministic",
            "loaded",
        ),
    );
    artefact_id
        .strip_prefix("procedure_index:")
        .map(str::to_string)
        .or(Some(artefact_id))
}

/// The deterministic `followed` evidence (§5f.1 §2.6's `procedure_invoked`
/// row): the run invoked `capability`, the activated procedure's
/// `allowed_capabilities` contains it, and the index's `delivery_id` is in the
/// call's `causes[]`. Returns the `evidence_ref`/`detector_ref` the caller
/// ledger-records as `verification.artefact.followed{detector: deterministic}`;
/// `None` — no emission — when the chain is broken.
pub fn detect_followed(
    index_delivery_id: &str,
    invoked_capability: &str,
    allowed_capabilities: &BTreeSet<String>,
    causes: &[String],
) -> Option<String> {
    if !allowed_capabilities.contains(invoked_capability) {
        return None;
    }
    if !causes.iter().any(|c| c == index_delivery_id) {
        return None;
    }
    Some(format!(
        "procedure_invoked:{index_delivery_id}:{invoked_capability}"
    ))
}
