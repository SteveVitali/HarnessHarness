//! L4.6 — the live end-to-end capstone (ADR-0363): the *reference task*
//! run for real through the shipped surface over every binding, every
//! provisioned provider, then verified + replayed.
//!
//! `HH_CAPSTONE_PROVIDER` (`local` | `anthropic` | `openai`) selects the
//! model lane; credentials ride `credential_ref: env:NAME` *coordinates*
//! — the value resolves inside the kernel's transport and never enters a
//! payload, blob, transcript or record (`provided: yes/no` only). The
//! cells are `#[ignore]`d — `scripts/live-verify.sh capstone-*` drives
//! them via `cargo test -- --ignored` (ADR-0363 D7); legs:
//!
//!   * `l46_live_reference_task` — `capstone-run-{local,anthropic,openai}`:
//!     the task over binding (a) in-process `EmbedService`, binding (b)
//!     the real `hh` CLI binary (`run start`/`status`/`events` — the
//!     spawned `hh-kernel serve` stdio transport), and binding (c)
//!     loopback HTTP (`hh-kernel serve --http` + the generated
//!     `NetClient` — a fresh task run *and* a read-only attach onto the
//!     `hh` run's store, verified + replayed cross-binding). Each arm:
//!     `open_session` → `submit` → `model.call.completed` (usage real) →
//!     `verify` → `replay{deterministic}` byte-identical, no provider
//!     re-call (model.* row count unchanged across the replay).
//!   * `l46_live_ledger_replay` — `capstone-ledger-replay`: an `hh` CLI
//!     run, then `verify` + deterministic `replay` over binding (c) —
//!     byte-identical offline.
//!   * `l46_live_web` — `capstone-web`: `hh-web` over the same served
//!     kernel — `GET /` answers the shell and `POST /api` `run_index`
//!     carries the live run's id (the HTTP read asserted).
//!   * `l46_live_bundle_repro` — `capstone-bundle-repro`: `hh bundle
//!     create --sink` → `hh bundle validate` → `hh bundle reproduce` on a
//!     *clean* store — the report's verdict recorded.
//!
//! Assertions are honest, never fabricated: a provider that cannot
//! answer prints `unprovided — skipped` and the leg records
//! `skipped(<reason>)`, not a pass.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

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

// ── the reference task + the lane table ─────────────────────────────────

/// The one task every provider/binding arm runs — a fixed instruction
/// whose only admissible shape is the `hh_submit` call or the one-word
/// refusal (the wire leg is what's proven, never the content).
const REFERENCE_TASK: &str = "You have exactly one tool available: hh_submit, which takes a string parameter named text. Call hh_submit now with text \"done\". If you cannot call tools, reply with the single word done.";

struct Lane {
    dialect: &'static str,
    endpoint: String,
    provider_model_id: String,
    /// The `env:NAME` credential coordinate — the NAME only, never a value.
    credential_env: Option<&'static str>,
    provider_params: BTreeMap<String, Json>,
}

fn local_lane() -> Lane {
    Lane {
        dialect: "openai-compatible@1",
        endpoint: "env:HH_LOCAL_BASE_URL".to_string(),
        provider_model_id: "env:HH_LOCAL_MODEL".to_string(),
        credential_env: None,
        provider_params: BTreeMap::new(),
    }
}

fn lane() -> Option<Lane> {
    match std::env::var("HH_CAPSTONE_PROVIDER").ok().as_deref() {
        Some("local") => Some(local_lane()),
        Some("anthropic") => Some(Lane {
            dialect: "anthropic-messages@1",
            endpoint: "https://api.anthropic.com/v1".to_string(),
            provider_model_id: "claude-haiku-4-5-20251001".to_string(),
            credential_env: Some("ANTHROPIC_API_KEY"),
            provider_params: BTreeMap::from([("max_tokens".to_string(), Json::Int(64))]),
        }),
        Some("openai") => Some(Lane {
            dialect: "openai@1",
            endpoint: "https://api.openai.com/v1".to_string(),
            provider_model_id: "gpt-4.1-mini-2025-04-14".to_string(),
            credential_env: Some("OPENAI_API_KEY"),
            provider_params: BTreeMap::from([("max_completion_tokens".to_string(), Json::Int(64))]),
        }),
        _ => None,
    }
}

