//! `EmbedDriver` — the `hh-embed/1`-backed [`SessionDriver`] (S4.12;
//! §7.4; ADR-0176/0179).
//!
//! The ACP surface is a **client of the embedding contract** — the same
//! contract every first-party surface drives. `EmbedDriver` wraps an
//! [`EmbedCall`] (one `hh-embed/1` op → `result | error` Json) so the
//! same driver rides binding (a) (`EmbedService::handle`) or binding
//! (b) (the CLI's `ProcessBoundary`) — the driver never names either;
//! the caller supplies the seam.
//!
//! Operation mapping (Group H/S/W/R over `hh-embed/1`):
//!
//! - [`SessionDriver::new_session`] → `open_session{spec:{kind:"new"}}`
//!   with the spec the caller bound at construction (definition,
//!   environment, attendance, approval_mode — the CLI's `hh acp` verb
//!   builds it). `params.history` (the serve loop's `session/fork`
//!   path) lowers onto `fork{at:{kind:"seq"}}` against the session the
//!   last `resume` read — a fork is a branch op, never a re-open.
//! - [`SessionDriver::prompt`] → `submit{input}` then `read` the
//!   durable tail until `state.turn.completed` / a durable
//!   `security.permission.requested{decider:human}` ask / the run
//!   finishes. `TurnDrive.events` are the `(seq, class, payload)`
//!   triples of the durable envelopes verbatim — the serve loop's
//!   `_meta.hh.source_seq` trace mapping needs nothing else.
//! - [`SessionDriver::permission_decision`] → `respond_permission`
//!   against the `permission_id` the driver queued when the durable
//!   `requested`/`pending` row crossed the watermark (the serve
//!   loop's `perm-*` request ids are transport-local — the kernel's
//!   `permission_id` is the only join key). `allowed`/`selected`
//!   map to `selected{option_id}` (`allow_once` default —
//!   `allow_lease` is DEFERRED, ADR-0214); `denied`/`cancelled` map
//!   to `cancelled`. Then the resolution's durable tail is re-read
//!   and returned.
//! - [`SessionDriver::cancel`] → `cancel{scope:{kind:"turn"}}` +
//!   the tail. [`SessionDriver::resume`] → `read` (replay is the
//!   durable record, never a buffer). [`SessionDriver::close`] →
//!   `close{reason:"done"}`.
//! - [`SessionDriver::ledger_read`] → `read` verbatim;
//!   [`SessionDriver::account`] → `account`;
//!   [`SessionDriver::participant_describe`] → `lab.hosting.describe`
//!   (the honest `hosting_plane_absent` refusal propagates as data).
//!
//! Idempotency: the driver mints `acp-{op}-{session}-{n}` keys —
//! deterministic per driver instance, never two calls sharing a key
//! (I-7). A retried `submit` (transport retry) reuses the same key.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use hh_wire::json::Json;

use crate::driver::{SessionDriver, SessionInfo, TurnDrive};

/// One `hh-embed/1` op invocation — the seam shape `Boundary::call`
/// (CLI binding (b)) and `EmbedService::handle` (binding (a)) both
/// lower to. `Ok` carries the op's `result` object; `Err` carries the
/// kernel's error envelope `{kind, code, message, data?}` verbatim —
/// the driver inspects `kind`, never a transport-specific error type.
///
/// No `Send` bound — the driver is single-threaded (`RefCell` inside),
/// and the CLI's adapter borrows `&mut dyn Boundary` for the serve
/// loop's lifetime.
pub trait EmbedCall {
    fn call(&mut self, op: &str, params: Json) -> Result<Json, Json>;
}

/// `FnMut` adapter — binding (a) tests wrap `EmbedService::handle`'s
/// request build + response decode; binding (b) wraps
/// `Boundary::call` (the CLI provides that adapter — `hh-acp` never
/// names `hh-cli`, CC5).
pub struct FnEmbedCall<F: FnMut(&str, Json) -> Result<Json, Json>> {
    f: F,
}

impl<F: FnMut(&str, Json) -> Result<Json, Json>> FnEmbedCall<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F: FnMut(&str, Json) -> Result<Json, Json>> EmbedCall for FnEmbedCall<F> {
    fn call(&mut self, op: &str, params: Json) -> Result<Json, Json> {
        (self.f)(op, params)
    }
}

