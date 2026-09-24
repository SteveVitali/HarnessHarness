//! hh-eval acceptance tests — `compare`, benefits, `equivalence_run`,
//! `render_scorecard`, the out-of-process oracle (AC-R-2.9.2-{4–13}).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

use hh_budget::matchspec::{ArmSpec, MatchMode, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::benefits::{artifact_benefit, equivalence_run, split_hash_agrees, BenefitError};
use hh_eval::compare::{compare, CompareError, CompareInput};
use hh_eval::runs::{CacheState, EvalRun, TaskContext};
use hh_eval::scorecard::{render_cell, render_scorecard, ScorecardError, ScorecardInput};
use hh_eval::{catalogue, stats};
use hh_ontology::compliance::{Detector, MetricDeclaration, NaReason};
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, MediationChannel, MetricValue, MetricValueKind, Pairing, PreRegistration,
    RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{CapabilityVerdict, Observability, ParticipantClass};
use hh_wire::Json;

const ORACLE_BIN: &str = env!("CARGO_BIN_EXE_hh-eval-oracle");

fn metric(name: &str) -> MetricDeclaration {
    catalogue::metric(name).expect("catalogue metric")
}

fn run(arm: &str, task: &str, rep: u64, value: i64, metric_name: &str) -> EvalRun {
    let mut r = EvalRun {
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
        suite_id: "suite-test".into(),
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
        }],
        environment_version_id: Some("env-1".into()),
        environment_family: EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: [("default".to_string(), "snap-1".to_string())]
            .into_iter()
            .collect(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        split_hash: Some("sha256:split-1".into()),
        facts: Default::default(),
    };
    r.run_id = format!("r-{arm}-{task}-{rep}");
    r
}

fn design() -> Design {
    Design {
        id: "design-1".into(),
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
            suite_id: "suite-test".into(),
            split_label: SplitLabel::HeldOut,
            split_hash: "sha256:split-1".into(),
            stratum: ContaminationStratum::PrivateHeldOut,
        })
        .collect()
}

fn arm_spec(dim: DimensionId) -> ArmSpec {
    let caps = BudgetSpec::hard_caps(BudgetMode::Pool, &[(DimensionKey::Primary(dim), 100)]);
    ArmSpec::native(caps.clone(), caps, MatchSpec::matched_cap(&[dim]))
}

fn input<'a>(
    runs: &'a [EvalRun],
    tasks: &'a [TaskContext],
    design: &'a Design,
    arms: &'a [ArmSpec],
    decls: &'a [MetricDeclaration],
    metrics: &'a [String],
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
    }
}

#[test]
fn compare_produces_paired_report() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, if task == "t1" { 1 } else { 0 }, &name));
        }
    }
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let ts = tasks();
    let d = design();
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .unwrap();
    assert_eq!(out.reports.len(), 1);
    let r = &out.reports[0];
    assert_eq!(r.metric, "task_success");
    assert_eq!(r.pairing, "by_task_and_replicate");
    assert_eq!(
        r.budget_match.status,
        hh_lab::analysis::BudgetMatchStatus::Matched
    );
    // A passes everywhere; B fails t2 → delta = 500_000 ppm (a − b).
    assert_eq!(
        r.paired_effect.point.as_ref().and_then(Json::as_int),
        Some(500_000)
    );
    assert!(r.held_out);
    assert_eq!(r.label, hh_lab::analysis::ReportLabelKind::Headlined);
    // Per-task effects carry (c, n) for both arms.
    let t2 = out.per_task[0].iter().find(|t| t.task_id == "t2").unwrap();
    assert_eq!(t2.counts, ((2, 2), (0, 2)));
}

#[test]
fn compare_refuses_exploratory_and_unmatched() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    let ts = tasks();
    let d = design();
    // No MatchSpec → MissingMatchSpec.
    let mut bad_arm = arm_spec(DimensionId::ModelCalls);
    bad_arm.match_spec = None;
    let arms = vec![bad_arm, arm_spec(DimensionId::ModelCalls)];
    let err = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .unwrap_err();
    assert!(matches!(err, CompareError::Match(_)));

    // mode = none → exploratory refusal.
    let mut expl = arm_spec(DimensionId::ModelCalls);
    expl.match_spec.as_mut().unwrap().mode = MatchMode::None;
    let mut expl2 = arm_spec(DimensionId::ModelCalls);
    expl2.match_spec.as_mut().unwrap().mode = MatchMode::None;
    let arms = vec![expl, expl2];
    let err = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .unwrap_err();
    match err {
        CompareError::Match(e) => assert_eq!(
            e.refusal,
            hh_budget::MatchRefusal::IncommensurableMatch {
                reason: hh_budget::RefusalReason::ExploratoryNoMatch,
                dimension: None,
                enforceability: None,
            }
        ),
        other => panic!("expected Match refusal, got {other}"),
    }

    // A non-comparable run refuses outright.
    let mut bad_runs = runs.clone();
    bad_runs[0].comparable = false;
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let err = compare(&input(&bad_runs, &ts, &d, &arms, &decls, &[name])).unwrap_err();
    assert!(matches!(err, CompareError::ExploratoryRun { .. }));
}

#[test]
fn compare_refuses_cross_arm_env_drift() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs[1].environment_version_id = Some("env-2".into()); // env differs, factor=model
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let ts = tasks();
    let d = design();
    let err = compare(&input(&runs, &ts, &d, &arms, &decls, &[name])).unwrap_err();
    assert!(matches!(
        err,
        CompareError::IncompatibleArms { ref field, .. } if field == "environment"
    ));
}

