//! R5.11 (REQ-E-007; partition X5; PADR-R5-1 A.1 item 5 + A.3) — the
//! instrument oracle-attempt battery over scratch instrument/subject runs:
//! invocation-before-execution on the sealed submission, frozen
//! instrument-only regrade/time/resource bounds, retained attempts through
//! the unchanged reward/infra mapping, and the AC-2 negative set.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_bench::benchset::Benchset;
use hharness_bench::oracle::{
    AttemptOutcome, AttemptSpec, InstrumentPolicy, OracleExecutor, OracleObservation, OracleRunner,
    SealedSubmission, ORACLE_INVOKED, ORACLE_VERDICT, SEAL_CLASS, SEAL_PURPOSE,
};
use hharness_bench::records::Submission;
use hharness_bench::OracleError;
use hharness_budget::attribution::ChargedTo;
use hharness_eval::selection::GradeOutcome;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::eval::MetricValueKind;
use hharness_ontology::lab::VerifierIsolation;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::Json;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("r5-11-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Experiment);
    // Non-agent runs carry no configuration cells (ADR-0183 §C) — the
    // instrument-run idiom the results tests use.
    m.configuration_id = None;
    m.configuration_version_id = None;
    m
}

fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("r5-11-test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("r5-11-test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// A rig: scratch store, one subject run (the sealed workspace's owner) and
/// one instrument run (charged_to = instrument; never the subject writer).
struct Rig {
    store: Store,
    subject: String,
    subject_lease: Lease,
    instrument: String,
    instrument_lease: Lease,
}

fn rig(tag: &str) -> Rig {
    let root = tmp(tag);
    let mut store = Store::open_test(&root, 42_000).expect("scratch store opens");
    let (subject, subject_lease) = store
        .open_run(manifest(), "r5-11-subject")
        .expect("subject run opens");
    let (instrument, instrument_lease) = store
        .open_run(manifest(), "r5-11-instrument")
        .expect("instrument run opens");
    Rig {
        store,
        subject,
        subject_lease,
        instrument,
        instrument_lease,
    }
}

/// Land the R5.10 `submission_seal` end-state pin on the subject run and
/// return its `EventRef` (the seal proof every attempt cites).
fn seal(rig: &mut Rig, handle: &str, snapshot_ref: &str) -> EventRef {
    let e = mint(
        &rig.store,
        &rig.subject,
        SEAL_CLASS,
        Json::obj([
            ("env_handle", Json::str(handle)),
            ("snapshot_ref", Json::str(snapshot_ref)),
            ("purpose", Json::str(SEAL_PURPOSE)),
        ]),
    );
    let id = e.event_id.clone();
    rig.store
        .append(&rig.subject, &rig.subject_lease, vec![e])
        .expect("seal lands");
    EventRef {
        run_id: rig.subject.clone(),
        event_id: id,
    }
}

fn task() -> hharness_lab::bench::TaskRecord {
    Benchset::load_default()
        .expect("committed benchset loads")
        .suites
        .values()
        .next()
        .and_then(|s| s.tasks.first())
        .map(|t| t.record().clone())
        .expect("a corpus task record")
}

fn submission(task: &hharness_lab::bench::TaskRecord, payload_hex: &str) -> Submission {
    let mut s = Submission {
        submission_id: String::new(),
        task_id: task.task_id.clone(),
        artifact_refs: vec!["sha256:aa".repeat(1)],
        payload_hex: payload_hex.to_string(),
        applied: true,
        apply_error: None,
    };
    s.submission_id = s.compute_id();
    s
}

fn policy(max_attempts: u32) -> InstrumentPolicy {
    InstrumentPolicy {
        policy_ref: "policy/instrument-1".to_string(),
        max_attempts,
        wall_ms_bound: 5_000,
        output_bytes_bound: 64 * 1024,
    }
}

fn spec(
    subject: &str,
    task: &hharness_lab::bench::TaskRecord,
    sub_ref: &str,
    oracle_digest: &str,
    isolation: VerifierIsolation,
) -> AttemptSpec {
    AttemptSpec {
        subject_run_id: subject.to_string(),
        submission_ref: sub_ref.to_string(),
        oracle: hharness_bench::OraclePin {
            oracle_ref: task.instrument.grader.adapter_ref.clone(),
            oracle_digest: oracle_digest.to_string(),
        },
        policy: policy(3),
        instrument_budget_ref: "budget/instrument".to_string(),
        task: task.clone(),
        isolation,
    }
}

/// The deterministic executor fixture — reports a pinned digest and a
/// scripted observation; `executed` records that the oracle actually ran.
struct FixtureExec {
    digest: String,
    obs: OracleObservation,
    executed: Cell<bool>,
}

impl FixtureExec {
    fn new(digest: &str, obs: OracleObservation) -> Self {
        FixtureExec {
            digest: digest.to_string(),
            obs,
            executed: Cell::new(false),
        }
    }
}

const OK: fn() -> OracleObservation = || OracleObservation {
    exit_code: Some(0),
    timed_out: false,
    stdout: "{\"reward\": 1000000}".to_string(),
    stderr: String::new(),
    environment_error: None,
    wall_ms: 12,
};

impl OracleExecutor for FixtureExec {
    fn oracle_digest(&self) -> &str {
        &self.digest
    }
    fn execute(
        &self,
        _task: &hharness_lab::bench::TaskRecord,
        _submission: &Submission,
    ) -> OracleObservation {
        self.executed.set(true);
        self.obs.clone()
    }
}

fn sealed(seal_ref: EventRef, s: Submission) -> SealedSubmission {
    SealedSubmission {
        submission_ref: s.compute_id(),
        seal_ref,
        submission: s,
    }
}

fn oracle_events<'a>(rig: &'a Rig, class: &str) -> Vec<&'a hharness_ledger::event::EventEnvelope> {
    rig.store
        .events(&rig.instrument)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .collect()
}

