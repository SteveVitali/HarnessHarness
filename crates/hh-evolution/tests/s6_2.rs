//! The S6.2 battery (§05h §2.4–§4; R-2.9.5, R-2.10.3⁴, R-2.6.4⁴,
//! R-2.6.5⁴, R-2.4.1⁴/2.4.2⁴/2.4.4⁴) — sub-stage 6b: the
//! `evolution_proposer` class + suite, the AHE-shaped first-party
//! variant (one target class per campaign), the automated-family
//! campaign gates (origin, stale-base, `coordination_loosening` at
//! S1+S7, G7 judge selectors + audit budget, G8 complete search
//! accounting), split canaries under `RolloutPolicy`, the three
//! adaptive-search allocators with their durable rows, the `predictor`
//! compute_policy variant, and the §5c legs (memory-lineage corpus,
//! debt-manager over layout rules).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::matchspec::{MatchMode, MatchSpec};
use hh_evolution::campaign::{doc_kind, EvolutionCampaign};
use hh_evolution::errors::{EvolutionError, Refusal};
use hh_evolution::proposer::{
    AheProposer, CorpusRow, EvidenceCorpus, EvolutionProposer, LineageEntry, ProposalConstraints,
};
use hh_evolution::proposer_conformance::{
    run_proposer_suite, InProcessPort, ProposerPort, SuiteDrive,
};
use hh_evolution::records::{
    AcceptanceItem, EvolutionAcceptanceReport, ItemStatus, PortabilityLabel,
    SecurityInvarianceReport, TransferRow,
};
use hh_evolution::records::{
    CandidateProposal, CorpusSpec, EvolutionCampaignSpec, FailureHypothesis, JudgePolicy,
    ParentSelectionPolicy, PredictedDelta, PredictedEffect, ProposerDeclaration, ProposerOutcome,
    RolloutPolicy, ScreenReport, SelectorDeclaration, SlotAllocation, StopRule, Tri,
};
use hh_experiment::docs::LabDocs;
use hh_hir::diff::{self, Delta, DiffDerivation, HirDiff};
use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::idp::idp_id;
use hh_lab::analysis::{
    BenefitKind, BudgetMatch, BudgetMatchStatus, ComparisonReport, Multiplicity, OutcomeBounds,
    OutcomeBoundsVerdict, PairedEffect, ReportLabelKind, SignProfile, TailEffects, TestKind,
    TestRecord,
};
use hh_lab::bench::SplitAssignmentRecord;
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};
use hh_lab::model::{CompatibilityRecord, CompatibilityStatus};
use hh_ledger::ids::ManualClock;
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::debt::ExpiryCondition;
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, FactorKind, IntervalMethod, Pairing, PreRegistration,
    RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::{EnvironmentFamily, SplitLabel};
use hh_ontology::participant::ParticipantClass;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixture plumbing (the s6_1a shape — same store/docs/doc machinery) ──

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-evolution-s62-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn rig(tag: &str) -> (PathBuf, ManualClock, Store, LabDocs) {
    let root = tmp(tag);
    let clock = ManualClock::at(1_000);
    let store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    (root, clock, store, docs)
}

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

/// The automated family's origin — `evolution{candidate_id,
/// hypothesis_ref}` (the diff's own provenance; S1's family leg).
fn evo_prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::evolution("cand:test", "hyp:test"),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn rule_node(id: &str, seq: u64, trigger: Json) -> Node {
    sid(
        Node::new(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger,
                action: RuleAction::RequestApproval(Json::Null),
                // `scope.threshold` is the AHE proposer's tighten-able
                // numeric leaf (`*_ppm`/`*_cap`/threshold names).
                scope: Json::obj([("threshold", Json::Int(500_000))]),
                conditioned_on: None,
                assumption_debt: None,
            }),
            prov(seq),
        ),
        id,
    )
}

fn budget_node(id: &str, seq: u64) -> Node {
    let dimensions = BTreeMap::from([(
        "tokens.blended".to_string(),
        DimensionBound {
            hard: Some(1_000),
            soft: None,
        },
    )]);
    sid(
        Node::new(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions,
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            prov(seq),
        ),
        id,
    )
}

fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
    sid(
        Node::new(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel(holder),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            prov(seq),
        ),
        id,
    )
}

fn agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
    let mut slots = BTreeMap::new();
    for name in ["control_strategy", "context_policy"] {
        slots.insert(
            name.to_string(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::pinned(
                name,
                format!("hh/{name}/test"),
                format!("sha256:variant-{name}"),
            ))),
        );
    }
    sid(
        Node::new(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel(id),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel(budget),
                    permissions: sel(perm),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            prov(seq),
        ),
        id,
    )
}

fn base_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node(
        "test:rule",
        1,
        Json::obj([("on", Json::str("turn_end"))]),
    ));
    doc.nodes.push(budget_node("test:budget", 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

fn target_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node(
        "test:rule",
        1,
        Json::obj([("on", Json::str("turn_start"))]),
    ));
    doc.nodes.push(budget_node("test:budget", 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

fn candidate_diff(base: &HirDocument, target: &HirDocument, prov: ProvenanceRecord) -> HirDiff {
    // A non-human/migration diff origin needs a derived-from member —
    // the fixture names the evolution candidate id.
    let candidate_id = match &prov.origin {
        Origin::Evolution { candidate_id, .. } => Some(candidate_id.clone()),
        _ => None,
    };
    match diff::diff(
        base,
        target,
        prov,
        DiffDerivation {
            hypothesis: None,
            trajectories: vec![],
            candidate_id,
        },
    ) {
        Ok(d) => d,
        Err(errs) => panic!("fixture diff failed: {errs:?}"),
    }
}

fn deposit_split(docs: &LabDocs, splits: &[(&str, SplitLabel)]) -> String {
    let rec = SplitAssignmentRecord {
        suite_id: "suite:test".into(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: splits.iter().map(|(t, l)| (t.to_string(), *l)).collect(),
        split_hash: idp_id("test.split", b"fixture").to_string(),
        registered_at: 1,
    };
    docs.put_named(doc_kind::SPLIT, "test-split", &rec.to_json())
        .unwrap();
    "test-split".to_string()
}

/// A complete `SearchBudgetRecord` — `proposer` slice included when the
/// family is automated (G8's named-facet rule).
fn search_budget_doc(automated: bool) -> hh_ontology::eval::SearchBudgetRecord {
    let allocation = if automated {
        BTreeMap::from([
            ("proposer".to_string(), 500_000u64),
            ("search".to_string(), 500_000),
        ])
    } else {
        BTreeMap::from([("search".to_string(), 1_000_000u64)])
    };
    hh_ontology::eval::SearchBudgetRecord {
        rollouts: 4,
        model_calls: 4,
        tokens: 8_000,
        feedback_labels_used: 0,
        judge_calls: 0,
        wall_clock: 1_000,
        allocation,
        adaptive_selection: false,
    }
}

fn spec(split_ref: &str) -> EvolutionCampaignSpec {
    EvolutionCampaignSpec {
        campaign_id: String::new(),
        protocol: "human_proposed".into(),
        base_definition_ref: "def:base".into(),
        service_definition_ref: "def:evolution-service".into(),
        corpus: CorpusSpec {
            environments: vec![EnvironmentFamily::CodingTerminal],
            suite_refs: vec!["suite:test".into()],
            metric_refs: vec!["metric:success".into(), "metric:latency".into()],
            task_ids: vec!["task:a".into(), "task:b".into()],
            evidence_refs: vec!["ev:1".into(), "ev:2".into()],
            split_assignment_ref: split_ref.into(),
            layers: None,
        },
        exclusion_targets: vec![],
        must_code_targets: vec![],
        allowed_target_kinds: None,
        semantic_ops_bound: 8,
        min_flip_share_ppm: 500_000,
        min_replicates: 1,
        retention_margin_ppm: 50_000,
        veto_metrics: vec!["metric:safety".into()],
        slot_allocation: SlotAllocation::Uniform { min_share_ppm: 0 },
        stop_rule: StopRule {
            max_candidates: Some(10),
            stagnation_window: Some(5),
            budget_cap_ref: None,
            no_addressable_failure: true,
        },
        maturity_flags: vec![],
        proposer_family: "human".into(),
        hosted_participants: false,
        reported_only_dimensions: vec![],
        proposer_variant_ref: None,
        target_class: None,
        rollout_policy: None,
        judge_policy: None,
    }
}

/// The automated-family spec — `protocol = automated`,
/// `proposer_family = ahe`, one `target_class`, the registered variant
/// ref, and the memory-lineage corpus layer.
fn automated_spec(split_ref: &str) -> EvolutionCampaignSpec {
    let mut s = spec(split_ref);
    s.protocol = "automated".into();
    s.proposer_family = "ahe".into();
    s.proposer_variant_ref = Some("variant:hh/evolution_proposer/ahe".into());
    s.target_class = Some("guideline".into());
    s.allowed_target_kinds = Some(vec!["HarnessRule".into()]);
    s.corpus.layers = Some(vec!["outcomes".into(), "memory_lineage".into()]);
    s
}

fn open_with(
    tag: &str,
    s: EvolutionCampaignSpec,
) -> (PathBuf, ManualClock, Store, LabDocs, EvolutionCampaign) {
    let (root, clock, mut store, docs) = rig(tag);
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::Search)],
    );
    let mut s = s;
    s.corpus.split_assignment_ref = split_ref;
    let (_, eng) = EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s)
        .unwrap_or_else(|e| panic!("campaign open: {e}"));
    (root, clock, store, docs, eng)
}

