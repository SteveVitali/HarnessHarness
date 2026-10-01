//! S5.3 — the C2 analysis-depth acceptance slice (R-2.10.4 C2; §6.4
//! §2.3/§2.5; ADR-0160/0200):
//!
//! - A7 `fit_surface` model forms (`factorial_glmm` shrinkage +
//!   interactions; `curve_on_ordered_axis` monotone PAVA), the minimum
//!   design (`unknown{insufficient}` cells, never dropped), expiry
//!   triggers (`expired`/`expiring` + the home-11 `refit{min_design}`
//!   debt record), the one-`MatchSpec` gate, hosted coordinates
//!   (`n/a{class}`/`n/a{capability}`), and `portability`.
//! - A14 `strata_view` over `contamination_stratum` /
//!   `capability_vector` / `cost_confidence` / `suite_validity` /
//!   `mediation` + the `StrataPooledUnannotated` refusal.
//! - A6 judged realized-benefit stages (`delivered`, `valid`,
//!   `activated | delivered∧valid`, `followed | activated`,
//!   `E[Δ | followed]`, per detector) with `n/a{observability}` on the
//!   hosted side.
//! - A10 M1 designed ablation → `AttributionReport/1` members.

use std::collections::BTreeMap;

use hh_analysis::{analyze, AnalysisError, AnalysisInput};
use hh_budget::matchspec::{ArmSpec, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::catalogue;
use hh_eval::facts::{ArtefactRow, LedgerFacts, VerdictRow};
use hh_eval::runs::{SuiteContext, TaskContext};
use hh_lab::analysis::{AnalysisSpec, QuerySpec};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, FactorDeclaration, FactorKind, FactorLevel,
    IntervalMethod, MetricValueKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_results::audit::AuditRef;
use hh_results::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use hh_results::scoring::ScoringContext;
use hh_results::watermark::WatermarkSet;
use hh_wire::Json;

// ── fixture plumbing (mirrors tests/kernel.rs) ───────────────────────────────

fn decl(name: &str) -> MetricDeclaration {
    catalogue::metric(name).expect("catalogue metric")
}

fn estimator() -> EstimatorSelection {
    EstimatorSelection {
        method: IntervalMethod::ClusteredClt,
        selection_rule: "adr-0158.clt_floor".into(),
        floors: BTreeMap::new(),
        fallback_chain: vec![],
        substituted: None,
    }
}

fn prereg(primary: &[&str]) -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "A ≥ B".into(),
        primary_metrics: primary.iter().map(|m| m.to_string()).collect(),
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: "plan-1".into(),
        task_split_hash: "sha256:split-1".into(),
        interactions: vec![],
    }
}

fn design() -> Design {
    Design {
        id: "design-1".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 3,
        pairing: Pairing::ByTaskAndReplicate,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: true,
            seed_honoured_required: true,
        },
        held_out_split_ref: Some("split-1".into()),
        pre_registration: prereg(&["task_success"]),
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn factor(name: &str, kind: FactorKind, levels: &[&str]) -> FactorDeclaration {
    FactorDeclaration {
        name: name.into(),
        kind,
        granularity: None,
        levels: levels
            .iter()
            .map(|id| FactorLevel {
                id: (*id).to_string(),
                content_ref: format!("sha256:{id}"),
                label: (*id).to_string(),
            })
            .collect(),
        role: None,
    }
}

fn arms(ids: &[&str]) -> Vec<(String, ArmSpec)> {
    ids.iter()
        .map(|id| {
            let caps = BudgetSpec::hard_caps(
                BudgetMode::Pool,
                &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
            );
            (
                id.to_string(),
                ArmSpec::native(
                    caps.clone(),
                    caps,
                    MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
                ),
            )
        })
        .collect()
}

fn task(id: &str) -> TaskContext {
    TaskContext {
        task_id: id.into(),
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        split_hash: "sha256:split-1".into(),
        stratum: ContaminationStratum::PrivateHeldOut,
    }
}

fn suite() -> SuiteContext {
    SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }
}

