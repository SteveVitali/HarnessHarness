//! R5.37 — the routed kind battery: `AnalysisSpec.kind =
//! "task_mixture_sensitivity"` through `analyze` → `analyze_and_record`
//! → the persisted `analysis_report_body/1` (REQ-D-008 primary; Packet D
//! companions). Fails-without: the kind arm, `task_mixture` module and
//! `AnalysisError::TaskMixture` do not exist at the R5.36 base — this
//! file compiles red there.

use std::collections::BTreeMap;

use hharness_analysis::{
    analyze, analyze_and_record, eval_run as project_eval_run, AnalysisError, AnalysisInput,
};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::runs::{EvalRun, SuiteContext, TaskContext};
use hharness_eval::task_mixture as tm;
use hharness_experiment::docs::LabDocs;
use hharness_experiment::view::{Attempt, ExperimentView, PlanState};
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{
    BootstrapPairedMethod, EstimatorSelection, IntervalMethod, MetricValueKind,
};
use hharness_results::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use hharness_results::scoring::ScoringContext;
use hharness_results::watermark::WatermarkSet;
use hharness_wire::Json;

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-37-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── fixture: `(task, pass_a, pass_b)` cells — five first-slot reps per
// (task, arm). `omit_metric` drops one bound run's metric cell
// (unresolved member); `drop_arm` drops one (task, arm) cell entirely.
const REPS: usize = 5;

fn view(tasks: &[(&str, u64, u64)]) -> ExperimentView {
    let mut v = ExperimentView::default();
    for (task, _, _) in tasks {
        for arm in ["A", "B"] {
            for i in 0..REPS {
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
                            run_id: format!("r-{arm}-{task}-{i}"),
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
    }
    v
}

fn row(arm: &str, task: &str, rep: usize, pass: bool, omit_metric: bool) -> ResultsRow {
    let rid = format!("r-{arm}-{task}-{rep}");
    let pin = 7u64;
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
        cells: if omit_metric {
            vec![]
        } else {
            vec![Cell {
                metric_ref: "task_success".into(),
                value: MetricValueKind::Bool(pass),
                detector: Some("deterministic".into()),
                oracle_ref: Some("oracle/executable".into()),
                confidence: None,
                evidence: vec![],
                computed_from: vec![],
            }]
        },
        consumption: ConsumptionSection {
            dimensions: BTreeMap::new(),
            utilization: None,
            spend: None,
            instrument_spend: None,
        },
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

// `tasks`: (task_id, pass_a, pass_b) — each arm has REPS slots;
// the first `pass_*` are successes. `omit`: (arm, task, rep) whose
// metric cell is absent (an unresolved member); `drop_cell`:
// (task, arm) whose whole cell is absent (unpaired task).
fn fx(
    tasks: &[(&str, u64, u64)],
    omit: &[(&str, &str, usize)],
    drop_cell: Option<(&str, &str)>,
) -> Fx {
    let mut rows = Vec::new();
    for (task, pa, pb) in tasks {
        for (arm, pass) in [("A", *pa), ("B", *pb)] {
            if drop_cell == Some((*task, arm)) {
                continue;
            }
            for i in 0..REPS {
                let om = omit.contains(&(arm, task, i));
                rows.push(row(arm, task, i, (i as u64) < pass, om));
            }
        }
    }
    // An absent (task, arm) cell removes both the rows AND the plans —
    // the task genuinely never paired a slot for that arm.
    let mut view = view(tasks);
    if let Some((task, arm)) = drop_cell {
        view.plans
            .retain(|_, ps| !(ps.task_id == task && ps.arm_id == arm));
    }
    let mut watermarks = WatermarkSet::new();
    for r in &rows {
        for (rid, seq) in &r.watermark_set.runs {
            watermarks.pin(rid, *seq);
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
        view,
        decls: vec![hharness_eval::catalogue::metric("task_success").expect("catalogue metric")],
    }
}

fn spec(method: BootstrapPairedMethod, filters: Json) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: tm::KIND.into(),
        query: QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(filters),
            grain: None,
        },
        spec_ref: Some("exp-spec".into()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::BootstrapPaired(method),
            selection_rule: "padr-r5-4.task_mixture".into(),
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

fn base_filters() -> Json {
    filters_with_member("m1")
}

// `overrides` replaces top-level filter members by key.
fn filters_with_member(member: &str) -> Json {
    Json::obj([
        ("test_arm", Json::str("B")),
        ("control_arm", Json::str("A")),
        ("null", Json::str("sharp_no_effect")),
        ("exchangeability", Json::str("lane_randomization")),
        (
            "family",
            Json::obj([
                ("id", Json::str("d2_primaries")),
                ("member", Json::str(member)),
                (
                    "members",
                    Json::Arr(vec![
                        Json::obj([("member", Json::str("m1"))]),
                        Json::obj([
                            ("member", Json::str("m2")),
                            ("raw_p_ppm", Json::Int(30_000)),
                        ]),
                        Json::obj([("member", Json::str("m3"))]),
                    ]),
                ),
            ]),
        ),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str("expl")),
                (
                    "members",
                    Json::Arr(vec![
                        Json::obj([
                            ("member", Json::str("e1")),
                            ("raw_p_ppm", Json::Int(10_000)),
                        ]),
                        Json::obj([
                            ("member", Json::str("e2")),
                            ("raw_p_ppm", Json::Int(80_000)),
                        ]),
                    ]),
                ),
            ]),
        ),
    ])
}

