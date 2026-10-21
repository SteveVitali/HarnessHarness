//! R4.5d — the hosted-provider dialect pair (ADR-0366): Anthropic Messages
//! (`anthropic-messages@1`) and hosted OpenAI Chat Completions (`openai@1`)
//! plus the broker-backed `CredentialPort`, transport auth injection, and
//! the AC-R-2.3.1-11 out-of-process live transport. Every cell is
//! deterministic — a `TcpListener` fixture answers the transport's legs;
//! no live service is touched here (the `#[ignore]`d live legs ride
//! `scripts/live-verify.sh`).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hh_gateway::codec::{self, WireFrame};
use hh_gateway::credentials::{
    header_secret, source_from_binding, source_from_ref, BrokerCredentialPort,
};
use hh_gateway::dialects;
use hh_gateway::gateway::{CredentialHandle, CredentialPort, Transport};
use hh_gateway::grammar::{decode_frame, DecodeState, ModelEvent, ModelEventKind};
use hh_gateway::http_transport::HttpModelTransport;
use hh_gateway::plan::{CachePlan, Capabilities, Extensions, ProviderRequestPlan};
use hh_gateway::vocab::{ModelErrorClass, StopReason};
use hh_gateway::{anthropic, openai};
use hh_secrets::channel::SecretSource;
use hh_secrets::StaticVault;
use hh_wire::json::Json;

const NORM: &str = "test#usage_mapping";

// ── loopback fixture (the r4_5b discipline — one-shot HTTP/1.1 server) ────

