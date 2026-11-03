//! `selection` — the shared pure grade-selection contract (PADR-R5-1
//! A.1 items 5–7 / A.3 / A.4; spec §2.2 steps 5–6; REQ-E-204, REQ-E-103
//! primary; REQ-E-102 contributing).
//!
//! Pure — every input is an already owner-validated fact (the owning
//! consumer resolved the durable verdict rows, `(run_id, seq)` pins and
//! cause coordinates through existing store access); this module never
//! touches a store (no eval→ledger edge) and is implemented **once** —
//! results/scheduler/export consumers compose it, never re-select.
//!
//! First valid wins inside the frozen [`ValidGradeSet`]: a later higher
//! reward or `OracleFailure` cannot displace it (failure→valid zero→
//! failure retains zero; wrong-but-valid is a valid zero). Wrong
//! subject/submission/oracle/member pins and absent/foreign cause
//! citations cannot win. Revisions are authorized + append-only (A.4):
//! a correction lacking `authority` is a typed refusal; invalidated
//! evidence stays unavailable until a valid replacement exists.
//! [`select_grade_at`] reads each run up to its watermark pin — a
//! historical pin reproduces the historical selection.

use std::collections::{BTreeMap, BTreeSet};

use hharness_ontology::control::OutcomeClass;
use hharness_ontology::eval::MetricValueKind;
use hharness_wire::json::Json;

/// Typed refusal — every invalid input is an error, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionError {
    /// The valid-grade set has no eligible members.
    EmptyEligibleSet,
    /// A required set pin (`subject_ref|submission_ref|oracle_ref|policy_ref`) is empty.
    EmptyPin { member: &'static str },
    /// A correction lacking the authorized field (A.4).
    UnauthorizedCorrection { revision_id: String },
    /// A revision carrying no `watermarks[]` pin.
    MissingWatermarks { revision_id: String },
    /// A revision binding a different eligible set.
    SetMismatch { revision_id: String },
    /// `supersedes` names no known attempt ref or prior revision.
    UnknownSupersedes {
        revision_id: String,
        supersedes: String,
    },
    /// `revision_id` re-appended with different bytes — immutable.
    ImmutableRecord { revision_id: String },
    /// One `(instrument_run_id, seq)` coordinate presented with two
    /// different facts — the durable record cannot diverge.
    ConflictingCoordinate { attempt_ref: String },
    /// Attempts presented out of durable order within one run.
    OutOfOrder { instrument_run_id: String, seq: u64 },
    /// [`select_grade_at`]: an attempt on a run the watermark does not
    /// pin — an unpinned read cannot claim durability.
    WatermarkMissing { run_id: String },
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SelectionError {}

/// The frozen valid-grade set — the `grade_binding`'s materialised pins
/// (`subject_ref`/`submission_ref`/`oracle_ref`/`policy_ref`) plus the
/// eligible grade members (metric refs); an attempt pinning anything
/// else is structurally ineligible.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidGradeSet {
    pub subject_ref: String,
    pub submission_ref: String,
    pub oracle_ref: String,
    pub policy_ref: String,
    pub members: BTreeSet<String>,
}

impl ValidGradeSet {
    /// Validate the freeze — empty members/pins are typed refusals.
    pub fn validate(&self) -> Result<(), SelectionError> {
        if self.members.is_empty() {
            return Err(SelectionError::EmptyEligibleSet);
        }
        for (member, value) in [
            ("subject_ref", &self.subject_ref),
            ("submission_ref", &self.submission_ref),
            ("oracle_ref", &self.oracle_ref),
            ("policy_ref", &self.policy_ref),
        ] {
            if value.is_empty() {
                return Err(SelectionError::EmptyPin { member });
            }
        }
        Ok(())
    }