#[test]
fn na_never_coerces_to_zero() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    // Arm B's run emits no metric value → n/a{not_run}, never 0 in the cell.
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs[1].values.clear();
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let ts = tasks();
    let d = design();
    let out = compare(&input(&runs, &ts, &d, &arms, &decls, &[name])).unwrap();
    let cell = &out.per_task[0][0];
    assert!(matches!(cell.b, MetricValueKind::Na(_)));
}

#[test]
fn artifact_benefit_gates() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let ts = tasks();
    let d = design();
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    // A search-split run refuses artifact_benefit (not held-out).
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs[0].split_label = SplitLabel::Search;
    let err = artifact_benefit(
        &input(&runs, &ts, &d, &arms, &decls, std::slice::from_ref(&name)),
        0,
    )
    .unwrap_err();
    assert!(matches!(err, BenefitError::NotHeldOut { .. }));

    // Held-out with eval-phase search spend refuses.
    let mut runs2 = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs2[1].eval_search_spend = 5;
    let err = artifact_benefit(
        &input(&runs2, &ts, &d, &arms, &decls, std::slice::from_ref(&name)),
        0,
    )
    .unwrap_err();
    assert!(matches!(err, BenefitError::SearchSpendInEval { .. }));

    // Split hash disagreement refuses.
    let mut runs3 = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs3[0].split_hash = Some("sha256:other".into());
    let prereg = design().pre_registration;
    assert!(split_hash_agrees(&prereg, &runs3).is_err());
    assert!(split_hash_agrees(&prereg, &runs[..1]).is_ok());

    // AC-R-2.9.4-8 — a held-out split registered *after* the earliest
    // `transitioned{to: proposed}` in the run facts fails `LeakedSplit`
    // (the split hash must predate the search).
    let mut runs4 = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs4[0].split_label = SplitLabel::HeldOut;
    runs4[1].split_label = SplitLabel::HeldOut;
    runs4[0].facts.first_proposed_at = Some(5);
    let err = artifact_benefit(
        &input(&runs4, &ts, &d, &arms, &decls, std::slice::from_ref(&name)),
        10,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        BenefitError::LeakedSplit {
            first_proposed_at: 5,
            split_registered_at: 10
        }
    ));
    // Registered before the proposal — admissible.
    assert!(artifact_benefit(
        &input(&runs4, &ts, &d, &arms, &decls, std::slice::from_ref(&name)),
        4,
    )
    .is_ok());
}

#[test]
fn equivalence_run_three_valued() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 1, &name));
        }
    }
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let ts = tasks();
    let mut d = design();
    d.pre_registration.equivalence_margin = Some(Json::obj([(
        "margins",
        Json::Arr(vec![Json::obj([
            ("dimension", Json::str("task_success")),
            ("margin_ppm", Json::Int(900_000)),
            ("kind", Json::str("absolute")),
        ])]),
    )]));
    let rep = equivalence_run(
        "ref-participant",
        "arm-B",
        "suite-test",
        &d.pre_registration,
        &input(&runs, &ts, &d, &arms, &decls, &[name]),
        None,
    )
    .unwrap();
    // Identical arms, margin ±900_000 ppm → equivalent.
    assert_eq!(
        rep.verdict,
        hh_eval::benefits::EquivalenceVerdict::Equivalent
    );
    // Caller-supplied margins refuse (OQ-113 — margins come only from prereg).
    let err = equivalence_run(
        "ref",
        "arm-B",
        "suite-test",
        &d.pre_registration,
        &input(&runs, &ts, &d, &arms, &decls, &["task_success".to_string()]),
        Some(&[]),
    )
    .unwrap_err();
    assert!(matches!(err, BenefitError::MarginNotPreRegistered));
}

#[test]
fn scorecard_renders_and_refuses_unannotated_pooling() {
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let mut runs = Vec::new();
    for rep in 0..3 {
        runs.push(run("A", "t1", rep, 1, &name));
    }
    let ts = tasks();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let fams: BTreeMap<String, String> = BTreeMap::new();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    assert_eq!(report.configurations.len(), 1);
    let cell = &report.configurations[0].cells[0];
    assert_eq!(cell.point, MetricValueKind::Decimal(1_000_000));
    assert!(!cell.per_task.is_empty());
    assert!(report.scorecard_id.starts_with("sha256:") || !report.scorecard_id.is_empty());
    // Hash-stable: a second render of identical inputs is byte-identical.
    let report2 = render_scorecard(&inp).unwrap();
    assert_eq!(report.to_json(), report2.to_json());

    // Two strata + pool without annotation → refused.
    let mut mixed = runs.clone();
    mixed[0].stratum = ContaminationStratum::PublicDated;
    let inp2 = ScorecardInput {
        runs: &mixed,
        pool_strata: true,
        annotate_pooling: false,
        ..inp
    };
    match render_scorecard(&inp2) {
        Err(ScorecardError::StrataPooledUnannotated { .. }) => {}
        other => panic!("expected StrataPooledUnannotated, got {other:?}"),
    }
    // Annotated pooling renders.
    let inp3 = ScorecardInput {
        runs: &mixed,
        pool_strata: true,
        annotate_pooling: true,
        ..inp
    };
    assert!(render_scorecard(&inp3).is_ok());
}

