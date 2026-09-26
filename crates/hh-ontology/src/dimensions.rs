//! The kernel resource-dimension registry (§8.2 R-2.1.6 §3, the `DimensionId` row;
//! ADR-0039 D1 as amended P2/P4). This is the **closed list of kernel resource
//! dimensions** — counters and gauges — plus the registered `ext.*`/`hh.*` entries.
//!
//! The registry lives in the ontology crate so `hh-hir` can check `Budget.dimensions`
//! keys against it (the DF-S1.4-1 seam) without a layering inversion; `hh-budget`
//! re-exports it as the accounting layer's single source (CC7).
//!
//! # Rules landed here
//!
//! - The kernel list is **closed**; [`DimensionId::ALL`] is the totality check.
//! - Registered names are a closed set too — Stage 1 registers
//!   `hh.egress.decisions.{allowed,denied,asked}`, `hh.containment.violations` and
//!   `ext.effects.external_irreversible` (§8.2 §3 "Registered `ext`"; the
//!   `ext.effects.external_irreversible` promotion to the kernel list is deferred —
//!   ADR-0211/OQ-463). Adding a name is a dialect bump.
//! - **Primary** dimensions are chargeable (they appear in `control.budget.consumed`).
//!   **Derived** names — `tokens.input.total`, `tokens.output.total`,
//!   `tokens.blended` — are never stored primary: they evaluate by a declared
//!   [`DerivedDimension::formula`]. [`DimensionKey`] is the bound-key type (a `Ceiling`
//!   may be written against a derived name; the bound evaluates over its components —
//!   ADR-0236 D-2).
//! - `tokens.input.cache_write` carries a per-charge `cache_ttl` qualifier — the
//!   `[ttl_class]` subscript of the spec row is provider-conditioned and lives in the
//!   pricing table, not in the kernel list (ADR-0039 (d)).
//! - Quantities are integers only; money is scaled integer micro-units of a declared
//!   currency; time is ms (ADR-0039 D1; WS-B1 canonical-number rule).
//! - Counters monotonically accumulate; gauges are instantaneous levels, max-aggregated,
//!   cap-bounded — they refuse the next increment (E5), never a charge target.

use std::collections::BTreeSet;
use std::fmt;

/// Whether a dimension accumulates (counter) or holds a level that must not exceed a cap
/// (gauge). §8.2 `DimensionId` row — `counter|gauge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DimensionClass {
    /// Monotone, summable, ceiling-bounded — charged on `control.budget.consumed`.
    Counter,
    /// Instantaneous level, max-aggregated, cap-bounded — refuses the next increment
    /// (E5: `SpawnRefused`, `CompactionRequired`); never a charge target.
    Gauge,
}

impl DimensionClass {
    pub const ALL: [DimensionClass; 2] = [DimensionClass::Counter, DimensionClass::Gauge];

    pub fn as_str(self) -> &'static str {
        match self {
            DimensionClass::Counter => "counter",
            DimensionClass::Gauge => "gauge",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionClass> {
        match s {
            "counter" => Some(DimensionClass::Counter),
            "gauge" => Some(DimensionClass::Gauge),
            _ => None,
        }
    }
}

/// A registered non-kernel dimension (§8.2 §3 "Registered `ext`"). The registered set is
/// closed — unknown spellings are refused, never coerced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegisteredDimension {
    /// `hh.egress.decisions.allowed` — egress decisions allowed (ADR-0039 P2 log).
    EgressDecisionsAllowed,
    /// `hh.egress.decisions.denied` — egress decisions denied.
    EgressDecisionsDenied,
    /// `hh.egress.decisions.asked` — egress decisions raised to `ask`.
    EgressDecisionsAsked,
    /// `hh.containment.violations` — containment violations observed.
    ContainmentViolations,
    /// `ext.effects.external_irreversible` — `irreversible ∧ scope = external` effects
    /// reaching `committed` (ADR-0207 d5; kernel-list promotion deferred — ADR-0211).
    EffectsExternalIrreversible,
}

impl RegisteredDimension {
    /// The registered set, in canonical (sorted-by-spelling) order.
    pub const ALL: [RegisteredDimension; 5] = [
        RegisteredDimension::EffectsExternalIrreversible,
        RegisteredDimension::EgressDecisionsAllowed,
        RegisteredDimension::EgressDecisionsAsked,
        RegisteredDimension::EgressDecisionsDenied,
        RegisteredDimension::ContainmentViolations,
    ];

