//! The value-of-compute scheduler — spec §5e.4 (R-2.6.4; ADR-0188/0189/0190).
//!
//! `compute_policy` is the P3 component class bound in
//! `AgentProcess.native.slots["compute_policy"]` and invoked by the driver
//! **between `decide` and `envelope.check`**. It *binds* parameters the
//! strategy left unbound, *tightens* bound ones, or advises model-owned
//! points through a measured artifact — it never proposes, refuses, grants,
//! widens or owns β (ADR-0188 D1–D2).
//!
//! The packaged default is `static`: binds nothing, runs no estimator and
//! appends no event — every earlier stage's ledger is byte-identical without
//! it (AC-F4-1). `uniform` binds every feasible option at its declared
//! maximum (the recipe baseline — ADR-0190 D1). `rules` is the C3 estimator
//! baseline: five conditioned rules over typed inputs only, **no model
//! calls** (ADR-0189 D3). `bandit`/`surface_prior`/`predictor` are registered
//! class variants whose implementations land at Stage 5/6 — `policy_for`
//! refuses them `PolicyInvalid{variant_not_admitted}` at bind-time.
//!
//! **Invariants B-1…B-6 (MUST-code — [`check_bound`] enforces them):**
//! B-1 kind/decision_point/owner unchanged; B-2 every bound parameter was
//! unbound or is tightened (`slice′ ≤ slice`, `k′ ≤ k`, `effort′ ≤ effort`,
//! a step-up admissible only as a declared rule on a recorded failure
//! streak); B-3 `bind` never produces a binding the envelope would refuse —
//! each option's reserve-sized estimate is checked against `remaining`
//! before it is emitted; never calls `amend`, never touches `authorize`,
//! never converts `ask → allow`; B-4 every supported option appears in
//! `options_considered[]` with an estimate or a typed infeasibility; B-5 no
//! `Text` leaf, no `model_claim` as sole basis (typed ctx members only);
//! B-6 `bind` is pure over `(decision, ctx)` — equal inputs yield equal
//! `d′` and equal records modulo allocated ids.

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::quantity::ResourceVector;
use hh_ontology::control::DecisionPoint;
use hh_wire::json::Json;

use crate::vocab::{ControlDecision, DecisionKind};

// ─────────────────────────────────────────────────────────────────────────────
// Closed vocabularies
// ─────────────────────────────────────────────────────────────────────────────

/// `ComputeOption` kinds (closed; extension by dialect bump — ADR-0188 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComputeOptionKind {
    /// `continue_as_configured` — the always-admissible baseline.
    ContinueAsConfigured,
    /// `spawn_subagent{spec_ref, slice}` — binds `delegate.budget_slice`
    /// (+ `spec_ref`, + an unbound `delegation_reason` from the declared
    /// reason); `infeasible{undeclared_parallelism}` without a declared
    /// `parallel` step or `subagent_task` target.
    SpawnSubagent,
    /// `extend_search{kind}` — binds an ensemble member count `≤ k_max`.
    ExtendSearch,
    /// `evaluator{placement_ref}` — binds `verify.validator_refs` to
    /// declared placements with `active` calibration only.
    Evaluator,
    /// `effort_level{level}` — sets the call's effort rung; a step-up is
    /// admissible only as a declared rule on a recorded failure streak.
    EffortLevel,
    /// `alternate_role{role, quality_target?}` — sets role/quality target
    /// on a validator-fail streak with a valid `QualityPrior`.
    AlternateRole,
    /// `stop_now{proposed_reason}` — never admissible from stability alone
    /// where a deterministic validator exists (§5e.4 failure table).
    StopNow,
}

impl ComputeOptionKind {
    /// The closed set (canonical order — `options_considered[]` ordering).
    pub const ALL: [ComputeOptionKind; 7] = [
        ComputeOptionKind::ContinueAsConfigured,
        ComputeOptionKind::SpawnSubagent,
        ComputeOptionKind::ExtendSearch,
        ComputeOptionKind::Evaluator,
        ComputeOptionKind::EffortLevel,
        ComputeOptionKind::AlternateRole,
        ComputeOptionKind::StopNow,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ComputeOptionKind::ContinueAsConfigured => "continue_as_configured",
            ComputeOptionKind::SpawnSubagent => "spawn_subagent",
            ComputeOptionKind::ExtendSearch => "extend_search",
            ComputeOptionKind::Evaluator => "evaluator",
            ComputeOptionKind::EffortLevel => "effort_level",
            ComputeOptionKind::AlternateRole => "alternate_role",
            ComputeOptionKind::StopNow => "stop_now",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ComputeOptionKind> {
        ComputeOptionKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
    }
}

/// `extend_search.kind` (closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendSearchKind {
    /// One more ensemble member.
    AdditionalSample,
    /// One more speculation branch.
    AdditionalBranch,
    /// Re-drive the task.
    RetryTask,
    /// Continue generation.
    ContinueGeneration,
}

impl ExtendSearchKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ExtendSearchKind::AdditionalSample => "additional_sample",
            ExtendSearchKind::AdditionalBranch => "additional_branch",
            ExtendSearchKind::RetryTask => "retry_task",
            ExtendSearchKind::ContinueGeneration => "continue_generation",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ExtendSearchKind> {
        Some(match s {
            "additional_sample" => ExtendSearchKind::AdditionalSample,
            "additional_branch" => ExtendSearchKind::AdditionalBranch,
            "retry_task" => ExtendSearchKind::RetryTask,
            "continue_generation" => ExtendSearchKind::ContinueGeneration,
            _ => return None,
        })
    }
}

/// The typed infeasibility sum (B-4's closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeInfeasible {
    /// A gauge cap (`fan_out`, `delegation_depth`) is at its ceiling.
    GaugeAtCap,
    /// The reserve-sized cost exceeds `remaining`.
    InsufficientBudget,
    /// No declared `parallel` step or `subagent_task` target.
    UndeclaredParallelism,
    /// No critic placement is declared in the sealed definition.
    NoPlacement,
    /// The placement's `CalibrationRecord` is not `active` (AC-F4-8).
    CalibrationInactive,
    /// The profile does not declare the required capability (e.g. `effort`).
    CapabilityUnmet,
    /// The required prior cell is `unknown`/expired/absent.
    PriorUnknown,
    /// The policy excludes this option (e.g. `rules` never `stop_now`; an
    /// option whose owning decision point does not match).
    PolicyExcluded,
}

impl ComputeInfeasible {
    /// The closed set.
    pub const ALL: [ComputeInfeasible; 8] = [
        ComputeInfeasible::GaugeAtCap,
        ComputeInfeasible::InsufficientBudget,
        ComputeInfeasible::UndeclaredParallelism,
        ComputeInfeasible::NoPlacement,
        ComputeInfeasible::CalibrationInactive,
        ComputeInfeasible::CapabilityUnmet,
        ComputeInfeasible::PriorUnknown,
        ComputeInfeasible::PolicyExcluded,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ComputeInfeasible::GaugeAtCap => "gauge_at_cap",
            ComputeInfeasible::InsufficientBudget => "insufficient_budget",
            ComputeInfeasible::UndeclaredParallelism => "undeclared_parallelism",
            ComputeInfeasible::NoPlacement => "no_placement",
            ComputeInfeasible::CalibrationInactive => "calibration_inactive",
            ComputeInfeasible::CapabilityUnmet => "capability_unmet",
            ComputeInfeasible::PriorUnknown => "prior_unknown",
            ComputeInfeasible::PolicyExcluded => "policy_excluded",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ComputeInfeasible> {
        ComputeInfeasible::ALL
            .iter()
            .copied()
            .find(|r| r.as_str() == s)
    }
}

/// `bind`'s typed failures (§5e.4 interface contract). `PriorUnknown` and
/// `EstimatorBudgetExhausted` degrade to `Unchanged`, never a stop; the
/// driver treats every `ComputeError` as an inert bind.
#[derive(Debug, Clone, PartialEq)]
pub enum ComputeError {
    /// A policy produced a binding violating B-1/B-2 — `PolicyInvalid` at
    /// link/bind (AC-F4-2).
    PolicyInvalid {
        /// The rule/parameter the violation names.
        rule_id: String,
        /// The detail.
        reason: String,
    },
    /// The required prior cell is unknown — the `rules` fallback fires.
    PriorUnknown {
        /// The cell.
        cell: String,
    },
    /// The estimator's own budget cap is spent — `Unchanged`, never a stop.
    EstimatorBudgetExhausted,
    /// A variant declaring `requires.task_value` ran without one.
    TaskValueMissing,
    /// The variant ref does not name an admitted implementation
    /// (`bandit`/`surface_prior`/`predictor` are registered but Stage 5/6).
    VariantNotAdmitted {
        /// The variant ref.
        variant_ref: String,
    },
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputeError::PolicyInvalid { rule_id, reason } => {
                write!(f, "policy_invalid{{{rule_id}}}: {reason}")
            }
            ComputeError::PriorUnknown { cell } => write!(f, "prior_unknown{{{cell}}}"),
            ComputeError::EstimatorBudgetExhausted => write!(f, "estimator_budget_exhausted"),
            ComputeError::TaskValueMissing => write!(f, "task_value_missing"),
            ComputeError::VariantNotAdmitted { variant_ref } => {
                write!(f, "variant_not_admitted{{{variant_ref}}}")
            }
        }
    }
}

impl std::error::Error for ComputeError {}

// ─────────────────────────────────────────────────────────────────────────────
// The typed ctx (B-5 — every member is data; no Text leaf, no model_claim)
// ─────────────────────────────────────────────────────────────────────────────

/// One observed ensemble sample (`output_hash` + verdict labels — never the
/// output body).
#[derive(Debug, Clone, PartialEq)]
pub struct SampleFact {
    /// The canonical hash of the sample's canonical output.
    pub output_hash: String,
    /// The validator verdicts recorded on it (spellings only).
    pub verdicts: Vec<String>,
}

/// The running validator verdict streaks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ValidatorStreaks {
    /// Consecutive passes.
    pub pass: u32,
    /// Consecutive fails.
    pub fail: u32,
    /// Consecutive format failures.
    pub format_failure: u32,
}

/// A declared critic placement (the ADR-0117 rule's four recorded inputs +
/// calibration status).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementFact {
    /// The critic/placement ref.
    pub critic_ref: String,
    /// The calibration status — `≠ active` ⇒ `infeasible{calibration_inactive}`.
    pub calibration_status: hh_verification::vocab::CalibrationStatus,
    /// `missed_failure_rate` (ppm — the critic misses this share of real failures).
    pub missed_failure_rate_ppm: i64,
    /// `false_stop_rate` (ppm — the critic halts good work this share).
    pub false_stop_rate_ppm: i64,
    /// `harness_overhead.verification` cost attributed to the critic.
    pub verification_overhead: i64,
    /// `cost(uncaught)` — the expected cost of an uncaught failure (from the
    /// task's `reversibility`; a currency amount, never a proxy).
    pub uncaught_cost: i64,
}

/// The declared ensemble shape (`EnsembleProcedure{sample_k_vote, k_max}` —
/// the `extend_search` ceiling and oracle conditionality).
#[derive(Debug, Clone, PartialEq)]
pub struct EnsembleFact {
    /// `k_max` — the declared member-count ceiling.
    pub k_max: u32,
    /// The reducer/validator oracle class spelling (`executable`, `judge`, …)
    /// or `None` — `delta_p_success` for `extend_search` is `unknown` without
    /// one (ADR-0189 D3).
    pub oracle: Option<String>,
}

/// A prior cell fact (`{cell, n, mean, interval, valid_until}` — read-only
/// evidence the `rules` variant records in `priors_used[]`; the online
/// `PriorCell` store itself is Stage 5).
#[derive(Debug, Clone, PartialEq)]
pub struct PriorFact {
    /// The cell key spelling.
    pub cell: String,
    /// The cell's observation count.
    pub n: u32,
    /// The cell mean (ppm).
    pub mean_ppm: i64,
    /// The interval `{lo, hi}` (ppm).
    pub interval: Option<(i64, i64)>,
    /// `valid_until` — an expiry seq; `Some(past)` ⇒ treated `unknown`.
    pub valid_until: Option<u64>,
}

/// The model-plane health view the ctx reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthView {
    /// Nominal.
    Ok,
    /// Degraded — conservative options only.
    Degraded,
}

/// The budget/gauge view `bind` reads (`budget_view{remaining, reserved,
/// fan_out, delegation_depth, context.occupancy}`).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetView {
    /// `remaining` — `ceiling − consumed` over the accounted dimensions.
    pub remaining: ResourceVector,
    /// `reserved` — the reservation total the envelope projects.
    pub reserved: ResourceVector,
    /// Live children (`control.subagent.spawned` minus terminals).
    pub live_fan_out: u32,
    /// The `fan_out` gauge cap.
    pub fan_out_cap: u32,
    /// The current delegation depth.
    pub delegation_depth: u32,
    /// The `delegation_depth` gauge cap.
    pub delegation_depth_cap: u32,
    /// `context.occupancy` (ppm of the window cap).
    pub occupancy_ppm: i64,
}

/// `ComputeContext` — the closed typed input set of §5e.4's `bind`
/// signature. Every member is a typed fact; the scheduler reads **no**
/// `Text` leaf and no `model_claim` (B-5; the type makes the leak
/// impossible — there is no member that can carry prose).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeContext {
    /// The budget/gauge view.
    pub budget: BudgetView,
    /// The declared `parallel` step count in the plan.
    pub declared_parallel_steps: u32,
    /// The declared `subagent_task` targets (spec refs the spawn may bind).
    pub subagent_task_targets: Vec<String>,
    /// The declared ensemble shape, when the definition carries one.
    pub ensemble: Option<EnsembleFact>,
    /// `samples_so_far{output_hash, verdicts[]}`.
    pub samples: Vec<SampleFact>,
    /// `verifier_available` — the oracle class of the available
    /// reducer/validator, `None` = `verifier: none`.
    pub verifier: Option<String>,
    /// The validator verdict streaks.
    pub validator_streaks: ValidatorStreaks,
    /// `profile.capabilities` — the scheduler reads `effort{ladder,
    /// per_message, cache_invalidation_on_change}`; a profile declaring no
    /// `effort` yields `capability_unmet` (§5e.4 data model).
    pub profile_capabilities: Json,
    /// `role_table` — `ModelRole → variant` data for `alternate_role`.
    pub role_table: Json,
    /// The declared critic placements.
    pub placements: Vec<PlacementFact>,
    /// The prior cells read (evidence refs — `rules` records them, never
    /// scores from them).
    pub priors: Vec<PriorFact>,
    /// The ADR-0089 `cost_model` estimate (reserve sizing), when declared.
    pub cost_model: Option<ResourceVector>,
    /// The health view.
    pub health: HealthView,
    /// The `TaskValue` (body from `hh_verification::gate::TaskValue`; the
    /// fact here names the contract ref it was projected from).
    pub task_value: Option<TaskValueFact>,
    /// The `context_label` (the assembled context's provenance label —
    /// carried for the record; bind never branches on it).
    pub context_label: Option<String>,
}

