//! S2.3 integration tests — the §5a.3 durable execution & recovery item (C1)
//! and the §5a.2 saga (`R-2.2.2¹`) against the real `Store`.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.3-5  — liveness shortening is safe (dead same-host holder ⇒
//!   early takeover; live/off-host ⇒ `WouldBlock` until `expires_at`;
//!   suspended runs exempt).
//! - AC-R-2.2.3-6  — suspension invariant (`OpenCommittedEffects`; a resumed
//!   suspended run re-emits no effect phase).
//! - AC-R-2.2.3-7  — wakeup idempotence (N ≥ 3 deliveries ⇒ 1 `fired` + N−1
//!   audited skips; KP-11 restart; `run_finished` skip; timers recompute).
//! - AC-R-2.2.3-8  — `follow_up` delivery waits for the committed effect's
//!   terminal (`deliver_after`; `wakeup_drain` withholds).
//! - AC-R-2.2.4-1  — `coherent_fork_points` / `check_fork_point` /
//!   `nearest_coherent_seq` (the S2.9 prerequisite — the projection, not the
//!   branch model).
//! - AC-R-2.2.4-3  — source immutability (the projections append nothing).
//! - R-2.2.2¹      — `compensate_run` (reverse order, `~comp` ids, full
//!   lifecycle, escalation on failure, idempotent re-entry).
//! - R-2.2.3⁰ᵇ     — HLC on continuation/child runs only; causal seeding;
//!   monotonic under clock regression.
//! - R-2.2.3       — `recovery_decision{last_durable_phase, action, cause}`,
//!   restore idempotence, suspended handling, retry timers, scoped leases.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::effect::EffectPhase;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::leases::{holder_spelling, LeaseScope};
use hh_ledger::manifest::{EventRef, LineageLink, RunKind, RunManifest};
use hh_ledger::saga::{compensating_effect_id, CompensationIntent};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::suspend::SuspendReason;
use hh_ledger::wakeup::{Coalesce, DeliveryMode, FireOutcome, OccurOutcome, Trigger, WakeupPolicy};
use hh_ledger::LedgerError;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-durable-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let (s, run, lease, _) = open_at(tag, 1_000);
    (s, run, lease)
}

fn open_at(tag: &str, ms: u64) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(ms);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease, clock)
}

/// Reopen the store over `dir` at `ms` — the process-crash-and-restart
/// fixture (the WAL is the record; process memory is gone).
fn reopen(d: &std::path::Path, ms: u64) -> Store {
    // `SeqIds` re-seeded high — the reopened store's kernel rows must not
    // re-mint ids the WAL already holds (production's TimeIds is
    // clock+tagged; the deterministic fixture starts above the waterline).
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

/// An `action.effect.*` row — audit-grade ⇒ kernel producer + provenance.
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

fn kernel_row(id: &str, class: &str, payload: Json) -> Event {
    eff(id, class, "unused", payload, 0)
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
        0,
    )
}

fn authorized(id: &str, effect_id: &str, rc: Json) -> Event {
    eff(
        id,
        "action.effect.authorized",
        effect_id,
        Json::obj([("effective_risk_class", rc)]),
        0,
    )
}

fn decided(id: &str, effect_id: &str, attempt: i64, decision: &str) -> Event {
    let mut ev = eff(
        id,
        "security.permission.decided",
        effect_id,
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("attempt_no", Json::Int(attempt)),
            ("decision", Json::str(decision)),
        ]),
        0,
    );
    ev.scope.effect_id = Some(effect_id.to_string());
    ev
}

fn committed(id: &str, effect_id: &str, attempt: i64, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.committed",
        effect_id,
        Json::obj([("attempt_no", Json::Int(attempt))]),
        fencing,
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
        ]),
        fencing,
    )
}

/// `intended → authorized → decided{allow} → prepared` in one append
/// (a compensable class's `prepared` carries `compensation_plan_id` —
/// ADR-0032 §1).
fn to_prepared(s: &mut Store, run: &str, lease: &Lease, eid: &str, rc: Json) {
    let compensable = rc.get("reversibility").and_then(Json::as_str) == Some("compensable");
    let mut prep = Json::obj([("idempotency_key", Json::str(format!("key-{eid}")))]);
    if compensable {
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
            eff(&format!("{eid}-p"), "action.effect.prepared", eid, prep, 0),
        ],
    )
    .unwrap();
}

/// `to_prepared` + `committed` — the write-ahead barrier crossed.
fn to_committed(s: &mut Store, run: &str, lease: &Lease, eid: &str, rc: Json, gen: u64) {
    to_prepared(s, run, lease, eid, rc);
    s.append(
        run,
        lease,
        vec![committed(&format!("{eid}-c"), eid, 1, gen)],
    )
    .unwrap();
}

/// `to_committed` + `observed{applied}` — terminal.
fn to_applied(s: &mut Store, run: &str, lease: &Lease, eid: &str, rc: Json) {
    to_committed(s, run, lease, eid, rc, lease.generation);
    s.append(
        run,
        lease,
        vec![observed(
            &format!("{eid}-o"),
            eid,
            1,
            "applied",
            lease.generation,
        )],
    )
    .unwrap();
}

fn events(s: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    s.events(run).unwrap().to_vec()
}

