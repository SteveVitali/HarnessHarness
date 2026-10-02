//! `hh-debt` S6.1b coverage — R-2.9.6 (§5h.6; ADR-0197/0198;
//! AC-R-2.9.6-4/-10/-11/-12; DF-S5.4-1): the schedule pipeline
//! (`schedule_removal_test` under `DebtPolicy` — `expiry_urgency` order,
//! the `max_open_removal_tests` bound, the instrument-budget reservation,
//! `Deferred{reason}`), the `retirement_batch` design, the `ManagerView`
//! durable-prefix fold (probation book, open/settled tests, model-version
//! ticks), the reflexive `no_dead_weight_found{window}` verdict, the
//! `propose_retirement`/`retire` gates, and the end-to-end
//! `DebtManager` sweep on a real ledger (restart rebuild equality —
//! the same prefix folds to the same view).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_budget::matchspec::MatchSpec;
use hh_debt::errors::{DebtManagerError, Refusal};
use hh_debt::manager::DebtManager;
use hh_debt::propose::{propose_retirement, ProposalDiff};
use hh_debt::records::{DebtManagerRecord, SchedulableEntry, SweepEntry};
use hh_debt::reflexive::{evaluate_reflexive, reflexive_record};
use hh_debt::schedule::{
    instantiate, priority_class, schedule_batch, schedule_removal_test, DeferredReason,
    ScheduleContext, ScheduleOutcome,
};
use hh_debt::view::ManagerView;
use hh_hir::debt::RetirementRecord;
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_hir::refs::RunRef;
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentRefusal, ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy,
    SchedulingPolicy, SpecContext, SuiteBinding, ValidationStrategy,
};
use hh_ledger::ids::{Clock, ManualClock};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::config::Ref;
use hh_ontology::debt::{
    DeadWeightWindow, DebtClass, DebtExpiry, DebtPolicy, DebtScope, DebtStatus, DeficiencyClass,
    EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, ExpiryParams, HypothesisSubject,
    HypothesisTyped, OwnerRef, PredictedEffect, RemovalTest, RemovalTestKind, Revalidation,
    RevalidationAction, RevalidationOn, Verdict,
};
use hh_ontology::eval::{
    Design, DesignKind, FactorKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixtures ────────────────────────────────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-debt-test-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A tempdir `Store` + a plain agent run (the registry/audit run the
/// manager's `lifecycle.debt.*` book of record rides).
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

fn evolution_prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::evolution("candidate:1", "hyp:1"),
        PersistenceScope::Run,
        1,
    )
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
        equivalence_margin: Some(Json::str("margin:ni")),
        min_n: 1,
        analysis_plan_ref: "analysis:plan".into(),
        task_split_hash: "sha256:cc33".into(),
        interactions: vec![],
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

// ── schedule_removal_test — pure legs ───────────────────────────────────────

/// The `expiry_urgency` order: `expired`+used > `expiring`+used >
/// `hypothesized` past probation > scheduled cadence; a record without a
/// removal test never schedules (class 4).
#[test]
fn priority_classes_order_as_specified() {
    assert_eq!(
        priority_class(&sched_entry("d:0", DebtStatus::Expired, true)),
        0
    );
    assert_eq!(
        priority_class(&sched_entry("d:1", DebtStatus::Expiring, true)),
        1
    );
    let mut e = sched_entry("d:2", DebtStatus::Active, false);
    e.probation_due = true;
    assert_eq!(priority_class(&e), 2);
    assert_eq!(
        priority_class(&sched_entry("d:3", DebtStatus::Active, false)),
        3
    );
    let mut no_test = sched_entry("d:4", DebtStatus::Expired, true);
    no_test.removal_kind = None;
    assert_eq!(priority_class(&no_test), 4);
}

/// A `Scheduled` outcome produces a `kind: retirement` spec charged to
/// `instrument` with the debt's `metric_ref` as the primary metric and
/// the lineage on `ext` (§5h.6 §2; AC-R-2.9.6-11).
#[test]
fn schedule_produces_an_instrument_charged_retirement_spec() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let policy = DebtPolicy::default();
    let ctx = ScheduleContext::default();
    let out =
        schedule_removal_test(&entry, &record, Some(&template_spec("r1")), &policy, &ctx).unwrap();
    let ScheduleOutcome::Scheduled {
        spec,
        priority_class: class,
    } = out
    else {
        panic!("expected Scheduled")
    };
    assert_eq!(class, 0);
    assert_eq!(spec.kind, ExperimentKind::Retirement);
    assert_eq!(
        spec.ext.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    assert_eq!(
        spec.ext.get("debt_ref").and_then(Json::as_str),
        Some("debt:r1")
    );
    assert_eq!(
        spec.pre_registration.as_ref().unwrap().primary_metrics,
        vec!["metric:task_success".to_string()]
    );
    assert!(!spec.experiment_id.is_empty());
    // The instantiated spec still registers as a retirement diff.
    let ctx_retire = SpecContext {
        retirement_diff: Some(true),
        ..SpecContext::member_level()
    };
    spec.register(&ctx_retire).unwrap();
}

/// `max_open_removal_tests` holds — the schedule defers (never drops,
/// never refuses).
#[test]
fn schedule_defers_at_the_open_test_bound() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let policy = DebtPolicy {
        max_open_removal_tests: 2,
        ..DebtPolicy::default()
    };
    let ctx = ScheduleContext {
        open_tests: 2,
        ..ScheduleContext::default()
    };
    let out =
        schedule_removal_test(&entry, &record, Some(&template_spec("r1")), &policy, &ctx).unwrap();
    assert!(matches!(
        out,
        ScheduleOutcome::Deferred {
            reason: DeferredReason::MaxOpenRemovalTests { max: 2, open: 2 }
        }
    ));
}

