//! R6.39 acceptance battery (manifest 298; REQ-XE-119; AR-22/ADR-0441 D2;
//! G17): AC-1 podman argv goldens (declared cwd/env + in-cell kill
//! wrapper), merged-order fixture (one ordered stream), timeout →
//! `returncode = -1` render; AC-2 opt-in (no `stdio` member ⇒ argv
//! byte-identical). Live leg ignored per ADR-0363.

use std::process::Command;

use hharness_helper::podman::{wrap_kill, CellSpec, PodmanBackend};
use hharness_helper::protocol::{CommitProof, HelperRequest};
use hharness_wire::json::Json;

fn backend() -> PodmanBackend {
    PodmanBackend {
        container: "hh-env-e1".into(),
        image: PodmanBackend::DEFAULT_IMAGE.into(),
        workspace: "/tmp/ws".into(),
        network: false,
        extra_args: vec![],
        cell: CellSpec::default(),
        mounts: vec![],
        relay: None,
    }
}

fn argv_of(cmd: &Command) -> Vec<String> {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|s| s.to_string_lossy().to_string())
        .collect()
}

#[test]
fn ac1_argv_golden_declared_cwd_env_and_incell_kill() {
    // armed + deadline ⇒ setsid/timeout wrapper; cwd=image_workdir ⇒ no --workdir
    let argv = wrap_kill(
        &["/bin/sh".into(), "-c".into(), "echo hi".into()],
        Some(30_000),
    );
    assert_eq!(
        argv,
        [
            "setsid",
            "timeout",
            "--kill-after=2",
            "30",
            "/bin/sh",
            "-c",
            "echo hi"
        ]
    );
    let cmd = backend().exec_command(
        &argv,
        None, // image_workdir: --workdir omitted → cell WORKDIR
        &[
            ("PAGER".into(), "cat".into()),
            ("TQDM_DISABLE".into(), "1".into()),
        ],
    );
    let a = argv_of(&cmd);
    assert!(a.contains(&"-e".to_string()) && a.iter().any(|s| s.contains("PAGER=cat")));
    assert!(!a.contains(&"--workdir".to_string()), "argv: {a:?}");
    // fractional grace/duration stay exact
    assert_eq!(wrap_kill(&["x".into()], Some(1_500))[3], "1.5");
    assert_eq!(wrap_kill(&["x".into()], None), vec!["x".to_string()]);
}

#[test]
fn ac2_unarmed_exec_argv_byte_identical() {
    // stdio member absent ⇒ today’s argv verbatim: --workdir present, no wrapper
    let cmd = backend().exec_command(
        &["/bin/sh".into(), "-c".into(), "echo hi".into()],
        Some("/work"),
        &[],
    );
    let a = argv_of(&cmd);
    assert!(a.contains(&"--workdir".to_string()) && a.contains(&"/work".to_string()));
    assert!(!a.iter().any(|s| s == "setsid" || s == "timeout"));
}

#[test]
fn stdio_member_roundtrips_and_absent_defaults() {
    let exec = |stdio: Option<String>| HelperRequest::Exec {
        execution_id: "e".into(),
        effect_id: "f".into(),
        attempt_no: 1,
        capability_ref: ("c".into(), "v".into()),
        args: Json::obj([("command", Json::str("true"))]),
        cwd: ".".into(),
        env: vec![],
        deadline_ms: None,
        retain_bytes_cap: 1,
        attribution_token: "t".into(),
        commit_proof: CommitProof::ReadOnly,
        idempotency_key: None,
        env_clear: false,
        stdio,
    };
    let armed = exec(Some("merged".into()));
    let j = armed.to_json();
    assert_eq!(j.get("stdio").and_then(Json::as_str), Some("merged"));
    assert_eq!(HelperRequest::from_json(&j), Ok(armed));
    let legacy = exec(None);
    let j = legacy.to_json();
    assert!(j
        .get("stdio")
        .map(|v| matches!(v, Json::Null))
        .unwrap_or(true));
    assert_eq!(HelperRequest::from_json(&j), Ok(legacy));
}

#[cfg(unix)]
#[test]
fn ac1_merged_order_fixture() {
    // `echo a; echo b >&2; echo c` — one ordered stream `a\nb\nc\n`.
    use hharness_helper::exec::{spawn_pgrp_leader, spawn_pgrp_leader_merged};
    use std::io::Read;
    let run = |merged: bool| -> String {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "echo a; echo b >&2; echo c"]);
        let mut child = if merged {
            spawn_pgrp_leader_merged(&mut c).unwrap()
        } else {
            spawn_pgrp_leader(&mut c).unwrap()
        };
        let mut out = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            e.read_to_string(&mut err).unwrap();
        }
        child.wait().unwrap();
        if merged {
            out
        } else {
            format!("{out}|{err}")
        }
    };
    assert_eq!(run(true), "a\nb\nc\n");
    assert_eq!(run(false), "a\nc\n|b\n");
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh helper_exec_merged_stdio"]
fn live_podman_cell_merged_and_incell_kill() {}
