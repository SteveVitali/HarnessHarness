//! The fleet defaults' removal-test debt records (§5i.1 6d; S6.4;
//! ADR-0207 D6; ADR-0325): **every fleet default is a conditioned rule
//! with `lab/org-policy-v1` as its removal test** (T-LCD-05). The
//! defaults the recipe discharge-tests are ADR-0206's:
//!
//! - `defer_with_escalation_chain` — the fleet-unattended `ask`
//!   resolution (`defer` + the `EscalationPolicy` chain);
//! - `foreground_cascade` — the foreground-cascade default;
//! - `continuation_delay_ms` — the continuation-delay knob;
//! - `stall_timeout_ms` — the stall window (`Defaults.stall_timeout_ms`
//!   is its durable member).
//!
//! The records are *records-out* — the caller deposits/registers them
//! through the §5h.6 path (`hh_hir::debt::validate_for_home` +
//! `lifecycle.debt.*`); the fleet layer never opens a run for them.
//! Each record's `removal_test` is a `retirement_experiment` whose
//! `template_ref` is the `lab/org-policy-v1` `ExperimentSpec`'s content
//! address — the recipe's matched-cap verdict, never a bespoke check
//! (CC9's matched-budget rule applied to the defaults themselves;
//! ADR-0207 (e)'s maturity flag: every default's retention past its
//! debt expiry needs the recipe's verdict).

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_ontology::debt::DebtStatus;
use hh_ontology::debt::{
    DebtClass, DebtExpiry, DebtScope, DeficiencyClass, EvidenceRef, ExpiryCondition, ExpiryKind,
    ExpiryParams, HypothesisSubject, HypothesisTyped, OwnerRef, PredictedEffect, RemovalTest,
    RemovalTestKind, Revalidation, RevalidationAction, RevalidationOn,
};
use hh_provenance::ProvenanceRecord;

/// The recipe id the removal tests instantiate (`lab/org-policy-v1`'s
/// `ExperimentSpec.design.id` — the `template_ref` the
/// `retirement_experiment` test resolves).
pub const ORG_POLICY_RECIPE: &str = "lab/org-policy-v1";

/// The closed fleet-default set the recipe discharge-tests (ADR-0206's
/// named defaults; §5i.1's Stage-6 row).
pub const FLEET_DEFAULTS: &[&str] = &[
    "defer_with_escalation_chain",
    "foreground_cascade",
    "continuation_delay_ms",
    "stall_timeout_ms",
];

/// The default's rule id (`fleet_default:<fleet>:<name>`) — the debt
/// record's `rule_id` coordinate.
pub fn default_rule_id(fleet: &str, default: &str) -> String {
    format!("fleet_default:{fleet}:{default}")
}

/// `fleet_default_debt_records(fleet, owner, recipe_spec_ref,
/// expires_at_ms, created_by, created_at)` — one conditioned
/// `AssumptionDebtRecord` per member of [`FLEET_DEFAULTS`]:
///
/// - `hypothesis` — the conditioned claim ("the default's benefit
///   survives on the matched dimensions — a `lab/org-policy-v1`
///   regression retires it");
/// - `debt_class: hypothesized` — the defaults are unproven claims
///   until the recipe's verdict lands (the probation leg is the
///   manager's `hypothesized_max_age` accounting);
/// - `expiry{condition: date, params{until: expires_at_ms}}` — the
///   revalidation cadence the default must not outlive (ADR-0207 (e);
///   `revalidation{on: [schedule], action: refresh_evidence}`);
/// - `removal_test{kind: retirement_experiment, template_ref:
///   recipe_spec_ref}` — the `lab/org-policy-v1` template (the caller
///   resolves `template_ref` to the registered spec's
///   `experiment_id`).
///
/// `expires_at_ms = None` produces an unbound expiry — the caller's
/// policy pins the cadence (an absent `until` is honest data, never a
/// fabricated bound).
pub fn fleet_default_debt_records(
    fleet: &str,
    owner: &OwnerRef,
    recipe_spec_ref: &str,
    expires_at_ms: Option<u64>,
    created_by: &ProvenanceRecord,
    created_at: u64,
) -> Vec<AssumptionDebtRecord> {
    FLEET_DEFAULTS
        .iter()
        .map(|name| {
            let rule_id = default_rule_id(fleet, name);
            AssumptionDebtRecord {
                rule_id: rule_id.clone(),
                hypothesis: Text::new(
                    format!(
                        "fleet default `{name}` conditions the org-policy claim — its \
                         retention past expiry needs the `lab/org-policy-v1` \
                         matched_cap verdict (ADR-0207 D6; §5i.1 6d)"
                    ),
                    owner.id.as_str(),
                    created_by.clone(),
                ),
                evidence_refs: vec![EvidenceRef {
                    kind: hh_ontology::debt::EvidenceKind::Source,
                    reference: format!("fleet_default:{name}"),
                    observed_at: Some(created_at),
                    tier: None,
                    provisional: false,
                }],
                owner: owner.clone(),
                expiry_condition: ExpiryCondition {
                    kind: ExpiryKind::Date,
                    value: expires_at_ms.map(|t| t.to_string()),
                },
                removal_test_ref: recipe_spec_ref.to_string(),
                status: DebtStatus::Active,
                debt_class: Some(DebtClass::Hypothesized),
                hypothesis_typed: Some(HypothesisTyped {
                    subject: HypothesisSubject::Deficiency,
                    deficiency_class: DeficiencyClass::PrematureStop,
                    predicted_effect: PredictedEffect::Increase,
                    metric_ref: Some("metric:escalations_per_item".into()),
                }),
                scope: Some(DebtScope {
                    model_selectors: vec![],
                    task_classes: vec!["fleet_activation".into()],
                    roles: vec![],
                }),
                expiry: Some(DebtExpiry {
                    condition: ExpiryKind::Date,
                    params: ExpiryParams {
                        until: expires_at_ms,
                        ..ExpiryParams::default()
                    },
                }),
                runway_ms: None,
                revalidation: Some(Revalidation {
                    on: vec![RevalidationOn::Schedule],
                    action: Some(RevalidationAction::RefreshEvidence),
                }),
                removal_test: Some(RemovalTest {
                    template_ref: Some(recipe_spec_ref.to_string()),
                    ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
                }),
                created_by: Some(created_by.clone()),
                created_at: Some(created_at),
                supersedes: None,
            }
        })
        .collect()
}
