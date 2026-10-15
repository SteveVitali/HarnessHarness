//! R2.17 boundary legs — DF-S1.24-1 at `lab.debt.sweep`: when the caller
//! does not project `entries[].template`, the boundary resolves the
//! record's `removal_test.template_ref` through `LabDocs` — the
//! production store `lab.experiment.register` deposits into — never a
//! fixture stub, never a fabricated spec. A ref the store does not carry
//! still answers `template_unresolved` (honest deferral); a caller-
//! supplied `template` stays authoritative (records-in wins). The whole
//! battery rides `tier-c4` (the manager is a removable tier — CC6).

#![cfg(feature = "tier-c4")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::matchspec::MatchSpec;
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey};
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};
use hh_ontology::config::Ref;
use hh_ontology::debt::{DeadWeightWindow, DebtPolicy};
use hh_ontology::eval::{Design, DesignKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_ontology::FactorKind;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r2-17-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> (PathBuf, EmbedService) {
    let root = test_dir("svc");
    let svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap();
    (root, svc)
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(r: &Json) -> &Json {
    r.get("result").unwrap_or_else(|| panic!("{r:?}"))
}

fn hello(svc: &mut EmbedService) {
    let mut m = BTreeMap::new();
    m.insert("experimental".to_string(), Json::Bool(true));
    m.insert("serves_measurement".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(m)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn pinned(tag: &str) -> String {
    hh_identity::idp::idp_id(&format!("r2-17.{tag}"), tag.as_bytes())
}

// ── fixtures (same shapes as `s6_1b.rs` — kept local so the battery
// stands alone) ──────────────────────────────────────────────────────────

/// A manager record whose operative policy declares `sink:ops` — the
/// fixture records' `owner.reach_via` names it, so the schedule point's
/// `OwnerUnreachable` check (the `notice_sinks` consumer) passes.
fn manager_record_json(manager_id: &str) -> Json {
    let reflexive = hh_debt::reflexive::reflexive_record(
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
    hh_debt::records::DebtManagerRecord {
        manager_id: manager_id.to_string(),
        maturity: "instrument-grade".into(),
        policy: DebtPolicy {
            notice_sinks: vec!["sink:ops".into()],
            ..DebtPolicy::default()
        },
        reflexive_debt: reflexive,
    }
    .to_json()
}

/// A conditioned-rule debt record (home 1) whose `removal_test.template_ref`
/// the sweep resolves through `LabDocs`.
fn rule_debt_json(rule_id: &str, template_ref: &str) -> Json {
    let prov = hh_provenance::ProvenanceRecord::minted(
        hh_provenance::Origin::human("test:owner", hh_provenance::HumanRole::Author),
        hh_provenance::PersistenceScope::Definition,
        1,
    );
    let record = hh_hir::records::AssumptionDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: hh_hir::leaves::Text::new(
            format!("{rule_id} conditions a deficiency claim"),
            "test:owner",
            prov.clone(),
        ),
        evidence_refs: vec![hh_ontology::debt::EvidenceRef::legacy(format!(
            "ev:{rule_id}"
        ))],
        owner: hh_ontology::debt::OwnerRef {
            team: false,
            id: "test:owner".into(),
            reach_via: vec!["sink:ops".into()],
        },
        expiry_condition: hh_ontology::debt::ExpiryCondition {
            kind: hh_ontology::debt::ExpiryKind::Date,
            value: Some("99999999999".into()),
        },
        removal_test_ref: template_ref.to_string(),
        status: hh_ontology::debt::DebtStatus::Active,
        debt_class: Some(hh_ontology::debt::DebtClass::Hypothesized),
        hypothesis_typed: None,
        scope: Some(hh_ontology::debt::DebtScope::default()),
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: Some(hh_ontology::debt::RemovalTest {
            template_ref: Some(template_ref.to_string()),
            ..hh_ontology::debt::RemovalTest::new(
                hh_ontology::debt::RemovalTestKind::RetirementExperiment,
            )
        }),
        created_by: Some(prov),
        created_at: Some(1),
        supersedes: None,
    };
    hh_hir::debt_json(&record, false)
}

/// The registered template spec — a 2-arm `[base, removal]` spec with a
/// matched-budget `match_spec` per arm, a suite carrying
/// `split_assignment_ref`, and pinned `artifact_ref.version_id`s — every
/// member the production `validate_removal_test` context legs read.
fn template_spec() -> ExperimentSpec {
    let arm = |id: &str, level: &str, tag: &str| ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("participant".to_string(), level.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "eval:a".into(),
        search_budget: Some("search:a".into()),
        inference_budget: None,
        match_spec: Some(MatchSpec::matched_cap(&[DimensionId::ModelCalls])),
        artifact_ref: Ref::new("artifact:x", pinned(&format!("artifact.{tag}"))),
        limits_enforced: "full".into(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.into(),
        ref_: pinned(&format!("level.{id}")),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let prereg = PreRegistration {
        registered_at: 1,
        hypothesis: "removing the conditioned rule is non-inferior".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: Some(Json::str("margin:ni")),
        min_n: 1,
        analysis_plan_ref: pinned("analysis.plan"),
        task_split_hash: pinned("split.hash"),
        interactions: vec![],
        dead_weight_purpose: false,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "design:r2-17".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: SeedPolicy {
                harness_rng: true,
                requested_sampling_seed: false,
                seed_honoured_required: false,
            },
            held_out_split_ref: Some(pinned("split.held_out")),
            pre_registration: prereg.clone(),
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![FactorSpec {
            name: "participant".into(),
            kind: FactorKind::Harness,
            granularity: None,
            role: None,
            levels: vec![level("base"), level("minus-rule")],
        }],
        arms: vec![arm("a", "base", "a"), arm("a-minus-r", "minus-rule", "b")],
        suite: SuiteBinding {
            suite_ref: pinned("suite.r2-17"),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pinned("split.assign")),
        },
        replicates_per_cell: 2,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: false,
            seed_honoured_required: false,
        },
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: "perm:r2-17".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 1_000_000,
            backoff: Backoff {
                min_ms: 0,
                multiplier_ppm: 1_000_000,
                max_ms: 0,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:r2-17".into(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

/// The resolver budget bodies `lab.experiment.register{budgets{}}`
/// deposits (hard caps — the register gate resolves the spec's refs).
fn budgets_json() -> Json {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
        .to_json()
    };
    Json::obj([
        ("eval:a", caps(100)),
        ("search:a", caps(50)),
        ("budget:exp", caps(1_000)),
        ("budget:inst", caps(1_000)),
    ])
}

fn open_manager_run(svc: &mut EmbedService) -> String {
    let r = call(svc, "lab.debt.manager_open", Json::obj([]));
    ok(&r)
        .get("run_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

fn register_manager(svc: &mut EmbedService, run: &str, manager_id: &str) {
    let r = call(
        svc,
        "lab.debt.register",
        Json::obj([
            ("run_id", Json::str(run)),
            ("record", manager_record_json(manager_id)),
        ]),
    );
    assert!(r.get("result").is_some(), "register: {r:?}");
}

fn sweep(svc: &mut EmbedService, run: &str, entries: Vec<Json>) -> Json {
    let r = call(
        svc,
        "lab.debt.sweep",
        Json::obj([
            ("run_id", Json::str(run)),
            ("manager_id", Json::str("m1")),
            ("entries", Json::Arr(entries)),
            ("now_ms", Json::Int(1_000)),
        ]),
    );
    ok(&r).clone()
}

fn scheduled(report: &Json) -> &Vec<Json> {
    match report.get("scheduled") {
        Some(Json::Arr(a)) => a,
        _ => panic!("no scheduled member: {report:?}"),
    }
}

fn deferred_reason(report: &Json, debt_ref: &str) -> Option<String> {
    report
        .get("deferred")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap()
        .iter()
        .find(|d| d.get("debt_ref").and_then(Json::as_str) == Some(debt_ref))
        .and_then(|d| d.get("reason").and_then(Json::as_str))
        .map(str::to_string)
}

/// The production-resolver leg: register the template spec through
/// `lab.experiment.register` (LabDocs deposit), then sweep an entry that
/// names the returned `experiment_id` as `template_ref` and carries no
/// `template` member — the schedule leg resolves the store spec, the
/// `validate_removal_test` context checks pass under the operative
/// policy, and the instantiated retirement spec schedules.
#[test]
fn sweep_resolves_template_ref_through_labdocs() {
    let (_root, mut svc) = service();
    hello(&mut svc);
    let spec = template_spec();
    let r = call(
        &mut svc,
        "lab.experiment.register",
        Json::obj([("spec", spec.to_json()), ("budgets", budgets_json())]),
    );
    let eid = ok(&r)
        .get("experiment_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run = open_manager_run(&mut svc);
    register_manager(&mut svc, &run, "m1");
    let entry = Json::obj([
        ("debt_ref", Json::str("debt:r1")),
        ("home", Json::Int(1)),
        ("record", rule_debt_json("r1", &eid)),
        // `used_by` — the expired+used ordering input.
        ("used_by", Json::Arr(vec![Json::str("run:consumer")])),
    ]);
    let report = sweep(&mut svc, &run, vec![entry]);
    let sched = scheduled(&report);
    assert_eq!(sched.len(), 1, "expected one scheduled test: {report:?}");
    let row = &sched[0];
    assert_eq!(row.get("debt_ref").and_then(Json::as_str), Some("debt:r1"));
    let scheduled_spec = row.get("spec").unwrap_or_else(|| panic!("{row:?}"));
    // `instantiate` stamps the retirement lineage — the scheduled spec is
    // the resolved template re-keyed, never a fresh fabrication.
    assert_eq!(
        scheduled_spec.get("kind").and_then(Json::as_str),
        Some("retirement")
    );
    let ext = scheduled_spec.get("ext").expect("scheduled spec ext");
    assert_eq!(ext.get("debt_ref").and_then(Json::as_str), Some("debt:r1"));
    assert_eq!(
        ext.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    assert_eq!(deferred_reason(&report, "debt:r1"), None);
}

/// A `template_ref` the store does not carry still answers
/// `template_unresolved` — the resolver is honest about a miss, never
/// fabricates a spec.
#[test]
fn sweep_template_ref_unknown_stays_deferred() {
    let (_root, mut svc) = service();
    hello(&mut svc);
    let run = open_manager_run(&mut svc);
    register_manager(&mut svc, &run, "m1");
    let entry = Json::obj([
        ("debt_ref", Json::str("debt:r2")),
        ("home", Json::Int(1)),
        ("record", rule_debt_json("r2", "tmpl:not-registered")),
    ]);
    let report = sweep(&mut svc, &run, vec![entry]);
    assert!(scheduled(&report).is_empty(), "{report:?}");
    assert_eq!(
        deferred_reason(&report, "debt:r2").as_deref(),
        Some("template_unresolved"),
        "{report:?}"
    );
}

/// Records-in stays authoritative: a caller-projected `template` wins
/// even when `template_ref` names a spec the store does not carry.
#[test]
fn sweep_caller_projected_template_stays_authoritative() {
    let (_root, mut svc) = service();
    hello(&mut svc);
    let run = open_manager_run(&mut svc);
    register_manager(&mut svc, &run, "m1");
    let entry = Json::obj([
        ("debt_ref", Json::str("debt:r3")),
        ("home", Json::Int(1)),
        ("record", rule_debt_json("r3", "tmpl:not-registered")),
        // The caller resolves the template itself — records-in.
        ("template", template_spec().to_json()),
        ("used_by", Json::Arr(vec![Json::str("run:consumer")])),
    ]);
    let report = sweep(&mut svc, &run, vec![entry]);
    assert_eq!(
        scheduled(&report).len(),
        1,
        "caller-projected template must schedule: {report:?}"
    );
}
