//! R4.16a acceptance battery — the E2 (python3) `hir-documents` +
//! `ledger-scenarios` ports (the GATE-G6 finding insert;
//! docs/tickets/165a_R4.16a__e2-python-extension-arms.md; discharges the
//! foreign legs of DF-S1.2-2 / DF-S1.5-3). Invariants:
//!
//! * the committed `answers.json` carries all eight arms and `verify` is
//!   57/0 — no `missing answer` cell remains for E2;
//! * the new arms' gated members agree member-for-member with the e3
//!   reference answers (the port is semantics-faithful, not a patch);
//! * a fresh `python3 foreign.py` replay reproduces the committed record
//!   exactly (the committed file is the replay, not a hand-edit);
//! * dropping either new arm's answers fails `verify` by name —
//!   `missing answer` — the foreign-answer path is real;
//! * one `_replay_wal` fold serves both ledger arms (CC1 — one ledger
//!   spelling, the e3 `replayWal` refactor's shape).

use std::path::{Path, PathBuf};
use std::process::Command;

use hharness_wire::json::Json;
use hharness_xcheck::check::verify_answers;
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

/// The five committed hir-documents cases.
const HIR_CASES: &[&str] = &[
    "minimal-native",
    "hosted-agent",
    "gated-tool-no-contract",
    "missing-provenance",
    "unresolved-ref",
];

/// The three committed ledger-scenarios cases.
const SCENARIO_CASES: &[&str] = &["blob-refs", "causal-chain", "writer-handover"];

/// `answers["arms"][<arm>][<case>][<member>]` — string or null access.
fn member<'a>(answers: &'a Json, arm: &str, case: &str, m: &str) -> &'a Json {
    static NULL: Json = Json::Null;
    answers
        .get("arms")
        .and_then(|a| a.get(arm))
        .and_then(|a| a.get(case))
        .and_then(|c| c.get(m))
        .unwrap_or(&NULL)
}

/// The committed E2 answers carry all eight arms (the ARMS table's ids —
/// no more, no less) and verify 57/0.
#[test]
fn e2_committed_answers_cover_all_eight_arms() {
    let answers = load(&root().join("foreign/e2-python/answers.json"));
    assert_eq!(
        answers.get("candidate").and_then(Json::as_str),
        Some("E2-candidate"),
        "the committed answers must name the E2 candidate"
    );
    let Json::Obj(arms) = answers.get("arms").expect("arms member") else {
        panic!("answers.json arms must be an object");
    };
    let mut ids: Vec<&str> = arms.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = ARMS.iter().map(|a| a.id).collect();
    expected.sort_unstable();
    assert_eq!(ids, expected, "the committed answers' arm set ≠ ARMS");
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(
        report.is_green(),
        "committed E2 answers must verify 57/0: {:?}",
        report.failures()
    );
    assert_eq!(report.counts(), (57, 0));
}

/// The new arms' gated members agree member-for-member with the e3
/// committed record — the e2 port is semantics-faithful to the
/// reference, per-case: the pipeline verdict spellings, the sealed
/// members, and the WAL fold outputs.
#[test]
fn e2_extension_answers_match_the_e3_reference() {
    let e2 = load(&root().join("foreign/e2-python/answers.json"));
    let e3 = load(&root().join("foreign/e3-typescript/answers.json"));
    for case in HIR_CASES {
        for m in [
            "parse",
            "validate",
            "seal",
            "canonical_sha256",
            "sealed_sha256",
            "definition_version_id",
        ] {
            assert_eq!(
                member(&e2, "hir-documents", case, m),
                member(&e3, "hir-documents", case, m),
                "hir-documents {case}.{m}: e2 ≠ e3"
            );
        }
    }
    for case in SCENARIO_CASES {
        for m in ["event_hashes", "head_hash"] {
            assert_eq!(
                member(&e2, "ledger-scenarios", case, m),
                member(&e3, "ledger-scenarios", case, m),
                "ledger-scenarios {case}.{m}: e2 ≠ e3"
            );
        }
        // The informational members ride too — carried, never gated.
        for m in ["wal_sha256", "event_count"] {
            assert_eq!(
                member(&e2, "ledger-scenarios", &format!("{case}.{m}"), ""),
                member(&e3, "ledger-scenarios", &format!("{case}.{m}"), ""),
                "ledger-scenarios {case}.{m} (informational): e2 ≠ e3"
            );
        }
    }
}

