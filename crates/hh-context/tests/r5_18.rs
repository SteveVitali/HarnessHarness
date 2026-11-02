//! R5.18 — the durable retained selection (`selection` member of
//! `context.compaction.completed`) and its deterministic replay
//! (REQ-E-206 contributing slice, §2.3; the ticket's AC-1/AC-2).
//!
//! Fixture discipline: the scenarios mint real `compact()` outcomes
//! through the production ladder — the emitted payload's `selection`
//! member is the durable row under test, never a fixture-asserted
//! label.

use std::collections::{BTreeMap, BTreeSet};

use hharness_context::compact::{
    self, ClearToolResults, CompactInput, CompactionTrigger, EvictOldest,
};
use hharness_context::events::CollectSink;
use hharness_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill,
};
use hharness_context::retained::{
    self, RetainedSelection, SelectionOutcome, SelectionRefusal, SelectionVerdict,
    RETAINED_SELECTION_SCHEMA,
};
use hharness_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hharness_hir::records::Validity;
use hharness_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_wire::json::Json;

// ── fixtures (the s5_2 shapes, trimmed) ─────────────────────────────────────

fn toolprov(cap: &str, inv: &str) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: Origin::tool(cap, inv),
        authority: AuthorityClass::Delegate,
        taint: BTreeSet::new(),
        readers: ReaderSet::Public,
        scope: PersistenceScope::Run,
        derived_from: Vec::new(),
        created_at: 0,
        attestation: None,
    }
}

fn cand(id: &str, tokens: u64, priority: PriorityClass, seq: u64) -> Candidate {
    Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(priority),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".into(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: ProvenanceRecord::kernel("kernel:context", 0),
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

fn planned(c: &Candidate) -> PlannedItem {
    PlannedItem {
        candidate_id: c.candidate_id.clone(),
        context_item_id: c.context_item_id.clone().unwrap(),
        artefact_id: None,
        delivery_id: format!("del:{}", c.candidate_id),
        authority: c.label.authority,
        label: c.label.clone(),
        tokens: c.estimate.tokens,
        state: c.state,
        delivered_by_reference: false,
        derived_from: None,
    }
}

fn plan_of(cands: &[Candidate], occupancy: u64) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let candidates: BTreeMap<String, Candidate> = cands
        .iter()
        .map(|c| (c.candidate_id.clone(), c.clone()))
        .collect();
    let items: Vec<PlannedItem> = cands.iter().map(planned).collect();
    let plan = ContextPlan {
        plan_id: "plan:r5_18".into(),
        model_call_id: "mc1".into(),
        derived_from: DerivedFrom {
            run_id: "run1".into(),
            seq: 10,
            view_hash: "sha256:view".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items,
        }],
        omitted: vec![],
        context_label: Label::top(),
        occupancy_estimate: occupancy,
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

fn cinput<'a>(
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
        summarizer: None,
        slot_min_authority: BTreeMap::new(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    }
}

/// Drive `compact()` and decode the emitted `selection` member.
fn selection_of(
    input: &CompactInput,
    variants: &[&dyn compact::CompactionStrategy],
) -> (compact::CompactOutcome, RetainedSelection) {
    let mut sink = CollectSink::default();
    let out = compact::compact(input, variants, &mut sink).unwrap();
    let completed = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .expect("completed row emitted");
    let member = completed
        .1
        .get("selection")
        .expect("selection member present");
    (
        out,
        RetainedSelection::from_json(member).expect("selection decodes"),
    )
}

/// The base set: a `paired_with` group, a `batch_id` group, a required
/// item — 400 evictable tokens + the protected 100.
fn base_set() -> Vec<Candidate> {
    let mut a = cand("a", 100, PriorityClass::Commentary, 1);
    let mut b = cand("b", 100, PriorityClass::ObservationOld, 2);
    a.paired_with = Some("b".to_string());
    b.paired_with = Some("a".to_string());
    let mut c = cand("c", 100, PriorityClass::ObservationRecent, 3);
    let mut d = cand("d", 100, PriorityClass::ObservationRecent, 4);
    c.batch_id = Some("batch-1".to_string());
    d.batch_id = Some("batch-1".to_string());
    let mut req = cand("req", 100, PriorityClass::TranscriptTail, 5);
    req.retention = Retention::Required;
    vec![a, b, c, d, req]
}

