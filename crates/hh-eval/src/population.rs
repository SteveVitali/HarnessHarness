//! `population` — the §3.1/§3.2 planned-slot population and equal-task
//! contrast bounds (R5.15; REQ-D-001/002/003/104, REQ-A-004/102; the
//! dossier S.1 shared population/metadata contract admitted at GATE-G9
//! — PADR-R5-4..7 common spine).
//!
//! The **one input/metadata contract** the method tickets (R5.16,
//! R5.36–41, receipt and study reports) consume: the caller presents the
//! planned-slot universe as records — slots keyed `(task, split,
//! stratum, arm, replicate, plan, attempt, subject)` — and `assemble`
//! reconciles it once:
//!
//! - **planned slots** — every presented slot is a required-universe
//!   member; an absent required member refuses (`IncompleteUniverse`),
//!   short cells keep their planned/not-run rows (M counts *slots*,
//!   never attempts);
//! - **first subjects** — the first bound subject is primary; the only
//!   recovery is the *first admissible completed retry* (flagged
//!   `recovered`, never best-of; a wrong answer never recovers);
//! - **all attempts** — every bound subject is retained and counted
//!   (attempt-level class/availability counts) without inflating n;
//! - **historical selection** — every resolved subject reads at a
//!   declared pin: above-pin is `HistoryLeak`, an unpinned resolved
//!   record is `WatermarkMissing` (the latest-history-leakage arm); the
//!   pins republish as `watermarks` so a historical analysis reproduces.
//!
//! The three §3.1 populations stay **distinct named sets** — `raw`,
//! `canonical`, `joint_task_pass_without_observed_veto` — beside the six
//! outcome classes, the veto axis and the availability axis; `unknown`
//! is never clean and an observed veto is never folded into a class.
//!
//! `contrast_bounds` computes `[s/M, (s+u)/M]` per (task, arm) and the
//! equal-task contrast `[Σw(L_test−U_control), Σw(U_test−L_control)]`,
//! `w_t=1/T` — whole unrun tasks retain weight (an unrun task
//! contributes its full-width `[0,1]` cell). The target is an explicit
//! `test_minus_control` (the protocol's B−A); the legacy `compare`
//! `a−b` path is untouched (AC-1). `BoundsDecl` admits only the binary
//! `Unit` rate — unbounded/censored metrics refuse, no invented finite
//! bound (§3.2; AC-2).

use std::collections::{BTreeMap, BTreeSet};

use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

use crate::stats::PPM;

/// Grade/evidence availability — a separate axis from outcome class and
/// veto (`grade_pending` is an admission state, never a seventh class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Availability {
    /// A valid grade/outcome resolved at the read pin.
    Resolved,
    /// `grade_pending` — withheld; cannot yield a full estimate.
    Pending,
    /// Admissible evidence exhausted, no valid grade (`oracle_failure`).
    Unavailable,
    /// Bound but nothing terminal/admissible yet.
    NotRun,
}

impl Availability {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Availability::Resolved => "resolved",
            Availability::Pending => "pending",
            Availability::Unavailable => "unavailable",
            Availability::NotRun => "not_run",
        }
    }
}

/// One bound subject on a planned slot (one attempt; records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotSubject {
    /// The subject run id — empty is `MissingSubjectKey`.
    pub subject_run: String,
    /// The attempt ordinal (`1` = first subject; `0` refuses).
    pub attempt_no: u32,
    /// The resolved outcome class at the read pin (`None` = unresolved —
    /// never guessed, never coerced).
    pub outcome: Option<OutcomeClass>,
    /// The slot measure's binary value for a resolved member; `None` =
    /// unknown.
    pub success: Option<bool>,
    /// Observed veto ids — a separate axis (§3.1).
    pub veto_tripped: Vec<String>,
    /// The grade-availability axis at the pin.
    pub availability: Availability,
    /// The durable seq the record was read at (`0` = unresolved — no
    /// durable fact to leak).
    pub seq: u64,
    /// Required observed checks absent at the pin — joint reads the
    /// member `unknown` (missing checks are not clean).
    pub missing_required_checks: bool,
    /// `false` = excluded/superseded — inadmissible for recovery,
    /// retained and counted.
    pub admissible: bool,
}