/// A fresh `python3` replay over the committed bundle reproduces the
/// committed `answers.json` byte-for-byte on the gated members — the
/// committed record is a replay artifact, never a patch. Honest
/// `skipped(python3-absent)` when python3 is not on PATH.
#[test]
fn e2_fresh_replay_matches_committed_record() {
    let scratch = std::env::temp_dir().join(format!("hh-r416a-py-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let have_python = Command::new("/bin/sh")
        .args(["-c", "command -v python3"])
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .output()
        .is_ok_and(|o| o.status.success());
    let _ = std::fs::remove_dir_all(&scratch);
    if !have_python {
        eprintln!("r4_16a: skipped(python3-absent)");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("hh-e2-answers-{}", std::process::id()));
    let out = Command::new("python3")
        .arg(root().join("foreign/e2-python/foreign.py"))
        .arg(bundle_dir())
        .arg(&tmp)
        .output()
        .expect("spawn python3 foreign.py");
    assert!(
        out.status.success(),
        "foreign.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = load(&tmp);
    let committed = load(&root().join("foreign/e2-python/answers.json"));
    for spec in ARMS {
        let Json::Obj(expected) = load(&bundle_dir().join(spec.dir).join("expected.json")) else {
            panic!("expected.json must be an object");
        };
        let fa = fresh
            .get("arms")
            .and_then(|a| a.get(spec.id))
            .unwrap_or_else(|| panic!("fresh replay missing arm {}", spec.id));
        let ca = committed
            .get("arms")
            .and_then(|a| a.get(spec.id))
            .unwrap_or_else(|| panic!("committed record missing arm {}", spec.id));
        for (case, want) in &expected {
            // Informational members (digests, timing) are carried, never
            // gated — compare only the members expected.json pins.
            if hharness_xcheck::check::is_informational(spec.id, case) {
                continue;
            }
            if let Json::Obj(wm) = want {
                for (m, w) in wm {
                    if spec.id == "canonical-parse" && m == "bytes" {
                        continue; // `bytes` is a bundle fact, not an answer
                    }
                    assert_eq!(
                        fa.get(case).and_then(|c| c.get(m)),
                        Some(w),
                        "{} {case}.{m}: fresh replay ≠ expected",
                        spec.id
                    );
                    assert_eq!(
                        ca.get(case).and_then(|c| c.get(m)),
                        Some(w),
                        "{} {case}.{m}: committed record ≠ expected",
                        spec.id
                    );
                }
            } else {
                assert_eq!(
                    fa.get(case),
                    Some(want),
                    "{} {case}: fresh replay ≠ expected",
                    spec.id
                );
                assert_eq!(
                    ca.get(case),
                    Some(want),
                    "{} {case}: committed record ≠ expected",
                    spec.id
                );
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
}

/// The negative leg: an E2 answers file with either extension arm
/// dropped fails `verify` by name — `missing answer` on every case of
/// the dropped arm. The foreign-answer path is real: removing an arm
/// re-opens the recorded failure, never a silent pass.
#[test]
fn e2_dropped_extension_arm_fails_verify_by_name() {
    for dropped in ["hir-documents", "ledger-scenarios"] {
        let mut answers = load(&root().join("foreign/e2-python/answers.json"));
        let Json::Obj(top) = &mut answers else {
            unreachable!()
        };
        let Json::Obj(arms) = top.get_mut("arms").unwrap() else {
            panic!("answers.json arms must be an object");
        };
        arms.remove(dropped);
        let report = verify_answers(&bundle_dir(), &answers).unwrap();
        assert!(
            !report.is_green(),
            "{dropped}: dropping the arm must fail verify"
        );
        let missing = report
            .failures()
            .iter()
            .filter(|(arm, _case, detail)| *arm == dropped && detail.contains("missing answer"))
            .count();
        let want = if dropped == "hir-documents" { 5 } else { 3 };
        assert_eq!(
            missing, want,
            "{dropped}: expected {want} named `missing answer` failures"
        );
        // The other arm's cases must still verify — no collateral.
        assert!(report.failures().iter().all(|(arm, _, _)| *arm == dropped));
    }
}

/// CC1 — one ledger spelling: `arm_ledger_scenarios` and
/// `arm_ledger_transcript` both ride the shared `_replay_wal` fold (the
/// e3 `replayWal` refactor's shape), and it is defined exactly once.
#[test]
fn e2_ledger_arms_share_the_replay_wal_fold() {
    let src = std::fs::read_to_string(root().join("foreign/e2-python/foreign.py")).unwrap();
    assert_eq!(
        src.matches("def _replay_wal(").count(),
        1,
        "the WAL fold must be defined exactly once"
    );
    for f in ["arm_ledger_transcript", "arm_ledger_scenarios"] {
        let pos = src
            .find(&format!("def {f}"))
            .unwrap_or_else(|| panic!("{f} missing"));
        let body = &src[pos..pos + 1200.min(src.len() - pos)];
        assert!(
            body.contains("_replay_wal("),
            "{f} must ride the shared _replay_wal fold"
        );
    }
}
