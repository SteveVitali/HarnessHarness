//! `hh-experiment` engine tests — the S3.4a lifecycle over a real `Store` +
//! `LabDocs` (spec §6.3; AC-R-2.10.3-{1,2,4,5,6,7,8} + KP-E1…E5 + E-1…E-4).
//! Every test uses unique `HH_*` temp roots; the store clock is a shared
//! `ManualClock` handle so claim-expiry/backoff tests advance wall time
//! deterministically.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey, MatchSpec};
use hh_experiment::docs::LabDocs;
use hh_experiment::engine::{
    ClaimTicket, EngineContext, ExperimentEngine, LaunchOutcome, NextVerdict,
};
use hh_experiment::errors::{refusal_code, ExperimentError};
use hh_experiment::events::PauseReason;
use hh_experiment::view::ExperimentView;
use hh_identity::idp::idp_id;
use hh_lab::expand::{ArmConfiguration, ExpandTask};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ManualClock;
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::config::Ref;
use hh_ontology::control::{CancelledBy, InfraError, InfraErrorFamily, OutcomeClass, StopReason};
use hh_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_ontology::FactorKind;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

// ── fixture plumbing ────────────────────────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-experiment-test-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A pinned `idp/1` id — the manifest/binding fields demand pinned refs.
fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

struct Rig {
    root: PathBuf,
    clock: ManualClock,
    store: Store,
    docs: LabDocs,
}

fn rig(tag: &str, now_ms: u64) -> Rig {
    let root = tmp(tag);
    let clock = ManualClock::at(now_ms);
    let store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    Rig {
        root,
        clock,
        store,
        docs,
    }
}

/// The standard budget bodies the resolver serves (per-run cap = 100
/// `model_calls`; the experiment pool = 1 000).
fn budgets() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("eval:a".to_string(), caps(100)),
        ("eval:b".to_string(), caps(100)),
        ("search:a".to_string(), caps(50)),
        ("search:b".to_string(), caps(50)),
        ("pool".to_string(), caps(1_000)),
        ("instr".to_string(), caps(1_000)),
    ])
}

fn arm(arm_id: &str, level: &str, eval: &str, search: Option<&str>) -> ArmSpec {
    ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis: format!("{arm_id} does better"),
        level_assignment: BTreeMap::from([("compaction_strategy".to_string(), level.to_string())]),
        eval_budget: eval.to_string(),
        search_budget: search.map(str::to_string),
        match_spec: Some(MatchSpec::matched_cap(&[DimensionId::ModelCalls])),
        artifact_ref: Ref::new("artifact:x", pinned("artifact.x")),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
    }
}

fn pre_registration() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "evict_oldest ≥ clear_tool_results".to_string(),
        primary_metrics: vec!["task_success".to_string()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
        interactions: vec![],
    }
}

fn design() -> Design {
    Design {
        id: "design:test".to_string(),
        kind: DesignKind::Paired,
        factors: vec![FactorDeclaration {
            name: "compaction_strategy".to_string(),
            kind: FactorKind::Harness,
            granularity: None,
            levels: vec![
                FactorLevel {
                    id: "evict_oldest".to_string(),
                    content_ref: pinned("level.evict"),
                    label: "evict oldest".to_string(),
                },
                FactorLevel {
                    id: "clear_tool_results".to_string(),
                    content_ref: pinned("level.clear"),
                    label: "clear tool results".to_string(),
                },
            ],
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
        pre_registration: pre_registration(),
        registry_snapshot_id: Some(pinned("registry.snap")),
        generators: None,
        resolution: None,
    }
}

/// A valid `comparative` paired spec (2 arms × 1 task × 2 replicates = 4 plans).
fn spec(kind: ExperimentKind) -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind,
        design: design(),
        pre_registration: Some(pre_registration()),
        factors: vec![FactorSpec {
            name: "compaction_strategy".to_string(),
            kind: FactorKind::Harness,
            granularity: Some(hh_ontology::participant::Granularity::ComponentLevel),
            role: None,
            levels: vec![
                LevelSpec {
                    level_id: "evict_oldest".to_string(),
                    ref_: pinned("level.evict"),
                    overrides: None,
                    label: "evict oldest".to_string(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
                LevelSpec {
                    level_id: "clear_tool_results".to_string(),
                    ref_: pinned("level.clear"),
                    overrides: None,
                    label: "clear tool results".to_string(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
            ],
        }],
        arms: vec![
            arm("arm:a", "evict_oldest", "eval:a", Some("search:a")),
            arm("arm:b", "clear_tool_results", "eval:b", Some("search:b")),
        ],
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
            permutation_seed: "perm:test".to_string(),
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
        budgets: hh_lab::experiment::ExperimentBudgets {
            experiment: "pool".to_string(),
            instrument: "instr".to_string(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:test".to_string(),
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
        suite_tasks: Some(Box::new(|_spec: &ExperimentSpec| {
            Some(vec![ExpandTask {
                task_id: "task:1".to_string(),
                split_label: SplitLabel::Dev,
            }])
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            })
        })),
        min_replicates: 1,
        ..Default::default()
    }
}

/// `register` under a fresh engine — returns the refusal/error code.
fn register_err(r: &mut Rig, s: &ExperimentSpec, c: EngineContext<'static>) -> String {
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), c);
    eng.register(s).map(|_| ()).unwrap_err().code().to_string()
}

/// A registered+expanded+opened experiment — returns (experiment_id, run_id).
fn open(rig: &mut Rig, spec: &ExperimentSpec) -> (String, String) {
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    let eid = eng.register(spec).unwrap();
    eng.expand(&eid).unwrap();
    let run_id = eng.open_experiment(&eid).unwrap();
    (eid, run_id)
}

fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-experiment-test/1"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "hh-experiment-test/1",
            store.now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

/// Drive a subject run to terminal — `control.budget.consumed` rows (the
/// run's own accounting) then `lifecycle.run.finished{stop_reason}`.
fn finish_subject(
    store: &mut Store,
    run_id: &str,
    writer: &Lease,
    stop: StopReason,
    consumed: &[(DimensionId, i64)],
) {
    let mut batch = Vec::new();
    for (d, a) in consumed {
        batch.push(mint(
            store,
            run_id,
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str(d.as_str())),
                ("amount", Json::Int(*a)),
            ]),
        ));
    }
    batch.push(mint(
        store,
        run_id,
        "lifecycle.run.finished",
        Json::obj([
            ("stop_reason", stop.to_json()),
            ("outcome_class", Json::str(stop.outcome_class().as_str())),
        ]),
    ));
    // Chain the parents (the append's batch-local known_ids rule).
    let mut parent = store.head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, writer, batch).unwrap();
}

