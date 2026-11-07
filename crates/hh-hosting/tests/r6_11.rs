//! Executable coverage for manifest ticket 264 / R6.11 —
//! PROXY-CORE-b + BUDGET (§4.2, §4.3, §4.4, §4.6, §4.8; design/33
//! F9/F10/F13/F16; REQ-XC-110/111/112/113/114/115/119/123/124/126/210/
//! 322, REQ-XC-004, REQ-XH-005/120). Every test fails if the behaviour
//! it names is removed.

use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hharness_budget::quantity::ResourceVector;
use hharness_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hharness_budget::tree::BudgetTree;
use hharness_budget::Account;
use hharness_hosting::proxy::budget::{
    feasibility_remaining_micros, refusal_is_premature, reservation_micros, slice_floor_micros,
    validate_slices, ProxyAuthorization, SliceViolation, WindowSpend,
};
use hharness_hosting::proxy::lane::{ProxyLane, Reservation, ReservationFormula};
use hharness_hosting::proxy::{Proxy, ProxyConfig};
use hharness_identity::idp::ContentAddress;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_ontology::dimensions::{DimensionId, DimensionKey};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::http::read_response;
use hharness_wire::json::{parse, Json};

const MODEL: &str = "glm-5.3-flash";
const RUN: &str = "phase-proxy";
const COMPONENT: &str = "hh-model-proxy/1";

// ─────────────────────────── fixtures ───────────────────────────

fn auth(cap: i64) -> ProxyAuthorization {
    auth_full(cap, cap, 0, vec![])
}

fn auth_full(
    authorized: i64,
    available: i64,
    m_ppm: i64,
    prior_windows: Vec<WindowSpend>,
) -> ProxyAuthorization {
    ProxyAuthorization {
        verbatim: "fixture authorization — R6.11 battery".into(),
        gate_record_ref: "gate.fixture".into(),
        authorized_micros: authorized,
        available_credit_micros: available,
        m_ppm,
        prior_windows,
    }
}

fn root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-r6-11-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[derive(Debug, Clone)]
struct Seen {
    target: String,
    body: String,
}

/// `(status, extra headers, content_type, body, delay_ms)` per request.
type Handler =
    Arc<dyn Fn(&Seen) -> (u16, Vec<(String, String)>, String, String, u64) + Send + Sync>;

fn fixture_upstream(handler: Handler) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
    let seen_w = seen.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { return };
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let Ok(Some((head, body))) = hharness_wire::http::read_request(&mut r, 8 << 20) else {
                return;
            };
            let s = Seen {
                target: head.target.clone(),
                body: String::from_utf8_lossy(&body).to_string(),
            };
            let (status, hdrs, ct, payload, delay) = handler(&s);
            seen_w.lock().unwrap().push(s);
            if delay > 0 {
                std::thread::sleep(Duration::from_millis(delay));
            }
            let mut resp = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: {ct}\r\ncontent-length: {}\r\n",
                payload.len()
            );
            for (k, v) in &hdrs {
                resp.push_str(&format!("{k}: {v}\r\n"));
            }
            resp.push_str(&format!("connection: close\r\n\r\n{payload}"));
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}/v1"), seen)
}

/// The canned happy-path chat response the R6.10 fixture used.
fn chat_ok() -> (u16, Vec<(String, String)>, String, String, u64) {
    (
        200,
        vec![],
        "application/json".into(),
        "{\"id\":\"fx\",\"object\":\"chat.completion\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":10,\"total_tokens\":50}}".into(),
        0,
    )
}

fn chat_fixture() -> (String, Arc<Mutex<Vec<Seen>>>) {
    fixture_upstream(Arc::new(|_| chat_ok()))
}

fn setup(
    tag: &str,
    _endpoint: &str,
    lane: ProxyLane,
    auth: ProxyAuthorization,
) -> (Proxy, String, PathBuf) {
    let store_root = root(tag);
    let cfg = ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane,
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth,
    };
    let (mut proxy, _bound) = Proxy::configure(cfg).unwrap();
    let token = proxy.register_slot("slot-A", "high", 2_000_000).unwrap();
    (proxy, token, store_root)
}

