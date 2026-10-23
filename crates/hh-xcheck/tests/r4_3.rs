//! R4.3 acceptance battery — the `hh-xcheck-bundle/1` extension arms
//! (`hir-documents`, DF-S1.2-2's HIR-corpus leg; `ledger-scenarios`,
//! DF-S1.5-3's ledger-corpus leg). Invariants: the new arms replay green
//! through E1 with real coverage; the HIR pipeline verdicts are typed
//! (`sealed` / `invalid:<Kind>` / `refuse:<Kind>` / `"skipped"`); `verify`
//! fails *by name* on a corrupted or missing new-arm answer and skips the
//! scenario arm's informational members; `bundle.json` carries the
//! extended arms plus the honest pending cells (the committed e2 tree
//! cannot answer the new arms — recorded, never claimed).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_wire::json::{parse, Json};
use hharness_xcheck::check::{is_informational, self_check, verify_answers};
use hharness_xcheck::ARMS;

fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/cross-impl")
}

fn load(p: &Path) -> Json {
    parse(&String::from_utf8_lossy(&std::fs::read(p).unwrap())).unwrap()
}

/// `answers["arms"][<arm>]` — mutable access for the negative legs.
fn arm_obj<'a>(answers: &'a mut Json, arm: &str) -> &'a mut BTreeMap<String, Json> {
    let Json::Obj(top) = answers else {
        unreachable!()
    };
    let Json::Obj(arms_map) = top.get_mut("arms").unwrap() else {
        unreachable!()
    };
    let Json::Obj(m) = arms_map.get_mut(arm).unwrap() else {
        unreachable!()
    };
    m
}

/// The exact `answers.json` a *perfect* foreign run would produce — built
/// from the bundle's own `expected` files (skipping informational
/// members through the shared rule).
fn perfect_answers(bundle: &Path) -> Json {
    let mut arms = BTreeMap::new();
    for spec in ARMS {
        let Json::Obj(expected) = load(&bundle.join(spec.dir).join("expected.json")) else {
            panic!("expected.json must be an object");
        };
        let mut cases = BTreeMap::new();
        for (case, want) in &expected {
            if !is_informational(spec.id, case) {
                cases.insert(case.clone(), want.clone());
            }
        }
        arms.insert(spec.id.to_string(), Json::Obj(cases));
    }
    Json::obj([
        ("arms", Json::Obj(arms)),
        ("candidate", Json::str("E-test")),
    ])
}

/// The new arms carry real coverage — an empty arm is a failure, not a
/// vacuous green (the `pass >= 40` floor's per-arm twin).
#[test]
fn self_check_covers_the_new_arms() {
    let report = self_check(&bundle_dir()).unwrap();
    assert!(report.is_green(), "failures: {:?}", report.failures());
    for arm in ["hir-documents", "ledger-scenarios"] {
        let n = report.results.iter().filter(|r| r.arm == arm).count();
        assert!(
            n >= 3,
            "{arm} contributed only {n} cases — an arm went missing"
        );
    }
    // The HIR corpus exercises every outcome class.
    let hir_cases = report
        .results
        .iter()
        .filter(|r| r.arm == "hir-documents")
        .count();
    assert_eq!(hir_cases, 5, "hir-documents corpus must cover 5 cases");
}

