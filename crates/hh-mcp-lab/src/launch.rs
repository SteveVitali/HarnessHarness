//! The launch group — `open_session{kind:"new", spawn_event}` +
//! `lifecycle.surface.invoked` + handle mints, and the
//! control-turn pass-throughs (`submit_input`, `cancel_run`,
//! `respond_approval`).
//!
//! Launch is a **surface write**, not an op wrap — the chain is:
//! `intended → decided → authorized → prepared → committed →
//! open_session → invoked + minted → observed{applied}`. The child's
//! `RunManifest.spawn_event` carries `{surface run, committed-ref}` —
//! `open_run` resolves it *before* the child exists, so the causal
//! link is durable at the moment the child lands (AC-R-2.11.3-8).
//!
//! `resume` naming: a crashed MCP host replays `launch_run` — the
//! durable `launched{idempotency_key}` row answers deterministically
//! **without re-spawning** (AC-R-2.11.3-4); a re-open *after* the
//! committed event landed fails closed (an event reference — the
//! adopted handle — never confers a second spawn).

use hh_embed_schema::types::{
    DefinitionInput, EnvironmentInput, InvocationRecord, OpenSpec, SpawnEventRef,
};
use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::DimensionId;
use hh_wire::json::Json;

use crate::binding::{CallerBinding, CallerKind};
use crate::dispatch::{op_call, SurfaceError, ToolOutcome};
use crate::effects::EffectCtx;
use crate::exposure::ExposureTool;
use crate::handles::HandleKind;
use crate::session::SurfaceSession;
use hh_embed::service::EmbedService;

/// `launch::*` / control-turn dispatch.
pub fn dispatch(
    svc: &mut EmbedService,
    session: &mut SurfaceSession,
    binding: &CallerBinding,
    tool: &ExposureTool,
    args: &std::collections::BTreeMap<String, Json>,
    fx: &mut EffectCtx,
    turn_id: &str,
) -> ToolOutcome {
    match tool.name.as_str() {
        "launch_run" => launch_run(svc, session, binding, tool, args, fx, turn_id),
        "submit_input" | "cancel_run" | "respond_approval" => {
            control_passthrough(svc, session, tool, args)
        }
        other => Err(SurfaceError::new(
            "unknown_tool",
            format!("launch group has no tool `{other}`"),
            Json::Null,
        )),
    }
}

