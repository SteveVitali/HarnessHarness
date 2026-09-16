//! `BudgetSpec` / `BudgetNode` / `Reservation` (§8.2 §3; ADR-0040 D1–D3).
//!
//! `BudgetSpec{mode, dimensions: map<dimension, DimensionRule{hard?: Ceiling, soft?:
//! [Threshold], metering?: MeteringFormula, grace?: Grace}>}` — the runtime
//! instantiation of the HIR/1 `Budget` entity. Bound keys are [`DimensionKey`]s —
//! primary dims plus the derived bound names (a `tokens.blended` ceiling evaluates
//! over its components; ADR-0236 D-2).

use hh_ledger::manifest::EventRef;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::BudgetError;
use crate::quantity::{MeteringFormula, ResourceVector};

/// `Ceiling{limit, unit}` (§8.2 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ceiling {
    /// The hard limit in `unit`.
    pub limit: i64,
    /// The unit — must equal the dimension's natural unit (`DimensionId::unit()`);
    /// validated by [`BudgetSpec::validate`].
    pub unit: String,
}

impl Ceiling {
    /// A ceiling in the dimension's natural unit.
    pub fn of(limit: i64, key: DimensionKey) -> Ceiling {
        let unit = match key {
            DimensionKey::Primary(d) => d.unit(),
            // derived token views count tokens
            DimensionKey::Derived(_) => "tokens",
        };
        Ceiling {
            limit,
            unit: unit.to_string(),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("limit", Json::Int(self.limit)),
            ("unit", Json::str(&self.unit)),
        ])
    }

    pub fn from_json(j: &Json) -> Option<Ceiling> {
        Some(Ceiling {
            limit: j.get("limit")?.as_int()?,
            unit: j.get("unit")?.as_str()?.to_string(),
        })
    }
}

/// `Threshold{at_remaining | at_fraction, action: Ref<HarnessRule>}` (§8.2 §3;
/// ADR-0040 D5 — a soft threshold's action is a `HarnessRule` (`insert ContextItem`),
/// rendered by the Model Profile; never affects `check`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Threshold {
    /// When the threshold fires.
    pub at: ThresholdAt,
    /// The `HarnessRule` ref whose action renders the reminder (a `insert
    /// ContextItem` rule — profile-conditioned templates carry an assumption-debt
    /// record, T-LCD-05, on the *rule*, not here).
    pub action: String,
}

/// `at_remaining` (an absolute amount left) | `at_fraction` (ppm of the ceiling used).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThresholdAt {
    /// Fire when `remaining ≤ amount`.
    Remaining(i64),
    /// Fire when `consumed ≥ fraction × ceiling` (ppm of the ceiling).
    Fraction(i64),
}

impl Threshold {
    /// A fraction threshold (`at_fraction`), ppm.
    pub fn at_fraction_ppm(ppm: i64, action: impl Into<String>) -> Threshold {
        Threshold {
            at: ThresholdAt::Fraction(ppm),
            action: action.into(),
        }
    }

    /// An absolute-remaining threshold (`at_remaining`).
    pub fn at_remaining(amount: i64, action: impl Into<String>) -> Threshold {
        Threshold {
            at: ThresholdAt::Remaining(amount),
            action: action.into(),
        }
    }

    /// Has this threshold fired given `consumed`/`limit`?
    pub fn fired(&self, consumed: i64, limit: i64) -> bool {
        match self.at {
            ThresholdAt::Remaining(a) => limit - consumed <= a,
            ThresholdAt::Fraction(ppm) => {
                limit > 0 && consumed * crate::quantity::PPM_SCALE >= ppm * limit
            }
        }
    }

