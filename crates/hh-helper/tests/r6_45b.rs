//! DF-R6.45b-2 — the Wc2 `cell-terminate` machinery repair. The
//! agent-deadline-lapse leg must not depend on `pkill` (absent on the
//! minimal task-env images — the verbatim H-pub machinery exception
//! was `pkill: not found`, rc 127). `terminate_group` now drives an
//! in-cell `/proc` cmdline walk under `sh`, with the pattern and the
//! signal riding `HH_KILL_PAT`/`HH_KILL_SIG` through `podman exec -e`.
//!
//! This battery runs the verb end-to-end through a `podman` stub that
//! *executes* the exec'd argv locally — a marker sleeper is spawned,
//! the TERM→KILL ladder runs, and the sleeper's death is observed
//! where `/proc` exists (the podman guest shape). On a `/proc`-less
//! host the walk is a no-op and the sleeper survives — asserted
//! honestly in both directions; in every case the recorded argv
//! carries no `pkill`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_helper::podman::{CellSpec, PodmanBackend};

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("r645b-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A `podman` stub that logs every argv (as the r6_9 stub does) and
/// additionally *runs* the payload of an `exec` line locally —
/// `-e KEY=VAL` pairs become real env, the container token is dropped,
/// and the remaining argv is executed with its status propagated.
/// That makes the host stand in for the cell: the terminate walk runs
/// against this host's `/proc` (or no-ops where there is none).
fn podman_exec_stub(dir: &Path) -> (PathBuf, PathBuf) {
    let bin = dir.join("podman");
    let log = dir.join("podman.log");
    std::fs::write(
        &bin,
        "#!/bin/sh\n\
         echo \"$@\" >> \"$HH_PODMAN_LOG\"\n\
         if [ \"$1\" = \"exec\" ]; then\n\
         \tshift\n\
         \tenvs=\"\"\n\
         \twhile [ $# -gt 0 ]; do\n\
         \t\tcase \"$1\" in\n\
         \t\t\t-e) envs=\"$envs $2\"; shift 2 ;;\n\
         \t\t\t--workdir) shift 2 ;;\n\
         \t\t\t*) break ;;\n\
         \t\tesac\n\
         \tdone\n\
         \tshift\n\
         \texec env $envs \"$@\"\n\
         \texit $?\n\
         fi\n\
         exit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (bin, log)
}

/// The defect: `cell-terminate` reached `pkill` inside the cell. The
/// repaired verb must (a) record no `pkill` anywhere in its argv and
/// (b) actually terminate a process whose cmdline matches the pattern
/// — exercised here by spawning a uniquely named sleeper and letting
/// the real walk kill it.
#[test]
fn terminate_group_walks_proc_and_never_needs_pkill() {
    let dir = scratch("term");
    let (stub, log) = podman_exec_stub(&dir);
    std::env::set_var("HH_PODMAN_BIN", &stub);
    std::env::set_var("HH_PODMAN_LOG", &log);
    let b = PodmanBackend {
        container: "hh-env-c1".into(),
        image: "img@sha256:x".into(),
        workspace: dir.join("ws").display().to_string(),
        network: false,
        extra_args: vec![],
        cell: CellSpec::default(),
        mounts: vec![],
        relay: None,
    };

    // An empty pattern refuses before any argv — it would match every
    // in-cell process; the guard is honest, never a kill-everything.
    assert!(b.terminate_group("").is_err());

    // A sleeper whose *invoked argv* carries a unique marker: symlink
    // `/bin/sleep` under a marker name so its `/proc/<pid>/cmdline`
    // matches only this test's pattern.
    let marker = format!("hh-r645b-mk-{}", std::process::id());
    let sleep_src = ["/bin/sleep", "/usr/bin/sleep"]
        .iter()
        .map(Path::new)
        .find(|p| p.exists())
        .expect("no sleep binary");
    let sleeper_bin = dir.join(format!("{marker}-sleep"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(sleep_src, &sleeper_bin).unwrap();
    let mut child = Command::new(&sleeper_bin)
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    b.terminate_group(&marker).unwrap();

    if Path::new("/proc").is_dir() {
        // The podman-guest shape: the TERM pass signalled the sleeper
        // (its zombie cmdline is empty, so the KILL pass is the
        // no-match-is-OK leg). `wait` reports the signalled exit.
        let st = child.wait().unwrap();
        assert!(
            !st.success(),
            "the marker sleeper must be terminated by the /proc walk: {st}"
        );
    } else {
        // No `/proc` on this host — the walk is an honest no-op and
        // the sleeper must still be running; clean it up ourselves.
        assert!(
            child.try_wait().unwrap().is_none(),
            "without /proc the walk must not kill: sleeper should survive"
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    let text = std::fs::read_to_string(&log).unwrap();
    // The mechanism, verbatim: `sh -c` /proc walk, pattern+signal by
    // env — and no `pkill` token anywhere in the recorded argv.
    assert!(text.contains("sh -c for p in /proc/"), "{text}");
    assert!(text.contains(&format!("HH_KILL_PAT={marker}")), "{text}");
    assert!(text.contains("HH_KILL_SIG=TERM"), "{text}");
    assert!(text.contains("HH_KILL_SIG=KILL"), "{text}");
    assert!(!text.contains("pkill"), "{text}");

    std::env::remove_var("HH_PODMAN_BIN");
    std::env::remove_var("HH_PODMAN_LOG");
}
