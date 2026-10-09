//! R2.17 — DF-S1.24-1's rowed residuals: the production resolver-backed
//! `validate_removal_test` context checks inside `schedule_removal_test`/
//! `schedule_batch` (the operative `DebtPolicy` + the caller-resolved
//! template supply the resolvers — never test stubs), and the `DebtPolicy`
//! members that now bind: `min_runway_ms` (`insufficient_runway`),
//! `notice_sinks` (`owner_unreachable`), `dead_weight_designs_allowed`
//! (`dead_weight_design_disallowed`), `schedule` (the `sweep.completed`
//! `cadence` attribution + non-empty at register), `priority` (closed-set
//! at register, not just per-sweep).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::matchspec::MatchSpec;
use hh_debt::records::{DebtManagerRecord, SchedulableEntry, SweepEntry};
use hh_debt::reflexive::reflexive_record;
use hh_debt::schedule::{
    schedule_batch, schedule_removal_test, DeferredReason, ScheduleContext, ScheduleOutcome,
};
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};
use hh_ledger::ids::ManualClock;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::config::Ref;
use hh_ontology::debt::{
    DeadWeightWindow, DebtClass, DebtExpiry, DebtPolicy, DebtScope, DebtStatus, DeficiencyClass,
    EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, ExpiryParams, HypothesisSubject,
    HypothesisTyped, OwnerRef, PredictedEffect, RemovalTest, RemovalTestKind, Revalidation,
    RevalidationAction, RevalidationOn,
};
use hh_ontology::eval::{
    Design, DesignKind, FactorKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};

// ── fixtures (same shapes as `s6_1b.rs` — kept local so this battery
// stands alone) ──────────────────────────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-debt-r2-17-{}-{}", tag, std::process::id()));
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

/// The fixture policy — `notice_sinks` declares `sink:ops`, the sink the
/// fixture records' `owner.reach_via` names (the schedule point binds the
/// declared sink table — an empty table with a declared `reach_via` is
/// `owner_unreachable`, never silently scheduled).
fn policy() -> DebtPolicy {
    DebtPolicy {
        notice_sinks: vec!["sink:ops".into()],
        ..DebtPolicy::default()
    }
}

/// A conditioned-rule debt record (home 1 shape — `harness_rule.
/// assumption_debt`; `retirement_experiment` removal test).
fn rule_debt(rule_id: &str, created_at: u64, grade: EvidenceKind) -> AssumptionDebtRecord {
    let prov = ProvenanceRecord::minted(
        Origin::human("test:owner", HumanRole::Author),
        PersistenceScope::Definition,
        created_at,
    );
    AssumptionDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: Text::new(
            format!("{rule_id} conditions a deficiency claim"),
            "test:owner",
            prov.clone(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: grade,
            reference: format!("ev:{rule_id}"),
            observed_at: Some(created_at),
            tier: None,
            provisional: false,
        }],
        owner: OwnerRef {
            team: false,
            id: "test:owner".into(),
            reach_via: vec!["sink:ops".into()],
        },
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::Date,
            value: Some("99999999999".into()),
        },
        removal_test_ref: format!("tmpl:{rule_id}"),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::Hypothesized),
        hypothesis_typed: Some(HypothesisTyped {
            subject: HypothesisSubject::Deficiency,
            deficiency_class: DeficiencyClass::PrematureStop,
            predicted_effect: PredictedEffect::Decrease,
            metric_ref: Some("metric:task_success".into()),
        }),
        scope: Some(DebtScope::default()),
        expiry: Some(DebtExpiry {
            condition: ExpiryKind::Date,
            params: ExpiryParams {
                until: Some(99_999_999_999),
                ..ExpiryParams::default()
            },
        }),
        runway_ms: None,
        revalidation: Some(Revalidation {
            on: vec![RevalidationOn::Schedule],
            action: Some(RevalidationAction::RefreshEvidence),
        }),
        removal_test: Some(RemovalTest {
            template_ref: Some(format!("tmpl:{rule_id}")),
            ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
        }),
        created_by: Some(prov),
        created_at: Some(created_at),
        supersedes: None,
    }
}

fn seed_policy() -> SeedPolicy {
    SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    }
}

fn pre_registration() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "removing the conditioned rule is non-inferior".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: Some(hh_wire::json::Json::str("margin:ni")),
        min_n: 1,
        analysis_plan_ref: "analysis:plan".into(),
        task_split_hash: "sha256:cc33".into(),
        interactions: vec![],
        dead_weight_purpose: false,
    }
}

