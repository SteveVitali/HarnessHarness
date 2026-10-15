//! R2.19 — the embed/MCP surface legs (spec §7.3; DF-S4.11-1,
//! DF-S4.11-2):
//!
//! - **`GET /mcp` SSE** — the stream opens `text/event-stream` under a
//!   live `Mcp-Session-Id`, pushes a notification minted *after* the
//!   subscription as `id: <seq>` + `data: <json-rpc-notification>`,
//!   and `Last-Event-ID` resumes the retained backlog without loss
//!   (the one `push_notification` mint serves both the SSE arm and the
//!   `subscriptions/listen` drain — the same frame, never a second
//!   spelling);
//! - **the WS-H3 credential mediator** — `oauth` caller credentials
//!   verify through the kernel broker's `minted_scoped` leg under the
//!   binding's declared `audience`: a token the kernel never minted is
//!   `credential_invalid`, a wrong-audience mint likewise, an expired
//!   mint `credential_expired`, an unknown spelling `no_grant`;
//!   `mtls` typed-refuses `transport_absent` at this slice (BL-31 —
//!   no TLS handshake exists, never a fabricated subject). The `401`
//!   body carries the refusal code — observable, never a guess.
//!
//! Each test fails if the behaviour is removed.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_mcp_lab::binding::{
    stdio_launch_binding, CallerBinding, CallerCredential, CallerKind, CredentialRefusal,
    SubjectKind,
};
use hh_mcp_lab::server::LabServer;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── store/service fixtures (same shape as the s5_8 battery) ────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-r219-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r2-19-lab".into(),
    })
    .unwrap()
}

fn lab() -> LabServer {
    LabServer::with_default_exposure(service())
}

fn agent() -> CallerBinding {
    stdio_launch_binding("test-agent", CallerKind::Agent)
}

/// An `oauth{audience}` binding — the mediator's positive leg (the
/// grant names the credential kind; the kernel's minted token is the
/// evidence it must verify).
fn oauth_binding(binding_id: &str, audience: &str) -> CallerBinding {
    let mut b = agent();
    b.binding_id = binding_id.to_string();
    b.credential = CallerCredential::OAuth {
        issuer_ref: "issuer:test".to_string(),
        subject_kind: SubjectKind::User,
        audience: audience.to_string(),
    };
    b
}

// ── the hir/1 fixture document (verbatim the s5_7/s5_8 shape) ─────

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

