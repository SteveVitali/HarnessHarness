//! R5.28 — `hh-recipe/1` acceptance battery: the one supported
//! in-process recipe (launch → adopt → execute → grade → project →
//! analyze → receipt) and the typed refusals the contract demands.
//! Positive: a clean scratch-store run composes the whole chain through
//! the boundary ops + owner seams and the `population-receipt/1`
//! closes. Negative: authored/unknown members, missing required
//! members, process-only execution and unspecified grades refuse.
//! Fails-without: `hharness::recipe` does not exist at base a2bed920
//! (compile-red).
//!
//! Scratch stores only — offline scripted-lane execution; USD 0.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness::recipe::{code, run_recipe, RecipeError, RecipePlan, RECIPE_VERSION};
use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_identity::idp::idp_id;
use hharness_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hharness_ledger::store::Store;
use hharness_ontology::config::Ref;
use hharness_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hharness_ontology::lab::SplitLabel;
use hharness_ontology::participant::ParticipantClass;
use hharness_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-recipe-r5-28-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

fn svc(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.to_path_buf(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.to_path_buf(),
        holder: "r5-28-recipe".to_string(),
    })
    .unwrap()
}

/// A minimal `exploratory` spec — one arm, one task, two replicates.
fn spec() -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Exploratory,
        design: Design {
            id: "design:r528".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![FactorDeclaration {
                name: "compaction_strategy".to_string(),
                kind: hharness_ontology::FactorKind::Harness,
                granularity: None,
                levels: vec![FactorLevel {
                    id: "evict_oldest".to_string(),
                    content_ref: pinned("level.evict"),
                    label: "evict oldest".to_string(),
                }],
                role: None,
            }],
            blocking: vec!["task".to_string()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: SeedPolicy {
                harness_rng: true,
                requested_sampling_seed: false,
                seed_honoured_required: false,
            },
            held_out_split_ref: None,
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "exploratory recipe fixture".to_string(),
                primary_metrics: vec!["task_success".to_string()],
                equivalence_margin: None,
                min_n: 1,
                analysis_plan_ref: pinned("analysis"),
                task_split_hash: pinned("split.hash"),
                interactions: vec![],
                dead_weight_purpose: false,
            },
            registry_snapshot_id: Some(pinned("registry.snap")),
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: vec![FactorSpec {
            name: "compaction_strategy".to_string(),
            kind: hharness_ontology::FactorKind::Harness,
            granularity: Some(hharness_ontology::participant::Granularity::ComponentLevel),
            role: None,
            levels: vec![LevelSpec {
                level_id: "evict_oldest".to_string(),
                ref_: pinned("level.evict"),
                overrides: None,
                label: "evict oldest".to_string(),
                class: ParticipantClass::Native,
                non_portable: false,
            }],
        }],
        arms: vec![ArmSpec {
            arm_id: "arm:a".to_string(),
            hypothesis: "arm:a".to_string(),
            level_assignment: BTreeMap::from([(
                "compaction_strategy".to_string(),
                "evict_oldest".to_string(),
            )]),
            eval_budget: "eval:a".to_string(),
            search_budget: None,
            inference_budget: None,
            match_spec: None,
            artifact_ref: Ref::new("artifact:x", pinned("artifact.x")),
            limits_enforced: "full".to_string(),
            model_role_table_ref: None,
            response_cache: None,
            ensemble_k: None,
        }],
        suite: SuiteBinding {
            suite_ref: pinned("suite.tb2"),
            split_labels_used: vec![SplitLabel::Dev],
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
            permutation_seed: "perm:r528".to_string(),
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
        budgets: hharness_lab::experiment::ExperimentBudgets {
            experiment: "pool".to_string(),
            instrument: "instr".to_string(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:r528".to_string(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

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
        ("pool", caps(1_000)),
        ("instr", caps(1_000)),
    ])
}

/// The plan body — `register`/`analysis` carry the boundary's own
/// params shapes verbatim; `spec_ref` is the spec's content-derived id.
fn plan_json(eid: &str) -> Json {
    Json::obj([
        (
            "hello",
            Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str("r5-28")),
                        ("version", Json::str("1")),
                        ("kind", Json::str("test")),
                    ]),
                ),
                (
                    "capabilities",
                    Json::obj([
                        ("experimental", Json::Bool(true)),
                        ("serves_measurement", Json::Bool(true)),
                        ("serves_host_executor", Json::Bool(true)),
                        ("serves_permission_channel", Json::Bool(true)),
                        ("accepts_ephemeral_frames", Json::Bool(true)),
                    ]),
                ),
            ]),
        ),
        (
            "register",
            Json::obj([
                ("spec", spec().to_json()),
                ("budgets", budgets_json()),
                (
                    "suite_tasks",
                    Json::Arr(vec![Json::obj([
                        ("task_id", Json::str("task:1")),
                        ("split_label", Json::str("dev")),
                    ])]),
                ),
                (
                    "arm_configs",
                    Json::obj([(
                        "arm:a",
                        Json::obj([
                            ("configuration_id", Json::str(pinned("cfg.arm:a"))),
                            ("configuration_version_id", Json::str(pinned("cfgv.arm:a"))),
                        ]),
                    )]),
                ),
            ]),
        ),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "subject_input",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("text")),
                ("text", Json::str("begin")),
            ])]),
        ),
        ("grade_metric", Json::str("task_success")),
        (
            "grades",
            Json::obj([(
                "arm:a",
                Json::Arr(vec![Json::Bool(true), Json::Bool(false)]),
            )]),
        ),
        (
            "analysis",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("spec_id", Json::str("")),
                        ("kind", Json::str("summarize")),
                        (
                            "query",
                            Json::obj([("metrics", Json::Arr(vec![Json::str("task_success")]))]),
                        ),
                        ("spec_ref", Json::str(eid)),
                        (
                            "estimator_selection",
                            hharness_ontology::eval::EstimatorSelection {
                                method: hharness_ontology::eval::IntervalMethod::ClusteredClt,
                                selection_rule: "adr-0158.clt_floor".to_string(),
                                floors: BTreeMap::new(),
                                fallback_chain: Vec::new(),
                                substituted: None,
                            }
                            .to_json(),
                        ),
                        ("outputs", Json::Arr(vec![Json::str("report")])),
                    ]),
                ),
                (
                    "tasks",
                    Json::Arr(vec![Json::obj([
                        ("task_id", Json::str("task:1")),
                        ("suite_id", Json::str(pinned("suite.tb2"))),
                        ("split_label", Json::str("dev")),
                        ("split_hash", Json::str(pinned("split.hash"))),
                        ("stratum", Json::str("public_dated")),
                    ])]),
                ),
                (
                    "suites",
                    Json::Arr(vec![Json::obj([
                        ("suite_id", Json::str(pinned("suite.tb2"))),
                        ("family", Json::str("coding_terminal")),
                    ])]),
                ),
                ("seed", Json::Int(11)),
                ("price_table_version", Json::str("prices.r5.28")),
            ]),
        ),
        (
            "weights",
            Json::obj([
                ("family", Json::str("uniform")),
                ("rule", Json::str("w_t = 1/T")),
            ]),
        ),
    ])
}

