//! `plan_execute` — the `model_emitted_plan` variant (ADR-0103 D6; §5e.1;
//! S3.10): the model emits a `Procedure`-shaped plan as a call on the
//! declared `hh.plan` surface; the driver schema-validates it and ledgers
//! `control.plan.emitted{plan_ref, schema_validator_ref, valid, steps}`;
//! the strategy sequences the validated steps (`act`/`verify`/`propose`
//! per step kind) and `stop{completed}`s when the plan drains.
//!
//! The plan artefact is `authority = delegate` — it may *propose* steps,
//! never confer them (CC2): every `act` step still flows `decide → check →
//! execute` (I1), every `verify` step runs through the `VerifyPort`, and a
//! plan naming an undeclared surface fails the closed-world check
//! (`control.plan.emitted{valid: false}` → re-plan bounded by
//! `max_continue_nudges` → `stop{format_failure}` — the honest exhausted
//! ladder, never a silent skip).
//!
//! The β preset (the registry's `plan_execute` row): `plan`/`delegate` are
//! `model`-owned, every other point `code` — the plan emission is the one
//! model-owned decision point; step sequencing is the driver's.

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{ControlBoundary, DecisionPoint, Owner, StopReason};
use hh_wire::json::Json;

use crate::state::{ControlState, PlanCursor};
use crate::strategy::{
    stamp_for, ControlCapabilities, ControlContext, ControlError, ControlStrategy, FinalReport,
    PlanProvenance, RestoreError, StrategyParams, VariantRequires,
};
use crate::vocab::{
    ActMode, ControlDecision, Cue, DecisionKind, ExpectedOutput, OnPartial, WaitUntil,
};

/// The `plan_execute` variant ref.
pub const PLAN_EXECUTE_REF: &str = "hh/plan-execute@1";

/// The `hh.plan` surface id — the model-emitted plan arrives as a call on
/// this declared surface (a closed-schema `args` document; the driver
/// validates it before `control.plan.emitted` lands).
pub const PLAN_SURFACE_ID: &str = "hh.plan";

/// The default plan-schema validator ref (`control.plan.emitted`'s
/// `schema_validator_ref` member when the context declares no override).
pub const PLAN_SCHEMA_REF: &str = "hh/plan-execute/schema@1";

/// The plan's closed grammar (Stage-3): `{steps: [{kind ∈ {act, verify,
/// propose}, …}]}`. `act` steps name a declared `surface` + `args`;
/// `verify` steps name `validator_refs`; `propose` steps drive a model
/// round. Unknown kinds/members are rejections, never skips (R-PARSE).
pub const PLAN_STEP_KINDS: &[&str] = &["act", "verify", "propose"];

/// The `act`-step member whitelist.
const ACT_STEP_MEMBERS: &[&str] = &["kind", "surface", "args", "effect_class", "read_only"];
/// The `verify`-step member whitelist.
const VERIFY_STEP_MEMBERS: &[&str] = &["kind", "validator_refs", "subject"];
/// The `propose`-step member whitelist.
const PROPOSE_STEP_MEMBERS: &[&str] = &["kind", "context_request"];

/// The step-count bound (a plan is bounded by construction — I4).
pub const MAX_PLAN_STEPS: usize = 64;

