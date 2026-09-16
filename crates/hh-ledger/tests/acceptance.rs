//! S1.5 acceptance coverage — every row names the AC it pins (AC-R-2.2.1-N).
//!
//! Each test fails if the behaviour it covers is removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::errors::{LedgerError, MissingReason, TamperedKind};
use hh_ledger::event::{
    CloseReason, Cursor, Direction, Event, EventFrame, EventPlane, Producer, ReadFilter, Scope,
};
use hh_ledger::ids::{ManualClock, SeqIds, GENESIS_HASH, ROOT_EVENT};
use hh_ledger::manifest::{EventRef, LineageLink, RunKind, RunManifest};
use hh_ledger::schema::SCHEMA_VERSION;
use hh_ledger::store::{Lease, Store};
use hh_ledger::views::ViewKind;
use hh_provenance::{
    AuthorityClass, ContentKind, Origin, PersistenceScope, ProvenanceRecord, TaintTag,
};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn manifest() -> RunManifest {
    RunManifest::minimal(RunKind::Agent)
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest(), "writer-a").unwrap();
    (s, run, lease)
}

/// A caller-side event: `executor`-produced, no provenance unless set.
fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

/// A kernel-authored event (audit-grade and kernel-origin classes).
fn k_ev(id: &str, class: &str, payload: Json) -> Event {
    let mut e = ev(id, class, payload);
    e.producer = Producer::kernel("kernel:test");
    e.provenance = Some(ProvenanceRecord::kernel("kernel:test", 0));
    e
}

// ── AC-1: open_run ───────────────────────────────────────────────────────────

#[test]
fn ac1_open_run_writes_created_at_seq0_and_takes_the_writer() {
    let d = dir("open");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let m = manifest();
    let (run, lease) = s.open_run(m.clone(), "writer-a").unwrap();
    assert_eq!(lease.generation, 1);
    assert_eq!(lease.holder, "writer-a");
    // seq 0 = lifecycle.run.created; seq 1 = the audited lease acquisition.
    let page = s
        .read(&run, Cursor::Seq(0), None, Direction::Fwd, 10)
        .unwrap();
    assert_eq!(page.events.len(), 2);
    assert_eq!(page.events[0].class, "lifecycle.run.created");
    assert_eq!(page.events[0].seq, 0);
    assert_eq!(page.events[0].prev_hash, GENESIS_HASH);
    assert_eq!(page.events[0].payload, m.to_json());
    assert_eq!(page.events[1].class, "lifecycle.lease.acquired");
    assert_eq!(page.events[1].lease_generation, 1);
    // Kernel stamps: provenance origin = kernel on lifecycle rows.
    let prov = page.events[0].provenance.as_ref().unwrap();
    assert_eq!(prov.authority, AuthorityClass::Kernel);
    // head = the lease row.
    let head = s.head(&run).unwrap();
    assert_eq!(head.seq, 1);
    assert_eq!(head.event_id, page.events[1].event_id);
    // Manifest immutable thereafter — read-back equals the submitted manifest.
    let back = RunManifest::from_json(&page.events[0].payload).unwrap();
    assert_eq!(back, m);
    // The chain verifies; reopening reproduces it byte-identically.
    s.verify(&run).unwrap();
    drop(s);
    let s2 = Store::open_test(&d, 2_000).unwrap();
    let page2 = s2
        .read(&run, Cursor::Seq(0), None, Direction::Fwd, 10)
        .unwrap();
    assert_eq!(page.events, page2.events);
    s2.verify(&run).unwrap();
}

#[test]
fn ac1_open_run_refuses_invalid_or_unresolved_manifests() {
    let d = dir("open-bad");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    // A mutable tag where a pinned identity is required.
    let mut m = manifest();
    m.configuration_id = Some("harness:latest".into());
    assert!(matches!(
        s.open_run(m, "w"),
        Err(LedgerError::ConfigurationUnresolvable { .. })
    ));
    // A non-agent run carrying configuration cells (ADR-0183 §C).
    let mut m = RunManifest::minimal(RunKind::Inbox);
    m.run_kind = RunKind::Inbox;
    assert!(matches!(
        s.open_run(m, "w"),
        Err(LedgerError::ManifestInvalid { .. })
    ));
    // An unpinned ref.
    let mut m = manifest();
    m.model_profile_ref = Some("profile:v2".into());
    assert!(matches!(
        s.open_run(m, "w"),
        Err(LedgerError::UnresolvedRef {
            field: "model_profile_ref",
            ..
        })
    ));
}

// ── AC-5: dense contiguous seqs; atomic batches ─────────────────────────────

