//! Executable coverage for manifest ticket 342 / R7.9 — the
//! DF-R7.1-2 usage-capture unblocker (§4.3, §4.8; REQ-XC-007/110).
//! The W4x window's pi/claude arms recorded `usage.source: absent`
//! on 3 678 of 5 127 `model.call.completed` events because the
//! extractor read `body.usage` verbatim and the upstream's streaming
//! replies carry usage inside SSE `data:` frames — the provider WAS
//! reporting per-call usage (pi clients already sent
//! `stream_options.include_usage`); the proxy could not read it.
//! These fixtures pin: SSE-carried usage folds to
//! `provider_per_call` with `tokens.input.uncached` on the retained
//! receipt view and the `model.call.completed` usage member, per
//! surface; a stream with no usage member stays honestly `absent`.
//! Every test fails if the behaviour it names is removed.

use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hharness_hosting::proxy::budget::{ProxyAuthorization, WindowSpend};
use hharness_hosting::proxy::lane::{ProxyLane, Reservation, ReservationFormula};
use hharness_hosting::proxy::{Proxy, ProxyConfig};
use hharness_identity::idp::ContentAddress;
use hharness_ledger::store::Store;
use hharness_wire::http::read_response;
use hharness_wire::json::{parse, Json};

const MODEL: &str = "glm-5.3-flash";
const RUN: &str = "phase-proxy";

// ─────────────────────────── fixtures (r6_11 shape) ───────────────────────────

fn auth(cap: i64) -> ProxyAuthorization {
    ProxyAuthorization {
        verbatim: "fixture authorization — R7.9 battery".into(),
        gate_record_ref: "gate.fixture".into(),
        authorized_micros: cap,
        available_credit_micros: cap,
        m_ppm: 0,
        prior_windows: Vec::<WindowSpend>::new(),
    }
}

fn root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-r7-9-{tag}-{}-{}",
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
    /// The request body the fixture upstream observed (kept for
    /// handler-side assertions; the pinned legs key on `target`).
    #[allow(dead_code)]
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

fn setup(tag: &str, endpoint: &str, auth_cap: i64) -> (Proxy, String, PathBuf) {
    let store_root = root(tag);
    let mut lane = ProxyLane::program(
        "lane.fixture",
        endpoint,
        "fixture.local",
        MODEL,
        Reservation::Formula(ReservationFormula::default()),
    );
    lane.catalog_base = Some("http://127.0.0.1:1".to_string());
    lane.heartbeat_ms = 30;
    let cfg = ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane,
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(auth_cap),
        mode: hharness_hosting::proxy::ProxyMode::Live,
    };
    let (mut proxy, _bound) = Proxy::configure(cfg).unwrap();
    let token = proxy.register_slot("slot-A", "high", 2_000_000).unwrap();
    (proxy, token, store_root)
}

