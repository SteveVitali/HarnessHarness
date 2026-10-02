//! The `tools/call` dispatch — the one path every Lab tool answer
//! takes (AC-R-2.11.3-2's typed `isError` for ordinary refusals;
//! JSON-RPC errors only for protocol-level failure).
//!
//! Decision order (the §2.5/§3.2 chain — `_meta` never consulted):
//! 1. tool lookup — unknown → `unknown_tool` isError (no turn: the
//!    catalogue never admitted the call);
//! 2. binding `permissions[]` ∩ the tool — empty ⇒ every exposed tool;
//! 3. supply-surface binding — Π evaluates before the Lab groups;
//! 4. caller-kind gate — `respond_approval` is `human_principal` only
//!    (`IllegitimateEndorsement`), `service` bindings launch
//!    `unattended`;
//! 5. the surface-session turn: `lifecycle.turn.started` →
//!    `action.effect.*` (intended → authorized (+decided for writers)
//!    → prepared → committed? → observed|refused) →
//!    `control.budget.consumed{tool_calls:+1, charged_to:instrument}`
//!    → `lifecycle.turn.finished`;
//! 6. the wrapped op / declared lowering — `EmbedService::handle` for
//!    `op ≠ ""`, the surface's own envelope lowering for `op = ""`;
//! 7. handles resolve through [`crate::handles::HandleTable`] —
//!    `surface_ids` names, typed refusals on misuse.

use hh_embed_schema::errors::EmbedError;
use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::DimensionId;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request as RpcRequest;

use crate::binding::CallerBinding;
use crate::effects::EffectCtx;
use crate::exposure::{ExposureTool, ToolGroup};
use crate::handles::HandleRefusal;
use crate::server::LabServer;
use crate::session::SurfaceSession;

/// A typed refusal — the `surface_error` record inside `isError` (the
/// §3.2 error-name table + protocol-free failure kinds).
#[derive(Debug, Clone)]
pub struct SurfaceError {
    /// The error-name spelling (`UnknownHandle`, `MissingBudget`, …).
    pub kind: String,
    /// The one-line message.
    pub message: String,
    /// Structured detail (`{dimension, requested, available}`, …).
    pub detail: Json,
    /// `retry_eligible` — a retry could change the answer (rate limits,
    /// durability backs off; authority/handle refusals never do).
    pub retry_eligible: bool,
}

impl SurfaceError {
    /// A refusal record `{kind, message, detail?, retry_eligible}`.
    pub fn new(kind: &str, message: impl Into<String>, detail: Json) -> SurfaceError {
        SurfaceError {
            kind: kind.to_string(),
            message: message.into(),
            detail,
            retry_eligible: false,
        }
    }

    /// From a handle refusal.
    pub fn handle(r: &HandleRefusal) -> SurfaceError {
        SurfaceError::new(r.kind(), r.message(), r.to_json())
    }

    /// From an `EmbedError` op refusal — the op's error kind maps to
    /// the surface-error spelling (the refusal table's names win where
    /// they exist; a schema/unknown stays its own kind — protocol
    /// faults are never re-labelled).
    pub fn op(e: &EmbedError) -> SurfaceError {
        let (kind, msg) = match e {
            EmbedError::SchemaViolation { path, code } => {
                ("schema_violation", format!("{path}: {code}"))
            }
            EmbedError::UnknownSession => ("unknown_session", "unknown session".to_string()),
            EmbedError::UnknownRun { .. } => ("UnknownHandle", format!("{e:?}")),
            EmbedError::Refused { reason } => {
                if reason == "stage_pending" {
                    ("stage_pending", "staged for a later ticket".to_string())
                } else {
                    ("refused", reason.clone())
                }
            }
            other => ("kernel_error", format!("{other:?}")),
        };
        SurfaceError::new(kind, msg, e.to_data_json())
    }
}