fn open(tag: &str) -> (PathBuf, ManualClock, Store, LabDocs, EvolutionCampaign) {
    open_with(tag, spec("test-split"))
}

fn refusal(e: &EvolutionError) -> &Refusal {
    match e {
        EvolutionError::Refusal(r) => r,
        o => panic!("expected a typed refusal, got {o:?}"),
    }
}

fn propose_with(
    store: &mut Store,
    eng: &mut EvolutionCampaign,
    base: &HirDocument,
    target: &HirDocument,
    prov: ProvenanceRecord,
    base_ref: &str,
) -> Result<String, EvolutionError> {
    let d = candidate_diff(base, target, prov);
    eng.propose(
        store,
        &CandidateProposal {
            base_ref: base_ref.into(),
            diff: d,
            slot: "guideline".into(),
            hypothesis: None,
        },
        base,
    )
}

// ── 1. The AheProposer contract ──────────────────────────────────────

fn golden_corpus() -> EvidenceCorpus {
    EvidenceCorpus {
        layers: vec!["outcomes".into(), "memory_lineage".into()],
        outcome_rows: vec![
            CorpusRow {
                task_id: "task:a".into(),
                arm: "base".into(),
                metric: "metric:success".into(),
                value_ppm: 400_000,
                replicates: 4,
            },
            CorpusRow {
                task_id: "task:a".into(),
                arm: "sibling".into(),
                metric: "metric:success".into(),
                value_ppm: 800_000,
                replicates: 4,
            },
        ],
        slot_history: BTreeMap::new(),
        memory_lineage: vec![],
        prior_candidates: vec![],
        exclusions: vec![],
        task_ids: vec!["task:a".into()],
        metric_refs: vec!["metric:success".into()],
    }
}

fn constraints(base_ref: &str) -> ProposalConstraints {
    ProposalConstraints {
        base_ref: base_ref.into(),
        target_classes: vec!["guideline".into()],
        exclusions: vec![],
        must_code: vec![],
        max_semantic_ops: 8,
        allowed_kinds: vec!["HarnessRule".into()],
        budget_slice_ppm: 500_000,
    }
}

#[test]
fn ahe_declares_the_family_shape() {
    let p = AheProposer::new("guideline");
    let d = p.declare();
    assert_eq!(d.family, "ahe");
    assert_eq!(d.op_classes_admissible, vec!["guideline".to_string()]);
    assert_eq!(d.uses_judge, Tri::No);
    assert_eq!(d.maturity, "instrument-grade");
    assert_eq!(p.judge_calls(), 0);
    // The declaration's conditioned rules all carry debt records —
    // the static suite kind's completeness leg.
    assert!(d.conditioned_rules.iter().all(|r| r.debt_record.is_some()));
    // And the declaration is canonical round-trippable.
    let back = ProposerDeclaration::from_json(&d.to_json()).unwrap();
    assert_eq!(back, d);
}

#[test]
fn ahe_proposes_candidate_and_hypothesis_on_addressable_corpus() {
    let mut p = AheProposer::new("guideline");
    let base = base_doc();
    let outs = p
        .propose(&golden_corpus(), &base, &constraints("def:base"))
        .unwrap_or_else(|e| panic!("propose: {e:?}"));
    assert_eq!(outs.len(), 2);
    let ProposerOutcome::Candidate(c) = &outs[0] else {
        panic!("expected a CandidateProposal, got {outs:?}")
    };
    assert_eq!(c.base_ref, "def:base");
    // `origin = evolution` — the automated family's provenance (S1's
    // family leg checks it).
    assert!(matches!(c.diff.provenance.origin, Origin::Evolution { .. }));
    // The emitted diff tightens: no widening/loosening leg.
    assert!(!matches!(
        c.diff.classification.authority_delta,
        diff::AuthorityDelta::Widening
    ));
    assert!(c.diff.classification.budget_delta != Delta::Loosening);
    assert!(c.diff.classification.validity_delta != Delta::Loosening);
    assert!(c.diff.classification.coordination_delta != Delta::Loosening);
    // Invertible: apply then invert restores the base byte-identically.
    let t = diff::apply(&base, &c.diff).unwrap();
    let back = diff::apply(&t, &diff::invert(&c.diff)).unwrap();
    assert_eq!(back.canonical_bytes(), base.canonical_bytes());
    // The hypothesis is the second outcome — falsifiable, corpus-cited.
    let ProposerOutcome::Hypothesis(h) = &outs[1] else {
        panic!("expected the hypothesis outcome")
    };
    assert!(!h.predicted.deltas.is_empty());
    assert!(!h.evidence_refs.is_empty());
    assert_eq!(h.predicted.affected_task_ids, vec!["task:a".to_string()]);
}

#[test]
fn ahe_returns_typed_no_addressable_failure() {
    let mut p = AheProposer::new("guideline");
    // A corpus where the base arm is never beaten.
    let mut corpus = golden_corpus();
    corpus.outcome_rows = vec![
        CorpusRow {
            task_id: "task:a".into(),
            arm: "base".into(),
            metric: "metric:success".into(),
            value_ppm: 900_000,
            replicates: 4,
        },
        CorpusRow {
            task_id: "task:a".into(),
            arm: "sibling".into(),
            metric: "metric:success".into(),
            value_ppm: 400_000,
            replicates: 4,
        },
    ];
    let outs = p
        .propose(&corpus, &base_doc(), &constraints("def:base"))
        .unwrap();
    assert!(matches!(
        outs.as_slice(),
        [ProposerOutcome::NoAddressableFailure { .. }]
    ));
    // And an empty corpus answers the same typed arm — never [].
    let outs2 = p
        .propose(
            &EvidenceCorpus::default(),
            &base_doc(),
            &constraints("def:base"),
        )
        .unwrap();
    assert!(matches!(
        outs2.as_slice(),
        [ProposerOutcome::NoAddressableFailure { .. }]
    ));
}