#[test]
fn invocation_persists_before_execution_and_verdict_retains() {
    let mut rig = rig("positive");
    let task = task();
    let sub = submission(&task, "ff00aa");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let outcome = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref, sub.clone()), &exec)
            .expect("the attempt executes and retains")
    };
    assert!(exec.executed.get(), "the pinned oracle ran");
    // Invocation before verdict — dense durable ordering on the instrument run.
    let invoked = oracle_events(&rig, ORACLE_INVOKED);
    let verdict = oracle_events(&rig, ORACLE_VERDICT);
    assert_eq!(invoked.len(), 1);
    assert_eq!(verdict.len(), 1);
    assert!(invoked[0].seq < verdict[0].seq);
    // The invoked row binds subject, submission (+digest/seal), oracle
    // (+digest), attempt, policy and instrument funding (A.3).
    let p = &invoked[0].payload;
    assert_eq!(
        p.get("subject_run").and_then(Json::as_str),
        Some(rig.subject.as_str())
    );
    assert_eq!(
        p.get("submission_ref").and_then(Json::as_str),
        Some(sub.compute_id().as_str())
    );
    assert_eq!(
        p.get("oracle_ref").and_then(Json::as_str),
        Some(task.instrument.grader.adapter_ref.as_str())
    );
    assert_eq!(p.get("attempt_no").and_then(Json::as_int), Some(1));
    assert_eq!(
        p.get("policy_ref").and_then(Json::as_str),
        Some("policy/instrument-1")
    );
    assert_eq!(
        p.get("instrument_budget_ref").and_then(Json::as_str),
        Some("budget/instrument")
    );
    assert_eq!(
        p.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    // The verdict retains actual output/status/timeout + isolation evidence.
    let v = &verdict[0].payload;
    assert_eq!(
        v.pointer_outcome("status"),
        Some("decided"),
        "sanctioned correct decides"
    );
    let ex = v.get("execution").unwrap();
    assert_eq!(ex.get("exit_code").and_then(Json::as_int), Some(0));
    assert_eq!(ex.get("timed_out"), Some(&Json::Bool(false)));
    assert!(ex
        .get("stdout")
        .and_then(Json::as_str)
        .unwrap()
        .contains("reward"));
    assert_eq!(
        ex.get("stdout_digest").and_then(Json::as_str),
        Some(
            hharness_identity::idp_id("oracle.stdout", "{\"reward\": 1000000}".as_bytes()).as_str()
        )
    );
    let iso = v.get("isolation").unwrap();
    assert_eq!(iso.get("mode").and_then(Json::as_str), Some("separate"));
    assert_eq!(iso.get("separate_verifier"), Some(&Json::Bool(true)));
    assert!(
        iso.get("tamper").is_none(),
        "separate never spells no_detector"
    );
    assert_eq!(
        v.get("cost_ref").and_then(Json::as_str),
        Some("budget/instrument")
    );
    match outcome.outcome {
        AttemptOutcome::Decided { reward_ppm, .. } => assert_eq!(reward_ppm, 1_000_000),
        _ => panic!("correct oracle decides"),
    }
}

