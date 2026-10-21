//! R4.4a acceptance battery — the `foreign/e3-typescript` part-2 replay
//! (rescoped by the ticket's appended 2026-10-21 amendment): the
//! plugin-manifests and ledger-scenarios arms over R4.4's shared `lib/`
//! core, the committed seven-arm `answers.json`, and the split's label
//! fixes (`scripts/xcheck-e3.sh`, `foreign.ts` header, `r4_4.rs`
//! `COVERED_ARMS`). `hir-documents` is R4.4b's — its five cells stay
//! `missing answer` pending cells, never claimed.
//!
//! Invariants: the committed `answers.json` answers exactly the seven
//! delivered arms; every delivered case verifies green and the five
//! `hir-documents` cases fail only as `missing answer`; the typed
//! verdict vocabulary is the corpus's (`admit:<id>` / `refuse:<Kind>`,
//! `event_hashes`/`head_hash` folds); a corrupted new-arm answer fails
//! `verify` by name (silence is not agreement); `xcheck-e3.sh` reports
//! `pending (R4.4b) cells missing: 5` end-to-end; and the committed
//! `foreign/e2-python` tree carries exactly the six original `arm_*`
//! functions — the recorded e2 absence for the R4.3 extension arms
//! (an e2-tree arm extension is its own operator-visible change; when
//! it lands this record updates with it).

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

fn committed_answers() -> Json {
    load(&root().join("foreign/e3-typescript/answers.json"))
}

/// The seven arms R4.4a delivers answers for (R4.4's five + this
/// ticket's plugin-manifests and ledger-scenarios).
const DELIVERED_ARMS: &[&str] = &[
    "identity-trees",
    "identity-records",
    "ledger-transcript",
    "registry-snapshot",
    "plugin-manifests",
    "canonical-parse",
    "ledger-scenarios",
];

/// The committed `answers.json` answers exactly the seven delivered
/// arms — `hir-documents` absent (R4.4b lands it; its arrival turns the
/// r4_4 battery's pending-arm assertion red, and this set-pin names the
/// boundary in this ticket's own battery).
#[test]
fn answers_cover_exactly_the_seven_delivered_arms() {
    let answers = committed_answers();
    assert_eq!(
        answers.get("candidate").and_then(Json::as_str),
        Some("E3-candidate"),
        "the committed answers must name the E3 candidate"
    );
    let Json::Obj(arms) = answers.get("arms").cloned().unwrap_or(Json::Null) else {
        panic!("answers must carry arms{{}}");
    };
    let mut ids: Vec<&str> = arms.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let mut want = DELIVERED_ARMS.to_vec();
    want.sort_unstable();
    assert_eq!(
        ids, want,
        "the committed answers must carry exactly the seven delivered arms"
    );
}

/// Every delivered case verifies green; the sole failures are the five
/// `hir-documents` cells reporting `missing answer` — the amended AC 1's
/// pending-cell pattern (52 pass / 5 missing at authoring; the assertion
/// is the *shape*, not a vacuous green).
#[test]
fn verify_green_except_the_hir_documents_pending_cells() {
    let report = verify_answers(&bundle_dir(), &committed_answers()).unwrap();
    let failures = report.failures();
    for (arm, case, detail) in &failures {
        assert_eq!(
            (*arm, *detail),
            ("hir-documents", "missing answer"),
            "non-pending failure {arm} {case}: {detail}"
        );
    }
    assert_eq!(
        failures.len(),
        5,
        "exactly the five hir-documents cases are pending: {failures:?}"
    );
    // Real coverage, never a vacuous green: the delivered arms account
    // for every passing case.
    let (pass, _) = report.counts();
    assert!(
        pass >= 50,
        "only {pass} passing cases — an arm went missing"
    );
}

/// The plugin-manifests arm's fourteen verdicts verify green and spell
/// the corpus's typed vocabulary — `admit:<id>` for the two valid
/// manifests, `refuse:<Kind>` for the twelve invalid ones.
#[test]
fn plugin_manifests_verdicts_are_typed_and_green() {
    let answers = committed_answers();
    let Json::Obj(arm) = answers
        .get("arms")
        .and_then(|a| a.get("plugin-manifests"))
        .cloned()
        .unwrap_or(Json::Null)
    else {
        panic!("answers must carry the plugin-manifests arm");
    };
    let mut admits = 0usize;
    let mut refuses = 0usize;
    for (case, verdict) in &arm {
        let v = verdict
            .as_str()
            .unwrap_or_else(|| panic!("{case}: verdict must be a string"));
        if let Some(id) = v.strip_prefix("admit:") {
            assert!(
                id.starts_with("hh/") || id.starts_with("local/"),
                "{case}: admitted id {id:?} is not a registry id"
            );
            admits += 1;
        } else if let Some(kind) = v.strip_prefix("refuse:") {
            assert!(!kind.is_empty(), "{case}: empty refuse kind");
            refuses += 1;
        } else {
            panic!("{case}: verdict {v:?} is outside the admit/refuse vocabulary");
        }
    }
    assert_eq!(
        (admits, refuses),
        (2, 12),
        "the corpus pins 2 admits + 12 refuses"
    );
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(
        report
            .results
            .iter()
            .filter(|r| r.arm == "plugin-manifests")
            .all(|r| r.outcome.is_ok()),
        "plugin-manifests must verify green"
    );
}

