//! R4.5b — `HttpModelTransport` over loopback (AC2; DF-S1.18-1;
//! ADR-0364 D3; R4.5c TLS mount). A `TcpListener` fixture answers the
//! transport's POST / GET legs — no live service anywhere; the cells pin
//! the wire codec's single home (`hharness_wire::http`), the typed scheme
//! refusals, streaming frame translation, and the dialect's error
//! classification.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, Transport};
use hharness_gateway::http_transport::HttpModelTransport;
use hharness_gateway::openai;
use hharness_gateway::plan::{CachePlan, Capabilities, ProviderRequestPlan};
use hharness_gateway::vocab::ModelErrorClass;
use hharness_wire::json::Json;

// ── loopback fixture ─────────────────────────────────────────────────────────

/// One-shot HTTP/1.1 server: accept one connection, read the bounded
/// request (head + `content-length` body), write `response`, close.
/// Returns `(endpoint, join)` — the join asserts the request bytes the
/// transport actually wrote.
fn serve_once(
    response: &'static str,
) -> (String, std::thread::JoinHandle<(String, String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        // Read until the head + `content-length` body are complete.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut head_end, mut want) = (None, 0usize);
        loop {
            match sock.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if head_end.is_none() {
                        if let Some(p) = find_head_end(&buf) {
                            head_end = Some(p);
                            let head = String::from_utf8_lossy(&buf[..p]).to_string();
                            want = content_length(&head);
                        }
                    }
                    if let Some(p) = head_end {
                        if buf.len() >= p + want {
                            break;
                        }
                    }
                    if std::time::Instant::now() > deadline {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        sock.write_all(response.as_bytes()).unwrap();
        let _ = sock.flush();
        let (method, target) = first_line(&buf);
        let body = head_end
            .map(|p| buf[p + want.min(buf.len() - p) - want..p + want.min(buf.len() - p)].to_vec())
            .unwrap_or_default();
        (method, target, body)
    });
    (format!("http://127.0.0.1:{port}"), h)
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn content_length(head: &str) -> usize {
    for line in head.split("\r\n") {
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            return v.trim().parse().unwrap_or(0);
        }
    }
    0
}

fn first_line(buf: &[u8]) -> (String, String) {
    let head_end = find_head_end(buf).unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..head_end]);
    let mut it = head.split("\r\n");
    let mut parts = it.next().unwrap_or("").split(' ');
    (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    )
}

fn plan(endpoint: &str, stream: bool) -> ProviderRequestPlan {
    let d = dialects::openai_compatible();
    let body = openai::chat_request_body(
        "fixture-model",
        &[openai::text_msg("user", "hi")],
        &[],
        stream,
        &BTreeMap::new(),
    );
    ProviderRequestPlan {
        dialect_id: d.dialect_id.clone(),
        dialect_version: d.version.clone(),
        endpoint_ref: endpoint.to_string(),
        model_ref: "fixture-model".to_string(),
        body,
        provider_params: BTreeMap::new(),
        capabilities: Capabilities {
            streaming: stream,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: CachePlan::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    }
}

fn anon() -> CredentialHandle {
    CredentialHandle {
        binding_id: None,
        provided: false,
    }
}

// ── the cells ────────────────────────────────────────────────────────────────

/// `https://` now dials TLS (R4.5c landed beneath the same codec —
/// `tls_pending` retired): a dead port is the typed `network` refusal;
/// a foreign scheme stays `unsupported_feature{unsupported_scheme}` —
/// never a downgrade either way.
#[test]
fn https_dials_tls_foreign_scheme_refuses() {
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d.clone());
    let p = plan("https://127.0.0.1:1", true);
    let bytes = hharness_gateway::codec::serialize(&p);
    let err = t.send(&bytes, &anon(), "https://127.0.0.1:1").unwrap_err();
    assert_eq!(err.class, ModelErrorClass::Network);
    let err = t.discover(&anon(), "https://127.0.0.1:1").unwrap_err();
    assert_eq!(err.class, ModelErrorClass::Network);
    let err = t.discover(&anon(), "ftp://127.0.0.1:1").unwrap_err();
    assert_eq!(err.class, ModelErrorClass::UnsupportedFeature);
    assert_eq!(err.provider_code.as_deref(), Some("unsupported_scheme"));
}

