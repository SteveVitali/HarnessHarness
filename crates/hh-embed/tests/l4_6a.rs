//! L4.6a (ticket 164w; ADR-0363) — the public SDK / embed-library leg:
//! the surface a downstream integrator actually consumes, driven live.
//!
//! Two arms on the provisioned local lane (`env:HH_LOCAL_*` coordinates,
//! USD 0 — the same `model_port` binding the L4.6 capstone compiles):
//!
//! - **binding (b) stdio** — `hh-kernel serve` spawned as a child and
//!   driven by the *generated* client through the public facade path
//!   (`hharness::embed_client::Client`) — canonical newline-delimited
//!   JSON-RPC, exactly the bytes an embed consumer writes.
//! - **binding (c) loopback HTTP** — `hh-kernel serve --http` +
//!   `NetClient` (token via `--token-file`, never argv).
//! - **consumer crate** — a crate authored *outside* the workspace (temp
//!   dir, `path` dep on the shipped `hharness` facade — never a repo
//!   member) compiled at leg time and driven in-process: `EmbedService`
//!   → `submit` → `verify` → `replay` on the same lane.
//!
//! Each arm: `hello` → `open_session` → `submit` →
//! `model.call.requested/completed{usage:available}` → `verify{ok}` →
//! `replay{deterministic}` — byte-identical offline (model.* row count
//! unchanged across the replay; no provider re-call).
//!
//! `#[ignore]`d — `scripts/live-verify.sh embed-sdk-live` drives it;
//! unprovisioned lanes print `unprovided — skipped`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::BufReader;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use hharness_assembly::grammar::Assembly;
use hharness_embed_client_generated::NetClient;
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;

/// The same reference task the L4.6 capstone runs — the wire leg is
/// what's proven, never the content.
const REFERENCE_TASK: &str = "You have exactly one tool available: hh_submit, which takes a string parameter named text. Call hh_submit now with text \"done\". If you cannot call tools, reply with the single word done.";

fn provisioned() -> bool {
    std::env::var("HH_LOCAL_BASE_URL").is_ok() && std::env::var("HH_LOCAL_MODEL").is_ok()
}

// ── the definition document (the L4.6 live-lane shape, CC1) ────────────

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

