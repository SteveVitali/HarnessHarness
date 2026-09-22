//! `MatchSpec`, `search_budget`/`eval_budget` and `validate_match` — the
//! matched-budget definitions M1/M2/M3 and the closed refusal sum (§8.2 §2–3;
//! §8.5; ADR-0041 D1–D7 as amended ADR-0128/0165, CF-278/350/334; ADR-0159 D2).
//!
//! **This module is where the definitions originate** (the ticket's CC9 note):
//! every downstream consumer (§06 engine precondition, results rows, replay R3)
//! reads these semantics, never re-decides them.
//!
//! # The three modes (ADR-0041 D1–D4)
//!
//! - **M1 `matched_cap`** — identical hard ceilings on the declared dimensions;
//!   utilisation `consumed / hard` and consumption distributions are always
//!   reported. Under `cross_model` admissible only on `spend`, `time.*`,
//!   `model_calls`, `tool_calls`, `approvals.*` — token-level matching across
//!   models is ill-defined (OQ-094; ADR-0159 D2: refused, never deferred).
//!   Requires `budget_enforcement[dimension] = enforced` on every arm.
//! - **M2 `iso_cost`** — equal *realized* consumption via `P(success | consumed
//!   ≤ c)` curves; spend rows with `confidence ∈ {estimate, unknown}` or
//!   `coverage < 1` render bands, never points. A single pinned `PricingTable`
//!   is mandatory — spend at a pinned table is the only cross-model common unit.
//! - **M3 `matched_total`** — `search + eval + inference` equal across arms;
//!   the frozen artifact evaluated under `eval_budget` alone; the baseline arm
//!   receives the same total as test-time compute. Refused when any arm has an
//!   `unenforceable` matched dimension; required for every C4 claim.
//!
//! # The refusal sum (closed — refusal, never a warning; ADR-0041 D6)
//!
//! `UnbudgetedArm` · `IncommensurableMatch{arm, dimension, enforceability}` ·
//! `MissingPricingTable` · `MissingMatchSpec`. Exploratory runs may execute with
//! `MatchSpec{mode: none}` but never report a comparison — `validate_match`
//! refuses `none` arms for reporting.

use hh_ontology::dimensions::DimensionId;
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::{EnforcementLevel, MatchError, MatchRefusal, RefusalReason};
use crate::pricing::PricingTableRef;
use crate::quantity::PPM_SCALE;
use crate::spec::BudgetSpec;

/// `MatchSpec.mode` (§8.2 §3) + `none` for exploratory runs (never a comparison).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchMode {
    /// M1 — identical hard ceilings on the declared dimensions.
    MatchedCap,
    /// M2 — equal realized consumption (`P(success | consumed ≤ c)`).
    IsoCost,
    /// M3 — `search + eval + inference` totals equal across arms.
    MatchedTotal,
    /// Exploratory — executes, never compares.
    None,
}

impl MatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchMode::MatchedCap => "matched_cap",
            MatchMode::IsoCost => "iso_cost",
            MatchMode::MatchedTotal => "matched_total",
            MatchMode::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<MatchMode> {
        match s {
            "matched_cap" => Some(MatchMode::MatchedCap),
            "iso_cost" => Some(MatchMode::IsoCost),
            "matched_total" => Some(MatchMode::MatchedTotal),
            "none" => Some(MatchMode::None),
            _ => None,
        }
    }
}

/// `model_scope ∈ {same_snapshot, cross_model}` (§8.2 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelScope {
    /// Every arm serves the same pinned model snapshot.
    SameSnapshot,
    /// Arms serve different models — the admissible-dimension rule applies.
    CrossModel,
}

impl ModelScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelScope::SameSnapshot => "same_snapshot",
            ModelScope::CrossModel => "cross_model",
        }
    }

    pub fn parse(s: &str) -> Option<ModelScope> {
        match s {
            "same_snapshot" => Some(ModelScope::SameSnapshot),
            "cross_model" => Some(ModelScope::CrossModel),
            _ => None,
        }
    }
}

/// `cache_policy ∈ {cold_start, natural, primed{prime_ref}}` (§8.2 §3) — uniform
/// across arms; `cold_start` is the default for M1/M2.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CachePolicy {
    /// Caches cold at arm start (the M1/M2 default).
    ColdStart,
    /// Whatever the run produces — recorded, uncontrolled.
    Natural,
    /// Primed from `prime_ref` (a seeding run/artifact).
    Primed(String),
}

