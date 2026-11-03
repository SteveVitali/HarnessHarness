//! R5.25 — the §6.3 Recompute battery: `recompute_analysis` verifies
//! the declared pins, reconciles the frozen selection/population
//! (R5.23 receipt), reruns the pinned analysis through the one kernel
//! and compares under the frozen exact/tolerance rules (AC-1); wrong
//! selector/price/weight/family/draw pins, withheld inputs,
//! unsupported estimators and nonfinite members refuse; a
//! threshold/verdict change fails even inside tolerance (AC-2).
//! Fails-without: `hharness_analysis::recompute` does not exist at the
//! R5.24 base `924ec646` — compile-red there.

use std::collections::BTreeMap;

use hharness_analysis::receipt::{
    AnalysisDecl, DeclaredReceipt, GradeDecl, GradeStatus, RowDecl, SettlementDecl,
};
use hharness_analysis::recompute::{
    recompute_analysis, within_tolerance, PopulationPins, RecomputeError, RecomputePlan,
};
use hharness_analysis::{analyze, AnalysisError, AnalysisInput};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::runs::{SuiteContext, TaskContext};
use hharness_experiment::view::{Attempt, ExperimentView, PlanState};
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{EstimatorSelection, IntervalMethod, MetricValueKind};
use hharness_results::row::{AuditSection, ConsumptionSection, DerivedFrom, OutcomeSection};
use hharness_results::row::{Cell, Coordinates, ResultsRow, RowKey};
use hharness_results::scoring::ScoringContext;
use hharness_results::watermark::WatermarkSet;
use hharness_wire::Json;

// ── EX-02 fixture (the r5_16 shape): two tasks × two arms × five ────
// Task 1: A = 1/5, B = 4/5. Task 2: A = 3/5, B = 2/5 — observed
// equal-task contrast .20, posterior mean 1/7. `beta{1,1}` only — the
// beta{2,2} arm is DF-R5.17-1 (mislabelled prior; owed to R5.40).
const EX02: &[(&str, &str, &[bool])] = &[
    ("t1", "A", &[false, false, false, false, true]),
    ("t1", "B", &[true, true, true, true, false]),
    ("t2", "A", &[true, true, true, false, false]),
    ("t2", "B", &[true, true, false, false, false]),
];

fn rid(arm: &str, task: &str, rep: usize) -> String {
    format!("r-{arm}-{task}-{rep}")
}

fn view() -> ExperimentView {
    let mut v = ExperimentView::default();
    for (task, arm, outs) in EX02 {
        for (i, _) in outs.iter().enumerate() {
            let rpid = format!("p-{arm}-{task}-{i}");
            v.plans.insert(
                rpid.clone(),
                PlanState {
                    run_plan_id: rpid,
                    cell_id: format!("{arm}:{task}"),
                    arm_id: arm.to_string(),
                    task_id: task.to_string(),
                    replicate_index: i as u32,
                    configuration_version_id: "cfg".into(),
                    split_label: "held_out".into(),
                    seed_material: Json::Null,
                    order_pos: i as u64,
                    claim: None,
                    attempts: vec![Attempt {
                        run_id: rid(arm, task, i),
                        attempt_no: 1,
                        budget_id: "b".into(),
                        bound: true,
                        outcome: Some(OutcomeClass::Scored),
                        accepted: false,
                        superseded: false,
                        excluded: None,
                        veto_tripped: vec![],
                        budget_utilization: None,
                        regrade_pending: false,
                        pools: vec![],
                    }],
                    next_attempt_no: 2,
                    not_before_ms: 0,
                    accepted_run: None,
                    finished_final: false,
                    launch_binding: None,
                    grade_pending: false,
                    disposition: None,
                },
            );
        }
    }
    v
}