/// `plan_validate` — the closed-schema codec + closed-world capability
/// check for a model-emitted plan (the `control.plan.emitted` validator —
/// a deterministic detector; the `schema_validator_ref` names this codec).
/// `Ok(steps)` is the canonical step list; `Err(reason)` produces
/// `valid: false` with the reason tag (never a partial plan).
pub fn plan_validate(
    args_raw: &str,
    declared_surfaces: &[crate::output::SurfaceSpec],
) -> Result<Vec<Json>, &'static str> {
    let j = hh_wire::json::parse(args_raw).map_err(|_| "unparseable")?;
    let steps_j = match j.get("steps") {
        Some(Json::Arr(a)) => a.clone(),
        _ => return Err("no_steps"),
    };
    if steps_j.is_empty() || steps_j.len() > MAX_PLAN_STEPS {
        return Err("steps_out_of_bounds");
    }
    let mut steps = Vec::new();
    for s in &steps_j {
        let Json::Obj(m) = s else {
            return Err("step_not_object");
        };
        let kind = m
            .get("kind")
            .and_then(Json::as_str)
            .ok_or("step_no_kind")?;
        if !PLAN_STEP_KINDS.contains(&kind) {
            return Err("step_kind_unknown");
        }
        let allowed = match kind {
            "act" => ACT_STEP_MEMBERS,
            "verify" => VERIFY_STEP_MEMBERS,
            _ => PROPOSE_STEP_MEMBERS,
        };
        if m.keys().any(|k| !allowed.contains(&k.as_str())) {
            return Err("step_member_unknown");
        }
        match kind {
            "act" => {
                let surface = m
                    .get("surface")
                    .and_then(Json::as_str)
                    .ok_or("act_no_surface")?;
                // The closed-world check — a plan step names only declared
                // surfaces (a `hh.plan` self-call is refused too).
                if !declared_surfaces
                    .iter()
                    .any(|s| s.surface_id == surface)
                    || surface == PLAN_SURFACE_ID
                {
                    return Err("surface_undeclared");
                }
            }
            "verify" => {
                match m.get("validator_refs") {
                    Some(Json::Arr(a)) if !a.is_empty() && a.iter().all(|v| v.as_str().is_some()) => {}
                    _ => return Err("verify_no_refs"),
                }
            }
            _ => {}
        }
        steps.push(s.clone());
    }
    Ok(steps)
}

/// `control.plan.emitted{plan_ref, schema_validator_ref, valid, steps?}` —
/// the driver's plan-validation row (S3.10; a deterministic detector row —
/// `detector: deterministic`).
pub fn plan_emitted_payload(
    plan_ref: &str,
    schema_validator_ref: &str,
    valid: bool,
    steps: &[Json],
    reject_reason: Option<&str>,
) -> Json {
    Json::obj([
        ("plan_ref", Json::str(plan_ref)),
        ("schema_validator_ref", Json::str(schema_validator_ref)),
        ("valid", Json::Bool(valid)),
        ("detector", Json::str("deterministic")),
        (
            "steps",
            Json::Arr(steps.to_vec()),
        ),
        (
            "reject_reason",
            reject_reason.map_or(Json::Null, |r| Json::str(r)),
        ),
    ])
}

mod ext {
    /// `awaiting_plan` | `executing` | `failed` — the plan phase.
    pub const PHASE: &str = "plan_phase";
    /// The validated plan steps (the `control.plan.emitted{steps}` member).
    pub const STEPS: &str = "plan_steps";
    /// The current step index.
    pub const IDX: &str = "plan_idx";
    /// The accepted plan's `plan_ref` (verify subjects name it).
    pub const PLAN_REF: &str = "plan_ref";
    /// Re-plan count (`max_continue_nudges` bound — AC-7).
    pub const REPLANS: &str = "plan_replans";
    /// Validator refs a `completion_refused` hold demanded
    /// (`require_validator(…)` required actions — the gate's own rows).
    pub const REQUIRED_VALIDATORS: &str = "plan_required_validators";
    /// The pending step awaiting `model_completed` (`propose` steps whose
    /// model emitted tool calls act before advancing).
    pub const PENDING_INTENTS: &str = "plan_pending_intents";
    /// The completion-submission ref (the plan's last `act` may carry it).
    pub const SUBMISSION: &str = "plan_submission";
}

/// `PlanExecute` — the `model_emitted_plan` variant (S3.10; one
/// `RuntimePlan/1` interpreter under the `plan_provenance` switch —
/// ADR-0103 D6).
pub struct PlanExecute {
    caps: ControlCapabilities,
    params: StrategyParams,
    /// The plan-schema validator ref (the `propose{plan}` expected_output +
    /// the `control.plan.emitted` schema_validator_ref).
    plan_schema_ref: String,
}

