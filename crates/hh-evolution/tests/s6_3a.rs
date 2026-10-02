//! The S6.3a battery (§05h R-2.9.5 sub-stage 6c; ADR-0195 D12,
//! ADR-0196 D7, R-2.8.5²) — multi-family code search over hosted
//! coordinates:
//!
//! * the closed proposer-family set grows to `{ahe, rho, gene_bank,
//!   code_search}` — the three research families declare
//!   `research-grade` and the campaign carries the `research-grade`
//!   maturity flag + `preview` seal wall;
//! * `SlotAllocation::{voi, concentrate}` + floor validation
//!   (`voi` reads the `slot_history` corpus layer only);
//! * `ParentSelectionPolicy::{gene_bank{niche}, rho{trajectory_ref}}`
//!   over `LineageEntry{niche, trajectory_refs}`;
//! * the `code_search` variant emits `CompiledPayload` candidates —
//!   `Opaque` steps under a declared interface, `OpaqueWithoutInterface`
//!   refused at S1;
//! * `rebase` — a post-S4 candidate replays the same semantic edit onto
//!   a moved lineage head and re-enters at S5;
//! * hosted coordinate admission via `hh-hosting/1` descriptors —
//!   `supported` + `enforced` only (the C4→C2 edge), records-in at open
//!   and value-scoped at S1;
//! * `install` under the experiment's `authority_cap` (R-2.8.5²).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::matchspec::{MatchMode, MatchSpec};
use hh_evolution::campaign::{doc_kind, EvolutionCampaign};
use hh_evolution::errors::{EvolutionError, Refusal};
use hh_evolution::proposer::{
    CodeSearchProposer, CorpusRow, EvidenceCorpus, EvolutionProposer, GeneBankProposer,
    LineageEntry, ProposalConstraints, RhoProposer,
};
use hh_evolution::records::{
    AcceptanceItem, CandidateProposal, CorpusSpec, EvolutionAcceptanceReport,
    EvolutionCampaignSpec, FailureHypothesis, HostedCoordinateDescriptor, InstallRequest,
    ItemStatus, ParentSelectionPolicy, PortabilityLabel, PredictedDelta, PredictedEffect,
    ProposerFailure, ProposerOutcome, ScreenReport, SecurityInvarianceReport, SlotAllocation,
    StopRule, TransferRow, Tri, EVOLVABLE_TARGET_CLASSES, PROPOSER_FAMILIES, RESEARCH_FAMILIES,
};
use hh_evolution::state::{self, CandidateState};
use hh_experiment::docs::LabDocs;
use hh_hir::diff::{self, DiffDerivation, HirDiff};
use hh_hir::document::{DefinitionVersionRef, HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::leaves::{CompiledPayload, DeclaredInterface};
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::idp::idp_id;
use hh_lab::analysis::{
    BenefitKind, BudgetMatch, BudgetMatchStatus, ComparisonReport, Multiplicity, OutcomeBounds,
    OutcomeBoundsVerdict, PairedEffect, SignProfile, TailEffects, TestKind, TestRecord,
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

// ── fixture plumbing (the s6_1a/s6_2 shape — same store/docs machinery) ──

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-evolution-s63a-{}-{}", tag, std::process::id()));
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

fn rule_node(id: &str, seq: u64, trigger: Json) -> Node {
    sid(
        Node::new(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::obj([("threshold", Json::Int(500_000))]),
                conditioned_on: None,
                assumption_debt: None,
            }),
            prov(seq),
        ),
        id,
    )
}