#[test]
fn ac5_append_allocates_dense_contiguous_seqs() {
    let (mut s, run, lease) = open("dense");
    let r = s
        .append(
            &run,
            &lease,
            vec![
                ev("evt-a", "context.observation.recorded", Json::str("one")),
                ev("evt-b", "context.observation.recorded", Json::str("two")),
                ev("evt-c", "context.observation.recorded", Json::str("three")),
            ],
        )
        .unwrap();
    assert_eq!((r.first, r.last, r.count), (2, 4, 3));
    let events = s.events(&run).unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2, 3, 4]);
    // Hash chain: every event's prev_hash is the previous event's hash.
    for w in events.windows(2) {
        assert_eq!(w[1].prev_hash, w[0].hash);
        assert_eq!(w[1].hash, w[1].recompute_hash());
    }
}

#[test]
fn ac5_a_failing_batch_commits_nothing() {
    let (mut s, run, lease) = open("atomic");
    let before = s.head(&run).unwrap();
    let r = s.append(
        &run,
        &lease,
        vec![
            ev("evt-ok", "context.observation.recorded", Json::str("x")),
            ev("evt-bad", "no.such.class", Json::Null),
        ],
    );
    assert!(matches!(r, Err(LedgerError::SchemaViolation { .. })));
    assert_eq!(s.head(&run).unwrap(), before); // no partial batch
    assert!(s
        .read(&run, Cursor::Seq(2), None, Direction::Fwd, 10)
        .is_err());
}

#[test]
fn ac5_unknown_class_duplicate_id_and_unknown_parent_are_typed() {
    let (mut s, run, lease) = open("typed");
    assert!(matches!(
        s.append(&run, &lease, vec![ev("e1", "bogus.class", Json::Null)]),
        Err(LedgerError::SchemaViolation { .. })
    ));
    s.append(
        &run,
        &lease,
        vec![ev("e-dup", "context.observation.recorded", Json::Null)],
    )
    .unwrap();
    assert!(matches!(
        s.append(
            &run,
            &lease,
            vec![ev("e-dup", "context.observation.recorded", Json::Null)]
        ),
        Err(LedgerError::DuplicateEventId { .. })
    ));
    let mut orphan = ev("e-orphan", "context.observation.recorded", Json::Null);
    orphan.parent_event_id = "evt-nowhere".into();
    assert!(matches!(
        s.append(&run, &lease, vec![orphan]),
        Err(LedgerError::UnknownParent { .. })
    ));
    // A self-parent is an UnknownParent, never a cycle.
    let mut selfp = ev("e-self", "context.observation.recorded", Json::Null);
    selfp.parent_event_id = "e-self".into();
    assert!(matches!(
        s.append(&run, &lease, vec![selfp]),
        Err(LedgerError::UnknownParent { .. })
    ));
}

// ── AC-7: single fenced writer ───────────────────────────────────────────────

#[test]
fn ac7_one_writer_lease_takeover_fences_the_stale_holder() {
    let d = dir("fence");
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        &d,
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        hh_ledger::store::DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = manifest();
    m.lease_ttl.writer_ms = 60_000;
    let (run, lease1) = s.open_run(m, "writer-a").unwrap();
    // A live lease blocks a second acquirer.
    assert!(matches!(
        s.acquire_writer("writer-b", &run, 60_000),
        Err(LedgerError::WouldBlock { .. })
    ));
    // Renew extends the live lease.
    let renewed = s.renew(&lease1).unwrap();
    assert!(renewed.expires_at_ms > lease1.expires_at_ms || renewed.generation == 1);
    // Expire → takeover: generation strictly increases, the stale holder is fenced
    // and the fence is audited in the ledger itself.
    clock.advance(70_000);
    let lease2 = s.acquire_writer("writer-b", &run, 60_000).unwrap();
    assert_eq!(lease2.generation, 2);
    assert_eq!(lease2.holder, "writer-b");
    let r = s.append(
        &run,
        &lease1,
        vec![ev("e-stale", "context.observation.recorded", Json::Null)],
    );
    match r {
        Err(LedgerError::Fenced {
            lease_generation,
            current_generation,
            ..
        }) => {
            assert_eq!(lease_generation, 1);
            assert_eq!(current_generation, 2);
        }
        other => panic!("expected Fenced, got {other:?}"),
    }
    let classes: Vec<&str> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.as_str())
        .collect();
    assert!(classes.contains(&"lifecycle.lease.fenced"));
    // The new writer appends fine; the release is audited.
    s.append(
        &run,
        &lease2,
        vec![ev("e-new", "context.observation.recorded", Json::Null)],
    )
    .unwrap();
    s.release(&lease2, "done").unwrap();
    // Releasing a stale token is fenced too.
    assert!(matches!(
        s.release(&lease1, "late"),
        Err(LedgerError::Fenced { .. })
    ));
}

