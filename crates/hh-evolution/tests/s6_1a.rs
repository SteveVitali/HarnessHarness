//! The S6.1a battery (§05h R-2.9.5) — the evolution campaign driver over
//! a real `Store` + `LabDocs`: the closed state machine, every gated
//! refusal landing a durable `rejected` row, the `evolution_link`
//! obligation fields, the campaign lifecycle rows, and the fold's
//! restart equality (`ensure` rebuilds the same view).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::matchspec::{MatchMode, MatchSpec};
use hh_evolution::campaign::{doc_kind, EvolutionCampaign};
use hh_evolution::errors::{EvolutionError, Refusal};
use hh_evolution::records::{
    AcceptanceItem, CandidateProposal, CorpusSpec, EvolutionAcceptanceReport,
    EvolutionCampaignSpec, FailureHypothesis, ItemStatus, PortabilityLabel, PredictedDelta,
    PredictedEffect, ScreenReport, SecurityInvarianceReport, SlotAllocation, StopRule, TransferRow,
};
use hh_evolution::state::{self, CandidateState};
use hh_evolution::view::CampaignView;
use hh_experiment::docs::LabDocs;
use hh_hir::diff::{self, DiffDerivation, HirDiff};
use hh_hir::document::{HirDocument, Node};
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

// ── fixture plumbing ────────────────────────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-evolution-test-{}-{}", tag, std::process::id()));
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

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn rule_node(id: &str, seq: u64, trigger: Json) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            seq,
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
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions,
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            seq,
        ),
        id,
    )
}

fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
    sid(
        node(
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
            seq,
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
        node(
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
            seq,
        ),
        id,
    )
}

/// The minimal valid base document (the hh-hir fixture's 4-node shape).
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

/// A candidate target — the same doc with the rule's trigger edited.
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

/// The human-proposed diff over `base → target`.
fn candidate_diff(base: &HirDocument, target: &HirDocument) -> HirDiff {
    match diff::diff(
        base,
        target,
        prov(10),
        DiffDerivation {
            hypothesis: None,
            trajectories: vec![],
            candidate_id: None,
        },
    ) {
        Ok(d) => d,
        Err(errs) => panic!("fixture diff failed: {errs:?}"),
    }
}

/// The pinned split assignment deposited under `doc_kind::SPLIT` (the L3
/// pin `open` resolves).
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
        target_classes: Vec::new(),
        hosted_coordinates: Vec::new(),
        hosted_descriptor_refs: Vec::new(),
        authority_cap: None,
        rollout_policy: None,
        judge_policy: None,
    }
}

fn open(tag: &str) -> (PathBuf, ManualClock, Store, LabDocs, EvolutionCampaign) {
    let (root, clock, mut store, docs) = rig(tag);
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::Search)],
    );
    let (_, eng) =
        EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, spec(&split_ref))
            .unwrap_or_else(|e| panic!("campaign open: {e}"));
    (root, clock, store, docs, eng)
}

fn refusal(e: &EvolutionError) -> Refusal {
    match e {
        EvolutionError::Refusal(r) => r.clone(),
        other => panic!("expected a Refusal, got {other}"),
    }
}

fn propose_ok(
    store: &mut Store,
    eng: &mut EvolutionCampaign,
    base: &HirDocument,
    target: &HirDocument,
) -> String {
    let d = candidate_diff(base, target);
    eng.propose(
        store,
        &CandidateProposal {
            base_ref: "def:base".into(),
            diff: d,
            slot: "control_strategy".into(),
            hypothesis: None,
            install: None,
            coordinate_values: Default::default(),
        },
        base,
    )
    .unwrap_or_else(|e| panic!("propose: {e}"))
}

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
        reference_trajectories: Vec::new(),
        attribution_ref: None,
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
        label: None,
    }
}

fn human_seal() -> ProvenanceRecord {
    let mut r = ProvenanceRecord::minted(
        Origin::human("test:sealer", HumanRole::Principal),
        PersistenceScope::Definition,
        99,
    );
    // `authority = definition` is minted only at the seal/pin boundary —
    // the fixture pins it directly (the same fixture convention the
    // hh-env tests use).
    r.authority = AuthorityClass::Definition;
    r
}

