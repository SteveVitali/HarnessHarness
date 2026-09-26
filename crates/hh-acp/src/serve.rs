//! `serve_session` — the agent-side ACP session loop (§5d.4
//! D1–D6). The loop is records-in/records-out: it reads JSON-RPC
//! frames off the [`SessionTransport`], dispatches `initialize`,
//! `session/*`, and the advertised `_hh/*` methods to the
//! [`SessionDriver`], renders the driver's *durable* event stream to
//! `session/update` notifications on the separate channel, and
//! round-trips `security.permission.requested` events as
//! `session/request_permission` requests whose responses it
//! forwards to the driver verbatim (the adapter never decides — Π
//! lives on the client/kernel side).

use std::collections::BTreeMap;

use hh_compiler::acp::{lower_event, AcpDialect};
use hh_mcp::protocol::NegotiateError;
use hh_wire::json::Json;

use crate::artifact::{render_session, AcpArtifact};
use crate::driver::SessionDriver;
use crate::permission::PermissionOutcome;
use crate::protocol::{negotiate_era, AcpEra};
use crate::transport::SessionTransport;

/// `ServeError` — the serve loop's typed failures.
#[derive(Debug)]
pub enum ServeError {
    /// The channel faulted.
    Transport(String),
    /// The driver refused/faulted.
    Driver(String),
    /// The peer offered nothing in the pinned support set (the typed
    /// refusal — `control.protocol.mismatch` spelling).
    Negotiate(NegotiateError),
}

impl ServeError {
    /// The ledger-safe spelling.
    pub fn refusal(&self) -> String {
        match self {
            ServeError::Transport(d) => format!("transport/{d}"),
            ServeError::Driver(d) => format!("driver/{d}"),
            ServeError::Negotiate(e) => format!("protocol/{}", e.refusal()),
        }
    }
}

/// JSON-RPC `-32601` — method not found (unknown methods *and*
/// unknown `_`-prefixed extension ids — T3 preserves them in the
/// artefact's `ext`, but an unimplemented method is never silently
/// dropped: the protocol error is the honest answer).
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC `-32602` — invalid params.
const INVALID_PARAMS: i64 = -32602;
/// `-32000` — server-side refusal (driver refused / session unknown).
const SERVER_ERROR: i64 = -32000;

/// One open session's state (the dialect it negotiated + whether it
/// is closed — closed sessions keep the durable record but stop
/// answering `session/*`).
struct OpenSession {
    /// `session/close` landed.
    closed: bool,
}

