//! R4.5 acceptance battery — `scripts/xcheck-foreign.sh`, the committed
//! two-toolchain replay leg (DF-S0.3-2 / M-S2-5): E1 `selfcheck` plus
//! `verify` over the E2 (python3) and E3 (node ≥ 22) answers regenerated
//! from the trees at run time, wired as a step on the existing CI job.
//!
//! Invariants: a full local run with both toolchains exits 0; a stripped
//! PATH reports `skipped(<tool>-absent)` (exit 3) per tool, never a
//! fabricated pass; a missing hh-xcheck binary exits 2; the committed
//! E2 answers carry exactly the six original arms — the extension arms
//! (`hir-documents`, `ledger-scenarios`) fail `missing answer` as the
//! recorded pending cells; the ci.yml step that runs the leg carries the
//! ADR-0362 step-mode gate.

use std::path::{Path, PathBuf};
use std::process::Command;

use hh_wire::json::Json;
use hh_xcheck::check::verify_answers;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/cross-impl")
}

fn load(p: &Path) -> Json {
    hh_wire::json::parse(&String::from_utf8_lossy(&std::fs::read(p).unwrap())).unwrap()
}

fn which(tool: &str) -> Option<PathBuf> {
    // `command -v` is a shell builtin — resolve via /bin/sh.
    let out = Command::new("/bin/sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .ok()?;
    if out.status.success() {
        Some(PathBuf::from(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ))
    } else {
        None
    }
}

/// With python3 and node ≥ 22 on PATH and the hh-xcheck binary built
/// (cargo test compiles the crate's bin target), the whole leg must be
/// green — every covered case verified, pending cells named, exit 0.
#[test]
fn xcheck_foreign_full_run_green() {
    let node_ok = which("node").is_some_and(|_| {
        Command::new("node")
            .args([
                "-e",
                "process.stdout.write(process.versions.node.split('.')[0])",
            ])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .is_some_and(|major| major >= 22)
    });
    if which("python3").is_none() || !node_ok {
        eprintln!("r4_5: skipped(tool-absent) — python3 and node >= 22 required");
        return;
    }
    let xcheck = root().join("target/debug/hh-xcheck");
    assert!(
        xcheck.exists(),
        "hh-xcheck binary must exist next to the test binaries"
    );
    let out = Command::new("bash")
        .arg(root().join("scripts/xcheck-foreign.sh"))
        .output()
        .expect("spawn xcheck-foreign.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "xcheck-foreign must be green with both toolchains:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("e2 ok") && stdout.contains("e3 ok"),
        "both foreign legs must report ok:\n{stdout}"
    );
    assert!(
        stdout.contains("xcheck-foreign: ok"),
        "the closing line must be the all-verified verdict:\n{stdout}"
    );
}

/// With neither toolchain on PATH the leg reports both honest skips and
/// exits 3 — never a pass, never a fake "green" run.
#[test]
fn xcheck_foreign_toolchains_absent_report_skips() {
    // A scratch PATH carrying only the utilities the script needs —
    // python3 and node are absent no matter what the host provides in
    // /usr/bin (macOS /usr/bin/python3 is real; ubuntu images too).
    let scratch = std::env::temp_dir().join(format!("hh-r45-path-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for tool in ["git", "mktemp", "awk", "grep", "cat", "rm"] {
        let mut linked = false;
        for dir in ["/usr/bin", "/bin"] {
            let src = Path::new(dir).join(tool);
            if src.exists() {
                std::os::unix::fs::symlink(&src, scratch.join(tool)).unwrap();
                linked = true;
                break;
            }
        }
        assert!(linked, "cannot provision scratch PATH: {tool} missing");
    }
    let out = Command::new("/bin/bash")
        .arg(root().join("scripts/xcheck-foreign.sh"))
        .env("PATH", &scratch)
        .output()
        .expect("spawn xcheck-foreign.sh");
    let _ = std::fs::remove_dir_all(&scratch);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "absent toolchains must exit 3:\n{stderr}"
    );
    assert!(
        stderr.contains("skipped(python3-absent)"),
        "the e2 skip must name its reason:\n{stderr}"
    );
    assert!(
        stderr.contains("skipped(node-absent)"),
        "the e3 skip must name its reason:\n{stderr}"
    );
}

/// A missing hh-xcheck binary is exit 2 with the build hint — the check
/// cannot run and the script says so.
#[test]
fn xcheck_foreign_missing_binary_is_exit_2() {
    let out = Command::new("bash")
        .arg(root().join("scripts/xcheck-foreign.sh"))
        .env("HH_XCHECK_BIN", "/nonexistent/hh-xcheck")
        .output()
        .expect("spawn xcheck-foreign.sh");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cargo build -p hh-xcheck"),
        "the hint must name the build command:\n{stderr}"
    );
}

/// The committed E2 answers verify green on the six original arms and
/// fail `missing answer` on exactly the two extension arms the e2 tree
/// does not carry — the recorded pending cells, never a vacuous pass.
#[test]
fn e2_committed_answers_pending_cells_are_named() {
    let answers = load(&root().join("foreign/e2-python/answers.json"));
    assert_eq!(
        answers.get("candidate").and_then(Json::as_str),
        Some("E2-candidate"),
        "the committed answers must name the E2 candidate"
    );
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    let (mut ok, mut pending) = (0usize, 0usize);
    for r in &report.results {
        match r.arm.as_str() {
            "hir-documents" | "ledger-scenarios" => {
                pending += 1;
                assert_eq!(
                    r.outcome.as_ref().err().map(String::as_str),
                    Some("missing answer"),
                    "extension-arm case {} {} must fail `missing answer`, got {:?}",
                    r.arm,
                    r.case,
                    r.outcome
                );
            }
            _ => {
                ok += 1;
                assert!(
                    r.outcome.is_ok(),
                    "covered case {} {} failed: {:?}",
                    r.arm,
                    r.case,
                    r.outcome
                );
            }
        }
    }
    assert_eq!(ok, 49, "the six E2 arms cover 49 cases");
    assert_eq!(
        pending, 8,
        "the extension arms carry 8 pending cells (5 hir-documents + 3 ledger-scenarios)"
    );
}

/// The ci.yml step running this leg carries the ADR-0362 step-mode gate
/// — `if: steps.path.outputs.mode != 'fast'` — inside the existing
/// build-test-drift job (R4.2a cadence: the leg rides the existing job,
/// never a second workflow run per head).
#[test]
fn ci_workflow_gates_the_foreign_leg() {
    let ci = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
    let pos = ci
        .find("scripts/xcheck-foreign.sh")
        .expect("ci.yml must run scripts/xcheck-foreign.sh");
    // The enclosing step block: back to the step's `      - ` start.
    let start = ci[..pos].rfind("\n      - ").map(|i| i + 1).unwrap_or(0);
    let end = ci[pos..]
        .find("\n      - ")
        .map(|i| pos + i + 1)
        .unwrap_or(ci.len());
    let block = &ci[start..end];
    assert!(
        block.contains("if: steps.path.outputs.mode != 'fast'"),
        "the foreign-replay step must carry the step-mode gate:\n{block}"
    );
}
