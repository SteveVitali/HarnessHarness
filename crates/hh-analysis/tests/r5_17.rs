//! R5.17 — independent verification of the early offline efficacy
//! route (contributing only: REQ-D-109/REQ-D-310/REQ-E-209/REQ-H-010 —
//! never a second primary owner; the full simulation battery and the
//! method's numerical-validation predicates stay owed to
//! R5.40/GATE-G10 per the GATE-G9 exclusion — this attests the narrow
//! EX-02 teaching scope only).
//!
//! Every scenario drives the real landed route — `kind =
//! "finite_manifest_contrast"` → `analyze`/`analyze_and_record` → the
//! persisted `analysis_report_body/1` (R5.15 population → R5.16
//! estimator → R5.16b routing, consumed unchanged). Expectations are
//! derived INDEPENDENTLY: exact rationals for the known answers and an
//! in-file SplitMix64 + integer-Beta order-statistic sampler (a
//! different generator, sampler and quantile than the seeded
//! inverse-CDF path) for the credible interval. An unrun scenario, a
//! discrepancy, missing method metadata or an unknown error is never a
//! pass (AC-2 — proven by `*_cannot_pass`).
//!
//! Fails-without: `IntervalMethod::FiniteManifestContrast` and the
//! routed kind do not exist before seq 185/185a — compiles red there.

use std::collections::BTreeMap;

use hharness_analysis::{
    analyze, analyze_and_record, AnalysisError, AnalysisInput, AnalysisOutcome,
};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::finite_manifest::ContrastError;
use hharness_eval::runs::SuiteContext;
use hharness_experiment::docs::LabDocs;
use hharness_experiment::view::{Attempt, ExperimentView, PlanState};
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{EstimatorSelection, IntervalMethod, MetricValueKind};
use hharness_results::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use hharness_results::scoring::ScoringContext;
use hharness_results::watermark::WatermarkSet;
use hharness_wire::Json;

// ── scenario harness ─────────────────────────────────────────────────
// `overall` is green only when every executed scenario carries Pass or
// an ExpectedRefusal — Unrun/Fail/UnexpectedRefusal/UnknownError are
// never a pass.