impl CachePolicy {
    pub fn to_json(&self) -> Json {
        match self {
            CachePolicy::ColdStart => Json::str("cold_start"),
            CachePolicy::Natural => Json::str("natural"),
            CachePolicy::Primed(r) => {
                Json::obj([("primed", Json::obj([("prime_ref", Json::str(r))]))])
            }
        }
    }

    pub fn from_json(j: &Json) -> Option<CachePolicy> {
        match j.as_str() {
            Some("cold_start") => Some(CachePolicy::ColdStart),
            Some("natural") => Some(CachePolicy::Natural),
            _ => j
                .get("primed")
                .and_then(|p| p.get("prime_ref"))
                .and_then(Json::as_str)
                .map(|r| CachePolicy::Primed(r.to_string())),
        }
    }
}

/// `MatchSpec{dimensions, mode, tolerance, pricing_table_ref?, model_scope,
/// cache_policy}` (§8.2 §3 verbatim; `tolerance` is ppm of 1.0 — canonical JSON
/// carries integers only).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchSpec {
    /// The matched dimensions.
    pub dimensions: Vec<DimensionId>,
    /// The match mode.
    pub mode: MatchMode,
    /// `tolerance` — ppm of 1.0 (exact-equality is `0`).
    pub tolerance_ppm: i64,
    /// The one pinned pricing table (mandatory for `iso_cost` and any spend match).
    pub pricing_table_ref: Option<PricingTableRef>,
    /// `same_snapshot` | `cross_model`.
    pub model_scope: ModelScope,
    /// `cold_start` | `natural` | `primed{prime_ref}` — uniform across arms.
    pub cache_policy: CachePolicy,
}

impl MatchSpec {
    /// A `matched_cap` spec over `dims` (same-snapshot, cold-start, exact).
    pub fn matched_cap(dims: &[DimensionId]) -> MatchSpec {
        MatchSpec {
            dimensions: dims.to_vec(),
            mode: MatchMode::MatchedCap,
            tolerance_ppm: 0,
            pricing_table_ref: None,
            model_scope: ModelScope::SameSnapshot,
            cache_policy: CachePolicy::ColdStart,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "dimensions".to_string(),
            Json::Arr(
                self.dimensions
                    .iter()
                    .map(|d| Json::str(d.as_str()))
                    .collect(),
            ),
        );
        m.insert("mode".to_string(), Json::str(self.mode.as_str()));
        m.insert("tolerance".to_string(), Json::Int(self.tolerance_ppm));
        if let Some(p) = &self.pricing_table_ref {
            m.insert("pricing_table_ref".to_string(), p.to_json());
        }
        m.insert(
            "model_scope".to_string(),
            Json::str(self.model_scope.as_str()),
        );
        m.insert("cache_policy".to_string(), self.cache_policy.to_json());
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<MatchSpec> {
        let dimensions = match j.get("dimensions") {
            Some(Json::Arr(ds)) => ds
                .iter()
                .map(|d| DimensionId::parse(d.as_str()?))
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        Some(MatchSpec {
            dimensions,
            mode: MatchMode::parse(j.get("mode")?.as_str()?)?,
            tolerance_ppm: j.get("tolerance")?.as_int()?,
            pricing_table_ref: j
                .get("pricing_table_ref")
                .and_then(PricingTableRef::from_json),
            model_scope: ModelScope::parse(j.get("model_scope")?.as_str()?)?,
            cache_policy: CachePolicy::from_json(j.get("cache_policy")?)?,
        })
    }
}

/// `budget_enforcement` (ADR-0165 D3) — `map<dimension, {enforced, advisory,
/// unenforceable}>`, derived by the hosting adapter from mechanism × placement ×
/// interception. **Native arms are trivially `enforced`** — a `native` arm's map
/// may be empty; lookups default to `Enforced`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BudgetEnforcement {
    /// Hosted arms declare per-dimension enforcement; a native arm's empty map
    /// means `enforced` for everything.
    pub levels: BTreeMap<DimensionId, Enforcement>,
    /// Whether the arm is native (trivially enforced).
    pub native: bool,
}

/// `enforced | advisory | unenforceable` (§8.2 `budget_enforcement` row) — the
/// [`crate::errors::EnforcementLevel`] spelling as used on arms.
pub type Enforcement = EnforcementLevel;

impl BudgetEnforcement {
    /// A native arm — trivially enforced.
    pub fn native() -> BudgetEnforcement {
        BudgetEnforcement {
            levels: BTreeMap::new(),
            native: true,
        }
    }