fn row(arm: &str, task: &str, rep: usize, pass: bool, pin: u64) -> ResultsRow {
    let rid = rid(arm, task, rep);
    let mut wm = WatermarkSet::new();
    wm.pin(&rid, pin);
    ResultsRow {
        key: RowKey {
            configuration_version_id: "cv".into(),
            run_id: rid.clone(),
        },
        coordinates: Coordinates {
            configuration_id: Some("cfg".into()),
            configuration_version_id: "cv".into(),
            model_snapshots: BTreeMap::new(),
            harness_def_ref: None,
            model_profile_ref: None,
            environment_ref: None,
            environment_version_id: Some("env".into()),
            task: Some(Json::obj([
                ("task_id", Json::str(task)),
                ("suite_id", Json::str("suite")),
                ("split_label", Json::str("held_out")),
            ])),
            budget: Json::obj([]),
            replicate: Json::obj([]),
            participant_class: "native".into(),
            observability_level: vec!["ledger".into()],
            hosting_mechanism: None,
            capability_vector_ref: None,
            mediation: vec![],
            registry_snapshot_id: None,
        },
        experiment: Some(Json::obj([
            ("experiment_run_id", Json::str("exp-1")),
            ("arm_id", Json::str(arm)),
            ("cell_id", Json::str(format!("{arm}:{task}"))),
            ("replicate_index", Json::Int(rep as i64)),
            ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ])),
        outcome: OutcomeSection {
            status: "finished".into(),
            outcome_class: Some("scored".into()),
            stop_reason: Some("completed".into()),
            veto_tripped: vec![],
            finished_at: None,
            wall_ms: None,
            disposition: None,
            activation_no: 1,
        },
        cells: vec![Cell {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(pass),
            detector: Some("deterministic".into()),
            oracle_ref: Some("oracle/executable".into()),
            confidence: None,
            evidence: vec![],
            computed_from: vec![],
        }],
        consumption: ConsumptionSection::default(),
        cache: Json::obj([]),
        lineage: Json::obj([]),
        annotations_from_ledger: Json::obj([]),
        audit: AuditSection {
            head: hharness_results::audit::AuditRef {
                run_id: rid.clone(),
                seq: pin,
                hash: "idp:head".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            },
            checkpoint_ref: None,
        },
        scoring: ScoringContext::native(),
        derived_from: DerivedFrom {
            run_id: rid.clone(),
            seq: pin,
            overlay_watermarks: BTreeMap::new(),
        },
        watermark_set: wm,
        view_hash: "v".into(),
        version_id: format!("v-{rid}"),
    }
}

struct Fx {
    rows: Vec<ResultsRow>,
    manifests: BTreeMap<String, RunManifest>,
    facts: BTreeMap<String, LedgerFacts>,
    tasks: Vec<TaskContext>,
    suites: Vec<SuiteContext>,
    watermarks: WatermarkSet,
    view: ExperimentView,
    decls: Vec<hharness_ontology::compliance::MetricDeclaration>,
}

fn fx(pin: u64) -> Fx {
    let rows: Vec<ResultsRow> = EX02
        .iter()
        .flat_map(|(task, arm, outs)| {
            outs.iter()
                .enumerate()
                .map(|(i, ok)| row(arm, task, i, *ok, pin))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut watermarks = WatermarkSet::new();
    for r in &rows {
        for (r, s) in &r.watermark_set.runs {
            watermarks.pin(r, *s);
        }
    }
    Fx {
        manifests: rows
            .iter()
            .map(|r| (r.key.run_id.clone(), RunManifest::minimal(RunKind::Agent)))
            .collect(),
        facts: rows
            .iter()
            .map(|r| (r.key.run_id.clone(), LedgerFacts::default()))
            .collect(),
        rows,
        tasks: vec![],
        suites: vec![SuiteContext {
            suite_id: "suite".into(),
            retired_for_headline: false,
            family: hharness_ontology::lab::EnvironmentFamily::CodingTerminal,
        }],
        watermarks,
        view: view(),
        decls: vec![hharness_eval::catalogue::metric("task_success").expect("catalogue")],
    }
}

fn spec(prior: &str) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: "finite_manifest_contrast".into(),
        query: QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(Json::obj([
                ("test_arm", Json::str("B")),
                ("control_arm", Json::str("A")),
            ])),
            grain: None,
        },
        spec_ref: Some("exp-spec".into()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::FiniteManifestContrast(prior.into()),
            selection_rule: "padr-r5-4.finite_manifest".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    s
}

fn input<'a>(f: &'a Fx) -> AnalysisInput<'a> {
    AnalysisInput {
        rows: &f.rows,
        manifests: &f.manifests,
        facts: &f.facts,
        tasks: &f.tasks,
        suites: &f.suites,
        design: None,
        arm_specs: &[],
        declarations: &f.decls,
        pre_registration: None,
        watermark_set: &f.watermarks,
        metric_registry_version: "test-registry",
        price_table_version: "test-pricing",
        confidence_ppm: 950_000,
        resampling_draws: 2000,
        seed: 0,
        post_amendment: false,
        view: Some(&f.view),
    }
}