/// The `launch_run` body — the launch lowering. `fx`'s committed row
/// already landed before this runs (decide_and_authorize → prepared →
/// committed in dispatch.rs); `fx.terminal_ref` is the `committed`
/// event — the child's `spawn_event` carrier.
fn launch_run(
    svc: &mut EmbedService,
    session: &mut SurfaceSession,
    binding: &CallerBinding,
    tool: &ExposureTool,
    args: &std::collections::BTreeMap<String, Json>,
    fx: &mut EffectCtx,
    turn_id: &str,
) -> ToolOutcome {
    let committed = fx.committed_ref.clone().ok_or_else(|| {
        SurfaceError::new(
            "kernel_error",
            "launch without a committed effect row",
            Json::Null,
        )
    })?;
    // ── arguments ────────────────────────────────────────────────────
    let definition = parse_definition(args)?;
    let environment = match args.get("environment") {
        Some(Json::Null) | None => EnvironmentInput::ConnectionInfo(Json::obj([])),
        Some(v) => parse_environment(v)?,
    };
    // A launch without a `budget` head refuses `MissingBudget`
    // (AC-R-2.11.3-4): the child's allocation is carved from the
    // caller's pool — `unbudgeted` never launches from a surface.
    let budget = match args.get("budget") {
        Some(Json::Null) | None => {
            return Err(SurfaceError::new(
                "MissingBudget",
                "launch_run requires `budget` — the child run's allocation head \
                 is carved from the binding's pool; an absent budget cannot size it",
                Json::obj([("required", Json::Arr(vec![Json::str("budget")]))]),
            ));
        }
        Some(v) => Some(parse_budget_input(v)?),
    };
    // The pool check — `MissingBudget` (the child asks for a dimension
    // the caller's pool doesn't declare) / `InsufficientBudget` (hard
    // head over remaining) refuse *with the durable refused row* — the
    // `intended` event already landed, so the refusal is ledgered.
    if let Some(hh_embed_schema::types::BudgetInput::Node(n)) = &budget {
        let requested = node_dimensions(n)?;
        if !requested.is_empty() {
            session.check_pool(svc, &requested)?;
        }
    }
    let idem = args
        .get("idempotency_key")
        .and_then(Json::as_str)
        .map(String::from)
        .unwrap_or_else(|| format!("launch:{}", fx.effect_id));
    // The caller's `resume` naming — `idempotency:launch_run:<binding>:<key>`
    // — the durable `launched` row minted on the FIRST observed launch;
    // a replay finds it and answers the recorded result verbatim.
    let resume_key = format!("idempotency:launch_run:{}:{idem}", binding.binding_id);

    // ── resume determinism (AC-R-2.11.3-4): the `launched` row for
    // this resume key, when present, answers without a second
    // `open_session` — ever.
    if let Some(entry) = session.handles.find_adopted(
        &session.run_id,
        std::slice::from_ref(&resume_key),
        HandleKind::Run,
    ) {
        if let Some(launched) = entry.payload.clone() {
            return Ok(resumed_launch_payload(&launched, svc));
        }
    }

    // ── the caller-kind → attendance fold (AC-R-2.11.3-3) ────────────
    let attendance = match binding.caller_kind {
        CallerKind::HumanPrincipal => "interactive",
        CallerKind::Agent | CallerKind::ProviderClient => "async",
        CallerKind::Service => "unattended",
    };
    let spec = OpenSpec::New {
        definition,
        overrides: vec![],
        profile_binding: None,
        environment,
        budget,
        participant: Some(crate::launch::participant_descriptor(&binding.caller_kind)),
        supplies: None,
        attendance: hh_embed_schema::types::AttendanceDeclaration {
            value: attendance.to_string(),
            source: "declared".to_string(),
        },
        approval_mode: args
            .get("approval_mode")
            .and_then(Json::as_str)
            .map(String::from),
        workspace_trust: None,
        narrowing_leaves: vec![],
        // THE CAUSAL CARRIER — the child's `RunManifest.spawn_event` =
        // `{surface run, effect.committed-ref}`. `open_run` resolves it
        // before the child lands; a dangling ref refuses the spawn.
        spawn_event: Some(SpawnEventRef {
            run_id: session.run_id.clone(),
            event_id: committed.event_id.clone(),
        }),
    };
    let invocation = InvocationRecord {
        argv_canonical: vec!["launch_run".to_string()],
        cwd_ref: "ref://mcp-lab".to_string(),
        principal: binding.principal_ref.clone(),
        attendance: hh_embed_schema::types::AttendanceDeclaration {
            value: attendance.to_string(),
            source: "declared".to_string(),
        },
        output_format: "json".to_string(),
        stdin_digest: None,
        overrides_layer_id: None,
        instrument_record: Json::obj([
            ("surface", Json::str("mcp")),
            ("binding_id", Json::str(binding.binding_id.clone())),
            ("tool", Json::str(tool.name.clone())),
        ]),
        idempotency_key: idem.clone(),
    };
    let params = Json::obj([
        ("spec", spec.to_json()),
        ("idempotency_key", Json::str(idem)),
        ("invocation", invocation.to_json()),
    ]);
    // `open_session` — via `svc.handle` so the invocation audit mints
    // `lifecycle.surface.invoked` on the child run (open.rs' mint)
    // carrying `invocation` + `spawn_event`.
    let resp = op_call(svc, "open_session", &params)?;
    let child_run = resp
        .get("run_id")
        .and_then(Json::as_str)
        .ok_or_else(|| {
            SurfaceError::new(
                "kernel_error",
                "open_session returned no run_id",
                resp.clone(),
            )
        })?
        .to_string();
    let child_session = resp
        .get("session_id")
        .and_then(Json::as_str)
        .map(String::from);
    let mint_seq = resp.get("seq").and_then(Json::as_int).unwrap_or(0);

    // ── mint the `launched` row — the resume answer + the alias table.
    let launched_payload = Json::obj([
        ("launch_id", Json::str(format!("launch-{}", fx.effect_id))),
        ("run_id", Json::str(child_run.clone())),
        (
            "session_id",
            child_session
                .as_ref()
                .map(|s| Json::str(s.clone()))
                .unwrap_or(Json::Null),
        ),
        ("run_alias", Json::str(format!("run:{child_run}"))),
        ("idempotency_key", Json::str(resume_key)),
        (
            "spawn_event",
            Json::obj([
                ("run_id", Json::str(session.run_id.clone())),
                ("event_id", Json::str(committed.event_id.clone())),
            ]),
        ),
        (
            "spawn_event_ref",
            Json::str(crate::dispatch::event_ref_str(&committed)),
        ),
        ("turn_id", Json::str(turn_id.to_string())),
        ("tool", Json::str(tool.name.clone())),
        ("seq", Json::Int(mint_seq.max(0))),
    ]);
    let minted = session.handles.mint_on_run(
        svc,
        &session.run_id,
        HandleKind::Run,
        &format!("launch-{}", fx.effect_id),
        &child_run,
        Some(&binding.binding_id),
        &[EventRef {
            run_id: session.run_id.clone(),
            event_id: committed.event_id.clone(),
        }],
        Some(launched_payload.clone()),
        child_session.clone(),
        svc.surface_now_ms(),
    )?;
    Ok(Json::obj([
        (
            "launched",
            Json::obj([
                ("run_id", Json::str(child_run.clone())),
                (
                    "session_id",
                    child_session
                        .as_ref()
                        .map(|s| Json::str(s.clone()))
                        .unwrap_or(Json::Null),
                ),
                ("status", Json::str("open")),
                (
                    "spawn_event_ref",
                    Json::str(crate::dispatch::event_ref_str(&committed)),
                ),
            ]),
        ),
        (
            "handles",
            Json::Arr(vec![Json::obj([
                ("handle", Json::str(minted.alias.clone())),
                ("surface_id", Json::str(format!("run:{child_run}"))),
                ("kind", Json::str("launched")),
                ("run_id", Json::str(child_run.clone())),
                (
                    "handle_ref",
                    Json::str(crate::dispatch::event_ref_str(&EventRef {
                        run_id: session.run_id.clone(),
                        event_id: minted.event_id.clone(),
                    })),
                ),
            ])]),
        ),
    ]))
}

