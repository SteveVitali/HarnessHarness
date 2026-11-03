//! R5.15 — the planned-slot population / metadata contract battery
//! (REQ-D-001/002/003/104, REQ-A-004/102; dossier S.1). Fails-without:
//! `eval::population` does not exist at the R5.14b base — this file
//! compiles red there.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::population::{
    assemble, contrast_bounds, Availability, BoundsDecl, PlannedSlot, PopulationError,
    PopulationInput, PopulationKind, SlotSubject,
};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

fn sub(run: &str, no: u32, oc: Option<OutcomeClass>, ok: Option<bool>) -> SlotSubject {
    SlotSubject {
        subject_run: run.into(),
        attempt_no: no,
        outcome: oc,
        success: ok,
        veto_tripped: vec![],
        availability: if oc.is_some() {
            Availability::Resolved
        } else {
            Availability::NotRun
        },
        seq: 0,
        missing_required_checks: false,
        admissible: true,
    }
}

fn slot(plan: &str, task: &str, arm: &str, subjects: Vec<SlotSubject>) -> PlannedSlot {
    PlannedSlot {
        plan_ref: plan.into(),
        task_id: task.into(),
        split_label: "held_out".into(),
        stratum: "private_held_out".into(),
        arm_id: arm.into(),
        replicate_index: 0,
        subjects,
    }
}

fn input(slots: Vec<PlannedSlot>) -> PopulationInput {
    PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    }
}

/// D1 counterexample: per task, arm A = 5 passes + 45 unknown, arm B =
/// 25 passes + 25 failures over M=50 planned first slots.
fn d1_pop(tasks: usize) -> PlannedSlotBuilder {
    let mut slots = Vec::new();
    for t in 0..tasks {
        let task = format!("t{t}");
        // A: 5 resolved passes + 45 unresolved (not-run).
        for i in 0..50 {
            let s = if i < 5 {
                sub(
                    &format!("a-{task}-{i}"),
                    1,
                    Some(OutcomeClass::Scored),
                    Some(true),
                )
            } else {
                sub(&format!("a-{task}-{i}"), 1, None, None)
            };
            slots.push(slot(&format!("pa-{task}-{i}"), &task, "A", vec![s]));
        }
        // B: 25 resolved passes + 25 resolved failures.
        for i in 0..50 {
            let ok = i < 25;
            slots.push(slot(
                &format!("pb-{task}-{i}"),
                &task,
                "B",
                vec![sub(
                    &format!("b-{task}-{i}"),
                    1,
                    Some(OutcomeClass::Scored),
                    Some(ok),
                )],
            ));
        }
    }
    PlannedSlotBuilder { slots }
}

struct PlannedSlotBuilder {
    slots: Vec<PlannedSlot>,
}

impl PlannedSlotBuilder {
    fn pop(self) -> hharness_eval::population::PlannedPopulation {
        assemble(input(self.slots)).expect("d1 assembles")
    }
}

#[test]
fn d1_golden_bounds_and_explicit_b_minus_a() {
    let pop = d1_pop(2).pop();
    // The §3.2 planned contrast: per-task A = [0.1, 1.0], B = [0.5, 0.5].
    let ab = contrast_bounds(&pop, PopulationKind::Raw, "A", "B", &BoundsDecl::Unit).unwrap();
    assert_eq!((ab.lo_ppm, ab.hi_ppm), (-400_000, 500_000));
    assert_eq!(ab.direction, "test_minus_control");
    assert!(!ab.full);
    // The explicit B−A target (the protocol's new-policy-minus-baseline).
    let ba = contrast_bounds(&pop, PopulationKind::Raw, "B", "A", &BoundsDecl::Unit).unwrap();
    assert_eq!((ba.lo_ppm, ba.hi_ppm), (-500_000, 400_000));
    assert_eq!(ba.metadata.keys["test_arm"], "B");
    // Whole unrun tasks retain weight: a third all-unrun task widens to
    // per-task contributions [(−.5,+.9),(−.5,+.9),(−1,+1)]/3.
    let mut b3 = d1_pop(2);
    for i in 0..50 {
        b3.slots.push(slot(
            &format!("pc-t2-{i}"),
            "t2",
            "A",
            vec![sub(&format!("c-t2-{i}"), 1, None, None)],
        ));
    }
    let pop3 = b3.pop();
    let w = contrast_bounds(&pop3, PopulationKind::Raw, "A", "B", &BoundsDecl::Unit);
    assert!(matches!(w, Err(PopulationError::UnpairedTask { .. })));
}