    /// A hosted arm with an explicit map.
    pub fn hosted(levels: &[(DimensionId, EnforcementLevel)]) -> BudgetEnforcement {
        BudgetEnforcement {
            levels: levels.iter().copied().collect(),
            native: false,
        }
    }

    /// The enforcement level for `dimension`.
    pub fn level(&self, dimension: DimensionId) -> EnforcementLevel {
        if self.native {
            return EnforcementLevel::Enforced;
        }
        self.levels
            .get(&dimension)
            .copied()
            .unwrap_or(EnforcementLevel::Unenforceable)
    }
}

/// One experiment arm's budget declarations — the `validate_match` input
/// (§8.2 `validate_match(arms[])`; §8.5's results-row fields).
#[derive(Debug, Clone)]
pub struct ArmSpec {
    /// The arm's search budget (mandatory — `UnbudgetedArm` when absent).
    pub search_budget: Option<BudgetSpec>,
    /// The arm's eval budget — identical across arms is the comparison
    /// precondition (M3 evaluates the frozen artifact under it alone).
    pub eval_budget: Option<BudgetSpec>,
    /// The arm's inference/test-time-compute budget (M3's third term; required
    /// under `matched_total`).
    pub inference_budget: Option<BudgetSpec>,
    /// The arm's `MatchSpec` (mandatory on every arm — `MissingMatchSpec`).
    pub match_spec: Option<MatchSpec>,
    /// Per-dimension `budget_enforcement` (native arms trivially `enforced`).
    pub enforcement: BudgetEnforcement,
    /// The arm's declared spend-confidence floor — M2 point-admissibility. `None`
    /// = not yet measured (bands, never points).
    pub spend_confidence: Option<crate::pricing::Confidence>,
    /// The arm's declared coverage floor (ppm of 1.0).
    pub coverage_ppm: Option<i64>,
}

impl ArmSpec {
    /// A native arm with the given budgets and spec.
    pub fn native(search: BudgetSpec, eval: BudgetSpec, spec: MatchSpec) -> ArmSpec {
        ArmSpec {
            search_budget: Some(search),
            eval_budget: Some(eval),
            inference_budget: None,
            match_spec: Some(spec),
            enforcement: BudgetEnforcement::native(),
            spend_confidence: None,
            coverage_ppm: None,
        }
    }
}

/// Is `d` admissible under `cross_model`? (OQ-094 answered: `spend`, `time.*`,
/// `model_calls`, `tool_calls`, `approvals.*` — no token-level normalisation.)
fn cross_model_admissible(d: DimensionId) -> bool {
    if d.is_token_role() {
        return false;
    }
    matches!(
        d,
        DimensionId::Spend
            | DimensionId::TimeWallMs
            | DimensionId::TimeWorkingMs
            | DimensionId::TimeModelLatencyMs
            | DimensionId::TimeHumanWaitMs
            | DimensionId::ModelCalls
            | DimensionId::ToolCalls
            | DimensionId::ApprovalsRequested
            | DimensionId::ApprovalsGranted
    )
}

/// `validate_match(arms[]) → ok` (§8.2 §2; ADR-0041 D1–D7). The engine
/// precondition at `register`/`launch`/`settle`/`close` — refusal, never a
/// warning (T-LCD-14).
pub fn validate_match(arms: &[ArmSpec]) -> Result<(), MatchError> {
    // ── presence gates (per-arm) ──────────────────────────────────────────
    for (i, arm) in arms.iter().enumerate() {
        if arm.match_spec.is_none() {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::MissingMatchSpec,
            });
        }
        if arm.search_budget.is_none() || arm.eval_budget.is_none() {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::UnbudgetedArm,
            });
        }
    }
    if arms.len() < 2 {
        return Ok(()); // a single arm has nothing to match against
    }
    let spec0 = arms[0].match_spec.clone().expect("checked");
    if spec0.mode == MatchMode::None {
        // Exploratory runs execute but never report a comparison.
        return Err(MatchError {
            arm: Some(0),
            refusal: MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::ExploratoryNoMatch,
                dimension: None,
                enforceability: None,
            },
        });
    }
    if spec0.dimensions.is_empty() && spec0.mode != MatchMode::MatchedTotal {
        return Err(MatchError {
            arm: Some(0),
            refusal: MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::NoDimensions,
                dimension: None,
                enforceability: None,
            },
        });
    }

    // ── cross-arm spec agreement ──────────────────────────────────────────
    for (i, arm) in arms.iter().enumerate().skip(1) {
        let s = arm.match_spec.as_ref().expect("checked");
        if s.mode != spec0.mode {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::IncommensurableMatch {
                    reason: RefusalReason::MixedMatchModes,
                    dimension: None,
                    enforceability: None,
                },
            });
        }
        if s.cache_policy != spec0.cache_policy {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::IncommensurableMatch {
                    reason: RefusalReason::MixedCachePolicies,
                    dimension: None,
                    enforceability: None,
                },
            });
        }
        if s.model_scope != spec0.model_scope {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::IncommensurableMatch {
                    reason: RefusalReason::MixedModelScopes,
                    dimension: None,
                    enforceability: None,
                },
            });
        }
        let mut d0 = spec0.dimensions.clone();
        d0.sort();
        let mut di = s.dimensions.clone();
        di.sort();
        if d0 != di {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::IncommensurableMatch {
                    reason: RefusalReason::MixedDimensions,
                    dimension: None,
                    enforceability: None,
                },
            });
        }
    }

    // ── eval_budget equality — the comparison precondition (§8.2 consumed-by;
    //    M3 evaluates the frozen artifact under it alone) ──────────────────
    let eval0 = arms[0].eval_budget.as_ref().expect("checked").to_json();
    for (i, arm) in arms.iter().enumerate().skip(1) {
        if arm.eval_budget.as_ref().expect("checked").to_json() != eval0 {
            return Err(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::IncommensurableMatch {
                    reason: RefusalReason::UnequalEvalBudgets,
                    dimension: None,
                    enforceability: None,
                },
            });
        }
    }

    // ── the pricing precondition (one pinned table) ───────────────────────
    // `iso_cost` always prices spend; `matched_cap`/`matched_total` over `spend`
    // and every `cross_model` money comparison need it too — spend at a pinned
    // table is the only cross-model common unit.
    let needs_pricing =
        spec0.mode == MatchMode::IsoCost || spec0.dimensions.contains(&DimensionId::Spend);
    if needs_pricing {
        let p0 = spec0.pricing_table_ref.as_ref().ok_or(MatchError {
            arm: Some(0),
            refusal: MatchRefusal::MissingPricingTable,
        })?;
        for (i, arm) in arms.iter().enumerate().skip(1) {
            match &arm.match_spec.as_ref().expect("checked").pricing_table_ref {
                Some(p) if p == p0 => {}
                Some(_) => {
                    return Err(MatchError {
                        arm: Some(i),
                        refusal: MatchRefusal::IncommensurableMatch {
                            reason: RefusalReason::MixedPricingTables,
                            dimension: None,
                            enforceability: None,
                        },
                    });
                }
                None => {
                    return Err(MatchError {
                        arm: Some(i),
                        refusal: MatchRefusal::MissingPricingTable,
                    });
                }
            }
        }
    }

    // ── per-dimension rules ───────────────────────────────────────────────
    for d in &spec0.dimensions {
        let dim = *d;
        // cross_model admissibility (OQ-094; ADR-0159 D2).
        if spec0.model_scope == ModelScope::CrossModel {
            if dim.is_token_role() {
                return Err(MatchError {
                    arm: None,
                    refusal: MatchRefusal::IncommensurableMatch {
                        reason: RefusalReason::CrossModelTokenMatch,
                        dimension: Some(dim),
                        enforceability: None,
                    },
                });
            }
            if !cross_model_admissible(dim) {
                return Err(MatchError {
                    arm: None,
                    refusal: MatchRefusal::IncommensurableMatch {
                        reason: RefusalReason::CrossModelDimension,
                        dimension: Some(dim),
                        enforceability: None,
                    },
                });
            }
        }
        // Same MeteringFormula across arms (common rule) — read each arm's
        // declared metering for the bound key from its search budget.
        let metering0 = arms[0]
            .search_budget
            .as_ref()
            .expect("checked")
            .metering_map()
            .get(dim.as_str())
            .cloned();
        for (i, arm) in arms.iter().enumerate().skip(1) {
            let m = arm
                .search_budget
                .as_ref()
                .expect("checked")
                .metering_map()
                .get(dim.as_str())
                .cloned();
            if m != metering0 {
                return Err(MatchError {
                    arm: Some(i),
                    refusal: MatchRefusal::IncommensurableMatch {
                        reason: RefusalReason::MixedMeteringFormulas,
                        dimension: Some(dim),
                        enforceability: None,
                    },
                });
            }
        }
        match spec0.mode {
            MatchMode::MatchedCap => {
                // M1: identical hard ceilings on the matched dimensions; every
                // arm's `budget_enforcement[d] = enforced`.
                let cap0 = arms[0]
                    .search_budget
                    .as_ref()
                    .expect("checked")
                    .hard_caps_map()
                    .get(dim.as_str())
                    .copied();
                for (i, arm) in arms.iter().enumerate() {
                    let e = arm.enforcement.level(dim);
                    if e != EnforcementLevel::Enforced {
                        return Err(MatchError {
                            arm: Some(i),
                            refusal: MatchRefusal::IncommensurableMatch {
                                reason: RefusalReason::Unenforceable,
                                dimension: Some(dim),
                                enforceability: Some(e),
                            },
                        });
                    }
                    if i == 0 {
                        continue;
                    }
                    let cap = arm
                        .search_budget
                        .as_ref()
                        .expect("checked")
                        .hard_caps_map()
                        .get(dim.as_str())
                        .copied();
                    let equal = match (cap0, cap) {
                        (Some(a), Some(b)) => {
                            // `tolerance` admits bounded slack: |a−b| ≤ tol × max.
                            let tol = (spec0.tolerance_ppm.max(0) as i128 * (a.max(b) as i128)
                                / PPM_SCALE as i128) as i64;
                            (a - b).abs() <= tol
                        }
                        (None, None) => true,
                        _ => false,
                    };
                    if !equal {
                        return Err(MatchError {
                            arm: Some(i),
                            refusal: MatchRefusal::IncommensurableMatch {
                                reason: RefusalReason::UnequalCaps,
                                dimension: Some(dim),
                                enforceability: None,
                            },
                        });
                    }
                }
            }
            MatchMode::IsoCost => {
                // M2: spend rows with confidence ∈ {estimate, unknown} or
                // coverage < 1 render bands, never points — a declared floor
                // below `bounded` refuses the *comparison*.
                for (i, arm) in arms.iter().enumerate() {
                    if dim != DimensionId::Spend {
                        continue;
                    }
                    let ok = arm
                        .spend_confidence
                        .map(|c| c.is_point_admissible())
                        .unwrap_or(false);
                    if !ok {
                        return Err(MatchError {
                            arm: Some(i),
                            refusal: MatchRefusal::IncommensurableMatch {
                                reason: RefusalReason::SpendConfidenceTooLow,
                                dimension: Some(dim),
                                enforceability: None,
                            },
                        });
                    }
                }
            }
            MatchMode::MatchedTotal => {
                // M3: refused when any arm has an `unenforceable` matched dim.
                for (i, arm) in arms.iter().enumerate() {
                    let e = arm.enforcement.level(dim);
                    if e == EnforcementLevel::Unenforceable {
                        return Err(MatchError {
                            arm: Some(i),
                            refusal: MatchRefusal::IncommensurableMatch {
                                reason: RefusalReason::Unenforceable,
                                dimension: Some(dim),
                                enforceability: Some(e),
                            },
                        });
                    }
                }
            }
            MatchMode::None => unreachable!("refused above"),
        }
    }

    // ── M3: `search + eval + inference` equal across arms ─────────────────
    if spec0.mode == MatchMode::MatchedTotal {
        let total = |arm: &ArmSpec| -> Option<BTreeMap<String, i64>> {
            let mut t: BTreeMap<String, i64> = BTreeMap::new();
            for b in [&arm.search_budget, &arm.eval_budget, &arm.inference_budget]
                .into_iter()
                .flatten()
            {
                for (k, v) in b.hard_caps_map() {
                    *t.entry(k).or_insert(0) += v;
                }
            }
            arm.inference_budget.as_ref()?; // M3 needs all three terms
            Some(t)
        };
        let t0 = total(&arms[0]).ok_or(MatchError {
            arm: Some(0),
            refusal: MatchRefusal::UnbudgetedArm,
        })?;
        for (i, arm) in arms.iter().enumerate().skip(1) {
            let t = total(arm).ok_or(MatchError {
                arm: Some(i),
                refusal: MatchRefusal::UnbudgetedArm,
            })?;
            if t != t0 {
                return Err(MatchError {
                    arm: Some(i),
                    refusal: MatchRefusal::IncommensurableMatch {
                        reason: RefusalReason::UnequalTotals,
                        dimension: None,
                        enforceability: None,
                    },
                });
            }
        }
    }

    Ok(())
}

