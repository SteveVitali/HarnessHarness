//! The consolidation half of S6.4 (R-2.9.5 6d; §5h.8 §5; ADR-0325):
//!
//! - [`consolidation_candidates`] — the `consolidation_candidates` view
//!   over the folded [`CampaignView`]: `active` candidates ≥ `k` cycles
//!   whose lesson facts are consolidation-kind and whose economics row
//!   bounds the declared factor land as proposal inputs; the
//!   never-consolidate legs (N1–N5) are spelled per row.
//! - [`check_retrained_pin_scope`] — the *policy-conditioned expiry*
//!   gate (R-2.9.5 6d "the pipeline ... refuses candidates whose S6
//!   profile is pinned to a retrained policy without a debt record"): a
//!   candidate binding a retrained snapshot needs every conditioned
//!   rule's debt record to carry `expiry.condition =
//!   model_version_change` **and** `scope.model_selectors` covering the
//!   pinned claim — else `ConditionedRuleIncomplete` at rollout
//!   (`canary_settle`'s debt check's second leg).
//!
//! Nothing here writes a ledger — the view is records-in/records-out
//! (the campaign's durable prefix is already folded; the caller mints
//! the `lifecycle.debt.*`/`measurement.evolution.*` rows the outcome
//! authors).

use std::collections::BTreeMap;

use hh_lab::coevolution::{
    ConsolidationCandidate, ConsolidationInput, ConsolidationPolicy, LessonFact, NeverConsolidate,
};
use hh_wire::Json;

use crate::errors::{EvolutionError, Refusal};
use crate::view::CampaignView;

/// `consolidation_candidates(view, lessons, economics, debt_complete,
/// policy) → (candidates, never_consolidate[])` — the campaign-side
/// fold (R-2.9.5 6d; §5h.8 §5.1 C1–C5/N1–N5):
///
/// - `state` rides the folded `CandidateRecord` (`active` only
///   consolidates).
/// - `cycles_active` counts the candidate's `active` transitions in the
///   durable prefix — the campaign's own activation count, never a
///   caller assertion (the ≥ `k_cycles` leg's numerator).
/// - `economics[rule → ratio]` supplies the projected
///   net-improvement÷per-cycle-cost bound (`None`/absent = `n/a` — the
///   candidate fails the economics leg honestly, never by a fabricated
///   bound).
/// - `debt_complete(rule)` answers whether the rule's debt record is
///   complete — N5's records-in half (the caller resolves the §5h.6
///   home inventory).
pub fn consolidation_candidates(
    view: &CampaignView,
    lessons: &[LessonFact],
    economics: &BTreeMap<String, f64>,
    debt_complete: &dyn Fn(&str) -> bool,
    policy: &ConsolidationPolicy,
) -> (Vec<ConsolidationCandidate>, Vec<(String, NeverConsolidate)>) {
    let mut inputs = Vec::new();
    for (cid, rec) in &view.candidates {
        let cycles_active = rec
            .history
            .iter()
            .filter(|t| t.to == "active")
            .count() as u64;
        let terminal = matches!(
            rec.state.as_str(),
            "rejected" | "withdrawn" | "reverted" | "retired" | "expired"
        );
        inputs.push(ConsolidationInput {
            rule_id: cid.clone(),
            state: rec.state.clone(),
            cycles_active,
            lessons: lessons
                .iter()
                .filter(|l| l.rule_id == *cid)
                .cloned()
                .collect(),
            economics_ratio: economics.get(cid).copied(),
            terminal,
        });
    }
    hh_lab::coevolution::consolidation_candidates(&inputs, debt_complete, policy)
}

/// `check_retrained_pin_scope(touched_rules, conditioned_debts,
/// pinned_claim)` — the policy-conditioned expiry gate (R-2.9.5 6d;
/// §5h.8 §5.2; ADR-0194 D6):
///
/// For every conditioned rule the candidate's diff touched, when the
/// candidate's S6 profile pins a *retrained* snapshot (`pinned_claim` —
/// records-in; `None` = no retrained pin, the check vacuously passes),
/// the rule's `AssumptionDebtRecord` must carry
/// `expiry.condition = model_version_change` **and** a
/// `scope.model_selectors` entry covering the pinned claim — a
/// conditioned rule bound to a retrained policy without its
/// version-scoped expiry is `ConditionedRuleIncomplete` (the S9 debt
/// check's refusal — the pin makes the expiry *conditioned on the
/// policy version*).
///
/// `conditioned_debts` are the debt records' canonical JSON (the
/// `canary_settle` op's records-in shape — `{rule_ref, record{...}}` or
/// the record body itself).
pub fn check_retrained_pin_scope(
    touched_rules: &[String],
    conditioned_debts: &[Json],
    pinned_claim: Option<&hh_lab::model::SnapshotClaim>,
) -> Result<(), EvolutionError> {
    let Some(claim) = pinned_claim else {
        return Ok(());
    };
    for rule in touched_rules {
        let debt = conditioned_debts.iter().find(|d| {
            d.get("rule_ref").and_then(Json::as_str) == Some(rule.as_str())
                || d.get("rule_id").and_then(Json::as_str) == Some(rule.as_str())
                || d.get("record")
                    .and_then(|r| r.get("rule_id"))
                    .and_then(Json::as_str)
                    == Some(rule.as_str())
        });
        let Some(debt) = debt else {
            // The S9 presence check owns this leg — nothing to add here.
            continue;
        };
        let record = debt.get("record").unwrap_or(debt);
        let expiry_model_version = record
            .get("expiry")
            .and_then(|x| x.get("condition"))
            .and_then(Json::as_str)
            .map(|c| c == "model_version_change")
            .unwrap_or(false);
        let covered = record
            .get("scope")
            .and_then(|s| s.get("model_selectors"))
            .and_then(|ms| match ms {
                Json::Arr(v) => Some(v.iter().any(|sel| {
                    hh_ontology::debt::ModelSelector::from_json(sel, "model_selectors")
                        .map(|m| hh_lab::coevolution::selector_covers_claim(&m, claim))
                        .unwrap_or(false)
                })),
                _ => None,
            })
            .unwrap_or(false);
        if !(expiry_model_version && covered) {
            return Err(Refusal::ConditionedRuleIncomplete {
                detail: format!(
                    "conditioned rule `{rule}` pins retrained snapshot `{}` without a \
                     `model_version_change`-expiry debt scoped to it",
                    claim.snapshot_id
                ),
            }
            .into());
        }
    }
    Ok(())
}