    /// The full registered spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RegisteredDimension::EgressDecisionsAllowed => "hh.egress.decisions.allowed",
            RegisteredDimension::EgressDecisionsDenied => "hh.egress.decisions.denied",
            RegisteredDimension::EgressDecisionsAsked => "hh.egress.decisions.asked",
            RegisteredDimension::ContainmentViolations => "hh.containment.violations",
            RegisteredDimension::EffectsExternalIrreversible => "ext.effects.external_irreversible",
        }
    }

    /// Parse a registered spelling. Unknown names are refused — the registration list is
    /// the authority.
    pub fn parse(s: &str) -> Option<RegisteredDimension> {
        RegisteredDimension::ALL
            .iter()
            .copied()
            .find(|e| e.as_str() == s)
    }
}

/// The kernel dimension list (§8.2 `DimensionId` row — closed). Spellings are the
/// canonical names; [`DimensionId::parse`] is the only constructor from text so unknown
/// spellings are refused at every boundary (CC1, R-PARSE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DimensionId {
    // ---- token counters (exclusive roles; `tokens.input.total`/`tokens.output.total`/
    //      `tokens.blended` are derived names, never stored primary) ----
    /// `tokens.input.uncached` — input tokens that hit no cache.
    TokensInputUncached,
    /// `tokens.input.cache_read` — input tokens served from a provider cache.
    TokensInputCacheRead,
    /// `tokens.input.cache_write` — input tokens written into a provider cache; the
    /// `[ttl_class]` qualifier rides on the charge (`cache_ttl`), not the kernel name.
    TokensInputCacheWrite,
    /// `tokens.output.visible` — output tokens the beneficiary can read.
    TokensOutputVisible,
    /// `tokens.output.reasoning` — hidden reasoning/thinking tokens.
    TokensOutputReasoning,

    // ---- call / step counters ----
    /// `model_calls` — a model invocation.
    ModelCalls,
    /// `tool_calls` — a tool invocation through the executor.
    ToolCalls,
    /// `turns` — a run turn.
    Turns,
    /// `retries` — a retry (shared attempt identity; INV-6's single counter).
    Retries,
    /// `spawns` — an AgentProcess spawn.
    Spawns,
    /// `approvals.requested` — a human-targeted durable `pending` not satisfied by a
    /// lease or coalescing (ADR-0071 D3/I-P5) — the budgeted approval dimension.
    ApprovalsRequested,
    /// `approvals.granted` — a human `allow` decision (the autonomy counter).
    ApprovalsGranted,
    /// `evaluator_calls` — deterministic/evaluator (incl. automatic-reviewer)
    /// invocations.
    EvaluatorCalls,

    // ---- network counters (producer Stage 2) ----
    /// `network.calls` — a network request through the egress mediator.
    NetworkCalls,
    /// `network.bytes_out` — egress bytes.
    NetworkBytesOut,
    /// `network.bytes_in` — ingress bytes.
    NetworkBytesIn,

    // ---- time counters (milliseconds) ----
    /// `time.wall_ms` — wall-clock time consumed.
    TimeWallMs,
    /// `time.working_ms` — `wall − waiting` (the matched-comparison time basis, AC-11).
    TimeWorkingMs,
    /// `time.model_latency_ms` — model-boundary latency.
    TimeModelLatencyMs,
    /// `time.human_wait_ms` — `pending → decided` for human targets (excluded from
    /// `time.working_ms`).
    TimeHumanWaitMs,

    // ---- environment counters (producer Stage 2) ----
    /// `env.active_ms` — environment held active.
    EnvActiveMs,
    /// `env.reserved_ms` — environment reserved.
    EnvReservedMs,
    /// `env.suspended_ms` — environment suspended (ADR-0039 P2 log).
    EnvSuspendedMs,

    // ---- spend (always derived from a PricingTable, never raw) ----
    /// `spend` — money, always derived from a `PricingTable`; charged in micro-units of
    /// the declared currency.
    Spend,

    // ---- registered non-kernel names (§8.2 §3 "Registered `ext`") ----
    /// `hh.egress.decisions.allowed`.
    ExtEgressAllowed,
    /// `hh.egress.decisions.denied`.
    ExtEgressDenied,
    /// `hh.egress.decisions.asked`.
    ExtEgressAsked,
    /// `hh.containment.violations`.
    ExtContainmentViolations,
    /// `ext.effects.external_irreversible`.
    ExtEffectsExternalIrreversible,

    // ---- gauges (instantaneous, max-aggregated, cap-bounded; E5) ----
    /// `context.occupancy` — fraction of the context window in use (ppm).
    ContextOccupancy,
    /// `fan_out` — live fan-out width.
    FanOut,
    /// `delegation_depth` — live delegation depth.
    DelegationDepth,
    /// `reconciliation.holds` — the completion gate's consumed-holds level
    /// against the F4 cap (ADR-0113 D4: a gauge cap on the run's `BudgetNode`;
    /// each `hold` consumes one unit; exhaustion ⇒ `escalate` or
    /// `budget_exhausted{reconciliation.holds}`). Stage-1 additive
    /// registration (S1.21; closed-set growth is per-dialect).
    ReconciliationHolds,
}

