//! `finite_manifest` — the finite-manifest Bayesian efficacy estimator
//! (R5.16; REQ-D-107 primary; the GATE-G9-admitted Packet D /
//! PADR-R5-4* method contract over the R5.15 population contract's
//! cells, dossier S.1–S.6 spine).
//!
//! For the frozen finite manifest within one declared contamination
//! stratum, each `(task, arm)` cell is an independent binomial —
//! `c_at | p_at ~ Binomial(n_at, p_at)` with `p_at ~ Beta(1,1)`,
//! posteriors `Beta(c_at+1, n_at−c_at+1)` independent across task/arm
//! cells. Posterior draws `Δ^(b) = Σ_t w_t [p_Bt^(b) − p_At^(b)]`,
//! `w_t = 1/T`, are `i64` ppm variates via integer inverse-CDF
//! bisection on [`crate::stats::regularized_beta`] under the
//! canonical-input-seeded [`crate::stats::XorShift64`]. The report
//! carries the analytic posterior mean, the observed equal-task
//! contrast beside it, and a labelled 95% equal-tail **`credible`**
//! interval at the declared **8,000** draws, with the frozen doubling
//! check recomputed at 16,000/32,000/64,000 draws and the max
//! endpoint delta recorded as `numerical_error` — a fact, never a
//! silent retry or redraw (S.3). `Beta{2,2}` is the predeclared
//! exploratory sensitivity and never replaces the registered prior —
//! the declared prior parameterizes the conjugate update (posterior
//! mean `(c+a)/(n+a+b)`, draws `beta_inv(u, c+a, n−c+b)`); a `beta{2,2}`
//! label over `Beta(1,1)` math was the DF-R5.17-1 defect, repaired at
//! R5.40.
//!
//! Task pairing is stratification, not correlated-replicate modelling:
//! a declared dependence, a pooled/hierarchical substitute, an unknown
//! method/prior, missing or short cells (`R ≥ 5` first-slot floor,
//! §4.1), one task, a non-unit declared bound, a level other than the
//! frozen 95%, a metadata-only declaration, or a credible-as-TOST use
//! are all typed refusals — never warnings, never a fabricated
//! interval. Constant binary samples keep a nonzero-width posterior
//! (the Beta posterior never degenerates).

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::population::{
    contrib, BoundsDecl, MethodMetadata, PlannedPopulation, PopulationError, PopulationKind,
};
use crate::stats::{quantile, regularized_beta, Interval, XorShift64, PPM};

/// The analysis `kind` spelling that routes to this estimator.
pub const CONTRAST_KIND: &str = "finite_manifest_contrast";
/// The method contract version (report `metadata.version`).
pub const CONTRAST_VERSION: &str = "finite-manifest-contrast/1";
/// The declared posterior draw count for the reported interval (S.3 base).
pub const BASE_DRAWS: u64 = 8_000;
/// The frozen doubling cap — the error-check ladder recomputes at
/// 2× base through this ceiling (S.3 cap).
pub const CAP_DRAWS: u64 = 64_000;
/// The frozen credible level — 95% equal-tail (the method's declared
/// level; any other declared level is a different method → refuse).
pub const LEVEL_PPM: i64 = 950_000;
/// The equal-tail mass on each side at [`LEVEL_PPM`].
pub const TAIL_PPM: i64 = (PPM - LEVEL_PPM) / 2;
/// The first-slot replicate floor per `(task, arm)` cell (§4.1:
/// `R ≥ 5/task/arm`; the floor is not power).
pub const CELL_REPLICATE_FLOOR: u64 = 5;
/// The integer-ppm inverse-CDF grid residual — the tolerance declared
/// before any numerical execution (S.2/S.3): an endpoint on the ppm
/// grid is exact to ≤ 1 ppm by construction.
pub const INVCDF_TOL_PPM: i64 = 1;
/// The registered prior — `Beta(1,1)`.
pub const REGISTERED_PRIOR: (u64, u64) = (1, 1);
/// The predeclared exploratory sensitivity prior — `Beta(2,2)`; it
/// never replaces the registered prior's headline.
pub const SENSITIVITY_PRIOR: (u64, u64) = (2, 2);
/// The canonical prior-name spellings the declared method admits.
pub const REGISTERED_PRIOR_NAME: &str = "beta{1,1}";
/// The sensitivity prior-name spelling.
pub const SENSITIVITY_PRIOR_NAME: &str = "beta{2,2}";

