//! R6.43 — the sweep's runner half (D3-c FP; REQ-XD-220's V5 gate,
//! REQ-XE-124's sealed-ref naming, the REQ-XE-125 liveness legs).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hharness_runner::l1_corpus::bm_fixtures;
use hharness_runner::l1_drive::{LiveExec, SequenceReplay, ToyMini};
use hharness_runner::l1_gate::{l1_pass_id, L1Identity};
use hharness_runner::l1_obs::ObsReplay;
use hharness_runner::native::L1GateDecl;
use hharness_runner::schedule::{build_schedule, PlanFact, ScheduleDecl, ScheduleKind};
use hharness_runner::sweep::{
    check_sweep_decl, phase4_cohort_id, sweep_arm_ok, SweepDecl, SWEEP_SCHEMA,
};
use hharness_wire::json::Json;

fn baseline() -> L1Identity {
    L1Identity {
        participant_ref: "bundle:r643".into(),
        version: "sha256:base-def-1".into(),
        digest: "sha256:kb-1".into(),
    }
}

fn decl() -> SweepDecl {
    SweepDecl {
        registry_sha256: "sha256:reg-1".into(),
        level_id: "V5".into(),
        level_name: "rsn-drop".into(),
        factor_id: "transcript_render.assistant_replay.reasoning".into(),
        level_ref: "sha256:v5-level-1".into(),
        baseline: baseline(),
        cohort_id: phase4_cohort_id(&baseline(), "sha256:v5-level-1"),
        departure: hharness_runner::sweep::AR33_DEPARTURE.into(),
        label: hharness_lab::sweep::SWEEP_VARIANT_LABEL.into(),
    }
}

fn hir(level_ref: &str) -> Json {
    Json::obj([
        ("definition_version_id", Json::str(level_ref)),
        ("semantic_id", Json::str("hh-native:agent")),
        ("profile_hash", Json::str("sha256:p")),
        ("bundle_id", Json::str("bundle:r643")),
        ("derivation_key", Json::str("d")),
        ("hh_commit", Json::str("c")),
        ("kernel_binary_sha256", Json::str("sha256:kb-1")),
        ("representation_ledger_ref", Json::str("l")),
        (
            "level_assignment",
            Json::obj([(
                "transcript_render.assistant_replay.reasoning",
                Json::str("V5"),
            )]),
        ),
        (
            "override_spellings",
            Json::Arr(vec![Json::str("reasoning_replay=drop")]),
        ),
    ])
}

fn gate() -> L1GateDecl {
    let h = baseline();
    L1GateDecl {
        pass_id: l1_pass_id(&h, "hh-l1-canon/1", "sha256:corpus-1"),
        h_identity: h,
        canon_version: "hh-l1-canon/1".into(),
        corpus_digest: "sha256:corpus-1".into(),
    }
}

// ── AC-5 — the V5 slot gate (REQ-XD-220) ─────────────────────────────────────