fn count_class(s: &Store, run: &str, class: &str) -> usize {
    events(s, run).iter().filter(|e| e.class == class).count()
}

/// The effect fold by id (the public view — `Store::run` is crate-private).
fn fold(s: &Store, run: &str, effect_id: &str) -> hh_ledger::effect::EffectFold {
    s.effect_folds(run)
        .unwrap()
        .into_iter()
        .find(|(id, _)| id == effect_id)
        .map(|(_, f)| f)
        .unwrap_or_else(|| panic!("no effect fold for {effect_id}"))
}

fn eref(run: &str) -> EventRef {
    EventRef {
        run_id: run.to_string(),
        event_id: "e-0".to_string(),
    }
}

// ── AC-R-2.2.3-5 — liveness shortening ──────────────────────────────────────

#[test]
fn ac_2_2_3_5_all_four_scopes_acquire_and_are_folded() {
    let (mut s, run, lease) = open("scopes");
    for scope in [
        LeaseScope::Effect("eff-1".into()),
        LeaseScope::Resource("workspace".into()),
        LeaseScope::Environment("env-1".into()),
        LeaseScope::Wakeup {
            subscription_id: "wsub-1".into(),
            occurrence_key: "timer:100".into(),
        },
    ] {
        let l = s
            .lease_acquire(&run, &lease, &scope, "holder-a", 60_000)
            .unwrap();
        assert_eq!(l.scope.spelling(), scope.spelling());
        assert_eq!(
            s.scoped_lease(&run, &scope).unwrap().unwrap().lease_id,
            l.lease_id
        );
    }
    // Every acquisition is durable — `lifecycle.lease.acquired{scope}` rows.
    let envs = events(&s, &run);
    let scopes: Vec<String> = envs
        .iter()
        .filter(|e| e.class == "lifecycle.lease.acquired")
        .filter_map(|e| {
            e.payload
                .get("scope")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    for want in [
        "effect:eff-1",
        "resource:workspace",
        "environment:env-1",
        "wakeup:wsub-1:timer:100",
    ] {
        assert!(
            scopes.iter().any(|x| x == want),
            "{want} missing: {scopes:?}"
        );
    }
    // A second acquire while the first is live is `WouldBlock`.
    let err = s
        .lease_acquire(
            &run,
            &lease,
            &LeaseScope::Effect("eff-1".into()),
            "holder-b",
            60_000,
        )
        .unwrap_err();
    assert!(matches!(err, LedgerError::WouldBlock { .. }), "{err:?}");
}

#[test]
fn ac_2_2_3_5_dead_holder_takeover_shortens_live_holder_refuses() {
    let (mut s, run, lease) = open("liveness");
    let scope = LeaseScope::Resource("device:gpu0".into());
    // A holder whose pid is certainly dead (4_000_000_000 exceeds every
    // platform's pid space; `kill -0` fails ⇒ `Dead`).
    let dead = holder_spelling("stale-worker", 4_000_000_000);
    s.lease_acquire(&run, &lease, &scope, &dead, 3_600_000)
        .unwrap();
    // Takeover succeeds long before `expires_at` — liveness shortening.
    let l = s
        .lease_takeover(&run, &lease, &scope, "holder-b", 60_000)
        .unwrap();
    assert_eq!(l.holder, "holder-b");
    // The fence is audited.
    assert!(events(&s, &run)
        .iter()
        .any(|e| e.class == "lifecycle.lease.fenced"
            && e.payload.get("scope").and_then(Json::as_str) == Some("resource:device:gpu0")));

    // A *live* holder (this test process) is `WouldBlock` until expiry.
    let scope2 = LeaseScope::Resource("device:gpu1".into());
    let live = holder_spelling("live-worker", std::process::id());
    s.lease_acquire(&run, &lease, &scope2, &live, 3_600_000)
        .unwrap();
    let err = s
        .lease_takeover(&run, &lease, &scope2, "holder-b", 60_000)
        .unwrap_err();
    assert!(matches!(err, LedgerError::WouldBlock { .. }), "{err:?}");

    // An off-host holder is never probed — `WouldBlock` until expiry.
    let scope3 = LeaseScope::Resource("device:gpu2".into());
    let offhost = "remote;host=not-this-host.invalid;pid=1".to_string();
    s.lease_acquire(&run, &lease, &scope3, &offhost, 3_600_000)
        .unwrap();
    let err = s
        .lease_takeover(&run, &lease, &scope3, "holder-b", 60_000)
        .unwrap_err();
    assert!(matches!(err, LedgerError::WouldBlock { .. }), "{err:?}");
}

#[test]
fn ac_2_2_3_5_suspended_run_is_exempt_from_liveness_takeover() {
    let (mut s, run, lease) = open("susp-exempt");
    let scope = LeaseScope::Environment("env-1".into());
    let dead = holder_spelling("suspended-holder", 4_000_000_000);
    s.lease_acquire(&run, &lease, &scope, &dead, 3_600_000)
        .unwrap();
    s.suspend(
        &run,
        &lease,
        &[SuspendReason::Hibernated],
        &[],
        Json::Null,
        false, // keep the writer lease — the probe runs under this generation
    )
    .unwrap();
    assert!(s.is_suspended(&run).unwrap());
    // The holder probes `Dead`, but the run is suspended — exempt from
    // liveness shortening (ADR-0131 §4): `takeover` refuses `WouldBlock`
    // until `expires_at` even though the holder is provably gone.
    let err = s
        .lease_takeover(&run, &lease, &scope, "holder-c", 60_000)
        .unwrap_err();
    assert!(matches!(err, LedgerError::WouldBlock { .. }), "{err:?}");
}

#[test]
fn ac_2_2_3_5_expired_scope_takes_over_via_acquire() {
    let (mut s, run, lease, clock) = open_at("scope-expiry", 1_000);
    let scope = LeaseScope::Effect("eff-x".into());
    s.lease_acquire(&run, &lease, &scope, "holder-a", 5_000)
        .unwrap();
    clock.advance(6_000); // past expires_at
    let l = s
        .lease_acquire(&run, &lease, &scope, "holder-b", 60_000)
        .unwrap();
    assert_eq!(l.holder, "holder-b");
    assert!(events(&s, &run)
        .iter()
        .any(|e| e.class == "lifecycle.lease.fenced"
            && e.payload.get("reason").and_then(Json::as_str) == Some("expired")));
}

// ── AC-R-2.2.3-6 — suspension invariant ─────────────────────────────────────

#[test]
fn ac_2_2_3_6_suspend_refuses_open_committed_effects() {
    let (mut s, run, lease) = open("suspend-refuse");
    to_prepared(&mut s, &run, &lease, "eff-1", compensable());
    let err = s
        .suspend(
            &run,
            &lease,
            &[SuspendReason::OperatorPause],
            &[],
            Json::Null,
            true,
        )
        .unwrap_err();
    match err {
        LedgerError::OpenCommittedEffects { effect_ids } => {
            assert_eq!(effect_ids, vec!["eff-1".to_string()])
        }
        other => panic!("expected OpenCommittedEffects, got {other:?}"),
    }
    // Same for `committed`.
    s.append(
        &run,
        &lease,
        vec![committed("eff-1-c", "eff-1", 1, lease.generation)],
    )
    .unwrap();
    assert!(matches!(
        s.suspend(
            &run,
            &lease,
            &[SuspendReason::OperatorPause],
            &[],
            Json::Null,
            true
        ),
        Err(LedgerError::OpenCommittedEffects { .. })
    ));
    // `intended` alone does not block suspension (S-1 names
    // prepared/deferred/committed only).
    s.append(
        &run,
        &lease,
        vec![intended("eff-2-i", "eff-2", irreversible())],
    )
    .unwrap();
    // Terminate eff-1 so the run is clean, then suspend lands.
    s.append(
        &run,
        &lease,
        vec![observed("eff-1-o", "eff-1", 1, "applied", lease.generation)],
    )
    .unwrap();
    let out = s
        .suspend(
            &run,
            &lease,
            &[SuspendReason::OperatorPause],
            &[],
            Json::Null,
            true,
        )
        .unwrap();
    assert!(out.released_lease);
    assert!(s.is_suspended(&run).unwrap());
    assert!(count_class(&s, &run, "lifecycle.run.suspended") == 1);
}

#[test]
fn ac_2_2_3_6_resumed_suspended_run_reemits_no_effect_phase() {
    let d = dir("suspend-resume");
    let (run, pre_head) = {
        let (mut s, run, lease, _) = {
            let clock = ManualClock::at(1_000);
            let mut s = Store::open_with(
                d.clone(),
                Box::new(clock.clone()),
                Some(Box::new(SeqIds::new())),
                DEFAULT_BLOB_MAX_BYTES,
            )
            .unwrap();
            let (run, lease) = s
                .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
                .unwrap();
            (s, run, lease, clock)
        };
        // An `intended`-only effect — a resume must not re-emit it.
        s.append(
            &run,
            &lease,
            vec![intended("eff-1-i", "eff-1", irreversible())],
        )
        .unwrap();
        s.suspend(
            &run,
            &lease,
            &[SuspendReason::AwaitingTimer { at: "t+60s".into() }],
            &[],
            Json::Null,
            true,
        )
        .unwrap();
        let pre = s.head(&run).unwrap().seq;
        (run, pre)
    };
    // Crash + restart: `restore` ends the suspension.
    let mut s2 = reopen(&d, 120_000);
    let rep = s2
        .restore_caused(&run, "writer-b", 60_000, "operator")
        .unwrap();
    assert!(!s2.is_suspended(&run).unwrap());
    // `was_suspended` surfaces in the audited recovery_decision.
    let envs = events(&s2, &run);
    let resumed = envs
        .iter()
        .find(|e| e.class == "lifecycle.run.resumed")
        .unwrap();
    let rd = resumed.payload.get("recovery_decision").unwrap();
    assert_eq!(rd.get("was_suspended"), Some(&Json::Bool(true)));
    // No effect-phase row after `intended` was re-emitted for eff-1.
    let eff_rows: Vec<&String> = envs
        .iter()
        .filter(|e| e.seq > pre_head && e.class.starts_with("action.effect."))
        .map(|e| &e.class)
        .collect();
    assert!(eff_rows.is_empty(), "re-emitted phases: {eff_rows:?}");
    let _ = rep;
}

// ── AC-R-2.2.3-7 — wakeup idempotence ───────────────────────────────────────

#[test]
fn ac_2_2_3_7_n_deliveries_one_fire_rest_skipped() {
    let (mut s, run, lease) = open("wakeup-idem");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 2_000 },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();
    // One occurrence delivered N=3 times through "two racing services" — the
    // duplicate occurrences are audited skips, the fires claim once.
    let o1 = s
        .wakeup_occurred(&run, &lease, &sub, "timer:2000", None, 2_000)
        .unwrap();
    assert!(matches!(o1, OccurOutcome::Occurred(_)));
    let o2 = s
        .wakeup_occurred(&run, &lease, &sub, "timer:2000", None, 2_000)
        .unwrap();
    assert!(matches!(o2, OccurOutcome::Skipped(_)));
    let o3 = s
        .wakeup_occurred(&run, &lease, &sub, "timer:2000", None, 2_000)
        .unwrap();
    assert!(matches!(o3, OccurOutcome::Skipped(_)));

    let f1 = s.wakeup_fire(&run, &lease, &sub, "timer:2000").unwrap();
    assert!(matches!(f1, FireOutcome::Fired { .. }), "{f1:?}");
    // Racing deliveries — both skip `already_claimed`, audited.
    for _ in 0..2 {
        match s.wakeup_fire(&run, &lease, &sub, "timer:2000").unwrap() {
            FireOutcome::Skipped { reason, .. } => assert_eq!(reason, "already_claimed"),
            other => panic!("expected already_claimed skip, got {other:?}"),
        }
    }
    let fired = count_class(&s, &run, "control.wakeup.fired");
    assert_eq!(fired, 1);
    // 2 duplicate_occurrence + 2 already_claimed skips, all durable.
    let skips: Vec<String> = events(&s, &run)
        .iter()
        .filter(|e| e.class == "control.wakeup.skipped")
        .filter_map(|e| {
            e.payload
                .get("reason")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    assert_eq!(
        skips,
        vec![
            "duplicate_occurrence",
            "duplicate_occurrence",
            "already_claimed",
            "already_claimed"
        ]
    );
}

#[test]
fn ac_2_2_3_7_kp11_occurred_before_fired_survives_restart() {
    let d = dir("kp11");
    let (run, _sub) = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock.clone()),
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
                Trigger::Timer { at_ms: 2_000 },
                WakeupPolicy::default_policy(),
                &eref(&run),
            )
            .unwrap();
        // Kill-point 11: `occurred` durable, process dies before `fired`.
        s.wakeup_occurred(&run, &lease, &sub, "timer:2000", None, 2_000)
            .unwrap();
        (run, sub)
    };
    // Restart — `deliver_wakeup` rediscovers the pending occurrence and fires
    // it exactly once (idempotent: the second pass fires nothing new).
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
    // A second `deliver_wakeup` — no new fires.
    let out2 = s2.deliver_wakeup(&run, &rep.lease, 120_100).unwrap();
    assert!(out2.is_empty(), "{out2:?}");
    assert_eq!(count_class(&s2, &run, "control.wakeup.fired"), 1);
}

