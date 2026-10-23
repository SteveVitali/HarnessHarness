//! R4.5b — the gateway-backed `ModelPort` on the run path (DF-S1.18-1;
//! ADR-0364 D4; AC1/AC2). A `native.slots["model_port"]` binding carries
//! the realized live lane (`dialect`, `endpoint`, `model`,
//! `provider_params`); `drive` selects `GatewayModel` over the scripted
//! `EmbedModel` and the ledger gains the live members — `plan_hash`,
//! `endpoint_ref`, `model_ref{profile_ref, provider_model_id}`,
//! `usage{record{view,normalizer_ref}}` (the provider's verbatim raw
//! document rides `outcome.usage_raw` on the model_io blob — the row's
//! OPEN_AUDIT bound keeps bulky provider documents off the inline audit
//! field), `timing`, `served_model`,
//! `credential_binding_id` — plus the `hh.model_io/1` blob the
//! `response_ref` resolves to.
//!
//! Hermetic: a loopback `TcpListener` fixture answers the dialect's
//! frames — no live provider claim (the local Ollama leg rides
//! `scripts/live-verify.sh local-run`; ADR-0363). R4.5c's TLS lane cells
//! (the `tls{}` declaration, the extra-roots hook, the typed refusals)
//! ride the committed test PKI (`hh-tls/fixtures/pki` — ADR-0365).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hharness_assembly::grammar::Assembly;
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

// ── fixture: the conformance document + a bound `model_port` slot ────────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

/// The conformance document — `Some(params)` binds `slots["model_port"]`.
fn document_json(model_port: Option<BTreeMap<String, Json>>) -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(100_000),
                        soft: None,
                    },
                )]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let mut slots = BTreeMap::new();
    if let Some(params) = model_port {
        slots.insert(
            "model_port".to_string(),
            SlotBindings::One(SlotBinding {
                variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
                params,
                enabled: true,
                locality: None,
            }),
        );
    }
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

/// The live-lane slot params — literal endpoint/model members.
fn lane_params(endpoint: &str, model: &str) -> BTreeMap<String, Json> {
    BTreeMap::from([
        ("dialect".to_string(), Json::str("openai-compatible@1")),
        ("endpoint".to_string(), Json::str(endpoint)),
        ("provider_model_id".to_string(), Json::str(model)),
    ])
}

// ── service helpers (the conformance battery's shape) ───────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r45b-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service_in(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.5b".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r4.5b")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![("experimental", Json::Bool(true))]),
            ),
        ]),
    );
    ok(&r);
}

fn spec_for(doc: Json) -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![("kind", Json::str("document")), ("document", doc)]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj(vec![("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "attendance",
            AttendanceDeclaration {
                value: "async".into(),
                source: "declared".into(),
            }
            .to_json(),
        ),
    ])
}

fn open_live(svc: &mut EmbedService, doc: Json, key: &str) -> (String, String) {
    let s = ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc)),
            ("idempotency_key", Json::str(key)),
        ]),
    ));
    (
        s.get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        s.get("run_id").and_then(Json::as_str).unwrap().to_string(),
    )
}

fn submit(svc: &mut EmbedService, session: &str, input: Vec<Json>, key: &str) -> Json {
    call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            ("input", Json::Arr(input)),
            ("idempotency_key", Json::str(key)),
        ]),
    )
}

fn text_input(t: &str) -> Vec<Json> {
    vec![Json::obj(vec![
        ("kind", Json::str("text")),
        ("text", Json::str(t)),
    ])]
}

fn classes(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.store()
        .envelopes(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

// ── the loopback fake provider ────────────────────────────────────────────────

/// A one-shot OpenAI-compatible server: reads the request, writes
/// `response`. Returns `(endpoint, received_request_body)` — the body is
/// moved out through a shared slot once the connection is served.
struct FakeProvider {
    endpoint: String,
    got: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

fn fake_provider(response: String) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let got2 = got.clone();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 8192];
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                let done = head_complete(&buf);
                if done || std::time::Instant::now() > deadline {
                    break;
                }
                match sock.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            *got2.lock().unwrap() = Some(buf);
            let _ = sock.write_all(response.as_bytes());
            let _ = sock.flush();
        }
    });
    FakeProvider {
        endpoint: format!("http://127.0.0.1:{port}"),
        got,
    }
}