/// The instrument-budget reservation failure defers — a spec that cannot
/// reserve never schedules (§08's reserve-before-spend, the manager's
/// `Deferred{reason}` row).
#[test]
fn schedule_defers_when_the_instrument_budget_wont_reserve() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let ctx = ScheduleContext {
        open_tests: 0,
        reserve: Some(&|_| false),
        on_cadence: false,
    };
    let out = schedule_removal_test(
        &entry,
        &record,
        Some(&template_spec("r1")),
        &DebtPolicy::default(),
        &ctx,
    )
    .unwrap();
    assert!(matches!(
        out,
        ScheduleOutcome::Deferred {
            reason: DeferredReason::InstrumentBudgetUnreservable { .. }
        }
    ));
}

/// Static kinds never schedule an experiment (`static_kind`) — "static
/// kinds are not comparisons and carry no claim" (AC-R-2.9.6-11); an
/// uninstantiated test defers `not_executable`; an unresolved template
/// defers `template_unresolved`.
#[test]
fn schedule_defers_static_unexecutable_and_unresolved() {
    let policy = DebtPolicy::default();
    let ctx = ScheduleContext::default();
    let mut record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);

    // Static kind — `inspection` never reaches the experiment substrate.
    record.removal_test = Some(RemovalTest {
        criteria: Some("checklist".into()),
        ..RemovalTest::new(RemovalTestKind::Inspection)
    });
    let out = schedule_removal_test(&entry, &record, None, &policy, &ctx).unwrap();
    assert!(matches!(
        out,
        ScheduleOutcome::Deferred {
            reason: DeferredReason::StaticKind { .. }
        }
    ));

    // `retirement_experiment` without `template_ref` — cannot instantiate.
    record.removal_test = Some(RemovalTest::new(RemovalTestKind::RetirementExperiment));
    let out = schedule_removal_test(&entry, &record, None, &policy, &ctx).unwrap();
    assert!(matches!(
        out,
        ScheduleOutcome::Deferred {
            reason: DeferredReason::NotExecutable { .. }
        }
    ));

    // Instantiated but the caller supplied no resolved template.
    record.removal_test = Some(RemovalTest {
        template_ref: Some("tmpl:r1".into()),
        ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
    });
    let out = schedule_removal_test(&entry, &record, None, &policy, &ctx).unwrap();
    assert!(matches!(
        out,
        ScheduleOutcome::Deferred {
            reason: DeferredReason::TemplateUnresolved { .. }
        }
    ));
}

/// An unsupported `DebtPolicy.priority` spelling refuses `Unsupported` —
/// the order vocabulary is closed.
#[test]
fn schedule_refuses_an_unknown_priority_spelling() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entry = sched_entry("debt:r1", DebtStatus::Expired, true);
    let policy = DebtPolicy {
        priority: "most_recent".into(),
        ..DebtPolicy::default()
    };
    let out = schedule_removal_test(
        &entry,
        &record,
        Some(&template_spec("r1")),
        &policy,
        &ScheduleContext::default(),
    );
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(Refusal::Unsupported { .. }))
    ));
}