fn document_json() -> Json {
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
                    "tool_calls".to_string(),
                    DimensionBound {
                        hard: Some(1_000),
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
                    slots: BTreeMap::new(),
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

fn launch_args(idem: &str) -> Json {
    Json::obj([
        (
            "definition",
            Json::obj([
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
        ),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "budget",
            Json::obj([(
                "dimensions",
                Json::obj([("tool_calls", Json::obj([("hard", Json::Int(10))]))]),
            )]),
        ),
        ("idempotency_key", Json::str(idem)),
    ])
}

fn msg(id: i64, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(id)),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

fn call_frame(name: &str, arguments: Json) -> Json {
    msg(
        7,
        "tools/call",
        Json::obj([("name", Json::str(name)), ("arguments", arguments)]),
    )
}

// ── The credential mediator (DF-S4.11-2) ────────────────────────────

#[test]
fn credential_mediator_oauth_resolves_and_refuses() {
    let mut srv = lab();
    // The kernel's own `minted_scoped` issue leg (the deployment's
    // authorization-server arm — durable granted/decided/bound/used
    // rows land before the token answers).
    let tok = srv
        .mint_caller_token("mcp://lab", 60_000)
        .expect("the kernel mints a caller token");
    srv.grant_bearer(&tok, oauth_binding("bind-oauth-1", "mcp://lab"));
    // A minted token verifies under the binding's declared audience —
    // the grant + the credential both check out.
    let b = srv
        .resolve_credential(&tok)
        .expect("a kernel-minted token resolves");
    assert_eq!(b.binding_id, "bind-oauth-1");
    assert_eq!(b.principal_ref, "principal:test");

    // A table grant over a token the kernel never minted — the grant
    // alone is never sufficient evidence: `credential_invalid`.
    srv.grant_bearer(
        "tok-never-minted",
        oauth_binding("bind-oauth-2", "mcp://lab"),
    );
    assert_eq!(
        srv.resolve_credential("tok-never-minted").unwrap_err(),
        CredentialRefusal::CredentialInvalid
    );

    // A token minted for a *different* audience — the MAC checks but
    // the audience claim mismatches: `credential_invalid`.
    let other = srv
        .mint_caller_token("mcp://other", 60_000)
        .expect("the kernel mints for a second audience");
    srv.grant_bearer(&other, oauth_binding("bind-oauth-3", "mcp://lab"));
    assert_eq!(
        srv.resolve_credential(&other).unwrap_err(),
        CredentialRefusal::CredentialInvalid
    );

    // An expired mint — `credential_expired` (the caller
    // re-authenticates, never a revoked-subject guess).
    let dead = srv
        .mint_caller_token("mcp://lab", 0)
        .expect("a zero-ttl mint lands");
    srv.grant_bearer(&dead, oauth_binding("bind-oauth-4", "mcp://lab"));
    assert_eq!(
        srv.resolve_credential(&dead).unwrap_err(),
        CredentialRefusal::CredentialExpired
    );

    // No grant names the token at all — `no_grant`.
    assert_eq!(
        srv.resolve_credential("tok-unknown").unwrap_err(),
        CredentialRefusal::NoGrant
    );

    // `mtls` — the declared credential kind needs the TLS handshake's
    // peer identity; no handshake ran, so the seam typed-refuses
    // `transport_absent` (BL-31 — never a fabricated subject).
    let mut m = agent();
    m.binding_id = "bind-mtls-1".to_string();
    m.credential = CallerCredential::Mtls {
        cert_ref: "cert:test".to_string(),
    };
    srv.grant_bearer("tok-mtls", m);
    assert_eq!(
        srv.resolve_credential("tok-mtls").unwrap_err(),
        CredentialRefusal::TransportAbsent
    );
}

// ── The HTTP arm — the `401` carries the typed refusal ─────────────

/// One-shot POST → the whole reply (the server closes after the
/// response — `read_to_end` terminates).
fn post(addr: &std::net::SocketAddr, headers: &[(&str, &str)], body: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!(
        "POST /mcp HTTP/1.1\r\nhost: {addr}\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    String::from_utf8_lossy(&buf).to_string()
}

/// `GET /mcp` — send the request, read the response head, return the
/// live stream plus the head/body boundary split.
fn get_stream(
    addr: &std::net::SocketAddr,
    headers: &[(&str, &str)],
) -> (TcpStream, String, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!("GET /mcp HTTP/1.1\r\nhost: {addr}\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut acc = Vec::new();
    let mut tmp = [0u8; 8192];
    let deadline = Instant::now() + Duration::from_secs(5);
    let head_end = loop {
        if let Some(p) = acc.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4) {
            break p;
        }
        assert!(Instant::now() < deadline, "the SSE head never arrived");
        match s.read(&mut tmp) {
            Ok(0) => panic!("the stream closed before the head"),
            Ok(n) => acc.extend_from_slice(&tmp[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("read: {e}"),
        }
    };
    let head = String::from_utf8_lossy(&acc[..head_end]).to_string();
    let rest = String::from_utf8_lossy(&acc[head_end..]).to_string();
    (s, head, rest)
}

/// Read `stream` into `buf` until `needle` appears or the deadline
/// passes — `true` when the needle showed.
fn read_until(stream: &mut TcpStream, buf: &mut String, needle: &str, within: Duration) -> bool {
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let deadline = Instant::now() + within;
    let mut tmp = [0u8; 8192];
    while Instant::now() < deadline {
        if buf.contains(needle) {
            return true;
        }
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.push_str(&String::from_utf8_lossy(&tmp[..n])),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
    }
    buf.contains(needle)
}

/// Spawn the serve loop on its own thread — the same `!Send` pointer
/// hand-off the s4_11 fixture uses: the serve thread is the only
/// owner of `srv` for the test's remainder; the main thread only does
/// client-side socket IO.
fn spawn_http(srv: LabServer, listener: std::net::TcpListener) {
    let origins = hh_mcp::http::OriginPolicy::new(&[], false);
    let srv_ptr = Box::into_raw(Box::new(srv)) as usize;
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    let origins_ptr = Box::into_raw(Box::new(origins)) as usize;
    std::thread::spawn(move || {
        let srv = unsafe { &mut *(srv_ptr as *mut LabServer) };
        let listener = unsafe { &*(listener_ptr as *const std::net::TcpListener) };
        let origins = unsafe { &*(origins_ptr as *const hh_mcp::http::OriginPolicy) };
        let _ = hh_mcp_lab::http::serve_http(srv, listener, origins);
    });
}

fn session_of(response: &str) -> String {
    response
        .lines()
        .find(|l| l.to_lowercase().starts_with("mcp-session-id:"))
        .map(|l| l.split(':').nth(1).unwrap().trim().to_string())
        .expect("Mcp-Session-Id issued")
}

#[test]
fn http_401_carries_the_typed_refusal() {
    let mut srv = lab();
    // An `oauth` grant over a token the kernel never minted — the
    // mediation refuses before any binding resolves; the wire carries
    // the code.
    srv.grant_bearer("tok-fake", oauth_binding("bind-oauth-x", "mcp://lab"));
    // An `mtls` grant — the handshake never ran (BL-31).
    let mut m = agent();
    m.credential = CallerCredential::Mtls {
        cert_ref: "cert:test".to_string(),
    };
    srv.grant_bearer("tok-mtls", m);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    spawn_http(srv, listener);

    let r = post(
        &addr,
        &[("authorization", "Bearer tok-fake")],
        &msg(1, "initialize", Json::obj([])).to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 401"), "{r}");
    assert!(
        r.contains("\"refusal\":\"credential_invalid\""),
        "the 401 names the mediation refusal: {r}"
    );

    let r = post(
        &addr,
        &[("authorization", "Bearer tok-mtls")],
        &msg(2, "initialize", Json::obj([])).to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 401"), "{r}");
    assert!(
        r.contains("\"refusal\":\"transport_absent\""),
        "mtls without a handshake typed-refuses: {r}"
    );
}

// ── The SSE arm (DF-S4.11-1) ────────────────────────────────────────

#[test]
fn sse_get_streams_pushed_notifications_and_resumes() {
    let mut srv = lab();
    srv.grant_bearer("tok-1", agent());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    spawn_http(srv, listener);

    let init = &msg(1, "initialize", Json::obj([])).to_canonical_string();
    // `GET` before any session — the stream is session-owned: `400`.
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(
        format!("GET /mcp HTTP/1.1\r\nhost: {addr}\r\nauthorization: Bearer tok-1\r\n\r\n")
            .as_bytes(),
    )
    .unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let r = String::from_utf8_lossy(&raw).to_string();
    assert!(r.starts_with("HTTP/1.1 400"), "GET wants a session: {r}");

    // initialize → the session id.
    let r = post(&addr, &[("authorization", "Bearer tok-1")], init);
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let sid = session_of(&r);

    // `GET` under a stale session → `404` (the same gate POST carries).
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(
        format!(
            "GET /mcp HTTP/1.1\r\nhost: {addr}\r\nauthorization: Bearer tok-1\r\nmcp-session-id: bogus\r\n\r\n"
        )
        .as_bytes(),
    )
    .unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let r = String::from_utf8_lossy(&raw).to_string();
    assert!(
        r.starts_with("HTTP/1.1 404"),
        "a stale session refuses: {r}"
    );

    // Launch a run + subscribe its ledger URI — the notification mint
    // lands only *after* the subscription exists (the AC's wording).
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-1"), ("mcp-session-id", &sid)],
        &call_frame("launch_run", launch_args("sse-1")).to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let body = r.split("\r\n\r\n").nth(1).unwrap_or("");
    let frame = hh_wire::json::parse(body).unwrap();
    let run_id = frame
        .get("result")
        .and_then(|x| x.get("structuredContent"))
        .and_then(|s| s.get("launched"))
        .and_then(|l| l.get("run_id"))
        .and_then(Json::as_str)
        .expect("launch_run answers run_id")
        .to_string();
    let ledger_uri = format!("hh://run/{run_id}/ledger");
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-1"), ("mcp-session-id", &sid)],
        &msg(
            3,
            "resources/subscribe",
            Json::obj([("uri", Json::str(ledger_uri.clone()))]),
        )
        .to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");

    // `GET /mcp` — the stream opens `text/event-stream` and stays.
    let (mut stream, head, _) = get_stream(
        &addr,
        &[("authorization", "Bearer tok-1"), ("mcp-session-id", &sid)],
    );
    assert!(
        head.starts_with("HTTP/1.1 200") && head.contains("content-type: text/event-stream"),
        "the SSE arm answers event-stream: {head}"
    );

    // A call touching the subscribed run mints the notification —
    // durable first, then the stream pushes `id:` + `data:`.
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-1"), ("mcp-session-id", &sid)],
        &call_frame(
            "submit_input",
            Json::obj([
                ("run", Json::str(run_id.clone())),
                ("content", Json::str("one")),
            ]),
        )
        .to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let mut buf = String::new();
    assert!(
        read_until(
            &mut stream,
            &mut buf,
            "notifications/resources/updated",
            Duration::from_secs(5)
        ),
        "the minted notification streams: {buf}"
    );
    assert!(buf.contains("id: 0"), "the frame carries its seq id: {buf}");
    assert!(
        buf.contains(&ledger_uri),
        "the narrowed uri member streams: {buf}"
    );

    // Drop the stream; mint a second notification; reconnect with
    // `Last-Event-ID` — the backlog replays without loss.
    drop(stream);
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-1"), ("mcp-session-id", &sid)],
        &call_frame(
            "submit_input",
            Json::obj([("run", Json::str(run_id)), ("content", Json::str("two"))]),
        )
        .to_canonical_string(),
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let (mut stream2, head2, rest2) = get_stream(
        &addr,
        &[
            ("authorization", "Bearer tok-1"),
            ("mcp-session-id", &sid),
            ("last-event-id", "0"),
        ],
    );
    assert!(head2.contains("text/event-stream"), "{head2}");
    let mut buf2 = rest2;
    assert!(
        read_until(
            &mut stream2,
            &mut buf2,
            "notifications/resources/updated",
            Duration::from_secs(5)
        ),
        "the resumed stream replays the missed mint: {buf2}"
    );
    assert!(
        buf2.contains("id: 1") && !buf2.contains("id: 0"),
        "resume replays seq>cursor exactly once — no loss, no duplicate: {buf2}"
    );
}
