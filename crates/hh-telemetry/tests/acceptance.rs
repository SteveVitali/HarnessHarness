//! S1.14 acceptance — R-2.9.1 (telemetry, tracing & cost/latency
//! instrumentation). Each test is named for its acceptance criterion
//! (AC-R-2.9.1-8 / -11 / -13) plus the universal phase gate (the workspace
//! suite itself). Fixtures drive the *real* ledger path — `Store::open_test`,
//! `append`, `read` — so every assertion runs over durable envelopes, never a
//! mock telemetry store (there is none — ADR-0042 D2).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::attribution::{Attribution, ModelRef};
use hh_budget::pricing::{CostProvenance, Derivation, ProvenanceClass, SpendRow};
use hh_budget::quantity::Money;
use hh_ledger::event::{Cursor, Direction, Event, EventEnvelope, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ontology::participant::Observability;
use hh_telemetry::catalogue::{self, check_catalogue};
use hh_telemetry::clocks::{skewed, DEFAULT_CLOCK_TOLERANCE_MS};
use hh_telemetry::events::{export_delivered_from_json, Timing};
use hh_telemetry::export::{deliver, export_events, export_view};
use hh_telemetry::propagation::{outbound_context, propagation_unsupported_loss, subprocess_env};
use hh_telemetry::sinks::{ContentClass, Sampling, SamplingMode, SinkPolicy};
use hh_telemetry::tokens::{normalize_usage, ProviderUsage, TokenVector};
use hh_telemetry::views::{cost_view, metric_view, sink_deliveries, trace_view};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-telemetry-ac-{}-{tag}-{n}", std::process::id()));
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

/// An audit-grade event (kernel producer + provenance — the class table's
/// `audit_grade` rule for `model.call.*`, `action.tool.*`, `measurement.*`).
fn ev(id: &str, class: &str, ts: &str, scope: Scope, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: ts.to_string(),
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

fn spend_row(run: &str, src_event: &str, micro: i64) -> Json {
    hh_budget::events::cost_attributed_payload(&SpendRow {
        resource: hh_ontology::dimensions::DimensionId::Spend,
        money: Money {
            micro_units: micro,
            currency: "USD".into(),
        },
        provenance: CostProvenance::ProviderReported,
        provenance_class: ProvenanceClass::Measured,
        derivation: Derivation::ProviderPriced {
            unit: "call".into(),
            rate_ref: None,
        },
        confidence: hh_budget::pricing::Confidence::Exact,
        coverage_ppm: hh_budget::quantity::PPM_SCALE,
        pricing_ref: None,
        source_event: EventRef {
            run_id: run.into(),
            event_id: src_event.into(),
        },
        model_ref: ModelRef {
            profile_ref: "profile:test".into(),
            provider_model_id: "m-1".into(),
            serving_route: "route-a".into(),
            effort: None,
        },
        attribution: Attribution::subject(run, "budget-1", "participant-0"),
        roles: BTreeMap::new(),
    })
}

/// A model call with one attempt — the fixture every test builds on.
/// `timing` carries the monotonic stamps; `usage` the normalized
/// `TokenVector`.
fn model_call(tag: &str, scope_turn: &str, timing: (i64, i64, i64, i64)) -> Vec<Event> {
    let mc = format!("mc-{tag}");
    let usage = TokenVector {
        input_total: 1100,
        input_uncached: 100,
        cache_read: 900,
        cache_write: 100,
        output_total: 40,
        output_reasoning: 10,
        output_text: None,
        convention: hh_telemetry::tokens::TOKEN_CONVENTION.into(),
        normalizer_ref: "norm:test".into(),
    };
    let (rs, fb, ft, lb) = timing;
    let timing_j = Timing {
        request_sent_ms: rs,
        first_byte_ms: Some(fb),
        first_token_ms: Some(ft),
        last_byte_ms: lb,
    }
    .to_json();
    vec![
        ev(
            &format!("turn-{tag}"),
            "lifecycle.turn.started",
            TS,
            Scope {
                turn_id: Some(scope_turn.into()),
                ..Scope::default()
            },
            Json::Null,
        ),
        ev(
            &format!("req-{tag}"),
            "model.call.requested",
            TS,
            Scope {
                turn_id: Some(scope_turn.into()),
                model_call_id: Some(mc.clone()),
                ..Scope::default()
            },
            // audit-grade ⇒ Rule-C partitioned object payload.
            Json::obj([]),
        ),
        ev(
            &format!("at0-{tag}"),
            "model.call.attempt.started",
            TS,
            Scope {
                turn_id: Some(scope_turn.into()),
                model_call_id: Some(mc.clone()),
                ..Scope::default()
            },
            Json::obj([
                ("attempt_no", Json::Int(0)),
                ("timing", timing_j.clone()),
                ("measured_at", Json::str("adapter")),
            ]),
        ),
        ev(
            &format!("atc-{tag}"),
            "model.call.attempt.completed",
            TS,
            Scope {
                turn_id: Some(scope_turn.into()),
                model_call_id: Some(mc.clone()),
                ..Scope::default()
            },
            Json::obj([
                ("attempt_no", Json::Int(0)),
                ("timing", timing_j),
                ("measured_at", Json::str("adapter")),
            ]),
        ),
        ev(
            &format!("done-{tag}"),
            "model.call.completed",
            TS,
            Scope {
                turn_id: Some(scope_turn.into()),
                model_call_id: Some(mc),
                ..Scope::default()
            },
            Json::obj([
                ("usage", usage.to_json()),
                ("latency_ms", Json::Int(lb - rs)),
                ("measured_at", Json::str("adapter")),
            ]),
        ),
    ]
}

fn declared_all() -> BTreeSet<Observability> {
    [
        Observability::Events,
        Observability::ModelIo,
        Observability::EndState,
        Observability::Ledger,
    ]
    .into_iter()
    .collect()
}

// ── AC-R-2.9.1-8 ────────────────────────────────────────────────────────────
// A sink declared {accounting} never receives a byte of any ContentAddress
/// payload — a fixture with a secret in a tool result exports only lengths and
/// digests; a {content} sink requires `requires_consent = true` satisfied in
/// the manifest (Stage-1 schema; the executable gate lands at Stage 3).
#[test]
fn ac_r_2_9_1_8_accounting_sinks_never_receive_content_and_content_needs_consent() {
    // The fixture: a tool result carrying a secret and an offloaded blob ref.
    let secret = "sk-live-9f8e7d6c5b4a-SECRET";
    let digest = "sha256:deadbeefcafe";
    let payload = Json::obj([
        ("result_ref", Json::str(digest)),
        ("result_bytes", Json::Int(4096)),
        ("result", Json::str(secret)),
    ]);
    let (mut s, run, lease) = open("ac8");
    s.append(
        &run,
        &lease,
        vec![ev(
            "t-done",
            "action.tool.completed",
            TS,
            Scope::default(),
            payload.clone(),
        )],
    )
    .unwrap();
    let events = read_all(&s, &run);

    // (a) {accounting}: no byte of the payload — no secret, no digest values
    // beyond the structural members the class level carries.
    let acct = SinkPolicy::accounting("acct");
    let batch = export_events(&acct, &events).unwrap();
    let wire = batch
        .rows
        .iter()
        .map(|r| r.to_canonical_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!wire.contains(secret), "L0 row leaked the secret");
    assert!(!wire.contains("result"), "L0 row leaked the member name");
    assert!(!wire.contains(digest), "L0 row leaked the address");

    // {accounting, structural}: digests/lengths may appear (L1 carries the
    // envelope's structural members) but the payload bytes never do.
    let mut structural = SinkPolicy::accounting("struct");
    structural.content_classes.insert(ContentClass::Structural);
    let batch = export_events(&structural, &events).unwrap();
    let wire = batch
        .rows
        .iter()
        .map(|r| r.to_canonical_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!wire.contains(secret), "L1 row leaked the secret payload");

    // (b) `{content}` ⇒ `requires_consent = true` — the schema half refuses.
    let mut content = SinkPolicy::accounting("content-sink");
    content.content_classes.insert(ContentClass::Content);
    content.requires_consent = false;
    assert!(matches!(
        content.validate(),
        Err(hh_telemetry::TelemetryError::ContentRequiresConsent { .. })
    ));
    // A content sink that *declares* consent still needs the manifest's grant.
    content.requires_consent = true;
    let b = export_events(&content, &events).unwrap();
    assert!(matches!(
        deliver(
            &mut s,
            &run,
            &lease,
            &content,
            &BTreeSet::new(),
            "event_batch",
            &b
        ),
        Err(hh_telemetry::TelemetryError::ConsentMissing { .. })
    ));
    // Granted ⇒ the delivery appends `measurement.export.delivered`, readable
    // back from the ledger (the only ledger write an exporter may make).
    let granted: BTreeSet<String> = ["content-sink".to_string()].into_iter().collect();
    deliver(&mut s, &run, &lease, &content, &granted, "event_batch", &b).unwrap();
    let deliveries = sink_deliveries(&read_all(&s, &run));
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].sink_id, "content-sink");
}