// ── retirement_batch ────────────────────────────────────────────────────────

/// `schedule_batch` folds k ≥ 2 single-rule removal arms sharing one base
/// arm into one `retirement_batch` spec (§5h.6 §2; ADR-0156 as amended) —
/// `ext.debt_refs[]`/`debt_rule_ids[]` parallel the removal arms;
/// `charged_to = instrument`; it registers under the retirement-diff
/// context.
#[test]
fn batch_folds_shared_base_into_one_spec() {
    let r1 = rule_debt("r1", 10, EvidenceKind::Source);
    let r2 = rule_debt("r2", 10, EvidenceKind::Source);
    let e1 = sched_entry("debt:r1", DebtStatus::Expired, true);
    let e2 = sched_entry("debt:r2", DebtStatus::Expired, true);
    // The caller's templates share the base arm (`a`/`with-rule`) — only
    // the removal arm differs per rule.
    let t1 = template_spec("r1");
    let t2 = template_spec("r2");
    let spec = schedule_batch(&[(&e1, &r1, &t1), (&e2, &r2, &t2)], &DebtPolicy::default()).unwrap();
    assert_eq!(spec.kind, ExperimentKind::RetirementBatch);
    assert_eq!(spec.arms.len(), 3); // base + r1-removal + r2-removal
    assert_eq!(
        spec.ext.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    let rule_ids: Vec<&str> = spec
        .ext
        .get("debt_rule_ids")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.iter().filter_map(Json::as_str).collect()),
            _ => None,
        })
        .unwrap();
    assert_eq!(rule_ids, vec!["r1", "r2"]);
    let ctx_retire = SpecContext {
        retirement_diff: Some(true),
        ..SpecContext::member_level()
    };
    spec.register(&ctx_retire).unwrap();
}

/// Batch incompatibility is a typed refusal: differing base arms refuse
/// `BatchIncompatible`; a singleton batch refuses (batch needs ≥ 2).
#[test]
fn batch_refuses_incompatible_members() {
    let r1 = rule_debt("r1", 10, EvidenceKind::Source);
    let r2 = rule_debt("r2", 10, EvidenceKind::Source);
    let e1 = sched_entry("debt:r1", DebtStatus::Expired, true);
    let e2 = sched_entry("debt:r2", DebtStatus::Expired, true);
    let t1 = template_spec("r1");
    let mut t2 = template_spec("r2");
    t2.arms[0].hypothesis = "a different base".into();
    let out = schedule_batch(&[(&e1, &r1, &t1), (&e2, &r2, &t2)], &DebtPolicy::default());
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(Refusal::BatchIncompatible { .. }))
    ));
    let out = schedule_batch(&[(&e1, &r1, &t1)], &DebtPolicy::default());
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(Refusal::BatchIncompatible { .. }))
    ));
}

// ── ManagerView fold + the reflexive verdict ────────────────────────────────

/// Mint raw `lifecycle.debt.*` rows on `run` (the test's own producer —
/// the view folds by class, any producer's rows fold).
fn mint(store: &mut Store, run: &str, class: &str, payload: Json) {
    store
        .commit_kernel_row_for("hh-debt:test", run, class, payload, vec![], vec![])
        .unwrap();
}

/// The fold rebuilds the probation book, the open/settled test map, the
/// status fold, and the model-version-change ticks — the durable prefix
/// is the state (CC3).
#[test]
fn view_folds_the_durable_prefix() {
    let (_root, _clock, mut store, run) = rig("fold");
    mint(
        &mut store,
        &run,
        "lifecycle.debt.probation.opened",
        Json::obj([
            ("debt_ref", Json::str("debt:r1")),
            ("opened_at_ms", Json::Int(100)),
            ("due_at_ms", Json::Int(200)),
        ]),
    );
    mint(
        &mut store,
        &run,
        "lifecycle.debt.removal_test.scheduled",
        Json::obj([
            ("debt_ref", Json::str("debt:r1")),
            ("kind", Json::str("retirement_experiment")),
            ("experiment_id", Json::str("exp:1")),
        ]),
    );
    mint(
        &mut store,
        &run,
        "lifecycle.debt.status.changed",
        Json::obj([
            ("debt_ref", Json::str("debt:r1")),
            ("from", Json::str("active")),
            ("to", Json::str("expiring")),
            ("trigger", Json::str("expiring")),
            (
                "causes",
                Json::Arr(vec![Json::str("served_model_mismatch")]),
            ),
        ]),
    );
    let view = ManagerView::fold(store.events(&run).unwrap()).unwrap();
    assert!(view.probation.contains_key("debt:r1"));
    assert!(view.is_test_open("debt:r1"));
    assert_eq!(view.open_test_count(), 1);
    assert_eq!(view.status.get("debt:r1"), Some(&DebtStatus::Expiring));
    assert_eq!(view.model_version_ticks.len(), 1);
    // The ledgered probation projection the evaluator consumes.
    let p = view.ledgered_probation("debt:r1").unwrap();
    assert_eq!(p.due_at_ms, 200);
    assert!(p.source_ref.starts_with("evt-"));
}

