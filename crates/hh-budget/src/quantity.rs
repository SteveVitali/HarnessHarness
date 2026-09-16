//! `ResourceQuantity`, `ResourceVector`, `MeteringFormula`, `Money` (§8.2 §3;
//! ADR-0039 D1; ADR-0043 D4 for `Money`).
//!
//! Canonical-JSON note: `Json` carries integers only (the WS-B1 canonical-number rule —
//! no IEEE doubles in hashed payloads). The spec row's `amount: int | decimal-string`
//! is therefore realized as a signed `i64` in the dimension's natural unit; a
//! decimal-string in a *source* document is parsed by the producing adapter into the
//! integer unit before it reaches this crate (ADR-0236 D-5). Ratios (`coverage`,
//! `tolerance`, `context.occupancy`, `budget_utilization`) are parts-per-million
//! integers — exact, never lossy.

use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::{DerivedDimension, DimensionId, DimensionKey};
use hh_wire::json::Json;
use std::collections::BTreeMap;

/// Parts-per-million of 1.0 — the canonical exact-ratio form (`coverage`,
/// `tolerance`, `budget_utilization`, `context.occupancy` levels).
pub const PPM_SCALE: i64 = 1_000_000;

/// `Money{micro_units, currency}` — ISO-4217, integer micro-units, never a float;
/// never summed across currencies (`MixedCurrency`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money {
    /// Integer micro-units (10⁻⁶ of `currency`).
    pub micro_units: i64,
    /// ISO-4217 currency code (e.g. `"USD"`).
    pub currency: String,
}

impl Money {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("currency", Json::str(&self.currency)),
            ("micro_units", Json::Int(self.micro_units)),
        ])
    }

    pub fn from_json(j: &Json) -> Option<Money> {
        Some(Money {
            micro_units: j.get("micro_units")?.as_int()?,
            currency: j.get("currency")?.as_str()?.to_string(),
        })
    }
}

/// `MeteringFormula{expr over role counters | provider_unit{name}}` (§8.2 §3).
///
/// How a dimension's quantity is metered. A `provider_unit` row is `provenance =
/// reported` by construction — the number is the provider's claim, not a kernel
/// measurement. `Expr` is the closed canonical text form over primary role counters
/// (see [`DerivedDimension::formula`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeteringFormula {
    /// An expression over role counters — canonical text (e.g.
    /// `"tokens.input.uncached + tokens.input.cache_read"`).
    Expr(String),
    /// A provider-reported unit — `provider_unit{name}`; `provenance = reported`.
    ProviderUnit(String),
}

impl MeteringFormula {
    /// The declared metering formula for a derived dimension.
    pub fn for_derived(d: DerivedDimension) -> MeteringFormula {
        MeteringFormula::Expr(d.formula().to_string())
    }

    /// The canonical wire form (`expr` is verbatim; provider units are `provider_unit{}`).
    pub fn to_json(&self) -> Json {
        match self {
            MeteringFormula::Expr(e) => Json::obj([("expr", Json::str(e))]),
            MeteringFormula::ProviderUnit(n) => Json::obj([("provider_unit", Json::str(n))]),
        }
    }

    pub fn from_json(j: &Json) -> Option<MeteringFormula> {
        if let Some(e) = j.get("expr").and_then(Json::as_str) {
            return Some(MeteringFormula::Expr(e.to_string()));
        }
        if let Some(n) = j.get("provider_unit").and_then(Json::as_str) {
            return Some(MeteringFormula::ProviderUnit(n.to_string()));
        }
        None
    }
}

/// `ResourceQuantity{dimension, amount, unit, model_ref?, measured_at: EventRef}`
/// (§8.2 §3) — one quantity of one primary dimension.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceQuantity {
    /// The primary dimension (never a derived name — those are computed).
    pub dimension: DimensionId,
    /// The amount in `dimension.unit()` (integers only).
    pub amount: i64,
    /// The unit — must equal `dimension.unit()` (checked at parse by callers that care;
    /// carried verbatim for schema fidelity).
    pub unit: String,
    /// The model the quantity was metered against (token/spend rows).
    pub model_ref: Option<String>,
    /// The event the quantity was measured at (the producing event).
    pub measured_at: EventRef,
}

/// `ResourceVector = map<DimensionId, amount>` (§8.2 §3) — over *primary* dimensions.
///
/// `BTreeMap` order is canonical (sorted by enum order of the name spelling —
/// `DimensionId`'s `Ord` is enum order, and the JSON form keys by spelling).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceVector {
    /// Amounts keyed by primary dimension.
    pub amounts: BTreeMap<DimensionId, i64>,
}

impl ResourceVector {
    /// The zero vector.
    pub fn zero() -> ResourceVector {
        ResourceVector::default()
    }

    /// A single-dimension vector.
    pub fn one(dimension: DimensionId, amount: i64) -> ResourceVector {
        let mut v = ResourceVector::zero();
        v.add(dimension, amount);
        v
    }

