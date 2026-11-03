//! R5.41 — deterministic central protocol materialization (REQ-D-007/
//! 101/102/103, REQ-E-011, REQ-B-101/103/106; AC-1 positive
//! materialization, AC-2 typed negatives). Fails-without:
//! `eval::central_protocol` does not exist at the R5.40 base — this
//! file compiles red there. Fixture-only: every input is a supplied
//! external stand-in, never a chosen actual task/seed/quota.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::central_protocol::{
    materialize_protocol, ArmDecl, Exclusion, ProtocolDecl, ProtocolError, Quota, TaskEntry,
    TaskSplit, Warmth, ARM_STRATEGIES, OUTCOME_BLIND_EXCLUSION_BASES, PHASES, PRIMARIES,
    RETRYABLE_CLASSES, ROUNDS, TASKS,
};
use hharness_lab::experiment::{
    Backoff, CancelPolicy, OrderKind, Pool, ReattemptPolicy, SchedulingPolicy,
};
use hharness_wire::Json;

fn obj(j: &Json) -> &BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected object: {j:?}"),
    }
}
fn arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("expected array: {j:?}"),
    }
}

fn scheduling() -> SchedulingPolicy {
    SchedulingPolicy {
        max_concurrent_runs: 2,
        pools: Vec::<Pool>::new(),
        order: OrderKind::InterleavedBlocked,
        permutation_seed: "perm:fixture".into(),
        start_stagger_ms: 0,
        deadline: None,
        priority: None,
    }
}

fn reattempt() -> ReattemptPolicy {
    ReattemptPolicy {
        max_per_plan: 2,
        max_fraction_of_plans_ppm: 100_000,
        backoff: Backoff {
            min_ms: 1_000,
            multiplier_ppm: 2_000_000,
            max_ms: 60_000,
        },
        error_classes_included: Some(RETRYABLE_CLASSES.iter().map(|s| s.to_string()).collect()),
        on_cancel: CancelPolicy::Replan,
    }
}

fn roles() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("subject".to_string(), "fp:subject-a".to_string()),
        (
            "summarizer_profile".to_string(),
            "fp:summarizer-unused".to_string(),
        ),
    ])
}

fn arm(arm_id: &str, strategy: &str) -> ArmDecl {
    ArmDecl {
        arm_id: arm_id.into(),
        strategy: strategy.into(),
        artifact_ref: format!("artifact:{strategy}"),
        roles: roles(),
    }
}

/// The golden declaration: 25 `low` + 20 `high` held-out candidates
/// (quotas 20 + 10 = 30), 6 dev tasks, 2 published exclusions.
fn golden() -> ProtocolDecl {
    let mut tasks: Vec<TaskEntry> = (0..25)
        .map(|i| TaskEntry {
            task_id: format!("task:lo-{i:02}"),
            split: TaskSplit::HeldOut,
            stratum: "low".into(),
        })
        .chain((0..20).map(|i| TaskEntry {
            task_id: format!("task:hi-{i:02}"),
            split: TaskSplit::HeldOut,
            stratum: "high".into(),
        }))
        .chain((0..6).map(|i| TaskEntry {
            task_id: format!("task:dev-{i:02}"),
            split: TaskSplit::Dev,
            stratum: "low".into(),
        }))
        .collect();
    tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    ProtocolDecl {
        suite_ref: "suite:tb2-held-out".into(),
        catalogue_identity: "terminal-bench/2.0".into(),
        catalogue_tasks: 89,
        oracle_ref: "oracle:tb2-upstream-pinned".into(),
        runner_ref: "runner:harbor-original".into(),
        tasks,
        exclusions: vec![
            Exclusion {
                task_id: "task:ex-0".into(),
                basis: OUTCOME_BLIND_EXCLUSION_BASES[0].into(),
            },
            Exclusion {
                task_id: "task:ex-1".into(),
                basis: OUTCOME_BLIND_EXCLUSION_BASES[3].into(),
            },
        ],
        exclusion_publication_ref: "publication:exclusions-pre-outcomes".into(),
        split_assignment_ref: "split:assignment-registered".into(),
        split_predates_access: true,
        quotas: vec![
            Quota {
                stratum: "low".into(),
                required: 20,
            },
            Quota {
                stratum: "high".into(),
                required: 10,
            },
        ],
        seed: "seed:committed-selection".into(),
        arms: vec![
            arm("arm:a", ARM_STRATEGIES[0]),
            arm("arm:b", ARM_STRATEGIES[1]),
        ],
        required_roles: roles(),
        lanes: vec!["lane:0".into(), "lane:1".into()],
        lane_spec_ref: "lane-spec:identical".into(),
        scheduling: scheduling(),
        reattempt: reattempt(),
        warmth: Warmth::ColdStart,
        drift_observed: false,
        support_route: true,
        support_cache: true,
        support_seed: true,
        support_fingerprint: true,
        search_budget: 0,
        task_conditioned: false,
        artificial_pressure: false,
        phases: PHASES
            .iter()
            .map(|p| (p.to_string(), format!("phase:{p}")))
            .collect(),
        family_id: "family:confirmatory-primaries".into(),
        primaries: PRIMARIES.iter().map(|s| s.to_string()).collect(),
        multiplicity_id: "holm:alpha-0.05".into(),
        stops: vec![
            "stop:integrity".into(),
            "stop:drift".into(),
            "stop:ceiling".into(),
        ],
        pilot_labelled_as: None,
        amendment_subjects_bound: None,
    }
}

