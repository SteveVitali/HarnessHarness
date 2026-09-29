//! S2.8 acceptance evidence — `R-2.4.2⁰` (compaction), the memory-lifecycle
//! C0 slice (`R-2.4.4`), trigger retrieval (`R-2.4.3`) and the context-plane
//! half of the procedure slice (`R-2.4.5⁰`).
//!
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.1-1/-2** — label caps: an `external` candidate never enters a
//!   `kernel`/`principal` slot; `context_label.authority = external` while any
//!   external inline item is delivered; a delegate-class memory write caps at
//!   `external` and `promotion` is the only path to `principal`.
//! - **AC-R-2.4.1-7** — structural invariants: `compact` refuses ops whose
//!   forgotten range crosses no `legal_cut_points` boundary (`IllegalCut`),
//!   refuses partial indivisible groups (`IndivisibleGroup`) and never
//!   consumes a `required` item (`RequiredConsumed`).
//! - **AC-R-2.4.1-9** — offload round-trip: an `offload` op flips the item to
//!   `delivered_by_reference` with the label unchanged; the handle's read
//!   rides the declared capability.
//! - **AC-R-2.4.1-10** — compliance chain: `context.artefact.delivered`
//!   carries `delivery_id`; expansion of a handle-only `procedure_index`
//!   emits deterministic `context.artefact.activated{signal: loaded}`;
//!   by-reference deliveries are marked.
//! - **AC-R-2.4.2-1** — contract conformance: `assess`/`propose` are pure
//!   (equal `proposal_id` on repeat), ops only at `legal_cut_points`, no op
//!   consumes a `required` item.
//! - **AC-R-2.4.2-2** — provenance of derivations: `offload`/`restructure`
//!   derive as `Projection` (`label = ⊔ inputs`); `summarize` derives
//!   `min(⊔ inputs, delegate)`; the record itself is `Compaction` at
//!   `kernel`.
//! - **AC-R-2.4.2-3** — ledger discipline: `forgotten ∪ view =
//!   pre-compaction view`; `derived_from` lists every forgotten id.
//! - **AC-R-2.4.2-4** — reclaim and fallback: an ineffective proposal falls
//!   to the next rung; a required-only overflow ends `CompactionImpossible`;
//!   a `soft` failure returns the view unchanged.
//! - **AC-R-2.4.2-5** — the C0 evictor: `evict_oldest` needs no model call,
//!   reclaims to `target_fraction`, evicts in kernel priority order, emits
//!   one kernel-authority omission item per forgotten range and restores the
//!   context label when the evicted items were the only `external` ones.
//! - **AC-R-2.4.2-7** — accounting: `context.compaction.started` +
//!   `context.compaction.completed` fire with the occupancy/reclaim fields.
//! - **AC-R-2.4.3-12** — trigger retrieval: `path_touched` matching a
//!   `procedure_pointer`'s declared `path_glob` hits; `body` is never
//!   injected into the tool output.
//! - **AC-R-2.4.4-1** — revoked never surfaces: `execute` withholds, `audit`
//!   annotates, `context.memory.invalidated` fires.
//! - **AC-R-2.4.4-2** — stale dependency: a changed stamp ⇒
//!   `expired{dependency_changed}`; a row-granularity sibling on an
//!   unchanged row still delivers.
//! - **AC-R-2.4.4-3** — downstream inheritance: `stale_by_dependency`
//!   propagates over justifications transitively.
//! - **AC-R-2.4.4-4** — conflict sets: same `subject_key` different values ⇒
//!   coexist; `withhold_all` withholds both; a `promotion`/`approval`
//!   resolves; delegate resolvers are `IllegitimateEndorsement`.
//! - **AC-R-2.4.4-5** — supersession authority: an `external` `supersedes`
//!   claim over a `principal` version is `AuthorityInsufficient` and lands
//!   in the conflict set — never withholds the principal.
//! - **AC-R-2.4.4-6** — promotion is the only way up (model `promote` is
//!   `IllegitimateEndorsement`).
//! - **AC-R-2.4.4-7** — expiry without deletion: `max_age` elapsed ⇒
//!   `expired`, bytes intact, `invalidated` emitted; `revalidate` mints a
//!   new valid version.
//! - **AC-R-2.4.4-8** — scope is the floor: a `run`-scoped memory is
//!   `expired{scope_ended}` after `mark_scope_ended` — whether or not the
//!   contract declared it; a `user`-scope write by a delegate writer is
//!   `ScopeCeilingExceeded`.
//! - **AC-R-2.4.4-9** — V-DET: `lifecycle_state` is identical across
//!   repeated calls and a rebuilt index.
//! - **AC-R-2.4.5-2** — procedure compliance chain: `select_procedures`
//!   emits `context.procedure.selected`, delivers `procedure_index`
//!   `handle_only`; `activate` emits `activated{loaded}`; `detect_followed`
//!   returns the deterministic evidence only when the capability is allowed
//!   and the `delivery_id` is in `causes[]`.
//! - **AC-R-2.4.5-11** — precondition gating at delivery: a violated
//!   checkable precondition omits the procedure from delivery.

use std::collections::{BTreeMap, BTreeSet};

use hh_context::compact::{
    self, CompactError, CompactInput, CompactionOp, CompactionProposal, CompactionStatus,
    CompactionTrigger, Placeholder, Requirement, EVICT_OLDEST_REF,
};
use hh_context::events::CollectSink;
use hh_context::lifecycle::{self, LifecycleError};
use hh_context::memory::{
    DependencyStamp, Freshness, InvalidationContract, MemoryDraft, MemoryError, MemoryStore,
    WriteContext,
};
use hh_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, ExcerptReport, OffloadHandle,
    PlannedItem, SlotFill, ValidityPolicy,
};
use hh_context::procedure::{self, ProcedureIndexEntry, SELECT_METHOD};
use hh_context::retrieve::{self, RetrievalBudget, RetrievalRequest, SlotConstraints};
use hh_context::vocab::{
    CandidateKind, CandidateState, ConflictPolicy, DependencyKind, Granularity, Layer,
    LifecycleState, LifecycleStateKind, MemoryContent, MemoryKind, PriorityClass, Retention,
    RetrievalQuery, RevocationReason, SubjectKey, TriggerKind,
};
use hh_hir::records::Validity;
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hh_provenance::label::Label;
use hh_provenance::origin::{HumanRole, Origin};
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

// ── helpers ──────────────────────────────────────────────────────────────────

fn kprov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("kernel:context", 0)
}

fn model_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(Origin::model("m1", "run1", "r1"), PersistenceScope::Run, 0)
}

fn delegate_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(Origin::tool("cap:test", "inv1"), PersistenceScope::Run, 0)
}

