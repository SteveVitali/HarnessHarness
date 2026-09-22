//! One root `BudgetNode` with hard ceilings + `check` at decision points (R-2.1.6; §8.2).
//! **Throwaway Stage-0 subset.**
//!
//! AC-R-2.1.6-1: "one root `BudgetNode` with hard ceilings on `tokens.blended`, `model_calls`
//! and `time.wall_ms`; `check` at decision points; a run over its ceiling ends with
//! `stop_reason = budget_exhausted{dimension}` and no dangling intent". The full kernel
//! dimension list, `control.budget.*` events, `attribution`, `PricingTable` and reservations
//! land at S1.6; this is the minimal three-dimension hard-ceiling root the baseline needs.
//!
//! CC3 (nothing unaccounted): every producing event posts a charge here and the trace's
//! `cost_view` totals are read back *from this account*, so raw event totals and charged
//! totals are equal by construction.

use std::collections::BTreeMap;

/// The three Stage-0 hard-ceiling dimensions (§08 R-2.1.6 Stage 0 row: token roles,
/// `model_calls`, `time.wall_ms`). `tokens.blended` is the exclusive blended token counter
/// (input + output); the `TokenVector` view lands at S1.14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    TokensBlended,
    ModelCalls,
    TimeWallMs,
}

impl Dimension {
    pub fn as_str(self) -> &'static str {
        match self {
            Dimension::TokensBlended => "tokens.blended",
            Dimension::ModelCalls => "model_calls",
            Dimension::TimeWallMs => "time.wall_ms",
        }
    }

    /// The three dimensions, in a stable order (used for total coverage tests).
    pub fn all() -> [Dimension; 3] {
        [
            Dimension::TokensBlended,
            Dimension::ModelCalls,
            Dimension::TimeWallMs,
        ]
    }
}

/// A run ended because a hard ceiling was reached. Carries the offending `dimension` so the
/// terminal `stop_reason = budget_exhausted{dimension}` is typed, never a bare flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    pub dimension: Dimension,
}

/// The single root budget node: hard ceilings and a running per-dimension charge account.
/// There is exactly one per run at Stage 0 (no `slice`/`pool` children — those land at S1.6).
#[derive(Debug, Clone)]
pub struct BudgetNode {
    ceilings: BTreeMap<Dimension, i64>,
    consumed: BTreeMap<Dimension, i64>,
}

impl BudgetNode {
    /// Construct the one root budget with the three hard ceilings.
    pub fn root(tokens_blended: i64, model_calls: i64, time_wall_ms: i64) -> Self {
        let mut ceilings = BTreeMap::new();
        ceilings.insert(Dimension::TokensBlended, tokens_blended);
        ceilings.insert(Dimension::ModelCalls, model_calls);
        ceilings.insert(Dimension::TimeWallMs, time_wall_ms);
        let mut consumed = BTreeMap::new();
        for d in Dimension::all() {
            consumed.insert(d, 0);
        }
        Self { ceilings, consumed }
    }

    /// Post a charge (accounting; CC3). Every producing event calls this — a model call
    /// charges `tokens.blended` + `model_calls`, a tool call charges `time.wall_ms`.
    pub fn charge(&mut self, dim: Dimension, amount: i64) {
        *self.consumed.entry(dim).or_insert(0) += amount;
    }

    pub fn consumed(&self, dim: Dimension) -> i64 {
        *self.consumed.get(&dim).unwrap_or(&0)
    }

    pub fn ceiling(&self, dim: Dimension) -> i64 {
        *self.ceilings.get(&dim).unwrap_or(&0)
    }

    pub fn remaining(&self, dim: Dimension) -> i64 {
        self.ceiling(dim) - self.consumed(dim)
    }

    /// The decision-point check: called *before* the loop commits to another turn or tool
    /// intent. Returns `Exhausted{dimension}` for the first dimension at or over its ceiling,
    /// so no dangling intent is issued past exhaustion. Dimensions are checked in a stable
    /// order for determinism.
    pub fn check(&self) -> Result<(), Exhausted> {
        for dim in Dimension::all() {
            if self.consumed(dim) >= self.ceiling(dim) {
                return Err(Exhausted { dimension: dim });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_root_passes_check() {
        let b = BudgetNode::root(100, 5, 10_000);
        assert!(b.check().is_ok());
        assert_eq!(b.remaining(Dimension::ModelCalls), 5);
    }

    #[test]
    fn charge_accumulates_per_dimension() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        b.charge(Dimension::TokensBlended, 30);
        b.charge(Dimension::TokensBlended, 10);
        b.charge(Dimension::ModelCalls, 1);
        assert_eq!(b.consumed(Dimension::TokensBlended), 40);
        assert_eq!(b.consumed(Dimension::ModelCalls), 1);
    }

    #[test]
    fn check_reports_the_exhausted_dimension() {
        let mut b = BudgetNode::root(100, 2, 10_000);
        b.charge(Dimension::ModelCalls, 2);
        // AC-R-2.1.6-1: check at the decision point returns the typed dimension.
        assert_eq!(
            b.check(),
            Err(Exhausted {
                dimension: Dimension::ModelCalls
            })
        );
    }

    #[test]
    fn tokens_ceiling_is_hard() {
        let mut b = BudgetNode::root(50, 99, 10_000);
        b.charge(Dimension::TokensBlended, 50);
        assert_eq!(b.check().unwrap_err().dimension, Dimension::TokensBlended);
    }

    #[test]
    fn dimension_ordering_is_deterministic_when_multiple_exhausted() {
        // tokens is checked before model_calls; the first over-ceiling dimension wins.
        let mut b = BudgetNode::root(10, 1, 10);
        b.charge(Dimension::TokensBlended, 10);
        b.charge(Dimension::ModelCalls, 1);
        assert_eq!(b.check().unwrap_err().dimension, Dimension::TokensBlended);
    }
}
