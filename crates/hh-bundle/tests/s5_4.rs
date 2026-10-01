//! S5.4 — the R2 reproduction gate slice (§5h.9; R-2.12.1¹): R2 admits
//! only when every bound profile ref is pinned
//! (`resolved_dependencies.profile_refs[].pinned = true`) and every
//! bound image ref carries a digest
//! (`resolved_dependencies.images[].digest`) — `B-R2-profile` /
//! `B-R2-images` are `Missing` basis rows when the closure is absent,
//! never a silent admission.

use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::levels;
use hh_bundle::manifest::{BasisSatisfaction, BundlePolicy, ReproLevel};
use hh_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-s54-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

struct Rig {
    store: Store,
    run: String,
    _lease: Lease,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

/// A native run — `participant_class: native` so `derive` evaluates the
/// real R2 profile/image bases (hosted marks them `NotApplicable`).
fn native_rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.participant_class = ParticipantClass::Native;
    let (run, lease) = store.open_run(m, "s54.test").unwrap();
    Rig {
        store,
        run,
        _lease: lease,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn inputs<'a>(rig: &'a Rig, profile_refs: Vec<Json>, images: Vec<Json>) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("s54.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        profile_refs,
        images,
        nondeterminism: vec![],
        participant_class: None,
    }
}

fn basis<'a>(entries: &'a [hh_bundle::manifest::LevelBasis], id: &str) -> &'a BasisSatisfaction {
    &entries
        .iter()
        .find(|e| e.requirement_id == id)
        .unwrap_or_else(|| panic!("basis row {id}"))
        .satisfied_by
}

#[test]
fn r2_profile_and_image_bases_satisfied_when_pinned() {
    let rig = native_rig("pinned");
    let a = assemble(&inputs(
        &rig,
        vec![Json::obj([
            ("profile_ref", Json::str("profile:p-1")),
            ("pinned", Json::Bool(true)),
        ])],
        vec![Json::obj([
            ("ref", Json::str("img:runner")),
            ("digest", Json::str("sha256:img")),
        ])],
    ))
    .unwrap();

    // The rows ship on the resolved_dependencies section.
    let deps = &a.manifest.resolved_dependencies;
    match deps.get("profile_refs") {
        Some(Json::Arr(rs)) => assert_eq!(rs.len(), 1),
        other => panic!("profile_refs: {other:?}"),
    }
    match deps.get("images") {
        Some(Json::Arr(rs)) => assert_eq!(rs.len(), 1),
        other => panic!("images: {other:?}"),
    }

    let (_level, entries) = levels::derive(&a.manifest, false);
    for id in ["B-R2-profile", "B-R2-images"] {
        match basis(&entries, id) {
            BasisSatisfaction::Member(_) | BasisSatisfaction::NotApplicable(_) => {}
            other => panic!("{id} not satisfied: {other:?}"),
        }
        // The basis rows are R2-level rows.
        let e = entries.iter().find(|e| e.requirement_id == id).unwrap();
        assert_eq!(e.level, ReproLevel::R2);
    }
}

#[test]
fn r2_profile_basis_missing_when_unpinned() {
    let rig = native_rig("unpinned");
    let a = assemble(&inputs(
        &rig,
        vec![Json::obj([
            ("profile_ref", Json::str("profile:p-1")),
            ("pinned", Json::Bool(false)),
        ])],
        vec![],
    ))
    .unwrap();
    let (_level, entries) = levels::derive(&a.manifest, false);
    assert_eq!(basis(&entries, "B-R2-profile"), &BasisSatisfaction::Missing);
}

#[test]
fn r2_images_basis_missing_without_digest() {
    let rig = native_rig("nodigest");
    let a = assemble(&inputs(
        &rig,
        vec![],
        // A bound image without a digest is declared — never silently
        // absent — and the gate reports it.
        vec![Json::obj([("ref", Json::str("img:runner"))])],
    ))
    .unwrap();
    let (_level, entries) = levels::derive(&a.manifest, false);
    assert_eq!(basis(&entries, "B-R2-images"), &BasisSatisfaction::Missing);
}

#[test]
fn r2_bases_satisfy_on_empty_closure() {
    // No bound profile refs / images at all — `[]` satisfies by the
    // section member (nothing to pin; the declaration, not an implied
    // pin).
    let rig = native_rig("empty");
    let a = assemble(&inputs(&rig, vec![], vec![])).unwrap();
    let (_level, entries) = levels::derive(&a.manifest, false);
    for id in ["B-R2-profile", "B-R2-images"] {
        match basis(&entries, id) {
            BasisSatisfaction::Member(_) => {}
            other => panic!("{id} should satisfy on empty closure: {other:?}"),
        }
    }
}
