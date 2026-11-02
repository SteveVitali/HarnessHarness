//! R5.16 — the routed kind battery: `AnalysisSpec.kind =
//! "finite_manifest_contrast"` through `analyze` → `analyze_and_record`
//! → the persisted `analysis_report_body/1` (REQ-D-107 primary; Packet D
//! / PADR-R5-4*). Fails-without: the kind arm, `AnalysisInput.view` and
//! `AnalysisError::Contrast` do not exist at the R5.15b base — this file
//! compiles red there.

use std::collections::BTreeMap;

use hharness_analysis::{
    analyze, analyze_and_record, eval_run as project_eval_run, AnalysisError, AnalysisInput,
};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::runs::{EvalRun, SuiteContext, TaskContext};
use hharness_experiment::docs::LabDocs;
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

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-16-analysis-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── EX-02 fixture: two tasks × two arms × five repetitions ─────────────
// Task 1: A = 1/5, B = 4/5. Task 2: A = 3/5, B = 2/5 (the pinned EX-02
// sequences; observed equal-task contrast .20, posterior mean 1/7).

const EX02: &[(&str, &str, &[bool])] = &[
    ("t1", "A", &[false, false, false, false, true]),
    ("t1", "B", &[true, true, true, true, false]),
    ("t2", "A", &[true, true, true, false, false]),
    ("t2", "B", &[true, true, false, false, false]),
];

fn run_id(arm: &str, task: &str, rep: usize) -> String {
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
                        run_id: run_id(arm, task, i),
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
    let rid = run_id(arm, task, rep);
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
        view: view(),
        decls: vec![hharness_eval::catalogue::metric("task_success").expect("catalogue metric")],
    }
}

impl Fx {
    fn runs(&self) -> Vec<EvalRun> {
        self.rows
            .iter()
            .map(|r| {
                project_eval_run(
                    r,
                    self.manifests.get(&r.key.run_id),
                    self.facts.get(&r.key.run_id).cloned().unwrap_or_default(),
                    None,
                    self.suites.first(),
                )
                .unwrap()
            })
            .collect()
    }
}

fn fmc_spec(prior: &str) -> AnalysisSpec {
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

// AC-1 — the routed path: planned population → estimator → persisted
// report body member with the EX-02 known answers.
#[test]
fn kind_routes_population_to_persisted_report() {
    let f = fx(7);
    let spec = fmc_spec("beta{1,1}");
    let out = analyze(&spec, &input(&f)).unwrap();
    let body = out
        .body
        .get("finite_manifest_contrast")
        .expect("body member");
    assert_eq!(
        body.get("observed_contrast_ppm").and_then(Json::as_int),
        Some(200_000)
    );
    assert_eq!(
        body.get("posterior_mean_ppm").and_then(Json::as_int),
        Some(142_857)
    );
    assert_eq!(
        body.get("interval")
            .and_then(|i| i.get("kind"))
            .and_then(Json::as_str),
        Some("credible")
    );
    assert_eq!(out.record.kind.as_deref(), Some("finite_manifest_contrast"));
    // The persisted report: deterministic id, idempotent re-serve.
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let r1 = analyze_and_record(&docs, &spec, &input(&f)).unwrap();
    let r2 = analyze_and_record(&docs, &spec, &input(&f)).unwrap();
    assert_eq!(r1.report_id, r2.report_id);
    assert_eq!(r1.body.to_canonical_string(), r2.body.to_canonical_string());
    assert!(r1.record.outputs.contains(&r1.report_id));
}

// AC-1 — a changed valid input changes the persisted body and identity:
// one flipped observation (with its honest watermark) is a different
// `(spec, input)` — a different report id AND a different body.
#[test]
fn changed_input_changes_body_and_identity() {
    let f = fx(7);
    let spec = fmc_spec("beta{1,1}");
    let docs = LabDocs::open(&tmp("identity")).unwrap();
    let r1 = analyze_and_record(&docs, &spec, &input(&f)).unwrap();
    // Flip one fixture observation (task 1 control, rep 0 → success)
    // under a later watermark — the honest changed input.
    let mut f2 = fx(9);
    for r in &mut f2.rows {
        if r.key.run_id == "r-A-t1-0" {
            r.cells[0].value = MetricValueKind::Bool(true);
        }
    }
    let r2 = analyze_and_record(&docs, &spec, &input(&f2)).unwrap();
    assert_ne!(r1.report_id, r2.report_id);
    assert_ne!(r1.body.to_canonical_string(), r2.body.to_canonical_string());
    assert_eq!(
        r2.body
            .get("finite_manifest_contrast")
            .and_then(|b| b.get("observed_contrast_ppm"))
            .and_then(Json::as_int),
        Some(100_000) // t1 control 2/5 → task-1 delta .4 → (.4 − .2)/2
    );
}

// AC-2 — metadata-only switch: the admitted method declared on a kind
// that does not compute it refuses; an unknown method on the kind
// refuses; a missing bound experiment view refuses.
#[test]
fn declaration_refusals() {
    let f = fx(7);
    // Metadata-only: method declared on `summarize`.
    let mut s = fmc_spec("beta{1,1}");
    s.kind = "summarize".into();
    s.spec_id = s.spec_id();
    assert!(matches!(
        analyze(&s, &input(&f)),
        Err(AnalysisError::Contrast(
            hharness_eval::finite_manifest::ContrastError::MetadataOnly { .. }
        ))
    ));
    // Unknown method on the kind.
    let mut s = fmc_spec("beta{1,1}");
    s.estimator_selection.method = IntervalMethod::ClusteredClt;
    s.spec_id = s.spec_id();
    assert!(matches!(
        analyze(&s, &input(&f)),
        Err(AnalysisError::Contrast(
            hharness_eval::finite_manifest::ContrastError::UnknownMethod { .. }
        ))
    ));
    // Pooled substitute declared through the prior name.
    let s = fmc_spec("pooled");
    assert!(matches!(
        analyze(&s, &input(&f)),
        Err(AnalysisError::Contrast(
            hharness_eval::finite_manifest::ContrastError::PooledSubstitute { .. }
        ))
    ));
    // Missing bound experiment view.
    let mut inp = input(&f);
    inp.view = None;
    assert!(matches!(
        analyze(&fmc_spec("beta{1,1}"), &inp),
        Err(AnalysisError::MissingSpecRef { .. })
    ));
}

// The projected EvalRuns carry the fixture's binary cells into the
// population adapter (smoke: the metric resolves to success flags).
#[test]
fn projection_feeds_binary_cells() {
    let f = fx(7);
    let runs = f.runs();
    assert_eq!(runs.len(), 20);
    let b_t1: Vec<&EvalRun> = runs
        .iter()
        .filter(|r| r.arm_id == "B" && r.task_id == "t1")
        .collect();
    assert_eq!(b_t1.len(), 5);
    let successes = b_t1
        .iter()
        .filter(|r| matches!(r.value_for("task_success"), MetricValueKind::Bool(true)))
        .count();
    assert_eq!(successes, 4);
}