/// One open ACP session's kernel-side state.
struct Sess {
    /// The `hh-embed/1` `session_id` (the ACP `sessionId` *is* this —
    /// one session id across both contracts, never a mapped pair).
    embed_session_id: String,
    /// The run the session drives.
    run_id: String,
    /// The high-water durable seq already handed to the serve loop
    /// (`read` cursor `seq > watermark` — dedupe is the record's).
    watermark: u64,
    /// `permission_id`s queued in durable order from
    /// `security.permission.requested`/`security.permission.pending`
    /// rows — `permission_decision` resolves them in order (the
    /// serve loop's `perm-*` request ids carry no kernel id).
    pending_permissions: VecDeque<String>,
}

/// The `hh-embed/1`-backed session driver. The `'a` lifetime is the
/// borrowed call channel's (the CLI's `BoundaryEmbedCall` borrows its
/// `Boundary` for one invocation); `EmbedDriver<'static>` is the
/// binding-(a)/owned-channel case.
pub struct EmbedDriver<'a> {
    call: RefCell<Box<dyn EmbedCall + 'a>>,
    /// The `OpenSpec::New` object every `session/new` opens with —
    /// `{kind:"new", definition, environment, attendance,
    /// approval_mode?, …}` verbatim; the caller (the CLI's `acp`
    /// verb, a host embedding the artefact) owns its construction.
    open_spec: Json,
    /// ACP `sessionId` → kernel-side state.
    sessions: BTreeMap<String, Sess>,
    /// Idempotency-key counter (`acp-{op}-{session}-{n}`).
    counter: u64,
    /// The session the last `resume` read — `session/fork`'s
    /// `new_session{history}` forks its run at the last replayed seq.
    last_resumed: Option<String>,
}

impl<'a> EmbedDriver<'a> {
    /// `new(call, open_spec)` — `open_spec` is the `OpenSpec::New`
    /// object (`{kind:"new", …}`) placed under `open_session`'s
    /// `spec` member on every `session/new`.
    pub fn new(call: impl EmbedCall + 'a, open_spec: Json) -> Self {
        Self {
            call: RefCell::new(Box::new(call)),
            open_spec,
            sessions: BTreeMap::new(),
            counter: 0,
            last_resumed: None,
        }
    }

    /// Construct over a bare `FnMut` (the binding-(a) test seam).
    pub fn over_fn(f: impl FnMut(&str, Json) -> Result<Json, Json> + 'a, open_spec: Json) -> Self {
        Self::new(FnEmbedCall::new(f), open_spec)
    }

    /// The `run_id` a session drives — the surface's trace line and
    /// the `session/load` reattach path's selector (never a join key
    /// the kernel reads back).
    pub fn run_id_of(&self, acp_session: &str) -> Option<&str> {
        self.sessions.get(acp_session).map(|s| s.run_id.as_str())
    }

    fn key(&mut self, op: &str, session: &str) -> String {
        self.counter += 1;
        format!("acp-{op}-{session}-{}", self.counter)
    }

    fn op(&mut self, op: &str, params: Json) -> Result<Json, String> {
        self.op_shared(op, params)
    }

    /// `&self` variant for the read-side `_hh/*` methods.
    fn op_shared(&self, op: &str, params: Json) -> Result<Json, String> {
        self.call.borrow_mut().call(op, params).map_err(err_text)
    }

    fn sess(&self, acp_session: &str) -> Result<&Sess, String> {
        self.sessions
            .get(acp_session)
            .ok_or_else(|| "session/unknown".to_string())
    }

    fn sess_mut(&mut self, acp_session: &str) -> Result<&mut Sess, String> {
        self.sessions
            .get_mut(acp_session)
            .ok_or_else(|| "session/unknown".to_string())
    }

