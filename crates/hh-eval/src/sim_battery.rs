//! `sim_battery` — the frozen numerical-validation battery (R5.40; M-G /
//! REQ-D-109 primary; dossier S.4 instantiated by ADR-0418).
//!
//! The battery executes the **exact** owner pipeline over synthesized
//! datasets — `generator` → `population::assemble` (the real R5.15
//! reconciliation) → `finite_manifest_headline` (the estimator's own
//! reported interval, bit-identical to `finite_manifest_contrast`'s
//! headline by construction and by test) — at **≥2,000 datasets per
//! scenario cell**, and records availability, the refusal census,
//! diagnostic coverage (labelled diagnostic, never a nominal
//! frequentist guarantee — S.4/M-G), width, observed/posterior error,
//! the sharp-null sign-flip verdict leg (`relevant` type-I on the null
//! cells, detection probability elsewhere) and binomial Monte Carlo
//! error. Every quantity is `i64` ppm (`Json::Int`-only — S.2).
//!
//! The protocol is frozen *before execution*: the scenario table,
//! mixture/mechanism parameters, the seed rule
//! `H(canonical(scenario ∥ battery_version ∥ replicate))` and the
//! recorded quantities live here and in ADR-0418 — never post-hoc,
//! never a favorable-method fallback. A scenario that does not run to
//! its declared count (`Stopped`), fails to generate (`Failed`), or
//! whose expectation is unmet can never count as method admission;
//! partial aggregates are retained, not discarded (AC-2).
//!
//! Offline only — the D2 numerical tranche envelope (≤8 h wall, 4
//! cores/8 GiB, 32 core-hours, 2 GiB) bounds the execution, never a
//! pooled build allowance (§9.2).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

use crate::finite_manifest::{
    finite_manifest_headline, ContrastDecl, ContrastError, Headline, REGISTERED_PRIOR,
    REGISTERED_PRIOR_NAME, SENSITIVITY_PRIOR, SENSITIVITY_PRIOR_NAME,
};
use crate::population::{
    assemble, Availability, BoundsDecl, PlannedSlot, PopulationInput, SlotSubject,
};
use crate::stats::{isqrt, permutation_signflip_p, XorShift64, PPM};

/// The battery contract version (record `schema`).
pub const BATTERY_VERSION: &str = "sim-battery/1";
/// The frozen per-scenario dataset floor (S.4 — ≥2,000).
pub const DATASETS_PER_SCENARIO: u64 = 2_000;
/// The frozen sharp-null level — α = .05 (the D2 Holm level's per-test
/// reading; the battery reports a probability, never a corrected claim).
pub const ALPHA_PPM: i64 = 50_000;
/// The frozen Monte Carlo draw count for the sign-flip leg above the
/// exact-enumeration ceiling (T > 20 → 8,000 seeded draws, S.3).
pub const SIGNFLIP_MC_DRAWS: u64 = 8_000;
/// The frozen task-difficulty mixtures (S.4 "easy/hard mixtures,
/// heterogeneous effects").
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mixture {
    /// `p_A,t ~ Uniform{0.70…0.90}` per task.
    Easy,
    /// `p_A,t ~ Uniform{0.10…0.30}` per task.
    Hard,
    /// `p_A,t ~ Uniform{0.30…0.70}` with a per-task effect
    /// `δ_t ~ Uniform{−.10…+.10}` — realized `Δ` varies dataset to dataset.
    Heterogeneous,
}

/// The frozen validity mechanisms (S.4 — each exercised at the declared
/// `T=20, R=5, effect 0, easy` cell).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mechanism {
    /// Independent Bernoulli members — the design assumption.
    Clean,
    /// Nothing runs — every member unresolved; the estimator must
    /// refuse (`UnresolvedCell`), never compute.
    ZeroActivation,
    /// Missingness ∝ failure (unresolved failures) plus observed vetoes
    /// on a fraction of passes — both leave `canonical` unresolved;
    /// missingness can never improve a denominator.
    InformativeMissingnessVeto,
    /// One shared uniform decides a whole cell's outcomes — perfect
    /// within-cell dependence the design *assumes away*; the recorded
    /// distortion is a fact, never a pass criterion.
    SharedShock,
}