/// One-shot HTTP/1.1 server: accept one connection, read the bounded
/// request, write `response`, close. The join returns the request head +
/// body the transport actually wrote.
fn serve_once(
    response: &'static str,
) -> (String, std::thread::JoinHandle<(String, String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
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
                            want = content_length(&String::from_utf8_lossy(&buf[..p]));
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
        let head_end = head_end.unwrap_or(buf.len());
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        (head, String::new(), buf[head_end.min(buf.len())..].to_vec())
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

fn plan(endpoint: &str, dialect: &hh_gateway::WireDialect, body: Json) -> Vec<u8> {
    let p = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: endpoint.to_string(),
        model_ref: "model.test".into(),
        body,
        provider_params: Extensions::default(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: true,
            vision: None,
            deferred: None,
        },
        cache: CachePlan::default(),
        budget_reservation_id: Some("br.test".into()),
        canonical_len: 0,
    };
    codec::serialize(&p)
}

fn decode_all(
    dialect: &hh_gateway::WireDialect,
    frames: &[(u64, WireFrame)],
) -> (Vec<ModelEvent>, DecodeState) {
    let mut state = DecodeState {
        model_call_id: "mc.t".into(),
        attempt_no: 1,
        ..DecodeState::default()
    };
    let mut events = Vec::new();
    for (at, f) in frames {
        events.extend(decode_frame(dialect, f, &mut state, *at, NORM));
    }
    (events, state)
}

fn completed(events: &[ModelEvent]) -> &hh_gateway::message::ModelMessage {
    events
        .iter()
        .find_map(|e| match &e.kind {
            ModelEventKind::Completed { message, .. } => Some(message),
            _ => None,
        })
        .expect("a Completed event lands")
}

fn completed_usage(events: &[ModelEvent]) -> &hh_telemetry::TokenVector {
    events
        .iter()
        .find_map(|e| match &e.kind {
            ModelEventKind::Completed { usage: Some(u), .. } => Some(u),
            _ => None,
        })
        .expect("Completed carries usage")
}

fn ev(ty: &str, doc: Json) -> Json {
    let mut m = match doc {
        Json::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    m.insert("type".into(), Json::str(ty));
    Json::Obj(m)
}

/// The fixture Messages SSE sequence — a text turn ending `end_turn`.
fn anthropic_sse_text() -> String {
    let events = [
        r#"{"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","model":"claude-haiku-4-5","usage":{"input_tokens":12,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":1}}}"#,
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}"#,
        r#"{"type":"content_block_stop","index":0}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":7}}"#,
        r#"{"type":"message_stop"}"#,
    ];
    let mut s = String::new();
    for e in events {
        s.push_str("data: ");
        s.push_str(e);
        s.push_str("\n\n");
    }
    s
}

fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

// ── dialect documents ─────────────────────────────────────────────────────

#[test]
fn dialect_docs_load_strict() {
    let a = hh_gateway::WireDialect::from_json(&dialects::anthropic_messages_doc())
        .expect("anthropic-messages@1 document loads");
    assert_eq!(a.dialect_id, "anthropic-messages");
    assert_eq!(a.auth_kinds, vec![hh_gateway::AuthKind::ApiKeyHeader]);
    assert!(a
        .plan_schema
        .required_keys
        .contains(&"max_tokens".to_string()));
    let o = hh_gateway::WireDialect::from_json(&dialects::openai_doc())
        .expect("openai@1 document loads");
    assert_eq!(o.dialect_id, "openai");
    assert_eq!(o.auth_kinds, vec![hh_gateway::AuthKind::Bearer]);
    assert!(o.plan_schema.admits("max_completion_tokens"));
    assert!(o.plan_schema.admits("reasoning_effort"));
    assert!(o.plan_schema.admits("prompt_cache_key"));
    assert!(!o.plan_schema.admits("made_up_key"));
}

// ── anthropic request render ──────────────────────────────────────────────

#[test]
fn anthropic_request_render_golden() {
    let messages = vec![
        anthropic::text_msg("user", "hi"),
        anthropic::assistant_tool_use_msg(&[(
            "toolu_1".into(),
            "fs_read".into(),
            "{\"path\":\"a.rs\"}".into(),
        )]),
        anthropic::tool_result_msg("toolu_1", Json::str("contents")),
    ];
    let tools = vec![anthropic::tool_entry(
        "fs_read",
        "read a file",
        Json::obj([
            ("type", Json::str("object")),
            (
                "properties",
                Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
            ),
            ("required", Json::Arr(vec![Json::str("path")])),
        ]),
    )];
    let params = BTreeMap::from([
        ("max_tokens".to_string(), Json::Int(64)),
        ("top_k".to_string(), Json::Int(40)),
    ]);
    let body =
        anthropic::messages_request_body("claude-haiku-4-5", &messages, &tools, true, &params);
    assert_eq!(
        body.get("model").and_then(Json::as_str),
        Some("claude-haiku-4-5")
    );
    assert_eq!(body.get("max_tokens"), Some(&Json::Int(64)));
    assert_eq!(body.get("stream"), Some(&Json::Bool(true)));
    // `tool_choice` is the family's object spelling — never the
    // chat-completions string.
    assert_eq!(
        body.get("tool_choice")
            .and_then(|t| t.get("type"))
            .and_then(Json::as_str),
        Some("auto")
    );
    // The tools[] member is `{name, description, input_schema}` — never
    // `{type:"function"}`.
    let t0 = body
        .get("tools")
        .and_then(|t| match t {
            Json::Arr(v) => v.first(),
            _ => None,
        })
        .expect("tools[0]");
    assert_eq!(t0.get("name").and_then(Json::as_str), Some("fs_read"));
    assert!(t0.get("input_schema").is_some());
    assert!(
        t0.get("type").is_none(),
        "Messages tools carry no `type` member"
    );
    let msgs = body.get("messages").expect("messages[]");
    let arr = match msgs {
        Json::Arr(v) => v,
        _ => panic!("messages is an array"),
    };
    // User text renders one `text` content block.
    assert_eq!(
        arr[0].get("content").and_then(|c| match c {
            Json::Arr(b) => b.first().and_then(|b| b.get("type").and_then(Json::as_str)),
            _ => None,
        }),
        Some("text")
    );
    // The assistant tool_use turn renders `tool_use` blocks with the
    // canonical args parsed back into `input`.
    let tu = arr[1]
        .get("content")
        .and_then(|c| match c {
            Json::Arr(b) => b.first().cloned(),
            _ => None,
        })
        .expect("assistant tool_use block");
    assert_eq!(tu.get("type").and_then(Json::as_str), Some("tool_use"));
    assert_eq!(tu.get("id").and_then(Json::as_str), Some("toolu_1"));
    assert_eq!(
        tu.get("input")
            .and_then(|i| i.get("path"))
            .and_then(Json::as_str),
        Some("a.rs")
    );
    // The tool result is a `user` turn's `tool_result` block.
    let tr = arr[2]
        .get("content")
        .and_then(|c| match c {
            Json::Arr(b) => b.first().cloned(),
            _ => None,
        })
        .expect("tool_result block");
    assert_eq!(tr.get("type").and_then(Json::as_str), Some("tool_result"));
    assert_eq!(
        tr.get("tool_use_id").and_then(Json::as_str),
        Some("toolu_1")
    );
}

// ── anthropic SSE translate + decode ──────────────────────────────────────

#[test]
fn anthropic_sse_stream_decodes_completed() {
    let dialect = dialects::anthropic_messages();
    let mut tr = anthropic::MessagesTranslator::default();
    let mut frames = Vec::new();
    let mut at = 0u64;
    for doc in anthropic_sse_text()
        .split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .map(|s| hh_wire::json::parse_provider(s.trim_start_matches("data:").trim()).unwrap())
    {
        at += 10;
        for f in tr.push_event(&doc).unwrap() {
            frames.push((at, f));
        }
    }
    for f in tr.finish() {
        frames.push((at, f));
    }
    let (events, state) = decode_all(&dialect, &frames);
    assert!(
        state.violations.is_empty(),
        "violations: {:?}",
        state.violations
    );
    let msg = completed(&events);
    assert_eq!(msg.text(), "Hello world");
    assert_eq!(msg.stop_reason, StopReason::EndTurn);
    assert_eq!(
        msg.served_model.as_deref(),
        Some("claude-haiku-4-5"),
        "message_start.model is the served-model alias"
    );
    let u = completed_usage(&events);
    // Usage coalesced on the terminal frame: input counters from
    // `message_start`, output total from `message_delta`.
    assert_eq!(u.input_total, 12);
    assert_eq!(u.output_total, 7);
    assert!(state.raw_usage.is_some());
}

#[test]
fn anthropic_nonstreaming_message_folds() {
    let dialect = dialects::anthropic_messages();
    let doc = hh_wire::json::parse_provider(
        r#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-haiku-4-5",
           "content":[{"type":"text","text":"done"}],
           "stop_reason":"end_turn","stop_sequence":null,
           "usage":{"input_tokens":5,"output_tokens":3}}"#,
    )
    .unwrap();
    let frames: Vec<(u64, WireFrame)> = anthropic::completion_frames(&doc)
        .unwrap()
        .into_iter()
        .map(|f| (0u64, f))
        .collect();
    let (events, state) = decode_all(&dialect, &frames);
    assert!(
        state.violations.is_empty(),
        "violations: {:?}",
        state.violations
    );
    let msg = completed(&events);
    assert_eq!(msg.text(), "done");
    assert_eq!(msg.stop_reason, StopReason::EndTurn);
    let u = completed_usage(&events);
    assert_eq!(u.input_total, 5);
    assert_eq!(u.output_total, 3);
}