#[rustfmt::skip]
#[derive(Debug)]
enum Verdict {
    Pass,
    Fail { expected: String, actual: String },
    ExpectedRefusal(&'static str), UnexpectedRefusal(String),
    Unrun, UnknownError(String),
}

#[rustfmt::skip]
struct Scenario { name: &'static str, verdict: Verdict }

#[rustfmt::skip]
fn judged(name: &'static str, ok: bool, detail: String) -> Scenario {
    let verdict = if ok {
        Verdict::Pass
    } else {
        Verdict::Fail { expected: "predicate".into(), actual: detail }
    };
    Scenario { name, verdict }
}

/// A routed call expected to refuse through `AnalysisError::Contrast`
/// with a specific variant; a computed result, a different refusal or
/// a different error class is never a pass.
#[rustfmt::skip]
fn expect_contrast_refusal(
    name: &'static str,
    want: &'static str,
    got: Result<AnalysisOutcome, AnalysisError>,
    matches: impl Fn(&ContrastError) -> bool,
) -> Scenario {
    let verdict = match got {
        Err(AnalysisError::Contrast(e)) if matches(&e) => Verdict::ExpectedRefusal(want),
        Err(AnalysisError::Contrast(e)) => Verdict::UnexpectedRefusal(e.to_string()),
        Err(other) => Verdict::UnknownError(format!("{other}")),
        Ok(_) => Verdict::Fail { expected: format!("refusal {want}"), actual: "computed".into() },
    };
    Scenario { name, verdict }
}

#[rustfmt::skip]
fn overall(scenarios: &[Scenario]) -> Result<(), String> {
    let bad: Vec<String> = scenarios
        .iter()
        .filter(|s| !matches!(s.verdict, Verdict::Pass | Verdict::ExpectedRefusal(_)))
        .map(|s| format!("{}: {:?}", s.name, s.verdict))
        .collect();
    if bad.is_empty() { Ok(()) } else { Err(bad.join("; ")) }
}

// ── fixture (parameterized `(task, arm, outcomes)` cells) ────────────
// `None` binds an attempt with no outcome and no row — genuinely unrun.

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("hh-r5-17-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn outs(c: usize, n: usize) -> Vec<Option<bool>> {
    (0..n).map(|i| Some(i < c)).collect()
}

#[rustfmt::skip]
fn view(cells: &[(&str, &str, Vec<Option<bool>>)]) -> ExperimentView {
    let mut v = ExperimentView::default();
    for (task, arm, outcomes) in cells {
        for (i, ok) in outcomes.iter().enumerate() {
            let rpid = format!("p-{arm}-{task}-{i}");
            v.plans.insert(
                rpid.clone(),
                PlanState {
                    run_plan_id: rpid, cell_id: format!("{arm}:{task}"),
                    arm_id: arm.to_string(), task_id: task.to_string(),
                    replicate_index: i as u32, configuration_version_id: "cfg".into(),
                    split_label: "held_out".into(), seed_material: Json::Null,
                    order_pos: i as u64, claim: None,
                    attempts: vec![Attempt {
                        run_id: format!("r-{arm}-{task}-{i}"), attempt_no: 1,
                        budget_id: "b".into(), bound: true,
                        outcome: ok.map(|_| OutcomeClass::Scored), accepted: false,
                        superseded: false, excluded: None, veto_tripped: vec![],
                        budget_utilization: None, regrade_pending: false, pools: vec![],
                    }],
                    next_attempt_no: 2, not_before_ms: 0, accepted_run: None,
                    finished_final: false, launch_binding: None,
                    grade_pending: false, disposition: None,
                },
            );
        }
    }
    v
}

#[rustfmt::skip]
fn row(arm: &str, task: &str, rep: usize, pass: bool, pin: u64) -> ResultsRow {
    let rid = format!("r-{arm}-{task}-{rep}");
    let mut wm = WatermarkSet::new();
    wm.pin(&rid, pin);
    ResultsRow {
        key: RowKey { configuration_version_id: "cv".into(), run_id: rid.clone() },
        coordinates: Coordinates {
            configuration_id: Some("cfg".into()), configuration_version_id: "cv".into(),
            model_snapshots: BTreeMap::new(), harness_def_ref: None,
            model_profile_ref: None, environment_ref: None,
            environment_version_id: Some("env".into()),
            task: Some(Json::obj([
                ("task_id", Json::str(task)), ("suite_id", Json::str("suite")),
                ("split_label", Json::str("held_out")),
            ])),
            budget: Json::obj([]), replicate: Json::obj([]),
            participant_class: "native".into(),
            observability_level: vec!["ledger".into()],
            hosting_mechanism: None, capability_vector_ref: None,
            mediation: vec![], registry_snapshot_id: None,
        },
        experiment: Some(Json::obj([
            ("experiment_run_id", Json::str("exp-1")), ("arm_id", Json::str(arm)),
            ("cell_id", Json::str(format!("{arm}:{task}"))),
            ("replicate_index", Json::Int(rep as i64)), ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ])),
        outcome: OutcomeSection {
            status: "finished".into(), outcome_class: Some("scored".into()),
            stop_reason: Some("completed".into()), activation_no: 1,
            ..Default::default()
        },
        cells: vec![Cell {
            metric_ref: "task_success".into(), value: MetricValueKind::Bool(pass),
            detector: Some("deterministic".into()), oracle_ref: Some("oracle/executable".into()),
            confidence: None, evidence: vec![], computed_from: vec![],
        }],
        consumption: ConsumptionSection::default(),
        cache: Json::obj([]), lineage: Json::obj([]),
        annotations_from_ledger: Json::obj([]),
        audit: AuditSection {
            head: hharness_results::audit::AuditRef {
                run_id: rid.clone(), seq: pin, hash: "idp:head".into(),
                checkpoint_ref: None, content_refs: vec![],
            },
            checkpoint_ref: None,
        },
        scoring: ScoringContext::native(),
        derived_from: DerivedFrom {
            run_id: rid.clone(), seq: pin, overlay_watermarks: BTreeMap::new(),
        },
        watermark_set: wm, view_hash: "v".into(), version_id: format!("v-{rid}"),
    }
}

#[rustfmt::skip]
struct Fx {
    rows: Vec<ResultsRow>,
    manifests: BTreeMap<String, RunManifest>,
    facts: BTreeMap<String, LedgerFacts>,
    watermarks: WatermarkSet, view: ExperimentView, suites: Vec<SuiteContext>,
    decls: Vec<hharness_ontology::compliance::MetricDeclaration>,
}

#[rustfmt::skip]
fn fx(cells: Vec<(&str, &str, Vec<Option<bool>>)>, pin: u64) -> Fx {
    let rows: Vec<ResultsRow> = cells
        .iter()
        .flat_map(|(task, arm, outcomes)| {
            outcomes
                .iter()
                .enumerate()
                .filter_map(|(i, ok)| ok.map(|v| row(arm, task, i, v, pin)))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut watermarks = WatermarkSet::new();
    let mut manifests = BTreeMap::new();
    let mut facts = BTreeMap::new();
    for r in &rows {
        for (rid, seq) in &r.watermark_set.runs {
            watermarks.pin(rid, *seq);
        }
        manifests.insert(r.key.run_id.clone(), RunManifest::minimal(RunKind::Agent));
        facts.insert(r.key.run_id.clone(), LedgerFacts::default());
    }
    Fx { rows, manifests, facts, watermarks, view: view(&cells),
        suites: vec![SuiteContext {
            suite_id: "suite".into(), retired_for_headline: false,
            family: hharness_ontology::lab::EnvironmentFamily::CodingTerminal,
        }],
        decls: vec![hharness_eval::catalogue::metric("task_success").expect("catalogue metric")] }
}

/// The pinned EX-02 manifest: 2 tasks × 2 arms × 5 repetitions —
/// task 1: A = 1/5, B = 4/5; task 2: A = 3/5, B = 2/5.
fn ex02() -> Vec<(&'static str, &'static str, Vec<Option<bool>>)> {
    vec![
        ("t1", "A", outs(1, 5)),
        ("t1", "B", outs(4, 5)),
        ("t2", "A", outs(3, 5)),
        ("t2", "B", outs(2, 5)),
    ]
}

#[rustfmt::skip]
fn fmc_spec(prior: &str) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(), kind: "finite_manifest_contrast".into(),
        query: QuerySpec {
            metrics: vec!["task_success".into()],
            filters: Some(Json::obj([
                ("test_arm", Json::str("B")), ("control_arm", Json::str("A")),
            ])),
            grain: None,
        },
        spec_ref: Some("exp-spec".into()), label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::FiniteManifestContrast(prior.into()),
            selection_rule: "padr-r5-4.finite_manifest".into(),
            floors: BTreeMap::new(), fallback_chain: vec![], substituted: None,
        },
        resample: None, outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    s
}

#[rustfmt::skip]
fn input<'a>(f: &'a Fx) -> AnalysisInput<'a> {
    AnalysisInput {
        rows: &f.rows, manifests: &f.manifests, facts: &f.facts,
        tasks: &[], suites: &f.suites, design: None, arm_specs: &[],
        declarations: &f.decls, pre_registration: None,
        watermark_set: &f.watermarks,
        metric_registry_version: "test-registry", price_table_version: "test-pricing",
        confidence_ppm: 950_000, resampling_draws: 2000, seed: 0,
        post_amendment: false, view: Some(&f.view),
    }
}