#[test]
fn ac_2_2_3_7_timer_fires_after_restart_within_ttl_plus_grace() {
    let d = dir("timer-restart");
    let run = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock.clone()),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        s.wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 5_000 },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();
        // Process dies at t=1_000 with the timer armed but unfired.
        run
    };
    // Restart past the deadline — `due` recomputes from the durable
    // subscription (no process memory survived).
    let mut s2 = reopen(&d, 120_000);
    let rep = s2.restore(&run, "writer-b", 60_000).unwrap();
    let due = s2.wakeup_due(&run, 120_000).unwrap();
    assert_eq!(due.len(), 1, "{due:?}");
    let out = s2.deliver_wakeup(&run, &rep.lease, 120_000).unwrap();
    assert!(out.iter().any(|o| matches!(o, FireOutcome::Fired { .. })));
}

#[test]
fn ac_2_2_3_7_occurrence_for_finished_run_is_audited_skip() {
    let (mut s, run, lease) = open("wakeup-finished");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 100 },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();
    s.wakeup_occurred(&run, &lease, &sub, "timer:100", None, 200)
        .unwrap();
    s.append(
        &run,
        &lease,
        vec![kernel_row(
            "fin-1",
            "lifecycle.run.finished",
            Json::obj([("stop_reason", Json::str("completed"))]),
        )],
    )
    .unwrap();
    match s.wakeup_fire(&run, &lease, &sub, "timer:100").unwrap() {
        FireOutcome::Skipped { reason, .. } => assert_eq!(reason, "run_finished"),
        other => panic!("expected run_finished skip, got {other:?}"),
    }
}