fn call(proxy: &mut Proxy, token: &str, body: &str) -> (u16, String, BTreeMap<String, String>) {
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let req = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: proxy.local\r\ncontent-type: application/json\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    for _ in 0..4000 {
        if proxy.tick() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut r = BufReader::new(s);
    let (head, rbody) = read_response(&mut r, 8 << 20).unwrap();
    (
        head.status,
        String::from_utf8_lossy(&rbody).to_string(),
        head.headers,
    )
}

/// Upstream exchanges on `/v1/*` (catalog probes hit `/api/*`).
fn v1(seen: &Arc<Mutex<Vec<Seen>>>) -> usize {
    seen.lock()
        .unwrap()
        .iter()
        .filter(|s| s.target.starts_with("/v1/"))
        .count()
}

fn chat_body(extra: &str) -> String {
    format!(
        "{{\"model\":\"anything\",\"messages\":[{{\"role\":\"user\",\"content\":\"hi\"}}]{extra}}}"
    )
}

fn formula_lane(endpoint: &str) -> ProxyLane {
    let mut l = ProxyLane::program(
        "lane.fixture",
        endpoint,
        "fixture.local",
        MODEL,
        Reservation::Formula(ReservationFormula::default()),
    );
    // The catalog probe is its own leg — every other test opts out
    // with a dead base so no probe exchanges land on the fixture.
    l.catalog_base = Some("http://127.0.0.1:1".to_string());
    l
}

fn blob_text(store_root: &PathBuf, sha_ref: &str) -> String {
    let store = Store::open(store_root).unwrap();
    let bytes = store
        .get_blob(&ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: sha_ref.strip_prefix("sha256:").unwrap().to_string(),
            media_type: "application/json".into(),
            size: 0,
        })
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

fn envs(store_root: &PathBuf) -> Vec<(String, Json)> {
    let store = Store::open(store_root).unwrap();
    store
        .envelopes(RUN)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

/// The terminal `model.call.*` payloads in order.
fn terminals(store_root: &PathBuf) -> Vec<Json> {
    envs(store_root)
        .into_iter()
        .filter(|(c, _)| c == "model.call.completed" || c == "model.call.failed")
        .map(|(_, p)| p)
        .collect()
}

/// The receipt blobs, in terminal order.
fn receipts(store_root: &PathBuf) -> Vec<Json> {
    terminals(store_root)
        .iter()
        .map(|t| {
            let r = t.get("response_ref").and_then(Json::as_str).unwrap();
            parse(&blob_text(store_root, r)).unwrap()
        })
        .collect()
}

fn proxy_member(receipt: &Json, k: &str) -> Json {
    receipt
        .get("proxy")
        .and_then(|p| p.get(k))
        .cloned()
        .unwrap_or(Json::Null)
}

/// AC-8's invariant over every durable prefix.
fn assert_invariant_all_prefixes(store_root: &PathBuf) {
    let store = Store::open(store_root).unwrap();
    let envs = store.envelopes(RUN).unwrap().to_vec();
    let spend = DimensionKey::Primary(DimensionId::Spend);
    for k in 1..=envs.len() {
        let t = BudgetTree::project(RUN.to_string(), &envs[..k]).unwrap();
        for n in t.nodes.values() {
            if let Some(hard) = n.node.spec.hard(spend) {
                let level = t.level(n, spend);
                assert!(
                    level <= hard,
                    "consumed + reserved {level} > hard {hard} at prefix {k}"
                );
            }
        }
    }
}

// ─────────────────────────── AC-1: release modes + deadline ───────────────────────────

#[test]
fn buffered_release_is_verbatim_and_deadline_maps_timeout() {
    // A 1 s lane-declared non-stream deadline vs a 3 s upstream stall →
    // upstream_timeout, surface-shaped 504 (REQ-XC-111, §4.8).
    let (endpoint, seen) = fixture_upstream(Arc::new(|s| {
        if s.target.contains("chat") {
            let mut r = chat_ok();
            r.4 = 3_000;
            r
        } else {
            chat_ok()
        }
    }));
    let mut lane = formula_lane(&endpoint);
    lane.nonstream_read_deadline_ms = Some(1_000);
    let (mut proxy, token, store_root) = setup("deadline", &endpoint, lane, auth(10_000_000));
    let (status, body, _h) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status, 504);
    assert!(body.contains("upstream_timeout"), "{body}");
    let t = terminals(&store_root);
    assert_eq!(t.len(), 1);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("upstream_timeout")
    );
    assert_eq!(v1(&seen), 1, "max_attempts = 1 — no proxy retry");
}

#[test]
fn buffered_heartbeat_heartbeats_then_buffered_bytes() {
    // REQ-XC-110/F9 — a 400 ms upstream stall emits `: hh-proxy`
    // comments ahead of the buffered SSE bytes; nothing else reaches
    // the client before the terminal row is durable.
    let sse = "data: {\"id\":\"x\",\"choices\":[]}\n\ndata: [DONE]\n\n";
    let (endpoint, _seen) = fixture_upstream(Arc::new(move |s| {
        if s.target.contains("chat") {
            (
                200,
                vec![],
                "text/event-stream".into(),
                sse.to_string(),
                400,
            )
        } else {
            chat_ok()
        }
    }));
    let mut lane = formula_lane(&endpoint);
    lane.heartbeat_ms = 30;
    let (mut proxy, token, store_root) = setup("heartbeat", &endpoint, lane, auth(10_000_000));
    let (status, body, hdrs) = call(&mut proxy, &token, &chat_body(",\"stream\":true"));
    assert_eq!(status, 200);
    assert_eq!(
        hdrs.get("content-type").map(String::as_str),
        Some("text/event-stream")
    );
    let hb = body.matches(": hh-proxy").count();
    assert!(hb >= 3, "expected ≥3 heartbeats, got {hb}: {body:?}");
    let first_hb = body.find(": hh-proxy").unwrap();
    let first_data = body.find("data:").unwrap();
    assert!(first_hb < first_data, "heartbeats precede response bytes");
    assert!(body.contains("data: [DONE]"));
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("forwarded")
    );
    assert_eq!(
        t[0].get("release_mode").and_then(Json::as_str),
        Some("buffered_heartbeat")
    );
    let r = receipts(&store_root);
    let rel = proxy_member(&r[0], "released");
    assert_eq!(
        rel.get("release_mode").and_then(Json::as_str),
        Some("buffered_heartbeat")
    );
    assert_eq!(rel.get("identical_to_upstream"), Some(&Json::Bool(true)));
}

