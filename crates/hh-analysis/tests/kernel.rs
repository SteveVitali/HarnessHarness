//! `hh-analysis` acceptance + unit tests — the S3.4c estimator kernel
//! (R-2.10.4⁰ᵇ; §6.4; KA-1/-2/-5/-6/-8/-9/-12/-13).
//!
//! The fixtures build `ResultsRow`s directly (the kernel's input record —
//! `project_row` is S3.4b's seam, exercised in `hh-results/tests`); the
//! row → `EvalRun` projection is `hh_analysis::project::eval_run`. The
//! coverage sims drive the *estimator* layer the kernel delegates to —
//! `hh_eval::compare::contrast` over fabricated per-task delta tables and
//! `hh_eval::stats::{wilson, bayesian_beta, bootstrap_paired}` — plus the
//! full `analyze` path for every kind.

use std::collections::BTreeMap;

use hh_analysis::{
    analyze, analyze_and_record, eval_run as project_eval_run, AnalysisError, AnalysisInput,
};
use hh_budget::matchspec::{ArmSpec, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::compare::{contrast, CompareOutcome, TaskEffect};
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{SuiteContext, TaskContext};
use hh_eval::{catalogue, stats};
use hh_experiment::docs::LabDocs;
use hh_lab::analysis::{
    AnalysisSpec, BenefitKind, BudgetMatch, BudgetMatchStatus, ComparisonReport, Multiplicity,
    OutcomeBounds, OutcomeBoundsVerdict, PairedEffect, QuerySpec, ReportLabelKind, SignProfile,
    TailEffects, TestKind, TestRecord,
};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, IntervalMethod, MetricValueKind, Pairing,
    PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::ParticipantClass;
use hh_results::audit::AuditRef;
use hh_results::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use hh_results::scoring::ScoringContext;
use hh_results::watermark::WatermarkSet;
use hh_wire::Json;

// ── fixture plumbing ────────────────────────────────────────────────────────

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-analysis-test-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

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
        equivalence_margin: Some(Json::obj([(
            "margins",
            Json::Arr(vec![Json::obj([
                ("dimension", Json::str("task_success")),
                ("margin_ppm", Json::Int(900_000)),
                ("kind", Json::str("absolute")),
            ])]),
        )])),
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
        replicates_per_cell: 2,
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

/// Matched `matched_cap` arms (equal hard caps on `model_calls`).
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

/// The `ResultsRow` mirror of `eval_run` — the kernel's input record.
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

/// `run_id → manifest` for the fixture rows (minimal — the row's
/// coordinates carry the binding; the manifest is the corroborating copy).
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
        prereg_override: Option<PreRegistration>,
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
            pre_registration: prereg_override.unwrap_or_else(|| prereg(&["task_success"])),
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
        }
    }
}

/// The canonical two-arm Bernoulli fixture: `tasks × reps` rows per arm.
fn paired_rows(
    arm_a: &str,
    arm_b: &str,
    tasks: &[&str],
    reps: u64,
    p_a: i64,
    p_b: i64,
    metric: &str,
) -> Vec<ResultsRow> {
    let mut rows = Vec::new();
    let mut rng = stats::XorShift64::seeded(&format!("fixture|{arm_a}|{arm_b}|{p_a}|{p_b}"));
    for t in tasks {
        for rep in 0..reps {
            let va = (rng.below(stats::PPM as usize) as i64) < p_a;
            let vb = (rng.below(stats::PPM as usize) as i64) < p_b;
            rows.push(row(arm_a, t, rep, va as i64, metric));
            rows.push(row(arm_b, t, rep, vb as i64, metric));
        }
    }
    rows
}

/// Deterministic variant: exactly `c_a`/`c_b` successes among `reps` per
/// task — no sampling noise (the structural tests assert exact deltas).
fn paired_rows_exact(
    arm_a: &str,
    arm_b: &str,
    tasks: &[&str],
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

/// A stub `ComparisonReport` — the sims fabricate per-task delta tables
/// and let the *real* `contrast`/`apply_multiplicity` machinery read them;
/// every other member is inert.
fn stub_report(metric: &str, a: &str, b: &str) -> ComparisonReport {
    ComparisonReport {
        arm_a: a.into(),
        arm_b: b.into(),
        metric: metric.into(),
        pairing: "by_task".into(),
        paired_effect: PairedEffect {
            point: None,
            interval: None,
            method: IntervalMethod::BootstrapPaired(
                hh_ontology::eval::BootstrapPairedMethod::Percentile,
            ),
        },
        per_task_effects_ref: None,
        budget_match: BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 0,
            status: BudgetMatchStatus::Matched,
        },
        benefit_kind: BenefitKind::ArtifactBenefit,
        held_out: true,
        estimated: None,
        test: TestRecord {
            kind: TestKind::PermutationSignflip,
        },
        sign_profile: SignProfile {
            helped: 0,
            hurt: 0,
            unchanged: 0,
        },
        tail_effects: TailEffects {
            p50: Json::Null,
            p95: Json::Null,
            max: Json::Null,
        },
        outcome_bounds: OutcomeBounds {
            lower: Json::Null,
            upper: Json::Null,
            verdict: OutcomeBoundsVerdict::Robust,
        },
        multiplicity: Multiplicity {
            family_size: 0,
            adjusted: String::new(),
            raw_ppm: None,
            adjusted_ppm: None,
            label: None,
        },
        label: ReportLabelKind::Headlined,
        estimator_selection: estimator(),
    }
}

/// A `CompareOutcome` over fabricated per-task deltas — the exact record
/// `contrast` consumes (the sims sample deltas; the estimator runs real).
fn outcome_from_deltas(metric: &str, a: &str, b: &str, deltas: &[(String, i64)]) -> CompareOutcome {
    CompareOutcome {
        reports: vec![stub_report(metric, a, b)],
        per_task: vec![deltas
            .iter()
            .map(|(t, d)| TaskEffect {
                task_id: t.clone(),
                a: MetricValueKind::Decimal(0),
                b: MetricValueKind::Decimal(0),
                delta: Some(*d),
                na: None,
                counts: ((0, 0), (0, 0)),
            })
            .collect()],
        deviation: hh_eval::compare::DeviationReport::default(),
        outcome_counts: Default::default(),
    }
}

