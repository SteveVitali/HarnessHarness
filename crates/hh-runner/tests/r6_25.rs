//! R6.25 acceptance battery (manifest 278; CAPTURE + SMOKE-BATTERY):
//! AC-1 `/logs/agent` copy-out + digests (REQ-XC-317); AC-2 post-hoc
//! ATIF + publication input (REQ-XC-318, REQ-XB-013 superseded); AC-3
//! receipt reconciliation (REQ-XC-207); AC-4 ST4 self-modification
//! (REQ-XC-328); AC-5 the offline checker over fixture slot records
//! per variant × harness (REQ-XC-324); AC-6 AG5/egress probe
//! (REQ-XC-311, REQ-XC-205 alias); AC-7 model-socket assertion
//! (REQ-XC-312); AC-8 endpoint admission (REQ-XC-203); AC-9 end
//! causes + descriptive status (REQ-XC-321); AC-10 the §5.8 classifier
//! truth table + recompute (REQ-XC-323); AC-11 offline-only +
//! ignore-cited live legs (REQ-XC-329). USD $0, no podman, no network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::admission::{self, ValidationContext};
use hharness_runner::admission_build::{st4_plan, St2Observation};
use hharness_runner::capture::{
    self, AtifConverter, AtifDoc, CallKind, NativeCall, St4Observation,
};
use hharness_runner::cell::{run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan};
use hharness_runner::disposition::{
    AdmissionReason, Disposition, EndCause, InconclusiveCause, InfraCause, SlotFlag, WithheldReason,
};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::slot_record::{
    ExperimentSlot, SlotCosts, SlotIntegrity, SlotPhase, SlotVerdict,
};
use hharness_runner::smoke::{
    self, check_battery, check_smoke, classify_hosting, endpoint_admission, facts_from_records,
    mediator_subset, model_socket_assert, native_status, receipt_on_surface, recording_shape,
    recording_surface, replay_decl, HostingFacts, Retry, SmokeRecords, SmokeVariant,
    BATTERY_SCHEMA, NA_ENDPOINT,
};
use hharness_wire::json::{self, Json};

const FIXTURES: &str = "tests/fixtures/r6_25";
const ADMISSIONS: &str = "tests/fixtures/r6_22";