// ── AC-1 — persist the exact selection; replay reproduces ────────────────────

#[test]
fn ac1_evict_oldest_selection_persists_and_replays() {
    let cands = base_set();
    // occupancy 500 / cap 400 / needed 0 → min_reclaim 300; the
    // {a,b,c,d} evict frees 392 — Applied.
    let (plan, cmap) = plan_of(&cands, 500);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let (out, sel) = selection_of(&input, &[]);

    // Exact admitted policy + parameters.
    assert_eq!(sel.input.policy.variant_ref, "hh/evict-oldest@1");
    assert_eq!(sel.input.policy.parameters, Json::obj([]));
    assert_eq!(sel.status, compact::CompactionStatus::Applied);
    // The recorded joins: pair, batch, required group.
    let item = |id: &str| {
        sel.input
            .items
            .iter()
            .find(|i| i.context_item_id == format!("sha256:item-{id}"))
            .unwrap()
    };
    assert_eq!(item("a").paired_with.as_deref(), Some("b"));
    assert_eq!(item("c").batch_id.as_deref(), Some("batch-1"));
    assert_eq!(item("req").retention, Some(Retention::Required));
    // Retained/omitted partition the input; the protected group stays.
    let total: BTreeSet<&String> = sel.retained.iter().chain(sel.omitted.iter()).collect();
    assert_eq!(total.len(), sel.input.items.len());
    assert_eq!(sel.retained, vec!["sha256:item-req".to_string()]);
    assert_eq!(out.record.forgotten, sel.omitted);
    // Replay reproduces the durable selection exactly.
    match retained::verify_selection(&sel) {
        SelectionVerdict::Reproduced { outcome, .. } => {
            assert_eq!(outcome, SelectionOutcome::Applied)
        }
        v => panic!("expected Reproduced, got {v:?}"),
    }
    // Canonical round-trip is byte-stable.
    let back = RetainedSelection::from_json(&sel.to_json()).unwrap();
    assert_eq!(back, sel);
}

