//! R6.7 — LANE acceptance battery (manifest 260, AR-11; REQ-XC-104,
//! alias REQ-XC-001). The Ollama same-dialect upstream lanes
//! (`ollama-chat@1`, `ollama-messages@1`) ride a new raw-forward
//! primitive on the gateway HTTP transport; the auth header derives
//! from the lane's declared `auth_kinds` — `Authorization: Bearer` on
//! every Ollama surface — and the credential stays confined to the
//! forwarding process.
//!
//! - AC-4 (`AC-4.1-2`): a Messages-family request through the Ollama
//!   lane reaches the fixture upstream (std `TcpListener` thread) with
//!   `Authorization: Bearer` and no `x-api-key`; the same for the chat
//!   lane.
//! - AC-5: the raw leg is byte-faithful — the fixture sees exactly the
//!   sent body bytes; the captured JSON and SSE responses equal the
//!   fixture's bytes; decoding them with the existing translators
//!   equals the `send` frames path.
//! - AC-6: `anthropic-messages@1` and the OpenAI dialects keep their
//!   current auth header bytes; a lane asked for an auth form its
//!   `auth_kinds` does not declare refuses typed.
//! - AC-7: `leak_scan` with the fixture credential finds no hit in any
//!   capture, error or record the lane returns.

#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpListener;

use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, Transport};
use hharness_gateway::http_transport::HttpModelTransport;
use hharness_gateway::vocab::ModelErrorClass;
use hharness_gateway::{dialect::AuthKind, family_of, Family};
use hharness_secrets::channel::SecretSource;
use hharness_secrets::StaticVault;
use hharness_wire::json::Json;

const SECRET: &str = "ollama-fixture-secret-9f2ac5d1";

/// One-shot HTTP/1.1 fixture upstream: accept one connection, read the
/// request head + `Content-Length` body, write `response`, close. The
/// join yields `(request_head_text, request_body_bytes)` — exactly what
/// the transport put on the wire.
fn serve_once(response: &'static str) -> (String, std::thread::JoinHandle<(String, Vec<u8>)>) {
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
                        if let Some(p) =
                            buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
                        {
                            head_end = Some(p);
                            want = String::from_utf8_lossy(&buf[..p])
                                .split("\r\n")
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|v| v.trim().parse().ok())
                                })
                                .unwrap_or(0);
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
        (
            String::from_utf8_lossy(&buf[..head_end]).to_string(),
            buf[head_end.min(buf.len())..].to_vec(),
        )
    });
    (format!("http://127.0.0.1:{port}/v1"), h)
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A close-delimited SSE response (exercises `BodyReader::Close` — the
/// streaming path an upstream actually speaks).
fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
        body
    )
}

fn transport(dialect: hharness_gateway::WireDialect) -> HttpModelTransport {
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::LaunchEnv {
            var: "OLLAMA_KEY".into(),
        },
        SECRET,
    );
    HttpModelTransport::with_credentials(
        dialect,
        hharness_tls::ClientTls::webpki_roots(),
        Box::new(vault),
    )
}

fn credential() -> CredentialHandle {
    CredentialHandle {
        binding_id: Some("launch_env:OLLAMA_KEY".into()),
        provided: true,
    }
}

