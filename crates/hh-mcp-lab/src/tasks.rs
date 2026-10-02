//! The tasks carrier — `io.modelcontextprotocol/tasks` over the Lab
//! catalogue (spec §7.3 §2.5 carrier table; ADR-0175 D3/D6;
//! AC-R-2.11.3-10).
//!
//! A request whose `_meta` declares the extension gets
//! `CreateTaskResult` from the launch verbs (`launch_run`,
//! `open_experiment`) — seq-0 durability already landed inside the
//! lowering, so `taskId` binds a durable `surface_ids` alias of the
//! run/experiment, never a guess. `tasks/get | update | cancel` are
//! protocol methods (not tools — they never enter the catalogue).
//!
//! Exhaustive `StopReason`/outcome mapping (the §2.5 row):
//! - `working` ← `created | running | suspended{reason ≠ awaiting_approval}`;
//! - `input_required` ← `suspended{awaiting_approval} | pending permission` —
//!   **only** for `human_principal` bindings (`inputRequests` keyed by
//!   `permission_id`); every delegate-ceilinged kind sees `working` with
//!   `statusMessage = awaiting_principal_approval` (the approval routes
//!   through H7);
//! - `completed` ← `lifecycle.run.finished` for **every** outcome class
//!   (`refused`, `budget_exhausted`, `oracle_failure`, …) — these are
//!   results, not faults;
//! - `cancelled` ← `finished{stop_reason: cancelled}`;
//! - `failed` ← only a JSON-RPC fault of the server itself — never a
//!   refused run.
//!
//! `-32021` is the tasks-extension error code: a client that never
//! declared the capability at `initialize` gets it for any `tasks/*`
//! call or task-declaring `tools/call` (enforced in `server.rs`).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::binding::{CallerBinding, CallerKind};
use crate::server::LabServer;

/// The tasks-extension error code (MCP `tasks/*` domain).
pub const TASKS_ERROR: i64 = -32021;

/// The `_meta` key a request carries to declare the tasks extension.
pub const TASKS_META_KEY: &str = "io.modelcontextprotocol/tasks";

/// The `taskId` prefix — `task:run:<run_id>` / `task:exp:<exp_id>`
/// (a `surface_ids` alias of the durable target).
pub const TASK_RUN_PREFIX: &str = "task:run:";
/// The experiment task prefix.
pub const TASK_EXP_PREFIX: &str = "task:exp:";

/// The task poll hint (`pollIntervalMs`) the carrier advertises.
pub const TASK_POLL_MS: i64 = 1_000;
/// The task `ttl` — ≥ the run's retention floor (OQ-397's ratified
/// default: `ttl ≥ floor` only).
pub const TASK_TTL_MS: i64 = 86_400_000;

/// A registered task — the run/experiment a `taskId` names plus the
/// binding that owns it (task ids never cross bindings: a foreign
/// `taskId` is `task_unknown`, the same name-only rule handles apply).
#[derive(Debug, Clone)]
pub struct TaskRecord {
    /// The `taskId`.
    pub task_id: String,
    /// `"run"` | `"experiment"`.
    pub target_kind: String,
    /// The run/experiment id the task names.
    pub target_id: String,
    /// The binding that created it.
    pub owner_binding: String,
}

