//! The `control_strategy` contract (§5e.1 interface contract; ADR-0103
//! D1–D3): `capabilities` / `open` / `observe` / `decide` / `terminate` /
//! `checkpoint` / `restore`. A variant is pure — it folds durable ledger
//! events into `ControlState` and answers each `Cue` with one
//! `ControlDecision` stamped `{decision_point, owner}`; it never issues an
//! effect, holds a handle, reads `Text`, or calls a model (I2/I7). The Core
//! driver executes; the envelope checks.
//!
//! The class is registered per ADR-0023 with `required_inputs ⊇
//! {ModelProfile, ResourceAccount}` (T-LCD-08); the `ClassRecord` declares
//! `steer_mode` and `concurrent_input` ([`ClassSteering`]).

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{BoundaryError, ControlBoundary, DecisionPoint, Owner, StopReason};
use hh_wire::json::Json;

use crate::state::ControlState;
use crate::vocab::{ControlDecision, Cue, DecisionStamp};

/// `ControlCapabilities` — the declared capability set (§5e.1; "declared,
/// then tested"; `n/a` when absent).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlCapabilities {
    /// `deterministic_replay` — re-driving the variant over recorded
    /// `model_io` reproduces the decision sequence (ADR-0028; AC-4).
    pub deterministic_replay: bool,
    /// `steering` — `steer` cues are admitted (`steer_mode` declared).
    pub steering: bool,
    /// `follow_up` — follow-up input is admitted.
    pub follow_up: bool,
    /// `parallel_effects` — `act` may be returned while `open_effects ≠ ∅`.
    pub parallel_effects: bool,
    /// `delegation` — `delegate` decisions are admitted (R-2.6.3 present).
    pub delegation: bool,
    /// `model_emitted_plan` — `plan_provenance = model_emitted`.
    pub model_emitted_plan: bool,
    /// `resumable_mid_effect` — resume tolerates in-flight effects.
    pub resumable_mid_effect: bool,
    /// `decision_points_owned` — the points β assigns to the model under
    /// this variant's preset.
    pub decision_points_owned: Vec<DecisionPoint>,
    /// `boundary_preset` — the variant's default `ControlBoundary` (data,
    /// overridable per definition; `open` refuses a preset assigning an
    /// envelope-reserved point to `model`).
    pub boundary_preset: ControlBoundary,
    /// `requires{goal, procedure?}` — the variant's input requirements.
    pub requires: VariantRequires,
}

/// `requires{goal, procedure?}` — a `MissingProcedure` `open` error when a
/// required procedure is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantRequires {
    /// A `Goal` is required (always true at Stage 1).
    pub goal: bool,
    /// A `Procedure` is required (`workflow`/`plan_execute`).
    pub procedure: bool,
}

/// `steer_mode`/`concurrent_input` — the `ClassRecord` declarations
/// (ADR-0103 Phase 3 log; ADR-0176).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteerMode {
    /// `interrupt_at_decision_point` — steer delivered at the next decide.
    InterruptAtDecisionPoint,
    /// `queue_next_turn` — steer queued for the next turn.
    QueueNextTurn,
    /// `unsupported` — `steer` → `Unsupported{by: control_strategy}`.
    Unsupported,
}

/// `concurrent_input ∈ {queue_only, steer}` — `submit` during an active
/// turn returns `TurnActive` under `queue_only` (AC-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrentInput {
    /// `queue_only`.
    QueueOnly,
    /// `steer`.
    Steer,
}