/// One declared planned slot — a member of the required universe.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedSlot {
    /// The plan coordinate (`run_plan_id`).
    pub plan_ref: String,
    /// The task (cluster unit).
    pub task_id: String,
    /// The task's split label (canonical spelling).
    pub split_label: String,
    /// The declared contamination stratum (canonical spelling).
    pub stratum: String,
    /// The arm.
    pub arm_id: String,
    /// The replicate/slot index.
    pub replicate_index: u64,
    /// Every bound subject (retries/regrades add no independent n).
    pub subjects: Vec<SlotSubject>,
}

/// The assembly input — records-in (the producer's plan/subject records
/// plus the read pins the historical read resolved at).
#[derive(Debug, Clone, Default)]
pub struct PopulationInput {
    /// Every presented planned slot.
    pub slots: Vec<PlannedSlot>,
    /// The required universe — the plan refs the analysis must cover
    /// (empty = the presented set *is* the universe).
    pub required: BTreeSet<String>,
    /// `subject_run → seq` — the declared read pins (the historical
    /// analysis coordinates).
    pub pins: BTreeMap<String, u64>,
}

/// The three named §3.1 populations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PopulationKind {
    /// `raw` — raw pass counts success, vetoes beside.
    Raw,
    /// `canonical` — the headline set; vetoed members leave it and are
    /// counted beside (never a silently improved denominator).
    Canonical,
    /// `joint_task_pass_without_observed_veto` — pass ∧ no observed
    /// veto; observed veto is a joint failure; missing checks `unknown`.
    JointTaskPassWithoutObservedVeto,
}

impl PopulationKind {
    /// The canonical spelling (the dossier's named sets).
    pub fn as_str(self) -> &'static str {
        match self {
            PopulationKind::Raw => "raw",
            PopulationKind::Canonical => "canonical",
            PopulationKind::JointTaskPassWithoutObservedVeto => {
                "joint_task_pass_without_observed_veto"
            }
        }
    }
}

/// The declared bound class of the measure the contrast computes over —
/// `contrast_bounds` admits the binary unit-rate target only.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundsDecl {
    /// The binary `[0,1]` task-success rate — the §3.2 planned bound.
    Unit,
    /// A declared physical `[lo,hi]` — admitted only when it *is* the
    /// unit interval; another finite bound is a different, separately
    /// owned target.
    Bounded { lo_ppm: i64, hi_ppm: i64 },
    /// Unbounded — no finite bound is fabricated (§3.2).
    Unbounded { detail: String },
    /// Right-censored — never a pretend cap-valued completion (§3.2).
    RightCensored { detail: String },
}

/// The closed refusal set — typed facts, never warnings (CC9/T-LCD-14).
#[derive(Debug, Clone, PartialEq)]
pub enum PopulationError {
    /// Two slots present the same `plan_ref`.
    ConflictingSlotKey { plan_ref: String },
    /// One `subject_run` binds more than one plan.
    DuplicateSubject { subject_run: String },
    /// A bound-subject record carries no `subject_run`.
    MissingSubjectKey { plan_ref: String },
    /// A required slot member is empty (`task_id`/`arm_id`).
    MissingKey {
        plan_ref: String,
        member: &'static str,
    },
    /// Two records claim the same `(plan, attempt_no)` or one subject
    /// claims two ordinals on a plan.
    ConflictingAttempt { plan_ref: String, attempt_no: u32 },
    /// A declared required member is absent — no full estimate.
    IncompleteUniverse { missing: Vec<String> },
    /// A resolved record (seq > 0) names no declared pin.
    WatermarkMissing { subject_run: String },
    /// A record reads above its pin — latest-history leakage refused.
    HistoryLeak {
        subject_run: String,
        seq: u64,
        pin: u64,
    },
    /// A task pairs slots for one arm only.
    UnpairedTask { task_id: String, arm_id: String },
    /// Zero planned slots — M=0 has no bound.
    EmptyUniverse,
    /// The measure is unbounded — no finite bound invented.
    UnboundedMetric { detail: String },
    /// The measure is right-censored — no cap-valued completion.
    CensoredMetric { detail: String },
    /// A finite bound other than the unit interval — a different target.
    UnsupportedBounds { detail: String },
}

