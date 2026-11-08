//! `comparative_protocol` — the `comparative_protocol/1` materializer
//! (experiments spec §6.3 REQ-XD-020/021; §6.1 REQ-XD-002/004; §4.6
//! REQ-XC-010 under AR-12/ADR-0434; §10.5 AR-12; manifest 271/R6.18
//! stage 2 D1-a).
//!
//! A sibling of `central_protocol` — never an edit (D1 F2, C-11):
//! generalized to H ≥ 2 admitted harness arms, a declared T (the drawn
//! manifest from a consumed `task-selection/1` record) and a declared
//! k, reusing the exclusion, digest-ranked selection, reattempt and
//! phase-identity machinery and owning arms, k, the
//! schedule-randomization record and the declared-unsupported support
//! members.
//!
//! Phase 2 has no randomized treatment assignment — every harness runs
//! every task — so the exchangeability that licenses a sharp-null sign
//! flip comes from **randomizing the execution schedule**: a seeded
//! within-round order of (task, harness) slots in k rounds over
//! identical, declared lanes, recorded before execution as
//! `slot_schedule_randomization`. That spelling is the protocol's only
//! admitted exchangeability argument; an unrandomized kind or any
//! other spelling refuses typed (REQ-XD-021).
//!
//! Support members are declared facts, not gate predicates (ADR-0434):
//! an arm with every support verified claims `admitted_support`; any
//! declared-unsupported member (`support_seed = unverified` treated
//! false) carries the weaker `declared_unsupported` class — the gate
//! is amended by ADR, never satisfied by flipping booleans
//! (REQ-XC-010). `hh.fingerprint/v1` is never an identity gate.
//!
//! Refusals are typed, never warnings: H < 2, an unadmitted or
//! unpinned arm, a drawn task overlapping the development pool or
//! outside the consumed split, a split that does not predate access, a
//! relabelled pilot, an unrandomized schedule or foreign
//! exchangeability spelling, k < 5 for a confirmatory identity
//! (REQ-XD-002), a cross-tranche pooled estimand undeclared at the TR1
//! freeze or with changed version/effort/rewrite/catalog pins
//! (REQ-XD-004), unfrozen retry classes, wrong-answer retries, unknown
//! warmth or observed drift. Records-in, pure and total: same
//! declaration ⇒ byte-identical record and `protocol_id`.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::idp::idp_digest;
use hharness_lab::experiment::ReattemptPolicy;
use hharness_wire::Json;

use crate::central_protocol::{
    Warmth, MAX_REATTEMPTS_PER_PLAN, MAX_REATTEMPT_FRACTION_PPM, RETRYABLE_CLASSES,
};
use crate::population::MethodMetadata;
use crate::task_selection::{Pool, TaskSelection};

/// The materialization name (the S.1 `estimator` member).
pub const KIND: &str = "comparative_protocol";
/// The durable payload version (the spec's spelling, verbatim).
pub const VERSION: &str = "comparative_protocol/1";
/// The minimum arm count (H ≥ 2; REQ-XD-020).
pub const MIN_ARMS: usize = 2;
/// The confirmatory replicate floor (REQ-XD-002 — the R ≥ 5 cell
/// floor is not amended for phase 2).
pub const K_MIN_CONFIRM: u32 = 5;
/// The only admitted schedule kind — R6.16's seeded k-round
/// interleave (`hh-runner::schedule::ScheduleKind`).
pub const SCHEDULE_KIND: &str = "k_round_interleave";
/// The only admitted exchangeability spelling (REQ-XD-021).
pub const EXCHANGEABILITY: &str = "slot_schedule_randomization";
/// The support claim classes (ADR-0434 / AR-12).
pub const CLAIM_SUPPORTED: &str = "admitted_support";
/// The weaker claim class of AR-12 (ADR-0434).
pub const CLAIM_UNSUPPORTED: &str = "declared_unsupported";
/// The admitting amendment.
pub const AR12: &str = "ADR-0434";

const DOMAIN: &str = "comparative_protocol";
const DOMAIN_SCHED: &str = "comparative_protocol.schedule";
const DOMAIN_SLOT: &str = "comparative_protocol.slot";

fn rank(domain: &str, material: &str) -> String {
    idp_digest(domain, material.as_bytes())
}

