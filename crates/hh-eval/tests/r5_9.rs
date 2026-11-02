//! R5.9 acceptance battery — REQ-E-204/REQ-E-103 (primary), REQ-E-102
//! (contributing): shared first-valid grade selection + immutable
//! authorized revisions (PADR-R5-1 A.1 5–7 / A.3 / A.4). Pure contract —
//! scratch in-memory facts, no store, offline, $0.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::selection::*;
use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::MetricValueKind;

const SUBJ: &str = "run-subj-1";
const SUBM: &str = "subm-sealed-1";
const ORCL: &str = "oracle-pinned-1";
const POL: &str = "policy-first-valid-1";
const M: &str = "capability.task_success";
const IRUN: &str = "run-instr-1";

fn set() -> ValidGradeSet {
    ValidGradeSet {
        subject_ref: SUBJ.into(),
        submission_ref: SUBM.into(),
        oracle_ref: ORCL.into(),
        policy_ref: POL.into(),
        members: BTreeSet::from([M.to_string()]),
    }
}

fn att(seq: u64, v: i64) -> GradeAttempt {
    GradeAttempt {
        attempt_ref: format!("{IRUN}:{seq}"),
        instrument_run_id: IRUN.into(),
        seq,
        subject_ref: SUBJ.into(),
        submission_ref: SUBM.into(),
        oracle_ref: ORCL.into(),
        metric_ref: M.into(),
        cause_refs: vec![format!("{SUBJ}:e-{seq}")],
        outcome: GradeOutcome::Decided(MetricValueKind::Decimal(v)),
    }
}

fn failed(seq: u64) -> GradeAttempt {
    GradeAttempt {
        outcome: GradeOutcome::OracleFailure,
        ..att(seq, 0)
    }
}

fn rev(
    id: &str,
    supersedes: &str,
    authority: &str,
    set: &ValidGradeSet,
    at: u64,
) -> SelectionRevision {
    SelectionRevision {
        revision_id: id.into(),
        reason: "correction".into(),
        supersedes: supersedes.into(),
        authority: authority.into(),
        watermarks: BTreeMap::from([(IRUN.to_string(), at)]),
        eligible_set_ref: set.eligible_set_ref(),
    }
}

fn status<'a>(s: &'a GradeSelection, m: &str) -> &'a MemberStatus {
    s.members.get(m).unwrap()
}

fn reason<'a>(s: &'a GradeSelection, attempt_ref: &str) -> &'a Exclusion {
    &s.non_winning
        .iter()
        .find(|e| e.attempt_ref == attempt_ref)
        .unwrap()
        .reason
}

// AC-1: failure→valid zero→failure retains zero.
#[test]
fn failure_then_valid_zero_then_failure_retains_zero() {
    let s = select_grade(&set(), &[failed(1), att(2, 0), failed(3)], &[]).unwrap();
    let MemberStatus::Selected(g) = status(&s, M) else {
        panic!("expected selected, got {:?}", status(&s, M))
    };
    assert_eq!(g.value, MetricValueKind::Decimal(0));
    assert_eq!(g.attempt_ref, format!("{IRUN}:2"));
    assert_eq!(reason(&s, &format!("{IRUN}:3")), &Exclusion::Unavailable);
}

// AC-1: historical watermark resolves the historical selection.
#[test]
fn historical_watermark_resolves_historical_selection() {
    let s = set();
    let attempts = vec![att(1, 0), att(2, 1)];
    let revisions = vec![rev("rev-1", &format!("{IRUN}:1"), "human:analyst", &s, 2)];
    let wm1 = BTreeMap::from([(IRUN.to_string(), 1u64)]);
    let early = select_grade_at(&s, &attempts, &revisions, &wm1).unwrap();
    let MemberStatus::Selected(g) = status(&early, M) else {
        panic!()
    };
    assert_eq!(g.attempt_ref, format!("{IRUN}:1")); // old pin, old selection
    let wm2 = BTreeMap::from([(IRUN.to_string(), 2u64)]);
    let late = select_grade_at(&s, &attempts, &revisions, &wm2).unwrap();
    let MemberStatus::Selected(g) = status(&late, M) else {
        panic!()
    };
    assert_eq!(g.attempt_ref, format!("{IRUN}:2")); // rev-1 visible at pin 2
    let unpinned = select_grade_at(&s, &attempts, &revisions, &BTreeMap::new());
    assert_eq!(
        unpinned.unwrap_err(),
        SelectionError::WatermarkMissing {
            run_id: IRUN.into()
        }
    );
}

