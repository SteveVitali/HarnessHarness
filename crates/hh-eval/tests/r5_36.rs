//! R5.36 — the §4.2 mechanism joins + versioned denominator battery
//! (REQ-D-005/106, REQ-E-004/107; AC-1 golden joins, AC-2 typed
//! negatives). Fails-without: `eval::mechanism` does not exist at the
//! R5.35 base — this file compiles red there.

use std::collections::BTreeMap;

use hharness_eval::facts::{FactRow, LedgerFacts};
use hharness_eval::mechanism::{contrast, detect, report, MechanismOutcome, SubjectMechanism};
use hharness_wire::Json;

fn row(seq: u64, class: &str, payload: Json) -> FactRow {
    FactRow {
        seq,
        event_id: None,
        class: class.into(),
        payload,
    }
}

fn obj(m: Vec<(&'static str, Json)>) -> Json {
    Json::obj(m)
}

fn strs(v: &[&str]) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(*s)).collect())
}

/// The AC-1 golden subject: two overlapping completed compactions,
/// repeated deliveries of one forgotten item, an exact idempotency
/// match, a next-dispatch join, an ineffective completion, in-window
/// instrument charges — then `phase → verify` (the execution stop), a
/// post-stop delivery that can never join, a graded row, and
/// `run.finished`.
fn golden_a() -> LedgerFacts {
    LedgerFacts::from_rows(&[
        row(1, "lifecycle.run.created", obj(vec![])),
        row(
            2,
            "action.environment.phase.changed",
            obj(vec![("to", Json::str("agent"))]),
        ),
        row(
            4,
            "action.effect.committed",
            obj(vec![
                ("effect_id", Json::str("e0")),
                ("idempotency_key", Json::str("k1")),
            ]),
        ),
        row(
            5,
            "model.call.requested",
            obj(vec![("model_call_id", Json::str("m0"))]),
        ),
        row(
            6,
            "context.compaction.started",
            obj(vec![("variant_ref", Json::str("evict_oldest"))]),
        ),
        row(
            8,
            "context.compaction.completed",
            obj(vec![
                ("variant_ref", Json::str("evict_oldest")),
                ("status", Json::str("applied")),
                ("forgotten", strs(&["x", "y"])),
            ]),
        ),
        row(
            9,
            "action.effect.committed",
            obj(vec![
                ("effect_id", Json::str("e1")),
                ("idempotency_key", Json::str("k1")),
            ]),
        ),
        row(
            10,
            "context.memory.read",
            obj(vec![("delivered", strs(&["x"]))]),
        ),
        row(
            12,
            "context.artefact.delivered",
            obj(vec![("artefact_id", Json::str("x"))]),
        ),
        row(
            14,
            "context.compaction.started",
            obj(vec![("variant_ref", Json::str("evict_oldest"))]),
        ),
        row(
            16,
            "context.compaction.completed",
            obj(vec![
                ("variant_ref", Json::str("evict_oldest")),
                ("status", Json::str("ineffective")),
                ("forgotten", strs(&["x", "z"])),
            ]),
        ),
        row(
            18,
            "context.memory.read",
            obj(vec![("delivered", strs(&["x", "z"]))]),
        ),
        row(
            20,
            "model.call.requested",
            obj(vec![("model_call_id", Json::str("m1"))]),
        ),
        row(
            21,
            "context.assembled",
            obj(vec![("model_call_id", Json::str("m1"))]),
        ),
        row(
            22,
            "control.budget.consumed",
            obj(vec![
                ("dimension", Json::str("tokens")),
                ("amount", Json::Int(40)),
                (
                    "attribution",
                    obj(vec![("charged_to", Json::str("instrument"))]),
                ),
            ]),
        ),
        row(
            24,
            "measurement.bench.graded",
            obj(vec![("exit_code", Json::Int(0))]),
        ),
        row(
            26,
            "action.environment.phase.changed",
            obj(vec![("to", Json::str("verify"))]),
        ),
        // seq 28 — a post-execution delivery: a match on `y` would exist
        // were the window unbounded; it can never join.
        row(
            28,
            "context.memory.read",
            obj(vec![("delivered", strs(&["y"]))]),
        ),
        row(30, "lifecycle.run.finished", obj(vec![])),
    ])
}

