//! R5.14 leg (a) — the **producer** battery (REQ-E-202/REQ-E-008
//! support-only; X7 SPLIT ruling `07b6be09`): `grade_pending` is an
//! admission state, never a seventh outcome class — the legs pin the
//! scheduler gates (`eligible_at`/`open_at`/`pending_plans`), close gates
//! (`CloseBlocked{grade_pending}` / partial enumeration), `settle_graded`
//! exactly-once, `record_selection_revision` validation,
//! `execution_settled` dedup, the `UnknownRequiredFeature` launch
//! refusal and the legacy-absent byte-identical binding. Consumer wiring
//! is seq 183a's scope. Fails-without: every leg compiles red at the
//! R5.13 base (the module/ops/members do not exist there).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_eval::selection::{PlanDisposition, SelectionRevision};
use hharness_experiment::disposition::DispositionPins;
use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{EngineContext, ExperimentEngine, NextVerdict};
use hharness_experiment::events::class;
use hharness_experiment::events::CloseStatus;
use hharness_identity::idp::idp_id;
use hharness_lab::expand::{ArmConfiguration, ExpandTask};
use hharness_lab::experiment::{ArmSpec, ExperimentSpec};
use hharness_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ontology::config::Ref;
use hharness_ontology::control::{OutcomeClass, StopReason};
use hharness_ontology::lab::SplitLabel;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

// ── fixture plumbing (mirrors tests/engine.rs) ──────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-r5-14a-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
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

/// The canonical `lab/compaction-family-v1` paired spec — 2 arms × 1 task
/// (the ctx task axis) × 5 replicates.
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

/// The frozen `ext.required_grade` member — `mode:"required"`,
/// `selection_policy_ref`, per-task `{oracle_ref, eligible[]}` pins.
fn required_ext() -> Json {
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
    ])
}

/// A spec with the required-grade freeze admitted.
fn required_spec() -> ExperimentSpec {
    let mut s = spec();
    s.ext.insert("required_grade".to_string(), required_ext());
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

fn eng(rig: &mut Rig) -> ExperimentEngine<'_> {
    ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx())
}

fn open(rig: &mut Rig, spec: &ExperimentSpec) -> (String, String) {
    let mut e = eng(rig);
    let eid = e.register(spec).unwrap();
    e.expand(&eid).unwrap();
    let run_id = e.open_experiment(&eid).unwrap();
    (eid, run_id)
}

fn mint(store: &Store, run_id: &str, class_name: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class_name.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("r5-14a-test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("r5-14a-test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// Append a batch, chaining parents (the batch-local known_ids rule).
fn append(store: &mut Store, run_id: &str, writer: &Lease, mut batch: Vec<Event>) {
    let mut parent = store.head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, writer, batch).unwrap();
}

/// A sealed subject finish — the `submission_seal` proof event plus
/// `lifecycle.run.finished{submission_seal:[{status:"sealed",…}]}` whose
/// `end_state_ref` is the admissible `submission_ref` pin.
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

/// An instrument run carrying one decided verdict for `subject_run`.
fn instrument_with_verdict(
    rig: &mut Rig,
    subject_run: &str,
    attempt_ref: &str,
    submission_ref: &str,
    outcome: Json,
) -> String {
    let (irun, ilease) = rig.store.open_run(manifest(), "instr").unwrap();
    let ev = mint(
        &rig.store,
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
    append(&mut rig.store, &irun, &ilease, vec![ev]);
    irun
}

fn decided(ppm: i64) -> Json {
    Json::obj([
        ("status", Json::str("decided")),
        ("reward_ppm", Json::Int(ppm)),
    ])
}

fn pins(irun: &str) -> DispositionPins {
    DispositionPins {
        experiment_until: None,
        overlay_runs: vec![(irun.to_string(), None)],
    }
}

/// Launch plan #1 of a required-mode experiment and settle its sealed
/// subject — returns `(experiment_id, run_id, run_plan_id, subject_run)`.
fn launch_pending(rig: &mut Rig) -> (String, String, String, String) {
    let s = required_spec();
    let (eid, run_id) = open(rig, &s);
    let mut e = eng(rig);
    e.attach(&eid).unwrap();
    let rpid = match e.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("expected a plan, got {other:?}"),
    };
    let ticket = e.claim(&rpid, "driver").unwrap();
    let launched = e.launch(&ticket, "subject").unwrap();
    let subject = launched.run_id.clone();
    drop(e);
    finish_subject_sealed(&mut rig.store, &subject, &launched.subject_writer, "snap-1");
    let mut e = eng(rig);
    e.attach(&eid).unwrap();
    let out = e.settle(&rpid).unwrap();
    assert!(
        out.grade_pending,
        "required-mode scored settle stays pending"
    );
    assert!(!out.accepted, "pending is not a scored success");
    (eid, run_id, rpid, subject)
}

fn events_of(store: &Store, run_id: &str, class_name: &str) -> Vec<EventEnvelope> {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class_name)
        .cloned()
        .collect()
}

// ── AC-2: launch admission — the typed refusal and the legacy zero ─────────