#[test]
fn unrun_tasks_retain_weight() {
    // One task all-unrun on both arms: cell [0,1] each → contrast [−1,+1].
    let mut slots = Vec::new();
    for arm in ["A", "B"] {
        slots.push(slot(
            &format!("p-{arm}"),
            "t0",
            arm,
            vec![sub(&format!("r-{arm}"), 1, None, None)],
        ));
    }
    let pop = assemble(input(slots)).unwrap();
    let b = contrast_bounds(&pop, PopulationKind::Raw, "A", "B", &BoundsDecl::Unit).unwrap();
    assert_eq!((b.lo_ppm, b.hi_ppm), (-1_000_000, 1_000_000));
    assert!(!b.full);
    // A fully resolved population collapses the bound to the point.
    let mut slots2 = Vec::new();
    for (arm, ok) in [("A", true), ("B", false)] {
        slots2.push(slot(
            &format!("q-{arm}"),
            "t0",
            arm,
            vec![sub(
                &format!("s-{arm}"),
                1,
                Some(OutcomeClass::Scored),
                Some(ok),
            )],
        ));
    }
    let pop2 = assemble(input(slots2)).unwrap();
    assert!(pop2.complete);
    let b2 = contrast_bounds(&pop2, PopulationKind::Raw, "A", "B", &BoundsDecl::Unit).unwrap();
    assert_eq!((b2.lo_ppm, b2.hi_ppm), (1_000_000, 1_000_000));
    assert!(b2.full);
}

#[test]
fn three_named_populations_veto_and_checks() {
    let vetoed_pass = SlotSubject {
        veto_tripped: vec!["v-1".into()],
        ..sub("r1", 1, Some(OutcomeClass::Scored), Some(true))
    };
    let missing_check = SlotSubject {
        missing_required_checks: true,
        ..sub("r2", 1, Some(OutcomeClass::Scored), Some(true))
    };
    let slots = vec![
        slot("p1", "t0", "A", vec![vetoed_pass]),
        slot("p2", "t0", "A", vec![missing_check]),
        slot(
            "p3",
            "t0",
            "A",
            vec![sub("r3", 1, Some(OutcomeClass::OracleFailure), None)],
        ),
    ];
    let pop = assemble(input(slots)).unwrap();
    let raw = &pop.populations["raw"];
    assert_eq!((raw.s, raw.f, raw.u), (2, 0, 1));
    let canon = &pop.populations["canonical"];
    assert_eq!((canon.s, canon.f, canon.u), (1, 0, 2));
    assert_eq!(canon.vetoed_passes, 1);
    let joint = &pop.populations["joint_task_pass_without_observed_veto"];
    assert_eq!((joint.s, joint.f, joint.u), (0, 1, 2));
    // The six-class axis counts the resolved members beside.
    assert_eq!(pop.first_class_counts["scored"], 2);
    assert_eq!(pop.first_class_counts["oracle_failure"], 1);
}