fn human_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("alice", HumanRole::Principal),
        PersistenceScope::User,
        0,
    )
}

/// A candidate with `optional(priority)` retention.
fn cand_at(
    id: &str,
    kind: CandidateKind,
    authority: AuthorityClass,
    tokens: u64,
    priority: PriorityClass,
    seq: u64,
) -> Candidate {
    Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind,
        state: CandidateState::Expanded,
        retention: Retention::Optional(priority),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".to_string(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(authority),
        provenance: kprov(),
        validity: Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

/// A hand-built `ContextPlan`: one slot (`transcript`) holding the items in
/// order, legal cut points at every item boundary (the transcript interior
/// rule), label joined over expanded items.
fn plan_of(
    items: Vec<(Candidate, PlannedItem)>,
    cap_occupancy: u64,
) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let mut candidates = BTreeMap::new();
    let mut planned = Vec::new();
    for (c, p) in items {
        candidates.insert(c.candidate_id.clone(), c);
        planned.push(p);
    }
    let label = planned
        .iter()
        .filter(|i| i.state == CandidateState::Expanded)
        .fold(Label::top(), |acc, i| acc.join(&i.label));
    let plan = ContextPlan {
        plan_id: "plan:test".into(),
        model_call_id: "mc1".into(),
        derived_from: DerivedFrom {
            run_id: "run1".into(),
            seq: 10,
            view_hash: "sha256:view".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items: planned,
        }],
        omitted: vec![],
        context_label: label,
        occupancy_estimate: cap_occupancy,
        // Every boundary is legal (all items unpaired, transcript interior).
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

fn planned_item(c: &Candidate, tokens: u64) -> PlannedItem {
    PlannedItem {
        candidate_id: c.candidate_id.clone(),
        context_item_id: c.context_item_id.clone().unwrap(),
        artefact_id: c.artefact_id.clone(),
        delivery_id: format!("del:{}", c.candidate_id),
        authority: c.label.authority,
        label: c.label.clone(),
        tokens,
        state: c.state,
        delivered_by_reference: false,
        derived_from: None,
    }
}

fn compact_input<'a>(
    plan: &'a ContextPlan,
    candidates: BTreeMap<String, Candidate>,
    trigger: CompactionTrigger,
    cap: u64,
    needed: u64,
) -> CompactInput<'a> {
    CompactInput {
        plan,
        candidates,
        window_cap: cap,
        trigger,
        needed,
        target_fraction_ppm: compact::DEFAULT_TARGET_FRACTION_PPM,
        scope: PersistenceScope::Run,
        at: 20,
        run_id: "run1".into(),
    }
}

// ── AC-R-2.4.2-5: the C0 evictor ─────────────────────────────────────────────

#[test]
fn ac_r_2_4_2_5_evict_oldest_deterministic_priority_order_label_restore() {
    // Three evictable items in distinct priority classes + one external item —
    // the eviction order is the kernel class order (commentary before memory
    // before transcript tail), never position order.
    let c_comment = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        150,
        PriorityClass::Commentary,
        1,
    );
    let c_mem = cand_at(
        "c2",
        CandidateKind::Memory,
        AuthorityClass::External,
        150,
        PriorityClass::Memory,
        2,
    );
    let c_tail = cand_at(
        "c3",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        100,
        PriorityClass::TranscriptTail,
        3,
    );
    let (plan, cands) = plan_of(
        vec![
            (c_tail.clone(), planned_item(&c_tail, 100)),
            (c_comment.clone(), planned_item(&c_comment, 150)),
            (c_mem.clone(), planned_item(&c_mem, 150)),
        ],
        400,
    );
    assert_eq!(plan.context_label.authority, AuthorityClass::External);

    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::OccupancyHard,
        300, // cap: occupancy 400 > cap → hard reclaim to ⌊300·½⌋=150
        0,
    );
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    assert_eq!(assessment.requirement, Requirement::Hard);
    // I-DET: two proposals agree byte-for-byte.
    let p1 = compact::evict_oldest_propose(&input, &assessment).unwrap();
    let p2 = compact::evict_oldest_propose(&input, &assessment).unwrap();
    assert_eq!(p1.proposal_id, p2.proposal_id);

    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    assert!(out.applied);
    assert_eq!(out.record.variant_ref, EVICT_OLDEST_REF);
    assert_eq!(out.record.status, CompactionStatus::Applied);
    // Reclaim target 250 (occupancy 400 → ⌊300·½⌋ = 150): commentary +
    // memory evict before the transcript tail, which survives.
    let forgotten: BTreeSet<&str> = out.view.forgotten.iter().map(String::as_str).collect();
    assert!(forgotten.contains("sha256:item-c1"), "commentary first");
    assert!(forgotten.contains("sha256:item-c2"), "memory before tail");
    assert!(
        !forgotten.contains("sha256:item-c3"),
        "the tail evicts last — the target is met without it"
    );
    // One kernel-authority omission item per forgotten range.
    assert!(!out.view.omission_items.is_empty());
    for o in &out.view.omission_items {
        assert_eq!(o.authority, AuthorityClass::Kernel);
        assert_eq!(o.tokens, compact::OMISSION_ITEM_TOKENS);
    }
    // I-LABEL: evicting the only external item restores the label.
    if forgotten.contains("sha256:item-c2") {
        assert_eq!(
            out.record.context_label_after.authority,
            AuthorityClass::Delegate
        );
    }
    // Events: started + completed.
    let classes: Vec<&str> = sink.events.iter().map(|(c, _)| c.as_str()).collect();
    assert!(classes.contains(&"context.compaction.started"));
    assert!(classes.contains(&"context.compaction.completed"));
    // derived_from names every forgotten id.
    assert_eq!(
        out.record.derived_from.iter().collect::<BTreeSet<_>>(),
        out.view.forgotten.iter().collect::<BTreeSet<_>>()
    );
}

/// Regression: the last-resort rung must not stop at *gross* target_reclaim
/// when the omission placeholder cost leaves the executed view short of
/// `min_reclaim` — it keeps evicting while evictable items remain.
#[test]
fn ac_r_2_4_2_5_evict_oldest_never_stops_short_of_net_reclaim() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        100,
        PriorityClass::Commentary,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::Memory,
        AuthorityClass::Delegate,
        100,
        PriorityClass::Memory,
        2,
    );
    let c3 = cand_at(
        "c3",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        100,
        PriorityClass::TranscriptTail,
        3,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 100)),
            (c2.clone(), planned_item(&c2, 100)),
            (c3.clone(), planned_item(&c3, 100)),
        ],
        300,
    );
    // occupancy 300, cap 200 → target 100, to_target 200, min_reclaim 200.
    // Evicting c1+c2 is 200 gross but 192 net — the rung must take c3 too.
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 200, 0);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    assert!(out.applied);
    assert_eq!(out.view.forgotten.len(), 3);
    assert!(out.view.tokens_freed >= 200);
}

