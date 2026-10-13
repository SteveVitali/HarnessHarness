//! R2.6 — the `delivery_mode = steer` admission leg (OQ-316's steer arm,
//! DF-S2.11-1's durable half) against the real `Store`.
//!
//! The rule the ticket lands: `steer` is admitted *only* on
//! `manual{principal}` subscriptions — the `steer{mode: next_turn}`
//! boundary op's durable queue — and, unlike `manual` + `follow_up`, on
//! any run kind (the principal steers the steered run itself; S4.9's
//! fleet boundary is a `follow_up` rule). Every non-`manual` trigger
//! keeps the typed `WakeupPolicyUnsupported` refusal. Fired steer
//! occurrences share `follow_up`'s `deliver_after` discipline (W-3: a
//! delivery never lands mid-committed-effect) and the same
//! restart/at-least-once machinery — the wakeup table is the one durable
//! steer record; no second cue channel exists.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{
    DeliveryMode, FireOutcome, OccurOutcome, ScheduleKind, Trigger, WakeupPolicy,
};
use hh_ledger::LedgerError;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-r26-steer-{}-{tag}-{n}", std::process::id()));
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

/// Reopen the store over `dir` — the process-crash-and-restart fixture
/// (the WAL is the record; process memory is gone).
fn reopen(d: &std::path::Path, ms: u64) -> Store {
    Store::open_with(
        d,
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::starting_at(1_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn steer_policy() -> WakeupPolicy {
    WakeupPolicy {
        delivery_mode: DeliveryMode::Steer,
        ..WakeupPolicy::default_policy()
    }
}

fn eref(run: &str) -> EventRef {
    EventRef {
        run_id: run.to_string(),
        event_id: "e-0".to_string(),
    }
}

fn eff(id: &str, class: &str, effect_id: &str, payload: Json, fencing: u64) -> Event {
    let mut payload = payload;
    if fencing > 0 {
        if let Json::Obj(m) = &mut payload {
            m.insert("fencing_token".to_string(), Json::Int(fencing as i64));
        }
    }
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
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn compensable() -> Json {
    Json::obj([
        ("reversibility", Json::str("compensable")),
        ("repeat_safety", Json::str("idempotent")),
        ("scope", Json::str("workspace_local")),
    ])
}

/// `intended → authorized → decided{allow} → prepared → committed` — the
/// effect crosses the write-ahead barrier and stays open.
fn to_committed(s: &mut Store, run: &str, lease: &Lease, eid: &str, gen: u64) {
    s.append(
        run,
        lease,
        vec![
            eff(
                &format!("{eid}-i"),
                "action.effect.intended",
                eid,
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("effective_risk_class", compensable()),
                ]),
                0,
            ),
            eff(
                &format!("{eid}-a"),
                "action.effect.authorized",
                eid,
                Json::obj([("effective_risk_class", compensable())]),
                0,
            ),
            eff(
                &format!("{eid}-d"),
                "security.permission.decided",
                eid,
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("attempt_no", Json::Int(1)),
                    ("decision", Json::str("allow")),
                ]),
                0,
            ),
            eff(
                &format!("{eid}-p"),
                "action.effect.prepared",
                eid,
                Json::obj([
                    ("idempotency_key", Json::str(format!("key-{eid}"))),
                    ("compensation_plan_id", Json::str(format!("plan-{eid}"))),
                ]),
                0,
            ),
            eff(
                &format!("{eid}-c"),
                "action.effect.committed",
                eid,
                Json::obj([("attempt_no", Json::Int(1))]),
                gen,
            ),
        ],
    )
    .unwrap();
}

fn observed(id: &str, effect_id: &str, outcome: &str, gen: u64) -> Event {
    eff(
        id,
        "action.effect.observed",
        effect_id,
        Json::obj([
            ("attempt_no", Json::Int(1)),
            ("outcome", Json::str(outcome)),
        ]),
        gen,
    )
}

fn events(s: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    s.events(run).unwrap().to_vec()
}

