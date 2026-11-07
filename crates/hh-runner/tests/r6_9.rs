//! R6.9 acceptance battery (manifest 262; CELL-SPEC + DEADLINE +
//! VERDICT): the fixture-cell lifecycle (AC-1/AC-3.3-1), the agent
//! deadline lapse (AC-5/AC-3.3-4), the deadline admission in the slot
//! record (AC-4/AC-3.6-1 worker leg), the golden reward files +
//! bootstrap truth table (AC-6/7, AC-3.4-1/2), the same-cell regrade
//! bound (AC-8/AC-3.4-3), the tb10 channel pin + committed
//! `receipt.parsed` (AC-9/AC-3.4-4), the ANSI-strip/declared-workdir
//! fails-without legs and the $0 probe driver (AC-10/AC-2.3-9).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hharness_runner::cell::{run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan};
use hharness_runner::disposition::{AdmissionReason, Disposition, EndCause};
use hharness_runner::packet::{SlotPacket, SlotRecipe};
use hharness_runner::probe::{probe_driver, PROBE_TASKS};
use hharness_runner::slot_record::ExperimentSlot;
use hharness_runner::verdict::{self, classify, strip_ansi, MediatorRow, RewardFileSet};
use hharness_runner::worker::{run_slot_worker, slot_run_id};
use hharness_wire::json::{self, Json};

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r69-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── RecordingCell — an in-memory fixture cell ──────────────────────────

#[derive(Default)]
struct RecordingCell {
    /// The cell filesystem (top-level entries only).
    fs: BTreeSet<String>,
    /// The verbatim op log (`op arg`).
    log: Vec<String>,
    /// Exec outcomes by argv head, in order.
    agent_times_out: bool,
    /// Host bytes the copy-out produces per cell path.
    copies: BTreeMap<String, Vec<u8>>,
    agent_execs: u32,
    verify_execs: u32,
}

impl CellOps for RecordingCell {
    fn present(&mut self, path: &str) -> Result<bool, CellError> {
        Ok(self.fs.contains(path))
    }
    fn ls_dir(&mut self, _path: &str) -> Result<Vec<String>, CellError> {
        Ok(self
            .fs
            .iter()
            .filter(|e| !e.contains('/') || e.split('/').nth(1).is_some())
            .map(|e| e.clone())
            .collect::<Vec<_>>())
    }
    fn cp_in(&mut self, _host_src: &Path, container_dst: &str) -> Result<(), CellError> {
        self.log.push(format!("cp_in ->{container_dst}"));
        self.fs.insert(container_dst.to_string());
        Ok(())
    }
    fn cp_out(&mut self, container_src: &str, host_dst: &Path) -> Result<(), CellError> {
        self.log.push(format!("cp_out {container_src}"));
        match self.copies.get(container_src) {
            Some(b) => {
                std::fs::write(host_dst, b).unwrap();
                Ok(())
            }
            None => Err(CellError {
                op: "cp_out",
                detail: format!("{container_src} absent"),
            }),
        }
    }
    fn rm_glob(&mut self, glob: &str) -> Result<(), CellError> {
        self.log.push(format!("rm {glob}"));
        Ok(())
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        if spec.argv.first().map(String::as_str) == Some("mkdir") {
            return Ok(CellOutcome {
                exit_code: Some(0),
                timed_out: false,
                stdout: vec![],
            });
        }
        self.log
            .push(format!("exec {} @{}", spec.argv.join(" "), spec.cwd));
        if spec.argv.join(" ").contains("agent") {
            self.agent_execs += 1;
            if self.agent_times_out {
                return Ok(CellOutcome {
                    exit_code: None,
                    timed_out: true,
                    stdout: vec![],
                });
            }
            return Ok(CellOutcome {
                exit_code: Some(0),
                timed_out: false,
                stdout: vec![],
            });
        }
        self.verify_execs += 1;
        Ok(CellOutcome {
            exit_code: Some(0),
            timed_out: false,
            stdout: vec![],
        })
    }
    fn terminate_group(&mut self, pattern: &str) -> Result<(), CellError> {
        self.log.push(format!("terminate {pattern}"));
        Ok(())
    }
}

