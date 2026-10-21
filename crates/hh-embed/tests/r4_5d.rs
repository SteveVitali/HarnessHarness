//! R4.5d — the hosted `model_port` families through `GatewayModel`
//! (ADR-0366): `anthropic-messages@1` and `openai@1` bound on the same
//! lane machinery R4.5b cut — `credential_ref` as an `env:NAME`
//! *coordinate* (the value resolves inside the transport at send; the
//! rows, blobs, and manifest carry `binding_id`/`provided` only).
//!
//! Hermetic: a loopback `TcpListener` fixture answers the dialect's
//! POST — no live service is touched here (the `#[ignore]`d live legs
//! ride `scripts/live-verify.sh`).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── the conformance document (the r4_5b shape) ────────────────────────────

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
                dimensions: BTreeMap::from([
                    (
                        "tokens.blended".to_string(),
                        DimensionBound {
                            hard: Some(500_000),
                            soft: None,
                        },
                    ),
                    (
                        "model_calls".to_string(),
                        DimensionBound {
                            hard: Some(6),
                            soft: None,
                        },
                    ),
                ]),
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

/// A hosted-lane slot params — `dialect`, `endpoint`, `provider_model_id`,
/// plus the hosted family's own members.
fn lane_params(
    dialect: &str,
    endpoint: &str,
    model: &str,
    provider_params: BTreeMap<String, Json>,
    credential_ref: Option<&str>,
) -> BTreeMap<String, Json> {
    let mut p = BTreeMap::from([
        ("dialect".to_string(), Json::str(dialect)),
        ("endpoint".to_string(), Json::str(endpoint)),
        ("provider_model_id".to_string(), Json::str(model)),
    ]);
    if !provider_params.is_empty() {
        p.insert("provider_params".into(), Json::Obj(provider_params));
    }
    if let Some(cr) = credential_ref {
        p.insert("credential_ref".into(), Json::str(cr));
    }
    p
}

// ── service helpers (the r4_5b shape) ─────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r45d-{}-{tag}-{}",
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
        holder: "r4.5d".into(),
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
                    ("name", Json::str("r4.5d")),
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

// ── the loopback fake provider (head + body captured; r4_5b's shape) ──────

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
                if head_complete(&buf) || std::time::Instant::now() > deadline {
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
    /// The full request head the provider received (post-drive).
    fn request_head(&self) -> String {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                return String::from_utf8_lossy(&buf[..p]).to_string();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }

    /// The request body the provider received (post-drive).
    fn request_json(&self) -> Json {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                return hh_wire::json::parse(&String::from_utf8_lossy(&buf[p + 4..])).unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }

    /// True when the provider was never dialed (a refused call's check).
    fn never_dialed(&self) -> bool {
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.got.lock().unwrap().is_none()
    }
}

fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A Messages SSE stream answering `end_turn` text — usage carries the
/// cache members (`cache_read_input_tokens`) so the exclusive-cache
/// mapping is exercised, not just the flat path.
fn anthropic_sse_text(model: &str, text: &str) -> String {
    let events = [
        format!(
            "{{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"{model}\",\"usage\":{{\"input_tokens\":17,\"cache_read_input_tokens\":4,\"cache_creation_input_tokens\":0,\"output_tokens\":1}}}}}}"
        ),
        "{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}".to_string(),
        format!(
            "{{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}"
        ),
        "{\"type\":\"content_block_stop\",\"index\":0}".to_string(),
        "{\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":9}}".to_string(),
        "{\"type\":\"message_stop\"}".to_string(),
    ];
    let mut s = String::new();
    for e in events {
        s.push_str("data: ");
        s.push_str(&e);
        s.push_str("\n\n");
    }
    s
}

/// A Messages SSE stream answering one `hh_submit` `tool_use` block.
fn anthropic_sse_submit(model: &str, text: &str) -> String {
    let args = Json::obj([("text", Json::str(text))]).to_canonical_string();
    let (a1, a2) = args.split_at(args.len() / 2);
    let events = [
        format!(
            "{{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"{model}\",\"usage\":{{\"input_tokens\":23,\"output_tokens\":1}}}}}}"
        ),
        "{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"hh_submit\",\"input\":{}}}".to_string(),
        format!(
            "{{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}}}",
            a1.replace('"', "\\\"")
        ),
        format!(
            "{{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}}}",
            a2.replace('"', "\\\"")
        ),
        "{\"type\":\"content_block_stop\",\"index\":0}".to_string(),
        "{\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":15}}".to_string(),
        "{\"type\":\"message_stop\"}".to_string(),
    ];
    let mut s = String::new();
    for e in events {
        s.push_str("data: ");
        s.push_str(&e);
        s.push_str("\n\n");
    }
    s
}