fn budget_node(id: &str, seq: u64, hard: u64) -> Node {
    let dimensions = BTreeMap::from([(
        "tokens.blended".to_string(),
        DimensionBound {
            hard: Some(hard),
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

/// A `Procedure` node whose single `Opaque` step carries a
/// `CompiledPayload` — `interface` toggles the `declared_interface`
/// member the `OpaqueWithoutInterface` gate reads.
fn proc_node(id: &str, seq: u64, interface: bool) -> Node {
    let mut n = sid(
        Node::new(
            EntityKind::Procedure,
            KindRecord::Procedure(ProcedureRecord {
                preconditions: Json::Null,
                steps: vec![ProcedureStep::Opaque(CompiledPayload {
                    format_tag: "wasm".into(),
                    bytes_hash: "sha256:payload-0".into(),
                    declared_interface: if interface {
                        Some(DeclaredInterface {
                            inputs: Json::obj([]),
                            outputs: Json::obj([]),
                            effects: Default::default(),
                            deterministic: true,
                            target: "wasm".into(),
                        })
                    } else {
                        None
                    },
                    owner: "test:owner".into(),
                    provenance: prov(seq),
                })],
                expected_evidence: Json::Arr(vec![Json::str("ok")]),
                allowed_capabilities: vec![],
                // Handlers must be total over the derived classes —
                // `Opaque` derives `precondition_failed` +
                // `step_failed:any`.
                failure_handlers: Json::Arr(vec![
                    Json::obj([
                        ("on", Json::str("precondition_failed")),
                        ("then", Json::str("abort")),
                    ]),
                    Json::obj([
                        ("on", Json::obj([("step_failed", Json::str("any"))])),
                        ("then", Json::str("abort")),
                    ]),
                ]),
            }),
            prov(seq),
        ),
        id,
    );
    n.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::Instruction,
    }));
    n
}

fn base_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node(
        "test:rule",
        1,
        Json::obj([("on", Json::str("turn_end"))]),
    ));
    doc.nodes.push(budget_node("test:budget", 2, 1_000));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

/// The B-candidate target — the `test:rule` trigger rewrites to
/// `turn_start` (the same leaf the rebased diff must still touch).
fn target_doc() -> HirDocument {
    let mut doc = base_doc();
    doc.nodes[0] = rule_node("test:rule", 1, Json::obj([("on", Json::str("turn_start"))]));
    doc
}

/// The A-candidate target — a *different* leaf (the budget's hard
/// bound tightens) so the moved head does not collide with B's edit.
fn head_doc() -> HirDocument {
    let mut doc = base_doc();
    doc.nodes[1] = budget_node("test:budget", 2, 900);
    doc
}

/// A third target — same class as B's edit but a distinct diff (the
/// stale re-propose leg needs a fresh candidate hash).
fn other_doc() -> HirDocument {
    let mut doc = base_doc();
    doc.nodes[0] = rule_node("test:rule", 1, Json::obj([("on", Json::str("turn_pause"))]));
    doc
}

/// The rebased-B target — the moved head *plus* B's own leaf edit.
fn rebased_target_doc() -> HirDocument {
    let mut doc = head_doc();
    doc.nodes[0] = rule_node("test:rule", 1, Json::obj([("on", Json::str("turn_start"))]));
    doc
}

fn candidate_diff(base: &HirDocument, target: &HirDocument) -> HirDiff {
    match diff::diff(
        base,
        target,
        prov(50),
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

fn deposit_split(docs: &LabDocs) -> String {
    let rec = SplitAssignmentRecord {
        suite_id: "suite:test".into(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: [
            ("task:a".to_string(), SplitLabel::Dev),
            ("task:b".to_string(), SplitLabel::Search),
        ]
        .into_iter()
        .collect(),
        split_hash: idp_id("test.split", b"fixture").to_string(),
        registered_at: 1,
    };
    docs.put_named(doc_kind::SPLIT, "test-split", &rec.to_json())
        .unwrap();
    "test-split".to_string()
}

/// A `hh-hosting/1` coordinate descriptor (the admission surface the
/// spec's `hosted_descriptor_refs` resolve to). `capability` is the
/// `capability_vector` verdict for `coordinate_model`;
/// `enforcement_key` picks which `budget_enforcement` list carries the
/// coordinate (`enforced` / `reported_only` / absent).
fn hosted_descriptor(capability: &str, enforcement_key: &str) -> HostedCoordinateDescriptor {
    let mut capability_vector = BTreeMap::new();
    capability_vector.insert("coordinate_model".to_string(), capability.to_string());
    let mut budget_enforcement = BTreeMap::new();
    if !enforcement_key.is_empty() {
        budget_enforcement.insert(enforcement_key.to_string(), vec!["model".to_string()]);
    }
    HostedCoordinateDescriptor {
        participant_ref: "participant:hosted-1".into(),
        participant_class: "hosted".into(),
        abi: "hh-hosting/1".into(),
        capability_vector,
        budget_enforcement,
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
        target_classes: Vec::new(),
        hosted_coordinates: Vec::new(),
        hosted_descriptor_refs: Vec::new(),
        authority_cap: None,
        rollout_policy: None,
        judge_policy: None,
    }
}

/// The 6c research-family spec — `protocol = automated`,
/// `proposer_family = rho`, multi-`target_classes`, the
/// `research-grade` maturity flag, a `voi` slot allocation over the
/// declared `slot_history` layer.
fn research_spec(split_ref: &str) -> EvolutionCampaignSpec {
    let mut s = spec(split_ref);
    s.protocol = "automated".into();
    s.proposer_family = "rho".into();
    s.proposer_variant_ref = Some("variant:hh/evolution_proposer/rho".into());
    s.target_class = Some("guideline".into());
    s.target_classes = vec!["guideline".into(), "code_payload".into()];
    s.maturity_flags = vec!["research-grade".into()];
    s.allowed_target_kinds = Some(vec!["HarnessRule".into(), "Procedure".into()]);
    s.corpus.layers = Some(vec![
        "outcomes".into(),
        "slot_history".into(),
        "reference_trajectories".into(),
    ]);
    s.slot_allocation = SlotAllocation::Voi {
        floors: BTreeMap::from([
            ("guideline".to_string(), 100_000u64),
            ("code_payload".to_string(), 100_000u64),
            ("native".to_string(), 500_000u64),
        ]),
    };
    s
}

fn open_with(
    tag: &str,
    mut s: EvolutionCampaignSpec,
) -> (PathBuf, ManualClock, Store, LabDocs, EvolutionCampaign) {
    let (root, clock, mut store, docs) = rig(tag);
    let split_ref = deposit_split(&docs);
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

fn proposal(base: &HirDocument, target: &HirDocument, base_ref: &str) -> CandidateProposal {
    CandidateProposal {
        base_ref: base_ref.into(),
        diff: candidate_diff(base, target),
        slot: "guideline".into(),
        hypothesis: None,
        install: None,
        coordinate_values: Default::default(),
    }
}

/// An `origin = evolution` proposal (the automated families' S1 leg —
/// the derivation names the candidate id the non-human origin needs).
fn evo_proposal(base: &HirDocument, target: &HirDocument, base_ref: &str) -> CandidateProposal {
    let mut p = proposal(base, target, base_ref);
    p.diff = match diff::diff(
        base,
        target,
        ProvenanceRecord::minted(
            Origin::evolution("cand:t", "hyp:t"),
            PersistenceScope::Definition,
            50,
        ),
        DiffDerivation {
            hypothesis: None,
            trajectories: vec![],
            candidate_id: Some("cand:t".into()),
        },
    ) {
        Ok(d) => d,
        Err(e) => panic!("diff: {e:?}"),
    };
    p
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
    r.authority = AuthorityClass::Definition;
    r
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

fn stage_spec(eng: &EvolutionCampaign, split: SplitLabel, mode: MatchMode) -> ExperimentSpec {
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
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "design:evo-stage".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy.clone(),
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
        label: hh_lab::analysis::ReportLabelKind::Headlined,
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
        dynamic_veto_table: BTreeMap::from([("metric:safety".to_string(), false)]),
    }
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

/// Drive `cid` from `classified` through `searched` (S2–S4) — the
/// rebase test's staging leg.
fn drive_to_searched(
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
    let search = stage_spec(eng, SplitLabel::Search, MatchMode::MatchedTotal);
    docs.put_spec(&search).unwrap();
    let allocation = if eng.spec.proposer_family == "human" {
        [("search".to_string(), 1_000_000u64)].into_iter().collect()
    } else {
        // G8's named-facet rule — an automated family's search budget
        // declares the `proposer` slice.
        [
            ("proposer".to_string(), 500_000u64),
            ("search".to_string(), 500_000u64),
        ]
        .into_iter()
        .collect()
    };
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
            allocation,
            adaptive_selection: false,
        }
        .to_json(),
    )
    .unwrap();
    let rep = format!("rep:search:{tag}");
    docs.put_named(
        doc_kind::REPORT,
        &rep,
        &comparison(
            BenefitKind::SearchTimeBenefit,
            false,
            Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
        )
        .to_json(),
    )
    .unwrap();
    eng.matched_eval(store, cid, &search.experiment_id, &rep)
        .unwrap_or_else(|e| panic!("matched_eval: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("searched"));
}

/// Drive `cid` from `searched`/`rebased` through `security_checked`
/// (S5–S7).
fn drive_to_checked(
    store: &mut Store,
    docs: &LabDocs,
    eng: &mut EvolutionCampaign,
    cid: &str,
    tag: &str,
) {
    let held = stage_spec(eng, SplitLabel::HeldOut, MatchMode::MatchedTotal);
    docs.put_spec(&held).unwrap();
    let rep = format!("rep:heldout:{tag}");
    docs.put_named(
        doc_kind::REPORT,
        &rep,
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
        &rep,
        &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
        &BTreeMap::from([("metric:safety".to_string(), false)]),
        None,
        None,
        false,
    )
    .unwrap_or_else(|e| panic!("held_out_eval: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("validated"));
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
    assert_eq!(eng.view.state_of(cid), Some("security_checked"));
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

fn golden_corpus() -> EvidenceCorpus {
    EvidenceCorpus {
        layers: vec!["outcomes".into()],
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
        reference_trajectories: Vec::new(),
    }
}

fn lineage_entry(id: &str, score: u64, niche: Option<&str>, trajectories: &[&str]) -> LineageEntry {
    LineageEntry {
        candidate_id: id.into(),
        target_ref: DefinitionVersionRef {
            semantic_id: format!("def:{id}"),
            version_id: format!("ver:{id}"),
        },
        score_ppm: score,
        children: 0,
        per_task: BTreeMap::new(),
        niche: niche.map(str::to_string),
        trajectory_refs: trajectories.iter().map(|t| t.to_string()).collect(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. The closed family/target-class surface + spec validation.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn closed_family_set_and_code_payload_target() {
    assert_eq!(
        PROPOSER_FAMILIES,
        &["ahe", "rho", "gene_bank", "code_search"]
    );
    assert_eq!(RESEARCH_FAMILIES, &["rho", "gene_bank", "code_search"]);
    assert!(EVOLVABLE_TARGET_CLASSES.contains(&"code_payload"));
}

#[test]
fn research_family_requires_the_research_grade_flag() {
    let mut s = research_spec("test-split");
    s.maturity_flags = vec![];
    let e = s.validate().unwrap_err();
    assert!(
        matches!(e, Refusal::SchemaViolation { .. }),
        "research family without the flag must refuse, got {e:?}"
    );
    s.maturity_flags = vec!["research-grade".into()];
    s.validate().unwrap();
}

#[test]
fn ahe_keeps_the_one_class_rule() {
    let mut s = spec("test-split");
    s.protocol = "automated".into();
    s.proposer_family = "ahe".into();
    s.proposer_variant_ref = Some("variant:hh/evolution_proposer/ahe".into());
    s.target_class = Some("guideline".into());
    s.allowed_target_kinds = Some(vec!["HarnessRule".into()]);
    // `target_classes` equal to {target_class} is legal.
    s.target_classes = vec!["guideline".into()];
    s.validate().unwrap();
    // A second class refuses — `ahe` stays single-class at 6c.
    s.target_classes = vec!["guideline".into(), "code_payload".into()];
    let e = s.validate().unwrap_err();
    assert!(matches!(e, Refusal::SchemaViolation { .. }), "got {e:?}");
}

#[test]
fn non_evolvable_target_class_refused() {
    let mut s = research_spec("test-split");
    s.target_classes = vec!["guideline".into(), "secrets".into()];
    let e = s.validate().unwrap_err();
    assert!(matches!(e, Refusal::SchemaViolation { .. }), "got {e:?}");
}

#[test]
fn voi_allocation_requires_the_slot_history_layer() {
    let mut s = research_spec("test-split");
    s.corpus.layers = Some(vec!["outcomes".into()]); // no slot_history
    let e = s.validate().unwrap_err();
    assert!(
        matches!(e, Refusal::SchemaViolation { ref detail } if detail.contains("slot_history")),
        "voi without the slot_history layer must refuse, got {e:?}"
    );
}

#[test]
fn concentrate_and_voi_floors_are_bounded_and_declared() {
    let mut s = research_spec("test-split");
    // Floors summing past the whole → BudgetSplittingTrap.
    s.slot_allocation = SlotAllocation::Concentrate {
        floors: BTreeMap::from([
            ("guideline".to_string(), 800_000u64),
            ("code_payload".to_string(), 400_000u64),
        ]),
    };
    let e = s.validate().unwrap_err();
    assert!(
        matches!(e, Refusal::BudgetSplittingTrap { .. }),
        "got {e:?}"
    );
    // A floor naming an undeclared slot → SchemaViolation.
    s.slot_allocation = SlotAllocation::Voi {
        floors: BTreeMap::from([("undeclared_slot".to_string(), 100_000u64)]),
    };
    let e = s.validate().unwrap_err();
    assert!(matches!(e, Refusal::SchemaViolation { .. }), "got {e:?}");
    // Declared floors pass (`native` covers the rest).
    s.slot_allocation = SlotAllocation::Voi {
        floors: BTreeMap::from([
            ("guideline".to_string(), 100_000u64),
            ("native".to_string(), 700_000u64),
        ]),
    };
    s.validate().unwrap();
}

#[test]
fn slot_allocation_and_parent_selection_round_trip() {
    let voi = SlotAllocation::Voi {
        floors: BTreeMap::from([("native".to_string(), 600_000u64)]),
    };
    assert_eq!(voi.mode(), "voi");
    assert_eq!(SlotAllocation::from_json(&voi.to_json()).unwrap(), voi);
    let conc = SlotAllocation::Concentrate {
        floors: BTreeMap::from([("guideline".to_string(), 250_000u64)]),
    };
    assert_eq!(conc.mode(), "concentrate");
    assert_eq!(SlotAllocation::from_json(&conc.to_json()).unwrap(), conc);
    let gb = ParentSelectionPolicy::GeneBank {
        niche: "code_payload".into(),
    };
    assert_eq!(ParentSelectionPolicy::from_json(&gb.to_json()).unwrap(), gb);
    let rho = ParentSelectionPolicy::Rho {
        trajectory_ref: "traj:ref-1".into(),
    };
    assert_eq!(
        ParentSelectionPolicy::from_json(&rho.to_json()).unwrap(),
        rho
    );
}

#[test]
fn authority_cap_requires_grant_spellings() {
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["not-a-grant".into()]);
    let e = s.validate().unwrap_err();
    assert!(matches!(e, Refusal::SchemaViolation { .. }), "got {e:?}");
    s.authority_cap = Some(vec!["models:install".into(), "tools:invoke".into()]);
    s.validate().unwrap();
}

#[test]
fn hosted_coordinates_need_participants_and_descriptors() {
    let mut s = spec("test-split");
    s.hosted_coordinates = vec!["model".into()];
    // No hosted_participants → SchemaViolation.
    let e = s.validate().unwrap_err();
    assert!(matches!(e, Refusal::SchemaViolation { .. }), "got {e:?}");
    // hosted_participants but no descriptor refs → UnresolvableEvidence.
    s.hosted_participants = true;
    let e = s.validate().unwrap_err();
    assert!(
        matches!(e, Refusal::UnresolvableEvidence { .. }),
        "got {e:?}"
    );
    // hosted_participants + reported-only dimensions → AC-15's
    // IncommensurableMatch at open.
    s.hosted_descriptor_refs = vec!["desc:hosted-1".into()];
    s.reported_only_dimensions = vec!["wall_clock".into()];
    let e = s.validate().unwrap_err();
    assert!(
        matches!(e, Refusal::IncommensurableMatch { .. }),
        "got {e:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Hosted-coordinate admission at open (the hh-hosting/1 edge).
// ═══════════════════════════════════════════════════════════════════════════

fn hosted_spec() -> EvolutionCampaignSpec {
    let mut s = spec("test-split");
    s.hosted_participants = true;
    s.hosted_coordinates = vec!["model".into()];
    s.hosted_descriptor_refs = vec!["desc:hosted-1".into()];
    s
}

#[test]
fn open_admits_a_supported_coordinate() {
    let (root, clock, mut store, docs) = rig("host-ok");
    let split_ref = deposit_split(&docs);
    docs.put_named(
        doc_kind::HOSTED_DESCRIPTOR,
        "desc:hosted-1",
        &hosted_descriptor("supported", "enforced").to_json(),
    )
    .unwrap();
    let mut s = hosted_spec();
    s.corpus.split_assignment_ref = split_ref;
    let (_run, _eng) = EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s)
        .unwrap_or_else(|e| panic!("open: {e}"));
    let _ = (root, clock);
}

#[test]
fn open_refuses_an_unsupported_coordinate() {
    let (_r, _c, mut store, docs) = rig("host-no");
    let split_ref = deposit_split(&docs);
    docs.put_named(
        doc_kind::HOSTED_DESCRIPTOR,
        "desc:hosted-1",
        &hosted_descriptor("unsupported", "enforced").to_json(),
    )
    .unwrap();
    let mut s = hosted_spec();
    s.corpus.split_assignment_ref = split_ref;
    let e = match EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s) {
        Err(e) => e,
        Ok(_) => panic!("open should have refused"),
    };
    assert!(
        matches!(refusal(&e), Refusal::HostedCoordinateUnsupported { .. }),
        "an unsupported coordinate refuses at open, got {e:?}"
    );
}

#[test]
fn open_refuses_a_non_hosting_descriptor() {
    let (_r, _c, mut store, docs) = rig("host-abi");
    let split_ref = deposit_split(&docs);
    let mut d = hosted_descriptor("supported", "enforced");
    d.abi = "hh-other/9".into();
    docs.put_named(doc_kind::HOSTED_DESCRIPTOR, "desc:hosted-1", &d.to_json())
        .unwrap();
    let mut s = hosted_spec();
    s.corpus.split_assignment_ref = split_ref;
    let e = match EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s) {
        Err(e) => e,
        Ok(_) => panic!("open should have refused"),
    };
    assert!(
        matches!(refusal(&e), Refusal::HostedCoordinateUnsupported { .. }),
        "a non-hh-hosting/1 descriptor refuses, got {e:?}"
    );
}

#[test]
fn open_refuses_an_unresolvable_descriptor_ref() {
    let (_r, _c, mut store, docs) = rig("host-miss");
    let split_ref = deposit_split(&docs);
    let mut s = hosted_spec();
    s.corpus.split_assignment_ref = split_ref;
    let e = match EvolutionCampaign::open(&mut store, docs, "holder", 60_000, s) {
        Err(e) => e,
        Ok(_) => panic!("open should have refused"),
    };
    assert!(
        matches!(refusal(&e), Refusal::UnresolvableEvidence { .. }),
        "an unresolvable descriptor ref refuses, got {e:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. The S1 hosted-coordinate + install legs.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn coordinate_values_carry_the_durable_admission_checks() {
    let (_r, _c, mut store, _docs, mut eng) = {
        let (root, clock, mut store, docs) = rig("coord");
        let split_ref = deposit_split(&docs);
        docs.put_named(
            doc_kind::HOSTED_DESCRIPTOR,
            "desc:hosted-1",
            &hosted_descriptor("supported", "enforced").to_json(),
        )
        .unwrap();
        let mut s = hosted_spec();
        s.corpus.split_assignment_ref = split_ref;
        let (_, eng) = EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s)
            .unwrap_or_else(|e| panic!("open: {e}"));
        (root, clock, store, docs, eng)
    };
    let base = base_doc();
    let target = target_doc();

    // A scalar value on the declared, supported, enforced coordinate —
    // admitted through S1.
    let mut p = proposal(&base, &target, "def:base");
    p.coordinate_values
        .insert("model".into(), Json::str("model:v2"));
    let cid = eng
        .propose(&mut store, &p, &base)
        .unwrap_or_else(|e| panic!("propose: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("classified"));

    // A structured value is a structural coordinate edit — refused.
    let mut p2 = proposal(&base, &target, "def:base");
    p2.diff = candidate_diff(&base, &head_doc()); // distinct diff → distinct cid
    p2.coordinate_values
        .insert("model".into(), Json::obj([("w", Json::Int(1))]));
    let e = eng.propose(&mut store, &p2, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::HostedCoordinateUnsupported { .. }),
        "a structured coordinate value refuses, got {e:?}"
    );

    // An undeclared coordinate — refused.
    let mut p3 = proposal(&base, &target, "def:base");
    p3.diff = candidate_diff(&base, &rebased_target_doc());
    p3.coordinate_values
        .insert("unlisted".into(), Json::str("x"));
    let e = eng.propose(&mut store, &p3, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::HostedCoordinateUnsupported { .. }),
        "an undeclared coordinate refuses, got {e:?}"
    );
}

#[test]
fn reported_only_coordinate_refuses_matched_admission() {
    let (_r, _c, mut store, _docs, mut eng) = {
        let (root, clock, mut store, docs) = rig("coord-ro");
        let split_ref = deposit_split(&docs);
        docs.put_named(
            doc_kind::HOSTED_DESCRIPTOR,
            "desc:hosted-1",
            &hosted_descriptor("supported", "reported_only").to_json(),
        )
        .unwrap();
        let mut s = hosted_spec();
        s.corpus.split_assignment_ref = split_ref;
        let (_, eng) = EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s)
            .unwrap_or_else(|e| panic!("open: {e}"));
        (root, clock, store, docs, eng)
    };
    let base = base_doc();
    let mut p = proposal(&base, &target_doc(), "def:base");
    p.coordinate_values
        .insert("model".into(), Json::str("model:v2"));
    let e = eng.propose(&mut store, &p, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::ReportedOnlyDimension { .. }),
        "a reported-only coordinate refuses matched admission, got {e:?}"
    );
}

#[test]
fn install_rides_the_experiments_authority_cap() {
    let base = base_doc();

    // No cap at all → AuthorityWidening (install is never admitted
    // without the experiment's declared ceiling).
    let (_r, _c, mut store, _docs, mut eng) = open("cap-none");
    let mut p = proposal(&base, &target_doc(), "def:base");
    p.install = Some(InstallRequest {
        name: "model:test".into(),
        kind: "model".into(),
        requested_grants: vec!["models:install".into()],
    });
    let e = eng.propose(&mut store, &p, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::AuthorityWidening { .. }),
        "install without a campaign authority_cap refuses, got {e:?}"
    );

    // A grant outside the cap → AuthorityWidening.
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["models:install".into()]);
    let (_r, _c, mut store, _docs, mut eng) = open_with("cap-out", s);
    let mut p = proposal(&base, &target_doc(), "def:base");
    p.install = Some(InstallRequest {
        name: "model:test".into(),
        kind: "model".into(),
        requested_grants: vec!["network:egress".into()],
    });
    let e = eng.propose(&mut store, &p, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::AuthorityWidening { .. }),
        "a grant outside the authority_cap refuses, got {e:?}"
    );

    // Grants ⊆ cap → admitted through S1.
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["models:install".into(), "tools:invoke".into()]);
    let (_r, _c, mut store, _docs, mut eng) = open_with("cap-in", s);
    let mut p = proposal(&base, &target_doc(), "def:base");
    p.install = Some(InstallRequest {
        name: "model:test".into(),
        kind: "model".into(),
        requested_grants: vec!["models:install".into()],
    });
    let cid = eng
        .propose(&mut store, &p, &base)
        .unwrap_or_else(|e| panic!("propose: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("classified"));
}

#[test]
fn install_plan_is_bounded_by_the_experiments_authority_cap() {
    use hh_registry::extension::lifecycle::{ProposerContext, TrustView};
    use hh_registry::extension::{
        Candidate, DeclaredSource, ExtensionKind, FetchOutcome, SourceLocator,
    };
    use hh_registry::records::ModelInstallRule;
    use std::collections::BTreeSet;

    let candidate = Candidate {
        name: "model:test".into(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "git".into(),
            credential_free_uri: "https://example.com/m.git".into(),
            selector: Some("main".into()),
            resolved: None,
            fetched_at: None,
        },
        source: DeclaredSource::Git {
            url: "https://example.com/m.git".into(),
            ref_: "main".into(),
        },
    };
    let outcome = FetchOutcome {
        resolved: "git:m@abc".into(),
        content: hh_identity::idp::address(b"pkg", "application/octet-stream"),
        fetched_at: 7,
        code_identity: vec![],
        source_snapshot: None,
        surface_pin: None,
    };
    let proposer = ProposerContext {
        provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp:1"),
            PersistenceScope::Run,
            0,
        ),
        grants: BTreeSet::from(["models:install".to_string()]),
    };
    let view = TrustView {
        allowed_sources: BTreeSet::new(),
        accepted_signers: BTreeSet::new(),
        required_predicates: BTreeSet::new(),
        require_signature_for: BTreeSet::new(),
        hash_only_ceiling: AuthorityClass::External,
        max_age: None,
        model_install: ModelInstallRule::AllowAttenuated,
        live_policy_ids: vec![],
    };
    let base = base_doc();

    // A grant the *proposer* holds but the cap does not →
    // AuthorityWidening at the campaign boundary (the experiment's cap
    // is the ceiling, not the proposer's grant set).
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["tools:invoke".into()]);
    let (_r, _c, mut store, _docs, mut eng) = open_with("cap-inst1", s);
    let cid = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_or_else(|e| panic!("propose: {e}"));
    let e = eng
        .install(
            &mut store,
            &cid,
            &candidate,
            &outcome,
            BTreeSet::from(["models:install".to_string()]),
            &proposer,
            &view,
        )
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::AuthorityWidening { .. }),
        "grant outside the cap must refuse, got {e:?}"
    );

    // The same grant under a covering cap → the plan returns and the
    // `install_requested` row mints on the campaign run.
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["models:install".into()]);
    let (_r, _c, mut store, _docs, mut eng) = open_with("cap-inst2", s);
    let cid = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_or_else(|e| panic!("propose: {e}"));
    let plan = eng
        .install(
            &mut store,
            &cid,
            &candidate,
            &outcome,
            BTreeSet::from(["models:install".to_string()]),
            &proposer,
            &view,
        )
        .unwrap_or_else(|e| panic!("install: {e}"));
    assert_eq!(
        plan.install_requested.get("class").and_then(Json::as_str),
        Some("security.extension.install_requested")
    );
    let classes: Vec<String> = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect();
    assert!(
        classes
            .iter()
            .any(|c| c == "security.extension.install_requested"),
        "the durable install_requested row must mint on the campaign run: {classes:?}"
    );

    // `model_install = deny` in the trust view → PolicyWidening (the
    // registry's own gate maps through, never swallowed).
    let mut deny = view.clone();
    deny.model_install = ModelInstallRule::Deny;
    let mut s = spec("test-split");
    s.authority_cap = Some(vec!["models:install".into()]);
    let (_r, _c, mut store, _docs, mut eng) = open_with("cap-inst3", s);
    let cid = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap();
    let e = eng
        .install(
            &mut store,
            &cid,
            &candidate,
            &outcome,
            BTreeSet::from(["models:install".to_string()]),
            &proposer,
            &deny,
        )
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::PolicyWidening { .. }),
        "model_install = deny must surface as PolicyWidening, got {e:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. `CompiledPayload` candidates — the OpaqueWithoutInterface gate.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn opaque_payload_without_interface_refused_at_s1() {
    let (_r, _c, mut store, _docs, mut eng) = open("opaque");
    let base = base_doc();
    let mut target = base.clone();
    target.nodes.push(proc_node("test:proc", 9, false));
    let p = proposal(&base, &target, "def:base");
    let e = eng.propose(&mut store, &p, &base).unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::OpaqueWithoutInterface { .. }),
        "an Opaque step without a declared interface refuses, got {e:?}"
    );
}