/// Wrap a successful `launch_run`/`open_experiment` result in
/// `CreateTaskResult` — `{task:{taskId,status,pollIntervalMs,ttl},
/// resultType:"task"}` — and register the task on the server (the
/// alias table `tasks/get` resolves). The wrapped result's `taskId`
/// derives from the durable target id the lowering recorded.
pub fn wrap_create_task_result(
    srv: &mut LabServer,
    binding: &CallerBinding,
    tool_name: &str,
    result: &Json,
) -> Json {
    let payload = result
        .get("structuredContent")
        .cloned()
        .unwrap_or(Json::Null);
    let (kind, target) = match tool_name {
        "launch_run" => {
            let run_id = payload
                .get("launched")
                .and_then(|l| l.get("run_id"))
                .and_then(Json::as_str)
                .or_else(|| payload.get("run_id").and_then(Json::as_str))
                .unwrap_or("")
                .to_string();
            ("run", run_id)
        }
        "open_experiment" => {
            let exp = payload
                .get("experiment_id")
                .and_then(Json::as_str)
                .or_else(|| payload.get("experiment").and_then(Json::as_str))
                .or_else(|| payload.get("handle").and_then(Json::as_str))
                .unwrap_or("")
                .to_string();
            ("experiment", exp)
        }
        _ => ("", String::new()),
    };
    if target.is_empty() {
        return result.clone();
    }
    let task_id = format!(
        "{}{}",
        if kind == "run" {
            TASK_RUN_PREFIX
        } else {
            TASK_EXP_PREFIX
        },
        target
    );
    srv.tasks.insert(
        task_id.clone(),
        TaskRecord {
            task_id: task_id.clone(),
            target_kind: kind.to_string(),
            target_id: target,
            owner_binding: binding.binding_id.clone(),
        },
    );
    Json::obj([
        ("isError", Json::Bool(false)),
        (
            "task",
            Json::obj([
                ("taskId", Json::str(task_id.clone())),
                ("status", Json::str("working")),
                ("pollIntervalMs", Json::Int(TASK_POLL_MS)),
                ("ttl", Json::Int(TASK_TTL_MS)),
            ]),
        ),
        ("resultType", Json::str("task")),
        (
            "_meta",
            Json::obj([(TASKS_META_KEY, Json::obj([("taskId", Json::str(task_id))]))]),
        ),
    ])
}

/// Resolve a `taskId` — the binding-owned registered task, or the
/// `task:run:<id>`/`task:exp:<id>` spelling when the id names a live
/// object the surface knows (a task created before a host crash
/// resolves the same way — the id is a name, the record is durable
/// underneath).
fn resolve_task(srv: &LabServer, binding: &CallerBinding, task_id: &str) -> Option<TaskRecord> {
    if let Some(t) = srv.tasks.get(task_id) {
        if t.owner_binding == binding.binding_id {
            return Some(t.clone());
        }
        return None;
    }
    if let Some(run_id) = task_id.strip_prefix(TASK_RUN_PREFIX) {
        if srv.svc.store().events(run_id).is_ok() {
            return Some(TaskRecord {
                task_id: task_id.to_string(),
                target_kind: "run".to_string(),
                target_id: run_id.to_string(),
                owner_binding: binding.binding_id.clone(),
            });
        }
    }
    if let Some(exp) = task_id.strip_prefix(TASK_EXP_PREFIX) {
        return Some(TaskRecord {
            task_id: task_id.to_string(),
            target_kind: "experiment".to_string(),
            target_id: exp.to_string(),
            owner_binding: binding.binding_id.clone(),
        });
    }
    None
}