/// The results-row budget fields (§8.2 `MatchSpec` row): every results row
/// carries `search_budget`, `eval_budget`, `artifact_ref`, `budget_utilization[]`,
/// `exhaustion`, `spend{amount, provenance_mix, confidence, coverage}`.
#[derive(Debug, Clone)]
pub struct ResultsRowFields {
    /// The arm's search budget.
    pub search_budget: BudgetSpec,
    /// The arm's eval budget.
    pub eval_budget: BudgetSpec,
    /// The frozen artifact's ref (evaluated under `eval_budget` alone under M3).
    pub artifact_ref: Option<String>,
    /// `consumed / hard` per bounded dimension (ppm of 1.0).
    pub budget_utilization: Vec<(DimensionId, i64)>,
    /// Whether the run exhausted any hard ceiling.
    pub exhaustion: bool,
    /// The spend summary.
    pub spend: ResultsSpend,
}

/// `spend{amount, provenance_mix, confidence, coverage}` on a results row.
#[derive(Debug, Clone, Default)]
pub struct ResultsSpend {
    /// Spend micro-units (per currency — the common case is one).
    pub amount_micro: i64,
    /// The currency of `amount_micro` (empty when no spend).
    pub currency: String,
    /// `provenance_mix` — micro-units per `CostProvenance`.
    pub provenance_mix: BTreeMap<String, i64>,
    /// The minimum confidence over spend rows (never an average).
    pub confidence: Option<crate::pricing::Confidence>,
    /// The minimum coverage over spend rows (ppm; `None` = no spend rows).
    pub coverage_ppm: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::Confidence;
    use hh_ontology::dimensions::DimensionKey;

