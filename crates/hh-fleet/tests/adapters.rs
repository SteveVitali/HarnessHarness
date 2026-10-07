//! S5.6 acceptance matrix — `AC-R-2.12.6-{9..11}` over the adapter
//! surface (§5i.1 #3; ADR-0205 D5; ADR-0207 D4/D5): signed-webhook
//! ingress (verify → key → dedup → provenance), the tri-state capability
//! probe, source-unavailable degradation, adapter `activate_run`
//! filtering, the optional `ext.effects.external_irreversible` ceiling,
//! and the FleetView `hh.fleet.view/2` member set.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::spec::BudgetMode;
use hh_budget::{BudgetSpec, DimensionId, DimensionKey};
use hh_fleet::capabilities::{AdapterCapabilities, CapState};
use hh_fleet::engine::{FleetEngine, ReconcileReport};
use hh_fleet::errors::FleetError;
use hh_fleet::ingress::{
    poll_occurrence_id, push_occurrence_id, IngressError, IngressOutcome, WebhookAdapter,
};
use hh_fleet::source::{FixtureAdapter, SourceOccurrence, WorkSourceAdapter};
use hh_fleet::spec::{Capacity, Defaults, FleetSpec, TriggerRule};
use hh_fleet::work_item::{derive_state, WorkItemInit};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256_hex;

const HOLDER: &str = "fleet-adapter-test";
const TTL: u64 = 60_000;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "hh-fleet-adapter-test-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn store(tag: &str, ms: u64) -> Store {
    let d = dir(tag);
    Store::open_with(
        d,
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::new())),
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
    FleetSpec {
        schema: "hh.fleet.spec/1".into(),
        version: "1".into(),
        name: "adapters".into(),
        purpose: "fleet_activation".into(),
        fixture_ref: "fixture:adapters".into(),
        agents: vec!["alice".into(), "ops".into()],
        capacity: Capacity {
            activate_run: 8,
            items: 64,
        },
        ownership: BTreeMap::new(),
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

fn item(id: &str, owner: Option<&str>) -> Json {
    Json::obj([
        ("item_id", Json::str(id)),
        ("title", Json::str(format!("ticket {id}"))),
        (
            "source",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("kind", Json::str("ticket")),
                ("source_ref", Json::str(format!("src:{id}"))),
            ]),
        ),
        ("idempotency_key", Json::str(format!("idem:{id}"))),
        ("owner", owner.map(Json::str).unwrap_or(Json::Null)),
    ])
}

fn item_init(id: &str, owner: Option<&str>) -> WorkItemInit {
    WorkItemInit::from_json(&item(id, owner)).unwrap()
}

fn open_engine(s: &mut Store, spec: FleetSpec) -> (String, FleetEngine) {
    FleetEngine::open(s, HOLDER, TTL, spec).unwrap()
}

// ── webhook fixtures ────────────────────────────────────────────────────

/// The `hh.fleet.webhook_source/1` doc — `webhook` = the IngressPolicy
/// verbatim (key_ref names the broker coordinate; the key bytes travel
/// per-call, never in a record).
fn webhook_doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook_source/1")),
        (
            "webhook",
            Json::obj([
                ("source_id", Json::str("tickets")),
                ("key_ref", Json::str("vault:hh/test/webhook")),
                ("replay_window_ms", Json::Int(3_600_000)),
                ("volatile_fields", Json::Arr(vec![Json::str("etag")])),
                ("external_kind", Json::str("ticket.updated")),
            ]),
        ),
    ])
}

const KEY: &[u8] = b"test-webhook-key";

fn delivery(id: Option<&str>, item_json: Json) -> Json {
    let mut m = BTreeMap::new();
    m.insert("schema_version".into(), Json::str("hh.fleet.webhook/1"));
    if let Some(d) = id {
        m.insert("delivery_id".into(), Json::str(d));
    }
    m.insert("item".into(), item_json);
    Json::Obj(m)
}

fn sign(d: &Json, key: &[u8]) -> String {
    format!(
        "hmac-sha256:{}",
        hmac_sha256_hex(key, d.to_canonical_string().as_bytes())
    )
}

fn webhook_adapter(run: &str) -> WebhookAdapter {
    WebhookAdapter::from_doc(&webhook_doc(), run).unwrap()
}

// ── capabilities (§5i.1 #3's capability row; T-LCD-07) ──────────────────

