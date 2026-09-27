//! `VariantHost` — the kernel's side of `plugin_abi/1`: bind, invoke,
//! stream, cancel, unbind, close, guard, run_conformance, and the closed
//! callback mediation (§8.4 §2; ADR-0181 D3/D5/D6/D7).
//!
//! Every operation is a `component_call` scope: one
//! `lifecycle.component.invoked{…}` terminal row per invocation carrying
//! the digests, the kernel-measured `boundary_overhead_ms` (M19) and
//! `charged_to`. Screening violations append
//! `security.extension.violation{extension_id, kind,
//! refused_message_digest}`; guard firings append `control.guard.fired`.
//! The events go through [`SessionEvents`] — the kernel adapter writes
//! them to the run ledger; the suites record them.

use std::time::{Duration, Instant};

use hh_embed_schema::plugin_abi::{
    plugin_abi_schema_hash, AbiError, AbiPayload, BindParams, BindResult, CallbackCall,
    CallbackResult, ConformanceParams, GuardParams, GuardVerdict, HostCallback, InvokeParams,
    InvokeResult, StreamParams,
};
use hh_wire::json::Json;

use crate::channel::ChannelError;
use crate::lower::stamp_json;
use crate::ports::{CallbackCtx, HostPorts};
use crate::screen::ScreenViolation;
use crate::session::{BindingRecord, VariantSession};
use crate::HostError;

/// The host-side event sink (the `component_call`-scoped ledger writer's
/// seam — `VecEvents` for the suites, the kernel's ledger writer in the
/// adapter).
pub trait SessionEvents: Send {
    /// Append one event — `class`, the `component_call` scope id (when the
    /// event is invocation-scoped) and the payload.
    fn emit(&mut self, class: &str, component_call: Option<&str>, payload: Json);
}

/// The in-memory event log (test + suite lane).
#[derive(Default)]
pub struct VecEvents {
    /// `(class, component_call_id, payload)` in append order.
    pub rows: Vec<(String, Option<String>, Json)>,
}

impl SessionEvents for VecEvents {
    fn emit(&mut self, class: &str, component_call: Option<&str>, payload: Json) {
        self.rows.push((
            class.to_string(),
            component_call.map(str::to_string),
            payload,
        ));
    }
}

impl SessionEvents for Vec<(String, Option<String>, Json)> {
    fn emit(&mut self, class: &str, component_call: Option<&str>, payload: Json) {
        self.push((
            class.to_string(),
            component_call.map(str::to_string),
            payload,
        ));
    }
}

/// The invocation terminal the host reports.
#[derive(Debug, Clone, PartialEq)]
pub enum InvokeOutcome {
    /// `outputs` — stamped (origin/authority/taint, V1).
    Outputs(Vec<Json>),
    /// The plugin's typed failure.
    Failed(AbiError),
}

/// `VariantHost` — generic over the kernel callback surface and the event
/// sink. One host drives many sessions; a session's whole authority is its
/// sealed Permission.
pub struct VariantHost<P: HostPorts, E: SessionEvents> {
    /// The mediated ports.
    pub ports: P,
    /// The event sink.
    pub events: E,
    /// The sealed schema hash every outbound envelope asserts (V5/V6).
    schema_hash: String,
}

impl<P: HostPorts, E: SessionEvents> VariantHost<P, E> {
    /// A host over `ports` + `events`.
    pub fn new(ports: P, events: E) -> VariantHost<P, E> {
        VariantHost {
            ports,
            events,
            schema_hash: plugin_abi_schema_hash(),
        }
    }

    fn emit(&mut self, class: &str, cc: Option<&str>, payload: Json) {
        self.events.emit(class, cc, payload);
    }

    /// Record a screening violation (`security.extension.violation`) —
    /// `refused_message_digest` is the content address of the refused
    /// canonical payload.
    fn violation(&mut self, s: &VariantSession, v: &ScreenViolation, digest: &str) {
        self.emit(
            "security.extension.violation",
            None,
            Json::obj([
                ("extension_id", Json::str(s.plugin_id.clone())),
                ("kind", Json::str(v.kind())),
                ("detail", Json::str(v.to_string())),
                ("refused_message_digest", Json::str(digest.to_string())),
            ]),
        );
    }

