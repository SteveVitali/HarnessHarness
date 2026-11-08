//! `task_mixture` — task-mixture sensitivity, sharp-null families and
//! tails (R5.37; REQ-D-008 primary; GATE-G9-admitted Packet D /
//! PADR-R5-4* companions over the R5.15 population contract's cells,
//! dossier S.1–S.6; partition memo M-D).
//!
//! The whole-task paired bootstrap describes **task-mixture
//! sensitivity**, never the finite-manifest Bayesian target: per-task
//! deltas come from the same `(s, n)` cells `population::
//! contrast_bounds` resolves (one counting rule — X7), the task is
//! the cluster/pairing/resampling unit (no flattened event/replicate
//! `n`), and the `confidence`-labelled 95% interval consumes the
//! **declared** draws (`≥ 2,000` floor) and seed. BCa applies at
//! `T ≥ 30` tasks, percentile below — a contradicting declared method
//! refuses. The sharp-null sign flip runs only under the registered
//! `lane_randomization` exchangeability argument: exact enumeration
//! at `T ≤ 20`, else 8,000 Monte-Carlo draws at
//! `(extreme+1)/(draws+1)`. The frozen primary family is declared
//! complete — unavailable members stay in the Holm `α=.05` correction
//! at worst-case mass and split reports recompute identically;
//! exploratory BH is a separate correction. Signed/absolute effect
//! tails report `n`/`p50`/`p95`/`p99`/`max` with clustered-resampling
//! uncertainty; the observed max is a sample fact, never a population
//! bound. No omnibus verdict combines these distinct targets.

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::population::{
    contrast_bounds, BoundsDecl, MethodMetadata, PlannedPopulation, PopulationError, PopulationKind,
};
use crate::stats::{
    benjamini_hochberg, bootstrap_paired_with, holm, mean, permutation_signflip_p, quantile,
    sign_profile, Interval, XorShift64, PPM, SIGNFLIP_EXACT_MAX_N,
};

/// The analysis `kind` spelling that routes to this estimator.
pub const KIND: &str = "task_mixture_sensitivity";
/// The method contract version (report `metadata.version`).
pub const VERSION: &str = "task-mixture/1";
/// The frozen confidence level — 95%; any other is a different method.
pub const LEVEL_PPM: i64 = 950_000;
/// The confirmatory bootstrap draw floor (S.3 / §4.3).
pub const CONFIRMATORY_DRAWS_FLOOR: u64 = 2_000;
/// BCa is selected at this many tasks; percentile below (frozen floor).
pub const BCA_TASK_FLOOR: usize = 30;
/// The frozen Monte-Carlo draw count for `T > 20` sign flips (§4.3).
pub const SIGNFLIP_DRAWS: u64 = 8_000;
/// The registered exchangeability argument — the only admitted
/// spelling (the fair task-level lane-randomization coin, §4.1).
pub const REGISTERED_EXCHANGEABILITY: &str = "lane_randomization";
/// The second admitted spelling (§6.3 REQ-XD-021; R6.19) — the seeded
/// schedule randomization, admitted **only** with the carried
/// randomization record ([`crate::comparative_protocol::ScheduleEvidence`]).
pub const SLOT_SCHEDULE_EXCHANGEABILITY: &str = "slot_schedule_randomization";
/// The only admitted null spelling (never weak-mean/practical-margin).
pub const SHARP_NULL: &str = "sharp_no_effect";
/// The frozen Holm level — `α = .05` over the primary family.
pub const ALPHA_PPM: i64 = 50_000;
/// The first-slot replicate floor per `(task, arm)` cell (§4.1).
pub const CELL_REPLICATE_FLOOR: u64 = 5;

/// One frozen multiplicity-family member as declared by the spec.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyMember {
    /// The frozen member id (e.g. `capability.task_success:held_out`).
    pub member: String,
    /// The member's raw sign-flip p (ppm) evidenced in a sibling
    /// report — `None` retains it `unavailable`, never dropped.
    pub raw_p_ppm: Option<i64>,
}

/// The frozen primary family — `member` names this report's own slot;
/// `members` is the complete frozen set (unavailable included).
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyDecl {
    /// The frozen family identity (missing/empty refuses).
    pub id: String,
    /// This report's own member id — must be a declared member.
    pub member: String,
    /// The complete frozen member set.
    pub members: Vec<FamilyMember>,
}

/// A named exploratory family (BH — always separate from primary
/// Holm).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedFamily {
    /// The frozen exploratory family identity.
    pub id: String,
    /// Its declared members.
    pub members: Vec<FamilyMember>,
}

