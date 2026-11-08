//! `comparative_estimators` — the §6.4 admission-map apparatus over the
//! admitted §4.3 machinery (R6.19, manifest 272; REQ-XD-001/022/030/031/061,
//! REQ-XF-123; contributes REQ-XA-006, REQ-XD-021/023/090).
//!
//! The pieces: the single-arm finite-manifest rate (Beta(1,1) cells,
//! 8,000 draws + doubling — the contrast machinery's per-arm marginal,
//! never a second implementation); the descriptive estimators rerun
//! flip, harness-swap flip, $/solve and the tied-with-leader route; the
//! per-tranche many-to-one confirmatory family (Holm α = .05) beside an
//! exploratory BH family; the §6.8 unresolved-cell refusal with
//! all-planned bounds and the completion envelope; and the
//! `hh-comparative-results/1` schema + validator (C-33) the site
//! consumes. The `slot_schedule_randomization` spelling lands on
//! `task_mixture`'s sign-flip path beside `lane_randomization`,
//! accepted only with R6.18's randomization record.
//!
//! **Every piece lands not admitted** (REQ-XD-023's route — AR-17's
//! §4.3 precedent, whose record R6.27 owns): a declared `confirmatory`
//! or `first_pass` use without a `GATE-G23` PASSED record refuses typed
//! `MethodNotAdmitted`. `descriptive`/`exploratory` uses compute and
//! carry their labels — descriptive metrics never carry a confirmatory
//! label (X-08). This row is the E-layer apparatus; the R-layer passes
//! of REQ-XD-001/030/061 are earned only when admitted first-pass
//! results recompute through it (R6.48).

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::comparative_protocol::{ScheduleEvidence, EXCHANGEABILITY};
use crate::finite_manifest::{
    arm_marginal_interval, DoublingStep, BASE_DRAWS, CAP_DRAWS, LEVEL_PPM, REGISTERED_PRIOR,
    REGISTERED_PRIOR_NAME,
};
use crate::population::{
    arm_bounds, contrib, BoundsDecl, MethodMetadata, PlannedPopulation, PopulationError,
    PopulationKind,
};
use crate::stats::{
    benjamini_hochberg, bootstrap_paired_with, holm, pass_at_k, pass_k, permutation_signflip_p,
    quantile, Interval, XorShift64, PPM, SIGNFLIP_EXACT_MAX_N,
};
use crate::task_mixture::SIGNFLIP_DRAWS;

/// The single-arm estimator kind.
pub const RATE_KIND: &str = "finite_manifest_rate";
/// The single-arm method contract version.
pub const RATE_VERSION: &str = "finite-manifest-rate/1";
/// The estimand composition version (θ_h and Δ(h − ref)).
pub const ESTIMAND_VERSION: &str = "comparative-estimand/1";
/// The descriptive flips version.
pub const FLIPS_VERSION: &str = "comparative-flips/1";
/// The per-tranche family version.
pub const FAMILY_VERSION: &str = "comparative-family/1";
/// The tied-with-leader route version.
pub const TIED_VERSION: &str = "tied-with-leader/1";
/// The $-route version.
pub const COST_VERSION: &str = "comparative-cost/1";
/// The results-packet schema (C-33; emitted by R6.48 — REQ-XD-090).
pub const RESULTS_SCHEMA: &str = "hh-comparative-results/1";
/// REQ-XD-023's method-admission gate (owned by R6.27).
pub const ADMISSION_GATE: &str = "GATE-G23";
/// The family's frozen primary metric (REQ-XD-030).
pub const PRIMARY_METRIC: &str = "capability.task_success";
/// The frozen Holm level (`α = .05`, ppm).
pub const ALPHA_PPM: i64 = 50_000;
/// The declared-draws floor shared with the §4.3 sensitivity route.
pub const DRAWS_FLOOR: u64 = 2_000;
/// The frozen interval level (95 %).
pub const LEVEL: i64 = LEVEL_PPM;

// ── mandatory label strings (§6.11, REQ-XD-092, REQ-XD-031, REQ-XH-119) ─

/// REQ-XD-092: internal record label.
pub const LABEL_INTERNAL: &str = "internal record — not outside-verified";
/// REQ-XD-092: hosted-participant label.
pub const LABEL_HOSTED: &str =
    "stochastic hosted participant glm-5.3-flash via Ollama — catalog identity reported, not fingerprinted";
/// REQ-XD-092: network label.
pub const LABEL_ALLOWLIST: &str = "allowlisted agent network";
/// REQ-XD-092: comparability label.
pub const LABEL_NOT_LEADERBOARD: &str =
    "fewer than 89 tasks and other departures — not leaderboard-comparable";
/// REQ-XD-092/REQ-XC-014: HF-context label.
pub const LABEL_HF_CONTEXT: &str = "HF board figures are context from a different serving setup";
/// §6.11: secondary-population label.
pub const LABEL_SECONDARY_POP: &str = "secondary population";
/// §6.11: descriptive label.
pub const LABEL_DESCRIPTIVE: &str = "descriptive";
/// §6.11/REQ-XD-031: tied-with-leader label (the selection caveat).
pub const LABEL_EXPLORATORY_LEADER: &str =
    "exploratory; leader selected from the same data — the 95% interval of difference covers 0 is not equivalence evidence";
/// §6.11: model-spend provenance.
pub const LABEL_MODEL_SPEND: &str = "model spend only";
/// §6.11: compute-inclusive provenance.
pub const LABEL_WITH_COMPUTE: &str = "with compute";
/// §6.11: Wilson clustering caveat.
pub const LABEL_CLUSTERING: &str = "ignores task clustering";
/// §6.8: eligible-only conditional secondary label.
pub const LABEL_CONDITIONAL_SECONDARY: &str =
    "conditional secondary — eligible-only analysis, not the pre-registered target";
/// §6.9: integrity minimum.
pub const LABEL_CONFIRMED_MIN: &str = "the confirmed count is a minimum";
/// REQ-XH-119: first-pass power label.
pub const LABEL_FIRST_PASS: &str =
    "pipeline demonstration; confirmatory test pre-stated as near-zero power";

/// The required top-level `labels{}` keys with pinned substrings
/// (REQ-XD-092/REQ-XF-123 — the validator refuses a missing or
/// weakened label).
pub const REQUIRED_LABELS: &[(&str, &[&str])] = &[
    ("internal_record", &["not outside-verified"]),
    (
        "finite_manifest_scope",
        &["finite-manifest", "not the 89-task benchmark"],
    ),
    (
        "hosted_participant",
        &["catalog identity reported, not fingerprinted"],
    ),
    ("allowlisted_network", &["allowlisted"]),
    (
        "not_leaderboard_comparable",
        &["not leaderboard-comparable"],
    ),
    ("hf_context", &["different serving setup"]),
];
/// The three population keys the packet carries (§6.11; `errors_zero`
/// names the all-planned errors-=0 bound, REQ-XD-005).
pub const REQUIRED_POPULATIONS: &[&str] = &[
    "joint_task_pass_without_observed_veto",
    "raw",
    "errors_zero",
];
/// The typed absent-value states (REQ-XF-122's class — a value is
/// never `null` or `0` in their place).
pub const ABSENT_STATES: &[&str] = &[
    "not_computed",
    "not_yet_available",
    "refused",
    "undefined",
    "estimator_undefined",
    "not_applicable",
];

// ── admission (REQ-XD-023's route) ────────────────────────────────────

/// The declared downstream use of a new §6.4 piece (records-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseClass {
    /// Confirmatory — gated.
    Confirmatory,
    /// First-pass — gated.
    FirstPass,
    /// Descriptive — computes, carries the descriptive label.
    Descriptive,
    /// Exploratory — computes, carries the exploratory label.
    Exploratory,
}

impl UseClass {
    /// Parse a declared spelling (`None` = not an admitted class).
    pub fn parse(s: &str) -> Option<UseClass> {
        match s {
            "confirmatory" => Some(UseClass::Confirmatory),
            "first_pass" => Some(UseClass::FirstPass),
            "descriptive" => Some(UseClass::Descriptive),
            "exploratory" => Some(UseClass::Exploratory),
            _ => None,
        }
    }
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UseClass::Confirmatory => "confirmatory",
            UseClass::FirstPass => "first_pass",
            UseClass::Descriptive => "descriptive",
            UseClass::Exploratory => "exploratory",
        }
    }
    /// `true` when the use is gated by [`ADMISSION_GATE`] (REQ-XD-023).
    pub fn gated(self) -> bool {
        matches!(self, UseClass::Confirmatory | UseClass::FirstPass)
    }
}

/// A recorded method-admission gate entry (REQ-XD-023's hook — "review
/// record and gate entry precede first use").
#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionRecord {
    /// The gate id — must be [`ADMISSION_GATE`].
    pub gate: String,
    /// The recorded status — must be `"passed"`.
    pub status: String,
    /// The committed gate-record ref — non-empty.
    pub record_ref: String,
}

/// The REQ-XD-023 check — `Ok(())` for ungated uses or a recorded
/// GATE-G23 PASSED entry; `MethodNotAdmitted` otherwise (typed, never
/// a silent compute).
pub fn check_method_admission(
    piece: &str,
    use_as: UseClass,
    admission: Option<&AdmissionRecord>,
) -> Result<(), EstimatorError> {
    if !use_as.gated() {
        return Ok(());
    }
    match admission {
        Some(a) if a.gate == ADMISSION_GATE && a.status == "passed" && !a.record_ref.is_empty() => {
            Ok(())
        }
        _ => Err(EstimatorError::MethodNotAdmitted {
            piece: piece.to_string(),
            gate: ADMISSION_GATE.to_string(),
        }),
    }
}

/// Parse a declared `use_as` + check the admission gate — the shared
/// prologue of every new piece (absent `use_as` reads `confirmatory`,
/// the strict default).
fn check_use(
    piece: &str,
    use_as: &str,
    admission: Option<&AdmissionRecord>,
) -> Result<UseClass, EstimatorError> {
    let u = UseClass::parse(use_as).ok_or_else(|| EstimatorError::UnsupportedUse {
        declared: use_as.to_string(),
    })?;
    check_method_admission(piece, u, admission)?;
    Ok(u)
}

/// A declared downstream claim that would present a descriptive route
/// as evidence it never carries (REQ-XD-031's claims check at the
/// estimator layer).
pub fn check_descriptive_claim(piece: &str, claim: Option<&str>) -> Result<(), EstimatorError> {
    if let Some(c) = claim {
        if matches!(
            c,
            "equivalence"
                | "tost"
                | "tost_equivalence"
                | "noninferiority"
                | "non_inferiority"
                | "no_difference"
                | "statistically_tied"
                | "equal"
                | "parity"
                | "confidence"
                | "concentration_bound"
        ) {
            return Err(EstimatorError::UnsupportedClaim {
                piece: piece.to_string(),
                declared: c.to_string(),
            });
        }
    }
    Ok(())
}

// ── the refusal set ──────────────────────────────────────────────────

