//! `react/minimal` (ADR-0104; the Stage-0 baseline and T-LCD-03 anchor) plus
//! the registered staged variants (`react/steerable`, `plan_execute`,
//! `workflow`, `program` — C1/Stage 2–4; their `capabilities`, β presets and
//! `requires` are bound now, their `decide` bodies land with their stage —
//! a staged variant `open`s and parks with `wait`, never fakes a step).
//!
//! The F1 decision table (ADR-0104 D3 as amended, CF-481 — read in the
//! gateway `StopReason` spellings):
//!
//! | cue | decision |
//! |---|---|
//! | `run_opened` | `propose{act}` |
//! | `model_completed{tool_use}` | `act{sequential}` |
//! | `model_completed{end_turn \| stop_sequence, no intents}` | nudge (streak++); streak ≥ `max_consecutive_format_errors` ⇒ `stop{format_failure}` else `propose` |
//! | `model_completed{max_output}` | `fail_calls` then `propose` |
//! | `model_completed{content_filter \| refusal}` | the format-error row (CF-481) |
//! | `model_completed{pause_turn}` | `retry{target: model_call}` (a control decision — never a gateway retry, ADR-0119 D1) |
//! | `model_completed{deferred}` | `wait{until: model_completed}` |
//! | `model_completed{cancelled}` | `stop{cancelled{by: principal}}` |
//! | `model_completed{error \| unknown}` | the `retryable_error`/`error(class)` row → `retry{model_call}` ≤ bound then `stop{infrastructure_failure}` |
//! | `effects_settled{all_terminal, submission}` | `stop{completed, submission_ref}` |
//! | `effects_settled{terminal, no submission}` | `propose` |
//! | `effects_settled{unknown}` | `propose` (the `unknown` is rendered explicit — ADR-0030) |
//! | `envelope_signal{refused: insufficient_budget{dim}}` | `stop{budget_exhausted{dimension}}` |
//! | `envelope_signal{retryable_error}` | `retry{model_call}` ≤ bound then `stop{infrastructure_failure}` |
//! | `envelope_signal{cancel_requested}` | `stop{cancelled{by}}` |
//! | `envelope_signal{soft_threshold}` | `propose` |
//! | `human_input{interrupt}` | `stop{cancelled{by: principal}}` |
//! | `resumed` | `observe` then `propose` |
//!
//! Totality (I3): while `open_effects ≠ ∅` every cue parks with
//! `wait{until: effects_settled}` (no `act` while effects are open —
//! `parallel_effects` is not declared); every remaining cue maps to
//! `propose` (the loop's next step).

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{
    CancelledBy, ControlBoundary, DecisionPoint, InfraError, InfraErrorFamily, Owner, StopReason,
};
use hh_wire::json::Json;

use crate::state::{ControlState, PlanCursor};
use crate::strategy::{
    stamp_for, ControlCapabilities, ControlContext, ControlError, ControlStrategy, FinalReport,
    RestoreError, StrategyParams, VariantRequires,
};
use crate::vocab::{
    ActMode, ControlDecision, Cue, DecisionKind, EnvelopeSignal, EscalateAsk, ExpectedOutput,
    HumanInput, OnPartial, RetryTarget, SettledOutcome, WaitUntil,
};

/// The `react/minimal` variant ref (`hh/` namespace — the kernel-shipped
/// anchor variant).
pub const REACT_MINIMAL_REF: &str = "hh/react-minimal@1";

/// The react β preset (ADR-0103 D6): `plan/act/retrieve/compact/verify/
/// delegate/authorize/retry/stop/escalate` = `model/model/model/code/code/
/// model/code/code/model→code(I6)/human`.
pub fn react_preset() -> ControlBoundary {
    let mut b = ControlBoundary::default();
    for (p, o) in [
        (DecisionPoint::Plan, Owner::Model),
        (DecisionPoint::Act, Owner::Model),
        (DecisionPoint::Retrieve, Owner::Model),
        (DecisionPoint::Compact, Owner::Code),
        (DecisionPoint::Verify, Owner::Code),
        (DecisionPoint::Delegate, Owner::Model),
        (DecisionPoint::Authorize, Owner::Code),
        (DecisionPoint::Retry, Owner::Code),
        (DecisionPoint::Stop, Owner::Model),
        (DecisionPoint::Escalate, Owner::Human),
    ] {
        b.assignments.insert(p, o);
    }
    b
}

/// `react/minimal` — the C0 variant (`deterministic_replay = true`; state =
/// the mandatory header + `{format_error_streak, pending_intents,
/// submissions, model_retries, last_error_class}` in `extension`). The
/// `StrategyParams` the `open` context supplies are held on `self` (the
/// MUST-data record is an input, never part of the state header).
#[derive(Debug)]
pub struct ReactMinimal {
    caps: ControlCapabilities,
    params: StrategyParams,
}

impl Default for ReactMinimal {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension keys (the closed per-variant record — `extension` stays
/// canonical-serialisable: ids, counters, class spellings only — never model
/// I/O bytes, ADR-0051 H-1).
mod ext {
    pub const FORMAT_ERROR_STREAK: &str = "format_error_streak";
    pub const PENDING_INTENTS: &str = "pending_intents";
    pub const SUBMISSIONS: &str = "submissions";
    pub const MODEL_RETRIES: &str = "model_retries";
    pub const LAST_ERROR_CLASS: &str = "last_error_class";
    pub const LAST_MODEL_CALL_ID: &str = "last_model_call_id";
    pub const CONSECUTIVE_ERRORS: &str = "consecutive_errors";
    /// `control.guard.fired{compaction_required}` — the recorded pressure
    /// (`required_tokens`/`cap`) the `context_exhausted` reason cites.
    pub const CX_REQUIRED_TOKENS: &str = "cx_required_tokens";
    /// The recorded `window_cap` (tokens).
    pub const CX_CAP: &str = "cx_cap";
    /// The seq of the latest `control.guard.fired{compaction_required}` row.
    pub const CX_GUARD_SEQ: &str = "cx_guard_seq";
    /// The seq of the `compaction_required` row before the latest.
    pub const CX_PREV_GUARD_SEQ: &str = "cx_prev_guard_seq";
    /// How many `compaction_required` signals this run has seen (I4 bound).
    pub const CX_GUARD_COUNT: &str = "cx_guard_count";
    /// The seq of the latest `context.compaction.completed` row.
    pub const LAST_COMPACTION_SEQ: &str = "last_compaction_seq";
    /// The latest compaction status (`applied | ineffective | failed`).
    pub const LAST_COMPACTION_STATUS: &str = "last_compaction_status";
    /// The last `control.loop.detected` detector (a `stop{loop_detected}`
    /// the strategy proposes cites the ledgered detection — never a guess).
    pub const LAST_LOOP_DETECTOR: &str = "last_loop_detector";
    /// The last `control.loop.detected` pattern (the `LoopPattern` record).
    pub const LAST_LOOP_PATTERN: &str = "last_loop_pattern";
    /// The guard-fired continuation count (`max_continue_nudges` bound).
    pub const CONTINUE_NUDGES: &str = "continue_nudges";
}

impl ReactMinimal {
    /// Construct the variant.
    pub fn new() -> Self {
        ReactMinimal {
            caps: ControlCapabilities {
                deterministic_replay: true,
                steering: false,
                follow_up: true,
                parallel_effects: false,
                delegation: false,
                model_emitted_plan: false,
                resumable_mid_effect: true,
                decision_points_owned: vec![
                    DecisionPoint::Plan,
                    DecisionPoint::Act,
                    DecisionPoint::Retrieve,
                    DecisionPoint::Delegate,
                ],
                boundary_preset: react_preset(),
                requires: VariantRequires {
                    goal: true,
                    procedure: false,
                },
            },
            params: StrategyParams::default(),
        }
    }

    fn ext_u64(state: &ControlState, key: &str) -> u64 {
        state.extension.get(key).and_then(Json::as_int).unwrap_or(0) as u64
    }

    fn ext_str<'a>(state: &'a ControlState, key: &str) -> Option<&'a str> {
        state.extension.get(key).and_then(Json::as_str)
    }

