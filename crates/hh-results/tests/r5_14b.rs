//! R5.14b — the **results consumer** battery of the required-grade pending
//! contract (REQ-E-202/REQ-E-008 discharging leg; X7 SPLIT ruling `07b6be09`
//! leg (b)): `scoring.experiment_run` composes the owner's
//! `plan_disposition_at` — a pending read withholds `outcome_class` and
//! stamps `outcome.disposition = "pending"`; the experiment pin enters
//! `watermark_set`; query refuses scored success (`outcome_class` filter
//! cannot match, `disposition` names the slot); export refuses
//! `foreign_leaderboard_submission` and lists `outcome.disposition` on
//! lowered targets; graded/oracle_failure dispositions restore the class;
//! an old-watermark projection reproduces the earlier selection;
//! legacy absence preserves bytes. Fails-without: `experiment_run`,
//! `disposition`, `Field::Disposition` and `DispositionUnresolved` do not
//! exist at the R5.14a base — every leg compiles red there.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_eval::selection::SelectionRevision;
use hharness_experiment::disposition::DispositionPins;
use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{EngineContext, ExperimentEngine, NextVerdict};
use hharness_experiment::events::class;
use hharness_identity::idp::idp_id;
use hharness_lab::expand::{ArmConfiguration, ExpandTask};
use hharness_lab::experiment::{ArmSpec, ExperimentSpec};
use hharness_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ontology::config::Ref;
use hharness_ontology::control::StopReason;
use hharness_ontology::lab::SplitLabel;
use hharness_provenance::ProvenanceRecord;
use hharness_results::error::ResultsError;
use hharness_results::export::{export_rows, ExportTarget};
use hharness_results::query::QuerySpec;
use hharness_results::scoring::{OverlayRef, ScoringContext};
use hharness_results::store::ResultsStore;
use hharness_results::version::DerivedReason;
use hharness_wire::json::Json;

// ── fixture plumbing (mirrors hh-experiment/tests/r5_14a.rs) ─────────────────

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-14b-results-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
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
    results: ResultsStore,
}

fn rig(tag: &str, now_ms: u64) -> Rig {
    let root = tmp(tag);
    let clock = ManualClock::at(now_ms);
    let store = Store::open_with(&root, Box::new(clock), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let results = ResultsStore::open(root.join("results")).unwrap();
    Rig {
        store,
        docs,
        results,
    }
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
        producer: Producer::kernel("r5-14b-results-test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "r5-14b-results-test",
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
    rig: &mut Rig,
    subject_run: &str,
    attempt_ref: &str,
    submission_ref: &str,
    outcome: Json,
) -> String {
    let (irun, ilease) = rig.store.open_run(manifest(), "instr").unwrap();
    let mut ev = mint(
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
    // The instrument verdict causally cites the subject's sealed finish —
    // the same citation the row's overlay-targeting fold requires (the
    // instrument run is the regrade overlay's admitted shape).
    ev.causes = vec![hharness_ledger::manifest::EventRef {
        run_id: subject_run.to_string(),
        event_id: rig.store.head_event_id(subject_run).unwrap(),
    }];
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
/// subject — `(experiment_id, experiment_run, run_plan_id, subject_run)`.
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
    (eid, run_id, rpid, subject)
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

fn events_of(store: &Store, run_id: &str, class_name: &str) -> Vec<EventEnvelope> {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class_name)
        .cloned()
        .collect()
}

// ── AC-2: the pending row withholds scored interpretation + query blocks ────

#[test]
fn pending_row_withholds_and_query_blocks_scored() {
    let mut r = rig("pending", 0);
    let (eid, exp_run, _rpid, subject) = launch_pending(&mut r);
    // The scheduler's own fold agrees — the plan sits in `pending_plans`.
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let view = e.project().unwrap();
        assert_eq!(view.pending_plans().len(), 1, "scheduler sees pending");
    }
    let sc = scoring(&exp_run, None);
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    // The scored-success interpretation is withheld — pending is an
    // admission state, never an outcome class.
    assert_eq!(row.outcome.status, "finished");
    assert_eq!(row.outcome.outcome_class, None);
    assert_eq!(row.outcome.disposition.as_deref(), Some("pending"));
    // The experiment pin enters the watermark set — the row records
    // exactly what the admission read consumed.
    assert!(row.watermark_set.get(&exp_run).is_some());
    // Query legs: a scored-success filter cannot match a pending row;
    // the declared `disposition` field names it.
    let key_id = row.key.key_id();
    r.results.record(&row, DerivedReason::Initial).unwrap();
    let page = r
        .results
        .query_rows(
            &r.store,
            &QuerySpec::all().filter("outcome_class", "scored").unwrap(),
        )
        .unwrap();
    assert!(
        page.rows.iter().all(|x| x.key.key_id() != key_id),
        "a pending row never answers outcome_class=scored"
    );
    let page = r
        .results
        .query_rows(
            &r.store,
            &QuerySpec::all().filter("disposition", "pending").unwrap(),
        )
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].outcome.disposition.as_deref(), Some("pending"));
}