/// Parse a declared prior name — `"beta{a,b}"` integer params.
/// Returns `None` for any other spelling (the caller maps `"pooled"`/
/// hierarchical names to `PooledSubstitute`, the rest to `UnknownMethod`).
pub fn parse_prior_name(name: &str) -> Option<(u64, u64)> {
    let inner = name.strip_prefix("beta{")?.strip_suffix('}')?;
    let (a, b) = inner.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// The declared method inputs (records-in — the caller reads the spec;
/// this module never sees filters/JSON spec members).
#[derive(Debug, Clone)]
pub struct ContrastDecl {
    /// The named population the cells read.
    pub population: PopulationKind,
    /// The test arm (B — the `test_minus_control` direction's minuend).
    pub test_arm: String,
    /// The control arm (A — the subtrahend).
    pub control_arm: String,
    /// The declared contamination stratum; `None` requires the
    /// candidate slots to already span exactly one stratum (strata are
    /// never silently pooled).
    pub stratum: Option<String>,
    /// The declared prior pair (from the method declaration).
    pub prior: (u64, u64),
    /// The declared prior name as written (for the metadata record).
    pub prior_name: String,
    /// The declared dependence model — `None`/`"independent"` is the
    /// only admitted reading; anything else is `UnsupportedDependence`.
    pub dependence: Option<String>,
    /// The declared target — `None`/`"finite_manifest"` only; a pooled
    /// or hierarchical target is `PooledSubstitute`, anything else
    /// `UnknownMethod`.
    pub target: Option<String>,
    /// The declared downstream use — a TOST/equivalence/confidence/
    /// concentration use of a credible interval is `CredibleAsTost`.
    pub use_as: Option<String>,
    /// The declared bound class of the measure — the binary unit rate
    /// only (the R5.15 `BoundsDecl` contract, same refusal vocabulary).
    pub bounds: BoundsDecl,
    /// The declared interval level — must equal [`LEVEL_PPM`]; a
    /// different declared level is a different method, never a silent
    /// substitution (`UnsupportedLevel`); out of `(0, PPM)` is
    /// `NonFiniteInput`.
    pub level_ppm: i64,
    /// The declared seed (mixed into the canonical-input seed material).
    pub seed: u64,
}

/// The closed refusal set — typed facts, never warnings (the D1/D2
/// refusal vocabulary; a refusal is a member, never an omission).
#[derive(Debug, Clone, PartialEq)]
pub enum ContrastError {
    /// The population contract refused (missing keys, unpaired task,
    /// empty universe, non-unit/unbounded/censored bound — the same
    /// [`PopulationError`] vocabulary, never a second rule).
    Population(PopulationError),
    /// Fewer than two tasks — the equal-task contrast needs `T ≥ 2`.
    OneTask,
    /// A `(task, arm)` cell is below the `R ≥ 5` first-slot floor.
    ShortCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Resolved trials in the cell.
        n: u64,
    },
    /// A `(task, arm)` cell carries unresolved members (not-run,
    /// pending, vetoed-out or unknown) — an inadequate cell keeps its
    /// descriptive bound elsewhere; it cannot carry this inference.
    UnresolvedCell {
        /// The task.
        task_id: String,
        /// The arm.
        arm_id: String,
        /// Unresolved members in the cell.
        u: u64,
    },
    /// A dependence model other than independent cells was declared —
    /// task pairing is stratification, never correlated-replicate
    /// modelling (unsupported dependence refuses, it is never assumed).
    UnsupportedDependence {
        /// The declared dependence.
        declared: String,
    },
    /// A pooled/hierarchical/task-population substitute for the
    /// finite-manifest target was declared — rejected (D.10).
    PooledSubstitute {
        /// The declared substitute.
        declared: String,
    },
    /// A method name outside the admitted set was declared.
    UnknownMethod {
        /// The declared method.
        declared: String,
    },
    /// A prior outside the admitted `{beta{1,1}, beta{2,2}}` set.
    UnsupportedPrior {
        /// The declared prior.
        declared: String,
    },
    /// A declared interval level other than the frozen 95%.
    UnsupportedLevel {
        /// The declared level (ppm).
        declared: i64,
    },
    /// A declared numeric/bound input outside its finite range (level
    /// outside `(0, PPM)`, malformed bound declaration, negative or
    /// over-counted cell). Integer ppm carries no floats — a
    /// non-finite *declaration* is what refuses here.
    NonFiniteInput {
        /// The detail.
        detail: String,
    },
    /// The credible interval was declared as a TOST/equivalence/
    /// confidence/concentration certificate — S.6's label separation
    /// is never crossed.
    CredibleAsTost {
        /// The declared use.
        declared: String,
    },
    /// The method was declared on an analysis `kind` that does not
    /// compute it — a metadata-only declaration fails (S.5/§9.2).
    MetadataOnly {
        /// The kind the declaration rode on.
        kind: String,
    },
    /// The candidate slots span more than one contamination stratum
    /// and no `stratum` was declared — strata are never silently
    /// pooled into one contrast.
    StrataSelectionRequired {
        /// The strata the candidates carry.
        strata: Vec<String>,
    },
}