impl std::fmt::Display for PopulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PopulationError as E;
        match self {
            E::ConflictingSlotKey { plan_ref } => write!(f, "conflicting slot key: {plan_ref}"),
            E::DuplicateSubject { subject_run } => {
                write!(f, "duplicate subject across plans: {subject_run}")
            }
            E::MissingSubjectKey { plan_ref } => {
                write!(f, "slot {plan_ref}: bound subject with no subject_run")
            }
            E::MissingKey { plan_ref, member } => write!(f, "slot {plan_ref}: missing {member}"),
            E::ConflictingAttempt {
                plan_ref,
                attempt_no,
            } => write!(f, "slot {plan_ref}: conflicting attempt_no {attempt_no}"),
            E::IncompleteUniverse { missing } => {
                write!(f, "incomplete required universe: missing {missing:?}")
            }
            E::WatermarkMissing { subject_run } => {
                write!(f, "resolved subject {subject_run} names no read pin")
            }
            E::HistoryLeak {
                subject_run,
                seq,
                pin,
            } => write!(f, "subject {subject_run} seq {seq} reads above pin {pin}"),
            E::UnpairedTask { task_id, arm_id } => {
                write!(f, "task {task_id}: no planned slot for arm {arm_id}")
            }
            E::EmptyUniverse => write!(f, "empty planned universe"),
            E::UnboundedMetric { detail } => {
                write!(f, "unbounded metric — no finite bound invented: {detail}")
            }
            E::CensoredMetric { detail } => {
                write!(f, "right-censored metric — no cap-valued bound: {detail}")
            }
            E::UnsupportedBounds { detail } => write!(f, "unsupported bounds target: {detail}"),
        }
    }
}

impl std::error::Error for PopulationError {}

/// How one slot resolved — the reconciliation record (never dropped).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotResolution {
    /// The plan coordinate.
    pub plan_ref: String,
    /// The task.
    pub task_id: String,
    /// The split label.
    pub split_label: String,
    /// The declared contamination stratum.
    pub stratum: String,
    /// The arm.
    pub arm_id: String,
    /// The replicate/slot index.
    pub replicate_index: u64,
    /// The primary subject — first bound subject by attempt ordinal.
    pub first_subject: Option<String>,
    /// The member read: first subject, else first admissible completed
    /// retry (then `recovered`). A wrong answer never recovers.
    pub member: Option<String>,
    /// `true` when `member` is a secondary-recovery retry.
    pub recovered: bool,
    /// The read member's outcome class (`None` = unresolved).
    pub outcome: Option<OutcomeClass>,
    /// The read member's binary value (`None` = unknown).
    pub success: Option<bool>,
    /// The read member's observed vetoes (separate axis).
    pub veto_tripped: Vec<String>,
    /// The read member's (or first subject's) availability.
    pub availability: Availability,
    /// Required checks missing on the member — joint: `unknown`.
    pub missing_required_checks: bool,
    /// Bound-subject count (all attempts retained).
    pub n_attempts: u64,
}

/// `{s,f,u}` plus the beside-counts each named population publishes
/// (§3.1–§3.2: base/clean/vetoed/check counts).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PopulationCounts {
    /// Known successes.
    pub s: u64,
    /// Known failures.
    pub f: u64,
    /// Unresolved — not-run/pending/non-base/unknown/veto/check.
    pub u: u64,
    /// Resolved members with ≥1 observed veto (beside, never folded).
    pub vetoed: u64,
    /// Vetoed *passes* — canonical excludes and reports beside.
    pub vetoed_passes: u64,
    /// Members with missing required checks (joint: `unknown`).
    pub missing_checks: u64,
}

/// The reconciled population — slots resolved once, the three named
/// sets beside the class/veto/availability axes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPopulation {
    /// One record per presented slot.
    pub slots: Vec<SlotResolution>,
    /// Per-kind counts, keyed by `PopulationKind::as_str`.
    pub populations: BTreeMap<String, PopulationCounts>,
    /// Task count in the presented universe.
    pub n_tasks: u64,
    /// Slot count (M — planned first slots; retries never inflate).
    pub n_slots: u64,
    /// Bound-subject count (all attempts).
    pub n_attempts: u64,
    /// `class → count` over first subjects (`unresolved` for none).
    pub first_class_counts: BTreeMap<String, u64>,
    /// `class → count` over all bound subjects (all attempts).
    pub attempt_class_counts: BTreeMap<String, u64>,
    /// `availability → count` over all bound subjects.
    pub availability_counts: BTreeMap<String, u64>,
    /// Plan refs whose first subject's admission is `pending`.
    pub pending_slots: Vec<String>,
    /// The declared read pins — the historical analysis coordinates.
    pub watermarks: BTreeMap<String, u64>,
    /// `true` when every slot resolved a member with a known value and
    /// nothing is pending — the precondition for a *full* estimate.
    pub complete: bool,
}

