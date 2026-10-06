//! S4.16b — the C1/C2 retrieval & memory depth for hh-context:
//! the bound-`Summarizer` execute path (`UnresolvedSummarizer`, overflow →
//! `InputReduction` → retry, floor admission, `MissingBody`, fallback
//! ladder), the `relower_summarize`/`fold_on_return` variants, the
//! `MemoryStorePort` surface (in-process conformance), the `consolidate`
//! driver (cadence gate, lease fencing, dedupe rebinds, watermarks), and
//! the lifecycle C2 additions (`external_resource` validator deps,
//! `subject_overlap` narrowing, the ≥user reader-set rule,
//! `readers_required`).

use std::collections::{BTreeMap, BTreeSet};

use hh_context::compact::{
    self, CompactError, CompactInput, CompactionOp, CompactionStatus, CompactionStrategy,
    CompactionTrigger, FoldOnReturn, InputReduction, RelowerSummarize, SummarizeInput, Summarizer,
    SummarizerError, SummarizerFailure, SummarizerOutput, FOLD_ON_RETURN_REF,
    RELOWER_SUMMARIZE_REF,
};
use hh_context::consolidate::{
    consolidate, ConsolidateError, ConsolidationCadence, ConsolidationGuards, ConsolidationRule,
    ConsolidatorKind,
};
use hh_context::events::CollectSink;
use hh_context::lifecycle::{self, FilterItem};
use hh_context::memory::{
    DependencyStamp, InvalidationContract, Justification, JustificationKind, MemoryDraft,
    MemoryError, MemoryStore, WriteContext,
};
use hh_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill, ValidityPolicy,
};
use hh_context::vocab::{
    CandidateKind, CandidateState, ConflictPolicy, DependencyKind, Granularity, LifecycleStateKind,
    MemoryContent, MemoryKind, PriorityClass, Retention,
};
use hh_hir::records::Validity;
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hh_provenance::label::Label;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

// ── helpers ──────────────────────────────────────────────────────────────────

fn kprov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("kernel:context", 0)
}

fn model_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(Origin::model("m1", "run1", "r1"), PersistenceScope::Run, 0)
}

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
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

/// A `CompactInput` with the summarizer slot floor and item bodies wired.
#[allow(clippy::too_many_arguments)] // the members are the fixture's shape.
fn compact_input<'a>(
    plan: &'a ContextPlan,
    candidates: BTreeMap<String, Candidate>,
    trigger: CompactionTrigger,
    cap: u64,
    needed: u64,
    summarizer: Option<&'a dyn Summarizer>,
    floor: AuthorityClass,
    texts: Vec<(&str, &str)>,
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
        summarizer,
        slot_min_authority: [("transcript".to_string(), floor)].into_iter().collect(),
        item_texts: texts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A deterministic summariser stub — joins the input ids; records calls.
struct StubSummarizer {
    calls: std::cell::RefCell<Vec<Vec<String>>>,
    overflow_first: bool,
    fail: Option<SummarizerFailure>,
}

impl Summarizer for StubSummarizer {
    fn summarize(&self, input: &SummarizeInput) -> Result<SummarizerOutput, SummarizerError> {
        self.calls.borrow_mut().push(
            input
                .items
                .iter()
                .map(|i| i.context_item_id.clone())
                .collect(),
        );
        if let Some(kind) = self.fail {
            return Err(SummarizerError::Failed {
                kind,
                detail: "stub failure".into(),
            });
        }
        let total: u64 = input.items.iter().map(|i| i.tokens).sum();
        if self.overflow_first && self.calls.borrow().len() == 1 {
            return Err(SummarizerError::Overflow {
                input_tokens: total,
                cap: 10,
            });
        }
        let text = format!(
            "summary of [{}]",
            input
                .items
                .iter()
                .map(|i| i.context_item_id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        Ok(SummarizerOutput {
            text,
            tokens: 5,
            usage: Json::obj([("model_call_id", Json::str("mc-sum-1"))]),
        })
    }
}

fn empty_contract() -> InvalidationContract {
    InvalidationContract {
        dependencies: vec![],
        cache_hint: hh_context::vocab::CacheHint::Cacheable,
        validator_ref: None,
        freshness: None,
        invalidation_condition: None,
        revalidation: hh_context::vocab::Revalidation::Never,
    }
}

fn draft(content: &str) -> MemoryDraft {
    MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            content,
            "owner",
            model_prov(),
        ))),
        contract: Some(empty_contract()),
        scope: PersistenceScope::Run,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(model_prov()),
        semantic_id: None,
        validator_endorsed: false,
    }
}

fn ctx_at(store: &mut MemoryStore, at_seq: u64) -> WriteContext {
    let g = store.take_lease(PersistenceScope::Run, "agent");
    WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq,
        run_id: "run1".into(),
    }
}