fn load(path: &str) -> Json {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    json::parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn rec(harness: &str) -> Json {
    load(&format!("{ADMISSIONS}/{harness}.json"))
}

fn recording(harness: &str) -> Json {
    load(&format!("{FIXTURES}/recordings/{harness}.json"))
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r625-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── fixture records ─────────────────────────────────────────────────────────

fn identity() -> Json {
    Json::obj([
        ("experiment_id", Json::str("exp")),
        ("protocol_id", Json::str("proto")),
        ("run_plan_id", Json::str("rp")),
        ("cell_id", Json::str("cell")),
        ("arm_id", Json::str("arm")),
        ("task_id", Json::str("hh-smoke/write-file")),
        ("replicate_index", Json::Int(0)),
        ("first", Json::Bool(true)),
        ("attempt_no", Json::Int(1)),
        ("subject_run_id", Json::str("subj")),
        (
            "slot_store",
            Json::obj([
                ("root_digest", Json::str("sha256:aa")),
                ("run_id", Json::str("slot-run")),
                ("head_hash", Json::str("hh")),
            ]),
        ),
        (
            "runner",
            Json::obj([
                ("driver_commit", Json::str("c")),
                ("binary_sha256", Json::str("sha256:bb")),
            ]),
        ),
    ])
}

fn phase(end: &str, exit: Option<i64>, deadline_ms: Option<u64>) -> SlotPhase {
    SlotPhase {
        phase: "agent".into(),
        wall_ms: 1,
        deadline_declared_sec: deadline_ms.map(|_| 60),
        deadline_multiplier_ppm: deadline_ms.map(|_| 1_000_000),
        deadline_enforced_ms: deadline_ms,
        end_cause: end.into(),
        exit_code: exit,
        dispatch_outcome: "observed:applied".into(),
    }
}

fn verdict(ppm: u64) -> SlotVerdict {
    SlotVerdict {
        channel: "hh-runner-fixture-verdict/1".into(),
        files: BTreeMap::new(),
        reward_raw: None,
        reward_ppm: Some(ppm),
        success: Some(ppm > 0),
        ctrf: None,
        classification: "decided".into(),
        flags: vec![],
        grade_attempts: vec![],
    }
}

fn slot(
    end: &str,
    exit: Option<i64>,
    dl: Option<u64>,
    disp: Disposition,
    v: Option<SlotVerdict>,
) -> ExperimentSlot {
    ExperimentSlot {
        identity: identity(),
        task: Json::obj([]),
        participant: Json::obj([]),
        cell: Json::obj([]),
        phases: vec![phase(end, exit, dl)],
        verdict: v,
        disposition: disp,
        prior_disposition: None,
        repairs: vec![],
        integrity: SlotIntegrity {
            seal: "sealed".into(),
            seal_ref: None,
            sentinel_intact: None,
            observer_survived: None,
            probe: None,
        },
        trajectory: BTreeMap::new(),
        costs: SlotCosts::default(),
    }
}

fn receipt(seq: i64, path: &str, disposition: &str) -> Json {
    Json::obj([
        ("slot_call_seq", Json::Int(seq)),
        (
            "downstream",
            Json::obj([("host", Json::str("127.0.0.1")), ("path", Json::str(path))]),
        ),
        ("disposition", Json::str(disposition)),
    ])
}

fn route(harness: &str) -> &'static str {
    match harness {
        "claude-code" => "/v1/messages",
        _ => "/v1/chat/completions",
    }
}

fn denied(host: &str) -> Json {
    Json::obj([("host", Json::str(host)), ("decision", Json::str("denied"))])
}

/// A passing SM-pos records bundle for `harness`.
fn pos_records(harness: &str) -> (Json, SmokeRecords) {
    let rec = rec(harness);
    let n = recording(harness)
        .get("calls")
        .and_then(Json::as_arr)
        .map(|a| a.len())
        .unwrap();
    let receipts: Vec<Json> = (0..n as i64)
        .map(|k| receipt(k, route(harness), "forwarded"))
        .collect();
    let covered = smoke::expected_denied(&rec);
    // A denied row covered by expected_denied: the bare host when the
    // pattern is exact, a subdomain when it is `*.`-suffixed.
    let pat = &covered[0];
    let host = match pat.strip_prefix("*.") {
        Some(sfx) => format!("x.{sfx}"),
        None => pat.clone(),
    };
    let mediator_rows = vec![denied(&host)];
    let st2 = St2Observation {
        version_stdout: "v1.0.0".into(),
        bundle_tree_sha256: "sha256:t".into(),
        config_sha256: "sha256:c".into(),
        binary_sha256: None,
    };
    let st4 = St4Observation {
        version_stdout: "v1.0.0".into(),
        install_tree_sha256: "sha256:t".into(),
    };
    let mut s = slot(
        "exited{0}",
        Some(0),
        Some(30_000),
        Disposition::Executed {
            resolved: true,
            end_cause: EndCause::Exited(0),
        },
        Some(verdict(1_000_000)),
    );
    s.trajectory = capture::trajectory_members(
        Some(&capture::hash_files(BTreeMap::from([(
            "a.txt".to_string(),
            b"log".to_vec(),
        )]))),
        Some(("blob:atif", Some("atif/1"))),
        Some("hh-receipt-index/1:tx"),
    );
    (
        rec,
        SmokeRecords {
            slot: Some(s),
            receipts,
            mediator_rows,
            native_calls: (0..n as u64)
                .map(|k| NativeCall {
                    seq: k,
                    kind: CallKind::Main,
                })
                .collect(),
            recording: Some(recording(harness)),
            st2: Some(st2),
            st4: Some(st4),
            agent_terminated: false,
            leak_hits: 0,
        },
    )
}

// ── AC-1 (AC-5.6-1; REQ-XC-317) — the copy-out fixture ────────────────────────

#[derive(Default)]
struct FakeCell {
    ops: Vec<String>,
    logs: BTreeMap<String, Vec<u8>>,
}

impl CellOps for FakeCell {
    fn present(&mut self, _p: &str) -> Result<bool, CellError> {
        Ok(false)
    }
    fn ls_dir(&mut self, _p: &str) -> Result<Vec<String>, CellError> {
        Ok(vec![])
    }
    fn cp_in(&mut self, _h: &Path, c: &str) -> Result<(), CellError> {
        self.ops.push(format!("cp_in {c}"));
        Ok(())
    }
    fn cp_out(&mut self, src: &str, dst: &Path) -> Result<(), CellError> {
        self.ops.push(format!("cp_out {src}"));
        for (rel, bytes) in &self.logs {
            let p = dst.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, bytes).unwrap();
        }
        Ok(())
    }
    fn rm_glob(&mut self, _g: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        self.ops.push(format!("exec {}", spec.argv.join(" ")));
        Ok(CellOutcome {
            exit_code: Some(0),
            timed_out: false,
            stdout: vec![],
        })
    }
    fn terminate_group(&mut self, _p: &str) -> Result<(), CellError> {
        Ok(())
    }
}

