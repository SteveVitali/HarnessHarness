//! The declared read group — `read_ledger`, `run_status`,
//! `get_trace`, `list_pending_approvals`.
//!
//! The ops are the embed layer's own (`read`, `project`) — the surface
//! attaches a writer session on the target run (`attach{take_over}`
//! when the recorded session died with a crashed host) and wraps the
//! answer with the `ExposurePolicy` fold + the
//! `measurement.export.delivered` mint (AC-R-2.11.3-7): every served
//! page commits `{sink_id: surface:mcp:<binding>, view_kind,
//! seq_range{from_seq,to_seq}, content_classes[]}` on the READ run —
//! the seq range IS the item count the AC pins.
//!
//! `run_status`/`list_pending_approvals` are the surface's own folds
//! (no equivalent embed op — `state` derives from the `lifecycle.run.*`
//! spine, `pending` = `security.permission.pending` minus decided).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::binding::CallerBinding;
use crate::dispatch::{op_call, SurfaceError, ToolOutcome};
use crate::exposure::ExposureDef;
use crate::exposure::ExposureTool;
use crate::session::SurfaceSession;
use hh_embed::service::EmbedService;

/// The sink spelling the delivered row records (`surface:mcp:<binding>`
/// — a distinct sink from the web instrument's `surface:<session>`;
/// both are surface-served sinks).
pub fn sink_id(binding: &CallerBinding) -> String {
    format!("surface:mcp:{}", binding.binding_id)
}

/// `read::*` declared dispatch.
pub fn dispatch(
    svc: &mut EmbedService,
    exposure: &ExposureDef,
    session: &mut SurfaceSession,
    binding: &CallerBinding,
    tool: &ExposureTool,
    args: &BTreeMap<String, Json>,
) -> ToolOutcome {
    match tool.name.as_str() {
        "read_ledger" => read_ledger(svc, exposure, session, binding, args),
        "run_status" => run_status(svc, session, args),
        "get_trace" => get_trace(svc, exposure, session, binding, args),
        "list_pending_approvals" => list_pending(svc, session, args),
        other => Err(SurfaceError::new(
            "unknown_tool",
            format!("read group has no tool `{other}`"),
            Json::Null,
        )),
    }
}

/// The run coordinate — the handle-resolved `run_id` member (the
/// `run` alias already rewrote through `arg_map`), else the literal
/// `run`/`run_id` arg.
fn run_of(args: &BTreeMap<String, Json>) -> Result<String, SurfaceError> {
    args.get("run_id")
        .and_then(Json::as_str)
        .or_else(|| args.get("run").and_then(Json::as_str))
        .map(String::from)
        .ok_or_else(|| {
            SurfaceError::new(
                "schema_violation",
                "a `run` handle (or `run_id`) is required",
                Json::obj([("required", Json::Arr(vec![Json::str("run")]))]),
            )
        })
}

/// A writer session on `run_id` — the recorded one when the handle row
/// carries it, else `attach{mode:"take_over"}` (a crashed host's
/// session dies with it; the surface always re-attaches).
fn session_on(
    svc: &mut EmbedService,
    args: &BTreeMap<String, Json>,
    run_id: &str,
) -> Result<String, SurfaceError> {
    let recorded = args
        .get("session_id")
        .and_then(Json::as_str)
        .map(String::from);
    crate::launch::session_for_run(svc, run_id, recorded.as_deref())
}