fn input<'a>(f: &'a Fx, draws: u64) -> AnalysisInput<'a> {
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
        resampling_draws: draws,
        seed: 0,
        post_amendment: false,
        view: Some(&f.view),
    }
}

// N tasks with distinct deltas: alternating (1,4) / (3,2) pass counts.
fn tasks_n(n: usize) -> Vec<(String, u64, u64)> {
    (0..n)
        .map(|i| {
            if i % 2 == 0 {
                (format!("t{:02}", i + 1), 1, 4)
            } else {
                (format!("t{:02}", i + 1), 3, 2)
            }
        })
        .collect()
}

fn member(body: &Json, name: &str) -> Vec<Json> {
    body.get("multiplicity")
        .and_then(|m| m.get("primary"))
        .and_then(|p| p.get("members"))
        .and_then(|m| match m {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.get("member").and_then(Json::as_str) == Some(name))
        .collect()
}

// AC-1 — routed path: percentile floor, exact sign flip, Holm family
// (unavailable member retained at worst case), separate BH, tails.
#[test]
fn routed_positive_persisted() {
    // EX-02-shaped two-task fixture: deltas [+0.6, −0.2] → exact
    // enumeration p = 4/4 = 1.0 (every assignment's |sum| ≥ 0.4).
    let f = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[], None);
    let s = spec(BootstrapPairedMethod::Percentile, base_filters());
    let body = analyze(&s, &input(&f, 2_000)).unwrap().body;
    let b = body.get("task_mixture_sensitivity").expect("body member");
    let boot = b.get("bootstrap").unwrap();
    assert_eq!(
        boot.get("method").and_then(Json::as_str),
        Some("percentile")
    );
    assert_eq!(
        boot.get("interval_kind").and_then(Json::as_str),
        Some("confidence")
    );
    assert_eq!(boot.get("tasks").and_then(Json::as_int), Some(2));
    assert_eq!(
        boot.get("draws")
            .and_then(|d| d.get("requested"))
            .and_then(Json::as_int),
        Some(2_000)
    );
    assert_eq!(
        boot.get("draws")
            .and_then(|d| d.get("used"))
            .and_then(Json::as_int),
        Some(2_000)
    );
    assert_eq!(
        b.get("observed_contrast_ppm").and_then(Json::as_int),
        Some(200_000)
    );
    let sf = b.get("sign_flip").unwrap();
    assert_eq!(sf.get("mode").and_then(Json::as_str), Some("exact"));
    assert_eq!(sf.get("draws").and_then(Json::as_int), Some(0));
    assert_eq!(sf.get("p_ppm").and_then(Json::as_int), Some(1_000_000));
    assert_eq!(
        sf.get("null").and_then(Json::as_str),
        Some("sharp_no_effect")
    );
    // Holm over the declared frozen family — m1 computed (raw 1.0),
    // m2 declared (raw .03 → adj 3·.03), m3 unavailable (worst mass).
    let m1 = member(&b, "m1");
    let m2 = member(&b, "m2");
    let m3 = member(&b, "m3");
    assert_eq!(m1[0].get("status").and_then(Json::as_str), Some("computed"));
    assert_eq!(m1[0].get("raw_ppm").and_then(Json::as_int), Some(1_000_000));
    assert_eq!(m2[0].get("status").and_then(Json::as_str), Some("declared"));
    assert_eq!(
        m2[0].get("adjusted_ppm").and_then(Json::as_int),
        Some(90_000)
    );
    assert_eq!(m2[0].get("rejected_at_alpha"), Some(&Json::Bool(false)));
    assert_eq!(
        m3[0].get("status").and_then(Json::as_str),
        Some("unavailable")
    );
    assert_eq!(
        m3[0].get("adjusted_ppm").and_then(Json::as_int),
        Some(1_000_000)
    );
    let expl = b
        .get("multiplicity")
        .and_then(|m| m.get("exploratory"))
        .unwrap();
    assert_eq!(
        expl.get("method").and_then(Json::as_str),
        Some("benjamini_hochberg")
    );
    // Tails — signed keeps direction; absolute stays separate; the
    // observed max is labelled, never a population bound.
    let tails = b.get("tails").unwrap();
    assert_eq!(
        tails
            .get("signed")
            .and_then(|t| t.get("p95_ppm"))
            .and_then(Json::as_int),
        Some(600_000)
    );
    assert_eq!(
        tails
            .get("signed")
            .and_then(|t| t.get("p50_ppm"))
            .and_then(Json::as_int),
        Some(-200_000)
    );
    assert_eq!(
        tails
            .get("absolute")
            .and_then(|t| t.get("p50_ppm"))
            .and_then(Json::as_int),
        Some(200_000)
    );
    assert_eq!(
        tails.get("max_note").and_then(Json::as_str),
        Some("observed sample max — not a population bound")
    );
    assert_eq!(
        tails
            .get("uncertainty")
            .and_then(|u| u.get("kind"))
            .and_then(Json::as_str),
        Some("clustered_resampling")
    );
    // No omnibus/certificate member exists anywhere on the body.
    assert!(b.get("verdict").is_none() && b.get("omnibus").is_none());
    // Persisted report: deterministic id, idempotent re-serve.
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let r1 = analyze_and_record(&docs, &s, &input(&f, 2_000)).unwrap();
    let r2 = analyze_and_record(&docs, &s, &input(&f, 2_000)).unwrap();
    assert_eq!(r1.report_id, r2.report_id);
    assert_eq!(r1.record.kind.as_deref(), Some(tm::KIND));
    assert!(r1.record.outputs.contains(&r1.report_id));
}

