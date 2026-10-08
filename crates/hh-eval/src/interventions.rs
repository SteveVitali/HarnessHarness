//! `interventions_experiment` (AC-R-2.7.2b-4; §5f.3) — the Γ/probe/notice
//! value measurement: `interventions_experiment(design, arms, runs, …)` runs
//! the `{interventions_off, interventions_on}` arms under the caller's
//! `MatchSpec` halves and reports the pinned deltas — `task_success`,
//! `false_completion_rate`, `gate_hold_count`, `reconciliation_cost` — as
//! `ComparisonReport[]`, plus the model × harness interaction the
//! pre-registered **H-G2-1** hypothesis requires
//! ("interventions reduce `false_completion_rate` more for lower-tier
//! models and the gap shrinks with capability"; ADR-0114 D5).
//!
//! Two enforcements beyond `compare`'s own:
//!
//! - **matched cost** — every arm's `MatchSpec` must cover the
//!   intervention-cost dimensions (`evaluator_calls` for validator-run
//!   cost, `reconciliation_holds` for hold cost, the output-token
//!   dimensions probe/notice tokens ride). An arm whose MatchSpec omits
//!   them is refused `MatchUnderScoped` — a comparison that does not
//!   count the intervention's own spend is not the comparison the
//!   pre-registration named (AC-R-2.7.2b-4 "matched budget including
//!   probe and notice tokens").
//! - **the interaction** — when the caller supplies a second model tier's
//!   runs, `interventions_experiment` computes the H-G2-1 contrast:
//!   `contrast(false_completion_rate, lower_tier, higher_tier)` — the
//!   difference-in-differences of the two tiers' on−off deltas. The
//!   estimate is returned as `interaction`; an undefined contrast is a
//!   typed refusal, never a zero.
//!
//! The returned `removal_test` is the deterministic handle every Γ row
//! and probe rule points at (the §5f.3 Lab recipe): removing the Γ table
//! or a probe rule while keeping the experiment's bindings makes the
//! comparison refuse to resolve, which is the removable-or-dead
//! enforcement the spec asks for (CC9/T-LCD-14).

use std::collections::BTreeMap;

use hh_budget::matchspec::ArmSpec;
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::eval::Design;

use hh_lab::analysis::{BenefitKind, ComparisonReport};

use crate::compare::{
    compare, contrast, CompareError, CompareInput, CompareOutcome, ContrastEstimate,
    DeviationReport, TaskEffect,
};
use crate::runs::{EvalRun, TaskContext};

/// The arm ids — the experiment is exactly `{interventions off, on}`.
pub const ARM_OFF: &str = "interventions_off";
/// The interventions-on arm id.
pub const ARM_ON: &str = "interventions_on";

/// The metrics the experiment must report, in catalogue order
/// (AC-R-2.7.2b-4: success, `false_completion_rate`, holds, cost).
pub const INTERVENTIONS_METRICS: [&str; 4] = [
    "task_success",
    "false_completion_rate",
    "gate_hold_count",
    "reconciliation_cost",
];

/// The pre-registered hypothesis id (ADR-0114 D5): "interventions reduce
/// `false_completion_rate` more for lower-tier models and the gap shrinks
/// with capability" — measured by [`contrast`] over
/// `false_completion_rate` across the declared model tiers.
pub const H_G2_1: &str = "H-G2-1";

/// The interaction term the pre-registration names — the `factor_a:factor_b`
/// spelling `Design.pre_registration.interactions` consumes (S3.4a).
pub const H_G2_1_INTERACTION: &str = "interventions:model_snapshot";

/// The `MatchSpec` dimensions the intervention-cost match must cover —
/// `evaluator_calls` (validator-run cost), `reconciliation_holds` (hold
/// cost), `tokens.output.visible` + `model_calls` (the dims probe and
/// notice tokens ride). An arm omitting any of them is not running the
/// matched-budget comparison the pre-registration named.
pub const INTERVENTION_MATCH_DIMS: [DimensionId; 4] = [
    DimensionId::EvaluatorCalls,
    DimensionId::ReconciliationHolds,
    DimensionId::TokensOutputVisible,
    DimensionId::ModelCalls,
];

