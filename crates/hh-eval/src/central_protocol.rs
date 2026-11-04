//! `central_protocol` — deterministic central protocol materialization
//! (evidence spec §4.1; REQ-D-007/101/102/103, REQ-E-011, REQ-B-101/103/106;
//! AC-4-1/AC-3-2; methods partition M-H; R5.41).
//!
//! `materialize_protocol` consumes the **supplied** eligible
//! manifest/split/quotas/seed — every value an external gated input,
//! never chosen here — and yields the frozen finite-manifest protocol
//! record: the outcome-blind 30-task selection without replacement
//! under equal within-stratum weights, the task-level fair lane coin
//! assigning arms to identical lanes interleaved over five rounds, the
//! first-slot bindings, and the phase/analysis-family/stop identities.
//! `protocol_id` is the `idp_digest` commitment that must predate every
//! dependent subject (REQ-D-007 — a material change after subjects
//! requires a new cohort, never a silent rewrite).
//!
//! The scheduling members validate the owning §6.3 records:
//! `SchedulingPolicy` declares the interleaved order and ≤2 concurrent
//! subjects (the two lanes bind one arm each); `ReattemptPolicy`
//! declares the frozen retry classes/ceilings (≤2/plan, ≤10 % of
//! plans, infrastructure classes only — a wrong answer never retries;
//! the engine replans `infrastructure_failure` alone, ADR-0155 D5).
//!
//! Refusals are typed, never warnings (CC9/T-LCD-14): outcome-selected
//! exclusions, dev/held-out overlap or a late split, insufficient
//! held-out tasks, unknown warmth, drift, altered roles/fingerprints/
//! route/cache/seed support, a changed protocol after bound subjects,
//! pilot relabelling, a third arm, task-conditioned policy, artificial
//! pressure or any search budget. No participant data, no live study
//! execution, no scientific claim — engineered machinery only.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::idp::idp_digest;
use hharness_lab::experiment::{OrderKind, ReattemptPolicy, SchedulingPolicy};
use hharness_wire::Json;

use crate::population::MethodMetadata;

/// The materialization name (the S.1 `estimator` member).
pub const KIND: &str = "central_protocol";
/// The durable payload version.
pub const VERSION: &str = "central-protocol/1";
/// The selected held-out task count (§4.1/F2).
pub const TASKS: u32 = 30;
/// The interleaved rounds — replicate index is the round (R ≥ 5/task/arm).
pub const ROUNDS: u32 = 5;
/// The concurrent-subject cap (§4.1: at most two, one per arm).
pub const MAX_CONCURRENT_SUBJECTS: u32 = 2;
/// The re-attempt ceilings (§4.3/OQ-360: ≤2/plan, ≤10 % of plans).
pub const MAX_REATTEMPTS_PER_PLAN: u32 = 2;
/// The re-attempted-plan fraction ceiling (ppm of 1.0).
pub const MAX_REATTEMPT_FRACTION_PPM: u64 = 100_000;
/// The frozen primaries (§4.1).
pub const PRIMARIES: [&str; 3] = [
    "capability.task_success",
    "grounding.reacquisition_rate",
    "reliability.repeated_action_rate",
];
/// The six phase identities (§4.1/F2).
pub const PHASES: [&str; 6] = [
    "diagnostic",
    "activation",
    "pilot",
    "parity",
    "confirmation",
    "replication",
];
/// The frozen arm strategies — A=`evict_oldest`, B=`clear_tool_results`.
pub const ARM_STRATEGIES: [&str; 2] = ["evict_oldest", "clear_tool_results"];
/// The admitted outcome-blind exclusion bases (§4.1/F2); every other
/// basis refuses.
pub const OUTCOME_BLIND_EXCLUSION_BASES: [&str; 6] = [
    "immutable_surface",
    "runner_infeasible",
    "resource_isolation",
    "rights",
    "validity_contamination",
    "task_surface_protection",
];
/// The only retryable classes — frozen infrastructure classes (§4.3:
/// no wrong-answer retries; `oracle_failure` regrades, never re-runs).
pub const RETRYABLE_CLASSES: [&str; 1] = ["infrastructure_failure"];

const DOMAIN: &str = "central_protocol";
const DOMAIN_SELECT: &str = "central_protocol.select";
const DOMAIN_ORDER: &str = "central_protocol.order";
const DOMAIN_COIN: &str = "central_protocol.coin";
const DOMAIN_SLOT: &str = "central_protocol.slot";

fn rank(domain: &str, material: &str) -> String {
    idp_digest(domain, material.as_bytes())
}