fn st(s: &str) -> CandidateState {
    match s {
        "proposed" => CandidateState::Proposed,
        "classified" => CandidateState::Classified,
        "hypothesized" => CandidateState::Hypothesized,
        "screened" => CandidateState::Screened,
        "searched" => CandidateState::Searched,
        "validated" => CandidateState::Validated,
        "transferred" => CandidateState::Transferred,
        "security_checked" => CandidateState::SecurityChecked,
        "sealed_candidate" => CandidateState::SealedCandidate,
        "canary" => CandidateState::Canary,
        "active" => CandidateState::Active,
        "expiring" => CandidateState::Expiring,
        "revalidated" => CandidateState::Revalidated,
        "retired" => CandidateState::Retired,
        "rejected" => CandidateState::Rejected {
            stage: "x".into(),
            code: "x".into(),
            report_ref: None,
        },
        "withdrawn" => CandidateState::Withdrawn { by: "x".into() },
        "reverted" => CandidateState::Reverted {
            from: "canary".into(),
            reason: "x".into(),
        },
        other => panic!("unknown state {other}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. The closed state machine (pure).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn state_machine_walks_s0_to_s10() {
    let mut cur = CandidateState::Proposed;
    for to in [
        "classified",
        "hypothesized",
        "screened",
        "searched",
        "validated",
        "transferred",
        "security_checked",
        "sealed_candidate",
        "canary",
        "active",
        "expiring",
        "retired",
    ] {
        assert!(state::allowed(&cur, to), "{cur:?} -> {to} must be admitted");
        cur = st(to);
    }
}

#[test]
fn state_machine_refuses_skips_and_terminal_moves() {
    for (from, to) in [
        ("proposed", "sealed_candidate"),
        ("classified", "validated"),
        ("canary", "hypothesized"),
        ("retired", "active"),
        ("rejected", "active"),
        ("withdrawn", "proposed"),
        ("reverted", "canary"),
    ] {
        let f = st(from);
        assert!(!state::allowed(&f, to), "{from} -> {to} must refuse");
    }
    // The failure legs.
    assert!(state::allowed(&st("revalidated"), "active"));
    assert!(state::allowed(&st("expiring"), "revalidated"));
    assert!(state::allowed(&st("expiring"), "retired"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Campaign open — spec validation, the L3 pin, the durable row.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn open_mints_campaign_opened_and_is_idempotent() {
    let (_root, clock, mut store, docs, eng) = open("open");
    // The `campaign.opened` row landed on the campaign run.
    let events = store.events(&eng.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "measurement.evolution.campaign.opened"));
    assert_eq!(eng.view.status, "open");
    // Re-open with the same spec is idempotent (content-derived id);
    // `open` already deposited `test-split`. The first writer's lease
    // must expire before `ensure`'s re-acquire fences it.
    clock.advance(120_000);
    let s = spec("test-split");
    let run_id = format!("evo-{}", s.derive_id().replace(':', "-"));
    let (rid2, _eng2) = EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s).unwrap();
    assert_eq!(rid2, run_id);
    assert_eq!(eng.run_id, rid2);
}

#[test]
fn open_refuses_missing_split_pin() {
    let (_root, _clock, mut store, docs) = rig("nosplit");
    let e = match EvolutionCampaign::open(&mut store, docs, "holder", 60_000, spec("no-such-split"))
    {
        Err(e) => e,
        Ok(_) => panic!("a missing split pin must refuse"),
    };
    assert!(matches!(refusal(&e), Refusal::EvidenceStale { .. }));
}

#[test]
fn open_refuses_non_human_protocol() {
    let (_root, _clock, mut store, docs) = rig("proto");
    let split_ref = deposit_split(&docs, &[]);
    let mut s = spec(&split_ref);
    s.protocol = "automated".into();
    let e = match EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s) {
        Err(e) => e,
        Ok(_) => panic!("a non-human_proposed protocol must refuse"),
    };
    assert!(matches!(refusal(&e), Refusal::SchemaViolation { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. S0/S1 propose — the intake + classification gates.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn propose_registers_proposed_then_classified() {
    let (_root, _clock, mut store, _docs, mut eng) = open("propose");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    let rec = eng.view.candidate(&cid).unwrap();
    assert_eq!(rec.state, "classified");
    assert_eq!(
        rec.history
            .iter()
            .map(|t| t.to.as_str())
            .collect::<Vec<_>>(),
        vec!["proposed", "classified"]
    );
}

#[test]
fn propose_refuses_self_modification() {
    let (_root, _clock, mut store, _docs, mut eng) = open("selfmod");
    let base = base_doc();
    let target = target_doc();
    let d = candidate_diff(&base, &target);
    let e = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:evolution-service".into(), // the service's own def
                diff: d,
                slot: "control_strategy".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &base,
        )
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SelfModificationRefused { .. }
    ));
    // The refusal minted a `rejected` row carrying the code.
    let rej = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.evolution.candidate.transitioned"
                && e.payload.get("to").and_then(Json::as_str) == Some("rejected")
        })
        .expect("rejected row");
    assert_eq!(
        rej.payload.get("code").and_then(Json::as_str),
        Some("self_modification_refused")
    );
}

#[test]
fn propose_refuses_excluded_target() {
    let (_root, _clock, mut store, docs) = rig("excl");
    let split_ref = deposit_split(&docs, &[]);
    let mut s = spec(&split_ref);
    s.exclusion_targets = vec!["test:rule".into()];
    let (_, mut eng) = EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s).unwrap();
    let base = base_doc();
    let target = target_doc();
    let d = candidate_diff(&base, &target);
    let e = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:base".into(),
                diff: d,
                slot: "control_strategy".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &base,
        )
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::ExcludedTarget { .. }));
}