impl DimensionId {
    /// The closed list — 24 kernel counters + 5 registered names + 4 gauges, in enum
    /// (canonical) order. `reconciliation.holds` joined at S1.21 (ADR-0113 D4).
    pub const ALL: [DimensionId; 33] = [
        DimensionId::TokensInputUncached,
        DimensionId::TokensInputCacheRead,
        DimensionId::TokensInputCacheWrite,
        DimensionId::TokensOutputVisible,
        DimensionId::TokensOutputReasoning,
        DimensionId::ModelCalls,
        DimensionId::ToolCalls,
        DimensionId::Turns,
        DimensionId::Retries,
        DimensionId::Spawns,
        DimensionId::ApprovalsRequested,
        DimensionId::ApprovalsGranted,
        DimensionId::EvaluatorCalls,
        DimensionId::NetworkCalls,
        DimensionId::NetworkBytesOut,
        DimensionId::NetworkBytesIn,
        DimensionId::TimeWallMs,
        DimensionId::TimeWorkingMs,
        DimensionId::TimeModelLatencyMs,
        DimensionId::TimeHumanWaitMs,
        DimensionId::EnvActiveMs,
        DimensionId::EnvReservedMs,
        DimensionId::EnvSuspendedMs,
        DimensionId::Spend,
        DimensionId::ExtEgressAllowed,
        DimensionId::ExtEgressDenied,
        DimensionId::ExtEgressAsked,
        DimensionId::ExtContainmentViolations,
        DimensionId::ExtEffectsExternalIrreversible,
        DimensionId::ContextOccupancy,
        DimensionId::FanOut,
        DimensionId::DelegationDepth,
        DimensionId::ReconciliationHolds,
    ];

