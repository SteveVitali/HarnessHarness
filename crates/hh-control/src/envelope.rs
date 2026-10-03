//! The control envelope (§5e.2; ADR-0106). Core MUST-code interpreting the
//! sealed [`EnvelopePolicy`] at the six guard points — **stop, refuse,
//! delay or tighten only**: it never chooses the next action, never grants,
//! never widens (INV-7; a verdict that would is an
//! [`InvariantId::Inv7`] violation, not a behaviour).
//!
//! The contract rows land here: `arm(run, envelope_policy_ref, lease)` →
//! [`EnvelopeState`] (every counter derived from the ledger); `guard(point,
//! input)` → [`GuardVerdict`] (pure over the durable prefix); `expire` —
//! the kind-fixed terminal + `control.timeout.fired`; `loop_state` /
//! `validation_state` — the materialized views; and the F2 seam
//! `check(d') → admitted | refused{reason}` — the driver-facing view of
//! G-DECIDE on a stamped [`ControlDecision`].

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;

use crate::guards::{GuardContext, GuardInput, GuardVerdict};
use crate::loops::LoopState;
use crate::output::ValidationState;
use crate::policy::{EnvelopePolicy, PolicyError};
use crate::vocab::{ControlDecision, DecisionKind, GuardPoint};

/// `CheckVerdict` — `envelope.check(d') → admitted | refused{reason}` (the
/// F2 seam's driver-facing view: a refused decision returns to β as
/// `envelope_signal{refused{decision_ref, reason}}`).
#[derive(Debug, Clone, PartialEq)]
pub enum CheckVerdict {
    /// The decision is admitted as stamped — `tightened` records the
    /// tightening the envelope applied (a smaller reservation, a deadline —
    /// never a widening, INV-7).
    Admitted {
        /// The tightening record (`{}` when none).
        tightened: Json,
    },
    /// The decision is refused — `reason` is the typed refusal spelling the
    /// driver echoes in `envelope_signal{refused}`.
    Refused {
        /// The refusal reason spelling (e.g. `insufficient_budget{turns}`,
        /// `missing_delegation_reason`, `open_effects`, `barrier_closed`,
        /// `retry_budget_exhausted`, `stop{kind}` for a kernel stop).
        reason: String,
        /// The guard-emitted rows the driver appends *before* the decision
        /// row — a `stop{invariant_violation}` refusal carries the
        /// `control.invariant.violated` evidence rows (the spec order is
        /// violation → stop → quarantine).
        events: Vec<crate::guards::GuardEvent>,
    },
}

/// `EnvelopeState` — `arm`'s product: every counter the envelope reads,
/// derived by folding the durable prefix (never stored state — `loop_state`
/// and `validation_state` are materialized views, ADR-0106 D5).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeState {
    /// The verified policy id (`policy_id` — the arm proved the seal).
    pub policy_id: String,
    /// The folded envelope view.
    pub view: crate::views::EnvelopeView,
    /// The folded loop state.
    pub loop_state: LoopState,
    /// The folded validation state.
    pub validation_state: ValidationState,
    /// Whether the stop barrier is engaged.
    pub barrier_engaged: bool,
}

/// `Envelope` — the armed interpreter (policy + the caller-supplied fold
/// inputs). `guard` is pure: two evaluations at the same `seq` yield the
/// same verdict (AC-F2-10).
pub struct Envelope {
    /// The sealed policy (verified at `arm`).
    pub policy: EnvelopePolicy,
}

/// `arm`'s typed refusals (`PolicyUnsealed`, `Fenced` is the lease check's —
/// the driver surfaces it before `arm` is called).
#[derive(Debug, Clone, PartialEq)]
pub enum ArmError {
    /// The policy's `policy_id` does not match its sealed body.
    PolicyUnsealed,
    /// The policy record is invalid.
    Invalid(PolicyError),
    /// `repair_then_strict` is declared — the C0 interpreter refuses the C1
    /// mode at arm (never silently repairs).
    C1ModeDeclared,
}

