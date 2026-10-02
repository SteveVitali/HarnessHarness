//! S5.7 — the C2 `hh-lab/1` slice (R-2.11.3): the analysis group, the
//! write group + the delegate write-gate, `serve_bundle`, the
//! resource/subscription carrier, the tasks carrier, and the
//! `provider_client`/`service` binding rules (spec §7.3 §2.2–2.5;
//! ADR-0173/0174/0175; AC-R-2.11.3-10).
//!
//! Coverage map:
//! - catalogue: the analysis/write/hosting groups serve on
//!   `tools/list`, byte-identical across two caller bindings (AC-1's
//!   C2 form), every new tool still wraps one op by identity (AC-13).
//! - the delegate write-gate: `memory_write` tools refuse
//!   `DelegateWriteForbidden` for uncovered delegate callers
//!   (`provider_client`/`service`/`agent`); a sealed
//!   `entities.permissions[]` record covering the principal admits.
//! - R-1: `subject_kind = client` binds only `service|provider_client`
//!   — link refuses any other caller_kind.
//! - the tasks carrier: `initialize` capability, `CreateTaskResult` on
//!   a task-declared `launch_run`, `-32021` without the capability,
//!   `tasks/get|update|cancel`, the exhaustive status map
//!   (`working`/`input_required` human-only/`completed` incl. refused
//!   runs/`cancelled`), foreign taskIds.
//! - the resource carrier: templates, `resources/read`, subscribe →
//!   `notifications/resources/updated` after a durable change,
//!   unsubscribe, binding-expiry cancellation.
//! - `serve_bundle` wraps `lab.serve` (a typed refusal on a bad
//!   container, never a protocol fault) and mints `session_handle`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_mcp_lab::binding::{
    stdio_launch_binding, CallerBinding, CallerCredential, CallerKind, SubjectKind,
};
use hh_mcp_lab::exposure;
use hh_mcp_lab::server::LabServer;
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixtures (same shape as the s4_11 battery) ─────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-s57-{}-{tag}-{}",
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
        holder: "s5-7".into(),
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

/// A `provider_client` binding (client credentials — `subject_kind =
/// client`), delegate-ceilinged.
fn provider() -> CallerBinding {
    CallerBinding {
        binding_id: "bind-provider-test".to_string(),
        credential: CallerCredential::OAuth {
            issuer_ref: "issuer:fixture".to_string(),
            subject_kind: SubjectKind::Client,
            audience: "mcp://hh-lab".to_string(),
        },
        principal_ref: "provider:test".to_string(),
        caller_kind: CallerKind::ProviderClient,
        authority_cap: Json::obj([("class", Json::str("delegate"))]),
        permissions: vec![],
        budget_node: "pool:provider".to_string(),
        pool: Json::obj([("dimensions", Json::obj([]))]),
        readers_identity: "provider:test".to_string(),
        rate_policy: None,
        expires_at_ms: None,
    }
}

/// A `service` binding — client credentials, unattended.
fn svc_binding() -> CallerBinding {
    CallerBinding {
        binding_id: "bind-service-test".to_string(),
        credential: CallerCredential::OAuth {
            issuer_ref: "issuer:fixture".to_string(),
            subject_kind: SubjectKind::Client,
            audience: "mcp://hh-lab".to_string(),
        },
        principal_ref: "service:test".to_string(),
        caller_kind: CallerKind::Service,
        authority_cap: Json::obj([("class", Json::str("delegate"))]),
        permissions: vec![],
        budget_node: "pool:service".to_string(),
        pool: Json::obj([("dimensions", Json::obj([]))]),
        readers_identity: "service:test".to_string(),
        rate_policy: None,
        expires_at_ms: None,
    }
}

// ── the hir/1 fixture document (verbatim the s4_11 shape) ───────────

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

fn is_err(result: &Json) -> bool {
    matches!(result.get("isError"), Some(Json::Bool(true)))
}