/// The declared analysis pins — bound exactly to the frozen body
/// (method = the section's metadata verbatim; weights/report/spec
/// refs as produced).
fn decl(spec: &AnalysisSpec, body: &Json) -> AnalysisDecl {
    AnalysisDecl {
        protocol_ref: Some("proto-1".into()),
        prices: Json::obj([
            ("price_table_version", Json::str("test-pricing")),
            ("confidence_ppm", Json::Int(950_000)),
            ("metric_registry_version", Json::str("test-registry")),
        ]),
        method: body
            .get("finite_manifest_contrast")
            .and_then(|s| s.get("metadata"))
            .cloned()
            .expect("frozen metadata"),
        seed: 0,
        draws: 2000,
        weights: Json::obj([("weights_ref", Json::str("equal_task:w_t=1/T"))]),
        spec_ref: Some(spec.spec_id()),
        record_ref: Some("rec-1".into()),
        report_ref: body
            .get("report_id")
            .and_then(Json::as_str)
            .map(str::to_string),
    }
}

/// The declared receipt covering the fixture — every slot, attempt,
/// settlement, selected grade and row (AC-1's selection rebuild).
fn receipt_decl(analysis: &AnalysisDecl) -> DeclaredReceipt {
    let mut d = DeclaredReceipt {
        analysis: analysis.clone(),
        cost_version: "cost-v1".into(),
        selection_ref: "sel-1".into(),
        ..DeclaredReceipt::default()
    };
    for (task, arm, outs) in EX02 {
        for (i, _) in outs.iter().enumerate() {
            let p = format!("p-{arm}-{task}-{i}");
            let r = rid(arm, task, i);
            d.slots.insert(p.clone());
            d.attempts.insert(p.clone(), vec![r.clone()]);
            d.settlements.push(SettlementDecl {
                run_id: r.clone(),
                settlement_ref: format!("st-{r}"),
                cost_version: "cost-v1".into(),
            });
            d.grades.push(GradeDecl {
                run_id: r.clone(),
                grade_ref: format!("g-{r}"),
                selection_ref: "sel-1".into(),
                ordinal: 1,
                counts_subject: true,
                status: GradeStatus::Selected,
            });
            d.rows.push(RowDecl {
                run_id: r.clone(),
                version_id: format!("v-{r}"),
                plan_ref: p,
            });
        }
    }
    d
}

/// The full plan: frozen body produced by the same kernel (the
/// recipient's reference), declared pins bound to it.
struct Plan {
    spec: AnalysisSpec,
    decl: AnalysisDecl,
    declared: DeclaredReceipt,
    reference: Json,
    body: Json,
}

fn plan(f: &Fx) -> Plan {
    let spec = spec("beta{1,1}");
    let body = analyze(&spec, &input(f)).expect("reference run").body;
    let decl = decl(&spec, &body);
    Plan {
        declared: receipt_decl(&decl),
        decl,
        spec,
        reference: body.clone(),
        body,
    }
}

