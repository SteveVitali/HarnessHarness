//! `task` — the §5c.5 `subagent_task` DelegateSpec constructor (ticket
//! S4.16b; AC-R-2.4.5-8): `Procedure` + bound parameters + the parent's
//! delegation inputs → the canonical [`SubagentSpec`] the compiler's
//! `delegate{spec, budget, permission}` plan node carries and
//! [`crate::spawn`] consumes.
//!
//! The spec's four cross-layer obligations land here as data + guards:
//!
//! - **`Permission ⊆` parent** — `allowed_effects` (the caller-flattened
//!   declared effect sets of `Procedure.allowed_capabilities` — the doc
//!   belongs to the caller, the spec is data here) must each be covered by a
//!   `requested_grants` member (`Grant.effect.covers(e)`); the constructor
//!   refuses [`TaskSpecError::UncoveredCapability`] otherwise — never a
//!   silent widen or a silent drop (CC2/CC3). Π's `delegable(parent)`
//!   attenuation still binds at `spawn` — the spec asks; `spawn` confers.
//! - **`Budget ≤` parent** — the caller supplies `budget_spec`; `spawn`'s
//!   `allocate` is the enforcement point (a delegate node that exceeds the
//!   parent pool refuses there, never a second scheme here).
//! - **Fresh context** — `isolation.context = fresh`, `memory_namespace =
//!   child` (the canonical members [`SubagentSpec::to_json`] stamps).
//! - **Result authority `≤ delegate`** — the child's ceiling is `delegate`
//!   unconditionally; `result::integrate` stamps `derived_from.kind =
//!   subagent_result` on the re-entered artifact (SP-1/M-1, already landed).
//!
//! `goal.statement` is a harness-minted `Text` leaf naming the procedure and
//! its bound parameters (canonical-JSON rendering — deterministic, and the
//! kernel never *reads* procedure prose; the `Text` leaf in the procedure
//! record travels content-addressed through `supplies.procedures`).

use hh_budget::BudgetMode;
use hh_env::handle::OnParentEnd;
use hh_hir::kinds::EffectClass;
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant};
use hh_hir::Ref;
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::types::{
    ChildProcess, ChildRole, ContextIsolation, DelegationReason, EnvIsolation, ReturnContract,
    SubagentSpec, Supplies, WaitMode,
};
use hh_budget::spec::BudgetSpec;

/// The caller-supplied members of the `subagent_task` spec — everything the
/// constructor cannot derive from the pinned refs themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSpecInputs {
    /// The child's native process — `native{harness_def}` (a pinned ref;
    /// semantic equality with the parent's definition is T3-legal).
    pub child_definition: Ref,
    /// The procedure ref the child's definition binds — carried in
    /// `supplies.procedures` and `goal.success_criteria` so the child
    /// resolves the same sealed version (§3.1.3's `Validator|Procedure`
    /// criterion slot admits it).
    pub procedure: Ref,
    /// The procedure's bound parameters (rendered into `goal.statement`,
    /// never into context).
    pub parameters: Json,
    /// The flattened declared effect sets of
    /// `Procedure.allowed_capabilities` — the caller resolves the
    /// `ToolCapability` refs (the sealed doc is the caller's; the spec is
    /// data here). Empty ⇒ the procedure invokes nothing and no grant is
    /// demanded.
    pub allowed_effects: Vec<EffectClass>,
    /// The grants the parent delegates for this task — must *cover* every
    /// `allowed_effects` member (`Grant.effect.covers(e)`); `spawn`
    /// attenuates to `delegable(parent)` on top (Π ⊆ — never widened).
    pub requested_grants: Vec<Grant>,
    /// The child's budget node spec (≤ parent — enforced at `spawn`'s
    /// `allocate`).
    pub budget_spec: BudgetSpec,
    /// The child budget node's ref — `GoalRecord.budget` and the delegate
    /// node's `budget` coordinate.
    pub budget_ref: Ref,
    /// The parent goal ref — `GoalRecord.parent` (lineage, CC3).
    pub parent_goal: Option<Ref>,
    /// How the parent waits (`await` | `background`).
    pub wait_mode: WaitMode,
    /// The `subagent` scope deadline bound (ms).
    pub wait_timeout_ms: u64,
    /// The return contract the child's envelope enforces at its `stop`.
    pub return_contract: ReturnContract,
    /// `cancel | detach_to_child` — the parent's terminal policy.
    pub on_parent_end: OnParentEnd,
    /// `isolation.environment` (`none` when no derived handle is needed).
    pub environment: EnvIsolation,
}