// AC-1 — the 29/30 task boundary selects percentile/BCa; a declared
// method contradicting the floor refuses.
#[test]
fn bca_floor_boundary() {
    let t29: Vec<(String, u64, u64)> = tasks_n(29);
    let t30: Vec<(String, u64, u64)> = tasks_n(30);
    let t30r: Vec<(&str, u64, u64)> = t30.iter().map(|(t, a, b)| (t.as_str(), *a, *b)).collect();
    let t29r: Vec<(&str, u64, u64)> = t29.iter().map(|(t, a, b)| (t.as_str(), *a, *b)).collect();
    let f30 = fx(&t30r, &[], None);
    let b = analyze(
        &spec(BootstrapPairedMethod::Bca, base_filters()),
        &input(&f30, 2_000),
    )
    .unwrap()
    .body;
    let boot = b
        .get("task_mixture_sensitivity")
        .and_then(|x| x.get("bootstrap"))
        .unwrap();
    assert_eq!(boot.get("method").and_then(Json::as_str), Some("bca"));
    let f29 = fx(&t29r, &[], None);
    let b = analyze(
        &spec(BootstrapPairedMethod::Percentile, base_filters()),
        &input(&f29, 2_000),
    )
    .unwrap()
    .body;
    assert_eq!(
        b.get("task_mixture_sensitivity")
            .and_then(|x| x.get("bootstrap"))
            .and_then(|x| x.get("method"))
            .and_then(Json::as_str),
        Some("percentile")
    );
    // Declared BCa at T=29 contradicts the floor → typed refusal.
    assert!(matches!(
        analyze(
            &spec(BootstrapPairedMethod::Bca, base_filters()),
            &input(&f29, 2_000),
        ),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::BootstrapFloorViolation { .. }
        ))
    ));
}