#[test]
fn ac5_v5_slot_gate() {
    let d = decl();
    assert!(check_sweep_decl(&d).is_empty());
    let g = gate();
    let h = hir("sha256:v5-level-1");
    assert!(
        sweep_arm_ok(&d, &g, &h),
        "matching pass + sealed ref admits"
    );

    // No matching L1 pass id for the exact H swept — a pass minted
    // for a different H refuses.
    let other = L1Identity {
        participant_ref: "bundle:r643".into(),
        version: "sha256:base-def-2".into(),
        digest: "sha256:kb-1".into(),
    };
    let g2 = L1GateDecl {
        pass_id: l1_pass_id(&other, "hh-l1-canon/1", "sha256:corpus-1"),
        h_identity: other.clone(),
        canon_version: "hh-l1-canon/1".into(),
        corpus_digest: "sha256:corpus-1".into(),
    };
    assert!(!sweep_arm_ok(&d, &g2, &h), "pass for another H refuses");

    // A slot naming a different document refuses.
    assert!(!sweep_arm_ok(&d, &g, &hir("sha256:v5-level-2")));
    // A slot whose level_assignment doesn't name the factor refuses.
    let mut hm = match &h {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.insert("level_assignment".into(), Json::obj([]));
    assert!(!sweep_arm_ok(&d, &g, &Json::Obj(hm)));

    // An H change yields a new phase-4 cohort id.
    let c1 = phase4_cohort_id(&baseline(), "sha256:v5-level-1");
    let c2 = phase4_cohort_id(&other, "sha256:v5-level-1");
    assert_ne!(c1, c2);

    // The AR-33 citation and the verbatim label are required members.
    let mut d2 = d.clone();
    d2.departure = "concurrent".into();
    assert!(!check_sweep_decl(&d2).is_empty());
    let mut d3 = d.clone();
    d3.label = "improved reasoning".into();
    assert!(!check_sweep_decl(&d3).is_empty());
    let mut d4 = d.clone();
    d4.cohort_id = "p4-cohort:sha256:wrong".into();
    assert!(!check_sweep_decl(&d4).is_empty());

    // Strict decode — unknown members refuse; every member required.
    let j = Json::obj([
        ("schema", Json::str(SWEEP_SCHEMA)),
        ("registry_sha256", Json::str("sha256:r")),
        ("level_id", Json::str("V5")),
        ("level_name", Json::str("rsn-drop")),
        (
            "factor_id",
            Json::str("transcript_render.assistant_replay.reasoning"),
        ),
        ("level_ref", Json::str("sha256:v5-level-1")),
        (
            "baseline",
            Json::obj([
                ("participant_ref", Json::str("b")),
                ("version", Json::str("v")),
                ("digest", Json::str("d")),
            ]),
        ),
        ("cohort_id", Json::str("c")),
        ("departure", Json::str("AR-33 x")),
        ("label", Json::str("l")),
    ]);
    SweepDecl::from_json(&j).unwrap();
    let mut jm = match &j {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    jm.insert("bogus".into(), Json::Int(1));
    assert!(SweepDecl::from_json(&Json::Obj(jm)).is_err());
}

// ── AC-5 — the seeded interleave binds V5 into W4 ────────────────────────────

#[test]
fn ac5_w4_interleave_names_v5_document() {
    // The declared W4 seed and 5 rounds over the five arms × 10 tasks
    // × k=5 (recorded-order positions): V5's slots interleave in the
    // one seeded schedule (REQ-XH-111).
    let mut facts = Vec::new();
    let mut pos = 0u64;
    let arms = ["mini-swe-agent", "pi", "claude-code", "hh_native", "v5"];
    for rep in 0..5u32 {
        for t in 0..10 {
            for a in arms {
                facts.push(PlanFact {
                    run_plan_id: format!("rp.{a}.{t}.{rep}"),
                    cell_id: format!("c.{a}.{t}"),
                    arm_id: a.to_string(),
                    task_id: format!("task:{t}"),
                    replicate_index: rep,
                    split_label: "held_out".to_string(),
                    order_pos: pos,
                });
                pos += 1;
            }
        }
    }
    let sched = ScheduleDecl {
        kind: ScheduleKind::Composed,
        rounds: 5,
        blocks: 5,
        seed: "fp-w4-schedule-r6.33-2026-11-10".to_string(),
        catalog_id: "catalog:pin".to_string(),
    };
    let units = build_schedule(&facts, &sched);
    // Every round carries V5 members (interleaved), and every unit is
    // seeded-order recomputed.
    for block in 0..5 {
        let v5_units: Vec<_> = units
            .iter()
            .filter(|u| u.block == block)
            .filter(|u| {
                u.members.iter().any(|m| {
                    facts
                        .iter()
                        .find(|f| &f.run_plan_id == m)
                        .is_some_and(|f| f.arm_id == "v5")
                })
            })
            .collect();
        assert!(!v5_units.is_empty(), "round {block} carries V5 slots");
    }
    // Deterministic — the same seed recomputes the same order.
    let again = build_schedule(&facts, &sched);
    assert_eq!(
        units.iter().map(|u| u.members.clone()).collect::<Vec<_>>(),
        again.iter().map(|u| u.members.clone()).collect::<Vec<_>>()
    );
    // Every V5 slot names its document via participant.hir
    // level_assignment — the sealed level ref, never an override.
    let h = hir("sha256:v5-level-1");
    assert_eq!(
        h.get("definition_version_id").and_then(Json::as_str),
        Some("sha256:v5-level-1")
    );
}

// ── AC-2 — the L1.1 request-diff and executed legs over the BM corpus ────────

#[test]
fn ac2_l11_diff_and_executed_leg() {
    // Drive baseline and V5 toys over the BM18 recording (reasoning
    // present) — the wire diff is confined to `reasoning_content` and
    // non-empty after turn 1.
    // A two-turn sequence: bm18's turn-1 response carries
    // `reasoning_content`; bm19's turn-2 submit ends it. The baseline
    // replays the reasoning member; the V5 drive drops it — the wire
    // diff lands at turn 2 (the declared after-turn-1 branch).
    let fxs = bm_fixtures();
    let bm18 = fxs.iter().find(|f| f.id == "bm18").expect("bm18");
    let bm19 = fxs.iter().find(|f| f.id == "bm19").expect("bm19");
    let calls = vec![
        bm18.recording.calls[0].clone(),
        bm19.recording.calls[1].clone(),
    ];
    let obs = |_: usize| {
        ObsReplay::new(vec![hharness_runner::l1_obs::RawObservation {
            output: "hi".to_string(),
            returncode: 0,
            exception_info: None,
        }])
    };
    let mut replay = SequenceReplay::new(calls.clone());
    let mut toy_b = ToyMini::new("h:baseline", "instance", Box::new(obs(0)));
    let db = toy_b.drive(&mut replay);
    let mut replay2 = SequenceReplay::new(calls);
    let mut toy_v = ToyMini::new("h:v5", "instance", Box::new(obs(1)));
    toy_v.replay_reasoning = false;
    let dv = toy_v.drive(&mut replay2);
    assert_eq!(db.requests.len(), dv.requests.len());
    let diff = hharness_lab::sweep::request_diff(
        &db.requests
            .iter()
            .map(|r| r.upstream_body.clone())
            .collect::<Vec<_>>(),
        &dv.requests
            .iter()
            .map(|r| r.upstream_body.clone())
            .collect::<Vec<_>>(),
    );
    let live = hharness_lab::sweep::v5_liveness();
    hharness_lab::sweep::check_request_diff(
        &live,
        "transcript_render.assistant_replay.reasoning",
        &diff,
    )
    .unwrap();

    // The `cmd-120` executed leg at fixture scale — the declared
    // deadline kills (rc −1 + the timeout text), the widened window
    // completes (rc 0). The production leg is the same check at the
    // declared 30 s/120 s/45 s values; $0 model spend.
    let mut short = LiveExec {
        timeout_ms: Some(60),
    };
    let killed = hharness_runner::l1_obs::L1Exec::run(&mut short, "sleep 1").unwrap();
    let mut wide = LiveExec {
        timeout_ms: Some(5_000),
    };
    let done = hharness_runner::l1_obs::L1Exec::run(&mut wide, "echo ok").unwrap();
    assert_eq!(killed.returncode, -1);
    assert!(killed.exception_info.is_some());
    assert_eq!(done.returncode, 0);
    hharness_lab::sweep::check_executed_leg(
        "exec_defaults.timeout_ms",
        killed.returncode,
        done.returncode,
    )
    .unwrap();
}

// ── AC-5 — the drive withholds V5 without the matching pass ──────────────────

mod drive {
    use super::*;
    use std::path::{Path, PathBuf};

    use hharness_budget::spec::{BudgetMode, BudgetSpec};
    use hharness_budget::{DimensionId, DimensionKey, MatchSpec};
    use hharness_experiment::docs::LabDocs;
    use hharness_identity::idp::idp_id;
    use hharness_lab::experiment::{
        ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
        ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
        SuiteBinding, ValidationStrategy,
    };
    use hharness_ledger::ids::ManualClock;
    use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
    use hharness_ontology::config::Ref;
    use hharness_ontology::eval::{
        Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration,
        RoutingPolicy, SeedPolicy,
    };
    use hharness_ontology::lab::SplitLabel;
    use hharness_ontology::participant::ParticipantClass;
    use hharness_ontology::FactorKind;
    use hharness_runner::coordinator::{self, CoordinatorConfig};
    use hharness_runner::json_ext::JsonExt;
    use hharness_runner::native::{NativeParticipantDecl, NativeTopology};
    use hharness_runner::packet::DrivePacket;
    use hharness_runner::schedule::{ScheduleDecl, ScheduleKind, WindowDecl};
    use hharness_runner::slot_record::RunnerIdentity;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("hh-runner-r643-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn pinned(tag: &str) -> String {
        idp_id(&format!("r6_43.{tag}"), tag.as_bytes())
    }
    fn hh_runner() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_hh-runner"))
    }

    fn spec_v5() -> ExperimentSpec {
        let pre = PreRegistration {
            registered_at: 1,
            hypothesis: "fixture".to_string(),
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
            hypothesis: format!("{arm_id}"),
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
                id: "design:r6_43".to_string(),
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
                replicates_per_cell: 1,
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
            arms: vec![arm("v5", "a"), arm("pi", "b")],
            suite: SuiteBinding {
                suite_ref: pinned("suite.r6_43"),
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
                permutation_seed: "perm:r6_43".to_string(),
                start_stagger_ms: 0,
                deadline: None,
                priority: None,
            },
            reattempt: ReattemptPolicy {
                max_per_plan: 0,
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
                experiment: "pool".to_string(),
                instrument: "instr".to_string(),
            },
            bundle_policy: BundlePolicy::Adhoc {
                salt: "salt:r6_43".to_string(),
            },
            ext: BTreeMap::new(),
        };
        s.experiment_id = s.experiment_id();
        s
    }

    fn packet(
        spec: ExperimentSpec,
        sweep: Option<SweepDecl>,
        gate: Option<L1GateDecl>,
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
            harbor: None,
            spec,
            budgets: BTreeMap::from([
                ("eval:a".to_string(), caps(100)),
                ("search:a".to_string(), caps(50)),
                ("pool".to_string(), caps(1_000)),
                ("instr".to_string(), caps(1_000)),
            ]),
            suite_tasks: ["task:1", "task:2"]
                .iter()
                .map(|t| (t.to_string(), "dev".to_string()))
                .collect(),
            arm_configs,
            recipes: BTreeMap::new(),
            protocol_id: "protocol:r6_43".to_string(),
            selection_id: "selection:r6_43".to_string(),
            epoch: "r6_43-fixture".to_string(),
            max_parallel_slots: 4,
            writer_ttl_ms: 60_000,
            claim_ttl_ms: 30_000,
            costs: BTreeMap::new(),
            window: Some(WindowDecl {
                window_id: "W4".to_string(),
                started_ms: 0,
                dead_man_ms: Some(86_400_000),
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
                    kind: ScheduleKind::Composed,
                    rounds: 1,
                    blocks: 2,
                    seed: "fp-w4-schedule-r6.33-2026-11-10".to_string(),
                    catalog_id: "catalog:pin".to_string(),
                },
            }),
            podman_bin: None,
            supervision: None,
            l1_gate: gate,
            native_participants: BTreeMap::from([(
                "v5".to_string(),
                NativeParticipantDecl {
                    topology: NativeTopology {
                        rung: 1,
                        ar04_amendment_ref: None,
                    },
                    hir: super::hir("sha256:v5-level-1"),
                },
            )]),
            sweep,
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

    /// AC-5 — the runner refuses V5 slots without a matching L1 pass
    /// id for the exact H swept (REQ-XD-220): every packing unit
    /// carrying a `v5` member lands `withheld{l1_not_passed}`.
    #[test]
    fn ac5_drive_withholds_v5_without_matching_pass() {
        let root = tmp("v5gate");
        let mut store = Store::open_with(
            &root,
            Box::new(ManualClock::at(1_000)),
            None,
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let docs = LabDocs::open(&root).unwrap();
        // The pass binds a different H — not the exact one swept.
        let other = super::L1Identity {
            participant_ref: "bundle:r643".into(),
            version: "sha256:base-def-2".into(),
            digest: "sha256:kb-1".into(),
        };
        let g = L1GateDecl {
            pass_id: l1_pass_id(&other, "hh-l1-canon/1", "sha256:corpus-1"),
            h_identity: other.clone(),
            canon_version: "hh-l1-canon/1".into(),
            corpus_digest: "sha256:corpus-1".into(),
        };
        let packet = packet(spec_v5(), Some(super::decl()), Some(g));
        let report = coordinator::drive(
            &mut store,
            &docs,
            &packet,
            &cfg_for(&root, &packet),
            &hh_runner(),
        )
        .unwrap();
        let wrec = report.window.as_ref().unwrap();
        let withheld: Vec<&Json> = wrec
            .get("withheld")
            .and_then(Json::as_arr)
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        assert!(
            withheld
                .iter()
                .any(|w| w.get("disposition").and_then(Json::as_str)
                    == Some("withheld{l1_not_passed}")),
            "no l1_not_passed row: {wrec:?}"
        );
    }

    /// The matching pass + complete sweep decl admits the V5 arm's
    /// slots (they drive the fixture recipe and settle).
    #[test]
    fn ac5_matching_pass_admits() {
        let root = tmp("v5ok");
        let mut store = Store::open_with(
            &root,
            Box::new(ManualClock::at(1_000)),
            None,
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let docs = LabDocs::open(&root).unwrap();
        let packet = packet(spec_v5(), Some(super::decl()), Some(super::gate()));
        let report = coordinator::drive(
            &mut store,
            &docs,
            &packet,
            &cfg_for(&root, &packet),
            &hh_runner(),
        )
        .unwrap();
        let wrec = report.window.as_ref().unwrap();
        let withheld: Vec<&Json> = wrec
            .get("withheld")
            .and_then(Json::as_arr)
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        assert!(
            !withheld
                .iter()
                .any(|w| w.get("disposition").and_then(Json::as_str)
                    == Some("withheld{l1_not_passed}")),
            "matching pass must admit: {wrec:?}"
        );
    }
}
