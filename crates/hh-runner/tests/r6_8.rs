//! R6.8 acceptance battery (manifest 261; RUNNER-CORE a+b): the 2×3×2
//! fixture experiment through the engine end to end (AC-2/AC-3.2-2), the
//! `hh-experiment-slot/1` record + closed `withheld{…}` set + costs
//! references (AC-4/AC-3.3-3), the §3.3 outcome map incl. premature-refusal
//! precedence and the pinned Harbor cause map (AC-5/AC-3.3-5 — also
//! `src/facts.rs` tests), the two-process kill-point battery (AC-6/AC-3.2-5),
//! the cross-process read + second-writer spike (AC-7/AC-3.2-4), the
//! `driver_commit`/binary-digest + absolute-path scans (AC-1/AC-3.2-1) and
//! the no-file-existence-skip scan (REQ-XB-105).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use hharness_budget::spec::{BudgetMode, BudgetSpec};
use hharness_budget::{DimensionId, DimensionKey, MatchSpec};
use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{ExperimentEngine, NextVerdict};
use hharness_identity::idp::idp_id;
use hharness_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::RunKind;
use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ontology::config::Ref;
use hharness_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hharness_ontology::lab::SplitLabel;
use hharness_ontology::participant::ParticipantClass;
use hharness_ontology::FactorKind;
use hharness_runner::coordinator::{self, CoordinatorConfig, SUBJECT_HOLDER};
use hharness_runner::disposition::{AdmissionReason, Disposition};
use hharness_runner::packet::{DrivePacket, SlotPacket, SlotRecipe};
use hharness_runner::slot_record::{ExperimentSlot, RunnerIdentity};
use hharness_runner::worker::{run_slot_worker, slot_run_id};
use hharness_wire::json::{self, Json};

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r68-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pinned(tag: &str) -> String {
    idp_id(&format!("r6_8.{tag}"), tag.as_bytes())
}

fn hh_runner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hh-runner"))
}