fn unpinned(v: &str) -> bool {
    v.is_empty() || v == "latest" || v.ends_with(":latest") || v == "stable" || v == "HEAD"
}

// ── declared inputs (records-in) ───────────────────────────────────

/// The seed-support declaration (`unverified` is treated as false
/// until a recorded replay test passes — REQ-XC-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedSupport {
    /// A recorded replay test passed.
    Verified,
    /// Unverified — treated as false.
    Unverified,
}

/// One arm's declared support members (ADR-0434 — recorded facts,
/// never gate predicates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportDecl {
    /// `hh.fingerprint/v1` byte-equality support (`false` for
    /// glm-5.3-flash on Ollama — never an identity gate).
    pub fingerprint: bool,
    /// Provider-cache bypass support.
    pub cache: bool,
    /// Seed-honouring support.
    pub seed: SeedSupport,
}

impl SupportDecl {
    /// The claim class the supports admit under AR-12.
    pub fn claim_class(&self) -> &'static str {
        if self.fingerprint && self.cache && self.seed == SeedSupport::Verified {
            CLAIM_SUPPORTED
        } else {
            CLAIM_UNSUPPORTED
        }
    }
}

/// One admitted harness arm (F14's admission record pins it).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmDecl {
    /// The arm id.
    pub arm_id: String,
    /// The harness admission-record ref — empty is `UnadmittedArm`.
    pub admission_ref: String,
    /// The pinned harness version — `latest`/`stable`/empty is
    /// `UnpinnedHarness`.
    pub harness_version: String,
    /// The pinned reasoning-effort declaration.
    pub effort_pin: String,
    /// The served catalog identity (REQ-XC-119's tuple ref).
    pub catalog_identity: String,
    /// The proxy rewrite version.
    pub rewrite_version: String,
    /// The declared support members.
    pub supports: SupportDecl,
}

/// The pin set a pooled cross-tranche estimand requires unchanged
/// (REQ-XD-004: harness versions, effort, proxy rewrite version and
/// catalog identity).
#[derive(Debug, Clone, PartialEq)]
pub struct VersionPins {
    /// The harness version.
    pub harness_version: String,
    /// The effort pin.
    pub effort_pin: String,
    /// The proxy rewrite version.
    pub rewrite_version: String,
    /// The catalog identity.
    pub catalog_identity: String,
}

/// A cross-tranche pooled estimand declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Pooling {
    /// No pooled estimand — tranches are separate cohorts.
    NotRequested,
    /// Requested — admissible only when `declared_at_tr1_freeze` and
    /// the `prior` pins equal this protocol's.
    Requested {
        /// `true` — declared at the TR1 freeze.
        declared_at_tr1_freeze: bool,
        /// `arm_id → pins` recorded at the TR1 freeze.
        prior: BTreeMap<String, VersionPins>,
    },
}

/// The protocol's identity (k < 5 refuses `Confirmatory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// Confirmatory — k ≥ [`K_MIN_CONFIRM`].
    Confirmatory,
    /// Exploratory — the floor does not apply; labels still required.
    Exploratory,
}

impl Identity {
    fn name(&self) -> &'static str {
        match self {
            Identity::Confirmatory => "confirmatory",
            Identity::Exploratory => "exploratory",
        }
    }
}

/// The declared schedule — the seeded k-round interleave and the
/// protocol's only admitted exchangeability spelling.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleDecl {
    /// The kind — must be [`SCHEDULE_KIND`] (R6.16's seeded kind);
    /// anything else is an unrandomized order and refuses.
    pub kind: String,
    /// The order seed — the whole schedule recomputes from it.
    pub seed: String,
    /// The exchangeability argument — must be [`EXCHANGEABILITY`].
    pub exchangeability: String,
}

