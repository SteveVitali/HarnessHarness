//! `hh-results` S4.2 tests — the bundle status vocabulary (the
//! eight-member ladder), `set_status`'s evidence gates, the
//! ledger-folded status book, and the lifecycle terminals' annotation
//! flags (§5h.3 §2; AC-R-2.9.3-11).

use std::path::PathBuf;

use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_results::catalogue::BundleStatus;
use hh_results::error::ResultsError;
use hh_results::status::{fold, load_book, set_status, STATUS_EVENT};
use hh_results::store::ResultsStore;
use hh_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-results-status-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn rig(tag: &str) -> (Store, ResultsStore, String) {
    let root = tmp(tag);
    let mut store = Store::open(root.join("store")).unwrap();
    let (run, _l) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "s42.test")
        .unwrap();
    let results = ResultsStore::open(root.join("results")).unwrap();
    (store, results, run)
}

fn valid_report() -> Json {
    Json::obj([(
        "validation_report",
        Json::obj([("status", Json::str("valid"))]),
    )])
}

#[test]
fn transitions_are_gated_by_evidence() {
    let (mut store, results, run) = rig("gates");
    let b = "idp:bundle-1";

    // `validated` without a `valid` report refuses.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Validated,
        "",
        vec![],
        Json::Null,
        Json::Null,
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("validation"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Validated,
        "",
        vec![],
        Json::Null,
        valid_report(),
    )
    .unwrap();

    // `attested` requires the attestation document.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Attested,
        "",
        vec![],
        Json::Null,
        Json::Null,
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("attestation"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Attested,
        "",
        vec![],
        Json::Null,
        Json::obj([("attestation", Json::obj([("bundle_id", Json::str(b))]))]),
    )
    .unwrap();

    // `reproduced` requires an independent reproduced verdict.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Reproduced,
        "",
        vec![],
        Json::Null,
        Json::obj([(
            "repro_report",
            Json::obj([
                ("bundle_id", Json::str(b)),
                ("verdict", Json::str("reproduced")),
                ("independent", Json::Bool(false)),
            ]),
        )]),
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("reproduced"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    // A report for *another* bundle is not evidence for this one
    // (S4.4 — the gate binds `repro_report.bundle_id` to the subject).
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Reproduced,
        "",
        vec![],
        Json::Null,
        Json::obj([(
            "repro_report",
            Json::obj([
                ("bundle_id", Json::str("idp:bundle-other")),
                ("verdict", Json::str("reproduced")),
                ("independent", Json::Bool(true)),
            ]),
        )]),
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("reproduced"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Reproduced,
        "",
        vec![],
        Json::Null,
        Json::obj([(
            "repro_report",
            Json::obj([
                ("bundle_id", Json::str(b)),
                ("verdict", Json::str("reproduced")),
                ("independent", Json::Bool(true)),
            ]),
        )]),
    )
    .unwrap();
    assert_eq!(
        load_book(&results).unwrap().status(b),
        BundleStatus::Reproduced
    );
}

#[test]
fn published_restricted_toggle_and_terminal_rules() {
    let (mut store, results, run) = rig("monotone");
    let b = "idp:bundle-2";

    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Validated,
        "",
        vec![],
        Json::Null,
        valid_report(),
    )
    .unwrap();
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Published,
        "",
        vec![],
        Json::Null,
        valid_report(),
    )
    .unwrap();
    // published ↔ restricted is legal.
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Restricted,
        "audience re-scope",
        vec!["team".into()],
        Json::Null,
        valid_report(),
    )
    .unwrap();
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Published,
        "",
        vec![],
        Json::Null,
        valid_report(),
    )
    .unwrap();
    // Backwards off the ladder refuses.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Validated,
        "",
        vec![],
        Json::Null,
        valid_report(),
    ) {
        Err(ResultsError::StatusGateFailed { .. }) => {}
        _ => panic!("expected StatusGateFailed"),
    }
    // `superseded` requires `evidence.successor`.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Superseded,
        "",
        vec![],
        Json::Null,
        Json::Null,
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("successor"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Superseded,
        "",
        vec![],
        Json::Null,
        Json::obj([("successor", Json::str("idp:bundle-3"))]),
    )
    .unwrap();
    // A terminal never leaves — even to retracted.
    match set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Retracted,
        "why",
        vec![],
        Json::Null,
        Json::Null,
    ) {
        Err(ResultsError::StatusGateFailed { .. }) => {}
        _ => panic!("expected StatusGateFailed"),
    }
    // `retracted` requires a reason — from a non-terminal bundle.
    let b2 = "idp:bundle-4";
    match set_status(
        &mut store,
        &results,
        &run,
        b2,
        BundleStatus::Retracted,
        "",
        vec![],
        Json::Null,
        Json::Null,
    ) {
        Err(ResultsError::StatusGateFailed { detail }) => {
            assert!(detail.contains("reason"), "{detail}")
        }
        _ => panic!("expected StatusGateFailed"),
    }
    set_status(
        &mut store,
        &results,
        &run,
        b2,
        BundleStatus::Retracted,
        "bad data",
        vec![],
        Json::Null,
        Json::Null,
    )
    .unwrap();
}

#[test]
fn the_book_folds_the_ledger_and_the_rows_record() {
    let (mut store, results, run) = rig("fold");
    let b = "idp:bundle-5";
    set_status(
        &mut store,
        &results,
        &run,
        b,
        BundleStatus::Validated,
        "",
        vec![],
        Json::Null,
        valid_report(),
    )
    .unwrap();
    // The row is on the ledger.
    assert!(store
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == STATUS_EVENT));
    // The fold rebuilds what the book cached.
    let book = fold(&store);
    assert_eq!(book.status(b), BundleStatus::Validated);
    assert_eq!(book.history(b).len(), 1);
    // An unknown bundle is `assembled` (the floor).
    assert_eq!(book.status("idp:never"), BundleStatus::Assembled);
}