    /// The threshold's stable index key for "once per (budget, threshold, window)" —
    /// the canonical `at` encoding + action ref.
    pub fn key(&self) -> String {
        let at = match self.at {
            ThresholdAt::Remaining(a) => format!("remaining:{a}"),
            ThresholdAt::Fraction(f) => format!("fraction:{f}"),
        };
        format!("{at}@{}", self.action)
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        match self.at {
            ThresholdAt::Remaining(a) => {
                m.insert("at_remaining".to_string(), Json::Int(a));
            }
            ThresholdAt::Fraction(f) => {
                m.insert("at_fraction".to_string(), Json::Int(f));
            }
        }
        m.insert("action".to_string(), Json::str(&self.action));
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<Threshold> {
        let at = match (
            j.get("at_remaining").and_then(Json::as_int),
            j.get("at_fraction").and_then(Json::as_int),
        ) {
            (Some(a), None) => ThresholdAt::Remaining(a),
            (None, Some(f)) => ThresholdAt::Fraction(f),
            _ => return None,
        };
        Some(Threshold {
            at,
            action: j.get("action")?.as_str()?.to_string(),
        })
    }
}

/// `grace` — a per-dimension soft allowance permitting up to `max_calls` terminal
/// summarising calls after the dimension is exhausted (E3; ADR-0040 D5; §05e.2's stop
/// protocol step 5). Never a ceiling widening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grace {
    /// How many grace calls the dimension permits (spec: "one terminal summarising
    /// call" — the field is a count so `0`/`1`/bounded are all expressible; the C0
    /// default is 1).
    pub max_calls: u32,
}

impl Grace {
    /// The C0 default: one terminal summarising call.
    pub fn one() -> Grace {
        Grace { max_calls: 1 }
    }
}

/// `DimensionRule{hard?: Ceiling, soft?: [Threshold], metering?: MeteringFormula,
/// grace?: Grace}` (§8.2 §3 `BudgetSpec.dimensions` value).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DimensionRule {
    /// The hard ceiling — enforced at decision points only (E1/E2).
    pub hard: Option<Ceiling>,
    /// Soft thresholds — `HarnessRule` actions, measured never enforced (E4).
    pub soft: Vec<Threshold>,
    /// How the dimension is metered (provider-unit rows are `provenance = reported`).
    pub metering: Option<MeteringFormula>,
    /// The per-dimension grace allowance (E3).
    pub grace: Option<Grace>,
}

impl DimensionRule {
    /// A hard-only rule.
    pub fn hard(limit: i64, key: DimensionKey) -> DimensionRule {
        DimensionRule {
            hard: Some(Ceiling::of(limit, key)),
            ..Default::default()
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(h) = &self.hard {
            m.insert("hard".to_string(), h.to_json());
        }
        if !self.soft.is_empty() {
            m.insert(
                "soft".to_string(),
                Json::Arr(self.soft.iter().map(Threshold::to_json).collect()),
            );
        }
        if let Some(mt) = &self.metering {
            m.insert("metering".to_string(), mt.to_json());
        }
        if let Some(g) = &self.grace {
            m.insert(
                "grace".to_string(),
                Json::obj([("max_calls", Json::Int(g.max_calls as i64))]),
            );
        }
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<DimensionRule> {
        let soft = match j.get("soft") {
            Some(Json::Arr(ts)) => ts
                .iter()
                .map(Threshold::from_json)
                .collect::<Option<Vec<_>>>()?,
            _ => vec![],
        };
        let grace = match j.get("grace") {
            Some(g) => Some(Grace {
                max_calls: g.get("max_calls")?.as_int()? as u32,
            }),
            None => None,
        };
        Some(DimensionRule {
            hard: j.get("hard").and_then(Ceiling::from_json),
            soft,
            metering: j.get("metering").and_then(MeteringFormula::from_json),
            grace,
        })
    }
}

/// `mode ∈ {slice, pool}` (§8.2 §3; ADR-0040 D2). Root nodes have no mode semantics
/// of their own; the mode is how a *child* draws from its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum BudgetMode {
    /// The child's ceiling moves out of the parent's available amount; the unspent
    /// remainder moves back on completion (affine — never aliased, never dropped).
    Slice,
    /// The child shares the parent's pool and may only tighten — concurrent pool
    /// children reserve before spend.
    #[default]
    Pool,
}

impl BudgetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetMode::Slice => "slice",
            BudgetMode::Pool => "pool",
        }
    }

    pub fn parse(s: &str) -> Option<BudgetMode> {
        match s {
            "slice" => Some(BudgetMode::Slice),
            "pool" => Some(BudgetMode::Pool),
            _ => None,
        }
    }
}