/// `serve_session(artifact, driver, transport)` — run the loop until
/// the channel closes (`recv → None`). Blocks on
/// `session/request_permission` responses inline (the in-memory
/// fixture channel makes the round-trip synchronous — a real
/// transport would interleave, the record ordering is identical).
pub fn serve_session<T: SessionTransport, D: SessionDriver>(
    artifact: &AcpArtifact,
    driver: &mut D,
    transport: &mut T,
) -> Result<(), ServeError> {
    let mut era = AcpEra::V2;
    let mut initialized = false;
    let mut sessions: BTreeMap<String, OpenSession> = BTreeMap::new();
    let mut perm_seq: u64 = 0;

    loop {
        let frame = match transport.recv() {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(()),
            Err(e) => return Err(ServeError::Transport(e)),
        };
        let method = match frame.get("method").and_then(Json::as_str) {
            Some(m) => m.to_string(),
            None => continue, // a response straggler — nothing to do
        };
        if method.starts_with("notifications/") {
            continue; // notifications carry no response
        }
        let id = frame.get("id").cloned().unwrap_or(Json::Null);
        let params = frame.get("params").cloned().unwrap_or(Json::obj([]));

        match method.as_str() {
            "initialize" => {
                let offered = params
                    .get("protocolVersion")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                match negotiate_era(offered) {
                    Ok(e) => {
                        era = e;
                        initialized = true;
                        send_result(transport, id, artifact.initialize_response(e.dialect()))?;
                    }
                    Err(ne) => {
                        send_error(transport, id, METHOD_NOT_FOUND, ne.refusal())?;
                    }
                }
            }
            "session/new" => {
                if !initialized {
                    send_error(transport, id, INVALID_PARAMS, "initialize required first")?;
                    continue;
                }
                match driver.new_session(&params) {
                    Ok(info) => {
                        sessions.insert(info.session_id.clone(), OpenSession { closed: false });
                        let declared = info.mcp_servers_declared.clone();
                        let has_servers = match &declared {
                            Json::Arr(a) => !a.is_empty(),
                            _ => false,
                        };
                        let mut result = BTreeMap::new();
                        result.insert("sessionId".to_string(), Json::str(info.session_id.clone()));
                        result.insert("configOptions".to_string(), info.config_options);
                        if has_servers {
                            // H5 — declared `mcpServers` sit in
                            // `pending_review`: never admitted to
                            // tools until the gate resolves them.
                            result.insert("mcp_servers_pending_review".to_string(), declared);
                            result.insert("admission".to_string(), Json::str("h5_review_required"));
                            result.insert("tools_unverified".to_string(), Json::Bool(true));
                        }
                        send_result(transport, id, Json::Obj(result))?;
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "session/prompt" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !is_open(&sessions, &session_id) {
                    send_error(transport, id, SERVER_ERROR, "session/unknown_or_closed")?;
                    continue;
                }
                let content = params.get("content").cloned().unwrap_or(Json::obj([]));
                match driver.prompt(&session_id, &content) {
                    Ok(drive) => {
                        // Render the durable events in order; a
                        // `security.permission.requested` event
                        // becomes a blocking
                        // `session/request_permission` — the
                        // response routes to the driver verbatim.
                        if let Err(e) = emit_events(
                            transport,
                            driver,
                            &session_id,
                            &drive.events,
                            era.dialect(),
                            &mut perm_seq,
                        ) {
                            send_error(transport, id, SERVER_ERROR, &e)?;
                            continue;
                        }
                        let mut result = BTreeMap::new();
                        result.insert(
                            "stopReason".to_string(),
                            Json::str(drive.stop_reason.clone()),
                        );
                        send_result(transport, id, Json::Obj(result))?;
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "session/cancel" => {
                let session_id = session_id_of(&params);
                if !is_open(&sessions, &session_id) {
                    send_error(transport, id, SERVER_ERROR, "session/unknown_or_closed")?;
                    continue;
                }
                match driver.cancel(&session_id) {
                    Ok(events) => {
                        for u in render_session(&events, era.dialect()) {
                            transport
                                .send(u.to_notification(&session_id))
                                .map_err(ServeError::Transport)?;
                        }
                        send_result(transport, id, Json::obj([]))?;
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "session/resume" => {
                let session_id = session_id_of(&params);
                let replay_from = params
                    .get("replayFrom")
                    .and_then(Json::as_int)
                    .map(|i| i as u64);
                match driver.resume(&session_id, replay_from) {
                    Ok(events) => {
                        let updates = render_session(&events, era.dialect());
                        let n = updates.len();
                        for u in updates {
                            transport
                                .send(u.to_notification(&session_id))
                                .map_err(ServeError::Transport)?;
                        }
                        send_result(
                            transport,
                            id,
                            Json::obj([("replayed", Json::Int(n as i64))]),
                        )?;
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "session/fork" => {
                let session_id = session_id_of(&params);
                let at_seq = params.get("atSeq").and_then(Json::as_int).map(|i| i as u64);
                match driver.resume(&session_id, None) {
                    Ok(events) => {
                        let prefix: Vec<(u64, String, Json)> = events
                            .into_iter()
                            .filter(|(seq, _, _)| at_seq.map(|a| *seq <= a).unwrap_or(true))
                            .collect();
                        let history = Json::Arr(
                            prefix
                                .iter()
                                .map(|(seq, class, payload)| {
                                    Json::obj([
                                        ("seq", Json::Int(*seq as i64)),
                                        ("class", Json::str(class.clone())),
                                        ("payload", payload.clone()),
                                    ])
                                })
                                .collect(),
                        );
                        let mut p = BTreeMap::new();
                        p.insert("history".to_string(), history);
                        match driver.new_session(&Json::Obj(p)) {
                            Ok(info) => {
                                sessions
                                    .insert(info.session_id.clone(), OpenSession { closed: false });
                                send_result(
                                    transport,
                                    id,
                                    Json::obj([("sessionId", Json::str(info.session_id.clone()))]),
                                )?;
                            }
                            Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                        }
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "session/close" => {
                let session_id = session_id_of(&params);
                match driver.close(&session_id) {
                    Ok(()) => {
                        if let Some(s) = sessions.get_mut(&session_id) {
                            s.closed = true;
                        }
                        send_result(transport, id, Json::obj([]))?;
                    }
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "_hh/ledger/read" => {
                if !artifact.advertises("_hh/ledger/read") {
                    send_error(transport, id, METHOD_NOT_FOUND, &method)?;
                    continue;
                }
                let from_seq = params
                    .get("from_seq")
                    .and_then(Json::as_int)
                    .map(|i| i as u64)
                    .unwrap_or(0);
                match driver.ledger_read(from_seq) {
                    Ok(tail) => send_result(transport, id, tail)?,
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "_hh/account" => {
                if !artifact.advertises("_hh/account") {
                    send_error(transport, id, METHOD_NOT_FOUND, &method)?;
                    continue;
                }
                match driver.account() {
                    Ok(a) => send_result(transport, id, a)?,
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            "_hh/participant/describe" => {
                if !artifact.advertises("_hh/participant/describe") {
                    send_error(transport, id, METHOD_NOT_FOUND, &method)?;
                    continue;
                }
                match driver.participant_describe(&params) {
                    Ok(d) => send_result(transport, id, d)?,
                    Err(e) => send_error(transport, id, SERVER_ERROR, &e)?,
                }
            }
            other => {
                send_error(transport, id, METHOD_NOT_FOUND, other)?;
            }
        }
    }
}

/// Emit the durable events as updates; on a `security.permission.
/// requested` event, send `session/request_permission`, block for
/// the response, forward the outcome to the driver verbatim, and
/// render the resolution's appended events inline before continuing
/// (durable-before-visible ordering is preserved — every update
/// renders from a durable event).
fn emit_events<T: SessionTransport, D: SessionDriver>(
    transport: &mut T,
    driver: &mut D,
    session_id: &str,
    events: &[(u64, String, Json)],
    dialect: AcpDialect,
    perm_seq: &mut u64,
) -> Result<(), String> {
    for (seq, class, payload) in events {
        let lowered = lower_event(class, payload);
        if let Some((kind, rendered)) = lowered {
            if kind == "session/request_permission" {
                if !dialect.admits(&kind) {
                    continue;
                }
                *perm_seq += 1;
                let req_id = Json::str(format!("perm-{}-{}", session_id, *perm_seq));
                let mut wire = BTreeMap::new();
                wire.insert("sessionId".to_string(), Json::str(session_id.to_string()));
                wire.insert(
                    "toolCall".to_string(),
                    payload.get("tool_call").cloned().unwrap_or_else(|| {
                        let mut tc = BTreeMap::new();
                        tc.insert(
                            "toolName".to_string(),
                            payload
                                .get("tool_name")
                                .cloned()
                                .unwrap_or(Json::str("unknown")),
                        );
                        tc.insert(
                            "effectId".to_string(),
                            payload.get("effect_id").cloned().unwrap_or(Json::str("")),
                        );
                        Json::Obj(tc)
                    }),
                );
                if let Json::Obj(m) = &rendered {
                    for (k, v) in m {
                        wire.insert(k.clone(), v.clone());
                    }
                }
                transport.send(Json::obj([
                    ("jsonrpc", Json::str("2.0")),
                    ("id", req_id.clone()),
                    ("method", Json::str("session/request_permission")),
                    ("params", Json::Obj(wire)),
                ]))?;
                // Block for the response — protocol errors and
                // non-allow outcomes both route to the driver
                // verbatim (deny lands `permission_refused`
                // driver-side; the tool call never runs).
                let outcome = await_permission_response(transport, &req_id)?;
                let resolution = driver
                    .permission_decision(
                        session_id,
                        req_id.as_str().unwrap_or_default(),
                        &outcome.to_wire(),
                    )
                    .map_err(|e| format!("driver/{e}"))?;
                for u in render_session(&resolution, dialect) {
                    transport
                        .send(u.to_notification(session_id))
                        .map_err(|e| format!("transport/{e}"))?;
                }
            } else if dialect.admits(&kind) {
                transport
                    .send(
                        crate::artifact::SessionUpdate {
                            source_seq: *seq,
                            kind,
                            params: rendered,
                        }
                        .to_notification(session_id),
                    )
                    .map_err(|e| format!("transport/{e}"))?;
            }
        }
    }
    Ok(())
}

/// Drain the channel until the response to `request_id` arrives —
/// notifications received meanwhile are dropped (the serve loop is
/// the *sender* of notifications; an inbound notification mid-turn
/// is unsolicited and carries no durable authority). A JSON-RPC
/// error response maps to [`PermissionOutcome::Deny`] — the
/// transport never converts a peer error into an approval.
fn await_permission_response<T: SessionTransport>(
    transport: &mut T,
    request_id: &Json,
) -> Result<PermissionOutcome, String> {
    loop {
        match transport.recv().map_err(|e| format!("transport/{e}"))? {
            None => return Err("transport/closed_awaiting_permission".to_string()),
            Some(frame) => {
                if frame.get("id") == Some(request_id) {
                    if frame.get("error").is_some() {
                        return Ok(PermissionOutcome::Deny);
                    }
                    let result = frame.get("result").cloned().unwrap_or(Json::obj([]));
                    return Ok(PermissionOutcome::from_wire(&result));
                }
                if frame.get("method").is_some() {
                    // An inbound request mid-permission — nothing the
                    // serve loop owns; answer method-not-found so the
                    // peer is never hung.
                    let mid = frame.get("id").cloned().unwrap_or(Json::Null);
                    transport.send(Json::obj([
                        ("jsonrpc", Json::str("2.0")),
                        ("id", mid),
                        (
                            "error",
                            Json::obj([
                                ("code", Json::Int(METHOD_NOT_FOUND)),
                                ("message", Json::str("busy")),
                            ]),
                        ),
                    ]))?;
                }
            }
        }
    }
}

fn session_id_of(params: &Json) -> String {
    params
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_open(sessions: &BTreeMap<String, OpenSession>, session_id: &str) -> bool {
    sessions.get(session_id).map(|s| !s.closed).unwrap_or(false)
}

fn send_result<T: SessionTransport>(
    transport: &mut T,
    id: Json,
    result: Json,
) -> Result<(), ServeError> {
    transport
        .send(Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", id),
            ("result", result),
        ]))
        .map_err(ServeError::Transport)
}

fn send_error<T: SessionTransport>(
    transport: &mut T,
    id: Json,
    code: i64,
    message: &str,
) -> Result<(), ServeError> {
    transport
        .send(Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", id),
            (
                "error",
                Json::obj([
                    ("code", Json::Int(code)),
                    ("message", Json::str(message.to_string())),
                ]),
            ),
        ]))
        .map_err(ServeError::Transport)
}