#[test]
fn first_subject_primary_retry_recovery_and_all_attempts() {
    // First subject oracle-failed → the first admissible completed retry
    // reads the slot (recovered); a wrong answer never recovers.
    let recovered = vec![
        sub("r-fail", 1, Some(OutcomeClass::OracleFailure), None),
        sub("r-retry", 2, Some(OutcomeClass::Scored), Some(true)),
    ];
    let wrong = vec![
        sub("r-wrong", 1, Some(OutcomeClass::Scored), Some(false)),
        sub("r-later", 2, Some(OutcomeClass::Scored), Some(true)),
    ];
    let slots = vec![
        slot("p1", "t0", "A", recovered),
        slot("p2", "t0", "A", wrong),
    ];
    let pop = assemble(input(slots)).unwrap();
    assert_eq!(pop.slots[0].member.as_deref(), Some("r-retry"));
    assert!(pop.slots[0].recovered);
    assert_eq!(pop.slots[0].n_attempts, 2);
    assert_eq!(pop.slots[1].member.as_deref(), Some("r-wrong"));
    assert!(!pop.slots[1].recovered);
    assert_eq!(pop.n_slots, 2);
    assert_eq!(pop.n_attempts, 4);
    // M counts slots: the recovered slot is a success, the wrong-answer
    // slot is a failure — retries add no independent n.
    let raw = &pop.populations["raw"];
    assert_eq!((raw.s, raw.f, raw.u), (1, 1, 0));
}

#[test]
fn duplicate_missing_and_conflicting_keys_refuse() {
    // Same subject on two plans.
    let dup = vec![
        slot(
            "p1",
            "t0",
            "A",
            vec![sub("r1", 1, Some(OutcomeClass::Scored), Some(true))],
        ),
        slot(
            "p2",
            "t0",
            "A",
            vec![sub("r1", 1, Some(OutcomeClass::Scored), Some(true))],
        ),
    ];
    assert!(matches!(
        assemble(input(dup)),
        Err(PopulationError::DuplicateSubject { .. })
    ));
    // Empty subject_run / task_id.
    let mut bad = vec![slot(
        "p1",
        "t0",
        "A",
        vec![sub("", 1, Some(OutcomeClass::Scored), Some(true))],
    )];
    assert!(matches!(
        assemble(input(bad.clone())),
        Err(PopulationError::MissingSubjectKey { .. })
    ));
    bad[0].task_id.clear();
    bad[0].subjects[0].subject_run = "r1".into();
    assert!(matches!(
        assemble(input(bad)),
        Err(PopulationError::MissingKey { .. })
    ));
    // Duplicate attempt ordinals on one plan.
    let conflict = vec![slot(
        "p1",
        "t0",
        "A",
        vec![
            sub("r1", 1, Some(OutcomeClass::Scored), Some(true)),
            sub("r2", 1, Some(OutcomeClass::Scored), Some(true)),
        ],
    )];
    assert!(matches!(
        assemble(input(conflict)),
        Err(PopulationError::ConflictingAttempt { .. })
    ));
}

#[test]
fn incomplete_required_universe_refuses() {
    let mut inp = input(vec![slot(
        "p1",
        "t0",
        "A",
        vec![sub("r1", 1, Some(OutcomeClass::Scored), Some(true))],
    )]);
    inp.required = ["p1".to_string(), "p2".to_string()].into_iter().collect();
    match assemble(inp) {
        Err(PopulationError::IncompleteUniverse { missing }) => {
            assert_eq!(missing, vec!["p2".to_string()])
        }
        _ => panic!("expected IncompleteUniverse"),
    }
}

#[test]
fn short_cells_and_pending_grade_cannot_yield_full_estimate() {
    // A pending first subject: unresolved, pending_slots, not complete.
    let pending = SlotSubject {
        availability: Availability::Pending,
        ..sub("r-p", 1, Some(OutcomeClass::Scored), None)
    };
    let slots = vec![
        slot("p1", "t0", "A", vec![pending]),
        slot(
            "p2",
            "t0",
            "A",
            vec![sub("r2", 1, Some(OutcomeClass::Scored), Some(true))],
        ),
        slot(
            "p3",
            "t0",
            "B",
            vec![sub("r3", 1, Some(OutcomeClass::Scored), Some(true))],
        ),
    ];
    let pop = assemble(input(slots)).unwrap();
    assert!(!pop.complete);
    assert_eq!(pop.pending_slots, vec!["p1".to_string()]);
    assert_eq!(pop.populations["raw"].u, 1);
    let b = contrast_bounds(&pop, PopulationKind::Raw, "A", "B", &BoundsDecl::Unit).unwrap();
    assert!(!b.full);
}

