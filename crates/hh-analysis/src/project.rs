//! `project` — the `ResultsRow + RunManifest + LedgerFacts → EvalRun`
//! projection (§6.4 §1: the kernel reads *results rows*; the eval-plane
//! `EvalRun/1` is the estimator's input record — CC7, one schema).
//!
//! Every field is sourced, never fabricated:
//!
//! - run/arm/cell/replicate/attempt/comparable — the row's `experiment`
//!   member (with `RunManifest.experiment` as the same fact's other
//!   projection; the row wins — it is the store's landed bytes);
//! - task/suite/split — `coordinates.task`, else `manifest.task_ref`; a
//!   row with neither is `MissingClusterKey` (ADR-0158: the task is the
//!   cluster, pairing key and resampling unit);
//! - `outcome_class` — the row's derived class; a row still `open`
//!   refuses `RowNotTerminal` (an open run has no outcome — never
//!   guessed);
//! - `values` — the row's `cells[]` verbatim, `MetricValueKind::Na`
//!   carried typed (never coerced to 0);
//! - `budget_consumed` — the `control.budget.consumed` fold on the row;
//! - `stratum`/`split_hash`/`split_label` — the records-in `TaskContext`
//!   (the suite plane's row); `environment_family` — the `SuiteContext`;
//!   both refuse `RowField` when unresolvable rather than guess;
//! - `seed_honoured` — `manifest.seed.is_some()` or a recorded
//!   `replicate.seed_material` (a bound seed is the honoured-seed
//!   evidence at Stage 3);
//! - `facts` — the run's `LedgerFacts::from_events` projection, supplied
//!   by the caller (the embed op streams the run's prefix; tests inject
//!   it directly);
//! - `eval_search_spend` — `0` at Stage 3: subject runs charge no search
//!   during the eval phase (the `artifact_benefit` zero-spend
//!   precondition reads this; a nonzero charge would come from a
//!   `search`-dimension consumed fold the Stage-3 store does not
//!   produce);
//! - `mediation`/`capability_vector` — empty: capability resolution is
//!   the C2 seam (ADR-0165); `applicability` treats absent entries as
//!   non-evidence, never `Supported`.

use std::collections::{BTreeMap, BTreeSet};

use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{CacheState, EvalRun, SuiteContext, TaskContext};
use hh_ledger::manifest::RunManifest;
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::eval::{MediationChannel, MetricValue};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{Observability, ParticipantClass};
use hh_results::row::ResultsRow;
use hh_wire::Json;

use crate::error::AnalysisError;

fn field_err(run_id: &str, field: &str, detail: impl Into<String>) -> AnalysisError {
    AnalysisError::RowField {
        run_id: run_id.to_string(),
        field: field.to_string(),
        detail: detail.into(),
    }
}