trait PointerOutcome {
    fn pointer_outcome(&self, member: &str) -> Option<&str>;
}
impl PointerOutcome for Json {
    fn pointer_outcome(&self, member: &str) -> Option<&str> {
        self.get("outcome")?.get(member)?.as_str()
    }
}

#[test]
fn wrong_and_broken_controls_diverge() {
    let mut rig = rig("controls");
    let task = task();
    // Sanctioned correct / plausible wrong / broken oracle must diverge.
    for (name, obs, want) in [
        ("correct", OK(), "decided"),
        (
            "wrong",
            OracleObservation {
                exit_code: Some(0),
                timed_out: false,
                stdout: "{\"reward\": 0}".to_string(),
                stderr: String::new(),
                environment_error: None,
                wall_ms: 10,
            },
            "zero",
        ),
        (
            "broken",
            OracleObservation {
                exit_code: None,
                timed_out: false,
                stdout: String::new(),
                stderr: "panic: grader crashed".to_string(),
                environment_error: None,
                wall_ms: 3,
            },
            "oracle_failure",
        ),
    ] {
        let sub = submission(&task, name);
        let seal_ref = seal(&mut rig, "env-1", &format!("snap-{name}"));
        let exec = FixtureExec::new("sha256:oracle-1", obs);
        let attempt = {
            let mut runner = OracleRunner::new(
                &mut rig.store,
                &rig.instrument,
                &rig.instrument_lease,
                spec(
                    &rig.subject,
                    &task,
                    &sub.compute_id(),
                    "sha256:oracle-1",
                    VerifierIsolation::Separate,
                ),
            );
            runner
                .attempt(&sealed(seal_ref, sub.clone()), &exec)
                .unwrap_or_else(|e| panic!("{name} attempt: {e}"))
        };
        match (want, &attempt.outcome) {
            (
                "decided",
                AttemptOutcome::Decided {
                    reward_ppm,
                    verdict,
                },
            ) => {
                assert_eq!(*reward_ppm, 1_000_000);
                assert_eq!(verdict.as_str(), "N");
            }
            (
                "zero",
                AttemptOutcome::Decided {
                    reward_ppm,
                    verdict,
                },
            ) => {
                // Plausible wrong with an existing artifact is a valid zero —
                // never an oracle failure.
                assert_eq!(*reward_ppm, 0);
                assert_eq!(verdict.as_str(), "C");
            }
            ("oracle_failure", AttemptOutcome::OracleFailure { detail }) => {
                assert!(detail.contains("VerifierFailed"), "{detail}");
            }
            (w, o) => panic!("{name}: wanted {w}, got {o:?}"),
        }
    }
}