fn call(proxy: &mut Proxy, token: &str, path: &str, body: &str) -> (u16, String) {
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nhost: proxy.local\r\ncontent-type: application/json\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
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
    (head.status, String::from_utf8_lossy(&rbody).to_string())
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

fn terminals(store_root: &PathBuf) -> Vec<Json> {
    envs(store_root)
        .into_iter()
        .filter(|(c, _)| c == "model.call.completed" || c == "model.call.failed")
        .map(|(_, p)| p)
        .collect()
}

fn receipts(store_root: &PathBuf) -> Vec<Json> {
    terminals(store_root)
        .iter()
        .map(|t| {
            let r = t.get("response_ref").and_then(Json::as_str).unwrap();
            parse(&blob_text(store_root, r)).unwrap()
        })
        .collect()
}

fn usage_view(receipt: &Json) -> Json {
    receipt
        .get("proxy")
        .and_then(|p| p.get("usage"))
        .and_then(|u| u.get("view"))
        .cloned()
        .unwrap_or(Json::Null)
}

fn usage_source(receipt: &Json) -> Option<String> {
    receipt
        .get("proxy")
        .and_then(|p| p.get("usage"))
        .and_then(|u| u.get("source"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

// ─────────────────────────── the DF-R7.1-2 unblocker legs ───────────────────────────

/// The verbatim Ollama openai.chat stream shape retained in the W4x
/// run-root: content/tool_call chunks, then the terminal
/// empty-`choices` usage chunk, then `[DONE]`.
const OPENAI_SSE_WITH_USAGE: &str = concat!(
    "data: {\"id\":\"chatcmpl-81\",\"object\":\"chat.completion.chunk\",\"created\":1795143784,\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chatcmpl-81\",\"object\":\"chat.completion.chunk\",\"created\":1795143784,\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: {\"id\":\"chatcmpl-81\",\"object\":\"chat.completion.chunk\",\"created\":1795143784,\"model\":\"glm-5.3-flash\",\"choices\":[],\"usage\":{\"prompt_tokens\":1272,\"completion_tokens\":26,\"total_tokens\":1298,\"prompt_tokens_details\":{\"cached_tokens\":64}}}\n\n",
    "data: [DONE]\n\n",
);

/// The verbatim Ollama anthropic.messages stream shape retained in the
/// W4x run-root: `message_start` carries the input count, the trailing
/// `message_delta` carries the cache split and cumulative output.
const ANTHROPIC_SSE_WITH_USAGE: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"glm-5.3-flash\",\"content\":[],\"usage\":{\"input_tokens\":17621,\"output_tokens\":0}}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"ok\"}}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"input_tokens\":16428,\"cache_read_input_tokens\":0,\"output_tokens\":41}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

/// A stream whose frames carry no usage member anywhere — the honest
/// `absent` survives (never a fabricated count).
const OPENAI_SSE_WITHOUT_USAGE: &str = concat!(
    "data: {\"id\":\"chatcmpl-82\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

#[test]
fn openai_stream_usage_lands_provider_per_call_and_uncached() {
    // The W4x pi-arm shape: `stream:true` downstream, SSE upstream
    // with the terminal usage chunk — previously `usage.source: absent`.
    let (endpoint, seen) = fixture_upstream(Arc::new(|_s| {
        (
            200,
            vec![],
            "text/event-stream".into(),
            OPENAI_SSE_WITH_USAGE.to_string(),
            0,
        )
    }));
    let (mut proxy, token, store_root) = setup("openai-sse", &endpoint, 10_000_000);
    let (status, _body) = call(
        &mut proxy,
        &token,
        "/v1/chat/completions",
        "{\"model\":\"anything\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"stream\":true}",
    );
    assert_eq!(status, 200);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].target, "/v1/chat/completions");
    drop(seen);

    let t = terminals(&store_root);
    assert_eq!(t.len(), 1);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("forwarded")
    );
    // The retained event's usage member carries the uncached role.
    let ev_usage = t[0].get("usage").cloned().unwrap_or(Json::Null);
    assert_eq!(
        ev_usage.get("tokens.input.uncached").and_then(Json::as_int),
        Some(1208),
        "1272 prompt − 64 cached"
    );

    let r = receipts(&store_root);
    assert_eq!(r.len(), 1);
    assert_eq!(usage_source(&r[0]).as_deref(), Some("provider_per_call"));
    let v = usage_view(&r[0]);
    assert_eq!(
        v.get("tokens.input.total").and_then(Json::as_int),
        Some(1272)
    );
    assert_eq!(
        v.get("tokens.input.uncached").and_then(Json::as_int),
        Some(1208)
    );
    assert_eq!(
        v.get("tokens.output.total").and_then(Json::as_int),
        Some(26)
    );
    // The priced charge mints — usage-derived micros, not a flat zero.
    let attributed: Vec<Json> = envs(&store_root)
        .into_iter()
        .filter(|(c, _)| c == "measurement.cost.attributed")
        .map(|(_, p)| p)
        .collect();
    assert_eq!(attributed.len(), 1, "provider usage prices a charge");
    let roles = attributed[0].get("roles").cloned().unwrap_or(Json::Null);
    let uncached_micros = roles
        .get("tokens.input.uncached")
        .and_then(Json::as_int)
        .unwrap_or(0);
    assert!(uncached_micros > 0, "uncached role priced: {roles:?}");
}

#[test]
fn anthropic_stream_usage_merges_message_start_and_delta() {
    // The W4x claude-arm shape: message_start's input + the final
    // message_delta's cache split and cumulative output (last-wins).
    let (endpoint, _seen) = fixture_upstream(Arc::new(|_s| {
        (
            200,
            vec![],
            "text/event-stream".into(),
            ANTHROPIC_SSE_WITH_USAGE.to_string(),
            0,
        )
    }));
    let (mut proxy, token, store_root) = setup("anthropic-sse", &endpoint, 10_000_000);
    let (status, _body) = call(
        &mut proxy,
        &token,
        "/v1/messages",
        "{\"model\":\"anything\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"max_tokens\":64,\"stream\":true}",
    );
    assert_eq!(status, 200);

    let t = terminals(&store_root);
    assert_eq!(t.len(), 1);
    assert_eq!(
        t[0].get("disposition").and_then(Json::as_str),
        Some("forwarded")
    );
    let ev_usage = t[0].get("usage").cloned().unwrap_or(Json::Null);
    assert_eq!(
        ev_usage.get("tokens.input.uncached").and_then(Json::as_int),
        Some(16428),
        "the trailing message_delta's input_tokens wins (last-writer), cache_read 0"
    );

    let r = receipts(&store_root);
    assert_eq!(r.len(), 1);
    assert_eq!(usage_source(&r[0]).as_deref(), Some("provider_per_call"));
    let v = usage_view(&r[0]);
    assert_eq!(
        v.get("tokens.input.total").and_then(Json::as_int),
        Some(16428)
    );
    assert_eq!(
        v.get("tokens.input.uncached").and_then(Json::as_int),
        Some(16428)
    );
    assert_eq!(
        v.get("tokens.output.total").and_then(Json::as_int),
        Some(41)
    );
}

#[test]
fn stream_without_usage_stays_honest_absent() {
    // A provider stream that never emits a usage member →
    // `usage.source: absent` and no fabricated counts.
    let (endpoint, _seen) = fixture_upstream(Arc::new(|_s| {
        (
            200,
            vec![],
            "text/event-stream".into(),
            OPENAI_SSE_WITHOUT_USAGE.to_string(),
            0,
        )
    }));
    let (mut proxy, token, store_root) = setup("sse-absent", &endpoint, 10_000_000);
    let (status, _body) = call(
        &mut proxy,
        &token,
        "/v1/chat/completions",
        "{\"model\":\"anything\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"stream\":true}",
    );
    assert_eq!(status, 200);
    let r = receipts(&store_root);
    assert_eq!(r.len(), 1);
    assert_eq!(usage_source(&r[0]).as_deref(), Some("absent"));
    // The absent-usage charge mints at the honest zero — the W4x
    // window's own "0 µ$ attribution" shape: the row exists with
    // `micro_units: 0`, never a fabricated count.
    let attributed: Vec<Json> = envs(&store_root)
        .into_iter()
        .filter(|(c, _)| c == "measurement.cost.attributed")
        .map(|(_, p)| p)
        .collect();
    assert_eq!(attributed.len(), 1);
    assert_eq!(
        attributed[0]
            .get("money")
            .and_then(|m| m.get("micro_units"))
            .and_then(Json::as_int),
        Some(0)
    );
}
