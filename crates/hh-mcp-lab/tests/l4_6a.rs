//! L4.6a — the `hh-mcp-lab` surface leg (ADR-0363; ticket 164w;
//! R-2.11.3¹): the Lab surface driven live by a real MCP client over
//! loopback Streamable HTTP — `launch_run` opens a child run on the
//! provisioned local lane (`env:HH_LOCAL_*` coordinates, USD 0),
//! `submit_input` drives its model turn, `resources/list` +
//! `resources/read` expose the ledger views — then `verify` +
//! `replay{deterministic}` prove the durable halves cross-service.
//!
//! The official `@modelcontextprotocol/sdk` client arm runs first when
//! `HH_LIVE_MCP_LAB_SDK` names a populated temp install dir (the leg
//! installs it runtime-only — never a repo dep); the in-repo
//! `McpClient<HttpTransport>` is the same-wire fallback — the
//! `MCP_LAB_CLIENT:` marker names which client actually ran.
//!
//! The un-ignored cell `l46a_submit_input_lowers_content` is the
//! L4.6a regression test for the `submit_input` content→input
//! lowering (a verbatim `content` forward never reached the strict
//! `submit` params).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use hharness_assembly::grammar::Assembly;
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_mcp::client::{McpClient, ToolOutcome};
use hharness_mcp::http::HttpTransport;
use hharness_mcp_lab::binding::{stdio_launch_binding, CallerBinding, CallerKind};
use hharness_mcp_lab::server::LabServer;
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

const TASK: &str = "You have exactly one tool available: hh_submit, which takes a string parameter named text. Call hh_submit now with text \"done\". If you cannot call tools, reply with the single word done.";

// ── fixtures (the s4_11 service/doc shape + the l4_6 local lane) ─────

fn test_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-l46a-mcplab-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service(store: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: store.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: store.join("ws"),
        holder: "l4.6a-mcp-lab".into(),
    })
    .unwrap()
}

fn agent() -> CallerBinding {
    stdio_launch_binding("l46a-agent", CallerKind::Agent)
}

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