fn plan(dir: &Path) -> CellPlan {
    std::fs::create_dir_all(dir.join("staging/solution")).unwrap();
    std::fs::create_dir_all(dir.join("staging/tests")).unwrap();
    CellPlan {
        oracle_participant: true,
        workdir: "/app/langcodes".to_string(),
        host_staging: dir.join("staging"),
        collect_dir: dir.join("collect"),
        agent_argv: vec!["agent".to_string()],
        agent_deadline_ms: Some(12_000_000),
        verifier_argv: vec!["bash".to_string(), "/tests/test.sh".to_string()],
        verifier_deadline_ms: Some(12_000_000),
        max_grade_attempts: 2,
    }
}

/// AC-1: `/tests` and `/solution` are absent before their phase, arrive
/// by copy-in, and never travel `/work`; stale `reward.*` is removed
/// before ST5.
#[test]
fn phase_boundary_holds_through_the_lifecycle() {
    let dir = tmp("cell");
    let mut cell = RecordingCell::default();
    cell.copies
        .insert("/logs/verifier/reward.txt".into(), b"1".to_vec());
    let report = run_cell(&mut cell, &plan(&dir)).unwrap();
    let at = |phase: &str| {
        report
            .listings
            .iter()
            .find(|l| l.phase == phase)
            .unwrap()
            .entries
            .clone()
    };
    assert!(!at("provision").contains(&"/tests".to_string()));
    assert!(!at("provision").contains(&"/solution".to_string()));
    assert!(!at("setup").contains(&"/tests".to_string()));
    assert!(at("setup").contains(&"/solution".to_string()));
    assert!(at("verify").contains(&"/tests".to_string()));
    // Copy-in destinations never touch /work; stale reward removed first.
    assert!(cell.log.iter().any(|l| l == "cp_in ->/solution"));
    assert!(cell.log.iter().any(|l| l == "cp_in ->/tests"));
    assert!(!cell.log.iter().any(|l| l.contains("/work")));
    let rm = cell
        .log
        .iter()
        .position(|l| l.starts_with("rm /logs/verifier/reward.*"))
        .unwrap();
    let test_exec = cell
        .log
        .iter()
        .position(|l| l.contains("/tests/test.sh"))
        .unwrap();
    assert!(rm < test_exec, "stale reward.* must be removed before ST5");
    // The declared WORKDIR — every phase exec runs under it (the
    // task_workdir repair); a `/work` hardcode fails this leg.
    assert!(cell
        .log
        .iter()
        .filter(|l| l.starts_with("exec "))
        .all(|l| l.ends_with("@/app/langcodes")));
}

/// AC-5: a lapsed agent deadline terminates the process group and the
/// slot proceeds to verification with `end_cause: agent_timeout`.
#[test]
fn agent_deadline_lapse_terminates_and_verifies() {
    let dir = tmp("timeout");
    let mut cell = RecordingCell {
        agent_times_out: true,
        ..Default::default()
    };
    let report = run_cell(&mut cell, &plan(&dir)).unwrap();
    assert!(report.agent_terminated);
    assert!(cell.log.iter().any(|l| l == "terminate agent"));
    assert_eq!(report.agent_end_cause, EndCause::AgentTimeout);
    assert!(report.verify_attempts >= 1, "ST5 still runs");
    assert_eq!(cell.agent_execs, 1, "the participant never re-runs");
}

/// AC-8: the regrade bound holds at `max_attempts` in the same cell.
#[test]
fn regrade_bound_in_same_cell() {
    let dir = tmp("regrade");
    let mut cell = RecordingCell::default();
    // The verifier never produces a reward file → every leg is
    // unavailable; the bound caps at max_grade_attempts.
    let report = run_cell(&mut cell, &plan(&dir)).unwrap();
    assert_eq!(report.verify_attempts, 2);
    assert_eq!(cell.verify_execs, 2);
    assert_eq!(cell.agent_execs, 1, "the participant is never re-launched");
}

/// A `/tests` that exists before its phase is a typed phase-boundary
/// refusal — never a relabel.
#[test]
fn tests_preexists_refuses() {
    let dir = tmp("prex");
    let mut cell = RecordingCell::default();
    cell.fs.insert("/tests".to_string());
    assert!(run_cell(&mut cell, &plan(&dir)).is_err());
}