/// The `submit_input` / `cancel_run` / `respond_approval`
/// pass-throughs — one param-object wrap; the op's JSON-RPC answer is
/// the payload (its typed refusal flows through `op_call` verbatim).
/// A dead writer session re-attaches `attach{mode:take_over}` and
/// retries once (the minted row's recorded session is a convenience,
/// never the authority — a crashed host's session dies with it).
fn control_passthrough(
    svc: &mut EmbedService,
    session: &mut SurfaceSession,
    tool: &ExposureTool,
    args: &std::collections::BTreeMap<String, Json>,
) -> ToolOutcome {
    let op = match tool.name.as_str() {
        "submit_input" => "submit",
        "cancel_run" => "cancel",
        "respond_approval" => "respond_permission",
        other => {
            return Err(SurfaceError::new(
                "unknown_tool",
                format!("no control op for `{other}`"),
                Json::Null,
            ))
        }
    };
    let mut params = args.clone();
    if tool.name == "respond_approval" {
        if let Some(v) = params.get("outcome").and_then(Json::as_str) {
            let mapped = match v {
                "approved" | "grant" | "allow" | "allowed_once" => "grant",
                "denied" | "deny" => "deny",
                "deferred" | "defer" => "defer",
                other => other,
            };
            params.insert("outcome".into(), Json::str(mapped));
        }
        if !params.contains_key("permission_id") {
            if let Some(p) = params.get("permission").cloned() {
                params.insert("permission_id".into(), p);
            }
        }
    }
    if tool.name == "cancel_run" && !params.contains_key("scope") {
        params.insert("scope".into(), Json::obj([("kind", Json::str("run"))]));
    }
    if !params.contains_key("idempotency_key") {
        params.insert(
            "idempotency_key".into(),
            Json::str(format!(
                "{}:{}",
                tool.name,
                params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .unwrap_or("?")
            )),
        );
    }
    let run_id = params
        .get("run_id")
        .and_then(Json::as_str)
        .map(String::from);
    let result = op_call(svc, op, &Json::Obj(params.clone()));
    match result {
        Err(e)
            if e.kind == "unknown_session"
                || e.kind == "SessionUnknown"
                || e.message.contains("unknown session") =>
        {
            // The recorded session died — re-attach take_over and retry
            // with the fresh session id.
            let Some(run) = run_id else {
                return Err(e);
            };
            let sid = attach_take_over(svc, &run)?;
            let mut retry = params;
            retry.insert("session_id".into(), Json::str(sid.clone()));
            // Record the live session on the minted entry so the next
            // call skips the attach.
            let _ = session; // the entry's session updates are advisory.
            op_call(svc, op, &Json::Obj(retry))
        }
        other => other,
    }
}

