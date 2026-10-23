//! R4.5c — `HttpModelTransport` over TLS (ADR-0365; DF-S2.4-2's model
//! lane). The committed TEST-ONLY PKI (`hh-tls/fixtures/pki`) terminates
//! TLS at the fixture server; the cells pin: `https:` legs ride the real
//! handshake beneath the one codec, the extra-roots file is the only
//! path to a private issuer, a bad chain is a typed refusal (never a
//! plaintext downgrade), and mTLS presents the client certificate.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Duration;

use hh_gateway::codec::WireFrame;
use hh_gateway::dialects;
use hh_gateway::gateway::{CredentialHandle, Transport};
use hh_gateway::http_transport::HttpModelTransport;
use hh_gateway::openai;
use hh_gateway::plan::{CachePlan, Capabilities, ProviderRequestPlan};
use hh_gateway::vocab::ModelErrorClass;
use hh_tls::{ClientTls, ServerTls, ServerTlsStream};
use hh_wire::json::Json;

fn pki(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../hh-tls/fixtures/pki/{name}"))
}

fn pem(name: &str) -> String {
    std::fs::read_to_string(pki(name)).unwrap()
}

fn client() -> ClientTls {
    ClientTls::webpki_roots()
        .extra_roots_file(&pki("hh-test-ca.pem"))
        .unwrap()
}

/// One-shot TLS-terminated HTTP server: accept, TLS-handshake, read the
/// bounded request through the ONE codec, write `response`, close.
/// Returns `(endpoint, join)` — the join carries the captured request
/// head's method+target.
fn serve_tls_once(
    tls: ServerTls,
    response: String,
) -> (String, std::thread::JoinHandle<(String, String)>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(10))).ok();
        sock.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let mut s: ServerTlsStream = tls.accept(sock).unwrap();
        let (method, target) = {
            let mut r = BufReader::new(&mut s);
            let (head, _body) = hh_wire::http::read_request(&mut r, 1024 * 1024)
                .unwrap()
                .expect("a request head");
            (head.method, head.target)
        };
        hh_wire::http::write_response(
            &mut s,
            200,
            &[("Content-Type", "application/json")],
            response.as_bytes(),
        )
        .unwrap();
        (method, target)
    });
    (format!("https://localhost:{port}"), h)
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

const COMPLETION: &str = "{\"id\":\"chatcmpl-t1\",\"object\":\"chat.completion\",\"model\":\"fixture-model\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}";

// ── the cells ────────────────────────────────────────────────────────────────

/// `send` over a real TLS session: the request crosses `https:` through
/// the extra-roots-verified handshake and the completion folds to the
/// same normalized frames — the codec never knows TLS happened.
#[test]
fn send_over_tls_folds_completion() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (endpoint, join) = serve_tls_once(tls, COMPLETION.to_string());
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::with_tls(d, client());
    let bytes = hh_gateway::codec::serialize(&plan(&endpoint, false));
    let frames: Vec<(u64, WireFrame)> = t.send(&bytes, &anon(), &endpoint).unwrap();
    let (method, target) = join.join().unwrap();
    assert_eq!(
        (method.as_str(), target.as_str()),
        ("POST", "/chat/completions")
    );
    let kinds: Vec<&str> = frames.iter().map(|(_, f)| f.kind.as_str()).collect();
    assert_eq!(kinds.first(), Some(&"stream_start"));
    assert_eq!(kinds.last(), Some(&"stream_end"));
}

/// `discover` over TLS answers the listing verbatim.
#[test]
fn discover_over_tls_lists() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (endpoint, join) = serve_tls_once(
        tls,
        "{\"object\":\"list\",\"data\":[{\"id\":\"fixture-model\",\"object\":\"model\"}]}"
            .to_string(),
    );
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::with_tls(d, client());
    let doc = t.discover(&anon(), &endpoint).unwrap();
    let (method, target) = join.join().unwrap();
    assert_eq!((method.as_str(), target.as_str()), ("GET", "/models"));
    match doc.get("data") {
        Some(Json::Arr(v)) => assert_eq!(v.len(), 1),
        other => panic!("expected a `data` array, got {other:?}"),
    }
}

/// The extra-roots file is the ONLY path to a private issuer: webpki-only
/// roots refuse the test CA's chain — a typed `network` refusal, never a
/// silently-accepted or silently-plaintext session.
#[test]
fn private_issuer_needs_explicit_roots() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (endpoint, join) = serve_tls_once(tls, COMPLETION.to_string());
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::new(d); // webpki roots only
    let err = t.discover(&anon(), &endpoint).unwrap_err();
    assert_eq!(err.class, ModelErrorClass::Network, "{err:?}");
    let _ = join.join();
}

/// A chain the roots don't cover is refused typed (rogue CA).
#[test]
fn rogue_chain_refused_typed() {
    let tls = ServerTls::new(&pem("rogue-server.pem"), &pem("rogue-server-key.pem")).unwrap();
    let (endpoint, join) = serve_tls_once(tls, COMPLETION.to_string());
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::with_tls(d, client());
    let err = t.discover(&anon(), &endpoint).unwrap_err();
    assert_eq!(err.class, ModelErrorClass::Network, "{err:?}");
    let _ = join.join();
}

/// mTLS — the client presents `client.pem`; the handshake completes and
/// the request crosses. (Server-side peer-identity extraction is the
/// hh-tls battery's cell; this pins the transport's presentation half.)
#[test]
fn mtls_presents_client_cert() {
    let tls = ServerTls::requiring_client_auth(
        &pem("server.pem"),
        &pem("server-key.pem"),
        &pem("hh-test-ca.pem"),
    )
    .unwrap();
    let (endpoint, join) = serve_tls_once(tls, COMPLETION.to_string());
    let c = client()
        .client_identity(&pem("client.pem"), &pem("client-key.pem"))
        .unwrap();
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::with_tls(d, c);
    let bytes = hh_gateway::codec::serialize(&plan(&endpoint, false));
    let frames = t.send(&bytes, &anon(), &endpoint).unwrap();
    assert!(!frames.is_empty());
    join.join().unwrap();
}

/// Missing client cert under `requiring_client_auth` — typed refusal,
/// never a session.
#[test]
fn missing_client_cert_refused() {
    let tls = ServerTls::requiring_client_auth(
        &pem("server.pem"),
        &pem("server-key.pem"),
        &pem("hh-test-ca.pem"),
    )
    .unwrap();
    let (endpoint, join) = serve_tls_once(tls, COMPLETION.to_string());
    let d = dialects::openai_compatible();
    let mut t = HttpModelTransport::with_tls(d, client());
    let err = t.discover(&anon(), &endpoint).unwrap_err();
    assert!(
        matches!(
            err.class,
            ModelErrorClass::Network | ModelErrorClass::UnsupportedFeature
        ),
        "expected a typed refusal, got {err:?}"
    );
    let _ = join.join();
}
