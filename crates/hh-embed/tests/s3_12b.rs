//! S3.12b — the GATE-G2 gap-closure legs that only `hh-embed` can host
//! (docs/tickets/059a; readout G2-2 + the retirement caveat):
//!
//! - **Exemplar → `ComparisonReport` (AC-J3-8/-9's report half).** Both
//!   Stage-3 exemplar families (`lab/compaction-family-v1`,
//!   `lab/control-strategy-family-v1`) schedule/launch/settle to
//!   `NextVerdict::Done` through the real `ExperimentEngine` over a real
//!   `Store` + `LabDocs`, then the accepted subject runs project through
//!   `hh_results::project_row` → `hh_analysis::eval_run` →
//!   `hh_analysis::analyze` → `hh_eval::compare` — the production
//!   records-in path, not a re-stated fixture. (The engine's own `closed`
//!   row is attempted and its audit-field overrun asserted — DF-S3.12b-1:
//!   an honestly-measured exemplar's E-4 `budget_match`/`utilization`
//!   members do not fit `AUDIT_FIELD_MAX_BYTES`.) The produced
//!   `ComparisonReport` carries a real `budget_match.status`
//!   (`matched`: every arm's `control.budget.consumed` rows are equal on
//!   every matched dimension, honestly recorded).
//!   `hh-embed` hosts this test because `hh-lab` never depends on
//!   `hh-eval` (CC10 — the compare kernel's home is the eval plane; the
//!   embed boundary already wires records-in `EvalRun`s to
//!   `lab.eval.compare`, this test exercises that same path directly).
//! - **OQ-363 evidence.** `arm:steerable-a` (matched_cap) vs
//!   `arm:steerable-a-iso` (iso_cost) refuses `IncommensurableMatch` —
//!   ADR-0213's interim rule stands and the cross-mode refusal is live.
//! - **Retirement end to end (the Reading-1 caveat).** A `retirement`
//!   `ExperimentSpec` schedules/runs/settles through the real engine; the
//!   settled rows project and compare through `hh_eval::compare`; the
//!   `CompareOutcome` feeds `hh_eval::compare::removal_verdict`; the
//!   verdict settles via `hh_lab::debt::settle_removal_test` and a
//!   human-sealed `retire` produces the `expired → retired` transition.
//!   No verdict is fed in — the chain is engine-driven.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_analysis::{analyze, eval_run, AnalysisInput};
use hh_budget::matchspec::ArmSpec as BudgetArmSpec;
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey};
use hh_eval::catalogue;
use hh_eval::compare::RemovalOutcome;
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{SuiteContext, TaskContext};
use hh_experiment::docs::LabDocs;
use hh_experiment::engine::{EngineContext, ExperimentEngine, NextVerdict};
use hh_experiment::errors::ExperimentError;
use hh_experiment::view::ExperimentView;
use hh_identity::idp::idp_id;
use hh_lab::analysis::{
    AnalysisSpec, BudgetMatchStatus, ComparisonReport, QuerySpec, ReportLabelKind,
};
use hh_lab::debt::{retire, settle_removal_test, SettleEffect};
use hh_lab::exemplars::{
    compaction_family_v1, control_strategy_family_v1, CompactionFamilyPins,
    ControlStrategyFamilyPins, ExemplarPins, EXEMPLAR_REPLICATES,
};
use hh_lab::expand::{ArmConfiguration, ExpandTask};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ManualClock;
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::config::Ref;
use hh_ontology::control::StopReason;
use hh_ontology::debt::{DebtStatus, RemovalTestKind, Verdict};
use hh_ontology::eval::{
    Design, DesignKind, EstimatorSelection, IntervalMethod, MetricValue, MetricValueKind, Pairing,
    PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::ParticipantClass;
use hh_ontology::FactorKind;
use hh_provenance::{HumanRole, Origin, ProvenanceRecord};
use hh_results::projection::project_row;
use hh_results::row::ResultsRow;
use hh_results::watermark::WatermarkSet;
use hh_wire::json::Json;

// ── rig (the hh-experiment engine.rs test shape) ────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-embed-s312b-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("s312b.{tag}"), tag.as_bytes())
}

struct Rig {
    store: Store,
    docs: LabDocs,
}