/// The `CallToolResult` — `{isError, content, structuredContent,
/// next_cursor?, handles?, plans?, pending_effects?}`.
pub fn call_result_ok(payload: Json) -> Json {
    Json::obj([
        ("isError", Json::Bool(false)),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                ("text", Json::str(payload.to_canonical_string())),
            ])]),
        ),
        ("structuredContent", payload),
    ])
}

/// The typed refusal result — the refusal record is the
/// `structuredContent` (`{refusal: <kind>, detail}` spells — the
/// `surface_error` shape adds `surface_error{kind,…}` when a name
/// exists in the §3.2 table).
pub fn call_result_refusal(e: &SurfaceError) -> Json {
    let record = Json::obj([
        ("refusal", Json::str(e.kind.clone())),
        ("detail", Json::str(e.message.clone())),
        ("surface_error", {
            let mut m = std::collections::BTreeMap::new();
            m.insert("kind".into(), Json::str(e.kind.clone()));
            m.insert("message".into(), Json::str(e.message.clone()));
            m.insert("retry_eligible".into(), Json::Bool(e.retry_eligible));
            if e.detail != Json::Null && e.detail != Json::obj([]) {
                m.insert("detail".into(), e.detail.clone());
            }
            Json::Obj(m)
        }),
    ]);
    Json::obj([
        ("isError", Json::Bool(true)),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                ("text", Json::str(record.to_canonical_string())),
            ])]),
        ),
        ("structuredContent", record),
    ])
}

/// The outcome one tool body returns — `Ok(payload)` answers, `Err(e)`
/// is ledgered `refused` and answered typed.
pub type ToolOutcome = Result<Json, SurfaceError>;

/// R-3 (ADR-0174 D4) — the request `_meta` is read for exactly the
/// declared keys: the tasks-extension declaration, `protocolVersion`,
/// `clientCapabilities` and the tracing members. Every other key is
/// preserved (unknown `_meta` survives in `ext`) and never consulted —
/// claims never decide (AC-K3-6).
///
/// The tasks declaration a request carries — `_meta`
/// `io.modelcontextprotocol/tasks` present (any value marks the
/// request task-shaped); returns the declaration object.
pub fn meta_tasks_declared(request_meta: Option<&Json>) -> bool {
    request_meta
        .and_then(|m| m.get("io.modelcontextprotocol/tasks"))
        .is_some_and(|v| v != &Json::Null)
}

