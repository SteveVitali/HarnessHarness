//! R2.2 acceptance coverage — the R-2.2.1 retention legs (BL-11;
//! DF-S1.5-1/DF-S2.9-1's residual; ADR-0068 R4; ADR-0333):
//! declared `RetentionPolicy` → durable `lifecycle.ledger.tier_transition`
//! rows before byte mutation; `HHZ1` cold compression under the same
//! content address; `gc` at the call site with fork-prefix + bound-set
//! pins; `subscribe` replay answering the spec'd `Rewind` — never a
//! fabricated frame; `ReplayValidityReport` rebuilding over a compacted
//! prefix; the R4 floor ordering refused on inversion.
//! Each test fails if the behaviour it covers is removed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::branch::{BranchKind, ForkOpts, NavigateTarget};
use hh_ledger::errors::{LedgerError, MissingReason};
use hh_ledger::event::{Cursor, Event, EventFrame, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::replay::{self, ValidityInput};
use hh_ledger::retention::{self, RetentionPolicy, RetentionTier};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-r22-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease, ManualClock) {
    open_at(dir(tag), 1_000)
}

fn open_at(d: PathBuf, ms: u64) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(ms);
    let mut s = Store::open_with(
        d,
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000; // residency advances stay inside the lease
    let (run, lease) = s.open_run(m, "writer-a").unwrap();
    (s, run, lease, clock)
}