/// The ledger-scenarios arm's three cases verify green, each folding to
/// `event_hashes` + `head_hash` with the head equal to the last event
/// hash — the shared `replayWal` fold over the per-case WALs.
#[test]
fn ledger_scenarios_cases_verify_green() {
    let answers = committed_answers();
    let Json::Obj(arm) = answers
        .get("arms")
        .and_then(|a| a.get("ledger-scenarios"))
        .cloned()
        .unwrap_or(Json::Null)
    else {
        panic!("answers must carry the ledger-scenarios arm");
    };
    let mut cases = 0usize;
    for (case, answer) in &arm {
        if case.contains('.') {
            continue; // informational `<case>.wal_sha256` / `.event_count`
        }
        cases += 1;
        let Json::Arr(hashes) = answer.get("event_hashes").cloned().unwrap_or(Json::Null) else {
            panic!("{case}: event_hashes must be an array");
        };
        assert!(!hashes.is_empty(), "{case}: empty event_hashes");
        assert_eq!(
            answer.get("head_hash"),
            hashes.last(),
            "{case}: head_hash must equal the last event hash"
        );
    }
    assert_eq!(cases, 3, "the arm carries three WAL scenario cases");
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(
        report
            .results
            .iter()
            .filter(|r| r.arm == "ledger-scenarios")
            .all(|r| r.outcome.is_ok()),
        "ledger-scenarios must verify green"
    );
}

/// A corrupted new-arm answer fails `verify` by name — the comparator
/// really checks these arms; silence is not agreement.
#[test]
fn verify_fails_named_on_corrupted_new_arm_answer() {
    let mut answers = committed_answers();
    let Json::Obj(top) = &mut answers else {
        unreachable!()
    };
    let Json::Obj(arms) = top.get_mut("arms").unwrap() else {
        unreachable!()
    };
    let Json::Obj(pm) = arms.get_mut("plugin-manifests").unwrap() else {
        unreachable!()
    };
    pm.insert(
        "valid/first-party-variant.json".to_string(),
        Json::str("refuse:SchemaViolation"),
    );
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(!report.is_green());
    assert!(report
        .failures()
        .iter()
        .any(|(arm, c, d)| *arm == "plugin-manifests"
            && *c == "valid/first-party-variant.json"
            && d.contains("refuse:SchemaViolation")));
}

/// `xcheck-e3.sh` end-to-end on a node-present PATH: the replay verifies
/// and the pending-cell label names R4.4b (`hir-documents` is its arm —
/// a label regressing to `(R4.4a)` or a covered-arm failure fails here).
/// `skipped(node-absent)` is the honest report when node < 22 is absent.
#[test]
fn xcheck_e3_end_to_end_reports_r4_4b_pending_cells() {
    match node_major() {
        Some(v) if v >= 22 => {}
        other => {
            eprintln!("r4_4a: skipped(node-absent) — node >= 22 required, found {other:?}");
            return;
        }
    }
    let out = Command::new("bash")
        .arg(root().join("scripts/xcheck-e3.sh"))
        .output()
        .expect("spawn xcheck-e3.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "xcheck-e3.sh must reconcile the pending cells:\n{stdout}"
    );
    assert!(
        stdout.contains("covered-arm failures: 0; pending (R4.4b) cells missing: 5"),
        "the pending-cell label must name R4.4b with five cells:\n{stdout}"
    );
}

/// The recorded e2 absence (the amendment's non-optional deliverable):
/// `foreign/e2-python/foreign.py` carries exactly the six original
/// `arm_*` functions — `arm_ledger_transcript` is the only WAL consumer;
/// no `arm_ledger_scenarios`/`arm_hir_documents`. An e2-tree arm
/// extension is its own operator-visible change; when it lands, this
/// pin and the dated DEFERRALS notes update with it.
#[test]
fn e2_tree_carries_exactly_the_six_original_arms() {
    let py = std::fs::read_to_string(root().join("foreign/e2-python/foreign.py")).unwrap();
    let arms: Vec<&str> = py
        .lines()
        .filter_map(|l| l.strip_prefix("def arm_"))
        .collect();
    assert_eq!(
        arms.len(),
        6,
        "foreign.py must carry exactly the six original arm_* functions: {arms:?}"
    );
    for absent in ["arm_ledger_scenarios", "arm_hir_documents"] {
        assert!(
            !py.contains(&format!("def {absent}")),
            "the recorded e2 absence is stale — foreign.py grew {absent}"
        );
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