/// The one `tools/call` path.
pub fn call_tool(
    srv: &mut LabServer,
    binding: &CallerBinding,
    name: &str,
    arguments: Json,
    call_id: &Json,
    request_meta: Option<&Json>,
) -> Json {
    // 1 — a supply-surface binding's calls are Π-evaluated, never the
    // Lab groups' (the hosted participant sees only its own surface —
    // its own ARTIFACT is its catalogue: a name it does not carry is
    // `unknown_tool` even when the Lab group exposes one by that
    // spelling, `callable ⇔ revealed` — AC-R-2.11.3-9). The dispatch is
    // before the Lab catalogue lookup so supply-only names resolve.
    if let Some(surface) = srv
        .exposure
        .supply_surfaces
        .iter()
        .find(|s| s.binding_id == binding.binding_id)
        .cloned()
    {
        let Some(at) = surface
            .artifact
            .tools
            .iter()
            .find(|t| t.name == name)
            .cloned()
        else {
            return call_result_refusal(&SurfaceError::new(
                "unknown_tool",
                format!("unknown tool `{name}`"),
                Json::obj([("name", Json::str(name))]),
            ));
        };
        let tool = crate::supply::tool_for_artifact(&at);
        return crate::supply::dispatch(srv, &surface, binding, &tool, arguments, call_id);
    }
    // R-3 — every `_meta` member except the declared set is ignored
    // for decisions (a forged `clientInfo`, entitlement key or
    // capability flag reads identically — AC-K3-6's byte-identity).
    let tasks_declared = meta_tasks_declared(request_meta);
    // 2 — the Lab catalogue.
    let Some(tool) = srv.exposure.tools.iter().find(|t| t.name == name) else {
        return call_result_refusal(&SurfaceError::new(
            "unknown_tool",
            format!("unknown tool `{name}`"),
            Json::obj([("name", Json::str(name))]),
        ));
    };
    let tool = tool.clone();
    // 3 — the binding's permission slice (`permissions[] ⊆ exposure`;
    // empty ⇒ every exposed tool).
    let permitted = binding.permissions.is_empty()
        || binding
            .permissions
            .iter()
            .any(|p| p == &tool.name || p == &tool.semantic_id);
    if !permitted {
        return call_result_refusal(&SurfaceError::new(
            "NoCoveringGrant",
            format!(
                "binding `{}` holds no permission for `{name}`",
                binding.binding_id
            ),
            Json::obj([("tool", Json::str(name))]),
        ));
    }
    // ── the delegate write-gate (§7.3 §2.3 effect-classes row;
    // WS-K3 §6.2 R-5): `memory_write` tools are refused for `delegate`
    // callers (agent | provider_client | service) unless a sealed
    // `entities.permissions[]` record covers the namespace for this
    // binding's principal. `human_principal` holds the principal
    // ceiling and writes under its own attestation.
    if tool.is_memory_write()
        && binding.caller_kind != crate::binding::CallerKind::HumanPrincipal
        && !srv.exposure.permission_covers(binding, &tool)
    {
        return call_result_refusal(&SurfaceError::new(
            "DelegateWriteForbidden",
            format!(
                "caller_kind `{}` is delegate-ceilinged — `{name}` is a \
                 memory_write tool and no sealed `entities.permissions[]` \
                 record covers its namespace for `{}`",
                binding.caller_kind.as_str(),
                binding.principal_ref
            ),
            Json::obj([
                ("tool", Json::str(name)),
                ("caller_kind", Json::str(binding.caller_kind.as_str())),
            ]),
        ));
    }
    let result = dispatch_lab(srv, binding, &tool, arguments.clone(), call_id);
    // ── the tasks carrier (§7.3 §2.5; AC-R-2.11.3-10): a request whose
    // `_meta` declares `io.modelcontextprotocol/tasks` gets the
    // `CreateTaskResult` shape for the launch verbs — seq-0 durability
    // already landed inside the lowering, so the task id binds the
    // durable run/experiment alias. (`-32021` for a declaration
    // without the client capability is enforced in `server::dispatch`
    // before this path runs.)
    if tasks_declared
        && matches!(tool.name.as_str(), "launch_run" | "open_experiment")
        && result.get("isError") == Some(&Json::Bool(false))
    {
        return crate::tasks::wrap_create_task_result(srv, binding, &tool.name, &result);
    }
    // A served subscription notifies after the underlying event is
    // durable — the call's effect chain already closed, so the
    // update lands now.
    crate::resources::notify_after_call(srv, binding, &tool.name, &arguments, &result);
    result
}