    fn set_ext(state: &mut ControlState, key: &'static str, v: Json) {
        let mut m = match &state.extension {
            Json::Obj(m) => m.clone(),
            _ => BTreeMap::new(),
        };
        m.insert(key.to_string(), v);
        state.extension = Json::Obj(m);
    }

    fn pending_intents(state: &ControlState) -> Vec<String> {
        match state.extension.get(ext::PENDING_INTENTS) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(String::from))
                .collect(),
            _ => vec![],
        }
    }

    /// `propose{act}` — the loop's next-step decision.
    fn propose(&self, state: &ControlState) -> ControlDecision {
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Act, None),
            kind: DecisionKind::Propose {
                decision_point: DecisionPoint::Act,
                context_request: Json::Null,
                expected_output: ExpectedOutput::Free,
            },
        }
    }

    fn stop(&self, state: &ControlState, reason: StopReason) -> ControlDecision {
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Stop, None),
            kind: DecisionKind::Stop {
                proposed_reason: reason,
                submission_ref: None,
            },
        }
    }
}

impl ControlStrategy for ReactMinimal {
    fn capabilities(&self) -> &ControlCapabilities {
        &self.caps
    }

    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
        // I5 — the boundary validates (envelope-reserved points are code).
        ctx.boundary
            .validate()
            .map_err(ControlError::IncompatibleBoundary)?;
        // `requires.procedure` — react needs only the goal.
        self.params = ctx.parameters.clone();
        Ok(ControlState {
            variant_ref: REACT_MINIMAL_REF.into(),
            cursor: PlanCursor {
                node_id: "loop-1".into(),
                iteration: 0,
                bound_ref: ctx.budget_ref.clone(),
            },
            decision_count: 0,
            open_effects: vec![],
            last_cue_seq: 0,
            boundary_view: ctx.boundary.assignments.clone(),
            extension: Json::obj([
                (ext::FORMAT_ERROR_STREAK, Json::Int(0)),
                (ext::PENDING_INTENTS, Json::Arr(vec![])),
                (ext::SUBMISSIONS, Json::Arr(vec![])),
                (ext::MODEL_RETRIES, Json::Int(0)),
                (ext::CONSECUTIVE_ERRORS, Json::Int(0)),
            ]),
        })
    }

    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        for ev in events {
            if ev.seq <= state.last_cue_seq {
                continue; // idempotent on last_cue_seq
            }
            state.last_cue_seq = ev.seq;
            match ev.class.as_str() {
                // A validated tool call the driver proposed at G-INTERPRET —
                // the strategy sequences it (`act{intents}` names the
                // `tool_call_id`s, never the arg bytes — I2).
                "action.tool.proposed" => {
                    if let Some(tc) = ev.scope.tool_call_id.clone() {
                        let mut p = Self::pending_intents(state);
                        // A tool_call_id is unique per call — a replayed
                        // fold or a re-proposed call never double-dispatches
                        // (INV-5's commit uniqueness at the intent level).
                        if !p.contains(&tc) {
                            p.push(tc);
                        }
                        Self::set_ext(
                            state,
                            ext::PENDING_INTENTS,
                            Json::Arr(p.iter().map(Json::str).collect()),
                        );
                    }
                }
                "action.effect.intended" => {
                    if let Some(e) = ev.scope.effect_id.clone() {
                        if !state.open_effects.contains(&e) {
                            state.open_effects.push(e);
                        }
                    }
                }
                "action.effect.observed"
                | "action.effect.refused"
                | "action.effect.unknown"
                | "action.effect.abandoned" => {
                    if let Some(e) = ev.scope.effect_id.clone() {
                        state.open_effects.retain(|x| x != &e);
                    }
                }
                "model.call.failed" => {
                    if let Some(m) = ev.scope.model_call_id.clone() {
                        Self::set_ext(state, ext::LAST_MODEL_CALL_ID, Json::str(m));
                    }
                    if let Some(c) = ev
                        .payload
                        .get("error")
                        .and_then(|e| e.get("class"))
                        .and_then(Json::as_str)
                    {
                        Self::set_ext(state, ext::LAST_ERROR_CLASS, Json::str(c));
                    }
                }
                // The `compaction_required` pressure + the ladder state —
                // folded so `decide` cites ledgered numbers, never guesses
                // (the `context_exhausted` reason and the
                // compact-vs-exhausted bound read these).
                "control.guard.fired" => {
                    if ev.payload.get("guard_id").and_then(Json::as_str)
                        == Some("compaction_required")
                    {
                        let prev = Self::ext_u64(state, ext::CX_GUARD_SEQ);
                        Self::set_ext(state, ext::CX_PREV_GUARD_SEQ, Json::Int(prev as i64));
                        Self::set_ext(state, ext::CX_GUARD_SEQ, Json::Int(ev.seq as i64));
                        let n = Self::ext_u64(state, ext::CX_GUARD_COUNT) + 1;
                        Self::set_ext(state, ext::CX_GUARD_COUNT, Json::Int(n as i64));
                        for (member, key) in [
                            ("required_tokens", ext::CX_REQUIRED_TOKENS),
                            ("cap", ext::CX_CAP),
                        ] {
                            if let Some(v) = ev.payload.get(member).and_then(Json::as_int) {
                                Self::set_ext(state, key, Json::Int(v));
                            }
                        }
                    }
                }
                "context.compaction.completed" => {
                    Self::set_ext(state, ext::LAST_COMPACTION_SEQ, Json::Int(ev.seq as i64));
                    if let Some(s) = ev.payload.get("status").and_then(Json::as_str) {
                        Self::set_ext(state, ext::LAST_COMPACTION_STATUS, Json::str(s));
                    }
                }
                "control.loop.detected" => {
                    if let Some(d) = ev.payload.get("detector").and_then(Json::as_str) {
                        Self::set_ext(state, ext::LAST_LOOP_DETECTOR, Json::str(d));
                    }
                    if let Some(pat) = ev.payload.get("pattern") {
                        Self::set_ext(state, ext::LAST_LOOP_PATTERN, pat.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        // I4 totality: while effects are open, every cue parks (no `act`,
        // no `propose` — the loop resumes on `effects_settled`).
        let parked = !state.open_effects.is_empty()
            && !matches!(cue, Cue::EffectsSettled { .. })
            && !matches!(
                cue,
                Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested { .. })
                    | Cue::HumanInput(HumanInput::Interrupt)
            );
        if parked {
            return ControlDecision {
                stamp: stamp_for(state, DecisionPoint::Act, None),
                kind: DecisionKind::Wait {
                    until: WaitUntil::CueKind {
                        cue_kind: "effects_settled".into(),
                    },
                },
            };
        }

        let decision = match cue {
            Cue::RunOpened { .. } | Cue::Resumed { .. } => self.propose(state),

            Cue::ModelCompleted {
                stop_reason,
                model_call_id,
                ..
            } => {
                use hh_gateway::vocab::StopReason as Gw;
                match stop_reason {
                    Gw::ToolUse => {
                        let intents = Self::pending_intents(state);
                        if intents.is_empty() {
                            // `tool_use` with no admissible calls is the
                            // format-error row (every call was rejected at
                            // G-INTERPRET).
                            self.format_error(state)
                        } else {
                            Self::set_ext(state, ext::PENDING_INTENTS, Json::Arr(vec![]));
                            ControlDecision {
                                stamp: stamp_for(state, DecisionPoint::Act, None),
                                kind: DecisionKind::Act {
                                    intents: intents
                                        .iter()
                                        .map(|tc| Json::obj([("tool_call_id", Json::str(tc))]))
                                        .collect(),
                                    mode: ActMode::Sequential,
                                    on_partial: OnPartial::FailBatch,
                                },
                            }
                        }
                    }
                    // `{end}` ≡ `end_turn | stop_sequence`; `refusal` takes
                    // the `content_filter` row (CF-481).
                    Gw::EndTurn | Gw::StopSequence | Gw::ContentFilter | Gw::Refusal => {
                        self.format_error(state)
                    }
                    // `{length}` ≡ `max_output` → `fail_calls` then `propose`.
                    Gw::MaxOutput => {
                        Self::set_ext(state, ext::PENDING_INTENTS, Json::Arr(vec![]));
                        self.propose(state)
                    }
                    // `pause_turn` → `retry{target: model_call}` — a control
                    // decision, never a gateway retry (ADR-0119 D1).
                    Gw::PauseTurn => ControlDecision {
                        stamp: stamp_for(state, DecisionPoint::Retry, None),
                        kind: DecisionKind::Retry {
                            target: RetryTarget::ModelCall {
                                model_call_id: model_call_id.clone(),
                            },
                            attempt: Self::ext_u64(state, ext::MODEL_RETRIES) + 1,
                            not_before: None,
                        },
                    },
                    // `deferred` → `wait{until: model_completed}`.
                    Gw::Deferred => ControlDecision {
                        stamp: stamp_for(state, DecisionPoint::Act, None),
                        kind: DecisionKind::Wait {
                            until: WaitUntil::CueKind {
                                cue_kind: "model_completed".into(),
                            },
                        },
                    },
                    // `{aborted}` ≡ `cancelled`.
                    Gw::Cancelled => self.stop(
                        state,
                        StopReason::Cancelled {
                            by: CancelledBy::Principal,
                        },
                    ),
                    // `error`/`unknown` → the retryable_error/error(class)
                    // rows.
                    Gw::Error | Gw::Unknown => self.retry_or_infra_stop(state, model_call_id),
                }
            }

            Cue::EffectsSettled {
                submission_ref,
                settled,
                ..
            } => {
                if let Some(sub) = submission_ref {
                    // Record the submission durably (the strategy's own
                    // ledger-free view — `terminate` reads it back).
                    let mut subs: Vec<String> = match state.extension.get(ext::SUBMISSIONS) {
                        Some(Json::Arr(items)) => items
                            .iter()
                            .filter_map(|i| i.as_str().map(String::from))
                            .collect(),
                        _ => vec![],
                    };
                    subs.push(sub.clone());
                    Self::set_ext(
                        state,
                        ext::SUBMISSIONS,
                        Json::Arr(subs.iter().map(Json::str).collect()),
                    );
                    let mut d = self.stop(state, StopReason::Completed);
                    if let DecisionKind::Stop {
                        submission_ref: sr, ..
                    } = &mut d.kind
                    {
                        *sr = Some(sub.clone());
                    }
                    d
                } else if settled
                    .iter()
                    .any(|s| matches!(s.outcome, SettledOutcome::Unknown { .. }))
                {
                    // `effects_settled{unknown} → propose` — the explicit
                    // `unknown` is rendered by the driver (ADR-0030).
                    self.propose(state)
                } else {
                    self.propose(state)
                }
            }

            Cue::EnvelopeSignal(sig) => match sig {
                EnvelopeSignal::Refused { reason, .. }
                    if reason.starts_with("insufficient_budget") =>
                {
                    let dim = reason
                        .strip_prefix("insufficient_budget{")
                        .and_then(|s| s.strip_suffix('}'))
                        .unwrap_or("unknown");
                    self.stop(
                        state,
                        StopReason::BudgetExhausted {
                            budget_id: state.cursor.bound_ref.clone(),
                            dimension: hh_ontology::dimensions::DimensionId::parse(dim)
                                .unwrap_or(hh_ontology::dimensions::DimensionId::Turns),
                        },
                    )
                }
                EnvelopeSignal::Refused {
                    decision_ref,
                    reason,
                } => {
                    // `escalate_on_exhaustion` arrives here when the
                    // decide-point guard fired it (the `envelope.check`
                    // seam reports Respond verdicts as `Refused{kind}` —
                    // unlike the post-effect path's `GuardFired` cue).
                    // Same landing: `escalate{handoff}` parks for the
                    // principal's ledgered `amend(budget)` or `stop`
                    // (ADR-0168 D6).
                    if reason == "escalate_on_exhaustion" {
                        ControlDecision {
                            stamp: stamp_for(state, DecisionPoint::Escalate, None),
                            kind: DecisionKind::Escalate {
                                ask: EscalateAsk::Handoff,
                            },
                        }
                    } else if reason.starts_with("retry_budget_exhausted")
                        || reason.starts_with("give_up")
                    {
                        self.infra_stop(state)
                    } else {
                        self.stop(
                            state,
                            StopReason::Refused {
                                blocking_effect_id: decision_ref.clone(),
                            },
                        )
                    }
                }
                EnvelopeSignal::RetryableError { attempt, .. } => {
                    if *attempt >= self.params.max_consecutive_errors as u64 {
                        self.infra_stop(state)
                    } else {
                        ControlDecision {
                            stamp: stamp_for(state, DecisionPoint::Retry, None),
                            kind: DecisionKind::Retry {
                                target: RetryTarget::ModelCall {
                                    model_call_id: Self::ext_str(state, ext::LAST_MODEL_CALL_ID)
                                        .unwrap_or("")
                                        .to_string(),
                                },
                                attempt: attempt + 1,
                                not_before: None,
                            },
                        }
                    }
                }
                EnvelopeSignal::CancelRequested { by } => self.stop(
                    state,
                    StopReason::Cancelled {
                        by: CancelledBy::parse(by).unwrap_or(CancelledBy::Principal),
                    },
                ),
                EnvelopeSignal::SoftThreshold { .. } => self.propose(state),
            },

            Cue::HumanInput(h) => match h {
                HumanInput::Interrupt => self.stop(
                    state,
                    StopReason::Cancelled {
                        by: CancelledBy::Principal,
                    },
                ),
                HumanInput::Approval { .. }
                | HumanInput::Steer { .. }
                | HumanInput::FollowUp { .. } => self.propose(state),
            },

            // `guard_fired` — a nudge landed or a refused `stop{completed}`
            // was converted; the loop proposes again (bounded by
            // `max_continue_nudges` — react/minimal's is 0, so the envelope's
            // second conversion is `refused{blocking_effect_id}`). The one
            // exception is `escalate_on_exhaustion`: the envelope fired it
            // only because `ExhaustionAction::Escalate` met
            // `interactive_attendance`, so the honest decision is
            // `escalate{handoff}` — the loop parks for the principal's
            // ledgered `amend(budget)` or `stop` (ADR-0168 D6;
            // AC-R-2.11.1-14).
            Cue::GuardFired { guard_id, .. } => {
                if guard_id == "escalate_on_exhaustion" {
                    ControlDecision {
                        stamp: stamp_for(state, DecisionPoint::Escalate, None),
                        kind: DecisionKind::Escalate {
                            ask: EscalateAsk::Handoff,
                        },
                    }
                } else if guard_id == "compaction_required" {
                    // `react/minimal` declares no `compact` — the occupancy
                    // cap is `stop{context_exhausted{required_tokens, cap}}`
                    // (§5e.1's minimal row; the numbers are the guard's
                    // ledgered pressure, folded in `observe`).
                    self.stop(
                        state,
                        StopReason::ContextExhausted {
                            required_tokens: Self::ext_u64(state, ext::CX_REQUIRED_TOKENS),
                            cap: Self::ext_u64(state, ext::CX_CAP),
                        },
                    )
                } else {
                    self.propose(state)
                }
            }

            // `woken` — every wakeup (peer messages included) resumes the
            // loop with a proposal; the payload rides the context, never
            // the decision (I7).
            Cue::Woken {
                delivery_mode,
                trigger,
                ..
            } => {
                let _ = (delivery_mode, trigger);
                self.propose(state)
            }

            // Completion cues the Stage-1 loop routes back to `propose` —
            // `verification`/`retrieval`/`compaction`/`delegation` decisions
            // are C1+ variant features; react never emits them (I3 totality
            // still requires a row for every cue).
            Cue::VerificationCompleted
            | Cue::RetrievalCompleted
            | Cue::CompactionCompleted { .. }
            | Cue::DelegationCompleted { .. } => self.propose(state),
        };

        // Record the decision (decision_count + boundary_observed — I8).
        state.record_decision(decision.stamp.decision_point, decision.stamp.owner);
        decision
    }

    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
        let submissions = match state.extension.get(ext::SUBMISSIONS) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(String::from))
                .collect::<Vec<String>>(),
            _ => vec![],
        };
        FinalReport {
            stop_reason: reason.clone(),
            submission_ref: submissions.last().cloned(),
            unresolved_effects: state.open_effects.clone(),
            decisions: vec![], // the driver owns the decision-event id list
            boundary_observed: state.boundary_view.clone(),
        }
    }

    fn restore(
        &mut self,
        bytes: &[u8],
        _ctx: &ControlContext,
    ) -> Result<ControlState, RestoreError> {
        let text = std::str::from_utf8(bytes).map_err(|_| RestoreError::Malformed)?;
        let j = hh_wire::json::parse(text).map_err(|_| RestoreError::Malformed)?;
        let dialect = j
            .get("dialect")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if dialect != crate::state::CONTROL_STATE_DIALECT {
            return Err(RestoreError::DialectMismatch {
                checkpoint: dialect,
            });
        }
        let state = ControlState::from_json(&j).ok_or(RestoreError::Malformed)?;
        if state.variant_ref != REACT_MINIMAL_REF {
            return Err(RestoreError::VariantMismatch {
                checkpoint: state.variant_ref,
                restoring: REACT_MINIMAL_REF.into(),
            });
        }
        Ok(state)
    }
}