/// The hir/1 document with `model_port` bound to the provisioned local
/// lane — `env:` coordinates only, never a value.
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
                            hard: Some(4),
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
    let slot = BTreeMap::from([
        ("dialect".to_string(), Json::str("openai-compatible@1")),
        ("endpoint".to_string(), Json::str("env:HH_LOCAL_BASE_URL")),
        (
            "provider_model_id".to_string(),
            Json::str("env:HH_LOCAL_MODEL"),
        ),
    ]);
    let mut slots = BTreeMap::new();
    slots.insert(
        "model_port".to_string(),
        SlotBindings::One(SlotBinding {
            variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
            params: slot,
            enabled: true,
            locality: None,
        }),
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

fn hello_params() -> Json {
    Json::obj(vec![
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj(vec![
                ("name", Json::str("l4.6a-embed-sdk")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        (
            "capabilities",
            Json::obj(vec![("experimental", Json::Bool(true))]),
        ),
    ])
}

fn open_spec(doc: Json) -> Json {
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

// ── spawned kernels ───────────────────────────────────────────────────

fn bin(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dbg = exe.parent().unwrap().parent().unwrap();
    let p = dbg.join(name);
    assert!(p.exists(), "{name} not built — the leg builds it first");
    p
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-l46a-sdk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_stdio(store: &Path, ws: &Path) -> Child {
    std::fs::create_dir_all(store).unwrap();
    std::fs::create_dir_all(ws).unwrap();
    Command::new(bin("hh-kernel"))
        .env("HH_STORE_ROOT", store)
        .env("HH_WORKSPACE_ROOT", ws)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hh-kernel serve (stdio)")
}

fn spawn_http(store: &Path, ws: &Path) -> (Child, SocketAddr, String) {
    std::fs::create_dir_all(store).unwrap();
    std::fs::create_dir_all(ws).unwrap();
    let port = free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let tfile = store.join("kernel.token");
    let mut cmd = Command::new(bin("hh-kernel"));
    cmd.env("HH_STORE_ROOT", store)
        .env("HH_WORKSPACE_ROOT", ws)
        .args([
            "serve",
            "--http",
            &addr.to_string(),
            "--token-file",
            tfile.to_str().unwrap(),
        ])
        .stderr(Stdio::null())
        .stdout(Stdio::null());
    let child = cmd.spawn().expect("spawn hh-kernel serve --http");
    let mut token = String::new();
    for _ in 0..100 {
        if let Ok(t) = std::fs::read_to_string(&tfile) {
            if !t.trim().is_empty() {
                token = t.trim().to_string();
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(!token.is_empty(), "kernel --token-file never landed");
    let mut up = false;
    for _ in 0..100 {
        if std::net::TcpStream::connect(addr).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(up, "serve --http never answered on {addr}");
    (child, addr, token)
}

// ── the shared drive (one `call` per binding) ─────────────────────────

type Call<'a> = dyn FnMut(&str, Json) -> Result<Json, String> + 'a;

fn read_all(call: &mut Call, sess: &str, label: &str) -> Vec<Json> {
    let mut out = Vec::new();
    let mut from = 0i64;
    loop {
        let page = call(
            "read",
            Json::obj([
                ("session_id", Json::str(sess.to_string())),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(from))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
            ]),
        )
        .unwrap_or_else(|e| panic!("{label}: read: {e}"));
        if let Json::Arr(evs) = page.get("events").cloned().unwrap_or(Json::Null) {
            let n = evs.len();
            out.extend(evs);
            if n < 256 {
                break;
            }
            from += n as i64;
        } else {
            break;
        }
    }
    out
}

fn event_class(e: &Json) -> &str {
    e.get("class").and_then(Json::as_str).unwrap_or("")
}

/// `hello → open_session → submit → read → verify → replay` — the
/// integrator's full loop through the public client. Returns the
/// `(run_id, usage_view, served_model)`.
fn drive(call: &mut Call, doc: Json, label: &str, tag: &str) -> (String, Json, String) {
    call("hello", hello_params()).unwrap_or_else(|e| panic!("{label}: hello: {e}"));
    let s = call(
        "open_session",
        Json::obj(vec![
            ("spec", open_spec(doc)),
            ("idempotency_key", Json::str(format!("k-{tag}"))),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: open_session: {e}"));
    let sess = s
        .get("session_id")
        .and_then(Json::as_str)
        .expect("session_id")
        .to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    call(
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(sess.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str(REFERENCE_TASK)),
                ])]),
            ),
            ("idempotency_key", Json::str(format!("k-{tag}-sub"))),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: submit: {e}"));
    let events = read_all(call, &sess, label);

    let requested = events
        .iter()
        .find(|e| event_class(e) == "model.call.requested")
        .unwrap_or_else(|| panic!("{label}: model.call.requested lands"));
    assert!(
        requested
            .get("payload")
            .and_then(|p| p.get("plan_hash"))
            .is_some(),
        "{label}: plan_hash member"
    );
    let completed = events
        .iter()
        .find(|e| event_class(e) == "model.call.completed")
        .unwrap_or_else(|| {
            for e in events
                .iter()
                .filter(|e| event_class(e) == "model.call.failed")
            {
                eprintln!("{label} FAILED ROW: {}", e.to_canonical_string());
            }
            panic!("{label}: model.call.completed lands (the live stream decoded)")
        });
    let cp = completed.get("payload").cloned().unwrap_or(Json::Null);
    let usage = cp.get("usage").expect("usage member");
    assert_eq!(
        usage.get("available"),
        Some(&Json::Bool(true)),
        "{label}: the provider reports usage"
    );
    let model_count = events
        .iter()
        .filter(|e| event_class(e).starts_with("model."))
        .count();

    let v = call(
        "verify",
        Json::obj([("session_id", Json::str(sess.clone()))]),
    )
    .unwrap_or_else(|e| panic!("{label}: verify: {e}"));
    assert_eq!(
        v.get("verdict").and_then(Json::as_str),
        Some("ok"),
        "{label}: verify verdict"
    );
    let rep = call(
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: replay: {e}"));
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "{label}: deterministic replay"
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "{label}: replay diverged: {}",
        rep.to_canonical_string()
    );
    let after = read_all(call, &sess, label);
    let model_after = after
        .iter()
        .filter(|e| event_class(e).starts_with("model."))
        .count();
    assert_eq!(
        model_count, model_after,
        "{label}: replay minted provider rows — not offline"
    );

    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .cloned()
        .unwrap_or(Json::Null);
    let served = cp
        .get("served_model")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    (run, view, served)
}

// ── the out-of-workspace consumer crate ────────────────────────────────

/// The consumer's `main.rs` — a downstream integrator driving the public
/// `hharness` facade (`hharness::embed::EmbedService`, binding (a)
/// in-process): hello → open_session → submit → read → verify → replay,
/// with the offline assertions the leg itself checks on the spawned
/// bindings. Written to a temp dir at leg time — never a workspace member.
const CONSUMER_MAIN_RS: &str = r#"
use hharness::embed::service::{EmbedService, ServiceConfig};
use hharness::wire::json::Json;
use hharness::wire::jsonrpc::Request;
use std::path::PathBuf;

fn call(svc: &mut EmbedService, m: &str, p: Json) -> Json {
    let r = svc.handle(&Request {
        id: Json::str(format!("c-{m}")),
        method: m.to_string(),
        params: p,
    });
    r.get("result")
        .cloned()
        .unwrap_or_else(|| panic!("{m}: {}", r.to_canonical_string()))
}

fn read_all(svc: &mut EmbedService, sess: &str) -> Vec<Json> {
    let mut out = Vec::new();
    let mut from = 0i64;
    loop {
        let page = call(
            svc,
            "read",
            Json::obj([
                ("session_id", Json::str(sess.to_string())),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(from))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
            ]),
        );
        match page.get("events") {
            Some(Json::Arr(evs)) => {
                let n = evs.len();
                out.extend(evs.iter().cloned());
                if n < 256 {
                    break;
                }
                from += n as i64;
            }
            _ => break,
        }
    }
    out
}

fn class(e: &Json) -> &str {
    e.get("class").and_then(Json::as_str).unwrap_or("")
}

fn main() {
    let doc_path = PathBuf::from(std::env::args().nth(1).expect("doc path"));
    let doc = hharness::wire::json::parse(
        &std::fs::read_to_string(&doc_path).expect("doc file"),
    )
    .expect("doc json");
    let store = PathBuf::from(std::env::var("HH_STORE_ROOT").unwrap());
    let ws = PathBuf::from(std::env::var("HH_WORKSPACE_ROOT").unwrap());
    std::fs::create_dir_all(&store).unwrap();
    std::fs::create_dir_all(&ws).unwrap();
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: store,
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: ws,
        holder: "l4.6a-consumer".into(),
    })
    .expect("EmbedService::open");

    call(
        &mut svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("l46a-consumer")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("sdk")),
                ]),
            ),
            ("capabilities", Json::obj([("experimental", Json::Bool(true))])),
        ]),
    );
    let s = call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("new")),
                    (
                        "definition",
                        Json::obj([("kind", Json::str("document")), ("document", doc)]),
                    ),
                    ("overrides", Json::Arr(vec![])),
                    (
                        "environment",
                        Json::obj([
                            ("kind", Json::str("connection_info")),
                            ("connection_info", Json::obj([("class", Json::str("local_host"))])),
                        ]),
                    ),
                    (
                        "attendance",
                        Json::obj([("value", Json::str("async")), ("source", Json::str("declared"))]),
                    ),
                ]),
            ),
            ("idempotency_key", Json::str("k-consumer-1")),
        ]),
    );
    let sess = s.get("session_id").and_then(Json::as_str).unwrap().to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    call(
        &mut svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str(std::env::var("L46A_TASK").unwrap())),
                ])]),
            ),
            ("idempotency_key", Json::str("k-consumer-sub")),
        ]),
    );
    let events = read_all(&mut svc, &sess);
    let completed = events
        .iter()
        .find(|e| class(e) == "model.call.completed")
        .expect("model.call.completed");
    let usage = completed
        .get("payload")
        .and_then(|p| p.get("usage"))
        .cloned()
        .unwrap_or(Json::Null);
    assert_eq!(
        usage.get("available"),
        Some(&Json::Bool(true)),
        "the provider reports usage"
    );
    let model_count = events
        .iter()
        .filter(|e| class(e).starts_with("model."))
        .count();
    let v = call(&mut svc, "verify", Json::obj([("session_id", Json::str(sess.clone()))]));
    assert_eq!(v.get("verdict").and_then(Json::as_str), Some("ok"), "verify");
    let rep = call(
        &mut svc,
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    );
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic")
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "replay diverged"
    );
    let after = read_all(&mut svc, &sess);
    let model_after = after
        .iter()
        .filter(|e| class(e).starts_with("model."))
        .count();
    assert_eq!(model_count, model_after, "replay minted provider rows");
    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .cloned()
        .unwrap_or(Json::Null);
    println!("CONSUMER_RUN: {run}");
    if view != Json::Null {
        println!("LIVE_USAGE: {}", view.to_canonical_string());
    }
    if let Some(m) = completed
        .get("payload")
        .and_then(|p| p.get("served_model"))
        .and_then(Json::as_str)
    {
        println!("LIVE_MODEL: {m}");
    }
    println!("CONSUMER: pass");
}
"#;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

