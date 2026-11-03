//! R5.23 — the all-planned receipt battery: `population-receipt/1`
//! reconciles slots→plans→first/all attempts→settlements/selected
//! grades→rows→analysis with separate prerequisite/process/task/veto
//! verdicts (AC-1); dropped unrun slot, orphan launch, duplicate grade,
//! skipped/format failure, stale selection/cost version and wrong
//! member join fail (AC-2). Fails-without: `hharness_analysis::receipt`
//! does not exist at the R5.22 base — compile-red there.

use std::collections::{BTreeMap, BTreeSet};

use hharness_analysis::receipt::{
    assemble_receipt, AnalysisDecl, AxisVerdict, DeclaredReceipt, GradeDecl, GradeStatus,
    Invocation, RowDecl, SettlementDecl, TaskVerdict, VetoVerdict, EVIDENCE_CLASS,
};
use hharness_eval::runs::{CacheState, EvalRun};
use hharness_experiment::view::{Attempt, ExperimentView, PlanState};
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{MetricValue, MetricValueKind};
use hharness_ontology::lab::{ContaminationStratum, SplitLabel};
use hharness_ontology::participant::{Observability, ParticipantClass};
use hharness_wire::Json;

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
        seed_material: Json::Null,
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

fn settle(run: &str) -> SettlementDecl {
    SettlementDecl {
        run_id: run.into(),
        settlement_ref: format!("settle-{run}"),
        cost_version: "cost-v1".into(),
    }
}

fn grade(run: &str, ord: u64) -> GradeDecl {
    GradeDecl {
        run_id: run.into(),
        grade_ref: format!("g-{run}-{ord}"),
        selection_ref: "sel-v1".into(),
        ordinal: ord,
        counts_subject: ord == 1,
        status: GradeStatus::Selected,
    }
}

fn row(run: &str, plan: &str) -> RowDecl {
    RowDecl {
        run_id: run.into(),
        version_id: format!("row-{run}"),
        plan_ref: plan.into(),
    }
}

fn analysis() -> AnalysisDecl {
    AnalysisDecl {
        protocol_ref: Some("proto-h".into()),
        prices: Json::obj([
            ("price_table_version", Json::str("prices-v3")),
            ("confidence", Json::str("declared")),
        ]),
        method: Json::obj([
            ("estimator", Json::str("planned_slot_equal_task_bounds")),
            ("version", Json::str("population/1")),
            ("prior", Json::Null),
        ]),
        seed: 42,
        draws: 0,
        weights: Json::obj([("rule", Json::str("w_t=1/T"))]),
        spec_ref: Some("spec-1".into()),
        record_ref: Some("rec-1".into()),
        report_ref: Some("rep-1".into()),
    }
}

/// The happy-path fixture: three plans (scored pass, scored fail,
/// planned-but-unrun) + a recovered retry — fully declared.
fn fixture() -> (
    ExperimentView,
    Vec<EvalRun>,
    BTreeMap<String, u64>,
    DeclaredReceipt,
) {
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
    view.plans
        .insert("p3".into(), plan("p3", "t0", "B", vec![]));
    let runs = vec![
        run("r-retry", "A", "t0", OutcomeClass::Scored, true),
        run("r-b", "B", "t0", OutcomeClass::Scored, false),
    ];
    let pins: BTreeMap<String, u64> = [("r-retry", 9u64), ("r-b", 7)]
        .into_iter()
        .map(|(r, s)| (r.to_string(), s))
        .collect();
    let declared = DeclaredReceipt {
        slots: ["p1", "p2", "p3"].iter().map(|s| s.to_string()).collect(),
        attempts: [
            ("p1", vec!["r-fail", "r-retry"]),
            ("p2", vec!["r-b"]),
            ("p3", vec![]),
        ]
        .into_iter()
        .map(|(p, rs)| (p.to_string(), rs.iter().map(|r| r.to_string()).collect()))
        .collect(),
        settlements: vec![settle("r-fail"), settle("r-retry"), settle("r-b")],
        grades: vec![grade("r-retry", 1), grade("r-b", 1)],
        rows: vec![row("r-fail", "p1"), row("r-retry", "p1"), row("r-b", "p2")],
        cost_version: "cost-v1".into(),
        selection_ref: "sel-v1".into(),
        analysis: analysis(),
    };
    (view, runs, pins, declared)
}

fn codes(r: &hharness_analysis::receipt::PopulationReceipt) -> Vec<&'static str> {
    r.failures.iter().map(|f| f.code).collect()
}