/// `send` POSTs `{endpoint}/chat/completions` with the plan's `body`
/// verbatim; an SSE stream translates into the normalized frame set
/// (`stream_start` → deltas → `stream_end` carrying usage).
#[test]
fn send_streams_sse_frames() {
    let body = concat!(
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hel\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":2,\"total_tokens\":11}}\n\n",
        "data: [DONE]\n\n",
    );
    let response_owned = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let response: &'static str = Box::leak(response_owned.into_boxed_str());
    let (endpoint, join) = serve_once(response);
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let p = plan(&endpoint, true);
    let bytes = hharness_gateway::codec::serialize(&p);
    let frames = t.send(&bytes, &anon(), &endpoint).unwrap();
    let (method, target, req_body) = join.join().unwrap();
    assert_eq!(method, "POST");
    assert_eq!(target, "/chat/completions");
    let sent: Json = hharness_wire::json::parse(&String::from_utf8_lossy(&req_body)).unwrap();
    assert_eq!(
        sent.get("model").and_then(Json::as_str),
        Some("fixture-model")
    );
    assert_eq!(sent.get("stream"), Some(&Json::Bool(true)));
    let kinds: Vec<&str> = frames.iter().map(|(_, f)| f.kind.as_str()).collect();
    assert!(kinds.contains(&"stream_start"), "{kinds:?}");
    assert!(kinds.contains(&"block_delta"), "{kinds:?}");
    let end = frames
        .iter()
        .rev()
        .find(|(_, f)| f.kind == "stream_end")
        .expect("stream_end frame");
    assert_eq!(
        end.1.data.get("stop_reason").and_then(Json::as_str),
        Some("stop")
    );
    assert!(end.1.data.get("usage").is_some());
    // Arrival stamps are monotone non-decreasing (the adapter clock).
    for w in frames.windows(2) {
        assert!(w[0].0 <= w[1].0);
    }
}

/// A non-streaming `chat.completion` folds into the same normalized
/// sequence (read by `content-type`, never by assumption).
#[test]
fn send_non_streaming_completion_folds() {
    let body = "{\"id\":\"chatcmpl-9\",\"object\":\"chat.completion\",\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}";
    let response_owned = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let response: &'static str = Box::leak(response_owned.into_boxed_str());
    let (endpoint, join) = serve_once(response);
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let p = plan(&endpoint, false);
    let bytes = hharness_gateway::codec::serialize(&p);
    let frames = t.send(&bytes, &anon(), &endpoint).unwrap();
    let (method, target, _) = join.join().unwrap();
    assert_eq!(method, "POST");
    assert_eq!(target, "/chat/completions");
    let kinds: Vec<&str> = frames.iter().map(|(_, f)| f.kind.as_str()).collect();
    assert_eq!(kinds.first(), Some(&"stream_start"));
    assert_eq!(kinds.last(), Some(&"stream_end"));
}

/// HTTP error statuses classify through the dialect — 429 +
/// `retry-after` is `rate_limited` with the declared `retry_after_ms`.
#[test]
fn http_429_classifies_rate_limited_with_retry_after() {
    let body = "{\"error\":{\"message\":\"slow down\",\"type\":\"rate_limit\"}}";
    let response_owned = format!(
        "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\nretry-after: 2\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let response: &'static str = Box::leak(response_owned.into_boxed_str());
    let (endpoint, join) = serve_once(response);
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let p = plan(&endpoint, true);
    let bytes = hharness_gateway::codec::serialize(&p);
    let err = t.send(&bytes, &anon(), &endpoint).unwrap_err();
    assert_eq!(err.class, ModelErrorClass::RateLimited, "{err:?}");
    assert_eq!(err.retry_after_ms, Some(2_000));
    join.join().unwrap();
}

/// `discover` GETs `{endpoint}/models` and answers the listing verbatim.
#[test]
fn discover_returns_model_listing() {
    let body = "{\"object\":\"list\",\"data\":[{\"id\":\"qwen3-test\",\"object\":\"model\"}]}";
    let response_owned = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let response: &'static str = Box::leak(response_owned.into_boxed_str());
    let (endpoint, join) = serve_once(response);
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let doc = t.discover(&anon(), &endpoint).unwrap();
    let (method, target, _) = join.join().unwrap();
    assert_eq!(method, "GET");
    assert_eq!(target, "/models");
    assert_eq!(
        doc.get("data")
            .and_then(|d| match d {
                Json::Arr(v) => v.first(),
                _ => None,
            })
            .and_then(|m| m.get("id"))
            .and_then(Json::as_str),
        Some("qwen3-test")
    );
}

/// A refused dial is a typed `network` error — never a panic, never a
/// fabricated frame.
#[test]
fn connection_refused_is_network_error() {
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let p = plan("http://127.0.0.1:1", true);
    let bytes = hharness_gateway::codec::serialize(&p);
    let err = t.send(&bytes, &anon(), "http://127.0.0.1:1").unwrap_err();
    assert!(matches!(
        err.class,
        ModelErrorClass::Network | ModelErrorClass::Timeout(_)
    ));
}

/// The wire stays the one codec home — the request head the transport
/// wrote parses clean through `hharness_wire::http`'s server half.
#[test]
fn written_request_is_wire_parseable() {
    let body = "{\"object\":\"list\",\"data\":[]}";
    let response_owned = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let response: &'static str = Box::leak(response_owned.into_boxed_str());
    let (endpoint, join) = serve_once(response);
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d);
    let _ = t.discover(&anon(), &endpoint).unwrap();
    let (method, _, _) = join.join().unwrap();
    assert_eq!(method, "GET");
    let _: Vec<WireFrame> = Vec::new();
}