/// `ControlContext` — `open`'s input (§5e.1): `{process_ref, plan:
/// RuntimePlan.control, boundary, profile, account, budget, envelope,
/// parameters, capabilities_available}`.
#[derive(Debug, Clone)]
pub struct ControlContext {
    /// The `AgentProcess` ref.
    pub process_ref: String,
    /// The `RuntimePlan/1` control nodes (the variant walks them — react's
    /// implicit `Loop(bound){step{propose};step{act}}` for `react/minimal`).
    pub plan: Vec<hh_compiler::plan::PlanNode>,
    /// The declared β (validated by `open` — I5).
    pub boundary: ControlBoundary,
    /// The `ModelProfile` the variant must honour (`required_inputs` —
    /// T-LCD-08); carried as the sealed profile's JSON projection (the
    /// strategy reads profile *facts* — e.g. the retry bound — never model
    /// I/O).
    pub profile: Json,
    /// The `ResourceAccount` ref (`required_inputs`).
    pub account_ref: String,
    /// The root budget id (`cursor.bound_ref` binds it).
    pub budget_ref: String,
    /// The envelope policy ref (the driver enforces it — the strategy sees
    /// the ref, not the policy: INV-7).
    pub envelope_ref: String,
    /// The MUST-data parameter record ([`StrategyParams`]).
    pub parameters: StrategyParams,
    /// `capabilities_available` — which capability inputs the definition
    /// supplies (a `delegate` decision without the R-2.6.3 subsystem is
    /// `DelegationUnavailable`, T0).
    pub capabilities_available: Vec<String>,
    /// `steer_mode` + `concurrent_input` from the `ClassRecord`.
    pub steering: (SteerMode, ConcurrentInput),
}

/// The MUST-data parameter record (§5e.1; ADR-0103 D7): `stop_rule ∈
/// {no_action, submit, either}`, `max_continue_nudges`,
/// `max_consecutive_format_errors`, `max_consecutive_errors`,
/// `tool_batch_mode ∈ {sequential, parallel}`, `on_truncated_response`,
/// `plan_provenance`, `replan_on`, `loop_guards[]`.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyParams {
    /// `stop_rule` — how a submission is detected.
    pub stop_rule: StopRuleParam,
    /// `max_continue_nudges` — the bound on `guard_fired` conversions of a
    /// refused `stop{completed}` (I6) and on re-plans (AC-7).
    pub max_continue_nudges: u32,
    /// `max_consecutive_format_errors` — the format-error streak bound
    /// (`react/minimal`: 3).
    pub max_consecutive_format_errors: u32,
    /// `max_consecutive_errors` — the generic error streak bound.
    pub max_consecutive_errors: u32,
    /// `tool_batch_mode`.
    pub tool_batch_mode: crate::vocab::ActMode,
    /// `on_truncated_response` (default `fail_calls`).
    pub on_truncated_response: TruncatedResponse,
    /// `plan_provenance ∈ {compiled, model_emitted}` (ADR-0103 D6 switch).
    pub plan_provenance: PlanProvenance,
    /// `replan_on ∈ {never, failure, always}`.
    pub replan_on: ReplanOn,
    /// `loop_guards[]` — declared guard refs (the envelope's `LoopPolicy`
    /// owns the deterministic detectors; this list is the variant's own
    /// guard refs, `[]` for react/minimal).
    pub loop_guards: Vec<String>,
}

/// `stop_rule ∈ {no_action, submit, either}` — `submit`: the marker on the
/// first line of an effect's `observed` output with `returncode == 0`,
/// detected by the **driver**, never the environment (ADR-0104).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRuleParam {
    /// `no_action` — no submission marker; completion is a `stop{completed}`
    /// proposal only.
    NoAction,
    /// `submit` — the marker-on-observed rule (react/minimal's value).
    Submit,
    /// `either` — either mechanism.
    Either,
}

/// `on_truncated_response` — what a `max_output`/`truncated` response does
/// to its tool calls (default `fail_calls` — the failed batch never
/// executes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatedResponse {
    /// `fail_calls` — the truncated batch's calls all fail; the strategy
    /// proposes again.
    FailCalls,
}

/// `plan_provenance ∈ {compiled, model_emitted}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanProvenance {
    /// The plan is a compiled `RuntimePlan/1` artifact.
    Compiled,
    /// The plan is model-emitted (`Procedure` artifact, `authority =
    /// delegate`, schema-validated before execution — `plan_execute`).
    ModelEmitted,
}

