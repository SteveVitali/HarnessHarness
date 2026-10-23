//! R4.7c — the OTLP live-sink seam (ADR-0346 trigger (a); BL-31; R-2.9.1):
//! `SinkWire::OtlpTraces` exports the durable delta's `gen_ai`-lowered
//! convention spans as an OTLP `ExportTraceServiceRequest` through the
//! *real* `EgressMediator` + a real loopback OTLP receiver fixture (the
//! fixture-verified ceiling — `LocalHttpTransport` over `127.0.0.1`).
//! `measurement.export.delivered` lands only on the sink's 2xx
//! acknowledgement; a refused destination stays an egress fact; a
//! forwarded-but-rejected delivery mints no row.
//!
//! The mediator construction mirrors `tests/r2_14.rs` verbatim — the
//! same tokens/policy/broker machinery; the OTLP cells add the
//! `model.call.*` rows the span projection consumes.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use hh_containment::egress::ApprovalCache;
use hh_containment::policy::{
    kernel_default, AmendmentBasis, ContainmentPolicy, DefaultUnmatched, EgressRule, HostPattern,
    NetMode, NonPublic, RuleDecision,
};
use hh_embed::subscriber::{
    ExportSubscriber, PollOutcome, SinkDestination, SinkWire, SubscriberEgress,
};
use hh_env::egress::{EgressMediator, LocalHttpTransport, StaticResolver};
use hh_env::events::{intended_payload, EventMinter, ScopeChain};
use hh_env::tokens::{TokenMinter, TokenResolver};
use hh_ledger::event::Scope;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
use hh_provenance::PersistenceScope;
use hh_secrets::{CredentialBroker, DenyAllResolver};
use hh_telemetry::propagation::{span_id, trace_id};
use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r47c-{}-{tag}-{n}", std::process::id()));
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

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec!["POST".into()],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn mediated_policy(rules: Vec<EgressRule>) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Mediated;
    p.net.rules = rules;
    p.net.default_unmatched = DefaultUnmatched::Deny;
    p.net.non_public_destinations = NonPublic::AllowListed;
    p.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Approval]);
    p.amendment.session_cache = true;
    p.amendment.persist_scope_ceiling = PersistenceScope::Run;
    p.compute_ids();
    p
}

/// The export effect's scope chain — the egress leg appends its
/// `security.egress.*` rows under `mc-eg`/`tc-eg`, which stay open;
/// `mc-1` (the *exported* call) is already closed by its terminal row.
fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-eg".into(),
        tool_call_id: "tc-eg".into(),
    }
}

/// The scope members + a model.call pair the span projection consumes —
/// `model.call.requested` is an `unrepresentable_event` loss on this
/// wire, `model.call.completed` lowers to the `gen_ai.chat` span and
/// *closes* `mc-1`. The export effect rides its own `mc-eg`/`tc-eg`
/// scope (kept open — the egress leg appends under it).
/// Returns `(effect_id, token, resolver, completed_event_id)`.
fn open_scopes(
    store: &mut Store,
    run: &str,
    lease: &Lease,
) -> (String, String, TokenResolver, String) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    // The call's terminal — the row the `gen_ai.chat` span lowers from
    // (and the row that closes the `mc-1` scope).
    let mut done = m
        .mint(
            "model.call.completed",
            Json::obj([
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
                            ]),
                        )]),
                    )]),
                ),
                ("timing", Json::obj([("latency_ms", Json::Int(321))])),
            ]),
        )
        .unwrap();
    done.scope = c.scope.clone();
    let done_id = done.event_id.clone();
    // The export effect's own scope members — `mc-eg` opens (never
    // closes), so the egress leg's security rows append under it.
    let mut c2 = m.mint("model.call.requested", Json::obj([])).unwrap();
    c2.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-eg".into()),
        ..Scope::default()
    };
    let mut p = m.mint("action.tool.proposed", Json::obj([])).unwrap();
    p.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-eg".into()),
        tool_call_id: Some("tc-eg".into()),
        ..Scope::default()
    };
    let risk = RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::External,
    };
    let eid = Store::effect_id(run, "mc-eg", "tc-eg", 0);
    let e = m
        .mint_effect(
            "action.effect.intended",
            intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            &eid,
            &chain(),
        )
        .unwrap();
    store
        .append(run, lease, vec![t, c, done, c2, p, e])
        .unwrap();
    let mut tm = TokenMinter::new([7u8; 32]);
    let tok = tm.mint(&eid, 1, "env-1");
    (eid, tok.token, tm.resolver(), done_id)
}

/// Read a full request: head + the declared `Content-Length` body
/// (the r2_14 loopback-flake rule — consume the request before
/// answering).
fn read_request(s: &mut std::net::TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(head_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let declared: usize = String::from_utf8_lossy(&buf[..head_end])
                        .split("\r\n")
                        .skip(1)
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if buf.len() >= head_end + 4 + declared {
                        break;
                    }
                }
            }
        }
    }
    buf
}

