//! R5.15 — the producer-side reconcile battery: `ExperimentView` plans
//! + `EvalRun`s → the eval planned-population contract (hh-analysis
//! `population::planned_population`). Fails-without: the module does
//! not exist at the R5.14b base — compiles red there.

use std::collections::{BTreeMap, BTreeSet};

use hharness_analysis::population::planned_population;
use hharness_eval::runs::{CacheState, EvalRun};
use hharness_experiment::view::{Attempt, ExperimentView, PlanState};
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{MetricValue, MetricValueKind};
use hharness_ontology::lab::{ContaminationStratum, SplitLabel};
use hharness_ontology::participant::{Observability, ParticipantClass};

fn attempt(run: &str, no: u32, oc: OutcomeClass) -> Attempt {
    Attempt {
        run_id: run.into(),
        attempt_no: no,
        budget_id: "b".into(),
        bound: true,
        outcome: Some(oc),
        accepted: false,
        superseded: false,
        excluded: None,
        veto_tripped: vec![],
        budget_utilization: None,
        regrade_pending: false,
        pools: vec![],
    }
}

fn plan(rpid: &str, task: &str, arm: &str, attempts: Vec<Attempt>) -> PlanState {
    PlanState {
        run_plan_id: rpid.into(),
        cell_id: format!("{arm}:{task}"),
        arm_id: arm.into(),
        task_id: task.into(),
        replicate_index: 0,
        configuration_version_id: "cfg".into(),
        split_label: "held_out".into(),
        seed_material: hharness_wire::Json::Null,
        order_pos: 0,
        claim: None,
        attempts,
        next_attempt_no: 1,
        not_before_ms: 0,
        accepted_run: None,
        finished_final: false,
        launch_binding: None,
        grade_pending: false,
        disposition: None,
    }
}

fn run(run_id: &str, arm: &str, task: &str, oc: OutcomeClass, pass: bool) -> EvalRun {
    EvalRun {
        run_id: run_id.into(),
        arm_id: arm.into(),
        cell_id: None,
        configuration_id: "cfg".into(),
        participant_class: ParticipantClass::Native,
        observability_level: [Observability::Ledger].into_iter().collect(),
        mediation: BTreeSet::new(),
        capability_vector: BTreeMap::new(),
        task_id: task.into(),
        suite_id: "suite".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: 0,
        attempt_no: 1,
        seed: None,
        seed_honoured: false,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: oc,
        disposition: None,
        budget_consumed: BTreeMap::new(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(pass),
            applies_to: run_id.into(),
            oracle_ref: "o".into(),
            detector: hharness_ontology::compliance::Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
        }],
        environment_version_id: Some("env".into()),
        environment_family: hharness_ontology::lab::EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: BTreeMap::new(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        split_hash: None,
        facts: Default::default(),
    }
}

#[test]
fn adapter_reconciles_planned_slots_first_subjects_and_attempts() {
    let mut view = ExperimentView::default();
    view.plans.insert(
        "p1".into(),
        plan(
            "p1",
            "t0",
            "A",
            vec![
                attempt("r-fail", 1, OutcomeClass::OracleFailure),
                attempt("r-retry", 2, OutcomeClass::Scored),
            ],
        ),
    );
    view.plans.insert(
        "p2".into(),
        plan(
            "p2",
            "t0",
            "B",
            vec![attempt("r-b", 1, OutcomeClass::Scored)],
        ),
    );
    view.plans.insert(
        "p3".into(),
        plan("p3", "t0", "B", vec![]), // planned, never bound → not-run
    );
    let runs = vec![
        run("r-retry", "A", "t0", OutcomeClass::Scored, true),
        run("r-b", "B", "t0", OutcomeClass::Scored, false),
    ];
    let mut pins = BTreeMap::new();
    pins.insert("r-retry".to_string(), 9u64);
    pins.insert("r-b".to_string(), 7u64);
    let pop = planned_population(&view, &runs, &[], "task_success", &pins).unwrap();
    assert_eq!(pop.n_slots, 3);
    // p1: first subject oracle-failed, the completed retry reads the slot.
    let p1 = &pop.slots[0];
    assert_eq!(p1.member.as_deref(), Some("r-retry"));
    assert!(p1.recovered);
    assert_eq!(p1.n_attempts, 2);
    // p3 stays a planned/not-run row — the required universe came from
    // the view's plans, and M counts slots.
    let p3 = &pop.slots[2];
    assert!(p3.member.is_none());
    let raw = &pop.populations["raw"];
    assert_eq!((raw.s, raw.f, raw.u), (1, 1, 1));
    assert!(!pop.complete);
}

#[test]
fn adapter_pending_disposition_is_unresolved_not_full() {
    let mut view = ExperimentView::default();
    view.plans.insert(
        "p1".into(),
        plan(
            "p1",
            "t0",
            "A",
            vec![attempt("r-p", 1, OutcomeClass::Scored)],
        ),
    );
    let mut r = run("r-p", "A", "t0", OutcomeClass::Scored, true);
    r.disposition = Some("pending".into());
    let mut pins = BTreeMap::new();
    pins.insert("r-p".to_string(), 5u64);
    let pop = planned_population(&view, &[r], &[], "task_success", &pins).unwrap();
    assert_eq!(pop.pending_slots, vec!["p1".to_string()]);
    assert!(!pop.complete);
    assert_eq!(pop.populations["raw"].u, 1);
}