#[test]
fn anthropic_tool_use_partial_json_assembles() {
    let dialect = dialects::anthropic_messages();
    let mut tr = anthropic::MessagesTranslator::default();
    let docs = [
        ev(
            "message_start",
            Json::obj([(
                "message",
                Json::obj([
                    ("id", Json::str("msg_2")),
                    ("model", Json::str("claude-haiku-4-5")),
                    (
                        "usage",
                        Json::obj([
                            ("input_tokens", Json::Int(20)),
                            ("output_tokens", Json::Int(1)),
                        ]),
                    ),
                ]),
            )]),
        ),
        ev(
            "content_block_start",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "content_block",
                    Json::obj([
                        ("type", Json::str("tool_use")),
                        ("id", Json::str("toolu_01")),
                        ("name", Json::str("fs_read")),
                        ("input", Json::obj([])),
                    ]),
                ),
            ]),
        ),
        ev(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("input_json_delta")),
                        ("partial_json", Json::str("{\"path\":")),
                    ]),
                ),
            ]),
        ),
        ev(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("input_json_delta")),
                        ("partial_json", Json::str("\"a.rs\"}")),
                    ]),
                ),
            ]),
        ),
        ev("content_block_stop", Json::obj([("index", Json::Int(0))])),
        ev(
            "message_delta",
            Json::obj([
                ("delta", Json::obj([("stop_reason", Json::str("tool_use"))])),
                ("usage", Json::obj([("output_tokens", Json::Int(11))])),
            ]),
        ),
        ev("message_stop", Json::obj([])),
    ];
    let mut frames = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        for f in tr.push_event(d).unwrap() {
            frames.push(((i * 10) as u64, f));
        }
    }
    let (events, state) = decode_all(&dialect, &frames);
    assert!(
        state.violations.is_empty(),
        "violations: {:?}",
        state.violations
    );
    let msg = completed(&events);
    assert_eq!(msg.stop_reason, StopReason::ToolUse);
    let call = msg
        .blocks
        .iter()
        .find_map(|b| match b {
            hh_gateway::message::ModelBlock::ToolCall(tc) => Some(tc),
            _ => None,
        })
        .expect("a tool-call block lands");
    // The kernel owns the call id (`{mc}.tc{idx}`); the provider's
    // `toolu_01` is a surface alias on the block.
    assert_eq!(call.tool_call_id, "mc.t.tc0");
    assert_eq!(
        call.surface_ids.get("provider_call_id").map(String::as_str),
        Some("toolu_01")
    );
    assert_eq!(call.surface_name, "fs_read");
    match &call.arguments {
        hh_gateway::message::ToolArgs::Parsed(j) => {
            assert_eq!(j.get("path").and_then(Json::as_str), Some("a.rs"))
        }
        other => panic!("args parse: {other:?}"),
    }
}

