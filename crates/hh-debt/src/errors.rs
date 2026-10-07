//! The closed `hh-debt` error table (S6.1b; §5h.6 §5 — every refusal is
//! typed, never a warning). `DebtManagerError::{Store, Schema}` carry the
//! substrate/schema halves; [`Refusal`] is the closed debt-manager refusal
//! set — every member maps onto a snake_case `code` the boundary surfaces
//! as `Refused{reason}`.

use hh_ledger::errors::LedgerError;

/// One manager-level refusal (the codes the `lifecycle.debt.*` rows and the
/// `lab.debt.*` boundary surface).
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// `RetirementNotEvidenced` — a removal without a `pass` verdict on the
    /// debt (the §5h.6 §2 `retire`/`propose_retirement` gate; AC-R-2.9.6-4).
    RetirementNotEvidenced {
        /// The debt the removal targeted.
        debt_ref: String,
    },
    /// `NotARetirementDiff` — the proposal's diff is not a retirement diff
    /// (it widens authority, loosens a budget, or removes something other
    /// than exactly the debt's rule).
    NotARetirementDiff {
        /// The offending detail.
        detail: String,
    },
    /// The record's removal test does not instantiate (`UnexecutableRemovalTest`
    /// at schedule time — the member-level half `hh_hir::debt` already ran).
    NotExecutable {
        /// The debt whose test cannot instantiate.
        debt_ref: String,
        /// The missing payload.
        reason: String,
    },
    /// `UnknownManager` — the manager id has no `service.registered` row in
    /// the durable prefix.
    UnknownManager {
        /// The unregistered id.
        manager_id: String,
    },
    /// `Unsupported{priority}` — the `DebtPolicy.priority` spelling names an
    /// ordering the manager does not implement (a closed vocabulary — the
    /// default `expiry_urgency` order only).
    Unsupported {
        /// The offending member/spelling.
        detail: String,
    },
    /// The batch inputs are incompatible (different base arms/designs/suites —
    /// a `retirement_batch` shares exactly one base arm).
    BatchIncompatible {
        /// The offending detail.
        detail: String,
    },
}

impl Refusal {
    /// The snake_case code the boundary surfaces (`Refused{reason}`).
    pub fn code(&self) -> String {
        match self {
            Refusal::RetirementNotEvidenced { .. } => "retirement_not_evidenced".to_string(),
            Refusal::NotARetirementDiff { .. } => "not_a_retirement_diff".to_string(),
            Refusal::NotExecutable { .. } => "not_executable".to_string(),
            Refusal::UnknownManager { .. } => "unknown_manager".to_string(),
            Refusal::Unsupported { .. } => "unsupported".to_string(),
            Refusal::BatchIncompatible { .. } => "batch_incompatible".to_string(),
        }
    }
}

/// The crate error — `Refusal` for typed refusals, `Schema`/`Store` for the
/// substrate halves.
#[derive(Debug)]
pub enum DebtManagerError {
    /// A typed refusal (durable-row-worthy — the caller records it).
    Refusal(Refusal),
    /// A schema/codec violation on an input record.
    Schema(String),
    /// A ledger substrate error.
    Store(LedgerError),
}

impl DebtManagerError {
    /// The boundary-mapped code.
    pub fn code(&self) -> String {
        match self {
            DebtManagerError::Refusal(r) => r.code(),
            DebtManagerError::Schema(_) => "schema_violation".to_string(),
            DebtManagerError::Store(_) => "store_error".to_string(),
        }
    }
}

impl From<Refusal> for DebtManagerError {
    fn from(r: Refusal) -> DebtManagerError {
        DebtManagerError::Refusal(r)
    }
}

impl From<LedgerError> for DebtManagerError {
    fn from(e: LedgerError) -> DebtManagerError {
        DebtManagerError::Store(e)
    }
}