/// Run the whole lifecycle to close: next → claim → launch → drive → settle.
fn run_to_close(
    rig: &mut Rig,
    eid: &str,
    consumed: i64,
) -> hh_experiment::events::ExperimentReport {
    loop {
        let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
        eng.attach(eid).unwrap();
        match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => {
                let ticket = eng.claim(&run_plan_id, "driver").unwrap();
                let launched = eng.launch(&ticket, "subject").unwrap();
                drop(eng);
                finish_subject(
                    &mut rig.store,
                    &launched.run_id,
                    &launched.subject_writer,
                    StopReason::Completed,
                    &[(DimensionId::ModelCalls, consumed)],
                );
                let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
                eng.attach(eid).unwrap();
                let out = eng.settle(&run_plan_id).unwrap();
                assert!(out.accepted);
            }
            NextVerdict::Done => break,
            other => panic!("unexpected next: {other:?}"),
        }
    }
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    eng.attach(eid).unwrap();
    eng.close(false).unwrap()
}

// ── E-1: the register refusal set ───────────────────────────────────────────

#[test]
fn register_admits_and_is_idempotent() {
    let mut r = rig("register-ok", 0);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    let s = spec(ExperimentKind::Comparative);
    let a = eng.register(&s).unwrap();
    let b = eng.register(&s).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, s.experiment_id);
}

#[test]
fn register_refusal_corpus() {
    let mut r = rig("refusals", 0);
    let base = spec(ExperimentKind::Comparative);

    // MissingMatchSpec — an arm without a MatchSpec is refused, never warned.
    let mut s = base.clone();
    s.arms[1].match_spec = None;
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "MissingMatchSpec");

    // UnbudgetedArm — empty eval budget.
    let mut s = base.clone();
    s.arms[0].eval_budget = String::new();
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "UnbudgetedArm");

    // UnbudgetedArm — matched kind without search budget.
    let mut s = base.clone();
    s.arms[0].search_budget = None;
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "UnbudgetedArm");

    // MissingPreRegistration.
    let mut s = base.clone();
    s.pre_registration = None;
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "MissingPreRegistration");

    // InsufficientReplicates — the context floor is 5, the spec declares 2.
    assert_eq!(
        register_err(
            &mut r,
            &base,
            EngineContext {
                min_replicates: 5,
                ..ctx()
            },
        ),
        "InsufficientReplicates"
    );

    // SplitUnassigned — matched kind without the split assignment.
    let mut s = base.clone();
    s.suite.split_assignment_ref = None;
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "SplitUnassigned");

    // UnsealedArtifact — an unpinned artifact ref.
    let mut s = base.clone();
    s.arms[0].artifact_ref = Ref::new("artifact:x", "mutable:latest");
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "UnsealedArtifact");

    // UnsealedArtifact — pinned but the context's sealed view says no.
    assert_eq!(
        register_err(
            &mut r,
            &base,
            EngineContext {
                artifact_sealed: Some(Box::new(|_| false)),
                ..ctx()
            },
        ),
        "UnsealedArtifact"
    );

    // InadmissibleFactor — an arm assigns an undeclared level.
    let mut s = base.clone();
    s.arms[0]
        .level_assignment
        .insert("compaction_strategy".to_string(), "nope".to_string());
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "InadmissibleFactor");

    // ResolutionInsufficient — generators/resolution on a non-fractional kind.
    let mut s = base.clone();
    s.design.generators = Some(vec!["a=b".to_string()]);
    s.design.resolution = Some(hh_ontology::eval::FractionalResolution::III);
    s.experiment_id = s.experiment_id();
    assert_eq!(register_err(&mut r, &s, ctx()), "ResolutionInsufficient");

    // DependsOnDriftedCapability — the context's drift view trips.
    let drift = pinned("level.evict");
    assert_eq!(
        register_err(
            &mut r,
            &base,
            EngineContext {
                capability_drifted: Some(Box::new(move |r: &str| r == drift)),
                ..ctx()
            },
        ),
        "DependsOnDriftedCapability"
    );

    // IncommensurableMatch — arms' eval budgets differ on the matched dim.
    let mut mismatched = budgets();
    mismatched.insert(
        "eval:b".to_string(),
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), 999)],
        ),
    );
    let e = register_err(
        &mut r,
        &base,
        EngineContext {
            resolve_budget: Some(Box::new(move |r: &str| mismatched.get(r).cloned())),
            ..ctx()
        },
    );
    assert!(e == "IncommensurableMatch" || e == "UnmatchedBudget", "{e}");
}