#[test]
fn heartbeat_upstream_error_releases_in_stream_event() {
    // §4.8 — the head is committed; a late upstream failure releases
    // the surface's in-stream error event, receipted upstream_error.
    let (endpoint, _seen) = fixture_upstream(Arc::new(|s| {
        if s.target.contains("chat") {
            (
                502,
                vec![],
                "application/json".into(),
                "{\"error\":{\"type\":\"bad_gateway\"}}".into(),
                0,
            )
        } else {
            chat_ok()
        }
    }));
    let mut lane = formula_lane(&endpoint);
    lane.heartbeat_ms = 20;
    let (mut proxy, token, store_root) = setup("hb-err", &endpoint, lane, auth(10_000_000));
    let (_status, body, hdrs) = call(&mut proxy, &token, &chat_body(",\"stream\":true"));
    assert_eq!(
        hdrs.get("content-type").map(String::as_str),
        Some("text/event-stream")
    );
    assert!(body.contains("event: error"), "{body:?}");
    assert!(body.contains("upstream_error"), "{body:?}");
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("upstream_error")
    );
}

// ─────────────────────────── AC-2/AC-3: §4.8 taxonomy, one attempt ───────────────────────────

#[test]
fn upstream_429_passes_through_verbatim() {
    let (endpoint, seen) = fixture_upstream(Arc::new(|s| {
        if s.target.contains("chat") {
            (
                429,
                vec![("retry-after".to_string(), "9".to_string())],
                "application/json".into(),
                "{\"error\":{\"type\":\"rate_limit\",\"retry\":\"9s\"}}".into(),
                0,
            )
        } else {
            chat_ok()
        }
    }));
    let lane = formula_lane(&endpoint);
    let (mut proxy, token, store_root) = setup("r429", &endpoint, lane, auth(10_000_000));
    let (status, body, hdrs) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status, 429, "verbatim upstream status");
    assert_eq!(
        body, "{\"error\":{\"type\":\"rate_limit\",\"retry\":\"9s\"}}",
        "verbatim body"
    );
    assert_eq!(hdrs.get("retry-after").map(String::as_str), Some("9"));
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("upstream_error")
    );
    assert_eq!(v1(&seen), 1, "REQ-XC-123 — max_attempts = 1");
}

#[test]
fn broken_and_empty_streams_are_stream_broken() {
    // (a) a head-committed body truncated mid-chunk → stream_broken.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { return };
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let Ok(Some((_head, _body))) = hharness_wire::http::read_request(&mut r, 8 << 20)
            else {
                return;
            };
            // Content-Length 100, then close after 10 bytes — an EOF
            // mid-body is a broken stream, not a transport refusal.
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100\r\nconnection: close\r\n\r\n{\"partial\":",
            );
        }
    });
    let endpoint = format!("http://{addr}/v1");
    let lane = formula_lane(&endpoint);
    let (mut proxy, token, store_root) = setup("broken", &endpoint, lane, auth(10_000_000));
    let (status, body, _h) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status, 502);
    assert!(body.contains("stream_broken"), "{body}");
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("stream_broken")
    );
    // The partial capture is durable.
    let r = receipts(&store_root);
    let up = proxy_member(&r[0], "upstream");
    assert_eq!(up.get("complete"), Some(&Json::Bool(false)));
    drop(proxy);

    // (b) a streaming call whose 200 body carries no SSE events.
    let (endpoint, _seen) = fixture_upstream(Arc::new(|s| {
        if s.target.contains("chat") {
            (200, vec![], "application/json".into(), "{}".into(), 0)
        } else {
            chat_ok()
        }
    }));
    let mut lane = formula_lane(&endpoint);
    lane.heartbeat_ms = 20;
    let (mut proxy, token, store_root) = setup("empty", &endpoint, lane, auth(10_000_000));
    let (_status, body, _h) = call(&mut proxy, &token, &chat_body(",\"stream\":true"));
    assert!(body.contains("stream_broken"), "{body:?}");
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("stream_broken")
    );
}

// ─────────────────────────── AC-5: client abandonment ───────────────────────────