#[test]
fn capabilities_are_tri_state_and_probe_only_unknown() {
    let mut caps = AdapterCapabilities::all_unknown();
    assert_eq!(caps.get("push_delivery_id"), Some(CapState::Unknown));
    assert!(!CapState::Unknown.supported());
    assert!(CapState::Declared.supported());
    // The probe resolves `unknown` only — a declared member is never
    // downgraded by evidence (T-LCD-07). (`probe_with`'s bool is "the
    // member is known"; the state is the evidence.)
    caps.probe_with("push_delivery_id", true);
    assert_eq!(caps.get("push_delivery_id"), Some(CapState::Probed));
    caps.probe_with("push_delivery_id", false);
    assert_eq!(
        caps.get("push_delivery_id"),
        Some(CapState::Probed),
        "a probed member is never downgraded by a later probe"
    );
    caps.set("poll", CapState::Declared);
    caps.probe_with("poll", false);
    assert_eq!(
        caps.get("poll"),
        Some(CapState::Declared),
        "a declared member is never downgraded by a probe"
    );
    // An absent probe leaves `unknown` standing; a negative probe
    // answers `probed_absent`.
    assert_eq!(caps.get("assignee_mapping"), Some(CapState::Unknown));
    caps.probe_with("assignee_mapping", false);
    assert_eq!(caps.get("assignee_mapping"), Some(CapState::ProbedAbsent));
    // Wire form — quad-state spellings, strict decode.
    let j = caps.to_json();
    let back = AdapterCapabilities::from_json(&j).unwrap();
    assert_eq!(back.get("push_delivery_id"), Some(CapState::Probed));
    assert_eq!(back.get("poll"), Some(CapState::Declared));
    // An unknown member spelling fails closed — never coerced.
    assert!(AdapterCapabilities::from_json(&Json::obj([("poll", Json::str("maybe"))])).is_err());
}

// ── OQ-313 occurrence keys ──────────────────────────────────────────────

#[test]
fn oq313_occurrence_keys() {
    let payload = Json::obj([
        ("item_id", Json::str("i1")),
        ("etag", Json::str("v1")),
        ("state", Json::str("open")),
    ]);
    // Push with a delivery id — the id, namespaced by source_ref.
    assert_eq!(
        push_occurrence_id("src:run:tickets", Some("d-9"), &payload, &[]),
        "push:src:run:tickets:d-9"
    );
    // Push without — H(canonical(payload − volatile_fields)); the
    // declared `etag` member does not move the key, real content does.
    let a = push_occurrence_id("src:run:tickets", None, &payload, &["etag".to_string()]);
    let b = push_occurrence_id(
        "src:run:tickets",
        None,
        &Json::obj([
            ("item_id", Json::str("i1")),
            ("etag", Json::str("v2")),
            ("state", Json::str("open")),
        ]),
        &["etag".to_string()],
    );
    assert_eq!(a, b, "volatile members must not move the content key");
    let c = push_occurrence_id(
        "src:run:tickets",
        None,
        &Json::obj([
            ("item_id", Json::str("i1")),
            ("etag", Json::str("v2")),
            ("state", Json::str("closed")),
        ]),
        &["etag".to_string()],
    );
    assert_ne!(a, c, "non-volatile content must move the key");
    // Poll — H(native_id ∥ normalized(state) ∥ updated_at); normalized is
    // trim+lowercase.
    assert_eq!(
        poll_occurrence_id("src:run:tickets", "T-1", "  Open ", "2024"),
        poll_occurrence_id("src:run:tickets", "T-1", "open", "2024"),
    );
    assert_ne!(
        poll_occurrence_id("src:run:tickets", "T-1", "open", "2024"),
        poll_occurrence_id("src:run:tickets", "T-1", "open", "2025"),
    );
}

// ── signed ingress: verify, key, enqueue, dedup, probe ─────────────────

