//! S3.8 context/memory evidence — `clear_tool_results` (§5c.2), the
//! `structural_index` view + `structural` query + `structural_pagerank`
//! (§5c.3), and the two-implementation retrieval determinism
//! (AC-R-2.4.3-7).
//!
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.2-9 (two-arm)** — `clear_tool_results` is a registered
//!   `CompactionStrategy` that executes under the same `compact` driver as
//!   `evict_oldest` (the `lab/compaction-family-v1` arms).
//! - **AC-R-2.4.3-7** — retrieval determinism across two implementations:
//!   `retrieve` and `retrieve_naive` deliver identical `(address, score)`
//!   sequences for identical inputs, indexed and structural paths alike.
//! - **§5c.3 structural** — `ViewKind::StructuralIndex`, the `structural`
//!   query kind, and `structural_pagerank` are deterministic and total.

use std::collections::{BTreeMap, BTreeSet};

use hh_context::compact::{
    self, ClearToolResults, CompactInput, CompactionStatus, CompactionStrategy, CompactionTrigger,
    CLEAR_TOOL_RESULTS_REF,
};
use hh_context::events::CollectSink;
use hh_context::memory::{MemoryDraft, MemoryStore, WriteContext};
use hh_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill, ValidityPolicy,
};
use hh_context::retrieve::{
    self, structural_index, RetrievalBudget, RetrievalIndexes, RetrievalRequest, SlotConstraints,
    DETERMINISTIC_DEFAULT, STRUCTURAL_PAGERANK,
};
use hh_context::vocab::{
    CandidateKind, CandidateState, ConflictPolicy, Layer, LifecycleStateKind, MatchMode,
    MemoryContent, MemoryKind, PriorityClass, Retention, RetrievalQuery,
};
use hh_hir::records::Validity;
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, PersistenceScope};
use hh_provenance::label::Label;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

// ── compaction helpers (mirror acceptance_s28) ───────────────────────────────

fn kprov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("kernel:context", 0)
}

fn tool_prov(cap: &str, inv: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(Origin::tool(cap, inv), PersistenceScope::Run, 0)
}

fn cand_with(
    id: &str,
    kind: CandidateKind,
    prov: ProvenanceRecord,
    tokens: u64,
    seq: u64,
) -> Candidate {
    Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind,
        state: CandidateState::Expanded,
        retention: Retention::Optional(PriorityClass::TranscriptTail),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".to_string(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: prov,
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

// ── clear_tool_results ───────────────────────────────────────────────────────

#[test]
fn clear_tool_results_evicts_only_targeted_completed_invocations() {
    // Two tool results: inv:old (completed at 5, ≤ older_than) targeted;
    // inv:new (completed at 15, > older_than) and a non-tool item survive.
    let c_old = cand_with(
        "c1",
        CandidateKind::Observation,
        tool_prov("tool:shell", "inv:old"),
        100,
        1,
    );
    let c_new = cand_with(
        "c2",
        CandidateKind::Observation,
        tool_prov("tool:shell", "inv:new"),
        100,
        2,
    );
    let c_other = cand_with(
        "c3",
        CandidateKind::Observation,
        tool_prov("tool:web", "inv:web"),
        100,
        3,
    );
    let c_model = cand_with("c4", CandidateKind::TranscriptItem, kprov(), 100, 4);
    let (plan, cands) = plan_of(
        vec![
            (c_old.clone(), planned_item(&c_old, 100)),
            (c_new.clone(), planned_item(&c_new, 100)),
            (c_other.clone(), planned_item(&c_other, 100)),
            (c_model.clone(), planned_item(&c_model, 100)),
        ],
        400,
    );
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::Schedule {
            rule_id: "rule:test".into(),
        },
        400,
        0,
    );
    let ctr = ClearToolResults {
        capabilities: ["tool:shell".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: [
            ("inv:old".to_string(), 5u64),
            ("inv:new".to_string(), 15u64),
            ("inv:web".to_string(), 1u64),
        ]
        .into_iter()
        .collect(),
    };
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&ctr], &mut sink).unwrap();
    assert_eq!(out.record.variant_ref, CLEAR_TOOL_RESULTS_REF);
    assert_eq!(out.record.status, CompactionStatus::Applied);
    let forgotten: BTreeSet<&str> = out.view.forgotten.iter().map(String::as_str).collect();
    assert_eq!(
        forgotten,
        ["sha256:item-c1"].into_iter().collect(),
        "only the targeted, old-enough tool result evicts"
    );
    // I-DET: the same inputs propose the same proposal.
    let assessment = compact::assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let p1 = ctr.propose(&input, &assessment).unwrap();
    let p2 = ctr.propose(&input, &assessment).unwrap();
    assert_eq!(p1.proposal_id, p2.proposal_id);
}