#[test]
fn ac_2_2_3_7_retry_due_derives_from_scheduled_rows() {
    let (mut s, run, lease) = open("retry-due");
    s.append(
        &run,
        &lease,
        vec![kernel_row(
            "rs-1",
            "control.retry.scheduled",
            Json::obj([
                ("scope_id", Json::str("eff-9")),
                ("kind", Json::str("effect")),
                ("not_before", Json::Int(5_000)),
                ("attempt_no", Json::Int(2)),
            ]),
        )],
    )
    .unwrap();
    assert!(s.retry_due(&run, 4_999).unwrap().is_empty());
    let due = s.retry_due(&run, 5_000).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].scope_id, "eff-9");
    assert_eq!(due[0].attempt_no, Some(2));
    // A `fired` row consumes the timer — the fold is the record.
    s.append(
        &run,
        &lease,
        vec![kernel_row(
            "rf-1",
            "control.retry.fired",
            Json::obj([("schedule_event_id", Json::str("rs-1"))]),
        )],
    )
    .unwrap();
    assert!(s.retry_due(&run, 9_999).unwrap().is_empty());
}

#[test]
fn ac_2_2_3_7_coalesce_latest_skips_older_pending() {
    let (mut s, run, lease) = open("wakeup-coalesce");
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 100 },
            WakeupPolicy {
                coalesce: Coalesce::Latest,
                ..WakeupPolicy::default_policy()
            },
            &eref(&run),
        )
        .unwrap();
    for key in ["k1", "k2", "k3"] {
        s.wakeup_occurred(&run, &lease, &sub, key, None, 200)
            .unwrap();
    }
    s.wakeup_fire(&run, &lease, &sub, "k3").unwrap();
    let skips: Vec<String> = events(&s, &run)
        .iter()
        .filter(|e| e.class == "control.wakeup.skipped")
        .filter_map(|e| {
            e.payload
                .get("occurrence_key")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    assert!(skips.contains(&"k1".to_string()) && skips.contains(&"k2".to_string()));
    assert_eq!(count_class(&s, &run, "control.wakeup.fired"), 1);
}

#[test]
fn ac_2_2_3_7_stage4_triggers_refuse_typed() {
    let (mut s, run, lease) = open("wakeup-stage4");
    let err = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Schedule {
                expr: "* * * * *".into(),
            },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap_err();
    assert!(
        matches!(err, LedgerError::TriggerUnsupported { .. }),
        "{err:?}"
    );
    // `steer` delivery is refused at subscribe (OQ-316 — follow_up only).
    let err = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 100 },
            WakeupPolicy {
                delivery_mode: DeliveryMode::Steer,
                ..WakeupPolicy::default_policy()
            },
            &eref(&run),
        )
        .unwrap_err();
    assert!(
        matches!(err, LedgerError::WakeupPolicyUnsupported { .. }),
        "{err:?}"
    );
}

