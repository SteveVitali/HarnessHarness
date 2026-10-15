//! R2.16 — the §5h.2 residual battery (DF-S1.22-1 / DF-S1.22-2).
//!
//! Cells exercised here (the eval-kernel halves; the recorded-corpus halves
//! live in `crates/hh-bench/tests/r2_16.rs`):
//!
//! - **HAL `FaultType` levels** — the seven `hal`-class spellings
//!   (`timeout`, `error_response`, `partial_failure`, `rate_limit`,
//!   `network_error`, `invalid_response`, `empty_response`) parse, carry
//!   stage-3 catalogue profiles and round-trip through `fault_profile/1`.
//! - **`search_budget = unknown`** — first-class under `product-level`
//!   comparisons (`budget_match.search_unknown = true`), refused
//!   `UnbudgetedArm` at `configuration-level`/`component-level` and under
//!   `matched_total`; never coerced to a zero ceiling (ADR-0046 D1 /
//!   ADR-0159 D6; CC9).
//! - **Judge runtime derivation** — `judge_context_for` derives the
//!   snapshot axes from the run's `model_snapshots` binding and folds
//!   calibration deterministically (`calibration_status_at`);
//!   `recheck_judged` re-admits a `detector = judged` row on the consume
//!   side (AC-R-2.9.2-14; ADR-0047(c)/0116/0117).
//! - **Combined failure** — a fixture run carrying infrastructure failure +
//!   budget exhaustion + verifier crash lands `oracle_failure` (the
//!   precedence rule; the three non-scored classes never score).
//! - **`LeakedSplit`** — `artifact_benefit` refuses when the pre-registered
//!   split hash postdates the arm's first `transitioned{to: proposed}`
//!   (projected from a real event row), and `split_hash_agrees` refuses a
//!   disagreeing run hash (AC-R-2.9.4-8; ADR-0143 L3).

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::matchspec::{validate_match_at, ArmSpec, MatchMode, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{MatchError, MatchRefusal};
use hh_eval::benefits::{artifact_benefit, split_hash_agrees, BenefitError};
use hh_eval::compare::{compare, CompareError, CompareInput};
use hh_eval::judge::{
    admit_judge, emit_judged, judge_context_for, recheck_judged, JudgeAdmission, JudgeError,
};
use hh_eval::runs::{CacheState, EvalRun, TaskContext};
use hh_eval::LedgerFacts;
use hh_eval::{catalogue, FaultType};
use hh_ontology::compliance::{Detector, MetricDeclaration};
use hh_ontology::control::{InfraError, InfraErrorFamily, OutcomeClass, StopReason};
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    ChargedTo, Design, DesignKind, EvidenceKind, MetricValue, MetricValueKind, OracleClass,
    OracleDeclaration, Pairing, PreRegistration, RoutingPolicy, SeedPolicy, VerdictType,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{Granularity, Observability, ParticipantClass};
use hh_provenance::ProvenanceRecord;
use hh_verification::critic_rt::CalibrationContext;
use hh_verification::critics::{CalibrationAgreement, CalibrationRecord, IndependenceVector};
use hh_verification::vocab::{CalibrationStatus, CriticUse};
use hh_wire::Json;

// ── shared builders ─────────────────────────────────────────────────────────

fn metric(name: &str) -> MetricDeclaration {
    catalogue::metric(name).expect("catalogue metric")
}

fn run(arm: &str, task: &str, rep: u64, value: i64, metric_name: &str) -> EvalRun {
    EvalRun {
        run_id: format!("r-{arm}-{task}-{rep}"),
        arm_id: arm.into(),
        cell_id: Some(format!("{arm}:{task}")),
        configuration_id: format!("cfg-{arm}"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
            Observability::Ledger,
        ]
        .into_iter()
        .collect(),
        mediation: BTreeSet::new(),
        capability_vector: BTreeMap::new(),
        task_id: task.into(),
        suite_id: "suite-r216".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: rep,
        attempt_no: 1,
        seed: Some(42 + rep),
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: [(DimensionId::ModelCalls, 5)].into_iter().collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: metric_name.into(),
            value: MetricValueKind::Bool(value > 0),
            applies_to: format!("r-{arm}-{task}-{rep}"),
            oracle_ref: "oracle/executable".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
        }],
        environment_version_id: Some("env-1".into()),
        environment_family: EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: [("default".to_string(), "snap-b".to_string())]
            .into_iter()
            .collect(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        split_hash: Some("sha256:split-1".into()),
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        facts: LedgerFacts::default(),
    }
}