/// One frozen scenario cell.
#[derive(Debug, Clone)]
pub struct Scenario {
    /// The cell id (stable seed material).
    pub id: String,
    /// Tasks `T` (finite manifest size).
    pub tasks: u64,
    /// Replicates `R` per `(task, arm)` cell.
    pub reps: u64,
    /// The declared per-task effect `B − A` (ppm) — the heterogeneous
    /// mixture ignores this and draws `δ_t` per task.
    pub effect_ppm: i64,
    /// The task-difficulty mixture.
    pub mixture: Mixture,
    /// The validity mechanism.
    pub mechanism: Mechanism,
    /// `true` when the diagnostic `beta{2,2}` arm runs beside the
    /// registered prior on the same datasets (S.4 prior sensitivity).
    pub prior_sensitivity: bool,
    /// The declared dataset floor — always ≥ [`DATASETS_PER_SCENARIO`].
    pub datasets: u64,
}

/// The frozen scenario table — every S.4 factor covered: T ∈ {10,20,30}
/// × R ∈ {5,10} × effect ∈ {0,±.05,±.10} on the `easy` grid (30 cells);
/// the `hard` mixture at T=20/R=10 over all five effects (5 cells);
/// `heterogeneous` at T=20 × R∈{5,10} × mean effect {0,+.05} (4 cells);
/// the three mechanism legs at T=20/R=5/effect 0/easy; the `beta{2,2}`
/// diagnostic arms on the T∈{10,30}/R=5/effect {0,+.05} easy cells.
pub fn scenarios() -> Vec<Scenario> {
    let mut v = Vec::new();
    let eff = [-100_000, -50_000, 0, 50_000, 100_000];
    for t in [10u64, 20, 30] {
        for r in [5u64, 10] {
            for e in eff {
                let sens = r == 5 && t != 20 && (e == 0 || e == 50_000);
                v.push(Scenario {
                    id: format!("t{t}-r{r}-e{}-easy", signed(e)),
                    tasks: t,
                    reps: r,
                    effect_ppm: e,
                    mixture: Mixture::Easy,
                    mechanism: Mechanism::Clean,
                    prior_sensitivity: sens,
                    datasets: DATASETS_PER_SCENARIO,
                });
            }
        }
    }
    for e in eff {
        v.push(Scenario {
            id: format!("t20-r10-e{}-hard", signed(e)),
            tasks: 20,
            reps: 10,
            effect_ppm: e,
            mixture: Mixture::Hard,
            mechanism: Mechanism::Clean,
            prior_sensitivity: false,
            datasets: DATASETS_PER_SCENARIO,
        });
    }
    for r in [5u64, 10] {
        for e in [0i64, 50_000] {
            v.push(Scenario {
                id: format!("t20-r{r}-het{}", signed(e)),
                tasks: 20,
                reps: r,
                effect_ppm: e,
                mixture: Mixture::Heterogeneous,
                mechanism: Mechanism::Clean,
                prior_sensitivity: false,
                datasets: DATASETS_PER_SCENARIO,
            });
        }
    }
    for (name, m) in [
        ("zero-activation", Mechanism::ZeroActivation),
        (
            "informative-missingness-veto",
            Mechanism::InformativeMissingnessVeto,
        ),
        ("shared-shock", Mechanism::SharedShock),
    ] {
        v.push(Scenario {
            id: format!("mech/{name}"),
            tasks: 20,
            reps: 5,
            effect_ppm: 0,
            mixture: Mixture::Easy,
            mechanism: m,
            prior_sensitivity: false,
            datasets: DATASETS_PER_SCENARIO,
        });
    }
    v
}

fn signed(ppm: i64) -> String {
    format!(
        "{}{:02}",
        if ppm < 0 { "-" } else { "+" },
        ppm.abs() / 10_000
    )
}