impl std::fmt::Display for ArmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArmError::PolicyUnsealed => write!(f, "policy_unsealed"),
            ArmError::Invalid(e) => write!(f, "invalid_policy{{{e}}}"),
            ArmError::C1ModeDeclared => write!(f, "c1_mode_declared{{repair_then_strict}}"),
        }
    }
}

impl std::error::Error for ArmError {}

impl Envelope {
    /// `arm(run, envelope_policy_ref, lease)` → `EnvelopeState` — verifies
    /// the seal (`PolicyUnsealed`), refuses a declared C1 mode the C0
    /// interpreter does not run, and folds every counter from the durable
    /// prefix.
    pub fn arm(
        policy: EnvelopePolicy,
        events: &[EventEnvelope],
    ) -> Result<(Envelope, EnvelopeState), ArmError> {
        if !policy.verify_seal() {
            return Err(ArmError::PolicyUnsealed);
        }
        if policy.output_validation.mode == crate::policy::ValidationMode::RepairThenStrict {
            return Err(ArmError::C1ModeDeclared);
        }
        let env = Envelope { policy };
        let state = EnvelopeState {
            policy_id: env.policy.policy_id.clone(),
            view: crate::views::fold_envelope_view(events),
            loop_state: crate::loops::fold(events),
            validation_state: crate::output::fold(events),
            barrier_engaged: crate::stop::barrier_engaged(events),
        };
        Ok((env, state))
    }

    /// `guard(run, lease, point, input)` → `GuardVerdict` — pure over the
    /// durable prefix.
    pub fn guard(
        &self,
        events: &[EventEnvelope],
        point: GuardPoint,
        input: &GuardInput,
        ctx: &GuardContext,
    ) -> GuardVerdict {
        crate::guards::guard(events, &self.policy, point, input, ctx)
    }

    /// `check(d')` — the F2 seam: G-DECIDE applied to a stamped decision
    /// (`envelope.check(d') → admitted | refused{reason}`). The
    /// `submission_present` flag is the driver's `stop_rule` reading.
    pub fn check(
        &self,
        events: &[EventEnvelope],
        decision: &ControlDecision,
        ctx: &GuardContext,
        submission_present: bool,
    ) -> CheckVerdict {
        // Decision-shape refusals that precede the guard fold —
        // `delegate{owner: model}` without `delegation_reason` is refused
        // `MissingDelegationReason` (ADR-0186 D4); a `delegate` without the
        // R-2.6.3 capability is `DelegationUnavailable`; `act` while
        // effects are open is refused unless `parallel_effects` (I4 — the
        // strategy contract makes it impossible; the envelope refuses a
        // hand-made row anyway).
        match &decision.kind {
            DecisionKind::Delegate {
                delegation_reason, ..
            } => {
                // ADR-0186 D4 — `delegation_reason` is mandatory for
                // `owner ∈ {code, model}` (a `model`-owned reason is a
                // `model_claim` at `delegate`; a `code`-owned reason is
                // the declared label — both are still data).
                if matches!(
                    decision.stamp.owner,
                    hh_ontology::control::Owner::Model | hh_ontology::control::Owner::Code
                ) && delegation_reason.is_none()
                {
                    return CheckVerdict::Refused {
                        reason: "missing_delegation_reason".into(),
                        events: vec![],
                    };
                }
                // T0 — a `delegate` without the R-2.6.3 capability bound
                // is `DelegationUnavailable`, never a silent no-op.
                if !ctx.delegation_available {
                    return CheckVerdict::Refused {
                        reason: "delegation_unavailable".into(),
                        events: vec![],
                    };
                }
            }
            DecisionKind::Act { .. } => {
                if !crate::views::fold_envelope_view(events)
                    .open_effects
                    .is_empty()
                {
                    return CheckVerdict::Refused {
                        reason: "open_effects".into(),
                        events: vec![],
                    };
                }
            }
            _ => {}
        }
        // The barrier — only `stop`/`wait` decisions pass once it engages
        // (drain operations ride the drain path, not `decide`).
        if crate::stop::barrier_engaged(events)
            && !matches!(
                decision.kind,
                DecisionKind::Stop { .. } | DecisionKind::Wait { .. }
            )
        {
            return CheckVerdict::Refused {
                reason: "barrier_closed".into(),
                events: vec![],
            };
        }
        let verdict = self.guard(
            events,
            GuardPoint::Decide,
            &GuardInput::Decide {
                proposed: Some(decision.clone()),
                submission_present,
            },
            ctx,
        );
        match verdict {
            GuardVerdict::Pass { .. } => CheckVerdict::Admitted {
                tightened: Json::obj([]),
            },
            GuardVerdict::Respond {
                observation,
                events,
                ..
            } => CheckVerdict::Refused {
                reason: observation
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("refused")
                    .to_string(),
                events,
            },
            GuardVerdict::Stop { reason, events } => {
                // A kernel stop rule converted the decision — the driver
                // runs the stop protocol with `reason`; the decision row
                // records `decider: envelope`, preceded by the guard's
                // evidence rows (`control.invariant.violated`, …).
                CheckVerdict::Refused {
                    reason: format!("stop{{{}}}", reason.to_json().to_canonical_string()),
                    events,
                }
            }
        }
    }