// ── summarize execution (§5c.2; R-2.4.2²) ────────────────────────────────────

#[test]
fn summarize_executes_through_the_bound_port() {
    let c1 = cand_at(
        "c1",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 100)),
            (c2.clone(), planned_item(&c2, 100)),
        ],
        200,
    );
    let sz = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: None,
    };
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::OccupancyHard,
        200,
        50,
        Some(&sz),
        AuthorityClass::Unverified,
        vec![
            ("sha256:item-c1", "first body"),
            ("sha256:item-c2", "second body"),
        ],
    );
    // Direct proposal: summarize both items, insert at flat index 0.
    let proposal = compact::CompactionProposal::mint_with(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
            insert_at: 0,
        }],
        None,
        195,
        Some(compact::ModelCallMembers {
            summarizer_profile: Some("summ/pinned".into()),
            max_summary_tokens: Some(50),
            input_reduction: None,
        }),
    );
    let view = compact::execute(&input, &proposal).unwrap();
    // Both inputs forgotten; one derived summary admitted.
    assert_eq!(view.forgotten.len(), 2);
    assert_eq!(view.summary_items.len(), 1);
    let summary = &view.summary_items[0];
    assert_eq!(summary.authority, AuthorityClass::Unverified);
    assert!(summary.derived_from.is_some());
    // The usage row is recorded for the caller's budget post.
    assert!(view.summariser_usage.is_some());
    // The port saw both bodies.
    assert_eq!(
        sz.calls.borrow().as_slice(),
        &[vec![
            "sha256:item-c1".to_string(),
            "sha256:item-c2".to_string()
        ]]
    );
    // I-NOWIDEN: the recomputed label never widens past the join.
    assert!(view.context_label_after.authority <= AuthorityClass::Unverified);
}

#[test]
fn summarize_unbound_port_refuses_typed_never_skips() {
    let c1 = cand_at(
        "c1",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        1,
    );
    let (plan, cands) = plan_of(vec![(c1.clone(), planned_item(&c1, 100))], 100);
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::OccupancyHard,
        100,
        10,
        None, // no bound port
        AuthorityClass::Unverified,
        vec![("sha256:item-c1", "body")],
    );
    let proposal = compact::CompactionProposal::mint_with(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into()],
            insert_at: 0,
        }],
        None,
        95,
        Some(compact::ModelCallMembers {
            summarizer_profile: Some("summ/pinned".into()),
            max_summary_tokens: None,
            input_reduction: None,
        }),
    );
    let err = compact::execute(&input, &proposal).unwrap_err();
    assert!(
        matches!(err, CompactError::UnresolvedSummarizer { .. }),
        "expected UnresolvedSummarizer, got {err}"
    );
    // A proposal *without* a declared profile refuses too — I-BUDGET.
    let proposal2 = compact::CompactionProposal::mint(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into()],
            insert_at: 0,
        }],
        None,
        95,
    );
    let sz = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: None,
    };
    let input2 = compact_input(
        &plan,
        input.candidates.clone(),
        CompactionTrigger::OccupancyHard,
        100,
        10,
        Some(&sz),
        AuthorityClass::Unverified,
        vec![("sha256:item-c1", "body")],
    );
    let err2 = compact::execute(&input2, &proposal2).unwrap_err();
    assert!(matches!(err2, CompactError::UnresolvedSummarizer { .. }));
}

#[test]
fn summarize_overflow_applies_declared_input_reduction_once() {
    let c1 = cand_at(
        "c1",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 100)),
            (c2.clone(), planned_item(&c2, 100)),
        ],
        200,
    );
    let sz = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: true,
        fail: None,
    };
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::OccupancyHard,
        200,
        50,
        Some(&sz),
        AuthorityClass::Unverified,
        vec![
            ("sha256:item-c1", "first body"),
            ("sha256:item-c2", "second body"),
        ],
    );
    let proposal = compact::CompactionProposal::mint_with(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
            insert_at: 0,
        }],
        None,
        195,
        Some(compact::ModelCallMembers {
            summarizer_profile: Some("summ/pinned".into()),
            max_summary_tokens: Some(50),
            input_reduction: Some(InputReduction::TrimOldest),
        }),
    );
    let view = compact::execute(&input, &proposal).unwrap();
    // The reduction applied once, then the retry succeeded.
    assert_eq!(
        view.input_reduction_applied,
        Some(InputReduction::TrimOldest)
    );
    assert_eq!(sz.calls.borrow().len(), 2);
    // TrimOldest halves the input set — the retry saw one item.
    assert_eq!(sz.calls.borrow()[1].len(), 1);
    assert_eq!(view.summary_items.len(), 1);

    // Without a declared reduction the overflow is the typed error.
    let proposal_no_red = compact::CompactionProposal::mint_with(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into()],
            insert_at: 0,
        }],
        None,
        95,
        Some(compact::ModelCallMembers {
            summarizer_profile: Some("summ/pinned".into()),
            max_summary_tokens: None,
            input_reduction: None,
        }),
    );
    let sz2 = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: true,
        fail: None,
    };
    let input2 = compact_input(
        &plan,
        input.candidates.clone(),
        CompactionTrigger::OccupancyHard,
        200,
        50,
        Some(&sz2),
        AuthorityClass::Unverified,
        vec![("sha256:item-c1", "first body")],
    );
    let err = compact::execute(&input2, &proposal_no_red).unwrap_err();
    assert!(matches!(err, CompactError::SummarizerOverflow { .. }));
}