/// The frozen dataset seed rule (S.4 proposed, ADR-0418 pinned):
/// `H(canonical(scenario ∥ battery_version ∥ replicate))` — one
/// derivable stream per dataset; never post-hoc seed selection.
pub fn dataset_seed_material(scenario_id: &str, replicate: u64) -> String {
    Json::obj([
        ("scenario", Json::str(scenario_id)),
        ("battery", Json::str(BATTERY_VERSION)),
        ("replicate", Json::Int(replicate as i64)),
    ])
    .to_canonical_string()
}

/// One synthesized dataset — the planned-slot universe plus the true
/// realized equal-task contrast the generator committed to.
#[derive(Debug)]
pub struct Dataset {
    /// The population assembly input (planned slots + required universe
    /// + read pins — the real reconciliation input shape).
    pub input: PopulationInput,
    /// The realized `Σ_t (p_Bt − p_At)/T` this dataset was drawn under.
    pub true_delta_ppm: i64,
    /// The dataset replicate index (the declared estimator seed).
    pub replicate: u64,
}

/// `dataset(s, replicate)` — one deterministic synthetic dataset under
/// the frozen generator (never asserts an outcome distribution on the
/// estimator; the seed is the scenario cell's own derivable stream).
pub fn dataset(s: &Scenario, replicate: u64) -> Dataset {
    let mut rng = XorShift64::seeded(&dataset_seed_material(&s.id, replicate));
    let mut slots = Vec::new();
    let mut required = BTreeSet::new();
    let mut pins = BTreeMap::new();
    let mut true_sum: i128 = 0;
    for t in 0..s.tasks {
        let (pa, eff) = match s.mixture {
            Mixture::Easy => (700_000 + 50_000 * rng.below(5) as i64, s.effect_ppm),
            Mixture::Hard => (100_000 + 50_000 * rng.below(5) as i64, s.effect_ppm),
            Mixture::Heterogeneous => (
                300_000 + 50_000 * rng.below(9) as i64,
                (rng.below(5) as i64 - 2) * 50_000,
            ),
        };
        let pb = (pa + eff).clamp(10_000, 990_000);
        true_sum += i128::from(pb - pa);
        let shock = rng.below(PPM as usize) as i64; // consumed in order, every mechanism
        for (arm, p) in [("A", pa), ("B", pb)] {
            for rep_i in 0..s.reps {
                let plan_ref = format!("sim-{arm}-{t}-{rep_i}");
                required.insert(plan_ref.clone());
                let subject_run = format!("r-{arm}-{t}-{rep_i}");
                let (outcome, success, veto, avail, seq) = match s.mechanism {
                    Mechanism::Clean => {
                        let ok = (rng.below(PPM as usize) as i64) < p;
                        (
                            Some(OutcomeClass::Scored),
                            Some(ok),
                            vec![],
                            Availability::Resolved,
                            1,
                        )
                    }
                    Mechanism::SharedShock => (
                        Some(OutcomeClass::Scored),
                        Some(shock < p),
                        vec![],
                        Availability::Resolved,
                        1,
                    ),
                    Mechanism::ZeroActivation => (None, None, vec![], Availability::NotRun, 0),
                    Mechanism::InformativeMissingnessVeto => {
                        let ok = (rng.below(PPM as usize) as i64) < p;
                        if !ok && rng.below(2) == 0 {
                            (None, None, vec![], Availability::NotRun, 0)
                        } else if ok && rng.below(10) == 0 {
                            (
                                Some(OutcomeClass::Scored),
                                Some(ok),
                                vec!["v:obs".to_string()],
                                Availability::Resolved,
                                1,
                            )
                        } else {
                            (
                                Some(OutcomeClass::Scored),
                                Some(ok),
                                vec![],
                                Availability::Resolved,
                                1,
                            )
                        }
                    }
                };
                if seq > 0 {
                    pins.insert(subject_run.clone(), seq);
                }
                slots.push(PlannedSlot {
                    plan_ref,
                    task_id: format!("t{t}"),
                    split_label: "held_out".into(),
                    stratum: "sim".into(),
                    arm_id: arm.to_string(),
                    replicate_index: rep_i,
                    subjects: vec![SlotSubject {
                        subject_run,
                        attempt_no: 1,
                        outcome,
                        success,
                        veto_tripped: veto,
                        availability: avail,
                        seq,
                        missing_required_checks: false,
                        admissible: true,
                    }],
                });
            }
        }
    }
    Dataset {
        input: PopulationInput {
            slots,
            required,
            pins,
        },
        true_delta_ppm: (true_sum / i128::from(s.tasks)) as i64,
        replicate,
    }
}