// ── declared inputs (records-in; the caller resolves records — every
// value is an external gated input) ────────────────────────────────

/// The split a task sits in (the two-pool study partition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSplit {
    /// Development pool — pilot/exploratory access only.
    Dev,
    /// Held-out pool — the confirmation selection draws here.
    HeldOut,
}

/// One outcome-blind eligible task.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEntry {
    /// The pinned task identity.
    pub task_id: String,
    /// The assigned pool.
    pub split: TaskSplit,
    /// The declared metadata/contamination stratum.
    pub stratum: String,
}

/// One published exclusion — the basis must be outcome-blind.
#[derive(Debug, Clone, PartialEq)]
pub struct Exclusion {
    /// The excluded task.
    pub task_id: String,
    /// The declared basis — a member of
    /// [`OUTCOME_BLIND_EXCLUSION_BASES`].
    pub basis: String,
}

/// One frozen quota (stratum → required count).
#[derive(Debug, Clone, PartialEq)]
pub struct Quota {
    /// The stratum.
    pub stratum: String,
    /// The required selection count.
    pub required: u32,
}

/// One arm — the frozen strategy, artifact and model-role map.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmDecl {
    /// The arm id.
    pub arm_id: String,
    /// The strategy — a member of [`ARM_STRATEGIES`].
    pub strategy: String,
    /// The sealed artifact pin.
    pub artifact_ref: String,
    /// The arm's fixed model roles (`role → fingerprint`, incl. the
    /// unused summarizer profile) — must equal `required_roles`.
    pub roles: BTreeMap<String, String>,
}

/// The warmth posture — reset env/session plus cache bypass or
/// enforced cold namespace; unknown warmth fails matching (§4.1/F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warmth {
    /// Reset environment/session, cold start enforced.
    ColdStart,
    /// Provider cache bypassed.
    CacheBypassed,
    /// Enforced cold namespace.
    ColdNamespace,
    /// Unknown — refuses.
    Unknown,
}

/// The materialization input — records-in.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolDecl {
    /// Suite/catalogue pins (REQ-B-101 — exact revisions, never a tag).
    pub suite_ref: String,
    /// The pinned catalogue identity.
    pub catalogue_identity: String,
    /// The catalogue task count (identity member, not an estimand).
    pub catalogue_tasks: u64,
    /// The unchanged upstream oracle pin.
    pub oracle_ref: String,
    /// The original runner pin.
    pub runner_ref: String,
    /// The outcome-blind eligible tasks.
    pub tasks: Vec<TaskEntry>,
    /// The published exclusions (empty = none).
    pub exclusions: Vec<Exclusion>,
    /// The ref under which exclusions were published before outcomes
    /// (mandatory when `exclusions` is non-empty).
    pub exclusion_publication_ref: String,
    /// The registered `SplitAssignmentRecord` ref.
    pub split_assignment_ref: String,
    /// `true` — the assignment predates any pilot access.
    pub split_predates_access: bool,
    /// The frozen selection quotas (must total [`TASKS`]).
    pub quotas: Vec<Quota>,
    /// The committed selection seed.
    pub seed: String,
    /// The arms — exactly [`ARM_STRATEGIES`], one each.
    pub arms: Vec<ArmDecl>,
    /// The fixed model roles every arm must carry.
    pub required_roles: BTreeMap<String, String>,
    /// The two identical lanes (`lane_spec_ref` pins the shared spec).
    pub lanes: Vec<String>,
    /// The identical-lane specification pin.
    pub lane_spec_ref: String,
    /// The experiment's scheduling policy.
    pub scheduling: SchedulingPolicy,
    /// The experiment's re-attempt policy.
    pub reattempt: ReattemptPolicy,
    /// The warmth posture.
    pub warmth: Warmth,
    /// `true` = observed drift — collection stops, refuses.
    pub drift_observed: bool,
    /// Route/cache/seed/fingerprint support declarations — every one
    /// must hold (§4.1).
    pub support_route: bool,
    /// See `support_route`.
    pub support_cache: bool,
    /// See `support_route`.
    pub support_seed: bool,
    /// See `support_route`.
    pub support_fingerprint: bool,
    /// Must be `0` — the zero-search contrast.
    pub search_budget: u64,
    /// `true` = a task-conditioned policy was declared — refuses.
    pub task_conditioned: bool,
    /// `true` = artificial pressure/padding declared — refuses.
    pub artificial_pressure: bool,
    /// The phase identities (`phase → id`, all six [`PHASES`]).
    pub phases: BTreeMap<String, String>,
    /// The frozen analysis-family identity.
    pub family_id: String,
    /// The declared primaries — must equal [`PRIMARIES`].
    pub primaries: Vec<String>,
    /// The frozen multiplicity-correction identity (the Holm family).
    pub multiplicity_id: String,
    /// The frozen stop-rule identities (non-empty).
    pub stops: Vec<String>,
    /// `Some(label)` = the pilot is relabelled — refuses.
    pub pilot_labelled_as: Option<String>,
    /// `Some(n)` = this amends a prior protocol after `n` bound
    /// subjects — refuses unless `0`.
    pub amendment_subjects_bound: Option<u64>,
}