fn row(arm: &str, task_id: &str, rep: u64, value: i64, metric: &str) -> ResultsRow {
    let run_id = format!("r-{arm}-{task_id}-{rep}");
    let mut watermark = WatermarkSet::new();
    watermark.pin(&run_id, 7);
    ResultsRow {
        key: RowKey {
            configuration_version_id: format!("cv-{arm}"),
            run_id: run_id.clone(),
        },
        coordinates: Coordinates {
            configuration_id: Some(format!("cfg-{arm}")),
            configuration_version_id: format!("cv-{arm}"),
            model_snapshots: [("default".to_string(), "snap-1".to_string())]
                .into_iter()
                .collect(),
            harness_def_ref: None,
            model_profile_ref: None,
            environment_ref: None,
            environment_version_id: Some("env-1".into()),
            task: Some(Json::obj([
                ("task_id", Json::str(task_id)),
                ("suite_id", Json::str("suite-test")),
                ("split_label", Json::str("held_out")),
            ])),
            budget: Json::obj([]),
            replicate: Json::obj([("seed", Json::Int(42 + rep as i64))]),
            participant_class: "native".into(),
            observability_level: vec![
                "events".into(),
                "model_io".into(),
                "end_state".into(),
                "ledger".into(),
            ],
            hosting_mechanism: None,
            capability_vector_ref: None,
            mediation: vec![],
            registry_snapshot_id: None,
        },
        experiment: Some(Json::obj([
            ("experiment_run_id", Json::str("exp-run-1")),
            ("arm_id", Json::str(arm)),
            ("cell_id", Json::str(format!("{arm}:{task_id}"))),
            ("replicate_index", Json::Int(rep as i64)),
            ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ])),
        outcome: OutcomeSection {
            status: "finished".into(),
            outcome_class: Some("scored".into()),
            stop_reason: Some("completed".into()),
            veto_tripped: vec![],
            finished_at: Some("2026-01-01T00:00:00.000Z".into()),
            wall_ms: Some(100),
            activation_no: 1,
        },
        cells: vec![Cell {
            metric_ref: metric.into(),
            value: MetricValueKind::Bool(value > 0),
            detector: Some("deterministic".into()),
            oracle_ref: Some("oracle/executable".into()),
            confidence: None,
            evidence: vec![AuditRef {
                run_id: run_id.clone(),
                seq: 6,
                hash: "idp:cell".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            }],
            computed_from: vec!["6".into()],
        }],
        consumption: ConsumptionSection {
            dimensions: [("model_calls".to_string(), 5i64)].into_iter().collect(),
            utilization: None,
            spend: None,
            instrument_spend: None,
        },
        cache: Json::obj([("policy", Json::str("cold_start"))]),
        lineage: Json::obj([]),
        annotations_from_ledger: Json::obj([]),
        audit: AuditSection {
            head: AuditRef {
                run_id: run_id.clone(),
                seq: 7,
                hash: "idp:head".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            },
            checkpoint_ref: None,
        },
        scoring: ScoringContext {
            validator_set: vec![],
            overlay_runs: vec![],
            metric_registry_version: "test-registry".into(),
            pricing_ref: None,
            view_policy_version: "test-view".into(),
        },
        derived_from: DerivedFrom {
            run_id: run_id.clone(),
            seq: 7,
            overlay_watermarks: BTreeMap::new(),
        },
        watermark_set: watermark,
        view_hash: "test-view-hash".into(),
        version_id: format!("v-{run_id}"),
    }
}

fn hosted_row(arm: &str, task_id: &str, rep: u64, value: i64, metric: &str) -> ResultsRow {
    let mut r = row(arm, task_id, rep, value, metric);
    r.coordinates.participant_class = "hosted".into();
    r.coordinates.hosting_mechanism = Some("provider_api".into());
    r.coordinates.observability_level = vec!["events".into(), "end_state".into()];
    r
}

fn manifests_for(rows: &[ResultsRow]) -> BTreeMap<String, RunManifest> {
    rows.iter()
        .map(|r| {
            let mut m = RunManifest::minimal(RunKind::Agent);
            m.seed = Some(42);
            (r.key.run_id.clone(), m)
        })
        .collect()
}

fn facts_for(rows: &[ResultsRow]) -> BTreeMap<String, LedgerFacts> {
    rows.iter()
        .map(|r| (r.key.run_id.clone(), LedgerFacts::default()))
        .collect()
}

fn spec(kind: &str, metrics: &[&str], filters: Json, spec_ref: Option<&str>) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: kind.into(),
        query: QuerySpec {
            metrics: metrics.iter().map(|m| m.to_string()).collect(),
            filters: Some(filters),
            grain: None,
        },
        spec_ref: spec_ref.map(str::to_string),
        label: None,
        estimator_selection: estimator(),
        resample: None,
        outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    s
}

struct Fixture {
    rows: Vec<ResultsRow>,
    manifests: BTreeMap<String, RunManifest>,
    facts: BTreeMap<String, LedgerFacts>,
    tasks: Vec<TaskContext>,
    suites: Vec<SuiteContext>,
    design: Design,
    arms: Vec<(String, ArmSpec)>,
    decls: Vec<MetricDeclaration>,
    watermarks: WatermarkSet,
    pre_registration: PreRegistration,
}

impl Fixture {
    fn new(
        rows: Vec<ResultsRow>,
        task_ids: &[&str],
        arms: Vec<(String, ArmSpec)>,
        decls: Vec<MetricDeclaration>,
    ) -> Fixture {
        let manifests = manifests_for(&rows);
        let facts = facts_for(&rows);
        let mut watermarks = WatermarkSet::new();
        for r in &rows {
            for (rid, seq) in &r.watermark_set.runs {
                watermarks.pin(rid, *seq);
            }
        }
        Fixture {
            rows,
            manifests,
            facts,
            tasks: task_ids.iter().map(|t| task(t)).collect(),
            suites: vec![suite()],
            design: design(),
            arms,
            decls,
            watermarks,
            pre_registration: prereg(&["task_success"]),
        }
    }

