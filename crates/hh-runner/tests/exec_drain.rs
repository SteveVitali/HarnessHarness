//! The Wc9 exec-drain repair battery (XO-05 — every leg fails on
//! pre-fix code). Diagnosis
//! `docs/build/reports/2026-11-16-pypi-server-discordance-diagnosis.json`:
//! `exec_guarded` spawned `podman exec` with `Stdio::piped()` on both
//! stdout and stderr but never drained either while the child ran —
//! `read_to_end` ran only after `try_wait` observed the exit. Once a
//! phase's cumulative output crossed the OS pipe bound (~64 KiB ± the
//! podman/conmon slack) the writer blocked: mid-script for the
//! in-cell-freeze shape (pypi-server's ~78 KB oracle transcript) or at
//! the tail for the return-hang shape (make-doom-for-mips' ~67.7 KB —
//! all the work done, the exec unable to finish delivering). The
//! in-cell `timeout` ladder then lapsed the exec and kill-on-lapse
//! recorded `agent_timeout` — the only two >~60 KB transcript members
//! were the only two `agent_timeout` members of the Wc9 cohort.
//!
//! The `podman` stub stands in for the exec client: it writes to the
//! same piped stdout/stderr `exec_guarded` must drain, so its own
//! writes block at the real OS pipe bound on pre-fix code.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use hharness_runner::cell::{
    exec_guarded, run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan, DeadlineSource,
};
use hharness_runner::disposition::EndCause;

/// `HH_PODMAN_BIN`/`HH_PODMAN_LOG` are process-global — the stub legs
/// serialize on this lock so parallel test scheduling never crosses two
/// stub installs.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("execdrain-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn backend(dir: &std::path::Path) -> hharness_helper::podman::PodmanBackend {
    hharness_helper::podman::PodmanBackend {
        container: "hh-w1-t-drain".into(),
        image: "img".into(),
        workspace: dir.display().to_string(),
        network: false,
        extra_args: vec![],
        cell: hharness_helper::podman::CellSpec::default(),
        mounts: vec![],
        relay: None,
    }
}

/// Install the `podman` stub: it logs every argv; the deadline-bearing
/// phase exec (carrying `setsid --wait timeout`) runs `body` — the emit/sleep
/// script that produces the oversized streams; the lapse sweep's
/// environ walks exit 0 (terminated) and the confirm walk exits 1 (no
/// tagged survivor). POSIX `dd|tr` emits — `head -c`/`yes` aren't
/// portable across the dev/CI hosts.
fn install_stub(dir: &std::path::Path, body: &str) -> (PathBuf, PathBuf) {
    let log = dir.join("podman.log");
    let stub = dir.join("podman");
    let script = format!(
        "#!/bin/sh\n\
         echo \"$@\" >> \"$HH_PODMAN_LOG\"\n\
         if [ \"$1\" = \"exec\" ]; then\n\
         \x20 case \" $* \" in\n\
         \x20   *\" setsid --wait timeout \"*) {body};;\n\
         \x20   *\"/environ\"*kill*) exit 0;;\n\
         \x20   *\"/environ\"*) exit 1;;\n\
         \x20   *) exit 0;;\n\
         \x20 esac\n\
         fi\nexit 0\n"
    );
    std::fs::write(&stub, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("HH_PODMAN_BIN", &stub);
    std::env::set_var("HH_PODMAN_LOG", &log);
    (stub, log)
}

/// Emit `kib` KiB of `ch` — the writer is the STUB PROCESS ITSELF: a
/// `dd|tr` builds one 4 KiB block, then the shell's own `printf`
/// builtins stream it. The stream ends with the stub's death, exactly
/// like an in-cell writer killed by the tag sweep — a detached helper
/// (`dd` writing the pipe directly) would survive `child.kill()` and
/// keep feeding a drained pipe, leaking post-kill bytes a real dead
/// exec's survivors cannot produce. `redir` rides on `printf` (e.g.
/// `" >&2"` for stderr).
fn emit_shell(kib: u64, ch: char, redir: &str) -> String {
    format!(
        "b=$(dd if=/dev/zero bs=4096 count=1 2>/dev/null | tr '\\0' '{ch}'); \
         i=0; while [ $i -lt {} ]; do printf '%s' \"$b\"{redir}; i=$((i+1)); done",
        kib / 4
    )
}

// ── XO-05a — the return-hang leg (the make-doom-for-mips shape) ────────

/// A phase that COMPLETED — all ~160 KB of transcript produced — but
/// whose output the exec cannot finish delivering: the tail sits past
/// the ~64 KiB pipe bound, so on pre-fix code the stub blocks on its
/// last writes, `podman exec` never exits, and the hard-deadline kill
/// lapses a finished phase into `agent_timeout`. Post-fix the drains
/// consume every byte as it arrives: the exec exits 0 promptly with the
/// full stdout AND stderr captured.
#[test]
fn xo05_return_hang_completed_phase_delivers_full_streams() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("hang");
    const OUT_KIB: u64 = 160; // well past the 64 KiB bound
    const ERR_KIB: u64 = 40;
    let body = format!(
        "{}; {}; exit 0",
        emit_shell(OUT_KIB, 'A', ""),
        emit_shell(ERR_KIB, 'B', " >&2")
    );
    let (_stub, log) = install_stub(&dir, &body);
    let b = backend(&dir);
    let deadline_ms = 3_000;
    let t0 = std::time::Instant::now();
    let out = exec_guarded(
        &b,
        &["bash".into(), "/solution/solve.sh".into()],
        "/app",
        &[],
        Some(deadline_ms),
    )
    .unwrap();
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
    // The completed phase exits 0 with BOTH full streams — pre-fix this
    // leg lapses: `timed_out`, `exit_code: None`, ~64 KiB partial stdout.
    assert!(!out.timed_out, "lapsed a completed phase: {out:?}");
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.stdout.len(), (OUT_KIB * 1024) as usize);
    assert_eq!(out.stderr.len(), (ERR_KIB * 1024) as usize);
    assert!(out.stdout.iter().all(|b| *b == b'A'));
    assert!(out.stderr.iter().all(|b| *b == b'B'));
    // Prompt return — the signature is that the call CANNOT come back
    // before the deadline + grace + slack lapse path on pre-fix code.
    assert!(
        t0.elapsed() < std::time::Duration::from_millis(deadline_ms),
        "exec took {:?} — past the {deadline_ms} ms deadline",
        t0.elapsed()
    );
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("setsid --wait timeout"), "{logged}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── XO-05b — the in-cell-freeze leg (the pypi-server shape) ────────────