#[rustfmt::skip]
fn routed_body(cells: Vec<(&str, &str, Vec<Option<bool>>)>, prior: &str) -> Json {
    let f = fx(cells, 7);
    analyze(&fmc_spec(prior), &input(&f))
        .map(|o| o.body.get("finite_manifest_contrast").cloned().unwrap_or(Json::Null))
        .unwrap_or(Json::Null)
}

// ── independent numerics (never the estimator's code path) ───────────

/// SplitMix64 — a different generator than the estimator's XorShift64.
struct Sm64(u64);

#[rustfmt::skip]
impl Sm64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
}

/// Independent Beta(a,b) draw for integer a,b: the a-th order
/// statistic of a+b−1 iid uniforms (U_(k) ~ Beta(k, n−k+1)).
fn beta_draw(rng: &mut Sm64, a: u64, b: u64) -> f64 {
    let mut xs: Vec<f64> = (0..(a + b - 1)).map(|_| rng.unit()).collect();
    xs.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    xs[(a - 1) as usize]
}

/// The persisted `(test_c, test_n, control_c, control_n)` cells — the
/// check reads what the route persisted, not the fixture it was fed.
#[rustfmt::skip]
fn body_cells(body: &Json) -> Vec<(u64, u64, u64, u64)> {
    match body.get("cells") {
        Some(Json::Arr(cells)) => cells
            .iter()
            .filter_map(|c| {
                let g = |arm: &str, k: &str| {
                    c.get(arm).and_then(|a| a.get(k)).and_then(Json::as_int).map(|v| v as u64)
                };
                Some((g("test", "c")?, g("test", "n")?, g("control", "c")?, g("control", "n")?))
            })
            .collect(),
        _ => vec![],
    }
}

