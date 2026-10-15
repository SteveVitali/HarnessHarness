//! S5.4 — the M1 designed-ablation deepening (§5h.7; R-2.9.7¹;
//! KA-I7-4/-10/-11):
//!
//! - Typed `ComponentTarget`s: the kind set widens to
//!   `{slot, variant, rule, parameter, leaf→text_leaf, procedure,
//!   decision_point}`; the target cap (64) is a `BadSpec`, not a silent
//!   truncation; the `observation_substitution` intervention is the one
//!   hosted-admissible form (`interception: model_io` at
//!   `granularity: configuration`).
//! - The `retirement` recipe — the same M1 math plus `removal_verdict`
//!   (`pass`/`fail`/`inconclusive`) the debt settle path consumes.
//! - `estimand ∈ {designed_ablation, loo, loi}` rides the effect rows.
//! - `budget_allocation{regime ∈ {uniform, concentrated}, …}` — regimes
//!   never pool: a `uniform` split below `search_floor` nulls every
//!   effect; `concentrated` funds only `concentrated_on`.
//! - `report_id` — the content address; deterministic reruns mint the
//!   same id.
//! - `multiplicity{n_tests, correction}` + `reasons[]` members.
//! - `ops::component_targets` / `ops::attribution_design` — the design
//!   surface helpers the embed ops expose verbatim.

use std::collections::BTreeMap;

use hh_analysis::{analyze, AnalysisError, AnalysisInput};
use hh_budget::matchspec::{ArmSpec, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::catalogue;
use hh_eval::facts::LedgerFacts;
use hh_lab::analysis::{AnalysisSpec, QuerySpec};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, FactorDeclaration, FactorKind, FactorLevel,
    IntervalMethod, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
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

// ── fixture plumbing (mirrors tests/s5_3.rs) ─────────────────────────────────

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
        dead_weight_purpose: false,
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

fn task(id: &str) -> hh_eval::runs::TaskContext {
    hh_eval::runs::TaskContext {
        task_id: id.into(),
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        split_hash: "sha256:split-1".into(),
        stratum: ContaminationStratum::PrivateHeldOut,
    }
}

fn suite() -> hh_eval::runs::SuiteContext {
    hh_eval::runs::SuiteContext {
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
            value: hh_ontology::eval::MetricValueKind::Bool(value > 0),
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
    tasks: Vec<hh_eval::runs::TaskContext>,
    suites: Vec<hh_eval::runs::SuiteContext>,
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

/// The canonical S5.4 fixture: two native arms, one declared factor.
fn ablation_fixture() -> Fixture {
    let tasks = task_ids(8);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    let rows = paired_rows_exact("a", "b", &tasks, 2, 2, 1, "task_success");
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "environment",
        FactorKind::Environment,
        &["coding_terminal"],
    )];
    for f in fixture.facts.values_mut() {
        f.finished = Some(Json::obj([]));
    }
    fixture
}

fn attribution_spec(filters: Json) -> AnalysisSpec {
    let mut base = vec![
        ("arm_a".to_string(), Json::str("a")),
        ("arm_b".to_string(), Json::str("b")),
    ];
    if let Json::Obj(m) = filters {
        base.extend(m);
    }
    spec(
        "attribution",
        &["task_success"],
        Json::Obj(base.into_iter().collect()),
        Some("exp-1"),
    )
}

// ── typed ComponentTargets ──────────────────────────────────────────────────

#[test]
fn typed_target_kinds_and_leaf_canonicalization() {
    let fixture = ablation_fixture();
    for kind in [
        "slot",
        "variant",
        "rule",
        "parameter",
        "procedure",
        "decision_point",
    ] {
        let s = attribution_spec(Json::obj([(
            "targets",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str(kind)),
                ("ref", Json::str("environment")),
            ])]),
        )]));
        let out = analyze(&s, &fixture.input()).unwrap_or_else(|e| panic!("kind {kind}: {e:?}"));
        let att = out.body.get("attribution").expect("attribution");
        match att.get("targets") {
            Some(Json::Arr(t)) => {
                assert_eq!(t[0].get("kind").and_then(Json::as_str), Some(kind));
            }
            other => panic!("targets: {other:?}"),
        }
    }
    // The Stage-3 spelling `leaf` canonicalizes to `text_leaf`.
    let s = attribution_spec(Json::obj([(
        "targets",
        Json::Arr(vec![Json::obj([
            ("kind", Json::str("leaf")),
            ("ref", Json::str("environment")),
        ])]),
    )]));
    let out = analyze(&s, &fixture.input()).expect("leaf");
    let att = out.body.get("attribution").expect("attribution");
    match att.get("targets") {
        Some(Json::Arr(t)) => {
            assert_eq!(t[0].get("kind").and_then(Json::as_str), Some("text_leaf"));
        }
        other => panic!("targets: {other:?}"),
    }
}