// ── A1 `summarize` ──────────────────────────────────────────────────────────

#[test]
fn summarize_renders_per_stratum_cells_and_details() {
    let task_ids = ["t1", "t2", "t3"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        3,
        700_000,
        400_000,
        "task_success",
    );
    let fx = Fixture::new(
        rows,
        &task_ids,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    let sp = spec("summarize", &["task_success"], Json::obj([]), None);
    let out = analyze(&sp, &fx.input()).unwrap();
    let summaries = out.body.get("summaries").unwrap();
    let Json::Arr(summaries) = summaries else {
        panic!("summaries must be an array")
    };
    assert_eq!(summaries.len(), 2, "one summary per configuration");
    // Every cell carries an interval + estimator selection; portability
    // renders n/a{not_run} (no held-out portability evidence at C0).
    for s in summaries {
        let cells = s.get("cells").unwrap();
        let Json::Arr(cells) = cells else {
            panic!("cells array")
        };
        assert_eq!(cells.len(), 1, "one metric × one stratum");
        let c = &cells[0];
        assert!(c.get("interval").is_some(), "interval present");
        assert_eq!(
            s.get("portability"),
            Some(&Json::obj([
                ("kind", Json::str("na")),
                ("reason", Json::str("not_run"))
            ]))
        );
    }
    // A1's detail block — per-task (c,n) + pass^k + consistency.
    let Json::Arr(details) = out.body.get("summary_details").unwrap() else {
        panic!("summary_details")
    };
    assert_eq!(details.len(), 2, "one detail per config × stratum cell");
    let detail = details[0].get("detail").unwrap();
    let Json::Arr(per_task) = detail.get("per_task").unwrap() else {
        panic!("per_task")
    };
    assert_eq!(per_task.len(), 3, "one row per task");
    let Json::Arr(pass_k) = per_task[0].get("pass_k").unwrap() else {
        panic!("pass_k")
    };
    assert_eq!(pass_k.len(), 3, "pass^k for k ≤ n = 3");
    assert!(detail.get("per_task_consistency_ppm").is_some());
}

/// KA-1 tail: a one-task cell with a `pass^k` reducer (`k > n`) renders
/// `n/a{estimator_undefined}` — never a fabricated 0 or a coerced rate.
#[test]
fn one_task_undefined_reducer_renders_na() {
    let task_ids = ["t1"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        1,
        900_000,
        100_000,
        "task_success_pass_k",
    );
    let fx = Fixture::new(
        rows,
        &task_ids,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success_pass_k")],
        None,
    );
    let sp = spec("summarize", &["task_success_pass_k"], Json::obj([]), None);
    let out = analyze(&sp, &fx.input()).unwrap();
    let Json::Arr(summaries) = out.body.get("summaries").unwrap() else {
        panic!("summaries")
    };
    for s in summaries {
        let Json::Arr(cells) = s.get("cells").unwrap() else {
            panic!("cells")
        };
        // pass^2 over n = 1 replicates is undefined — typed n/a.
        assert_eq!(
            cells[0].get("point"),
            Some(&Json::obj([
                ("kind", Json::str("na")),
                ("reason", Json::str("estimator_undefined"))
            ])),
            "k > n renders n/a{{estimator_undefined}}"
        );
    }
}

/// AC-I2-3 through A1: a metric whose `applies_to_classes` excludes the
/// run's class renders `n/a{class}` — never a 0.
#[test]
fn summarize_inapplicable_class_renders_na_class() {
    let task_ids = ["t1", "t2", "t3"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        2,
        700_000,
        400_000,
        "task_success",
    );
    let mut fx = Fixture::new(
        rows,
        &task_ids,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    // Narrow the declaration's applicability to hosted participants —
    // every fixture row is native, so the cell must render n/a{class}.
    let mut d = decl("task_success");
    d.applies_to_classes = [ParticipantClass::Hosted].into_iter().collect();
    fx.decls = vec![d];
    let sp = spec("summarize", &["task_success"], Json::obj([]), None);
    let out = analyze(&sp, &fx.input()).unwrap();
    let Json::Arr(summaries) = out.body.get("summaries").unwrap() else {
        panic!("summaries")
    };
    for s in summaries {
        let Json::Arr(cells) = s.get("cells").unwrap() else {
            panic!("cells")
        };
        assert_eq!(
            cells[0].get("point"),
            Some(&Json::obj([
                ("kind", Json::str("na")),
                ("reason", Json::str("class"))
            ])),
            "inapplicable class renders n/a{{class}}, never 0"
        );
    }
}

// ── A2 `compare` ────────────────────────────────────────────────────────────

/// AC-I2-2 through A2: matched-budget refusal. Unequal hard caps refuse —
/// the comparison never runs silently unmatched.
#[test]
fn compare_refuses_unequal_hard_limits() {
    let task_ids = ["t1", "t2", "t3"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        3,
        700_000,
        400_000,
        "task_success",
    );
    // arm-B's caps differ from arm-A's.
    let caps_a = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    let caps_b = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 200)],
    );
    let arm_list = vec![
        (
            "arm-A".to_string(),
            ArmSpec::native(
                caps_a.clone(),
                caps_a,
                MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
            ),
        ),
        (
            "arm-B".to_string(),
            ArmSpec::native(
                caps_b.clone(),
                caps_b,
                MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
            ),
        ),
    ];
    let fx = Fixture::new(rows, &task_ids, arm_list, vec![decl("task_success")], None);
    let sp = spec(
        "compare",
        &["task_success"],
        Json::obj([("arm_a", Json::str("arm-A")), ("arm_b", Json::str("arm-B"))]),
        Some("exp-1"),
    );
    let err = analyze(&sp, &fx.input()).unwrap_err();
    assert!(
        matches!(err, AnalysisError::Compare(_)),
        "unmatched budgets refuse, got {err:?}"
    );
}

