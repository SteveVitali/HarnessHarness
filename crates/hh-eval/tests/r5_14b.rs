//! R5.14b — the **compare consumer** battery (REQ-E-202/REQ-E-008
//! discharging leg; X7 SPLIT `07b6be09` leg (b)): `EvalRun.disposition`
//! is caller-set data — `disposition: "pending"` is the `grade_pending`
//! admission state and enters no denominator, whatever `outcome_class`
//! the record claims (the same `plan_disposition_at` admission decision
//! every consumer composes — never a divergent per-consumer predicate).
//! `disposition` absent preserves the legacy denominator byte-for-byte.
//! Fails-without: `EvalRun.disposition` and the `in_denominator` gate do
//! not exist at the R5.14a base — this file compiles red there.

use std::collections::{BTreeMap, BTreeSet};

use hharness_budget::matchspec::ArmSpec;
use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey, MatchSpec};
use hharness_eval::compare::{compare, CompareInput};
use hharness_eval::runs::{CacheState, EvalRun, TaskContext};
use hharness_ontology::compliance::MetricDeclaration;
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{
    Design, DesignKind, MetricValue, MetricValueKind, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hharness_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hharness_ontology::participant::{Observability, ParticipantClass};
use hharness_wire::json::Json;

fn name() -> String {
    "task_success".to_string()
}

fn catalogue() -> MetricDeclaration {
    hharness_eval::catalogue::metric("task_success").expect("catalogue metric")
}

fn run(arm: &str, task: &str, rep: u64, value: i64, disposition: Option<&str>) -> EvalRun {
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
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: rep,
        attempt_no: 1,
        seed: Some(42 + rep),
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        disposition: disposition.map(str::to_string),
        budget_consumed: [(DimensionId::ModelCalls, 5)].into_iter().collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(value > 0),
            applies_to: format!("r-{arm}-{task}-{rep}"),
            oracle_ref: "oracle/executable".into(),
            detector: hharness_ontology::compliance::Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
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

fn arm_spec() -> ArmSpec {
    let caps = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    ArmSpec::native(
        caps.clone(),
        caps,
        MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
    )
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
        benefit_kind: hharness_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
        granularity: hharness_ontology::participant::Granularity::ConfigurationLevel,
    }
}

// AC-2 (negative): a `disposition:"pending"` run enters no denominator —
// even carrying `outcome_class = scored` and a concrete metric value.
#[test]
fn pending_never_enters_a_denominator() {
    let decls = vec![catalogue()];
    let ts = tasks();
    let d = design();
    let arms = vec![arm_spec(), arm_spec()];

    // Baseline: B's t1 rep-1 run fails (value 0) — it denominates.
    let mut runs = Vec::new();
    for rep in 0..2 {
        for task in ["t1", "t2"] {
            runs.push(run("A", task, rep, 1, None));
            runs.push(run("B", task, rep, if task == "t2" { 1 } else { 0 }, None));
        }
    }
    let out = compare(&input(&runs, &ts, &d, &arms, &decls, &[name()])).unwrap();
    // t1: B denominates 2 runs, 0 successes.
    let t1 = out.per_task[0].iter().find(|e| e.task_id == "t1").unwrap();
    assert_eq!(t1.counts.1, (0, 2), "failing run denominates");

    // The same run marked `pending` — the admission state removes it
    // from the denominator entirely (never a scored interpretation,
    // never a 0 either).
    runs.iter_mut()
        .find(|r| r.run_id == "r-B-t1-1")
        .unwrap()
        .disposition = Some("pending".to_string());
    let out = compare(&input(&runs, &ts, &d, &arms, &decls, &[name()])).unwrap();
    let t1 = out.per_task[0].iter().find(|e| e.task_id == "t1").unwrap();
    assert_eq!(
        t1.counts.1,
        (0, 1),
        "pending never denominates — n dropped, never counted as success or failure"
    );
    // And it does not silently vanish — the compare's excluded-classes
    // accounting still sees the row's class beside the headline.
    let t2 = out.per_task[0].iter().find(|e| e.task_id == "t2").unwrap();
    assert_eq!(t2.counts.1, (2, 2), "non-pending runs unchanged");
}

// AC-2 (legacy): `disposition` absent preserves the denominator exactly —
// a run with `None` behaves byte-for-byte as before.
#[test]
fn disposition_absent_preserves_legacy_denominator() {
    let decls = vec![catalogue()];
    let ts = tasks();
    let d = design();
    let arms = vec![arm_spec(), arm_spec()];
    let runs: Vec<EvalRun> = (0..2)
        .flat_map(|rep| {
            ["t1", "t2"].iter().flat_map(move |t| {
                [
                    run("A", t, rep, 1, None),
                    run("B", t, rep, if *t == "t1" { 0 } else { 1 }, None),
                ]
            })
        })
        .collect();
    let out = compare(&input(&runs, &ts, &d, &arms, &decls, &[name()])).unwrap();
    let t1 = out.per_task[0].iter().find(|e| e.task_id == "t1").unwrap();
    assert_eq!(t1.counts.1, (0, 2));
    // A resolved disposition that is not "pending" denominates normally —
    // a graded row's class decides.
    let mut graded = runs.clone();
    for r in &mut graded {
        r.disposition = Some("graded".to_string());
    }
    let out2 = compare(&input(&graded, &ts, &d, &arms, &decls, &[name()])).unwrap();
    let t1b = out2.per_task[0].iter().find(|e| e.task_id == "t1").unwrap();
    assert_eq!(t1b.counts.1, (0, 2), "graded denominates by outcome_class");
}

// The records-in member: `disposition` round-trips; absent → `None`.
#[test]
fn disposition_member_round_trips() {
    let mut r = run("A", "t1", 0, 1, None);
    let j = r.to_json();
    assert!(j.get("disposition").is_none(), "absent member stays absent");
    let back = EvalRun::from_json(&j).unwrap();
    assert_eq!(back.disposition, None);
    r.disposition = Some("pending".to_string());
    let j = r.to_json();
    assert_eq!(j.get("disposition").and_then(Json::as_str), Some("pending"));
    let back = EvalRun::from_json(&j).unwrap();
    assert_eq!(back.disposition.as_deref(), Some("pending"));
    // An unknown member still refuses (closed codec).
    let mut j = r.to_json();
    if let Json::Obj(ref mut m) = j {
        m.insert("bogus_member".into(), Json::Bool(true));
    }
    assert!(EvalRun::from_json(&j).is_err());
}