#[test]
fn clear_tool_results_never_evicts_required_or_out_of_window() {
    let mut c_req = cand_with(
        "c1",
        CandidateKind::Observation,
        tool_prov("tool:shell", "inv:old"),
        100,
        1,
    );
    c_req.retention = Retention::Required;
    let c_old = cand_with(
        "c2",
        CandidateKind::Observation,
        tool_prov("tool:shell", "inv:old2"),
        100,
        2,
    );
    let (plan, cands) = plan_of(
        vec![
            (c_req.clone(), planned_item(&c_req, 100)),
            (c_old.clone(), planned_item(&c_old, 100)),
        ],
        200,
    );
    let input = compact_input(
        &plan,
        cands,
        CompactionTrigger::Schedule {
            rule_id: "rule:test".into(),
        },
        200,
        0,
    );
    // inv:old (the required item's invocation) is old; inv:old2 completed
    // after the cutoff — nothing may evict.
    let ctr = ClearToolResults {
        capabilities: ["tool:shell".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: [
            ("inv:old".to_string(), 5u64),
            ("inv:old2".to_string(), 20u64),
        ]
        .into_iter()
        .collect(),
    };
    let mut sink = CollectSink::default();
    let out = compact::compact(&input, &[&ctr], &mut sink).unwrap();
    assert!(
        out.view.forgotten.is_empty(),
        "a required item and an out-of-window item never evict"
    );
}

// ── structural_index + structural query + structural_pagerank ────────────────

fn write_ctx(store: &mut MemoryStore, at: u64) -> WriteContext {
    let g = store.lease(PersistenceScope::Run);
    WriteContext {
        context_label: Label::top(),
        lease_generation: g,
        at_seq: at,
        run_id: "run1".to_string(),
    }
}

fn draft_text(text: &str) -> MemoryDraft {
    MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: None,
        content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
            text,
            "owner",
            ProvenanceRecord::kernel("kernel:mem", 0),
        ))),
        contract: None,
        scope: PersistenceScope::Run,
        declared_inputs: Vec::new(),
        justifications: Vec::new(),
        supersedes: None,
        validity: None,
        provenance: Some(ProvenanceRecord::minted(
            Origin::model("m1", "run1", "r1"),
            PersistenceScope::Run,
            0,
        )),
        semantic_id: None,
        validator_endorsed: false,
    }
}

fn req(query: RetrievalQuery, at: u64, ranker: &str) -> RetrievalRequest {
    RetrievalRequest {
        model_call_id: "mc1".to_string(),
        at: ("run1".to_string(), at),
        layers: [
            Layer::Episodic,
            Layer::Procedural,
            Layer::Session,
            Layer::Artifact,
        ]
        .into_iter()
        .collect(),
        query,
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
        reader: "model".to_string(),
        budget: RetrievalBudget {
            tokens: 10_000,
            k: 100,
        },
        ranker: ranker.to_string(),
        mode: ResolveMode::Execute,
    }
}