// AC-2: a later higher reward cannot win; a duplicate instrument cannot win.
#[test]
fn later_higher_reward_and_duplicate_instrument_cannot_win() {
    let s = set();
    let sel = select_grade(&s, &[att(1, 0), att(1, 0), att(2, 1)], &[]).unwrap();
    let MemberStatus::Selected(g) = status(&sel, M) else {
        panic!()
    };
    assert_eq!(g.value, MetricValueKind::Decimal(0));
    assert_eq!(reason(&sel, &format!("{IRUN}:2")), &Exclusion::NotFirst);
    assert_eq!(
        sel.non_winning
            .iter()
            .filter(|e| e.reason == Exclusion::DuplicateInstrument)
            .count(),
        1
    );
    let conflict = GradeAttempt {
        outcome: GradeOutcome::Decided(MetricValueKind::Decimal(1)),
        ..att(1, 0)
    };
    assert_eq!(
        select_grade(&s, &[att(1, 0), conflict], &[]).unwrap_err(),
        SelectionError::ConflictingCoordinate {
            attempt_ref: format!("{IRUN}:1")
        }
    );
}

// AC-2: wrong subject/submission/oracle/set and bad causes cannot win.
#[test]
fn wrong_pins_set_and_causes_cannot_win() {
    let s = set();
    let cases: [(fn(&mut GradeAttempt), &str); 3] = [
        (|a| a.subject_ref = "run-other".into(), "subject_ref"),
        (|a| a.submission_ref = "subm-other".into(), "submission_ref"),
        (|a| a.oracle_ref = "oracle-other".into(), "oracle_ref"),
    ];
    for (f, member) in cases {
        let mut a = att(1, 1);
        f(&mut a);
        let sel = select_grade(&s, &[a.clone()], &[]).unwrap();
        assert_eq!(status(&sel, M), &MemberStatus::Pending);
        assert_eq!(
            reason(&sel, &a.attempt_ref),
            &Exclusion::PinMismatch { member }
        );
    }
    let mut wrong_member = att(1, 1);
    wrong_member.metric_ref = "other.metric".into();
    let sel = select_grade(&s, &[wrong_member.clone()], &[]).unwrap();
    assert_eq!(reason(&sel, &wrong_member.attempt_ref), &Exclusion::SetMiss);
    let mut no_cause = att(1, 1);
    no_cause.cause_refs = vec![];
    let sel = select_grade(&s, &[no_cause.clone()], &[]).unwrap();
    assert_eq!(
        reason(&sel, &no_cause.attempt_ref),
        &Exclusion::UncitedCauses
    );
    let mut foreign = att(1, 1);
    foreign.cause_refs = vec![format!("{SUBJ}:e-1"), "run-foreign:e-9".into()];
    let sel = select_grade(&s, &[foreign.clone()], &[]).unwrap();
    assert_eq!(
        reason(&sel, &foreign.attempt_ref),
        &Exclusion::ForeignCauses
    );
}