// ── AC-2: export legs — foreign leaderboard refuses; lowered targets list ───

#[test]
fn export_refuses_pending_foreign_leaderboard() {
    let mut r = rig("export", 0);
    let (_eid, exp_run, _rpid, subject) = launch_pending(&mut r);
    let sc = scoring(&exp_run, None);
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    // A scored claim surface refuses a pending admission outright.
    let err = export_rows(
        &r.results,
        &mut r.store,
        &[row.clone()],
        ExportTarget::ForeignLeaderboardSubmission,
        &Json::Null,
    )
    .unwrap_err();
    assert!(
        matches!(err, ResultsError::LeaderboardRefused { .. }),
        "pending refuses the scored submission: {err:?}"
    );
    // Lowered targets list `outcome.disposition` as a `no_slot` member —
    // named in the loss report, never silently dropped.
    for target in [ExportTarget::InterchangeTable, ExportTarget::CsvLikeTable] {
        let out = export_rows(
            &r.results,
            &mut r.store,
            &[row.clone()],
            target,
            &Json::Null,
        )
        .unwrap();
        assert!(
            out.loss_report
                .entries
                .iter()
                .any(|e| e.field == "outcome.disposition" && e.detail.contains("pending")),
            "{target:?} lists the pending disposition as a loss entry"
        );
    }
    // The lossless native export keeps the row — pending is data, not a
    // hidden exclusion.
    let out = export_rows(
        &r.results,
        &mut r.store,
        &[row.clone()],
        ExportTarget::LedgerNativeRows,
        &Json::Null,
    )
    .unwrap();
    assert!(out.loss_report.entries.is_empty());
}

// ── AC-1: the graded/oracle_failure dispositions agree with the owner ───────

#[test]
fn graded_disposition_agrees_with_owner_and_scheduler() {
    let mut r = rig("graded", 0);
    let (eid, exp_run, rpid, subject) = launch_pending(&mut r);
    let irun = instrument_with_verdict(&mut r, &subject, "att-1", "snap-1", decided(900_000));
    let p = pins(&irun);
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        e.settle_graded(&rpid, &p).unwrap();
        let view = e.project().unwrap();
        assert!(view.pending_plans().is_empty(), "disposition resolved");
    }
    // The recorded owner row is the agreement reference (step 6).
    let recorded = events_of(&r.store, &exp_run, class::PLAN_DISPOSITION);
    assert_eq!(recorded.len(), 1);
    let payload = &recorded[0].payload;
    assert_eq!(
        payload.get("disposition").and_then(Json::as_str),
        Some("graded")
    );
    let sc = scoring(&exp_run, Some(&irun));
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    // step-6 agreement — owner read, scheduler fold, results row agree.
    assert_eq!(row.outcome.disposition.as_deref(), Some("graded"));
    assert_eq!(
        row.outcome.outcome_class.as_deref(),
        payload.get("outcome_class").and_then(Json::as_str)
    );
    assert_eq!(row.outcome.outcome_class.as_deref(), Some("scored"));
    // The valid grade exports — `disposition` lists as a member on
    // lowered targets, the submission itself is admitted.
    let out = export_rows(
        &r.results,
        &mut r.store,
        &[row.clone()],
        ExportTarget::ForeignLeaderboardSubmission,
        &Json::Null,
    )
    .unwrap();
    assert!(out
        .loss_report
        .entries
        .iter()
        .any(|e| e.field == "outcome.disposition" && e.detail.contains("graded")));
}

#[test]
fn oracle_failure_disposition_projects_final() {
    let mut r = rig("oracle", 0);
    let (eid, exp_run, rpid, subject) = launch_pending(&mut r);
    let irun = instrument_with_verdict(
        &mut r,
        &subject,
        "att-f",
        "snap-1",
        Json::obj([("status", Json::str("oracle_failure"))]),
    );
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        e.settle_graded(&rpid, &pins(&irun)).unwrap();
    }
    let sc = scoring(&exp_run, Some(&irun));
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(row.outcome.disposition.as_deref(), Some("oracle_failure"));
    assert_eq!(row.outcome.outcome_class.as_deref(), Some("oracle_failure"));
}

// ── AC-1: old-watermark reproduction ────────────────────────────────────────