impl ReactMinimal {
    /// The format-error row: nudge (streak++), `stop{format_failure{count}}`
    /// at `max_consecutive_format_errors`, else `propose`.
    fn format_error(&self, state: &mut ControlState) -> ControlDecision {
        let streak = Self::ext_u64(state, ext::FORMAT_ERROR_STREAK) + 1;
        Self::set_ext(state, ext::FORMAT_ERROR_STREAK, Json::Int(streak as i64));
        if streak >= self.params.max_consecutive_format_errors as u64 {
            return self.stop(
                state,
                StopReason::FormatFailure {
                    count: streak as u32,
                },
            );
        }
        self.propose(state)
    }

    /// `model_completed{error|unknown}` — `retry{model_call}` up to the
    /// bound, then `stop{infrastructure_failure{error_class}}`.
    fn retry_or_infra_stop(
        &self,
        state: &mut ControlState,
        model_call_id: &str,
    ) -> ControlDecision {
        let retries = Self::ext_u64(state, ext::MODEL_RETRIES) + 1;
        Self::set_ext(state, ext::MODEL_RETRIES, Json::Int(retries as i64));
        if retries > self.params.max_consecutive_errors as u64 {
            return self.infra_stop(state);
        }
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Retry, None),
            kind: DecisionKind::Retry {
                target: RetryTarget::ModelCall {
                    model_call_id: model_call_id.to_string(),
                },
                attempt: retries,
                not_before: None,
            },
        }
    }

    /// `stop{infrastructure_failure{error_class}}` — the class is the last
    /// `model.call.failed` class the fold recorded (a `ModelErrorClass`
    /// spelling; `unknown` when none was seen).
    fn infra_stop(&self, state: &ControlState) -> ControlDecision {
        let class = Self::ext_str(state, ext::LAST_ERROR_CLASS)
            .unwrap_or("unknown")
            .to_string();
        self.stop(
            state,
            StopReason::InfrastructureFailure {
                error_class: InfraError {
                    family: InfraErrorFamily::Model,
                    class,
                },
            },
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `react/steerable` — the Stage-2 steerable variant (R-2.6.1¹; S2.11).
// ─────────────────────────────────────────────────────────────────────────────

/// The `react/steerable` variant ref.
pub const REACT_STEERABLE_REF: &str = "hh/react-steerable@1";

/// `react/steerable` — `react/minimal`'s F1 table plus three differences
/// (§5e.1; AC-R-2.6.1):
///
/// * **steering** — `human_input{steer}`/`{follow_up}` propose the next step
///   with the payload *ref* riding `context_request` (`{steer_ref}`/
///   `{follow_up_ref}` — I7: the cue names a ref, never the bytes; the
///   assembler resolves it). Under `steer_mode = unsupported` the cue can
///   only arrive forged — the embed surface refuses `steer` before the
///   driver ever sees it (AC-R-2.6.1-10).
/// * **compact routing** — `guard_fired{compaction_required}` decides
///   `compact{reason}` (a `code`-owned point per the react β preset; the
///   driver runs the injected `CompactionPort`). Bounded (I4): when the
///   ledger shows a `context.compaction.completed` *after* the previous
///   `compaction_required` — the ladder ran and did not relieve — or its
///   status is `ineffective`/`failed`, or the signal count exceeds 4, the
///   decision is `stop{context_exhausted{required_tokens, cap}}`.
/// * **`continue_nudge` budget** — `max_continue_nudges` bounds
///   `guard_fired` continuations; exceeding it stops `loop_detected` with
///   the detector/pattern the ledgered `control.loop.detected` recorded.
///
/// Everything else delegates to the inner `ReactMinimal` (the F1 rows the
/// variants share are one implementation — `observe`, `terminate`,
/// `checkpoint`, the format/error/retry arms).
#[derive(Debug)]
pub struct ReactSteerable {
    caps: ControlCapabilities,
    params: StrategyParams,
    /// The declared `steer_mode` (`open` reads `ctx.steering.0`).
    steer_mode: crate::strategy::SteerMode,
    /// The shared F1 table.
    inner: ReactMinimal,
}

impl Default for ReactSteerable {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactSteerable {
    /// Construct the variant.
    pub fn new() -> Self {
        ReactSteerable {
            caps: ControlCapabilities {
                deterministic_replay: true,
                steering: true,
                follow_up: true,
                parallel_effects: false,
                delegation: false,
                model_emitted_plan: false,
                resumable_mid_effect: true,
                decision_points_owned: vec![
                    DecisionPoint::Plan,
                    DecisionPoint::Act,
                    DecisionPoint::Retrieve,
                    DecisionPoint::Delegate,
                ],
                boundary_preset: react_preset(),
                requires: VariantRequires {
                    goal: true,
                    procedure: false,
                },
            },
            params: StrategyParams::default(),
            steer_mode: crate::strategy::SteerMode::Unsupported,
            inner: ReactMinimal::new(),
        }
    }

    /// `propose{plan, context_request{steer_ref|follow_up_ref}}` — a steer
    /// or follow-up re-plans the next step under the referenced input (the
    /// context builder resolves `Ref<Text>` at assembly — the strategy
    /// never reads the payload, I2/I7).
    fn steered_propose(
        &self,
        state: &ControlState,
        member: &'static str,
        payload_ref: &str,
    ) -> ControlDecision {
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Plan, None),
            kind: DecisionKind::Propose {
                decision_point: DecisionPoint::Plan,
                context_request: Json::obj([(member, Json::str(payload_ref.to_string()))]),
                expected_output: ExpectedOutput::Free,
            },
        }
    }

    /// `guard_fired{compaction_required}` → `compact{reason}` — bounded by
    /// the ledgered ladder state (`context_exhausted` when a compaction
    /// already ran for this pressure, reported `ineffective`/`failed`, or
    /// the signal count trips the I4 bound).
    fn compact_or_exhausted(&self, state: &mut ControlState) -> ControlDecision {
        let exhausted = |s: &ControlState| -> ControlDecision {
            self.inner.stop(
                s,
                StopReason::ContextExhausted {
                    required_tokens: ReactMinimal::ext_u64(s, ext::CX_REQUIRED_TOKENS),
                    cap: ReactMinimal::ext_u64(s, ext::CX_CAP),
                },
            )
        };
        let prev_guard = ReactMinimal::ext_u64(state, ext::CX_PREV_GUARD_SEQ);
        let last_compact = ReactMinimal::ext_u64(state, ext::LAST_COMPACTION_SEQ);
        let count = ReactMinimal::ext_u64(state, ext::CX_GUARD_COUNT);
        let status = ReactMinimal::ext_str(state, ext::LAST_COMPACTION_STATUS)
            .unwrap_or("")
            .to_string();
        if matches!(status.as_str(), "ineffective" | "failed")
            || (prev_guard > 0 && last_compact > prev_guard)
            || count > 4
        {
            return exhausted(state);
        }
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Compact, None),
            kind: DecisionKind::Compact {
                reason: "compaction_required".into(),
            },
        }
    }
}