    /// The canonical spelling (§8.2 `DimensionId` row names).
    pub fn as_str(self) -> &'static str {
        match self {
            DimensionId::TokensInputUncached => "tokens.input.uncached",
            DimensionId::TokensInputCacheRead => "tokens.input.cache_read",
            DimensionId::TokensInputCacheWrite => "tokens.input.cache_write",
            DimensionId::TokensOutputVisible => "tokens.output.visible",
            DimensionId::TokensOutputReasoning => "tokens.output.reasoning",
            DimensionId::ModelCalls => "model_calls",
            DimensionId::ToolCalls => "tool_calls",
            DimensionId::Turns => "turns",
            DimensionId::Retries => "retries",
            DimensionId::Spawns => "spawns",
            DimensionId::ApprovalsRequested => "approvals.requested",
            DimensionId::ApprovalsGranted => "approvals.granted",
            DimensionId::EvaluatorCalls => "evaluator_calls",
            DimensionId::NetworkCalls => "network.calls",
            DimensionId::NetworkBytesOut => "network.bytes_out",
            DimensionId::NetworkBytesIn => "network.bytes_in",
            DimensionId::TimeWallMs => "time.wall_ms",
            DimensionId::TimeWorkingMs => "time.working_ms",
            DimensionId::TimeModelLatencyMs => "time.model_latency_ms",
            DimensionId::TimeHumanWaitMs => "time.human_wait_ms",
            DimensionId::EnvActiveMs => "env.active_ms",
            DimensionId::EnvReservedMs => "env.reserved_ms",
            DimensionId::EnvSuspendedMs => "env.suspended_ms",
            DimensionId::Spend => "spend",
            DimensionId::ExtEgressAllowed => "hh.egress.decisions.allowed",
            DimensionId::ExtEgressDenied => "hh.egress.decisions.denied",
            DimensionId::ExtEgressAsked => "hh.egress.decisions.asked",
            DimensionId::ExtContainmentViolations => "hh.containment.violations",
            DimensionId::ExtEffectsExternalIrreversible => "ext.effects.external_irreversible",
            DimensionId::ContextOccupancy => "context.occupancy",
            DimensionId::FanOut => "fan_out",
            DimensionId::DelegationDepth => "delegation_depth",
            DimensionId::ReconciliationHolds => "reconciliation.holds",
        }
    }

    /// Every registered name (kernel + registered set) — for registration-time checks.
    pub fn registered_names() -> BTreeSet<&'static str> {
        DimensionId::ALL.iter().map(|d| d.as_str()).collect()
    }

    /// `counter` or `gauge` (§8.2 `DimensionId` row).
    pub fn class(self) -> DimensionClass {
        match self {
            DimensionId::ContextOccupancy
            | DimensionId::FanOut
            | DimensionId::DelegationDepth
            | DimensionId::ReconciliationHolds => DimensionClass::Gauge,
            _ => DimensionClass::Counter,
        }
    }

    /// The natural unit the dimension is counted in — the `unit` a `Ceiling` against it
    /// carries (§8.2 `Ceiling{limit, unit}`).
    pub fn unit(self) -> &'static str {
        match self {
            DimensionId::TokensInputUncached
            | DimensionId::TokensInputCacheRead
            | DimensionId::TokensInputCacheWrite
            | DimensionId::TokensOutputVisible
            | DimensionId::TokensOutputReasoning => "tokens",
            DimensionId::ModelCalls
            | DimensionId::ToolCalls
            | DimensionId::EvaluatorCalls
            | DimensionId::NetworkCalls => "calls",
            DimensionId::TimeWallMs
            | DimensionId::TimeWorkingMs
            | DimensionId::TimeModelLatencyMs
            | DimensionId::TimeHumanWaitMs
            | DimensionId::EnvActiveMs
            | DimensionId::EnvReservedMs
            | DimensionId::EnvSuspendedMs => "ms",
            DimensionId::NetworkBytesOut | DimensionId::NetworkBytesIn => "bytes",
            DimensionId::Spend => "micro_units",
            DimensionId::ContextOccupancy => "fraction_ppm",
            DimensionId::Turns
            | DimensionId::Retries
            | DimensionId::Spawns
            | DimensionId::ApprovalsRequested
            | DimensionId::ApprovalsGranted
            | DimensionId::ExtEgressAllowed
            | DimensionId::ExtEgressDenied
            | DimensionId::ExtEgressAsked
            | DimensionId::ExtContainmentViolations
            | DimensionId::ExtEffectsExternalIrreversible
            | DimensionId::FanOut
            | DimensionId::DelegationDepth
            | DimensionId::ReconciliationHolds => "count",
        }
    }

    /// Parse the canonical spelling; unknown names — registered or not — are refused
    /// (`None`), never coerced.
    pub fn parse(s: &str) -> Option<DimensionId> {
        if let Some(r) = RegisteredDimension::parse(s) {
            return Some(match r {
                RegisteredDimension::EgressDecisionsAllowed => DimensionId::ExtEgressAllowed,
                RegisteredDimension::EgressDecisionsDenied => DimensionId::ExtEgressDenied,
                RegisteredDimension::EgressDecisionsAsked => DimensionId::ExtEgressAsked,
                RegisteredDimension::ContainmentViolations => DimensionId::ExtContainmentViolations,
                RegisteredDimension::EffectsExternalIrreversible => {
                    DimensionId::ExtEffectsExternalIrreversible
                }
            });
        }
        DimensionId::ALL.iter().copied().find(|d| d.as_str() == s)
    }

    /// Is this one of the five exclusive token roles? (Usage-mapping checks.)
    pub fn is_token_role(self) -> bool {
        matches!(
            self,
            DimensionId::TokensInputUncached
                | DimensionId::TokensInputCacheRead
                | DimensionId::TokensInputCacheWrite
                | DimensionId::TokensOutputVisible
                | DimensionId::TokensOutputReasoning
        )
    }
}