#[test]
fn client_gone_is_receipted_and_charged() {
    // REQ-XC-322 — a client that closes before release gets
    // `released: client_gone`; the charge still lands.
    let (endpoint, _seen) = chat_fixture();
    let lane = formula_lane(&endpoint);
    let (mut proxy, token, store_root) = setup("gone", &endpoint, lane, auth(10_000_000));
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let body = chat_body("");
    let req = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: proxy.local\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    drop(s); // abandon before release
    for _ in 0..4000 {
        if proxy.tick() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let r = receipts(&store_root);
    assert_eq!(r.len(), 1);
    let rel = proxy_member(&r[0], "released");
    assert_eq!(
        rel.get("status").and_then(Json::as_str),
        Some("client_gone"),
        "{rel:?}"
    );
    // charged — measurement.cost.attributed exists for the call.
    assert!(envs(&store_root)
        .iter()
        .any(|(c, _)| c == "measurement.cost.attributed"));
}

// ─────────────────────────── AC-4: bounded crash recovery ───────────────────────────

/// Seed a dead run: one open turn, root pool + slot slice, one held
/// `mc:seed-1` reservation with requested/started markers — the crash
/// artifact REQ-XC-124 settles. The writer lease expires at once so
/// the resumed proxy's takeover is a `lifecycle.lease.fenced` row.
fn seed_orphan(store_root: &PathBuf, reserved_micros: i64) -> (String, String) {
    let mut store = Store::open(store_root).unwrap();
    let mut manifest = RunManifest::minimal(RunKind::Experiment);
    manifest.configuration_id = None;
    manifest.configuration_version_id = None;
    manifest.environment_ref = None;
    manifest.environment_version_id = None;
    manifest.participant_class = ParticipantClass::Native;
    manifest.lease_ttl.writer_ms = 400;
    let (_m, lease) = store.open_run_with_id(RUN, manifest, COMPONENT).unwrap();
    let mk = |store: &mut Store, class: &str, payload: Json, scope: Scope| Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel(COMPONENT),
        scope,
        parent_event_id: store.head_event_id(RUN).unwrap_or_default(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(COMPONENT, store.now_ms())),
        content_kind: None,
        payload,
    };
    let turn_id = store.alloc_id("turn");
    let t_ev = mk(
        &mut store,
        "lifecycle.turn.started",
        Json::obj([("turn_id", Json::str(&turn_id))]),
        Scope {
            turn_id: Some(turn_id.clone()),
            ..Default::default()
        },
    );
    store.append(RUN, &lease, vec![t_ev]).unwrap();
    let mc_scope = Scope {
        turn_id: Some(turn_id.clone()),
        model_call_id: Some("seed-1".to_string()),
        ..Default::default()
    };
    let req_ev = mk(
        &mut store,
        "model.call.requested",
        Json::obj([("model_call_id", Json::str("seed-1"))]),
        mc_scope.clone(),
    );
    let att_ev = mk(
        &mut store,
        "model.call.attempt.started",
        Json::obj([
            ("model_call_id", Json::str("seed-1")),
            ("attempt_no", Json::Int(1)),
            ("queue_wait_ms", Json::Int(0)),
        ]),
        mc_scope.clone(),
    );
    store.append(RUN, &lease, vec![req_ev, att_ev]).unwrap();
    let spend = DimensionKey::parse("spend").unwrap();
    let mut acct = Account::open(&mut store, RUN).unwrap();
    let root_id = acct
        .allocate(
            &lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::Experiment,
                target: RUN.to_string(),
            },
            BudgetSpec::hard_caps(BudgetMode::Pool, &[(spend, 10_000_000)]),
        )
        .unwrap();
    let slice_id = acct
        .allocate(
            &lease,
            Some(root_id.as_str()),
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: "slot-A".to_string(),
            },
            BudgetSpec::hard_caps(BudgetMode::Pool, &[(spend, 1_000_000)]),
        )
        .unwrap();
    let mut q = ResourceVector::zero();
    q.add(DimensionId::Spend, reserved_micros);
    let rid = acct
        .reserve(&lease, &slice_id, &q, "mc:seed-1", 300_000)
        .unwrap();
    (root_id, rid)
}