/// The declaration the battery runs — the admitted method at the
/// declared prior and frozen level; the dataset replicate is the
/// declared seed (recorded, never shopped).
fn decl(prior: (u64, u64), prior_name: &str, seed: u64) -> ContrastDecl {
    ContrastDecl {
        population: crate::population::PopulationKind::Canonical,
        test_arm: "B".into(),
        control_arm: "A".into(),
        stratum: None,
        prior,
        prior_name: prior_name.to_string(),
        dependence: None,
        target: None,
        use_as: None,
        bounds: BoundsDecl::Unit,
        level_ppm: 950_000,
        seed,
    }
}

/// One dataset's outcome under the exact pipeline — a fact, never a
/// verdict on the method.
#[derive(Debug)]
pub enum Outcome {
    /// The estimator computed: reported interval, posterior mean,
    /// observed contrast, per-task observed deltas and the sign-flip p.
    Computed {
        /// The shared headline (estimator-identical).
        headline: Box<Headline>,
        /// Sharp-null sign-flip p (ppm) over per-task observed deltas.
        signflip_p_ppm: Option<i64>,
        /// The `beta{2,2}` arm's posterior mean (armed cells only).
        sens_posterior_mean_ppm: Option<i64>,
    },
    /// A typed refusal through the estimator path (its variant name).
    Refused(&'static str),
    /// `population::assemble` failed — a generator failure, retained.
    GeneratorFailure(String),
}

fn refusal_name(e: &ContrastError) -> &'static str {
    use ContrastError as E;
    match e {
        E::Population(_) => "population",
        E::OneTask => "one_task",
        E::ShortCell { .. } => "short_cell",
        E::UnresolvedCell { .. } => "unresolved_cell",
        E::UnsupportedDependence { .. } => "unsupported_dependence",
        E::PooledSubstitute { .. } => "pooled_substitute",
        E::UnknownMethod { .. } => "unknown_method",
        E::UnsupportedPrior { .. } => "unsupported_prior",
        E::UnsupportedLevel { .. } => "unsupported_level",
        E::NonFiniteInput { .. } => "non_finite_input",
        E::CredibleAsTost { .. } => "credible_as_tost",
        E::MetadataOnly { .. } => "metadata_only",
        E::StrataSelectionRequired { .. } => "strata_selection_required",
    }
}

/// `eval_dataset(s, data)` — run one dataset through the exact pipeline
/// (`assemble` → `finite_manifest_headline` → sign-flip leg → armed
/// sensitivity). Refusals and generator failures are outcomes, never
/// silent skips.
pub fn eval_dataset(s: &Scenario, data: &Dataset) -> Outcome {
    let pop = match assemble(PopulationInput {
        slots: data.input.slots.clone(),
        required: data.input.required.clone(),
        pins: data.input.pins.clone(),
    }) {
        Ok(p) => p,
        Err(e) => return Outcome::GeneratorFailure(e.to_string()),
    };
    let d = decl(REGISTERED_PRIOR, REGISTERED_PRIOR_NAME, data.replicate);
    let h = match finite_manifest_headline(&pop, &d) {
        Ok(h) => h,
        Err(e) => return Outcome::Refused(refusal_name(&e)),
    };
    // Sharp-null leg — the family's registered sign-flip over the
    // per-task observed deltas (exact ≤20, 8,000 MC above — S.3).
    let deltas: Vec<i64> = h
        .cells
        .iter()
        .map(|c| {
            ((i128::from(c.test_c) * i128::from(c.control_n)
                - i128::from(c.control_c) * i128::from(c.test_n))
                * i128::from(PPM)
                / (i128::from(c.test_n) * i128::from(c.control_n))) as i64
        })
        .collect();
    let p = permutation_signflip_p(&deltas, SIGNFLIP_MC_DRAWS, &signflip_seed(s, data));
    let sens = if s.prior_sensitivity {
        let sd = decl(SENSITIVITY_PRIOR, SENSITIVITY_PRIOR_NAME, d.seed);
        finite_manifest_headline(&pop, &sd)
            .ok()
            .map(|sh| sh.posterior_mean_ppm)
    } else {
        None
    };
    Outcome::Computed {
        headline: Box::new(h),
        signflip_p_ppm: p,
        sens_posterior_mean_ppm: sens,
    }
}