/// One resolved member's `(s,f,u)` contribution under `kind` — the
/// single rule `assemble`'s counts, `contrast_bounds`' cells and the
/// `finite_manifest` estimator's cells share (one counting rule — X7).
pub(crate) fn contrib(kind: PopulationKind, res: &SlotResolution) -> (u64, u64, u64) {
    let vetoed = !res.veto_tripped.is_empty();
    match kind {
        PopulationKind::Raw => match res.success {
            Some(true) => (1, 0, 0),
            Some(false) => (0, 1, 0),
            None => (0, 0, 1),
        },
        PopulationKind::Canonical => match (res.success, vetoed) {
            (Some(true), false) => (1, 0, 0),
            (Some(false), false) => (0, 1, 0),
            _ => (0, 0, 1),
        },
        PopulationKind::JointTaskPassWithoutObservedVeto => match res.success {
            Some(true) if !vetoed && !res.missing_required_checks => (1, 0, 0),
            Some(true) if res.missing_required_checks => (0, 0, 1),
            Some(_) => (0, 1, 0),
            None => (0, 0, 1),
        },
    }
}

/// `assemble` — the one reconciliation: validate the presented universe,
/// resolve each slot (first-subject primary / first-admissible-retry
/// recovery), count the three named populations beside the axes.
pub fn assemble(input: PopulationInput) -> Result<PlannedPopulation, PopulationError> {
    // ── keys ──────────────────────────────────────────────────────────
    let mut seen_plans: BTreeSet<&str> = BTreeSet::new();
    let mut subject_plan: BTreeMap<&str, &str> = BTreeMap::new();
    for slot in &input.slots {
        if !seen_plans.insert(slot.plan_ref.as_str()) {
            return Err(PopulationError::ConflictingSlotKey {
                plan_ref: slot.plan_ref.clone(),
            });
        }
        for (member, empty) in [
            ("task_id", slot.task_id.is_empty()),
            ("arm_id", slot.arm_id.is_empty()),
        ] {
            if empty {
                return Err(PopulationError::MissingKey {
                    plan_ref: slot.plan_ref.clone(),
                    member,
                });
            }
        }
        let mut seen_attempts: BTreeSet<u32> = BTreeSet::new();
        for sub in &slot.subjects {
            if sub.subject_run.is_empty() {
                return Err(PopulationError::MissingSubjectKey {
                    plan_ref: slot.plan_ref.clone(),
                });
            }
            if sub.attempt_no == 0 || !seen_attempts.insert(sub.attempt_no) {
                return Err(PopulationError::ConflictingAttempt {
                    plan_ref: slot.plan_ref.clone(),
                    attempt_no: sub.attempt_no,
                });
            }
            if let Some(owner) = subject_plan.insert(sub.subject_run.as_str(), &slot.plan_ref) {
                return Err(if owner == slot.plan_ref.as_str() {
                    PopulationError::ConflictingAttempt {
                        plan_ref: slot.plan_ref.clone(),
                        attempt_no: sub.attempt_no,
                    }
                } else {
                    PopulationError::DuplicateSubject {
                        subject_run: sub.subject_run.clone(),
                    }
                });
            }
        }
    }
    if !input.required.is_empty() {
        let missing: Vec<String> = input
            .required
            .iter()
            .filter(|r| !seen_plans.contains(r.as_str()))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(PopulationError::IncompleteUniverse { missing });
        }
    }

    // ── read pins (historical selection coordinates) ──────────────────
    for slot in &input.slots {
        for sub in &slot.subjects {
            if sub.seq == 0 {
                continue;
            }
            let pin = input.pins.get(sub.subject_run.as_str()).ok_or_else(|| {
                PopulationError::WatermarkMissing {
                    subject_run: sub.subject_run.clone(),
                }
            })?;
            if sub.seq > *pin {
                return Err(PopulationError::HistoryLeak {
                    subject_run: sub.subject_run.clone(),
                    seq: sub.seq,
                    pin: *pin,
                });
            }
        }
    }

    // ── per-slot resolution ───────────────────────────────────────────
    let mut slots = Vec::with_capacity(input.slots.len());
    let mut pending_slots = Vec::new();
    for slot in &input.slots {
        let mut subs: Vec<&SlotSubject> = slot.subjects.iter().collect();
        subs.sort_by_key(|s| s.attempt_no);
        let first = subs.first().copied();
        // First subject primary: an admissible resolved member stands (a
        // wrong answer never recovers). A first subject that is
        // inadmissible, unresolved, or resolved in a retryable
        // infrastructure class (`infrastructure_failure`/`oracle_failure`
        // — the frozen retry classes) reads the first admissible
        // completed retry. `Pending` holds the slot — pending grade
        // cannot yield a full estimate.
        fn retry<'a>(subs: &[&'a SlotSubject]) -> Option<(&'a SlotSubject, bool)> {
            subs.iter()
                .skip(1)
                .copied()
                .find(|s| {
                    s.admissible && s.outcome.is_some() && s.availability != Availability::Pending
                })
                .map(|s| (s, true))
        }
        let member: Option<(&SlotSubject, bool)> = match first {
            None => None,
            Some(f) if f.availability == Availability::Pending => None,
            Some(f) if !f.admissible || f.outcome.is_none() => retry(&subs),
            Some(f)
                if matches!(
                    f.outcome,
                    Some(OutcomeClass::InfrastructureFailure | OutcomeClass::OracleFailure)
                ) =>
            {
                retry(&subs).or(Some((f, false)))
            }
            Some(f) => Some((f, false)),
        };
        if first
            .map(|f| f.availability == Availability::Pending)
            .unwrap_or(false)
        {
            pending_slots.push(slot.plan_ref.clone());
        }
        let (member_ref, recovered) = member.map(|(s, r)| (Some(s), r)).unwrap_or((None, false));
        slots.push(SlotResolution {
            plan_ref: slot.plan_ref.clone(),
            task_id: slot.task_id.clone(),
            split_label: slot.split_label.clone(),
            stratum: slot.stratum.clone(),
            arm_id: slot.arm_id.clone(),
            replicate_index: slot.replicate_index,
            first_subject: first.map(|f| f.subject_run.clone()),
            member: member_ref.map(|s| s.subject_run.clone()),
            recovered,
            outcome: member_ref.and_then(|s| s.outcome),
            success: member_ref.and_then(|s| s.success),
            veto_tripped: member_ref
                .map(|s| s.veto_tripped.clone())
                .unwrap_or_default(),
            availability: member_ref.map(|s| s.availability).unwrap_or_else(|| {
                first
                    .map(|f| f.availability)
                    .unwrap_or(Availability::NotRun)
            }),
            missing_required_checks: member_ref
                .map(|s| s.missing_required_checks)
                .unwrap_or(false),
            n_attempts: slot.subjects.len() as u64,
        });
    }

    // ── populations + axes ────────────────────────────────────────────
    let mut populations: BTreeMap<String, PopulationCounts> = BTreeMap::new();
    for kind in [
        PopulationKind::Raw,
        PopulationKind::Canonical,
        PopulationKind::JointTaskPassWithoutObservedVeto,
    ] {
        let mut counts = PopulationCounts::default();
        for res in &slots {
            let (s, f, u) = contrib(kind, res);
            counts.s += s;
            counts.f += f;
            counts.u += u;
            if !res.veto_tripped.is_empty() {
                counts.vetoed += 1;
                if res.success == Some(true) {
                    counts.vetoed_passes += 1;
                }
            }
            if res.missing_required_checks {
                counts.missing_checks += 1;
            }
        }
        populations.insert(kind.as_str().to_string(), counts);
    }

    let mut first_class_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut attempt_class_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut availability_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut n_attempts = 0u64;
    for slot in &input.slots {
        let first_no = slot.subjects.iter().map(|s| s.attempt_no).min();
        for sub in &slot.subjects {
            let class = sub
                .outcome
                .map(|c| c.as_str())
                .unwrap_or("unresolved")
                .to_string();
            *attempt_class_counts.entry(class.clone()).or_insert(0) += 1;
            *availability_counts
                .entry(sub.availability.as_str().to_string())
                .or_insert(0) += 1;
            n_attempts += 1;
            if Some(sub.attempt_no) == first_no {
                *first_class_counts.entry(class).or_insert(0) += 1;
            }
        }
    }
    let complete = pending_slots.is_empty()
        && slots
            .iter()
            .all(|s| s.member.is_some() && s.success.is_some());
    Ok(PlannedPopulation {
        n_slots: slots.len() as u64,
        n_tasks: slots
            .iter()
            .map(|s| s.task_id.clone())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        n_attempts,
        slots,
        populations,
        first_class_counts,
        attempt_class_counts,
        availability_counts,
        pending_slots,
        watermarks: input.pins.clone(),
        complete,
    })
}