// ── the materialized record ───────────────────────────────────────

/// One selected task — its seeded order position and lane coin.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedTask {
    /// The task.
    pub task_id: String,
    /// The quota stratum it was drawn under.
    pub stratum: String,
    /// The seeded task-order position (`0..TASKS`).
    pub position: u32,
    /// The fair task-level coin (`0` = A on `lanes[0]`; `1` = swapped).
    pub coin: u8,
    /// `arm_id → lane` — the coin's assignment onto the two lanes.
    pub lane_of_arm: BTreeMap<String, String>,
}

/// One scheduled subject slot — `(round, task, arm, lane)`;
/// `replicate = round`, so five rounds give R = 5/task/arm.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleSlot {
    /// The slot identity (`H(seed ∥ task ∥ arm ∥ round)`).
    pub slot_ref: String,
    /// The interleaved round (`0..ROUNDS`) — the replicate index.
    pub round: u32,
    /// The task.
    pub task_id: String,
    /// The arm.
    pub arm_id: String,
    /// The lane the arm occupies for this task.
    pub lane_id: String,
    /// `true` on the first slot of the `(task, arm)` pair (round 0 —
    /// first subjects are primary).
    pub first: bool,
}

impl ScheduleSlot {
    fn to_json(&self) -> Json {
        Json::obj([
            ("slot_ref", Json::str(&self.slot_ref)),
            ("round", Json::Int(self.round as i64)),
            ("task_id", Json::str(&self.task_id)),
            ("arm_id", Json::str(&self.arm_id)),
            ("lane_id", Json::str(&self.lane_id)),
            ("first", Json::Bool(self.first)),
        ])
    }
}

/// The materialized protocol — the frozen identity every dependent
/// subject names (`protocol_id` is the predating hash).
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedProtocol {
    /// `H(canonical(record minus protocol_id))` under
    /// `central_protocol` — the REQ-D-007 commitment.
    pub protocol_id: String,
    /// The ordered 30-task selection.
    pub selection: Vec<SelectedTask>,
    /// The interleaved schedule (`TASKS × 2 × ROUNDS` slots).
    pub schedule: Vec<ScheduleSlot>,
    /// The recorded suite pins.
    pub suite_ref: String,
    /// The catalogue identity.
    pub catalogue_identity: String,
    /// The catalogue task count.
    pub catalogue_tasks: u64,
    /// The oracle pin.
    pub oracle_ref: String,
    /// The runner pin.
    pub runner_ref: String,
    /// The phase identities.
    pub phases: BTreeMap<String, String>,
    /// The frozen analysis family.
    pub family_id: String,
    /// The frozen primaries.
    pub primaries: Vec<String>,
    /// The frozen multiplicity identity.
    pub multiplicity_id: String,
    /// The frozen stop-rule identities.
    pub stops: Vec<String>,
    /// The S.1 method-metadata record (the M-A contract, consumed
    /// unchanged).
    pub metadata: MethodMetadata,
}

