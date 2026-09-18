//! The control driver (§5e.1 driver obligations — Core MUST-code): the cue
//! inbox, the `next_cue → observe → decide → bind → check →
//! control.decision → execute` loop, `checkpoint_ref` on every
//! `control.decision`, the F2 seam (`envelope.check → admitted |
//! refused{reason}` returned to β as `envelope_signal{refused}`), the
//! run/turn lifecycle rows, and **resume-by-leaf** (`restore` the leaf
//! checkpoint + `observe` the durable tail — never a re-run).
//!
//! The driver owns *orchestration*, never I/O: the model boundary is a
//! [`ModelPort`], effect dispatch an [`EffectGate`], context assembly an
//! [`AssemblerPort`], durability a [`LedgerSink`]. Every port is an
//! injected seam — the Stage-1 tests drive the whole loop offline with
//! scripted ports (determinism: the logical clock and id allocator are
//! `u64` counters the driver owns, never `Instant`/`random`).

use hh_ledger::event::{Event, EventEnvelope, Scope};
use hh_ledger::manifest::EventRef;
use hh_ontology::control::{CancelledBy, DecisionPoint, Owner, StopReason};
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

use crate::envelope::{CheckVerdict, Envelope, EnvelopeState};
use crate::guards::{GuardContext, GuardInput, GuardVerdict};
use crate::output::{ParsedCall, SurfaceSpec};
use crate::policy::EnvelopePolicy;
use crate::react::ReactMinimal;
use crate::state::ControlState;
use crate::stop::DrainReport;
use crate::strategy::{ControlContext, ControlError, ControlStrategy, FinalReport};
use crate::vocab::{
    Cue, Decider, DecisionKind, EffectOutcome, EnvelopeSignal, GuardPoint, ScopeKind,
    SettledOutcome,
};

// ─────────────────────────────────────────────────────────────────────────────
// Ports — the injected seams (the driver never does I/O).
// ─────────────────────────────────────────────────────────────────────────────

/// The model boundary — `call` issues one model call and returns its parsed
/// outcome (the gateway's record: the closed gateway `StopReason`, the
/// response ref, the parsed calls). At Stage 1 the port is scripted in
/// tests; in production it is the `hh-gateway` boundary.
pub trait ModelPort {
    /// Issue the call (`model_call_id` is the scope the driver allocated).
    fn call(&mut self, model_call_id: &str, request: &Json) -> ModelOutcome;
}

/// A parsed model outcome — the members G-INTERPRET and the cue read.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOutcome {
    /// The gateway stop reason (R-2.3.1's sum — never the control-plane's).
    pub stop_reason: hh_gateway::vocab::StopReason,
    /// The response blob ref (`model_completed.response_ref`).
    pub response_ref: String,
    /// Whether the response carried no text.
    pub text_empty: bool,
    /// The parsed tool calls (empty on text-only responses).
    pub calls: Vec<ParsedCall>,
    /// The `ModelErrorClass` spelling for `error`/`unknown` terminals.
    pub error_class: Option<String>,
    /// A provider `retry-after` hint (ms) — honoured per the `RetrySpec`.
    pub retry_after_ms: Option<u64>,
}

/// The effect boundary — `dispatch` runs one intent through the
/// `intended → prepared → committed → observed` lifecycle (the gate
/// appends its own rows through the sink the driver hands it — the
/// `authorize`/`commit`/`observe` internals are the effect subsystem's,
/// R-2.2.2). `read_only` and the effect class ride the intent record.
pub trait EffectGate {
    /// Dispatch one intent; return the settled outcome (+ the driver's
    /// `stop_rule = submit` detection — the marker is detected here,
    /// never by the environment, ADR-0104).
    fn dispatch(&mut self, effect_id: &str, attempt_no: u64, intent: &Json) -> GateOutcome;
    /// The structured `finish` record the model submitted through the
    /// completion capability (the claim surface — ADR-0112 D2:
    /// `finish{completion?, criteria_status[], artifacts_claimed[],
    /// open_items[]}`). Defaulted `None` — additive for Stage-1 gates that
    /// predate the claim surface; the driver still emits a completion claim
    /// for every `stop{completed}` (AC-R-2.7.2a-1).
    fn finish_record(&self) -> Option<Json> {
        None
    }
}

/// The gate's terminal report.
#[derive(Debug, Clone, PartialEq)]
pub struct GateOutcome {
    /// The settled outcome.
    pub outcome: SettledOutcome,
    /// The detected submission ref (`stop_rule = submit` marker).
    pub submission_ref: Option<String>,
    /// The `ErrorClass` spelling for retryable failures.
    pub error_class: Option<String>,
}

/// The context assembler — `assemble` turns a `propose.context_request`
/// into the request record the `ModelPort` consumes plus the canonical
/// `context.assembled` payload the `hh-context` builder produced (the
/// driver owns the `append` — DF-S1.19-1's Stage-1 half: the emitter call
/// site is the turn loop's). The strategy never sees the bytes — I2.
pub trait AssemblerPort {
    /// Assemble the next request.
    fn assemble(&mut self, context_request: &Json) -> AssembledRequest;
}

/// What `assemble` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledRequest {
    /// The request record the `ModelPort` consumes.
    pub request: Json,
    /// The `context.assembled` payload (the builder's canonical record —
    /// `None` only for a port that does not assemble).
    pub assembled_payload: Option<Json>,
}

/// Durability — the sink the driver appends through (the `hh-ledger`
/// `Store::append` in production; a vec in tests). `prefix()` is the
/// durable prefix every pure fold reads — the driver never keeps a second
/// copy of ledger state (the ledger is the sole durable source of truth).
pub trait LedgerSink {
    /// Append the batch (atomic; `Fenced`/`RunFinished` are the sink's
    /// typed errors as strings for the driver's typed `DriverError`).
    fn append(&mut self, events: Vec<Event>) -> Result<(), String>;
    /// The durable prefix.
    fn prefix(&self) -> &[EventEnvelope];
}

// ─────────────────────────────────────────────────────────────────────────────
// DriverError / RunResult
// ─────────────────────────────────────────────────────────────────────────────

/// `DriverError` — the driver's typed failures (never panics on malformed
/// input; a port error surfaces as `Port{class, detail}`).
#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    /// `strategy.open` refused (`IncompatibleBoundary`, `MissingProcedure`,
    /// `UnsupportedPlanNode`).
    Open(ControlError),
    /// The envelope refused to arm.
    Arm(String),
    /// The sink refused an append (`fenced`, `run_finished`, …).
    Append(String),
    /// A port returned a malformed record.
    Port {
        /// The port.
        port: &'static str,
        /// What was malformed.
        detail: String,
    },
    /// `restore` refused the checkpoint.
    Restore(crate::strategy::RestoreError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Open(e) => write!(f, "open{{{e}}}"),
            DriverError::Arm(e) => write!(f, "arm{{{e}}}"),
            DriverError::Append(e) => write!(f, "append{{{e}}}"),
            DriverError::Port { port, detail } => write!(f, "port{{{port}:{detail}}}"),
            DriverError::Restore(e) => write!(f, "restore{{{e}}}"),
        }
    }
}

impl std::error::Error for DriverError {}

/// `RunResult` — what `run()` returns (the terminal `FinalReport` plus the
/// `DrainReport` the stop protocol produced).
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// The strategy's `terminate` report.
    pub report: FinalReport,
    /// The drain record (`drain_report_ref` content-addresses its JSON).
    pub drain: DrainReport,
    /// The decision event ids emitted (the `control.decision` audit list).
    pub decision_events: Vec<String>,
}