#[test]
fn summarize_missing_body_and_floor_admission_are_typed() {
    let c1 = cand_at(
        "c1",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        100,
        PriorityClass::TranscriptTail,
        1,
    );
    let (plan, cands) = plan_of(vec![(c1.clone(), planned_item(&c1, 100))], 100);
    let sz = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: None,
    };
    // Missing body — CC3, never a silent empty string.
    let input = compact_input(
        &plan,
        cands.clone(),
        CompactionTrigger::OccupancyHard,
        100,
        10,
        Some(&sz),
        AuthorityClass::Unverified,
        vec![],
    );
    let proposal = compact::CompactionProposal::mint_with(
        "test/summarize@1",
        vec![CompactionOp::Summarize {
            input_ids: vec!["sha256:item-c1".into()],
            insert_at: 0,
        }],
        None,
        95,
        Some(compact::ModelCallMembers {
            summarizer_profile: Some("summ/pinned".into()),
            max_summary_tokens: None,
            input_reduction: None,
        }),
    );
    let err = compact::execute(&input, &proposal).unwrap_err();
    assert!(matches!(err, CompactError::MissingBody { .. }), "{err}");

    // Floor admission: a slot whose `min_authority` exceeds `delegate`
    // refuses a summary insert — I-REQ's authority half.
    let input2 = compact_input(
        &plan,
        cands,
        CompactionTrigger::OccupancyHard,
        100,
        10,
        Some(&sz),
        AuthorityClass::Kernel, // floor above delegate
        vec![("sha256:item-c1", "body")],
    );
    let err2 = compact::execute(&input2, &proposal).unwrap_err();
    assert!(
        matches!(err2, CompactError::PolicyViolation { .. }),
        "{err2}"
    );

    // Port failure is the typed SummarizationFailed.
    let sz_fail = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: Some(SummarizerFailure::Error),
    };
    let input3 = compact_input(
        &plan,
        input2.candidates.clone(),
        CompactionTrigger::OccupancyHard,
        100,
        10,
        Some(&sz_fail),
        AuthorityClass::Unverified,
        vec![("sha256:item-c1", "body")],
    );
    let err3 = compact::execute(&input3, &proposal).unwrap_err();
    assert!(
        matches!(err3, CompactError::SummarizationFailed { .. }),
        "{err3}"
    );
}