/// The declared method inputs (records-in — the caller reads the spec;
/// this module never sees filters/JSON spec members).
#[derive(Debug, Clone)]
pub struct TaskMixtureDecl {
    /// The named population the cells read.
    pub population: PopulationKind,
    /// The test arm (the `test_minus_control` minuend).
    pub test_arm: String,
    /// The control arm (the subtrahend).
    pub control_arm: String,
    /// The declared contamination stratum (`None` requires the
    /// candidates to already span exactly one stratum).
    pub stratum: Option<String>,
    /// The declared bootstrap method — must equal the floor-selected
    /// method (`bca` iff `T ≥ 30`): contradiction refuses.
    pub bootstrap: String,
    /// `Some("sharp_no_effect")` requests the sign-flip companion; any
    /// other spelling refuses.
    pub null: Option<String>,
    /// The declared exchangeability argument — must be
    /// [`REGISTERED_EXCHANGEABILITY`] when the sign-flip runs.
    pub exchangeability: Option<String>,
    /// The declared dependence model — `None`/`"independent"` only.
    pub dependence: Option<String>,
    /// The declared downstream use — omnibus/weak-mean/
    /// practical-margin/TOST spellings refuse.
    pub verdict: Option<String>,
    /// The declared bound class — the binary unit rate only.
    pub bounds: BoundsDecl,
    /// The declared level — must equal [`LEVEL_PPM`].
    pub level_ppm: i64,
    /// The declared bootstrap draws — `≥` [`CONFIRMATORY_DRAWS_FLOOR`].
    pub draws: u64,
    /// The declared seed (mixed into the canonical-input material).
    pub seed: u64,
    /// The frozen primary family (absent = no Holm leg).
    pub family: Option<FamilyDecl>,
    /// The frozen exploratory family (absent = no BH leg).
    pub exploratory: Option<NamedFamily>,
    /// The R6.18 schedule-randomization evidence — required when
    /// `exchangeability == "slot_schedule_randomization"` (REQ-XD-021;
    /// R6.19). `None` refuses `ExchangeabilityNotAdmitted`.
    pub schedule_randomization: Option<crate::comparative_protocol::ScheduleEvidence>,
    /// The declared downstream use — `confirmatory`/`first_pass` uses
    /// of the new `slot_schedule_randomization` spelling require a
    /// GATE-G23 PASSED record (REQ-XD-023; absent reads `confirmatory`,
    /// the strict default); `descriptive`/`exploratory` compute.
    pub use_class: Option<String>,
    /// The method-admission record for the new spelling.
    pub admission: Option<crate::comparative_estimators::AdmissionRecord>,
}

/// The closed refusal set — typed facts, never warnings.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskMixtureError {
    /// The population contract refused (missing keys, unpaired task,
    /// empty universe — the same [`PopulationError`] vocabulary).
    Population(PopulationError),
    /// Fewer than two tasks — whole-task resampling needs `T ≥ 2`
    /// (one task permits raw observations only).
    OneTask,
    /// A `(task, arm)` cell carries unresolved members (missing
    /// clusters cannot carry the inference).
    UnresolvedCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Unresolved members in the cell.
        u: u64,
    },
    /// A `(task, arm)` cell is below the `R ≥ 5` first-slot floor.
    ShortCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Resolved trials in the cell.
        n: u64,
    },
    /// Fewer than two distinct per-task deltas — a point-mass resample
    /// distribution; no interval is invented.
    DegenerateBootstrap,
    /// The declared bootstrap method contradicts the frozen
    /// `T ≥ 30` BCa floor.
    BootstrapFloorViolation {
        /// The declared method.
        declared: String,
        /// The task count the floor selects on.
        tasks: usize,
        /// The method the floor selects.
        selected: String,
    },
    /// Declared draws below the ≥2,000 confirmatory floor.
    DrawsBelowFloor {
        /// The declared draws.
        declared: u64,
    },
    /// A dependence model other than independent task clusters was
    /// declared (shared shocks/carryover are unsupported).
    UnsupportedDependence {
        /// The declared dependence.
        declared: String,
    },
    /// A null other than the sharp no-effect null was declared.
    UnsupportedNull {
        /// The declared null.
        declared: String,
    },
    /// The sharp-null test was requested without the registered
    /// lane-randomization exchangeability argument.
    ExchangeabilityNotAdmitted {
        /// The declared exchangeability (`"(absent)"` when none).
        declared: String,
    },
    /// A downstream use that would fuse these distinct targets into
    /// an omnibus/practical-margin/TOST certificate.
    UnsupportedVerdict {
        /// The declared use.
        declared: String,
    },
    /// The frozen family declaration is malformed — missing identity,
    /// empty member set, or this report's member not in the family.
    FamilyIdentity {
        /// The detail.
        detail: String,
    },
    /// A declared interval level other than the frozen 95%.
    UnsupportedLevel {
        /// The declared level (ppm).
        declared: i64,
    },
    /// A declared numeric/bound input outside its finite range.
    NonFiniteInput {
        /// The detail.
        detail: String,
    },
    /// A method name outside the admitted set was declared.
    UnknownMethod {
        /// The declared method.
        declared: String,
    },
    /// The method was declared on an analysis `kind` that does not
    /// compute it — a metadata-only declaration fails.
    MetadataOnly {
        /// The kind the declaration rode on.
        kind: String,
    },
    /// The candidate slots span more than one contamination stratum
    /// and no `stratum` was declared — strata are never pooled.
    StrataSelectionRequired {
        /// The strata the candidates carry.
        strata: Vec<String>,
    },
    /// The carried randomization record fails its recompute or the
    /// admitted kind/spelling (REQ-XD-021; R6.19).
    RandomizationRecord {
        /// The mismatch detail.
        detail: String,
    },
    /// A confirmatory/first-pass use of `slot_schedule_randomization`
    /// without a GATE-G23 PASSED record (REQ-XD-023; R6.19).
    MethodNotAdmitted {
        /// The new piece.
        piece: String,
        /// The gate whose PASSED record is owed.
        gate: String,
    },
}