/// The closed refusal set — typed facts, never warnings.
#[derive(Debug, Clone, PartialEq)]
pub enum EstimatorError {
    /// The population contract refused.
    Population(PopulationError),
    /// Fewer than two tasks.
    OneTask,
    /// A cell below the `R ≥ 5` floor.
    ShortCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Resolved trials in the cell.
        n: u64,
    },
    /// A cell carries unresolved members (§6.8 — REQ-XD-061).
    UnresolvedCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Unresolved members in the cell.
        u: u64,
    },
    /// Fewer than two distinct values — a point-mass resample.
    DegenerateBootstrap,
    /// Declared draws below the floor.
    DrawsBelowFloor {
        /// The declared draws.
        declared: u64,
    },
    /// A confirmatory/first-pass use without a GATE-G23 PASSED record
    /// (REQ-XD-023 — the not-admitted route).
    MethodNotAdmitted {
        /// The new piece.
        piece: String,
        /// The gate whose PASSED record is owed.
        gate: String,
    },
    /// A `use_as` spelling outside the admitted use classes.
    UnsupportedUse {
        /// The declared use.
        declared: String,
    },
    /// A downstream claim presenting a descriptive route as evidence
    /// it never carries (REQ-XD-031).
    UnsupportedClaim {
        /// The piece.
        piece: String,
        /// The declared claim.
        declared: String,
    },
    /// The estimator is undefined at the observed data ($/solve at
    /// 0 solves, a flip at k < 2 — a typed `estimator_undefined`,
    /// never a fabricated value).
    EstimatorUndefined {
        /// The detail.
        detail: String,
    },
    /// The exchangeability spelling is not admitted on this path, or
    /// `slot_schedule_randomization` was declared without its
    /// randomization record (REQ-XD-021).
    ExchangeabilityNotAdmitted {
        /// The declared spelling.
        declared: String,
    },
    /// The carried randomization record fails its recompute.
    RandomizationRecord {
        /// The mismatch detail.
        detail: String,
    },
    /// A required member is absent/empty.
    MissingKey {
        /// The member.
        member: &'static str,
    },
    /// A declared input outside its admitted set (level, prior,
    /// population, metric, bounds).
    NonFiniteInput {
        /// The detail.
        detail: String,
    },
    /// An arm outside the declared set.
    UnknownArm {
        /// The arm.
        arm_id: String,
    },
    /// A task pairs for one arm only.
    UnpairedTask {
        /// The task.
        task_id: String,
        /// The arm missing the pair.
        arm_id: String,
    },
}

impl std::fmt::Display for EstimatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EstimatorError as E;
        match self {
            E::Population(e) => write!(f, "{e}"),
            E::OneTask => write!(f, "one task — equal-task estimators require T >= 2"),
            E::ShortCell { task_id, arm_id, n } => write!(
                f,
                "insufficient_replicates: cell ({task_id}, {arm_id}) has n={n} < 5"
            ),
            E::UnresolvedCell { task_id, arm_id, u } => write!(
                f,
                "UnresolvedCell: cell ({task_id}, {arm_id}) carries {u} unresolved member(s)"
            ),
            E::DegenerateBootstrap => write!(f, "degenerate bootstrap: <2 distinct values"),
            E::DrawsBelowFloor { declared } => {
                write!(f, "draws {declared} below the {DRAWS_FLOOR} floor")
            }
            E::MethodNotAdmitted { piece, gate } => write!(
                f,
                "method not admitted: {piece} refuses confirmatory/first-pass use until a {gate} PASSED record exists (REQ-XD-023)"
            ),
            E::UnsupportedUse { declared } => {
                write!(f, "unsupported use_as: {declared}")
            }
            E::UnsupportedClaim { piece, declared } => write!(
                f,
                "unsupported claim on {piece}: {declared} — a descriptive route is never equivalence evidence"
            ),
            E::EstimatorUndefined { detail } => {
                write!(f, "estimator_undefined: {detail}")
            }
            E::ExchangeabilityNotAdmitted { declared } => write!(
                f,
                "exchangeability not admitted: {declared} — sharp-null requires lane_randomization or slot_schedule_randomization with its record"
            ),
            E::RandomizationRecord { detail } => {
                write!(f, "randomization record mismatch: {detail}")
            }
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::NonFiniteInput { detail } => write!(f, "nonfinite input: {detail}"),
            E::UnknownArm { arm_id } => write!(f, "arm {arm_id} is not declared"),
            E::UnpairedTask { task_id, arm_id } => {
                write!(f, "task {task_id}: no cell for arm {arm_id}")
            }
        }
    }
}

impl std::error::Error for EstimatorError {}

impl From<PopulationError> for EstimatorError {
    fn from(e: PopulationError) -> EstimatorError {
        EstimatorError::Population(e)
    }
}

/// Round `a / b` to the nearest integer, ties away from zero (the one
/// rounding rule — `stats::mean`'s).
fn round_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    ((if a >= 0 { a + b / 2 } else { a - b / 2 }) / b) as i64
}

// ── cell derivation (the population contract's one counting rule) ────

/// One arm's per-task cell counts `(task → (m, s, u))` under `contrib`.
type ArmCellMap = BTreeMap<String, (u64, u64, u64)>;

/// One arm's `ArmCellMap` — plus the strata the arm's slots span.
/// `stratum = Some` selects; `None` requires one.
fn arm_cells(
    pop: &PlannedPopulation,
    kind: PopulationKind,
    arm: &str,
    stratum: Option<&str>,
) -> Result<(ArmCellMap, Vec<String>), EstimatorError> {
    let strata: BTreeSet<String> = pop
        .slots
        .iter()
        .filter(|s| s.arm_id == arm)
        .map(|s| s.stratum.clone())
        .collect();
    if stratum.is_none() && strata.len() > 1 {
        return Err(EstimatorError::NonFiniteInput {
            detail: format!("arm {arm} spans strata {strata:?} — declare stratum"),
        });
    }
    let mut cells: ArmCellMap = BTreeMap::new();
    let mut membership: BTreeSet<String> = BTreeSet::new();
    for s in pop.slots.iter().filter(|s| s.arm_id == arm) {
        if let Some(st) = stratum {
            if s.stratum != st {
                continue;
            }
        }
        membership.insert(s.stratum.clone());
        let c = cells.entry(s.task_id.clone()).or_insert((0, 0, 0));
        c.0 += 1;
        let (ds, _df, du) = contrib(kind, s);
        c.1 += ds;
        c.2 += du;
    }
    if cells.is_empty() {
        return Err(EstimatorError::Population(PopulationError::EmptyUniverse));
    }
    Ok((cells, membership.into_iter().collect()))
}

/// The `(c, n)` cell list a fully-resolved arm carries — `UnresolvedCell`
/// on any `u > 0`, `ShortCell` below the R ≥ 5 floor.
fn resolved_cells(
    cells: &ArmCellMap,
    arm: &str,
) -> Result<Vec<(String, u64, u64)>, EstimatorError> {
    let mut out = Vec::with_capacity(cells.len());
    for (task, &(m, s, u)) in cells {
        if u > 0 {
            return Err(EstimatorError::UnresolvedCell {
                task_id: task.clone(),
                arm_id: arm.to_string(),
                u,
            });
        }
        if m < 5 {
            return Err(EstimatorError::ShortCell {
                task_id: task.clone(),
                arm_id: arm.to_string(),
                n: m,
            });
        }
        out.push((task.clone(), s, m));
    }
    Ok(out)
}

/// The completion envelope `{planned, first, recovered, unresolved}` —
/// §6.8's published member beside every bound (records-in counts; a
/// member resolved on its first subject vs its first admissible
/// completed retry; unresolved = no resolved member value).
fn completion_envelope(pop: &PlannedPopulation, arm: &str) -> (u64, u64, u64, u64) {
    let (mut planned, mut first, mut recovered, mut unresolved) = (0u64, 0u64, 0u64, 0u64);
    for s in pop.slots.iter().filter(|s| s.arm_id == arm) {
        planned += 1;
        match (s.member.is_some() && s.success.is_some(), s.recovered) {
            (true, false) => first += 1,
            (true, true) => recovered += 1,
            _ => unresolved += 1,
        }
    }
    (planned, first, recovered, unresolved)
}

#[allow(clippy::too_many_arguments)] // the arity is the S.1 metadata spine's.
fn meta(
    estimator: &str,
    version: &str,
    target: &str,
    assumptions: Vec<String>,
    keys: BTreeMap<String, String>,
    strata: Vec<String>,
    seed: u64,
    draws: u64,
    interval: (String, Option<i64>),
) -> MethodMetadata {
    MethodMetadata {
        estimator: estimator.to_string(),
        version: version.to_string(),
        target: target.to_string(),
        assumptions,
        prior: None,
        keys,
        stratum_membership: strata,
        seed,
        draws_requested: draws,
        draws_used: draws,
        floors: vec![
            "cell_replicates>=5".to_string(),
            "tasks>=2".to_string(),
            format!("draws>={DRAWS_FLOOR}"),
            format!("admission_gate={ADMISSION_GATE}:confirmatory,first_pass"),
        ],
        interval,
        refusal: None,
    }
}

// ── 1. single-arm finite-manifest rate (REQ-XD-022 route (b)) ────────

/// The declared inputs — records-in.
#[derive(Debug, Clone)]
pub struct RateDecl {
    /// The named population the cells read.
    pub population: PopulationKind,
    /// The arm.
    pub arm: String,
    /// The declared stratum (`None` requires the arm to span one).
    pub stratum: Option<String>,
    /// The declared prior — [`REGISTERED_PRIOR`] only.
    pub prior: (u64, u64),
    /// The declared prior name.
    pub prior_name: String,
    /// The declared bound class — the binary unit rate only.
    pub bounds: BoundsDecl,
    /// The declared level — [`LEVEL_PPM`] only.
    pub level_ppm: i64,
    /// The declared seed.
    pub seed: u64,
    /// The declared use (`confirmatory`/`first_pass` gated).
    pub use_as: String,
    /// The GATE-G23 record.
    pub admission: Option<AdmissionRecord>,
}

/// One resolved cell in the rate report.
#[derive(Debug, Clone, PartialEq)]
pub struct RateCell {
    /// The task.
    pub task_id: String,
    /// Successes.
    pub c: u64,
    /// Trials.
    pub n: u64,
}

