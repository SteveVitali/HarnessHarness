//! R2.21 acceptance battery — the cross-implementation replay/checker
//! packaging (spec §10.7; ADR-0353; DF-S0.3-2, DF-S1.2-2, DF-S1.5-3,
//! DF-S1.8-1, DF-S1.27-1 progress legs).
//!
//! Invariants under test: the committed `fixtures/cross-impl` bundle
//! self-checks green through E1 (AC: hermetic E1 replay); `export` is
//! byte-deterministic (a stale/tampered bundle fails the suite — the CC7 leg);
//! `verify_answers` accepts an exact foreign answer file and fails *by name*
//! on a wrong or missing case (never a vacuous pass); and the bundle itself
//! carries the honesty record (`pending_cells[]` — the foreign cells this
//! ticket does not close).
//!
//! Nothing here is a foreign-toolchain claim — the environment-pending cells
//! are enumerated in `bundle.json` and stay open in `DEFERRALS.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hh_wire::json::{parse, Json};
use hh_xcheck::check::{self_check, verify_answers};
use hh_xcheck::export::export_bundle;
use hh_xcheck::trees::{manifest_to_tree, tree_manifest};
use hh_xcheck::ARMS;

fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/cross-impl")
}

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hh-xcheck-test-{tag}-{}", std::process::id()))
}

fn load(p: &Path) -> Json {
    parse(&String::from_utf8_lossy(&std::fs::read(p).unwrap())).unwrap()
}

/// Every file under `dir`, relative path → bytes.
fn tree_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(
                    p.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    std::fs::read(&p).unwrap(),
                );
            }
        }
    }
    out
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
/// from the bundle's own `expected` files (skipping the informational
/// members the comparator doesn't gate on).
fn perfect_answers(bundle: &Path) -> Json {
    let mut arms = BTreeMap::new();
    for spec in ARMS {
        let Json::Obj(expected) = load(&bundle.join(spec.dir).join("expected.json")) else {
            panic!("expected.json must be an object");
        };
        let mut cases = BTreeMap::new();
        for (case, want) in &expected {
            let informational = matches!(
                (spec.id, case.as_str()),
                ("registry-snapshot", "registry_sha256")
                    | ("ledger-transcript", "wal_sha256")
                    | ("ledger-transcript", "event_count")
            );
            if !informational {
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

/// AC: the committed bundle replays green through E1 — every arm, every
/// case, hermetically. Fails if any arm's replay leg is removed.
#[test]
fn committed_bundle_self_checks_green() {
    let report = self_check(&bundle_dir()).unwrap();
    assert!(report.is_green(), "failures: {:?}", report.failures());
    // The floor pins coverage breadth — a silently-empty arm is a failure.
    let (pass, fail) = report.counts();
    assert_eq!(fail, 0);
    assert!(pass >= 40, "only {pass} cases — an arm went missing");
}

/// CC7 leg: `export` is deterministic — a fresh export is byte-identical to
/// the committed bundle (file set AND contents). A drifted corpus or a
/// tampered fixture fails here.
#[test]
fn export_is_byte_deterministic_against_committed_bundle() {
    let tmp = tmp("export");
    let _ = std::fs::remove_dir_all(&tmp);
    export_bundle(&tmp).unwrap();
    let fresh = tree_files(&tmp);
    let committed = tree_files(&bundle_dir());
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(fresh.len(), committed.len(), "file set drifted");
    for (rel, bytes) in &committed {
        assert_eq!(fresh.get(rel), Some(bytes), "bundle member {rel} drifted");
    }
}

/// The tree-manifest codec round-trips every golden fixture — the input leg
/// a foreign implementation reads must decode to the same tree.
#[test]
fn tree_manifest_codec_round_trips() {
    for (name, tree) in hh_identity::corpus::golden_tree_fixtures() {
        let manifest = tree_manifest(&tree);
        let back = manifest_to_tree(&manifest).unwrap();
        assert_eq!(back, tree, "manifest round-trip changed {name}");
        assert_eq!(
            back.address().id(),
            tree.address().id(),
            "round-tripped {name} addresses differently"
        );
    }
}

/// A byte-exact `answers.json` verifies green — the comparator's positive
/// leg (a foreign run's happy path).
#[test]
fn verify_accepts_exact_answers() {
    let report = verify_answers(&bundle_dir(), &perfect_answers(&bundle_dir())).unwrap();
    let (pass, fail) = report.counts();
    assert!(report.is_green(), "failures: {:?}", report.failures());
    assert!(pass > 0);
    assert_eq!(fail, 0);
}

/// A wrong answer fails *by name* — never a vacuous green.
#[test]
fn verify_fails_named_on_wrong_answer() {
    let mut answers = perfect_answers(&bundle_dir());
    let trees = arm_obj(&mut answers, "identity-trees");
    let case = trees.keys().next().unwrap().clone();
    trees.insert(case.clone(), Json::str("sha256:wrong"));
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(!report.is_green());
    assert!(report
        .failures()
        .iter()
        .any(|(arm, c, _)| *arm == "identity-trees" && *c == case));
}

/// A missing answer fails — silence is not agreement.
#[test]
fn verify_fails_named_on_missing_answer() {
    let mut answers = perfect_answers(&bundle_dir());
    let plugins = arm_obj(&mut answers, "plugin-manifests");
    let case = plugins.keys().next().unwrap().clone();
    plugins.remove(&case);
    let report = verify_answers(&bundle_dir(), &answers).unwrap();
    assert!(!report.is_green());
    assert!(report
        .failures()
        .iter()
        .any(|(arm, c, d)| *arm == "plugin-manifests" && *c == case && d.contains("missing")));
}

/// Honesty record: `bundle.json` declares the format, the six arms, and a
/// non-empty `pending_cells[]` — every one `environment-pending` on
/// `HUMAN-H3`. The bundle carries the "no foreign claim" record itself.
#[test]
fn bundle_manifest_carries_the_honesty_record() {
    let Json::Obj(m) = load(&bundle_dir().join("bundle.json")) else {
        panic!("bundle.json must be an object");
    };
    assert_eq!(
        m.get("format").and_then(Json::as_str),
        Some(hh_xcheck::BUNDLE_FORMAT)
    );
    let Json::Arr(arms) = m.get("arms").unwrap() else {
        panic!("arms must be an array");
    };
    assert_eq!(arms.len(), 6, "an arm went missing from bundle.json");
    let Json::Arr(pending) = m.get("pending_cells").unwrap() else {
        panic!("pending_cells must be an array");
    };
    assert!(!pending.is_empty(), "pending_cells must not be empty");
    for cell in pending {
        assert_eq!(
            cell.get("status").and_then(Json::as_str),
            Some("environment-pending"),
            "a pending cell lost its honesty record"
        );
    }
}

/// `CHECKER.md` travels with the bundle — the committed copy is byte-equal
/// to the crate's contract document.
#[test]
fn checker_doc_travels_with_the_bundle() {
    let committed = std::fs::read(bundle_dir().join("CHECKER.md")).unwrap();
    let source = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("CHECKER.md")).unwrap();
    assert_eq!(committed, source);
}