    /// `expire(run, lease, scope_id)` — the kind-fixed terminal plus
    /// `control.timeout.fired` (the payload is a [`GuardEvent`] the driver
    /// appends in the scope).
    pub fn expire(
        &self,
        events: &[EventEnvelope],
        kind: crate::vocab::ScopeKind,
        scope_id: &str,
        read_only: bool,
        ctx: &GuardContext,
    ) -> Vec<crate::guards::GuardEvent> {
        let d = ctx.deadlines.get(scope_id).copied().unwrap_or(ctx.now_ms);
        let hard = self
            .policy
            .timeouts
            .spec(kind)
            .map(|s| s.hard_max_ms)
            .unwrap_or(0);
        let terminal = crate::retry::expiry_terminal(&self.policy.timeouts, kind, read_only);
        let evs = vec![crate::guards::GuardEvent {
            class: "control.timeout.fired".into(),
            payload: crate::events::timeout_fired_payload(
                kind,
                scope_id,
                d,
                hard,
                terminal.as_str(),
                "expire",
            ),
            scope_id: Some(scope_id.into()),
        }];
        let _ = events;
        evs
    }

    /// `loop_state(run)` — the materialized detector view.
    pub fn loop_state(events: &[EventEnvelope]) -> LoopState {
        crate::loops::fold(events)
    }

    /// `validation_state(run)` — the materialized validation view.
    pub fn validation_state(events: &[EventEnvelope]) -> ValidationState {
        crate::output::fold(events)
    }

    /// `schedule_retry` — delegates to [`crate::retry::schedule_retry`]
    /// (the caller supplies `retries_used`/`retries_ceiling` from the
    /// budget fold — INV-6's single counter).
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_retry(
        &self,
        events: &[EventEnvelope],
        kind: crate::vocab::ScopeKind,
        scope_id: &str,
        error_class: &str,
        now_ms: u64,
        retry_after_ms: Option<u64>,
        attempt_delta: Option<crate::vocab::DeltaKind>,
        retries_used: u64,
        retries_ceiling: u64,
    ) -> crate::retry::RetryOutcome {
        crate::retry::schedule_retry(
            events,
            &self.policy.retry,
            kind,
            scope_id,
            error_class,
            now_ms,
            retry_after_ms,
            attempt_delta,
            retries_used,
            retries_ceiling,
        )
    }
}