// AC-1 — T=21 uses the frozen 8,000 Monte-Carlo `(extreme+1)/(draws+1)`
// rule; T=20 is still exact.
#[test]
fn signflip_mc_above_twenty() {
    let t21: Vec<(String, u64, u64)> = tasks_n(21);
    let t21r: Vec<(&str, u64, u64)> = t21.iter().map(|(t, a, b)| (t.as_str(), *a, *b)).collect();
    let f = fx(&t21r, &[], None);
    let b = analyze(
        &spec(BootstrapPairedMethod::Percentile, base_filters()),
        &input(&f, 2_000),
    )
    .unwrap()
    .body;
    let sf = b
        .get("task_mixture_sensitivity")
        .and_then(|x| x.get("sign_flip"))
        .unwrap();
    assert_eq!(sf.get("mode").and_then(Json::as_str), Some("monte_carlo"));
    assert_eq!(sf.get("draws").and_then(Json::as_int), Some(8_000));
    assert_eq!(
        sf.get("rule").and_then(Json::as_str),
        Some("(extreme+1)/(draws+1)")
    );
}

// AC-1 — the same frozen family recomputes identical Holm adjustments
// from split reports (different self-member) — the missing member can
// never improve the correction.
#[test]
fn split_reports_same_holm() {
    let f = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[], None);
    // Report A declares itself member m1; report B declares m3 (its own
    // member unavailable to that report's sign flip → unavailable).
    let ba = analyze(
        &spec(BootstrapPairedMethod::Percentile, filters_with_member("m1")),
        &input(&f, 2_000),
    )
    .unwrap()
    .body;
    let bb = analyze(
        &spec(BootstrapPairedMethod::Percentile, filters_with_member("m3")),
        &input(&f, 2_000),
    )
    .unwrap()
    .body;
    let ba = ba.get("task_mixture_sensitivity").unwrap();
    let bb = bb.get("task_mixture_sensitivity").unwrap();
    // The shared *declared* member m2 adjusts identically in both.
    assert_eq!(
        member(&ba, "m2")[0].get("adjusted_ppm"),
        member(&bb, "m2")[0].get("adjusted_ppm")
    );
    // m3 declared as own member without a sign flip → unavailable row.
    let m3 = member(&bb, "m3");
    assert_eq!(m3[0].get("status").and_then(Json::as_str), Some("computed"));
    // Family size stays 3 in both — a missing member cannot shrink it.
    assert_eq!(
        ba.get("multiplicity")
            .and_then(|m| m.get("primary"))
            .and_then(|p| p.get("family_size"))
            .and_then(Json::as_int),
        Some(3)
    );
}

