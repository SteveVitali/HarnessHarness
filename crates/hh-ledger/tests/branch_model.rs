//! S2.9 integration tests — the §5a.1 §5 branch model (R-2.2.4⁰ᵃ; ADR-0271):
//! `fork`, `navigate`, `rollback`, the `forked`/`rolled_back`/`head.moved`
//! audit rows, the source-prefix pin, the `branch_tree` projection, the
//! subscription `Rewind` frame, and rebuild parity.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.4-4  — `fork` binds `forked_from{run_id, at_seq, head_hash}` to
//!   the source's hash at the (possibly coerced) cut; the child's seq-1 row is
//!   `lifecycle.run.forked{BranchRecord}`; `branch_tree` folds the edge.
//! - AC-R-2.2.4-5  — an incoherent cut is `ForkPointNotCoherent`;
//!   `coerce_to_boundary` coerces to `nearest_coherent_seq` and the record
//!   says so.
//! - AC-R-2.2.4-6  — `navigate` moves the *logical* head only: the WAL stays
//!   dense (`seq`/`prev_hash` never rewrite), `read` still serves the whole
//!   prefix, and subscribers receive `EventFrame::Rewind`.
//! - AC-R-2.2.4-7  — `rollback` compensates the applied-after-`at` compensable
//!   set in reverse commit order, names the uncompensable, lands
//!   `rolled_back` + `head.moved`, and the rewind note blob is pinned by the
//!   row's `record_ref`.
//! - AC-R-2.2.4-8  — the source-prefix pin: content `refs`ed inside `0..=at`
//!   is `Pinned{referenced_by_run}` against another run's `gc`/`redact`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::branch::{BranchKind, EnvBinding, ForkOpts, NavigateTarget, ReplayMode};
use hh_ledger::effect::EffectPhase;
use hh_ledger::event::{Cursor, Direction, Event, EventFrame, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::saga::CompensationIntent;
use hh_ledger::store::{Lease, RedactTarget, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::LedgerError;
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-branch-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
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

fn risk(rev: &str, rs: &str, scope: &str) -> Json {
    Json::obj([
        ("reversibility", Json::str(rev)),
        ("repeat_safety", Json::str(rs)),
        ("scope", Json::str(scope)),
    ])
}

fn compensable() -> Json {
    risk("compensable", "idempotent", "workspace_local")
}

fn irreversible() -> Json {
    risk("irreversible", "non_idempotent", "external")
}

fn eff(id: &str, class: &str, effect_id: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: if class.starts_with("action.effect.") {
                Some(effect_id.to_string())
            } else {
                None
            },
            ..Scope::default()
        },
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

fn intended(id: &str, effect_id: &str, rc: Json) -> Event {
    eff(
        id,
        "action.effect.intended",
        effect_id,
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("effective_risk_class", rc),
        ]),
    )
}

fn authorized(id: &str, effect_id: &str, rc: Json) -> Event {
    eff(
        id,
        "action.effect.authorized",
        effect_id,
        Json::obj([("effective_risk_class", rc)]),
    )
}

fn decided(id: &str, effect_id: &str, attempt: i64, decision: &str) -> Event {
    eff(
        id,
        "security.permission.decided",
        effect_id,
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("attempt_no", Json::Int(attempt)),
            ("decision", Json::str(decision)),
        ]),
    )
}

fn committed(id: &str, effect_id: &str, attempt: i64, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.committed",
        effect_id,
        Json::obj([
            ("attempt_no", Json::Int(attempt)),
            ("fencing_token", Json::Int(fencing as i64)),
        ]),
    )
}

fn observed(id: &str, effect_id: &str, attempt: i64, outcome: &str, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.observed",
        effect_id,
        Json::obj([
            ("attempt_no", Json::Int(attempt)),
            ("outcome", Json::str(outcome)),
            ("fencing_token", Json::Int(fencing as i64)),
        ]),
    )
}

fn to_applied(s: &mut Store, run: &str, lease: &Lease, eid: &str, rc: Json) {
    let is_comp = rc.get("reversibility").and_then(Json::as_str) == Some("compensable");
    let mut prep = Json::obj([("idempotency_key", Json::str(format!("key-{eid}")))]);
    if is_comp {
        if let Json::Obj(m) = &mut prep {
            m.insert(
                "compensation_plan_id".into(),
                Json::str(format!("plan-{eid}")),
            );
        }
    }
    s.append(
        run,
        lease,
        vec![
            intended(&format!("{eid}-i"), eid, rc.clone()),
            authorized(&format!("{eid}-a"), eid, rc),
            decided(&format!("{eid}-d"), eid, 1, "allow"),
            eff(&format!("{eid}-p"), "action.effect.prepared", eid, prep),
            committed(&format!("{eid}-c"), eid, 1, lease.generation),
            observed(&format!("{eid}-o"), eid, 1, "applied", lease.generation),
        ],
    )
    .unwrap();
}

