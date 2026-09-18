//! The control boundary β and the closed decision-point set 𝒟 (spec §2.5.5; ADR-0012 D6,
//! ADR-0048 rule 10, ADR-0103, ADR-0106).
//!
//! β : 𝒟 → {code, model, human} with per-decision-point guards; a typed record on
//! `AgentProcess.native` (§2.5.5). Stage-1 scope: β is **representable** (this record and the
//! closed decision-point sum); β becomes *variable* only at Stage 3. The envelope-reserved
//! points are always `code`; a contrary assignment is refused with [`BoundaryError`]
//! (`IncompatibleBoundary`) at `validate`/`open` (§2.5.5; ADR-0103 I5/I6).

use std::collections::BTreeMap;

use hh_wire::Json;

use crate::dimensions::DimensionId;

/// The closed decision-point set 𝒟 (§2.5.5; ADR-0048 rule 10, CF-107). Extended only by a
/// dialect bump (`route`/`effort` scheduled for HIR/2, OQ-425).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DecisionPoint {
    /// plan
    Plan,
    /// act
    Act,
    /// retrieve
    Retrieve,
    /// compact
    Compact,
    /// verify
    Verify,
    /// delegate
    Delegate,
    /// authorize — envelope-reserved: always `code`.
    Authorize,
    /// retry — infrastructure retry is envelope-reserved: always `code`.
    Retry,
    /// stop — `stop{budget_exhausted|context_exhausted|cancelled}` is always `code`.
    Stop,
    /// escalate
    Escalate,
}

impl DecisionPoint {
    /// The full closed set (ten points).
    pub const ALL: [DecisionPoint; 10] = [
        DecisionPoint::Plan,
        DecisionPoint::Act,
        DecisionPoint::Retrieve,
        DecisionPoint::Compact,
        DecisionPoint::Verify,
        DecisionPoint::Delegate,
        DecisionPoint::Authorize,
        DecisionPoint::Retry,
        DecisionPoint::Stop,
        DecisionPoint::Escalate,
    ];

    /// Whether this point is envelope-reserved to `code` *at the point
    /// granularity* (§2.5.5; ADR-0103 I5/I6). Only `authorize` is
    /// unconditionally code-owned: every `authorize` decision is the
    /// envelope's, so a `model`/`human` assignment is refused
    /// `IncompatibleBoundary`. `stop` and `retry` are *reason*-scoped
    /// reservations — `stop{budget_exhausted|context_exhausted|cancelled}`
    /// and the retry of infrastructure errors are always `code`, but the
    /// strategy legitimately owns `stop{completed|format_failure|…}` and
    /// `retry{pause_turn, target: model_call}`; the envelope enforces its
    /// reserved reasons through its own guards/`check` (the stop decision
    /// row is written with `decider: envelope`), so the *point-level*
    /// assignment may name `model`.
    pub fn is_envelope_reserved_code(self) -> bool {
        matches!(self, DecisionPoint::Authorize)
    }
}

/// The owner a decision point may be assigned to (§2.5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    /// Deterministic code.
    Code,
    /// The model.
    Model,
    /// A human.
    Human,
}

/// `IncompatibleBoundary` — a `ControlBoundary` assigns an envelope-reserved point to a non-code
/// owner (§2.5.5; §2.9.6; ADR-0103 I5/I6). Raised by `validate`/`open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryError {
    /// The reserved point that was assigned to a non-code owner.
    pub point: DecisionPoint,
    /// The offending owner.
    pub owner: Owner,
}

/// `ControlBoundary{assignments, guards}` — a typed record on `AgentProcess.native` (§2.5.5;
/// §2.9.4). `guards` are predicates over ledger-derived state (represented here as opaque
/// predicate refs; their evaluation is Stage 2+).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlBoundary {
    /// The per-decision-point owner assignment (β itself).
    pub assignments: BTreeMap<DecisionPoint, Owner>,
    /// The per-decision-point guard predicate references.
    pub guards: BTreeMap<DecisionPoint, String>,
}

impl ControlBoundary {
    /// Validate the boundary (`validate`/`open`): every envelope-reserved point must be `code`,
    /// else [`BoundaryError`] (`IncompatibleBoundary`). Unassigned points default to `code`.
    pub fn validate(&self) -> Result<(), BoundaryError> {
        for (point, owner) in &self.assignments {
            if point.is_envelope_reserved_code() && *owner != Owner::Code {
                return Err(BoundaryError {
                    point: *point,
                    owner: *owner,
                });
            }
        }
        Ok(())
    }