fn design() -> Design {
    Design {
        id: "design:retirement".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 5,
        pairing: Pairing::ByTask,
        seed_policy: seed_policy(),
        held_out_split_ref: Some("split:held_out".into()),
        pre_registration: pre_registration(),
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn level(id: &str) -> LevelSpec {
    LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    }
}

fn arm(id: &str, lid: &str) -> ArmSpec {
    ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("model".to_string(), lid.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        inference_budget: None,
        match_spec: Some(MatchSpec::matched_cap(&[])),
        artifact_ref: Ref::new("def:x", "sha256:ee55"),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    }
}

/// A 2-arm `[base, removal]` retirement template (the resolved
/// `template_ref` the sweep's `entries[]` carry).
fn template_spec(tag: &str) -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Retirement,
        design: design(),
        pre_registration: Some(pre_registration()),
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("with-rule"), level("without-rule")],
        }],
        arms: vec![
            arm("a", "with-rule"),
            arm(&format!("a-minus-{tag}"), "without-rule"),
        ],
        suite: SuiteBinding {
            suite_ref: "suite:heldout".into(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some("split:1".into()),
        },
        replicates_per_cell: 5,
        seed_policy: seed_policy(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn sched_entry(debt_ref: &str, status: DebtStatus, used: bool) -> SchedulableEntry {
    SchedulableEntry {
        debt_ref: debt_ref.to_string(),
        current_status: status,
        evidence_grade: hh_ontology::debt::EvidenceGrade::Hypothesized,
        used,
        probation_due: false,
        next_time_expiry_ms: None,
        removal_kind: Some(RemovalTestKind::RetirementExperiment),
        home: Some(1),
    }
}

fn default_ctx() -> ScheduleContext<'static> {
    ScheduleContext::default()
}

// ── the resolver-backed context checks (fail-if-removed legs) ────────────

/// `match_spec` absent on a resolved template's arm ⇒ the production
/// resolver reports `has_match_spec: false` ⇒ `missing_match_spec`
/// (never a silent schedule — CC9's matched-budget contract).
#[test]
fn schedule_defers_when_template_lacks_match_spec() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let mut t = template_spec("r1");
    t.arms[1].match_spec = None;
    let out = schedule_removal_test(&entry, &record, Some(&t), &policy(), &default_ctx()).unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::Unexecutable { ref reason }
            } if reason == "missing_match_spec"
        ),
        "expected unexecutable:missing_match_spec, got {out:?}"
    );
}

/// `split_assignment_ref` absent on the resolved template's suite ⇒
/// `missing_split_assignment`.
#[test]
fn schedule_defers_when_split_unassigned() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let mut t = template_spec("r1");
    t.suite.split_assignment_ref = None;
    let out = schedule_removal_test(&entry, &record, Some(&t), &policy(), &default_ctx()).unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::Unexecutable { ref reason }
            } if reason == "missing_split_assignment"
        ),
        "expected unexecutable:missing_split_assignment, got {out:?}"
    );
}

/// An unpinned arm `artifact_ref` (`version_id` not `<algo>:<hex>`) ⇒
/// `unsealed_artifact`.
#[test]
fn schedule_defers_when_artifact_unsealed() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let mut t = template_spec("r1");
    t.arms[0].artifact_ref = Ref::new("def:x", "latest"); // no `algo:digest`
    let out = schedule_removal_test(&entry, &record, Some(&t), &policy(), &default_ctx()).unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::Unexecutable { ref reason }
            } if reason == "unsealed_artifact"
        ),
        "expected unexecutable:unsealed_artifact, got {out:?}"
    );
}

/// `min_runway_ms` binds: `expiry.until − created_at < policy.min_runway_ms`
/// ⇒ `insufficient_runway` — the operative policy is the resolver, not a
/// stub.
#[test]
fn schedule_defers_insufficient_runway_under_policy() {
    let mut record = rule_debt("r1", 1_000_000, EvidenceKind::Source);
    record.expiry = Some(DebtExpiry {
        condition: ExpiryKind::Date,
        params: ExpiryParams {
            until: Some(1_000_001),
            ..ExpiryParams::default()
        },
    });
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let out = schedule_removal_test(
        &entry,
        &record,
        Some(&template_spec("r1")),
        &policy(),
        &default_ctx(),
    )
    .unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::InsufficientRunway { .. }
            }
        ),
        "expected insufficient_runway, got {out:?}"
    );
}

/// `notice_sinks` binds: a record whose `owner.reach_via` names no declared
/// sink ⇒ `owner_unreachable`. The paired leg (the sink declared) is every
/// other green test in this file — and `sched_entry`'s happy path below.
#[test]
fn schedule_defers_owner_unreachable_under_policy() {
    let record = rule_debt("r1", 10, EvidenceKind::Source); // reach_via: sink:ops
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let no_sinks = DebtPolicy::default(); // empty notice_sinks
    let out = schedule_removal_test(
        &entry,
        &record,
        Some(&template_spec("r1")),
        &no_sinks,
        &default_ctx(),
    )
    .unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::OwnerUnreachable { .. }
            }
        ),
        "expected owner_unreachable, got {out:?}"
    );
}