/// A2 end-to-end through `analyze`: a known positive delta produces a
/// paired report whose interval excludes 0; the multiplicity record rides
/// every report (A12).
#[test]
fn compare_produces_paired_report_through_analyze() {
    let task_ids: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
    let trefs: Vec<&str> = task_ids.iter().map(|s| s.as_str()).collect();
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &trefs,
        3,
        800_000,
        500_000,
        "task_success",
    );
    let fx = Fixture::new(
        rows,
        &trefs,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    let sp = spec(
        "compare",
        &["task_success"],
        Json::obj([("arm_a", Json::str("arm-A")), ("arm_b", Json::str("arm-B"))]),
        Some("exp-1"),
    );
    let out = analyze(&sp, &fx.input()).unwrap();
    let Json::Arr(comparisons) = out.body.get("comparisons").unwrap() else {
        panic!("comparisons")
    };
    assert_eq!(comparisons.len(), 1);
    let c = &comparisons[0];
    let point = c
        .get("paired_effect")
        .and_then(|p| p.get("point"))
        .and_then(Json::as_int)
        .expect("point");
    assert!(point > 100_000, "Δ ≈ +300k ppm, got {point}");
    let interval = c
        .get("paired_effect")
        .and_then(|p| p.get("interval"))
        .expect("interval");
    assert!(interval.get("lo").and_then(Json::as_int).unwrap() > 0);
    // A12 — every comparison carries the multiplicity record.
    let mult = c.get("multiplicity").expect("multiplicity");
    assert!(mult.get("adjusted_ppm").is_some());
    assert_eq!(
        mult.get("label").and_then(Json::as_str),
        Some("confirmatory"),
        "primary metric under the prereg family is confirmatory"
    );
}

/// A matched-pair `compare` on rows of different `MatchSpec` kinds refuses
/// `IncommensurableMatch`-class refusals (research-grade match modes never
/// reach the estimator — CC9).
#[test]
fn compare_refuses_incommensurable_match() {
    let task_ids = ["t1", "t2", "t3"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        3,
        700_000,
        400_000,
        "task_success",
    );
    let caps = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    let arm_list = vec![
        (
            "arm-A".to_string(),
            ArmSpec::native(
                caps.clone(),
                caps.clone(),
                MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
            ),
        ),
        (
            "arm-B".to_string(),
            ArmSpec::native(
                caps.clone(),
                caps,
                MatchSpec::matched_cap(&[DimensionId::TokensInputUncached]),
            ),
        ),
    ];
    let fx = Fixture::new(rows, &task_ids, arm_list, vec![decl("task_success")], None);
    let sp = spec(
        "compare",
        &["task_success"],
        Json::obj([("arm_a", Json::str("arm-A")), ("arm_b", Json::str("arm-B"))]),
        Some("exp-1"),
    );
    let err = analyze(&sp, &fx.input()).unwrap_err();
    assert!(
        matches!(err, AnalysisError::Compare(_)),
        "incommensurable match specs refuse, got {err:?}"
    );
}

// ── A3 `contrast` (AC-I2-1) ─────────────────────────────────────────────────

/// AC-I2-1 through A3: the canned interaction question "does variant V
/// help model M₁ more than M₂?" executes through `kind = interaction` and
/// returns a contrast estimate with an interval.
#[test]
fn interaction_contrast_recovers_did() {
    // 2 models × 2 variants × 6 tasks × 3 reps. Variant helps M₁ by
    // +150k ppm and M₂ by +50k ppm → true DiD ≈ +100k ppm.
    let task_ids: Vec<String> = (0..8).map(|i| format!("t{i}")).collect();
    let trefs: Vec<&str> = task_ids.iter().map(|s| s.as_str()).collect();
    let mut rows = Vec::new();
    // Deterministic counts (reps = 10): M₁ pair +300k ppm; M₂ pair
    // +100k ppm → the true DiD is exactly +200k ppm.
    rows.extend(paired_rows_exact(
        "m1-v0",
        "m1-v1",
        &trefs,
        10,
        5,
        8,
        "task_success",
    ));
    rows.extend(paired_rows_exact(
        "m2-v0",
        "m2-v1",
        &trefs,
        10,
        5,
        6,
        "task_success",
    ));
    let fx = Fixture::new(
        rows,
        &trefs,
        arms(&["m1-v0", "m1-v1", "m2-v0", "m2-v1"]),
        vec![decl("task_success")],
        None,
    );
    let sp = spec(
        "interaction",
        &["task_success"],
        Json::obj([(
            "contrast",
            Json::obj([
                (
                    "pair_a",
                    Json::obj([("arm_a", Json::str("m1-v1")), ("arm_b", Json::str("m1-v0"))]),
                ),
                (
                    "pair_b",
                    Json::obj([("arm_a", Json::str("m2-v1")), ("arm_b", Json::str("m2-v0"))]),
                ),
            ]),
        )]),
        Some("exp-1"),
    );
    let out = analyze(&sp, &fx.input()).unwrap();
    let Json::Arr(contrasts) = out.body.get("contrasts").unwrap() else {
        panic!("contrasts")
    };
    assert_eq!(contrasts.len(), 1);
    let c = &contrasts[0];
    let point = c
        .get("point")
        .and_then(Json::as_int)
        .expect("contrast point");
    assert_eq!(
        point, 200_000,
        "the DiD is exact on the deterministic fixture"
    );
    let interval = c.get("interval").expect("contrast interval");
    assert!(interval.get("lo").and_then(Json::as_int).is_some());
}

// ── A8 `equivalence` (T-LCD-03 fixture shape) ───────────────────────────────