/// The ctx's `TaskValue` view — the sealed contract's `task_value`
/// projection (ADR-0109/§5e.4 `{success_value, failure_cost |
/// from_reversibility, currency}`).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskValueFact {
    /// The `TaskContract` ref this was projected from.
    pub contract_ref: String,
    /// The success value (money-valued in Lab arms; `None` = unscored).
    pub success_value: Option<i64>,
    /// `failure_cost` as a declared amount, or the symbolic
    /// `from_reversibility` class (resolved by [`Self::failure_cost_ppm`]).
    pub failure_cost: FailureCost,
    /// The currency dimension spelling (`spend`, `tokens.*`…).
    pub currency: String,
}

/// `failure_cost` — an amount or the reversibility class it defaults from
/// (§5e.4 data model: "`failure_cost | from_reversibility`").
#[derive(Debug, Clone, PartialEq)]
pub enum FailureCost {
    /// A declared amount in `currency` units.
    Amount(i64),
    /// `from_reversibility{class}` — the class's ordinal weight is the cost
    /// basis (`read_only` 0 … `irreversible` 1.0 in ppm).
    FromReversibility(String),
}

impl FailureCost {
    /// The ppm weight the objective uses (declared amounts are normalised
    /// by [`TaskValueFact::failure_cost_ppm`]).
    pub fn class_ppm(class: &str) -> Option<i64> {
        Some(match class {
            "read_only" => 0,
            "reversible" => 250_000,
            "compensable" => 500_000,
            "irreversible" => 1_000_000,
            _ => return None,
        })
    }
}

impl TaskValueFact {
    /// The failure-cost basis in ppm (a declared amount saturates at the
    /// scale — amounts are relative weights for the objective, never
    /// absolute spend authority).
    pub fn failure_cost_ppm(&self) -> i64 {
        match &self.failure_cost {
            FailureCost::Amount(a) => (*a).clamp(0, 1_000_000),
            FailureCost::FromReversibility(c) => FailureCost::class_ppm(c).unwrap_or(500_000),
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("contract_ref", Json::str(&self.contract_ref)),
            ("currency", Json::str(&self.currency)),
        ];
        match &self.failure_cost {
            FailureCost::Amount(a) => m.push(("failure_cost", Json::Int(*a))),
            FailureCost::FromReversibility(c) => {
                m.push(("from_reversibility", Json::str(c.clone())))
            }
        }
        if let Some(s) = self.success_value {
            m.push(("success_value", Json::Int(s)));
        }
        Json::obj(m)
    }

    /// Parse the contract's `task_value` member.
    pub fn from_json(j: &Json) -> Option<TaskValueFact> {
        let currency = j.get("currency")?.as_str()?.to_string();
        let contract_ref = j
            .get("contract_ref")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let failure_cost = if let Some(a) = j.get("failure_cost").and_then(Json::as_int) {
            FailureCost::Amount(a)
        } else if let Some(c) = j.get("from_reversibility").and_then(Json::as_str) {
            FailureCost::FromReversibility(c.to_string())
        } else {
            return None;
        };
        Some(TaskValueFact {
            contract_ref,
            success_value: j.get("success_value").and_then(Json::as_int),
            failure_cost,
            currency,
        })
    }

    /// Project a sealed `hh_verification::gate::TaskValue` into the ctx
    /// fact — the contract is the authority; `contract_ref` names it.
    pub fn from_task_value(
        contract_ref: &str,
        tv: &hh_verification::gate::TaskValue,
    ) -> TaskValueFact {
        TaskValueFact {
            contract_ref: contract_ref.to_string(),
            success_value: tv.success_value,
            failure_cost: if let Some(a) = tv.failure_cost {
                FailureCost::Amount(a)
            } else {
                FailureCost::FromReversibility(
                    tv.from_reversibility
                        .clone()
                        .unwrap_or_else(|| "irreversible".to_string()),
                )
            },
            currency: tv.currency.clone(),
        }
    }
}

/// `ComputeFacts` — the declared (sealed-at-open) members of
/// [`ComputeContext`] the driver cannot fold from the durable prefix: the
/// definition's ensemble shape and `parallel`/`subagent_task` declarations,
/// the profile's capability map and role table, the critic placements, the
/// prior-cell seeds, the cost model, the `TaskValue`, and the delegation
/// depth this process runs at. Everything the driver *can* fold —
/// `remaining`, gauges, live fan-out, samples, verdict streaks — comes
/// from the ledger, so a resumed driver rebuilds the identical ctx (B-6).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeFacts {
    /// The declared `parallel` step count in the plan.
    pub declared_parallel_steps: u32,
    /// The declared `subagent_task` targets (spec refs the spawn may bind).
    pub subagent_task_targets: Vec<String>,
    /// The declared ensemble shape (`EnsembleProcedure{sample_k_vote,
    /// k_max}` + the reducer/validator oracle class).
    pub ensemble: Option<EnsembleFact>,
    /// The oracle class of the bound verifier, when one is declared.
    pub verifier: Option<String>,
    /// `profile.capabilities` verbatim (the scheduler reads the `effort`
    /// member).
    pub profile_capabilities: Json,
    /// `role_table` — `ModelRole → variant` data for `alternate_role`.
    pub role_table: Json,
    /// The declared critic placements.
    pub placements: Vec<PlacementFact>,
    /// The prior-cell seeds (provider drift / profile supersession resets
    /// them — the driver folds the reset from `model.rerouted` /
    /// `model.profile.expired_used` and lands `control.compute.prior_reset`).
    pub priors: Vec<PriorFact>,
    /// The ADR-0089 `cost_model` estimate, when declared.
    pub cost_model: Option<ResourceVector>,
    /// The `TaskValue` projected from the sealed `TaskContract`.
    pub task_value: Option<TaskValueFact>,
    /// The delegation depth this `AgentProcess` runs at.
    pub delegation_depth: u32,
    /// The assembled-context provenance label (recorded, never branched on).
    pub context_label: Option<String>,
}

impl Default for ComputeFacts {
    fn default() -> Self {
        ComputeFacts {
            declared_parallel_steps: 0,
            subagent_task_targets: vec![],
            ensemble: None,
            verifier: None,
            profile_capabilities: Json::obj([]),
            role_table: Json::obj([]),
            placements: vec![],
            priors: vec![],
            cost_model: None,
            task_value: None,
            delegation_depth: 0,
            context_label: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Estimates (ADR-0189 D2 — `MarginalValueEstimate` + the derived `RiskEstimate`)
// ─────────────────────────────────────────────────────────────────────────────

/// `delta_p_success.source` (closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaPSource {
    /// A conditioned rule.
    Rule,
    /// A prior cell.
    Prior,
    /// The online estimator.
    Online,
    /// Unestimable — never ranks above `continue_as_configured`.
    Unknown,
}

impl DeltaPSource {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeltaPSource::Rule => "rule",
            DeltaPSource::Prior => "prior",
            DeltaPSource::Online => "online",
            DeltaPSource::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<DeltaPSource> {
        Some(match s {
            "rule" => DeltaPSource::Rule,
            "prior" => DeltaPSource::Prior,
            "online" => DeltaPSource::Online,
            "unknown" => DeltaPSource::Unknown,
            _ => return None,
        })
    }
}

/// `delta_p_success{point, interval, source}` — ppm deltas.
#[derive(Debug, Clone, PartialEq)]
pub struct DeltaP {
    /// The point estimate (ppm); `None` on an `unknown` source.
    pub point_ppm: Option<i64>,
    /// The `{lo, hi}` interval (ppm).
    pub interval: Option<(i64, i64)>,
    /// The estimate's provenance.
    pub source: DeltaPSource,
}

impl DeltaP {
    /// The `unknown` delta — never ranks above `continue_as_configured`.
    pub fn unknown() -> DeltaP {
        DeltaP {
            point_ppm: None,
            interval: None,
            source: DeltaPSource::Unknown,
        }
    }

    /// A rule-sourced point estimate.
    pub fn rule(point_ppm: i64) -> DeltaP {
        DeltaP {
            point_ppm: Some(point_ppm),
            interval: None,
            source: DeltaPSource::Rule,
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![("source", Json::str(self.source.as_str()))];
        if let Some(p) = self.point_ppm {
            m.push(("point_ppm", Json::Int(p)));
        }
        if let Some((lo, hi)) = self.interval {
            m.push((
                "interval",
                Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))]),
            ));
        }
        Json::obj(m)
    }

    /// Parse the canonical JSON.
    pub fn from_json(j: &Json) -> Option<DeltaP> {
        let source = DeltaPSource::parse(j.get("source")?.as_str()?)?;
        let interval = j
            .get("interval")
            .and_then(|i| Some((i.get("lo")?.as_int()?, i.get("hi")?.as_int()?)));
        Some(DeltaP {
            point_ppm: j.get("point_ppm").and_then(Json::as_int),
            interval,
            source,
        })
    }
}

/// `risk.coordination` (closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordination {
    /// No coordination.
    None,
    /// A fan-out.
    FanOut,
    /// A merge.
    Merge,
}

impl Coordination {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Coordination::None => "none",
            Coordination::FanOut => "fan_out",
            Coordination::Merge => "merge",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Coordination> {
        Some(match s {
            "none" => Coordination::None,
            "fan_out" => Coordination::FanOut,
            "merge" => Coordination::Merge,
            _ => return None,
        })
    }
}

/// `RiskEstimate` — **derived**, never declared by the estimator (§5e.4).
#[derive(Debug, Clone, PartialEq)]
pub struct RiskEstimate {
    /// The option's effect classes (ADR-0031).
    pub effect_classes: Vec<String>,
    /// The coordination the option introduces.
    pub coordination: Coordination,
    /// `starvation_p` (ppm) — the reservation refusal rate, when measured.
    pub starvation_p_ppm: Option<i64>,
    /// Whether the option's effort change invalidates the cache (the
    /// profile's `effort.cache_invalidation_on_change` declaration).
    pub cache_invalidation: bool,
}

impl RiskEstimate {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            (
                "effect_classes",
                Json::Arr(self.effect_classes.iter().map(Json::str).collect()),
            ),
            ("coordination", Json::str(self.coordination.as_str())),
            ("cache_invalidation", Json::Bool(self.cache_invalidation)),
        ];
        if let Some(p) = self.starvation_p_ppm {
            m.push(("starvation_p_ppm", Json::Int(p)));
        }
        Json::obj(m)
    }

    /// Parse the canonical JSON.
    pub fn from_json(j: &Json) -> Option<RiskEstimate> {
        let effect_classes = match j.get("effect_classes")? {
            Json::Arr(a) => a
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        Some(RiskEstimate {
            effect_classes,
            coordination: Coordination::parse(j.get("coordination")?.as_str()?)?,
            starvation_p_ppm: j.get("starvation_p_ppm").and_then(Json::as_int),
            cache_invalidation: matches!(j.get("cache_invalidation"), Some(Json::Bool(true))),
        })
    }
}

/// `MarginalValueEstimate` (ADR-0189 D2) — the estimator's per-option
/// output. `expected_cost` is **reserve-sized**: profile `max_output`,
/// capability `cost_model`, child slice.
#[derive(Debug, Clone, PartialEq)]
pub struct MarginalValueEstimate {
    /// `delta_p_success` — `unknown` ranks below `continue_as_configured`.
    pub delta_p: DeltaP,
    /// The reserve-sized expected cost.
    pub expected_cost: ResourceVector,
    /// Expected added latency (ms).
    pub expected_latency_ms: Option<i64>,
    /// The derived risk.
    pub risk: RiskEstimate,
    /// `oracle_conditionality` — `none` ⇒ `delta_p` is `unknown` for
    /// `extend_search` (no reducer/validator to credit).
    pub oracle_conditionality: Option<String>,
    /// Evidence refs (`QualityPrior`, `FittedSurfaceReport`…).
    pub evidence_refs: Vec<String>,
}

impl MarginalValueEstimate {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("delta_p_success", self.delta_p.to_json()),
            ("expected_cost", self.expected_cost.to_json()),
            (
                "expected_latency_ms",
                self.expected_latency_ms
                    .map(Json::Int)
                    .unwrap_or(Json::Null),
            ),
            ("risk", self.risk.to_json()),
            (
                "oracle_conditionality",
                self.oracle_conditionality
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or_else(|| Json::str("none")),
            ),
            (
                "evidence_refs",
                Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Parse the canonical JSON.
    pub fn from_json(j: &Json) -> Option<MarginalValueEstimate> {
        let latency = j.get("expected_latency_ms").and_then(Json::as_int);
        let oracle = j
            .get("oracle_conditionality")
            .and_then(Json::as_str)
            .filter(|s| *s != "none")
            .map(str::to_string);
        Some(MarginalValueEstimate {
            delta_p: DeltaP::from_json(j.get("delta_p_success")?)?,
            expected_cost: ResourceVector::from_json(j.get("expected_cost")?)?,
            expected_latency_ms: latency,
            risk: RiskEstimate::from_json(j.get("risk")?)?,
            oracle_conditionality: oracle,
            evidence_refs: match j.get("evidence_refs") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => vec![],
            },
        })
    }
}

/// `options_considered[]`'s per-option payload — an estimate or a typed
/// infeasibility (B-4).
#[derive(Debug, Clone, PartialEq)]
pub enum OptionEstimate {
    /// A `MarginalValueEstimate`.
    Estimate(MarginalValueEstimate),
    /// A typed infeasibility.
    Infeasible(ComputeInfeasible),
}

/// A `ComputeOption` — `{kind, params…}`; the option *requests*, the owning
/// contract *decides* (CF-403).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeOption {
    /// The option kind.
    pub kind: ComputeOptionKind,
    /// `extend_search.kind`, when `kind = ExtendSearch`.
    pub extend_kind: Option<ExtendSearchKind>,
    /// `spec_ref` (spawn_subagent) / `placement_ref` (evaluator) / `role`
    /// (alternate_role) / `level` (effort_level) — the option's named
    /// target, when one exists.
    pub target: Option<String>,
    /// `quality_target` (alternate_role) / `proposed_reason` (stop_now).
    pub qualifier: Option<String>,
}

impl ComputeOption {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![("kind", Json::str(self.kind.as_str()))];
        if let Some(k) = self.extend_kind {
            m.push(("extend_kind", Json::str(k.as_str())));
        }
        let (tk, qk) = match self.kind {
            ComputeOptionKind::SpawnSubagent => ("spec_ref", ""),
            ComputeOptionKind::Evaluator => ("placement_ref", ""),
            ComputeOptionKind::EffortLevel => ("level", ""),
            ComputeOptionKind::AlternateRole => ("role", "quality_target"),
            ComputeOptionKind::StopNow => ("", "proposed_reason"),
            _ => ("", ""),
        };
        if let Some(t) = &self.target {
            if !tk.is_empty() {
                m.push((tk, Json::str(t)));
            }
        }
        if let Some(q) = &self.qualifier {
            if !qk.is_empty() {
                m.push((qk, Json::str(q)));
            }
        }
        Json::obj(m)
    }

    /// Parse the canonical JSON.
    pub fn from_json(j: &Json) -> Option<ComputeOption> {
        let kind = ComputeOptionKind::parse(j.get("kind")?.as_str()?)?;
        let target_key = match kind {
            ComputeOptionKind::SpawnSubagent => Some("spec_ref"),
            ComputeOptionKind::Evaluator => Some("placement_ref"),
            ComputeOptionKind::EffortLevel => Some("level"),
            ComputeOptionKind::AlternateRole => Some("role"),
            _ => None,
        };
        let qualifier_key = match kind {
            ComputeOptionKind::AlternateRole => Some("quality_target"),
            ComputeOptionKind::StopNow => Some("proposed_reason"),
            _ => None,
        };
        Some(ComputeOption {
            kind,
            extend_kind: j
                .get("extend_kind")
                .and_then(Json::as_str)
                .and_then(ExtendSearchKind::parse),
            target: target_key
                .and_then(|k| j.get(k))
                .and_then(|v| v.as_str().map(str::to_string)),
            qualifier: qualifier_key
                .and_then(|k| j.get(k))
                .and_then(|v| v.as_str().map(str::to_string)),
        })
    }
}

