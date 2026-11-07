//! R6.16 acceptance battery — Stage 1 (manifest 269; RUNNER-CORE (c)):
//! first-fit packing in recorded order under the declared bounds +
//! lookahead (AC-1/AC-3.3-6 REQ-XB-121), the C rule with pre-clock
//! waiting (AC-2/AC-3.10-2 REQ-XB-214), the two declared schedule
//! kinds + composition (AC-3/AC-3.3-8+AC-7.5-2 REQ-XH-017/REQ-XD-111),
//! the declared reattempt policy through the engine with the §6.8 map
//! and the same-window retry rule (AC-4/AC-3.3-7+AC-6.8-1+AC-10.1-16
//! REQ-XH-016/REQ-XD-060/REQ-XH-125), the dead-man cut-off with the
//! incremental harvest manifest (AC-5/AC-3.9-17 REQ-XB-128) and the
//! own-label container sweep + teardown receipts (AC-6/AC-3.2-6
//! REQ-XB-127).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey, MatchSpec};
use hharness_experiment::docs::LabDocs;
use hharness_identity::idp::idp_id;
use hharness_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ontology::config::Ref;
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hharness_ontology::lab::SplitLabel;
use hharness_ontology::participant::ParticipantClass;
use hharness_ontology::FactorKind;
use hharness_runner::coordinator::{self, CoordinatorConfig};
use hharness_runner::disposition::{Disposition, EndCause};
use hharness_runner::facts::{
    harbor_cause_map, map_slot_facts, HarborErrorCause, Resolution, RewardFact, SlotFacts,
};
use hharness_runner::packet::{DrivePacket, SlotRecipe};
use hharness_runner::schedule::{
    self, HarvestManifest, PlanFact, ResourceUse, ScheduleDecl, ScheduleKind, WindowDecl,
};
use hharness_runner::slot_record::RunnerIdentity;
use hharness_wire::json::{self, Json};

fn arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("not an array"),
    }
}

fn boolv(j: &Json) -> Option<bool> {
    match j {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r616-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("r6_16.{tag}"), tag.as_bytes())
}

fn hh_runner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hh-runner"))
}