#[test]
fn refusal_codes_cover_the_closed_set() {
    // Every ExperimentRefusal member renders a stable code (T-LCD-14 — the
    // boundary renders the code, never a warning).
    for r in [
        hh_lab::experiment::ExperimentRefusal::MissingMatchSpec { arm: "a".into() },
        hh_lab::experiment::ExperimentRefusal::UnbudgetedArm { arm: "a".into() },
        hh_lab::experiment::ExperimentRefusal::UnmatchedBudget { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::IncommensurableMatch { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::MissingPricingTable { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::InadmissibleFactor {
            factor: "f".into(),
            reason: "r".into(),
        },
        hh_lab::experiment::ExperimentRefusal::MissingPreRegistration,
        hh_lab::experiment::ExperimentRefusal::SplitUnassigned {
            suite_ref: "s".into(),
        },
        hh_lab::experiment::ExperimentRefusal::LeakedSplit { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::UnsealedArtifact {
            artifact_ref: "a".into(),
        },
        hh_lab::experiment::ExperimentRefusal::InsufficientReplicates {
            detail: "d".into(),
            have: 1,
            need: 5,
        },
        hh_lab::experiment::ExperimentRefusal::ResolutionInsufficient { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::ProfilePinnedAcrossProfiles { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::DependsOnDriftedCapability {
            capability: "c".into(),
        },
        hh_lab::experiment::ExperimentRefusal::NotARetirementDiff { detail: "d".into() },
        hh_lab::experiment::ExperimentRefusal::AdaptiveOutsideSearch { detail: "d".into() },
    ] {
        assert_ne!(refusal_code(&r), "");
    }
}

// ── expand ──────────────────────────────────────────────────────────────────

#[test]
fn expand_is_deterministic_and_content_addressed() {
    let mut r = rig("expand", 0);
    let s = spec(ExperimentKind::Comparative);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    let eid = eng.register(&s).unwrap();
    let p1 = eng.expand(&eid).unwrap();
    let p2 = eng.expand(&eid).unwrap();
    assert_eq!(p1, p2);
    let plan = r.docs.plan(&p1).unwrap().unwrap();
    // 2 arms × 1 task × 2 replicates = 4 run plans; the identity binds
    // (experiment, arm, configuration_version, task, replicate).
    assert_eq!(plan.cells.len(), 2);
    assert_eq!(plan.run_plans.len(), 4);
    let ids: BTreeMap<_, _> = plan
        .run_plans
        .iter()
        .map(|p| (p.run_plan_id.clone(), p.replicate_index))
        .collect();
    assert_eq!(ids.len(), 4);
}

#[test]
fn expand_marks_ineligible_cells_na() {
    let mut r = rig("expand-na", 0);
    let s = spec(ExperimentKind::Comparative);
    let ineligible = pinned("level.clear");
    let mut eng = ExperimentEngine::new(
        &mut r.store,
        r.docs.clone(),
        EngineContext {
            level_ineligible: Some(Box::new(move |r: &str| {
                (r == ineligible).then_some(hh_ontology::compliance::NaReason::Capability)
            })),
            ..ctx()
        },
    );
    let eid = eng.register(&s).unwrap();
    let pid = eng.expand(&eid).unwrap();
    let plan = r.docs.plan(&pid).unwrap().unwrap();
    // The `clear_tool_results` cell is planned-with-`n/a{capability}` and
    // yields no run plans (T-LCD-15 — typed, never dropped).
    let na_cell = plan.cells.iter().find(|c| c.arm_id == "arm:b").unwrap();
    assert!(na_cell.na_reason.is_some());
    assert_eq!(plan.run_plans.len(), 2);
    assert!(plan.run_plans.iter().all(|p| {
        plan.cells
            .iter()
            .find(|c| c.cell_id == p.cell_id)
            .unwrap()
            .arm_id
            == "arm:a"
    }));
}

#[test]
fn expand_requires_register_and_resolvers() {
    let mut r = rig("expand-gates", 0);
    let s = spec(ExperimentKind::Comparative);
    let eid = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        assert!(matches!(
            eng.expand(&s.experiment_id).unwrap_err(),
            ExperimentError::UnknownExperiment { .. }
        ));
        eng.register(&s).unwrap()
    };
    // No suite-task resolver → Unresolvable, not a guess.
    let mut eng = ExperimentEngine::new(
        &mut r.store,
        r.docs.clone(),
        EngineContext {
            suite_tasks: None,
            ..ctx()
        },
    );
    assert!(matches!(
        eng.expand(&eid).unwrap_err(),
        ExperimentError::Unresolvable { .. }
    ));
}

// ── open / lifecycle ────────────────────────────────────────────────────────

#[test]
fn open_commits_declared_planned_and_bracket() {
    let mut r = rig("open", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, run_id) = open(&mut r, &s);
    let view = ExperimentView::fold(r.store.events(&run_id).unwrap());
    assert_eq!(view.declared.as_ref().unwrap().experiment_id, eid);
    assert_eq!(view.plans.len(), 4);
    assert!(view.drift_brackets.iter().any(|b| b.phase == "opened"));
    // Rebuild equality — the fold is deterministic over the same stream.
    assert_eq!(view, ExperimentView::fold(r.store.events(&run_id).unwrap()));
    // A second open on the same experiment refuses AlreadyOpen.
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    assert!(matches!(
        eng.open_experiment(&eid).unwrap_err(),
        ExperimentError::AlreadyOpen { .. }
    ));
}

#[test]
fn open_without_plan_is_plan_not_found() {
    let mut r = rig("open-noplan", 0);
    let s = spec(ExperimentKind::Comparative);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    let eid = eng.register(&s).unwrap();
    assert!(matches!(
        eng.open_experiment(&eid).unwrap_err(),
        ExperimentError::PlanNotFound { .. }
    ));
}

#[test]
fn lifecycle_next_claim_launch_settle_close() {
    let mut r = rig("lifecycle", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let report = run_to_close(&mut r, &eid, 40);
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
    assert_eq!(
        report.coverage.get("runs_accepted").and_then(Json::as_int),
        Some(4)
    );
    assert_eq!(report.outcome_counts.get("scored"), Some(&4));
}

#[test]
fn claim_blocks_second_claimant_until_expiry() {
    let mut r = rig("claim-expiry", 1_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx()).with_ttls(60_000, 100);
    eng.attach(&eid).unwrap();
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let t1 = eng.claim(&rpid, "worker-1").unwrap();
    // A second claimant on the live claim → WouldBlock.
    assert!(matches!(
        eng.claim(&rpid, "worker-2").unwrap_err(),
        ExperimentError::WouldBlock { .. }
    ));
    // The claim is still inside its TTL — `next` holds the plan back.
    if let NextVerdict::Plan { run_plan_id } = eng.next().unwrap() {
        assert_ne!(run_plan_id, rpid);
    }
    // Advance past the claim TTL — reconcile appends `claim_expired` and the
    // plan returns to eligible.
    r.clock.advance(200);
    let t2 = eng.claim(&rpid, "worker-2").unwrap();
    assert_ne!(t1.lease_id, t2.lease_id);
    let view = eng.project().unwrap();
    assert!(view
        .plans
        .get(&rpid)
        .unwrap()
        .claim
        .as_ref()
        .map(|c| c.holder == "worker-2")
        .unwrap_or(false));
    let _ = t1;
}

#[test]
fn launch_requires_a_live_claim() {
    let mut r = rig("launch-gates", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    // Unclaimed → BadPlanState.
    let bogus = ClaimTicket {
        run_plan_id: rpid.clone(),
        holder: "nobody".to_string(),
        lease_id: "lease:bogus".to_string(),
        expires_at_ms: u64::MAX,
    };
    assert!(matches!(
        eng.launch(&bogus, "subject").unwrap_err(),
        ExperimentError::BadPlanState { .. }
    ));
    // Claim then launch with a different lease id → BadPlanState.
    let ticket = eng.claim(&rpid, "driver").unwrap();
    let mut wrong = ticket.clone();
    wrong.lease_id = "lease:wrong".to_string();
    assert!(matches!(
        eng.launch(&wrong, "subject").unwrap_err(),
        ExperimentError::BadPlanState { .. }
    ));
    let out = eng.launch(&ticket, "subject").unwrap();
    assert_eq!(out.attempt_no, 1);
    let exp_run_id = eng.run_id().map(str::to_string);
    drop(eng);
    // The subject run carries the complete row-key binding — the arm is
    // whichever plan the seeded order dispatches first.
    let expected_arm = ExperimentView::fold(r.store.events(&_run_id).unwrap()).plans[&rpid]
        .arm_id
        .clone();
    let m = r.store.manifest(&out.run_id).unwrap();
    let binding = m.experiment.as_ref().unwrap();
    assert_eq!(binding.arm_id.as_deref(), Some(expected_arm.as_str()));
    assert_eq!(binding.attempt_no, Some(1));
    assert_eq!(binding.comparable, Some(true));
    assert_eq!(
        binding.registry_snapshot_id.as_deref(),
        Some(pinned("registry.snap").as_str())
    );
    assert_eq!(m.parent_run_id.as_deref(), exp_run_id.as_deref());
}

#[test]
fn settle_is_exactly_once() {
    let mut r = rig("settle-once", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, run_id) = open(&mut r, &s);
    let (rpid, launched) = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        let rpid = match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => run_plan_id,
            other => panic!("{other:?}"),
        };
        let ticket = eng.claim(&rpid, "driver").unwrap();
        let l = eng.launch(&ticket, "subject").unwrap();
        (rpid, l)
    };
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[(DimensionId::ModelCalls, 40)],
    );
    let o1 = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        let o1 = eng.settle(&rpid).unwrap();
        assert!(o1.accepted);
        assert_eq!(o1.outcome_class, OutcomeClass::Scored);
        o1
    };
    let n = r
        .store
        .events(&run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "measurement.experiment.run_settled")
        .count();
    let o2 = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        eng.settle(&rpid).unwrap()
    };
    assert_eq!(o1, o2);
    assert_eq!(
        r.store
            .events(&run_id)
            .unwrap()
            .iter()
            .filter(|e| e.class == "measurement.experiment.run_settled")
            .count(),
        n,
        "a second settle appends nothing (S-2 exactly-once)"
    );
}

#[test]
fn settle_replans_infra_failure_within_policy() {
    let mut r = rig("settle-replan", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, l1) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l1.run_id,
        &l1.subject_writer,
        StopReason::InfrastructureFailure {
            error_class: InfraError {
                family: InfraErrorFamily::Env,
                class: "image_pull_failed".to_string(),
            },
        },
        &[],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert!(out.replanned);
    assert!(out.superseded);
    assert!(!out.accepted);
    // The plan is eligible again — the second attempt launches at attempt_no 2.
    let ticket = eng.claim(&rpid, "driver").unwrap();
    let l2 = eng.launch(&ticket, "subject").unwrap();
    assert_eq!(l2.attempt_no, 2);
}

#[test]
fn settle_final_on_budget_exhausted_stop() {
    let mut r = rig("settle-final", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::BudgetExhausted {
            budget_id: "budget:x".to_string(),
            dimension: DimensionId::ModelCalls,
        },
        &[(DimensionId::ModelCalls, 100)],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert!(!out.accepted);
    assert!(out.plan_final);
    // The plan is terminally done — never eligible again.
    assert!(!eng
        .project()
        .unwrap()
        .plans
        .get(&rpid)
        .unwrap()
        .eligible_at(0));
}

#[test]
fn unsettled_run_is_run_not_finished() {
    let mut r = rig("settle-unfinished", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, _l) = launch_one(&mut r, &eid);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    assert!(matches!(
        eng.settle(&rpid).unwrap_err(),
        ExperimentError::RunNotFinished { .. }
    ));
}

fn launch_one(rig: &mut Rig, eid: &str) -> (String, LaunchOutcome) {
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    eng.attach(eid).unwrap();
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&rpid, "driver").unwrap();
    let l = eng.launch(&ticket, "subject").unwrap();
    (rpid, l)
}

// ── pause / close gates (S-4) ───────────────────────────────────────────────

#[test]
fn pause_gates_next_claim_launch_but_not_settle() {
    let mut r = rig("pause", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    eng.pause(PauseReason::Operator).unwrap();
    assert!(matches!(
        eng.next().unwrap_err(),
        ExperimentError::ExperimentPaused { .. }
    ));
    assert!(matches!(
        eng.claim(&rpid, "driver").unwrap_err(),
        ExperimentError::ExperimentPaused { .. }
    ));
    // In-flight settles still land while paused.
    drop(eng);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::Completed,
        &[(DimensionId::ModelCalls, 10)],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    assert!(eng.settle(&rpid).unwrap().accepted);
    eng.resume().unwrap();
    assert!(matches!(
        eng.next().unwrap(),
        NextVerdict::Plan { .. } | NextVerdict::Done
    ));
}

#[test]
fn close_blocks_while_plans_are_open() {
    let mut r = rig("close-blocked", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    // Eligible plans remain — `ExperimentReadyToLaunch` holds.
    let e = eng.close(false).unwrap_err();
    assert!(matches!(e, ExperimentError::CloseBlocked { .. }));
    assert!(e.to_string().contains("ExperimentReadyToLaunch"));
    // A claimed-but-live plan still blocks.
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let _t = eng.claim(&rpid, "driver").unwrap();
    assert!(matches!(
        eng.close(false).unwrap_err(),
        ExperimentError::CloseBlocked { .. }
    ));
    // `partial = true` closes with the honest coverage row.
    let report = eng.close(true).unwrap();
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Partial);
    // Post-close gates (S-4): no next/claim/launch/settle.
    assert!(matches!(
        eng.next().unwrap_err(),
        ExperimentError::ExperimentClosed { .. }
    ));
    assert!(matches!(
        eng.settle(&rpid).unwrap_err(),
        ExperimentError::ExperimentClosed { .. }
    ));
}

// ── KP-E1…E5 — kill the engine mid-lifecycle, restore from the ledger ───────

#[test]
fn kill_point_recovery_kp_e1_through_e5() {
    let mut r = rig("kp", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, run_id) = open(&mut r, &s);

    // KP-E1: kill after open — the committed stream is the whole state.
    drop_store(&mut r);
    let view = ExperimentView::fold(r.store.events(&run_id).unwrap());
    assert_eq!(view.plans.len(), 4);

    // KP-E2: kill between claim and launch — the claim row is committed; the
    // restored engine sees it (claim still live at this clock).
    let (rpid, ticket) = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        let rpid = match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => run_plan_id,
            other => panic!("{other:?}"),
        };
        let t = eng.claim(&rpid, "driver").unwrap();
        (rpid, t)
    };
    drop_store(&mut r);
    let launched = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        eng.launch(&ticket, "subject").unwrap()
    };

    // KP-E3: kill with the subject in flight — settle after restore.
    drop_store(&mut r);
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[(DimensionId::ModelCalls, 20)],
    );
    {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        assert!(eng.settle(&rpid).unwrap().accepted);
    }

    // KP-E4: kill between settle and the rest of the sweep — continue.
    drop_store(&mut r);
    // KP-E5: kill just before close — close after restore.
    let report = run_to_close(&mut r, &eid, 30);
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
    // Rebuild equality at the end: the view is a pure function of the stream.
    let v1 = ExperimentView::fold(r.store.events(&run_id).unwrap());
    let v2 = ExperimentView::fold(r.store.events(&run_id).unwrap());
    assert_eq!(v1, v2);
    assert!(v1.closed.is_some());
}