fn count_class(s: &Store, run: &str, class: &str) -> usize {
    events(s, run).iter().filter(|e| e.class == class).count()
}

// ── Admission ─────────────────────────────────────────────────────────────

/// The R2.6 arm: `manual{principal}` + `delivery_mode = steer` subscribes
/// on an `agent` run — the steer queue is a property of the run the
/// principal steers, not of fleet activation (the S4.9 boundary is a
/// `follow_up` rule).
#[test]
fn r2_6_steer_manual_subscribes_on_agent_run() {
    let (mut s, run, lease) = open("steer-manual-agent");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            steer_policy(),
            &eref(&run),
        )
        .unwrap();
    assert!(!sub.is_empty());
    assert_eq!(count_class(&s, &run, "control.wakeup.scheduled"), 1);
    let evs = events(&s, &run);
    let row = evs
        .iter()
        .find(|e| e.class == "control.wakeup.scheduled")
        .unwrap();
    let policy = row
        .payload
        .get("subscription")
        .and_then(|s| s.get("policy"))
        .unwrap();
    assert_eq!(
        policy.get("delivery_mode").and_then(Json::as_str),
        Some("steer")
    );
}

/// `manual` + `follow_up` stays fleet-bound — the steer leg widened
/// `manual` admission for *steer only*; the S4.9 boundary is untouched.
#[test]
fn r2_6_manual_follow_up_stays_fleet_bound() {
    let (mut s, run, lease) = open("manual-follow-up-agent");
    let err = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap_err();
    assert!(
        matches!(err, LedgerError::TriggerUnsupported { .. }),
        "manual+follow_up on an agent run must keep the fleet refusal: {err:?}"
    );
    // …and on a `fleet` run the same pair still binds (the boundary is
    // preserved, not widened away).
    let (mut s2, fleet_run, fleet_lease) = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            dir("manual-follow-up-fleet"),
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let mut fm = RunManifest::minimal(RunKind::Fleet);
        fm.configuration_id = None;
        fm.configuration_version_id = None;
        s.open_run(fm, "writer-a").map(|(r, l)| (s, r, l)).unwrap()
    };
    let sub = s2
        .wakeup_subscribe(
            &fleet_run,
            &fleet_lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            WakeupPolicy::default_policy(),
            &eref(&fleet_run),
        )
        .unwrap();
    assert!(!sub.is_empty());
}

/// Every non-`manual` trigger keeps the typed refusal on
/// `delivery_mode = steer` — the admission rule is one leg, not a mode
/// the table accepts wholesale.
#[test]
fn r2_6_steer_refused_on_every_nonmanual_trigger() {
    let (mut s, run, lease) = open("steer-nonmanual");
    // `child_terminal` needs a real child — open one for the trigger that
    // names it (the refusal must be the policy's, not the child's).
    let mut child_manifest = RunManifest::minimal(RunKind::Agent);
    child_manifest.parent_run_id = Some(run.clone());
    let (child, _child_lease) = s.open_run(child_manifest, "writer-a").unwrap();
    let triggers = vec![
        Trigger::Timer { at_ms: 100 },
        Trigger::Schedule {
            expr: "*/5 * * * *".into(),
            kind: ScheduleKind::Cron,
            timezone: "UTC".into(),
        },
        Trigger::External {
            kind: "ingress".into(),
            source_ref: Some("adapter:a".into()),
            filter: None,
        },
        Trigger::PermissionDecided {
            permission_id: "perm-1".into(),
        },
        Trigger::ChildTerminal {
            child_run_id: child,
        },
        Trigger::EffectTerminal {
            effect_id: "eff-1".into(),
        },
        Trigger::EnvironmentReady {
            env_handle_id: "env-1".into(),
        },
        Trigger::RetryDue {
            scope_id: "scope-1".into(),
        },
        Trigger::PeerMessage {
            from: "peer-1".into(),
        },
    ];
    for t in triggers {
        let name = t.type_name().to_string();
        let err = s
            .wakeup_subscribe(&run, &lease, t, steer_policy(), &eref(&run))
            .unwrap_err();
        // `environment_ready`/`peer_message` may refuse at the trigger
        // stage instead — either way the subscription must not land and
        // `delivery_mode = steer` must never produce `scheduled` on a
        // non-manual trigger.
        assert!(
            matches!(
                err,
                LedgerError::WakeupPolicyUnsupported { .. }
                    | LedgerError::TriggerUnsupported { .. }
            ),
            "{name}: expected a typed refusal, got {err:?}"
        );
    }
    assert_eq!(count_class(&s, &run, "control.wakeup.scheduled"), 0);
}

