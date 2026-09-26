//! Adapter A — the session-ABI adapter (§6.6 §5 / §7; R-2.10.6; S4.5a;
//! ADR-0164/0166).
//!
//! Adapter A speaks a session protocol to a participant process (the ACP
//! stable core shape — `initialize`, `session/new`, `session/prompt`,
//! `session/cancel`, `session/set_config_option`, `session/load`,
//! `session/resume`, `session/request_permission` upcall, `session/update`
//! notifications) and lifts the protocol observations into [`HostedEvent`]s.
//!
//! Honesty rules the adapter obeys (I-6):
//!
//! * The adapter **never asserts `mediated`** on a participant/adapter row —
//!   `mediated` lands only on kernel-channel facts (permission rows the Lab's
//!   own decider produced; intercepted model rows on the proxy channel).
//! * An adapter that auto-approves participant permission requests fails the
//!   admission check (`AdapterA::auto_approve` — the Lab supplies the decider;
//!   an adapter that decides for itself is refused at `open`).
//! * `session/new` on a definition the Lab does not own refuses (adapter zero
//!   attaches via `hh-embed/1 attach`; Adapter A serves `session_abi`
//!   participants at `in_environment`/`lab_host`/`remote_service` placements).
//! * Unknown `session/update` kinds and `_`-prefixed protocol members are
//!   preserved through `ext`/`native_record` leaves — never dropped (I-3).

use std::collections::BTreeMap;

use hh_ontology::participant::HostingMechanism;
use hh_provenance::AuthorityClass;
use hh_wire::Json;

use crate::abi::{HostingError, ResumeCursor};
use crate::events::{EventChannel, HostedOrigin, Mediation};
use crate::records::{AdapterRecord, ModelIoIntercept, ProcessPlacement};

/// A participant→Lab *request* that needs a Lab response (the session-ABI
/// upcall surface: `session/request_permission`, `session/elicit`,
/// `fs/read_text_file`-style tool-facing calls the boundary may mediate).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpcall {
    /// The protocol request id (correlates [`SessionTransport::respond_upcall`]).
    pub id: String,
    /// The method spelling.
    pub method: String,
    /// The params.
    pub params: Json,
}

/// The session-protocol wire the adapter drives — records-in/records-out
/// (every call is a method+params `Json` pair; the transport owns process
/// plumbing). An in-process fixture, a stdio peer, or a socket peer all
/// implement this surface; the adapter never sees a handle.
pub trait SessionTransport {
    /// A protocol request/response.
    fn request(&mut self, method: &str, params: &Json) -> Result<Json, String>;
    /// Drain pending participant→Lab notifications (`{method, params}`).
    fn drain_notifications(&mut self) -> Vec<(String, Json)>;
    /// Drain pending upcalls awaiting a Lab response (the pull-model for
    /// transports that cannot call back mid-request).
    fn drain_upcalls(&mut self) -> Vec<PendingUpcall>;
    /// Answer an upcall.
    fn respond_upcall(&mut self, id: &str, result: Result<Json, String>);
    /// Drain the intercepted-model observations the proxy channel produced
    /// (empty when `model_io_intercept = none` — the channel does not exist).
    fn drain_model_observations(&mut self) -> Vec<Json> {
        Vec::new()
    }
}

/// The adapter's lift of protocol observations into hosted events — the
/// service stamps `seq`/`session`/`at`; the adapter stamps `kind`, `payload`,
/// `provenance{origin, authority}`, `mediation`, `event_channel`, `raw_ref`
/// and never invents `mediated` on a participant claim.
#[derive(Debug, Clone, PartialEq)]
pub struct LiftedObservation {
    /// The hosted kind (a [`crate::events::KNOWN_KINDS`] member or a
    /// preserved unknown spelling).
    pub kind: String,
    /// The kind payload.
    pub payload: Json,
    /// The channel origin.
    pub origin: HostedOrigin,
    /// The authority (bounded by I-4 — `kernel` only on kernel-channel rows).
    pub authority: AuthorityClass,
    /// The mediation stamp.
    pub mediation: Mediation,
    /// The arrival channel.
    pub event_channel: EventChannel,
    /// The raw protocol pointer, when there is one.
    pub raw_ref: Option<String>,
    /// Preserved extras (I-3).
    pub ext: BTreeMap<String, Json>,
}