/// The lab-group path — the surface-session turn + effect chain around
/// one tool body.
fn dispatch_lab(
    srv: &mut LabServer,
    binding: &CallerBinding,
    tool: &ExposureTool,
    arguments: Json,
    call_id: &Json,
) -> Json {
    // The session — opened lazily on the first call (the binding is
    // already resolved; the run is the session's durable record). The
    // open consumes `&mut srv` once; every later use borrows the
    // `sessions`/`svc` FIELDS disjointly (a whole-`srv` method borrow
    // can't coexist with a field borrow).
    let bid = binding.binding_id.clone();
    if let Err(e) = srv.ensure_session(binding) {
        return call_result_refusal(&SurfaceError::new(
            "kernel_error",
            format!("surface session: {e:?}"),
            Json::Null,
        ));
    }
    let run_id = srv.sessions[&bid].run_id.clone();
    let (turn_id, _turn_ref) = match srv
        .sessions
        .get_mut(&bid)
        .expect("session ensured")
        .begin_turn(&mut srv.svc, &tool.name, call_id)
    {
        Ok(t) => t,
        Err(e) => {
            return call_result_refusal(&SurfaceError::new(
                "kernel_error",
                format!("turn open: {e:?}"),
                Json::Null,
            ))
        }
    };
    let mut fx = EffectCtx::mint(
        &mut srv.svc,
        &run_id,
        &tool.semantic_id,
        &srv.exposure.version_id,
        &arguments,
        &tool.effect,
    );
    if let Err(e) = fx.intended(
        &mut srv.svc,
        srv.sessions.get(&bid).expect("session"),
        &turn_id,
        binding,
        &tool.name,
        &arguments,
    ) {
        return call_result_refusal(&SurfaceError::new(
            "kernel_error",
            format!("effect intended: {e:?}"),
            Json::Null,
        ));
    }

    // ── the caller-kind gate (AC-R-2.11.3-3) ────────────────────────
    // `respond_approval` is the `permission_request` domain — R-2
    // principals only. Every other caller kind refuses
    // `IllegitimateEndorsement` *with the durable refused row*.
    let gate: Option<SurfaceError> =
        if tool.name == "respond_approval" && !binding.caller_kind.may_respond_approval() {
            Some(SurfaceError::new(
                "IllegitimateEndorsement",
                format!(
                    "caller_kind `{}` cannot decide a permission ask — \
                 `respond_approval` is human_principal-only",
                    binding.caller_kind.as_str()
                ),
                Json::obj([("caller_kind", Json::str(binding.caller_kind.as_str()))]),
            ))
        } else {
            None
        };
    // Handle arguments resolve through the table — `surface_ids` are
    // names; the typed refusals land before any op runs (a handle never
    // guesses, never confers).
    let (resolved_args, gate): (
        Option<std::collections::BTreeMap<String, Json>>,
        Option<SurfaceError>,
    ) = match gate {
        Some(e) => (None, Some(e)),
        None => match resolve_handle_args(
            &mut srv.svc,
            srv.sessions.get(&bid).expect("session"),
            tool,
            &arguments,
        ) {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e)),
        },
    };

    let (payload_or_err, authorized) = match (gate, resolved_args) {
        (Some(e), _) => (Err(e), false),
        (None, Some(args)) => {
            if tool.effect.read_only {
                // read_only: Π allow → prepared → dispatch → observed.
                let mut fail = None;
                if let Err(e) = fx.authorize(
                    &mut srv.svc,
                    srv.sessions.get(&bid).expect("session"),
                    &turn_id,
                ) {
                    fail = Some(SurfaceError::new(
                        "kernel_error",
                        format!("authorized: {e:?}"),
                        Json::Null,
                    ));
                }
                if fail.is_none() {
                    if let Err(e) = fx.prepared(
                        &mut srv.svc,
                        srv.sessions.get(&bid).expect("session"),
                        &turn_id,
                    ) {
                        fail = Some(SurfaceError::new(
                            "kernel_error",
                            format!("prepared: {e:?}"),
                            Json::Null,
                        ));
                    }
                }
                match fail {
                    Some(e) => (Err(e), true),
                    None => (
                        run_tool_body(srv, binding, tool, &args, &mut fx, &turn_id),
                        true,
                    ),
                }
            } else {
                // mutating: decided{allow} → authorized → prepared →
                // committed → dispatch → observed (I-H7's write-ahead).
                let mut fail = None;
                if let Err(e) = fx.decide_and_authorize(
                    &mut srv.svc,
                    srv.sessions.get(&bid).expect("session"),
                    &turn_id,
                    binding,
                    "allow",
                    "caller_binding authority",
                ) {
                    fail = Some(SurfaceError::new(
                        "kernel_error",
                        format!("decided: {e:?}"),
                        Json::Null,
                    ));
                }
                if fail.is_none() {
                    if let Err(e) = fx.prepared(
                        &mut srv.svc,
                        srv.sessions.get(&bid).expect("session"),
                        &turn_id,
                    ) {
                        fail = Some(SurfaceError::new(
                            "kernel_error",
                            format!("prepared: {e:?}"),
                            Json::Null,
                        ));
                    }
                }
                if fail.is_none() {
                    if let Err(e) = fx.committed(
                        &mut srv.svc,
                        srv.sessions.get(&bid).expect("session"),
                        &turn_id,
                    ) {
                        fail = Some(SurfaceError::new(
                            "kernel_error",
                            format!("committed: {e:?}"),
                            Json::Null,
                        ));
                    }
                }
                match fail {
                    Some(e) => (Err(e), true),
                    None => (
                        run_tool_body(srv, binding, tool, &args, &mut fx, &turn_id),
                        true,
                    ),
                }
            }
        }
        _ => unreachable!(),
    };

    // ── terminal row + charge + turn close ──────────────────────────
    let (result_json, outcome, refused_reason) = match &payload_or_err {
        Ok(p) => (p.clone(), "applied", None),
        Err(e) => (Json::Null, "refused", Some(e.kind.clone())),
    };
    // A refused *call* (gate/permission/handle) lands `refused` from
    // `intended`; an authorized dispatch that failed lands
    // `observed{not_applied}`.
    match &payload_or_err {
        Err(e) => {
            if authorized {
                let _ = fx.observed(
                    &mut srv.svc,
                    srv.sessions.get(&bid).expect("session"),
                    &turn_id,
                    "not_applied",
                    None,
                );
            }
            let _ = fx.refused(
                &mut srv.svc,
                srv.sessions.get(&bid).expect("session"),
                &turn_id,
                &e.kind,
                e.detail.clone(),
            );
        }
        Ok(_) => {
            let _ = fx.observed(
                &mut srv.svc,
                srv.sessions.get(&bid).expect("session"),
                &turn_id,
                outcome,
                Some(&result_json),
            );
        }
    }
    // The charge — every surface-session turn posts `tool_calls +1`
    // `instrument` against the pool (AC-R-2.11.3-5; `source` = the
    // terminal row — `(source, dimension)` charges exactly once, so a
    // rebuilt session never double-posts).
    let source = fx
        .terminal_ref
        .clone()
        .or_else(|| fx.intended_ref.clone())
        .unwrap_or(EventRef {
            run_id: run_id.clone(),
            event_id: "seq0".to_string(),
        });
    let _ = srv.sessions.get_mut(&bid).expect("session").charge_turn(
        &mut srv.svc,
        &source,
        DimensionId::ToolCalls,
        1,
    );
    let detail = Json::obj([
        ("tool", Json::str(tool.name.clone())),
        ("effect_id", Json::str(fx.effect_id.clone())),
    ]);
    let _ = srv.sessions.get_mut(&bid).expect("session").finish_turn(
        &mut srv.svc,
        &turn_id,
        outcome,
        detail,
    );
    match payload_or_err {
        Ok(p) => call_result_ok(p),
        Err(e) => call_result_refusal(&e),
    }
    .with_extras(|m| {
        if refused_reason.is_some() {
            m.insert(
                "pending_effects".into(),
                Json::Arr(vec![Json::str(fx.effect_id.clone())]),
            );
        }
    })
}