/// The single-arm finite-manifest rate — θ_h's credible interval.
#[derive(Debug, Clone, PartialEq)]
pub struct FiniteManifestRate {
    /// The arm.
    pub arm: String,
    /// The declared population.
    pub population: String,
    /// The strata the cells carry.
    pub stratum_membership: Vec<String>,
    /// The resolved cells.
    pub cells: Vec<RateCell>,
    /// The observed equal-task rate `Σ w_t s/n`.
    pub observed_ppm: i64,
    /// The analytic posterior mean `Σ w_t (c+a)/(n+a+b)`.
    pub posterior_mean_ppm: i64,
    /// The all-planned bound `[s/M,(s+u)/M]`.
    pub bounds: Interval,
    /// The reported 95% equal-tail credible interval at [`BASE_DRAWS`].
    pub interval: Interval,
    /// Declared draws.
    pub draws_requested: u64,
    /// Consumed draws (the full ladder).
    pub draws_used: u64,
    /// The doubling record.
    pub doubling: Vec<DoublingStep>,
    /// The max endpoint delta over the ladder.
    pub numerical_error_ppm: i64,
    /// The canonical seed material (a record fact — the marginal
    /// recomputes byte-identically from it).
    pub seed_material: String,
    /// The declared use class.
    pub use_as: String,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

/// `finite_manifest_rate(pop, decl)` — θ_h's single-arm finite-manifest
/// credible interval: Beta(1,1) cells, 8,000 draws + doubling record
/// (REQ-XD-022). The estimator IS the contrast machinery's per-arm
/// marginal ([`arm_marginal_interval`]) over this arm's resolved cells.
pub fn finite_manifest_rate(
    pop: &PlannedPopulation,
    decl: &RateDecl,
) -> Result<FiniteManifestRate, EstimatorError> {
    let u = check_use(RATE_KIND, &decl.use_as, decl.admission.as_ref())?;
    if decl.level_ppm != LEVEL_PPM || !(0..PPM).contains(&decl.level_ppm) {
        return Err(EstimatorError::NonFiniteInput {
            detail: format!("level_ppm {} — frozen at {LEVEL_PPM}", decl.level_ppm),
        });
    }
    if !matches!(decl.bounds, BoundsDecl::Unit)
        && !matches!(&decl.bounds, BoundsDecl::Bounded { lo_ppm, hi_ppm } if *lo_ppm == 0 && *hi_ppm == PPM)
    {
        return Err(EstimatorError::NonFiniteInput {
            detail: "rate bounds admit the binary unit rate only".into(),
        });
    }
    if decl.prior != REGISTERED_PRIOR || decl.prior_name != REGISTERED_PRIOR_NAME {
        return Err(EstimatorError::NonFiniteInput {
            detail: format!(
                "prior {} — the single-arm rate is Beta(1,1) cells only",
                decl.prior_name
            ),
        });
    }
    if decl.arm.is_empty() {
        return Err(EstimatorError::MissingKey { member: "arm" });
    }
    let (cells_map, membership) =
        arm_cells(pop, decl.population, &decl.arm, decl.stratum.as_deref())?;
    if cells_map.len() < 2 {
        return Err(EstimatorError::OneTask);
    }
    let resolved = resolved_cells(&cells_map, &decl.arm)?;
    let cn: Vec<(u64, u64)> = resolved.iter().map(|(_, c, n)| (*c, *n)).collect();
    let seed_material = Json::obj([
        ("estimator", Json::str(RATE_KIND)),
        ("version", Json::str(RATE_VERSION)),
        ("population", Json::str(decl.population.as_str())),
        ("arm", Json::str(&decl.arm)),
        (
            "stratum",
            decl.stratum.as_ref().map(Json::str).unwrap_or(Json::Null),
        ),
        ("prior", Json::str(&decl.prior_name)),
        ("level_ppm", Json::Int(decl.level_ppm)),
        ("draws_base", Json::Int(BASE_DRAWS as i64)),
        ("seed", Json::Int(decl.seed as i64)),
        (
            "cells",
            Json::Arr(
                cn.iter()
                    .map(|&(c, n)| Json::Arr(vec![Json::Int(c as i64), Json::Int(n as i64)]))
                    .collect(),
            ),
        ),
    ])
    .to_canonical_string();
    // The per-arm marginal IS the contrast machinery's marginal over
    // these cells — same draws, same ladder, same quantile.
    let marg = arm_marginal_interval(&cn, decl.prior, &seed_material);
    let t_n = resolved.len() as i128;
    let observed = round_div(
        resolved
            .iter()
            .map(|(_, c, n)| round_div(i128::from(*c) * i128::from(PPM), i128::from(*n)) as i128)
            .sum::<i128>(),
        t_n,
    );
    let (pa, pab) = (
        i128::from(decl.prior.0),
        i128::from(decl.prior.0 + decl.prior.1),
    );
    let posterior_mean = round_div(
        resolved
            .iter()
            .map(|(_, c, n)| {
                round_div((*c as i128 + pa) * i128::from(PPM), *n as i128 + pab) as i128
            })
            .sum::<i128>(),
        t_n,
    );
    let b = arm_bounds(pop, decl.population, &decl.arm)?;
    let mut keys = BTreeMap::new();
    keys.insert("population".into(), decl.population.as_str().to_string());
    keys.insert("arm".into(), decl.arm.clone());
    if let Some(st) = &decl.stratum {
        keys.insert("stratum".into(), st.clone());
    }
    let mut metadata = meta(
        RATE_KIND,
        RATE_VERSION,
        "finite_manifest_equal_task_rate:single_arm",
        vec![
            "independent Bernoulli runs within cells; independent posteriors across task cells"
                .into(),
            "equal task weights w_t=1/T; the per-arm marginal of the pairwise FM machinery".into(),
            "finite-manifest target only — no population extrapolation".into(),
            "not admitted for confirmatory/first-pass use until GATE-G23 PASSED".into(),
        ],
        keys,
        membership.clone(),
        decl.seed,
        CAP_DRAWS,
        ("credible".to_string(), Some(LEVEL_PPM)),
    );
    metadata.prior = Some(decl.prior_name.clone());
    metadata.draws_requested = BASE_DRAWS;
    Ok(FiniteManifestRate {
        arm: decl.arm.clone(),
        population: decl.population.as_str().to_string(),
        stratum_membership: membership,
        cells: resolved
            .iter()
            .map(|(t, c, n)| RateCell {
                task_id: t.clone(),
                c: *c,
                n: *n,
            })
            .collect(),
        observed_ppm: observed,
        posterior_mean_ppm: posterior_mean,
        bounds: Interval {
            lo: b.lo_ppm,
            hi: b.hi_ppm,
        },
        interval: marg.interval,
        draws_requested: BASE_DRAWS,
        draws_used: CAP_DRAWS,
        doubling: marg.doubling,
        numerical_error_ppm: marg.numerical_error_ppm,
        seed_material,
        use_as: u.as_str().to_string(),
        metadata,
    })
}

// ── 2. descriptive flips + cost routes (REQ-XD-022) ──────────────────

/// One task's cell for a flip computation.
#[derive(Debug, Clone, PartialEq)]
pub struct FlipCell {
    /// The task.
    pub task_id: String,
    /// Successes.
    pub c: u64,
    /// Trials (the cell's replicate count k).
    pub n: u64,
}

/// A flip/dollar report — point, per-task values, a labelled
/// descriptive task bootstrap, and the unavailable members listed.
#[derive(Debug, Clone, PartialEq)]
pub struct FlipReport {
    /// The estimator spelling.
    pub estimator: String,
    /// The point estimate (ppm or µ$ by `unit`).
    pub point: i64,
    /// The value unit.
    pub unit: String,
    /// Per-task values `(task_id, ppm)`.
    pub per_task: Vec<(String, i64)>,
    /// Tasks whose estimator is undefined (listed, never dropped).
    pub undefined_tasks: Vec<String>,
    /// The descriptive task-bootstrap 95% interval (`None` = degenerate).
    pub interval: Option<Interval>,
    /// The declared draws.
    pub draws: u64,
    /// The declared use.
    pub use_as: String,
    /// The mandatory labels.
    pub labels: Vec<String>,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// The descriptive-draws validation shared by every new descriptive
/// route.
fn check_descriptive(
    piece: &str,
    use_as: &str,
    admission: Option<&AdmissionRecord>,
    draws: u64,
) -> Result<UseClass, EstimatorError> {
    let u = check_use(piece, use_as, admission)?;
    if draws < DRAWS_FLOOR {
        return Err(EstimatorError::DrawsBelowFloor { declared: draws });
    }
    Ok(u)
}

/// The descriptive task-bootstrap interval over per-task values.
fn descriptive_interval(
    per_task: &[(String, i64)],
    level: i64,
    seed_material: &str,
    draws: u64,
) -> Option<Interval> {
    let xs: Vec<i64> = per_task.iter().map(|(_, v)| *v).collect();
    if xs.len() < 2 || xs.iter().collect::<BTreeSet<_>>().len() < 2 {
        return None;
    }
    bootstrap_paired_with(
        &xs,
        level,
        false,
        &format!("{seed_material}|descriptive"),
        draws as u32,
    )
}

/// `rerun_flip(cells, decl)` — `f_h = mean_t 2c(k−c)/(k(k−1))` — the
/// unbiased rerun-flip estimate of `2p(1−p)` under a constant task rate
/// (REQ-XD-022, descriptive only).
pub fn rerun_flip(
    cells: &[FlipCell],
    use_as: &str,
    admission: Option<&AdmissionRecord>,
    draws: u64,
    seed: u64,
) -> Result<FlipReport, EstimatorError> {
    let u = check_descriptive("rerun_flip", use_as, admission, draws)?;
    if cells.is_empty() {
        return Err(EstimatorError::MissingKey { member: "cells" });
    }
    let mut per_task = Vec::new();
    let mut undefined = Vec::new();
    for c in cells {
        if c.n < 2 || c.c > c.n {
            undefined.push(c.task_id.clone());
            continue;
        }
        let (cc, k) = (i128::from(c.c), i128::from(c.n));
        per_task.push((
            c.task_id.clone(),
            round_div(2 * cc * (k - cc) * i128::from(PPM), k * (k - 1)),
        ));
    }
    if per_task.is_empty() {
        return Err(EstimatorError::EstimatorUndefined {
            detail: "every task cell has k < 2 — a flip needs two replicates".into(),
        });
    }
    let point = round_div(
        per_task.iter().map(|(_, v)| *v as i128).sum::<i128>(),
        per_task.len() as i128,
    );
    let seed_material = format!("comparative-flips/1|rerun|{seed}|{per_task:?}");
    let mut keys = BTreeMap::new();
    keys.insert("tasks".into(), per_task.len().to_string());
    Ok(FlipReport {
        estimator: "rerun_flip".into(),
        point,
        unit: "ppm".into(),
        interval: descriptive_interval(&per_task, LEVEL, &seed_material, draws),
        per_task,
        undefined_tasks: undefined,
        draws,
        use_as: u.as_str().into(),
        labels: vec![LABEL_DESCRIPTIVE.into()],
        metadata: meta(
            "rerun_flip",
            FLIPS_VERSION,
            "descriptive rerun flip mean_t 2c(k-c)/(k(k-1))",
            vec![
                "unbiased estimator of 2p(1-p) under a constant task rate".into(),
                "descriptive only — never a confirmatory label".into(),
            ],
            keys,
            vec![],
            seed,
            draws,
            ("descriptive".to_string(), Some(LEVEL)),
        ),
    })
}

/// `swap_flip(g, h, decl)` — `f_gh = mean_t [p̂_g(1−p̂_h)+p̂_h(1−p̂_g)]`,
/// `p̂ = c/k`, plus the `harness_excess` member `f_swap − ½(f_rerun,g +
/// f_rerun,h)` with its own interval (the D1 F7 identity — the site
/// reads it verbatim; REQ-XF-123).
#[derive(Debug, Clone, PartialEq)]
pub struct SwapReport {
    /// The two arms.
    pub arms: (String, String),
    /// The swap flip point (ppm).
    pub swap_ppm: i64,
    /// Each arm's rerun flip (ppm) over the paired task set.
    pub rerun_ppm: (i64, i64),
    /// `f_swap − ½(f_g + f_h)` (ppm) — the harness-excess member.
    pub excess_ppm: i64,
    /// The paired per-task values `(task, swap, excess)`.
    pub per_task: Vec<(String, i64, i64)>,
    /// Descriptive bootstrap intervals `(swap, excess)`.
    pub swap_interval: Option<Interval>,
    /// The excess interval.
    pub excess_interval: Option<Interval>,
    /// The declared draws.
    pub draws: u64,
    /// The declared use.
    pub use_as: String,
    /// The mandatory labels.
    pub labels: Vec<String>,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// `swap_flip` — the harness-swap flip and the `harness_excess`
/// identity (descriptive only; REQ-XD-022/REQ-XF-123).
#[allow(clippy::too_many_arguments)] // the arity is the paired records-in shape.
pub fn swap_flip(
    arm_g: &str,
    g: &[FlipCell],
    arm_h: &str,
    h: &[FlipCell],
    use_as: &str,
    admission: Option<&AdmissionRecord>,
    draws: u64,
    seed: u64,
) -> Result<SwapReport, EstimatorError> {
    let u = check_descriptive("swap_flip", use_as, admission, draws)?;
    if g.is_empty() || h.is_empty() {
        return Err(EstimatorError::MissingKey { member: "cells" });
    }
    let gm: BTreeMap<&str, &FlipCell> = g.iter().map(|c| (c.task_id.as_str(), c)).collect();
    let hm: BTreeMap<&str, &FlipCell> = h.iter().map(|c| (c.task_id.as_str(), c)).collect();
    if gm.len() != g.len() || hm.len() != h.len() {
        return Err(EstimatorError::NonFiniteInput {
            detail: "duplicate task id in a flip cell set".into(),
        });
    }
    for t in gm.keys() {
        if !hm.contains_key(t) {
            return Err(EstimatorError::UnpairedTask {
                task_id: (*t).to_string(),
                arm_id: arm_h.to_string(),
            });
        }
    }
    for t in hm.keys() {
        if !gm.contains_key(t) {
            return Err(EstimatorError::UnpairedTask {
                task_id: (*t).to_string(),
                arm_id: arm_g.to_string(),
            });
        }
    }
    let mut per_task = Vec::with_capacity(g.len());
    let (mut f_g, mut f_h) = (0i128, 0i128);
    for (t, cg) in &gm {
        let ch = hm[t];
        if cg.n < 2 || ch.n < 2 || cg.c > cg.n || ch.c > ch.n {
            return Err(EstimatorError::EstimatorUndefined {
                detail: format!("task {t}: a swap needs k >= 2 resolved trials per arm"),
            });
        }
        let (c1, n1, c2, n2) = (
            i128::from(cg.c),
            i128::from(cg.n),
            i128::from(ch.c),
            i128::from(ch.n),
        );
        // swap_t = p̂g(1−p̂h) + p̂h(1−p̂g) = (c_g·n_h + c_h·n_g − 2·c_g·c_h)/(n_g·n_h)
        let swap = round_div((c1 * n2 + c2 * n1 - 2 * c1 * c2) * i128::from(PPM), n1 * n2);
        // u_x,t = 2c(k−c)/(k(k−1)) — the unbiased rerun kernel.
        let u_g = round_div(2 * c1 * (n1 - c1) * i128::from(PPM), n1 * (n1 - 1));
        let u_h = round_div(2 * c2 * (n2 - c2) * i128::from(PPM), n2 * (n2 - 1));
        // excess_t = swap_t − ½(u_g + u_h) → mean_t (p̂g − p̂h)² in
        // expectation (the D1 F7 identity).
        let excess = swap - (u_g + u_h) / 2;
        f_g += i128::from(u_g);
        f_h += i128::from(u_h);
        per_task.push(((*t).to_string(), swap, excess));
    }
    let t_n = per_task.len() as i128;
    let swap_ppm = round_div(
        per_task.iter().map(|(_, s, _)| *s as i128).sum::<i128>(),
        t_n,
    );
    let excess_ppm = round_div(
        per_task.iter().map(|(_, _, e)| *e as i128).sum::<i128>(),
        t_n,
    );
    let seed_material = format!("comparative-flips/1|swap|{arm_g}|{arm_h}|{seed}|{per_task:?}");
    let swap_tasks: Vec<(String, i64)> = per_task.iter().map(|(t, s, _)| (t.clone(), *s)).collect();
    let excess_tasks: Vec<(String, i64)> =
        per_task.iter().map(|(t, _, e)| (t.clone(), *e)).collect();
    let mut keys = BTreeMap::new();
    keys.insert("arms".into(), format!("{arm_g}|{arm_h}"));
    keys.insert("tasks".into(), per_task.len().to_string());
    Ok(SwapReport {
        arms: (arm_g.into(), arm_h.into()),
        swap_ppm,
        rerun_ppm: (round_div(f_g, t_n), round_div(f_h, t_n)),
        excess_ppm,
        swap_interval: descriptive_interval(&swap_tasks, LEVEL, &format!("{seed_material}|swap"), draws),
        excess_interval: descriptive_interval(&excess_tasks, LEVEL, &format!("{seed_material}|excess"), draws),
        per_task,
        draws,
        use_as: u.as_str().into(),
        labels: vec![LABEL_DESCRIPTIVE.into()],
        metadata: meta(
            "harness_swap_flip",
            FLIPS_VERSION,
            "descriptive swap flip mean_t [p_g(1-p_h)+p_h(1-p_g)] and harness_excess",
            vec![
                "harness_excess = f_swap - 1/2(f_rerun,g + f_rerun,h) — the D1 F7 identity, mean_t (p_g - p_h)^2 in expectation".into(),
                "descriptive only — never a confirmatory label".into(),
            ],
            keys,
            vec![],
            seed,
            draws,
            ("descriptive".to_string(), Some(LEVEL)),
        ),
    })
}

/// One scored trial's cost record (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct TrialCost {
    /// The task (the cluster unit).
    pub task_id: String,
    /// Settled or labelled per-trial cost in integer µ$.
    pub cost_micros: i64,
    /// `true` = solved (scored a pass).
    pub solved: bool,
}

/// The $/trial + $/solve report.
#[derive(Debug, Clone, PartialEq)]
pub struct CostReport {
    /// Mean per-trial cost (µ$).
    pub per_trial_micros: i64,
    /// Tails `(p50, p90, p95, max)` — sample facts, never bounds.
    pub tails_micros: (i64, i64, i64, i64),
    /// The clustered task-bootstrap interval for the mean.
    pub per_trial_interval: Option<Interval>,
    /// `Σ scored cost / Σ solved` (µ$).
    pub per_solve_micros: Option<i64>,
    /// The task-resampled ratio interval for $/solve.
    pub per_solve_interval: Option<Interval>,
    /// Resamples whose solved count was zero (a fact, never hidden).
    pub undefined_solve_draws: u64,
    /// The declared draws.
    pub draws: u64,
    /// The declared use.
    pub use_as: String,
    /// The cost provenance label (`model spend only` | `with compute`).
    pub provenance: String,
    /// The mandatory labels.
    pub labels: Vec<String>,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// `cost_routes(trials, provenance, decl)` — $/trial mean + clustered
/// task bootstrap and tails, and $/solve = Σ scored-trial cost /
/// Σ solved scored trials with a task-resampled ratio interval —
/// `estimator_undefined` at 0 solves (REQ-XD-022).
pub fn cost_routes(
    trials: &[TrialCost],
    provenance: &str,
    use_as: &str,
    admission: Option<&AdmissionRecord>,
    draws: u64,
    seed: u64,
) -> Result<CostReport, EstimatorError> {
    let u = check_descriptive("cost_routes", use_as, admission, draws)?;
    if !matches!(provenance, "model_spend_only" | "with_compute") {
        return Err(EstimatorError::NonFiniteInput {
            detail: format!("cost provenance {provenance} — declared label required"),
        });
    }
    if trials.is_empty() {
        return Err(EstimatorError::MissingKey { member: "trials" });
    }
    if trials.iter().any(|t| t.cost_micros < 0) {
        return Err(EstimatorError::NonFiniteInput {
            detail: "negative trial cost".into(),
        });
    }
    let costs: Vec<i64> = trials.iter().map(|t| t.cost_micros).collect();
    let per_trial = round_div(
        costs.iter().map(|&c| c as i128).sum::<i128>(),
        costs.len() as i128,
    );
    let tails = (
        quantile(&costs, 500_000).unwrap_or(0),
        quantile(&costs, 900_000).unwrap_or(0),
        quantile(&costs, 950_000).unwrap_or(0),
        costs.iter().copied().max().unwrap_or(0),
    );
    // per-task totals — the task is the cluster unit.
    let mut by_task: BTreeMap<&str, (i128, u64, u64)> = BTreeMap::new();
    for t in trials {
        let e = by_task.entry(t.task_id.as_str()).or_insert((0, 0, 0));
        e.0 += i128::from(t.cost_micros);
        e.1 += u64::from(t.solved);
        e.2 += 1;
    }
    let tasks: Vec<&str> = by_task.keys().copied().collect();
    let total_solved: u64 = by_task.values().map(|(_, s, _)| *s).sum();
    let seed_material = format!("comparative-cost/1|{seed}|{trials:?}");
    // Clustered bootstrap for the mean $/trial: resample whole tasks,
    // all their trials ride along.
    let pt_interval = if tasks.len() < 2 {
        None
    } else {
        let mut rng = XorShift64::seeded(&format!("{seed_material}|per_trial"));
        let mut stats_v = Vec::with_capacity(draws as usize);
        for _ in 0..draws {
            let mut sum: i128 = 0;
            let mut cnt: i128 = 0;
            for _ in 0..tasks.len() {
                let (c, _, n) = by_task[tasks[rng.below(tasks.len())]];
                sum += c;
                cnt += i128::from(n);
            }
            stats_v.push(round_div(sum, cnt));
        }
        let tail = (PPM - LEVEL) / 2;
        match (quantile(&stats_v, tail), quantile(&stats_v, PPM - tail)) {
            (Some(lo), Some(hi)) => Some(Interval { lo, hi }),
            _ => None,
        }
    };
    // $/solve — task-resampled ratio; 0-solve resamples are recorded.
    let (per_solve, ps_interval, undefined_draws) = if total_solved == 0 {
        (None, None, 0)
    } else {
        let total_cost: i128 = trials.iter().map(|t| i128::from(t.cost_micros)).sum();
        let point = round_div(total_cost, i128::from(total_solved));
        let (mut interval, mut undef) = (None, 0u64);
        if tasks.len() >= 2 {
            let mut rng = XorShift64::seeded(&format!("{seed_material}|per_solve"));
            let mut stats_v = Vec::with_capacity(draws as usize);
            for _ in 0..draws {
                let (mut sc, mut ss) = (0i128, 0u64);
                for _ in 0..tasks.len() {
                    let (c, s, _) = by_task[tasks[rng.below(tasks.len())]];
                    sc += c;
                    ss += s;
                }
                if ss == 0 {
                    undef += 1;
                } else {
                    stats_v.push(round_div(sc, i128::from(ss)));
                }
            }
            let tail = (PPM - LEVEL) / 2;
            if let (Some(lo), Some(hi)) = (quantile(&stats_v, tail), quantile(&stats_v, PPM - tail))
            {
                interval = Some(Interval { lo, hi });
            }
        }
        (Some(point), interval, undef)
    };
    if total_solved == 0 {
        return Err(EstimatorError::EstimatorUndefined {
            detail: "0 solves — $/solve undefined".into(),
        });
    }
    let mut keys = BTreeMap::new();
    keys.insert("trials".into(), trials.len().to_string());
    keys.insert("provenance".into(), provenance.to_string());
    Ok(CostReport {
        per_trial_micros: per_trial,
        tails_micros: tails,
        per_trial_interval: pt_interval,
        per_solve_micros: per_solve,
        per_solve_interval: ps_interval,
        undefined_solve_draws: undefined_draws,
        draws,
        use_as: u.as_str().into(),
        provenance: provenance.into(),
        labels: vec![
            LABEL_DESCRIPTIVE.into(),
            if provenance == "model_spend_only" {
                LABEL_MODEL_SPEND.into()
            } else {
                LABEL_WITH_COMPUTE.into()
            },
        ],
        metadata: meta(
            "cost_routes",
            COST_VERSION,
            "per-trial cost and per-solve ratio",
            vec![
                "task is the cluster unit; per-solve ratio resamples tasks".into(),
                "tails are sample facts, never population bounds".into(),
                "descriptive only — never a confirmatory label".into(),
            ],
            keys,
            vec![],
            seed,
            draws,
            ("descriptive".to_string(), Some(LEVEL)),
        ),
    })
}

// ── 3. pass^k / pass@k (admitted composition, §6.11 row) ─────────────

/// `pass_rates(cells, k, decl)` — mean_t `pass^k` and `pass@k` with a
/// descriptive task bootstrap (§3.2's admitted estimators composed for
/// the §6.11 row; `k > n` tasks are listed undefined, never dropped).
pub fn pass_rates(
    cells: &[FlipCell],
    k: u64,
    use_as: &str,
    admission: Option<&AdmissionRecord>,
    draws: u64,
    seed: u64,
) -> Result<(FlipReport, FlipReport), EstimatorError> {
    let u = check_descriptive("pass_rates", use_as, admission, draws)?;
    if cells.is_empty() || k == 0 {
        return Err(EstimatorError::MissingKey { member: "cells/k" });
    }
    let (mut all, mut any, mut undef) = (Vec::new(), Vec::new(), Vec::new());
    for c in cells {
        match (pass_k(c.c, c.n, k), pass_at_k(c.c, c.n, k)) {
            (Some(a), Some(b)) => {
                all.push((c.task_id.clone(), a));
                any.push((c.task_id.clone(), b));
            }
            _ => undef.push(c.task_id.clone()),
        }
    }
    if all.is_empty() {
        return Err(EstimatorError::EstimatorUndefined {
            detail: format!("k={k} exceeds every cell's n — pass^{k}/pass@{k} undefined"),
        });
    }
    let mk = |name: &str, per_task: Vec<(String, i64)>, undef: Vec<String>| {
        let point = round_div(
            per_task.iter().map(|(_, v)| *v as i128).sum::<i128>(),
            per_task.len() as i128,
        );
        let sm = format!("comparative-flips/1|{name}|{k}|{seed}|{per_task:?}");
        let mut keys = BTreeMap::new();
        keys.insert("k".into(), k.to_string());
        keys.insert("tasks".into(), per_task.len().to_string());
        FlipReport {
            estimator: name.into(),
            point,
            unit: "ppm".into(),
            interval: descriptive_interval(&per_task, LEVEL, &sm, draws),
            per_task,
            undefined_tasks: undef,
            draws,
            use_as: u.as_str().into(),
            labels: vec![LABEL_DESCRIPTIVE.into()],
            metadata: meta(
                name,
                FLIPS_VERSION,
                "descriptive pass^k/pass@k over the manifest",
                vec!["admitted stats estimators; descriptive only".into()],
                keys.clone(),
                vec![],
                seed,
                draws,
                ("descriptive".to_string(), Some(LEVEL)),
            ),
        }
    };
    Ok((mk("pass^k", all, undef.clone()), mk("pass@k", any, undef)))
}

// ── 4. tied-with-leader (REQ-XD-031) ─────────────────────────────────

/// One arm's tied-with-leader row.
#[derive(Debug, Clone, PartialEq)]
pub struct TiedRow {
    /// The arm.
    pub arm_id: String,
    /// `Some(tied)` iff the task-bootstrap 95% interval of Δ(L − h)
    /// covers 0; `None` when < 2 mutually-resolved tasks.
    pub tied: Option<bool>,
    /// The paired-delta interval used.
    pub diff_interval: Option<Interval>,
    /// The mutually-resolved tasks the interval reads.
    pub tasks: Vec<String>,
}

/// The tied-with-leader report (descriptive/exploratory only).
#[derive(Debug, Clone, PartialEq)]
pub struct TiedReport {
    /// The selected leader (argmax θ̂, deterministic tie-break by arm id —
    /// a selection from the same data, recorded as such).
    pub leader_id: String,
    /// `true` when another arm shares the max θ̂ (a selection fact).
    pub leader_contested: bool,
    /// Per-arm θ̂ used for the selection (`None` = no resolved tasks).
    pub theta: Vec<(String, Option<i64>)>,
    /// Per-arm rows.
    pub rows: Vec<TiedRow>,
    /// The declared draws.
    pub draws: u64,
    /// The declared use.
    pub use_as: String,
    /// The mandatory labels.
    pub labels: Vec<String>,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// `tied_with_leader(pop, decl)` — the descriptive tied route: leader
/// = argmax θ̂; tied iff the task-bootstrap 95% interval of Δ(L − h)
/// covers 0 (REQ-XD-031 — descriptive, exploratory, with the selection
/// caveat; never equivalence evidence).
#[allow(clippy::too_many_arguments)] // the arity is the records-in declaration.
pub fn tied_with_leader(
    pop: &PlannedPopulation,
    kind: PopulationKind,
    arms: &[String],
    stratum: Option<&str>,
    use_as: &str,
    claim: Option<&str>,
    admission: Option<&AdmissionRecord>,
    draws: u64,
    seed: u64,
) -> Result<TiedReport, EstimatorError> {
    let u = check_descriptive("tied_with_leader", use_as, admission, draws)?;
    check_descriptive_claim("tied_with_leader", claim)?;
    if arms.len() < 2 {
        return Err(EstimatorError::NonFiniteInput {
            detail: "tied-with-leader needs >= 2 arms".into(),
        });
    }
    let mut cells: BTreeMap<String, BTreeMap<String, (u64, u64, u64)>> = BTreeMap::new();
    let mut theta = Vec::new();
    for arm in arms {
        let (m, _) = arm_cells(pop, kind, arm, stratum)?;
        let (mut sum, mut cnt) = (0i128, 0i128);
        for &(mm, s, u_) in m.values() {
            if u_ == 0 && mm > 0 {
                sum += round_div(i128::from(s) * i128::from(PPM), i128::from(mm)) as i128;
                cnt += 1;
            }
        }
        theta.push((
            arm.clone(),
            if cnt > 0 {
                Some(round_div(sum, cnt))
            } else {
                None
            },
        ));
        cells.insert(arm.clone(), m);
    }
    // Leader: argmax θ̂; deterministic tie-break by arm id; contested
    // when the max is shared (a selection fact, not a silence).
    let mut best: Option<(&String, i64)> = None;
    for (arm, t) in &theta {
        if let Some(v) = t {
            best = match best {
                None => Some((arm, *v)),
                Some((_, bv)) if *v > bv => Some((arm, *v)),
                other => other,
            };
        }
    }
    let leader = best
        .ok_or_else(|| EstimatorError::EstimatorUndefined {
            detail: "no arm has a resolved cell — no leader to tie with".into(),
        })?
        .0
        .clone();
    let leader_theta = best.map(|(_, v)| v).unwrap_or(0);
    let contested = theta
        .iter()
        .filter(|(a, t)| a != &leader && *t == Some(leader_theta))
        .count()
        > 0;
    let leader_cells = &cells[&leader];
    let mut rows = Vec::new();
    for arm in arms {
        if arm == &leader {
            rows.push(TiedRow {
                arm_id: arm.clone(),
                tied: Some(true),
                diff_interval: Some(Interval { lo: 0, hi: 0 }),
                tasks: vec![],
            });
            continue;
        }
        let hc = &cells[arm];
        let mut deltas = Vec::new();
        let mut tasks = Vec::new();
        for (t, &(lm, ls, lu)) in leader_cells {
            let &(hm, hs, hu) = hc.get(t).unwrap_or(&(0, 0, 0));
            if lu == 0 && hu == 0 && lm > 0 && hm > 0 {
                let dl = i128::from(ls) * i128::from(hm) - i128::from(hs) * i128::from(lm);
                deltas.push(round_div(
                    dl * i128::from(PPM),
                    i128::from(lm) * i128::from(hm),
                ));
                tasks.push(t.clone());
            }
        }
        let interval = if deltas.len() >= 2 && deltas.iter().collect::<BTreeSet<_>>().len() >= 2 {
            let sm = format!("tied-with-leader/1|{leader}|{arm}|{seed}|{deltas:?}");
            bootstrap_paired_with(&deltas, LEVEL, false, &sm, draws as u32)
        } else {
            None
        };
        rows.push(TiedRow {
            arm_id: arm.clone(),
            tied: interval.map(|i| i.lo <= 0 && 0 <= i.hi),
            diff_interval: interval,
            tasks,
        });
    }
    let mut keys = BTreeMap::new();
    keys.insert("leader".into(), leader.clone());
    keys.insert("arms".into(), arms.join("|"));
    Ok(TiedReport {
        leader_id: leader,
        leader_contested: contested,
        theta,
        rows,
        draws,
        use_as: u.as_str().into(),
        labels: vec![LABEL_DESCRIPTIVE.into(), LABEL_EXPLORATORY_LEADER.into()],
        metadata: meta(
            "tied_with_leader",
            TIED_VERSION,
            "descriptive tied-with-leader route",
            vec![
                "leader = argmax theta; tied iff the task-bootstrap 95% interval of delta(L - h) covers 0".into(),
                "descriptive/exploratory with the selection caveat — never equivalence evidence (REQ-XD-031)".into(),
            ],
            keys,
            vec![],
            seed,
            draws,
            ("descriptive".to_string(), Some(LEVEL)),
        ),
    })
}

// ── 5. the estimand + §6.8 envelope (REQ-XD-001/061) ─────────────────

/// The declared estimand inputs (records-in).
#[derive(Debug, Clone)]
pub struct EstimandDecl {
    /// The named population (`joint_task_pass_without_observed_veto`
    /// primary; `raw`/`errors-zero` bounds are secondary).
    pub population: PopulationKind,
    /// The admitted arms (≥ 2).
    pub arms: Vec<String>,
    /// The pre-registered reference arm.
    pub reference_arm: String,
    /// The declared stratum.
    pub stratum: Option<String>,
    /// The declared use.
    pub use_as: String,
    /// The GATE-G23 record.
    pub admission: Option<AdmissionRecord>,
}

/// A labelled conditional-secondary estimate (§6.8 — eligible-only
/// only ever labelled, never the primary).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalEstimate {
    /// The mandatory label.
    pub label: String,
    /// The eligible tasks the estimate reads.
    pub tasks: Vec<String>,
    /// The tasks excluded by resolution status.
    pub excluded_tasks: Vec<String>,
    /// The estimate (ppm).
    pub value_ppm: i64,
}

/// One arm's estimand row.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmEstimand {
    /// The arm.
    pub arm_id: String,
    /// θ̂_h — `None` when any cell is unresolved (the point needs the
    /// executed manifest complete).
    pub theta_ppm: Option<i64>,
    /// Tasks whose cells are unresolved.
    pub unresolved_tasks: Vec<String>,
    /// The all-planned bound `[s/M,(s+u)/M]`.
    pub bounds: Interval,
    /// The completion envelope `{planned, first, recovered, unresolved}`.
    pub completion: (u64, u64, u64, u64),
    /// The labelled eligible-only conditional secondary.
    pub conditional: Option<ConditionalEstimate>,
}

/// One primary contrast's estimand row.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastEstimand {
    /// The test arm.
    pub test_arm: String,
    /// The control arm (the reference).
    pub control_arm: String,
    /// The observed `Δ = mean_t d_t` — `None` when a primary cell is
    /// unresolved/short.
    pub delta_ppm: Option<i64>,
    /// The typed refusal when `delta_ppm` is `None` (§6.8 — a member,
    /// never a silence).
    pub refusal: Option<String>,
    /// The all-planned contrast bound.
    pub bounds: Interval,
    /// The labelled eligible-only conditional secondary.
    pub conditional: Option<ConditionalEstimate>,
}

/// The estimand report.
#[derive(Debug, Clone, PartialEq)]
pub struct EstimandReport {
    /// The declared population.
    pub population: String,
    /// The reference arm.
    pub reference_arm: String,
    /// The per-arm rows.
    pub arms: Vec<ArmEstimand>,
    /// The per-contrast rows (`h − ref` for every h ≠ ref).
    pub contrasts: Vec<ContrastEstimand>,
    /// The declared use.
    pub use_as: String,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// `comparative_estimand(pop, decl)` — θ_h and Δ(h − ref) over the
/// executed held-out manifest (REQ-XD-001) with the §6.8
/// unresolved-cell refusal, all-planned bounds and the completion
/// envelope (REQ-XD-061).
pub fn comparative_estimand(
    pop: &PlannedPopulation,
    decl: &EstimandDecl,
) -> Result<EstimandReport, EstimatorError> {
    let u = check_use(
        "comparative_estimand",
        &decl.use_as,
        decl.admission.as_ref(),
    )?;
    if decl.arms.len() < 2 {
        return Err(EstimatorError::MissingKey { member: "arms" });
    }
    if !decl.arms.iter().any(|a| a == &decl.reference_arm) {
        return Err(EstimatorError::UnknownArm {
            arm_id: decl.reference_arm.clone(),
        });
    }
    let st = decl.stratum.as_deref();
    let mut arm_rows = Vec::new();
    let mut per_arm: BTreeMap<String, BTreeMap<String, (u64, u64, u64)>> = BTreeMap::new();
    for arm in &decl.arms {
        let (m, _) = arm_cells(pop, decl.population, arm, st)?;
        // An arm's manifest = the tasks its slots cover; the executed
        // manifest pairs cells across arms — a task absent for this
        // arm is surfaced on the contrast leg.
        let mut unresolved = Vec::new();
        let mut eligible = Vec::new();
        for (t, &(mm, _s, uu)) in &m {
            if uu > 0 {
                unresolved.push(t.clone());
            } else if mm > 0 {
                eligible.push(t.clone());
            }
        }
        let theta = if unresolved.is_empty() {
            Some(round_div(
                m.values()
                    .map(|&(mm, s, _)| {
                        round_div(i128::from(s) * i128::from(PPM), i128::from(mm)) as i128
                    })
                    .sum::<i128>(),
                m.len() as i128,
            ))
        } else {
            None
        };
        let conditional = if unresolved.is_empty() || eligible.len() < 2 {
            None
        } else {
            let v = round_div(
                eligible
                    .iter()
                    .map(|t| {
                        let (mm, s, _) = m[t];
                        round_div(i128::from(s) * i128::from(PPM), i128::from(mm)) as i128
                    })
                    .sum::<i128>(),
                eligible.len() as i128,
            );
            Some(ConditionalEstimate {
                label: LABEL_CONDITIONAL_SECONDARY.into(),
                excluded_tasks: unresolved.clone(),
                tasks: eligible,
                value_ppm: v,
            })
        };
        let b = arm_bounds(pop, decl.population, arm)?;
        arm_rows.push(ArmEstimand {
            arm_id: arm.clone(),
            theta_ppm: theta,
            unresolved_tasks: unresolved,
            bounds: Interval {
                lo: b.lo_ppm,
                hi: b.hi_ppm,
            },
            completion: completion_envelope(pop, arm),
            conditional,
        });
        per_arm.insert(arm.clone(), m);
    }
    // Per-contrast Δ(h − ref): a residual unresolved/short cell makes
    // the contrast refuse `UnresolvedCell`/`ShortCell` (§6.8); the
    // all-planned bound still publishes; an eligible-only Δ is a
    // labelled conditional secondary.
    let refc = &per_arm[&decl.reference_arm];
    let mut contrasts = Vec::new();
    for arm in &decl.arms {
        if arm == &decl.reference_arm {
            continue;
        }
        let hc = &per_arm[arm];
        // The paired manifest — a task absent for either arm pairs no
        // cell and is named (never silently dropped).
        for t in refc.keys() {
            if !hc.contains_key(t) {
                return Err(EstimatorError::UnpairedTask {
                    task_id: t.clone(),
                    arm_id: arm.clone(),
                });
            }
        }
        for t in hc.keys() {
            if !refc.contains_key(t) {
                return Err(EstimatorError::UnpairedTask {
                    task_id: t.clone(),
                    arm_id: decl.reference_arm.clone(),
                });
            }
        }
        let (mut lo_sum, mut hi_sum) = (0i128, 0i128);
        let mut deltas = Vec::new();
        let mut excluded = Vec::new();
        let mut refusal: Option<String> = None;
        for (t, &(hm, hs, hu)) in hc {
            let &(rm, rs, ru) = &refc[t];
            // all-planned bound leg — computed inward, rounded outward.
            let h_lo = (i128::from(hs) * i128::from(PPM)) / i128::from(hm);
            let h_hi =
                (i128::from(hs + hu) * i128::from(PPM) + i128::from(hm) - 1) / i128::from(hm);
            let r_lo = (i128::from(rs) * i128::from(PPM)) / i128::from(rm);
            let r_hi =
                (i128::from(rs + ru) * i128::from(PPM) + i128::from(rm) - 1) / i128::from(rm);
            lo_sum += h_lo - r_hi;
            hi_sum += h_hi - r_lo;
            if hu > 0 || ru > 0 {
                excluded.push(t.clone());
                if refusal.is_none() {
                    refusal = Some(if hu > 0 {
                        format!(
                            "UnresolvedCell: cell ({t}, {arm}) carries {hu} unresolved member(s)"
                        )
                    } else {
                        format!(
                            "UnresolvedCell: cell ({t}, {}) carries {ru} unresolved member(s)",
                            decl.reference_arm
                        )
                    });
                }
                continue;
            }
            if hm < 5 || rm < 5 {
                excluded.push(t.clone());
                if refusal.is_none() {
                    refusal = Some(format!(
                        "insufficient_replicates: cell ({t}, {arm}/{}) below the R >= 5 floor",
                        decl.reference_arm
                    ));
                }
                continue;
            }
            deltas.push((
                t.clone(),
                round_div(
                    (i128::from(hs) * i128::from(rm) - i128::from(rs) * i128::from(hm))
                        * i128::from(PPM),
                    i128::from(hm) * i128::from(rm),
                ),
            ));
        }
        let t_n = i128::from(hc.len() as u64);
        let conditional = if excluded.is_empty() || deltas.len() < 2 {
            None
        } else {
            Some(ConditionalEstimate {
                label: LABEL_CONDITIONAL_SECONDARY.into(),
                tasks: deltas.iter().map(|(t, _)| t.clone()).collect(),
                excluded_tasks: excluded,
                value_ppm: round_div(
                    deltas.iter().map(|(_, d)| *d as i128).sum::<i128>(),
                    deltas.len() as i128,
                ),
            })
        };
        contrasts.push(ContrastEstimand {
            test_arm: arm.clone(),
            control_arm: decl.reference_arm.clone(),
            delta_ppm: if refusal.is_none() {
                Some(round_div(
                    deltas.iter().map(|(_, d)| *d as i128).sum::<i128>(),
                    deltas.len() as i128,
                ))
            } else {
                None
            },
            refusal,
            bounds: Interval {
                lo: (lo_sum / t_n) as i64,
                hi: ((hi_sum + t_n - 1) / t_n) as i64,
            },
            conditional,
        });
    }
    let mut keys = BTreeMap::new();
    keys.insert("population".into(), decl.population.as_str().to_string());
    keys.insert("reference_arm".into(), decl.reference_arm.clone());
    keys.insert("arms".into(), decl.arms.join("|"));
    Ok(EstimandReport {
        population: decl.population.as_str().to_string(),
        reference_arm: decl.reference_arm.clone(),
        arms: arm_rows,
        contrasts,
        use_as: u.as_str().into(),
        metadata: meta(
            "comparative_estimand",
            ESTIMAND_VERSION,
            "theta_h and delta(h - ref) over the executed held-out manifest",
            vec![
                "equal-task mean of per-task replicate means; first subject or first admissible completed retry".into(),
                "a residual unresolved cell refuses the involving primary contrasts (§6.8)".into(),
                "all-planned bounds and the completion envelope publish; eligible-only is a labelled conditional secondary".into(),
            ],
            keys,
            vec![],
            0,
            0,
            ("descriptive".to_string(), None),
        ),
    })
}

// ── 6. the per-tranche family (REQ-XD-030) ───────────────────────────

/// The declared family inputs (records-in).
#[derive(Debug, Clone)]
pub struct FamilyDeclC {
    /// The tranche id.
    pub tranche_id: String,
    /// The frozen primary family id.
    pub family_id: String,
    /// The frozen exploratory family id.
    pub exploratory_id: String,
    /// The admitted arms (≥ 2).
    pub arms: Vec<String>,
    /// The pre-registered reference arm.
    pub reference_arm: String,
    /// The primary metric — [`PRIMARY_METRIC`] only (REQ-XD-030).
    pub metric: String,
    /// The population — `joint_task_pass_without_observed_veto` only
    /// for the primary family.
    pub population: PopulationKind,
    /// The declared stratum.
    pub stratum: Option<String>,
    /// The exchangeability spelling — [`EXCHANGEABILITY`] only.
    pub exchangeability: String,
    /// The R6.18 schedule-randomization evidence (REQ-XD-021).
    pub schedule: Option<ScheduleEvidence>,
    /// The declared seed (mixed into member sign-flip material).
    pub seed: u64,
    /// The declared use.
    pub use_as: String,
    /// The GATE-G23 record.
    pub admission: Option<AdmissionRecord>,
}

/// One family member's row.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyRowC {
    /// The member id (`h` vs ref for primary; `g|h` for exploratory).
    pub member: String,
    /// `computed` | `unavailable` (retained at worst-case mass — never
    /// dropped, REQ-XD-030).
    pub status: String,
    /// Why the member is unavailable (a member, never a silence).
    pub unavailable: Option<String>,
    /// The observed Δ (ppm) when computed.
    pub delta_ppm: Option<i64>,
    /// The raw sign-flip p (ppm).
    pub raw_p_ppm: Option<i64>,
    /// The family-adjusted value (ppm).
    pub adjusted_ppm: i64,
    /// Rejected at α = .05 (Holm rows only).
    pub rejected_at_alpha: bool,
    /// The sign-flip mode (`exact` | `monte_carlo`) when computed.
    pub mode: Option<String>,
}

/// The per-tranche family report.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparativeFamily {
    /// The tranche.
    pub tranche_id: String,
    /// The primary family `{id, method:holm, α, reference, members}`.
    pub family: (String, Vec<FamilyRowC>),
    /// The exploratory family `{id, method:benjamini_hochberg, members}`.
    pub exploratory: (String, Vec<FamilyRowC>),
    /// The declared use.
    pub use_as: String,
    /// The mandatory labels.
    pub labels: Vec<String>,
    /// The S.1 metadata.
    pub metadata: MethodMetadata,
}