fn signflip_seed(s: &Scenario, data: &Dataset) -> String {
    format!("signflip|{}", dataset_seed_material(&s.id, data.replicate))
}

/// The execution cap — a wall-clock and dataset bound; a breach yields
/// `Stopped` with the partial aggregate retained (AC-2), never an
/// admission.
#[derive(Debug, Clone)]
pub struct Cap {
    /// Maximum datasets actually run per scenario (the declared floor
    /// when `≥ datasets`).
    pub max_datasets: u64,
    /// Wall-clock deadline for the *whole* battery run.
    pub deadline: Option<Instant>,
}

impl Cap {
    /// The uncapped-by-count cap (wall only).
    pub fn full(deadline: Option<Instant>) -> Cap {
        Cap {
            max_datasets: u64::MAX,
            deadline,
        }
    }
}

/// Per-scenario disposition — the executed fact.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Ran to the declared count.
    Executed,
    /// Budget breach stopped it early — partial evidence retained.
    Stopped,
    /// A generator path failed before any dataset resolved — retained.
    Failed(String),
}

/// The frozen per-scenario record.
#[derive(Debug, Clone)]
pub struct ScenarioReport {
    /// The cell id.
    pub id: String,
    /// Declared datasets (≥2,000) / actually run / computed.
    pub datasets_declared: u64,
    /// Actually run.
    pub datasets_run: u64,
    /// Estimator computed a headline.
    pub computed: u64,
    /// Typed-refusal census (`variant → count`).
    pub refused: BTreeMap<String, u64>,
    /// `assemble` failures (retained, not hidden).
    pub generator_failures: u64,
    /// First generator failure detail (the count is above).
    pub first_failure: Option<String>,
    /// `computed / run` (ppm) — 0 when nothing ran.
    pub availability_ppm: i64,
    /// Fraction of computed intervals covering the realized `Δ`
    /// (**diagnostic** — never a nominal frequentist guarantee).
    pub coverage_ppm: Option<i64>,
    /// Mean / max reported width.
    pub width_mean_ppm: Option<i64>,
    /// Max reported width.
    pub width_max_ppm: Option<i64>,
    /// Mean `|posterior_mean − true Δ|` over computed datasets.
    pub mean_abs_err_ppm: Option<i64>,
    /// Fraction of computed datasets with sign-flip p < α
    /// (the `relevant` type-I reading on null cells — a probability,
    /// never a corrected family claim).
    pub verdict_ppm: Option<i64>,
    /// Armed cells: mean `|posterior_mean(beta{2,2}) − posterior_mean|`.
    pub prior_shift_ppm: Option<i64>,
    /// Max binomial Monte Carlo error over the recorded probabilities.
    pub mc_error_ppm: i64,
    /// Wall time for this cell (ms).
    pub wall_ms: u64,
    /// The executed disposition.
    pub status: Status,
}

fn se_ppm(c: u64, n: u64) -> i64 {
    if n == 0 {
        return PPM;
    }
    let num = u128::from(c) * u128::from(n - c) * (PPM as u128) * (PPM as u128);
    isqrt(num / (u128::from(n) * u128::from(n) * u128::from(n))) as i64
}