// ── AC-R-2.9.1-11 ───────────────────────────────────────────────────────────
// Under any SinkPolicy.sampling, read(run) returns every ledger-durable event;
// metric_view values are unchanged by the sink configuration.
#[test]
fn ac_r_2_9_1_11_sampling_never_touches_the_ledger_and_views_ignore_sinks() {
    let (mut s, run, lease) = open("ac11");
    s.append(&run, &lease, model_call("a", "t1", (100, 150, 160, 400)))
        .unwrap();
    s.append(
        &run,
        &lease,
        vec![ev(
            "cost-1",
            "measurement.cost.attributed",
            TS,
            Scope::default(),
            spend_row(&run, "req-a", 2_500),
        )],
    )
    .unwrap();
    let before = read_all(&s, &run);
    let n = before.len();

    // An aggressive sampled sink exports a strict subset…
    let mut sampled = SinkPolicy::accounting("sampled");
    sampled.sampling = Sampling {
        mode: SamplingMode::Ratio,
        ratio_ppm: Some(1),
    };
    let batch = export_events(&sampled, &before).unwrap();
    assert!(batch.rows.len() < n);

    // …and the ledger read is untouched — every durable event returns.
    let after = read_all(&s, &run);
    assert_eq!(after, before, "sink configuration mutated the ledger");
    assert_eq!(after.len(), n);

    // metric_view is a pure fold — no SinkPolicy input exists, and two
    // rebuilds agree byte-for-byte (the same `view_hash`).
    let v1 = metric_view(&run, &run, &declared_all(), &after, None);
    let v2 = metric_view(&run, &run, &declared_all(), &after, None);
    assert_eq!(v1.view_hash, v2.view_hash);
    // Sink configuration cannot alter the metric values (there is no sink
    // parameter to pass — and a metric_view export under sampling refuses).
    let mut mp = SinkPolicy::accounting("msink");
    mp.sampling = Sampling {
        mode: SamplingMode::Ratio,
        ratio_ppm: Some(500_000),
    };
    assert!(matches!(
        export_view(&mp, &v1),
        Err(hh_telemetry::TelemetryError::SamplingForbidden { .. })
    ));
    let m = v1.payload.get("metrics").unwrap();
    assert_eq!(m.get("model_calls"), Some(&Json::Int(1)));
    assert!(m.get("cost_money").unwrap().get("USD") == Some(&Json::Int(2_500)));
}