#[test]
fn unknown_target_kind_and_target_cap_refuse() {
    let fixture = ablation_fixture();
    let s = attribution_spec(Json::obj([(
        "targets",
        Json::Arr(vec![Json::obj([
            ("kind", Json::str("orchestration")),
            ("ref", Json::str("environment")),
        ])]),
    )]));
    match analyze(&s, &fixture.input()) {
        Err(AnalysisError::BadSpec { member, .. }) => {
            assert_eq!(member, "filters.targets.kind")
        }
        other => panic!("unknown kind must refuse: {other:?}"),
    }
    // The 64-target cap — never a silent truncation.
    let too_many: Vec<Json> = (0..65)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("parameter")),
                ("ref", Json::str(format!("p{i}"))),
            ])
        })
        .collect();
    let s = attribution_spec(Json::obj([("targets", Json::Arr(too_many))]));
    match analyze(&s, &fixture.input()) {
        Err(AnalysisError::BadSpec { member, .. }) => assert_eq!(member, "filters.targets"),
        other => panic!("over-cap targets must refuse: {other:?}"),
    }
}

// ── recipe + estimand ───────────────────────────────────────────────────────

#[test]
fn retirement_recipe_carries_removal_verdict() {
    let fixture = ablation_fixture();
    let s = attribution_spec(Json::obj([
        ("recipe", Json::str("retirement")),
        (
            "targets",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("rule")),
                ("ref", Json::str("environment")),
            ])]),
        ),
    ]));
    let out = analyze(&s, &fixture.input()).expect("retirement");
    let att = out.body.get("attribution").expect("attribution");
    assert_eq!(att.get("recipe").and_then(Json::as_str), Some("retirement"));
    assert_eq!(
        att.get("ablation_manifest")
            .and_then(|m| m.get("recipe"))
            .and_then(Json::as_str),
        Some("retirement")
    );
    // Every computed effect under tolerance → `pass`.
    let v = att
        .get("removal_verdict")
        .and_then(|v| v.get("verdict"))
        .and_then(Json::as_str)
        .expect("removal_verdict");
    assert!(["pass", "fail", "inconclusive"].contains(&v));
    let want = format!("retirement_verdict:{v}");
    let reasons_has = |att: &Json, needle: &str| match att.get("reasons") {
        Some(Json::Arr(rs)) => rs.iter().any(|r| r.as_str() == Some(needle)),
        _ => false,
    };
    assert!(reasons_has(att, &want), "reasons name the verdict");
}

#[test]
fn estimand_spellings_ride_the_effects() {
    let fixture = ablation_fixture();
    for estimand in ["loo", "loi"] {
        let s = attribution_spec(Json::obj([
            ("estimand", Json::str(estimand)),
            (
                "targets",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("parameter")),
                    ("ref", Json::str("environment")),
                ])]),
            ),
        ]));
        let out =
            analyze(&s, &fixture.input()).unwrap_or_else(|e| panic!("estimand {estimand}: {e:?}"));
        let att = out.body.get("attribution").expect("attribution");
        assert_eq!(att.get("estimand").and_then(Json::as_str), Some(estimand));
        match att.get("effects") {
            Some(Json::Arr(e)) => {
                assert!(!e.is_empty());
                for eff in e {
                    assert_eq!(eff.get("estimand").and_then(Json::as_str), Some(estimand));
                }
            }
            other => panic!("effects: {other:?}"),
        }
    }
}

