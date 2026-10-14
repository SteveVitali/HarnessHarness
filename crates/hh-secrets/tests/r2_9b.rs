//! R2.9b acceptance battery — DF-S2.4-3, the Stage-3 `leak_scan` surfaces:
//! `leak_scan_run` sweeps the run's committed events **plus** the run
//! manifest (seq-0), the whole blob pool, and every
//! `measurement.export.delivered` row re-tagged under its `sink_delivery`
//! surface. A planted value on any of those surfaces is a `Leak` attributed
//! to the surface that carried it; a clean run scans `∅` (LT-01).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-r29b-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn open_with_manifest(tag: &str, manifest: RunManifest) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest, "writer-a").unwrap();
    (s, run, lease)
}

fn k_ev(store: &Store, run_id: &str, id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

fn spec() -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: SecretSource::OperatorVault {
            vault_ref: "vault:github".into(),
        },
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            revocation_path: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
        allowed_env_names: Some(["GH_API_TOKEN".into()].into_iter().collect()),
        delivery_modes: [hh_monitor::assess::SecretTransport::ProxyInjected]
            .into_iter()
            .collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "test channel".into(),
    }
}

fn broker_with(value: &str) -> CredentialBroker {
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::OperatorVault {
            vault_ref: "vault:github".into(),
        },
        value,
    );
    let mut b = CredentialBroker::new(Box::new(vault), "test-fp-key");
    b.register_channel(
        "github",
        spec(),
        ProvenanceRecord::kernel("kernel:test", 1_000),
    )
    .unwrap();
    b
}

const SECRET: &str = "ghp_real_token_value_0123456789abcdef";

// ── blob pool ──────────────────────────────────────────────────────────────

/// A value staged in the blob pool (bundle member, report, exported
/// artefact) is a `blob`-attributed leak — the pool sweep, not the event
/// sweep, is what names it.
#[test]
fn blob_pool_leak_is_attributed_to_the_blob() {
    let (mut store, run, _lease) = open("blob-leak");
    let broker = broker_with(SECRET);
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());

    let body = format!("report body\nAuthorization: Bearer {SECRET}\n");
    let addr = store.put_blob(body.as_bytes(), "text/plain").unwrap();

    let leaks = broker.leak_scan_run(&store, &run, &detectors).unwrap();
    let blob_hits: Vec<_> = leaks
        .iter()
        .filter(|l| matches!(l.target, ScanTarget::Blob(_)))
        .collect();
    assert_eq!(blob_hits.len(), 1, "expected the pool hit: {leaks:?}");
    assert_eq!(blob_hits[0].detector, DetectorKind::KnownValue);
    assert_eq!(
        blob_hits[0].location_string(),
        format!("blob:{}", addr.digest)
    );
}

// ── run manifest ───────────────────────────────────────────────────────────

/// A value on the seq-0 manifest record is a `manifest`-attributed leak —
/// the immutable record is in the scan domain.
#[test]
fn manifest_leak_is_attributed_to_the_manifest() {
    let mut m = RunManifest::minimal(RunKind::Agent);
    // `extra` is the manifest's verbatim-trailing member — a planted value
    // there reaches the seq-0 record without tripping a ref-format check.
    m.extra.insert("note".to_string(), Json::str(SECRET));
    let (store, run, _lease) = open_with_manifest("manifest-leak", m);
    let broker = broker_with(SECRET);
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());

    let leaks = broker.leak_scan_run(&store, &run, &detectors).unwrap();
    assert!(
        leaks
            .iter()
            .any(|l| matches!(l.target, ScanTarget::Manifest(_))
                && l.detector == DetectorKind::KnownValue),
        "expected a manifest-attributed hit: {leaks:?}"
    );
}

// ── measurement.export.delivered ──────────────────────────────────────────

/// A value on a delivered-export row reports twice: under the `event`
/// coordinate and under the `sink_delivery` surface the row names — the
/// leak attributes to the leg that carried it.
#[test]
fn export_delivered_row_reports_its_sink_delivery_leg() {
    let (mut store, run, lease) = open("export-leak");
    let broker = broker_with(SECRET);
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());

    let ev = k_ev(
        &store,
        &run,
        "evt-export-1",
        "measurement.export.delivered",
        Json::obj([
            ("sink_id", Json::str(format!("sink://{SECRET}"))),
            ("view_kind", Json::str("report")),
            ("seq_range", Json::Arr(vec![Json::Int(1), Json::Int(2)])),
            ("content_classes", Json::Arr(vec![Json::str("events")])),
        ]),
    );
    store.append(&run, &lease, vec![ev]).unwrap();

    let leaks = broker.leak_scan_run(&store, &run, &detectors).unwrap();
    assert!(
        leaks
            .iter()
            .any(|l| matches!(l.target, ScanTarget::Event(Some(ref id)) if id == "evt-export-1")),
        "the row scans as an event: {leaks:?}"
    );
    assert!(
        leaks
            .iter()
            .any(|l| matches!(l.target, ScanTarget::SinkDelivery(ref s) if s.contains("sink://"))),
        "the row re-tags under the delivery leg: {leaks:?}"
    );
}

// ── the clean-run verdict (LT-01 over the widened domain) ──────────────────

/// Placeholder spellings are the safe form and arbitrary blobs/manifests
/// hold no value: a run with a bound binding, a benign pool object and a
/// delivered row scans `∅`.
#[test]
fn clean_run_scans_empty_across_all_surfaces() {
    let (mut store, run, lease) = open("clean");
    let broker = broker_with(SECRET);
    let detectors = DetectorSet::standard(broker.mask_set().unwrap());

    store.put_blob(b"clean report body", "text/plain").unwrap();
    let ev = k_ev(
        &store,
        &run,
        "evt-export-1",
        "measurement.export.delivered",
        Json::obj([
            ("sink_id", Json::str("sink://archive")),
            ("view_kind", Json::str("report")),
            ("seq_range", Json::Arr(vec![Json::Int(1), Json::Int(1)])),
            ("content_classes", Json::Arr(vec![Json::str("events")])),
        ]),
    );
    store.append(&run, &lease, vec![ev]).unwrap();

    let leaks = broker.leak_scan_run(&store, &run, &detectors).unwrap();
    assert!(leaks.is_empty(), "clean run must scan ∅: {leaks:?}");
}
