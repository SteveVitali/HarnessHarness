//! `hh-debt` S6.4 coverage — R-2.9.8 §2.3 (§5h.8; ADR-0325;
//! AC-R-2.9.8-7): the import-driven `post_import_sweep` — a
/// `model_version_change`-conditioned record whose
/// `scope.model_selectors` covers the imported snapshot transitions
/// `→ expiring{trigger: model_version_change}` (the import is the
/// trigger — `evidence_ref` names the snapshot); scoped records the
/// snapshot does *not* cover run `schedule_removal_test` (the
/// reverse-sweep side); `sweep.completed` carries
/// `kind: post_import` + `trigger_ref` (never attributed to cadence).
use std::path::PathBuf;

use hh_debt::manager::DebtManager;
use hh_debt::records::{DebtManagerRecord, SweepEntry};
use hh_debt::reflexive::reflexive_record;
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_hir::refs::RunRef;
use hh_lab::coevolution::{self as coe};
use hh_lab::debt::DebtObservables;
use hh_lab::model::{PolicyVersionExposed, SnapshotClaim};
use hh_ledger::ids::ManualClock;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::debt::{
    DeadWeightWindow, DebtClass, DebtExpiry, DebtPolicy, DebtScope, DebtStatus, EvidenceKind,
    EvidenceRef, ExpiryCondition, ExpiryKind, ExpiryParams, HypothesisSubject, HypothesisTyped,
    ModelSelector, OwnerRef, PredictedEffect, RemovalTest, RemovalTestKind,
};
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-debt-s64-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn rig(tag: &str) -> (PathBuf, ManualClock, Store, String) {
    let root = tmp(tag);
    let clock = ManualClock::at(1_000_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let manifest = RunManifest::minimal(RunKind::Agent);
    let (run_id, _lease) = store.open_run(manifest, "test:holder").unwrap();
    (root, clock, store, run_id)
}

fn human(author: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author, HumanRole::Principal),
        PersistenceScope::Run,
        1,
    )
}

/// A conditioned debt — `model_version_change` expiry + a
/// `scope.model_selectors` entry (the AC-R-2.9.8-12 shape).
fn conditioned_debt(rule_id: &str, model_id: &str) -> AssumptionDebtRecord {
    let prov = human("test:owner");
    AssumptionDebtRecord {
        rule_id: rule_id.into(),
        hypothesis: Text::new(
            format!("{rule_id} conditions a snapshot-scoped claim"),
            "test:owner",
            prov.clone(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: EvidenceKind::Source,
            reference: format!("ev:{rule_id}"),
            observed_at: Some(1),
            tier: None,
            provisional: false,
        }],
        owner: OwnerRef {
            team: false,
            id: "test:owner".into(),
            reach_via: vec!["sink:ops".into()],
        },
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: format!("tmpl:{rule_id}"),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::ModelConditioned),
        hypothesis_typed: Some(HypothesisTyped {
            subject: HypothesisSubject::Deficiency,
            deficiency_class: hh_ontology::debt::DeficiencyClass::PrematureStop,
            predicted_effect: PredictedEffect::Increase,
            metric_ref: Some("metric:task_success".into()),
        }),
        scope: Some(DebtScope {
            model_selectors: vec![ModelSelector::Exact {
                model_id: model_id.into(),
            }],
            task_classes: vec![],
            roles: vec![],
        }),
        expiry: Some(DebtExpiry {
            condition: ExpiryKind::ModelVersionChange,
            params: ExpiryParams::default(),
        }),
        runway_ms: None,
        revalidation: None,
        removal_test: Some(RemovalTest {
            template_ref: Some(format!("tmpl:{rule_id}")),
            ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
        }),
        created_by: Some(prov),
        created_at: Some(1),
        supersedes: None,
    }
}

fn entry_for(record: &AssumptionDebtRecord) -> SweepEntry {
    SweepEntry {
        debt_ref: format!("debt:{}", record.rule_id),
        home: Some(1),
        version_id: Some("v:1".into()),
        record: record.clone(),
        observables: DebtObservables::default(),
        template: None,
        used_by: vec![],
    }
}

fn manager_record(manager_id: &str) -> DebtManagerRecord {
    let reflexive = reflexive_record(
        manager_id,
        "test:owner",
        vec!["sink:ops".into()],
        DeadWeightWindow {
            model_version_changes: 2,
        },
        60_000,
        1,
    )
    .unwrap();
    DebtManagerRecord {
        manager_id: manager_id.to_string(),
        maturity: "instrument-grade".to_string(),
        policy: DebtPolicy::default(),
        reflexive_debt: reflexive,
    }
}

