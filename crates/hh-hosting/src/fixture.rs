//! The fixture participant — an in-process session-protocol peer
//! (S4.5a; hermetic, offline-only). It implements [`SessionTransport`] with
//! scripted, deterministic behavior and honest capability *performance*: the
//! flags model what the participant actually does (never what it declares —
//! the probe catalogue measures the difference).
//!
//! Scripted behaviors the flags control:
//!
//! * `streams` — emit `agent_message_chunk` notifications during a turn.
//! * `pending_turns` — turns stay `running` until a protocol event finishes
//!   them (the cancel/steer window P-03/P-13/P-14 exercise).
//! * `ignore_cancel` — swallow `session/cancel` (P-03 DRIFT evidence).
//! * `auto_approve` — perform permission-gated tool calls without ever
//!   emitting `session/request_permission` (the auto-approval hazard —
//!   AC-R-2.10.6-4).
//! * `request_permissions` — emit the `session/request_permission` upcall for
//!   gated tool calls and hold the turn until the Lab answers.
//! * `usage_reported` — emit a `usage_update` notification at turn end.
//! * `steer_effective` — a `session/steer` on a running turn changes the
//!   output and finishes it.
//! * `resume_cold`/`resume_warm` — accept `session/load`/`session/resume`.
//! * `accepts_coordinates`/`accepts_credentials` — the config ids / credential
//!   channels the participant honors (P-09/P-10/P-15).
//! * `delivers_context` — acknowledge `context_items` at `session/new`.
//! * `model_observations` — scripted proxy-channel observations (the
//!   intercepted model calls a `model_io_intercept` adapter would see).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::Json;

use crate::adapter_a::{PendingUpcall, SessionTransport};

/// A scripted tool call the fixture performs during a prompt —
/// `{kind_hint, permission_gated}`. A `permission_gated` call goes through
/// `session/request_permission` when `request_permissions` is set (and
/// silently proceeds when `auto_approve` is).
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptedTool {
    /// The `kind_hint` the `tool_call` notification reports.
    pub kind_hint: String,
    /// `true` = the effect requires a permission decision.
    pub permission_gated: bool,
    /// `true` = the call rides the Lab's sealed tool surface (P-04);
    /// `false` = the participant's own tool (P-05, observed).
    pub lab_supplied: bool,
}

/// Behavior + state of the fixture.
pub struct FixtureParticipant {
    /// The `abi_versions` `session/describe` advertises.
    pub abi_versions: Vec<String>,
    /// The capability declaration `session/describe` reports.
    pub declaration: BTreeMap<String, Json>,

    /// Emit `agent_message_chunk` notifications during a prompt.
    pub streams: bool,
    /// Turns stay running until a protocol event finishes them.
    pub pending_turns: bool,
    /// Swallow `session/cancel` (the P-03 drift behavior).
    pub ignore_cancel: bool,
    /// Perform gated effects without asking (auto-approval hazard).
    pub auto_approve: bool,
    /// Emit `session/request_permission` upcalls for gated calls.
    pub request_permissions: bool,
    /// Emit a `usage_update` at turn end.
    pub usage_reported: bool,
    /// `session/steer` changes a running turn's output.
    pub steer_effective: bool,
    /// Accept `session/load` (cold resume) / `session/resume` (warm).
    pub resume_cold: bool,
    /// See `resume_cold`.
    pub resume_warm: bool,
    /// Config ids `session/set_config_option` accepts.
    pub accepts_coordinates: BTreeSet<String>,
    /// Credential channels `session/credential` accepts.
    pub accepts_credentials: BTreeSet<String>,
    /// Acknowledge delivered context items.
    pub delivers_context: bool,
    /// Scripted intercept observations drained by the proxy channel.
    pub model_observations: Vec<Json>,
    /// The tool calls a prompt performs.
    pub scripted_tools: Vec<ScriptedTool>,

    sessions: BTreeMap<String, FixtureSession>,
    snapshots: BTreeMap<String, Json>,
    notifications: Vec<(String, Json)>,
    upcalls: Vec<PendingUpcall>,
    awaiting: Vec<PendingUpcall>,
    seq: u64,
    session_seq: u64,
    upcall_seq: u64,
}

