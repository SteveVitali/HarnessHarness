//! R4.4b acceptance battery — the `hir-documents` arm of the E3
//! (TypeScript) foreign replay, plus the boundary invariants the ticket
//! (docs/tickets/153b_R4.4b__e3-typescript-foreign-impl-part3.md)
//! asserts:
//!
//! * the committed `answers.json` carries all five `hir-documents` case
//!   answers and `verify` passes them green (no `missing answer` cells);
//! * the fresh node replay agrees with the committed record on the gated
//!   members (the recorded answers are the replay, not a patch);
//! * the E2 python tree still carries exactly its original six `arm_*`
//!   functions and no `arm_hir_documents` (the e2 limitation is a
//!   documented absence — `hir-documents.e2-replay` stays
//!   `environment-pending`, never edited or fabricated);
//! * `scripts/xcheck-e3.sh` reports zero pending cells.

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

/// The five committed hir-documents cases — each must carry a real answer
/// in the committed record and verify green.
const HIR_CASES: &[&str] = &[
    "minimal-native",
    "hosted-agent",
    "gated-tool-no-contract",
    "missing-provenance",
    "unresolved-ref",
];

/// The committed `answers.json` answers all five hir-documents cases and
/// `verify` passes every hir-documents cell — the R4.4b-owned pending
/// cells are resolved, nothing left `missing answer`.
#[test]
fn e3_hir_documents_answers_verify() {
    let answers = load(&root().join("foreign/e3-typescript/answers.json"));
    let Json::Obj(hir) = answers
        .get("arms")
        .and_then(|a| a.get("hir-documents"))
        .cloned()
        .unwrap_or(Json::Null)
    else {
        panic!("committed answers must carry the hir-documents arm");
    };
    for case in HIR_CASES {
        assert!(
            hir.contains_key(*case),
            "hir-documents case {case} missing from the committed answers"
        );
    }
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    let mut hir_cells = 0usize;
    for r in &report.results {
        assert!(
            r.outcome.is_ok(),
            "case {} {} must verify — {:?}",
            r.arm,
            r.case,
            r.outcome
        );
        if r.arm == "hir-documents" {
            hir_cells += 1;
        }
    }
    assert_eq!(hir_cells, 5, "expected all five hir-documents cells");
    let (passed, failed) = report.counts();
    assert_eq!(
        (passed, failed),
        (57, 0),
        "no cells may remain failing or missing — the E3 replay is fully green"
    );
}

/// The sealed cases carry the E1-verified sealed members — the honest
/// verdict spellings are the pipeline outcomes (`skipped` gating and the
/// `refuse:<Kind>` / `invalid:<Kind>` joins), never a fabricated result.
#[test]
fn e3_hir_documents_case_verdicts() {
    let answers = load(&root().join("foreign/e3-typescript/answers.json"));
    let hir = answers
        .get("arms")
        .and_then(|a| a.get("hir-documents"))
        .expect("hir-documents arm present");
    let member = |case: &str, m: &str| hir.get(case).and_then(|c| c.get(m)).cloned();
    // missing-provenance refuses at parse; validate/seal stay `skipped`.
    assert_eq!(
        member("missing-provenance", "parse"),
        Some(Json::str("refuse:MissingProvenance"))
    );
    assert_eq!(
        member("missing-provenance", "validate"),
        Some(Json::str("skipped"))
    );
    assert_eq!(
        member("missing-provenance", "seal"),
        Some(Json::str("skipped"))
    );
    // unresolved-ref parses, then refuses validation; seal skipped.
    assert_eq!(member("unresolved-ref", "parse"), Some(Json::str("ok")));
    assert_eq!(
        member("unresolved-ref", "validate"),
        Some(Json::str("invalid:UnresolvedRef"))
    );
    assert_eq!(member("unresolved-ref", "seal"), Some(Json::str("skipped")));
    // the gated tool validates but refuses sealing (no flow contract).
    assert_eq!(
        member("gated-tool-no-contract", "validate"),
        Some(Json::str("valid"))
    );
    assert_eq!(
        member("gated-tool-no-contract", "seal"),
        Some(Json::str("refuse:ContributionUndeclared"))
    );
    // the sealed legs emit a definition version id + sealed hash.
    for case in ["minimal-native", "hosted-agent"] {
        assert_eq!(member(case, "seal"), Some(Json::str("sealed")));
        let def = member(case, "definition_version_id").unwrap();
        assert!(
            def.as_str().is_some_and(|s| s.starts_with("sha256:")),
            "{case}: sealed doc must carry a sha256: definition version id"
        );
        assert!(member(case, "sealed_sha256").unwrap().as_str().is_some());
    }
}