#[test]
fn ac_2_2_3_7_child_terminal_trigger_fires_on_child_finish() {
    let (mut s, run, lease) = open("wakeup-child");
    // The child run exists in this store.
    let mut child_manifest = RunManifest::minimal(RunKind::Agent);
    child_manifest.parent_run_id = Some(run.clone());
    let (child, child_lease) = s.open_run(child_manifest, "writer-a").unwrap();
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::ChildTerminal {
                child_run_id: child.clone(),
            },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();
    // Not due while the child runs.
    assert!(s.wakeup_due(&run, 9_999).unwrap().is_empty());
    // Finish the child (under its own writer lease from open_run).
    s.append(
        &child,
        &child_lease,
        vec![kernel_row(
            "cfin-1",
            "lifecycle.run.finished",
            Json::obj([("stop_reason", Json::str("completed"))]),
        )],
    )
    .unwrap();
    let due = s.wakeup_due(&run, 9_999).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].0, sub);
    let out = s.deliver_wakeup(&run, &lease, 9_999).unwrap();
    assert!(out.iter().any(|o| matches!(o, FireOutcome::Fired { .. })));
}

// ── AC-R-2.2.3-8 — delivery at decision points ──────────────────────────────

#[test]
fn ac_2_2_3_8_follow_up_waits_for_committed_effects_terminal() {
    let (mut s, run, lease) = open("wakeup-deliver-after");
    to_committed(
        &mut s,
        &run,
        &lease,
        "eff-1",
        compensable(),
        lease.generation,
    );
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 100 },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();
    s.wakeup_occurred(&run, &lease, &sub, "timer:100", None, 200)
        .unwrap();
    // The fire lands (the claim is the audit) but `deliver_after` names the
    // open committed effect — `wakeup_drain` withholds the delivery.
    match s.wakeup_fire(&run, &lease, &sub, "timer:100").unwrap() {
        FireOutcome::Fired { deliver_after, .. } => {
            assert_eq!(deliver_after.as_deref(), Some("eff-1"))
        }
        other => panic!("expected Fired, got {other:?}"),
    }
    assert!(s.wakeup_drain(&run).unwrap().is_empty());
    // The effect reaches its terminal — the delivery unblocks.
    s.append(
        &run,
        &lease,
        vec![observed("eff-1-o", "eff-1", 1, "applied", lease.generation)],
    )
    .unwrap();
    let drain = s.wakeup_drain(&run).unwrap();
    assert_eq!(drain.len(), 1);
    assert_eq!(drain[0].occurrence_key, "timer:100");
}

// ── AC-R-2.2.4-1/-3 — coherent fork points (the S2.9 prerequisite) ──────────