fn full<'a>(p: &'a Plan, f: &'a Fx) -> RecomputePlan<'a> {
    RecomputePlan {
        spec: &p.spec,
        declared: &p.decl,
        reference: Some(&p.reference),
        population: Some(PopulationPins {
            view: &f.view,
            metric_ref: "task_success",
            declared: &p.declared,
        }),
    }
}

fn err_of(
    r: Result<hharness_analysis::recompute::RecomputeReport, AnalysisError>,
) -> AnalysisError {
    r.expect_err("expected refusal")
}

/// Mutable access to a named object member's own object (fixture edits).
fn section_mut<'a>(j: &'a mut Json, k: &str) -> &'a mut BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => match m.get_mut(k) {
            Some(Json::Obj(s)) => s,
            _ => panic!("section {k} missing"),
        },
        _ => panic!("not an object"),
    }
}

// AC-1 — verify first, rebuild selection/population and the native
// analysis; exact + tolerance rules hold on the informative fixture.
#[test]
fn reproduces_frozen_analysis() {
    let f = fx(7);
    let p = plan(&f);
    let out = recompute_analysis(&full(&p, &f), &input(&f)).expect("reproduce");
    assert_eq!(out.verdict, "reproduced");
    assert!(out.drifts.is_empty());
    assert_eq!(out.report_id, out.reference_report_id);
    assert_eq!(out.body.to_canonical_string(), p.body.to_canonical_string());
    let legs: Vec<&str> = out.legs.iter().map(|l| l.verdict).collect();
    assert_eq!(legs, vec!["pass", "closed", "pass", "identical"]);
    // The persisted path agrees — the frozen record is the served one.
    let j = out.to_json();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some("analysis-recompute/1")
    );
    assert_eq!(j.get("verdict").and_then(Json::as_str), Some("reproduced"));
}

// AC-1 — the run-set identity binds through `generated_from` even
// without the optional population pins (the leg reports not_supplied).
#[test]
fn reproduces_without_population_pins() {
    let f = fx(7);
    let p = plan(&f);
    let rp = RecomputePlan {
        spec: &p.spec,
        declared: &p.decl,
        reference: Some(&p.reference),
        population: None,
    };
    let out = recompute_analysis(&rp, &input(&f)).expect("reproduce");
    assert_eq!(out.verdict, "reproduced");
    assert_eq!(out.legs[1].verdict, "not_supplied");
}

// AC-1/2 — the frozen tolerance rule itself: full-precision values,
// boundary-inclusive, ppm-scale unequal integers outside.
#[test]
fn tolerance_rule() {
    assert!(within_tolerance(0, 0));
    assert!(within_tolerance(10_000_000_001, 10_000_000_000)); // 1 ≤ 1.0 tol
    assert!(within_tolerance(2_000_000_000_200, 2_000_000_000_000)); // boundary
    assert!(!within_tolerance(1, 2));
    assert!(!within_tolerance(142_858, 142_857)); // ppm-scale exact
    assert!(!within_tolerance(-2, -1));
}

// AC-2 — wrong selector: a changed spec no longer recomputes to the
// declared spec pin.
#[test]
fn wrong_selector_refuses() {
    let f = fx(7);
    let p = plan(&f);
    let mut s2 = spec("beta{1,1}");
    if let Json::Obj(ref mut m) = s2.query.filters.as_mut().unwrap() {
        m.insert("test_arm".into(), Json::str("C"));
    }
    s2.spec_id = s2.spec_id();
    let rp = RecomputePlan {
        spec: &s2,
        ..full(&p, &f)
    };
    match err_of(recompute_analysis(&rp, &input(&f))) {
        AnalysisError::Recompute(RecomputeError::WrongPin { member, .. }) => {
            assert_eq!(member, "spec_ref")
        }
        e => panic!("expected WrongPin spec_ref, got {e:?}"),
    }
}