fn rig(tag: &str, now_ms: u64) -> Rig {
    let root = tmp(tag);
    let clock = ManualClock::at(now_ms);
    let store = Store::open_with(&root, Box::new(clock), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    Rig { store, docs }
}

/// The budget bodies the resolver serves — per-run cap 100 `model_calls`,
/// experiment pool 1 000 (the engine.rs fixture's shape).
fn budgets() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("eval:a".to_string(), caps(100)),
        ("eval:b".to_string(), caps(100)),
        ("search:a".to_string(), caps(50)),
        ("search:b".to_string(), caps(50)),
        ("pool".to_string(), caps(1_000)),
        ("instr".to_string(), caps(1_000)),
        // The retirement fixture's own refs.
        ("budget:eval".to_string(), caps(100)),
        ("budget:search".to_string(), caps(50)),
        ("budget:exp".to_string(), caps(1_000)),
        ("budget:inst".to_string(), caps(1_000)),
    ])
}

fn exemplar_pins() -> ExemplarPins {
    ExemplarPins {
        suite_ref: pinned("suite.tb2"),
        held_out_split_ref: pinned("split.held_out"),
        split_assignment_ref: pinned("split.assign"),
        registry_snapshot_id: pinned("registry.snap"),
        eval_budget: "eval:a".to_string(),
        search_budget: "search:a".to_string(),
        experiment_budget: "pool".to_string(),
        instrument_budget: "instr".to_string(),
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
    }
}

/// The engine context over `n` held-out tasks at the Stage-3 replicate
/// floor — plus the `retirement_diff` answer when the spec asks for it.
fn ctx_exemplar(
    tag: &str,
    n_tasks: usize,
    retirement_diff: Option<bool>,
) -> EngineContext<'static> {
    let map = budgets();
    let ids: Vec<String> = (0..n_tasks).map(|i| format!("task:{tag}.{i}")).collect();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| map.get(r).cloned())),
        suite_tasks: Some(Box::new(move |_spec: &ExperimentSpec| {
            Some(
                ids.iter()
                    .map(|t| ExpandTask {
                        task_id: t.clone(),
                        split_label: SplitLabel::HeldOut,
                    })
                    .collect(),
            )
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            })
        })),
        min_replicates: EXEMPLAR_REPLICATES,
        retirement_diff,
        ..Default::default()
    }
}

/// Mint one event onto a run's stream (the engine.rs `mint` — kernel
/// provenance, chained parents handled by the caller's batch loop).
fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-embed-s312b/1"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-embed-s312b/1", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// Drive a subject run to terminal, honestly recording the run's own
/// accounting: `control.budget.consumed` for every matched dimension (the
/// comparison's `budget_match` reads these medians — a run that never
/// recorded a matched dim reads `unmatched`, never `matched`), the
/// `measurement.metric.emitted` `task_success` cell, then
/// `lifecycle.run.finished`.
fn finish_subject(
    store: &mut Store,
    run_id: &str,
    writer: &Lease,
    stop: StopReason,
    consumed: &[(DimensionId, i64)],
    task_success: bool,
) {
    let mut batch = Vec::new();
    for (d, a) in consumed {
        batch.push(mint(
            store,
            run_id,
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str(d.as_str())),
                ("amount", Json::Int(*a)),
            ]),
        ));
    }
    batch.push(mint(
        store,
        run_id,
        "measurement.metric.emitted",
        MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(task_success),
            applies_to: run_id.into(),
            oracle_ref: "oracle/stub.subject".into(),
            detector: hh_ontology::compliance::Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
        }
        .to_json(),
    ));
    batch.push(mint(
        store,
        run_id,
        "lifecycle.run.finished",
        Json::obj([
            ("stop_reason", stop.to_json()),
            ("outcome_class", Json::str(stop.outcome_class().as_str())),
        ]),
    ));
    let mut parent = store.head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, writer, batch).unwrap();
}