#[test]
fn ac_2_2_4_1_mid_scope_cut_is_refused_naming_open_scopes() {
    let (mut s, run, lease) = open("fork-coherence");
    // turn ⊃ model_call open, plus an `intended` effect — the cut inside
    // those scopes is incoherent.
    s.append(
        &run,
        &lease,
        vec![
            {
                let mut t = kernel_row("t1", "lifecycle.turn.started", Json::obj([]));
                t.scope.turn_id = Some("turn-1".into());
                t
            },
            {
                let mut c = kernel_row("mc1", "model.call.requested", Json::obj([]));
                c.scope = Scope {
                    turn_id: Some("turn-1".into()),
                    model_call_id: Some("mc-1".into()),
                    ..Scope::default()
                };
                c
            },
            intended("eff-1-i", "eff-1", irreversible()),
        ],
    )
    .unwrap();
    let head = s.head(&run).unwrap().seq;
    match s.check_fork_point(&run, head).unwrap_err() {
        LedgerError::ForkPointNotCoherent { open_scopes, .. } => {
            assert!(!open_scopes.is_empty());
        }
        other => panic!("expected ForkPointNotCoherent, got {other:?}"),
    }
    // `nearest_coherent_seq` backs off to before the open scopes.
    let nearest = s.nearest_coherent_seq(&run, head).unwrap().unwrap();
    assert!(nearest < head);
    s.check_fork_point(&run, nearest).unwrap();
}

#[test]
fn ac_2_2_4_1_coherent_points_after_terminal_scopes() {
    let (mut s, run, lease) = open("fork-coherent");
    to_applied(&mut s, &run, &lease, "eff-1", compensable());
    let head = s.head(&run).unwrap().seq;
    // A terminal fold is coherent at the head.
    s.check_fork_point(&run, head).unwrap();
    let points = s.coherent_fork_points(&run, None, None).unwrap();
    assert!(points.contains(&0), "seq 0 is always coherent");
    assert!(points.contains(&head), "head is coherent: {points:?}");
    // Mid-lifecycle seqs are incoherent — the projection names them absent.
    let intended_seq = events(&s, &run)
        .iter()
        .find(|e| e.class == "action.effect.intended")
        .unwrap()
        .seq;
    assert!(!points.contains(&(intended_seq as u64)));
}

#[test]
fn ac_2_2_4_3_fork_projections_append_nothing() {
    let (mut s, run, lease) = open("fork-immutable");
    to_committed(
        &mut s,
        &run,
        &lease,
        "eff-1",
        irreversible(),
        lease.generation,
    );
    let head_before = s.head(&run).unwrap().hash.clone();
    let len_before = events(&s, &run).len();
    let _ = s.coherent_fork_points(&run, None, None).unwrap();
    let _ = s.check_fork_point(&run, 0);
    let _ = s.nearest_coherent_seq(&run, 3);
    assert_eq!(s.head(&run).unwrap().hash, head_before);
    assert_eq!(events(&s, &run).len(), len_before);
}

// ── R-2.2.2¹ — the saga ──────────────────────────────────────────────────────

#[test]
fn r_2_2_2_1_compensate_run_reverse_order_full_lifecycle() {
    let (mut s, run, lease) = open("saga");
    to_applied(&mut s, &run, &lease, "eff-a", compensable());
    to_applied(&mut s, &run, &lease, "eff-b", compensable());
    let mut dispatched: Vec<String> = Vec::new();
    let report = s
        .compensate_run(&run, &lease, &mut |i: &CompensationIntent| {
            dispatched.push(i.original_effect_id.clone());
            Ok(Json::obj([]))
        })
        .unwrap();
    // Reverse commit order: eff-b (committed last) compensates first.
    assert_eq!(dispatched, vec!["eff-b".to_string(), "eff-a".to_string()]);
    assert_eq!(
        report.compensated,
        vec!["eff-b".to_string(), "eff-a".to_string()]
    );
    // The compensating ids derive as `<original>~comp`.
    assert_eq!(compensating_effect_id("eff-a"), "eff-a~comp");
    for orig in ["eff-a", "eff-b"] {
        let comp = compensating_effect_id(orig);
        assert_eq!(
            fold(&s, &run, &comp).phase,
            EffectPhase::Observed,
            "{comp} not observed"
        );
        assert_eq!(
            fold(&s, &run, orig).phase,
            EffectPhase::Compensated,
            "{orig} not compensated"
        );
        // The full compensator lifecycle is durable.
        for phase in [
            "intended",
            "authorized",
            "prepared",
            "committed",
            "observed",
        ] {
            let class = format!("action.effect.{phase}");
            assert!(
                events(&s, &run).iter().any(
                    |e| e.class == class && e.scope.effect_id.as_deref() == Some(comp.as_str())
                ),
                "{class} for {comp} missing"
            );
        }
    }
}