/// Fresh node replay: the hir-documents answers the tree produces equal
/// the committed record member-for-member (replay, not a patched answer).
/// `skipped(node-absent)` honestly when node < 22 is not on PATH.
#[test]
fn e3_hir_documents_fresh_replay_matches_committed() {
    let Some(v) = node_major() else {
        eprintln!("r4_4b: skipped(node-absent) — node >= 22 required");
        return;
    };
    assert!(
        v >= 22,
        "r4_4b: skipped(node-absent) — node >= 22 required, found {v}"
    );
    let tmp = std::env::temp_dir().join(format!("hh-e3b-answers-{}", std::process::id()));
    let out = Command::new("node")
        .arg(root().join("foreign/e3-typescript/foreign.ts"))
        .arg(bundle_dir())
        .arg(&tmp)
        .output()
        .expect("spawn node foreign.ts");
    assert!(
        out.status.success(),
        "foreign.ts failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = load(&tmp);
    let committed = load(&root().join("foreign/e3-typescript/answers.json"));
    for case in HIR_CASES {
        let Json::Obj(fc) = fresh
            .get("arms")
            .and_then(|a| a.get("hir-documents"))
            .and_then(|h| h.get(case))
            .cloned()
            .unwrap_or(Json::Null)
        else {
            panic!("fresh replay missing hir-documents case {case}");
        };
        let Json::Obj(cc) = committed
            .get("arms")
            .and_then(|a| a.get("hir-documents"))
            .and_then(|h| h.get(case))
            .cloned()
            .unwrap_or(Json::Null)
        else {
            panic!("committed record missing hir-documents case {case}");
        };
        assert_eq!(
            fc, cc,
            "hir-documents {case}: fresh replay and committed record disagree"
        );
    }
    let _ = std::fs::remove_file(&tmp);
}

/// The E2 boundary: `foreign/e2-python/foreign.py` still defines exactly
/// the original six `arm_*` functions and no `arm_hir_documents` — the
/// e2 tree is not edited by this ticket (the dated note in the e2 README
/// records the absence; `hir-documents.e2-replay` stays
/// environment-pending).
#[test]
fn e2_tree_has_six_arms_and_no_hir_documents() {
    let src = std::fs::read_to_string(root().join("foreign/e2-python/foreign.py")).unwrap();
    let arms: Vec<&str> = src
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("def arm_"))
        .collect();
    assert_eq!(
        arms.len(),
        6,
        "e2 must still carry exactly its six original arm functions: {arms:?}"
    );
    assert!(
        !src.contains("arm_hir_documents") && !src.contains("hir-documents"),
        "the e2 tree must not grow a hir-documents arm under this ticket"
    );
}

/// `xcheck-e3.sh` over the committed bundle answers zero pending cells —
/// all eight arms are covered by the E3 tree as of R4.4b.
/// `skipped(node-absent)` honestly when node < 22 is not on PATH.
#[test]
fn xcheck_e3_reports_zero_pending_cells() {
    let Some(v) = node_major() else {
        eprintln!("r4_4b: skipped(node-absent) — node >= 22 required");
        return;
    };
    if v < 22 {
        eprintln!("r4_4b: skipped(node-absent) — node >= 22 required, found {v}");
        return;
    }
    let xcheck = root().join("target/debug/hh-xcheck");
    if !xcheck.exists() {
        eprintln!("r4_4b: skipped — target/debug/hh-xcheck not built");
        return;
    }
    let out = Command::new("bash")
        .arg(root().join("scripts/xcheck-e3.sh"))
        .env("HH_XCHECK_BIN", &xcheck)
        .output()
        .expect("spawn xcheck-e3.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "xcheck-e3.sh failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("covered-arm failures: 0; pending (R4.4b) cells missing: 0"),
        "the E3 leg must report 0 covered failures and 0 pending cells:\n{stdout}"
    );
}

/// The installed node major version, or `None` when node is not on PATH.
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