#[test]
fn propose_refuses_duplicate_candidate() {
    let (_root, _clock, mut store, _docs, mut eng) = open("dup");
    let base = base_doc();
    let target = target_doc();
    let d = candidate_diff(&base, &target);
    let cid = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:base".into(),
                diff: d.clone(),
                slot: "control_strategy".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &base,
        )
        .unwrap();
    let e = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:base".into(),
                diff: d,
                slot: "control_strategy".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &base,
        )
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::DuplicateCandidate { ref candidate_id } if *candidate_id == cid
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. S2 hypothesize — falsifiability, evidence resolution, split labels.
// ═══════════════════════════════════════════════════════════════════════════

fn classified_candidate(
    tag: &str,
) -> (
    PathBuf,
    ManualClock,
    Store,
    LabDocs,
    EvolutionCampaign,
    String,
) {
    let (root, clock, mut store, docs, mut eng) = open(tag);
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    (root, clock, store, docs, eng, cid)
}

#[test]
fn hypothesize_binds_and_mints_the_evolution_link_fields() {
    let (_root, _clock, mut store, _docs, mut eng, cid) = classified_candidate("hyp");
    let href = eng
        .hypothesize(&mut store, &cid, &hypothesis())
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("hypothesized"));
    // The `transitioned{to: hypothesized}` row carries the obligation's
    // link fields — `hypothesis_ref` + a non-empty `evidence_refs[]`.
    let row = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.evolution.candidate.transitioned"
                && e.payload.get("to").and_then(Json::as_str) == Some("hypothesized")
        })
        .expect("hypothesized row");
    assert_eq!(
        row.payload.get("hypothesis_ref").and_then(Json::as_str),
        Some(href.as_str())
    );
    match row.payload.get("evidence_refs") {
        Some(Json::Arr(refs)) => assert!(!refs.is_empty()),
        other => panic!("evidence_refs missing: {other:?}"),
    }
}

#[test]
fn hypothesize_refuses_unfalsifiable_and_unknown_metric() {
    // Each refusal is terminal (`rejected`) — every leg gets a fresh
    // classified candidate.
    for (i, mutate) in [
        |h: &mut FailureHypothesis| h.evidence_refs = vec![],
        |h: &mut FailureHypothesis| h.predicted.deltas = vec![],
    ]
    .into_iter()
    .enumerate()
    {
        let (_root, _clock, mut store, _docs, mut eng, cid) =
            classified_candidate(&format!("hypref{i}"));
        let mut h = hypothesis();
        mutate(&mut h);
        let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
        assert!(matches!(
            refusal(&e),
            Refusal::HypothesisUnfalsifiable { .. }
        ));
        assert_eq!(eng.view.state_of(&cid), Some("rejected"));
    }
    // unresolvable evidence ref.
    let (_root, _clock, mut store, _docs, mut eng, cid) = classified_candidate("hypref-ure");
    let mut h = hypothesis();
    h.evidence_refs = vec!["ev:nope".into()];
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::UnresolvableEvidence { .. }));
    // unknown metric.
    let (_root, _clock, mut store, _docs, mut eng, cid) = classified_candidate("hypref-um");
    let mut h = hypothesis();
    h.predicted.deltas[0].metric = "metric:unregistered".into();
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::UnknownMetric { .. }));
}