impl std::fmt::Display for ContrastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ContrastError as E;
        match self {
            E::Population(e) => write!(f, "{e}"),
            E::OneTask => write!(f, "one task — the equal-task contrast requires T >= 2"),
            E::ShortCell { task_id, arm_id, n } => write!(
                f,
                "insufficient_replicates: cell ({task_id}, {arm_id}) has n={n} < {CELL_REPLICATE_FLOOR}"
            ),
            E::UnresolvedCell { task_id, arm_id, u } => write!(
                f,
                "estimator_unavailable: cell ({task_id}, {arm_id}) carries {u} unresolved member(s)"
            ),
            E::UnsupportedDependence { declared } => {
                write!(f, "unsupported dependence declared: {declared}")
            }
            E::PooledSubstitute { declared } => write!(
                f,
                "pooled substitute refused — the finite-manifest target admits no pooled/hierarchical model: {declared}"
            ),
            E::UnknownMethod { declared } => write!(f, "unknown method declared: {declared}"),
            E::UnsupportedPrior { declared } => write!(
                f,
                "unsupported prior: {declared} — admitted set is beta{{1,1}} (registered) / beta{{2,2}} (predeclared exploratory)"
            ),
            E::UnsupportedLevel { declared } => write!(
                f,
                "unsupported level {declared} — the method's credible interval is frozen at {LEVEL_PPM} ppm"
            ),
            E::NonFiniteInput { detail } => write!(f, "nonfinite input: {detail}"),
            E::CredibleAsTost { declared } => write!(
                f,
                "credible-as-TOST refused — a credible interval is never a TOST/equivalence/confidence certificate: {declared}"
            ),
            E::MetadataOnly { kind } => write!(
                f,
                "metadata-only method switch refused — finite_manifest_contrast declared on kind {kind} computes nothing"
            ),
            E::StrataSelectionRequired { strata } => write!(
                f,
                "candidate slots span strata {strata:?} — declare filters.stratum (strata are never silently pooled)"
            ),
        }
    }
}

impl std::error::Error for ContrastError {}

impl From<PopulationError> for ContrastError {
    fn from(e: PopulationError) -> ContrastError {
        ContrastError::Population(e)
    }
}

/// One task's resolved pair of arm cells.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastCell {
    /// The task.
    pub task_id: String,
    /// Test-arm successes `c_Bt`.
    pub test_c: u64,
    /// Test-arm trials `n_Bt`.
    pub test_n: u64,
    /// Control-arm successes `c_At`.
    pub control_c: u64,
    /// Control-arm trials `n_At`.
    pub control_n: u64,
}

/// One doubling-ladder entry — `draws` is the level's draw count and
/// `max_delta_ppm` the max absolute endpoint movement versus the
/// previous level (the base level carries `0`).
#[derive(Debug, Clone, PartialEq)]
pub struct DoublingStep {
    /// The level's draw count.
    pub draws: u64,
    /// `max(|lo_i − lo_{i−1}|, |hi_i − hi_{i−1}|)` in ppm.
    pub max_delta_ppm: i64,
}

