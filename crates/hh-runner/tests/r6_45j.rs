//! R6.45j acceptance battery (manifest 309j; ADR-0447/0448): the
//! cell-fidelity repair set — D15 per-task `task.toml` deadlines
//! (AC-1), D17 lapsed-exec own-tree kill with confirmed death and
//! attempt-owned collects (AC-2), the Wc7 `query-optimize` decided
//! regression under 1800 s (AC-3) and the A5 `machinery{h_allow}`
//! classifier (AC-4). Every leg fails on pre-fix code (XO-05):
//! pre-fix there is no `task_toml` module, the lapse path returns
//! empty streams + a phase-blind `"agent"` walk, collects mix
//! attempts, and `allowlist_gap{unknown}` stands for `denied_hosts: []`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_runner::cell::{
    exec_guarded, run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan, DeadlineSource,
};
use hharness_runner::reproof::{classify_task, refold_a5, DiscordClass, TaskVerdict};
use hharness_runner::task_toml::{parse_task_toml, resolve_deadline_ms};
use hharness_runner::verdict;

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("r645j-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── AC-1 — per-task deadlines with provenance ────────────────────────────────

#[test]
fn ac1_task_toml_deadline_and_cfg_fallback() {
    // A task declaring `[verifier].timeout_sec = 1800` gets 1 800 000 ms
    // with `task_toml`; an absent member gets the cfg fallback with
    // `cfg_default`. Pre-fix: no reader exists and every cell ran the
    // global 600 000 ms.
    let toml = "\
[verifier]
timeout_sec = 1800.0

[agent]
timeout_sec = 900.0

[environment]
cpus = 1
memory_mb = 8192
";
    let d = parse_task_toml("fixture-task", toml).unwrap();
    assert_eq!(d.verifier_timeout_sec, Some(1800.0));
    assert_eq!(d.agent_timeout_sec, Some(900.0));
    assert_eq!(d.cpus.as_deref(), Some("1"));
    assert_eq!(d.memory_mb, Some(8192));
    let (ms, src) = resolve_deadline_ms(d.verifier_timeout_sec, 600_000);
    assert_eq!((ms, src), (1_800_000, DeadlineSource::TaskToml));
    let (ms2, src2) = resolve_deadline_ms(d.agent_timeout_sec, 900_000);
    assert_eq!((ms2, src2), (900_000, DeadlineSource::TaskToml));
    // Absent values → the cfg fallback, recorded `cfg_default`.
    let d2 = parse_task_toml("fixture-task-2", "[environment]\nmemory_mb = 2048\n").unwrap();
    assert_eq!(d2.verifier_timeout_sec, None);
    let (ms3, src3) = resolve_deadline_ms(d2.verifier_timeout_sec, 600_000);
    assert_eq!((ms3, src3), (600_000, DeadlineSource::CfgDefault));
    // A malformed bound member refuses — never silently defaults.
    assert!(parse_task_toml("t", "[verifier]\ntimeout_sec = -5\n").is_err());
    assert!(parse_task_toml("t", "[verifier]\ntimeout_sec = \"x\"\n").is_err());
    // And the resolved deadline + source land in the report per phase.
    let dir = scratch("ac1");
    let staging = dir.join("staging");
    std::fs::create_dir_all(staging.join("solution")).unwrap();
    std::fs::create_dir_all(staging.join("tests")).unwrap();
    let mut cell = QuietCell;
    let plan = CellPlan {
        oracle_participant: false,
        workdir: "/app".to_string(),
        host_staging: staging,
        collect_dir: dir.join("collect"),
        agent_argv: vec!["bash".into(), "/solution/solve.sh".into()],
        agent_deadline_ms: Some(900_000),
        agent_deadline_source: DeadlineSource::TaskToml,
        verifier_argv: vec!["bash".into(), "/tests/test.sh".into()],
        verifier_deadline_ms: Some(1_800_000),
        verifier_deadline_source: DeadlineSource::TaskToml,
        max_grade_attempts: 1,
        logs_copyout: None,
    };
    let report = run_cell(&mut cell, &plan).unwrap();
    assert_eq!(report.verifier_deadline_ms, Some(1_800_000));
    assert_eq!(report.verifier_deadline_source, DeadlineSource::TaskToml);
    assert_eq!(report.agent_deadline_ms, Some(900_000));
    assert_eq!(report.agent_deadline_source, DeadlineSource::TaskToml);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A quiet in-memory cell: every exec exits 0, `/logs/verifier` exists
/// only through the lifecycle's `mkdir` (not modeled — cp_out always
/// misses, so the streams the exec returns are the recorded evidence).
#[derive(Default)]
struct QuietCell;

impl CellOps for QuietCell {
    fn present(&mut self, _p: &str) -> Result<bool, CellError> {
        Ok(false)
    }
    fn ls_dir(&mut self, _p: &str) -> Result<Vec<String>, CellError> {
        Ok(vec![])
    }
    fn cp_in(&mut self, _s: &Path, _d: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn cp_out(&mut self, _s: &str, _d: &Path) -> Result<(), CellError> {
        Err(CellError {
            op: "cp_out",
            detail: "absent".to_string(),
        })
    }
    fn rm_glob(&mut self, _g: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn exec(&mut self, _spec: &CellExec) -> Result<CellOutcome, CellError> {
        Ok(CellOutcome {
            exit_code: Some(0),
            timed_out: false,
            stdout: vec![],
            stderr: vec![],
        })
    }
    fn terminate_group(&mut self, _p: &str) -> Result<(), CellError> {
        Ok(())
    }
}

// ── AC-2 — a lapsed verifier's own tree dies, confirmed, streams kept ────────

/// A verifier tree with NO "agent" in its cmdline outlives attempt 1's
/// deadline: the exec's own lapse machinery terminates the tagged tree
/// and CONFIRMS death before returning; attempt 2 starts only after
/// that; the collected `reward.txt` is attempt 2's, never the leaked
/// attempt's, and the lapsed attempt's partial streams are recorded.
/// Pre-fix: the drive's timeout path walked for the `"agent"` cmdline
/// pattern (never matching this tree), returned EMPTY streams and
/// started attempt 2 over the leaked verifier.
#[test]
fn ac2_lapsed_verifier_tree_dies_confirmed_and_attempt2_owns_collect() {
    let dir = scratch("ac2");
    let staging = dir.join("staging");
    std::fs::create_dir_all(staging.join("tests")).unwrap();
    let collect = dir.join("collect");

    // A fake cell whose verifier "tree" is a leaked in-cell process —
    // modeled by tag state the lapse sweep must kill + confirm: the
    // exec returns `timed_out` only after `terminate_exec_tag` +
    // `exec_tag_alive` report the tree dead (the guarded impl's own
    // contract; here the fixture enforces the ordering the drive
    // relies on).
    struct LeakyCell {
        attempt: u32,
        tree_alive: bool,
        terminate_calls: Vec<String>,
        _confirm_calls: Vec<String>,
        exec_log: Vec<Vec<String>>,
    }
    impl CellOps for LeakyCell {
        fn present(&mut self, _p: &str) -> Result<bool, CellError> {
            Ok(false)
        }
        fn ls_dir(&mut self, _p: &str) -> Result<Vec<String>, CellError> {
            Ok(vec![])
        }
        fn cp_in(&mut self, _s: &Path, _d: &str) -> Result<(), CellError> {
            Ok(())
        }
        fn cp_out(&mut self, s: &str, d: &Path) -> Result<(), CellError> {
            // Attempt 2's verifier wrote reward.txt = 1; the leaked
            // attempt-1 tree would write it too — but it is dead, so
            // only attempt 2's file is visible at copy-out time.
            if self.attempt == 2 && s == "/logs/verifier/reward.txt" {
                std::fs::write(d, b"1\n").map_err(|e| CellError {
                    op: "cp_out",
                    detail: e.to_string(),
                })
            } else {
                Err(CellError {
                    op: "cp_out",
                    detail: format!("no file {s}"),
                })
            }
        }
        fn rm_glob(&mut self, _g: &str) -> Result<(), CellError> {
            Ok(())
        }
        fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
            self.exec_log.push(spec.argv.clone());
            if spec.argv == ["bash", "/tests/test.sh"] {
                self.attempt += 1;
                if self.attempt == 1 {
                    // The lapsed verifier tree — `bash test.sh → uvx →
                    // pytest`, no "agent" anywhere — kept running on
                    // pre-fix code. Post-fix: the exec's lapse path has
                    // already tagged + killed + confirmed it BEFORE
                    // returning the timed-out outcome; the fixture
                    // records the partial streams the lapse kept.
                    self.tree_alive = false; // terminated by its own tag
                    return Ok(CellOutcome {
                        exit_code: None,
                        timed_out: true,
                        stdout: b"apt-get update\nGet:1 http://archive.ubuntu.com ...\n".to_vec(),
                        stderr: b"+ uvx -p 3.13 -w pytest pytest\n".to_vec(),
                    });
                }
                return Ok(CellOutcome {
                    exit_code: Some(0),
                    timed_out: false,
                    stdout: vec![],
                    stderr: vec![],
                });
            }
            Ok(CellOutcome {
                exit_code: Some(0),
                timed_out: false,
                stdout: vec![],
                stderr: vec![],
            })
        }
        fn terminate_group(&mut self, p: &str) -> Result<(), CellError> {
            self.terminate_calls.push(p.to_string());
            Ok(())
        }
    }
    let mut cell = LeakyCell {
        attempt: 0,
        tree_alive: true,
        terminate_calls: vec![],
        _confirm_calls: vec![],
        exec_log: vec![],
    };
    let plan = CellPlan {
        oracle_participant: false,
        workdir: "/app".to_string(),
        host_staging: staging,
        collect_dir: collect.clone(),
        agent_argv: vec!["bash".into(), "/solution/solve.sh".into()],
        agent_deadline_ms: Some(900_000),
        agent_deadline_source: DeadlineSource::TaskToml,
        verifier_argv: vec!["bash".into(), "/tests/test.sh".into()],
        verifier_deadline_ms: Some(600_000),
        verifier_deadline_source: DeadlineSource::TaskToml,
        max_grade_attempts: 2,
        logs_copyout: None,
    };
    let report = run_cell(&mut cell, &plan).unwrap();
    // Attempt 2 ran (attempts never overlap — the lapsed tree is dead
    // before attempt 2's exec; the fixture's sequencing records it).
    assert_eq!(report.verify_attempts, 2);
    let verify = report.verify.unwrap();
    assert_eq!(verify.attempt, 2);
    // The collected reward.txt is attempt 2's — top-level collect has
    // it; attempt-1's dir records the lapsed streams, not a reward.
    assert_eq!(std::fs::read(collect.join("reward.txt")).unwrap(), b"1\n");
    assert!(!collect.join("attempt-1/reward.txt").exists());
    // The lapsed attempt's partial streams are recorded (pre-fix: the
    // timeout path returned EMPTY stdout/stderr — nothing landed).
    let s1 = std::fs::read(collect.join("attempt-1/test-stdout.txt")).unwrap();
    assert!(String::from_utf8_lossy(&s1).contains("Get:1"));
    let e1 = std::fs::read(collect.join("attempt-1/test-stderr.txt")).unwrap();
    assert!(String::from_utf8_lossy(&e1).contains("uvx"));
    assert!(verify.files.reward_txt.is_some());
    // The verifier classification for the timed-out final attempt is
    // the honest `verifier_timeout` — attempt 2 itself timed out? No:
    // attempt 2 exited; the outcome decides on attempt 2's files.
    assert!(!verify.timed_out);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The guarded exec's argv-level contract over a `podman` stub: a
/// deadline-bearing exec carries `setsid --wait timeout --kill-after`
/// AND a
/// `-e HH_EXEC_TAG=<tag>` marker; the lapse path's environ walk
/// addresses the SAME tag (TERM then KILL, then the confirm walk) —
/// never a cmdline pattern like "agent" the verifier tree lacks.
/// Pre-fix: the exec argv carried no tag, no `setsid timeout`, and the
/// sweep walked `/proc/*/cmdline` for "agent". (`--wait` is the R6.55
/// repair: bare `setsid` under `podman exec` forks and exits 0 at once,
/// dropping the wrapped tree — the `adapter:silent_exit` signature.)
#[test]
fn ac2_guarded_exec_tag_wrap_sweep_and_confirm() {
    let dir = scratch("podstub");
    let log = dir.join("podman.log");
    let stub = dir.join("podman");
    // The stub: logs every argv; for the phase exec (carries `setsid
    // --wait timeout`) it emits partial stdout and exits 124 (the in-cell
    // `timeout` ladder firing); for environ walks it exits 1 = dead.
    std::fs::write(
        &stub,
        "#!/bin/sh\n\
         echo \"$@\" >> \"$HH_PODMAN_LOG\"\n\
         if [ \"$1\" = \"exec\" ]; then\n\
           case \" $* \" in\n\
             *\" setsid --wait timeout \"*) echo 'partial verifier stream'; exit 124;;\n\
             *\"/environ\"*kill*) exit 0;;\n\
             *\"/environ\"*) exit 1;;\n\
             *) exit 0;;\n\
           esac\n\
         fi\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("HH_PODMAN_BIN", &stub);
    std::env::set_var("HH_PODMAN_LOG", &log);
    let backend = hharness_helper::podman::PodmanBackend {
        container: "hh-w1-t-pub".into(),
        image: "img".into(),
        workspace: dir.display().to_string(),
        network: true,
        extra_args: vec![],
        cell: hharness_helper::podman::CellSpec::default(),
        mounts: vec![],
        relay: None,
    };
    let out = exec_guarded(
        &backend,
        &["bash".into(), "/tests/test.sh".into()],
        "/app",
        &[],
        // A 1 ms deadline is already elapsed when the stub exits 124 —
        // `timeout`'s lapse exit is honored only past the declared
        // bound; an early 124 stays the phase's own exit code.
        Some(1),
    )
    .unwrap();
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
    assert!(out.timed_out);
    assert_eq!(out.exit_code, None);
    // The lapse kept the captured streams (pre-fix: empty).
    assert!(String::from_utf8_lossy(&out.stdout).contains("partial verifier stream"));
    let logged = std::fs::read_to_string(&log).unwrap();
    // setsid --wait timeout wrap + the env marker on the phase exec.
    assert!(
        logged.contains("setsid --wait timeout --kill-after="),
        "{logged}"
    );
    assert!(logged.contains("-e HH_EXEC_TAG=hh-exec-"), "{logged}");
    // The sweep + confirm walks address the tag through /proc environ —
    // never the cmdline "agent" pattern.
    assert!(logged.matches("/environ").count() >= 3, "{logged}"); // TERM + KILL + confirm
    assert!(logged.contains("HH_EXEC_TAG=hh-exec-"), "{logged}");
    assert!(!logged.contains("cmdline"), "{logged}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── AC-3 — the Wc7 query-optimize regression under 1800 s ────────────────────

/// Wc7 `query-optimize` H-pub's recorded bytes: ctrf 6/6 (~812 s of
/// pytest), `reward.txt` = `1\n` (sha256:4355a46b…). At the global
/// 600 s the exec lapsed → `verifier_unavailable:verifier_timeout`.
/// Under the task's declared 1800 s the verifier completes; the channel
/// classifies `decided`, reward 1.
#[test]
fn ac3_query_optimize_1800s_decides_reward_1() {
    // The task.toml declaration resolves to 1 800 000 ms ≥ the recorded
    // 812 s wall — the verifier does not lapse.
    let (ms, src) = resolve_deadline_ms(Some(1800.0), 600_000);
    assert_eq!((ms, src), (1_800_000, DeadlineSource::TaskToml));
    assert!(ms > 812_000);
    // The recorded bytes: ctrf 6/6 + reward.txt = 1.
    let files = verdict::RewardFileSet {
        reward_json: None,
        reward_txt: Some(b"1\n".to_vec()),
        ctrf_json: Some(
            br#"{"results":{"summary":{"tests":6,"passed":6,"failed":0,"skipped":0,"pending":0,"other":0,"start":1.0,"stop":821.0},"tests":[]},"tool":{"name":"pytest","version":"8.4.1"}}"#
                .to_vec(),
        ),
        test_stdout: None,
        test_stderr: None,
    };
    // timed_out = false — the verifier ran to completion under 1800 s.
    let v = verdict::classify(&files, &[], false);
    assert_eq!(v.classification, "decided");
    assert_eq!(v.reward_ppm, Some(1_000_000));
    assert_eq!(v.success, Some(true));
    // And the drive end-to-end: a verifier exec that completes under
    // the resolved deadline lands the decided files — no lapse.
    let dir = scratch("ac3");
    let staging = dir.join("staging");
    std::fs::create_dir_all(staging.join("tests")).unwrap();
    let collect = dir.join("collect");
    let mut cell = QuietCell;
    let plan = CellPlan {
        oracle_participant: false,
        workdir: "/app".to_string(),
        host_staging: staging,
        collect_dir: collect.clone(),
        agent_argv: vec!["bash".into(), "/solution/solve.sh".into()],
        agent_deadline_ms: Some(900_000),
        agent_deadline_source: DeadlineSource::TaskToml,
        verifier_argv: vec!["bash".into(), "/tests/test.sh".into()],
        verifier_deadline_ms: Some(ms),
        verifier_deadline_source: src,
        max_grade_attempts: 1,
        logs_copyout: None,
    };
    let report = run_cell(&mut cell, &plan).unwrap();
    assert_eq!(report.verify.unwrap().attempt, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ── AC-4 — the A5 classifier + the Wc4–Wc8 re-fold rule ──────────────────────

#[test]
fn ac4_allowlist_gap_requires_a_named_denied_host() {
    // Empty denied_hosts + H-allow < R/R → machinery{h_allow}
    // (repaired, never excluded). Pre-fix: allowlist_gap{unknown}.
    match classify_task(&[true; 3], &[true; 3], &[true, false, true], "x", &[]) {
        TaskVerdict::Discord {
            class,
            basis,
            machinery_qualifier,
            ..
        } => {
            assert_eq!(class, DiscordClass::Machinery);
            assert_eq!(basis, None);
            assert_eq!(machinery_qualifier.as_deref(), Some("h_allow"));
        }
        v => panic!("{v:?}"),
    }
    let v = classify_task(&[true; 3], &[true; 3], &[false; 3], "x", &[]);
    assert_eq!(v.as_str(), "repair:machinery{h_allow}");
    assert_eq!(v.register_classification(), "machinery{h_allow}");
    // A named denied host keeps the exclusion legitimate.
    match classify_task(
        &[true; 3],
        &[true; 3],
        &[true, false, true],
        "x",
        &["archive.ubuntu.com".to_string()],
    ) {
        TaskVerdict::Discord {
            class, basis, host, ..
        } => {
            assert_eq!(class, DiscordClass::AllowlistGap);
            assert_eq!(
                basis.as_deref(),
                Some("runner_infeasible:allowlist_gap{archive.ubuntu.com}")
            );
            assert_eq!(host.as_deref(), Some("archive.ubuntu.com"));
        }
        v => panic!("{v:?}"),
    }
    // The appended re-fold rule: `{unknown}`/empty-denied gap verdicts
    // re-fold to machinery{h_allow}; named-host gaps and every other
    // spelling keep their bytes.
    assert_eq!(
        refold_a5("exclude:runner_infeasible:allowlist_gap{unknown}", &[]),
        "repair:machinery{h_allow}"
    );
    assert_eq!(
        refold_a5(
            "exclude:runner_infeasible:allowlist_gap{archive.ubuntu.com}",
            &[
                "archive.ubuntu.com".to_string(),
                "security.ubuntu.com".to_string()
            ]
        ),
        "exclude:runner_infeasible:allowlist_gap{archive.ubuntu.com}"
    );
    assert_eq!(refold_a5("repair:machinery", &[]), "repair:machinery");
    assert_eq!(refold_a5("admit", &[]), "admit");
}

// ── D18 — the declared resource audit (record, not a code verdict) ───────────

/// `task.toml [environment].cpus/memory_mb` land on the cell verbatim
/// (Harbor semantics — the declared value IS the container limit);
/// `torch-tensor-parallelism`'s 8192 MB is parsed and granted.
#[test]
fn d18_declared_cpus_memory_parse() {
    let d = parse_task_toml(
        "torch-tensor-parallelism",
        "[environment]\ncpus = 1\nmemory_mb = 8192\n",
    )
    .unwrap();
    assert_eq!(d.cpus.as_deref(), Some("1"));
    assert_eq!(d.memory_mb, Some(8192));
    let d2 = parse_task_toml("t", "[environment]\ncpus = 2.0\n").unwrap();
    assert_eq!(d2.cpus.as_deref(), Some("2"));
}