#[test]
fn anthropic_thinking_and_signature_deposit() {
    let dialect = dialects::anthropic_messages();
    let mut tr = anthropic::MessagesTranslator::default();
    let docs = [
        ev(
            "message_start",
            Json::obj([(
                "message",
                Json::obj([
                    ("id", Json::str("msg_3")),
                    ("model", Json::str("m")),
                    ("usage", Json::obj([("input_tokens", Json::Int(9))])),
                ]),
            )]),
        ),
        ev(
            "content_block_start",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "content_block",
                    Json::obj([("type", Json::str("thinking"))]),
                ),
            ]),
        ),
        ev(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("thinking_delta")),
                        ("thinking", Json::str("let me think")),
                    ]),
                ),
            ]),
        ),
        ev(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("signature_delta")),
                        ("signature", Json::str("sig-bytes")),
                    ]),
                ),
            ]),
        ),
        ev("content_block_stop", Json::obj([("index", Json::Int(0))])),
        // A real Messages turn never ends thinking-only — the grammar's
        // `thinking_only` refusal is the honest shape; the block after is
        // the provider's visible text.
        ev(
            "content_block_start",
            Json::obj([
                ("index", Json::Int(1)),
                ("content_block", Json::obj([("type", Json::str("text"))])),
            ]),
        ),
        ev(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(1)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("text_delta")),
                        ("text", Json::str("done")),
                    ]),
                ),
            ]),
        ),
        ev("content_block_stop", Json::obj([("index", Json::Int(1))])),
        ev(
            "message_delta",
            Json::obj([
                ("delta", Json::obj([("stop_reason", Json::str("end_turn"))])),
                ("usage", Json::obj([("output_tokens", Json::Int(4))])),
            ]),
        ),
        ev("message_stop", Json::obj([])),
    ];
    let mut frames = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        for f in tr.push_event(d).unwrap() {
            frames.push(((i * 10) as u64, f));
        }
    }
    let (events, state) = decode_all(&dialect, &frames);
    assert!(
        state.violations.is_empty(),
        "violations: {:?}",
        state.violations
    );
    let msg = completed(&events);
    assert_eq!(msg.text(), "done");
    let reasoning = msg
        .blocks
        .iter()
        .find_map(|b| match b {
            hh_gateway::message::ModelBlock::Reasoning {
                text,
                signature,
                redacted,
                ..
            } => Some((text, signature, *redacted)),
            _ => None,
        })
        .expect("a reasoning block lands");
    assert_eq!(
        reasoning.0.as_ref().map(|t| t.content.as_str()),
        Some("let me think")
    );
    assert!(!reasoning.2);
    assert!(
        reasoning.1.is_some(),
        "the signature deposits as the block's OpaqueLeaf"
    );
}

#[test]
fn anthropic_stop_reason_map_complete() {
    let dialect = dialects::anthropic_messages();
    let cases = [
        ("end_turn", StopReason::EndTurn),
        ("tool_use", StopReason::ToolUse),
        ("max_tokens", StopReason::MaxOutput),
        ("stop_sequence", StopReason::StopSequence),
        ("pause_turn", StopReason::PauseTurn),
        ("refusal", StopReason::Refusal),
    ];
    for (raw, want) in cases {
        let (got, _) = codec::map_stop_reason(&dialect, raw);
        assert_eq!(got, want, "stop_reason {raw}");
    }
    // An unmapped spelling preserves the raw value, never collapses.
    let (got, detail) = codec::map_stop_reason(&dialect, "some_future_reason");
    assert_eq!(got, StopReason::Unknown);
    assert_eq!(detail.as_deref(), Some("some_future_reason"));
}

