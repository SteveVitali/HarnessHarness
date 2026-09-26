//! `SessionDriver` — the records-in/records-out kernel seam the
//! serve loop calls (D6 — the session is the same language). The
//! driver hands the *durable* event stream back; the serve loop only
//! renders + transports — Π decisions and ledger writes stay in the
//! kernel/driver, never in the adapter.

use std::collections::VecDeque;

use hh_wire::json::Json;

/// `SessionInfo` — `session/new`'s result.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// The minted session id (`sess-*` — the run/dispatch-family id).
    pub session_id: String,
    /// Config options the session carries (`configOptions[]` — mode/
    /// model/thought-configurations echo).
    pub config_options: Json,
    /// `mcpServers[]` the client declared — returned to the serve
    /// loop so the H5 pending-review response carries them verbatim
    /// (never admitted to tools).
    pub mcp_servers_declared: Json,
}

/// `TurnDrive` — the result of one `session/prompt` turn: the
/// durable events the turn appended (`[(seq, class, payload)]`, seq
/// ascending) and the terminal stop reason.
#[derive(Debug, Clone)]
pub struct TurnDrive {
    /// The turn's appended durable events.
    pub events: Vec<(u64, String, Json)>,
    /// The `stopReason` the turn ended under (`end_turn`,
    /// `cancelled`, `refused`, `idle`).
    pub stop_reason: String,
}

/// `SessionDriver` — the agent-side driver contract.
pub trait SessionDriver {
    /// `session/new{meta, mcpServers, config, history}` → the
    /// session record. `history: None` = fresh; `Some(events)` is a
    /// fork/replay the driver folds into the new session's visible
    /// prefix.
    fn new_session(&mut self, params: &Json) -> Result<SessionInfo, String>;

    /// `session/prompt{sessionId, content}` → the turn drive
    /// (`events` are appended durably by the driver; the serve loop
    /// renders them to updates).
    fn prompt(&mut self, session_id: &str, content: &Json) -> Result<TurnDrive, String>;

    /// `session/cancel{sessionId}` → the cancellation's appended
    /// events (`[]` when the session was already idle).
    fn cancel(&mut self, session_id: &str) -> Result<Vec<(u64, String, Json)>, String>;

    /// `session/resume{sessionId, replayFrom?}` → the durable events
    /// to re-serve (`seq > replay_from` when set; the durable prefix
    /// when absent). Replay comes from the durable record — never a
    /// speculative buffer.
    fn resume(
        &mut self,
        session_id: &str,
        replay_from: Option<u64>,
    ) -> Result<Vec<(u64, String, Json)>, String>;

    /// `session/close{sessionId}` — terminal close (the durable
    /// record survives; the session stops answering `session/*`).
    fn close(&mut self, session_id: &str) -> Result<(), String>;

    /// The permission decision arrived (`session/request_permission`'s
    /// response routed through Π — the adapter forwards verbatim).
    /// Returns the resolution's appended events.
    fn permission_decision(
        &mut self,
        session_id: &str,
        request_id: &str,
        decision: &Json,
    ) -> Result<Vec<(u64, String, Json)>, String>;

    /// `_hh/ledger/read{from_seq}` → the durable tail verbatim.
    fn ledger_read(&self, from_seq: u64) -> Result<Json, String>;

    /// `_hh/account` → the projection (spend/run-tree overview —
    /// never authority-bearing).
    fn account(&self) -> Result<Json, String>;

    /// `_hh/participant/describe{participant}` → the role/dispatch-
    /// family descriptor.
    fn participant_describe(&self, participant: &Json) -> Result<Json, String>;
}

/// `FixtureDriver` — a scripted `SessionDriver` for tests: queue
/// turn drives + canned `_hh/*` answers. Sessions get `sess-fixture-N`
/// ids; `prompt` pops the scripted drive; `permission_decision`
/// pops the scripted resolution events.
/// One scripted `permission_decision` outcome — the batch of
/// `(seq, kind, event)` ledger appends it resolves, or its refusal.
type ScriptedDecision = Result<Vec<(u64, String, Json)>, String>;

pub struct FixtureDriver {
    /// The next session id counter.
    next_session: u64,
    /// Scripted turn drives (`session/prompt` pops them in order).
    pub scripted_turns: VecDeque<Result<TurnDrive, String>>,
    /// Scripted permission-resolution events.
    pub scripted_decisions: VecDeque<ScriptedDecision>,
    /// The canned `_hh/ledger/read` tail.
    pub ledger_tail: Vec<(u64, String, Json)>,
    /// Open sessions (`session_id → durable events appended`).
    pub sessions: BTreeMapForSessions,
    /// Every `permission_decision` call `(session_id, request_id,
    /// decision-verbatim)` — the AC-R-2.5.4-6 trace (the adapter
    /// forwarded, never decided).
    pub decisions_seen: Vec<(String, String, Json)>,
}