    /// The effective owner of a point — the assignment, or `code` by default.
    pub fn owner_of(&self, point: DecisionPoint) -> Owner {
        self.assignments.get(&point).copied().unwrap_or(Owner::Code)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The control-plane `StopReason` sum (spec §5e.2 data model; ADR-0106 D6 —
// "owned here"). Distinct from the *gateway* `StopReason` (R-2.3.1, ADR-0119
// D1) that lives in `hh_gateway::vocab` and spells the model-boundary
// `{end_turn, tool_use, …}` sum — no cue or envelope record carries a third
// spelling, and neither sum is read against the other.
// ─────────────────────────────────────────────────────────────────────────────

/// The `StopReason` tag sum — the closed nine-member kind index (§5e.2; ADR-0106
/// D6). `RuntimePlan/1` `stop-rule` nodes name a `StopKind`; the payload-carrying
/// form is [`StopReason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StopKind {
    /// `completed`
    Completed,
    /// `budget_exhausted{budget_id, dimension}`
    BudgetExhausted,
    /// `context_exhausted{required_tokens, cap}`
    ContextExhausted,
    /// `loop_detected{detector, pattern}`
    LoopDetected,
    /// `format_failure{count}`
    FormatFailure,
    /// `invariant_violation{invariant_id}`
    InvariantViolation,
    /// `refused{blocking_effect_id}`
    Refused,
    /// `cancelled{by}`
    Cancelled,
    /// `infrastructure_failure{error_class}`
    InfrastructureFailure,
}

impl StopKind {
    /// The full closed set (nine kinds).
    pub const ALL: [StopKind; 9] = [
        StopKind::Completed,
        StopKind::BudgetExhausted,
        StopKind::ContextExhausted,
        StopKind::LoopDetected,
        StopKind::FormatFailure,
        StopKind::InvariantViolation,
        StopKind::Refused,
        StopKind::Cancelled,
        StopKind::InfrastructureFailure,
    ];

    /// The canonical spelling (HIR/1 dialect).
    pub fn as_str(self) -> &'static str {
        match self {
            StopKind::Completed => "completed",
            StopKind::BudgetExhausted => "budget_exhausted",
            StopKind::ContextExhausted => "context_exhausted",
            StopKind::LoopDetected => "loop_detected",
            StopKind::FormatFailure => "format_failure",
            StopKind::InvariantViolation => "invariant_violation",
            StopKind::Refused => "refused",
            StopKind::Cancelled => "cancelled",
            StopKind::InfrastructureFailure => "infrastructure_failure",
        }
    }

    /// Parse a canonical spelling; `None` on any other input (closed sum —
    /// unknown spellings are never coerced).
    pub fn parse(s: &str) -> Option<StopKind> {
        Some(match s {
            "completed" => StopKind::Completed,
            "budget_exhausted" => StopKind::BudgetExhausted,
            "context_exhausted" => StopKind::ContextExhausted,
            "loop_detected" => StopKind::LoopDetected,
            "format_failure" => StopKind::FormatFailure,
            "invariant_violation" => StopKind::InvariantViolation,
            "refused" => StopKind::Refused,
            "cancelled" => StopKind::Cancelled,
            "infrastructure_failure" => StopKind::InfrastructureFailure,
            _ => return None,
        })
    }
}

/// `cancelled{by ∈ {principal, parent, hosting, operator}}` — who initiated the
/// cancel (§5e.2). Distinct from `hh_env::observe::CancelBy` (the effect-level
/// `cancelled{by}` of `ErrorClass`), which spells a different closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CancelledBy {
    /// The run's principal (human interrupt / steer).
    Principal,
    /// The parent run (subagent drain).
    Parent,
    /// The hosting boundary (Hosting ABI cancel).
    Hosting,
    /// An operator action outside the run.
    Operator,
}

impl CancelledBy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CancelledBy::Principal => "principal",
            CancelledBy::Parent => "parent",
            CancelledBy::Hosting => "hosting",
            CancelledBy::Operator => "operator",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CancelledBy> {
        Some(match s {
            "principal" => CancelledBy::Principal,
            "parent" => CancelledBy::Parent,
            "hosting" => CancelledBy::Hosting,
            "operator" => CancelledBy::Operator,
            _ => return None,
        })
    }
}