/// One arm's per-task cell bound — `[s/M, (s+u)/M]` ppm (computed inward,
/// rounded outward: lo floored, hi ceiled — dossier S.2).
#[derive(Debug, Clone, PartialEq)]
pub struct CellBound {
    /// Planned first slots (M).
    pub m: u64,
    /// Known successes.
    pub s: u64,
    /// Known failures.
    pub f: u64,
    /// Unresolved.
    pub u: u64,
    /// `⌊s·PPM/M⌋`.
    pub lo_ppm: i64,
    /// `⌈(s+u)·PPM/M⌉`.
    pub hi_ppm: i64,
}

/// One task's two arm cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskBound {
    /// The task.
    pub task_id: String,
    /// The test arm's cell.
    pub test: CellBound,
    /// The control arm's cell.
    pub control: CellBound,
}

/// The §3.2 equal-task contrast bound — an explicit `test_minus_control`
/// target (the protocol's B−A) on a named population.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastBounds {
    /// `"test_minus_control"` — the declared direction (never implied).
    pub direction: String,
    /// The test arm.
    pub test_arm: String,
    /// The control arm.
    pub control_arm: String,
    /// The named population the bound reads.
    pub population: String,
    /// Per-task cells, task-sorted.
    pub per_task: Vec<TaskBound>,
    /// `⌊Σ_t w_t (L_test − U_control)⌋`, `w_t = 1/T`.
    pub lo_ppm: i64,
    /// `⌈Σ_t w_t (U_test − L_control)⌉`.
    pub hi_ppm: i64,
    /// `true` only when every cell resolved (u=0 both arms, population
    /// complete) — the bounds collapse to the point a *full* estimate
    /// may report.
    pub full: bool,
    /// The S.1 method metadata record.
    pub metadata: MethodMetadata,
}