    /// `read` the session's durable tail above its watermark; fold
    /// `permission_id`s into the pending queue and advance the
    /// watermark. Returns the new `(seq, class, payload)` triples.
    fn tail(&mut self, acp_session: &str) -> Result<Vec<(u64, String, Json)>, String> {
        let (embed_sid, from) = {
            let s = self.sess(acp_session)?;
            (s.embed_session_id.clone(), s.watermark + 1)
        };
        let page = self.op(
            "read",
            Json::obj([
                ("session_id", Json::str(embed_sid)),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(from as i64))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(512)),
            ]),
        )?;
        let mut out = Vec::new();
        if let Json::Arr(events) = page.get("events").cloned().unwrap_or(Json::Arr(vec![])) {
            let s = self.sess_mut(acp_session)?;
            for e in events {
                let seq = e.get("seq").and_then(Json::as_int).unwrap_or(0) as u64;
                if seq <= s.watermark {
                    continue; // inclusive-cursor re-read — dedupe by seq (I-7)
                }
                s.watermark = seq;
                let class = e
                    .get("class")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                let payload = e.get("payload").cloned().unwrap_or(Json::obj([]));
                queue_permission(s, &class, &payload);
                out.push((seq, class, payload));
            }
        }
        Ok(out)
    }
}

/// `security.permission.requested` (`decider:human` — the row the
/// serve loop renders as `session/request_permission`) and
/// `security.permission.pending` both name the ask's `permission_id`;
/// queue it once, in durable order.
fn queue_permission(s: &mut Sess, class: &str, payload: &Json) {
    let asked = class == "security.permission.pending"
        || (class == "security.permission.requested"
            && payload.get("decider").and_then(Json::as_str) == Some("human"));
    if !asked {
        return;
    }
    if let Some(pid) = payload.get("permission_id").and_then(Json::as_str) {
        if !s.pending_permissions.iter().any(|p| p == pid) {
            s.pending_permissions.push_back(pid.to_string());
        }
    }
}

/// The wire error → the driver's `String` channel — the envelope's
/// `{kind, code, message}` rendered, never retyped.
fn err_text(e: Json) -> String {
    let kind = e.get("kind").and_then(Json::as_str).unwrap_or("error");
    let code = e
        .get("code")
        .and_then(Json::as_int)
        .map(|c| c.to_string())
        .or_else(|| e.get("code").and_then(Json::as_str).map(str::to_string))
        .unwrap_or_else(|| "?".into());
    let msg = e.get("message").and_then(Json::as_str).unwrap_or_default();
    format!("{kind}({code}): {msg}")
}