// ── AC-R-2.9.1-13 ───────────────────────────────────────────────────────────
// Durations in payloads come from monotonic clocks; a fixture with a stepped
// wall clock yields correct durations and a clock_skew_flag.
#[test]
fn ac_r_2_9_1_13_monotonic_durations_and_clock_skew_flag() {
    let (mut s, run, lease) = open("ac13");
    // A stepped-back wall clock: the attempt's *closing* envelopes carry `ts`
    // 10s *earlier* than the open — the ts difference is negative while the
    // monotonic `timing` says 300ms.
    let late = "2025-12-31T23:59:50.000Z"; // before TS — a stepped clock
    let mut events = model_call("skew", "t1", (1_000, 1_100, 1_120, 1_300));
    for e in events.iter_mut() {
        if e.class == "model.call.attempt.completed" || e.class == "model.call.completed" {
            e.ts = late.to_string();
        }
    }
    s.append(&run, &lease, events).unwrap();
    let all = read_all(&s, &run);

    let view = trace_view(&run, &run, &all, None, DEFAULT_CLOCK_TOLERANCE_MS);
    let spans = view.payload.get("spans").unwrap();
    let Json::Arr(spans) = spans else { panic!() };
    let attempt = spans
        .iter()
        .find(|s| s.get("scope_kind").and_then(Json::as_str) == Some("model_attempt"))
        .expect("the model_attempt span exists");
    // The duration is the monotonic `last_byte − request_sent` = 300ms…
    assert_eq!(attempt.get("duration_ms"), Some(&Json::Int(300)));
    assert_eq!(
        attempt.get("duration_source"),
        Some(&Json::str("timing_pair"))
    );
    // …never the stepped wall `ts` — and the disagreement is flagged.
    assert_eq!(attempt.get("clock_skew_flag"), Some(&Json::Bool(true)));
    let skew = attempt.get("skew_ms").unwrap().as_int().unwrap();
    assert_eq!(skew, 300 - (-10_000));

    // The raw helpers pin the convention: `ts` orders nothing, skew is
    // `|payload − ts_span| > tolerance`.
    assert!(skewed(300, -10_000, DEFAULT_CLOCK_TOLERANCE_MS));
    assert!(!skewed(300, 304, DEFAULT_CLOCK_TOLERANCE_MS));
}

// ── supporting coverage (every behavior fails if removed) ───────────────────

/// `trace_view` is a pure fold over the durable prefix — identical rebuilds,
/// spans for the whole wired taxonomy, structural parents, open residue.
#[test]
fn trace_view_folds_spans_deterministically() {
    let (mut s, run, lease) = open("tv");
    s.append(&run, &lease, model_call("x", "t1", (0, 50, 60, 200)))
        .unwrap();
    let all = read_all(&s, &run);

    let v1 = trace_view(&run, &run, &all, None, DEFAULT_CLOCK_TOLERANCE_MS);
    let v2 = trace_view(&run, &run, &all, None, DEFAULT_CLOCK_TOLERANCE_MS);
    assert_eq!(v1.view_hash, v2.view_hash, "INV-3 — rebuilds agree");

    let Json::Arr(spans) = v1.payload.get("spans").unwrap() else {
        panic!()
    };
    let kinds: BTreeSet<&str> = spans
        .iter()
        .filter_map(|s| s.get("scope_kind").and_then(Json::as_str))
        .collect();
    // run (auto `lifecycle.run.created`), model_call, model_attempt.
    assert!(kinds.contains("run"));
    assert!(kinds.contains("model_call"));
    assert!(kinds.contains("model_attempt"));
    // The attempt's parent is the model call span; the call's is the run.
    let attempt = spans
        .iter()
        .find(|s| s.get("scope_kind").and_then(Json::as_str) == Some("model_attempt"))
        .unwrap();
    assert_eq!(
        attempt.get("parent_scope").and_then(Json::as_str),
        Some("model_call:mc-x")
    );
    // The run span is open (no `lifecycle.run.finished` in the prefix).
    let run_span = spans
        .iter()
        .find(|s| s.get("scope_kind").and_then(Json::as_str) == Some("run"))
        .unwrap();
    assert_eq!(run_span.get("status"), Some(&Json::str("open")));
    assert_eq!(attempt.get("status"), Some(&Json::str("closed")));
}