    fn caps(caps: &[(DimensionId, i64)]) -> BudgetSpec {
        BudgetSpec::hard_caps(
            crate::spec::BudgetMode::Pool,
            &caps
                .iter()
                .map(|(d, l)| (DimensionKey::Primary(*d), *l))
                .collect::<Vec<_>>(),
        )
    }

    fn arm() -> ArmSpec {
        ArmSpec::native(
            caps(&[
                (DimensionId::ModelCalls, 10),
                (DimensionId::TimeWallMs, 60_000),
            ]),
            caps(&[(DimensionId::ModelCalls, 5)]),
            MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
        )
    }

    #[test]
    fn matched_cap_equal_ceilings_validate() {
        assert!(validate_match(&[arm(), arm()]).is_ok());
    }

    #[test]
    fn missing_match_spec_and_unbudgeted() {
        let mut a = arm();
        a.match_spec = None;
        assert!(matches!(
            validate_match(&[arm(), a]),
            Err(MatchError {
                refusal: MatchRefusal::MissingMatchSpec,
                ..
            })
        ));
        let mut b = arm();
        b.search_budget = None;
        assert!(matches!(
            validate_match(&[arm(), b]),
            Err(MatchError {
                refusal: MatchRefusal::UnbudgetedArm,
                ..
            })
        ));
    }