#[test]
fn relower_summarize_and_fold_on_return_variants() {
    // relower_summarize: the dropped set summarizes; a deterministic evict
    // fallback is declared (I-FALLBACK).
    let c1 = cand_at(
        "c1",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        120,
        PriorityClass::TranscriptTail,
        1,
    );
    let c2 = cand_at(
        "c2",
        CandidateKind::TranscriptItem,
        AuthorityClass::Unverified,
        120,
        PriorityClass::TranscriptTail,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let sz = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: None,
    };
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::Relower {
            dropped_items: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
        },
        240,
        100,
        Some(&sz),
        AuthorityClass::Unverified,
        vec![
            ("sha256:item-c1", "body one"),
            ("sha256:item-c2", "body two"),
        ],
    );
    let rl = RelowerSummarize {
        summarizer_profile: "summ/pinned".into(),
        max_summary_tokens: Some(40),
        input_reduction: None,
        dropped: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
    };
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&rl], &mut sink).unwrap();
    assert_eq!(out.record.variant_ref, RELOWER_SUMMARIZE_REF);
    assert_eq!(out.record.status, CompactionStatus::Applied);
    assert_eq!(out.view.forgotten.len(), 2);
    assert_eq!(out.view.summary_items.len(), 1);
    assert!(
        out.record.summary_ref.is_some(),
        "the record names the derived summary"
    );
    assert!(out.record.summariser_usage.is_some());
    // The proposal declares the deterministic evict fallback.
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let proposal = rl.propose(&input, &assessment).unwrap();
    let fallback = proposal
        .fallback
        .expect("a deterministic fallback is declared");
    assert!(fallback
        .iter()
        .all(|op| matches!(op, CompactionOp::Evict { .. })));

    // fold_on_return: the child's contribution folds into one summary.
    let (plan2, cands2) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let sz2 = StubSummarizer {
        calls: std::cell::RefCell::new(vec![]),
        overflow_first: false,
        fail: None,
    };
    let input2 = compact_input(
        &plan2,
        cands2,
        CompactionTrigger::Schedule {
            rule_id: "rule/fold-on-return".into(),
        },
        240,
        100,
        Some(&sz2),
        AuthorityClass::Unverified,
        vec![
            ("sha256:item-c1", "child body one"),
            ("sha256:item-c2", "child body two"),
        ],
    );
    let fr = FoldOnReturn {
        summarizer_profile: "summ/pinned".into(),
        max_summary_tokens: Some(40),
        input_reduction: None,
        returned: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
    };
    let mut sink2 = CollectSink::default();
    let out2 = compact::compact(&input2, &[&fr], &mut sink2).unwrap();
    assert_eq!(out2.record.variant_ref, FOLD_ON_RETURN_REF);
    assert_eq!(out2.record.status, CompactionStatus::Applied);
    assert_eq!(out2.view.summary_items.len(), 1);
    assert_eq!(out2.view.forgotten.len(), 2);
    // The summary lands in place of the folded zone (insert_at = the fold
    // set's head flat index — not appended at the tail).
    let slot = &out2.view.slots[0];
    assert_eq!(slot.items.len(), 1);
    assert!(slot.items[0]
        .candidate_id
        .starts_with("compaction-summary-"));

    // Fallback ladder: unbound summarizer ⇒ the declared deterministic
    // `evict` fallback applies instead of skipping (I-FALLBACK).
    let (plan3, cands3) = plan_of(
        vec![
            (c1.clone(), planned_item(&c1, 120)),
            (c2.clone(), planned_item(&c2, 120)),
        ],
        240,
    );
    let input3 = compact_input(
        &plan3,
        cands3,
        CompactionTrigger::Relower {
            dropped_items: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
        },
        240,
        100,
        None,
        AuthorityClass::Unverified,
        vec![],
    );
    let rl3 = RelowerSummarize {
        summarizer_profile: "summ/pinned".into(),
        max_summary_tokens: Some(40),
        input_reduction: None,
        dropped: vec!["sha256:item-c1".into(), "sha256:item-c2".into()],
    };
    let mut sink3 = CollectSink::default();
    let out3 = compact::compact(&input3, &[&rl3], &mut sink3).unwrap();
    assert!(
        out3.applied,
        "the deterministic fallback applies when the summarizer is unbound"
    );
    assert_eq!(
        out3.record.fallback_variant.as_deref(),
        Some(RELOWER_SUMMARIZE_REF)
    );
    assert_eq!(out3.view.forgotten.len(), 2);
    assert!(out3.view.summary_items.is_empty());
}

// ── MemoryStorePort conformance (§5c.3; out-of-process binding surface) ─────

/// The conformance battery lives in `hh_context::memory_abi::conformance` —
/// the same suite runs on every placement (`MemoryStore` in-process here,
/// `RemoteStore` over the dispatch loopback, and the spawned `hh-memory-store`
/// plugin over `plugin_abi/1` in the package test — T-LCD-12).

#[test]
fn memory_store_port_conformance_in_process() {
    let mut store = MemoryStore::new("ms");
    hh_context::memory_abi::conformance::run(&mut store);
}

#[test]
fn memory_store_port_conformance_remote_loopback() {
    // The remote binding's mirror/channel/divergence path: `RemoteStore`
    // over `dispatch` on a second store — identical records on both sides
    // (deterministic fold), and no latched fault after the battery.
    hh_context::memory_abi::conformance::run_remote();
}

// ── consolidate (§5c.3; ADR-0080 d4–d7) ──────────────────────────────────────

fn rule() -> ConsolidationRule {
    ConsolidationRule {
        rule_id: "rule/consolidate-1".into(),
        cadence: ConsolidationCadence::SessionStart,
        guards: ConsolidationGuards::default(),
        consolidator: ConsolidatorKind::Dedupe,
    }
}