#[test]
fn anthropic_error_code_class_maps() {
    let dialect = dialects::anthropic_messages();
    // `error.type` is the provider's class signal — the adapter copies it
    // to `error.code`; a code absent the type falls to the kernel status
    // table. The status table rows the kernel owns already agree with
    // Anthropic's status semantics — no `status_to_class` rows exist to
    // shadow the codes.
    let cases: &[(&str, u32, ModelErrorClass)] = &[
        ("authentication_error", 401, ModelErrorClass::Auth),
        ("permission_error", 403, ModelErrorClass::Forbidden),
        (
            "not_found_error",
            404,
            ModelErrorClass::NotFound(hh_gateway::vocab::NotFoundKind::Model),
        ),
        ("rate_limit_error", 429, ModelErrorClass::RateLimited),
        ("overloaded_error", 529, ModelErrorClass::Overloaded),
        ("api_error", 500, ModelErrorClass::ServerError),
        (
            "invalid_request_error",
            400,
            ModelErrorClass::InvalidRequest,
        ),
        ("billing_error", 400, ModelErrorClass::CreditsExhausted),
        (
            "request_too_large",
            413,
            ModelErrorClass::ContextLengthExceeded,
        ),
        (
            "timeout_error",
            500,
            ModelErrorClass::Timeout(hh_gateway::vocab::TimeoutKind::Attempt),
        ),
    ];
    for (ty, status, want) in cases {
        let body = anthropic::error_body_for_classify(&Json::obj([(
            "error",
            Json::obj([("type", Json::str(*ty)), ("message", Json::str("boom"))]),
        )]));
        let err = codec::classify_http_error(&dialect, *status, &body);
        assert_eq!(&err.class, want, "error.type {ty} @ {status}");
    }
    // A typeless body classifies on status alone (the kernel table).
    let err = codec::classify_http_error(&dialect, 401, &Json::obj([("error", Json::obj([]))]));
    assert_eq!(err.class, ModelErrorClass::Auth);
}

#[test]
fn anthropic_ping_is_notice() {
    let dialect = dialects::anthropic_messages();
    let mut tr = anthropic::MessagesTranslator::default();
    let frames: Vec<(u64, WireFrame)> = tr
        .push_event(&Json::obj([("type", Json::str("ping"))]))
        .unwrap()
        .into_iter()
        .map(|f| (0u64, f))
        .collect();
    let (events, state) = decode_all(&dialect, &frames);
    assert!(state.violations.is_empty());
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, ModelEventKind::ProviderNotice { .. })));
}

#[test]
fn anthropic_unknown_delta_is_notice() {
    let dialect = dialects::anthropic_messages();
    let mut tr = anthropic::MessagesTranslator::default();
    // A declared opaque delta kind (`citations_delta`) rides the
    // `delta:<kind>` spelling — a provider notice, no violation.
    let declared = Json::obj([
        ("type", Json::str("content_block_delta")),
        ("index", Json::Int(0)),
        (
            "delta",
            Json::obj([
                ("type", Json::str("citations_delta")),
                ("citation", Json::str("…")),
            ]),
        ),
    ]);
    let frames: Vec<(u64, WireFrame)> = tr
        .push_event(&declared)
        .unwrap()
        .into_iter()
        .map(|f| (0u64, f))
        .collect();
    let (events, state) = decode_all(&dialect, &frames);
    assert!(
        state.violations.is_empty(),
        "declared delta kinds are notices, not violations"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, ModelEventKind::ProviderNotice { .. })));
    // A kind the dialect never declared is preserved AND recorded as an
    // `unknown_frame` violation — honest, never dropped silently.
    let mut tr2 = anthropic::MessagesTranslator::default();
    let undeclared = Json::obj([
        ("type", Json::str("content_block_delta")),
        ("index", Json::Int(0)),
        ("delta", Json::obj([("type", Json::str("future_delta"))])),
    ]);
    let frames2: Vec<(u64, WireFrame)> = tr2
        .push_event(&undeclared)
        .unwrap()
        .into_iter()
        .map(|f| (0u64, f))
        .collect();
    let (events2, state2) = decode_all(&dialect, &frames2);
    assert!(state2.violations.iter().any(|v| matches!(
        v,
        hh_gateway::grammar::GrammarViolation::UnknownFrame { .. }
    )));
    assert!(events2
        .iter()
        .any(|e| matches!(e.kind, ModelEventKind::ProviderNotice { .. })));
}

// ── openai hosted dialect ─────────────────────────────────────────────────

#[test]
fn openai_error_code_class_maps() {
    let dialect = dialects::openai();
    let cases: &[(&str, u32, ModelErrorClass)] = &[
        ("rate_limit_exceeded", 429, ModelErrorClass::RateLimited),
        ("invalid_api_key", 401, ModelErrorClass::Auth),
        (
            "billing_hard_limit_reached",
            402,
            ModelErrorClass::CreditsExhausted,
        ),
    ];
    for (code, status, want) in cases {
        let body = Json::obj([(
            "error",
            Json::obj([("code", Json::str(*code)), ("message", Json::str("boom"))]),
        )]);
        let err = codec::classify_http_error(&dialect, *status, &body);
        assert_eq!(&err.class, want, "error.code {code} @ {status}");
    }
}