#[test]
fn r_2_2_2_1_failed_compensation_escalates_and_abandons() {
    let (mut s, run, lease) = open("saga-fail");
    to_applied(&mut s, &run, &lease, "eff-a", compensable());
    let report = s
        .compensate_run(&run, &lease, &mut |_| Err("compensator blew up".into()))
        .unwrap();
    assert_eq!(report.abandoned, vec!["eff-a".to_string()]);
    assert_eq!(report.escalations.len(), 1);
    let comp = compensating_effect_id("eff-a");
    assert_eq!(fold(&s, &run, &comp).phase, EffectPhase::Abandoned);
    // The escalation precedes the abandoned row and is named by it.
    let envs = events(&s, &run);
    let esc = envs
        .iter()
        .position(|e| e.class == "lifecycle.escalation.raised")
        .unwrap();
    let ab = envs
        .iter()
        .position(|e| {
            e.class == "action.effect.abandoned"
                && e.scope.effect_id.as_deref() == Some(comp.as_str())
        })
        .unwrap();
    assert!(esc < ab);
    // Re-running the saga does not re-dispatch the abandoned compensator.
    let mut n = 0;
    let report2 = s
        .compensate_run(&run, &lease, &mut |_| {
            n += 1;
            Ok(Json::Null)
        })
        .unwrap();
    assert_eq!(n, 0, "abandoned compensator re-dispatched");
    assert!(report2.compensated.is_empty());
}

#[test]
fn r_2_2_2_1_saga_idempotent_across_partial_completion() {
    let d = dir("saga-restart");
    let run = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            d.clone(),
            Box::new(clock.clone()),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        to_applied(&mut s, &run, &lease, "eff-a", compensable());
        to_applied(&mut s, &run, &lease, "eff-b", compensable());
        // The saga dispatches eff-b's compensator, then the process "dies"
        // mid-run (a panic through the dispatch seam) — eff-a is still
        // uncompensated. The panicking store is dropped; the WAL holds.
        let run2 = run.clone();
        let mut sp = std::panic::AssertUnwindSafe(&mut s);
        let _ = std::panic::catch_unwind(move || {
            let _ = sp.compensate_run(&run2, &lease, &mut |i: &CompensationIntent| {
                if i.original_effect_id == "eff-a" {
                    panic!("simulated mid-saga crash");
                }
                Ok(Json::Null)
            });
        });
        run
    };
    let mut s2 = reopen(&d, 120_000);
    let rep = s2.restore(&run, "writer-b", 60_000).unwrap();
    // The crash left `eff-a~comp` mid-lifecycle (committed, pre-dispatch) —
    // restore's table fenced it `unknown{worker_lost}` and the saga reports
    // it `in_flight` (the probe/retry path owns its completion — it is
    // never re-dispatched here, so no second dispatch of the same act).
    let mut dispatched = Vec::new();
    let report = s2
        .compensate_run(&run, &rep.lease, &mut |i: &CompensationIntent| {
            dispatched.push(i.original_effect_id.clone());
            Ok(Json::Null)
        })
        .unwrap();
    assert!(dispatched.is_empty(), "re-dispatched: {dispatched:?}");
    assert_eq!(report.in_flight, vec!["eff-a".to_string()]);
    // eff-b's compensator was `observed` before the crash — exactly one
    // applied compensator row for it after the restart (no duplicate).
    let comp_b = compensating_effect_id("eff-b");
    let observed_b = events(&s2, &run)
        .iter()
        .filter(|e| {
            e.class == "action.effect.observed"
                && e.scope.effect_id.as_deref() == Some(comp_b.as_str())
        })
        .count();
    assert_eq!(observed_b, 1);
}

// ── R-2.2.3⁰ᵇ — HLC on continuation/child runs ──────────────────────────────