impl std::fmt::Display for TaskMixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TaskMixtureError as E;
        match self {
            E::Population(e) => write!(f, "{e}"),
            E::OneTask => write!(f, "one task — whole-task resampling requires T >= 2"),
            E::UnresolvedCell { task_id, arm_id, u } => {
                write!(f, "estimator_unavailable: ({task_id}, {arm_id}) has {u} unresolved")
            }
            E::ShortCell { task_id, arm_id, n } => {
                write!(f, "insufficient_replicates: ({task_id}, {arm_id}) n={n} < {CELL_REPLICATE_FLOOR}")
            }
            E::DegenerateBootstrap => write!(f, "degenerate bootstrap: <2 distinct task deltas"),
            E::BootstrapFloorViolation { declared, tasks, selected } => write!(
                f,
                "bootstrap floor: declared {declared} but T={tasks} selects {selected} (BCa at >= {BCA_TASK_FLOOR})"
            ),
            E::DrawsBelowFloor { declared } => {
                write!(f, "draws {declared} below confirmatory floor {CONFIRMATORY_DRAWS_FLOOR}")
            }
            E::UnsupportedDependence { declared } => {
                write!(f, "unsupported dependence declared: {declared}")
            }
            E::UnsupportedNull { declared } => {
                write!(f, "unsupported null: {declared} — only {SHARP_NULL} is admitted")
            }
            E::ExchangeabilityNotAdmitted { declared } => write!(
                f,
                "exchangeability not admitted: {declared} — sharp-null requires {REGISTERED_EXCHANGEABILITY}"
            ),
            E::UnsupportedVerdict { declared } => {
                write!(f, "unsupported verdict use: {declared} — no omnibus/margin certificate")
            }
            E::FamilyIdentity { detail } => write!(f, "frozen family identity malformed: {detail}"),
            E::UnsupportedLevel { declared } => {
                write!(f, "unsupported level {declared} — frozen at {LEVEL_PPM} ppm")
            }
            E::NonFiniteInput { detail } => write!(f, "nonfinite input: {detail}"),
            E::UnknownMethod { declared } => write!(f, "unknown method declared: {declared}"),
            E::MetadataOnly { kind } => {
                write!(f, "metadata-only method switch refused on kind {kind}")
            }
            E::StrataSelectionRequired { strata } => {
                write!(f, "candidate slots span strata {strata:?} — declare filters.stratum")
            }
            E::RandomizationRecord { detail } => {
                write!(f, "randomization record mismatch: {detail}")
            }
            E::MethodNotAdmitted { piece, gate } => write!(
                f,
                "method not admitted: {piece} refuses confirmatory/first-pass use until a {gate} PASSED record exists (REQ-XD-023)"
            ),
        }
    }
}

impl std::error::Error for TaskMixtureError {}

impl From<PopulationError> for TaskMixtureError {
    fn from(e: PopulationError) -> TaskMixtureError {
        TaskMixtureError::Population(e)
    }
}

/// The sign-flip leg's outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum SignFlip {
    /// Not requested (`filters.null` absent).
    NotDeclared,
    /// Computed under the admitted exchangeability argument.
    Computed {
        /// `exact` (`T ≤ 20` full enumeration) or `monte_carlo`.
        mode: String,
        /// The admitted exchangeability spelling the p ran under
        /// (`lane_randomization` | `slot_schedule_randomization`).
        exchangeability: String,
        /// Monte-Carlo draws actually consumed (0 for exact).
        draws: u64,
        /// Two-sided p in ppm (`(extreme+1)/(draws+1)` for MC; exact
        /// enumeration needs no smoothing).
        p_ppm: i64,
    },
}

/// One multiplicity member's report row.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyRow {
    /// The frozen member id.
    pub member: String,
    /// `computed` (own sign-flip) | `declared` (sibling-evidenced raw
    /// p) | `unavailable` (worst-case mass — never dropped).
    pub status: String,
    /// The member's raw p (ppm; `None` = unavailable).
    pub raw_p_ppm: Option<i64>,
    /// The family-adjusted value (ppm).
    pub adjusted_ppm: i64,
    /// Rejected at the frozen `α = .05` (Holm rows only; BH rows
    /// report the q-value and never a rejection flag).
    pub rejected_at_alpha: bool,
}

