//! R5.56 — the `--network <none|bridge>` / `S4_HELPER_NETWORK` helper
//! flag battery (AC-R5.56-1; ADR-0421). Each leg spawns the real
//! `hh-helper` binary against a `podman` stub on `PATH` (every verb logs
//! its argv and exits 0) so the `podman create` argv is observable
//! without a live podman. Offline, `std::env::temp_dir()` scratch, USD 0.
//!
//! - `--network bridge` produces `podman create … --network bridge …`;
//! - `--network none` and an absent flag produce `--network none`
//!   byte-identically to the pre-change argv;
//! - `S4_HELPER_NETWORK` selects the mode when the flag is absent and an
//!   explicit flag wins over the env;
//! - an unknown flag/env value exits usage (2), never coerced.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const HELPER: &str = env!("CARGO_BIN_EXE_hh-helper");

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("r556-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A `podman` stub on PATH: logs every invocation's argv to
/// `$HH_PODMAN_LOG` (one line per call) and exits 0 — `info`/`create`/
/// `start`/`stop`/`rm` all succeed so the helper runs its real code path.
fn podman_stub(dir: &Path) -> (PathBuf, PathBuf) {
    let bin = dir.join("podman");
    let log = dir.join("podman.log");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho \"$@\" >> \"$HH_PODMAN_LOG\"\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (bin, log)
}

/// The `create` argv the stub recorded.
fn create_argv(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    text.lines()
        .find(|l| l.starts_with("create "))
        .map(|l| l.to_string())
        .unwrap_or_else(|| panic!("no `create` invocation recorded:\n{text}"))
}

fn base_args(sock: &Path, ws: &Path) -> Vec<String> {
    vec![
        "--socket".to_string(),
        sock.display().to_string(),
        "--backend".to_string(),
        "podman".to_string(),
        "--container".to_string(),
        "hh-env-t1".to_string(),
        "--image".to_string(),
        "alpine:3.20".to_string(),
        "--workspace".to_string(),
        ws.display().to_string(),
    ]
}

/// Spawn the helper against the stub; wait for the session socket, then
/// kill it — the helper's own teardown leg is irrelevant to argv shape.
fn spawn_helper(dir: &Path, extra_args: &[&str], envs: &[(&str, &str)]) -> PathBuf {
    let (stub_bin, log) = podman_stub(dir);
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let sock = dir.join("h.sock");
    let mut cmd = Command::new(HELPER);
    cmd.args(base_args(&sock, &ws)).args(extra_args);
    let path = format!(
        "{}:{}",
        stub_bin.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    cmd.env("PATH", path)
        .env("HH_PODMAN_LOG", &log)
        // No ambient admission leaks between legs.
        .env_remove("S4_HELPER_NETWORK");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !sock.exists() {
        assert!(
            Instant::now() < deadline,
            "helper socket never appeared: {extra_args:?}"
        );
        if let Ok(Some(status)) = child.try_wait() {
            panic!("helper exited early {status}: {extra_args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    log
}

/// Run the helper expecting an immediate refusal; returns (status, stderr).
fn run_expect_exit(dir: &Path, extra_args: &[&str], envs: &[(&str, &str)]) -> (i32, String) {
    let (stub_bin, _log) = podman_stub(dir);
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let sock = dir.join("h.sock");
    let mut cmd = Command::new(HELPER);
    cmd.args(base_args(&sock, &ws)).args(extra_args);
    let path = format!(
        "{}:{}",
        stub_bin.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    cmd.env("PATH", path)
        .env("HH_PODMAN_LOG", dir.join("podman.log"))
        .env_remove("S4_HELPER_NETWORK");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.stdin(Stdio::null()).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// AC-1: `--network bridge` produces `podman create … --network bridge`.
#[test]
fn flag_bridge_produces_bridge_argv() {
    let dir = scratch("bridge");
    let log = spawn_helper(&dir, &["--network", "bridge"], &[]);
    let argv = create_argv(&log);
    assert!(argv.contains("--network bridge"), "argv: {argv}");
}

/// AC-1: `--network none` produces the byte-identical `--network none`.
#[test]
fn flag_none_produces_none_argv() {
    let dir = scratch("none");
    let log = spawn_helper(&dir, &["--network", "none"], &[]);
    let argv = create_argv(&log);
    assert!(argv.contains("--network none"), "argv: {argv}");
    assert!(!argv.contains("bridge"), "argv: {argv}");
}

/// AC-1: an absent flag is the unchanged default posture — `--network
/// none`, never a silent bridge.
#[test]
fn absent_flag_defaults_to_none() {
    let dir = scratch("default");
    let log = spawn_helper(&dir, &[], &[]);
    let argv = create_argv(&log);
    assert!(argv.contains("--network none"), "argv: {argv}");
    assert!(!argv.contains("bridge"), "argv: {argv}");
}

/// AC-1/ADR-0421: `S4_HELPER_NETWORK` selects the mode when the flag is
/// absent (hh-env's fixed spawn argv carries the admission through env).
#[test]
fn env_selects_bridge_when_flag_absent() {
    let dir = scratch("env");
    let log = spawn_helper(&dir, &[], &[("S4_HELPER_NETWORK", "bridge")]);
    let argv = create_argv(&log);
    assert!(argv.contains("--network bridge"), "argv: {argv}");
}

/// The explicit flag wins over the env — never an ambiguity.
#[test]
fn flag_wins_over_env() {
    let dir = scratch("flagwins");
    let log = spawn_helper(
        &dir,
        &["--network", "none"],
        &[("S4_HELPER_NETWORK", "bridge")],
    );
    let argv = create_argv(&log);
    assert!(argv.contains("--network none"), "argv: {argv}");
    assert!(!argv.contains("bridge"), "argv: {argv}");
}

/// AC-1: an unknown `--network` value is a usage failure (exit 2),
/// never coerced to a mode.
#[test]
fn unknown_flag_value_exits_usage() {
    let dir = scratch("badflag");
    let (code, err) = run_expect_exit(&dir, &["--network", "host"], &[]);
    assert_eq!(code, 2);
    assert!(err.contains("usage"), "stderr: {err}");
}

/// The env vocabulary is the same closed sum — an unknown env value
/// refuses the same way (exit 2).
#[test]
fn unknown_env_value_exits_usage() {
    let dir = scratch("badenv");
    let (code, err) = run_expect_exit(&dir, &[], &[("S4_HELPER_NETWORK", "host")]);
    assert_eq!(code, 2);
    assert!(err.contains("usage"), "stderr: {err}");
}
