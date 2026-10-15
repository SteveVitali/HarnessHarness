//! R2.16 — the §5h.2 fault-battery cells over the recorded corpus
//! (DF-S1.22-2; the eval-kernel halves live in `hh-eval/tests/r2_16.rs`).
//!
//! Every cell keys its `EvalRun` rows to real `benchset.stage3.v1` task
//! records (`suite.task_named` / `record().task_id`, the suite's committed
//! split label + `split_hash`) and drives the real eval kernels — `compare`,
//! `artifact_benefit`, `LedgerFacts::from_events` — so a regression in any
//! of these lands red on corpus data:
//!
//! - **combined failure** — one fixture run carrying infrastructure
//!   failure (stop class) + budget exhaustion (consumed vector at the cap)
//!   + verifier crash (`oracle_failed`) derives `oracle_failure` beside the
//!     scored rows in one `compare` — the three non-scored classes stratify
//!     in `outcome_counts`, never inside the estimate;
//! - **`search_budget = unknown`** — a hosted arm (session-ABI row beside
//!   the native one) compares at `product-level` flagged `search_unknown`
//!   and refuses `UnbudgetedArm` at `configuration-level`;
//! - **`LeakedSplit`** — a real `candidate.transitioned{to: proposed}` row
//!   on a corpus run's prefix makes `artifact_benefit` refuse when the
//!   suite's committed `registered_at` postdates it;
//! - **HAL `FaultType` levels** — every `f/hal-*` catalogue profile binds
//!   as a corpus run's `fault_profile` level and the binding round-trips
//!   `eval_run/1`.

use std::collections::BTreeMap;

use hh_bench::benchset::{BenchSuite, Benchset, BenchsetTask};
use hh_budget::matchspec::{ArmSpec, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::benefits::{artifact_benefit, split_hash_agrees, BenefitError};
use hh_eval::compare::{compare, CompareError, CompareInput};
use hh_eval::runs::{CacheState, EvalRun, TaskContext};
use hh_eval::LedgerFacts;
use hh_lab::analysis::{BenefitKind, ComparisonReport};
use hh_ontology::compliance::Detector;
use hh_ontology::control::{InfraError, InfraErrorFamily, OutcomeClass, StopReason};
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, MetricValue, MetricValueKind, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hh_ontology::participant::{Granularity, Observability, ParticipantClass};
use hh_wire::Json;

// ── corpus-keyed builders ───────────────────────────────────────────────────

fn eval_run(
    run_id: &str,
    arm: &str,
    task: &BenchsetTask,
    suite: &BenchSuite,
    replicate: u64,
    reward_ppm: i64,
    model_calls: i64,
) -> EvalRun {
    EvalRun {
        run_id: run_id.into(),
        arm_id: arm.into(),
        cell_id: Some(format!("{arm}:{}", task.record().task_id)),
        configuration_id: format!("cfg:{arm}"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
        ]
        .into_iter()
        .collect(),
        mediation: Default::default(),
        capability_vector: Default::default(),
        task_id: task.record().task_id.clone(),
        suite_id: suite.suite_id().into(),
        split_label: task.record().split_label,
        replicate_index: replicate,
        attempt_no: 1,
        seed: None,
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: [(DimensionId::ModelCalls, model_calls)]
            .into_iter()
            .collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Decimal(reward_ppm),
            applies_to: run_id.into(),
            oracle_ref: "oracle/adapter.grade".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
        }],
        environment_version_id: Some(format!("env:{}", suite.suite_id())),
        environment_family: suite.family(),
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: BTreeMap::new(),
        stratum: suite.stratum(),
        eval_search_spend: 0,
        split_hash: Some(suite.manifest.split_hash.clone()),
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        facts: LedgerFacts::default(),
    }
}

fn task_ctx(task: &BenchsetTask, suite: &BenchSuite) -> TaskContext {
    TaskContext {
        task_id: task.record().task_id.clone(),
        suite_id: suite.suite_id().into(),
        split_label: task.record().split_label,
        split_hash: suite.manifest.split_hash.clone(),
        stratum: suite.stratum(),
    }
}