fn events<'a>(s: &'a Store, run: &str) -> &'a [hh_ledger::event::EventEnvelope] {
    s.envelopes(run).unwrap()
}

fn child_manifest() -> RunManifest {
    RunManifest::minimal(RunKind::Agent)
}

// ── AC-R-2.2.4-4 — fork: lineage, branch row, tree ─────────────────────────

#[test]
fn ac_2_2_4_4_fork_binds_the_lineage_anchor_and_mints_the_branch_row() {
    let (mut s, run, lease) = open("fork-basic");
    to_applied(&mut s, &run, &lease, "eff-1", compensable());
    let head = s.head(&run).unwrap();
    let (child, _clease, rec) = s
        .fork(
            &run,
            head.seq,
            BranchKind::Branch,
            &ForkOpts::default(),
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert_ne!(child, run);
    // The lineage anchor binds the source's hash at the cut — `open_run`
    // re-checked it (`check_lineage_link` is the manifest validator's share).
    let link = s
        .manifest(&child)
        .unwrap()
        .forked_from
        .clone()
        .expect("forked_from");
    assert_eq!(link.run_id, run);
    assert_eq!(link.at_seq, head.seq);
    assert_eq!(link.head_hash, head.hash);
    // The child's durable prefix: `created` at seq 0, then the writer-lease
    // row, then `run.forked` (all appended — order fixed by `fork`).
    let cev = events(&s, &child);
    assert_eq!(cev[0].class, "lifecycle.run.created");
    let forked = cev
        .iter()
        .find(|e| e.class == "lifecycle.run.forked")
        .expect("run.forked row");
    assert_eq!(forked.event_id, rec.forked_event_id);
    let p = &forked.payload;
    assert_eq!(
        p.get("source_run_id").and_then(Json::as_str),
        Some(run.as_str())
    );
    assert_eq!(
        p.get("at_seq").and_then(Json::as_int),
        Some(head.seq as i64)
    );
    assert_eq!(
        p.get("env").and_then(Json::as_str),
        Some("none"),
        "an absent env member records the no-env binding"
    );
    // AC-R-2.2.1-9 — fork immutability: the source's head hash at the cut is
    // unchanged and nothing was appended to the source.
    assert_eq!(s.head(&run).unwrap().hash, head.hash);
    assert_eq!(events(&s, &run).last().unwrap().seq, head.seq);
    // The derived index folds the edge (rebuildable — never authoritative).
    let tree = s.branch_tree();
    let parent = tree.iter().find(|i| i.run_id == run).unwrap();
    assert!(parent.children.contains(&child));
    let child_info = tree.iter().find(|i| i.run_id == child).unwrap();
    let r = child_info.record.as_ref().unwrap();
    assert_eq!(r.source_run_id, run);
    assert_eq!(r.at_seq, head.seq);
}

#[test]
fn ac_2_2_4_5_incoherent_cut_refuses_and_coerce_lands_at_the_boundary() {
    let (mut s, run, lease) = open("fork-coerce");
    to_applied(&mut s, &run, &lease, "eff-1", compensable());
    let coherent_head = s.head(&run).unwrap().seq;
    // An open `turn ⊃ model_call` scope makes every later cut incoherent.
    s.append(
        &run,
        &lease,
        vec![
            {
                let mut t = eff("t1", "lifecycle.turn.started", "x", Json::obj([]));
                t.scope.turn_id = Some("turn-1".into());
                t
            },
            {
                let mut c = eff("mc1", "model.call.requested", "x", Json::obj([]));
                c.scope = Scope {
                    turn_id: Some("turn-1".into()),
                    model_call_id: Some("mc-1".into()),
                    ..Scope::default()
                };
                c
            },
        ],
    )
    .unwrap();
    let bad = s.head(&run).unwrap().seq;
    match s.fork(
        &run,
        bad,
        BranchKind::Branch,
        &ForkOpts::default(),
        child_manifest(),
        "writer-a",
    ) {
        Err(LedgerError::ForkPointNotCoherent { .. }) => {}
        other => panic!("expected ForkPointNotCoherent, got {other:?}"),
    }
    // `coerce_to_boundary` coerces to the nearest earlier coherent seq —
    // and the record names the coercion (never a silent retarget).
    let opts = ForkOpts {
        coerce_to_boundary: true,
        ..ForkOpts::default()
    };
    let want = s.nearest_coherent_seq(&run, bad).unwrap().unwrap();
    let (_child, _l, rec) = s
        .fork(
            &run,
            bad,
            BranchKind::Branch,
            &opts,
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert_eq!(rec.at_seq, want);
    assert!(rec.at_seq < bad && rec.at_seq >= coherent_head);
    s.check_fork_point(&run, rec.at_seq).unwrap();
    assert!(rec.coerce_to_boundary, "the coercion must be recorded");
}

// ── AC-R-2.2.4-6 — navigate: logical head, dense WAL, Rewind ───────────────

#[test]
fn ac_2_2_4_6_navigate_moves_the_logical_head_and_leaves_the_wal_dense() {
    let (mut s, run, lease) = open("nav");
    to_applied(&mut s, &run, &lease, "eff-1", compensable());
    let _tip = s.head(&run).unwrap().seq;
    let wal_len = events(&s, &run).len();
    let mut sub = s.subscribe(&run, Cursor::Now).unwrap();
    // Rewind the logical head to seq 0 — `created` stays the head.
    let moved = s
        .navigate(&run, &lease, NavigateTarget::Seq(0), "test rewind")
        .unwrap();
    assert_eq!(moved.class, "lifecycle.head.moved");
    assert_eq!(s.head(&run).unwrap().seq, 0);
    // The WAL grew by exactly the head.moved row — seqs dense, hashes chained.
    let evs = events(&s, &run);
    assert_eq!(evs.len(), wal_len + 1);
    for (i, e) in evs.iter().enumerate() {
        assert_eq!(e.seq as usize, i, "seq gap at {i}");
        let want_prev = if i == 0 {
            // seq 0 anchors the lineage link (or genesis).
            evs[0].prev_hash.clone()
        } else {
            evs[i - 1].hash.clone()
        };
        assert_eq!(e.prev_hash, want_prev, "hash chain broken at seq {i}");
    }
    // The last durable row is the head.moved row itself.
    assert_eq!(evs.last().unwrap().event_id, moved.event_id);
    // `read` still serves the whole prefix — navigation hides nothing.
    let page = s
        .read(&run, Cursor::Seq(0), None, Direction::Fwd, 10_000)
        .unwrap();
    assert_eq!(page.events.len(), wal_len + 1);
    // The subscriber received the Rewind frame.
    let mut saw_rewind = false;
    while let Some(f) = sub.try_next() {
        if let EventFrame::Rewind { to_seq, .. } = f {
            assert_eq!(to_seq, 0);
            saw_rewind = true;
        }
    }
    assert!(saw_rewind, "no Rewind frame delivered");
    // The next append continues the WAL linearly from the *tip*, not the
    // logical head — `seq`/`prev_hash` never rewrite (append-only).

    s.append(&run, &lease, vec![kernel_note("post-nav")])
        .unwrap();
    let evs = events(&s, &run);
    assert_eq!(evs.last().unwrap().seq as usize, wal_len + 1);
    assert_eq!(
        evs.last().unwrap().prev_hash,
        moved.hash,
        "append must chain the WAL tip, not the logical head"
    );
    // And the logical head advances to the new row.
    assert_eq!(s.head(&run).unwrap().event_id, evs.last().unwrap().event_id);
}

#[test]
fn ac_2_2_4_6_navigate_to_root_then_rebuild_keeps_the_fold() {
    let d = dir("nav-root");
    let run;
    {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (r, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        run = r;
        to_applied(&mut s, &run, &lease, "eff-1", compensable());
        s.navigate(&run, &lease, NavigateTarget::Root, "back to genesis")
            .unwrap();
        // Head is the root sentinel — `head` is absent, not forged.
        assert!(s.head(&run).is_err());
    }
    // Rebuild parity: reopening the store replays the same fold — the
    // logical head is a projection of the WAL, not a side store.
    let s2 = reopen(&d, 60_000);
    assert!(
        s2.head(&run).is_err(),
        "rebuilt head must stay at the root sentinel"
    );
    let evs = events(&s2, &run);
    assert!(evs.iter().any(|e| e.class == "lifecycle.head.moved"
        && e.payload.get("to_event_id").and_then(Json::as_str) == Some(ROOT_EVENT)));
}

fn kernel_note(tag: &str) -> Event {
    eff(
        tag,
        "lifecycle.hosted.native_record",
        "x",
        Json::obj([("note", Json::str(tag))]),
    )
}

// ── AC-R-2.2.4-7 — rollback: compensation list, rewind note, head move ──────

#[test]
fn ac_2_2_4_7_rollback_compensates_in_reverse_and_moves_the_head() {
    let (mut s, run, lease) = open("rollback");
    to_applied(&mut s, &run, &lease, "eff-a", compensable());
    let cut = s.head(&run).unwrap().seq;
    to_applied(&mut s, &run, &lease, "eff-b", compensable());
    to_applied(&mut s, &run, &lease, "eff-c", compensable());
    let wal_before = events(&s, &run).len();
    let mut dispatched: Vec<String> = Vec::new();
    let rec = s
        .rollback(
            &run,
            &lease,
            cut,
            "rollback",
            vec![],
            &mut |i: &CompensationIntent| {
                dispatched.push(i.original_effect_id.clone());
                Ok(Json::obj([]))
            },
        )
        .unwrap();
    // Reverse commit order — eff-c first, then eff-b.
    assert_eq!(dispatched, vec!["eff-c".to_string(), "eff-b".to_string()]);
    assert_eq!(rec.compensation_list.len(), 2);
    assert!(rec
        .compensation_list
        .iter()
        .all(|e| e.disposition == "compensated"));
    assert!(rec.uncompensable.is_empty());
    assert!(rec.rewound);
    // The durable rows: rolled_back then head.moved — appended, never a
    // truncate (the WAL only grew).
    let evs = events(&s, &run);
    assert!(evs.len() > wal_before);
    let rb = evs
        .iter()
        .position(|e| e.class == "lifecycle.run.rolled_back")
        .expect("rolled_back row");
    let hm = evs
        .iter()
        .position(|e| e.class == "lifecycle.head.moved")
        .expect("head.moved row");
    assert!(rb < hm, "rolled_back must precede head.moved");
    // The rewind note blob is live and named by the row's record_ref.
    let record_ref = evs[rb]
        .payload
        .get("record_ref")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let addr = hh_identity::idp::parse_id(&record_ref)
        .map(|p| hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: p.digest_hex,
            media_type: "application/json".to_string(),
            size: 0,
        })
        .unwrap();
    let note = s.get_blob(&addr).unwrap();
    let note = String::from_utf8(note).unwrap();
    assert!(note.contains("eff-b~comp"), "note lacks the list: {note}");
    // The logical head moved to the cut; the originals stay readable.
    assert_eq!(s.head(&run).unwrap().seq, cut);
    // The compensated fold is durable — eff-b/eff-c read `Compensated`.
    for orig in ["eff-b", "eff-c"] {
        assert_eq!(
            s.effect_fold(&run, orig).unwrap().unwrap().phase,
            EffectPhase::Compensated,
            "{orig} not compensated"
        );
    }
}

#[test]
fn ac_2_2_4_7_rollback_names_the_uncompensable_and_survives_rebuild() {
    let d = dir("rollback-irrev");
    let run;
    let cut;
    {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (r, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        run = r;
        cut = s.head(&run).unwrap().seq;
        to_applied(&mut s, &run, &lease, "eff-irrev", irreversible());
        let rec = s
            .rollback(
                &run,
                &lease,
                cut,
                "operator",
                vec!["net_egress".into()],
                &mut |_| panic!("an uncompensable effect must never be dispatched"),
            )
            .unwrap();
        assert_eq!(rec.uncompensable, vec!["eff-irrev".to_string()]);
        assert_eq!(rec.uncaptured, vec!["net_egress".to_string()]);
        assert!(rec.compensation_list.is_empty());
    }
    // Rebuild parity — the rewind is a fold of the WAL, not process memory.
    let s2 = reopen(&d, 90_000);
    assert_eq!(s2.head(&run).unwrap().seq, cut);
    let evs = events(&s2, &run);
    assert!(evs.iter().any(|e| e.class == "lifecycle.run.rolled_back"));
    // Append-only: the irreversible original is still there.
    assert!(evs.iter().any(|e| e.class == "action.effect.observed"
        && e.scope.effect_id.as_deref() == Some("eff-irrev")));
}

// ── AC-R-2.2.4-8 — the source-prefix pin ───────────────────────────────────

#[test]
fn ac_2_2_4_8_fork_pins_source_prefix_content_against_gc_and_redact() {
    let (mut s, run, lease) = open("fork-pin");
    // A blob the source prefix references (the envelope `refs` member).
    let blob = s.put_blob(b"evidence", "text/plain").unwrap();
    let mut ev = eff(
        "with-ref",
        "lifecycle.hosted.native_record",
        "x",
        Json::obj([("note", Json::str("carries a ref"))]),
    );
    ev.refs = vec![blob.clone()];
    s.append(&run, &lease, vec![ev]).unwrap();
    let cut = s.head(&run).unwrap().seq;
    // Before the fork the blob is collectible from *another* run's view…
    // after it, the child's `forked` row pins every prefix ref.
    let (_child, _l, rec) = s
        .fork(
            &run,
            cut,
            BranchKind::Branch,
            &ForkOpts::default(),
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert_eq!(rec.at_seq, cut);
    // GC from the source run — the child's refs pin the address.
    match s.gc(&run, &lease, vec![blob.id()], "policy:default", "hot", None) {
        Err(LedgerError::Pinned { reason, .. }) => {
            assert!(reason.starts_with("referenced_by_run:"), "{reason}");
        }
        other => panic!("expected Pinned, got {other:?}"),
    }
    // Redaction is pinned the same way — the endorsement gate passes, the
    // pin refuses.
    let endorser = ProvenanceRecord::minted(
        Origin::human("human:alice", hh_provenance::HumanRole::Principal),
        PersistenceScope::Run,
        0,
    );
    match s.redact(
        &run,
        &lease,
        vec![RedactTarget::Address(blob.id())],
        "subject_request",
        &endorser,
        "approval",
    ) {
        Err(LedgerError::Pinned { .. }) => {}
        other => panic!("expected Pinned, got {other:?}"),
    }
    // And the bytes are still there.
    assert_eq!(s.get_blob(&blob).unwrap(), b"evidence");
}

#[test]
fn ac_2_2_4_8_snapshot_ref_is_pinned_into_the_fork_row() {
    let (mut s, run, lease) = open("fork-snap-pin");
    to_applied(&mut s, &run, &lease, "eff-1", compensable());
    let cut = s.head(&run).unwrap().seq;
    let snap_blob = s.put_blob(b"snapshot-record", "application/json").unwrap();
    let opts = ForkOpts {
        env: EnvBinding::Snapshot,
        snapshot_ref: Some(snap_blob.id()),
        snapshot_at_seq: Some(cut),
        ..ForkOpts::default()
    };
    let (child, _l, rec) = s
        .fork(
            &run,
            cut,
            BranchKind::Branch,
            &opts,
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert_eq!(rec.snapshot_ref.as_deref(), Some(snap_blob.id().as_str()));
    // The forked row's envelope refs carry the snapshot blob id.
    let forked = events(&s, &child)
        .iter()
        .find(|e| e.event_id == rec.forked_event_id)
        .unwrap();
    assert!(forked
        .refs
        .iter()
        .any(|r| format!("{}:{}", r.algorithm, r.digest) == snap_blob.id()));
    match s.gc(
        &run,
        &lease,
        vec![snap_blob.id()],
        "policy:default",
        "hot",
        None,
    ) {
        Err(LedgerError::Pinned { .. }) => {}
        other => panic!("expected Pinned, got {other:?}"),
    }
}

// ── trace_only / replay-mode record fields ─────────────────────────────────

#[test]
fn trace_only_marks_the_record_read_only() {
    let (mut s, run, _lease) = open("fork-trace");
    let cut = s.head(&run).unwrap().seq;
    let opts = ForkOpts {
        env: EnvBinding::TraceOnly,
        read_only: true,
        ..ForkOpts::default()
    };
    let (_c, _l, rec) = s
        .fork(
            &run,
            cut,
            BranchKind::Branch,
            &opts,
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert!(rec.read_only);
    assert_eq!(rec.env, EnvBinding::TraceOnly);
    // Replay-mode downgrade is *carried*, never invented — the ledger stores
    // what the caller computed.
    let opts2 = ForkOpts {
        replay_mode: ReplayMode::Exact,
        effective_replay: ReplayMode::Observational,
        downgrade_reason: Some("coverage".into()),
        ..ForkOpts::default()
    };
    let (_c2, _l2, rec2) = s
        .fork(
            &run,
            cut,
            BranchKind::Branch,
            &opts2,
            child_manifest(),
            "writer-a",
        )
        .unwrap();
    assert_eq!(rec2.replay_mode, ReplayMode::Exact);
    assert_eq!(rec2.effective_replay, ReplayMode::Observational);
    assert_eq!(rec2.downgrade_reason.as_deref(), Some("coverage"));
}
