//! R4.4 acceptance battery — the `foreign/e3-typescript` replay (the E3
//! candidate's shared `lib/` core plus all eight arms: R4.4's
//! identity-trees/records, ledger-transcript, registry-snapshot and
//! canonical-parse; R4.4a's plugin-manifests and ledger-scenarios;
//! R4.4b's hir-documents; `answers.json`, `scripts/xcheck-e3.sh`).
//!
//! Invariants: the committed `foreign/e3-typescript/answers.json`
//! verifies green on every arm — all eight bundle arms are covered as of
//! R4.4b, with zero cells left `missing answer`; the tree carries no npm
//! dependency and only `node:`/relative `.ts` imports; `xcheck-e3.sh`
//! reports `skipped(node-absent)` (exit 3) when node < 22 is not on PATH.
//! When node ≥ 22 *is* on PATH the replay leg re-runs `foreign.ts` and
//! asserts byte-identical covered answers — which is also what proves the
//! tree is erasable-TypeScript (node's own type-stripping rejects
//! enums/namespaces/parameter properties).
//!
//! The R4.4b-focused invariants (honest bound, E2 absence, coverage
//! floor) live in `tests/r4_4b.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

use hharness_wire::json::Json;
use hharness_xcheck::check::{is_informational, verify_answers};
use hharness_xcheck::ARMS;

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
    hharness_wire::json::parse(&String::from_utf8_lossy(&std::fs::read(p).unwrap())).unwrap()
}

/// The arms the E3 tree answers — all eight bundle arms as of R4.4b
/// (R4.4's five + R4.4a's plugin-manifests and ledger-scenarios +
/// R4.4b's hir-documents). The `missing answer` branch below still
/// guards any future arm the tree has not claimed.
const COVERED_ARMS: &[&str] = &[
    "identity-trees",
    "identity-records",
    "ledger-transcript",
    "registry-snapshot",
    "plugin-manifests",
    "canonical-parse",
    "ledger-scenarios",
    "hir-documents",
];

/// The committed `answers.json` verifies every covered case and reports
/// only the pending arms' cells as `missing answer` — never a vacuous
/// pass, never a patched answer.
#[test]
fn e3_committed_answers_verify_covered_arms() {
    let answers_path = root().join("foreign/e3-typescript/answers.json");
    let answers = load(&answers_path);
    assert_eq!(
        answers.get("candidate").and_then(Json::as_str),
        Some("E3-candidate"),
        "the committed answers must name the E3 candidate"
    );
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    let mut covered = 0usize;
    for r in &report.results {
        if COVERED_ARMS.contains(&r.arm.as_str()) {
            covered += 1;
            assert!(
                r.outcome.is_ok(),
                "covered case {} {} failed: {:?}",
                r.arm,
                r.case,
                r.outcome
            );
        } else {
            assert_eq!(
                r.outcome.as_ref().err().map(String::as_str),
                Some("missing answer"),
                "uncovered arm {} case {} must fail only as `missing answer`, got {:?}",
                r.arm,
                r.case,
                r.outcome
            );
        }
    }
    assert!(
        covered >= 30,
        "the committed E3 answers verified only {covered} covered cases — a covered arm went missing"
    );
}