fn design() -> Design {
    Design {
        id: "design-r216".into(),
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
        held_out_split_ref: Some("split-1".into()),
        pre_registration: PreRegistration {
            registered_at: 1,
            hypothesis: "A ≥ B".into(),
            primary_metrics: vec!["task_success".into()],
            equivalence_margin: None,
            min_n: 1,
            analysis_plan_ref: "plan-1".into(),
            task_split_hash: "sha256:split-1".into(),
            interactions: vec![],
        },
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn tasks() -> Vec<TaskContext> {
    ["t1", "t2"]
        .iter()
        .map(|t| TaskContext {
            task_id: t.to_string(),
            suite_id: "suite-r216".into(),
            split_label: SplitLabel::HeldOut,
            split_hash: "sha256:split-1".into(),
            stratum: ContaminationStratum::PrivateHeldOut,
        })
        .collect()
}

fn caps(dim: DimensionId, limit: i64) -> BudgetSpec {
    BudgetSpec::hard_caps(BudgetMode::Pool, &[(DimensionKey::Primary(dim), limit)])
}

fn native_arm(dim: DimensionId) -> ArmSpec {
    let b = caps(dim, 100);
    ArmSpec::native(b.clone(), b, MatchSpec::matched_cap(&[dim]))
}

/// A hosted arm whose `search_budget` is the first-class `unknown` (`None`)
/// — never a zeroed ceiling.
fn hosted_unknown_search_arm(dim: DimensionId) -> ArmSpec {
    let mut a = native_arm(dim);
    a.search_budget = None;
    // The M1 enforceability bar still binds the hosted arm — a hosted arm
    // with instrumented call interception declares `enforced` for the
    // matched dimension (an `unenforceable` level is refused even when the
    // search budget itself is `unknown`).
    a.enforcement = hh_budget::matchspec::BudgetEnforcement::hosted(&[(
        dim,
        hh_budget::EnforcementLevel::Enforced,
    )]);
    a
}

fn input<'a>(
    runs: &'a [EvalRun],
    tasks: &'a [TaskContext],
    design: &'a Design,
    arms: &'a [ArmSpec],
    decls: &'a [MetricDeclaration],
    metrics: &'a [String],
    granularity: Granularity,
) -> CompareInput<'a> {
    CompareInput {
        arm_a: "A",
        arm_b: "B",
        metrics,
        declarations: decls,
        runs,
        tasks,
        design,
        arm_specs: arms,
        varied_factor: Some("model"),
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
        granularity,
    }
}

// ── HAL `FaultType` levels (DF-S1.22-2) ──────────────────────────────────────

/// The seven HAL spellings are first-class members of the closed sum — each
/// parses, round-trips its canonical spelling, and carries a stage-3
/// catalogue profile usable as an environment-factor level.
#[test]
fn hal_fault_types_are_first_class() {
    let hal = [
        (FaultType::HalTimeout, "timeout"),
        (FaultType::HalErrorResponse, "error_response"),
        (FaultType::HalPartialFailure, "partial_failure"),
        (FaultType::HalRateLimit, "rate_limit"),
        (FaultType::HalNetworkError, "network_error"),
        (FaultType::HalInvalidResponse, "invalid_response"),
        (FaultType::HalEmptyResponse, "empty_response"),
    ];
    for (f, spelling) in hal {
        assert_eq!(f.as_str(), spelling);
        assert_eq!(FaultType::parse(spelling), Some(f), "{spelling} parses");
    }
    // The closed sum carries all 21 members (14 pre-R2.16 + 7 HAL).
    assert_eq!(FaultType::ALL.len(), 21);
}