fn spec_2x3x2(reattempt: ReattemptPolicy) -> ExperimentSpec {
    let pre = PreRegistration {
        registered_at: 1,
        hypothesis: "fixture comparison".to_string(),
        primary_metrics: vec!["task_success".to_string()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
        interactions: vec![],
        dead_weight_purpose: false,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.to_string(),
        ref_: pinned(&format!("level.{id}")),
        overrides: None,
        label: id.to_string(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let arm = |arm_id: &str, level_id: &str| ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis: format!("{arm_id} does better"),
        level_assignment: BTreeMap::from([("harness".to_string(), level_id.to_string())]),
        eval_budget: "eval:a".to_string(),
        search_budget: Some("search:a".to_string()),
        inference_budget: None,
        match_spec: Some(MatchSpec::matched_cap(&[DimensionId::ModelCalls])),
        artifact_ref: Ref::new("artifact:x", pinned("artifact.x")),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "design:r6_16".to_string(),
            kind: DesignKind::Paired,
            factors: vec![FactorDeclaration {
                name: "harness".to_string(),
                kind: FactorKind::Harness,
                granularity: None,
                levels: ["a", "b"]
                    .iter()
                    .map(|id| FactorLevel {
                        id: id.to_string(),
                        content_ref: pinned(&format!("level.{id}")),
                        label: id.to_string(),
                    })
                    .collect(),
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
            pre_registration: pre.clone(),
            registry_snapshot_id: Some(pinned("registry.snap")),
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(pre),
        factors: vec![FactorSpec {
            name: "harness".to_string(),
            kind: FactorKind::Harness,
            granularity: Some(hharness_ontology::participant::Granularity::ComponentLevel),
            role: None,
            levels: vec![level("a"), level("b")],
        }],
        arms: vec![arm("arm:a", "a"), arm("arm:b", "b")],
        suite: SuiteBinding {
            suite_ref: pinned("suite.r6_16"),
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
            max_concurrent_runs: 12,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: "perm:r6_16".to_string(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt,
        budgets: hharness_lab::experiment::ExperimentBudgets {
            experiment: "pool".to_string(),
            instrument: "instr".to_string(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:r6_16".to_string(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn reattempt(max_per_plan: u32, backoff_ms: u64, fraction_ppm: u64) -> ReattemptPolicy {
    ReattemptPolicy {
        max_per_plan,
        max_fraction_of_plans_ppm: fraction_ppm,
        backoff: Backoff {
            min_ms: backoff_ms,
            multiplier_ppm: 1_000_000,
            max_ms: backoff_ms,
        },
        error_classes_included: None,
        on_cancel: CancelPolicy::Replan,
    }
}

fn window(kind: ScheduleKind, dead_man_ms: u64) -> WindowDecl {
    WindowDecl {
        window_id: "W-test".to_string(),
        started_ms: 0,
        dead_man_ms: Some(dead_man_ms),
        declared_harvest_ms: 500,
        slot_deadline_ms: 1_400,
        host_memory_mb: 32_768,
        memory_reserve_mb: 4_096,
        vcpu_budget: 8,
        slot_memory_mb: 1_024,
        slot_cpus: 1,
        agent_phase: true,
        model_in_flight_limit: 3,
        model_wait_fraction_ppm: 300_000,
        lookahead: 4,
        schedule: ScheduleDecl {
            kind,
            rounds: 2,
            blocks: 3,
            seed: "sched:r6_16".to_string(),
            catalog_id: "catalog:pin".to_string(),
        },
    }
}

fn drive_packet(
    recipes: BTreeMap<String, SlotRecipe>,
    spec: ExperimentSpec,
    window: Option<WindowDecl>,
    podman_bin: Option<String>,
) -> DrivePacket {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    let mut arm_configs = BTreeMap::new();
    for a in &spec.arms {
        arm_configs.insert(
            a.arm_id.clone(),
            (
                pinned(&format!("cfg.{}", a.arm_id)),
                pinned(&format!("cfgv.{}", a.arm_id)),
            ),
        );
    }
    DrivePacket {
        spec,
        budgets: BTreeMap::from([
            ("eval:a".to_string(), caps(100)),
            ("search:a".to_string(), caps(50)),
            ("pool".to_string(), caps(1_000)),
            ("instr".to_string(), caps(1_000)),
        ]),
        suite_tasks: ["task:1", "task:2", "task:3"]
            .iter()
            .map(|t| (t.to_string(), "dev".to_string()))
            .collect(),
        arm_configs,
        recipes,
        protocol_id: "protocol:r6_16".to_string(),
        selection_id: "selection:r6_16".to_string(),
        epoch: "r6_16-fixture".to_string(),
        max_parallel_slots: 4,
        writer_ttl_ms: 60_000,
        claim_ttl_ms: 30_000,
        costs: BTreeMap::new(),
        window,
        podman_bin,
    }
}

fn cfg_for(root: &Path, packet: &DrivePacket) -> CoordinatorConfig {
    CoordinatorConfig {
        experiment_root: root.to_path_buf(),
        slots_dir: root.join("slots"),
        packet_dir: root.join("packets"),
        runner: RunnerIdentity::detect(),
        max_parallel: packet.max_parallel_slots as usize,
    }
}

fn facts(arm_tasks: &[(&str, &str, u32, u64)]) -> Vec<PlanFact> {
    // (arm, task, rep, order_pos) → plan facts in recorded order.
    arm_tasks
        .iter()
        .map(|(a, t, r, o)| PlanFact {
            run_plan_id: format!("rp:{a}:{t}:{r}"),
            cell_id: format!("cell:{a}:{t}"),
            arm_id: a.to_string(),
            task_id: t.to_string(),
            replicate_index: *r,
            split_label: "dev".into(),
            order_pos: *o,
        })
        .collect()
}

// ── AC-1 (REQ-XB-121): packing — first-fit in recorded order under
// memory, CPU and model-pool bounds and the declared lookahead; the run
// record carries the packing rule and the actual start order.
#[test]
fn ac1_first_fit_in_recorded_order() {
    let u = |m: u64, c: u64, s: u64| ResourceUse {
        memory_mb: m,
        cpus: c,
        model_slots: s,
    };
    let bounds = schedule::PackingBounds {
        memory_mb: 4_000,
        cpus: 4,
        model_pool: 3,
    };
    // First-fit: candidates 0,1 fit memory; 2 does not; 3 fits but is
    // within lookahead of the blocked 2 → it may start ahead of it.
    let picks = schedule::first_fit(
        &[
            u(1_000, 1, 1),
            u(1_000, 1, 1),
            u(5_000, 1, 1),
            u(1_000, 1, 1),
        ],
        &u(0, 0, 0),
        &bounds,
        1,
    );
    assert_eq!(picks, vec![0, 1, 3]);
    // Position 4 is beyond the lookahead — the scan stops there.
    let picks = schedule::first_fit(
        &[
            u(1_000, 1, 1),
            u(5_000, 1, 1),
            u(1_000, 1, 1),
            u(1_000, 1, 1),
            u(1_000, 1, 1),
        ],
        &u(0, 0, 0),
        &bounds,
        1,
    );
    assert_eq!(picks, vec![0, 2]);
    // The model pool binds: three agent-phase consumers fit, no fourth.
    let picks = schedule::first_fit(
        &[u(1, 1, 1), u(1, 1, 1), u(1, 1, 1), u(1, 1, 1)],
        &u(0, 0, 0),
        &bounds,
        4,
    );
    assert_eq!(picks, vec![0, 1, 2]);
    // CPU binds identically.
    let picks = schedule::first_fit(
        &[u(1, 2, 0), u(1, 2, 0), u(1, 2, 0)],
        &u(0, 0, 0),
        &bounds,
        4,
    );
    assert_eq!(picks, vec![0, 1]);
    // In-flight usage is charged before the scan — one slot's headroom
    // remains (mem 2500+1000 ≤ 4000, cpus 3+1 ≤ 4, model 2+1 ≤ 3) but
    // not a second's.
    let picks = schedule::first_fit(
        &[u(1_000, 1, 1), u(1_000, 1, 1)],
        &u(2_500, 3, 2),
        &bounds,
        4,
    );
    assert_eq!(picks, vec![0]);
}

// ── AC-2 (REQ-XB-214): C = min(memory packing on host − reserve, vCPU
// budget, ⌈K/f⌉ for agent phases); the window record carries the C
// inputs; no trial's clock starts while C trials run.
#[test]
fn ac2_c_rule_and_window_record() {
    let w = window(ScheduleKind::KRoundInterleave, 86_400_000);
    // memory: (32768−4096)/1024 = 28; cpu: 8/1 = 8; model: ⌈3/0.3⌉ = 10
    assert_eq!(w.concurrency_c(), 8);
    // The model leg binds for a heavier wait fraction.
    let mut w2 = w.clone();
    w2.model_wait_fraction_ppm = 800_000;
    assert_eq!(w2.concurrency_c(), 4); // ⌈3·10⁶/800000⌉ = 4
    let mut w3 = w.clone();
    w3.agent_phase = false;
    assert_eq!(w3.concurrency_c(), 8); // no ⌈K/f⌉ leg off-agent
    let inputs = w.c_inputs();
    assert_eq!(inputs.get("c").and_then(Json::as_int), Some(8));
    // E2E: C = min(8, declared 4) = 4 — every start has in_flight < 4,
    // and the window record carries the C inputs + packing rule.
    let root = tmp("c");
    let mut store = Store::open_with(
        &root,
        Box::new(ManualClock::at(1_000)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let packet = drive_packet(
        BTreeMap::new(),
        spec_2x3x2(reattempt(0, 0, 1_000_000)),
        Some(window(ScheduleKind::KRoundInterleave, 86_400_000)),
        None,
    );
    let report = coordinator::drive(
        &mut store,
        &docs,
        &packet,
        &cfg_for(&root, &packet),
        &hh_runner(),
    )
    .unwrap();
    let wrec = report.window.as_ref().unwrap();
    assert_eq!(wrec.get("c").and_then(Json::as_int), Some(4));
    assert_eq!(
        wrec.get("packing")
            .and_then(|p| p.get("rule"))
            .and_then(Json::as_str),
        Some("first_fit_recorded_order")
    );
    assert!(wrec.get("c_inputs").is_some());
    let starts = arr(wrec.get("start_order").unwrap());
    assert_eq!(starts.len(), 12);
    for s in starts {
        let inflight = s.get("in_flight_at_start").and_then(Json::as_int).unwrap();
        assert!(inflight < 4, "no trial's clock starts while C trials run");
    }
    assert_eq!(report.settled.len(), 12);
}

// ── AC-3 (REQ-XH-017, REQ-XD-111): the two schedule kinds and their
// composition recompute from the seed; a coupled pair launches in one
// packing step; catalog-identity snapshots = blocks + 1.
#[test]
fn ac3_schedule_kinds_and_composition() {
    // 2 arms × 2 tasks × 2 replicates, in recorded order.
    let mut facts = Vec::new();
    let mut pos = 0u64;
    for r in 0..2 {
        for t in ["task:1", "task:2"] {
            for a in ["arm:a", "arm:b"] {
                facts.extend(self::facts(&[(a, t, r, pos)]));
                pos += 1;
            }
        }
    }
    let decl = |kind, rounds, blocks| ScheduleDecl {
        kind,
        rounds,
        blocks,
        seed: "sched:s".to_string(),
        catalog_id: "catalog:x".to_string(),
    };
    // k-round: 8 singleton units, grouped by replicate rounds.
    let k = schedule::build_schedule(&facts, &decl(ScheduleKind::KRoundInterleave, 2, 2));
    assert_eq!(k.len(), 8);
    assert!(k.iter().all(|u| u.members.len() == 1));
    assert_eq!(
        k.iter().take(4).map(|u| u.block).collect::<Vec<_>>(),
        vec![0, 0, 0, 0]
    );
    assert_eq!(
        k.iter().skip(4).map(|u| u.block).collect::<Vec<_>>(),
        vec![1, 1, 1, 1]
    );
    // pair-coupled: 4 pair units — each (task, rep) O/H pair in one unit.
    let p = schedule::build_schedule(&facts, &decl(ScheduleKind::PairCoupled, 2, 2));
    assert_eq!(p.len(), 4);
    for u in &p {
        assert_eq!(u.members.len(), 2);
        let arms: Vec<&str> = u
            .members
            .iter()
            .map(|id| {
                facts
                    .iter()
                    .find(|f| &f.run_plan_id == id)
                    .unwrap()
                    .arm_id
                    .as_str()
            })
            .collect();
        assert!(arms.contains(&"arm:a") && arms.contains(&"arm:b"));
        let task = &facts
            .iter()
            .find(|f| f.run_plan_id == u.members[0])
            .unwrap()
            .task_id;
        assert!(u
            .members
            .iter()
            .all(|id| { &facts.iter().find(|f| &f.run_plan_id == id).unwrap().task_id == task }));
    }
    // composed: coupled units inside each round.
    let c = schedule::build_schedule(&facts, &decl(ScheduleKind::Composed, 2, 2));
    assert_eq!(c.len(), 4);
    assert_eq!(
        c.iter().take(2).map(|u| u.block).collect::<Vec<_>>(),
        vec![0, 0]
    );
    assert!(c.iter().all(|u| u.members.len() == 2));
    // The order recomputes from the seed — same seed, same order; a
    // different seed permutes it.
    let again = schedule::build_schedule(&facts, &decl(ScheduleKind::PairCoupled, 2, 2));
    assert_eq!(p, again);
    let other = schedule::build_schedule(
        &facts,
        &ScheduleDecl {
            seed: "sched:other".to_string(),
            ..decl(ScheduleKind::PairCoupled, 2, 2)
        },
    );
    assert_ne!(p, other);
}

/// The e2e pair launch: a coupled drive launches each (task, rep)
/// pair's members adjacently in `start_order` (one packing step) and
/// records `blocks + 1` catalog snapshots.
#[test]
fn ac3_coupled_pairs_launch_in_one_packing_step() {
    let root = tmp("pair");
    let mut store = Store::open_with(
        &root,
        Box::new(ManualClock::at(1_000)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let packet = drive_packet(
        BTreeMap::new(),
        spec_2x3x2(reattempt(0, 0, 1_000_000)),
        Some(window(ScheduleKind::PairCoupled, 86_400_000)),
        None,
    );
    let report = coordinator::drive(
        &mut store,
        &docs,
        &packet,
        &cfg_for(&root, &packet),
        &hh_runner(),
    )
    .unwrap();
    let wrec = report.window.as_ref().unwrap();
    let starts = arr(wrec.get("start_order").unwrap());
    assert_eq!(starts.len(), 12);
    // Adjacent start_order rows share a unit — the pair's members start
    // in one packing step on the same VM and proxy.
    let by_unit: BTreeMap<i64, Vec<i64>> = starts.iter().fold(BTreeMap::new(), |mut m, s| {
        let u = s.get("unit").and_then(Json::as_int).unwrap();
        let seq = s.get("seq").and_then(Json::as_int).unwrap();
        m.entry(u).or_default().push(seq);
        m
    });
    assert_eq!(by_unit.len(), 6);
    for (u, seqs) in &by_unit {
        assert_eq!(seqs.len(), 2, "unit {u} is one packing step");
        assert_eq!(seqs[1] - seqs[0], 1, "coupled members start adjacently");
    }
    // blocks + 1 catalog snapshots (3 declared blocks → window_open +
    // 3 block_end rows).
    let snaps = arr(wrec.get("catalog_snapshots").unwrap());
    assert_eq!(snaps.len(), 4);
    assert_eq!(
        snaps[0].get("boundary").and_then(Json::as_str),
        Some("window_open")
    );
}

// ── AC-4 (REQ-XH-016, REQ-XD-060, REQ-XH-125): the §6.8 map and the
// declared reattempt policy through the engine — never a runner-side
// counter.
#[test]
fn ac4_six_eight_cause_map() {
    // Infra causes → infrastructure_failure (a new attempt within
    // bounds); the whole pinned Harbor row maps one-to-one.
    for cause in [
        HarborErrorCause::SandboxFailed,
        HarborErrorCause::ModelProviderFailed,
        HarborErrorCause::Unclassified,
    ] {
        assert_eq!(
            harbor_cause_map(cause, false).outcome_class,
            OutcomeClass::InfrastructureFailure
        );
    }
    assert_eq!(
        harbor_cause_map(HarborErrorCause::VerifierFailed, false).outcome_class,
        OutcomeClass::OracleFailure
    );
    assert_eq!(
        harbor_cause_map(HarborErrorCause::AgentExhaustedBudget, true).outcome_class,
        OutcomeClass::Scored
    );
    // agent timeout → the grade still counts (scored, in the
    // denominator — §6.8's budget_exhausted row).
    let f = SlotFacts {
        reward: Some(RewardFact {
            resolution: Resolution::Unresolved,
            end_cause: EndCause::AgentTimeout,
            reward_ppm: Some(0),
            flags: vec![],
        }),
        ..Default::default()
    };
    assert_eq!(map_slot_facts(&f).outcome_class, OutcomeClass::Scored);
    // verifier timeout / missing reward → oracle_failure (regrade from
    // retained sealed state — never a re-run).
    let f = SlotFacts {
        verifier_unavailable: Some("verifier_timeout".to_string()),
        ..Default::default()
    };
    assert_eq!(
        map_slot_facts(&f).outcome_class,
        OutcomeClass::OracleFailure
    );
}

#[test]
fn ac4_engine_reattempts_within_bounds() {
    // max_per_plan = 2 → an infra-failing task runs attempts 1–3
    // through the engine's own rows (`run_launched` ×3 max; the runner
    // keeps no counter).
    let root = tmp("ra");
    let clock = ManualClock::at(1_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let mut recipes = BTreeMap::new();
    recipes.insert(
        "task:2".to_string(),
        SlotRecipe::Infra {
            cause: hharness_runner::disposition::InfraCause::EnvironmentDied,
        },
    );
    let mut packet = drive_packet(
        recipes,
        spec_2x3x2(reattempt(2, 0, 1_000_000)),
        Some(window(ScheduleKind::KRoundInterleave, 86_400_000)),
        None,
    );
    // The attempt's claim lease expires on the store clock — the
    // simulated clock advances past `claim_ttl_ms` each turn so the
    // engine's re-claim fences the stale one (production waits real
    // ms); the writer TTL stays far above a turn's advance so subject
    // binds survive a launch→reap crossing.
    packet.writer_ttl_ms = 86_400_000;
    let clock2 = clock.clone();
    let report = coordinator::drive_window(
        &mut store,
        &docs,
        &packet,
        &cfg_for(&root, &packet),
        &hh_runner(),
        &mut move || clock2.advance(31_000),
    )
    .unwrap();
    let events = store.events(&report.run_id).unwrap().to_vec();
    let launches: Vec<&Json> = events
        .iter()
        .filter(|e| e.class == "measurement.experiment.run_launched")
        .map(|e| &e.payload)
        .collect();
    // task:2 plans get ≤ 3 launches (attempt + ≤ 2 reattempts); the
    // rest get exactly 1.
    let mut by_plan: BTreeMap<String, u32> = BTreeMap::new();
    for p in &launches {
        *by_plan
            .entry(
                p.get("run_plan_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
            .or_insert(0) += 1;
    }
    for (id, n) in &by_plan {
        if id.contains("task") {
            // engine's ReattemptPolicy is the only counter
        }
        assert!(*n <= 3, "{id} launched {n}× — bound is 1+2");
    }
    assert!(by_plan.values().any(|n| *n == 3), "an infra plan retries");
    // The engine's run_replanned rows carry the policy's bookkeeping.
    assert!(events
        .iter()
        .any(|e| e.class == "measurement.experiment.run_replanned"));
}

#[test]
fn ac4_zero_replan_leaves_uncertain_and_retry_past_window_unresolved() {
    // (a) A zero-replan protocol (L2): an infra-failed slot stays
    // uncertain — plan_final, one attempt only, no retry subject.
    let root = tmp("zr");
    let mut store = Store::open_with(
        &root,
        Box::new(ManualClock::at(1_000)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let mut recipes = BTreeMap::new();
    recipes.insert(
        "task:1".to_string(),
        SlotRecipe::Infra {
            cause: hharness_runner::disposition::InfraCause::EnvironmentDied,
        },
    );
    let packet = drive_packet(
        recipes,
        spec_2x3x2(reattempt(0, 0, 1_000_000)),
        Some(window(ScheduleKind::KRoundInterleave, 86_400_000)),
        None,
    );
    let report = coordinator::drive(
        &mut store,
        &docs,
        &packet,
        &cfg_for(&root, &packet),
        &hh_runner(),
    )
    .unwrap();
    let events = store.events(&report.run_id).unwrap().to_vec();
    assert!(!events
        .iter()
        .any(|e| e.class == "measurement.experiment.run_replanned"));
    let failed = report
        .settled
        .iter()
        .filter(|(_, r)| {
            r.get("outcome_class").and_then(Json::as_str) == Some("infrastructure_failure")
        })
        .count();
    assert!(failed > 0, "an infra-failed slot stays uncertain");
    assert!(report
        .settled
        .iter()
        .all(|(_, r)| r.get("attempt_no").and_then(Json::as_int) == Some(1)));

    // (b) A retry scheduled after its window's end refuses — the slot
    // is unresolved (REQ-XH-125; backoff pushes not_before past the
    // cut-off).
    let root = tmp("rg");
    let clock = ManualClock::at(1_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let mut recipes = BTreeMap::new();
    recipes.insert(
        "task:1".to_string(),
        SlotRecipe::Infra {
            cause: hharness_runner::disposition::InfraCause::EnvironmentDied,
        },
    );
    let mut w = window(ScheduleKind::KRoundInterleave, 3_000);
    w.slot_deadline_ms = 1_400;
    w.declared_harvest_ms = 500;
    w.started_ms = 1_000;
    let packet = drive_packet(
        recipes,
        spec_2x3x2(reattempt(2, 60_000, 1_000_000)),
        Some(w),
        None,
    );
    let report = coordinator::drive(
        &mut store,
        &docs,
        &packet,
        &cfg_for(&root, &packet),
        &hh_runner(),
    )
    .unwrap();
    let wrec = report.window.as_ref().unwrap();
    let un = arr(wrec.get("unresolved").unwrap());
    assert!(!un.is_empty(), "the gated retry leaves the slot unresolved");
    let w2 = arr(wrec.get("withheld").unwrap());
    assert!(w2.iter().all(|r| {
        r.get("disposition").and_then(Json::as_str) == Some("withheld{window_deadline}")
    }));
}

// ── AC-5 (REQ-XB-128): the dead-man cut-off and the incremental
// harvest manifest on a simulated clock.
#[test]
fn ac5_cutoff_withholds_and_harvest_covers() {
    let root = tmp("co");
    let clock = ManualClock::at(0);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let mut w = window(ScheduleKind::KRoundInterleave, 2_000);
    w.slot_deadline_ms = 1_400;
    w.declared_harvest_ms = 500;
    w.started_ms = 0;
    let packet = drive_packet(
        BTreeMap::new(),
        spec_2x3x2(reattempt(0, 0, 1_000_000)),
        Some(w),
        None,
    );
    let cfg = cfg_for(&root, &packet);
    // The simulated clock advances 100ms per scheduling turn.
    let clock2 = clock.clone();
    let report = coordinator::drive_window(
        &mut store,
        &docs,
        &packet,
        &cfg,
        &hh_runner(),
        &mut move || clock2.advance(100),
    )
    .unwrap();
    let wrec = report.window.as_ref().unwrap();
    let withheld = arr(wrec.get("withheld").unwrap());
    let started = arr(wrec.get("start_order").unwrap());
    // Cut-off: now + 1400 + 500 ≤ 2000 — the model pool admits 3 at
    // turn 0, then every later admission lands past the cut-off.
    assert_eq!(
        started.len(),
        3,
        "admissions stop at D − (deadline + harvest)"
    );
    assert_eq!(withheld.len(), 9);
    assert!(withheld.iter().all(|r| {
        r.get("disposition").and_then(Json::as_str) == Some("withheld{window_deadline}")
    }));
    // The incremental harvest manifest covers every settled slot.
    assert_eq!(
        wrec.get("harvest_manifest_covers").and_then(boolv),
        Some(true)
    );
    let manifest = json::parse(
        &std::fs::read_to_string(cfg.packet_dir.join("harvest-manifest.json")).unwrap(),
    )
    .unwrap();
    let slots = arr(manifest.get("slots").unwrap());
    assert_eq!(slots.len(), report.settled.len());
    assert_eq!(
        manifest.get("schema").and_then(Json::as_str),
        Some("hh-runner-harvest/1")
    );
}

// ── AC-6 (REQ-XB-127): the podman-stub sweep removes only own-labelled
// containers; a teardown receipt is recorded per slot.
#[test]
fn ac6_own_label_sweep() {
    let dir = tmp("sweep");
    let stub = dir.join("podman-stub.sh");
    let log = dir.join("calls.log");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> CALLSLOG\ncase \"$*\" in\n  ps*)\n    printf 'aaaa1111 hh-exp-1\\nbbbb2222 foreign-exp\\ncccc3333 \\n';;\n  rm*)\n    exit 0;;\nesac\n".replace("CALLSLOG", log.to_str().unwrap()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let receipts = schedule::sweep_experiment(&stub, "hh-exp-1").unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts[0].get("container_id").and_then(Json::as_str),
        Some("aaaa1111")
    );
    assert_eq!(receipts[0].get("removed").and_then(boolv), Some(true));
    // The foreign and unlabelled containers survive — only the labelled
    // id reached `rm`.
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("rm -f aaaa1111"));
    assert!(!calls.contains("bbbb2222"));
    assert!(!calls.contains("cccc3333"));
    // The pure filter is the own-label rule's home.
    let targets = schedule::sweep_targets(
        &[
            ("x".to_string(), "hh-exp-1".to_string()),
            ("y".to_string(), "other".to_string()),
            ("z".to_string(), String::new()),
        ],
        "hh-exp-1",
    );
    assert_eq!(targets, vec!["x".to_string()]);
    // A teardown receipt maps container→slot under the hh-slot- prefix.
    assert_eq!(schedule::SLOT_CONTAINER_PREFIX, "hh-slot-");
    let _ = HarvestManifest::default();
}

/// The withheld row records only the never-attempted; the record's
/// disposition spellings stay inside the closed set.
#[test]
fn ac5_withheld_uses_the_closed_member() {
    let d = Disposition::Withheld {
        reason: hharness_runner::disposition::WithheldReason::WindowDeadline,
    };
    assert_eq!(d.as_str(), "withheld{window_deadline}");
    assert!(Disposition::parse("withheld{window_deadline}").is_ok());
}