#[test]
fn ahe_refuses_second_class_and_unsatisfiable_constraints() {
    use hh_evolution::records::ProposerFailure;
    let mut p = AheProposer::new("guideline");
    let base = base_doc();
    // Two classes — the 6b one-class rule.
    let mut c = constraints("def:base");
    c.target_classes = vec!["guideline".into(), "scheduling_rule".into()];
    let e = p.propose(&golden_corpus(), &base, &c).unwrap_err();
    assert!(matches!(e, ProposerFailure::ConstraintUnsatisfiable { .. }));
    // A different class — the variant's construction-time pin refuses.
    let mut c = constraints("def:base");
    c.target_classes = vec!["compaction_guideline".into()];
    let e = p.propose(&golden_corpus(), &base, &c).unwrap_err();
    assert!(matches!(e, ProposerFailure::ConstraintUnsatisfiable { .. }));
    // No admitted kinds — unsatisfiable.
    let mut c = constraints("def:base");
    c.allowed_kinds = vec![];
    let e = p.propose(&golden_corpus(), &base, &c).unwrap_err();
    assert!(matches!(e, ProposerFailure::ConstraintUnsatisfiable { .. }));
    // The exclusion set covers every target leaf — unsatisfiable.
    let mut c = constraints("def:base");
    c.exclusions = vec![
        "test:rule".into(),
        "test:budget".into(),
        "test:perm".into(),
        "test:agent".into(),
    ];
    let e = p.propose(&golden_corpus(), &base, &c).unwrap_err();
    assert!(matches!(e, ProposerFailure::ConstraintUnsatisfiable { .. }));
}

#[test]
fn ahe_budget_slice_exhausts() {
    use hh_evolution::records::ProposerFailure;
    let mut p = AheProposer::new("guideline");
    let base = base_doc();
    let corpus = golden_corpus();
    let mut c = constraints("def:base");
    c.budget_slice_ppm = 1; // one proposal's worth of slice
    p.propose(&corpus, &base, &c).unwrap();
    let e = p.propose(&corpus, &base, &c).unwrap_err();
    assert!(matches!(e, ProposerFailure::BudgetExhausted { .. }));
}

#[test]
fn ahe_guideline_text_is_the_optimisation_leg() {
    // R-2.4.1⁴ — a target-class leaf carrying a *guideline* `Text`
    // member takes the deterministic rewrite (the numeric arm is
    // `*_ppm`/`*_cap`/threshold tightening).
    let mut p = AheProposer::new("guideline");
    let mut base = HirDocument::new(sel("test:agent"));
    base.nodes.push(sid(
        Node::new(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:guideline.rule".into(),
                trigger: Json::obj([("on", Json::str("turn_end"))]),
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::obj([("guideline", Json::str("prefer cheap tools first"))]),
                conditioned_on: None,
                assumption_debt: None,
            }),
            prov(1),
        ),
        "test:guideline",
    ));
    let outs = p
        .propose(&golden_corpus(), &base, &constraints("def:base"))
        .unwrap();
    let ProposerOutcome::Candidate(c) = &outs[0] else {
        panic!("expected a candidate, got {outs:?}")
    };
    // The emitted diff touches only `test:guideline` — apply it and
    // check the rewrite named the observed task.
    let t = diff::apply(&base, &c.diff).unwrap();
    let node = t
        .nodes
        .iter()
        .find(|n| n.semantic_id() == "test:guideline")
        .unwrap();
    let KindRecord::HarnessRule(r) = &node.semantic else {
        panic!("kind changed")
    };
    let text = r
        .scope
        .get("guideline")
        .and_then(Json::as_str)
        .expect("guideline member");
    assert!(text.contains("prefer cheap tools first"));
    assert!(
        text.contains("task:a"),
        "the rewrite names the observed failure: {text}"
    );
    // Invert restores the base.
    let back = diff::apply(&t, &diff::invert(&c.diff)).unwrap();
    assert_eq!(back.canonical_bytes(), base.canonical_bytes());
}

#[test]
fn ahe_corpus_carries_memory_lineage() {
    // R-2.4.4⁴ — `memory_lineage` is a first-class corpus layer: the
    // links ride `EvidenceCorpus` into `propose`; a forbidden layer
    // name is `CorpusUnreadable` at the proposer's own gate too.
    use hh_evolution::proposer::LineageLink;
    use hh_evolution::records::ProposerFailure;
    let mut corpus = golden_corpus();
    corpus.memory_lineage = vec![
        LineageLink {
            version_ref: "mem:v2".into(),
            derived_from: Some("mem:v1".into()),
            supersedes: Some("mem:v1".into()),
        },
        LineageLink {
            version_ref: "mem:v1".into(),
            derived_from: None,
            supersedes: None,
        },
    ];
    let mut p = AheProposer::new("guideline");
    let outs = p
        .propose(&corpus, &base_doc(), &constraints("def:base"))
        .unwrap_or_else(|e| panic!("lineage corpus propose: {e:?}"));
    assert!(matches!(outs[0], ProposerOutcome::Candidate(_)));
    // The layer is in the corpus the proposer saw.
    assert!(corpus.layers.iter().any(|l| l == "memory_lineage"));
    assert_eq!(
        corpus.memory_lineage[0].derived_from.as_deref(),
        Some("mem:v1")
    );
    // A forbidden layer at the proposer boundary — CorpusUnreadable.
    let mut bad = corpus.clone();
    bad.layers.push("held_out".into());
    let e = p
        .propose(&bad, &base_doc(), &constraints("def:base"))
        .unwrap_err();
    assert!(matches!(e, ProposerFailure::CorpusUnreadable { .. }));
}

#[test]
fn ahe_select_parent_policies() {
    let p = AheProposer::new("guideline");
    let entry = |id: &str, score: u64, children: u32| LineageEntry {
        candidate_id: id.into(),
        target_ref: DefinitionVersionRef {
            semantic_id: format!("def:{id}"),
            version_id: format!("ver:{id}"),
        },
        score_ppm: score,
        children,
        per_task: BTreeMap::new(),
    };
    let mut lineage = vec![entry("a", 100, 0), entry("b", 900, 0), entry("c", 500, 3)];
    for (e, pt) in lineage.iter_mut().zip([50u64, 600, 400]) {
        e.per_task = BTreeMap::from([("task:a".to_string(), pt)]);
    }
    let r = p
        .select_parent(&lineage, &ParentSelectionPolicy::Best)
        .unwrap();
    assert_eq!(r.semantic_id, "def:b");
    let r = p
        .select_parent(
            &lineage,
            &ParentSelectionPolicy::ScoreChildProportional {
                alpha_ppm: 1,
                children_penalty_ppm: 400,
            },
        )
        .unwrap();
    assert_eq!(r.semantic_id, "def:b");
    // `pareto_per_task` — the key is the per-task *minimum* (d's 700
    // beats b's 600 even though b's headline score is higher).
    let mut pt = entry("d", 10, 0);
    pt.per_task = BTreeMap::from([("task:a".to_string(), 700u64)]);
    let mut lineage2 = lineage.clone();
    lineage2.push(pt);
    let r = p
        .select_parent(&lineage2, &ParentSelectionPolicy::ParetoPerTask)
        .unwrap();
    assert_eq!(r.semantic_id, "def:d");
    // Empty lineage → None.
    assert!(p.select_parent(&[], &ParentSelectionPolicy::Best).is_none());
}

