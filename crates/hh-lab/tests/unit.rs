//! `hh-lab` deterministic test suite — the S1.24 test matrix (R-2.9.4⁰ᵃ /
//! R-2.9.8⁰ / R-2.10.3⁰ᵃ / R-2.10.4⁰ᵃ / R-2.10.5⁰ row-key fields):
//! closed-sum parse/refuse, canonical JSON round-trips, content-derived ids
//! (`task_id` semantic projection, `experiment_id`, `suite_id`, `split_hash`,
//! `run_plan_id`), the TaskRecord import battery, the `register` refusal set
//! and the analysis/debt codecs.

use std::collections::BTreeMap;

use hh_budget::matchspec::MatchSpec;
use hh_hir::leaves::Text;
use hh_ontology::config::Ref;
use hh_ontology::debt::ExpiryCondition;
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, FactorKind, IntervalMethod, Pairing, PreRegistration,
    SeedPolicy,
};
use hh_ontology::lab::{
    BenchmarkNetworkMode, ContaminationStratum, EnvironmentFamily, EnvironmentFamilyRecord,
    EpisodeModel, SplitLabel, SubmissionKind, TaskValidityState, VerifierIsolation,
};
use hh_ontology::participant::ParticipantClass;
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use hh_lab::analysis::*;
use hh_lab::bench::*;
use hh_lab::debt::*;
use hh_lab::experiment::*;
use hh_lab::model::*;

// ── fixtures ────────────────────────────────────────────────────────────────

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("lab.test", 0)
}

fn text(s: &str) -> Text {
    Text::new(s, "test:owner", kernel())
}

fn foreign_ref() -> ForeignRef {
    ForeignRef {
        system: "hf".into(),
        digest: "sha256:aa11".into(),
        label: None,
        provenance: kernel(),
    }
}

fn foreign_task() -> ForeignTaskId {
    ForeignTaskId {
        name: "task-1".into(),
        version: "1.0".into(),
        source_ref: "https://suite.example/tasks/1".into(),
        digest_claim: foreign_ref(),
    }
}

fn environment() -> EnvironmentSpec {
    EnvironmentSpec {
        image_ref: "img:base".into(),
        image_digest: "sha256:bb22".into(),
        runtime: None,
        network_mode: BenchmarkNetworkMode::None,
        limits: None,
        adapters: vec![],
        seed_data: vec![],
        build_context: vec![],
        ext: BTreeMap::new(),
    }
}

fn task() -> TaskRecord {
    TaskRecord {
        task_id: String::new(), // computed below via `semantic_id`
        foreign: foreign_task(),
        family: EnvironmentFamily::CodingTerminal,
        tags: vec!["unit".into()],
        split_label: SplitLabel::Dev,
        environment: environment(),
        visible: TaskVisible {
            instruction: text("fix the bug"),
            attachments: vec!["ref:readme".into()],
            task_metadata_scope: Json::Null,
        },
        held_out: TaskHeldOut {
            validators: vec!["ref:checker".into()],
            fixtures: vec!["ref:fixture".into()],
            goal_state: None,
            oracle_solution: None,
            never_delivered: true,
        },
        instrument: TaskInstrument {
            grader: AdapterGraderSpec {
                adapter_ref: "adapter:judge".into(),
                kind: GraderKind::Result,
                config: None,
            },
            budget_defaults: None,
            verify_budget: None,
            validity: TaskValidity {
                state: TaskValidityState::Valid,
                audit_ref: None,
                noise_ceiling: None,
                flawed_task_ids: vec![],
            },
            contamination: TaskContamination {
                first_public_at: None,
                stratum: ContaminationStratum::PrivateHeldOut,
            },
            episode_model: None,
            verifier_isolation: None,
        },
        provenance: kernel(),
        ext: BTreeMap::new(),
    }
}

fn task_with_id() -> TaskRecord {
    let mut t = task();
    t.task_id = t.semantic_id();
    t
}

fn family_record() -> EnvironmentFamilyRecord {
    EnvironmentFamilyRecord {
        family_id: EnvironmentFamily::CodingTerminal,
        version: "1".into(),
        requires: BTreeMap::new(),
        oracle_classes_available: vec!["executable".into()],
        submission_kind: SubmissionKind::Patch,
        verifier_isolation_default: VerifierIsolation::Separate,
        process_metrics: vec![],
        fault_profiles_admissible: vec![],
        perturbation_profiles_admissible: vec![],
        episode_model: EpisodeModel::SingleEpisode,
        provenance: kernel(),
    }
}