#[test]
fn webhook_accepts_a_signed_delivery_and_carries_provenance() {
    let mut ad = webhook_adapter("fleet-run-1");
    let d = delivery(Some("d-1"), item("i1", Some("alice")));
    let sig = sign(&d, KEY);
    match ad.receive(&d, &sig, KEY, 1_000).unwrap() {
        IngressOutcome::Received { occurrence_id } => {
            assert_eq!(
                occurrence_id,
                "push:".to_string()
                    + &hh_fleet::identity::source_ref("fleet-run-1", "tickets")
                    + ":d-1"
            );
        }
        other => panic!("expected Received, got {other:?}"),
    }
    // The delivered id probed `push_delivery_id` — the adapter's doc
    // leaves it `declared` (the push lane's own capability).
    assert_eq!(
        ad.capabilities().get("push_delivery_id"),
        Some(CapState::Declared)
    );
    let occs = ad.occurrences(None);
    assert_eq!(occs.len(), 1);
    let occ = &occs[0];
    // Actor provenance on the observation (§5i.1 #6 — the occurrence's
    // actor member; the item's source carries ingress/external).
    assert_eq!(occ.actor, "webhook");
    assert_eq!(
        occ.item.source.get("origin").and_then(Json::as_str),
        Some("ingress")
    );
    assert_eq!(
        occ.item.source.get("authority").and_then(Json::as_str),
        Some("external")
    );
    // The content-hash idempotency key replaced the dossier's — a stale
    // re-delivery under a fresh delivery id still admits `Known`.
    assert_ne!(occ.item.idempotency_key, "idem:i1");
    assert!(occ.item.idempotency_key.contains(':'));
    // The queue drains.
    assert!(ad.occurrences(None).is_empty());
}

#[test]
fn webhook_refuses_closed() {
    let mut ad = webhook_adapter("fleet-run-1");
    let d = delivery(Some("d-1"), item("i1", Some("alice")));
    // Bad signature — fail closed.
    let bad = sign(&d, b"wrong-key");
    assert_eq!(
        ad.receive(&d, &bad, KEY, 1_000),
        Err(IngressError::BadSignature)
    );
    // Unsupported algorithm spelling — the closed scheme, never coerced.
    assert!(matches!(
        ad.receive(&d, "sha1:deadbeef", KEY, 1_000),
        Err(IngressError::UnsupportedAlg { .. })
    ));
    // Malformed payload — missing schema_version.
    let no_schema = Json::obj([("item", item("i1", Some("alice")))]);
    let sig = sign(&no_schema, KEY);
    assert!(matches!(
        ad.receive(&no_schema, &sig, KEY, 1_000),
        Err(IngressError::MalformedPayload { .. })
    ));
    // A refused delivery never enqueues.
    assert!(ad.occurrences(None).is_empty());
}