    #[test]
    fn unequal_caps_refuse() {
        let mut a = arm();
        a.search_budget = Some(caps(&[(DimensionId::ModelCalls, 20)]));
        let e = validate_match(&[arm(), a]).unwrap_err();
        assert!(matches!(
            e.refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::UnequalCaps,
                ..
            }
        ));
    }

    #[test]
    fn advisory_enforcement_refuses_matched_cap() {
        let mut a = arm();
        a.enforcement =
            BudgetEnforcement::hosted(&[(DimensionId::ModelCalls, EnforcementLevel::Advisory)]);
        assert!(matches!(
            validate_match(&[arm(), a.clone()]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::Unenforceable,
                enforceability: Some(EnforcementLevel::Advisory),
                ..
            }
        ));
        // …but the same arm passes `matched_total`'s weaker bar (advisory ≠
        // unenforceable) — matched_total only refuses `unenforceable`.
        a.match_spec = Some(MatchSpec {
            mode: MatchMode::MatchedTotal,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        a.inference_budget = Some(caps(&[(DimensionId::ModelCalls, 1)]));
        a.search_budget = Some(caps(&[(DimensionId::ModelCalls, 10)]));
        let mut b = arm();
        b.match_spec = Some(MatchSpec {
            mode: MatchMode::MatchedTotal,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        b.inference_budget = Some(caps(&[(DimensionId::ModelCalls, 1)]));
        b.search_budget = Some(caps(&[(DimensionId::ModelCalls, 10)]));
        assert!(validate_match(&[b, a]).is_ok());
    }

    #[test]
    fn unenforceable_refuses_matched_total() {
        let mut a = arm();
        a.enforcement = BudgetEnforcement::hosted(&[(
            DimensionId::ModelCalls,
            EnforcementLevel::Unenforceable,
        )]);
        a.match_spec = Some(MatchSpec {
            mode: MatchMode::MatchedTotal,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        a.inference_budget = Some(caps(&[]));
        let mut b = arm();
        b.match_spec = Some(MatchSpec {
            mode: MatchMode::MatchedTotal,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        b.inference_budget = Some(caps(&[]));
        assert!(matches!(
            validate_match(&[b, a]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::Unenforceable,
                ..
            }
        ));
    }

    #[test]
    fn cross_model_token_match_refused() {
        let mut a = arm();
        a.match_spec = Some(MatchSpec {
            model_scope: ModelScope::CrossModel,
            ..MatchSpec::matched_cap(&[DimensionId::TokensInputUncached])
        });
        let mut b = arm();
        b.match_spec = Some(MatchSpec {
            model_scope: ModelScope::CrossModel,
            ..MatchSpec::matched_cap(&[DimensionId::TokensInputUncached])
        });
        assert!(matches!(
            validate_match(&[a, b]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::CrossModelTokenMatch,
                ..
            }
        ));
    }

    #[test]
    fn mixed_cache_policies_refuse() {
        let mut a = arm();
        a.match_spec = Some(MatchSpec {
            cache_policy: CachePolicy::Natural,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        assert!(matches!(
            validate_match(&[arm(), a]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::MixedCachePolicies,
                ..
            }
        ));
    }

    #[test]
    fn iso_cost_needs_one_pinned_table_and_point_confidence() {
        let spec = MatchSpec {
            mode: MatchMode::IsoCost,
            pricing_table_ref: Some(PricingTableRef {
                table_id: "t".into(),
                version: "1".into(),
                pin: Some("sha256:aa".into()),
            }),
            ..MatchSpec::matched_cap(&[DimensionId::Spend])
        };
        let mut a = arm();
        a.match_spec = Some(spec.clone());
        a.spend_confidence = Some(Confidence::Bounded { lo: 0, hi: 10 });
        let mut b = arm();
        b.match_spec = Some(MatchSpec {
            pricing_table_ref: None,
            ..spec.clone()
        });
        b.spend_confidence = Some(Confidence::Exact);
        assert!(matches!(
            validate_match(&[a.clone(), b.clone()]).unwrap_err().refusal,
            MatchRefusal::MissingPricingTable
        ));
        b.match_spec = Some(spec);
        b.spend_confidence = Some(Confidence::Estimate); // bands, never points
        assert!(matches!(
            validate_match(&[a, b]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::SpendConfidenceTooLow,
                ..
            }
        ));
    }

    #[test]
    fn exploratory_mode_never_compares() {
        let mut a = arm();
        a.match_spec = Some(MatchSpec {
            mode: MatchMode::None,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        let mut b = arm();
        b.match_spec = Some(MatchSpec {
            mode: MatchMode::None,
            ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
        });
        assert!(matches!(
            validate_match(&[a, b]).unwrap_err().refusal,
            MatchRefusal::IncommensurableMatch {
                reason: RefusalReason::ExploratoryNoMatch,
                ..
            }
        ));
    }

    #[test]
    fn matchspec_json_round_trip() {
        let s = MatchSpec {
            dimensions: vec![DimensionId::Spend, DimensionId::ModelCalls],
            mode: MatchMode::MatchedTotal,
            tolerance_ppm: 50_000,
            pricing_table_ref: Some(PricingTableRef {
                table_id: "pt".into(),
                version: "v1".into(),
                pin: None,
            }),
            model_scope: ModelScope::CrossModel,
            cache_policy: CachePolicy::Primed("sha256:seed".into()),
        };
        assert_eq!(MatchSpec::from_json(&s.to_json()), Some(s));
    }
}