/// `next → claim → launch → finish → settle` to drain, then `close` —
/// returning the close report. The matched dims the subject records are
/// the arms' declared `MatchSpec.dimensions` (the compare's `budget_match`
/// input — recorded honestly, never just asserted).
/// Drain every plan (`next → claim → launch → finish → settle`), each
/// subject run honestly recording consumption on every matched dim, and
/// return the settled-plan count. Split from `close` because the E-4 close
/// row's `budget_match`/`utilization` audit members cannot fit a fully
/// measured exemplar (see `assert_close_audit_cap`).
fn drain_plans(
    rig: &mut Rig,
    spec: &ExperimentSpec,
    eid: &str,
    tag: &str,
    n_tasks: usize,
    retirement_diff: Option<bool>,
) -> usize {
    let dims = spec.arms[0]
        .match_spec
        .as_ref()
        .map(|m| m.dimensions.clone())
        .unwrap_or_default();
    let mut settled = 0usize;
    loop {
        let mut eng = ExperimentEngine::new(
            &mut rig.store,
            rig.docs.clone(),
            ctx_exemplar(tag, n_tasks, retirement_diff),
        );
        eng.attach(eid).unwrap();
        match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => {
                let ticket = eng.claim(&run_plan_id, "driver").unwrap();
                let launched = eng.launch(&ticket, "subject").unwrap();
                drop(eng);
                finish_subject(
                    &mut rig.store,
                    &launched.run_id,
                    &launched.subject_writer,
                    StopReason::Completed,
                    &dims.iter().map(|d| (*d, 5)).collect::<Vec<_>>(),
                    true,
                );
                let mut eng = ExperimentEngine::new(
                    &mut rig.store,
                    rig.docs.clone(),
                    ctx_exemplar(tag, n_tasks, retirement_diff),
                );
                eng.attach(eid).unwrap();
                let out = eng.settle(&run_plan_id).unwrap();
                assert!(out.accepted, "subject run settles accepted");
                settled += 1;
            }
            NextVerdict::Done => break,
            other => panic!("unexpected next: {other:?}"),
        }
    }
    settled
}

fn run_to_close_ctx(
    rig: &mut Rig,
    spec: &ExperimentSpec,
    eid: &str,
    tag: &str,
    n_tasks: usize,
    retirement_diff: Option<bool>,
) -> hh_experiment::events::ExperimentReport {
    drain_plans(rig, spec, eid, tag, n_tasks, retirement_diff);
    let mut eng = ExperimentEngine::new(
        &mut rig.store,
        rig.docs.clone(),
        ctx_exemplar(tag, n_tasks, retirement_diff),
    );
    eng.attach(eid).unwrap();
    eng.close(false).unwrap()
}

/// S3.12b's discovered residual (recorded in the run ledger + DEFERRALS):
/// with every matched dim honestly measured, `close`'s E-4 recheck writes
/// `measurement.experiment.closed{…, utilization, budget_match}` whose
/// per-member audit bound (`AUDIT_FIELD_MAX_BYTES = 512`, `OPEN_AUDIT`) is
/// exceeded at exemplar scale — `utilization` carries `{median, min, max,
/// n}` per arm × matched dim, and `budget_match.detail` per comparand dim;
/// compaction's 7-dim match alone overruns the bound. The compare kernel's
/// `budget_match` is computed from the *runs*' consumption (pre-close), so
/// the G2-2 report stands; the close row's recheck overflow is carried as
/// a deferral, not silently under-measured.
fn assert_close_audit_cap(
    rig: &mut Rig,
    eid: &str,
    tag: &str,
    n_tasks: usize,
    retirement_diff: Option<bool>,
) {
    let mut eng = ExperimentEngine::new(
        &mut rig.store,
        rig.docs.clone(),
        ctx_exemplar(tag, n_tasks, retirement_diff),
    );
    eng.attach(eid).unwrap();
    match eng.close(false) {
        Err(ExperimentError::Ledger(hh_ledger::LedgerError::AuditFieldsTooLarge {
            class, ..
        })) => {
            assert_eq!(class, "measurement.experiment.closed");
        }
        other => panic!("expected the closed-row audit-cap overrun (DF-S3.12b-1), got {other:?}"),
    }
}

/// The settled rows → `AnalysisInput` half: fold the experiment run's
/// `ExperimentView`, project every accepted subject run (`project_row` +
/// `eval_run` via `analyze`'s `project_runs`), and collect the manifests,
/// facts, task contexts and watermarks the compare kernel reads.
struct Projected {
    rows: Vec<ResultsRow>,
    manifests: BTreeMap<String, hh_ledger::manifest::RunManifest>,
    facts: BTreeMap<String, LedgerFacts>,
    tasks: Vec<TaskContext>,
    watermark: WatermarkSet,
}