#[test]
fn openai_family_stop_reasons_inherit() {
    // The hosted dialect shares the family's finish_reason map — `stop`,
    // `tool_calls`, `length`, `content_filter` are the chat-completions
    // spellings (one spelling per fact — CC1).
    let dialect = dialects::openai();
    let cases = [
        ("stop", StopReason::EndTurn),
        ("tool_calls", StopReason::ToolUse),
        ("length", StopReason::MaxOutput),
        ("content_filter", StopReason::ContentFilter),
    ];
    for (raw, want) in cases {
        let (got, _) = codec::map_stop_reason(&dialect, raw);
        assert_eq!(got, want, "finish_reason {raw}");
    }
}

// ── credentials ───────────────────────────────────────────────────────────

#[test]
fn credential_ref_coordinate_parse() {
    assert_eq!(
        source_from_ref("env:ANTHROPIC_API_KEY").unwrap(),
        SecretSource::LaunchEnv {
            var: "ANTHROPIC_API_KEY".into()
        }
    );
    // `env:` with an empty name is a typed refusal.
    assert!(source_from_ref("env:").is_err());
    // Non-launch-env kinds are coordinates the port does not yet admit —
    // a typed refusal, never a guess.
    for bad in ["vault:item", "keyring:item", "external:ref", "bare-word"] {
        assert!(source_from_ref(bad).is_err(), "{bad} refused");
    }
    // The binding coordinate inverts back to its source.
    assert_eq!(
        source_from_binding("launch_env:OPENAI_API_KEY"),
        Some(SecretSource::LaunchEnv {
            var: "OPENAI_API_KEY".into()
        })
    );
    assert!(source_from_binding("os_keyring:item").is_none());
}

#[test]
fn credential_bind_carries_coordinate_only() {
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::LaunchEnv { var: "KEY".into() },
        "sk-TESTSECRET-do-not-leak",
    );
    let mut port = BrokerCredentialPort::new(Box::new(vault.clone()));
    let handle = port
        .bind(
            Some("env:KEY"),
            "https://api.example.test",
            &[hh_gateway::AuthKind::ApiKeyHeader],
        )
        .unwrap();
    assert_eq!(handle.binding_id.as_deref(), Some("launch_env:KEY"));
    assert!(handle.provided);
    // The handle's debug/JSON forms carry the coordinate, never the value.
    let dbg = format!("{handle:?}");
    assert!(!dbg.contains("sk-TESTSECRET"), "handle debug: {dbg}");
    // The transport-side resolve is the one place the value appears.
    assert_eq!(
        header_secret(&vault, &handle).unwrap(),
        "sk-TESTSECRET-do-not-leak"
    );
}

#[test]
fn credential_bind_fails_closed() {
    // A coordinate the resolver can't serve → CredentialUnavailable.
    let mut port = BrokerCredentialPort::new(Box::<StaticVault>::default());
    let err = port
        .bind(
            Some("env:MISSING"),
            "https://api.example.test",
            &[hh_gateway::AuthKind::ApiKeyHeader],
        )
        .unwrap_err();
    assert!(matches!(
        err,
        hh_gateway::GatewayError::CredentialUnavailable { .. }
    ));
    // No `credential_ref` under an auth-requiring dialect → refused.
    let mut port2 = BrokerCredentialPort::new(Box::<StaticVault>::default());
    let err2 = port2
        .bind(
            None,
            "https://api.example.test",
            &[hh_gateway::AuthKind::Bearer],
        )
        .unwrap_err();
    assert!(matches!(
        err2,
        hh_gateway::GatewayError::CredentialUnavailable { .. }
    ));
    // A `credential_ref` under a `none`-auth dialect → refused (a bound
    // secret with nowhere legal to ride is a fault, not a convenience).
    let mut port3 = BrokerCredentialPort::new(Box::<StaticVault>::default());
    let err3 = port3
        .bind(
            Some("env:KEY"),
            "http://127.0.0.1:11434",
            &[hh_gateway::AuthKind::None],
        )
        .unwrap_err();
    assert!(matches!(
        err3,
        hh_gateway::GatewayError::CredentialUnavailable { .. }
    ));
    // And `none`-auth with no ref binds `provided: no` — the local lane.
    let mut port4 = BrokerCredentialPort::new(Box::<StaticVault>::default());
    let h = port4
        .bind(
            None,
            "http://127.0.0.1:11434",
            &[hh_gateway::AuthKind::None],
        )
        .unwrap();
    assert!(!h.provided);
    assert!(h.binding_id.is_none());
}

// ── transport: auth injection + the wire path ────────────────────────────