/// `run_scenario(s, cap)` — execute one frozen cell.
pub fn run_scenario(s: &Scenario, cap: &Cap) -> ScenarioReport {
    let start = Instant::now();
    let n = s.datasets.min(cap.max_datasets);
    let mut rep = ScenarioReport {
        id: s.id.clone(),
        datasets_declared: s.datasets,
        datasets_run: 0,
        computed: 0,
        refused: BTreeMap::new(),
        generator_failures: 0,
        first_failure: None,
        availability_ppm: 0,
        coverage_ppm: None,
        width_mean_ppm: None,
        width_max_ppm: None,
        mean_abs_err_ppm: None,
        verdict_ppm: None,
        prior_shift_ppm: None,
        mc_error_ppm: 0,
        wall_ms: 0,
        status: Status::Executed,
    };
    let (mut cov, mut wsum, mut wmax, mut err, mut verd, mut pshift) =
        (0u64, 0i128, 0i64, 0i128, 0u64, 0i128);
    let (mut verd_n, mut sens_n) = (0u64, 0u64);
    for i in 0..n {
        if let Some(d) = cap.deadline {
            if Instant::now() >= d {
                rep.status = Status::Stopped;
                break;
            }
        }
        rep.datasets_run += 1;
        let data = dataset(s, i);
        match eval_dataset(s, &data) {
            Outcome::Computed {
                headline,
                signflip_p_ppm,
                sens_posterior_mean_ppm,
            } => {
                rep.computed += 1;
                let h = *headline;
                let covered =
                    h.interval.lo <= data.true_delta_ppm && data.true_delta_ppm <= h.interval.hi;
                if covered {
                    cov += 1;
                }
                let w = h.interval.hi - h.interval.lo;
                wsum += i128::from(w);
                wmax = wmax.max(w);
                err += (i128::from(h.posterior_mean_ppm) - i128::from(data.true_delta_ppm)).abs();
                if let Some(p) = signflip_p_ppm {
                    verd_n += 1;
                    if p < ALPHA_PPM {
                        verd += 1;
                    }
                }
                if let Some(sm) = sens_posterior_mean_ppm {
                    sens_n += 1;
                    pshift += (i128::from(sm) - i128::from(h.posterior_mean_ppm)).abs();
                }
            }
            Outcome::Refused(name) => *rep.refused.entry(name.to_string()).or_default() += 1,
            Outcome::GeneratorFailure(detail) => {
                rep.generator_failures += 1;
                if rep.first_failure.is_none() {
                    rep.first_failure = Some(detail);
                }
            }
        }
    }
    rep.wall_ms = start.elapsed().as_millis() as u64;
    let run = rep.datasets_run;
    rep.availability_ppm = if run > 0 {
        ((u128::from(rep.computed) * PPM as u128) / u128::from(run)) as i64
    } else {
        0
    };
    if rep.computed > 0 {
        let c = rep.computed;
        rep.coverage_ppm = Some(((u128::from(cov) * PPM as u128) / u128::from(c)) as i64);
        rep.width_mean_ppm = Some((wsum / i128::from(c)) as i64);
        rep.width_max_ppm = Some(wmax);
        rep.mean_abs_err_ppm = Some((err / i128::from(c)) as i64);
        rep.verdict_ppm =
            (verd_n > 0).then(|| ((u128::from(verd) * PPM as u128) / u128::from(verd_n)) as i64);
        rep.mc_error_ppm = se_ppm(cov, c)
            .max(se_ppm(rep.computed, run))
            .max(if verd_n > 0 { se_ppm(verd, verd_n) } else { 0 });
    } else {
        rep.mc_error_ppm = se_ppm(0, run.max(1));
    }
    rep.prior_shift_ppm = (sens_n > 0).then(|| (pshift / i128::from(sens_n)) as i64);
    rep
}

/// The whole battery record — per-scenario facts plus the admission
/// rollup (all-or-nothing; never a best-subset selection).
#[derive(Debug, Clone)]
pub struct BatteryReport {
    /// `sim-battery/1`.
    pub schema: String,
    /// The declared D2 envelope this run cites (a bound, not spend).
    pub envelope: String,
    /// One record per frozen cell, in table order.
    pub scenarios: Vec<ScenarioReport>,
    /// `"complete"` only when every cell `Executed` at its declared
    /// count with zero generator failures; otherwise
    /// `"incomplete:<ids>"` — an incomplete battery never counts as
    /// method admission.
    pub rollup: String,
}