#[test]
fn hypothesize_refuses_held_out_task_leak() {
    // Pin `task:a` to `held_out` — the hypothesis naming it is LeakedSplit.
    let (_root, _clock, mut store, docs) = rig("leak");
    let split_ref = deposit_split(
        &docs,
        &[
            ("task:a", SplitLabel::HeldOut),
            ("task:b", SplitLabel::Search),
        ],
    );
    let (_, mut eng) =
        EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, spec(&split_ref))
            .unwrap();
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    let e = eng
        .hypothesize(&mut store, &cid, &hypothesis())
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::LeakedSplit { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. S3 screen — judge-only, replicates, flip-share, split labels.
// ═══════════════════════════════════════════════════════════════════════════

fn hypothesized_candidate(
    tag: &str,
) -> (
    PathBuf,
    ManualClock,
    Store,
    LabDocs,
    EvolutionCampaign,
    String,
) {
    let (root, clock, mut store, docs, mut eng, cid) = classified_candidate(tag);
    eng.hypothesize(&mut store, &cid, &hypothesis())
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    (root, clock, store, docs, eng, cid)
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

#[test]
fn screen_passes_and_stamps_report_ref() {
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("screen");
    let rref = eng
        .screen(&mut store, &cid, &screen_ok())
        .unwrap_or_else(|e| panic!("screen: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("screened"));
    assert_eq!(eng.view.candidate(&cid).unwrap().reports["S3"], rref);
}

#[test]
fn screen_refusals() {
    // judge_only → JudgeOnlyAcceptance.
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("judge");
    let mut r = screen_ok();
    r.judge_only = true;
    let e = eng.screen(&mut store, &cid, &r).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::JudgeOnlyAcceptance { .. }));
    assert_eq!(eng.view.state_of(&cid), Some("rejected"));

    // held_out label consulted → LeakedSplit.
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("sleak");
    let mut r = screen_ok();
    r.split_labels_used = vec![SplitLabel::HeldOut];
    let e = eng.screen(&mut store, &cid, &r).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::LeakedSplit { .. }));

    // below the replicate floor → InsufficientReplicates.
    let (_root, _clock, mut store, docs) = rig("reps");
    let split_ref = deposit_split(&docs, &[("task:a", SplitLabel::Dev)]);
    let mut s = spec(&split_ref);
    s.min_replicates = 5;
    let (_, mut eng) =
        EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s).unwrap();
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let e = eng.screen(&mut store, &cid, &screen_ok()).unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::InsufficientReplicates { .. }
    ));

    // flip share below the floor → PredictionFalsified.
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("flips");
    let mut r = screen_ok();
    r.flips_in_direction = 0;
    let e = eng.screen(&mut store, &cid, &r).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::PredictionFalsified { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. S4 matched_eval — the M3 gate against the registered spec.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn matched_eval_refuses_unregistered_specs() {
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("m3");
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    // Unregistered experiment id → SpecNotRegistered.
    let e = eng
        .matched_eval(&mut store, &cid, "exp:never-registered", "report:x")
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::SpecNotRegistered { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. S7/S8 — state-boundary enforcement and the typed seal refusals.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn seal_refuses_out_of_order_and_security_check_too() {
    let (_root, _clock, mut store, _docs, mut eng, cid) = hypothesized_candidate("seal");
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    // Without S4–S7 evidence the seal refuses at the state boundary
    // (P-1: no stage runs before its predecessor's gate passed).
    let e = eng
        .seal(&mut store, &cid, &acceptance(true), &human_seal())
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::IllegalTransition { .. }));
    // Security check on a `screened` candidate is likewise refused.
    let report = SecurityInvarianceReport {
        placement: "subprocess_confined".into(),
        has_interface: true,
        policy_leaves: BTreeMap::new(),
        dynamic_veto_table: BTreeMap::new(),
    };
    let e = eng.security_check(&mut store, &cid, &report).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::IllegalTransition { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. The campaign lifecycle + the fold's restart equality.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stop_and_close_mint_the_lifecycle_rows() {
    let (_root, _clock, mut store, _docs, mut eng) = open("lifecycle");
    eng.stop(&mut store, "stagnation").unwrap();
    assert_eq!(eng.view.status, "stopped");
    // A stopped campaign refuses stage ops.
    let base = base_doc();
    let target = target_doc();
    let d = candidate_diff(&base, &target);
    let e = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: "def:base".into(),
                diff: d,
                slot: "s".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &base,
        )
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::CampaignNotOpen { .. }));
    eng.close(&mut store).unwrap();
    assert_eq!(eng.view.status, "closed");
    let events = store.events(&eng.run_id).unwrap();
    for class in [
        "measurement.evolution.campaign.opened",
        "measurement.evolution.campaign.stopped",
        "measurement.evolution.campaign.closed",
    ] {
        assert!(events.iter().any(|e| e.class == class), "{class}");
    }
}

#[test]
fn ensure_rebuilds_the_same_view() {
    let (_root, clock, mut store, docs, mut eng) = open("ensure");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let run_id = eng.run_id.clone();
    // Drop the engine; the stale writer lease expires, then `ensure`'s
    // re-acquire fences it (audited) and folds the durable prefix.
    drop(eng);
    clock.advance(120_000);
    let eng2 = EvolutionCampaign::ensure(&mut store, docs, &run_id, "holder", 60_000)
        .unwrap_or_else(|e| panic!("ensure: {e}"));
    assert_eq!(eng2.view.state_of(&cid), Some("hypothesized"));
    assert_eq!(eng2.view.n_candidates_registered(), 1);
}