fn leak_scan(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

// ── dialect documents ─────────────────────────────────────────────────

#[test]
fn ollama_dialect_docs_load_and_bind_family() {
    let c = hharness_gateway::WireDialect::from_json(&dialects::ollama_chat_doc())
        .expect("ollama-chat@1 document loads");
    assert_eq!(c.dialect_id, "ollama-chat");
    assert_eq!(c.auth_kinds, vec![AuthKind::Bearer]);
    assert!(!c.plan_schema.admits("tool_choice")); // §4.1: unsupported upstream
    assert!(c.plan_schema.admits("reasoning_effort"));
    assert_eq!(family_of(&c), Family::ChatCompletions);

    let m = hharness_gateway::WireDialect::from_json(&dialects::ollama_messages_doc())
        .expect("ollama-messages@1 document loads");
    assert_eq!(m.dialect_id, "ollama-messages");
    assert_eq!(m.auth_kinds, vec![AuthKind::Bearer]);
    assert!(m.plan_schema.required_keys.contains(&"max_tokens".into()));
    assert!(!m.plan_schema.admits("tool_choice"));
    assert!(!m.plan_schema.admits("metadata"));
    assert!(m.plan_schema.admits("budget_tokens")); // accepted, not enforced
    assert_eq!(family_of(&m), Family::Messages);
    assert!(m.count_tokens.is_none());
}

// ── AC-4: Bearer on every Ollama surface, no x-api-key ────────────────

#[test]
fn ac4_ollama_messages_lane_sends_bearer_not_x_api_key() {
    let (ep, h) = serve_once(Box::leak(json_response("{}").into_boxed_str()));
    let mut t = transport(dialects::ollama_messages());
    let body =
        br#"{"model":"glm-5.3-flash","max_tokens":4,"messages":[{"role":"user","content":"hi"}]}"#;
    let ex = t
        .forward_raw(
            &credential(),
            &ep,
            "messages",
            &[
                ("content-type", "application/json"),
                ("anthropic-version", "2023-06-01"),
            ],
            body,
            None,
        )
        .unwrap();
    assert_eq!(ex.status, 200);
    let (head, got_body) = h.join().unwrap();
    assert!(head.contains("POST /v1/messages HTTP/1.1"), "head: {head}");
    assert!(
        head.contains(&format!("authorization: Bearer {SECRET}"))
            || head.contains(&format!("Authorization: Bearer {SECRET}")),
        "head: {head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("x-api-key"),
        "head: {head}"
    );
    assert_eq!(got_body, body, "the body is forwarded verbatim");
}

#[test]
fn ac4_ollama_chat_lane_sends_bearer() {
    let (ep, h) = serve_once(Box::leak(json_response("{}").into_boxed_str()));
    let mut t = transport(dialects::ollama_chat());
    let body =
        br#"{"model":"glm-5.3-flash","messages":[{"role":"user","content":"hi"}],"stream":true}"#;
    let ex = t
        .forward_raw(
            &credential(),
            &ep,
            "chat/completions",
            &[
                ("content-type", "application/json"),
                ("accept", "text/event-stream"),
            ],
            body,
            None,
        )
        .unwrap();
    assert_eq!(ex.status, 200);
    let (head, got_body) = h.join().unwrap();
    assert!(
        head.contains("POST /v1/chat/completions HTTP/1.1"),
        "head: {head}"
    );
    assert!(
        head.to_ascii_lowercase().contains("authorization: bearer "),
        "head: {head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("x-api-key"),
        "head: {head}"
    );
    assert_eq!(got_body, body);
}

// ── AC-5: byte-faithful capture + decode equals the frames path ───────

#[test]
fn ac5_json_capture_is_byte_identical_and_decodes() {
    const BODY: &str = r#"{"id":"chatcmpl-1","model":"glm-5.3-flash","choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":11,"completion_tokens":3}}"#;
    let (ep, h) = serve_once(Box::leak(json_response(BODY).into_boxed_str()));
    let mut t = transport(dialects::ollama_chat());
    let ex = t
        .forward_raw(&credential(), &ep, "chat/completions", &[], b"{}", None)
        .unwrap();
    let (_head, _req) = h.join().unwrap();
    assert_eq!(ex.status, 200);
    assert_eq!(ex.body, BODY.as_bytes(), "the capture is byte-identical");
    // The recorded chunks cover the whole body.
    assert_eq!(
        ex.chunk_stamps.iter().map(|(_, n)| *n).sum::<usize>(),
        ex.body.len()
    );
    // Decoding the captured bytes rides the family's completion_frames.
    let frames = t.raw_frames(&ex).unwrap();
    let doc = hharness_wire::json::parse_provider(BODY).unwrap();
    let expected = hharness_gateway::openai::completion_frames(&doc).unwrap();
    let got: Vec<WireFrame> = frames.iter().map(|(_, f)| f.clone()).collect();
    assert_eq!(got, expected, "raw decode equals the frames path");
}

#[test]
fn ac5_sse_capture_is_byte_identical_and_decodes_like_send() {
    const SSE: &str = "data: {\"id\":\"c1\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"}}]}\n\ndata: {\"id\":\"c1\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";

    // Leg 1 — the `send` frames path over an identical fixture.
    let (ep_send, h_send) = serve_once(Box::leak(sse_response(SSE).into_boxed_str()));
    let mut t_send = transport(dialects::ollama_chat());
    let plan = Json::obj([(
        "body",
        Json::obj([
            ("model", Json::str("glm-5.3-flash")),
            ("stream", Json::Bool(true)),
            ("messages", Json::Arr(vec![])),
        ]),
    )])
    .to_canonical_string()
    .into_bytes();
    let send_frames: Vec<WireFrame> = t_send
        .send(&plan, &credential(), &ep_send)
        .unwrap()
        .iter()
        .map(|(_, f)| f.clone())
        .collect();
    let _ = h_send.join().unwrap();

    // Leg 2 — raw forward + `raw_frames` over an identical fixture.
    let (ep_raw, h_raw) = serve_once(Box::leak(sse_response(SSE).into_boxed_str()));
    let mut t_raw = transport(dialects::ollama_chat());
    let ex = t_raw
        .forward_raw(&credential(), &ep_raw, "chat/completions", &[], b"{}", None)
        .unwrap();
    let _ = h_raw.join().unwrap();
    assert_eq!(ex.body, SSE.as_bytes(), "the capture is byte-identical");
    let raw_frames: Vec<WireFrame> = t_raw
        .raw_frames(&ex)
        .unwrap()
        .iter()
        .map(|(_, f)| f.clone())
        .collect();

    assert_eq!(raw_frames, send_frames, "raw decode equals the frames path");
    assert!(!raw_frames.is_empty());
}

// ── AC-6: existing auth bytes + the undeclared-form refusal ───────────