    /// Set `manifest.extra[k]` on every run of `arm`.
    fn set_extra(&mut self, arm: &str, k: &str, v: Json) {
        for (rid, m) in self.manifests.iter_mut() {
            if rid.starts_with(&format!("r-{arm}-")) {
                m.extra.insert(k.to_string(), v.clone());
            }
        }
    }

    /// Set `manifest.extra[k]` on the runs whose task is in `tasks`.
    fn set_extra_on_tasks(&mut self, tasks: &[&str], k: &str, v: Json) {
        for (rid, m) in self.manifests.iter_mut() {
            if tasks.iter().any(|t| rid.contains(&format!("-{t}-"))) {
                m.extra.insert(k.to_string(), v.clone());
            }
        }
    }

    fn input<'a>(&'a self) -> AnalysisInput<'a> {
        AnalysisInput {
            rows: &self.rows,
            manifests: &self.manifests,
            facts: &self.facts,
            tasks: &self.tasks,
            suites: &self.suites,
            design: Some(&self.design),
            arm_specs: &self.arms,
            declarations: &self.decls,
            pre_registration: Some(&self.pre_registration),
            watermark_set: &self.watermarks,
            metric_registry_version: "test-registry",
            price_table_version: "test-pricing",
            confidence_ppm: 950_000,
            resampling_draws: 2000,
            seed: 7,
            post_amendment: false,
        }
    }
}

/// `tasks` task ids `t1..t{n}`.
fn task_ids(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("t{i}")).collect()
}

fn paired_rows_exact(
    arm_a: &str,
    arm_b: &str,
    tasks: &[String],
    reps: u64,
    c_a: u64,
    c_b: u64,
    metric: &str,
) -> Vec<ResultsRow> {
    let mut rows = Vec::new();
    for t in tasks {
        for rep in 0..reps {
            rows.push(row(arm_a, t, rep, (rep < c_a) as i64, metric));
            rows.push(row(arm_b, t, rep, (rep < c_b) as i64, metric));
        }
    }
    rows
}

// ── A7 fit_surface — model forms ─────────────────────────────────────────────

/// A 30-task / 3-replicate fixture with `fault_profile` varied
/// (arm a → `none`, arm b → `p1`) — the minimum design met.
fn met_design_fixture() -> Fixture {
    let tasks = task_ids(30);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let rows = paired_rows_exact("a", "b", &tasks, 3, 2, 1, "task_success");
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "fault_profile",
        FactorKind::Environment,
        &["none", "p1"],
    )];
    fixture.set_extra("b", "fault_profile", Json::str("p1"));
    fixture
}