#[test]
fn crash_recovery_settles_orphans_bounded() {
    let store_root = root("recover");
    let (endpoint, _seen) = chat_fixture();
    let (_root_id, _rid) = seed_orphan(&store_root, 300_000);
    // The dead writer's lease expires; the resumed writer's takeover
    // is a recorded lifecycle.lease.fenced row.
    std::thread::sleep(Duration::from_millis(450));
    let lane = formula_lane(&endpoint);
    let cfg = ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane,
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(10_000_000),
    };
    let (_proxy, _bound) = Proxy::configure(cfg).unwrap();
    let t = terminals(&store_root);
    assert_eq!(t.len(), 1);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("proxy_aborted"),
        "{t:?}"
    );
    assert_eq!(
        t[0].get("bounded")
            .and_then(|b| b.get("hi"))
            .and_then(Json::as_int),
        Some(300_000)
    );
    let attributed = envs(&store_root)
        .into_iter()
        .find(|(c, _)| c == "measurement.cost.attributed")
        .unwrap()
        .1;
    assert_eq!(
        attributed
            .get("money")
            .and_then(|m| m.get("micro_units"))
            .and_then(Json::as_int),
        Some(300_000),
        "charged bounded{{0, reserved}} at hi"
    );
    assert_eq!(
        attributed
            .get("confidence")
            .and_then(|c| c.get("bounded"))
            .and_then(|b| b.get("hi"))
            .and_then(Json::as_int),
        Some(300_000),
        "confidence bounded{{lo:0, hi:reserved}}"
    );
    assert!(envs(&store_root)
        .iter()
        .any(|(c, _)| c == "control.budget.released"));
    assert_invariant_all_prefixes(&store_root);
}

// ─────────────────────────── AC-6: catalog identity stop ───────────────────────────

/// A catalog upstream: `/api/tags` + `/api/show` serve the declared
/// tuple; `modified_at` is mutable so the test can drift it.
fn catalog_upstream(modified: Arc<Mutex<String>>) -> (String, Arc<Mutex<Vec<Seen>>>) {
    fixture_upstream(Arc::new(move |s| {
        if s.target == "/api/tags" {
            let m = modified.lock().unwrap().clone();
            (
                200,
                vec![],
                "application/json".into(),
                format!(
                    "{{\"models\":[{{\"name\":\"{MODEL}\",\"model\":\"{MODEL}\",\"digest\":\"sha256:abc123def456\",\"modified_at\":\"{m}\",\"size\":9000000000}}]}}"
                ),
                0,
            )
        } else if s.target == "/api/show" {
            (
                200,
                vec![],
                "application/json".into(),
                "{\"details\":{\"parameter_size\":\"9B\",\"quantization_level\":\"Q4_K_M\",\"family\":\"glm\"},\"model_info\":{\"glm5.context_length\":262144},\"thinking\":{\"values\":[\"on\",\"off\"],\"default\":\"on\"},\"license\":\"mit\"}".into(),
                0,
            )
        } else if s.target.contains("chat") {
            chat_ok()
        } else {
            (404, vec![], "application/json".into(), "{}".into(), 0)
        }
    }))
}