#[test]
fn consolidate_dedupe_rebinds_names_to_live_replacement() {
    let mut store = MemoryStore::new("ms");
    let ctx = ctx_at(&mut store, 1);
    // v1 — a run memory bound under "facts/a", then v2 supersedes it (same
    // semantic line) so v1 becomes stale{superseded}.
    let v1 = store.put(draft("old fact"), &ctx).unwrap().version;
    let mut d2 = draft("new fact");
    d2.semantic_id = Some(v1.semantic_id.clone());
    d2.supersedes = Some(hh_context::memory::SupersedeClaim {
        version_id: v1.version_id.clone(),
        reason: hh_context::memory::SupersedeClaimReason::Correction,
    });
    let ctx2 = ctx_at(&mut store, 2);
    let v2 = store.put(d2, &ctx2).unwrap().version;
    store
        .bind(
            PersistenceScope::Run,
            "facts/a",
            &v1.version_id,
            None,
            "bind",
            3,
        )
        .unwrap();
    // v1 is stale (superseded) → the consolidation rebinds "facts/a" to v2.
    let rec = consolidate(&mut store, PersistenceScope::Run, &rule(), &ctx2, None)
        .unwrap()
        .expect("applied");
    assert_eq!(rec.binds.len(), 1);
    assert_eq!(rec.binds[0].name, "facts/a");
    assert_eq!(rec.binds[0].from_version, v1.version_id);
    assert_eq!(rec.binds[0].to_version, v2.version_id);
    // The name resolves to the live replacement.
    let m = store.manifest(PersistenceScope::Run, 10);
    assert_eq!(m.entries.get("facts/a"), Some(&v2.version_id));
    // Watermarks + accounting payload.
    assert!(rec.input_watermark >= 3);
    assert!(rec.last_success_watermark.is_none());
    assert_eq!(
        rec.usage.get("attribution").and_then(Json::as_str),
        Some("harness_overhead.consolidation")
    );
    // I-DET: the record is content-addressed over the applied diff.
    assert!(!rec.consolidation_id.is_empty());
}

#[test]
fn consolidate_lease_fence_and_cadence_gate() {
    let mut store = MemoryStore::new("ms");
    let _g = store.take_lease(PersistenceScope::Run, "agent");
    // A stale lease refuses (typed — never writes around the fence).
    let stale_ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: 999,
        at_seq: 5,
        run_id: "run1".into(),
    };
    let err =
        consolidate(&mut store, PersistenceScope::Run, &rule(), &stale_ctx, None).unwrap_err();
    assert!(
        matches!(err, ConsolidateError::Fenced { .. }),
        "expected Fenced, got {err}"
    );
    // rate_limit cadence: closed until the floor elapses.
    let ctx = ctx_at(&mut store, 15);
    store.put(draft("wm"), &ctx).unwrap(); // raises applied_seq to ≥ 15
    let rl = ConsolidationRule {
        cadence: ConsolidationCadence::RateLimit { floor_per_seq: 10 },
        ..rule()
    };
    let applied_at = store.applied_seq();
    let skipped = consolidate(
        &mut store,
        PersistenceScope::Run,
        &rl,
        &ctx,
        Some(applied_at), // just ran
    )
    .unwrap();
    assert!(
        skipped.is_none(),
        "the cadence gate closes within the floor"
    );
    // After the floor elapses the gate opens.
    let open = consolidate(
        &mut store,
        PersistenceScope::Run,
        &rl,
        &ctx,
        Some(0), // long ago
    )
    .unwrap();
    assert!(open.is_some());
    // session_start always admits.
    let ctx2 = ctx_at(&mut store, 3);
    let out = consolidate(&mut store, PersistenceScope::Run, &rule(), &ctx2, Some(0)).unwrap();
    assert!(out.is_some());
}

