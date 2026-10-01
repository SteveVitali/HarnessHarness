//! The typed error sums for the §8.2 ops — every refusal is a named member, never a
//! string and never a warning (T-LCD-14 discipline extended to accounting).
//!
//! The spellings are the spec's: `charge` → `UnknownBudget`, `DimensionNotBudgetable`,
//! `Fenced`; `allocate` → `BudgetExceedsParent`, `DimensionUnknown`; `reserve` →
//! `InsufficientBudget{dimension, requested, available}`; `amend` →
//! `AuthorityInsufficient`; `attribute_spend` → `NoPrice`, `MixedCurrency`;
//! `validate_match` → the closed refusal sum [`MatchRefusal`].

use hh_ledger::LedgerError;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use std::fmt;

/// The budget/accounting error sum (§8.2 §2 error columns).
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetError {
    /// `charge`/`reserve`/`check`/`amend` named a budget that does not exist.
    UnknownBudget { budget_id: String },
    /// A `charge` named a non-counter dimension (a gauge or a derived name) — gauges
    /// refuse the next increment (E5) and derived names are never stored primary.
    DimensionNotBudgetable { dimension: String },
    /// `allocate` named a bound key outside the kernel registry (primary or derived).
    DimensionUnknown { dimension: String },
    /// A second root budget was attempted — one root `BudgetNode` per run.
    DuplicateRoot { existing: String },
    /// `child.hard ≤ parent.remaining` failed dimension-wise at allocation.
    BudgetExceedsParent {
        dimension: DimensionKey,
        child_limit: i64,
        parent_remaining: i64,
    },
    /// A bound key carried a `soft` threshold above its `hard` ceiling.
    SoftAboveHard { dimension: DimensionKey },
    /// `reserve` could not claim `quantity` — `Σ reservations + consumed ≤ hard` along
    /// the ancestor chain is the conservation law applied to reservations
    /// (ADR-0040 D3). A typed pre-dispatch refusal.
    InsufficientBudget {
        dimension: DimensionKey,
        requested: i64,
        available: i64,
    },
    /// `charge`/`release` named a reservation that does not exist or is already
    /// released.
    UnknownReservation { reservation_id: String },
    /// `amend` without the required authority — root: operator/human (`principal` or
    /// `kernel`); child: must keep new bounds within the parent's remaining; host
    /// overrides tighten only.
    AuthorityInsufficient { detail: String },
    /// `charge`'s `source` did not resolve to a committed event in this run.
    UnknownSourceEvent { event_id: String },
    /// E1 — `exhaust` was invoked while effects are still open; the committed effects
    /// must reach a terminal or `unknown` first (drain before `exceeded`).
    EffectsInFlight { open: Vec<String> },
    /// E2 — a stop decision already exists for this run (a stop decision is made
    /// exactly once); carries the existing decision's event ref.
    AlreadyStopped { decision_event_id: String },
    /// E3 — the per-dimension grace allowance is exhausted; `grace` is never a ceiling
    /// widening.
    GraceExhausted {
        budget_id: String,
        dimension: DimensionKey,
        max_calls: u32,
    },
    /// E5 — a gauge increment would exceed its cap (`SpawnRefused`/`CompactionRequired`
    /// territory): refused, never a charge.
    GaugeCapExceeded {
        budget_id: String,
        dimension: DimensionId,
        value: i64,
        cap: i64,
    },
    /// The spend/pricing layer refused.
    Spend(SpendError),
    /// The usage-mapping lower refused.
    Usage(UsageError),
    /// The ledger refused (fencing, schema, …). `Fenced` rides inside.
    Ledger(LedgerError),
    /// The event payload could not be decoded into the expected shape (projection).
    CorruptPayload { detail: String },
}