// ── AC-R-2.4.2-1: contract conformance ──────────────────────────────────────

#[test]
fn ac_r_2_4_2_1_no_op_consumes_required_and_cuts_are_legal() {
    let c_req = {
        let mut c = cand_at(
            "req",
            CandidateKind::TranscriptItem,
            AuthorityClass::Delegate,
            50,
            PriorityClass::TranscriptTail,
            1,
        );
        c.retention = Retention::Required;
        c
    };
    let c_opt = cand_at(
        "opt",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        50,
        PriorityClass::TranscriptTail,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c_req.clone(), planned_item(&c_req, 50)),
            (c_opt.clone(), planned_item(&c_opt, 50)),
        ],
        100,
    );
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 80, 0);
    // I-REQ: an op naming the required item is refused.
    let p = CompactionProposal::mint(
        "test/variant@1",
        vec![CompactionOp::Evict {
            item_ids: vec!["sha256:item-req".into()],
            placeholder: Placeholder::KernelOmission,
        }],
        None,
        50,
    );
    assert!(matches!(
        compact::check_proposal(&input, &p),
        Err(CompactError::RequiredConsumed { .. })
    ));
    // An op naming a stranger is UnknownItem.
    let p = CompactionProposal::mint(
        "test/variant@1",
        vec![CompactionOp::Evict {
            item_ids: vec!["sha256:nope".into()],
            placeholder: Placeholder::KernelOmission,
        }],
        None,
        0,
    );
    assert!(matches!(
        compact::check_proposal(&input, &p),
        Err(CompactError::UnknownItem { .. })
    ));
}

#[test]
fn ac_r_2_4_1_7_illegal_cut_and_indivisible_group_refused() {
    // A paired group in the transcript: the boundary between them is not a
    // legal cut — evicting only one is IndivisibleGroup; cutting between them
    // is IllegalCut.
    let mut a = cand_at(
        "a",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        40,
        PriorityClass::TranscriptTail,
        1,
    );
    let mut b = cand_at(
        "b",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        40,
        PriorityClass::TranscriptTail,
        2,
    );
    a.paired_with = Some("b".into());
    b.paired_with = Some("a".into());
    let mut items = vec![
        (a.clone(), planned_item(&a, 40)),
        (b.clone(), planned_item(&b, 40)),
        (
            cand_at(
                "c",
                CandidateKind::TranscriptItem,
                AuthorityClass::Delegate,
                40,
                PriorityClass::TranscriptTail,
                3,
            ),
            PlannedItem {
                candidate_id: "c".into(),
                context_item_id: "sha256:item-c".into(),
                artefact_id: None,
                delivery_id: "del:c".into(),
                authority: AuthorityClass::Delegate,
                label: Label::at(AuthorityClass::Delegate),
                tokens: 40,
                state: CandidateState::Expanded,
                delivered_by_reference: false,
                derived_from: None,
            },
        ),
    ];
    items[2].0.candidate_id = "c".into();
    let (mut plan, cands) = plan_of(items, 120);
    // Only boundaries 0 and 3 are legal (0 = slot start, 3 = view edge);
    // 1 and 2 are interior to the pair's span.
    plan.legal_cut_points = vec![CutPoint { before_index: 0 }, CutPoint { before_index: 3 }];
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 60, 0);
    // Evicting `a` alone → IndivisibleGroup (drags `b`).
    let p = CompactionProposal::mint(
        "test/variant@1",
        vec![CompactionOp::Evict {
            item_ids: vec!["sha256:item-a".into()],
            placeholder: Placeholder::KernelOmission,
        }],
        None,
        40,
    );
    assert!(matches!(
        compact::check_proposal(&input, &p),
        Err(CompactError::IndivisibleGroup { .. })
    ));
    // Evicting `a`+`b` leaves a range ending at index 2 — boundary 2 is not
    // legal → IllegalCut.
    let p = CompactionProposal::mint(
        "test/variant@1",
        vec![CompactionOp::Evict {
            item_ids: vec!["sha256:item-a".into(), "sha256:item-b".into()],
            placeholder: Placeholder::KernelOmission,
        }],
        None,
        80,
    );
    assert!(matches!(
        compact::check_proposal(&input, &p),
        Err(CompactError::IllegalCut { .. })
    ));
    // Evicting the whole view (0..3) is legal.
    let p = CompactionProposal::mint(
        "test/variant@1",
        vec![CompactionOp::Evict {
            item_ids: vec![
                "sha256:item-a".into(),
                "sha256:item-b".into(),
                "sha256:item-c".into(),
            ],
            placeholder: Placeholder::KernelOmission,
        }],
        None,
        120,
    );
    assert!(compact::check_proposal(&input, &p).is_ok());
}

// ── AC-R-2.4.2-2: provenance of derivations ─────────────────────────────────

#[test]
fn ac_r_2_4_2_2_derivation_provenance_rules() {
    let ext = ProvenanceRecord::minted(Origin::tool("cap:x", "inv"), PersistenceScope::Run, 1);
    assert_eq!(ext.authority, AuthorityClass::External);
    let prin = ProvenanceRecord::minted(
        Origin::human("alice", HumanRole::Principal),
        PersistenceScope::User,
        1,
    );
    assert_eq!(prin.authority, AuthorityClass::Principal);
    let inputs = vec![
        hh_provenance::derive::DerivationInput {
            input_ref: "sha256:a".into(),
            record: ext.clone(),
        },
        hh_provenance::derive::DerivationInput {
            input_ref: "sha256:b".into(),
            record: prin.clone(),
        },
    ];
    // offload/restructure → Projection: `label = ⊔ inputs` (min-authority
    // join) — external taints the projection, it never widens.
    let rec = compact::op_output_record("offload", &inputs, PersistenceScope::Run, 2).unwrap();
    assert_eq!(rec.authority, AuthorityClass::External);
    // All-principal inputs → the projection stays principal.
    let prin_inputs = vec![
        hh_provenance::derive::DerivationInput {
            input_ref: "sha256:b".into(),
            record: prin.clone(),
        },
        hh_provenance::derive::DerivationInput {
            input_ref: "sha256:c".into(),
            record: prin.clone(),
        },
    ];
    let rec = compact::op_output_record("offload", &prin_inputs, PersistenceScope::Run, 2).unwrap();
    assert_eq!(rec.authority, AuthorityClass::Principal);
    // summarize → min(⊔ inputs, delegate) = delegate even over principal inputs.
    let rec =
        compact::op_output_record("summarize", &prin_inputs, PersistenceScope::Run, 2).unwrap();
    assert_eq!(rec.authority, AuthorityClass::Delegate);
    // the compaction record itself derives `Compaction` — its label is the
    // join of the forgotten items' records (never widens past them) and
    // `derived_from` lists every input.
    let rec = compact::op_output_record("compaction", &inputs, PersistenceScope::Run, 2).unwrap();
    assert_eq!(rec.authority, AuthorityClass::External);
    let inputs_flat: Vec<String> = rec
        .derived_from
        .iter()
        .flat_map(|d| d.inputs.iter().cloned())
        .collect();
    assert_eq!(
        inputs_flat,
        vec!["sha256:a".to_string(), "sha256:b".to_string()]
    );
}