/// The ADR-0045 outcome classes — the total projection target of [`StopReason`]
/// (§5e.2; ADR-0106 D7: `loop_detected`/`format_failure` are `scored`
/// stratifiers — the end-state oracle scores whatever exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutcomeClass {
    /// `scored` — the run reached a scoreable end state.
    Scored,
    /// `budget_exhausted` — a hard ceiling or the context cap ended the run.
    BudgetExhausted,
    /// `refused` — a blocking refusal ended the run.
    Refused,
    /// `cancelled` — the run was cancelled.
    Cancelled,
    /// `infrastructure_failure` — an invariant violation or infrastructure
    /// error ended the run (never scored).
    InfrastructureFailure,
}

impl OutcomeClass {
    /// The full closed set (five classes).
    pub const ALL: [OutcomeClass; 5] = [
        OutcomeClass::Scored,
        OutcomeClass::BudgetExhausted,
        OutcomeClass::Refused,
        OutcomeClass::Cancelled,
        OutcomeClass::InfrastructureFailure,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeClass::Scored => "scored",
            OutcomeClass::BudgetExhausted => "budget_exhausted",
            OutcomeClass::Refused => "refused",
            OutcomeClass::Cancelled => "cancelled",
            OutcomeClass::InfrastructureFailure => "infrastructure_failure",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<OutcomeClass> {
        Some(match s {
            "scored" => OutcomeClass::Scored,
            "budget_exhausted" => OutcomeClass::BudgetExhausted,
            "refused" => OutcomeClass::Refused,
            "cancelled" => OutcomeClass::Cancelled,
            "infrastructure_failure" => OutcomeClass::InfrastructureFailure,
            _ => return None,
        })
    }
}

/// The `loop_detector` variant classes (§5e.2 `LoopPolicy`; ADR-0108 D1).
/// `content_chant` (OQ-264) and `judged` are declared members admitted at C1 —
/// the C0 detectors are the four deterministic variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LoopDetectorKind {
    /// Same `loop_key` repeated past `cycle_max`/`threshold`.
    ExactRepeat,
    /// Same `loop_key` ∧ same `Observation.version_id` past threshold.
    NoProgress,
    /// Consecutive error-class terminals past threshold.
    ErrorStreak,
    /// Consecutive model turns with no `act` past threshold.
    Monologue,
    /// `content_chant` (C1; OQ-264 — declared member, not admitted at Stage 1).
    ContentChant,
    /// `judged` (C1; a `Validator{kind: judge}` — never sole grounds for stop).
    Judged,
}

impl LoopDetectorKind {
    /// The full closed set (six detector classes; two C1-admitted).
    pub const ALL: [LoopDetectorKind; 6] = [
        LoopDetectorKind::ExactRepeat,
        LoopDetectorKind::NoProgress,
        LoopDetectorKind::ErrorStreak,
        LoopDetectorKind::Monologue,
        LoopDetectorKind::ContentChant,
        LoopDetectorKind::Judged,
    ];

    /// Whether this detector variant is admitted at C0/Stage 1.
    pub fn admitted_at_stage1(self) -> bool {
        !matches!(
            self,
            LoopDetectorKind::ContentChant | LoopDetectorKind::Judged
        )
    }

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LoopDetectorKind::ExactRepeat => "exact_repeat",
            LoopDetectorKind::NoProgress => "no_progress",
            LoopDetectorKind::ErrorStreak => "error_streak",
            LoopDetectorKind::Monologue => "monologue",
            LoopDetectorKind::ContentChant => "content_chant",
            LoopDetectorKind::Judged => "judged",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<LoopDetectorKind> {
        Some(match s {
            "exact_repeat" => LoopDetectorKind::ExactRepeat,
            "no_progress" => LoopDetectorKind::NoProgress,
            "error_streak" => LoopDetectorKind::ErrorStreak,
            "monologue" => LoopDetectorKind::Monologue,
            "content_chant" => LoopDetectorKind::ContentChant,
            "judged" => LoopDetectorKind::Judged,
            _ => return None,
        })
    }
}