fn populated() -> (MemoryStore, String, String) {
    let mut store = MemoryStore::new("ms");
    let ctx = write_ctx(&mut store, 1);
    let a = store
        .put(
            draft_text("the parse_expr function reads src/parser.rs"),
            &ctx,
        )
        .unwrap()
        .version;
    let ctx = write_ctx(&mut store, 2);
    let b = store
        .put(draft_text("unrelated note about lunch"), &ctx)
        .unwrap()
        .version;
    (store, a.version_id, b.version_id)
}

#[test]
fn structural_index_extracts_anchors_and_idents() {
    let (store, a, _b) = populated();
    let idx = structural_index(&store, 2);
    assert!(
        idx.anchors
            .get("src/parser.rs")
            .is_some_and(|vs| vs.contains(&a)),
        "the path token is an anchor: {:?}",
        idx.anchors.keys().collect::<Vec<_>>()
    );
    assert!(
        idx.idents
            .get("parse_expr")
            .is_some_and(|vs| vs.contains(&a)),
        "the snake_case token is an ident: {:?}",
        idx.idents.keys().collect::<Vec<_>>()
    );
    // The stamped view is a `structural_index` view.
    assert_eq!(idx.view.kind, hh_ledger::views::ViewKind::StructuralIndex);
    // Rebuild-equal.
    let idx2 = structural_index(&store, 2);
    assert_eq!(idx.anchors, idx2.anchors);
    assert_eq!(idx.idents, idx2.idents);
}

#[test]
fn structural_query_hits_and_stays_layer_filtered() {
    let (mut store, a, _b) = populated();
    let q = RetrievalQuery::Structural {
        anchors: vec!["src/parser.rs".to_string()],
        mentioned_idents: vec![],
    };
    let mut sink = CollectSink::default();
    let (items, report) = retrieve::retrieve(
        &mut store,
        &req(q, 2, DETERMINISTIC_DEFAULT),
        &mut sink,
        None,
        || 1,
    )
    .unwrap();
    assert_eq!(
        items.iter().map(|i| i.address.as_str()).collect::<Vec<_>>(),
        vec![a.as_str()],
        "the anchored item hits; the unrelated one does not"
    );
    assert_eq!(report.ranker_ref, DETERMINISTIC_DEFAULT);
    assert!(sink
        .events
        .iter()
        .any(|(c, _)| c == "context.retrieval.completed"));
}

#[test]
fn ac_r_2_4_3_7_two_implementations_agree_lexical_and_structural() {
    let (mut store, _a, _b) = populated();
    let queries = [
        RetrievalQuery::Lexical {
            terms: vec!["note".to_string()],
            match_mode: MatchMode::Any,
            context_lines: 0,
            case_sensitive: false,
            normalized: true,
        },
        RetrievalQuery::Structural {
            anchors: vec!["src/parser.rs".to_string()],
            mentioned_idents: vec!["parse_expr".to_string()],
        },
    ];
    for q in queries {
        for ranker in [DETERMINISTIC_DEFAULT, STRUCTURAL_PAGERANK] {
            let request = req(q.clone(), 2, ranker);
            let mut sink = CollectSink::default();
            let (items, _) =
                retrieve::retrieve(&mut store, &request, &mut sink, None, || 1).unwrap();
            let prod: Vec<(String, String)> = items
                .iter()
                .map(|i| (i.address.clone(), i.rank_evidence.score.clone()))
                .collect();
            let naive = retrieve::retrieve_naive(&store, &request).unwrap();
            assert_eq!(prod, naive, "two implementations agree on {q:?}/{ranker}");
        }
    }
}