/// `BudgetSpec{mode, dimensions}` (§8.2 §3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BudgetSpec {
    /// `slice` | `pool` — meaningful on children; a root spec carries `pool`.
    pub mode: BudgetMode,
    /// Bound key → rule. Keys are [`DimensionKey`]s: primary dims and the derived
    /// bound names (`tokens.blended` &c.).
    pub dimensions: BTreeMap<DimensionKey, DimensionRule>,
}

impl BudgetSpec {
    /// A spec with the given mode and hard ceilings `{key → limit}`.
    pub fn hard_caps(mode: BudgetMode, caps: &[(DimensionKey, i64)]) -> BudgetSpec {
        let mut s = BudgetSpec {
            mode,
            ..Default::default()
        };
        for (k, limit) in caps {
            s.dimensions.insert(*k, DimensionRule::hard(*limit, *k));
        }
        s
    }

    /// The hard ceiling for `key`, if bounded.
    pub fn hard(&self, key: DimensionKey) -> Option<i64> {
        self.dimensions
            .get(&key)
            .and_then(|r| r.hard.as_ref().map(|c| c.limit))
    }

    /// Structural validation — every bound key in the registry (structurally
    /// guaranteed while `DimensionKey` is a closed sum; the check stays as defense for
    /// a future registered-ext variant), soft ≤ hard, sane amounts.
    pub fn validate(&self) -> Result<(), BudgetError> {
        for (key, rule) in &self.dimensions {
            if DimensionId::parse(key.as_str()).is_none()
                && hh_ontology::dimensions::DerivedDimension::parse(key.as_str()).is_none()
            {
                return Err(BudgetError::DimensionUnknown {
                    dimension: key.as_str().to_string(),
                });
            }
            if let Some(h) = &rule.hard {
                for t in &rule.soft {
                    // Soft above hard: `at_remaining(a)` with `a > limit` fires
                    // before any consumption (remaining starts ≤ a); a
                    // `fraction` past 100% can never fire. Either way the
                    // threshold is degenerate — refuse.
                    let invalid = match t.at {
                        ThresholdAt::Remaining(a) => a > h.limit,
                        ThresholdAt::Fraction(f) => f > crate::quantity::PPM_SCALE,
                    };
                    if invalid {
                        return Err(BudgetError::SoftAboveHard { dimension: *key });
                    }
                }
            }
        }
        Ok(())
    }

    /// `child ≤ parent` — for *every* key the child bounds, the parent must bound it
    /// too and the child's `hard` must not exceed `parent_remaining` (dynamic
    /// containment; the parent-silent key is refused — a child may not widen the
    /// bound *set*, mirroring HIR `DimensionBound::within`).
    ///
    /// `parent_remaining` maps `DimensionKey → remaining` (see [`crate::tree::BudgetTree::remaining`]).
    pub fn within_parent(
        &self,
        parent: &BudgetSpec,
        parent_remaining: &dyn Fn(DimensionKey) -> i64,
    ) -> Result<(), BudgetError> {
        for (key, rule) in &self.dimensions {
            let Some(h) = &rule.hard else { continue };
            if parent.hard(*key).is_none() {
                return Err(BudgetError::BudgetExceedsParent {
                    dimension: *key,
                    child_limit: h.limit,
                    parent_remaining: 0,
                });
            }
            let rem = parent_remaining(*key);
            if h.limit > rem {
                return Err(BudgetError::BudgetExceedsParent {
                    dimension: *key,
                    child_limit: h.limit,
                    parent_remaining: rem,
                });
            }
        }
        Ok(())
    }

    /// The hard-ceiling map (`key → limit`) — the `eval_budget`/`search_budget`
    /// equality basis and M1's comparison input.
    pub fn hard_caps_map(&self) -> BTreeMap<String, i64> {
        self.dimensions
            .iter()
            .filter_map(|(k, r)| r.hard.as_ref().map(|c| (k.as_str().to_string(), c.limit)))
            .collect()
    }

