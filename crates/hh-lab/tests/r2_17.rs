//! R2.17 — the `DebtPolicy.grace_period_ms` runtime consumer (DF-S1.24-1):
//! `ProfileChange.changed_at_ms` lets `evaluate_debt` derive the
//! `retired`-grace hard leg under the operative policy — `changed_at_ms +
//! grace_period_ms <= now_ms` — rather than trusting a caller-projected
//! `grace_elapsed` flag alone (the flag and the derived leg are OR-ed;
//! the live-trigger seam stamps `changed_at_ms` so a later sweep gets the
//! policy-true answer).

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_ontology::debt::{
    DebtClass, DebtPolicy, DebtScope, DebtStatus, EvidenceRef, ExpiryCondition, ExpiryKind,
    ModelSelector, OwnerRef, RemovalTest, RemovalTestKind,
};
use hh_provenance::ProvenanceRecord;

use hh_lab::debt::{evaluate_debt, DebtObservables, ProfileChange, ProfileChangeKind};

fn debt_record(rule_id: &str) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: rule_id.into(),
        hypothesis: Text::new(
            "the assumption holds while the profile does",
            "owner:o",
            ProvenanceRecord::kernel("r217.test", 0),
        ),
        evidence_refs: vec![EvidenceRef::legacy("sha256:ev-1")],
        owner: OwnerRef::principal("test:owner"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "sha256:test".into(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::Hypothesized),
        hypothesis_typed: None,
        scope: Some(DebtScope {
            model_selectors: vec![ModelSelector::Exact {
                model_id: "test:model".into(),
            }],
            ..Default::default()
        }),
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: Some(RemovalTest::new(RemovalTestKind::Inspection)),
        created_by: None,
        created_at: Some(1),
        supersedes: None,
    }
}

fn retired_change(changed_at_ms: u64) -> DebtObservables {
    DebtObservables {
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::Retired,
            grace_elapsed: false,
            changed_at_ms: Some(changed_at_ms),
            profile_ref: "profile:p-1".into(),
        }),
        ..Default::default()
    }
}

/// Inside the operative grace window the retired-profile change warns
/// (`expiring`), never expires — the caller's `grace_elapsed` flag is
/// `false` and `changed_at_ms + grace_period_ms > now_ms`.
#[test]
fn changed_at_ms_inside_grace_warns() {
    let r = debt_record("r1");
    let policy = DebtPolicy {
        grace_period_ms: 30_000,
        ..DebtPolicy::default()
    };
    // profile home id 4 (model_profile_ext) — the profile-change family.
    let ts = evaluate_debt(
        "debt:r1",
        &r,
        Some(4),
        &retired_change(1_000),
        20_000,
        &policy,
    );
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expiring);
}

/// Past the operative grace window the derived leg fires the hard
/// transition — `changed_at_ms + grace_period_ms <= now_ms` under the
/// manager's policy, not a stubbed flag.
#[test]
fn changed_at_ms_past_grace_expires() {
    let r = debt_record("r1");
    let policy = DebtPolicy {
        grace_period_ms: 30_000,
        ..DebtPolicy::default()
    };
    let ts = evaluate_debt(
        "debt:r1",
        &r,
        Some(4),
        &retired_change(1_000),
        31_001,
        &policy,
    );
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expired);
    assert!(ts[0].causes.contains(&"profile_change:retired".to_string()));
}

/// The derived leg is policy-relative: the same `changed_at_ms`/`now`
/// pair warns under a longer `grace_period_ms`.
#[test]
fn grace_leg_tracks_the_operative_policy() {
    let r = debt_record("r1");
    let wide = DebtPolicy {
        grace_period_ms: 10_000_000,
        ..DebtPolicy::default()
    };
    let ts = evaluate_debt(
        "debt:r1",
        &r,
        Some(4),
        &retired_change(1_000),
        31_001,
        &wide,
    );
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expiring);
}
