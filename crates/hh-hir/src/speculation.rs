//! `SpeculationPolicy` — the §5a.4 `HarnessRule`-class artifact (ADR-0134 §5;
//! S2.3's shared AC-R-2.2.4-12 floor, consumed by the S2.9 branch model).
//!
//! The **policy floor**: `defer_irreversible = true` is a hard floor no
//! composition layer, `authority_cap` override or evolution `HirDiff` may
//! loosen, and `allow_classes` only ever narrows below the parent's —
//! `check_override(parent, proposed)` raises `AuthorityWidening` /
//! `ConditionedRuleIncomplete` (AC-R-2.2.4-12; the ratified OQ-324 default —
//! narrowing only — holds until the Stage-4 question resolves).

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::errors::HirError;

/// `allow_classes ⊆ {read_only, reversible_workspace_local, compensable}`
/// (§5a.4 `SpeculationPolicy` — the closed class sum a speculative branch may
/// run un-deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpeculativeClass {
    /// `read_only` — always admissible.
    ReadOnly,
    /// `reversible ∧ workspace_local` — the default admissible set's second
    /// member.
    ReversibleWorkspaceLocal,
    /// `compensable` — opt-in only, and only with a registered
    /// `CompensationPlan` at `prepare` (`require_compensator`).
    Compensable,
}

impl SpeculativeClass {
    /// The closed-set spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SpeculativeClass::ReadOnly => "read_only",
            SpeculativeClass::ReversibleWorkspaceLocal => "reversible_workspace_local",
            SpeculativeClass::Compensable => "compensable",
        }
    }

    /// Parse a member spelling (`None` on any other — the sum is closed).
    pub fn parse(s: &str) -> Option<SpeculativeClass> {
        match s {
            "read_only" => Some(SpeculativeClass::ReadOnly),
            "reversible_workspace_local" => Some(SpeculativeClass::ReversibleWorkspaceLocal),
            "compensable" => Some(SpeculativeClass::Compensable),
            _ => None,
        }
    }
}

/// `SpeculationPolicy{allow_classes, require_compensator,
/// max_concurrent_branches, budget_fraction, coverage_required,
/// defer_irreversible}` (§5a.4 §"types"; ADR-0134 §5).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeculationPolicy {
    /// The risk classes a speculative branch may run un-deferred (default
    /// `{read_only, reversible_workspace_local}`).
    pub allow_classes: BTreeSet<SpeculativeClass>,
    /// A `compensable` effect needs a registered `CompensationPlan` at
    /// `prepare` (default `true`).
    pub require_compensator: bool,
    /// The concurrent-branch bound `K`.
    pub max_concurrent_branches: u32,
    /// The branch budget ceiling as a fraction of the parent's slice
    /// (0 < f ≤ 1).
    pub budget_fraction: f64,
    /// Snapshot coverage a branch's `env: snapshot` must declare.
    pub coverage_required: BTreeSet<String>,
    /// The hard floor — `true` always; no layer loosens it.
    pub defer_irreversible: bool,
}

impl SpeculationPolicy {
    /// The §5a.4 default: `{read_only, reversible_workspace_local}`,
    /// `require_compensator`, `K = 1`, `budget_fraction = 0.25`,
    /// `defer_irreversible = true`.
    pub fn default_policy() -> SpeculationPolicy {
        SpeculationPolicy {
            allow_classes: BTreeSet::from([
                SpeculativeClass::ReadOnly,
                SpeculativeClass::ReversibleWorkspaceLocal,
            ]),
            require_compensator: true,
            max_concurrent_branches: 1,
            budget_fraction: 0.25,
            coverage_required: BTreeSet::new(),
            defer_irreversible: true,
        }
    }

    /// `fork(env: trace_only)`'s forced policy — `allow_classes =
    /// {read_only}` (B3: sharing a live environment between two writing
    /// branches is a refusal, never a default).
    pub fn trace_only() -> SpeculationPolicy {
        SpeculationPolicy {
            allow_classes: BTreeSet::from([SpeculativeClass::ReadOnly]),
            ..SpeculationPolicy::default_policy()
        }
    }