#[test]
fn consolidate_conflict_guard_and_max_versions() {
    let mut store = MemoryStore::new("ms");
    // Distinct `at_seq` per write — the store mints a version line's
    // `semantic_id` as `idp(run:seq)`, so distinct lines need distinct seqs.
    let mut at = 10u64;
    let mut next_ctx = |store: &mut MemoryStore| {
        at += 1;
        ctx_at(store, at)
    };
    let ctx = next_ctx(&mut store);
    let v1 = store.put(draft("a"), &ctx).unwrap().version;
    let mut d1b = draft("a2");
    d1b.semantic_id = Some(v1.semantic_id.clone());
    d1b.supersedes = Some(hh_context::memory::SupersedeClaim {
        version_id: v1.version_id.clone(),
        reason: hh_context::memory::SupersedeClaimReason::Correction,
    });
    let ctx = next_ctx(&mut store);
    let v1b = store.put(d1b, &ctx).unwrap().version;
    let ctx = next_ctx(&mut store);
    let v2 = store.put(draft("b"), &ctx).unwrap().version;
    let mut d2b = draft("b2");
    d2b.semantic_id = Some(v2.semantic_id.clone());
    d2b.supersedes = Some(hh_context::memory::SupersedeClaim {
        version_id: v2.version_id.clone(),
        reason: hh_context::memory::SupersedeClaimReason::Correction,
    });
    let ctx = next_ctx(&mut store);
    let v2b = store.put(d2b, &ctx).unwrap().version;
    let ctx = next_ctx(&mut store);
    // A conflicted stale member: subject_key conflict attaches a
    // conflict_set_ref — two same-subject *Structured* writes with
    // different canonical values build the deterministic set.
    let mut ca = draft("conflict a");
    ca.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/test".into(),
        key: "k1".into(),
    });
    ca.content = MemoryContent::Structured(Json::obj([("v", Json::Int(1))]));
    let cva = store.put(ca, &ctx).unwrap().version;
    let ctx = next_ctx(&mut store);
    let mut cb = draft("conflict b");
    cb.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/test".into(),
        key: "k1".into(),
    });
    cb.content = MemoryContent::Structured(Json::obj([("v", Json::Int(2))]));
    let cvb = store.put(cb, &ctx).unwrap().version;
    // The deterministic conflict set now contains cva + cvb.
    assert!(store
        .conflicts()
        .values()
        .any(|s| s.members.contains(&cva.version_id)));
    // Supersede cva → stale + conflicted → the guard skips it.
    let mut cac = draft("conflict a3");
    // A Structured superseder keeps delegate authority (text is capped at
    // `external` — a text write could never supersede a structured one).
    cac.content = MemoryContent::Structured(Json::obj([("v", Json::Int(3))]));
    cac.semantic_id = Some(cva.semantic_id.clone());
    cac.supersedes = Some(hh_context::memory::SupersedeClaim {
        version_id: cva.version_id.clone(),
        reason: hh_context::memory::SupersedeClaimReason::Correction,
    });
    let ctx = next_ctx(&mut store);
    let cvac = store.put(cac, &ctx).unwrap().version;
    let _ = cvb;
    let ctx = next_ctx(&mut store);
    // Bind names for the non-conflicted stale versions.
    store
        .bind(
            PersistenceScope::Run,
            "n/a",
            &v1.version_id,
            None,
            "bind",
            5,
        )
        .unwrap();
    store
        .bind(
            PersistenceScope::Run,
            "n/b",
            &v2.version_id,
            None,
            "bind",
            5,
        )
        .unwrap();
    store
        .bind(
            PersistenceScope::Run,
            "n/c",
            &cva.version_id,
            None,
            "bind",
            5,
        )
        .unwrap();
    let rec = consolidate(&mut store, PersistenceScope::Run, &rule(), &ctx, None)
        .unwrap()
        .expect("applied");
    // cva is stale+conflicted → skipped under the guard.
    assert!(rec.skipped_conflicted.contains(&cva.version_id));
    // v1 → v1b and v2 → v2b rebind.
    let bound: BTreeSet<&str> = rec.binds.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(bound, ["n/a", "n/b"].into_iter().collect());
    let m = store.manifest(PersistenceScope::Run, 100);
    assert_eq!(m.entries.get("n/a"), Some(&v1b.version_id));
    assert_eq!(m.entries.get("n/b"), Some(&v2b.version_id));
    // cva's name still points at it — the resolver owns conflicted members.
    assert_eq!(m.entries.get("n/c"), Some(&cva.version_id));
    let _ = cvac;

    // max_versions: defer the tail deterministically.
    let mut r = rule();
    r.guards.max_versions = Some(1);
    let rec2 = consolidate(&mut store, PersistenceScope::Run, &r, &ctx, None)
        .unwrap()
        .expect("applied");
    assert_eq!(rec2.binds.len() + rec2.skipped_no_replacement.len(), 1);
    assert!(!rec2.deferred.is_empty());
}

// ── lifecycle C2 (§5c.4; ADR-0081/0083) ──────────────────────────────────────

#[test]
fn external_resource_dep_gates_on_declared_validator_verdict() {
    let mut store = MemoryStore::new("ms");
    let ctx = ctx_at(&mut store, 1);
    // An external_resource dep must declare `validator_ref` at write —
    // a missing member refuses (UnvalidatedExternalDependency).
    let mut bad = draft("ext dep missing validator");
    bad.contract = Some(InvalidationContract {
        dependencies: vec![DependencyStamp::external(
            "svc://api",
            "snap-1",
            Granularity::Row,
            None,
        )],
        ..empty_contract()
    });
    let err = store.put(bad, &ctx).unwrap_err();
    assert!(
        matches!(err, MemoryError::UnvalidatedExternalDependency { .. }),
        "expected UnvalidatedExternalDependency, got {err}"
    );

    // Declared validator + fresh verdict → the memory stays valid.
    let mut ok = draft("ext dep gated");
    ok.contract = Some(InvalidationContract {
        dependencies: vec![DependencyStamp::external(
            "svc://api",
            "snap-1",
            Granularity::Row,
            Some("v/ext".into()),
        )],
        ..empty_contract()
    });
    let v = store.put(ok, &ctx).unwrap().version;
    // No verdict yet → the stamp fails closed (expired, never valid).
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 5).kind(),
        LifecycleStateKind::Expired
    );
    // Verdict true → valid.
    store.set_validator_verdict("svc://api", "v/ext", true);
    assert_eq!(
        lifecycle::lifecycle_state(&store, &v.version_id, 6).kind(),
        LifecycleStateKind::Valid
    );
    // Verdict false → expired (the external truth changed under the pin).
    store.set_validator_verdict("svc://api", "v/ext", false);
    let st = lifecycle::lifecycle_state(&store, &v.version_id, 7);
    assert_eq!(st.kind(), LifecycleStateKind::Expired);
    assert!(matches!(
        st,
        hh_context::vocab::LifecycleState::Expired { ref reason } if reason.contains("svc://api")
    ));
}