/// `loop_detected{pattern}` — the detection payload (`control.loop.detected`'s
/// `pattern{cycle_len, repeats, loop_keys[]}` member; §5e.2 ledger row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopPattern {
    /// The detected cycle length (`1` for a single repeated key; `2…5` for
    /// alternating cycles under `exact_repeat.cycle_max`).
    pub cycle_len: u32,
    /// How many times the cycle repeated.
    pub repeats: u32,
    /// The `loop_key`s forming the cycle (content hashes, never surface names —
    /// T-LCD-10).
    pub loop_keys: Vec<String>,
}

/// The mandatory state-invariant set INV-1…9 plus declared `ext.*` predicates
/// (§5e.2 `InvariantSet` — "INV-1…9 mandatory; `ext.*` predicates may be added,
/// never removed"; ADR-0108 D3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InvariantId {
    /// INV-1 — every opened scope closes by exactly one terminal by deadline.
    Inv1,
    /// INV-2 — no dangling intent.
    Inv2,
    /// INV-3 — stop is a barrier.
    Inv3,
    /// INV-4 — conservation (child consumed + reserved ≤ ancestor ceiling).
    Inv4,
    /// INV-5 — attempt identity (monotone `attempt_no`; one `committed` per
    /// `(effect_id, attempt_no)`; idempotency-key invariance).
    Inv5,
    /// INV-6 — every feedback path is bounded.
    Inv6,
    /// INV-7 — the envelope never widens.
    Inv7,
    /// INV-8 — no automatic redispatch of `irreversible`; no redispatch of
    /// `unknown` without probe/idempotent class.
    Inv8,
    /// INV-9 — rebuild equality at turn boundaries (`effect_ledger`, budget
    /// `remaining`).
    Inv9,
    /// A declared extension predicate (`ext.<name>` — added, never a removal of
    /// the mandatory set).
    Ext(String),
}

impl InvariantId {
    /// The canonical spelling (`inv-1`…`inv-9`, `ext.<name>`).
    pub fn as_str(&self) -> String {
        match self {
            InvariantId::Inv1 => "inv-1".into(),
            InvariantId::Inv2 => "inv-2".into(),
            InvariantId::Inv3 => "inv-3".into(),
            InvariantId::Inv4 => "inv-4".into(),
            InvariantId::Inv5 => "inv-5".into(),
            InvariantId::Inv6 => "inv-6".into(),
            InvariantId::Inv7 => "inv-7".into(),
            InvariantId::Inv8 => "inv-8".into(),
            InvariantId::Inv9 => "inv-9".into(),
            InvariantId::Ext(name) => format!("ext.{name}"),
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<InvariantId> {
        Some(match s {
            "inv-1" => InvariantId::Inv1,
            "inv-2" => InvariantId::Inv2,
            "inv-3" => InvariantId::Inv3,
            "inv-4" => InvariantId::Inv4,
            "inv-5" => InvariantId::Inv5,
            "inv-6" => InvariantId::Inv6,
            "inv-7" => InvariantId::Inv7,
            "inv-8" => InvariantId::Inv8,
            "inv-9" => InvariantId::Inv9,
            _ => {
                let name = s.strip_prefix("ext.")?;
                if name.is_empty() {
                    return None;
                }
                InvariantId::Ext(name.into())
            }
        })
    }
}

/// The four kernel-internal `infrastructure_failure{error_class}` causes this
/// document fixes (§5e.2; CF-479): values of the `error_class` member, never
/// `StopReason` members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KernelInfraCause {
    /// `drain_timeout` — the drain never completed (ADR-0106 D8).
    DrainTimeout,
    /// `child_unresponsive` — a child ignored its cancel (ADR-0187 C-2).
    ChildUnresponsive,
    /// `environment_lost` — `MaxHealsExceeded` (ADR-0132 D2 as amended, CF-479).
    EnvironmentLost,
    /// `escalation_unresolved` — the reconciler's `final = stop_activation`
    /// (ADR-0206 D1 as amended, CF-479).
    EscalationUnresolved,
}

impl KernelInfraCause {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            KernelInfraCause::DrainTimeout => "drain_timeout",
            KernelInfraCause::ChildUnresponsive => "child_unresponsive",
            KernelInfraCause::EnvironmentLost => "environment_lost",
            KernelInfraCause::EscalationUnresolved => "escalation_unresolved",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<KernelInfraCause> {
        Some(match s {
            "drain_timeout" => KernelInfraCause::DrainTimeout,
            "child_unresponsive" => KernelInfraCause::ChildUnresponsive,
            "environment_lost" => KernelInfraCause::EnvironmentLost,
            "escalation_unresolved" => KernelInfraCause::EscalationUnresolved,
            _ => return None,
        })
    }
}

/// Which of the CF-470 sums (or the kernel-internal set) an
/// `infrastructure_failure{error_class}` value is drawn from. The typed sums
/// live in `hh_gateway::vocab::ModelErrorClass` and `hh_env::observe::ErrorClass`
/// — downstream of this crate — so the value is carried as
/// `{family, class}` and validated against the registered sums by the control
/// crate (the only constructor surface for typed values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InfraErrorFamily {
    /// A `ModelErrorClass` spelling (§05b.1; ADR-0119 D2).
    Model,
    /// An `ErrorClass` spelling (§05d.5; ADR-0087 D4 / ADR-0102 D2).
    Env,
    /// A [`KernelInfraCause`] spelling (CF-479).
    Kernel,
}