/// `run_tool_body` — the wrapped op or declared lowering.
fn run_tool_body(
    srv: &mut LabServer,
    binding: &CallerBinding,
    tool: &ExposureTool,
    args: &std::collections::BTreeMap<String, Json>,
    fx: &mut EffectCtx,
    turn_id: &str,
) -> ToolOutcome {
    let bid = binding.binding_id.clone();
    match tool.group {
        // The launch group is the surface's own lowering — `open_session`
        // + `lifecycle.surface.invoked` + handle mints, never a raw op
        // pass-through.
        ToolGroup::Launch => crate::launch::dispatch(
            &mut srv.svc,
            srv.sessions.get_mut(&bid).expect("session"),
            binding,
            tool,
            args,
            fx,
            turn_id,
        ),
        // The hosting group — `serve_bundle` wraps `lab.serve` and
        // mints the `session_handle` the §7.3 table names (the
        // connection's surface name; the spawn is the caller's — the
        // `launch` descriptor it returns).
        ToolGroup::Hosting => {
            let payload = op_call(&mut srv.svc, &tool.op, &Json::Obj(args.clone()))?;
            if tool.name == "serve_bundle" {
                let now_ms = srv.svc.surface_now_ms();
                let session = srv.sessions.get_mut(&bid).expect("session");
                let surface_run = session.run_id.clone();
                let minted = session.handles.mint_on_run(
                    &mut srv.svc,
                    &surface_run,
                    crate::handles::HandleKind::Session,
                    &format!("serve-{}", fx.effect_id),
                    &format!("serve-session-{}", fx.effect_id),
                    Some(&binding.binding_id),
                    &[EventRef {
                        run_id: surface_run.clone(),
                        event_id: fx
                            .terminal_ref
                            .clone()
                            .map(|r| r.event_id)
                            .unwrap_or_else(|| "seq0".to_string()),
                    }],
                    None,
                    None,
                    now_ms,
                )?;
                let mut out = payload;
                if let Json::Obj(m) = &mut out {
                    m.insert("session_handle".into(), Json::str(minted.alias.clone()));
                    m.entry("connection_info".into()).or_insert_with(|| {
                        Json::obj([("handle", Json::str(minted.alias.clone()))])
                    });
                }
                return Ok(out);
            }
            Ok(payload)
        }
        // Declared reads (`op = ""`) go through the surface's read
        // path — `ExposurePolicy` filters + the `delivered` mint.
        _ if tool.op.is_empty() => crate::reads::dispatch(
            &mut srv.svc,
            &srv.exposure,
            srv.sessions.get_mut(&bid).expect("session"),
            binding,
            tool,
            args,
        ),
        // The one-wraps-one-op path — `svc.handle` with the mapped
        // params; the session's `run` handle already rewrote to
        // `session_id`/`run_id`/`experiment_id`/`row_key`.
        _ => {
            let params = Json::Obj(args.clone());
            op_call(&mut srv.svc, &tool.op, &params)
        }
    }
}