#[test]
fn legacy_absent_freezes_legacy_binding_byte_identical() {
    let mut r = rig("legacy", 0);
    let s = spec();
    let (eid, run_id) = open(&mut r, &s);
    let mut e = eng(&mut r);
    e.attach(&eid).unwrap();
    let rpid = match e.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = e.claim(&rpid, "driver").unwrap();
    e.launch(&ticket, "subject").unwrap();
    drop(e);
    // The frozen binding names `mode:"legacy"` and carries no required-mode
    // member — byte-for-byte the pre-R5.14 shape.
    let bound = events_of(&r.store, &run_id, class::LAUNCH_BOUND)
        .into_iter()
        .find(|e| e.payload.get("phase").and_then(Json::as_str) == Some("bound"))
        .expect("a bound launch row");
    let gb = bound.payload.get("grade_binding").expect("grade_binding");
    assert_eq!(gb.get("mode").and_then(Json::as_str), Some("legacy"));
    assert!(
        gb.get("selection_policy_ref").is_none(),
        "legacy binding carries no required-mode member"
    );
}

#[test]
fn unknown_required_feature_refuses_at_launch() {
    let mut r = rig("unknown-mode", 0);
    let mut s = spec();
    s.ext.insert(
        "required_grade".to_string(),
        Json::obj([
            ("mode", Json::str("bogus")),
            ("selection_policy_ref", Json::str("policy:p")),
            ("tasks", Json::obj([])),
        ]),
    );
    s.experiment_id = s.experiment_id();
    let (eid, _) = open(&mut r, &s);
    let mut e = eng(&mut r);
    e.attach(&eid).unwrap();
    let rpid = match e.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = e.claim(&rpid, "driver").unwrap();
    let err = e.launch(&ticket, "subject").unwrap_err();
    assert_eq!(err.code(), "UnknownRequiredFeature");
}

// ── AC-2: the producer-side pending gates ──────────────────────────────────

#[test]
fn pending_blocks_scheduler_and_counts_open() {
    let mut r = rig("sched-block", 0);
    let (eid, run_id, rpid, _) = launch_pending(&mut r);
    // The launch froze `mode:"required"` + `selection_policy_ref` durably.
    let bound = events_of(&r.store, &run_id, class::LAUNCH_BOUND)
        .into_iter()
        .find(|e| e.payload.get("phase").and_then(Json::as_str) == Some("bound"))
        .expect("a bound launch row");
    let gb = bound.payload.get("grade_binding").expect("grade_binding");
    assert_eq!(gb.get("mode").and_then(Json::as_str), Some("required"));
    assert_eq!(
        gb.get("selection_policy_ref").and_then(Json::as_str),
        Some("policy:first-valid")
    );
    let now = r.store.now_ms();
    let view = {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let view = e.project().unwrap();
        // Retries/re-offers: the durable rows are exactly-once — no second
        // settled/execution row, no replan, no bound — and the next
        // scheduling tick never re-offers the pending plan.
        let _ = e.settle(&rpid);
        let _ = e.settle(&rpid);
        match e.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => assert_ne!(run_plan_id, rpid),
            _ => {}
        }
        view
    };
    assert!(
        view.pending_plans().iter().any(|p| p.run_plan_id == rpid),
        "pending_plans enumerates the admission-pending plan"
    );
    let ps = view
        .pending_plans()
        .into_iter()
        .find(|p| p.run_plan_id == rpid)
        .unwrap();
    assert!(ps.open_at(now), "a pending plan is still an open slot");
    assert!(
        !view.eligible_at(now).iter().any(|p| p.run_plan_id == rpid),
        "grade_pending blocks relaunch — never eligible"
    );
    // The durable marks: `run_settled{grade_pending}` once +
    // `execution_settled{dedup_key}` once — the once-charged cost.
    let settled = events_of(&r.store, &run_id, class::RUN_SETTLED);
    assert_eq!(settled.len(), 1);
    assert!(matches!(
        settled[0].payload.get("grade_pending"),
        Some(Json::Bool(true))
    ));
    assert_eq!(
        events_of(&r.store, &run_id, class::EXECUTION_SETTLED).len(),
        1
    );
    // …and the retries minted no replan and no second bound (no relaunch).
    assert_eq!(events_of(&r.store, &run_id, class::RUN_REPLANNED).len(), 0);
    assert_eq!(events_of(&r.store, &run_id, class::RUN_BOUND).len(), 1);
}

#[test]
fn close_blocks_nonpartial_and_partial_enumerates_pending() {
    let mut r = rig("close-pending", 0);
    let (eid, _, rpid, _) = launch_pending(&mut r);
    let mut e = eng(&mut r);
    e.attach(&eid).unwrap();
    let err = e.close(false).unwrap_err();
    assert_eq!(err.code(), "CloseBlocked");
    match &err {
        hharness_experiment::errors::ExperimentError::CloseBlocked { detail } => {
            assert!(
                detail.contains(&rpid),
                "the refusal names the slot: {detail}"
            );
        }
        other => panic!("{other:?}"),
    }
    let report = e.close(true).unwrap();
    assert_eq!(report.status, CloseStatus::Partial);
    assert!(
        report.pending.contains(&rpid),
        "partial close enumerates unresolved slots"
    );
}

