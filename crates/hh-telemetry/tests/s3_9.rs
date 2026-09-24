//! S3.9 — `harness_overhead.execution_ms` (AC-R-2.5.5-11's registered
//! distribution slice): per-effect execution-window samples fold into a
//! `{executor_class/isolation_class → {n, min, p50, p95, max}}` distribution,
//! `n/a{estimator_undefined}` on no samples, `n/a{observability}` under a
//! hosted-grade declaration (`events` only — never 0, never a proxy).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Cursor, Direction, Event, EventEnvelope, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ontology::participant::Observability;
use hh_telemetry::views::metric_view;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-telemetry-s39-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn read_all(s: &Store, run: &str) -> Vec<EventEnvelope> {
    s.read(run, Cursor::Seq(0), None, Direction::Fwd, 10_000)
        .unwrap()
        .events
}

fn ev(id: &str, class: &str, scope: Scope, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope,
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

/// Open the `turn ⊃ model_call` chain the `tool_call` scope members require.
fn open_scopes(s: &mut Store, run: &str, lease: &Lease) {
    let mut t = ev(
        "turn-open",
        "lifecycle.turn.started",
        Scope::default(),
        Json::obj([]),
    );
    t.scope.turn_id = Some("turn-1".to_string());
    let mut c = ev(
        "mc-open",
        "model.call.requested",
        Scope::default(),
        Json::obj([]),
    );
    c.scope = Scope {
        turn_id: Some("turn-1".to_string()),
        model_call_id: Some("mc-1".to_string()),
        ..Scope::default()
    };
    s.append(run, lease, vec![t, c]).unwrap();
}

/// A `proposed → completed` tool-call pair; `completed` carries the
/// `execution_ms` M-point the dispatcher stamps (hh-env), plus the declared
/// `(executor_class, isolation_class)` stratifiers.
fn tool_call(id: &str, tc: &str, exec_ms: i64, executor: &str, isolation: &str) -> Vec<Event> {
    let scope = Scope {
        turn_id: Some("turn-1".to_string()),
        model_call_id: Some("mc-1".to_string()),
        tool_call_id: Some(tc.to_string()),
        ..Scope::default()
    };
    vec![
        ev(
            &format!("{id}-p"),
            "action.tool.proposed",
            scope.clone(),
            Json::obj([]),
        ),
        ev(
            &format!("{id}-c"),
            "action.tool.completed",
            scope,
            Json::obj([
                ("status", Json::str("ok")),
                ("execution_ms", Json::Int(exec_ms)),
                ("executor_class", Json::str(executor)),
                ("isolation_class", Json::str(isolation)),
            ]),
        ),
    ]
}

fn metric_cell<'a>(v: &'a hh_ledger::views::View, name: &str) -> &'a Json {
    v.payload
        .get("metrics")
        .and_then(|m| m.get(name))
        .expect("metric cell present")
}

#[test]
fn execution_ms_folds_to_per_stratum_distributions() {
    let (mut s, run, lease) = open("dist");
    open_scopes(&mut s, &run, &lease);
    let mut events = Vec::new();
    // Two strata: `sandbox_helper/process` ×3 samples, `kernel_internal/none`
    // ×1; plus one cache-served completion with no M-point (no sample).
    for (i, ms) in [10i64, 20, 40].iter().enumerate() {
        events.extend(tool_call(
            &format!("e{i}"),
            &format!("tc-{i}"),
            *ms,
            "sandbox_helper",
            "process",
        ));
    }
    events.extend(tool_call("e9", "tc-9", 5, "kernel_internal", "none"));
    events.push(ev(
        "e-cache",
        "action.tool.completed",
        Scope::default(),
        Json::obj([("status", Json::str("ok"))]),
    ));
    s.append(&run, &lease, events).unwrap();

    let declared: BTreeSet<Observability> = [Observability::Events, Observability::Ledger]
        .into_iter()
        .collect();
    let v = metric_view(&run, &run, &declared, &read_all(&s, &run), None);
    let cell = metric_cell(&v, "harness_overhead.execution_ms");

    let helper = cell.get("sandbox_helper/process").expect("helper stratum");
    assert_eq!(helper.get("n").and_then(Json::as_int), Some(3));
    assert_eq!(helper.get("min").and_then(Json::as_int), Some(10));
    assert_eq!(helper.get("p50").and_then(Json::as_int), Some(20));
    assert_eq!(helper.get("p95").and_then(Json::as_int), Some(40));
    assert_eq!(helper.get("max").and_then(Json::as_int), Some(40));

    let kern = cell.get("kernel_internal/none").expect("kernel stratum");
    assert_eq!(kern.get("n").and_then(Json::as_int), Some(1));
    assert_eq!(kern.get("p50").and_then(Json::as_int), Some(5));

    // The cache-served completion contributed no sample (absent ≠ 0).
    assert!(cell.get("unknown/unknown").is_none());

    // Deterministic fold — a second render is byte-identical.
    let v2 = metric_view(&run, &run, &declared, &read_all(&s, &run), None);
    assert_eq!(v.payload, v2.payload);
}

#[test]
fn execution_ms_renders_typed_na() {
    let (mut s, run, lease) = open("na");
    open_scopes(&mut s, &run, &lease);
    // No measured samples → n/a{estimator_undefined}, never an empty map or 0.
    let declared: BTreeSet<Observability> = [Observability::Events, Observability::Ledger]
        .into_iter()
        .collect();
    let v = metric_view(&run, &run, &declared, &read_all(&s, &run), None);
    assert_eq!(
        metric_cell(&v, "harness_overhead.execution_ms"),
        &Json::obj([("na", Json::str("estimator_undefined"))])
    );

    // A hosted-grade declaration (`events` only — `ledger ∉` per §5d.5 §8)
    // → n/a{observability}.
    let hosted: BTreeSet<Observability> = [Observability::Events].into_iter().collect();
    let v = metric_view(&run, &run, &hosted, &read_all(&s, &run), None);
    assert_eq!(
        metric_cell(&v, "harness_overhead.execution_ms"),
        &Json::obj([("na", Json::str("observability"))])
    );

    // A measured row exists but the run lacks `ledger` → still n/a{observability}.
    s.append(
        &run,
        &lease,
        tool_call("e0", "tc-0", 7, "sandbox_helper", "process"),
    )
    .unwrap();
    let v = metric_view(&run, &run, &hosted, &read_all(&s, &run), None);
    assert_eq!(
        metric_cell(&v, "harness_overhead.execution_ms"),
        &Json::obj([("na", Json::str("observability"))])
    );
}