#[test]
fn unknown_recipe_estimand_intervention_refuse() {
    let fixture = ablation_fixture();
    for (member, filters) in [
        (
            "filters.recipe",
            Json::obj([("recipe", Json::str("ad_hoc"))]),
        ),
        (
            "filters.estimand",
            Json::obj([("estimand", Json::str("ate"))]),
        ),
        (
            "filters.targets.intervention",
            Json::obj([(
                "targets",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("parameter")),
                    ("ref", Json::str("environment")),
                    ("intervention", Json::str("prompt_rewrite")),
                ])]),
            )]),
        ),
    ] {
        match analyze(&attribution_spec(filters), &fixture.input()) {
            Err(AnalysisError::BadSpec { member: m, .. }) => assert_eq!(m, member),
            other => panic!("{member} must refuse: {other:?}"),
        }
    }
}

// ── budget allocation regimes ───────────────────────────────────────────────

#[test]
fn uniform_allocation_below_search_floor_nulls_effects() {
    let fixture = ablation_fixture();
    let s = attribution_spec(Json::obj([
        (
            "targets",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("parameter")),
                ("ref", Json::str("environment")),
            ])]),
        ),
        (
            "budget_allocation",
            Json::obj([
                ("regime", Json::str("uniform")),
                ("per_target", Json::Int(3)),
                ("search_floor", Json::Int(10)),
            ]),
        ),
    ]));
    let out = analyze(&s, &fixture.input()).expect("below-floor");
    let att = out.body.get("attribution").expect("attribution");
    // `budget_allocation_ref` — the content address names the allocation.
    let alloc_ref = att
        .get("budget")
        .and_then(|b| b.get("budget_allocation_ref"))
        .and_then(Json::as_str)
        .expect("budget_allocation_ref");
    assert!(alloc_ref.starts_with("sha256:"), "{alloc_ref}");
    // Every effect nulled (the allocation below floor nulls them — never
    // a pooled estimate).
    match att.get("effects") {
        Some(Json::Arr(e)) => {
            assert!(!e.is_empty());
            for eff in e {
                assert_eq!(eff.get("point"), Some(&Json::Null));
                assert_eq!(eff.get("interval"), Some(&Json::Null));
            }
        }
        other => panic!("effects: {other:?}"),
    }
    assert!(match att.get("reasons") {
        Some(Json::Arr(rs)) => rs
            .iter()
            .any(|r| r.as_str() == Some("budget_allocation_below_floor")),
        _ => false,
    });
    assert_eq!(att.get("n_effects").and_then(Json::as_int), Some(0));
}

#[test]
fn concentrated_allocation_funds_only_the_named_target() {
    let fixture = ablation_fixture();
    let s = attribution_spec(Json::obj([
        (
            "targets",
            Json::Arr(vec![
                Json::obj([
                    ("kind", Json::str("parameter")),
                    ("ref", Json::str("environment")),
                ]),
                Json::obj([
                    ("kind", Json::str("parameter")),
                    ("ref", Json::str("unfunded")),
                ]),
            ]),
        ),
        (
            "factors",
            Json::Arr(vec![Json::str("environment"), Json::str("unfunded")]),
        ),
        (
            "budget_allocation",
            Json::obj([
                ("regime", Json::str("concentrated")),
                ("concentrated_on", Json::str("environment")),
            ]),
        ),
    ]));
    let out = analyze(&s, &fixture.input()).expect("concentrated");
    let att = out.body.get("attribution").expect("attribution");
    match att.get("effects") {
        Some(Json::Arr(e)) => {
            let unfunded = e
                .iter()
                .find(|x| x.get("factor").and_then(Json::as_str) == Some("unfunded"))
                .expect("the unfunded row is listed, never dropped");
            assert_eq!(unfunded.get("point"), Some(&Json::Null));
        }
        other => panic!("effects: {other:?}"),
    }
    assert!(match att.get("reasons") {
        Some(Json::Arr(rs)) => rs.iter().any(|r| r.as_str() == Some("budget_concentrated")),
        _ => false,
    });
}