    /// Accumulate `amount` into `dimension` (monotone add — counters only go up).
    pub fn add(&mut self, dimension: DimensionId, amount: i64) {
        *self.amounts.entry(dimension).or_insert(0) += amount;
    }

    /// The amount for `dimension` (0 if absent).
    pub fn get(&self, dimension: DimensionId) -> i64 {
        *self.amounts.get(&dimension).unwrap_or(&0)
    }

    /// Sum the vector (or another vector's projection) into `self`.
    pub fn add_vec(&mut self, other: &ResourceVector) {
        for (d, a) in &other.amounts {
            self.add(*d, *a);
        }
    }

    /// Evaluate a derived name over this vector (`input.total`, `output.total`,
    /// `tokens.blended`) — the declared `MeteringFormula` applied to stored primaries.
    pub fn derived(&self, d: DerivedDimension) -> i64 {
        d.components().iter().map(|c| self.get(*c)).sum()
    }

    /// The total over a bound key (primary → `get`; derived → `derived`).
    pub fn key_total(&self, key: DimensionKey) -> i64 {
        match key {
            DimensionKey::Primary(d) => self.get(d),
            DimensionKey::Derived(d) => self.derived(d),
        }
    }

    /// `self + other`.
    pub fn plus(&self, other: &ResourceVector) -> ResourceVector {
        let mut v = self.clone();
        v.add_vec(other);
        v
    }

    /// `self − other` — for view arithmetic only (remaining, utilization); charges are
    /// never subtracted in the ledger (reverts exclude by projection).
    pub fn minus(&self, other: &ResourceVector) -> ResourceVector {
        let mut v = self.clone();
        for (d, a) in &other.amounts {
            *v.amounts.entry(*d).or_insert(0) -= a;
        }
        v
    }

    /// Every key, sorted by spelling — the canonical iteration order.
    pub fn iter(&self) -> impl Iterator<Item = (DimensionId, i64)> + '_ {
        self.amounts.iter().map(|(d, a)| (*d, *a))
    }

    pub fn is_empty(&self) -> bool {
        self.amounts.values().all(|a| *a == 0)
    }

    /// `{dim_spelling: amount}` — canonical object (sorted keys by `Json`'s BTreeMap).
    pub fn to_json(&self) -> Json {
        Json::Obj(
            self.amounts
                .iter()
                .map(|(d, a)| (d.as_str().to_string(), Json::Int(*a)))
                .collect(),
        )
    }

    /// Parse `{dim_spelling: amount}`; unknown spellings are refused (`None`).
    pub fn from_json(j: &Json) -> Option<ResourceVector> {
        let m = match j {
            Json::Obj(m) => m,
            _ => return None,
        };
        let mut v = ResourceVector::zero();
        for (k, a) in m {
            let d = DimensionId::parse(k)?;
            v.add(d, a.as_int()?);
        }
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_arithmetic_and_derived_views() {
        let mut v = ResourceVector::zero();
        v.add(DimensionId::TokensInputUncached, 60);
        v.add(DimensionId::TokensInputCacheRead, 30);
        v.add(DimensionId::TokensInputCacheWrite, 10);
        v.add(DimensionId::TokensOutputVisible, 25);
        v.add(DimensionId::TokensOutputReasoning, 15);
        // AC-2 identity.
        assert_eq!(v.derived(DerivedDimension::TokensInputTotal), 100);
        assert_eq!(v.derived(DerivedDimension::TokensOutputTotal), 40);
        assert_eq!(v.derived(DerivedDimension::TokensBlended), 140);
        assert_eq!(
            v.key_total(DimensionKey::Derived(DerivedDimension::TokensBlended)),
            140
        );
        let w = ResourceVector::one(DimensionId::TokensInputUncached, 10);
        assert_eq!(v.plus(&w).get(DimensionId::TokensInputUncached), 70);
        assert_eq!(v.minus(&w).get(DimensionId::TokensInputUncached), 50);
    }

    #[test]
    fn vector_json_round_trip_and_refusal() {
        let mut v = ResourceVector::zero();
        v.add(DimensionId::ModelCalls, 3);
        v.add(DimensionId::Spend, 12500);
        let j = v.to_json();
        assert_eq!(
            j.to_canonical_string(),
            r#"{"model_calls":3,"spend":12500}"#
        );
        assert_eq!(ResourceVector::from_json(&j), Some(v));
        // Unknown spelling refused — never coerced.
        assert_eq!(
            ResourceVector::from_json(&Json::obj([("gpu_hours", Json::Int(1))])),
            None
        );
        // A derived name in a *stored* vector is refused (derived names are views).
        assert_eq!(
            ResourceVector::from_json(&Json::obj([("tokens.blended", Json::Int(1))])),
            None
        );
    }
}