    /// The declared `MeteringFormula`s (`key → formula`) — M-rules' "same
    /// MeteringFormula across arms" input.
    pub fn metering_map(&self) -> BTreeMap<String, MeteringFormula> {
        self.dimensions
            .iter()
            .filter_map(|(k, r)| r.metering.clone().map(|m| (k.as_str().to_string(), m)))
            .collect()
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("mode", Json::str(self.mode.as_str())),
            (
                "dimensions",
                Json::Obj(
                    self.dimensions
                        .iter()
                        .map(|(k, r)| (k.as_str().to_string(), r.to_json()))
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> Option<BudgetSpec> {
        let mode = BudgetMode::parse(j.get("mode")?.as_str()?)?;
        let dims = match j.get("dimensions") {
            Some(Json::Obj(m)) => m,
            _ => return None,
        };
        let mut dimensions = BTreeMap::new();
        for (k, r) in dims {
            let key = DimensionKey::parse(k)?;
            dimensions.insert(key, DimensionRule::from_json(r)?);
        }
        Some(BudgetSpec { mode, dimensions })
    }
}

/// `scope: Ref<Goal | AgentProcess | Procedure | Experiment>` (§8.2 `BudgetNode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BudgetScopeKind {
    /// A `Goal`.
    Goal,
    /// An `AgentProcess`.
    AgentProcess,
    /// A `Procedure`.
    Procedure,
    /// An `Experiment`.
    Experiment,
}

impl BudgetScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetScopeKind::Goal => "goal",
            BudgetScopeKind::AgentProcess => "agent_process",
            BudgetScopeKind::Procedure => "procedure",
            BudgetScopeKind::Experiment => "experiment",
        }
    }

    pub fn parse(s: &str) -> Option<BudgetScopeKind> {
        BudgetScopeKind::all()
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
    }

    fn all() -> &'static [BudgetScopeKind] {
        &[
            BudgetScopeKind::Goal,
            BudgetScopeKind::AgentProcess,
            BudgetScopeKind::Procedure,
            BudgetScopeKind::Experiment,
        ]
    }
}

/// The `scope` ref — `{kind, target}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetScope {
    /// The entity kind the budget covers.
    pub kind: BudgetScopeKind,
    /// The entity ref (semantic id / run-scoped id).
    pub target: String,
}

impl BudgetScope {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.as_str())),
            ("target", Json::str(&self.target)),
        ])
    }

    pub fn from_json(j: &Json) -> Option<BudgetScope> {
        Some(BudgetScope {
            kind: BudgetScopeKind::parse(j.get("kind")?.as_str()?)?,
            target: j.get("target")?.as_str()?.to_string(),
        })
    }
}

/// `BudgetNode{budget_id, scope, parent?, mode, spec, created_by: EventRef,
/// amendments: [EventRef]}` (§8.2 §3) — the runtime instantiation of the HIR/1
/// `Budget` entity. Views `consumed`/`reserved`/`remaining` are projections of
/// `control.budget.*` events — carried on the tree, not stored here.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetNode {
    /// The allocated budget id.
    pub budget_id: String,
    /// The scope the budget covers.
    pub scope: BudgetScope,
    /// The parent node (`None` on the one root per run).
    pub parent: Option<String>,
    /// `slice` | `pool` — how this node draws from its parent (roots carry `pool`).
    pub mode: BudgetMode,
    /// The declared spec.
    pub spec: BudgetSpec,
    /// The `control.budget.allocated` event that created the node.
    pub created_by: EventRef,
    /// The `control.budget.amended` events applied, in order.
    pub amendments: Vec<EventRef>,
    /// Whether the node has completed (slice remainder released).
    pub completed: bool,
}