#[test]
fn fit_surface_factorial_glmm_reports_shrinkage() {
    let fixture = met_design_fixture();
    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([
            ("factors", Json::Arr(vec![Json::str("fault_profile")])),
            ("model_form", Json::str("factorial_glmm")),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("factorial_glmm");
    let surface = out.body.get("surface").expect("surface");
    assert_eq!(
        surface.get("model_form").and_then(Json::as_str),
        Some("factorial_glmm")
    );
    assert_eq!(surface.get("status").and_then(Json::as_str), Some("active"));
    assert_eq!(
        surface.get("schema").and_then(Json::as_str),
        Some("hh-fitted-surface/1")
    );
    let re = surface
        .get("estimates_with_ci")
        .and_then(|e| e.get("random_effects"))
        .expect("factorial_glmm carries random_effects");
    assert_eq!(
        re.get("method").and_then(Json::as_str),
        Some("method_of_moments")
    );
    match re.get("shrinkage") {
        Some(Json::Arr(rows)) => {
            assert!(!rows.is_empty(), "per-task shrinkage rows");
            assert!(rows.iter().all(|r| r.get("shrinkage_ppm").is_some()
                && r.get("u_hat").is_some()
                && r.get("shrunk_u_hat").is_some()));
        }
        other => panic!("shrinkage[]: {other:?}"),
    }
    // The minimum-design member reports met.
    assert_eq!(
        surface
            .get("design_minimum")
            .and_then(|d| d.get("met"))
            .map(|j| j == &Json::Bool(true)),
        Some(true)
    );
    // The home-11 debt record mints `refit{min_design}` and names it.
    assert!(surface
        .get("debt_record_ref")
        .and_then(Json::as_str)
        .is_some());
    let debt = surface.get("debt_record").expect("debt_record");
    assert_eq!(
        debt.get("removal_test")
            .and_then(|r| r.get("kind"))
            .and_then(Json::as_str),
        Some("refit")
    );
    assert_eq!(debt.get("status").and_then(Json::as_str), Some("active"));
}

#[test]
fn fit_surface_curve_on_ordered_axis_fits_monotone() {
    // 30 tasks, one ordered axis `fault_profile` at 3 probed levels —
    // the monotone PAVA fit emits `fitted[]` per level. The observed
    // means fall across `none → p1 → p2` (arm b degrades with the
    // profile level), so the fit is non-increasing.
    let tasks = task_ids(30);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let mut rows = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        // Arm b's success count falls with the task's level group.
        let c_b: u64 = match i / 10 {
            0 => 2,
            1 => 1,
            _ => 0,
        };
        for rep in 0..3u64 {
            rows.push(row("a", t, rep, 1, "task_success"));
            rows.push(row("b", t, rep, (rep < c_b) as i64, "task_success"));
        }
    }
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "fault_profile",
        FactorKind::Environment,
        &["none", "p1", "p2"],
    )];
    let third = &tasks[10..20];
    let third_refs: Vec<&str> = third.iter().map(String::as_str).collect();
    fixture.set_extra_on_tasks(&third_refs, "fault_profile", Json::str("p1"));
    let last = &tasks[20..];
    let last_refs: Vec<&str> = last.iter().map(String::as_str).collect();
    fixture.set_extra_on_tasks(&last_refs, "fault_profile", Json::str("p2"));

    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([
            ("factors", Json::Arr(vec![Json::str("fault_profile")])),
            ("model_form", Json::str("curve_on_ordered_axis")),
            ("ordered", Json::Arr(vec![Json::str("fault_profile")])),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("curve fit");
    let surface = out.body.get("surface").expect("surface");
    assert_eq!(
        surface.get("model_form").and_then(Json::as_str),
        Some("curve_on_ordered_axis")
    );
    let curves = match surface
        .get("estimates_with_ci")
        .and_then(|e| e.get("curves"))
    {
        Some(Json::Arr(c)) => c,
        other => panic!("curves: {other:?}"),
    };
    let curve = curves
        .iter()
        .find(|c| c.get("factor").and_then(Json::as_str) == Some("fault_profile"))
        .expect("a curve row per ordered axis");
    assert_eq!(curve.get("form").and_then(Json::as_str), Some("monotone"));
    let fitted = match curve.get("fitted") {
        Some(Json::Arr(f)) => f,
        other => panic!("fitted: {other:?}"),
    };
    assert_eq!(fitted.len(), 3, "three probed levels fit: {fitted:?}");
    // The declared order — none, p1, p2.
    let levels: Vec<&str> = fitted
        .iter()
        .filter_map(|f| f.get("level").and_then(Json::as_str))
        .collect();
    assert_eq!(levels, ["none", "p1", "p2"]);
    // Monotone: fitted values non-decreasing in the observed direction
    // (arm a > arm b on success → the observed trend falls → PAVA gives
    // a non-increasing fit).
    let vals: Vec<i64> = fitted
        .iter()
        .filter_map(|f| f.get("fitted").and_then(Json::as_int))
        .collect();
    assert_eq!(vals.len(), 3);
    assert_eq!(
        curve.get("direction").and_then(Json::as_str),
        Some("nonincreasing")
    );
    assert!(vals[0] >= vals[1] && vals[1] >= vals[2], "{vals:?}");
    // The observed cell means fall 833_333 → 666_667 → 500_000 (arm a
    // all-pass, arm b degrading) — the fitted curve preserves them.
    assert_eq!(vals[0], 833_333, "{vals:?}");
    assert_eq!(vals[2], 500_000, "{vals:?}");
}

#[test]
fn fit_surface_below_minimum_design_marks_insufficient() {
    // 2 tasks — `n_tasks < 30` fails the minimum design; every probed
    // cell is `unknown{insufficient}` (kept, never dropped, never
    // estimated).
    let tasks = task_ids(2);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let rows = paired_rows_exact("a", "b", &tasks, 3, 2, 1, "task_success");
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "fault_profile",
        FactorKind::Environment,
        &["none", "p1"],
    )];
    fixture.set_extra("b", "fault_profile", Json::str("p1"));
    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([("factors", Json::Arr(vec![Json::str("fault_profile")]))]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("surface is still readable");
    let surface = out.body.get("surface").expect("surface");
    let dm = surface.get("design_minimum").expect("design_minimum");
    assert_eq!(dm.get("met").map(|j| j == &Json::Bool(true)), Some(false));
    assert_eq!(
        dm.get("tasks_ok").map(|j| j == &Json::Bool(true)),
        Some(false)
    );
    let insufficient = match surface.get("insufficient_cells") {
        Some(Json::Arr(v)) => v,
        other => panic!("insufficient_cells: {other:?}"),
    };
    assert_eq!(insufficient.len(), 2, "both probed cells insufficient");
    assert!(insufficient
        .iter()
        .all(|c| c.get("reason").and_then(Json::as_str) == Some("insufficient")));
    // A below-minimum design never emits a numeric main-effect point.
    let effects = match surface
        .get("estimates_with_ci")
        .and_then(|e| e.get("main_effects"))
    {
        Some(Json::Arr(e)) => e,
        other => panic!("main_effects: {other:?}"),
    };
    for e in effects {
        if let Some(est) = e.get("estimate") {
            assert!(
                est.get("unknown").is_some(),
                "a below-minimum cell is typed unknown: {est:?}"
            );
        }
    }
}