/// The loopback OTLP receiver fixture — captures the raw POST, answers
/// `status` with a JSON body (a real collector's ack/reject shape).
fn otlp_fixture(status: u16) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut served = 0;
        while served < 8 && std::time::Instant::now() < deadline {
            let Ok((mut s, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            served += 1;
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            let buf = read_request(&mut s);
            cap.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).to_string());
            let resp = format!(
                "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{{\"status\":1}}",
                if status == 200 { "OK" } else { "ERR" }
            );
            let _ = s.write_all(resp.as_bytes());
        }
    });
    (port, captured)
}

fn class_count(store: &Store, run_id: &str, class: &str) -> usize {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .count()
}

fn subscriber(run: &str, port: u16, eid: &str, token: &str) -> ExportSubscriber {
    ExportSubscriber {
        run_id: run.to_string(),
        policy: SinkPolicy::accounting("sink-otel"),
        consents: BTreeSet::new(),
        watermark: 0,
        destination: SinkDestination {
            host: "127.0.0.1".into(),
            port,
            path: "/v1/traces".into(),
            wire: SinkWire::OtlpTraces {
                service_name: "hh.exporter".into(),
            },
        },
        egress: SubscriberEgress {
            token: token.to_string(),
            effect_id: eid.to_string(),
            env_handle: "env-1".into(),
            tool_call_id: "tc-eg".into(),
        },
    }
}

fn mediator<'a>(
    store: &'a mut Store,
    run: &str,
    lease: &'a Lease,
    tokens: TokenResolver,
    broker: &'a mut CredentialBroker,
    rules: Vec<EgressRule>,
) -> EgressMediator<'a> {
    EgressMediator {
        store,
        run_id: run.to_string(),
        lease,
        tokens,
        broker,
        policy: mediated_policy(rules),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.exporter".into(),
        resolver: Box::new(StaticResolver {
            map: BTreeMap::from([("127.0.0.1".to_string(), vec!["127.0.0.1".parse().unwrap()])]),
        }),
        transport: Box::new(LocalHttpTransport::default()),
        inspect_hooks: BTreeMap::new(),
    }
}

/// `attr(span, key)` — one OTLP `attributes[]` member's `value`.
fn attr<'a>(span: &'a Json, key: &str) -> Option<&'a Json> {
    let Json::Arr(attrs) = span.get("attributes")? else {
        return None;
    };
    attrs
        .iter()
        .find(|kv| kv.get("key").and_then(Json::as_str) == Some(key))
        .and_then(|kv| kv.get("value"))
}

#[test]
fn otlp_acknowledged_delivery_lands_delivered_and_a_valid_request() {
    let (mut store, run, lease) = open("otlp-ack");
    let (eid, token, tokens, done_id) = open_scopes(&mut store, &run, &lease);
    let (port, captured) = otlp_fixture(200);
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = mediator(
        &mut store,
        &run,
        &lease,
        tokens,
        &mut broker,
        vec![allow_rule("127.0.0.1")],
    );
    let mut sub = subscriber(&run, port, &eid, &token);

    let out = sub.poll(&mut med, &chain()).unwrap();
    let (delivered, seq_range) = match out {
        PollOutcome::Delivered {
            seq,
            seq_range,
            loss_report_ref,
        } => {
            // `model.call.requested` is unrepresentable on this wire —
            // the loss rides the delivered row.
            assert!(loss_report_ref.is_some(), "the lowering loss rides");
            (seq, seq_range)
        }
        other => panic!("expected Delivered, got {other:?}"),
    };
    assert!(seq_range.1 >= seq_range.0);

    // The OTLP/HTTP body crossed the loopback wire — a valid
    // ExportTraceServiceRequest whose spans name the run's trace id
    // and lift coordinates.
    let got = captured.lock().unwrap();
    assert_eq!(got.len(), 1, "exactly one POST crossed the wire");
    assert!(got[0].starts_with("POST /v1/traces"), "the OTLP path");
    let body = got[0].split("\r\n\r\n").nth(1).expect("request body");
    let doc = hh_wire::json::parse(body).expect("the OTLP body parses");
    let Json::Arr(rs) = doc.get("resourceSpans").unwrap() else {
        panic!("resourceSpans[]")
    };
    assert_eq!(rs.len(), 1, "one resource span group");
    // `service.name` — the collector attributes the spans to the
    // subscriber's configured service.
    let Json::Arr(rattrs) = rs[0]
        .get("resource")
        .and_then(|r| r.get("attributes"))
        .unwrap()
    else {
        panic!("resource.attributes[]")
    };
    assert!(rattrs.iter().any(
        |kv| kv.get("key").and_then(Json::as_str) == Some("service.name")
            && kv
                .get("value")
                .and_then(|v| v.get("stringValue"))
                .and_then(Json::as_str)
                == Some("hh.exporter")
    ));
    let Json::Arr(ss) = rs[0].get("scopeSpans").unwrap() else {
        panic!("scopeSpans[]")
    };
    let Json::Arr(spans) = ss[0].get("spans").unwrap() else {
        panic!("spans[]")
    };
    // One `gen_ai.chat` span — the completed call (requested lowered to
    // loss; the scope/lifecycle rows are outside the projection).
    assert_eq!(spans.len(), 1, "one convention span exported");
    let span = &spans[0];
    assert_eq!(
        span.get("traceId").and_then(Json::as_str),
        Some(trace_id(&run).as_str())
    );
    assert_eq!(
        span.get("spanId").and_then(Json::as_str),
        Some(span_id(&done_id).as_str())
    );
    assert_eq!(span.get("name").and_then(Json::as_str), Some("gen_ai.chat"));
    assert_eq!(
        attr(span, "hh.event_id")
            .and_then(|v| v.get("stringValue"))
            .and_then(Json::as_str),
        Some(done_id.as_str()),
        "the lift coordinate rides the wire"
    );
    assert_eq!(
        attr(span, "gen_ai.usage.input_tokens")
            .and_then(|v| v.get("intValue"))
            .and_then(Json::as_str),
        Some("100")
    );
    drop(got);

    // The durable record: `security.egress.decided{allow}` +
    // `measurement.export.delivered` — acked delivery only.
    let delivered_rows: Vec<_> = med
        .store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "measurement.export.delivered")
        .cloned()
        .collect();
    assert_eq!(delivered_rows.len(), 1);
    assert_eq!(delivered_rows[0].seq, delivered);
    assert_eq!(
        delivered_rows[0]
            .payload
            .get("sink_id")
            .and_then(Json::as_str),
        Some("sink-otel")
    );
    assert!(class_count(med.store, &run, "security.egress.decided") >= 1);
}

