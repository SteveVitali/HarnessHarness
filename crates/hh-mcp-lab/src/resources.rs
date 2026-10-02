//! The ledger-resource carrier — `resources/list | templates/list |
//! read | subscribe | unsubscribe` + `notifications/resources/updated`
//! (spec §7.3 §2.5 carrier table; ADR-0175 D2/D6).
//!
//! `ResourceTemplate`s (the `hh://` scheme is the ADR-0210 placeholder —
//! `hh://run/{run_handle}/ledger`, `hh://run/{run_handle}/status`,
//! `hh://results/{configuration_id}`):
//! - `resources/read` serves the same `ExposurePolicy`-folded page the
//!   `read_ledger`/`run_status`/`query_rows` tools serve — the carrier
//!   is a *lowering of the run envelope*, never a second read path;
//! - a subscription is **bound to the caller's binding** — binding
//!   change or expiry cancels it (the server drops the set when the
//!   binding stops being live);
//! - `notifications/resources/updated` fires only after the underlying
//!   event is durable — the notification is `narrowed` (it carries the
//!   URI, never the payload; `resources/read` serves the content);
//! - the server is a declared exporter/subscriber, never a store —
//!   every served page is the same sink-folded read the tools run.

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::binding::CallerBinding;
use crate::dispatch::{op_call, SurfaceError};
use crate::server::LabServer;

/// The `hh://` scheme placeholder (ADR-0210 — WS-L6 allocates the real
/// scheme; the carrier's shape is already the contract).
pub const HH_SCHEME: &str = "hh://";

/// The resource URI templates the server advertises on
/// `resources/templates/list`.
pub fn templates() -> Json {
    Json::obj([
        (
            "resourceTemplates",
            Json::Arr(vec![
                Json::obj([
                    ("uriTemplate", Json::str("hh://run/{run_handle}/ledger")),
                    ("name", Json::str("run-ledger")),
                    ("description", Json::str(
                        "A run's durable ledger — `resources/read` honours `from_seq`; the page is the ExposurePolicy-folded read_ledger page.",
                    )),
                    ("mimeType", Json::str("application/json")),
                ]),
                Json::obj([
                    ("uriTemplate", Json::str("hh://run/{run_handle}/status")),
                    ("name", Json::str("run-status")),
                    ("description", Json::str(
                        "The run's RunStatusView — status, pending_approvals, account, head_seq.",
                    )),
                    ("mimeType", Json::str("application/json")),
                ]),
                Json::obj([
                    ("uriTemplate", Json::str("hh://results/{configuration_id}")),
                    ("name", Json::str("results")),
                    ("description", Json::str(
                        "A results-store query for one configuration — the query_rows projection.",
                    )),
                    ("mimeType", Json::str("application/json")),
                ]),
            ]),
        ),
        ("resultType", Json::str("complete")),
    ])
}

/// Parse a `hh://` resource URI → `(kind, key)` where kind ∈
/// `run/ledger | run/status | results`.
fn parse_uri(uri: &str) -> Option<(String, String)> {
    let rest = uri.strip_prefix(HH_SCHEME)?;
    if let Some(r) = rest.strip_prefix("run/") {
        if let Some(h) = r.strip_suffix("/ledger") {
            return Some(("run-ledger".to_string(), h.to_string()));
        }
        if let Some(h) = r.strip_suffix("/status") {
            return Some(("run-status".to_string(), h.to_string()));
        }
        return None;
    }
    if let Some(c) = rest.strip_prefix("results/") {
        return Some(("results".to_string(), c.to_string()));
    }
    None
}

/// Resolve a run name — a minted alias (`hnd-run-…`/`run:<id>`), the
/// `task:run:<id>` carrier spelling, or the bare `run_id` (the read
/// ops' own `UnknownHandle` gates misuse).
fn resolve_run(
    srv: &mut LabServer,
    binding: &CallerBinding,
    name: &str,
) -> Result<String, SurfaceError> {
    let bare = name.strip_prefix("run:").unwrap_or(name);
    // The caller's own minted aliases resolve through the table; a bare
    // run id the surface minted resolves by the alias' deterministic
    // name — a foreign run's id resolves to nothing here (the read op
    // still applies ExposurePolicy + the handle rule).
    let bid = binding.binding_id.clone();
    if let Some(session) = srv.sessions.get(&bid) {
        for candidate in [
            name.to_string(),
            format!("run:{bare}"),
            crate::handles::handle_alias(crate::handles::HandleKind::Run, bare),
        ] {
            if let Ok(entry) =
                session
                    .handles
                    .resolve(&candidate, &bid, srv.svc.surface_now_ms().max(0) as u64)
            {
                return Ok(entry.target_id.clone());
            }
        }
    }
    if srv.svc.store().events(bare).is_ok() {
        return Ok(bare.to_string());
    }
    Err(SurfaceError::new(
        "UnknownHandle",
        format!("resource run `{name}` names no handle this surface minted"),
        Json::obj([("run", Json::str(name))]),
    ))
}