impl fmt::Display for BudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetError::UnknownBudget { budget_id } => {
                write!(f, "UnknownBudget: {budget_id}")
            }
            BudgetError::DimensionNotBudgetable { dimension } => {
                write!(f, "DimensionNotBudgetable: {dimension}")
            }
            BudgetError::DimensionUnknown { dimension } => {
                write!(f, "DimensionUnknown: {dimension}")
            }
            BudgetError::DuplicateRoot { existing } => {
                write!(f, "DuplicateRoot: {existing} is already the run root")
            }
            BudgetError::BudgetExceedsParent {
                dimension,
                child_limit,
                parent_remaining,
            } => write!(
                f,
                "BudgetExceedsParent: {dimension} child {child_limit} > parent remaining {parent_remaining}"
            ),
            BudgetError::SoftAboveHard { dimension } => {
                write!(f, "SoftAboveHard: {dimension}")
            }
            BudgetError::InsufficientBudget {
                dimension,
                requested,
                available,
            } => write!(
                f,
                "InsufficientBudget: {dimension} requested {requested} > available {available}"
            ),
            BudgetError::UnknownReservation { reservation_id } => {
                write!(f, "UnknownReservation: {reservation_id}")
            }
            BudgetError::AuthorityInsufficient { detail } => {
                write!(f, "AuthorityInsufficient: {detail}")
            }
            BudgetError::UnknownSourceEvent { event_id } => {
                write!(f, "UnknownSourceEvent: {event_id}")
            }
            BudgetError::EffectsInFlight { open } => {
                write!(f, "EffectsInFlight: {} open ({})", open.len(), open.join(", "))
            }
            BudgetError::AlreadyStopped {
                decision_event_id,
            } => write!(f, "AlreadyStopped: decision {decision_event_id}"),
            BudgetError::GraceExhausted {
                budget_id,
                dimension,
                max_calls,
            } => write!(
                f,
                "GraceExhausted: {budget_id} {dimension} already used {max_calls} grace calls"
            ),
            BudgetError::GaugeCapExceeded {
                budget_id,
                dimension,
                value,
                cap,
            } => write!(
                f,
                "GaugeCapExceeded: {budget_id} {} {value} > cap {cap}",
                dimension.as_str()
            ),
            BudgetError::Spend(e) => write!(f, "{e}"),
            BudgetError::Usage(e) => write!(f, "{e}"),
            BudgetError::Ledger(e) => write!(f, "{e}"),
            BudgetError::CorruptPayload { detail } => {
                write!(f, "CorruptPayload: {detail}")
            }
        }
    }
}

impl std::error::Error for BudgetError {}

impl From<LedgerError> for BudgetError {
    fn from(e: LedgerError) -> BudgetError {
        BudgetError::Ledger(e)
    }
}

impl From<SpendError> for BudgetError {
    fn from(e: SpendError) -> BudgetError {
        BudgetError::Spend(e)
    }
}

impl From<UsageError> for BudgetError {
    fn from(e: UsageError) -> BudgetError {
        BudgetError::Usage(e)
    }
}

/// `attribute_spend` refusals (§8.2 `attribute_spend` error column).
#[derive(Debug, Clone, PartialEq)]
pub enum SpendError {
    /// No pricing row covers this model/role — never zero, never a silent fallback.
    NoPrice { model_ref: String, detail: String },
    /// A sum over rows in more than one currency — never summed across currencies.
    MixedCurrency { a: String, b: String },
    /// The declared `confidence`/`coverage`/`provenance` stamps are inconsistent with
    /// what was measured (`exact` requires `measured` and `coverage = 1`).
    ConfidenceViolation { detail: String },
}

impl fmt::Display for SpendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpendError::NoPrice { model_ref, detail } => {
                write!(f, "NoPrice: {model_ref} ({detail})")
            }
            SpendError::MixedCurrency { a, b } => {
                write!(f, "MixedCurrency: {a} vs {b}")
            }
            SpendError::ConfidenceViolation { detail } => {
                write!(f, "ConfidenceViolation: {detail}")
            }
        }
    }
}

impl std::error::Error for SpendError {}

/// A `usage_mapping` lower refused.
#[derive(Debug, Clone, PartialEq)]
pub enum UsageError {
    /// The declared dialect cannot decompose the raw payload (e.g. an inclusive `input`
    /// smaller than the sum of its reported cached roles) — the negative residual would
    /// be a lie, so the lower refuses rather than clamping.
    InconsistentRaw { detail: String },
    /// A negative or otherwise invalid quantity.
    InvalidQuantity { detail: String },
    /// The dialect itself is unknown to this kernel.
    UnknownDialect { dialect: String },
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::InconsistentRaw { detail } => {
                write!(f, "InconsistentRaw: {detail}")
            }
            UsageError::InvalidQuantity { detail } => {
                write!(f, "InvalidQuantity: {detail}")
            }
            UsageError::UnknownDialect { dialect } => {
                write!(f, "UnknownDialect: {dialect}")
            }
        }
    }
}

impl std::error::Error for UsageError {}

/// `validate_match`/`search_budget`/`eval_budget` failure — carries the closed refusal
/// sum member (§8.2 `validate_match` error column; ADR-0041 D6).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchError {
    /// The refused arm index (when the refusal names one).
    pub arm: Option<usize>,
    /// The closed refusal member.
    pub refusal: MatchRefusal,
}