type BTreeMapForSessions = std::collections::BTreeMap<String, Vec<(u64, String, Json)>>;

impl FixtureDriver {
    /// A fresh fixture (`next_session` starts at 1).
    pub fn new() -> Self {
        Self {
            next_session: 1,
            scripted_turns: VecDeque::new(),
            scripted_decisions: VecDeque::new(),
            ledger_tail: Vec::new(),
            sessions: BTreeMapForSessions::new(),
            decisions_seen: Vec::new(),
        }
    }
}

impl Default for FixtureDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionDriver for FixtureDriver {
    fn new_session(&mut self, params: &Json) -> Result<SessionInfo, String> {
        let id = format!("sess-fixture-{}", self.next_session);
        self.next_session += 1;
        let history: Vec<(u64, String, Json)> = match params.get("history") {
            Some(Json::Arr(a)) => a
                .iter()
                .map(|e| {
                    (
                        e.get("seq")
                            .and_then(Json::as_int)
                            .map(|i| i as u64)
                            .unwrap_or(0),
                        e.get("class")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        e.get("payload").cloned().unwrap_or(Json::obj([])),
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        self.sessions.insert(id.clone(), history);
        Ok(SessionInfo {
            session_id: id,
            config_options: params.get("config").cloned().unwrap_or(Json::Arr(vec![])),
            mcp_servers_declared: params
                .get("mcpServers")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
        })
    }

    fn prompt(&mut self, session_id: &str, _content: &Json) -> Result<TurnDrive, String> {
        if !self.sessions.contains_key(session_id) {
            return Err("session/cancelled — unknown session".to_string());
        }
        let drive = self.scripted_turns.pop_front().unwrap_or(Ok(TurnDrive {
            events: vec![],
            stop_reason: "end_turn".to_string(),
        }))?;
        if let Some(events) = self.sessions.get_mut(session_id) {
            events.extend(drive.events.iter().cloned());
        }
        Ok(drive)
    }

    fn cancel(&mut self, session_id: &str) -> Result<Vec<(u64, String, Json)>, String> {
        if !self.sessions.contains_key(session_id) {
            return Err("session/cancelled — unknown session".to_string());
        }
        let seq = self.sessions[session_id]
            .last()
            .map(|e| e.0 + 1)
            .unwrap_or(1);
        let ev = vec![(
            seq,
            "runtime.session.cancelled".to_string(),
            Json::obj([("session_id", Json::str(session_id.to_string()))]),
        )];
        if let Some(events) = self.sessions.get_mut(session_id) {
            events.extend(ev.iter().cloned());
        }
        Ok(ev)
    }

    fn resume(
        &mut self,
        session_id: &str,
        replay_from: Option<u64>,
    ) -> Result<Vec<(u64, String, Json)>, String> {
        let events = self
            .sessions
            .get(session_id)
            .ok_or_else(|| "session/unknown".to_string())?;
        Ok(events
            .iter()
            .filter(|(seq, _, _)| replay_from.map(|f| *seq > f).unwrap_or(true))
            .cloned()
            .collect())
    }

    fn close(&mut self, session_id: &str) -> Result<(), String> {
        if !self.sessions.contains_key(session_id) {
            return Err("session/unknown".to_string());
        }
        Ok(())
    }

    fn permission_decision(
        &mut self,
        session_id: &str,
        request_id: &str,
        decision: &Json,
    ) -> Result<Vec<(u64, String, Json)>, String> {
        if !self.sessions.contains_key(session_id) {
            return Err("session/unknown".to_string());
        }
        self.decisions_seen.push((
            session_id.to_string(),
            request_id.to_string(),
            decision.clone(),
        ));
        let out = self.scripted_decisions.pop_front().unwrap_or(Ok(vec![]))?;
        if let Some(events) = self.sessions.get_mut(session_id) {
            events.extend(out.iter().cloned());
        }
        Ok(out)
    }

    fn ledger_read(&self, from_seq: u64) -> Result<Json, String> {
        Ok(Json::Arr(
            self.ledger_tail
                .iter()
                .filter(|(seq, _, _)| *seq > from_seq)
                .map(|(seq, class, payload)| {
                    Json::obj([
                        ("seq", Json::Int(*seq as i64)),
                        ("class", Json::str(class.clone())),
                        ("payload", payload.clone()),
                    ])
                })
                .collect(),
        ))
    }

    fn account(&self) -> Result<Json, String> {
        Ok(Json::obj([("fixture", Json::Bool(true))]))
    }

    fn participant_describe(&self, participant: &Json) -> Result<Json, String> {
        Ok(Json::obj([
            (
                "participant",
                participant
                    .get("participant")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
            ("family", Json::str("dispatch")),
        ]))
    }
}