#[test]
fn candidate_view_prefixes_are_prefixes() {
    let (_root, _clock, mut store, _docs, mut eng) = open("prefix");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    let events = store.events(&eng.run_id).unwrap().to_vec();
    // At the seq of the `proposed` row the candidate is `proposed`.
    let proposed_seq = events
        .iter()
        .find(|e| {
            e.class == "measurement.evolution.candidate.transitioned"
                && e.payload.get("to").and_then(Json::as_str) == Some("proposed")
        })
        .map(|e| e.seq)
        .unwrap();
    let v = eng.candidate_view(&store, Some(proposed_seq)).unwrap();
    assert_eq!(v.state_of(&cid), Some("proposed"));
    let v = eng.candidate_view(&store, None).unwrap();
    assert_eq!(v.state_of(&cid), Some("hypothesized"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. `check_publish_namespace` — the `exp/` publishing bound
//    (AC-R-2.12.2-14).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn publish_namespace_is_exp_scoped() {
    let (_root, _clock, _store, _docs, eng) = open("ns");
    let expected = format!("exp/{}", eng.spec.campaign_id);
    eng.check_publish_namespace(&expected).unwrap();
    for ns in ["stable/x", "exp/other", "exp", ""] {
        let e = eng.check_publish_namespace(ns).unwrap_err();
        assert!(
            matches!(refusal(&e), Refusal::NamespaceForbidden { .. }),
            "{ns}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Spec/record round-trips (schema symmetry — CC7).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn spec_codec_round_trips() {
    let s = spec("split:ref");
    let j = s.to_json();
    let back = EvolutionCampaignSpec::from_json(&j).unwrap();
    assert_eq!(back, s);
}

#[test]
fn acceptance_report_check_enforces_the_gate_items() {
    let r = acceptance(true);
    assert!(r.check().is_empty(), "all-pass report must pass");
    // Item 1 failing → AcceptanceIncomplete{1}.
    let mut r = acceptance(true);
    r.items[0].status = ItemStatus::Fail;
    assert_eq!(r.check(), vec![1]);
    // Item 4 `n/a` with a declared granularity is admissible.
    let mut r = acceptance(true);
    r.items[3].status = ItemStatus::NotApplicable {
        reason: "no_boundary".into(),
    };
    assert!(r.check().is_empty());
    // Item 7 must pass outright — n/a does not carry.
    let mut r = acceptance(true);
    r.items[6].status = ItemStatus::NotApplicable {
        reason: "observability".into(),
    };
    assert!(r.check().contains(&7));
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. The view's fold is deterministic.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn view_fold_is_deterministic() {
    let (_root, _clock, mut store, _docs, mut eng) = open("fold");
    let base = base_doc();
    let target = target_doc();
    let _cid = propose_ok(&mut store, &mut eng, &base, &target);
    let events = store.events(&eng.run_id).unwrap().to_vec();
    let a = CampaignView::fold(&events).unwrap();
    let b = CampaignView::fold(&events).unwrap();
    assert_eq!(a.status, b.status);
    assert_eq!(a.candidates.len(), b.candidates.len());
    for (cid, rec) in &a.candidates {
        assert_eq!(b.candidates[cid].state, rec.state);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 12. `RetirementBatch` — the multi-rule removal kind (R-2.10.3⁴).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn retirement_batch_kind_parses_and_is_retirement() {
    use hh_lab::experiment::ExperimentKind;
    assert_eq!(
        ExperimentKind::parse("retirement_batch"),
        Some(ExperimentKind::RetirementBatch)
    );
    assert_eq!(ExperimentKind::RetirementBatch.name(), "retirement_batch");
    assert!(ExperimentKind::RetirementBatch.is_retirement());
    assert!(ExperimentKind::Retirement.is_retirement());
    assert!(!ExperimentKind::Comparative.is_retirement());
    // requires_match/pre_registration like every non-exploratory kind.
    assert!(ExperimentKind::RetirementBatch.requires_match());
    assert!(ExperimentKind::RetirementBatch.requires_pre_registration());
}

// ═══════════════════════════════════════════════════════════════════════════
// 13. Context — AC-R-2.4.5-9's `PromotionRefused` (the induced-procedure
//     promotion gate; the hh-context half of this ticket).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn evolution_origin_promotion_refused_without_human_and_validity() {
    use hh_context::lifecycle::{promote, LifecycleError};
    use hh_context::memory::{MemoryDraft, MemoryStore, WriteContext};
    use hh_context::vocab::{MemoryContent, MemoryKind};
    use hh_provenance::label::Label;

    let mut store = MemoryStore::new("ms");
    let ctx = WriteContext {
        context_label: Label::top(),
        lease_generation: store.lease(PersistenceScope::Run),
        at_seq: 1,
        run_id: "run1".into(),
    };
    // An evolution-origin version (the induced record).
    let evo_prov = ProvenanceRecord::minted(
        Origin::evolution("cand:x", "hyp:x"),
        PersistenceScope::Run,
        1,
    );
    let out = store
        .put(
            MemoryDraft {
                kind: MemoryKind::Fact,
                subject_key: None,
                content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
                    "induced",
                    "owner",
                    ProvenanceRecord::kernel("kernel:mem", 0),
                ))),
                contract: None,
                scope: PersistenceScope::Run,
                declared_inputs: vec![],
                justifications: vec![],
                supersedes: None,
                validity: None,
                provenance: Some(evo_prov),
                semantic_id: None,
                validator_endorsed: false,
            },
            &ctx,
        )
        .expect("put");
    let vid = out.version.version_id.clone();
    // A kernel reviewer is not a human promotion act → not_human.
    let kernel = ProvenanceRecord::minted(
        Origin::kernel("test:kernel"),
        PersistenceScope::Definition,
        2,
    );
    let e = promote(
        &mut store,
        &vid,
        &kernel,
        PersistenceScope::Project,
        AuthorityClass::Principal,
        &ctx,
    )
    .unwrap_err();
    match e {
        LifecycleError::PromotionRefused { reason, .. } => assert_eq!(reason, "not_human"),
        other => panic!("expected PromotionRefused, got {other}"),
    }
    // A human reviewer on an unendorsed version → validity_not_valid.
    let e = promote(
        &mut store,
        &vid,
        &human_seal(),
        PersistenceScope::Project,
        AuthorityClass::Principal,
        &ctx,
    )
    .unwrap_err();
    match e {
        LifecycleError::PromotionRefused { reason, .. } => {
            assert_eq!(reason, "validity_not_valid")
        }
        other => panic!("expected PromotionRefused, got {other}"),
    }
    // Both refusals ledgered `context.memory.promotion_refused`.
    let refused = store
        .drain_events()
        .iter()
        .filter(|(c, _)| c == "context.memory.promotion_refused")
        .count();
    assert_eq!(refused, 2);
}

#[test]
fn candidate_diff_inverts_byte_identical() {
    // The S1 apply/invert gate's honest happy path — the fixture diff
    // round-trips byte-identically (§3.1.7).
    let base = base_doc();
    let target = target_doc();
    let d = candidate_diff(&base, &target);
    let t = hh_hir::diff::apply(&base, &d).unwrap();
    let b = hh_hir::diff::apply(&t, &hh_hir::diff::invert(&d)).unwrap();
    assert_eq!(b.canonical_bytes(), base.canonical_bytes());
    hh_hir::validate::validate(&t).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. The full happy path — a candidate driven S0→S10 through the engine
//    (every gate accepts on evidence), plus the lifecycle legs
//    (revalidate/reactivate/withdraw/revert) and the stage-kind admission.
// ═══════════════════════════════════════════════════════════════════════════

/// An arm with the M3 `matched_total` shape (eval + search budgeted).
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

/// A stage experiment bound to the campaign (`ext.evolution_campaign` —
/// the `ForeignExperiment` gate's binding member). `kind` and the arms'
/// match mode vary per stage (a retirement spec's arms carry
/// `matched_cap`/`cold_start` per its own match-shape rule).
fn stage_spec(
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
                dead_weight_purpose: false,
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

/// A `ComparisonReport` — `benefit_kind`/`held_out`/`interval` vary per
/// stage; everything else is the honest matched shape.
fn comparison(benefit: BenefitKind, held_out: bool, interval: Json) -> ComparisonReport {
    ComparisonReport {
        arm_a: "a".into(),
        arm_b: "b".into(),
        metric: "task_success".into(),
        pairing: "by_task".into(),
        paired_effect: PairedEffect {
            point: Some(Json::str("0.05")),
            interval: Some(interval),
            method: IntervalMethod::ClusteredClt,
        },
        per_task_effects_ref: Some("effects:1".into()),
        budget_match: BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 50_000,
            status: BudgetMatchStatus::Matched,
            search_unknown: false,
        },
        benefit_kind: benefit,
        held_out,
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

fn security_ok() -> SecurityInvarianceReport {
    SecurityInvarianceReport {
        placement: "subprocess_confined".into(),
        has_interface: true,
        policy_leaves: BTreeMap::from([("policy:test".into(), "narrowing".into())]),
        dynamic_veto_table: BTreeMap::from([("metric:safety".into(), false)]),
    }
}

/// A `verified` compatibility record carrying its evidence ref.
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

/// Drive `cid` from `classified` through the sealed candidate — the
/// shared happy-path leg (S2…S8) two tests build on. Report doc names are
/// `tag`-scoped so several candidates can advance in one campaign.
fn advance_to_sealed(
    store: &mut Store,
    docs: &LabDocs,
    eng: &mut EvolutionCampaign,
    cid: &str,
    tag: &str,
) {
    eng.hypothesize(store, cid, &hypothesis())
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    eng.screen(store, cid, &screen_ok())
        .unwrap_or_else(|e| panic!("screen: {e}"));

    // S4 — a registered `comparative` spec + a matched
    // `search_time_benefit` comparison.
    let search = stage_spec(
        eng,
        ExperimentKind::Comparative,
        SplitLabel::Search,
        MatchMode::MatchedTotal,
    );
    docs.put_spec(&search).unwrap();
    // S6.2 G8 — the arm's `search_budget` ref resolves to a complete
    // `SearchBudgetRecord` (allocation shares sum to the ppm scale).
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
            allocation: [("search".to_string(), 1_000_000)].into_iter().collect(),
            adaptive_selection: false,
        }
        .to_json(),
    )
    .unwrap();
    let search_rep = format!("rep:search:{tag}");
    docs.put_named(
        doc_kind::REPORT,
        &search_rep,
        &comparison(
            BenefitKind::SearchTimeBenefit,
            false,
            Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
        )
        .to_json(),
    )
    .unwrap();
    eng.matched_eval(store, cid, &search.experiment_id, &search_rep)
        .unwrap_or_else(|e| panic!("matched_eval: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("searched"));

    // S5 — a `full_set` held-out `artifact_benefit` comparison whose
    // interval excludes 0, retention clean, vetoes clean.
    let held = stage_spec(
        eng,
        ExperimentKind::Comparative,
        SplitLabel::HeldOut,
        MatchMode::MatchedTotal,
    );
    docs.put_spec(&held).unwrap();
    let held_rep = format!("rep:heldout:{tag}");
    docs.put_named(
        doc_kind::REPORT,
        &held_rep,
        &comparison(
            BenefitKind::ArtifactBenefit,
            true,
            Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
        )
        .to_json(),
    )
    .unwrap();
    eng.held_out_eval(
        store,
        cid,
        &held.experiment_id,
        &held_rep,
        &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
        &BTreeMap::from([("metric:safety".to_string(), false)]),
        None,
        None,
        false,
    )
    .unwrap_or_else(|e| panic!("held_out_eval: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("validated"));

    // S6 — one measured held-out family row with sign + interval, plus a
    // proven compatibility record.
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
    assert_eq!(eng.view.state_of(cid), Some("transferred"));

    // S7 — the security invariance re-check.
    eng.security_check(store, cid, &security_ok())
        .unwrap_or_else(|e| panic!("security_check: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("security_checked"));

    // S8 — the seven-item acceptance report + the human seal.
    eng.seal(store, cid, &acceptance(true), &human_seal())
        .unwrap_or_else(|e| panic!("seal: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("sealed_candidate"));
}

#[test]
fn pipeline_walks_s0_to_s10_happy_path() {
    let (_root, _clock, mut store, docs, mut eng) = open("full");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    assert_eq!(eng.view.state_of(&cid), Some("classified"));
    advance_to_sealed(&mut store, &docs, &mut eng, &cid, "a");

    // S9 — the shadow canary settles clean → active.
    eng.canary(&mut store, &cid, "shadow", "int:counterfactual-1")
        .unwrap_or_else(|e| panic!("canary: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("canary"));
    eng.canary_settle(&mut store, &cid, "clean", "", &[], None)
        .unwrap_or_else(|e| panic!("canary_settle: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("active"));

    // S10 — expiry → revalidate → reactivate → expiry → the human-sealed
    // removal test retires the candidate.
    eng.expire(&mut store, &cid, "superseded").unwrap();
    assert_eq!(eng.view.state_of(&cid), Some("expiring"));
    eng.revalidate(&mut store, &cid, "rep:heldout:a").unwrap();
    eng.reactivate(&mut store, &cid).unwrap();
    assert_eq!(eng.view.state_of(&cid), Some("active"));
    eng.expire(&mut store, &cid, "deadline").unwrap();
    let retire_spec = stage_spec(
        &eng,
        ExperimentKind::Retirement,
        SplitLabel::HeldOut,
        MatchMode::MatchedCap,
    );
    docs.put_spec(&retire_spec).unwrap();
    docs.put_named(
        doc_kind::REPORT,
        "rep:removal",
        &comparison(
            BenefitKind::ArtifactBenefit,
            true,
            // Non-inferior within margin — the lo bound clears
            // `retention_margin_ppm` (0 + 50_000 ≥ 0).
            Json::obj([("lo", Json::Int(0)), ("hi", Json::Int(4))]),
        )
        .to_json(),
    )
    .unwrap();
    eng.retire(
        &mut store,
        &cid,
        &retire_spec.experiment_id,
        "rep:removal",
        &human_seal(),
    )
    .unwrap_or_else(|e| panic!("retire: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("retired"));

    // The durable spine — every S-stage transition landed on the
    // campaign run.
    let events = store.events(&eng.run_id).unwrap();
    for to in [
        "proposed",
        "classified",
        "hypothesized",
        "screened",
        "searched",
        "validated",
        "transferred",
        "security_checked",
        "sealed_candidate",
        "canary",
        "active",
        "expiring",
        "revalidated",
        "retired",
    ] {
        assert!(
            events.iter().any(|e| {
                e.class == "measurement.evolution.candidate.transitioned"
                    && e.payload.get("to").and_then(Json::as_str) == Some(to)
            }),
            "no transitioned{{to: {to}}} row"
        );
    }
    eng.stop(&mut store, "no_addressable_failure").unwrap();
    eng.close(&mut store).unwrap();
    assert_eq!(eng.view.status, "closed");
}

#[test]
fn stage_kind_admission_refuses_mismatched_specs() {
    let (_root, _clock, mut store, docs, mut eng, cid) = hypothesized_candidate("kind");
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    // A `retirement` spec is a removal test — it is not an eval kind.
    let ret = stage_spec(
        &eng,
        ExperimentKind::Retirement,
        SplitLabel::Search,
        MatchMode::MatchedCap,
    );
    docs.put_spec(&ret).unwrap();
    let e = eng
        .matched_eval(&mut store, &cid, &ret.experiment_id, "rep:x")
        .unwrap_err();
    match refusal(&e) {
        Refusal::StageKindMismatch { kind, stage, .. } => {
            assert_eq!(kind, "retirement");
            assert_eq!(stage, "S4");
        }
        other => panic!("expected StageKindMismatch, got {other}"),
    }
    // Fresh candidate — the rejected one is terminal.
    let (_root, _clock, mut store, docs, mut eng, cid) = hypothesized_candidate("kind2");
    eng.screen(&mut store, &cid, &screen_ok()).unwrap();
    // An `exploratory` spec never compares — not an eval kind either.
    let ex = stage_spec(
        &eng,
        ExperimentKind::Exploratory,
        SplitLabel::Search,
        MatchMode::MatchedTotal,
    );
    docs.put_spec(&ex).unwrap();
    let e = eng
        .matched_eval(&mut store, &cid, &ex.experiment_id, "rep:x")
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::StageKindMismatch { .. }));
    // And a comparative spec is not a removal test at S10 — checked at
    // `retire` (the stage gate + `is_retirement`).
    let (_root, _clock, mut store, docs, mut eng) = open("kind3");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    advance_to_sealed(&mut store, &docs, &mut eng, &cid, "b");
    eng.canary(&mut store, &cid, "shadow", "int:1").unwrap();
    eng.canary_settle(&mut store, &cid, "clean", "", &[], None)
        .unwrap();
    eng.expire(&mut store, &cid, "deadline").unwrap();
    let eval = stage_spec(
        &eng,
        ExperimentKind::Comparative,
        SplitLabel::HeldOut,
        MatchMode::MatchedTotal,
    );
    docs.put_spec(&eval).unwrap();
    let e = eng
        .retire(
            &mut store,
            &cid,
            &eval.experiment_id,
            "rep:x",
            &human_seal(),
        )
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::StageKindMismatch { .. }));
}

#[test]
fn canary_abort_withdraw_and_revert_land_terminal_rows() {
    // A canary abort lands `reverted` (the durable decision — the
    // sealed-ancestor restore is the caller's apply(base, invert(diff))).
    let (_root, _clock, mut store, docs, mut eng) = open("abort");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    advance_to_sealed(&mut store, &docs, &mut eng, &cid, "c");
    eng.canary(&mut store, &cid, "shadow", "int:1").unwrap();
    let e = eng
        .canary_settle(&mut store, &cid, "veto", "safety regression", &[], None)
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::CanaryAborted { .. }));
    assert_eq!(eng.view.state_of(&cid), Some("reverted"));

    // `revert` from a non-serving state refuses.
    let (_root, _clock, mut store, _docs, mut eng, cid) = classified_candidate("rvt");
    let e = eng.revert(&mut store, &cid, "manual").unwrap_err();
    assert!(matches!(refusal(&e), Refusal::IllegalTransition { .. }));

    // `withdraw` from a non-serving state lands `withdrawn`; a second
    // withdraw refuses (terminal).
    eng.withdraw(&mut store, &cid, "test:author").unwrap();
    assert_eq!(eng.view.state_of(&cid), Some("withdrawn"));
    let e = eng.withdraw(&mut store, &cid, "test:author").unwrap_err();
    assert!(matches!(refusal(&e), Refusal::IllegalTransition { .. }));

    // A `split`-mode canary never commits (the noncommittable rollout
    // refusal) — fresh campaign, fresh candidate.
    let (_root, _clock, mut store, docs, mut eng) = open("splitroll");
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    advance_to_sealed(&mut store, &docs, &mut eng, &cid, "d");
    let e = eng.canary(&mut store, &cid, "split", "int:1").unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::SplitRolloutNotCommittable { .. }
    ));
}
