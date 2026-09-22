//! `hh-budget` — the C0/Stage-1 resource-economics & accounting model
//! (spec §8.2, scope item **R-2.1.6**; ADR-0039/0040/0041 as amended; ADR-0043 for the
//! measurement stamps). The single canonical schema + accounting substrate for:
//!
//! - the **closed kernel dimension registry** ([`hh_ontology::dimensions`], re-exported)
//!   — 24 counters + 3 gauges + 5 registered `ext`/`hh` names, plus the three derived
//!   bound names (`tokens.input.total`, `tokens.output.total`, `tokens.blended`);
//! - **quantities and vectors** — [`quantity::ResourceQuantity`],
//!   [`quantity::ResourceVector`], [`quantity::Money`] (micro-units, never a float),
//!   [`quantity::MeteringFormula`];
//! - **`usage_mapping`** — [`usage`]: the exclusive-role decomposition both provider
//!   dialects lower to (`input.total = uncached + cache_read + cache_write`);
//! - **`Attribution`** — [`attribution`]: `charged_to ∈ {subject, instrument}` (R-ACC-3),
//!   the producer table, `cache{hit}`, `over_reservation`;
//! - **`PricingTable` and spend rows** — [`pricing`]: `NoPrice` on a missing row (never
//!   zero), `MixedCurrency` on a mixed sum, `CostProvenance`/`provenance_class`/
//!   `derivation`/`confidence`/`coverage` stamps (ADR-0043);
//! - **the budget tree** — [`spec`] (`BudgetSpec`/`Ceiling`/`Threshold`/`Reservation`),
//!   [`tree`] (the pure projection: dynamic containment, `slice`/`pool`, reserve-before-
//!   spend, root-first `check`), [`events`] (the `control.budget.*` +
//!   `measurement.cost.attributed` payloads), [`account`] (the ledger-bound ops —
//!   `allocate`/`charge`/`reserve`/`release`/`exhaust`/`amend`/`advise`/`attribute_spend`/
//!   `totals`/`resolve_ask`/gauge caps, exhaustion rules **E1–E5**);
//! - **matched budgets** — [`matchspec`]: `MatchSpec`, `search_budget`/`eval_budget` arm
//!   schema, `validate_match` with the closed refusal sum {`UnbudgetedArm`,
//!   `IncommensurableMatch`, `MissingPricingTable`, `MissingMatchSpec`}, M1 `matched_cap`
//!   / M2 `iso_cost` / M3 `matched_total`;
//! - **governance** — [`governance`]: `HirDiff.budget_delta = loosening` refused in
//!   evolution contexts (ADR-0002/0040 D6).
//!
//! # The exhaustion rule E1–E5 (ADR-0040 D5, enumerated — ADR-0236 D-4)
//!
//! - **E1** — a hard ceiling is enforced *only at decision points* (before a call ⇒
//!   reservation refusal; after an effect's terminal event): a ceiling never interrupts
//!   a committed effect; every open effect reaches a terminal or `unknown` first.
//! - **E2** — on exhaustion the envelope appends `control.budget.exceeded` then
//!   `control.decision{kind: stop, reason: budget_exhausted{dimension}}` *atomically*;
//!   the run ends `budget_exhausted` with every open effect closed.
//! - **E3** — a per-dimension `grace` allowance permits one terminal summarising call —
//!   a soft action, never a ceiling widening.
//! - **E4** — a *soft* threshold is a `HarnessRule` (insert `ContextItem`), recorded once
//!   per (budget, threshold, context window), re-armed only on
//!   `context.compaction.completed{status: applied | fallback_applied}`; it never
//!   affects `check`.
//! - **E5** — gauge caps (`context.occupancy`, `fan_out`, `delegation_depth`) refuse the
//!   next increment (`SpawnRefused`, `CompactionRequired`) rather than stopping the run.
//!
//! # Invariants (§8.2 §2)
//!
//! Charges are idempotent on `(source_event, dimension)`; raw usage stays on the
//! producing event (R-ACC-1); every accountable event yields ≥ 1 charge (R-ACC-2 — see
//! [`account::AccountabilityReport`]); charges propagate to every ancestor;
//! `child.hard ≤ parent.remaining` at allocation; `Σ reservations + consumed ≤ hard`
//! along the ancestor chain; over-consumption is charged and flagged
//! `over_reservation = true`, never refused after the fact; spend is derived, never raw;
//! `cost_totals` is a materialized view rebuilt from charges and spend rows; one root
//! `BudgetNode` per run under one accounting authority (the writer lease).

pub mod account;
pub mod attribution;
pub mod errors;
pub mod events;
pub mod governance;
pub mod matchspec;
pub mod pricing;
pub mod quantity;
pub mod spec;
pub mod tree;
pub mod usage;

pub use hh_ontology::dimensions::{
    DerivedDimension, DimensionClass, DimensionId, DimensionKey, RegisteredDimension,
};

pub use account::{
    Account, AccountabilityReport, AmendAuthority, AskOutcome, ChargeRequest, TotalsGroupBy,
    TotalsScope,
};
pub use attribution::{Attribution, CacheAttribution, ChargedTo, ModelRef};
pub use errors::{
    BudgetError, EnforcementLevel, MatchError, MatchRefusal, RefusalReason, SpendError, UsageError,
};
pub use matchspec::{
    validate_match, ArmSpec, BudgetEnforcement, CachePolicy, Enforcement, MatchMode, MatchSpec,
    ModelScope, ResultsRowFields, ResultsSpend,
};
pub use pricing::{
    attribute_spend, price, Confidence, CostProvenance, Derivation, PricingRow, PricingTable,
    PricingTableRef, ProvenanceClass, SpendRow, SpendSource,
};
pub use quantity::{MeteringFormula, Money, ResourceQuantity, ResourceVector};
pub use spec::{
    BudgetMode, BudgetNode, BudgetScope, BudgetScopeKind, BudgetSpec, Ceiling, DimensionRule,
    Grace, Reservation, Threshold, ThresholdAt,
};
pub use tree::{BudgetTree, Exceeded};
pub use usage::{
    decompose, RawUsage, ReasoningSource, TokenDecomposition, TokenVector, UsageDialect,
    UsageMapping,
};