/// `no_dead_weight_found{window}`: `inconclusive{window_open}` while the
/// window is open; `fail` when a scheduled test passed inside the window
/// (the manager is doing work); `pass` when the window closed and nothing
/// ever passed — the manager is dead weight (ADR-0197 D10).
#[test]
fn reflexive_verdict_reads_the_window() {
    let (_root, _clock, mut store, run) = rig("reflexive");
    // window = 2 model-version changes; mint two ticks (cause spellings
    // from the §05b family).
    for cause in ["served_model_mismatch", "compatibility_token_changed"] {
        mint(
            &mut store,
            &run,
            "lifecycle.debt.status.changed",
            Json::obj([
                ("debt_ref", Json::str("debt:other")),
                ("from", Json::str("active")),
                ("to", Json::str("expiring")),
                ("trigger", Json::str("expiring")),
                ("causes", Json::Arr(vec![Json::str(cause)])),
            ]),
        );
    }
    let view = ManagerView::fold(store.events(&run).unwrap()).unwrap();
    // Window of 3 is still open → inconclusive{window_open}.
    let v = evaluate_reflexive(
        &view,
        "debt:mgr",
        DeadWeightWindow {
            model_version_changes: 3,
        },
        999,
        "sweep:1",
    );
    assert_eq!(v.verdict, Verdict::Inconclusive);
    assert_eq!(v.reason.as_deref(), Some("window_open"));
    // Window of 2 closed with no passes → `pass` (the manager is dead
    // weight — honestly).
    let v = evaluate_reflexive(
        &view,
        "debt:mgr",
        DeadWeightWindow {
            model_version_changes: 2,
        },
        999,
        "sweep:1",
    );
    assert_eq!(v.verdict, Verdict::Pass);

    // A `pass` verdict inside the window → `fail` (the manager is
    // working).
    mint(
        &mut store,
        &run,
        "lifecycle.debt.removal_test.settled",
        Json::obj([
            ("debt_ref", Json::str("debt:r1")),
            ("kind", Json::str("retirement_experiment")),
            ("verdict", Json::str("pass")),
            ("report_ref", Json::str("report:1")),
            ("settled_at", Json::Int(500)),
        ]),
    );
    let view = ManagerView::fold(store.events(&run).unwrap()).unwrap();
    let v = evaluate_reflexive(
        &view,
        "debt:mgr",
        DeadWeightWindow {
            model_version_changes: 2,
        },
        999,
        "sweep:2",
    );
    assert_eq!(v.verdict, Verdict::Fail);
}

// ── propose_retirement / retire gates ───────────────────────────────────────