#[test]
fn transport_injects_anthropic_headers() {
    let (ep, join) = serve_once(Box::leak(
        sse_response(&anthropic_sse_text()).into_boxed_str(),
    ));
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::LaunchEnv {
            var: "ANT_KEY".into(),
        },
        "sk-ant-TESTSECRET",
    );
    let dialect = dialects::anthropic_messages();
    let mut t = HttpModelTransport::with_credentials(
        dialect.clone(),
        hh_tls::ClientTls::webpki_roots(),
        Box::new(vault),
    );
    let credential = CredentialHandle {
        binding_id: Some("launch_env:ANT_KEY".into()),
        provided: true,
    };
    let body = anthropic::messages_request_body(
        "claude-haiku-4-5",
        &[anthropic::text_msg("user", "hi")],
        &[],
        true,
        &BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
    );
    let frames = t
        .send(
            &plan(&format!("{ep}/v1"), &dialect, body),
            &credential,
            &format!("{ep}/v1"),
        )
        .unwrap();
    let (head, _target_line, body_in) = join.join().unwrap();
    assert!(
        head.starts_with("POST /v1/messages "),
        "the Messages op path: {}",
        head.lines().next().unwrap_or("")
    );
    assert!(
        head.to_lowercase().contains("x-api-key: sk-ant-testsecret"),
        "x-api-key injected: {head}"
    );
    assert!(
        head.contains("anthropic-version: 2023-06-01"),
        "anthropic-version header: {head}"
    );
    // The credential value never lands in a frame — the decoded output
    // is the provider's stream, nothing else.
    let framed = frames
        .iter()
        .map(|(_, f)| f.data.to_canonical_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!framed.contains("sk-ant-TESTSECRET"));
    assert!(!String::from_utf8_lossy(&body_in).contains("sk-ant-TESTSECRET"));
}

#[test]
fn transport_injects_openai_bearer() {
    let chunk = r#"data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"hi"},"finish_reason":null}]}

data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}

data: [DONE]

"#;
    let (ep, join) = serve_once(Box::leak(sse_response(chunk).into_boxed_str()));
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::LaunchEnv {
            var: "OAI_KEY".into(),
        },
        "sk-oai-TESTSECRET",
    );
    let dialect = dialects::openai();
    let mut t = HttpModelTransport::with_credentials(
        dialect.clone(),
        hh_tls::ClientTls::webpki_roots(),
        Box::new(vault),
    );
    let credential = CredentialHandle {
        binding_id: Some("launch_env:OAI_KEY".into()),
        provided: true,
    };
    let body = openai::chat_request_body(
        "gpt-4.1-mini",
        &[openai::text_msg("user", "hi")],
        &[],
        true,
        &BTreeMap::from([(
            "stream_options".to_string(),
            Json::obj([("include_usage", Json::Bool(true))]),
        )]),
    );
    let frames = t
        .send(
            &plan(&format!("{ep}/v1"), &dialect, body),
            &credential,
            &format!("{ep}/v1"),
        )
        .unwrap();
    let (head, _t, _b) = join.join().unwrap();
    assert!(
        head.starts_with("POST /v1/chat/completions "),
        "the chat.completions op path: {}",
        head.lines().next().unwrap_or("")
    );
    assert!(
        head.to_lowercase()
            .contains("authorization: bearer sk-oai-testsecret"),
        "Bearer injected: {head}"
    );
    let (events, state) = decode_all(&dialect, &frames);
    assert!(state.violations.is_empty());
    let msg = completed(&events);
    assert_eq!(msg.text(), "hi");
    assert_eq!(msg.stop_reason, StopReason::EndTurn);
    let u = completed_usage(&events);
    assert_eq!(u.input_total, 5);
    assert_eq!(u.output_total, 2);
}

// ── AC-R-2.3.1-11 — the OOP live transport member ────────────────────────

