//! S4.13 integration tests — the §5a.3/§5a.4 extension slices (C1/C2) and
//! the §5a.1 derived-index latency criterion against the real `Store`.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.1-16 — derived-index query latency: a `class`/`ir_refs`
//!   filtered `read` resolves from postings (result-proportional, not
//!   run-length-proportional); results are identical to the linear
//!   semantics and stable across a rebuild (the index is derived — the
//!   WAL is the truth).
//! - AC-R-2.2.3-10 — the continuation chain: `continued_from` verifies,
//!   a second continuation from the same head is `AlreadyContinued`, a
//!   goal-scoped subscription lands on the inbox run, `activation_no`
//!   increments, `carried` is recorded (never a copied transcript).
//! - AC-R-2.2.4-10 — budget and authority containment: branch spend
//!   posts `control.budget.consumed` to the branch's slice; root-first
//!   exhaustion disposes open branches `abandoned{budget_exhausted}`; a
//!   permission claim or `SpeculationPolicy` the parent does not hold is
//!   `AuthorityWidening` at `fork`.
//! - `subscribe`/`record_occurrence` — `schedule`/`external`/
//!   `peer_message` triggers land `control.wakeup.{scheduled,occurred}`;
//!   a duplicate occurrence is `skipped{duplicate_occurrence}`.
//! - `promote`/`discard` — deferred effects release only through the
//!   explicit release map; `read_only` refuses writes and promotion.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::branch_ops::{IntraBranchState, OpenBranchSpec};
use hh_ledger::event::{Cursor, Direction, Event, Producer, ReadFilter, Scope};
use hh_ledger::goal::ContinueCarried;
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{AttendanceSource, AttendanceValue, EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{
    Coalesce, DeliveryMode, OccurOutcome, ScheduleKind, Trigger, WakeupPolicy,
};
use hh_ledger::LedgerError;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-s413-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open_store(tag: &str) -> Store {
    Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = open_store(tag);
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn row(id: &str, class: &str, payload: Json) -> Event {
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
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn eff(id: &str, class: &str, effect_id: &str, payload: Json, branch: Option<&str>) -> Event {
    let mut ev = row(id, class, payload);
    ev.scope = Scope {
        effect_id: Some(effect_id.to_string()),
        branch_id: branch.map(str::to_string),
        ..Scope::default()
    };
    ev
}

fn risk(rev: &str, rs: &str, scope: &str) -> Json {
    Json::obj([
        ("reversibility", Json::str(rev)),
        ("repeat_safety", Json::str(rs)),
        ("scope", Json::str(scope)),
    ])
}

fn reversible() -> Json {
    risk("reversible", "idempotent", "workspace_local")
}

fn noop_branch_spec() -> OpenBranchSpec {
    OpenBranchSpec {
        kind: "speculative".into(),
        fork_seq: None,
        read_only: false,
        policy: None,
        budget_slice_id: None,
        env_binding: None,
        evidence_path: None,
        permissions: vec![],
        extra: vec![],
    }
}

fn class_filter(c: &str) -> ReadFilter {
    ReadFilter {
        class: Some(c.to_string()),
        ..ReadFilter::default()
    }
}

fn read_all(s: &Store, run: &str, filter: &ReadFilter) -> Vec<u64> {
    s.read(run, Cursor::Seq(0), Some(filter), Direction::Fwd, 100_000)
        .unwrap()
        .events
        .iter()
        .map(|e| e.seq)
        .collect()
}

// ── AC-R-2.2.1-16 — derived-index read latency + equivalence ────────────

#[test]
fn ac_r_2_2_1_16_class_filtered_read_matches_linear_semantics() {
    let (mut s, run, lease) = open("idxeq");
    // Interleave classes so the postings see a mixed log.
    for i in 0..200u64 {
        let class = if i % 5 == 0 {
            "lifecycle.hosted.native_record"
        } else if i % 7 == 0 {
            "control.budget.consumed"
        } else {
            "lifecycle.turn.started"
        };
        s.append(
            &run,
            &lease,
            vec![row(&format!("ev-{i}"), class, Json::obj([]))],
        )
        .unwrap();
    }
    for class in [
        "lifecycle.hosted.native_record",
        "control.budget.consumed",
        "lifecycle.turn.started",
    ] {
        let got = read_all(&s, &run, &class_filter(class));
        let linear: Vec<u64> = s
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == class)
            .map(|e| e.seq)
            .collect();
        assert_eq!(got, linear, "indexed read diverges for {class}");
        assert!(got.windows(2).all(|w| w[0] < w[1]), "ordering preserved");
    }
    // `class.*` prefix filter — the dotted-family range.
    let got = read_all(&s, &run, &class_filter("control.budget.*"));
    let linear: Vec<u64> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class.starts_with("control.budget."))
        .map(|e| e.seq)
        .collect();
    assert_eq!(got, linear, "prefix filter diverges");
    // Inclusive cursor through the index: page 1 of 5, then resume —
    // the union is exactly the linear result, no dup, no loss.
    let f = class_filter("lifecycle.hosted.native_record");
    let p1 = s
        .read(&run, Cursor::Seq(0), Some(&f), Direction::Fwd, 5)
        .unwrap();
    assert_eq!(p1.events.len(), 5);
    let cursor = p1.next_cursor.clone().expect("cursor on a partial page");
    let p2 = s
        .read(&run, cursor, Some(&f), Direction::Fwd, 100_000)
        .unwrap();
    let all: Vec<u64> = p1
        .events
        .iter()
        .chain(p2.events.iter())
        .map(|e| e.seq)
        .collect();
    let linear: Vec<u64> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.hosted.native_record")
        .map(|e| e.seq)
        .collect();
    assert_eq!(all, linear, "cursor-paged indexed read loses rows");
}

#[test]
fn ac_r_2_2_1_16_index_survives_rebuild_identically() {
    let d = dir("idxre");
    let (run_id, expected) = {
        let mut s = Store::open_with(
            d.clone(),
            Box::new(ManualClock::at(1_000)),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        for i in 0..300u64 {
            let class = if i % 3 == 0 {
                "lifecycle.branch.opened"
            } else {
                "lifecycle.turn.started"
            };
            s.append(
                &run,
                &lease,
                vec![row(&format!("ev-{i}"), class, Json::obj([]))],
            )
            .unwrap();
        }
        let f = class_filter("lifecycle.branch.opened");
        let page = s
            .read(&run, Cursor::Seq(0), Some(&f), Direction::Fwd, 100_000)
            .unwrap();
        (
            run,
            page.events
                .iter()
                .map(|e| (e.seq, e.event_id.clone()))
                .collect::<Vec<_>>(),
        )
    };
    // Rebuild from the WAL — the derived index is reconstructed; the
    // filtered read answers identically (rebuild/live-commit equivalence,
    // CC1).
    let s2 = Store::open_with(
        d,
        Box::new(ManualClock::at(2_000)),
        Some(Box::new(SeqIds::starting_at(1_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let f = class_filter("lifecycle.branch.opened");
    let page = s2
        .read(&run_id, Cursor::Seq(0), Some(&f), Direction::Fwd, 100_000)
        .unwrap();
    let got: Vec<(u64, String)> = page
        .events
        .iter()
        .map(|e| (e.seq, e.event_id.clone()))
        .collect();
    assert_eq!(
        got, expected,
        "rebuilt index answers differ from live commit"
    );
}

/// The empirical leg of AC-R-2.2.1-16 — a 10⁵-event fixture whose
/// class-filtered read returns in result-proportional time (the bound's
/// *shape*; OQ-388's numeric value is ADR-0214-deferred and the
/// measurement is recorded in the run ledger, not asserted by wall clock
/// here — a debug-profile hard bound would flake). Run with:
/// `cargo test -p hh-ledger --test s4_13 latency_1e5 -- --ignored`.
#[test]
#[ignore]
fn ac_r_2_2_1_16_latency_1e5_fixture() {
    let (mut s, run, lease) = open("idx1e5");
    let t0 = std::time::Instant::now();
    // Batched appends — the durable batch is the WAL's atomic unit; the
    // fixture measures the *index read*, not per-call append overhead.
    for chunk in 0..100u64 {
        let batch: Vec<Event> = (0..1_000u64)
            .map(|i| {
                let n = chunk * 1_000 + i;
                let class = if n % 1_000 == 0 {
                    "lifecycle.hosted.native_record"
                } else {
                    "lifecycle.turn.started"
                };
                row(&format!("ev-{n}"), class, Json::obj([]))
            })
            .collect();
        s.append(&run, &lease, batch).unwrap();
    }
    let build = t0.elapsed();
    let f = class_filter("lifecycle.hosted.native_record");
    let t1 = std::time::Instant::now();
    let page = s
        .read(&run, Cursor::Seq(0), Some(&f), Direction::Fwd, 100_000)
        .unwrap();
    let read_t = t1.elapsed();
    assert_eq!(page.events.len(), 100);
    eprintln!(
        "AC-R-2.2.1-16 measurement: build={build:?} filtered_read(100 of 1e5)={read_t:?} \
         (bound shape: result-proportional — OQ-388)"
    );
}

// ── AC-R-2.2.3-10 — the continuation chain ──────────────────────────────

#[test]
fn ac_r_2_2_3_10_continuation_chain_and_inbox() {
    let d = dir("cont");
    let mut s = Store::open_with(
        d.clone(),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    // Activation 1 — a goal-scoped agent run.
    let mut m1 = RunManifest::minimal(RunKind::Agent);
    m1.goal_ref = Some("goal-77".to_string());
    let (a1, lease1) = s.open_run(m1, "writer-a").unwrap();
    s.append(
        &a1,
        &lease1,
        vec![row("ev-a1", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    s.append(
        &a1,
        &lease1,
        vec![row(
            "ev-a1f",
            "lifecycle.run.finished",
            Json::obj([("stop_reason", Json::str("goal_continue"))]),
        )],
    )
    .unwrap();
    // The goal's inbox run holds goal-scoped subscriptions.
    let (inbox, inbox_lease) = s.open_inbox("goal-77", "writer-a").unwrap();
    assert_eq!(
        s.inbox_for_goal("goal-77").unwrap().as_deref(),
        Some(inbox.as_str())
    );
    let m_inbox = s.manifest(&inbox).unwrap();
    assert_eq!(m_inbox.run_kind, RunKind::Inbox);
    assert_eq!(m_inbox.goal_ref.as_deref(), Some("goal-77"));
    // A goal-scoped subscription lands on the inbox run (the durable
    // `control.wakeup.scheduled` — survives every activation).
    let created = EventRef {
        run_id: inbox.clone(),
        event_id: s.head(&inbox).unwrap().event_id,
    };
    let sub = s
        .goal_subscribe(
            "goal-77",
            &inbox,
            &inbox_lease,
            Trigger::Schedule {
                expr: "every:60000".to_string(),
                kind: ScheduleKind::Interval,
                timezone: "UTC".to_string(),
            },
            WakeupPolicy {
                delivery_mode: DeliveryMode::FollowUp,
                coalesce: Coalesce::Latest,
                max_pending: 4,
                expires_at_ms: None,
                attendance_required: None,
                occurrence_key_fn: None,
            },
            &created,
        )
        .unwrap();
    // An occurrence arrives while no activation is open — durable in the
    // inbox run BEFORE any run acts (the §5a.3 rule).
    let occ = s
        .goal_occurred("goal-77", &inbox_lease, &sub, "tick-1", None, 5_000)
        .unwrap();
    assert!(matches!(occ, OccurOutcome::Occurred(_)));
    assert!(
        s.events(&inbox)
            .unwrap()
            .iter()
            .any(|e| e.class == "control.wakeup.occurred"
                && e.payload.get("occurrence_key").and_then(Json::as_str) == Some("tick-1")),
        "the goal occurrence is durable in the inbox run"
    );
    // Activation 2 — `continue_goal` carries refs, never a transcript.
    let carried = ContinueCarried {
        resume_set_heads: vec!["rs:ctx-1".to_string()],
        budget_id: Some("bch_goal:goal-77".to_string()),
    };
    let c2 = s
        .continue_goal("goal-77", &a1, &carried, None, "writer-a")
        .unwrap();
    assert_eq!(c2.activation_no, 2);
    let m2 = s.manifest(&c2.run_id).unwrap();
    assert_eq!(m2.activation_no, 2);
    assert_eq!(m2.goal_ref.as_deref(), Some("goal-77"));
    let link = m2.continued_from.as_ref().expect("continued_from set");
    assert_eq!(link.run_id, a1);
    assert_eq!(
        link.at_seq as usize,
        s.events(&a1).unwrap().len() - 1,
        "the link binds the from-run's head"
    );
    assert_eq!(m2.budget.as_deref(), Some("bch_goal:goal-77"));
    assert_eq!(
        m2.extra.get("carried"),
        Some(&carried.to_json()),
        "carried rides the manifest — refs, never a transcript"
    );
    // A second continuation from the same head — `AlreadyContinued`.
    let dup = s.continue_goal("goal-77", &a1, &carried, None, "writer-a");
    assert!(
        matches!(dup, Err(LedgerError::AlreadyContinued { .. })),
        "duplicate continuation must refuse: {dup:?}"
    );
    // Activation 3 — the chain walks forward.
    s.append(
        &c2.run_id,
        &c2.lease,
        vec![row("ev-a2f", "lifecycle.run.finished", Json::obj([]))],
    )
    .unwrap();
    let c3 = s
        .continue_goal(
            "goal-77",
            &c2.run_id,
            &ContinueCarried::default(),
            None,
            "writer-a",
        )
        .unwrap();
    assert_eq!(s.manifest(&c3.run_id).unwrap().activation_no, 3);
    // The chain is restart-stable — rebuild and re-check the links.
    drop(s);
    let s2 = Store::open_with(
        d,
        Box::new(ManualClock::at(9_000)),
        Some(Box::new(SeqIds::starting_at(9_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let l2 = s2.manifest(&c2.run_id).unwrap().continued_from.clone();
    let l3 = s2.manifest(&c3.run_id).unwrap().continued_from.clone();
    assert_eq!(l2.unwrap().run_id, a1);
    assert_eq!(l3.unwrap().run_id, c2.run_id);
}

// ── the wakeup pair — schedule / external / peer_message ────────────────

#[test]
fn s4_13_schedule_external_peer_message_subscribe_occurred() {
    let (mut s, run, lease) = open("wakes413");
    let created = EventRef {
        run_id: run.clone(),
        event_id: s.head(&run).unwrap().event_id,
    };
    let pol = || WakeupPolicy {
        delivery_mode: DeliveryMode::FollowUp,
        coalesce: Coalesce::Latest,
        max_pending: 8,
        expires_at_ms: None,
        attendance_required: None,
        occurrence_key_fn: None,
    };
    // `schedule` — cron + interval kinds admitted; a malformed expression
    // is a schema error at subscribe.
    let sub_cron = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Schedule {
                expr: "0 9 * * *".to_string(),
                kind: ScheduleKind::Cron,
                timezone: "UTC".to_string(),
            },
            pol(),
            &created,
        )
        .unwrap();
    let bad = s.wakeup_subscribe(
        &run,
        &lease,
        Trigger::Schedule {
            expr: "not-a-schedule".to_string(),
            kind: ScheduleKind::Interval,
            timezone: "UTC".to_string(),
        },
        pol(),
        &created,
    );
    assert!(
        matches!(bad, Err(LedgerError::SchemaViolation { .. })),
        "malformed schedule must refuse: {bad:?}"
    );
    // `external` — admitted on every run kind at S4.13 (`source_ref` names
    // the registered ingress adapter; `filter` rides verbatim).
    let sub_ext = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "webhook".to_string(),
                source_ref: Some("ingress:github".to_string()),
                filter: Some("event = push".to_string()),
            },
            pol(),
            &created,
        )
        .unwrap();
    // `peer_message` — the one peer-messaging mechanism (ADR-0191).
    let sub_peer = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::PeerMessage {
                from: "run:sibling".to_string(),
            },
            pol(),
            &created,
        )
        .unwrap();
    // `control.wakeup.scheduled` rows are durable — one per subscription.
    let scheduled = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.wakeup.scheduled")
        .count();
    assert_eq!(scheduled, 3, "{sub_cron} {sub_ext} {sub_peer}");
    // `record_occurrence` — durable before any run acts; a duplicate key
    // is `skipped{duplicate_occurrence}`, audited, never a second fire.
    let o1 = s
        .wakeup_occurred(&run, &lease, &sub_peer, "msg-9", Some("blob:m9"), 5_000)
        .unwrap();
    assert!(matches!(o1, OccurOutcome::Occurred(_)));
    let o2 = s
        .wakeup_occurred(&run, &lease, &sub_peer, "msg-9", Some("blob:m9"), 5_001)
        .unwrap();
    assert!(
        matches!(o2, OccurOutcome::Skipped(_)),
        "duplicate occurrence must skip: {o2:?}"
    );
    assert_eq!(
        s.events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "control.wakeup.occurred")
            .count(),
        1
    );
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "control.wakeup.skipped"
                && e.payload.get("reason").and_then(Json::as_str) == Some("duplicate_occurrence")),
        "the duplicate is audited"
    );
    // An interactive-attendance run refuses an unattended `schedule`
    // subscription unless a reachable principal is declared (§5a.3).
    let mut m_int = RunManifest::minimal(RunKind::Agent);
    m_int.attendance = (AttendanceValue::Interactive, AttendanceSource::Declared);
    let (run_i, lease_i) = s.open_run(m_int, "writer-i").unwrap();
    let created_i = EventRef {
        run_id: run_i.clone(),
        event_id: s.head(&run_i).unwrap().event_id,
    };
    let refused = s.wakeup_subscribe(
        &run_i,
        &lease_i,
        Trigger::Schedule {
            expr: "every:60000".to_string(),
            kind: ScheduleKind::Interval,
            timezone: "UTC".to_string(),
        },
        pol(),
        &created_i,
    );
    assert!(
        matches!(refused, Err(LedgerError::WakeupPolicyUnsupported { .. })),
        "interactive run must refuse an unattended schedule: {refused:?}"
    );
    // … and admits it when the policy names a reachable principal.
    let ok = s.wakeup_subscribe(
        &run_i,
        &lease_i,
        Trigger::Schedule {
            expr: "every:60000".to_string(),
            kind: ScheduleKind::Interval,
            timezone: "UTC".to_string(),
        },
        WakeupPolicy {
            attendance_required: Some("principal:steven".to_string()),
            ..pol()
        },
        &created_i,
    );
    assert!(ok.is_ok(), "reachable principal must admit: {ok:?}");
}

// ── AC-R-2.2.4-10 — budget + authority containment ──────────────────────

#[test]
fn ac_r_2_2_4_10_authority_widening_at_fork() {
    let (mut s, run, lease) = open("br-widen");
    s.append(
        &run,
        &lease,
        vec![row("ev-base", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    // A `SpeculationPolicy` that widens `defer_irreversible` — refused at
    // `fork`, never coerced (the floor is not overridable by any layer).
    let mut spec = noop_branch_spec();
    spec.policy = Some(Json::obj([
        ("allow_classes", Json::Arr(vec![Json::str("irreversible")])),
        ("defer_irreversible", Json::Bool(false)),
    ]));
    let widened = s.open_branch(&run, &lease, &spec);
    assert!(
        matches!(widened, Err(LedgerError::AuthorityWidening { .. })),
        "defer_irreversible=false must refuse: {widened:?}"
    );
    // `allow_classes ⊋ irreversible` alone widens too.
    spec.policy = Some(Json::obj([(
        "allow_classes",
        Json::Arr(vec![Json::str("irreversible")]),
    )]));
    let widened2 = s.open_branch(&run, &lease, &spec);
    assert!(
        matches!(widened2, Err(LedgerError::AuthorityWidening { .. })),
        "allow_classes with irreversible must refuse: {widened2:?}"
    );
    // A permission claim the parent does not hold — `AuthorityWidening`.
    spec.policy = None;
    spec.permissions = vec!["perm:write:net".to_string()];
    let unheld = s.open_branch(&run, &lease, &spec);
    assert!(
        matches!(unheld, Err(LedgerError::AuthorityWidening { .. })),
        "unheld permission claim must refuse: {unheld:?}"
    );
    // After `security.permission.decided{allow}` for the id, the same
    // claim holds — the branch narrows-or-matches, never widens.
    let mut grant = row(
        "ev-p",
        "security.permission.decided",
        Json::obj([
            ("permission_id", Json::str("perm:write:net")),
            ("decision", Json::str("allow")),
        ]),
    );
    grant.scope = Scope::default();
    s.append(&run, &lease, vec![grant]).unwrap();
    let held = s.open_branch(&run, &lease, &spec).unwrap();
    let opened = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| {
            e.class == "lifecycle.branch.opened"
                && e.scope.branch_id.as_deref() == Some(held.as_str())
        })
        .unwrap();
    assert_eq!(
        opened.payload.get("permissions"),
        Some(&Json::Arr(vec![Json::str("perm:write:net")]))
    );
}

#[test]
fn ac_r_2_2_4_10_branch_spend_posts_to_slice_and_root_first_abandons() {
    let mut s = open_store("br-budget");
    // The run's root budget node is the manifest's `budget` member.
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.budget = Some("bch_root".to_string());
    let (run, lease) = s.open_run(m, "writer-a").unwrap();
    s.append(
        &run,
        &lease,
        vec![row("ev-0", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    // Two branches on two slices — the second open's policy declares the
    // raised concurrent cap (the floor is 1; the policy may only narrow
    // `MAX_INTRA_BRANCHES` above it, so the declaration is the gate).
    let mut spec_a = noop_branch_spec();
    spec_a.budget_slice_id = Some("slice:a".to_string());
    let ba = s.open_branch(&run, &lease, &spec_a).unwrap();
    let mut spec_b = noop_branch_spec();
    spec_b.budget_slice_id = Some("slice:b".to_string());
    spec_b.policy = Some(Json::obj([("max_concurrent_branches", Json::Int(2))]));
    let bb = s.open_branch(&run, &lease, &spec_b).unwrap();
    // Slice-scoped exhaustion: only `slice:a`'s branch abandons.
    s.budget_exhausted(&run, &lease, "slice:a").unwrap();
    assert!(matches!(
        s.intra_branch(&run, &ba).map(|b| b.state),
        Some(IntraBranchState::Disposed(ref d)) if d == "abandoned"
    ));
    assert!(matches!(
        s.intra_branch(&run, &bb).map(|b| b.state),
        Some(IntraBranchState::Open)
    ));
    let disposed_a = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| {
            e.class == "lifecycle.branch.disposed"
                && e.payload.get("branch_id").and_then(Json::as_str) == Some(ba.as_str())
        })
        .unwrap();
    assert_eq!(
        disposed_a.payload.get("reason").and_then(Json::as_str),
        Some("budget_exhausted")
    );
    // The branch's spend posts `control.budget.consumed` to its slice.
    let consumed = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| {
            e.class == "control.budget.consumed"
                && e.payload.get("branch_id").and_then(Json::as_str) == Some(ba.as_str())
        })
        .expect("consumed row posted");
    assert_eq!(
        consumed
            .payload
            .get("budget_slice_id")
            .and_then(Json::as_str),
        Some("slice:a")
    );
    // Root-first exhaustion at the manifest `budget` id ends the rest.
    s.budget_exhausted(&run, &lease, "bch_root").unwrap();
    assert!(matches!(
        s.intra_branch(&run, &bb).map(|b| b.state),
        Some(IntraBranchState::Disposed(ref d)) if d == "abandoned"
    ));
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "control.budget.exceeded"
            && e.payload.get("root_first") == Some(&Json::Bool(true))));
}

// ── promote / discard — the deferred-effect release discipline ──────────

#[test]
fn s4_13_branch_open_promote_discard_and_policy_gate() {
    let (mut s, run, lease) = open("br-ops");
    s.append(
        &run,
        &lease,
        vec![row("ev-base", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    let spec = noop_branch_spec();
    let b = s.open_branch(&run, &lease, &spec).unwrap();
    // A real effect chain under the branch: intended → authorized →
    // prepared → deferred — every row branch-scoped so the fold's
    // `effect_first_seen` tracks it (first-seen order = promotion order).
    let rc = reversible();
    let mut chain = vec![
        eff(
            "ev-i",
            "action.effect.intended",
            "eff-b1",
            Json::obj([
                ("effect_id", Json::str("eff-b1")),
                ("effective_risk_class", rc.clone()),
            ]),
            Some(&b),
        ),
        eff(
            "ev-a",
            "action.effect.authorized",
            "eff-b1",
            Json::obj([("effective_risk_class", rc)]),
            Some(&b),
        ),
        eff(
            "ev-p",
            "action.effect.prepared",
            "eff-b1",
            Json::obj([
                ("idempotency_key", Json::str("key-b1")),
                ("baseline_ref", Json::str("snap-1")),
            ]),
            Some(&b),
        ),
        eff(
            "ev-d",
            "action.effect.deferred",
            "eff-b1",
            Json::obj([
                ("reason", Json::str("speculative_branch")),
                ("branch_id", Json::str(&b)),
            ]),
            Some(&b),
        ),
    ];
    for ev in chain.drain(..) {
        s.append(&run, &lease, vec![ev]).unwrap();
    }
    // Promote without naming the deferred effect — `DeferredReleaseMissing`
    // (the caller must name every one; silence is never a release).
    let missing = s.promote_branch(&run, &lease, &b, &BTreeMap::new());
    assert!(
        matches!(missing, Err(LedgerError::DeferredReleaseMissing { .. })),
        "promote must name every deferred effect: {missing:?}"
    );
    // Discard — HEAD rewinds to the fork point; disposed{discarded}; the
    // deferred effect is refused `branch_discarded`.
    let fork_seq = s.intra_branch(&run, &b).unwrap().fork_seq;
    let head_before = s.head(&run).unwrap().seq;
    assert!(head_before > fork_seq);
    s.discard_branch(&run, &lease, &b, &[], None, &mut |_| {
        Err("no_compensator".to_string())
    })
    .unwrap();
    // The rewind is the `head.moved{to_seq: fork}` row — the disposed and
    // consumed accounting rows land after it and themselves extend the
    // logical head (ADR-0027 §5: every durable row extends HEAD).
    let after = s.head(&run).unwrap().seq;
    assert!(after > head_before, "the discard batch extends the tip");
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "lifecycle.head.moved"
                && e.payload.get("to_seq").and_then(Json::as_int) == Some(fork_seq as i64)
                && e.payload
                    .get("reason")
                    .and_then(Json::as_str)
                    .map(|r| r.contains("discard"))
                    .unwrap_or(false)),
        "discard records the rewind to the fork point"
    );
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "lifecycle.branch.disposed"
            && e.payload.get("disposition").and_then(Json::as_str) == Some("discarded")));
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "action.effect.refused"
                && e.payload.get("reason").and_then(Json::as_str) == Some("branch_discarded")),
        "the deferred effect is refused on discard"
    );
    // `read_only` — observes, never writes, never promotes.
    let mut ro = noop_branch_spec();
    ro.read_only = true;
    let rb = s.open_branch(&run, &lease, &ro).unwrap();
    let w = eff(
        "ev-rw",
        "action.effect.intended",
        "eff-ro",
        Json::obj([
            ("effect_id", Json::str("eff-ro")),
            ("effective_risk_class", reversible()),
        ]),
        Some(&rb),
    );
    let refused = s.append(&run, &lease, vec![w]);
    assert!(
        matches!(refused, Err(LedgerError::SpeculationViolation { .. })),
        "read_only branch admits no action writes: {refused:?}"
    );
    let promoted = s.promote_branch(&run, &lease, &rb, &BTreeMap::new());
    assert!(
        matches!(promoted, Err(LedgerError::SpeculationViolation { .. })),
        "read_only is never promotable: {promoted:?}"
    );
}
