//! The `CoEvolutionCycleRecord` driver (§5h.8 §6; R-2.9.8; S6.4;
//! ADR-0325): a **sidecar** over ordinary campaigns — the cycle never
//! opens a run of its own, mints no new event class, and introduces no
//! new `run_kind`/registry kind/design kind. Each phase's evidence is
//! the same `ComparisonReport`/experiment vocabulary the Lab already
//! owns; the record itself deposits under LabDocs kind
//! [`doc_kind::CO_EVOLUTION_CYCLE`] so a restart re-folds the sidecar
//! byte-for-byte (CC3).
//!
//! The driver's work is *bookkeeping*: `begin_phase`/`complete_phase`
//! advance the sidecar record, `next` plans the following phase
//! (an ordinary `campaign_open`/`export_training`/`import_snapshot`/
//! `consolidation` leg the caller executes), `stop` seals the terminal
//! verdict + `stop_reason`. `matched_total` is the only budget
//! arithmetic — the harness budgets are *charged*, the external weight
//! budget is *declared* (the trainer spends it, never the harness —
//! N9).

use hh_experiment::docs::LabDocs;
use hh_lab::coevolution::{
    self as coe, CoEvolutionCycleRecord, CyclePhase, CyclePolicy, CycleStopReason, PhasePlan,
};
use hh_wire::Json;

use crate::errors::EvolutionError;

/// The LabDocs kind the sidecar deposits under (`lab_doc.<kind>`
/// content addressing — one document store, CC3).
pub mod doc_kind {
    /// A `CoEvolutionCycleRecord` sidecar body.
    pub const CO_EVOLUTION_CYCLE: &str = "co_evolution_cycle";
}

/// `CycleDriver` — the thin durable wrapper over the pure cycle folds
/// in [`hh_lab::coevolution`]. The driver holds the *current* record;
/// every mutator re-seals it (the `cycle_id = version_id` re-derives —
/// an amendment is a new content address, the sidecar chain is the
/// audit trail).
#[derive(Debug, Clone)]
pub struct CycleDriver {
    /// The current sidecar record.
    pub record: CoEvolutionCycleRecord,
    /// The deposit ref of the last `deposit` (the doc address the
    /// caller restores from).
    pub deposit_ref: Option<String>,
}

impl CycleDriver {
    /// `open(policy, lineage_ref)` — mint the cycle's sidecar record.
    pub fn open(policy: CyclePolicy, lineage_ref: &str) -> CycleDriver {
        CycleDriver {
            record: coe::cycle_open(policy, lineage_ref),
            deposit_ref: None,
        }
    }

    /// `deposit(docs)` — write the current record under the LabDocs
    /// kind (returns the doc address — the restore key).
    pub fn deposit(&mut self, docs: &LabDocs) -> Result<String, EvolutionError> {
        let r = docs
            .put(doc_kind::CO_EVOLUTION_CYCLE, &self.record.to_json())
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.deposit_ref = Some(r.clone());
        Ok(r)
    }

    /// `restore(docs, ref)` — re-fold the deposited sidecar (the
    /// restart path — the record decodes member-strict; a torn/absent
    /// doc is `None`, never a fabricated record).
    pub fn restore(docs: &LabDocs, doc_ref: &str) -> Result<Option<CycleDriver>, EvolutionError> {
        let body = docs
            .get(doc_kind::CO_EVOLUTION_CYCLE, doc_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        let Some(body) = body else {
            return Ok(None);
        };
        let record = CoEvolutionCycleRecord::from_json(&body)
            .map_err(|e| EvolutionError::Schema(e.code()))?;
        Ok(Some(CycleDriver {
            record,
            deposit_ref: Some(doc_ref.to_string()),
        }))
    }

    /// `begin_phase(phase, inputs)` — open the next phase entry.
    pub fn begin_phase(&mut self, phase: CyclePhase, inputs: Json) -> Result<(), EvolutionError> {
        coe::cycle_begin_phase(&mut self.record, phase, inputs)
            .map_err(|e| EvolutionError::Schema(e.code()))
    }

    /// `complete_phase(outputs, experiment_ref?, training_run_ref?,
    /// verdict?, budgets)` — fill the open phase's outputs (the
    /// `matched_total` accounting runs in the pure fold).
    pub fn complete_phase(
        &mut self,
        outputs: Json,
        experiment_ref: Option<&str>,
        training_run_ref: Option<&str>,
        verdict: Option<&str>,
        budgets: Json,
    ) -> Result<(), EvolutionError> {
        coe::cycle_complete_phase(
            &mut self.record,
            outputs,
            experiment_ref,
            training_run_ref,
            verdict,
            budgets,
        )
        .map_err(|e| EvolutionError::Schema(e.code()))
    }

    /// `next()` — the deterministic phase plan (an ordinary leg the
    /// caller executes — `HarnessSearch` is a `campaign_open`,
    /// `WeightUpdate` the export→train→import seam, `ReEvaluation` the
    /// regression suite, `Consolidation` the proposal→retire leg).
    pub fn next(&self) -> PhasePlan {
        coe::cycle_next(&self.record)
    }

    /// `stop(reason, verdict)` — the terminal fold.
    pub fn stop(&mut self, reason: CycleStopReason, verdict: Option<&str>) {
        coe::cycle_stop(&mut self.record, reason, verdict);
    }
}