// ── AC-1: the admitted disposition through the producer callsite ───────────

#[test]
fn settle_graded_records_once_and_replays() {
    let mut r = rig("graded-once", 0);
    let (eid, run_id, rpid, subject) = launch_pending(&mut r);
    // A pending read is not an error and never writes the row.
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let read = e
            .settle_graded(&rpid, &DispositionPins::default())
            .expect("a pending read is not an error");
        assert!(matches!(
            read.disposition,
            Some(PlanDisposition::Pending { .. }) | None
        ));
    }
    assert_eq!(
        events_of(&r.store, &run_id, class::PLAN_DISPOSITION).len(),
        0,
        "pending never writes the disposition row"
    );
    let irun = instrument_with_verdict(&mut r, &subject, "att-1", "snap-1", decided(900_000));
    let p = pins(&irun);
    let (read, view) = {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let read = e.settle_graded(&rpid, &p).unwrap();
        // Replay — no second row, no recharge, no replan, n unchanged.
        let read2 = e.settle_graded(&rpid, &p).unwrap();
        assert_eq!(read2.disposition, read.disposition);
        (read, e.project().unwrap())
    };
    match &read.disposition {
        Some(PlanDisposition::Graded {
            outcome_class,
            selected_refs,
            ..
        }) => {
            assert_eq!(*outcome_class, OutcomeClass::Scored);
            assert_eq!(selected_refs.len(), 1, "first valid grade wins");
        }
        other => panic!("expected Graded, got {other:?}"),
    }
    assert_eq!(
        events_of(&r.store, &run_id, class::PLAN_DISPOSITION).len(),
        1
    );
    assert_eq!(events_of(&r.store, &run_id, class::RUN_SETTLED).len(), 1);
    assert_eq!(
        events_of(&r.store, &run_id, class::EXECUTION_SETTLED).len(),
        1
    );
    assert_eq!(events_of(&r.store, &run_id, class::RUN_REPLANNED).len(), 0);
    assert_eq!(events_of(&r.store, &run_id, class::RUN_BOUND).len(), 1);
    // The plan's admission is resolved — no longer pending, no longer open.
    let now = r.store.now_ms();
    assert!(view.pending_plans().is_empty());
    let ps = view
        .plans
        .values()
        .find(|p| p.run_plan_id == rpid)
        .expect("the plan");
    assert!(!ps.grade_pending);
    assert!(!ps.open_at(now), "a graded plan is a resolved slot");
}

#[test]
fn oracle_failure_disposition_is_final_not_pending() {
    let mut r = rig("oracle-final", 0);
    let (eid, run_id, rpid, subject) = launch_pending(&mut r);
    let irun = instrument_with_verdict(
        &mut r,
        &subject,
        "att-f",
        "snap-1",
        Json::obj([("status", Json::str("oracle_failure"))]),
    );
    let (read, view) = {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let read = e.settle_graded(&rpid, &pins(&irun)).unwrap();
        (read, e.project().unwrap())
    };
    assert!(
        matches!(
            read.disposition,
            Some(PlanDisposition::OracleFailure { .. })
        ),
        "all-unavailable unresolved members → oracle_failure, got {:?}",
        read.disposition
    );
    assert_eq!(
        events_of(&r.store, &run_id, class::PLAN_DISPOSITION).len(),
        1
    );
    assert!(
        view.pending_plans().is_empty(),
        "oracle_failure resolves the slot"
    );
}

#[test]
fn record_selection_revision_validates_and_lands() {
    let mut r = rig("revision", 0);
    let (eid, run_id, rpid, subject) = launch_pending(&mut r);
    let irun = instrument_with_verdict(&mut r, &subject, "att-1", "snap-1", decided(800_000));
    let iseq = r.store.head(&irun).unwrap().seq;
    let p = pins(&irun);
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let read = e.settle_graded(&rpid, &p).unwrap();
        let set_ref = read.eligible_set_ref.expect("the materialised set ref");
        let revision = SelectionRevision {
            revision_id: "rev-1".to_string(),
            reason: "correction".to_string(),
            supersedes: "att-1".to_string(),
            authority: "human:analyst".to_string(),
            watermarks: BTreeMap::from([(irun.clone(), iseq)]),
            eligible_set_ref: set_ref.clone(),
        };
        e.record_selection_revision(&rpid, &p, &revision).unwrap();
        // A revision bound to a different eligible set refuses — set-bound,
        // typed, never a silent append.
        let mut bad = revision.clone();
        bad.eligible_set_ref = "set:other".to_string();
        bad.revision_id = "rev-2".to_string();
        let err = e.record_selection_revision(&rpid, &p, &bad).unwrap_err();
        assert_ne!(err.code(), "UnknownRequiredFeature");
    }
    assert_eq!(
        events_of(&r.store, &run_id, class::SELECTION_REVISION).len(),
        1
    );
}