#[test]
fn out_of_process_oracle_matches_in_process() {
    // AC-11: the same request through hh-eval-oracle returns the identical
    // verdict bytes.
    let decls = catalogue::oracle_declarations();
    let oracle = decls
        .iter()
        .find(|o| o.class == hh_ontology::eval::OracleClass::OutputCheck)
        .unwrap();
    let req = hh_eval::oracle::OracleRequest {
        oracle: oracle.clone(),
        criterion: "exact".into(),
        expected: Some(b"hello".to_vec()),
        actual: b"hello".to_vec(),
        transcript: None,
    };
    let in_proc = hh_eval::oracle::run_oracle(&req).unwrap();

    let mut child = Command::new(ORACLE_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn hh-eval-oracle");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(req.to_json().to_canonical_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    let j = hh_wire::parse(&text).unwrap();
    let verdict = hh_eval::oracle::OracleVerdict::from_json(&j).unwrap();
    assert_eq!(verdict, in_proc);
}

#[test]
fn pass_k_catalogue_metric_renders() {
    // pass^k on a k > n cell → n/a{estimator_undefined}, never 0.
    assert_eq!(stats::pass_k(0, 2, 3), None);
    assert_eq!(stats::pass_k(2, 3, 2), Some(333_333));
    assert_eq!(stats::pass_at_k(2, 3, 1), Some(666_667));
}

/// AC-R-2.9.2-1 — the `contrast` interaction estimate: 2 models × 2
/// variants (arms) × 3 tasks × n = 3 replicates. Under M1 the variant is a
/// clean win (A passes, B fails on every task); under M2 both arms pass
/// half the time — the interaction estimate must be positive with an
/// interval, and a metric absent from an outcome refuses typed.
#[test]
fn contrast_interaction_estimate() {
    use hh_eval::compare::contrast;
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts: Vec<TaskContext> = ["t1", "t2", "t3"]
        .iter()
        .map(|t| TaskContext {
            task_id: t.to_string(),
            suite_id: "suite-test".into(),
            split_label: SplitLabel::HeldOut,
            split_hash: "sha256:split-1".into(),
            stratum: ContaminationStratum::PrivateHeldOut,
        })
        .collect();
    let d = design();
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    // Model M1: A wins everywhere (delta +1M per task).
    let mut m1 = Vec::new();
    for t in ["t1", "t2", "t3"] {
        for rep in 0..3 {
            m1.push(run("A", t, rep, 1, &name));
            m1.push(run("B", t, rep, -1, &name));
        }
    }
    for r in &mut m1 {
        r.model_snapshots.insert("default".into(), "snap-m1".into());
    }
    // Model M2: no difference (both arms pass half the reps → delta 0).
    let mut m2 = Vec::new();
    for t in ["t1", "t2", "t3"] {
        for rep in 0..3 {
            m2.push(run("A", t, rep, if rep == 0 { -1 } else { 1 }, &name));
            m2.push(run("B", t, rep, if rep == 0 { -1 } else { 1 }, &name));
        }
    }
    for r in &mut m2 {
        r.model_snapshots.insert("default".into(), "snap-m2".into());
    }
    let metrics = vec![name.clone()];
    let out_m1 = compare(&input(&m1, &ts, &d, &arms, &decls, &metrics)).unwrap();
    let out_m2 = compare(&input(&m2, &ts, &d, &arms, &decls, &metrics)).unwrap();
    let est = contrast(&name, &out_m1, &out_m2, 950_000).unwrap();
    assert_eq!(est.n_tasks, 3);
    assert!(est.point > 0, "interaction should be positive: {est:?}");
    assert!(est.interval.lo <= est.point && est.point <= est.interval.hi);
    // A metric absent from the outcome refuses typed.
    match contrast("no_such_metric", &out_m1, &out_m2, 950_000) {
        Err(CompareError::UnknownMetric { .. }) => {}
        other => panic!("expected UnknownMetric, got {other:?}"),
    }
}

/// AC-R-2.9.2-10 — the portability cell: ≥ 2 snapshots across ≥ 2 declared
/// families plus a held-out level render `bool{true}`; a single-snapshot
/// configuration renders `n/a{not_run}`.
#[test]
fn portability_cell_gates_on_snapshots_families_held_out() {
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts = tasks();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    // Two snapshots, two families, held-out → the claim substantiates.
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("A", "t2", 0, 1, &name)];
    runs[1]
        .model_snapshots
        .insert("default".into(), "snap-other".into());
    let fams: BTreeMap<String, String> = [
        ("snap-1".to_string(), "fam-a".to_string()),
        ("snap-other".to_string(), "fam-b".to_string()),
    ]
    .into_iter()
    .collect();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    assert_eq!(
        report.configurations[0].portability,
        MetricValueKind::Bool(true)
    );
    // One snapshot → n/a{not_run}.
    let runs2 = vec![run("A", "t1", 0, 1, &name)];
    let inp2 = ScorecardInput {
        runs: &runs2,
        model_families: &fams,
        ..inp
    };
    let report2 = render_scorecard(&inp2).unwrap();
    assert!(matches!(
        report2.configurations[0].portability,
        MetricValueKind::Na(NaReason::NotRun)
    ));
}

/// AC-R-2.9.2-5 — an infrastructure failure, a budget exhaustion and an
/// oracle crash never enter the headline denominator silently: the cell
/// renders over the declared (scored) denominator with the three counts
/// beside it, and the reported rate is the denominator rate — never a
/// naive mean over all runs.
#[test]
fn outcome_classes_counted_beside_headline() {
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts = tasks();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("A", "t2", 0, -1, &name)];
    let mut infra = run("A", "t1", 1, 1, &name);
    infra.outcome_class = OutcomeClass::InfrastructureFailure;
    let mut budget = run("A", "t2", 1, -1, &name);
    budget.outcome_class = OutcomeClass::BudgetExhausted;
    let mut oracle = run("A", "t1", 2, 1, &name);
    oracle.outcome_class = OutcomeClass::OracleFailure;
    runs.extend([infra, budget, oracle]);
    let fams: BTreeMap<String, String> = BTreeMap::new();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    let cell = &report.configurations[0].cells[0];
    // ReportBeside classes are excluded and counted beside.
    assert_eq!(cell.excluded.get("infrastructure_failure"), Some(&1));
    assert_eq!(cell.excluded.get("oracle_failure"), Some(&1));
    // `budget_exhausted` is `count_as_failure` under the capability policy —
    // inside the declared denominator (t2's n covers the scored run + the
    // exhaustion), never a silent skip.
    let t2 = cell.per_task.iter().find(|t| t.task_id == "t2").unwrap();
    assert_eq!(t2.n, 2);
    assert_eq!(t2.c, 0);
    // The naive mean over all five runs is not what is reported — the
    // denominator is the policy's (scored + counted classes only).
    assert_eq!(cell.point, MetricValueKind::Decimal(500_000));
}