#[test]
fn opaque_payload_with_interface_passes_s1() {
    let (_r, _c, mut store, _docs, mut eng) = open("opaque-ok");
    let base = base_doc();
    let mut target = base.clone();
    target.nodes.push(proc_node("test:proc", 9, true));
    let p = proposal(&base, &target, "def:base");
    let cid = eng
        .propose(&mut store, &p, &base)
        .unwrap_or_else(|e| panic!("propose: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("classified"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. The research-family proposers.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn research_family_declarations() {
    let rho = RhoProposer::new(vec!["guideline".into()]);
    let d = rho.declare();
    assert_eq!(d.family, "rho");
    assert_eq!(d.maturity, "research-grade");
    assert_eq!(d.needs_reference_trajectories, Tri::Yes);

    let gb = GeneBankProposer::new(vec!["guideline".into(), "code_payload".into()]);
    let d = gb.declare();
    assert_eq!(d.family, "gene_bank");
    assert_eq!(d.maturity, "research-grade");
    assert_eq!(
        d.op_classes_admissible,
        vec!["guideline".to_string(), "code_payload".to_string()]
    );

    let cs = CodeSearchProposer::new(vec!["code_payload".into()]);
    let d = cs.declare();
    assert_eq!(d.family, "code_search");
    assert_eq!(d.maturity, "research-grade");
    assert_eq!(d.uses_judge, Tri::No);
}

#[test]
fn rho_requires_the_reference_trajectories_layer() {
    let mut p = RhoProposer::new(vec!["guideline".into()]);
    // Corpus without the layer → CorpusUnreadable.
    let e = p
        .propose(&golden_corpus(), &base_doc(), &constraints("def:base"))
        .unwrap_err();
    assert!(matches!(e, ProposerFailure::CorpusUnreadable { .. }));
    // The layer declared but empty → typed NoAddressableFailure.
    let mut corpus = golden_corpus();
    corpus.layers.push("reference_trajectories".into());
    let outs = p
        .propose(&corpus, &base_doc(), &constraints("def:base"))
        .unwrap();
    assert!(matches!(
        outs.as_slice(),
        [ProposerOutcome::NoAddressableFailure { .. }]
    ));
}

#[test]
fn rho_anchors_the_emitted_hypothesis_to_the_trajectory() {
    let mut p = RhoProposer::new(vec!["guideline".into()]);
    let mut corpus = golden_corpus();
    corpus.layers.push("reference_trajectories".into());
    corpus.reference_trajectories = vec!["traj:ref-1".into()];
    let outs = p
        .propose(&corpus, &base_doc(), &constraints("def:base"))
        .unwrap_or_else(|e| panic!("propose: {e:?}"));
    let ProposerOutcome::Hypothesis(h) = &outs[1] else {
        panic!("expected the hypothesis outcome, got {outs:?}")
    };
    assert_eq!(h.reference_trajectories, vec!["traj:ref-1".to_string()]);
}

#[test]
fn gene_bank_picks_the_least_covered_niche() {
    let mut p = GeneBankProposer::new(vec!["guideline".into(), "scheduling_rule".into()]);
    let mut c = constraints("def:base");
    c.target_classes = vec!["guideline".into(), "scheduling_rule".into()];
    // First emission lands in `guideline` (canonical order on a
    // zeroed archive); the second lands in the still-uncovered niche.
    let outs = p
        .propose(&golden_corpus(), &base_doc(), &c)
        .unwrap_or_else(|e| panic!("propose: {e:?}"));
    let ProposerOutcome::Candidate(c1) = &outs[0] else {
        panic!("expected a candidate, got {outs:?}")
    };
    assert_eq!(c1.slot, "guideline");
    let outs = p
        .propose(&golden_corpus(), &base_doc(), &c)
        .unwrap_or_else(|e| panic!("propose: {e:?}"));
    let ProposerOutcome::Candidate(c2) = &outs[0] else {
        panic!("expected a candidate, got {outs:?}")
    };
    assert_eq!(c2.slot, "scheduling_rule");
}

#[test]
fn code_search_emits_a_compiled_payload_candidate() {
    let mut p = CodeSearchProposer::new(vec!["code_payload".into()]);
    let mut base = base_doc();
    base.nodes.push(proc_node("test:proc", 9, true));
    let mut c = constraints("def:base");
    c.target_classes = vec!["code_payload".into()];
    c.allowed_kinds = vec!["Procedure".into()];
    let outs = p
        .propose(&golden_corpus(), &base, &c)
        .unwrap_or_else(|e| panic!("propose: {e:?}"));
    let ProposerOutcome::Candidate(cand) = &outs[0] else {
        panic!("expected a candidate, got {outs:?}")
    };
    // The emitted diff replaces the payload leaf — same interface,
    // fresh bytes_hash (the interface is the contract the leaf
    // satisfies; bytes are content-addressed, never embedded).
    assert!(
        cand.diff
            .ops
            .iter()
            .any(|o| matches!(o, diff::DiffOp::ReplaceLeaf { .. })),
        "the code_search diff must rewrite a leaf, got {:?}",
        cand.diff.ops
    );
    let t = diff::apply(&base, &cand.diff).unwrap();
    hh_hir::validate::validate(&t).unwrap();
}

#[test]
fn code_search_without_payload_class_or_leaf_is_typed() {
    // `code_payload` outside the admitted classes → ConstraintUnsatisfiable.
    let mut p = CodeSearchProposer::new(vec!["code_payload".into()]);
    let e = p
        .propose(&golden_corpus(), &base_doc(), &constraints("def:base"))
        .unwrap_err();
    assert!(matches!(e, ProposerFailure::ConstraintUnsatisfiable { .. }));
    // A base with no interface-bearing payload → NoAddressableFailure.
    let mut c = constraints("def:base");
    c.target_classes = vec!["code_payload".into()];
    c.allowed_kinds = vec!["Procedure".into(), "HarnessRule".into()];
    let outs = p.propose(&golden_corpus(), &base_doc(), &c).unwrap();
    assert!(matches!(
        outs.as_slice(),
        [ProposerOutcome::NoAddressableFailure { .. }]
    ));
}

#[test]
fn proposer_suite_in_process_passes_for_research_families() {
    use hh_evolution::proposer_conformance::{run_proposer_suite, InProcessPort, SuiteDrive};
    use hh_registry::kinds::ConformanceVerdict::*;

    // gene_bank — a multi-class variant over the shared corpus.
    let mut p = GeneBankProposer::new(vec!["guideline".into(), "scheduling_rule".into()]);
    let base = base_doc();
    let corpus = golden_corpus();
    let mut cons = constraints("def:base");
    cons.target_classes = vec!["guideline".into(), "scheduling_rule".into()];
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
            "gene_bank row {:?} failed: {}",
            r.subject,
            r.detail
        );
    }

    // rho — the corpus must carry its reference_trajectories layer.
    let mut p = RhoProposer::new(vec!["guideline".into()]);
    let base = base_doc();
    let mut corpus = golden_corpus();
    corpus.layers.push("reference_trajectories".into());
    corpus.reference_trajectories = vec!["traj:ref-1".into()];
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
            "rho row {:?} failed: {}",
            r.subject,
            r.detail
        );
    }

    // code_search — the CompiledPayload-emitting variant.
    let mut p = CodeSearchProposer::new(vec!["code_payload".into()]);
    let mut base = base_doc();
    base.nodes.push(proc_node("test:proc", 9, true));
    let corpus = golden_corpus();
    let mut cons = constraints("def:base");
    cons.target_classes = vec!["code_payload".into()];
    cons.allowed_kinds = vec!["Procedure".into()];
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
            "code_search row {:?} failed: {}",
            r.subject,
            r.detail
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. The niche/trajectory parent selectors.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn rho_selection_anchors_to_the_named_trajectory() {
    let p = GeneBankProposer::new(vec!["guideline".into()]);
    let lineage = vec![
        lineage_entry("a", 900, None, &["traj:1"]),
        lineage_entry("b", 950, None, &["traj:2"]),
        lineage_entry("c", 100, None, &["traj:1"]),
    ];
    // Anchored to traj:1 → `a` (the highest-scoring member citing it),
    // not the lineage-wide best `b`.
    let pick = p
        .select_parent(
            &lineage,
            &ParentSelectionPolicy::Rho {
                trajectory_ref: "traj:1".into(),
            },
        )
        .unwrap();
    assert_eq!(pick.version_id, "ver:a");
    // No member cites the anchor → None (an anchorless pick is never
    // invented).
    assert!(p
        .select_parent(
            &lineage,
            &ParentSelectionPolicy::Rho {
                trajectory_ref: "traj:nobody".into(),
            },
        )
        .is_none());
}