    /// Canonical JSON — the durable/wire form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("subject_ref", Json::str(&self.subject_ref)),
            ("submission_ref", Json::str(&self.submission_ref)),
            ("oracle_ref", Json::str(&self.oracle_ref)),
            ("policy_ref", Json::str(&self.policy_ref)),
            (
                "members",
                Json::Arr(self.members.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// The content-addressed eligible-set ref revisions bind (A.3).
    pub fn eligible_set_ref(&self) -> String {
        hharness_identity::idp::idp_id(
            "valid_grade_set",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

/// A single attempt's grading outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum GradeOutcome {
    /// A decided grade — wrong-but-valid is a valid zero.
    Decided(MetricValueKind),
    /// Crash/timeout/empty/non-finite/refusal — unavailable, never a
    /// task zero and never an erasure of an earlier valid grade.
    OracleFailure,
}

impl GradeOutcome {
    /// Whether the outcome can win — a typed `n/a` is unavailable.
    fn is_winning(&self) -> bool {
        matches!(self, Self::Decided(v) if !matches!(v, MetricValueKind::Na(_)))
    }
}

/// One immutable instrument-attempt fact — a durable verdict the owning
/// consumer already resolved. `cause_refs` carry `run_id:event_id`
/// coordinates; the attempt must cite its subject run and no foreign run.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeAttempt {
    pub attempt_ref: String,
    pub instrument_run_id: String,
    pub seq: u64,
    pub subject_ref: String,
    pub submission_ref: String,
    pub oracle_ref: String,
    pub metric_ref: String,
    pub cause_refs: Vec<String>,
    pub outcome: GradeOutcome,
}

/// The run a cause coordinate cites — the `run_id:event_id` coordinate
/// splits on the LAST separator: run ids are themselves colon-bearing
/// (`subj-<idp…>`, `sha256:…` content addresses) while event ids are
/// colon-free, so the rightmost segment is the event id (R5.14 landing
/// fix — the naive prefix split made a `subj-*` subject uncitable;
/// ADR-0393, R5.9 revisit flag).
fn cause_run(cause: &str) -> &str {
    cause.rsplit_once(':').map(|(r, _)| r).unwrap_or(cause)
}

/// `SelectionRevision` — the authorized append-only invalidation record
/// `{revision_id, reason, supersedes, authority, watermarks[],
/// eligible_set_ref}` (A.3): immutable, one home, authorized-only.
/// `supersedes` names the invalidated `attempt_ref`, or a prior
/// `revision_id` revoked (correction of a correction). `authority` is
/// required (A.4; ADR-0162 D2's class); its absence is a typed refusal,
/// never a warning.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionRevision {
    pub revision_id: String,
    pub reason: String,
    pub supersedes: String,
    pub authority: String,
    pub watermarks: BTreeMap<String, u64>,
    pub eligible_set_ref: String,
}

impl SelectionRevision {
    /// Canonical JSON — the record's immutable wire form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("revision_id", Json::str(&self.revision_id)),
            ("reason", Json::str(&self.reason)),
            ("supersedes", Json::str(&self.supersedes)),
            ("authority", Json::str(&self.authority)),
            (
                "watermarks",
                Json::Obj(
                    self.watermarks
                        .iter()
                        .map(|(r, s)| (r.clone(), Json::Int(*s as i64)))
                        .collect(),
                ),
            ),
            ("eligible_set_ref", Json::str(&self.eligible_set_ref)),
        ])
    }
}

/// The append-only revision chain — `append` is the only mutation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RevisionLog {
    revisions: Vec<SelectionRevision>,
}

impl RevisionLog {
    /// The recorded revisions in append order.
    pub fn revisions(&self) -> &[SelectionRevision] {
        &self.revisions
    }

    /// Append an authorized revision. Refuses missing `authority`,
    /// empty `watermarks`, a foreign `eligible_set_ref`, a `supersedes`
    /// resolving to no known attempt ref or prior revision, and a
    /// `revision_id` re-presented with different bytes (an identical
    /// re-append is an idempotent replay).
    pub fn append(
        &mut self,
        set: &ValidGradeSet,
        rev: SelectionRevision,
        known_attempt_refs: &BTreeSet<String>,
    ) -> Result<(), SelectionError> {
        if rev.authority.is_empty() {
            return Err(SelectionError::UnauthorizedCorrection {
                revision_id: rev.revision_id,
            });
        }
        if rev.watermarks.is_empty() {
            return Err(SelectionError::MissingWatermarks {
                revision_id: rev.revision_id,
            });
        }
        if rev.eligible_set_ref != set.eligible_set_ref() {
            return Err(SelectionError::SetMismatch {
                revision_id: rev.revision_id,
            });
        }
        if let Some(prior) = self
            .revisions
            .iter()
            .find(|r| r.revision_id == rev.revision_id)
        {
            return if *prior == rev {
                Ok(())
            } else {
                Err(SelectionError::ImmutableRecord {
                    revision_id: rev.revision_id,
                })
            };
        }
        let resolves = known_attempt_refs.contains(&rev.supersedes)
            || self
                .revisions
                .iter()
                .any(|r| r.revision_id == rev.supersedes);
        if !resolves {
            return Err(SelectionError::UnknownSupersedes {
                revision_id: rev.revision_id,
                supersedes: rev.supersedes,
            });
        }
        self.revisions.push(rev);
        Ok(())
    }
}