/// AC-R-2.9.2-9 — a veto-tripped success is excluded from the headline and
/// counted beside it (`vetoed` / `vetoed_successes`), never merged in.
#[test]
fn vetoed_success_excluded_from_headline() {
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts = tasks();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    // t1 passes clean; t2 "passes" but tripped secret_leak.
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("A", "t2", 0, 1, &name)];
    runs[1].veto_tripped = vec!["secret_leak".into()];
    let fams: BTreeMap<String, String> = BTreeMap::new();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    let cfg = &report.configurations[0];
    let cell = &cfg.cells[0];
    assert_eq!(cell.vetoed, 1, "the vetoed run is excluded from the cell");
    assert_eq!(cfg.vetoed_successes, 1, "counted beside the headline");
    // The headline reflects t1 only — the vetoed "success" never merges.
    assert_eq!(cell.point, MetricValueKind::Decimal(1_000_000));
}

/// AC-R-2.9.2-13 — a `judged` detector value against a deterministic-only
/// headline metric is a typed non-value (the cell renders `n/a`, never
/// the judged number); judged metrics render under their own names.
#[test]
fn judged_values_never_enter_the_headline() {
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts = tasks();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let mut runs = vec![run("A", "t1", 0, 1, &name)];
    runs[0].values[0].detector = Detector::Judged;
    let fams: BTreeMap<String, String> = BTreeMap::new();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    let cell = &report.configurations[0].cells[0];
    assert!(
        matches!(cell.point, MetricValueKind::Na(_)),
        "judged value must not feed the headline cell: {:?}",
        cell.point
    );
}

