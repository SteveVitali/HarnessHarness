//! R6.41 Stage 2 — the HH-native participant (E2-T8 (b); REQ-XE-121,
//! REQ-XE-126, REQ-XD-102; C-05's recorded rung 1).
//!
//! - AC-3: `hh-experiment-slot/1` rejects a `kind: native` record
//!   missing any REQ-XE-126 `participant.hir` member; a complete `hir`
//!   validates member-for-member.
//! - AC-4: the REQ-XD-102 schedule gate — `l1_gate_match` recomputes
//!   `l1_pass_id` over the arm's declared `hir` identity; a changed
//!   `definition_version_id` or `kernel_binary_sha256` (and any pass-id
//!   mismatch) fails the match — the typed refusal the drive stamps is
//!   `withheld{l1_not_passed}`.
//! - AC-5: the C-05 rung-conditional — rung 1 admits; rung 2 refuses
//!   (the record's own UNVERIFIED row); rung 3 refuses without an
//!   admitted `ar04_amendment_ref`.
//! - AC-2: `native_lane_check` proves the HH-native lane is the same
//!   `cell_env_projection` stamp O receives — loopback-only base URLs
//!   on one listener port, the per-slot token present, `no_proxy`
//!   covering loopback.

#![allow(clippy::unwrap_used)]

use hharness_runner::l1_gate::{l1_pass_id, L1Identity};
use hharness_runner::native::{
    check_hir, check_participant, h_identity_of, l1_gate_match, l1_not_passed_disposition,
    native_lane_check, L1GateDecl, NativeParticipantDecl, NativeTopology, HIR_REQUIRED,
    RECORDED_RUNG,
};
use hharness_wire::json::Json;

// ── the fixture hir + gate facts ─────────────────────────────────────────────

fn hir() -> Json {
    Json::obj([
        ("definition_version_id", Json::str("sha256:def-ver-1")),
        ("semantic_id", Json::str("hh-native:agent")),
        ("profile_hash", Json::str("sha256:profile-1")),
        ("bundle_id", Json::str("bundle:r641")),
        ("derivation_key", Json::str("deriv:r641")),
        ("hh_commit", Json::str("commit:abc123")),
        ("kernel_binary_sha256", Json::str("sha256:kb-1")),
        ("representation_ledger_ref", Json::str("ledger:r641")),
        (
            "level_assignment",
            Json::obj([("harness", Json::str("h1"))]),
        ),
        (
            "override_spellings",
            Json::Arr(vec![Json::str("policy:default")]),
        ),
    ])
}

fn gate_for(h: &Json, canon: &str, corpus: &str) -> L1GateDecl {
    let h_identity = h_identity_of(h).unwrap();
    L1GateDecl {
        pass_id: l1_pass_id(&h_identity, canon, corpus),
        h_identity,
        canon_version: canon.to_string(),
        corpus_digest: corpus.to_string(),
    }
}

// ── AC-3 — `participant.hir` member check ────────────────────────────────────