/// `cost_view` preserves provenance per row — never averaged — and reports the
/// minimum confidence and coverage (AC-R-2.9.1-8's read side).
#[test]
fn cost_view_preserves_provenance_and_min_confidence() {
    let (mut s, run, lease) = open("cv");
    s.append(
        &run,
        &lease,
        vec![
            ev(
                "c1",
                "measurement.cost.attributed",
                TS,
                Scope::default(),
                spend_row(&run, "r1", 2_500),
            ),
            {
                let mut row = spend_row(&run, "r2", 1_000);
                if let Json::Obj(m) = &mut row {
                    m.insert(
                        "provenance".into(),
                        Json::str("reconstructed_from_native_log"),
                    );
                    m.insert("provenance_class".into(), Json::str("reconstructed"));
                    m.insert("confidence".into(), Json::str("estimate"));
                }
                ev(
                    "c2",
                    "measurement.cost.attributed",
                    TS,
                    Scope::default(),
                    row,
                )
            },
        ],
    )
    .unwrap();
    let all = read_all(&s, &run);
    let v = cost_view(&run, &run, &all, None);
    let subject = v
        .payload
        .get("subjects")
        .and_then(|s| s.get("budget-1"))
        .expect("the budget-1 subject row");
    // Both provenance classes survive — a mix, never a merge.
    let mix = subject.get("provenance_mix").unwrap();
    assert_eq!(mix.get("measured"), Some(&Json::Int(1)));
    assert_eq!(mix.get("reconstructed"), Some(&Json::Int(1)));
    // The minimum confidence wins.
    assert_eq!(
        subject.get("confidence_min").and_then(Json::as_str),
        Some("estimate")
    );
    assert_eq!(
        v.payload
            .get("total_spend_micro")
            .and_then(|t| t.get("USD")),
        Some(&Json::Int(3_500))
    );
}

/// `metric_view` honours `requires_observability` — a run declared `events`
/// renders model_io-requiring metrics `n/a{observability}`, never 0.
#[test]
fn metric_view_renders_typed_na_under_insufficient_observability() {
    let (mut s, run, lease) = open("mv");
    s.append(&run, &lease, model_call("o", "t1", (0, 10, 20, 100)))
        .unwrap();
    let all = read_all(&s, &run);
    let events_only: BTreeSet<Observability> = [Observability::Events].into_iter().collect();
    let v = metric_view(&run, &run, &events_only, &all, None);
    let m = v.payload.get("metrics").unwrap();
    // ttft_ms reads `model.call.attempt.*` (min model_io) → n/a.
    assert_eq!(
        m.get("ttft_ms")
            .and_then(|c| c.get("na"))
            .and_then(Json::as_str),
        Some("observability")
    );
    // model_calls reads events-level classes → a real value.
    assert_eq!(m.get("model_calls"), Some(&Json::Int(1)));
    // A declared-but-not-computable row renders its typed n/a.
    assert_eq!(
        m.get("audit_completeness")
            .and_then(|c| c.get("na"))
            .and_then(Json::as_str),
        Some("observability") // {events} < {events, ledger}
    );
    let v2 = metric_view(&run, &run, &declared_all(), &all, None);
    let m2 = v2.payload.get("metrics").unwrap();
    assert_eq!(
        m2.get("audit_completeness")
            .and_then(|c| c.get("na"))
            .and_then(Json::as_str),
        Some("not_run")
    );
    // ttft computes under {model_io}: first_token − request_sent = 20ms.
    assert_eq!(m2.get("ttft_ms"), Some(&Json::Int(20)));
}

/// The catalogue + the `requires_observability` registry check (DF-S1.5-2 —
/// the metric-registry half of AC-R-2.2.1-5).
#[test]
fn the_metric_catalogue_resolves_against_the_class_table() {
    check_catalogue().unwrap();
    assert!(catalogue::metric("tokens_total").is_some());
    assert!(catalogue::metric("egress_blocked").is_some());
}

/// TokenVector normalization — the four provider usage shapes converge on
/// `hh-inclusive/1` with `normalizer_ref` mandatory (§2.6).
#[test]
fn token_normalization_converges_provider_shapes() {
    let a = normalize_usage(
        &ProviderUsage::Flat {
            prompt_tokens: 100,
            completion_tokens: 40,
        },
        "norm:a",
    )
    .unwrap();
    assert_eq!(a.input_total, 100);
    assert_eq!(a.normalizer_ref, "norm:a");
    // The vector projects onto the primary §08 dimensions.
    let rv = a.to_resource_vector();
    assert_eq!(
        rv.get(hh_ontology::dimensions::DimensionId::TokensInputUncached),
        100
    );
}

/// Propagation: the subprocess env carries the W3C pair; an unsupported target
/// reports loss, never silence.
#[test]
fn propagation_renders_and_reports_loss_honestly() {
    let ctx = outbound_context("root-run", "r1", "e1", None);
    let env = subprocess_env(&ctx);
    assert!(env
        .iter()
        .any(|(k, v)| k == "HH_TRACEPARENT" && v.starts_with("00-")));
    let loss = propagation_unsupported_loss("mcp_meta");
    assert!(loss.contains("mcp_meta"));
}