fn drop_store(r: &mut Rig) {
    r.store = Store::open_with(
        &r.root,
        Box::new(r.clock.clone()),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
}

// ── E-2 — budget containment ────────────────────────────────────────────────

#[test]
fn launch_refuses_insufficient_budget_and_stays_replannable() {
    let mut r = rig("budget", 0);
    let s = spec(ExperimentKind::Comparative);
    // A pool that funds exactly one slice (100) of the four planned.
    let mut small = budgets();
    small.insert(
        "pool".to_string(),
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
        ),
    );
    let pool_map = small;
    let eid;
    {
        let mut eng = ExperimentEngine::new(
            &mut r.store,
            r.docs.clone(),
            EngineContext {
                resolve_budget: Some(Box::new(move |r: &str| pool_map.get(r).cloned())),
                ..ctx()
            },
        );
        eid = eng.register(&s).unwrap();
        eng.expand(&eid).unwrap();
        eng.open_experiment(&eid).unwrap();
    }
    let mut eng = ExperimentEngine::new(
        &mut r.store,
        r.docs.clone(),
        EngineContext {
            resolve_budget: Some(Box::new(move |r: &str| budgets().get(r).cloned())),
            ..ctx()
        },
    );
    // NOTE: the engine re-resolves the pool spec at open only; the pool root
    // is already committed. Attach with any resolver — slices check the pool.
    eng.attach(&eid).unwrap();
    // Claim plan1 (its claim stays live while the pool drains underneath it).
    let rpid1 = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let t1 = eng.claim(&rpid1, "driver").unwrap();
    // Plan2 launches and settles — draining the pool (consumed 100 of 100).
    let rpid2 = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    assert_ne!(rpid1, rpid2);
    let t2 = eng.claim(&rpid2, "driver").unwrap();
    let l = eng.launch(&t2, "subject").unwrap();
    drop(eng);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::Completed,
        &[(DimensionId::ModelCalls, 100)],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    eng.settle(&rpid2).unwrap();
    // The pool is drained — launching the still-claimed plan1 fails
    // `InsufficientBudget`; the claim is not consumed (re-plannable).
    let e = eng.launch(&t1, "subject").unwrap_err();
    assert!(
        matches!(
            e,
            ExperimentError::Budget(_) | ExperimentError::InsufficientBudget { .. }
        ),
        "{e:?}"
    );
    // After the claim lapses, `next` emits `paused{budget_exhausted}` and
    // reports the exhaustion; the pause then gates further ops (S-4).
    r.clock.advance(60_000);
    assert_eq!(eng.next().unwrap(), NextVerdict::BudgetExhausted);
    assert!(matches!(
        eng.next().unwrap_err(),
        ExperimentError::ExperimentPaused { .. }
    ));
    let view = eng.project().unwrap();
    assert_eq!(view.paused.as_deref(), Some("budget_exhausted"));
}