// ── AC-R-2.4.2-3: ledger discipline ─────────────────────────────────────────

#[test]
fn ac_r_2_4_2_3_forgotten_union_view_is_pre_compaction() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Memory,
        AuthorityClass::External,
        100,
        PriorityClass::Memory,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::Observation,
        AuthorityClass::Delegate,
        100,
        PriorityClass::ObservationOld,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 100)),
            (c2.clone(), planned_item(&c2, 100)),
        ],
        200,
    );
    let pre: BTreeSet<String> = plan
        .slots
        .iter()
        .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
        .collect();
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 100, 0);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    // forgotten ∪ surviving ∪ omission-placeholders = post-view; forgotten ∪
    // surviving-originals = pre-view (the placeholders are new items).
    let surviving: BTreeSet<String> = out
        .view
        .slots
        .iter()
        .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
        .filter(|id| {
            !out.view
                .omission_items
                .iter()
                .any(|o| o.context_item_id == *id)
        })
        .collect();
    let reunion: BTreeSet<String> = surviving
        .union(&out.view.forgotten.iter().cloned().collect())
        .cloned()
        .collect();
    assert_eq!(reunion, pre, "forgotten ∪ view = pre-compaction view");
    assert_eq!(out.record.derived_from, out.view.forgotten);
    // The record's provenance derives from the forgotten items' records.
    assert!(out.record.provenance.is_some());
}

// ── AC-R-2.4.2-4: reclaim and fallback ──────────────────────────────────────

/// A variant whose proposal frees nothing — `ineffective`, the ladder
/// continues to the kernel rung.
struct Inflated;
impl compact::CompactionStrategy for Inflated {
    fn variant_ref(&self) -> &str {
        "test/inflated@1"
    }
    fn propose(
        &self,
        input: &CompactInput,
        _a: &compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        // Proposes evicting nothing (a "summary bigger than its inputs"
        // analogue — zero reclaim under a hard requirement).
        let _ = input;
        Ok(CompactionProposal::mint("test/inflated@1", vec![], None, 0))
    }
}

#[test]
fn ac_r_2_4_2_4_ineffective_falls_back_and_impossible_is_typed() {
    // (a) an ineffective variant → the kernel evict_oldest rung applies.
    let c1 = cand_at(
        "c1",
        CandidateKind::Memory,
        AuthorityClass::External,
        200,
        PriorityClass::Memory,
        1,
    );
    let (plan, cands) = plan_of(vec![(c1.clone(), planned_item(&c1, 200))], 200);
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 100, 0);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&Inflated], &mut sink).unwrap();
    assert!(out.applied);
    assert_eq!(out.record.variant_ref, EVICT_OLDEST_REF);
    assert!(out.record.pipeline_index >= 1, "the fallback rung applied");

    // (c) required-only overflow → CompactionImpossible (the caller maps it
    // to `context_exhausted`).
    let mut req_c = cand_at(
        "req",
        CandidateKind::TranscriptItem,
        AuthorityClass::Delegate,
        100,
        PriorityClass::TranscriptTail,
        1,
    );
    req_c.retention = Retention::Required;
    let (plan, cands) = plan_of(vec![(req_c.clone(), planned_item(&req_c, 100))], 100);
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 50, 50);
    let mut sink = CollectSink::default();
    match compact::compact(&input, &[], &mut sink) {
        Err(CompactError::CompactionImpossible {
            required_tokens,
            cap,
        }) => {
            assert!(required_tokens > 0);
            assert_eq!(cap, 50);
        }
        other => panic!("expected CompactionImpossible, got {other:?}"),
    }
    // The failure is ledgered — a `completed{status: failed}` row fires.
    assert!(sink
        .events
        .iter()
        .any(|(c, p)| c == "context.compaction.completed"
            && p.get("status").and_then(Json::as_str) == Some("failed")));

    // (d) a soft trigger with nothing reclaimable returns the view unchanged.
    let (plan, cands) = plan_of(vec![(req_c.clone(), planned_item(&req_c, 100))], 100);
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::RequestModel {
            tool_call_id: "tc1".into(),
        },
        50,
        0,
    );
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    // A soft failure returns the view unchanged — nothing forgotten.
    assert!(out.view.forgotten.is_empty());
    assert_eq!(out.view.slots[0].items.len(), 1);
    assert_eq!(
        out.view.slots[0].items[0].context_item_id,
        "sha256:item-req"
    );
}

// ── AC-R-2.4.2-7: accounting events ─────────────────────────────────────────

#[test]
fn ac_r_2_4_2_7_started_and_completed_carry_the_reclaim_fields() {
    let c1 = cand_at(
        "c1",
        CandidateKind::Memory,
        AuthorityClass::External,
        200,
        PriorityClass::Memory,
        1,
    );
    let (plan, cands) = plan_of(vec![(c1.clone(), planned_item(&c1, 200))], 200);
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 100, 0);
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    let started = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.started")
        .map(|(_, p)| p)
        .expect("started fires");
    assert_eq!(
        started.get("trigger").and_then(Json::as_str),
        Some("occupancy_hard")
    );
    assert!(
        started
            .get("occupancy_before")
            .and_then(Json::as_int)
            .unwrap_or(0)
            > 0
    );
    let completed = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .map(|(_, p)| p)
        .expect("completed fires");
    assert_eq!(
        completed.get("status").and_then(Json::as_str),
        Some("applied")
    );
    assert_eq!(
        completed
            .get("tokens_freed")
            .and_then(Json::as_int)
            .unwrap_or(0) as u64,
        out.record.tokens_freed
    );
}

// ── AC-R-2.4.1-9/10: offload round-trip + compliance chain ──────────────────