/// A chat-completions SSE stream answering `stop` text (the hosted
/// `openai@1` family's own chunk shape — usage rides the trailing chunk).
fn openai_sse_text(model: &str, text: &str) -> String {
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
    let body = format!(
        "{}{}{}data: [DONE]\n\n",
        chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                ("content", Json::str(text))
            ]),
            Json::Null,
            Json::Null,
        ),
        chunk(
            Json::obj([]),
            Json::str("stop"),
            Json::obj([
                ("prompt_tokens", Json::Int(11)),
                ("completion_tokens", Json::Int(4)),
                ("total_tokens", Json::Int(15)),
            ]),
        ),
        ""
    );
    body
}

// ── the cells ────────────────────────────────────────────────────────────

/// The Messages lane end to end: `anthropic-messages@1` armed on
/// `model_port`, `credential_ref` as an `env:` coordinate, the request
/// wire-checked (`/v1/messages`, `x-api-key`, `anthropic-version`, the
/// family's request members), the rows carrying `binding_id` + usage —
/// and the planted secret present nowhere but the wire's header.
#[test]
fn anthropic_lane_streaming_call_lands_members() {
    let secret = "sk-ant-planted-not-a-key";
    std::env::set_var("HH_R45D_ANT", secret);
    let root = test_dir("ant");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_response(&anthropic_sse_text("claude-haiku-4-5", "hi")));
    let doc = document_json(Some(lane_params(
        "anthropic-messages@1",
        &format!("{}/v1", provider.endpoint),
        "claude-haiku-4-5",
        BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
        Some("env:HH_R45D_ANT"),
    )));
    let (sess, run) = open_live(&mut svc, doc, "k-ant");
    ok(&submit(&mut svc, &sess, text_input("say hi"), "k-sub"));

    // The wire: the family's op path + auth headers + request members.
    let head = provider.request_head();
    assert!(
        head.starts_with("POST /v1/messages "),
        "Messages op path: {}",
        head.lines().next().unwrap_or("")
    );
    assert!(
        head.to_lowercase()
            .contains("x-api-key: sk-ant-planted-not-a-key"),
        "x-api-key injected at the transport: {head}"
    );
    assert!(
        head.contains("anthropic-version: 2023-06-01"),
        "anthropic-version rides every call: {head}"
    );
    let req = provider.request_json();
    assert_eq!(
        req.get("model").and_then(Json::as_str),
        Some("claude-haiku-4-5")
    );
    assert_eq!(req.get("stream"), Some(&Json::Bool(true)));
    assert_eq!(req.get("max_tokens"), Some(&Json::Int(8)));
    // The family's own request members — `messages[].content[]` blocks,
    // `tools[]{name,description,input_schema}`, `tool_choice{type}`.
    assert_eq!(
        req.get("tool_choice")
            .and_then(|t| t.get("type"))
            .and_then(Json::as_str),
        Some("auto"),
        "tool_choice is the family's object spelling: {}",
        req.to_canonical_string()
    );
    let tools = req.get("tools").expect("tools[]");
    let first = match tools {
        Json::Arr(v) => v.first().cloned().unwrap_or(Json::Null),
        _ => panic!("tools is an array"),
    };
    assert_eq!(first.get("name").and_then(Json::as_str), Some("hh_submit"));
    assert!(first.get("input_schema").is_some());
    assert!(
        first.get("type").is_none(),
        "no function wrapper: {}",
        first.to_canonical_string()
    );
    assert_eq!(
        req.get("messages").and_then(|m| match m {
            Json::Arr(v) => v.first().and_then(|mm| {
                mm.get("content").and_then(|c| match c {
                    Json::Arr(b) => b.first().and_then(|b| b.get("type").and_then(Json::as_str)),
                    _ => None,
                })
            }),
            _ => None,
        }),
        Some("text"),
        "user content rides text blocks: {}",
        req.to_canonical_string()
    );

    // The rows: requested carries the *coordinate* binding id; completed
    // carries usage (cache members normalized), served model, timing.
    let evs = classes(&svc, &run);
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("requested row");
    assert_eq!(
        requested
            .2
            .get("credential_binding_id")
            .and_then(Json::as_str),
        Some("launch_env:HH_R45D_ANT"),
        "the binding id is the coordinate: {requested:?}"
    );
    assert_eq!(
        requested
            .2
            .get("model_ref")
            .and_then(|m| m.get("provider_model_id"))
            .and_then(Json::as_str),
        Some("claude-haiku-4-5")
    );
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("completed row");
    assert_eq!(
        completed.2.get("stop_reason").and_then(Json::as_str),
        Some("end_turn")
    );
    let usage = completed.2.get("usage").expect("usage member");
    assert_eq!(usage.get("available"), Some(&Json::Bool(true)));
    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .expect("usage.record.view");
    assert_eq!(
        view.get("input_total").and_then(Json::as_int),
        Some(21),
        "17 uncached + 4 cache-read (Anthropic exclusive spelling): {}",
        view.to_canonical_string()
    );
    assert_eq!(view.get("cache_read").and_then(Json::as_int), Some(4));
    assert_eq!(view.get("output_total").and_then(Json::as_int), Some(9));
    assert_eq!(
        view.get("normalizer_ref").and_then(Json::as_str),
        Some("anthropic-messages@1")
    );
    // The raw usage is the provider's own document, verbatim — it rides
    // `outcome.usage_raw` on the model_io blob (the row's OPEN_AUDIT
    // bound keeps bulky provider documents off the inline audit field).
    let resp_ref = completed
        .2
        .get("response_ref")
        .and_then(Json::as_str)
        .expect("response_ref");
    let parsed = hh_identity::idp::parse_id(resp_ref).expect("idp id");
    let blob = std::fs::read(svc.store().root().join("blobs").join(&parsed.digest_hex))
        .expect("model_io blob resolves");
    let io: Json = hh_wire::json::parse(&String::from_utf8_lossy(&blob)).unwrap();
    assert_eq!(
        io.get("schema").and_then(Json::as_str),
        Some("hh.model_io/1")
    );
    let raw = io
        .get("outcome")
        .and_then(|o| o.get("usage_raw"))
        .expect("outcome.usage_raw");
    assert_eq!(raw.get("input_tokens").and_then(Json::as_int), Some(17));
    assert_eq!(
        completed.2.get("served_model").and_then(Json::as_str),
        Some("claude-haiku-4-5")
    );
    assert!(completed.2.get("timing").is_some());

    // The manifest carries the *coordinate*, never the value.
    let manifest = svc.store().manifest(&run).unwrap();
    let lane = manifest.extra.get("model_lane").expect("model_lane");
    assert_eq!(
        lane.get("credential_ref").and_then(Json::as_str),
        Some("env:HH_R45D_ANT")
    );

    // The leak sweep — the planted value is in no payload and no blob.
    let all_payloads: String = evs
        .iter()
        .map(|(_, _, p)| p.to_canonical_string())
        .collect();
    assert!(
        !all_payloads.contains(secret),
        "no ledger payload carries the credential value"
    );
    let blobs = svc.store().root().join("blobs");
    if blobs.exists() {
        for e in std::fs::read_dir(&blobs).unwrap() {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&b).contains(secret),
                "no blob carries the credential value"
            );
        }
    }
    std::env::remove_var("HH_R45D_ANT");
}

