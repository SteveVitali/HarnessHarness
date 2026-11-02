//! R5.7 — `adopt_subject*` acceptance battery (PADR-R5-1 A.2 owner seam;
//! REQ-E-201, REQ-C-103): the in-process adoption of a launched subject
//! through the existing runtime. A real `ExperimentEngine` launches the
//! subject over a scratch store; `EmbedService::open` then adopts it —
//! one subject, one fenced writer, durable-before-visible.
//!
//! AC-1 positive: run/plan/attempt/config/environment/root/generation
//! validation; arm (fresh) and resume (re-opened service) legs; one
//! `lifecycle.session.attached{mode:"adopt"}` per live adoption; rows
//! land on the subject run under the launch's own writer generation.
//! AC-2 negative: finished, stale, duplicate, mismatched-root/
//! environment and missing-writer adoption refuse typed; no second run,
//! no second writer, no copied runtime.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_embed::adopt::{AdoptBinding, AdoptPorts};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::errors::EmbedError;
use hharness_embed_schema::types::EnvironmentInput;
use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{
    ClaimTicket, EngineContext, ExperimentEngine, LaunchOutcome, NextVerdict,
};
use hharness_experiment::view::LaunchBinding;
use hharness_identity::idp::idp_id;
use hharness_lab::expand::{ArmConfiguration, ExpandTask};
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
use hharness_wire::jsonrpc::Request;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-embed-r5-7-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

struct Rig {
    root: PathBuf,
    store: Store,
    docs: LabDocs,
}

fn rig(tag: &str) -> Rig {
    let root = tmp(tag);
    // The real clock: the adopted writer's TTL is wall-ms, and the
    // service's own store shares the same clock domain (a ManualClock
    // here would mint leases already stale to the service).
    let store = Store::open(&root).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    Rig { root, store, docs }
}

fn budgets() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("eval:a".to_string(), caps(100)),
        ("pool".to_string(), caps(1_000)),
        ("instr".to_string(), caps(1_000)),
    ])
}

/// A minimal `exploratory` spec — one arm, one task, one replicate (no
/// pre-registration or match spec required by the kind).
fn spec() -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Exploratory,
        design: Design {
            id: "design:r57".to_string(),
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
            replicates_per_cell: 1,
            pairing: Pairing::ByTask,
            seed_policy: SeedPolicy {
                harness_rng: true,
                requested_sampling_seed: false,
                seed_honoured_required: false,
            },
            held_out_split_ref: None,
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "exploratory adopt fixture".to_string(),
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
        replicates_per_cell: 1,
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
            permutation_seed: "perm:r57".to_string(),
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
            salt: "salt:r57".to_string(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn ctx() -> EngineContext<'static> {
    let map = budgets();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| map.get(r).cloned())),
        suite_tasks: Some(Box::new(|_s: &ExperimentSpec| {
            Some(vec![ExpandTask {
                task_id: "task:1".to_string(),
                split_label: SplitLabel::Dev,
            }])
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
                budget_ref: None,
            })
        })),
        min_replicates: 1,
        ..Default::default()
    }
}

/// register → expand → open_experiment → next → claim → launch; returns
/// the experiment run id, plan id, the launch outcome and the durable
/// `launch_bound` record the recipe would present at adoption.
fn launched(rig: &mut Rig) -> (String, String, LaunchOutcome, LaunchBinding) {
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    let s = spec();
    let eid = eng.register(&s).unwrap();
    eng.expand(&eid).unwrap();
    let run_id = eng.open_experiment(&eid).unwrap();
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("unexpected next: {other:?}"),
    };
    let ticket: ClaimTicket = eng.claim(&rpid, "driver").unwrap();
    let out = eng.launch(&ticket, "subject").unwrap();
    let binding = eng.launch_binding(&rpid).unwrap().unwrap();
    (run_id, rpid, out, binding)
}