impl MaterializedProtocol {
    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("protocol_id");
        }
        self.protocol_id == idp_digest(DOMAIN, j.to_canonical_string().as_bytes())
    }

    /// The first-slot bindings — one ref per `(task, arm)` (the
    /// round-0 slots; first subjects are primary).
    pub fn first_slots(&self) -> Vec<&str> {
        self.schedule
            .iter()
            .filter(|s| s.first)
            .map(|s| s.slot_ref.as_str())
            .collect()
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("protocol_id", Json::str(&self.protocol_id)),
            (
                "suite",
                Json::obj([
                    ("suite_ref", Json::str(&self.suite_ref)),
                    ("catalogue_identity", Json::str(&self.catalogue_identity)),
                    ("catalogue_tasks", Json::Int(self.catalogue_tasks as i64)),
                    ("oracle_ref", Json::str(&self.oracle_ref)),
                    ("runner_ref", Json::str(&self.runner_ref)),
                ]),
            ),
            (
                "selection",
                Json::Arr(
                    self.selection
                        .iter()
                        .map(|t| {
                            Json::obj([
                                ("task_id", Json::str(&t.task_id)),
                                ("stratum", Json::str(&t.stratum)),
                                ("position", Json::Int(t.position as i64)),
                                ("coin", Json::Int(t.coin as i64)),
                                (
                                    "lane_of_arm",
                                    Json::Obj(
                                        t.lane_of_arm
                                            .iter()
                                            .map(|(a, l)| (a.clone(), Json::str(l)))
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "schedule",
                Json::Arr(self.schedule.iter().map(ScheduleSlot::to_json).collect()),
            ),
            (
                "first_slots",
                Json::Arr(self.first_slots().into_iter().map(Json::str).collect()),
            ),
            (
                "phases",
                Json::Obj(
                    self.phases
                        .iter()
                        .map(|(p, id)| (p.clone(), Json::str(id)))
                        .collect(),
                ),
            ),
            (
                "family",
                Json::obj([
                    ("family_id", Json::str(&self.family_id)),
                    (
                        "primaries",
                        Json::Arr(self.primaries.iter().map(Json::str).collect()),
                    ),
                    ("multiplicity_id", Json::str(&self.multiplicity_id)),
                ]),
            ),
            (
                "stops",
                Json::Arr(self.stops.iter().map(Json::str).collect()),
            ),
            (
                "counts",
                Json::obj([
                    ("tasks", Json::Int(TASKS as i64)),
                    ("arms", Json::Int(ARM_STRATEGIES.len() as i64)),
                    ("rounds", Json::Int(ROUNDS as i64)),
                    (
                        "subjects",
                        Json::Int((TASKS * ARM_STRATEGIES.len() as u32 * ROUNDS) as i64),
                    ),
                    ("first_slots", Json::Int(self.first_slots().len() as i64)),
                    (
                        "max_concurrent_subjects",
                        Json::Int(MAX_CONCURRENT_SUBJECTS as i64),
                    ),
                ]),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

// ── the refusal set ───────────────────────────────────────────────

/// The closed refusal set — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolError {
    /// A required member is absent/empty.
    MissingKey { member: &'static str },
    /// An exclusion's basis is not outcome-blind.
    OutcomeSelectedExclusion { task_id: String, basis: String },
    /// Exclusions exist with no before-outcomes publication ref.
    UnpublishedExclusions,
    /// A task sits in both pools (duplicate id, conflicting splits).
    DevOverlap { task_id: String },
    /// The same task appears twice in one pool.
    ConflictingTask { task_id: String },
    /// An excluded task is also eligible.
    ExclusionEligible { task_id: String },
    /// The split assignment does not predate pilot access.
    LateSplit,
    /// A held-out task's stratum carries no quota.
    UndeclaredStratum { stratum: String },
    /// A stratum appears twice in the quota table — the frozen table
    /// declares each stratum once; a repeat would silently re-select
    /// the same candidates (replacement).
    DuplicateStratum { stratum: String },
    /// The quotas do not total [`TASKS`].
    QuotaMismatch { declared: u32, required: u32 },
    /// Too few eligible held-out tasks — revision or stop, never
    /// replacement or a smaller silent manifest.
    InsufficientHeldOut {
        stratum: String,
        have: u32,
        need: u32,
    },
    /// The arms are not exactly [`ARM_STRATEGIES`].
    ArmSet { detail: String },
    /// `search_budget > 0` — adaptive search enters the contrast.
    PolicySearch { search_budget: u64 },
    /// A task-conditioned policy was declared.
    TaskConditioned,
    /// Artificial pressure/padding was declared.
    ArtificialPressure,
    /// `max_concurrent_runs` exceeds two concurrent subjects.
    ConcurrencyCap { declared: u32 },
    /// The lanes are not two distinct lanes under one spec (the
    /// one-per-arm assignment needs two identical lanes).
    ArmConcurrency { detail: String },
    /// The order policy is not interleaved.
    OrderPolicy { declared: String },
    /// The re-attempt ceilings exceed ≤2/plan, ≤10 % plans.
    RetryCeiling { detail: String },
    /// `error_classes_included` unset — retry classes unfrozen.
    UnfrozenRetryClasses,
    /// A retry class outside [`RETRYABLE_CLASSES`] — a wrong answer or
    /// non-infrastructure outcome retries.
    WrongAnswerRetry { class: String },
    /// Warmth unknown — the match fails, never a silent warm lane.
    UnknownWarmth,
    /// Observed drift — collection stops.
    Drift,
    /// A required support declaration failed.
    SupportMismatch { member: &'static str },
    /// An arm's roles differ from the fixed `required_roles`.
    RolesChanged { arm_id: String },
    /// The six [`PHASES`] identities are absent/empty/duplicated.
    PhaseIdentity { detail: String },
    /// The declared primaries differ from [`PRIMARIES`] or the
    /// family/multiplicity id is empty.
    FamilyMismatch { detail: String },
    /// The stop-rule set is empty or duplicated.
    StopIdentity { detail: String },
    /// A material protocol change after bound subjects — a new cohort
    /// is required, never a silent rewrite.
    ChangedAfterSubjects { subjects_bound: u64 },
    /// The pilot was relabelled (e.g. as confirmation).
    PilotRelabel { label: String },
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ProtocolError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::OutcomeSelectedExclusion { task_id, basis } => {
                write!(f, "exclusion of {task_id} is outcome-selected: {basis}")
            }
            E::UnpublishedExclusions => write!(f, "exclusions lack a publication ref"),
            E::DevOverlap { task_id } => write!(f, "task {task_id} sits in both pools"),
            E::ConflictingTask { task_id } => write!(f, "task {task_id} presented twice"),
            E::ExclusionEligible { task_id } => {
                write!(f, "excluded task {task_id} presented as eligible")
            }
            E::LateSplit => write!(f, "split assignment does not predate access"),
            E::UndeclaredStratum { stratum } => {
                write!(f, "held-out task in undeclared stratum {stratum}")
            }
            E::DuplicateStratum { stratum } => {
                write!(f, "stratum {stratum} declared twice in quotas")
            }
            E::QuotaMismatch { declared, required } => {
                write!(f, "quotas total {declared}, required {required}")
            }
            E::InsufficientHeldOut {
                stratum,
                have,
                need,
            } => {
                write!(f, "stratum {stratum}: {have} held-out < {need} required")
            }
            E::ArmSet { detail } => write!(f, "arm set invalid: {detail}"),
            E::PolicySearch { search_budget } => {
                write!(
                    f,
                    "search_budget {search_budget} enters the zero-search contrast"
                )
            }
            E::TaskConditioned => write!(f, "task-conditioned policy declared"),
            E::ArtificialPressure => write!(f, "artificial pressure/padding declared"),
            E::ConcurrencyCap { declared } => {
                write!(
                    f,
                    "max_concurrent_runs {declared} exceeds {MAX_CONCURRENT_SUBJECTS}"
                )
            }
            E::ArmConcurrency { detail } => write!(f, "lane/arm concurrency: {detail}"),
            E::OrderPolicy { declared } => {
                write!(f, "order {declared} cannot interleave the five rounds")
            }
            E::RetryCeiling { detail } => write!(f, "re-attempt ceiling exceeded: {detail}"),
            E::UnfrozenRetryClasses => write!(f, "retry classes unfrozen"),
            E::WrongAnswerRetry { class } => {
                write!(
                    f,
                    "retry class {class} is not a frozen infrastructure class"
                )
            }
            E::UnknownWarmth => write!(f, "warmth unknown — the match fails"),
            E::Drift => write!(f, "observed drift — collection stops"),
            E::SupportMismatch { member } => write!(f, "support mismatch: {member}"),
            E::RolesChanged { arm_id } => write!(f, "arm {arm_id} roles differ from frozen"),
            E::PhaseIdentity { detail } => write!(f, "phase identity invalid: {detail}"),
            E::FamilyMismatch { detail } => write!(f, "analysis family invalid: {detail}"),
            E::StopIdentity { detail } => write!(f, "stop identity invalid: {detail}"),
            E::ChangedAfterSubjects { subjects_bound } => {
                write!(
                    f,
                    "protocol change after {subjects_bound} subjects — new cohort"
                )
            }
            E::PilotRelabel { label } => write!(f, "pilot relabelled as {label}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

// ── the materializer ──────────────────────────────────────────────

/// `materialize_protocol(decl)` — validate the supplied gated inputs
/// and emit the deterministic protocol record. Pure and total: same
/// inputs ⇒ byte-identical record and `protocol_id`.
pub fn materialize_protocol(decl: &ProtocolDecl) -> Result<MaterializedProtocol, ProtocolError> {
    use ProtocolError as E;

    // ── missing keys (cheap first) ────────────────────────────────
    for (member, empty) in [
        ("suite_ref", decl.suite_ref.is_empty()),
        ("catalogue_identity", decl.catalogue_identity.is_empty()),
        ("oracle_ref", decl.oracle_ref.is_empty()),
        ("runner_ref", decl.runner_ref.is_empty()),
        ("seed", decl.seed.is_empty()),
        ("split_assignment_ref", decl.split_assignment_ref.is_empty()),
        ("lane_spec_ref", decl.lane_spec_ref.is_empty()),
    ] {
        if empty {
            return Err(E::MissingKey { member });
        }
    }
    if decl.catalogue_tasks == 0 {
        return Err(E::MissingKey {
            member: "catalogue_tasks",
        });
    }

    // ── change control / relabel / drift / warmth / support ───────
    if let Some(n) = decl.amendment_subjects_bound {
        if n > 0 {
            return Err(E::ChangedAfterSubjects { subjects_bound: n });
        }
    }
    if let Some(label) = &decl.pilot_labelled_as {
        return Err(E::PilotRelabel {
            label: label.clone(),
        });
    }
    if decl.drift_observed {
        return Err(E::Drift);
    }
    if matches!(decl.warmth, Warmth::Unknown) {
        return Err(E::UnknownWarmth);
    }
    for (member, ok) in [
        ("route", decl.support_route),
        ("cache", decl.support_cache),
        ("seed", decl.support_seed),
        ("fingerprint", decl.support_fingerprint),
    ] {
        if !ok {
            return Err(E::SupportMismatch { member });
        }
    }

    // ── zero-search / policy shape ────────────────────────────────
    if decl.search_budget > 0 {
        return Err(E::PolicySearch {
            search_budget: decl.search_budget,
        });
    }
    if decl.task_conditioned {
        return Err(E::TaskConditioned);
    }
    if decl.artificial_pressure {
        return Err(E::ArtificialPressure);
    }

    // ── arms: exactly evict_oldest + clear_tool_results ───────────
    if decl.arms.len() != ARM_STRATEGIES.len() {
        return Err(E::ArmSet {
            detail: format!("{} arms; exactly {} required", decl.arms.len(), 2),
        });
    }
    let mut by_strategy: BTreeMap<&str, &ArmDecl> = BTreeMap::new();
    let mut arm_ids: BTreeSet<&str> = BTreeSet::new();
    for arm in &decl.arms {
        if arm.arm_id.is_empty() || arm.artifact_ref.is_empty() {
            return Err(E::ArmSet {
                detail: "arm id/artifact empty".into(),
            });
        }
        if !arm_ids.insert(arm.arm_id.as_str()) {
            return Err(E::ArmSet {
                detail: format!("arm id {} duplicated", arm.arm_id),
            });
        }
        if !ARM_STRATEGIES.contains(&arm.strategy.as_str())
            || by_strategy.insert(arm.strategy.as_str(), arm).is_some()
        {
            return Err(E::ArmSet {
                detail: format!("strategy {} missing/duplicated", arm.strategy),
            });
        }
        if arm.roles != decl.required_roles {
            return Err(E::RolesChanged {
                arm_id: arm.arm_id.clone(),
            });
        }
    }
    let arm_a = by_strategy[ARM_STRATEGIES[0]];
    let arm_b = by_strategy[ARM_STRATEGIES[1]];

    // ── lanes/scheduling: two distinct lanes, ≤2 concurrent,
    // interleaved order ────────────────────────────────────────────
    if decl.lanes.len() != 2
        || decl.lanes.iter().any(|l| l.is_empty())
        || decl.lanes[0] == decl.lanes[1]
    {
        return Err(E::ArmConcurrency {
            detail: "two distinct lanes under one spec required".into(),
        });
    }
    if decl.scheduling.max_concurrent_runs > MAX_CONCURRENT_SUBJECTS {
        return Err(E::ConcurrencyCap {
            declared: decl.scheduling.max_concurrent_runs,
        });
    }
    if !matches!(
        decl.scheduling.order,
        OrderKind::Interleaved | OrderKind::InterleavedBlocked
    ) {
        return Err(E::OrderPolicy {
            declared: decl.scheduling.order.name().to_string(),
        });
    }

    // ── re-attempts: frozen classes, ≤2/plan, ≤10 % plans ─────────
    if decl.reattempt.max_per_plan > MAX_REATTEMPTS_PER_PLAN
        || decl.reattempt.max_fraction_of_plans_ppm > MAX_REATTEMPT_FRACTION_PPM
    {
        return Err(E::RetryCeiling {
            detail: format!(
                "max_per_plan {} / fraction {}ppm",
                decl.reattempt.max_per_plan, decl.reattempt.max_fraction_of_plans_ppm
            ),
        });
    }
    match &decl.reattempt.error_classes_included {
        None => return Err(E::UnfrozenRetryClasses),
        Some(classes) => {
            for c in classes {
                if !RETRYABLE_CLASSES.contains(&c.as_str()) {
                    return Err(E::WrongAnswerRetry { class: c.clone() });
                }
            }
        }
    }

    // ── exclusions: outcome-blind bases, published first ──────────
    for x in &decl.exclusions {
        if !OUTCOME_BLIND_EXCLUSION_BASES.contains(&x.basis.as_str()) {
            return Err(E::OutcomeSelectedExclusion {
                task_id: x.task_id.clone(),
                basis: x.basis.clone(),
            });
        }
    }
    if !decl.exclusions.is_empty() && decl.exclusion_publication_ref.is_empty() {
        return Err(E::UnpublishedExclusions);
    }
    if !decl.split_predates_access {
        return Err(E::LateSplit);
    }

    // ── pool uniqueness: one task, one pool; excluded ≠ eligible ──
    let excluded: BTreeSet<&str> = decl.exclusions.iter().map(|x| x.task_id.as_str()).collect();
    let mut pool: BTreeMap<&str, TaskSplit> = BTreeMap::new();
    for t in &decl.tasks {
        if t.task_id.is_empty() || t.stratum.is_empty() {
            return Err(E::MissingKey { member: "tasks[]" });
        }
        if excluded.contains(t.task_id.as_str()) {
            return Err(E::ExclusionEligible {
                task_id: t.task_id.clone(),
            });
        }
        match pool.insert(&t.task_id, t.split) {
            Some(p) if p != t.split => {
                return Err(E::DevOverlap {
                    task_id: t.task_id.clone(),
                })
            }
            Some(_) => {
                return Err(E::ConflictingTask {
                    task_id: t.task_id.clone(),
                })
            }
            None => {}
        }
    }

    // ── quotas total 30; each stratum declared once ───────────────
    let need_total: u32 = decl.quotas.iter().map(|q| q.required).sum();
    if need_total != TASKS {
        return Err(E::QuotaMismatch {
            declared: need_total,
            required: TASKS,
        });
    }
    let mut quota_strata: BTreeSet<&str> = BTreeSet::new();
    for q in &decl.quotas {
        if q.stratum.is_empty() {
            return Err(E::MissingKey {
                member: "quotas[].stratum",
            });
        }
        if !quota_strata.insert(q.stratum.as_str()) {
            return Err(E::DuplicateStratum {
                stratum: q.stratum.clone(),
            });
        }
    }
    let held_out: Vec<&TaskEntry> = decl
        .tasks
        .iter()
        .filter(|t| t.split == TaskSplit::HeldOut)
        .collect();
    for t in &held_out {
        if !decl.quotas.iter().any(|q| q.stratum == t.stratum) {
            return Err(E::UndeclaredStratum {
                stratum: t.stratum.clone(),
            });
        }
    }

    // ── phases/family/stops identities ────────────────────────────
    if decl.phases.len() != PHASES.len() {
        return Err(E::PhaseIdentity {
            detail: format!(
                "exactly the {} frozen phases; {} declared",
                PHASES.len(),
                decl.phases.len()
            ),
        });
    }
    let mut phase_ids: BTreeSet<&str> = BTreeSet::new();
    for phase in PHASES {
        match decl.phases.get(phase) {
            Some(id) if !id.is_empty() && phase_ids.insert(id.as_str()) => {}
            _ => {
                return Err(E::PhaseIdentity {
                    detail: format!("phase {phase} missing/empty/duplicated"),
                })
            }
        }
    }
    if decl.family_id.is_empty() || decl.multiplicity_id.is_empty() {
        return Err(E::FamilyMismatch {
            detail: "family/multiplicity id empty".into(),
        });
    }
    let mut prim = decl.primaries.clone();
    prim.sort();
    let mut frozen = PRIMARIES.to_vec();
    frozen.sort();
    if prim != frozen {
        return Err(E::FamilyMismatch {
            detail: "primaries differ from the frozen three".into(),
        });
    }
    if decl.stops.is_empty()
        || decl.stops.iter().any(|s| s.is_empty())
        || decl.stops.iter().collect::<BTreeSet<_>>().len() != decl.stops.len()
    {
        return Err(E::StopIdentity {
            detail: "stop-rule identities empty/duplicated".into(),
        });
    }

    // ── seeded selection without replacement (equal within-stratum
    // weights: one seeded rank per candidate) ──────────────────────
    let mut selected: Vec<&TaskEntry> = Vec::with_capacity(TASKS as usize);
    for q in &decl.quotas {
        let mut cands: Vec<&TaskEntry> = held_out
            .iter()
            .copied()
            .filter(|t| t.stratum == q.stratum)
            .collect();
        if (cands.len() as u32) < q.required {
            return Err(E::InsufficientHeldOut {
                stratum: q.stratum.clone(),
                have: cands.len() as u32,
                need: q.required,
            });
        }
        cands.sort_by(|a, b| {
            rank(
                DOMAIN_SELECT,
                &format!("{}|{}|{}", decl.seed, q.stratum, a.task_id),
            )
            .cmp(&rank(
                DOMAIN_SELECT,
                &format!("{}|{}|{}", decl.seed, q.stratum, b.task_id),
            ))
            .then(a.task_id.cmp(&b.task_id))
        });
        selected.extend(cands.into_iter().take(q.required as usize));
    }

    // ── order + fair task-level lane coin ─────────────────────────
    let mut ordered = selected;
    ordered.sort_by(|a, b| {
        rank(DOMAIN_ORDER, &format!("{}|{}", decl.seed, a.task_id))
            .cmp(&rank(DOMAIN_ORDER, &format!("{}|{}", decl.seed, b.task_id)))
            .then(a.task_id.cmp(&b.task_id))
    });
    let mut selection = Vec::with_capacity(TASKS as usize);
    let mut schedule = Vec::with_capacity((TASKS * 2 * ROUNDS) as usize);
    for (position, t) in ordered.iter().enumerate() {
        let coin = rank(DOMAIN_COIN, &format!("{}|{}", decl.seed, t.task_id)).as_bytes()[0] & 1;
        let (lane_a, lane_b) = if coin == 0 {
            (&decl.lanes[0], &decl.lanes[1])
        } else {
            (&decl.lanes[1], &decl.lanes[0])
        };
        selection.push(SelectedTask {
            task_id: t.task_id.clone(),
            stratum: t.stratum.clone(),
            position: position as u32,
            coin,
            lane_of_arm: BTreeMap::from([
                (arm_a.arm_id.clone(), lane_a.clone()),
                (arm_b.arm_id.clone(), lane_b.clone()),
            ]),
        });
        for r in 0..ROUNDS {
            for (arm, lane) in [(arm_a, lane_a), (arm_b, lane_b)] {
                schedule.push(ScheduleSlot {
                    slot_ref: format!(
                        "slot:{}",
                        rank(
                            DOMAIN_SLOT,
                            &format!("{}|{}|{}|{}", decl.seed, t.task_id, arm.arm_id, r),
                        )
                    ),
                    round: r,
                    task_id: t.task_id.clone(),
                    arm_id: arm.arm_id.clone(),
                    lane_id: lane.clone(),
                    first: r == 0,
                });
            }
        }
    }

    let metadata = MethodMetadata {
        estimator: KIND.to_string(),
        version: VERSION.to_string(),
        target: "finite-manifest selection/schedule identity".into(),
        assumptions: vec![
            "outcome-blind eligibility; exclusions published before outcomes".into(),
            "equal within-stratum weights; seeded sampling without replacement".into(),
            "task-level fair lane coin; identical lanes; five interleaved rounds".into(),
            "first subjects primary; infrastructure-only retries".into(),
        ],
        prior: None,
        keys: BTreeMap::from([
            ("suite_ref".to_string(), decl.suite_ref.clone()),
            ("selection_seed".to_string(), decl.seed.clone()),
            ("test_arm".to_string(), ARM_STRATEGIES[1].to_string()),
            ("control_arm".to_string(), ARM_STRATEGIES[0].to_string()),
        ]),
        stratum_membership: decl.quotas.iter().map(|q| q.stratum.clone()).collect(),
        seed: 0,
        draws_requested: 0,
        draws_used: 0,
        floors: vec![
            format!("replicates_per_pair>={ROUNDS}"),
            format!("max_concurrent_subjects={MAX_CONCURRENT_SUBJECTS}"),
            "one_per_arm".into(),
        ],
        interval: ("descriptive".into(), None),
        refusal: None,
    };

    let mut out = MaterializedProtocol {
        protocol_id: String::new(),
        selection,
        schedule,
        suite_ref: decl.suite_ref.clone(),
        catalogue_identity: decl.catalogue_identity.clone(),
        catalogue_tasks: decl.catalogue_tasks,
        oracle_ref: decl.oracle_ref.clone(),
        runner_ref: decl.runner_ref.clone(),
        phases: decl.phases.clone(),
        family_id: decl.family_id.clone(),
        primaries: decl.primaries.clone(),
        multiplicity_id: decl.multiplicity_id.clone(),
        stops: decl.stops.clone(),
        metadata,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("protocol_id");
    }
    out.protocol_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
