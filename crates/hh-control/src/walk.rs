//! `PlanWalker` — the `hh/workflow@1`/`hh/program@1` interpreter (R3.3;
//! ADR-0103 D6's "one `RuntimePlan/1` interpreter" arm for the compiled-plan
//! family). The staged `wait{human_input}` park is retired: the variant
//! walks `ctx.plan` — the sealed definition's `RuntimePlan/1.control` nodes
//! (declaration data; β is `code` for every point but `escalate`) — one
//! admitted decision per cue, durable rows before visibility, typed
//! refusals where the substrate refuses. `compute_policy = static` means
//! the driver never mints scheduler rows — the walk's cadence is exactly
//! cue-in/decision-out (AC-F4-1).
//!
//! Node → decision table (the closed `PlanNodePayload` set):
//!
//! | node | decision | awaits |
//! |---|---|---|
//! | `step{instruction}` | `propose{context_request{instruction_ref}}` | `model_completed` |
//! | `step{invoke}` | `act{intents[{surface, args}]}` (`step.mode` 1:1) | `effects_settled` |
//! | `branch-on-validator` | `verify{validator_refs}` | `verification_completed` → then/else on the folded verdict (fail-closed) |
//! | `delegate{spec,budget,permission}` | `delegate{spec, …}` — the spec's own `wait`, `requested_grants`, mandatory `delegation_reason` ride the audit row | `woken{child_terminal}`/`delegation_completed` when `spec.wait.mode ≠ background`; `background` walks on |
//! | `loop{bound_budget, body}` | body frames; the `Budget` is the bound — the envelope's `budget_exhausted` ends it (I4) | — |
//! | `stop-rule{…}` | skipped — the boundary's envelope-reserved declarations (`BudgetEnvelope.stop_rule_nodes`); the envelope fires them as `refused`/`guard_fired`/`insufficient_budget` signals (I6), never a sequential step | — |
//! | plan exhausted | `stop{completed}` | — |
//!
//! A mid-walk cue the phase does not await parks `wait{cue_kind}` — a
//! bounded, typed park on the audit row, never a fake step. An interrupt /
//! `cancel_requested` answers `stop{cancelled}` at any phase (INV-3's
//! barrier); a refused signal answers `stop{refused}` or
//! `stop{budget_exhausted}`; a `guard_fired` the compiled plan cannot
//! interpret answers `escalate{handoff}` — the boundary's one human-owned
//! point.
//!
//! The walk position lives in `state.extension.walk` (`{frames, phase}` —
//! container-kind/index/iteration triples resolved against `self.plan`),
//! so `checkpoint`/`restore` round-trips the interpreter mid-step (the
//! plan re-binds from `ctx` at `open`/`restore`; the position is the
//! state's own).

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{
    CancelledBy, ControlBoundary, DecisionPoint, InvariantId, Owner, StopReason,
};
use hh_ontology::dimensions::DimensionId;
use hh_wire::json::Json;

use hh_compiler::plan::{PlanNode, PlanNodePayload, StepAction, StepMode};

use crate::state::{ControlState, PlanCursor};
use crate::strategy::{
    stamp_for, ControlCapabilities, ControlContext, ControlError, ControlStrategy, FinalReport,
    PlanProvenance, RestoreError, StrategyParams, VariantRequires,
};
use crate::vocab::{
    ActMode, ControlDecision, Cue, DecisionKind, EnvelopeSignal, EscalateAsk, ExpectedOutput,
    HumanInput, OnPartial, SettledOutcome, WaitUntil, WokenTrigger,
};

/// `walk` extension members (the closed per-variant record — container
/// kinds, indices and counters only; never authority-bearing).
mod ext {
    /// The walk position `{frames: [{k, i, it}], phase}`.
    pub const WALK: &str = "walk";
    /// The folded `verification.validator.verdict{status, value}` — the
    /// `branch-on-validator` arm reads it (the ledgered verdict, never a
    /// guess).
    pub const LAST_VERDICT: &str = "last_verdict";
}