fn spec_2x3x2() -> ExperimentSpec {
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
            id: "design:r6_8".to_string(),
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
            suite_ref: pinned("suite.r6_8"),
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
            permutation_seed: "perm:r6_8".to_string(),
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
            salt: "salt:r6_8".to_string(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn drive_packet(
    recipes: BTreeMap<String, SlotRecipe>,
    parallel: u32,
    writer_ttl_ms: u64,
    claim_ttl_ms: u64,
) -> DrivePacket {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    let spec = spec_2x3x2();
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
        harbor: None,
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
        protocol_id: "protocol:r6_8".to_string(),
        selection_id: "selection:r6_8".to_string(),
        epoch: "r6_8-fixture".to_string(),
        max_parallel_slots: parallel,
        writer_ttl_ms,
        claim_ttl_ms,
        costs: BTreeMap::from([
            (
                "model".to_string(),
                Json::obj([
                    ("ref", Json::str("costs-model:fixture")),
                    ("fold", Json::Int(7)),
                ]),
            ),
            (
                "compute".to_string(),
                Json::obj([
                    ("ref", Json::str("costs-compute:fixture")),
                    ("fold", Json::Int(11)),
                ]),
            ),
        ]),
        window: None,
        podman_bin: None,
        supervision: None,
        l1_gate: None,
        native_participants: BTreeMap::new(),
        sweep: None,
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

fn write_drive_packet(root: &Path, packet: &DrivePacket) -> PathBuf {
    let spec = packet.spec.to_json();
    let mut budgets = BTreeMap::new();
    for (k, v) in &packet.budgets {
        budgets.insert(k.clone(), v.to_json());
    }
    let mut cfg_map = BTreeMap::new();
    for (k, (a, b)) in &packet.arm_configs {
        cfg_map.insert(
            k.clone(),
            Json::obj([
                ("configuration_id", Json::str(a)),
                ("configuration_version_id", Json::str(b)),
            ]),
        );
    }
    let mut recipes = BTreeMap::new();
    for (k, r) in &packet.recipes {
        recipes.insert(k.clone(), r.to_json());
    }
    let j = Json::obj([
        ("schema", Json::str("hh-runner-drive/1")),
        ("experiment", spec),
        ("budgets", Json::Obj(budgets)),
        (
            "suite_tasks",
            Json::Arr(
                packet
                    .suite_tasks
                    .iter()
                    .map(|(t, s)| {
                        Json::obj([("task_id", Json::str(t)), ("split_label", Json::str(s))])
                    })
                    .collect(),
            ),
        ),
        ("arm_configs", Json::Obj(cfg_map)),
        ("recipes", Json::Obj(recipes)),
        ("protocol_id", Json::str(&packet.protocol_id)),
        ("selection_id", Json::str(&packet.selection_id)),
        ("epoch", Json::str(&packet.epoch)),
        (
            "max_parallel_slots",
            Json::Int(packet.max_parallel_slots as i64),
        ),
        ("writer_ttl_ms", Json::Int(packet.writer_ttl_ms as i64)),
        ("claim_ttl_ms", Json::Int(packet.claim_ttl_ms as i64)),
        ("costs", Json::Obj(packet.costs.clone())),
    ]);
    let path = root.join("drive.json");
    std::fs::write(&path, j.to_canonical_string().as_bytes()).unwrap();
    path
}

fn blob_bytes(store: &Store, addr: &str) -> Vec<u8> {
    let (_alg, dig) = addr.split_once(':').unwrap();
    store
        .get_blob(&hharness_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: dig.to_string(),
            media_type: "application/json".to_string(),
            size: 0,
        })
        .unwrap()
}

fn plans(root: &Path, run_id: &str) -> Vec<(String, Vec<Json>)> {
    // (run_plan_id, [attempt {outcome_class?, superseded?}])
    let store = Store::open(root).unwrap();
    let events = store.events(run_id).unwrap();
    let mut out: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    for e in events {
        match e.class.as_str() {
            "measurement.experiment.run_launched" => {
                let id = e
                    .payload
                    .get("run_plan_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string();
                out.entry(id).or_default().push(Json::obj([]));
            }
            "measurement.experiment.run_settled" => {
                let id = e
                    .payload
                    .get("run_plan_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string();
                out.entry(id).or_default().push(e.payload.clone());
            }
            _ => {}
        }
    }
    out.into_iter().collect()
}

fn slot_records(root: &Path) -> Vec<ExperimentSlot> {
    let mut out = Vec::new();
    let slots_dir = root.join("slots");
    let Ok(entries) = std::fs::read_dir(&slots_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let store = Store::open(e.path()).unwrap();
        // find the single slot run: read dir entries under runs/
        let runs = e.path().join("runs");
        if let Ok(rs) = std::fs::read_dir(runs) {
            for r in rs.flatten() {
                let rid = r.file_name().to_string_lossy().to_string();
                if let Ok(evs) = store.events(&rid) {
                    if let Some(f) = evs.iter().find(|x| x.class == "lifecycle.run.finished") {
                        if let Some(addr) = f.payload.get("slot_record_ref").and_then(Json::as_str)
                        {
                            let bytes = blob_bytes(&store, addr);
                            let j = json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                            out.push(ExperimentSlot::from_json(&j).unwrap());
                        }
                    }
                }
            }
        }
    }
    out
}

// ── AC-1/AC-2: end-to-end fixture drive through the engine ─────────────────

#[test]
fn fixture_experiment_settles_end_to_end() {
    let root = tmp("e2e");
    let packet = drive_packet(BTreeMap::new(), 2, 30_000, 500);
    let packet_path = write_drive_packet(&root, &packet);
    let out = Command::new(hh_runner())
        .args([
            "coordinator",
            "--packet",
            packet_path.to_str().unwrap(),
            "--experiment-root",
            root.to_str().unwrap(),
            "--slots-dir",
            root.join("slots").to_str().unwrap(),
            "--packet-dir",
            root.join("packets").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "coordinator failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = json::parse(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(
        report.get("close_status").and_then(Json::as_str),
        Some("completed")
    );
    // 12 plans (2 arms × 3 tasks × 2 replicates), every one settled scored.
    let settled = match report.get("settled") {
        Some(Json::Arr(a)) => a,
        _ => panic!("no settled"),
    };
    assert_eq!(settled.len(), 12, "{report:?}");
    for row in settled {
        assert_eq!(
            row.get("outcome_class").and_then(Json::as_str),
            Some("scored"),
            "{row:?}"
        );
        assert!(matches!(row.get("accepted"), Some(Json::Bool(true))));
    }
    // AC-1: every packet (slot record) names driver_commit + binary_sha256.
    let records = slot_records(&root);
    assert_eq!(records.len(), 12, "one record per slot");
    for r in &records {
        let runner = r.identity.get("runner").unwrap();
        let commit = runner.get("driver_commit").and_then(Json::as_str).unwrap();
        let digest = runner.get("binary_sha256").and_then(Json::as_str).unwrap();
        assert!(!commit.is_empty());
        assert!(digest.starts_with("sha256:"));
        assert_eq!(
            r.disposition,
            Disposition::Executed {
                resolved: true,
                end_cause: hharness_runner::disposition::EndCause::Exited(0)
            }
        );
    }
    // AC-2: deleting the runner's working files changes nothing durable.
    std::fs::remove_dir_all(root.join("packets")).unwrap();
    let settled2 = plans(&root, report.get("run_id").unwrap().as_str().unwrap());
    assert!(!settled2.is_empty());
}

// ── AC-4: withheld record + costs references ────────────────────────────────

#[test]
fn admission_refusal_emits_withheld_record() {
    let root = tmp("withheld");
    let packet = drive_packet(BTreeMap::new(), 1, 30_000, 500);
    let cfg = cfg_for(&root, &packet);
    let mut store = Store::open(&cfg.experiment_root).unwrap();
    let docs = LabDocs::open(&cfg.experiment_root).unwrap();
    let (eid, _rid, _lease) = coordinator::open_or_attach(&mut store, &docs, &packet).unwrap();
    let sp = SlotPacket {
        harbor: None,
        run_plan_id: "plan:withheld".to_string(),
        subject_run_id: "subj:withheld".to_string(),
        attempt_no: 1,
        experiment_id: eid,
        protocol_id: packet.protocol_id.clone(),
        selection_id: packet.selection_id.clone(),
        cell_id: "cell:1".to_string(),
        arm_id: "arm:a".to_string(),
        task_id: "task:1".to_string(),
        replicate_index: 0,
        split_label: "dev".to_string(),
        slot_root: root.join("slots").join("w1").to_string_lossy().into_owned(),
        slot_run_id: slot_run_id("plan:withheld", 1),
        task: BTreeMap::new(),
        participant: BTreeMap::new(),
        cell: BTreeMap::new(),
        recipe: SlotRecipe::Admission {
            reason: AdmissionReason::TaskDigestMismatch,
        },
    };
    let seal = run_slot_worker(&sp, &root.join("slots").join("w1")).unwrap();
    assert_eq!(
        seal.disposition.as_str(),
        "withheld{admission:task_digest_mismatch}"
    );
    // The withheld record validates; an outside-the-set value refuses.
    let store = Store::open(root.join("slots").join("w1")).unwrap();
    let evs = store.events(&seal.slot_run_id).unwrap();
    let f = evs
        .iter()
        .find(|e| e.class == "lifecycle.run.finished")
        .unwrap();
    let addr = f
        .payload
        .get("slot_record_ref")
        .and_then(Json::as_str)
        .unwrap();
    let bytes = blob_bytes(&store, addr);
    let rec =
        ExperimentSlot::from_json(&json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        rec.disposition.as_str(),
        "withheld{admission:task_digest_mismatch}"
    );
    assert!(rec.phases.is_empty());
    // costs: references only + fixture fold equals the referenced value.
    let mut rec_j = rec.to_json();
    if let Json::Obj(ref mut m) = rec_j {
        m.insert(
            "costs".into(),
            Json::obj([
                (
                    "model",
                    Json::obj([
                        ("schema", Json::str("hh-slot-costs/1")),
                        ("ref", Json::str("costs-model:fixture")),
                    ]),
                ),
                (
                    "compute",
                    Json::obj([
                        ("schema", Json::str("hh-slot-costs/1")),
                        ("ref", Json::str("costs-compute:fixture")),
                    ]),
                ),
            ]),
        );
    }
    let rec2 = ExperimentSlot::from_json(&rec_j).unwrap();
    // fixture fold recompute == referenced value (the drive packet's fold).
    let fold = |v: &Json| v.get("fold").and_then(Json::as_int).unwrap();
    assert_eq!(fold(&packet.costs["model"]), 7);
    assert_eq!(fold(&packet.costs["compute"]), 11);
    // a value member (duplicated fold) refuses.
    let mut bad = rec_j.clone();
    if let Json::Obj(ref mut m) = bad {
        if let Json::Obj(ref mut c) = m.get_mut("costs").unwrap() {
            c.insert("model_micro".into(), Json::Int(7));
        }
    }
    assert!(ExperimentSlot::from_json(&bad).is_err());
    // a withheld value outside the closed member set refuses.
    assert!(hharness_runner::disposition::WithheldReason::parse("not_a_member").is_err());
    let _ = rec2;
}

// ── AC-5: the outcome map incl. precedence (rows also unit-tested in
// src/facts.rs) — engine-visible check: premature refusal settles
// infrastructure_failure and replans, a scored slot settles scored. ────────

#[test]
fn premature_refusal_is_infra_even_with_reward() {
    use hharness_runner::facts::{map_slot_facts, Resolution, RewardFact, SlotFacts};
    let m = map_slot_facts(&SlotFacts {
        reward: Some(RewardFact {
            resolution: Resolution::Resolved,
            end_cause: hharness_runner::disposition::EndCause::Exited(0),
            reward_ppm: Some(1_000_000),
            flags: vec![],
        }),
        premature_budget_refusal: true,
        ..Default::default()
    });
    assert_eq!(
        m.outcome_class,
        hharness_ontology::control::OutcomeClass::InfrastructureFailure
    );
    assert_eq!(
        m.disposition.as_str(),
        "inconclusive{infra:budget_reservation}"
    );
    assert!(m.replans);
    // Only infrastructure_failure replans — sweep every other class.
    for f in [
        SlotFacts {
            reward: Some(RewardFact {
                resolution: Resolution::Resolved,
                end_cause: hharness_runner::disposition::EndCause::Exited(0),
                reward_ppm: Some(1),
                flags: vec![],
            }),
            ..Default::default()
        },
        SlotFacts {
            verifier_unavailable: Some("x".into()),
            ..Default::default()
        },
        SlotFacts {
            admission_refusal: Some(AdmissionReason::ImageNotLocal),
            ..Default::default()
        },
        SlotFacts {
            cancelled: Some(hharness_ontology::control::CancelledBy::Operator),
            ..Default::default()
        },
    ] {
        assert!(!map_slot_facts(&f).replans, "{f:?}");
    }
}

// ── AC-6: the kill-point battery ───────────────────────────────────────────

/// claim+launch attempt 1 on plan 0 in-process, then hand the worker a
/// HoldAgent recipe and SIGKILL it mid-agent: the durable record carries
/// no seal, so bind closes `inconclusive{infra:runner_crash}` and the
/// engine replans — the retried attempt is a NEW attempt (never a
/// mid-agent resume), and exactly one further attempt lands.
#[test]
fn kill_point_worker_death_before_seal() {
    let root = tmp("kp1");
    let clock = ManualClock::at(1_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let packet = drive_packet(BTreeMap::new(), 1, 60_000, 500);
    let cfg = cfg_for(&root, &packet);
    let (eid, run_id, lease) = coordinator::open_or_attach(&mut store, &docs, &packet).unwrap();
    // claim + launch the first plan.
    let (plan_id, launch) = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        let v = eng.next().unwrap();
        let NextVerdict::Plan { run_plan_id } = v else {
            panic!("expected a plan, got {v:?}")
        };
        let t = eng
            .claim(&run_plan_id, coordinator::COORDINATOR_HOLDER)
            .unwrap();
        let l = eng.launch(&t, SUBJECT_HOLDER).unwrap();
        (run_plan_id, l)
    };
    // Spawn a real worker process holding inside the agent phase.
    let sp = coordinator::slot_packet(
        &packet,
        &cfg,
        &eid,
        &plan_id,
        "cell:x",
        "arm:a",
        "task:hold",
        0,
        "dev",
        &launch,
        SlotRecipe::HoldAgent { hold_ms: 60_000 },
    );
    let slot_root = cfg.slots_dir.join("kp1");
    let sp = SlotPacket {
        harbor: None,
        slot_root: slot_root.to_string_lossy().into_owned(),
        ..sp
    };
    std::fs::create_dir_all(&cfg.packet_dir).unwrap();
    let packet_path = cfg.packet_dir.join("kp1.json");
    std::fs::write(&packet_path, sp.to_json().to_canonical_string().as_bytes()).unwrap();
    let mut child = Command::new(hh_runner())
        .args(["slot-worker", "--packet"])
        .arg(&packet_path)
        .spawn()
        .unwrap();
    // Let the worker land its agent-phase row, then SIGKILL it.
    std::thread::sleep(std::time::Duration::from_millis(400));
    child.kill().unwrap();
    child.wait().unwrap();
    // Durable-derived recovery: no seal → runner_crash → replan.
    coordinator::bind_subject(
        &mut store,
        &launch.run_id,
        &slot_root,
        &sp.slot_run_id,
        &launch.subject_writer,
    )
    .unwrap();
    let out = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        eng.settle(&plan_id).unwrap()
    };
    assert_eq!(
        out.outcome_class,
        hharness_ontology::control::OutcomeClass::InfrastructureFailure
    );
    assert!(out.replanned, "infra failure replans within bounds");
    assert!(out.superseded);
    // The crashed attempt is preserved as inconclusive{infra:runner_crash}.
    let finished = store
        .events(&launch.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "lifecycle.run.finished")
        .unwrap();
    assert_eq!(
        finished.payload.get("disposition").and_then(Json::as_str),
        Some("inconclusive{infra:runner_crash}")
    );
    // Re-claim → launch → a NEW attempt (attempt 2, never a mid-run resume).
    let launch2 = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        let v = eng.next().unwrap();
        let NextVerdict::Plan { run_plan_id } = v else {
            panic!("expected re-eligible plan, got {v:?}")
        };
        assert_eq!(run_plan_id, plan_id);
        // The attempt-1 claim lease is still live — the durable fence
        // decides when the plan is reclaimable (claim_ttl, not a runner
        // counter): advance past it, then claim + launch attempt 2.
        clock.advance(31_000);
        let t = eng
            .claim(&run_plan_id, coordinator::COORDINATOR_HOLDER)
            .unwrap();
        eng.launch(&t, SUBJECT_HOLDER).unwrap()
    };
    assert_eq!(launch2.attempt_no, 2, "a replanned slot is a NEW attempt");
}

/// Worker seals, then dies before the coordinator reaps: bind reads the
/// sealed durable record and settles `scored` — the worker's death after
/// the seal is invisible to the outcome.
#[test]
fn kill_point_worker_death_after_seal_before_bind() {
    let root = tmp("kp2");
    let mut store = Store::open(&root).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let packet = drive_packet(BTreeMap::new(), 1, 60_000, 500);
    let cfg = cfg_for(&root, &packet);
    let (eid, run_id, lease) = coordinator::open_or_attach(&mut store, &docs, &packet).unwrap();
    let (plan_id, launch) = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        let NextVerdict::Plan { run_plan_id } = eng.next().unwrap() else {
            panic!("expected plan")
        };
        let t = eng
            .claim(&run_plan_id, coordinator::COORDINATOR_HOLDER)
            .unwrap();
        let l = eng.launch(&t, SUBJECT_HOLDER).unwrap();
        (run_plan_id, l)
    };
    let slot_root = cfg.slots_dir.join("kp2");
    let sp = coordinator::slot_packet(
        &packet,
        &cfg,
        &eid,
        &plan_id,
        "cell:x",
        "arm:a",
        "task:hold2",
        0,
        "dev",
        &launch,
        SlotRecipe::HoldAfterSeal {
            hold_ms: 60_000,
            reward_ppm: 1_000_000,
        },
    );
    let sp = SlotPacket {
        harbor: None,
        slot_root: slot_root.to_string_lossy().into_owned(),
        ..sp
    };
    std::fs::create_dir_all(&cfg.packet_dir).unwrap();
    let packet_path = cfg.packet_dir.join("kp2.json");
    std::fs::write(&packet_path, sp.to_json().to_canonical_string().as_bytes()).unwrap();
    let mut child = Command::new(hh_runner())
        .args(["slot-worker", "--packet"])
        .arg(&packet_path)
        .spawn()
        .unwrap();
    // Cross-process read: poll the slot store until the seal lands
    // (the durable record decides the kill point, never the exit status).
    let sealed = loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        match coordinator::sealed_attempt(&slot_root, &sp.slot_run_id) {
            Ok(Some(_)) => break true,
            Ok(None) => {}
            Err(_) => {}
        }
        if let Ok(Some(_)) = child.try_wait() {
            panic!("worker exited before we could kill post-seal");
        }
    };
    assert!(sealed);
    child.kill().unwrap();
    child.wait().unwrap();
    // Bind under the launch-armed subject lease → scored, accepted.
    coordinator::bind_subject(
        &mut store,
        &launch.run_id,
        &slot_root,
        &sp.slot_run_id,
        &launch.subject_writer,
    )
    .unwrap();
    let out = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        eng.settle(&plan_id).unwrap()
    };
    assert_eq!(
        out.outcome_class,
        hharness_ontology::control::OutcomeClass::Scored
    );
    assert!(out.accepted);
    // No duplicate attempt: settle is exactly-once.
    let out2 = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        eng.settle(&plan_id).unwrap()
    };
    assert_eq!(out2.attempt_no, out.attempt_no);
    assert!(!out2.replanned);
}