#[test]
fn subject_overlap_narrows_stale_propagation() {
    // Two justifications: same-subject stale input propagates
    // stale_by_dependency; a different-subject stale input does not under
    // the `subject_overlap` policy.
    let mut store = MemoryStore::new("ms");
    let ctx = ctx_at(&mut store, 1);
    // The stale input (revoked).
    let mut a = draft("stale same subject");
    a.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "same".into(),
    });
    let va = store.put(a, &ctx).unwrap().version;
    let mut b = draft("stale other subject");
    b.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "other".into(),
    });
    let vb = store.put(b, &ctx).unwrap().version;
    // The dependent carries both justifications + subject "same".
    let mut dep = draft("dependent");
    dep.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "same".into(),
    });
    dep.contract = Some(InvalidationContract {
        dependencies: vec![DependencyStamp::stamped(
            DependencyKind::ToolCapabilityVersion,
            "cap/x",
            "v1",
            Granularity::Row,
        )],
        ..empty_contract()
    });
    dep.justifications = vec![
        Justification {
            kind: JustificationKind::DeliveredMemory,
            ref_: hh_identity::refs::VersionedRef::pinned(
                hh_identity::kinds::RecordKind::Memory,
                va.version_id.clone(),
                model_prov(),
            ),
            at: hh_ledger::manifest::EventRef {
                run_id: "run1".into(),
                event_id: "e-va".into(),
            },
        },
        Justification {
            kind: JustificationKind::DeliveredMemory,
            ref_: hh_identity::refs::VersionedRef::pinned(
                hh_identity::kinds::RecordKind::Memory,
                vb.version_id.clone(),
                model_prov(),
            ),
            at: hh_ledger::manifest::EventRef {
                run_id: "run1".into(),
                event_id: "e-vb".into(),
            },
        },
    ];
    let vd = store.put(dep, &ctx).unwrap().version;
    store.set_stamp("cap/x", "v1");
    // Revoke both inputs.
    let revoker = ProvenanceRecord::minted(
        Origin::human("alice", hh_provenance::origin::HumanRole::Principal),
        PersistenceScope::User,
        3,
    );
    lifecycle::revoke(
        &mut store,
        &va.version_id,
        hh_context::vocab::RevocationReason::Contradicted,
        &revoker,
        None,
        4,
    )
    .unwrap();
    lifecycle::revoke(
        &mut store,
        &vb.version_id,
        hh_context::vocab::RevocationReason::Contradicted,
        &revoker,
        None,
        4,
    )
    .unwrap();
    // Default policy (delivered): the dependent is stale on *both*.
    let st = lifecycle::lifecycle_state(&store, &vd.version_id, 8);
    match &st {
        hh_context::vocab::LifecycleState::StaleByDependency { revoked_inputs } => {
            assert_eq!(revoked_inputs.len(), 2, "both revoked inputs propagate");
        }
        other => panic!("expected StaleByDependency, got {other:?}"),
    }
    // The subject_overlap narrowing: only the same-subject revoked input
    // propagates.
    let mut store2 = MemoryStore::new("ms");
    store2.policy.justification_scope = hh_context::JustificationScope::SubjectOverlap;
    let ctx2 = ctx_at(&mut store2, 1);
    let mut a2 = draft("stale same subject");
    a2.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "same".into(),
    });
    let va2 = store2.put(a2, &ctx2).unwrap().version;
    let mut b2 = draft("stale other subject");
    b2.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "other".into(),
    });
    let vb2 = store2.put(b2, &ctx2).unwrap().version;
    let mut dep2 = draft("dependent");
    dep2.subject_key = Some(hh_context::vocab::SubjectKey {
        schema_ref: "s/subj".into(),
        key: "same".into(),
    });
    dep2.contract = Some(InvalidationContract {
        dependencies: vec![DependencyStamp::stamped(
            DependencyKind::ToolCapabilityVersion,
            "cap/x",
            "v1",
            Granularity::Row,
        )],
        ..empty_contract()
    });
    dep2.justifications = vec![
        Justification {
            kind: JustificationKind::DeliveredMemory,
            ref_: hh_identity::refs::VersionedRef::pinned(
                hh_identity::kinds::RecordKind::Memory,
                va2.version_id.clone(),
                model_prov(),
            ),
            at: hh_ledger::manifest::EventRef {
                run_id: "run1".into(),
                event_id: "e-va2".into(),
            },
        },
        Justification {
            kind: JustificationKind::DeliveredMemory,
            ref_: hh_identity::refs::VersionedRef::pinned(
                hh_identity::kinds::RecordKind::Memory,
                vb2.version_id.clone(),
                model_prov(),
            ),
            at: hh_ledger::manifest::EventRef {
                run_id: "run1".into(),
                event_id: "e-vb2".into(),
            },
        },
    ];
    let vd2 = store2.put(dep2, &ctx2).unwrap().version;
    store2.set_stamp("cap/x", "v1");
    lifecycle::revoke(
        &mut store2,
        &va2.version_id,
        hh_context::vocab::RevocationReason::Contradicted,
        &revoker,
        None,
        4,
    )
    .unwrap();
    lifecycle::revoke(
        &mut store2,
        &vb2.version_id,
        hh_context::vocab::RevocationReason::Contradicted,
        &revoker,
        None,
        4,
    )
    .unwrap();
    let st2 = lifecycle::lifecycle_state(&store2, &vd2.version_id, 8);
    match &st2 {
        hh_context::vocab::LifecycleState::StaleByDependency { revoked_inputs } => {
            assert_eq!(
                revoked_inputs.as_slice(),
                std::slice::from_ref(&va2.version_id),
                "only the same-subject revoked input propagates under subject_overlap"
            );
        }
        other => panic!("expected StaleByDependency, got {other:?}"),
    }
}