struct FixtureSession {
    open: bool,
    /// The Lab-side `session_ref` carried on `session/new` — the key the
    /// Lab uses to address saved state on cold resume.
    lab_ref: String,
    turns: BTreeMap<String, FixtureTurn>,
    coordinates: BTreeMap<String, Json>,
}

struct FixtureTurn {
    finished: Option<String>,
    pending_upcall: Option<String>,
}

impl Default for FixtureParticipant {
    /// The well-behaved session-ABI participant — supports every probed
    /// dimension honestly.
    fn default() -> FixtureParticipant {
        FixtureParticipant {
            abi_versions: vec!["hh-hosting/1".to_string()],
            declaration: BTreeMap::new(),
            streams: true,
            pending_turns: false,
            ignore_cancel: false,
            auto_approve: false,
            request_permissions: true,
            usage_reported: true,
            steer_effective: true,
            resume_cold: true,
            resume_warm: true,
            accepts_coordinates: ["model", "mode"].iter().map(|s| s.to_string()).collect(),
            accepts_credentials: ["env", "mcp"].iter().map(|s| s.to_string()).collect(),
            delivers_context: true,
            model_observations: Vec::new(),
            // The default turn performs one *own* ungated call (P-05) and one
            // *Lab-supplied* gated call (P-04/P-06/P-07 — permission first).
            scripted_tools: vec![
                ScriptedTool {
                    kind_hint: "read".into(),
                    permission_gated: false,
                    lab_supplied: false,
                },
                ScriptedTool {
                    kind_hint: "execute".into(),
                    permission_gated: true,
                    lab_supplied: true,
                },
            ],
            sessions: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            notifications: Vec::new(),
            upcalls: Vec::new(),
            awaiting: Vec::new(),
            seq: 0,
            session_seq: 0,
            upcall_seq: 0,
        }
    }
}

impl FixtureParticipant {
    /// Queue a notification the next `drain_notifications` reports.
    fn notify(&mut self, method: &str, params: Json) {
        self.seq += 1;
        self.notifications.push((method.to_string(), params));
    }

    fn session_mut(&mut self, id: &str) -> Result<&mut FixtureSession, String> {
        self.sessions
            .get_mut(id)
            .ok_or_else(|| format!("unknown session {id}"))
    }

    /// The turn sequence for a prompt: deltas, scripted tools (with the
    /// permission upcall when gated), usage, completion. `pending_turns`
    /// holds the finish notification for a later protocol event.
    fn run_prompt(&mut self, session_id: &str, turn_id: &str) -> Result<Json, String> {
        let scripted = self.scripted_tools.clone();
        let mut needs_permission: Option<String> = None;
        if self.streams {
            self.notify(
                "session/update",
                Json::obj([
                    ("sessionUpdate", Json::str("agent_message_chunk")),
                    ("content", Json::obj([("text", Json::str("chunk-1"))])),
                ]),
            );
            self.notify(
                "session/update",
                Json::obj([
                    ("sessionUpdate", Json::str("agent_message_chunk")),
                    ("content", Json::obj([("text", Json::str("chunk-2"))])),
                ]),
            );
        }
        for (i, t) in scripted.iter().enumerate() {
            let call_id = format!("{turn_id}-tool-{i}");
            self.notify(
                "session/update",
                Json::obj([
                    ("sessionUpdate", Json::str("tool_call")),
                    ("toolCallId", Json::str(&call_id)),
                    ("kind_hint", Json::str(&t.kind_hint)),
                    ("lab_supplied", Json::Bool(t.lab_supplied)),
                ]),
            );
            if t.permission_gated && !self.auto_approve && self.request_permissions {
                self.upcall_seq += 1;
                let id = format!("up-{}", self.upcall_seq);
                let u = PendingUpcall {
                    id: id.clone(),
                    method: "session/request_permission".to_string(),
                    params: Json::obj([
                        ("tool_call_id", Json::str(&call_id)),
                        ("action", Json::str(&t.kind_hint)),
                        ("capability", Json::str("tool")),
                    ]),
                };
                self.awaiting.push(u.clone());
                self.upcalls.push(u);
                needs_permission = Some(call_id);
                break; // the turn waits on the Lab's answer
            }
            // Ungated (or auto-approved) — the effect completes immediately;
            // an auto-approved call completes with NO permission upcall.
            self.notify(
                "session/update",
                Json::obj([
                    ("sessionUpdate", Json::str("tool_call_update")),
                    ("toolCallId", Json::str(&call_id)),
                    ("status", Json::str("completed")),
                ]),
            );
        }
        let pending = needs_permission.is_some() || self.pending_turns;
        {
            let s = self.session_mut(session_id)?;
            s.turns.insert(
                turn_id.to_string(),
                FixtureTurn {
                    finished: None,
                    pending_upcall: needs_permission,
                },
            );
        }
        if pending {
            return Ok(Json::obj([
                ("status", Json::str("running")),
                ("turn_id", Json::str(turn_id)),
            ]));
        }
        self.finish_turn(session_id, turn_id, "end_turn");
        Ok(Json::obj([
            ("status", Json::str("finished")),
            ("turn_id", Json::str(turn_id)),
            ("stop_reason", Json::str("end_turn")),
        ]))
    }

