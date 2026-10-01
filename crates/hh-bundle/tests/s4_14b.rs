//! S4.14b acceptance — the bundle-attestation slice of R-2.8.6¹ (§5g.6
//! D5; ADR-0038/0067): `traces.<run>.audit_tree_head` carries the newest
//! *signed* `security.audit.checkpoint` claim — `{checkpoint_ref,
//! tree_size, tree_head, signatures[]}` verbatim — so an auditor verifies
//! the bundle against the writer's own signed head. A run with no signed
//! checkpoint attests to nothing (`None`, never fabricated).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_bundle::export::{build_ledger_export, MemberBytes};
use hh_bundle::manifest::LedgerExport;
use hh_ledger::audit::{CheckpointKind, FixedSigner};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-bundle-s414b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn ev(store: &Store, run_id: &str, id: &str, class: &str) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload: Json::obj([]),
    }
}

#[test]
fn audit_tree_head_carries_the_signed_checkpoint_claim() {
    let mut store = Store::open_test(dir("head"), 1_000).unwrap();
    let mut manifest = RunManifest::minimal(RunKind::Agent);
    manifest.signer_key_ids = vec!["test-key".to_string()];
    let (run_id, lease) = store.open_run(manifest, "writer-a").unwrap();
    store
        .append(
            &run_id,
            &lease,
            vec![ev(&store, &run_id, "evt-1", "model.call.requested")],
        )
        .unwrap();
    let checkpoint = store
        .checkpoint(
            &run_id,
            &lease,
            CheckpointKind::OnDemand,
            &mut FixedSigner::new("test-key", b"s4.14b-key".to_vec()),
        )
        .expect("signed checkpoint");

    let mut members = MemberBytes::new();
    let (export, _roles) = build_ledger_export(&store, &run_id, &mut members).unwrap();
    let head = export
        .audit_tree_head
        .clone()
        .expect("a signed checkpoint attests");
    // The cited ref is the checkpoint row itself; tree_size/tree_head come
    // from the signed claim, signatures verbatim.
    assert_eq!(
        head.checkpoint_ref.get("event_id").and_then(Json::as_str),
        Some(checkpoint.event_id.as_str())
    );
    assert!(head.tree_size > 0);
    assert!(!head.tree_head.is_empty());
    assert_eq!(head.signatures.len(), 1);
    assert_eq!(
        head.signatures[0].get("key_id").and_then(Json::as_str),
        Some("test-key")
    );

    // Round-trip: the member survives encode→decode byte-faithful.
    let decoded = LedgerExport::from_json(&export.to_json()).unwrap();
    assert_eq!(decoded.audit_tree_head, Some(head));
}

#[test]
fn audit_tree_head_absent_without_a_signed_checkpoint() {
    let mut store = Store::open_test(dir("none"), 1_000).unwrap();
    let manifest = RunManifest::minimal(RunKind::Agent);
    let (run_id, lease) = store.open_run(manifest, "writer-a").unwrap();
    store
        .append(
            &run_id,
            &lease,
            vec![ev(&store, &run_id, "evt-1", "model.call.requested")],
        )
        .unwrap();

    let mut members = MemberBytes::new();
    let (export, _roles) = build_ledger_export(&store, &run_id, &mut members).unwrap();
    assert_eq!(export.audit_tree_head, None);
    // Absent on the wire too — additive member, never a null placeholder.
    assert!(export.to_json().get("audit_tree_head").is_none());
}