/// `svc.handle(op, params)` → `ToolOutcome` — a JSON-RPC `error` is the
/// op's typed refusal, surfaced verbatim (`surface_error`); a
/// `result` is the payload. `hello` is issued on demand (the embed
/// service gates every op on it).
pub fn op_call(svc: &mut hh_embed::service::EmbedService, op: &str, params: &Json) -> ToolOutcome {
    let req = RpcRequest {
        id: Json::Int(1),
        method: op.to_string(),
        params: params.clone(),
    };
    let mut resp = svc.handle(&req);
    // The hello gate — `NotInitialized` (any op before `hello`) or a
    // `hello`-naming refusal → handshake once, retry once.
    let needs_hello = resp
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Json::as_str)
        .is_some_and(|m| m.contains("hello") || m.contains("NotInitialized"))
        || resp
            .get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str)
            .is_some_and(|k| k == "NotInitialized");
    if needs_hello {
        // A full `HelloParams` — `contract_major 1`, the server names
        // itself, and the capabilities the slice's op set needs (the
        // host executor for launched runs, the permission channel for
        // `respond_approval`, ephemeral frames for stream reads).
        let caps = |names: &[(&str, bool)]| {
            Json::Obj(
                names
                    .iter()
                    .map(|(k, v)| (k.to_string(), Json::Bool(*v)))
                    .collect(),
            )
        };
        let hello = RpcRequest {
            id: Json::Int(0),
            method: "hello".to_string(),
            params: Json::obj(vec![
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
                    caps(&[
                        ("serves_host_executor", true),
                        ("serves_permission_channel", true),
                        ("accepts_ephemeral_frames", true),
                    ]),
                ),
            ]),
        };
        let _ = svc.handle(&hello);
        resp = svc.handle(&req);
    }
    if let Some(err) = resp.get("error") {
        let msg = err
            .get("message")
            .and_then(Json::as_str)
            .unwrap_or("op refused")
            .to_string();
        let kind = err
            .get("data")
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str)
            .unwrap_or("refused")
            .to_string();
        return Err(SurfaceError::new(
            &kind,
            msg,
            err.get("data").cloned().unwrap_or(Json::Null),
        ));
    }
    Ok(resp.get("result").cloned().unwrap_or(Json::Null))
}