/// `confirmatory_family(pop, decl)` — the per-tranche many-to-one
/// family: Δ(h − ref) for every admitted h ≠ ref under
/// `capability.task_success` in the joint population, sharp-null
/// sign-flip p under `slot_schedule_randomization`, Holm α = .05;
/// other pairs form the exploratory BH family; unavailable members
/// stay in the family (REQ-XD-030).
pub fn confirmatory_family(
    pop: &PlannedPopulation,
    decl: &FamilyDeclC,
) -> Result<ComparativeFamily, EstimatorError> {
    let u = check_use("confirmatory_family", &decl.use_as, decl.admission.as_ref())?;
    if decl.metric != PRIMARY_METRIC {
        return Err(EstimatorError::NonFiniteInput {
            detail: format!(
                "primary metric is {PRIMARY_METRIC} — declared {}",
                decl.metric
            ),
        });
    }
    if decl.population != PopulationKind::JointTaskPassWithoutObservedVeto {
        return Err(EstimatorError::NonFiniteInput {
            detail: "the primary family reads the joint population only".into(),
        });
    }
    if decl.exchangeability != EXCHANGEABILITY {
        return Err(EstimatorError::ExchangeabilityNotAdmitted {
            declared: decl.exchangeability.clone(),
        });
    }
    let ev = decl
        .schedule
        .as_ref()
        .ok_or_else(|| EstimatorError::ExchangeabilityNotAdmitted {
            declared: "slot_schedule_randomization (no randomization record)".into(),
        })?;
    ev.check()
        .map_err(|detail| EstimatorError::RandomizationRecord { detail })?;
    if decl.family_id.is_empty() || decl.exploratory_id.is_empty() || decl.tranche_id.is_empty() {
        return Err(EstimatorError::MissingKey {
            member: "family/tranche id",
        });
    }
    if decl.arms.len() < 2 || !decl.arms.iter().any(|a| a == &decl.reference_arm) {
        return Err(EstimatorError::MissingKey {
            member: "arms/reference",
        });
    }
    let st = decl.stratum.as_deref();
    // Per-arm cells once.
    let mut per_arm: BTreeMap<String, BTreeMap<String, (u64, u64, u64)>> = BTreeMap::new();
    for arm in &decl.arms {
        let (m, _) = arm_cells(pop, decl.population, arm, st)?;
        per_arm.insert(arm.clone(), m);
    }
    // One member's deltas — `Err(reason)` keeps the member unavailable.
    let member_deltas = |test: &str, control: &str| -> Result<Vec<i64>, String> {
        let tc = &per_arm[test];
        let cc = &per_arm[control];
        for t in tc.keys().chain(cc.keys()) {
            if !tc.contains_key(t) {
                return Err(format!("UnpairedTask: {t} has no cell for {test}"));
            }
            if !cc.contains_key(t) {
                return Err(format!("UnpairedTask: {t} has no cell for {control}"));
            }
        }
        let mut ds = Vec::with_capacity(tc.len());
        for (t, &(tm_, ts_, tu)) in tc {
            let &(cm, cs, cu) = &cc[t];
            if tu > 0 {
                return Err(format!("UnresolvedCell: ({t}, {test}) u={tu}"));
            }
            if cu > 0 {
                return Err(format!("UnresolvedCell: ({t}, {control}) u={cu}"));
            }
            if tm_ < 5 || cm < 5 {
                return Err(format!("insufficient_replicates: ({t}, {test}|{control})"));
            }
            ds.push(round_div(
                (i128::from(ts_) * i128::from(cm) - i128::from(cs) * i128::from(tm_))
                    * i128::from(PPM),
                i128::from(tm_) * i128::from(cm),
            ));
        }
        if ds.len() < 2 {
            return Err("one task — the family member needs T >= 2".into());
        }
        Ok(ds)
    };
    let member_row = |id: &str, test: &str, control: &str| -> (FamilyRowC, i64) {
        match member_deltas(test, control) {
            Err(reason) => (
                FamilyRowC {
                    member: id.into(),
                    status: "unavailable".into(),
                    unavailable: Some(reason),
                    delta_ppm: None,
                    raw_p_ppm: None,
                    adjusted_ppm: 0,
                    rejected_at_alpha: false,
                    mode: None,
                },
                PPM, // worst-case mass — retained, never dropped.
            ),
            Ok(deltas) => {
                let mean = round_div(
                    deltas.iter().map(|&d| d as i128).sum::<i128>(),
                    deltas.len() as i128,
                );
                let p = permutation_signflip_p(
                    &deltas,
                    SIGNFLIP_DRAWS,
                    &format!("comparative-family/1|{}|{id}|{test}|{control}", decl.seed),
                )
                .unwrap_or(PPM);
                (
                    FamilyRowC {
                        member: id.into(),
                        status: "computed".into(),
                        unavailable: None,
                        delta_ppm: Some(mean),
                        raw_p_ppm: Some(p),
                        adjusted_ppm: 0,
                        rejected_at_alpha: false,
                        mode: Some(
                            if deltas.len() <= SIGNFLIP_EXACT_MAX_N {
                                "exact"
                            } else {
                                "monte_carlo"
                            }
                            .into(),
                        ),
                    },
                    p,
                )
            }
        }
    };
    // Primary: Δ(h − ref) for every admitted h ≠ ref — unavailable
    // members stay in the family at worst-case mass.
    let mut prim = Vec::new();
    let mut raw = Vec::new();
    for h in &decl.arms {
        if h == &decl.reference_arm {
            continue;
        }
        let (row, p) = member_row(h, h, &decl.reference_arm);
        prim.push(row);
        raw.push(p);
    }
    let adj = holm(&raw);
    for (i, r) in prim.iter_mut().enumerate() {
        r.adjusted_ppm = adj[i];
        r.rejected_at_alpha = adj[i] <= ALPHA_PPM;
    }
    // Exploratory: every other pair under BH.
    let others: Vec<&String> = decl
        .arms
        .iter()
        .filter(|a| *a != &decl.reference_arm)
        .collect();
    let mut expl = Vec::new();
    let mut eraw = Vec::new();
    for i in 0..others.len() {
        for j in (i + 1)..others.len() {
            let (g, h) = (others[i], others[j]);
            let (row, p) = member_row(&format!("{g}|{h}"), g, h);
            expl.push(row);
            eraw.push(p);
        }
    }
    let eadj = benjamini_hochberg(&eraw);
    for (i, r) in expl.iter_mut().enumerate() {
        r.adjusted_ppm = eadj[i];
    }
    let mut keys = BTreeMap::new();
    keys.insert("tranche".into(), decl.tranche_id.clone());
    keys.insert("family".into(), decl.family_id.clone());
    keys.insert("reference".into(), decl.reference_arm.clone());
    Ok(ComparativeFamily {
        tranche_id: decl.tranche_id.clone(),
        family: (decl.family_id.clone(), prim),
        exploratory: (decl.exploratory_id.clone(), expl),
        use_as: u.as_str().into(),
        labels: vec![
            format!("confirmatory family {}; α = .05 Holm", decl.family_id),
            LABEL_EXPLORATORY_LEADER.into(),
        ],
        metadata: meta(
            "confirmatory_family",
            FAMILY_VERSION,
            "per-tranche many-to-one Holm family over sharp-null sign-flip p",
            vec![
                "delta(h - ref) for every admitted h != ref under capability.task_success in the joint population".into(),
                "unavailable members retained at worst-case mass; other pairs exploratory BH".into(),
                "slot_schedule_randomization requires the recorded randomization record (REQ-XD-021)".into(),
            ],
            keys,
            vec![],
            decl.seed,
            SIGNFLIP_DRAWS,
            ("descriptive".to_string(), None),
        ),
    })
}