#[test]
fn unknown_allocation_regime_refuses() {
    let fixture = ablation_fixture();
    let s = attribution_spec(Json::obj([(
        "budget_allocation",
        Json::obj([("regime", Json::str("pooled"))]),
    )]));
    match analyze(&s, &fixture.input()) {
        Err(AnalysisError::BadSpec { member, .. }) => {
            assert_eq!(member, "filters.budget_allocation.regime")
        }
        other => panic!("unknown regime must refuse: {other:?}"),
    }
}

// ── report_id + multiplicity ────────────────────────────────────────────────

#[test]
fn report_id_is_deterministic_and_multiplicity_counts_tests() {
    let fixture = ablation_fixture();
    let mk = || {
        attribution_spec(Json::obj([(
            "targets",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("rule")),
                ("ref", Json::str("environment")),
            ])]),
        )]))
    };
    let a = analyze(&mk(), &fixture.input()).expect("run a");
    let b = analyze(&mk(), &fixture.input()).expect("run b");
    let att_a = a.body.get("attribution").expect("attribution a");
    let att_b = b.body.get("attribution").expect("attribution b");
    let id_a = att_a.get("report_id").and_then(Json::as_str).expect("id a");
    let id_b = att_b.get("report_id").and_then(Json::as_str).expect("id b");
    assert!(id_a.starts_with("sha256:"), "{id_a}");
    assert_eq!(id_a, id_b, "deterministic reruns mint the same id");
    // The id is the content address of the report *sans* the member —
    // byte-identical bodies carry it.
    let mut sans = att_a.clone();
    if let Json::Obj(m) = &mut sans {
        m.remove("report_id");
    }
    let mut sans_b = att_b.clone();
    if let Json::Obj(m) = &mut sans_b {
        m.remove("report_id");
    }
    assert_eq!(sans, sans_b);
    // `multiplicity{n_tests, correction}` — named, never silent.
    match att_a.get("multiplicity") {
        Some(m) => {
            assert!(m.get("n_tests").and_then(Json::as_int).is_some());
            assert_eq!(m.get("correction").and_then(Json::as_str), Some("none"));
        }
        other => panic!("multiplicity: {other:?}"),
    }
    assert!(att_a.get("reasons").is_some());
}

// ── the hosted observation_substitution exception ───────────────────────────

#[test]
fn hosted_observation_substitution_at_model_io_is_admissible() {
    let tasks = task_ids(8);
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    // Hosted rows in *both* arms — the substitution compares them.
    let mut rows = paired_rows_exact("a", "b", &tasks, 2, 2, 1, "task_success");
    for t in &tasks {
        for rep in 0..2u64 {
            rows.push(hosted_row("a", t, rep, 1, "task_success"));
            rows.push(hosted_row("b", t, rep, 1, "task_success"));
        }
    }
    let mut fixture = Fixture::new(rows, &refs, arms(&["a", "b"]), vec![decl("task_success")]);
    fixture.design.factors = vec![factor(
        "environment",
        FactorKind::Environment,
        &["coding_terminal"],
    )];
    for f in fixture.facts.values_mut() {
        f.finished = Some(Json::obj([]));
    }
    let s = attribution_spec(Json::obj([(
        "targets",
        Json::Arr(vec![Json::obj([
            ("kind", Json::str("parameter")),
            ("ref", Json::str("environment")),
            ("intervention", Json::str("observation_substitution")),
            ("interception", Json::str("model_io")),
            ("granularity", Json::str("configuration")),
        ])]),
    )]));
    let out = analyze(&s, &fixture.input()).expect("hosted substitution");
    let att = out.body.get("attribution").expect("attribution");
    match att.get("effects") {
        Some(Json::Arr(e)) => {
            let sub = e
                .iter()
                .find(|x| {
                    x.get("estimand").and_then(Json::as_str) == Some("observation_substitution")
                })
                .expect("a hosted substitution effect row");
            assert_eq!(
                sub.get("validity_mode").and_then(Json::as_str),
                Some("hosted_interception")
            );
            assert_eq!(
                sub.get("interception").and_then(Json::as_str),
                Some("model_io")
            );
        }
        other => panic!("effects: {other:?}"),
    }
    // The hosted rows still surface in `na_rows` for the class-level
    // accounting (nothing silently dropped).
    assert!(att.get("na_rows").is_some());
}