#[test]
fn gene_bank_selection_picks_the_niche_elite() {
    let p = GeneBankProposer::new(vec!["guideline".into()]);
    let lineage = vec![
        lineage_entry("a", 400, Some("n1"), &[]),
        lineage_entry("b", 950, Some("n2"), &[]),
        lineage_entry("c", 600, Some("n1"), &[]),
    ];
    // Within niche n1 → `c` (600 > 400), not the lineage-wide best `b`.
    let pick = p
        .select_parent(
            &lineage,
            &ParentSelectionPolicy::GeneBank { niche: "n1".into() },
        )
        .unwrap();
    assert_eq!(pick.version_id, "ver:c");
    // An empty niche falls back to the lineage-wide elite.
    let pick = p
        .select_parent(
            &lineage,
            &ParentSelectionPolicy::GeneBank {
                niche: "n-empty".into(),
            },
        )
        .unwrap();
    assert_eq!(pick.version_id, "ver:b");
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. `rebase` — the moved-head S5 re-entry.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn rebase_replays_the_same_edit_onto_a_moved_head() {
    let (_r, _c, mut store, docs, mut eng) = open("rebase");
    let base = base_doc();

    // Candidate B proposes on the pre-move head and reaches `searched`.
    let b = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_or_else(|e| panic!("propose B: {e}"));
    drive_to_searched(&mut store, &docs, &mut eng, &b, "b");

    // Candidate A lands and activates — the lineage head moves.
    let a = eng
        .propose(&mut store, &proposal(&base, &head_doc(), "def:base"), &base)
        .unwrap_or_else(|e| panic!("propose A: {e}"));
    drive_to_searched(&mut store, &docs, &mut eng, &a, "a");
    drive_to_checked(&mut store, &docs, &mut eng, &a, "a");
    eng.seal(&mut store, &a, &acceptance(true), &human_seal())
        .unwrap_or_else(|e| panic!("seal A: {e}"));
    eng.canary(&mut store, &a, "shadow", "int:1")
        .unwrap_or_else(|e| panic!("canary A: {e}"));
    eng.canary_settle(&mut store, &a, "clean", "", &[], None)
        .unwrap_or_else(|e| panic!("settle A: {e}"));
    let head = eng.view.head_ref().expect("A is active — head moved");

    // A fresh proposal on the old head refuses StaleBase (the lineage
    // head moved under it).
    let e = eng
        .propose(
            &mut store,
            &proposal(&base, &other_doc(), "def:base"),
            &base,
        )
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::StaleBase { .. }),
        "got {e:?}"
    );

    // Rebase B: the same leaf edit replayed onto the moved head. The
    // `rebased` transition mints and the candidate re-enters at S5.
    let rb = eng
        .rebase(
            &mut store,
            &b,
            &proposal(&head_doc(), &rebased_target_doc(), &head),
            &head_doc(),
        )
        .unwrap_or_else(|e| panic!("rebase: {e}"));
    assert!(!rb.is_empty()); // the deposited rebased-proposal ref
    assert_eq!(eng.view.state_of(&b), Some("rebased"));
    let rec = eng.view.candidate(&b).unwrap();
    assert_eq!(rec.rebase_of.as_deref(), Some("def:base"));

    // S5 re-entry — the rebased candidate must re-pass held-out; it
    // cannot skip ahead.
    let e = eng
        .security_check(&mut store, &b, &security_ok())
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::IllegalTransition { .. }),
        "rebased must re-enter at S5, got {e:?}"
    );
    drive_to_checked(&mut store, &docs, &mut eng, &b, "b2");
    assert_eq!(eng.view.state_of(&b), Some("security_checked"));
}