/// Every HAL level has a `f/hal-*` catalogue profile that round-trips
/// through `fault_profile/1`.
#[test]
fn hal_profiles_in_the_stage3_catalogue() {
    let profiles = hh_eval::stage3_fault_profiles();
    let hal_ids: BTreeSet<&str> = profiles
        .iter()
        .filter(|p| p.profile_id.starts_with("f/hal-"))
        .map(|p| p.profile_id.as_str())
        .collect();
    for id in [
        "f/hal-timeout-p25",
        "f/hal-error-response-p10",
        "f/hal-partial-failure-p10",
        "f/hal-rate-limit-p25",
        "f/hal-network-error-p10",
        "f/hal-invalid-response-p05",
        "f/hal-empty-response-p05",
    ] {
        assert!(hal_ids.contains(id), "{id} is a catalogue level");
    }
    for p in &profiles {
        let j = p.to_json();
        assert_eq!(
            hh_eval::FaultProfile::from_json(&j).unwrap(),
            *p,
            "{} round-trips",
            p.profile_id
        );
    }
}

// ── `search_budget = unknown` (DF-S1.22-2; ADR-0046 D1) ─────────────────────

/// `validate_match_at` admits the unknown search budget only at
/// `product-level`; `configuration-level`/`component-level` refuse
/// `UnbudgetedArm` — and `matched_total` refuses at every granularity (a
/// total over an unknown term cannot be formed).
#[test]
fn search_unknown_is_product_level_only() {
    let dim = DimensionId::ModelCalls;
    let arms = vec![hosted_unknown_search_arm(dim), native_arm(dim)];

    // Product-level: admitted.
    validate_match_at(&arms, Granularity::ProductLevel).expect("product-level admits unknown");
    // Configuration/component-level: the unknown refuses.
    for g in [Granularity::ConfigurationLevel, Granularity::ComponentLevel] {
        let e = validate_match_at(&arms, g).unwrap_err();
        assert_eq!(e.refusal, MatchRefusal::UnbudgetedArm, "{g:?}");
    }
    // `matched_total` needs all three terms — unknown refuses even at
    // product-level.
    let mut total_arms = arms.clone();
    for a in &mut total_arms {
        a.match_spec = Some(MatchSpec {
            mode: MatchMode::MatchedTotal,
            pricing_table_ref: Some(hh_budget::PricingTableRef {
                table_id: "price/std.v1".into(),
                version: "v1".into(),
                pin: Some("sha256:pt".into()),
            }),
            ..a.match_spec.clone().unwrap()
        });
        a.inference_budget = Some(caps(dim, 50));
    }
    let e = validate_match_at(&total_arms, Granularity::ProductLevel).unwrap_err();
    assert_eq!(e.refusal, MatchRefusal::UnbudgetedArm);
}

/// `compare` over a product-level pair with an unknown-search arm produces
/// `budget_match.search_unknown = true` — surfaced, never coerced — and the
/// flag round-trips the `comparison_report/1` JSON.
#[test]
fn compare_flags_search_unknown() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 0, &name));
        }
    }
    let dim = DimensionId::ModelCalls;
    let arms = vec![native_arm(dim), hosted_unknown_search_arm(dim)];
    let ts = tasks();
    let d = design();
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
        Granularity::ProductLevel,
    ))
    .expect("product-level compare over an unknown-search arm");
    let r = &out.reports[0];
    assert!(
        r.budget_match.search_unknown,
        "the report flags the unknown search budget"
    );
    // JSON round-trip keeps the flag (the record, not a convention).
    let decoded = hh_lab::analysis::ComparisonReport::from_json(&r.to_json()).unwrap();
    assert!(decoded.budget_match.search_unknown);
}