/// The estimator's result — every member a fact on the report.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskMixture {
    /// `"test_minus_control"`.
    pub direction: String,
    /// The test arm.
    pub test_arm: String,
    /// The control arm.
    pub control_arm: String,
    /// The named population the cells read.
    pub population: String,
    /// The declared stratum selection.
    pub stratum: Option<String>,
    /// The strata the selected cells carry.
    pub stratum_membership: Vec<String>,
    /// `(task_id, delta_ppm, test_s, test_n, control_s, control_n)`.
    pub cells: Vec<(String, i64, u64, u64, u64, u64)>,
    /// The observed equal-task contrast `Σ w_t d_t`, `w_t = 1/T`.
    pub observed_contrast_ppm: i64,
    /// The floor-selected bootstrap method (`bca` | `percentile`).
    pub bootstrap_method: String,
    /// The declared bootstrap draws (requested == used).
    pub draws: u64,
    /// The labelled `confidence` interval.
    pub interval: Interval,
    /// The sign-flip leg.
    pub sign_flip: SignFlip,
    /// The frozen primary family (Holm α=.05), when declared.
    pub primary: Option<(String, Vec<FamilyRow>)>,
    /// The exploratory family (BH), when declared.
    pub exploratory: Option<(String, Vec<FamilyRow>)>,
    /// Signed-delta tail `(n, p50, p95, p99, max)` ppm.
    pub tail_signed: (usize, i64, i64, i64, i64),
    /// Absolute-delta tail `(n, p50, p95, p99, max)` ppm.
    pub tail_absolute: (usize, i64, i64, i64, i64),
    /// Sign counts `(pos, zero, neg)`.
    pub signs: (u64, u64, u64),
    /// Clustered-resampling interval for the signed p95 tail
    /// (`None` = undefined — no valid clustering).
    pub tail_uncertainty: Option<Interval>,
    /// The declared seed.
    pub seed: u64,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