fn plan(eid: &str) -> RecipePlan {
    RecipePlan::from_json(&plan_json(eid)).unwrap()
}

fn events(root: &std::path::Path, run_id: &str) -> Vec<(String, Json)> {
    Store::open(root)
        .unwrap()
        .events(run_id)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

#[test]
fn full_recipe_composes_and_receipt_closes() {
    let root = tmp("positive");
    let eid = spec().experiment_id();
    let mut s = svc(&root);
    let r = run_recipe(&mut s, &plan(&eid)).unwrap();

    // Negotiation recorded verbatim; the recipe version is stamped.
    assert_eq!(r.recipe_version, RECIPE_VERSION);
    assert!(r.hello.get("kernel").is_some(), "hello: {:?}", r.hello);
    // Every boundary op was called, in order, and recorded.
    let methods: Vec<&str> = r
        .requests
        .iter()
        .filter_map(|q| q.get("method").and_then(Json::as_str))
        .collect();
    for m in [
        "hello",
        "lab.experiment.register",
        "lab.experiment.expand",
        "lab.experiment.open_experiment",
        "lab.experiment.next",
        "lab.experiment.claim",
        "lab.experiment.launch",
        "submit",
        "lab.experiment.settle",
        "lab.experiment.close",
        "lab.analysis.analyze",
    ] {
        assert!(methods.contains(&m), "missing {m}: {methods:?}");
    }
    // Owner seams recorded.
    let seams: Vec<&str> = r
        .owner_calls
        .iter()
        .filter_map(|q| q.get("seam").and_then(Json::as_str))
        .collect();
    assert_eq!(
        seams,
        vec!["adopt", "grade", "adopt", "grade", "project", "project", "receipt",]
    );
    // No callback channels armed — typed inapplicability, never faked.
    assert_eq!(
        r.notifications.get("reason").and_then(Json::as_str),
        Some("n/a{no_callback_channels}")
    );
    // The chain closed: experiment completed, both slots recorded.
    assert_eq!(r.close_status, "completed");
    assert_eq!(r.slots.len(), 2);
    for slot in &r.slots {
        assert!(!slot.submission_ref.is_empty(), "{slot:?}");
        assert!(!slot.grade_ref.is_empty(), "{slot:?}");
        assert!(!slot.row_version_id.is_empty(), "{slot:?}");
        assert_eq!(slot.outcome_class, "scored", "{slot:?}");
    }
    // Analysis persisted — actual refs, not fabricated.
    assert!(r
        .analysis_ref
        .get("record_ref")
        .and_then(Json::as_str)
        .is_some());
    assert!(r
        .analysis_ref
        .get("report_ref")
        .and_then(Json::as_str)
        .is_some());
    // The receipt reconciles closed.
    assert_eq!(r.receipt.verdict, "closed", "{:?}", r.receipt.failures);
    assert_eq!(r.receipt.slots.len(), 2);
    // Divergent declared grades land durable under the subject writer —
    // replicate 0 passed, replicate 1 failed.
    let mut by_rep: Vec<(u64, bool)> = Vec::new();
    for slot in &r.slots {
        let evs = events(&root, &slot.run_id);
        assert!(evs.iter().any(|(c, _)| c == "lifecycle.run.finished"));
        assert!(evs.iter().any(|(c, p)| c == "lifecycle.session.attached"
            && p.get("mode").and_then(Json::as_str) == Some("adopt")));
        let emitted = evs
            .iter()
            .find(|(c, p)| {
                c == "measurement.metric.emitted"
                    && p.get("oracle_ref").and_then(Json::as_str) == Some(RECIPE_VERSION)
            })
            .map(|(_, p)| p.clone())
            .expect("recipe grade row durable");
        by_rep.push((
            slot.attempt_no as u64,
            matches!(
                emitted.get("value").and_then(|v| v.get("value")),
                Some(Json::Bool(true))
            ),
        ));
    }
    by_rep.sort();
    let bools: Vec<bool> = by_rep.iter().map(|(_, b)| *b).collect();
    assert!(bools.contains(&true) && bools.contains(&false), "{bools:?}");
}

#[test]
fn authored_and_unknown_members_refuse() {
    for member in ["rows", "slots", "attempts", "metrics", "success"] {
        let mut j = plan_json("eid:x");
        if let Json::Obj(m) = &mut j {
            m.insert(member.to_string(), Json::Arr(vec![]));
        }
        let e = RecipePlan::from_json(&j).unwrap_err();
        assert!(matches!(
            e,
            RecipeError::Refused {
                code: code::UNSUPPORTED_SHAPE,
                ..
            }
        ));
    }
    let mut j = plan_json("eid:x");
    if let Json::Obj(m) = &mut j {
        m.insert("bogus".to_string(), Json::Null);
    }
    assert!(matches!(
        RecipePlan::from_json(&j).unwrap_err(),
        RecipeError::Refused {
            code: code::UNSUPPORTED_SHAPE,
            ..
        }
    ));
    // `analysis.rows` is an authored record — the recipe fills it.
    let mut j = plan_json("eid:x");
    if let Json::Obj(m) = &mut j {
        if let Some(Json::Obj(a)) = m.get_mut("analysis") {
            a.insert("rows".to_string(), Json::Arr(vec![]));
        }
    }
    assert!(matches!(
        RecipePlan::from_json(&j).unwrap_err(),
        RecipeError::Refused {
            code: code::UNSUPPORTED_SHAPE,
            ..
        }
    ));
}

#[test]
fn missing_required_members_refuse() {
    for member in [
        "hello",
        "register",
        "environment",
        "subject_input",
        "grades",
        "analysis",
        "weights",
    ] {
        let mut j = plan_json("eid:x");
        if let Json::Obj(m) = &mut j {
            m.remove(member);
        }
        let e = RecipePlan::from_json(&j).unwrap_err();
        assert!(
            matches!(
                e,
                RecipeError::Refused {
                    code: code::MISSING_FIELD,
                    ..
                }
            ),
            "{member}: {e}"
        );
    }
}

#[test]
fn process_only_run_never_grades() {
    // `subject_input` the `submit` op refuses at decode: the subject is
    // launched but never executes — the recipe must surface the typed
    // boundary error and land no grade row (a process-only success
    // never grades).
    let root = tmp("process-only");
    let eid = spec().experiment_id();
    let mut j = plan_json(&eid);
    if let Json::Obj(m) = &mut j {
        m.insert("subject_input".to_string(), Json::Null);
    }
    let mut s = svc(&root);
    match run_recipe(&mut s, &RecipePlan::from_json(&j).unwrap()) {
        Err(
            RecipeError::Refused { .. } | RecipeError::Boundary { .. } | RecipeError::Owner { .. },
        ) => {}
        Ok(_) => panic!("a refused drive must not produce a graded recipe"),
    }
    // No `hh-recipe/1` grade row anywhere — the authored-success leg
    // of AC-2 fails closed.
    for run_id in hharness_ledger::store::Store::open(&root)
        .unwrap()
        .run_ids()
    {
        let graded = events(&root, &run_id).iter().any(|(c, p)| {
            c == "measurement.metric.emitted"
                && p.get("oracle_ref").and_then(Json::as_str) == Some(RECIPE_VERSION)
        });
        assert!(!graded, "{run_id} carries a recipe grade without execution");
    }
}

#[test]
fn grade_map_must_cover_the_bound_slot() {
    let root = tmp("grade-unspecified");
    let eid = spec().experiment_id();
    let mut j = plan_json(&eid);
    if let Json::Obj(m) = &mut j {
        m.insert(
            "grades".to_string(),
            Json::obj([("arm:other", Json::Arr(vec![]))]),
        );
    }
    let mut s = svc(&root);
    let err = run_recipe(&mut s, &RecipePlan::from_json(&j).unwrap()).unwrap_err();
    assert!(matches!(
        err,
        RecipeError::Refused {
            code: code::GRADE_UNSPECIFIED,
            ..
        }
    ));
}
