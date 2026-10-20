//! R4.2 — the E3 (TypeScript) live leg, wrapped honestly (DF-S4.10-1;
//! HUMAN-H1's "Confirm TypeScript" ruling).
//!
//! `scripts/e3-smoke.sh` spawns `target/debug/hh-kernel serve --http`
//! (binding (c), `local_network`) on a free loopback port and drives the
//! generated TypeScript client (`clients/ts/hh-embed.ts` — emitted from
//! the single schema source, CC7; never hand-edited) end to end:
//! hello/doctor-equivalent + one run's open/submit/read + an
//! event-stream read, asserting the same facts `hh doctor` reports.
//!
//! Honesty: when node ≥ 22 is not on PATH the leg reports
//! `skipped(node-absent)` and passes — it never fabricates a result;
//! when the kernel binary is absent it reports `skipped(kernel-absent)`
//! (CI's `--all-targets` build lands `target/debug/hh-kernel` before the
//! test run — the leg executes there and locally after a workspace
//! build).

use std::path::PathBuf;
use std::process::Command;

/// The repository root — `crates/hh-codegen`'s parent.parent (works in
/// the build worktree, never assumes the main checkout).
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// The installed node major version, or `None` when node is not on PATH
/// or its version cannot be parsed.
fn node_major() -> Option<u64> {
    let out = Command::new("node")
        .args(["-e", "process.stdout.write(process.versions.node)"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// The live leg: node ≥ 22 + the built kernel drive the generated client
/// over binding (c) end to end; both absences are reported, never faked.
#[test]
fn e3_smoke_generated_ts_client_over_binding_c() {
    match node_major() {
        Some(v) if v >= 22 => {}
        other => {
            eprintln!("r4_2: skipped(node-absent) — node >= 22 required, found {other:?}");
            return;
        }
    }
    if !root().join("target/debug/hh-kernel").exists() {
        eprintln!("r4_2: skipped(kernel-absent) — run 'cargo build -p hh-kernel' first");
        return;
    }
    let out = Command::new("bash")
        .arg(root().join("scripts/e3-smoke.sh"))
        .output()
        .expect("spawn e3-smoke.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("e3-smoke: ok"),
        "e3-smoke failed (status {:?}):\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status.code()
    );
    // The leg must have executed the checks, not only reported the ok
    // marker — the smoke prints one `ok`/`FAIL` line per check.
    assert!(
        stdout.matches("\nok   ").count() + usize::from(stdout.starts_with("ok   ")) >= 10,
        "e3-smoke ran too few checks:\n{stdout}"
    );
}

/// The honest-skip arm: with node off PATH the script answers
/// `skipped(node-absent)` (exit 3), never a fabricated pass. `/usr/bin`
/// + `/bin` carry `bash` and `git` but no node on the build hosts.
#[test]
fn e3_smoke_node_absent_reports_skip_honestly() {
    let out = Command::new("bash")
        .arg(root().join("scripts/e3-smoke.sh"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("spawn e3-smoke.sh");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "expected the honest skip (3), got {:?}:\n{stderr}",
        out.status.code()
    );
    assert!(
        stderr.contains("skipped(node-absent)"),
        "skip must name its reason:\n{stderr}"
    );
}