impl PlanExecute {
    /// Construct the variant.
    pub fn new() -> Self {
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
        PlanExecute {
            caps: ControlCapabilities {
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
                    // The plan is model-emitted through `hh.plan` — the
                    // variant binds no compiled procedure at `open`
                    // (S3.10; `plan_provenance = ModelEmitted` is the
                    // defining attribute).
                    procedure: false,
                },
            },
            params: StrategyParams::default(),
            plan_schema_ref: PLAN_SCHEMA_REF.to_string(),
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

    fn steps(state: &ControlState) -> Vec<Json> {
        match state.extension.get(ext::STEPS) {
            Some(Json::Arr(a)) => a.clone(),
            _ => vec![],
        }
    }

    /// `propose{decision_point: plan, expected_output: schema}` — the
    /// plan-emission call (the model-owned `plan` point).
    fn plan_propose(&self, state: &ControlState) -> ControlDecision {
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Plan, None),
            kind: DecisionKind::Propose {
                decision_point: DecisionPoint::Plan,
                context_request: Json::obj([("kind", Json::str("plan"))]),
                expected_output: ExpectedOutput::Schema {
                    validator_ref: self.plan_schema_ref.clone(),
                },
            },
        }
    }

    fn stop(&self, state: &ControlState, reason: StopReason) -> ControlDecision {
        ControlDecision {
            stamp: stamp_for(state, DecisionPoint::Stop, None),
            kind: DecisionKind::Stop {
                proposed_reason: reason,
                submission_ref: Self::ext_str(state, ext::SUBMISSION).map(str::to_string),
            },
        }
    }

    /// The next step's decision (or `stop{completed}` past the last step).
    fn step_decision(&self, state: &mut ControlState) -> ControlDecision {
        let steps = Self::steps(state);
        let idx = Self::ext_u64(state, ext::IDX) as usize;
        let Some(step) = steps.get(idx) else {
            return self.stop(state, StopReason::Completed);
        };
        match step.get("kind").and_then(Json::as_str) {
            Some("act") => {
                let surface = step
                    .get("surface")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                let args = step.get("args").cloned().unwrap_or(Json::Null);
                let plan_ref = Self::ext_str(state, ext::PLAN_REF).unwrap_or("plan");
                let intent = Json::obj([
                    (
                        "tool_call_id",
                        Json::str(format!("{plan_ref}-step-{idx}")),
                    ),
                    ("surface", Json::str(surface)),
                    ("args", args),
                    (
                        "effect_class",
                        step.get("effect_class")
                            .cloned()
                            .unwrap_or_else(|| Json::str("reversible")),
                    ),
                    ("plan_step", Json::Int(idx as i64)),
                    ("plan_ref", Json::str(plan_ref)),
                ]);
                ControlDecision {
                    stamp: stamp_for(state, DecisionPoint::Act, None),
                    kind: DecisionKind::Act {
                        intents: vec![intent],
                        mode: ActMode::Sequential,
                        on_partial: OnPartial::FailBatch,
                    },
                }
            }
            Some("verify") => {
                let refs: Vec<String> = match step.get("validator_refs") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect(),
                    _ => vec![],
                };
                ControlDecision {
                    stamp: stamp_for(state, DecisionPoint::Verify, None),
                    kind: DecisionKind::Verify {
                        validator_refs: refs,
                        subject: Json::str(
                            Self::ext_str(state, ext::PLAN_REF).unwrap_or("plan"),
                        ),
                    },
                }
            }
            Some("propose") => ControlDecision {
                stamp: stamp_for(state, DecisionPoint::Plan, None),
                kind: DecisionKind::Propose {
                    decision_point: DecisionPoint::Act,
                    context_request: step
                        .get("context_request")
                        .cloned()
                        .unwrap_or(Json::Null),
                    expected_output: ExpectedOutput::Free,
                },
            },
            _ => self.stop(state, StopReason::FormatFailure { count: 0 }),
        }
    }

    /// Advance past the current step and emit the next decision.
    fn advance(&self, state: &mut ControlState) -> ControlDecision {
        Self::set_ext(state, ext::IDX, Json::Int(Self::ext_u64(state, ext::IDX) as i64 + 1));
        self.step_decision(state)
    }
}