// ── Delivery discipline ───────────────────────────────────────────────────

/// W-3 shared: a fired steer waits on a committed effect's terminal —
/// the committed window is not a decision point for `steer` either.
#[test]
fn r2_6_steer_waits_for_committed_effects_terminal() {
    let (mut s, run, lease) = open("steer-deliver-after");
    to_committed(&mut s, &run, &lease, "eff-1", lease.generation);
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            steer_policy(),
            &eref(&run),
        )
        .unwrap();
    s.wakeup_occurred(&run, &lease, &sub, "steer:art-1", Some("art-1"), 2_000)
        .unwrap();
    match s.wakeup_fire(&run, &lease, &sub, "steer:art-1").unwrap() {
        FireOutcome::Fired { deliver_after, .. } => {
            assert_eq!(deliver_after.as_deref(), Some("eff-1"))
        }
        other => panic!("expected Fired, got {other:?}"),
    }
    // Withheld while the effect is committed.
    assert!(s.wakeup_drain(&run).unwrap().is_empty());
    // The effect folds terminal — the steer delivers, carrying the mode
    // and the staged payload ref.
    s.append(
        &run,
        &lease,
        vec![observed("eff-1-o", "eff-1", "applied", lease.generation)],
    )
    .unwrap();
    let drained = s.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 1, "{drained:?}");
    assert_eq!(drained[0].delivery_mode, DeliveryMode::Steer);
    assert_eq!(drained[0].payload_ref.as_deref(), Some("art-1"));
    assert_eq!(drained[0].occurrence_key, "steer:art-1");
}

/// A steer occurrence with no committed effect pending delivers at the
/// first drain after its fire — `deliver_after` is absent.
#[test]
fn r2_6_steer_drains_with_payload_and_mode() {
    let (mut s, run, lease) = open("steer-drain");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            steer_policy(),
            &eref(&run),
        )
        .unwrap();
    s.wakeup_occurred(&run, &lease, &sub, "steer:art-9", Some("art-9"), 2_000)
        .unwrap();
    match s.wakeup_fire(&run, &lease, &sub, "steer:art-9").unwrap() {
        FireOutcome::Fired { deliver_after, .. } => assert!(deliver_after.is_none()),
        other => panic!("expected Fired, got {other:?}"),
    }
    let drained = s.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].delivery_mode, DeliveryMode::Steer);
    assert_eq!(drained[0].payload_ref.as_deref(), Some("art-9"));
}

/// `coalesce = none` (the queue_steer policy) — two steers are two
/// occurrences; the duplicate *key* is the audited skip.
#[test]
fn r2_6_steer_occurrences_queue_and_dedupe_by_key() {
    let (mut s, run, lease) = open("steer-dedupe");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Manual {
                principal: "principal:p".into(),
            },
            steer_policy(),
            &eref(&run),
        )
        .unwrap();
    let o1 = s
        .wakeup_occurred(&run, &lease, &sub, "steer:art-1", Some("art-1"), 2_000)
        .unwrap();
    assert!(matches!(o1, OccurOutcome::Occurred(_)));
    let o2 = s
        .wakeup_occurred(&run, &lease, &sub, "steer:art-2", Some("art-2"), 2_100)
        .unwrap();
    assert!(matches!(o2, OccurOutcome::Occurred(_)));
    // A re-delivered occurrence (racing steer ops, replayed drive) is the
    // audited skip — never a second fire.
    let dup = s
        .wakeup_occurred(&run, &lease, &sub, "steer:art-1", Some("art-1"), 2_200)
        .unwrap();
    assert!(matches!(dup, OccurOutcome::Skipped(_)));
    // `deliver_wakeup` fires every pending occurrence — both steers land.
    let out = s.deliver_wakeup(&run, &lease, 3_000).unwrap();
    assert_eq!(
        out.iter()
            .filter(|o| matches!(o, FireOutcome::Fired { .. }))
            .count(),
        2,
        "{out:?}"
    );
    let drained = s.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 2, "{drained:?}");
    assert!(drained
        .iter()
        .all(|d| d.delivery_mode == DeliveryMode::Steer));
    assert_eq!(count_class(&s, &run, "control.wakeup.fired"), 2);
}