#[test]
fn ac1_clear_tool_results_persists_parameters_and_replays() {
    let mut t1 = cand("t1", 100, PriorityClass::ObservationOld, 1);
    t1.provenance = toolprov("read_file", "inv-1");
    let mut t2 = cand("t2", 100, PriorityClass::ObservationOld, 2);
    t2.provenance = toolprov("read_file", "inv-2");
    let mut t3 = cand("t3", 100, PriorityClass::ObservationOld, 3);
    t3.provenance = toolprov("read_file", "inv-3");
    let keep = cand("keep", 100, PriorityClass::TranscriptTail, 4);
    let cands = vec![t1, t2, t3, keep];
    // occupancy 300 / cap 400 → to_target 100; {t1,t2} completes ≤ 10,
    // t3's invocation completed at 20 — retained.
    let (plan, cmap) = plan_of(&cands, 300);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let ctr = ClearToolResults {
        capabilities: ["read_file".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: [
            ("inv-1".to_string(), 5u64),
            ("inv-2".to_string(), 7),
            ("inv-3".to_string(), 20),
        ]
        .into_iter()
        .collect(),
    };
    let eol = EvictOldest;
    let (_out, sel) = selection_of(&input, &[&ctr, &eol]);

    assert_eq!(sel.input.policy.variant_ref, "hh/clear-tool-results@1");
    let p = &sel.input.policy.parameters;
    assert_eq!(p.get("older_than").and_then(Json::as_int), Some(10));
    assert!(p
        .get("completed_invocations")
        .and_then(|c| c.get("inv-1"))
        .is_some());
    // The tool join rides each item row; t3 stays retained.
    assert!(sel
        .input
        .items
        .iter()
        .any(|i| i.tool.as_ref().is_some_and(|t| t.capability == "read_file")));
    assert_eq!(
        sel.omitted,
        vec!["sha256:item-t1".to_string(), "sha256:item-t2".to_string()]
    );
    match retained::verify_selection(&sel) {
        SelectionVerdict::Reproduced { outcome, .. } => {
            assert_eq!(outcome, SelectionOutcome::Applied)
        }
        v => panic!("expected Reproduced, got {v:?}"),
    }
}

#[test]
fn ac1_payload_is_additive_and_input_is_the_shared_representation() {
    let cands = base_set();
    let (plan, cmap) = plan_of(&cands, 500);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let mut sink = CollectSink::default();
    let _ = compact::compact(&input, &[], &mut sink).unwrap();
    let completed = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .unwrap();
    // The pre-existing members stand; `selection` is the additive one.
    assert!(completed.1.get("compaction_id").is_some());
    assert!(completed.1.get("forgotten").is_some());
    let sel = completed.1.get("selection").unwrap();
    assert_eq!(
        sel.get("schema").and_then(Json::as_str),
        Some(RETAINED_SELECTION_SCHEMA)
    );
    // The shared input representation the prepared call consumes: the
    // assessed envelope + the flattened join rows.
    let i = sel.get("input").unwrap();
    assert_eq!(i.get("window_cap").and_then(Json::as_int), Some(400));
    assert_eq!(i.get("occupancy").and_then(Json::as_int), Some(500));
    assert!(i.get("cut_points").is_some());
    assert!(i.get("items").is_some());
    assert_eq!(
        i.get("trigger")
            .and_then(|t| t.get("kind"))
            .and_then(Json::as_str),
        Some("occupancy_hard")
    );
}

// ── AC-2 — the negatives keep explicit distinct results ─────────────────────

#[test]
fn ac2_distinct_refusals() {
    let cands = base_set();
    let (plan, cmap) = plan_of(&cands, 500);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let (_out, sel) = selection_of(&input, &[]);

    // Unknown policy.
    let mut r = sel.clone();
    r.input.policy.variant_ref = "hh/unknown@9".into();
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::UnknownPolicy { .. })
    ));

    // Duplicate ids — item id and candidate id forms.
    let mut r = sel.clone();
    let dup = r.input.items[0].clone();
    r.input.items.push(dup);
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::DuplicateIds { .. })
    ));
    let mut r = sel.clone();
    let mut dup = r.input.items[0].clone();
    dup.context_item_id = "sha256:other".into();
    r.input.items.push(dup);
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::DuplicateIds { .. })
    ));

    // Protected-group eviction: a required item in `omitted`.
    let mut r = sel.clone();
    r.omitted.push("sha256:item-req".to_string());
    r.retained.retain(|i| i != "sha256:item-req");
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::ProtectedGroup { .. })
    ));
    // …and a kernel-authored row (no candidate) is protected equally.
    let mut r = sel.clone();
    r.input.items[0].candidate_id = None;
    r.input.items[0].retention = None;
    r.omitted = vec![r.input.items[0].context_item_id.clone()];
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::ProtectedGroup { .. })
    ));

    // Orphan pair: the selection split a `paired_with` group.
    let mut r = sel.clone();
    r.omitted = vec!["sha256:item-a".to_string()];
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::OrphanPair { .. })
    ));

    // Orphan batch: the selection split a `batch_id` group.
    let mut r = sel.clone();
    r.omitted = vec!["sha256:item-c".to_string()];
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::OrphanBatch { .. })
    ));

    // Trigger absent — struct-level and through the durable member
    // (`trigger: null` decodes `None`, never a fabricated default).
    let mut r = sel.clone();
    r.input.trigger = None;
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::TriggerAbsent)
    ));
    let r = RetainedSelection::from_json(&r.to_json()).unwrap();
    assert!(matches!(
        retained::replay_selection(&r),
        Err(SelectionRefusal::TriggerAbsent)
    ));
}

