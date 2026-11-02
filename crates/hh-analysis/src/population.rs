//! `population` — the producer-side reconcile (R5.15): the experiment
//! run's [`ExperimentView`] plans + the projected [`EvalRun`]s → the
//! `hh-eval` planned-population records (`eval::population::assemble`
//! does the contract validation; this module only maps — never a
//! second resolution rule, X7).
//!
//! Each `PlanState` becomes one `PlannedSlot`; its bound subjects come
//! from `attempts` ∪ `launch_binding` (the same subject→plan binding
//! `hh-results`' projection composes). `success` resolves the named
//! metric on the bound `EvalRun` (`bool`/`decimal`/`verdict-N`; a
//! lattice `P`/`I`/`n/a` or absent cell stays `None` — unknown is never
//! coerced). `availability` reads the row's admitted disposition:
//! `pending` → `Pending` (the `grade_pending` admission state —
//! R5.14b's consumer contract, consumed unchanged), `oracle_failure` →
//! `Unavailable`, a resolved outcome → `Resolved`, else `NotRun`.

use std::collections::BTreeMap;

use hharness_eval::population::{
    assemble, Availability, PlannedPopulation, PlannedSlot, PopulationInput, SlotSubject,
};
use hharness_eval::runs::{EvalRun, TaskContext};
use hharness_experiment::view::ExperimentView;
use hharness_ontology::eval::{LatticeValue, MetricValueKind};

use crate::error::AnalysisError;

/// `planned_population(view, runs, tasks, metric_ref, pins)` — fold the
/// pinned `ExperimentView` plus the projected `EvalRun`s into the R5.15
/// population contract. `pins` are the declared read watermarks
/// (`subject_run → seq`); the view's plans are the required universe.
pub fn planned_population(
    view: &ExperimentView,
    runs: &[EvalRun],
    tasks: &[TaskContext],
    metric_ref: &str,
    pins: &BTreeMap<String, u64>,
) -> Result<PlannedPopulation, AnalysisError> {
    let mut slots = Vec::with_capacity(view.plans.len());
    for (rpid, ps) in &view.plans {
        // Bound subjects: every recorded attempt plus the launch-binding
        // subject (dedup by run id — the attempt record wins; it carries
        // the settled outcome/veto axes).
        let mut subjects: Vec<SlotSubject> = ps
            .attempts
            .iter()
            .map(|a| {
                subject(
                    runs,
                    metric_ref,
                    pins,
                    a.run_id.as_str(),
                    a.attempt_no,
                    !a.superseded && a.excluded.is_none(),
                    a.outcome,
                    a.regrade_pending,
                )
            })
            .collect();
        if let Some(b) = &ps.launch_binding {
            if !ps.attempts.iter().any(|a| a.run_id == b.subject_run_id) {
                subjects.push(subject(
                    runs,
                    metric_ref,
                    pins,
                    b.subject_run_id.as_str(),
                    b.attempt_no,
                    true,
                    None,
                    false,
                ));
            }
        }
        let stratum = tasks
            .iter()
            .find(|t| t.task_id == ps.task_id)
            .map(|t| t.stratum.name().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        slots.push(PlannedSlot {
            plan_ref: rpid.clone(),
            task_id: ps.task_id.clone(),
            split_label: ps.split_label.clone(),
            stratum,
            arm_id: ps.arm_id.clone(),
            replicate_index: ps.replicate_index as u64,
            subjects,
        });
    }
    assemble(PopulationInput {
        slots,
        required: view.plans.keys().cloned().collect(),
        pins: pins.clone(),
    })
    .map_err(AnalysisError::Population)
}

/// One bound subject record: outcome/success from the bound `EvalRun`
/// where present (a pending row withholds `outcome_class` — it projects
/// no `EvalRun`, so the attempt's settled class is the fallback), else
/// the attempt's own folded facts.
fn subject(
    runs: &[EvalRun],
    metric_ref: &str,
    pins: &BTreeMap<String, u64>,
    run_id: &str,
    attempt_no: u32,
    admissible: bool,
    folded_outcome: Option<hharness_ontology::control::OutcomeClass>,
    regrade_pending: bool,
) -> SlotSubject {
    let run = runs.iter().find(|r| r.run_id == run_id);
    let pending = run
        .map(|r| r.disposition.as_deref() == Some("pending"))
        .unwrap_or(false)
        || regrade_pending;
    let outcome = run.map(|r| r.outcome_class).or(folded_outcome);
    let success = run.and_then(|r| match r.value_for(metric_ref) {
        MetricValueKind::Bool(b) => Some(b),
        MetricValueKind::Decimal(d) => Some(d > 0),
        MetricValueKind::Verdict(LatticeValue::N) => Some(false),
        _ => None,
    });
    let availability = if pending {
        Availability::Pending
    } else if outcome.is_some() {
        Availability::Resolved
    } else if run
        .map(|r| r.disposition.as_deref() == Some("oracle_failure"))
        .unwrap_or(false)
    {
        Availability::Unavailable
    } else {
        Availability::NotRun
    };
    SlotSubject {
        subject_run: run_id.to_string(),
        attempt_no,
        outcome,
        success,
        veto_tripped: run.map(|r| r.veto_tripped.clone()).unwrap_or_default(),
        availability,
        seq: pins.get(run_id).copied().unwrap_or(0),
        missing_required_checks: false,
        admissible,
    }
}