#[test]
fn rebase_refuses_a_changed_edit_and_earlier_states() {
    let (_r, _c, mut store, docs, mut eng) = open("rebase-eq");
    let base = base_doc();
    // Two stale candidates — every refused rebase rejects its
    // candidate (a refusal is reported, the state is stored), so each
    // leg needs its own.
    let b = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_or_else(|e| panic!("propose B: {e}"));
    let c = eng
        .propose(
            &mut store,
            &proposal(&base, &other_doc(), "def:base"),
            &base,
        )
        .unwrap_or_else(|e| panic!("propose C: {e}"));
    // Too early — only post-S4 states rebase.
    let e = eng
        .rebase(
            &mut store,
            &b,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::IllegalTransition { .. }));

    drive_to_searched(&mut store, &docs, &mut eng, &b, "b");
    drive_to_searched(&mut store, &docs, &mut eng, &c, "c");
    // Move the head.
    let a = eng
        .propose(&mut store, &proposal(&base, &head_doc(), "def:base"), &base)
        .unwrap_or_else(|e| panic!("propose A: {e}"));
    drive_to_searched(&mut store, &docs, &mut eng, &a, "a");
    drive_to_checked(&mut store, &docs, &mut eng, &a, "a");
    eng.seal(&mut store, &a, &acceptance(true), &human_seal())
        .unwrap();
    eng.canary(&mut store, &a, "shadow", "int:1").unwrap();
    eng.canary_settle(&mut store, &a, "clean", "", &[], None).unwrap();
    let head = eng.view.head_ref().unwrap();

    // A rebase naming the wrong base → StaleBase, candidate rejected.
    let e = eng
        .rebase(
            &mut store,
            &b,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::StaleBase { .. }),
        "got {e:?}"
    );
    assert_eq!(eng.view.state_of(&b), Some("rejected"));

    // A rebase whose semantic ops differ → StaleBase (a rebase replays
    // the same edit; a different edit is a new candidate).
    let mut swapped = head_doc();
    swapped.nodes[1] = budget_node("test:budget", 2, 800);
    let mut merged = swapped.clone();
    merged.nodes[0] = rule_node("test:rule", 1, Json::obj([("on", Json::str("turn_pause"))]));
    let e = eng
        .rebase(
            &mut store,
            &c,
            &proposal(&head_doc(), &merged, &head),
            &head_doc(),
        )
        .unwrap_err();
    assert!(
        matches!(refusal(&e), Refusal::StaleBase { .. }),
        "got {e:?}"
    );
    assert_eq!(eng.view.state_of(&c), Some("rejected"));
}

