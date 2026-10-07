//! S5.8 — the `control.wakeup.*` → `notifications/tasks` lowering and
//! the run-state → tasks push arm of §05d (R-2.2.3²; ADR-0175 D3;
//! AC-R-2.2.3-13's surface leg).
//!
//! Coverage map:
//! - `notifications/tasks` pushes for a task-bound run after a durable
//!   change — narrowed (`taskId` only, the client re-reads
//!   `tasks/get`), binding-scoped (a foreign binding's call touching
//!   the same run pushes nothing), and never emitted to a client that
//!   did not declare `capabilities.tasks`.
//! - the wakeup drain: a due `control.wakeup` timer on a task-bound
//!   run's writer session is durable (`control.wakeup.occurred` +
//!   `control.wakeup.fired` in the record) *before* the surface sees
//!   the delivery, and the `(subscription, occurrence)` dedup key
//!   suppresses the second drain — the same `delivered_wokens` set the
//!   run-loop drain uses.
//! - a not-yet-due timer pushes nothing (no fabricated delivery).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_mcp_lab::binding::{stdio_launch_binding, CallerBinding, CallerKind};
use hh_mcp_lab::server::LabServer;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixtures (same shape as the s5_7 battery) ──────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-s58-{}-{tag}-{}",
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
        holder: "s5-8-lab".into(),
    })
    .unwrap()
}

fn lab() -> LabServer {
    LabServer::with_default_exposure(service())
}

fn agent() -> CallerBinding {
    stdio_launch_binding("test-agent", CallerKind::Agent)
}

fn human() -> CallerBinding {
    stdio_launch_binding("test-human", CallerKind::HumanPrincipal)
}

// ── the hir/1 fixture document (verbatim the s5_7 shape) ────────────

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

// ── protocol helpers ───────────────────────────────────────────────

fn msg(id: i64, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(id)),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

fn call(srv: &mut LabServer, binding: &CallerBinding, name: &str, arguments: Json) -> Json {
    call_meta(srv, binding, name, arguments, Json::Null)
}

fn call_meta(
    srv: &mut LabServer,
    binding: &CallerBinding,
    name: &str,
    arguments: Json,
    meta: Json,
) -> Json {
    let mut p = Json::obj([("name", Json::str(name)), ("arguments", arguments)]);
    if let (Json::Obj(m), Json::Obj(mm)) = (&mut p, &meta) {
        m.insert("_meta".into(), Json::Obj(mm.clone()));
    }
    let frame = srv
        .handle_message(binding, &msg(7, "tools/call", p))
        .expect("request frame");
    frame.get("result").cloned().unwrap_or(Json::obj([(
        "__error",
        frame.get("error").cloned().unwrap_or(Json::Null),
    )]))
}

fn request(
    srv: &mut LabServer,
    binding: &CallerBinding,
    id: i64,
    method: &str,
    params: Json,
) -> Json {
    srv.handle_message(binding, &msg(id, method, params))
        .expect("request frame")
}