/// `DriverConfig` — the injected facts the driver needs that are not the
/// strategy's or the policy's (the logical clock's start, the compiled
/// surface set for G-INTERPRET, the gauge caps, the budget-conservation
/// view source, attendance).
#[derive(Debug, Clone)]
pub struct DriverConfig {
    /// The compiled surface set (`ModelSurface` projection — G-INTERPRET).
    pub surfaces: Vec<SurfaceSpec>,
    /// The gauge caps (`context.occupancy`, `delegation_depth`, `fan_out`).
    pub gauge_caps: crate::guards::GaugeCaps,
    /// `remaining(dimension)` — the caller-maintained budget view for
    /// dimensions the driver does not itself count (outside-run
    /// reservations, `spend`, `approvals.requested`).
    pub remaining: std::collections::BTreeMap<String, i64>,
    /// The declared hard ceilings for the dimensions the driver folds
    /// itself — `model_calls` (`model.call.requested` count), `turns`
    /// (`control.decision` count), `retries` (`control.retry.scheduled`
    /// count), `time.working_ms`/`time.wall_ms` (the logical clock).
    /// `remaining` presented to the guards is `ceiling − consumed`;
    /// exhaustion is `remaining ≤ 0` (AC-R-2.6.2-2).
    pub budget_ceiling: std::collections::BTreeMap<String, i64>,
    /// The `retries` hard ceiling (INV-6).
    pub retries_ceiling: u64,
    /// `interactive` attendance (`escalate` on exhaustion — C1).
    pub interactive_attendance: bool,
    /// The profile's `max_output` bound (reservation sizing — ADR-0107 D6).
    pub profile_max_output_bound: u64,
}

impl Default for DriverConfig {
    fn default() -> Self {
        DriverConfig {
            surfaces: vec![],
            gauge_caps: crate::guards::GaugeCaps {
                occupancy_ppm: 900_000,
                delegation_depth: 4,
                fan_out: 8,
            },
            remaining: std::collections::BTreeMap::new(),
            budget_ceiling: std::collections::BTreeMap::new(),
            retries_ceiling: 64,
            interactive_attendance: false,
            profile_max_output_bound: 8_192,
        }
    }
}

/// The driver — owns the cue inbox, the logical clock, the id allocator
/// and the `decide → check → emit → execute` loop over injected ports.
pub struct Driver<S: ControlStrategy> {
    /// The armed envelope.
    pub envelope: Envelope,
    /// The armed envelope state.
    pub envelope_state: EnvelopeState,
    strategy: S,
    state: ControlState,
    /// The cue inbox (FIFO; `submit`/`next_cue` — the spec's queue).
    inbox: std::collections::VecDeque<Cue>,
    /// The logical clock (ms) — injectable, monotone.
    now_ms: u64,
    /// The id allocator (monotone — `mc-N`/`tc-N`/`ef-N`/`d-N`/`e-N`).
    next_id: u64,
    /// The recorded decision event ids.
    decision_events: Vec<String>,
    /// The last `control.decision` event ref (`causes ∋` for `intended`).
    last_decision_ref: Option<EventRef>,
    /// Whether a submission has been detected (`stop_rule` reading).
    submission: Option<String>,
    /// The model-call retry counter for the F2 seam.
    config: DriverConfig,
    /// A guard's `stop` verdict pending the drain (the envelope owns the
    /// reason — `run` picks it up after `execute`).
    stop_pending: Option<StopReason>,
    /// The derived deadline table — `scope_id → deadline_ms` from the
    /// `TimeoutPolicy` at scope open (INV-1's input; the driver derives,
    /// never guesses).
    deadlines: std::collections::BTreeMap<String, u64>,
    /// The run id the claim ledger records (`verification.claim.recorded`'s
    /// `run_id` — the `AgentProcess` ref of this run).
    run_id: String,
    /// The most recent `model_call_id` scope (the claim's `model_call_id` —
    /// the call whose output the completion claim rides).
    last_model_call_id: Option<String>,
    /// The subject model ref (the claim provenance's `Origin::Model.model_ref`
    /// — read from the sealed profile projection, `model_ref` key).
    model_ref: String,
}

impl<S: ControlStrategy> Driver<S> {
    /// `open` — arm the envelope over the durable prefix, `strategy.open`
    /// the `ControlContext`, seed the inbox with `run_opened`.
    pub fn open(
        mut strategy: S,
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<S>, DriverError> {
        let (envelope, envelope_state) =
            Envelope::arm(policy, sink.prefix()).map_err(|e| DriverError::Arm(e.to_string()))?;
        let state = strategy.open(ctx).map_err(DriverError::Open)?;
        let mut inbox = std::collections::VecDeque::new();
        inbox.push_back(Cue::RunOpened {
            goal_ref: ctx.process_ref.clone(),
            inputs: Json::Null,
        });
        sink.append(vec![crate::events::kernel_event(
            "e-0".into(),
            "lifecycle.turn.started",
            "1970-01-01T00:00:00.000Z".into(),
            Scope {
                turn_id: Some("turn-1".into()),
                ..scope_empty()
            },
            hh_ledger::ids::ROOT_EVENT.to_string(),
            vec![],
            Json::obj([
                ("turn_id", Json::str("turn-1")),
                ("process_ref", Json::str(&ctx.process_ref)),
            ]),
        )])
        .map_err(DriverError::Append)?;
        Ok(Driver {
            envelope,
            envelope_state,
            strategy,
            state,
            inbox,
            now_ms: 0,
            next_id: 0,
            decision_events: vec![],
            last_decision_ref: None,
            submission: None,
            config,
            stop_pending: None,
            deadlines: std::collections::BTreeMap::new(),
            run_id: ctx.process_ref.clone(),
            last_model_call_id: None,
            model_ref: ctx
                .profile
                .get("model_ref")
                .and_then(Json::as_str)
                .unwrap_or("model/subject")
                .to_string(),
        })
    }

    /// `submit(cue)` — enqueue a cue (the wakeup/ingress path; delivery is
    /// ledger-ordered — I7: cues are the strategy's only input).
    pub fn submit(&mut self, cue: Cue) {
        self.inbox.push_back(cue);
    }

    /// The folded `GuardContext` at `now` — `remaining` is the caller's
    /// pass-through view merged with `ceiling − consumed` for the
    /// driver-counted dimensions (the consumption fold is over the durable
    /// prefix — a pure read, never a widening).
    fn guard_ctx(&self, events: &[EventEnvelope]) -> GuardContext {
        let mut remaining = self.config.remaining.clone();
        if !self.config.budget_ceiling.is_empty() {
            let view = crate::views::fold_envelope_view(events);
            let model_calls = events
                .iter()
                .filter(|e| e.class == "model.call.requested")
                .count() as i64;
            for (dim, cap) in &self.config.budget_ceiling {
                let used = match dim.as_str() {
                    "model_calls" => model_calls,
                    "retries" => view.retries_scheduled as i64,
                    "turns" => view.decisions as i64,
                    "time.working_ms" | "time.wall_ms" => self.now_ms as i64,
                    _ => 0,
                };
                remaining.insert(dim.clone(), cap - used);
            }
        }
        GuardContext {
            remaining,
            retries_ceiling: self.config.retries_ceiling,
            now_ms: self.now_ms,
            deadlines: self.deadlines.clone(),
            gauges: Default::default(),
            effect_classes: Default::default(),
            cancel_requested: None,
            interactive_attendance: self.config.interactive_attendance,
        }
    }

    fn alloc(&mut self, tag: &str) -> String {
        self.next_id += 1;
        format!("{tag}-{}", self.next_id)
    }

    fn tick(&mut self, ms: u64) {
        self.now_ms = self.now_ms.saturating_add(ms.max(1));
    }

    /// `amend_budget_ceiling(dimension, new_cap)` — the runtime half of a
    /// ledgered `amend{budget}`: lift (or re-pin) the hard ceiling the
    /// guard fold reads. The boundary mints `control.budget.amended`
    /// first — this call only re-arms the in-memory ceiling so the next
    /// `guard_ctx` reports `new_cap − consumed` (the parked escalation's
    /// wake cue then re-drives `propose`; ADR-0168 D6). A `new_cap` still
    /// below consumed keeps `remaining ≤ 0` — the next guard crossing
    /// escalates again (attended) or stops (unattended).
    pub fn amend_budget_ceiling(&mut self, dimension: &str, new_cap: i64) {
        self.config
            .budget_ceiling
            .insert(dimension.to_string(), new_cap);
    }

    /// `checkpoint_ref` — the content address of the state's canonical
    /// checkpoint (`H("control.checkpoint" ∥ bytes)` — every
    /// `control.decision` carries it; resume-by-leaf restores it).
    pub fn checkpoint_ref(&self) -> String {
        let bytes = self.strategy.checkpoint(&self.state);
        format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(
                [b"control.checkpoint".as_slice(), bytes.as_slice()]
                    .concat()
                    .as_slice()
            )
        )
    }