/// Independent equal-tail endpoints of `Δ = Σ_t w_t (p_Bt − p_At)`
/// (w_t = 1/T) in ppm — own generator, own sampler, own quantile.
#[rustfmt::skip]
fn independent_endpoints(
    cells: &[(u64, u64, u64, u64)],
    prior: (u64, u64),
    draws: usize,
    seed: u64,
) -> (i64, i64) {
    let mut rng = Sm64(seed ^ 0x517C_0DE5_BA5E);
    let mut ds: Vec<f64> = Vec::with_capacity(draws);
    for _ in 0..draws {
        let mut sum = 0.0;
        for &(tc, tn, cc, cn) in cells {
            sum += beta_draw(&mut rng, tc + prior.0, tn - tc + prior.1)
                - beta_draw(&mut rng, cc + prior.0, cn - cc + prior.1);
        }
        ds.push(sum / cells.len() as f64);
    }
    ds.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    ((ds[(25 * draws / 1000).min(draws - 1)] * 1e6).round() as i64,
     (ds[(975 * draws / 1000).min(draws - 1)] * 1e6).round() as i64)
}

/// Exact rational → ppm (the file's own expectation arithmetic).
fn exact_ppm(numer: i128, denom: i128) -> i64 {
    let a = numer * 1_000_000;
    ((if a >= 0 { a + denom / 2 } else { a - denom / 2 }) / denom) as i64
}

// ── scenarios ────────────────────────────────────────────────────────

// AC-1 — EX-02 known answers as exact rationals: observed = 1/5;
// posterior mean = 1/7.
#[rustfmt::skip]
fn ex02_known_answers() -> Scenario {
    let b = routed_body(ex02(), "beta{1,1}");
    let ok = b.get("observed_contrast_ppm").and_then(Json::as_int) == Some(exact_ppm(1, 5))
        && b.get("posterior_mean_ppm").and_then(Json::as_int) == Some(exact_ppm(1, 7));
    judged("ex02_known_answers", ok, format!("body member {b:?}"))
}

// AC-1 — the reported 95% equal-tail credible interval is reproduced
// by the independent sampler over the persisted cells (declared tol).
const INDEPENDENT_TOL_PPM: i64 = 20_000;

#[rustfmt::skip]
fn ex02_interval_independent_check() -> Scenario {
    let b = routed_body(ex02(), "beta{1,1}");
    let cells = body_cells(&b);
    let (lo, hi) = independent_endpoints(&cells, (1, 1), 60_000, 0x5EED);
    let get = |k: &str| {
        b.get("interval").and_then(|i| i.get(k)).and_then(Json::as_int).unwrap_or(i64::MIN)
    };
    let (rl, rh) = (get("lo_ppm"), get("hi_ppm"));
    judged("ex02_interval_independent_check",
        cells.len() == 2
            && (rl - lo).abs() <= INDEPENDENT_TOL_PPM
            && (rh - hi).abs() <= INDEPENDENT_TOL_PPM,
        format!("reported [{rl},{rh}] vs independent [{lo},{hi}] (tol {INDEPENDENT_TOL_PPM})"))
}

// AC-1 — prior placement: beta{1,1} labels `registered`; predeclared
// beta{2,2} labels `predeclared_exploratory` with its declared params
// recorded and a body distinct from registered. The exploratory arm's
// posterior parameterization is recorded defect DF-R5.17-1 (owed to
// R5.40/GATE-G10 — never asserted here).
#[rustfmt::skip]
fn prior_sensitivity_placement() -> Scenario {
    let reg = routed_body(ex02(), "beta{1,1}");
    let sen = routed_body(ex02(), "beta{2,2}");
    fn prior_label(b: &Json) -> Option<&str> {
        b.get("prior").and_then(|p| p.get("label")).and_then(Json::as_str)
    }
    let ok = prior_label(&reg) == Some("registered")
        && prior_label(&sen) == Some("predeclared_exploratory")
        && sen.get("prior").and_then(|p| p.get("beta")).and_then(|b| b.get("a"))
            .and_then(Json::as_int)
            == Some(2)
        && sen.get("metadata").and_then(|m| m.get("prior")).and_then(Json::as_str)
            == Some("beta{2,2}")
        && reg.to_canonical_string() != sen.to_canonical_string();
    judged("prior_sensitivity_placement", ok,
        format!("labels {:?}/{:?}", prior_label(&reg), prior_label(&sen)))
}