// AC-2 — the closed refusal set through the actual report path.
#[test]
fn refusal_table() {
    let f = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[], None);
    let ok = || spec(BootstrapPairedMethod::Percentile, base_filters());
    // One task.
    let f1 = fx(&[("t1", 1, 4)], &[], None);
    assert!(matches!(
        analyze(&ok(), &input(&f1, 2_000)),
        Err(AnalysisError::TaskMixture(tm::TaskMixtureError::OneTask))
    ));
    // Missing cluster: an unresolved member (metric cell absent).
    let fu = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[("A", "t1", 0)], None);
    assert!(matches!(
        analyze(&ok(), &input(&fu, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::UnresolvedCell { .. }
        ))
    ));
    // Unpaired task: arm B of t2 absent.
    let fp = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[], Some(("t2", "B")));
    assert!(matches!(
        analyze(&ok(), &input(&fp, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::Population(
                hharness_eval::population::PopulationError::UnpairedTask { .. }
            )
        ))
    ));
    // Degenerate: identical deltas on every task.
    let fd = fx(&[("t1", 2, 4), ("t2", 2, 4)], &[], None);
    assert!(matches!(
        analyze(&ok(), &input(&fd, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::DegenerateBootstrap
        ))
    ));
    let with = |kv: &[(&str, &str)]| {
        let mut pairs: Vec<(String, Json)> = vec![
            ("test_arm".into(), Json::str("B")),
            ("control_arm".into(), Json::str("A")),
            ("null".into(), Json::str("sharp_no_effect")),
            ("exchangeability".into(), Json::str("lane_randomization")),
        ];
        for (k, v) in kv {
            pairs.retain(|(pk, _)| pk != k);
            pairs.push((k.to_string(), Json::str(*v)));
        }
        spec(
            BootstrapPairedMethod::Percentile,
            Json::Obj(pairs.into_iter().collect()),
        )
    };
    // Shared-shock dependence declared.
    assert!(matches!(
        analyze(&with(&[("dependence", "shared_shocks")]), &input(&f, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::UnsupportedDependence { .. }
        ))
    ));
    // Weak-mean / practical-margin nulls refuse.
    for n in ["weak_mean", "practical_margin", "omnibus"] {
        let r = analyze(&with(&[("null", n)]), &input(&f, 2_000));
        assert!(
            matches!(
                r,
                Err(AnalysisError::TaskMixture(
                    tm::TaskMixtureError::UnsupportedNull { .. }
                ))
            ),
            "null {n}: {r:?}"
        );
    }
    // No admitted exchangeability.
    for x in ["shared_shocks", "crn", "carryover", "assumed"] {
        let r = analyze(&with(&[("exchangeability", x)]), &input(&f, 2_000));
        assert!(
            matches!(
                r,
                Err(AnalysisError::TaskMixture(
                    tm::TaskMixtureError::ExchangeabilityNotAdmitted { .. }
                ))
            ),
            "exchangeability {x}: {r:?}"
        );
    }
    // Omnibus/practical-margin verdict use refuses.
    assert!(matches!(
        analyze(&with(&[("verdict", "omnibus")]), &input(&f, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::UnsupportedVerdict { .. }
        ))
    ));
    // Draws below the confirmatory floor.
    assert!(matches!(
        analyze(&ok(), &input(&f, 1_999)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::DrawsBelowFloor { .. }
        ))
    ));
    // Level other than the frozen 95%.
    let mut inp = input(&f, 2_000);
    inp.confidence_ppm = 900_000;
    assert!(matches!(
        analyze(&ok(), &inp),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::UnsupportedLevel { .. }
        ))
    ));
    // Report member not in the frozen family → malformed identity.
    assert!(matches!(
        analyze(
            &spec(
                BootstrapPairedMethod::Percentile,
                filters_with_member("not_a_member")
            ),
            &input(&f, 2_000)
        ),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::FamilyIdentity { .. }
        ))
    ));
    // Method vocabulary outside BootstrapPaired → unknown method.
    let mut s = ok();
    s.estimator_selection.method = IntervalMethod::Wilson;
    s.spec_id = s.spec_id();
    assert!(matches!(
        analyze(&s, &input(&f, 2_000)),
        Err(AnalysisError::TaskMixture(
            tm::TaskMixtureError::UnknownMethod { .. }
        ))
    ));
    // Missing bound experiment view.
    let mut inp = input(&f, 2_000);
    inp.view = None;
    assert!(matches!(
        analyze(&ok(), &inp),
        Err(AnalysisError::MissingSpecRef { .. })
    ));
    // No sign-flip declaration → the leg reports not_declared, and the
    // own family member stays unavailable (never dropped).
    let fl = Json::obj([
        ("test_arm", Json::str("B")),
        ("control_arm", Json::str("A")),
        (
            "family",
            Json::obj([
                ("id", Json::str("d2_primaries")),
                ("member", Json::str("m1")),
                (
                    "members",
                    Json::Arr(vec![
                        Json::obj([("member", Json::str("m1"))]),
                        Json::obj([
                            ("member", Json::str("m2")),
                            ("raw_p_ppm", Json::Int(30_000)),
                        ]),
                    ]),
                ),
            ]),
        ),
    ]);
    let b = analyze(
        &spec(BootstrapPairedMethod::Percentile, fl),
        &input(&f, 2_000),
    )
    .unwrap()
    .body;
    let b = b.get("task_mixture_sensitivity").unwrap();
    assert_eq!(
        b.get("sign_flip")
            .and_then(|x| x.get("status"))
            .and_then(Json::as_str),
        Some("not_declared")
    );
    assert_eq!(
        member(&b, "m1")[0].get("status").and_then(Json::as_str),
        Some("unavailable")
    );
}