impl SessionDriver for EmbedDriver<'_> {
    fn new_session(&mut self, params: &Json) -> Result<SessionInfo, String> {
        // `session/fork` arrives as `new_session{history}` — a branch
        // op against the last resumed session, never a fresh open.
        let history: Vec<&Json> = match params.get("history") {
            Some(Json::Arr(a)) => a.iter().collect(),
            _ => Vec::new(),
        };
        if !history.is_empty() {
            let source = self
                .last_resumed
                .clone()
                .ok_or_else(|| "session/fork: no resumed source".to_string())?;
            let at = history
                .iter()
                .filter_map(|e| e.get("seq").and_then(Json::as_int))
                .max()
                .unwrap_or(0);
            let embed_sid = self.sess(&source)?.embed_session_id.clone();
            let result = self.op(
                "fork",
                Json::obj([
                    ("session_id", Json::str(embed_sid)),
                    (
                        "at",
                        Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(at))]),
                    ),
                ]),
            )?;
            return self.register(result, params);
        }
        let key = self.key("open", "new");
        let spec = self.open_spec.clone();
        let result = self.op(
            "open_session",
            Json::obj([("spec", spec), ("idempotency_key", Json::str(key))]),
        )?;
        self.register(result, params)
    }

    fn prompt(&mut self, session_id: &str, content: &Json) -> Result<TurnDrive, String> {
        let embed_sid = self.sess(session_id)?.embed_session_id.clone();
        // ACP `content` is a block list (`[{type:"text",text}]`) or a
        // bare `{text}` — the kernel's `input[]` wants `text`-member
        // blocks; non-text blocks ride verbatim.
        let input: Vec<Json> = match content {
            Json::Arr(blocks) => blocks
                .iter()
                .map(|b| {
                    if b.get("text").is_some() {
                        b.clone()
                    } else if let Some(t) = b
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(Json::as_str)
                    {
                        Json::obj([("text", Json::str(t))])
                    } else {
                        b.clone()
                    }
                })
                .collect(),
            other => vec![other.clone()],
        };
        let key = self.key("submit", session_id);
        self.op(
            "submit",
            Json::obj([
                ("session_id", Json::str(embed_sid)),
                ("input", Json::Arr(input)),
                ("idempotency_key", Json::str(key)),
            ]),
        )?;
        // Poll the durable tail: `submit` drives the turn inline on the
        // scripted model port; an `ask` or a live turn leaves the tail
        // short of `completed` — the serve loop's request_permission
        // round-trip is what unblocks it (durable rows only, never a
        // speculative buffer).
        let mut events = Vec::new();
        let mut idle_reads = 0;
        loop {
            let batch = self.tail(session_id)?;
            if batch.is_empty() {
                idle_reads += 1;
                if idle_reads >= 4 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            idle_reads = 0;
            let terminal = batch
                .iter()
                .any(|(_, c, _)| c == "state.turn.completed" || c == "lifecycle.run.finished");
            let asked = !self.sess(session_id)?.pending_permissions.is_empty();
            events.extend(batch);
            if terminal || asked {
                break;
            }
        }
        let stop_reason = events
            .iter()
            .rev()
            .find(|(_, c, _)| c == "state.turn.completed")
            .and_then(|(_, _, p)| {
                p.get("stop_reason")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                if events
                    .iter()
                    .any(|(_, c, _)| c == "runtime.session.cancelled")
                {
                    "cancelled".to_string()
                } else {
                    "end_turn".to_string()
                }
            });
        Ok(TurnDrive {
            events,
            stop_reason,
        })
    }

    fn cancel(&mut self, session_id: &str) -> Result<Vec<(u64, String, Json)>, String> {
        let embed_sid = self.sess(session_id)?.embed_session_id.clone();
        let turn = self
            .op(
                "describe",
                Json::obj([("session_id", Json::str(embed_sid.clone()))]),
            )
            .ok()
            .and_then(|d| {
                d.get("active_turn")
                    .or_else(|| d.get("turn_id"))
                    .and_then(Json::as_str)
                    .map(str::to_string)
            });
        let scope = match turn {
            Some(t) => Json::obj([("kind", Json::str("turn")), ("turn_id", Json::str(t))]),
            None => Json::obj([("kind", Json::str("run"))]),
        };
        self.op(
            "cancel",
            Json::obj([("session_id", Json::str(embed_sid)), ("scope", scope)]),
        )?;
        self.tail(session_id)
    }

    fn resume(
        &mut self,
        session_id: &str,
        replay_from: Option<u64>,
    ) -> Result<Vec<(u64, String, Json)>, String> {
        // Replay from the durable record — rewinds the watermark to the
        // requested point and re-reads (dedupe keeps monotonic order;
        // a `replayFrom` below the watermark re-serves the prefix).
        let from = replay_from.unwrap_or(0);
        {
            let s = self.sess_mut(session_id)?;
            if from < s.watermark {
                s.watermark = from;
            }
        }
        self.last_resumed = Some(session_id.to_string());
        self.tail(session_id)
    }

    fn close(&mut self, session_id: &str) -> Result<(), String> {
        let embed_sid = self.sess(session_id)?.embed_session_id.clone();
        self.op(
            "close",
            Json::obj([
                ("session_id", Json::str(embed_sid)),
                ("reason", Json::str("done")),
            ]),
        )
        .map(|_| ())
    }

    fn permission_decision(
        &mut self,
        session_id: &str,
        _request_id: &str,
        decision: &Json,
    ) -> Result<Vec<(u64, String, Json)>, String> {
        let (embed_sid, permission_id) = {
            let s = self.sess_mut(session_id)?;
            let pid = s
                .pending_permissions
                .pop_front()
                .ok_or_else(|| "permission_decision: no pending ask on this session".to_string())?;
            (s.embed_session_id.clone(), pid)
        };
        // ACP `{outcome{outcome, optionId?}}` → the boundary's
        // `PermissionOutcome`: `allowed`/`selected` →
        // `selected{option_id}` (`allow_once` when the peer named no
        // option — `allow_lease` is DEFERRED, ADR-0214);
        // `denied`/`cancelled` → `cancelled`.
        let outcome = match decision
            .get("outcome")
            .and_then(|o| o.get("outcome"))
            .and_then(Json::as_str)
            .unwrap_or("denied")
        {
            "selected" | "allowed" => {
                let option_id = decision
                    .get("outcome")
                    .and_then(|o| o.get("optionId"))
                    .and_then(Json::as_str)
                    .unwrap_or("allow_once");
                Json::obj([
                    ("kind", Json::str("selected")),
                    ("option_id", Json::str(option_id)),
                ])
            }
            _ => Json::obj([("kind", Json::str("cancelled"))]),
        };
        let key = self.key("respond", session_id);
        self.op(
            "respond_permission",
            Json::obj([
                ("session_id", Json::str(embed_sid)),
                ("permission_id", Json::str(permission_id)),
                ("outcome", outcome),
                ("idempotency_key", Json::str(key)),
            ]),
        )?;
        // The resolution unblocks the turn — re-read the tail so the
        // serve loop renders the `decided`/`granted`/`resumed` rows.
        self.tail(session_id)
    }

    fn ledger_read(&self, from_seq: u64) -> Result<Json, String> {
        // `_hh/ledger/read` — any live session's tail verbatim (the
        // boundary's `read` is session-scoped; the serve loop's
        // extension method has no session id, so the most recent
        // session's run is the read target — the record's own
        // watermark discipline is the session's).
        let s = self
            .last_resumed
            .as_ref()
            .and_then(|id| self.sessions.get(id))
            .or_else(|| self.sessions.values().last())
            .ok_or_else(|| "ledger/read: no session".to_string())?;
        let sid = s.embed_session_id.clone();
        self.op_shared(
            "read",
            Json::obj([
                ("session_id", Json::str(sid)),
                (
                    "cursor",
                    Json::obj([
                        ("kind", Json::str("seq")),
                        ("seq", Json::Int(from_seq as i64 + 1)),
                    ]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(512)),
            ]),
        )
    }

    fn account(&self) -> Result<Json, String> {
        let s = self
            .sessions
            .values()
            .last()
            .ok_or_else(|| "account: no session".to_string())?;
        let sid = s.embed_session_id.clone();
        self.op_shared("account", Json::obj([("session_id", Json::str(sid))]))
    }

    fn participant_describe(&self, participant: &Json) -> Result<Json, String> {
        self.op_shared("lab.hosting.describe", participant.clone())
    }
}

impl EmbedDriver<'_> {
    /// `session/new` shared tail — record the session and build the
    /// `SessionInfo` the serve loop answers with.
    fn register(&mut self, result: Json, params: &Json) -> Result<SessionInfo, String> {
        let session_id = result
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| {
                format!(
                    "open/fork result without session_id: {}",
                    result.to_canonical_string()
                )
            })?
            .to_string();
        let run_id = result
            .get("run_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let watermark = result
            .get("cursor")
            .and_then(|c| c.get("seq"))
            .and_then(Json::as_int)
            .unwrap_or(0) as u64;
        self.sessions.insert(
            session_id.clone(),
            Sess {
                embed_session_id: session_id.clone(),
                run_id,
                watermark,
                pending_permissions: VecDeque::new(),
            },
        );
        Ok(SessionInfo {
            session_id,
            config_options: params.get("config").cloned().unwrap_or(Json::Arr(vec![])),
            mcp_servers_declared: params
                .get("mcpServers")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// The stub's call log — `(op, params)` pairs in dispatch order.
    type CallLog = Arc<Mutex<Vec<(String, Json)>>>;

    /// A scripted `hh-embed/1` stub — records `(op, params)`, answers
    /// canned results in order. Proves the driver speaks the contract's
    /// op names and param shapes; nothing transport-specific.
    fn stub(results: Vec<Result<Json, Json>>) -> (EmbedDriver<'static>, CallLog) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let mut rest = results.into_iter();
        let d = EmbedDriver::over_fn(
            move |op: &str, params: Json| {
                seen.lock().unwrap().push((op.to_string(), params));
                match rest.next() {
                    Some(r) => r,
                    None => Err(Json::obj([("kind", Json::str("stub_exhausted"))])),
                }
            },
            Json::obj([("kind", Json::str("new"))]),
        );
        (d, calls)
    }

    fn envelope(seq: i64, class: &str, payload: Json) -> Json {
        Json::obj([
            ("seq", Json::Int(seq)),
            ("class", Json::str(class)),
            ("payload", payload),
        ])
    }

    #[test]
    fn new_session_opens_and_registers() {
        let (mut d, calls) = stub(vec![Ok(Json::obj([
            ("session_id", Json::str("sess-1")),
            ("run_id", Json::str("run-1")),
            ("cursor", Json::obj([("seq", Json::Int(4))])),
        ]))]);
        let info = d.new_session(&Json::obj([])).expect("new_session");
        assert_eq!(info.session_id, "sess-1");
        let (op, params) = &calls.lock().unwrap()[0];
        assert_eq!(op, "open_session");
        assert_eq!(
            params
                .get("spec")
                .and_then(|s| s.get("kind"))
                .and_then(Json::as_str),
            Some("new")
        );
        assert!(params
            .get("idempotency_key")
            .and_then(Json::as_str)
            .is_some());
    }

    #[test]
    fn prompt_submits_then_reads_the_durable_tail() {
        let (mut d, calls) = stub(vec![
            Ok(Json::obj([
                ("session_id", Json::str("sess-1")),
                ("run_id", Json::str("run-1")),
                ("cursor", Json::obj([("seq", Json::Int(3))])),
            ])),
            // submit
            Ok(Json::obj([("turn_id", Json::str("turn-1"))])),
            // read → dispatched + completed
            Ok(Json::obj([(
                "events",
                Json::Arr(vec![
                    envelope(4, "state.turn.dispatched", Json::obj([])),
                    envelope(
                        5,
                        "state.turn.completed",
                        Json::obj([("stop_reason", Json::str("end_turn"))]),
                    ),
                ]),
            )])),
        ]);
        d.new_session(&Json::obj([])).unwrap();
        let drive = d
            .prompt("sess-1", &Json::obj([("text", Json::str("hi"))]))
            .expect("prompt");
        assert_eq!(drive.stop_reason, "end_turn");
        assert_eq!(drive.events.len(), 2);
        assert_eq!(drive.events[0].1, "state.turn.dispatched");
        let ops: Vec<String> = calls
            .lock()
            .unwrap()
            .iter()
            .map(|(o, _)| o.clone())
            .collect();
        assert_eq!(ops, vec!["open_session", "submit", "read"]);
    }

    #[test]
    fn permission_round_trip_uses_kernel_permission_id() {
        let (mut d, _calls) = stub(vec![
            Ok(Json::obj([
                ("session_id", Json::str("s")),
                ("run_id", Json::str("r")),
            ])),
            Ok(Json::obj([("turn_id", Json::str("t"))])),
            // read → an ask row (durable)
            Ok(Json::obj([(
                "events",
                Json::Arr(vec![envelope(
                    7,
                    "security.permission.requested",
                    Json::obj([
                        ("decider", Json::str("human")),
                        ("permission_id", Json::str("perm-9")),
                    ]),
                )]),
            )])),
            // respond_permission
            Ok(Json::obj([("decided", Json::Bool(true))])),
            // post-resolution tail
            Ok(Json::obj([(
                "events",
                Json::Arr(vec![envelope(
                    8,
                    "security.permission.decided",
                    Json::obj([("permission_id", Json::str("perm-9"))]),
                )]),
            )])),
        ]);
        d.new_session(&Json::obj([])).unwrap();
        let drive = d
            .prompt("s", &Json::obj([("text", Json::str("go"))]))
            .unwrap();
        assert_eq!(drive.events[0].1, "security.permission.requested");
        let resolution = d
            .permission_decision(
                "s",
                "perm-s-1",
                &Json::obj([("outcome", Json::obj([("outcome", Json::str("denied"))]))]),
            )
            .unwrap();
        assert_eq!(resolution[0].1, "security.permission.decided");
        // A second decision without a pending ask refuses honestly.
        assert!(d
            .permission_decision("s", "perm-s-2", &Json::obj([]))
            .is_err());
    }
}