fn provisioned(l: &Lane) -> bool {
    match l.dialect {
        "openai-compatible@1" => {
            std::env::var("HH_LOCAL_BASE_URL").is_ok() && std::env::var("HH_LOCAL_MODEL").is_ok()
        }
        _ => l
            .credential_env
            .map(|n| std::env::var(n).map(|v| !v.is_empty()).unwrap_or(false))
            .unwrap_or(true),
    }
}

// ── the definition document (r4_5d's live-lane shape, CC1) ──────────────

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

fn document_json(l: &Lane) -> Json {
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
                    // The loop bound — a live model that keeps calling
                    // escalates + parks, never spins.
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
    let mut slot = BTreeMap::from([
        ("dialect".to_string(), Json::str(l.dialect)),
        ("endpoint".to_string(), Json::str(l.endpoint.clone())),
        (
            "provider_model_id".to_string(),
            Json::str(l.provider_model_id.clone()),
        ),
    ]);
    if let Some(name) = l.credential_env {
        slot.insert(
            "credential_ref".to_string(),
            Json::str(format!("env:{name}")),
        );
    }
    if !l.provider_params.is_empty() {
        slot.insert(
            "provider_params".to_string(),
            Json::Obj(l.provider_params.clone()),
        );
    }
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

// ── boundary plumbing — one `call` closure per binding ──────────────────

type Call<'a> = dyn FnMut(&str, Json) -> Result<Json, String> + 'a;

fn hello_params() -> Json {
    Json::obj(vec![
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj(vec![
                ("name", Json::str("l4.6-live")),
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

/// open → submit → read the full prefix → verify → replay. Returns
/// `(session_id, run_id, events)`. `label` marks each arm's failures.
fn drive(call: &mut Call, doc: Json, label: &str, tag: &str) -> (String, String, Vec<Json>) {
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
    (sess, run, events)
}

/// `read` the session's full prefix over the binding itself.
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

fn event_payload(e: &Json) -> Json {
    e.get("payload").cloned().unwrap_or(Json::Null)
}

/// The live-run assertions every arm shares: `model.call.requested` +
/// `model.call.completed` (usage available) → `verify` ok →
/// `replay{deterministic}` no divergence + no new model rows (the
/// offline byte-identity proof). Returns the usage view and the served
/// model id (printed only for hosted lanes — the local lane's served id
/// is a provisioned value).
fn assert_live_run(
    call: &mut Call,
    sess: &str,
    events: &[Json],
    lane: &Lane,
    label: &str,
) -> (Json, Option<String>) {
    let requested = events
        .iter()
        .find(|e| event_class(e) == "model.call.requested")
        .unwrap_or_else(|| panic!("{label}: model.call.requested lands"));
    let rp = event_payload(requested);
    assert!(rp.get("plan_hash").is_some(), "{label}: plan_hash member");
    if let Some(name) = lane.credential_env {
        assert_eq!(
            rp.get("credential_binding_id").and_then(Json::as_str),
            Some(format!("launch_env:{name}").as_str()),
            "{label}: credential coordinate, never the value"
        );
    }
    let completed = events
        .iter()
        .find(|e| event_class(e) == "model.call.completed")
        .unwrap_or_else(|| {
            for e in events {
                if event_class(e) == "model.call.failed" {
                    eprintln!(
                        "{label} FAILED ROW: {}",
                        event_payload(e).to_canonical_string()
                    );
                }
            }
            panic!("{label}: model.call.completed lands (the live stream decoded)");
        });
    let cp = event_payload(completed);
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
        Json::obj([("session_id", Json::str(sess.to_string()))]),
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
            ("session_id", Json::str(sess.to_string())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: replay: {e}"));
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "{label}: deterministic replay: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "{label}: replay diverged: {}",
        rep.to_canonical_string()
    );
    // Offline byte-identity: the deterministic fold consulted no
    // provider — the model.* row count is unchanged across the replay.
    let after = read_all(call, sess, label);
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
        .map(str::to_string);
    (view, served)
}

/// The planted credential value appears in no event payload and no blob
/// (r4_5d's sweep — the store root exists only on the (a)/CLI arms).
fn leak_sweep(events: &[Json], secret: &str, store_root: &Path, label: &str) {
    let payloads: String = events
        .iter()
        .map(event_payload)
        .map(|p| p.to_canonical_string())
        .collect();
    assert!(
        !payloads.contains(secret),
        "{label}: a ledger payload carries the credential value"
    );
    let blobs = store_root.join("blobs");
    if blobs.exists() {
        for e in std::fs::read_dir(&blobs).unwrap() {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&b).contains(secret),
                "{label}: a blob carries the credential value"
            );
        }
    }
}

// ── spawned kernels + the `hh` binary ───────────────────────────────────

fn bin(name: &str) -> PathBuf {
    // target/debug/deps/<test>-<hash> → target/debug/<name>
    let exe = std::env::current_exe().unwrap();
    let dbg = exe.parent().unwrap().parent().unwrap();
    let p = dbg.join(name);
    assert!(p.exists(), "{name} not built — the leg builds it first");
    p
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-l46-{tag}-{}", std::process::id()));
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

fn kernel_envs(cmd: &mut Command, store: &Path, ws: &Path) {
    cmd.env("HH_STORE_ROOT", store).env("HH_WORKSPACE_ROOT", ws);
}

/// Binding (c): spawn `hh-kernel serve --http` on a free loopback port;
/// return the child, bound addr and the capability token (token-file —
/// never on argv).
fn spawn_http(store: &Path, ws: &Path) -> (Child, SocketAddr, String) {
    std::fs::create_dir_all(store).unwrap();
    std::fs::create_dir_all(ws).unwrap();
    let port = free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let tfile = store.join("kernel.token");
    let mut cmd = Command::new(bin("hh-kernel"));
    kernel_envs(&mut cmd, store, ws);
    cmd.args([
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

/// `hh` on its own store — the real CLI over binding (b).
fn hh_run(args: &[&str], store: &Path, ws: &Path, cwd: &Path) -> (bool, String) {
    let mut cmd = Command::new(bin("hh"));
    kernel_envs(&mut cmd, store, ws);
    cmd.env("HH_KERNEL_CMD", bin("hh-kernel"))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd.output().expect("spawn hh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// `hh run start` to terminal; returns the `run_id` parsed off the
/// emitted records.
fn hh_run_start(doc_file: &Path, store: &Path, ws: &Path, cwd: &Path) -> String {
    let (ok_, text) = hh_run(
        &[
            "run",
            "start",
            doc_file.to_str().unwrap(),
            REFERENCE_TASK,
            "--format",
            "json",
        ],
        store,
        ws,
        cwd,
    );
    eprintln!("hh run start → {}", text.lines().last().unwrap_or(""));
    assert!(ok_, "hh run start failed: {text}");
    let run = text
        .lines()
        .filter_map(|l| hharness_wire::json::parse(l).ok())
        .find_map(|j| {
            for k in ["run_id", "payload", "result"] {
                let v = if k == "run_id" {
                    j.get(k)
                } else {
                    j.get(k).and_then(|r| r.get("run_id"))
                };
                if let Some(s) = v.and_then(Json::as_str) {
                    return Some(s.to_string());
                }
            }
            None
        })
        .expect("hh run start printed no run_id");
    assert!(
        !text.contains("\"detached\":\"parked\""),
        "the hh run parked instead of finishing: {text}"
    );
    run
}

/// A read-only attach session onto an existing run over `call` —
/// `attach`, falling back to the writer `resume` when the boundary
/// refuses the read-only leg for a verify/replay op.
fn attach_session(call: &mut Call, run: &str, label: &str) -> String {
    let s = call(
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run.to_string())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str(format!("k-attach-{run}"))),
        ]),
    );
    let s = match s {
        Ok(s) => s,
        Err(e1) => call(
            "open_session",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("resume")),
                        ("run_id", Json::str(run.to_string())),
                        ("mode", Json::str("continue")),
                    ]),
                ),
                ("idempotency_key", Json::str(format!("k-resume-{run}"))),
            ]),
        )
        .unwrap_or_else(|e2| panic!("{label}: attach {e1} / resume {e2}")),
    };
    s.get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

// ── the legs ────────────────────────────────────────────────────────────

/// `capstone-run-{local,anthropic,openai}` — the reference task over all
/// three bindings, the `hh` binary included.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh capstone-run-<provider>"]
fn l46_live_reference_task() {
    let Some(l) = lane() else {
        eprintln!("HH_CAPSTONE_PROVIDER unset — skipped");
        return;
    };
    if !provisioned(&l) {
        eprintln!("unprovided — skipped");
        return;
    }
    let provider = std::env::var("HH_CAPSTONE_PROVIDER").unwrap();
    let doc = document_json(&l);
    let secret = l
        .credential_env
        .and_then(|n| std::env::var(n).ok())
        .filter(|v| !v.is_empty());
    let mut usages: Vec<Json> = Vec::new();
    let mut served_model: Option<String> = None;

    // ── binding (a): the in-process EmbedService ──────────────────────
    {
        let dir = tmpdir("a");
        let store = dir.join("store");
        let mut svc = EmbedService::open(ServiceConfig {
            store_root: store.clone(),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: dir.join("ws"),
            holder: "l4.6-live".into(),
        })
        .unwrap();
        let mut call = |m: &str, p: Json| -> Result<Json, String> {
            let r = svc.handle(&Request {
                id: Json::str(format!("a-{m}")),
                method: m.into(),
                params: p,
            });
            r.get("result")
                .cloned()
                .ok_or_else(|| r.to_canonical_string())
        };
        let (sess, run, events) = drive(&mut call, doc.clone(), "binding-a", "a");
        let (u, sm) = assert_live_run(&mut call, &sess, &events, &l, "binding-a");
        if let Some(s) = &secret {
            leak_sweep(&events, s, &store, "binding-a");
        }
        println!("CAPSTONE_RUN: provider={provider} binding=in-process run={run}");
        if !matches!(u, Json::Null) {
            usages.push(u);
        }
        served_model = sm.or(served_model);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── binding (b) via the `hh` CLI binary — the shipped surface ─────
    let hh_dir = tmpdir("hh");
    let hh_store = hh_dir.join("store");
    let hh_ws = hh_dir.join("ws");
    let def = hh_dir.join("def.json");
    std::fs::write(&def, doc.to_canonical_string()).unwrap();
    let cli_run = hh_run_start(&def, &hh_store, &hh_ws, &hh_dir);
    {
        let (ok_, text) = hh_run(
            &["run", "status", &cli_run, "--format", "json"],
            &hh_store,
            &hh_ws,
            &hh_dir,
        );
        assert!(ok_, "hh run status failed: {text}");
        let (ok_, text) = hh_run(
            &["run", "events", &cli_run, "--format", "json"],
            &hh_store,
            &hh_ws,
            &hh_dir,
        );
        assert!(ok_, "hh run events failed: {text}");
        assert!(text.contains("through_seq"), "events output: {text}");
        println!("CAPSTONE_RUN: provider={provider} binding=hh-cli run={cli_run}");
    }

    // ── binding (c): `hh-kernel serve --http` + the generated client ──
    // Two arms on the same client path: a fresh reference-task run
    // end-to-end, then a read-only attach onto the `hh` CLI run's store
    // — verify + replay it byte-identically cross-binding.
    {
        let dir = tmpdir("c");
        let (mut child, addr, token) = spawn_http(&dir.join("store"), &dir.join("ws"));
        let mut net = hharness_embed_client_generated::NetClient::connect(addr, &token);
        let mut call = |m: &str, p: Json| net.call(m, p).map_err(|e| format!("{e:?}"));
        let (sess, run, events) = drive(&mut call, doc.clone(), "binding-c", "c");
        let (u, sm) = assert_live_run(&mut call, &sess, &events, &l, "binding-c");
        println!("CAPSTONE_RUN: provider={provider} binding=http run={run}");
        if !matches!(u, Json::Null) {
            usages.push(u);
        }
        served_model = sm.or(served_model);
        let _ = child.kill();
        let _ = child.wait();

        let (mut child, addr, token) = spawn_http(&hh_store, &hh_ws);
        let mut net = hharness_embed_client_generated::NetClient::connect(addr, &token);
        let mut call = |m: &str, p: Json| net.call(m, p).map_err(|e| format!("{e:?}"));
        call("hello", hello_params()).unwrap();
        let sess = attach_session(&mut call, &cli_run, "hh-cli-attach");
        let events = read_all(&mut call, &sess, "hh-cli-attach");
        let (u, _) = assert_live_run(&mut call, &sess, &events, &l, "hh-cli-attach");
        if let Some(s) = &secret {
            leak_sweep(&events, s, &hh_store, "hh-cli-attach");
        }
        println!("CAPSTONE_RUN: provider={provider} binding=hh-cli-verify run={cli_run}");
        if !matches!(u, Json::Null) {
            usages.push(u);
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }
    let _ = std::fs::remove_dir_all(&hh_dir);

    // Aggregated hosted usage → the leg's measured spend. The local
    // lane's rows are the declared zero-spend arm and print nothing.
    if l.credential_env.is_some() {
        let (mut i, mut o, mut n) = (0i64, 0i64, 0i64);
        for u in &usages {
            i += u.get("input_total").and_then(Json::as_int).unwrap_or(0);
            o += u.get("output_total").and_then(Json::as_int).unwrap_or(0);
            n += 1;
        }
        println!(
            "LIVE_USAGE: {}",
            Json::obj([
                ("input_total", Json::Int(i)),
                ("output_total", Json::Int(o)),
                ("calls", Json::Int(n)),
            ])
            .to_canonical_string()
        );
        if let Some(m) = served_model {
            println!("LIVE_MODEL: {m}");
        }
    }
}

/// `capstone-ledger-replay` — an `hh`-CLI local run verified + replayed
/// byte-identically over binding (c), no provider re-call.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh capstone-ledger-replay"]
fn l46_live_ledger_replay() {
    if std::env::var("HH_LOCAL_BASE_URL").is_err() || std::env::var("HH_LOCAL_MODEL").is_err() {
        eprintln!("HH_LOCAL_BASE_URL/HH_LOCAL_MODEL unprovided — skipped");
        return;
    }
    let doc = document_json(&local_lane());
    let dir = tmpdir("replay");
    let store = dir.join("store");
    let ws = dir.join("ws");
    let def = dir.join("def.json");
    std::fs::write(&def, doc.to_canonical_string()).unwrap();
    let run = hh_run_start(&def, &store, &ws, &dir);

    let (mut child, addr, token) = spawn_http(&store, &ws);
    let mut net = hharness_embed_client_generated::NetClient::connect(addr, &token);
    let mut call = |m: &str, p: Json| net.call(m, p).map_err(|e| format!("{e:?}"));
    call("hello", hello_params()).unwrap();
    let sess = attach_session(&mut call, &run, "replay-leg");
    let events = read_all(&mut call, &sess, "replay-leg");
    let l = local_lane();
    let _ = assert_live_run(&mut call, &sess, &events, &l, "replay-leg");
    println!("CAPSTONE_REPLAY: run={run} verify=ok replay=deterministic byte-identical");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `capstone-web` — `hh-web` serving the live run over binding (c); the
/// HTTP read is asserted (shell + `run_index` carrying the run id).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh capstone-web"]
fn l46_live_web() {
    if std::env::var("HH_LOCAL_BASE_URL").is_err() || std::env::var("HH_LOCAL_MODEL").is_err() {
        eprintln!("HH_LOCAL_BASE_URL/HH_LOCAL_MODEL unprovided — skipped");
        return;
    }
    let doc = document_json(&local_lane());
    let dir = tmpdir("web");
    let store = dir.join("store");
    let ws = dir.join("ws");
    let def = dir.join("def.json");
    std::fs::write(&def, doc.to_canonical_string()).unwrap();
    let run = hh_run_start(&def, &store, &ws, &dir);

    let (mut kernel, kaddr, ktoken) = spawn_http(&store, &ws);
    // `hh-web` on its own loopback port; the surface token passed by
    // value (P9 — never a URL, never stdout; the leg is loopback-only).
    let wport = free_port();
    let waddr: SocketAddr = format!("127.0.0.1:{wport}").parse().unwrap();
    let wtoken = format!(
        "l46{:016x}{:016x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut cmd = Command::new(bin("hh-web"));
    cmd.args([
        "--kernel",
        &kaddr.to_string(),
        "--kernel-token",
        &ktoken,
        "--bind",
        &waddr.to_string(),
        "--token",
        &wtoken,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let mut web = cmd.spawn().expect("spawn hh-web");
    for _ in 0..100 {
        if std::net::TcpStream::connect(waddr).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // GET / — the public shell answers 200.
    let (status, body) = http_req(&waddr, "GET", "/", &[("host", &waddr.to_string())], "");
    assert_eq!(status, 200, "GET / shell: {status}");
    assert!(body.contains("<html"), "shell html: {}", body.len());

    // POST /api — the read op over the live store; `run_index` carries
    // the run we just drove.
    let (status, body) = http_req(
        &waddr,
        "POST",
        "/api",
        &[
            ("host", &waddr.to_string()),
            ("authorization", &format!("Bearer {wtoken}")),
            ("x-hh-script", "1"),
            ("content-type", "application/json"),
        ],
        &Json::obj([("op", Json::str("run_index")), ("params", Json::obj([]))])
            .to_canonical_string(),
    );
    assert_eq!(status, 200, "POST /api run_index: {status} {body}");
    assert!(
        body.contains(&run),
        "run_index does not name the live run {run}: {body}"
    );
    println!("CAPSTONE_WEB: run={run} shell=200 api=200 run_index=carries-run");
    let _ = web.kill();
    let _ = web.wait();
    let _ = kernel.kill();
    let _ = kernel.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Minimal HTTP/1.1 request over a plain TcpStream (loopback only).
fn http_req(
    addr: &SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    let mut req = format!("{method} {target} HTTP/1.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    let status: u16 = resp
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(resp.len());
    (status, resp[body_start..].to_string())
}

/// `capstone-bundle-repro` — bundle export of the live run, reproduced
/// on a clean store at the bundle's max supported level.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh capstone-bundle-repro"]
fn l46_live_bundle_repro() {
    if std::env::var("HH_LOCAL_BASE_URL").is_err() || std::env::var("HH_LOCAL_MODEL").is_err() {
        eprintln!("HH_LOCAL_BASE_URL/HH_LOCAL_MODEL unprovided — skipped");
        return;
    }
    let doc = document_json(&local_lane());
    let dir = tmpdir("bundle");
    let store = dir.join("store");
    let ws = dir.join("ws");
    let def = dir.join("def.json");
    std::fs::write(&def, doc.to_canonical_string()).unwrap();
    let run = hh_run_start(&def, &store, &ws, &dir);

    // `hh bundle create <run> --sink <dir>` — the bundle lands on disk.
    let sink = dir.join("bundle");
    let (ok_, text) = hh_run(
        &[
            "bundle",
            "create",
            &run,
            "--sink",
            sink.to_str().unwrap(),
            "--format",
            "json",
        ],
        &store,
        &ws,
        &dir,
    );
    assert!(ok_, "hh bundle create failed: {text}");
    assert!(sink.exists(), "bundle sink absent: {text}");

    // `hh bundle validate <dir>` — the staged gate; read
    // max_supported_level for the reproduce leg.
    let (ok_, text) = hh_run(
        &[
            "bundle",
            "validate",
            sink.to_str().unwrap(),
            "--format",
            "json",
        ],
        &store,
        &ws,
        &dir,
    );
    assert!(ok_, "hh bundle validate failed: {text}");
    eprintln!("bundle validate → {}", text.lines().last().unwrap_or(""));
    let max_level = text
        .lines()
        .filter_map(|l| hharness_wire::json::parse(l).ok())
        .filter_map(|j| {
            let body = j.get("payload").cloned().unwrap_or(j);
            body.get("max_supported_level")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .next()
        .unwrap_or_else(|| "R0".to_string());

    // Reproduce on a CLEAN store — the bundle is the only evidence.
    let clean = dir.join("clean-store");
    let (ok_, text) = hh_run(
        &[
            "bundle",
            "reproduce",
            sink.to_str().unwrap(),
            "--level",
            &max_level,
            "--format",
            "json",
        ],
        &clean,
        &ws,
        &dir,
    );
    assert!(ok_, "hh bundle reproduce failed: {text}");
    eprintln!("bundle reproduce → {}", text.lines().last().unwrap_or(""));
    assert!(
        text.contains("\"outcome\":\"pass\"") || text.contains("\"verdict\":\"reproduced\""),
        "reproduce verdict not reproduced: {text}"
    );
    println!("CAPSTONE_BUNDLE: run={run} level={max_level} verdict=reproduced clean-store");
    let _ = std::fs::remove_dir_all(&dir);
}