/// The materialization input — records-in.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparativeDecl {
    /// The consumed `task-selection/1` record.
    pub selection: TaskSelection,
    /// The drawn manifest — every task must sit in the consumed
    /// record's held-out pool.
    pub tasks: Vec<String>,
    /// The arms — at least [`MIN_ARMS`] distinct, all admitted/pinned.
    pub arms: Vec<ArmDecl>,
    /// The declared replicate slots per (task, arm).
    pub k: u32,
    /// The protocol identity.
    pub identity: Identity,
    /// The declared schedule.
    pub schedule: ScheduleDecl,
    /// The identical lanes (`lane_spec_ref` pins the shared spec).
    pub lanes: Vec<String>,
    /// The identical-lane specification pin.
    pub lane_spec_ref: String,
    /// The experiment's re-attempt policy (the same frozen ceilings as
    /// `central_protocol`: ≤2/plan, ≤10 % of plans, infrastructure
    /// classes only).
    pub reattempt: ReattemptPolicy,
    /// The warmth posture.
    pub warmth: Warmth,
    /// `true` = observed drift — collection stops, refuses.
    pub drift_observed: bool,
    /// `Some(label)` = the pilot is relabelled — refuses.
    pub pilot_labelled_as: Option<String>,
    /// The phase identities (`phase → id`; non-empty, distinct ids).
    pub phases: BTreeMap<String, String>,
    /// The frozen analysis-family identity.
    pub family_id: String,
    /// The many-to-one reference arm — must name an arm.
    pub reference_arm: String,
    /// The declared primaries (non-empty).
    pub primaries: Vec<String>,
    /// The frozen multiplicity-correction identity.
    pub multiplicity_id: String,
    /// The frozen stop-rule identities (non-empty).
    pub stops: Vec<String>,
    /// The pooled-estimand declaration.
    pub pooling: Pooling,
    /// `true` asserts `hh.fingerprint/v1` byte-equality as an identity
    /// gate — refuses (REQ-XC-010: never an identity gate).
    pub fingerprint_gate: bool,
}

// ── the materialized record ────────────────────────────────────────

/// One scheduled slot — `(round, task, arm)`; `replicate = round`.
#[derive(Debug, Clone, PartialEq)]
pub struct CompSlot {
    /// The slot identity (`H(seed ∥ task ∥ arm ∥ round)`).
    pub slot_ref: String,
    /// The round (`0..k`) — the replicate index.
    pub round: u32,
    /// The task.
    pub task_id: String,
    /// The arm.
    pub arm_id: String,
    /// `true` on the first slot of the `(task, arm)` pair (round 0 —
    /// first subjects are primary).
    pub first: bool,
}

/// The recorded schedule randomization — recorded before execution;
/// its digest is the REQ-XD-021 exchangeability record.
#[derive(Debug, Clone, PartialEq)]
pub struct RandomizationRecord {
    /// The schedule kind.
    pub kind: String,
    /// The order seed.
    pub seed: String,
    /// The rounds (= k).
    pub rounds: u32,
    /// The exchangeability spelling.
    pub exchangeability: String,
    /// `idp_digest` over the kind/seed/rounds/spelling and the emitted
    /// slot order — recomputes byte-identically.
    pub digest: String,
}

/// Recompute the randomization-record digest — the **one** construction
/// [`materialize_comparative`] seals and sign-flip consumers verify
/// (REQ-XD-021; CC1). `slot_order` is the emitted slot-ref order the
/// digest binds.
pub fn randomization_digest(record: &RandomizationRecord, slot_order: &[String]) -> String {
    let rand_json = Json::obj([
        ("kind", Json::str(&record.kind)),
        ("seed", Json::str(&record.seed)),
        ("rounds", Json::Int(record.rounds as i64)),
        ("exchangeability", Json::str(&record.exchangeability)),
        (
            "slot_order",
            Json::Arr(slot_order.iter().map(Json::str).collect()),
        ),
    ]);
    idp_digest(DOMAIN_SCHED, rand_json.to_canonical_string().as_bytes())
}

/// The evidence a sharp-null sign-flip consumer must carry to use
/// `slot_schedule_randomization`: the recorded randomization record
/// plus the emitted slot order its digest seals (REQ-XD-021; R6.19).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleEvidence {
    /// The recorded randomization record.
    pub record: RandomizationRecord,
    /// The emitted slot order (slot refs) the digest seals.
    pub slot_order: Vec<String>,
}

impl ScheduleEvidence {
    /// The evidence a materialized protocol carries.
    pub fn of(protocol: &ComparativeProtocol) -> ScheduleEvidence {
        ScheduleEvidence {
            record: protocol.randomization.clone(),
            slot_order: protocol
                .schedule
                .iter()
                .map(|s| s.slot_ref.clone())
                .collect(),
        }
    }