fn suite_validity() -> SuiteValidityRecord {
    SuiteValidityRecord {
        audit_ref: "audit:1".into(),
        audited_at: Some(7),
        epoch_seeded: true,
        dispatch_ref: Some("dispatch:1".into()),
        flawed_task_ids: vec![],
        noise_ceiling: None,
        retired_for_headline: false,
        reason: None,
    }
}

fn suite() -> SuiteManifest {
    let mut split_map = BTreeMap::new();
    split_map.insert("task:1".to_string(), SplitLabel::Dev);
    let mut m = SuiteManifest {
        suite_id: String::new(),
        foreign: foreign_task(),
        primary_family: EnvironmentFamily::CodingTerminal,
        tasks: vec!["task:1".into()],
        split_map,
        split_hash: String::new(),
        validity: suite_validity(),
        contamination_default: ContaminationStratum::PrivateHeldOut,
        adapter_ref: "adapter:suite".into(),
        parity_report: None,
        provenance: kernel(),
    };
    m.split_hash = split_hash(&m.split_map);
    m.suite_id = m.suite_id();
    m
}

fn seed_policy() -> SeedPolicy {
    SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    }
}

fn pre_registration() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "arm B beats arm A".into(),
        primary_metrics: vec!["score".into()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: "analysis:plan".into(),
        task_split_hash: "sha256:cc33".into(),
    }
}

fn design() -> Design {
    Design {
        id: "design:1".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 5,
        pairing: Pairing::ByTask,
        seed_policy: seed_policy(),
        held_out_split_ref: None,
        pre_registration: pre_registration(),
    }
}

fn level(id: &str) -> LevelSpec {
    LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    }
}

fn factor() -> FactorSpec {
    FactorSpec {
        name: "model".into(),
        kind: FactorKind::ModelSnapshot,
        granularity: None,
        role: None,
        levels: vec![level("l1"), level("l2")],
    }
}