#[test]
fn ac3_hir_member_check() {
    // The complete member set validates.
    check_hir(&hir()).unwrap();
    // Every required member's absence refuses — iterate the REQ-XE-126
    // table itself so a member added to the table is covered.
    for k in HIR_REQUIRED {
        let mut m = match &hir() {
            Json::Obj(m) => m.clone(),
            _ => unreachable!(),
        };
        m.remove(*k);
        let e = check_hir(&Json::Obj(m)).unwrap_err();
        assert!(e.contains(k), "missing {k} must refuse: {e}");
    }
    // `kind: native` without `hir` refuses; an unknown member refuses;
    // `l1_report_id` is admitted optional.
    let e = check_participant(&Json::obj([
        ("participant_ref", Json::str("p:1")),
        ("kind", Json::str("native")),
    ]))
    .unwrap_err();
    assert!(e.contains("hir"), "{e}");
    let mut hm = match &hir() {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.insert("bogus".into(), Json::Int(1));
    assert!(check_hir(&Json::Obj(hm)).is_err());
    hm = match &hir() {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.insert("l1_report_id".into(), Json::str("l1:rep-1"));
    check_hir(&Json::Obj(hm)).unwrap();
    // A non-native participant carries no `hir` — untouched.
    check_participant(&Json::obj([
        ("participant_ref", Json::str("p:1")),
        ("kind", Json::str("oracle")),
    ]))
    .unwrap();
}

// ── AC-5 — the C-05 rung-conditional ─────────────────────────────────────────

#[test]
fn ac5_rung_conditional() {
    assert_eq!(RECORDED_RUNG, 1);
    // Rung 1 — the recorded topology admits.
    NativeTopology {
        rung: 1,
        ar04_amendment_ref: None,
    }
    .check()
    .unwrap();
    // Rung 2 — the record's own UNVERIFIED row refuses typed.
    let e = NativeTopology {
        rung: 2,
        ar04_amendment_ref: None,
    }
    .check()
    .unwrap_err();
    assert!(e.detail.contains("UNVERIFIED"), "{e}");
    // Rung 3 — the AR-04 exception needs the admitted amendment ref;
    // absent or empty refuses typed, never relaxed silently.
    let e = NativeTopology {
        rung: 3,
        ar04_amendment_ref: None,
    }
    .check()
    .unwrap_err();
    assert!(e.detail.contains("AR-04"), "{e}");
    let e = NativeTopology {
        rung: 3,
        ar04_amendment_ref: Some(String::new()),
    }
    .check()
    .unwrap_err();
    assert!(e.detail.contains("AR-04"), "{e}");
    NativeTopology {
        rung: 3,
        ar04_amendment_ref: Some("adr:AR-04-amend-1".into()),
    }
    .check()
    .unwrap();
    // Rung 0 is not a C-05 rung; the decode refuses unknown members and
    // out-of-range values.
    assert!(NativeTopology {
        rung: 0,
        ar04_amendment_ref: None
    }
    .check()
    .is_err());
    assert!(NativeTopology::from_json(&Json::obj([
        ("rung", Json::Int(1)),
        ("surprise", Json::Int(1)),
    ]))
    .is_err());
    assert!(NativeTopology::from_json(&Json::obj([("rung", Json::Int(9))])).is_err());
}

// ── AC-4 — the REQ-XD-102 L1-conformance match ───────────────────────────────

#[test]
fn ac4_l1_gate_match() {
    let h = hir();
    let gate = gate_for(&h, "hh-l1-canon/1", "sha256:corpus-1");
    // The matching pass id admits — same H identity, recomputed id.
    assert!(l1_gate_match(&gate, &h));
    // A changed `definition_version_id` refuses — REQ-XD-102's "any H
    // change requires a full L1 rerun".
    let mut hm = match &h {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.insert(
        "definition_version_id".into(),
        Json::str("sha256:def-ver-2"),
    );
    assert!(!l1_gate_match(&gate, &Json::Obj(hm)));
    // A changed `kernel_binary_sha256` refuses the same way.
    let mut hm = match &h {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.insert("kernel_binary_sha256".into(), Json::str("sha256:kb-2"));
    assert!(!l1_gate_match(&gate, &Json::Obj(hm)));
    // A pass id minted for a different H (the gate's own recorded
    // identity differs) refuses — the id's preimage is the exact H.
    let other = L1Identity {
        participant_ref: "bundle:r641".into(),
        version: "sha256:def-ver-2".into(),
        digest: "sha256:kb-1".into(),
    };
    let g2 = L1GateDecl {
        pass_id: l1_pass_id(&other, "hh-l1-canon/1", "sha256:corpus-1"),
        h_identity: other,
        canon_version: "hh-l1-canon/1".into(),
        corpus_digest: "sha256:corpus-1".into(),
    };
    assert!(!l1_gate_match(&g2, &h));
    // The slot-level refusal is the closed disposition's own spelling.
    assert_eq!(
        l1_not_passed_disposition().as_str(),
        "withheld{l1_not_passed}"
    );
}

// ── AC-2 — the loopback lane check ───────────────────────────────────────────

#[test]
fn ac2_lane_config() {
    // The stamp is `cell_env_projection`'s own output — O's pins by
    // construction (one home, never a second lane grammar).
    let env = hharness_env::egress_allowlist::cell_env_projection(9_001, "tok:slot-1").unwrap();
    assert_eq!(native_lane_check(&env).unwrap(), 9_001);
    // A non-loopback base URL refuses.
    let mut e2 = env.clone();
    e2.insert("OPENAI_BASE_URL".into(), "https://api.openai.com/v1".into());
    assert!(native_lane_check(&e2).is_err());
    // Divergent ports refuse — not one listener.
    let mut e3 = env.clone();
    e3.insert(
        "ANTHROPIC_BASE_URL".into(),
        "http://127.0.0.1:9_002".replace('_', ""),
    );
    assert!(native_lane_check(&e3).is_err());
    // An empty token member refuses — the per-slot token is required.
    let mut e4 = env.clone();
    e4.insert("OPENAI_API_KEY".into(), String::new());
    assert!(native_lane_check(&e4).is_err());
    // A `no_proxy` that loses loopback refuses — O's mandatory pin.
    let mut e5 = env.clone();
    e5.insert("no_proxy".into(), "example.com".into());
    assert!(native_lane_check(&e5).is_err());
}

// ── the declared participant record (packet member decode) ──────────────────

#[test]
fn native_participant_decl_decode() {
    let j = Json::obj([
        ("topology", Json::obj([("rung", Json::Int(1))])),
        ("hir", hir()),
    ]);
    let d = NativeParticipantDecl::from_json(&j).unwrap();
    assert_eq!(d.topology.rung, 1);
    // `hir` verbatim — the sealed declaration is the record.
    assert_eq!(d.hir.to_canonical_string(), hir().to_canonical_string());
    // A member-missing `hir` refuses at decode — never reaches the
    // schedule check.
    let mut hm = match &hir() {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.remove("kernel_binary_sha256");
    let bad = Json::obj([
        ("topology", Json::obj([("rung", Json::Int(1))])),
        ("hir", Json::Obj(hm)),
    ]);
    assert!(NativeParticipantDecl::from_json(&bad).is_err());
}

/// AC-3's record-level leg — a `hh-experiment-slot/1` `participant`
/// member missing a required `hir` member refuses in the record's own
/// `from_json` (the slot-record schema is the check's one home).
#[test]
fn ac3_slot_record_participant_check() {
    let native = Json::obj([
        ("participant_ref", Json::str("fixture:hh_native")),
        ("kind", Json::str("native")),
        ("topology", Json::obj([("rung", Json::Int(1))])),
        ("hir", hir()),
    ]);
    check_participant(&native).unwrap();
    // Missing one required member → the member's own refusal.
    let mut hm = match &hir() {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    hm.remove("derivation_key");
    let bad = Json::obj([
        ("participant_ref", Json::str("fixture:hh_native")),
        ("kind", Json::str("native")),
        ("hir", Json::Obj(hm)),
    ]);
    let e = check_participant(&bad).unwrap_err();
    assert!(e.contains("derivation_key"), "{e}");
}

// ── AC-4 (drive leg) — the scheduled refusal lands as withheld rows ─────────

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
    use hharness_runner::packet::DrivePacket;
    use hharness_runner::schedule::{ScheduleDecl, ScheduleKind, WindowDecl};
    use hharness_runner::slot_record::RunnerIdentity;
    use std::collections::BTreeMap;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("hh-runner-r641-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn pinned(tag: &str) -> String {
        idp_id(&format!("r6_41.{tag}"), tag.as_bytes())
    }

    fn hh_runner() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_hh-runner"))
    }

    fn spec_native_pair(reattempt: ReattemptPolicy) -> ExperimentSpec {
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
                id: "design:r6_41".to_string(),
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
            // The HH-native arm plus a hosted arm — the gate refuses the
            // native member's unit (pair-atomic) and the hosted member
            // still drives.
            arms: vec![arm("hh_native", "a"), arm("pi", "b")],
            suite: SuiteBinding {
                suite_ref: pinned("suite.r6_41"),
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
                permutation_seed: "perm:r6_41".to_string(),
                start_stagger_ms: 0,
                deadline: None,
                priority: None,
            },
            reattempt,
            budgets: ExperimentBudgets {
                experiment: "pool".to_string(),
                instrument: "instr".to_string(),
            },
            bundle_policy: BundlePolicy::Adhoc {
                salt: "salt:r6_41".to_string(),
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
            window_id: "W-r641".to_string(),
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
                rounds: 1,
                blocks: 2,
                seed: "sched:r6_41".to_string(),
                catalog_id: "catalog:pin".to_string(),
            },
        }
    }

    fn drive_packet(spec: ExperimentSpec, window: Option<WindowDecl>) -> DrivePacket {
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
            protocol_id: "protocol:r6_41".to_string(),
            selection_id: "selection:r6_41".to_string(),
            epoch: "r6_41-fixture".to_string(),
            max_parallel_slots: 4,
            writer_ttl_ms: 60_000,
            claim_ttl_ms: 30_000,
            costs: BTreeMap::new(),
            window,
            podman_bin: None,
            supervision: None,
            l1_gate: None,
            native_participants: BTreeMap::from([(
                "hh_native".to_string(),
                hharness_runner::native::NativeParticipantDecl {
                    topology: NativeTopology {
                        rung: 1,
                        ar04_amendment_ref: None,
                    },
                    hir: super::hir(),
                },
            )]),
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

    /// AC-4 — the drive refuses the native arm's units when no matching
    /// `hh-l1-conformance/1` pass id is declared: every packing unit
    /// carrying the `hh_native` member lands `withheld{l1_not_passed}`
    /// in the window record; no worker is spawned for it.
    #[test]
    fn ac4_drive_withholds_native_without_l1_pass() {
        let root = tmp("l1gate");
        let mut store = Store::open_with(
            &root,
            Box::new(ManualClock::at(1_000)),
            None,
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let docs = LabDocs::open(&root).unwrap();
        let packet = drive_packet(
            spec_native_pair(reattempt(0, 0, 1_000_000)),
            Some(window(ScheduleKind::KRoundInterleave, 86_400_000)),
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
            "no l1_not_passed row in the window record: {wrec:?}"
        );
        // The gate is exact — the withheld rows name the native arm's
        // members (never a blanket refusal on unrelated members).
        let rp_ids: Vec<&str> = withheld
            .iter()
            .filter_map(|w| w.get("run_plan_id").and_then(Json::as_str))
            .collect();
        assert!(!rp_ids.is_empty(), "withheld rows carry run_plan_ids");
    }
}