/// AC-R-2.9.4-11 — a stratum-C (`contaminated_public`) round-trip renders
/// labelled: the stratum is named on the report and a retired suite marks
/// the report `guarded`, never headline-clean.
#[test]
fn stratum_c_renders_labelled_and_retired() {
    let name = "task_success".to_string();
    let decls = vec![metric(&name)];
    let ts: Vec<TaskContext> = ["t1"]
        .iter()
        .map(|t| TaskContext {
            task_id: t.to_string(),
            suite_id: "suite-c".into(),
            split_label: SplitLabel::Public,
            split_hash: "sha256:split-c".into(),
            stratum: ContaminationStratum::ContaminatedPublic,
        })
        .collect();
    let suites = vec![hh_eval::runs::SuiteContext {
        suite_id: "suite-c".into(),
        retired_for_headline: true,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let mut runs = vec![run("A", "t1", 0, 1, &name)];
    runs[0].suite_id = "suite-c".into();
    runs[0].split_label = SplitLabel::Public;
    runs[0].stratum = ContaminationStratum::ContaminatedPublic;
    runs[0].split_hash = Some("sha256:split-c".into());
    let fams: BTreeMap<String, String> = BTreeMap::new();
    let inp = ScorecardInput {
        runs: &runs,
        tasks: &ts,
        suites: &suites,
        declarations: &decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    };
    let report = render_scorecard(&inp).unwrap();
    assert_eq!(
        report.strata,
        vec!["contaminated_public".to_string()],
        "the stratum is named"
    );
    assert_eq!(
        report.label,
        hh_lab::analysis::ReportLabelKind::Guarded,
        "a retired suite guards the report"
    );
}

#[test]
fn compare_refuses_cross_mode_match() {
    // CF-334 / OQ-363 — a `matched_cap` arm vs an `iso_cost` arm is
    // `IncommensurableMatch`, never compared (AC-R-2.10.3-4/E-4).
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    let ts = tasks();
    let d = design();
    let mut iso = arm_spec(DimensionId::ModelCalls);
    iso.match_spec.as_mut().unwrap().mode = MatchMode::IsoCost;
    iso.match_spec.as_mut().unwrap().pricing_table_ref =
        Some(hh_budget::pricing::PricingTableRef {
            table_id: "pricing:t".to_string(),
            version: "1".to_string(),
            pin: Some("sha256:p".to_string()),
        });
    let arms = vec![arm_spec(DimensionId::ModelCalls), iso];
    let err = compare(&input(&runs, &ts, &d, &arms, &decls, &[name])).unwrap_err();
    match err {
        CompareError::Match(e) => assert!(matches!(
            e.refusal,
            hh_budget::MatchRefusal::IncommensurableMatch {
                reason: hh_budget::RefusalReason::MixedMatchModes,
                ..
            }
        )),
        other => panic!("expected Match refusal, got {other}"),
    }
}

/// S3.4d (R-2.10.6⁰; ADR-0165 D6) — `applicability_at` on the scorecard path:
/// a hosted row without the required mediation renders `n/a{mediation}`,
/// native-only vetoes render `n/a{class}` on hosted rows, and a
/// `requires_capabilities` metric is `n/a{capability}` absent a `supported`
/// verdict (`unknown`/absent never coerced — T-LCD-07/T-LCD-15).
#[test]
fn hosted_applicability_mediation_and_capability_na() {
    let mut hosted = run("h", "t1", 0, 1, "task_success");
    hosted.participant_class = ParticipantClass::Hosted;
    hosted.observability_level = [Observability::Events, Observability::EndState]
        .into_iter()
        .collect();
    hosted.mediation = BTreeSet::new();

    // `veto.permission_violation` requires `mediated(effects)` (ADR-0165 D7):
    // an unmediated hosted row is `n/a{mediation}`, never 0.
    let veto = metric("veto.permission_violation");
    let cell = render_cell(&veto, &[&hosted], None, &BTreeMap::new(), 950_000, "cfg-h");
    assert_eq!(cell.point, MetricValueKind::Na(NaReason::Mediation));
    assert_eq!(cell.excluded.get("n/a:mediation"), Some(&1));
    // With `effects` mediated the metric applies (not n/a{mediation}).
    hosted.mediation.insert(MediationChannel::Effects);
    let cell = render_cell(&veto, &[&hosted], None, &BTreeMap::new(), 950_000, "cfg-h");
    assert_ne!(cell.point, MetricValueKind::Na(NaReason::Mediation));

    // `veto.duplicate_effect`/`veto.audit_completeness` are native-only —
    // `n/a{class}` on the hosted row (ADR-0165 D8).
    for name in ["veto.duplicate_effect", "veto.audit_completeness"] {
        let decl = metric(name);
        let cell = render_cell(&decl, &[&hosted], None, &BTreeMap::new(), 950_000, "cfg-h");
        assert_eq!(
            cell.point,
            MetricValueKind::Na(NaReason::Class),
            "{name} must be n/a{{class}} on a hosted row"
        );
    }

    // `requires_capabilities`: absent/`unknown` → `n/a{capability}`;
    // `supported` applies.
    let mut cap_decl = metric("task_success");
    cap_decl.requires_capabilities = ["subagents".to_string()].into_iter().collect();
    let cell = render_cell(
        &cap_decl,
        &[&hosted],
        None,
        &BTreeMap::new(),
        950_000,
        "cfg-h",
    );
    assert_eq!(cell.point, MetricValueKind::Na(NaReason::Capability));
    hosted
        .capability_vector
        .insert("subagents".into(), CapabilityVerdict::Unknown);
    let cell = render_cell(
        &cap_decl,
        &[&hosted],
        None,
        &BTreeMap::new(),
        950_000,
        "cfg-h",
    );
    assert_eq!(
        cell.point,
        MetricValueKind::Na(NaReason::Capability),
        "unknown never coerces to supported"
    );
    hosted
        .capability_vector
        .insert("subagents".into(), CapabilityVerdict::Supported);
    let cell = render_cell(
        &cap_decl,
        &[&hosted],
        None,
        &BTreeMap::new(),
        950_000,
        "cfg-h",
    );
    assert_ne!(cell.point, MetricValueKind::Na(NaReason::Capability));
}

// ── AC-R-2.3.1-10 / AC-R-2.3.2-3 / AC-R-2.3.4-6 — model-plane fact
// projection (`from_envelopes`) and `model_accounting` (S3.7) ──────────────

use hh_eval::accounting::{model_accounting, AccountingViolation};
use hh_eval::facts::LedgerFacts;

fn ev(seq: u64, id: &str, class: &str, payload: Json) -> (u64, String, String, Json) {
    (seq, id.to_string(), class.to_string(), payload)
}

fn model_ref(provider: &str) -> Json {
    Json::obj([
        ("profile_ref", Json::str("p@1")),
        ("provider_model_id", Json::str(provider)),
        ("serving_route", Json::str("route-1")),
    ])
}

/// A clean one-call run: requested → decided → attempt span → completed →
/// charge → spend (the §5b.1/§5b.4 accounting shape).
fn model_plane_run() -> Vec<(u64, String, String, Json)> {
    vec![
        ev(
            1,
            "e1",
            "model.call.requested",
            Json::obj([
                ("model_call_id", Json::str("c1")),
                ("purpose", Json::str("main")),
            ]),
        ),
        ev(
            2,
            "e2",
            "model.route.decided",
            Json::obj([
                ("model_call_id", Json::str("c1")),
                ("selected", model_ref("m-a")),
                ("deviation", Json::Bool(false)),
            ]),
        ),
        ev(
            3,
            "e3",
            "model.call.attempt.started",
            Json::obj([
                ("model_call_id", Json::str("c1")),
                ("attempt_no", Json::Int(1)),
            ]),
        ),
        ev(
            4,
            "e4",
            "model.call.attempt.completed",
            Json::obj([
                ("model_call_id", Json::str("c1")),
                ("attempt_no", Json::Int(1)),
            ]),
        ),
        ev(
            5,
            "e5",
            "model.call.completed",
            Json::obj([
                ("model_call_id", Json::str("c1")),
                ("served_model", Json::str("m-a")),
                ("timing", Json::obj([("latency_ms", Json::Int(10))])),
                (
                    "cache_observation",
                    Json::obj([("cache_read", Json::Int(120))]),
                ),
                (
                    "usage",
                    Json::obj([(
                        "record",
                        Json::obj([("view", Json::obj([("input_total", Json::Int(200))]))]),
                    )]),
                ),
            ]),
        ),
        ev(
            6,
            "e6",
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str("model_calls")),
                ("amount", Json::Int(1)),
                (
                    "attribution",
                    Json::obj([
                        ("charged_to", Json::str("subject")),
                        ("model_ref", model_ref("m-a")),
                    ]),
                ),
                ("source_event", Json::obj([("event_id", Json::str("e5"))])),
            ]),
        ),
        ev(
            7,
            "e7",
            "measurement.cost.attributed",
            Json::obj([
                ("subject_ref", Json::str("c1")),
                ("model_ref", model_ref("m-a")),
                (
                    "attribution",
                    Json::obj([("charged_to", Json::str("subject"))]),
                ),
                ("provenance", Json::str("measured")),
                ("money", Json::obj([("micro_units", Json::Int(5))])),
            ]),
        ),
    ]
}