fn arm(id: &str, lid: &str) -> ArmSpec {
    ArmSpec {
        arm_id: id.into(),
        hypothesis: "this arm wins".into(),
        level_assignment: [("model".to_string(), lid.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        match_spec: Some(MatchSpec::matched_cap(&[])),
        artifact_ref: Ref::new("def:x", "sha256:ee55"),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
    }
}

fn scheduling() -> SchedulingPolicy {
    SchedulingPolicy {
        max_concurrent_runs: 4,
        pools: vec![],
        order: OrderKind::RandomPermuted,
        permutation_seed: "seed:1".into(),
        start_stagger_ms: 0,
        deadline: None,
        priority: None,
    }
}

fn reattempt() -> ReattemptPolicy {
    ReattemptPolicy {
        max_per_plan: 2,
        max_fraction_of_plans_ppm: 100_000,
        backoff: Backoff {
            min_ms: 100,
            multiplier_ppm: 2_000_000,
            max_ms: 10_000,
        },
        error_classes_included: None,
        on_cancel: CancelPolicy::Replan,
    }
}

fn spec() -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: design(),
        pre_registration: Some(pre_registration()),
        factors: vec![factor()],
        arms: vec![arm("a", "l1"), arm("b", "l2")],
        suite: SuiteBinding {
            suite_ref: "suite:1".into(),
            split_labels_used: vec![SplitLabel::Dev],
            split_assignment_ref: Some("split:1".into()),
        },
        replicates_per_cell: 5,
        seed_policy: seed_policy(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling(),
        reattempt: reattempt(),
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn estimator() -> EstimatorSelection {
    EstimatorSelection {
        method: IntervalMethod::ClusteredClt,
        selection_rule: "adr-0158.clt_floor".into(),
        floors: BTreeMap::new(),
        fallback_chain: vec![],
        substituted: None,
    }
}

fn comparison() -> ComparisonReport {
    ComparisonReport {
        arm_a: "a".into(),
        arm_b: "b".into(),
        metric: "score".into(),
        pairing: "by_task".into(),
        paired_effect: PairedEffect {
            point: Some(Json::str("0.05")),
            interval: Some(Json::str("[0.01,0.09]")),
            method: IntervalMethod::ClusteredClt,
        },
        per_task_effects_ref: Some("effects:1".into()),
        budget_match: BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 50_000,
            status: BudgetMatchStatus::Matched,
        },
        benefit_kind: BenefitKind::ArtifactBenefit,
        held_out: true,
        estimated: None,
        test: TestRecord {
            kind: TestKind::PermutationSignflip,
        },
        sign_profile: SignProfile {
            helped: 3,
            hurt: 1,
            unchanged: 0,
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
        },
        label: ReportLabelKind::Headlined,
        estimator_selection: estimator(),
    }
}

// ── closed sums refuse unknown spellings ────────────────────────────────────

#[test]
fn closed_sums_round_trip_and_refuse_unknown() {
    // The R-2.9.4 vocabulary.
    for f in [
        EnvironmentFamily::CodingTerminal,
        EnvironmentFamily::BrowserComputer,
        EnvironmentFamily::SearchResearch,
        EnvironmentFamily::StructuredTool,
        EnvironmentFamily::PersistentMultiEpisode,
        EnvironmentFamily::AdversarialSecurity,
        EnvironmentFamily::LongRunningRecovery,
    ] {
        assert_eq!(EnvironmentFamily::parse(f.name()), Some(f));
    }
    assert_eq!(EnvironmentFamily::parse("mainframe"), None);

    for l in [
        SplitLabel::Dev,
        SplitLabel::Search,
        SplitLabel::HeldOut,
        SplitLabel::Public,
        SplitLabel::Private,
        SplitLabel::Control,
    ] {
        assert_eq!(SplitLabel::parse(l.name()), Some(l));
    }
    assert_eq!(SplitLabel::parse("train"), None);

    for c in [
        ContaminationStratum::PrivateHeldOut,
        ContaminationStratum::FreshTemporal,
        ContaminationStratum::PublicDated,
        ContaminationStratum::ContaminatedPublic,
        ContaminationStratum::Unknown,
    ] {
        assert_eq!(ContaminationStratum::parse(c.name()), Some(c));
    }
    assert_eq!(ContaminationStratum::parse("clean"), None);

    for s in [
        SubmissionKind::None,
        SubmissionKind::Patch,
        SubmissionKind::Answer,
        SubmissionKind::Session,
        SubmissionKind::Trace,
        SubmissionKind::Artifact,
    ] {
        assert_eq!(SubmissionKind::parse(s.name()), Some(s));
    }
    assert_eq!(SubmissionKind::parse("tweet"), None);

    for v in [
        VerifierIsolation::Separate,
        VerifierIsolation::Shared,
        VerifierIsolation::Hosted,
    ] {
        assert_eq!(VerifierIsolation::parse(v.name()), Some(v));
    }
    assert_eq!(VerifierIsolation::parse("together"), None);

    for e in [
        EpisodeModel::SingleEpisode,
        EpisodeModel::FreshPerEpisode,
        EpisodeModel::Persistent,
    ] {
        assert_eq!(EpisodeModel::parse(e.name()), Some(e));
    }
    for t in [
        TaskValidityState::Valid,
        TaskValidityState::Flawed,
        TaskValidityState::Retired,
        TaskValidityState::UnderReview,
    ] {
        assert_eq!(TaskValidityState::parse(t.name()), Some(t));
    }
    assert_eq!(TaskValidityState::parse("great"), None);

    for g in [
        GraderKind::Result,
        GraderKind::Event,
        GraderKind::Trace,
        GraderKind::Composite,
    ] {
        assert_eq!(GraderKind::parse(g.name()), Some(g));
    }
    assert_eq!(GraderKind::parse("vibes"), None);

    // The R-2.10.3 vocabulary.
    for k in [
        ExperimentKind::Comparative,
        ExperimentKind::Exploratory,
        ExperimentKind::Equivalence,
        ExperimentKind::Retirement,
        ExperimentKind::Reproduction,
    ] {
        assert_eq!(ExperimentKind::parse(k.name()), Some(k));
    }
    assert_eq!(ExperimentKind::parse("yolo"), None);
    assert!(ExperimentKind::Comparative.requires_match());
    assert!(!ExperimentKind::Exploratory.requires_match());

    for d in [
        EnvironmentDerivation::FreshFromImage,
        EnvironmentDerivation::ForkSnapshot,
    ] {
        assert_eq!(EnvironmentDerivation::parse(d.name()), Some(d));
    }
    assert_eq!(EnvironmentDerivation::parse("clone"), None);

    // The R-2.10.4 vocabulary.
    for s in [
        BudgetMatchStatus::Matched,
        BudgetMatchStatus::Imbalanced,
        BudgetMatchStatus::Unmatched,
    ] {
        assert_eq!(BudgetMatchStatus::parse(s.name()), Some(s));
    }
    assert_eq!(BudgetMatchStatus::parse("close_enough"), None);

    for b in [
        BenefitKind::ArtifactBenefit,
        BenefitKind::SearchTimeBenefit,
        BenefitKind::Transfer,
    ] {
        assert_eq!(BenefitKind::parse(b.name()), Some(b));
    }
    assert_eq!(BenefitKind::parse("synergy"), None);
}

// ── canonical JSON round-trips ───────────────────────────────────────────────

#[test]
fn task_record_json_round_trips_and_checks_id() {
    let t = task_with_id();
    let j = t.to_json();
    let back = TaskRecord::from_json(&j).unwrap();
    // The `Text` leaf serializes hash-only (the canonical sealed form elides
    // `content`) — round-trip equality holds at the JSON level, and the
    // decoded record re-encodes byte-identically.
    assert_eq!(back.to_json(), j);
    assert_eq!(back.task_id, t.task_id);
    assert!(back.check_id());
    // Strict decode — an unknown member refuses.
    let mut bad = j.clone();
    if let Json::Obj(ref mut m) = bad {
        m.insert("surprise".into(), Json::Bool(true));
    }
    assert!(TaskRecord::from_json(&bad).is_err());
}

#[test]
fn task_id_is_a_semantic_projection() {
    let t = task_with_id();
    let id = t.semantic_id();
    // `foreign`, `provenance`, `ext`, `instrument.validity`/`contamination`
    // are annotation/claim members — changing them never changes `task_id`.
    let mut t2 = t.clone();
    t2.foreign.version = "9.9".into();
    t2.ext.insert("note".into(), Json::str("x"));
    t2.instrument.validity.state = TaskValidityState::UnderReview;
    t2.instrument.contamination.stratum = ContaminationStratum::Unknown;
    assert_eq!(t2.semantic_id(), id);
    // A semantic member does change it.
    let mut t3 = t.clone();
    t3.tags.push("extra".into());
    assert_ne!(t3.semantic_id(), id);
}

#[test]
fn suite_records_round_trip_and_validate() {
    let m = suite();
    let j = m.to_json();
    let back = SuiteManifest::from_json(&j).unwrap();
    assert_eq!(back, m);
    m.validate().unwrap();

    // `split_map` keys ⊆ `tasks`.
    let mut bad = m.clone();
    bad.split_map.insert("task:ghost".into(), SplitLabel::Dev);
    assert!(matches!(
        bad.validate(),
        Err(SuiteError::SplitKeyNotInTasks { .. })
    ));
    // `split_hash` must match the map content.
    let mut bad2 = m.clone();
    bad2.split_hash = "sha256:wrong".into();
    assert!(matches!(
        bad2.validate(),
        Err(SuiteError::SplitHashMismatch)
    ));
    // The L2 epoch-seeded dispatch is mandatory.
    let mut bad3 = m.clone();
    bad3.validity.epoch_seeded = false;
    assert!(matches!(
        bad3.validate(),
        Err(SuiteError::MissingEpochSeededDispatch)
    ));

    // `split_hash` is deterministic over the map.
    let mut same = BTreeMap::new();
    same.insert("t".to_string(), SplitLabel::HeldOut);
    assert_eq!(split_hash(&same), split_hash(&same));
    let mut diff = same.clone();
    diff.insert("t2".to_string(), SplitLabel::Dev);
    assert_ne!(split_hash(&same), split_hash(&diff));

    // `SplitAssignmentRecord` — codec + hash check.
    let sar = SplitAssignmentRecord {
        suite_id: "suite:1".into(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: same.clone(),
        split_hash: split_hash(&same),
        registered_at: 3,
    };
    let back = SplitAssignmentRecord::from_json(&sar.to_json()).unwrap();
    assert_eq!(back, sar);
    sar.validate().unwrap();
    let mut bad_sar = sar.clone();
    bad_sar.split_hash = "sha256:bad".into();
    assert!(matches!(
        bad_sar.validate(),
        Err(SuiteError::SplitHashMismatch)
    ));

    // `SuiteValidityRecord` codec.
    let v = suite_validity();
    assert_eq!(SuiteValidityRecord::from_json(&v.to_json()).unwrap(), v);
}

// ── TaskRecord import battery (AC-R-2.9.4-1 schema half) ────────────────────

#[test]
fn task_import_battery() {
    task_with_id().validate().unwrap();

    // `never_delivered = false` is a contradiction.
    let mut t = task();
    t.held_out.never_delivered = false;
    assert_eq!(t.validate(), Err(TaskError::HeldOutDeliverable));

    // Disjoint surfaces — a ref on two surfaces refuses.
    let mut t = task();
    t.held_out.fixtures.push("ref:readme".into()); // also in visible.attachments
    assert!(matches!(
        t.validate(),
        Err(TaskError::SurfaceOverlap { .. })
    ));

    // Held-out material inside the environment image is a leak at import.
    let mut t = task();
    t.environment.seed_data.push("ref:checker".into());
    assert!(matches!(
        t.validate(),
        Err(TaskError::HeldOutInEnvironment { .. })
    ));

    // `verifier_isolation = shared` requires the family to declare it.
    let mut t = task();
    t.instrument.verifier_isolation = Some(VerifierIsolation::Shared);
    assert_eq!(
        t.validate_for_family(&family_record()),
        Err(TaskError::SharedUndeclared)
    );
    // A family whose default is `shared` admits it.
    let mut shared_family = family_record();
    shared_family.verifier_isolation_default = VerifierIsolation::Shared;
    t.validate_for_family(&shared_family).unwrap();
    // `separate` (the default) is always admissible.
    let t = task();
    t.validate_for_family(&family_record()).unwrap();
}

// ── R-2.9.8 claims ───────────────────────────────────────────────────────────

#[test]
fn snapshot_claim_and_compatibility_round_trip() {
    let claim = SnapshotClaim {
        provider: "vendor".into(),
        model_id: "m-7".into(),
        snapshot_id: "snap:1".into(),
        serving_route: Some("route:a".into()),
        base_snapshot_ref: None,
        training_lineage: Some(TrainingLineage {
            training_run_ref: "run:t1".into(),
            step: 42,
            data_refs: vec!["data:1".into()],
            method: "sft".into(),
            compute_claim: ComputeClaim {
                descriptor: Json::obj([("flops", Json::str("1e25"))]),
            },
        }),
        trained_under: vec![TrainedUnderRef {
            definition_semantic_id: "def:sem".into(),
            profile_semantic_id: "prof:sem".into(),
            bundle_id: "bundle:1".into(),
        }],
        training_cutoff_claim: Some(TrainingCutoffClaim {
            value: Some("2025-06".into()),
            provenance: CutoffProvenance::ProviderStated,
        }),
        weights_digest: Some(foreign_ref()),
        policy_version_exposed: PolicyVersionExposed::Supported,
    };
    let back = SnapshotClaim::from_json(&claim.to_json()).unwrap();
    assert_eq!(back, claim);
    // `training_cutoff_claim.provenance` is a closed sum.
    let mut j = claim.to_json();
    if let Json::Obj(ref mut m) = j {
        if let Some(Json::Obj(ref mut c)) = m.get_mut("training_cutoff_claim") {
            c.insert("provenance".into(), Json::str("guesswork"));
        }
    }
    assert!(SnapshotClaim::from_json(&j).is_err());
    for p in [
        CutoffProvenance::ProviderStated,
        CutoffProvenance::Inferred,
        CutoffProvenance::Unknown,
    ] {
        assert_eq!(CutoffProvenance::parse(p.name()), Some(p));
    }

    // `CompatibilityRecord` — verdict members require `evidence_ref`.
    let compat = CompatibilityRecord {
        snapshot_ref: "snap:1".into(),
        definition_semantic_id: "def:sem".into(),
        profile_semantic_id: "prof:sem".into(),
        status: CompatibilityStatus::Verified,
        evidence_ref: Some("ev:1".into()),
        regression_suite_ref: Some("suite:reg".into()),
        created_at: 9,
        expiry_condition: Some(ExpiryCondition {
            kind: hh_ontology::debt::ExpiryKind::ModelVersionChange,
            value: None,
        }),
        provenance: kernel(),
    };
    let back = CompatibilityRecord::from_json(&compat.to_json()).unwrap();
    assert_eq!(back, compat);
    compat.validate().unwrap();
    let mut no_evidence = compat.clone();
    no_evidence.evidence_ref = None;
    assert_eq!(
        no_evidence.validate(),
        Err(CompatibilityError::MissingEvidence)
    );
    // `trained_under`/`unknown` carry no evidence requirement.
    let mut claim_only = compat.clone();
    claim_only.status = CompatibilityStatus::TrainedUnder;
    claim_only.evidence_ref = None;
    claim_only.validate().unwrap();
}

// ── R-2.10.3 experiment schemas ──────────────────────────────────────────────

#[test]
fn experiment_spec_round_trips_and_checks_id() {
    let s = spec();
    assert!(s.check_id());
    let back = ExperimentSpec::from_json(&s.to_json()).unwrap();
    assert_eq!(back, s);
    // `experiment_id` is content-derived — changing the spec changes the id.
    let mut s2 = spec();
    s2.replicates_per_cell = 6;
    assert_ne!(s2.experiment_id(), s.experiment_id());
}

#[test]
fn register_admits_a_valid_comparative_spec() {
    spec().register(&SpecContext::member_level()).unwrap();
}

#[test]
fn register_refusals_are_typed() {
    let ctx = SpecContext::member_level();

    // MissingPreRegistration — every kind but `exploratory`.
    let mut s = spec();
    s.pre_registration = None;
    assert_eq!(
        s.register(&ctx),
        Err(ExperimentRefusal::MissingPreRegistration)
    );
    // … and `exploratory` is exempt (but then needs no match members either).
    let mut s = spec();
    s.kind = ExperimentKind::Exploratory;
    s.pre_registration = None;
    for a in s.arms.iter_mut() {
        a.match_spec = None;
        a.search_budget = None;
    }
    s.register(&ctx).unwrap();

    // UnbudgetedArm — empty eval budget.
    let mut s = spec();
    s.arms[0].eval_budget = String::new();
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::UnbudgetedArm { .. })
    ));
    // … or a missing search budget on a matched kind.
    let mut s = spec();
    s.arms[0].search_budget = None;
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::UnbudgetedArm { .. })
    ));

    // MissingMatchSpec on a matched kind.
    let mut s = spec();
    s.arms[0].match_spec = None;
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::MissingMatchSpec { .. })
    ));

    // UnsealedArtifact — an unpinned `version_id` refuses at member level.
    let mut s = spec();
    s.arms[0].artifact_ref = Ref::new("def:x", "latest");
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::UnsealedArtifact { .. })
    ));
    // … and the context's sealed view refines it.
    let s = spec();
    let ctx_unsealed = SpecContext {
        artifact_sealed: Some(&|_| false),
        ..SpecContext::member_level()
    };
    assert!(matches!(
        s.register(&ctx_unsealed),
        Err(ExperimentRefusal::UnsealedArtifact { .. })
    ));

    // SplitUnassigned — a matched kind without the registered assignment.
    let mut s = spec();
    s.suite.split_assignment_ref = None;
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::SplitUnassigned { .. })
    ));

    // InsufficientReplicates against the context floor.
    let ctx5 = SpecContext {
        min_replicates: 5,
        ..SpecContext::member_level()
    };
    let mut s = spec();
    s.replicates_per_cell = 2;
    assert!(matches!(
        s.register(&ctx5),
        Err(ExperimentRefusal::InsufficientReplicates {
            have: 2,
            need: 5,
            ..
        })
    ));

    // InadmissibleFactor — a non-portable level on `equivalence`.
    let mut s = spec();
    s.kind = ExperimentKind::Equivalence;
    s.factors[0].levels[0].non_portable = true;
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::InadmissibleFactor { .. })
    ));
    // … an arm assigning an undeclared factor.
    let mut s = spec();
    s.arms[0]
        .level_assignment
        .insert("ghost".into(), "l1".into());
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::InadmissibleFactor { .. })
    ));

    // AdaptiveOutsideSearch — an adaptive strategy off `adaptive_search`.
    let mut s = spec();
    s.validation_strategy = ValidationStrategy::SuccessiveHalving {
        eta: 3,
        min_budget: 1,
        brackets: 2,
    };
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::AdaptiveOutsideSearch { .. })
    ));
    // … and on an adaptive design, a non-search label leaks.
    let mut s = spec();
    s.design.kind = DesignKind::AdaptiveSearch;
    s.suite.split_labels_used = vec![SplitLabel::HeldOut];
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::AdaptiveOutsideSearch { .. })
            | Err(ExperimentRefusal::LeakedSplit { .. })
    ));

    // NotARetirementDiff — the context's retirement view reports `false`.
    let mut s = spec();
    s.kind = ExperimentKind::Retirement;
    let ctx_retire = SpecContext {
        retirement_diff: Some(false),
        ..SpecContext::member_level()
    };
    assert!(matches!(
        s.register(&ctx_retire),
        Err(ExperimentRefusal::NotARetirementDiff { .. })
    ));

    // ProfilePinnedAcrossProfiles — a non_portable level varied on a
    // `harness` factor (ADR-0154 D5).
    let mut s = spec();
    s.factors.push(FactorSpec {
        name: "harness".into(),
        kind: FactorKind::Harness,
        granularity: None,
        role: None,
        levels: vec![LevelSpec {
            non_portable: true,
            ..level("hp")
        }],
    });
    s.arms[0]
        .level_assignment
        .insert("harness".into(), "hp".into());
    assert!(matches!(
        s.register(&ctx),
        Err(ExperimentRefusal::ProfilePinnedAcrossProfiles { .. })
    ));

    // DependsOnDriftedCapability — the context reports a drifted level ref.
    let ctx_drift = SpecContext {
        capability_drifted: Some(&|_| true),
        ..SpecContext::member_level()
    };
    assert!(matches!(
        spec().register(&ctx_drift),
        Err(ExperimentRefusal::DependsOnDriftedCapability { .. })
    ));
}