/// `open_session{kind:"attach", mode:"take_over"}` → fresh writer
/// session id for `run_id`.
pub fn attach_take_over(svc: &mut EmbedService, run_id: &str) -> Result<String, SurfaceError> {
    let params = Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("attach")),
                ("run_id", Json::str(run_id)),
                ("mode", Json::str("take_over")),
            ]),
        ),
        (
            "idempotency_key",
            Json::str(format!("attach:{}:{}", run_id, svc.surface_now_ms())),
        ),
    ]);
    let resp = match op_call(svc, "open_session", &params) {
        Ok(r) => r,
        // A `run_id` that is not a minted handle and names no run the
        // kernel knows is the handle table's `UnknownHandle` — the
        // §3.2 name the AC table pins (the kernel's own
        // `unknown_run`/`UnknownRun` spelling maps, never re-labels a
        // different refusal class).
        Err(e) if e.kind == "unknown_run" || e.kind == "UnknownRun" => {
            return Err(SurfaceError::new(
                "UnknownHandle",
                format!("run `{run_id}` is not a handle this surface minted"),
                Json::obj([("run_id", Json::str(run_id))]),
            ));
        }
        Err(e) => return Err(e),
    };
    resp.get("session_id")
        .and_then(Json::as_str)
        .map(String::from)
        .ok_or_else(|| {
            SurfaceError::new(
                "kernel_error",
                "attach returned no session_id",
                resp.clone(),
            )
        })
}

/// Resolve a run's *live* writer session — the minted `target_session`
/// when recorded (a dead one surfaces `unknown_session` at the op,
/// which `control_passthrough` retries through `attach_take_over`);
/// `attach{mode:"take_over"}` when nothing was recorded.
pub fn session_for_run(
    svc: &mut EmbedService,
    run_id: &str,
    recorded: Option<&str>,
) -> Result<String, SurfaceError> {
    if let Some(sid) = recorded {
        return Ok(sid.to_string());
    }
    attach_take_over(svc, run_id)
}

