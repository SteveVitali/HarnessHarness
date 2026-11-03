//! R5.14b — the **analysis consumer** battery (REQ-E-202/REQ-E-008
//! discharging leg; X7 SPLIT `07b6be09` leg (b)): `lab.analysis.analyze`
//! refuses scored analysis over a row whose `outcome.disposition` is the
//! `grade_pending` admission state — the same `plan_disposition_at` read
//! every consumer composes, never a divergent predicate; graded/legacy
//! rows never trip the gate. Fails-without: the pending check and the
//! row's `disposition` member do not exist at the R5.14a base — this
//! file compiles red there.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{EngineContext, ExperimentEngine, NextVerdict};
use hharness_identity::idp::idp_id;
use hharness_lab::expand::{ArmConfiguration, ExpandTask};
use hharness_lab::experiment::{ArmSpec, ExperimentSpec};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ontology::config::Ref;
use hharness_ontology::control::StopReason;
use hharness_ontology::lab::SplitLabel;
use hharness_provenance::ProvenanceRecord;
use hharness_results::scoring::{OverlayRef, ScoringContext};
use hharness_results::store::ResultsStore;
use hharness_results::version::DerivedReason;
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

// ── fixture plumbing (mirrors hh-experiment/tests/r5_14a.rs) ─────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-r5-14b-embed-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

fn budgets() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("eval:a".to_string(), caps(100)),
        ("search:a".to_string(), caps(50)),
        ("pool".to_string(), caps(1_000)),
        ("instr".to_string(), caps(1_000)),
    ])
}

fn exemplar_pins() -> hharness_lab::exemplars::ExemplarPins {
    hharness_lab::exemplars::ExemplarPins {
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

fn spec() -> ExperimentSpec {
    hharness_lab::exemplars::compaction_family_v1(
        &exemplar_pins(),
        &hharness_lab::exemplars::CompactionFamilyPins {
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
    )
}

fn required_spec() -> ExperimentSpec {
    let mut s = spec();
    s.ext.insert(
        "required_grade".to_string(),
        Json::obj([
            ("mode", Json::str("required")),
            ("selection_policy_ref", Json::str("policy:first-valid")),
            (
                "tasks",
                Json::obj([(
                    "task:1",
                    Json::obj([
                        ("oracle_ref", Json::str("oracle:o1")),
                        ("eligible", Json::Arr(vec![Json::str("metric:m1")])),
                    ]),
                )]),
            ),
        ]),
    );
    s.experiment_id = s.experiment_id();
    s
}

fn ctx() -> EngineContext<'static> {
    let map = budgets();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| map.get(r).cloned())),
        suite_tasks: Some(Box::new(|_spec: &ExperimentSpec| {
            Some(vec![ExpandTask {
                task_id: "task:1".to_string(),
                split_label: SplitLabel::HeldOut,
            }])
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
                budget_ref: None,
            })
        })),
        min_replicates: hharness_lab::exemplars::EXEMPLAR_REPLICATES,
        ..Default::default()
    }
}

fn mint(store: &Store, run_id: &str, class_name: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class_name.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("r5-14b-embed-test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "r5-14b-embed-test",
            store.now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

fn append(store: &mut Store, run_id: &str, writer: &Lease, mut batch: Vec<Event>) {
    let mut parent = store.head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, writer, batch).unwrap();
}

fn finish_subject_sealed(store: &mut Store, run_id: &str, writer: &Lease, end_state_ref: &str) {
    let batch = vec![
        mint(
            store,
            run_id,
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str(DimensionId::ModelCalls.as_str())),
                ("amount", Json::Int(40)),
            ]),
        ),
        mint(
            store,
            run_id,
            "action.environment.snapshot",
            Json::obj([
                ("purpose", Json::str("submission_seal")),
                ("snapshot_ref", Json::str(end_state_ref)),
            ]),
        ),
        mint(
            store,
            run_id,
            "lifecycle.run.finished",
            Json::obj([
                ("stop_reason", StopReason::Completed.to_json()),
                ("outcome_class", Json::str("scored")),
                (
                    "submission_seal",
                    Json::Arr(vec![Json::obj([
                        ("env_handle", Json::str("h1")),
                        ("status", Json::str("sealed")),
                        ("end_state_ref", Json::str(end_state_ref)),
                    ])]),
                ),
            ]),
        ),
    ];
    append(store, run_id, writer, batch);
}

fn manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Experiment);
    m.configuration_id = None;
    m.configuration_version_id = None;
    m
}

fn instrument_with_verdict(
    store: &mut Store,
    subject_run: &str,
    attempt_ref: &str,
    submission_ref: &str,
    outcome: Json,
) -> String {
    let (irun, ilease) = store.open_run(manifest(), "instr").unwrap();
    let mut ev = mint(
        store,
        &irun,
        "verification.oracle.verdict",
        Json::obj([
            ("invocation_ref", Json::str(&format!("{irun}:evt-inv-1"))),
            ("subject_run", Json::str(subject_run)),
            ("submission_ref", Json::str(submission_ref)),
            ("oracle_ref", Json::str("oracle:o1")),
            ("attempt_ref", Json::str(attempt_ref)),
            ("outcome", outcome),
        ]),
    );
    ev.causes = vec![hharness_ledger::manifest::EventRef {
        run_id: subject_run.to_string(),
        event_id: store.head_event_id(subject_run).unwrap(),
    }];
    append(store, &irun, &ilease, vec![ev]);
    irun
}