#[test]
fn hosted_ablation_still_marks_na_class() {
    // A hosted target without the `model_io`/`configuration` exception
    // stays `n/a{class}` — the hosted rows are listed, never folded.
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
    let s = attribution_spec(Json::obj([(
        "targets",
        Json::Arr(vec![Json::obj([
            ("kind", Json::str("parameter")),
            ("ref", Json::str("environment")),
        ])]),
    )]));
    let out = analyze(&s, &fixture.input()).expect("hosted ablation");
    let att = out.body.get("attribution").expect("attribution");
    let na = match att.get("na_rows") {
        Some(Json::Arr(v)) => v.clone(),
        other => panic!("na_rows: {other:?}"),
    };
    assert_eq!(na.len(), 1);
    assert_eq!(na[0].get("n/a").and_then(Json::as_str), Some("class"));
}

// ── the design-surface helpers ──────────────────────────────────────────────

#[test]
fn component_targets_enumerates_the_sealed_definition() {
    let def = Json::obj([
        ("slots", Json::Arr(vec![Json::str("model")])),
        (
            "variants",
            Json::Arr(vec![Json::obj([("name", Json::str("v1"))])]),
        ),
        (
            "rules",
            Json::Arr(vec![Json::obj([("rule_id", Json::str("r1"))])]),
        ),
        ("parameters", Json::obj([("temperature", Json::Int(0))])),
        ("procedures", Json::Arr(vec![Json::str("proc:main")])),
        ("leaves", Json::Arr(vec![Json::str("leaf:a")])),
        ("decision_points", Json::Arr(vec![Json::str("dp:route")])),
    ]);
    let targets = hh_analysis::ops::component_targets(&def, None);
    let kinds: Vec<&str> = targets
        .iter()
        .filter_map(|t| t.get("kind").and_then(Json::as_str))
        .collect();
    for k in [
        "slot",
        "variant",
        "rule",
        "parameter",
        "procedure",
        "text_leaf",
        "decision_point",
    ] {
        assert!(kinds.contains(&k), "kind {k} missing: {targets:?}");
    }
    // `filter{kinds[], pattern}` narrows the enumeration.
    let only_procs = hh_analysis::ops::component_targets(
        &def,
        Some(&Json::obj([(
            "kinds",
            Json::Arr(vec![Json::str("procedure")]),
        )])),
    );
    assert_eq!(only_procs.len(), 1);
    assert_eq!(
        only_procs[0].get("ref").and_then(Json::as_str),
        Some("proc:main")
    );
    let pat = hh_analysis::ops::component_targets(
        &def,
        Some(&Json::obj([("pattern", Json::str("leaf"))])),
    );
    assert_eq!(pat.len(), 1);
    assert_eq!(pat[0].get("kind").and_then(Json::as_str), Some("text_leaf"));
}

#[test]
fn attribution_design_document_shape() {
    let d = hh_analysis::ops::attribution_design(
        "design:d1",
        vec![Json::obj([
            ("kind", Json::str("procedure")),
            ("ref", Json::str("proc:main")),
        ])],
        &["a".to_string(), "b".to_string()],
        Some("matchspec:1"),
        Some(Json::obj([("regime", Json::str("uniform"))])),
    );
    assert_eq!(
        d.get("schema").and_then(Json::as_str),
        Some("hh-attribution-design/1")
    );
    assert_eq!(d.get("method").and_then(Json::as_str), Some("M1"));
    assert_eq!(
        d.get("design_ref").and_then(Json::as_str),
        Some("design:d1")
    );
    assert_eq!(
        d.get("match_spec_ref").and_then(Json::as_str),
        Some("matchspec:1")
    );
    assert!(d.get("budget_allocation").is_some());
    match d.get("arms") {
        Some(Json::Arr(a)) => assert_eq!(a.len(), 2),
        other => panic!("arms: {other:?}"),
    }
}