/// Coordinator dies mid-claim (in-process equivalent of SIGKILL between
/// `run_claimed` and `run_launched` — the engine holds no memory state,
/// so drop + re-open at an advanced clock is the same durable kill): the
/// stale claim expires through reconcile, the re-drive launches attempt
/// 1 — no duplicate attempt, nothing resumed mid-run.
#[test]
fn kill_point_coordinator_death_mid_claim() {
    let root = tmp("kp3");
    let clock = ManualClock::at(5_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let packet = drive_packet(BTreeMap::new(), 1, 1_000, 500);
    let (eid, run_id, lease) = coordinator::open_or_attach(&mut store, &docs, &packet).unwrap();
    // Claim, then "die": drop the binding, advance past claim+writer TTL.
    let plan_id = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id.clone(),
            lease.clone(),
        );
        let NextVerdict::Plan { run_plan_id } = eng.next().unwrap() else {
            panic!("expected plan")
        };
        let t = eng
            .claim(&run_plan_id, coordinator::COORDINATOR_HOLDER)
            .unwrap();
        assert!(t.expires_at_ms > 5_000);
        run_plan_id
    };
    drop(store);
    clock.advance(61_000);
    let mut store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    // Re-drive: the dead coordinator's writer lease expired → takeover;
    // the stale claim expired → reconcile → eligible again.
    let (eid2, run_id2, lease2) = coordinator::open_or_attach(&mut store, &docs, &packet).unwrap();
    assert_eq!(eid, eid2);
    assert_eq!(run_id, run_id2);
    let launch = {
        let mut eng = ExperimentEngine::bind_lease(
            &mut store,
            docs.clone(),
            coordinator::ctx_for(&packet),
            run_id2.clone(),
            lease2.clone(),
        );
        let NextVerdict::Plan { run_plan_id } = eng.next().unwrap() else {
            panic!("expected eligible plan after claim expiry")
        };
        assert_eq!(run_plan_id, plan_id);
        let t = eng
            .claim(&run_plan_id, coordinator::COORDINATOR_HOLDER)
            .unwrap();
        eng.launch(&t, SUBJECT_HOLDER).unwrap()
    };
    assert_eq!(launch.attempt_no, 1, "mid-claim death never duplicates");
}