#[test]
fn ac1_same_inputs_same_materialization() {
    let a = materialize_protocol(&golden()).expect("golden materializes");
    let b = materialize_protocol(&golden()).expect("golden materializes");
    assert_eq!(a, b, "same inputs ⇒ identical record");
    assert_eq!(
        a.to_json().to_canonical_string(),
        b.to_json().to_canonical_string()
    );
    assert!(a.check_id());
    assert!(!a.protocol_id.is_empty());
    // A different committed seed materializes a different protocol —
    // the seed is load-bearing, never decorative.
    let mut d2 = golden();
    d2.seed = "seed:other".into();
    let c = materialize_protocol(&d2).unwrap();
    assert_ne!(a.protocol_id, c.protocol_id);
    assert_ne!(
        a.selection.iter().map(|t| &t.task_id).collect::<Vec<_>>(),
        c.selection.iter().map(|t| &t.task_id).collect::<Vec<_>>()
    );
}

#[test]
fn ac1_selection_outcome_blind_30_no_replacement() {
    let p = materialize_protocol(&golden()).unwrap();
    assert_eq!(p.selection.len(), TASKS as usize);
    let ids: BTreeSet<&str> = p.selection.iter().map(|t| t.task_id.as_str()).collect();
    assert_eq!(ids.len(), TASKS as usize, "no replacement");
    // Every selected task is an eligible held-out member; no dev task
    // or excluded task can enter.
    for t in &p.selection {
        assert!(t.task_id.starts_with("task:lo-") || t.task_id.starts_with("task:hi-"));
        assert!(!t.task_id.starts_with("task:dev-") && !t.task_id.starts_with("task:ex-"));
    }
    // Quotas honored per stratum.
    let low = p.selection.iter().filter(|t| t.stratum == "low").count();
    let high = p.selection.iter().filter(|t| t.stratum == "high").count();
    assert_eq!((low, high), (20, 10));
    // Positions are a permutation of 0..30.
    let mut pos: Vec<u32> = p.selection.iter().map(|t| t.position).collect();
    pos.sort_unstable();
    assert_eq!(pos, (0..TASKS).collect::<Vec<u32>>());
}

#[test]
fn ac1_fair_lane_coin_five_rounds_first_slots() {
    let p = materialize_protocol(&golden()).unwrap();
    assert_eq!(p.schedule.len(), (TASKS * 2 * ROUNDS) as usize);
    let a_id = "arm:a";
    // Five rounds; each (task, arm) once per round; replicate = round;
    // both arms of a task ride the two declared lanes.
    for r in 0..ROUNDS {
        let rows: Vec<_> = p.schedule.iter().filter(|s| s.round == r).collect();
        assert_eq!(rows.len(), (TASKS * 2) as usize);
        let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
        for s in &rows {
            assert!(seen.insert((s.task_id.as_str(), s.arm_id.as_str())));
        }
    }
    for t in &p.selection {
        let lanes: BTreeSet<&str> = t.lane_of_arm.values().map(String::as_str).collect();
        assert_eq!(lanes, BTreeSet::from(["lane:0", "lane:1"]));
        let expect_a = if t.coin == 0 { "lane:0" } else { "lane:1" };
        assert_eq!(t.lane_of_arm[a_id], expect_a);
        // Schedule agrees with the recorded coin.
        let s0 = p
            .schedule
            .iter()
            .find(|s| s.round == 0 && s.task_id == t.task_id && s.arm_id == a_id)
            .unwrap();
        assert_eq!(s0.lane_id, expect_a);
    }
    // The coin is fair across the suite — both orientations occur.
    let heads = p.selection.iter().filter(|t| t.coin == 0).count();
    assert!(heads > 0 && heads < TASKS as usize, "fair coin varies");
    // First slots: exactly one per (task, arm), all round 0.
    let first = p.first_slots();
    assert_eq!(first.len(), (TASKS * 2) as usize);
    for s in p.schedule.iter().filter(|s| s.first) {
        assert_eq!(s.round, 0);
    }
    // Slot refs are unique.
    let refs: BTreeSet<&str> = p.schedule.iter().map(|s| s.slot_ref.as_str()).collect();
    assert_eq!(refs.len(), (TASKS * 2 * ROUNDS) as usize);
}