#[test]
fn ac1_copyout_after_agent_before_verify_and_work_alias_refused() {
    let dir = tmp("copyout");
    let work = tmp("work");
    std::fs::create_dir_all(dir.join("staging/tests")).unwrap();
    std::fs::create_dir_all(dir.join("staging/solution")).unwrap();
    let spec = capture::copyout_spec(capture::LOGS_AGENT, &dir.join("logs"), &work).unwrap();
    // /work aliasing refuses — both directions.
    for bad in ["\u{2f}work", "\u{2f}work\u{2f}logs"] {
        assert_eq!(
            capture::copyout_spec(bad, &dir.join("x"), &work)
                .unwrap_err()
                .rule,
            "work_alias"
        );
    }
    assert_eq!(
        capture::copyout_spec(capture::LOGS_AGENT, &work.join("x"), &work)
            .unwrap_err()
            .rule,
        "work_alias"
    );
    assert!(capture::copyout_spec("\u{2f}other", &dir.join("x"), &work).is_err());
    let plan = CellPlan {
        oracle_participant: false,
        workdir: "app".into(),
        host_staging: dir.join("staging"),
        collect_dir: dir.join("collect"),
        agent_argv: vec!["agent".into()],
        agent_deadline_ms: Some(1000),
        verifier_argv: vec!["verify".into()],
        verifier_deadline_ms: Some(1000),
        max_grade_attempts: 1,
        logs_copyout: Some(spec),
    };
    let mut cell = FakeCell::default();
    cell.logs.insert(
        "mini-swe-agent.trajectory.json".into(),
        b"[{\"step\":1}]".to_vec(),
    );
    cell.logs
        .insert("mini-swe-agent.txt".into(), b"log tail".to_vec());
    let report = run_cell(&mut cell, &plan).unwrap();
    // Ordering: the copy-out ran after the agent exec and before any
    // verify exec (the plan's verifier argv runs after it in the log).
    let at = |needle: &str| {
        cell.ops
            .iter()
            .position(|o| o.contains(needle))
            .unwrap_or(usize::MAX)
    };
    let cp = at("cp_out \u{2f}logs\u{2f}agent");
    assert!(at("exec agent") < cp, "copy-out follows agent end");
    assert!(cp < at("exec verify"), "copy-out precedes verification");
    let bundle = report.native_log.unwrap();
    assert_eq!(bundle.files.len(), 2);
    // The slot-record digests recompute.
    let bytes_of = |p: &str| cell.logs.get(p).cloned();
    bundle.verify(&bytes_of).unwrap();
    // A tampered member fails the recompute.
    let mut drifted = bundle.clone();
    drifted.files.values_mut().next().unwrap().sha256 = "sha256:0".repeat(1) + &"0".repeat(63);
    assert!(drifted.verify(&bytes_of).is_err());
    // The trajectory member round-trips the slot codec.
    s_integrity_bundle();
}

fn s_integrity_bundle() {
    // Integrity-input bundle — members' digests + manifest recompute.
    let members: Vec<(String, Vec<u8>)> = [
        ("receipts", b"r".to_vec()),
        ("egress_rows", b"e".to_vec()),
        ("seal", b"s".to_vec()),
        ("native_artifacts", b"n".to_vec()),
        ("terminal_transcript", b"t".to_vec()),
    ]
    .into_iter()
    .map(|(r, b)| (r.to_string(), b))
    .collect();
    let doc = capture::integrity_bundle(&members).unwrap();
    let map: BTreeMap<String, Vec<u8>> = members.into_iter().collect();
    capture::verify_bundle(&doc, &|r| map.get(r).cloned()).unwrap();
    let mut bad = map.clone();
    bad.insert("seal".into(), b"tampered".to_vec());
    assert!(capture::verify_bundle(&doc, &|r| bad.get(r).cloned()).is_err());
}

// ── AC-2 (AC-5.6-2; REQ-XC-318) — post-hoc ATIF + the publication input ───────

struct FixedConverter;
impl AtifConverter for FixedConverter {
    fn convert(&self, logs: &BTreeMap<String, Vec<u8>>) -> Result<AtifDoc, String> {
        if logs.is_empty() {
            return Err("no logs".into());
        }
        Ok(AtifDoc {
            bytes: b"{\"atif_version\":\"atif/1\"}".to_vec(),
            schema_version: Some("atif/1".into()),
        })
    }
}

#[test]
fn ac2_atif_post_hoc_and_publication_needs_transcript() {
    let rec = rec("mini-swe-agent");
    assert!(capture::atif_pin_check(&rec).unwrap());
    let logs = BTreeMap::from([("a.json".to_string(), b"[]".to_vec())]);
    let doc = capture::convert_atif(&rec, Some(&FixedConverter), &logs)
        .unwrap()
        .unwrap();
    assert_eq!(doc.schema_version.as_deref(), Some("atif/1"));
    // Absence is a reported attribute — `artifacts.atif:"none"` and a
    // failed converter both produce None, never an error.
    let mut none_rec = rec.clone();
    if let Json::Obj(m) = &mut none_rec {
        if let Json::Obj(a) = m.get_mut("artifacts").unwrap() {
            a.insert("atif".into(), Json::str("none"));
        }
    }
    assert!(!capture::atif_pin_check(&none_rec).unwrap());
    assert!(
        capture::convert_atif(&none_rec, Some(&FixedConverter), &logs)
            .unwrap()
            .is_none()
    );
    struct Fail;
    impl AtifConverter for Fail {
        fn convert(&self, _l: &BTreeMap<String, Vec<u8>>) -> Result<AtifDoc, String> {
            Err("converter crashed".into())
        }
    }
    assert!(capture::convert_atif(&rec, Some(&Fail), &logs)
        .unwrap()
        .is_none());
    // A converter not at the pin refuses.
    let mut bad = rec.clone();
    if let Json::Obj(m) = &mut bad {
        if let Json::Obj(a) = m.get_mut("artifacts").unwrap() {
            a.insert(
                "atif".into(),
                Json::obj([("converter_at", Json::str("deadbeef"))]),
            );
        }
    }
    assert_eq!(capture::atif_pin_check(&bad).unwrap_err().rule, "atif");
    // Trajectory members: absent → reported attribute; the scored
    // trial's publication input is the transcript, never ATIF.
    let absent = capture::trajectory_members(None, None, Some("tx"));
    assert_eq!(
        absent.get("atif_absent").and_then(Json::as_bool),
        Some(true)
    );
    let mut s = slot(
        "exited{0}",
        Some(0),
        None,
        Disposition::Executed {
            resolved: true,
            end_cause: EndCause::Exited(0),
        },
        Some(verdict(1_000_000)),
    );
    // Missing-ATIF slot still scores.
    s.trajectory = capture::trajectory_members(None, None, Some("tx"));
    assert!(matches!(s.disposition, Disposition::Executed { .. }));
    assert_eq!(capture::publication_check_input(&s).unwrap(), "tx");
    // A scored trial without the transcript ref fails the check.
    s.trajectory = capture::trajectory_members(None, Some(("atif:x", Some("atif/1"))), None);
    assert_eq!(
        capture::publication_check_input(&s).unwrap_err().rule,
        "publication"
    );
    // An inconclusive trial doesn't require the member.
    s.disposition = capture::unreceipted_disposition();
    s.trajectory = capture::trajectory_members(None, None, None);
    assert_eq!(capture::publication_check_input(&s).unwrap(), "");
}