/// `resources/list` — the binding's own minted objects as resources
/// (the catalogue invariant holds for the *template* list; instances
/// enumerate the caller's own handles — names it already holds).
pub fn list(srv: &mut LabServer, binding: &CallerBinding) -> Json {
    let bid = binding.binding_id.clone();
    let mut resources = Vec::new();
    if let Some(session) = srv.sessions.get(&bid) {
        for (alias, entry) in &session.handles.by_alias {
            let name = match entry.kind {
                crate::handles::HandleKind::Run => [
                    Json::obj([
                        ("uri", Json::str(format!("hh://run/{alias}/ledger"))),
                        ("name", Json::str(format!("ledger:{alias}"))),
                        ("mimeType", Json::str("application/json")),
                    ]),
                    Json::obj([
                        ("uri", Json::str(format!("hh://run/{alias}/status"))),
                        ("name", Json::str(format!("status:{alias}"))),
                        ("mimeType", Json::str("application/json")),
                    ]),
                ]
                .into_iter()
                .collect::<Vec<_>>(),
                _ => vec![],
            };
            resources.extend(name);
        }
    }
    Json::obj([
        ("resources", Json::Arr(resources)),
        ("resultType", Json::str("complete")),
    ])
}

/// `resources/read{uri, from_seq?}` — one `contents[]` page.
pub fn read(
    srv: &mut LabServer,
    binding: &CallerBinding,
    params: &Json,
) -> Result<Json, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(Json::as_str)
        .ok_or_else(|| (-32602, "resources/read requires `uri`".to_string()))?
        .to_string();
    let (kind, key) =
        parse_uri(&uri).ok_or_else(|| (-32002, format!("resource_not_found: `{uri}`")))?;
    let payload = match kind.as_str() {
        "run-ledger" => {
            let run_id = resolve_run(srv, binding, &key)
                .map_err(|e| (-32002, format!("{}: {}", e.kind, e.message)))?;
            // The same `read` path `read_ledger` runs — an attach
            // session + the seq cursor; the policy fold is the op's.
            let session_id = crate::launch::attach_take_over(&mut srv.svc, &run_id)
                .map_err(|e| (-32002, format!("{}: {}", e.kind, e.message)))?;
            let p = Json::obj([
                ("session_id", Json::str(session_id)),
                (
                    "cursor",
                    Json::obj([
                        ("kind", Json::str("seq")),
                        (
                            "seq",
                            Json::Int(params.get("from_seq").and_then(Json::as_int).unwrap_or(0)),
                        ),
                    ]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(64)),
            ]);
            op_call(&mut srv.svc, "read", &p)
                .map_err(|e| (-32002, format!("{}: {}", e.kind, e.message)))?
        }
        "run-status" => {
            let run_id = resolve_run(srv, binding, &key)
                .map_err(|e| (-32002, format!("{}: {}", e.kind, e.message)))?;
            Json::obj([
                ("run_id", Json::str(run_id.clone())),
                (
                    "status",
                    Json::str(crate::reads::run_status_of(&mut srv.svc, &run_id)),
                ),
            ])
        }
        "results" => op_call(
            &mut srv.svc,
            "lab.results.query_rows",
            &Json::obj([(
                "query",
                Json::obj([("configuration_id", Json::str(key.clone()))]),
            )]),
        )
        .map_err(|e| (-32002, format!("{}: {}", e.kind, e.message)))?,
        _ => return Err((-32002, format!("resource_not_found: `{uri}`"))),
    };
    Ok(Json::obj([(
        "contents",
        Json::Arr(vec![Json::obj([
            ("uri", Json::str(uri)),
            ("mimeType", Json::str("application/json")),
            ("text", Json::str(payload.to_canonical_string())),
        ])]),
    )]))
}