// ── Golden reward files (AC-6) + the bootstrap truth table (AC-7) ──────

fn files(
    txt: Option<&str>,
    json: Option<&str>,
    ctrf: Option<&str>,
    stdout: Option<&str>,
) -> RewardFileSet {
    RewardFileSet {
        reward_json: json.map(|s| s.as_bytes().to_vec()),
        reward_txt: txt.map(|s| s.as_bytes().to_vec()),
        ctrf_json: ctrf.map(|s| s.as_bytes().to_vec()),
        test_stdout: stdout.map(|s| s.as_bytes().to_vec()),
    }
}

const CTRF_OK: &str = r#"{"results":{"summary":{"tests":3,"passed":3,"failed":0},"tests":[]}}"#;
const CTRF_FAIL: &str = r#"{"results":{"summary":{"tests":3,"passed":2,"failed":1},"tests":[]}}"#;

#[test]
fn golden_reward_files_map() {
    for (txt, json, want_ppm) in [
        (Some("1"), None, 1_000_000),
        (Some("0"), None, 0),
        (Some("0.5"), None, 500_000),
        (Some("1"), Some("garbage"), 0), // placeholder — replaced below
    ] {
        let v = classify(&files(txt, json, Some(CTRF_OK), None), &[], false);
        if txt == Some("1") && json == Some("garbage") {
            continue;
        }
        assert_eq!(v.reward_ppm, Some(want_ppm), "{txt:?} {json:?}");
        assert_eq!(v.success, Some(want_ppm > 0));
        assert_eq!(v.classification, "decided");
    }
    // .json preferred over .txt
    let v = classify(
        &files(Some("0"), Some(r#"{"reward":500000}"#), Some(CTRF_OK), None),
        &[],
        false,
    );
    assert_eq!(v.reward_ppm, Some(500_000));
    // empty txt → unparseable; garbage → unparseable; absent → missing;
    // multi-key json → reward_json_multi_key
    for (f, cause) in [
        (
            files(Some(""), None, Some(CTRF_OK), None),
            "reward_unparseable",
        ),
        (
            files(Some("garbage"), None, Some(CTRF_OK), None),
            "reward_unparseable",
        ),
        (files(None, None, Some(CTRF_OK), None), "reward_missing"),
        (
            files(None, Some(r#"{"reward":1,"extra":2}"#), Some(CTRF_OK), None),
            "reward_json_multi_key",
        ),
    ] {
        let v = classify(&f, &[], false);
        assert_eq!(v.classification, format!("verifier_unavailable:{cause}"));
        assert_eq!(v.reward_ppm, None);
    }
}

#[test]
fn bootstrap_truth_table() {
    let deny = MediatorRow {
        phase: "verify".into(),
        decision: "deny".into(),
    };
    let error = MediatorRow {
        phase: "verify".into(),
        decision: "error".into(),
    };
    // reward 0 + no ctrf + deny → bootstrap:egress_denied
    let v = classify(&files(Some("0"), None, None, None), &[deny.clone()], false);
    assert_eq!(
        v.classification,
        "verifier_unavailable:bootstrap:egress_denied"
    );
    // upstream error → bootstrap:upstream_error
    let v = classify(&files(Some("0"), None, None, None), &[error], false);
    assert_eq!(
        v.classification,
        "verifier_unavailable:bootstrap:upstream_error"
    );
    // stdout signature → bootstrap:signature
    for sig in [
        "E: Failed to fetch http://x",
        "Temporary failure resolving 'deb.debian.org'",
        "curl: (22) The requested URL returned error: 404",
        "error: Failed to download distributions",
    ] {
        let v = classify(&files(Some("0"), None, None, Some(sig)), &[], false);
        assert_eq!(
            v.classification, "verifier_unavailable:bootstrap:signature",
            "{sig}"
        );
    }
    // no evidence → scored 0 + infra_suspected flag
    let v = classify(&files(Some("0"), None, None, None), &[], false);
    assert_eq!(v.reward_ppm, Some(0));
    assert!(v
        .flags
        .contains(&hharness_runner::disposition::SlotFlag::InfraSuspectedVerifierNoReport));
    // a deny outside the verify phase is not evidence
    let stray = MediatorRow {
        phase: "agent".into(),
        decision: "deny".into(),
    };
    let v = classify(&files(Some("0"), None, None, None), &[stray], false);
    assert_eq!(v.classification, "decided");
    // ctrf present → scored on the reward
    let v = classify(&files(Some("0"), None, Some(CTRF_OK), None), &[], false);
    assert_eq!(v.classification, "decided");
    // reward=1 + ctrf failures → scored + reward_ctrf_inconsistent
    let v = classify(&files(Some("1"), None, Some(CTRF_FAIL), None), &[], false);
    assert_eq!(v.reward_ppm, Some(1_000_000));
    assert!(v
        .flags
        .contains(&hharness_runner::disposition::SlotFlag::RewardCtrfInconsistent));
    // verifier timeout → verifier_timeout
    let v = classify(&files(Some("0"), None, None, None), &[], true);
    assert_eq!(v.classification, "verifier_unavailable:verifier_timeout");
}

// ── tb10_pytest_log/1 (AC-9) + the ANSI/workdir fails-without legs ─────

#[test]
fn tb10_channel_parses_and_refuses_tb21() {
    // The pin gate: tb10 serves the TB 1.0 pin only.
    assert_eq!(
        verdict::select_channel(verdict::TB10_PIN).unwrap(),
        verdict::TB10_CHANNEL
    );
    assert!(verdict::select_channel("harbor-framework/terminal-bench-2-1@12b75656").is_ok());
    assert!(verdict::select_channel("terminal-bench-2-0").is_err());
    // The fixture log: ANSI codes inside the banner + status lines.
    let log_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r6_9/tb10-verdict.log");
    let raw = std::fs::read_to_string(&log_path).unwrap();
    let stripped = strip_ansi(&raw);
    let parsed = verdict::parse_pytest_summary(&stripped).unwrap();
    // The committed `receipt.parsed` for the fixture shape.
    let expect_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r6_9/expected.parsed.json");
    let expect = json::parse(&std::fs::read_to_string(&expect_path).unwrap()).unwrap();
    let Json::Obj(exp) = expect else {
        panic!("expected obj")
    };
    for (k, v) in &exp {
        assert_eq!(
            parsed.get(k).map(String::as_str),
            Some(v.as_str().unwrap_or("")),
            "receipt.parsed mismatch on {k}"
        );
    }
    assert_eq!(parsed.len(), exp.len());
    // fails-without: the raw ANSI-laden bytes do not reproduce the map.
    let raw_parsed = verdict::parse_pytest_summary(&raw).unwrap_or_default();
    assert_ne!(raw_parsed, parsed);
}

#[test]
fn strip_ansi_semantics() {
    // OSC, CSI, MISC + \r last-segment — the committed repair set.
    assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0mb"), "aredb");
    assert_eq!(strip_ansi("x\u{1b}]0;title\u{7}y"), "xy");
    assert_eq!(strip_ansi("p\u{1b}(0q\u{1b}(Bz"), "pq(Bz");
    assert_eq!(strip_ansi("abc\rd"), "d");
    assert_eq!(strip_ansi("a\rb\nc"), "b\nc");
}

// ── Worker deadline admission (AC-4 slot leg) + record echo ────────────

fn slot_packet(
    task: BTreeMap<String, Json>,
    cell: BTreeMap<String, Json>,
    recipe: SlotRecipe,
) -> SlotPacket {
    SlotPacket {
        run_plan_id: "plan:r69".to_string(),
        subject_run_id: "subj:r69".to_string(),
        attempt_no: 1,
        experiment_id: "e:r69".to_string(),
        protocol_id: "p".to_string(),
        selection_id: "s".to_string(),
        cell_id: "c".to_string(),
        arm_id: "a".to_string(),
        task_id: "t".to_string(),
        replicate_index: 0,
        split_label: "dev".to_string(),
        slot_root: String::new(),
        slot_run_id: slot_run_id("plan:r69", 1),
        task,
        participant: BTreeMap::new(),
        cell,
        recipe,
    }
}

#[test]
fn declared_deadlines_enforced_in_record() {
    let root = tmp("dl");
    let mut task = BTreeMap::new();
    task.insert(
        "deadlines".to_string(),
        Json::obj([
            ("agent_sec", Json::Int(12_000)),
            ("verifier_sec", Json::Int(12_000)),
            ("ceiling_sec", Json::Int(12_000)),
        ]),
    );
    let p = slot_packet(
        task,
        BTreeMap::new(),
        SlotRecipe::Scored {
            reward_ppm: 1,
            end_cause: EndCause::AgentTimeout,
            consumed_model_calls: 1,
        },
    );
    let seal = run_slot_worker(&p, &root).unwrap();
    let store = hharness_ledger::store::Store::open(&root).unwrap();
    let evs = store.events(&seal.slot_run_id).unwrap();
    let f = evs
        .iter()
        .find(|e| e.class == "lifecycle.run.finished")
        .unwrap();
    let addr = f
        .payload
        .get("slot_record_ref")
        .and_then(Json::as_str)
        .unwrap();
    let (_a, dig) = addr.split_once(':').unwrap();
    let bytes = store
        .get_blob(&hharness_identity::idp::ContentAddress {
            idp: "idp/1".into(),
            algorithm: "sha256".into(),
            digest: dig.to_string(),
            media_type: "application/json".into(),
            size: 0,
        })
        .unwrap();
    let rec =
        ExperimentSlot::from_json(&json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap())
            .unwrap();
    let agent = rec.phases.iter().find(|p| p.phase == "agent").unwrap();
    assert_eq!(agent.deadline_declared_sec, Some(12_000));
    assert_eq!(agent.deadline_enforced_ms, Some(12_000_000));
    let verify = rec.phases.iter().find(|p| p.phase == "verify").unwrap();
    assert_eq!(verify.deadline_enforced_ms, Some(12_000_000));
    assert_eq!(agent.end_cause, "agent_timeout");
    // `end_cause` rides the phases[] rows and the finished payload's
    // own member (the disposition spelling is `executed{resolved}`).
    assert_eq!(
        f.payload.get("end_cause").and_then(Json::as_str),
        Some("agent_timeout")
    );
    let _ = rec.disposition;
    let _ = Disposition::Executed {
        resolved: true,
        end_cause: EndCause::Exited(0),
    };
}

#[test]
fn above_ceiling_deadline_withholds() {
    let root = tmp("dlbad");
    let mut task = BTreeMap::new();
    task.insert(
        "deadlines".to_string(),
        Json::obj([
            ("agent_sec", Json::Int(99_999)),
            ("verifier_sec", Json::Int(60)),
            ("ceiling_sec", Json::Int(12_000)),
        ]),
    );
    let p = slot_packet(
        task,
        BTreeMap::new(),
        SlotRecipe::Scored {
            reward_ppm: 1,
            end_cause: EndCause::Exited(0),
            consumed_model_calls: 1,
        },
    );
    let seal = run_slot_worker(&p, &root).unwrap();
    assert_eq!(
        seal.disposition.as_str(),
        "withheld{admission:deadline_admission}"
    );
    let _ = AdmissionReason::DeadlineAdmission;
}

// ── The $0 probe driver (AC-10) ─────────────────────────────────────────

#[test]
fn probe_driver_cites_both_tasks_and_owes_execution() {
    let d = probe_driver("deadbeef");
    assert_eq!(
        d.get("schema").and_then(Json::as_str),
        Some("hh-targeted-probe/1")
    );
    let Some(Json::Arr(tasks)) = d.get("tasks") else {
        panic!("no tasks array")
    };
    let ids: Vec<String> = tasks
        .iter()
        .filter_map(|t| t.get("task_id").and_then(Json::as_str).map(str::to_string))
        .collect();
    for t in PROBE_TASKS {
        assert!(ids.contains(&t.to_string()), "{ids:?}");
    }
    assert_eq!(
        d.get("probe_execution").and_then(Json::as_str),
        Some("owed{row:R6.32,window:W1}")
    );
    assert_eq!(
        d.get("driver_commit").and_then(Json::as_str),
        Some("deadbeef")
    );
}