/// `eval_run(row, manifest, facts, task_ctx, suite_ctx)` — project one
/// head `ResultsRow` into the estimator's `EvalRun`.
///
/// `manifest`/`facts` come from the row's `derived_from` prefix
/// (`run_id + seq`); the caller resolves them (the embed op reads the
/// ledger store; the results store does not own manifests). `task_ctx` /
/// `suite_ctx` are the records-in context rows for the row's task/suite —
/// `None` is honoured where the field has a typed-unknown spelling and
/// refused where it does not.
pub fn eval_run(
    row: &ResultsRow,
    manifest: Option<&RunManifest>,
    facts: LedgerFacts,
    task_ctx: Option<&TaskContext>,
    suite_ctx: Option<&SuiteContext>,
) -> Result<EvalRun, AnalysisError> {
    let run_id = row.key.run_id.clone();

    // Open rows never project — an unfinished run has no outcome class
    // and nothing may be guessed.
    if row.outcome.status != "finished" {
        return Err(AnalysisError::RowNotTerminal { run_id });
    }
    let outcome_class = row
        .outcome
        .outcome_class
        .as_deref()
        .and_then(OutcomeClass::parse)
        .ok_or_else(|| field_err(&run_id, "outcome_class", "unparseable class"))?;

    // The experiment binding — the row's `experiment` member is the
    // landed projection; the manifest's binding corroborates it.
    let binding = manifest.and_then(|m| m.experiment.as_ref());
    let jexp = row.experiment.as_ref();
    let jget = |k: &str| -> Option<&Json> { jexp.and_then(|e| e.get(k)) };
    let arm_id = jget("arm_id")
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| binding.and_then(|b| b.arm_id.clone()))
        .unwrap_or_default();
    let cell_id = jget("cell_id")
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| binding.and_then(|b| b.cell_id.clone()));
    let replicate_index = jget("replicate_index")
        .and_then(Json::as_int)
        .map(|v| v as u64)
        .or_else(|| binding.and_then(|b| b.replicate_index))
        .unwrap_or(0);
    let attempt_no = jget("attempt_no")
        .and_then(Json::as_int)
        .map(|v| v as u64)
        .or_else(|| binding.and_then(|b| b.attempt_no))
        .unwrap_or(1);
    let comparable = jget("comparable")
        .and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        })
        .or_else(|| binding.and_then(|b| b.comparable))
        .unwrap_or(true);

    // Task coordinates — `coordinates.task`, else `manifest.task_ref`.
    let (task_id, suite_id, split_label) = {
        let from_row = row.coordinates.task.as_ref().and_then(|t| {
            Some((
                t.get("task_id")?.as_str()?.to_string(),
                t.get("suite_id")?.as_str()?.to_string(),
                SplitLabel::parse(t.get("split_label")?.as_str()?)?,
            ))
        });
        match from_row {
            Some(t) => t,
            None => {
                let tr = manifest.and_then(|m| m.task_ref.as_ref()).ok_or_else(|| {
                    AnalysisError::MissingClusterKey {
                        run_id: run_id.clone(),
                    }
                })?;
                (tr.task_id.clone(), tr.suite_id.clone(), tr.split_label)
            }
        }
    };

    let participant_class = ParticipantClass::parse(&row.coordinates.participant_class)
        .ok_or_else(|| field_err(&run_id, "participant_class", "unparseable class"))?;
    let mut observability_level = BTreeSet::new();
    for o in &row.coordinates.observability_level {
        observability_level.insert(
            Observability::parse(o)
                .ok_or_else(|| field_err(&run_id, "observability_level", "unparseable"))?,
        );
    }

    // The environment family resolves from the declared suite context,
    // else the manifest's `extra.environment_family` — never guessed.
    let environment_family = suite_ctx
        .map(|s| s.family)
        .or_else(|| {
            manifest
                .and_then(|m| m.extra.get("environment_family"))
                .and_then(Json::as_str)
                .and_then(EnvironmentFamily::parse)
        })
        .ok_or_else(|| {
            field_err(
                &run_id,
                "environment_family",
                "no suite context and no manifest environment_family",
            )
        })?;

    let cache_state = match row
        .cache
        .get("policy")
        .and_then(Json::as_str)
        .unwrap_or("cold_start")
    {
        "natural" => CacheState::Natural,
        "primed" => CacheState::Primed,
        _ => CacheState::ColdStart,
    };

    let seed = manifest.and_then(|m| m.seed).or_else(|| {
        row.coordinates
            .replicate
            .get("seed")
            .and_then(Json::as_int)
            .map(|v| v as u64)
    });
    let seed_honoured = seed.is_some()
        || row
            .coordinates
            .replicate
            .get("seed_material")
            .and_then(Json::as_str)
            .is_some();

    let mut budget_consumed = BTreeMap::new();
    for (d, v) in &row.consumption.dimensions {
        let dim = DimensionId::parse(d)
            .ok_or_else(|| field_err(&run_id, "consumption", format!("unknown dim {d}")))?;
        budget_consumed.insert(dim, *v);
    }

    let values: Vec<MetricValue> = row
        .cells
        .iter()
        .map(|c| MetricValue {
            metric_ref: c.metric_ref.clone(),
            value: c.value.clone(),
            applies_to: run_id.clone(),
            oracle_ref: c.oracle_ref.clone().unwrap_or_default(),
            // Stage-3 detectors are deterministic; an undeclared cell
            // detector is read as deterministic (judged/human classes
            // enter at C2 and would be declared on the cell).
            detector: c
                .detector
                .as_deref()
                .and_then(hh_ontology::compliance::Detector::parse)
                .unwrap_or(hh_ontology::compliance::Detector::Deterministic),
            confidence: c.confidence,
            evidence_ref: c.evidence.first().map(|e| e.hash.clone()),
        })
        .collect();

    let stratum = task_ctx
        .map(|t| t.stratum)
        .unwrap_or(ContaminationStratum::Unknown);
    let split_hash = task_ctx.map(|t| t.split_hash.clone());

    Ok(EvalRun {
        run_id,
        arm_id,
        cell_id,
        configuration_id: row
            .coordinates
            .configuration_id
            .clone()
            .unwrap_or_else(|| row.key.configuration_version_id.clone()),
        participant_class,
        observability_level,
        // §6.6 / ADR-0165 D6 — the row's mediation channels + capability vector
        // feed `applicability_at` (a native row's surface is fully mediated —
        // the kernel owns every channel; a hosted row carries the row's
        // declared spellings, never defaulted, T-LCD-07). The capability
        // vector resolves through `capability_vector_ref` (Stage 4 binding —
        // absent = empty, and `requires_capabilities` metrics render
        // `n/a{capability}` rather than a coerced verdict).
        mediation: if participant_class == ParticipantClass::Native {
            [
                MediationChannel::Effects,
                MediationChannel::Egress,
                MediationChannel::ModelCalls,
            ]
            .into_iter()
            .collect()
        } else {
            row.coordinates
                .mediation
                .iter()
                .filter_map(|s| MediationChannel::parse(s))
                .collect()
        },
        capability_vector: BTreeMap::new(),
        task_id,
        suite_id,
        split_label,
        replicate_index,
        attempt_no,
        seed,
        seed_honoured,
        cache_state,
        comparable,
        outcome_class,
        budget_consumed,
        veto_tripped: row.outcome.veto_tripped.clone(),
        values,
        environment_version_id: row.coordinates.environment_version_id.clone(),
        environment_family,
        fault_profile: manifest
            .and_then(|m| m.extra.get("fault_profile"))
            .and_then(Json::as_str)
            .map(str::to_string),
        perturbation_profile: manifest
            .and_then(|m| m.extra.get("perturbation_profile"))
            .and_then(Json::as_str)
            .map(str::to_string),
        model_snapshots: row.coordinates.model_snapshots.clone(),
        stratum,
        eval_search_spend: 0,
        split_hash,
        facts,
    })
}