// ── 2. The four-kind conformance suite over the in-process port ──────

#[test]
fn proposer_suite_in_process_passes() {
    use hh_registry::kinds::ConformanceVerdict::*;
    let mut p = AheProposer::new("guideline");
    let base = base_doc();
    let corpus = golden_corpus();
    let cons = constraints("def:base");
    let mut port = InProcessPort::new(&mut p, base.clone(), corpus.clone(), cons.clone());
    let drive = SuiteDrive {
        corpus,
        base,
        constraints: cons,
        in_process: true,
    };
    let rows = run_proposer_suite(&mut port, &drive);
    assert!(!rows.is_empty());
    for r in &rows {
        assert!(
            matches!(r.verdict, Supported | NotApplicable),
            "row {:?} failed: {}",
            r.subject,
            r.detail
        );
    }
    for kind in ["static", "contract", "property", "executable"] {
        assert!(
            rows.iter().any(|r| r.subject.starts_with(kind)),
            "no {kind} rows in {rows:?}"
        );
    }
}

/// A mock port for the DRIFT/isolation legs — `uses_judge = no` on the
/// declaration but a non-zero judge-call count is `Unsupported{drift}`.
struct DriftyPort;

impl ProposerPort for DriftyPort {
    fn declare(&mut self) -> Result<Json, String> {
        Ok(AheProposer::new("guideline").declare().to_json())
    }
    fn propose(&mut self, _req: &Json) -> Result<Vec<Json>, String> {
        Ok(vec![Json::obj([
            ("kind", Json::str("no_addressable_failure")),
            ("reason", Json::str("drifty mock")),
        ])])
    }
    fn select_parent(&mut self, _l: &Json, _p: &Json) -> Result<Json, String> {
        Ok(Json::Null)
    }
    fn judge_call_attempts(&self) -> u64 {
        3 // DRIFT — a `uses_judge = no` declaration with live calls
    }
    fn foreign_read_attempts(&self) -> u64 {
        1 // a read outside the corpus — the isolation probe fails
    }
}

#[test]
fn proposer_suite_flags_drift_and_foreign_reads() {
    use hh_registry::kinds::ConformanceVerdict::*;
    let mut port = DriftyPort;
    let drive = SuiteDrive {
        corpus: golden_corpus(),
        base: base_doc(),
        constraints: constraints("def:base"),
        in_process: true,
    };
    let rows = run_proposer_suite(&mut port, &drive);
    let judge = rows
        .iter()
        .find(|r| r.subject == "property.judge_use")
        .expect("judge row");
    assert_eq!(judge.verdict, Unsupported);
    assert!(judge.detail.contains("DRIFT"));
    let foreign = rows
        .iter()
        .find(|r| r.subject == "property.isolation.corpus_only")
        .expect("isolation row");
    assert_eq!(foreign.verdict, Unsupported);
}

/// A malformed declaration — `static.declaration` fails.
struct BadDeclPort;

impl ProposerPort for BadDeclPort {
    fn declare(&mut self) -> Result<Json, String> {
        let mut d = AheProposer::new("guideline").declare();
        d.maturity = "bogus-grade".into();
        Ok(d.to_json())
    }
    fn propose(&mut self, _req: &Json) -> Result<Vec<Json>, String> {
        Ok(vec![Json::obj([
            ("kind", Json::str("no_addressable_failure")),
            ("reason", Json::str("mock")),
        ])])
    }
    fn select_parent(&mut self, _l: &Json, _p: &Json) -> Result<Json, String> {
        Ok(Json::Null)
    }
    fn judge_call_attempts(&self) -> u64 {
        0
    }
    fn foreign_read_attempts(&self) -> u64 {
        0
    }
}

#[test]
fn proposer_suite_fails_bad_declaration() {
    use hh_registry::kinds::ConformanceVerdict::*;
    let mut port = BadDeclPort;
    let drive = SuiteDrive {
        corpus: golden_corpus(),
        base: base_doc(),
        constraints: constraints("def:base"),
        in_process: true,
    };
    let rows = run_proposer_suite(&mut port, &drive);
    let st = rows
        .iter()
        .find(|r| r.subject == "static.declaration")
        .expect("static row");
    assert_eq!(st.verdict, Unsupported);
}

// ── 3. The campaign's automated-family gates ─────────────────────────

#[test]
fn automated_family_admits_evolution_origin() {
    let (_r, _c, mut store, _docs, mut eng) = open_with("auto1", automated_spec("test-split"));
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        evo_prov(10),
        "def:base",
    )
    .unwrap_or_else(|e| panic!("evolution-origin propose: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("classified"));
}

#[test]
fn automated_family_refuses_human_origin_and_vice_versa() {
    let (_r, _c, mut store, _docs, mut eng) = open_with("auto2", automated_spec("test-split"));
    let e = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::InvalidProvenance { .. }));
    let (_r, _c, mut store, _docs, mut eng) = open("hum-ev");
    let e = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        evo_prov(10),
        "def:base",
    )
    .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::InvalidProvenance { .. }));
}

#[test]
fn open_refuses_incomplete_automated_family() {
    let (_r, _c, mut store, docs) = rig("incomplete");
    let split_ref = deposit_split(&docs, &[]);
    // `automated` without a proposer_variant_ref — schema violation.
    let mut s = spec(&split_ref);
    s.protocol = "automated".into();
    s.proposer_family = "ahe".into();
    match EvolutionCampaign::open(&mut store, docs.clone(), "h", 60_000, s) {
        Err(EvolutionError::Refusal(Refusal::SchemaViolation { .. })) => {}
        Err(o) => panic!("expected SchemaViolation, got {o:?}"),
        Ok(_) => panic!("the incomplete automated family opened"),
    }
    // A `target_class` outside the §5c evolvable set — violation.
    let mut s = automated_spec(&split_ref);
    s.target_class = Some("evolution_proposer".into()); // X6's own class
    match EvolutionCampaign::open(&mut store, docs.clone(), "h", 60_000, s) {
        Err(EvolutionError::Refusal(Refusal::SchemaViolation { .. })) => {}
        Err(o) => panic!("expected SchemaViolation for a non-evolvable class, got {o:?}"),
        Ok(_) => panic!("a non-evolvable target_class opened"),
    }
    // `proposer_family = human` with `protocol = automated` — incoherent.
    let mut s = spec(&split_ref);
    s.protocol = "automated".into();
    match EvolutionCampaign::open(&mut store, docs, "h", 60_000, s) {
        Err(EvolutionError::Refusal(Refusal::SchemaViolation { .. })) => {}
        Err(o) => panic!("expected SchemaViolation for incoherent family, got {o:?}"),
        Ok(_) => panic!("an incoherent family opened"),
    }
}

#[test]
fn proposer_variant_is_outside_the_target_set() {
    // G5/X6 — an op touching the registered proposer variant refuses
    // `SelfModificationRefused`.
    let (_r, _c, mut store, _docs, mut eng) = open_with("x6", automated_spec("test-split"));
    let mut base = base_doc();
    base.nodes.push(rule_node(
        "variant:hh/evolution_proposer/ahe",
        5,
        Json::obj([("on", Json::str("turn_end"))]),
    ));
    let mut target = base.clone();
    for n in target.nodes.iter_mut() {
        if n.semantic_id() == "variant:hh/evolution_proposer/ahe" {
            if let KindRecord::HarnessRule(r) = &mut n.semantic {
                r.trigger = Json::obj([("on", Json::str("turn_start"))]);
            }
        }
    }
    let e = propose_with(
        &mut store,
        &mut eng,
        &base,
        &target,
        evo_prov(10),
        "def:base",
    )
    .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SelfModificationRefused { .. }
    ));
}