/// `Reservation{reservation_id, budget_id, holder, quantity, expires_with: lease}`
/// (§8.2 §3; ADR-0040 D3). `holder` is the `EffectId | ModelCallId` the reservation
/// is held for; retries of `unknown` effects re-use the same reservation.
#[derive(Debug, Clone, PartialEq)]
pub struct Reservation {
    /// The allocated reservation id.
    pub reservation_id: String,
    /// The budget node the claim is held against.
    pub budget_id: String,
    /// The holder — `effect:<id>` or `model_call:<id>`.
    pub holder: String,
    /// The claimed quantity (a `ResourceVector` — one entry per dimension claimed).
    pub quantity: ResourceVector,
    /// `expires_with` — the lease scope the reservation dies with (`writer` at
    /// Stage 1; `reap` releases it on lease loss).
    pub expires_with: String,
    /// The reservation's TTL in ms (sizing/hint for reapers; expiry is by lease loss).
    pub ttl_ms: u64,
    /// How much of the claim has been consumed by charges naming this reservation.
    pub consumed: ResourceVector,
    /// How much of the claim has been released (`reservation_excess`/`release`/
    /// `lease_lost`).
    pub released: ResourceVector,
    /// Whether the reservation is still outstanding (released ⇒ not claimable).
    pub outstanding: bool,
}

impl Reservation {
    /// The claim still held on `dim` — `quantity − consumed − released` (affine:
    /// a reservation's claim is consumed, released or dies with the lease; it is
    /// never aliased and never silently dropped — ADR-0040 D3).
    pub fn claim_left(&self, dim: DimensionId) -> i64 {
        self.quantity.get(dim) - self.consumed.get(dim) - self.released.get(dim)
    }

    /// Any dim still holding a claim.
    pub fn has_claim_left(&self) -> bool {
        self.quantity.iter().any(|(d, _)| self.claim_left(d) > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ontology::dimensions::DerivedDimension;

    fn spec(caps: &[(DimensionKey, i64)]) -> BudgetSpec {
        BudgetSpec::hard_caps(BudgetMode::Pool, caps)
    }

    #[test]
    fn spec_round_trip() {
        let mut s = spec(&[
            (DimensionKey::Primary(DimensionId::ModelCalls), 10),
            (
                DimensionKey::Derived(DerivedDimension::TokensBlended),
                100_000,
            ),
        ]);
        s.dimensions
            .get_mut(&DimensionKey::Primary(DimensionId::ModelCalls))
            .unwrap()
            .soft
            .push(Threshold::at_fraction_ppm(800_000, "rule:budget-reminder"));
        let j = s.to_json();
        assert_eq!(BudgetSpec::from_json(&j), Some(s));
    }

    #[test]
    fn validate_refuses_unknown_and_soft_above_hard() {
        let mut s = BudgetSpec::default();
        s.dimensions.insert(
            // Not a registry name — keys come pre-parsed in `dimensions`, so exercise
            // `validate` through `from_json` refusal instead.
            DimensionKey::Primary(DimensionId::ModelCalls),
            DimensionRule {
                hard: Some(Ceiling {
                    limit: 10,
                    unit: "calls".into(),
                }),
                soft: vec![Threshold::at_remaining(20, "r")],
                ..Default::default()
            },
        );
        assert!(matches!(
            s.validate(),
            Err(BudgetError::SoftAboveHard { .. })
        ));
        // Unknown names can't even enter via from_json.
        let j = hh_wire::json::parse(
            r#"{"dimensions":{"gpu_hours":{"hard":{"limit":1,"unit":"h"}}},"mode":"pool"}"#,
        )
        .unwrap();
        assert!(BudgetSpec::from_json(&j).is_none());
    }

    #[test]
    fn containment_checks_every_bounded_key() {
        let parent = spec(&[(DimensionKey::Primary(DimensionId::ModelCalls), 10)]);
        let child_ok = spec(&[(DimensionKey::Primary(DimensionId::ModelCalls), 4)]);
        let child_over = spec(&[(DimensionKey::Primary(DimensionId::ModelCalls), 11)]);
        let child_new_dim = spec(&[(DimensionKey::Primary(DimensionId::ToolCalls), 1)]);
        let rem = |_: DimensionKey| 10;
        assert!(child_ok.within_parent(&parent, &rem).is_ok());
        assert!(matches!(
            child_over.within_parent(&parent, &rem),
            Err(BudgetError::BudgetExceedsParent { .. })
        ));
        // A parent-silent key is refused — child bounds ⊆ parent bounds.
        assert!(matches!(
            child_new_dim.within_parent(&parent, &rem),
            Err(BudgetError::BudgetExceedsParent { .. })
        ));
    }
}