/// Author + build the out-of-workspace consumer crate; returns its binary
/// path. `path` dep on the shipped `hharness` facade — the crate lives in
/// a temp dir, never in the workspace.
fn build_consumer(root: &Path) -> PathBuf {
    let crate_dir = root.join("consumer");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"l46a-consumer\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nhharness = {{ path = \"{}\" }}\n",
            repo_root().join("crates/hharness").display()
        ),
    )
    .unwrap();
    std::fs::write(crate_dir.join("src/main.rs"), CONSUMER_MAIN_RS).unwrap();
    let out = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .arg(crate_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", repo_root().join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo build consumer");
    assert!(
        out.status.success(),
        "consumer crate build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    repo_root().join("target/debug/l46a-consumer")
}

// ── the leg ───────────────────────────────────────────────────────────

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh embed-sdk-live"]
fn l46a_live_embed_sdk() {
    if !provisioned() {
        eprintln!("unprovided — skipped");
        return;
    }
    let doc = document_json();

    // ── arm 1: binding (b) stdio, the generated client through the
    //    public `hharness` facade — the path a downstream crate takes.
    let stdio_root = tmpdir("stdio");
    let mut child = spawn_stdio(&stdio_root.join("store"), &stdio_root.join("ws"));
    let (stdio_run, mut usage_total, mut served_model) = {
        // `hharness::embed_client::Client` — the facade re-export; the
        // consumer's spelling, not the crate-local one.
        let mut client = hharness::embed_client::Client::new(
            BufReader::new(child.stdout.take().unwrap()),
            child.stdin.take().unwrap(),
        );
        let mut call = |m: &str, p: Json| client.call(m, p).map_err(|e| format!("{e:?}"));
        let (run, view, served) = drive(&mut call, doc.clone(), "sdk:stdio", "l46a-stdio");
        println!("EMBED_SDK_STDIO: run={run}");
        drop(client);
        (run, view, served)
    };
    let _ = child.kill();
    let _ = child.wait();

    // ── arm 2: binding (c) loopback HTTP, `NetClient` (facade path).
    let http_root = tmpdir("http");
    let (mut hchild, addr, token) = spawn_http(&http_root.join("store"), &http_root.join("ws"));
    {
        let mut client = hharness::embed_client::NetClient::connect(addr, &token);
        let mut call = |m: &str, p: Json| client.call(m, p).map_err(|e| format!("{e:?}"));
        let (run, view, served) = drive(&mut call, doc, "sdk:http", "l46a-http");
        if usage_total == Json::Null {
            usage_total = view;
        }
        if served_model.is_empty() {
            served_model = served;
        }
        println!("EMBED_SDK_HTTP: run={run}");
    }
    let _ = hchild.kill();
    let _ = hchild.wait();

    // The in-crate alias proves the facade and the crate export the same
    // type (the leg imports it for that compile-time check).
    let _ = std::mem::size_of::<NetClient>();

    // ── arm 3: a consumer crate outside the workspace — compiled at leg
    //    time against `path`-dep `hharness`, driven in-process through
    //    `EmbedService` (binding a) on its own store.
    let consumer_root = tmpdir("consumer");
    let consumer_bin = build_consumer(&consumer_root);
    let doc_file = consumer_root.join("def.json");
    std::fs::write(&doc_file, document_json().to_canonical_string()).unwrap();
    let cout = Command::new(&consumer_bin)
        .arg(&doc_file)
        .env("HH_STORE_ROOT", consumer_root.join("store"))
        .env("HH_WORKSPACE_ROOT", consumer_root.join("ws"))
        .env("L46A_TASK", REFERENCE_TASK)
        .env(
            "HH_LOCAL_BASE_URL",
            std::env::var("HH_LOCAL_BASE_URL").unwrap(),
        )
        .env("HH_LOCAL_MODEL", std::env::var("HH_LOCAL_MODEL").unwrap())
        .output()
        .expect("spawn consumer binary");
    let ctext = format!(
        "{}{}",
        String::from_utf8_lossy(&cout.stdout),
        String::from_utf8_lossy(&cout.stderr)
    );
    print!("{ctext}");
    assert!(
        cout.status.success() && ctext.contains("CONSUMER: pass"),
        "consumer crate drive failed: {ctext}"
    );
    if let Some(line) = ctext.lines().find(|l| l.starts_with("LIVE_USAGE: ")) {
        if usage_total == Json::Null {
            if let Ok(v) = hharness_wire::json::parse(&line["LIVE_USAGE: ".len()..]) {
                usage_total = v;
            }
        }
    }

    let usage_view = usage_total;

    println!("EMBED_SDK: arms=stdio+http+consumer-crate client=generated+facade run={stdio_run}");
    if usage_view != Json::Null {
        println!("LIVE_USAGE: {}", usage_view.to_canonical_string());
    }
    if !served_model.is_empty() {
        println!("LIVE_MODEL: {served_model}");
    } else {
        println!("LIVE_MODEL: {}", std::env::var("HH_LOCAL_MODEL").unwrap());
    }
}