#[test]
fn stub_oop_live_transport_binds_and_sends() {
    // The out-of-process live variant: `hh-gateway-stub` bound with
    // `transport: "live"` rides the real `HttpModelTransport` —
    // the credential *coordinate* crosses the boundary (`env:NAME`),
    // the value lives only in the child process's env.
    let (ep, join) = serve_once(Box::leak(
        sse_response(&anthropic_sse_text()).into_boxed_str(),
    ));
    let req = Json::obj([
        ("command", Json::str("run_call")),
        ("transport", Json::str("live")),
        ("dialect", dialects::anthropic_messages_doc()),
        (
            "endpoint_allowlist",
            Json::Arr(vec![Json::str(format!("{ep}/v1"))]),
        ),
        ("model_call_id", Json::str("mc-oop")),
        ("normalizer_ref", Json::str("test#usage_mapping")),
        (
            "plan",
            Json::obj([
                ("endpoint_ref", Json::str(format!("{ep}/v1"))),
                ("model_ref", Json::str("claude-haiku-4-5")),
                ("credential_ref", Json::str("env:HH_R45D_OOP_KEY")),
                ("budget_reservation_id", Json::str("br-oop")),
                ("provider_params", Json::obj([("max_tokens", Json::Int(8))])),
                (
                    "body",
                    anthropic::messages_request_body(
                        "claude-haiku-4-5",
                        &[anthropic::text_msg("user", "hi")],
                        &[],
                        true,
                        &BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
                    ),
                ),
            ]),
        ),
    ]);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hh-gateway-stub"))
        .env("HH_R45D_OOP_KEY", "sk-oop-TESTSECRET")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write as _;
            c.stdin
                .as_mut()
                .unwrap()
                .write_all(req.to_canonical_string().as_bytes())?;
            c.wait_with_output()
        })
        .expect("hh-gateway-stub spawns");
    let (head, _t, _b) = join.join().unwrap();
    // The secret reached the wire as the header — never the boundary.
    assert!(
        head.to_lowercase().contains("x-api-key: sk-oop-testsecret"),
        "the live OOP send injects the header: {head}"
    );
    let doc = hh_wire::json::parse(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(
        doc.get("result").and_then(Json::as_str),
        Some("completed"),
        "the OOP live call completes: {}",
        doc.to_canonical_string()
    );
    // `credential_binding_id` carries the coordinate — and the secret
    // value is nowhere in the emitted boundary record.
    let events = doc.get("events").expect("events");
    let requested = match events {
        Json::Arr(v) => v
            .iter()
            .find(|e| e.get("class").and_then(Json::as_str) == Some("model.call.requested"))
            .cloned()
            .expect("model.call.requested emitted"),
        _ => panic!("events array"),
    };
    assert_eq!(
        requested
            .get("payload")
            .and_then(|p| p.get("credential_binding_id"))
            .and_then(Json::as_str),
        Some("launch_env:HH_R45D_OOP_KEY"),
        "the binding id is the coordinate"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("sk-oop-TESTSECRET"),
        "no secret value crosses the OOP boundary"
    );
}

// ── usage → cost (pinned pricing, never a silent zero) ────────────────────

#[test]
fn usage_to_cost_pinned_pricing() {
    use hh_budget::{price, ModelRef, PricingRow, PricingTable, TokenDecomposition};
    // The pinned table — USD micro-units per token (micro = 1e-6 USD;
    // e.g. 1000 micro/token ⇒ $1/1M).
    let table = PricingTable {
        table_id: "test/live-pricing".into(),
        version: "2026-10-21".into(),
        currency: "USD".into(),
        rows: vec![PricingRow {
            model_ref: "claude-haiku-4-5@anthropic".into(),
            input_uncached: 1_000,
            input_cache_read: 100,
            input_cache_write: BTreeMap::from([("5m".to_string(), 1_250)]),
            output_visible: 5_000,
            output_reasoning: None,
            tiers: vec![],
            valid_from: "2026-10-21".into(),
            source: "test-pinned rates".into(),
        }],
    };
    let decomp = TokenDecomposition {
        input_uncached: 10,
        cache_read: 0,
        cache_write: BTreeMap::new(),
        output_visible: 7,
        output_reasoning: 0,
    };
    let mref = ModelRef {
        profile_ref: "prof.test".into(),
        provider_model_id: "claude-haiku-4-5".into(),
        serving_route: "anthropic".into(),
        effort: None,
    };
    let (money, roles) = price(&decomp, &mref, &table).unwrap();
    assert_eq!(money.currency, "USD");
    assert_eq!(money.micro_units, 10 * 1_000 + 7 * 5_000);
    assert_eq!(roles.get("tokens.input.uncached"), Some(&10_000));
    // No row for the model ⇒ `NoPrice`, never a silent zero.
    let missing = ModelRef {
        provider_model_id: "unpriced-model".into(),
        ..mref.clone()
    };
    assert!(matches!(
        price(&decomp, &missing, &table),
        Err(hh_budget::SpendError::NoPrice { .. })
    ));
}

// ── dialect doc → live family dispatch ────────────────────────────────────

#[test]
fn family_dispatch_is_dialect_bound() {
    assert_eq!(
        hh_gateway::family_of(&dialects::anthropic_messages()),
        hh_gateway::Family::Messages
    );
    assert_eq!(
        hh_gateway::family_of(&dialects::openai()),
        hh_gateway::Family::ChatCompletions
    );
    assert_eq!(
        hh_gateway::family_of(&dialects::openai_compatible()),
        hh_gateway::Family::ChatCompletions
    );
}