impl LiftedObservation {
    fn participant_observed(kind: &str, payload: Json) -> LiftedObservation {
        LiftedObservation {
            kind: kind.to_string(),
            payload,
            origin: HostedOrigin::Participant,
            authority: AuthorityClass::Delegate,
            mediation: Mediation::Observed,
            event_channel: EventChannel::Protocol,
            raw_ref: None,
            ext: BTreeMap::new(),
        }
    }

    fn adapter_fact(kind: &str, payload: Json) -> LiftedObservation {
        LiftedObservation {
            kind: kind.to_string(),
            payload,
            origin: HostedOrigin::Adapter,
            authority: AuthorityClass::Unverified,
            mediation: Mediation::Observed,
            event_channel: EventChannel::Protocol,
            raw_ref: None,
            ext: BTreeMap::new(),
        }
    }

    fn kernel_fact(kind: &str, payload: Json) -> LiftedObservation {
        LiftedObservation {
            kind: kind.to_string(),
            payload,
            origin: HostedOrigin::Environment,
            authority: AuthorityClass::Kernel,
            mediation: Mediation::Mediated,
            event_channel: EventChannel::Protocol,
            raw_ref: None,
            ext: BTreeMap::new(),
        }
    }

    fn intercepted(kind: &str, payload: Json) -> LiftedObservation {
        LiftedObservation {
            kind: kind.to_string(),
            payload,
            origin: HostedOrigin::Intercept,
            authority: AuthorityClass::Environment,
            mediation: Mediation::Mediated,
            event_channel: EventChannel::Proxy,
            raw_ref: None,
            ext: BTreeMap::new(),
        }
    }
}

/// The policy decider the Lab installs — maps a `session/request_permission`
/// params object to the decision (`{"decision": "allow"|"deny",
/// "approval_wait_ms"}` — the pending/ask path is a decision the Lab made
/// after escalation; the wait is reported, never hidden).
pub type PolicyDecider = Box<dyn FnMut(&Json) -> Json>;

/// A decider that denies nothing (tests use scripted deciders; production
/// plugs the Π boundary).
pub fn allow_all_decider() -> PolicyDecider {
    Box::new(|_| Json::obj([("decision", Json::str("allow"))]))
}

/// A decider that denies everything (the P-06 drive — the participant's
/// gated effect is refused at the boundary).
pub fn deny_all_decider() -> PolicyDecider {
    Box::new(|_| Json::obj([("decision", Json::str("deny"))]))
}

/// A decider that *asks* first — models the `ask` path: the decision lands
/// after a reported wait (`approval_wait_ms`), P-07's observable.
pub fn ask_then_allow_decider() -> PolicyDecider {
    Box::new(|_| {
        Json::obj([
            ("decision", Json::str("allow")),
            ("approval_wait_ms", Json::Int(120)),
            ("decider", Json::str("human")),
        ])
    })
}

/// Adapter A (§6.6 §5) — drives a [`SessionTransport`], lifts notifications,
/// answers upcalls through the Lab's decider.
pub struct AdapterA {
    transport: Box<dyn SessionTransport>,
    record: AdapterRecord,
    /// `true` marks the auto-approval hazard: the adapter answers permission
    /// requests itself instead of deferring to the Lab's decider —
    /// `Service::open` refuses to attach such an adapter (AC-R-2.10.6-4).
    pub auto_approve: bool,
    decider: Option<PolicyDecider>,
    lifted: Vec<LiftedObservation>,
    /// `model_io_intercept` the adapter runs (`base_url`/`client_patch` open
    /// the proxy channel; `none`/`unknown` never invent measurements).
    pub intercept: ModelIoIntercept,
    /// Active session handles the transport minted (session_ref → protocol id).
    sessions: BTreeMap<String, String>,
    /// A per-adapter monotone counter for `raw_ref` correlation.
    observation_seq: u64,
}