// AC-2 — wrong price / weight / family / draw pins refuse before rerun.
#[test]
fn wrong_pins_refuse() {
    let f = fx(7);
    let p = plan(&f);
    let member_of = |rp: &RecomputePlan<'_>| match err_of(recompute_analysis(rp, &input(&f))) {
        AnalysisError::Recompute(RecomputeError::WrongPin { member, .. }) => member,
        e => panic!("expected WrongPin, got {e:?}"),
    };
    // price — rebuilt input disagrees with the declared pin.
    let mut inp = input(&f);
    inp.price_table_version = "other-pricing";
    match err_of(recompute_analysis(&full(&p, &f), &inp)) {
        AnalysisError::Recompute(RecomputeError::WrongPin { member, .. }) => {
            assert_eq!(member, "price_table_version")
        }
        e => panic!("expected WrongPin price_table_version, got {e:?}"),
    }
    // weight — the declared rule differs from the frozen section member.
    let mut d2 = p.decl.clone();
    d2.weights = Json::obj([("weights_ref", Json::str("equal_task:w_t=1"))]);
    let rp = RecomputePlan {
        declared: &d2,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "weights");
    // family — the declared method's population key drifts.
    let mut d3 = p.decl.clone();
    if let Json::Obj(ref mut m) = d3.method {
        if let Some(Json::Obj(keys)) = m.get_mut("keys") {
            keys.insert("population".into(), Json::str("raw"));
        }
    }
    let rp = RecomputePlan {
        declared: &d3,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "method.keys.population");
    // draws — the rebuilt draw pins disagree with the declaration.
    let mut i2 = input(&f);
    i2.seed = 9;
    match err_of(recompute_analysis(&full(&p, &f), &i2)) {
        AnalysisError::Recompute(RecomputeError::WrongPin { member, .. }) => {
            assert_eq!(member, "seed")
        }
        e => panic!("expected WrongPin seed, got {e:?}"),
    }
    let mut i3 = input(&f);
    i3.resampling_draws = 4000;
    match err_of(recompute_analysis(&full(&p, &f), &i3)) {
        AnalysisError::Recompute(RecomputeError::WrongPin { member, .. }) => {
            assert_eq!(member, "draws")
        }
        e => panic!("expected WrongPin draws, got {e:?}"),
    }
}

// AC-2 — withheld inputs refuse: reference body, declared pin
// members, the selector pin.
#[test]
fn withheld_inputs_refuse() {
    let f = fx(7);
    let p = plan(&f);
    let member_of = |rp: &RecomputePlan<'_>| match err_of(recompute_analysis(rp, &input(&f))) {
        AnalysisError::Recompute(RecomputeError::Withheld { member }) => member,
        e => panic!("expected Withheld, got {e:?}"),
    };
    let rp = RecomputePlan {
        reference: None,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "reference_body");
    let rp = RecomputePlan {
        reference: Some(&Json::Null),
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "reference_body");
    let mut d = p.decl.clone();
    d.prices = Json::Null;
    let rp = RecomputePlan {
        declared: &d,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "prices");
    let mut d = p.decl.clone();
    d.spec_ref = None;
    let rp = RecomputePlan {
        declared: &d,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "spec_ref");
    let mut d = p.decl.clone();
    d.method = Json::Null;
    let rp = RecomputePlan {
        declared: &d,
        ..full(&p, &f)
    };
    assert_eq!(member_of(&rp), "method");
}