/// A member's winning attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedGrade {
    pub metric_ref: String,
    pub attempt_ref: String,
    pub instrument_run_id: String,
    pub seq: u64,
    /// The selected raw grade — a separate axis from the outcome class.
    pub value: MetricValueKind,
}

/// Where a member stands after selection.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberStatus {
    /// A first-valid grade won.
    Selected(SelectedGrade),
    /// Admissible evidence exists but none produced a valid grade —
    /// every attempt failed or was revoked without replacement.
    Unavailable,
    /// No admissible evidence seen at this watermark yet.
    Pending,
}

/// Why a presented attempt could not win.
#[derive(Debug, Clone, PartialEq)]
pub enum Exclusion {
    /// A re-presented `(instrument_run_id, seq)` coordinate.
    DuplicateInstrument,
    /// Revoked by the named revision — unavailable until replaced.
    Invalidated { revision_id: String },
    /// A pin differs from the frozen set.
    PinMismatch { member: &'static str },
    /// `metric_ref` is outside the eligible set.
    SetMiss,
    /// Causes do not cite the subject run.
    UncitedCauses,
    /// A cause cites a run that is neither subject nor instrument.
    ForeignCauses,
    /// `OracleFailure`/typed `n/a` — unavailable, never a task zero.
    Unavailable,
    /// A later admissible valid grade — first valid already won.
    NotFirst,
}

/// A non-winning attempt with its reason.
#[derive(Debug, Clone, PartialEq)]
pub struct ExcludedAttempt {
    pub attempt_ref: String,
    pub reason: Exclusion,
}

/// The selection result — carries the eligible set, the selected
/// attempt refs, the read watermarks and the applied revisions so
/// reports pin exactly what was read (A.1 item 7).
#[derive(Debug, Clone, PartialEq)]
pub struct GradeSelection {
    /// The bound set's content address.
    pub eligible_set_ref: String,
    /// Every member's status.
    pub members: BTreeMap<String, MemberStatus>,
    /// Non-winning attempts with reasons — every presented attempt is
    /// accounted for.
    pub non_winning: Vec<ExcludedAttempt>,
    /// The resolved read pins — `instrument_run_id → max seq read`.
    pub watermarks: BTreeMap<String, u64>,
    /// The revision ids applied (effective, in append order).
    pub revisions_applied: Vec<String>,
}

/// Effective revisions in append order — a revision whose `supersedes`
/// names a prior effective `revision_id` revokes it (correction of a
/// correction); every revision must carry an authority and bind the set.
fn effective_revisions<'a>(
    set: &ValidGradeSet,
    revisions: &'a [SelectionRevision],
) -> Result<Vec<&'a SelectionRevision>, SelectionError> {
    let set_ref = set.eligible_set_ref();
    let mut effective: Vec<&SelectionRevision> = Vec::new();
    for r in revisions {
        if r.authority.is_empty() {
            return Err(SelectionError::UnauthorizedCorrection {
                revision_id: r.revision_id.clone(),
            });
        }
        if r.eligible_set_ref != set_ref {
            return Err(SelectionError::SetMismatch {
                revision_id: r.revision_id.clone(),
            });
        }
        if let Some(pos) = effective.iter().position(|p| p.revision_id == r.supersedes) {
            effective.remove(pos);
        }
        effective.push(r);
    }
    Ok(effective)
}