fn paired_design(suite: &BenchSuite) -> Design {
    Design {
        id: format!("design:r2_16:{}", suite.key),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 2,
        pairing: Pairing::ByTaskAndReplicate,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: true,
            seed_honoured_required: true,
        },
        held_out_split_ref: Some("split:held_out".into()),
        pre_registration: PreRegistration {
            registered_at: 1,
            hypothesis: "A ≈ B".into(),
            primary_metrics: vec!["task_success".into()],
            equivalence_margin: None,
            min_n: 1,
            analysis_plan_ref: "plan-1".into(),
            task_split_hash: suite.manifest.split_hash.clone(),
            interactions: vec![],
            dead_weight_purpose: false,
        },
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn caps(dim: DimensionId, limit: i64) -> BudgetSpec {
    BudgetSpec::hard_caps(BudgetMode::Pool, &[(DimensionKey::Primary(dim), limit)])
}

fn native_arm(dim: DimensionId) -> ArmSpec {
    let b = caps(dim, 64);
    ArmSpec::native(b.clone(), b, MatchSpec::matched_cap(&[dim]))
}

/// A hosted arm whose `search_budget` is the first-class `unknown` — the
/// session-ABI row beside the native arm (enforcement still declares
/// `enforced` on the matched dimension — the M1 bar binds hosted arms too).
fn hosted_unknown_arm(dim: DimensionId) -> ArmSpec {
    let mut a = native_arm(dim);
    a.search_budget = None;
    a.enforcement = hh_budget::matchspec::BudgetEnforcement::hosted(&[(
        dim,
        hh_budget::EnforcementLevel::Enforced,
    )]);
    a
}

/// Two replicates of two parity-subset tasks per arm — a comparison-shaped
/// run set over real corpus task rows.
fn corpus_runs(suite: &BenchSuite, arm_a: &str, arm_b: &str) -> (Vec<EvalRun>, Vec<TaskContext>) {
    let mut runs = Vec::new();
    let mut tasks = Vec::new();
    for name in suite.parity_subset.iter().take(2) {
        let task = suite.task_named(name).expect("parity member in tasks");
        tasks.push(task_ctx(task, suite));
        for rep in 0..2u64 {
            let tid = &task.record().task_id;
            runs.push(eval_run(
                &format!("{arm_a}:{tid}:{rep}"),
                arm_a,
                task,
                suite,
                rep,
                1_000_000,
                2,
            ));
            runs.push(eval_run(
                &format!("{arm_b}:{tid}:{rep}"),
                arm_b,
                task,
                suite,
                rep,
                0,
                2,
            ));
        }
    }
    (runs, tasks)
}

fn compare_input<'a>(
    runs: &'a [EvalRun],
    tasks: &'a [TaskContext],
    design: &'a Design,
    arms: &'a [ArmSpec],
    decl: &'a hh_ontology::compliance::MetricDeclaration,
    metrics: &'a [String],
    granularity: Granularity,
) -> CompareInput<'a> {
    CompareInput {
        arm_a: "subject",
        arm_b: "baseline",
        metrics,
        declarations: std::slice::from_ref(decl),
        runs,
        tasks,
        design,
        arm_specs: arms,
        varied_factor: None,
        confidence_ppm: 950_000,
        benefit_kind: BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
        granularity,
    }
}

// ── combined failure (infra + budget exhaustion + verifier crash) ───────────

