//! The supply-surface Π dispatch (spec §7.3 §3.2/§3.4; AC-R-2.11.3-4):
//! a hosted participant binding sees only its own supply surface's
//! catalogue, and every `tools/call` on it evaluates the surface's
//! `pi[]` rules **before** anything else runs:
//!
//! - `hidden` ⇒ `unknown_tool` (the tool is never revealed — a name
//!   the catalogue does not admit);
//! - `deny` ⇒ `DeniedByPolicy` — the effect lands
//!   `intended → refused{denied}` with the rule named;
//! - `ask` ⇒ `security.permission.pending` is minted durably on the
//!   caller's surface run and the call answers
//!   `PermissionAskRequired` + `pending_effects[]` — the ask is NEVER
//!   auto-approved (the human half arrives via `respond_approval`, and
//!   a resumed call re-asks until a `decided` row exists);
//! - `allow` ⇒ the declared lowering runs (`authorized → prepared →
//!   observed`), the same chain a lab tool takes.
//!
//! Every decision is a surface-session turn — the hosted run sees the
//! same `lifecycle.turn.*` + `action.effect.*` spine a lab caller gets.

use hh_wire::json::Json;

use crate::binding::CallerBinding;
use crate::dispatch::{call_result_refusal, effect_ctx_for, op_call, SurfaceError, ToolOutcome};
use crate::exposure::{ExposureTool, SupplySurface};
use crate::server::LabServer;
use crate::session::mint_event;
use hh_ledger::event::Scope;

/// The `ExposureTool` a supply artifact member lowers to — the
/// artifact's own record is the declaration (name/semantic_id/
/// schemas); `op` comes out of `hir_meta` (empty ⇒ the declared stub).
/// The effect shape is the read-only chain (`authorized → prepared →
/// dispatch → observed` — the allow arm's own spelling).
pub fn tool_for_artifact(at: &hh_mcp::artifact::ArtifactTool) -> ExposureTool {
    ExposureTool {
        name: at.name.clone(),
        semantic_id: at.semantic_id.clone(),
        description: at.description.clone().unwrap_or_default(),
        op: at
            .hir_meta
            .get("op")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        group: crate::exposure::ToolGroup::Supply,
        effect: crate::exposure::EffectDecl::read_only(),
        arg_map: std::collections::BTreeMap::new(),
        handle_args: vec![],
        input_schema: at.input_schema.clone(),
        output_schema: at.output_schema.clone(),
        lowering: None,
    }
}

