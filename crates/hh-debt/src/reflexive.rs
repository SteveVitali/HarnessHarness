//! The reflexive `no_dead_weight_found` discipline (S6.1b; §5h.6 §3's
//! home-16 row; ADR-0197 D10 — the same no-belief-without-a-test
//! discipline that makes a rule's debt honest makes the manager's own
//! debt honest).
//!
//! The manager's own debt record lives on home 16
//! (`assumption_debt_manager.debt`), carries the full vocabulary
//! (hypothesis + evidence + owner + expiry_condition + removal_test
//! `no_dead_weight_found{window}`), and rides the same `evaluate_debt` →
//! `removal_test.settled` → `retire`/`propose_retirement` path as every
//! other record — the *only* manager-special parts are this constructor
//! and the window evaluation in `ManagerView::no_dead_weight_verdict`.

use hh_hir::debt::RemovalTestContext;
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_ontology::debt::{
    DeadWeightWindow, DebtClass, DebtExpiry, DebtPolicy, DebtScope, DebtStatus, DeficiencyClass,
    EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, ExpiryParams, HypothesisSubject,
    HypothesisTyped, OwnerRef, PredictedEffect, RemovalTest, RemovalTestKind, RemovalVerdict,
    Revalidation, RevalidationAction, RevalidationOn,
};
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};

use crate::errors::DebtManagerError;
use crate::view::ManagerView;

/// The home-16 coordinate (the `DebtHomes/1` row the reflexive record
/// validates under).
pub const HOME16_RECORD_KIND: &str = "assumption_debt_manager";
/// The home-16 field.
pub const HOME16_FIELD: &str = "debt";

/// Build the manager's reflexive `AssumptionDebtRecord` (home 16 —
/// `assumption_debt_manager.debt`; member-level valid: the ratified base
/// fields + `debt_class` + the instantiated
/// `no_dead_weight_found{window}` removal test; `hypothesis_typed{
/// subject: component(debt_manager:<id>), predicted_effect: zero}`; a
/// revalidation cadence (`model_change` + `schedule` — AC-R-2.9.6-12);
/// and the `created_by` provenance stamp).
pub fn reflexive_record(
    manager_id: &str,
    owner: &str,
    reach_via: Vec<String>,
    window: DeadWeightWindow,
    probation_window_ms: u64,
    created_at_ms: u64,
) -> Result<AssumptionDebtRecord, DebtManagerError> {
    let prov = ProvenanceRecord::minted(
        Origin::kernel(format!("hh-debt:{manager_id}")),
        PersistenceScope::Project,
        created_at_ms,
    );
    let record = AssumptionDebtRecord {
        rule_id: format!("manager:{manager_id}"),
        hypothesis: Text::new(
            "if no scheduled removal test ever passes across the window, \
             the manager is itself dead weight — retired is the honest end \
             state",
            owner,
            prov.clone(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: EvidenceKind::Source,
            reference: "spec:§5h.6 §3 home-16 (the reflexive row)".to_string(),
            observed_at: Some(created_at_ms),
            tier: None,
            provisional: true,
        }],
        owner: OwnerRef {
            team: false,
            id: owner.to_string(),
            reach_via,
        },
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: Some(window.model_version_changes.to_string()),
        },
        removal_test_ref: "no_dead_weight_found".to_string(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::Unclassified),
        hypothesis_typed: Some(HypothesisTyped {
            subject: HypothesisSubject::Component(format!("debt_manager:{manager_id}")),
            deficiency_class: DeficiencyClass::Unknown("dead_weight".to_string()),
            predicted_effect: PredictedEffect::Zero,
            metric_ref: None,
        }),
        scope: Some(DebtScope::default()),
        expiry: Some(DebtExpiry {
            condition: ExpiryKind::ModelVersionChange,
            params: ExpiryParams {
                until: None,
                evidence_max_age_ms: Some(probation_window_ms),
                dependency_capabilities: vec![],
                design_ref: None,
            },
        }),
        runway_ms: None,
        revalidation: Some(Revalidation {
            on: vec![RevalidationOn::ModelChange, RevalidationOn::Schedule],
            action: Some(RevalidationAction::RefreshEvidence),
        }),
        removal_test: Some(RemovalTest {
            criteria: Some("no scheduled removal test passes across the window".to_string()),
            window: Some(window),
            ..RemovalTest::new(RemovalTestKind::NoDeadWeightFound)
        }),
        created_by: Some(prov),
        created_at: Some(created_at_ms),
        supersedes: None,
    };
    hh_hir::debt::validate_for_home(
        &record,
        HOME16_RECORD_KIND,
        HOME16_FIELD,
        &DebtPolicy::default(),
        &RemovalTestContext::member_level(),
    )
    .map_err(|e| DebtManagerError::Schema(format!("reflexive record: {e:?}")))?;
    Ok(record)
}

/// The reflexive evaluation a sweep performs for the manager's own
/// record: `ManagerView::no_dead_weight_verdict` over the folded window
/// (kept as a named op so the schedule pipeline and the test suite share
/// one entry point — the verdict mints a `removal_test.settled` row like
/// any static test).
pub fn evaluate_reflexive(
    view: &ManagerView,
    reflexive_debt_ref: &str,
    window: DeadWeightWindow,
    now_ms: u64,
    sweep_ref: &str,
) -> RemovalVerdict {
    view.no_dead_weight_verdict(
        reflexive_debt_ref,
        window.model_version_changes,
        now_ms,
        sweep_ref,
    )
}