// AC-1/AC-2: authorized append-only invalidation; unauthorized correction
// cannot win; invalidated evidence stays unavailable until a valid
// replacement exists.
#[test]
fn authorized_append_only_invalidation() {
    let s = set();
    let known = BTreeSet::from([format!("{IRUN}:1"), format!("{IRUN}:2")]);
    let mut log = RevisionLog::default();
    let unauthorized = rev("rev-x", &format!("{IRUN}:1"), "", &s, 1);
    assert_eq!(
        log.append(&s, unauthorized.clone(), &known).unwrap_err(),
        SelectionError::UnauthorizedCorrection {
            revision_id: "rev-x".into()
        }
    );
    assert_eq!(
        select_grade(&s, &[att(1, 0)], &[unauthorized]).unwrap_err(),
        SelectionError::UnauthorizedCorrection {
            revision_id: "rev-x".into()
        }
    );
    let mut bad_set_rev = rev("rev-y", &format!("{IRUN}:1"), "human:analyst", &s, 1);
    bad_set_rev.eligible_set_ref = "other-set".into();
    assert_eq!(
        log.append(&s, bad_set_rev, &known).unwrap_err(),
        SelectionError::SetMismatch {
            revision_id: "rev-y".into()
        }
    );
    let ghost = rev("rev-z", "no-such-attempt", "human:analyst", &s, 1);
    assert_eq!(
        log.append(&s, ghost, &known).unwrap_err(),
        SelectionError::UnknownSupersedes {
            revision_id: "rev-z".into(),
            supersedes: "no-such-attempt".into()
        }
    );
    let r1 = rev("rev-1", &format!("{IRUN}:1"), "human:analyst", &s, 1);
    log.append(&s, r1.clone(), &known).unwrap();
    let mut mutated = r1.clone();
    mutated.reason = "different".into();
    assert_eq!(
        log.append(&s, mutated, &known).unwrap_err(),
        SelectionError::ImmutableRecord {
            revision_id: "rev-1".into()
        }
    );
    log.append(&s, r1, &known).unwrap(); // identical re-append is idempotent
    assert_eq!(log.revisions().len(), 1);
    // invalidated → unavailable; valid replacement then wins.
    let sel = select_grade(&s, &[att(1, 0)], log.revisions()).unwrap();
    assert_eq!(status(&sel, M), &MemberStatus::Unavailable);
    let sel = select_grade(&s, &[att(1, 0), att(2, 0)], log.revisions()).unwrap();
    let MemberStatus::Selected(g) = status(&sel, M) else {
        panic!()
    };
    assert_eq!(g.attempt_ref, format!("{IRUN}:2"));
    // correction of a correction revokes the first revision.
    let r2 = rev("rev-2", "rev-1", "human:analyst", &s, 2);
    log.append(&s, r2, &known).unwrap();
    let sel = select_grade(&s, &[att(1, 0)], log.revisions()).unwrap();
    let MemberStatus::Selected(g) = status(&sel, M) else {
        panic!()
    };
    assert_eq!(g.attempt_ref, format!("{IRUN}:1"));
}

// AC-2: budget_exhausted base + veto stay distinct axes; pending is
// admission state, oracle_failure the all-unavailable disposition.
#[test]
fn disposition_axes_and_pending() {
    let s = set();
    let sel = select_grade(&s, &[att(1, 0)], &[]).unwrap();
    let d = plan_disposition(
        OutcomeClass::BudgetExhausted,
        &["veto-loop".to_string()],
        &sel,
    );
    let PlanDisposition::Graded {
        outcome_class,
        grades,
        veto_tripped,
        ..
    } = d
    else {
        panic!("expected graded, got {d:?}")
    };
    assert_eq!(outcome_class, OutcomeClass::BudgetExhausted);
    assert_eq!(grades.get(M), Some(&MetricValueKind::Decimal(0)));
    assert_eq!(veto_tripped, vec!["veto-loop".to_string()]);
    let sel = select_grade(&s, &[], &[]).unwrap();
    let PlanDisposition::Pending { missing, .. } =
        plan_disposition(OutcomeClass::Scored, &[], &sel)
    else {
        panic!("expected pending")
    };
    assert_eq!(missing, vec![M.to_string()]);
    let sel = select_grade(&s, &[failed(1)], &[]).unwrap();
    let PlanDisposition::OracleFailure { missing, .. } =
        plan_disposition(OutcomeClass::Scored, &[], &sel)
    else {
        panic!("expected oracle_failure")
    };
    assert_eq!(missing, vec![M.to_string()]);
}

// AC-1: the freeze itself refuses empty pins/members.
#[test]
fn freeze_validation_refuses_empty() {
    let mut s = set();
    s.members = BTreeSet::new();
    assert_eq!(
        select_grade(&s, &[], &[]).unwrap_err(),
        SelectionError::EmptyEligibleSet
    );
    let mut s = set();
    s.oracle_ref.clear();
    assert_eq!(
        select_grade(&s, &[], &[]).unwrap_err(),
        SelectionError::EmptyPin {
            member: "oracle_ref"
        }
    );
}