/// The TLS twin of `fake_provider` — `hharness_tls::ServerTls` terminates the
/// handshake over the committed test PKI, then the same loopback shape
/// serves (R4.5c; the response body is the caller's).
fn fake_provider_tls(response: String) -> FakeProvider {
    let tls = hharness_tls::ServerTls::new(
        &std::fs::read_to_string(pki("server.pem")).unwrap(),
        &std::fs::read_to_string(pki("server-key.pem")).unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let got2 = got.clone();
    std::thread::spawn(move || {
        if let Ok((sock, _)) = listener.accept() {
            sock.set_read_timeout(Some(std::time::Duration::from_secs(15)))
                .ok();
            if let Ok(mut s) = tls.accept(sock) {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                loop {
                    if head_complete(&buf) || std::time::Instant::now() > deadline {
                        break;
                    }
                    match s.read(&mut tmp) {
                        Ok(0) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                        Err(_) => break,
                    }
                }
                *got2.lock().unwrap() = Some(buf);
                let _ = s.write_all(response.as_bytes());
                let _ = s.flush();
            }
        }
    });
    FakeProvider {
        endpoint: format!("https://localhost:{port}"),
        got,
    }
}

fn pki(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../hh-tls/fixtures/pki/{name}"))
}

fn head_complete(buf: &[u8]) -> bool {
    let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..p]);
    let mut want = 0usize;
    for line in head.split("\r\n") {
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            want = v.trim().parse().unwrap_or(0);
        }
    }
    buf.len() >= p + 4 + want
}

impl FakeProvider {
    /// The request body the provider received (post-drive).
    fn request_json(&self) -> Json {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                return hharness_wire::json::parse(&String::from_utf8_lossy(&buf[p + 4..]))
                    .unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }
}

/// An SSE stream answering one `hh_submit` tool call, split across
/// argument deltas to exercise the grammar's accumulation leg.
fn sse_submit_stream(model: &str, text: &str) -> String {
    let args = Json::obj([("text", Json::str(text))]).to_canonical_string();
    let (a1, a2) = args.split_at(args.len() / 2);
    let chunk = |delta: Json, finish: Json, usage: Json| {
        let mut c = std::collections::BTreeMap::new();
        c.insert("id".into(), Json::str("chatcmpl-1"));
        c.insert("object".into(), Json::str("chat.completion.chunk"));
        c.insert("model".into(), Json::str(model));
        c.insert(
            "choices".into(),
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("delta", delta),
                ("finish_reason", finish),
            ])]),
        );
        if !matches!(usage, Json::Null) {
            c.insert("usage".into(), usage);
        }
        format!("data: {}\n\n", Json::Obj(c).to_canonical_string())
    };
    let usage = Json::obj([
        ("prompt_tokens", Json::Int(41)),
        ("completion_tokens", Json::Int(7)),
        ("total_tokens", Json::Int(48)),
    ]);
    let body = format!(
        "{}{}{}{}data: [DONE]\n\n",
        chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                (
                    "tool_calls",
                    Json::Arr(vec![Json::obj([
                        ("index", Json::Int(0)),
                        ("id", Json::str("call_abc")),
                        ("type", Json::str("function")),
                        (
                            "function",
                            Json::obj([
                                ("name", Json::str("hh_submit")),
                                ("arguments", Json::str(a1)),
                            ]),
                        ),
                    ])]),
                ),
            ]),
            Json::Null,
            Json::Null,
        ),
        chunk(
            Json::obj([(
                "tool_calls",
                Json::Arr(vec![Json::obj([
                    ("index", Json::Int(0)),
                    ("function", Json::obj([("arguments", Json::str(a2))]),),
                ])]),
            )]),
            Json::Null,
            Json::Null,
        ),
        chunk(Json::obj([]), Json::str("tool_calls"), usage),
        ""
    );
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

// ── the cells ────────────────────────────────────────────────────────────────