#[test]
fn ac1_identities_and_counts_recorded() {
    let p = materialize_protocol(&golden()).unwrap();
    let j = p.to_json();
    let m = obj(&j);
    assert_eq!(m["schema"].as_str().unwrap(), "central-protocol/1");
    // Phase/family/stops identities carried verbatim.
    let phases = obj(&m["phases"]);
    assert_eq!(phases.len(), 6);
    for phase in PHASES {
        assert_eq!(phases[phase].as_str().unwrap(), format!("phase:{phase}"));
    }
    let family = obj(&m["family"]);
    assert_eq!(
        family["family_id"].as_str().unwrap(),
        "family:confirmatory-primaries"
    );
    assert_eq!(
        family["multiplicity_id"].as_str().unwrap(),
        "holm:alpha-0.05"
    );
    assert_eq!(arr(&m["stops"]).len(), 3);
    let counts = obj(&m["counts"]);
    assert_eq!(counts["tasks"].as_int().unwrap(), 30);
    assert_eq!(counts["rounds"].as_int().unwrap(), 5);
    assert_eq!(counts["subjects"].as_int().unwrap(), 300);
    assert_eq!(counts["first_slots"].as_int().unwrap(), 60);
    assert_eq!(counts["max_concurrent_subjects"].as_int().unwrap(), 2);
    // S.1 metadata consumed unchanged.
    let md = obj(&m["metadata"]);
    assert_eq!(md["estimator"].as_str().unwrap(), "central_protocol");
    assert_eq!(md["version"].as_str().unwrap(), "central-protocol/1");
    // The protocol hash is the predating commitment — recorded once.
    assert!(p.protocol_id.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn ac1_scheduling_and_retry_validated() {
    // The golden policies already satisfy ≤2 concurrency / interleaved
    // / frozen retries — mutating each refuses (legs in ac2 too).
    let p = materialize_protocol(&golden()).unwrap();
    let j = p.to_json();
    let counts = obj(&obj(&j)["counts"]);
    assert!(counts["max_concurrent_subjects"].as_int().unwrap() <= 2);
}

// ── AC-2 typed negatives ──────────────────────────────────────────

#[test]
fn ac2_outcome_selected_exclusion_refuses() {
    let mut d = golden();
    d.exclusions[0].basis = "outcome_selected".into();
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::OutcomeSelectedExclusion {
            task_id: "task:ex-0".into(),
            basis: "outcome_selected".into()
        })
    );
    // An unknown basis is never admitted either.
    let mut d = golden();
    d.exclusions[0].basis = "task_too_hard".into();
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::OutcomeSelectedExclusion { .. })
    ));
}

#[test]
fn ac2_unpublished_exclusions_refuses() {
    let mut d = golden();
    d.exclusion_publication_ref = String::new();
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::UnpublishedExclusions)
    );
}

#[test]
fn ac2_excluded_task_cannot_be_eligible() {
    let mut d = golden();
    d.tasks.push(TaskEntry {
        task_id: "task:ex-0".into(),
        split: TaskSplit::HeldOut,
        stratum: "low".into(),
    });
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::ExclusionEligible {
            task_id: "task:ex-0".into()
        })
    );
}

#[test]
fn ac2_dev_overlap_and_late_split_refuse() {
    let mut d = golden();
    d.tasks.push(TaskEntry {
        task_id: "task:dev-00".into(),
        split: TaskSplit::HeldOut,
        stratum: "low".into(),
    });
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::DevOverlap {
            task_id: "task:dev-00".into()
        })
    );
    let mut d = golden();
    d.split_predates_access = false;
    assert_eq!(materialize_protocol(&d), Err(ProtocolError::LateSplit));
}

#[test]
fn ac2_insufficient_and_malformed_quotas_refuse() {
    // Drop held-out low candidates below the 20-task quota floor —
    // revision or stop, never replacement or a silent smaller manifest.
    let mut d = golden();
    d.tasks
        .retain(|t| !(t.split == TaskSplit::HeldOut && t.task_id <= "task:lo-05".to_string()));
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::InsufficientHeldOut {
            stratum: "low".into(),
            have: 19,
            need: 20
        })
    );
    // Quotas totaling ≠30 refuse before selection.
    let mut d = golden();
    d.quotas[0].required = 19; // total 29
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::QuotaMismatch {
            declared: 29,
            required: TASKS
        })
    );
    // A held-out task in an undeclared stratum refuses.
    let mut d = golden();
    let i = d
        .tasks
        .iter()
        .position(|t| t.split == TaskSplit::HeldOut)
        .unwrap();
    d.tasks[i].stratum = "undeclared".into();
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::UndeclaredStratum {
            stratum: "undeclared".into()
        })
    );
    // A stratum declared twice would re-rank the same candidates and
    // silently re-select (replacement) — the frozen table refuses.
    let mut d = golden();
    d.quotas[1] = Quota {
        stratum: "low".into(),
        required: 10,
    };
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::DuplicateStratum {
            stratum: "low".into()
        })
    );
}