// AC-2 — unsupported estimator: the frozen record self-consistently
// declares a spec the kernel no longer admits — the refusal is the
// kernel's own, verbatim.
#[test]
fn unsupported_estimator_refuses() {
    let f = fx(7);
    let p = plan(&f);
    // beta{9,9} — outside the admitted {beta{1,1}, beta{2,2}} set.
    let s2 = spec("beta{9,9}");
    let mut reference = p.reference.clone();
    let mut d2 = p.decl.clone();
    d2.spec_ref = Some(s2.spec_id());
    // Pin the frozen report id to the declared spec so the record is
    // self-consistent — the refusal is the estimator, not the pins.
    let gf = hharness_analysis::report::generated_from_for(&input(&f));
    let rid = hharness_analysis::report::report_id_for(&s2, &gf);
    if let Json::Obj(ref mut m) = reference {
        m.insert("report_id".into(), Json::str(&rid));
    }
    d2.report_ref = Some(rid);
    let rp = RecomputePlan {
        spec: &s2,
        declared: &d2,
        reference: Some(&reference),
        population: None,
    };
    assert!(matches!(
        err_of(recompute_analysis(&rp, &input(&f))),
        AnalysisError::Contrast(
            hharness_eval::finite_manifest::ContrastError::UnsupportedPrior { .. }
        )
    ));
    // A method outside the kind's admitted set refuses UnknownMethod.
    let mut s3 = spec("beta{1,1}");
    s3.estimator_selection.method = IntervalMethod::ClusteredClt;
    s3.spec_id = s3.spec_id();
    let mut d3 = p.decl.clone();
    d3.spec_ref = Some(s3.spec_id());
    let rid3 = hharness_analysis::report::report_id_for(&s3, &gf);
    let mut ref3 = p.reference.clone();
    if let Json::Obj(ref mut m) = ref3 {
        m.insert("report_id".into(), Json::str(&rid3));
    }
    d3.report_ref = Some(rid3);
    let rp = RecomputePlan {
        spec: &s3,
        declared: &d3,
        reference: Some(&ref3),
        population: None,
    };
    assert!(matches!(
        err_of(recompute_analysis(&rp, &input(&f))),
        AnalysisError::Contrast(
            hharness_eval::finite_manifest::ContrastError::UnknownMethod { .. }
        )
    ));
}

// AC-2 — a nonfinite declared pin refuses (never a silent default).
#[test]
fn nonfinite_pin_refuses() {
    let f = fx(7);
    let p = plan(&f);
    let mut d = p.decl.clone();
    if let Json::Obj(ref mut m) = d.prices {
        m.insert("confidence_ppm".into(), Json::str("nan"));
    }
    let rp = RecomputePlan {
        declared: &d,
        ..full(&p, &f)
    };
    assert!(matches!(
        err_of(recompute_analysis(&rp, &input(&f))),
        AnalysisError::Recompute(RecomputeError::NonFinite { .. })
    ));
}

// AC-2 — a categorical verdict change fails while every scalar sits
// inside tolerance (identical numerics are trivially within it).
#[test]
fn verdict_change_fails_inside_tolerance() {
    let f = fx(7);
    let p = plan(&f);
    let mut reference = p.reference.clone();
    if let Some(Json::Obj(i)) =
        section_mut(&mut reference, "finite_manifest_contrast").get_mut("interval")
    {
        i.insert("kind".into(), Json::str("significant"));
    }
    let rp = RecomputePlan {
        reference: Some(&reference),
        ..full(&p, &f)
    };
    let out = recompute_analysis(&rp, &input(&f)).expect("verdict at exit zero");
    assert_eq!(out.verdict, "not_reproduced");
    assert_eq!(out.legs[3].verdict, "drifted");
    assert!(out
        .drifts
        .iter()
        .any(|d| d.kind == "verdict" && d.member.ends_with("interval.kind")));
}

// AC-2 — a threshold member change fails even when the delta sits
// inside the scalar tolerance (exact members never ride it).
#[test]
fn threshold_change_fails_inside_tolerance() {
    let f = fx(7);
    let p = plan(&f);
    // seed is an exact member; 10^10-vs-10^10+1 is inside 1e-10·max.
    let mut i = input(&f);
    i.seed = 10_000_000_001;
    let body = analyze(&p.spec, &i).expect("reference run").body;
    let mut reference = body.clone();
    section_mut(&mut reference, "finite_manifest_contrast")
        .insert("seed".into(), Json::Int(10_000_000_000));
    let mut d = decl(&p.spec, &reference);
    d.seed = 10_000_000_001;
    let declared = receipt_decl(&d);
    let rp = RecomputePlan {
        spec: &p.spec,
        declared: &d,
        reference: Some(&reference),
        population: Some(PopulationPins {
            view: &f.view,
            metric_ref: "task_success",
            declared: &declared,
        }),
    };
    let out = recompute_analysis(&rp, &i).expect("verdict at exit zero");
    assert_eq!(out.verdict, "not_reproduced");
    assert!(out
        .drifts
        .iter()
        .any(|d| d.kind == "threshold" && d.member.ends_with(".seed")));
}