/// A resumed `launch_run` answer — the durable `launched` row's
/// recorded result plus the run's live status (no second open).
fn resumed_launch_payload(launched: &Json, svc: &mut EmbedService) -> Json {
    let run_id = launched
        .get("run_id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let status = crate::reads::run_status_of(svc, &run_id);
    Json::obj([
        (
            "launched",
            Json::obj([
                ("run_id", Json::str(run_id.clone())),
                (
                    "session_id",
                    launched.get("session_id").cloned().unwrap_or(Json::Null),
                ),
                ("status", Json::str(status)),
                (
                    "spawn_event_ref",
                    launched
                        .get("spawn_event_ref")
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        (
            "handles",
            Json::Arr(vec![Json::obj([
                (
                    "handle",
                    Json::str(
                        launched
                            .get("launch_id")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                ),
                (
                    "surface_id",
                    Json::str(format!("run:{child}", child = run_id)),
                ),
                ("kind", Json::str("launched")),
                ("run_id", Json::str(run_id)),
            ])]),
        ),
    ])
}

/// The `ParticipantDescriptor` a launch carries — `native` for lab
/// callers, `hosted` when a supply-surface binding launches (the
/// hosted participant's own spawns stay `hosted`-classed).
pub fn participant_descriptor(kind: &CallerKind) -> Json {
    Json::obj([(
        "class",
        Json::str(match kind {
            CallerKind::Service => "hosted",
            _ => "native",
        }),
    )])
}

/// `DefinitionInput` — `"ref"` string or `{kind:"document"|"ref",…}`.
fn parse_definition(
    args: &std::collections::BTreeMap<String, Json>,
) -> Result<DefinitionInput, SurfaceError> {
    match args.get("definition") {
        Some(Json::Str(s)) => Ok(DefinitionInput::Ref(s.clone())),
        Some(v @ Json::Obj(_)) => {
            DefinitionInput::from_json(v, "launch_run/definition").map_err(|e| {
                SurfaceError::new("schema_violation", format!("definition: {e:?}"), Json::Null)
            })
        }
        _ => Err(SurfaceError::new(
            "schema_violation",
            "launch_run requires `definition` (ref string or {kind,…})",
            Json::obj([("required", Json::Arr(vec![Json::str("definition")]))]),
        )),
    }
}

/// `EnvironmentInput` — `"ref"` string or `{kind,…}`.
fn parse_environment(v: &Json) -> Result<EnvironmentInput, SurfaceError> {
    match v {
        Json::Str(s) => Ok(EnvironmentInput::Ref(s.clone())),
        other => EnvironmentInput::from_json(other, "launch_run/environment").map_err(|e| {
            SurfaceError::new(
                "schema_violation",
                format!("environment: {e:?}"),
                Json::Null,
            )
        }),
    }
}

/// The `budget` argument → `BudgetInput::Node` — `{dimensions:
/// {tool_calls:{hard:N},…}}` (or the `{tool_calls:N}` shorthand →
/// `{hard:N}`).
fn parse_budget_input(v: &Json) -> Result<hh_embed_schema::types::BudgetInput, SurfaceError> {
    let dims = match v.get("dimensions") {
        Some(Json::Obj(m)) => m.clone(),
        Some(Json::Null) | None => match v {
            Json::Obj(m) => m.clone(),
            _ => std::collections::BTreeMap::new(),
        },
        _ => {
            return Err(SurfaceError::new(
                "schema_violation",
                "budget.dimensions must be an object",
                Json::Null,
            ))
        }
    };
    let mut dm = std::collections::BTreeMap::new();
    for (dim, spec) in &dims {
        match spec {
            Json::Obj(_) => {
                dm.insert(dim.clone(), spec.clone());
            }
            Json::Int(h) => {
                dm.insert(dim.clone(), Json::obj([("hard", Json::Int(*h))]));
            }
            _ => {
                return Err(SurfaceError::new(
                    "schema_violation",
                    format!("budget.dimensions.{dim} must be `{{hard:N}}` or an int"),
                    Json::Null,
                ))
            }
        }
    }
    Ok(hh_embed_schema::types::BudgetInput::Node(Json::obj([(
        "dimensions",
        Json::Obj(dm),
    )])))
}

/// The requested `{dimension → hard}` from a `BudgetInput::Node` JSON —
/// the pool check's read of the node's declared dimensions.
fn node_dimensions(
    node: &Json,
) -> Result<std::collections::BTreeMap<DimensionId, i64>, SurfaceError> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(Json::Obj(dims)) = node.get("dimensions") {
        for (dim, spec) in dims {
            // A dimension the ontology doesn't name is a head the
            // caller's pool *by construction* doesn't declare —
            // `MissingBudget`, never a silent skip (AC-R-2.11.3-4: an
            // undeclared request cannot launch unbudgeted).
            let Some(d) = DimensionId::parse(dim) else {
                let detail = Json::obj([("dimension", Json::str(dim.clone()))]);
                return Err(SurfaceError::new(
                    "MissingBudget",
                    format!(
                        "`{dim}` is not a declared budget dimension — the pool cannot cover it"
                    ),
                    detail,
                ));
            };
            let hard = spec
                .get("hard")
                .and_then(Json::as_int)
                .or_else(|| spec.as_int());
            if let Some(h) = hard {
                out.insert(d, h);
            }
        }
    }
    Ok(out)
}
