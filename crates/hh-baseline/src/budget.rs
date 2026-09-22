//! One root `BudgetNode` with hard ceilings + `check` at decision points (R-2.1.6; §8.2).
//! **Throwaway Stage-0 subset — a thin adapter over `hh_budget`/`hh_ontology` types
//! (CC1: one scheme per concern — the canonical dimension vocabulary, `BudgetSpec` and
//! `ResourceVector` are the only types; no second budget model is defined here).**
//!
//! AC-R-2.1.6-1: "one root `BudgetNode` with hard ceilings on `tokens.blended`, `model_calls`
//! and `time.wall_ms`; `check` at decision points; a run over its ceiling ends with
//! `stop_reason = budget_exhausted{dimension}` and no dangling intent". The ledger-bound
//! `Account`/`BudgetTree` machinery (reservations, containment, E1–E5) is `hh-budget`'s;
//! this adapter is the in-memory Stage-0 loop's view of the same canonical types.
//!
//! Charges post to the exclusive primary token roles (`tokens.input.uncached`,
//! `tokens.output.visible`); `tokens.blended` is the *derived* bound, evaluated by its
//! fixed `MeteringFormula` — never stored primary (§8.2; ADR-0039 D2).
//!
//! CC3 (nothing unaccounted): every producing event posts a charge here and the trace's
//! `cost_view` totals are read back *from this account*, so raw event totals and charged
//! totals are equal by construction.

use hh_budget::{
    BudgetMode, BudgetSpec, DerivedDimension, DimensionId, DimensionKey, ResourceVector,
};

/// The three Stage-0 hard-ceiling dimensions (§08 R-2.1.6 Stage 0 row: token roles,
/// `model_calls`, `time.wall_ms`). `TokensBlended` is the *derived* bound
/// (`tokens.input.total + tokens.output.total`) — its spellings and components are the
/// canonical registry's, not a baseline-local vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    TokensBlended,
    ModelCalls,
    TimeWallMs,
}

impl Dimension {
    /// The canonical bound key this Stage-0 name stands for (`hh_ontology::dimensions`).
    pub fn key(self) -> DimensionKey {
        match self {
            Dimension::TokensBlended => DimensionKey::Derived(DerivedDimension::TokensBlended),
            Dimension::ModelCalls => DimensionKey::Primary(DimensionId::ModelCalls),
            Dimension::TimeWallMs => DimensionKey::Primary(DimensionId::TimeWallMs),
        }
    }

    /// The canonical spelling (`tokens.blended` / `model_calls` / `time.wall_ms`).
    pub fn as_str(self) -> &'static str {
        self.key().as_str()
    }

    /// The three dimensions, in a stable order (used for total coverage tests).
    pub fn all() -> [Dimension; 3] {
        [
            Dimension::TokensBlended,
            Dimension::ModelCalls,
            Dimension::TimeWallMs,
        ]
    }

    /// The `Dimension` bucket a chargeable canonical dimension name sums into for the
    /// Stage-0 totals view: a token primary role sums into `TokensBlended` (its derived
    /// components), a bound name maps to itself.
    pub fn bucket_of(dim_name: &str) -> Option<Dimension> {
        match DimensionKey::parse(dim_name)? {
            DimensionKey::Primary(p) => {
                if DerivedDimension::TokensBlended.components().contains(&p) {
                    Some(Dimension::TokensBlended)
                } else {
                    Dimension::all()
                        .iter()
                        .copied()
                        .find(|d| d.key() == DimensionKey::Primary(p))
                }
            }
            DimensionKey::Derived(d) => Dimension::all()
                .iter()
                .copied()
                .find(|b| b.key() == DimensionKey::Derived(d)),
        }
    }
}

/// A run ended because a hard ceiling was reached. Carries the offending `dimension` so the
/// terminal `stop_reason = budget_exhausted{dimension}` is typed, never a bare flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    pub dimension: Dimension,
}

/// The single root budget node: the canonical [`BudgetSpec`] hard ceilings plus a
/// [`ResourceVector`] charge account over primary roles. There is exactly one per run at
/// Stage 0 (no `slice`/`pool` children — the ledger-bound tree is `hh-budget`'s).
#[derive(Debug, Clone)]
pub struct BudgetNode {
    spec: BudgetSpec,
    consumed: ResourceVector,
}