    fn violation_abi(&mut self, s: &VariantSession, e: AbiError, digest: &str) {
        let kind = if e == AbiError::AuthorityCrossing {
            "authority_crossing"
        } else {
            "reach"
        };
        self.emit(
            "security.extension.violation",
            None,
            Json::obj([
                ("extension_id", Json::str(s.plugin_id.clone())),
                ("kind", Json::str(kind)),
                ("detail", Json::str(e.as_str())),
                ("refused_message_digest", Json::str(digest.to_string())),
            ]),
        );
    }

    // ── session verbs ────────────────────────────────────────────────────

    /// `bind` — atomic per slot; emits `lifecycle.component.bound`
    /// (bind_result on the row — ok or the typed failure).
    pub fn bind(&mut self, s: &mut VariantSession, p: BindParams) -> Result<String, HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        s.chan
            .send(&self.schema_hash, AbiPayload::Bind(p.clone()))
            .map_err(HostError::Channel)?;
        let result = self.pump(s, None, |payload| match payload {
            AbiPayload::BindResult(r) => Some(r.clone()),
            _ => None,
        })?;
        match result {
            BindResult::Bound { binding_id } => {
                s.bindings.insert(
                    binding_id.clone(),
                    BindingRecord {
                        binding_id: binding_id.clone(),
                        slot: p.slot.clone(),
                        class_id: p.class_id.clone(),
                        contract_version: p.contract_version.clone(),
                        placement: p.placement.clone(),
                    },
                );
                self.emit(
                    "lifecycle.component.bound",
                    None,
                    Json::obj([
                        ("binding_id", Json::str(binding_id.clone())),
                        ("slot", Json::str(p.slot.clone())),
                        ("class_id", Json::str(p.class_id.clone())),
                        ("placement", Json::str(p.placement.clone())),
                        ("host_id", Json::str(s.session_id.clone())),
                        ("bind_result", Json::str("ok")),
                    ]),
                );
                Ok(binding_id)
            }
            BindResult::Failed(f) => {
                self.emit(
                    "lifecycle.component.bound",
                    None,
                    Json::obj([
                        ("slot", Json::str(p.slot.clone())),
                        ("class_id", Json::str(p.class_id.clone())),
                        ("placement", Json::str(p.placement.clone())),
                        ("host_id", Json::str(s.session_id.clone())),
                        ("bind_result", Json::str(f.as_str())),
                    ]),
                );
                Err(HostError::BindFailed(f))
            }
        }
    }

    /// `bind_all` — the AC-7 atomicity helper: every slot or none (a
    /// failing contribution unbinds the earlier ones; no
    /// `bind_result: ok` row survives for them — the bound rows are
    /// emitted as `superseded` on rollback).
    pub fn bind_all(
        &mut self,
        s: &mut VariantSession,
        params: Vec<BindParams>,
    ) -> Result<Vec<String>, HostError> {
        let mut ids = Vec::new();
        for p in params {
            match self.bind(s, p) {
                Ok(id) => ids.push(id),
                Err(e) => {
                    for id in &ids {
                        let _ = self.unbind(s, id);
                    }
                    return Err(e);
                }
            }
        }
        Ok(ids)
    }

    /// `invoke` — one class operation, one `component_call` scope.
    pub fn invoke(
        &mut self,
        s: &mut VariantSession,
        binding_id: &str,
        operation: &str,
        inputs: Vec<Json>,
        reservation: &str,
        deadline: Duration,
    ) -> Result<InvokeOutcome, HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        if !s.bindings.contains_key(binding_id) {
            return Err(HostError::Abi(AbiError::UnknownBinding));
        }
        let invocation_id = format!("inv-{}", s.chan.next_tx());
        let started = Instant::now();
        s.in_flight = Some(invocation_id.clone());
        let send = s.chan.send(
            &self.schema_hash,
            AbiPayload::Invoke(InvokeParams {
                binding_id: binding_id.to_string(),
                operation: operation.to_string(),
                inputs: inputs.clone(),
                reservation: reservation.to_string(),
                deadline: deadline.as_millis() as i64,
            }),
        );
        if let Err(e) = send {
            s.in_flight = None;
            return Err(HostError::Channel(e));
        }
        let outcome = self
            .pump(s, Some(deadline), |payload| match payload {
                AbiPayload::InvokeResult(r) => Some(r.clone()),
                _ => None,
            })
            .map(|r| match r {
                InvokeResult::Outputs(o) => {
                    InvokeOutcome::Outputs(o.iter().map(|d| stamp_json(&s.plugin_id, d)).collect())
                }
                InvokeResult::Failed(e) => InvokeOutcome::Failed(e),
            });
        s.in_flight = None;
        let overhead = started.elapsed().as_millis() as u64;
        let (terminal, cc) = match &outcome {
            Ok(InvokeOutcome::Outputs(o)) => (
                Json::obj([("outputs_digest", Json::str(digest_docs(o)))]),
                Some(invocation_id.as_str()),
            ),
            Ok(InvokeOutcome::Failed(e)) => (
                Json::obj([("failure", Json::str(e.as_str()))]),
                Some(invocation_id.as_str()),
            ),
            Err(_) => (
                Json::obj([("failure", Json::str("PluginCrashed"))]),
                Some(invocation_id.as_str()),
            ),
        };
        let mut row = Json::obj([
            ("binding_id", Json::str(binding_id.to_string())),
            ("operation", Json::str(operation.to_string())),
            ("inputs_digest", Json::str(digest_docs(&inputs))),
            ("boundary_overhead_ms", Json::Int(overhead as i64)),
            ("charged_to", Json::str(reservation.to_string())),
        ]);
        if let (Json::Obj(m), Json::Obj(t)) = (&mut row, &terminal) {
            m.extend(t.clone());
        }
        self.emit("lifecycle.component.invoked", cc, row);
        outcome
    }

    /// `stream` — open a resumable document stream; the returned
    /// invocation id is `cancel`'s operand.
    pub fn open_stream(
        &mut self,
        s: &mut VariantSession,
        binding_id: &str,
        operation: &str,
        inputs: Vec<Json>,
        reservation: &str,
        deadline: Duration,
    ) -> Result<String, HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        if !s.bindings.contains_key(binding_id) {
            return Err(HostError::Abi(AbiError::UnknownBinding));
        }
        let invocation_id = format!("inv-{}", s.chan.next_tx());
        s.chan
            .send(
                &self.schema_hash,
                AbiPayload::Stream(StreamParams {
                    invoke: InvokeParams {
                        binding_id: binding_id.to_string(),
                        operation: operation.to_string(),
                        inputs,
                        reservation: reservation.to_string(),
                        deadline: deadline.as_millis() as i64,
                    },
                    resume_from_seq: 0,
                }),
            )
            .map_err(HostError::Channel)?;
        s.open_streams.insert(invocation_id.clone());
        Ok(invocation_id)
    }

    /// `stream_next` — the next item of an open stream (`None` at the
    /// stream's `invoke_result` terminator).
    pub fn stream_next(
        &mut self,
        s: &mut VariantSession,
        invocation_id: &str,
        deadline: Duration,
    ) -> Result<Option<Json>, HostError> {
        if !s.open_streams.contains(invocation_id) {
            return Err(HostError::Abi(AbiError::UnknownBinding));
        }
        s.in_flight = Some(invocation_id.to_string());
        let plugin_id = s.plugin_id.clone();
        let r = self
            .pump(s, Some(deadline), |payload| match payload {
                AbiPayload::StreamItem(d) => Some(Ok::<Option<Json>, HostError>(Some(stamp_json(
                    &plugin_id, d,
                )))),
                AbiPayload::InvokeResult(InvokeResult::Outputs(_)) => Some(Ok(None)),
                AbiPayload::InvokeResult(InvokeResult::Failed(e)) => Some(Err(HostError::Abi(*e))),
                _ => None,
            })
            .and_then(|x| x);
        s.in_flight = None;
        if matches!(r, Ok(None)) {
            s.open_streams.remove(invocation_id);
        }
        r
    }

    /// `cancel(invocation_id)` — honoured within the deadline (the stream's
    /// `invoke_result` terminator arrives or the session detaches).
    pub fn cancel(
        &mut self,
        s: &mut VariantSession,
        invocation_id: &str,
        deadline: Duration,
    ) -> Result<(), HostError> {
        if !s.open_streams.contains(invocation_id) && s.in_flight.is_none() {
            return Err(HostError::Abi(AbiError::UnknownBinding));
        }
        s.chan
            .send(
                &self.schema_hash,
                AbiPayload::Cancel {
                    invocation_id: invocation_id.to_string(),
                },
            )
            .map_err(HostError::Channel)?;
        // Drain until the stream's terminal arrives.
        let r = self.pump(s, Some(deadline), |payload| match payload {
            AbiPayload::InvokeResult(_) => Some(()),
            _ => None,
        });
        s.open_streams.remove(invocation_id);
        r.map(|_| ())
    }

    /// `unbind(binding_id)` — releases the slot (fire-and-forget; the
    /// record leaves the live set).
    pub fn unbind(&mut self, s: &mut VariantSession, binding_id: &str) -> Result<(), HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        s.chan
            .send(
                &self.schema_hash,
                AbiPayload::Unbind {
                    binding_id: binding_id.to_string(),
                },
            )
            .map_err(HostError::Channel)?;
        s.bindings.remove(binding_id);
        Ok(())
    }

    /// `close(host_id)` — end the session (the channel closes; the helper
    /// process is cancelled).
    pub fn close(&mut self, s: &mut VariantSession) -> Result<(), HostError> {
        if s.detached {
            return Ok(());
        }
        let _ = s.chan.send(
            &self.schema_hash,
            AbiPayload::Close {
                host_id: s.session_id.clone(),
            },
        );
        s.detach();
        Ok(())
    }

    /// `guard` — a decision-point invocation; the verdict is the closed
    /// sum (no `allow` — decode refuses it `AuthorityCrossing`; the host
    /// then reports `no_decision` and ledger the violation).
    pub fn guard(
        &mut self,
        s: &mut VariantSession,
        p: GuardParams,
        required: bool,
        deadline: Duration,
    ) -> Result<GuardVerdict, HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        if !s.bindings.contains_key(&p.binding_id) {
            return Err(HostError::Abi(AbiError::UnknownBinding));
        }
        s.in_guard = true;
        s.chan
            .send(&self.schema_hash, AbiPayload::GuardInvoke(p.clone()))
            .map_err(HostError::Channel)?;
        let r = self.pump(s, Some(deadline), |payload| match payload {
            AbiPayload::GuardResult(v) => Some(v.clone()),
            _ => None,
        });
        s.in_guard = false;
        let verdict = match r {
            Ok(v) => v,
            Err(HostError::Channel(ChannelError::Schema(AbiError::AuthorityCrossing))) => {
                // A non-GuardVerdict output (`allow`, a rewritten proposal):
                // refused, violation ledgered, `no_decision` to the meet.
                self.emit(
                    "control.guard.fired",
                    None,
                    Json::obj([
                        ("decision_point", Json::str(p.decision_point.clone())),
                        ("binding_id", Json::str(p.binding_id.clone())),
                        ("verdict", Json::str("no_decision")),
                        ("required", Json::Bool(required)),
                        ("refused", Json::str("AuthorityCrossing")),
                    ]),
                );
                return Ok(GuardVerdict::NoDecision);
            }
            Err(HostError::Channel(ChannelError::Timeout)) => {
                self.emit(
                    "control.guard.fired",
                    None,
                    Json::obj([
                        ("decision_point", Json::str(p.decision_point.clone())),
                        ("binding_id", Json::str(p.binding_id.clone())),
                        ("verdict", Json::str("no_decision")),
                        ("required", Json::Bool(required)),
                        ("refused", Json::str("InvocationTimeout")),
                    ]),
                );
                return Ok(GuardVerdict::NoDecision);
            }
            Err(e) => return Err(e),
        };
        self.emit(
            "control.guard.fired",
            None,
            Json::obj([
                ("decision_point", Json::str(p.decision_point.clone())),
                ("binding_id", Json::str(p.binding_id.clone())),
                ("verdict", Json::str(verdict_label(&verdict))),
                ("required", Json::Bool(required)),
            ]),
        );
        Ok(verdict)
    }

    /// `run_conformance` — the `registry_ci` driver (the report is the
    /// plugin's `conformance_result` document).
    pub fn run_conformance(
        &mut self,
        s: &mut VariantSession,
        p: ConformanceParams,
        deadline: Duration,
    ) -> Result<Json, HostError> {
        if s.detached {
            return Err(HostError::Abi(AbiError::SessionDetached));
        }
        s.chan
            .send(&self.schema_hash, AbiPayload::RunConformance(p))
            .map_err(HostError::Channel)?;
        self.pump(s, Some(deadline), |payload| match payload {
            AbiPayload::ConformanceResult { report } => Some(report.clone()),
            _ => None,
        })
    }

    // ── the pump: callbacks + screening until the awaited reply ─────────

    /// Drive the channel until `want` matches, handling callbacks and
    /// stray items under screening. `deadline` bounds the whole wait —
    /// elapsed maps to `InvocationTimeout`.
    fn pump<T>(
        &mut self,
        s: &mut VariantSession,
        deadline: Option<Duration>,
        want: impl Fn(&AbiPayload) -> Option<T>,
    ) -> Result<T, HostError> {
        let end = deadline.map(|d| Instant::now() + d);
        loop {
            let remain = end.map(|e| e.saturating_duration_since(Instant::now()));
            if end.is_some() && remain == Some(Duration::ZERO) {
                // The call outlived its deadline — cancel is the plugin's
                // chance to unwind; the host's answer is InvocationTimeout.
                if let Some(inv) = s.in_flight.clone() {
                    let _ = s
                        .chan
                        .send(&self.schema_hash, AbiPayload::Cancel { invocation_id: inv });
                }
                return Err(HostError::Abi(AbiError::InvocationTimeout));
            }
            let env = match s.chan.recv(remain) {
                Ok(e) => e,
                Err(ChannelError::Timeout) => {
                    if let Some(inv) = s.in_flight.clone() {
                        let _ = s
                            .chan
                            .send(&self.schema_hash, AbiPayload::Cancel { invocation_id: inv });
                    }
                    return Err(HostError::Abi(AbiError::InvocationTimeout));
                }
                Err(ChannelError::Schema(e)) => {
                    // Refused message — violation ledgered; the operation
                    // fails typed (never a desync — rx already advanced).
                    self.violation_abi(s, e, "");
                    return Err(HostError::Channel(ChannelError::Schema(e)));
                }
                Err(ChannelError::SeqViolation { .. }) | Err(ChannelError::Oversized(_)) => {
                    s.detach();
                    return Err(HostError::Abi(AbiError::SessionDetached));
                }
                Err(e) => {
                    // EOF/IO — the plugin died mid-call.
                    s.detach();
                    return Err(HostError::Abi(match e {
                        ChannelError::Eof | ChannelError::Io(_) => AbiError::PluginCrashed,
                        _ => AbiError::SessionDetached,
                    }));
                }
            };
            if let Err(v) = s.screen(&env) {
                let digest = hh_identity::address(
                    env.payload.to_json().to_canonical_string().as_bytes(),
                    "application/json",
                )
                .id();
                self.violation(s, &v, &digest);
                if v.detaches() {
                    s.detach();
                    return Err(HostError::Abi(AbiError::SessionDetached));
                }
                continue;
            }
            match &env.payload {
                AbiPayload::Callback(call) => {
                    let result = self.dispatch_callback(s, call);
                    s.chan
                        .send(&self.schema_hash, AbiPayload::CallbackResult(result))
                        .map_err(HostError::Channel)?;
                }
                AbiPayload::Refused(e) => return Err(HostError::Abi(*e)),
                other => {
                    // `want` first: a `stream_item` is awaited while
                    // `stream_next` pumps that stream; the same frame is
                    // stray (reach violation) under any other want.
                    if let Some(t) = want(other) {
                        return Ok(t);
                    }
                    // A legal-but-unawaited reply (a bind_result during an
                    // invoke) is a `reach` violation — refused, session
                    // stays.
                    self.violation(
                        s,
                        &ScreenViolation::Direction {
                            verb: format!("unawaited {}", other.verb()),
                        },
                        "",
                    );
                }
            }
        }
    }

    /// The closed callback set — grant check, then the port. Inside a
    /// `guard` invocation `propose_effect` is refused outright (§8.4 §2).
    fn dispatch_callback(&mut self, s: &VariantSession, call: &CallbackCall) -> CallbackResult {
        let ctx = CallbackCtx {
            session_id: s.session_id.clone(),
            plugin_id: s.plugin_id.clone(),
            binding_id: s
                .in_flight
                .as_ref()
                .and_then(|_| s.bindings.keys().next().cloned()),
            in_guard: s.in_guard,
        };
        match call.callback {
            HostCallback::ProposeEffect => {
                if ctx.in_guard {
                    self.violation_abi(s, AbiError::AuthorityCrossing, "");
                    return CallbackResult::Refused(AbiError::AuthorityCrossing);
                }
                // The proposal must name a granted `{domain, scope}` and
                // claim no authority the plugin lacks (screening already
                // refused ≥ environment).
                let domain = call.args.get("domain").and_then(Json::as_str).unwrap_or("");
                let scope = call.args.get("scope").and_then(Json::as_str).unwrap_or("*");
                let key = format!("{domain}:{scope}");
                let wildcard = format!("{domain}:*");
                if !s.grants.contains(&key) && !s.grants.contains(&wildcard) {
                    self.violation_abi(s, AbiError::NotGranted, "");
                    return CallbackResult::Refused(AbiError::NotGranted);
                }
                match self.ports.propose_effect(&ctx, call.args.clone()) {
                    Ok(r) => CallbackResult::EffectRef(r),
                    Err(e) => CallbackResult::Refused(e),
                }
            }
            HostCallback::ReadView => {
                let kind = call
                    .args
                    .get("view_kind")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let until = call
                    .args
                    .get("until_seq")
                    .and_then(Json::as_int)
                    .unwrap_or(0);
                if !s.view_kinds.contains(kind) {
                    // Peer-plugin-private kinds, Π, monitor state — a view
                    // outside the grant set is an authority crossing (V1).
                    self.violation_abi(s, AbiError::AuthorityCrossing, "");
                    return CallbackResult::Refused(AbiError::AuthorityCrossing);
                }
                match self.ports.read_view(&ctx, kind, until) {
                    Ok(p) => CallbackResult::Projection(p),
                    Err(e) => CallbackResult::Refused(e),
                }
            }
            HostCallback::RequestBudget => {
                if s.grants.is_empty() {
                    self.violation_abi(s, AbiError::NotGranted, "");
                    return CallbackResult::Refused(AbiError::NotGranted);
                }
                match self.ports.request_budget(&ctx, &call.args) {
                    Ok(r) => CallbackResult::Reservation(r),
                    Err(e) => CallbackResult::Refused(e),
                }
            }
            HostCallback::EmitDiagnostic => {
                self.ports.emit_diagnostic(&ctx, call.args.clone());
                CallbackResult::Acknowledged
            }
        }
    }
}

/// The `verdict` label for `control.guard.fired`.
fn verdict_label(v: &GuardVerdict) -> &'static str {
    match v {
        GuardVerdict::Pass => "pass",
        GuardVerdict::Annotate(_) => "annotate",
        GuardVerdict::Narrow(hh_embed_schema::plugin_abi::Narrow::Deny { .. }) => "narrow:deny",
        GuardVerdict::Narrow(hh_embed_schema::plugin_abi::Narrow::Ask { .. }) => "narrow:ask",
        GuardVerdict::Narrow(hh_embed_schema::plugin_abi::Narrow::Attenuate { .. }) => {
            "narrow:attenuate"
        }
        GuardVerdict::ProposeReplacement(_) => "propose_replacement",
        GuardVerdict::NoDecision => "no_decision",
    }
}

/// `inputs_digest`/`outputs_digest` — the canonical-documents content
/// address (V2 — the ledger records what crossed).
pub fn digest_docs(docs: &[Json]) -> String {
    hh_identity::address(
        Json::Arr(docs.to_vec()).to_canonical_string().as_bytes(),
        "application/json",
    )
    .id()
}