/// `propose_retirement` — a `pass` verdict + a retirement-shaped diff
/// admit `state: proposed`; no pass ⇒ `RetirementNotEvidenced`; a
/// widening/over-removing diff ⇒ `NotARetirementDiff`.
#[test]
fn proposal_gate_separates_proposal_from_deployment() {
    let verdicts = vec![hh_ontology::debt::RemovalVerdict {
        debt_ref: "debt:r1".into(),
        kind: RemovalTestKind::RetirementExperiment,
        verdict: Verdict::Pass,
        reason: None,
        report_ref: "report:1".into(),
        settled_at: 7,
    }];
    let diff = ProposalDiff {
        removed_rules: ["r1".to_string()].into_iter().collect::<BTreeSet<_>>(),
        widens_authority: false,
        loosens_budget: false,
        diff_ref: "diff:1".into(),
    };
    let p = propose_retirement("debt:r1", "r1", &diff, &verdicts, evolution_prov()).unwrap();
    assert_eq!(p.state, "proposed");
    assert_eq!(p.evidence_report_ref, "report:1");

    // No pass → evidenced-gate refusal.
    let out = propose_retirement("debt:r2", "r2", &diff, &verdicts, evolution_prov());
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(
            Refusal::RetirementNotEvidenced { .. }
        ))
    ));

    // The diff removes more than the rule → not a retirement diff.
    let mut wide = diff.clone();
    wide.removed_rules.insert("r9".into());
    let out = propose_retirement("debt:r1", "r1", &wide, &verdicts, evolution_prov());
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(
            Refusal::NotARetirementDiff { .. }
        ))
    ));

    // Authority widening / budget loosening → refused.
    let mut widen = diff.clone();
    widen.widens_authority = true;
    assert!(matches!(
        propose_retirement("debt:r1", "r1", &widen, &verdicts, evolution_prov()),
        Err(DebtManagerError::Refusal(
            Refusal::NotARetirementDiff { .. }
        ))
    ));
    let mut loose = diff.clone();
    loose.loosens_budget = true;
    assert!(matches!(
        propose_retirement("debt:r1", "r1", &loose, &verdicts, evolution_prov()),
        Err(DebtManagerError::Refusal(
            Refusal::NotARetirementDiff { .. }
        ))
    ));
}

// ── end-to-end DebtManager on a real ledger ─────────────────────────────────

/// The full manager loop on a real ledger (DF-S5.4-1's append→consume
/// seam + AC-R-2.9.6-4/-12): `register` lands the service record; the
/// first `sweep` opens a ledgered `probation.opened` for the
/// `hypothesized` record and schedules its removal test; the second
/// sweep — past `due_at` — fires `probation_overrun` through
/// `evaluate_debt` with `evidence_ref` = the ledgered row (the §5h.7
/// gate resolves the durable record); `settle`+`retire` retire under a
/// human seal; a restart `open` rebuilds the same view.
#[test]
fn manager_sweep_settle_retire_restart() {
    let (_root, clock, mut store, run) = rig("e2e");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    mgr.register(&mut store, &manager_record("m1")).unwrap();
    assert!(mgr.view.managers.contains_key("m1"));

    // A hypothesized debt with a resolvable retirement template.
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let entries = vec![entry_for(&record, Some(template_spec("r1")))];
    clock.advance(1_000_000);
    let report = mgr
        .sweep(&mut store, "m1", &entries, 2_000_000, None)
        .unwrap();
    // Probation opened; the executable test scheduled (class 3 — active,
    // not used).
    assert_eq!(report.probation_opened, vec!["debt:r1".to_string()]);
    assert_eq!(report.scheduled.len(), 1);
    assert_eq!(report.scheduled[0].priority_class, 3);
    assert!(mgr.view.is_test_open("debt:r1"));

    // The second sweep — past `due_at` — fires the ledgered
    // `probation_overrun` leg (DF-S5.4-1: the transition's `evidence_ref`
    // is the ledgered `probation.opened` row's event ref).
    let due = mgr.view.probation["debt:r1"].due_at_ms;
    clock.advance(due + 1 - clock.now_ms());
    let report = mgr
        .sweep(&mut store, "m1", &entries, due + 1, None)
        .unwrap();
    let overruns: Vec<_> = report
        .transitions
        .iter()
        .filter(|t| t.causes.iter().any(|c| c == "probation_overrun"))
        .collect();
    assert_eq!(overruns.len(), 1);
    let evidence = overruns[0].evidence_ref.as_deref().unwrap_or("");
    assert!(
        evidence.starts_with("evt-"),
        "probation_overrun must cite the ledgered row, got {evidence:?}"
    );
    assert_eq!(mgr.view.status["debt:r1"], DebtStatus::Expiring);

    // Idempotent: a third sweep fires nothing new for the debt (the open
    // test is not rescheduled; the probation entry is not reopened).
    let report = mgr
        .sweep(&mut store, "m1", &entries, due + 2, None)
        .unwrap();
    assert!(report.scheduled.is_empty());
    assert!(report.probation_opened.is_empty());

    // `settle` a `pass` verdict — `retirement-eligible` is a verdict, not
    // a status change (D-7).
    let (v, transitions) = mgr
        .settle(
            &mut store,
            "debt:r1",
            RemovalTestKind::RetirementExperiment,
            "report:1",
            Verdict::Pass,
            None,
            due + 3,
        )
        .unwrap();
    assert_eq!(v.verdict, Verdict::Pass);
    assert!(transitions.is_empty());
    assert!(!mgr.view.is_test_open("debt:r1"));

    // `propose` under evolution provenance — admitted as a proposal.
    let diff = ProposalDiff {
        removed_rules: ["r1".to_string()].into_iter().collect(),
        widens_authority: false,
        loosens_budget: false,
        diff_ref: "diff:1".into(),
    };
    let p = mgr
        .propose("debt:r1", "r1", &diff, evolution_prov())
        .unwrap();
    assert_eq!(p.state, "proposed");

    // `retire` under a non-human seal refuses (the gate is human-sealed
    // by construction — AC-R-2.9.6-4).
    let evo_record = RetirementRecord {
        removal_test_report_ref: "report:1".into(),
        verdict_ref: "verdict:1".into(),
        decided_by: evolution_prov(),
        rationale: Text::new("the test passed", "test:owner", evolution_prov()),
    };
    assert!(matches!(
        mgr.retire(&mut store, "debt:r1", &evo_record),
        Err(DebtManagerError::Refusal(
            Refusal::NotARetirementDiff { .. }
        ))
    ));

    // Human seal — the `→ retired` transition lands with
    // `supersedes{reason: expiry}`.
    let seal = retirement_record_for("report:1", "verdict:1");
    let outcome = mgr.retire(&mut store, "debt:r1", &seal).unwrap();
    assert_eq!(outcome.supersedes_reason, "expiry");
    assert_eq!(outcome.transition.to, DebtStatus::Retired);
    assert_eq!(mgr.view.status["debt:r1"], DebtStatus::Retired);

    // A second retire refuses — retired is terminal.
    assert!(mgr.retire(&mut store, "debt:r1", &seal).is_err());

    // Restart rebuild equality: `open` folds the same prefix to the same
    // view (CC3/CC10).
    let reopened = DebtManager::open(&store, &run_ref).unwrap();
    assert_eq!(reopened.view.sweep_count, mgr.view.sweep_count);
    assert_eq!(reopened.view.status, mgr.view.status);
    assert_eq!(reopened.view.probation, mgr.view.probation);
    assert_eq!(reopened.view.settled.len(), mgr.view.settled.len());
}