#[test]
fn r_2_2_3_0b_hlc_absent_on_plain_run_present_on_continuation() {
    let (mut s, run, lease) = open("hlc");
    s.append(
        &run,
        &lease,
        vec![kernel_row("e1", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    // Plain run — every envelope's `hlc` member stays absent.
    assert!(events(&s, &run).iter().all(|e| e.hlc.is_none()));
    let head = s.head(&run).unwrap();
    // A continuation run binds the lineage link — its envelopes carry `hlc`.
    let mut cont = RunManifest::minimal(RunKind::Agent);
    cont.continued_from = Some(LineageLink {
        run_id: run.clone(),
        at_seq: head.seq,
        head_hash: head.hash.clone(),
    });
    cont.activation_no = 2;
    let (cont_run, cont_lease) = s.open_run(cont, "writer-a").unwrap();
    s.append(
        &cont_run,
        &cont_lease,
        vec![kernel_row("ce1", "lifecycle.turn.started", Json::obj([]))],
    )
    .unwrap();
    let envs = events(&s, &cont_run);
    assert!(
        envs.iter().all(|e| e.hlc.is_some()),
        "hlc missing on continuation"
    );
    // The rendered stamps parse back (canonical round-trip).
    for e in &envs {
        let stamp = e.hlc.as_deref().unwrap();
        assert!(hh_ledger::hlc::Hlc::parse(stamp).is_some(), "{stamp}");
    }
}

#[test]
fn r_2_2_3_0b_hlc_monotonic_under_clock_regression() {
    let d = dir("hlc-mono");
    let (run, cont_run, cont_lease) = {
        let mut s = Store::open_with(
            d.clone(),
            Box::new(ManualClock::at(10_000)),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, _lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        let head = s.head(&run).unwrap();
        let mut cont = RunManifest::minimal(RunKind::Agent);
        cont.continued_from = Some(LineageLink {
            run_id: run.clone(),
            at_seq: head.seq,
            head_hash: head.hash.clone(),
        });
        let (cont_run, cont_lease) = s.open_run(cont, "writer-a").unwrap();
        (run, cont_run, cont_lease)
    };
    // Reopen with the clock regressed far behind the continuation's seed —
    // the HLC still ticks monotonically (never backward).
    let mut s2 = reopen(&d, 1);
    s2.append(
        &cont_run,
        &cont_lease,
        vec![
            kernel_row("c1", "lifecycle.turn.started", Json::obj([])),
            kernel_row("c2", "control.decision", Json::obj([])),
        ],
    )
    .unwrap();
    let hlcs: Vec<String> = events(&s2, &cont_run)
        .iter()
        .filter_map(|e| e.hlc.clone())
        .collect();
    assert!(hlcs.len() >= 3);
    for w in hlcs.windows(2) {
        assert!(w[0] < w[1], "non-monotonic hlc: {} !< {}", w[0], w[1]);
    }
    let _ = run;
}

// ── R-2.2.3 — recovery_decision + restore idempotence ───────────────────────

#[test]
fn r_2_2_3_recovery_decision_payload_and_cause() {
    let (mut s, run, lease) = open("recovery-decision");
    to_committed(
        &mut s,
        &run,
        &lease,
        "eff-1",
        irreversible(),
        lease.generation,
    );
    // The writer "dies" (the lease is still live on disk — restore's
    // takeover fences it... actually a live lease WouldBlocks; release to
    // simulate the dead-holder release path, then restore).
    s.release(&lease, "test-simulated-crash").unwrap();
    let rep = s.restore_caused(&run, "writer-b", 60_000, "crash").unwrap();
    let envs = events(&s, &run);
    let resumed = envs
        .iter()
        .find(|e| e.event_id == rep.resumed_event_id)
        .unwrap();
    let rd = resumed.payload.get("recovery_decision").unwrap();
    assert!(rd
        .get("last_durable_phase")
        .and_then(Json::as_str)
        .is_some());
    // The committed non-read_only effect → `unknown{worker_lost}` ⇒
    // `resolve_unknowns`.
    assert_eq!(
        rd.get("action").and_then(Json::as_str),
        Some("resolve_unknowns")
    );
    assert_eq!(rd.get("cause").and_then(Json::as_str), Some("crash"));
    assert!(rep.unknowned.contains(&"eff-1".to_string()));
    // Exactly one `resumed` row for this restore.
    assert_eq!(count_class(&s, &run, "lifecycle.run.resumed"), 1);
}

#[test]
fn r_2_2_3_second_restore_is_idempotent_no_double_unknown() {
    let (mut s, run, lease) = open("restore-idem");
    to_committed(
        &mut s,
        &run,
        &lease,
        "eff-1",
        irreversible(),
        lease.generation,
    );
    s.release(&lease, "crash-1").unwrap();
    let r1 = s.restore(&run, "writer-b", 60_000).unwrap();
    // A second restore over the same ledger marks nothing twice.
    s.release(&r1.lease, "crash-2").unwrap();
    let r2 = s.restore(&run, "writer-c", 60_000).unwrap();
    assert!(r2.unknowned.is_empty(), "double-marked: {:?}", r2.unknowned);
    let unknowns = events(&s, &run)
        .iter()
        .filter(|e| {
            e.class == "action.effect.unknown" && e.scope.effect_id.as_deref() == Some("eff-1")
        })
        .count();
    assert_eq!(unknowns, 1);
    assert_eq!(count_class(&s, &run, "lifecycle.run.resumed"), 2);
}

#[test]
fn r_2_2_3_pending_permissions_surface_in_restore_report() {
    let (mut s, run, lease) = open("restore-pending");
    let mut p = kernel_row(
        "pp-1",
        "security.permission.pending",
        Json::obj([
            ("permission_id", Json::str("perm-1")),
            (
                "request",
                Json::obj([("reason", Json::str("needs approval"))]),
            ),
        ]),
    );
    p.scope.effect_id = None;
    s.append(&run, &lease, vec![p]).unwrap();
    s.release(&lease, "crash").unwrap();
    let rep = s.restore(&run, "writer-b", 60_000).unwrap();
    assert_eq!(rep.pending_permissions, vec!["perm-1".to_string()]);
}

// ── removability(0) — the C1 surface absent ⇒ typed refusal ────────────
// Built only under `--no-default-features` (the check-removability leg):
// every C1 op answers `UnsupportedTier{tier:"c1"}`, never a silent
// degrade (CC6).
#[cfg(not(feature = "tier-c1"))]
#[test]
fn removability0_c1_ops_refuse_typed() {
    let (mut s, run, lease) = open("rem0");
    let attempts: Vec<Result<(), LedgerError>> = vec![
        s.suspend(&run, &lease, &[], &[], Json::Null, true)
            .map(|_| ()),
        s.wakeup_subscribe(
            &run,
            &lease,
            Trigger::Timer { at_ms: 0 },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .map(|_| ()),
        s.lease_acquire(
            &run,
            &lease,
            &LeaseScope::Effect("e-1".into()),
            "holder-a",
            60_000,
        )
        .map(|_| ()),
        s.compensate_run(&run, &lease, &mut |_| Ok(Json::Null))
            .map(|_| ()),
    ];
    for r in attempts {
        match r {
            Err(LedgerError::UnsupportedTier { tier, .. }) => assert_eq!(tier, "c1"),
            other => panic!("expected UnsupportedTier, got {other:?}"),
        }
    }
}