/// `run_battery(cap)` — execute the frozen table.
pub fn run_battery(cap: &Cap) -> BatteryReport {
    let mut reports = Vec::new();
    for s in scenarios() {
        reports.push(run_scenario(&s, cap));
    }
    battery_rollup(reports)
}

/// The admission rollup over a retained scenario set — `"complete"`
/// only when every cell executed to its declared count with zero
/// generator failures (a `Stopped`/`Failed`/short cell can never
/// count as method admission; no favorable subset is selected).
pub fn battery_rollup(scenarios: Vec<ScenarioReport>) -> BatteryReport {
    let bad: Vec<String> = scenarios
        .iter()
        .filter(|r| {
            r.status != Status::Executed
                || r.datasets_run < r.datasets_declared
                || r.generator_failures > 0
        })
        .map(|r| r.id.clone())
        .collect();
    let rollup = if bad.is_empty() {
        "complete".to_string()
    } else {
        format!("incomplete:{}", bad.join(","))
    };
    BatteryReport {
        schema: BATTERY_VERSION.to_string(),
        envelope: "offline D2 numerical tranche: <=8h wall, 4 cores/8GiB, 32 core-hours, 2GiB"
            .into(),
        scenarios,
        rollup,
    }
}

fn opt_ppm(v: Option<i64>) -> Json {
    v.map(Json::Int).unwrap_or(Json::Null)
}

impl ScenarioReport {
    /// The integer-canonical record (S.2 — `Json::Int` only).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("id", Json::str(&self.id)),
            (
                "datasets_declared",
                Json::Int(self.datasets_declared as i64),
            ),
            ("datasets_run", Json::Int(self.datasets_run as i64)),
            ("computed", Json::Int(self.computed as i64)),
            (
                "refused",
                Json::Arr(
                    self.refused
                        .iter()
                        .map(|(k, v)| {
                            Json::obj([("variant", Json::str(k)), ("count", Json::Int(*v as i64))])
                        })
                        .collect(),
                ),
            ),
            (
                "generator_failures",
                Json::Int(self.generator_failures as i64),
            ),
            (
                "first_failure",
                self.first_failure
                    .as_ref()
                    .map(|f| Json::str(f))
                    .unwrap_or(Json::Null),
            ),
            ("availability_ppm", Json::Int(self.availability_ppm)),
            ("coverage_ppm", opt_ppm(self.coverage_ppm)),
            ("coverage_label", Json::str("diagnostic")),
            ("width_mean_ppm", opt_ppm(self.width_mean_ppm)),
            ("width_max_ppm", opt_ppm(self.width_max_ppm)),
            ("mean_abs_err_ppm", opt_ppm(self.mean_abs_err_ppm)),
            ("verdict_ppm", opt_ppm(self.verdict_ppm)),
            ("verdict_kind", Json::str("sharp_null_signflip")),
            ("alpha_ppm", Json::Int(ALPHA_PPM)),
            ("prior_shift_ppm", opt_ppm(self.prior_shift_ppm)),
            ("mc_error_ppm", Json::Int(self.mc_error_ppm)),
            ("wall_ms", Json::Int(self.wall_ms as i64)),
            (
                "status",
                Json::str(match &self.status {
                    Status::Executed => "executed",
                    Status::Stopped => "stopped",
                    Status::Failed(_) => "failed",
                }),
            ),
            (
                "status_detail",
                match &self.status {
                    Status::Failed(d) => Json::str(d),
                    _ => Json::Null,
                },
            ),
        ])
    }
}

impl BatteryReport {
    /// `true` only when the rollup is `complete` — the admission
    /// predicate (all-or-nothing).
    pub fn is_complete(&self) -> bool {
        self.rollup == "complete"
    }

    /// The retained integer-canonical battery record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(&self.schema)),
            ("envelope", Json::str(&self.envelope)),
            ("rollup", Json::str(&self.rollup)),
            (
                "scenarios",
                Json::Arr(self.scenarios.iter().map(|s| s.to_json()).collect()),
            ),
        ])
    }
}