/// The writer still mid-script: ~256 KB of stdout produced
/// continuously while the phase runs — on pre-fix code the writer
/// stalls the moment the unread pipe fills (~64 KiB in), the script
/// never advances (the pypi-server freeze between two network steps:
/// 841 s of zero egress to the lapse), the exec never exits, and the
/// deadline lapses it. Post-fix the drains keep the pipe empty — the
/// writer runs to completion and exits 0.
#[test]
fn xo05_in_cell_writer_never_blocks_past_the_pipe_bound() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("freeze");
    const OUT_KIB: u64 = 256;
    let body = format!("{}; exit 0", emit_shell(OUT_KIB, 'C', ""));
    let (_stub, _log) = install_stub(&dir, &body);
    let b = backend(&dir);
    let deadline_ms = 3_000;
    let t0 = std::time::Instant::now();
    let out = exec_guarded(
        &b,
        &["bash".into(), "/solution/solve.sh".into()],
        "/app",
        &[],
        Some(deadline_ms),
    )
    .unwrap();
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
    assert!(!out.timed_out, "writer blocked at the pipe bound: {out:?}");
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.stdout.len(), (OUT_KIB * 1024) as usize);
    assert!(out.stdout.iter().all(|b| *b == b'C'));
    assert!(
        t0.elapsed() < std::time::Duration::from_millis(deadline_ms),
        "exec took {:?} — past the {deadline_ms} ms deadline",
        t0.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── XO-05c — the lapse keeps everything the drains captured (D17) ──────

/// A phase that emits ~160 KB and then keeps running until the
/// deadline lapses: post-fix the drains delivered every produced byte
/// BEFORE the lapse, so the timed-out outcome still carries the full
/// partial streams — the D17 streams-kept contract is not regressed by
/// draining early. Pre-fix the outcome carries only the ~64 KiB pipe
/// residue (the writer blocked there) — the emitted tail is lost
/// forever even though it was produced before the lapse.
#[test]
fn xo05_lapse_outcome_keeps_all_drained_bytes() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("lapse");
    const OUT_KIB: u64 = 160;
    // Emit the oversized stream, then model the still-running phase:
    // the survivor's streams go to /dev/null — a real in-cell survivor
    // writes to conmon, never to the dead `podman exec` client's pipes
    // (the host-side hard-deadline kill ends the stub; `exec_guarded`'s
    // watcher is authoritative).
    let body = format!(
        "{}; sleep 30 >/dev/null 2>&1; exit 0",
        emit_shell(OUT_KIB, 'D', "")
    );
    let (_stub, _log) = install_stub(&dir, &body);
    let b = backend(&dir);
    let out = exec_guarded(
        &b,
        &["bash".into(), "/solution/solve.sh".into()],
        "/app",
        &[],
        Some(500),
    )
    .unwrap();
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
    assert!(out.timed_out);
    assert_eq!(out.exit_code, None);
    // EVERY emitted byte — the drains kept pace, so the lapse outcome's
    // partial stream is all ~160 KiB, never the ~64 KiB pipe residue.
    assert_eq!(out.stdout.len(), (OUT_KIB * 1024) as usize);
    assert!(out.stdout.iter().all(|b| *b == b'D'));
    let _ = std::fs::remove_dir_all(&dir);
}