/// `measurement.export.delivered` — appended through `Store::append`,
/// readable back, strict codec (BadMember on unknown members).
#[test]
fn export_delivered_round_trips_through_the_ledger() {
    let (mut s, run, lease) = open("ed");
    let p = SinkPolicy::accounting("acct");
    let batch = export_events(&p, &read_all(&s, &run)).unwrap();
    deliver(
        &mut s,
        &run,
        &lease,
        &p,
        &BTreeSet::new(),
        "event_batch",
        &batch,
    )
    .unwrap();
    let all = read_all(&s, &run);
    let rows = sink_deliveries(&all);
    assert_eq!(rows.len(), 1);
    // The durable payload decodes strictly — unknown members refuse.
    let env = all
        .iter()
        .find(|e| e.class == "measurement.export.delivered")
        .unwrap();
    let mut m = match env.payload.clone() {
        Json::Obj(m) => m,
        _ => panic!(),
    };
    m.insert("bogus".into(), Json::Null);
    assert!(export_delivered_from_json(&Json::Obj(m)).is_err());
}

/// AC-R-2.5.2-12 (declaration half — §5d.2; S1.17): `surface_rejection_rate`
/// is a registered `MetricDeclaration` with `applies_to_classes = {native}`
/// and `requires_observability ⊇ {ledger}`, computed from
/// `surface_rejected ÷ model-emitted calls` (computation lands at C0/S3 —
/// the catalogue row is the C0/S1 half).
#[test]
fn ac_e2_12_surface_rejection_rate_is_registered() {
    let m = catalogue::metric("surface_rejection_rate").expect("registered");
    assert_eq!(
        m.applies_to,
        &[hh_ontology::participant::ParticipantClass::Native]
    );
    assert!(m.requires_observability.contains(&Observability::Ledger));
    assert_eq!(m.unit, catalogue::MetricUnit::Ppm);
    assert_eq!(
        m.computed_from,
        &["action.tool.surface_rejected", "action.tool.proposed"]
    );
    // The declaration shape is the §2.6.3 `MetricDeclaration`.
    let d = m.declaration();
    assert_eq!(d.name, "surface_rejection_rate");
    // The exposure family declared with it (ADR-0094 D6).
    for name in [
        "tool_surface_tokens",
        "catalog_exposure_ratio",
        "discovery_calls",
        "unrevealed_call_rate",
        "catalog_drift_events",
        "epochs_adopted",
    ] {
        assert!(catalogue::metric(name).is_some(), "{name} registered");
    }
}