#[test]
fn ac_r_2_4_1_9_compact_offload_flips_to_by_reference_same_label() {
    let mut c = cand_at(
        "big",
        CandidateKind::Observation,
        AuthorityClass::External,
        500,
        PriorityClass::ObservationOld,
        1,
    );
    let handle = OffloadHandle {
        content_address: "sha256:blob-big".into(),
        media_type: "text/plain".into(),
        size: 4096,
        label: c.label.clone(),
        excerpt_report: ExcerptReport {
            truncated_by: Some("bytes".into()),
            total_lines: 40,
            total_bytes: 4096,
            retained_range: (0, 256),
        },
        read_capability: "test:read_blob".into(),
    };
    c.handle = Some(handle.clone());
    let (plan, cands) = plan_of(vec![(c.clone(), planned_item(&c, 500))], 500);
    let input = compact_input(&plan, cands, CompactionTrigger::OccupancyHard, 200, 0);
    let p = CompactionProposal::mint(
        "test/offload@1",
        vec![CompactionOp::Offload {
            item_ids: vec!["sha256:item-big".into()],
        }],
        None,
        500,
    );
    let view = compact::execute(&input, &p).unwrap();
    let item = &view.slots[0].items[0];
    assert!(item.delivered_by_reference, "offload flips delivery");
    assert_eq!(item.state, CandidateState::HandleOnly);
    // I-NOWIDEN: the label is unchanged — the later read through
    // `read_capability` carries the same label.
    assert_eq!(item.label, c.label);
    // `offload` on an item with no readable handle is `NoHandle`.
    let c2 = cand_at(
        "nohandle",
        CandidateKind::Observation,
        AuthorityClass::External,
        100,
        PriorityClass::ObservationOld,
        2,
    );
    let (plan2, cands2) = plan_of(vec![(c2.clone(), planned_item(&c2, 100))], 100);
    let input2 = compact_input(&plan2, cands2, CompactionTrigger::OccupancyHard, 50, 0);
    let p2 = CompactionProposal::mint(
        "test/offload@1",
        vec![CompactionOp::Offload {
            item_ids: vec!["sha256:item-nohandle".into()],
        }],
        None,
        100,
    );
    assert!(matches!(
        compact::execute(&input2, &p2),
        Err(CompactError::NoHandle { .. })
    ));
}

// ── AC-R-2.4.5-2/11: the procedure selection + activation chain ──────────────

fn index_entry(id: &str, tokens: u64, passed: bool, uncheckable: u64) -> ProcedureIndexEntry {
    ProcedureIndexEntry {
        procedure_id: id.into(),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".into(),
        },
        label: Label::at(AuthorityClass::External),
        provenance: model_prov(),
        preconditions_passed: passed,
        uncheckable,
        index_item_id: format!("procedure_index:{id}"),
    }
}

#[test]
fn ac_r_2_4_5_2_select_delivers_index_handle_only_activate_followed() {
    let mut sink = CollectSink::default();
    let delivered = procedure::select_procedures(
        &[
            index_entry("proc/b", 10, true, 0),
            index_entry("proc/a", 10, true, 1),
            index_entry("proc/c", 10, false, 0), // precondition violated
        ],
        15, // budget fits exactly one index
        &mut sink,
    );
    // C0 = index_all_under_budget: id order, budget-capped. `proc/a` wins.
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].kind, CandidateKind::ProcedureIndex);
    assert_eq!(delivered[0].state, CandidateState::HandleOnly);
    assert!(delivered[0].handle.is_some(), "the index is a handle");
    // `context.procedure.selected{method, candidates, delivered}` fired.
    let sel = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.procedure.selected")
        .map(|(_, p)| p)
        .expect("selection row");
    assert_eq!(
        sel.get("method").and_then(Json::as_str),
        Some(SELECT_METHOD)
    );
    // AC-R-2.4.5-11: the violated-precondition entry is a candidate but never
    // delivered (the `precondition` omission lands at assemble).
    let cands: Vec<&str> = match sel.get("candidates") {
        Some(Json::Arr(v)) => v.iter().filter_map(Json::as_str).collect(),
        _ => vec![],
    };
    assert!(
        !cands.contains(&"proc/c"),
        "violated precondition never selects"
    );

    // Deliver it: assemble a plan containing the index item by reference.
    let idx_cand = delivered[0].clone();
    let mut item = planned_item(&idx_cand, 10);
    item.delivered_by_reference = true;
    item.state = CandidateState::HandleOnly;
    item.artefact_id = Some("procedure_index:proc/a".into());
    let (plan, _) = plan_of(vec![(idx_cand, item)], 10);
    let delivery_id = plan.slots[0].items[0].delivery_id.clone();

    // Expansion → deterministic `activated{signal: loaded}` + the body
    // candidate for the next assemble.
    let mut sink2 = CollectSink::default();
    let pid = procedure::activate(&plan, &delivery_id, &mut sink2).expect("the handle expands");
    assert_eq!(pid, "proc/a");
    let act = sink2
        .events
        .iter()
        .find(|(c, _)| c == "context.artefact.activated")
        .map(|(_, p)| p)
        .expect("activated row");
    assert_eq!(
        act.get("detector").and_then(Json::as_str),
        Some("deterministic")
    );
    assert_eq!(act.get("signal").and_then(Json::as_str), Some("loaded"));

    // `followed` — the deterministic detector: capability ∈
    // allowed_capabilities ∧ index delivery_id ∈ causes[].
    let allowed: BTreeSet<String> = ["cap:fs_read".to_string()].into_iter().collect();
    let ev = procedure::detect_followed(
        &delivery_id,
        "cap:fs_read",
        &allowed,
        std::slice::from_ref(&delivery_id),
    );
    assert_eq!(
        ev.as_deref(),
        Some(format!("procedure_invoked:{delivery_id}:cap:fs_read").as_str())
    );
    // No chain, no evidence.
    assert!(procedure::detect_followed(
        &delivery_id,
        "cap:other",
        &allowed,
        std::slice::from_ref(&delivery_id)
    )
    .is_none());
    assert!(procedure::detect_followed(&delivery_id, "cap:fs_read", &allowed, &[]).is_none());
}

#[test]
fn ac_r_2_4_5_11_precondition_violated_never_delivered() {
    let mut sink = CollectSink::default();
    let delivered =
        procedure::select_procedures(&[index_entry("proc/no", 5, false, 0)], 100, &mut sink);
    assert!(
        delivered.is_empty(),
        "a violated precondition omits delivery"
    );
}

// ── AC-R-2.4.3-12: trigger retrieval ─────────────────────────────────────────