#[test]
fn model_plane_facts_project_and_account_clean() {
    let f = LedgerFacts::from_envelopes(&model_plane_run());
    assert_eq!(f.attempts.len(), 2);
    assert_eq!(f.call_terminals.len(), 1);
    assert_eq!(f.call_terminals[0].cache_read, Some(120));
    assert_eq!(f.call_terminals[0].input_total, Some(200));
    assert_eq!(f.call_terminals[0].served_model.as_deref(), Some("m-a"));
    assert_eq!(f.routes.len(), 1);
    assert!(!f.routes[0].deviation);
    assert_eq!(f.charges.len(), 1);
    // The charge joined its call through `source_event.event_id → e5 → c1`.
    assert_eq!(f.charges[0].model_call_id.as_deref(), Some("c1"));
    assert_eq!(f.spend_rows.len(), 1);
    assert_eq!(f.spend_rows[0].micro_units, Some(5));
    assert!(f.cost_attributed_calls.contains("c1"));
    assert!(f.model_calls_completed.contains("c1"));
    // The clean run is the green state — no violations.
    assert_eq!(model_accounting(&f), Vec::<AccountingViolation>::new());
}

#[test]
fn model_accounting_flags_every_violation_kind() {
    // A missing charge → CallChargeCount{0}.
    let mut run = model_plane_run();
    run.retain(|(_, _, c, _)| c != "control.budget.consumed");
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::CallChargeCount { charges: 0, .. })));

    // A probe call charged to the subject → InstrumentCharge.
    let mut run = model_plane_run();
    run[0].3 = Json::obj([
        ("model_call_id", Json::str("c1")),
        ("purpose", Json::str("probe")),
    ]);
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::InstrumentCharge { .. })));

    // A charge priced at a different model than the route decision →
    // ChargeRouteMismatch (charge ≠ decision).
    let mut run = model_plane_run();
    run[5].3 = Json::obj([
        ("dimension", Json::str("model_calls")),
        ("amount", Json::Int(1)),
        (
            "attribution",
            Json::obj([
                ("charged_to", Json::str("subject")),
                ("model_ref", model_ref("m-b")),
            ]),
        ),
        ("source_event", Json::obj([("event_id", Json::str("e5"))])),
    ]);
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::ChargeRouteMismatch { .. })));

    // A charge with no charged_to → ChargedToMissing; a model-priced charge
    // whose source_event joins no call → ChargeSourceUnresolved.
    let mut run = model_plane_run();
    run[5].3 = Json::obj([
        ("dimension", Json::str("model_calls")),
        ("amount", Json::Int(1)),
        ("attribution", Json::obj([("model_ref", model_ref("m-a"))])),
        (
            "source_event",
            Json::obj([("event_id", Json::str("e-ghost"))]),
        ),
    ]);
    let f = LedgerFacts::from_envelopes(&run);
    let vs = model_accounting(&f);
    assert!(vs.iter().any(|v| matches!(
        v,
        AccountingViolation::ChargedToMissing {
            class: "charge",
            ..
        }
    )));
    assert!(vs
        .iter()
        .any(|v| matches!(v, AccountingViolation::ChargeSourceUnresolved { .. })));

    // A `served_from_cache` terminal without the resolved row / the
    // zero-amount cache_hit charge / the n/a timing → CacheServeUnaccounted.
    let mut run = model_plane_run();
    run[4].3 = Json::obj([
        ("model_call_id", Json::str("c1")),
        ("served_from_cache", Json::str("entry:k5-1")),
        ("timing", Json::str("n/a{not_run}")),
    ]);
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::CacheServeUnaccounted { .. })));

    // The fully-accounted K5 serve passes: resolved row naming the call,
    // zero-amount cache_hit charge, `timing = n/a{not_run}`.
    let mut run = model_plane_run();
    run[4].3 = Json::obj([
        ("model_call_id", Json::str("c1")),
        ("served_from_cache", Json::str("entry:k5-1")),
        ("timing", Json::str("n/a{not_run}")),
    ]);
    run.insert(
        5,
        ev(
            50,
            "e50",
            "model.cache.resolved",
            Json::obj([
                ("cache_kind", Json::str("k5")),
                ("key", Json::str("sha256:k5key")),
                ("outcome", Json::str("hit")),
                ("served_by", Json::str("c1")),
                ("attribution", Json::str("subject")),
                ("purpose", Json::str("main")),
                (
                    "avoided",
                    Json::obj([
                        ("spend", Json::obj([("micro_units", Json::Int(5))])),
                        ("provenance", Json::str("estimated_from_pricing")),
                    ]),
                ),
            ]),
        ),
    );
    run[6].3 = Json::obj([
        ("dimension", Json::str("model_calls")),
        ("amount", Json::Int(0)),
        (
            "attribution",
            Json::obj([
                ("charged_to", Json::str("subject")),
                ("model_ref", model_ref("m-a")),
                ("cache", Json::obj([("hit", Json::Bool(true))])),
            ]),
        ),
        ("source_event", Json::obj([("event_id", Json::str("e5"))])),
    ]);
    let f = LedgerFacts::from_envelopes(&run);
    assert_eq!(f.cache_resolutions.len(), 1);
    assert_eq!(model_accounting(&f), Vec::<AccountingViolation>::new());

    // An `avoided` estimate without estimated_from_pricing → AvoidedProvenance.
    let mut run = model_plane_run();
    run.insert(
        5,
        ev(
            50,
            "e50",
            "model.cache.resolved",
            Json::obj([
                ("cache_kind", Json::str("k4")),
                ("key", Json::str("sha256:k4key")),
                ("outcome", Json::str("hit")),
                ("attribution", Json::str("subject")),
                ("purpose", Json::str("main")),
                ("avoided", Json::obj([("spend", Json::obj([]))])),
            ]),
        ),
    );
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::AvoidedProvenance { .. })));

    // A malformed attempt span (two terminals, no started) → AttemptSpan.
    let mut run = model_plane_run();
    run.retain(|(_, _, c, _)| c != "model.call.attempt.started");
    let f = LedgerFacts::from_envelopes(&run);
    assert!(model_accounting(&f)
        .iter()
        .any(|v| matches!(v, AccountingViolation::AttemptSpan { .. })));
}