#[test]
fn run_plan_id_is_deterministic_and_excludes_scheduling() {
    // `run_plan_id = H(experiment ∥ arm ∥ configuration_version ∥ task ∥
    // replicate)` — deterministic over exactly those five members.
    let a = run_plan_id("exp:1", "arm:a", "sha256:cfg", "task:1", 0);
    let b = run_plan_id("exp:1", "arm:a", "sha256:cfg", "task:1", 0);
    assert_eq!(a, b);
    assert!(a.starts_with("sha256:"));
    // Every member is in the projection.
    assert_ne!(a, run_plan_id("exp:2", "arm:a", "sha256:cfg", "task:1", 0));
    assert_ne!(a, run_plan_id("exp:1", "arm:b", "sha256:cfg", "task:1", 0));
    assert_ne!(a, run_plan_id("exp:1", "arm:a", "sha256:zzz", "task:1", 0));
    assert_ne!(a, run_plan_id("exp:1", "arm:a", "sha256:cfg", "task:2", 0));
    assert_ne!(a, run_plan_id("exp:1", "arm:a", "sha256:cfg", "task:1", 1));
    // Scheduling/transport members are not parameters — there is no way to
    // vary `pool`/`order`/`deadline`/`bundle_id` into the id by construction;
    // the `CellPlan` codec carries them outside `run_plan_id`.
}