#[test]
fn coordination_loosening_refused_at_s1() {
    // `coordination_delta = loosening` refuses alongside widening (K-4 /
    // ADR-0193; the ADR-0053 D-5 shape — the hand-set classification is
    // what S1 reads).
    let (_r, _c, mut store, _docs, mut eng) = open("coord");
    let mut d = candidate_diff(&base_doc(), &target_doc(), prov(10));
    d.classification.coordination_delta = Delta::Loosening;
    let e = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:base".into(),
                diff: d,
                slot: "guideline".into(),
                hypothesis: None,
            },
            &base_doc(),
        )
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::CoordinationLoosening { .. }));
}

#[test]
fn stale_base_after_head_moves() {
    // OQ-061/ADR-0195 D12 — once an accepted edit advances the head, a
    // proposal on the old base is `StaleBase`.
    let (_r, _c, mut store, docs, mut eng) = open("stale");
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    drive_to_active(&mut store, &docs, &mut eng, &cid);
    assert_eq!(eng.view.state_of(&cid), Some("active"));
    let e = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(11),
        "def:base",
    )
    .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::StaleBase { .. }));
}

// ── 4. Split canary under RolloutPolicy ─────────────────────────────

#[test]
fn split_canary_commits_under_rollout_policy() {
    let mut s = spec("test-split");
    s.rollout_policy = Some(RolloutPolicy {
        mode: "split".into(),
        share_ppm: 50_000,
        min_runs: 0,
        max_duration_ms: 0,
        abort_on: vec!["veto".into()],
        advance_rule: "clean".into(),
        assignment_seed: "seed:test".into(),
    });
    let (_r, _c, mut store, docs, mut eng) = open_with("split", s);
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    drive_to_sealed(&mut store, &docs, &mut eng, &cid);
    eng.canary(&mut store, &cid, "split", "int:1")
        .unwrap_or_else(|e| panic!("split canary: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("canary"));
    // The exploratory mark rides the durable transition row.
    let row = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.evolution.candidate.transitioned"
                && e.payload.get("to") == Some(&Json::str("canary"))
        })
        .expect("canary transition row");
    assert_eq!(row.payload.get("exploratory"), Some(&Json::Bool(true)));
    assert_eq!(row.payload.get("comparable"), Some(&Json::Bool(false)));
    assert_eq!(
        row.payload.get("assignment_seed"),
        Some(&Json::str("seed:test"))
    );
    eng.canary_settle(&mut store, &cid, "clean", "", &[])
        .unwrap();
    assert_eq!(eng.view.state_of(&cid), Some("active"));
}

#[test]
fn split_canary_refuses_without_policy_and_veto_reverts() {
    let (_r, _c, mut store, docs, mut eng) = open("nosplit");
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    drive_to_sealed(&mut store, &docs, &mut eng, &cid);
    let e = eng.canary(&mut store, &cid, "split", "int:1").unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SplitRolloutNotCommittable { .. }
    ));
    // With the policy, a veto settle lands `reverted`.
    let mut s = spec("test-split");
    s.rollout_policy = Some(RolloutPolicy {
        mode: "split".into(),
        share_ppm: 50_000,
        min_runs: 0,
        max_duration_ms: 0,
        abort_on: vec!["veto".into()],
        advance_rule: "clean".into(),
        assignment_seed: "seed:test".into(),
    });
    let (_r, _c, mut store, docs, mut eng) = open_with("splitv", s);
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    drive_to_sealed(&mut store, &docs, &mut eng, &cid);
    eng.canary(&mut store, &cid, "split", "int:1").unwrap();
    let e = eng
        .canary_settle(&mut store, &cid, "veto", "metric:safety tripped", &[])
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::CanaryAborted { .. }));
    assert_eq!(eng.view.state_of(&cid), Some("reverted"));
}

// ── 5. G7/G8 — judge selectors and complete search accounting ────────

#[test]
fn g8_unresolved_or_incomplete_search_budget_refuses() {
    let (_r, _c, mut store, docs, mut eng) = open("g8");
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    let search = stage_spec(&eng, ExperimentKind::Comparative, SplitLabel::Search);
    docs.put_spec(&search).unwrap();
    // No `budget:search` doc deposited — the ref does not resolve.
    let rep = deposit_comparison(&docs, "rep:unresolved");
    let e = eng
        .matched_eval(&mut store, &cid, &search.experiment_id, &rep)
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SearchBudgetIncomplete { .. }
    ));
    // An incomplete record — allocation shares don't sum to PPM.
    let (_r, _c, mut store, docs, mut eng) = open("g8b");
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    let search = stage_spec(&eng, ExperimentKind::Comparative, SplitLabel::Search);
    docs.put_spec(&search).unwrap();
    docs.put_named(
        doc_kind::SEARCH_BUDGET,
        "budget:search",
        &hh_ontology::eval::SearchBudgetRecord {
            rollouts: 4,
            model_calls: 4,
            tokens: 8_000,
            feedback_labels_used: 0,
            judge_calls: 0,
            wall_clock: 1_000,
            allocation: BTreeMap::from([("search".to_string(), 500_000u64)]),
            adaptive_selection: false,
        }
        .to_json(),
    )
    .unwrap();
    let rep = deposit_comparison(&docs, "rep:incomplete");
    let e = eng
        .matched_eval(&mut store, &cid, &search.experiment_id, &rep)
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SearchBudgetIncomplete { .. }
    ));
}