/// The estimator's result — every member a fact on the report.
#[derive(Debug, Clone, PartialEq)]
pub struct FiniteManifestContrast {
    /// `"test_minus_control"` — the declared direction (never implied).
    pub direction: String,
    /// The test arm.
    pub test_arm: String,
    /// The control arm.
    pub control_arm: String,
    /// The named population the cells read.
    pub population: String,
    /// The declared stratum selection (`None` = the single stratum the
    /// universe already spanned).
    pub stratum: Option<String>,
    /// The strata the selected cells carry.
    pub stratum_membership: Vec<String>,
    /// The admitted prior pair.
    pub prior: (u64, u64),
    /// The declared prior name (metadata record).
    pub prior_name: String,
    /// `true` when the prior is the predeclared exploratory sensitivity
    /// (`beta{2,2}`) rather than the registered `beta{1,1}`.
    pub sensitivity: bool,
    /// The resolved per-task cells (`{s, n}` per arm, `u == 0`).
    pub cells: Vec<ContrastCell>,
    /// The observed equal-task contrast `Σ w_t (s_B/n_B − s_A/n_A)`.
    pub observed_contrast_ppm: i64,
    /// The analytic posterior mean `Σ w_t (E[p_Bt] − E[p_At])` — exact,
    /// never draw-estimated.
    pub posterior_mean_ppm: i64,
    /// The labelled 95% equal-tail **credible** interval at
    /// [`BASE_DRAWS`] posterior draws.
    pub interval: Interval,
    /// Declared draws (the frozen base).
    pub draws_requested: u64,
    /// Actual draws consumed (the full frozen ladder through the cap).
    pub draws_used: u64,
    /// The doubling record — one entry per ladder level.
    pub doubling: Vec<DoublingStep>,
    /// The max endpoint delta over the doubling ladder (S.3 — a
    /// recorded fact, never a threshold verdict).
    pub numerical_error_ppm: i64,
    /// The declared tolerance (the inverse-CDF ppm-grid residual).
    pub tol_ppm: i64,
    /// The declared seed (mixed into the canonical-input material).
    pub seed: u64,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

impl FiniteManifestContrast {
    /// The canonical JSON member — the Packet D.3 payload verbatim
    /// (`{stratum, cells, weights_ref, prior, draws{requested, used,
    /// doubling, tol_ppm}, seed, posterior_mean_ppm,
    /// interval{lo,hi,kind}, observed_contrast_ppm}`) plus the S.1
    /// metadata record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(CONTRAST_VERSION)),
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
                        .map(|c| {
                            Json::obj([
                                ("task_id", Json::str(&c.task_id)),
                                (
                                    "test",
                                    Json::obj([
                                        ("c", Json::Int(c.test_c as i64)),
                                        ("n", Json::Int(c.test_n as i64)),
                                    ]),
                                ),
                                (
                                    "control",
                                    Json::obj([
                                        ("c", Json::Int(c.control_c as i64)),
                                        ("n", Json::Int(c.control_n as i64)),
                                    ]),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("weights_ref", Json::str("equal_task:w_t=1/T")),
            (
                "prior",
                Json::obj([
                    (
                        "beta",
                        Json::obj([
                            ("a", Json::Int(self.prior.0 as i64)),
                            ("b", Json::Int(self.prior.1 as i64)),
                        ]),
                    ),
                    (
                        "label",
                        Json::str(if self.sensitivity {
                            "predeclared_exploratory"
                        } else {
                            "registered"
                        }),
                    ),
                ]),
            ),
            (
                "draws",
                Json::obj([
                    ("requested", Json::Int(self.draws_requested as i64)),
                    ("used", Json::Int(self.draws_used as i64)),
                    (
                        "doubling",
                        Json::Arr(
                            self.doubling
                                .iter()
                                .map(|d| {
                                    Json::obj([
                                        ("draws", Json::Int(d.draws as i64)),
                                        ("max_delta_ppm", Json::Int(d.max_delta_ppm)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    ("tol_ppm", Json::Int(self.tol_ppm)),
                ]),
            ),
            ("numerical_error_ppm", Json::Int(self.numerical_error_ppm)),
            ("seed", Json::Int(self.seed as i64)),
            ("posterior_mean_ppm", Json::Int(self.posterior_mean_ppm)),
            (
                "observed_contrast_ppm",
                Json::Int(self.observed_contrast_ppm),
            ),
            (
                "interval",
                Json::obj([
                    ("lo_ppm", Json::Int(self.interval.lo)),
                    ("hi_ppm", Json::Int(self.interval.hi)),
                    ("kind", Json::str("credible")),
                    ("level_ppm", Json::Int(self.level_ppm())),
                ]),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }

    fn level_ppm(&self) -> i64 {
        self.metadata.interval.1.unwrap_or(LEVEL_PPM)
    }
}

/// Round `a / b` to the nearest integer, ties away from zero (the
/// `stats::mean` rounding rule — one rounding rule, deterministic).
fn round_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    ((if a >= 0 { a + b / 2 } else { a - b / 2 }) / b) as i64
}

/// The integer-ppm inverse CDF: the smallest `p_ppm ∈ [0, PPM]` with
/// `I_{p/PPM}(a,b) ≥ u_ppm/PPM` — bisection on the ppm grid (residual
/// ≤ [`INVCDF_TOL_PPM`], deterministic; `u_ppm ∈ [1, PPM−1]` interior
/// by construction so `0 < p < 1` always).
fn beta_inv_ppm(u_ppm: i64, a: u64, b: u64) -> i64 {
    let (mut lo, mut hi) = (0i64, PPM);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let c = regularized_beta(mid as f64 / PPM as f64, a as f64, b as f64);
        if c * PPM as f64 >= u_ppm as f64 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// One arm's accumulated cell counts under the named population.
#[derive(Default)]
struct ArmCell {
    m: u64,
    s: u64,
    u: u64,
}

/// `finite_manifest_contrast(pop, decl)` — the admitted estimator over
/// the reconciled planned population. `pop` is the R5.15
/// `PlannedPopulation` (consumed unchanged); `decl` carries the
/// declared method inputs. Every admitted-input validation is a typed
/// [`ContrastError`].
pub fn finite_manifest_contrast(
    pop: &PlannedPopulation,
    decl: &ContrastDecl,
) -> Result<FiniteManifestContrast, ContrastError> {
    // ── declared-input validation + cell derivation (shared core) ─
    let core = resolve_core(pop, decl)?;

    // ── posterior draws — the frozen ladder through the cap ───────────
    let mut rng = XorShift64::seeded(&core.seed_material);
    let deltas = draw_deltas(&core.cells, decl.prior, &mut rng, CAP_DRAWS);

    // ── equal-tail endpoints per ladder level + the doubling record ───
    let mut levels = Vec::new();
    let mut d = BASE_DRAWS;
    while d <= CAP_DRAWS {
        levels.push(d);
        d = d.saturating_mul(2).min(CAP_DRAWS + 1);
    }
    let mut endpoints: Vec<Interval> = Vec::with_capacity(levels.len());
    for &n in &levels {
        let draws = &deltas[..n as usize];
        endpoints.push(Interval {
            lo: quantile(draws, TAIL_PPM).unwrap_or(0),
            hi: quantile(draws, PPM - TAIL_PPM).unwrap_or(0),
        });
    }
    let mut doubling: Vec<DoublingStep> = Vec::with_capacity(levels.len());
    let mut numerical_error_ppm = 0i64;
    for (i, &n) in levels.iter().enumerate() {
        let delta = if i == 0 {
            0
        } else {
            (endpoints[i].lo - endpoints[i - 1].lo)
                .abs()
                .max((endpoints[i].hi - endpoints[i - 1].hi).abs())
        };
        numerical_error_ppm = numerical_error_ppm.max(delta);
        doubling.push(DoublingStep {
            draws: n,
            max_delta_ppm: delta,
        });
    }

    let mut keys = BTreeMap::new();
    keys.insert("population".into(), decl.population.as_str().to_string());
    keys.insert("test_arm".into(), decl.test_arm.clone());
    keys.insert("control_arm".into(), decl.control_arm.clone());
    if let Some(st) = &decl.stratum {
        keys.insert("stratum".into(), st.clone());
    }
    Ok(FiniteManifestContrast {
        direction: "test_minus_control".to_string(),
        test_arm: decl.test_arm.clone(),
        control_arm: decl.control_arm.clone(),
        population: decl.population.as_str().to_string(),
        stratum: decl.stratum.clone(),
        stratum_membership: core.stratum_membership.clone(),
        prior: decl.prior,
        prior_name: decl.prior_name.clone(),
        sensitivity: decl.prior != REGISTERED_PRIOR,
        cells: core.cells,
        observed_contrast_ppm: core.observed_contrast_ppm,
        posterior_mean_ppm: core.posterior_mean_ppm,
        interval: endpoints[0], // the declared BASE_DRAWS estimate
        draws_requested: BASE_DRAWS,
        draws_used: CAP_DRAWS,
        doubling,
        numerical_error_ppm,
        tol_ppm: INVCDF_TOL_PPM,
        seed: decl.seed,
        metadata: MethodMetadata {
            estimator: CONTRAST_KIND.to_string(),
            version: CONTRAST_VERSION.to_string(),
            target: "finite_manifest_equal_task_contrast:test_minus_control".to_string(),
            assumptions: vec![
                "independent Bernoulli runs within cells; independent posteriors across task/arm cells".into(),
                "task pairing is stratification, not correlated-replicate modelling".into(),
                "equal within-stratum task weights w_t=1/T; first-slot coverage".into(),
                "finite-manifest target only — no population, original-runner or catalogue extrapolation".into(),
            ],
            prior: Some(decl.prior_name.clone()),
            keys,
            stratum_membership: core.stratum_membership,
            seed: decl.seed,
            draws_requested: BASE_DRAWS,
            draws_used: CAP_DRAWS,
            floors: vec![
                format!("cell_replicates>={CELL_REPLICATE_FLOOR}"),
                "tasks>=2".to_string(),
                format!("posterior_draws_base={BASE_DRAWS}"),
                format!("doubling_cap={CAP_DRAWS}"),
            ],
            interval: ("credible".to_string(), Some(LEVEL_PPM)),
            refusal: None,
        },
    })
}

/// The shared validated core — declaration validation, cell derivation
/// (the population contract's one counting rule), the observed/mean
/// contrasts and the canonical seed material. `finite_manifest_contrast`
/// and [`finite_manifest_headline`] share it: the headline is the
/// estimator's **reported** interval by construction (same seed stream,
/// same first [`BASE_DRAWS`] draws, same quantile — never a second
/// implementation; the 16k–64k doubling ladder is the report's own
/// numerical-error instrumentation, not part of the interval).
struct ContrastCore {
    cells: Vec<ContrastCell>,
    stratum_membership: Vec<String>,
    observed_contrast_ppm: i64,
    posterior_mean_ppm: i64,
    seed_material: String,
}

/// `resolve_core` — the front half of the estimator: every declared-input
/// validation (unchanged order, unchanged refusals), cell derivation and
/// the analytic/observed contrasts. Never draws.
fn resolve_core(
    pop: &PlannedPopulation,
    decl: &ContrastDecl,
) -> Result<ContrastCore, ContrastError> {
    // ── declared-input validation (all before any draw) ──────────────
    if !(0..PPM).contains(&decl.level_ppm) {
        return Err(ContrastError::NonFiniteInput {
            detail: format!("level_ppm {} outside (0, {PPM})", decl.level_ppm),
        });
    }
    if decl.level_ppm != LEVEL_PPM {
        return Err(ContrastError::UnsupportedLevel {
            declared: decl.level_ppm,
        });
    }
    match &decl.bounds {
        BoundsDecl::Unit => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } if *lo_ppm == 0 && *hi_ppm == PPM => {}
        BoundsDecl::Bounded { lo_ppm, hi_ppm } => {
            return Err(PopulationError::UnsupportedBounds {
                detail: format!(
                "declared bound [{lo_ppm},{hi_ppm}] is not the unit interval — a different target"
            ),
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
    if let Some(t) = &decl.target {
        match t.as_str() {
            "finite_manifest" => {}
            "pooled" | "hierarchical" | "task_population" | "population" => {
                return Err(ContrastError::PooledSubstitute {
                    declared: t.clone(),
                })
            }
            other => {
                return Err(ContrastError::UnknownMethod {
                    declared: other.to_string(),
                })
            }
        }
    }
    if let Some(d) = &decl.dependence {
        if d != "independent" {
            return Err(ContrastError::UnsupportedDependence {
                declared: d.clone(),
            });
        }
    }
    if let Some(u) = &decl.use_as {
        if matches!(
            u.as_str(),
            "tost"
                | "tost_equivalence"
                | "equivalence"
                | "noninferiority"
                | "non_inferiority"
                | "confidence"
                | "concentration_bound"
        ) {
            return Err(ContrastError::CredibleAsTost {
                declared: u.clone(),
            });
        }
    }
    if decl.prior != REGISTERED_PRIOR && decl.prior != SENSITIVITY_PRIOR {
        return Err(ContrastError::UnsupportedPrior {
            declared: decl.prior_name.clone(),
        });
    }
    if decl.test_arm.is_empty() || decl.control_arm.is_empty() {
        return Err(ContrastError::Population(PopulationError::MissingKey {
            plan_ref: "(contrast)".into(),
            member: "arm_id",
        }));
    }
    if decl.test_arm == decl.control_arm {
        return Err(ContrastError::NonFiniteInput {
            detail: format!(
                "test_arm == control_arm ({}) — a self-contrast is not a valid target",
                decl.test_arm
            ),
        });
    }

    let strata: BTreeSet<String> = pop
        .slots
        .iter()
        .filter(|s| s.arm_id == decl.test_arm || s.arm_id == decl.control_arm)
        .map(|s| s.stratum.clone())
        .collect();
    if decl.stratum.is_none() && strata.len() > 1 {
        return Err(ContrastError::StrataSelectionRequired {
            strata: strata.into_iter().collect(),
        });
    }
    let mut tasks: BTreeMap<String, [Option<ArmCell>; 2]> = BTreeMap::new();
    let mut selected_strata: BTreeSet<String> = BTreeSet::new();
    for s in &pop.slots {
        if s.arm_id != decl.test_arm && s.arm_id != decl.control_arm {
            continue;
        }
        if let Some(st) = &decl.stratum {
            if &s.stratum != st {
                continue;
            }
        }
        selected_strata.insert(s.stratum.clone());
        let entry = tasks.entry(s.task_id.clone()).or_insert([None, None]);
        let idx = if s.arm_id == decl.test_arm { 0 } else { 1 };
        let cell = entry[idx].get_or_insert_with(ArmCell::default);
        cell.m += 1;
        let (ds, _df, du) = contrib(decl.population, s);
        cell.s += ds;
        cell.u += du;
    }
    if tasks.is_empty() {
        return Err(ContrastError::Population(PopulationError::EmptyUniverse));
    }
    if tasks.len() < 2 {
        return Err(ContrastError::OneTask);
    }
    let mut cells: Vec<ContrastCell> = Vec::with_capacity(tasks.len());
    for (task_id, [te, co]) in tasks {
        let te = te.ok_or_else(|| {
            ContrastError::Population(PopulationError::UnpairedTask {
                task_id: task_id.clone(),
                arm_id: decl.test_arm.clone(),
            })
        })?;
        let co = co.ok_or_else(|| {
            ContrastError::Population(PopulationError::UnpairedTask {
                task_id: task_id.clone(),
                arm_id: decl.control_arm.clone(),
            })
        })?;
        if te.u > 0 {
            return Err(ContrastError::UnresolvedCell {
                task_id: task_id.clone(),
                arm_id: decl.test_arm.clone(),
                u: te.u,
            });
        }
        if co.u > 0 {
            return Err(ContrastError::UnresolvedCell {
                task_id: task_id.clone(),
                arm_id: decl.control_arm.clone(),
                u: co.u,
            });
        }
        // With u == 0 every slot resolved: n = s + f = m.
        let (tn, cn) = (te.m, co.m);
        if tn < CELL_REPLICATE_FLOOR {
            return Err(ContrastError::ShortCell {
                task_id: task_id.clone(),
                arm_id: decl.test_arm.clone(),
                n: tn,
            });
        }
        if cn < CELL_REPLICATE_FLOOR {
            return Err(ContrastError::ShortCell {
                task_id: task_id.clone(),
                arm_id: decl.control_arm.clone(),
                n: cn,
            });
        }
        cells.push(ContrastCell {
            task_id,
            test_c: te.s,
            test_n: tn,
            control_c: co.s,
            control_n: cn,
        });
    }
    let t_n = cells.len() as i128;

    // ── observed equal-task contrast + analytic posterior mean ────────
    // Per-task deltas are one-shot rational→ppm rounds (never a
    // per-cell pre-round — the single rounding rule applies once).
    let mut obs_sum: i128 = 0;
    let mut pm_sum: i128 = 0;
    for c in &cells {
        let (tc, tn, cc, cn) = (
            i128::from(c.test_c),
            i128::from(c.test_n),
            i128::from(c.control_c),
            i128::from(c.control_n),
        );
        obs_sum += i128::from(round_div((tc * cn - cc * tn) * i128::from(PPM), tn * cn));
        // Posterior mean under the DECLARED prior (a,b): E[p]=(c+a)/(n+a+b)
        // — the DF-R5.17-1 repair; beta{1,1} reduces to (c+1)/(n+2)
        // byte-identically.
        let (pa, pab) = (
            i128::from(decl.prior.0),
            i128::from(decl.prior.0 + decl.prior.1),
        );
        pm_sum += i128::from(round_div(
            ((tc + pa) * (cn + pab) - (cc + pa) * (tn + pab)) * i128::from(PPM),
            (tn + pab) * (cn + pab),
        ));
    }
    let observed_contrast_ppm = round_div(obs_sum, t_n);
    let posterior_mean_ppm = round_div(pm_sum, t_n);

    // ── canonical seed material (declared inputs + resolved cells) ───
    let stratum_membership: Vec<String> = selected_strata.into_iter().collect();
    let seed_material = Json::obj([
        ("estimator", Json::str(CONTRAST_KIND)),
        ("version", Json::str(CONTRAST_VERSION)),
        ("population", Json::str(decl.population.as_str())),
        ("test_arm", Json::str(&decl.test_arm)),
        ("control_arm", Json::str(&decl.control_arm)),
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
                cells
                    .iter()
                    .map(|c| {
                        Json::Arr(vec![
                            Json::str(&c.task_id),
                            Json::Int(c.test_c as i64),
                            Json::Int(c.test_n as i64),
                            Json::Int(c.control_c as i64),
                            Json::Int(c.control_n as i64),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
    .to_canonical_string();
    Ok(ContrastCore {
        cells,
        stratum_membership,
        observed_contrast_ppm,
        posterior_mean_ppm,
        seed_material,
    })
}

/// `draw_deltas` — `n` posterior contrast draws under the declared prior
/// `(a,b)`: per cell `p ~ Beta(c+a, n−c+a+b)` via the seeded inverse-CDF
/// (interior uniforms — the CDF endpoints are never drawn), equal-task
/// mean `Σ w_t (p_Bt − p_At)`, `w_t = 1/T`.
fn draw_deltas(
    cells: &[ContrastCell],
    prior: (u64, u64),
    rng: &mut XorShift64,
    n: u64,
) -> Vec<i64> {
    let (a, b) = prior;
    let t_n = cells.len() as i128;
    let mut deltas: Vec<i64> = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let mut sum: i128 = 0;
        for c in cells {
            let u_b = 1 + rng.below((PPM - 1) as usize) as i64;
            let p_b = beta_inv_ppm(u_b, c.test_c + a, c.test_n - c.test_c + b);
            let u_a = 1 + rng.below((PPM - 1) as usize) as i64;
            let p_a = beta_inv_ppm(u_a, c.control_c + a, c.control_n - c.control_c + b);
            sum += i128::from(p_b) - i128::from(p_a);
        }
        deltas.push(round_div(sum, t_n));
    }
    deltas
}

/// The estimator's reported headline — identical validation, cells,
/// observed contrast, analytic posterior mean and the declared
/// [`BASE_DRAWS`] equal-tail **credible** interval as
/// [`finite_manifest_contrast`] produces them (same seed stream, same
/// first `BASE_DRAWS` draws, same quantile — verified bit-identical by
/// the R5.40 battery; the 16k–64k doubling ladder stays the report's
/// own numerical-error instrumentation). Used by the offline
/// `sim_battery` validation apparatus so the ≥2,000-datasets/scenario
/// execution stays inside the D2 numerical envelope; never a second
/// implementation and never a report path.
#[derive(Debug, Clone, PartialEq)]
pub struct Headline {
    /// The resolved per-task cells.
    pub cells: Vec<ContrastCell>,
    /// The observed equal-task contrast.
    pub observed_contrast_ppm: i64,
    /// The analytic posterior mean under the declared prior.
    pub posterior_mean_ppm: i64,
    /// The 95% equal-tail credible interval at [`BASE_DRAWS`].
    pub interval: Interval,
}

/// `finite_manifest_headline(pop, decl)` — the estimator's reported
/// interval/mean/observed without the doubling-ladder instrumentation.
pub fn finite_manifest_headline(
    pop: &PlannedPopulation,
    decl: &ContrastDecl,
) -> Result<Headline, ContrastError> {
    let core = resolve_core(pop, decl)?;
    let mut rng = XorShift64::seeded(&core.seed_material);
    let deltas = draw_deltas(&core.cells, decl.prior, &mut rng, BASE_DRAWS);
    Ok(Headline {
        cells: core.cells,
        observed_contrast_ppm: core.observed_contrast_ppm,
        posterior_mean_ppm: core.posterior_mean_ppm,
        interval: Interval {
            lo: quantile(&deltas, TAIL_PPM).unwrap_or(0),
            hi: quantile(&deltas, PPM - TAIL_PPM).unwrap_or(0),
        },
    })
}