impl ControlStrategy for ReactSteerable {
    fn capabilities(&self) -> &ControlCapabilities {
        &self.caps
    }

    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
        ctx.boundary
            .validate()
            .map_err(ControlError::IncompatibleBoundary)?;
        self.params = ctx.parameters.clone();
        self.inner.params = ctx.parameters.clone();
        self.steer_mode = ctx.steering.0;
        let mut state = self.inner.open(ctx)?;
        state.variant_ref = REACT_STEERABLE_REF.into();
        for (k, v) in [
            (ext::CX_REQUIRED_TOKENS, Json::Int(0)),
            (ext::CX_CAP, Json::Int(0)),
            (ext::CX_GUARD_SEQ, Json::Int(0)),
            (ext::CX_PREV_GUARD_SEQ, Json::Int(0)),
            (ext::CX_GUARD_COUNT, Json::Int(0)),
            (ext::LAST_COMPACTION_SEQ, Json::Int(0)),
            (ext::CONTINUE_NUDGES, Json::Int(0)),
        ] {
            ReactMinimal::set_ext(&mut state, k, v);
        }
        Ok(state)
    }

    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        self.inner.observe(state, events)
    }

    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        let decision = match cue {
            // Steering — declared only: under `unsupported` a steer cue can
            // only arrive forged (the embed refuses before `submit`); the
            // honest fallback is the shared table's plain `propose`.
            Cue::HumanInput(HumanInput::Steer { payload_ref })
                if self.steer_mode != crate::strategy::SteerMode::Unsupported =>
            {
                self.steered_propose(state, "steer_ref", payload_ref)
            }
            Cue::HumanInput(HumanInput::FollowUp { payload_ref }) => {
                self.steered_propose(state, "follow_up_ref", payload_ref)
            }
            // Compaction routing — the react β preset owns `compact` to
            // code; the decision still passes `envelope.check` like every
            // other (F2).
            Cue::GuardFired { guard_id, .. } if guard_id == "compaction_required" => {
                self.compact_or_exhausted(state)
            }
            _ => return self.inner.decide(state, cue),
        };
        state.record_decision(decision.stamp.decision_point, decision.stamp.owner);
        decision
    }

    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
        self.inner.terminate(state, reason)
    }

    fn restore(
        &mut self,
        bytes: &[u8],
        _ctx: &ControlContext,
    ) -> Result<ControlState, RestoreError> {
        let text = std::str::from_utf8(bytes).map_err(|_| RestoreError::Malformed)?;
        let j = hh_wire::json::parse(text).map_err(|_| RestoreError::Malformed)?;
        let dialect = j
            .get("dialect")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if dialect != crate::state::CONTROL_STATE_DIALECT {
            return Err(RestoreError::DialectMismatch {
                checkpoint: dialect,
            });
        }
        let state = ControlState::from_json(&j).ok_or(RestoreError::Malformed)?;
        if state.variant_ref != REACT_STEERABLE_REF {
            return Err(RestoreError::VariantMismatch {
                checkpoint: state.variant_ref,
                restoring: REACT_STEERABLE_REF.into(),
            });
        }
        Ok(state)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The staged variants — registered with their β presets + required inputs
// (AC-R-2.6.1-7's bind half); `decide` parks (`wait`) until their stage lands
// (C1/Stage 2–4 — a staged variant never fakes a step).
// ─────────────────────────────────────────────────────────────────────────────

/// A staged variant — `open`/`restore`/`observe`/`terminate` are real;
/// `decide` returns `wait{until: human_input}` (the variants' model/code
/// paths land at their stage).
pub struct StagedVariant {
    variant_ref: String,
    caps: ControlCapabilities,
    /// The staged `decide` — parks every cue on `human_input` (the
    /// envelope's ceilings still bound the run — I4).
    parked: bool,
}

impl StagedVariant {
    fn new(variant_ref: &str, caps: ControlCapabilities) -> Self {
        StagedVariant {
            variant_ref: variant_ref.into(),
            caps,
            parked: true,
        }
    }
}

impl ControlStrategy for StagedVariant {
    fn capabilities(&self) -> &ControlCapabilities {
        &self.caps
    }

    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
        ctx.boundary
            .validate()
            .map_err(ControlError::IncompatibleBoundary)?;
        if self.caps.requires.procedure && ctx.plan.is_empty() {
            return Err(ControlError::MissingProcedure);
        }
        Ok(ControlState {
            variant_ref: self.variant_ref.clone(),
            cursor: PlanCursor {
                node_id: "staged-0".into(),
                iteration: 0,
                bound_ref: ctx.budget_ref.clone(),
            },
            decision_count: 0,
            open_effects: vec![],
            last_cue_seq: 0,
            boundary_view: ctx.boundary.assignments.clone(),
            extension: Json::obj([("staged", Json::Bool(self.parked))]),
        })
    }

    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        for ev in events {
            if ev.seq > state.last_cue_seq {
                state.last_cue_seq = ev.seq;
            }
        }
    }

    fn decide(&self, state: &mut ControlState, _cue: &Cue) -> ControlDecision {
        // Staged: the variant's interpreter lands at its build stage — the
        // run parks on principal input rather than fabricating steps (the
        // envelope's ceilings still bound it — I4).
        let d = ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Act, None),
            kind: DecisionKind::Wait {
                until: WaitUntil::CueKind {
                    cue_kind: "human_input".into(),
                },
            },
        };
        state.record_decision(d.stamp.decision_point, d.stamp.owner);
        d
    }

    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
        FinalReport {
            stop_reason: reason.clone(),
            submission_ref: None,
            unresolved_effects: state.open_effects.clone(),
            decisions: vec![],
            boundary_observed: state.boundary_view.clone(),
        }
    }

    fn restore(
        &mut self,
        bytes: &[u8],
        _ctx: &ControlContext,
    ) -> Result<ControlState, RestoreError> {
        let text = std::str::from_utf8(bytes).map_err(|_| RestoreError::Malformed)?;
        let j = hh_wire::json::parse(text).map_err(|_| RestoreError::Malformed)?;
        let dialect = j
            .get("dialect")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if dialect != crate::state::CONTROL_STATE_DIALECT {
            return Err(RestoreError::DialectMismatch {
                checkpoint: dialect,
            });
        }
        let state = ControlState::from_json(&j).ok_or(RestoreError::Malformed)?;
        if state.variant_ref != self.variant_ref {
            return Err(RestoreError::VariantMismatch {
                checkpoint: state.variant_ref,
                restoring: self.variant_ref.clone(),
            });
        }
        Ok(state)
    }
}