#[test]
fn webhook_duplicate_inside_window_labels_and_the_durable_skip_lands() {
    let mut s = store("dup", 1_000);
    let (run, mut eng) = open_engine(&mut s, spec());
    let mut ad = webhook_adapter(&run);
    let d = delivery(Some("d-1"), item("i1", Some("alice")));
    let sig = sign(&d, KEY);
    // First delivery — Received; second — Duplicate, still enqueued (the
    // durable fold audits the skip; the label is never a drop).
    assert!(matches!(
        ad.receive(&d, &sig, KEY, 1_100).unwrap(),
        IngressOutcome::Received { .. }
    ));
    assert!(matches!(
        ad.receive(&d, &sig, KEY, 1_200).unwrap(),
        IngressOutcome::Duplicate { .. }
    ));
    let r = eng.reconcile(&mut s, &mut ad, 1_300).unwrap();
    assert_eq!(r.observed.len(), 1, "one occurrence is new");
    // The audited duplicate row — `control.wakeup.skipped` carrying
    // `duplicate_occurrence` (CC3 — evidence, not a silent drop).
    let skipped: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.wakeup.skipped")
        .collect();
    assert!(
        skipped.iter().any(|e| {
            e.payload
                .get("reason")
                .and_then(Json::as_str)
                .map(|r| r.contains("duplicate_occurrence"))
                .unwrap_or(false)
                || e.payload
                    .get("verb")
                    .and_then(Json::as_str)
                    .map(|v| v.contains("duplicate"))
                    .unwrap_or(false)
        }),
        "no audited duplicate_occurrence row: {:?}",
        skipped
            .iter()
            .map(|e| e.payload.to_canonical_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(eng.work_item("i1").unwrap().item_id, "i1");
}

#[test]
fn webhook_push_delivery_id_probe_resolves_unknown() {
    // The doc declares the capability `unknown` — the first delivery
    // carrying a `delivery_id` probes it `probed` (T-LCD-07).
    let mut doc = webhook_doc();
    if let Json::Obj(ref mut o) = doc {
        o.insert(
            "capabilities".into(),
            Json::obj([("push_delivery_id", Json::str("unknown"))]),
        );
    }
    let mut ad = WebhookAdapter::from_doc(&doc, "fleet-run-1").unwrap();
    assert_eq!(
        ad.capabilities().get("push_delivery_id"),
        Some(CapState::Unknown)
    );
    let d = delivery(Some("d-1"), item("i1", Some("alice")));
    let sig = sign(&d, KEY);
    ad.receive(&d, &sig, KEY, 1_000).unwrap();
    assert_eq!(
        ad.capabilities().get("push_delivery_id"),
        Some(CapState::Probed)
    );
}

// ── source fault — SourceUnavailable degrades, never fabricates ────────

/// A faulted adapter — the `fault()` seam the reconciler reads (a
/// plugin's latched transport fault is the production shape).
struct FaultyAdapter;

impl WorkSourceAdapter for FaultyAdapter {
    fn occurrences(&mut self, _since: Option<u64>) -> Vec<SourceOccurrence> {
        panic!("a faulted adapter must never be polled")
    }
    fn suspended(&mut self, _source_id: &str) -> bool {
        panic!("a faulted adapter must never be polled")
    }
    fn activate_run(&mut self, _candidates: &[String]) -> Vec<String> {
        panic!("a faulted adapter must never be polled")
    }
    fn fault(&self) -> Option<String> {
        Some("transport:connection_refused".to_string())
    }
}

#[test]
fn source_unavailable_skips_source_passes_and_keeps_activations() {
    let mut s = store("fault", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let mut bad = FaultyAdapter;
    // observe refuses outright — the typed failure, not a drop.
    assert!(matches!(
        eng.observe(&mut s, &mut bad, 1_100),
        Err(FleetError::SourceUnavailable { .. })
    ));
    // reconcile degrades: the adapter-reading passes skip, the report
    // carries the failure row, nothing is invented.
    let r = eng.reconcile(&mut s, &mut bad, 1_200).unwrap();
    assert_eq!(
        r.source_unavailable.as_deref(),
        Some("transport:connection_refused")
    );
    assert!(r.observed.is_empty() && r.dispatched.is_empty());
    assert!(r.source_unavailable.is_some());
    // The item stays admitted — no synthetic state.
    let it = eng.work_item("i1").unwrap();
    assert_eq!(derive_state(&it), "queued");
}

// ── adapter dispatch filtering (activate.run) ──────────────────────────

#[test]
fn activate_run_filters_candidates() {
    let mut s = store("runnable", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    eng.admit(&mut s, item_init("i2", Some("alice"))).unwrap();
    // The source declares only `i2` dispatchable — `i1` stays queued
    // through the reconcile pass (the dispatch filter is `activate.run`'s
    // intersection, §5i.1 #2 — not a refusal, the item waits).
    let mut ad = FixtureAdapter::from_doc(Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        ("activate_run", Json::Arr(vec![Json::str("i2")])),
    ]))
    .unwrap();
    let r = eng.reconcile(&mut s, &mut ad, 1_100).unwrap();
    assert_eq!(r.dispatched, vec!["i2".to_string()]);
    assert_eq!(derive_state(&eng.work_item("i1").unwrap()), "queued");
    assert_eq!(derive_state(&eng.work_item("i2").unwrap()), "dispatching");
}

// ── the optional irreversibility ceiling (ADR-0207 D5; AC-11) ──────────

/// A matched spec with `ext.effects.external_irreversible` capped at
/// `cap` on the activation root pool.
fn ceiling_spec(cap: i64) -> FleetSpec {
    let mut sp = spec();
    sp.out_of_scope = false;
    sp.budget = Some(BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
            (
                DimensionKey::Primary(DimensionId::ExtEffectsExternalIrreversible),
                cap,
            ),
        ],
    ));
    sp.budget_ref = Some("budget:fleet:ceiling".into());
    sp
}