// ── AC-3 (AC-4.3-4; REQ-XC-207) — reconciliation ──────────────────────────────

#[test]
fn ac3_unreceipted_call_is_inconclusive_and_unscored() {
    let native = vec![
        NativeCall {
            seq: 0,
            kind: CallKind::Main,
        },
        NativeCall {
            seq: 1,
            kind: CallKind::Subagent,
        },
        NativeCall {
            seq: 2,
            kind: CallKind::Compaction,
        },
    ];
    let full: Vec<Json> = (0..3)
        .map(|k| receipt(k, "/v1/chat/completions", "forwarded"))
        .collect();
    assert!(capture::reconcile_receipts(&native, &full).is_empty());
    let missing = &full[..2];
    let unreceipted = capture::reconcile_receipts(&native, missing);
    assert_eq!(unreceipted.len(), 1);
    assert_eq!(unreceipted[0].kind, CallKind::Compaction);
    // A trial with a reward *and* an unreceipted call is inconclusive,
    // never scored — the precedence leg in the classifier.
    let o = classify_hosting(&HostingFacts {
        unreceipted_call: true,
        reward: Some(true),
        agent_exit: Some(0),
        ..Default::default()
    });
    assert_eq!(
        o.disposition.unwrap().as_str(),
        "inconclusive{infra:unreceipted_call}"
    );
}

// ── AC-4 (AC-5.6-3; REQ-XC-328) — ST4 self-modification ───────────────────────

#[test]
fn ac4_changed_tree_or_version_flags_and_stays_scored() {
    let st2 = St2Observation {
        version_stdout: "mini-swe-agent 2.4.6".into(),
        bundle_tree_sha256: "sha256:t".into(),
        config_sha256: "sha256:c".into(),
        binary_sha256: None,
    };
    let same = St4Observation {
        version_stdout: st2.version_stdout.clone(),
        install_tree_sha256: st2.bundle_tree_sha256.clone(),
    };
    assert!(capture::st4_verify(&st2, &same).is_empty());
    for altered in [
        St4Observation {
            version_stdout: "mini-swe-agent 2.4.7".into(),
            ..same.clone()
        },
        St4Observation {
            install_tree_sha256: "sha256:x".into(),
            ..same.clone()
        },
    ] {
        let flags = capture::st4_verify(&st2, &altered);
        assert_eq!(flags, vec![SlotFlag::HarnessSelfModified]);
        let o = classify_hosting(&HostingFacts {
            self_modified: true,
            agent_exit: Some(0),
            reward: Some(true),
            ..Default::default()
        });
        assert!(matches!(o.disposition, Some(Disposition::Executed { .. })));
        assert!(o.flags.contains(&SlotFlag::HarnessSelfModified));
    }
    // st4_plan re-runs the declared version probe + tree hash.
    let plan = st4_plan(&rec("mini-swe-agent")).unwrap();
    assert_eq!(plan.probes.len(), 2);
    assert!(plan.probes[0].argv[0].contains("mini-swe-agent"));
    assert!(plan.probes[1].argv[2].contains("sha256sum"));
}

// ── AC-5 (AC-5.7-1; REQ-XC-324) — the offline checker, per variant × harness ──

#[test]
fn ac5_battery_plan_and_recordings_are_committed() {
    let battery = load(&format!("{FIXTURES}/battery.json"));
    assert_eq!(
        battery.get("schema").and_then(Json::as_str),
        Some(BATTERY_SCHEMA)
    );
    assert!(check_battery(&battery, &["mini-swe-agent", "pi", "claude-code"]).is_empty());
    // The task is committed and never a TB task.
    let task = load(&format!("{FIXTURES}/task/task.json"));
    assert_eq!(
        task.get("task_id").and_then(Json::as_str),
        Some(smoke::SMOKE_TASK_ID)
    );
    assert_eq!(
        task.get("never_tb_task").and_then(Json::as_bool),
        Some(true)
    );
    // One scripted recording per harness in its own surface + streaming
    // mode; every doc passes the records-level shape check.
    for (h, stream) in [
        ("mini-swe-agent", false),
        ("pi", true),
        ("claude-code", true),
    ] {
        let d = recording(h);
        assert!(recording_shape(&d).is_empty(), "{h}");
        assert!(
            recording_surface(&d, route(h), stream).is_empty(),
            "{h} recording not in its own tool schema / streaming mode"
        );
    }
    // Replay settings: sequence + serve_recorded; SM-slow records latency.
    for v in SmokeVariant::ALL {
        let d = replay_decl(v);
        assert_eq!(d.get("keying").and_then(Json::as_str), Some("sequence"));
        assert_eq!(
            d.get("on_divergence").and_then(Json::as_str),
            Some("serve_recorded")
        );
    }
    assert_eq!(
        replay_decl(SmokeVariant::Slow)
            .get("latency")
            .and_then(Json::as_str),
        Some("recorded")
    );
}

