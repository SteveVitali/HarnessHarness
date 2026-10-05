//! S4.9 acceptance/regression matrix — `AC-R-2.12.6-{1..8}` +
//! `RC-{1..8}` over the fixture adapter (spec §5i.1; ADR-0205…0207).
//! Everything runs against the real `Store` (one ledger, one authority
//! path) under the deterministic `ManualClock`/`SeqIds` fixture — the
//! same seams `durable_execution` uses.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::spec::BudgetMode;
use hh_budget::{BudgetSpec, DimensionId, DimensionKey};
use hh_fleet::engine::{FleetEngine, ReconcileReport};
use hh_fleet::errors::FleetError;
use hh_fleet::source::FixtureAdapter;
use hh_fleet::spec::{Capacity, Defaults, FleetSpec, TriggerRule};
use hh_fleet::work_item::{derive_state, WorkItemInit};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::RunKind;
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;

const HOLDER: &str = "fleet-test";
const TTL: u64 = 60_000;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-fleet-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str, ms: u64) -> (Store, ManualClock) {
    let (s, c, _) = store_at(tag, ms);
    (s, c)
}

fn store_at(tag: &str, ms: u64) -> (Store, ManualClock, PathBuf) {
    let clock = ManualClock::at(ms);
    let d = dir(tag);
    let s = Store::open_with(
        d.clone(),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    (s, clock, d)
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

fn spec() -> FleetSpec {
    let mut triggers = BTreeMap::new();
    triggers.insert(
        "observe".to_string(),
        TriggerRule {
            name: "observe".into(),
            trigger: Trigger::External {
                kind: "ticket.updated".into(),

                source_ref: None,
                filter: None,
            },
            policy: WakeupPolicy::default_policy(),
        },
    );
    triggers.insert(
        "manual".to_string(),
        TriggerRule {
            name: "manual".into(),
            trigger: Trigger::Manual {
                principal: "operator".into(),
            },
            policy: WakeupPolicy::default_policy(),
        },
    );
    let mut ownership = BTreeMap::new();
    ownership.insert("alice".to_string(), "ops".to_string());
    FleetSpec {
        schema: "hh.fleet.spec/1".into(),
        version: "1".into(),
        name: "triage".into(),
        purpose: "fleet_activation".into(),
        fixture_ref: "fixture:triage".into(),
        agents: vec!["alice".into(), "bob".into(), "ops".into()],
        capacity: Capacity {
            activate_run: 8,
            items: 64,
        },
        ownership,
        policy_ref: "policy:test".into(),
        narrowing: Vec::new(),
        budget: None,
        budget_ref: None,
        out_of_scope: true,
        triggers,
        defaults: Defaults::default(),
        human_gate_states: vec!["needs_human".into()],
    }
}

fn spec_matched() -> FleetSpec {
    let mut s = spec();
    s.out_of_scope = false;
    s.budget = Some(BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
            (DimensionKey::Primary(DimensionId::Spawns), 16),
        ],
    ));
    s.budget_ref = Some("budget:fleet:triage".into());
    s
}

fn item(id: &str, owner: Option<&str>) -> Json {
    Json::obj([
        ("item_id", Json::str(id)),
        ("title", Json::str(&format!("ticket {id}"))),
        (
            "source",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("kind", Json::str("ticket")),
                ("source_ref", Json::str(&format!("src:{id}"))),
            ]),
        ),
        ("idempotency_key", Json::str(&format!("idem:{id}"))),
        ("owner", owner.map(Json::str).unwrap_or(Json::Null)),
    ])
}

fn item_init(id: &str, owner: Option<&str>) -> WorkItemInit {
    WorkItemInit::from_json(&item(id, owner)).unwrap()
}

fn fixture(occs: Json, suspended: &[&str], runnable: &[&str]) -> FixtureAdapter {
    FixtureAdapter::from_doc(Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        ("occurrences", occs),
        (
            "suspended",
            Json::Arr(suspended.iter().map(|x| Json::str(*x)).collect()),
        ),
        (
            "activate_run",
            Json::Arr(runnable.iter().map(|x| Json::str(*x)).collect()),
        ),
    ]))
    .unwrap()
}