#[test]
fn rebase_folds_identically_through_ensure() {
    let (root, clock, mut store, docs, mut eng) = open("rebase-fold");
    let base = base_doc();
    let b = eng
        .propose(
            &mut store,
            &proposal(&base, &target_doc(), "def:base"),
            &base,
        )
        .unwrap();
    drive_to_searched(&mut store, &docs, &mut eng, &b, "b");
    let a = eng
        .propose(&mut store, &proposal(&base, &head_doc(), "def:base"), &base)
        .unwrap();
    drive_to_searched(&mut store, &docs, &mut eng, &a, "a");
    drive_to_checked(&mut store, &docs, &mut eng, &a, "a");
    eng.seal(&mut store, &a, &acceptance(true), &human_seal())
        .unwrap();
    eng.canary(&mut store, &a, "shadow", "int:1").unwrap();
    eng.canary_settle(&mut store, &a, "clean", "", &[], None).unwrap();
    let head = eng.view.head_ref().unwrap();
    eng.rebase(
        &mut store,
        &b,
        &proposal(&head_doc(), &rebased_target_doc(), &head),
        &head_doc(),
    )
    .unwrap();
    let run_id = eng.run_id.clone();
    let before_head = eng.view.head_ref();
    let before_b = eng.view.candidate(&b).unwrap().to_json();
    let before_a = eng.view.candidate(&a).unwrap().to_json();

    // Restart — drop the engine, let the writer lease expire, and the
    // fold over the durable prefix rebuilds `rebased` byte-identically.
    drop(eng);
    clock.advance(120_000);
    let eng2 = EvolutionCampaign::ensure(&mut store, docs, &run_id, "holder", 60_000)
        .unwrap_or_else(|e| panic!("ensure: {e}"));
    assert_eq!(eng2.view.head_ref(), before_head);
    assert_eq!(eng2.view.candidate(&b).unwrap().to_json(), before_b);
    assert_eq!(eng2.view.candidate(&a).unwrap().to_json(), before_a);
    assert_eq!(eng2.view.state_of(&b), Some("rebased"));
    let _ = root;
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. The research-grade preview wall.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn research_grade_campaign_seals_preview_only() {
    // A research-family campaign: non-preview labels refuse at S8.
    let (_r, _c, mut store, docs, mut eng) = open_with("preview-no", research_spec("test-split"));
    let base = base_doc();
    // Automated family ⇒ `origin = evolution` on the diff.
    let p = evo_proposal(&base, &target_doc(), "def:base");
    let cid = eng
        .propose(&mut store, &p, &base)
        .unwrap_or_else(|e| panic!("propose: {e}"));
    drive_to_searched(&mut store, &docs, &mut eng, &cid, "p");
    drive_to_checked(&mut store, &docs, &mut eng, &cid, "p");
    let e = eng
        .seal(&mut store, &cid, &acceptance(true), &human_seal())
        .unwrap_err();
    assert!(
        matches!(
            refusal(&e),
            Refusal::SealRefused { reason } if reason == "research_grade_not_preview"
        ),
        "a research-grade campaign seals preview only, got {e:?}"
    );
}

#[test]
fn research_grade_campaign_accepts_the_preview_label() {
    let (_r, _c, mut store, docs, mut eng) = open_with("preview-ok", research_spec("test-split"));
    let base = base_doc();
    let p = evo_proposal(&base, &target_doc(), "def:base");
    let cid = eng
        .propose(&mut store, &p, &base)
        .unwrap_or_else(|e| panic!("propose: {e}"));
    drive_to_searched(&mut store, &docs, &mut eng, &cid, "q");
    drive_to_checked(&mut store, &docs, &mut eng, &cid, "q");
    let mut acc = acceptance(true);
    acc.label = Some("preview".into());
    eng.seal(&mut store, &cid, &acc, &human_seal())
        .unwrap_or_else(|e| panic!("seal: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("sealed_candidate"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. The refusal surface renders durably.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn new_refusals_carry_codes_and_stages() {
    let cases: Vec<Refusal> = vec![Refusal::HostedCoordinateUnsupported {
        coordinate: "model".into(),
        detail: "x".into(),
    }];
    for r in cases {
        assert!(!r.code().is_empty());
        assert!(!r.stage().is_empty());
        assert!(format!("{r:?}").contains("HostedCoordinateUnsupported"));
    }
    assert_eq!(
        Refusal::HostedCoordinateUnsupported {
            coordinate: "m".into(),
            detail: "d".into()
        }
        .stage(),
        "S1"
    );
}

#[test]
fn state_machine_admits_the_rebase_leg() {
    for from in ["searched", "validated", "transferred", "security_checked"] {
        let f = match from {
            "searched" => CandidateState::Searched,
            "validated" => CandidateState::Validated,
            "transferred" => CandidateState::Transferred,
            _ => CandidateState::SecurityChecked,
        };
        assert!(state::allowed(&f, "rebased"), "{from} → rebased");
    }
    assert!(state::allowed(&CandidateState::Rebased, "validated"));
    // Earlier states cannot rebase; `rebased` cannot skip ahead.
    assert!(!state::allowed(&CandidateState::Screened, "rebased"));
    assert!(!state::allowed(
        &CandidateState::Rebased,
        "security_checked"
    ));
}