/// One fixture run carrying all three non-scored classes over a real corpus
/// task: the run stopped `infrastructure_failure`, its consumed vector sat
/// at the cap (`budget_exhausted` facts), and the verifier crashed
/// (`oracle_failed`) — the derived class is `oracle_failure` and the
/// comparison stratifies it beside, never inside, the estimate.
#[test]
fn combined_failure_fixture_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("c").unwrap();
    let decl = hh_eval::catalogue::metric("task_success").unwrap();
    let (mut runs, tasks) = corpus_runs(suite, "subject", "baseline");

    // The combined cell — the baseline's first replicate carries all three
    // failure facts at once.
    {
        let crashed = runs
            .iter_mut()
            .find(|r| r.arm_id == "baseline" && r.replicate_index == 0)
            .unwrap();
        crashed.outcome_class = hh_ontology::eval::derive_outcome_class(
            &StopReason::InfrastructureFailure {
                error_class: InfraError {
                    family: InfraErrorFamily::Env,
                    class: "sandbox_died".into(),
                },
            },
            true, // verifier crash — the oracle_failed fact
        );
        assert_eq!(crashed.outcome_class, OutcomeClass::OracleFailure);
        crashed.values.clear(); // a crashed verifier emits no value
        crashed.facts = LedgerFacts::from_events(&[
            // the budget ceiling fired mid-run (the budget-exhaustion half)
            (
                3u64,
                "control.budget.exhausted".to_string(),
                Json::obj([
                    ("budget_id", Json::str("b/run")),
                    ("dimension", Json::str("model_calls")),
                ]),
            ),
            (
                4u64,
                "lifecycle.run.finished".to_string(),
                Json::obj([("stop_reason", Json::str("infrastructure_failure"))]),
            ),
        ]);
    }

    let dim = DimensionId::ModelCalls;
    let arms = [native_arm(dim), native_arm(dim)];
    let design = paired_design(suite);
    let metrics = vec!["task_success".to_string()];
    let out = compare(&compare_input(
        &runs,
        &tasks,
        &design,
        &arms,
        &decl,
        &metrics,
        Granularity::ConfigurationLevel,
    ))
    .expect("the combined-failure cell still compares");
    assert_eq!(
        out.outcome_counts["baseline"]["oracle_failure"], 1,
        "the three-class failure lands oracle_failure, stratified"
    );
}

// ── `search_budget = unknown` over the corpus ───────────────────────────────

/// A hosted arm carrying `search_budget = unknown` beside the native arm:
/// `product-level` compare admits it flagged `search_unknown`; the same
/// pair at `configuration-level` refuses `UnbudgetedArm`. The unknown is
/// first-class — never a silent zero (ADR-0046 D1; CC9).
#[test]
fn unknown_search_budget_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("d").unwrap(); // tau_bench — the hosted-class family
    let decl = hh_eval::catalogue::metric("task_success").unwrap();
    let (runs, tasks) = corpus_runs(suite, "subject", "baseline");
    let dim = DimensionId::ModelCalls;
    let arms = [native_arm(dim), hosted_unknown_arm(dim)];
    let design = paired_design(suite);

    let metrics = vec!["task_success".to_string()];
    let out = compare(&compare_input(
        &runs,
        &tasks,
        &design,
        &arms,
        &decl,
        &metrics,
        Granularity::ProductLevel,
    ))
    .expect("product-level admits the unknown-search hosted arm");
    let report: &ComparisonReport = &out.reports[0];
    assert!(report.budget_match.search_unknown);
    assert!(
        ComparisonReport::from_json(&report.to_json())
            .unwrap()
            .budget_match
            .search_unknown,
        "the flag round-trips comparison_report/1"
    );

    let err = compare(&compare_input(
        &runs,
        &tasks,
        &design,
        &arms,
        &decl,
        &metrics,
        Granularity::ConfigurationLevel,
    ))
    .unwrap_err();
    assert!(matches!(
        err,
        CompareError::Match(hh_budget::MatchError {
            refusal: hh_budget::MatchRefusal::UnbudgetedArm,
            ..
        })
    ));
}

// ── `LeakedSplit` over the corpus ───────────────────────────────────────────