/// One `options_considered[]` row.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionRow {
    /// The option.
    pub option: ComputeOption,
    /// The estimate or typed infeasibility.
    pub estimate: OptionEstimate,
}

// ─────────────────────────────────────────────────────────────────────────────
// `ComputeDecisionRecord` — the `control.compute.decided` payload
// ─────────────────────────────────────────────────────────────────────────────

/// `estimator_ref{variant_ref, version_id}`.
#[derive(Debug, Clone, PartialEq)]
pub struct EstimatorRef {
    /// The `compute_estimator` variant ref.
    pub variant_ref: String,
    /// The pinned version id.
    pub version_id: String,
}

/// `priors_used[]` — one prior cell as read.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorUsed {
    /// The cell key.
    pub cell: String,
    /// The observation count (`0`/`n` post-reset).
    pub n: u32,
    /// The cell mean (ppm); `None` ⇒ `unknown`.
    pub mean_ppm: Option<i64>,
    /// The interval.
    pub interval: Option<(i64, i64)>,
    /// `valid_until`.
    pub valid_until: Option<u64>,
}

/// `RationaleRecord{objective_value_per_option, lambda, task_value_ref}` —
/// `objective = ΔP̂(success) × success_value − λ·cost − P̂(harm)·failure_cost`
/// with `lambda` MUST-data on the arm.
#[derive(Debug, Clone, PartialEq)]
pub struct RationaleRecord {
    /// `{option → objective}` — the scored value in ppm-scaled units; an
    /// `unknown` delta carries no member (it never ranks).
    pub objective_value_per_option: BTreeMap<String, i64>,
    /// `λ` — the cost weight (ppm).
    pub lambda_ppm: i64,
    /// The `TaskValue` ref the objective read (`None` = unscored).
    pub task_value_ref: Option<String>,
}

/// `ComputeDecisionRecord{record_id, decision_id, decision_point,
/// options_considered[], chosen, binding, estimator_ref, rules_fired[],
/// priors_used[], inputs_read[] + inputs_digest, rationale,
/// cost_of_estimation}` — content-addressed; the `control.compute.decided`
/// payload verbatim (§5e.4 data model; CF-401).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeDecisionRecord {
    /// The content-addressed record id (`record:` + identify over the body
    /// minus `record_id`).
    pub record_id: String,
    /// The `control.decision` this record binds/advises.
    pub decision_id: String,
    /// The decision point (must equal the linked decision's — AC-F4-2).
    pub decision_point: DecisionPoint,
    /// `options_considered[]` — every supported option with an estimate or
    /// a typed infeasibility (B-4).
    pub options_considered: Vec<OptionRow>,
    /// The chosen option.
    pub chosen: ComputeOption,
    /// `binding: map<param, value>` — the parameter bindings `d′` carries.
    pub binding: BTreeMap<String, Json>,
    /// The estimator that scored.
    pub estimator_ref: EstimatorRef,
    /// `rules_fired[]` — the conditioned rules whose predicates evaluated
    /// positive (`compute_rule` ids — each carries an `AssumptionDebtRecord`
    /// in its variant row).
    pub rules_fired: Vec<String>,
    /// `priors_used[]` — prior cells read (empty for `rules`; `n = 0` /
    /// `unknown` post-`prior_reset`).
    pub priors_used: Vec<PriorUsed>,
    /// `inputs_read[]` — the ctx member spellings the bind consumed
    /// (provenance-bounded — B-5's evidence).
    pub inputs_read: Vec<String>,
    /// `inputs_digest` — `sha256:` of the ctx's canonical form.
    pub inputs_digest: String,
    /// The rationale.
    pub rationale: RationaleRecord,
    /// `cost_of_estimation` — the estimator's own accounted spend (zero for
    /// `rules`/`static` — no model calls; `harness_overhead.scheduling`
    /// never records a phantom charge).
    pub cost_of_estimation: ResourceVector,
}

impl ComputeDecisionRecord {
    /// The canonical JSON — the `control.compute.decided` payload verbatim.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("record_id", Json::str(&self.record_id)),
            ("decision_id", Json::str(&self.decision_id)),
            (
                "decision_point",
                Json::str(crate::state::decision_point_str(self.decision_point)),
            ),
            (
                "options_considered",
                Json::Arr(
                    self.options_considered
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("option", r.option.to_json()),
                                match &r.estimate {
                                    OptionEstimate::Estimate(e) => ("estimate", e.to_json()),
                                    OptionEstimate::Infeasible(r) => {
                                        ("infeasible", Json::str(r.as_str()))
                                    }
                                },
                            ])
                        })
                        .collect(),
                ),
            ),
            ("chosen", self.chosen.to_json()),
            (
                "binding",
                Json::Obj(
                    self.binding
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ),
            (
                "estimator_ref",
                Json::obj([
                    ("variant_ref", Json::str(&self.estimator_ref.variant_ref)),
                    ("version_id", Json::str(&self.estimator_ref.version_id)),
                ]),
            ),
            (
                "rules_fired",
                Json::Arr(self.rules_fired.iter().map(Json::str).collect()),
            ),
            (
                "priors_used",
                Json::Arr(
                    self.priors_used
                        .iter()
                        .map(|p| {
                            let mut m =
                                vec![("cell", Json::str(&p.cell)), ("n", Json::Int(p.n as i64))];
                            match p.mean_ppm {
                                Some(v) => m.push(("mean", Json::Int(v))),
                                None => m.push(("mean", Json::str("unknown"))),
                            }
                            if let Some((lo, hi)) = p.interval {
                                m.push((
                                    "interval",
                                    Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))]),
                                ));
                            }
                            if let Some(v) = p.valid_until {
                                m.push(("valid_until", Json::Int(v as i64)));
                            }
                            Json::obj(m)
                        })
                        .collect(),
                ),
            ),
            (
                "inputs_read",
                Json::Arr(self.inputs_read.iter().map(Json::str).collect()),
            ),
            ("inputs_digest", Json::str(&self.inputs_digest)),
            (
                "rationale",
                Json::obj([
                    (
                        "objective_value_per_option",
                        Json::Obj(
                            self.rationale
                                .objective_value_per_option
                                .iter()
                                .map(|(k, v)| (k.clone(), Json::Int(*v)))
                                .collect(),
                        ),
                    ),
                    ("lambda_ppm", Json::Int(self.rationale.lambda_ppm)),
                    (
                        "task_value_ref",
                        self.rationale
                            .task_value_ref
                            .as_deref()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                ]),
            ),
            ("cost_of_estimation", self.cost_of_estimation.to_json()),
        ])
    }

    /// The content address — `identify_bytes(ComputeDecisionRecord,
    /// canonical(body minus record_id))`.
    pub fn compute_record_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("record_id");
        }
        hh_identity::idp::identify_bytes(
            hh_identity::kinds::RecordKind::ComputeDecisionRecord,
            j.to_canonical_string().as_bytes(),
        )
    }

    /// Parse the payload (the projection + codec round-trip test).
    pub fn from_json(j: &Json) -> Option<ComputeDecisionRecord> {
        let options = match j.get("options_considered")? {
            Json::Arr(a) => {
                let mut v = Vec::with_capacity(a.len());
                for r in a {
                    let option = ComputeOption::from_json(r.get("option")?)?;
                    let estimate = if let Some(e) = r.get("estimate") {
                        OptionEstimate::Estimate(MarginalValueEstimate::from_json(e)?)
                    } else {
                        OptionEstimate::Infeasible(ComputeInfeasible::parse(
                            r.get("infeasible")?.as_str()?,
                        )?)
                    };
                    v.push(OptionRow { option, estimate });
                }
                v
            }
            _ => return None,
        };
        let priors_used = match j.get("priors_used") {
            Some(Json::Arr(a)) => {
                let mut v = Vec::new();
                for p in a {
                    v.push(PriorUsed {
                        cell: p.get("cell")?.as_str()?.to_string(),
                        n: p.get("n")?.as_int()? as u32,
                        mean_ppm: match p.get("mean") {
                            Some(Json::Int(i)) => Some(*i),
                            _ => None,
                        },
                        interval: p
                            .get("interval")
                            .and_then(|i| Some((i.get("lo")?.as_int()?, i.get("hi")?.as_int()?))),
                        valid_until: p
                            .get("valid_until")
                            .and_then(Json::as_int)
                            .map(|v| v as u64),
                    });
                }
                v
            }
            _ => vec![],
        };
        let binding = match j.get("binding") {
            Some(Json::Obj(m)) => m.clone(),
            _ => BTreeMap::new(),
        };
        let er = j.get("estimator_ref")?;
        let rationale = j.get("rationale")?;
        let objectives = match rationale.get("objective_value_per_option") {
            Some(Json::Obj(m)) => m
                .iter()
                .map(|(k, v)| v.as_int().map(|i| (k.clone(), i)))
                .collect::<Option<BTreeMap<_, _>>>()?,
            _ => BTreeMap::new(),
        };
        Some(ComputeDecisionRecord {
            record_id: j.get("record_id")?.as_str()?.to_string(),
            decision_id: j.get("decision_id")?.as_str()?.to_string(),
            decision_point: crate::state::parse_decision_point(j.get("decision_point")?.as_str()?)?,
            options_considered: options,
            chosen: ComputeOption::from_json(j.get("chosen")?)?,
            binding,
            estimator_ref: EstimatorRef {
                variant_ref: er.get("variant_ref")?.as_str()?.to_string(),
                version_id: er.get("version_id")?.as_str()?.to_string(),
            },
            rules_fired: match j.get("rules_fired") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => vec![],
            },
            priors_used,
            inputs_read: match j.get("inputs_read") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => vec![],
            },
            inputs_digest: j.get("inputs_digest")?.as_str()?.to_string(),
            rationale: RationaleRecord {
                objective_value_per_option: objectives,
                lambda_ppm: rationale.get("lambda_ppm")?.as_int()?,
                task_value_ref: match rationale.get("task_value_ref") {
                    Some(Json::Str(s)) => Some(s.clone()),
                    _ => None,
                },
            },
            cost_of_estimation: ResourceVector::from_json(j.get("cost_of_estimation")?)?,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `bind` — outcome, trait, capabilities
// ─────────────────────────────────────────────────────────────────────────────

/// `ComputeAdvice` — the typed payload `advise` hands to the context
/// builder, lowered there into a `ContextItem{kind: compute_advice}` and
/// rendered through the Model Profile by a `definition`-authority
/// `HarnessRule` (ADR-0188 D5; T-LCD-02). hh-control does not depend on
/// hh-context — the `compute_advice` candidate kind is a dialect-bump
/// member of the closed `CandidateKind` sum (§5c.1) — so advice travels
/// as this typed record. Delivery is measured via
/// `context.artefact.delivered/activated` +
/// `verification.artefact.followed{kind: compute_advice}` (T-LCD-13 →
/// `scheduling.advice_compliance`). Advice **never affects `bind`**.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeAdvice {
    /// The model-owned decision point the advice applies to.
    pub decision_point: DecisionPoint,
    /// The option the advice recommends — structured, never free text
    /// (B-5; the profile's `HarnessRule` renders the surface form).
    pub option: ComputeOption,
    /// The estimate behind the recommendation, when one exists.
    pub estimate: Option<MarginalValueEstimate>,
    /// The conditioned rules that authored the advice (attribution).
    pub rules_fired: Vec<String>,
}

impl ComputeAdvice {
    /// The context-candidate kind — always `compute_advice` (a
    /// dialect-bump member of the closed `CandidateKind` sum — §5c.1).
    pub const KIND: &'static str = "compute_advice";
}

/// `bind`'s outcome (§5e.4 contract).
#[derive(Debug)]
pub enum BindOutcome {
    /// `d′` differs — the record lands `control.compute.decided`.
    Bound {
        /// The bound decision (same kind/point/owner — B-1).
        decision: ControlDecision,
        /// The decision record.
        record: ComputeDecisionRecord,
    },
    /// No binding applied — the record still lands when options were
    /// considered (a considered-options row, never a stop).
    Unchanged {
        /// The decision record.
        record: ComputeDecisionRecord,
    },
}

/// `capabilities(variant)` — the declaration the driver validates at
/// bind-time (`set_compute_policy` — AC-F4-2's link gate).
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyCapabilities {
    /// `options_supported ⊆ ComputeOption`.
    pub options_supported: BTreeSet<ComputeOptionKind>,
    /// `decision_points ⊆ {propose, delegate, verify, retry, stop}` — the
    /// spec's declared *kind-level* spellings (§5e.4; `propose` covers the
    /// `plan`/`act` stamps — the point enum itself stays closed).
    pub decision_points: BTreeSet<String>,
    /// Whether the estimator issues model calls (`rules`/`static`/`uniform`:
    /// `false` — AC-F4-5's zero-model-calls claim).
    pub makes_model_calls: bool,
    /// The `compute_estimator` variant the policy scores with.
    pub estimator_ref: Option<String>,
    /// `requires.task_value` — a missing `TaskValue` yields
    /// `TaskValueMissing` (→ `Unchanged`).
    pub requires_task_value: bool,
    /// `requires.priors`.
    pub requires_priors: bool,
    /// `deterministic` — every admitted C3 variant is (B-6).
    pub deterministic: bool,
}

/// The `compute_policy` contract — `static`/`uniform`/`rules` land at C3;
/// `bandit`/`surface_prior`/`predictor` at C3(S5)/C4 respectively.
pub trait ComputePolicy {
    /// The variant ref (`static`, `uniform`, `rules`, or a registered ref).
    fn variant_ref(&self) -> &str;
    /// `capabilities(variant)`.
    fn capabilities(&self) -> PolicyCapabilities;
    /// `bind(decision, ctx)` — pure over its inputs (B-6).
    fn bind(
        &self,
        decision: &ControlDecision,
        ctx: &ComputeContext,
    ) -> Result<BindOutcome, ComputeError>;
    /// `advise(decision_point, ctx)` — model-owned points only. Returns a
    /// [`ComputeAdvice`] the context builder lowers into
    /// `ContextItem{kind: compute_advice}` (rendered through the Model
    /// Profile by a `definition`-authority `HarnessRule`), or `None`
    /// (ADR-0188 D5). The contract row's error column is `—`: advice may
    /// fail silently to `None`, never to a run-visible error, and never
    /// affects `bind`. Every C3 variant advises nothing — the default is
    /// `None` (`scheduling.advice_compliance` stays `n/a` until an
    /// advising variant lands).
    fn advise(
        &self,
        _decision_point: DecisionPoint,
        _ctx: &ComputeContext,
    ) -> Option<ComputeAdvice> {
        None
    }
    /// `observe(events[])` — durable-ledger-only prior updates (ADR-0189 D4;
    /// `rules` reads no priors — the default is a no-op).
    fn observe(&mut self, _events: &[hh_ledger::event::EventEnvelope]) {}
}