/// `interventions_experiment`'s typed refusals — `compare`'s own plus the
/// under-scoped-match refusal.
#[derive(Debug, Clone, PartialEq)]
pub enum InterventionsError {
    /// `compare`/`contrast` refused.
    Compare(CompareError),
    /// An arm's `MatchSpec` does not cover [`INTERVENTION_MATCH_DIMS`] —
    /// the probe/notice/validator-run/hold cost the matched budget must
    /// count (AC-R-2.7.2b-4).
    MatchUnderScoped {
        /// The offending arm.
        arm_id: String,
        /// The dimension names the MatchSpec lacks.
        missing: Vec<String>,
    },
}

impl std::fmt::Display for InterventionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterventionsError::Compare(e) => write!(f, "{e}"),
            InterventionsError::MatchUnderScoped { arm_id, missing } => write!(
                f,
                "arm {arm_id}: MatchSpec lacks intervention-cost dims {}",
                missing.join(", ")
            ),
        }
    }
}

impl std::error::Error for InterventionsError {}

impl From<CompareError> for InterventionsError {
    fn from(e: CompareError) -> Self {
        InterventionsError::Compare(e)
    }
}

/// `interventions_experiment(design, arms, runs, …)`'s inputs. `tier_b`
/// carries a second model tier's `{off, on}` arms when the experiment
/// reads the H-G2-1 interaction; absent it, the cell-level reports still
/// compute and `interaction` is `None`.
#[derive(Debug)]
pub struct InterventionsExperimentInput<'a> {
    /// The pinned design — its `pre_registration` must carry the H-G2-1
    /// interaction (`interventions:model_snapshot`) for a two-tier read.
    pub design: &'a Design,
    /// The two arms' `ArmSpec`s in `[on, off]` order — each `MatchSpec`
    /// must cover [`INTERVENTION_MATCH_DIMS`].
    pub arm_specs: &'a [ArmSpec],
    /// All runs (arms `"interventions_off"` / `"interventions_on"`).
    pub runs: &'a [EvalRun],
    /// The suite's task contexts.
    pub tasks: &'a [TaskContext],
    /// The metric catalogue rows — must cover [`INTERVENTIONS_METRICS`]
    /// or the comparison refuses (`UnknownMetric`).
    pub declarations: &'a [MetricDeclaration],
    /// The interval confidence (ppm; `950_000` = 95 %).
    pub confidence_ppm: i64,
    /// The comparison's benefit kind.
    pub benefit_kind: BenefitKind,
    /// Whether the comparison ran over held-out splits.
    pub held_out: bool,
    /// The Γ/probe-rule content ref the `removal_test` handle binds — the
    /// "disable Γ for model M" recipe's handle (AC-R-2.7.2b-5).
    pub removal_subject: &'a str,
    /// The second (higher-capability) model tier for the H-G2-1 contrast —
    /// `None` renders `interaction = None` (the cell reports still run).
    pub higher_tier: Option<InterventionsTier<'a>>,
}

/// A second model tier's half of the experiment — same arms, same tasks,
/// same design shape (the `contrast` pairs per task across the tiers).
#[derive(Debug)]
pub struct InterventionsTier<'a> {
    /// The tier label (rendered in the outcome for the report footer).
    pub label: &'a str,
    /// The tier's design.
    pub design: &'a Design,
    /// The tier's `[on, off]` `ArmSpec`s (same MatchSpec requirement).
    pub arm_specs: &'a [ArmSpec],
    /// The tier's runs.
    pub runs: &'a [EvalRun],
}

/// The experiment's outcome — the `ComparisonReport[]` plus the H-G2-1
/// contrast and the removal-test handle.
#[derive(Debug)]
pub struct InterventionsExperimentOutcome {
    /// One `ComparisonReport` per [`INTERVENTIONS_METRICS`] row —
    /// `task_success`, `false_completion_rate`, `gate_hold_count`,
    /// `reconciliation_cost`.
    pub reports: Vec<ComparisonReport>,
    /// The per-task effect tables (parallel to `reports`).
    pub per_task: Vec<Vec<TaskEffect>>,
    /// The H-G2-1 model × harness interaction — the per-task DiD of the
    /// two tiers' `false_completion_rate` on−off deltas (`None` without a
    /// `higher_tier`).
    pub interaction: Option<ContrastEstimate>,
    /// The higher tier's own comparison, when supplied (the contrast's
    /// other operand — kept so the report can render both halves).
    pub higher_tier_reports: Option<Vec<ComparisonReport>>,
    /// The `removal_test` handle — `interventions_experiment:{subject}`.
    /// Pointing every Γ row / probe rule at this string makes "remove the
    /// rule, keep the experiment" a refusal (T-LCD-05/CC9).
    pub removal_test: String,
    /// The `routing.deviation` accounting.
    pub deviation: DeviationReport,
    /// `arm → outcome_class → count`.
    pub outcome_counts: BTreeMap<String, BTreeMap<String, u64>>,
}