/// The hir/1 document — the s4_11 fixture shape; `live=true` binds the
/// `model_port` slot to the provisioned local lane (`env:` coordinates
/// resolve inside the kernel — the NAME only, never a value).
fn document_json(live: bool) -> Json {
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
                            hard: Some(8),
                            soft: None,
                        },
                    ),
                    (
                        "tool_calls".to_string(),
                        DimensionBound {
                            hard: Some(1_000),
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
    if live {
        slots.insert(
            "model_port".to_string(),
            SlotBindings::One(SlotBinding {
                variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
                params: BTreeMap::from([
                    ("dialect".to_string(), Json::str("openai-compatible@1")),
                    ("endpoint".to_string(), Json::str("env:HH_LOCAL_BASE_URL")),
                    (
                        "provider_model_id".to_string(),
                        Json::str("env:HH_LOCAL_MODEL"),
                    ),
                ]),
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

fn launch_args(live: bool, idem: &str) -> Json {
    Json::obj([
        (
            "definition",
            Json::obj([
                ("kind", Json::str("document")),
                ("document", document_json(live)),
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
                Json::obj([("tool_calls", Json::obj([("hard", Json::Int(100))]))]),
            )]),
        ),
        ("idempotency_key", Json::str(idem)),
    ])
}

/// `tools/call{name, arguments}` → the `result` member (in-process
/// dispatch — the regression cell's channel).
fn call(srv: &mut LabServer, binding: &CallerBinding, name: &str, arguments: Json) -> Json {
    let frame = srv
        .handle_message(
            binding,
            &Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", Json::Int(7)),
                ("method", Json::str("tools/call")),
                (
                    "params",
                    Json::obj([("name", Json::str(name)), ("arguments", arguments)]),
                ),
            ]),
        )
        .expect("request frame");
    frame
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("no result: {}", frame.to_canonical_string()))
}

fn is_err(result: &Json) -> bool {
    matches!(result.get("isError"), Some(Json::Bool(true)))
}

fn structured(result: &Json) -> &Json {
    result.get("structuredContent").unwrap_or(&Json::Null)
}

/// L4.6a regression — `submit_input`'s public `content` arg lowers to
/// the embed `submit` op's strict `input[]` block list. Pre-fix the
/// verbatim `content` member died at `SubmitParams`'s closed decode
/// (`UnknownField`/`missing input`) and no `context.artefact.delivered`
/// row landed — the op never ran.
#[test]
fn l46a_submit_input_lowers_content() {
    let root = test_dir("regress");
    let mut srv = LabServer::with_default_exposure(service(&root));
    let a = agent();
    let r = call(
        &mut srv,
        &a,
        "launch_run",
        launch_args(false, "l46a-launch"),
    );
    assert!(
        !is_err(&r),
        "launch_run refused: {}",
        r.to_canonical_string()
    );
    let sc = structured(&r);
    let run_id = sc
        .get("launched")
        .and_then(|l| l.get("run_id"))
        .and_then(Json::as_str)
        .expect("launched.run_id")
        .to_string();
    let handle = sc
        .get("handles")
        .and_then(|h| match h {
            Json::Arr(a) => a.first(),
            _ => None,
        })
        .and_then(|h| h.get("handle"))
        .and_then(Json::as_str)
        .expect("handles[0].handle")
        .to_string();

    let r = call(
        &mut srv,
        &a,
        "submit_input",
        Json::obj([
            ("run", Json::str(handle.clone())),
            ("content", Json::str("ping")),
            ("idempotency_key", Json::str("l46a-sub-1")),
        ]),
    );
    // The strict-params refusal would carry `unknown`/`missing` in the
    // error spelling; a reached op answers its own outcome — refused
    // (stage-pending lane) or observed, never a params decode fault.
    let body = r.to_canonical_string();
    assert!(
        !body.contains("UnknownField")
            && !body.contains("unknown member")
            && !body.contains("\"missing\""),
        "submit_input died at params decode — the content→input lowering is absent: {body}"
    );
    // Durable proof the op ran: the staged input's delivered row.
    let delivered = srv
        .svc
        .surface_events(&run_id)
        .unwrap()
        .iter()
        .any(|e| e.class == "context.artefact.delivered");
    assert!(delivered, "submit staged no delivered row: {body}");
    let _ = std::fs::remove_dir_all(&root);
}

// ── the live leg ────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn provisioned() -> bool {
    std::env::var("HH_LOCAL_BASE_URL").is_ok() && std::env::var("HH_LOCAL_MODEL").is_ok()
}

/// Spawn the Streamable-HTTP serve loop on its own thread — the
/// `!Send` pointer hand-off the r2_19/s4_11 fixtures use (the serve
/// thread owns `srv` for the test's remainder).
fn spawn_http(srv: LabServer, listener: TcpListener) {
    // The in-repo `HttpTransport` always sends `Origin: http://test.local`
    // — the policy allowlists it verbatim (the official SDK sends no
    // `Origin`, admitted under require=false).
    let origins = hharness_mcp::http::OriginPolicy::new(&["http://test.local"], false);
    let srv_ptr = Box::into_raw(Box::new(srv)) as usize;
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    let origins_ptr = Box::into_raw(Box::new(origins)) as usize;
    std::thread::spawn(move || {
        let srv = unsafe { &mut *(srv_ptr as *mut LabServer) };
        let listener = unsafe { &*(listener_ptr as *const std::net::TcpListener) };
        let origins = unsafe { &*(origins_ptr as *const hharness_mcp::http::OriginPolicy) };
        let _ = hharness_mcp_lab::http::serve_http(srv, listener, origins);
    });
}

/// The `tools/call` outcome's `structuredContent` — official-SDK and
/// in-repo clients both carry the verbatim member.
fn call_ok(out: &ToolOutcome, name: &str) -> Json {
    match out {
        ToolOutcome::Observed { result } => result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| result.clone()),
        other => panic!("{name}: not an observed call: {other:?}"),
    }
}