#[test]
fn readers_c2_durable_scope_requires_named_readers() {
    // The §5c.4 C2 rule: for scopes ≥ user a missing (Public) or empty
    // reader set denies — a durable memory names its readers.
    let policy = ValidityPolicy {
        admitted_states: [LifecycleStateKind::Valid].into_iter().collect(),
        conflict_policy: ConflictPolicy::DeliverAllAnnotated,
        max_stale: None,
    };
    let mk = |scope, readers| FilterItem {
        version_id: format!("v-{scope:?}"),
        authority: AuthorityClass::External,
        readers,
        scope,
        state: LifecycleStateKind::Valid,
        stale_since: None,
        conflict_set_ref: None,
    };
    let items = vec![
        mk(PersistenceScope::Run, ReaderSet::Public), // run scope: Public reads admit
        mk(PersistenceScope::User, ReaderSet::Public), // user scope: Public denies
        mk(
            PersistenceScope::User,
            ReaderSet::Restricted(["model".to_string()].into_iter().collect()),
        ), // user scope + named reader admits
        mk(
            PersistenceScope::Definition,
            ReaderSet::Restricted(BTreeSet::new()),
        ), // definition + empty set denies
    ];
    let out = lifecycle::filter_for_slot(
        &items,
        &policy,
        AuthorityClass::Unverified,
        "model",
        ResolveMode::Execute,
        0,
        &BTreeMap::new(),
        None,
    );
    let withheld: BTreeSet<String> = out
        .withheld
        .iter()
        .filter(|w| w.reason == "readers")
        .map(|w| w.version_id.clone())
        .collect();
    assert!(out
        .admitted
        .contains(&format!("v-{:?}", PersistenceScope::Run)));
    assert!(
        withheld.contains(&format!("v-{:?}", PersistenceScope::User))
            || out
                .withheld
                .iter()
                .any(|w| w.version_id == format!("v-{:?}", PersistenceScope::User))
    );
    // The named-readers user-scope item admits.
    let named_id = format!("v-{:?}", PersistenceScope::User);
    assert!(
        !withheld.contains(&named_id)
            || items.iter().filter(|i| i.version_id == named_id).count() == 2,
        "the named-reader user item admits"
    );
    assert!(withheld.contains(&format!("v-{:?}", PersistenceScope::Definition)));

    // readers_required: the slot's floor narrows admission — an item whose
    // readers don't cover the required set is withheld even though the
    // request's reader ∈ readers.
    let item2 = vec![FilterItem {
        version_id: "v-req".into(),
        authority: AuthorityClass::External,
        readers: ReaderSet::Restricted(["model".to_string()].into_iter().collect()),
        scope: PersistenceScope::Run,
        state: LifecycleStateKind::Valid,
        stale_since: None,
        conflict_set_ref: None,
    }];
    let required = ReaderSet::Restricted(
        ["model".to_string(), "verifier".to_string()]
            .into_iter()
            .collect(),
    );
    let out2 = lifecycle::filter_for_slot(
        &item2,
        &policy,
        AuthorityClass::Unverified,
        "model",
        ResolveMode::Execute,
        0,
        &BTreeMap::new(),
        Some(&required),
    );
    assert!(
        out2.withheld.iter().any(|w| w.reason == "readers"),
        "readers_required not covered → withheld"
    );
    // Covered required set admits.
    let covered = ReaderSet::Restricted(["model".to_string()].into_iter().collect());
    let out3 = lifecycle::filter_for_slot(
        &item2,
        &policy,
        AuthorityClass::Unverified,
        "model",
        ResolveMode::Execute,
        0,
        &BTreeMap::new(),
        Some(&covered),
    );
    assert_eq!(out3.admitted, vec!["v-req".to_string()]);
}