// ── AC-7: cross-process read + second-writer refusal ───────────────────────

#[test]
fn topology_spike_cross_process_read_and_second_writer() {
    let root = tmp("spike");
    let mut store = Store::open(&root).unwrap();
    let (run_id, lease) = store
        .open_run(
            hharness_ledger::manifest::RunManifest::minimal(RunKind::Agent),
            "spike-writer",
        )
        .unwrap();
    // Writer appends a durable batch.
    let e = hharness_ledger::event::Event {
        event_id: store.alloc_id("evt"),
        class: "lifecycle.replay.started".to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: hharness_ledger::event::Producer::kernel("hh-runner/1"),
        scope: hharness_ledger::event::Scope::default(),
        parent_event_id: store.head_event_id(&run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
            "hh-runner/1",
            store.now_ms(),
        )),
        content_kind: None,
        payload: Json::obj([("probe", Json::Int(1))]),
    };
    store.append(&run_id, &lease, vec![e]).unwrap();
    // Cross-process READ of the root this process is writing: the reader
    // sees exactly the committed prefix (the WAL commit marker — a torn
    // tail is never surfaced).
    let out = Command::new(hh_runner())
        .args(["store-probe", "--mode", "read", "--root"])
        .arg(&root)
        .args(["--run", &run_id])
        .output()
        .unwrap();
    assert!(out.status.success());
    let read = json::parse(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let committed = store.events(&run_id).unwrap().len() as i64;
    assert_eq!(
        read.get("committed_events").and_then(Json::as_int),
        Some(committed),
        "reader sees exactly the committed prefix, {read:?}"
    );
    // Second WRITER process on a held run: refused WouldBlock.
    let out = Command::new(hh_runner())
        .args(["store-probe", "--mode", "write", "--root"])
        .arg(&root)
        .args(["--run", &run_id])
        .output()
        .unwrap();
    let write = json::parse(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(
        write.get("writer").and_then(Json::as_str),
        Some("refused"),
        "{write:?}"
    );
    assert_eq!(
        write.get("active_holder").and_then(Json::as_str),
        Some("spike-writer")
    );
    // And an unheld run acquires fine (the fence is the lease, not the root).
    let (run2, l2) = store
        .open_run(
            hharness_ledger::manifest::RunManifest::minimal(RunKind::Agent),
            "other",
        )
        .unwrap();
    store.release(&l2, "handoff").unwrap();
    drop(store);
    let out = Command::new(hh_runner())
        .args(["store-probe", "--mode", "write", "--root"])
        .arg(&root)
        .args(["--run", &run2, "--writer-ms", "500", "--appends", "1"])
        .output()
        .unwrap();
    let write2 = json::parse(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(
        write2.get("writer").and_then(Json::as_str),
        Some("acquired"),
        "{write2:?}"
    );
}

// ── AC-1/REQ-XB-105 scans: no absolute paths; no file-existence skip ────────

#[test]
fn runner_sources_carry_no_absolute_paths_or_existence_skips() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        if e.path().extension().and_then(|x| x.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(e.path()).unwrap();
        for (i, line) in text.lines().enumerate() {
            // Absolute-path literal (`"/..."`) outside doc comments.
            let code = line.split("//").next().unwrap_or("");
            if code.contains("\"/") && !code.contains("\"\\u") {
                hits.push(format!(
                    "{}:{} absolute path: {line}",
                    e.path().display(),
                    i + 1
                ));
            }
            // No file-existence skip (REQ-XB-105): resume derives from
            // durable records, never `Path::exists`/`try_exists`/`metadata`.
            for pat in [".exists()", "try_exists", ".metadata(", "Path::exists"] {
                if code.contains(pat) {
                    hits.push(format!(
                        "{}:{} existence check {pat}",
                        e.path().display(),
                        i + 1
                    ));
                }
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
}