    /// The completion sequence — usage, message, turn_finished notification.
    fn finish_turn(&mut self, session_id: &str, turn_id: &str, reason: &str) {
        if let Ok(s) = self.session_mut(session_id) {
            if let Some(t) = s.turns.get_mut(turn_id) {
                if t.finished.is_some() {
                    return;
                }
                t.finished = Some(reason.to_string());
            }
        }
        if self.usage_reported {
            self.notify(
                "session/update",
                Json::obj([
                    ("sessionUpdate", Json::str("usage_update")),
                    (
                        "usage",
                        Json::obj([
                            ("used", Json::Int(1200)),
                            ("size", Json::Int(300)),
                            ("groups", Json::Arr(vec![Json::str("cache_read")])),
                        ]),
                    ),
                ]),
            );
        }
        self.notify(
            "session/update",
            Json::obj([
                ("sessionUpdate", Json::str("agent_message_chunk")),
                ("content", Json::obj([("text", Json::str("done"))])),
            ]),
        );
        self.notify(
            "session/turn_finished",
            Json::obj([
                ("turn_id", Json::str(turn_id)),
                ("stop_reason", Json::str(reason)),
            ]),
        );
    }
}

impl SessionTransport for FixtureParticipant {
    fn request(&mut self, method: &str, params: &Json) -> Result<Json, String> {
        match method {
            "initialize" => Ok(Json::obj([
                ("protocolVersion", Json::Int(1)),
                ("agent", Json::str("fixture-participant")),
            ])),
            "session/describe" => Ok(Json::obj([
                (
                    "abi_versions",
                    Json::Arr(self.abi_versions.iter().map(Json::str).collect()),
                ),
                (
                    "capabilities",
                    Json::Obj(
                        self.declaration
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    ),
                ),
            ])),
            "session/new" => {
                self.session_seq += 1;
                let id = format!("proto-{}", self.session_seq);
                let context_items = params
                    .get("context_items")
                    .and_then(|c| match c {
                        Json::Arr(items) => Some(items.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let lab_ref = params
                    .get("session_ref")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                self.sessions.insert(
                    id.clone(),
                    FixtureSession {
                        open: true,
                        lab_ref,
                        turns: BTreeMap::new(),
                        coordinates: BTreeMap::new(),
                    },
                );
                if self.delivers_context && !context_items.is_empty() {
                    self.notify(
                        "session/update",
                        Json::obj([
                            ("sessionUpdate", Json::str("context_delivered")),
                            ("count", Json::Int(context_items.len() as i64)),
                        ]),
                    );
                }
                Ok(Json::obj([("session_id", Json::str(id))]))
            }
            "session/prompt" => {
                let sid = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .ok_or("missing session_id")?
                    .to_string();
                let tid = params
                    .get("turn_id")
                    .and_then(Json::as_str)
                    .ok_or("missing turn_id")?
                    .to_string();
                if !self.session_mut(&sid)?.open {
                    return Err("session closed".into());
                }
                self.run_prompt(&sid, &tid)
            }
            "session/cancel" => {
                let sid = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .ok_or("missing session_id")?
                    .to_string();
                if self.ignore_cancel {
                    return Ok(Json::obj([])); // swallowed — P-03 drift evidence
                }
                let running: Vec<String> = {
                    let s = self.session_mut(&sid)?;
                    s.turns
                        .iter()
                        .filter(|(_, t)| t.finished.is_none())
                        .map(|(id, _)| id.clone())
                        .collect()
                };
                for tid in running {
                    self.finish_turn(&sid, &tid, "cancelled");
                }
                Ok(Json::obj([]))
            }
            "session/steer" => {
                let sid = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .ok_or("missing session_id")?
                    .to_string();
                let tid = params
                    .get("turn_id")
                    .and_then(Json::as_str)
                    .ok_or("missing turn_id")?
                    .to_string();
                if !self.steer_effective {
                    return Ok(Json::obj([])); // inert — P-14 unsupported evidence
                }
                let running = {
                    let s = self.session_mut(&sid)?;
                    matches!(s.turns.get(&tid), Some(t) if t.finished.is_none())
                };
                if !running {
                    return Err("turn not running".into());
                }
                let text = params
                    .get("text")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                self.notify(
                    "session/update",
                    Json::obj([
                        ("sessionUpdate", Json::str("agent_message_chunk")),
                        (
                            "content",
                            Json::obj([("text", Json::str(format!("steered:{text}")))]),
                        ),
                    ]),
                );
                self.finish_turn(&sid, &tid, "end_turn");
                Ok(Json::obj([]))
            }
            "session/set_config_option" => {
                let sid = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .ok_or("missing session_id")?
                    .to_string();
                let cfg = params
                    .get("config_id")
                    .and_then(Json::as_str)
                    .ok_or("missing config_id")?
                    .to_string();
                if !self.accepts_coordinates.contains(&cfg) {
                    return Err(format!("unsupported config option {cfg}"));
                }
                let value = params.get("value").cloned().unwrap_or(Json::Null);
                self.session_mut(&sid)?
                    .coordinates
                    .insert(cfg.clone(), value.clone());
                self.notify(
                    "session/update",
                    Json::obj([
                        ("sessionUpdate", Json::str("current_mode_update")),
                        ("modeId", value),
                    ]),
                );
                Ok(Json::obj([]))
            }
            "session/credential" => {
                let channel = params
                    .get("channel")
                    .and_then(Json::as_str)
                    .ok_or("missing channel")?
                    .to_string();
                if !self.accepts_credentials.contains(&channel) {
                    return Err(format!("undeclared credential channel {channel}"));
                }
                self.notify(
                    "session/update",
                    Json::obj([
                        ("sessionUpdate", Json::str("credential_received")),
                        ("channel", Json::str(channel)),
                    ]),
                );
                Ok(Json::obj([]))
            }
            "session/load" => {
                if !self.resume_cold {
                    return Err("cold resume unsupported".into());
                }
                let sref = params
                    .get("session_ref")
                    .and_then(Json::as_str)
                    .ok_or("missing session_ref")?;
                if !self.snapshots.contains_key(sref) {
                    return Err(format!("no saved state for {sref}"));
                }
                self.session_seq += 1;
                let id = format!("proto-{}", self.session_seq);
                self.sessions.insert(
                    id.clone(),
                    FixtureSession {
                        open: true,
                        lab_ref: sref.to_string(),
                        turns: BTreeMap::new(),
                        coordinates: BTreeMap::new(),
                    },
                );
                Ok(Json::obj([("session_id", Json::str(id))]))
            }
            "session/resume" => {
                if !self.resume_warm {
                    return Err("warm resume unsupported".into());
                }
                let sref = params
                    .get("session_ref")
                    .and_then(Json::as_str)
                    .ok_or("missing session_ref")?;
                // Warm resume continues a session the transport still holds —
                // find it by protocol id or by the saved snapshot's id.
                let sid = sref.to_string();
                if self.sessions.contains_key(&sid) {
                    self.session_mut(&sid)?.open = true;
                    Ok(Json::obj([("session_id", Json::str(sid))]))
                } else if self.snapshots.contains_key(&sid) {
                    self.session_seq += 1;
                    let id = format!("proto-{}", self.session_seq);
                    self.sessions.insert(
                        id.clone(),
                        FixtureSession {
                            open: true,
                            lab_ref: sid.clone(),
                            turns: BTreeMap::new(),
                            coordinates: BTreeMap::new(),
                        },
                    );
                    Ok(Json::obj([("session_id", Json::str(id))]))
                } else {
                    Err(format!("no session {sid} to resume"))
                }
            }
            "session/close" => {
                let sid = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .ok_or("missing session_id")?
                    .to_string();
                let s = self.session_mut(&sid)?;
                if !s.open {
                    return Err("already closed".into());
                }
                s.open = false;
                let turns = s.turns.len() as i64;
                let lab_ref = s.lab_ref.clone();
                self.snapshots
                    .insert(sid.clone(), Json::obj([("session_id", Json::str(&sid))]));
                if !lab_ref.is_empty() {
                    self.snapshots
                        .insert(lab_ref, Json::obj([("session_id", Json::str(&sid))]));
                }
                Ok(Json::obj([
                    ("reason", Json::str("closed")),
                    ("turns", Json::Int(turns)),
                ]))
            }
            "session/export" => Ok(Json::obj([(
                "trajectory_ref",
                Json::str(format!(
                    "traj:{}",
                    params
                        .get("session_id")
                        .and_then(Json::as_str)
                        .unwrap_or("?")
                )),
            )])),
            "session/elicit" => Ok(Json::obj([("answer", Json::str("42"))])),
            "account/status" => Ok(Json::obj([
                ("balance_micro", Json::Int(1_000)),
                ("currency", Json::str("usd")),
                ("exact", Json::Bool(true)),
            ])),
            other => Err(format!("unsupported method {other}")),
        }
    }

    fn drain_notifications(&mut self) -> Vec<(String, Json)> {
        std::mem::take(&mut self.notifications)
    }

    fn drain_upcalls(&mut self) -> Vec<PendingUpcall> {
        std::mem::take(&mut self.upcalls)
    }

    fn respond_upcall(&mut self, id: &str, result: Result<Json, String>) {
        let Some(u) = self.awaiting.iter().position(|u| u.id == id) else {
            return;
        };
        let u = self.awaiting.remove(u);
        let call_id = u
            .params
            .get("tool_call_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        // Find the turn waiting on this upcall (the prompt recorded it).
        let mut resolved: Option<(String, String)> = None;
        for (sid, s) in self.sessions.iter_mut() {
            for (tid, t) in s.turns.iter() {
                if t.pending_upcall.as_deref() == Some(call_id.as_str()) {
                    resolved = Some((sid.clone(), tid.clone()));
                }
            }
        }
        let decision = result
            .ok()
            .and_then(|r| r.get("decision").and_then(Json::as_str).map(String::from))
            .unwrap_or_else(|| "deny".to_string());
        let status = if decision == "allow" {
            "completed"
        } else {
            "refused"
        };
        self.notify(
            "session/update",
            Json::obj([
                ("sessionUpdate", Json::str("tool_call_update")),
                ("toolCallId", Json::str(&call_id)),
                ("status", Json::str(status)),
            ]),
        );
        if let Some((sid, tid)) = resolved {
            // The participant observes the outcome and finishes the turn —
            // a denial is a handled failure, the turn ends `end_turn`.
            if let Ok(s) = self.session_mut(&sid) {
                if let Some(t) = s.turns.get_mut(&tid) {
                    t.pending_upcall = None;
                }
            }
            self.finish_turn(&sid, &tid, "end_turn");
        }
    }

    fn drain_model_observations(&mut self) -> Vec<Json> {
        std::mem::take(&mut self.model_observations)
    }
}