#[test]
fn g8_automated_family_needs_the_proposer_slice() {
    // A *complete* record without the `proposer` facet refuses —
    // `put_named` is write-once so each leg runs its own campaign.
    let (_r, _c, mut store, docs, mut eng) = open_with("g8auto", automated_spec("test-split"));
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        evo_prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    let search = stage_spec(&eng, ExperimentKind::Comparative, SplitLabel::Search);
    docs.put_spec(&search).unwrap();
    docs.put_named(
        doc_kind::SEARCH_BUDGET,
        "budget:search",
        &search_budget_doc(false).to_json(),
    )
    .unwrap();
    let rep = deposit_comparison(&docs, "rep:nofacet");
    let e = eng
        .matched_eval(&mut store, &cid, &search.experiment_id, &rep)
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SearchBudgetIncomplete { .. }
    ));
    // With the proposer facet declared, S4 lands.
    let (_r, _c, mut store, docs, mut eng) = open_with("g8auto2", automated_spec("test-split"));
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        evo_prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    let search = stage_spec(&eng, ExperimentKind::Comparative, SplitLabel::Search);
    docs.put_spec(&search).unwrap();
    docs.put_named(
        doc_kind::SEARCH_BUDGET,
        "budget:search",
        &search_budget_doc(true).to_json(),
    )
    .unwrap();
    let rep = deposit_comparison(&docs, "rep:facet");
    eng.matched_eval(&mut store, &cid, &search.experiment_id, &rep)
        .unwrap_or_else(|e| panic!("matched_eval with proposer slice: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("searched"));
}

#[test]
fn g7_undeclared_selector_and_honeypot_floor() {
    let (_r, _c, mut store, _docs, mut eng) = open("g7");
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let mut rep = screen_ok();
    rep.selector_ref = Some("sel:rogue".into());
    rep.honeypots = 2;
    let e = eng.screen(&mut store, &cid, &rep).unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::JudgeSelectorUndeclared { .. }
    ));
    // Declared selector, honeypot floor missed.
    let mut s = spec("test-split");
    s.judge_policy = Some(JudgePolicy {
        selectors: vec![SelectorDeclaration {
            selector_ref: "sel:a".into(),
            kind: "judge".into(),
            calibration_ref: "cal:a".into(),
            independent_of: vec!["def:base".into()],
        }],
        audit_budget_ref: "budget:audit".into(),
        audited_share_ppm: 100_000,
        min_honeypots: 3,
    });
    let (_r, _c, mut store, _docs, mut eng) = open_with("g7b", s);
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(10),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let mut rep = screen_ok();
    rep.selector_ref = Some("sel:a".into());
    rep.honeypots = 1;
    let e = eng.screen(&mut store, &cid, &rep).unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::JudgeSelectorUndeclared { .. }
    ));
    // Floor met + independent_of ∋ base_ref → the screen lands — on a
    // fresh candidate (a refused screen rejected the first).
    let cid = propose_with(
        &mut store,
        &mut eng,
        &base_doc(),
        &target_doc(),
        prov(11),
        "def:base",
    )
    .unwrap();
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let mut rep = screen_ok();
    rep.selector_ref = Some("sel:a".into());
    rep.honeypots = 3;
    eng.screen(&mut store, &cid, &rep)
        .unwrap_or_else(|e| panic!("declared selector screen: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("screened"));
}

// ── 6. §5c legs — memory lineage + debt-manager coverage ─────────────

#[test]
fn memory_lineage_layer_and_evidence_prefix() {
    let (_r, _c, mut store, docs) = rig("lineage");
    let split_ref = deposit_split(&docs, &[]);
    // A `memory:`-prefixed evidence ref without the layer — violation.
    let mut s = spec(&split_ref);
    s.corpus.evidence_refs.push("memory:mem:v1".into());
    match EvolutionCampaign::open(&mut store, docs.clone(), "h", 60_000, s) {
        Err(EvolutionError::Refusal(Refusal::SchemaViolation { .. })) => {}
        Err(o) => panic!("expected SchemaViolation for undeclared memory: ref, got {o:?}"),
        Ok(_) => panic!("an undeclared `memory:` ref opened"),
    }
    // With `memory_lineage` declared the ref is admissible.
    let mut s = spec(&split_ref);
    s.corpus.evidence_refs.push("memory:mem:v1".into());
    s.corpus.layers = Some(vec!["outcomes".into(), "memory_lineage".into()]);
    EvolutionCampaign::open(&mut store, docs.clone(), "h", 60_000, s)
        .unwrap_or_else(|e| panic!("memory-lineage corpus: {e}"));
    // A held-out layer name is a leak, never a schema slip.
    let mut s = spec(&split_ref);
    s.corpus.layers = Some(vec!["held_out".into()]);
    match EvolutionCampaign::open(&mut store, docs, "h", 60_000, s) {
        Err(e) => assert!(matches!(refusal(&e), Refusal::HeldOutLeak { .. })),
        Ok(_) => panic!("a held_out layer opened"),
    }
}

#[test]
fn debt_manager_covers_layout_rules() {
    // Layout rules are `harness_rule` records — the debt manager's
    // `home_for` resolves their `assumption_debt` field (R-2.4.1⁴'s
    // debt-coverage leg).
    let home = hh_lab::debt::home_for("harness_rule", "assumption_debt");
    assert!(
        home.is_some(),
        "no debt home for harness_rule.assumption_debt"
    );
    assert!(hh_lab::debt::home_for("bogus_kind", "assumption_debt").is_none());
}

// ── 7. The registry class + suite are in the corpus ──────────────────

#[test]
fn registry_registers_class_and_suite() {
    use hh_registry::records::RegistryRecord;
    let dir = tmp("regcorpus");
    let (_root_v, _variants_a) = hh_registry::corpus::build(&dir).unwrap();
    let store =
        hh_registry::store::RegistryStore::open(&dir, &hh_registry::corpus::kernel_registrar())
            .unwrap();
    let vids_a: Vec<String> = store.version_ids().cloned().collect();
    // Walk the registered records — the class lands with its
    // `conformance_suite_ref` back-patched onto a registered suite
    // carrying exactly the four kinds.
    let mut class_found = false;
    let mut suite_ref_found = false;
    let mut kinds_seen = Vec::new();
    for vid in &vids_a {
        let Some((_v, rec)) = store.get(vid).map(|(v, r)| (v, r.clone())) else {
            continue;
        };
        match rec {
            RegistryRecord::Class(c) if c.class_id == "evolution_proposer" => {
                class_found = true;
                if let Some(sv) = &c.conformance_suite_ref {
                    suite_ref_found = true;
                    if let Some((_sv, RegistryRecord::Suite(s))) =
                        store.get(sv).map(|(v, r)| (v, r.clone()))
                    {
                        kinds_seen = s.tests.iter().map(|t| t.kind).collect();
                    }
                }
            }
            _ => {}
        }
    }
    assert!(class_found, "evolution_proposer class not registered");
    assert!(suite_ref_found, "conformance_suite_ref never back-patched");
    for k in [
        hh_registry::kinds::TestKind::Static,
        hh_registry::kinds::TestKind::Contract,
        hh_registry::kinds::TestKind::Executable,
        hh_registry::kinds::TestKind::Property,
    ] {
        assert!(kinds_seen.contains(&k), "suite missing kind {k:?}");
    }
    // Deterministic registration — a second corpus build yields the
    // identical version-id set (content-addressed ids).
    let dir2 = tmp("regcorpus2");
    let (_r2, _v2) = hh_registry::corpus::build(&dir2).unwrap();
    let store2 =
        hh_registry::store::RegistryStore::open(&dir2, &hh_registry::corpus::kernel_registrar())
            .unwrap();
    let vids_b: Vec<String> = store2.version_ids().cloned().collect();
    assert_eq!(vids_a, vids_b, "corpus build is not deterministic");
}

// ── 8. The adaptive allocators (R-2.10.3⁴) ───────────────────────────

#[test]
fn disagreement_weighted_allocator_replays() {
    let params = hh_lab::adaptive::DisagreementParams {
        estimator: "hajek",
        lambda: 100_000,
        min_inclusion_fraction_ppm: 1_000,
        min_replicates: 1,
    };
    let candidates: Vec<(String, String)> = (0..6)
        .map(|i| (format!("plan:{i}"), format!("cell:{}", i % 3)))
        .collect();
    let stats = |cell: &str| -> hh_lab::adaptive::CellStats {
        match cell {
            "cell:0" => hh_lab::adaptive::CellStats {
                completed: 4,
                scored: 1,
            },
            "cell:1" => hh_lab::adaptive::CellStats {
                completed: 4,
                scored: 3,
            },
            _ => hh_lab::adaptive::CellStats::default(),
        }
    };
    let a =
        hh_lab::adaptive::disagreement_weighted_allocate(&params, &candidates, &stats, "seed", 0)
            .unwrap();
    let b =
        hh_lab::adaptive::disagreement_weighted_allocate(&params, &candidates, &stats, "seed", 0)
            .unwrap();
    assert_eq!(a, b, "the draw must replay deterministically");
    assert_eq!(a.strategy, "disagreement_weighted");
    assert_eq!(a.estimator, "hajek");
    assert_eq!(a.per_plan.len(), candidates.len());
    assert!(a.per_plan.values().all(|p| *p >= 1_000));
}

#[test]
fn successive_halving_brackets_prune() {
    let params = hh_lab::adaptive::HalvingParams {
        eta: 2,
        min_budget: 1_000,
        brackets: 3,
    };
    let candidates: Vec<(String, String)> = (0..8)
        .map(|i| (format!("plan:{i}"), format!("cell:{i}")))
        .collect();
    let stats = |cell: &str| -> hh_lab::adaptive::CellStats {
        let i: u64 = cell["cell:".len()..].parse().unwrap();
        hh_lab::adaptive::CellStats {
            completed: 8,
            scored: (8 - i) as u32,
        }
    };
    let b0 = hh_lab::adaptive::successive_halving_allocate(&params, &candidates, &stats, "seed", 0)
        .unwrap();
    // Bracket 0's live set is the whole pool (nothing dropped yet);
    // the record's `per_plan` carries each plan's last-surviving bracket.
    assert_eq!(b0.kept.len(), 8);
    assert_eq!(b0.dropped.len(), 0);
    // Bracket 1 keeps the top ⌈8/2⌉ = 4 ranks… whose last-bracket ≥ 1 is
    // the top ⌈8/4⌉ = 2 (rank < ⌈8/2²⌉).
    let b1 = hh_lab::adaptive::successive_halving_allocate(&params, &candidates, &stats, "seed", 1)
        .unwrap();
    assert_eq!(b1.kept.len(), 2);
    assert_eq!(b1.dropped.len(), 6);
    let b0b =
        hh_lab::adaptive::successive_halving_allocate(&params, &candidates, &stats, "seed", 0)
            .unwrap();
    assert_eq!(b0, b0b);
    // Bracket 2 — the live set is the top ⌈8/8⌉ = 1.
    let b2 = hh_lab::adaptive::successive_halving_allocate(&params, &candidates, &stats, "seed", 2)
        .unwrap();
    assert_eq!(b2.kept.len(), 1);
    assert_eq!(b2.kept[0], "plan:0");
    assert_eq!(b2.eta, 2);
    assert_eq!(*b2.per_plan.get("plan:0").unwrap(), 2);
    assert_eq!(*b2.per_plan.get("plan:7").unwrap(), 0);
}

// ── 9. The predictor compute_policy variant (R-2.6.4⁴) ───────────────

#[test]
fn predictor_is_admitted_and_inert_without_a_budget_slice() {
    // The registered variant resolves — no more VariantNotAdmitted.
    let p =
        hh_control::compute::policy_for("predictor").unwrap_or_else(|e| panic!("predictor: {e}"));
    assert_eq!(p.variant_ref(), "predictor");
    let caps = p.capabilities();
    assert_eq!(caps.estimator_ref.as_deref(), Some("predictor"));
    assert!(caps.deterministic);
    assert!(!caps.makes_model_calls);
    // Inert without a declared `SearchBudgetRecord` slice — a bind lands
    // `Unchanged`, never a stop.
    let ctx = predictor_ctx();
    let decision = predictor_decision();
    let out = p.bind(&decision, &ctx).unwrap();
    assert!(matches!(
        out,
        hh_control::compute::BindOutcome::Unchanged { .. }
    ));
    // With the slice declared, a cold-start bind runs the rules fallback.
    use hh_control::compute::ComputePolicy;
    let p2 = hh_control::compute::PredictorPolicy {
        config: hh_control::compute::PredictorConfig {
            search_budget_ref: Some("sbr:predictor".into()),
            ..hh_control::compute::PredictorConfig::default()
        },
    };
    let out = p2.bind(&decision, &ctx).unwrap();
    let record = match out {
        hh_control::compute::BindOutcome::Unchanged { record } => record,
        hh_control::compute::BindOutcome::Bound { record, .. } => record,
    };
    // Cold start — the rules fallback is stamped, never silent.
    assert!(record
        .rules_fired
        .iter()
        .any(|r| r == "predictor.cold_start"));
    drop(ctx);
    // Tightening-only scheduling targets.
    assert!(matches!(
        hh_control::compute::classify_compute_rule_delta("budget_share_cap_ppm", 500_000, 600_000),
        hh_control::compute::BudgetDelta::Loosening
    ));
    assert!(matches!(
        hh_control::compute::classify_compute_rule_delta("drift_reset_ppm", 300_000, 200_000),
        hh_control::compute::BudgetDelta::Loosening
    ));
    assert!(hh_control::compute::compute_rule_delta_admissible(
        "budget_share_cap_ppm",
        600_000,
        500_000
    )
    .is_ok());
}

/// A minimal `ComputeContext` (typed facts only — B-5).
fn predictor_ctx() -> hh_control::compute::ComputeContext {
    use hh_control::compute::*;
    let mut remaining = hh_budget::quantity::ResourceVector::zero();
    remaining.add(
        hh_ontology::dimensions::DimensionId::TokensOutputVisible,
        1_000_000,
    );
    remaining.add(hh_ontology::dimensions::DimensionId::ModelCalls, 8);
    ComputeContext {
        budget: BudgetView {
            remaining,
            reserved: hh_budget::quantity::ResourceVector::zero(),
            live_fan_out: 0,
            fan_out_cap: 8,
            delegation_depth: 0,
            delegation_depth_cap: 4,
            occupancy_ppm: 100_000,
        },
        declared_parallel_steps: 0,
        subagent_task_targets: vec![],
        ensemble: None,
        samples: vec![],
        verifier: None,
        validator_streaks: ValidatorStreaks::default(),
        profile_capabilities: Json::obj([]),
        role_table: Json::obj([]),
        placements: vec![],
        priors: vec![],
        cost_model: None,
        health: HealthView::Ok,
        task_value: None,
        context_label: None,
        profile_ref: Some("profile:test".into()),
        snapshot_fingerprint: None,
        task_class: None,
        now_seq: 0,
    }
}

fn predictor_decision() -> hh_control::vocab::ControlDecision {
    hh_control::vocab::ControlDecision {
        stamp: hh_control::vocab::DecisionStamp {
            decision_point: hh_ontology::control::DecisionPoint::Plan,
            owner: hh_ontology::control::Owner::Model,
            rationale_ref: None,
        },
        kind: hh_control::vocab::DecisionKind::Propose {
            decision_point: hh_ontology::control::DecisionPoint::Plan,
            context_request: Json::obj([]),
            expected_output: hh_control::vocab::ExpectedOutput::Free,
        },
    }
}

// ── 10. Stage drivers (the s6_1a shapes — same S2→S8 evidence) ───────

fn hypothesis() -> FailureHypothesis {
    FailureHypothesis {
        kind: "failure".into(),
        evidence_refs: vec!["ev:1".into()],
        predicted: PredictedEffect {
            deltas: vec![PredictedDelta {
                metric: "metric:success".into(),
                direction: "increase".into(),
            }],
            affected_task_ids: vec!["task:a".into()],
            model_scope: "same_snapshot".into(),
            horizon: None,
        },
        semantic_op_targets: vec![],
    }
}

fn screen_ok() -> ScreenReport {
    ScreenReport {
        counterexample_set_ref: "ce:set".into(),
        observations: 10,
        flips_in_direction: 6,
        replicate_count: 1,
        split_labels_used: vec![SplitLabel::Dev],
        judge_only: false,
        selector_ref: None,
        honeypots: 0,
    }
}

fn stage_arm(id: &str, lid: &str, mode: MatchMode) -> ArmSpec {
    ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("model".to_string(), lid.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        inference_budget: None,
        match_spec: Some(MatchSpec {
            mode,
            ..MatchSpec::matched_cap(&[])
        }),
        artifact_ref: hh_ontology::config::Ref::new("def:x", "sha256:ee55"),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    }
}

fn stage_spec(eng: &EvolutionCampaign, kind: ExperimentKind, split: SplitLabel) -> ExperimentSpec {
    stage_spec_mode(eng, kind, split, MatchMode::MatchedTotal)
}

fn stage_spec_mode(
    eng: &EvolutionCampaign,
    kind: ExperimentKind,
    split: SplitLabel,
    mode: MatchMode,
) -> ExperimentSpec {
    let seed_policy = SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind,
        design: Design {
            id: "design:evo-stage".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: SeedPolicy {
                ..seed_policy.clone()
            },
            held_out_split_ref: Some("split:held_out".into()),
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "the candidate's predicted delta holds".into(),
                primary_metrics: vec!["task_success".into()],
                equivalence_margin: None,
                min_n: 1,
                analysis_plan_ref: "analysis:plan".into(),
                task_split_hash: "sha256:cc33".into(),
                interactions: vec![],
            },
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("base"), level("cand")],
        }],
        arms: vec![stage_arm("a", "base", mode), stage_arm("b", "cand", mode)],
        suite: SuiteBinding {
            suite_ref: "suite:test".into(),
            split_labels_used: vec![split],
            split_assignment_ref: Some("test-split".into()),
        },
        replicates_per_cell: 2,
        seed_policy,
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 2,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 1,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::from([(
            "evolution_campaign".to_string(),
            Json::str(&eng.spec.campaign_id),
        )]),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn comparison() -> ComparisonReport {
    ComparisonReport {
        arm_a: "a".into(),
        arm_b: "b".into(),
        metric: "task_success".into(),
        pairing: "by_task".into(),
        paired_effect: PairedEffect {
            point: Some(Json::str("0.05")),
            interval: Some(Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))])),
            method: IntervalMethod::ClusteredClt,
        },
        per_task_effects_ref: Some("effects:1".into()),
        budget_match: BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 50_000,
            status: BudgetMatchStatus::Matched,
        },
        benefit_kind: BenefitKind::SearchTimeBenefit,
        held_out: false,
        estimated: None,
        test: TestRecord {
            kind: TestKind::PermutationSignflip,
        },
        sign_profile: SignProfile {
            helped: 3,
            hurt: 0,
            unchanged: 1,
        },
        tail_effects: TailEffects {
            p50: Json::str("0.04"),
            p95: Json::str("0.12"),
            max: Json::str("0.30"),
        },
        outcome_bounds: OutcomeBounds {
            lower: Json::str("0.0"),
            upper: Json::str("1.0"),
            verdict: OutcomeBoundsVerdict::Robust,
        },
        multiplicity: Multiplicity {
            family_size: 2,
            adjusted: "holm".into(),
            raw_ppm: None,
            adjusted_ppm: None,
            label: None,
        },
        label: ReportLabelKind::Headlined,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
    }
}