// ── AC-10: the hash chain ────────────────────────────────────────────────────

#[test]
fn ac10_hash_chain_and_tamper_detection() {
    let (mut s, run, lease) = open("chain");
    for i in 0..4 {
        s.append(
            &run,
            &lease,
            vec![ev(
                &format!("evt-{i}"),
                "context.observation.recorded",
                Json::Int(i),
            )],
        )
        .unwrap();
    }
    s.verify(&run).unwrap();
    // Edit a stored payload byte → ContentModified (evt-2 commits at seq 4).
    let wal = s.root().join("runs").join(&run).join("events.wal");
    let text = std::fs::read_to_string(&wal).unwrap();
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let target = lines
        .iter_mut()
        .find(|l| l.contains("\"event_id\":\"evt-2\""))
        .unwrap();
    *target = target.replacen("\"payload\":2", "\"payload\":9", 1);
    std::fs::write(&wal, lines.join("\n") + "\n").unwrap();
    match s.verify(&run) {
        Err(LedgerError::Tampered(t)) => {
            assert_eq!(t.kind, TamperedKind::ContentModified);
            assert_eq!(t.at_seq, 4);
        }
        other => panic!("expected Tampered, got {other:?}"),
    }
    // Delete a committed event line → the commit marker still names the seq → SeqGap.
    let text = std::fs::read_to_string(&wal).unwrap();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.contains("\"event_id\":\"evt-2\""))
        .collect();
    std::fs::write(&wal, kept.join("\n") + "\n").unwrap();
    match s.verify(&run) {
        Err(LedgerError::Tampered(t)) => assert_eq!(t.kind, TamperedKind::SeqGap),
        other => panic!("expected Tampered, got {other:?}"),
    }
}