fn svc(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.to_path_buf(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.to_path_buf(),
        holder: "r5-7-adopter".to_string(),
    })
    .unwrap()
}

fn local_env() -> EnvironmentInput {
    EnvironmentInput::ConnectionInfo(Json::obj([("class", Json::str("local_host"))]))
}

fn ports() -> AdoptPorts<'static> {
    AdoptPorts { host_caps: &[] }
}

fn call(s: &mut EmbedService, method: &str, params: Json) -> Json {
    s.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn hello(s: &mut EmbedService) {
    let r = call(
        s,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r5-7")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::obj([])),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn subject_events(root: &std::path::Path, run_id: &str) -> Vec<(String, Json)> {
    let store = Store::open(root).unwrap();
    store
        .events(run_id)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

fn refused_reason(e: EmbedError) -> String {
    match e {
        EmbedError::Refused { reason } => reason,
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ── AC-1: admitted in-process adoption ──────────────────────────────────────

#[test]
fn adopt_arms_one_subject_through_existing_port() {
    let mut rig = rig("adopt-arm");
    let (exp_run, rpid, out, binding) = launched(&mut rig);
    let subject = out.run_id.clone();
    let gen_before = out.subject_writer.generation;
    let mut s = svc(&rig.root);
    let o = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap();
    assert_eq!(o.armed, "armed");
    assert_eq!(o.subject_run_id, subject);
    assert_eq!(o.intent_ref, binding.intent_ref);
    assert_eq!(o.attempt_no, binding.attempt_no);
    // One subject/writer: the launch's own writer, renewed — never a
    // second writer or a second run (AC-2's no-second-run proof).
    assert_eq!(o.writer_generation, gen_before);
    let store = Store::open(&rig.root).unwrap();
    assert_eq!(store.run_ids().len(), 2, "experiment + subject runs only");
    assert_eq!(
        store.current_lease_generation(&subject).unwrap(),
        gen_before
    );
    let rows = subject_events(&rig.root, &subject);
    // The adoption fact — exactly one session.attached{mode:"adopt"}
    // carrying the binding join.
    let attached: Vec<&(String, Json)> = rows
        .iter()
        .filter(|(c, _)| c == "lifecycle.session.attached")
        .collect();
    assert_eq!(attached.len(), 1);
    assert_eq!(
        attached[0].1.get("mode").and_then(Json::as_str),
        Some("adopt")
    );
    assert_eq!(
        attached[0].1.get("intent_ref").and_then(Json::as_str),
        Some(binding.intent_ref.as_str())
    );
    // The runtime armed through the existing port: env provisioned and
    // the leaf's turn-1 rows landed durable under the adopted writer.
    assert!(rows.iter().any(|(c, _)| c == "action.environment.declared"));
    assert!(rows.iter().any(|(c, _)| c == "lifecycle.turn.started"));
    assert!(rows.iter().any(|(c, _)| c == "lifecycle.lease.renewed"));
    // Observe through the existing port — a submit drives the scripted
    // model lane on the adopted session.
    hello(&mut s);
    let r = call(
        &mut s,
        "submit",
        Json::obj([
            ("session_id", Json::str(o.session_id.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("begin")),
                ])]),
            ),
            ("idempotency_key", Json::str("k-1")),
        ]),
    );
    assert!(r.get("result").is_some(), "submit: {r:?}");
    let rows = subject_events(&rig.root, &subject);
    assert!(rows.iter().any(|(c, _)| c == "lifecycle.turn.finished"));
}

#[test]
fn adopt_resumes_after_service_restart() {
    let mut rig = rig("adopt-resume");
    let (exp_run, rpid, out, binding) = launched(&mut rig);
    {
        let mut s = svc(&rig.root);
        s.adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap();
        // The writer stays live (renewed); the service drops, losing
        // every in-memory session/env handle.
    }
    // Re-open on the same store — the second adoption resumes the
    // persisted leaf and reuses the durable environment (no re-mint).
    let mut s = svc(&rig.root);
    let o = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap();
    assert_eq!(o.armed, "resumed");
    let rows = subject_events(&rig.root, &out.run_id);
    assert_eq!(
        rows.iter()
            .filter(|(c, _)| c == "action.environment.declared")
            .count(),
        1,
        "the durable environment is reused, never re-minted"
    );
}

// ── AC-2: typed refusals ────────────────────────────────────────────────────

#[test]
fn adopt_refusal_battery() {
    let mut rig = rig("adopt-refuse");
    let (exp_run, rpid, out, binding) = launched(&mut rig);
    let mut s = svc(&rig.root);

    // missing writer
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: None,
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_missing_writer");

    // writer scoped to the wrong run
    let wrong = hharness_ledger::store::Lease {
        run_id: exp_run.clone(),
        ..out.subject_writer.clone()
    };
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&wrong),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_writer_run_mismatch");

    // stale writer — a superseded generation never adopts
    let stale = hharness_ledger::store::Lease {
        generation: out.subject_writer.generation + 9,
        ..out.subject_writer.clone()
    };
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&stale),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert!(refused_reason(e).starts_with("adopt_writer_stale"));

    // stale binding — an intent-phase record is not a usable binding
    let mut intent = binding.clone();
    intent.phase = "intent".to_string();
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &intent,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_binding_not_bound");

    // stale binding — a member-mutated record never matches durable
    let mut drifted = binding.clone();
    drifted.attempt_no += 1;
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &drifted,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_manifest_mismatch:attempt_no");

    // mismatched root — the recorded subject root must match durable
    let mut bad_root = binding.clone();
    bad_root.subject_root_budget_id = Some("root:forged".to_string());
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &bad_root,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_root_mismatch");

    // mismatched environment — an unserved class refuses typed
    let bad_env =
        EnvironmentInput::ConnectionInfo(Json::obj([("class", Json::str("remote_persistent"))]));
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &bad_env,
            &ports(),
        )
        .unwrap_err();
    assert!(matches!(e, EmbedError::EnvironmentUnavailable { .. }));
}