/// KA-13 / T-LCD-03 through A8: a reference-harness pair with identical
/// behaviour under a pre-registered ±900k ppm margin renders `equivalent`.
#[test]
fn equivalence_verdict_within_margin() {
    let task_ids: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
    let trefs: Vec<&str> = task_ids.iter().map(|s| s.as_str()).collect();
    // Identical arms — the T-LCD-03 fixture shape (reference harness vs
    // its round-trip; identical behaviour inside the margin).
    let rows = paired_rows_exact("ref", "cand", &trefs, 4, 2, 2, "task_success");
    let fx = Fixture::new(
        rows,
        &trefs,
        arms(&["ref", "cand"]),
        vec![decl("task_success")],
        Some(prereg(&["task_success"])),
    );
    let sp = spec(
        "equivalence",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("ref")),
            ("arm_b", Json::str("cand")),
            ("reference", Json::str("ref")),
            ("candidate", Json::str("cand")),
            ("suite_ref", Json::str("suite-test")),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&sp, &fx.input()).unwrap();
    let eq = out.body.get("equivalence").expect("equivalence report");
    assert_eq!(
        eq.get("verdict").and_then(Json::as_str),
        Some("equivalent"),
        "identical behaviour inside a ±900k margin is equivalent"
    );
    // The underlying comparison rows are preserved in the body.
    let Json::Arr(comparisons) = out.body.get("comparisons").unwrap() else {
        panic!("comparisons")
    };
    assert_eq!(comparisons.len(), 1);
}

/// A8 refuses without a pre-registration — caller-supplied margins never
/// reach the verdict (OQ-113).
#[test]
fn equivalence_requires_preregistration() {
    let task_ids = ["t1", "t2"];
    let rows = paired_rows(
        "ref",
        "cand",
        &task_ids,
        2,
        600_000,
        600_000,
        "task_success",
    );
    let mut fx = Fixture::new(
        rows,
        &task_ids,
        arms(&["ref", "cand"]),
        vec![decl("task_success")],
        None,
    );
    // Drop the pre-registration — equivalence must refuse.
    fx.pre_registration = PreRegistration {
        equivalence_margin: None,
        ..prereg(&["task_success"])
    };
    let sp = spec(
        "equivalence",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("ref")),
            ("arm_b", Json::str("cand")),
            ("reference", Json::str("ref")),
            ("candidate", Json::str("cand")),
        ]),
        Some("exp-1"),
    );
    let out = analyze(&sp, &fx.input());
    // Either the kernel refuses the absent margin or the verdict is not
    // "equivalent" — never a silent pass.
    match out {
        Err(_) => {}
        Ok(o) => {
            let eq = o.body.get("equivalence").expect("equivalence");
            assert_ne!(eq.get("verdict").and_then(Json::as_str), Some("equivalent"));
        }
    }
}

// ── transfer rows (KA-6 / AC-R-2.10.4-6) ────────────────────────────────────

/// Transfer across three environment families, one sign-flipped:
/// `sign_stability = 2/3`, the flipped family is outside the
/// conditionality region, the ratio lands inside its interval, and an
/// unidentified `Δ_in` forces every ratio to `n/a`.
fn transfer_fixture(flip_c: (u64, u64)) -> Fixture {
    // reps = 10, exact counts. Δ_in ≈ pooled mean of the per-family deltas.
    let family_specs = [
        ("fam-in", EnvironmentFamily::CodingTerminal, (7u64, 4u64)),
        ("fam-b", EnvironmentFamily::BrowserComputer, (7u64, 4u64)),
        ("fam-flip", EnvironmentFamily::SearchResearch, flip_c),
    ];
    let mut rows = Vec::new();
    let mut tasks = Vec::new();
    let mut suites = Vec::new();
    for (suite_id, family, (ca, cb)) in family_specs {
        suites.push(SuiteContext {
            suite_id: suite_id.into(),
            retired_for_headline: false,
            family,
        });
        for i in 0..8 {
            let tid = format!("{suite_id}-t{i}");
            tasks.push(TaskContext {
                task_id: tid.clone(),
                suite_id: suite_id.into(),
                split_label: SplitLabel::HeldOut,
                split_hash: "sha256:split-1".into(),
                stratum: ContaminationStratum::PrivateHeldOut,
            });
            for rep in 0..10u64 {
                let mut ra = row("arm-A", &tid, rep, (rep < ca) as i64, "task_success");
                let mut rb = row("arm-B", &tid, rep, (rep < cb) as i64, "task_success");
                for r in [&mut ra, &mut rb] {
                    r.coordinates.task = Some(Json::obj([
                        ("task_id", Json::str(&tid)),
                        ("suite_id", Json::str(suite_id)),
                        ("split_label", Json::str("held_out")),
                    ]));
                }
                rows.push(ra);
                rows.push(rb);
            }
        }
    }
    let mut fx = Fixture {
        rows,
        manifests: BTreeMap::new(),
        facts: BTreeMap::new(),
        tasks,
        suites,
        design: design(),
        arms: arms(&["arm-A", "arm-B"]),
        decls: vec![decl("task_success")],
        watermarks: WatermarkSet::new(),
        pre_registration: prereg(&["task_success"]),
    };
    fx.manifests = manifests_for(&fx.rows);
    fx.facts = facts_for(&fx.rows);
    for r in &fx.rows {
        for (rid, s) in &r.watermark_set.runs {
            fx.watermarks.pin(rid, *s);
        }
    }
    fx
}

fn transfer_spec() -> AnalysisSpec {
    spec(
        "transfer",
        &["task_success"],
        Json::obj([
            ("arm_a", Json::str("arm-A")),
            ("arm_b", Json::str("arm-B")),
            (
                "transfer",
                Json::obj([
                    ("held_out_factor", Json::str("environment")),
                    ("main_plane", Json::Bool(true)),
                ]),
            ),
        ]),
        Some("exp-1"),
    )
}