    /// The current `ControlState` (read-only — the caller never mutates).
    pub fn state(&self) -> &ControlState {
        &self.state
    }

    /// The checkpoint bytes (for `resume`/`suspend`).
    pub fn checkpoint(&self) -> Vec<u8> {
        self.strategy.checkpoint(&self.state)
    }

    /// **Resume-by-leaf** — `restore` the leaf checkpoint against `ctx`,
    /// then `observe` the durable tail past `state.last_cue_seq` (the fold
    /// is idempotent — the tail replays only what the leaf missed).
    /// `resumed` is the post-restore cue (ADR-0130).
    pub fn resume(
        &mut self,
        ctx: &ControlContext,
        checkpoint: &[u8],
        sink: &mut dyn LedgerSink,
    ) -> Result<(), DriverError> {
        self.state = self
            .strategy
            .restore(checkpoint, ctx)
            .map_err(DriverError::Restore)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        // G-RESUME — re-arm the envelope over the durable prefix and
        // resume a pre-crash drain if the barrier is engaged.
        let (env, st) = Envelope::arm(self.envelope.policy.clone(), sink.prefix())
            .map_err(|e| DriverError::Arm(e.to_string()))?;
        self.envelope = env;
        self.envelope_state = st;
        // G-RESUME — the resume guard restates the barrier state: a
        // pre-crash drain (engaged barrier, no `lifecycle.run.finished`)
        // resumes as a `stop` with the recorded reason.
        let verdict = self.envelope.guard(
            sink.prefix(),
            GuardPoint::Resume,
            &GuardInput::Resume,
            &self.guard_ctx(sink.prefix()),
        );
        if let GuardVerdict::Stop { reason, events } = verdict {
            for ge in events {
                self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
            }
            self.stop_pending = Some(reason);
        }
        self.inbox.push_back(Cue::Resumed {
            last_durable: sink.prefix().last().map(|e| e.seq).unwrap_or(0),
            recovery_decision: Json::obj([("restored", Json::Bool(true))]),
        });
        Ok(())
    }