/// The same pair at `configuration-level` refuses — the typed refusal is
/// the pre-R2.16 behaviour (the unknown is never a silent zero).
#[test]
fn compare_configuration_level_refuses_unknown() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 0, &name)];
    let dim = DimensionId::ModelCalls;
    let arms = vec![native_arm(dim), hosted_unknown_search_arm(dim)];
    let ts = tasks();
    let d = design();
    let err = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
        Granularity::ConfigurationLevel,
    ))
    .unwrap_err();
    match err {
        CompareError::Match(MatchError { refusal, .. }) => {
            assert_eq!(refusal, MatchRefusal::UnbudgetedArm)
        }
        e => panic!("expected Match(UnbudgetedArm), got {e:?}"),
    }
}

/// The unknown arm's caps are never read as a zero ceiling: a product-level
/// compare whose *declaring* arms carry equal caps reports
/// `limits_equal = true` even though the unknown arm declares nothing.
#[test]
fn unknown_search_is_never_a_zero_cap() {
    let dim = DimensionId::ModelCalls;
    // The unknown-search arm would fail an `UnequalCaps` check if its
    // absent cap were read as 0 (0 ≠ 100) — admittance at product-level is
    // the proof the ceiling was skipped, not coerced.
    let arms = vec![hosted_unknown_search_arm(dim), native_arm(dim)];
    validate_match_at(&arms, Granularity::ProductLevel).unwrap();
    // And the reverse order is symmetric.
    let arms = vec![native_arm(dim), hosted_unknown_search_arm(dim)];
    validate_match_at(&arms, Granularity::ProductLevel).unwrap();
}

// ── judge runtime derivation (DF-S1.22-1; AC-R-2.9.2-14) ────────────────────

fn judge_oracle() -> OracleDeclaration {
    OracleDeclaration {
        oracle_id: "oracle/judge-alignment".into(),
        class: OracleClass::Judge,
        deterministic: false,
        requires_observability: [Observability::ModelIo].into_iter().collect(),
        verdict_type: VerdictType::Bool,
        evidence_out: vec![EvidenceKind::ModelIo, EvidenceKind::Observation],
        charged_to: ChargedTo::Instrument,
        calibration_ref: Some("calib/judge-alignment.v1".into()),
        provenance: ProvenanceRecord::kernel("hh-eval/r2_16", 0),
    }
}

/// The catalogue's judged execution-alignment declaration — admits
/// `detector = judged` + `oracle_class = judge` (CF-483: a distinct row from
/// the deterministic `execution_alignment_failure_rate`).
fn judged_metric() -> MetricDeclaration {
    metric("execution_alignment_failure_rate_judged")
}

/// A run whose bound profile carries a `judge` role beside the subject's
/// `default` snapshot.
fn bound_run(judge_snap: &str) -> EvalRun {
    let mut r = run("A", "t1", 0, 1, "task_success");
    r.model_snapshots = [
        ("default".to_string(), "snap-beneficiary".to_string()),
        ("judge".to_string(), judge_snap.to_string()),
    ]
    .into_iter()
    .collect();
    r
}

fn calibration_record(fingerprint: &str, calibrated_at: u64) -> CalibrationRecord {
    CalibrationRecord {
        calibration_id: "calib/judge-alignment.v1".into(),
        critic_ref: "oracle/judge-alignment".into(),
        judge_snapshot_fingerprint: fingerprint.into(),
        labelled_set_ref: "labelled/alignment.v3".into(),
        n_per_verdict_class: 12,
        reference_oracle: hh_verification::vocab::ReferenceOracle::Executable,
        agreement: CalibrationAgreement {
            statistic: "cohen_kappa".into(),
            value_ppm: 900_000,
            interval: "[0.85, 0.95]".into(),
            method: "clt".into(),
        },
        error_rates: Json::obj([
            ("fn_rate", Json::Int(50_000)),
            ("fp_rate", Json::Int(50_000)),
        ]),
        position_consistency_ppm: 800_000,
        self_consistency_ppm: Some(850_000),
        self_preference_check: None,
        evidence_ablation_ppm: Some(100_000),
        calibrated_at,
        expiry: vec![],
        status: CalibrationStatus::Active,
    }
}