#[test]
fn ac_r_2_4_3_7_indexed_and_ondemand_views_agree() {
    // The materialized view and the on-demand fold serve identical results.
    let (mut store, _a, _b) = populated();
    let sidx = structural_index(&store, 2);
    let q = RetrievalQuery::Structural {
        anchors: vec![],
        mentioned_idents: vec!["parse_expr".to_string()],
    };
    let request = req(q, 2, STRUCTURAL_PAGERANK);
    let mut s1 = CollectSink::default();
    let (i1, _) = retrieve::retrieve_indexed(
        &mut store,
        &request,
        &mut s1,
        &RetrievalIndexes {
            lexical: None,
            structural: Some(&sidx),
        },
        || 1,
    )
    .unwrap();
    let mut s2 = CollectSink::default();
    let (i2, _) = retrieve::retrieve_indexed(
        &mut store,
        &request,
        &mut s2,
        &RetrievalIndexes::default(),
        || 1,
    )
    .unwrap();
    let a1: Vec<(String, String)> = i1
        .iter()
        .map(|i| (i.address.clone(), i.rank_evidence.score.clone()))
        .collect();
    let a2: Vec<(String, String)> = i2
        .iter()
        .map(|i| (i.address.clone(), i.rank_evidence.score.clone()))
        .collect();
    assert_eq!(a1, a2, "materialized == on-demand");
}

#[test]
fn structural_pagerank_is_deterministic_integer_math() {
    let (store, a, _b) = populated();
    let idx = structural_index(&store, 2);
    let ids: BTreeSet<String> = store.versions().keys().cloned().collect();
    let s1 = retrieve::structural_pagerank(&idx, &ids);
    let s2 = retrieve::structural_pagerank(&idx, &ids);
    assert_eq!(s1, s2);
    // The anchored item outranks the unrelated one (it sits on the graph).
    assert!(s1.get(&a).copied().unwrap_or(0) > 0);
    let _ = Json::Null;
}

// ── AC-R-2.4.1-1/-2: the executable privilege-preservation fixtures ──────────
// M-CPE: discovered instruction file, injected observation, project skill
// body, prior-run memory — each at ≤ external lands only in `external`/
// `unverified` slots (never kernel/definition/principal); the join keeps
// `context_label.authority = external`.

use hh_context::assemble::{assemble, AssemblyError, AssemblyRequest};
use hh_context::plan::default_layout;
use hh_context::plan::ContextBudget;
use hh_context::policy::Selection;
use hh_context::policy::{ContextPolicy, DefaultPolicy, PolicyRequest};

fn asm_req(candidates: Vec<Candidate>, cap: u64) -> AssemblyRequest {
    AssemblyRequest {
        model_call_id: "mc1".to_string(),
        view: DerivedFrom {
            run_id: "run1".to_string(),
            seq: 10,
            view_hash: "sha256:view".to_string(),
        },
        at_seq: 10,
        min_view_seq: None,
        candidates,
        advisories: Vec::new(),
        layout: default_layout(),
        budget: ContextBudget {
            window_cap: cap,
            margin: 0,
            reservations: Vec::new(),
            hard: true,
        },
        estimator_ref: "est/pinned".to_string(),
        policy_params: Json::Null,
    }
}