/// The retirement gate refuses without a `pass` verdict — even under a
/// human seal (`RetirementNotEvidenced`; AC-R-2.9.6-4).
#[test]
fn retire_without_a_pass_verdict_refuses() {
    let (_root, _clock, mut store, run) = rig("gate");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    mgr.register(&mut store, &manager_record("m1")).unwrap();
    let seal = retirement_record_for("report:0", "verdict:0");
    let out = mgr.retire(&mut store, "debt:never-tested", &seal);
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(
            Refusal::RetirementNotEvidenced { .. }
        ))
    ));
}

/// The sweep's max-open bound: two executable debts, `max_open = 1` —
/// the higher-priority (`expired`+used) schedules; the other defers with
/// `max_open_removal_tests` (the bound is data on the report).
#[test]
fn sweep_respects_max_open_removal_tests() {
    let (_root, _clock, mut store, run) = rig("bound");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    let mut rec = manager_record("m1");
    rec.policy.max_open_removal_tests = 1;
    mgr.register(&mut store, &rec).unwrap();

    // Two debts — `r_used` (used by a live definition) schedules ahead of
    // `r_idle` (cadence class).
    let mut used = rule_debt("r_used", 10, EvidenceKind::Source);
    used.status = DebtStatus::Expiring;
    let mut e_used = entry_for(&used, Some(template_spec("r_used")));
    e_used.used_by = vec!["def:live".into()];
    let idle = rule_debt("r_idle", 10, EvidenceKind::Source);
    let e_idle = entry_for(&idle, Some(template_spec("r_idle")));
    let report = mgr
        .sweep(&mut store, "m1", &[e_used, e_idle], 5_000, None)
        .unwrap();
    assert_eq!(report.scheduled.len(), 1);
    assert_eq!(report.scheduled[0].debt_ref, "debt:r_used");
    assert_eq!(
        report.deferred,
        vec![(
            "debt:r_idle".to_string(),
            "max_open_removal_tests".to_string()
        )]
    );
}