#[test]
fn ac5_checker_passes_every_variant_for_each_harness() {
    for h in ["mini-swe-agent", "pi", "claude-code"] {
        let (rec, records) = pos_records(h);
        assert!(
            check_smoke(SmokeVariant::Pos, &rec, &records).is_empty(),
            "{h} SM-pos"
        );
        // SM-429 — 429 + Retry-After, then a retry receipt.
        let mut r429 = records.clone();
        if let Json::Obj(m) = &mut r429.receipts[0] {
            m.insert(
                "upstream".into(),
                Json::obj([
                    ("status", Json::Int(429)),
                    ("headers", Json::obj([("retry-after", Json::str("1"))])),
                ]),
            );
        }
        assert!(
            check_smoke(SmokeVariant::R429, &rec, &r429).is_empty(),
            "{h} SM-429"
        );
        // SM-402 — call 2 is refused_budget; end_cause budget_refused.
        let mut r402 = records.clone();
        if let Json::Obj(m) = &mut r402.receipts[1] {
            m.insert("disposition".into(), Json::str("refused_budget"));
        }
        if let Some(s) = &mut r402.slot {
            s.phases[0].end_cause = "budget_refused".into();
            s.phases[0].exit_code = None;
            s.disposition = Disposition::Executed {
                resolved: true,
                end_cause: EndCause::BudgetRefused,
            };
        }
        assert!(
            check_smoke(SmokeVariant::R402, &rec, &r402).is_empty(),
            "{h} SM-402"
        );
        // SM-slow — buffered* release modes, no client_gone.
        let mut rslow = records.clone();
        for r in rslow.receipts.iter_mut() {
            if let Json::Obj(m) = r {
                m.insert(
                    "released".into(),
                    Json::obj([
                        ("status", Json::Int(200)),
                        ("release_mode", Json::str("buffered_heartbeat")),
                    ]),
                );
            }
        }
        assert!(
            check_smoke(SmokeVariant::Slow, &rec, &rslow).is_empty(),
            "{h} SM-slow"
        );
        // SM-hang — agent_timeout, group killed, verifies.
        let mut rhang = records.clone();
        if let Some(s) = &mut rhang.slot {
            s.phases[0].end_cause = "agent_timeout".into();
            s.phases[0].exit_code = None;
            s.phases[0].deadline_enforced_ms = Some(60_000);
            s.disposition = Disposition::Executed {
                resolved: false,
                end_cause: EndCause::AgentTimeout,
            };
        }
        rhang.agent_terminated = true;
        assert!(
            check_smoke(SmokeVariant::Hang, &rec, &rhang).is_empty(),
            "{h} SM-hang"
        );
    }
}

#[test]
fn ac5_failing_fixtures_per_variant() {
    let (rec, records) = pos_records("mini-swe-agent");
    // SM-pos failures: undeclared denied host; receipts ≠ script length;
    // non-zero exit; self-modified tree; leak hits; unreceipted call.
    for (mut r, want) in [
        (records.clone(), "expected_denied"),
        (records.clone(), "receipts"),
        (records.clone(), "end_cause"),
        (records.clone(), "self_modified"),
        (records.clone(), "leak_scan"),
        (records.clone(), "unreceipted_call"),
    ] {
        match want {
            "expected_denied" => r.mediator_rows.push(denied("evil.example.com")),
            "receipts" => {
                r.receipts.pop();
                r.native_calls.pop();
            }
            "end_cause" => {
                if let Some(s) = &mut r.slot {
                    s.phases[0].end_cause = "exited{1}".into();
                    s.phases[0].exit_code = Some(1);
                }
            }
            "self_modified" => {
                r.st4 = Some(St4Observation {
                    version_stdout: "other".into(),
                    install_tree_sha256: "sha256:z".into(),
                });
            }
            "leak_scan" => r.leak_hits = 1,
            _ => r.native_calls.push(NativeCall {
                seq: 9,
                kind: CallKind::Title,
            }),
        }
        let fails = check_smoke(SmokeVariant::Pos, &rec, &r);
        assert!(fails.iter().any(|f| f.check == want), "{want}: {fails:?}");
    }
    // SM-429 without a 429; SM-402 without refused_budget; SM-slow with
    // a client_gone; SM-hang without termination.
    let mut b = records.clone();
    assert!(check_smoke(SmokeVariant::R429, &rec, &b)
        .iter()
        .any(|f| f.check == "inject_429"));
    assert!(check_smoke(SmokeVariant::R402, &rec, &b)
        .iter()
        .any(|f| f.check == "inject_402"));
    if let Json::Obj(m) = &mut b.receipts[0] {
        m.insert(
            "released".into(),
            Json::obj([
                ("status", Json::str("client_gone")),
                ("release_mode", Json::str("buffered")),
            ]),
        );
    }
    assert!(check_smoke(SmokeVariant::Slow, &rec, &b)
        .iter()
        .any(|f| f.check == "client_timeout"));
    let mut h = records.clone();
    if let Some(s) = &mut h.slot {
        s.phases[0].end_cause = "agent_timeout".into();
        s.phases[0].deadline_enforced_ms = Some(60_000);
    }
    assert!(check_smoke(SmokeVariant::Hang, &rec, &h)
        .iter()
        .any(|f| f.check == "terminated"));
}