#[test]
fn ac2_missing_completion_age_join() {
    // t3's tool join has no `completed_invocations` entry in the bound
    // map — the persisted record cannot prove its age was evaluated:
    // a distinct refusal on replay, never an untargeted read.
    let mut t1 = cand("t1", 100, PriorityClass::ObservationOld, 1);
    t1.provenance = toolprov("read_file", "inv-1");
    let mut t2 = cand("t2", 100, PriorityClass::ObservationOld, 2);
    t2.provenance = toolprov("read_file", "inv-2");
    let mut t3 = cand("t3", 100, PriorityClass::ObservationOld, 3);
    t3.provenance = toolprov("read_file", "inv-3");
    let cands = vec![t1, t2, t3, cand("x", 100, PriorityClass::TranscriptTail, 4)];
    let (plan, cmap) = plan_of(&cands, 300);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let ctr = ClearToolResults {
        capabilities: ["read_file".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: [("inv-1".to_string(), 5u64), ("inv-2".to_string(), 7)]
            .into_iter()
            .collect(),
    };
    let eol = EvictOldest;
    let (_out, sel) = selection_of(&input, &[&ctr, &eol]);
    assert_eq!(sel.input.policy.variant_ref, "hh/clear-tool-results@1");
    assert!(matches!(
        retained::replay_selection(&sel),
        Err(SelectionRefusal::MissingCompletionAge { item_id, .. }) if item_id == "sha256:item-t3"
    ));
    assert!(matches!(
        retained::verify_selection(&sel),
        SelectionVerdict::Refused(SelectionRefusal::MissingCompletionAge { .. })
    ));
}

#[test]
fn ac2_untriggered_ineffective_fallback_and_divergence() {
    // Untriggered: nothing to reclaim — the distinct outcome kind.
    let cands = base_set();
    let (plan, cmap) = plan_of(&cands, 100);
    let input = cinput(
        &plan,
        cmap,
        CompactionTrigger::OccupancySoft {
            rule_id: "r".into(),
        },
        10_000,
        0,
    );
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[], &mut sink).unwrap();
    let sel = out.record.selection.clone();
    assert!(sel.omitted.is_empty());
    let re = retained::replay_selection(&sel).unwrap();
    assert_eq!(re.outcome, SelectionOutcome::Untriggered);
    match retained::verify_selection(&sel) {
        SelectionVerdict::Reproduced { outcome, .. } => {
            assert_eq!(outcome, SelectionOutcome::Untriggered)
        }
        v => panic!("expected Reproduced, got {v:?}"),
    }

    // Ineffective: the rung's ops free less than the hard requirement
    // (392 < 400) — the distinct `Ineffective` outcome; the recorded
    // fallback leg stays distinct.
    let cands = base_set();
    let (plan, cmap) = plan_of(&cands, 700);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 600, 1);
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let proposal = compact::evict_oldest_propose(&input, &assessment).unwrap();
    let view = compact::execute(&input, &proposal).unwrap();
    let rec = RetainedSelection::mint(
        &input,
        &view,
        compact::CompactionStatus::Ineffective,
        "hh/evict-oldest@1",
        Json::obj([]),
        0,
        Some("hh/declared@1".to_string()),
    );
    match retained::verify_selection(&rec) {
        SelectionVerdict::Reproduced {
            outcome,
            via_fallback,
        } => {
            assert_eq!(outcome, SelectionOutcome::Ineffective);
            assert_eq!(via_fallback.as_deref(), Some("hh/declared@1"));
        }
        v => panic!("expected Reproduced, got {v:?}"),
    }

    // Divergence: a tampered retained set is reported, never
    // reconciled (an already-omitted id may not also be retained).
    let cands = base_set();
    let (plan, cmap) = plan_of(&cands, 500);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let (_out, mut sel) = selection_of(&input, &[]);
    sel.retained.push("sha256:item-c".to_string());
    assert!(matches!(
        retained::verify_selection(&sel),
        SelectionVerdict::Diverged { .. }
    ));
}