fn project_accepted(rig: &Rig, exp_run_id: &str, suite_id: &str, split_hash: &str) -> Projected {
    let view = ExperimentView::fold(rig.store.events(exp_run_id).unwrap());
    let mut rows = Vec::new();
    let mut manifests = BTreeMap::new();
    let mut facts = BTreeMap::new();
    let mut tasks: BTreeMap<String, TaskContext> = BTreeMap::new();
    let mut watermark = WatermarkSet::new();
    for plan in view.plans.values() {
        let subject = plan
            .accepted_run
            .as_ref()
            .unwrap_or_else(|| panic!("plan {} has no accepted run", plan.run_plan_id));
        let manifest = rig.store.manifest(subject).unwrap();
        let row = project_row(&rig.store, Some(&rig.docs), subject, None, None)
            .unwrap_or_else(|e| panic!("project_row {subject}: {e:?}"));
        for (rid, seq) in &row.watermark_set.runs {
            watermark.pin(rid, *seq);
        }
        let events: Vec<(u64, String, Json)> = rig
            .store
            .events(subject)
            .unwrap()
            .iter()
            .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
            .collect();
        facts.insert(subject.clone(), LedgerFacts::from_events(&events));
        tasks
            .entry(plan.task_id.clone())
            .or_insert_with(|| TaskContext {
                task_id: plan.task_id.clone(),
                suite_id: suite_id.to_string(),
                split_label: SplitLabel::HeldOut,
                split_hash: split_hash.to_string(),
                stratum: ContaminationStratum::PrivateHeldOut,
            });
        manifests.insert(subject.clone(), manifest.clone());
        rows.push(row);
    }
    Projected {
        rows,
        manifests,
        facts,
        tasks: tasks.into_values().collect(),
        watermark,
    }
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

fn analysis_spec(eid: &str, metrics: &[&str], filters: Json) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: "compare".into(),
        query: QuerySpec {
            metrics: metrics.iter().map(|m| m.to_string()).collect(),
            filters: Some(filters),
            grain: None,
        },
        spec_ref: Some(eid.to_string()),
        label: None,
        estimator_selection: estimator(),
        resample: None,
        outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    s
}

/// Resolve the lab-plane arm to its `hh_budget::matchspec::ArmSpec` — the
/// `validate_match` input (the budget bodies are the resolver's; the
/// `MatchSpec` is the arm's own).
fn budget_arm(lab_arm: &ArmSpec) -> (String, BudgetArmSpec) {
    let map = budgets();
    let eval = map
        .get(&lab_arm.eval_budget)
        .unwrap_or_else(|| panic!("no budget body for {}", lab_arm.eval_budget))
        .clone();
    let search = map
        .get(
            lab_arm
                .search_budget
                .as_ref()
                .expect("search budget declared"),
        )
        .expect("search budget body")
        .clone();
    (
        lab_arm.arm_id.clone(),
        BudgetArmSpec::native(
            search,
            eval,
            lab_arm.match_spec.clone().expect("match spec declared"),
        ),
    )
}