/// A **derived** dimension name — valid as a bound key (`Ceiling`) and in cost views, but
/// never a stored primary: it evaluates by a declared `MeteringFormula` over the primary
/// roles (§8.2 `DimensionId` row, "derived by a declared MeteringFormula, never stored
/// primary").
///
/// `tokens.blended` is the Stage-0 blended-token bound — the sole sanctioned view of
/// tokens for comparison and the name the baseline's budget ceilings migrate onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivedDimension {
    /// `tokens.input.total` = `input.uncached + input.cache_read + input.cache_write`
    /// (the hh-inclusive/1 identity — AC-2).
    TokensInputTotal,
    /// `tokens.output.total` = `output.visible + output.reasoning`.
    TokensOutputTotal,
    /// `tokens.blended` = `tokens.input.total + tokens.output.total`.
    TokensBlended,
}

impl DerivedDimension {
    pub const ALL: [DerivedDimension; 3] = [
        DerivedDimension::TokensInputTotal,
        DerivedDimension::TokensOutputTotal,
        DerivedDimension::TokensBlended,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DerivedDimension::TokensInputTotal => "tokens.input.total",
            DerivedDimension::TokensOutputTotal => "tokens.output.total",
            DerivedDimension::TokensBlended => "tokens.blended",
        }
    }

    pub fn parse(s: &str) -> Option<DerivedDimension> {
        DerivedDimension::ALL
            .iter()
            .copied()
            .find(|d| d.as_str() == s)
    }

    /// The primary roles summed — the closed derivation (the `MeteringFormula` for these
    /// names is fixed, never configurable).
    pub fn components(self) -> &'static [DimensionId] {
        match self {
            DerivedDimension::TokensInputTotal => &[
                DimensionId::TokensInputUncached,
                DimensionId::TokensInputCacheRead,
                DimensionId::TokensInputCacheWrite,
            ],
            DerivedDimension::TokensOutputTotal => &[
                DimensionId::TokensOutputVisible,
                DimensionId::TokensOutputReasoning,
            ],
            DerivedDimension::TokensBlended => &[
                DimensionId::TokensInputUncached,
                DimensionId::TokensInputCacheRead,
                DimensionId::TokensInputCacheWrite,
                DimensionId::TokensOutputVisible,
                DimensionId::TokensOutputReasoning,
            ],
        }
    }

    /// The declared formula text (the `MeteringFormula{expr over role counters}` for this
    /// derived name).
    pub fn formula(self) -> &'static str {
        match self {
            DerivedDimension::TokensInputTotal => {
                "tokens.input.uncached + tokens.input.cache_read + tokens.input.cache_write"
            }
            DerivedDimension::TokensOutputTotal => {
                "tokens.output.visible + tokens.output.reasoning"
            }
            DerivedDimension::TokensBlended => "tokens.input.total + tokens.output.total",
        }
    }
}

/// A budget-bound key or usage key: either a primary [`DimensionId`] or a derived name.
/// Bound keys admit derived names (a ceiling on `tokens.blended`); charge keys are
/// primary only — see [`DimensionKey::primary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DimensionKey {
    /// A chargeable kernel/registered dimension.
    Primary(DimensionId),
    /// A formula-derived name — valid as a bound key, evaluated by its fixed derivation.
    Derived(DerivedDimension),
}

impl DimensionKey {
    /// Parse a bound-key spelling; unknown names are refused (`None`).
    pub fn parse(s: &str) -> Option<DimensionKey> {
        if let Some(d) = DimensionId::parse(s) {
            return Some(DimensionKey::Primary(d));
        }
        DerivedDimension::parse(s).map(DimensionKey::Derived)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DimensionKey::Primary(d) => d.as_str(),
            DimensionKey::Derived(d) => d.as_str(),
        }
    }

    /// The chargeable primary dimension, if this key names one.
    pub fn primary(self) -> Option<DimensionId> {
        match self {
            DimensionKey::Primary(d) => Some(d),
            DimensionKey::Derived(_) => None,
        }
    }
}