#[test]
fn t1_full_chain_reconciles_closed() {
    let (view, runs, pins, declared) = fixture();
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert_eq!(r.verdict, "closed");
    assert!(r.failures.is_empty());
    assert_eq!(r.legs.len(), 6);
    assert_eq!(
        r.legs.iter().map(|l| l.name).collect::<Vec<_>>(),
        vec![
            "slots_to_plans",
            "plans_to_attempts",
            "attempts_to_settlements",
            "attempts_to_grades",
            "attempts_to_rows",
            "rows_to_analysis"
        ]
    );
    // The unrun slot is a retained member — M counts slots.
    assert_eq!(r.population.n_slots, 3);
    assert_eq!(r.slots.len(), 3);
    let p1 = &r.slots[0];
    assert_eq!(p1.member.as_deref(), Some("r-retry"));
    assert!(p1.recovered);
    assert_eq!(
        p1.all_attempts,
        vec!["r-fail".to_string(), "r-retry".to_string()]
    );
    assert_eq!(p1.prerequisite, AxisVerdict::Available);
    assert_eq!(p1.invocation, Invocation::Finished);
    assert_eq!(p1.task, TaskVerdict::Pass);
    assert_eq!(p1.veto, VetoVerdict::None);
    let p3 = &r.slots[2];
    assert!(p3.member.is_none());
    assert_eq!(p3.invocation, Invocation::NotRun);
    assert_eq!(p3.task, TaskVerdict::Unknown);
    assert_eq!(p3.prerequisite, AxisVerdict::Unknown);
    // Pins bound verbatim — protocol/prices/method/seed/draw/weights.
    let j = r.to_json();
    assert_eq!(
        j.get("schema").unwrap().as_str(),
        Some("population-receipt/1")
    );
    assert_eq!(
        j.get("evidence_class").unwrap().as_str(),
        Some(EVIDENCE_CLASS)
    );
    let pj = j.get("pins").unwrap();
    assert_eq!(pj.get("seed").unwrap().as_int(), Some(42));
    assert_eq!(pj.get("protocol_ref").unwrap().as_str(), Some("proto-h"));
    assert_eq!(
        pj.get("prices")
            .unwrap()
            .get("price_table_version")
            .unwrap()
            .as_str(),
        Some("prices-v3")
    );
}

#[test]
fn t2_dropped_unrun_slot_fails() {
    let (view, runs, pins, mut declared) = fixture();
    declared.slots.remove("p3"); // the unrun slot — retrospective shrink
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert_eq!(r.verdict, "failed");
    assert!(codes(&r).contains(&"dropped_slot"));
    // Extra members make no producer decision either.
    let (view, runs, pins, mut declared) = fixture();
    declared.slots.insert("pX".into());
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"extra_slot"));
}

#[test]
fn t3_orphan_launch_fails() {
    // A settlement, a grade and a row on a run bound to no plan.
    let (view, runs, pins, mut declared) = fixture();
    declared.settlements.push(settle("r-ghost"));
    declared.grades.push(grade("r-ghost", 1));
    declared.rows.push(row("r-ghost", "p1"));
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert_eq!(r.verdict, "failed");
    let orphan_legs: Vec<&str> = r
        .failures
        .iter()
        .filter(|f| f.code == "orphan_launch")
        .map(|f| f.leg)
        .collect();
    assert_eq!(orphan_legs.len(), 3);
    // An attempt declared under a plan it does not bind is a wrong join.
    let (view, runs, pins, mut declared) = fixture();
    declared
        .attempts
        .get_mut("p2")
        .unwrap()
        .push("r-retry".into());
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"wrong_member_join"));
}

#[test]
fn t4_duplicate_grade_fails() {
    // A second counting grade on one attempt + a repeated grade_ref.
    let (view, runs, pins, mut declared) = fixture();
    let mut dup = grade("r-b", 1);
    dup.grade_ref = "g-b-1".into(); // same ref as grade("r-b",1)
    declared.grades.push(dup);
    let mut extra = grade("r-b", 1);
    extra.grade_ref = "g-b-second".into();
    declared.grades.push(extra);
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"duplicate_grade"));
    // A retry can never count subject n (I-03).
    let (view, runs, pins, mut declared) = fixture();
    let mut retry = grade("r-retry", 2);
    retry.counts_subject = true;
    declared.grades.push(retry);
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"grade_retry_adds_n"));
}