#[test]
fn empty_nonfinite_crashed_timedout_refused_are_oracle_failure() {
    let mut rig = rig("oracle-failures");
    let task = task();
    let cases: Vec<(&str, OracleObservation)> = vec![
        (
            "empty",
            OracleObservation {
                stdout: String::new(),
                stderr: String::new(),
                ..OK()
            },
        ),
        (
            "non-numeric",
            OracleObservation {
                stdout: "{\"reward\": \"lots\"}".into(),
                ..OK()
            },
        ),
        (
            "out-of-range",
            OracleObservation {
                stdout: "{\"reward\": 2000000}".into(),
                ..OK()
            },
        ),
        (
            "unparseable",
            OracleObservation {
                stdout: "not json".into(),
                ..OK()
            },
        ),
        (
            "timed-out",
            OracleObservation {
                timed_out: true,
                wall_ms: 9_999,
                ..OK()
            },
        ),
        (
            "over-wall-bound",
            OracleObservation {
                timed_out: false,
                wall_ms: 9_999,
                ..OK()
            },
        ),
        (
            "refused",
            OracleObservation {
                exit_code: Some(2),
                stderr: "refused submission".into(),
                ..OK()
            },
        ),
    ];
    let mut n = 0;
    for (name, obs) in cases {
        n += 1;
        let sub = submission(&task, &format!("case-{n}"));
        let seal_ref = seal(&mut rig, "env-1", &format!("snap-{name}"));
        let exec = FixtureExec::new("sha256:oracle-1", obs);
        let attempt = {
            let mut runner = OracleRunner::new(
                &mut rig.store,
                &rig.instrument,
                &rig.instrument_lease,
                spec(
                    &rig.subject,
                    &task,
                    &sub.compute_id(),
                    "sha256:oracle-1",
                    VerifierIsolation::Separate,
                ),
            );
            runner
                .attempt(&sealed(seal_ref, sub.clone()), &exec)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        match &attempt.outcome {
            AttemptOutcome::OracleFailure { detail } => {
                assert!(!detail.is_empty(), "{name} must name its class");
            }
            o => panic!("{name}: oracle failure expected, got {o:?}"),
        }
    }
}

#[test]
fn over_output_bound_is_oracle_failure_with_digests() {
    let mut rig = rig("over-bound");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let exec = FixtureExec::new(
        "sha256:oracle-1",
        OracleObservation {
            stdout: "x".repeat(70 * 1024),
            ..OK()
        },
    );
    let attempt = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref, sub), &exec)
            .expect("over-bound retains")
    };
    match &attempt.outcome {
        AttemptOutcome::OracleFailure { detail } => assert!(detail.contains("output_over_bound")),
        o => panic!("over-bound is oracle failure, got {o:?}"),
    }
    let v = &oracle_events(&rig, ORACLE_VERDICT)[0].payload;
    let ex = v.get("execution").unwrap();
    assert_eq!(ex.get("over_bound"), Some(&Json::Bool(true)));
    assert!(
        ex.get("stdout").is_none(),
        "over-bound keeps digests, not bytes"
    );
    assert_eq!(
        ex.get("stdout_digest").and_then(Json::as_str),
        Some(hharness_identity::idp_id("oracle.stdout", "x".repeat(70 * 1024).as_bytes()).as_str())
    );
}

#[test]
fn attempt_cap_is_durable_derived_across_runner_restart() {
    let mut rig = rig("attempt-cap");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let mut sp = spec(
        &rig.subject,
        &task,
        &sub.compute_id(),
        "sha256:oracle-1",
        VerifierIsolation::Separate,
    );
    sp.policy.max_attempts = 2;
    for want_no in [1u32, 2] {
        let exec = FixtureExec::new("sha256:oracle-1", OK());
        let attempt = {
            let mut runner = OracleRunner::new(
                &mut rig.store,
                &rig.instrument,
                &rig.instrument_lease,
                sp.clone(),
            );
            runner
                .attempt(&sealed(seal_ref.clone(), sub.clone()), &exec)
                .expect("within-cap attempt lands")
        };
        assert_eq!(attempt.attempt_no, want_no);
    }
    // A fresh runner re-derives the count from the durable stream — the
    // frozen cap is enforced against the ledger, not local memory.
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let err = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            sp.clone(),
        );
        runner
            .attempt(&sealed(seal_ref, sub.clone()), &exec)
            .expect_err("the third attempt exceeds the frozen cap")
    };
    assert!(
        matches!(err, OracleError::AttemptCap { max_attempts: 2 }),
        "{err}"
    );
    assert!(!exec.executed.get(), "a capped attempt never executes");
    // Both earlier attempts stay retained — nothing is erased.
    assert_eq!(oracle_events(&rig, ORACLE_VERDICT).len(), 2);
}