/// An unknown manager id refuses `UnknownManager` — the service record
/// is the authority for the policy/reflexive record the sweep runs.
#[test]
fn sweep_on_an_unregistered_manager_refuses() {
    let (_root, _clock, mut store, run) = rig("unknown");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let out = mgr.sweep(&mut store, "ghost", &[entry_for(&record, None)], 1, None);
    assert!(matches!(
        out,
        Err(DebtManagerError::Refusal(Refusal::UnknownManager { .. }))
    ));
}

/// The reflexive sweep: once the window closes with no passing removal
/// test the manager's own debt records a `pass` verdict in the ledger —
/// its honest end state waits on the human seal like every other debt.
#[test]
fn sweep_mints_the_reflexive_verdict_when_the_window_closes() {
    let (_root, _clock, mut store, run) = rig("reflexive-sweep");
    let run_ref = RunRef { run: run.clone() };
    let mut mgr = DebtManager::open(&store, &run_ref).unwrap();
    // window = 2 model-version changes.
    mgr.register(&mut store, &manager_record("m1")).unwrap();
    // Two model-version-change ticks land on the book.
    for cause in ["served_model_mismatch", "fingerprint_drift"] {
        mint(
            &mut store,
            &run,
            "lifecycle.debt.status.changed",
            Json::obj([
                ("debt_ref", Json::str("debt:other")),
                ("from", Json::str("active")),
                ("to", Json::str("expiring")),
                ("trigger", Json::str("expiring")),
                ("causes", Json::Arr(vec![Json::str(cause)])),
            ]),
        );
    }
    // Re-fold (the ticks landed outside the live emit path).
    mgr.view = ManagerView::fold(store.events(&run).unwrap()).unwrap();
    let report = mgr.sweep(&mut store, "m1", &[], 9_999, None).unwrap();
    let v = report.reflexive_verdict.unwrap();
    assert_eq!(v.verdict, Verdict::Pass);
    // The settled verdict is ledgered (the human seal still gates the
    // `retired` transition itself).
    let reflexive = mgr.manager("m1").unwrap().reflexive_debt_ref();
    assert!(mgr
        .view
        .settled
        .iter()
        .any(|(_, s)| s.debt_ref == reflexive && s.verdict == Verdict::Pass));
    assert_ne!(mgr.view.status.get(&reflexive), Some(&DebtStatus::Retired));
}

/// `DebtManagerRecord` validation: a non-home-16 reflexive kind and an
/// unknown maturity spelling refuse (AC-R-2.9.6-12 — the service record
/// carries the maturity flag and the reflexive debt).
#[test]
fn service_record_validates_maturity_and_the_reflexive_kind() {
    let mut rec = manager_record("m1");
    rec.maturity = "gold-standard".into();
    assert!(rec.validate().is_err());
    let mut rec = manager_record("m1");
    rec.reflexive_debt.removal_test = Some(RemovalTest {
        criteria: Some("checklist".into()),
        ..RemovalTest::new(RemovalTestKind::Inspection)
    });
    assert!(rec.validate().is_err());
    // And the codec round-trips the service record.
    let rec = manager_record("m1");
    let j = rec.to_json();
    let back = DebtManagerRecord::from_json(&j).unwrap();
    assert_eq!(back.manager_id, "m1");
    assert_eq!(back.maturity, "instrument-grade");
}

/// The instantiated spec is a *proposal half* — the retirement gate's
/// `register` still refuses a non-removal-diff view
/// (`NotARetirementDiff`, the shape check the manager honors).
#[test]
fn instantiated_spec_still_requires_the_retirement_shape() {
    let record = rule_debt("r1", 10, EvidenceKind::Source);
    let spec = instantiate("debt:r1", &record, template_spec("r1"));
    let not_diff = SpecContext {
        retirement_diff: Some(false),
        ..SpecContext::member_level()
    };
    assert!(matches!(
        spec.register(&not_diff),
        Err(ExperimentRefusal::NotARetirementDiff { .. })
    ));
}

fn retirement_record_for(report: &str, verdict: &str) -> RetirementRecord {
    RetirementRecord {
        removal_test_report_ref: report.to_string(),
        verdict_ref: verdict.to_string(),
        decided_by: human("test:sealer"),
        rationale: Text::new("the removal test passed", "test:owner", human("test:owner")),
    }
}