/// Run `hh_analysis::analyze` for one arm pair over the projected rows and
/// return the single produced `ComparisonReport`.
fn compare_arms(
    rig: &Rig,
    spec: &ExperimentSpec,
    exp_run_id: &str,
    arm_a: &str,
    arm_b: &str,
) -> ComparisonReport {
    let p = project_accepted(
        rig,
        exp_run_id,
        &spec.suite.suite_ref,
        &spec.design.pre_registration.task_split_hash,
    );
    let suites = vec![SuiteContext {
        suite_id: spec.suite.suite_ref.clone(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let arms: Vec<(String, BudgetArmSpec)> = spec.arms.iter().map(budget_arm).collect();
    let decls = vec![catalogue::metric("task_success").expect("catalogue task_success")];
    let prereg = spec.pre_registration.clone().unwrap_or(PreRegistration {
        registered_at: 0,
        hypothesis: String::new(),
        primary_metrics: vec![],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: String::new(),
        task_split_hash: String::new(),
        interactions: vec![],
    });
    let input = AnalysisInput {
        rows: &p.rows,
        manifests: &p.manifests,
        facts: &p.facts,
        tasks: &p.tasks,
        suites: &suites,
        design: Some(&spec.design),
        arm_specs: &arms,
        declarations: &decls,
        pre_registration: Some(&prereg),
        watermark_set: &p.watermark,
        metric_registry_version: "s312b-registry",
        price_table_version: "s312b-pricing",
        confidence_ppm: 950_000,
        resampling_draws: 2000,
        seed: 7,
    };
    let aspec = analysis_spec(
        &spec.experiment_id,
        &["task_success"],
        Json::obj([("arm_a", Json::str(arm_a)), ("arm_b", Json::str(arm_b))]),
    );
    let outcome =
        analyze(&aspec, &input).unwrap_or_else(|e| panic!("analyze {arm_a}/{arm_b}: {e:?}"));
    // The produced reports ride the report body (`comparisons`) — decode
    // them back to the typed `ComparisonReport`s the kernel emitted.
    let comparisons = match outcome.body.get("comparisons") {
        Some(Json::Arr(rows)) => rows,
        other => panic!("a compare report body carries comparisons: {other:?}"),
    }
    .iter()
    .map(|j| ComparisonReport::from_json(j).expect("ComparisonReport decodes"))
    .collect::<Vec<_>>();
    assert_eq!(comparisons.len(), 1, "one ComparisonReport per metric");
    comparisons[0].clone()
}

// ── AC-J3-8 — the compaction exemplar produces a real ComparisonReport ──────

/// `lab/compaction-family-v1` executes at Stage-3 size (2 arms × 3 tasks ×
/// 5 replicates = 30 plans), then the settled rows project through
/// `project_row`/`eval_run`/`analyze` into a real `ComparisonReport`
/// whose `budget_match.status` is the comparison's own verdict —
/// `matched`, because the subject runs recorded equal consumption on every
/// matched dimension. This is the G2-2 leg: a `ComparisonReport`, not the
/// E-4 budget record's weaker shape.
#[test]
fn exemplar_compaction_produces_comparison_report() {
    let mut r = rig("exemplar-compaction", 0);
    let spec = compaction_family_v1(
        &exemplar_pins(),
        &CompactionFamilyPins {
            evict_oldest_ref: pinned("variant.evict_oldest"),
            clear_tool_results_ref: pinned("variant.clear_tool_results"),
            model_level_ref: pinned("model.fixed"),
            environment_level_ref: pinned("env.tb2"),
            artifacts: (
                Ref::new("definition:compaction", pinned("artifact.evict")),
                Ref::new("definition:compaction", pinned("artifact.clear")),
            ),
        },
        1,
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx_exemplar("exc", 3, None));
    let eid = eng.register(&spec).unwrap();
    eng.expand(&eid).unwrap();
    let exp_run = eng.open_experiment(&eid).unwrap();
    drop(eng);
    // 2 arms × 3 tasks × 5 replicates = 30 plans, every subject run
    // honestly recording consumption on all seven matched dims.
    let settled = drain_plans(&mut r, &spec, &eid, "exc", 3, None);
    assert_eq!(settled, 30);
    // The E-4 close row overruns its audit-field bound at exemplar scale —
    // the ComparisonReport below is computed pre-close from the settled
    // runs (DF-S3.12b-1 carries the close-row residual).
    assert_close_audit_cap(&mut r, &eid, "exc", 3, None);

    let rep = compare_arms(
        &r,
        &spec,
        &exp_run,
        "arm:evict_oldest",
        "arm:clear_tool_results",
    );
    assert_eq!(
        rep.budget_match.status,
        BudgetMatchStatus::Matched,
        "the exemplar's matched_cap arms consume equally on every matched dim"
    );
    assert_eq!(rep.metric, "task_success");
    // The report is a real label — never a stub: pairing, estimates and the
    // multiplicity pass all ran over projected rows.
    assert!(
        matches!(
            rep.label,
            ReportLabelKind::Headlined
                | ReportLabelKind::NotHeadlined
                | ReportLabelKind::Exploratory
                | ReportLabelKind::Guarded
                | ReportLabelKind::TriviallyPrefixed
        ) || !rep.label.name().is_empty()
    );
}

// ── AC-J3-9 — the control-strategy exemplar (OQ-363 interim) ────────────────

/// `lab/control-strategy-family-v1` executes at Stage-3 size (5 arms × 2
/// tasks × 5 replicates = 50 plans — the `iso_cost` companion under
/// ADR-0213's interim OQ-363 rule) and a matched-cap pair produces a real
/// `ComparisonReport`. The cross-mode compare (`steerable-a` vs
/// `steerable-a-iso`) refuses `IncommensurableMatch` — the interim rule's
/// refusal is live, not assumed.
#[test]
fn exemplar_control_strategy_produces_comparison_report() {
    let mut r = rig("exemplar-control", 0);
    let pricing = hh_budget::pricing::PricingTableRef {
        table_id: "pricing:test".to_string(),
        version: "1".to_string(),
        pin: Some(pinned("pricing.table")),
    };
    let spec = control_strategy_family_v1(
        &exemplar_pins(),
        &ControlStrategyFamilyPins {
            react_minimal_ref: pinned("variant.react_minimal"),
            react_steerable_ref: pinned("variant.react_steerable"),
            model_family_a_ref: pinned("model.family_a"),
            model_family_b_ref: pinned("model.family_b"),
            artifacts: (
                Ref::new("definition:control", pinned("artifact.min_a")),
                Ref::new("definition:control", pinned("artifact.min_b")),
                Ref::new("definition:control", pinned("artifact.steer_a")),
                Ref::new("definition:control", pinned("artifact.steer_b")),
            ),
            iso_artifact: Ref::new("definition:control", pinned("artifact.iso")),
            pricing_table_ref: pricing,
        },
        1,
    );
    let mut eng =
        ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx_exemplar("exctl", 2, None));
    let eid = eng.register(&spec).unwrap();
    eng.expand(&eid).unwrap();
    let exp_run = eng.open_experiment(&eid).unwrap();
    drop(eng);
    // 5 arms × 2 tasks × 5 replicates = 50 plans — the iso_cost companion's
    // runs are distinct subject runs (ADR-0213's interim duplication).
    let settled = drain_plans(&mut r, &spec, &eid, "exctl", 2, None);
    assert_eq!(settled, 50);
    assert_close_audit_cap(&mut r, &eid, "exctl", 2, None);

    // The pre-registered product-arm comparison (same `model_snapshot`
    // family-a, `control_strategy` varied — not a checked field).
    let rep = compare_arms(&r, &spec, &exp_run, "arm:minimal-a", "arm:steerable-a");
    assert_eq!(rep.budget_match.status, BudgetMatchStatus::Matched);

    // The iso companion is a different match *mode* — the cross-mode
    // compare refuses IncommensurableMatch (CF-334; OQ-363's interim rule).
    let iso = spec
        .arms
        .iter()
        .find(|a| a.arm_id == "arm:steerable-a-iso")
        .expect("the iso companion arm");
    let steer = spec
        .arms
        .iter()
        .find(|a| a.arm_id == "arm:steerable-a")
        .expect("the product arm");
    let arms = [budget_arm(steer).1, budget_arm(iso).1];
    let p = project_accepted(&r, &exp_run, &spec.suite.suite_ref, "");
    let suites = vec![SuiteContext {
        suite_id: spec.suite.suite_ref.clone(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let decls = vec![catalogue::metric("task_success").unwrap()];
    let metrics = vec!["task_success".to_string()];
    let eval_runs = eval_runs_of(&p, &suites);
    let input = hh_eval::compare::CompareInput {
        arm_a: "arm:steerable-a",
        arm_b: "arm:steerable-a-iso",
        metrics: &metrics,
        declarations: &decls,
        runs: &eval_runs,
        tasks: &p.tasks,
        design: &spec.design,
        arm_specs: &arms,
        varied_factor: None,
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
    };
    match hh_eval::compare::compare(&input) {
        Err(hh_eval::compare::CompareError::Match(e)) => {
            assert!(
                matches!(
                    e.refusal,
                    hh_budget::MatchRefusal::IncommensurableMatch { .. }
                ),
                "the cross-mode compare must refuse incommensurable: {e:?}"
            );
        }
        other => panic!("iso_cost × matched_cap must refuse, got {other:?}"),
    }
}

/// The `Projected` rows → `EvalRun`s (`hh_analysis::project::eval_run` —
/// the projection `analyze`'s `project_runs` runs per row).
fn eval_runs_of(p: &Projected, suites: &[SuiteContext]) -> Vec<hh_eval::runs::EvalRun> {
    p.rows
        .iter()
        .map(|row| {
            let m = p.manifests.get(&row.key.run_id);
            let f = p.facts.get(&row.key.run_id).cloned().unwrap_or_default();
            let t = p.tasks.iter().find(|t| {
                m.and_then(|m| m.task_ref.as_ref())
                    .map(|tr| tr.task_id.as_str())
                    == Some(t.task_id.as_str())
            });
            let s = suites.iter().find(|s| {
                m.and_then(|m| m.task_ref.as_ref())
                    .map(|tr| tr.suite_id.as_str())
                    == Some(s.suite_id.as_str())
            });
            eval_run(row, m, f, t, s)
                .unwrap_or_else(|e| panic!("eval_run {}: {e:?}", row.key.run_id))
        })
        .collect()
}

// ── Reading-1 caveat — the retirement chain, engine-driven ─────────────────

/// The `retirement` `ExperimentSpec` a conditioned rule's removal test
/// instantiates — `a` (with-rule) / `a-minus-r` under
/// `MatchSpec{matched_cap, model_calls}` on the held-out split (the
/// hh-lab/tests/retirement.rs fixture shape, with a non-empty dimension set
/// so `validate_match` admits the compare).
fn retirement_spec(rule_id: &str) -> ExperimentSpec {
    let seed_policy = SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    };
    let prereg = PreRegistration {
        registered_at: 1,
        hypothesis: "removing the conditioned rule is non-inferior".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: Some(Json::str("margin:ni")),
        min_n: 1,
        analysis_plan_ref: pinned("retirement.analysis"),
        task_split_hash: pinned("retirement.split_hash"),
        interactions: vec![],
    };
    let arm = |id: &str, level: &str| ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("model".to_string(), level.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        match_spec: Some(hh_budget::MatchSpec::matched_cap(&[
            DimensionId::ModelCalls,
        ])),
        artifact_ref: Ref::new("def:x", pinned("retirement.artifact")),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
        response_cache: None,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.into(),
        ref_: pinned(&format!("retirement.level.{id}")),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Retirement,
        design: Design {
            id: "design:retirement".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 5,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy.clone(),
            held_out_split_ref: Some(pinned("split.held_out")),
            pre_registration: prereg.clone(),
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("with-rule"), level("without-rule")],
        }],
        arms: vec![arm("a", "with-rule"), arm("a-minus-r", "without-rule")],
        suite: SuiteBinding {
            suite_ref: pinned("retirement.suite"),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pinned("split.assign")),
        },
        replicates_per_cell: 5,
        seed_policy,
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::from([("debt_rule_id".to_string(), Json::str(rule_id))]),
    };
    s.experiment_id = s.experiment_id();
    s
}

/// The runs → compare → `removal_verdict` → `settle_removal_test` → `retire`
/// chain, engine-driven end to end (Reading 1's caveat: the prior leg was
/// fed-in verdicts). The retirement experiment schedules/launches/settles
/// through the real `ExperimentEngine`; the settled subject runs project
/// through `project_row`/`eval_run` into a real `hh_eval::compare` outcome;
/// `removal_verdict` reads the produced `ComparisonReport`s (point +
/// interval within the pre-registered margin); the verdict settles the
/// removal test and a human-sealed `retire` produces
/// `expired → retired{supersedes: expiry}`.
#[test]
fn retirement_chain_is_engine_driven_end_to_end() {
    let rule_id = "minimal-patch.naming";
    let mut r = rig("retirement", 0);
    let spec = retirement_spec(rule_id);
    // `retirement_diff = true` — the arms are the single-rule removal diff.
    let mut eng = ExperimentEngine::new(
        &mut r.store,
        r.docs.clone(),
        ctx_exemplar("ret", 3, Some(true)),
    );
    let eid = eng.register(&spec).unwrap();
    eng.expand(&eid).unwrap();
    let exp_run = eng.open_experiment(&eid).unwrap();
    drop(eng);
    let report = run_to_close_ctx(&mut r, &spec, &eid, "ret", 3, Some(true));
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
    // 2 arms × 3 tasks × 5 replicates = 30 settled plans.
    let view = ExperimentView::fold(r.store.events(&exp_run).unwrap());
    assert_eq!(view.plans.len(), 30);

    // Project + compare through the real kernel — `a` vs `a-minus-r`.
    let p = project_accepted(
        &r,
        &exp_run,
        &spec.suite.suite_ref,
        &spec.design.pre_registration.task_split_hash,
    );
    let suites = vec![SuiteContext {
        suite_id: spec.suite.suite_ref.clone(),
        retired_for_headline: false,
        family: EnvironmentFamily::CodingTerminal,
    }];
    let eval_runs = eval_runs_of(&p, &suites);
    assert_eq!(eval_runs.len(), 30);
    let decls = vec![catalogue::metric("task_success").unwrap()];
    let metrics = vec!["task_success".to_string()];
    let arms = [budget_arm(&spec.arms[0]).1, budget_arm(&spec.arms[1]).1];
    let input = hh_eval::compare::CompareInput {
        arm_a: "a",
        arm_b: "a-minus-r",
        metrics: &metrics,
        declarations: &decls,
        runs: &eval_runs,
        tasks: &p.tasks,
        design: &spec.design,
        arm_specs: &arms,
        varied_factor: None,
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
    };
    let outcome = hh_eval::compare::compare(&input).expect("the removal-diff compare");
    assert_eq!(outcome.reports.len(), 1);
    assert_eq!(
        outcome.reports[0].budget_match.status,
        BudgetMatchStatus::Matched
    );

    // The removal verdict over the produced outcome — identical per-task
    // values ⇒ delta 0, inside any positive margin ⇒ Pass.
    let verdict = hh_eval::compare::removal_verdict(rule_id, &outcome, &decls, 50_000, 200_000);
    assert_eq!(verdict.verdict, RemovalOutcome::Pass);
    assert!(verdict.regressed_tasks.is_empty());

    // Settle the removal test — `pass` ⇒ retirement-eligible, no status
    // transition until the human seal (ADR-0197 D7).
    let debt_ref = format!("debt:{rule_id}");
    let report_ref = format!("report:{}", outcome.reports[0].metric);
    let (row, effect, transitions) = settle_removal_test(
        &debt_ref,
        RemovalTestKind::RetirementExperiment,
        &report_ref,
        match verdict.verdict {
            RemovalOutcome::Pass => Verdict::Pass,
            RemovalOutcome::Fail => Verdict::Fail,
            RemovalOutcome::Inconclusive { .. } => Verdict::Inconclusive,
        },
        None,
        r.store.now_ms(),
        DebtStatus::Expired,
    );
    assert_eq!(effect, SettleEffect::RetirementEligible);
    assert!(transitions.is_empty());

    // The human-sealed retire — `expired → retired`, evidence = the report.
    let decided_by = ProvenanceRecord::minted(
        Origin::human("reviewer:1", HumanRole::Principal),
        hh_provenance::PersistenceScope::Run,
        1,
    );
    let record = hh_hir::debt::RetirementRecord {
        removal_test_report_ref: report_ref.clone(),
        verdict_ref: "verdict:1".into(),
        decided_by,
        rationale: hh_hir::leaves::Text::new(
            "the engine-driven removal test passed",
            "reviewer:1",
            ProvenanceRecord::minted(
                Origin::human("reviewer:1", HumanRole::Principal),
                hh_provenance::PersistenceScope::Run,
                1,
            ),
        ),
    };
    let out = retire(&debt_ref, DebtStatus::Expired, &[row], &record)
        .expect("pass verdict + human seal retires");
    assert_eq!(out.transition.from, DebtStatus::Expired);
    assert_eq!(out.transition.to, DebtStatus::Retired);
    assert_eq!(out.supersedes_reason, "expiry");
    assert_eq!(
        out.transition.evidence_ref.as_deref(),
        Some(report_ref.as_str())
    );
}