/// The walk phases — what the next decision is gated on (names carry the
/// awaited cue kind: `await_effect` → `effects_settled`, `await_model` →
/// `model_completed`, `await_verify` → `verification_completed`,
/// `await_child` → `woken{child_terminal}`/`delegation_completed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Ready for the next node's decision.
    Ready,
    /// A `step{invoke}` dispatched.
    AwaitEffect,
    /// A `step{instruction}` model call ran.
    AwaitModel,
    /// A `branch-on-validator` `verify` ran.
    AwaitVerify,
    /// A `delegate` spawned under `wait.mode ≠ background`.
    AwaitChild,
    /// The plan's terminal `stop` was emitted.
    Done,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Ready => "ready",
            Phase::AwaitEffect => "await_effect",
            Phase::AwaitModel => "await_model",
            Phase::AwaitVerify => "await_verify",
            Phase::AwaitChild => "await_child",
            Phase::Done => "done",
        }
    }
    fn parse(s: &str) -> Option<Phase> {
        Some(match s {
            "ready" => Phase::Ready,
            "await_effect" => Phase::AwaitEffect,
            "await_model" => Phase::AwaitModel,
            "await_verify" => Phase::AwaitVerify,
            "await_child" => Phase::AwaitChild,
            "done" => Phase::Done,
            _ => return None,
        })
    }
}

/// A walk frame `{k, i, it}` — the container kind (`root`/`body`/`then`/
/// `else`/`loop`), the index into that container, and the iteration (the
/// `loop` marker's own; `0` elsewhere).
type Frame = (String, u64, u64);

/// `PlanWalker` — one interpreter for `hh/workflow@1` and `hh/program@1`
/// (the `RuntimePlan/1` walk under identical β presets — the variants
/// differ by declared topology/preset members, never by a second
/// interpreter).
pub struct PlanWalker {
    variant_ref: String,
    caps: ControlCapabilities,
    params: StrategyParams,
    /// The bound `RuntimePlan/1.control` — `ctx.plan` at `open`/`restore`
    /// (declaration data the code-owned β interprets — never state; the
    /// position rides `extension.walk`).
    plan: Vec<PlanNode>,
}

impl PlanWalker {
    /// Construct the variant for `variant_ref` with `caps`.
    pub fn new(variant_ref: &str, caps: ControlCapabilities) -> Self {
        PlanWalker {
            variant_ref: variant_ref.into(),
            caps,
            params: StrategyParams::default(),
            plan: vec![],
        }
    }

    fn set_ext(state: &mut ControlState, key: &'static str, v: Json) {
        let mut m = match &state.extension {
            Json::Obj(m) => m.clone(),
            _ => std::collections::BTreeMap::new(),
        };
        m.insert(key.to_string(), v);
        state.extension = Json::Obj(m);
    }