/// The `subagent_task` spec-construction failure modes — typed, never a
/// silent drop or widen (§5c.5's `Permission ⊆` is a guard, not a hope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSpecError {
    /// A declared capability effect is covered by no requested grant — the
    /// child may not demand what the parent did not delegate.
    UncoveredCapability {
        /// The uncovered effect (canonical spelling).
        effect: String,
    },
}

impl std::fmt::Display for TaskSpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskSpecError::UncoveredCapability { effect } => write!(
                f,
                "subagent_task: no delegated grant covers effect {effect}"
            ),
        }
    }
}

impl std::error::Error for TaskSpecError {}

/// `spec_for_procedure` — assemble the canonical `SubagentSpec` for a
/// `subagent_task`-targeted `Procedure` (§5c.5; AC-R-2.4.5-8). `created_at`
/// is the minting seq (ledger/logical time — never a wall clock).
///
/// The check that runs here (once, at spec minting): every flattened
/// `allowed_effects` member is covered by ≥1 `requested_grants` member — the
/// parent never silently under-delegates (`UncoveredCapability`) and the
/// child never asks above what was conferred (CC2's subset obligation,
/// checked at the seam where the spec is data).
pub fn spec_for_procedure(
    inputs: &TaskSpecInputs,
    created_at: u64,
) -> Result<SubagentSpec, TaskSpecError> {
    for effect in &inputs.allowed_effects {
        if !inputs
            .requested_grants
            .iter()
            .any(|g| g.effect.covers(effect))
        {
            return Err(TaskSpecError::UncoveredCapability {
                effect: effect.to_json().to_canonical_string(),
            });
        }
    }
    // The task statement names the procedure + the bound parameters — the
    // kernel never reads a `Text` leaf (SP-3); the child resolves the
    // procedure's prose through `supplies.procedures` under its own
    // profile.
    let statement = Text::new(
        format!(
            "subagent_task:{} params:{}",
            inputs.procedure.semantic_id,
            inputs.parameters.to_canonical_string()
        ),
        "hh-subagent.task",
        ProvenanceRecord::kernel("hh-subagent.task", created_at),
    );
    Ok(SubagentSpec {
        process: ChildProcess::Native {
            harness_def: inputs.child_definition.clone(),
            profile_binding: None, // the parent's subagent role (ADR-0121/0124)
            control_strategy: None,
            slots: None,
        },
        goal: GoalRecord {
            statement,
            success_criteria: vec![inputs.procedure.clone()],
            unverifiable_reason: None,
            budget: inputs.budget_ref.clone(),
            origin: GoalOrigin::Delegated,
            parent: inputs.parent_goal.clone(),
        },
        role: ChildRole::Subagent,
        requested_grants: inputs.requested_grants.clone(),
        ceiling: Some(AuthorityClass::Delegate),
        budget_spec: inputs.budget_spec.clone(),
        budget_mode: match inputs.budget_spec.mode {
            BudgetMode::Slice => BudgetMode::Slice,
            other => other,
        },
        context: ContextIsolation::Fresh,
        environment: inputs.environment.clone(),
        supplies: Supplies {
            procedures: vec![inputs.procedure.clone()],
            ..Supplies::default()
        },
        return_contract: inputs.return_contract.clone(),
        wait_mode: inputs.wait_mode,
        wait_timeout_ms: inputs.wait_timeout_ms,
        on_parent_end: inputs.on_parent_end,
        delegation_reason: DelegationReason::Specialization,
        topology_ref: None,
        stage_index: None,
        ownership_grants: Vec::new(),
        reserved_keys: Vec::new(),
        consistency_declarations: Vec::new(),
        merge_policy_ref: None,
        messaging_policy: None,
    })
}