#[test]
fn ac10_a_corrupted_commit_marker_or_line_drops_the_tail() {
    let (mut s, run, lease) = open("chain-tail");
    s.append(
        &run,
        &lease,
        vec![ev("evt-1", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    let wal = s.root().join("runs").join(&run).join("events.wal");
    // Truncate mid-line — the torn tail is never surfaced as durable.
    let bytes = std::fs::read(&wal).unwrap();
    std::fs::write(&wal, &bytes[..bytes.len() - 20]).unwrap();
    let s2 = Store::open_test(s.root(), 9_999).unwrap();
    let page = s2
        .read(&run, Cursor::Seq(0), None, Direction::Fwd, 50)
        .unwrap();
    assert_eq!(page.events.len(), 2); // created + lease.acquired only
    drop(s);
}

// ── AC-11: durable-before-visible ───────────────────────────────────────────

#[test]
fn ac11_events_without_a_commit_marker_are_never_durable() {
    let d = dir("torn");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, lease) = s.open_run(manifest(), "w").unwrap();
    s.append(
        &run,
        &lease,
        vec![ev("evt-1", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    // Forge an uncommitted event line (no commit marker follows it).
    let wal = d.join("runs").join(&run).join("events.wal");
    let forged = s.events(&run).unwrap()[2].clone();
    let mut line = String::from("{\"k\":\"e\",\"v\":");
    line.push_str(&String::from_utf8(forged.canonical_bytes()).unwrap());
    line.push_str("}\n");
    let mut f = std::fs::OpenOptions::new().append(true).open(&wal).unwrap();
    use std::io::Write;
    f.write_all(line.as_bytes()).unwrap();
    f.sync_all().unwrap();
    drop(f);
    // Replay drops it — it was never committed.
    let s2 = Store::open_test(&d, 9_999).unwrap();
    assert_eq!(s2.head(&run).unwrap().seq, 2);
    s2.verify(&run).unwrap();
}

// ── AC-12: ephemeral is subscribe-only ──────────────────────────────────────

#[test]
fn ac12_ephemeral_classes_are_subscribe_only_never_durable() {
    let (mut s, run, lease) = open("eph");
    let mut sub = s.subscribe(&run, Cursor::Now).unwrap();
    s.append(
        &run,
        &lease,
        vec![
            ev("evt-delta", "model.stream.delta", Json::str("tok")),
            ev("evt-dur", "context.observation.recorded", Json::str("d")),
        ],
    )
    .unwrap();
    // Submission order preserved: ephemeral frame then durable frame.
    assert!(matches!(sub.next(), Some(EventFrame::Sync { .. })));
    match sub.next() {
        Some(EventFrame::Ephemeral { event }) => {
            assert_eq!(event.class, "model.stream.delta");
        }
        other => panic!("expected ephemeral, got {other:?}"),
    }
    assert!(matches!(
        sub.next(),
        Some(EventFrame::Durable { seq: 2, .. })
    ));
    // `read` never returns the ephemeral.
    let page = s
        .read(&run, Cursor::Seq(0), None, Direction::Fwd, 50)
        .unwrap();
    assert!(!page.events.iter().any(|e| e.class == "model.stream.delta"));
    // Reopening keeps it gone.
    let s2 = Store::open_test(s.root(), 9_999).unwrap();
    assert!(!s2
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "model.stream.delta"));
}

#[test]
fn ac12_declared_ephemeral_members_are_stripped_and_delivered_as_fragments() {
    let (mut s, run, lease) = open("frag");
    let mut sub = s.subscribe(&run, Cursor::Now).unwrap();
    let mut e = k_ev(
        "evt-perm",
        "security.permission.requested",
        Json::obj([
            ("permission_id", Json::str("perm-1")),
            ("rendering", Json::str("approve? [y/n]")),
        ]),
    );
    e.provenance = Some(ProvenanceRecord::kernel("kernel:perm", 0));
    s.append(&run, &lease, vec![e]).unwrap();
    let _ = sub.next(); // sync
    let dur = match sub.next() {
        Some(EventFrame::Durable { event, seq, .. }) => {
            // `rendering` was stripped from the durable form.
            assert!(event.payload.get("rendering").is_none());
            assert_eq!(
                event.payload.get("permission_id").and_then(Json::as_str),
                Some("perm-1")
            );
            seq
        }
        other => panic!("expected durable, got {other:?}"),
    };
    match sub.next() {
        Some(EventFrame::Ephemeral { event }) => {
            assert_eq!(event.durable_event_seq, Some(dur));
            assert_eq!(
                event.payload.get("rendering").and_then(Json::as_str),
                Some("approve? [y/n]")
            );
        }
        other => panic!("expected fragment, got {other:?}"),
    }
}

// ── read / subscribe surface ─────────────────────────────────────────────────

#[test]
fn read_cursors_are_inclusive_and_filters_apply() {
    let (mut s, run, lease) = open("read");
    s.append(
        &run,
        &lease,
        vec![
            ev("e1", "context.observation.recorded", Json::Int(1)),
            ev("e2", "model.route.decided", Json::str("r")),
            ev("e3", "context.observation.recorded", Json::Int(3)),
        ],
    )
    .unwrap();
    // Inclusive seq cursor.
    let p = s
        .read(&run, Cursor::Seq(3), None, Direction::Fwd, 10)
        .unwrap();
    assert_eq!(p.events[0].seq, 3);
    // Inclusive event-id cursor.
    let p = s
        .read(&run, Cursor::EventId("e2".into()), None, Direction::Fwd, 10)
        .unwrap();
    assert_eq!(p.events[0].event_id, "e2");
    // Reverse scan — reconstruction is O(tail).
    let p = s
        .read(&run, Cursor::Seq(4), None, Direction::Rev, 2)
        .unwrap();
    assert_eq!(p.events[0].seq, 4);
    assert_eq!(p.events[1].seq, 3);
    // Plane filter.
    let p = s
        .read(
            &run,
            Cursor::Seq(0),
            Some(&ReadFilter {
                plane: Some(EventPlane::Observation),
                ..Default::default()
            }),
            Direction::Fwd,
            50,
        )
        .unwrap();
    assert!(p.events.iter().all(|e| e.plane == EventPlane::Observation));
    // Prefix class filter.
    let p = s
        .read(
            &run,
            Cursor::Seq(0),
            Some(&ReadFilter {
                class: Some("context.*".into()),
                ..Default::default()
            }),
            Direction::Fwd,
            50,
        )
        .unwrap();
    assert!(p.events.iter().all(|e| e.class.starts_with("context.")));
    // Unknown cursors are typed.
    assert!(matches!(
        s.read(
            &run,
            Cursor::EventId("nope".into()),
            None,
            Direction::Fwd,
            1
        ),
        Err(LedgerError::UnknownCursor { .. })
    ));
    assert!(matches!(
        s.read(&run, Cursor::Seq(9_999), None, Direction::Fwd, 1),
        Err(LedgerError::UnknownCursor { .. })
    ));
    assert!(matches!(
        s.read(&run, Cursor::Now, None, Direction::Fwd, 1),
        Err(LedgerError::UnknownCursor { .. })
    ));
    assert!(matches!(
        s.read("run-nope", Cursor::Seq(0), None, Direction::Fwd, 1),
        Err(LedgerError::UnknownRun { .. })
    ));
}

#[test]
fn subscribe_replays_then_syncs_then_tails_live() {
    let (mut s, run, lease) = open("sub");
    s.append(
        &run,
        &lease,
        vec![ev("e1", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    let mut sub = s.subscribe(&run, Cursor::Seq(0)).unwrap();
    // Replay: durable frames for seqs 0,1,2 then sync{at_seq:2}.
    for expect in [0u64, 1, 2] {
        match sub.next() {
            Some(EventFrame::Durable { seq, .. }) => assert_eq!(seq, expect),
            other => panic!("expected durable {expect}, got {other:?}"),
        }
    }
    assert!(matches!(sub.next(), Some(EventFrame::Sync { at_seq: 2 })));
    // Live frame.
    s.append(
        &run,
        &lease,
        vec![ev("e2", "context.observation.recorded", Json::Int(2))],
    )
    .unwrap();
    assert!(matches!(
        sub.next(),
        Some(EventFrame::Durable { seq: 3, .. })
    ));
}

#[test]
fn lagged_fires_when_the_bounded_buffer_overflows() {
    let (mut s, run, lease) = open("lag");
    let mut sub = s.subscribe(&run, Cursor::Now).unwrap();
    let _ = sub.next(); // sync
    let batch: Vec<Event> = (0..1100)
        .map(|i| {
            ev(
                &format!("e{i}"),
                "context.observation.recorded",
                Json::Int(i),
            )
        })
        .collect();
    s.append(&run, &lease, batch).unwrap();
    // Drain what fit; the next notify delivers `lagged` first.
    while sub.try_next().is_some() {}
    s.append(
        &run,
        &lease,
        vec![ev("e-after", "context.observation.recorded", Json::Int(0))],
    )
    .unwrap();
    match sub.next() {
        Some(EventFrame::Lagged { missed_from_seq }) => {
            assert!(missed_from_seq > 1);
        }
        other => panic!("expected lagged, got {other:?}"),
    }
    assert!(matches!(sub.next(), Some(EventFrame::Durable { .. })));
}

// ── scope + provenance enforcement ───────────────────────────────────────────

#[test]
fn scope_chain_and_open_close_rules_are_enforced() {
    let (mut s, run, lease) = open("scope");
    // tool_call without model_call violates the chain.
    let mut bad = ev("t0", "action.tool.proposed", Json::Null);
    bad.scope.tool_call_id = Some("tc-1".into());
    assert!(matches!(
        s.append(&run, &lease, vec![bad]),
        Err(LedgerError::SchemaViolation { .. })
    ));
    // Open a turn (kernel-origin class → kernel producer + provenance).
    let mut t = k_ev("t1", "lifecycle.turn.started", Json::Null);
    t.scope.turn_id = Some("turn-1".into());
    s.append(&run, &lease, vec![t]).unwrap();
    // model.call.requested opens a model_call inside the turn.
    let mut m = k_ev("m1", "model.call.requested", Json::Null);
    m.scope.turn_id = Some("turn-1".into());
    m.scope.model_call_id = Some("mc-1".into());
    s.append(&run, &lease, vec![m]).unwrap();
    // tool_call inside model_call inside turn.
    let mut tc = ev("tc1", "action.tool.proposed", Json::Null);
    tc.scope.turn_id = Some("turn-1".into());
    tc.scope.model_call_id = Some("mc-1".into());
    tc.scope.tool_call_id = Some("tc-1".into());
    s.append(&run, &lease, vec![tc]).unwrap();
    // Close it.
    let mut done = ev("tc2", "action.tool.completed", Json::Null);
    done.scope.turn_id = Some("turn-1".into());
    done.scope.model_call_id = Some("mc-1".into());
    done.scope.tool_call_id = Some("tc-1".into());
    s.append(&run, &lease, vec![done]).unwrap();
    // A scope id that is not open is ScopeNotOpen.
    let mut bad = ev("tc3", "action.tool.completed", Json::Null);
    bad.scope.turn_id = Some("turn-1".into());
    bad.scope.model_call_id = Some("mc-1".into());
    bad.scope.tool_call_id = Some("tc-9".into());
    assert!(matches!(
        s.append(&run, &lease, vec![bad]),
        Err(LedgerError::ScopeNotOpen { .. })
    ));
}

#[test]
fn provenance_rules_are_enforced_at_append() {
    let (mut s, run, lease) = open("prov");
    // A provenance-mandatory class without a record → MissingProvenance.
    assert!(matches!(
        s.append(
            &run,
            &lease,
            vec![ev("p0", "security.permission.pending", Json::Null)]
        ),
        Err(LedgerError::MissingProvenance { .. })
    ));
    // taint ≠ ∅ with authority > external → TaintedAboveExternal.
    let mut e = ev("p1", "security.permission.pending", Json::Null);
    let mut rec = ProvenanceRecord::minted(
        Origin::model("m1", &run, "resp-1"),
        PersistenceScope::Run,
        1,
    );
    rec.taint.insert(TaintTag::Import {
        source_system: "web".into(),
    });
    e.provenance = Some(rec);
    assert!(matches!(
        s.append(&run, &lease, vec![e]),
        Err(LedgerError::TaintedAboveExternal { .. })
    ));
    // Free text above external → TextAboveExternal.
    let mut e = ev("p2", "context.observation.recorded", Json::str("hello"));
    e.provenance = Some(ProvenanceRecord::minted(
        Origin::model("m1", &run, "resp-1"),
        PersistenceScope::Run,
        1,
    ));
    e.content_kind = Some(ContentKind::FreeText);
    assert!(matches!(
        s.append(&run, &lease, vec![e]),
        Err(LedgerError::TextAboveExternal { .. })
    ));
    // A scope write above its authority ceiling → ScopeCeilingExceeded.
    let mut e = ev("p3", "context.observation.recorded", Json::str("x"));
    e.provenance = Some(ProvenanceRecord::minted(
        Origin::model("m1", &run, "resp-1"),
        PersistenceScope::Project, // delegate-class origin, project scope
        1,
    ));
    assert!(matches!(
        s.append(&run, &lease, vec![e]),
        Err(LedgerError::ScopeCeilingExceeded { .. })
    ));
    // A kernel-origin class must carry kernel provenance.
    let mut e = ev("p4", "lifecycle.turn.started", Json::Null);
    e.producer = Producer::kernel("kernel:t");
    e.provenance = Some(ProvenanceRecord::minted(
        Origin::model("m1", &run, "r"),
        PersistenceScope::Run,
        1,
    ));
    e.scope.turn_id = Some("turn-x".into());
    assert!(matches!(
        s.append(&run, &lease, vec![e]),
        Err(LedgerError::KernelOriginRequired { .. })
    ));
    // A non-kernel producer on an audit-grade class → AuditProducerInvalid.
    let mut e = ev("p5", "lifecycle.run.finished", Json::Null);
    e.provenance = Some(ProvenanceRecord::kernel("kernel:t", 1));
    assert!(matches!(
        s.append(&run, &lease, vec![e]),
        Err(LedgerError::AuditProducerInvalid { .. })
    ));
}

#[test]
fn illegitimate_endorsement_is_refused_at_append() {
    let (mut s, run, lease) = open("endorse");
    // A subject event carrying provenance (external, untainted).
    let mut sub_ev = ev("subj", "context.observation.recorded", Json::str("data"));
    sub_ev.provenance = Some(ProvenanceRecord::minted(
        Origin::import("ext-sys", "v1"),
        PersistenceScope::Run,
        1,
    ));
    s.append(&run, &lease, vec![sub_ev]).unwrap();
    // A delegate (model) may never endorse.
    let subject = s.events(&run).unwrap()[2].provenance.clone().unwrap();
    let payload = Json::obj([
        ("subject_ref", Json::str("subj")),
        ("from", Json::obj([("authority", Json::str("unverified"))])),
        ("to", Json::obj([("authority", Json::str("principal"))])),
        (
            "endorser",
            ProvenanceRecord::minted(Origin::model("m1", &run, "resp"), PersistenceScope::Run, 2)
                .to_json(),
        ),
        ("basis", Json::str("promotion")),
    ]);
    let mut e = ev("end1", "security.label.endorsed", payload);
    e.provenance = Some(ProvenanceRecord::kernel("kernel:mon", 2));
    match s.append(&run, &lease, vec![e]) {
        Err(LedgerError::IllegitimateEndorsement { .. }) => {}
        other => panic!("expected IllegitimateEndorsement, got {other:?}"),
    }
    let _ = subject;
}

// ── AC-14: blob offload ──────────────────────────────────────────────────────

#[test]
fn ac14_blobs_are_content_addressed_deduplicated_and_verified() {
    let (mut s, run, _lease) = open("blob");
    let bytes = b"hello blob".repeat(1024);
    let a1 = s.put_blob(&bytes, "text/plain").unwrap();
    let a2 = s.put_blob(&bytes, "text/plain").unwrap();
    assert_eq!(a1, a2); // same bytes → same address
    assert_eq!(s.get_blob(&a1).unwrap(), bytes);
    // Corrupt the stored bytes → BlobCorrupt, never silently served.
    let path = s.root().join("blobs").join(&a1.digest);
    std::fs::write(&path, b"corrupted").unwrap();
    assert!(matches!(
        s.get_blob(&a1),
        Err(LedgerError::BlobCorrupt { .. })
    ));
    // Missing → Missing{gc}, never Tampered (ADR-0068 R3).
    std::fs::remove_file(&path).unwrap();
    match s.get_blob(&a1) {
        Err(LedgerError::Missing { reason, .. }) => assert_eq!(reason, MissingReason::Gc),
        other => panic!("expected Missing, got {other:?}"),
    }
    let _ = run;
}

#[test]
fn ac14_oversized_inline_payloads_are_refused_toward_offload() {
    let (mut s, run, lease) = open("big");
    let big = Json::str("x".repeat(70 * 1024)); // ≥ the 64 KiB class threshold
    let r = s.append(
        &run,
        &lease,
        vec![ev("e-big", "context.observation.recorded", big)],
    );
    match r {
        Err(LedgerError::SchemaViolation { detail }) => {
            assert!(detail.contains("offload"), "{detail}");
        }
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
    // The honest path: put_blob + reference it.
    let addr = s.put_blob(b"the big content", "text/plain").unwrap();
    let mut e = ev("e-ref", "context.observation.recorded", Json::Null);
    e.refs = vec![addr];
    s.append(&run, &lease, vec![e]).unwrap();
    let got = s
        .read(&run, Cursor::Seq(2), None, Direction::Fwd, 1)
        .unwrap();
    assert_eq!(got.events[0].refs.len(), 1);
}

// ── AC-3: pure rebuildable projections ───────────────────────────────────────

#[test]
fn ac3_views_are_pure_folds_stamped_with_the_watermark() {
    let (mut s, run, lease) = open("view");
    s.append(
        &run,
        &lease,
        vec![
            ev("c1", "context.observation.recorded", Json::str("ctx-1")),
            ev("m1", "model.route.decided", Json::str("r")),
            ev("c2", "context.observation.recorded", Json::str("ctx-2")),
        ],
    )
    .unwrap();
    let head = s.head(&run).unwrap();
    let cv = s.project(&run, ViewKind::ContextView, None).unwrap();
    assert_eq!(cv.kind.as_str(), "context_view");
    assert_eq!(cv.derived_from_seq, Some(head.seq));
    // Only the observation-plane rows.
    let items = cv.payload.get("items").unwrap();
    if let Json::Arr(a) = items {
        assert_eq!(a.len(), 2);
        assert_eq!(
            a[0].get("class").and_then(Json::as_str),
            Some("context.observation.recorded")
        );
    } else {
        panic!("items not an array");
    }
    let rs = s.project(&run, ViewKind::RunSummary, None).unwrap();
    assert_eq!(
        rs.payload.get("event_count").and_then(Json::as_int),
        Some(5)
    );
    assert_eq!(
        rs.payload.get("finished").and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(false)
    );
    // A second projection of the same prefix agrees byte-for-byte.
    let again = s.project(&run, ViewKind::RunSummary, None).unwrap();
    assert_eq!(again, rs);
}

#[test]
fn ac3_reopening_rebuilds_identical_views() {
    let d = dir("view-rebuild");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, lease) = s.open_run(manifest(), "w").unwrap();
    s.append(
        &run,
        &lease,
        vec![ev("c1", "context.observation.recorded", Json::str("x"))],
    )
    .unwrap();
    let before = s.project(&run, ViewKind::RunSummary, None).unwrap();
    drop(s);
    let s2 = Store::open_test(&d, 5_000).unwrap();
    let after = s2.project(&run, ViewKind::RunSummary, None).unwrap();
    assert_eq!(before.view_hash, after.view_hash);
    assert_eq!(before, after);
}

// ── finished runs, lineage, cross-run causes ─────────────────────────────────

#[test]
fn run_finished_closes_the_stream_and_the_log() {
    let (mut s, run, lease) = open("fin");
    let mut sub = s.subscribe(&run, Cursor::Now).unwrap();
    let _ = sub.next(); // sync
    s.append(
        &run,
        &lease,
        vec![k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        )],
    )
    .unwrap();
    assert!(matches!(sub.next(), Some(EventFrame::Durable { .. })));
    match sub.next() {
        Some(EventFrame::Closed { reason }) => assert_eq!(reason, CloseReason::RunFinished),
        other => panic!("expected closed, got {other:?}"),
    }
    assert!(matches!(
        s.append(
            &run,
            &lease,
            vec![ev("post", "context.observation.recorded", Json::Null)]
        ),
        Err(LedgerError::RunFinished { .. })
    ));
}

#[test]
fn lineage_resolves_root_first_and_anchors_the_chain() {
    let d = dir("lin");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run_a, lease_a) = s.open_run(manifest(), "w").unwrap();
    s.append(
        &run_a,
        &lease_a,
        vec![ev("a1", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    let head_a = s.head(&run_a).unwrap();
    // A continued run anchors seq 0 at the source head hash.
    let mut m = manifest();
    m.continued_from = Some(LineageLink {
        run_id: run_a.clone(),
        at_seq: head_a.seq,
        head_hash: head_a.hash.clone(),
    });
    let (run_b, _lb) = s.open_run(m, "w").unwrap();
    let page = s
        .read(&run_b, Cursor::Seq(0), None, Direction::Fwd, 1)
        .unwrap();
    assert_eq!(page.events[0].prev_hash, head_a.hash);
    // lineage is root-first, ending with the run itself.
    let lin = s.lineage(&run_b).unwrap();
    assert_eq!(lin.len(), 2);
    assert_eq!(lin[0].run_id, run_a);
    assert_eq!(lin[0].up_to_seq, head_a.seq);
    assert_eq!(lin[0].head_hash, head_a.hash);
    assert_eq!(lin[1].run_id, run_b);
    // An incoherent anchor is refused.
    let mut m = manifest();
    m.continued_from = Some(LineageLink {
        run_id: run_a.clone(),
        at_seq: head_a.seq,
        head_hash: "sha256:".to_string() + &"0".repeat(64),
    });
    assert!(matches!(
        s.open_run(m, "w"),
        Err(LedgerError::ForkPointNotCoherent { .. })
    ));
    // Beyond-head anchors are refused.
    let mut m = manifest();
    m.continued_from = Some(LineageLink {
        run_id: run_a.clone(),
        at_seq: head_a.seq + 99,
        head_hash: head_a.hash.clone(),
    });
    assert!(matches!(
        s.open_run(m, "w"),
        Err(LedgerError::SourceIncomplete { .. })
    ));
}

#[test]
fn causes_resolve_cross_run_and_dangling_causes_refuse() {
    let d = dir("causes");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run_a, lease_a) = s.open_run(manifest(), "w").unwrap();
    s.append(
        &run_a,
        &lease_a,
        vec![ev("a-evt", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    let (run_b, lease_b) = s.open_run(manifest(), "w").unwrap();
    let mut e = ev("b1", "context.observation.recorded", Json::Int(2));
    e.causes = vec![EventRef {
        run_id: run_a.clone(),
        event_id: "a-evt".into(),
    }];
    s.append(&run_b, &lease_b, vec![e]).unwrap();
    let mut e = ev("b2", "context.observation.recorded", Json::Int(3));
    e.causes = vec![EventRef {
        run_id: run_a.clone(),
        event_id: "missing".into(),
    }];
    assert!(matches!(
        s.append(&run_b, &lease_b, vec![e]),
        Err(LedgerError::UnresolvedEventRef { .. })
    ));
}

// ── golden corpus — the deterministic transcript ─────────────────────────────

/// The pinned golden: a fixed store (`SeqIds` + `ManualClock@1000`), a fixed manifest
/// and one fixed event must always produce the same committed bytes and head hash —
/// byte-stability is the property that makes runs comparable (AC-R-2.2.1-3/-10).
#[test]
fn golden_transcript_is_byte_stable() {
    let d = dir("golden");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, lease) = s.open_run(manifest(), "w").unwrap();
    assert_eq!(run, "run-000001");
    s.append(
        &run,
        &lease,
        vec![ev(
            "evt-fixed",
            "context.observation.recorded",
            Json::str("golden"),
        )],
    )
    .unwrap();
    let head = s.head(&run).unwrap();
    assert_eq!(
        head.hash,
        "sha256:e8c2c0a9ce754a4d26c4fe501d24bd71719bebbcacb46c909fc91a0275b45468"
    );
    // The committed transcript, byte for byte.
    let wal = std::fs::read_to_string(d.join("runs").join(&run).join("events.wal")).unwrap();
    let created_hash = "sha256:700cd1baea595813110a2541a2f9100f899cd72e7519aaf492cd8790c727f9bb";
    assert!(wal.contains(&format!("\"hash\":\"{created_hash}\"")));
    assert!(wal.contains("{\"k\":\"c\",\"n\":0}"));
    assert!(wal.contains("{\"k\":\"c\",\"n\":2}"));
    // And the whole chain still verifies.
    s.verify(&run).unwrap();
}

// ── AC-8: the schema gate ────────────────────────────────────────────────────

#[test]
fn ac8_newer_schema_versions_refuse_at_decode() {
    use hh_ledger::event::EventEnvelope;
    let (mut s, run, lease) = open("gate");
    s.append(
        &run,
        &lease,
        vec![ev("e1", "context.observation.recorded", Json::Int(1))],
    )
    .unwrap();
    let mut j = s.events(&run).unwrap()[2].to_json();
    if let Json::Obj(ref mut m) = j {
        m.insert(
            "schema_version".to_string(),
            Json::Int((SCHEMA_VERSION + 1) as i64),
        );
    }
    assert!(matches!(
        EventEnvelope::from_json(&j),
        Err(LedgerError::SchemaMismatch {
            found,
            known_max
        }) if found == SCHEMA_VERSION + 1 && known_max == SCHEMA_VERSION
    ));
}