// ── E-4 — exploratory ───────────────────────────────────────────────────────

#[test]
fn exploratory_registers_and_runs_non_comparable() {
    let mut r = rig("exploratory", 0);
    let mut s = spec(ExperimentKind::Exploratory);
    s.pre_registration = None; // not required for exploratory
    for a in &mut s.arms {
        a.match_spec = None; // `none` mode — never comparable
        a.search_budget = None;
    }
    s.experiment_id = s.experiment_id();
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    let eid = eng.register(&s).unwrap();
    eng.expand(&eid).unwrap();
    let run_id = eng.open_experiment(&eid).unwrap();
    let rpid = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&rpid, "driver").unwrap();
    let l = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    let m = r.store.manifest(&l.run_id).unwrap();
    // E-4: the row is marked non-comparable.
    assert_eq!(m.experiment.as_ref().unwrap().comparable, Some(false));
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::Completed,
        &[(DimensionId::ModelCalls, 10)],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert!(out.accepted);
    let _ = run_id;
}

#[test]
fn cancelled_replans_and_final_cancels() {
    let mut r = rig("cancel", 0);
    // on_cancel = final — a cancelled subject terminates the plan.
    let mut s = spec(ExperimentKind::Comparative);
    s.reattempt.on_cancel = CancelPolicy::Final;
    s.experiment_id = s.experiment_id();
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::Cancelled {
            by: CancelledBy::Principal,
        },
        &[],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert!(out.plan_final);
    assert!(!out.replanned);
}