// ── AC-6 (AC-5.3-6; REQ-XC-311) — the egress probe and the AG5 rule ───────────

#[test]
fn ac6_expected_denied_subset_and_ag5_no_grant() {
    let (adm, records) = pos_records("claude-code");
    assert!(mediator_subset(&adm, &records.mediator_rows).is_empty());
    // A granted non-model host fails AG5.
    let granted = vec![Json::obj([
        ("host", Json::str("telemetry.example.com")),
        ("decision", Json::str("granted")),
    ])];
    assert_eq!(mediator_subset(&adm, &granted)[0].check, "ag5_granted");
    // An admission without an egress probe log refuses.
    for h in ["mini-swe-agent", "pi", "claude-code"] {
        let r = rec(h);
        admission::validate(&r, &ValidationContext::first_pass())
            .unwrap_or_else(|e| panic!("{h}: {e}"));
        let mut stripped = r.clone();
        if let Json::Obj(m) = &mut stripped {
            if let Json::Obj(e) = m.get_mut("egress").unwrap() {
                e.remove("probe");
            }
        }
        assert_eq!(
            admission::validate(&stripped, &ValidationContext::first_pass())
                .unwrap_err()
                .rule,
            "egress_probe"
        );
        // A probe observation outside expected_denied refuses.
        let mut bad = r.clone();
        if let Json::Obj(m) = &mut bad {
            if let Json::Obj(e) = m.get_mut("egress").unwrap() {
                if let Json::Obj(pr) = e.get_mut("probe").unwrap() {
                    pr.insert(
                        "observed".into(),
                        Json::Arr(vec![Json::str("unlisted.example.org")]),
                    );
                }
            }
        }
        assert_eq!(
            admission::validate(&bad, &ValidationContext::first_pass())
                .unwrap_err()
                .rule,
            "egress_probe"
        );
    }
}

// ── AC-7 (AC-5.3-7; REQ-XC-312) — the model-socket assertion ──────────────────

#[test]
fn ac7_model_calls_arrive_on_the_model_socket() {
    for h in ["mini-swe-agent", "pi", "claude-code"] {
        let (rec, records) = pos_records(h);
        assert!(
            model_socket_assert(&rec, &records.mediator_rows, &records.receipts).is_empty(),
            "{h}"
        );
        // A model-host CONNECT (or relay-address request) fails.
        let model_host = if h == "claude-code" {
            "api.anthropic.com"
        } else {
            "api.openai.com"
        };
        let connect = vec![Json::obj([
            ("host", Json::str(model_host)),
            ("method", Json::str("CONNECT")),
            ("decision", Json::str("denied")),
        ])];
        let fails = model_socket_assert(&rec, &connect, &records.receipts);
        assert!(fails.iter().any(|f| f.check == "model_host"), "{h}");
        let relay_row = vec![Json::obj([
            ("host", Json::str("127.0.0.1:3128")),
            ("decision", Json::str("denied")),
        ])];
        assert!(model_socket_assert(&rec, &relay_row, &records.receipts)
            .iter()
            .any(|f| f.check == "model_host"));
    }
}

// ── AC-8 (AC-5.1-3; REQ-XC-203) — endpoint admission ──────────────────────────

#[test]
fn ac8_custom_base_url_evidenced_and_excluded_typed() {
    for h in ["mini-swe-agent", "pi", "claude-code"] {
        let r = rec(h);
        endpoint_admission(&r).unwrap();
        let (_, records) = pos_records(h);
        assert!(receipt_on_surface(&r, &records.receipts).is_empty(), "{h}");
    }
    // Google-format-only and vendor-hosted harnesses exclude n/a{endpoint}.
    for (surface, hosted) in [("google.gemini", false), ("openai.chat", true)] {
        let doc = Json::obj([
            (
                "harness",
                Json::obj([("vendor_hosted", Json::Bool(hosted))]),
            ),
            ("config", Json::obj([("surface", Json::str(surface))])),
        ]);
        assert_eq!(endpoint_admission(&doc).unwrap_err(), NA_ENDPOINT);
    }
    // A receipt not on the relay fails the evidence leg.
    let r = rec("mini-swe-agent");
    let bad = vec![receipt(0, "/v1/chat/completions", "forwarded")];
    let mut off = bad[0].clone();
    if let Json::Obj(m) = &mut off {
        if let Json::Obj(d) = m.get_mut("downstream").unwrap() {
            d.insert("host".into(), Json::str("api.openai.com"));
        }
    }
    assert_eq!(receipt_on_surface(&r, &[off])[0].check, "endpoint");
}