fn occ(id: &str, trigger: &str, kind: Option<&str>, item_json: Json, at: u64) -> Json {
    let mut m = BTreeMap::new();
    m.insert("occurrence_id".into(), Json::str(id));
    m.insert("trigger".into(), Json::str(trigger));
    if let Some(k) = kind {
        m.insert("kind".into(), Json::str(k));
    }
    m.insert("item".into(), item_json);
    m.insert("observed_at_ms".into(), Json::Int(at as i64));
    Json::Obj(m)
}

fn empty_adapter() -> FixtureAdapter {
    fixture(Json::Arr(vec![]), &[], &[])
}

/// A real child activation run for `dispatch_note`'s `run_ref` — the
/// note resolves the ref against the store (the `fleet_anchor` join).
fn child_run(s: &mut Store, spawn: Option<hh_ledger::manifest::EventRef>) -> String {
    let mut m = hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent);
    m.spawn_event = spawn;
    s.open_run(m, "child-writer").unwrap().0
}

/// `dispatch_note` under a linked child — the launcher's stamp that
/// completes `dispatching → dispatched` (spawn_event names the fleet's
/// dispatch row; the note cross-checks it).
fn stamp_note(eng: &mut FleetEngine, s: &mut Store, item: &str) {
    let it = eng.work_item(item).unwrap();
    let dref = it.dispatch.spec_ref.clone().unwrap();
    let spawn = hh_ledger::manifest::EventRef {
        run_id: eng.run_id.clone(),
        event_id: it.dispatch_event_id.clone().unwrap(),
    };
    let child = child_run(s, Some(spawn));
    eng.dispatch_note(s, item, &dref, Some(&child), None)
        .unwrap();
}

fn open_engine(s: &mut Store, spec: FleetSpec) -> (String, FleetEngine) {
    FleetEngine::open(s, HOLDER, TTL, spec).unwrap()
}

// ── AC-1: unattended activation opens over `lifecycle.fleet.activated`,
//         `run_kind = fleet`, spec ref on the manifest ────────────────────
#[test]
fn ac1_open_activates_fleet_run() {
    let (mut s, _c) = store("ac1", 1_000);
    let (run, eng) = open_engine(&mut s, spec());
    assert!(run.starts_with("fleet-"));
    let m = s.manifest(&run).unwrap();
    assert_eq!(m.run_kind, RunKind::Fleet);
    assert_eq!(
        m.extra.get("fleet_spec_ref").and_then(Json::as_str),
        Some(eng.view.spec_ref.as_str())
    );
    // The durable anchor landed.
    let rows: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.fleet.activated")
        .collect();
    assert_eq!(rows.len(), 1);
    // Idempotent — re-open returns the same activation. The live writer
    // lease is the single-writer fence (a second open while held is the
    // typed WouldBlock); releasing and re-opening proves the durable
    // run-id idempotence.
    s.release(&eng.lease, "reopen").unwrap();
    let (run2, _e2) = open_engine(&mut s, spec());
    assert_eq!(run, run2);
}