impl BudgetNode {
    /// Construct the one root budget with the three hard ceilings.
    pub fn root(tokens_blended: i64, model_calls: i64, time_wall_ms: i64) -> Self {
        let spec = BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[
                (Dimension::TokensBlended.key(), tokens_blended),
                (Dimension::ModelCalls.key(), model_calls),
                (Dimension::TimeWallMs.key(), time_wall_ms),
            ],
        );
        Self {
            spec,
            consumed: ResourceVector::zero(),
        }
    }

    /// Post a charge on a *primary* dimension (accounting; CC3). Derived bound names are
    /// never chargeable — token usage posts via [`BudgetNode::charge_tokens`].
    pub fn charge(&mut self, dim: Dimension, amount: i64) {
        let id = dim
            .key()
            .primary()
            .expect("a derived bound is never chargeable — use charge_tokens");
        self.consumed.add(id, amount);
    }

    /// Post token usage as the exclusive primary roles (§8.2 decomposition): uncached
    /// input + visible output. `tokens.blended` is derived from them, never stored.
    pub fn charge_tokens(&mut self, input: i64, output: i64) {
        self.consumed.add(DimensionId::TokensInputUncached, input);
        self.consumed.add(DimensionId::TokensOutputVisible, output);
    }

    /// Consumption on `dim` — for `TokensBlended` the derived formula evaluates over the
    /// stored primary roles (`ResourceVector::key_total`).
    pub fn consumed(&self, dim: Dimension) -> i64 {
        self.consumed.key_total(dim.key())
    }

    pub fn ceiling(&self, dim: Dimension) -> i64 {
        self.spec.hard(dim.key()).unwrap_or(0)
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

    /// The raw primary-role account (for tests/views that need the exclusive roles).
    pub fn account(&self) -> &ResourceVector {
        &self.consumed
    }
}

/// Map a trace charge event's `dimension` name into the Stage-0 totals bucket — canonical
/// spellings only (`Dimension::bucket_of`).
pub fn bucket_of(dim_name: &str) -> Option<Dimension> {
    Dimension::bucket_of(dim_name)
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
        b.charge_tokens(30, 10);
        b.charge_tokens(5, 0);
        assert_eq!(b.consumed(Dimension::TokensBlended), 45);
        b.charge(Dimension::ModelCalls, 1);
        assert_eq!(b.consumed(Dimension::ModelCalls), 1);
        assert_eq!(b.remaining(Dimension::TokensBlended), 55);
    }

    #[test]
    fn token_roles_post_as_exclusive_primaries() {
        // §8.2: the stored account is the exclusive roles; blended is derived.
        let mut b = BudgetNode::root(100, 5, 10_000);
        b.charge_tokens(30, 10);
        let v = b.account();
        assert_eq!(v.get(DimensionId::TokensInputUncached), 30);
        assert_eq!(v.get(DimensionId::TokensOutputVisible), 10);
        assert_eq!(v.key_total(DerivedDimension::TokensBlended.into()), 40);
    }

    #[test]
    fn tokens_ceiling_is_hard() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        b.charge_tokens(60, 40);
        assert_eq!(
            b.check(),
            Err(Exhausted {
                dimension: Dimension::TokensBlended
            })
        );
    }

    #[test]
    fn check_reports_the_exhausted_dimension() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        b.charge(Dimension::ModelCalls, 5);
        assert_eq!(
            b.check(),
            Err(Exhausted {
                dimension: Dimension::ModelCalls
            })
        );
    }

    #[test]
    fn dimension_ordering_is_deterministic_when_multiple_exhausted() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        b.charge_tokens(60, 50);
        b.charge(Dimension::ModelCalls, 5);
        assert_eq!(
            b.check(),
            Err(Exhausted {
                dimension: Dimension::TokensBlended
            })
        );
    }

    #[test]
    fn spellings_are_the_canonical_registry() {
        assert_eq!(Dimension::TokensBlended.as_str(), "tokens.blended");
        assert_eq!(Dimension::ModelCalls.as_str(), "model_calls");
        assert_eq!(Dimension::TimeWallMs.as_str(), "time.wall_ms");
        assert_eq!(
            bucket_of("tokens.input.uncached"),
            Some(Dimension::TokensBlended)
        );
        assert_eq!(bucket_of("tokens.blended"), Some(Dimension::TokensBlended));
        assert_eq!(bucket_of("not.a.dimension"), None);
    }

    #[test]
    fn derived_names_are_never_chargeable() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            b.charge(Dimension::TokensBlended, 1)
        }));
        assert!(r.is_err());
    }
}