/// The hosted OpenAI lane end to end — `openai@1` (Chat Completions,
/// chosen over Responses because the gateway already owns the family's
/// normalized codec; ADR-0366 D2 records the selection): `Authorization:
/// Bearer` injection, `stream_options.include_usage` requested, and the
/// `binding_id` coordinate on the requested row.
#[test]
fn openai_lane_streaming_call_lands_members() {
    let secret = "sk-oai-planted-not-a-key";
    std::env::set_var("HH_R45D_OAI", secret);
    let root = test_dir("oai");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_response(&openai_sse_text("gpt-4.1-mini", "hi")));
    let doc = document_json(Some(lane_params(
        "openai@1",
        &format!("{}/v1", provider.endpoint),
        "gpt-4.1-mini",
        BTreeMap::from([("max_completion_tokens".to_string(), Json::Int(16))]),
        Some("env:HH_R45D_OAI"),
    )));
    let (sess, run) = open_live(&mut svc, doc, "k-oai");
    ok(&submit(&mut svc, &sess, text_input("say hi"), "k-sub"));

    let head = provider.request_head();
    assert!(
        head.starts_with("POST /v1/chat/completions "),
        "chat.completions op path: {}",
        head.lines().next().unwrap_or("")
    );
    assert!(
        head.to_lowercase()
            .contains("authorization: bearer sk-oai-planted-not-a-key"),
        "Bearer injected: {head}"
    );
    let req = provider.request_json();
    assert_eq!(
        req.get("model").and_then(Json::as_str),
        Some("gpt-4.1-mini")
    );
    assert_eq!(req.get("stream"), Some(&Json::Bool(true)));
    assert_eq!(
        req.get("stream_options")
            .and_then(|s| s.get("include_usage")),
        Some(&Json::Bool(true)),
        "stream_options.include_usage requested (the trailing chunk the grammar reads)"
    );
    assert_eq!(req.get("max_completion_tokens"), Some(&Json::Int(16)));

    let evs = classes(&svc, &run);
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("requested row");
    assert_eq!(
        requested
            .2
            .get("credential_binding_id")
            .and_then(Json::as_str),
        Some("launch_env:HH_R45D_OAI")
    );
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("completed row");
    assert_eq!(
        completed.2.get("stop_reason").and_then(Json::as_str),
        Some("end_turn")
    );
    let usage = completed.2.get("usage").expect("usage member");
    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .expect("view");
    assert_eq!(view.get("input_total").and_then(Json::as_int), Some(11));
    assert_eq!(view.get("output_total").and_then(Json::as_int), Some(4));

    let all_payloads: String = evs
        .iter()
        .map(|(_, _, p)| p.to_canonical_string())
        .collect();
    assert!(!all_payloads.contains(secret));
    std::env::remove_var("HH_R45D_OAI");
}