/// The exhaustive run-envelope → task-status fold (the §2.5 mapping —
/// the AC-R-2.11.3-10 exhaustiveness point). `pending` names the run's
/// live `security.permission.pending` ids.
pub fn task_status_of(srv: &mut LabServer, binding: &CallerBinding, record: &TaskRecord) -> Json {
    let run_id = &record.target_id;
    let events = srv.svc.store().events(run_id).unwrap_or_default();
    let mut finished: Option<(String, Json)> = None;
    let mut suspended_reason: Option<String> = None;
    let mut pending: Vec<String> = Vec::new();
    let mut decided: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in events {
        match e.class.as_str() {
            "lifecycle.run.finished" | "lifecycle.run.completed" => {
                let stop = e
                    .payload
                    .get("stop_reason")
                    .and_then(Json::as_str)
                    .or_else(|| {
                        e.payload
                            .get("outcome")
                            .and_then(|o: &Json| o.get("stop_reason"))
                            .and_then(Json::as_str)
                    })
                    .unwrap_or("")
                    .to_string();
                finished = Some((stop, e.payload.clone()));
            }
            "lifecycle.run.cancelled" => {
                finished = Some(("cancelled".to_string(), e.payload.clone()));
            }
            "lifecycle.run.paused" | "lifecycle.run.suspended" => {
                suspended_reason = Some(
                    e.payload
                        .get("reason")
                        .and_then(Json::as_str)
                        .unwrap_or("suspended")
                        .to_string(),
                );
            }
            "lifecycle.run.resumed" => suspended_reason = None,
            "security.permission.pending" => {
                if let Some(pid) = e
                    .payload
                    .get("permission_id")
                    .and_then(Json::as_str)
                    .map(String::from)
                    .or_else(|| {
                        e.payload
                            .get("permission")
                            .and_then(|p: &Json| p.get("id"))
                            .and_then(Json::as_str)
                            .map(String::from)
                    })
                {
                    pending.push(pid);
                }
            }
            "security.permission.decided" => {
                if let Some(pid) = e.payload.get("permission_id").and_then(Json::as_str) {
                    decided.insert(pid.to_string());
                }
            }
            _ => {}
        }
    }
    pending.retain(|p| !decided.contains(p));
    let mut task: BTreeMap<String, Json> = BTreeMap::new();
    task.insert("taskId".into(), Json::str(record.task_id.clone()));
    task.insert("pollIntervalMs".into(), Json::Int(TASK_POLL_MS));
    task.insert("ttl".into(), Json::Int(TASK_TTL_MS));
    if let Some((stop, payload)) = &finished {
        if stop == "cancelled" {
            task.insert("status".into(), Json::str("cancelled"));
        } else {
            // `completed` for EVERY outcome class — refused /
            // budget_exhausted / oracle_failure are results, not
            // faults; `failed` is reserved for a JSON-RPC fault of the
            // server itself.
            task.insert("status".into(), Json::str("completed"));
            task.insert(
                "result".into(),
                Json::obj([
                    ("stop_reason", Json::str(stop.clone())),
                    (
                        "outcome",
                        payload.get("outcome").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "run_status",
                        Json::str(crate::reads::run_status_of(&mut srv.svc, run_id)),
                    ),
                ]),
            );
        }
    } else if !pending.is_empty() || suspended_reason.as_deref() == Some("awaiting_approval") {
        if binding.caller_kind == CallerKind::HumanPrincipal {
            task.insert("status".into(), Json::str("input_required"));
            task.insert(
                "inputRequests".to_string(),
                Json::Arr(
                    pending
                        .iter()
                        .map(|p| Json::obj([("permission_id", Json::str(p.clone()))]))
                        .collect(),
                ),
            );
        } else {
            // Delegate-ceilinged kinds never observe `input_required`
            // for an approval — the run stays `working` and the ask
            // routes to the principal through H7 (AC-R-2.11.3-3).
            task.insert("status".into(), Json::str("working"));
            task.insert(
                "statusMessage".into(),
                Json::str("awaiting_principal_approval"),
            );
        }
    } else {
        task.insert("status".into(), Json::str("working"));
        if let Some(r) = suspended_reason {
            task.insert("statusMessage".into(), Json::str(format!("suspended:{r}")));
        }
    }
    Json::Obj(task)
}

/// Push one narrowed `notifications/tasks` — the `taskId` only (the
/// client re-reads `tasks/get`; the same `narrowed` discipline
/// `notifications/resources/updated` follows, and the caller runs this
/// only after the underlying row is durable — ADR-0175 D3).
pub fn push_task_notification(srv: &mut LabServer, task_id: &str) {
    srv.pending.push_back(Json::obj([
        ("method", Json::str("notifications/tasks")),
        ("params", Json::obj([("taskId", Json::str(task_id))])),
    ]));
}

/// Push `notifications/tasks` for every task the calling binding owns
/// whose target run is in `touched` — the run-state → tasks push arm of
/// the §05d lowering (S5.8; R-2.2.3²). A binding that never declared
/// `capabilities.tasks` has no sink and gets nothing (the rows stay
/// durable; `tasks/get` re-reads them — never a fabricated push).
pub fn push_for_runs(
    srv: &mut LabServer,
    binding: &CallerBinding,
    touched: &std::collections::BTreeSet<String>,
) {
    if !srv.client_tasks.contains(&binding.binding_id) {
        return;
    }
    let tids: Vec<String> = srv
        .tasks
        .iter()
        .filter(|(_, rec)| {
            rec.owner_binding == binding.binding_id
                && rec.target_kind == "run"
                && touched.contains(&rec.target_id)
        })
        .map(|(tid, _)| tid.clone())
        .collect();
    for tid in tids {
        push_task_notification(srv, &tid);
    }
}