/// An item declaring `on.external_irreversible`.
fn irreversible_item(id: &str) -> Json {
    let mut m = match item(id, Some("alice")) {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    m.insert(
        "on".into(),
        Json::obj([("external_irreversible", Json::Bool(true))]),
    );
    Json::Obj(m)
}

#[test]
fn irreversibility_ceiling_denies_and_escalates() {
    let mut s = store("ceiling", 1_000);
    let (run, mut eng) = open_engine(&mut s, ceiling_spec(0));
    eng.admit(
        &mut s,
        WorkItemInit::from_json(&irreversible_item("i1")).unwrap(),
    )
    .unwrap();
    let mut ad = FixtureAdapter::from_doc(Json::obj([(
        "schema_version",
        Json::str("hh.fleet.fixture/1"),
    )]))
    .unwrap();
    let mut rep = ReconcileReport::default();
    let err = eng
        .dispatch(&mut s, &mut ad, "i1", 1_100, &mut rep)
        .unwrap_err();
    assert!(matches!(err, FleetError::IrreversibilityCeiling { .. }));
    // The durable evidence: blocked{irreversibility_ceiling} + an open
    // escalation naming the optional dimension.
    let it = eng.work_item("i1").unwrap();
    assert!(it.blocked.contains("irreversibility_ceiling"));
    let esc_rows: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.escalation.raised")
        .collect();
    assert_eq!(esc_rows.len(), 1);
    assert_eq!(
        esc_rows[0].payload.get("issue").and_then(Json::as_str),
        Some("irreversibility_ceiling")
    );
    // And the item did not dispatch.
    assert_ne!(derive_state(&it), "dispatching");
    assert_ne!(derive_state(&it), "dispatched");
}

#[test]
fn irreversibility_ceiling_absent_or_unset_is_a_no_op() {
    // Cap present but generous — the reservation lands and dispatch
    // proceeds (one unit of `ext.effects.external_irreversible`).
    let mut s = store("ceiling-ok", 1_000);
    let (_run, mut eng) = open_engine(&mut s, ceiling_spec(4));
    eng.admit(
        &mut s,
        WorkItemInit::from_json(&irreversible_item("i1")).unwrap(),
    )
    .unwrap();
    let mut ad = FixtureAdapter::from_doc(Json::obj([(
        "schema_version",
        Json::str("hh.fleet.fixture/1"),
    )]))
    .unwrap();
    let mut rep = ReconcileReport::default();
    eng.dispatch(&mut s, &mut ad, "i1", 1_100, &mut rep)
        .unwrap();
    assert_eq!(derive_state(&eng.work_item("i1").unwrap()), "dispatching");
    // An item without the flag under a cap-0 pool dispatches — the
    // optional dimension changes nothing for undeclared items.
    let mut s2 = store("ceiling-noop", 1_000);
    let (_r2, mut eng2) = open_engine(&mut s2, ceiling_spec(0));
    eng2.admit(&mut s2, item_init("i2", Some("alice"))).unwrap();
    let mut rep2 = ReconcileReport::default();
    eng2.dispatch(&mut s2, &mut ad, "i2", 1_100, &mut rep2)
        .unwrap();
    assert_eq!(derive_state(&eng2.work_item("i2").unwrap()), "dispatching");
}

// ── FleetView hh.fleet.view/2 members (§7.1; ADR-0207 D4) ──────────────

#[test]
fn fleet_view_carries_the_spec_members() {
    let mut s = store("view", 1_000);
    let (_run, mut eng) = open_engine(&mut s, spec());
    eng.admit(&mut s, item_init("i1", Some("alice"))).unwrap();
    let v = eng.fleet_view(&mut s);
    assert_eq!(
        v.get("schema").and_then(Json::as_str),
        Some("hh.fleet.view/2")
    );
    let item = match v.get("items") {
        Some(Json::Arr(a)) => &a[0],
        _ => panic!("FleetView.items must be an array"),
    };
    for m in [
        "work_item_id",
        "state",
        "owner",
        "source_state",
        "activation",
        "activation_no",
        "blocked",
        "escalations_open",
        "budget",
        "last_activity",
        "external_effects_count",
        "children_open",
    ] {
        assert!(item.get(m).is_some(), "FleetView item missing {m}");
    }
    for m in [
        "capacity",
        "escalations",
        "totals",
        "watermark",
        "derived_from",
    ] {
        assert!(v.get(m).is_some(), "FleetView missing {m}");
    }
    // Rebuild equality — the fold over the same durable prefix is the
    // same view (ADR-0207 D4's project-pure rule).
    let v2 = eng.fleet_view(&mut s);
    assert_eq!(v.to_canonical_string(), v2.to_canonical_string());
}