    /// `run` — drive the inbox until the run finishes (`stop` admitted +
    /// drain assessed) or the inbox empties (a `wait`/`escalate` parks the
    /// loop — the caller feeds the next cue).
    pub fn run(
        &mut self,
        model: &mut dyn ModelPort,
        gate: &mut dyn EffectGate,
        assembler: &mut dyn AssemblerPort,
        sink: &mut dyn LedgerSink,
    ) -> Result<RunResult, DriverError> {
        loop {
            let cue = match self.inbox.pop_front() {
                Some(c) => c,
                None => {
                    // The inbox is empty — park (the caller submits the
                    // next cue; `run` returns a suspended result rather
                    // than spinning — offline determinism: no timers fire
                    // inside the loop).
                    return Err(DriverError::Port {
                        port: "inbox",
                        detail: "exhausted: run parked awaiting a cue".into(),
                    });
                }
            };
            self.tick(1);
            let decision = self.strategy.decide(&mut self.state, &cue);
            // The F2 seam — `envelope.check(d') → admitted | refused`.
            let verdict = self.envelope.check(
                sink.prefix(),
                &decision,
                &self.guard_ctx(sink.prefix()),
                self.submission.is_some(),
            );
            match verdict {
                CheckVerdict::Admitted { .. } => {
                    let ev_id = self.alloc("d");
                    let payload = crate::events::decision_payload(
                        &ev_id,
                        &decision,
                        &self.state.cursor,
                        &self.checkpoint_ref(),
                        Decider::Strategy,
                        &[],
                        &Json::str("admitted"),
                        if let DecisionKind::Stop {
                            proposed_reason, ..
                        } = &decision.kind
                        {
                            Some(proposed_reason)
                        } else {
                            None
                        },
                    );
                    self.append_decision(sink, &ev_id, payload)?;
                    if let DecisionKind::Stop {
                        proposed_reason, ..
                    } = &decision.kind
                    {
                        return self.finish(sink, gate, proposed_reason.clone());
                    }
                    self.execute(sink, model, gate, assembler, decision)?;
                    if let Some(r) = self.stop_pending.take() {
                        self.append_envelope_stop(sink, &r)?;
                        return self.finish(sink, gate, r);
                    }
                }
                CheckVerdict::Refused { reason, events } => {
                    // The guard's evidence rows land first (the spec order
                    // is `control.invariant.violated` → the stop decision
                    // → quarantine).
                    for ge in events {
                        self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
                    }
                    // The refusal returns to β as `envelope_signal{refused}`
                    // (the F2 seam's feedback cue — the strategy owns the
                    // next decision).
                    let is_kernel_stop = reason.starts_with("stop{");
                    if is_kernel_stop {
                        // Strip exactly the wrapper's `stop{` + `}` — the
                        // payload is the reason's canonical JSON whose own
                        // braces must survive (`trim_end_matches` would eat
                        // them).
                        let stop_kind = reason
                            .strip_prefix("stop{")
                            .and_then(|s| s.strip_suffix('}'))
                            .unwrap_or(&reason);
                        // A kernel stop rule fired during `check` — the
                        // envelope owns the stop (decider: envelope).
                        let sr = kernel_stop_reason(stop_kind, &self.envelope.policy);
                        self.append_envelope_stop(sink, &sr)?;
                        return self.finish(sink, gate, sr);
                    }
                    self.inbox
                        .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                            decision_ref: self
                                .last_decision_ref
                                .as_ref()
                                .map(|r| r.event_id.clone())
                                .unwrap_or_default(),
                            reason,
                        }));
                }
            }
        }
    }

    /// The envelope-owned `stop` decision row — `decider: envelope`
    /// (the envelope's own stops never flow through β; INV-3's barrier is
    /// the row itself).
    fn append_envelope_stop(
        &mut self,
        sink: &mut dyn LedgerSink,
        reason: &StopReason,
    ) -> Result<(), DriverError> {
        let ev_id = self.alloc("d");
        let stop_decision = crate::vocab::ControlDecision {
            stamp: crate::vocab::DecisionStamp {
                decision_point: DecisionPoint::Stop,
                owner: Owner::Code,
                rationale_ref: None,
            },
            kind: DecisionKind::Stop {
                proposed_reason: reason.clone(),
                submission_ref: self.submission.clone(),
            },
        };
        let payload = crate::events::decision_payload(
            &ev_id,
            &stop_decision,
            &self.state.cursor,
            &self.checkpoint_ref(),
            Decider::Envelope,
            &[],
            &Json::str("stopped"),
            Some(reason),
        );
        self.append_decision(sink, &ev_id, payload)
    }

    /// Append the `control.decision` row (audit-grade; `checkpoint_ref`
    /// rides every row).
    fn append_decision(
        &mut self,
        sink: &mut dyn LedgerSink,
        ev_id: &str,
        payload: Json,
    ) -> Result<(), DriverError> {
        let ev = crate::events::kernel_event(
            ev_id.into(),
            "control.decision",
            self.ts(),
            Scope {
                turn_id: Some("turn-1".into()),
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        self.decision_events.push(ev_id.to_string());
        self.last_decision_ref = Some(EventRef {
            run_id: "run".into(),
            event_id: ev_id.to_string(),
        });
        // Fold the appended row into the strategy's view (idempotent —
        // `observe` skips seq ≤ last_cue_seq; the new row's seq is the
        // tail's max).
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// `execute` — dispatch the admitted decision (the driver owns the
    /// effect lifecycle rows the strategy may not emit — I1:
    /// `action.effect.intended`'s `causes` name the `control.decision`).
    fn execute(
        &mut self,
        sink: &mut dyn LedgerSink,
        model: &mut dyn ModelPort,
        gate: &mut dyn EffectGate,
        assembler: &mut dyn AssemblerPort,
        decision: crate::vocab::ControlDecision,
    ) -> Result<(), DriverError> {
        match decision.kind {
            DecisionKind::Propose {
                context_request, ..
            } => {
                let mc = self.alloc("mc");
                let assembled = assembler.assemble(&context_request);
                // The `context.assembled` emitter call site (DF-S1.19-1's
                // Stage-1 half) — the builder produced the payload; the
                // driver owns the `append`, scoped to the call it feeds.
                if let Some(p) = assembled.assembled_payload {
                    self.append(sink, "context.assembled", p, Some(&mc))?;
                }
                self.model_round(sink, model, &assembled.request, Some((mc, 1)))?;
            }
            DecisionKind::Act { intents, .. } => {
                self.act_round(sink, gate, &intents)?;
            }
            DecisionKind::Retry {
                target,
                attempt: _,
                not_before,
            } => {
                if let crate::vocab::RetryTarget::ModelCall { model_call_id } = target {
                    let request = assembler.assemble(&Json::Null).request;
                    let next_attempt = crate::retry::attempt_no(sink.prefix(), &model_call_id) + 1;
                    let payload = crate::events::retry_scheduled_payload(
                        ScopeKind::ModelCall,
                        &model_call_id,
                        next_attempt,
                        "resubmit",
                        0,
                        not_before.unwrap_or(self.now_ms),
                        &self.envelope.policy.policy_id,
                        None,
                    );
                    self.append(sink, "control.retry.scheduled", payload, None)?;
                    self.model_round(sink, model, &request, Some((model_call_id, next_attempt)))?;
                }
            }
            DecisionKind::Wait { until } => {
                // Park — the inbox stays empty until the declared cue kind
                // arrives (the caller submits it; `run` returns when the
                // inbox drains).
                let _ = until;
            }
            DecisionKind::Escalate { .. } => {
                // The escalation row lands with §5g's machinery — at
                // Stage 1 the decision row (already appended) is the
                // audit; the loop parks on `human_input`.
            }
            DecisionKind::Retrieve { .. }
            | DecisionKind::Compact { .. }
            | DecisionKind::Verify { .. }
            | DecisionKind::Delegate { .. } => {
                // C1+ decision kinds — react/minimal never emits them; a
                // staged variant parks before this point.
            }
            DecisionKind::Stop { .. } => {
                // Handled in `run` before `execute`.
            }
        }
        Ok(())
    }

    /// One `propose`/`retry` round: `model.call.requested` → the port's
    /// call → G-INTERPRET → `action.tool.proposed` per validated call →
    /// `model.call.completed`/`failed` → the `model_completed` cue. A
    /// retry reuses the same `model_call_id` scope (INV-5 — the shared
    /// attempt identity); a fresh propose allocates a new one.
    fn model_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        model: &mut dyn ModelPort,
        request: &Json,
        scope: Option<(String, u64)>,
    ) -> Result<(), DriverError> {
        let (mc, attempt) = scope.unwrap_or_else(|| (self.alloc("mc"), 1));
        self.last_model_call_id = Some(mc.clone());
        // G-PRE-CALL — budget/deadline/gauge/reservation/ladder.
        let pre = self.envelope.guard(
            sink.prefix(),
            GuardPoint::PreCall,
            &GuardInput::PreCall {
                model_call_id: mc.clone(),
                reservation_size: crate::retry::reserve_model_call_size(
                    self.config.profile_max_output_bound,
                    self.config
                        .remaining
                        .values()
                        .next()
                        .copied()
                        .unwrap_or(i64::MAX) as u64,
                ),
                gauge_caps: self.config.gauge_caps,
            },
            &self.guard_ctx(sink.prefix()),
        );
        if let Some(cue) = self.apply_guard_verdict(sink, pre, &mc)? {
            self.inbox.push_back(cue);
            return Ok(());
        }
        self.derive_deadline(ScopeKind::ModelCall, &mc);
        self.append(
            sink,
            "model.call.requested",
            Json::obj([
                ("model_call_id", Json::str(&mc)),
                ("attempt_no", Json::Int(attempt as i64)),
                ("request_ref", Json::str(format!("req-{mc}-a{attempt}"))),
            ]),
            Some(&mc),
        )?;
        let outcome = model.call(&mc, request);
        // G-INTERPRET — output validation + loop detectors + empty ladder.
        let interp = self.envelope.guard(
            sink.prefix(),
            GuardPoint::Interpret,
            &GuardInput::Interpret {
                model_call_id: mc.clone(),
                stop_reason: outcome.stop_reason,
                text_empty: outcome.text_empty,
                calls: outcome.calls.clone(),
                surfaces: self.config.surfaces.clone(),
            },
            &self.guard_ctx(sink.prefix()),
        );
        let mut proposed_tool_calls: Vec<String> = vec![];
        // A `nudge` is advisory — the call still lands (the loop window
        // accumulates it; a `deny`/`stop`/format rejection suppresses it).
        let suppressed = match &interp {
            GuardVerdict::Stop { .. } => true,
            GuardVerdict::Respond { observation, .. } => !matches!(
                observation.get("kind").and_then(Json::as_str),
                Some("loop_nudge")
            ),
            GuardVerdict::Pass { .. } => false,
        };
        if !suppressed {
            // `action.tool.proposed` per validated call — the strategy
            // folds them into `pending_intents`.
            if let crate::output::ValidationVerdict::Pass { calls } = crate::output::validate(
                &self.envelope.policy.output_validation,
                &self.config.surfaces,
                outcome.stop_reason,
                outcome.text_empty,
                &outcome.calls,
            ) {
                for c in calls {
                    self.append(
                        sink,
                        "action.tool.proposed",
                        Json::obj([
                            ("tool_call_id", Json::str(&c.tool_call_id)),
                            ("surface_id", Json::str(&c.surface_id)),
                            ("loop_key", Json::str(&c.loop_key)),
                        ]),
                        Some(&mc),
                    )?;
                    proposed_tool_calls.push(c.tool_call_id);
                }
            }
        }
        // The completed/failed row then the cue.
        match outcome.stop_reason {
            hh_gateway::vocab::StopReason::Error | hh_gateway::vocab::StopReason::Unknown => {
                let class = outcome
                    .error_class
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                self.append(
                    sink,
                    "model.call.failed",
                    Json::obj([
                        ("model_call_id", Json::str(&mc)),
                        ("attempt_no", Json::Int(attempt as i64)),
                        ("error", Json::obj([("class", Json::str(&class))])),
                    ]),
                    Some(&mc),
                )?;
                // The F2 seam — `schedule_retry` decides `retry` vs `give_up`.
                let retries_used =
                    crate::views::fold_envelope_view(sink.prefix()).retries_scheduled;
                match self.envelope.schedule_retry(
                    sink.prefix(),
                    ScopeKind::ModelCall,
                    &mc,
                    &class,
                    self.now_ms,
                    outcome.retry_after_ms,
                    None,
                    retries_used,
                    self.config.retries_ceiling,
                ) {
                    crate::retry::RetryOutcome::Retry(s) => {
                        self.append(
                            sink,
                            "control.retry.scheduled",
                            crate::events::retry_scheduled_payload(
                                ScopeKind::ModelCall,
                                &mc,
                                s.attempt_no,
                                &class,
                                s.delay_ms,
                                s.not_before,
                                &self.envelope.policy.policy_id,
                                s.attempt_delta.as_ref(),
                            ),
                            Some(&mc),
                        )?;
                        self.inbox
                            .push_back(Cue::EnvelopeSignal(EnvelopeSignal::RetryableError {
                                class,
                                attempt: s.attempt_no - 1,
                            }));
                    }
                    crate::retry::RetryOutcome::GiveUp(_)
                    | crate::retry::RetryOutcome::BudgetExhausted { .. } => {
                        self.inbox
                            .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                                decision_ref: mc.clone(),
                                reason: "retry_budget_exhausted".into(),
                            }));
                    }
                }
            }
            _ => {
                self.append(
                    sink,
                    "model.call.completed",
                    Json::obj([
                        ("model_call_id", Json::str(&mc)),
                        ("attempt_no", Json::Int(attempt as i64)),
                        ("stop_reason", Json::str(outcome.stop_reason.as_str())),
                        ("response_ref", Json::str(&outcome.response_ref)),
                    ]),
                    Some(&mc),
                )?;
                self.inbox.push_back(Cue::ModelCompleted {
                    model_call_id: mc,
                    response_ref: outcome.response_ref.clone(),
                    stop_reason: outcome.stop_reason,
                });
            }
        }
        // A guard verdict that was not pass (a nudge/deny/stop from
        // G-INTERPRET) rides the inbox as its cue form.
        if let Some(cue) = self.apply_guard_verdict(sink, interp, "")? {
            self.inbox.push_back(cue);
        }
        let _ = proposed_tool_calls;
        Ok(())
    }

    /// One `act` round (sequential): `action.effect.intended` (causes ∋
    /// the `control.decision` — I1) → G-PRE-DISPATCH → the gate's
    /// dispatch → the settled terminal → the `effects_settled` cue.
    fn act_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
        intents: &[Json],
    ) -> Result<(), DriverError> {
        let mut settled: Vec<EffectOutcome> = vec![];
        for intent in intents {
            let ef = self.alloc("ef");
            self.derive_deadline(ScopeKind::ToolAttempt, &ef);
            // `action.effect.intended` — `causes ∋ control.decision` (I1).
            let intended = crate::events::kernel_event(
                self.alloc("e"),
                "action.effect.intended",
                self.ts(),
                Scope {
                    turn_id: Some("turn-1".into()),
                    effect_id: Some(ef.clone()),
                    ..scope_empty()
                },
                self.parent_id(sink),
                self.last_decision_ref.clone().into_iter().collect(),
                Json::obj([
                    ("effect_id", Json::str(&ef)),
                    ("intent", intent.clone()),
                    (
                        "effect_class",
                        intent
                            .get("effect_class")
                            .cloned()
                            .unwrap_or_else(|| Json::str("reversible")),
                    ),
                ]),
            );
            sink.append(vec![intended]).map_err(DriverError::Append)?;
            // G-PRE-DISPATCH — retry eligibility, deadline, INV-3.
            let pre = self.envelope.guard(
                sink.prefix(),
                GuardPoint::PreDispatch,
                &GuardInput::PreDispatch {
                    effect_id: ef.clone(),
                    attempt_no: 1,
                    effect_class: intent
                        .get("effect_class")
                        .and_then(Json::as_str)
                        .unwrap_or("reversible")
                        .to_string(),
                    read_only: intent
                        .get("read_only")
                        .map(|b| matches!(b, Json::Bool(true)))
                        .unwrap_or(false),
                    last_terminal_unknown: false,
                    probed_or_idempotent: false,
                },
                &self.guard_ctx(sink.prefix()),
            );
            if matches!(pre, GuardVerdict::Pass { .. }) {
                let out = gate.dispatch(&ef, 1, intent);
                let terminal_class = match &out.outcome {
                    SettledOutcome::Observed { .. } => "action.effect.observed",
                    SettledOutcome::Refused => "action.effect.refused",
                    SettledOutcome::Unknown { .. } => "action.effect.unknown",
                    SettledOutcome::Abandoned => "action.effect.abandoned",
                };
                self.append(
                    sink,
                    terminal_class,
                    crate::events::settled_outcome_json(&out.outcome),
                    Some(&ef),
                )?;
                if let Some(s) = &out.submission_ref {
                    self.submission = Some(s.clone());
                }
                settled.push(EffectOutcome {
                    effect_id: ef,
                    outcome: out.outcome,
                });
            } else if let Some(cue) = self.apply_guard_verdict(sink, pre, &ef)? {
                self.inbox.push_back(cue);
                return Ok(());
            }
        }
        self.inbox.push_back(Cue::EffectsSettled {
            settled,
            all_terminal: true,
            submission_ref: self.submission.clone(),
        });
        Ok(())
    }

    /// Apply a non-`pass` guard verdict — append its rows and map it to the
    /// cue the inbox receives (`respond` → `guard_fired`-adjacent signal;
    /// `stop` → the finish path).
    fn apply_guard_verdict(
        &mut self,
        sink: &mut dyn LedgerSink,
        verdict: GuardVerdict,
        scope_id: &str,
    ) -> Result<Option<Cue>, DriverError> {
        match verdict {
            GuardVerdict::Pass { .. } => Ok(None),
            GuardVerdict::Respond {
                observation,
                events,
            } => {
                for ge in events {
                    self.append(
                        sink,
                        &ge.class,
                        ge.payload,
                        ge.scope_id
                            .as_deref()
                            .or(Some(scope_id).filter(|s| !s.is_empty())),
                    )?;
                }
                Ok(Some(Cue::GuardFired {
                    decision_point: DecisionPoint::Act,
                    guard_id: observation
                        .get("kind")
                        .and_then(Json::as_str)
                        .unwrap_or("guard")
                        .to_string(),
                }))
            }
            GuardVerdict::Stop { reason, events } => {
                for ge in events {
                    self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
                }
                // The envelope owns the stop — the `stop` decision row +
                // drain run on `run`'s next iteration (decider: envelope).
                self.stop_pending = Some(reason);
                Ok(None)
            }
        }
    }

    /// `finish` — the stop protocol: `control.decision{stop}` was already
    /// appended by the caller; the drain assessment decides the terminal
    /// reason, then `lifecycle.turn.finished` + `lifecycle.run.finished`.
    fn finish(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
        reason: StopReason,
    ) -> Result<RunResult, DriverError> {
        self.tick(1);
        // S1.21 — the claim-ledger obligation on `stop{completed}`: emit
        // `verification.completion.proposed` + `verification.claim.recorded`
        // before the drain assessment (AC-R-2.7.2a-1: ≥1 claim, kind ∈
        // {achieved, unachievable}, criteria_status aligned, delegate
        // provenance). A missing/malformed finish surface still records the
        // completion claim itself — nothing silently lost (CC3).
        if matches!(reason, StopReason::Completed) {
            self.emit_completion_claims(sink, gate)?;
        }
        let drain = crate::stop::assess_drain(
            sink.prefix(),
            &reason,
            self.now_ms.saturating_add(30_000),
            self.now_ms,
            &[],
            &[],
        );
        // Drain timeout ⇒ `unknown{cause: drain_timeout}` per open effect.
        for ef in &drain.unknown_recorded {
            self.append(
                sink,
                "action.effect.unknown",
                Json::obj([("cause", Json::str("drain_timeout"))]),
                Some(ef),
            )?;
        }
        let report = self.strategy.terminate(&self.state, &drain.final_reason);
        let drain_ref = format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(drain.to_json().to_canonical_string().as_bytes())
        );
        self.append(
            sink,
            "lifecycle.turn.finished",
            crate::events::turn_finished_payload(&drain.final_reason),
            None,
        )?;
        self.append(
            sink,
            "lifecycle.run.finished",
            crate::events::run_finished_payload(
                "finished",
                &drain.final_reason,
                &report.unresolved_effects,
                &drain_ref,
            ),
            None,
        )?;
        Ok(RunResult {
            report,
            drain,
            decision_events: self.decision_events.clone(),
        })
    }

    /// The claim-ledger obligation on `stop{completed}` (S1.21 — ADR-0112
    /// D1/D2/D6; AC-R-2.7.2a-1): `verification.completion.proposed` then
    /// `verification.claim.recorded` per extracted claim. The model's
    /// structured `finish` record comes from the gate (`stop_rule = submit`
    /// detection owns it — the driver never reads the response bytes);
    /// `extract` runs the structured channel at confidence 1.0. A missing
    /// surface, a `NoClaimSurface`/`MalformedClaim` refusal, or a gate that
    /// pre-dates the record all still emit the completion claim itself (the
    /// `stop{completed}` decision *is* the claim) — the extract error rides
    /// the claim's `asserted` so nothing is silently lost (CC3).
    fn emit_completion_claims(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
    ) -> Result<(), DriverError> {
        use hh_provenance::authority::PersistenceScope;
        use hh_provenance::origin::Origin;
        use hh_verification::claims::{self, Claim};
        use hh_verification::vocab::{ClaimKind, ExtractedBy, SubjectRef};

        let mc = self
            .last_model_call_id
            .clone()
            .unwrap_or_else(|| "mc-0".into());
        let prov = ProvenanceRecord::minted(
            Origin::model(self.model_ref.clone(), self.run_id.clone(), mc.clone()),
            PersistenceScope::Run,
            self.now_ms,
        );
        let at_seq = sink.prefix().last().map(|e| e.seq).unwrap_or(0);
        // `completion.proposed` — the model proposed completion at the stop
        // decision point (`by` names the model call).
        self.append_prov(
            sink,
            "verification.completion.proposed",
            hh_verification::events::completion_proposed(&mc, "stop"),
            None,
            prov.clone(),
        )?;
        // The claim surface — the gate's submitted `finish` record wrapped in
        // the response view `extract` reads (`{"finish": …}`).
        let response_view = match gate.finish_record() {
            Some(fields) => Json::obj([("finish", fields)]),
            None => Json::Null,
        };
        let claims = match claims::extract(
            &mc,
            &response_view,
            "finish",
            &self.run_id,
            at_seq,
            prov.clone(),
        ) {
            Ok(cs) => cs,
            Err(e) => {
                // The surface was absent or malformed — the completion claim
                // itself is still recorded (the admitted `stop{completed}`
                // is the claim; the refusal detail rides `asserted`).
                let c = Claim {
                    claim_id: format!("{mc}:claim:1"),
                    run_id: self.run_id.clone(),
                    model_call_id: mc.clone(),
                    at_seq,
                    kind: ClaimKind::Achieved,
                    subject: SubjectRef::Run,
                    predicate: "is_done".into(),
                    asserted: Json::obj([("extract_error", Json::str(e.to_string()))]),
                    evidence_refs: vec![],
                    extracted_by: ExtractedBy::Structured("finish".into()),
                    extraction_confidence_ppm: 1_000_000,
                    criteria_status: vec![],
                    provenance: prov.clone(),
                };
                // The delegate provenance is minted above — `validate`
                // cannot fail here, but honour the contract anyway.
                let _ = c.validate();
                vec![c]
            }
        };
        for claim in &claims {
            self.append_prov(
                sink,
                "verification.claim.recorded",
                hh_verification::events::claim_recorded(claim),
                Some(&mc),
                claim.provenance.clone(),
            )?;
        }
        Ok(())
    }

    /// Append one kernel row carrying explicit provenance (the verification
    /// family is provenance-mandatory — the claim rows carry the claim's
    /// own `delegate` origin, never a kernel fact — ADR-0112 D6).
    fn append_prov(
        &mut self,
        sink: &mut dyn LedgerSink,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
        provenance: ProvenanceRecord,
    ) -> Result<(), DriverError> {
        let mut ev = crate::events::kernel_event(
            self.alloc("e"),
            class,
            self.ts(),
            Scope {
                turn_id: Some("turn-1".into()),
                model_call_id: scope_id.map(String::from),
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        ev.provenance = Some(provenance);
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// Append one kernel row through the sink (scope id → `Scope.effect_id`
    /// — the driver maps it; the class registry validates).
    fn append(
        &mut self,
        sink: &mut dyn LedgerSink,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
    ) -> Result<(), DriverError> {
        let ev = crate::events::kernel_event(
            self.alloc("e"),
            class,
            self.ts(),
            Scope {
                // `lifecycle.run.*` rows are run-scope — they live beside
                // the turn chain, not inside it (`run.finished` lands after
                // `turn.finished` closed the turn scope, so a `turn_id`
                // stamp would trip the ledger's `ScopeNotOpen` gate).
                turn_id: if class.starts_with("lifecycle.run.") {
                    None
                } else {
                    Some("turn-1".into())
                },
                effect_id: scope_id
                    .filter(|_| class.starts_with("action.effect."))
                    .map(String::from),
                model_call_id: scope_id
                    .filter(|_| class.starts_with("model.") || class == "action.tool.proposed")
                    .map(String::from),
                tool_call_id: if class == "action.tool.proposed" {
                    payload
                        .get("tool_call_id")
                        .and_then(Json::as_str)
                        .map(String::from)
                } else {
                    None
                },
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        // Fold the tail into the strategy state (the `pending_intents`/
        // `open_effects` fold — idempotent on `last_cue_seq`).
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// Derive a scope deadline from the `TimeoutPolicy` (`started_at = now`
    ///   `default_ms`, never past `hard_max_ms` — the policy's own
    /// `deadline()` arithmetic).
    fn derive_deadline(&mut self, kind: ScopeKind, scope_id: &str) {
        let d =
            crate::retry::deadline(&self.envelope.policy.timeouts, kind, self.now_ms, &[], None);
        self.deadlines.insert(scope_id.to_string(), d.at);
    }

    fn ts(&self) -> String {
        format!("1970-01-01T00:00:{:02}.000Z", (self.now_ms / 1000) % 60)
    }

    fn parent_id(&self, sink: &dyn LedgerSink) -> String {
        sink.prefix()
            .last()
            .map(|e| e.event_id.clone())
            .unwrap_or_else(|| hh_ledger::ids::ROOT_EVENT.to_string())
    }
}

impl Driver<ReactMinimal> {
    /// `open` for the common `react/minimal` case.
    pub fn open_react(
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<ReactMinimal>, DriverError> {
        Driver::open(ReactMinimal::new(), ctx, policy, sink, config)
    }
}

fn scope_empty() -> Scope {
    Scope {
        turn_id: None,
        model_call_id: None,
        tool_call_id: None,
        effect_id: None,
        child_run_id: None,
        branch_id: None,
    }
}

/// Map a kernel stop-rule payload to a `StopReason` (the `check` path's
/// `stop{<canonical StopReason json>}` spellings — the fired members
/// (`budget_exhausted`'s dimension, `cancelled`'s `by`,
/// `format_failure`'s count) survive verbatim; the bare-kind spellings
/// remain as a fallback for any hand-made refusal).
fn kernel_stop_reason(kind: &str, policy: &EnvelopePolicy) -> StopReason {
    if let Ok(j) = hh_wire::json::parse(kind) {
        if let Some(r) = StopReason::from_json(&j) {
            return r;
        }
    }
    match kind {
        "completed" => StopReason::Completed,
        "format_failure" => StopReason::FormatFailure { count: 0 },
        "budget_exhausted" => StopReason::BudgetExhausted {
            budget_id: policy.budget_ref.clone(),
            dimension: hh_ontology::dimensions::DimensionId::Turns,
        },
        "cancelled" => StopReason::Cancelled {
            by: CancelledBy::Principal,
        },
        other => StopReason::InfrastructureFailure {
            error_class: hh_ontology::control::InfraError {
                family: hh_ontology::control::InfraErrorFamily::Kernel,
                class: other.to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::EnvelopePolicy;
    use crate::strategy::{ConcurrentInput, SteerMode, StrategyParams};

    /// An in-memory sink (the test double — `prefix()` is the fold input).
    struct MemSink {
        events: Vec<EventEnvelope>,
        seq: u64,
    }

    impl LedgerSink for MemSink {
        fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
            for e in events {
                self.seq += 1;
                self.events.push(EventEnvelope {
                    event_id: e.event_id,
                    run_id: "run".into(),
                    seq: self.seq,
                    ts: e.ts,
                    hlc: None,
                    plane: hh_ledger::event::EventPlane::Control,
                    class: e.class,
                    schema_version: 1,
                    producer: e.producer,
                    participant_class: hh_ledger::manifest::ParticipantClass::Native,
                    observability_level: Default::default(),
                    durability: hh_ledger::classes::Durability::Ledger,
                    scope: e.scope,
                    lease_generation: 1,
                    parent_event_id: e.parent_event_id,
                    causes: e.causes,
                    refs: vec![],
                    ir_refs: vec![],
                    surface_ids: Default::default(),
                    provenance: e.provenance,
                    payload: e.payload,
                    prev_hash: "h".into(),
                    hash: format!("h{}", self.seq),
                });
            }
            Ok(())
        }
        fn prefix(&self) -> &[EventEnvelope] {
            &self.events
        }
    }

    /// A scripted model port (the deterministic Stage-1 double).
    struct ScriptedModel {
        script: std::collections::VecDeque<ModelOutcome>,
    }

    impl ModelPort for ScriptedModel {
        fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
            self.script.pop_front().unwrap_or(ModelOutcome {
                stop_reason: hh_gateway::vocab::StopReason::EndTurn,
                response_ref: "r-empty".into(),
                text_empty: true,
                calls: vec![],
                error_class: None,
                retry_after_ms: None,
            })
        }
    }

    /// A scripted gate.
    struct ScriptedGate {
        out: GateOutcome,
        /// The structured `finish` record the gate "detected" (the claim
        /// surface — `None` exercises the synthesized-claim fallback).
        finish: Option<Json>,
    }

    impl EffectGate for ScriptedGate {
        fn dispatch(&mut self, _ef: &str, _a: u64, _i: &Json) -> GateOutcome {
            self.out.clone()
        }
        fn finish_record(&self) -> Option<Json> {
            self.finish.clone()
        }
    }

    struct NullAssembler;
    impl AssemblerPort for NullAssembler {
        fn assemble(&mut self, _req: &Json) -> AssembledRequest {
            AssembledRequest {
                request: Json::Null,
                assembled_payload: None,
            }
        }
    }

    fn ctx() -> ControlContext {
        ControlContext {
            process_ref: "proc-1".into(),
            plan: vec![],
            boundary: crate::react::react_preset(),
            profile: Json::Null,
            account_ref: "acct".into(),
            budget_ref: "b-1".into(),
            envelope_ref: "env-1".into(),
            parameters: StrategyParams::default(),
            capabilities_available: vec![],
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        }
    }

    fn outcome(sr: hh_gateway::vocab::StopReason, calls: Vec<ParsedCall>) -> ModelOutcome {
        ModelOutcome {
            stop_reason: sr,
            response_ref: "r-1".into(),
            text_empty: calls.is_empty(),
            calls,
            error_class: None,
            retry_after_ms: None,
        }
    }

    #[test]
    fn a_submission_run_drives_the_full_loop_to_completed() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [(
                        "path".into(),
                        crate::output::ParamSpec {
                            required: true,
                            kind: crate::output::ParamKind::Str,
                            enum_values: vec![],
                            domain: vec![],
                        },
                    )]
                    .into_iter()
                    .collect(),
                }],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        assert_eq!(r.report.submission_ref.as_deref(), Some("sub-1"));
        // The audit row set: decision rows carry checkpoint_ref; the run
        // finished row carries outcome_class + drain_report_ref.
        let classes: Vec<&str> = sink.events.iter().map(|e| e.class.as_str()).collect();
        assert!(classes.contains(&"control.decision"));
        assert!(classes.contains(&"action.tool.proposed"));
        assert!(classes.contains(&"action.effect.intended"));
        assert!(classes.contains(&"action.effect.observed"));
        assert!(classes.contains(&"lifecycle.run.finished"));
        let decision = sink
            .events
            .iter()
            .find(|e| e.class == "control.decision")
            .unwrap();
        assert!(decision.payload.get("checkpoint_ref").is_some());
        let finished = sink
            .events
            .iter()
            .find(|e| e.class == "lifecycle.run.finished")
            .unwrap();
        assert_eq!(
            finished.payload.get("outcome_class").and_then(Json::as_str),
            Some("scored")
        );
    }

    #[test]
    fn a_completed_stop_emits_completion_and_claim_rows() {
        // AC-R-2.7.2a-1: every `stop{completed}` emits
        // `verification.completion.proposed` + ≥1
        // `verification.claim.recorded` with `kind ∈ {achieved,
        // unachievable}`, the `criteria_status` table carried through, and
        // `authority = delegate` provenance on every row.
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [(
                        "path".into(),
                        crate::output::ParamSpec {
                            required: true,
                            kind: crate::output::ParamKind::Str,
                            enum_values: vec![],
                            domain: vec![],
                        },
                    )]
                    .into_iter()
                    .collect(),
                }],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
            },
            finish: Some(Json::obj([
                ("completion", Json::str("achieved")),
                (
                    "criteria_status",
                    Json::Arr(vec![Json::obj([
                        ("criterion_ref", Json::str("c:read-done")),
                        ("status", Json::str("met")),
                        ("evidence_refs", Json::Arr(vec![Json::str("sha256:ev-1")])),
                    ])]),
                ),
                (
                    "artifacts_claimed",
                    Json::Arr(vec![Json::str("sha256:art-1")]),
                ),
            ])),
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        // `completion.proposed` carries the proposing call at the `stop`
        // decision point.
        let proposed = sink
            .events
            .iter()
            .find(|e| e.class == "verification.completion.proposed")
            .expect("completion.proposed emitted");
        assert_eq!(
            proposed
                .payload
                .get("decision_point")
                .and_then(Json::as_str),
            Some("stop")
        );
        // ≥1 `claim.recorded` — the completion claim (kind = achieved,
        // criteria_status aligned) plus the per-artifact `effected` claim.
        let recorded: Vec<&EventEnvelope> = sink
            .events
            .iter()
            .filter(|e| e.class == "verification.claim.recorded")
            .collect();
        assert!(recorded.len() >= 2, "completion + artifact claims");
        let completion = &recorded[0];
        assert_eq!(
            completion.payload.get("kind").and_then(Json::as_str),
            Some("achieved")
        );
        assert_eq!(
            completion.payload.get("predicate").and_then(Json::as_str),
            Some("is_done")
        );
        // The criteria_status table is carried through verbatim.
        let cs = match completion.payload.get("criteria_status") {
            Some(Json::Arr(items)) => items,
            _ => panic!("criteria_status present"),
        };
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].get("status").and_then(Json::as_str), Some("met"));
        assert_eq!(
            cs[0].get("criterion_ref").and_then(Json::as_str),
            Some("c:read-done")
        );
        // Provenance on every claim row is `authority = delegate` — the
        // claim is the model's, never a kernel fact (AC-R-2.7.2a-1/CC2).
        for e in &recorded {
            let p = e.provenance.as_ref().expect("provenance mandatory");
            assert_eq!(
                p.authority,
                hh_provenance::authority::AuthorityClass::Delegate
            );
        }
        // The per-artifact claim is `effected` on `artifact{…}`.
        assert_eq!(
            recorded[1].payload.get("kind").and_then(Json::as_str),
            Some("effected")
        );
        // Claim rows precede `lifecycle.run.finished` — the gate reads them.
        let pos = |class: &str| sink.events.iter().position(|e| e.class == class).unwrap();
        assert!(pos("verification.claim.recorded") < pos("lifecycle.run.finished"));
    }

    #[test]
    fn an_error_model_call_retries_then_stops_infra() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(
                ModelOutcome {
                    stop_reason: hh_gateway::vocab::StopReason::Error,
                    response_ref: "r".into(),
                    text_empty: true,
                    calls: vec![],
                    error_class: Some("network".into()),
                    retry_after_ms: None,
                },
                10,
            )
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Abandoned,
                submission_ref: None,
                error_class: None,
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(
                r.report.stop_reason,
                StopReason::InfrastructureFailure { .. }
            ),
            "got {:?}",
            r.report.stop_reason
        );
        assert!(sink
            .events
            .iter()
            .any(|e| e.class == "control.retry.scheduled"));
    }

    #[test]
    fn an_interrupt_stops_cancelled() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        driver.submit(Cue::HumanInput(crate::vocab::HumanInput::Interrupt));
        // First the run_opened cue fires a propose… park by giving no model
        // outcome → the inbox drains to the interrupt? No — run_opened is
        // first; decide→propose→model call → EndTurn empty → format_error →
        // propose → … To keep the test deterministic, drive the interrupt
        // as the *second* cue after the first propose completes.
        let mut model = ScriptedModel {
            script: [ModelOutcome {
                stop_reason: hh_gateway::vocab::StopReason::Cancelled,
                response_ref: "r".into(),
                text_empty: true,
                calls: vec![],
                error_class: None,
                retry_after_ms: None,
            }]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Abandoned,
                submission_ref: None,
                error_class: None,
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(r.report.stop_reason, StopReason::Cancelled { .. }));
    }

    #[test]
    fn resume_restores_the_leaf_and_observes_the_tail() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let ckpt = driver.checkpoint();
        let policy2 = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver2 =
            Driver::open_react(&ctx(), policy2, &mut sink, DriverConfig::default()).unwrap();
        driver2.resume(&ctx(), &ckpt, &mut sink).unwrap();
        assert_eq!(driver2.state().variant_ref, "hh/react-minimal@1");
        // The `resumed` cue is queued (the post-restore cue).
        assert!(driver2
            .inbox
            .iter()
            .any(|c| matches!(c, Cue::Resumed { .. })));
    }

    // ── AC-R-2.6.1-5 fault-injection battery (I4 — every stub family
    // terminates with a typed stop inside budget) ────────────────────────

    /// A surface spec for `fs.read` (the scripted-call target).
    fn fs_read_surface() -> SurfaceSpec {
        SurfaceSpec {
            surface_id: "fs.read".into(),
            semantic_id: "sem/fs.read".into(),
            params: [(
                "path".into(),
                crate::output::ParamSpec {
                    required: true,
                    kind: crate::output::ParamKind::Str,
                    enum_values: vec![],
                    domain: vec![],
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn tool_call_outcome(path: &str) -> ModelOutcome {
        outcome(
            hh_gateway::vocab::StopReason::ToolUse,
            vec![ParsedCall {
                tool_call_id: format!("tc-{path}"),
                surface: "fs.read".into(),
                args_raw: format!(r#"{{"path":"{path}"}}"#),
            }],
        )
    }

    fn observed_gate() -> ScriptedGate {
        ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: None,
                error_class: None,
            },
            finish: None,
        }
    }

    #[test]
    fn ac5a_always_tool_calls_stops_budget_exhausted() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 4);
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(tool_call_outcome("/a"), 20).collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(r.report.stop_reason, StopReason::BudgetExhausted { .. }),
            "got {:?}",
            r.report.stop_reason
        );
        // The stop decision row is decider:envelope (the envelope owns it).
        let stop_row = sink
            .events
            .iter()
            .find(|e| {
                e.class == "control.decision"
                    && e.payload.get("kind").and_then(Json::as_str) == Some("stop")
            })
            .expect("a stop decision row");
        assert_eq!(
            stop_row.payload.get("decider").and_then(Json::as_str),
            Some("envelope")
        );
    }

    #[test]
    fn ac5b_never_tool_calls_stops_format_failure() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        // ScriptedModel's default outcome is `end_turn` text-empty.
        let mut model = ScriptedModel {
            script: [].into_iter().collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(
            r.report.stop_reason,
            StopReason::FormatFailure { .. }
        ));
        // Every rejection row carries `failure_class` + `repaired`, never
        // the rejected bytes (AC-R-2.6.2-5).
        let rejected: Vec<_> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.output.rejected")
            .collect();
        assert!(!rejected.is_empty());
        for e in &rejected {
            assert!(e.payload.get("failure_class").is_some());
            assert_eq!(e.payload.get("repaired"), Some(&Json::Bool(false)));
        }
    }

    #[test]
    fn ac5c_identical_calls_stop_loop_detected() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 30);
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        // The same canonical args → the same `loop_key` → `exact_repeat`.
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(tool_call_outcome("/same"), 30).collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(r.report.stop_reason, StopReason::LoopDetected { .. }),
            "got {:?}",
            r.report.stop_reason
        );
        // The ladder: nudge → deny → stop (each `control.loop.detected`).
        let actions: Vec<&str> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.loop.detected")
            .filter_map(|e| e.payload.get("action").and_then(Json::as_str))
            .collect();
        assert_eq!(actions, ["nudge", "deny", "stop"]);
    }

    #[test]
    fn ac5d_length_is_format_failure() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(
                outcome(hh_gateway::vocab::StopReason::MaxOutput, vec![]),
                10,
            )
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(
            r.report.stop_reason,
            StopReason::FormatFailure { .. }
        ));
    }

    // ── AC-R-2.6.1-3 — every `action.effect.intended` names its
    // `control.decision` in `causes[]` (I1) ──────────────────────────────
    #[test]
    fn ac3_every_intent_names_its_decision() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        let decision_ids: Vec<&str> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.decision")
            .map(|e| e.event_id.as_str())
            .collect();
        for e in sink
            .events
            .iter()
            .filter(|e| e.class == "action.effect.intended")
        {
            assert!(
                e.causes
                    .iter()
                    .any(|c| decision_ids.contains(&c.event_id.as_str())),
                "intended {} has no control.decision in causes",
                e.event_id
            );
        }
    }

    // ── AC-R-2.6.1-4 — re-driving against the same scripted model_io
    // reproduces the decision sequence (deterministic replay) ────────────
    #[test]
    fn ac4_replay_reproduces_the_decision_sequence() {
        fn one_run() -> Vec<(String, String)> {
            let mut sink = MemSink {
                events: vec![],
                seq: 0,
            };
            let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
            let mut driver = Driver::open_react(
                &ctx(),
                policy,
                &mut sink,
                DriverConfig {
                    surfaces: vec![fs_read_surface()],
                    ..DriverConfig::default()
                },
            )
            .unwrap();
            let mut model = ScriptedModel {
                script: [
                    tool_call_outcome("/a"),
                    outcome(hh_gateway::vocab::StopReason::EndTurn, vec![]),
                    tool_call_outcome("/b"),
                ]
                .into_iter()
                .collect(),
            };
            let mut gate = ScriptedGate {
                out: GateOutcome {
                    outcome: SettledOutcome::Observed {
                        outcome: "applied".into(),
                    },
                    submission_ref: Some("sub-1".into()),
                    error_class: None,
                },
                finish: None,
            };
            let mut asm = NullAssembler;
            let _ = driver
                .run(&mut model, &mut gate, &mut asm, &mut sink)
                .unwrap();
            sink.events
                .iter()
                .filter(|e| e.class == "control.decision")
                .map(|e| {
                    (
                        e.payload
                            .get("kind")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        e.payload
                            .get("decision_point")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                })
                .collect()
        }
        assert_eq!(one_run(), one_run());
    }
}