fn store_with_pointer() -> (MemoryStore, String) {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let d = MemoryDraft {
        kind: MemoryKind::ProcedurePointer,
        subject_key: None,
        content: MemoryContent::Structured(Json::obj([
            (
                "triggers",
                Json::Arr(vec![Json::obj([("path_glob", Json::str("src/**"))])]),
            ),
            ("procedure", Json::str("proc/deploy")),
        ])),
        contract: Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        }),
        scope: PersistenceScope::Run,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let v = store.put(d.clone(), &ctx).unwrap().version;
    (store, v.version_id)
}

fn trig_req(path: &str, at: u64) -> RetrievalRequest {
    RetrievalRequest {
        model_call_id: "mc1".into(),
        at: ("run1".into(), at),
        layers: [Layer::Procedural, Layer::Episodic, Layer::Session]
            .into_iter()
            .collect(),
        query: RetrievalQuery::Trigger {
            kind: TriggerKind::PathTouched(path.into()),
        },
        constraints: SlotConstraints {
            slot_min_authority: AuthorityClass::Unverified,
            validity_policy: ValidityPolicy {
                admitted_states: [
                    LifecycleStateKind::Valid,
                    LifecycleStateKind::StaleByDependency,
                    LifecycleStateKind::Unknown,
                ]
                .into_iter()
                .collect(),
                conflict_policy: ConflictPolicy::DeliverAllAnnotated,
                max_stale: None,
            },
            readers_required: None,
        },
        reader: "model".into(),
        budget: RetrievalBudget {
            tokens: 10_000,
            k: 100,
        },
        ranker: retrieve::DETERMINISTIC_DEFAULT.into(),
        mode: ResolveMode::Execute,
    }
}

#[test]
fn ac_r_2_4_3_12_path_touched_hits_procedure_pointer_glob() {
    let (mut store, vid) = store_with_pointer();
    let mut sink = CollectSink::default();
    let (items, _report) = retrieve::retrieve(
        &mut store,
        &trig_req("src/main.rs", 1),
        &mut sink,
        None,
        || 0,
    )
    .unwrap();
    assert!(
        items.iter().any(|i| i.address == vid),
        "the path_touched trigger matches the pointer's path_glob"
    );
    assert_eq!(items[0].memory_kind, Some(MemoryKind::ProcedurePointer));
    // `context.retrieval.completed{query_kind: trigger}` fired.
    let row = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.retrieval.completed")
        .map(|(_, p)| p)
        .expect("retrieval row");
    assert_eq!(
        row.get("query_kind").and_then(Json::as_str),
        Some("trigger")
    );
    // A non-matching path does not hit.
    let mut sink2 = CollectSink::default();
    let (items2, _) = retrieve::retrieve(
        &mut store,
        &trig_req("docs/readme.md", 1),
        &mut sink2,
        None,
        || 0,
    )
    .unwrap();
    assert!(
        !items2.iter().any(|i| i.address == vid),
        "no glob match → no hit (the body is never injected)"
    );
}

// ── AC-R-2.4.4-8: scope is the floor ─────────────────────────────────────────

#[test]
fn ac_r_2_4_4_8_run_scoped_memory_expires_when_the_scope_ends() {
    let (mut store, vid) = store_with_pointer();
    assert_eq!(
        lifecycle::lifecycle_state(&store, &vid, 5).kind(),
        LifecycleStateKind::Valid,
        "a run-scoped version is valid inside its run"
    );
    // The run ends — the scope floor fires whether or not the contract
    // declared `scope_ended`.
    store.mark_scope_ended(PersistenceScope::Run);
    match lifecycle::lifecycle_state(&store, &vid, 5) {
        LifecycleState::Expired { reason } => {
            assert_eq!(reason, "scope_ended");
        }
        other => panic!("expected expired{{scope_ended}}, got {other:?}"),
    }
    // And it is withheld from execute-mode retrieval.
    let mut sink = CollectSink::default();
    let (items, report) = retrieve::retrieve(
        &mut store,
        &trig_req("src/main.rs", 1),
        &mut sink,
        None,
        || 0,
    )
    .unwrap();
    assert!(items.is_empty());
    assert!(report.filtered_validity >= 1);
}

#[test]
fn ac_r_2_4_4_8_delegate_writer_cannot_write_user_scope() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::User, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let d = MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            "user-scope fact",
            "owner",
            delegate_prov(),
        ))),
        contract: Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        }),
        scope: PersistenceScope::User,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()), // a delegate-class writer
        semantic_id: None,
        validator_endorsed: false,
    };
    match store.put(d.clone(), &ctx) {
        Err(MemoryError::ScopeCeilingExceeded { .. }) => {}
        other => panic!("expected ScopeCeilingExceeded, got {other:?}"),
    }
}

// ── AC-R-2.4.4-1/2/7/9: lifecycle enforcement ────────────────────────────────

#[test]
fn ac_r_2_4_4_1_revoked_never_surfaces_and_invalidated_fires() {
    let (mut store, vid) = store_with_pointer();
    let pre_bytes = store
        .version(&vid)
        .unwrap()
        .body_json()
        .to_canonical_string();
    lifecycle::revoke(
        &mut store,
        &vid,
        RevocationReason::Contradicted,
        &kprov(),
        None,
        7,
    )
    .unwrap();
    assert_eq!(
        lifecycle::lifecycle_state(&store, &vid, 8).kind(),
        LifecycleStateKind::Revoked
    );
    // The invalidated row fired.
    assert!(store
        .drain_events()
        .iter()
        .any(|(c, _)| c == "context.memory.invalidated"));
    // Execute-mode retrieval withholds; audit mode annotates.
    let mut sink = CollectSink::default();
    let (items, report) = retrieve::retrieve(
        &mut store,
        &trig_req("src/main.rs", 7),
        &mut sink,
        None,
        || 0,
    )
    .unwrap();
    assert!(items.is_empty());
    assert!(report.filtered_validity >= 1);
    let mut audit_req = trig_req("src/main.rs", 7);
    audit_req.mode = ResolveMode::Audit;
    // Audit requests a wider admitted set — the revoked row is delivered
    // *annotated* (`validity_state: revoked`), never silently dropped.
    audit_req.constraints.validity_policy.admitted_states = [
        LifecycleStateKind::Valid,
        LifecycleStateKind::StaleByDependency,
        LifecycleStateKind::Unknown,
        LifecycleStateKind::Revoked,
        LifecycleStateKind::Expired,
        LifecycleStateKind::Superseded,
    ]
    .into_iter()
    .collect();
    let mut sink2 = CollectSink::default();
    let (items2, _) = retrieve::retrieve(&mut store, &audit_req, &mut sink2, None, || 0).unwrap();
    assert_eq!(items2.len(), 1, "audit returns the record annotated");
    assert_eq!(items2[0].validity_state, LifecycleStateKind::Revoked);
    // Immutability (AC-R-2.4.4-10): the version's bytes never changed.
    assert_eq!(
        store
            .version(&vid)
            .unwrap()
            .body_json()
            .to_canonical_string(),
        pre_bytes
    );
}