/// AC2 — the live lane drives a real HTTP exchange: the dialect's
/// request lands on the wire (`model`/`stream:true`/`tools[]` carrying
/// `hh_submit`), the SSE chunks decode to one parsed call, and the
/// ledger gains the live members + the `hh.model_io/1` blob.
#[test]
fn live_lane_streaming_call_lands_members_and_model_io() {
    let root = test_dir("live");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_submit_stream("qwen3-test", "goal complete"));
    let doc = document_json(Some(lane_params(&provider.endpoint, "qwen3-test")));
    let (sess, run) = open_live(&mut svc, doc, "k-live");
    ok(&submit(
        &mut svc,
        &sess,
        text_input("do the thing"),
        "k-sub",
    ));

    // The provider saw the dialect-rendered request.
    let req = provider.request_json();
    assert_eq!(req.get("model").and_then(Json::as_str), Some("qwen3-test"));
    assert_eq!(req.get("stream"), Some(&Json::Bool(true)));
    let tools = req.get("tools").and_then(|t| match t {
        Json::Arr(v) => Some(v.clone()),
        _ => None,
    });
    assert!(
        tools.as_deref().unwrap_or(&[]).iter().any(|t| t
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Json::as_str)
            == Some("hh_submit")),
        "tools[] carries the armed surfaces: {}",
        &req.to_canonical_string()
    );
    // The transcript's first message is the submitted instruction.
    assert!(
        req.to_canonical_string().contains("do the thing"),
        "user message carries the instruction: {}",
        &req.to_canonical_string()
    );

    let evs = classes(&svc, &run);
    // `model.call.requested` carries the live members.
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("requested row");
    assert!(requested.2.get("plan_hash").is_some(), "{requested:?}");
    assert_eq!(
        requested.2.get("endpoint_ref").and_then(Json::as_str),
        Some(provider.endpoint.as_str())
    );
    assert_eq!(
        requested
            .2
            .get("model_ref")
            .and_then(|m| m.get("provider_model_id"))
            .and_then(Json::as_str),
        Some("qwen3-test")
    );
    assert!(requested.2.get("token_estimate").is_some());
    assert_eq!(
        requested
            .2
            .get("credential_binding_id")
            .and_then(Json::as_str),
        Some("credentials:none")
    );

    // `model.call.completed` — parsed call, usage, timing, served model.
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("completed row");
    assert_eq!(
        completed.2.get("stop_reason").and_then(Json::as_str),
        Some("tool_use")
    );
    let call0 = completed
        .2
        .get("calls")
        .and_then(|c| match c {
            Json::Arr(v) => v.first().cloned(),
            _ => None,
        })
        .expect("parsed call");
    assert_eq!(
        call0.get("surface").and_then(Json::as_str),
        Some("hh.submit")
    );
    assert_eq!(
        call0.get("args_raw").and_then(Json::as_str),
        Some(
            Json::obj([("text", Json::str("goal complete"))])
                .to_canonical_string()
                .as_str()
        )
    );
    let usage = completed.2.get("usage").expect("usage member");
    assert_eq!(usage.get("available"), Some(&Json::Bool(true)));
    assert_eq!(
        usage
            .get("record")
            .and_then(|r| r.get("normalizer_ref"))
            .and_then(Json::as_str),
        Some("openai-compatible@1")
    );
    assert!(completed.2.get("timing").is_some());
    assert_eq!(
        completed.2.get("served_model").and_then(Json::as_str),
        Some("qwen3-test")
    );

    // The `response_ref` resolves — the `hh.model_io/1` blob deposited
    // before the terminal row (durable-before-visible).
    let resp_ref = completed
        .2
        .get("response_ref")
        .and_then(Json::as_str)
        .unwrap();
    let parsed = hharness_identity::idp::parse_id(resp_ref).expect("idp id");
    let blob = std::fs::read(svc.store().root().join("blobs").join(&parsed.digest_hex))
        .expect("model_io blob resolves");
    let io: Json = hharness_wire::json::parse(&String::from_utf8_lossy(&blob)).unwrap();
    assert_eq!(
        io.get("schema").and_then(Json::as_str),
        Some("hh.model_io/1")
    );
    assert!(io.get("request_body").is_some());
    let frame_kinds: Vec<&str> = match io.get("frames") {
        Some(Json::Arr(v)) => v
            .iter()
            .filter_map(|f| f.get("kind").and_then(Json::as_str))
            .collect(),
        _ => vec![],
    };
    assert!(frame_kinds.contains(&"stream_start"), "{frame_kinds:?}");
    assert!(frame_kinds.contains(&"stream_end"), "{frame_kinds:?}");

    // The tool call ran — `hh.submit` dispatched and the run finished.
    assert!(
        evs.iter().any(|(_, c, _)| c == "lifecycle.run.finished"),
        "run finished"
    );

    // The manifest recorded the lane (a durable resume re-binds it).
    let manifest = svc.store().manifest(&run).unwrap();
    let lane = manifest.extra.get("model_lane").expect("model_lane member");
    assert_eq!(
        lane.get("endpoint").and_then(Json::as_str),
        Some(provider.endpoint.as_str())
    );
    assert_eq!(
        lane.get("dialect").and_then(Json::as_str),
        Some("openai-compatible@1")
    );
}