/// The family registry (ADR-0103 D6 — one `RuntimePlan/1` interpreter under
/// two switches: the β preset and `plan_provenance`; hybrids are parameter
/// points, never a fifth variant). Stage-1 registers all four with their
/// presets; `decide` lands per stage.
pub fn registry() -> Vec<Box<dyn ControlStrategy>> {
    let plan_execute = {
        let mut b = ControlBoundary::default();
        for (p, o) in [
            (DecisionPoint::Plan, Owner::Model),
            (DecisionPoint::Act, Owner::Code),
            (DecisionPoint::Retrieve, Owner::Code),
            (DecisionPoint::Compact, Owner::Code),
            (DecisionPoint::Verify, Owner::Code),
            (DecisionPoint::Delegate, Owner::Model),
            (DecisionPoint::Authorize, Owner::Code),
            (DecisionPoint::Retry, Owner::Code),
            (DecisionPoint::Stop, Owner::Code),
            (DecisionPoint::Escalate, Owner::Human),
        ] {
            b.assignments.insert(p, o);
        }
        ControlCapabilities {
            deterministic_replay: true,
            steering: false,
            follow_up: true,
            parallel_effects: false,
            delegation: true,
            model_emitted_plan: true,
            resumable_mid_effect: true,
            decision_points_owned: vec![DecisionPoint::Plan, DecisionPoint::Delegate],
            boundary_preset: b,
            requires: VariantRequires {
                goal: true,
                procedure: true,
            },
        }
    };
    let workflow = {
        let mut b = ControlBoundary::default();
        for p in DecisionPoint::ALL {
            b.assignments.insert(p, Owner::Code);
        }
        b.assignments.insert(DecisionPoint::Escalate, Owner::Human);
        ControlCapabilities {
            deterministic_replay: true,
            steering: false,
            follow_up: false,
            parallel_effects: true,
            delegation: true,
            model_emitted_plan: false,
            resumable_mid_effect: true,
            decision_points_owned: vec![],
            boundary_preset: b,
            requires: VariantRequires {
                goal: true,
                procedure: true,
            },
        }
    };
    let program = {
        let mut b = ControlBoundary::default();
        for p in DecisionPoint::ALL {
            b.assignments.insert(p, Owner::Code);
        }
        b.assignments.insert(DecisionPoint::Escalate, Owner::Human);
        ControlCapabilities {
            deterministic_replay: true,
            steering: false,
            follow_up: false,
            parallel_effects: true,
            delegation: true,
            model_emitted_plan: false,
            resumable_mid_effect: true,
            decision_points_owned: vec![],
            boundary_preset: b,
            requires: VariantRequires {
                goal: true,
                procedure: true,
            },
        }
    };
    vec![
        Box::new(ReactMinimal::new()),
        Box::new(ReactSteerable::new()),
        Box::new(StagedVariant::new("hh/plan-execute@1", plan_execute)),
        Box::new(StagedVariant::new("hh/workflow@1", workflow)),
        Box::new(StagedVariant::new("hh/program@1", program)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{ConcurrentInput, SteerMode};
    use crate::vocab::{EffectOutcome, WokenTrigger};

    fn ctx(boundary: ControlBoundary) -> ControlContext {
        ControlContext {
            process_ref: "proc-1".into(),
            plan: vec![],
            boundary,
            profile: Json::Null,
            account_ref: "acct-1".into(),
            budget_ref: "budget-1".into(),
            envelope_ref: "env-1".into(),
            parameters: StrategyParams::default(),
            capabilities_available: vec![],
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        }
    }

    fn opened() -> ControlState {
        let mut v = ReactMinimal::new();
        v.open(&ctx(react_preset())).unwrap()
    }

    fn decide(state: &mut ControlState, cue: Cue) -> DecisionKind {
        let v = ReactMinimal::new();
        v.decide(state, &cue).kind
    }

    #[test]
    fn run_opened_proposes_act() {
        let mut s = opened();
        let d = decide(
            &mut s,
            Cue::RunOpened {
                goal_ref: "g".into(),
                inputs: Json::Null,
            },
        );
        assert!(matches!(d, DecisionKind::Propose { .. }));
    }

    #[test]
    fn tool_use_acts_on_pending_intents_sequentially() {
        let mut s = opened();
        ReactMinimal::set_ext(
            &mut s,
            ext::PENDING_INTENTS,
            Json::Arr(vec![Json::str("tc-1"), Json::str("tc-2")]),
        );
        let d = decide(
            &mut s,
            Cue::ModelCompleted {
                model_call_id: "m-1".into(),
                response_ref: "r".into(),
                stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            },
        );
        match d {
            DecisionKind::Act { intents, mode, .. } => {
                assert_eq!(mode, ActMode::Sequential);
                assert_eq!(intents.len(), 2);
            }
            other => panic!("expected act, got {other:?}"),
        }
    }

    #[test]
    fn end_turn_no_intents_is_the_format_error_row() {
        let mut s = opened();
        let cue = Cue::ModelCompleted {
            model_call_id: "m".into(),
            response_ref: "r".into(),
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
        };
        // streak 1,2 → propose; 3 → stop{format_failure{3}}.
        for i in 1..=2u64 {
            assert!(
                matches!(decide(&mut s, cue.clone()), DecisionKind::Propose { .. }),
                "streak {i} should propose"
            );
        }
        match decide(&mut s, cue) {
            DecisionKind::Stop {
                proposed_reason, ..
            } => assert_eq!(proposed_reason, StopReason::FormatFailure { count: 3 }),
            other => panic!("expected stop, got {other:?}"),
        }
    }

    #[test]
    fn submission_yields_stop_completed() {
        let mut s = opened();
        let d = decide(
            &mut s,
            Cue::EffectsSettled {
                settled: vec![EffectOutcome {
                    effect_id: "e".into(),
                    outcome: SettledOutcome::Observed {
                        outcome: "applied".into(),
                    },
                }],
                all_terminal: true,
                submission_ref: Some("sub-1".into()),
            },
        );
        match d {
            DecisionKind::Stop {
                proposed_reason,
                submission_ref,
            } => {
                assert_eq!(proposed_reason, StopReason::Completed);
                assert_eq!(submission_ref.as_deref(), Some("sub-1"));
            }
            other => panic!("expected stop{{completed}}, got {other:?}"),
        }
    }

    #[test]
    fn insufficient_budget_signal_stops_budget_exhausted() {
        let mut s = opened();
        let d = decide(
            &mut s,
            Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                decision_ref: "d-1".into(),
                reason: "insufficient_budget{turns}".into(),
            }),
        );
        match d {
            DecisionKind::Stop {
                proposed_reason, ..
            } => assert_eq!(
                proposed_reason,
                StopReason::BudgetExhausted {
                    budget_id: "budget-1".into(),
                    dimension: hh_ontology::dimensions::DimensionId::Turns,
                }
            ),
            other => panic!("expected stop{{budget_exhausted}}, got {other:?}"),
        }
    }

    #[test]
    fn pause_turn_is_a_control_retry_never_a_gateway_retry() {
        let mut s = opened();
        let d = decide(
            &mut s,
            Cue::ModelCompleted {
                model_call_id: "m-9".into(),
                response_ref: "r".into(),
                stop_reason: hh_gateway::vocab::StopReason::PauseTurn,
            },
        );
        match d {
            DecisionKind::Retry { target, .. } => assert_eq!(
                target,
                RetryTarget::ModelCall {
                    model_call_id: "m-9".into()
                }
            ),
            other => panic!("expected retry{{model_call}}, got {other:?}"),
        }
    }

    #[test]
    fn interrupt_stops_cancelled_principal() {
        let mut s = opened();
        let d = decide(&mut s, Cue::HumanInput(HumanInput::Interrupt));
        match d {
            DecisionKind::Stop {
                proposed_reason, ..
            } => assert_eq!(
                proposed_reason,
                StopReason::Cancelled {
                    by: CancelledBy::Principal
                }
            ),
            other => panic!("expected stop{{cancelled}}, got {other:?}"),
        }
    }

    #[test]
    fn open_effects_park_every_cue_but_settled_and_cancel() {
        let mut s = opened();
        s.open_effects = vec!["e-1".into()];
        let d = decide(
            &mut s,
            Cue::Woken {
                trigger: WokenTrigger::PeerMessage { from: "r".into() },
                payload_ref: "p".into(),
                delivery_mode: crate::vocab::DeliveryMode::FollowUp,
            },
        );
        assert!(matches!(d, DecisionKind::Wait { .. }));
        // …and a mid-effect interrupt still wins (INV-3 stop is a barrier).
        let d2 = decide(&mut s, Cue::HumanInput(HumanInput::Interrupt));
        assert!(matches!(d2, DecisionKind::Stop { .. }));
    }

    #[test]
    fn incompatible_boundary_is_refused_at_open() {
        let mut b = react_preset();
        b.assignments.insert(DecisionPoint::Authorize, Owner::Model);
        let mut v = ReactMinimal::new();
        assert!(matches!(
            v.open(&ctx(b)),
            Err(ControlError::IncompatibleBoundary(_))
        ));
    }

    #[test]
    fn checkpoint_restore_round_trips_and_variant_mismatch_is_typed() {
        let mut v = ReactMinimal::new();
        let s = opened();
        let bytes = v.checkpoint(&s);
        let back = v.restore(&bytes, &ctx(react_preset())).unwrap();
        assert_eq!(back, s);
        // A foreign variant ref is VariantMismatch.
        let mut other = s.clone();
        other.variant_ref = "hh/plan-execute@1".into();
        let bytes2 = other.checkpoint();
        assert!(matches!(
            v.restore(&bytes2, &ctx(react_preset())),
            Err(RestoreError::VariantMismatch { .. })
        ));
    }

    #[test]
    fn all_four_variants_register_and_bind() {
        let vars = registry();
        assert_eq!(vars.len(), 5); // react/minimal + steerable + plan_execute + workflow + program
                                   // Every preset validates (I5 — none assigns a reserved point to model).
        for v in &vars {
            let c = v.capabilities();
            c.boundary_preset.validate().unwrap();
        }
    }

    #[test]
    fn deferred_is_a_wait_on_model_completed() {
        let mut s = opened();
        let d = decide(
            &mut s,
            Cue::ModelCompleted {
                model_call_id: "m".into(),
                response_ref: "r".into(),
                stop_reason: hh_gateway::vocab::StopReason::Deferred,
            },
        );
        match d {
            DecisionKind::Wait { until } => assert_eq!(
                until,
                WaitUntil::CueKind {
                    cue_kind: "model_completed".into()
                }
            ),
            other => panic!("expected wait, got {other:?}"),
        }
    }

    // ── S2.11 — `react/steerable` (R-2.6.1¹) ─────────────────────────────

    fn steerable_ctx() -> ControlContext {
        let mut c = ctx(react_preset());
        c.steering = (SteerMode::InterruptAtDecisionPoint, ConcurrentInput::Steer);
        c
    }

    fn steerable_opened() -> ControlState {
        let mut v = ReactSteerable::new();
        v.open(&steerable_ctx()).unwrap()
    }

    /// A `ReactSteerable` opened against the steerable ctx (decide reads
    /// `self.steer_mode`, so the variant instance must be the opened one).
    fn steerable() -> ReactSteerable {
        let mut v = ReactSteerable::new();
        v.open(&steerable_ctx()).unwrap();
        v
    }

    #[test]
    fn steerable_declares_steering_and_follow_up() {
        let v = ReactSteerable::new();
        let c = v.capabilities();
        assert!(c.steering);
        assert!(c.follow_up);
        assert!(c.deterministic_replay);
        c.boundary_preset.validate().unwrap();
    }

    #[test]
    fn steerable_steer_proposes_with_the_ref_not_the_bytes() {
        let v = steerable();
        let mut s = steerable_opened();
        let d = v.decide(
            &mut s,
            &Cue::HumanInput(HumanInput::Steer {
                payload_ref: "sha256:steer-1".into(),
            }),
        );
        match d.kind {
            DecisionKind::Propose {
                context_request, ..
            } => {
                assert_eq!(
                    context_request.get("steer_ref").and_then(Json::as_str),
                    Some("sha256:steer-1")
                );
            }
            other => panic!("expected propose, got {other:?}"),
        }
    }

    #[test]
    fn steerable_follow_up_proposes_with_follow_up_ref() {
        let mut s = steerable_opened();
        let v = steerable();
        let d = v.decide(
            &mut s,
            &Cue::HumanInput(HumanInput::FollowUp {
                payload_ref: "sha256:fu-1".into(),
            }),
        );
        match d.kind {
            DecisionKind::Propose {
                context_request, ..
            } => {
                assert_eq!(
                    context_request.get("follow_up_ref").and_then(Json::as_str),
                    Some("sha256:fu-1")
                );
            }
            other => panic!("expected propose, got {other:?}"),
        }
    }

    #[test]
    fn steerable_compaction_required_decides_compact() {
        let mut s = steerable_opened();
        let v = steerable();
        let d = v.decide(
            &mut s,
            &Cue::GuardFired {
                decision_point: DecisionPoint::Act,
                guard_id: "compaction_required".into(),
            },
        );
        match d.kind {
            DecisionKind::Compact { reason } => {
                assert_eq!(reason, "compaction_required")
            }
            other => panic!("expected compact, got {other:?}"),
        }
        // The compact point is code-owned under the react preset.
        assert_eq!(d.stamp.decision_point, DecisionPoint::Compact);
        assert_eq!(d.stamp.owner, Owner::Code);
    }

    #[test]
    fn steerable_unrelieved_compaction_stops_context_exhausted() {
        let mut s = steerable_opened();
        let v = steerable();
        // The ledgered sequence: guard.fired{cx} (seq 10) →
        // compaction.completed{applied} (seq 20) → guard.fired{cx} (seq 30)
        // — the ladder ran and did not relieve.
        ReactMinimal::set_ext(&mut s, ext::CX_GUARD_SEQ, Json::Int(30));
        ReactMinimal::set_ext(&mut s, ext::CX_PREV_GUARD_SEQ, Json::Int(10));
        ReactMinimal::set_ext(&mut s, ext::LAST_COMPACTION_SEQ, Json::Int(20));
        ReactMinimal::set_ext(&mut s, ext::LAST_COMPACTION_STATUS, Json::str("applied"));
        ReactMinimal::set_ext(&mut s, ext::CX_GUARD_COUNT, Json::Int(2));
        ReactMinimal::set_ext(&mut s, ext::CX_REQUIRED_TOKENS, Json::Int(950));
        ReactMinimal::set_ext(&mut s, ext::CX_CAP, Json::Int(1000));
        let d = v.decide(
            &mut s,
            &Cue::GuardFired {
                decision_point: DecisionPoint::Act,
                guard_id: "compaction_required".into(),
            },
        );
        match d.kind {
            DecisionKind::Stop {
                proposed_reason:
                    StopReason::ContextExhausted {
                        required_tokens,
                        cap,
                    },
                ..
            } => {
                assert_eq!(required_tokens, 950);
                assert_eq!(cap, 1000);
            }
            other => panic!("expected stop{{context_exhausted}}, got {other:?}"),
        }
    }

    #[test]
    fn steerable_ineffective_compaction_stops_context_exhausted() {
        let mut s = steerable_opened();
        let v = steerable();
        ReactMinimal::set_ext(
            &mut s,
            ext::LAST_COMPACTION_STATUS,
            Json::str("ineffective"),
        );
        ReactMinimal::set_ext(&mut s, ext::CX_CAP, Json::Int(1000));
        let d = v.decide(
            &mut s,
            &Cue::GuardFired {
                decision_point: DecisionPoint::Act,
                guard_id: "compaction_required".into(),
            },
        );
        assert!(matches!(
            d.kind,
            DecisionKind::Stop {
                proposed_reason: StopReason::ContextExhausted { .. },
                ..
            }
        ));
    }

    #[test]
    fn minimal_compaction_required_stops_context_exhausted() {
        let mut s = opened();
        ReactMinimal::set_ext(&mut s, ext::CX_REQUIRED_TOKENS, Json::Int(950));
        ReactMinimal::set_ext(&mut s, ext::CX_CAP, Json::Int(1000));
        let d = decide(
            &mut s,
            Cue::GuardFired {
                decision_point: DecisionPoint::Act,
                guard_id: "compaction_required".into(),
            },
        );
        assert!(matches!(
            d,
            DecisionKind::Stop {
                proposed_reason: StopReason::ContextExhausted {
                    required_tokens: 950,
                    cap: 1000
                },
                ..
            }
        ));
    }

    #[test]
    fn steerable_restore_round_trips_and_rejects_foreign_variants() {
        let mut v = ReactSteerable::new();
        let s = steerable_opened();
        let bytes = v.checkpoint(&s);
        let restored = v.restore(&bytes, &steerable_ctx()).unwrap();
        assert_eq!(restored.variant_ref, REACT_STEERABLE_REF);
        // A react/minimal checkpoint refuses (variant mismatch).
        let m = ReactMinimal::new();
        let ms = opened();
        let mbytes = m.checkpoint(&ms);
        assert!(matches!(
            v.restore(&mbytes, &steerable_ctx()),
            Err(RestoreError::VariantMismatch { .. })
        ));
    }
}
