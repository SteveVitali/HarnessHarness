//! L4.6b — `replay{driver_mode: deterministic}` over an effect-dispatching
//! run reproduces byte-identically (leg (a), ADR-0381): the recorded §5a.2
//! dispatch ceremony — `security.permission.decided`,
//! `action.effect.{authorized,prepared,committed}` — re-serves verbatim
//! through `ReplayGate`'s `GateOutcome.emitted`, so the driver's shared
//! `next_id` allocator mints the same `e-N`s the record holds and every
//! later `mc-N`/`d-N`/`e-N` matches. Pre-fix the re-drive emitted no
//! ceremony row, skewed the allocator ~4 ids per dispatch, and honestly
//! answered `invalid{replay_diverged}` (the capstone's 3/3 finding on the
//! anthropic lane — its `hh.submit` completion dispatch carried the same
//! 4-row write-ahead chain).
//!
//! `l46b_replay_effect_run` (ignored) is the live-evidence leg
//! (`scripts/live-verify.sh l4-6b-replay`): a dispatching record produced
//! over binding (c) (`hh-kernel serve --http` + the scripted lane, USD 0,
//! no provider), replayed `deterministic` from a read-only attach session
//! on the same store — the focused equivalent of the
//! `capstone-run-anthropic` hh-cli-attach arm that diverged.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
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

// ── the definition document (the conformance fixture, `hh/round_robin`) ──

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

/// The conformance document — `control_strategy → hh/round_robin` (the
/// canonical `react/minimal` interpreter; `deterministic_replay`
/// declared — the variant the capstone arms bound).
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
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1000),
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

fn spec() -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
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

// ── service + request helpers ────────────────────────────────────────────

fn test_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-l46b-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service_in(dir: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: dir.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: dir.join("ws"),
        holder: "l46b".into(),
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

/// `EmbedService` behind the same `FnMut(&str, Json) -> Result<Json, String>`
/// signature the `NetClient` closure uses — one drive/assert pair serves
/// both bindings.
fn svc_call(svc: &mut EmbedService, m: &str, p: Json) -> Result<Json, String> {
    let r = call(svc, m, p);
    r.get("result")
        .cloned()
        .ok_or_else(|| r.to_canonical_string())
}

fn caps_json(extra: &[(&str, bool)]) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in extra {
        m.insert((*k).to_string(), Json::Bool(*v));
    }
    Json::Obj(m)
}