// ── Restart ───────────────────────────────────────────────────────────────

/// DF-S2.11-1's headline: a queued steer — `occurred` durable, process
/// dead before the fire — re-delivers after restart through the same
/// machinery (the `occurred`-not-`fired` row is rediscovered by
/// `deliver_wakeup`; KP-11's discipline covers the steer leg too).
#[test]
fn r2_6_steer_occurred_before_fired_survives_restart() {
    let d = dir("steer-restart");
    let run = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        let sub = s
            .wakeup_subscribe(
                &run,
                &lease,
                Trigger::Manual {
                    principal: "principal:p".into(),
                },
                steer_policy(),
                &eref(&run),
            )
            .unwrap();
        s.wakeup_occurred(&run, &lease, &sub, "steer:art-1", Some("art-1"), 1_500)
            .unwrap();
        run
    };
    // Restart — the subscription and occurrence re-fold from the log;
    // `deliver_wakeup` fires the pending occurrence exactly once.
    let mut s2 = reopen(&d, 120_000);
    let rep = s2.restore(&run, "writer-b", 60_000).unwrap();
    let out = s2.deliver_wakeup(&run, &rep.lease, 120_000).unwrap();
    assert_eq!(
        out.iter()
            .filter(|o| matches!(o, FireOutcome::Fired { .. }))
            .count(),
        1,
        "{out:?}"
    );
    assert_eq!(count_class(&s2, &run, "control.wakeup.fired"), 1);
    let drained = s2.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 1, "{drained:?}");
    assert_eq!(drained[0].delivery_mode, DeliveryMode::Steer);
    assert_eq!(drained[0].payload_ref.as_deref(), Some("art-1"));
    // A second pass fires nothing new — the claim is durable.
    let out2 = s2.deliver_wakeup(&run, &rep.lease, 120_100).unwrap();
    assert!(out2.is_empty(), "{out2:?}");
    assert_eq!(count_class(&s2, &run, "control.wakeup.fired"), 1);
}

/// An already-*fired* steer also re-surfaces after restart — `drain`
/// withholds nothing a fired row carries (at-least-once: the caller-side
/// dedup set is process state; the `fired` row is the record).
#[test]
fn r2_6_steer_fired_redelivers_after_restart() {
    let d = dir("steer-fired-restart");
    let (run, sub) = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        let sub = s
            .wakeup_subscribe(
                &run,
                &lease,
                Trigger::Manual {
                    principal: "principal:p".into(),
                },
                steer_policy(),
                &eref(&run),
            )
            .unwrap();
        s.wakeup_occurred(&run, &lease, &sub, "steer:art-7", Some("art-7"), 1_500)
            .unwrap();
        s.wakeup_fire(&run, &lease, &sub, "steer:art-7").unwrap();
        (run, sub)
    };
    let mut s2 = reopen(&d, 120_000);
    let _rep = s2.restore(&run, "writer-b", 60_000).unwrap();
    let drained = s2.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 1, "{drained:?}");
    assert_eq!(drained[0].delivery_mode, DeliveryMode::Steer);
    assert_eq!(drained[0].payload_ref.as_deref(), Some("art-7"));
    assert_eq!(drained[0].subscription_id, sub);
}