impl InfraErrorFamily {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            InfraErrorFamily::Model => "model_error",
            InfraErrorFamily::Env => "env_error",
            InfraErrorFamily::Kernel => "kernel_internal",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<InfraErrorFamily> {
        Some(match s {
            "model_error" => InfraErrorFamily::Model,
            "env_error" => InfraErrorFamily::Env,
            "kernel_internal" => InfraErrorFamily::Kernel,
            _ => return None,
        })
    }
}

/// `infrastructure_failure{error_class}` — a value of the two CF-470 sums or a
/// [`KernelInfraCause`], carried as `{family, class}` where `class` is the
/// spelling's canonical form (validated against the registered sum at
/// construction in `hh-control`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InfraError {
    /// The sum this value is drawn from.
    pub family: InfraErrorFamily,
    /// The canonical spelling within that sum.
    pub class: String,
}

/// `StopReason` — the closed nine-member control-plane sum (§5e.2; ADR-0106 D6;
/// "owned here"). Serializes as `{kind: <StopKind spelling>, …payload}` — the
/// `kind` member is the [`StopKind`] tag; the remaining members are the
/// payload. `from_json` refuses an unknown `kind` or missing/mistyped payload
/// members (closed sum — no coercion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// `completed` — β's completion proposal was admitted.
    Completed,
    /// `budget_exhausted{budget_id, dimension}` — a hard ceiling fired (covers
    /// `turns`, `model_calls`, `retries`, `time.*`, `spend`,
    /// `approvals.requested` — CF-224).
    BudgetExhausted {
        /// The exhausted budget node.
        budget_id: String,
        /// The exhausted dimension.
        dimension: DimensionId,
    },
    /// `context_exhausted{required_tokens, cap}` — occupancy cap reached with
    /// no legal compaction (CF-168).
    ContextExhausted {
        /// The tokens the next step required.
        required_tokens: u64,
        /// The context cap.
        cap: u64,
    },
    /// `loop_detected{detector, pattern}` — a deterministic `loop_detector`
    /// variant reached the `stop` rung.
    LoopDetected {
        /// The detector variant.
        detector: LoopDetectorKind,
        /// The detected pattern.
        pattern: LoopPattern,
    },
    /// `format_failure{count}` — `max_format_failures` reached under strict
    /// output validation.
    FormatFailure {
        /// The running format-failure count at stop.
        count: u32,
    },
    /// `invariant_violation{invariant_id}` — an INV-1…9 (or `ext.*`) predicate
    /// fired; outcome `infrastructure_failure`, quarantine checkpoint.
    InvariantViolation {
        /// The violated invariant.
        invariant_id: InvariantId,
    },
    /// `refused{blocking_effect_id}` — a blocking refusal ended the run.
    Refused {
        /// The effect whose refusal blocked progress.
        blocking_effect_id: String,
    },
    /// `cancelled{by}` — cancelled through a declared channel.
    Cancelled {
        /// Who cancelled.
        by: CancelledBy,
    },
    /// `infrastructure_failure{error_class}` — an infrastructure error ended
    /// the run (a CF-470 `ModelErrorClass`/`ErrorClass` value or a
    /// [`KernelInfraCause`] — CF-479).
    InfrastructureFailure {
        /// The error-class value.
        error_class: InfraError,
    },
}