/// Resolve a tool's `handle_args` — each named argument's string value
/// resolves through the session's table (the typed refusal propagates
/// verbatim; the argument rewrites to the resolved target — `run`
/// becomes `session_id`/`run_id`/`experiment_id`/`row_key` per the
/// `arg_map`).
fn resolve_handle_args(
    svc: &mut hh_embed::service::EmbedService,
    session: &SurfaceSession,
    tool: &ExposureTool,
    arguments: &Json,
) -> Result<std::collections::BTreeMap<String, Json>, SurfaceError> {
    let mut out = match arguments {
        Json::Obj(m) => m.clone(),
        _ => std::collections::BTreeMap::new(),
    };
    let now = svc.surface_now_ms();
    for arg in &tool.handle_args {
        let Some(alias) = out.get(arg).and_then(Json::as_str).map(String::from) else {
            continue;
        };
        let entry = session
            .handles
            .resolve(&alias, &session.binding.binding_id, now.max(0) as u64)
            .map_err(|r| SurfaceError::handle(&r))?;
        let target = entry.target_id.clone();
        // The recorded writer session (the `launched`/`minted` payload's
        // `session_id` member) — every session-mediated op reads it;
        // a dead session re-attaches in the op path itself.
        let target_session = entry
            .payload
            .as_ref()
            .and_then(|p| p.get("session_id"))
            .and_then(Json::as_str)
            .map(String::from);
        if let Some(sid) = &target_session {
            out.entry("session_id".to_string())
                .or_insert(Json::str(sid.clone()));
        }
        // The arg rewrites per `arg_map` — `run` → `session_id` for the
        // session-mediated ops (the live session is resumed on demand
        // by the launch module; the resolved session id is recorded
        // durably in the minted row), `run` → `run_id` for reads,
        // `experiment` → `experiment_id`.
        let mapped = tool
            .arg_map
            .get(arg)
            .cloned()
            .unwrap_or_else(|| arg.clone());
        match mapped.as_str() {
            "session_id" => {
                let sid = crate::launch::session_for_run(svc, &target, target_session.as_deref())?;
                out.insert(mapped, Json::str(sid));
                // The op's run coordinate the read paths want.
                out.entry("run_id".to_string()).or_insert(Json::str(target));
            }
            "run_id" => {
                out.insert(mapped, Json::str(target));
            }
            other => {
                out.insert(other.to_string(), Json::str(target));
            }
        }
    }
    Ok(out)
}

/// `Json` extension — append extra members to a `CallToolResult`.
trait WithExtras {
    fn with_extras(self, f: impl FnOnce(&mut std::collections::BTreeMap<String, Json>)) -> Json;
}
impl WithExtras for Json {
    fn with_extras(self, f: impl FnOnce(&mut std::collections::BTreeMap<String, Json>)) -> Json {
        match self {
            Json::Obj(mut m) => {
                f(&mut m);
                Json::Obj(m)
            }
            other => other,
        }
    }
}

/// An `EventRef`'s canonical string — `run_id/event_id` (the
/// `spawn_event_ref`/`handle_ref` spellings in payloads).
pub fn event_ref_str(r: &hh_ledger::manifest::EventRef) -> String {
    format!("{}/{}", r.run_id, r.event_id)
}

/// The supply path's effect context — `session.rs`'s own shape (the
/// same chain, minted under the hosted binding's surface session).
pub fn effect_ctx_for(
    svc: &mut hh_embed::service::EmbedService,
    exposure_version_id: &str,
    session: &SurfaceSession,
    tool: &ExposureTool,
    arguments: &Json,
) -> EffectCtx {
    EffectCtx::mint(
        svc,
        &session.run_id,
        &tool.semantic_id,
        exposure_version_id,
        arguments,
        &tool.effect,
    )
}