    /// `Ok(())` when the record carries the admitted kind and
    /// exchangeability spelling and its digest recomputes over the
    /// carried slot order; `Err(detail)` names the mismatch.
    pub fn check(&self) -> Result<(), String> {
        if self.record.kind != SCHEDULE_KIND {
            return Err(format!("kind {} is not {SCHEDULE_KIND}", self.record.kind));
        }
        if self.record.exchangeability != EXCHANGEABILITY {
            return Err(format!(
                "exchangeability {} is not {EXCHANGEABILITY}",
                self.record.exchangeability
            ));
        }
        if self.record.seed.is_empty() || self.record.rounds == 0 || self.slot_order.is_empty() {
            return Err("empty seed/rounds/slot_order".to_string());
        }
        let want = randomization_digest(&self.record, &self.slot_order);
        if want != self.record.digest {
            return Err("digest does not recompute over the carried slot order".to_string());
        }
        Ok(())
    }
}

/// One recorded arm with its derived claim class.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmRecord {
    /// The arm id.
    pub arm_id: String,
    /// The admission-record ref.
    pub admission_ref: String,
    /// The pinned harness version.
    pub harness_version: String,
    /// The effort pin.
    pub effort_pin: String,
    /// The catalog identity.
    pub catalog_identity: String,
    /// The proxy rewrite version.
    pub rewrite_version: String,
    /// The derived claim class (`admitted_support` |
    /// `declared_unsupported` under AR-12).
    pub claim_class: String,
    /// The echoed support declarations.
    pub supports: SupportDecl,
}

/// The materialized protocol — `protocol_id` is the `idp_digest`
/// commitment under `comparative_protocol` that must predate every
/// dependent subject.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparativeProtocol {
    /// `H(canonical(record minus protocol_id))`.
    pub protocol_id: String,
    /// The consumed selection record's id.
    pub selection_id: String,
    /// The drawn manifest (the tranche's T tasks).
    pub tasks: Vec<String>,
    /// The recorded arms.
    pub arms: Vec<ArmRecord>,
    /// The declared k.
    pub k: u32,
    /// The protocol identity.
    pub identity: String,
    /// The reference arm.
    pub reference_arm: String,
    /// The identical lanes.
    pub lanes: Vec<String>,
    /// The lane spec pin.
    pub lane_spec_ref: String,
    /// The schedule-randomization record.
    pub randomization: RandomizationRecord,
    /// The emitted slot order (`T·H·k` slots, rounds then seeded
    /// within-round order).
    pub schedule: Vec<CompSlot>,
    /// The phase identities.
    pub phases: BTreeMap<String, String>,
    /// The frozen analysis family.
    pub family_id: String,
    /// The declared primaries.
    pub primaries: Vec<String>,
    /// The multiplicity identity.
    pub multiplicity_id: String,
    /// The stop-rule identities.
    pub stops: Vec<String>,
    /// The pooling outcome (`none` | `admitted`).
    pub pooling: String,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