fn deposit_comparison(docs: &LabDocs, name: &str) -> String {
    docs.put_named(doc_kind::REPORT, name, &comparison().to_json())
        .unwrap();
    name.to_string()
}

fn compat_record() -> Json {
    CompatibilityRecord {
        snapshot_ref: "snap:1".into(),
        definition_semantic_id: "def:base".into(),
        profile_semantic_id: "sha256:profile".into(),
        status: CompatibilityStatus::Verified,
        evidence_ref: Some("ev:compat".into()),
        regression_suite_ref: Some("suite:regression".into()),
        created_at: 1,
        expiry_condition: None::<ExpiryCondition>,
        provenance: prov(20),
    }
    .to_json()
}

fn security_ok() -> SecurityInvarianceReport {
    SecurityInvarianceReport {
        placement: "subprocess_confined".into(),
        has_interface: true,
        policy_leaves: BTreeMap::from([("policy:test".into(), "narrowing".into())]),
        dynamic_veto_table: BTreeMap::from([("metric:safety".to_string(), false)]),
    }
}

fn acceptance(pass: bool) -> EvolutionAcceptanceReport {
    EvolutionAcceptanceReport {
        items: (1..=7)
            .map(|i| AcceptanceItem {
                item: i,
                status: if pass {
                    ItemStatus::Pass
                } else {
                    ItemStatus::NotApplicable {
                        reason: "not shown".into(),
                    }
                },
                evidence_refs: vec![format!("ev:{i}")],
            })
            .collect(),
        attribution_granularity: "designed_ablation".into(),
        portability_label: PortabilityLabel::Reported,
    }
}