#[test]
fn plan_records_round_trip() {
    let cell = PlanCell {
        cell_id: "c:1".into(),
        arm_id: "a".into(),
        configuration_id: "cfg:sem".into(),
        configuration_version_id: "sha256:cfg".into(),
        task_id: "task:1".into(),
        split_label: SplitLabel::Dev,
    };
    let rp = RunPlan {
        run_plan_id: run_plan_id("exp:1", "a", "sha256:cfg", "task:1", 0),
        cell_id: "c:1".into(),
        replicate_index: 0,
        seed_material: Json::Arr(vec![Json::Int(7)]),
        environment_derivation: EnvironmentDerivation::FreshFromImage,
        cache_scope_salt: "salt:cold".into(),
    };
    let plan = CellPlan {
        plan_id: String::new(),
        cells: vec![cell],
        run_plans: vec![rp],
        order: OrderPlan {
            permutation_seed: "seed:o".into(),
            blocks: vec!["b:1".into()],
        },
        generators: None,
        aliasing_table: None,
    };
    let mut plan = plan;
    plan.plan_id = plan.plan_id();
    let back = CellPlan::from_json(&plan.to_json()).unwrap();
    assert_eq!(back, plan);
}

// ── R-2.10.4 analysis schemas ────────────────────────────────────────────────