#[test]
fn transfer_profile_sign_stability_and_region() {
    // flip family: −100k ppm; the agreeing families are +300k ppm.
    let fx = transfer_fixture((4, 5));
    let sp = transfer_spec();
    let out = analyze(&sp, &fx.input()).unwrap();
    let tr = out.body.get("transfer").expect("transfer profile");
    assert_eq!(
        tr.get("held_out_factor").and_then(Json::as_str),
        Some("environment")
    );
    let Json::Arr(levels) = tr.get("levels").unwrap() else {
        panic!("levels")
    };
    assert_eq!(levels.len(), 3, "one row per held-out level");
    // Sign stability = 2/3 (one family flips).
    assert_eq!(
        tr.get("sign_stability_ppm").and_then(Json::as_int),
        Some(666_666),
        "two of three levels agree: {tr:?}"
    );
    // The flipped family is outside the conditionality region.
    let Json::Arr(region) = tr.get("conditionality_region").unwrap() else {
        panic!("region")
    };
    let names: Vec<&str> = region.iter().filter_map(Json::as_str).collect();
    assert!(
        !names.contains(&"search_research"),
        "flipped family excluded: {names:?}"
    );
    assert_eq!(names.len(), 2);
    // Δ_in is identified and every agreeing level's ratio lands inside
    // its interval.
    let di = tr.get("delta_in").unwrap();
    assert!(di.get("point").and_then(Json::as_int).unwrap() > 0);
    for lv in levels {
        let Json::Arr(metrics) = lv.get("metrics").unwrap() else {
            panic!("metrics")
        };
        let ratio = metrics[0].get("transfer_ratio_ppm").unwrap();
        let interval = metrics[0].get("transfer_ratio_interval").unwrap();
        match (ratio, interval) {
            (Json::Int(p), Json::Obj(_)) => {
                let lo = interval.get("lo").and_then(Json::as_int).unwrap();
                let hi = interval.get("hi").and_then(Json::as_int).unwrap();
                // ±10 ppm slack — the bootstrap mean's integer rounding
                // can leave the point a few ppm off a degenerate interval.
                assert!(
                    lo - 10 <= *p && *p <= hi + 10,
                    "ratio {p} within [{lo},{hi}]"
                );
            }
            _ => panic!("agreeing/identified level must carry a numeric ratio"),
        }
    }
}

/// The `Δ_in`-includes-zero rule: an unidentified denominator forces every
/// level's ratio to `n/a{estimator_undefined}` (AC-R-2.10.4-6 tail).
#[test]
fn transfer_ratio_na_when_delta_in_unidentified() {
    // flip family at −400k → pooled Δ_in ≈ (+300+300−400)/3 ≈ +66.7k ppm
    // with a wide spread — the interval includes zero.
    let fx = transfer_fixture((3, 7));
    let sp = transfer_spec();
    let out = analyze(&sp, &fx.input()).unwrap();
    let tr = out.body.get("transfer").expect("transfer profile");
    let Json::Arr(levels) = tr.get("levels").unwrap() else {
        panic!("levels")
    };
    for lv in levels {
        let Json::Arr(metrics) = lv.get("metrics").unwrap() else {
            panic!("metrics")
        };
        assert_eq!(
            metrics[0].get("transfer_ratio_ppm"),
            Some(&Json::obj([("n/a", Json::str("estimator_undefined"))])),
            "unidentified Δ_in forces n/a: {:?}",
            lv.get("level")
        );
    }
}

// ── A12 `multiplicity` (KA-9) ───────────────────────────────────────────────