/// `read_ledger{run, cursor?, direction?, limit?, filter?}` — the
/// `read` op under the caller's session; every admitted page item
/// flows through the `readers` policy and the `delivered` mint lands.
fn read_ledger(
    svc: &mut EmbedService,
    exposure: &ExposureDef,
    session: &mut SurfaceSession,
    binding: &CallerBinding,
    args: &BTreeMap<String, Json>,
) -> ToolOutcome {
    let run_id = run_of(args)?;
    let session_id = session_on(svc, args, &run_id)?;
    // The cursor — `{kind:"seq",seq}|{kind:"event_id",event_id}|{kind:"now"}`
    // or the shorthand int/string.
    let cursor = match args.get("cursor") {
        Some(Json::Obj(_)) => args["cursor"].clone(),
        Some(Json::Int(n)) => Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(*n))]),
        Some(Json::Str(s)) => {
            if s.starts_with("evt-") || s.starts_with("ev_") {
                Json::obj([
                    ("kind", Json::str("event_id")),
                    ("event_id", Json::str(s.clone())),
                ])
            } else {
                Json::obj([
                    ("kind", Json::str("seq")),
                    ("seq", Json::Int(s.parse().unwrap_or(0))),
                ])
            }
        }
        _ => Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
    };
    let mut params = BTreeMap::new();
    params.insert("session_id".into(), Json::str(session_id));
    params.insert("cursor".into(), cursor);
    params.insert(
        "direction".into(),
        Json::str(
            args.get("direction")
                .and_then(Json::as_str)
                .unwrap_or("fwd"),
        ),
    );
    params.insert(
        "limit".into(),
        args.get("limit").cloned().unwrap_or(Json::Int(50)),
    );
    if let Some(f) = args.get("filter") {
        params.insert("filter".into(), f.clone());
    }
    let resp = op_call(svc, "read", &Json::Obj(params))?;
    // The policy fold — items are admitted under the binding's
    // `readers.content-classes`; the `withheld[]` list names what the
    // policy drops (never silently). The `read` op's `Page{events,
    // next}` shape passes through verbatim — the filtered `events`
    // list is the one member the surface replaces (AC-R-2.11.3-13's
    // "same operation records").
    let items_in = resp
        .get("events")
        .and_then(|i| match i {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut items = Vec::new();
    let mut withheld = Vec::new();
    let mut served_classes = Vec::new();
    let mut first = i64::MAX;
    let mut last = -1i64;
    for it in &items_in {
        let class = it
            .get("class")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let seq = it.get("seq").and_then(Json::as_int).unwrap_or(0);
        // A content-bearing item (offloaded payload or `refs[]`) needs
        // the L2 `content` class declared, on top of L1 `structural`.
        let carries_content = it
            .get("refs")
            .is_some_and(|r| matches!(r, Json::Arr(a) if !a.is_empty()))
            || it.get("payload_ref").is_some_and(|p| *p != Json::Null);
        if !exposure.admits_read_class(binding, "read_ledger", &class, carries_content) {
            withheld.push(Json::obj([
                ("seq", Json::Int(seq)),
                ("class", Json::str(class)),
                ("reason", Json::str("not_exposed")),
            ]));
            continue;
        }
        first = first.min(seq);
        last = last.max(seq);
        if !class.is_empty() && !served_classes.contains(&class) {
            served_classes.push(class);
        }
        items.push(it.clone());
    }
    if last < first {
        first = 0;
        last = -1;
    }
    mint_delivered(
        svc,
        &run_id,
        &sink_id(binding),
        "events",
        first,
        last,
        &served_classes,
        &session.run_id,
    );
    let mut out = match resp {
        Json::Obj(m) => m,
        other => {
            return Ok(Json::obj([
                ("events", Json::Arr(items)),
                ("withheld", Json::Arr(withheld)),
                ("raw", other),
            ]))
        }
    };
    out.insert("events".into(), Json::Arr(items));
    out.insert("withheld".into(), Json::Arr(withheld));
    out.insert("run_id".into(), Json::str(run_id));
    Ok(Json::Obj(out))
}

/// `run_status{run}` — `{state, account, pending_approvals[],
/// head_seq}`: state = the last `lifecycle.run.*` spine; pending =
/// `security.permission.pending` minus `.decided`; account = the
/// `Account` fold (AC-5 — the projection itself, never a
/// recomputation).
fn run_status(
    svc: &mut EmbedService,
    session: &mut SurfaceSession,
    args: &BTreeMap<String, Json>,
) -> ToolOutcome {
    let run_id = run_of(args)?;
    let events = svc.store().events(&run_id).map_err(|e| {
        SurfaceError::new(
            "UnknownHandle",
            format!("run_status: {e}"),
            Json::obj([("run_id", Json::str(run_id.clone()))]),
        )
    })?;
    let mut state = "open".to_string();
    let mut pending = BTreeMap::new();
    let mut decided = std::collections::BTreeSet::new();
    let mut head_seq = 0i64;
    for e in events {
        head_seq = e.seq as i64;
        match e.class.as_str() {
            "lifecycle.run.paused" => state = "paused".to_string(),
            "lifecycle.run.resumed" => state = "open".to_string(),
            "lifecycle.run.completed" | "lifecycle.run.finished" => state = "completed".to_string(),
            "lifecycle.run.cancelled" => state = "cancelled".to_string(),
            "security.permission.pending" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    pending.insert(id.to_string(), e.payload.clone());
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
    let pending_list: Vec<Json> = pending
        .iter()
        .filter(|(id, _)| !decided.contains(*id))
        .map(|(_, p)| p.clone())
        .collect();
    let account = account_json_for(svc, &run_id);
    let _ = session;
    Ok(Json::obj([
        ("run_id", Json::str(run_id)),
        ("state", Json::str(state)),
        ("head_seq", Json::Int(head_seq)),
        ("pending_approvals", Json::Arr(pending_list)),
        ("account", account),
    ]))
}

/// The `run_status` spelling a resumed `launch_run` reports — a cheap
/// state read (no account — the resume answer only carries the
/// recorded fields + live `status`).
pub fn run_status_of(svc: &mut EmbedService, run_id: &str) -> String {
    let Ok(events) = svc.store().events(run_id) else {
        return "unknown".to_string();
    };
    let mut state = "open".to_string();
    for e in events {
        match e.class.as_str() {
            "lifecycle.run.paused" => state = "paused".to_string(),
            "lifecycle.run.resumed" => state = "open".to_string(),
            "lifecycle.run.completed" | "lifecycle.run.finished" => state = "completed".to_string(),
            "lifecycle.run.cancelled" => state = "cancelled".to_string(),
            _ => {}
        }
    }
    state
}

/// `get_trace{run, view_kind?, until_seq?}` — the `project` op under
/// the caller's session (the kernel's own projection pipeline —
/// `trace_view`/`cost_view`/`metric_view` included); the policy's
/// `served_context_views` narrows what the trace may serve.
fn get_trace(
    svc: &mut EmbedService,
    exposure: &ExposureDef,
    session: &mut SurfaceSession,
    binding: &CallerBinding,
    args: &BTreeMap<String, Json>,
) -> ToolOutcome {
    let run_id = run_of(args)?;
    let kind = args
        .get("view_kind")
        .and_then(Json::as_str)
        .unwrap_or("run_summary")
        .to_string();
    let allowed = exposure.allowed_views(binding);
    if !allowed.is_empty() && !allowed.iter().any(|v| v == &kind) {
        return Err(SurfaceError::new(
            "NotExposed",
            format!(
                "view `{kind}` not exposed to binding `{}`",
                binding.binding_id
            ),
            Json::obj([("view_kind", Json::str(kind.clone()))]),
        ));
    }
    let session_id = session_on(svc, args, &run_id)?;
    let mut params = BTreeMap::new();
    params.insert("session_id".into(), Json::str(session_id));
    params.insert("view_kind".into(), Json::str(kind.clone()));
    if let Some(u) = args.get("until_seq") {
        params.insert("until_seq".into(), u.clone());
    }
    let resp = op_call(svc, "project", &Json::Obj(params))?;
    let seq_hi = svc
        .store()
        .head(&run_id)
        .map(|h| h.seq as i64)
        .unwrap_or(-1);
    mint_delivered(
        svc,
        &run_id,
        &sink_id(binding),
        &kind,
        0,
        seq_hi,
        std::slice::from_ref(&kind),
        &session.run_id,
    );
    let mut out = match resp {
        Json::Obj(m) => m,
        other => return Ok(other),
    };
    out.entry("run_id".to_string()).or_insert(Json::str(run_id));
    Ok(Json::Obj(out))
}

/// `list_pending_approvals{run}` — `security.permission.pending`
/// minus decided (the `respond_approval` surface).
fn list_pending(
    svc: &mut EmbedService,
    _session: &mut SurfaceSession,
    args: &BTreeMap<String, Json>,
) -> ToolOutcome {
    let run_id = run_of(args)?;
    let events = svc.store().events(&run_id).map_err(|e| {
        SurfaceError::new(
            "UnknownHandle",
            format!("list_pending_approvals: {e}"),
            Json::obj([("run_id", Json::str(run_id.clone()))]),
        )
    })?;
    let mut pending = BTreeMap::new();
    let mut decided = std::collections::BTreeSet::new();
    for e in events {
        match e.class.as_str() {
            "security.permission.pending" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    pending.insert(id.to_string(), e.payload.clone());
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
    let items: Vec<Json> = pending
        .iter()
        .filter(|(id, _)| !decided.contains(*id))
        .map(|(_, p)| p.clone())
        .collect();
    Ok(Json::obj([
        ("run_id", Json::str(run_id)),
        ("pending", Json::Arr(items)),
    ]))
}

/// The `Account` fold for `run_id` — `{consumed{dims}}` over the run's
/// own `control.budget.*` rows (the projection itself does the work —
/// the surface never recomputes).
fn account_json_for(svc: &mut EmbedService, run_id: &str) -> Json {
    let Ok(account) = svc.surface_account(run_id) else {
        return Json::Null;
    };
    let totals = account.totals(
        &hh_budget::account::TotalsScope::Run,
        &hh_budget::account::TotalsGroupBy {
            charged_to: true,
            ..Default::default()
        },
    );
    let mut consumed = BTreeMap::new();
    for (d, amt) in &totals.by_dimension.amounts {
        consumed.insert(d.as_str().to_string(), Json::Int(*amt));
    }
    Json::obj([("consumed", Json::Obj(consumed))])
}

/// The `measurement.export.delivered` mint — `commit_kernel_row_for`
/// on the READ run (the same closed audit-field shape
/// `work.rs::record_delivery` emits for the web instrument;
/// `loss_report_ref` stays null until a loss report exists). The seq
/// range IS the served item count (AC-R-2.11.3-7's "matching item
/// count").
#[allow(clippy::too_many_arguments)]
pub fn mint_delivered(
    svc: &mut EmbedService,
    run_id: &str,
    sink: &str,
    view_kind: &str,
    from_seq: i64,
    to_seq: i64,
    content_classes: &[String],
    _surface_run: &str,
) {
    let _ = svc.surface_commit_row(
        "kernel:surface",
        run_id,
        "measurement.export.delivered",
        Json::obj([
            ("sink_id", Json::str(sink)),
            ("view_kind", Json::str(view_kind)),
            (
                "seq_range",
                Json::obj([
                    ("from_seq", Json::Int(from_seq)),
                    ("to_seq", Json::Int(to_seq)),
                ]),
            ),
            (
                "content_classes",
                Json::Arr(
                    content_classes
                        .iter()
                        .map(|c| Json::str(c.clone()))
                        .collect(),
                ),
            ),
            ("loss_report_ref", Json::Null),
        ]),
        vec![],
        vec![],
    );
}