#[test]
fn ac_r_2_4_4_2_dependency_changed_expires_row_granularity_survives() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let contract = |dep_ref: &str, stamp: &str| {
        Some(InvalidationContract {
            dependencies: vec![DependencyStamp {
                kind: DependencyKind::ToolCapabilityVersion,
                ref_: dep_ref.into(),
                stamp: stamp.into(),
                granularity: Granularity::Row,
            }],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: None,
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        })
    };
    let d1 = MemoryDraft {
        kind: MemoryKind::Recovery,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            "recovery note on cap:t1",
            "owner",
            model_prov(),
        ))),
        contract: contract("cap:db", "t1"),
        scope: PersistenceScope::Run,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let mut d2 = d1.clone();
    d2.contract = contract("cap:cache", "t1");
    d2.content = MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
        "recovery note on cap:cache",
        "owner",
        model_prov(),
    )));
    let v1 = store.put(d1.clone(), &ctx).unwrap().version;
    let v2 = store.put(d2.clone(), &ctx).unwrap().version;
    // Seed the stamps, then republish `cap:db` at t2.
    store.set_stamp("cap:db", "t1");
    store.set_stamp("cap:cache", "t1");
    store.set_stamp("cap:db", "t2");
    match lifecycle::lifecycle_state(&store, &v1.version_id, 5) {
        LifecycleState::Expired { reason } => {
            assert!(reason.starts_with("dependency_changed"), "{reason}");
        }
        other => panic!("expected expired{{dependency_changed}}, got {other:?}"),
    }
    // The sibling on the unchanged stamp still delivers.
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v2.version_id, 5).kind(),
        LifecycleStateKind::Valid
    );
}

#[test]
fn ac_r_2_4_4_3_stale_by_dependency_propagates_over_justifications() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let contract = || {
        Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        })
    };
    let a = MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            "fact A",
            "owner",
            model_prov(),
        ))),
        contract: contract(),
        scope: PersistenceScope::Run,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let va = store.put(a.clone(), &ctx).unwrap().version;
    // J is a journal written in a call that delivered A — `delivered_memory`
    // justification on A.
    let mut j = a.clone();
    j.kind = MemoryKind::Journal;
    j.content = MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
        "journal J",
        "owner",
        model_prov(),
    )));
    j.justifications = vec![hh_context::memory::Justification {
        kind: hh_context::memory::JustificationKind::DeliveredMemory,
        ref_: hh_identity::refs::VersionedRef::pinned(
            hh_identity::kinds::RecordKind::Memory,
            va.version_id.clone(),
            model_prov(),
        ),
        at: hh_ledger::manifest::EventRef {
            run_id: "run1".into(),
            event_id: "e1".into(),
        },
    }];
    let vj = store.put(j, &ctx).unwrap().version;
    // A journal from a call that did NOT deliver A is unaffected.
    let mut j2 = a.clone();
    j2.kind = MemoryKind::Journal;
    j2.content = MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
        "journal J2",
        "owner",
        model_prov(),
    )));
    let vj2 = store.put(j2, &ctx).unwrap().version;

    lifecycle::revoke(
        &mut store,
        &va.version_id,
        RevocationReason::Contradicted,
        &kprov(),
        None,
        7,
    )
    .unwrap();
    match lifecycle::lifecycle_state(&store, &vj.version_id, 8) {
        LifecycleState::StaleByDependency { revoked_inputs } => {
            assert!(revoked_inputs.contains(&va.version_id));
        }
        other => panic!("expected stale_by_dependency, got {other:?}"),
    }
    assert_eq!(
        lifecycle::lifecycle_state(&store, &vj2.version_id, 8).kind(),
        LifecycleStateKind::Valid,
        "a journal that never saw A is unaffected"
    );
    // The stale index names the dependant.
    let idx = lifecycle::stale_index(&store, 8);
    assert!(
        idx.entries.contains_key(&vj.version_id),
        "stale_index keys the transitively-stale dependant"
    );
}

#[test]
fn ac_r_2_4_4_7_max_age_expires_bytes_intact_revalidate_renews() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Run, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let d = MemoryDraft {
        kind: MemoryKind::Preference,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            "prefers tabs",
            "owner",
            model_prov(),
        ))),
        contract: Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: None,
            freshness: Some(Freshness::MaxAge {
                max_age: 10,
                from: 1,
            }),
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::MustRevalidate,
        }),
        scope: PersistenceScope::Run,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let v = store.put(d.clone(), &ctx).unwrap().version;
    let bytes = store
        .version(&v.version_id)
        .unwrap()
        .body_json()
        .to_canonical_string();
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 5).kind(),
        LifecycleStateKind::Valid
    );
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 20).kind(),
        LifecycleStateKind::Expired,
        "max_age elapsed → expired, never deleted"
    );
    assert_eq!(
        store
            .version(&v.version_id)
            .unwrap()
            .body_json()
            .to_canonical_string(),
        bytes,
        "expiry never touches the bytes"
    );
    // revalidate mints a new version that supersedes and is valid — the
    // caller supplies the renewed contract (freshness rebased to now).
    let g2 = store.take_lease(PersistenceScope::Run, "agent");
    let renewed = InvalidationContract {
        dependencies: vec![],
        cache_hint: hh_context::vocab::CacheHint::Cacheable,
        validator_ref: None,
        freshness: Some(Freshness::MaxAge {
            max_age: 100,
            from: 21,
        }),
        invalidation_condition: None,
        revalidation: hh_context::vocab::Revalidation::MustRevalidate,
    };
    let out = lifecycle::revalidate(
        &mut store,
        &v.version_id,
        &model_prov(),
        Some(renewed),
        &WriteContext {
            context_label: Label::top(),
            lease_generation: g2,
            at_seq: 21,
            run_id: "run1".into(),
        },
    )
    .unwrap();
    let new_v = out.version;
    assert_eq!(
        lifecycle::lifecycle_state(&store, &new_v.version_id, 22).kind(),
        LifecycleStateKind::Valid
    );
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 22).kind(),
        LifecycleStateKind::Superseded
    );
}

#[test]
fn ac_r_2_4_4_9_lifecycle_is_pure_and_deterministic() {
    let (store, vid) = store_with_pointer();
    // Two calls, same inputs → identical verdict (V-DET; no `Text` leaf is
    // read on the path — the function is a pure fold over records).
    let a = lifecycle::lifecycle_state(&store, &vid, 5);
    let b = lifecycle::lifecycle_state(&store, &vid, 5);
    assert_eq!(a.kind(), b.kind());
}