    /// The walk `{frames, phase}` — the extension's own record.
    fn walk(state: &ControlState) -> (Vec<Frame>, Phase) {
        let w = state.extension.get(ext::WALK);
        let frames = match w.and_then(|w| w.get("frames")) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| {
                    Some((
                        i.get("k")?.as_str()?.to_string(),
                        i.get("i")?.as_int()?.max(0) as u64,
                        i.get("it")?.as_int()?.max(0) as u64,
                    ))
                })
                .collect(),
            _ => vec![],
        };
        let phase = w
            .and_then(|w| w.get("phase"))
            .and_then(Json::as_str)
            .and_then(Phase::parse)
            .unwrap_or(Phase::Done);
        (frames, phase)
    }

    fn set_walk(state: &mut ControlState, frames: &[Frame], phase: Phase) {
        let frames = Json::Arr(
            frames
                .iter()
                .map(|(k, i, it)| {
                    Json::obj([
                        ("k", Json::str(k.clone())),
                        ("i", Json::Int(*i as i64)),
                        ("it", Json::Int(*it as i64)),
                    ])
                })
                .collect(),
        );
        Self::set_ext(
            state,
            ext::WALK,
            Json::obj([("frames", frames), ("phase", Json::str(phase.name()))]),
        );
    }

    /// The node list frame `depth - 1` indexes — `root` is `self.plan`;
    /// `body`/`then`/`else` resolve the descent node's own list: the
    /// descended node sits at `parent_i - 1` in the parent's container
    /// (a parent's index moved past the node when it was consumed, and
    /// never moves again while the descent lives). A `loop` frame is a
    /// marker — its list is the grandparent's, and the loop node is at
    /// `grandparent_i - 1`.
    fn frame_nodes<'a>(&'a self, frames: &[Frame], depth: usize) -> Option<&'a [PlanNode]> {
        if depth == 0 {
            return Some(&self.plan);
        }
        match frames[depth - 1].0.as_str() {
            "root" => Some(&self.plan),
            "then" | "else" => {
                let parent_nodes = self.frame_nodes(frames, depth - 1)?;
                let bi = frames.get(depth - 2)?.1.checked_sub(1)? as usize;
                match &parent_nodes.get(bi)?.payload {
                    PlanNodePayload::BranchOnValidator(b) => {
                        Some(if frames[depth - 1].0 == "then" {
                            &b.then_body
                        } else {
                            &b.else_body
                        })
                    }
                    _ => None,
                }
            }
            "body" => {
                if frames.get(depth - 2)?.0 != "loop" {
                    return None;
                }
                let parent_nodes = self.frame_nodes(frames, depth - 2)?;
                let li = frames.get(depth - 3)?.1.checked_sub(1)? as usize;
                match &parent_nodes.get(li)?.payload {
                    PlanNodePayload::Loop(l) => Some(&l.body),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn stamp(state: &ControlState, point: DecisionPoint) -> crate::vocab::DecisionStamp {
        stamp_for(state, point, None)
    }

    fn stop(state: &ControlState, reason: StopReason) -> ControlDecision {
        ControlDecision {
            stamp: Self::stamp(state, DecisionPoint::Stop),
            kind: DecisionKind::Stop {
                proposed_reason: reason,
                submission_ref: None,
            },
        }
    }

    /// Advance the walk to the next decision-producing node (a descent
    /// ceiling bounds it — a corrupt frame set or unbody'd `loop` cannot
    /// spin; the ceiling's terminal is `invariant_violation`).
    fn advance(&self, state: &mut ControlState) -> ControlDecision {
        let (mut frames, _) = Self::walk(state);
        for _ in 0..4096 {
            let depth = frames.len();
            if depth == 0 {
                Self::set_walk(state, &frames, Phase::Done);
                state.cursor.node_id = "done".into();
                return Self::stop(state, StopReason::Completed);
            }
            let nodes = match self.frame_nodes(&frames, depth) {
                Some(n) => n,
                None => {
                    // A frame set that no longer resolves against the
                    // plan — the typed terminal names the violation,
                    // never a silently re-derived position.
                    Self::set_walk(state, &frames, Phase::Done);
                    return Self::stop(
                        state,
                        StopReason::InvariantViolation {
                            invariant_id: InvariantId::Ext("walk_position_corrupt".into()),
                        },
                    );
                }
            };
            let (k, i, _) = frames[depth - 1].clone();
            if i as usize >= nodes.len() {
                frames.pop();
                if k == "body" {
                    if let Some(top) = frames.last_mut() {
                        if top.0 == "loop" {
                            // Loop body exhausted — iterate under
                            // `Loop.bound_budget` (the envelope's
                            // `insufficient_budget` is the terminator).
                            top.2 += 1;
                            let nit = top.2;
                            state.cursor.iteration = nit;
                            frames.push(("body".into(), 0, nit));
                            continue;
                        }
                    }
                }
                continue;
            }
            let node = &nodes[i as usize];
            frames[depth - 1].1 = i + 1;
            state.cursor.node_id = node.node_id.clone();
            match &node.payload {
                PlanNodePayload::Loop(_) => {
                    frames.push(("loop".into(), 0, 0));
                    frames.push(("body".into(), 0, 0));
                }
                PlanNodePayload::Step(step) => match &step.action {
                    StepAction::Instruction { content_hash, .. } => {
                        Self::set_walk(state, &frames, Phase::AwaitModel);
                        let expected = match &step.output_schema {
                            Some(schema) => ExpectedOutput::Schema {
                                validator_ref: schema.semantic_id.clone(),
                            },
                            None => ExpectedOutput::Free,
                        };
                        return ControlDecision {
                            stamp: Self::stamp(state, DecisionPoint::Plan),
                            kind: DecisionKind::Propose {
                                decision_point: DecisionPoint::Plan,
                                context_request: Json::obj([(
                                    "instruction_ref",
                                    Json::str(content_hash.clone()),
                                )]),
                                expected_output: expected,
                            },
                        };
                    }
                    StepAction::Invoke { capability, args } => {
                        Self::set_walk(state, &frames, Phase::AwaitEffect);
                        return ControlDecision {
                            stamp: Self::stamp(state, DecisionPoint::Act),
                            kind: DecisionKind::Act {
                                intents: vec![Json::obj([
                                    ("surface", Json::str(capability.semantic_id.clone())),
                                    ("args", args.clone()),
                                ])],
                                mode: match step.mode {
                                    StepMode::Sequential => ActMode::Sequential,
                                    StepMode::Parallel => ActMode::Parallel,
                                },
                                on_partial: OnPartial::FailBatch,
                            },
                        };
                    }
                },
                PlanNodePayload::BranchOnValidator(b) => {
                    Self::set_walk(state, &frames, Phase::AwaitVerify);
                    return ControlDecision {
                        stamp: Self::stamp(state, DecisionPoint::Verify),
                        kind: DecisionKind::Verify {
                            validator_refs: vec![b.validator.semantic_id.clone()],
                            subject: Json::obj([]),
                        },
                    };
                }
                PlanNodePayload::Delegate(d) => {
                    // `delegate{spec}` — the compiled `SubagentSpec` rides
                    // the audit row; `delegation_reason` is the spec's own
                    // member — absent ⇒ `missing_delegation_reason` at
                    // `check` (the walker never fills a default, CC2).
                    let awaits = d
                        .spec
                        .get("wait")
                        .and_then(|w| w.get("mode"))
                        .and_then(Json::as_str)
                        != Some("background");
                    let decision = ControlDecision {
                        stamp: Self::stamp(state, DecisionPoint::Delegate),
                        kind: DecisionKind::Delegate {
                            spec: d.spec.clone(),
                            budget_slice: Json::obj([
                                ("budget_ref", Json::str(d.budget.semantic_id.clone())),
                                ("budget_version", Json::str(d.budget.version_id.clone())),
                            ]),
                            permissions: Json::obj([
                                (
                                    "permission_ref",
                                    Json::str(d.permission.semantic_id.clone()),
                                ),
                                (
                                    "permission_version",
                                    Json::str(d.permission.version_id.clone()),
                                ),
                            ]),
                            delegation_reason: d.spec.get("delegation_reason").cloned(),
                        },
                    };
                    Self::set_walk(
                        state,
                        &frames,
                        if awaits {
                            Phase::AwaitChild
                        } else {
                            Phase::Ready
                        },
                    );
                    return decision;
                }
                PlanNodePayload::StopRule(_) => {
                    // The boundary's envelope-reserved declarations
                    // (`BudgetEnvelope.stop_rule_nodes`) — the envelope
                    // fires them as `refused`/`guard_fired` signals; a
                    // declaration is never a sequential step (I6).
                }
            }
        }
        Self::set_walk(state, &frames, Phase::Done);
        Self::stop(
            state,
            StopReason::InvariantViolation {
                invariant_id: InvariantId::Ext("walk_descent_bound".into()),
            },
        )
    }
    /// The decide body — `decide` (the trait surface) records the
    /// returned stamp on the state.
    fn raw_decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        match cue {
            Cue::HumanInput(HumanInput::Interrupt) => {
                return Self::stop(
                    state,
                    StopReason::Cancelled {
                        by: CancelledBy::Principal,
                    },
                );
            }
            Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested { by }) => {
                return Self::stop(
                    state,
                    StopReason::Cancelled {
                        by: CancelledBy::parse(by).unwrap_or(CancelledBy::Principal),
                    },
                );
            }
            Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                decision_ref,
                reason,
            }) => {
                if reason.starts_with("insufficient_budget") {
                    let dim = reason
                        .strip_prefix("insufficient_budget{")
                        .and_then(|s| s.strip_suffix('}'))
                        .unwrap_or("unknown");
                    return Self::stop(
                        state,
                        StopReason::BudgetExhausted {
                            budget_id: state.cursor.bound_ref.clone(),
                            dimension: DimensionId::parse(dim).unwrap_or(DimensionId::Turns),
                        },
                    );
                }
                return Self::stop(
                    state,
                    StopReason::Refused {
                        blocking_effect_id: decision_ref.clone(),
                    },
                );
            }
            Cue::GuardFired { .. } => {
                // A guard the compiled plan cannot interpret — the
                // boundary's one human-owned point answers (never a
                // silently continued walk).
                return ControlDecision {
                    stamp: Self::stamp(state, DecisionPoint::Escalate),
                    kind: DecisionKind::Escalate {
                        ask: EscalateAsk::Handoff,
                    },
                };
            }
            _ => {}
        }
        let (_, phase) = Self::walk(state);
        match phase {
            Phase::Done => Self::stop(state, StopReason::Completed),
            Phase::AwaitEffect => match cue {
                Cue::EffectsSettled {
                    settled,
                    all_terminal,
                    ..
                } => {
                    if !*all_terminal {
                        decision_park(state, "effects_settled")
                    } else if settled.iter().any(|s| {
                        matches!(
                            s.outcome,
                            SettledOutcome::Refused | SettledOutcome::Abandoned
                        )
                    }) {
                        // A refused/abandoned step effect ends the walk —
                        // the plan carries no `on_failure` (recorded on
                        // the `stop` row; never continued).
                        Self::stop(
                            state,
                            StopReason::Refused {
                                blocking_effect_id: settled
                                    .iter()
                                    .find(|s| {
                                        matches!(
                                            s.outcome,
                                            SettledOutcome::Refused | SettledOutcome::Abandoned
                                        )
                                    })
                                    .map(|s| s.effect_id.clone())
                                    .unwrap_or_default(),
                            },
                        )
                    } else {
                        self.advance(state)
                    }
                }
                _ => decision_park(state, "effects_settled"),
            },
            Phase::AwaitModel => match cue {
                Cue::ModelCompleted { .. } => self.advance(state),
                _ => decision_park(state, "model_completed"),
            },
            Phase::AwaitVerify => match cue {
                Cue::VerificationCompleted => {
                    // `decided` + affirmative ⇒ `then`; every other
                    // verdict is fail-closed `else`.
                    let verdict = state.extension.get(ext::LAST_VERDICT);
                    let pass = verdict.and_then(|v| v.get("status")).and_then(Json::as_str)
                        == Some("decided")
                        && verdict.and_then(|v| v.get("value")).is_some_and(|v| {
                            v.get("bool") == Some(&Json::Bool(true))
                                || v.get("three_valued").and_then(Json::as_str) == Some("pass")
                                || v.get("lattice").and_then(Json::as_str) == Some("N")
                        });
                    let (mut frames, _) = Self::walk(state);
                    frames.push((if pass { "then" } else { "else" }.to_string(), 0, 0));
                    Self::set_walk(state, &frames, Phase::Ready);
                    self.advance(state)
                }
                _ => decision_park(state, "verification_completed"),
            },
            Phase::AwaitChild => match cue {
                Cue::Woken {
                    trigger: WokenTrigger::ChildTerminal { .. },
                    ..
                }
                | Cue::DelegationCompleted { .. } => {
                    let (frames, _) = Self::walk(state);
                    Self::set_walk(state, &frames, Phase::Ready);
                    self.advance(state)
                }
                _ => decision_park(state, "woken"),
            },
            Phase::Ready => self.advance(state),
        }
    }
}