// ── AC-9 (AC-5.3-10; REQ-XC-321) — completion detection ───────────────────────

#[test]
fn ac9_end_causes_exit_codes_and_descriptive_status() {
    let (rec, records) = pos_records("mini-swe-agent");
    // SM-pos exits 0 with the code recorded; the native status parses
    // descriptively and never decides the outcome.
    assert_eq!(records.slot.as_ref().unwrap().phases[0].exit_code, Some(0));
    let status = native_status(
        "mini-swe-agent",
        "…\necho COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT\n",
    );
    assert_eq!(status.get("kind").and_then(Json::as_str), Some("submitted"));
    assert_eq!(
        native_status(
            "claude-code",
            "{\"type\":\"result\",\"subtype\":\"success\"}"
        )
        .get("kind")
        .and_then(Json::as_str),
        Some("result")
    );
    // A timeout without the enforced deadline recorded fails (only the
    // task deadline stops a harness).
    let mut r = records.clone();
    if let Some(s) = &mut r.slot {
        s.phases[0].end_cause = "agent_timeout".into();
        s.phases[0].deadline_enforced_ms = None;
    }
    assert!(check_smoke(SmokeVariant::Pos, &rec, &r)
        .iter()
        .any(|f| f.check == "deadline"));
    // Exited without the recorded exit code fails.
    let mut e = records.clone();
    if let Some(s) = &mut e.slot {
        s.phases[0].exit_code = None;
    }
    assert!(check_smoke(SmokeVariant::Pos, &rec, &e)
        .iter()
        .any(|f| f.check == "exit_code"));
}

// ── AC-10 (AC-5.8-1; REQ-XC-323) — the classifier truth table ─────────────────

#[test]
fn ac10_every_58_row_maps_through_one_classifier() {
    use hharness_runner::admission_build::InstallStep;
    // Row 1 — build failure at admission.
    let o = classify_hosting(&HostingFacts {
        build_failure: Some(InstallStep::Layer),
        ..Default::default()
    });
    assert_eq!(o.label.as_deref(), Some("n/a{install:layer}"));
    assert_eq!(o.retry, Retry::NewAdmission);
    // Row 2 — ST2 verify mismatch.
    let o = classify_hosting(&HostingFacts {
        st2_mismatch: true,
        ..Default::default()
    });
    assert_eq!(
        o.disposition.unwrap().as_str(),
        "withheld{admission:harness_layer_mismatch}"
    );
    assert_eq!(o.retry, Retry::No);
    // Row 3 — ST2 probe crash, healthy cell.
    let o = classify_hosting(&HostingFacts {
        st2_probe_crash: Some("sigsegv".into()),
        ..Default::default()
    });
    assert_eq!(
        o.disposition.unwrap().as_str(),
        "inconclusive{infra:harness_setup:sigsegv}"
    );
    assert_eq!(o.retry, Retry::EngineReplan);
    // Row 13 — relay/cell death.
    let o = classify_hosting(&HostingFacts {
        environment_died: true,
        ..Default::default()
    });
    assert_eq!(
        o.disposition.unwrap().as_str(),
        "inconclusive{infra:environment_died}"
    );
    assert_eq!(o.retry, Retry::EngineReplan);
    // Premature refusal (§3.3 precedence).
    let o = classify_hosting(&HostingFacts {
        premature_refusal: true,
        reward: Some(true),
        ..Default::default()
    });
    assert_eq!(
        o.disposition.unwrap().as_str(),
        "inconclusive{infra:budget_reservation}"
    );
    // Row 4 — non-zero exit after a model-route failure.
    for d in [
        "upstream_error",
        "upstream_timeout",
        "stream_broken",
        "proxy_aborted",
        "refused_auth",
    ] {
        let o = classify_hosting(&HostingFacts {
            agent_exit: Some(1),
            last_call_disposition: Some(d.into()),
            ..Default::default()
        });
        assert_eq!(
            o.disposition.unwrap().as_str(),
            format!("inconclusive{{infra:model_route:{d}}}"),
            "{d}"
        );
        assert_eq!(o.retry, Retry::EngineReplan);
    }
    // Row 5 — non-zero exit otherwise verifies + scores.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(2),
        last_call_disposition: Some("forwarded".into()),
        reward: Some(false),
        ..Default::default()
    });
    assert_eq!(o.disposition.unwrap().as_str(), "executed{unresolved}");
    assert_eq!(o.retry, Retry::No);
    // Row 6 — hang past deadline.
    let o = classify_hosting(&HostingFacts {
        agent_timed_out: true,
        reward: Some(false),
        ..Default::default()
    });
    assert!(matches!(
        o.disposition,
        Some(Disposition::Executed {
            end_cause: EndCause::AgentTimeout,
            ..
        })
    ));
    // Row 7 — slot budget refusal.
    let o = classify_hosting(&HostingFacts {
        budget_refused: true,
        reward: Some(true),
        ..Default::default()
    });
    assert!(matches!(
        o.disposition,
        Some(Disposition::Executed {
            resolved: true,
            end_cause: EndCause::BudgetRefused,
        })
    ));
    // Row 8 — survived upstream failure: scored + covariate.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(0),
        upstream_failure_survived: true,
        reward: Some(true),
        ..Default::default()
    });
    assert!(matches!(o.disposition, Some(Disposition::Executed { .. })));
    assert!(o.covariates.iter().any(|c| c.contains("upstream")));
    // Row 9 — malformed output: scored at task level.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(0),
        malformed_output: true,
        reward: Some(false),
        ..Default::default()
    });
    assert!(matches!(o.disposition, Some(Disposition::Executed { .. })));
    // Row 10 — replay divergence: finding only, never paid.
    let o = classify_hosting(&HostingFacts {
        replay_diverged: true,
        ..Default::default()
    });
    assert_eq!(o.label.as_deref(), Some("smoke_finding{replay_diverged}"));
    assert_eq!(o.retry, Retry::NeverPaid);
    // Rows 11–12 — declared denial is a residual; undeclared flags.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(0),
        reward: Some(true),
        undeclared_denied: vec!["unlisted.example.org".into()],
        ..Default::default()
    });
    assert!(o.flags.contains(&SlotFlag::UndeclaredEgressDenied));
    // Row 14 — self-modification: flag + scored.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(0),
        self_modified: true,
        reward: Some(true),
        ..Default::default()
    });
    assert!(o.flags.contains(&SlotFlag::HarnessSelfModified));
    // Row 15 — client_gone: charged covariate.
    let o = classify_hosting(&HostingFacts {
        agent_exit: Some(0),
        client_gone: true,
        reward: Some(true),
        ..Default::default()
    });
    assert!(o.covariates.iter().any(|c| c.contains("client_gone")));
}

