//! R2.14 acceptance — the Stage-2 telemetry halves (DF-S1.14-1's residual:
//! `turn_phase_profile`, the propagation `_meta` carriers, the
//! `{value, measured_at}` duration resolution + scope coordinates) and
//! DF-S1.14-2's residual (the OpenInference convention sink + `lift_span`
//! round-trip, the hosted `n/a` sweep, per-configuration catalogue
//! distributions). Every fixture drives the real fold — durable envelopes
//! through `Store` where a writer is needed, the pure view functions
//! otherwise (ADR-0042: no second telemetry store).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::event::{Cursor, Direction, Event, EventEnvelope, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::views::ViewKind;
use hh_ontology::participant::{Observability, ParticipantClass};
use hh_telemetry::genai;
use hh_telemetry::lift::lift_span;
use hh_telemetry::openinfer as oi;
use hh_telemetry::phase::turn_phase_profile;
use hh_telemetry::propagation::{
    acp_hh_members, carrier_loss, inbound_carrier, meta_carrier, outbound_context,
    MCP_PROPAGATION_KEY,
};
use hh_telemetry::views::{catalogue_distributions, metric_view, metric_view_scoped, trace_view};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "hh-telemetry-r214-{}-{tag}-{n}",
        std::process::id()
    ));
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
        event_id: id.into(),
        class: class.into(),
        ts: TS.into(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope,
        parent_event_id: ROOT_EVENT.into(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

// ── DF-S1.14-1: the propagation `_meta` carriers (§5h.1 §2.3) ───────────

#[test]
fn mcp_meta_carrier_round_trips_through_inbound_carrier() {
    let ctx = outbound_context("root-1", "run-1", "event-1", Some("cfg-9"));
    let carrier = meta_carrier(&ctx);
    assert_eq!(
        carrier.get("traceparent").and_then(Json::as_str),
        Some(ctx.traceparent.as_str())
    );
    assert!(carrier
        .get("tracestate")
        .and_then(Json::as_str)
        .is_some_and(|t| t.contains("hh=")));
    assert_eq!(
        carrier.get("baggage").and_then(Json::as_str),
        Some("hh.configuration_id=cfg-9")
    );
    // The lift half resolves the ledger coordinates — the `hh` member is
    // the only path (foreign contexts never graft).
    let link = inbound_carrier(&carrier).unwrap().expect("carrier present");
    assert_eq!(link.run_id.as_deref(), Some("run-1"));
    assert_eq!(link.event_id.as_deref(), Some("event-1"));
    assert_eq!(link.configuration_id.as_deref(), Some("cfg-9"));
    // The MCP slot key is the `dev.cognition/*` namespace's.
    assert_eq!(MCP_PROPAGATION_KEY, "dev.cognition/propagation");
}

#[test]
fn acp_hh_members_carry_the_flat_spelling() {
    let ctx = outbound_context("root-1", "run-1", "event-1", Some("cfg-9"));
    let members: BTreeMap<String, Json> = acp_hh_members(&ctx).into_iter().collect();
    assert_eq!(
        members.get("traceparent").and_then(Json::as_str),
        Some(ctx.traceparent.as_str())
    );
    assert!(members
        .get("tracestate")
        .and_then(Json::as_str)
        .is_some_and(|t| t.contains("hh=run:run-1;ev:event-1")));
    // The baggage's `hh.configuration_id=` value lands under its own
    // member name (the slot's flat rule).
    assert_eq!(
        members.get("configuration_id").and_then(Json::as_str),
        Some("cfg-9")
    );
    // And `inbound_carrier` reads the flat set back too.
    let link = inbound_carrier(&Json::Obj(members))
        .unwrap()
        .expect("carrier present");
    assert_eq!(link.run_id.as_deref(), Some("run-1"));
}

#[test]
fn inbound_carrier_refuses_malformed_and_absent_members() {
    // No carrier — `None`, not an error (an uninstrumented member is
    // absent, not malformed).
    assert_eq!(inbound_carrier(&Json::obj([])).unwrap(), None);
    // A malformed `traceparent` is refused — the same honesty gate
    // `inbound_context` applies.
    let bad = Json::obj([("traceparent", Json::str("not-a-traceparent"))]);
    assert!(inbound_carrier(&bad).is_err());
    // A foreign (valid but hh-less) context lifts aliases only.
    let foreign = Json::obj([
        (
            "traceparent",
            Json::str("00-0123456789abcdef0123456789abcdef-0123456789abcdef-01"),
        ),
        ("tracestate", Json::str("other=x")),
    ]);
    let link = inbound_carrier(&foreign).unwrap().expect("valid carrier");
    assert_eq!(link.run_id, None);
    assert_eq!(link.event_id, None);
}

#[test]
fn carrier_loss_spells_dropped_members() {
    // `PropagationUnsupported` per member — the loss-report spelling.
    assert_eq!(carrier_loss("acp", "baggage"), "propagation.acp.baggage");
    assert_eq!(
        carrier_loss("mcp", "tracestate"),
        "propagation.mcp.tracestate"
    );
}

// ── DF-S1.14-1: `turn_phase_profile` (the M2 phase partition) ────────────

#[test]
fn turn_phase_profile_partitions_the_wall() {
    let (mut s, run, lease) = open("phase");
    let events = vec![
        ev(
            "e-ts",
            "lifecycle.turn.started",
            Scope {
                turn_id: Some("turn-1".into()),
                ..Scope::default()
            },
            Json::obj([("started_at_ms", Json::Int(1000))]),
        ),
        ev(
            "e-req",
            "model.call.requested",
            Scope {
                turn_id: Some("turn-1".into()),
                model_call_id: Some("mc-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("at_ms", Json::Int(1100)),
            ]),
        ),
        ev(
            "e-att2",
            "model.call.attempt.started",
            Scope {
                turn_id: Some("turn-1".into()),
                model_call_id: Some("mc-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("attempt_no", Json::Int(2)),
            ]),
        ),
        ev(
            "e-done",
            "model.call.completed",
            Scope {
                turn_id: Some("turn-1".into()),
                model_call_id: Some("mc-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("at_ms", Json::Int(1400)),
            ]),
        ),
        ev(
            "e-perm",
            "security.permission.decided",
            Scope {
                turn_id: Some("turn-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("requested_at", Json::Int(1500)),
                (
                    "wait_ms",
                    Json::obj([
                        ("value", Json::Int(80)),
                        ("measured_at", Json::str("runtime")),
                    ]),
                ),
            ]),
        ),
        ev(
            "e-tf",
            "lifecycle.turn.finished",
            Scope {
                turn_id: Some("turn-1".into()),
                ..Scope::default()
            },
            Json::obj([("turn_e2e_ms", Json::Int(900))]),
        ),
    ];
    s.append(&run, &lease, events).unwrap();
    let v = turn_phase_profile(&run, "turn-1", &read_all(&s, &run), 5);
    assert_eq!(v.kind, ViewKind::TurnPhaseProfile);
    assert_eq!(
        v.payload.get("turn_e2e_ms").and_then(Json::as_int),
        Some(900)
    );
    assert_eq!(
        v.payload.get("sampling_requests").and_then(Json::as_int),
        Some(1)
    );
    // `attempt_no: 2` on one call = one retry (durable fact).
    assert_eq!(
        v.payload.get("sampling_retries").and_then(Json::as_int),
        Some(1)
    );
    // The permission wait bounded [1500, 1580] inside the wall.
    let perm = v.payload.get("permission_wait_ms").expect("phase member");
    assert_ne!(perm.get("na"), Some(&Json::str("observability")));
    // Deterministic — a rebuilt fold is byte-identical.
    let v2 = turn_phase_profile(&run, "turn-1", &read_all(&s, &run), 5);
    assert_eq!(v.payload, v2.payload);
    assert_eq!(v.view_hash, v2.view_hash);
}

#[test]
fn turn_phase_profile_unbounded_renders_typed_na() {
    let (mut s, run, lease) = open("phase-na");
    // A turn whose boundary stamps are absent — the counts stay honest,
    // every `*_ms` cell is `n/a{observability}` (never 0, never a ts
    // derivation).
    let events = vec![
        ev(
            "e-ts",
            "lifecycle.turn.started",
            Scope {
                turn_id: Some("turn-1".into()),
                ..Scope::default()
            },
            Json::obj([]),
        ),
        ev(
            "e-tf",
            "lifecycle.turn.finished",
            Scope {
                turn_id: Some("turn-1".into()),
                ..Scope::default()
            },
            Json::obj([]),
        ),
    ];
    s.append(&run, &lease, events).unwrap();
    let v = turn_phase_profile(&run, "turn-1", &read_all(&s, &run), 5);
    assert_eq!(v.kind, ViewKind::TurnPhaseProfile);
    assert_eq!(
        v.payload.get("sampling_requests").and_then(Json::as_int),
        Some(0)
    );
    for member in ["sampling_ms", "overhead_ms"] {
        let cell = v.payload.get(member);
        assert!(
            cell.is_none() || cell.and_then(|c| c.get("na")).is_some(),
            "{member} renders typed n/a when the wall is unbounded"
        );
    }
}

// ── DF-S1.14-1: duration resolution + scope coordinates ─────────────────

#[test]
fn trace_view_resolves_nested_measured_durations_and_scope_coords() {
    let (mut s, run, lease) = open("durations");
    let events = vec![
        ev(
            "e-turn",
            "lifecycle.turn.started",
            Scope {
                turn_id: Some("t1".into()),
                ..Scope::default()
            },
            Json::obj([("started_at_ms", Json::Int(0))]),
        ),
        // M3 — `timing.latency_ms` (a timing member, never a ts diff).
        ev(
            "e-mc",
            "model.call.requested",
            Scope {
                turn_id: Some("t1".into()),
                model_call_id: Some("mc-1".into()),
                ..Scope::default()
            },
            Json::obj([("model_call_id", Json::str("mc-1"))]),
        ),
        ev(
            "e-mcd",
            "model.call.completed",
            Scope {
                turn_id: Some("t1".into()),
                model_call_id: Some("mc-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                (
                    "timing",
                    Json::obj([
                        ("latency_ms", Json::Int(321)),
                        ("measured_at", Json::str("adapter")),
                    ]),
                ),
            ]),
        ),
        // M6 — `duration_ms` in `{value, measured_at}` form (started →
        // completed pair on the shared `compaction_id` coordinate).
        ev(
            "e-comp-s",
            "context.compaction.started",
            Scope {
                turn_id: Some("t1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("compaction_id", Json::str("comp-1")),
                ("at_ms", Json::Int(450)),
            ]),
        ),
        ev(
            "e-comp",
            "context.compaction.completed",
            Scope {
                turn_id: Some("t1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("compaction_id", Json::str("comp-1")),
                ("at_ms", Json::Int(500)),
                (
                    "duration_ms",
                    Json::obj([
                        ("value", Json::Int(42)),
                        ("measured_at", Json::str("runtime")),
                    ]),
                ),
            ]),
        ),
        // M10 — opened by the durable proposal (`intended`'s event id is
        // the coordinate), closed by `decided{proposal}`; `wait_ms`
        // plain-int.
        ev(
            "e-int",
            "action.effect.intended",
            Scope {
                turn_id: Some("t1".into()),
                effect_id: Some("fx-1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("effect_id", Json::str("fx-1")),
                (
                    "effective_risk_class",
                    Json::obj([
                        ("reversibility", Json::str("reversible")),
                        ("repeat_safety", Json::str("idempotent")),
                        ("scope", Json::str("workspace_local")),
                    ]),
                ),
                ("capability_version", Json::str("v-cap")),
                ("args_canonical_hash", Json::str("h")),
                ("ordinal", Json::Int(0)),
            ]),
        ),
        ev(
            "e-perm",
            "security.permission.decided",
            Scope {
                turn_id: Some("t1".into()),
                ..Scope::default()
            },
            Json::obj([
                ("proposal", Json::str("e-int")),
                ("requested_at", Json::Int(600)),
                ("wait_ms", Json::Int(30)),
            ]),
        ),
    ];
    s.append(&run, &lease, events).unwrap();
    let v = trace_view(&run, &run, &read_all(&s, &run), None, 5);
    let spans = match v.payload.get("spans") {
        Some(Json::Arr(s)) => s,
        _ => panic!("spans"),
    };
    let find = |point: &str| {
        spans
            .iter()
            .find(|s| s.get("measurement_point").and_then(Json::as_str) == Some(point))
            .unwrap_or_else(|| panic!("span {point}"))
    };
    // M3 resolves `timing.latency_ms = 321` — the member, never ts.
    assert_eq!(
        find("M3").get("duration_ms").and_then(Json::as_int),
        Some(321)
    );
    assert_eq!(
        find("M3").get("duration_source").and_then(Json::as_str),
        Some("timing_member")
    );
    // M6 resolves the nested `{value}` — 42.
    assert_eq!(
        find("M6").get("duration_ms").and_then(Json::as_int),
        Some(42)
    );
    // M10 resolves `wait_ms` — 30.
    assert_eq!(
        find("M10").get("duration_ms").and_then(Json::as_int),
        Some(30)
    );
}

// ── DF-S1.14-2: the OpenInference convention sink + lift-back ────────────

#[test]
fn openinference_lowering_names_loss_and_lifts_back() {
    let payload = Json::obj([
        ("model_call_id", Json::str("mc-1")),
        ("served_model", Json::str("m-fixture")),
        ("stop_reason", Json::str("stop")),
        (
            "usage",
            Json::obj([(
                "record",
                Json::obj([(
                    "view",
                    Json::obj([
                        ("input_total", Json::Int(100)),
                        ("output_total", Json::Int(40)),
                        ("cache_read", Json::Int(12)),
                    ]),
                )]),
            )]),
        ),
        ("timing", Json::obj([("latency_ms", Json::Int(321))])),
        ("expected_state", Json::str("s0")), // no OI spelling — named loss
    ]);
    let out = oi::lower_run_refs(&[(7, "run-1", "event-mc", "model.call.completed", &payload)]);
    assert_eq!(out.spans.len(), 1);
    let span = &out.spans[0];
    let attrs = span.get("attributes").unwrap();
    assert_eq!(
        attrs.get(oi::attr::SPAN_KIND).and_then(Json::as_str),
        Some("LLM")
    );
    assert_eq!(
        attrs.get(oi::attr::TOKENS_PROMPT).and_then(Json::as_int),
        Some(100)
    );
    assert_eq!(
        attrs
            .get(oi::attr::TOKENS_COMPLETION)
            .and_then(Json::as_int),
        Some(40)
    );
    assert_eq!(
        attrs.get(oi::attr::MODEL_NAME).and_then(Json::as_str),
        Some("m-fixture")
    );
    // The cache role + the un-carried member are named loss — nothing
    // silently dropped (CC3).
    assert!(out.loss.iter().any(|l| l.detail.contains("cache_read")));
    assert!(out
        .loss
        .iter()
        .any(|l| l.detail == "model.call.completed.expected_state"));
    // The lift-back — the `hh.*` carrier resolves the EventRef.
    let r = lift_span(span).expect("liftable span");
    assert_eq!(r.run_id, "run-1");
    assert_eq!(r.event_id, "event-mc");
    // A span without the carrier lifts `None` (foreign, never grafted).
    let foreign = Json::obj([("attributes", Json::obj([]))]);
    assert_eq!(lift_span(&foreign), None);
    assert!(out.loss_report_ref().is_some());
}

#[test]
fn genai_liftable_leg_stamps_the_same_carrier() {
    let payload = Json::obj([
        ("model_call_id", Json::str("mc-1")),
        ("timing", Json::obj([("latency_ms", Json::Int(9))])),
    ]);
    let out = genai::lower_run_refs(&[(3, "run-2", "event-7", "model.call.completed", &payload)]);
    let r = lift_span(&out.spans[0]).expect("gen_ai span lifts");
    assert_eq!(
        (r.run_id.as_str(), r.event_id.as_str()),
        ("run-2", "event-7")
    );
}

// ── DF-S1.14-2: the hosted `n/a` sweep + per-configuration distributions ─

#[test]
fn hosted_metric_view_renders_na_class_for_native_only_metrics() {
    let (s, run, _lease) = open("hosted-na");
    let hosted: BTreeSet<Observability> = [Observability::Events].into_iter().collect();
    let v = metric_view_scoped(
        &run,
        &run,
        ParticipantClass::Hosted,
        &hosted,
        &read_all(&s, &run),
        None,
    );
    // A native-only metric on a hosted run — `n/a{class}`, never a
    // computed reading, never 0.
    for name in ["scheduling.decision_count", "recovery_latency_ms"] {
        let metrics = v.payload.get("metrics").and_then(|m| m.get(name));
        assert_eq!(
            metrics.and_then(|c| c.get("na")).and_then(Json::as_str),
            Some("class"),
            "{name} is n/a{{class}} on a hosted participant"
        );
    }
    // A BOTH-applicable metric still gates on observability.
    assert_eq!(
        v.payload
            .get("metrics")
            .and_then(|m| m.get("harness_overhead.execution_ms"))
            .and_then(|c| c.get("na"))
            .and_then(Json::as_str),
        Some("observability")
    );
    // `participant_class` is stamped on the view.
    assert_eq!(
        v.payload.get("participant_class").and_then(Json::as_str),
        Some("hosted")
    );
    // The pre-R2.14 spelling is byte-identical for native runs (the
    // `applies_to` gate is a no-op).
    let native_v = metric_view(&run, &run, &hosted, &read_all(&s, &run), None);
    let scoped_v = metric_view_scoped(
        &run,
        &run,
        ParticipantClass::Native,
        &hosted,
        &read_all(&s, &run),
        None,
    );
    assert_eq!(native_v.payload, scoped_v.payload);
}

#[test]
fn catalogue_distributions_fold_per_configuration() {
    let metrics_a = Json::obj([
        ("model_calls", Json::Int(4)),
        (
            "loop_stop_rate",
            Json::obj([("na", Json::str("estimator_undefined"))]),
        ),
    ]);
    let metrics_b = Json::obj([
        ("model_calls", Json::Int(8)),
        ("loop_stop_rate", Json::Int(500_000)),
    ]);
    let metrics_c = Json::obj([("model_calls", Json::Int(2))]);
    let out = catalogue_distributions(
        "cfg-1",
        &[
            ("run-a".to_string(), metrics_a.clone()),
            ("run-c".to_string(), metrics_c.clone()),
            ("run-b".to_string(), metrics_b.clone()),
        ],
    );
    assert_eq!(out.get("run_count").and_then(Json::as_int), Some(3));
    let mc = out
        .get("metrics")
        .and_then(|m| m.get("model_calls"))
        .unwrap();
    assert_eq!(mc.get("n").and_then(Json::as_int), Some(3));
    assert_eq!(mc.get("min").and_then(Json::as_int), Some(2));
    assert_eq!(mc.get("p50").and_then(Json::as_int), Some(4));
    assert_eq!(mc.get("max").and_then(Json::as_int), Some(8));
    // The `n/a` cell counted under its reason — never folded as 0.
    let lsr = out
        .get("metrics")
        .and_then(|m| m.get("loop_stop_rate"))
        .unwrap();
    assert_eq!(lsr.get("n").and_then(Json::as_int), Some(1));
    assert_eq!(
        lsr.get("na")
            .and_then(|n| n.get("estimator_undefined"))
            .and_then(Json::as_int),
        Some(1)
    );
    // Deterministic under input order.
    let out2 = catalogue_distributions(
        "cfg-1",
        &[
            ("run-b".to_string(), metrics_b.clone()),
            ("run-a".to_string(), metrics_a.clone()),
            ("run-c".to_string(), metrics_c.clone()),
        ],
    );
    assert_eq!(out, out2);
}
