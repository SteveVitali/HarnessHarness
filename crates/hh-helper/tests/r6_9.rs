//! R6.9 — the AR-06 cell-spec helper battery (REQ-XB-107/106): the
//! `--workdir`/`--workdir-from-image`/`--cpus`/`--memory` flags pin the
//! `podman create` argv, an undeclared spec is byte-identical to the
//! pre-admission argv, and the copy/exec/terminate verbs carry the
//! verbatim `podman` argv the kernel-mediated lifecycle drives —
//! `/work` aliasing refuses before any argv runs.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_helper::podman::{CellSpec, PodmanBackend};

const HELPER: &str = env!("CARGO_BIN_EXE_hh-helper");

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("r69-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn podman_stub(dir: &Path, workdir: &str) -> (PathBuf, PathBuf) {
    let bin = dir.join("podman");
    let log = dir.join("podman.log");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho \"$@\" >> \"$HH_PODMAN_LOG\"\nif [ \"$1\" = \"image\" ]; then echo 'WORKDIR_VALUE'; fi\nexit 0\n"
            .replace("WORKDIR_VALUE", workdir)
            .as_str(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (bin, log)
}

fn base_args(sock: &Path, ws: &Path) -> Vec<String> {
    vec![
        "--socket".into(),
        sock.display().to_string(),
        "--backend".into(),
        "podman".into(),
        "--container".into(),
        "hh-env-t1".into(),
        "--image".into(),
        "alpine:3.20".into(),
        "--workspace".into(),
        ws.display().to_string(),
    ]
}

fn spawn_helper(dir: &Path, extra: &[&str], stub_workdir: &str) -> PathBuf {
    let (stub, log) = podman_stub(dir, stub_workdir);
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let sock = dir.join("h.sock");
    let mut cmd = Command::new(HELPER);
    cmd.args(base_args(&sock, &ws)).args(extra);
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            stub.parent().unwrap().display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    )
    .env("HH_PODMAN_LOG", &log)
    .env_remove("S4_HELPER_NETWORK")
    .env_remove("HH_PODMAN_BIN")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !sock.exists() {
        assert!(std::time::Instant::now() < deadline, "no socket: {extra:?}");
        if let Ok(Some(s)) = child.try_wait() {
            panic!("helper exited early {s}: {extra:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    log
}

fn create_argv(log: &Path) -> String {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("create "))
        .unwrap_or_else(|| panic!("no create argv"))
        .to_string()
}

fn run_expect_exit(dir: &Path, extra: &[&str], stub_workdir: &str) -> i32 {
    let (stub, _log) = podman_stub(dir, stub_workdir);
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let sock = dir.join("h.sock");
    let mut cmd = Command::new(HELPER);
    cmd.args(base_args(&sock, &ws)).args(extra);
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            stub.parent().unwrap().display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    )
    .env("HH_PODMAN_LOG", dir.join("podman.log"))
    .env_remove("S4_HELPER_NETWORK")
    .env_remove("HH_PODMAN_BIN")
    .stdin(Stdio::null());
    cmd.output().unwrap().status.code().unwrap_or(-1)
}

/// AC-2: `--workdir`/`--cpus`/`--memory` pin the create argv verbatim.
#[test]
fn cell_spec_flags_pin_the_argv() {
    let dir = scratch("spec");
    let log = spawn_helper(
        &dir,
        &[
            "--workdir",
            "/app/langcodes",
            "--cpus",
            "4",
            "--memory",
            "8192",
        ],
        "",
    );
    let argv = create_argv(&log);
    assert!(argv.contains("--workdir /app/langcodes"), "{argv}");
    assert!(argv.contains("--cpus 4"), "{argv}");
    assert!(argv.contains("--memory 8192m"), "{argv}");
}

/// AC-2: `--workdir-from-image` resolves `.Config.WorkingDir` through
/// `podman image inspect` and lands it verbatim.
#[test]
fn workdir_from_image_resolves_the_declared_workdir() {
    let dir = scratch("fromimg");
    let log = spawn_helper(&dir, &["--workdir-from-image"], "/app/langcodes");
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains("image inspect --format {{.Config.WorkingDir}}"),
        "{text}"
    );
    let argv = create_argv(&log);
    assert!(argv.contains("--workdir /app/langcodes"), "{argv}");
}

/// AC-2 (fails-without): every flag absent reproduces the pre-admission
/// argv byte-identically.
#[test]
fn undeclared_spec_is_byte_identical() {
    let dir = scratch("default");
    let log = spawn_helper(&dir, &[], "");
    let argv = create_argv(&log);
    assert_eq!(
        argv,
        "create --name hh-env-t1 --network none --pids-limit 512 \
         --security-opt no-new-privileges -v WS:/work:rw \
         --workdir /work --init alpine:3.20 sleep infinity"
            .replace("WS", &dir.join("ws").display().to_string())
    );
}

/// The two workdir spellings are exclusive (exit 2); an absent image
/// WORKDIR refuses (exit 3), never substituted.
#[test]
fn exclusive_and_absent_workdir_refuse() {
    let dir = scratch("excl");
    assert_eq!(
        run_expect_exit(&dir, &["--workdir", "/x", "--workdir-from-image"], "/y"),
        2
    );
    let dir2 = scratch("empty");
    assert_eq!(run_expect_exit(&dir2, &["--workdir-from-image"], ""), 3);
    let dir3 = scratch("badcpus");
    assert_eq!(run_expect_exit(&dir3, &["--cpus", "nope"], ""), 2);
    assert_eq!(run_expect_exit(&dir3, &["--memory", "0"], ""), 2);
}

/// AC-1/REQ-XB-106: the copy/exec/terminate verbs carry verbatim argv
/// (the stub logs them); `/work` aliasing refuses before any argv runs.
#[test]
fn cell_verbs_carry_verbatim_argv_and_refuse_work_alias() {
    let dir = scratch("verbs");
    let (stub, log) = podman_stub(&dir, "");
    std::env::set_var("HH_PODMAN_BIN", &stub);
    std::env::set_var("HH_PODMAN_LOG", &log);
    let b = PodmanBackend {
        container: "hh-env-c1".into(),
        image: "img@sha256:x".into(),
        workspace: dir.join("ws").display().to_string(),
        network: false,
        extra_args: vec![],
        cell: CellSpec::default(),
    };
    // /work aliasing refuses *before* argv — no log line.
    assert!(b.cp_in("/host/tests", "/work/tests").is_err());
    assert!(b.cp_out("/work/reward", "/host/r").is_err());
    assert!(!std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("cp /host/tests"));
    b.cp_in("/host/tests", "/tests").unwrap();
    b.cp_out("/logs/verifier/reward.txt", "/host/reward.txt")
        .unwrap();
    b.rm_glob("/logs/verifier/reward.*").unwrap();
    let _ = b.exists("/tests").unwrap();
    let _ = b.ls_dir("/").unwrap();
    b.terminate_group("agent").unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("cp /host/tests hh-env-c1:/tests"), "{text}");
    assert!(
        text.contains("cp hh-env-c1:/logs/verifier/reward.txt"),
        "{text}"
    );
    assert!(text.contains("rm -f /logs/verifier/reward.*"), "{text}");
    assert!(text.contains("test -e /tests"), "{text}");
    assert!(text.contains("ls -1A /"), "{text}");
    assert!(text.contains("pkill -TERM -f agent"), "{text}");
    assert!(text.contains("pkill -KILL -f agent"), "{text}");
    assert!(!text.contains("cp /host/tests hh-env-c1:/work"), "{text}");
    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
}
