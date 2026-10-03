//! `AcpClient` — the binding-(c) client half (§5d.4: the ACP
//! session artefact is a client — the harness *attaches* to the
//! session on a separate channel, never an in-process call).
//! `attach_session(descriptor)` runs the `initialize` probe and
//! mints the `acp` [`ProtocolBinding`] record; the session verbs
//! collect `session/update` notifications until the turn's response
//! arrives and route inbound `session/request_permission` requests
//! through the caller's [`PiGate`] (the kernel's Π — the client
//! never approves on its own).

use std::collections::BTreeMap;

use hh_compiler::acp::AcpDialect;
use hh_mcp::protocol::{NegotiateError, ProtocolBinding};
use hh_wire::json::Json;

use crate::permission::{PermissionOutcome, PermissionRequest, PiGate};
use crate::protocol::{acp_binding, negotiate_era, AcpEra, ACP_VERSION};
use crate::transport::SessionTransport;

/// `ClientError` — the client half's typed failures.
#[derive(Debug)]
pub enum ClientError {
    /// The channel faulted.
    Transport(String),
    /// A JSON-RPC protocol error from the peer (distinct from
    /// tool-reported failures — the dispatcher contract's split).
    Protocol {
        /// The error `code`.
        code: i64,
        /// The error `message`.
        message: String,
    },
    /// Version negotiation failed (typed `control.protocol.mismatch`
    /// payload).
    Negotiate(NegotiateError),
}

impl ClientError {
    /// The ledger-safe spelling.
    pub fn refusal(&self) -> String {
        match self {
            ClientError::Transport(d) => format!("transport/{d}"),
            ClientError::Protocol { code, message } => {
                format!("protocol/jsonrpc_{code}:{message}")
            }
            ClientError::Negotiate(e) => format!("protocol/{}", e.refusal()),
        }
    }
}

/// `CollectedUpdate` — one `session/update` the client observed.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectedUpdate {
    /// The session the update binds to.
    pub session_id: String,
    /// The `sessionUpdate` kind.
    pub kind: String,
    /// The update params verbatim.
    pub params: Json,
}

/// `PromptOutcome` — the completed `session/prompt`: every update
/// the channel carried in order, plus the turn's `stopReason`.
#[derive(Debug, Clone)]
pub struct PromptOutcome {
    /// The updates observed (order = arrival order).
    pub updates: Vec<CollectedUpdate>,
    /// The turn's `stopReason`.
    pub stop_reason: String,
}

/// `AttachDescriptor` — the hosted-session descriptor `attach_session`
/// yields (AC-R-2.5.4-7). Under the v1 compatibility profile the
/// host's observability ceiling is `events`/`end_state` — `ledger`
/// is *never* claimable there (the profile drops the update surface
/// it can't carry); steer/fork/compaction/subagents report
/// `"unknown"` (the profile cannot tell — an honest answer, never a
/// silent `false`); component metrics are `"n/a"`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachDescriptor {
    /// `observability_level` — `"events" | "end_state" | "ledger"`.
    /// `"ledger"` requires the v2 profile's `_hh/ledger/read`
    /// advertisement; a v1 host can never yield it.
    pub observability_level: String,
    /// `steer` — `"supported" | "unknown" | "unsupported"`.
    pub steer: String,
    /// `fork` — `session/fork` admitted.
    pub fork: String,
    /// `compaction` — the compaction surface.
    pub compaction: String,
    /// `subagents` — delegated-task surface.
    pub subagents: String,
    /// `component_metrics` — per-component telemetry level (`"n/a"`
    /// under v1 — the profile carries none).
    pub component_metrics: String,
}

impl AttachDescriptor {
    /// The record form (`to_json` — the attach result's claimable
    /// members; the binding record rides alongside).
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "observability_level",
                Json::str(self.observability_level.clone()),
            ),
            ("steer", Json::str(self.steer.clone())),
            ("fork", Json::str(self.fork.clone())),
            ("compaction", Json::str(self.compaction.clone())),
            ("subagents", Json::str(self.subagents.clone())),
            (
                "component_metrics",
                Json::str(self.component_metrics.clone()),
            ),
        ])
    }
}