impl AdapterA {
    /// Build over a transport + the adapter's registry record (the claims
    /// the service checks — I-6).
    pub fn new(
        transport: Box<dyn SessionTransport>,
        record: AdapterRecord,
        decider: Option<PolicyDecider>,
    ) -> AdapterA {
        AdapterA {
            transport,
            intercept: record
                .ext
                .get("model_io_intercept")
                .and_then(Json::as_str)
                .and_then(ModelIoIntercept::parse)
                .unwrap_or(ModelIoIntercept::None),
            record,
            auto_approve: false,
            decider,
            lifted: Vec::new(),
            sessions: BTreeMap::new(),
            observation_seq: 0,
        }
    }

    /// The adapter's registry record.
    pub fn record(&self) -> &AdapterRecord {
        &self.record
    }

    fn next_raw(&mut self, tag: &str) -> String {
        self.observation_seq += 1;
        format!("adapterA:{}:{tag}", self.observation_seq)
    }

    /// `describe` — `initialize` + `session/describe`: the participant's
    /// advertised `abi_versions` and capability declaration (verbatim —
    /// reconcile happens service-side, never adapter-side).
    pub fn describe(&mut self) -> Result<Json, HostingError> {
        let _init = self
            .transport
            .request(
                "initialize",
                &Json::obj([("protocolVersion", Json::Int(1))]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        self.transport
            .request("session/describe", &Json::obj([]))
            .map_err(|e| HostingError::Transport { detail: e })
    }

    /// `open` — `session/new{definition_ref, params, placement,
    /// connection_info, context_items}`. The spec has already passed the
    /// boundary checks (no handles, no credentials/authority members — the
    /// service strips them; the adapter forwards the sealed record only).
    pub fn open(&mut self, session_ref: &str, spec_json: &Json) -> Result<Json, HostingError> {
        let mut params = spec_json.clone();
        if let Json::Obj(m) = &mut params {
            m.insert("session_ref".into(), Json::str(session_ref));
        }
        let res = self
            .transport
            .request("session/new", &params)
            .map_err(|e| HostingError::Transport { detail: e })?;
        let session_id = res
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| HostingError::SchemaViolation {
                path: "session/new.session_id".into(),
                detail: "the participant returned no session id".into(),
            })?
            .to_string();
        self.sessions
            .insert(session_ref.to_string(), session_id.clone());
        self.lifted.push(LiftedObservation::adapter_fact(
            "session.opened",
            Json::obj([
                ("session_ref", Json::str(session_ref)),
                ("protocol_session_id", Json::str(session_id)),
            ]),
        ));
        Ok(res)
    }

    /// `submit` — `session/prompt{session_id, turn_id, prompt, context_items?}`.
    /// The response carries `{status: "finished", stop_reason} |
    /// {status: "running"}` (a pending turn finishes by notification/cancel).
    pub fn submit(
        &mut self,
        session_ref: &str,
        turn_id: &str,
        prompt: &Json,
    ) -> Result<Json, HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.lifted.push(LiftedObservation::adapter_fact(
            "turn.started",
            Json::obj([("turn_id", Json::str(turn_id))]),
        ));
        let res = self
            .transport
            .request(
                "session/prompt",
                &Json::obj([
                    ("session_id", Json::str(sid)),
                    ("turn_id", Json::str(turn_id)),
                    ("prompt", prompt.clone()),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        if res.get("status").and_then(Json::as_str) == Some("finished") {
            let raw = res
                .get("stop_reason")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            self.lifted.push(LiftedObservation::adapter_fact(
                "turn.finished",
                Json::obj([
                    ("turn_id", Json::str(turn_id)),
                    ("stop_reason_raw", Json::str(&raw)),
                    ("stop_reason", crate::proj::lift_stop_reason(&raw).to_json()),
                ]),
            ));
        }
        Ok(res)
    }

    /// `cancel` — `session/cancel{session_id, turn_id?}`.
    pub fn cancel(&mut self, session_ref: &str, turn_id: Option<&str>) -> Result<(), HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/cancel",
                &Json::obj([
                    ("session_id", Json::str(sid)),
                    (
                        "turn_id",
                        turn_id.map_or(Json::Null, |t| Json::str(t.to_string())),
                    ),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        Ok(())
    }

    /// `resume` — `cold` → `session/load{session_ref}` (a fresh protocol
    /// session over the recorded state); `warm` → `session/resume{session_ref}`
    /// on the live transport; `none` → schema violation upstream (the service
    /// never calls resume for it).
    pub fn resume(
        &mut self,
        cursor: &ResumeCursor,
        new_session_ref: &str,
    ) -> Result<String, HostingError> {
        let method = match cursor.mode.as_str() {
            "cold" => "session/load",
            "warm" => "session/resume",
            other => {
                return Err(HostingError::SessionState {
                    session: cursor.session_ref.clone(),
                    detail: format!("resume mode {other} is not a session-ABI mode"),
                })
            }
        };
        let res = self
            .transport
            .request(
                method,
                &Json::obj([
                    ("session_ref", Json::str(&cursor.session_ref)),
                    ("resume_seq", Json::Int(cursor.seq as i64)),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        let sid = res
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| HostingError::SchemaViolation {
                path: "resume.session_id".into(),
                detail: "the participant returned no session id".into(),
            })?
            .to_string();
        self.sessions
            .insert(new_session_ref.to_string(), sid.clone());
        self.lifted.push(LiftedObservation::adapter_fact(
            "session.opened",
            Json::obj([
                ("session_ref", Json::str(new_session_ref)),
                ("protocol_session_id", Json::str(sid.clone())),
                ("resumed_from", Json::str(&cursor.session_ref)),
            ]),
        ));
        Ok(sid)
    }

    /// `close` — `session/close{session_id}` → the end-state snapshot the
    /// participant-side reports; the *kernel* snapshot is the service's own
    /// event log (the adapter reports, the service decides).
    pub fn close(&mut self, session_ref: &str) -> Result<Json, HostingError> {
        let sid = self.protocol_id(session_ref)?;
        let res = self
            .transport
            .request(
                "session/close",
                &Json::obj([("session_id", Json::str(sid))]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        self.sessions.remove(session_ref);
        Ok(res)
    }

    /// `set_coordinate` — `session/set_config_option{config_id, value}`
    /// (ACP's coordinate surface; the dimension gates service-side).
    pub fn set_coordinate(
        &mut self,
        session_ref: &str,
        dimension: &str,
        value: &Json,
    ) -> Result<(), HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/set_config_option",
                &Json::obj([
                    ("session_id", Json::str(sid)),
                    ("config_id", Json::str(dimension)),
                    ("value", value.clone()),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        self.lifted.push(LiftedObservation::adapter_fact(
            "coordinate.changed",
            Json::obj([
                ("dimension", Json::str(dimension)),
                ("value", value.clone()),
            ]),
        ));
        Ok(())
    }

    /// `steer` — `session/steer{session_id, turn_id, text}` (a mid-turn
    /// steering message).
    pub fn steer(
        &mut self,
        session_ref: &str,
        turn_id: &str,
        text: &str,
    ) -> Result<(), HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/steer",
                &Json::obj([
                    ("session_id", Json::str(sid)),
                    ("turn_id", Json::str(turn_id)),
                    ("text", Json::str(text)),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        Ok(())
    }

    /// `account` — `account/status` — exact account data (capability-declared).
    pub fn account(&mut self) -> Result<Json, HostingError> {
        self.transport
            .request("account/status", &Json::obj([]))
            .map_err(|e| HostingError::Transport { detail: e })
    }

    /// `export` — `session/export{session_id}` — the trajectory bundle
    /// (capability-declared).
    pub fn export(&mut self, session_ref: &str) -> Result<Json, HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/export",
                &Json::obj([("session_id", Json::str(sid))]),
            )
            .map_err(|e| HostingError::Transport { detail: e })
    }

    /// `elicit` — `session/elicit{session_id, prompt}` — the Lab asks the
    /// participant a structured question (capability-declared upcall target).
    pub fn elicit(&mut self, session_ref: &str, prompt: &Json) -> Result<Json, HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/elicit",
                &Json::obj([("session_id", Json::str(sid)), ("prompt", prompt.clone())]),
            )
            .map_err(|e| HostingError::Transport { detail: e })
    }

    /// Deliver a credential over a declared `credential_supply` channel —
    /// `session/credential{channel, ref}` carries a **ref**, never the value
    /// (the participant fetches through the channel; the service checked the
    /// channel is declared).
    pub fn deliver_credential(
        &mut self,
        session_ref: &str,
        channel: &str,
        credential_ref: &str,
    ) -> Result<(), HostingError> {
        let sid = self.protocol_id(session_ref)?;
        self.transport
            .request(
                "session/credential",
                &Json::obj([
                    ("session_id", Json::str(sid)),
                    ("channel", Json::str(channel)),
                    ("credential_ref", Json::str(credential_ref)),
                ]),
            )
            .map_err(|e| HostingError::Transport { detail: e })?;
        Ok(())
    }

    fn protocol_id(&self, session_ref: &str) -> Result<String, HostingError> {
        self.sessions
            .get(session_ref)
            .cloned()
            .ok_or_else(|| HostingError::SessionState {
                session: session_ref.to_string(),
                detail: "no protocol session bound".to_string(),
            })
    }

    /// Answer one pending upcall (`session/request_permission` → the Lab
    /// decider; every other method → a protocol-level refusal the participant
    /// observes as unsupported — the boundary surfaces only declared upcalls).
    fn answer_upcall(&mut self, u: &PendingUpcall) -> Result<Json, String> {
        match u.method.as_str() {
            "session/request_permission" => {
                let decided = match &mut self.decider {
                    Some(d) => d(&u.params),
                    None => Json::obj([("decision", Json::str("deny"))]),
                };
                let decision = decided
                    .get("decision")
                    .and_then(Json::as_str)
                    .unwrap_or("deny")
                    .to_string();
                // The kernel's own channel rows — `mediated`, kernel authority.
                self.lifted.push(LiftedObservation::kernel_fact(
                    "permission.requested",
                    Json::obj([
                        (
                            "permission_id",
                            u.params.get("tool_call_id").cloned().unwrap_or(Json::Null),
                        ),
                        ("params", u.params.clone()),
                    ]),
                ));
                self.lifted.push(LiftedObservation::kernel_fact(
                    "permission.decided",
                    Json::obj([
                        ("decision", Json::str(&decision)),
                        (
                            "decider",
                            decided
                                .get("decider")
                                .cloned()
                                .unwrap_or_else(|| Json::str("policy")),
                        ),
                        (
                            "approval_wait_ms",
                            decided
                                .get("approval_wait_ms")
                                .cloned()
                                .unwrap_or(Json::Int(0)),
                        ),
                    ]),
                ));
                Ok(decided)
            }
            _ => Err(format!("unsupported upcall: {}", u.method)),
        }
    }

    /// Pump the protocol — drain notifications (lifted), answer pending
    /// upcalls through the Lab's decider, drain intercepted model
    /// observations. Returns the observations lifted this pump.
    pub fn pump(&mut self) -> Vec<LiftedObservation> {
        for u in self.transport.drain_upcalls() {
            let result = self.answer_upcall(&u);
            self.transport.respond_upcall(&u.id, result);
        }
        for (method, params) in self.transport.drain_notifications() {
            self.lift_notification(&method, &params);
        }
        for obs in self.transport.drain_model_observations() {
            self.lift_model_observation(&obs);
        }
        std::mem::take(&mut self.lifted)
    }

    /// The ACP `session/update` lift table — `sessionUpdate` kind → hosted
    /// kind (§6.6 §5's session-ABI lift). Unknown kinds preserve verbatim
    /// (`_`-prefixed and vendor spellings land as unknown-kind leaves).
    fn lift_notification(&mut self, method: &str, params: &Json) {
        let raw = self.next_raw("notif");
        let push = |obs: LiftedObservation, this: &mut AdapterA| {
            let mut o = obs;
            o.raw_ref = Some(raw.clone());
            this.lifted.push(o);
        };
        match method {
            "session/update" => {
                let update_kind = params
                    .get("sessionUpdate")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                match update_kind {
                    "agent_message_chunk" => {
                        let text = params
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(Json::as_str)
                            .unwrap_or("");
                        push(
                            LiftedObservation::participant_observed(
                                "message.delta",
                                Json::obj([("text", Json::str(text))]),
                            ),
                            self,
                        );
                    }
                    "agent_thought_chunk" => {
                        let text = params
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(Json::as_str)
                            .unwrap_or("");
                        push(
                            LiftedObservation::participant_observed(
                                "thought.delta",
                                Json::obj([("text", Json::str(text))]),
                            ),
                            self,
                        );
                    }
                    "user_message_chunk" => {
                        let text = params
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(Json::as_str)
                            .unwrap_or("");
                        push(
                            LiftedObservation::participant_observed(
                                "message.completed",
                                Json::obj([("text", Json::str(text)), ("role", Json::str("user"))]),
                            ),
                            self,
                        );
                    }
                    "tool_call" => {
                        let hint = params
                            .get("kind_hint")
                            .and_then(Json::as_str)
                            .or_else(|| params.get("kind").and_then(Json::as_str))
                            .unwrap_or("tool");
                        push(
                            LiftedObservation::participant_observed(
                                "tool.proposed",
                                Json::obj([
                                    (
                                        "tool_call_id",
                                        params.get("toolCallId").cloned().unwrap_or(Json::Null),
                                    ),
                                    ("kind_hint", Json::str(hint)),
                                ]),
                            ),
                            self,
                        );
                    }
                    "tool_call_update" => {
                        let status = params
                            .get("status")
                            .and_then(Json::as_str)
                            .unwrap_or("completed");
                        push(
                            LiftedObservation::participant_observed(
                                "tool.completed",
                                Json::obj([
                                    (
                                        "tool_call_id",
                                        params.get("toolCallId").cloned().unwrap_or(Json::Null),
                                    ),
                                    ("status", Json::str(status)),
                                ]),
                            ),
                            self,
                        );
                    }
                    "current_mode_update" => {
                        push(
                            LiftedObservation::participant_observed(
                                "coordinate.changed",
                                Json::obj([
                                    ("dimension", Json::str("mode")),
                                    ("value", params.get("modeId").cloned().unwrap_or(Json::Null)),
                                ]),
                            ),
                            self,
                        );
                    }
                    "usage_update" => {
                        push(
                            LiftedObservation::participant_observed(
                                "usage.reported",
                                params.get("usage").cloned().unwrap_or(Json::obj([])),
                            ),
                            self,
                        );
                    }
                    "session_info_update" | "available_commands_update" | "plan" => {
                        // ACP protocol facts with no hosted kind — preserved
                        // as an unknown-kind leaf (I-3, CC3).
                        push(
                            LiftedObservation::participant_observed(update_kind, params.clone()),
                            self,
                        );
                    }
                    _ => {
                        push(
                            LiftedObservation::participant_observed(update_kind, params.clone()),
                            self,
                        );
                    }
                }
            }
            "session/turn_finished" => {
                let raw_reason = params
                    .get("stop_reason")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                push(
                    LiftedObservation::adapter_fact(
                        "turn.finished",
                        Json::obj([
                            (
                                "turn_id",
                                params.get("turn_id").cloned().unwrap_or(Json::Null),
                            ),
                            ("stop_reason_raw", Json::str(&raw_reason)),
                            (
                                "stop_reason",
                                crate::proj::lift_stop_reason(&raw_reason).to_json(),
                            ),
                        ]),
                    ),
                    self,
                );
            }
            _ => {
                // Unknown notification — preserved verbatim (I-3).
                push(
                    LiftedObservation::participant_observed(method, params.clone()),
                    self,
                );
            }
        }
    }

    /// Lift one intercepted model observation — `{model_call_id, phase,
    /// usage?, status?, request?, response?}` on the proxy channel
    /// (`mediated`, `intercept` origin — the Lab measured it).
    fn lift_model_observation(&mut self, obs: &Json) {
        let raw = self.next_raw("proxy");
        let phase = obs.get("phase").and_then(Json::as_str).unwrap_or("");
        let id = obs.get("model_call_id").cloned().unwrap_or(Json::Null);
        match phase {
            "started" => {
                let mut o = LiftedObservation::intercepted(
                    "model.call.started",
                    Json::obj([("model_call_id", id)]),
                );
                o.raw_ref = Some(raw);
                self.lifted.push(o);
            }
            "completed" | "failed" => {
                let mut p = BTreeMap::new();
                p.insert("model_call_id".into(), id);
                p.insert(
                    "status".into(),
                    Json::str(if phase == "failed" {
                        "failed"
                    } else {
                        "completed"
                    }),
                );
                if let Some(u) = obs.get("usage") {
                    p.insert("usage".into(), u.clone());
                }
                let mut o = LiftedObservation::intercepted("model.call.completed", Json::Obj(p));
                o.raw_ref = Some(raw);
                self.lifted.push(o);
            }
            _ => {
                let mut o = LiftedObservation::intercepted("model.call.observed", obs.clone());
                o.raw_ref = Some(raw);
                self.lifted.push(o);
            }
        }
    }
}

/// The `adapter` record Adapter A registers — claims `session_abi` at every
/// placement; `declaration_defaults` are the conservative claim set the
/// probe catalogue verifies (nothing the adapter cannot lift is claimed).
/// `debt` is the complete conditioned artefact (AC-R-2.10.6-8 — a record
/// without complete debt is refused at attach, by schema *and* by check).
pub fn adapter_a_record(
    adapter_version: &str,
    intercept: ModelIoIntercept,
    debt: hh_hir::records::AssumptionDebtRecord,
) -> AdapterRecord {
    let mut defaults = BTreeMap::new();
    for (dim, v) in [
        ("streaming", "supported"),
        ("interrupt", "supported"),
        ("permission_surface", "supported"),
        ("coordinate_model", "supported"),
        ("usage_reporting", "supported"),
        ("resume_cold", "supported"),
        ("resume_warm", "supported"),
    ] {
        defaults.insert(dim.to_string(), Json::str(v));
    }
    let mut ext = BTreeMap::new();
    ext.insert(
        "model_io_intercept".to_string(),
        Json::str(intercept.as_str()),
    );
    AdapterRecord {
        adapter_id: "hh.adapter.a".to_string(),
        version_id: adapter_version.to_string(),
        hosting_mechanism: HostingMechanism::SessionAbi,
        participant_selector: Json::obj([("mechanism", Json::str("session_abi"))]),
        declaration_defaults: defaults,
        placement_supported: [
            ProcessPlacement::InEnvironment,
            ProcessPlacement::LabHost,
            ProcessPlacement::RemoteService,
        ]
        .into_iter()
        .collect(),
        lowering_table_ref: None,
        loss_report_ref: None,
        debt,
        ext,
    }
}