impl ComparativeProtocol {
    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("protocol_id");
        }
        self.protocol_id == idp_digest(DOMAIN, j.to_canonical_string().as_bytes())
    }

    /// The first-slot refs — one per `(task, arm)` (round 0).
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
            ("selection_id", Json::str(&self.selection_id)),
            (
                "tasks",
                Json::Arr(self.tasks.iter().map(Json::str).collect()),
            ),
            (
                "arms",
                Json::Arr(
                    self.arms
                        .iter()
                        .map(|a| {
                            Json::obj([
                                ("arm_id", Json::str(&a.arm_id)),
                                ("admission_ref", Json::str(&a.admission_ref)),
                                ("harness_version", Json::str(&a.harness_version)),
                                ("effort_pin", Json::str(&a.effort_pin)),
                                ("catalog_identity", Json::str(&a.catalog_identity)),
                                ("rewrite_version", Json::str(&a.rewrite_version)),
                                ("claim_class", Json::str(&a.claim_class)),
                                (
                                    "supports",
                                    Json::obj([
                                        ("fingerprint", Json::Bool(a.supports.fingerprint)),
                                        ("cache", Json::Bool(a.supports.cache)),
                                        (
                                            "seed",
                                            Json::str(match a.supports.seed {
                                                SeedSupport::Verified => "verified",
                                                SeedSupport::Unverified => "unverified",
                                            }),
                                        ),
                                    ]),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("k", Json::Int(self.k as i64)),
            ("identity", Json::str(&self.identity)),
            ("reference_arm", Json::str(&self.reference_arm)),
            (
                "lanes",
                Json::Arr(self.lanes.iter().map(Json::str).collect()),
            ),
            ("lane_spec_ref", Json::str(&self.lane_spec_ref)),
            (
                "randomization",
                Json::obj([
                    ("kind", Json::str(&self.randomization.kind)),
                    ("seed", Json::str(&self.randomization.seed)),
                    ("rounds", Json::Int(self.randomization.rounds as i64)),
                    (
                        "exchangeability",
                        Json::str(&self.randomization.exchangeability),
                    ),
                    ("digest", Json::str(&self.randomization.digest)),
                ]),
            ),
            (
                "schedule",
                Json::Arr(
                    self.schedule
                        .iter()
                        .map(|s| {
                            Json::obj([
                                ("slot_ref", Json::str(&s.slot_ref)),
                                ("round", Json::Int(s.round as i64)),
                                ("task_id", Json::str(&s.task_id)),
                                ("arm_id", Json::str(&s.arm_id)),
                                ("first", Json::Bool(s.first)),
                            ])
                        })
                        .collect(),
                ),
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
            ("pooling", Json::str(&self.pooling)),
            ("ar12", Json::str(AR12)),
            (
                "counts",
                Json::obj([
                    ("tasks", Json::Int(self.tasks.len() as i64)),
                    ("arms", Json::Int(self.arms.len() as i64)),
                    ("k", Json::Int(self.k as i64)),
                    (
                        "slots",
                        Json::Int((self.tasks.len() * self.arms.len()) as i64 * self.k as i64),
                    ),
                ]),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

// ── the refusal set ────────────────────────────────────────────────

/// The closed refusal set — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum ComparativeError {
    /// A required member is absent/empty.
    MissingKey { member: &'static str },
    /// Fewer than [`MIN_ARMS`] arms.
    TooFewArms { h: usize },
    /// An arm with no admission-record ref.
    UnadmittedArm { arm_id: String },
    /// An arm whose version is unpinned (`latest`/`stable`/empty).
    UnpinnedHarness { arm_id: String },
    /// An arm id declared twice.
    DuplicateArm { arm_id: String },
    /// The reference names no declared arm.
    UnknownReference { arm_id: String },
    /// A drawn task sits in the development pool — confirmatory draws
    /// read held-out only (REQ-XD-011).
    SplitOverlap { task_id: String },
    /// A drawn task is outside the consumed selection entirely.
    UnknownTask { task_id: String },
    /// The same task drawn twice.
    DuplicateTask { task_id: String },
    /// The consumed split does not predate access.
    LateSplit,
    /// The pilot was relabelled.
    PilotRelabel { label: String },
    /// The declared schedule kind is not the seeded k-round
    /// interleave — an unrandomized order (REQ-XD-021).
    UnrandomizedSchedule { kind: String },
    /// An exchangeability spelling other than
    /// `slot_schedule_randomization` — the protocol's only admitted
    /// argument.
    Exchangeability { spelling: String },
    /// k < [`K_MIN_CONFIRM`] for a confirmatory identity (REQ-XD-002).
    ConfirmatoryFloor { k: u32 },
    /// A pooled estimand not declared at the TR1 freeze (REQ-XD-004).
    PoolingUndeclared,
    /// A pooled estimand whose declared pins changed (REQ-XD-004).
    PoolingChanged {
        arm_id: String,
        member: &'static str,
    },
    /// `hh.fingerprint/v1` byte-equality asserted as an identity gate
    /// (REQ-XC-010 — never an identity gate).
    FingerprintGate,
    /// The re-attempt ceilings exceed ≤2/plan, ≤10 % plans.
    RetryCeiling { detail: String },
    /// `error_classes_included` unset — retry classes unfrozen.
    UnfrozenRetryClasses,
    /// A retry class outside the frozen infrastructure classes — a
    /// wrong answer retries.
    WrongAnswerRetry { class: String },
    /// Warmth unknown — the match fails.
    UnknownWarmth,
    /// Observed drift — collection stops.
    Drift,
    /// The lanes are not distinct under one spec.
    LaneConcurrency { detail: String },
    /// The phase identities are absent/empty/duplicated.
    PhaseIdentity { detail: String },
    /// The family/reference/multiplicity/primaries are invalid.
    FamilyMismatch { detail: String },
    /// The stop-rule set is empty or duplicated.
    StopIdentity { detail: String },
}

impl std::fmt::Display for ComparativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ComparativeError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::TooFewArms { h } => write!(f, "{h} arms; at least {MIN_ARMS} required"),
            E::UnadmittedArm { arm_id } => write!(f, "arm {arm_id} has no admission record"),
            E::UnpinnedHarness { arm_id } => write!(f, "arm {arm_id} version is unpinned"),
            E::DuplicateArm { arm_id } => write!(f, "arm {arm_id} declared twice"),
            E::UnknownReference { arm_id } => write!(f, "reference {arm_id} names no arm"),
            E::SplitOverlap { task_id } => {
                write!(f, "drawn task {task_id} sits in the development pool")
            }
            E::UnknownTask { task_id } => {
                write!(f, "drawn task {task_id} is outside the selection")
            }
            E::DuplicateTask { task_id } => write!(f, "task {task_id} drawn twice"),
            E::LateSplit => write!(f, "the split does not predate access"),
            E::PilotRelabel { label } => write!(f, "pilot relabelled as {label}"),
            E::UnrandomizedSchedule { kind } => {
                write!(
                    f,
                    "schedule kind {kind} is not the seeded k-round interleave"
                )
            }
            E::Exchangeability { spelling } => write!(
                f,
                "exchangeability {spelling} is not slot_schedule_randomization"
            ),
            E::ConfirmatoryFloor { k } => {
                write!(f, "k={k} below the confirmatory floor {K_MIN_CONFIRM}")
            }
            E::PoolingUndeclared => {
                write!(f, "pooled estimand not declared at the TR1 freeze")
            }
            E::PoolingChanged { arm_id, member } => {
                write!(f, "pooled estimand: {arm_id} {member} changed")
            }
            E::FingerprintGate => {
                write!(f, "hh.fingerprint/v1 asserted as an identity gate")
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
            E::LaneConcurrency { detail } => write!(f, "lanes invalid: {detail}"),
            E::PhaseIdentity { detail } => write!(f, "phase identity invalid: {detail}"),
            E::FamilyMismatch { detail } => write!(f, "analysis family invalid: {detail}"),
            E::StopIdentity { detail } => write!(f, "stop identity invalid: {detail}"),
        }
    }
}

impl std::error::Error for ComparativeError {}

// ── the materializer ───────────────────────────────────────────────

/// `materialize_comparative(decl)` — validate the supplied gated
/// inputs and emit the deterministic `comparative_protocol/1` record.
/// Pure and total: same inputs ⇒ byte-identical record and
/// `protocol_id`.
pub fn materialize_comparative(
    decl: &ComparativeDecl,
) -> Result<ComparativeProtocol, ComparativeError> {
    use ComparativeError as E;

    // ── missing keys (cheap first) ─────────────────────────────────
    for (member, empty) in [
        ("lane_spec_ref", decl.lane_spec_ref.is_empty()),
        ("schedule.seed", decl.schedule.seed.is_empty()),
        ("family_id", decl.family_id.is_empty()),
        ("multiplicity_id", decl.multiplicity_id.is_empty()),
        ("reference_arm", decl.reference_arm.is_empty()),
    ] {
        if empty {
            return Err(E::MissingKey { member });
        }
    }
    if decl.tasks.is_empty() {
        return Err(E::MissingKey { member: "tasks" });
    }
    if decl.k == 0 {
        return Err(E::MissingKey { member: "k" });
    }

    // ── the refusal matrix ─────────────────────────────────────────
    if !decl.selection.split_predates_access {
        return Err(E::LateSplit);
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
    if decl.fingerprint_gate {
        return Err(E::FingerprintGate);
    }

    // ── arms: H ≥ 2 distinct, admitted, pinned ─────────────────────
    if decl.arms.len() < MIN_ARMS {
        return Err(E::TooFewArms { h: decl.arms.len() });
    }
    let mut arm_ids: BTreeSet<&str> = BTreeSet::new();
    for a in &decl.arms {
        if a.arm_id.is_empty() {
            return Err(E::MissingKey {
                member: "arms[].arm_id",
            });
        }
        if !arm_ids.insert(a.arm_id.as_str()) {
            return Err(E::DuplicateArm {
                arm_id: a.arm_id.clone(),
            });
        }
        if a.admission_ref.is_empty() {
            return Err(E::UnadmittedArm {
                arm_id: a.arm_id.clone(),
            });
        }
        if unpinned(&a.harness_version) || unpinned(&a.effort_pin) {
            return Err(E::UnpinnedHarness {
                arm_id: a.arm_id.clone(),
            });
        }
    }
    if !arm_ids.contains(decl.reference_arm.as_str()) {
        return Err(E::UnknownReference {
            arm_id: decl.reference_arm.clone(),
        });
    }

    // ── the drawn manifest: held-out members of the consumed split ─
    let mut drawn: BTreeSet<&str> = BTreeSet::new();
    for t in &decl.tasks {
        if t.is_empty() {
            return Err(E::MissingKey { member: "tasks[]" });
        }
        if !drawn.insert(t.as_str()) {
            return Err(E::DuplicateTask { task_id: t.clone() });
        }
        match decl.selection.pool_of(t) {
            Some(Pool::HeldOut) => {}
            Some(Pool::Dev) => return Err(E::SplitOverlap { task_id: t.clone() }),
            None => return Err(E::UnknownTask { task_id: t.clone() }),
        }
    }

    // ── the k floor and the pooling rule ───────────────────────────
    if decl.identity == Identity::Confirmatory && decl.k < K_MIN_CONFIRM {
        return Err(E::ConfirmatoryFloor { k: decl.k });
    }
    let pooling = match &decl.pooling {
        Pooling::NotRequested => "none".to_string(),
        Pooling::Requested {
            declared_at_tr1_freeze,
            prior,
        } => {
            if !declared_at_tr1_freeze {
                return Err(E::PoolingUndeclared);
            }
            for (arm_id, pins) in prior {
                let arm = decl
                    .arms
                    .iter()
                    .find(|a| &a.arm_id == arm_id)
                    .ok_or_else(|| E::PoolingChanged {
                        arm_id: arm_id.clone(),
                        member: "arm_absent",
                    })?;
                for (member, prior_v, now_v) in [
                    (
                        "harness_version",
                        &pins.harness_version,
                        &arm.harness_version,
                    ),
                    ("effort_pin", &pins.effort_pin, &arm.effort_pin),
                    (
                        "rewrite_version",
                        &pins.rewrite_version,
                        &arm.rewrite_version,
                    ),
                    (
                        "catalog_identity",
                        &pins.catalog_identity,
                        &arm.catalog_identity,
                    ),
                ] {
                    if prior_v != now_v {
                        return Err(E::PoolingChanged {
                            arm_id: arm_id.clone(),
                            member,
                        });
                    }
                }
            }
            "admitted".to_string()
        }
    };

    // ── schedule kind and the exchangeability spelling ─────────────
    if decl.schedule.kind != SCHEDULE_KIND {
        return Err(E::UnrandomizedSchedule {
            kind: decl.schedule.kind.clone(),
        });
    }
    if decl.schedule.exchangeability != EXCHANGEABILITY {
        return Err(E::Exchangeability {
            spelling: decl.schedule.exchangeability.clone(),
        });
    }

    // ── lanes: distinct, non-empty, under one spec ─────────────────
    if decl.lanes.is_empty()
        || decl.lanes.iter().any(|l| l.is_empty())
        || decl.lanes.iter().collect::<BTreeSet<_>>().len() != decl.lanes.len()
    {
        return Err(E::LaneConcurrency {
            detail: "identical distinct lanes under one spec required".into(),
        });
    }

    // ── re-attempts: the same frozen ceilings ──────────────────────
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

    // ── phase/family/stops identities (the reused machinery — the
    // constants are this protocol's declared set) ───────────────────
    if decl.phases.is_empty() {
        return Err(E::PhaseIdentity {
            detail: "phase identities absent".into(),
        });
    }
    let mut phase_ids: BTreeSet<&str> = BTreeSet::new();
    for (p, id) in &decl.phases {
        if p.is_empty() || id.is_empty() || !phase_ids.insert(id.as_str()) {
            return Err(E::PhaseIdentity {
                detail: format!("phase {p} missing/empty/duplicated"),
            });
        }
    }
    if decl.primaries.is_empty() || decl.primaries.iter().any(|p| p.is_empty()) {
        return Err(E::FamilyMismatch {
            detail: "primaries empty".into(),
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

    // ── the seeded schedule: k rounds, seeded within-round order of
    // every (task, arm) slot — the REQ-XD-021 exchangeability record ─
    let mut schedule: Vec<CompSlot> =
        Vec::with_capacity(decl.tasks.len() * decl.arms.len() * decl.k as usize);
    for r in 0..decl.k {
        let mut keyed: Vec<(String, &str, &ArmDecl)> = Vec::new();
        for t in &decl.tasks {
            for a in &decl.arms {
                keyed.push((
                    rank(
                        DOMAIN_SCHED,
                        &format!("{}|{}|{}|{}", decl.schedule.seed, r, t, a.arm_id),
                    ),
                    t,
                    a,
                ));
            }
        }
        keyed.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(b.1))
                .then(a.2.arm_id.cmp(&b.2.arm_id))
        });
        for (_, t, a) in keyed {
            schedule.push(CompSlot {
                slot_ref: format!(
                    "slot:{}",
                    rank(
                        DOMAIN_SLOT,
                        &format!("{}|{}|{}|{}", decl.schedule.seed, t, a.arm_id, r),
                    )
                ),
                round: r,
                task_id: t.to_string(),
                arm_id: a.arm_id.clone(),
                first: r == 0,
            });
        }
    }
    let slot_order: Vec<String> = schedule.iter().map(|s| s.slot_ref.clone()).collect();
    let mut randomization = RandomizationRecord {
        kind: decl.schedule.kind.clone(),
        seed: decl.schedule.seed.clone(),
        rounds: decl.k,
        exchangeability: EXCHANGEABILITY.to_string(),
        digest: String::new(),
    };
    randomization.digest = randomization_digest(&randomization, &slot_order);

    let arms: Vec<ArmRecord> = decl
        .arms
        .iter()
        .map(|a| ArmRecord {
            arm_id: a.arm_id.clone(),
            admission_ref: a.admission_ref.clone(),
            harness_version: a.harness_version.clone(),
            effort_pin: a.effort_pin.clone(),
            catalog_identity: a.catalog_identity.clone(),
            rewrite_version: a.rewrite_version.clone(),
            claim_class: a.supports.claim_class().to_string(),
            supports: a.supports,
        })
        .collect();

    let metadata = MethodMetadata {
        estimator: KIND.to_string(),
        version: VERSION.to_string(),
        target: "comparative H-arm manifest and schedule-randomization identity".into(),
        assumptions: vec![
            "every arm runs every task; exchangeability is the randomized schedule".into(),
            "declared-unsupported support members admitted under AR-12's weaker class".into(),
            "first subjects primary; infrastructure-only retries".into(),
            "selection consumed from task-selection/1; held-out pool only".into(),
        ],
        prior: None,
        keys: BTreeMap::from([
            (
                "selection_id".to_string(),
                decl.selection.selection_id.clone(),
            ),
            ("schedule_seed".to_string(), decl.schedule.seed.clone()),
            ("reference_arm".to_string(), decl.reference_arm.clone()),
        ]),
        stratum_membership: decl
            .selection
            .cells
            .iter()
            .map(|c| format!("{}|{}", c.band, c.difficulty))
            .collect(),
        seed: 0,
        draws_requested: 0,
        draws_used: 0,
        floors: vec![
            format!("k>={K_MIN_CONFIRM}:confirmatory"),
            format!("arms>={MIN_ARMS}"),
        ],
        interval: ("descriptive".into(), None),
        refusal: None,
    };

    let mut out = ComparativeProtocol {
        protocol_id: String::new(),
        selection_id: decl.selection.selection_id.clone(),
        tasks: decl.tasks.clone(),
        arms,
        k: decl.k,
        identity: decl.identity.name().to_string(),
        reference_arm: decl.reference_arm.clone(),
        lanes: decl.lanes.clone(),
        lane_spec_ref: decl.lane_spec_ref.clone(),
        randomization,
        schedule,
        phases: decl.phases.clone(),
        family_id: decl.family_id.clone(),
        primaries: decl.primaries.clone(),
        multiplicity_id: decl.multiplicity_id.clone(),
        stops: decl.stops.clone(),
        pooling,
        metadata,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("protocol_id");
    }
    out.protocol_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