// ── AC-R-2.3.2-{8,9} — deviation pooling, replay refusal, outage counts ────

#[test]
fn deviation_policy_gates_pooling() {
    use hh_eval::compare::RemovalOutcome;
    use hh_ontology::eval::DeviationPolicy;
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let ts = tasks();
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 1, &name));
        }
    }
    // One B run carries routing.deviation.
    runs.iter_mut()
        .find(|r| r.arm_id == "B" && r.task_id == "t1" && r.replicate_index == 0)
        .unwrap()
        .routing_deviation = true;

    // fail_fast forbids deviation outright.
    let d = design();
    assert!(matches!(
        compare(&input(
            &runs,
            &ts,
            &d,
            &arms,
            &decls,
            std::slice::from_ref(&name)
        )),
        Err(CompareError::DeviationUnderFailFast { .. })
    ));

    let mut d = design();
    d.routing_policy = RoutingPolicy::Route("policy-x".into());
    // No declared policy → pooling refused.
    assert!(matches!(
        compare(&input(
            &runs,
            &ts,
            &d,
            &arms,
            &decls,
            std::slice::from_ref(&name)
        )),
        Err(CompareError::UnstratifiedDeviation)
    ));
    // exclude_deviated → the deviated row is excluded and counted beside.
    d.deviation_policy = Some(DeviationPolicy::ExcludeDeviated);
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("exclude_deviated compares");
    assert_eq!(out.deviation.excluded_runs.len(), 1);
    assert_eq!(out.deviation.arm_counts.get("B"), Some(&(1, 4)));
    // pool_with_flag → pooled estimate carries the flag.
    d.deviation_policy = Some(DeviationPolicy::PoolWithFlag);
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("pool_with_flag compares");
    assert!(out.deviation.pooled_with_flag);
    assert_eq!(out.reports.len(), 1);
    // stratify → one outcome per stratum, no pooled estimate; the deviated
    // stratum has only B rows → unpaired (recorded, never dropped).
    d.deviation_policy = Some(DeviationPolicy::Stratify);
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("stratify returns strata");
    assert!(out.reports.is_empty());
    assert_eq!(out.deviation.strata.len(), 2);
    let dev_stratum = out
        .deviation
        .strata
        .iter()
        .find(|s| s.deviated)
        .expect("deviated stratum recorded");
    assert!(dev_stratum.outcome.is_none(), "one-arm stratum is unpaired");
    let clean = out.deviation.strata.iter().find(|s| !s.deviated).unwrap();
    assert!(clean.outcome.is_some());

    // A replayed trajectory is a hard refusal (AC-R-2.3.2-9's replay arm).
    let mut runs = vec![run("A", "t1", 0, 1, &name), run("B", "t1", 0, 1, &name)];
    runs[1].replayed_trajectory = true;
    let d = design();
    assert!(matches!(
        compare(&input(
            &runs,
            &ts,
            &d,
            &arms,
            &decls,
            std::slice::from_ref(&name)
        )),
        Err(CompareError::ReplayedTrajectory { .. })
    ));

    // Outage counts: an infrastructure_failure under fail_fast counts in
    // `outcome_counts` beside the headline (AC-R-2.3.2-9).
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 1, &name));
        }
    }
    runs.iter_mut()
        .find(|r| r.arm_id == "B" && r.task_id == "t1" && r.replicate_index == 0)
        .unwrap()
        .outcome_class = OutcomeClass::InfrastructureFailure;
    let d = design();
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("compares");
    assert_eq!(
        out.outcome_counts
            .get("B")
            .and_then(|m| m.get("infrastructure_failure")),
        Some(&1)
    );
    let _ = RemovalOutcome::Pass; // referenced in the removal test below
}

// ── AC-R-2.3.3-8 — the removal verdict over a retirement comparison ────────