/// Whether an arm's `MatchSpec` covers every [`INTERVENTION_MATCH_DIMS`]
/// row; returns the missing dimension names.
pub fn match_gaps(spec: &ArmSpec) -> Vec<String> {
    let dims = match &spec.match_spec {
        Some(m) => &m.dimensions,
        None => {
            return INTERVENTION_MATCH_DIMS
                .iter()
                .map(|d| format!("{d:?}"))
                .collect();
        }
    };
    INTERVENTION_MATCH_DIMS
        .iter()
        .filter(|d| !dims.contains(d))
        .map(|d| format!("{d:?}"))
        .collect()
}

fn run_tier(
    design: &Design,
    arm_specs: &[ArmSpec],
    runs: &[EvalRun],
    input: &InterventionsExperimentInput<'_>,
) -> Result<CompareOutcome, InterventionsError> {
    // The matched-cost enforcement — every arm's MatchSpec must cover the
    // intervention-cost dims before `compare` sees the arms.
    for (i, spec) in arm_specs.iter().enumerate() {
        let missing = match_gaps(spec);
        if !missing.is_empty() {
            return Err(InterventionsError::MatchUnderScoped {
                arm_id: format!("arm[{i}]"),
                missing,
            });
        }
    }
    let metrics: Vec<String> = INTERVENTIONS_METRICS
        .iter()
        .map(|m| (*m).to_string())
        .collect();
    Ok(compare(&CompareInput {
        arm_a: ARM_ON,
        arm_b: ARM_OFF,
        metrics: &metrics,
        declarations: input.declarations,
        runs,
        tasks: input.tasks,
        design,
        arm_specs,
        // The Γ/probe/notice fabric is the harness *environment* the arms
        // differ on (same convention as `critic_experiment`); the
        // intervention factor itself rides the arms' `level_assignment`.
        varied_factor: Some("environment"),
        confidence_ppm: input.confidence_ppm,
        benefit_kind: input.benefit_kind,
        held_out: input.held_out,
        family_size: Some(INTERVENTIONS_METRICS.len() as u32),
        // Native component-level comparison — `search_budget` mandatory.
        granularity: hh_ontology::participant::Granularity::ConfigurationLevel,
    })?)
}

/// `interventions_experiment(design, arms, runs, …) → ComparisonReport[]`
/// — the `{on, off}` matched comparison over the four pinned metrics plus
/// the H-G2-1 `false_completion_rate` contrast when a higher tier is
/// supplied. `varied_factor = "interventions"`: the Γ/probe/notice fabric
/// is the harness factor the arms differ on; every other compatibility
/// field must match or `compare` refuses.
pub fn interventions_experiment(
    input: &InterventionsExperimentInput<'_>,
) -> Result<InterventionsExperimentOutcome, InterventionsError> {
    let base = run_tier(input.design, input.arm_specs, input.runs, input)?;
    let (interaction, higher_tier_reports) = match &input.higher_tier {
        None => (None, None),
        Some(tier) => {
            let upper = run_tier(tier.design, tier.arm_specs, tier.runs, input)?;
            // H-G2-1: the lower tier is `base`; the contrast is
            // `delta_lower(t) − delta_higher(t)` over `false_completion_rate`
            // — positive = interventions help the lower tier more.
            let est = contrast("false_completion_rate", &base, &upper, input.confidence_ppm)?;
            (Some(est), Some(upper.reports))
        }
    };
    Ok(InterventionsExperimentOutcome {
        reports: base.reports,
        per_task: base.per_task,
        interaction,
        higher_tier_reports,
        removal_test: format!("interventions_experiment:{}", input.removal_subject),
        deviation: base.deviation,
        outcome_counts: base.outcome_counts,
    })
}