impl Default for PlanExecute {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlStrategy for PlanExecute {
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
        self.params = ctx.parameters.clone();
        // The `plan_provenance` switch is this variant's defining attribute —
        // a compiled-plan context still binds (the switch is the caller's;
        // the variant's behaviour is `model_emitted` regardless).
        let _ = PlanProvenance::ModelEmitted;
        Ok(ControlState {
            variant_ref: PLAN_EXECUTE_REF.into(),
            cursor: PlanCursor {
                node_id: "plan-0".into(),
                iteration: 0,
                bound_ref: ctx.budget_ref.clone(),
            },
            decision_count: 0,
            open_effects: vec![],
            last_cue_seq: 0,
            boundary_view: ctx.boundary.assignments.clone(),
            extension: Json::obj([
                (ext::PHASE, Json::str("awaiting_plan")),
                (ext::STEPS, Json::Arr(vec![])),
                (ext::IDX, Json::Int(0)),
                (ext::REPLANS, Json::Int(0)),
                (ext::REQUIRED_VALIDATORS, Json::Arr(vec![])),
                (ext::PENDING_INTENTS, Json::Arr(vec![])),
            ]),
        })
    }

    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        for ev in events {
            if ev.seq <= state.last_cue_seq {
                continue;
            }
            state.last_cue_seq = ev.seq;
            match ev.class.as_str() {
                // The driver's plan-validation row — the strategy folds the
                // *validated* steps (closed-schema data, never the raw
                // bytes — I2).
                "control.plan.emitted" => {
                    if matches!(ev.payload.get("valid"), Some(Json::Bool(true))) {
                        let steps = ev
                            .payload
                            .get("steps")
                            .cloned()
                            .unwrap_or(Json::Arr(vec![]));
                        Self::set_ext(state, ext::STEPS, steps);
                        Self::set_ext(state, ext::IDX, Json::Int(0));
                        Self::set_ext(state, ext::PHASE, Json::str("executing"));
                        if let Some(r) = ev.payload.get("plan_ref").and_then(Json::as_str) {
                            Self::set_ext(state, ext::PLAN_REF, Json::str(r));
                        }
                    } else {
                        // `valid: false` — the next `model_completed` decides
                        // the bounded re-plan.
                        Self::set_ext(state, ext::STEPS, Json::Arr(vec![]));
                        Self::set_ext(state, ext::PHASE, Json::str("awaiting_plan"));
                    }
                }
                // A `propose` step's validated calls park as pending intents
                // (the `act` decision names tool_call_ids — I2).
                "action.tool.proposed" => {
                    if let Some(tc) = ev.scope.tool_call_id.clone() {
                        let mut p = match state.extension.get(ext::PENDING_INTENTS) {
                            Some(Json::Arr(items)) => items
                                .iter()
                                .filter_map(|i| i.as_str().map(String::from))
                                .collect::<Vec<String>>(),
                            _ => vec![],
                        };
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
                // The gate's refused-completion audit — the required actions
                // are the strategy's next-step input (require_validator →
                // `verify`; resolve_effect → the effect stays open).
                "control.guard.fired"
                    if ev.payload.get("guard_id").and_then(Json::as_str)
                        == Some("completion_refused") =>
                {
                    if let Some(Json::Arr(actions)) = ev.payload.get("required_actions") {
                        let refs: Vec<String> = actions
                            .iter()
                            .filter_map(|a| a.as_str())
                            .filter_map(|a| {
                                a.strip_prefix("require_validator(")
                                    .and_then(|r| r.strip_suffix(')'))
                                    .map(str::to_string)
                            })
                            .collect();
                        Self::set_ext(
                            state,
                            ext::REQUIRED_VALIDATORS,
                            Json::Arr(refs.iter().map(Json::str).collect()),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        // I4 totality — open effects park every cue but the settle/cancel
        // pair (`parallel_effects` is not declared).
        let parked = !state.open_effects.is_empty()
            && !matches!(cue, Cue::EffectsSettled { .. })
            && !matches!(
                cue,
                Cue::EnvelopeSignal(crate::vocab::EnvelopeSignal::CancelRequested { .. })
                    | Cue::HumanInput(crate::vocab::HumanInput::Interrupt)
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
            Cue::RunOpened { .. } | Cue::Resumed { .. } => {
                Self::set_ext(state, ext::PHASE, Json::str("awaiting_plan"));
                self.plan_propose(state)
            }

            Cue::ModelCompleted { stop_reason, .. } => {
                use hh_gateway::vocab::StopReason as Gw;
                match Self::ext_str(state, ext::PHASE) {
                    Some("awaiting_plan") => {
                        if !Self::steps(state).is_empty() {
                            // A valid plan landed — step 0 decides.
                            self.step_decision(state)
                        } else {
                            // Invalid/missing plan — bounded re-plan (AC-7:
                            // `max_continue_nudges` bounds the ladder).
                            let n = Self::ext_u64(state, ext::REPLANS) + 1;
                            Self::set_ext(state, ext::REPLANS, Json::Int(n as i64));
                            if n > self.params.max_continue_nudges as u64 {
                                self.stop(state, StopReason::FormatFailure { count: n as u32 })
                            } else {
                                self.plan_propose(state)
                            }
                        }
                    }
                    Some("executing") => {
                        // A `propose` step completed — pending intents act
                        // first, then the plan advances.
                        let pending = match state.extension.get(ext::PENDING_INTENTS) {
                            Some(Json::Arr(a)) => a
                                .iter()
                                .filter_map(|i| i.as_str().map(String::from))
                                .collect::<Vec<String>>(),
                            _ => vec![],
                        };
                        if *stop_reason == Gw::ToolUse && !pending.is_empty() {
                            Self::set_ext(state, ext::PENDING_INTENTS, Json::Arr(vec![]));
                            ControlDecision {
                                stamp: stamp_for(state, DecisionPoint::Act, None),
                                kind: DecisionKind::Act {
                                    intents: pending
                                        .iter()
                                        .map(|tc| {
                                            Json::obj([("tool_call_id", Json::str(tc))])
                                        })
                                        .collect(),
                                    mode: ActMode::Sequential,
                                    on_partial: OnPartial::FailBatch,
                                },
                            }
                        } else {
                            self.advance(state)
                        }
                    }
                    _ => self.stop(state, StopReason::FormatFailure { count: 0 }),
                }
            }

            Cue::EffectsSettled {
                submission_ref, ..
            } => {
                if let Some(s) = submission_ref {
                    Self::set_ext(state, ext::SUBMISSION, Json::str(s));
                }
                self.advance(state)
            }

            Cue::VerificationCompleted => self.advance(state),

            // The gate's hold — `require_validator(refs)` actions become the
            // next `verify` decision; `resolve_effect`-only holds re-propose
            // `stop{completed}` (the open effects diverge again — the
            // `reconciliation.holds` budget is the bound, F4).
            Cue::GuardFired { guard_id, .. } if guard_id == "completion_refused" => {
                let refs: Vec<String> = match state.extension.get(ext::REQUIRED_VALIDATORS) {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect(),
                    _ => vec![],
                };
                if !refs.is_empty() {
                    ControlDecision {
                        stamp: stamp_for(state, DecisionPoint::Verify, None),
                        kind: DecisionKind::Verify {
                            validator_refs: refs,
                            subject: Json::str(
                                Self::ext_str(state, ext::PLAN_REF).unwrap_or("plan"),
                            ),
                        },
                    }
                } else {
                    self.stop(state, StopReason::Completed)
                }
            }

            // Everything else parks — a plan_execute run awaiting a cue the
            // plan did not sequence waits rather than improvising (I3
            // totality is still honoured: every cue maps to a row).
            _ => ControlDecision {
                stamp: stamp_for(state, DecisionPoint::Act, None),
                kind: DecisionKind::Wait {
                    until: WaitUntil::CueKind {
                        cue_kind: "guard_fired".into(),
                    },
                },
            },
        };
        state.record_decision(decision.stamp.decision_point, decision.stamp.owner);
        decision
    }

    fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
        FinalReport {
            stop_reason: reason.clone(),
            submission_ref: Self::ext_str(state, ext::SUBMISSION).map(str::to_string),
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
        if j.get("dialect").and_then(Json::as_str).unwrap_or("")
            != crate::state::CONTROL_STATE_DIALECT
        {
            return Err(RestoreError::DialectMismatch {
                checkpoint: j
                    .get("dialect")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
        let state = ControlState::from_json(&j).ok_or(RestoreError::Malformed)?;
        if state.variant_ref != PLAN_EXECUTE_REF {
            return Err(RestoreError::VariantMismatch {
                checkpoint: state.variant_ref,
                restoring: PLAN_EXECUTE_REF.into(),
            });
        }
        Ok(state)
    }
}