// ── Exemplars — the two ADR-0156 documents at Stage-3 size (AC-R-2.10.3-8;
//    AC-R-2.10.3-9 executed under ADR-0213's OQ-363 interim rule) ────────────

fn exemplar_pins() -> hh_lab::exemplars::ExemplarPins {
    hh_lab::exemplars::ExemplarPins {
        suite_ref: pinned("suite.tb2"),
        held_out_split_ref: pinned("split.held_out"),
        split_assignment_ref: pinned("split.assign"),
        registry_snapshot_id: pinned("registry.snap"),
        eval_budget: "eval:a".to_string(),
        search_budget: "search:a".to_string(),
        experiment_budget: "pool".to_string(),
        instrument_budget: "instr".to_string(),
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
    }
}

/// An engine context over `n` held-out tasks at the Stage-3 replicate floor.
fn ctx_exemplar(tag: &str, n_tasks: usize) -> EngineContext<'static> {
    let map = budgets();
    let ids: Vec<String> = (0..n_tasks)
        .map(|i| pinned(&format!("{tag}.task.{i}")))
        .collect();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| map.get(r).cloned())),
        suite_tasks: Some(Box::new(move |_spec: &ExperimentSpec| {
            Some(
                ids.iter()
                    .map(|t| ExpandTask {
                        task_id: t.clone(),
                        split_label: SplitLabel::HeldOut,
                    })
                    .collect(),
            )
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            })
        })),
        min_replicates: hh_lab::exemplars::EXEMPLAR_REPLICATES,
        ..Default::default()
    }
}

fn open_with(rig: &mut Rig, spec: &ExperimentSpec, tag: &str, n_tasks: usize) -> (String, String) {
    let mut eng =
        ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx_exemplar(tag, n_tasks));
    let eid = eng.register(spec).unwrap();
    eng.expand(&eid).unwrap();
    let run_id = eng.open_experiment(&eid).unwrap();
    (eid, run_id)
}

/// Drive `next → claim → launch → finish → settle` until the sweep is done,
/// then close — over a context factory (the exemplars' task axis).
fn run_to_close_ctx(
    rig: &mut Rig,
    eid: &str,
    consumed: i64,
    tag: &str,
    n_tasks: usize,
) -> hh_experiment::events::ExperimentReport {
    loop {
        let mut eng =
            ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx_exemplar(tag, n_tasks));
        eng.attach(eid).unwrap();
        match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => {
                let ticket = eng.claim(&run_plan_id, "driver").unwrap();
                let launched = eng.launch(&ticket, "subject").unwrap();
                drop(eng);
                finish_subject(
                    &mut rig.store,
                    &launched.run_id,
                    &launched.subject_writer,
                    StopReason::Completed,
                    &[(DimensionId::ModelCalls, consumed)],
                );
                let mut eng = ExperimentEngine::new(
                    &mut rig.store,
                    rig.docs.clone(),
                    ctx_exemplar(tag, n_tasks),
                );
                eng.attach(eid).unwrap();
                let out = eng.settle(&run_plan_id).unwrap();
                assert!(out.accepted);
            }
            NextVerdict::Done => break,
            other => panic!("unexpected next: {other:?}"),
        }
    }
    let mut eng =
        ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx_exemplar(tag, n_tasks));
    eng.attach(eid).unwrap();
    eng.close(false).unwrap()
}

#[test]
fn exemplar_compaction_family_executes_at_stage3_size() {
    let mut r = rig("exemplar-compaction", 0);
    let spec = hh_lab::exemplars::compaction_family_v1(
        &exemplar_pins(),
        &hh_lab::exemplars::CompactionFamilyPins {
            evict_oldest_ref: pinned("variant.evict_oldest"),
            clear_tool_results_ref: pinned("variant.clear_tool_results"),
            model_level_ref: pinned("model.fixed"),
            environment_level_ref: pinned("env.tb2"),
            artifacts: (
                Ref::new("definition:compaction", pinned("artifact.evict")),
                Ref::new("definition:compaction", pinned("artifact.clear")),
            ),
        },
        1,
    );
    let (eid, run_id) = open_with(&mut r, &spec, "exc", 3);
    // 2 arms × 3 tasks × 5 replicates = 30 plans.
    {
        let eng_view = ExperimentView::fold(r.store.events(&run_id).unwrap());
        assert_eq!(eng_view.plans.len(), 30);
    }
    let report = run_to_close_ctx(&mut r, &eid, 5, "exc", 3);
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
}

#[test]
fn exemplar_control_strategy_executes_with_iso_companion() {
    let mut r = rig("exemplar-control", 0);
    let pricing = hh_budget::pricing::PricingTableRef {
        table_id: "pricing:test".to_string(),
        version: "1".to_string(),
        pin: Some(pinned("pricing.table")),
    };
    let spec = hh_lab::exemplars::control_strategy_family_v1(
        &exemplar_pins(),
        &hh_lab::exemplars::ControlStrategyFamilyPins {
            react_minimal_ref: pinned("variant.react_minimal"),
            react_steerable_ref: pinned("variant.react_steerable"),
            model_family_a_ref: pinned("model.family_a"),
            model_family_b_ref: pinned("model.family_b"),
            artifacts: (
                Ref::new("definition:control", pinned("artifact.min_a")),
                Ref::new("definition:control", pinned("artifact.min_b")),
                Ref::new("definition:control", pinned("artifact.steer_a")),
                Ref::new("definition:control", pinned("artifact.steer_b")),
            ),
            iso_artifact: Ref::new("definition:control", pinned("artifact.steer_a")),
            pricing_table_ref: pricing,
        },
        1,
    );
    let (eid, run_id) = open_with(&mut r, &spec, "exctl", 2);
    // 5 arms × 2 tasks × 5 replicates = 50 plans — the iso_cost companion's
    // runs are distinct subject runs (ADR-0213's interim duplication).
    {
        let view = ExperimentView::fold(r.store.events(&run_id).unwrap());
        assert_eq!(view.plans.len(), 50);
    }
    let report = run_to_close_ctx(&mut r, &eid, 5, "exctl", 2);
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
    // Rebuild equality — the whole sweep re-folds from the ledger stream.
    let v1 = ExperimentView::fold(r.store.events(&run_id).unwrap());
    let v2 = ExperimentView::fold(r.store.events(&run_id).unwrap());
    assert_eq!(v1, v2);
}