    /// The `HarnessRule`-body member form (the artifact's payload is
    /// MUST-data; this is its canonical spelling).
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "allow_classes",
                Json::Arr(
                    self.allow_classes
                        .iter()
                        .map(|c| Json::str(c.as_str()))
                        .collect(),
                ),
            ),
            ("require_compensator", Json::Bool(self.require_compensator)),
            (
                "max_concurrent_branches",
                Json::Int(self.max_concurrent_branches as i64),
            ),
            (
                "budget_fraction_milli",
                Json::Int((self.budget_fraction * 1000.0).round() as i64),
            ),
            (
                "coverage_required",
                Json::Arr(self.coverage_required.iter().map(Json::str).collect()),
            ),
            ("defer_irreversible", Json::Bool(self.defer_irreversible)),
        ])
    }

    /// `from_json` — the closed product (unknown members refuse).
    pub fn from_json(j: &Json) -> Result<SpeculationPolicy, HirError> {
        let mut p = SpeculationPolicy::default_policy();
        let Json::Obj(m) = j else {
            return Err(HirError::SchemaViolation {
                detail: "SpeculationPolicy must be an object".to_string(),
            });
        };
        for (k, v) in m {
            match k.as_str() {
                "allow_classes" => {
                    let Json::Arr(rows) = v else {
                        return Err(HirError::SchemaViolation {
                            detail: "allow_classes must be a list".to_string(),
                        });
                    };
                    p.allow_classes = rows
                        .iter()
                        .filter_map(Json::as_str)
                        .map(|s| {
                            SpeculativeClass::parse(s).ok_or_else(|| HirError::UnknownKind {
                                kind: format!("speculative class {s}"),
                            })
                        })
                        .collect::<Result<_, _>>()?;
                }
                "require_compensator" => {
                    p.require_compensator = matches!(v, Json::Bool(true));
                }
                "max_concurrent_branches" => {
                    p.max_concurrent_branches = v.as_int().map(|n| n.max(0) as u32).unwrap_or(1);
                }
                "budget_fraction_milli" => {
                    if let Some(m) = v.as_int() {
                        p.budget_fraction = m as f64 / 1000.0;
                    }
                }
                "coverage_required" => {
                    if let Json::Arr(rows) = v {
                        p.coverage_required = rows
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect();
                    }
                }
                "defer_irreversible" => {
                    p.defer_irreversible = matches!(v, Json::Bool(true));
                }
                other => {
                    return Err(HirError::SchemaViolation {
                        detail: format!("SpeculationPolicy.{other}: unknown member"),
                    });
                }
            }
        }
        Ok(p)
    }

    /// The floor check — `proposed` must be a **narrowing** of `parent`
    /// (AC-R-2.2.4-12). Refuses `defer_irreversible = false` and any
    /// `allow_classes` widening — under a composition layer, an
    /// `authority_cap` override or an evolution `HirDiff` alike (the check is
    /// over the policy pair, not the vehicle — one floor, CC1).
    ///
    /// `rule_id` is the conditioned rule the override rides (a `HarnessRule`
    /// conditioned on a profile without a complete assumption-debt record is
    /// `ConditionedRuleIncomplete` — T-LCD-05 joins the floor).
    pub fn check_override(
        parent: &SpeculationPolicy,
        proposed: &SpeculationPolicy,
        conditioned_rule_id: Option<&str>,
        debt_recorded: bool,
    ) -> Result<(), HirError> {
        if !proposed.defer_irreversible {
            return Err(HirError::AuthorityWidening {
                detail: "SpeculationPolicy.defer_irreversible is a hard floor — \
                         no layer, override or HirDiff loosens it (ADR-0134 §5)"
                    .to_string(),
            });
        }
        if !proposed.allow_classes.is_subset(&parent.allow_classes) {
            let widened: Vec<&str> = proposed
                .allow_classes
                .difference(&parent.allow_classes)
                .map(|c| c.as_str())
                .collect();
            return Err(HirError::AuthorityWidening {
                detail: format!(
                    "SpeculationPolicy.allow_classes widened by {widened:?} — \
                     narrowing only (OQ-324's ratified default)"
                ),
            });
        }
        if proposed.max_concurrent_branches > parent.max_concurrent_branches
            || proposed.budget_fraction > parent.budget_fraction
        {
            return Err(HirError::AuthorityWidening {
                detail: "SpeculationPolicy bounds (max_concurrent_branches, \
                         budget_fraction) only ever tighten"
                    .to_string(),
            });
        }
        if let Some(rule_id) = conditioned_rule_id {
            if !debt_recorded {
                return Err(HirError::ConditionedRuleIncomplete {
                    rule_id: rule_id.to_string(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_2_2_4_12_defer_irreversible_is_a_hard_floor() {
        let parent = SpeculationPolicy::default_policy();
        let mut loosened = parent.clone();
        loosened.defer_irreversible = false;
        let err = SpeculationPolicy::check_override(&parent, &loosened, None, false).unwrap_err();
        assert!(matches!(err, HirError::AuthorityWidening { .. }));
    }

    #[test]
    fn ac_2_2_4_12_allow_classes_only_narrows() {
        let parent = SpeculationPolicy::trace_only(); // {read_only}
        let mut widened = parent.clone();
        widened.allow_classes.insert(SpeculativeClass::Compensable);
        let err = SpeculationPolicy::check_override(&parent, &widened, None, false).unwrap_err();
        assert!(matches!(err, HirError::AuthorityWidening { .. }));
        // Narrowing to {read_only} of a wider parent is admitted.
        let wide = SpeculationPolicy::default_policy();
        assert!(SpeculationPolicy::check_override(&wide, &parent, None, false).is_ok());
    }

    #[test]
    fn ac_2_2_4_12_conditioned_rule_needs_its_debt_record() {
        let p = SpeculationPolicy::default_policy();
        let err = SpeculationPolicy::check_override(&p, &p, Some("rule-x"), false).unwrap_err();
        assert!(matches!(err, HirError::ConditionedRuleIncomplete { .. }));
        assert!(SpeculationPolicy::check_override(&p, &p, Some("rule-x"), true).is_ok());
    }

    #[test]
    fn ac_2_2_4_12_bounds_only_tighten() {
        let parent = SpeculationPolicy::default_policy();
        let mut looser = parent.clone();
        looser.max_concurrent_branches = parent.max_concurrent_branches + 1;
        assert!(matches!(
            SpeculationPolicy::check_override(&parent, &looser, None, false),
            Err(HirError::AuthorityWidening { .. })
        ));
        let mut tighter = parent.clone();
        tighter.budget_fraction = parent.budget_fraction / 2.0;
        assert!(SpeculationPolicy::check_override(&parent, &tighter, None, false).is_ok());
    }
}