// AC-1 — heterogeneity: unequal-n cells keep equal-task weights
// (equal-task 17/40, never pooled 5/13); per-task cells stay visible.
#[rustfmt::skip]
fn heterogeneity_unequal_n() -> Scenario {
    let b = routed_body(
        vec![("t1", "A", outs(1, 5)), ("t1", "B", outs(4, 5)),
             ("t2", "A", outs(5, 8)), ("t2", "B", outs(7, 8))],
        "beta{1,1}",
    );
    // Posterior mean: (5/7−2/7 + 9/10−7/10)/2 = (3/7 + 1/5)/2 = 11/35.
    let got = b.get("observed_contrast_ppm").and_then(Json::as_int);
    let pm = b.get("posterior_mean_ppm").and_then(Json::as_int);
    judged("heterogeneity_unequal_n",
        got == Some(exact_ppm(17, 40))
            && got != Some(exact_ppm(5, 13))
            && pm == Some(exact_ppm(11, 35))
            && body_cells(&b).len() == 2,
        format!("observed {got:?} vs equal-task 425000 / pooled ~384615; pm {pm:?}"))
}

// AC-1 — missingness: unrun member and below-floor cell refuse typed.
#[rustfmt::skip]
fn missingness_unrun_member() -> Scenario {
    let mut cells = ex02();
    cells[3].2[4] = None;
    let f = fx(cells, 7);
    expect_contrast_refusal("missingness_unrun_member", "UnresolvedCell",
        analyze(&fmc_spec("beta{1,1}"), &input(&f)),
        |e| matches!(e, ContrastError::UnresolvedCell { .. }))
}

#[rustfmt::skip]
fn missingness_short_cell() -> Scenario {
    let mut cells = ex02();
    cells[3].2.truncate(4); // t2/B n=4 < the R≥5 first-slot floor
    let f = fx(cells, 7);
    expect_contrast_refusal("missingness_short_cell", "ShortCell",
        analyze(&fmc_spec("beta{1,1}"), &input(&f)),
        |e| matches!(e, ContrastError::ShortCell { .. }))
}

// AC-1 — instrumentation: the persisted report carries the actual
// selected draws/method/report identity — deterministic report_id,
// re-serve, record.kind, the full S.1 metadata record, the
// 8,000→64,000 doubling ladder, seed/tol/schema — int-canonical ppm.
#[rustfmt::skip]
fn report_persistence_instrumentation() -> Scenario {
    let f = fx(ex02(), 7);
    let spec = fmc_spec("beta{1,1}");
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let pair = (
        analyze_and_record(&docs, &spec, &input(&f)),
        analyze_and_record(&docs, &spec, &input(&f)),
    );
    let (r1, r2) = match pair {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            return judged(
                "report_persistence_instrumentation",
                false,
                "analyze_and_record refused".into(),
            )
        }
    };
    let b = r1.body.get("finite_manifest_contrast").cloned().unwrap_or(Json::Null);
    let draws = b.get("draws").cloned().unwrap_or(Json::Null);
    let md = b.get("metadata").cloned().unwrap_or(Json::Null);
    let s1_members = [
        "estimator", "version", "target", "assumptions", "prior", "keys",
        "stratum_membership", "seed", "draws_requested", "draws_used", "floors",
        "interval", "refusal",
    ]
    .iter()
    .all(|m| md.get(m).is_some());
    let int_member = |j: &Json, k: &str| matches!(j.get(k), Some(Json::Int(_))) || j.get(k).is_none();
    let ladder_ok = match draws.get("doubling") {
        Some(Json::Arr(d)) => {
            d.len() == 4
                && d.iter().all(|e| {
                    matches!(e.get("draws"), Some(Json::Int(_)))
                        && matches!(e.get("max_delta_ppm"), Some(Json::Int(_)))
                })
        }
        _ => false,
    };
    let ok = r1.report_id == r2.report_id
        && r1.body.to_canonical_string() == r2.body.to_canonical_string()
        && r1.record.outputs.contains(&r1.report_id)
        && r1.record.kind.as_deref() == Some("finite_manifest_contrast")
        && b.get("schema").and_then(Json::as_str) == Some("finite-manifest-contrast/1")
        && b.get("seed").and_then(Json::as_int) == Some(0)
        && draws.get("requested").and_then(Json::as_int) == Some(8_000)
        && draws.get("used").and_then(Json::as_int) == Some(64_000)
        && draws.get("tol_ppm").and_then(Json::as_int) == Some(1)
        && ladder_ok && s1_members
        && md.get("estimator").and_then(Json::as_str) == Some("finite_manifest_contrast")
        && md.get("prior").and_then(Json::as_str) == Some("beta{1,1}")
        && md.get("interval").and_then(|i| i.get("kind")).and_then(Json::as_str) == Some("credible")
        && md.get("interval").and_then(|i| i.get("level")).and_then(Json::as_int) == Some(950_000)
        && matches!(md.get("refusal"), Some(Json::Null))
        && ["observed_contrast_ppm", "posterior_mean_ppm", "numerical_error_ppm"]
            .iter()
            .all(|k| matches!(b.get(k), Some(Json::Int(_))))
        && int_member(&md, "seed") && int_member(&md, "draws_requested");
    judged(
        "report_persistence_instrumentation",
        ok,
        format!("report_id {} ladder_ok={ladder_ok} s1={s1_members}", r1.report_id),
    )
}