impl StopReason {
    /// The [`StopKind`] tag of this reason.
    pub fn kind(&self) -> StopKind {
        match self {
            StopReason::Completed => StopKind::Completed,
            StopReason::BudgetExhausted { .. } => StopKind::BudgetExhausted,
            StopReason::ContextExhausted { .. } => StopKind::ContextExhausted,
            StopReason::LoopDetected { .. } => StopKind::LoopDetected,
            StopReason::FormatFailure { .. } => StopKind::FormatFailure,
            StopReason::InvariantViolation { .. } => StopKind::InvariantViolation,
            StopReason::Refused { .. } => StopKind::Refused,
            StopReason::Cancelled { .. } => StopKind::Cancelled,
            StopReason::InfrastructureFailure { .. } => StopKind::InfrastructureFailure,
        }
    }

    /// The total outcome-class projection (§5e.2; ADR-0106 D7):
    /// `completed | loop_detected | format_failure → scored`;
    /// `budget_exhausted | context_exhausted → budget_exhausted`;
    /// `refused → refused`; `cancelled → cancelled`;
    /// `invariant_violation | infrastructure_failure → infrastructure_failure`.
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            StopReason::Completed
            | StopReason::LoopDetected { .. }
            | StopReason::FormatFailure { .. } => OutcomeClass::Scored,
            StopReason::BudgetExhausted { .. } | StopReason::ContextExhausted { .. } => {
                OutcomeClass::BudgetExhausted
            }
            StopReason::Refused { .. } => OutcomeClass::Refused,
            StopReason::Cancelled { .. } => OutcomeClass::Cancelled,
            StopReason::InvariantViolation { .. } | StopReason::InfrastructureFailure { .. } => {
                OutcomeClass::InfrastructureFailure
            }
        }
    }

    /// The canonical JSON form `{kind, …payload}` (omitted members never
    /// appear; `dimension` spells the `DimensionId` canonical name).
    pub fn to_json(&self) -> Json {
        let mut m = vec![("kind", Json::str(self.kind().as_str()))];
        match self {
            StopReason::Completed => {}
            StopReason::BudgetExhausted {
                budget_id,
                dimension,
            } => {
                m.push(("budget_id", Json::str(budget_id)));
                m.push(("dimension", Json::str(dimension.as_str())));
            }
            StopReason::ContextExhausted {
                required_tokens,
                cap,
            } => {
                m.push(("required_tokens", Json::Int(*required_tokens as i64)));
                m.push(("cap", Json::Int(*cap as i64)));
            }
            StopReason::LoopDetected { detector, pattern } => {
                m.push(("detector", Json::str(detector.as_str())));
                m.push((
                    "pattern",
                    Json::obj([
                        ("cycle_len", Json::Int(pattern.cycle_len as i64)),
                        ("repeats", Json::Int(pattern.repeats as i64)),
                        (
                            "loop_keys",
                            Json::Arr(pattern.loop_keys.iter().map(Json::str).collect()),
                        ),
                    ]),
                ));
            }
            StopReason::FormatFailure { count } => {
                m.push(("count", Json::Int(*count as i64)));
            }
            StopReason::InvariantViolation { invariant_id } => {
                m.push(("invariant_id", Json::str(invariant_id.as_str())));
            }
            StopReason::Refused { blocking_effect_id } => {
                m.push(("blocking_effect_id", Json::str(blocking_effect_id)));
            }
            StopReason::Cancelled { by } => {
                m.push(("by", Json::str(by.as_str())));
            }
            StopReason::InfrastructureFailure { error_class } => {
                m.push((
                    "error_class",
                    Json::obj([
                        ("family", Json::str(error_class.family.as_str())),
                        ("class", Json::str(&error_class.class)),
                    ]),
                ));
            }
        }
        Json::obj(m)
    }

    /// Parse the canonical form; `None` on an unknown `kind` or a
    /// missing/mistyped payload member.
    pub fn from_json(j: &Json) -> Option<StopReason> {
        let kind = StopKind::parse(j.get("kind")?.as_str()?)?;
        Some(match kind {
            StopKind::Completed => StopReason::Completed,
            StopKind::BudgetExhausted => StopReason::BudgetExhausted {
                budget_id: j.get("budget_id")?.as_str()?.into(),
                dimension: DimensionId::parse(j.get("dimension")?.as_str()?)?,
            },
            StopKind::ContextExhausted => StopReason::ContextExhausted {
                required_tokens: nonneg(j.get("required_tokens")?)?,
                cap: nonneg(j.get("cap")?)?,
            },
            StopKind::LoopDetected => {
                let p = j.get("pattern")?;
                StopReason::LoopDetected {
                    detector: LoopDetectorKind::parse(j.get("detector")?.as_str()?)?,
                    pattern: LoopPattern {
                        cycle_len: nonneg(p.get("cycle_len")?)? as u32,
                        repeats: nonneg(p.get("repeats")?)? as u32,
                        loop_keys: match p.get("loop_keys")? {
                            Json::Arr(items) => items
                                .iter()
                                .map(|k| k.as_str().map(String::from))
                                .collect::<Option<Vec<_>>>()?,
                            _ => return None,
                        },
                    },
                }
            }
            StopKind::FormatFailure => StopReason::FormatFailure {
                count: nonneg(j.get("count")?)? as u32,
            },
            StopKind::InvariantViolation => StopReason::InvariantViolation {
                invariant_id: InvariantId::parse(j.get("invariant_id")?.as_str()?)?,
            },
            StopKind::Refused => StopReason::Refused {
                blocking_effect_id: j.get("blocking_effect_id")?.as_str()?.into(),
            },
            StopKind::Cancelled => StopReason::Cancelled {
                by: CancelledBy::parse(j.get("by")?.as_str()?)?,
            },
            StopKind::InfrastructureFailure => {
                let e = j.get("error_class")?;
                StopReason::InfrastructureFailure {
                    error_class: InfraError {
                        family: InfraErrorFamily::parse(e.get("family")?.as_str()?)?,
                        class: e.get("class")?.as_str()?.into(),
                    },
                }
            }
        })
    }
}