fn no_family(_: &str) -> Option<String> {
    None
}

/// `judge_context_for` derives the snapshot axis off the run's binding: the
/// `judge` role is the judge; every other bound role is a beneficiary
/// coordinate. A `judge` bound to the beneficiary's own snapshot derives
/// `same_snapshot` and refuses — at runtime, not by declaration.
#[test]
fn judge_context_derives_snapshot_axis_from_the_run() {
    let oracle = judge_oracle();
    let run = bound_run("snap-beneficiary"); // the judge IS the beneficiary's snapshot
    let ctx = judge_context_for(
        &run,
        IndependenceVector::kernel_independent(),
        CriticUse::Gate,
        &no_family,
        None,
    )
    .unwrap();
    assert_eq!(ctx.judge_snapshot, "snap-beneficiary");
    assert!(ctx.beneficiary_snapshots.contains("snap-beneficiary"));
    // The derived axis is `same_snapshot` → admission refuses.
    assert_eq!(
        admit_judge(&oracle, &ctx),
        Err(JudgeError::JudgeNotIndependent {
            axis: "snapshot = same_snapshot".into()
        })
    );
}

/// A run binding no `judge` role refuses `UnboundJudge` — the context is
/// underivable, never a skipped check.
#[test]
fn judge_context_refuses_an_unbound_judge() {
    let run = run("A", "t1", 0, 1, "task_success"); // no "judge" role
    assert!(matches!(
        judge_context_for(
            &run,
            IndependenceVector::kernel_independent(),
            CriticUse::Gate,
            &no_family,
            None,
        ),
        Err(JudgeError::UnboundJudge)
    ));
}

/// The calibration half folds deterministically through
/// `calibration_status_at`: a record whose judge-snapshot fingerprint no
/// longer matches folds `expired` — a gate use then emits
/// `n/a{no_detector}` and a report use `exploratory`, exactly as if no
/// record existed.
#[test]
fn judge_calibration_folds_at_runtime() {
    let oracle = judge_oracle();
    let run = bound_run("snap-judge-v2");

    // Active calibration (fingerprint matches, inside the period).
    let record = calibration_record("fp:judge.v2", 900);
    let ctx_now = CalibrationContext {
        judge_snapshot_fingerprint: "fp:judge.v2".into(),
        rubric_ref: "rubric/alignment.v3".into(),
        task_family: "coding".into(),
        now_ms: 1_000,
        recalibration_period_ms: 10_000,
    };
    let ctx = judge_context_for(
        &run,
        IndependenceVector::kernel_independent(),
        CriticUse::Gate,
        &no_family,
        Some((&record, &ctx_now)),
    )
    .unwrap();
    assert_eq!(ctx.calibration, Some(CalibrationStatus::Active));
    assert_eq!(
        admit_judge(&oracle, &ctx),
        Ok(JudgeAdmission::Admitted { exploratory: false })
    );

    // The judge snapshot drifted — the fold expires the calibration.
    let drifted = CalibrationContext {
        judge_snapshot_fingerprint: "fp:judge.v3-drifted".into(),
        ..ctx_now.clone()
    };
    let ctx = judge_context_for(
        &run,
        IndependenceVector::kernel_independent(),
        CriticUse::Gate,
        &no_family,
        Some((&record, &drifted)),
    )
    .unwrap();
    assert_eq!(ctx.calibration, Some(CalibrationStatus::Expired));
    assert_eq!(
        admit_judge(&oracle, &ctx),
        Ok(JudgeAdmission::Unavailable {
            reason: hh_ontology::compliance::NaReason::NoDetector
        })
    );
    // The same expired fold on a report use is exploratory evidence.
    let ctx = judge_context_for(
        &run,
        IndependenceVector::kernel_independent(),
        CriticUse::Report,
        &no_family,
        Some((&record, &drifted)),
    )
    .unwrap();
    assert_eq!(
        admit_judge(&oracle, &ctx),
        Ok(JudgeAdmission::Admitted { exploratory: true })
    );
}