fn human_seal() -> ProvenanceRecord {
    let mut r = ProvenanceRecord::minted(
        Origin::human("test:sealer", HumanRole::Principal),
        PersistenceScope::Definition,
        99,
    );
    r.authority = AuthorityClass::Definition;
    r
}

/// Drive `cid` from `classified` through `sealed_candidate` (S2…S8 —
/// the s6_1a happy-path shape; the S4 search-budget doc is deposited
/// with the campaign family's facet set).
fn drive_to_sealed(store: &mut Store, docs: &LabDocs, eng: &mut EvolutionCampaign, cid: &str) {
    eng.hypothesize(store, cid, &hypothesis())
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    eng.screen(store, cid, &screen_ok())
        .unwrap_or_else(|e| panic!("screen: {e}"));
    let search = stage_spec(eng, ExperimentKind::Comparative, SplitLabel::Search);
    docs.put_spec(&search).unwrap();
    docs.put_named(
        doc_kind::SEARCH_BUDGET,
        "budget:search",
        &search_budget_doc(eng.spec.proposer_family != "human").to_json(),
    )
    .unwrap();
    let rep = deposit_comparison(docs, "rep:s4");
    eng.matched_eval(store, cid, &search.experiment_id, &rep)
        .unwrap_or_else(|e| panic!("matched_eval: {e}"));
    let held = stage_spec(eng, ExperimentKind::Comparative, SplitLabel::HeldOut);
    docs.put_spec(&held).unwrap();
    docs.put_named(doc_kind::REPORT, "rep:held", &{
        let mut c = comparison();
        c.benefit_kind = BenefitKind::ArtifactBenefit;
        c.held_out = true;
        c.to_json()
    })
    .unwrap();
    eng.held_out_eval(
        store,
        cid,
        &held.experiment_id,
        "rep:held",
        &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
        &BTreeMap::from([("metric:safety".to_string(), false)]),
    )
    .unwrap_or_else(|e| panic!("held_out_eval: {e}"));
    eng.transfer(
        store,
        cid,
        &[TransferRow {
            environment_family: EnvironmentFamily::StructuredTool,
            status: "measured".into(),
            point: Some(Json::str("0.03")),
            interval: Some(Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(7))])),
            method: "clustered_clt".into(),
            held_out: true,
        }],
        &[compat_record()],
    )
    .unwrap_or_else(|e| panic!("transfer: {e}"));
    eng.security_check(store, cid, &security_ok())
        .unwrap_or_else(|e| panic!("security_check: {e}"));
    eng.seal(store, cid, &acceptance(true), &human_seal())
        .unwrap_or_else(|e| panic!("seal: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("sealed_candidate"));
}

fn drive_to_active(store: &mut Store, docs: &LabDocs, eng: &mut EvolutionCampaign, cid: &str) {
    drive_to_sealed(store, docs, eng, cid);
    eng.canary(store, cid, "shadow", "int:1").unwrap();
    eng.canary_settle(store, cid, "clean", "", &[]).unwrap();
}