/// The `hir-documents` verdict spellings are typed and honest: every
/// outcome class is present in the committed `expected.json` — sealed
/// docs, a validate-invalid, a seal-refusal and a parse-refusal.
#[test]
fn hir_documents_verdicts_are_typed() {
    let Json::Obj(expected) = load(&bundle_dir().join("arms/hir-documents/expected.json")) else {
        panic!("expected.json must be an object");
    };
    let get = |case: &str, member: &str| {
        expected
            .get(case)
            .and_then(|c| c.get(member))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(get("minimal-native", "seal"), "sealed");
    assert_eq!(get("minimal-native", "validate"), "valid");
    assert!(!get("minimal-native", "sealed_sha256").is_empty());
    assert_eq!(get("unresolved-ref", "validate"), "invalid:UnresolvedRef");
    assert_eq!(get("unresolved-ref", "seal"), "skipped");
    assert_eq!(get("gated-tool-no-contract", "validate"), "valid");
    assert_eq!(
        get("gated-tool-no-contract", "seal"),
        "refuse:ContributionUndeclared"
    );
    assert_eq!(
        get("missing-provenance", "parse"),
        "refuse:MissingProvenance"
    );
    assert_eq!(get("missing-provenance", "validate"), "skipped");
    assert!(
        load(&bundle_dir().join("arms/hir-documents/scenario.json"))
            .get("sealed_at")
            .is_some(),
        "the arm's sealed_at must be pinned"
    );
    assert_eq!(get("hosted-agent", "seal"), "sealed");
}

/// A corrupted foreign answer for a new-arm case fails `verify` by name.
#[test]
fn verify_fails_named_on_corrupted_new_arm_answer() {
    let mut answers = perfect_answers(&bundle_dir());
    let hir = arm_obj(&mut answers, "hir-documents");
    let Json::Obj(case) = hir.get_mut("minimal-native").unwrap() else {
        unreachable!()
    };
    case.insert("sealed_sha256".to_string(), Json::str("corrupted-digest"));
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(!report.is_green());
    assert!(report
        .failures()
        .iter()
        .any(|(arm, c, d)| *arm == "hir-documents"
            && *c == "minimal-native"
            && d.contains("sealed_sha256")));
}

/// A missing new-arm answer fails by name — silence is not agreement.
#[test]
fn verify_fails_named_on_missing_new_arm_answer() {
    let mut answers = perfect_answers(&bundle_dir());
    let scn = arm_obj(&mut answers, "ledger-scenarios");
    scn.remove("causal-chain");
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(!report.is_green());
    assert!(report
        .failures()
        .iter()
        .any(|(arm, c, d)| *arm == "ledger-scenarios"
            && *c == "causal-chain"
            && d.contains("missing")));
}

/// The scenario arm's `<case>.wal_sha256` / `<case>.event_count` members
/// are informational — a foreign answer never carries them and `verify`
/// never demands them.
#[test]
fn verify_skips_ledger_scenarios_informational_members() {
    let answers = perfect_answers(&bundle_dir());
    // The perfect answer already omits informational members — green.
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(
        report.is_green(),
        "informational members must not be demanded: {:?}",
        report.failures()
    );
    // And no expected member named `<case>.wal_sha256` is ever a verify case.
    assert!(report
        .results
        .iter()
        .all(|r| !(r.arm == "ledger-scenarios" && r.case.ends_with(".wal_sha256"))));
}

/// `bundle.json` declares the extended arms and the honest pending cells:
/// the new arms' E3 replays stay `environment-pending` on `HUMAN-H3` and
/// their E2 replays record the e2-tree extension they need — the
/// committed `foreign/e2-python` tree cannot answer them today (CF-493
/// vocabulary: `status` stays `environment-pending`, the `needs` member
/// names the real prerequisite).
#[test]
fn bundle_manifest_records_the_new_arms_pending_cells() {
    let Json::Obj(m) = load(&bundle_dir().join("bundle.json")) else {
        panic!("bundle.json must be an object");
    };
    let Json::Arr(arms) = m.get("arms").unwrap() else {
        panic!("arms must be an array");
    };
    assert_eq!(arms.len(), ARMS.len(), "bundle.json arms ≠ the ARMS table");
    for id in ["hir-documents", "ledger-scenarios"] {
        let arm = arms
            .iter()
            .find(|a| a.get("id").and_then(Json::as_str) == Some(id))
            .unwrap_or_else(|| panic!("arm {id} missing from bundle.json"));
        assert_eq!(
            arm.get("status").and_then(Json::as_str),
            Some("e1-self-checked")
        );
    }
    let Json::Arr(pending) = m.get("pending_cells").unwrap() else {
        panic!("pending_cells must be an array");
    };
    for (cell, needs) in [
        ("hir-documents.e2-replay", "e2-tree arm extension"),
        ("hir-documents.e3-replay", "HUMAN-H3"),
        ("ledger-scenarios.e2-replay", "e2-tree arm extension"),
        ("ledger-scenarios.e3-replay", "HUMAN-H3"),
    ] {
        let c = pending
            .iter()
            .find(|p| p.get("cell").and_then(Json::as_str) == Some(cell))
            .unwrap_or_else(|| panic!("pending cell {cell} missing"));
        assert_eq!(
            c.get("status").and_then(Json::as_str),
            Some("environment-pending"),
            "{cell}: pending status vocabulary must stay environment-pending"
        );
        assert_eq!(
            c.get("needs").and_then(Json::as_str),
            Some(needs),
            "{cell}: the honest prerequisite"
        );
        // CF-493 — `verified_by[]` records partial discharge honestly:
        // the e3-replay cells name `E3-candidate`'s recorded run; the
        // e2-replay cells stay empty (the committed e2 tree cannot
        // answer the extension arms).
        let Json::Arr(vs) = c
            .get("verified_by")
            .unwrap_or_else(|| panic!("{cell}: verified_by missing"))
        else {
            panic!("{cell}: verified_by not an array");
        };
        let candidates: Vec<&str> = vs
            .iter()
            .filter_map(|v| v.get("candidate").and_then(Json::as_str))
            .collect();
        let expected: &[&str] = if cell.ends_with(".e3-replay") {
            &["E3-candidate"]
        } else {
            &[]
        };
        assert_eq!(
            candidates, expected,
            "{cell}: verified_by ≠ the recorded evidence"
        );
    }
    // Every pinned file member exists on disk — `files` pins every member.
    let Json::Obj(files) = m.get("files").unwrap() else {
        panic!("files must be an object");
    };
    for rel in files.keys() {
        assert!(
            bundle_dir().join(rel).exists(),
            "bundle.json pins {rel} but the member is absent"
        );
    }
}
