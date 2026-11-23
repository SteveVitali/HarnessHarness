//! R6.52 (manifest 325a) — writer-lease TTL representability at the
//! shared ledger boundary.
//!
//! The W4 live drive declared its writer TTL through
//! `HH_WRITER_TTL_MS` (crates/hh-embed/src/open.rs), which accepts any
//! parsed `u64`. The lease record and `run.created.lease_ttl.writer`
//! serialize as signed JSON milliseconds, so a zero TTL (expired at
//! creation), a TTL above `i64::MAX` (negative JSON) or a TTL whose
//! expiry overflows at the current clock must refuse typed **before**
//! any durable act — never panic, wrap or clamp. These controls pin
//! the mechanical predicate only; no maximum-TTL policy is chosen.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_ledger::errors::LedgerError;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};

const NOW_MS: u64 = 1_763_540_000_000; // a positive, realistic epoch-ms clock

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "hh-ledger-r652-ttl-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str) -> (Store, ManualClock, PathBuf) {
    let d = dir(tag);
    let clock = ManualClock::at(NOW_MS);
    let s = Store::open_with(
        &d,
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    (s, clock, d)
}

fn manifest(writer_ms: u64) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = writer_ms;
    m
}

/// Every refusal must leave no run behind — not in the store, not on disk.
fn assert_refused_without_durable_run(tag: &str, writer_ms: u64) {
    let (mut s, _clock, d) = store(tag);
    let r = s.open_run(manifest(writer_ms), "writer-a");
    assert!(
        matches!(r, Err(LedgerError::ManifestInvalid { .. })),
        "writer_ms {writer_ms}: expected ManifestInvalid, got {r:?}"
    );
    assert!(
        s.run_ids().is_empty(),
        "writer_ms {writer_ms}: a run was created"
    );
    // The store root holds store-level files (`blobs`, `store.tag`, an
    // empty `runs/`); it must hold no run directory under `runs/`.
    let runs: Vec<_> = std::fs::read_dir(d.join("runs"))
        .map(|it| it.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(
        runs.is_empty(),
        "writer_ms {writer_ms}: durable run residue {runs:?}"
    );
}

#[test]
fn zero_writer_ttl_refuses_before_any_durable_creation() {
    assert_refused_without_durable_run("zero", 0);
}

#[test]
fn writer_ttl_above_i64_max_refuses_typed() {
    assert_refused_without_durable_run("i64max1", i64::MAX as u64 + 1);
    assert_refused_without_durable_run("u64max", u64::MAX);
}

#[test]
fn writer_ttl_that_fits_but_overflows_signed_expiry_refuses() {
    // i64::MAX itself is a representable TTL, but now + TTL is not.
    assert_refused_without_durable_run("i64max", i64::MAX as u64);
    assert_refused_without_durable_run("edge", i64::MAX as u64 - NOW_MS + 1);
}

#[test]
fn exact_representable_boundary_is_accepted_and_roundtrips() {
    let (mut s, _clock, _d) = store("boundary");
    let ttl = i64::MAX as u64 - NOW_MS;
    let (run, lease) = s.open_run(manifest(ttl), "writer-a").unwrap();
    assert_eq!(lease.expires_at_ms, i64::MAX as u64);
    let created = &s.events(&run).unwrap()[0];
    assert_eq!(created.class, "lifecycle.run.created");
    let recorded = created
        .payload
        .get("lease_ttl")
        .and_then(|t| t.get("writer"))
        .and_then(|w| w.as_int());
    assert_eq!(recorded, Some(ttl as i64), "declared TTL recorded verbatim");
}

#[test]
fn long_recorded_ttl_still_fences_and_renews() {
    // The W4 drive's recorded 1,800,000 ms value: an ordinary lease.
    let (mut s, clock, _d) = store("w4");
    let (run, lease) = s.open_run(manifest(1_800_000), "writer-a").unwrap();
    assert_eq!(lease.expires_at_ms, NOW_MS + 1_800_000);
    assert!(matches!(
        s.acquire_writer("writer-b", &run, 60_000),
        Err(LedgerError::WouldBlock { .. })
    ));
    // A long idle turn inside the lease, then renewal keeps the interval.
    clock.advance(1_700_000);
    let renewed = s.renew(&lease).unwrap();
    assert_eq!(renewed.generation, 1);
    assert_eq!(renewed.expires_at_ms, NOW_MS + 1_700_000 + 1_800_000);
    // Past expiry: takeover bumps the generation and fences the stale holder.
    clock.advance(1_900_000);
    let taken = s.acquire_writer("writer-b", &run, 60_000).unwrap();
    assert_eq!(taken.generation, 2);
    assert!(matches!(s.renew(&lease), Err(LedgerError::Fenced { .. })));
}

#[test]
fn takeover_with_unrepresentable_ttl_refuses_without_fencing() {
    let (mut s, clock, _d) = store("takeover");
    let (run, _lease) = s.open_run(manifest(60_000), "writer-a").unwrap();
    clock.advance(70_000);
    let r = s.acquire_writer("writer-b", &run, u64::MAX);
    assert!(
        matches!(r, Err(LedgerError::ManifestInvalid { .. })),
        "expected ManifestInvalid, got {r:?}"
    );
    // The refused takeover wrote nothing: a valid takeover still lands at generation 2.
    let ok = s.acquire_writer("writer-b", &run, 60_000).unwrap();
    assert_eq!(ok.generation, 2);
}

#[test]
fn renewal_near_the_representable_boundary_refuses_without_writing() {
    // A lease expiring exactly at i64::MAX: renewing one ms later would
    // need now + interval > i64::MAX — refused typed, never wrapped or
    // serialized negative, and the live lease is left untouched.
    let (mut s, clock, _d) = store("renew-edge");
    let ttl = i64::MAX as u64 - NOW_MS;
    let (run, lease) = s.open_run(manifest(ttl), "writer-a").unwrap();
    clock.advance(1);
    let r = s.renew(&lease);
    assert!(
        matches!(r, Err(LedgerError::ManifestInvalid { .. })),
        "expected ManifestInvalid, got {r:?}"
    );
    // Nothing was written: the original lease still blocks a takeover.
    assert!(matches!(
        s.acquire_writer("writer-b", &run, 60_000),
        Err(LedgerError::WouldBlock { .. })
    ));
}

#[test]
fn creation_records_one_clock_sample_for_acquisition_and_expiry() {
    let (mut s, _clock, _d) = store("one-sample");
    let (_run, lease) = s.open_run(manifest(1_800_000), "writer-a").unwrap();
    assert_eq!(lease.expires_at_ms - 1_800_000, NOW_MS);
}