// AC-2 — changed declared draws change the computation and the
// persisted identity (draws are consumed, never label-only).
#[test]
fn changed_draws_change_computation() {
    let f = fx(&[("t1", 1, 4), ("t2", 3, 2), ("t3", 0, 5)], &[], None);
    let s = spec(BootstrapPairedMethod::Percentile, base_filters());
    let docs = LabDocs::open(&tmp("draws")).unwrap();
    let r1 = analyze_and_record(&docs, &s, &input(&f, 2_000)).unwrap();
    let r2 = analyze_and_record(&docs, &s, &input(&f, 4_000)).unwrap();
    assert_ne!(r1.report_id, r2.report_id);
    assert_ne!(r1.body.to_canonical_string(), r2.body.to_canonical_string());
    assert_eq!(
        r2.body
            .get("task_mixture_sensitivity")
            .and_then(|b| b.get("bootstrap"))
            .and_then(|b| b.get("draws"))
            .and_then(|d| d.get("used"))
            .and_then(Json::as_int),
        Some(4_000)
    );
}

// Smoke: the projected EvalRuns feed the population adapter (the
// fixture's cells resolve to member success flags).
#[test]
fn projection_feeds_binary_cells() {
    let f = fx(&[("t1", 1, 4), ("t2", 3, 2)], &[], None);
    let runs: Vec<EvalRun> = f
        .rows
        .iter()
        .map(|r| {
            project_eval_run(
                r,
                f.manifests.get(&r.key.run_id),
                f.facts.get(&r.key.run_id).cloned().unwrap_or_default(),
                None,
                f.suites.first(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(runs.len(), 20);
    let b_t1: Vec<&EvalRun> = runs
        .iter()
        .filter(|r| r.arm_id == "B" && r.task_id == "t1")
        .collect();
    assert_eq!(
        b_t1.iter()
            .filter(|r| matches!(r.value_for("task_success"), MetricValueKind::Bool(true)))
            .count(),
        4
    );
}