/// The node leg: replay the bundle through `foreign.ts` and verify — the
/// fresh answers must agree with the committed record on every covered
/// case. `skipped(node-absent)` is the honest report, never a fake pass.
#[test]
fn e3_foreign_ts_replays_the_covered_arms() {
    match node_major() {
        Some(v) if v >= 22 => {}
        other => {
            eprintln!("r4_4: skipped(node-absent) — node >= 22 required, found {other:?}");
            return;
        }
    }
    let tmp = std::env::temp_dir().join(format!("hh-e3-answers-{}", std::process::id()));
    let out = Command::new("node")
        .arg(root().join("foreign/e3-typescript/foreign.ts"))
        .arg(bundle_dir())
        .arg(&tmp)
        .output()
        .expect("spawn node foreign.ts");
    assert!(
        out.status.success(),
        "foreign.ts failed (status {:?}):\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = load(&tmp);
    let committed = load(&root().join("foreign/e3-typescript/answers.json"));
    // Covered-arm answers are byte-stable across runs (timing members in
    // canonical-parse are recorded, never gated — compare case answers
    // member-wise on the gated members only, via verify_answers).
    let report = verify_answers(&bundle_dir(), &fresh).unwrap();
    for r in &report.results {
        if COVERED_ARMS.contains(&r.arm.as_str()) {
            assert!(
                r.outcome.is_ok(),
                "fresh replay failed covered case {} {}: {:?}",
                r.arm,
                r.case,
                r.outcome
            );
        }
    }
    // The committed record and the fresh run agree on every covered arm's
    // gated members (the recorded output is the replay, not a patched
    // answer).
    for spec in ARMS.iter().filter(|s| COVERED_ARMS.contains(&s.id)) {
        let Json::Obj(expected) = load(&bundle_dir().join(spec.dir).join("expected.json")) else {
            panic!("expected.json must be an object");
        };
        let Json::Obj(fresh_arm) = fresh
            .get("arms")
            .and_then(|a| a.get(spec.id))
            .cloned()
            .unwrap_or(Json::Null)
        else {
            panic!("fresh answers missing arm {}", spec.id);
        };
        let Json::Obj(committed_arm) = committed
            .get("arms")
            .and_then(|a| a.get(spec.id))
            .cloned()
            .unwrap_or(Json::Null)
        else {
            panic!("committed answers missing arm {}", spec.id);
        };
        for (case, want) in &expected {
            if is_informational(spec.id, case) {
                continue;
            }
            if let Json::Obj(wm) = want {
                for k in wm.keys() {
                    if spec.id == "canonical-parse" && k == "bytes" {
                        continue;
                    }
                    assert_eq!(
                        fresh_arm.get(case).and_then(|c| c.get(k)),
                        committed_arm.get(case).and_then(|c| c.get(k)),
                        "{}:{} member {k} — committed record vs fresh replay disagree",
                        spec.id,
                        case
                    );
                }
            } else {
                assert_eq!(
                    fresh_arm.get(case),
                    committed_arm.get(case),
                    "{}:{} — committed record vs fresh replay disagree",
                    spec.id,
                    case
                );
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
}

/// `scripts/xcheck-e3.sh` answers `skipped(node-absent)` (exit 3) with
/// node off PATH — the honest skip R4.5's CI leg relies on.
#[test]
fn xcheck_e3_node_absent_reports_skip_honestly() {
    let out = Command::new("bash")
        .arg(root().join("scripts/xcheck-e3.sh"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("spawn xcheck-e3.sh");
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

/// The no-npm-deps / erasable-TS floor: the tree carries no `package.json`
/// or `node_modules`, and every import specifier is `node:`-stdlib or a
/// relative `.ts` path. (Erasability itself is proven by the node leg —
/// type-stripping rejects enums/namespaces/parameter properties at run
/// time.)
#[test]
fn e3_tree_carries_no_npm_dependency() {
    let tree = root().join("foreign/e3-typescript");
    assert!(tree.join("foreign.ts").exists(), "the driver must exist");
    assert!(
        !tree.join("package.json").exists() && !tree.join("node_modules").exists(),
        "the E3 tree must not carry npm machinery"
    );
    let mut sources = Vec::new();
    collect_ts(&tree, &mut sources);
    assert!(!sources.is_empty());
    for src in sources {
        let text = std::fs::read_to_string(&src).unwrap();
        // Every `from "<spec>"` specifier (covers multi-line imports). A
        // bare `find("from")` also matches the tail of string literals
        // like `"derived-from"`/`"from"` (real members of the hir.ts
        // surface), so the `from` must not be preceded by a word or
        // string character — real imports have whitespace/`}` before it.
        let mut rest = text.as_str();
        while let Some(i) = rest.find("from") {
            let inside_word = rest.as_bytes()[..i].last().is_some_and(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'"' | b'\'' | b'`')
            });
            rest = &rest[i + 4..];
            if inside_word {
                continue;
            }
            let spec = rest
                .trim_start()
                .strip_prefix('"')
                .and_then(|s| s.split('"').next());
            if let Some(spec) = spec {
                assert!(
                    spec.starts_with("node:") || (spec.starts_with("./") && spec.ends_with(".ts")),
                    "{}: non-stdlib/non-relative import {spec:?}",
                    src.display()
                );
            }
        }
    }
}

fn collect_ts(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_ts(&p, out);
        } else if p.extension().is_some_and(|x| x == "ts") {
            out.push(p);
        }
    }
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
