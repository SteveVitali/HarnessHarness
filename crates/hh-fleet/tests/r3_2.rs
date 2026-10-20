//! R3.2 — the non-fleet ingress-adapter producer (R-2.12.6; DF-S2.3-1;
//! BL-44): `observe_run` is the same adapter→durable seam
//! `FleetEngine::observe` runs, on a `run_kind != fleet` run whose
//! manifest declares the adapter (`extra.ingress_adapters[]` — stamped
//! from `connection_info.ingress_adapters` at `open_session`).
//!
//! Each adapter occurrence mints `context.observation.recorded` then
//! `control.wakeup.occurred` on the covering subscription — durable
//! before `deliver_wakeup` fires the pending set (CC3); a replayed
//! occurrence hits the audited `skipped{duplicate_occurrence}` dedup,
//! never a second fire; an occurrence no declared subscription covers is
//! recorded but never fired (the trigger set is the contract);
//! undeclared manifests and `run_kind = fleet` runs refuse typed (one
//! producer per run kind — CC10).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_fleet::engine::FleetEngine;
use hh_fleet::errors::FleetError;
use hh_fleet::source::FixtureAdapter;
use hh_fleet::spec::{Capacity, Defaults, FleetSpec, TriggerRule};
use hh_fleet::{observe_run, COMPONENT};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-fleet-r32-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str) -> Store {
    Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

/// An agent run declaring the `ing:ci` `external` adapter — the
/// `connection_info{ingress_adapters}` channel's durable spelling.
fn declared_run(s: &mut Store) -> (String, Lease) {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.extra.insert(
        "ingress_adapters".to_string(),
        Json::Arr(vec![Json::obj([
            ("source_ref", Json::str("ing:ci")),
            ("triggers", Json::Arr(vec![Json::str("external")])),
        ])]),
    );
    s.open_run(m, "writer-a").unwrap()
}

fn eref(run: &str) -> EventRef {
    EventRef {
        run_id: run.to_string(),
        event_id: "e-0".to_string(),
    }
}

fn item(id: &str) -> Json {
    Json::obj([
        ("item_id", Json::str(id)),
        ("title", Json::str(format!("ticket {id}"))),
        (
            "source",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("kind", Json::str("ticket")),
                ("source_ref", Json::str("ing:ci")),
            ]),
        ),
        ("idempotency_key", Json::str(format!("idem:{id}"))),
        ("owner", Json::Null),
    ])
}

fn occ(id: &str, kind: &str, item_json: Json, at: u64) -> Json {
    Json::obj([
        ("occurrence_id", Json::str(id)),
        ("trigger", Json::str("external")),
        ("kind", Json::str(kind)),
        ("item", item_json),
        ("observed_at_ms", Json::Int(at as i64)),
    ])
}

fn fixture(occs: Vec<Json>) -> FixtureAdapter {
    FixtureAdapter::from_doc(Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        ("occurrences", Json::Arr(occs)),
        ("suspended", Json::Arr(vec![])),
        ("activate_run", Json::Arr(vec![])),
    ]))
    .unwrap()
}

fn class_seqs(s: &Store, run: &str) -> Vec<(u64, String)> {
    s.events(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone()))
        .collect()
}

fn count(s: &Store, run: &str, class: &str) -> usize {
    s.events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .count()
}

#[test]
fn r3_2_observe_run_produces_durable_ingress_rows() {
    let mut s = store("observe");
    let (run, lease) = declared_run(&mut s);
    // The run's declared subscription covers `ticket.updated`.
    let sub = s
        .wakeup_subscribe(
            &run,
            &lease,
            Trigger::External {
                kind: "ticket.updated".to_string(),
                source_ref: Some("ing:ci".to_string()),
                filter: None,
            },
            WakeupPolicy::default_policy(),
            &eref(&run),
        )
        .unwrap();

    let mut adapter = fixture(vec![occ("occ-1", "ticket.updated", item("w-1"), 1_100)]);
    let delivered = observe_run(&mut s, &run, &lease, &mut adapter, 2_000).unwrap();
    assert_eq!(delivered, vec!["occ-1".to_string()]);

    // Durable-before-visible: `recorded → occurred → fired` in order,
    // under the run's writer lease.
    let seqs = class_seqs(&s, &run);
    let at = |c: &str| {
        seqs.iter()
            .find(|(_, k)| k == c)
            .map(|(n, _)| *n)
            .unwrap_or_else(|| panic!("{c} missing: {seqs:?}"))
    };
    let (rec, occr, fired) = (
        at("context.observation.recorded"),
        at("control.wakeup.occurred"),
        at("control.wakeup.fired"),
    );
    assert!(rec < occr && occr < fired, "{seqs:?}");
    // The occurrence row binds the subscription the declaration admitted.
    let occurred = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.wakeup.occurred")
        .unwrap();
    assert_eq!(
        occurred
            .payload
            .get("subscription_id")
            .and_then(Json::as_str),
        Some(sub.as_str())
    );
    // The recorded row is kernel-produced (`hh-fleet` — the adapter's
    // producer component) and names the occurrence.
    let recorded = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "context.observation.recorded")
        .unwrap();
    assert_eq!(
        recorded.producer.component_variant_ref, COMPONENT,
        "the row's producer is the ingress component"
    );
    assert_eq!(
        recorded
            .payload
            .get("trigger")
            .and_then(|t| t.get("occurrence_id"))
            .and_then(Json::as_str),
        Some("occ-1")
    );
    // The fired delivery drains — the run's re-cue is visible.
    let drained = s.wakeup_drain(&run).unwrap();
    assert_eq!(drained.len(), 1, "{drained:?}");
    assert!(matches!(
        &drained[0].trigger,
        Trigger::External { kind, .. } if kind == "ticket.updated"
    ));
}