/// The closed refusal sum — refusal, never a warning (ADR-0041 D6; T-LCD-14).
#[derive(Debug, Clone, PartialEq)]
pub enum MatchRefusal {
    /// An arm lacks `search_budget`/`eval_budget`.
    UnbudgetedArm,
    /// The arms are not comparable — carries the spec's
    /// `{arm, dimension, enforceability}` triple where applicable.
    IncommensurableMatch {
        /// The reason the match is incommensurable.
        reason: RefusalReason,
        /// The dimension the refusal names, if any.
        dimension: Option<DimensionId>,
        /// The enforceability the refusal names, if any.
        enforceability: Option<EnforcementLevel>,
    },
    /// Spend matching without one pinned `PricingTable` version.
    MissingPricingTable,
    /// An arm carries no `MatchSpec`.
    MissingMatchSpec,
}

/// Why a match is incommensurable (the named mechanisms of §8.2 §5 + ADR-0041).
#[derive(Debug, Clone, PartialEq)]
pub enum RefusalReason {
    /// Token roles matched across models — ill-defined (OQ-094); spend at a pinned
    /// table is the only cross-model common unit.
    CrossModelTokenMatch,
    /// The matched dimension is not admissible under `cross_model` scope (admissible:
    /// `spend`, `time.*`, `model_calls`, `tool_calls`, `approvals.*`).
    CrossModelDimension,
    /// The arms declare different `MeteringFormula`s for a matched dimension.
    MixedMeteringFormulas,
    /// Mixed `cache_policy` across arms (ADR-0041 P2 amendment).
    MixedCachePolicies,
    /// A `matched_cap` arm can only enforce the dimension `advisory`/`unenforceable`
    /// (ADR-0165 D3), or a `matched_total` arm has an `unenforceable` dimension.
    Unenforceable,
    /// M2 over `spend` with an arm whose spend confidence is `estimate`/`unknown` —
    /// those rows render bands, never points (ADR-0041 P3 log).
    SpendConfidenceTooLow,
    /// M3 totals (`search + eval + inference`) differ across arms.
    UnequalTotals,
    /// `dimensions` empty under a matching mode.
    NoDimensions,
    /// `MatchSpec{mode: none}` — exploratory; executes, never reports a comparison.
    ExploratoryNoMatch,
    /// The arms' `MatchSpec.mode` values differ.
    MixedMatchModes,
    /// The arms' `model_scope` values differ.
    MixedModelScopes,
    /// The arms' matched `dimensions` differ.
    MixedDimensions,
    /// The arms name different pinned `PricingTable`s.
    MixedPricingTables,
    /// M1: the arms' hard ceilings on a matched dimension differ beyond `tolerance`.
    UnequalCaps,
    /// The comparison precondition — `eval_budget` differs across arms.
    UnequalEvalBudgets,
}

/// The `budget_enforcement` level an arm can apply to a dimension (ADR-0165 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnforcementLevel {
    /// Kernel-enforced (native arms are trivially `enforced`).
    Enforced,
    /// Advisory only — renders, does not stop.
    Advisory,
    /// Cannot be enforced or observed at this hosting mechanism.
    Unenforceable,
}

impl EnforcementLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            EnforcementLevel::Enforced => "enforced",
            EnforcementLevel::Advisory => "advisory",
            EnforcementLevel::Unenforceable => "unenforceable",
        }
    }

    /// Parse a canonical spelling (`Option` — unknown spellings refuse).
    pub fn parse(s: &str) -> Option<EnforcementLevel> {
        Some(match s {
            "enforced" => EnforcementLevel::Enforced,
            "advisory" => EnforcementLevel::Advisory,
            "unenforceable" => EnforcementLevel::Unenforceable,
            _ => return None,
        })
    }
}

impl fmt::Display for MatchRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchRefusal::UnbudgetedArm => write!(f, "UnbudgetedArm"),
            MatchRefusal::IncommensurableMatch {
                reason,
                dimension,
                enforceability,
            } => write!(
                f,
                "IncommensurableMatch{{reason: {:?}, dimension: {:?}, enforceability: {:?}}}",
                reason,
                dimension.map(|d| d.as_str()),
                enforceability.map(|e| e.as_str())
            ),
            MatchRefusal::MissingPricingTable => write!(f, "MissingPricingTable"),
            MatchRefusal::MissingMatchSpec => write!(f, "MissingMatchSpec"),
        }
    }
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.arm {
            Some(a) => write!(f, "arm {a}: {}", self.refusal),
            None => write!(f, "{}", self.refusal),
        }
    }
}

impl std::error::Error for MatchError {}