/// The S.1 method-metadata contract — every declared member, verbatim;
/// records-in facts only (a refusal is a member, never an omission).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodMetadata {
    /// The estimator name.
    pub estimator: String,
    /// The contract version.
    pub version: String,
    /// The estimand/target.
    pub target: String,
    /// The declared assumptions.
    pub assumptions: Vec<String>,
    /// The declared prior (`None` for non-Bayesian targets).
    pub prior: Option<String>,
    /// The binding keys (`population`, `test_arm`, `control_arm`).
    pub keys: BTreeMap<String, String>,
    /// The strata the population carries.
    pub stratum_membership: Vec<String>,
    /// The declared seed (0 — deterministic bounds, no draws).
    pub seed: u64,
    /// Requested draws (0 — no resampling).
    pub draws_requested: u64,
    /// Used draws (0).
    pub draws_used: u64,
    /// The declared floors.
    pub floors: Vec<String>,
    /// `(kind, level_ppm)` — `descriptive` bounds carry no level (S.6's
    /// fourth label kind; never confidence/credible/concentration).
    pub interval: (String, Option<i64>),
    /// The typed refusal, when the target could not compute.
    pub refusal: Option<String>,
}

impl MethodMetadata {
    /// The canonical JSON member set (S.1 spine, verbatim member names).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("estimator", Json::str(&self.estimator)),
            ("version", Json::str(&self.version)),
            ("target", Json::str(&self.target)),
            (
                "assumptions",
                Json::Arr(self.assumptions.iter().map(Json::str).collect()),
            ),
            (
                "prior",
                self.prior.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
            (
                "keys",
                Json::Obj(
                    self.keys
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            ),
            (
                "stratum_membership",
                Json::Arr(self.stratum_membership.iter().map(Json::str).collect()),
            ),
            ("seed", Json::Int(self.seed as i64)),
            ("draws_requested", Json::Int(self.draws_requested as i64)),
            ("draws_used", Json::Int(self.draws_used as i64)),
            (
                "floors",
                Json::Arr(self.floors.iter().map(Json::str).collect()),
            ),
            (
                "interval",
                Json::obj([
                    ("kind", Json::str(&self.interval.0)),
                    (
                        "level",
                        self.interval.1.map(Json::Int).unwrap_or(Json::Null),
                    ),
                ]),
            ),
            (
                "refusal",
                self.refusal.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
        ])
    }
}