/// A provider 429 surfaces `model.call.failed` with the dialect's class
/// and the header's `retry_after_ms` — typed, never a panic.
#[test]
fn live_lane_http_error_classifies_into_failed_row() {
    let root = test_dir("err");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let body = "{\"error\":{\"message\":\"rate limited\",\"type\":\"rate_limit\"}}";
    let provider = fake_provider(format!(
        "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\nretry-after: 1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    let doc = document_json(Some(lane_params(&provider.endpoint, "m")));
    let (sess, run) = open_live(&mut svc, doc, "k-err");
    let _ = submit(&mut svc, &sess, text_input("go"), "k-sub2");
    let evs = classes(&svc, &run);
    let failed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.failed")
        .expect("failed row");
    assert_eq!(
        failed
            .2
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("rate_limited"),
        "{failed:?}"
    );
    assert_eq!(
        failed.2.get("retry_after_ms").and_then(Json::as_int),
        Some(1_000)
    );
    // The audit tail recorded — `model_io` carried the error verbatim.
    assert!(failed.2.get("usage").is_some());
}

/// A served model ≠ the bound `model` is `served_model_mismatch` —
/// `substitution_allowed: false` is the default.
#[test]
fn live_lane_served_model_mismatch_is_typed() {
    let root = test_dir("mismatch");
    let mut svc = service_in(&root);
    hello(&mut svc);
    // The provider serves a *different* model id than bound.
    let provider = fake_provider(sse_submit_stream("other-model", "x"));
    let doc = document_json(Some(lane_params(&provider.endpoint, "bound-model")));
    let (sess, run) = open_live(&mut svc, doc, "k-mm");
    let _ = submit(&mut svc, &sess, text_input("go"), "k-sub3");
    let evs = classes(&svc, &run);
    let failed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.failed")
        .expect("failed row");
    assert_eq!(
        failed
            .2
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("served_model_mismatch"),
        "{failed:?}"
    );
}

/// `https://` arms at open now (R4.5c — `tls_pending` retired); a dead
/// endpoint answers the typed `model.call.failed{network}` row at the
/// call — the refusal moved from arm to wire, never silently plain.
#[test]
fn https_lane_arms_dead_endpoint_fails_typed() {
    let root = test_dir("https");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let doc = document_json(Some(lane_params("https://127.0.0.1:9", "m")));
    let (sess, run) = open_live(&mut svc, doc, "k-https");
    let _ = submit(&mut svc, &sess, text_input("go"), "k-sub-https");
    let evs = classes(&svc, &run);
    let failed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.failed")
        .expect("failed row");
    assert_eq!(
        failed
            .2
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("network"),
        "{failed:?}"
    );
}

/// `tls{extra_roots_file}` — the lane's explicit local-CA hook carries a
/// real HTTPS submit to `model.call.completed`; the served model is the
/// bound one.
#[test]
fn tls_lane_extra_roots_completes() {
    let root = test_dir("tls-ca");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider_tls(sse_submit_stream("fixture-model", "ok"));
    let mut params = lane_params(&provider.endpoint, "fixture-model");
    params.insert(
        "tls".into(),
        Json::Obj(BTreeMap::from([(
            "extra_roots_file".to_string(),
            Json::str(pki("hh-test-ca.pem").display().to_string()),
        )])),
    );
    let doc = document_json(Some(params));
    let (sess, run) = open_live(&mut svc, doc, "k-tls");
    let _ = submit(&mut svc, &sess, text_input("go"), "k-sub-tls");
    let evs = classes(&svc, &run);
    assert!(
        evs.iter().any(|(_, c, _)| c == "model.call.completed"),
        "no completed row: {evs:?}"
    );
    // The captured request crossed the TLS session — the provider saw
    // the dialect's POST bytes, never plaintext-offered TLS.
    let got = provider.got.lock().unwrap().clone().unwrap_or_default();
    let head = String::from_utf8_lossy(&got);
    assert!(head.starts_with("POST /chat/completions"), "{head}");
}

/// No `extra_roots_file` ⇒ the private test CA is untrusted — the
/// refusal is the typed `network` row; the lane never downgraded.
#[test]
fn tls_lane_without_roots_refuses() {
    let root = test_dir("tls-no-ca");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider_tls(sse_submit_stream("fixture-model", "ok"));
    let doc = document_json(Some(lane_params(&provider.endpoint, "fixture-model")));
    let (sess, run) = open_live(&mut svc, doc, "k-tls-nc");
    let _ = submit(&mut svc, &sess, text_input("go"), "k-sub-tls-nc");
    let evs = classes(&svc, &run);
    let failed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.failed")
        .expect("failed row");
    assert_eq!(
        failed
            .2
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("network"),
        "{failed:?}"
    );
}

/// `tls{}` member discipline at arm: an unpaired `client_cert_file` and
/// an unknown key are `InvalidDefinition`; a missing extra-roots file is
/// the typed `Refused` at drive (the arm-time file read happens at
/// `GatewayModel::new` — never a silently smaller root set).
#[test]
fn tls_lane_member_refusals() {
    let root = test_dir("tls-ref");
    let mut svc = service_in(&root);
    hello(&mut svc);
    // Unpaired cert/key.
    let mut params = lane_params("https://127.0.0.1:9", "m");
    params.insert(
        "tls".into(),
        Json::Obj(BTreeMap::from([(
            "client_cert_file".to_string(),
            Json::str("/nope/cert.pem"),
        )])),
    );
    let r = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(document_json(Some(params)))),
            ("idempotency_key", Json::str("k-tls-pair")),
        ]),
    );
    assert_eq!(err_kind(&r), "InvalidDefinition");
    // Unknown key.
    let mut params = lane_params("https://127.0.0.1:9", "m");
    params.insert(
        "tls".into(),
        Json::Obj(BTreeMap::from([("bogus".to_string(), Json::str("x"))])),
    );
    let r = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(document_json(Some(params)))),
            ("idempotency_key", Json::str("k-tls-bogus")),
        ]),
    );
    assert_eq!(err_kind(&r), "InvalidDefinition");
    // Missing extra-roots file — the open decodes; the drive refuses
    // typed (`Refused`), never a session on the default roots.
    let mut params = lane_params("https://127.0.0.1:9", "m");
    params.insert(
        "tls".into(),
        Json::Obj(BTreeMap::from([(
            "extra_roots_file".to_string(),
            Json::str("/definitely/not/here.pem"),
        )])),
    );
    let (sess, _run) = open_live(&mut svc, document_json(Some(params)), "k-tls-miss");
    let r = submit(&mut svc, &sess, text_input("go"), "k-sub-tls-miss");
    assert_eq!(err_kind(&r), "Refused", "{r:?}");
}