/// A non-negative JSON integer as `u64`.
fn nonneg(j: &Json) -> Option<u64> {
    let n = j.as_int()?;
    if n < 0 {
        return None;
    }
    Some(n as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_point_set_is_closed_at_ten() {
        assert_eq!(DecisionPoint::ALL.len(), 10);
    }

    #[test]
    fn a_wellformed_boundary_validates() {
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Plan, Owner::Model);
        b.assignments.insert(DecisionPoint::Act, Owner::Model);
        b.assignments.insert(DecisionPoint::Authorize, Owner::Code);
        assert!(b.validate().is_ok());
        // β is representable: an unassigned point defaults to code.
        assert_eq!(b.owner_of(DecisionPoint::Stop), Owner::Code);
    }

    #[test]
    fn authorize_is_always_code() {
        // §2.5.5: authorize is envelope-reserved; a model assignment is IncompatibleBoundary.
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Authorize, Owner::Model);
        assert_eq!(
            b.validate(),
            Err(BoundaryError {
                point: DecisionPoint::Authorize,
                owner: Owner::Model,
            })
        );
    }

    #[test]
    fn authorize_is_reserved_code_stop_and_retry_are_reason_scoped() {
        // `authorize` is unconditionally code (I5) — a contrary assignment
        // is `IncompatibleBoundary`. `stop`/`retry` reservations are
        // reason-scoped (`stop{budget_exhausted|context_exhausted|cancelled}`,
        // infrastructure retry) and enforced by the envelope itself — the
        // strategy legitimately owns `stop{completed}` (I6, F1's decision
        // table), so a `model`/`human` point assignment validates.
        assert!(DecisionPoint::Authorize.is_envelope_reserved_code());
        assert!(!DecisionPoint::Stop.is_envelope_reserved_code());
        assert!(!DecisionPoint::Retry.is_envelope_reserved_code());
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Authorize, Owner::Human);
        assert!(b.validate().is_err());
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Stop, Owner::Model);
        b.assignments.insert(DecisionPoint::Retry, Owner::Code);
        assert!(b.validate().is_ok());
    }

    // ── StopReason (§5e.2; ADR-0106 D6) ──────────────────────────────────────

    #[test]
    fn stop_reason_is_a_closed_nine_member_sum() {
        assert_eq!(StopKind::ALL.len(), 9);
        for k in StopKind::ALL {
            assert_eq!(StopKind::parse(k.as_str()), Some(k));
        }
        // The gateway's spellings are a different sum — never parse here.
        assert_eq!(StopKind::parse("end_turn"), None);
        assert_eq!(StopKind::parse("approvals_exhausted"), None); // CF-224 alias, not a member
    }

    #[test]
    fn outcome_class_projection_is_total_and_matches_the_spec_table() {
        use crate::dimensions::DimensionId;
        let cases = [
            (StopReason::Completed, OutcomeClass::Scored),
            (
                StopReason::LoopDetected {
                    detector: LoopDetectorKind::ExactRepeat,
                    pattern: LoopPattern {
                        cycle_len: 1,
                        repeats: 5,
                        loop_keys: vec!["k".into()],
                    },
                },
                OutcomeClass::Scored,
            ),
            (StopReason::FormatFailure { count: 3 }, OutcomeClass::Scored),
            (
                StopReason::BudgetExhausted {
                    budget_id: "b".into(),
                    dimension: DimensionId::Turns,
                },
                OutcomeClass::BudgetExhausted,
            ),
            (
                StopReason::ContextExhausted {
                    required_tokens: 10,
                    cap: 8,
                },
                OutcomeClass::BudgetExhausted,
            ),
            (
                StopReason::Refused {
                    blocking_effect_id: "e".into(),
                },
                OutcomeClass::Refused,
            ),
            (
                StopReason::Cancelled {
                    by: CancelledBy::Principal,
                },
                OutcomeClass::Cancelled,
            ),
            (
                StopReason::InvariantViolation {
                    invariant_id: InvariantId::Inv3,
                },
                OutcomeClass::InfrastructureFailure,
            ),
            (
                StopReason::InfrastructureFailure {
                    error_class: InfraError {
                        family: InfraErrorFamily::Kernel,
                        class: KernelInfraCause::DrainTimeout.as_str().into(),
                    },
                },
                OutcomeClass::InfrastructureFailure,
            ),
        ];
        assert_eq!(cases.len(), 9); // one per member — total.
        for (reason, want) in cases {
            assert_eq!(reason.outcome_class(), want, "{reason:?}");
        }
    }

    #[test]
    fn stop_reason_json_round_trips_and_refuses_unknown() {
        use crate::dimensions::DimensionId;
        let reasons = vec![
            StopReason::Completed,
            StopReason::BudgetExhausted {
                budget_id: "b-1".into(),
                dimension: DimensionId::Retries,
            },
            StopReason::ContextExhausted {
                required_tokens: 4096,
                cap: 4000,
            },
            StopReason::LoopDetected {
                detector: LoopDetectorKind::NoProgress,
                pattern: LoopPattern {
                    cycle_len: 2,
                    repeats: 3,
                    loop_keys: vec!["ka".into(), "kb".into()],
                },
            },
            StopReason::FormatFailure { count: 3 },
            StopReason::InvariantViolation {
                invariant_id: InvariantId::Ext("debt_clock".into()),
            },
            StopReason::Refused {
                blocking_effect_id: "e-7".into(),
            },
            StopReason::Cancelled {
                by: CancelledBy::Hosting,
            },
            StopReason::InfrastructureFailure {
                error_class: InfraError {
                    family: InfraErrorFamily::Env,
                    class: "timeout".into(),
                },
            },
        ];
        for r in &reasons {
            assert_eq!(StopReason::from_json(&r.to_json()).as_ref(), Some(r));
        }
        assert_eq!(
            StopReason::from_json(&Json::obj([("kind", Json::str("timeout"))])),
            None
        );
        // Missing payload member refuses.
        assert_eq!(
            StopReason::from_json(&Json::obj([("kind", Json::str("cancelled"))])),
            None
        );
    }

    #[test]
    fn kernel_infra_causes_are_values_never_stop_reasons() {
        // CF-479: the four kernel-internal causes are `error_class` values of
        // `infrastructure_failure`, not `StopReason` members.
        for c in [
            KernelInfraCause::DrainTimeout,
            KernelInfraCause::ChildUnresponsive,
            KernelInfraCause::EnvironmentLost,
            KernelInfraCause::EscalationUnresolved,
        ] {
            assert_eq!(StopKind::parse(c.as_str()), None);
            assert_eq!(KernelInfraCause::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn c1_detector_variants_are_declared_not_admitted() {
        assert!(!LoopDetectorKind::Judged.admitted_at_stage1());
        assert!(!LoopDetectorKind::ContentChant.admitted_at_stage1());
        assert!(LoopDetectorKind::ExactRepeat.admitted_at_stage1());
        assert!(LoopDetectorKind::NoProgress.admitted_at_stage1());
    }
}