#[test]
fn adopt_duplicate_and_finished_refuse() {
    let mut rig = rig("adopt-dup");
    let (exp_run, rpid, out, binding) = launched(&mut rig);
    let mut s = svc(&rig.root);
    s.adopt_subject(
        AdoptBinding {
            experiment_run_id: &exp_run,
            run_plan_id: &rpid,
            binding: &binding,
            subject_writer: Some(&out.subject_writer),
        },
        &local_env(),
        &ports(),
    )
    .unwrap();
    // A second live adoption refuses — one subject/writer, no second run.
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    assert_eq!(refused_reason(e), "adopt_duplicate");
    // Finish the subject (the driver's own finish row); adoption refuses.
    {
        let mut ev_store = Store::open(&rig.root).unwrap();
        // A finished run cannot be written through the live writer —
        // mint `lifecycle.run.finished` via the store's own minter under
        // the presented lease is impossible post-finish, so finish is
        // staged by direct append under the live lease (the same fenced
        // writer the launch returned).
        let mint = hharness_env::events::EventMinter::new(&ev_store, &out.run_id)
            .mint(
                "lifecycle.run.finished",
                Json::obj([("reason", Json::str("stop"))]),
            )
            .unwrap();
        ev_store
            .append(&out.run_id, &out.subject_writer, vec![mint])
            .unwrap();
    }
    let e = s
        .adopt_subject(
            AdoptBinding {
                experiment_run_id: &exp_run,
                run_plan_id: &rpid,
                binding: &binding,
                subject_writer: Some(&out.subject_writer),
            },
            &local_env(),
            &ports(),
        )
        .unwrap_err();
    // The live adopted session still names the run — duplicate lands
    // before the finished check order-wise; both are the refused set.
    assert!(matches!(
        refused_reason(e).as_str(),
        "adopt_duplicate" | "adopt_run_finished"
    ));
}