/// `dead_weight_designs_allowed` binds: the resolved template's design
/// declares `dead_weight_purpose` and the operative policy disallows it ⇒
/// `dead_weight_design_disallowed`; under an allowing policy the same
/// record schedules.
#[test]
fn schedule_gates_dead_weight_designs_on_policy() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let mut t = template_spec("r1");
    t.design.pre_registration.dead_weight_purpose = true;

    let forbidding = DebtPolicy {
        dead_weight_designs_allowed: false,
        ..policy()
    };
    let out =
        schedule_removal_test(&entry, &record, Some(&t), &forbidding, &default_ctx()).unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::DeadWeightDisallowed
            }
        ),
        "expected dead_weight_design_disallowed, got {out:?}"
    );

    let allowing = DebtPolicy {
        dead_weight_designs_allowed: true,
        ..policy()
    };
    let out = schedule_removal_test(&entry, &record, Some(&t), &allowing, &default_ctx()).unwrap();
    assert!(
        matches!(out, ScheduleOutcome::Scheduled { .. }),
        "expected Scheduled under the allowing policy, got {out:?}"
    );
}

/// `template_ref` unresolved keeps the landed `template_unresolved`
/// spelling (the resolver reports absent — data, not an error).
#[test]
fn schedule_defers_unresolved_template_spelling() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let out = schedule_removal_test(&entry, &record, None, &policy(), &default_ctx()).unwrap();
    assert!(
        matches!(
            out,
            ScheduleOutcome::Deferred {
                reason: DeferredReason::TemplateUnresolved { .. }
            }
        ),
        "expected template_unresolved, got {out:?}"
    );
}

/// `schedule_batch` runs the same battery — a member whose test cannot
/// instantiate under the operative policy makes the batch inadmissible.
#[test]
fn batch_member_failing_context_check_is_inadmissible() {
    let r1 = rule_debt("r1", 10, EvidenceKind::Source);
    let r2 = rule_debt("r2", 10, EvidenceKind::Source);
    let e1 = sched_entry("debt:r1", DebtStatus::Expired, true);
    let e2 = sched_entry("debt:r2", DebtStatus::Expired, true);
    let t1 = template_spec("r1");
    let mut t2 = template_spec("r2");
    t2.arms[1].match_spec = None; // r2's removal arm is unmatched
    let out = schedule_batch(&[(&e1, &r1, &t1), (&e2, &r2, &t2)], &policy());
    match out {
        Err(e) => assert_eq!(e.code(), "batch_incompatible"),
        Ok(_) => panic!("expected BatchIncompatible"),
    }
}

/// `DebtManagerRecord::validate` consumes the policy spellings at register:
/// an empty `schedule` cadence and an unknown `priority` refuse at
/// admission (AC-R-2.9.6-12 — never deferred to the sweep).
#[test]
fn manager_record_validate_consumes_policy_spellings() {
    let reflexive = reflexive_record(
        "m1",
        "test:owner",
        vec!["sink:ops".into()],
        DeadWeightWindow {
            model_version_changes: 2,
        },
        60_000,
        1,
    )
    .unwrap();
    let mut rec = DebtManagerRecord {
        manager_id: "m1".into(),
        maturity: "instrument-grade".into(),
        policy: policy(),
        reflexive_debt: reflexive.clone(),
    };
    rec.validate().unwrap();

    rec.policy.schedule = String::new();
    assert!(rec.validate().is_err(), "empty cadence must refuse");

    rec.policy = DebtPolicy {
        priority: "bogus".into(),
        ..policy()
    };
    assert!(rec.validate().is_err(), "unknown priority must refuse");
}

/// `policy.schedule` attribution: `sweep`'s durable `sweep.completed` row
/// carries `kind: cadence` + the operative cadence spelling — a runtime
/// consumer, never a carried-but-unread member.
#[test]
fn sweep_completed_row_carries_the_cadence_spelling() {
    let (_root, _clock, mut store, run) = rig("cadence");
    let mut mgr =
        hh_debt::manager::DebtManager::open(&store, &hh_hir::refs::RunRef { run: run.clone() })
            .unwrap();
    let record = manager_record("m1");
    mgr.register(&mut store, &record).unwrap();
    let entry = entry_for(
        &rule_debt("r1", 10, EvidenceKind::Source),
        Some(template_spec("r1")),
    );
    let report = mgr
        .sweep(&mut store, "m1", &[entry], 1_000_000, None)
        .unwrap();
    assert_eq!(report.scheduled.len(), 1);

    let completed = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.debt.sweep.completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1);
    let payload = &completed[0].payload;
    assert_eq!(
        payload.get("kind").and_then(|v| v.as_str()),
        Some("cadence")
    );
    assert_eq!(
        payload.get("cadence").and_then(|v| v.as_str()),
        Some(policy().schedule.as_str())
    );
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
        policy: policy(),
        reflexive_debt: reflexive,
    }
}

fn entry_for(record: &AssumptionDebtRecord, template: Option<ExperimentSpec>) -> SweepEntry {
    SweepEntry {
        debt_ref: format!("debt:{}", record.rule_id),
        home: Some(1),
        version_id: Some("v:1".into()),
        record: record.clone(),
        observables: Default::default(),
        template,
        used_by: vec![],
    }
}