// ── S3.4a self-review fixes: outcome-class semantics, order, E-3/E-4 ───────

/// AC-R-2.10.3-6 — `oracle_failure` marks the cell regrade-only. It must never
/// re-run the plan: no `run_replanned` event, no second dispatch, and the
/// settlement row carries the regrade markers.
#[test]
fn oracle_failure_marks_regrade_never_reruns() {
    let mut r = rig("oracle-regrade", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    // The subject finished cleanly but the oracle row reports a fault —
    // the experiment's own settlement sees `oracle_failure`.
    let consumed = mint(
        &r.store,
        &l.run_id,
        "control.budget.consumed",
        Json::obj([
            ("dimension", Json::str("model_calls")),
            ("amount", Json::Int(10)),
        ]),
    );
    let ev = mint(
        &r.store,
        &l.run_id,
        "lifecycle.run.finished",
        Json::obj([
            ("stop_reason", Json::obj([("kind", Json::str("completed"))])),
            ("outcome_class", Json::str("oracle_failure")),
            ("regrade_reason", Json::str("oracle_timeout")),
        ]),
    );
    r.store
        .append(&l.run_id, &l.subject_writer, vec![consumed, ev])
        .unwrap();
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert_eq!(out.outcome_class, OutcomeClass::OracleFailure);
    assert!(out.plan_final);
    assert!(!out.replanned);
    assert!(!out.superseded);
    assert!(out.regrade_pending);
    drop(eng);
    // No re-run: the ledger carries no `run_replanned` naming this plan.
    let evs = r.store.events(&run_id).unwrap().to_vec();
    assert!(!evs.iter().any(|e| {
        e.class == "measurement.experiment.run_replanned"
            && e.payload.get("run_plan_id").and_then(Json::as_str) == Some(rpid.as_str())
    }));
    // The settled row carries the regrade members.
    let settled = evs
        .iter()
        .find(|e| {
            e.class == "measurement.experiment.run_settled"
                && e.payload.get("run_plan_id").and_then(Json::as_str) == Some(rpid.as_str())
        })
        .unwrap();
    assert_eq!(settled.payload.get("regrade"), Some(&Json::Bool(true)));
    // And `next` never re-dispatches the plan.
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    if let NextVerdict::Plan { run_plan_id: next } = eng.next().unwrap() {
        assert_ne!(next, rpid);
    }
    // Idempotent replay.
    let out2 = eng.settle(&rpid).unwrap();
    assert_eq!(out2.outcome_class, OutcomeClass::OracleFailure);
    assert!(out2.regrade_pending);
}

/// §2.3 cancellation table — `cancelled{principal}` and `cancelled{parent}`
/// are final regardless of the declared `on_cancel`; `cancelled{operator}`
/// follows the declared policy.
#[test]
fn cancelled_by_principal_is_final_under_replan_policy() {
    let mut r = rig("cancel-principal", 0);
    let mut s = spec(ExperimentKind::Comparative);
    s.reattempt.on_cancel = CancelPolicy::Replan;
    s.experiment_id = s.experiment_id();
    let (eid, _run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::Cancelled {
            by: CancelledBy::Principal,
        },
        &[],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out = eng.settle(&rpid).unwrap();
    assert!(out.plan_final);
    assert!(!out.replanned);
    assert!(!eng
        .project()
        .unwrap()
        .plans
        .get(&rpid)
        .unwrap()
        .eligible_at(0));
    drop(eng);

    // `operator` cancellation under the same spec re-plans.
    let (rpid2, l2) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l2.run_id,
        &l2.subject_writer,
        StopReason::Cancelled {
            by: CancelledBy::Operator,
        },
        &[],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let out2 = eng.settle(&rpid2).unwrap();
    assert!(out2.replanned);
}

/// §2.2 — the recorded `run_planned` order under `interleaved` is
/// task-blocked, arm-interleaved: every task's plans are contiguous and each
/// replicate round within a block carries every arm.
#[test]
fn interleaved_order_is_task_blocked_arm_interleaved() {
    let mut r = rig("order-blocks", 0);
    let mut s = spec(ExperimentKind::Comparative);
    s.scheduling.order = OrderKind::Interleaved;
    s.experiment_id = s.experiment_id();
    let c = EngineContext {
        suite_tasks: Some(Box::new(|_| {
            Some(vec![
                ExpandTask {
                    task_id: "task:1".to_string(),
                    split_label: SplitLabel::Dev,
                },
                ExpandTask {
                    task_id: "task:2".to_string(),
                    split_label: SplitLabel::Dev,
                },
            ])
        })),
        ..ctx()
    };
    let (eid, run_id) = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), c);
        let eid = eng.register(&s).unwrap();
        eng.expand(&eid).unwrap();
        let run_id = eng.open_experiment(&eid).unwrap();
        (eid, run_id)
    };
    let _ = eid;
    let evs = r.store.events(&run_id).unwrap().to_vec();
    let planned: Vec<(String, String, i64)> = evs
        .iter()
        .filter(|e| e.class == "measurement.experiment.run_planned")
        .map(|e| {
            (
                e.payload
                    .get("task_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string(),
                e.payload
                    .get("arm_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string(),
                e.payload
                    .get("replicate_index")
                    .and_then(Json::as_int)
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(planned.len(), 8); // 2 arms × 2 tasks × 2 replicates
                                  // Task-blocked: task membership changes at most once.
    let mut task_runs: Vec<String> = Vec::new();
    for (task, _, _) in &planned {
        if task_runs.last() != Some(task) {
            task_runs.push(task.clone());
        }
    }
    assert_eq!(task_runs.len(), 2, "planned order must be task-blocked");
    // Within each task block: rep-major, and every replicate round covers
    // both arms (arm-interleaved).
    let mut i = 0;
    while i < planned.len() {
        let task = planned[i].0.clone();
        let mut j = i;
        let mut block = Vec::new();
        while j < planned.len() && planned[j].0 == task {
            block.push(planned[j].clone());
            j += 1;
        }
        assert_eq!(block.len(), 4, "task {task}: 2 arms × 2 reps");
        let reps: Vec<i64> = block.iter().map(|r| r.2).collect();
        let mut sorted = reps.clone();
        sorted.sort_unstable();
        assert_eq!(reps, sorted, "task {task}: replicate rounds are rep-major");
        for rep in [0i64, 1] {
            let arms: std::collections::BTreeSet<&str> = block
                .iter()
                .filter(|r| r.2 == rep)
                .map(|r| r.1.as_str())
                .collect();
            assert_eq!(
                arms.len(),
                2,
                "task {task} rep {rep} must interleave both arms"
            );
        }
        i = j;
    }
    // The deterministic order is stable: `permutation_seed` rotates arm ranks
    // but never breaks task blocks.
}

/// E-3 — `utilization_floor_ppm` arms the under-utilisation annotation; a
/// matched pair whose realized consumption crosses the tolerance flips
/// `budget_match.status` to `imbalanced` at close.
#[test]
fn under_utilised_and_budget_match_recheck_at_close() {
    let mut r = rig("e3-e4", 0);
    let mut s = spec(ExperimentKind::Comparative);
    for arm in &mut s.arms {
        arm.match_spec.as_mut().unwrap().utilization_floor_ppm = Some(500_000);
    }
    s.experiment_id = s.experiment_id();
    let (eid, run_id) = open(&mut r, &s);
    // Drive the sweep manually so arm:a consumes near the cap and arm:b sips.
    loop {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => {
                let ticket = eng.claim(&run_plan_id, "driver").unwrap();
                let launched = eng.launch(&ticket, "subject").unwrap();
                drop(eng);
                let view = ExperimentView::fold(r.store.events(&run_id).unwrap());
                let arm = view.plans[&run_plan_id].arm_id.clone();
                let consumed = if arm == "arm:a" { 100 } else { 10 };
                finish_subject(
                    &mut r.store,
                    &launched.run_id,
                    &launched.subject_writer,
                    StopReason::Completed,
                    &[(DimensionId::ModelCalls, consumed)],
                );
                let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
                eng.attach(&eid).unwrap();
                assert!(eng.settle(&run_plan_id).unwrap().accepted);
            }
            NextVerdict::Done => break,
            other => panic!("unexpected next: {other:?}"),
        }
    }
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let report = eng.close(false).unwrap();
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Completed);
    drop(eng);
    // arm:b medians at 10% of cap → under the 50% floor; arm:a at the cap.
    assert_eq!(report.under_utilised, vec!["arm:b".to_string()]);
    // arm:a medians 100 vs arm:b medians 10 — tolerance 0 → imbalanced.
    assert_eq!(report.budget_match.len(), 1);
    let rec = &report.budget_match[0];
    assert_eq!(rec.get("status").and_then(Json::as_str), Some("imbalanced"));
    let arms = rec.get("arms").and_then(|a| match a {
        Json::Arr(v) => Some(v.len()),
        _ => None,
    });
    assert_eq!(arms, Some(2));
    // The closed row carries the additive re-check members.
    let evs = r.store.events(&run_id).unwrap().to_vec();
    let closed = evs
        .iter()
        .find(|e| e.class == "measurement.experiment.closed")
        .unwrap();
    assert!(closed.payload.get("budget_match").is_some());
    // E-3's distribution record — per arm per dim `{median, samples}`.
    let util = closed.payload.get("utilization").unwrap();
    let arm_b = util
        .get("arm:b")
        .and_then(|a| a.get("model_calls"))
        .unwrap();
    assert_eq!(arm_b.get("median").and_then(Json::as_int), Some(100_000));
    assert_eq!(
        closed.payload.get("under_utilised").and_then(|u| match u {
            Json::Arr(v) => Some(v.len()),
            _ => None,
        }),
        Some(1)
    );
}

/// E-4 — a partial close renders insufficient-replicate cells as
/// `n/a{not_run}`; a completed close leaves `na_cells` empty.
#[test]
fn partial_close_reports_not_run_cells() {
    let mut r = rig("partial-close", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _run_id) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let report = eng.close(true).unwrap();
    assert_eq!(report.status, hh_experiment::events::CloseStatus::Partial);
    assert!(!report.na_cells.is_empty());
    assert!(report
        .na_cells
        .iter()
        .all(|c| c.get("reason").and_then(Json::as_str) == Some("not_run")));
}

/// Ordering inside the ledger — `settled` lands before `excluded`, which
/// lands before `replanned`, on every retry path (§2.3).
#[test]
fn infra_retry_lands_settled_excluded_replanned_in_order() {
    let mut r = rig("retry-order", 0);
    let s = spec(ExperimentKind::Comparative);
    let (eid, run_id) = open(&mut r, &s);
    let (rpid, l) = launch_one(&mut r, &eid);
    finish_subject(
        &mut r.store,
        &l.run_id,
        &l.subject_writer,
        StopReason::InfrastructureFailure {
            error_class: InfraError {
                family: InfraErrorFamily::Env,
                class: "image_pull_failed".to_string(),
            },
        },
        &[],
    );
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    assert!(eng.settle(&rpid).unwrap().replanned);
    drop(eng);
    let evs = r.store.events(&run_id).unwrap().to_vec();
    let pos = |class: &str| {
        evs.iter()
            .position(|e| {
                e.class == class
                    && e.payload.get("run_plan_id").and_then(Json::as_str) == Some(rpid.as_str())
            })
            .unwrap_or(usize::MAX)
    };
    let settled = pos("measurement.experiment.run_settled");
    let excluded = pos("measurement.experiment.run_excluded");
    let replanned = pos("measurement.experiment.run_replanned");
    assert!(settled < excluded && excluded < replanned);
}