fn floor_div(a: i128, b: i128) -> i64 {
    let q = a / b;
    let r = a % b;
    (q - i128::from(r != 0 && ((r < 0) != (b < 0)))) as i64
}

fn ceil_div(a: i128, b: i128) -> i64 {
    let q = a / b;
    let r = a % b;
    (q + i128::from(r != 0 && ((r < 0) == (b < 0)))) as i64
}

/// `contrast_bounds(pop, kind, test_arm, control_arm, bounds)` — the
/// §3.2 equal-task planned bound over the named population. `bounds`
/// admits the binary `Unit` rate only; unbounded/censored and non-unit
/// finite declarations refuse — no invented finite bound.
pub fn contrast_bounds(
    pop: &PlannedPopulation,
    kind: PopulationKind,
    test_arm: &str,
    control_arm: &str,
    bounds: &BoundsDecl,
) -> Result<ContrastBounds, PopulationError> {
    match bounds {
        BoundsDecl::Unit => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } if *lo_ppm == 0 && *hi_ppm == PPM => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } => {
            return Err(PopulationError::UnsupportedBounds {
                detail: format!(
                "declared bound [{lo_ppm},{hi_ppm}] is not the unit interval — a different target"
            ),
            })
        }
        BoundsDecl::Unbounded { detail } => {
            return Err(PopulationError::UnboundedMetric {
                detail: detail.clone(),
            })
        }
        BoundsDecl::RightCensored { detail } => {
            return Err(PopulationError::CensoredMetric {
                detail: detail.clone(),
            })
        }
    }
    if pop.slots.is_empty() {
        return Err(PopulationError::EmptyUniverse);
    }
    let tasks: BTreeSet<&str> = pop
        .slots
        .iter()
        .filter(|s| s.arm_id == test_arm || s.arm_id == control_arm)
        .map(|s| s.task_id.as_str())
        .collect();
    if tasks.is_empty() {
        return Err(PopulationError::EmptyUniverse);
    }
    let cell = |task: &str, arm: &str| -> Option<CellBound> {
        let mut c = CellBound {
            m: 0,
            s: 0,
            f: 0,
            u: 0,
            lo_ppm: 0,
            hi_ppm: 0,
        };
        let mut any = false;
        for res in pop
            .slots
            .iter()
            .filter(|s| s.task_id == task && s.arm_id == arm)
        {
            any = true;
            c.m += 1;
            let (ds, df, du) = contrib(kind, res);
            c.s += ds;
            c.f += df;
            c.u += du;
        }
        if !any {
            return None;
        }
        let m = i128::from(c.m);
        c.lo_ppm = floor_div(i128::from(c.s) * i128::from(PPM), m);
        c.hi_ppm = ceil_div((i128::from(c.s) + i128::from(c.u)) * i128::from(PPM), m);
        Some(c)
    };
    let mut per_task = Vec::new();
    let (mut lo_sum, mut hi_sum) = (0i128, 0i128);
    let mut full = pop.complete;
    for t in &tasks {
        let test = cell(t, test_arm).ok_or_else(|| PopulationError::UnpairedTask {
            task_id: (*t).to_string(),
            arm_id: test_arm.to_string(),
        })?;
        let control = cell(t, control_arm).ok_or_else(|| PopulationError::UnpairedTask {
            task_id: (*t).to_string(),
            arm_id: control_arm.to_string(),
        })?;
        lo_sum += i128::from(test.lo_ppm - control.hi_ppm);
        hi_sum += i128::from(test.hi_ppm - control.lo_ppm);
        if test.u > 0 || control.u > 0 {
            full = false;
        }
        per_task.push(TaskBound {
            task_id: (*t).to_string(),
            test,
            control,
        });
    }
    let t_n = i128::from(tasks.len() as u64);
    let mut keys = BTreeMap::new();
    keys.insert("population".into(), kind.as_str().to_string());
    keys.insert("test_arm".into(), test_arm.to_string());
    keys.insert("control_arm".into(), control_arm.to_string());
    Ok(ContrastBounds {
        direction: "test_minus_control".to_string(),
        test_arm: test_arm.to_string(),
        control_arm: control_arm.to_string(),
        population: kind.as_str().to_string(),
        per_task,
        lo_ppm: floor_div(lo_sum, t_n),
        hi_ppm: ceil_div(hi_sum, t_n),
        full,
        metadata: MethodMetadata {
            estimator: "planned_slot_equal_task_bounds".to_string(),
            version: "population/1".to_string(),
            target: "equal_task_contrast_bounds:test_minus_control".to_string(),
            assumptions: vec![
                "independent task clusters; equal within-stratum task weights w_t=1/T".into(),
                "first subject per slot primary; first admissible completed retry only".into(),
                "planned bounds, not an eligible-only point estimate".into(),
            ],
            prior: None,
            keys,
            stratum_membership: pop
                .slots
                .iter()
                .map(|s| s.stratum.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            seed: 0,
            draws_requested: 0,
            draws_used: 0,
            floors: Vec::new(),
            interval: ("descriptive".to_string(), None),
            refusal: None,
        },
    })
}

