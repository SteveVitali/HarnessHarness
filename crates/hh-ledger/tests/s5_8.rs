//! S5.8 — `lifecycle.contract.deprecated_use` projection (§7.4 rule 5;
//! ADR-0178 D5). The fold is rebuildable (WAL rows are the record; the
//! `(method, client) → count` map is a derived view), per-run scoped and
//! store-wide aggregated, and it tolerates rows a non-kernel writer malforms
//! (a missing `method` member is skipped, never counted under a guessed key).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::{SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-s58-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_with(
        dir(tag),
        Box::new(hh_ledger::ids::ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

/// A kernel `lifecycle.contract.deprecated_use{method, client}` row — the
/// shape `EmbedService::note_deprecated_use` mints (kernel producer +
/// provenance; the `client` member is the negotiated `ClientDescriptor`
/// record, never a caller-supplied string).
fn dep_row(id: &str, method: &str, client: Option<Json>) -> Event {
    Event {
        event_id: id.to_string(),
        class: "lifecycle.contract.deprecated_use".to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload: Json::obj([
            ("method", Json::str(method)),
            ("client", client.unwrap_or(Json::Null)),
        ]),
    }
}

fn client_obj(name: &str, version: &str, kind: &str) -> Json {
    Json::obj([
        ("name", Json::str(name)),
        ("version", Json::str(version)),
        ("kind", Json::str(kind)),
    ])
}

#[test]
fn deprecated_use_counts_per_run_and_store() {
    let (mut s, run, lease) = open("dep");
    s.append(
        &run,
        &lease,
        vec![
            dep_row("e1", "env.resume", Some(client_obj("web", "2.1", "web"))),
            dep_row("e2", "env.resume", Some(client_obj("web", "2.1", "web"))),
            dep_row("e3", "read", None),
            dep_row("e4", "env.resume", Some(Json::str("legacy-cli"))),
            // Another class interleaved — ignored by the fold.
            Event {
                class: "lifecycle.run.created".to_string(),
                ..dep_row("e5", "whatever", None)
            },
        ],
    )
    .unwrap();
    // A row with no `method` member is skipped — never keyed on a guess.
    let mut no_method = dep_row("e6", "env.resume", None);
    if let Json::Obj(m) = &mut no_method.payload {
        m.remove("method");
    }
    s.append(&run, &lease, vec![no_method]).unwrap();

    let counts = s.deprecated_use_counts(&run).unwrap();
    assert_eq!(counts.len(), 3);
    assert_eq!(counts[&("env.resume".into(), "web@2.1:web".into())], 2);
    assert_eq!(counts[&("read".into(), "unknown".into())], 1);
    assert_eq!(counts[&("env.resume".into(), "legacy-cli".into())], 1);
    // Store-wide totals equal the single run's map here.
    assert_eq!(s.deprecated_use_totals(), counts);
}

#[test]
fn deprecated_use_totals_aggregate_across_runs() {
    let (mut s, run_a, lease_a) = open("dep2");
    let (run_b, lease_b) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-b")
        .unwrap();
    s.append(
        &run_a,
        &lease_a,
        vec![dep_row(
            "a1",
            "env.resume",
            Some(client_obj("web", "2.1", "web")),
        )],
    )
    .unwrap();
    s.append(
        &run_b,
        &lease_b,
        vec![dep_row(
            "b1",
            "env.resume",
            Some(client_obj("web", "2.1", "web")),
        )],
    )
    .unwrap();
    let totals = s.deprecated_use_totals();
    assert_eq!(totals[&("env.resume".into(), "web@2.1:web".into())], 2);
    // Per-run scoping holds.
    assert_eq!(s.deprecated_use_counts(&run_a).unwrap().len(), 1);
    assert_eq!(s.deprecated_use_counts(&run_b).unwrap().len(), 1);
}

#[test]
fn deprecated_use_counts_survive_rebuild() {
    // The projection is a derived view of durable WAL rows — a cold-open
    // rebuild reproduces the identical counts (CC1: one scheme; the WAL
    // is the record, the map is re-folded, never persisted).
    let d = dir("dep3");
    let run;
    {
        let mut s = Store::open_with(
            d.clone(),
            Box::new(hh_ledger::ids::ManualClock::at(1_000)),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (r, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        s.append(
            &r,
            &lease,
            vec![
                dep_row("x1", "env.resume", Some(client_obj("cli", "1.0", "cli"))),
                dep_row("x2", "env.resume", Some(client_obj("cli", "1.0", "cli"))),
            ],
        )
        .unwrap();
        run = r;
    }
    let reopened = Store::open_with(
        d,
        Box::new(hh_ledger::ids::ManualClock::at(2_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let counts = reopened.deprecated_use_counts(&run).unwrap();
    assert_eq!(counts[&("env.resume".into(), "cli@1.0:cli".into())], 2);
    assert_eq!(reopened.deprecated_use_totals().len(), 1);
}