/// Drain `srv.pending` — the notification sink a transport would
/// flush after each request.
fn listen(srv: &mut LabServer, binding: &CallerBinding) -> Vec<Json> {
    let r = request(srv, binding, 90, "subscriptions/listen", Json::obj([]));
    r.get("result")
        .and_then(|r| r.get("notifications"))
        .and_then(|v| match v {
            Json::Arr(x) => Some(x.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The `notifications/tasks` frames in a drained set — narrowed: a
/// `taskId` param and nothing else the client could mistake for a
/// re-computed status.
fn task_pushes(notes: &[Json]) -> Vec<String> {
    notes
        .iter()
        .filter(|n| n.get("method").and_then(Json::as_str) == Some("notifications/tasks"))
        .filter_map(|n| {
            let p = n.get("params")?;
            // Narrowed carrier — `taskId` is the only member.
            if let Json::Obj(m) = p {
                assert!(
                    m.keys().all(|k| k == "taskId"),
                    "notifications/tasks is narrowed: {p:?}"
                );
            }
            p.get("taskId")
                .and_then(Json::as_str)
                .map(|s| s.to_string())
        })
        .collect()
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

fn init_tasks(srv: &mut LabServer, binding: &CallerBinding) {
    let _ = request(
        srv,
        binding,
        1,
        "initialize",
        Json::obj([
            ("protocolVersion", Json::str("2026-07-28")),
            ("capabilities", Json::obj([("tasks", Json::obj([]))])),
        ]),
    );
}

/// A task-declared launch — returns `(task_id, run_id, writer
/// session_id)`. The `CreateTaskResult` replaces the launch payload,
/// so the run id parses off `task:run:<id>` and the writer session
/// comes from the launch-minted handle record (the same lookup the
/// drain performs).
fn launch_tasked(
    srv: &mut LabServer,
    binding: &CallerBinding,
    idem: &str,
) -> (String, String, String) {
    let r = call_meta(
        srv,
        binding,
        "launch_run",
        launch_args(idem),
        Json::obj([("io.modelcontextprotocol/tasks", Json::obj([]))]),
    );
    let task_id = r
        .get("task")
        .and_then(|t| t.get("taskId"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("CreateTaskResult.task.taskId: {r:?}"))
        .to_string();
    let run_id = task_id
        .strip_prefix("task:run:")
        .expect("a launch task names task:run:<id>")
        .to_string();
    let session_id = srv
        .sessions
        .get(&binding.binding_id)
        .and_then(|s| {
            s.handles.by_alias.values().find_map(|e| {
                if e.target_id != run_id {
                    return None;
                }
                e.payload
                    .as_ref()
                    .and_then(|p| p.get("session_id"))
                    .and_then(Json::as_str)
                    .map(String::from)
            })
        })
        .expect("the launch mints a run handle recording the writer session_id");
    (task_id, run_id, session_id)
}

/// A direct `svc.handle` — the kernel-internal op path the drain and
/// the fixture's `subscribe` use (the lab's own ops never carry the
/// experimental capability, so the fixture re-helloes explicitly).
fn svc_call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn svc_ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

// ── tests ───────────────────────────────────────────────────────────

/// A durable change to a task-bound run pushes the owning binding's
/// narrowed `notifications/tasks` (R-2.2.3² — the run-state arm of the
/// §05d lowering).
#[test]
fn tasks_push_after_durable_run_change() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    let (task_id, run_id, _sid) = launch_tasked(&mut srv, &a, "push-1");
    let _ = listen(&mut srv, &a); // drain anything the launch queued

    let _ = call(
        &mut srv,
        &a,
        "submit_input",
        Json::obj([
            ("run", Json::str(run_id.clone())),
            ("content", Json::str("ping")),
        ]),
    );
    let pushes = task_pushes(&listen(&mut srv, &a));
    assert!(
        pushes.iter().any(|t| t == &task_id),
        "a durable run change pushes the task's narrowed note: {pushes:?}"
    );
}

/// The push is binding-scoped: a foreign tasks-capable binding that
/// touches the run pushes nothing for the owner's task (grants are the
/// binding's — never a broadcast).
#[test]
fn tasks_push_is_binding_scoped() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    let (task_id, run_id, _sid) = launch_tasked(&mut srv, &a, "push-2");
    let _ = listen(&mut srv, &a);

    let h = human();
    init_tasks(&mut srv, &h);
    // The human binding never launched — `submit_input` resolves no
    // writer session under its binding and refuses, but the touch set
    // still names the run; the push gate is `owner_binding`, so the
    // foreign call lands no `notifications/tasks` for `task_id`.
    let _ = call(
        &mut srv,
        &h,
        "submit_input",
        Json::obj([
            ("run", Json::str(run_id.clone())),
            ("content", Json::str("ping")),
        ]),
    );
    let notes = listen(&mut srv, &h);
    assert!(
        task_pushes(&notes).is_empty(),
        "a foreign binding's call pushes nothing for the owner's task: {notes:?}"
    );
    // The owner still polls its real status — the durable row is the
    // record, never the push.
    let g = request(
        &mut srv,
        &a,
        12,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    assert_eq!(
        g.get("result")
            .and_then(|r| r.get("status"))
            .and_then(Json::as_str),
        Some("working"),
        "{g:?}"
    );
}

/// The wakeup lowering: a due `control.wakeup` timer on the task-bound
/// run's writer session lands `occurred`/`fired` durable-first, then
/// pushes the task's `notifications/tasks` — once (the
/// `delivered_wokens` dedup key covers the second drain).
#[test]
fn due_wakeup_drains_to_tasks_push_durable_first() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    let (task_id, run_id, session_id) = launch_tasked(&mut srv, &a, "wake-1");
    let _ = listen(&mut srv, &a);

    // Subscribe the writer session to a timer already due — the
    // experimental op, so re-hello with the capability first (the
    // lab's internal hello never asks for it).
    svc_ok(&svc_call(
        &mut srv.svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("hh-mcp-lab")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("mcp_server")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![("experimental", Json::Bool(true))]),
            ),
        ]),
    ));
    let sub = svc_ok(&svc_call(
        &mut srv.svc,
        "subscribe",
        Json::obj([
            ("session_id", Json::str(session_id.clone())),
            (
                "trigger",
                Json::obj([("type", Json::str("timer")), ("at_ms", Json::Int(0))]),
            ),
        ]),
    ));
    assert!(sub.get("subscription_id").is_some(), "{sub:?}");

    // Any `tasks/*` call drains first — the due occurrence fires and
    // the task's push queues.
    let _ = request(
        &mut srv,
        &a,
        20,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    let pushes = task_pushes(&listen(&mut srv, &a));
    assert!(
        pushes.iter().any(|t| t == &task_id),
        "the fired wakeup pushes the task's narrowed note: {pushes:?}"
    );

    // Durable-first — the `occurred`/`fired` rows are in the record
    // before the surface ever saw the delivery.
    let page = svc_ok(&svc_call(
        &mut srv.svc,
        "read",
        Json::obj([
            ("session_id", Json::str(session_id.clone())),
            (
                "cursor",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(1))]),
            ),
            ("direction", Json::str("fwd")),
            ("limit", Json::Int(512)),
        ]),
    ));
    let events = match page.get("events") {
        Some(Json::Arr(v)) => v.clone(),
        _ => Vec::new(),
    };
    for want in ["control.wakeup.occurred", "control.wakeup.fired"] {
        assert!(
            events
                .iter()
                .any(|e| e.get("class").and_then(Json::as_str) == Some(want)),
            "{want} durable on {run_id}: {events:?}"
        );
    }

    // Dedup — a second drain delivers nothing fresh, so no second push.
    let _ = request(
        &mut srv,
        &a,
        21,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    let pushes = task_pushes(&listen(&mut srv, &a));
    assert!(
        pushes.is_empty(),
        "the (subscription, occurrence) dedup key suppresses the redelivery: {pushes:?}"
    );
}

/// A timer that is not yet due drains nothing and pushes nothing — no
/// fabricated delivery on a surface poll.
#[test]
fn future_wakeup_pushes_nothing() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    let (task_id, _run_id, session_id) = launch_tasked(&mut srv, &a, "wake-2");
    let _ = listen(&mut srv, &a);

    svc_ok(&svc_call(
        &mut srv.svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("hh-mcp-lab")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("mcp_server")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![("experimental", Json::Bool(true))]),
            ),
        ]),
    ));
    svc_ok(&svc_call(
        &mut srv.svc,
        "subscribe",
        Json::obj([
            ("session_id", Json::str(session_id)),
            (
                "trigger",
                Json::obj([
                    ("type", Json::str("timer")),
                    // Far past any wall clock this test process sees.
                    ("at_ms", Json::Int(4_000_000_000_000)),
                ]),
            ),
        ]),
    ));

    let _ = request(
        &mut srv,
        &a,
        30,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    let pushes = task_pushes(&listen(&mut srv, &a));
    assert!(
        pushes.is_empty(),
        "a not-yet-due timer never fabricates a push: {pushes:?}"
    );
}