fn reopen(d: &std::path::Path, ms: u64) -> Store {
    Store::open_with(
        d,
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::starting_at(1_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn policy(residency_ms: u64, bundle_ms: u64) -> RetentionPolicy {
    RetentionPolicy {
        policy_ref: "policy:r2.2-test".to_string(),
        blob_residency_ms: residency_ms,
        bundle_retention_ms: bundle_ms,
        compress_cold: true,
    }
}

/// A kernel-stamped row (audit-grade classes admit kernel producers only).
fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn tier_rows(s: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    s.envelopes(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.ledger.tier_transition")
        .cloned()
        .collect()
}

fn blob_file(s: &Store, id: &str) -> PathBuf {
    let parsed = hh_identity::idp::parse_id(id).unwrap();
    s.root().join("blobs").join(&parsed.digest_hex)
}

fn is_cold_on_disk(s: &Store, id: &str) -> bool {
    retention::is_compressed(&std::fs::read(blob_file(s, id)).unwrap())
}

/// A blob whose `id` rides an event's `refs` (a live prefix reference).
fn blob_with_ref(
    s: &mut Store,
    run: &str,
    lease: &Lease,
    body: &[u8],
    id: &str,
) -> hh_identity::idp::ContentAddress {
    let blob = s.put_blob(body, "text/plain").unwrap();
    let mut e = ev(
        id,
        "lifecycle.hosted.native_record",
        Json::obj([("n", Json::str("ref"))]),
    );
    e.refs = vec![blob.clone()];
    s.append(run, lease, vec![e]).unwrap();
    blob
}

// ── AC-R-2.2.1-1 — declared policy → durable transitions ───────────────

#[test]
fn ac_r221_1_policy_drives_durable_tier_transitions() {
    let (mut s, run, lease, clock) = open("tiers");
    let body = b"hot blob bytes - transition me".repeat(8);
    let blob = s.put_blob(&body, "text/plain").unwrap();
    let id = blob.id();
    // First pass: observed → `none → hot`.
    let r = s
        .evaluate_retention(&run, &lease, &policy(60_000, 120_000), &BTreeSet::new())
        .unwrap();
    assert_eq!(r.policy_ref, "policy:r2.2-test");
    assert_eq!(r.transitions.len(), 1);
    assert_eq!(r.transitions[0].address, id);
    assert!(!r.transitions[0].from_tracked);
    assert_eq!(r.transitions[0].to_tier, RetentionTier::Hot);
    let rows = tier_rows(&s, &run);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].payload.get("from_tier").and_then(Json::as_str),
        Some("none")
    );
    assert_eq!(
        rows[0].payload.get("to_tier").and_then(Json::as_str),
        Some("hot")
    );
    assert_eq!(
        rows[0].payload.get("policy_ref").and_then(Json::as_str),
        Some("policy:r2.2-test")
    );
    // Within residency a second pass is a no-op — no duplicate rows.
    let r = s
        .evaluate_retention(&run, &lease, &policy(60_000, 120_000), &BTreeSet::new())
        .unwrap();
    assert!(r.transitions.is_empty());
    // Residency elapsed → `hot → cold` + HHZ1 at rest; the address still
    // answers the original bytes (file name is the plaintext digest).
    clock.advance(61_000);
    let r = s
        .evaluate_retention(&run, &lease, &policy(60_000, 120_000), &BTreeSet::new())
        .unwrap();
    assert_eq!(r.transitions.len(), 1);
    assert_eq!(r.transitions[0].from_tier, RetentionTier::Hot);
    assert_eq!(r.transitions[0].to_tier, RetentionTier::Cold);
    assert_eq!(r.compressed, 1);
    assert!(is_cold_on_disk(&s, &id));
    assert_eq!(s.get_blob(&blob).unwrap(), body);
    assert_eq!(tier_rows(&s, &run).len(), 2, "exactly two transition rows");
}

#[test]
fn ac_r221_1_tier_fold_survives_restart_and_cold_is_absorbing() {
    let d = dir("tiers-restart");
    let (mut s, run, lease, _c) = open_at(d.clone(), 1_000);
    let blob = s.put_blob(&b"restart me".repeat(4), "text/plain").unwrap();
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    assert!(is_cold_on_disk(&s, &blob.id()));
    drop(s);
    // Reopen past the first lease's expiry — the tier fold rebuilds from
    // the durable rows; a fresh pass emits nothing and rewrites nothing.
    let mut s2 = reopen(&d, 2_000_000);
    let lease2 = s2.acquire_writer("writer-a", &run, 60_000).unwrap();
    let r = s2
        .evaluate_retention(&run, &lease2, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    assert!(
        r.transitions.is_empty(),
        "cold is absorbing: {:?}",
        r.transitions
    );
    assert_eq!(s2.get_blob(&blob).unwrap(), b"restart me".repeat(4));
}

// ── AC-R-2.2.1-2 — ReplayValidityReport over a compacted prefix ────────

#[test]
fn ac_r221_2_replay_validity_rebuilds_over_compacted_cold_prefix() {
    let d = dir("replay-cold");
    let (mut s, run, lease, _c) = open_at(d.clone(), 1_000);
    let blob = blob_with_ref(
        &mut s,
        &run,
        &lease,
        &b"prefix evidence".repeat(16),
        "with-ref",
    );
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    assert!(is_cold_on_disk(&s, &blob.id()));
    let input = ValidityInput::default();
    let before = replay::validity(&s, &run, &input).unwrap();
    // Reopen — durable-state rebuild over compacted bytes is identical.
    let s2 = reopen(&d, 2_000);
    let after = replay::validity(&s2, &run, &input).unwrap();
    assert_eq!(before.mode, after.mode);
    assert_eq!(before.to_json(), after.to_json());
    assert_eq!(s2.get_blob(&blob).unwrap(), b"prefix evidence".repeat(16));
}

// ── AC-R-2.2.1-3 — gc call-site row + the pin set ──────────────────────

#[test]
fn ac_r221_3_gc_row_durable_before_delete_and_typed_missing_after() {
    let (mut s, run, lease, _c) = open("gc-cold");
    let blob = s.put_blob(b"collect me", "text/plain").unwrap();
    let id = blob.id();
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    let env = s
        .gc(
            &run,
            &lease,
            vec![id.clone()],
            "policy:r2.2-test",
            "cold",
            None,
        )
        .unwrap();
    assert_eq!(env.class, "lifecycle.ledger.gc");
    assert_eq!(
        env.payload.get("addresses"),
        Some(&Json::Arr(vec![Json::str(&id)]))
    );
    assert_eq!(env.payload.get("tier").and_then(Json::as_str), Some("cold"));
    assert!(!blob_file(&s, &id).exists());
    match s.get_blob(&blob) {
        Err(LedgerError::Missing { reason, .. }) => assert_eq!(reason, MissingReason::Gc),
        other => panic!("expected Missing{{gc}}, got {other:?}"),
    }
}

#[test]
fn ac_r221_3_gc_honours_fork_pin_and_bound_set() {
    let (mut s, run, lease, _c) = open("gc-pins");
    // The live-fork pin (R-2.2.4⁰ᵇ — re-verify, do not rewrite).
    let blob = blob_with_ref(&mut s, &run, &lease, b"pinned evidence", "with-ref");
    let cut = s.head(&run).unwrap().seq;
    s.fork(
        &run,
        cut,
        BranchKind::Branch,
        &ForkOpts::default(),
        RunManifest::minimal(RunKind::Agent),
        "writer-a",
    )
    .unwrap();
    match s.gc(
        &run,
        &lease,
        vec![blob.id()],
        "policy:r2.2-test",
        "hot",
        None,
    ) {
        Err(LedgerError::Pinned { reason, .. }) => {
            assert!(reason.starts_with("referenced_by_run:"), "{reason}")
        }
        other => panic!("expected Pinned, got {other:?}"),
    }
    // The caller-declared bound set (retained-bundle / leaderboard
    // citations — CF-347) pins identically via `gc_bounded`.
    let cited = s.put_blob(b"leaderboard evidence", "text/plain").unwrap();
    let bound = BTreeSet::from([cited.id()]);
    match s.gc_bounded(
        &run,
        &lease,
        vec![cited.id()],
        "policy:r2.2-test",
        "hot",
        None,
        &bound,
    ) {
        Err(LedgerError::Pinned { reason, .. }) => {
            assert_eq!(reason, "bound_by_retention_policy")
        }
        other => panic!("expected Pinned, got {other:?}"),
    }
    // Retention keeps bound bytes warm while an unbound sibling is hot.
    let unbound = s.put_blob(b"ordinary bytes", "text/plain").unwrap();
    let r = s
        .evaluate_retention(&run, &lease, &policy(60_000, 120_000), &bound)
        .unwrap();
    let warm = r
        .transitions
        .iter()
        .find(|t| t.address == cited.id())
        .unwrap();
    assert_eq!(warm.to_tier, RetentionTier::Warm);
    let hot = r
        .transitions
        .iter()
        .find(|t| t.address == unbound.id())
        .unwrap();
    assert_eq!(hot.to_tier, RetentionTier::Hot);
}

#[test]
fn ac_r221_3_bound_ages_warm_to_cold_after_bundle_retention() {
    let (mut s, run, lease, clock) = open("bound-ages");
    let cited = s
        .put_blob(&b"bundle cited".repeat(4), "text/plain")
        .unwrap();
    let bound = BTreeSet::from([cited.id()]);
    s.evaluate_retention(&run, &lease, &policy(10_000, 50_000), &bound)
        .unwrap();
    // Past blob residency, inside bundle retention — the citation floor
    // holds it warm (unbound content would be cold already).
    clock.advance(30_000);
    let r = s
        .evaluate_retention(&run, &lease, &policy(10_000, 50_000), &bound)
        .unwrap();
    assert!(r.transitions.is_empty(), "bound content stays warm");
    assert!(!is_cold_on_disk(&s, &cited.id()));
    clock.advance(30_000);
    let r = s
        .evaluate_retention(&run, &lease, &policy(10_000, 50_000), &bound)
        .unwrap();
    let t = r
        .transitions
        .iter()
        .find(|t| t.address == cited.id())
        .unwrap();
    assert_eq!(t.from_tier, RetentionTier::Warm);
    assert_eq!(t.to_tier, RetentionTier::Cold);
}

// ── AC-R-2.2.1-4 — subscribe over a compacted prefix ───────────────────

#[test]
fn ac_r221_4_subscribe_replays_the_specd_rewind_and_never_fabricates() {
    let (mut s, run, lease, _c) = open("sub-rewind");
    for i in 0..3 {
        s.append(
            &run,
            &lease,
            vec![ev(
                &format!("pre-{i}"),
                "lifecycle.hosted.native_record",
                Json::obj([("n", Json::Int(i))]),
            )],
        )
        .unwrap();
    }
    let moved = s
        .navigate(&run, &lease, NavigateTarget::Seq(1), "spec'd rewind")
        .unwrap();
    s.append(
        &run,
        &lease,
        vec![ev(
            "post",
            "lifecycle.hosted.native_record",
            Json::obj([("n", Json::Int(99))]),
        )],
    )
    .unwrap();
    // Compact a blob behind the prefix — the "compacted prefix" the AC names.
    let blob = s.put_blob(b"prefix content", "text/plain").unwrap();
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    let mut sub = s.subscribe(&run, Cursor::Seq(0)).unwrap();
    let mut frames = Vec::new();
    while let Some(f) = sub.try_next() {
        let done = matches!(f, EventFrame::Sync { .. });
        frames.push(f);
        if done {
            break;
        }
    }
    // Every durable row replays — events never leave (ADR-0068); the
    // compacted prefix still answers its envelopes.
    let durables = frames
        .iter()
        .filter(|f| matches!(f, EventFrame::Durable { .. }))
        .count();
    assert!(durables >= 6, "all prefix rows replay, got {durables}");
    // The spec'd rewind frame follows the replayed `head.moved` row —
    // the same shape live subscribers get (§5a.1 §4).
    let pos = frames
        .iter()
        .position(|f| matches!(f, EventFrame::Rewind { .. }))
        .expect("a replayed head.moved answers a rewind frame");
    match &frames[pos - 1] {
        EventFrame::Durable { event, .. } => {
            assert_eq!(event.class, "lifecycle.head.moved");
            assert_eq!(event.event_id, moved.event_id);
        }
        other => panic!("rewind follows the durable head.moved, got {other:?}"),
    }
    match &frames[pos] {
        EventFrame::Rewind {
            to_seq,
            to_event_id,
            reason,
        } => {
            assert_eq!(*to_seq, 1);
            assert_eq!(reason, "spec'd rewind");
            assert_eq!(
                Some(to_event_id.as_str()),
                moved.payload.get("to_event_id").and_then(Json::as_str)
            );
        }
        _ => unreachable!(),
    }
    assert!(matches!(frames.last(), Some(EventFrame::Sync { .. })));
    // Typed n/a — compacted bytes answer; after gc the typed `Missing{gc}`.
    assert_eq!(s.get_blob(&blob).unwrap(), b"prefix content");
    s.gc(
        &run,
        &lease,
        vec![blob.id()],
        "policy:r2.2-test",
        "cold",
        None,
    )
    .unwrap();
    match s.get_blob(&blob) {
        Err(LedgerError::Missing { reason, .. }) => assert_eq!(reason, MissingReason::Gc),
        other => panic!("expected Missing{{gc}}, got {other:?}"),
    }
}

// ── AC-R-2.2.1-5 — HHZ1 codec + BlobCorrupt on bad frames ──────────────

#[test]
fn ac_r221_5_hhz1_round_trips_and_refuses_corruption() {
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8],
        b"aaaa".to_vec(),
        b"the quick brown fox jumps over the lazy dog. ".repeat(64),
        (0..=255u8).cycle().take(70000).collect(), // exercises the window
        b"json{\"k\":\"v\",\"arr\":[1,2,3]}".repeat(200),
    ];
    for (i, raw) in cases.iter().enumerate() {
        let frame = retention::compress(raw);
        assert!(retention::is_compressed(&frame), "case {i}");
        assert_eq!(&frame[..4], b"HHZ1");
        assert_eq!(
            &retention::decompress(&frame).unwrap(),
            raw,
            "case {i} round-trips"
        );
        assert_eq!(retention::compress(raw), frame, "case {i} deterministic");
    }
    assert!(retention::decompress(b"not-a-frame").is_err());
    let mut bad = retention::compress(b"hello world hello world");
    bad.truncate(bad.len() - 3);
    assert!(retention::decompress(&bad).is_err());
    let mut bad2 = retention::compress(b"hello");
    bad2[4] = 0xFF;
    assert!(retention::decompress(&bad2).is_err());
}

#[test]
fn ac_r221_5_get_blob_reports_blob_corrupt_on_bad_cold_frame() {
    let (mut s, run, lease, _c) = open("corrupt-frame");
    let blob = s.put_blob(b"integrity", "text/plain").unwrap();
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    let path = blob_file(&s, &blob.id());
    let mut frame = std::fs::read(&path).unwrap();
    assert!(retention::is_compressed(&frame));
    *frame.last_mut().unwrap() ^= 0xFF;
    std::fs::write(&path, &frame).unwrap();
    match s.get_blob(&blob) {
        Err(LedgerError::BlobCorrupt { .. }) => {}
        other => panic!("expected BlobCorrupt, got {other:?}"),
    }
}

// ── AC-R-2.2.1-6 — the R4 floor ordering + tier-row subjects never pin ─

#[test]
fn ac_r221_6_policy_floor_ordering_refuses_inversion() {
    let (mut s, run, lease, _c) = open("floors");
    s.put_blob(b"x", "text/plain").unwrap();
    // bundle_retention < blob_residency inverts R4 (manifests above blobs).
    match s.evaluate_retention(&run, &lease, &policy(50_000, 10_000), &BTreeSet::new()) {
        Err(LedgerError::SchemaViolation { detail }) => {
            assert!(detail.contains("floor ordering"), "{detail}")
        }
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
    let unnamed = RetentionPolicy {
        policy_ref: String::new(),
        ..policy(1, 1)
    };
    match s.evaluate_retention(&run, &lease, &unnamed, &BTreeSet::new()) {
        Err(LedgerError::SchemaViolation { .. }) => {}
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
    let p = policy(1_000, 2_000);
    assert_eq!(RetentionPolicy::from_json(&p.to_json()).unwrap(), p);
    assert!(RetentionPolicy::from_json(&Json::obj([
        ("policy_ref", Json::str("p")),
        ("blob_residency_ms", Json::Int(5)),
        ("bundle_retention_ms", Json::Int(1)),
        ("compress_cold", Json::Bool(false)),
    ]))
    .is_err());
}

#[test]
fn ac_r221_transition_rows_do_not_pin_their_subjects() {
    let (mut s, run, lease, _c) = open("tier-no-pin");
    // A tier row names its subject — it is not a content dependency
    // (ADR-0333 D3); the run can gc its own tiered subject.
    let blob = blob_with_ref(&mut s, &run, &lease, b"subject", "with-ref");
    s.evaluate_retention(&run, &lease, &policy(0, 0), &BTreeSet::new())
        .unwrap();
    s.gc(
        &run,
        &lease,
        vec![blob.id()],
        "policy:r2.2-test",
        "cold",
        None,
    )
    .unwrap();
    match s.get_blob(&blob) {
        Err(LedgerError::Missing {
            reason: MissingReason::Gc,
            ..
        }) => {}
        other => panic!("expected Missing{{gc}}, got {other:?}"),
    }
}
