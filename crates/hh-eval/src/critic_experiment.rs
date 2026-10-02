//! `critic_experiment` (AC-R-2.7.3-9; §5f.4) — the programmatic-critic
//! value measurement: `critic_experiment(definition, placement, design)`
//! runs the `{critic off, critic on}` arms under the caller's `MatchSpec`
//! (the `ArmSpec` halves) and reports the five required deltas —
//! `task_success`, `harness_overhead.verification`, `evaluator_calls`,
//! `false_stop_rate`, `missed_failure_rate` — as `ComparisonReport[]`.
//!
//! The measurement is a matched comparison, not a bespoke statistic: the
//! whole thing delegates to [`compare`] with the five metric names pinned,
//! so pairing, intervals, multiplicity and the deviation accounting are the
//! same machinery every other matched comparison uses. The delta
//! convention is `arm_a − arm_b` with `arm_a = critic_on` — a positive
//! `task_success` delta means the critic reads higher; a positive
//! `harness_overhead.verification` delta means the critic costs more
//! verification.
//!
//! The returned `removal_test` is the deterministic handle the critic's
//! `placement.removal_test` must point at — removing the critic's source
//! while keeping the placement declaration makes the experiment refuse to
//! resolve (the `critic_ref` no longer names a live definition), which is
//! the removable-or-dead enforcement the spec asks for (CC9/T-LCD-14).

use std::collections::BTreeMap;

use hh_budget::matchspec::ArmSpec;
use hh_ontology::compliance::MetricDeclaration;
use hh_ontology::eval::Design;

use hh_lab::analysis::{BenefitKind, ComparisonReport};

use crate::compare::{compare, CompareError, CompareInput, DeviationReport, TaskEffect};
use crate::runs::{EvalRun, TaskContext};

/// The arm ids — the experiment is exactly `{critic off, critic on}`.
pub const ARM_OFF: &str = "critic_off";
/// The critic-on arm id.
pub const ARM_ON: &str = "critic_on";

/// The five metrics the experiment must report, in catalogue order
/// (AC-R-2.7.3-9: delta task success, `harness_overhead.verification`,
/// `evaluator_calls`, `false_stop_rate`, `missed_failure_rate`).
pub const CRITIC_EXPERIMENT_METRICS: [&str; 5] = [
    "task_success",
    "harness_overhead.verification",
    "evaluator_calls",
    "false_stop_rate",
    "missed_failure_rate",
];

/// `critic_experiment(definition, placement, design)`'s inputs — the
/// `definition` is `critic_ref`, the `placement` names the decision point,
/// and `design` plus the two `ArmSpec`s pin the comparison the way every
/// other matched experiment does.
#[derive(Debug)]
pub struct CriticExperimentInput<'a> {
    /// The critic definition under test (`{kind}:{id}@{version}`) — the
    /// `definition` argument; rides `removal_test` so removing the critic
    /// while keeping its placement fails the experiment.
    pub critic_ref: &'a str,
    /// The placement's decision point (`"completion_gate"`,
    /// `"pre_completion"`, `"continuous"`, …) — the `placement` argument.
    pub placement: &'a str,
    /// The pinned design (`pairing`, `replicates_per_cell`, `seed_policy`,
    /// `routing_policy`, `deviation_policy`) — the `design` argument.
    pub design: &'a Design,
    /// The two arms' `ArmSpec`s in `[on, off]` (arm_a, arm_b) order — the
    /// MatchSpec the experiment runs under (`validate_match` is enforced
    /// by `compare`).
    pub arm_specs: &'a [ArmSpec],
    /// All runs (arms `"critic_off"` / `"critic_on"`).
    pub runs: &'a [EvalRun],
    /// The suite's task contexts.
    pub tasks: &'a [TaskContext],
    /// The metric catalogue rows — must cover
    /// [`CRITIC_EXPERIMENT_METRICS`] or the comparison refuses
    /// (`UnknownMetric`).
    pub declarations: &'a [MetricDeclaration],
    /// The interval confidence (ppm; `950_000` = 95 %).
    pub confidence_ppm: i64,
    /// The comparison's benefit kind.
    pub benefit_kind: BenefitKind,
    /// Whether the comparison ran over held-out splits.
    pub held_out: bool,
}

/// The experiment's outcome — the `ComparisonReport[]` the signature
/// promises, plus the per-task effect tables and the placement's
/// `removal_test` handle.
#[derive(Debug)]
pub struct CriticExperimentOutcome {
    /// One `ComparisonReport` per [`CRITIC_EXPERIMENT_METRICS`] row —
    /// `task_success`, `harness_overhead.verification`, `evaluator_calls`,
    /// `false_stop_rate`, `missed_failure_rate`.
    pub reports: Vec<ComparisonReport>,
    /// The per-task effect tables (parallel to `reports`).
    pub per_task: Vec<Vec<TaskEffect>>,
    /// The `placement.removal_test` handle —
    /// `critic_experiment:{critic_ref}:{placement}`. Pointing the placement
    /// at this string makes "remove the critic, keep the placement" a
    /// refusal: the handle's `critic_ref` no longer resolves.
    pub removal_test: String,
    /// The `routing.deviation` accounting.
    pub deviation: DeviationReport,
    /// `arm → outcome_class → count`.
    pub outcome_counts: BTreeMap<String, BTreeMap<String, u64>>,
}

/// `critic_experiment(definition, placement, design) → ComparisonReport[]`
/// — the `{on, off}` matched comparison over the five pinned metrics.
/// `varied_factor = "environment"`: the critic's presence is the harness
/// environment the arms differ on; every other compatibility field must
/// match or `compare` refuses.
pub fn critic_experiment(
    input: &CriticExperimentInput<'_>,
) -> Result<CriticExperimentOutcome, CompareError> {
    let metrics: Vec<String> = CRITIC_EXPERIMENT_METRICS
        .iter()
        .map(|m| (*m).to_string())
        .collect();
    let out = compare(&CompareInput {
        arm_a: ARM_ON,
        arm_b: ARM_OFF,
        metrics: &metrics,
        declarations: input.declarations,
        runs: input.runs,
        tasks: input.tasks,
        design: input.design,
        arm_specs: input.arm_specs,
        varied_factor: Some("environment"),
        confidence_ppm: input.confidence_ppm,
        benefit_kind: input.benefit_kind,
        held_out: input.held_out,
        family_size: Some(CRITIC_EXPERIMENT_METRICS.len() as u32),
    })?;
    Ok(CriticExperimentOutcome {
        reports: out.reports,
        per_task: out.per_task,
        removal_test: format!("critic_experiment:{}:{}", input.critic_ref, input.placement),
        deviation: out.deviation,
        outcome_counts: out.outcome_counts,
    })
}