/// One arm's §3.2 all-planned bound — `[s/M,(s+u)/M]` per task and the
/// equal-task manifest bound `[⌊Σ w_t lo_t⌋, ⌈Σ w_t hi_t⌉]`, computed
/// with the same inward-compute/outward-round rule as
/// [`contrast_bounds`] (R6.19's single-arm θ_h companion; §6.8's
/// "all-planned bounds" member).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmBounds {
    /// The arm.
    pub arm_id: String,
    /// The named population the bound reads.
    pub population: String,
    /// Per-task cells, task-sorted.
    pub per_task: Vec<(String, CellBound)>,
    /// `⌊Σ_t w_t lo_t⌋`, `w_t = 1/T`.
    pub lo_ppm: i64,
    /// `⌈Σ_t w_t hi_t⌉`.
    pub hi_ppm: i64,
    /// `true` only when every cell resolved.
    pub full: bool,
}

/// `arm_bounds(pop, kind, arm)` — the single-arm all-planned bound over
/// the named population. One counting rule (`contrib`); an absent arm
/// is `EmptyUniverse`, never a zero bound.
pub fn arm_bounds(
    pop: &PlannedPopulation,
    kind: PopulationKind,
    arm: &str,
) -> Result<ArmBounds, PopulationError> {
    let mut per_task: BTreeMap<String, CellBound> = BTreeMap::new();
    for res in pop.slots.iter().filter(|s| s.arm_id == arm) {
        let c = per_task.entry(res.task_id.clone()).or_insert(CellBound {
            m: 0,
            s: 0,
            f: 0,
            u: 0,
            lo_ppm: 0,
            hi_ppm: 0,
        });
        c.m += 1;
        let (ds, df, du) = contrib(kind, res);
        c.s += ds;
        c.f += df;
        c.u += du;
    }
    if per_task.is_empty() {
        return Err(PopulationError::EmptyUniverse);
    }
    let mut full = pop.complete;
    let (mut lo_sum, mut hi_sum) = (0i128, 0i128);
    let mut cells = Vec::with_capacity(per_task.len());
    for (task_id, mut c) in per_task {
        let m = i128::from(c.m);
        c.lo_ppm = floor_div(i128::from(c.s) * i128::from(PPM), m);
        c.hi_ppm = ceil_div((i128::from(c.s) + i128::from(c.u)) * i128::from(PPM), m);
        if c.u > 0 {
            full = false;
        }
        lo_sum += i128::from(c.lo_ppm);
        hi_sum += i128::from(c.hi_ppm);
        cells.push((task_id, c));
    }
    let t_n = i128::from(cells.len() as u64);
    Ok(ArmBounds {
        arm_id: arm.to_string(),
        population: kind.as_str().to_string(),
        per_task: cells,
        lo_ppm: floor_div(lo_sum, t_n),
        hi_ppm: ceil_div(hi_sum, t_n),
        full,
    })
}