// ── XO-05e — a lapsed agent's streams land in the collect dir ─────────

/// The diagnosis's observability candidate (`secondary_site`): the
/// agent exec's captured streams ride back in `CellOutcome` but
/// `run_cell` dropped them — the H-cell agent transcript was never
/// persisted to the cell dir, so pypi-server's freeze position had to
/// be inferred from the oracle transcript + the mediator WAL, never
/// byte-confirmed. Pre-fix `collect/agent-stdout.txt` and
/// `collect/agent-stderr.txt` never exist; post-fix they carry the
/// exec's bytes verbatim (the verifier streams' `test-*.txt` rule —
/// non-empty streams only — applied to the agent phase).
struct ScriptedCell {
    agent_out: Vec<u8>,
    agent_err: Vec<u8>,
    terminated: Vec<String>,
}

impl CellOps for ScriptedCell {
    fn present(&mut self, _p: &str) -> Result<bool, CellError> {
        Ok(false)
    }
    fn ls_dir(&mut self, _p: &str) -> Result<Vec<String>, CellError> {
        Ok(vec![])
    }
    fn cp_in(&mut self, _s: &std::path::Path, _d: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn cp_out(&mut self, s: &str, d: &std::path::Path) -> Result<(), CellError> {
        // Attempt 1's verifier produced reward.txt = 1 — the drive
        // decides and stops after one attempt.
        if s == "/logs/verifier/reward.txt" {
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
        if spec.argv == ["bash", "/solution/solve.sh"] {
            // The pypi-server shape: the agent lapsed its deadline —
            // the exec kept every captured byte but the outcome is a
            // lapse (the stub's own drain made this reachable; the
            // pre-drain code would have parked here instead).
            return Ok(CellOutcome {
                exit_code: None,
                timed_out: true,
                stdout: self.agent_out.clone(),
                stderr: self.agent_err.clone(),
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
        self.terminated.push(p.to_string());
        Ok(())
    }
}

#[test]
fn xo05_lapsed_agent_streams_land_in_collect() {
    let dir = scratch("agent-transcript");
    let staging = dir.join("staging");
    std::fs::create_dir_all(staging.join("tests")).unwrap();
    let collect = dir.join("collect");
    let mut cell = ScriptedCell {
        agent_out: vec![b'A'; 78_095], // pypi-server's oracle transcript size
        agent_err: b"agent-err-line\n".to_vec(),
        terminated: vec![],
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
    // The lapse end-cause and the group termination are unchanged.
    assert_eq!(report.agent_end_cause, EndCause::AgentTimeout);
    assert_eq!(cell.terminated, vec!["agent".to_string()]);
    // The captured agent streams land verbatim in the collect dir —
    // pre-fix both members were absent (the bytes died with the
    // dropped CellOutcome).
    assert_eq!(
        std::fs::read(collect.join("agent-stdout.txt")).unwrap(),
        cell.agent_out
    );
    assert_eq!(
        std::fs::read(collect.join("agent-stderr.txt")).unwrap(),
        cell.agent_err
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── XO-05d — a small-stream exec is byte-identical to before ──────────

/// A phase under the pipe bound behaves exactly as it always did: exit
/// code verbatim, both streams captured in order, no lapse. Guards the
/// common path — draining must not reorder, truncate, or re-label.
#[test]
fn xo05_small_streams_unchanged() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("small");
    let body = "echo 'small-out'; echo 'small-err' >&2; exit 3".to_string();
    let (_stub, _log) = install_stub(&dir, &body);
    let b = backend(&dir);
    let out = exec_guarded(&b, &["true".into()], "/app", &[], Some(60_000)).unwrap();
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
    assert!(!out.timed_out);
    assert_eq!(out.exit_code, Some(3));
    assert_eq!(out.stdout, b"small-out\n");
    assert_eq!(out.stderr, b"small-err\n");
    let _ = std::fs::remove_dir_all(&dir);
}