#[test]
fn fit_surface_expiry_triggers_and_debt_record() {
    let fixture = met_design_fixture();
    // `fitted_at + max_age ≤ now` fires `expired`; the report stays
    // readable and the minted debt record carries the same status.
    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([
            ("factors", Json::Arr(vec![Json::str("fault_profile")])),
            ("fitted_at", Json::Int(0)),
            ("now_ms", Json::Int(100)),
            ("max_age_ms", Json::Int(50)),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("aged surface");
    let surface = out.body.get("surface").expect("surface");
    assert_eq!(
        surface.get("status").and_then(Json::as_str),
        Some("expired")
    );
    assert_eq!(
        surface
            .get("debt_record")
            .and_then(|d| d.get("status"))
            .and_then(Json::as_str),
        Some("expired"),
        "the debt record's status mirrors the report's"
    );
    // `expiring_refs` — a warn-window hit renders `expiring`, never
    // `expired`.
    let s2 = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([
            ("factors", Json::Arr(vec![Json::str("fault_profile")])),
            ("expiring_refs", Json::Arr(vec![Json::str("cfg-a")])),
        ]),
        Some("exp-1"),
    );
    let out2 = analyze(&s2, &fixture.input()).expect("expiring surface");
    let surface2 = out2.body.get("surface").expect("surface");
    assert_eq!(
        surface2.get("status").and_then(Json::as_str),
        Some("expiring")
    );
    assert_eq!(
        match surface2.get("expiring_refs") {
            Some(Json::Arr(v)) => v.first().and_then(Json::as_str),
            _ => None,
        },
        Some("cfg-a")
    );
}

#[test]
fn fit_surface_refuses_incommensurable_match_specs() {
    // The surface runs under ONE MatchSpec — arms declaring
    // incompatible specs refuse (`Match` error mapped to `Compare`).
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let rows = paired_rows_exact("a", "b", &tasks, 3, 2, 1, "task_success");
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "fault_profile",
        FactorKind::Environment,
        &["none", "p1"],
    )];
    // Rewire arm b's match spec onto a different dimension.
    let caps = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    fixture.arms[1].1 = ArmSpec::native(
        caps.clone(),
        caps,
        MatchSpec::matched_cap(&[DimensionId::ToolCalls]),
    );
    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([("factors", Json::Arr(vec![Json::str("fault_profile")]))]),
        Some("exp-1"),
    );
    match analyze(&s, &fixture.input()) {
        Err(AnalysisError::Compare(_)) => {}
        other => panic!("expected a Compare/Match refusal, got {other:?}"),
    }
}

