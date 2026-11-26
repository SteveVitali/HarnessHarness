//! R6.55 — the corrected V5 sealed document (ticket 325d; ADR-0453).
//!
//! W4's bound V5 doc spelled its pins `version_selector:"sha256:<vid>"`
//! — the grammar admits `latest | label:<l> | version:<vid> | range: |
//! deny:` only (`resolve.rs parse_selector`), so the slot resolve
//! refused `RefUnresolved` and the arm recorded `verifier_unavailable`.
//! The correction is a NEW sealed artifact minted through the real
//! `resolve` → `seal` path whose authored selectors spell `version:<vid>`
//! against the kernel Stage-1 seed (`suites::seed_stage1`) — the same
//! content-derived `version_id`s a slot's store publishes, so the sealed
//! pins resolve on the window's probe/store verbatim.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use hharness_assembly::resolve::{resolve, ResolveEnv};
use hharness_assembly::Stage1Catalog;
use hharness_hir::document::HirDocument;
use hharness_identity::names::ResolveMode;
use hharness_lab::sweep::corrected_v5;
use hharness_provenance::ProvenanceRecord;
use hharness_registry::store::RegistryStore;
use hharness_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-lab-r655-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn env<'a>(store: &'a mut RegistryStore, cat: &'a Stage1Catalog) -> ResolveEnv<'a> {
    ResolveEnv {
        registry: store,
        catalog: cat,
        snapshot_id: None,
        mode: ResolveMode::Audit,
        registrar: ProvenanceRecord::kernel("sweep:test", 0),
        resolved_at: 1_763_000_000_000,
        notices: None,
    }
}

/// AC-1 — the mint succeeds: `version:<vid>` selectors resolve, the
/// sealed doc pins the kernel seed's real vids, `model_port` binds
/// `hh/gateway_http` with the env-mediated param set, and the artifact
/// identity is new (a re-seal is a new artifact, never an edit).
#[test]
fn corrected_v5_mints_a_new_grammar_conformant_seal() {
    let c = corrected_v5(&dir("mint")).unwrap();
    assert_ne!(
        c.version_id, "sha256:38d1389a2e8254aa31b7a59e519529cdaad987fe7cd4ad4d9cc9d29f0ef5c29c",
        "the corrected seal must mint a new vid — the original stays untouched"
    );
    let doc = c
        .sealed_json
        .get("document")
        .unwrap_or_else(|| panic!("sealed carries document"));
    let slots = doc
        .get("assembly")
        .and_then(|a| a.get("slots"))
        .unwrap_or_else(|| panic!("assembly slots"));
    let variant = |slot: &str| {
        slots
            .get(slot)
            .and_then(|s| s.get("one"))
            .and_then(|o| o.get("variant"))
            .unwrap_or_else(|| panic!("{slot} variant"))
    };
    assert_eq!(
        variant("model_port")
            .get("variant_id")
            .and_then(Json::as_str),
        Some("hh/gateway_http")
    );
    for (slot, name) in [
        ("model_port", "hh/gateway_http"),
        ("control_strategy", "hh/round_robin"),
        ("context_policy", "hh/full_window"),
    ] {
        assert_eq!(
            variant(slot).get("version_id").and_then(Json::as_str),
            Some(c.bound_vids[slot].as_str()),
            "{slot} must pin the published {name} vid"
        );
        // no selector survives the seal — the ref is pinned
        assert!(variant(slot).get("version_selector").is_none(), "{slot}");
    }
    // the env-mediated param set rides the sealed slot verbatim
    let params = slots
        .get("model_port")
        .and_then(|s| s.get("one"))
        .and_then(|o| o.get("params"))
        .unwrap_or_else(|| panic!("model_port params"));
    assert_eq!(
        params.get("credential_ref").and_then(Json::as_str),
        Some("env:HH_L1_API_KEY")
    );
    assert_eq!(
        params
            .get("transcript_render")
            .and_then(|t| t.get("assistant_replay"))
            .and_then(|t| t.get("reasoning"))
            .and_then(Json::as_str),
        Some("drop"),
        "the declared rsn-drop override is applied"
    );
}