#[test]
fn ac2_retry_shape_refusals() {
    // Wrong-answer retry — a non-infrastructure class retries.
    let mut d = golden();
    d.reattempt.error_classes_included = Some(vec!["scored".into()]);
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::WrongAnswerRetry {
            class: "scored".into()
        })
    );
    // Unfrozen classes (None = all).
    let mut d = golden();
    d.reattempt.error_classes_included = None;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::UnfrozenRetryClasses)
    );
    // Ceilings.
    let mut d = golden();
    d.reattempt.max_per_plan = 3;
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::RetryCeiling { .. })
    ));
    let mut d = golden();
    d.reattempt.max_fraction_of_plans_ppm = 200_000;
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::RetryCeiling { .. })
    ));
}

#[test]
fn ac2_warmth_drift_support_roles_refuse() {
    let mut d = golden();
    d.warmth = Warmth::Unknown;
    assert_eq!(materialize_protocol(&d), Err(ProtocolError::UnknownWarmth));
    let mut d = golden();
    d.drift_observed = true;
    assert_eq!(materialize_protocol(&d), Err(ProtocolError::Drift));
    let mut d = golden();
    d.support_cache = false;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::SupportMismatch { member: "cache" })
    );
    let mut d = golden();
    d.arms[0]
        .roles
        .insert("subject".into(), "fp:changed".into());
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::RolesChanged {
            arm_id: "arm:a".into()
        })
    );
}

#[test]
fn ac2_changed_protocol_and_pilot_relabel_refuse() {
    let mut d = golden();
    d.amendment_subjects_bound = Some(7);
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::ChangedAfterSubjects { subjects_bound: 7 })
    );
    // Pre-subject revision (0 bound) is the admitted amendment path.
    let mut d = golden();
    d.amendment_subjects_bound = Some(0);
    assert!(materialize_protocol(&d).is_ok());
    let mut d = golden();
    d.pilot_labelled_as = Some("confirmation".into());
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::PilotRelabel {
            label: "confirmation".into()
        })
    );
}

#[test]
fn ac2_arm_set_and_policy_shape_refuse() {
    // Third arm.
    let mut d = golden();
    d.arms.push(arm("arm:c", ARM_STRATEGIES[0]));
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::ArmSet { .. })
    ));
    // A duplicated arm id would collapse `lane_of_arm` to one entry —
    // the arm set must carry two distinct ids.
    let mut d = golden();
    d.arms[1].arm_id = "arm:a".into();
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::ArmSet { .. })
    ));
    // Search budget > 0.
    let mut d = golden();
    d.search_budget = 1;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::PolicySearch { search_budget: 1 })
    );
    let mut d = golden();
    d.task_conditioned = true;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::TaskConditioned)
    );
    let mut d = golden();
    d.artificial_pressure = true;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::ArtificialPressure)
    );
}

#[test]
fn ac2_concurrency_and_order_refuse() {
    let mut d = golden();
    d.scheduling.max_concurrent_runs = 3;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::ConcurrencyCap { declared: 3 })
    );
    let mut d = golden();
    d.scheduling.order = OrderKind::Serial;
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::OrderPolicy {
            declared: "serial".into()
        })
    );
    let mut d = golden();
    d.lanes = vec!["lane:0".into(), "lane:0".into()];
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::ArmConcurrency { .. })
    ));
    let mut d = golden();
    d.lanes = vec!["lane:0".into()];
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::ArmConcurrency { .. })
    ));
}

#[test]
fn ac2_identity_refusals() {
    // Duplicate phase ids.
    let mut d = golden();
    d.phases.insert("pilot".into(), "phase:confirmation".into());
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::PhaseIdentity { .. })
    ));
    // A non-frozen phase key smuggled into the record refuses — the
    // phase set is exactly the six.
    let mut d = golden();
    d.phases.insert("staging".into(), "phase:staging".into());
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::PhaseIdentity { .. })
    ));
    // Primaries differ from the frozen three.
    let mut d = golden();
    d.primaries = vec!["capability.task_success".into()];
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::FamilyMismatch { .. })
    ));
    // Empty stops.
    let mut d = golden();
    d.stops = vec![];
    assert!(matches!(
        materialize_protocol(&d),
        Err(ProtocolError::StopIdentity { .. })
    ));
    // Missing seed.
    let mut d = golden();
    d.seed = String::new();
    assert_eq!(
        materialize_protocol(&d),
        Err(ProtocolError::MissingKey { member: "seed" })
    );
}