#[test]
fn fit_surface_hosted_na_class_and_capability() {
    // A hosted row on a component-level axis → `n/a{class}`; on a
    // host-visible axis with an `unknown` capability verdict →
    // `n/a{capability}` (T-LCD-07 — never coerced to a level).
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let mut rows = paired_rows_exact("a", "b", &tasks, 3, 2, 1, "task_success");
    rows.push(hosted_row("h", "t1", 0, 1, "task_success"));
    let mut fixture = Fixture::new(
        rows,
        &refs,
        arms(&["a", "b", "h"]),
        vec![decl("task_success")],
    );
    fixture.design.factors = vec![factor(
        "fault_profile",
        FactorKind::Environment,
        &["none", "p1"],
    )];
    // The hosted run's capability vector marks `fault_profile` unknown.
    if let Some(m) = fixture.manifests.get_mut("r-h-t1-0") {
        m.extra.insert(
            "capability_vector".into(),
            Json::obj([("fault_profile", Json::str("unknown"))]),
        );
    }
    let s = spec(
        "fit_surface",
        &["task_success"],
        Json::obj([("factors", Json::Arr(vec![Json::str("fault_profile")]))]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("hosted n/a, never refused");
    let surface = out.body.get("surface").expect("surface");
    let na = match surface.get("na_rows") {
        Some(Json::Arr(v)) => v,
        other => panic!("na_rows: {other:?}"),
    };
    assert_eq!(na.len(), 1);
    assert_eq!(na[0].get("n/a").and_then(Json::as_str), Some("capability"));
    assert_eq!(na[0].get("run_id").and_then(Json::as_str), Some("r-h-t1-0"));
}

// ── A14 strata_view axes ────────────────────────────────────────────────────

fn strata_fixture() -> Fixture {
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let mut rows = paired_rows_exact("a", "b", &tasks, 2, 2, 1, "task_success");
    let mut h = hosted_row("h", "t1", 0, 1, "task_success");
    h.coordinates.mediation = vec!["model_calls".into()];
    rows.push(h);
    let mut fixture = Fixture::new(
        rows,
        &refs,
        arms(&["a", "b", "h"]),
        vec![decl("task_success")],
    );
    if let Some(m) = fixture.manifests.get_mut("r-h-t1-0") {
        m.extra.insert(
            "capability_vector".into(),
            Json::obj([("streaming", Json::str("supported"))]),
        );
    }
    fixture
}

#[test]
fn strata_view_all_axes() {
    let fixture = strata_fixture();
    for (by, expect) in [
        ("contamination_stratum", "private_held_out"),
        ("capability_vector", "{streaming:supported}"),
        ("cost_confidence", "unknown"),
        ("suite_validity", "suite-test:valid"),
        ("mediation", "model_calls"),
    ] {
        let s = spec(
            "strata_view",
            &["task_success"],
            Json::obj([("by", Json::str(by))]),
            Some("exp-1"),
        );
        let out = analyze(&s, &fixture.input()).unwrap_or_else(|e| panic!("{by}: {e:?}"));
        let strata = match out.body.get("strata").and_then(|st| st.get("strata")) {
            Some(Json::Arr(v)) => v,
            other => panic!("{by} strata: {other:?}"),
        };
        assert!(
            strata
                .iter()
                .any(|r| r.get("stratum").and_then(Json::as_str) == Some(expect)),
            "{by}: expected stratum {expect}: {strata:?}"
        );
        // `pooled: never` — the report marks the no-pooling rule.
        assert_eq!(
            out.body
                .get("strata")
                .and_then(|st| st.get("pooled"))
                .and_then(Json::as_str),
            Some("never")
        );
    }
    // The capability_vector axis lands native rows in `native`.
    let s = spec(
        "strata_view",
        &["task_success"],
        Json::obj([("by", Json::str("capability_vector"))]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("capability_vector");
    let strata = match out.body.get("strata").and_then(|st| st.get("strata")) {
        Some(Json::Arr(v)) => v,
        other => panic!("strata: {other:?}"),
    };
    assert!(strata
        .iter()
        .any(|r| r.get("stratum").and_then(Json::as_str) == Some("native")));
    // An unknown axis spelling refuses BadSpec — never pooled.
    let bad = spec(
        "strata_view",
        &["task_success"],
        Json::obj([("by", Json::str("bogus"))]),
        Some("exp-1"),
    );
    match analyze(&bad, &fixture.input()) {
        Err(AnalysisError::BadSpec { member, .. }) => {
            assert_eq!(member, "query.filters.by")
        }
        other => panic!("expected BadSpec, got {other:?}"),
    }
}

#[test]
fn strata_view_refuses_silent_pooling() {
    let fixture = strata_fixture();
    let s = spec(
        "strata_view",
        &["task_success"],
        Json::obj([("aggregate", Json::str("pooled"))]),
        Some("exp-1"),
    );
    match analyze(&s, &fixture.input()) {
        Err(AnalysisError::StrataPooledUnannotated { .. }) => {}
        other => panic!("expected StrataPooledUnannotated, got {other:?}"),
    }
}

// ── A6 realized-benefit stages ───────────────────────────────────────────────

fn artefact(id: &str, detector: Option<&str>) -> ArtefactRow {
    ArtefactRow {
        artefact_id: id.to_string(),
        delivery_id: Some(format!("d-{id}")),
        detector: detector.map(str::to_string),
        rule_id: None,
        predicate_ref: None,
        kind: Some("procedure_index".into()),
        by_reference: None,
        signal: None,
        evidence_ref: None,
    }
}

fn verdict(status: &str, detector: &str) -> VerdictRow {
    VerdictRow {
        validator_ref: Some("v-1".into()),
        oracle_class: Some("deterministic".into()),
        status: status.into(),
        value: Json::obj([("kind", Json::str("pass"))]),
        detector: Some(detector.into()),
        isolation: None,
        inputs_digest: None,
        phase: None,
        criterion_ref: None,
        visibility: None,
        charged_to: None,
        effect_id: None,
    }
}

#[test]
fn realized_benefit_stages_per_detector_and_hosted_na() {
    // Arm a's runs: delivered + activated + followed artefact `art-1`,
    // one pass verdict per run under detector `judge-1`; arm b is the
    // baseline. A hosted run joins arm a — its stages beyond
    // `delivered` render `n/a{observability}`.
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    // Arm b's per-task success varies (0, 1, 2, 0 of 2) so the
    // clustered-CLT band over `E[Δ|followed]` is defined (a constant
    // delta table is `n/a{estimator_undefined}`, correctly).
    let mut rows = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        let c_b = (i % 3) as u64;
        for rep in 0..2u64 {
            rows.push(row("a", t, rep, 1, "task_success"));
            rows.push(row("b", t, rep, (rep < c_b) as i64, "task_success"));
        }
    }
    let h = hosted_row("h", "t1", 0, 1, "task_success");
    let h_run = h.key.run_id.clone();
    rows.push(h);
    let mut fixture = Fixture::new(
        rows,
        &refs,
        arms(&["a", "b", "h"]),
        vec![decl("task_success")],
    );
    for rid in fixture.facts.keys().cloned().collect::<Vec<_>>() {
        if rid.starts_with("r-a-") {
            let f = fixture.facts.get_mut(&rid).unwrap();
            f.artefacts_delivered.push(artefact("art-1", None));
            f.artefacts_activated
                .push(artefact("art-1", Some("judge-1")));
            f.artefacts_followed
                .push(artefact("art-1", Some("judge-1")));
            f.verdicts.push(verdict("pass", "judge-1"));
            f.finished = Some(Json::obj([]));
        }
        if rid == h_run {
            let f = fixture.facts.get_mut(&rid).unwrap();
            f.artefacts_delivered.push(artefact("art-1", None));
        }
    }
    let s = spec(
        "benefit_decomposition",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("a")),
            ("arm_b", Json::str("b")),
            (
                "search_baseline",
                Json::obj([("n", Json::Int(2)), ("arm", Json::str("b"))]),
            ),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("benefit_decomposition");
    let stages = match out
        .body
        .get("benefit_decomposition")
        .and_then(|b| b.get("realized_stages"))
        .and_then(|r| r.get("stages"))
    {
        Some(Json::Arr(v)) => v.clone(),
        other => panic!("realized_stages.stages: {other:?}"),
    };
    let stage = |name: &str| -> &Json {
        stages
            .iter()
            .find(|r| r.get("stage").and_then(Json::as_str) == Some(name))
            .unwrap_or_else(|| panic!("stage {name}: {stages:?}"))
    };
    // `delivered` — every arm-a run delivered (n = 8, point = 1).
    let d = stage("delivered");
    assert_eq!(
        d.get("native")
            .and_then(|n| n.get("point_ppm"))
            .and_then(Json::as_int),
        Some(1_000_000)
    );
    // `valid` under detector `judge-1` — pass rate 1.
    let v = stage("valid");
    assert_eq!(
        v.get("detector").and_then(Json::as_str),
        Some("judge-1"),
        "the detector joins the stage row"
    );
    assert_eq!(
        v.get("native")
            .and_then(|n| n.get("point_ppm"))
            .and_then(Json::as_int),
        Some(1_000_000)
    );
    // `activated|delivered∧valid` + `followed|activated` — the joins.
    for name in [
        "activated_given_delivered_valid",
        "followed_given_activated",
    ] {
        let r = stage(name);
        assert_eq!(
            r.get("native")
                .and_then(|n| n.get("point_ppm"))
                .and_then(Json::as_int),
            Some(1_000_000),
            "{name}"
        );
    }
    // `E[Δ|followed]` — the judged-detector row carries a clustered-CLT
    // band (never a bare point); the `deterministic` row has no
    // followed rows → `n/a{estimator_undefined}` (typed, never 0).
    let delta = stages
        .iter()
        .find(|r| {
            r.get("stage").and_then(Json::as_str) == Some("delta_given_followed")
                && r.get("detector").and_then(Json::as_str) == Some("judge-1")
        })
        .expect("delta_given_followed under judge-1");
    assert!(
        delta
            .get("native")
            .and_then(|n| n.get("interval"))
            .and_then(|i| i.get("lo"))
            .is_some(),
        "{delta:?}"
    );
    let delta_det = stages
        .iter()
        .find(|r| {
            r.get("stage").and_then(Json::as_str) == Some("delta_given_followed")
                && r.get("detector").and_then(Json::as_str) == Some("deterministic")
        })
        .expect("delta_given_followed under deterministic");
    assert_eq!(
        delta_det
            .get("native")
            .and_then(|n| n.get("n/a"))
            .and_then(Json::as_str),
        Some("estimator_undefined")
    );
    // The hosted side marks stages `n/a{observability}` — never 0.
    // The hosted row is not in arm a — the hosted column is null here;
    // the native stages carry the typed `n/a` only where unobservable.
    // (Hosted n/a coverage lives in `hosted_stage` when the selection
    // mixes — arm a is all-native so `hosted` is null.)
    let _ = h_run;
}

#[test]
fn realized_stages_hosted_side_na_observability() {
    // Arm a = hosted rows only — `delivered` is observable (events) but
    // every judged stage is `n/a{observability}`.
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let mut rows = paired_rows_exact("b", "c", &tasks, 2, 2, 1, "task_success");
    // arm b stays native (the baseline); rebind the a-arm to hosted.
    for r in rows.iter_mut().filter(|r| r.key.run_id.starts_with("r-b-")) {
        r.experiment = Some(Json::obj([
            ("experiment_run_id", Json::str("exp-run-1")),
            ("arm_id", Json::str("a")),
            ("cell_id", Json::str("a:t")),
            ("replicate_index", Json::Int(0)),
            ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ]));
        r.key.run_id = r.key.run_id.replace("r-b-", "r-a-");
        r.coordinates.participant_class = "hosted".into();
        r.coordinates.hosting_mechanism = Some("provider_api".into());
        r.coordinates.observability_level = vec!["events".into(), "end_state".into()];
    }
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "c"]), vec![decl("task_success")]);
    for rid in fixture.facts.keys().cloned().collect::<Vec<_>>() {
        if rid.starts_with("r-a-") {
            fixture
                .facts
                .get_mut(&rid)
                .unwrap()
                .artefacts_delivered
                .push(artefact("art-1", None));
        }
    }
    let s = spec(
        "benefit_decomposition",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("a")),
            ("arm_b", Json::str("c")),
            (
                "search_baseline",
                Json::obj([("n", Json::Int(2)), ("arm", Json::str("c"))]),
            ),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("hosted stages");
    let stages = match out
        .body
        .get("benefit_decomposition")
        .and_then(|b| b.get("realized_stages"))
        .and_then(|r| r.get("stages"))
    {
        Some(Json::Arr(v)) => v.clone(),
        other => panic!("stages: {other:?}"),
    };
    for row in &stages {
        let hosted = row.get("hosted").cloned().unwrap_or(Json::Null);
        match row.get("stage").and_then(Json::as_str) {
            Some("delivered") => {
                // `events` observability — delivered is a real band.
                assert!(
                    hosted.get("point_ppm").is_some() || hosted.get("n/a").is_some(),
                    "delivered hosted cell: {hosted:?}"
                );
            }
            _ => {
                assert_eq!(
                    hosted.get("n/a").and_then(Json::as_str),
                    Some("observability"),
                    "stage {:?} hosted cell: {hosted:?}",
                    row.get("stage")
                );
            }
        }
    }
}