fn hello_params() -> Json {
    Json::obj([
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj([
                ("name", Json::str("l46b")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        (
            "capabilities",
            caps_json(&[
                ("serves_host_executor", true),
                ("serves_permission_channel", true),
                ("accepts_ephemeral_frames", true),
                ("experimental", true),
            ]),
        ),
    ])
}

/// The shared drive — one `submit` whose scripted model proposes
/// `hh.submit` (the `invoke` block's surface): the gate dispatches the
/// §5a.2 write-ahead ceremony and the observed terminal in-loop, then
/// the `stop` decision mints — the post-ceremony id the pre-fix skew
/// corrupted. Same calls over `EmbedService` (a) or `NetClient` (c).
fn drive_dispatching_run<C: FnMut(&str, Json) -> Result<Json, String>>(
    call: &mut C,
    label: &str,
) -> (String, String) {
    call("hello", hello_params()).unwrap_or_else(|e| panic!("{label}: hello: {e}"));
    let s = call(
        "open_session",
        Json::obj([
            ("spec", spec()),
            ("idempotency_key", Json::str("open-l46b")),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: open_session: {e}"));
    let sess = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    call(
        "submit",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("hh.submit")),
                    ("args", Json::obj([])),
                ])]),
            ),
            ("idempotency_key", Json::str("submit-1")),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: submit: {e}"));
    (sess, run)
}

/// The `read` op's seq-cursor page — the durable prefix over any binding.
fn read_prefix<C: FnMut(&str, Json) -> Result<Json, String>>(
    call: &mut C,
    sess: &str,
    label: &str,
) -> Vec<Json> {
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
        if let Some(Json::Arr(evs)) = page.get("events") {
            let n = evs.len();
            out.extend(evs.iter().cloned());
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

/// The 13-row dispatch ceremony the capstone surfaced: the
/// `action.tool.proposed` ask plus the contiguous 12-row pair of §5a.2
/// write-ahead chains — `action.effect.intended` →
/// `security.permission.decided` → `action.effect.{authorized,prepared,
/// committed}` → `action.effect.observed`, once per dispatched effect —
/// and the verification/lifecycle rows the run seals with.
fn assert_dispatch_ceremony(classes: &[&str], label: &str) {
    assert!(
        classes.contains(&"action.tool.proposed"),
        "{label}: the record carries the dispatching ask: {classes:?}"
    );
    const CHAIN: &[&str] = &[
        "action.effect.intended",
        "security.permission.decided",
        "action.effect.authorized",
        "action.effect.prepared",
        "action.effect.committed",
        "action.effect.observed",
    ];
    let pair: Vec<&str> = CHAIN.iter().chain(CHAIN.iter()).copied().collect();
    assert!(
        classes.windows(pair.len()).any(|w| w == pair.as_slice()),
        "{label}: the record carries the contiguous 12-row ceremony \
         (two write-ahead chains): {classes:?}"
    );
    for class in ["verification.gate.evaluated", "lifecycle.run.finished"] {
        assert!(
            classes.contains(&class),
            "{label}: the record carries {class}: {classes:?}"
        );
    }
}

/// `replay{driver_mode: deterministic}` — the verdict assertions the
/// capstone's AC carries: `deterministic`, no `replay_diverged` reason.
fn assert_deterministic_replay<C: FnMut(&str, Json) -> Result<Json, String>>(
    call: &mut C,
    sess: &str,
    label: &str,
) {
    let rep = call(
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.to_string())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    )
    .unwrap_or_else(|e| panic!("{label}: replay: {e}"));
    let report = rep
        .get("report")
        .unwrap_or_else(|| panic!("{label}: replay report: {}", rep.to_canonical_string()));
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "{label}: deterministic replay over a dispatching record: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "{label}: replay carries no divergence reasons: {}",
        rep.to_canonical_string()
    );
}

/// The regression cell — the dispatching run replays `deterministic`
/// with no divergence reasons (AC: byte-identical offline replay of an
/// effect-dispatching run; fails loudly without the L4.6b re-serve —
/// the pre-fix allocator skew reported `replay_diverged` here). A
/// read-only attach session asks the replay — the capstone's diverged
/// arm was exactly this shape (attach replay of a recorded ledger).
#[test]
fn l46b_effect_dispatch_replay_byte_identical() {
    let dir = test_dir("inproc");
    let mut svc = service_in(&dir);
    let (sess, run) = drive_dispatching_run(&mut |m, p| svc_call(&mut svc, m, p), "inproc");
    {
        let evs = svc.store().envelopes(&run).unwrap();
        let classes: Vec<&str> = evs.iter().map(|e| e.class.as_str()).collect();
        assert_dispatch_ceremony(&classes, "inproc");
    }
    // The replay must mint zero provider/host rows — only the replay
    // lifecycle markers land (the capstone's assert_live_run check).
    let model_rows = |svc: &EmbedService| {
        svc.store()
            .envelopes(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class.starts_with("model.") || e.class.starts_with("action.effect."))
            .count()
    };
    let before = model_rows(&svc);
    // The writer session replays its own run.
    assert_deterministic_replay(&mut |m, p| svc_call(&mut svc, m, p), &sess, "inproc");
    // And a read-only attach session answers the same verdict — the
    // diverged arm's shape (cross-binding replay of the recorded ledger).
    let s = svc_call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run.clone())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str("k-attach")),
        ]),
    )
    .unwrap_or_else(|e| panic!("inproc: attach: {e}"));
    let attach = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert_deterministic_replay(
        &mut |m, p| svc_call(&mut svc, m, p),
        &attach,
        "inproc-attach",
    );
    assert_eq!(
        model_rows(&svc),
        before,
        "inproc: replay minted no model/effect rows — the re-serve never dials"
    );

    // ── the mutation arm — a corrupted ledger still answers
    // `invalid{replay_diverged}`, never a silent pass (the honest-
    // refusal half of the AC). The boundary is append-only by contract,
    // so the corruption is applied to the durable WAL bytes directly —
    // a mid-record payload edit that leaves the seq/`prev_hash` links
    // untouched. A fresh service cold-loads the tampered record and the
    // re-driven `control.decision` (minted fresh by the replay driver,
    // never re-served) no longer matches the recorded row.
    drop(svc);
    let wal = dir.join("store").join("runs").join(&run).join("events.wal");
    let text = std::fs::read_to_string(&wal).unwrap();
    let mut mutated = false;
    let mut rewritten: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut line = line.to_string();
        if !mutated {
            if let Ok(j) = hharness_wire::json::parse(&line) {
                if let Some(v) = j.get("v") {
                    if v.get("class").and_then(Json::as_str) == Some("control.decision") {
                        if let (Json::Obj(mut top), Json::Obj(mut vm)) = (j.clone(), v.clone()) {
                            if let Some(Json::Obj(mut p)) = vm.get("payload").cloned() {
                                p.insert("tampered".to_string(), Json::Bool(true));
                                vm.insert("payload".to_string(), Json::Obj(p));
                                top.insert("v".to_string(), Json::Obj(vm));
                                line = Json::Obj(top).to_canonical_string();
                                mutated = true;
                            }
                        }
                    }
                }
            }
        }
        rewritten.push(line);
    }
    assert!(mutated, "inproc: no control.decision row to tamper");
    std::fs::write(&wal, format!("{}\n", rewritten.join("\n"))).unwrap();

    let mut svc = service_in(&dir);
    svc_call(&mut svc, "hello", hello_params()).unwrap_or_else(|e| panic!("mutated: hello: {e}"));
    let s = svc_call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run.clone())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str("k-attach-mutated")),
        ]),
    )
    .unwrap_or_else(|e| panic!("mutated: attach: {e}"));
    let attach = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let rep = svc_call(
        &mut svc,
        "replay",
        Json::obj([
            ("session_id", Json::str(attach)),
            ("driver_mode", Json::str("deterministic")),
        ]),
    )
    .unwrap_or_else(|e| panic!("mutated: replay: {e}"));
    let report = rep
        .get("report")
        .unwrap_or_else(|| panic!("mutated: replay report: {}", rep.to_canonical_string()));
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("invalid"),
        "mutated: a corrupted record must answer invalid: {}",
        rep.to_canonical_string()
    );
    let reasons = report.get("reasons").cloned().unwrap_or(Json::Arr(vec![]));
    assert!(
        matches!(&reasons, Json::Arr(rs) if rs.iter().any(|r| r.as_str() == Some("replay_diverged"))),
        "mutated: the divergence reason survives: {}",
        rep.to_canonical_string()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── the live leg (USD 0 — scripted lane, no provider) ────────────────────

fn bin(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dbg = exe.parent().unwrap().parent().unwrap();
    let p = dbg.join(name);
    assert!(p.exists(), "{name} not built — the leg builds it first");
    p
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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

/// `l4-6b-replay` — the USD-0 focused equivalent of the
/// `capstone-run-anthropic` hh-cli-attach arm that answered
/// `invalid{replay_diverged}`: a dispatching record produced over
/// binding (c) itself (`hh-kernel serve --http` + the scripted
/// `EmbedModel` lane — no provider dial), then replayed `deterministic`
/// from a *read-only attach* session on the same store — cross-session
/// replay of the recorded ledger, the leg that diverged.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh l4-6b-replay"]
fn l46b_replay_effect_run() {
    let dir = test_dir("live");
    let store = dir.join("store");
    let ws = dir.join("ws");
    let (mut child, addr, token) = spawn_http(&store, &ws);
    let mut net = hharness_embed_client_generated::NetClient::connect(addr, &token);
    let mut c = |m: &str, p: Json| net.call(m, p).map_err(|e| format!("{e:?}"));
    let (_sess, run) = drive_dispatching_run(&mut c, "live");
    // The diverged arm: a fresh read-only attach session over binding (c)
    // asks `replay{deterministic}` of the recorded ledger.
    let s = c(
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run.clone())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str("k-attach-replay")),
        ]),
    )
    .unwrap_or_else(|e| panic!("live: replay attach: {e}"));
    let attach = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let events = read_prefix(&mut c, &attach, "live-attach");
    let classes: Vec<&str> = events
        .iter()
        .filter_map(|e| e.get("class").and_then(Json::as_str))
        .collect();
    assert_dispatch_ceremony(&classes, "live-attach");
    assert_deterministic_replay(&mut c, &attach, "live-attach");
    println!("L4_6B_REPLAY: run={run} attach={attach} replay=deterministic byte-identical");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
