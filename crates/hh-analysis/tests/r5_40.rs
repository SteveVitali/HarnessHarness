//! R5.40 — composed numerical validation and independent method
//! evidence (M-G): the frozen `sim_battery/1` protocol over the exact
//! `population::assemble` → `finite_manifest` headline pipeline at the
//! declared ≥2,000-datasets/scenario floor, the DF-R5.17-1 prior
//! repair (beta{2,2} now parameterizes the conjugate update), and the
//! routed/persisted native report path for the exploratory arm.
//!
//! Fails-without: `hharness_eval::sim_battery` does not exist at base
//! `f5f9ae1e` (compile-red); on the base `finite_manifest.rs` the
//! beta{2,2} assertions fail (posterior mean reported 142_857 over
//! Beta(1,1) math — the recorded DF-R5.17-1 defect).
//!
//! Layer note: the mass battery executes OFFLINE under the cited D2
//! tranche (the retained record lives at
//! `docs/build/reports/`); this file proves the protocol, determinism,
//! estimator-identity, refusal/rollup semantics and the routed path at
//! probe scale — never a study-level or live claim.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use hharness_analysis::{analyze, analyze_and_record, AnalysisInput, AnalysisOutcome};
use hharness_eval::facts::LedgerFacts;
use hharness_eval::finite_manifest::{
    finite_manifest_contrast, finite_manifest_headline, ContrastDecl, ContrastError,
};
use hharness_eval::population::{self, BoundsDecl, PopulationKind};
use hharness_eval::runs::SuiteContext;
use hharness_eval::sim_battery::{
    battery_rollup, dataset, dataset_seed_material, run_scenario, scenarios, BatteryReport, Cap,
    ScenarioReport, Status, DATASETS_PER_SCENARIO,
};
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

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("hh-r5-40-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── routed fixture (the r5_17 EX-02 harness, consumed unchanged) ─────

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

/// The pinned EX-02 manifest: 2 tasks × 2 arms × 5 repetitions.
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
        .map(|o: AnalysisOutcome| o.body.get("finite_manifest_contrast").cloned().unwrap_or(Json::Null))
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

/// Independent Beta(a,b) draw: the a-th order statistic of a+b−1 iid
/// uniforms — a different sampler than the seeded inverse-CDF path.
fn beta_draw(rng: &mut Sm64, a: u64, b: u64) -> f64 {
    let mut xs: Vec<f64> = (0..(a + b - 1)).map(|_| rng.unit()).collect();
    xs.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    xs[(a - 1) as usize]
}

fn exact_ppm(numer: i128, denom: i128) -> i64 {
    let a = numer * 1_000_000;
    ((if a >= 0 { a + denom / 2 } else { a - denom / 2 }) / denom) as i64
}

// ── AC-1: frozen protocol + exact-pipeline evidence ─────────────────

// Every S.4 factor is present in the frozen table and every cell
// declares the ≥2,000-dataset floor; ids are unique seed material.
#[test]
fn protocol_table_frozen() {
    let table = scenarios();
    let mut ids = std::collections::BTreeSet::new();
    let (mut ts, mut rs, mut effs) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    let (mut easy, mut hard, mut het) = (0, 0, 0);
    let (mut mech, mut sens) = (0, 0);
    for s in &table {
        assert!(s.datasets >= DATASETS_PER_SCENARIO, "{} below floor", s.id);
        assert!(s.reps >= 5, "{} below the R>=5 floor", s.id);
        assert!(ids.insert(s.id.clone()), "duplicate scenario id {}", s.id);
        ts.insert(s.tasks);
        rs.insert(s.reps);
        effs.insert(s.effect_ppm);
        match s.mixture {
            hharness_eval::sim_battery::Mixture::Easy => easy += 1,
            hharness_eval::sim_battery::Mixture::Hard => hard += 1,
            hharness_eval::sim_battery::Mixture::Heterogeneous => het += 1,
        }
        if s.mechanism != hharness_eval::sim_battery::Mechanism::Clean {
            mech += 1;
        }
        if s.prior_sensitivity {
            sens += 1;
        }
    }
    assert_eq!(ts.into_iter().collect::<Vec<_>>(), vec![10, 20, 30]);
    assert_eq!(rs.into_iter().collect::<Vec<_>>(), vec![5, 10]);
    assert_eq!(
        effs.into_iter().collect::<Vec<_>>(),
        vec![-100_000, -50_000, 0, 50_000, 100_000]
    );
    assert!(easy >= 30 && hard >= 1 && het >= 1 && mech == 3 && sens >= 1);
    // The seed rule is canonical and derivable — never post-hoc.
    let m = dataset_seed_material("t10-r5-e+00-easy", 0);
    assert!(m.contains("sim-battery/1") && m.contains("t10-r5-e+00-easy"));
}

// Same replicate → byte-identical dataset; different → different.
#[test]
fn dataset_determinism() {
    let s = scenarios()
        .into_iter()
        .find(|s| s.id == "t10-r5-e+00-easy")
        .unwrap();
    let (a, b, c) = (dataset(&s, 7), dataset(&s, 7), dataset(&s, 8));
    let slots = |d: &hharness_eval::sim_battery::Dataset| {
        d.input
            .slots
            .iter()
            .map(|sl| (sl.plan_ref.clone(), sl.subjects[0].success))
            .collect::<Vec<_>>()
    };
    assert_eq!(slots(&a), slots(&b));
    assert_eq!(a.true_delta_ppm, b.true_delta_ppm);
    assert_ne!(slots(&a), slots(&c));
}

// The battery headline IS the estimator's reported headline — same
// validation, same first-8,000 seeded draws, same quantile — never a
// second implementation.
#[test]
fn headline_is_estimator() {
    let s = scenarios()
        .into_iter()
        .find(|s| s.id == "t10-r5-e+05-easy")
        .unwrap();
    for rep in [0u64, 3, 11] {
        let d = dataset(&s, rep);
        let pop = population::assemble(population::PopulationInput {
            slots: d.input.slots.clone(),
            required: d.input.required.clone(),
            pins: d.input.pins.clone(),
        })
        .unwrap();
        let decl = ContrastDecl {
            population: PopulationKind::Canonical,
            test_arm: "B".into(),
            control_arm: "A".into(),
            stratum: None,
            prior: (1, 1),
            prior_name: "beta{1,1}".into(),
            dependence: None,
            target: None,
            use_as: None,
            bounds: BoundsDecl::Unit,
            level_ppm: 950_000,
            seed: rep,
        };
        let full = finite_manifest_contrast(&pop, &decl).unwrap();
        let head = finite_manifest_headline(&pop, &decl).unwrap();
        assert_eq!(head.interval, full.interval, "rep {rep} interval");
        assert_eq!(head.posterior_mean_ppm, full.posterior_mean_ppm);
        assert_eq!(head.observed_contrast_ppm, full.observed_contrast_ppm);
    }
    // The headline refuses identically — a declared unadmitted prior is
    // never silently computed by the mass path either.
    let s = scenarios()
        .into_iter()
        .find(|s| s.id == "t10-r5-e+00-easy")
        .unwrap();
    let d = dataset(&s, 0);
    let pop = population::assemble(population::PopulationInput {
        slots: d.input.slots.clone(),
        required: d.input.required.clone(),
        pins: d.input.pins.clone(),
    })
    .unwrap();
    let decl = ContrastDecl {
        population: PopulationKind::Canonical,
        test_arm: "B".into(),
        control_arm: "A".into(),
        stratum: None,
        prior: (3, 3),
        prior_name: "beta{3,3}".into(),
        dependence: None,
        target: None,
        use_as: None,
        bounds: BoundsDecl::Unit,
        level_ppm: 950_000,
        seed: 0,
    };
    assert!(matches!(
        finite_manifest_headline(&pop, &decl),
        Err(ContrastError::UnsupportedPrior { .. })
    ));
}

// Probe legs — every mechanism behaves as designed at small scale.
#[test]
fn probe_scenario_legs() {
    let table = scenarios();
    let cap = Cap {
        max_datasets: 24,
        deadline: None,
    };
    let get = |id: &str| {
        let s = table.iter().find(|s| s.id == id).unwrap();
        run_scenario(s, &cap)
    };
    // Clean null cell: full availability, every dataset computes.
    let r = get("t10-r5-e+00-easy");
    assert_eq!(r.status, Status::Executed);
    assert_eq!(r.computed, r.datasets_run);
    assert_eq!(r.generator_failures, 0);
    assert!(r.coverage_ppm.is_some() && r.mc_error_ppm > 0);
    // Zero activation: nothing computes; unresolved cells refuse.
    let r = get("mech/zero-activation");
    assert_eq!(r.computed, 0);
    assert_eq!(r.availability_ppm, 0);
    assert_eq!(r.refused.values().sum::<u64>(), r.datasets_run);
    // Informative missingness/vetoes: missing members never improve a
    // denominator — unresolved refusals are recorded beside.
    let r = get("mech/informative-missingness-veto");
    assert!(r.refused.values().sum::<u64>() + r.computed == r.datasets_run);
    assert!(r.refused.values().sum::<u64>() > 0);
    // Shared shock: computes (dependence is assumed, never detected) —
    // the distortion is recorded as a fact, not a pass criterion.
    let r = get("mech/shared-shock");
    assert_eq!(r.computed, r.datasets_run);
}

// Armed cells carry the beta{2,2} diagnostic beside the registered arm.
#[test]
fn probe_prior_sensitivity_armed() {
    let table = scenarios();
    let cap = Cap {
        max_datasets: 16,
        deadline: None,
    };
    let s = table.iter().find(|s| s.id == "t10-r5-e+00-easy").unwrap();
    let r = run_scenario(s, &cap);
    assert!(
        r.prior_shift_ppm.is_some(),
        "armed cell missing the beta{{2,2}} leg"
    );
    let unarmed = table.iter().find(|s| s.id == "t20-r5-e+00-easy").unwrap();
    assert!(run_scenario(unarmed, &cap).prior_shift_ppm.is_none());
}

// ── AC-1/DF-R5.17-1: the prior now parameterizes the math ───────────

// The repaired exploratory arm: beta{2,2} EX-02 posterior mean is the
// exact 1/9 = 111_111 (registered stays 142_857 — byte-identical arm),
// and the 95% endpoints reproduce under an INDEPENDENT beta{2,2}
// sampler (SplitMix64 + order statistics — a different generator,
// sampler and quantile). "Changing metadata without changing
// computation" is exactly the recorded defect this closes.
#[test]
fn beta_2_2_known_answer_and_independent() {
    let b = routed_body(ex02(), "beta{2,2}");
    assert_eq!(
        b.get("posterior_mean_ppm").and_then(Json::as_int),
        Some(exact_ppm(1, 9)),
        "beta{{2,2}} posterior mean must be 1/9 — DF-R5.17-1 repair"
    );
    assert_eq!(
        b.get("observed_contrast_ppm").and_then(Json::as_int),
        Some(exact_ppm(1, 5))
    );
    let reg = routed_body(ex02(), "beta{1,1}");
    assert_eq!(
        reg.get("posterior_mean_ppm").and_then(Json::as_int),
        Some(exact_ppm(1, 7)),
        "registered arm byte-identical"
    );
    assert_ne!(
        b.get("posterior_mean_ppm").and_then(Json::as_int),
        reg.get("posterior_mean_ppm").and_then(Json::as_int),
        "the declared prior must change the computation"
    );
    // Independent beta{2,2} endpoints over the persisted cells.
    let cells: Vec<(u64, u64, u64, u64)> = match b.get("cells") {
        Some(Json::Arr(cs)) => cs
            .iter()
            .map(|c| {
                let g = |arm: &str, k: &str| {
                    c.get(arm)
                        .and_then(|a| a.get(k))
                        .and_then(Json::as_int)
                        .unwrap() as u64
                };
                (
                    g("test", "c"),
                    g("test", "n"),
                    g("control", "c"),
                    g("control", "n"),
                )
            })
            .collect(),
        _ => panic!("no cells"),
    };
    let mut rng = Sm64(0x5EED);
    let mut ds: Vec<f64> = Vec::new();
    for _ in 0..60_000 {
        let mut sum = 0.0;
        for &(tc, tn, cc, cn) in &cells {
            sum +=
                beta_draw(&mut rng, tc + 2, tn - tc + 2) - beta_draw(&mut rng, cc + 2, cn - cc + 2);
        }
        ds.push(sum / cells.len() as f64);
    }
    ds.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let (ilo, ihi) = (
        (ds[1_500] * 1e6).round() as i64,
        (ds[58_500] * 1e6).round() as i64,
    );
    let get = |k: &str| {
        b.get("interval")
            .and_then(|i| i.get(k))
            .and_then(Json::as_int)
            .unwrap_or(i64::MIN)
    };
    assert!(
        (get("lo_ppm") - ilo).abs() <= 20_000 && (get("hi_ppm") - ihi).abs() <= 20_000,
        "reported [{},{}] vs independent beta{{2,2}} [{ilo},{ihi}]",
        get("lo_ppm"),
        get("hi_ppm")
    );
}

// The native persisted-report path carries the exploratory arm: prior
// params + label + the repaired posterior + the S.1 metadata record.
#[test]
fn routed_beta_2_2_persisted() {
    let f = fx(ex02(), 7);
    let spec = fmc_spec("beta{2,2}");
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let r = analyze_and_record(&docs, &spec, &input(&f)).expect("routed beta{2,2}");
    let b = r
        .body
        .get("finite_manifest_contrast")
        .cloned()
        .unwrap_or(Json::Null);
    assert_eq!(
        b.get("schema").and_then(Json::as_str),
        Some("finite-manifest-contrast/1")
    );
    assert_eq!(
        b.get("prior")
            .and_then(|p| p.get("label"))
            .and_then(Json::as_str),
        Some("predeclared_exploratory")
    );
    assert_eq!(
        b.get("prior")
            .and_then(|p| p.get("beta"))
            .and_then(|q| q.get("a"))
            .and_then(Json::as_int),
        Some(2)
    );
    assert_eq!(
        b.get("posterior_mean_ppm").and_then(Json::as_int),
        Some(exact_ppm(1, 9))
    );
    assert_eq!(
        b.get("draws")
            .and_then(|d| d.get("requested"))
            .and_then(Json::as_int),
        Some(8_000)
    );
    assert_eq!(
        b.get("metadata")
            .and_then(|m| m.get("prior"))
            .and_then(Json::as_str),
        Some("beta{2,2}")
    );
    assert_eq!(r.record.kind.as_deref(), Some("finite_manifest_contrast"));
}

// A metadata-only declaration still fails — declaring the method on a
// kind that does not compute it.
#[test]
fn metadata_only_still_refuses() {
    let f = fx(ex02(), 7);
    let mut spec = fmc_spec("beta{1,1}");
    spec.kind = "nonexistent_kind".into();
    spec.spec_id = spec.spec_id();
    assert!(matches!(
        analyze(&spec, &input(&f)),
        Err(hharness_analysis::AnalysisError::Contrast(
            ContrastError::MetadataOnly { .. }
        ))
    ));
}

// ── AC-2: unrun/failed/mismatch/breach can never count ───────────────

// A wall-clock breach stops the cell and retains the partial — the
// rollup can never count it as method admission.
#[test]
fn stopped_budget_retains_partial_not_admitted() {
    let table = scenarios();
    let s = table.iter().find(|s| s.id == "t10-r5-e+00-easy").unwrap();
    let r = run_scenario(
        s,
        &Cap {
            max_datasets: u64::MAX,
            deadline: Some(Instant::now() + Duration::from_millis(1)),
        },
    );
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(r.status, Status::Stopped);
    assert!(r.datasets_run < r.datasets_declared);
    let rep = battery_rollup(vec![r]);
    assert!(
        !rep.is_complete(),
        "a stopped cell can never admit the method"
    );
    // A short-count cap likewise can never admit — the run is retained.
    let r = run_scenario(
        s,
        &Cap {
            max_datasets: 4,
            deadline: None,
        },
    );
    assert_eq!(r.datasets_run, 4);
    assert!(!battery_rollup(vec![r]).is_complete());
}

// An unrun/failed/mismatched member breaks the all-or-nothing rollup;
// there is no favorable-subset fallback.
#[test]
fn rollup_never_counts_unrun_failed_or_short() {
    let ok = |id: &str, run: u64, computed: u64, status: Status| ScenarioReport {
        id: id.into(),
        datasets_declared: DATASETS_PER_SCENARIO,
        datasets_run: run,
        computed,
        refused: BTreeMap::new(),
        generator_failures: 0,
        first_failure: None,
        availability_ppm: if run > 0 { 1_000_000 } else { 0 },
        coverage_ppm: Some(950_000),
        width_mean_ppm: Some(100_000),
        width_max_ppm: Some(100_000),
        mean_abs_err_ppm: Some(1_000),
        verdict_ppm: Some(50_000),
        prior_shift_ppm: None,
        mc_error_ppm: 1_000,
        wall_ms: 0,
        status,
    };
    let good = ok(
        "a",
        DATASETS_PER_SCENARIO,
        DATASETS_PER_SCENARIO,
        Status::Executed,
    );
    assert!(battery_rollup(vec![good.clone()]).is_complete());
    // Unrun (zero datasets).
    assert!(!battery_rollup(vec![ok("b", 0, 0, Status::Executed)]).is_complete());
    // Stopped mid-cell.
    assert!(!battery_rollup(vec![ok(
        "c",
        DATASETS_PER_SCENARIO - 1,
        DATASETS_PER_SCENARIO - 1,
        Status::Stopped
    )])
    .is_complete());
    // Failed generator path.
    assert!(!battery_rollup(vec![ok("d", 0, 0, Status::Failed("gen".into()))]).is_complete());
    // A generator failure count blocks admission even when executed.
    let mut g = good.clone();
    g.generator_failures = 1;
    g.first_failure = Some("boom".into());
    assert!(!battery_rollup(vec![g]).is_complete());
}

// The record shape is integer-canonical: schema/envelope/rollup +
// per-cell members — `Json::Int` only (S.2).
#[test]
fn record_is_integer_canonical() {
    let table = scenarios();
    let s = table
        .iter()
        .find(|s| s.id == "mech/zero-activation")
        .unwrap();
    let r = run_scenario(
        s,
        &Cap {
            max_datasets: 6,
            deadline: None,
        },
    );
    let rep = BatteryReport {
        schema: "sim-battery/1".into(),
        envelope: "test".into(),
        scenarios: vec![r],
        rollup: "incomplete:test".into(),
    };
    fn all_int_or_leaf(j: &Json) -> bool {
        match j {
            Json::Obj(m) => m.values().all(all_int_or_leaf),
            Json::Arr(a) => a.iter().all(all_int_or_leaf),
            _ => true,
        }
    }
    let j = rep.to_json();
    assert!(all_int_or_leaf(&j), "non-canonical member in {j:?}");
    let first = match j.get("scenarios") {
        Some(Json::Arr(a)) => a.first().cloned().unwrap_or(Json::Null),
        _ => Json::Null,
    };
    assert_eq!(
        first.get("coverage_label").and_then(Json::as_str),
        Some("diagnostic"),
        "coverage stays labelled diagnostic — never a nominal guarantee"
    );
}