#[test]
fn old_watermark_reproduces_the_earlier_selection() {
    let mut r = rig("watermark", 0);
    let (eid, exp_run, rpid, subject) = launch_pending(&mut r);
    let irun = instrument_with_verdict(&mut r, &subject, "att-1", "snap-1", decided(800_000));
    let iseq = r.store.head(&irun).unwrap().seq;
    let p = pins(&irun);
    let set_ref = {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let read = e.settle_graded(&rpid, &p).unwrap();
        read.eligible_set_ref.expect("materialised set ref")
    };
    // The watermark BEFORE the authorized revision lands — the report
    // pins it up front so the reproduction check diffs identical scoring.
    let before_rev = r.store.head(&exp_run).unwrap().seq;
    let mut sc = scoring(&exp_run, Some(&irun));
    sc.experiment_run = Some(OverlayRef {
        run_id: exp_run.clone(),
        until_seq: Some(before_rev),
    });
    let row_before = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(row_before.outcome.disposition.as_deref(), Some("graded"));
    // An authorized revision supersedes the winning attempt.
    {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        e.record_selection_revision(
            &rpid,
            &p,
            &SelectionRevision {
                revision_id: "rev-1".to_string(),
                reason: "correction".to_string(),
                supersedes: "att-1".to_string(),
                authority: "human:analyst".to_string(),
                watermarks: BTreeMap::from([(irun.clone(), iseq)]),
                eligible_set_ref: set_ref,
            },
        )
        .unwrap();
    }
    // The head read now applies the revision — superseded evidence is
    // unavailable until replaced (no pending, no silent erase).
    let mut sc_head = scoring(&exp_run, Some(&irun));
    sc_head.experiment_run = Some(OverlayRef {
        run_id: exp_run.clone(),
        until_seq: None,
    });
    let row_head = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc_head))
        .unwrap();
    assert_ne!(
        row_head.outcome.disposition.as_deref(),
        Some("graded"),
        "the revised selection moved"
    );
    // The report pinned at the OLD watermark reproduces the earlier
    // selection — the appended revision is above the pin, so the row is
    // byte-identical (same disposition, outcome and version).
    let row_old = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(row_old.outcome.disposition.as_deref(), Some("graded"));
    assert_eq!(row_old.outcome.outcome_class.as_deref(), Some("scored"));
    assert_eq!(row_old.version_id, row_before.version_id);
}

// ── AC-2: typed refusals + legacy byte-identity ──────────────────────────────

#[test]
fn experiment_pin_refusals_are_typed() {
    let mut r = rig("refusals", 0);
    let (_eid, exp_run, _rpid, subject) = launch_pending(&mut r);
    // The read needs the LabDocs resolver.
    let sc = scoring(&exp_run, None);
    let err = r
        .results
        .project_row(&r.store, None, &subject, None, Some(&sc))
        .unwrap_err();
    assert!(matches!(err, ResultsError::DispositionUnresolved { .. }));
    // A pin naming an experiment run that binds no plan to this subject.
    let (other_run, other_lease) = r.store.open_run(manifest(), "other").unwrap();
    let ev = mint(
        &r.store,
        &other_run,
        "measurement.experiment.declared",
        Json::obj([]),
    );
    append(&mut r.store, &other_run, &other_lease, vec![ev]);
    let sc = scoring(&other_run, None);
    let err = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap_err();
    assert!(matches!(err, ResultsError::DispositionUnresolved { .. }));
}

#[test]
fn legacy_absent_freeze_is_byte_identical() {
    let mut r = rig("legacy", 0);
    // No `required_grade` member — the binding freezes `mode:"legacy"`.
    let s = spec();
    let (eid, exp_run) = open(&mut r, &s);
    let subject = {
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let rpid = match e.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => run_plan_id,
            other => panic!("{other:?}"),
        };
        let ticket = e.claim(&rpid, "driver").unwrap();
        let launched = e.launch(&ticket, "subject").unwrap();
        let subject = launched.run_id.clone();
        drop(e);
        finish_subject_sealed(&mut r.store, &subject, &launched.subject_writer, "snap-1");
        let mut e = eng(&mut r);
        e.attach(&eid).unwrap();
        let out = e.settle(&rpid).unwrap();
        assert!(!out.grade_pending, "legacy settles scored directly");
        subject
    };
    // No experiment pin — the row carries no disposition member at all.
    let plain = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, None)
        .unwrap();
    assert_eq!(plain.outcome.disposition, None);
    assert_eq!(plain.outcome.outcome_class.as_deref(), Some("scored"));
    assert!(!plain
        .to_json()
        .get("outcome")
        .unwrap()
        .to_canonical_string()
        .contains("disposition"));
    // An experiment pin under the legacy binding resolves `mode:"legacy"`
    // — the outcome is unchanged; only the recorded pin differs.
    let sc = scoring(&exp_run, None);
    let pinned_row = r
        .results
        .project_row(&r.store, Some(&r.docs), &subject, None, Some(&sc))
        .unwrap();
    assert_eq!(pinned_row.outcome.disposition, None);
    assert_eq!(pinned_row.outcome.outcome_class.as_deref(), Some("scored"));
    assert!(pinned_row.watermark_set.get(&exp_run).is_some());
}