// ── 7. hh-comparative-results/1 (C-33; REQ-XF-123, REQ-XD-090) ───────

// ── validator helpers (plain fns — one `errs` channel) ───────────────

fn req<'a>(j: &'a Json, path: &str, member: &str, errs: &mut Vec<String>) -> Option<&'a Json> {
    match j.get(member) {
        Some(v) if !matches!(v, Json::Null) => Some(v),
        _ => {
            errs.push(format!("{path}.{member}: required member absent"));
            None
        }
    }
}

fn int_at(j: &Json, path: &str, member: &str, errs: &mut Vec<String>) {
    match req(j, path, member, errs) {
        Some(v) if v.as_int().is_some() => {}
        Some(_) => errs.push(format!("{path}.{member}: must be an integer")),
        None => {}
    }
}

fn str_at(j: &Json, path: &str, member: &str, errs: &mut Vec<String>) {
    match req(j, path, member, errs) {
        Some(v) if v.as_str().map(|s| !s.is_empty()).unwrap_or(false) => {}
        Some(_) => errs.push(format!("{path}.{member}: must be a non-empty string")),
        None => {}
    }
}

/// An absent value is a typed object with `state` in the closed set —
/// never `null`, never `0` (REQ-XF-122's class).
fn typed_absent(v: &Json) -> bool {
    v.get("state")
        .and_then(Json::as_str)
        .map(|s| ABSENT_STATES.contains(&s))
        .unwrap_or(false)
}