// ── AC-R-2.2.3-12 (§5a.3 §8; S3.6) ────────────────────────────────────────────
// The five recovery metrics fold over the durable recovery rows:
// `recovery_latency_ms` pairs the takeover `lease.acquired{scope: writer}` with
// `lifecycle.run.resumed.resumed_at_ms`; `wasted_calls` counts calls the crash
// left open at `from_seq`; `unknown_effects_per_resume` is the ppm rate of
// recovery-emitted `unknown` rows; `heal_count` and `wakeups_skipped{reason}`
// count theirs. Every value is computed — never fabricated.
#[test]
fn ac_r_2_2_3_12_recovery_metrics_fold_the_recovery_rows() {
    let (mut s, run, lease) = open("recovery-metrics");
    let rc = Json::obj([
        ("class", Json::str("irreversible")),
        ("reversibility", Json::str("irreversible")),
        ("repeat_safety", Json::str("non_idempotent")),
        ("scope", Json::str("external")),
    ]);
    let eff_scope = |eid: &str| Scope {
        effect_id: Some(eid.to_string()),
        ..Scope::default()
    };
    // The crash-wasted call — requested, never closed before `from_seq`.
    // `ef-1` is committed but not dispatched when the worker dies.
    s.append(
        &run,
        &lease,
        vec![
            ev(
                "e-t1",
                "lifecycle.turn.started",
                TS,
                Scope {
                    turn_id: Some("turn-1".into()),
                    ..Scope::default()
                },
                Json::obj([("turn_id", Json::str("turn-1"))]),
            ),
            ev(
                "e-mc",
                "model.call.requested",
                TS,
                Scope {
                    turn_id: Some("turn-1".into()),
                    model_call_id: Some("mc-1".into()),
                    ..Scope::default()
                },
                Json::obj([
                    ("model_call_id", Json::str("mc-1")),
                    ("attempt_no", Json::Int(1)),
                ]),
            ),
            ev(
                "e-i1",
                "action.effect.intended",
                TS,
                eff_scope("ef-1"),
                Json::obj([
                    ("effect_id", Json::str("ef-1")),
                    ("effective_risk_class", rc.clone()),
                ]),
            ),
            ev(
                "e-a1",
                "action.effect.authorized",
                TS,
                eff_scope("ef-1"),
                Json::obj([("effective_risk_class", rc)]),
            ),
            ev(
                "e-d1",
                "security.permission.decided",
                TS,
                eff_scope("ef-1"),
                Json::obj([
                    ("effect_id", Json::str("ef-1")),
                    ("attempt_no", Json::Int(1)),
                    ("decision", Json::str("allow")),
                ]),
            ),
            ev(
                "e-p1",
                "action.effect.prepared",
                TS,
                eff_scope("ef-1"),
                Json::obj([("idempotency_key", Json::str("key-ef-1"))]),
            ),
            ev(
                "e-c1",
                "action.effect.committed",
                TS,
                eff_scope("ef-1"),
                Json::obj([
                    ("attempt_no", Json::Int(1)),
                    ("fencing_token", Json::Int(1)),
                ]),
            ),
        ],
    )
    .unwrap();
    let from_seq = read_all(&s, &run).last().unwrap().seq;

    // The takeover + resume + the restore's own resolution rows.
    s.append(
        &run,
        &lease,
        vec![
            ev(
                "e-la",
                "lifecycle.lease.acquired",
                TS,
                Scope::default(),
                Json::obj([
                    ("scope", Json::str("writer")),
                    ("lease_id", Json::str("lease-2")),
                    ("holder", Json::str("writer-b")),
                    ("generation", Json::Int(2)),
                    ("acquired_at_ms", Json::Int(1000)),
                    ("expires_at_ms", Json::Int(5000)),
                ]),
            ),
            ev(
                "e-rs",
                "lifecycle.run.resumed",
                TS,
                Scope::default(),
                Json::obj([
                    ("generation", Json::Int(2)),
                    ("from_seq", Json::Int(from_seq as i64)),
                    ("resumed_at_ms", Json::Int(1250)),
                ]),
            ),
            ev(
                "e-u1",
                "action.effect.unknown",
                TS,
                eff_scope("ef-1"),
                Json::obj([
                    ("cause", Json::str("worker_lost")),
                    ("fencing_token", Json::Int(1)),
                ]),
            ),
            ev(
                "e-heal",
                "action.environment.healed",
                TS,
                Scope::default(),
                Json::obj([("restored", Json::Bool(true))]),
            ),
            ev(
                "e-ws",
                "control.wakeup.skipped",
                TS,
                Scope::default(),
                Json::obj([("reason", Json::str("duplicate_occurrence"))]),
            ),
        ],
    )
    .unwrap();

    let all = read_all(&s, &run);
    let v = metric_view(&run, &run, &declared_all(), &all, None);
    let m = v.payload.get("metrics").unwrap().clone();
    // takeover at t=1000 → resume delivered at t=1250.
    assert_eq!(m.get("recovery_latency_ms"), Some(&Json::Int(250)));
    // mc-1 was open at from_seq and never closed.
    assert_eq!(m.get("wasted_calls"), Some(&Json::Int(1)));
    // one recovery-emitted `unknown` over one resume — ppm of 1.0.
    assert_eq!(
        m.get("unknown_effects_per_resume"),
        Some(&Json::Int(1_000_000))
    );
    assert_eq!(m.get("heal_count"), Some(&Json::Int(1)));
    assert_eq!(
        m.get("wakeups_skipped"),
        Some(&Json::obj([("duplicate_occurrence", Json::Int(1))]))
    );
    // The catalogue declarations carry the AC's shape (requires_observability
    // ⊇ {events}, applies_to = {native}) — the T-LCD-15 row.
    for name in [
        "recovery_latency_ms",
        "wasted_calls",
        "unknown_effects_per_resume",
        "heal_count",
        "wakeups_skipped",
    ] {
        let d = catalogue::metric(name).unwrap_or_else(|| panic!("{name} registered"));
        assert!(
            d.requires_observability.contains(&Observability::Events),
            "{name} requires {{events}}"
        );
    }
}

// ── AC-R-2.3.4-13 — the GenAI semantic-convention lowering (S3.7) ──────────

use hh_telemetry::genai::{attr, lower_run};