fn select(
    set: &ValidGradeSet,
    attempts: &[GradeAttempt],
    effective: &[&SelectionRevision],
) -> Result<GradeSelection, SelectionError> {
    set.validate()?;
    let revision_ids: BTreeSet<&str> = effective.iter().map(|r| r.revision_id.as_str()).collect();
    let invalidated: BTreeMap<&str, &str> = effective
        .iter()
        .filter(|r| !revision_ids.contains(r.supersedes.as_str()))
        .map(|r| (r.supersedes.as_str(), r.revision_id.as_str()))
        .collect();

    let mut selected: BTreeMap<String, SelectedGrade> = BTreeMap::new();
    let mut non_winning: Vec<ExcludedAttempt> = Vec::new();
    let mut seen: BTreeMap<(String, u64), &GradeAttempt> = BTreeMap::new();
    let mut last_seq: BTreeMap<&str, u64> = BTreeMap::new();
    let mut watermarks: BTreeMap<String, u64> = BTreeMap::new();
    let mut admissible: BTreeSet<&str> = BTreeSet::new();

    for a in attempts {
        // Durable order within each run is the caller's obligation — a
        // regressed seq is a contract violation, not a silent reorder.
        if let Some(last) = last_seq.get(a.instrument_run_id.as_str()) {
            if a.seq < *last {
                return Err(SelectionError::OutOfOrder {
                    instrument_run_id: a.instrument_run_id.clone(),
                    seq: a.seq,
                });
            }
        }
        last_seq.insert(&a.instrument_run_id, a.seq);
        *watermarks.entry(a.instrument_run_id.clone()).or_insert(0) = a.seq;

        // One coordinate, one fact — a conflicting re-presentation is a
        // refusal; an identical one is a duplicate that cannot win twice.
        if let Some(prior) = seen.get(&(a.instrument_run_id.clone(), a.seq)) {
            if *prior == a {
                non_winning.push(ExcludedAttempt {
                    attempt_ref: a.attempt_ref.clone(),
                    reason: Exclusion::DuplicateInstrument,
                });
                continue;
            }
            return Err(SelectionError::ConflictingCoordinate {
                attempt_ref: a.attempt_ref.clone(),
            });
        }
        seen.insert((a.instrument_run_id.clone(), a.seq), a);

        if let Some(rev) = invalidated.get(a.attempt_ref.as_str()) {
            admissible.insert(&a.metric_ref);
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::Invalidated {
                    revision_id: rev.to_string(),
                },
            });
            continue;
        }
        let pin_miss = if a.subject_ref != set.subject_ref {
            Some("subject_ref")
        } else if a.submission_ref != set.submission_ref {
            Some("submission_ref")
        } else if a.oracle_ref != set.oracle_ref {
            Some("oracle_ref")
        } else {
            None
        };
        if let Some(member) = pin_miss {
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::PinMismatch { member },
            });
            continue;
        }
        if !set.members.contains(&a.metric_ref) {
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::SetMiss,
            });
            continue;
        }
        // Owner-validated causes: the attempt must cite its subject run
        // and name no run that is neither subject nor instrument.
        if !a.cause_refs.iter().any(|c| cause_run(c) == a.subject_ref) {
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::UncitedCauses,
            });
            continue;
        }
        if a.cause_refs
            .iter()
            .any(|c| cause_run(c) != a.subject_ref && cause_run(c) != a.instrument_run_id)
        {
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::ForeignCauses,
            });
            continue;
        }

        admissible.insert(&a.metric_ref);
        if !a.outcome.is_winning() {
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::Unavailable,
            });
            continue;
        }
        let GradeOutcome::Decided(value) = &a.outcome else {
            unreachable!()
        };
        if selected.contains_key(&a.metric_ref) {
            // First valid already won — a later higher reward cannot
            // displace it (never best-of, never last-write-wins).
            non_winning.push(ExcludedAttempt {
                attempt_ref: a.attempt_ref.clone(),
                reason: Exclusion::NotFirst,
            });
            continue;
        }
        selected.insert(
            a.metric_ref.clone(),
            SelectedGrade {
                metric_ref: a.metric_ref.clone(),
                attempt_ref: a.attempt_ref.clone(),
                instrument_run_id: a.instrument_run_id.clone(),
                seq: a.seq,
                value: value.clone(),
            },
        );
    }

    let members: BTreeMap<String, MemberStatus> = set
        .members
        .iter()
        .map(|m| {
            let status = match selected.get(m) {
                Some(g) => MemberStatus::Selected(g.clone()),
                None if admissible.contains(m.as_str()) => MemberStatus::Unavailable,
                None => MemberStatus::Pending,
            };
            (m.clone(), status)
        })
        .collect();

    Ok(GradeSelection {
        eligible_set_ref: set.eligible_set_ref(),
        members,
        non_winning,
        watermarks,
        revisions_applied: effective.iter().map(|r| r.revision_id.clone()).collect(),
    })
}

/// `select_grade(set, attempts, revisions)` — the first-valid selection
/// over the full presented prefix (all attempts, every effective
/// revision). `attempts` arrive in durable order.
pub fn select_grade(
    set: &ValidGradeSet,
    attempts: &[GradeAttempt],
    revisions: &[SelectionRevision],
) -> Result<GradeSelection, SelectionError> {
    select(set, attempts, &effective_revisions(set, revisions)?)
}