/// The `env:` indirection is the sanctioned secret-name channel — an
/// unset name refuses `EnvironmentUnavailable` at open (the *name*, never
/// the value, is the record).
#[test]
fn env_indirection_unset_refuses_at_open() {
    let root = test_dir("env");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let mut params = lane_params(
        "env:HH_R45B_DEFINITELY_UNSET",
        "env:HH_R45B_DEFINITELY_UNSET_M",
    );
    params.insert("endpoint".into(), Json::str("env:HH_R45B_DEFINITELY_UNSET"));
    let doc = document_json(Some(params));
    let r = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc)),
            ("idempotency_key", Json::str("k-env")),
        ]),
    );
    assert_eq!(
        err_kind(&r),
        "EnvironmentUnavailable",
        "{}",
        r.to_canonical_string()
    );
}

/// The env indirection resolves a *set* name — the provisioned local
/// lane's shape (`HH_LOCAL_BASE_URL`/`HH_LOCAL_MODEL`).
#[test]
fn env_indirection_resolves_set_names() {
    let root = test_dir("env2");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_submit_stream("env-model", "ok"));
    std::env::set_var("HH_R45B_TEST_ENDPOINT", &provider.endpoint);
    std::env::set_var("HH_R45B_TEST_MODEL", "env-model");
    let doc = document_json(Some(lane_params(
        "env:HH_R45B_TEST_ENDPOINT",
        "env:HH_R45B_TEST_MODEL",
    )));
    let (sess, run) = open_live(&mut svc, doc, "k-env2");
    ok(&submit(&mut svc, &sess, text_input("go"), "k-sub4"));
    let evs = classes(&svc, &run);
    assert!(
        evs.iter().any(|(_, c, _)| c == "model.call.completed"),
        "completed"
    );
    // The *name* resolved; the manifest carries the resolved coordinate,
    // never the env spelling's indirection.
    let manifest = svc.store().manifest(&run).unwrap();
    let lane = manifest.extra.get("model_lane").unwrap();
    assert_eq!(
        lane.get("provider_model_id").and_then(Json::as_str),
        Some("env-model")
    );
    std::env::remove_var("HH_R45B_TEST_ENDPOINT");
    std::env::remove_var("HH_R45B_TEST_MODEL");
}