impl ControlStrategy for PlanWalker {
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
        // `plan_provenance` (ADR-0103 D6) — the walker interprets a
        // *compiled* `RuntimePlan/1`; a `model_emitted` declaration under
        // `model_emitted_plan = false` is the typed refusal the capability
        // declares, never a coerced walk.
        if ctx.parameters.plan_provenance == PlanProvenance::ModelEmitted {
            return Err(ControlError::UnsupportedPlanNode {
                node: "plan_provenance:model_emitted".into(),
            });
        }
        self.params = ctx.parameters.clone();
        self.plan = ctx.plan.clone();
        let mut state = ControlState {
            variant_ref: self.variant_ref.clone(),
            cursor: PlanCursor {
                node_id: "root".into(),
                iteration: 0,
                bound_ref: ctx.budget_ref.clone(),
            },
            decision_count: 0,
            open_effects: vec![],
            last_cue_seq: 0,
            boundary_view: ctx.boundary.assignments.clone(),
            extension: Json::Null,
        };
        Self::set_walk(&mut state, &[("root".into(), 0, 0)], Phase::Ready);
        Ok(state)
    }

    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        for ev in events {
            if ev.seq <= state.last_cue_seq {
                continue;
            }
            state.last_cue_seq = ev.seq;
            match ev.class.as_str() {
                // The branch arm's verdict fold — `decided` +
                // affirmative ⇒ `then`; everything else ⇒ `else`
                // (fail-closed, never a guessed pass).
                "verification.validator.verdict" => {
                    Self::set_ext(
                        state,
                        ext::LAST_VERDICT,
                        Json::obj([
                            (
                                "status",
                                ev.payload.get("status").cloned().unwrap_or(Json::Null),
                            ),
                            (
                                "value",
                                ev.payload.get("value").cloned().unwrap_or(Json::Null),
                            ),
                        ]),
                    );
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
                    if let Some(e) = &ev.scope.effect_id {
                        state.open_effects.retain(|x| x != e);
                    }
                }
                _ => {}
            }
        }
    }

    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        let d = self.raw_decide(state, cue);
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
        ctx: &ControlContext,
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
        // The plan re-binds from `ctx` — the position rides the state; a
        // mismatched plan is `frame_nodes`' typed terminal, never a
        // silently re-derived position.
        self.params = ctx.parameters.clone();
        self.plan = ctx.plan.clone();
        Ok(state)
    }
}

/// `wait{cue_kind}` — the bounded park a mismatched cue answers (a typed
/// decision on the audit row — never a fake step, never a spin).
fn decision_park(state: &ControlState, cue_kind: &str) -> ControlDecision {
    ControlDecision {
        stamp: stamp_for(state, DecisionPoint::Act, None),
        kind: DecisionKind::Wait {
            until: WaitUntil::CueKind {
                cue_kind: cue_kind.into(),
            },
        },
    }
}

/// The `hh/workflow@1`/`hh/program@1` capabilities + β preset — every
/// point `code` but `escalate → human`; `requires.procedure` — an empty
/// `ctx.plan` is `MissingProcedure`, never a zero-step run.
pub fn plan_walker_caps() -> ControlCapabilities {
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
}