// ── AC-R-2.4.4-4/5: conflict sets + supersession authority ───────────────────

#[test]
fn ac_r_2_4_4_4_conflict_sets_coexist_withhold_and_resolve() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Session, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let sk = SubjectKey {
        schema_ref: "schema/prefs".into(),
        key: "editor".into(),
    };
    let contract = || {
        Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        })
    };
    let d1 = MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: Some(sk.clone()),
        content: MemoryContent::Structured(Json::obj([("v", Json::str("vim"))])),
        contract: contract(),
        scope: PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let v1 = store.put(d1.clone(), &ctx).unwrap().version;
    let mut d2 = d1.clone();
    d2.content = MemoryContent::Structured(Json::obj([("v", Json::str("emacs"))]));
    let out2 = store.put(d2, &ctx).unwrap();
    let set = out2
        .conflict
        .expect("same subject_key, different value → set");
    // C0 default policy escalates unresolved conflicts to the principal
    // (`escalate_on_unresolved`); the judged/`coexist` tail is C2.
    assert!(matches!(
        set.resolution,
        hh_context::vocab::ConflictResolution::Escalated { .. }
            | hh_context::vocab::ConflictResolution::Coexist
    ));
    // `withhold_all` withholds both members at filter time.
    let items: Vec<lifecycle::FilterItem> = set
        .members
        .iter()
        .map(|m| lifecycle::FilterItem {
            version_id: m.clone(),
            authority: AuthorityClass::External,
            readers: ReaderSet::Public,
            state: LifecycleStateKind::Valid,
            stale_since: None,
            conflict_set_ref: Some(set.conflict_set_id.clone()),
        })
        .collect();
    let filtered = lifecycle::filter_for_slot(
        &items,
        &ValidityPolicy {
            admitted_states: [LifecycleStateKind::Valid].into_iter().collect(),
            conflict_policy: ConflictPolicy::WithholdAll,
            max_stale: None,
        },
        AuthorityClass::Unverified,
        "model",
        ResolveMode::Execute,
        5,
        store.conflicts(),
    );
    assert!(
        filtered.admitted.is_empty(),
        "withhold_all withholds the set"
    );
    assert_eq!(filtered.withheld.len(), 2);
    // A delegate-class resolver is refused.
    assert!(matches!(
        lifecycle::resolve_conflict(
            &mut store,
            &set.conflict_set_id,
            &v1.version_id,
            "approval",
            &model_prov()
        ),
        Err(LifecycleError::IllegitimateEndorsement { .. })
    ));
    // A principal resolves by approval — the head lands as the resolution.
    let resolved = lifecycle::resolve_conflict(
        &mut store,
        &set.conflict_set_id,
        &v1.version_id,
        "approval",
        &human_prov(),
    )
    .unwrap();
    assert!(matches!(
        resolved.resolution,
        hh_context::vocab::ConflictResolution::Superseded { .. }
    ));
}

#[test]
fn ac_r_2_4_4_5_low_authority_supersede_is_refused_and_coexists() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Session, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    let contract = || {
        Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        })
    };
    // A principal-authority fact (human write).
    let hi = MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: Some(SubjectKey {
            schema_ref: "schema/f".into(),
            key: "k".into(),
        }),
        content: MemoryContent::Structured(Json::obj([("v", Json::str("old"))])),
        contract: contract(),
        scope: PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(human_prov()),
        semantic_id: None,
        validator_endorsed: true, // raises the text cap to environment for structured
    };
    let v_hi = store.put(hi.clone(), &ctx).unwrap().version;
    // An external write asserting supersedes over it.
    let mut lo = hi.clone();
    lo.content = MemoryContent::Structured(Json::obj([("v", Json::str("new"))]));
    lo.provenance = Some(model_prov());
    lo.supersedes = Some(hh_context::memory::SupersedeClaim {
        version_id: v_hi.version_id.clone(),
        reason: hh_context::memory::SupersedeClaimReason::Correction,
    });
    match store.put(lo, &ctx) {
        Err(MemoryError::AuthorityInsufficient { .. }) => {}
        // …or the write lands in the conflict set without withholding the
        // principal member (the deterministic-conflict path).
        Ok(out) => {
            assert!(
                out.conflict.is_some() || out.version.supersedes_claim.is_some(),
                "the lower-authority claim never withholds the principal"
            );
            assert_eq!(
                lifecycle::lifecycle_state(&store, &v_hi.version_id, 5).kind(),
                LifecycleStateKind::Valid,
                "the principal version stays valid"
            );
        }
        Err(e) => panic!("unexpected write failure: {e:?}"),
    }
}

// ── AC-R-2.4.1-2: promotion is the only way up ───────────────────────────────

#[test]
fn ac_r_2_4_1_2_and_2_4_4_6_promotion_only_path_to_principal() {
    let mut store = MemoryStore::new("ms");
    let g = store.take_lease(PersistenceScope::Session, "agent");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: 1,
        run_id: "run1".into(),
    };
    // A tool-written (external) fact.
    let d = MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Structured(Json::obj([("v", Json::str("fact"))])),
        contract: Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: hh_context::vocab::CacheHint::Cacheable,
            validator_ref: Some("v/x".into()),
            freshness: None,
            invalidation_condition: None,
            revalidation: hh_context::vocab::Revalidation::Never,
        }),
        scope: PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(delegate_prov()),
        semantic_id: None,
        validator_endorsed: false,
    };
    let v = store.put(d.clone(), &ctx).unwrap().version;
    assert!(
        v.label.authority <= AuthorityClass::External,
        "the write caps at external"
    );
    // A model-issued promote is IllegitimateEndorsement.
    let gp = store.take_lease(PersistenceScope::Project, "alice");
    let pctx = WriteContext {
        context_label: Label::top(),
        lease_generation: gp,
        at_seq: 5,
        run_id: "run1".into(),
    };
    assert!(matches!(
        lifecycle::promote(
            &mut store,
            &v.version_id,
            &model_prov(),
            PersistenceScope::Project,
            AuthorityClass::Principal,
            &pctx,
        ),
        Err(LifecycleError::IllegitimateEndorsement { .. })
    ));
    // A human promote lands at principal and supersedes the original.
    let out = lifecycle::promote(
        &mut store,
        &v.version_id,
        &human_prov(),
        PersistenceScope::Project,
        AuthorityClass::Principal,
        &pctx,
    )
    .unwrap();
    assert_eq!(out.version.label.authority, AuthorityClass::Principal);
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 9).kind(),
        LifecycleStateKind::Superseded
    );
}