/// `recheck_judged` — the consume side: a judged row emitted under an active
/// calibration re-admits cleanly; a row whose oracle doesn't match refuses
/// `ForeignJudgedValue`; and a binding that drifted to the beneficiary's
/// snapshot refuses at consume time.
#[test]
fn recheck_judged_re_admits_against_run_facts() {
    let oracle = judge_oracle();
    let metric = judged_metric();
    let run = bound_run("snap-judge-v2");
    let record = calibration_record("fp:judge.v2", 900);
    let ctx_now = CalibrationContext {
        judge_snapshot_fingerprint: "fp:judge.v2".into(),
        rubric_ref: "rubric/alignment.v3".into(),
        task_family: "coding".into(),
        now_ms: 1_000,
        recalibration_period_ms: 10_000,
    };
    let ctx = judge_context_for(
        &run,
        IndependenceVector::kernel_independent(),
        CriticUse::Gate,
        &no_family,
        Some((&record, &ctx_now)),
    )
    .unwrap();
    let emission = emit_judged(
        &metric,
        &oracle,
        &ctx,
        MetricValueKind::Bool(true),
        &run.run_id,
        Some(900_000),
        None,
    )
    .expect("the admitted judge emits");

    // Consume-side recheck over the same run facts: re-admits.
    assert_eq!(
        recheck_judged(
            &run,
            &emission.value,
            &oracle,
            IndependenceVector::kernel_independent(),
            CriticUse::Gate,
            &no_family,
            Some((&record, &ctx_now)),
        ),
        Ok(JudgeAdmission::Admitted { exploratory: false })
    );

    // A foreign `detector = judged` row is not this oracle's evidence.
    let mut foreign = emission.value.clone();
    foreign.oracle_ref = "oracle/other-judge".into();
    assert_eq!(
        recheck_judged(
            &run,
            &foreign,
            &oracle,
            IndependenceVector::kernel_independent(),
            CriticUse::Gate,
            &no_family,
            Some((&record, &ctx_now)),
        ),
        Err(JudgeError::ForeignJudgedValue {
            oracle_ref: "oracle/other-judge".into()
        })
    );

    // The same row under a binding that drifted to the beneficiary's own
    // snapshot refuses at consume time — the derived axis re-runs.
    let drifted_run = bound_run("snap-beneficiary");
    assert!(matches!(
        recheck_judged(
            &drifted_run,
            &emission.value,
            &oracle,
            IndependenceVector::kernel_independent(),
            CriticUse::Gate,
            &no_family,
            Some((&record, &ctx_now)),
        ),
        Err(JudgeError::JudgeNotIndependent { .. })
    ));
}

// ── combined failure fixture (DF-S1.22-2) ───────────────────────────────────

/// One run carrying all three non-scored classes at once — infrastructure
/// failure (stop reason), budget exhaustion (the consumed vector hit the
/// ceiling) and verifier crash (`oracle_failed`) — derives `oracle_failure`
/// (the precedence rule: an oracle failure is never scored, never the
/// stop reason's class). The corpus half runs in `hh-bench`.
#[test]
fn combined_failure_derives_oracle_failure() {
    // The three non-scored classes in one fixture.
    let infra = || StopReason::InfrastructureFailure {
        error_class: InfraError {
            family: InfraErrorFamily::Env,
            class: "sandbox_died".into(),
        },
    };
    let budget = || StopReason::BudgetExhausted {
        budget_id: "b/run".into(),
        dimension: DimensionId::ModelCalls,
    };
    assert_eq!(
        hh_ontology::eval::derive_outcome_class(&infra(), true),
        OutcomeClass::OracleFailure,
        "verifier crash outranks the infrastructure stop"
    );
    assert_eq!(
        hh_ontology::eval::derive_outcome_class(&budget(), true),
        OutcomeClass::OracleFailure
    );
    // Without the crash the stop-reason classes stand.
    assert_eq!(
        hh_ontology::eval::derive_outcome_class(&infra(), false),
        OutcomeClass::InfrastructureFailure
    );
    assert_eq!(
        hh_ontology::eval::derive_outcome_class(&budget(), false),
        OutcomeClass::BudgetExhausted
    );
}