// AC-2 — estimate/count/membership/order drifts fail the verdict at
// exit zero; old bytes are never rewritten.
#[test]
fn drifted_members_fail_the_verdict() {
    let f = fx(7);
    let p = plan(&f);
    // scalar estimate beyond tolerance.
    let mut r1 = p.reference.clone();
    section_mut(&mut r1, "finite_manifest_contrast")
        .insert("posterior_mean_ppm".into(), Json::Int(142_858));
    let rp = RecomputePlan {
        reference: Some(&r1),
        ..full(&p, &f)
    };
    let out = recompute_analysis(&rp, &input(&f)).expect("verdict");
    assert_eq!(out.verdict, "not_reproduced");
    assert!(out.drifts.iter().any(|d| d.kind == "scalar"));
    // dropped member — count/identity.
    let mut r2 = p.reference.clone();
    section_mut(&mut r2, "finite_manifest_contrast").remove("seed");
    let rp = RecomputePlan {
        reference: Some(&r2),
        ..full(&p, &f)
    };
    let out = recompute_analysis(&rp, &input(&f)).expect("verdict");
    assert_eq!(out.verdict, "not_reproduced");
    assert!(out.drifts.iter().any(|d| d.kind == "identity"));
    // reordered membership — order.
    let mut r3 = p.reference.clone();
    if let Some(Json::Arr(c)) = section_mut(&mut r3, "finite_manifest_contrast").get_mut("cells") {
        c.reverse();
    }
    let rp = RecomputePlan {
        reference: Some(&r3),
        ..full(&p, &f)
    };
    let out = recompute_analysis(&rp, &input(&f)).expect("verdict");
    assert_eq!(out.verdict, "not_reproduced");
    assert!(out.drifts.iter().any(|d| d.kind == "order"));
}

// AC-2 — an unclosed frozen selection refuses before any rerun (the
// receipt's dropped_slot is a verify-first predicate, not a drift).
#[test]
fn unclosed_selection_refuses() {
    let f = fx(7);
    let p = plan(&f);
    let mut declared = receipt_decl(&p.decl);
    declared.slots.remove("p-B-t1-0");
    let rp = RecomputePlan {
        spec: &p.spec,
        declared: &p.decl,
        reference: Some(&p.reference),
        population: Some(PopulationPins {
            view: &f.view,
            metric_ref: "task_success",
            declared: &declared,
        }),
    };
    assert!(matches!(
        err_of(recompute_analysis(&rp, &input(&f))),
        AnalysisError::Recompute(RecomputeError::SelectionNotClosed { .. })
    ));
}

// AC-2 — two different analysis declarations cannot bind one frozen
// analysis.
#[test]
fn declared_analysis_drift_refuses() {
    let f = fx(7);
    let p = plan(&f);
    let mut declared = receipt_decl(&p.decl);
    declared.analysis.seed = 99;
    let rp = RecomputePlan {
        spec: &p.spec,
        declared: &p.decl,
        reference: Some(&p.reference),
        population: Some(PopulationPins {
            view: &f.view,
            metric_ref: "task_success",
            declared: &declared,
        }),
    };
    assert!(matches!(
        err_of(recompute_analysis(&rp, &input(&f))),
        AnalysisError::Recompute(RecomputeError::DeclaredAnalysisDrift { .. })
    ));
}