fn completed_payload() -> Json {
    Json::obj([
        ("model_call_id", Json::str("c1")),
        ("served_model", Json::str("m-a")),
        (
            "surface_ids",
            Json::obj([
                ("response_id", Json::str("resp-1")),
                ("request_id", Json::str("req-1")),
            ]),
        ),
        ("stop_reason", Json::str("stop")),
        (
            "usage",
            Json::obj([
                ("arrival", Json::str("provider_reported")),
                (
                    "record",
                    Json::obj([
                        ("raw", Json::obj([("prompt_tokens", Json::Int(210))])),
                        ("normalizer_ref", Json::str("norm:1")),
                        (
                            "view",
                            Json::obj([
                                ("input_total", Json::Int(200)),
                                ("output_total", Json::Int(50)),
                                ("cache_read", Json::Int(120)),
                                ("cache_write", Json::Int(30)),
                                ("provider_extra", Json::Int(7)),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "timing",
            Json::obj([("latency_ms", Json::Int(42)), ("ttft_ms", Json::Int(9))]),
        ),
        (
            "cache_observation",
            Json::obj([
                ("expected_state", Json::str("warm")),
                ("observed_state", Json::str("hit")),
                ("affinity_key", Json::str("aff-1")),
                ("static_hash", Json::str("sha256:ab")),
                ("hit_ratio_ppm", Json::Int(600_000)),
                ("miss_reason", Json::str("none")),
            ]),
        ),
        ("served_from_cache", Json::str("entry:1")),
        ("substitution", Json::obj([("declared", Json::str("m-b"))])),
        ("snapshot_id", Json::str("snap:1")),
    ])
}

fn resolved_payload() -> Json {
    Json::obj([
        ("cache_kind", Json::str("k4")),
        ("key", Json::str("sha256:k4key")),
        ("scope", Json::str("run")),
        ("outcome", Json::str("hit")),
        ("reason", Json::str("expired")),
        ("entry_ref", Json::str("entry:1")),
        (
            "avoided",
            Json::obj([
                ("spend", Json::obj([("micro_units", Json::Int(5))])),
                ("provenance", Json::str("estimated_from_pricing")),
            ]),
        ),
        ("attribution", Json::str("subject")),
        ("purpose", Json::str("main")),
        ("lookup_duration_ms", Json::Int(1)),
        ("served_by", Json::str("c2")),
    ])
}

#[test]
fn genai_lowering_carries_cache_roles_and_names_every_loss() {
    let completed = completed_payload();
    let resolved = resolved_payload();
    let reroute = Json::obj([("model_call_id", Json::str("c1"))]);
    let other = Json::obj([("x", Json::Int(1))]);
    let events: Vec<(u64, &str, &Json)> = vec![
        (1, "model.call.completed", &completed),
        (2, "model.cache.resolved", &resolved),
        (3, "model.rerouted", &reroute),
        (4, "run.started", &other), // outside the model-plane projection
    ];
    let out = lower_run(&events);

    // One span for the terminal; the resolved row is a named event loss.
    assert_eq!(out.spans.len(), 1);
    let span = &out.spans[0];
    assert_eq!(span.get("name"), Some(&Json::str("gen_ai.chat")));
    assert_eq!(span.get("span_id"), Some(&Json::str("c1")));
    assert_eq!(span.get("duration_ms"), Some(&Json::Int(42)));
    let a = span.get("attributes").expect("attributes");
    assert_eq!(a.get(attr::OPERATION), Some(&Json::str("chat")));
    assert_eq!(a.get(attr::RESPONSE_MODEL), Some(&Json::str("m-a")));
    assert_eq!(a.get(attr::RESPONSE_ID), Some(&Json::str("resp-1")));
    assert_eq!(
        a.get(attr::FINISH_REASONS),
        Some(&Json::Arr(vec![Json::str("stop")]))
    );
    assert_eq!(a.get(attr::INPUT_TOKENS), Some(&Json::Int(200)));
    assert_eq!(a.get(attr::OUTPUT_TOKENS), Some(&Json::Int(50)));
    // The cache roles lower to the GenAI spellings (§5b.4).
    assert_eq!(a.get(attr::CACHE_READ), Some(&Json::Int(120)));
    assert_eq!(a.get(attr::CACHE_WRITE), Some(&Json::Int(30)));

    // Every unrepresentable member is a named loss — nothing silently drops.
    let details: BTreeSet<&str> = out.loss.iter().map(|l| l.detail.as_str()).collect();
    for expected in [
        "model.call.completed.surface_ids.request_id",
        "model.call.completed.usage.arrival",
        "model.call.completed.usage.record.raw",
        "model.call.completed.usage.record.normalizer_ref",
        "model.call.completed.usage.record.view.provider_extra",
        "model.call.completed.timing.ttft_ms",
        "model.call.completed.cache_observation.expected_state",
        "model.call.completed.cache_observation.affinity_key",
        "model.call.completed.cache_observation.static_hash",
        "model.call.completed.cache_observation.observed_state",
        "model.call.completed.cache_observation.hit_ratio_ppm",
        "model.call.completed.cache_observation.miss_reason",
        "model.call.completed.served_from_cache",
        "model.call.completed.substitution",
        "model.call.completed.snapshot_id",
        "model.cache.resolved",               // unrepresentable_event
        "model.cache.resolved.miss_reason",   // `reason` spelled per the AC
        "model.cache.resolved.avoided.spend", // avoided cost is declared loss
        "model.cache.resolved.key",
        "model.rerouted", // unrepresentable_event
    ] {
        assert!(
            details.contains(expected),
            "missing loss entry `{expected}`"
        );
    }
    assert!(out
        .loss
        .iter()
        .all(|l| l.kind == "unrepresentable_member" || l.kind == "unrepresentable_event"));
    // The loss report ref is present and deterministic.
    let r1 = out.loss_report_ref().expect("losses produce a report ref");
    assert_eq!(r1, lower_run(&events).loss_report_ref().unwrap());
    assert!(r1.starts_with("sha256:"));
    assert_eq!(out.seq_range, (1, 3), "non-model rows are out of scope");
}

#[test]
fn genai_lowering_failed_call_and_empty_run() {
    let failed = Json::obj([
        ("model_call_id", Json::str("c9")),
        (
            "error",
            Json::obj([
                ("class", Json::str("infrastructure_failure")),
                ("detail", Json::str("boom")),
            ]),
        ),
        ("timing", Json::obj([("latency_ms", Json::Int(3))])),
        (
            "usage",
            Json::obj([("input_total", Json::Int(10)), ("cache_read", Json::Int(4))]),
        ),
        ("served_model", Json::str("m-a")), // unrepresentable on a failed call
    ]);
    let out = lower_run(&[(7, "model.call.failed", &failed)]);
    assert_eq!(out.spans.len(), 1);
    let a = out.spans[0].get("attributes").unwrap();
    assert_eq!(
        a.get(attr::ERROR_TYPE),
        Some(&Json::str("infrastructure_failure"))
    );
    assert_eq!(a.get(attr::CACHE_READ), Some(&Json::Int(4)));
    let details: BTreeSet<&str> = out.loss.iter().map(|l| l.detail.as_str()).collect();
    assert!(details.contains("model.call.failed.error.detail"));
    assert!(details.contains("model.call.failed.served_model"));

    // An empty/zero-loss run has no report ref (never an empty list).
    let empty = lower_run(&[]);
    assert_eq!(empty.seq_range, (0, 0));
    assert!(empty.loss_report_ref().is_none());
}

// ---------------------------------------------------------------------------
// AC-R-2.7.3-10 — `gen_ai.evaluation.result` round-trips name/score/label/
// explanation/response id; the loss report names judge identity, evidence,
// calibration, independence and cost.
// ---------------------------------------------------------------------------

#[test]
fn genai_verdict_lowering_roundtrips_and_names_loss_classes() {
    use hh_telemetry::genai::lower_verdict;
    let verdict = Json::obj([
        ("verdict_id", Json::str("v-9")),
        ("validator_ref", Json::str("critic:reconciliation@1")),
        ("detector", Json::str("deterministic")),
        ("oracle_class", Json::str("deterministic")),
        ("target", Json::str("task:cap-1")),
        ("criterion_ref", Json::str("crit:loop-guard")),
        ("contract_id", Json::str("contract:t1")),
        ("phase", Json::str("post_completion")),
        ("role", Json::str("independent")),
        (
            "value",
            Json::obj([("kind", Json::str("bool")), ("value", Json::Bool(true))]),
        ),
        ("status", Json::str("decided")),
        (
            "evidence_refs",
            Json::Arr(vec![Json::str("sha256:ev-1"), Json::str("sha256:ev-2")]),
        ),
        ("inputs_digest", Json::str("sha256:in-1")),
        ("evidence_head_seq", Json::Int(41)),
        ("freshness_ok", Json::Bool(true)),
        (
            "findings",
            Json::Arr(vec![Json::obj([
                ("code", Json::str("loop_guard_passed")),
                ("severity", Json::str("info")),
                ("location", Json::Null),
                ("evidence_ref", Json::Null),
            ])]),
        ),
        ("cost_ppm", Json::Int(3_000)),
        ("charged_to", Json::str("instrument")),
        ("calibration_ref", Json::str("cal:t1")),
        ("independence_vector", Json::str("separate-model")),
        ("response_ref", Json::str("resp-77")),
    ]);
    let (event, loss) = lower_verdict(9, &verdict);
    assert_eq!(event.get("name").unwrap().as_str().unwrap(), "gen_ai.evaluation.result");
    let attrs = event.get("attributes").unwrap();
    assert_eq!(
        attrs.get("gen_ai.evaluation.name").unwrap().as_str().unwrap(),
        "crit:loop-guard"
    );
    assert_eq!(
        attrs
            .get("gen_ai.evaluation.score.label")
            .unwrap()
            .as_str()
            .unwrap(),
        "pass"
    );
    assert_eq!(
        attrs
            .get("gen_ai.evaluation.explanation")
            .unwrap()
            .as_str()
            .unwrap(),
        "loop_guard_passed"
    );
    assert_eq!(
        attrs.get("gen_ai.evaluation.status").unwrap().as_str().unwrap(),
        "decided"
    );
    assert_eq!(
        attrs.get("gen_ai.response.id").unwrap().as_str().unwrap(),
        "resp-77"
    );
    // The five named loss classes are all named on the loss report.
    let details: Vec<String> = loss.iter().map(|l| l.detail.clone()).collect();
    for named in [
        "validator_ref",         // judge identity
        "evidence_refs",         // evidence
        "calibration_ref",       // calibration
        "independence_vector",   // independence
        "cost_ppm",              // cost
    ] {
        assert!(
            details
                .iter()
                .any(|d| d == &format!("verification.validator.verdict.{named}")),
            "loss must name {named}: {details:?}"
        );
    }
    // Determinism: identical input → identical output.
    assert_eq!((event.clone(), loss.clone()), lower_verdict(9, &verdict));
    // A numeric score lowers to score.value.
    let v2 = Json::obj([
        ("verdict_id", Json::str("v-10")),
        ("target", Json::str("task:cap-2")),
        (
            "value",
            Json::obj([("kind", Json::str("int")), ("value", Json::Int(85))]),
        ),
        ("status", Json::str("decided")),
    ]);
    let (e2, _) = lower_verdict(10, &v2);
    let a2 = e2.get("attributes").unwrap();
    assert_eq!(a2.get("gen_ai.evaluation.score.value").unwrap().as_int(), Some(85));
    assert_eq!(
        a2.get("gen_ai.evaluation.name").unwrap().as_str().unwrap(),
        "task:cap-2"
    );
    assert_eq!(
        a2.get("gen_ai.response.id").unwrap().as_str().unwrap(),
        "v-10"
    );
}