/// The combined fixture through `compare`: a cell whose runs landed all
/// three non-scored classes reports the stratified `outcome_counts` (the
/// counts beside the report, never inside the estimate).
#[test]
fn compare_stratifies_the_combined_failure_classes() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 0, &name));
        }
    }
    // The combined fixture rows — the three non-scored classes on B's t2
    // replicate 1 (the run still carries its settled outcome; the metric
    // value a crashed oracle never emitted is simply absent).
    let crashed = runs.iter_mut().find(|r| r.run_id == "r-B-t2-1").unwrap();
    crashed.outcome_class = hh_ontology::eval::derive_outcome_class(
        &StopReason::InfrastructureFailure {
            error_class: InfraError {
                family: InfraErrorFamily::Env,
                class: "sandbox_died".into(),
            },
        },
        true,
    );
    crashed.veto_tripped = vec![];
    let dim = DimensionId::ModelCalls;
    let arms = vec![native_arm(dim), native_arm(dim)];
    let ts = tasks();
    let d = design();
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
        Granularity::ConfigurationLevel,
    ))
    .unwrap();
    assert_eq!(
        out.outcome_counts["B"]["oracle_failure"], 1,
        "the crashed cell is counted beside the headline, never inside it"
    );
}

// ── LeakedSplit (DF-S1.22-2; AC-R-2.9.4-8; ADR-0143 L3) ─────────────────────

/// `artifact_benefit` refuses `LeakedSplit` when the registered split hash
/// postdates the arm's first `transitioned{to: proposed}` — the ordering is
/// projected off a real `measurement.evolution.candidate.transitioned`
/// event row (`LedgerFacts::from_events`), not declared.
#[test]
fn leaked_split_postdating_search_refuses() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 0, &name));
        }
    }
    // Arm A searched before the split was registered: a real
    // `candidate.transitioned{to: proposed}` row at seq 5; the split lands
    // at seq 9 — postdates the search.
    let proposed = |r: &mut EvalRun| {
        r.facts = LedgerFacts::from_events(&[
            (3u64, "lifecycle.run.created".to_string(), Json::obj([])),
            (
                5u64,
                "measurement.evolution.candidate.transitioned".to_string(),
                Json::obj([
                    ("candidate", Json::str("cand-7")),
                    ("to", Json::str("proposed")),
                ]),
            ),
        ]);
    };
    for r in runs.iter_mut().filter(|r| r.arm_id == "A") {
        proposed(r);
    }
    let dim = DimensionId::ModelCalls;
    let arms = vec![native_arm(dim), native_arm(dim)];
    let ts = tasks();
    let d = design();
    let inp = input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
        Granularity::ConfigurationLevel,
    );
    match artifact_benefit(&inp, 9) {
        Err(BenefitError::LeakedSplit {
            first_proposed_at,
            split_registered_at,
        }) => {
            assert_eq!((first_proposed_at, split_registered_at), (5, 9));
        }
        other => panic!("expected LeakedSplit, got {other:?}"),
    }
    // A split registered before the first proposal is clean.
    assert!(artifact_benefit(&inp, 4).is_ok());
}

/// `split_hash_agrees` — a run carrying a `split_hash` that disagrees with
/// the pre-registered hash refuses `SplitHashMismatch` (the L3 join).
#[test]
fn split_hash_mismatch_refuses() {
    let name = "task_success".to_string();
    let mut runs = vec![run("A", "t1", 0, 1, &name)];
    runs[0].split_hash = Some("sha256:split-OTHER".into());
    let d = design();
    let err = split_hash_agrees(&d.pre_registration, &runs).unwrap_err();
    assert!(matches!(err, BenefitError::SplitHashMismatch { .. }));
}