#[test]
fn ac10_records_in_recompute_equals_stored_dispositions() {
    // Passing fixture: recompute the facts from the same records the
    // disposition came from — classify equals the stored spelling.
    let (rec, records) = pos_records("mini-swe-agent");
    let s = records.slot.as_ref().unwrap();
    let expected = smoke::expected_denied(&rec);
    let facts = facts_from_records(
        s,
        &records.native_calls,
        &records.receipts,
        &records.mediator_rows,
        &expected,
    );
    let o = classify_hosting(&facts);
    assert_eq!(o.disposition.unwrap().as_str(), s.disposition.as_str());
    // A stored inconclusive{infra:environment_died} recomputes.
    let s2 = slot(
        "exited{0}",
        Some(0),
        None,
        Disposition::Inconclusive {
            cause: InconclusiveCause::Infra(InfraCause::EnvironmentDied),
        },
        None,
    );
    let f2 = facts_from_records(&s2, &[], &[], &[], &[]);
    assert_eq!(
        classify_hosting(&f2).disposition.unwrap().as_str(),
        s2.disposition.as_str()
    );
    // A stored withheld{admission:harness_layer_mismatch} recomputes.
    let s3 = slot(
        "exited{0}",
        Some(0),
        None,
        Disposition::Withheld {
            reason: WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch),
        },
        None,
    );
    let f3 = facts_from_records(&s3, &[], &[], &[], &[]);
    assert_eq!(
        classify_hosting(&f3).disposition.unwrap().as_str(),
        s3.disposition.as_str()
    );
    // An unreceipted fixture recomputes to the REQ-XC-207 disposition.
    let mut s4 = slot(
        "exited{0}",
        Some(0),
        None,
        capture::unreceipted_disposition(),
        Some(verdict(0)),
    );
    s4.phases[0].end_cause = "exited{0}".into();
    let f4 = facts_from_records(
        &s4,
        &[NativeCall {
            seq: 5,
            kind: CallKind::Main,
        }],
        &[],
        &[],
        &[],
    );
    assert_eq!(
        classify_hosting(&f4).disposition.unwrap().as_str(),
        "inconclusive{infra:unreceipted_call}"
    );
}

// ── AC-11 (AC-5.7-4; REQ-XC-329) — offline battery; cited live legs ───────────

#[test]
fn ac11_check_script_catches_a_seed_live_test() {
    // The committed grep battery must fail on a hosting live test
    // without an ADR-0363 ignore citation.
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts/check-hosting-offline.sh");
    let script = script.canonicalize().unwrap();
    assert!(script.exists(), "{script:?}");
    let out = std::process::Command::new("bash")
        .arg(&script)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    // The seeded fixture must fail the scan.
    let out = std::process::Command::new("bash")
        .arg(&script)
        .arg("--self-test")
        .output()
        .unwrap();
    assert!(out.status.success(), "--self-test must pass");
}

/// $0 live smoke leg — executes only on the phase backend in W1 (R6.32).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh smoke — the $0 replay smoke battery (SM-pos/429/402/slow/hang × the three first-pass harnesses) on the provisioned phase backend; its records are the AG3/AG5/AG6/AG8 evidence (W1, R6.32)"]
fn live_smoke_battery_leg() {}

/// $0 live build leg — the smoke task's image layer build on the phase
/// backend.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh smoke-build — the hh-smoke/write-file layer build on the provisioned phase backend (W1, R6.32)"]
fn live_smoke_build_leg() {}