/// `resources/subscribe{uri}` — the subscription binds to the caller's
/// binding; an unknown URI shape refuses `-32002`. Returns `{}`.
pub fn subscribe(
    srv: &mut LabServer,
    binding: &CallerBinding,
    params: &Json,
) -> Result<Json, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(Json::as_str)
        .ok_or_else(|| (-32602, "resources/subscribe requires `uri`".to_string()))?
        .to_string();
    if parse_uri(&uri).is_none() {
        return Err((-32002, format!("resource_not_found: `{uri}`")));
    }
    srv.subscriptions
        .entry(binding.binding_id.clone())
        .or_default()
        .insert(uri);
    Ok(Json::obj([]))
}

/// `resources/unsubscribe{uri}` — idempotent.
pub fn unsubscribe(
    srv: &mut LabServer,
    binding: &CallerBinding,
    params: &Json,
) -> Result<Json, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(Json::as_str)
        .ok_or_else(|| (-32602, "resources/unsubscribe requires `uri`".to_string()))?;
    if let Some(set) = srv.subscriptions.get_mut(&binding.binding_id) {
        set.remove(uri);
    }
    Ok(Json::obj([]))
}

/// A binding's expiry/change cancels its subscriptions (ADR-0175 D2 —
/// "a subscription is bound to the caller's binding and cancelled when
/// the binding changes or expires"; the Codex event-stream precedent).
pub fn cancel_for_binding(srv: &mut LabServer, binding_id: &str) {
    srv.subscriptions.remove(binding_id);
}

/// The post-call notification pass — after a `tools/call` whose
/// arguments or result name a subscribed run's URI, enqueue
/// `notifications/resources/updated` (the `narrowed` notification —
/// the URI only). The call's effect chain already closed, so the
/// underlying event is durable before the notification lands.
pub fn notify_after_call(
    srv: &mut LabServer,
    binding: &CallerBinding,
    _tool: &str,
    arguments: &Json,
    result: &Json,
) {
    // The run ids this call touched — the `run`/`run_id`/`run_handle`
    // argument spellings plus any `run_id`/`run:<id>` members the
    // result payload carries. Computed before the subscription gate —
    // the tasks carrier and the wakeup drain consume the same set and
    // are independent of `resources/subscribe`.
    let mut touched: BTreeSet<String> = BTreeSet::new();
    if let Json::Obj(m) = arguments {
        for (k, v) in m {
            if matches!(k.as_str(), "run" | "run_id" | "run_handle" | "taskId") {
                if let Some(s) = v.as_str() {
                    touched.insert(s.to_string());
                }
            }
        }
    }
    collect_run_ids(result, &mut touched);
    if let Some(subs) = srv.subscriptions.get(&binding.binding_id) {
        let subs = subs.clone();
        for uri in subs {
            if let Some((kind, key)) = parse_uri(&uri) {
                let named = key.clone();
                let hit = touched.iter().any(|t| {
                    t == &named
                        || format!("run:{t}") == named
                        || t.strip_prefix("run:") == Some(named.as_str())
                        || crate::handles::handle_alias(crate::handles::HandleKind::Run, t) == named
                });
                if hit && kind.starts_with("run-") {
                    srv.pending.push_back(Json::obj([
                        ("method", Json::str("notifications/resources/updated")),
                        ("params", Json::obj([("uri", Json::str(uri.clone()))])),
                    ]));
                }
            }
        }
    }
    // A run the tasks carrier tracks also notifies `notifications/tasks`
    // — the same narrowed/durable-first rule (S5.8; R-2.2.3²).
    crate::tasks::push_for_runs(srv, binding, &touched);
    // `control.wakeup.*` → `notifications/tasks` — drain due wakeups on
    // the binding's task-bound runs; a fresh `fired` delivery pushes the
    // task's narrowed notification (the client re-reads `tasks/get`).
    crate::tasks::drain_due_wakeups(srv, binding);
}

/// Depth-first `run_id`/`launched.run_id` member scan on a result.
fn collect_run_ids(v: &Json, out: &mut BTreeSet<String>) {
    match v {
        Json::Obj(m) => {
            for (k, x) in m {
                if k == "run_id" {
                    if let Some(s) = x.as_str() {
                        out.insert(s.to_string());
                    }
                }
                collect_run_ids(x, out);
            }
        }
        Json::Arr(a) => {
            for x in a {
                collect_run_ids(x, out);
            }
        }
        _ => {}
    }
}

/// The subscription URIs a binding holds (test/diagnostic surface).
pub fn subscriptions_of(srv: &LabServer, binding_id: &str) -> BTreeSet<String> {
    srv.subscriptions
        .get(binding_id)
        .cloned()
        .unwrap_or_default()
}