/// AC1 — no `model_port` slot ⇒ the scripted lane is untouched: no
/// `plan_hash`/`endpoint_ref`/`usage`/`served_model` members, no
/// `model_lane` manifest member.
#[test]
fn unbound_lane_is_byte_identical() {
    let root = test_dir("unbound");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let doc = document_json(None);
    let (sess, run) = open_live(&mut svc, doc, "k-legacy");
    ok(&submit(&mut svc, &sess, text_input("hi"), "k-sub5"));
    let evs = classes(&svc, &run);

    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("requested row");
    assert!(requested.2.get("plan_hash").is_none(), "{requested:?}");
    assert!(requested.2.get("endpoint_ref").is_none());
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("completed row");
    assert!(completed.2.get("usage").is_none());
    assert!(completed.2.get("served_model").is_none());
    assert!(completed.2.get("timing").is_none());
    let manifest = svc.store().manifest(&run).unwrap();
    assert!(!manifest.extra.contains_key("model_lane"));

    // Probe — does deterministic replay reproduce the scripted record?
    let rep = ok(&call(
        &mut svc,
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    ));
    eprintln!("SCRIPTED REPLAY: {}", rep.to_canonical_string());
    let evs = svc.store().envelopes(&run).unwrap();
    for e in evs.iter().take(30) {
        eprintln!("PROD {} {} {:?}", e.seq, e.class, e.producer);
    }
}

/// A non-streaming completion (`content-type: application/json`) folds
/// end_turn — the model answered text, the call completed, the parsed
/// calls are empty.
#[test]
fn live_lane_non_streaming_end_turn() {
    let root = test_dir("nonstream");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let body = "{\"id\":\"c-9\",\"object\":\"chat.completion\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"hello\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}";
    let provider = fake_provider(format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    let doc = document_json(Some(lane_params(&provider.endpoint, "m")));
    let (sess, run) = open_live(&mut svc, doc, "k-ns");
    let _ = submit(&mut svc, &sess, text_input("say hi"), "k-sub6");
    let evs = classes(&svc, &run);
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("completed row");
    assert_eq!(
        completed.2.get("stop_reason").and_then(Json::as_str),
        Some("end_turn")
    );
}

/// A deterministic replay over the *live-shaped* record reproduces the
/// run — the recorded `model.call.*` rows feed `ReplayModel`; no second
/// provider call (the fake server is one-shot; a second dial would hang).
#[test]
fn live_lane_replay_reproduces_record() {
    let root = test_dir("replay");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_submit_stream("qwen3-test", "done"));
    let doc = document_json(Some(lane_params(&provider.endpoint, "qwen3-test")));
    let (sess, run) = open_live(&mut svc, doc, "k-rep");
    let _ = &run;
    ok(&submit(&mut svc, &sess, text_input("go"), "k-subr"));
    let v = ok(&call(
        &mut svc,
        "verify",
        Json::obj([("session_id", Json::str(sess.clone()))]),
    ));
    assert_eq!(v.get("verdict").and_then(Json::as_str), Some("ok"));
    let rep = ok(&call(
        &mut svc,
        "replay",
        Json::obj([
            ("session_id", Json::str(sess)),
            ("driver_mode", Json::str("deterministic")),
        ]),
    ));
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "replay reproduces byte-identically: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "replay carries no divergence reasons: {}",
        rep.to_canonical_string()
    );
}