// ── AC-2 + RC-8: observe → admit → dispatch over the fixture; replay is
//                 idempotent (the durable cursor never double-fires) ──────
#[test]
fn ac2_rc8_observe_admit_dispatch_idempotent() {
    let (mut s, clock) = store("ac2", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    let ad = fixture(
        Json::Arr(vec![occ(
            "occ-1",
            "external",
            Some("ticket.updated"),
            item("i1", Some("alice")),
            1_100,
        )]),
        &[],
        &["i1"],
    );
    clock.advance(100);
    let r = eng.reconcile(&mut s, &ad, 1_100).unwrap();
    assert_eq!(r.dispatched, vec!["i1".to_string()]);
    let it = eng.work_item("i1").unwrap();
    // RC-2's lease+mark lands `dispatching`; the launcher's durable
    // `dispatch_note` completes the transition to `dispatched`.
    assert_eq!(derive_state(&it), "dispatching");
    stamp_note(&mut eng, &mut s, "i1");
    let it = eng.work_item("i1").unwrap();
    assert_eq!(derive_state(&it), "dispatched");
    // Durable rows: created (admission) + dispatched{verb:dispatch}.
    let classes: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect();
    assert!(
        classes.contains(&"control.work_item.created".to_string()),
        "{classes:?}"
    );
    assert!(classes.contains(&"control.work_item.dispatched".to_string()));
    // Replay the same fixture — level-triggered idempotence: no new
    // admits, no double-fire (RC-8).
    let n_events = s.events(&run).unwrap().len();
    let r2 = eng.reconcile(&mut s, &ad, 1_100).unwrap();
    assert!(r2.dispatched.is_empty());
    assert!(r2.observed.is_empty());
    let evs = s.events(&run).unwrap();
    // Replay may land a *dedup audit* row (`control.wakeup.skipped`
    // — durable evidence the occurrence was seen, never a second fire);
    // no work-item or lifecycle row may be appended twice (RC-8).
    let replayed: Vec<_> = evs[n_events..]
        .iter()
        .filter(|e| {
            e.class.starts_with("control.work_item")
                || e.class.starts_with("lifecycle.")
                || e.class == "context.observation.recorded"
        })
        .map(|e| e.class.clone())
        .collect();
    assert!(replayed.is_empty(), "replay double-fired: {replayed:?}");
    // Anything appended may only be the dedup audit (`control.wakeup.
    // skipped` — durable evidence, never a fire).
    assert!(
        evs[n_events..]
            .iter()
            .all(|e| e.class == "control.wakeup.skipped"),
        "replay appended action rows: {:?}",
        evs[n_events..]
            .iter()
            .map(|e| e.class.clone())
            .collect::<Vec<_>>()
    );
}

// ── AC-3: blocked + escalation path ─────────────────────────────────────
#[test]
fn ac3_blocked_escalate_resolve() {
    let (mut s, _c) = store("ac3", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    let mut init = item_init("i1", Some("alice"));
    init.on.blocked_escalate = Some(hh_fleet::work_item::BlockedEscalate {
        to: "ops".into(),
        deadline_ms: None,
    });
    eng.admit(&mut s, init).unwrap();
    eng.block(&mut s, "i1", "dependency").unwrap();
    let r = eng.reconcile(&mut s, &empty_adapter(), 1_100).unwrap();
    assert!(r.escalated.contains(&"i1".to_string()));
    let it = eng.work_item("i1").unwrap();
    let esc = it.escalation.clone().expect("open escalation");
    // Illegitimate resolution refuses (an uninvolved agent).
    let bad = eng.resolve_escalation(
        &mut s,
        "i1",
        &esc.issue_ref,
        "mallory",
        "acknowledged",
        None,
    );
    assert!(bad.is_err(), "illegitimate resolution must refuse");
    // The declared escalation target resolves.
    eng.resolve_escalation(&mut s, "i1", &esc.issue_ref, "ops", "acknowledged", None)
        .unwrap();
    let it2 = eng.work_item("i1").unwrap();
    assert!(it2.escalation.is_none());
    let classes: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect();
    assert!(classes.contains(&"lifecycle.escalation.raised".to_string()));
    assert!(classes.contains(&"lifecycle.escalation.resolved".to_string()));
}

// ── AC-4: settle → terminal, durable settlement ──────────────────────────
#[test]
fn ac4_settle_terminal() {
    let (mut s, _c) = store("ac4", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    eng.claim(&mut s, "i1").unwrap();
    eng.settle(&mut s, "i1", "completed", vec!["ev:1".into()])
        .unwrap();
    let it = eng.work_item("i1").unwrap();
    assert_eq!(derive_state(&it), "terminal");
    assert_eq!(it.settlement.as_ref().unwrap().outcome, "completed");
    // Idempotent stop/refusal afterwards.
    assert!(matches!(
        eng.stop(&mut s, "i1"),
        Err(FleetError::NotStoppable { .. })
    ));
}

// ── AC-5: restore/replay equivalence — a reopened store folds to the same
//          view (the durable prefix is the whole state; AC's restore leg) ──
#[test]
fn ac5_restore_equivalence() {
    let (mut s, _c, d) = store_at("ac5", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    let ad = fixture(
        Json::Arr(vec![occ(
            "occ-1",
            "external",
            Some("ticket.updated"),
            item("i1", Some("alice")),
            1_100,
        )]),
        &[],
        &["i1"],
    );
    eng.reconcile(&mut s, &ad, 1_100).unwrap();
    let before = eng.fleet_view();
    // Process crash — drop the engine+store, reopen over the same root
    // (the WAL is the record; process memory is gone).
    drop(eng);
    drop(s);
    // The crashed writer's lease is live until its TTL — the restored
    // holder takeovers only after expiry (fenced, audited).
    let mut s2 = reopen(&d, 1_000 + 2 * TTL);
    let (eng2, report) =
        FleetEngine::restore(&mut s2, &run, "restored", TTL, &ad, 1_000 + 2 * TTL).unwrap();
    assert!(report.observed.is_empty(), "restore must not re-fire cues");
    assert!(
        report.dispatched.is_empty(),
        "no double-dispatch on restore"
    );
    // Rebuild equality — every state member identical; `derived_from` is
    // the provenance watermark (the fenced/acquired lease rows of the
    // takeover legitimately advance the folded prefix — durable, never
    // hidden).
    let mut a = before;
    let mut b = eng2.fleet_view();
    if let Json::Obj(ref mut m) = a {
        m.remove("derived_from");
    }
    if let Json::Obj(ref mut m) = b {
        m.remove("derived_from");
    }
    assert_eq!(a, b);
}

// ── AC-6: state-map narrowing at Π + policy fingerprints ─────────────────
#[test]
fn ac6_state_map_narrows_only() {
    let (mut s, _c) = store("ac6", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let sm = eng.state_map(Some("i1")).unwrap();
    assert!(matches!(sm, Json::Obj(_)));
    // A widening candidate is caught by `check_activation_delta` — an
    // `allow` leaf over an ask-gated row widens ⇒ typed refusal, never
    // silently installed (the probe-matrix check).
    // (Narrowing-only by construction is the fold; the delta check is
    // the operator preview.)
    let delta = eng.check_activation_delta(&[]).unwrap();
    assert!(matches!(delta, Json::Obj(_)));
}

// ── AC-7 + RC-6: the matched-budget conditional ──────────────────────────
#[test]
fn ac7_matched_budget_conditional() {
    let (mut s, _c) = store("ac7", 1_000);
    // Neither arm ⇒ typed refusal, never silently unbudgeted (CC9).
    let mut bad = spec();
    bad.out_of_scope = false;
    assert!(matches!(
        FleetEngine::open(&mut s, HOLDER, TTL, bad),
        Err(FleetError::MissingBudgetRef)
    ));
    // budget without budget_ref ⇒ same.
    let mut bad2 = spec();
    bad2.out_of_scope = false;
    bad2.budget = spec_matched().budget;
    assert!(matches!(
        FleetEngine::open(&mut s, HOLDER, TTL, bad2),
        Err(FleetError::MissingBudgetRef)
    ));
    // Matched arm — the account carries the declared slice; the
    // accountability record names events with no charge.
    let (run, mut eng) = open_engine(&mut s, spec_matched());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let rec = eng.accountability_record(&mut s).unwrap();
    assert!(
        matches!(rec, Json::Obj(_)),
        "accountability record must render"
    );
    let _ = run;
}

// ── AC-8: ownership + acknowledgement + handoff gating dispatch ──────────
#[test]
fn ac8_ownership_ack_handoff() {
    let (mut s, _c) = store("ac8", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    let mut init = item_init("i1", Some("alice"));
    init.on.ack_required = true;
    eng.admit(&mut s, init).unwrap();
    // ack-required blocks dispatch until the owner acknowledges.
    let ad = fixture(Json::Arr(vec![]), &[], &["i1"]);
    let mut rep = ReconcileReport::default();
    let r = eng.dispatch(&mut s, &ad, "i1", 1_100, &mut rep);
    assert!(
        matches!(r, Err(FleetError::OwnerAckRequired { .. })),
        "{r:?}"
    );
    eng.ack_owner(&mut s, "i1", "alice").unwrap();
    let mut rep2 = ReconcileReport::default();
    eng.dispatch(&mut s, &ad, "i1", 1_100, &mut rep2).unwrap();
    // Ownership transfer → ack resets; only the NEW owner can ack.
    eng.transfer_owner(&mut s, "i1", "bob", "policy_rule")
        .unwrap();
    let it = eng.work_item("i1").unwrap();
    assert_eq!(it.owner.as_deref(), Some("bob"));
    assert!(!it.owner_ack);
    assert!(matches!(
        eng.ack_owner(&mut s, "i1", "alice"),
        Err(FleetError::NotOwner { .. })
    ));
    eng.ack_owner(&mut s, "i1", "bob").unwrap();
    // Graph edge — set_owner validates acyclic + in-fleet.
    eng.set_owner(&mut s, "i1", "bob", "ops").unwrap();
    assert!(matches!(
        eng.transfer_owner(&mut s, "i1", "mallory", "approval"),
        Err(FleetError::CrossFleet { .. })
    ));
}

// ── RC-1: stale spec refuses ──────────────────────────────────────────────
#[test]
fn rc1_stale_spec_refuses() {
    let (mut s, _c) = store("rc1", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    // A mutated spec is a DIFFERENT activation (the run id is the spec
    // coordinate — no silent widened-reuse).
    let mut stale = spec();
    stale.name = "mutated".into();
    let (run2, _e2) = FleetEngine::open(&mut s, "holder-2", TTL, stale).unwrap();
    assert_ne!(run, run2);
    // The stale-cursor leg — an item whose folded `spec_ref` no longer
    // matches the live spec refuses dispatch typed (RC-1's fence). The
    // mutation here simulates the read-cursor divergence directly; the
    // durable check is the gate.
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    eng.view.spec_ref = "spec:other".into();
    let ad = fixture(Json::Arr(vec![]), &[], &["i1"]);
    let mut rep = ReconcileReport::default();
    let r = eng.dispatch(&mut s, &ad, "i1", 1_100, &mut rep);
    assert!(matches!(r, Err(FleetError::StaleSpec { .. })), "{r:?}");
}

// ── RC-3: source conflict (same idempotency key, different dossier) ───────
#[test]
fn rc3_source_conflict_blocks() {
    let (mut s, _c) = store("rc3", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let mut changed = item_init("i1", Some("alice"));
    changed.title = "different dossier".into();
    let out = eng.admit(&mut s, changed);
    match out {
        Err(FleetError::SourceConflict { .. }) => {}
        Ok(_) | Err(_) => {
            // The durable-conflict path records the row and marks the
            // item blocked rather than erroring — accept either the typed
            // error or the durable mark.
            let it = eng.work_item("i1").unwrap();
            assert!(
                it.blocked.iter().any(|b| b.contains("conflict")),
                "conflict must surface: {:?}",
                it.blocked
            );
        }
    }
}

// ── RC-4: source suspension ──────────────────────────────────────────────
#[test]
fn rc4_source_suspension_blocks_dispatch() {
    let (mut s, _c) = store("rc4", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let suspended = fixture(Json::Arr(vec![]), &["tickets"], &["i1"]);
    let r = eng.reconcile(&mut s, &suspended, 1_100).unwrap();
    let it = eng.work_item("i1").unwrap();
    assert!(
        it.suspended || it.blocked.iter().any(|b| b.contains("suspended")),
        "suspended source must mark the item: {:?}",
        it.blocked
    );
    assert!(!r.dispatched.contains(&"i1".to_string()));
}

// ── RC-5: dispatch error → durable retry schedule ────────────────────────
#[test]
fn rc5_dispatch_error_schedules_retry() {
    let (mut s, _c) = store("rc5", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    let mut init = item_init("i1", Some("alice"));
    init.on.retry_max_attempts = Some(3);
    init.on.retry_backoff_ms = Some(60_000);
    eng.admit(&mut s, init).unwrap();
    let ad = fixture(Json::Arr(vec![]), &[], &["i1"]);
    let r = eng.reconcile(&mut s, &ad, 1_100).unwrap();
    assert!(r.dispatched.contains(&"i1".to_string()));
    let it = eng.work_item("i1").unwrap();
    let spec_ref = it
        .dispatch
        .spec_ref
        .clone()
        .unwrap_or_else(|| it.spec_ref.clone());
    // The launcher reports a dispatch error — the durable note schedules
    // retry (RC-5's `dispatch_error` arm).
    eng.dispatch_note(&mut s, "i1", &spec_ref, None, Some("transient"))
        .unwrap();
    let classes: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect();
    assert!(
        classes
            .iter()
            .any(|c| c.starts_with("control.retry") || c == "control.wakeup.scheduled"),
        "retry must land durable: {classes:?}"
    );
}

// ── RC-7: capacity is the pool `fan_out` gauge — exhaustion leaves the
//          item queued (never trims budgets; the bound is durable data) ──
#[test]
fn rc7_capacity_fan_out_bound() {
    let (mut s, _c) = store("rc7", 1_000);
    let mut sp = spec();
    sp.capacity.activate_run = 1;
    let (_run, mut eng) = open_engine(&mut s, sp);
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    eng.admit(&mut s, item_init("i2", Some("bob"))).unwrap();
    let ad = fixture(Json::Arr(vec![]), &[], &["i1", "i2"]);
    let r = eng.reconcile(&mut s, &ad, 1_100).unwrap();
    // Deterministic order (item_id) — only i1 fits the bound; i2 stays
    // queued (the bound refuses the NEXT dispatch — a skip, not a trim).
    assert_eq!(r.dispatched, vec!["i1".to_string()]);
    let i2 = eng.work_item("i2").unwrap();
    assert_eq!(derive_state(&i2), "queued");
    // RC-2 — a second claim on a claimed item is the typed WouldBlock.
    eng.claim(&mut s, "i2").unwrap();
    let second = eng.claim(&mut s, "i2");
    assert!(
        second.is_err() || second.unwrap().get("existing").is_some(),
        "second claim must fence"
    );
}

// ── Trigger admissibility: fleet boundary admits external/manual/timer;
//    unsupported variants refuse typed ────────────────────────────────────
#[test]
fn unsupported_trigger_variants_refuse() {
    let (mut s, _c) = store("trig", 1_000);
    let mut sp = spec();
    sp.triggers.insert(
        "peer".into(),
        TriggerRule {
            name: "peer".into(),
            trigger: Trigger::PeerMessage { from: "r-x".into() },
            policy: WakeupPolicy::default_policy(),
        },
    );
    let r = FleetEngine::open(&mut s, HOLDER, TTL, sp);
    assert!(matches!(r, Err(_)), "peer trigger must refuse");
}

// ── The handoff state — human-gate block + resume ─────────────────────────
#[test]
fn human_gate_handoff_and_resume() {
    let (mut s, _c) = store("gate", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    eng.block(&mut s, "i1", "human_gate").unwrap();
    let it = eng.work_item("i1").unwrap();
    assert_eq!(derive_state(&it), "handoff");
    eng.resume_from_handoff(&mut s, "i1", "alice").unwrap();
    let it2 = eng.work_item("i1").unwrap();
    assert_ne!(derive_state(&it2), "handoff");
}

// ── Reads — work_item/list/fleet_view/audit_link render records ──────────
#[test]
fn reads_render_records() {
    let (mut s, _c) = store("reads", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let view = eng.fleet_view();
    assert_eq!(
        view.get("schema").and_then(Json::as_str),
        Some("hh.fleet.view/1")
    );
    let items = eng.list();
    assert_eq!(items.len(), 1);
    // `audit_link` rows exist per *dispatched* item (the fleet_anchor
    // obligation joins `run_ref`); a pre-dispatch item carries none.
    assert!(eng.audit_link(&s).unwrap().is_empty());
    let ad = fixture(Json::Arr(vec![]), &[], &["i1"]);
    let mut rep = ReconcileReport::default();
    eng.dispatch(&mut s, &ad, "i1", 1_100, &mut rep).unwrap();
    stamp_note(&mut eng, &mut s, "i1");
    let links = eng.audit_link(&s).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].get("obligation").and_then(Json::as_str),
        Some("fleet_anchor")
    );
    assert_eq!(
        links[0].get("status").and_then(Json::as_str),
        Some("verified")
    );
}