/// A family member set's JSON rows under `method` at `alpha`.
fn family_json(fam: &(String, Vec<FamilyRow>), method: &str) -> Json {
    Json::obj([
        ("family_id", Json::str(&fam.0)),
        ("method", Json::str(method)),
        ("alpha_ppm", Json::Int(ALPHA_PPM)),
        ("family_size", Json::Int(fam.1.len() as i64)),
        (
            "members",
            Json::Arr(
                fam.1
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("member", Json::str(&r.member)),
                            ("status", Json::str(&r.status)),
                            ("raw_ppm", r.raw_p_ppm.map(Json::Int).unwrap_or(Json::Null)),
                            ("adjusted_ppm", Json::Int(r.adjusted_ppm)),
                            ("rejected_at_alpha", Json::Bool(r.rejected_at_alpha)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

impl TaskMixture {
    /// The canonical JSON member — the report body payload.
    pub fn to_json(&self) -> Json {
        let tail = |t: &(usize, i64, i64, i64, i64)| {
            Json::obj([
                ("n", Json::Int(t.0 as i64)),
                ("p50_ppm", Json::Int(t.1)),
                ("p95_ppm", Json::Int(t.2)),
                ("p99_ppm", Json::Int(t.3)),
                ("max_ppm", Json::Int(t.4)),
            ])
        };
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("direction", Json::str(&self.direction)),
            ("test_arm", Json::str(&self.test_arm)),
            ("control_arm", Json::str(&self.control_arm)),
            ("population", Json::str(&self.population)),
            (
                "stratum",
                self.stratum.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
            (
                "stratum_membership",
                Json::Arr(self.stratum_membership.iter().map(Json::str).collect()),
            ),
            (
                "cells",
                Json::Arr(
                    self.cells
                        .iter()
                        .map(|(t, d, ts, tn, cs, cn)| {
                            Json::obj([
                                ("task_id", Json::str(t)),
                                ("delta_ppm", Json::Int(*d)),
                                ("test_s", Json::Int(*ts as i64)),
                                ("test_n", Json::Int(*tn as i64)),
                                ("control_s", Json::Int(*cs as i64)),
                                ("control_n", Json::Int(*cn as i64)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("weights_ref", Json::str("equal_task:w_t=1/T")),
            (
                "observed_contrast_ppm",
                Json::Int(self.observed_contrast_ppm),
            ),
            (
                "bootstrap",
                Json::obj([
                    ("interval_kind", Json::str("confidence")),
                    ("method", Json::str(&self.bootstrap_method)),
                    ("level_ppm", Json::Int(LEVEL_PPM)),
                    ("cluster", Json::str("task")),
                    ("target", Json::str("task_mixture_sensitivity")),
                    (
                        "interval",
                        Json::obj([
                            ("lo_ppm", Json::Int(self.interval.lo)),
                            ("hi_ppm", Json::Int(self.interval.hi)),
                        ]),
                    ),
                    (
                        "draws",
                        Json::obj([
                            ("requested", Json::Int(self.draws as i64)),
                            ("used", Json::Int(self.draws as i64)),
                        ]),
                    ),
                    ("seed", Json::Int(self.seed as i64)),
                    ("tasks", Json::Int(self.cells.len() as i64)),
                ]),
            ),
            (
                "sign_flip",
                match &self.sign_flip {
                    SignFlip::NotDeclared => Json::obj([("status", Json::str("not_declared"))]),
                    SignFlip::Computed {
                        mode,
                        exchangeability,
                        draws,
                        p_ppm,
                    } => Json::obj([
                        ("status", Json::str("computed")),
                        ("null", Json::str(SHARP_NULL)),
                        ("exchangeability", Json::str(exchangeability)),
                        ("mode", Json::str(mode)),
                        ("draws", Json::Int(*draws as i64)),
                        ("p_ppm", Json::Int(*p_ppm)),
                        (
                            "rule",
                            Json::str(if *mode == "exact" {
                                "exact_enumeration"
                            } else {
                                "(extreme+1)/(draws+1)"
                            }),
                        ),
                    ]),
                },
            ),
            (
                "multiplicity",
                Json::obj([
                    (
                        "primary",
                        self.primary
                            .as_ref()
                            .map(|f| family_json(f, "holm"))
                            .unwrap_or(Json::obj([("status", Json::str("not_declared"))])),
                    ),
                    (
                        "exploratory",
                        self.exploratory
                            .as_ref()
                            .map(|f| family_json(f, "benjamini_hochberg"))
                            .unwrap_or(Json::obj([("status", Json::str("not_declared"))])),
                    ),
                ]),
            ),
            (
                "tails",
                Json::obj([
                    ("signed", tail(&self.tail_signed)),
                    ("absolute", tail(&self.tail_absolute)),
                    (
                        "sign_counts",
                        Json::obj([
                            ("pos", Json::Int(self.signs.0 as i64)),
                            ("zero", Json::Int(self.signs.1 as i64)),
                            ("neg", Json::Int(self.signs.2 as i64)),
                        ]),
                    ),
                    (
                        "uncertainty",
                        self.tail_uncertainty
                            .map(|i| {
                                Json::obj([
                                    ("kind", Json::str("clustered_resampling")),
                                    ("stat", Json::str("p95_ppm")),
                                    (
                                        "interval",
                                        Json::obj([
                                            ("lo_ppm", Json::Int(i.lo)),
                                            ("hi_ppm", Json::Int(i.hi)),
                                        ]),
                                    ),
                                ])
                            })
                            .unwrap_or(Json::obj([("kind", Json::str("undefined"))])),
                    ),
                    (
                        "max_note",
                        Json::str("observed sample max — not a population bound"),
                    ),
                ]),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

/// Round `a / b` to the nearest integer, ties away from zero (the
/// `stats::mean` rounding rule — one rounding rule, deterministic).
fn round_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    ((if a >= 0 { a + b / 2 } else { a - b / 2 }) / b) as i64
}

/// `task_mixture(pop, decl)` — the admitted estimator over the
/// reconciled planned population. `pop` is the R5.15
/// `PlannedPopulation` (consumed unchanged); every admitted-input
/// violation is a typed [`TaskMixtureError`].
pub fn task_mixture(
    pop: &PlannedPopulation,
    decl: &TaskMixtureDecl,
) -> Result<TaskMixture, TaskMixtureError> {
    // ── declared-input validation (all before any draw) ──────────────
    if !(0..PPM).contains(&decl.level_ppm) {
        return Err(TaskMixtureError::NonFiniteInput {
            detail: format!("level_ppm {} outside (0, {PPM})", decl.level_ppm),
        });
    }
    if decl.level_ppm != LEVEL_PPM {
        return Err(TaskMixtureError::UnsupportedLevel {
            declared: decl.level_ppm,
        });
    }
    match &decl.bounds {
        BoundsDecl::Unit => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } if *lo_ppm == 0 && *hi_ppm == PPM => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } => {
            return Err(PopulationError::UnsupportedBounds {
                detail: format!("bound [{lo_ppm},{hi_ppm}] is not the unit interval"),
            }
            .into())
        }
        BoundsDecl::Unbounded { detail } => {
            return Err(PopulationError::UnboundedMetric {
                detail: detail.clone(),
            }
            .into())
        }
        BoundsDecl::RightCensored { detail } => {
            return Err(PopulationError::CensoredMetric {
                detail: detail.clone(),
            }
            .into())
        }
    }
    if let Some(d) = &decl.dependence {
        if d != "independent" {
            return Err(TaskMixtureError::UnsupportedDependence {
                declared: d.clone(),
            });
        }
    }
    if let Some(v) = &decl.verdict {
        if matches!(
            v.as_str(),
            "omnibus"
                | "combined"
                | "combined_verdict"
                | "practical_margin"
                | "weak_mean"
                | "tost"
                | "equivalence"
                | "noninferiority"
                | "non_inferiority"
        ) {
            return Err(TaskMixtureError::UnsupportedVerdict {
                declared: v.clone(),
            });
        }
    }
    if let Some(n) = &decl.null {
        if n != SHARP_NULL {
            return Err(TaskMixtureError::UnsupportedNull {
                declared: n.clone(),
            });
        }
        match decl.exchangeability.as_deref() {
            Some(REGISTERED_EXCHANGEABILITY) => {}
            Some(SLOT_SCHEDULE_EXCHANGEABILITY) => {
                // REQ-XD-021 route: the recorded randomization record
                // must be carried and recompute; a confirmatory/
                // first-pass use needs GATE-G23 PASSED (REQ-XD-023).
                let ev = decl.schedule_randomization.as_ref().ok_or_else(|| {
                    TaskMixtureError::ExchangeabilityNotAdmitted {
                        declared: format!(
                            "{SLOT_SCHEDULE_EXCHANGEABILITY} (no randomization record)"
                        ),
                    }
                })?;
                ev.check()
                    .map_err(|detail| TaskMixtureError::RandomizationRecord { detail })?;
                let use_class = decl
                    .use_class
                    .clone()
                    .unwrap_or_else(|| "confirmatory".into());
                let u = crate::comparative_estimators::UseClass::parse(&use_class).ok_or_else(
                    || TaskMixtureError::NonFiniteInput {
                        detail: format!("use_class {use_class}"),
                    },
                )?;
                crate::comparative_estimators::check_method_admission(
                    SLOT_SCHEDULE_EXCHANGEABILITY,
                    u,
                    decl.admission.as_ref(),
                )
                .map_err(|e| match e {
                    crate::comparative_estimators::EstimatorError::MethodNotAdmitted {
                        piece,
                        gate,
                    } => TaskMixtureError::MethodNotAdmitted { piece, gate },
                    other => TaskMixtureError::NonFiniteInput {
                        detail: format!("{other}"),
                    },
                })?;
            }
            other => {
                return Err(TaskMixtureError::ExchangeabilityNotAdmitted {
                    declared: other
                        .map(str::to_string)
                        .unwrap_or_else(|| "(absent)".into()),
                })
            }
        }
    }
    if !matches!(decl.bootstrap.as_str(), "bca" | "percentile") {
        return Err(TaskMixtureError::UnknownMethod {
            declared: decl.bootstrap.clone(),
        });
    }
    if decl.draws < CONFIRMATORY_DRAWS_FLOOR {
        return Err(TaskMixtureError::DrawsBelowFloor {
            declared: decl.draws,
        });
    }
    if decl.draws > u32::MAX as u64 {
        return Err(TaskMixtureError::NonFiniteInput {
            detail: format!("draws {} exceeds the u32 resample bound", decl.draws),
        });
    }
    if let Some(f) = &decl.family {
        if f.id.is_empty() || f.members.is_empty() {
            return Err(TaskMixtureError::FamilyIdentity {
                detail: "family id or member set missing".into(),
            });
        }
        if !f.members.iter().any(|m| m.member == f.member) {
            return Err(TaskMixtureError::FamilyIdentity {
                detail: format!("report member {} not in family {}", f.member, f.id),
            });
        }
    }
    if let Some(e) = &decl.exploratory {
        if e.id.is_empty() || e.members.is_empty() {
            return Err(TaskMixtureError::FamilyIdentity {
                detail: "exploratory family id or member set missing".into(),
            });
        }
    }
    if decl.test_arm.is_empty() || decl.control_arm.is_empty() {
        return Err(TaskMixtureError::Population(PopulationError::MissingKey {
            plan_ref: "(task_mixture)".into(),
            member: "arm_id",
        }));
    }
    if decl.test_arm == decl.control_arm {
        return Err(TaskMixtureError::NonFiniteInput {
            detail: format!("test_arm == control_arm ({})", decl.test_arm),
        });
    }

    // ── per-task cells — the population contract's one counting rule ─
    let strata: BTreeSet<String> = pop
        .slots
        .iter()
        .filter(|s| s.arm_id == decl.test_arm || s.arm_id == decl.control_arm)
        .map(|s| s.stratum.clone())
        .collect();
    if decl.stratum.is_none() && strata.len() > 1 {
        return Err(TaskMixtureError::StrataSelectionRequired {
            strata: strata.into_iter().collect(),
        });
    }
    let slots: Vec<_> = pop
        .slots
        .iter()
        .filter(|s| s.arm_id == decl.test_arm || s.arm_id == decl.control_arm)
        .filter(|s| decl.stratum.as_ref().map_or(true, |st| &s.stratum == st))
        .cloned()
        .collect();
    let stratum_membership: Vec<String> = slots
        .iter()
        .map(|s| s.stratum.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let scoped = PlannedPopulation {
        slots,
        ..pop.clone()
    };
    let bounds = contrast_bounds(
        &scoped,
        decl.population,
        &decl.test_arm,
        &decl.control_arm,
        &BoundsDecl::Unit,
    )?;
    if bounds.per_task.len() < 2 {
        return Err(TaskMixtureError::OneTask);
    }
    let mut cells = Vec::with_capacity(bounds.per_task.len());
    let mut deltas = Vec::with_capacity(bounds.per_task.len());
    for tb in &bounds.per_task {
        let (te, co) = (&tb.test, &tb.control);
        for (cell, arm) in [(te, &decl.test_arm), (co, &decl.control_arm)] {
            if cell.u > 0 {
                return Err(TaskMixtureError::UnresolvedCell {
                    task_id: tb.task_id.clone(),
                    arm_id: arm.clone(),
                    u: cell.u,
                });
            }
            if cell.m < CELL_REPLICATE_FLOOR {
                return Err(TaskMixtureError::ShortCell {
                    task_id: tb.task_id.clone(),
                    arm_id: arm.clone(),
                    n: cell.m,
                });
            }
        }
        // `d_t = s_B/n_B − s_A/n_A` — one-shot rational→ppm round.
        let d = round_div(
            (i128::from(te.s) * i128::from(co.m) - i128::from(co.s) * i128::from(te.m))
                * i128::from(PPM),
            i128::from(te.m) * i128::from(co.m),
        );
        deltas.push(d);
        cells.push((tb.task_id.clone(), d, te.s, te.m, co.s, co.m));
    }
    if deltas.iter().collect::<BTreeSet<_>>().len() < 2 {
        return Err(TaskMixtureError::DegenerateBootstrap);
    }
    let observed_contrast_ppm = mean(&deltas).unwrap_or(0);

    // ── whole-task paired bootstrap (declared draws/seed) ────────────
    let tasks_n = deltas.len();
    let selected = if tasks_n >= BCA_TASK_FLOOR {
        "bca"
    } else {
        "percentile"
    };
    if decl.bootstrap != selected {
        return Err(TaskMixtureError::BootstrapFloorViolation {
            declared: decl.bootstrap.clone(),
            tasks: tasks_n,
            selected: selected.to_string(),
        });
    }
    let seed_material = Json::obj([
        ("estimator", Json::str(KIND)),
        ("version", Json::str(VERSION)),
        ("population", Json::str(decl.population.as_str())),
        ("test_arm", Json::str(&decl.test_arm)),
        ("control_arm", Json::str(&decl.control_arm)),
        (
            "stratum",
            decl.stratum.as_ref().map(Json::str).unwrap_or(Json::Null),
        ),
        ("seed", Json::Int(decl.seed as i64)),
        ("draws", Json::Int(decl.draws as i64)),
        (
            "cells",
            Json::Arr(
                cells
                    .iter()
                    .map(|(t, d, ts, tn, cs, cn)| {
                        Json::Arr(vec![
                            Json::str(t),
                            Json::Int(*d),
                            Json::Int(*ts as i64),
                            Json::Int(*tn as i64),
                            Json::Int(*cs as i64),
                            Json::Int(*cn as i64),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
    .to_canonical_string();
    let interval = bootstrap_paired_with(
        &deltas,
        LEVEL_PPM,
        selected == "bca",
        &format!("{seed_material}|bootstrap"),
        decl.draws as u32,
    )
    .ok_or(TaskMixtureError::DegenerateBootstrap)?;

    // ── sharp-null sign flip (admitted exchangeability only) ─────────
    // `null`/`exchangeability` validated above — here the spelling is
    // SHARP_NULL and the argument is REGISTERED, or `null` is absent.
    let sign_flip = if decl.null.is_none() {
        SignFlip::NotDeclared
    } else {
        let p = permutation_signflip_p(
            &deltas,
            SIGNFLIP_DRAWS,
            &format!("{seed_material}|signflip"),
        )
        .ok_or(TaskMixtureError::DegenerateBootstrap)?;
        let exch = decl
            .exchangeability
            .clone()
            .unwrap_or_else(|| REGISTERED_EXCHANGEABILITY.to_string());
        if tasks_n <= SIGNFLIP_EXACT_MAX_N {
            SignFlip::Computed {
                mode: "exact".to_string(),
                exchangeability: exch,
                draws: 0,
                p_ppm: p,
            }
        } else {
            SignFlip::Computed {
                mode: "monte_carlo".to_string(),
                exchangeability: exch,
                draws: SIGNFLIP_DRAWS,
                p_ppm: p,
            }
        }
    };
    let own_p = match &sign_flip {
        SignFlip::Computed { p_ppm, .. } => Some(*p_ppm),
        SignFlip::NotDeclared => None,
    };

    // ── frozen families: Holm primary / BH exploratory ───────────────
    let primary = decl.family.as_ref().map(|f| {
        let raw: Vec<Option<i64>> = f
            .members
            .iter()
            .map(|m| {
                if m.member == f.member {
                    own_p.or(m.raw_p_ppm)
                } else {
                    m.raw_p_ppm
                }
            })
            .collect();
        let adj = holm(&raw.iter().map(|r| r.unwrap_or(PPM)).collect::<Vec<_>>());
        let rows = f
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| FamilyRow {
                member: m.member.clone(),
                status: if m.member == f.member && own_p.is_some() {
                    "computed"
                } else if m.raw_p_ppm.is_some() {
                    "declared"
                } else {
                    "unavailable"
                }
                .to_string(),
                raw_p_ppm: raw[i],
                adjusted_ppm: adj[i],
                rejected_at_alpha: adj[i] <= ALPHA_PPM,
            })
            .collect();
        (f.id.clone(), rows)
    });
    let exploratory = decl.exploratory.as_ref().map(|e| {
        let adj = benjamini_hochberg(
            &e.members
                .iter()
                .map(|m| m.raw_p_ppm.unwrap_or(PPM))
                .collect::<Vec<_>>(),
        );
        let rows = e
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| FamilyRow {
                member: m.member.clone(),
                status: if m.raw_p_ppm.is_some() {
                    "declared"
                } else {
                    "unavailable"
                }
                .to_string(),
                raw_p_ppm: m.raw_p_ppm,
                adjusted_ppm: adj[i],
                rejected_at_alpha: false,
            })
            .collect();
        (e.id.clone(), rows)
    });

    // ── tails: signed/absolute deltas + clustered p95 uncertainty ────
    let tail = |xs: &[i64]| -> (usize, i64, i64, i64, i64) {
        (
            xs.len(),
            quantile(xs, 500_000).unwrap_or(0),
            quantile(xs, 950_000).unwrap_or(0),
            quantile(xs, 990_000).unwrap_or(0),
            xs.iter().copied().max().unwrap_or(0),
        )
    };
    let abs: Vec<i64> = deltas.iter().map(|d| d.abs()).collect();
    let tail_signed = tail(&deltas);
    let tail_absolute = tail(&abs);
    let (pos, zero, neg, _flip) = sign_profile(&deltas);
    // Clustered tail uncertainty: resample whole tasks; recompute the
    // signed p95 per resample under the declared seed/draws.
    let tail_uncertainty = {
        let mut rng = XorShift64::seeded(&format!("{seed_material}|tails"));
        let mut p95s = Vec::with_capacity(decl.draws as usize);
        for _ in 0..decl.draws {
            let resampled: Vec<i64> = (0..tasks_n).map(|_| deltas[rng.below(tasks_n)]).collect();
            p95s.push(quantile(&resampled, 950_000).unwrap_or(0));
        }
        let tail = (PPM - LEVEL_PPM) / 2;
        match (quantile(&p95s, tail), quantile(&p95s, PPM - tail)) {
            (Some(lo), Some(hi)) => Some(Interval { lo, hi }),
            _ => None,
        }
    };

    let mut keys = BTreeMap::new();
    keys.insert("population".into(), decl.population.as_str().to_string());
    keys.insert("test_arm".into(), decl.test_arm.clone());
    keys.insert("control_arm".into(), decl.control_arm.clone());
    if let Some(st) = &decl.stratum {
        keys.insert("stratum".into(), st.clone());
    }
    if let Some(f) = &decl.family {
        keys.insert("family".into(), f.id.clone());
        keys.insert("family_member".into(), f.member.clone());
    }
    Ok(TaskMixture {
        direction: "test_minus_control".to_string(),
        test_arm: decl.test_arm.clone(),
        control_arm: decl.control_arm.clone(),
        population: decl.population.as_str().to_string(),
        stratum: decl.stratum.clone(),
        stratum_membership: stratum_membership.clone(),
        cells,
        observed_contrast_ppm,
        bootstrap_method: selected.to_string(),
        draws: decl.draws,
        interval,
        sign_flip,
        primary,
        exploratory,
        tail_signed,
        tail_absolute,
        signs: (pos, zero, neg),
        tail_uncertainty,
        seed: decl.seed,
        metadata: MethodMetadata {
            estimator: KIND.to_string(),
            version: VERSION.to_string(),
            target: "task_mixture_sensitivity:test_minus_control".to_string(),
            assumptions: vec![
                "task is the cluster/pairing/resampling unit; equal within-stratum weights w_t=1/T; no flattened event/replicate n".into(),
                "task-mixture sensitivity — a descriptive confidence interval, never the finite-manifest credible target".into(),
                "sharp-null sign flip only under the registered lane_randomization exchangeability argument".into(),
                "frozen families: unavailable members retained at worst-case mass; exploratory BH separate from primary Holm".into(),
                "observed sample max is not a population bound".into(),
            ],
            prior: None,
            keys,
            stratum_membership,
            seed: decl.seed,
            draws_requested: decl.draws,
            draws_used: decl.draws,
            floors: vec![
                format!("cell_replicates>={CELL_REPLICATE_FLOOR}"),
                "tasks>=2".to_string(),
                format!("bootstrap_draws>={CONFIRMATORY_DRAWS_FLOOR}"),
                format!("bca_at_tasks>={BCA_TASK_FLOOR}"),
                format!("signflip_exact<=20;mc={SIGNFLIP_DRAWS}"),
            ],
            interval: ("confidence".to_string(), Some(LEVEL_PPM)),
            refusal: None,
        },
    })
}