/// AC-2 — the RefUnresolved leg flips honestly: the sealed doc's
/// `document`, parsed and re-resolved on a *fresh* kernel-seeded store
/// (the slot's probe/store shape), resolves — the idempotent path the
/// W4 drive never reached.
#[test]
fn corrected_doc_resolves_on_a_fresh_kernel_seeded_store() {
    let c = corrected_v5(&dir("mint2")).unwrap();
    let doc_json = c
        .sealed_json
        .get("document")
        .cloned()
        .unwrap_or_else(|| panic!("sealed carries document"));
    let doc: HirDocument =
        hharness_hir::wire::document_from_json(&doc_json).expect("document parses");

    let mut store = RegistryStore::open(
        dir("probe").join("registry"),
        &ProvenanceRecord::kernel("sweep:test", 0),
    )
    .unwrap();
    hharness_registry::suites::seed_stage1(&mut store);
    let cat = Stage1Catalog::stage1();
    let mut e = env(&mut store, &cat);
    let resealed = resolve(&doc, &mut e).unwrap_or_else(|diags| {
        panic!(
            "corrected doc must resolve on a kernel-seeded store: {:?}",
            diags.iter().map(|d| format!("{:?}:{}", d.code, d.subject))
        )
    });
    assert_eq!(resealed.definition_ref.version_id, c.version_id);
}

/// AC-3 — the regression guard: the *defective* spelling still refuses.
/// Re-spelling any slot `version_selector:"sha256:<vid>"` must fail
/// `RefUnresolved` — the exact diagnostic the W4 run record shows.
#[test]
fn defective_sha256_selector_still_refuses_ref_unresolved() {
    let c = corrected_v5(&dir("mint3")).unwrap();
    let mut doc_json = c
        .sealed_json
        .get("document")
        .cloned()
        .unwrap_or_else(|| panic!("sealed carries document"));
    // re-spell one slot's pin as the defective `sha256:<vid>` selector —
    // the spelling W4's bound doc carried.
    let Json::Obj(dm) = &mut doc_json else {
        panic!("document object")
    };
    let Json::Obj(am) = dm.get_mut("assembly").unwrap_or_else(|| panic!("assembly")) else {
        panic!("assembly object")
    };
    let Json::Obj(sm) = am.get_mut("slots").unwrap_or_else(|| panic!("slots")) else {
        panic!("slots object")
    };
    let Json::Obj(om) = sm
        .get_mut("model_port")
        .and_then(|s| {
            if let Json::Obj(sm2) = s {
                sm2.get_mut("one")
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("model_port one"))
    else {
        panic!("one object")
    };
    let Json::Obj(vm) = om.get_mut("variant").unwrap_or_else(|| panic!("variant")) else {
        panic!("variant object")
    };
    let pin = vm
        .get("version_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    vm.remove("version_id");
    vm.insert("version_selector".to_string(), Json::str(pin));
    let doc: HirDocument =
        hharness_hir::wire::document_from_json(&doc_json).expect("mutated doc parses");
    // the kernel-seeded store — every other pin resolves, so the only
    // refusal is the defective selector spelling itself.
    let mut store = RegistryStore::open(
        dir("bad").join("registry"),
        &ProvenanceRecord::kernel("sweep:test", 0),
    )
    .unwrap();
    hharness_registry::suites::seed_stage1(&mut store);
    let cat = Stage1Catalog::stage1();
    let mut e = env(&mut store, &cat);
    let diags = resolve(&doc, &mut e).expect_err("sha256:<vid> selector must refuse");
    assert!(
        diags
            .iter()
            .any(|d| format!("{:?}", d.code) == "RefUnresolved"),
        "expected RefUnresolved, got: {:?}",
        diags.iter().map(|d| format!("{:?}", d.code))
    );
    let subjects: Vec<String> = diags.iter().map(|d| d.subject.clone()).collect();
    assert!(
        subjects.iter().any(|s| s.starts_with("sha256:")),
        "the refusing subject is the bad selector spelling: {subjects:?}"
    );
}