#[test]
fn analysis_records_round_trip() {
    let wm = WatermarkSet {
        watermarks: [("runs".to_string(), "sha256:w1".to_string())]
            .into_iter()
            .collect(),
    };
    let query = QuerySpec {
        metrics: vec!["score".into()],
        filters: None,
        grain: Some("cell".into()),
    };
    let spec = AnalysisSpec {
        spec_id: String::new(),
        query: query.clone(),
        estimator_selection: estimator(),
        resample: Some(ResamplePlan {
            kind: ResampleKind::Bootstrap,
            replicates: 1000,
            seed: Some("seed:r".into()),
            block_size: None,
        }),
        outputs: vec!["comparison".into()],
    };
    let mut spec = spec;
    spec.spec_id = spec.spec_id();
    assert_eq!(AnalysisSpec::from_json(&spec.to_json()).unwrap(), spec);
    assert_eq!(QuerySpec::from_json(&query.to_json()).unwrap(), query);
    assert_eq!(WatermarkSet::from_json(&wm.to_json()).unwrap(), wm);

    let rec = AnalysisRecord {
        analysis_id: String::new(),
        spec_ref: spec.spec_id.clone(),
        generated_from: wm.clone(),
        outputs: vec!["comparison".into()],
        status: AnalysisStatus::Final,
    };
    let mut rec = rec;
    rec.analysis_id = rec.analysis_id();
    assert_eq!(AnalysisRecord::from_json(&rec.to_json()).unwrap(), rec);

    let report = AnalysisReport {
        report_id: "report:1".into(),
        spec_hash: "sha256:spec".into(),
        generated_from: wm,
        status: AnalysisStatus::Final,
        result_ref: Some("result:1".into()),
    };
    assert_eq!(
        AnalysisReport::from_json(&report.to_json()).unwrap(),
        report
    );

    let cell = CellRecord {
        cell_id: "c:1".into(),
        configuration_version_id: "sha256:cfg".into(),
        arm_id: "a".into(),
        task_ref: "task:1".into(),
        replicate_count: 5,
    };
    assert_eq!(CellRecord::from_json(&cell.to_json()).unwrap(), cell);

    let render = RenderSpec {
        panels: vec!["scorecard".into(), "tails".into()],
        source_refs: vec!["result:1".into()],
    };
    assert_eq!(RenderSpec::from_json(&render.to_json()).unwrap(), render);
}

