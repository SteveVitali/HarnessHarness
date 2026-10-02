//! `hh-fleet` S6.4 coverage — §5i.1 6d / ADR-0207 D6 / ADR-0325
//! (AC-R-2.12.6-12; T-LCD-05): every fleet default is a conditioned
//! rule whose removal test is the `lab/org-policy-v1` recipe — the
//! `fleet_default_debt_records` projection mints one
//! `AssumptionDebtRecord` per member of the closed default set, each
//! carrying the recipe `template_ref`, a date-bounded expiry, and the
//! `schedule`-revalidation contract (records-out — the caller deposits
//! through the §5h.6 path).

use hh_fleet::org_policy::{
    default_rule_id, fleet_default_debt_records, FLEET_DEFAULTS, ORG_POLICY_RECIPE,
};
use hh_ontology::debt::{
    DebtClass, ExpiryKind, RemovalTestKind, RevalidationAction, RevalidationOn,
};
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};

fn human(author: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author, HumanRole::Principal),
        PersistenceScope::Definition,
        1,
    )
}

fn owner() -> hh_ontology::debt::OwnerRef {
    hh_ontology::debt::OwnerRef {
        team: true,
        id: "team:platform".into(),
        reach_via: vec!["sink:ops".into()],
    }
}

/// One conditioned debt record per fleet default — the closed set is
/// complete (a removal test exists for every ADR-0206 default), each
/// record names `lab/org-policy-v1` as its `retirement_experiment`
/// template (T-LCD-05).
#[test]
fn every_fleet_default_carries_the_recipe_removal_test() {
    let records = fleet_default_debt_records(
        "fleet:prod",
        &owner(),
        "spec:lab/org-policy-v1",
        Some(9_999_999),
        &human("test:fleet"),
        42,
    );
    assert_eq!(records.len(), FLEET_DEFAULTS.len());
    assert_eq!(records.len(), 4, "ADR-0206's closed default set");

    for (name, r) in FLEET_DEFAULTS.iter().zip(records.iter()) {
        // The rule id coordinates `fleet_default:<fleet>:<name>`.
        assert_eq!(r.rule_id, default_rule_id("fleet:prod", name));
        // `debt_class: hypothesized` — the default is an unproven claim
        // until the recipe's verdict lands.
        assert_eq!(r.debt_class, Some(DebtClass::Hypothesized));
        // The removal test is `retirement_experiment` naming the
        // recipe spec ref (never a bespoke check — CC9's
        // matched-budget rule applied to the defaults themselves).
        let test = r.removal_test.as_ref().expect("removal test present");
        assert_eq!(test.kind, RemovalTestKind::RetirementExperiment);
        assert_eq!(test.template_ref.as_deref(), Some("spec:lab/org-policy-v1"));
        assert_eq!(r.removal_test_ref, "spec:lab/org-policy-v1");
        // The date-bounded expiry the cadence must not outlive.
        let expiry = r.expiry.as_ref().expect("expiry member");
        assert_eq!(expiry.condition, ExpiryKind::Date);
        assert_eq!(expiry.params.until, Some(9_999_999));
        // `revalidation{on: [schedule], action: refresh_evidence}` —
        // the revalidation cadence contract.
        let reval = r.revalidation.as_ref().expect("revalidation");
        assert!(reval.on.contains(&RevalidationOn::Schedule));
        assert_eq!(reval.action, Some(RevalidationAction::RefreshEvidence));
        // The record is active at mint.
        assert_eq!(r.status, hh_ontology::debt::DebtStatus::Active);
    }

    // The recipe coordinate is the exemplar's id — the template the
    // `removal_test_ref` resolves (the register-side resolver pins it).
    assert_eq!(ORG_POLICY_RECIPE, "lab/org-policy-v1");
}

/// An unbound `expires_at_ms` produces an honest unbound expiry — the
/// caller's policy pins the cadence; the record never fabricates a
/// bound.
#[test]
fn unbound_expiry_is_honest_not_fabricated() {
    let records = fleet_default_debt_records(
        "fleet:dev",
        &owner(),
        "spec:lab/org-policy-v1",
        None,
        &human("test:fleet"),
        1,
    );
    for r in &records {
        assert_eq!(
            r.expiry.as_ref().and_then(|x| x.params.until),
            None,
            "unbound expiry is data, not a fabricated bound"
        );
        assert_eq!(r.expiry_condition.value, None);
    }
}