#[test]
fn removal_verdict_pass_fail_inconclusive() {
    use hh_eval::compare::{removal_verdict, RemovalOutcome};
    let name = "task_success".to_string();
    let decls = vec![metric("task_success")];
    let ts = tasks();
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let d = design();

    // P − R non-inferior: identical runs → pass.
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 1, &name));
        }
    }
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("compares");
    let v = removal_verdict("R1", &out, &decls, 100_000, 100_000);
    assert_eq!(v.verdict, RemovalOutcome::Pass);
    assert!(v.regressed_tasks.is_empty());

    // A reliable subset regresses (B fails t2 on every replicate) while the
    // mean improves nowhere — fail; the mean never rescues the subset.
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, if task == "t2" { 0 } else { 1 }, &name));
        }
    }
    let out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("compares");
    let v = removal_verdict("R1", &out, &decls, 100_000, 100_000);
    assert_eq!(v.verdict, RemovalOutcome::Fail);
    assert_eq!(v.regressed_tasks, vec!["t2".to_string()]);
    // The share counts unique tasks, not task-metric cells (1 of 2).
    assert_eq!(v.regressed_share_ppm, 500_000);

    // An undefined estimate never coerces to pass — inconclusive (use the
    // non-regressed equal-runs outcome so no per-task delta is defined).
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, &name));
            runs.push(run("B", task, rep, 1, &name));
        }
    }
    let mut out = compare(&input(
        &runs,
        &ts,
        &d,
        &arms,
        &decls,
        std::slice::from_ref(&name),
    ))
    .expect("compares");
    out.reports[0].paired_effect.point = None;
    out.reports[0].paired_effect.interval = None;
    let v = removal_verdict("R1", &out, &decls, 100_000, 100_000);
    assert!(matches!(v.verdict, RemovalOutcome::Inconclusive { .. }));
}

// ---------------------------------------------------------------------------
// AC-R-2.7.3-9 — `critic_experiment` runs `{on, off}` under a MatchSpec and
// reports the five deltas; the placement's removal_test points at it.
// ---------------------------------------------------------------------------

#[test]
fn critic_experiment_reports_five_deltas_under_matchspec() {
    use hh_eval::critic_experiment::{
        critic_experiment, CriticExperimentInput, CRITIC_EXPERIMENT_METRICS, ARM_OFF, ARM_ON,
    };
    let decls: Vec<MetricDeclaration> = CRITIC_EXPERIMENT_METRICS
        .iter()
        .map(|m| metric(m))
        .collect();
    // Runs: {off, on} × {t1, t2} × 2 replicates × 5 metrics. The on-arm
    // succeeds on t2 (task_success up), spends more verification (overhead
    // and evaluator_calls up), and reads lower on the error rates.
    let mut runs = Vec::new();
    for m in CRITIC_EXPERIMENT_METRICS {
        for rep in 0..2u64 {
            for task in ["t1", "t2"] {
                for arm in [ARM_OFF, ARM_ON] {
                    let mut r = run(arm, task, rep, 1, m);
                    r.run_id = format!("r-{arm}-{task}-{rep}-{m}");
                    r.values[0].value = match m {
                        "task_success" => {
                            MetricValueKind::Bool(!(arm == ARM_OFF && task == "t2"))
                        }
                        "harness_overhead.verification" => {
                            MetricValueKind::Decimal(if arm == ARM_ON { 120_000 } else { 100_000 })
                        }
                        "evaluator_calls" => {
                            MetricValueKind::Decimal(if arm == ARM_ON { 3 } else { 1 })
                        }
                        "false_stop_rate" => {
                            MetricValueKind::Decimal(if arm == ARM_ON { 0 } else { 200_000 })
                        }
                        _ => MetricValueKind::Decimal(if arm == ARM_ON { 50_000 } else { 300_000 }),
                    };
                    runs.push(r);
                }
            }
        }
    }
    let arms = vec![
        arm_spec(DimensionId::ModelCalls),
        arm_spec(DimensionId::ModelCalls),
    ];
    let ts = tasks();
    let d = design();
    let out = critic_experiment(&CriticExperimentInput {
        critic_ref: "deterministic:reconciliation@1",
        placement: "completion_gate",
        design: &d,
        arm_specs: &arms,
        runs: &runs,
        tasks: &ts,
        declarations: &decls,
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
    })
    .unwrap();
    // Five reports, in pinned order.
    assert_eq!(out.reports.len(), 5);
    let names: Vec<&str> = out.reports.iter().map(|r| r.metric.as_str()).collect();
    assert_eq!(names, CRITIC_EXPERIMENT_METRICS);
    // Δtask_success = on − off = +500_000 ppm (t2 recovered).
    assert_eq!(
        out.reports[0]
            .paired_effect
            .point
            .as_ref()
            .and_then(Json::as_int),
        Some(500_000)
    );
    // The overhead deltas are positive (the critic costs more
    // verification/evaluator calls); the error-rate deltas are negative.
    for (i, sign) in [(1usize, 1i64), (2, 1), (3, -1), (4, -1)] {
        let d_i = out.reports[i]
            .paired_effect
            .point
            .as_ref()
            .and_then(Json::as_int)
            .unwrap();
        assert_eq!(
            d_i.signum(),
            sign,
            "metric {} delta {d_i} sign",
            CRITIC_EXPERIMENT_METRICS[i]
        );
    }
    // The placement's removal_test names this experiment — removing the
    // critic while keeping the placement leaves a dangling handle.
    assert_eq!(
        out.removal_test,
        "critic_experiment:deterministic:reconciliation@1:completion_gate"
    );
    // Deterministic: identical inputs → identical reports.
    let out2 = critic_experiment(&CriticExperimentInput {
        critic_ref: "deterministic:reconciliation@1",
        placement: "completion_gate",
        design: &d,
        arm_specs: &arms,
        runs: &runs,
        tasks: &ts,
        declarations: &decls,
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
    })
    .unwrap();
    assert_eq!(out.reports, out2.reports);
    // A declaration gap refuses — never silently drops a required metric.
    let err = critic_experiment(&CriticExperimentInput {
        critic_ref: "deterministic:reconciliation@1",
        placement: "completion_gate",
        design: &d,
        arm_specs: &arms,
        runs: &runs,
        tasks: &ts,
        declarations: &decls[..3],
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
    })
    .unwrap_err();
    assert!(matches!(err, CompareError::UnknownMetric { .. }));
}
