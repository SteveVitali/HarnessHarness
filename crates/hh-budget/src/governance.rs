//! The `budget_delta` governance gate (§8.2: "any `HirDiff.budget_delta =
//! loosening` MUST refuse in evolution contexts"; invariant X6 — the evolution
//! service edits only diffs with `budget_delta ∈ {none, tightening}`;
//! ADR-0053 D-5: evolution-authored loosening of `child_floor`, `k_max`,
//! `budget_share_cap` is rejected; `amend` is absent from the envelope's own
//! verb set — INV-7).
//!
//! The gate is a pure predicate over a [`DiffClassification`]: given *who
//! proposed* the diff and whether it runs **in an evolution context**, it
//! refuses budget-loosening edits. Human-authored and migration diffs may
//! loosen — that path is governance outside this ticket (the evolution gate is
//! what R-2.1.6's AC-13 requires: refused in evolution contexts).

use hh_hir::diff::{Delta, DiffClassification};

/// Who authored the `HirDiff` — the gate's context axis (`provenance.origin`
/// classes: `human`/`migration` origins may loosen with governance review;
/// evolution origins may not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposerOrigin {
    /// `origin = human` — may loosen; governed by the human change process.
    Human,
    /// `origin = migration` — may loosen; governed like `human`.
    Migration,
    /// An evolution-service-authored candidate — `budget_delta = loosening`
    /// refuses, always.
    Evolution,
}

/// The gate's refusal — `budget_delta = loosening` in an evolution context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetGateError {
    /// The offending delta (always `Loosening` here — `None`/`Tightening` pass).
    pub delta: Delta,
    /// The proposer origin that made it a refusal.
    pub origin: ProposerOrigin,
}

impl std::fmt::Display for BudgetGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HirDiff.budget_delta = loosening refused in an evolution context (origin: {:?})",
            self.origin
        )
    }
}

impl std::error::Error for BudgetGateError {}

/// `check_budget_delta(classification, origin) → ok` — refuses
/// `budget_delta = loosening` when `origin` is an evolution context
/// (`Evolution`). `Human`/`Migration` origins pass; `None`/`Tightening` deltas
/// pass for every origin. The caller (the evolution service's apply path —
/// §05h, Stage 5+) invokes this before applying any candidate diff; at Stage 1
/// the gate exists and is tested even though no evolution service runs yet.
pub fn check_budget_delta(
    classification: &DiffClassification,
    origin: ProposerOrigin,
) -> Result<(), BudgetGateError> {
    if classification.budget_delta == Delta::Loosening && origin == ProposerOrigin::Evolution {
        return Err(BudgetGateError {
            delta: classification.budget_delta,
            origin,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(delta: Delta) -> DiffClassification {
        DiffClassification {
            semantic_ops: 1,
            surface_ops: 0,
            provenance_only_ops: 0,
            ext_ops: 0,
            authority_delta: hh_hir::diff::AuthorityDelta::None,
            budget_delta: delta,
            validity_delta: Delta::None,
            coordination_delta: Delta::None,
            touches_conditioned_rules: vec![],
        }
    }

    #[test]
    fn loosening_refused_in_evolution_contexts() {
        assert!(check_budget_delta(&cls(Delta::Loosening), ProposerOrigin::Evolution).is_err());
    }

    #[test]
    fn tightening_and_none_pass_for_every_origin() {
        for d in [Delta::None, Delta::Tightening] {
            for o in [
                ProposerOrigin::Human,
                ProposerOrigin::Migration,
                ProposerOrigin::Evolution,
            ] {
                assert!(check_budget_delta(&cls(d), o).is_ok());
            }
        }
    }

    #[test]
    fn human_and_migration_origins_may_loosen() {
        for o in [ProposerOrigin::Human, ProposerOrigin::Migration] {
            assert!(check_budget_delta(&cls(Delta::Loosening), o).is_ok());
        }
    }
}