/// Π-dispatch a call against the caller's supply surface.
pub fn dispatch(
    srv: &mut LabServer,
    surface: &SupplySurface,
    binding: &CallerBinding,
    tool: &ExposureTool,
    arguments: Json,
    call_id: &Json,
) -> Json {
    // The surface's own catalogue decides visibility — a tool the
    // artifact does not carry is `unknown_tool` (never reached).
    let in_surface = surface.artifact.tools.iter().any(|t| t.name == tool.name);
    if !in_surface {
        return call_result_refusal(&SurfaceError::new(
            "unknown_tool",
            format!(
                "unknown tool `{}` on surface `{}`",
                tool.name, surface.surface_id
            ),
            Json::obj([("surface", Json::str(surface.surface_id.clone()))]),
        ));
    }
    // Π — first matching rule wins (the document's own order).
    let rule = surface
        .pi
        .iter()
        .find(|r| r.match_ == "*" || r.match_ == tool.name || r.match_ == tool.semantic_id);
    let (decision, hidden) = rule
        .map(|r| (r.decision.as_str(), r.hidden))
        .unwrap_or(("allow", false));
    if hidden {
        return call_result_refusal(&SurfaceError::new(
            "unknown_tool",
            format!("unknown tool `{}`", tool.name),
            Json::Null,
        ));
    }

    // The caller's surface session — opened lazily like a lab call's
    // (ensure consumes `&mut srv` once; every use then borrows the
    // `sessions`/`svc`/`exposure` fields disjointly).
    let bid = binding.binding_id.clone();
    if let Err(e) = srv.ensure_session(binding) {
        return call_result_refusal(&SurfaceError::new(
            "kernel_error",
            format!("surface session: {e:?}"),
            Json::Null,
        ));
    }
    let (turn_id, _tref) = match srv
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
    let mut fx = effect_ctx_for(
        &mut srv.svc,
        &srv.exposure.version_id,
        srv.sessions.get(&bid).expect("session"),
        tool,
        &arguments,
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

    let outcome: ToolOutcome = match decision {
        "deny" => {
            let err = SurfaceError::new(
                "DeniedByPolicy",
                format!("Π denies `{}` on `{}`", tool.name, surface.surface_id),
                Json::obj([
                    ("surface", Json::str(surface.surface_id.clone())),
                    ("rule", Json::str("deny")),
                ]),
            );
            let _ = fx.refused(
                &mut srv.svc,
                srv.sessions.get(&bid).expect("session"),
                &turn_id,
                &err.kind,
                err.detail.clone(),
            );
            Err(err)
        }
        "ask" => {
            // Mint the durable ask — `security.permission.pending` on
            // the caller's surface run (the SAME class the kernel uses;
            // `respond_approval` against the surface run answers it).
            let permission_id = format!("perm-{}", fx.effect_id);
            let run_id = srv.sessions[&bid].run_id.clone();
            // PENDING_FIELDS is the closed partition — the ask's
            // coordinates ride `request`/`subject_ref`/`capability_ref`/
            // `args_canonical_hash` (the §5g.7 `PermissionRequest`
            // record — the tool+surface spelling is the ask's durable
            // description).
            let requested_at = srv.svc.surface_ts_now();
            let pending = mint_event(
                &mut srv.svc,
                &run_id,
                "security.permission.pending",
                Json::obj([
                    ("permission_id", Json::str(permission_id.clone())),
                    ("effect_id", Json::str(fx.effect_id.clone())),
                    ("requested_at", Json::str(requested_at)),
                    ("subject_ref", Json::str(binding.principal_ref.clone())),
                    ("capability_ref", Json::str(tool.semantic_id.clone())),
                    ("args_canonical_hash", Json::str(fx.args_hash.clone())),
                    (
                        "request",
                        Json::obj([
                            ("subject_ref", Json::str(binding.principal_ref.clone())),
                            ("capability_ref", Json::str(tool.semantic_id.clone())),
                            ("args_canonical_hash", Json::str(fx.args_hash.clone())),
                            (
                                "reason",
                                Json::str(format!(
                                    "pi ask {} on {}",
                                    tool.name, surface.surface_id
                                )),
                            ),
                        ]),
                    ),
                ]),
                Scope {
                    turn_id: Some(turn_id.clone()),
                    effect_id: Some(fx.effect_id.clone()),
                    ..Scope::default()
                },
                vec![],
            );
            match pending {
                Ok(ev) => {
                    let s = srv.sessions.get(&bid).expect("session");
                    if let Err(e) = srv
                        .svc
                        .surface_append(&s.run_id.clone(), &s.lease, vec![ev])
                    {
                        return call_result_refusal(&SurfaceError::new(
                            "kernel_error",
                            format!("permission pending append: {e:?}"),
                            Json::Null,
                        ));
                    }
                }
                Err(e) => {
                    return call_result_refusal(&SurfaceError::new(
                        "kernel_error",
                        format!("permission pending: {e:?}"),
                        Json::Null,
                    ));
                }
            }
            let _ = fx.refused(
                &mut srv.svc,
                srv.sessions.get(&bid).expect("session"),
                &turn_id,
                "PermissionAskRequired",
                Json::obj([("permission_id", Json::str(permission_id.clone()))]),
            );
            Err(SurfaceError::new(
                "PermissionAskRequired",
                format!("`{}` requires a permission grant — answer `{permission_id}` via respond_approval", tool.name),
                Json::obj([
                    ("permission_id", Json::str(permission_id)),
                    ("surface", Json::str(surface.surface_id.clone())),
                ]),
            ))
        }
        _ => {
            // allow — `authorized → prepared → dispatch → observed`.
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
                Some(e) => Err(e),
                None => {
                    // The supply tool's wrapped op — the artifact's own
                    // `hir_meta.op` (empty ⇒ the C1 fixture answers a
                    // declared stub: `{surface, tool, echo}` — no
                    // silently-invented effects).
                    let op = surface
                        .artifact
                        .tools
                        .iter()
                        .find(|t| t.name == tool.name)
                        .and_then(|t| t.hir_meta.get("op"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    if op.is_empty() {
                        Ok(Json::obj([
                            ("surface", Json::str(surface.surface_id.clone())),
                            ("tool", Json::str(tool.name.clone())),
                            ("bundle_id", Json::str(surface.bundle_id.clone())),
                            ("echo", arguments.clone()),
                        ]))
                    } else {
                        op_call(&mut srv.svc, &op, &arguments)
                    }
                }
            }
        }
    };

    // The terminal row + charge + turn close — same spine as the lab
    // path (charges post on the CALLER's surface run).
    match &outcome {
        Ok(p) => {
            let _ = fx.observed(
                &mut srv.svc,
                srv.sessions.get(&bid).expect("session"),
                &turn_id,
                "applied",
                Some(p),
            );
        }
        Err(_) => { /* the refused row already landed in the arm */ }
    }
    if let Some(source) = fx.terminal_ref.clone().or_else(|| fx.intended_ref.clone()) {
        let _ = srv.sessions.get_mut(&bid).expect("session").charge_turn(
            &mut srv.svc,
            &source,
            hh_ontology::dimensions::DimensionId::ToolCalls,
            1,
        );
    }
    let _ = srv.sessions.get_mut(&bid).expect("session").finish_turn(
        &mut srv.svc,
        &turn_id,
        if outcome.is_ok() {
            "applied"
        } else {
            "refused"
        },
        Json::obj([
            ("tool", Json::str(tool.name.clone())),
            ("surface", Json::str(surface.surface_id.clone())),
            ("pi_decision", Json::str(decision)),
        ]),
    );
    match outcome {
        Ok(p) => crate::dispatch::call_result_ok(p),
        Err(e) => {
            let mut r = call_result_refusal(&e);
            if let Json::Obj(m) = &mut r {
                if e.kind == "PermissionAskRequired" {
                    m.insert(
                        "pending_effects".into(),
                        Json::Arr(vec![Json::str(fx.effect_id.clone())]),
                    );
                }
            }
            r
        }
    }
}

/// The `security.permission.pending` row a later `respond_approval`
/// resolves — `list_pending_approvals` on the surface run reports it.
pub fn pending_ids(events: &[hh_ledger::event::EventEnvelope]) -> Vec<String> {
    let mut pending = Vec::new();
    let mut decided = std::collections::BTreeSet::new();
    for e in events {
        match e.class.as_str() {
            "security.permission.pending" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    pending.push(id.to_string());
                }
            }
            "security.permission.decided" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    decided.insert(id.to_string());
                }
            }
            _ => {}
        }
    }
    pending
        .into_iter()
        .filter(|id| !decided.contains(id))
        .collect()
}