/// The `control.wakeup.*` → `notifications/tasks` lowering pass
/// (R-2.2.3²; ADR-0175 D3; §05d's push-notification config): for every
/// run-bound task the binding owns, drain the target run's due wakeups
/// through `EmbedService::deliver_due_wakeups` — the kernel-internal
/// trigger pass lands `control.wakeup.occurred`/`fired` durable-first
/// under the writer session the launch mint recorded, and each task
/// with a *fresh* delivery gets one narrowed push (delivery dedup is
/// the session's `delivered_wokens` set — the same key the run-loop
/// drain uses, so a surface poll never double-delivers).
///
/// Recovery posture: an `unknown_session` — the recorded writer is
/// provably gone — re-attaches `attach{mode:take_over}` once (the same
/// documented recovery `control_passthrough` uses); a `SessionDetached`
/// means a live writer holds the run and delivers on its own cadence
/// (skipped, never double-delivered); any other refusal skips — the
/// `fired` rows stay durable and deliverable, nothing is dropped.
pub fn drain_due_wakeups(srv: &mut LabServer, binding: &CallerBinding) {
    if !srv.client_tasks.contains(&binding.binding_id) {
        return;
    }
    // (task_id, run_id, writer session) — collected before the mutable
    // service borrow the drain needs.
    let mut targets: Vec<(String, String, String)> = Vec::new();
    if let Some(sess) = srv.sessions.get(&binding.binding_id) {
        for (tid, rec) in &srv.tasks {
            if rec.owner_binding != binding.binding_id || rec.target_kind != "run" {
                continue;
            }
            let sid = sess.handles.by_alias.values().find_map(|e| {
                if e.target_id != rec.target_id {
                    return None;
                }
                e.payload
                    .as_ref()
                    .and_then(|p| p.get("session_id"))
                    .and_then(Json::as_str)
                    .map(String::from)
            });
            if let Some(sid) = sid {
                targets.push((tid.clone(), rec.target_id.clone(), sid));
            }
        }
    }
    for (tid, run_id, sid) in targets {
        let fresh = match srv.svc.deliver_due_wakeups(&sid) {
            Ok(w) => w,
            Err(hh_embed_schema::errors::EmbedError::UnknownSession) => {
                // A wakeup-driven reattach reports `cause: "wakeup"` —
                // the durable `lifecycle.run.resumed{recovery_decision.
                // cause}` the hosted-resume mapping requires
                // (AC-R-2.2.3-13).
                match crate::launch::attach_take_over_caused(&mut srv.svc, &run_id, Some("wakeup"))
                {
                    Ok(new_sid) => srv.svc.deliver_due_wakeups(&new_sid).unwrap_or_default(),
                    Err(_) => Vec::new(),
                }
            }
            Err(_) => Vec::new(),
        };
        if !fresh.is_empty() {
            push_task_notification(srv, &tid);
        }
    }
}