impl fmt::Display for DimensionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<DimensionId> for DimensionKey {
    fn from(d: DimensionId) -> DimensionKey {
        DimensionKey::Primary(d)
    }
}

impl From<DerivedDimension> for DimensionKey {
    fn from(d: DerivedDimension) -> DimensionKey {
        DimensionKey::Derived(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §8.2 §3 `DimensionId` row, verbatim — the totality oracle.
    const SPEC_COUNTERS: &[&str] = &[
        "tokens.input.uncached",
        "tokens.input.cache_read",
        "tokens.input.cache_write",
        "tokens.output.visible",
        "tokens.output.reasoning",
        "model_calls",
        "tool_calls",
        "turns",
        "retries",
        "spawns",
        "approvals.requested",
        "approvals.granted",
        "evaluator_calls",
        "network.calls",
        "network.bytes_out",
        "network.bytes_in",
        "time.wall_ms",
        "time.working_ms",
        "time.model_latency_ms",
        "time.human_wait_ms",
        "env.active_ms",
        "env.reserved_ms",
        "env.suspended_ms",
        "spend",
    ];
    const SPEC_GAUGES: &[&str] = &[
        "context.occupancy",
        "fan_out",
        "delegation_depth",
        "reconciliation.holds", // S1.21 — ADR-0113 D4 (F4)
    ];
    const SPEC_REGISTERED: &[&str] = &[
        "hh.egress.decisions.allowed",
        "hh.egress.decisions.denied",
        "hh.egress.decisions.asked",
        "hh.containment.violations",
        "ext.effects.external_irreversible",
    ];

    #[test]
    fn kernel_list_matches_the_spec_row_exactly() {
        // §8.2 §3: 24 counters + 4 gauges + 5 registered = 33 names
        // (`reconciliation.holds` joined at S1.21 — ADR-0113 D4).
        assert_eq!(DimensionId::ALL.len(), 33);
        let names: BTreeSet<&'static str> = DimensionId::ALL.iter().map(|d| d.as_str()).collect();
        assert_eq!(names.len(), DimensionId::ALL.len(), "duplicate spellings");
        let expected: BTreeSet<&'static str> = SPEC_COUNTERS
            .iter()
            .chain(SPEC_GAUGES)
            .chain(SPEC_REGISTERED)
            .copied()
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn gauge_class_is_exactly_the_spec_set() {
        let gauges: BTreeSet<&'static str> = DimensionId::ALL
            .iter()
            .filter(|d| d.class() == DimensionClass::Gauge)
            .map(|d| d.as_str())
            .collect();
        assert_eq!(gauges, SPEC_GAUGES.iter().copied().collect());
    }

    #[test]
    fn parse_round_trips_and_refuses_unknown() {
        for d in DimensionId::ALL {
            assert_eq!(DimensionId::parse(d.as_str()), Some(d));
        }
        assert_eq!(DimensionId::parse("tokens.blended"), None); // derived, not primary
        assert_eq!(DimensionId::parse("ext.mystery"), None);
        assert_eq!(DimensionId::parse("hh.egress.decisions.maybe"), None);
        assert_eq!(DimensionId::parse("gpu_hours"), None);
        assert_eq!(DimensionId::parse(""), None);
    }

    #[test]
    fn dimension_key_admits_derived_bound_names() {
        assert_eq!(
            DimensionKey::parse("tokens.blended"),
            Some(DimensionKey::Derived(DerivedDimension::TokensBlended))
        );
        assert_eq!(
            DimensionKey::parse("model_calls"),
            Some(DimensionKey::Primary(DimensionId::ModelCalls))
        );
        assert_eq!(DimensionKey::parse("bogus"), None);
        assert!(DimensionKey::parse("tokens.blended")
            .unwrap()
            .primary()
            .is_none());
    }

    #[test]
    fn derived_components_are_exclusive_primary_roles() {
        // input.total = uncached + cache_read + cache_write (the AC-2 identity).
        let input: BTreeSet<DimensionId> = DerivedDimension::TokensInputTotal
            .components()
            .iter()
            .copied()
            .collect();
        assert_eq!(
            input,
            [
                DimensionId::TokensInputUncached,
                DimensionId::TokensInputCacheRead,
                DimensionId::TokensInputCacheWrite
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(DerivedDimension::TokensBlended.components().len(), 5);
    }
}
