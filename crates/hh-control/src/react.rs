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
    ActMode, ControlDecision, Cue, DecisionKind, EnvelopeSignal, ExpectedOutput, HumanInput,
    OnPartial, RetryTarget, SettledOutcome, WaitUntil,
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
                    if reason.starts_with("retry_budget_exhausted") || reason.starts_with("give_up")
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
            // second conversion is `refused{blocking_effect_id}`).
            Cue::GuardFired { .. } => self.propose(state),

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
    let react_steerable = {
        let mut b = react_preset();
        b.assignments.insert(DecisionPoint::Escalate, Owner::Human);
        ControlCapabilities {
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
            boundary_preset: b,
            requires: VariantRequires {
                goal: true,
                procedure: false,
            },
        }
    };
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
        Box::new(StagedVariant::new("hh/react-steerable@1", react_steerable)),
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
}