/// `replan_on ∈ {never, failure, always}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanOn {
    /// Never re-plan.
    Never,
    /// Re-plan on step failure.
    Failure,
    /// Re-plan every step.
    Always,
}

impl Default for StrategyParams {
    /// The `react/minimal` fixed parameters (ADR-0104): `stop_rule = submit`,
    /// `max_continue_nudges = 0`, `max_consecutive_format_errors = 3`,
    /// `tool_batch_mode = sequential`, `on_truncated_response = fail_calls`,
    /// `loop_guards = []`.
    fn default() -> Self {
        StrategyParams {
            stop_rule: StopRuleParam::Submit,
            max_continue_nudges: 0,
            max_consecutive_format_errors: 3,
            max_consecutive_errors: 3,
            tool_batch_mode: crate::vocab::ActMode::Sequential,
            on_truncated_response: TruncatedResponse::FailCalls,
            plan_provenance: PlanProvenance::Compiled,
            replan_on: ReplanOn::Never,
            loop_guards: vec![],
        }
    }
}

/// `ControlError` — `open`'s typed refusals plus `decide`'s
/// conformance-time `UnhandledCue` (raised at the conformance suite, never
/// at run time — I3).
#[derive(Debug, Clone, PartialEq)]
pub enum ControlError {
    /// `IncompatibleBoundary` — the boundary assigns an envelope-reserved
    /// point to a non-code owner (I5; wraps [`BoundaryError`]).
    IncompatibleBoundary(BoundaryError),
    /// `MissingProcedure` — the variant `requires.procedure` and none is
    /// present.
    MissingProcedure,
    /// `UnsupportedPlanNode{node}` — a plan node the variant cannot
    /// interpret (closed-world rule — never an escape hatch).
    UnsupportedPlanNode {
        /// The offending node.
        node: String,
    },
    /// `UnhandledCue` — a cue the variant's `decide` does not cover
    /// (conformance-time only — `decide` is total at run time, I3).
    UnhandledCue {
        /// The cue kind.
        cue_kind: String,
    },
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlError::IncompatibleBoundary(e) => {
                write!(f, "incompatible_boundary{{{:?}→{:?}}}", e.point, e.owner)
            }
            ControlError::MissingProcedure => write!(f, "missing_procedure"),
            ControlError::UnsupportedPlanNode { node } => {
                write!(f, "unsupported_plan_node{{{node}}}")
            }
            ControlError::UnhandledCue { cue_kind } => write!(f, "unhandled_cue{{{cue_kind}}}"),
        }
    }
}

impl std::error::Error for ControlError {}

/// `RestoreError` — `restore`'s typed refusals (§5e.1; ADR-0130).
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreError {
    /// The checkpoint names a different variant.
    VariantMismatch {
        /// The checkpoint's variant.
        checkpoint: String,
        /// The restoring variant.
        restoring: String,
    },
    /// The checkpoint's dialect tag differs.
    DialectMismatch {
        /// The checkpoint's dialect.
        checkpoint: String,
    },
    /// The bytes do not parse as a `ControlState` checkpoint.
    Malformed,
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::VariantMismatch {
                checkpoint,
                restoring,
            } => write!(f, "variant_mismatch{{{checkpoint}≠{restoring}}}"),
            RestoreError::DialectMismatch { checkpoint } => {
                write!(f, "dialect_mismatch{{{checkpoint}}}")
            }
            RestoreError::Malformed => write!(f, "malformed_checkpoint"),
        }
    }
}

impl std::error::Error for RestoreError {}