#[test]
fn comparison_report_carries_the_amended_shape() {
    let r = comparison();
    let back = ComparisonReport::from_json(&r.to_json()).unwrap();
    assert_eq!(back, r);
    // `budget_match.status` is the closed three-sum.
    for s in [
        BudgetMatchStatus::Matched,
        BudgetMatchStatus::Imbalanced,
        BudgetMatchStatus::Unmatched,
    ] {
        let mut r = comparison();
        r.budget_match.status = s;
        let j = r.to_json();
        assert_eq!(
            ComparisonReport::from_json(&j).unwrap().budget_match.status,
            s
        );
    }
    let mut j = r.to_json();
    if let Json::Obj(ref mut m) = j {
        if let Some(Json::Obj(ref mut b)) = m.get_mut("budget_match") {
            b.insert("status".into(), Json::str("sorta_matched"));
        }
    }
    assert!(ComparisonReport::from_json(&j).is_err());
}

#[test]
fn estimator_selection_bump_decodes_legacy_and_refuses_unknown_methods() {
    // The §6.4 bump emits `declared`; legacy `method` bodies still decode.
    let sel = estimator();
    let j = sel.to_json();
    let Json::Obj(m) = &j else {
        panic!("selection is an object")
    };
    assert!(m.contains_key("declared"));
    assert!(!m.contains_key("method"));

    // A legacy `method`-only body decodes identically.
    let mut legacy = m.clone();
    let declared = legacy.remove("declared").unwrap();
    legacy.insert("method".into(), declared);
    let back = EstimatorSelection::from_json(&Json::Obj(legacy)).unwrap();
    assert_eq!(back.method, sel.method);

    // `declared` + `method` present must agree.
    let mut both = m.clone();
    both.insert(
        "method".into(),
        Json::obj([("bootstrap_paired", Json::str("percentile"))]),
    );
    assert!(EstimatorSelection::from_json(&Json::Obj(both)).is_err());

    // An unknown `interval_method` form refuses (never silently carried).
    let mut bad = m.clone();
    bad.insert("declared".into(), Json::str("guess_interval"));
    assert!(EstimatorSelection::from_json(&Json::Obj(bad)).is_err());
}