#[test]
fn pin_and_digest_mismatches_refuse_without_rows() {
    let mut rig = rig("mismatches");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    // Wrong submission pin.
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let err = {
        let mut sp = spec(
            &rig.subject,
            &task,
            "idp/wrong-submission",
            "sha256:oracle-1",
            VerifierIsolation::Separate,
        );
        sp.oracle.oracle_digest = "sha256:oracle-1".into();
        let mut runner =
            OracleRunner::new(&mut rig.store, &rig.instrument, &rig.instrument_lease, sp);
        runner
            .attempt(&sealed(seal_ref.clone(), sub.clone()), &exec)
            .expect_err("submission pin mismatch refuses")
    };
    assert!(
        matches!(
            err,
            OracleError::PinMismatch { .. } | OracleError::DigestMismatch { .. }
        ),
        "{err}"
    );
    assert!(!exec.executed.get());
    // Mutated artifact — the presented bytes digest to something else.
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let mut mutated = sub.clone();
    mutated.payload_hex = "beef".into();
    let err = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        let s = SealedSubmission {
            submission_ref: sub.compute_id(),
            seal_ref: seal_ref.clone(),
            submission: mutated,
        };
        runner
            .attempt(&s, &exec)
            .expect_err("mutated submission refuses")
    };
    assert!(matches!(err, OracleError::DigestMismatch { .. }), "{err}");
    assert!(!exec.executed.get());
    // Wrong oracle identity — the pin names a different grader.
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let err = {
        let mut sp = spec(
            &rig.subject,
            &task,
            &sub.compute_id(),
            "sha256:oracle-1",
            VerifierIsolation::Separate,
        );
        sp.oracle.oracle_ref = "oracle/impostor".into();
        let mut runner =
            OracleRunner::new(&mut rig.store, &rig.instrument, &rig.instrument_lease, sp);
        runner
            .attempt(&sealed(seal_ref.clone(), sub.clone()), &exec)
            .expect_err("wrong oracle refuses")
    };
    assert!(
        matches!(
            err,
            OracleError::PinMismatch {
                member: "oracle_ref",
                ..
            }
        ),
        "{err}"
    );
    assert!(!exec.executed.get());
    // Executor running non-pinned bytes.
    let exec = FixtureExec::new("sha256:different-oracle", OK());
    let err = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref, sub.clone()), &exec)
            .expect_err("oracle digest mismatch refuses")
    };
    assert!(
        matches!(
            err,
            OracleError::DigestMismatch {
                member: "oracle_digest",
                ..
            }
        ),
        "{err}"
    );
    assert!(!exec.executed.get());
    // No rows landed on the instrument run for any refusal.
    assert!(oracle_events(&rig, ORACLE_INVOKED).is_empty());
    assert!(oracle_events(&rig, ORACLE_VERDICT).is_empty());
}

#[test]
fn unsealed_submission_refuses() {
    let mut rig = rig("unsealed");
    let task = task();
    let sub = submission(&task, "ff");
    // A ref to a non-seal row is not a seal.
    let not_seal = mint(
        &rig.store,
        &rig.subject,
        SEAL_CLASS,
        Json::obj([
            ("env_handle", Json::str("env-1")),
            ("snapshot_ref", Json::str("snap-x")),
            ("purpose", Json::str("checkpoint")),
        ]),
    );
    let not_seal_id = not_seal.event_id.clone();
    rig.store
        .append(&rig.subject, &rig.subject_lease, vec![not_seal])
        .expect("row lands");
    let bad_ref = EventRef {
        run_id: rig.subject.clone(),
        event_id: not_seal_id,
    };
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let err = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(bad_ref, sub.clone()), &exec)
            .expect_err("a non-seal ref refuses")
    };
    assert!(
        matches!(err, OracleError::UnsealedSubmission { .. }),
        "{err}"
    );
    // A dangling ref is not a seal either.
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let err = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(
                &sealed(
                    EventRef {
                        run_id: rig.subject.clone(),
                        event_id: "evt-does-not-exist".into(),
                    },
                    sub.clone(),
                ),
                &exec,
            )
            .expect_err("a dangling seal ref refuses")
    };
    assert!(
        matches!(err, OracleError::UnsealedSubmission { .. }),
        "{err}"
    );
    assert!(oracle_events(&rig, ORACLE_INVOKED).is_empty());
}