#[rustfmt::skip]
fn registry() -> Vec<Scenario> {
    vec![
        ex02_known_answers(), ex02_interval_independent_check(),
        prior_sensitivity_placement(), heterogeneity_unequal_n(),
        missingness_unrun_member(),
        missingness_short_cell(), report_persistence_instrumentation(),
    ]
}

// AC-1 — every declared scenario executed and carried a pass-class
// verdict (a missing scenario would shorten the registry, not pass it).
#[test]
fn early_route_battery_all_scenarios() {
    let scenarios = registry();
    assert_eq!(scenarios.len(), 7, "every declared scenario executed");
    if let Err(bad) = overall(&scenarios) {
        panic!("non-pass verdicts: {bad}");
    }
}

// AC-2 — an unrun scenario can never be a pass.
#[rustfmt::skip]
#[test]
fn unrun_scenario_cannot_pass() {
    let reg = vec![Scenario { name: "never_executed", verdict: Verdict::Unrun }];
    assert!(overall(&reg).is_err());
}

// AC-2 — an expected-vs-actual discrepancy is a Fail, never a pass.
#[test]
fn discrepancy_cannot_pass() {
    let s = judged("discrepancy", 200_000 == 199_999, "199999 ≠ 200000".into());
    assert!(matches!(s.verdict, Verdict::Fail { .. }));
    assert!(overall(&[s]).is_err());
}

// AC-2 — missing method metadata is a Fail, never a pass: strip
// `metadata` from a real persisted body and the check refuses.
#[rustfmt::skip]
#[test]
fn missing_method_metadata_cannot_pass() {
    let mut body = routed_body(ex02(), "beta{1,1}");
    if let Json::Obj(m) = &mut body {
        m.remove("metadata");
    }
    let s = judged("missing_metadata", body.get("metadata").is_some(), "metadata absent".into());
    assert!(matches!(s.verdict, Verdict::Fail { .. }));
    assert!(overall(&[s]).is_err());
}

// AC-2 — an error outside the declared refusal is an UnknownError,
// never a pass.
#[rustfmt::skip]
#[test]
fn unknown_error_cannot_pass() {
    let f = fx(ex02(), 7);
    let mut spec = fmc_spec("beta{1,1}");
    spec.query.metrics = vec!["nonexistent_metric".into()];
    spec.spec_id = spec.spec_id();
    let s = expect_contrast_refusal("unknown_error", "UnresolvedCell",
        analyze(&spec, &input(&f)),
        |e| matches!(e, ContrastError::UnresolvedCell { .. }));
    assert!(matches!(s.verdict, Verdict::UnknownError(_)));
    assert!(overall(&[s]).is_err());
}