/// A Messages `tool_use` answer parses to the kernel call — `stop_reason:
/// tool_use`, `calls[]` carries `surface: hh.submit` + the assembled args
/// (`input_json_delta` accumulation across chunks).
#[test]
fn anthropic_lane_tool_use_lands_parsed_call() {
    let secret = "sk-ant-planted-not-a-key";
    std::env::set_var("HH_R45D_ANT", secret);
    let root = test_dir("anttool");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_response(&anthropic_sse_submit(
        "claude-haiku-4-5",
        "goal complete",
    )));
    let doc = document_json(Some(lane_params(
        "anthropic-messages@1",
        &format!("{}/v1", provider.endpoint),
        "claude-haiku-4-5",
        BTreeMap::from([("max_tokens".to_string(), Json::Int(64))]),
        Some("env:HH_R45D_ANT"),
    )));
    let (sess, run) = open_live(&mut svc, doc, "k-ant2");
    ok(&submit(&mut svc, &sess, text_input("finish"), "k-sub"));

    let evs = classes(&svc, &run);
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
        .expect("a parsed call");
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
    assert!(
        evs.iter().any(|(_, c, _)| c == "lifecycle.run.finished"),
        "the tool ran — the run finished"
    );
    let all_payloads: String = evs
        .iter()
        .map(|(_, _, p)| p.to_canonical_string())
        .collect();
    assert!(!all_payloads.contains(secret));
    std::env::remove_var("HH_R45D_ANT");
}

/// `credential_ref` refuses typed at arm: an unset name is
/// `EnvironmentUnavailable`; a non-`env:` coordinate is
/// `InvalidDefinition`. Neither case opens a session.
#[test]
fn credential_ref_coordinate_refusals_at_arm() {
    let root = test_dir("credrefuse");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let doc = document_json(Some(lane_params(
        "anthropic-messages@1",
        "http://127.0.0.1:1",
        "m",
        BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
        Some("env:HH_R45D_DEFINITELY_UNSET"),
    )));
    let r = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc)),
            ("idempotency_key", Json::str("k-cr1")),
        ]),
    );
    assert_eq!(
        err_kind(&r),
        "EnvironmentUnavailable",
        "{}",
        r.to_canonical_string()
    );

    let doc2 = document_json(Some(lane_params(
        "openai@1",
        "http://127.0.0.1:1",
        "m",
        BTreeMap::new(),
        Some("vault:item"),
    )));
    let r2 = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc2)),
            ("idempotency_key", Json::str("k-cr2")),
        ]),
    );
    assert_eq!(
        err_kind(&r2),
        "InvalidDefinition",
        "{}",
        r2.to_canonical_string()
    );
}

/// A hosted dialect bound *without* `credential_ref` fails the call
/// closed — `model.call.failed{error.class: auth}` lands, the plan's own
/// `plan_error` marks the requested row, and the provider is never
/// dialed (no bare request rides an auth-requiring dialect).
#[test]
fn hosted_lane_without_credential_fails_closed() {
    let root = test_dir("nocreds");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let provider = fake_provider(sse_response(&anthropic_sse_text("m", "unreached")));
    let doc = document_json(Some(lane_params(
        "anthropic-messages@1",
        &provider.endpoint,
        "m",
        BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
        None,
    )));
    let (sess, run) = open_live(&mut svc, doc, "k-nocreds");
    ok(&submit(&mut svc, &sess, text_input("hi"), "k-sub"));

    let evs = classes(&svc, &run);
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("requested row");
    assert_eq!(
        requested.2.get("plan_error").and_then(Json::as_str),
        Some("auth"),
        "the refusal class rides the requested row: {requested:?}"
    );
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
        Some("auth"),
        "model.call.failed class=auth: {failed:?}"
    );
    assert!(
        provider.never_dialed(),
        "an auth-refused call never reaches the wire"
    );
}