/// `policy_for(variant_ref)` — the link-time resolver (§5e.4: a fixture
/// policy outside the admitted set fails `PolicyInvalid`; `bandit`/
/// `surface_prior`/`predictor` are registered class variants whose
/// implementations land at Stage 5/6 — `VariantNotAdmitted`, never silent).
pub fn policy_for(variant_ref: &str) -> Result<Box<dyn ComputePolicy>, ComputeError> {
    match variant_ref {
        "static" => Ok(Box::new(StaticPolicy)),
        "uniform" => Ok(Box::new(UniformPolicy)),
        "rules" => Ok(Box::new(RulesPolicy::default())),
        other => Err(ComputeError::VariantNotAdmitted {
            variant_ref: other.to_string(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `static` — the packaged null variant (AC-F4-1)
// ─────────────────────────────────────────────────────────────────────────────

/// `static` — binds nothing, runs no estimator, appends no event. The
/// driver never calls `bind` on it (its `capabilities` declare no options),
/// so a `static`-bound run is byte-identical to a run without the slot.
#[derive(Debug, Default)]
pub struct StaticPolicy;

impl ComputePolicy for StaticPolicy {
    fn variant_ref(&self) -> &str {
        "static"
    }

    fn capabilities(&self) -> PolicyCapabilities {
        PolicyCapabilities {
            options_supported: BTreeSet::new(),
            decision_points: BTreeSet::new(),
            makes_model_calls: false,
            estimator_ref: None,
            requires_task_value: false,
            requires_priors: false,
            deterministic: true,
        }
    }

    fn bind(
        &self,
        _decision: &ControlDecision,
        _ctx: &ComputeContext,
    ) -> Result<BindOutcome, ComputeError> {
        // Never invoked — the driver short-circuits on the empty capability
        // set. If it were invoked, `Unchanged` with an empty consideration
        // set is the honest no-op (no event: `options_considered` is empty).
        Ok(BindOutcome::Unchanged {
            record: empty_record(_decision.stamp.decision_point),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `uniform` — the recipe baseline: every feasible option at its declared max
// ─────────────────────────────────────────────────────────────────────────────

/// `uniform` — binds every applicable, feasible option at its configured
/// maximum (`sample_k = k_max`, slice = `remaining ÷ (steps + 1)`, the
/// effort ladder's top rung). The matched-budget comparison's generous
/// baseline (ADR-0190 D1).
#[derive(Debug, Default)]
pub struct UniformPolicy;

impl UniformPolicy {
    /// The maximal binding map for this `(decision, ctx)` — shared with
    /// [`RulesPolicy`] where the shapes agree.
    fn maximal_binding(
        decision: &ControlDecision,
        ctx: &ComputeContext,
    ) -> Result<BTreeMap<String, Json>, ComputeError> {
        let mut b = BTreeMap::new();
        match &decision.kind {
            DecisionKind::Propose { .. } => {
                if let Some(e) = &ctx.ensemble {
                    if reserve_fits(ctx, sample_cost(ctx)) {
                        b.insert("propose.sample_k".into(), Json::Int(e.k_max as i64));
                    }
                }
                if let Some(top) = effort_ladder(ctx).last() {
                    b.insert("propose.effort".into(), Json::str(top.clone()));
                }
            }
            DecisionKind::Delegate { .. } => {
                let steps = ctx.declared_parallel_steps.max(1);
                if parallel_declared(ctx)
                    && ctx.budget.live_fan_out < ctx.budget.fan_out_cap
                    && ctx.budget.delegation_depth < ctx.budget.delegation_depth_cap
                {
                    let slice = scale_vector(&ctx.budget.remaining, steps + 1);
                    if reserve_fits(ctx, slice.clone()) {
                        b.insert("delegate.budget_slice".into(), slice.to_json());
                        if let Some(t) = ctx.subagent_task_targets.first() {
                            b.insert("delegate.spec_ref".into(), Json::str(t.clone()));
                        }
                    }
                }
            }
            DecisionKind::Verify { .. } => {
                let active: Vec<&PlacementFact> = ctx
                    .placements
                    .iter()
                    .filter(|p| {
                        p.calibration_status == hh_verification::vocab::CalibrationStatus::Active
                    })
                    .collect();
                if !active.is_empty() {
                    b.insert(
                        "verify.validator_refs".into(),
                        Json::Arr(
                            active
                                .iter()
                                .map(|p| Json::str(p.critic_ref.clone()))
                                .collect(),
                        ),
                    );
                }
            }
            _ => {}
        }
        Ok(b)
    }
}

impl ComputePolicy for UniformPolicy {
    fn variant_ref(&self) -> &str {
        "uniform"
    }

    fn capabilities(&self) -> PolicyCapabilities {
        PolicyCapabilities {
            options_supported: ComputeOptionKind::ALL.iter().copied().collect(),
            decision_points: all_decision_points(),
            makes_model_calls: false,
            estimator_ref: Some("uniform".into()),
            requires_task_value: false,
            requires_priors: false,
            deterministic: true,
        }
    }

    fn bind(
        &self,
        decision: &ControlDecision,
        ctx: &ComputeContext,
    ) -> Result<BindOutcome, ComputeError> {
        let binding = UniformPolicy::maximal_binding(decision, ctx)?;
        finish_bind(
            decision,
            ctx,
            "uniform",
            &eval_options(decision, ctx, &RulesConfig::default()),
            vec!["uniform.max".into()],
            binding,
            1_000_000,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `rules` — the C3 baseline (ADR-0189 D3: five conditioned rules, no calls)
// ─────────────────────────────────────────────────────────────────────────────

/// `RulesConfig` — the conditioned-rule thresholds (MUST-data with
/// `AssumptionDebtRecord`s; the Stage-4 placeholder set ADR-0189 names:
/// `θ_agree 0.8` over ≥ 3 samples, `k_max 5`, `child_floor 10 %` of
/// `remaining`, streak `n 2`, `λ` ppm).
#[derive(Debug, Clone, PartialEq)]
pub struct RulesConfig {
    /// `θ_agree` — the agreement threshold (ppm).
    pub theta_agree_ppm: i64,
    /// The agreement window (samples considered for θ).
    pub agree_window: u32,
    /// `child_floor` — the minimum slice a spawn needs, as a share of
    /// `remaining` (ppm).
    pub child_floor_ppm: i64,
    /// The failure-streak length that admits an effort step-up / role change.
    pub streak_n: u32,
    /// `λ` — the objective's cost weight (ppm).
    pub lambda_ppm: i64,
}

impl Default for RulesConfig {
    fn default() -> Self {
        RulesConfig {
            theta_agree_ppm: 800_000,
            agree_window: 3,
            child_floor_ppm: 100_000,
            streak_n: 2,
            lambda_ppm: 500_000,
        }
    }
}

/// The AC-F4-10 rule — an evolution-authored `HirDiff` over the `rules`
/// parameters. Raising `child_floor`, `k_max`/`agree_window` spend ceilings
/// or `budget_share_cap` **is** `loosening` and is rejected; lowering
/// `θ_agree`/raising thresholds toward more spend likewise. Tightening
/// direction is per-parameter, declared here — never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDelta {
    /// The change tightens (less spend authority).
    Tightening,
    /// The change is spend-neutral.
    Neutral,
    /// The change loosens (more spend authority) — rejected on an
    /// evolution-authored diff (ADR-0053 D-5; `amend` is absent from the
    /// policy's verb set).
    Loosening,
}

impl BudgetDelta {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetDelta::Tightening => "tightening",
            BudgetDelta::Neutral => "neutral",
            BudgetDelta::Loosening => "loosening",
        }
    }
}

/// `classify_compute_rule_delta(param, old, new)` — the declared direction
/// table for the `rules` parameters (AC-F4-10). `k_max` is the ensemble
/// ceiling the parameter names (the `EnsembleFact` it would edit).
pub fn classify_compute_rule_delta(param: &str, old: i64, new: i64) -> BudgetDelta {
    let loosening = match param {
        // Spend ceilings — raising them loosens.
        "child_floor_ppm" | "k_max" | "budget_share_cap_ppm" | "agree_window" => new > old,
        // Guard thresholds — lowering them loosens (more spend admissible).
        "theta_agree_ppm" | "streak_n" => new < old,
        // Unregistered parameters are never classified tightening.
        _ => false,
    };
    if loosening {
        BudgetDelta::Loosening
    } else if new != old {
        match param {
            "child_floor_ppm"
            | "k_max"
            | "budget_share_cap_ppm"
            | "agree_window"
            | "theta_agree_ppm"
            | "streak_n" => BudgetDelta::Tightening,
            _ => BudgetDelta::Neutral,
        }
    } else {
        BudgetDelta::Neutral
    }
}

/// Whether an evolution-authored parameter edit is admissible — `Loosening`
/// is refused (AC-F4-10); `amend` never appears (the verb set this class
/// registers is `{bind, capabilities, advise, observe, estimate, explain}`
/// — no `amend`, no `authorize`).
pub fn compute_rule_delta_admissible(param: &str, old: i64, new: i64) -> Result<(), ComputeError> {
    match classify_compute_rule_delta(param, old, new) {
        BudgetDelta::Loosening => Err(ComputeError::PolicyInvalid {
            rule_id: param.to_string(),
            reason: "budget_delta = loosening — rejected".to_string(),
        }),
        _ => Ok(()),
    }
}

/// `rules` — the C3 estimator baseline: five conditioned rules over typed
/// ctx members, **no model calls** (its `cost_of_estimation` is always the
/// zero vector — `harness_overhead.scheduling` records `model_calls: 0`).
#[derive(Debug, Default)]
pub struct RulesPolicy {
    /// The conditioned-rule config (debt-recorded thresholds).
    pub config: RulesConfig,
}

impl ComputePolicy for RulesPolicy {
    fn variant_ref(&self) -> &str {
        "rules"
    }

    fn capabilities(&self) -> PolicyCapabilities {
        PolicyCapabilities {
            options_supported: ComputeOptionKind::ALL.iter().copied().collect(),
            decision_points: all_decision_points(),
            makes_model_calls: false,
            estimator_ref: Some("rules".into()),
            requires_task_value: false,
            requires_priors: false,
            deterministic: true,
        }
    }

    fn bind(
        &self,
        decision: &ControlDecision,
        ctx: &ComputeContext,
    ) -> Result<BindOutcome, ComputeError> {
        if self.capabilities().requires_task_value && ctx.task_value.is_none() {
            return Err(ComputeError::TaskValueMissing);
        }
        let rows = eval_options(decision, ctx, &self.config);
        let binding = rules_binding(decision, ctx, &rows, &self.config);
        finish_bind(
            decision,
            ctx,
            "rules",
            &rows,
            rules_fired(&rows),
            binding,
            self.config.lambda_ppm,
        )
    }
}

fn all_decision_points() -> BTreeSet<String> {
    ["propose", "delegate", "verify", "retry", "stop"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn empty_record(point: DecisionPoint) -> ComputeDecisionRecord {
    ComputeDecisionRecord {
        record_id: String::new(),
        decision_id: String::new(),
        decision_point: point,
        options_considered: vec![],
        chosen: ComputeOption {
            kind: ComputeOptionKind::ContinueAsConfigured,
            extend_kind: None,
            target: None,
            qualifier: None,
        },
        binding: BTreeMap::new(),
        estimator_ref: EstimatorRef {
            variant_ref: "static".into(),
            version_id: "0".into(),
        },
        rules_fired: vec![],
        priors_used: vec![],
        inputs_read: vec![],
        inputs_digest: String::new(),
        rationale: RationaleRecord {
            objective_value_per_option: BTreeMap::new(),
            lambda_ppm: 0,
            task_value_ref: None,
        },
        cost_of_estimation: ResourceVector::zero(),
    }
}

/// `degraded_record` — the `Unchanged` record for a typed estimator
/// degradation (`EstimatorBudgetExhausted` / `TaskValueMissing` →
/// `Unchanged`, never a stop — §5e.4): `chosen` is
/// `continue_as_configured` with the typed failure on `qualifier`,
/// `binding` empty; `inputs_read`/`inputs_digest` still land — the bind
/// ran, and the evidence says exactly which ctx members it read first.
pub fn degraded_record(
    decision: &ControlDecision,
    ctx: &ComputeContext,
    variant_ref: &str,
    err: &ComputeError,
) -> ComputeDecisionRecord {
    let mut r = empty_record(decision.stamp.decision_point);
    r.estimator_ref.variant_ref = variant_ref.to_string();
    r.chosen.qualifier = Some(err.to_string());
    r.inputs_read = inputs_read(ctx);
    r.inputs_digest = inputs_digest(ctx);
    r.rationale.task_value_ref = ctx.task_value.as_ref().map(|t| t.contract_ref.clone());
    r.record_id = r.compute_record_id();
    r
}

// ─────────────────────────────────────────────────────────────────────────────
// The `rules` evaluator — five conditioned rules over the typed ctx
// ─────────────────────────────────────────────────────────────────────────────

fn opt(kind: ComputeOptionKind) -> ComputeOption {
    ComputeOption {
        kind,
        extend_kind: None,
        target: None,
        qualifier: None,
    }
}

fn infeasible(kind: ComputeOptionKind, reason: ComputeInfeasible) -> OptionRow {
    OptionRow {
        option: opt(kind),
        estimate: OptionEstimate::Infeasible(reason),
    }
}

fn estimate(kind: ComputeOptionKind, e: MarginalValueEstimate) -> OptionRow {
    OptionRow {
        option: opt(kind),
        estimate: OptionEstimate::Estimate(e),
    }
}

fn no_risk(coordination: Coordination) -> RiskEstimate {
    RiskEstimate {
        effect_classes: vec![],
        coordination,
        starvation_p_ppm: None,
        cache_invalidation: false,
    }
}

fn base_estimate(
    delta_p: DeltaP,
    cost: ResourceVector,
    coordination: Coordination,
    oracle: Option<String>,
) -> MarginalValueEstimate {
    MarginalValueEstimate {
        delta_p,
        expected_cost: cost,
        expected_latency_ms: None,
        risk: no_risk(coordination),
        oracle_conditionality: oracle,
        evidence_refs: vec![],
    }
}

/// Whether `declared parallel steps or subagent_task targets` exist.
fn parallel_declared(ctx: &ComputeContext) -> bool {
    ctx.declared_parallel_steps > 0 || !ctx.subagent_task_targets.is_empty()
}

/// `remaining ÷ n` — a ResourceVector scaled by an integer share (integer
/// division per dimension; the bound slice is reserve-sized inside it).
fn scale_vector(v: &ResourceVector, divisor: u32) -> ResourceVector {
    let mut out = ResourceVector::zero();
    if divisor == 0 {
        return out;
    }
    for (d, a) in v.iter() {
        out.add(d, a / divisor as i64);
    }
    out
}

/// A vector as a ppm share of `remaining`.
fn vector_ppm_of(v: &ResourceVector, ppm: i64) -> ResourceVector {
    let mut out = ResourceVector::zero();
    for (d, a) in v.iter() {
        out.add(d, a.saturating_mul(ppm) / 1_000_000);
    }
    out
}

/// `cost ≤ remaining` componentwise — the B-3 reserve projection (a binding
/// the envelope would refuse is never emitted).
fn reserve_fits(ctx: &ComputeContext, cost: ResourceVector) -> bool {
    cost.iter().all(|(d, a)| a <= ctx.budget.remaining.get(d))
}

/// One ensemble member's reserve-sized cost: `cost_model` when declared,
/// else the profile `max_output` share of `remaining` (one member's
/// canonical unit — the `sample_cost` floor is one `model_calls` plus
/// `max_output` tokens when no cost model resolves).
fn sample_cost(ctx: &ComputeContext) -> ResourceVector {
    if let Some(c) = &ctx.cost_model {
        return c.clone();
    }
    let mut v = ResourceVector::zero();
    v.add(hh_ontology::dimensions::DimensionId::ModelCalls, 1);
    // `max_output` reserve: the profile's declared bound lands on
    // `tokens.output.visible` when no finer view exists.
    if let Some(b) = ctx
        .profile_capabilities
        .get("max_output")
        .and_then(Json::as_int)
    {
        v.add(hh_ontology::dimensions::DimensionId::TokensOutputVisible, b);
    }
    v
}

/// Agreement over the last `window` samples — canonical-output-hash
/// plurality or a unanimous verdict; `None` under the window.
fn agreement_ppm(ctx: &ComputeContext, window: u32) -> Option<i64> {
    if window == 0 || ctx.samples.len() < window as usize {
        return None;
    }
    let tail = &ctx.samples[ctx.samples.len() - window as usize..];
    // Canonical-answer agreement: the plurality share of one output_hash.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for s in tail {
        *counts.entry(s.output_hash.as_str()).or_insert(0) += 1;
    }
    let plurality = counts.values().copied().max().unwrap_or(0);
    // A verdict row counts too: a unanimous verdict set on every sample.
    let unanimous_verdict = tail
        .iter()
        .all(|s| !s.verdicts.is_empty() && s.verdicts == tail[0].verdicts);
    let verdict_ppm = if unanimous_verdict { 1_000_000 } else { 0 };
    Some((plurality as i64 * 1_000_000 / window as i64).max(verdict_ppm))
}

/// The effort ladder — `profile.capabilities.effort.ladder` (ordered rung
/// spellings). `None` ⇒ `capability_unmet`.
fn effort_ladder(ctx: &ComputeContext) -> Vec<String> {
    ctx.profile_capabilities
        .get("effort")
        .and_then(|e| e.get("ladder"))
        .and_then(|l| match l {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

fn effort_declared(ctx: &ComputeContext) -> Option<Json> {
    ctx.profile_capabilities.get("effort").cloned()
}

/// The full `options_considered[]` evaluation — every supported option with
/// an estimate or a typed infeasibility (B-4). Option-kind applicability is
/// decided by the decision's kind: a `delegate` parameter can only bind on
/// a `delegate` decision &c. — a non-applicable option is `policy_excluded`
/// *for this decision* (the class-level `options_supported` declaration is
/// unchanged).
fn eval_options(
    decision: &ControlDecision,
    ctx: &ComputeContext,
    cfg: &RulesConfig,
) -> Vec<OptionRow> {
    let mut rows = Vec::with_capacity(7);

    // continue_as_configured — always estimable: Δp = 0, zero cost.
    rows.push(estimate(
        ComputeOptionKind::ContinueAsConfigured,
        base_estimate(
            DeltaP::rule(0),
            ResourceVector::zero(),
            Coordination::None,
            ctx.verifier.clone(),
        ),
    ));

    // spawn_subagent — rule (b): only on a `delegate` decision, only on
    // declared parallel steps/subagent_task targets, under both gauges,
    // with a reservable slice ≥ child_floor.
    match &decision.kind {
        DecisionKind::Delegate { .. } => {
            if !parallel_declared(ctx) {
                rows.push(infeasible(
                    ComputeOptionKind::SpawnSubagent,
                    ComputeInfeasible::UndeclaredParallelism,
                ));
            } else if ctx.budget.live_fan_out >= ctx.budget.fan_out_cap
                || ctx.budget.delegation_depth >= ctx.budget.delegation_depth_cap
            {
                rows.push(infeasible(
                    ComputeOptionKind::SpawnSubagent,
                    ComputeInfeasible::GaugeAtCap,
                ));
            } else {
                let steps = ctx.declared_parallel_steps.max(1);
                let slice = scale_vector(&ctx.budget.remaining, steps + 1);
                let floor = vector_ppm_of(&ctx.budget.remaining, cfg.child_floor_ppm);
                let clears_floor = floor
                    .iter()
                    .all(|(d, f)| slice.get(d) >= f || ctx.budget.remaining.get(d) == 0);
                if !clears_floor || !reserve_fits(ctx, slice.clone()) {
                    rows.push(infeasible(
                        ComputeOptionKind::SpawnSubagent,
                        ComputeInfeasible::InsufficientBudget,
                    ));
                } else {
                    let mut e = base_estimate(
                        DeltaP::rule(200_000),
                        slice,
                        Coordination::FanOut,
                        ctx.verifier.clone(),
                    );
                    e.evidence_refs = ctx.subagent_task_targets.iter().take(1).cloned().collect();
                    rows.push(estimate(ComputeOptionKind::SpawnSubagent, e));
                }
            }
        }
        _ => rows.push(infeasible(
            ComputeOptionKind::SpawnSubagent,
            ComputeInfeasible::PolicyExcluded,
        )),
    }

    // extend_search{additional_sample} — rule (a): keep sampling while
    // agreement < θ_agree over the window and one more member is
    // reservable; stop at agreement ≥ θ or k_max. `delta_p` is `unknown`
    // without a reducer/validator (oracle_conditionality = none) — it then
    // never ranks above continue.
    match &decision.kind {
        DecisionKind::Propose { .. } => {
            let Some(ens) = &ctx.ensemble else {
                rows.push(infeasible(
                    ComputeOptionKind::ExtendSearch,
                    ComputeInfeasible::CapabilityUnmet,
                ));
                return finish_rows(decision, ctx, cfg, rows);
            };
            let agree = agreement_ppm(ctx, cfg.agree_window);
            let samples = ctx.samples.len() as u32;
            if ens.oracle.is_none() && ctx.verifier.is_none() {
                let mut e = base_estimate(
                    DeltaP::unknown(),
                    sample_cost(ctx),
                    Coordination::None,
                    None,
                );
                e.delta_p.source = DeltaPSource::Unknown;
                rows.push(OptionRow {
                    option: ComputeOption {
                        kind: ComputeOptionKind::ExtendSearch,
                        extend_kind: Some(ExtendSearchKind::AdditionalSample),
                        target: None,
                        qualifier: None,
                    },
                    estimate: OptionEstimate::Estimate(e),
                });
            } else if samples >= ens.k_max {
                rows.push(infeasible(
                    ComputeOptionKind::ExtendSearch,
                    ComputeInfeasible::GaugeAtCap,
                ));
            } else if !reserve_fits(ctx, sample_cost(ctx)) {
                rows.push(infeasible(
                    ComputeOptionKind::ExtendSearch,
                    ComputeInfeasible::InsufficientBudget,
                ));
            } else {
                let wants_more = match agree {
                    Some(a) => a < cfg.theta_agree_ppm,
                    None => true, // under the window — keep probing
                };
                let delta = if wants_more {
                    DeltaP::rule(100_000)
                } else {
                    DeltaP::rule(0)
                };
                rows.push(OptionRow {
                    option: ComputeOption {
                        kind: ComputeOptionKind::ExtendSearch,
                        extend_kind: Some(ExtendSearchKind::AdditionalSample),
                        target: None,
                        qualifier: None,
                    },
                    estimate: OptionEstimate::Estimate(base_estimate(
                        delta,
                        sample_cost(ctx),
                        Coordination::None,
                        ens.oracle.clone().or_else(|| ctx.verifier.clone()),
                    )),
                });
            }
        }
        _ => rows.push(infeasible(
            ComputeOptionKind::ExtendSearch,
            ComputeInfeasible::PolicyExcluded,
        )),
    }

    finish_rows(decision, ctx, cfg, rows)
}

fn finish_rows(
    decision: &ControlDecision,
    ctx: &ComputeContext,
    cfg: &RulesConfig,
    mut rows: Vec<OptionRow>,
) -> Vec<OptionRow> {
    // evaluator — rule (c): ADR-0117's rule over the four recorded inputs,
    // positive only when the placement's calibration is `active`.
    match &decision.kind {
        DecisionKind::Verify { .. } => {
            if ctx.placements.is_empty() {
                rows.push(infeasible(
                    ComputeOptionKind::Evaluator,
                    ComputeInfeasible::NoPlacement,
                ));
            } else {
                // Score every declared placement; the best admissible one
                // wins. An `inactive` placement is a typed infeasibility —
                // never a silently-skipped row (B-4).
                let mut any_admissible = false;
                for p in &ctx.placements {
                    if p.calibration_status != hh_verification::vocab::CalibrationStatus::Active {
                        continue;
                    }
                    any_admissible = true;
                    let false_stop_cost = ctx
                        .task_value
                        .as_ref()
                        .and_then(|t| t.success_value)
                        .unwrap_or(0);
                    // ADR-0117 d.7: ev = missed·cost(uncaught)
                    //   − false_stop·cost(false_stop) − overhead.
                    let ev = p.missed_failure_rate_ppm * p.uncaught_cost / 1_000_000
                        - p.false_stop_rate_ppm * false_stop_cost / 1_000_000
                        - p.verification_overhead;
                    if ev > 0 {
                        let mut e = base_estimate(
                            DeltaP::rule(ev.min(1_000_000)),
                            {
                                let mut v = ResourceVector::zero();
                                v.add(hh_ontology::dimensions::DimensionId::ModelCalls, 1);
                                v
                            },
                            Coordination::None,
                            Some("judge".into()),
                        );
                        e.evidence_refs = vec![p.critic_ref.clone()];
                        rows.push(OptionRow {
                            option: ComputeOption {
                                kind: ComputeOptionKind::Evaluator,
                                extend_kind: None,
                                target: Some(p.critic_ref.clone()),
                                qualifier: None,
                            },
                            estimate: OptionEstimate::Estimate(e),
                        });
                    }
                }
                if !any_admissible {
                    rows.push(infeasible(
                        ComputeOptionKind::Evaluator,
                        ComputeInfeasible::CalibrationInactive,
                    ));
                } else if !rows
                    .iter()
                    .any(|r| r.option.kind == ComputeOptionKind::Evaluator)
                {
                    // Admissible but the rule evaluated non-positive — a
                    // measured zero, not an infeasibility.
                    rows.push(estimate(
                        ComputeOptionKind::Evaluator,
                        base_estimate(
                            DeltaP::rule(0),
                            ResourceVector::zero(),
                            Coordination::None,
                            Some("judge".into()),
                        ),
                    ));
                }
            }
        }
        _ => rows.push(infeasible(
            ComputeOptionKind::Evaluator,
            ComputeInfeasible::PolicyExcluded,
        )),
    }

    // effort_level — rule (d): by `ModelRole` at the profile's declared
    // ladder; a step-up admissible only on a recorded failure streak ≥ n
    // and only when the profile declares per-message effort without
    // cache-invalidation.
    match &decision.kind {
        DecisionKind::Propose { .. } => {
            let ladder = effort_ladder(ctx);
            if ladder.is_empty() || effort_declared(ctx).is_none() {
                rows.push(infeasible(
                    ComputeOptionKind::EffortLevel,
                    ComputeInfeasible::CapabilityUnmet,
                ));
            } else {
                let e = effort_declared(ctx).unwrap_or(Json::Null);
                let per_message = matches!(e.get("per_message"), Some(Json::Bool(true)));
                let invalidates = matches!(
                    e.get("cache_invalidation_on_change"),
                    Some(Json::Bool(true))
                );
                let streak_ok = ctx.validator_streaks.fail >= cfg.streak_n
                    || ctx.validator_streaks.format_failure >= cfg.streak_n;
                let step_up_admissible = streak_ok && per_message && !invalidates;
                let mut est = base_estimate(
                    if step_up_admissible {
                        DeltaP::rule(50_000)
                    } else {
                        DeltaP::rule(0)
                    },
                    ResourceVector::zero(),
                    Coordination::None,
                    ctx.verifier.clone(),
                );
                est.risk.cache_invalidation = invalidates;
                rows.push(OptionRow {
                    option: ComputeOption {
                        kind: ComputeOptionKind::EffortLevel,
                        extend_kind: None,
                        target: if step_up_admissible {
                            current_rung(decision)
                                .and_then(|r| ladder.get(r + 1).cloned())
                                .or_else(|| ladder.last().cloned())
                        } else {
                            Some(ladder[0].clone())
                        },
                        qualifier: None,
                    },
                    estimate: OptionEstimate::Estimate(est),
                });
            }
        }
        _ => rows.push(infeasible(
            ComputeOptionKind::EffortLevel,
            ComputeInfeasible::PolicyExcluded,
        )),
    }

    // alternate_role — rule (e): only on a validator-fail streak with a
    // `QualityPrior` whose `valid_until` holds and level is not `unknown`.
    match &decision.kind {
        DecisionKind::Propose { .. } => {
            let streak_ok = ctx.validator_streaks.fail >= cfg.streak_n;
            let prior_ok = ctx
                .priors
                .iter()
                .any(|p| p.valid_until.is_none() && p.n > 0 && p.cell.contains("quality"));
            if !streak_ok || !prior_ok {
                rows.push(infeasible(
                    ComputeOptionKind::AlternateRole,
                    ComputeInfeasible::PriorUnknown,
                ));
            } else {
                rows.push(estimate(
                    ComputeOptionKind::AlternateRole,
                    base_estimate(
                        DeltaP::rule(50_000),
                        sample_cost(ctx),
                        Coordination::None,
                        ctx.verifier.clone(),
                    ),
                ));
            }
        }
        _ => rows.push(infeasible(
            ComputeOptionKind::AlternateRole,
            ComputeInfeasible::PolicyExcluded,
        )),
    }

    // stop_now — never admissible for `rules` (stability is never
    // sufficient where a deterministic validator exists; the baseline
    // excludes it unconditionally).
    rows.push(infeasible(
        ComputeOptionKind::StopNow,
        ComputeInfeasible::PolicyExcluded,
    ));

    rows
}

/// The B-2 widening exception: an effort step-up is admissible only on a
/// recorded failure streak >= `streak_n` **and** a profile declaring
/// `effort.per_message` without `cache_invalidation_on_change`
/// (§5e.4 rule (d)).
fn effort_step_up_admissible(ctx: &ComputeContext, _rungs: usize) -> bool {
    let cfg = RulesConfig::default();
    let streak_ok = ctx.validator_streaks.fail >= cfg.streak_n
        || ctx.validator_streaks.format_failure >= cfg.streak_n;
    let Some(e) = effort_declared(ctx) else {
        return false;
    };
    let per_message = matches!(e.get("per_message"), Some(Json::Bool(true)));
    let invalidates = matches!(
        e.get("cache_invalidation_on_change"),
        Some(Json::Bool(true))
    );
    streak_ok && per_message && !invalidates
}

/// The decision's current effort rung index (the bound `context_request.
/// effort` member read against the ladder).
fn current_rung(decision: &ControlDecision) -> Option<usize> {
    if let DecisionKind::Propose {
        context_request, ..
    } = &decision.kind
    {
        context_request
            .get("effort")
            .and_then(Json::as_str)
            .map(|_| 0)
    } else {
        None
    }
}

/// The `rules_fired` names — the conditioned rules whose predicates
/// evaluated positive (each id is a MUST-data rule with an
/// `AssumptionDebtRecord` in the variant row).
fn rules_fired(rows: &[OptionRow]) -> Vec<String> {
    let mut fired = vec![];
    for r in rows {
        let positive = matches!(
            &r.estimate,
            OptionEstimate::Estimate(e) if e.delta_p.point_ppm.is_some_and(|p| p > 0)
        );
        if positive {
            fired.push(match r.option.kind {
                ComputeOptionKind::SpawnSubagent => "compute_rule.spawn_declared_parallel",
                ComputeOptionKind::ExtendSearch => "compute_rule.extend_search_agreement",
                ComputeOptionKind::Evaluator => "compute_rule.evaluator_calibration",
                ComputeOptionKind::EffortLevel => "compute_rule.effort_streak",
                ComputeOptionKind::AlternateRole => "compute_rule.alternate_role_streak",
                _ => "compute_rule.continue",
            });
        }
    }
    fired.into_iter().map(str::to_string).collect()
}

/// The objective — `ΔP̂ × success_value − λ·cost − P̂(harm)·failure_cost`,
/// integer ppm arithmetic. `unknown` deltas never rank (no member).
fn objective(e: &MarginalValueEstimate, ctx: &ComputeContext, lambda_ppm: i64) -> Option<i64> {
    let dp = e.delta_p.point_ppm?;
    let success = ctx
        .task_value
        .as_ref()
        .and_then(|t| t.success_value)
        .unwrap_or(1_000_000);
    let fail = ctx
        .task_value
        .as_ref()
        .map(TaskValueFact::failure_cost_ppm)
        .unwrap_or(0);
    // Cost basis: the option's reserve share of `remaining` (ppm) — an
    // options-relative cost, never a spend authority.
    let remaining_total: i64 = ctx.budget.remaining.iter().map(|(_, a)| a).sum();
    let cost_total: i64 = e.expected_cost.iter().map(|(_, a)| a).sum();
    let cost_ppm = if remaining_total > 0 {
        (cost_total * 1_000_000 / remaining_total).min(1_000_000)
    } else if cost_total > 0 {
        1_000_000
    } else {
        0
    };
    let harm_ppm = match e.risk.coordination {
        Coordination::None => 0,
        Coordination::FanOut | Coordination::Merge => 50_000,
    };
    Some(dp * success / 1_000_000 - lambda_ppm * cost_ppm / 1_000_000 - harm_ppm * fail / 1_000_000)
}

/// The binding map `rules` produces for this decision — the best-ranked
/// applicable option with a positive objective (B-3 reserve check already
/// ran inside `eval_options`; `continue` binds nothing).
fn rules_binding(
    decision: &ControlDecision,
    ctx: &ComputeContext,
    rows: &[OptionRow],
    cfg: &RulesConfig,
) -> BTreeMap<String, Json> {
    let mut best: Option<(&OptionRow, i64)> = None;
    for r in rows {
        if let OptionEstimate::Estimate(e) = &r.estimate {
            if r.option.kind == ComputeOptionKind::ContinueAsConfigured {
                continue;
            }
            if let Some(o) = objective(e, ctx, cfg.lambda_ppm) {
                if o > 0 && best.map(|(_, b)| o > b).unwrap_or(true) {
                    best = Some((r, o));
                }
            }
        }
    }
    let mut binding = BTreeMap::new();
    let Some((row, _)) = best else {
        return binding;
    };
    match row.option.kind {
        ComputeOptionKind::ExtendSearch => {
            if let Some(ens) = &ctx.ensemble {
                let next = (ctx.samples.len() as u32 + 1).min(ens.k_max);
                if next > ctx.samples.len() as u32 {
                    binding.insert("propose.sample_k".into(), Json::Int(next as i64));
                }
            }
        }
        ComputeOptionKind::SpawnSubagent => {
            let steps = ctx.declared_parallel_steps.max(1);
            let slice = scale_vector(&ctx.budget.remaining, steps + 1);
            binding.insert("delegate.budget_slice".into(), slice.to_json());
            if let Some(t) = ctx.subagent_task_targets.first() {
                binding.insert("delegate.spec_ref".into(), Json::str(t.clone()));
            }
            if matches!(&decision.kind, DecisionKind::Delegate { delegation_reason, .. } if delegation_reason.is_none())
            {
                binding.insert(
                    "delegate.delegation_reason".into(),
                    Json::str("parallelism"),
                );
            }
        }
        ComputeOptionKind::Evaluator => {
            if let Some(t) = &row.option.target {
                binding.insert(
                    "verify.validator_refs".into(),
                    Json::Arr(vec![Json::str(t.clone())]),
                );
            }
        }
        ComputeOptionKind::EffortLevel => {
            if let Some(t) = &row.option.target {
                binding.insert("propose.effort".into(), Json::str(t.clone()));
            }
        }
        ComputeOptionKind::AlternateRole => {
            if let Some(role) = ctx.role_table.get("roles").and_then(|r| match r {
                Json::Arr(a) => a.first().and_then(Json::as_str),
                _ => None,
            }) {
                binding.insert("propose.role".into(), Json::str(role.to_string()));
            }
        }
        _ => {}
    }
    binding
}

// ─────────────────────────────────────────────────────────────────────────────
// The shared finish — apply the binding, verify B-1/B-2, build the record
// ─────────────────────────────────────────────────────────────────────────────

fn finish_bind(
    decision: &ControlDecision,
    ctx: &ComputeContext,
    estimator_variant: &str,
    rows: &[OptionRow],
    fired: Vec<String>,
    binding: BTreeMap<String, Json>,
    lambda_ppm: i64,
) -> Result<BindOutcome, ComputeError> {
    let mut objectives = BTreeMap::new();
    for r in rows {
        if let OptionEstimate::Estimate(e) = &r.estimate {
            if let Some(o) = objective(e, ctx, lambda_ppm) {
                objectives.insert(option_key(&r.option), o);
            }
        }
    }
    let chosen = choose(rows, &objectives);
    let mut record = ComputeDecisionRecord {
        record_id: String::new(),
        decision_id: String::new(), // the driver stamps it (allocated id)
        decision_point: decision.stamp.decision_point,
        options_considered: rows.to_vec(),
        chosen,
        binding: binding.clone(),
        estimator_ref: EstimatorRef {
            variant_ref: estimator_variant.into(),
            version_id: "0".into(),
        },
        rules_fired: fired,
        priors_used: ctx
            .priors
            .iter()
            .map(|p| PriorUsed {
                cell: p.cell.clone(),
                n: p.n,
                mean_ppm: Some(p.mean_ppm),
                interval: p.interval,
                valid_until: p.valid_until,
            })
            .collect(),
        inputs_read: inputs_read(ctx),
        inputs_digest: inputs_digest(ctx),
        rationale: RationaleRecord {
            objective_value_per_option: objectives,
            lambda_ppm,
            task_value_ref: ctx.task_value.as_ref().map(|t| t.contract_ref.clone()),
        },
        cost_of_estimation: ResourceVector::zero(),
    };
    record.record_id = record.compute_record_id();

    if binding.is_empty() {
        return Ok(BindOutcome::Unchanged { record });
    }
    let bound = apply_binding(decision, &binding)?;
    check_bound(decision, &bound, ctx)?;
    Ok(BindOutcome::Bound {
        decision: bound,
        record,
    })
}

/// The `options_considered` map key — `kind` plus the named target when one
/// exists (two `evaluator` rows name two placements).
fn option_key(o: &ComputeOption) -> String {
    match &o.target {
        Some(t) => format!("{}:{}", o.kind.as_str(), t),
        None => o.kind.as_str().to_string(),
    }
}

/// `chosen` — the best-scoring option; `continue_as_configured` on a tie or
/// an empty positive set (an `unknown` estimate never ranks).
fn choose(rows: &[OptionRow], objectives: &BTreeMap<String, i64>) -> ComputeOption {
    let mut best: Option<(&ComputeOption, i64)> = None;
    for r in rows {
        if r.option.kind == ComputeOptionKind::ContinueAsConfigured {
            continue;
        }
        if let OptionEstimate::Estimate(_) = &r.estimate {
            if let Some(o) = objectives.get(&option_key(&r.option)) {
                if *o > 0 && best.map(|(_, b)| *o > b).unwrap_or(true) {
                    best = Some((&r.option, *o));
                }
            }
        }
    }
    best.map(|(o, _)| o.clone()).unwrap_or(ComputeOption {
        kind: ComputeOptionKind::ContinueAsConfigured,
        extend_kind: None,
        target: None,
        qualifier: None,
    })
}

/// `inputs_read` — the ctx member spellings consumed (B-5 evidence; the
/// list is total over members the bind actually read).
fn inputs_read(ctx: &ComputeContext) -> Vec<String> {
    let mut v = vec![
        "budget_view.remaining".to_string(),
        "budget_view.reserved".to_string(),
        "budget_view.fan_out".to_string(),
        "budget_view.delegation_depth".to_string(),
        "budget_view.context_occupancy".to_string(),
        "plan_cursor.declared_parallel_steps".to_string(),
        "plan_cursor.subagent_task_targets".to_string(),
        "samples_so_far".to_string(),
        "verifier_available".to_string(),
        "validator_streaks".to_string(),
        "profile.capabilities".to_string(),
        "placements".to_string(),
        "priors".to_string(),
        "cost_model".to_string(),
        "health_view".to_string(),
        "context_label".to_string(),
    ];
    if ctx.task_value.is_some() {
        v.push("task_value".to_string());
    }
    if ctx.ensemble.is_some() {
        v.push("ensemble".to_string());
    }
    v
}

/// `inputs_digest` — `sha256:` over the ctx's canonical form.
fn inputs_digest(ctx: &ComputeContext) -> String {
    format!(
        "sha256:{}",
        hh_wire::sha256::sha256_hex(ctx_json(ctx).to_canonical_string().as_bytes())
    )
}

/// The ctx's canonical JSON form (the digest input — typed members only).
fn ctx_json(ctx: &ComputeContext) -> Json {
    Json::obj([
        ("remaining", ctx.budget.remaining.to_json()),
        ("reserved", ctx.budget.reserved.to_json()),
        ("live_fan_out", Json::Int(ctx.budget.live_fan_out as i64)),
        ("fan_out_cap", Json::Int(ctx.budget.fan_out_cap as i64)),
        (
            "delegation_depth",
            Json::Int(ctx.budget.delegation_depth as i64),
        ),
        (
            "delegation_depth_cap",
            Json::Int(ctx.budget.delegation_depth_cap as i64),
        ),
        ("occupancy_ppm", Json::Int(ctx.budget.occupancy_ppm)),
        (
            "declared_parallel_steps",
            Json::Int(ctx.declared_parallel_steps as i64),
        ),
        (
            "subagent_task_targets",
            Json::Arr(ctx.subagent_task_targets.iter().map(Json::str).collect()),
        ),
        (
            "ensemble",
            ctx.ensemble
                .as_ref()
                .map(|e| {
                    Json::obj([
                        ("k_max", Json::Int(e.k_max as i64)),
                        (
                            "oracle",
                            e.oracle.as_deref().map(Json::str).unwrap_or(Json::Null),
                        ),
                    ])
                })
                .unwrap_or(Json::Null),
        ),
        (
            "samples",
            Json::Arr(
                ctx.samples
                    .iter()
                    .map(|s| {
                        Json::obj([
                            ("output_hash", Json::str(&s.output_hash)),
                            (
                                "verdicts",
                                Json::Arr(s.verdicts.iter().map(Json::str).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "verifier",
            ctx.verifier.as_deref().map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "validator_streaks",
            Json::obj([
                ("pass", Json::Int(ctx.validator_streaks.pass as i64)),
                ("fail", Json::Int(ctx.validator_streaks.fail as i64)),
                (
                    "format_failure",
                    Json::Int(ctx.validator_streaks.format_failure as i64),
                ),
            ]),
        ),
        ("profile_capabilities", ctx.profile_capabilities.clone()),
        ("role_table", ctx.role_table.clone()),
        (
            "placements",
            Json::Arr(
                ctx.placements
                    .iter()
                    .map(|p| {
                        Json::obj([
                            ("critic_ref", Json::str(&p.critic_ref)),
                            (
                                "calibration_status",
                                Json::str(p.calibration_status.as_str()),
                            ),
                            (
                                "missed_failure_rate_ppm",
                                Json::Int(p.missed_failure_rate_ppm),
                            ),
                            ("false_stop_rate_ppm", Json::Int(p.false_stop_rate_ppm)),
                            ("verification_overhead", Json::Int(p.verification_overhead)),
                            ("uncaught_cost", Json::Int(p.uncaught_cost)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "priors",
            Json::Arr(
                ctx.priors
                    .iter()
                    .map(|p| {
                        Json::obj([
                            ("cell", Json::str(&p.cell)),
                            ("n", Json::Int(p.n as i64)),
                            ("mean_ppm", Json::Int(p.mean_ppm)),
                            (
                                "valid_until",
                                p.valid_until
                                    .map(|v| Json::Int(v as i64))
                                    .unwrap_or(Json::Null),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "cost_model",
            ctx.cost_model
                .as_ref()
                .map(|c| c.to_json())
                .unwrap_or(Json::Null),
        ),
        (
            "health",
            Json::str(match ctx.health {
                HealthView::Ok => "ok",
                HealthView::Degraded => "degraded",
            }),
        ),
        (
            "task_value",
            ctx.task_value
                .as_ref()
                .map(|t| t.to_json())
                .unwrap_or(Json::Null),
        ),
        (
            "context_label",
            ctx.context_label
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────
// `apply_binding` + `check_bound` — B-1/B-2 as MUST-code
// ─────────────────────────────────────────────────────────────────────────────

/// Apply `binding` to `decision` — the only parameter paths the scheduler
/// may write. Any other path or a wrong-kind decision is `PolicyInvalid`
/// (the map itself is the declared write set).
pub fn apply_binding(
    decision: &ControlDecision,
    binding: &BTreeMap<String, Json>,
) -> Result<ControlDecision, ComputeError> {
    let mut d = decision.clone();
    for (param, value) in binding {
        match param.as_str() {
            "delegate.budget_slice" => {
                let DecisionKind::Delegate { budget_slice, .. } = &mut d.kind else {
                    return Err(invalid(param, "not a delegate decision"));
                };
                *budget_slice = value.clone();
            }
            "delegate.spec_ref" => {
                let DecisionKind::Delegate { spec, .. } = &mut d.kind else {
                    return Err(invalid(param, "not a delegate decision"));
                };
                let mut m = match spec.clone() {
                    Json::Obj(m) => m,
                    Json::Null => BTreeMap::new(),
                    _ => return Err(invalid(param, "spec member is not an object")),
                };
                m.insert("spec_ref".to_string(), value.clone());
                *spec = Json::Obj(m);
            }
            "delegate.delegation_reason" => {
                let DecisionKind::Delegate {
                    delegation_reason, ..
                } = &mut d.kind
                else {
                    return Err(invalid(param, "not a delegate decision"));
                };
                *delegation_reason = Some(value.clone());
            }
            "propose.sample_k" | "propose.effort" | "propose.role" | "propose.quality_target" => {
                let DecisionKind::Propose {
                    context_request, ..
                } = &mut d.kind
                else {
                    return Err(invalid(param, "not a propose decision"));
                };
                let key = param.trim_start_matches("propose.").to_string();
                let mut m = match context_request.clone() {
                    Json::Obj(m) => m,
                    Json::Null => BTreeMap::new(),
                    _ => return Err(invalid(param, "context_request is not an object")),
                };
                m.insert(key, value.clone());
                *context_request = Json::Obj(m);
            }
            "verify.validator_refs" => {
                let DecisionKind::Verify { validator_refs, .. } = &mut d.kind else {
                    return Err(invalid(param, "not a verify decision"));
                };
                let Json::Arr(refs) = value else {
                    return Err(invalid(param, "validator_refs is not an array"));
                };
                let mut merged: BTreeSet<String> = validator_refs.iter().cloned().collect();
                for r in refs {
                    let Some(s) = r.as_str() else {
                        return Err(invalid(param, "validator_refs member is not a string"));
                    };
                    merged.insert(s.to_string());
                }
                *validator_refs = merged.into_iter().collect();
            }
            "stop.proposed_reason" => {
                // `stop_now{proposed_reason}` — the option requests, the
                // envelope's stop conversion still owns the verdict.
                let DecisionKind::Stop {
                    proposed_reason, ..
                } = &mut d.kind
                else {
                    return Err(invalid(param, "not a stop decision"));
                };
                let Some(r) = parse_stop_reason(value) else {
                    return Err(invalid(param, "unparsable proposed_reason"));
                };
                *proposed_reason = r;
            }
            other => {
                return Err(invalid(other, "outside the declared binding set"));
            }
        }
    }
    Ok(d)
}

fn invalid(param: &str, reason: &str) -> ComputeError {
    ComputeError::PolicyInvalid {
        rule_id: param.to_string(),
        reason: reason.to_string(),
    }
}

fn parse_stop_reason(j: &Json) -> Option<hh_ontology::control::StopReason> {
    // The scheduler binds only the closed `completed`/`cancelled` spellings —
    // envelope-owned reasons are never synthesised here.
    match j.as_str()? {
        "completed" => Some(hh_ontology::control::StopReason::Completed),
        "cancelled" => Some(hh_ontology::control::StopReason::Cancelled {
            by: hh_ontology::control::CancelledBy::Principal,
        }),
        _ => None,
    }
}

/// `check_bound(before, after, ctx)` — B-1/B-2 as a standalone verdict
/// (AC-F4-2's link gate: a fixture policy whose output violates them fails
/// `PolicyInvalid`).
///
/// B-1 — `d′.kind = d.kind`, `d′.decision_point = d.decision_point`,
/// `d′.owner = d.owner`. B-2 — every *changed* parameter was unbound or is
/// tightened: `delegate.budget_slice` componentwise ≤; `propose.sample_k`
/// `k′ ≤ k`; `propose.effort` same-or-lower rung on the profile's declared
/// ladder (a step-up only by the declared streak rule); `verify.
/// validator_refs` superset-of only; `delegate.spec_ref`/
/// `delegation_reason`/`propose.role`/`quality_target` bind-once only;
/// `stop.proposed_reason` is a proposal the envelope still owns — a changed
/// reason is admissible only through `stop_now` (binding-name checked by
/// the caller; here the member change is permitted verbatim per §5e.4).
pub fn check_bound(
    before: &ControlDecision,
    after: &ControlDecision,
    ctx: &ComputeContext,
) -> Result<(), ComputeError> {
    // B-1.
    if before.stamp != after.stamp {
        return Err(invalid(
            "B-1",
            "stamp drifted (decision_point/owner/rationale_ref)",
        ));
    }
    if before.kind.as_str() != after.kind.as_str() {
        return Err(invalid("B-1", "decision kind changed"));
    }
    // B-2 per kind.
    match (&before.kind, &after.kind) {
        (
            DecisionKind::Propose {
                context_request: old,
                ..
            },
            DecisionKind::Propose {
                context_request: new,
                ..
            },
        ) => check_context_request(old, new, ctx),
        (
            DecisionKind::Delegate {
                spec: os,
                budget_slice: ob,
                permissions: op,
                delegation_reason: odr,
            },
            DecisionKind::Delegate {
                spec: ns,
                budget_slice: nb,
                permissions: np,
                delegation_reason: ndr,
            },
        ) => {
            if op != np {
                return Err(invalid("delegate.permissions", "changed"));
            }
            // budget_slice — tighten only (componentwise ≤; `null` = unbound).
            // Binding the unbound is admissible; reverting a bound slice to
            // `null` removes a bound member — never.
            match (ob, nb) {
                (Json::Null, _) => {}
                (_, Json::Null) => {
                    return Err(invalid("delegate.budget_slice", "removed a bound member"));
                }
                (o, n) => {
                    let Some(ov) = ResourceVector::from_json(o) else {
                        return Err(invalid(
                            "delegate.budget_slice",
                            "prior member not a vector",
                        ));
                    };
                    let Some(nv) = ResourceVector::from_json(n) else {
                        return Err(invalid(
                            "delegate.budget_slice",
                            "bound member not a vector",
                        ));
                    };
                    for (d, a) in nv.iter() {
                        if a > ov.get(d) {
                            return Err(invalid("delegate.budget_slice", "widened a bound slice"));
                        }
                    }
                }
            }
            // spec.spec_ref / delegation_reason — bind-once only.
            if os.get("spec_ref") != ns.get("spec_ref") && os.get("spec_ref").is_some() {
                return Err(invalid("delegate.spec_ref", "rebound"));
            }
            if odr.is_some() && odr != ndr {
                return Err(invalid("delegate.delegation_reason", "rebound"));
            }
            // spec members other than spec_ref must not change.
            let strip = |s: &Json| -> Json {
                match s {
                    Json::Obj(m) => {
                        let mut m = m.clone();
                        m.remove("spec_ref");
                        Json::Obj(m)
                    }
                    other => other.clone(),
                }
            };
            if strip(os) != strip(ns) {
                return Err(invalid("delegate.spec", "changed"));
            }
            Ok(())
        }
        (
            DecisionKind::Verify {
                validator_refs: ov,
                subject: os,
            },
            DecisionKind::Verify {
                validator_refs: nv,
                subject: ns,
            },
        ) => {
            if os != ns {
                return Err(invalid("verify.subject", "changed"));
            }
            if ov.iter().any(|r| !nv.contains(r)) {
                return Err(invalid("verify.validator_refs", "removed a bound ref"));
            }
            Ok(())
        }
        (a, b) if a == b => Ok(()),
        _ => Err(invalid(
            "B-2",
            "decision member changed outside the binding set",
        )),
    }
}

fn check_context_request(old: &Json, new: &Json, ctx: &ComputeContext) -> Result<(), ComputeError> {
    let (om, nm) = match (old, new) {
        (Json::Null, Json::Null) => return Ok(()),
        (Json::Null, Json::Obj(n)) => {
            // Everything bound was unbound — admissible for the declared
            // keys only.
            for k in n.keys() {
                if !matches!(
                    k.as_str(),
                    "sample_k" | "effort" | "role" | "quality_target"
                ) && !k.starts_with("steer")
                    && !k.starts_with("follow_up")
                {
                    return Err(invalid(k, "bound outside the scheduler's set"));
                }
            }
            return Ok(());
        }
        (Json::Obj(o), Json::Obj(n)) => (o, n),
        _ => return Err(invalid("context_request", "shape changed")),
    };
    // Removed members — never.
    for k in om.keys() {
        if !nm.contains_key(k) {
            return Err(invalid(k, "removed a bound member"));
        }
    }
    for (k, nv) in nm {
        let Some(ov) = om.get(k) else {
            // A newly-bound member: declared keys only.
            if !matches!(
                k.as_str(),
                "sample_k" | "effort" | "role" | "quality_target"
            ) {
                return Err(invalid(k, "bound outside the scheduler's set"));
            }
            continue;
        };
        if ov == nv {
            continue;
        }
        match k.as_str() {
            "sample_k" => {
                let (a, b) = (ov.as_int(), nv.as_int());
                match (a, b) {
                    (_, None) => return Err(invalid(k, "not an integer")),
                    (Some(a), Some(b)) if b > a => {
                        return Err(invalid(k, "k′ > k — a bound k only tightens"))
                    }
                    _ => {}
                }
            }
            "effort" => {
                let ladder = effort_ladder(ctx);
                let rank = |v: &Json| v.as_str().and_then(|s| ladder.iter().position(|r| r == s));
                match (rank(ov), rank(nv)) {
                    (Some(a), Some(b)) if b > a => {
                        // B-2's only widening: the declared streak rule —
                        // a recorded fail/format streak ≥ n on a per-message
                        // profile that does not invalidate the cache.
                        if !effort_step_up_admissible(ctx, b - a) {
                            return Err(invalid(
                                k,
                                "effort′ > effort without the declared streak rule",
                            ));
                        }
                    }
                    (Some(_), Some(_)) => {}
                    _ => return Err(invalid(k, "effort outside the declared ladder")),
                }
            }
            "role" | "quality_target" => {
                return Err(invalid(k, "rebound — bind-once only"));
            }
            other => {
                return Err(invalid(other, "changed outside the scheduler's set"));
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// `compute_decision_outcome(run)` — the project() view + `explain`
// ─────────────────────────────────────────────────────────────────────────────

/// One joined row of `compute_decision_outcome` — the record plus what the
/// run realised (ADR-0188 D6; a projection, never a stored score).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeOutcomeRow {
    /// The decision record.
    pub record: ComputeDecisionRecord,
    /// The linked `control.decision`'s kind (B-1's evidence member).
    pub decision_kind: String,
    /// The run's outcome class, when finished.
    pub outcome_class: Option<String>,
    /// The verdicts that followed the decision (`verification.*` verdict
    /// payloads after the decision's seq).
    pub verdicts: Vec<Json>,
    /// The consumption recorded after the decision (`resource.consumed`
    /// payloads after the decision's seq).
    pub consumption: Vec<Json>,
}

/// `compute_decision_outcome(events)` — fold the durable prefix: every
/// `control.compute.decided` joined to its linked `control.decision`, the
/// run outcome and the verdicts/consumption that followed (AC-F4-4).
pub fn compute_decision_outcome(
    events: &[hh_ledger::event::EventEnvelope],
) -> Vec<ComputeOutcomeRow> {
    let decisions: BTreeMap<String, (u64, Json)> = events
        .iter()
        .filter(|e| e.class == "control.decision")
        .filter_map(|e| {
            e.payload
                .get("decision_id")
                .and_then(Json::as_str)
                .map(|id| (id.to_string(), (e.seq, e.payload.clone())))
        })
        .collect();
    let finished = events
        .iter()
        .find(|e| e.class == "lifecycle.run.finished")
        .and_then(|e| e.payload.get("outcome_class").and_then(Json::as_str))
        .map(str::to_string);
    let mut rows = Vec::new();
    for e in events {
        if e.class != "control.compute.decided" {
            continue;
        }
        let Some(record) = ComputeDecisionRecord::from_json(&e.payload) else {
            continue;
        };
        let dseq = decisions
            .get(&record.decision_id)
            .map(|(s, _)| *s)
            .unwrap_or(e.seq);
        let verdicts: Vec<Json> = events
            .iter()
            .filter(|v| v.seq > dseq && v.class.starts_with("verification."))
            .map(|v| v.payload.clone())
            .collect();
        let consumption: Vec<Json> = events
            .iter()
            .filter(|v| v.seq > dseq && v.class == "resource.consumed")
            .map(|v| v.payload.clone())
            .collect();
        rows.push(ComputeOutcomeRow {
            decision_kind: decisions
                .get(&record.decision_id)
                .and_then(|(_, p)| p.get("kind").and_then(Json::as_str))
                .unwrap_or("")
                .to_string(),
            record,
            outcome_class: finished.clone(),
            verdicts,
            consumption,
        });
    }
    rows
}

/// `explain(record_ref)` — the projection's render over one record
/// (AC-F4-4): the record's rationale plus its considered options, as
/// canonical JSON for WS-K2/J4 consumers.
pub fn explain(row: &ComputeOutcomeRow) -> Json {
    Json::obj([
        ("record_id", Json::str(&row.record.record_id)),
        ("decision_id", Json::str(&row.record.decision_id)),
        ("decision_kind", Json::str(&row.decision_kind)),
        ("chosen", row.record.chosen.to_json()),
        (
            "rationale",
            row.record
                .to_json()
                .get("rationale")
                .cloned()
                .unwrap_or(Json::Null),
        ),
        (
            "outcome_class",
            row.outcome_class
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{DecisionKind, DecisionStamp};

    fn rv(pairs: &[(hh_ontology::dimensions::DimensionId, i64)]) -> ResourceVector {
        let mut v = ResourceVector::zero();
        for (d, a) in pairs {
            v.add(*d, *a);
        }
        v
    }

    fn ctx_base() -> ComputeContext {
        ComputeContext {
            budget: BudgetView {
                remaining: rv(&[
                    (
                        hh_ontology::dimensions::DimensionId::TokensOutputVisible,
                        1_000_000,
                    ),
                    (hh_ontology::dimensions::DimensionId::ModelCalls, 8),
                ]),
                reserved: ResourceVector::zero(),
                live_fan_out: 0,
                fan_out_cap: 8,
                delegation_depth: 0,
                delegation_depth_cap: 4,
                occupancy_ppm: 100_000,
            },
            declared_parallel_steps: 0,
            subagent_task_targets: vec![],
            ensemble: None,
            samples: vec![],
            verifier: None,
            validator_streaks: ValidatorStreaks::default(),
            profile_capabilities: Json::obj([]),
            role_table: Json::obj([]),
            placements: vec![],
            priors: vec![],
            cost_model: None,
            health: HealthView::Ok,
            task_value: None,
            context_label: None,
        }
    }

    fn propose() -> ControlDecision {
        ControlDecision {
            stamp: DecisionStamp {
                decision_point: DecisionPoint::Plan,
                owner: hh_ontology::control::Owner::Model,
                rationale_ref: None,
            },
            kind: DecisionKind::Propose {
                decision_point: DecisionPoint::Plan,
                context_request: Json::obj([]),
                expected_output: crate::vocab::ExpectedOutput::Free,
            },
        }
    }

    fn delegate() -> ControlDecision {
        ControlDecision {
            stamp: DecisionStamp {
                decision_point: DecisionPoint::Delegate,
                owner: hh_ontology::control::Owner::Model,
                rationale_ref: None,
            },
            kind: DecisionKind::Delegate {
                spec: Json::obj([]),
                budget_slice: Json::Null,
                permissions: Json::obj([]),
                delegation_reason: None,
            },
        }
    }

    fn verify() -> ControlDecision {
        ControlDecision {
            stamp: DecisionStamp {
                decision_point: DecisionPoint::Verify,
                owner: hh_ontology::control::Owner::Model,
                rationale_ref: None,
            },
            kind: DecisionKind::Verify {
                validator_refs: vec!["v/base".into()],
                subject: Json::str("s"),
            },
        }
    }

    #[test]
    fn static_is_inert() {
        let p = StaticPolicy;
        assert!(p.capabilities().options_supported.is_empty());
        let out = p.bind(&propose(), &ctx_base()).unwrap();
        match out {
            BindOutcome::Unchanged { record } => {
                assert!(record.options_considered.is_empty())
            }
            _ => panic!("static binds"),
        }
    }

    #[test]
    fn policy_for_refuses_unadmitted() {
        assert!(policy_for("static").is_ok());
        assert!(policy_for("uniform").is_ok());
        assert!(policy_for("rules").is_ok());
        for v in [
            "bandit",
            "surface_prior",
            "predictor",
            "oracle_allocation",
            "bogus",
        ] {
            assert!(matches!(
                policy_for(v),
                Err(ComputeError::VariantNotAdmitted { .. })
            ));
        }
    }

    #[test]
    fn rules_extend_search_stops_at_agreement() {
        // AC-F4-6 — identical canonical answers from sample 1: once the
        // window fills, agreement ≥ θ → k stops growing.
        let mut ctx = ctx_base();
        ctx.ensemble = Some(EnsembleFact {
            k_max: 5,
            oracle: Some("executable".into()),
        });
        ctx.samples = (0..3)
            .map(|_| SampleFact {
                output_hash: "same".into(),
                verdicts: vec![],
            })
            .collect();
        let p = RulesPolicy::default();
        let rows = match p.bind(&propose(), &ctx).unwrap() {
            BindOutcome::Bound { record, .. } | BindOutcome::Unchanged { record } => {
                record.options_considered
            }
        };
        let es = rows
            .iter()
            .find(|r| r.option.kind == ComputeOptionKind::ExtendSearch)
            .unwrap();
        match &es.estimate {
            OptionEstimate::Estimate(e) => assert_eq!(e.delta_p.point_ppm, Some(0)),
            _ => panic!("expected an estimate"),
        }
    }

    #[test]
    fn rules_extend_search_runs_to_kmax_on_disagreement() {
        let mut ctx = ctx_base();
        ctx.ensemble = Some(EnsembleFact {
            k_max: 5,
            oracle: Some("executable".into()),
        });
        ctx.samples = (0..3)
            .map(|i| SampleFact {
                output_hash: format!("alt-{}", i % 2),
                verdicts: vec![],
            })
            .collect();
        let p = RulesPolicy::default();
        let out = p.bind(&propose(), &ctx).unwrap();
        match out {
            BindOutcome::Bound { record, decision } => {
                assert_eq!(record.chosen.kind, ComputeOptionKind::ExtendSearch);
                assert_eq!(record.binding.get("propose.sample_k"), Some(&Json::Int(4)));
                if let DecisionKind::Propose {
                    context_request, ..
                } = &decision.kind
                {
                    assert_eq!(context_request.get("sample_k"), Some(&Json::Int(4)));
                } else {
                    panic!("kind changed");
                }
            }
            _ => panic!("expected a bound decision"),
        }
    }

    #[test]
    fn rules_extend_search_unknown_without_reducer() {
        // AC-F4-6 — no reducer/validator ⇒ `unknown`, k unchanged.
        let mut ctx = ctx_base();
        ctx.ensemble = Some(EnsembleFact {
            k_max: 5,
            oracle: None,
        });
        ctx.samples = vec![SampleFact {
            output_hash: "h".into(),
            verdicts: vec![],
        }];
        let p = RulesPolicy::default();
        match p.bind(&propose(), &ctx).unwrap() {
            BindOutcome::Unchanged { record } => {
                let es = record
                    .options_considered
                    .iter()
                    .find(|r| r.option.kind == ComputeOptionKind::ExtendSearch)
                    .unwrap();
                match &es.estimate {
                    OptionEstimate::Estimate(e) => {
                        assert_eq!(e.delta_p.source, DeltaPSource::Unknown)
                    }
                    _ => panic!("expected an estimate"),
                }
            }
            _ => panic!("expected unchanged"),
        }
    }

    #[test]
    fn spawn_subagent_undeclared_parallelism() {
        // AC-F4-7 — no declared parallel steps ⇒ infeasible{undeclared_parallelism}.
        let p = RulesPolicy::default();
        match p.bind(&delegate(), &ctx_base()).unwrap() {
            BindOutcome::Unchanged { record } => {
                let row = record
                    .options_considered
                    .iter()
                    .find(|r| r.option.kind == ComputeOptionKind::SpawnSubagent)
                    .unwrap();
                assert_eq!(
                    row.estimate,
                    OptionEstimate::Infeasible(ComputeInfeasible::UndeclaredParallelism)
                );
            }
            _ => panic!("expected unchanged"),
        }
    }

    #[test]
    fn spawn_subagent_binds_slice_within_share() {
        let mut ctx = ctx_base();
        ctx.declared_parallel_steps = 2;
        let p = RulesPolicy::default();
        match p.bind(&delegate(), &ctx).unwrap() {
            BindOutcome::Bound { record, decision } => {
                assert_eq!(record.chosen.kind, ComputeOptionKind::SpawnSubagent);
                let bound =
                    ResourceVector::from_json(record.binding.get("delegate.budget_slice").unwrap())
                        .unwrap();
                // ≤ remaining ÷ (steps + 1) = 1_000_000/3.
                assert!(
                    bound.get(hh_ontology::dimensions::DimensionId::TokensOutputVisible) <= 333_333
                );
                if let DecisionKind::Delegate {
                    budget_slice,
                    delegation_reason,
                    ..
                } = &decision.kind
                {
                    assert!(ResourceVector::from_json(budget_slice).is_some());
                    assert!(delegation_reason.is_some());
                } else {
                    panic!("kind changed");
                }
            }
            _ => panic!("expected bound"),
        }
    }

    #[test]
    fn evaluator_requires_active_calibration() {
        // AC-F4-8 — calibration ≠ active ⇒ infeasible{calibration_inactive}.
        let mut ctx = ctx_base();
        ctx.placements = vec![PlacementFact {
            critic_ref: "critic/1".into(),
            calibration_status: hh_verification::vocab::CalibrationStatus::Expired,
            missed_failure_rate_ppm: 100_000,
            false_stop_rate_ppm: 10_000,
            verification_overhead: 10,
            uncaught_cost: 10_000,
        }];
        let p = RulesPolicy::default();
        match p.bind(&verify(), &ctx).unwrap() {
            BindOutcome::Unchanged { record } => {
                let row = record
                    .options_considered
                    .iter()
                    .find(|r| r.option.kind == ComputeOptionKind::Evaluator)
                    .unwrap();
                assert_eq!(
                    row.estimate,
                    OptionEstimate::Infeasible(ComputeInfeasible::CalibrationInactive)
                );
            }
            _ => panic!("expected unchanged"),
        }
    }

    #[test]
    fn evaluator_binds_on_positive_rule() {
        let mut ctx = ctx_base();
        ctx.placements = vec![PlacementFact {
            critic_ref: "critic/1".into(),
            calibration_status: hh_verification::vocab::CalibrationStatus::Active,
            missed_failure_rate_ppm: 900_000,
            false_stop_rate_ppm: 1_000,
            verification_overhead: 1,
            uncaught_cost: 100_000,
        }];
        let p = RulesPolicy::default();
        match p.bind(&verify(), &ctx).unwrap() {
            BindOutcome::Bound { record, decision } => {
                assert_eq!(record.chosen.kind, ComputeOptionKind::Evaluator);
                assert!(record
                    .rules_fired
                    .iter()
                    .any(|r| r == "compute_rule.evaluator_calibration"));
                if let DecisionKind::Verify { validator_refs, .. } = &decision.kind {
                    assert!(validator_refs.contains(&"critic/1".to_string()));
                    assert!(validator_refs.contains(&"v/base".to_string()));
                } else {
                    panic!("kind changed");
                }
            }
            _ => panic!("expected bound"),
        }
    }

    #[test]
    fn bind_is_pure() {
        // AC-F4-11 — identical (decision, ctx) ⇒ identical d′ and record
        // minus allocated ids (decision_id stays blank pre-stamp here).
        let mut ctx = ctx_base();
        ctx.declared_parallel_steps = 2;
        let p = RulesPolicy::default();
        let a = p.bind(&delegate(), &ctx).unwrap();
        let b = p.bind(&delegate(), &ctx).unwrap();
        match (a, b) {
            (
                BindOutcome::Bound {
                    decision: da,
                    record: ra,
                },
                BindOutcome::Bound {
                    decision: db,
                    record: rb,
                },
            ) => {
                assert_eq!(da, db);
                assert_eq!(ra, rb);
            }
            _ => panic!("expected bound"),
        }
    }

    #[test]
    fn check_bound_refuses_kind_change_and_widening() {
        // AC-F4-2 — a widened slice or changed kind is PolicyInvalid.
        let d = delegate();
        let mut bad = d.clone();
        if let DecisionKind::Delegate { budget_slice, .. } = &mut bad.kind {
            *budget_slice = Json::Null;
        }
        let mut wide = d.clone();
        if let DecisionKind::Delegate { budget_slice, .. } = &mut wide.kind {
            *budget_slice =
                rv(&[(hh_ontology::dimensions::DimensionId::TokensOutputVisible, 5)]).to_json();
        }
        // Unbound (null) → bound is admissible (it binds, doesn't widen).
        assert!(check_bound(&d, &wide, &ctx_base()).is_ok());
        let mut narrow_src = wide.clone();
        if let DecisionKind::Delegate { budget_slice, .. } = &mut narrow_src.kind {
            *budget_slice = rv(&[(
                hh_ontology::dimensions::DimensionId::TokensOutputVisible,
                10,
            )])
            .to_json();
        }
        assert!(check_bound(&narrow_src, &d, &ctx_base()).is_err());
        let mut kind_changed = d.clone();
        kind_changed.kind = DecisionKind::Wait {
            until: crate::vocab::WaitUntil::CueKind {
                cue_kind: "x".into(),
            },
        };
        assert!(check_bound(&d, &kind_changed, &ctx_base()).is_err());
    }

    #[test]
    fn check_bound_refuses_k_widening() {
        let mut d = propose();
        if let DecisionKind::Propose {
            context_request, ..
        } = &mut d.kind
        {
            *context_request = Json::obj([("sample_k", Json::Int(4))]);
        }
        let mut wide = d.clone();
        if let DecisionKind::Propose {
            context_request: Json::Obj(m),
            ..
        } = &mut wide.kind
        {
            m.insert("sample_k".to_string(), Json::Int(9));
        }
        assert!(check_bound(&d, &wide, &ctx_base()).is_err());
        let mut tight = d.clone();
        if let DecisionKind::Propose {
            context_request: Json::Obj(m),
            ..
        } = &mut tight.kind
        {
            m.insert("sample_k".to_string(), Json::Int(2));
        }
        assert!(check_bound(&d, &tight, &ctx_base()).is_ok());
    }

    #[test]
    fn compute_rule_delta_classifies_loosening() {
        // AC-F4-10 — raising the spend ceilings is loosening; rejected.
        assert_eq!(
            classify_compute_rule_delta("child_floor_ppm", 100_000, 200_000),
            BudgetDelta::Loosening
        );
        assert_eq!(
            classify_compute_rule_delta("k_max", 5, 8),
            BudgetDelta::Loosening
        );
        assert_eq!(
            classify_compute_rule_delta("budget_share_cap_ppm", 1, 2),
            BudgetDelta::Loosening
        );
        assert_eq!(
            classify_compute_rule_delta("theta_agree_ppm", 800_000, 700_000),
            BudgetDelta::Loosening
        );
        assert_eq!(
            classify_compute_rule_delta("theta_agree_ppm", 800_000, 900_000),
            BudgetDelta::Tightening
        );
        assert!(compute_rule_delta_admissible("k_max", 5, 8).is_err());
        assert!(compute_rule_delta_admissible("k_max", 5, 4).is_ok());
    }

    #[test]
    fn record_codec_round_trips() {
        let mut ctx = ctx_base();
        ctx.declared_parallel_steps = 2;
        let p = RulesPolicy::default();
        let record = match p.bind(&delegate(), &ctx).unwrap() {
            BindOutcome::Bound { record, .. } => record,
            _ => panic!("expected bound"),
        };
        let j = record.to_json();
        let back = ComputeDecisionRecord::from_json(&j).unwrap();
        assert_eq!(back, record);
        assert_eq!(back.compute_record_id(), record.record_id);
    }

    #[test]
    fn advise_defaults_to_none_at_c3() {
        // §5e.4 verb set: `advise` exists on the trait; every C3 variant
        // (`static`/`uniform`/`rules`) advises nothing — the default is
        // `None` and never an error (ADR-0188 D5).
        let ctx = ctx_base();
        for p in [
            Box::new(StaticPolicy) as Box<dyn ComputePolicy>,
            Box::new(UniformPolicy),
            Box::new(RulesPolicy::default()),
        ] {
            for point in DecisionPoint::ALL {
                assert_eq!(p.advise(point, &ctx), None);
            }
        }
    }

    #[test]
    fn effort_capability_unmet_without_declaration() {
        let mut ctx = ctx_base();
        ctx.ensemble = Some(EnsembleFact {
            k_max: 5,
            oracle: Some("executable".into()),
        });
        let p = RulesPolicy::default();
        match p.bind(&propose(), &ctx).unwrap() {
            BindOutcome::Unchanged { record } | BindOutcome::Bound { record, .. } => {
                let row = record
                    .options_considered
                    .iter()
                    .find(|r| r.option.kind == ComputeOptionKind::EffortLevel)
                    .unwrap();
                assert_eq!(
                    row.estimate,
                    OptionEstimate::Infeasible(ComputeInfeasible::CapabilityUnmet)
                );
            }
        }
    }

    #[test]
    fn effort_ladder_bound_and_step_up_rule() {
        // AC-R-2.3.3-15 — a step outside the declared ladder is refused; a
        // step-up is admissible only by the declared streak rule on a
        // per-message, non-invalidating profile.
        let mut ctx = ctx_base();
        ctx.profile_capabilities = Json::obj([(
            "effort",
            Json::obj([
                (
                    "ladder",
                    Json::Arr(vec![Json::str("low"), Json::str("high")]),
                ),
                ("per_message", Json::Bool(true)),
                ("cache_invalidation_on_change", Json::Bool(false)),
            ]),
        )]);
        let mut before = propose();
        if let DecisionKind::Propose {
            context_request, ..
        } = &mut before.kind
        {
            *context_request = Json::obj([("effort", Json::str("low"))]);
        }
        // A rung outside the ladder → refused.
        let mut off = before.clone();
        if let DecisionKind::Propose {
            context_request, ..
        } = &mut off.kind
        {
            *context_request = Json::obj([("effort", Json::str("turbo"))]);
        }
        assert!(check_bound(&before, &off, &ctx).is_err());
        // A step-up without the streak → refused.
        let mut up = before.clone();
        if let DecisionKind::Propose {
            context_request, ..
        } = &mut up.kind
        {
            *context_request = Json::obj([("effort", Json::str("high"))]);
        }
        assert!(check_bound(&before, &up, &ctx).is_err());
        // With the recorded streak (fail ≥ streak_n) on a per-message,
        // non-invalidating profile the step-up binds.
        ctx.validator_streaks.fail = RulesConfig::default().streak_n;
        assert!(check_bound(&before, &up, &ctx).is_ok());
        // …but never on a profile whose effort changes invalidate the cache.
        ctx.profile_capabilities = Json::obj([(
            "effort",
            Json::obj([
                (
                    "ladder",
                    Json::Arr(vec![Json::str("low"), Json::str("high")]),
                ),
                ("per_message", Json::Bool(true)),
                ("cache_invalidation_on_change", Json::Bool(true)),
            ]),
        )]);
        assert!(check_bound(&before, &up, &ctx).is_err());
        // A same-or-lower step always binds.
        let mut down_ctx = ctx_base();
        down_ctx.profile_capabilities = ctx.profile_capabilities.clone();
        let mut high_before = before.clone();
        if let DecisionKind::Propose {
            context_request, ..
        } = &mut high_before.kind
        {
            *context_request = Json::obj([("effort", Json::str("high"))]);
        }
        assert!(check_bound(&high_before, &before, &down_ctx).is_ok());
    }
}