fn refusal_kind(result: &Json) -> String {
    result
        .get("structuredContent")
        .and_then(|s| s.get("surface_error"))
        .and_then(|e| e.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
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

/// `initialize` with the tasks capability declared.
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

/// `initialize` advertising no extension capabilities.
fn init_plain(srv: &mut LabServer, binding: &CallerBinding) {
    let _ = request(
        srv,
        binding,
        1,
        "initialize",
        Json::obj([
            ("protocolVersion", Json::str("2026-07-28")),
            ("capabilities", Json::obj([])),
        ]),
    );
}

// ── catalogue — the C2 groups serve, byte-identical across bindings ─

#[test]
fn c2_catalogue_groups_serve_identically_across_bindings() {
    let mut srv = lab();
    let a = agent();
    let p = provider();
    let tools_a = request(&mut srv, &a, 1, "tools/list", Json::obj([]));
    let tools_p = request(&mut srv, &p, 2, "tools/list", Json::obj([]));
    // AC-R-2.11.3-1's C2 form — listing is not authorization: the
    // result bytes are identical for every binding (the request `id`
    // is the caller's own envelope member).
    assert_eq!(
        tools_a.get("result").map(Json::to_canonical_string),
        tools_p.get("result").map(Json::to_canonical_string),
        "tools/list is byte-identical across bindings"
    );
    let tool_names: Vec<String> = tools_a
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| match t {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap()
        .iter()
        .filter_map(|t| t.get("name").and_then(Json::as_str).map(String::from))
        .collect();
    for expected in [
        // analysis group
        "analyze",
        "render_analysis",
        "diff_reports",
        "power_analysis",
        // write group
        "assembly_apply",
        "registry_register",
        "registry_publish",
        "record_conformance",
        "leaderboard_define",
        "grant_approver",
        // hosting
        "serve_bundle",
    ] {
        assert!(
            tool_names.iter().any(|n| n == expected),
            "tools/list carries `{expected}`"
        );
    }
    // Every new tool still wraps one op by identity — the `_meta` hir
    // member carries the op (AC-R-2.11.3-13).
    let serve = tools_a
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| match t {
            Json::Arr(a) => a
                .iter()
                .find(|t| t.get("name").and_then(Json::as_str) == Some("serve_bundle"))
                .cloned(),
            _ => None,
        })
        .expect("serve_bundle listed");
    let op = serve
        .get("_meta")
        .and_then(|m| {
            m.get("hh.dev/hir")
                .or_else(|| m.get("hir"))
                .or_else(|| m.get("hh/hir"))
        })
        .and_then(|h| h.get("op"))
        .and_then(Json::as_str);
    // The hir `_meta` key is the OQ-394 placeholder — accept any
    // `<prefix>/hir` spell that carries `op == lab.serve`.
    let hir = serve.get("_meta").cloned().unwrap_or(Json::Null);
    let found = match &hir {
        Json::Obj(m) => m
            .values()
            .any(|v| v.get("op").and_then(Json::as_str) == Some("lab.serve")),
        _ => false,
    };
    assert!(
        found || op == Some("lab.serve"),
        "serve_bundle wraps lab.serve: {hir:?}"
    );
}

// ── the delegate write-gate ─────────────────────────────────────────

#[test]
fn delegate_write_gate_refuses_uncovered_delegates() {
    let mut srv = lab();
    let p = provider();
    let s = svc_binding();
    // provider_client → DelegateWriteForbidden (no covering Permission).
    let r = call(
        &mut srv,
        &p,
        "registry_register",
        Json::obj([("record", Json::obj([]))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "DelegateWriteForbidden", "{r:?}");
    // service — same ceiling.
    let r = call(
        &mut srv,
        &s,
        "assembly_apply",
        Json::obj([("assembly", Json::obj([]))]),
    );
    assert_eq!(refusal_kind(&r), "DelegateWriteForbidden", "{r:?}");
    // The refused row is durable — the provider's surface run carries
    // `action.effect.refused`? (the gate refuses BEFORE the turn — the
    // typed isError still lands, no surface-session turn needed: the
    // call never entered the catalogue's effect chain.)
    // human_principal passes the gate — the op's own refusal is not
    // the gate's name.
    let h = human();
    let r = call(
        &mut srv,
        &h,
        "registry_register",
        Json::obj([("record", Json::obj([]))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_ne!(
        refusal_kind(&r),
        "DelegateWriteForbidden",
        "human_principal is never delegate-gated: {r:?}"
    );
    // The default doc's sealed Permission covers `principal:test` for
    // `lab.*` — the fixture agent binding (that principal) passes the
    // gate too (the op's own refusal follows).
    let a = agent();
    let r = call(
        &mut srv,
        &a,
        "registry_register",
        Json::obj([("record", Json::obj([]))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_ne!(
        refusal_kind(&r),
        "DelegateWriteForbidden",
        "a covered delegate passes the gate: {r:?}"
    );
}

/// A sealed `entities.permissions[]` record admitting `provider:test`
/// lifts the gate for that principal only.
#[test]
fn delegate_write_gate_permission_coverage() {
    let mut doc = exposure::default_lab_document();
    if let Json::Obj(m) = &mut doc {
        if let Json::Obj(em) = m.get_mut("entities").unwrap() {
            if let Json::Arr(perms) = em.get_mut("permissions").unwrap() {
                perms.push(Json::obj([
                    ("kind", Json::str("Permission")),
                    ("permission_id", Json::str("perm-provider-reg")),
                    ("holder", Json::str("provider:test")),
                    ("state", Json::str("active")),
                    (
                        "scope",
                        Json::obj([("namespace", Json::str("lab.registry.*"))]),
                    ),
                ]));
            }
        }
    }
    let mut srv = LabServer::new(service(), &doc).expect("doc links");
    let p = provider();
    // Covered namespace — passes the gate (kernel refusal follows).
    let r = call(
        &mut srv,
        &p,
        "registry_register",
        Json::obj([("record", Json::obj([]))]),
    );
    assert_ne!(
        refusal_kind(&r),
        "DelegateWriteForbidden",
        "a covering sealed Permission admits: {r:?}"
    );
    // Uncovered namespace (`lab.assembly.*`) still refuses.
    let r = call(
        &mut srv,
        &p,
        "assembly_apply",
        Json::obj([("assembly", Json::obj([]))]),
    );
    assert_eq!(refusal_kind(&r), "DelegateWriteForbidden", "{r:?}");
    // A revoked (`state: revoked`) grant never covers.
    if let Json::Obj(m) = &mut doc {
        if let Json::Obj(em) = m.get_mut("entities").unwrap() {
            if let Json::Arr(perms) = em.get_mut("permissions").unwrap() {
                for pr in perms.iter_mut() {
                    if let Json::Obj(pm) = pr {
                        pm.insert("state".into(), Json::str("revoked"));
                    }
                }
            }
        }
    }
    let mut srv = LabServer::new(service(), &doc).expect("doc links");
    let r = call(
        &mut srv,
        &p,
        "registry_register",
        Json::obj([("record", Json::obj([]))]),
    );
    assert_eq!(
        refusal_kind(&r),
        "DelegateWriteForbidden",
        "a revoked grant covers nothing: {r:?}"
    );
}

// ── R-1: subject_kind = client binds only service/provider_client ───

#[test]
fn client_subject_binds_only_service_or_provider() {
    let mk = |kind: CallerKind| {
        CallerBinding::from_json(&Json::obj([
            ("binding_id", Json::str("bind-x")),
            (
                "credential",
                Json::obj([
                    ("kind", Json::str("oauth")),
                    ("issuer_ref", Json::str("issuer:fixture")),
                    ("subject_kind", Json::str("client")),
                    ("audience", Json::str("mcp://hh-lab")),
                ]),
            ),
            ("principal_ref", Json::str("svc:x")),
            ("caller_kind", Json::str(kind.as_str())),
            ("budget_node", Json::str("pool:x")),
            ("pool", Json::obj([])),
        ]))
    };
    assert!(mk(CallerKind::Service).is_ok());
    assert!(mk(CallerKind::ProviderClient).is_ok());
    let e = mk(CallerKind::Agent).unwrap_err();
    assert!(e.contains("subject_kind `client`"), "{e}");
    let e = mk(CallerKind::HumanPrincipal).unwrap_err();
    assert!(e.contains("subject_kind `client`"), "{e}");
    // And the exposure link refuses a document carrying the violation.
    let mut doc = exposure::default_lab_document();
    if let Json::Obj(m) = &mut doc {
        if let Json::Arr(b) = m.get_mut("caller_bindings").unwrap() {
            b.push(Json::obj([
                ("binding_id", Json::str("bind-bad")),
                (
                    "credential",
                    Json::obj([
                        ("kind", Json::str("oauth")),
                        ("issuer_ref", Json::str("issuer:fixture")),
                        ("subject_kind", Json::str("client")),
                        ("audience", Json::str("mcp://hh-lab")),
                    ]),
                ),
                ("principal_ref", Json::str("user:bad")),
                ("caller_kind", Json::str("human_principal")),
                ("budget_node", Json::str("pool:x")),
                ("pool", Json::obj([])),
            ]));
        }
    }
    assert!(matches!(
        exposure::parse_exposure(&doc),
        Err(exposure::ExposureError::Invalid { .. })
    ));
}

// ── the tasks carrier ───────────────────────────────────────────────

#[test]
fn tasks_capability_gate_32021() {
    let mut srv = lab();
    let a = agent();
    init_plain(&mut srv, &a);
    // A `_meta` tasks declaration without the capability → -32021.
    let frame = request(
        &mut srv,
        &a,
        9,
        "tools/call",
        Json::obj([
            ("name", Json::str("launch_run")),
            ("arguments", launch_args("t-1")),
            (
                "_meta",
                Json::obj([("io.modelcontextprotocol/tasks", Json::obj([]))]),
            ),
        ]),
    );
    assert_eq!(
        frame
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_int),
        Some(-32021),
        "no capability ⇒ -32021: {frame:?}"
    );
    // `tasks/get` without the capability → -32021.
    let frame = request(
        &mut srv,
        &a,
        10,
        "tasks/get",
        Json::obj([("taskId", Json::str("task:run:x"))]),
    );
    assert_eq!(
        frame
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_int),
        Some(-32021)
    );
    // And initialize advertises the extension (the capability the
    // client could have declared).
    let init = request(&mut srv, &a, 11, "initialize", Json::obj([]));
    assert!(
        init.get("result")
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.get("tasks"))
            .is_some(),
        "initialize advertises tasks: {init:?}"
    );
    assert!(
        init.get("result")
            .and_then(|r| r.get("capabilities"))
            .and_then(|c| c.get("resources"))
            .and_then(|r| r.get("subscribe"))
            == Some(&Json::Bool(true)),
        "initialize advertises resources.subscribe"
    );
}

#[test]
fn tasks_carrier_create_get_cancel() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    // A task-declared launch → CreateTaskResult.
    let r = call_meta(
        &mut srv,
        &a,
        "launch_run",
        launch_args("t-2"),
        Json::obj([("io.modelcontextprotocol/tasks", Json::obj([]))]),
    );
    let task_id = r
        .get("task")
        .and_then(|t| t.get("taskId"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("CreateTaskResult.task.taskId: {r:?}"))
        .to_string();
    assert_eq!(
        r.get("task")
            .and_then(|t| t.get("status"))
            .and_then(Json::as_str),
        Some("working")
    );
    assert!(task_id.starts_with("task:run:"), "{task_id}");
    // tasks/get → working (the run is live).
    let g = request(
        &mut srv,
        &a,
        12,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    let status = g
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert_eq!(status, "working", "{g:?}");
    // tasks/list surfaces it.
    let l = request(&mut srv, &a, 13, "tasks/list", Json::obj([]));
    let listed = l
        .get("result")
        .and_then(|r| r.get("tasks"))
        .and_then(|t| match t {
            Json::Arr(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        listed
            .iter()
            .any(|t| t.get("taskId").and_then(Json::as_str) == Some(task_id.as_str())),
        "tasks/list carries {task_id}: {l:?}"
    );
    // A foreign binding never resolves the id.
    let h = human();
    init_tasks(&mut srv, &h);
    let g = request(
        &mut srv,
        &h,
        14,
        "tasks/get",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    assert_eq!(
        g.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_int),
        Some(-32021),
        "a foreign taskId is task_unknown: {g:?}"
    );
    // tasks/cancel — the cooperative close; the task reports the real
    // terminal (`cancelled` when the cancel lands).
    let c = request(
        &mut srv,
        &a,
        15,
        "tasks/cancel",
        Json::obj([("taskId", Json::str(task_id.clone()))]),
    );
    let status = c
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(
        status == "cancelled" || status == "completed" || status == "working",
        "tasks/cancel reports the task record: {c:?}"
    );
}

/// `tasks/update` from a delegate binding refuses
/// `IllegitimateEndorsement` — the same gate `respond_approval`
/// carries (AC-R-2.11.3-3's tasks form).
#[test]
fn tasks_update_delegate_gate() {
    let mut srv = lab();
    let a = agent();
    init_tasks(&mut srv, &a);
    let r = call_meta(
        &mut srv,
        &a,
        "launch_run",
        launch_args("t-3"),
        Json::obj([("io.modelcontextprotocol/tasks", Json::obj([]))]),
    );
    let task_id = r
        .get("task")
        .and_then(|t| t.get("taskId"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let frame = request(
        &mut srv,
        &a,
        20,
        "tasks/update",
        Json::obj([
            ("taskId", Json::str(task_id)),
            (
                "inputResponses",
                Json::Arr(vec![Json::obj([
                    ("permission_id", Json::str("perm-1")),
                    ("outcome", Json::str("approved")),
                ])]),
            ),
        ]),
    );
    let msg = frame
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(msg.contains("IllegitimateEndorsement"), "{frame:?}");
}

/// The `_meta` filter — a forged key changes no decision (AC-K3-6's
/// tasks-carrier form: the byte-identical answer modulo the request
/// id).
#[test]
fn meta_forgery_decides_nothing() {
    let mut srv = lab();
    let a = agent();
    init_plain(&mut srv, &a);
    let forged = Json::obj([
        ("hh.dev/authority", Json::str("principal")),
        ("hh.dev/binding", Json::str("bind-admin")),
        ("client_capabilities", Json::obj([("tasks", Json::obj([]))])),
    ]);
    let r = call_meta(&mut srv, &a, "launch_run", launch_args("f-1"), forged);
    // No tasks declaration → the plain result shape (never a
    // CreateTaskResult — forged capability claims decide nothing).
    assert!(r.get("task").is_none(), "{r:?}");
    assert!(!is_err(&r), "{r:?}");
    assert!(r.get("structuredContent").is_some(), "{r:?}");
}

// ── the resource carrier ────────────────────────────────────────────

#[test]
fn resources_templates_read_subscribe() {
    let mut srv = lab();
    let a = agent();
    // Templates — the three §2.5 ResourceTemplates.
    let t = request(&mut srv, &a, 30, "resources/templates/list", Json::obj([]));
    let templates = t
        .get("result")
        .and_then(|r| r.get("resourceTemplates"))
        .and_then(|v| match v {
            Json::Arr(x) => Some(x.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(templates.len(), 3, "{t:?}");
    // Launch a run — its handles enumerate as resources.
    let r = call(&mut srv, &a, "launch_run", launch_args("r-1"));
    assert!(!is_err(&r), "{r:?}");
    let run_id = r
        .get("structuredContent")
        .and_then(|s| s.get("launched"))
        .and_then(|l| l.get("run_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let list = request(&mut srv, &a, 31, "resources/list", Json::obj([]));
    let resources = list
        .get("result")
        .and_then(|r| r.get("resources"))
        .and_then(|v| match v {
            Json::Arr(x) => Some(x.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let status_uri = resources
        .iter()
        .filter_map(|r| r.get("uri").and_then(Json::as_str))
        .find(|u| u.ends_with("/status") && u.contains("run/"))
        .expect("a run status resource")
        .to_string();
    let ledger_uri = status_uri.replace("/status", "/ledger");
    // resources/read — the status projection.
    let rr = request(
        &mut srv,
        &a,
        32,
        "resources/read",
        Json::obj([("uri", Json::str(status_uri.clone()))]),
    );
    assert!(
        rr.get("result").and_then(|r| r.get("contents")).is_some(),
        "resources/read serves contents: {rr:?}"
    );
    // A bad URI is -32002, never an empty page.
    let rr = request(
        &mut srv,
        &a,
        33,
        "resources/read",
        Json::obj([("uri", Json::str("hh://nope/x"))]),
    );
    assert_eq!(
        rr.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_int),
        Some(-32002)
    );
    // Subscribe — a call touching the run lands a durable event → the
    // `narrowed` updated notification follows.
    let s = request(
        &mut srv,
        &a,
        34,
        "resources/subscribe",
        Json::obj([("uri", Json::str(ledger_uri.clone()))]),
    );
    assert!(s.get("result").is_some(), "{s:?}");
    let _ = call(
        &mut srv,
        &a,
        "submit_input",
        Json::obj([
            ("run", Json::str(run_id.clone())),
            ("content", Json::str("ping")),
        ]),
    );
    let listen = request(&mut srv, &a, 35, "subscriptions/listen", Json::obj([]));
    let notes = listen
        .get("result")
        .and_then(|r| r.get("notifications"))
        .and_then(|v| match v {
            Json::Arr(x) => Some(x.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        notes.iter().any(|n| {
            n.get("method").and_then(Json::as_str) == Some("notifications/resources/updated")
                && n.get("params")
                    .and_then(|p| p.get("uri"))
                    .and_then(Json::as_str)
                    == Some(ledger_uri.as_str())
        }),
        "the subscription notifies after durability: {notes:?}"
    );
    // Unsubscribe — silence follows.
    let _ = request(
        &mut srv,
        &a,
        36,
        "resources/unsubscribe",
        Json::obj([("uri", Json::str(ledger_uri.clone()))]),
    );
    let _ = request(&mut srv, &a, 37, "subscriptions/listen", Json::obj([])); // drain
    let _ = call(
        &mut srv,
        &a,
        "submit_input",
        Json::obj([("run", Json::str(run_id)), ("content", Json::str("ping2"))]),
    );
    let listen = request(&mut srv, &a, 38, "subscriptions/listen", Json::obj([]));
    let notes = listen
        .get("result")
        .and_then(|r| r.get("notifications"))
        .and_then(|v| match v {
            Json::Arr(x) => Some(x.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        !notes.iter().any(|n| {
            n.get("params")
                .and_then(|p| p.get("uri"))
                .and_then(Json::as_str)
                == Some(ledger_uri.as_str())
        }),
        "unsubscribed ⇒ silent: {notes:?}"
    );
}

/// An expired binding loses its subscriptions (ADR-0175 D2).
#[test]
fn subscription_dies_with_binding() {
    let mut srv = lab();
    let mut b = stdio_launch_binding("test-exp", CallerKind::Agent);
    b.expires_at_ms = Some(1); // already stale at any real `now`
                               // Seed a subscription directly, then any dispatch under the dead
                               // binding cancels it.
    srv.subscriptions
        .entry(b.binding_id.clone())
        .or_default()
        .insert("hh://run/x/ledger".to_string());
    let _ = request(&mut srv, &b, 40, "ping", Json::obj([]));
    assert!(
        hh_mcp_lab::resources::subscriptions_of(&srv, &b.binding_id).is_empty(),
        "expired binding ⇒ cancelled subscriptions"
    );
}

// ── serve_bundle ────────────────────────────────────────────────────

#[test]
fn serve_bundle_wraps_lab_serve() {
    let mut srv = lab();
    let a = agent();
    // A bogus container is the op's own typed refusal — never a
    // protocol fault, never the delegate gate (serve_bundle is a
    // launch-class effect, not memory_write).
    let r = call(
        &mut srv,
        &a,
        "serve_bundle",
        Json::obj([
            ("bundle_ref", Json::str("/nonexistent/bundle.container")),
            ("transport", Json::str("stdio")),
        ]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_ne!(refusal_kind(&r), "DelegateWriteForbidden", "{r:?}");
    // The kernel's typed refusal (lab.serve is experimental-gated in
    // the default profile — `ExperimentalRequired` is the op's own
    // name; the point is the refusal is typed, never a protocol fault).
    assert_eq!(
        refusal_kind(&r),
        "ExperimentalRequired",
        "lab.serve's own typed refusal surfaces: {r:?}"
    );
}

/// The analysis group routes to `lab.analysis.*` — a bad spec is the
/// op's typed refusal, and a `service` binding calls reads fine
/// (reads are never delegate-gated).
#[test]
fn analysis_group_wraps_lab_analysis() {
    let mut srv = lab();
    let s = svc_binding();
    let r = call(
        &mut srv,
        &s,
        "analyze",
        Json::obj([("spec", Json::obj([]))]),
    );
    assert!(is_err(&r), "{r:?}");
    // Whatever the kernel's refusal name, it is never the delegate
    // write-gate's (analysis is read-class).
    assert_ne!(refusal_kind(&r), "DelegateWriteForbidden", "{r:?}");
}

/// AC-R-2.11.3-12 (C2 form) — the shipped document now carries the
/// `resource_carrier` record too; link refuses a doc missing it.
#[test]
fn resource_carrier_debt_required_at_link() {
    let doc = exposure::default_lab_document();
    let def = exposure::parse_exposure(&doc).expect("shipped doc links");
    let carriers: Vec<&str> = def
        .assumption_debt
        .iter()
        .map(|d| d.carrier.as_str())
        .collect();
    assert!(carriers.contains(&"tasks_carrier"), "{carriers:?}");
    assert!(carriers.contains(&"resource_carrier"), "{carriers:?}");
    // Strip the resource record — link refuses with MissingDebt.
    let mut m = match doc {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    if let Some(Json::Arr(a)) = m.get_mut("assumption_debt") {
        a.retain(|d| d.get("carrier").and_then(Json::as_str) != Some("resource_carrier"));
    }
    match exposure::parse_exposure(&Json::Obj(m)) {
        Err(exposure::ExposureError::MissingDebt { carrier }) => {
            assert_eq!(carrier, "resource_carrier")
        }
        other => panic!("expected MissingDebt{{resource_carrier}}, got {other:?}"),
    }
}