fn subject<'a>(facts: &'a LedgerFacts, task: &'a str, arm: &'a str) -> SubjectMechanism<'a> {
    SubjectMechanism {
        subject_run: "s1",
        task_id: task,
        arm_id: arm,
        first_subject: true,
        facts,
        eligible_opportunities: Some(3),
        triggered: Some(true),
        intended_variant: Some("evict_oldest"),
        extractable_forgotten: Some(vec!["x".into(), "z".into()]),
        pre_label: Some("pre"),
        post_label: Some("post"),
        summary_present: true,
        summary_applicable: Some(true),
        summary_followed: Some(true),
        opacity_pre: Some(10),
        opacity_post: Some(14),
        subject_totals: BTreeMap::from([("tokens", 100), ("calls", 4), ("micro_usd", 200)]),
    }
}

fn at<'a>(
    r: &'a hharness_eval::mechanism::SubjectMechanismReport,
    name: &str,
) -> &'a MechanismOutcome {
    r.metrics
        .get(name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn ac1_overlapping_windows_retain_joins_and_intensity() {
    let f = golden_a();
    assert_eq!(f.subject_stop_seq, Some(26), "phase→verify is the stop");
    let r = detect(&subject(&f, "t1", "A"));
    // joins: (8,10,x) (8,12,x) (8,18,x) (16,18,x) (16,18,z) — the seq-18
    // read joins BOTH windows; seq-28 never joins.
    assert_eq!(
        at(&r, "grounding.reacquisition_rate"),
        &MechanismOutcome::Value {
            numerator: 5,
            denominator: 4
        },
        "intensity 5/4 > 1 — repeated deliveries count, never clamped"
    );
    let js: Vec<(u64, u64, &str)> = r
        .joins
        .iter()
        .filter(|j| j.metric == "grounding.reacquisition_rate")
        .map(|j| (j.compaction_seq, j.event_seq, j.key.as_str()))
        .collect();
    assert!(js.contains(&(8, 18, "x")) && js.contains(&(16, 18, "x")));
    assert!(!js.iter().any(|j| j.1 == 28));
    // Exact idempotency: e1(9,k1) matches pre-c key k1 in c8's window.
    assert_eq!(
        at(&r, "reliability.repeated_action_rate"),
        &MechanismOutcome::Value {
            numerator: 1,
            denominator: 1
        }
    );
    // Distinct counts — fidelity/fallback/ineffective/eligible.
    for (name, want) in [
        ("mechanism.eligible_count", 3),
        ("mechanism.starts_count", 2),
        ("mechanism.completions_count", 2),
        ("mechanism.ineffective_count", 1),
        ("mechanism.fallback_count", 0),
        ("mechanism.compaction_count", 2),
    ] {
        assert_eq!(
            at(&r, name),
            &MechanismOutcome::Count { value: want },
            "{name}"
        );
    }
    assert_eq!(
        at(&r, "mechanism.activation_start_rate"),
        &MechanismOutcome::Value {
            numerator: 2,
            denominator: 3
        }
    );
    assert_eq!(
        at(&r, "mechanism.fidelity_completion_rate"),
        &MechanismOutcome::Value {
            numerator: 2,
            denominator: 2
        }
    );
    // Next-dispatch join: m1 follows both completions; assembled ⇒ changed.
    assert_eq!(
        at(&r, "mechanism.fidelity_request_change_rate"),
        &MechanismOutcome::Value {
            numerator: 2,
            denominator: 2
        }
    );
    assert_eq!(
        at(&r, "mechanism.ineffective_rate"),
        &MechanismOutcome::Value {
            numerator: 1,
            denominator: 2
        }
    );
    assert_eq!(
        at(&r, "mechanism.trigger_first_subject_rate"),
        &MechanismOutcome::Value {
            numerator: 1,
            denominator: 1
        }
    );
    // Recall: extractable {x,z}; both delivered in-window.
    assert_eq!(
        at(&r, "mechanism.recall_rate"),
        &MechanismOutcome::Value {
            numerator: 2,
            denominator: 2
        }
    );
    // Overhead: 40 instrument tokens in-window / 100 subject tokens.
    assert_eq!(
        at(&r, "mechanism.overhead.tokens_rate"),
        &MechanismOutcome::Value {
            numerator: 40,
            denominator: 100
        }
    );
}

#[test]
fn ac2_post_execution_grade_cannot_enter_windows() {
    let mut f = golden_a();
    // Drop the phase→verify row: the stop falls back to finished(30),
    // and the seq-28 read then joins — proving the boundary is what
    // excludes it, never the row's own class.
    f.subject_stop_seq = None;
    let r = detect(&subject(&f, "t1", "A"));
    assert!(matches!(
        at(&r, "grounding.reacquisition_rate"),
        MechanismOutcome::NoDetector { .. }
    ));
    let bounded = detect(&subject(&golden_a(), "t1", "A"));
    assert_eq!(
        at(&bounded, "grounding.reacquisition_rate"),
        &MechanismOutcome::Value {
            numerator: 5,
            denominator: 4
        },
        "the graded/post-stop rows changed nothing"
    );
}

#[test]
fn ac2_missing_key_extractor_and_zero_denominators() {
    // Missing idempotency key in-window ⇒ no_detector.
    let no_key = LedgerFacts::from_rows(&[
        row(1, "lifecycle.run.created", obj(vec![])),
        row(
            8,
            "context.compaction.completed",
            obj(vec![
                ("status", Json::str("applied")),
                ("forgotten", strs(&[])),
            ]),
        ),
        row(
            9,
            "action.effect.committed",
            obj(vec![("effect_id", Json::str("e1"))]),
        ),
        row(30, "lifecycle.run.finished", obj(vec![])),
    ]);
    let r = detect(&subject(&no_key, "t1", "A"));
    assert!(matches!(
        at(&r, "reliability.repeated_action_rate"),
        MechanismOutcome::NoDetector { .. }
    ));
    // Absent extractor ⇒ no_detector; no summary ⇒ inapplicable.
    let mut s2 = subject(&no_key, "t1", "A");
    s2.extractable_forgotten = None;
    s2.summary_present = false;
    let r2 = detect(&s2);
    assert!(matches!(
        at(&r2, "mechanism.recall_rate"),
        MechanismOutcome::NoDetector { .. }
    ));
    assert!(matches!(
        at(&r2, "mechanism.summary_followed_rate"),
        MechanismOutcome::Inapplicable { .. }
    ));
    // Zero denominators ⇒ estimator_undefined (reacquisition has no
    // forgotten items; fidelity has no starts).
    assert!(matches!(
        at(&r, "grounding.reacquisition_rate"),
        MechanismOutcome::EstimatorUndefined
    ));
    assert!(matches!(
        at(&r, "mechanism.fidelity_completion_rate"),
        MechanismOutcome::EstimatorUndefined
    ));
    // No compaction ⇒ count 0 stays a value; conditional rates
    // inapplicable.
    let bare = LedgerFacts::from_rows(&[
        row(1, "lifecycle.run.created", obj(vec![])),
        row(30, "lifecycle.run.finished", obj(vec![])),
    ]);
    let rb = detect(&subject(&bare, "t1", "A"));
    assert_eq!(
        at(&rb, "mechanism.compaction_count"),
        &MechanismOutcome::Count { value: 0 }
    );
    assert!(matches!(
        at(&rb, "mechanism.overhead.tokens_rate"),
        MechanismOutcome::Inapplicable { .. }
    ));
    // The retained old approximation keeps its own distinct name.
    assert!(matches!(
        at(&r, "reliability.repeated_action_similarity"),
        MechanismOutcome::Count { .. }
    ));
    assert_eq!(
        hharness_eval::mechanism::registry_decl("repeated_action_count"),
        None,
        "the old count oracle keeps its name — the approximation row is the similarity name"
    );
    assert!(hharness_eval::mechanism::MECHANISM_REGISTRY
        .iter()
        .any(|d| d.name == "reliability.repeated_action_similarity" && !d.required));
}

#[test]
fn ac2_unavailable_required_members_stay_in_family() {
    let f = golden_a();
    let mut s = subject(&f, "t1", "A");
    s.eligible_opportunities = None;
    s.extractable_forgotten = None;
    let rep = report(&[s]);
    assert_eq!(
        rep.family.len(),
        hharness_eval::mechanism::MECHANISM_REGISTRY.len()
    );
    let act = rep
        .family
        .iter()
        .find(|r| r.name == "mechanism.activation_start_rate")
        .unwrap();
    assert_eq!(act.status, "unavailable");
    assert!(act.required);
}

#[test]
fn ac1_subject_ratio_task_mean_equal_task_contrast() {
    // t1: A arm s-a1 ratio 1/1 (the golden fold), s-a2 no_detector
    //     (coverage 1 of 2); B arm s-b1 ratio 0/2.
    // t2: A arm s-a3 ratio 1/2; B arm s-b2 ratio 1/4.
    let facts = golden_a();
    let staged = |task: &str, arm: &str, outcome: MechanismOutcome| {
        SubjectMechanismReportShell::staged(task, arm, outcome)
    };
    let rep = hharness_eval::mechanism::MechanismReport {
        subjects: vec![
            detect(&subject(&facts, "t1", "A")),
            staged("t1", "A", MechanismOutcome::NoDetector { reason: "x" }),
            staged(
                "t1",
                "B",
                MechanismOutcome::Value {
                    numerator: 0,
                    denominator: 2,
                },
            ),
            staged(
                "t2",
                "A",
                MechanismOutcome::Value {
                    numerator: 1,
                    denominator: 2,
                },
            ),
            staged(
                "t2",
                "B",
                MechanismOutcome::Value {
                    numerator: 1,
                    denominator: 4,
                },
            ),
        ],
        family: vec![],
    };
    let c = contrast(&rep, "reliability.repeated_action_rate", "A", "B");
    assert_eq!(c.unit, "ratio", "declared unit from the registry");
    assert_eq!(c.paired.len(), 2);
    // t1 A mean = 1.0 (only s-a1 available — coverage 1 of 2).
    let (t, ma, mb) = &c.paired[0];
    assert_eq!(t, "t1");
    assert_eq!((ma.available, ma.total), (1, 2));
    assert_eq!(ma.mean, 1.0);
    assert_eq!(mb.mean, 0.0);
    // delta = ((0 − 1) + (1/4 − 1/2)) / 2 = −0.625.
    assert!(
        (c.delta - -0.625).abs() < 1e-12,
        "equal-task contrast w_t=1/T"
    );
    // Unpaired tasks are retained beside, never folded.
    let rep2 = hharness_eval::mechanism::MechanismReport {
        subjects: vec![staged(
            "t9",
            "B",
            MechanismOutcome::Value {
                numerator: 1,
                denominator: 1,
            },
        )],
        family: vec![],
    };
    let rep3 = hharness_eval::mechanism::MechanismReport {
        subjects: rep.subjects.iter().cloned().chain(rep2.subjects).collect(),
        family: vec![],
    };
    let c3 = contrast(&rep3, "reliability.repeated_action_rate", "A", "B");
    assert_eq!(c3.unpaired, vec!["t9".to_string()]);
}

/// Build a staged subject report carrying just the one metric outcome
/// the contrast leg needs.
struct SubjectMechanismReportShell;

impl SubjectMechanismReportShell {
    fn staged(
        task: &str,
        arm: &str,
        outcome: MechanismOutcome,
    ) -> hharness_eval::mechanism::SubjectMechanismReport {
        let mut metrics = BTreeMap::new();
        metrics.insert("reliability.repeated_action_rate", outcome);
        hharness_eval::mechanism::SubjectMechanismReport {
            subject_run: format!("s-{task}-{arm}"),
            task_id: task.into(),
            arm_id: arm.into(),
            metrics,
            joins: vec![],
            pre_label: None,
            post_label: None,
        }
    }
}