#[test]
fn history_leak_and_missing_pin_refuse() {
    let mut pinned = BTreeMap::new();
    pinned.insert("r1".to_string(), 10u64);
    let mut inp = input(vec![slot(
        "p1",
        "t0",
        "A",
        vec![SlotSubject {
            seq: 11,
            ..sub("r1", 1, Some(OutcomeClass::Scored), Some(true))
        }],
    )]);
    inp.pins = pinned.clone();
    assert!(matches!(
        assemble(inp.clone()),
        Err(PopulationError::HistoryLeak {
            seq: 11,
            pin: 10,
            ..
        })
    ));
    // Resolved record, no pin at all.
    inp.slots[0].subjects[0].seq = 5;
    inp.pins.clear();
    assert!(matches!(
        assemble(inp),
        Err(PopulationError::WatermarkMissing { .. })
    ));
    // A valid pin reproduces: same input → same population.
    let mut inp2 = input(vec![slot(
        "p1",
        "t0",
        "A",
        vec![SlotSubject {
            seq: 10,
            ..sub("r1", 1, Some(OutcomeClass::Scored), Some(true))
        }],
    )]);
    inp2.pins = pinned;
    let a = assemble(inp2.clone()).unwrap();
    let b = assemble(inp2).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.watermarks["r1"], 10);
}

#[test]
fn unbounded_censored_and_nonunit_bounds_refuse() {
    let pop = d1_pop(1).pop();
    assert!(matches!(
        contrast_bounds(
            &pop,
            PopulationKind::Raw,
            "A",
            "B",
            &BoundsDecl::Unbounded {
                detail: "cost".into()
            }
        ),
        Err(PopulationError::UnboundedMetric { .. })
    ));
    assert!(matches!(
        contrast_bounds(
            &pop,
            PopulationKind::Raw,
            "A",
            "B",
            &BoundsDecl::RightCensored {
                detail: "wall_ms".into()
            }
        ),
        Err(PopulationError::CensoredMetric { .. })
    ));
    assert!(matches!(
        contrast_bounds(
            &pop,
            PopulationKind::Raw,
            "A",
            "B",
            &BoundsDecl::Bounded {
                lo_ppm: 100_000,
                hi_ppm: 900_000
            }
        ),
        Err(PopulationError::UnsupportedBounds { .. })
    ));
    // The unit interval declared explicitly is the same target.
    assert!(contrast_bounds(
        &pop,
        PopulationKind::Raw,
        "A",
        "B",
        &BoundsDecl::Bounded {
            lo_ppm: 0,
            hi_ppm: 1_000_000
        }
    )
    .is_ok());
}

#[test]
fn metadata_record_is_the_s1_spine() {
    let pop = d1_pop(1).pop();
    let b = contrast_bounds(&pop, PopulationKind::Canonical, "A", "B", &BoundsDecl::Unit).unwrap();
    let j = b.metadata.to_json();
    for m in [
        "estimator",
        "version",
        "target",
        "assumptions",
        "prior",
        "keys",
        "stratum_membership",
        "seed",
        "draws_requested",
        "draws_used",
        "floors",
        "interval",
        "refusal",
    ] {
        assert!(j.get(m).is_some(), "metadata member {m}");
    }
    assert_eq!(
        j.get("interval").and_then(|i| i.get("kind")),
        Some(&Json::str("descriptive"))
    );
    assert_eq!(b.metadata.draws_used, 0);
}