#[test]
fn r3_2_observe_run_replay_is_audited_dedup() {
    let mut s = store("observe-replay");
    let (run, lease) = declared_run(&mut s);
    s.wakeup_subscribe(
        &run,
        &lease,
        Trigger::External {
            kind: "ticket.updated".to_string(),
            source_ref: Some("ing:ci".to_string()),
            filter: None,
        },
        WakeupPolicy::default_policy(),
        &eref(&run),
    )
    .unwrap();
    let mk = || fixture(vec![occ("occ-1", "ticket.updated", item("w-1"), 1_100)]);
    let mut a1 = mk();
    let first = observe_run(&mut s, &run, &lease, &mut a1, 2_000).unwrap();
    assert_eq!(first.len(), 1);
    s.wakeup_drain(&run).unwrap();
    // The same fixture replayed — the occurrence dedups: no second
    // `recorded`, no second `occurred`; the audited `skipped` row lands.
    let mut a2 = mk();
    let second = observe_run(&mut s, &run, &lease, &mut a2, 3_000).unwrap();
    assert!(second.is_empty(), "a replayed fixture delivers nothing new");
    assert_eq!(count(&s, &run, "context.observation.recorded"), 1);
    assert_eq!(count(&s, &run, "control.wakeup.occurred"), 1);
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "control.wakeup.skipped"),
        "the dedup is audited, never a silent drop"
    );
}

#[test]
fn r3_2_observe_run_undeclared_manifest_refuses() {
    let mut s = store("observe-undeclared");
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let mut adapter = fixture(vec![occ("occ-1", "ticket.updated", item("w-1"), 1_100)]);
    let err = observe_run(&mut s, &run, &lease, &mut adapter, 2_000).unwrap_err();
    assert!(
        matches!(
            err,
            FleetError::Unsupported {
                op: "observe_run",
                ..
            }
        ),
        "undeclared manifest refuses typed: {err:?}"
    );
    assert_eq!(
        count(&s, &run, "context.observation.recorded"),
        0,
        "nothing durable on a refused observe"
    );
    // `run_kind = fleet` routes to `FleetEngine::observe` — one producer
    // per run kind (CC10).
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
    let spec = FleetSpec {
        schema: "hh.fleet.spec/1".into(),
        version: "1".into(),
        name: "triage".into(),
        purpose: "fleet_activation".into(),
        fixture_ref: "fixture:triage".into(),
        agents: vec!["ops".into()],
        capacity: Capacity {
            activate_run: 4,
            items: 16,
        },
        ownership: BTreeMap::new(),
        policy_ref: "policy:test".into(),
        narrowing: Vec::new(),
        budget: None,
        budget_ref: None,
        out_of_scope: true,
        triggers,
        defaults: Defaults::default(),
        human_gate_states: vec![],
    };
    let (frun, engine) = FleetEngine::open(&mut s, "fleet-test", 60_000, spec).unwrap();
    let mut adapter2 = fixture(vec![occ("occ-9", "ticket.updated", item("w-9"), 1_100)]);
    let err2 = observe_run(&mut s, &frun, &engine.lease, &mut adapter2, 2_000).unwrap_err();
    assert!(matches!(err2, FleetError::Unsupported { .. }), "{err2:?}");
}

#[test]
fn r3_2_observe_run_uncovered_occurrence_records_but_never_fires() {
    let mut s = store("observe-uncovered");
    let (run, lease) = declared_run(&mut s);
    // The subscription covers `ticket.updated`; the fixture also carries
    // an occurrence kind nothing declared — recorded (CC3) but never
    // fired: the trigger set is the contract.
    s.wakeup_subscribe(
        &run,
        &lease,
        Trigger::External {
            kind: "ticket.updated".to_string(),
            source_ref: Some("ing:ci".to_string()),
            filter: None,
        },
        WakeupPolicy::default_policy(),
        &eref(&run),
    )
    .unwrap();
    let mut adapter = fixture(vec![
        occ("occ-1", "ticket.updated", item("w-1"), 1_100),
        occ("occ-2", "pr.merged", item("w-2"), 1_200),
    ]);
    let delivered = observe_run(&mut s, &run, &lease, &mut adapter, 2_000).unwrap();
    assert_eq!(delivered, vec!["occ-1".to_string()]);
    // Both occurrences recorded; only the covered one occurred/fired.
    assert_eq!(count(&s, &run, "context.observation.recorded"), 2);
    assert_eq!(count(&s, &run, "control.wakeup.occurred"), 1);
    assert_eq!(count(&s, &run, "control.wakeup.fired"), 1);
}