/// `stop(run, reason)` — the stop protocol's assessment
/// ([`crate::stop::assess_drain`]); `DrainTimeout` ⇒ `unknown` recorded and
/// `infrastructure_failure{drain_timeout}` with the original reason in
/// `causes[]`.
pub use crate::stop::{assess_drain, DrainReport};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::EnvelopePolicy;
    use crate::vocab::{ActMode, DecisionKind, DecisionStamp, OnPartial};
    use hh_ontology::control::StopReason;
    use hh_ontology::control::{DecisionPoint, Owner};

    fn sealed() -> EnvelopePolicy {
        EnvelopePolicy::stage1_default("b-1").seal().unwrap()
    }

    fn decision(kind: DecisionKind) -> ControlDecision {
        ControlDecision {
            stamp: DecisionStamp {
                decision_point: DecisionPoint::Act,
                owner: Owner::Model,
                rationale_ref: None,
            },
            kind,
        }
    }

    #[test]
    fn arm_refuses_an_unsealed_policy() {
        let mut p = EnvelopePolicy::stage1_default("b-1");
        p.policy_id = "forged".into();
        assert!(matches!(
            Envelope::arm(p, &[]),
            Err(ArmError::PolicyUnsealed)
        ));
    }

    #[test]
    fn arm_refuses_a_tampered_sealed_policy() {
        let mut p = sealed();
        p.loop_policy.window_events = 1; // tamper — the seal no longer binds
        assert!(matches!(
            Envelope::arm(p, &[]),
            Err(ArmError::PolicyUnsealed)
        ));
    }

    #[test]
    fn check_refuses_a_model_delegate_without_delegation_reason() {
        let (env, _st) = Envelope::arm(sealed(), &[]).unwrap();
        let d = decision(DecisionKind::Delegate {
            spec: Json::Null,
            budget_slice: Json::Null,
            permissions: Json::Null,
            delegation_reason: None,
        });
        match env.check(&[], &d, &GuardContext::default(), false) {
            CheckVerdict::Refused { reason, .. } => {
                assert_eq!(reason, "missing_delegation_reason")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn check_admits_a_conforming_decision() {
        let (env, _st) = Envelope::arm(sealed(), &[]).unwrap();
        let d = decision(DecisionKind::Act {
            intents: vec![],
            mode: ActMode::Sequential,
            on_partial: OnPartial::FailBatch,
        });
        match env.check(&[], &d, &GuardContext::default(), false) {
            CheckVerdict::Admitted { .. } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_violation_becomes_a_stop_verdict_with_the_event_row() {
        // INV-3 — a commit after the barrier.
        use hh_ledger::classes::Durability;
        use hh_ledger::event::{EventPlane, Producer, Scope};
        use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};
        let ev = |seq: u64, class: &str| EventEnvelope {
            event_id: format!("e{seq}"),
            run_id: "r".into(),
            seq,
            ts: "t".into(),
            hlc: None,
            plane: EventPlane::Action,
            class: class.into(),
            schema_version: 1,
            producer: Producer::kernel("t"),
            participant_class: ParticipantClass::Native,
            observability_level: [ObservabilityLevel::Events].into_iter().collect(),
            durability: Durability::Ledger,
            scope: Scope {
                turn_id: None,
                model_call_id: None,
                tool_call_id: None,
                effect_id: None,
                child_run_id: None,
                component_call_id: None,
                branch_id: None,
            },
            lease_generation: 1,
            parent_event_id: "root".into(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: Default::default(),
            provenance: None,
            payload: if class == "control.decision" {
                Json::obj([("kind", Json::str("stop"))])
            } else {
                Json::obj([("attempt_no", Json::Int(1))])
            },
            prev_hash: "h".into(),
            hash: "h".into(),
        };
        let events = vec![ev(0, "control.decision"), ev(1, "action.effect.committed")];
        let (env, _st) = Envelope::arm(sealed(), &[]).unwrap();
        let v = env.guard(
            &events,
            GuardPoint::PostEffect,
            &GuardInput::PostEffect {
                scope_kind: crate::vocab::ScopeKind::ToolAttempt,
                scope_id: "e1".into(),
                terminal: "committed".into(),
            },
            &GuardContext::default(),
        );
        match v {
            GuardVerdict::Stop { reason, events } => {
                assert!(matches!(reason, StopReason::InvariantViolation { .. }));
                assert_eq!(events[0].class, "control.invariant.violated");
            }
            other => panic!("{other:?}"),
        }
    }
}