/// The `tasks/*` method dispatch — returns `(result_json, error)`.
/// Callers wrap in `result_frame`/`error_frame` (`-32021` errors are
/// produced by the caller on capability misses — these bodies assume
/// the capability was declared).
pub fn dispatch(
    srv: &mut LabServer,
    binding: &CallerBinding,
    method: &str,
    params: &Json,
) -> Result<Json, (i64, String)> {
    // The tasks carrier is also the wakeup sink (R-2.2.3²): every
    // `tasks/*` call drains the binding's task-bound runs first, so a
    // `tasks/get` status read reflects `control.wakeup.fired` rows that
    // materialised since the last call, and any *other* task that moved
    // gets its narrowed `notifications/tasks` push.
    drain_due_wakeups(srv, binding);
    if method == "tasks/list" {
        let records: Vec<TaskRecord> = srv
            .tasks
            .values()
            .filter(|t| t.owner_binding == binding.binding_id)
            .cloned()
            .collect();
        let tasks: Vec<Json> = records
            .iter()
            .map(|t| task_status_of(srv, binding, t))
            .collect();
        return Ok(Json::obj([
            ("tasks", Json::Arr(tasks)),
            ("resultType", Json::str("complete")),
        ]));
    }
    let task_id = params
        .get("taskId")
        .and_then(Json::as_str)
        .ok_or_else(|| (-32602, format!("{method} requires `taskId`")))?
        .to_string();
    match method {
        "tasks/get" => {
            let record = resolve_task(srv, binding, &task_id).ok_or_else(|| {
                (
                    TASKS_ERROR,
                    format!("task_unknown: `{task_id}` names no task this binding owns"),
                )
            })?;
            Ok(task_status_of(srv, binding, &record))
        }
        "tasks/update" => {
            // `inputResponses[]` → `respond_approval` under R-2 — the
            // caller-kind gate is identical to the tool's: a delegate
            // answer is `IllegitimateEndorsement`, never applied.
            if !binding.caller_kind.may_respond_approval() {
                return Err((
                    TASKS_ERROR,
                    format!(
                        "IllegitimateEndorsement: caller_kind `{}` cannot answer an \
                         input request — approvals are human_principal-only",
                        binding.caller_kind.as_str()
                    ),
                ));
            }
            let record = resolve_task(srv, binding, &task_id).ok_or_else(|| {
                (
                    TASKS_ERROR,
                    format!("task_unknown: `{task_id}` names no task this binding owns"),
                )
            })?;
            if record.target_kind != "run" {
                return Err((
                    TASKS_ERROR,
                    "task_not_input_capable: the task names no run".to_string(),
                ));
            }
            let responses = params
                .get("inputResponses")
                .and_then(|v| match v {
                    Json::Arr(a) => Some(a.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let mut answered = Vec::new();
            for r in &responses {
                let permission_id = r
                    .get("permission_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let outcome = r
                    .get("outcome")
                    .and_then(Json::as_str)
                    .map(|o| match o {
                        "approved" | "allow" | "allowed_once" => "grant",
                        "denied" => "deny",
                        "deferred" => "defer",
                        other => other,
                    })
                    .unwrap_or("grant")
                    .to_string();
                let args = Json::obj([
                    ("run_id", Json::str(record.target_id.clone())),
                    ("permission_id", Json::str(permission_id.clone())),
                    ("outcome", Json::str(outcome)),
                    (
                        "session_id",
                        Json::str(
                            crate::launch::attach_take_over(&mut srv.svc, &record.target_id)
                                .unwrap_or_default(),
                        ),
                    ),
                    (
                        "idempotency_key",
                        Json::str(format!("tasks.update:{}:{}", task_id, permission_id)),
                    ),
                ]);
                match crate::dispatch::op_call(&mut srv.svc, "respond_permission", &args) {
                    Ok(v) => answered.push(Json::obj([
                        ("permission_id", Json::str(permission_id)),
                        ("answered", Json::Bool(true)),
                        ("result", v),
                    ])),
                    Err(e) => answered.push(Json::obj([
                        ("permission_id", Json::str(permission_id)),
                        ("answered", Json::Bool(false)),
                        ("refusal", Json::str(e.kind)),
                    ])),
                }
            }
            let mut task = task_status_of(srv, binding, &record);
            if let Json::Obj(m) = &mut task {
                m.insert("answered".into(), Json::Arr(answered));
            }
            Ok(task)
        }
        "tasks/cancel" => {
            // `cancel_run` — cooperative: a run that finished first
            // reports its real terminal (the mapping reads durable
            // state after the cancel lands).
            let record = resolve_task(srv, binding, &task_id).ok_or_else(|| {
                (
                    TASKS_ERROR,
                    format!("task_unknown: `{task_id}` names no task this binding owns"),
                )
            })?;
            if record.target_kind == "run" {
                let session_id = crate::launch::attach_take_over(&mut srv.svc, &record.target_id)
                    .unwrap_or_default();
                let args = Json::obj([
                    ("run_id", Json::str(record.target_id.clone())),
                    ("session_id", Json::str(session_id)),
                    ("scope", Json::obj([("kind", Json::str("run"))])),
                    (
                        "idempotency_key",
                        Json::str(format!("tasks.cancel:{task_id}")),
                    ),
                ]);
                let _ = crate::dispatch::op_call(&mut srv.svc, "cancel", &args);
            } else {
                let args = Json::obj([
                    ("experiment_id", Json::str(record.target_id.clone())),
                    (
                        "idempotency_key",
                        Json::str(format!("tasks.cancel:{task_id}")),
                    ),
                ]);
                let _ = crate::dispatch::op_call(&mut srv.svc, "lab.experiment.close", &args);
            }
            Ok(task_status_of(srv, binding, &record))
        }
        other => Err((-32601, format!("method_not_found: {other}"))),
    }
}