#[test]
fn shared_isolation_stays_shared_with_no_detector() {
    let mut rig = rig("shared");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Shared,
            ),
        );
        runner
            .attempt(&sealed(seal_ref, sub.clone()), &exec)
            .expect("shared attempt retains");
    }
    let v = &oracle_events(&rig, ORACLE_VERDICT)[0].payload;
    let iso = v.get("isolation").unwrap();
    assert_eq!(iso.get("mode").and_then(Json::as_str), Some("shared"));
    assert_eq!(iso.get("separate_verifier"), Some(&Json::Bool(false)));
    assert_eq!(
        iso.get("tamper").and_then(Json::as_str),
        Some("n/a{no_detector}"),
        "shared mode keeps the honest no_detector tamper — never an enum upgrade"
    );
}

#[test]
fn no_participant_feedback_no_subject_charge_no_relaunch() {
    let mut rig = rig("no-feedback");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let subject_rows_before = rig.store.events(&rig.subject).unwrap().len();
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref, sub.clone()), &exec)
            .expect("attempt lands");
    }
    // The subject run gained nothing — no feedback rows, no relaunch.
    let after = rig.store.events(&rig.subject).unwrap();
    assert_eq!(after.len(), subject_rows_before);
    assert!(!after
        .iter()
        .any(|e| e.class.contains("replanned") || e.class.contains("launched")));
    // Instrument attribution: both new classes charge instrument (R-ACC-3).
    assert_eq!(
        hharness_budget::attribution::default_charged_to(ORACLE_INVOKED),
        ChargedTo::Instrument
    );
    assert_eq!(
        hharness_budget::attribution::default_charged_to(ORACLE_VERDICT),
        ChargedTo::Instrument
    );
    // The instrument run carries only instrument rows for this attempt.
    let verdict = &oracle_events(&rig, ORACLE_VERDICT)[0];
    assert_eq!(
        verdict.payload.get("cost_ref").and_then(Json::as_str),
        Some("budget/instrument")
    );
    assert_eq!(
        verdict.payload.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
}

#[test]
fn grade_attempt_produces_selection_facts() {
    let mut rig = rig("selection");
    let task = task();
    let sub = submission(&task, "ff");
    let seal_ref = seal(&mut rig, "env-1", "snap-1");
    let exec = FixtureExec::new("sha256:oracle-1", OK());
    let attempt = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref.clone(), sub.clone()), &exec)
            .expect("attempt lands")
    };
    let ga = attempt.to_grade_attempt("task_success");
    assert_eq!(ga.instrument_run_id, rig.instrument);
    assert_eq!(ga.seq, attempt.verdict_seq);
    assert_eq!(ga.subject_ref, rig.subject);
    assert_eq!(ga.submission_ref, sub.compute_id());
    assert_eq!(ga.oracle_ref, task.instrument.grader.adapter_ref);
    match &ga.outcome {
        GradeOutcome::Decided(MetricValueKind::Decimal(v)) => assert_eq!(*v, 1_000_000),
        o => panic!("decided grade fact, got {o:?}"),
    }
    // Causes cite the subject run's seal plus the instrument run's rows —
    // no foreign run.
    let cited: Vec<&str> = ga
        .cause_refs
        .iter()
        .map(|c| c.split(':').next().unwrap())
        .collect();
    assert!(cited.contains(&rig.subject.as_str()));
    assert!(cited.contains(&rig.instrument.as_str()));
    assert!(cited
        .iter()
        .all(|r| *r == rig.subject || *r == rig.instrument));
    // A failed attempt produces the OracleFailure fact — never a zero.
    let seal_ref2 = seal(&mut rig, "env-1", "snap-2");
    let sub2 = submission(&task, "ee");
    let exec2 = FixtureExec::new(
        "sha256:oracle-1",
        OracleObservation {
            timed_out: true,
            ..OK()
        },
    );
    let attempt2 = {
        let mut runner = OracleRunner::new(
            &mut rig.store,
            &rig.instrument,
            &rig.instrument_lease,
            spec(
                &rig.subject,
                &task,
                &sub2.compute_id(),
                "sha256:oracle-1",
                VerifierIsolation::Separate,
            ),
        );
        runner
            .attempt(&sealed(seal_ref2, sub2), &exec2)
            .expect("failed attempt retains")
    };
    assert!(matches!(
        attempt2.to_grade_attempt("task_success").outcome,
        GradeOutcome::OracleFailure
    ));
}