fn post_op_head(dialect: hharness_gateway::WireDialect, provided: bool) -> String {
    let (ep, h) = serve_once(Box::leak(json_response("{}").into_boxed_str()));
    let mut t = transport(dialect);
    let cred = if provided {
        credential()
    } else {
        CredentialHandle {
            binding_id: None,
            provided: false,
        }
    };
    let _ = t.post_op(&cred, &ep, "messages", b"{}", &[]);
    h.join().unwrap().0
}

#[test]
fn ac6_existing_dialects_keep_their_auth_header_bytes() {
    // The family refactor must not change one byte of the admitted
    // dialects' auth wire form.
    let head = post_op_head(dialects::anthropic_messages(), true);
    assert!(
        head.contains(&format!("x-api-key: {SECRET}")),
        "anthropic lane: {head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "anthropic lane: {head}"
    );
    let head = post_op_head(dialects::openai(), true);
    assert!(
        head.to_ascii_lowercase().contains("authorization: bearer "),
        "openai lane: {head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("x-api-key"),
        "openai lane: {head}"
    );
    let head = post_op_head(dialects::openai_compatible(), true);
    assert!(
        head.to_ascii_lowercase().contains("authorization: bearer "),
        "compatible lane: {head}"
    );
    // … and the credential-less local lane still sends no auth header.
    let head = post_op_head(dialects::openai_compatible(), false);
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "local lane: {head}"
    );
}

#[test]
fn ac6_undeclared_auth_form_refuses_typed() {
    let (ep, _h) = serve_once(Box::leak(json_response("{}").into_boxed_str()));
    let mut t = transport(dialects::ollama_messages());
    // The Ollama Messages lane declares Bearer only — an x-api-key ask
    // is a typed refusal, never a silent swap.
    let err = t
        .forward_raw(
            &credential(),
            &ep,
            "messages",
            &[],
            b"{}",
            Some(AuthKind::ApiKeyHeader),
        )
        .unwrap_err();
    assert_eq!(err.class, ModelErrorClass::UnsupportedFeature);
    assert!(err.to_string().contains("auth_kinds"), "err: {err}");
    // … and `none` is not admissible on an authenticated lane either.
    let err = t
        .forward_raw(
            &credential(),
            &ep,
            "messages",
            &[],
            b"{}",
            Some(AuthKind::None),
        )
        .unwrap_err();
    assert_eq!(err.class, ModelErrorClass::UnsupportedFeature);
    // A caller-supplied auth header is refused — the wire form is the
    // lane's own.
    let err = t
        .forward_raw(
            &credential(),
            &ep,
            "messages",
            &[("authorization", "Bearer caller-injected")],
            b"{}",
            None,
        )
        .unwrap_err();
    assert_eq!(err.class, ModelErrorClass::InvalidRequest);
}

// ── AC-7: the credential is confined to the forwarding process ────────

#[test]
fn ac7_leak_scan_finds_no_credential_in_any_returned_surface() {
    // Success exchange.
    let (ep, h) = serve_once(Box::leak(json_response("{}").into_boxed_str()));
    let mut t = transport(dialects::ollama_messages());
    let ex = t
        .forward_raw(&credential(), &ep, "messages", &[], b"{}", None)
        .unwrap();
    let (head, _) = h.join().unwrap();
    // Sanity: the fixture upstream DID receive the credential — the
    // lane carries it, the returned capture does not.
    assert!(
        leak_scan(head.as_bytes(), SECRET.as_bytes()),
        "upstream saw the key"
    );
    assert!(!leak_scan(&ex.body, SECRET.as_bytes()));
    assert!(!leak_scan(
        format!("{:?}", ex.headers).as_bytes(),
        SECRET.as_bytes()
    ));
    assert!(!leak_scan(format!("{ex:?}").as_bytes(), SECRET.as_bytes()));

    // A non-2xx exchange — captured verbatim, still credential-free.
    let err_response: &'static str = Box::leak(
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"error\":\"invalid api key\"}"
            .into(),
    );
    let (ep, h) = serve_once(err_response);
    let ex = t
        .forward_raw(&credential(), &ep, "messages", &[], b"{}", None)
        .unwrap();
    let _ = h.join().unwrap();
    assert_eq!(ex.status, 401);
    assert!(!leak_scan(&ex.body, SECRET.as_bytes()));
    assert!(!leak_scan(format!("{ex:?}").as_bytes(), SECRET.as_bytes()));

    // Typed refusals carry the class and the coordinate, never the
    // value.
    let err = t
        .forward_raw(
            &credential(),
            &ep,
            "messages",
            &[],
            b"{}",
            Some(AuthKind::None),
        )
        .unwrap_err();
    assert!(!leak_scan(err.to_string().as_bytes(), SECRET.as_bytes()));
    assert!(!leak_scan(format!("{err:?}").as_bytes(), SECRET.as_bytes()));
    // A vanished binding is `auth` — naming the coordinate only.
    let unbound = CredentialHandle {
        binding_id: Some("launch_env:NO_SUCH_KEY".into()),
        provided: true,
    };
    let err = t
        .forward_raw(&unbound, &ep, "messages", &[], b"{}", None)
        .unwrap_err();
    assert_eq!(err.class, ModelErrorClass::Auth);
    assert!(!leak_scan(err.to_string().as_bytes(), SECRET.as_bytes()));
}