/// `select_grade_at(set, attempts, revisions, watermark)` — the same
/// selection resolved at a historical watermark: an attempt above its
/// run's pin does not exist yet, a revision whose `watermarks` the pin
/// does not cover is not applied, and every presented run must be pinned
/// (`WatermarkMissing`). The historical pin reproduces the historical
/// selection exactly.
pub fn select_grade_at(
    set: &ValidGradeSet,
    attempts: &[GradeAttempt],
    revisions: &[SelectionRevision],
    watermark: &BTreeMap<String, u64>,
) -> Result<GradeSelection, SelectionError> {
    let mut visible: Vec<GradeAttempt> = Vec::new();
    for a in attempts {
        match watermark.get(&a.instrument_run_id) {
            Some(pin) if a.seq <= *pin => visible.push(a.clone()),
            Some(_) => {}
            None => {
                return Err(SelectionError::WatermarkMissing {
                    run_id: a.instrument_run_id.clone(),
                });
            }
        }
    }
    let applied: Vec<SelectionRevision> = revisions
        .iter()
        .filter(|r| {
            r.watermarks
                .iter()
                .all(|(run, seq)| watermark.get(run).copied().unwrap_or(0) >= *seq)
        })
        .cloned()
        .collect();
    select(set, &visible, &effective_revisions(set, &applied)?)
}

/// The graded disposition — `plan_disposition_at`/`settle_graded` share
/// this composition: a valid grade restores the stop-derived outcome
/// class (possibly `budget_exhausted`); raw grade, stop and veto stay
/// separate axes. `Pending` is the `grade_pending` admission state —
/// never a seventh outcome class.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanDisposition {
    /// Some members still lack admissible evidence — `grade_pending`
    /// admission state; scored-success/completion interpretations block.
    Pending {
        /// Members without admissible evidence.
        missing: Vec<String>,
        /// Tripped vetoes — a separate axis, preserved.
        veto_tripped: Vec<String>,
    },
    /// Every member selected a grade.
    Graded {
        /// The stop-derived class, restored — may be `budget_exhausted`.
        outcome_class: OutcomeClass,
        /// `metric → raw grade` (a separate axis from the class).
        grades: BTreeMap<String, MetricValueKind>,
        /// `metric → winning attempt_ref` — the selection refs reports pin.
        selected_refs: BTreeMap<String, String>,
        /// Tripped vetoes — a separate axis, never folded into the grade.
        veto_tripped: Vec<String>,
        /// The applied revisions.
        revisions: Vec<String>,
    },
    /// Every member resolved with no valid grade — `oracle_failure`;
    /// revoked-unreplaced evidence lands here, not pending.
    OracleFailure {
        /// Members whose evidence is unavailable.
        missing: Vec<String>,
        /// Tripped vetoes — a separate axis, preserved.
        veto_tripped: Vec<String>,
    },
}

/// `plan_disposition(stop_class, veto_tripped, selection)` — compose the
/// graded disposition from the stop-derived base class, the tripped
/// vetoes and a [`GradeSelection`]. `stop_class` is what the run's stop
/// reason derived pre-grade (`derive_outcome_class`); a valid grade
/// restores it verbatim — `budget_exhausted` stays `budget_exhausted`.
pub fn plan_disposition(
    stop_class: OutcomeClass,
    veto_tripped: &[String],
    selection: &GradeSelection,
) -> PlanDisposition {
    let mut pending: Vec<String> = Vec::new();
    let mut unavailable: Vec<String> = Vec::new();
    for (m, s) in &selection.members {
        match s {
            MemberStatus::Selected(_) => {}
            MemberStatus::Unavailable => unavailable.push(m.clone()),
            MemberStatus::Pending => pending.push(m.clone()),
        }
    }
    if !pending.is_empty() {
        return PlanDisposition::Pending {
            missing: pending,
            veto_tripped: veto_tripped.to_vec(),
        };
    }
    if !unavailable.is_empty() {
        return PlanDisposition::OracleFailure {
            missing: unavailable,
            veto_tripped: veto_tripped.to_vec(),
        };
    }
    let mut grades = BTreeMap::new();
    let mut selected_refs = BTreeMap::new();
    for (m, s) in &selection.members {
        if let MemberStatus::Selected(g) = s {
            grades.insert(m.clone(), g.value.clone());
            selected_refs.insert(m.clone(), g.attempt_ref.clone());
        }
    }
    PlanDisposition::Graded {
        outcome_class: stop_class,
        grades,
        selected_refs,
        veto_tripped: veto_tripped.to_vec(),
        revisions: selection.revisions_applied.clone(),
    }
}