/// A rig at `root`: required-mode experiment, one plan settled into
/// `grade_pending`; `(experiment_id, experiment_run, run_plan_id, subject)`.
fn rig_pending(root: &PathBuf) -> (Store, LabDocs, ResultsStore, String, String, String, String) {
    let store = Store::open_with(
        root,
        Box::new(ManualClock::at(0)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let docs = LabDocs::open(root).unwrap();
    let results = ResultsStore::open(root.join("results")).unwrap();
    let mut store = store;
    let (eid, exp_run, rpid, subject) = {
        let s = required_spec();
        let (eid, exp_run) = {
            let mut e = ExperimentEngine::new(&mut store, docs.clone(), ctx());
            let eid = e.register(&s).unwrap();
            e.expand(&eid).unwrap();
            let exp_run = e.open_experiment(&eid).unwrap();
            (eid, exp_run)
        };
        let (rpid, launched) = {
            let mut e = ExperimentEngine::new(&mut store, docs.clone(), ctx());
            e.attach(&eid).unwrap();
            let rpid = match e.next().unwrap() {
                NextVerdict::Plan { run_plan_id } => run_plan_id,
                other => panic!("{other:?}"),
            };
            let ticket = e.claim(&rpid, "driver").unwrap();
            let launched = e.launch(&ticket, "subject").unwrap();
            (rpid, launched)
        };
        let subject = launched.run_id.clone();
        finish_subject_sealed(&mut store, &subject, &launched.subject_writer, "snap-1");
        {
            let mut e = ExperimentEngine::new(&mut store, docs.clone(), ctx());
            e.attach(&eid).unwrap();
            assert!(e.settle(&rpid).unwrap().grade_pending);
        }
        (eid, exp_run, rpid, subject)
    };
    (store, docs, results, eid, exp_run, rpid, subject)
}

fn scoring(exp_run: &str, irun: Option<&str>) -> ScoringContext {
    let mut s = ScoringContext::native();
    s.experiment_run = Some(OverlayRef {
        run_id: exp_run.to_string(),
        until_seq: None,
    });
    if let Some(r) = irun {
        s.overlay_runs.push(OverlayRef {
            run_id: r.to_string(),
            until_seq: None,
        });
    }
    s
}

fn svc(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.to_path_buf(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.to_path_buf(),
        holder: "r5-14b-analysis".to_string(),
    })
    .unwrap()
}

fn hello(s: &mut EmbedService) {
    let r = call(
        s,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r5-14b")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn call(s: &mut EmbedService, method: &str, params: Json) -> Json {
    s.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn analysis_params(rows: &[String]) -> Json {
    let mut aspec = hharness_lab::analysis::AnalysisSpec {
        spec_id: String::new(),
        kind: "summarize".into(),
        query: hharness_lab::analysis::QuerySpec {
            metrics: vec![],
            filters: None,
            grain: None,
        },
        spec_ref: None,
        label: None,
        estimator_selection: hharness_ontology::eval::EstimatorSelection {
            method: hharness_ontology::eval::IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    aspec.spec_id = aspec.spec_id();
    Json::obj([
        ("spec", aspec.to_json()),
        ("rows", Json::Arr(rows.iter().map(Json::str).collect())),
        ("tasks", Json::Arr(vec![])),
    ])
}

// AC-2 — a pending row refuses scored analysis (never a silent
// exclusion/downgrade); the refusal names the unresolved admission.
#[test]
fn analysis_refuses_over_pending_rows() {
    let root = tmp("pending");
    let (store, docs, results, _eid, exp_run, _rpid, subject) = rig_pending(&root);
    let sc = scoring(&exp_run, None);
    let row = results
        .project_row(&store, Some(&docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(row.outcome.disposition.as_deref(), Some("pending"));
    let key = row.key.key_id();
    results.record(&row, DerivedReason::Initial).unwrap();
    let mut s = svc(&root);
    hello(&mut s);
    let r = call(&mut s, "lab.analysis.analyze", analysis_params(&[key]));
    let err = r.get("error").expect("a pending row must refuse");
    assert_eq!(
        err.get("data")
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str),
        Some("Refused"),
        "expected a typed Refused surface: {r:?}"
    );
    let reason = err
        .get("data")
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(
        reason.contains("grade_pending"),
        "the refusal names the admission state: {r:?}"
    );
}

// AC-2 — a graded row (the resolved admission) and a legacy row (no
// required freeze — byte-identical behavior) never trip the pending
// gate: whatever the analysis returns downstream, it is not the
// `grade_pending` refusal.
#[test]
fn non_pending_rows_never_trip_the_gate() {
    let root = tmp("nonpending");
    let (mut store, docs, results, eid, exp_run, rpid, subject) = rig_pending(&root);
    // Resolve the admission — a decided verdict → `graded` disposition.
    let irun = instrument_with_verdict(
        &mut store,
        &subject,
        "att-1",
        "snap-1",
        Json::obj([
            ("status", Json::str("decided")),
            ("reward_ppm", Json::Int(900_000)),
        ]),
    );
    {
        let mut e = ExperimentEngine::new(&mut store, docs.clone(), ctx());
        e.attach(&eid).unwrap();
        e.settle_graded(
            &rpid,
            &hharness_experiment::disposition::DispositionPins {
                experiment_until: None,
                overlay_runs: vec![(irun.clone(), None)],
            },
        )
        .unwrap();
    }
    let sc = scoring(&exp_run, Some(&irun));
    let row = results
        .project_row(&store, Some(&docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(row.outcome.disposition.as_deref(), Some("graded"));
    let key = row.key.key_id();
    results.record(&row, DerivedReason::Initial).unwrap();
    let mut s = svc(&root);
    hello(&mut s);
    let r = call(&mut s, "lab.analysis.analyze", analysis_params(&[key]));
    if let Some(err) = r.get("error") {
        let reason = err
            .get("data")
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str)
            .unwrap_or("");
        assert!(
            !reason.contains("grade_pending"),
            "a graded row never trips the pending gate: {r:?}"
        );
    }
}