#[test]
fn ac_r_2_4_1_1_mcpe_fixture_never_reaches_privileged_slots() {
    // The four M-CPE shapes — all `external` (or below) regardless of kind:
    // a discovered instruction file, an injected tool observation, a project
    // skill body, a prior-run memory.
    let fixture = vec![
        cand_with(
            "mcp:instructions",
            CandidateKind::DefinitionInstruction,
            ProvenanceRecord::minted(
                Origin::tool("tool:fs", "inv:scan"),
                PersistenceScope::Project,
                0,
            ),
            50,
            1,
        ),
        cand_with(
            "mcp:injected",
            CandidateKind::Observation,
            ProvenanceRecord::minted(
                Origin::tool("tool:shell", "inv:1"),
                PersistenceScope::Run,
                0,
            ),
            50,
            2,
        ),
        cand_with(
            "mcp:skill-body",
            CandidateKind::ProcedureBody,
            ProvenanceRecord::minted(
                Origin::tool("tool:fs", "inv:skill"),
                PersistenceScope::Project,
                0,
            ),
            50,
            3,
        ),
        cand_with(
            "mcp:prior-run-memory",
            CandidateKind::Memory,
            ProvenanceRecord::minted(
                Origin::model("m1", "run0", "r0"),
                PersistenceScope::Session,
                0,
            ),
            50,
            4,
        ),
    ];
    // Stamp every one at `external` — the M-CPE cap.
    let fixture: Vec<Candidate> = fixture
        .into_iter()
        .map(|mut c| {
            c.label = Label::at(AuthorityClass::External);
            c
        })
        .collect();
    let mut sink = CollectSink::default();
    let out = assemble(
        &asm_req(fixture, 10_000),
        &DefaultPolicy::default(),
        "layout/default",
        "none",
        3,
        &mut sink,
    )
    .unwrap();
    for fill in &out.plan.slots {
        assert!(
            matches!(
                fill.slot_id.as_str(),
                "external" | "unverified" | "transcript"
            ),
            "an M-CPE item never lands in a privileged slot: {}",
            fill.slot_id
        );
        for it in &fill.items {
            assert!(it.authority <= AuthorityClass::External);
        }
    }
    // I-LABEL — the join reports external while any external inline item is
    // delivered.
    assert_eq!(out.plan.context_label.authority, AuthorityClass::External);
    // And the relevance-first top-role injection is inexpressible: a hostile
    // policy trying to seat one in `kernel` is a PolicyViolation.
    struct InjectTop;
    impl ContextPolicy for InjectTop {
        fn declare(&self) -> hh_context::policy::PolicyDeclaration {
            DefaultPolicy::default().declare()
        }
        fn priority(&self, _c: &Candidate) -> PriorityClass {
            PriorityClass::Commentary
        }
        fn select(
            &self,
            req: &PolicyRequest<'_>,
        ) -> Result<Selection, hh_context::policy::PolicyViolation> {
            let v = req.candidates.first().unwrap();
            Ok(Selection {
                chosen: vec![(v.candidate.candidate_id.clone(), "kernel".to_string())],
                order: [("kernel".to_string(), vec![v.candidate.candidate_id.clone()])]
                    .into_iter()
                    .collect(),
                evict_order: vec![],
                by_reference: vec![],
                expand_requests: vec![],
            })
        }
    }
    let fixture2 = vec![{
        let mut c = cand_with(
            "mcp:x",
            CandidateKind::DefinitionInstruction,
            tool_prov("tool:fs", "inv:x"),
            50,
            1,
        );
        c.label = Label::at(AuthorityClass::External);
        c
    }];
    let mut sink2 = CollectSink::default();
    assert!(matches!(
        assemble(
            &asm_req(fixture2, 10_000),
            &InjectTop,
            "layout/default",
            "none",
            3,
            &mut sink2,
        ),
        Err(AssemblyError::PolicyViolation { .. })
    ));
}

#[test]
fn ac_r_2_4_1_2_xcpe_external_memory_never_enters_principal_slot() {
    // A memory written after external content rides ≤ external; a slot with
    // `min_authority ≥ principal` never admits it — only a ledgered
    // promotion endorsement lifts it (covered in acceptance_s28).
    let mut mem = cand_with(
        "xcpe:mem",
        CandidateKind::Memory,
        tool_prov("tool:fs", "inv:read"),
        60,
        1,
    );
    mem.label = Label::at(AuthorityClass::External);
    let mut principal = cand_with(
        "xcpe:msg",
        CandidateKind::PrincipalMessage,
        ProvenanceRecord::minted(
            Origin::human("alice", hh_provenance::origin::HumanRole::Principal),
            PersistenceScope::User,
            0,
        ),
        20,
        2,
    );
    principal.label = Label::at(AuthorityClass::Principal);
    let mut sink = CollectSink::default();
    let out = assemble(
        &asm_req(vec![mem, principal], 10_000),
        &DefaultPolicy::default(),
        "layout/default",
        "none",
        3,
        &mut sink,
    )
    .unwrap();
    // The external memory is never in `principal`/`kernel`/`definition`.
    for fill in &out.plan.slots {
        if fill.slot_id == "principal" {
            assert!(fill
                .items
                .iter()
                .all(|i| i.authority >= AuthorityClass::Principal));
        }
    }
    assert_eq!(out.plan.context_label.authority, AuthorityClass::External);
}