/// A corpus run whose prefix carries a real
/// `measurement.evolution.candidate.transitioned{to: proposed}` row before
/// the suite's `SplitAssignmentRecord.registered_at` — `artifact_benefit`
/// refuses `LeakedSplit`; and a run hash disagreeing with the suite's
/// committed `split_hash` refuses `SplitHashMismatch`.
#[test]
fn leaked_split_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("c").unwrap();
    let decl = hh_eval::catalogue::metric("task_success").unwrap();
    let (mut runs, tasks) = corpus_runs(suite, "subject", "baseline");

    // Arm A's runs searched before the split was committed: a real
    // `candidate.transitioned{to: proposed}` row lands at seq 5, and the
    // registered split postdates it (the corpus's own `registered_at`
    // wherever it sits — the refusal compares ordering, not magnitudes).
    let split_at = suite.split_assignment.registered_at;
    let first_proposed = split_at.saturating_sub(1).max(1);
    for r in runs.iter_mut().filter(|r| r.arm_id == "subject") {
        r.facts = LedgerFacts::from_events(&[
            (3u64, "lifecycle.run.created".to_string(), Json::obj([])),
            (
                first_proposed,
                "measurement.evolution.candidate.transitioned".to_string(),
                Json::obj([
                    ("candidate", Json::str("cand-1")),
                    ("to", Json::str("proposed")),
                ]),
            ),
        ]);
    }
    // `split_registered_at` one past the projected proposal seq — the split
    // postdated the search.
    let leaked_registered_at = first_proposed + 1;
    let dim = DimensionId::ModelCalls;
    let arms = [native_arm(dim), native_arm(dim)];
    let design = paired_design(suite);
    let metrics = vec!["task_success".to_string()];
    let inp = compare_input(
        &runs,
        &tasks,
        &design,
        &arms,
        &decl,
        &metrics,
        Granularity::ConfigurationLevel,
    );
    match artifact_benefit(&inp, leaked_registered_at) {
        Err(BenefitError::LeakedSplit {
            first_proposed_at,
            split_registered_at,
        }) => {
            assert_eq!(first_proposed_at, first_proposed);
            assert_eq!(split_registered_at, leaked_registered_at);
        }
        other => panic!("expected LeakedSplit, got {other:?}"),
    }
    // The clean side — a `registered_at` strictly before the projected
    // `first_proposed` passes the ordering gate.
    let mut clean_runs = runs.clone();
    for r in clean_runs.iter_mut().filter(|r| r.arm_id == "subject") {
        r.facts = LedgerFacts::from_events(&[(
            split_at + 10,
            "measurement.evolution.candidate.transitioned".to_string(),
            Json::obj([
                ("candidate", Json::str("cand-1")),
                ("to", Json::str("proposed")),
            ]),
        )]);
    }
    let inp = compare_input(
        &clean_runs,
        &tasks,
        &design,
        &arms,
        &decl,
        &metrics,
        Granularity::ConfigurationLevel,
    );
    artifact_benefit(&inp, split_at).expect("split predating the search is clean");

    // A disagreeing run hash refuses the L3 join.
    clean_runs[0].split_hash = Some("sha256:post-search-split".into());
    assert!(matches!(
        split_hash_agrees(&design.pre_registration, &clean_runs),
        Err(BenefitError::SplitHashMismatch { .. })
    ));
}

// ── HAL `FaultType` levels bind over the corpus ─────────────────────────────

/// Every `f/hal-*` catalogue profile binds as an `EvalRun.fault_profile`
/// level on a real corpus task row — the level survives the `eval_run/1`
/// codec (a bound environment-factor level is data on the run row).
#[test]
fn hal_levels_bind_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("a").unwrap();
    let task = suite.task_named(&suite.parity_subset[0]).unwrap();
    let hal_profiles: Vec<_> = hh_eval::stage3_fault_profiles()
        .into_iter()
        .filter(|p| p.profile_id.starts_with("f/hal-"))
        .collect();
    assert_eq!(hal_profiles.len(), 7, "the seven HAL levels");
    for p in &hal_profiles {
        let mut r = eval_run("r", "subject", task, suite, 0, 1_000_000, 2);
        r.fault_profile = Some(p.profile_id.clone());
        let decoded = EvalRun::from_json(&r.to_json()).expect("eval_run/1 decodes");
        assert_eq!(
            decoded.fault_profile.as_deref(),
            Some(p.profile_id.as_str()),
            "{} binds on a corpus row",
            p.profile_id
        );
        // The fault inside the profile is one of the seven HAL spellings.
        assert_eq!(p.faults.len(), 1);
        assert!(matches!(
            p.faults[0].fault_type,
            hh_eval::FaultType::HalTimeout
                | hh_eval::FaultType::HalErrorResponse
                | hh_eval::FaultType::HalPartialFailure
                | hh_eval::FaultType::HalRateLimit
                | hh_eval::FaultType::HalNetworkError
                | hh_eval::FaultType::HalInvalidResponse
                | hh_eval::FaultType::HalEmptyResponse
        ));
    }
}