#[test]
fn t5_skipped_or_format_failure_cannot_pass() {
    // A skipped grade presented as counting → grade_status_violation;
    // and the task axis reads `skipped`, never `pass`, beside a
    // finished invocation — a process pass erases nothing.
    let (view, runs, pins, mut declared) = fixture();
    let mut skipped = grade("r-retry", 1);
    skipped.grade_ref = "g-skip".into();
    skipped.status = GradeStatus::Skipped;
    skipped.counts_subject = true;
    declared.grades.push(skipped);
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"grade_status_violation"));
    // Non-counting skipped grade: the task axis still reports it.
    let (view, runs, pins, mut declared) = fixture();
    let mut skipped = grade("r-retry", 1);
    skipped.grade_ref = "g-skip".into();
    skipped.status = GradeStatus::Skipped;
    skipped.counts_subject = false;
    declared.grades.clear();
    declared.grades.push(skipped);
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    let p1 = &r.slots[0];
    assert_eq!(p1.invocation, Invocation::Finished);
    // The member's grade set is {skipped} — the task axis reports it,
    // never a pass; the selected-grade leg stays satisfied.
    assert_eq!(p1.task, TaskVerdict::Skipped);
}

#[test]
fn t6_stale_versions_fail() {
    let (view, runs, pins, mut declared) = fixture();
    declared.settlements[0].cost_version = "cost-v0".into();
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"stale_cost_version"));
    let (view, runs, pins, mut declared) = fixture();
    declared.grades[0].selection_ref = "sel-v0".into();
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"stale_selection"));
}

#[test]
fn t7_wrong_member_join_and_missing_row_fail() {
    let (view, runs, pins, mut declared) = fixture();
    declared.rows[1].plan_ref = "p2".into(); // r-retry binds p1
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"wrong_member_join"));
    let (view, runs, pins, mut declared) = fixture();
    declared.rows.retain(|row| row.run_id != "r-b");
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"missing_row"));
    // A settled attempt with no settlement likewise fails.
    let (view, runs, pins, mut declared) = fixture();
    declared.settlements.retain(|s| s.run_id != "r-b");
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"missing_settlement"));
}

#[test]
fn t8_pending_and_notrun_verdicts() {
    // grade_pending member: unresolved, never a pass; veto stays a
    // separate axis.
    let (mut view, mut runs, pins, mut declared) = fixture();
    let mut pending_run = run("r-b", "B", "t0", OutcomeClass::Scored, false);
    pending_run.disposition = Some("pending".into());
    runs[1] = pending_run;
    // An observed veto on the resolved member rides its own axis.
    let mut vetoed = run("r-retry", "A", "t0", OutcomeClass::Scored, true);
    vetoed.veto_tripped = vec!["v1".into()];
    runs[0] = vetoed;
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert_eq!(r.population.pending_slots, vec!["p2".to_string()]);
    assert!(!r.population.complete);
    let p1 = &r.slots[0];
    assert_eq!(p1.veto, VetoVerdict::Observed);
    assert_eq!(p1.veto_ids, vec!["v1".to_string()]);
    assert_eq!(p1.task, TaskVerdict::Pass);
    let p2 = r.slots.iter().find(|s| s.plan_ref == "p2").unwrap();
    // Pending member: unresolved, never a pass — veto reads unknown.
    assert_eq!(p2.veto, VetoVerdict::Unknown);
    assert_eq!(p2.task, TaskVerdict::Unknown);
    view.plans.get_mut("p2").unwrap().grade_pending = true;
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(r.population.pending_slots.contains(&"p2".to_string()));
    declared.analysis.weights = Json::Null;
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"missing_analysis_pin"));
}

#[test]
fn t9_extras_make_no_decision_no_class_promotion() {
    // An extra declared attempt under a real plan but for an unbound
    // run fails as orphan — extras never widen the universe.
    let (view, runs, pins, mut declared) = fixture();
    declared
        .attempts
        .get_mut("p3")
        .unwrap()
        .push("r-ghost".into());
    let r = assemble_receipt(&view, &runs, &[], "task_success", &pins, &declared).unwrap();
    assert!(codes(&r).contains(&"orphan_launch"));
    // Finished invocation + a hash-level agreement never promotes the
    // evidence label — the record fixes `derived_index` only.
    let j = r.to_json();
    assert_eq!(
        j.get("evidence_class").unwrap().as_str(),
        Some("derived_index")
    );
    assert_eq!(j.get("verdict").unwrap().as_str(), Some("failed"));
}