/// `FinalReport{stop_reason, submission_ref?, unresolved_effects[],
/// decisions, boundary_observed}` — `terminate`'s pure output (§5e.1).
#[derive(Debug, Clone, PartialEq)]
pub struct FinalReport {
    /// The admitted stop reason.
    pub stop_reason: StopReason,
    /// The submission ref, when `stop_rule` detected one.
    pub submission_ref: Option<String>,
    /// Effects still non-terminal at stop (incl. `abandoned` — the
    /// ADR-0113 gate reads them).
    pub unresolved_effects: Vec<String>,
    /// The recorded `control.decision` event ids, in order.
    pub decisions: Vec<String>,
    /// `boundary_observed` — the owner map as exercised (I8; the driver
    /// diffs it against the declared `assignments` for
    /// `control.boundary_drift` — reported, never corrected).
    pub boundary_observed: BTreeMap<DecisionPoint, Owner>,
}

/// The `control_strategy` contract (§5e.1). Object-safe: the driver holds
/// `Box<dyn ControlStrategy>`; the conformance suite drives a variant
/// out-of-process over canonical `Cue`/`ControlDecision` documents (AC-1).
///
/// Contract obligations (I2–I4): every method is pure over its inputs —
/// `observe`/`decide`/`terminate` read `ControlState` + events/cues only;
/// `decide` is total (must return `stop` when `cursor.bound` is exhausted;
/// may not return `act` while `open_effects ≠ ∅` unless
/// `capabilities.parallel_effects`).
pub trait ControlStrategy {
    /// `capabilities(variant)` — the declared set.
    fn capabilities(&self) -> &ControlCapabilities;

    /// `open(ctx)` → `ControlState₀`. Validates the boundary (I5 —
    /// `IncompatibleBoundary` on an envelope-reserved point owned by
    /// non-code) and the plan nodes (`UnsupportedPlanNode`), checks
    /// `requires` (`MissingProcedure`).
    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError>;

    /// `observe(state, events[])` → `state'` — a pure fold of `seq`-ordered
    /// durable events; idempotent on `last_cue_seq`.
    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]);

    /// `decide(state, cue)` → `(state', ControlDecision)` — pure, total.
    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision;

    /// `terminate(state, reason)` → `FinalReport` — called by the driver
    /// after the envelope has closed every open effect (pure, no effects).
    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport;

    /// `checkpoint(state)` → canonical bytes (ADR-0029 class).
    fn checkpoint(&self, state: &ControlState) -> Vec<u8> {
        state.checkpoint()
    }

    /// `restore(bytes, ctx)` → `state` — validates `variant_ref` and the
    /// dialect tag (`VariantMismatch`/`DialectMismatch`); the variant
    /// re-validates its extension.
    fn restore(&mut self, bytes: &[u8], ctx: &ControlContext)
        -> Result<ControlState, RestoreError>;
}

/// `Box<dyn ControlStrategy>` forwards every member — the session driver is
/// variant-generic (`Driver<Box<dyn ControlStrategy>>` — S2.11's steerable
/// half; the embed boundary picks the variant the sealed definition's
/// `slots.control_strategy` names).
impl ControlStrategy for Box<dyn ControlStrategy> {
    fn capabilities(&self) -> &ControlCapabilities {
        (**self).capabilities()
    }
    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
        (**self).open(ctx)
    }
    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        (**self).observe(state, events)
    }
    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        (**self).decide(state, cue)
    }
    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
        (**self).terminate(state, reason)
    }
    fn checkpoint(&self, state: &ControlState) -> Vec<u8> {
        (**self).checkpoint(state)
    }
    fn restore(
        &mut self,
        bytes: &[u8],
        ctx: &ControlContext,
    ) -> Result<ControlState, RestoreError> {
        (**self).restore(bytes, ctx)
    }
}

/// A helper for variants: stamp a decision with the boundary's effective
/// owner for its point (`boundary_view` — the map `open` seeds from
/// `ctx.boundary`).
pub fn stamp_for(
    state: &ControlState,
    point: DecisionPoint,
    rationale_ref: Option<String>,
) -> DecisionStamp {
    DecisionStamp {
        decision_point: point,
        owner: state
            .boundary_view
            .get(&point)
            .copied()
            .unwrap_or(Owner::Code),
        rationale_ref,
    }
}