// ── debt views ───────────────────────────────────────────────────────────────

#[test]
fn debt_views_round_trip_and_project() {
    let row = DebtIndexRow {
        rule_id: "rule:1".into(),
        owner: hh_ontology::debt::OwnerRef::principal("test:owner"),
        status: hh_ontology::debt::DebtStatus::Active,
        removal_test_kind: Some(hh_ontology::debt::RemovalTestKind::RetirementExperiment),
        expiry_condition: hh_ontology::debt::ExpiryCondition {
            kind: hh_ontology::debt::ExpiryKind::Date,
            value: Some("2099-01-01".into()),
        },
        created_at: 0,
        expiry_at: None,
    };
    let back = DebtIndexRow::from_json(&row.to_json()).unwrap();
    assert_eq!(back, row);

    // The health projection over the index is deterministic.
    let health = AssumptionDebtHealth {
        warn_within_ms: AssumptionDebtHealth::DEFAULT_WARN_WITHIN_MS,
    };
    let report = health.report(std::slice::from_ref(&row), 0);
    assert_eq!(DebtReport::from_json(&report.to_json()).unwrap(), report);
}

// ── misc: SplitLabel::search_admissible + EnvironmentFamilyRecord codec ──────

#[test]
fn environment_family_record_round_trips() {
    let f = family_record();
    f.validate().unwrap();
    let back = EnvironmentFamilyRecord::from_json(&f.to_json()).unwrap();
    assert_eq!(back, f);
    // A family accepting submissions with no oracle classes refuses.
    let mut bad = family_record();
    bad.oracle_classes_available = vec![];
    assert!(bad.validate().is_err());
    // `submission_kind = none` is exempt.
    let mut none_family = bad.clone();
    none_family.submission_kind = SubmissionKind::None;
    none_family.validate().unwrap();
}