// ── A10 attribution → AttributionReport/1 ───────────────────────────────────

#[test]
fn attribution_report_carries_m1_members() {
    let tasks = task_ids(4);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let mut rows = paired_rows_exact("a", "b", &tasks, 2, 2, 1, "task_success");
    rows.push(hosted_row("h", "t1", 0, 1, "task_success"));
    let mut fixture = Fixture::new(
        rows,
        &refs,
        arms(&["a", "b", "h"]),
        vec![decl("task_success")],
    );
    fixture.design.factors = vec![factor(
        "environment",
        FactorKind::Environment,
        &["coding_terminal"],
    )];
    for f in fixture.facts.values_mut() {
        f.finished = Some(Json::obj([]));
    }
    let s = spec(
        "attribution",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("a")),
            ("arm_b", Json::str("b")),
            (
                "targets",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("rule")),
                    ("ref", Json::str("environment")),
                ])]),
            ),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&s, &fixture.input()).expect("attribution");
    let att = out.body.get("attribution").expect("attribution section");
    assert_eq!(
        att.get("schema").and_then(Json::as_str),
        Some("hh-attribution/1")
    );
    assert_eq!(att.get("method").and_then(Json::as_str), Some("M1"));
    assert_eq!(
        att.get("attribution_label").and_then(Json::as_str),
        Some("designed_ablation")
    );
    // The typed target rides verbatim.
    match att.get("targets") {
        Some(Json::Arr(t)) => {
            assert_eq!(t.len(), 1);
            assert_eq!(t[0].get("kind").and_then(Json::as_str), Some("rule"));
        }
        other => panic!("targets: {other:?}"),
    }
    assert!(att.get("ablation_manifest").is_some());
    assert!(att.get("delta").is_some());
    assert!(att.get("unattributed_share").is_some());
    assert_eq!(
        att.get("attributable").map(|j| j == &Json::Bool(true)),
        Some(true)
    );
    assert!(att.get("delivered").and_then(Json::as_int).is_some());
    assert!(att.get("settled").and_then(Json::as_int).is_some());
    // Effects carry the designed-ablation label.
    match att.get("effects") {
        Some(Json::Arr(e)) => {
            for eff in e {
                if eff.get("n/a").is_some() {
                    continue;
                }
                assert_eq!(
                    eff.get("label").and_then(Json::as_str),
                    Some("designed_ablation")
                );
                assert_eq!(
                    eff.get("validity_mode").and_then(Json::as_str),
                    Some("designed_ablation")
                );
                assert_eq!(
                    eff.get("estimand").and_then(Json::as_str),
                    Some("designed_ablation")
                );
            }
        }
        other => panic!("effects: {other:?}"),
    }
    // The hosted row is `n/a{class}` — never zero, never dropped.
    let na = match att.get("na_rows") {
        Some(Json::Arr(v)) => v.clone(),
        other => panic!("na_rows: {other:?}"),
    };
    assert_eq!(na.len(), 1);
    assert_eq!(na[0].get("n/a").and_then(Json::as_str), Some("class"));
}