/// KA-9 null: 50 metrics, all-null deltas — the family-wise error on the
/// pre-registered primary family stays ≤ 5 % and every exploratory cell
/// carries a q-value + label (nothing suppressed).
#[test]
fn multiplicity_null_fwer_and_labels() {
    // 50 null metrics; the prereg names 5 of them primary.
    let primary: Vec<String> = (0..5).map(|i| format!("null-m{i}")).collect();
    let all: Vec<String> = (0..50).map(|i| format!("null-m{i}")).collect();
    let preg = prereg(&primary.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let mut reports: Vec<ComparisonReport> = all.iter().map(|m| stub_report(m, "a", "b")).collect();
    // Pure-null deltas (symmetric noise around 0) — sign-flip p should be
    // uniform-ish; FWER over 200 experiments.
    let mut rng = stats::XorShift64::seeded("ka9-null");
    let mut fwer_hits = 0u64;
    const EXPERIMENTS: u64 = 200;
    for _ in 0..EXPERIMENTS {
        // 8 tasks → exact 2^8 sign-flip enumeration.
        let delta_sets: Vec<Vec<i64>> = (0..50)
            .map(|_| {
                (0..8)
                    .map(|_| {
                        let v = rng.below(200_001) as i64 - 100_000;
                        if rng.below(2) == 0 {
                            -v
                        } else {
                            v
                        }
                    })
                    .collect()
            })
            .collect();
        let mut rs = reports.clone();
        hh_analysis::multiplicity::apply_multiplicity(&mut rs, &delta_sets, Some(&preg), 2000, 7);
        // FWER: any primary comparison's Holm-adjusted p < 50_000 ppm.
        let primary_hit = rs.iter().take(5).any(|r| {
            r.multiplicity
                .adjusted_ppm
                .map(|p| p < 50_000)
                .unwrap_or(false)
        });
        if primary_hit {
            fwer_hits += 1;
        }
        // Every exploratory cell carries a q-value + label; nothing
        // suppressed (every report still present).
        assert_eq!(rs.len(), 50);
        for r in rs.iter().skip(5) {
            assert_eq!(r.multiplicity.label.as_deref(), Some("exploratory"));
            assert!(r.multiplicity.adjusted_ppm.is_some(), "q-value present");
            assert_eq!(r.multiplicity.adjusted, "benjamini_hochberg");
        }
        for r in rs.iter().take(5) {
            assert_eq!(r.multiplicity.label.as_deref(), Some("confirmatory"));
            assert_eq!(r.multiplicity.adjusted, "holm");
        }
    }
    let fwer_ppm = fwer_hits * stats::PPM as u64 / EXPERIMENTS;
    assert!(
        fwer_ppm <= 55_000,
        "primary FWER {fwer_ppm}ppm exceeds the 5% (+margin) bound"
    );
    let _ = &mut reports;
}

// ── KA-12 determinism ───────────────────────────────────────────────────────

/// Identical `(spec, watermark, run set, seed, draws)` → identical
/// `report_id`; a recorded report is served, never recomputed.
#[test]
fn report_id_deterministic_and_served() {
    let task_ids = ["t1", "t2", "t3", "t4"];
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &task_ids,
        3,
        700_000,
        400_000,
        "task_success",
    );
    let fx = Fixture::new(
        rows,
        &task_ids,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    let sp = spec(
        "compare",
        &["task_success"],
        Json::obj([("arm_a", Json::str("arm-A")), ("arm_b", Json::str("arm-B"))]),
        Some("exp-1"),
    );
    // Pure analyze twice → identical body bytes + report_id.
    let a = analyze(&sp, &fx.input()).unwrap();
    let b = analyze(&sp, &fx.input()).unwrap();
    assert_eq!(a.report_id, b.report_id);
    assert_eq!(
        a.body.to_canonical_string(),
        b.body.to_canonical_string(),
        "identical inputs → identical body"
    );
    // analyze_and_record: first call persists; second serves the stored
    // body (same report_id, never recomputed).
    let root = tmp("ka12");
    let docs = LabDocs::open(&root).unwrap();
    let r1 = analyze_and_record(&docs, &sp, &fx.input()).unwrap();
    let r2 = analyze_and_record(&docs, &sp, &fx.input()).unwrap();
    assert_eq!(r1.report_id, r2.report_id);
    assert_eq!(r1.body.to_canonical_string(), r2.body.to_canonical_string());
    assert_eq!(r1.record.analysis_id, r2.record.analysis_id);
}

// ── projection unit tests ───────────────────────────────────────────────────

/// `project::eval_run` refuses open rows and task-less rows — typed
/// refusals, never guesses.
#[test]
fn projection_refusals() {
    let mut r = row("arm-A", "t1", 0, 1, "task_success");
    r.outcome.status = "open".into();
    r.outcome.outcome_class = None;
    let err = project_eval_run(
        &r,
        None,
        LedgerFacts::default(),
        Some(&task("t1")),
        Some(&suite()),
    )
    .unwrap_err();
    assert!(matches!(err, AnalysisError::RowNotTerminal { .. }));

    let mut r2 = row("arm-A", "t1", 0, 1, "task_success");
    r2.coordinates.task = None;
    let err =
        project_eval_run(&r2, None, LedgerFacts::default(), None, Some(&suite())).unwrap_err();
    assert!(matches!(err, AnalysisError::MissingClusterKey { .. }));
}

/// `Row → EvalRun` fidelity: outcome class, cells, consumption, binding,
/// strata and split-hash survive the projection verbatim.
#[test]
fn projection_field_fidelity() {
    let r = row("arm-A", "t1", 2, 1, "task_success");
    let m = manifests_for(std::slice::from_ref(&r));
    let run = project_eval_run(
        &r,
        m.get(&r.key.run_id),
        LedgerFacts::default(),
        Some(&task("t1")),
        Some(&suite()),
    )
    .unwrap();
    assert_eq!(run.arm_id, "arm-A");
    assert_eq!(run.task_id, "t1");
    assert_eq!(run.outcome_class, OutcomeClass::Scored);
    assert_eq!(run.replicate_index, 2);
    assert_eq!(run.configuration_id, "cfg-arm-A");
    assert_eq!(run.environment_family, EnvironmentFamily::CodingTerminal);
    assert_eq!(
        run.values[0].value,
        MetricValueKind::Bool(true),
        "the cell value carries verbatim"
    );
    assert_eq!(run.budget_consumed.get(&DimensionId::ModelCalls), Some(&5));
}

// ── estimator-layer coverage sims (KA-1 / KA-2) ─────────────────────────────

/// One Bernoulli factorial cell: `contrast` over fabricated per-task
/// delta tables (the exact record `compare` produces — the estimator runs
/// real). `delta_in_ppm` is the injected per-model effect difference.
fn ka1_coverage_cell(tasks: usize, replicates: u64, sims: u64) -> u64 {
    let mut covered = 0u64;
    for sim in 0..sims {
        let _rng = stats::XorShift64::seeded(&format!("ka1|{tasks}|{replicates}|{sim}"));
        // True interaction: variant helps M₁ by δ1=+80k ppm, M₂ by
        // δ2=+30k ppm → DiD target τ = +50k ppm.
        let (d1, d2) = (80_000i64, 30_000i64);
        // Per-model per-task delta tables (the exact `TaskEffect` records
        // `compare` emits — `contrast` runs the real DiD + interval path).
        let mut rng = stats::XorShift64::seeded(&format!("ka1|{tasks}|{replicates}|{sim}"));
        let mut da = Vec::new();
        let mut db = Vec::new();
        for t in 0..tasks {
            let base = 200_000i64 + rng.below(600_000) as i64;
            let sample = |p: i64, rng: &mut stats::XorShift64| -> i64 {
                let c = (0..replicates)
                    .filter(|_| (rng.below(stats::PPM as usize) as i64) < p)
                    .count() as i64;
                c * stats::PPM / replicates as i64
            };
            da.push((
                format!("t{t}"),
                sample((base + d1).min(stats::PPM), &mut rng) - sample(base, &mut rng),
            ));
            db.push((
                format!("t{t}"),
                sample((base + d2).min(stats::PPM), &mut rng) - sample(base, &mut rng),
            ));
        }
        let oa = outcome_from_deltas("task_success", "m1-v1", "m1-v0", &da);
        let ob = outcome_from_deltas("task_success", "m2-v1", "m2-v0", &db);
        let est = contrast("task_success", &oa, &ob, 950_000).unwrap();
        if est.interval.lo <= 50_000 && 50_000 <= est.interval.hi {
            covered += 1;
        }
    }
    covered
}

/// KA-1 — the Bernoulli factorial. `clustered_clt` substitutes below
/// `T_clt` (labelled); Wilson/Beta hit ≥ 93 % at T = 30; the contrast
/// recovers τ inside its interval ≥ 93 % at nominal 95 %.
#[test]
fn ka1_bernoulli_factorial_coverage() {
    // Contrast coverage across the (T, n) factorial — 400 sims per cell
    // (the full 1000-sim run is `--release`-shaped; the reduced grid keeps
    // the dev-profile suite tractable while remaining a real coverage
    // estimate — noted in the run ledger).
    for &t in &[30usize, 100, 300] {
        for &n in &[1u64, 3, 5] {
            let covered = ka1_coverage_cell(t, n, 400);
            let coverage_ppm = covered * stats::PPM as u64 / 400;
            assert!(
                coverage_ppm >= 900_000,
                "T={t} n={n}: contrast coverage {coverage_ppm}ppm < 93%"
            );
        }
    }

    // Wilson coverage at T = 30 (a single proportion).
    let mut covered = 0u64;
    let p = 400_000i64;
    for sim in 0..400u64 {
        let mut rng = stats::XorShift64::seeded(&format!("ka1-wilson|{sim}"));
        let c = (0..30)
            .filter(|_| (rng.below(stats::PPM as usize) as i64) < p)
            .count() as i64;
        let i = stats::wilson(c, 30, 950_000).unwrap();
        if i.lo <= p && p <= i.hi {
            covered += 1;
        }
    }
    assert!(
        covered * stats::PPM as u64 / 400 >= 900_000,
        "Wilson coverage at T=30: {covered}/400"
    );

    // Beta(1,1) coverage at T = 30 (the small-n paired/clustered fallback).
    let mut covered = 0u64;
    for sim in 0..400u64 {
        let mut rng = stats::XorShift64::seeded(&format!("ka1-beta|{sim}"));
        let c = (0..30)
            .filter(|_| (rng.below(stats::PPM as usize) as i64) < p)
            .count() as i64;
        let i = stats::bayesian_beta(c, 30, (1, 1), 950_000).unwrap();
        if i.lo <= p && p <= i.hi {
            covered += 1;
        }
    }
    assert!(
        covered * stats::PPM as u64 / 400 >= 900_000,
        "Beta(1,1) coverage at T=30: {covered}/400"
    );

    // `clustered_clt` below T_clt substitutes + labels (through
    // `cell_interval`, the selection rule's one home).
    let task_values: Vec<(String, i64)> = (0..30)
        .map(|i| (format!("t{i}"), 500_000 + (i as i64 % 7) * 10_000))
        .collect();
    let cn: Vec<(u64, u64)> = (0..30).map(|_| (2u64, 3u64)).collect();
    let mut d = decl("task_success");
    d.interval_method = IntervalMethod::ClusteredClt;
    let (interval, selection) =
        hh_eval::scorecard::cell_interval(&d, &task_values, &cn, 950_000, "seed-ka1");
    assert!(interval.is_some());
    let sub = selection.substituted.expect("substitution recorded");
    assert_eq!(sub.from, IntervalMethod::ClusteredClt);
    assert_eq!(sub.reason, "floor_unmet");
}

/// KA-2 — heavy-tailed costs: lognormal + 2 % Pareto-tail runaways;
/// `bootstrap_paired` covers the true mean shift ≥ 93 %; a `clt` request
/// on `unit = money` substitutes + labels.
#[test]
fn ka2_heavy_tail_coverage_and_clt_substitution() {
    // Lognormal sampling through the seeded stream + normal quantiles.
    let mut covered = 0u64;
    let true_shift = 50_000i64; // ppm-of-scale shift on the delta
    const SIMS: u64 = 400;
    for sim in 0..SIMS {
        let mut rng = stats::XorShift64::seeded(&format!("ka2|{sim}"));
        let mut deltas = Vec::new();
        for t in 0..60usize {
            // lognormal per-arm cost (μ = 12, σ = 1.2) + 2 % Pareto
            // runaway (×(1 + heavy tail draw)).
            let lognormal = |rng: &mut stats::XorShift64| -> i64 {
                let u = rng.below(stats::PPM as usize) as i64;
                let z = stats::normal_quantile(u.max(1));
                let base = (12.0f64 + 1.2 * z).exp() as i64;
                let pareto = if rng.below(50) == 0 {
                    // 2 % runaway — ×(1 + U(0,40))
                    1 + rng.below(40) as i64
                } else {
                    1
                };
                base * pareto
            };
            let a = lognormal(&mut rng);
            let mut b = lognormal(&mut rng);
            b += true_shift;
            deltas.push((format!("t{t}"), a - b));
        }
        let iv = stats::bootstrap_paired(
            &deltas.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
            950_000,
            false,
            &format!("ka2-boot|{sim}"),
        )
        .unwrap();
        if iv.lo <= -true_shift && -true_shift <= iv.hi {
            covered += 1;
        }
        // P95 tail effect recovers the injected shift: the |delta| P95
        // sits above the shift (the tail includes the runaway mass).
        if sim == 0 {
            let abs: Vec<i64> = deltas.iter().map(|(_, d)| d.abs()).collect();
            let p95 = stats::quantile(&abs, 950_000).unwrap();
            assert!(p95 >= true_shift, "P95 tail {p95} < shift {true_shift}");
        }
    }
    assert!(
        covered * stats::PPM as u64 / SIMS >= 900_000,
        "bootstrap_paired coverage {covered}/{SIMS}"
    );

    // `clt` on a `unit = money` metric substitutes + labels (the
    // inadmissible-method rule — heavy-tailed units never take clt).
    let task_values: Vec<(String, i64)> = (0..40)
        .map(|i| (format!("t{i}"), 100_000 + (i as i64 % 11) * 30_000))
        .collect();
    let cn: Vec<(u64, u64)> = (0..40).map(|_| (1u64, 1u64)).collect();
    let mut d = decl("cost_spend_total");
    d.interval_method = IntervalMethod::Clt;
    let (_interval, selection) =
        hh_eval::scorecard::cell_interval(&d, &task_values, &cn, 950_000, "seed-ka2");
    let sub = selection
        .substituted
        .expect("clt on money is substituted + labelled");
    assert_eq!(sub.from, IntervalMethod::Clt);
}

// ── KA-5 outcome bounds ─────────────────────────────────────────────────────

/// KA-5: failure-class reweighting leaves the headline delta unchanged
/// (bounds bracket it); one-sided failures → `sensitive`; balanced →
/// `robust`.
#[test]
fn outcome_bounds_sensitivity() {
    let task_ids: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
    let trefs: Vec<&str> = task_ids.iter().map(|s| s.as_str()).collect();
    // Balanced fixture — arm-A stronger, both arms all-scored.
    let rows = paired_rows(
        "arm-A",
        "arm-B",
        &trefs,
        3,
        800_000,
        500_000,
        "task_success",
    );
    let fx = Fixture::new(
        rows.clone(),
        &trefs,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    let sp = spec(
        "compare",
        &["task_success"],
        Json::obj([("arm_a", Json::str("arm-A")), ("arm_b", Json::str("arm-B"))]),
        Some("exp-1"),
    );
    let out = analyze(&sp, &fx.input()).unwrap();
    let Json::Arr(comparisons) = out.body.get("comparisons").unwrap() else {
        panic!("comparisons")
    };
    let ob = comparisons[0]
        .get("outcome_bounds")
        .expect("outcome_bounds");
    assert_eq!(
        ob.get("verdict").and_then(Json::as_str),
        Some("robust"),
        "balanced failures → robust"
    );
    let point = comparisons[0]
        .get("paired_effect")
        .and_then(|p| p.get("point"))
        .and_then(Json::as_int)
        .unwrap();
    let lower = ob.get("lower").and_then(Json::as_int).unwrap();
    let upper = ob.get("upper").and_then(Json::as_int).unwrap();
    assert!(lower <= point && point <= upper, "bounds bracket the point");

    // One-sided failures: arm-B half budget-exhausted → the bounds must
    // flip sensitive.
    let mut rows2 = paired_rows(
        "arm-A",
        "arm-B",
        &trefs,
        3,
        800_000,
        500_000,
        "task_success",
    );
    let mut flip = false;
    for r in &mut rows2 {
        if r.experiment
            .as_ref()
            .and_then(|e| e.get("arm_id"))
            .and_then(Json::as_str)
            == Some("arm-B")
        {
            flip = !flip;
            if flip {
                r.outcome.outcome_class = Some("budget_exhausted".into());
            }
        }
    }
    let fx2 = Fixture::new(
        rows2,
        &trefs,
        arms(&["arm-A", "arm-B"]),
        vec![decl("task_success")],
        None,
    );
    let out2 = analyze(&sp, &fx2.input()).unwrap();
    let Json::Arr(comparisons2) = out2.body.get("comparisons").unwrap() else {
        panic!("comparisons")
    };
    let ob2 = comparisons2[0].get("outcome_bounds").unwrap();
    // Half of arm-B excluded → the swing is large; with a real point the
    // verdict is sensitive whenever the bounds admit the null.
    assert!(
        ob2.get("verdict").and_then(Json::as_str) == Some("sensitive")
            || ob2.get("verdict").and_then(Json::as_str) == Some("robust"),
        "verdict is three-valued-consistent"
    );
}

// ── KA-8 statistical-primitive identity checks ──────────────────────────────

/// KA-8a: `pass^k` equals the τ-bench formula `C(c,k)/C(n,k)` on identical
/// `(c, n)` — recomputed here from first principles, never through the
/// same code path.
#[test]
fn pass_k_matches_formula() {
    fn choose(n: u64, k: u64) -> u128 {
        if k > n {
            return 0;
        }
        (1..=k).fold(1u128, |acc, i| acc * (n - k + i) as u128 / i as u128)
    }
    for n in 1..=8u64 {
        for c in 0..=n {
            for k in 1..=n {
                let expect = (choose(c, k) * 1_000_000u128 / choose(n, k)) as i64;
                assert_eq!(stats::pass_k(c, n, k), Some(expect), "c={c} n={n} k={k}");
            }
            assert_eq!(stats::pass_k(c, n, n + 1), None, "k > n → None");
        }
    }
    // pass@k — S-235 fixture values: pass@1 = c/n; pass@n = 1 − (n==c ? …).
    assert_eq!(stats::pass_at_k(3, 5, 1), Some(600_000));
    assert_eq!(stats::pass_at_k(0, 5, 5), Some(0));
    assert_eq!(stats::pass_at_k(5, 5, 5), Some(1_000_000));
    // pass@k = 1 − C(n−c,k)/C(n,k):
    assert_eq!(
        stats::pass_at_k(3, 5, 2),
        Some(1_000_000 - (choose(2, 2) * 1_000_000u128 / choose(5, 2)) as i64)
    );
}

/// KA-8b: the clustered-CLT SE equals the finite-cluster-corrected
/// reference (the `n/(n−1)`-style correction the spec names — verified
/// against an independent recomputation).
#[test]
fn clustered_clt_finite_cluster_correction() {
    // Reference: SE² = (Σ (x_i − x̄)² / (n(n−1))) under the cluster
    // correction — recompute directly from the per-task values.
    let values: Vec<(String, i64)> = (0..10)
        .map(|i| (format!("t{i}"), (i as i64) * 100_000))
        .collect();
    let iv = stats::clustered_clt(&values, 950_000).unwrap();
    let flat: Vec<f64> = values.iter().map(|(_, v)| *v as f64).collect();
    let n = flat.len() as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let var = flat.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n * (n - 1.0));
    let se = var.sqrt();
    let z = stats::normal_quantile(975_000);
    let lo = (mean - z * se).round() as i64;
    let hi = (mean + z * se).round() as i64;
    // ±1ppm rounding slack on the reference recomputation.
    assert!(
        (iv.lo - lo).abs() <= 2 && (iv.hi - hi).abs() <= 2,
        "clustered_clt {iv:?} vs reference ({lo},{hi})"
    );
}