#[test]
fn otlp_rejected_delivery_is_not_acknowledged_never_a_delivered_row() {
    let (mut store, run, lease) = open("otlp-reject");
    let (eid, token, tokens, _done_id) = open_scopes(&mut store, &run, &lease);
    let (port, captured) = otlp_fixture(400);
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = mediator(
        &mut store,
        &run,
        &lease,
        tokens,
        &mut broker,
        vec![allow_rule("127.0.0.1")],
    );
    let mut sub = subscriber(&run, port, &eid, &token);

    let out = sub.poll(&mut med, &chain()).unwrap();
    match out {
        PollOutcome::NotAcknowledged {
            decided_ref,
            status,
        } => {
            assert_eq!(status, 400);
            assert!(!decided_ref.is_empty());
        }
        other => panic!("expected NotAcknowledged, got {other:?}"),
    }
    // The wire fired (decided{allow} durable) but the sink rejected —
    // no `measurement.export.delivered`, and the watermark stays so the
    // next poll re-attempts honestly.
    assert_eq!(
        class_count(med.store, &run, "measurement.export.delivered"),
        0
    );
    assert!(class_count(med.store, &run, "security.egress.decided") >= 1);
    assert_eq!(captured.lock().unwrap().len(), 1, "the wire did fire");
}

#[test]
fn unlisted_otlp_sink_is_egress_refusal_never_a_delivered_row() {
    let (mut store, run, lease) = open("otlp-deny");
    let (eid, token, tokens, _done_id) = open_scopes(&mut store, &run, &lease);
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = mediator(
        &mut store,
        &run,
        &lease,
        tokens,
        &mut broker,
        vec![], // no allow rule — `default_unmatched: deny`
    );
    let mut sub = subscriber(&run, 4318, &eid, &token);

    let out = sub.poll(&mut med, &chain()).unwrap();
    match out {
        PollOutcome::Refused {
            decided_ref,
            detail,
        } => {
            assert!(detail.contains("egress denied"), "{detail}");
            assert!(!decided_ref.is_empty());
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(
        class_count(med.store, &run, "measurement.export.delivered"),
        0
    );
    assert!(class_count(med.store, &run, "security.egress.decided") >= 1);
}

#[test]
fn model_free_delta_is_quiet_and_consumed() {
    let (mut store, run, lease) = open("otlp-quiet");
    // Only the run-open row — no model.* rows at all → nothing
    // projects; the watermark still advances (consumed, never sent).
    let m = EventMinter::new(&store, &run);
    let e = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    store.append(&run, &lease, vec![e]).unwrap();
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = mediator(
        &mut store,
        &run,
        &lease,
        TokenMinter::new([9u8; 32]).resolver(),
        &mut broker,
        vec![allow_rule("127.0.0.1")],
    );
    let (port, captured) = otlp_fixture(200);
    let mut sub = subscriber(&run, port, "eff-1", "tok-1");

    let out = sub.poll(&mut med, &chain()).unwrap();
    assert!(matches!(out, PollOutcome::Quiet), "{out:?}");
    assert!(sub.watermark > 0, "the consumed delta advances");
    assert!(captured.lock().unwrap().is_empty(), "no wire fired");
    assert_eq!(
        class_count(med.store, &run, "measurement.export.delivered"),
        0
    );
}