/// Drive the surface through `client`: discover+initialize already
/// ran — `tools/list` → `launch_run` → `submit_input` → `resources/*`.
/// Returns `(child_run_id, launched_handle)`.
fn drive_client(client: &mut McpClient<HttpTransport>) -> (String, String) {
    let listing = client.list_tools().expect("tools/list");
    let names: Vec<String> = listing
        .tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Json::as_str).map(str::to_string))
        .collect();
    for want in [
        "launch_run",
        "submit_input",
        "cancel_run",
        "respond_approval",
    ] {
        assert!(
            names.iter().any(|n| n == want),
            "tools/list missing {want}: {names:?}"
        );
    }
    println!("MCP_LAB_TOOLS: {}", names.join(","));

    let out = client
        .call_tool("launch_run", &launch_args(true, "l46a-live-launch"), None)
        .expect("launch_run call");
    let sc = call_ok(&out, "launch_run");
    let run_id = sc
        .get("launched")
        .and_then(|l| l.get("run_id"))
        .and_then(Json::as_str)
        .expect("launched.run_id")
        .to_string();
    let handle = sc
        .get("handles")
        .and_then(|h| match h {
            Json::Arr(a) => a.first(),
            _ => None,
        })
        .and_then(|h| h.get("handle"))
        .and_then(Json::as_str)
        .expect("handles[0].handle")
        .to_string();
    println!("MCP_LAB_LAUNCH: run={run_id} handle={handle}");

    // Ledger/resource reads while the child run is still open — the
    // `hh://run/{handle}/ledger` resource attaches a take-over session,
    // which answers typed `Draining` once `lifecycle.run.finished`
    // lands, so the read goes out *before* `submit_input` starts the
    // turn. `resources/list` first, then `resources/read`.
    let listed = client
        .request_raw("resources/list", Json::obj([]))
        .expect("resources/list");
    let uris: Vec<String> = listed
        .get("resources")
        .and_then(|r| match r {
            Json::Arr(v) => Some(
                v.iter()
                    .filter_map(|e| e.get("uri").and_then(Json::as_str).map(str::to_string))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    assert!(!uris.is_empty(), "resources/list empty");
    println!("MCP_LAB_RESOURCES: {}", uris.len());
    let ledger_uri = uris
        .iter()
        .find(|u| u.contains("ledger"))
        .cloned()
        .unwrap_or_else(|| uris[0].clone());
    let read = client
        .request_raw(
            "resources/read",
            Json::obj([("uri", Json::str(ledger_uri.clone()))]),
        )
        .expect("resources/read");
    assert!(
        read.get("contents").is_some() || read.to_canonical_string().len() > 2,
        "resources/read empty: {read:?}"
    );
    println!("MCP_LAB_READ: uri={ledger_uri}");

    let out = client
        .call_tool(
            "submit_input",
            &Json::obj([
                ("run", Json::str(handle.clone())),
                ("content", Json::str(TASK)),
                ("idempotency_key", Json::str("l46a-live-sub")),
            ]),
            None,
        )
        .expect("submit_input call");
    let sc = call_ok(&out, "submit_input");
    let body = sc.to_canonical_string();
    assert!(
        !body.contains("UnknownField") && !body.contains("\"missing\""),
        "submit_input died at params decode: {body}"
    );
    println!("MCP_LAB_SUBMIT: ok");

    // Poll `run_status` until the turn settles — the launched run must
    // actually reach a terminal ledger state under the live model.
    let mut status = String::new();
    for _ in 0..120 {
        let st = client
            .call_tool(
                "run_status",
                &Json::obj([("run", Json::str(handle.clone()))]),
                None,
            )
            .expect("run_status call");
        let sc = call_ok(&st, "run_status");
        status = sc
            .get("state")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if status != "open" && status != "paused" && !status.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert_eq!(status, "completed", "run did not complete: {status}");
    println!("MCP_LAB_STATUS: {status}");
    (run_id, handle)
}

/// `verify` + deterministic `replay` on a second service over the same
/// store — the cross-service attach proves the durable record reads
/// off disk, never the serving session's memory.
fn verify_replay(store_root: &Path, run_id: &str) {
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: store_root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: store_root.join("ws2"),
        holder: "l4.6a-verify".into(),
    })
    .unwrap();
    let mut call = |m: &str, p: Json| -> Result<Json, String> {
        let r = svc.handle(&Request {
            id: Json::str(format!("v-{m}")),
            method: m.into(),
            params: p,
        });
        r.get("result")
            .cloned()
            .ok_or_else(|| r.to_canonical_string())
    };
    call(
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("l4.6a-verify")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([("experimental", Json::Bool(true))]),
            ),
        ]),
    )
    .unwrap_or_else(|e| panic!("hello: {e}"));
    let s = call(
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id.to_string())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str(format!("k-attach-{run_id}"))),
        ]),
    )
    .unwrap_or_else(|e| panic!("attach: {e}"));
    let sess = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut from = 0i64;
    let mut events = Vec::new();
    loop {
        let page = call(
            "read",
            Json::obj([
                ("session_id", Json::str(sess.clone())),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(from))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
            ]),
        )
        .unwrap_or_else(|e| panic!("read: {e}"));
        if let Json::Arr(evs) = page.get("events").cloned().unwrap_or(Json::Null) {
            let n = evs.len();
            events.extend(evs);
            if n < 256 {
                break;
            }
            from += n as i64;
        } else {
            break;
        }
    }
    fn class(e: &Json) -> &str {
        e.get("class").and_then(Json::as_str).unwrap_or("")
    }
    assert!(
        events.iter().any(|e| class(e) == "model.call.requested"),
        "model.call.requested absent on {run_id}"
    );
    assert!(
        events.iter().any(|e| class(e) == "model.call.completed"),
        "model.call.completed absent on {run_id}"
    );
    let model_rows = events
        .iter()
        .filter(|e| class(e).starts_with("model."))
        .count();
    // The provider's own usage fold off the durable row — the evidence
    // record's `token_usage`/`model_reported` members ride these markers.
    let completed = events
        .iter()
        .find(|e| class(e) == "model.call.completed")
        .expect("model.call.completed row");
    let usage_view = completed
        .get("payload")
        .and_then(|p| p.get("usage"))
        .and_then(|u| u.get("record"))
        .and_then(|r| r.get("view"))
        .cloned()
        .unwrap_or(Json::Null);
    if usage_view != Json::Null {
        println!("LIVE_USAGE: {}", usage_view.to_canonical_string());
    }
    if let Some(m) = completed
        .get("payload")
        .and_then(|p| p.get("served_model"))
        .and_then(Json::as_str)
    {
        println!("LIVE_MODEL: {m}");
    }
    let v = call(
        "verify",
        Json::obj([("session_id", Json::str(sess.clone()))]),
    )
    .unwrap_or_else(|e| panic!("verify: {e}"));
    assert_eq!(
        v.get("verdict").and_then(Json::as_str),
        Some("ok"),
        "verify verdict"
    );
    let rep = call(
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    )
    .unwrap_or_else(|e| panic!("replay: {e}"));
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "replay mode: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "replay diverged: {}",
        rep.to_canonical_string()
    );
    // The replay consulted no provider — the model.* count is unchanged.
    let mut model_after = 0;
    {
        let page = call(
            "read",
            Json::obj([
                ("session_id", Json::str(sess.clone())),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(1024)),
            ]),
        )
        .unwrap();
        if let Json::Arr(evs) = page.get("events").cloned().unwrap_or(Json::Null) {
            model_after = evs
                .iter()
                .filter(|e| class(e).starts_with("model."))
                .count();
        }
    }
    assert_eq!(
        model_rows, model_after,
        "replay minted provider rows — not offline"
    );
    println!("MCP_LAB_REPLAY: run={run_id} deterministic model_rows={model_rows}");
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh surface-mcp-lab"]
fn l46a_live_mcp_lab() {
    if !provisioned() {
        eprintln!("unprovided — skipped");
        return;
    }
    let root = test_dir("live");
    let mut srv = LabServer::with_default_exposure(service(&root));
    srv.grant_bearer("tok-l46a", agent());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    spawn_http(srv, listener);

    let mut client_kind = "in-repo".to_string();
    let mut official_out = String::new();
    // The official arm — `node <sdk>/live-capstone-mcp-lab.mjs` against
    // the served URL; the SDK lives in a temp dir the leg populated.
    if let Ok(sdk) = std::env::var("HH_LIVE_MCP_LAB_SDK") {
        let args_file = root.join("launch_args.json");
        std::fs::write(
            &args_file,
            launch_args(true, "l46a-live-launch").to_canonical_string(),
        )
        .unwrap();
        let src = repo_root().join("scripts/live-capstone-mcp-lab.mjs");
        let dst = PathBuf::from(&sdk).join("live-capstone-mcp-lab.mjs");
        let _ = std::fs::copy(&src, &dst);
        let out = Command::new("node")
            .arg(&dst)
            .arg(format!("http://{addr}/mcp"))
            .arg("tok-l46a")
            .arg(&args_file)
            .env(
                "HH_LOCAL_BASE_URL",
                std::env::var("HH_LOCAL_BASE_URL").unwrap(),
            )
            .env("HH_LOCAL_MODEL", std::env::var("HH_LOCAL_MODEL").unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("node driver spawn");
        official_out = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        print!("{official_out}");
        if out.status.success() {
            client_kind = "official-sdk".to_string();
        }
    }

    if client_kind == "in-repo" {
        // The in-repo client on the same real transport — probe-first
        // connect (server/discover → pinned initialize), bearer grant.
        // The Streamable-HTTP session gate answers 400 for any non-
        // `initialize` POST without `Mcp-Session-Id` — so `initialize`
        // goes first on the raw transport (the session id is captured
        // and echoed on every later POST), then `connect`'s discover-
        // first exchange rides that session.
        let mut t = HttpTransport::new(&addr.to_string(), "/mcp", "http://test.local");
        t.set_bearer("tok-l46a");
        {
            use hharness_mcp::Transport;
            let init = Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", Json::Int(1)),
                ("method", Json::str("initialize")),
                (
                    "params",
                    Json::obj([
                        ("protocolVersion", Json::str("2026-07-28")),
                        ("capabilities", Json::obj([])),
                        (
                            "clientInfo",
                            Json::obj([("name", Json::str("l4.6a")), ("version", Json::str("1"))]),
                        ),
                    ]),
                ),
            ]);
            t.send(&init.to_canonical_string())
                .expect("initialize send");
            let _ = t.recv().expect("initialize recv");
            assert!(
                t.session_id().is_some(),
                "no Mcp-Session-Id captured at initialize"
            );
        }
        let mut client = McpClient::connect(t).expect("mcp connect");
        let b = client.binding().clone();
        println!(
            "MCP_LAB_BINDING: era={:?} version={}",
            b.era, b.negotiated_version
        );
        let (run_id, _handle) = drive_client(&mut client);
        verify_replay(&root, &run_id);
        println!("MCP_LAB_CLIENT: in-repo");
    } else {
        // The official run leaves the child run id on its marker —
        // verify+replay it off the same store.
        let run_id = official_out
            .lines()
            .find_map(|l| l.strip_prefix("MCP_LAB_LAUNCH: run="))
            .and_then(|rest| rest.split_whitespace().next())
            .expect("official driver printed no run id");
        verify_replay(&root, run_id);
        println!("MCP_LAB_CLIENT: official-sdk");
    }
    let _ = std::fs::remove_dir_all(&root);
}