fn claim() -> SnapshotClaim {
    SnapshotClaim {
        provider: "prov:acme".into(),
        model_id: "model:m-7".into(),
        snapshot_id: "snap:imported-1".into(),
        serving_route: None,
        base_snapshot_ref: None,
        training_lineage: None,
        trained_under: vec![],
        training_cutoff_claim: None,
        weights_digest: None,
        policy_version_exposed: PolicyVersionExposed::Supported,
    }
}

/// The import-driven sweep: the covered debt transitions
/// `active → expiring{model_version_change}`; the uncovered scoped debt
/// schedules (or defers `template_unresolved` — the same
/// bound + budget legs `sweep` runs); `sweep.completed` attributes the
/// run to the import (`kind: post_import`, `trigger_ref` the snapshot).
#[test]
fn post_import_sweep_transitions_covered_and_schedules_rest() {
    let (_root, _clock, mut store, run) = rig("sweep");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    mgr.register(&mut store, &manager_record("m1")).unwrap();

    let covered = conditioned_debt("rule:covered", "model:m-7");
    let scheduled = conditioned_debt("rule:scheduled", "model:other");
    let mut unscoped = conditioned_debt("rule:unscoped", "model:m-7");
    unscoped.scope = None;
    let debts = vec![covered.clone(), scheduled.clone(), unscoped.clone()];
    let c = claim();

    // The lab-side partition (records-out).
    let sweep = coe::post_import_sweep(&debts, &c);
    assert_eq!(sweep.covered, vec!["rule:covered".to_string()]);
    assert_eq!(sweep.scheduled, vec!["rule:scheduled".to_string()]);
    assert_eq!(sweep.unchanged, vec!["rule:unscoped".to_string()]);

    let entries = vec![
        entry_for(&covered),
        entry_for(&scheduled),
        entry_for(&unscoped),
    ];
    let report = mgr
        .post_import_sweep(
            &mut store,
            "m1",
            "snap:imported-1",
            &sweep,
            &entries,
            2_000_000,
            None,
        )
        .unwrap();

    // covered → expiring, evidence_ref names the imported snapshot.
    assert_eq!(report.transitions.len(), 1);
    let t = &report.transitions[0];
    assert_eq!(t.debt_ref, "debt:rule:covered");
    assert_eq!(t.from, DebtStatus::Active);
    assert_eq!(t.to, DebtStatus::Expiring);
    assert_eq!(t.trigger, "model_version_change");
    assert_eq!(t.evidence_ref.as_deref(), Some("snap:imported-1"));

    // scheduled → a removal test schedules or defers honestly
    // (`template_unresolved` — the caller projected no template; the
    // leg runs, never vanishes).
    let scheduled_or_deferred: Vec<String> = report
        .scheduled
        .iter()
        .map(|s| s.debt_ref.clone())
        .chain(report.deferred.iter().map(|(d, _)| d.clone()))
        .collect();
    assert!(scheduled_or_deferred.contains(&"debt:rule:scheduled".to_string()));

    // The durable rows landed: `status.changed` for the covered debt,
    // `sweep.completed` attributed to the import.
    let events = store.events(&run).unwrap();
    let statuses: Vec<_> = events
        .iter()
        .filter(|e| e.class == "lifecycle.debt.status.changed")
        .collect();
    assert_eq!(statuses.len(), 1);
    assert_eq!(
        statuses[0].payload.get("debt_ref").and_then(Json::as_str),
        Some("debt:rule:covered")
    );
    assert_eq!(
        statuses[0].payload.get("trigger").and_then(Json::as_str),
        Some("model_version_change")
    );

    let completed = events
        .iter()
        .find(|e| e.class == "lifecycle.debt.sweep.completed")
        .expect("sweep.completed row");
    assert_eq!(
        completed.payload.get("kind").and_then(Json::as_str),
        Some("post_import")
    );
    assert_eq!(
        completed.payload.get("trigger_ref").and_then(Json::as_str),
        Some("snap:imported-1")
    );

    // The unscoped debt never touched — no transition, no schedule.
    assert!(!scheduled_or_deferred.contains(&"debt:rule:unscoped".to_string()));
    assert!(!events.iter().any(|e| {
        e.class == "lifecycle.debt.status.changed"
            && e.payload.get("debt_ref").and_then(Json::as_str) == Some("debt:rule:unscoped")
    }));
}