fn probed_rows(store_root: &PathBuf) -> Vec<Json> {
    envs(store_root)
        .into_iter()
        .filter(|(c, _)| c == "model.profile.probed")
        .flat_map(|(_, p)| match p.get("records") {
            Some(Json::Arr(v)) => v.clone(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn catalog_identity_tuple_and_drift_stop() {
    let modified = Arc::new(Mutex::new("2026-08-26T14:00:00Z".to_string()));
    let (endpoint, _seen) = catalog_upstream(modified.clone());
    let mut lane = formula_lane(&endpoint);
    lane.catalog_base = None; // derive → the real fixture
    lane.served_model_expected = vec![MODEL.to_string()];
    let (mut proxy, token, store_root) = setup("catalog", &endpoint, lane, auth(10_000_000));
    // run_start probe landed with the tuple.
    let recs = probed_rows(&store_root);
    assert_eq!(recs.len(), 1);
    let tuple = recs[0].get("tuple").unwrap();
    assert_eq!(
        tuple.get("parameter_count").and_then(Json::as_str),
        Some("9B")
    );
    assert_eq!(
        tuple.get("quantization_level").and_then(Json::as_str),
        Some("Q4_K_M")
    );
    assert_eq!(
        tuple.get("context_length").and_then(Json::as_int),
        Some(262_144)
    );
    assert_eq!(recs[0].get("drift_observed"), Some(&Json::Bool(false)));
    // Cosmetic reformat of the same instant — `+00:00` for `Z` — is
    // not drift (instants compare, not strings).
    *modified.lock().unwrap() = "2026-08-26T14:00:00.000+00:00".into();
    proxy.control(
        "catalog_boundary",
        &Json::obj([("label", Json::str("slot_batch"))]),
    );
    assert_eq!(
        probed_rows(&store_root)[1].get("drift_observed"),
        Some(&Json::Bool(false)),
        "cosmetic modified_at is not drift"
    );
    // A real instant change sets drift_observed and stops collection.
    *modified.lock().unwrap() = "2026-09-01T09:30:00Z".into();
    proxy.control(
        "catalog_boundary",
        &Json::obj([("label", Json::str("block_end"))]),
    );
    assert_eq!(
        probed_rows(&store_root)[2].get("drift_observed"),
        Some(&Json::Bool(true))
    );
    let (status, body, _h) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status, 402);
    assert!(body.contains("refused_phase_stopped"), "{body}");
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("refused_phase_stopped")
    );
}

// ─────────────────────────── AC-7/9/15: root cap, carry-over, citation ───────────────────────────

#[test]
fn root_cap_is_a_eff_times_one_minus_m() {
    // A=$70, included credit $60, m=.20 → root $48 (§4.4).
    let (endpoint, _seen) = chat_fixture();
    let a = auth_full(70_000_000, 60_000_000, 200_000, vec![]);
    let (_p, _token, store_root) = setup("rootcap", &endpoint, formula_lane(&endpoint), a);
    let allocated = envs(&store_root)
        .into_iter()
        .find(|(c, p)| c == "control.budget.allocated" && p.get("parent").is_none())
        .unwrap()
        .1;
    let hard = allocated
        .get("spec")
        .and_then(|s| s.get("dimensions"))
        .and_then(|d| d.get("spend"))
        .and_then(|d| d.get("hard"))
        .and_then(|h| h.get("limit"))
        .and_then(Json::as_int);
    assert_eq!(hard, Some(48_000_000), "A_eff $60 × .80 = $48");
    // REQ-XC-004 — the run's allocation record cites the verbatim
    // authorization (manifest `extra.proxy.authorization` on the
    // lifecycle.run.created row).
    let created = envs(&store_root)
        .into_iter()
        .find(|(c, _)| c == "lifecycle.run.created")
        .unwrap()
        .1;
    let mtext = created.to_canonical_string();
    assert!(
        mtext.contains("fixture authorization — R6.11 battery"),
        "{mtext}"
    );
    assert!(mtext.contains("gate.fixture"));
}

#[test]
fn missing_authorization_refuses() {
    let (endpoint, _seen) = chat_fixture();
    let store_root = root("noauth");
    let mut a = auth(10_000_000);
    a.verbatim = "  ".into();
    let cfg = ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane: formula_lane(&endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: a,
    };
    let e = Proxy::configure(cfg).err().unwrap();
    assert!(format!("{e}").contains("missing_authorization"), "{e}");
}

#[test]
fn carry_over_pool_and_unharvested_refusal() {
    let (endpoint, _seen) = chat_fixture();
    // W4's pool = cap − Σ prior windows' consumed + orphan hi.
    let prior = WindowSpend {
        window_id: "w2".into(),
        consumed_micros: 10_000_000,
        orphan_calls: 1,
        orphan_bounded_hi_micros: 300_000,
        harvested: true,
        verified: true,
    };
    let a = auth_full(60_000_000, 60_000_000, 200_000, vec![prior]);
    // cap 48M − prior 10M → pool 38M.
    let (_p, _t, store_root) = setup("carry", &endpoint, formula_lane(&endpoint), a);
    let allocated = envs(&store_root)
        .into_iter()
        .find(|(c, p)| c == "control.budget.allocated" && p.get("parent").is_none())
        .unwrap()
        .1;
    let hard = allocated
        .get("spec")
        .and_then(|s| s.get("dimensions"))
        .and_then(|d| d.get("spend"))
        .and_then(|d| d.get("hard"))
        .and_then(|h| h.get("limit"))
        .and_then(Json::as_int);
    assert_eq!(hard, Some(38_000_000), "cap 48M − prior 10M");
    drop(_p);

    // An unharvested prior ledger refuses the next window (REQ-XH-120).
    let bad = WindowSpend {
        window_id: "w3".into(),
        consumed_micros: 0,
        orphan_calls: 0,
        orphan_bounded_hi_micros: 0,
        harvested: false,
        verified: false,
    };
    let a = auth_full(60_000_000, 60_000_000, 200_000, vec![bad]);
    let e = Proxy::configure(ProxyConfig {
        store_root: root("carry2"),
        run_id: RUN.to_string(),
        lane: formula_lane(&endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: a,
    })
    .err()
    .unwrap();
    assert!(
        format!("{e}").contains("unharvested_or_unverified_ledger"),
        "{e}"
    );
}

// ─────────────────────────── AC-10/12/14: slices, no-cache, classification ───────────────────────────

#[test]
fn slice_validator_and_no_cache_formula() {
    let table = hharness_budget::pricing::ollama_glm_5_3_flash();
    // REQ-XC-113/210 — no-cache planning: the whole input prices at
    // the uncached rate until a measured stage records the share.
    let uncached = reservation_micros(200_000, 2_048, 131_072, None, &table).unwrap();
    // 202_048 × 150/1k + 131_072 × 500/1k ≈ $0.0303 + $0.0655 ≈ $0.0958.
    assert_eq!(uncached, 95_844);
    // A measured 50% cache share prices the cached half at the
    // cache-read rate — and lowers the bound.
    let cached = reservation_micros(200_000, 2_048, 131_072, Some(500_000), &table).unwrap();
    assert!(cached < uncached);
    // REQ-XC-126 — the slice floor is P90 + w and slices are equal.
    let w = uncached;
    let floor = slice_floor_micros(850_000, w);
    assert_eq!(floor, 850_000 + w);
    assert!(validate_slices(&[("a".into(), floor), ("b".into(), floor)], floor).is_ok());
    assert_eq!(
        validate_slices(&[("a".into(), floor - 1)], floor),
        Err(SliceViolation::BelowFloor {
            slot_id: "a".into(),
            slice_micros: floor - 1,
            floor_micros: floor,
        })
    );
    assert_eq!(
        validate_slices(&[("a".into(), floor), ("b".into(), floor + 1)], floor),
        Err(SliceViolation::Unequal {
            expected_micros: floor,
            slot_id: "b".into(),
            actual_micros: floor + 1,
        })
    );
    // Feasibility counts C × s, never mean costs.
    assert_eq!(
        feasibility_remaining_micros(48_000_000, 3, 850_000),
        45_450_000
    );
    // REQ-XC-126's premature predicate: infra refusals below s − w are
    // inconclusive; at/above is budget_refused.
    assert!(refusal_is_premature(0, 950_000, 100_000));
    assert!(refusal_is_premature(849_999, 950_000, 100_000));
    assert!(!refusal_is_premature(850_000, 950_000, 100_000));
}

#[test]
fn slot_slice_refusal_is_budget_refused_not_phase_stop() {
    // A refusal the SLOT binds is a ledgered 402 `refused_budget`
    // (REQ-XC-126's record: requested/available/consumed and the
    // classification) — it never emits exhaustion rows and never
    // stops the phase: another slot still forwards.
    let (endpoint, _seen) = chat_fixture();
    let store_root = root("slotref");
    let (mut proxy, _b) = Proxy::configure(ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane: formula_lane(&endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(10_000_000),
    })
    .unwrap();
    // 60_000 < the ≈95_844 formula reservation → every call on this
    // slice refuses.
    let tight = proxy.register_slot("slot-tight", "high", 60_000).unwrap();
    let room = proxy.register_slot("slot-room", "high", 2_000_000).unwrap();
    let (status, _body, _h) = call(&mut proxy, &tight, &chat_body(""));
    assert_eq!(status, 402);
    let classes: Vec<String> = envs(&store_root).iter().map(|(c, _)| c.clone()).collect();
    for absent in [
        "control.budget.exceeded",
        "lifecycle.escalation.raised",
        "control.decision",
    ] {
        assert!(
            !classes.contains(&absent.to_string()),
            "slot refusal must not emit {absent}"
        );
    }
    let r = receipts(&store_root);
    let res = proxy_member(&r[0], "reservation");
    assert_eq!(res.get("refused"), Some(&Json::Bool(true)), "{res:?}");
    assert!(
        res.get("requested_micros").and_then(Json::as_int).unwrap() > 60_000,
        "{res:?}"
    );
    assert!(res.get("available_micros").and_then(Json::as_int).is_some());
    assert_eq!(
        res.get("slot_consumed_micros").and_then(Json::as_int),
        Some(0)
    );
    assert_eq!(
        res.get("classification").and_then(Json::as_str),
        Some("budget_refused")
    );
    assert_eq!(
        terminals(&store_root)[0]
            .get("disposition")
            .and_then(Json::as_str),
        Some("refused_budget")
    );
    // The phase is not stopped — the roomy slot still forwards.
    let (status2, _body, _h) = call(&mut proxy, &room, &chat_body(""));
    let t2 = terminals(&store_root);
    assert_eq!(
        t2[1].get("disposition").and_then(Json::as_str),
        Some("forwarded")
    );
    assert_eq!(status2, 200);
}

#[test]
fn root_exhaustion_emits_stop_rows_and_stops_phase() {
    // REQ-XC-112/E1 — only a refusal the ROOT binds emits
    // `control.budget.exceeded` + `lifecycle.escalation.raised` +
    // `control.decision{stop}`. Root 60_000 admits one slice; the
    // ~66_000 formula reservation exceeds the root itself → the
    // refusal is root-exhaustion, not a slot-share refusal.
    let (endpoint, _seen) = chat_fixture();
    let store_root = root("rootstop");
    let (mut proxy, _b) = Proxy::configure(ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane: formula_lane(&endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(60_000),
    })
    .unwrap();
    let token_b = proxy.register_slot("slot-B", "high", 60_000).unwrap();
    let (status, _body, _h) = call(&mut proxy, &token_b, &chat_body(""));
    assert_eq!(status, 402);
    let classes: Vec<String> = envs(&store_root).iter().map(|(c, _)| c.clone()).collect();
    for want in [
        "control.budget.exceeded",
        "lifecycle.escalation.raised",
        "control.decision",
    ] {
        assert!(
            classes.contains(&want.to_string()),
            "root exhaustion must emit {want}: {classes:?}"
        );
    }
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("refused_budget")
    );
    // Phase stopped → every later admit refuses phase_stopped.
    let (status2, body2, _h) = call(&mut proxy, &token_b, &chat_body(""));
    assert_eq!(status2, 402);
    assert!(body2.contains("refused_phase_stopped"), "{body2}");
}

// ─────────────────────────── AC-13: reservation bound violation ───────────────────────────

#[test]
fn reservation_bound_violation_halts_admissions() {
    // Reported input tokens above the reservation bound stop the phase
    // (REQ-XC-114).
    let (endpoint, _seen) = fixture_upstream(Arc::new(|s| {
        if s.target.contains("chat") {
            (
                200,
                vec![],
                "application/json".into(),
                "{\"model\":\"glm-5.3-flash\",\"choices\":[],\"usage\":{\"prompt_tokens\":5000,\"completion_tokens\":10,\"total_tokens\":5010}}".into(),
                0,
            )
        } else {
            chat_ok()
        }
    }));
    let lane = formula_lane(&endpoint);
    let (mut proxy, token, store_root) = setup("bound", &endpoint, lane, auth(10_000_000));
    let (status, _body, _h) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status, 200);
    let t = terminals(&store_root);
    assert_eq!(
        t[0].get("reservation_bound_violated"),
        Some(&Json::Bool(true))
    );
    let (status2, body2, _h) = call(&mut proxy, &token, &chat_body(""));
    assert_eq!(status2, 402);
    assert!(body2.contains("refused_phase_stopped"), "{body2}");
}

// ─────────────────────────── AC-11: FIFO + concurrency bound ───────────────────────────

#[test]
fn fifo_queue_records_queue_wait_and_bounded_inflight() {
    // The serial writer is stronger than the declared ≤3 limit —
    // three simultaneous clients are answered in accept order, each
    // receipted with its queue wait.
    let (endpoint, seen) = chat_fixture();
    let store_root = root("fifo");
    let (mut proxy, _b) = Proxy::configure(ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane: formula_lane(&endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(10_000_000),
    })
    .unwrap();
    let mut streams = vec![];
    let body = chat_body("");
    for slot in ["slot-1", "slot-2", "slot-3"] {
        let token = proxy.register_slot(slot, "high", 2_000_000).unwrap();
        let mut s = TcpStream::connect(proxy.bound_addr()).unwrap();
        s.write_all(
            format!(
                "POST /v1/chat/completions HTTP/1.1\r\nhost: proxy.local\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
        streams.push(s);
    }
    // Three ticks: one whole call each (serial accept order is FIFO).
    for _ in 0..3 {
        let mut done = false;
        for _ in 0..4000 {
            if proxy.tick() {
                done = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(done);
    }
    let mut statuses = vec![];
    for s in streams {
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut r = BufReader::new(s);
        let (head, _b) = read_response(&mut r, 8 << 20).unwrap();
        statuses.push(head.status);
    }
    assert_eq!(statuses, vec![200, 200, 200]);
    assert_eq!(v1(&seen), 3);
    let r = receipts(&store_root);
    assert_eq!(r.len(), 3);
    for rec in &r {
        assert!(proxy_member(rec, "timing").get("queue_wait_ms").is_some());
    }
    assert_invariant_all_prefixes(&store_root);
}

// ─────────────────────────── AC-8: conservation at every prefix ───────────────────────────

#[test]
fn conservation_holds_across_success_and_failure() {
    let (endpoint, _seen) = fixture_upstream(Arc::new(|s| {
        if s.body.contains("fail_me") {
            (
                500,
                vec![],
                "application/json".into(),
                "{\"error\":{\"type\":\"upstream\"}}".into(),
                0,
            )
        } else {
            chat_ok()
        }
    }));
    let lane = formula_lane(&endpoint);
    let (mut proxy, token, store_root) = setup("conserve", &endpoint, lane, auth(10_000_000));
    let _ = call(&mut proxy, &token, &chat_body(""));
    let _ = call(&mut proxy, &token, &chat_body(",\"fail_me\":true"));
    let _ = call(&mut proxy, &token, &chat_body(""));
    assert_invariant_all_prefixes(&store_root);
    let t = terminals(&store_root);
    let dispos: Vec<String> = t
        .iter()
        .map(|x| {
            x.get("disposition")
                .and_then(Json::as_str)
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(dispos, vec!["forwarded", "upstream_error", "forwarded"]);
}

// ─────────────────────────── AC-15: authorization citation ───────────────────────────

#[test]
fn allocation_record_cites_verbatim_authorization() {
    let (endpoint, _seen) = chat_fixture();
    let mut a = auth(40_000_000);
    a.verbatim = "O-2026-11-07: GLM-5.3-flash pass FP-S up to $56".into();
    a.gate_record_ref = "FP-S".into();
    let (_p, _t, store_root) = setup("cite", &endpoint, formula_lane(&endpoint), a);
    let created = envs(&store_root)
        .into_iter()
        .find(|(c, _)| c == "lifecycle.run.created")
        .unwrap()
        .1;
    let mtext = created.to_canonical_string();
    assert!(
        mtext.contains("O-2026-11-07: GLM-5.3-flash pass FP-S up to $56"),
        "{mtext}"
    );
    assert!(mtext.contains("FP-S"));
    // The credential env is `provided: no`, never a value.
    assert!(!mtext.contains("fixture.local/api_key"));
}