/// `AcpClient` — stateful client over a [`SessionTransport`].
pub struct AcpClient<T: SessionTransport> {
    /// The channel.
    transport: T,
    /// The negotiated era (set by `attach_session`).
    era: Option<AcpEra>,
    /// The minted binding record (set by `attach_session`).
    binding: Option<ProtocolBinding>,
    /// The agent's semantic id from `initialize` (`agentCapabilities.
    /// _meta.hh.agent_semantic_id`).
    agent_semantic_id: Option<String>,
    /// The attach descriptor (set by `attach_session`).
    descriptor: Option<AttachDescriptor>,
    /// The client's request-id counter (`acp-c-N`).
    next_id: u64,
}

impl<T: SessionTransport> AcpClient<T> {
    /// `new(transport)` — the unattached client.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            era: None,
            binding: None,
            agent_semantic_id: None,
            descriptor: None,
            next_id: 1,
        }
    }

    /// The negotiated dialect (post-`attach_session`; `V2` before).
    pub fn dialect(&self) -> AcpDialect {
        self.era.unwrap_or(AcpEra::V2).dialect()
    }

    /// The minted binding record.
    pub fn binding(&self) -> Option<&ProtocolBinding> {
        self.binding.as_ref()
    }

    /// The agent's semantic id (post-`attach_session`).
    pub fn agent_semantic_id(&self) -> Option<&str> {
        self.agent_semantic_id.as_deref()
    }

    /// The attach descriptor (post-`attach_session`) — the
    /// observability/steer/fork/compaction/subagents/metrics claims
    /// the negotiated era admits.
    pub fn descriptor(&self) -> Option<&AttachDescriptor> {
        self.descriptor.as_ref()
    }

    /// `attach_session(descriptor, era_hint)` — the binding-(c)
    /// handshake: send `initialize` offering `era_hint`'s version
    /// (`None` → the pinned `ACP_VERSION`), verify the peer's
    /// reported `protocolVersion` against the support set
    /// (`negotiate_era` — mismatch is the typed refusal, never a
    /// coercion), and mint the `acp` `ProtocolBinding` record.
    /// `descriptor.request_target` becomes the record's
    /// `request_target` — the idempotency coordinate the session's
    /// requests carry (R-2.5.4's `target` forwarding).
    pub fn attach_session(
        &mut self,
        descriptor: &Json,
        era_hint: Option<AcpEra>,
    ) -> Result<ProtocolBinding, ClientError> {
        let offered = match era_hint.unwrap_or(AcpEra::V2) {
            AcpEra::V2 => ACP_VERSION.to_string(),
            AcpEra::V1 => crate::protocol::ACP_LEGACY_LABEL.to_string(),
        };
        let client_info = descriptor.get("client_info").cloned().unwrap_or(Json::obj([
            ("name", Json::str("harnessharness")),
            ("title", Json::str("HarnessHarness kernel")),
        ]));
        let result = self.request_raw(
            "initialize",
            Json::obj([
                ("protocolVersion", Json::str(offered)),
                ("clientInfo", client_info),
                (
                    "clientCapabilities",
                    descriptor
                        .get("clientCapabilities")
                        .cloned()
                        .unwrap_or(Json::obj([])),
                ),
            ]),
            None,
        )?;
        let reported = result
            .get("protocolVersion")
            .and_then(Json::as_str)
            .unwrap_or(ACP_VERSION);
        let era = negotiate_era(reported).map_err(ClientError::Negotiate)?;
        self.era = Some(era);
        self.agent_semantic_id = result
            .get("agentCapabilities")
            .and_then(|c| c.get("_meta"))
            .and_then(|m| m.get("hh"))
            .and_then(|h| h.get("agent_semantic_id"))
            .and_then(Json::as_str)
            .map(String::from);
        let binding = acp_binding(
            era,
            "attach_session",
            result.get("agentInfo").cloned().unwrap_or(Json::obj([])),
            result
                .get("agentCapabilities")
                .cloned()
                .unwrap_or(Json::obj([])),
            BTreeMap::new(),
            descriptor
                .get("request_target")
                .and_then(Json::as_str)
                .map(String::from),
        );
        self.binding = Some(binding.clone());
        self.descriptor = Some(match era {
            AcpEra::V1 => AttachDescriptor {
                // The v1 compatibility profile caps observability at
                // the update stream it can carry — `events` when the
                // host streams `session/update`, else `end_state`.
                // `ledger` is unreachable under v1.
                observability_level: if result
                    .get("agentCapabilities")
                    .and_then(|c| c.get("streamableUpdates"))
                    .map(|c| matches!(c, Json::Bool(true)))
                    .unwrap_or(true)
                {
                    "events"
                } else {
                    "end_state"
                }
                .to_string(),
                steer: "unknown".to_string(),
                fork: "unknown".to_string(),
                compaction: "unknown".to_string(),
                subagents: "unknown".to_string(),
                component_metrics: "n/a".to_string(),
            },
            AcpEra::V2 => {
                let hh = result
                    .get("agentCapabilities")
                    .and_then(|c| c.get("_meta"))
                    .and_then(|m| m.get("hh"))
                    .cloned()
                    .unwrap_or(Json::obj([]));
                let admits = |m: &str| -> bool {
                    hh.get("hh_methods")
                        .and_then(|a| match a {
                            Json::Arr(v) => Some(v.iter().any(|x| x.as_str() == Some(m))),
                            _ => None,
                        })
                        .unwrap_or(false)
                };
                let yes = |b: bool| if b { "supported" } else { "unsupported" };
                AttachDescriptor {
                    observability_level: if admits("_hh/ledger/read") {
                        "ledger"
                    } else {
                        "events"
                    }
                    .to_string(),
                    steer: yes(admits("_hh/participant/describe")).to_string(),
                    fork: "supported".to_string(),
                    compaction: "unknown".to_string(),
                    subagents: "unknown".to_string(),
                    component_metrics: "run".to_string(),
                }
            }
        });
        Ok(binding)
    }

    /// `session/new{meta, mcpServers, config}` → `{sessionId, …}`
    /// verbatim (`mcp_servers_pending_review` rides through — the
    /// record is the gate's answer, never an admission).
    pub fn new_session(&mut self, params: &Json) -> Result<Json, ClientError> {
        self.request_raw("session/new", params.clone(), None)
    }

    /// `session/prompt{sessionId, content}` — collect updates +
    /// answer `session/request_permission` through `pi` until the
    /// turn's response lands.
    pub fn prompt<P: PiGate>(
        &mut self,
        session_id: &str,
        content: &Json,
        pi: &mut P,
    ) -> Result<PromptOutcome, ClientError> {
        let mut updates = Vec::new();
        let result = self.request_raw(
            "session/prompt",
            Json::obj([
                ("sessionId", Json::str(session_id.to_string())),
                ("content", content.clone()),
            ]),
            Some((&mut updates, pi as &mut dyn PiGate)),
        )?;
        Ok(PromptOutcome {
            updates,
            stop_reason: result
                .get("stopReason")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    /// `session/cancel{sessionId}` → the cancel response (updates the
    /// cancellation rendered land in `updates` when provided).
    pub fn cancel(&mut self, session_id: &str) -> Result<Json, ClientError> {
        self.request_raw(
            "session/cancel",
            Json::obj([("sessionId", Json::str(session_id.to_string()))]),
            None,
        )
    }

    /// `session/resume{sessionId, replayFrom?}` → `(updates, result)`
    /// — the replayed stream rides the channel before the response.
    pub fn resume(
        &mut self,
        session_id: &str,
        replay_from: Option<u64>,
    ) -> Result<(Vec<CollectedUpdate>, Json), ClientError> {
        let mut params = BTreeMap::new();
        params.insert("sessionId".to_string(), Json::str(session_id.to_string()));
        if let Some(f) = replay_from {
            params.insert("replayFrom".to_string(), Json::Int(f as i64));
        }
        let mut updates = Vec::new();
        let result =
            self.request_raw_collected("session/resume", Json::Obj(params), &mut updates)?;
        Ok((updates, result))
    }

    /// `session/close{sessionId}`.
    pub fn close(&mut self, session_id: &str) -> Result<Json, ClientError> {
        self.request_raw(
            "session/close",
            Json::obj([("sessionId", Json::str(session_id.to_string()))]),
            None,
        )
    }

    /// `_hh/ledger/read{from_seq}` → the durable tail verbatim.
    pub fn ledger_read(&mut self, from_seq: u64) -> Result<Json, ClientError> {
        self.request_raw(
            "_hh/ledger/read",
            Json::obj([("from_seq", Json::Int(from_seq as i64))]),
            None,
        )
    }

    /// `_hh/account` → the projection.
    pub fn account(&mut self) -> Result<Json, ClientError> {
        self.request_raw("_hh/account", Json::obj([]), None)
    }

    /// `_hh/participant/describe{participant}`.
    pub fn participant_describe(&mut self, participant: &Json) -> Result<Json, ClientError> {
        self.request_raw("_hh/participant/describe", participant.clone(), None)
    }

    /// `call_raw(method, params)` — an arbitrary request on the
    /// session channel (extension methods the peer may or may not
    /// serve — `-32601` answers honestly).
    pub fn call_raw(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        self.request_raw(method, params, None)
    }

    /// The core request — send the frame, then drain until the
    /// matching response arrives: `session/update` notifications
    /// collect (into `sink` when the caller drives a turn), inbound
    /// `session/request_permission` requests route through `pi`
    /// (absent Π → `denied` — never an implicit approval).
    fn request_raw(
        &mut self,
        method: &str,
        params: Json,
        turn: Option<(&mut Vec<CollectedUpdate>, &mut dyn PiGate)>,
    ) -> Result<Json, ClientError> {
        let id = Json::str(format!("acp-c-{}", self.next_id));
        self.next_id += 1;
        self.transport
            .send(Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", id.clone()),
                ("method", Json::str(method.to_string())),
                ("params", params),
            ]))
            .map_err(ClientError::Transport)?;
        let (sink, pi) = match turn {
            Some((s, p)) => (Some(s), Some(p)),
            None => (None, None),
        };
        self.await_response(&id, sink, pi)
    }

    /// `request_raw` with an update sink but no Π (inbound
    /// permission requests get the deny-default response).
    fn request_raw_collected(
        &mut self,
        method: &str,
        params: Json,
        sink: &mut Vec<CollectedUpdate>,
    ) -> Result<Json, ClientError> {
        let id = Json::str(format!("acp-c-{}", self.next_id));
        self.next_id += 1;
        self.transport
            .send(Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", id.clone()),
                ("method", Json::str(method.to_string())),
                ("params", params),
            ]))
            .map_err(ClientError::Transport)?;
        self.await_response(&id, Some(sink), None)
    }

    /// Drain until the matching response. Inbound requests:
    /// `session/request_permission` → `pi.decide` (deny-default
    /// without Π); anything else → `-32601`.
    fn await_response(
        &mut self,
        id: &Json,
        mut sink: Option<&mut Vec<CollectedUpdate>>,
        mut pi: Option<&mut dyn PiGate>,
    ) -> Result<Json, ClientError> {
        loop {
            let frame = match self.transport.recv().map_err(ClientError::Transport)? {
                Some(f) => f,
                None => {
                    return Err(ClientError::Transport(
                        "closed_awaiting_response".to_string(),
                    ))
                }
            };
            if frame.get("id") == Some(id) && frame.get("method").is_none() {
                if let Some(err) = frame.get("error") {
                    return Err(ClientError::Protocol {
                        code: err.get("code").and_then(Json::as_int).unwrap_or(0),
                        message: err
                            .get("message")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                return Ok(frame.get("result").cloned().unwrap_or(Json::obj([])));
            }
            if frame.get("method").and_then(Json::as_str) == Some("session/update") {
                if let Some(s) = sink.as_deref_mut() {
                    let params = frame.get("params").cloned().unwrap_or(Json::obj([]));
                    let update = params.get("update").cloned().unwrap_or(Json::obj([]));
                    s.push(CollectedUpdate {
                        session_id: params
                            .get("sessionId")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind: update
                            .get("sessionUpdate")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        params: update,
                    });
                }
                continue;
            }
            if let Some(m) = frame.get("method").and_then(Json::as_str) {
                let req_id = frame.get("id").cloned().unwrap_or(Json::Null);
                if m == "session/request_permission" {
                    let params = frame.get("params").cloned().unwrap_or(Json::obj([]));
                    let request = PermissionRequest {
                        request_id: req_id.clone(),
                        session_id: params
                            .get("sessionId")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        params,
                    };
                    let outcome = match pi.as_deref_mut() {
                        Some(g) => g.decide(&request),
                        None => PermissionOutcome::Deny,
                    };
                    self.transport
                        .send(Json::obj([
                            ("jsonrpc", Json::str("2.0")),
                            ("id", req_id),
                            ("result", outcome.to_wire()),
                        ]))
                        .map_err(ClientError::Transport)?;
                } else {
                    self.transport
                        .send(Json::obj([
                            ("jsonrpc", Json::str("2.0")),
                            ("id", req_id),
                            (
                                "error",
                                Json::obj([
                                    ("code", Json::Int(-32601)),
                                    ("message", Json::str(m.to_string())),
                                ]),
                            ),
                        ]))
                        .map_err(ClientError::Transport)?;
                }
            }
        }
    }
}