fn interval_at(j: &Json, path: &str, member: &str, errs: &mut Vec<String>) {
    match req(j, path, member, errs) {
        Some(v) if typed_absent(v) => {}
        Some(v) => {
            let p = format!("{path}.{member}");
            int_at(v, &p, "lo_ppm", errs);
            int_at(v, &p, "hi_ppm", errs);
            str_at(v, &p, "kind", errs);
        }
        None => {}
    }
}

fn at(errs: &mut Vec<String>, path: &str, detail: &str) {
    errs.push(format!("{path}: {detail}"));
}

/// `validate_results_packet(j)` — the `hh-comparative-results/1`
/// validator: every §6.11 quantity with its interval and label, the
/// F2 F8 site-facing fields (`harness_excess` with its interval), the
/// mandatory label strings, per-arm intended-vs-served counts, the
/// three populations, and typed absent-value objects (never `null`).
/// `Err` lists every violation `(path, detail)` in canonical order.
pub fn validate_results_packet(j: &Json) -> Result<(), Vec<String>> {
    let mut errs: Vec<String> = Vec::new();
    // ── top level ────────────────────────────────────────────────────
    match j.get("schema").and_then(Json::as_str) {
        Some(RESULTS_SCHEMA) => {}
        other => at(
            &mut errs,
            "schema",
            &format!("must be {RESULTS_SCHEMA} — got {other:?}"),
        ),
    }
    if let Some(t) = req(j, "", "tranche", &mut errs) {
        str_at(t, "tranche", "id", &mut errs);
    }
    if let Some(f) = req(j, "", "family", &mut errs) {
        str_at(f, "family", "id", &mut errs);
        if f.get("method").and_then(Json::as_str) != Some("holm") {
            at(&mut errs, "family.method", "must be \"holm\"");
        }
        int_at(f, "family", "alpha_ppm", &mut errs);
        str_at(f, "family", "reference", &mut errs);
        match f.get("members") {
            Some(Json::Arr(ms)) if !ms.is_empty() => {
                for (i, m) in ms.iter().enumerate() {
                    let p = format!("family.members[{i}]");
                    str_at(m, &p, "member", &mut errs);
                    str_at(m, &p, "status", &mut errs);
                    match m.get("raw_p_ppm") {
                        Some(v) if v.as_int().is_some() || matches!(v, Json::Null) => {}
                        _ => at(&mut errs, &format!("{p}.raw_p_ppm"), "must be int or null"),
                    }
                    int_at(m, &p, "adjusted_ppm", &mut errs);
                }
            }
            _ => at(
                &mut errs,
                "family.members",
                "a non-empty member set is required (unavailable retained)",
            ),
        }
    }
    if let Some(e) = req(j, "", "exploratory_family", &mut errs) {
        str_at(e, "exploratory_family", "id", &mut errs);
        if e.get("method").and_then(Json::as_str) != Some("benjamini_hochberg") {
            at(
                &mut errs,
                "exploratory_family.method",
                "must be \"benjamini_hochberg\"",
            );
        }
        match e.get("members") {
            Some(Json::Arr(_)) => {}
            _ => at(
                &mut errs,
                "exploratory_family.members",
                "member array required",
            ),
        }
    }
    if j.get("metric").and_then(Json::as_str) != Some(PRIMARY_METRIC) {
        at(&mut errs, "metric", &format!("must be {PRIMARY_METRIC}"));
    }
    match j.get("populations") {
        Some(Json::Arr(ps)) => {
            let have: Vec<&str> = ps.iter().filter_map(Json::as_str).collect();
            for want in REQUIRED_POPULATIONS {
                if !have.contains(want) {
                    at(
                        &mut errs,
                        "populations",
                        &format!("missing population {want}"),
                    );
                }
            }
        }
        _ => at(
            &mut errs,
            "populations",
            "array of population keys required",
        ),
    }
    if let Some(labels) = req(j, "", "labels", &mut errs) {
        for (key, subs) in REQUIRED_LABELS {
            match labels.get(key).and_then(Json::as_str) {
                Some(s) => {
                    for sub in *subs {
                        if !s.contains(sub) {
                            at(
                                &mut errs,
                                &format!("labels.{key}"),
                                &format!("must carry \"{sub}\""),
                            );
                        }
                    }
                }
                None => at(
                    &mut errs,
                    &format!("labels.{key}"),
                    "mandatory label absent",
                ),
            }
        }
    }
    // ── per harness ──────────────────────────────────────────────────
    match j.get("harnesses") {
        Some(Json::Obj(hs)) if hs.len() >= 2 => {
            for (arm, h) in hs {
                let p = format!("harnesses.{arm}");
                if let Some(th) = req(h, &p, "theta", &mut errs) {
                    int_at(th, &format!("{p}.theta"), "ppm", &mut errs);
                    int_at(th, &format!("{p}.theta"), "lo_ppm", &mut errs);
                    int_at(th, &format!("{p}.theta"), "hi_ppm", &mut errs);
                    interval_at(th, &format!("{p}.theta"), "interval", &mut errs);
                    interval_at(th, &format!("{p}.theta"), "bounds", &mut errs);
                    str_at(th, &format!("{p}.theta"), "label", &mut errs);
                }
                if let Some(sp) = req(h, &p, "secondary_populations", &mut errs) {
                    for pop in ["raw", "errors_zero"] {
                        if let Some(v) =
                            req(sp, &format!("{p}.secondary_populations"), pop, &mut errs)
                        {
                            str_at(
                                v,
                                &format!("{p}.secondary_populations.{pop}"),
                                "label",
                                &mut errs,
                            );
                        }
                    }
                }
                if let Some(d) = req(h, &p, "delta_vs_ref", &mut errs) {
                    if !typed_absent(d) {
                        int_at(d, &format!("{p}.delta_vs_ref"), "ppm", &mut errs);
                        interval_at(d, &format!("{p}.delta_vs_ref"), "interval", &mut errs);
                        int_at(d, &format!("{p}.delta_vs_ref"), "p_raw_ppm", &mut errs);
                        int_at(d, &format!("{p}.delta_vs_ref"), "p_holm_ppm", &mut errs);
                        str_at(d, &format!("{p}.delta_vs_ref"), "label", &mut errs);
                    }
                }
                if let Some(lg) = req(h, &p, "leader_group", &mut errs) {
                    match lg.get("member") {
                        Some(Json::Bool(_)) => {}
                        _ => at(
                            &mut errs,
                            &format!("{p}.leader_group.member"),
                            "must be bool",
                        ),
                    }
                    str_at(lg, &format!("{p}.leader_group"), "leader_id", &mut errs);
                    interval_at(lg, &format!("{p}.leader_group"), "diff_interval", &mut errs);
                    str_at(lg, &format!("{p}.leader_group"), "label", &mut errs);
                }
                if let Some(pa) = req(h, &p, "pass_at", &mut errs) {
                    int_at(pa, &format!("{p}.pass_at"), "k", &mut errs);
                    for m in ["all_ppm", "any_ppm"] {
                        match pa.get(m) {
                            Some(v) if v.as_int().is_some() || typed_absent(v) => {}
                            _ => at(
                                &mut errs,
                                &format!("{p}.pass_at.{m}"),
                                "int or typed absent required",
                            ),
                        }
                    }
                }
                if let Some(fl) = req(h, &p, "flips", &mut errs) {
                    if let Some(r) = req(fl, &format!("{p}.flips"), "rerun", &mut errs) {
                        if !typed_absent(r) {
                            int_at(r, &format!("{p}.flips.rerun"), "ppm", &mut errs);
                            interval_at(r, &format!("{p}.flips.rerun"), "interval", &mut errs);
                        }
                    }
                    if let Some(sv) = req(fl, &format!("{p}.flips"), "swap_vs", &mut errs) {
                        match sv {
                            Json::Obj(o) if !o.is_empty() => {
                                for (other, v) in o {
                                    interval_at(
                                        v,
                                        &format!("{p}.flips.swap_vs.{other}"),
                                        "interval",
                                        &mut errs,
                                    );
                                }
                            }
                            _ => at(
                                &mut errs,
                                &format!("{p}.flips.swap_vs"),
                                "object of pairs required",
                            ),
                        }
                    }
                    // `harness_excess` carries its own interval —
                    // REQ-XF-123 names it verbatim.
                    if let Some(he) = req(fl, &format!("{p}.flips"), "harness_excess", &mut errs) {
                        match he {
                            Json::Obj(o) if !o.is_empty() => {
                                for (other, v) in o {
                                    let q = format!("{p}.flips.harness_excess.{other}");
                                    if typed_absent(v) {
                                        continue;
                                    }
                                    int_at(v, &q, "ppm", &mut errs);
                                    interval_at(v, &q, "interval", &mut errs);
                                }
                            }
                            _ => at(
                                &mut errs,
                                &format!("{p}.flips.harness_excess"),
                                "object of pairs required",
                            ),
                        }
                    }
                }
                if let Some(c) = req(h, &p, "cost", &mut errs) {
                    if let Some(pt) = req(c, &format!("{p}.cost"), "per_trial", &mut errs) {
                        int_at(pt, &format!("{p}.cost.per_trial"), "micros", &mut errs);
                        interval_at(pt, &format!("{p}.cost.per_trial"), "interval", &mut errs);
                    }
                    match c.get("per_solve") {
                        Some(v) if typed_absent(v) => {}
                        Some(v) => {
                            int_at(v, &format!("{p}.cost.per_solve"), "micros", &mut errs);
                            interval_at(v, &format!("{p}.cost.per_solve"), "interval", &mut errs);
                        }
                        None => at(
                            &mut errs,
                            &format!("{p}.cost.per_solve"),
                            "required (typed estimator_undefined admissible)",
                        ),
                    }
                    match c.get("provenance").and_then(Json::as_str) {
                        Some("model_spend_only") | Some("with_compute") => {}
                        _ => at(
                            &mut errs,
                            &format!("{p}.cost.provenance"),
                            "must be model_spend_only|with_compute",
                        ),
                    }
                }
                if let Some(n) = req(h, &p, "counts", &mut errs) {
                    match n.get("analysis_basis").and_then(Json::as_str) {
                        Some("assignment") | Some("served") => {}
                        _ => at(
                            &mut errs,
                            &format!("{p}.counts.analysis_basis"),
                            "must be assignment|served",
                        ),
                    }
                    for m in ["intended", "served"] {
                        match n.get(m) {
                            Some(Json::Obj(o)) if !o.is_empty() => {
                                for (k, v) in o {
                                    if v.as_int().is_none() {
                                        at(
                                            &mut errs,
                                            &format!("{p}.counts.{m}.{k}"),
                                            "must be an integer count",
                                        );
                                    }
                                }
                            }
                            _ => at(
                                &mut errs,
                                &format!("{p}.counts.{m}"),
                                "non-empty object of counts required",
                            ),
                        }
                    }
                }
                if let Some(cp) = req(h, &p, "completion", &mut errs) {
                    for m in ["planned", "first", "recovered", "unresolved"] {
                        int_at(cp, &format!("{p}.completion"), m, &mut errs);
                    }
                }
                if let Some(ig) = req(h, &p, "integrity", &mut errs) {
                    for m in ["flagged", "confirmed", "cleared", "coverage_ppm"] {
                        int_at(ig, &format!("{p}.integrity"), m, &mut errs);
                    }
                    str_at(ig, &format!("{p}.integrity"), "detector", &mut errs);
                    str_at(ig, &format!("{p}.integrity"), "label", &mut errs);
                }
            }
        }
        _ => at(
            &mut errs,
            "harnesses",
            "an object of >= 2 harness rows required",
        ),
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}
