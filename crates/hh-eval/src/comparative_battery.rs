//! `comparative_battery` — the comparative extension of `sim_battery`
//! (R6.27 stage 1; REQ-XD-024 · `AC-6.4-3`; the §4.3 route's retained
//! simulation battery over the exact R6.18 → R6.19 pipeline).
//!
//! The battery executes the exact pipeline over synthesized datasets —
//! `materialize_comparative` (the R6.18 `comparative_protocol/1`
//! materializer, the seeded k-round schedule and its
//! `slot_schedule_randomization` record) → `population::assemble` →
//! `comparative_estimand` → the measured contrast's
//! `finite_manifest_headline` and the reference arm's
//! `arm_marginal_headline` (estimator-identical reported intervals —
//! the same `cell_posterior_draws` stream the estimators consume,
//! truncated at `BASE_DRAWS` so the ≥2,000-datasets/scenario execution
//! stays inside the offline envelope, exactly the `sim_battery`
//! precedent) → `confirmatory_family` (Holm α = .05 primary +
//! exploratory BH) → `tied_with_leader` → `rerun_flip`, `swap_flip`,
//! `pass_rates`, `cost_routes` — at **≥2,000 datasets per scenario
//! cell**, recording availability, the per-leg refusal census, the
//! family's verdict probabilities (the `relevant` type-I reading on
//! null cells, detection probability elsewhere), coverage (diagnostic,
//! never a nominal guarantee), interval widths, the non-tied-task
//! count `m` and binomial Monte Carlo error.
//!
//! The grid is frozen *before execution* (the `sim_battery` rule):
//! `H ∈ {2,4,8} × T ∈ {20,37,48} × k = 5 × mixtures
//! {board,easyband,hard} × σ ∈ {0, .75}` at the null effect; the
//! `board / σ = .75 / Δ = +.10` power strip; the three mechanism legs
//! (`informative infra missingness`, `flagged-pass vetoes`,
//! `shared-shock rounds`) at the declared `H = 4, T = 37` base cell
//! with and without effect; and the first-pass `T = 10` grid at
//! `H ∈ {3, 2}` over `Δ ∈ {0, +.09, +.13, +.20}` — the cells the
//! pre-registration quotes `P(m ≥ 7 non-tied tasks)`, the power table
//! and the median bootstrap half-width from (§10.1.3; contributes
//! REQ-XH-111). The arm × task interaction is the σ = .75 leg: the
//! per-(task, arm) Gaussian draw *is* the interaction (σ = 0 removes
//! it — the declared reading of the REQ-XD-024 "interaction" factor).
//!
//! The DGP mirrors design/35 Appendix B (`sim.py`): per-task base rate
//! `p_t ~ Beta(a, b)` clipped to `[.005, .995]` under the declared
//! mixture (`board` Beta(.575, .384), `easyband` Beta(.636, .273),
//! `hard` Beta(.384, .575)); arm `h ≠ ref` applies the logit shift
//! `β = logit(p̄ + δ) − logit(p̄)`, `p̄ = a/(a+b)`, the closed-form
//! counterpart of `sim.py`'s bisected shift — the **realized** equal-
//! task Δ each dataset committed to is recorded as a fact
//! (`true_delta_ppm`), never the declared target. Outcomes are
//! Bernoulli per slot unless the mechanism alters them:
//!
//! - `informative infra missingness` — a failed trial drops to
//!   unresolved at rate 1/2 (missingness ∝ failure; the cell's
//!   `UnresolvedCell` refusals and the completion envelope are the
//!   record);
//! - `flagged-pass vetoes` — a passing trial trips the observed-veto
//!   axis at rate 1/10 (the `joint` population scores it a failure —
//!   the flagged = fail route the scan feeds);
//! - `shared-shock rounds` — one shared uniform decides every arm's
//!   outcome on a (task, round) slot (a schedule-position shock the
//!   randomization record does not remove; the distortion is a fact).
//!
//! New pieces run `use_as: descriptive` — `confirmatory`/`first_pass`
//! stays `MethodNotAdmitted` until a GATE-G23 PASSED record exists
//! (REQ-XD-023; a fabricated admission record is never manufactured
//! for the battery). The battery accepts an apparatus, never a
//! favorable treatment effect (AC-4-2). Records are integer-canonical
//! (`Json::Int`-only numerics) like `sim_battery/1`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use hharness_lab::experiment::{Backoff, CancelPolicy, ReattemptPolicy};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

use crate::central_protocol::Warmth;
use crate::comparative_estimators::{
    comparative_estimand, confirmatory_family, cost_routes, pass_rates, rerun_flip, swap_flip,
    tied_with_leader, FlipCell, TrialCost, UseClass,
};
use crate::comparative_estimators::{EstimandDecl, FamilyDeclC};
use crate::comparative_protocol::{
    materialize_comparative, ArmDecl, ComparativeDecl, ComparativeProtocol, Identity, Pooling,
    ScheduleDecl, ScheduleEvidence, SeedSupport, SupportDecl, EXCHANGEABILITY, SCHEDULE_KIND,
};
use crate::finite_manifest::{
    arm_marginal_headline, finite_manifest_headline, ContrastDecl, REGISTERED_PRIOR,
    REGISTERED_PRIOR_NAME,
};
use crate::population::{
    assemble, contrib, Availability, BoundsDecl, PlannedSlot, PopulationKind, SlotSubject,
};
use crate::sim_battery::dataset_seed_material;
use crate::stats::{bootstrap_paired_with, isqrt, quantile, XorShift64, PPM};
use crate::task_selection::{
    materialize_selection, DatasetPin, SelectionDecl, TaskRow, STRATIFICATION,
};

/// The battery contract version (record `schema`).
pub const CB_VERSION: &str = "comparative-battery/1";
/// The frozen per-scenario dataset floor (≥2,000 — REQ-XD-024).
pub const CB_DATASETS: u64 = 2_000;
/// The frozen replicate count k = 5 (REQ-XD-002's confirmatory floor).
pub const K: u32 = 5;
/// The clip bounds of the DGP ([.005, .995] — `sim.py`'s `clip`).
pub const CLIP_LO: f64 = 0.005;
/// The clip bounds of the DGP.
pub const CLIP_HI: f64 = 0.995;
/// The reference arm id.
pub const REF_ARM: &str = "ref";
/// The observed-veto axis spelling the flagged-pass mechanism trips —
/// any non-empty `veto_tripped` member is the veto fact; the id names
/// its source.
pub const FLAG_VETO: &str = "v:flagged_pass";
/// The declared per-trial cost model (µ$): base + per-failure tail.
/// Synthesized, declared, never a spend claim.
pub const COST_BASE_MICROS: i64 = 40_000;
/// The declared per-failure cost tail (µ$).
pub const COST_FAILURE_MICROS: i64 = 12_000;
/// The descriptive bootstrap draws (the declared-draws floor).
const BOOT_DRAWS: u64 = 2_000;

/// The frozen task-difficulty mixtures (D1's DGP — design/35 Appx B).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CMixture {
    /// `Beta(.575, .384)` — the HF-board-like rate mixture.
    Board,
    /// `Beta(.636, .273)` — the easier, tighter band.
    Easyband,
    /// `Beta(.384, .575)` — the hard band.
    Hard,
}

impl CMixture {
    fn name(self) -> &'static str {
        match self {
            CMixture::Board => "board",
            CMixture::Easyband => "easyband",
            CMixture::Hard => "hard",
        }
    }
    fn ab(self) -> (f64, f64) {
        match self {
            CMixture::Board => (0.575, 0.384),
            CMixture::Easyband => (0.636, 0.273),
            CMixture::Hard => (0.384, 0.575),
        }
    }
}

/// The frozen validity mechanisms (REQ-XD-024's mechanism factors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CMechanism {
    /// Independent Bernoulli members — the design assumption.
    Clean,
    /// Failed trials drop to unresolved at rate 1/2 — informative
    /// infra missingness (∝ failure).
    InformativeInfra,
    /// Passing trials trip the observed-veto axis at rate 1/10 — the
    /// flagged-pass route (flagged = fail in the `joint` population).
    FlaggedPassVeto,
    /// One shared uniform decides every arm's outcome on a
    /// (task, round) slot — a schedule-position shared shock.
    SharedShockRounds,
}

/// One frozen scenario cell.
#[derive(Debug, Clone)]
pub struct CScenario {
    /// The cell id (stable seed material).
    pub id: String,
    /// Arms `H` (the reference plus `H − 1` comparators at the
    /// declared effect).
    pub harnesses: u64,
    /// Tasks `T`.
    pub tasks: u64,
    /// The mixture.
    pub mixture: CMixture,
    /// The arm × task interaction scale (ppm of the declared σ: 0 or
    /// 750_000 for σ = .75).
    pub sigma_ppm: i64,
    /// The declared effect `Δ` (ppm, rate scale — the logit-shift
    /// target; the realized Δ is recorded, never the declaration).
    pub effect_ppm: i64,
    /// The mechanism.
    pub mechanism: CMechanism,
    /// The declared dataset floor — always ≥ [`CB_DATASETS`].
    pub datasets: u64,
}

fn eff_name(ppm: i64) -> String {
    format!(
        "{}{:02}",
        if ppm < 0 { "-" } else { "+" },
        ppm.abs() / 10_000
    )
}

/// The frozen scenario table (77 cells): the full
/// `H × T × mixture × σ` grid at the null effect (54); the
/// `board/σ=.75/Δ=+.10` power strip (9); the three mechanism legs at
/// `h4-t37-board-s75` under `Δ ∈ {0, +.10}` (6); the first-pass
/// `T = 10` cells at `H ∈ {3, 2}`, `Δ ∈ {0, +.09, +.13, +.20}` under
/// `board/σ = .75` (8 — the §10.1.3 quoted shape).
pub fn cb_scenarios() -> Vec<CScenario> {
    let mut v = Vec::new();
    for h in [2u64, 4, 8] {
        for t in [20u64, 37, 48] {
            for mix in [CMixture::Board, CMixture::Easyband, CMixture::Hard] {
                for s in [0i64, 750_000] {
                    v.push(CScenario {
                        id: format!("h{h}-t{t}-{}-s{:02}-e+00", mix.name(), s / 10_000),
                        harnesses: h,
                        tasks: t,
                        mixture: mix,
                        sigma_ppm: s,
                        effect_ppm: 0,
                        mechanism: CMechanism::Clean,
                        datasets: CB_DATASETS,
                    });
                }
            }
        }
    }
    // The power strip — the full H × T crossing under board/σ=.75 at
    // Δ = +.10 (the §6.7 menu's quoted effect).
    for h in [2u64, 4, 8] {
        for t in [20u64, 37, 48] {
            v.push(CScenario {
                id: format!("h{h}-t{t}-board-s75-e+10"),
                harnesses: h,
                tasks: t,
                mixture: CMixture::Board,
                sigma_ppm: 750_000,
                effect_ppm: 100_000,
                mechanism: CMechanism::Clean,
                datasets: CB_DATASETS,
            });
        }
    }
    // The mechanism legs at the declared base cell, with and without
    // effect.
    for (name, mech) in [
        ("informative-infra", CMechanism::InformativeInfra),
        ("flagged-pass-veto", CMechanism::FlaggedPassVeto),
        ("shared-shock-rounds", CMechanism::SharedShockRounds),
    ] {
        for e in [0i64, 100_000] {
            v.push(CScenario {
                id: format!("mech/{name}-h4-t37-board-s75-e{}", eff_name(e)),
                harnesses: 4,
                tasks: 37,
                mixture: CMixture::Board,
                sigma_ppm: 750_000,
                effect_ppm: e,
                mechanism: mech,
                datasets: CB_DATASETS,
            });
        }
    }
    // The first-pass T = 10 grid — H ∈ {3, 2} × Δ ∈ {0, +.09, +.13,
    // +.20} under board/σ = .75 (§10.1.3's quoted quantities).
    for h in [3u64, 2] {
        for e in [0i64, 90_000, 130_000, 200_000] {
            v.push(CScenario {
                id: format!("fp/h{h}-t10-board-s75-e{}", eff_name(e)),
                harnesses: h,
                tasks: 10,
                mixture: CMixture::Board,
                sigma_ppm: 750_000,
                effect_ppm: e,
                mechanism: CMechanism::Clean,
                datasets: CB_DATASETS,
            });
        }
    }
    v
}

// ── the frozen DGP helpers (deterministic under the seed rule) ───────

/// A uniform (0,1) draw from the canonical stream.
fn unif(rng: &mut XorShift64) -> f64 {
    ((rng.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
}

/// A standard Gaussian draw (Box–Muller) — deterministic under the
/// scenario's derivable stream.
fn gauss(rng: &mut XorShift64) -> f64 {
    let u1 = unif(rng).max(1e-300);
    let u2 = unif(rng);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// `Gamma(α, 1)` — Marsaglia–Tsang with the α < 1 boost (deterministic).
fn gamma_sample(rng: &mut XorShift64, alpha: f64) -> f64 {
    if alpha < 1.0 {
        return gamma_sample(rng, alpha + 1.0) * unif(rng).powf(1.0 / alpha);
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = gauss(rng);
        let v = (1.0 + c * x).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u = unif(rng);
        if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

/// `Beta(a, b)` via the two-gamma construction — deterministic.
fn beta_sample(rng: &mut XorShift64, a: f64, b: f64) -> f64 {
    let x = gamma_sample(rng, a);
    let y = gamma_sample(rng, b);
    x / (x + y)
}

fn logit(p: f64) -> f64 {
    (p / (1.0 - p)).ln()
}
fn expit(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}
fn clip(p: f64) -> f64 {
    p.clamp(CLIP_LO, CLIP_HI)
}

// ── the consumed selection + materialized protocol (per scenario) ────

fn sim_selection(tasks: u64) -> crate::task_selection::TaskSelection {
    let rows: Vec<TaskRow> = (0..tasks)
        .map(|i| TaskRow {
            task_id: format!("t{i}"),
            agent_timeout_sec: 900,
            difficulty: "medium".into(),
            hf_saturated: false,
        })
        .collect();
    let mut digests = BTreeMap::new();
    for r in &rows {
        digests.insert(r.task_id.clone(), format!("sim-dataset-toml:{}", r.task_id));
    }
    materialize_selection(&SelectionDecl {
        pin: DatasetPin {
            git_commit: "12b75656f4b2dbb8b3b8e5a8a4f2a30e2d5d6e70".into(),
            hub_dataset_ref: "hub:tb2.1@12b75656".into(),
            dataset_toml_digests: digests,
        },
        census: rows,
        exclusions: vec![],
        exclusion_ref: String::new(),
        seed: "seed:comparative-battery".into(),
        stratification: STRATIFICATION.into(),
        dev_pool_size: 0,
        split_predates_access: true,
        input_manifest: vec![
            "task_id".into(),
            "agent_timeout_sec".into(),
            "difficulty".into(),
            "hf_saturated".into(),
        ],
        draws: vec![],
    })
    .expect("battery selection")
}

fn arm(id: &str) -> ArmDecl {
    ArmDecl {
        arm_id: id.into(),
        admission_ref: format!("admission:{id}"),
        harness_version: format!("{id}@2.4.6"),
        effort_pin: "effort:high".into(),
        catalog_identity: format!("catalog:{id}"),
        rewrite_version: "rewrite:1".into(),
        supports: SupportDecl {
            fingerprint: true,
            cache: true,
            seed: SeedSupport::Verified,
        },
    }
}

/// `materialize_for(s)` — the scenario's `comparative_protocol/1`
/// record through the exact R6.18 materializer (the consumed selection
/// puts every drawn task in held-out; the schedule seed is the
/// scenario id's derivable stream — recorded, never shopped).
pub fn materialize_for(s: &CScenario) -> ComparativeProtocol {
    let sel = sim_selection(s.tasks);
    let tasks: Vec<String> = sel
        .cells
        .iter()
        .flat_map(|c| c.held_out.iter().cloned())
        .take(s.tasks as usize)
        .collect();
    debug_assert_eq!(tasks.len() as u64, s.tasks);
    let mut arms = vec![arm(REF_ARM)];
    for h in 1..s.harnesses {
        arms.push(arm(&format!("h{h}")));
    }
    materialize_comparative(&ComparativeDecl {
        selection: sel,
        tasks,
        arms,
        k: K,
        identity: Identity::Confirmatory,
        schedule: ScheduleDecl {
            kind: SCHEDULE_KIND.into(),
            seed: format!("sched:{}", s.id),
            exchangeability: EXCHANGEABILITY.into(),
        },
        lanes: vec!["lane:a".into(), "lane:b".into(), "lane:c".into()],
        lane_spec_ref: "lane-spec:sim".into(),
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 1_000,
                multiplier_ppm: 2_000_000,
                max_ms: 60_000,
            },
            error_classes_included: Some(vec!["infrastructure_failure".into()]),
            on_cancel: CancelPolicy::Replan,
        },
        warmth: Warmth::ColdStart,
        drift_observed: false,
        pilot_labelled_as: None,
        phases: BTreeMap::from([
            ("pilot".to_string(), "phase:pilot".to_string()),
            ("tranche".to_string(), "phase:tranche".to_string()),
        ]),
        family_id: "family:many-to-one".into(),
        reference_arm: REF_ARM.into(),
        primaries: vec!["capability.task_success".into()],
        multiplicity_id: "holm".into(),
        stops: vec!["stop:budget".into(), "stop:infra".into()],
        pooling: Pooling::NotRequested,
        fingerprint_gate: false,
    })
    .expect("battery protocol materializes")
}

/// The scenario's arms — `ref` first, comparators at the effect.
pub fn arms_of(s: &CScenario) -> Vec<String> {
    let mut v = vec![REF_ARM.to_string()];
    for h in 1..s.harnesses {
        v.push(format!("h{h}"));
    }
    v
}

// ── one synthesized dataset ──────────────────────────────────────────

/// One dataset — the planned-slot universe plus the realized equal-task
/// Δ of the measured contrast (the last declared arm vs `ref`).
#[derive(Debug)]
pub struct CDataset {
    /// The population assembly input.
    pub input: crate::population::PopulationInput,
    /// The realized `Σ_t (p_last,t − p_ref,t)/T` this dataset was drawn
    /// under (ppm of probability — a fact, never the declared target).
    pub true_delta_ppm: i64,
    /// The realized reference-arm rate `Σ_t p_ref,t/T` (ppm).
    pub true_theta_ppm: i64,
    /// The dataset replicate index (the declared estimator seed).
    pub replicate: u64,
}

/// `cb_dataset(s, replicate)` — one deterministic synthetic dataset
/// under the frozen DGP over the materialized schedule (the slot
/// identity is the protocol's own; outcomes fill its slots).
pub fn cb_dataset(s: &CScenario, proto: &ComparativeProtocol, replicate: u64) -> CDataset {
    let mut rng = XorShift64::seeded(&dataset_seed_material(&s.id, replicate));
    let (a, b) = s.mixture.ab();
    let pbar = a / (a + b);
    let delta = s.effect_ppm as f64 / PPM as f64;
    let beta_shift = if delta == 0.0 {
        0.0
    } else {
        logit(clip(pbar + delta)) - logit(pbar)
    };
    let sigma = s.sigma_ppm as f64 / PPM as f64;
    // Per-(task, arm) rates: p_t ~ Beta(a,b); p_{t,h} =
    // expit(logit(p_t) + β_h + g_{t,h}), g ~ N(0, σ²); ref's β = 0.
    let arms = arms_of(s);
    let mut rate: BTreeMap<(usize, usize), f64> = BTreeMap::new(); // (task, arm idx) → p
    let mut true_sum: i128 = 0;
    let mut theta_sum: i128 = 0;
    for t in 0..s.tasks as usize {
        let p_t = clip(beta_sample(&mut rng, a, b));
        let l = logit(p_t);
        for (hi, _) in arms.iter().enumerate() {
            let g = if sigma > 0.0 {
                gauss(&mut rng) * sigma
            } else {
                0.0
            };
            let shift = if hi == 0 { 0.0 } else { beta_shift };
            let p = clip(expit(l + shift + g));
            rate.insert((t, hi), p);
            if hi == 0 {
                theta_sum += (p * PPM as f64) as i128;
            }
        }
        true_sum += ((rate[&(t, arms.len() - 1)] - rate[&(t, 0)]) * PPM as f64) as i128;
    }
    // Fill the materialized schedule's slots in emitted order.
    let mut slots = Vec::with_capacity(proto.schedule.len());
    let mut required = BTreeSet::new();
    let mut pins = BTreeMap::new();
    for slot in &proto.schedule {
        required.insert(slot.slot_ref.clone());
        let t = slot
            .task_id
            .strip_prefix('t')
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or(0);
        let hi = if slot.arm_id == REF_ARM {
            0
        } else {
            slot.arm_id
                .strip_prefix('h')
                .and_then(|x| x.parse::<usize>().ok())
                .unwrap_or(0)
        };
        let p = rate[&(t, hi)];
        let (outcome, success, veto, avail) = match s.mechanism {
            CMechanism::Clean | CMechanism::InformativeInfra => {
                let ok = unif(&mut rng) < p;
                if s.mechanism == CMechanism::InformativeInfra && !ok && rng.below(2) == 0 {
                    (None, None, vec![], Availability::NotRun)
                } else {
                    (
                        Some(OutcomeClass::Scored),
                        Some(ok),
                        vec![],
                        Availability::Resolved,
                    )
                }
            }
            CMechanism::FlaggedPassVeto => {
                let ok = unif(&mut rng) < p;
                let v = if ok && rng.below(10) == 0 {
                    vec![FLAG_VETO.to_string()]
                } else {
                    vec![]
                };
                (
                    Some(OutcomeClass::Scored),
                    Some(ok),
                    v,
                    Availability::Resolved,
                )
            }
            CMechanism::SharedShockRounds => {
                // The shared uniform is drawn per (task, round) once —
                // consumed in slot order for every arm on that slot.
                // Draw it lazily but deterministically: one uniform per
                // (task, round) shared across arms.
                let u = shared_uniform(s, replicate, t, slot.round);
                (
                    Some(OutcomeClass::Scored),
                    Some(u < p),
                    vec![],
                    Availability::Resolved,
                )
            }
        };
        let subject_run = format!("run:{}", slot.slot_ref);
        pins.insert(subject_run.clone(), 1u64);
        slots.push(PlannedSlot {
            plan_ref: slot.slot_ref.clone(),
            task_id: slot.task_id.clone(),
            split_label: "held_out".into(),
            stratum: "sim".into(),
            arm_id: slot.arm_id.clone(),
            replicate_index: u64::from(slot.round),
            subjects: vec![SlotSubject {
                subject_run,
                attempt_no: 1,
                outcome,
                success,
                veto_tripped: veto,
                availability: avail,
                seq: 1,
                missing_required_checks: false,
                admissible: true,
            }],
        });
    }
    CDataset {
        input: crate::population::PopulationInput {
            slots,
            required,
            pins,
        },
        true_delta_ppm: (true_sum / i128::from(s.tasks)) as i64,
        true_theta_ppm: (theta_sum / i128::from(s.tasks)) as i64,
        replicate,
    }
}

/// The shared-shock round's one uniform per (task, round) — a distinct
/// derivable stream, identical across arms on that slot.
fn shared_uniform(s: &CScenario, replicate: u64, task: usize, round: u32) -> f64 {
    let mut rng = XorShift64::seeded(&format!("shock|{}|{replicate}|{task}|{round}", s.id));
    unif(&mut rng)
}

// ── per-dataset eval through the exact pipeline ──────────────────────

/// One arm's `(task → (m, s, u))` under the population contract's one
/// counting rule (`population::contrib` — CC1 reuse, never a second
/// count).
fn arm_cells(
    pop: &crate::population::PlannedPopulation,
    kind: PopulationKind,
    arm: &str,
) -> BTreeMap<String, (u64, u64, u64)> {
    let mut cells: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for s in pop.slots.iter().filter(|s| s.arm_id == arm) {
        let c = cells.entry(s.task_id.clone()).or_insert((0, 0, 0));
        c.0 += 1;
        let (ds, _df, du) = contrib(kind, s);
        c.1 += ds;
        c.2 += du;
    }
    cells
}

/// The measured contrast's per-task deltas (`h_last − ref`, ppm).
fn contrast_deltas(
    pop: &crate::population::PlannedPopulation,
    kind: PopulationKind,
    test: &str,
    control: &str,
) -> Result<Vec<(String, i64)>, String> {
    let tc = arm_cells(pop, kind, test);
    let cc = arm_cells(pop, kind, control);
    let mut out = Vec::new();
    for (t, &(tm, ts, tu)) in &tc {
        let &(cm, cs, cu) = cc.get(t).ok_or_else(|| format!("unpaired {t}"))?;
        if tu > 0 {
            return Err(format!("UnresolvedCell: ({t}, {test}) u={tu}"));
        }
        if cu > 0 {
            return Err(format!("UnresolvedCell: ({t}, {control}) u={cu}"));
        }
        if tm < 5 || cm < 5 {
            return Err(format!("insufficient_replicates: ({t}, {test}|{control})"));
        }
        out.push((
            t.clone(),
            (((i128::from(ts) * i128::from(cm) - i128::from(cs) * i128::from(tm))
                * i128::from(PPM))
                / (i128::from(tm) * i128::from(cm))) as i64,
        ));
    }
    Ok(out)
}

/// One dataset's outcome under the exact pipeline — facts, never
/// verdicts on the method.
#[derive(Debug)]
pub enum COutcome {
    /// The pipeline computed — every leg's recorded facts.
    Computed {
        /// The measured contrast's reported interval and observed Δ.
        fm: Option<FmLeg>,
        /// The ref arm's reported θ interval and observed θ̂.
        rate: Option<RateLeg>,
        /// The family rows `(member, status, raw_p, adjusted, rejected, mode)`.
        family: Vec<(String, String, Option<i64>, i64, bool, Option<String>)>,
        /// Exploratory member count.
        exploratory_members: u64,
        /// Per-primary-member non-tied task count `m` (`member → m`).
        nontied: Vec<(String, u64)>,
        /// The task-bootstrap interval of the measured contrast.
        boot: Option<crate::stats::Interval>,
        /// The tied leg's leader and tied flags (`None` = no interval).
        tied: (String, Vec<(String, Option<bool>)>),
        /// Descriptive leg points `(rerun_ref, swap_last, excess_last,
        /// pass_all5, pass_any5, per_trial, per_solve)`.
        points: (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
            Option<i64>,
        ),
        /// The estimand's per-contrast refusals (`member → reason`).
        refusals: Vec<(String, String)>,
    },
    /// A typed refusal through an estimator path (its variant name).
    Refused(&'static str),
    /// `population::assemble` failed — a generator failure, retained.
    GeneratorFailure(String),
}

/// The measured-contrast leg's recorded facts.
#[derive(Debug)]
pub struct FmLeg {
    /// The reported 95% equal-tail credible interval (estimator-identical).
    pub interval: crate::stats::Interval,
    /// The observed equal-task Δ.
    pub observed_ppm: i64,
}

/// The single-arm leg's recorded facts.
#[derive(Debug)]
pub struct RateLeg {
    /// The reported 95% equal-tail credible interval (estimator-identical).
    pub interval: crate::stats::Interval,
    /// The observed equal-task rate.
    pub observed_ppm: i64,
}

fn refusal_name(e: &crate::comparative_estimators::EstimatorError) -> &'static str {
    use crate::comparative_estimators::EstimatorError as E;
    match e {
        E::Population(_) => "population",
        E::OneTask => "one_task",
        E::ShortCell { .. } => "short_cell",
        E::UnresolvedCell { .. } => "unresolved_cell",
        E::DegenerateBootstrap => "degenerate_bootstrap",
        E::DrawsBelowFloor { .. } => "draws_below_floor",
        E::MethodNotAdmitted { .. } => "method_not_admitted",
        E::UnsupportedUse { .. } => "unsupported_use",
        E::UnsupportedClaim { .. } => "unsupported_claim",
        E::EstimatorUndefined { .. } => "estimator_undefined",
        E::ExchangeabilityNotAdmitted { .. } => "exchangeability_not_admitted",
        E::RandomizationRecord { .. } => "randomization_record",
        E::MissingKey { .. } => "missing_key",
        E::NonFiniteInput { .. } => "non_finite_input",
        E::UnknownArm { .. } => "unknown_arm",
        E::UnpairedTask { .. } => "unpaired_task",
    }
}

/// `eval_cb_dataset(s, proto, data)` — run one dataset through the
/// exact pipeline. Every leg's outcome is a recorded fact.
pub fn eval_cb_dataset(s: &CScenario, proto: &ComparativeProtocol, data: &CDataset) -> COutcome {
    let pop = match assemble(crate::population::PopulationInput {
        slots: data.input.slots.clone(),
        required: data.input.required.clone(),
        pins: data.input.pins.clone(),
    }) {
        Ok(p) => p,
        Err(e) => return COutcome::GeneratorFailure(e.to_string()),
    };
    let arms = arms_of(s);
    let last = arms.last().cloned().unwrap_or_else(|| REF_ARM.into());
    let use_descriptive = UseClass::Descriptive.as_str();

    // The estimand — θ_h, Δ(h − ref), refusals, bounds, envelope.
    let est = match comparative_estimand(
        &pop,
        &EstimandDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            arms: arms.clone(),
            reference_arm: REF_ARM.into(),
            stratum: Some("sim".into()),
            use_as: use_descriptive.into(),
            admission: None,
        },
    ) {
        Ok(r) => r,
        Err(e) => return COutcome::Refused(refusal_name(&e)),
    };
    let refusals: Vec<(String, String)> = est
        .contrasts
        .iter()
        .filter_map(|c| c.refusal.clone().map(|r| (c.test_arm.clone(), r)))
        .collect();

    // The measured contrast's reported interval (estimator-identical).
    let fm = match finite_manifest_headline(
        &pop,
        &ContrastDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            test_arm: last.clone(),
            control_arm: REF_ARM.into(),
            stratum: Some("sim".into()),
            prior: REGISTERED_PRIOR,
            prior_name: REGISTERED_PRIOR_NAME.into(),
            dependence: None,
            target: None,
            use_as: None,
            bounds: BoundsDecl::Unit,
            level_ppm: 950_000,
            seed: data.replicate,
        },
    ) {
        Ok(h) => Some(FmLeg {
            interval: h.interval,
            observed_ppm: h.observed_contrast_ppm,
        }),
        Err(_) => None,
    };

    // The ref arm's θ interval — the single-arm marginal's reported
    // rung (estimator-identical), derived under the one counting rule.
    let rate = {
        let cells = arm_cells(
            &pop,
            PopulationKind::JointTaskPassWithoutObservedVeto,
            REF_ARM,
        );
        let mut cn: Vec<(u64, u64)> = Vec::new();
        let mut refused = false;
        let mut obs_sum: i128 = 0;
        for (_, (m, s_, u)) in &cells {
            if *u > 0 {
                refused = true;
            }
            cn.push((*s_, *m));
            if *m > 0 {
                obs_sum += i128::from(*s_) * i128::from(PPM) / i128::from(*m);
            }
        }
        if refused {
            None
        } else {
            let seed_material = format!(
                "finite_manifest_rate|finite-manifest-rate/1|{}|{data_replicate}",
                s.id,
                data_replicate = data.replicate
            );
            let interval = arm_marginal_headline(&cn, REGISTERED_PRIOR, &seed_material);
            Some(RateLeg {
                interval,
                observed_ppm: (obs_sum / cells.len().max(1) as i128) as i64,
            })
        }
    };

    // The per-tranche family — the real estimator, descriptive use.
    let fam = match confirmatory_family(
        &pop,
        &FamilyDeclC {
            tranche_id: format!("tr:{}", s.id),
            family_id: "family:many-to-one".into(),
            exploratory_id: "family:exploratory".into(),
            arms: arms.clone(),
            reference_arm: REF_ARM.into(),
            metric: "capability.task_success".into(),
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            stratum: Some("sim".into()),
            exchangeability: EXCHANGEABILITY.into(),
            schedule: Some(ScheduleEvidence::of(proto)),
            seed: data.replicate,
            use_as: use_descriptive.into(),
            admission: None,
        },
    ) {
        Ok(f) => Some(f),
        Err(e) => return COutcome::Refused(refusal_name(&e)),
    };
    let (family_rows, exploratory_n, nontied) = match &fam {
        Some(f) => {
            let rows: Vec<(String, String, Option<i64>, i64, bool, Option<String>)> = f
                .family
                .1
                .iter()
                .map(|r| {
                    (
                        r.member.clone(),
                        r.status.clone(),
                        r.raw_p_ppm,
                        r.adjusted_ppm,
                        r.rejected_at_alpha,
                        r.mode.clone(),
                    )
                })
                .collect();
            // Non-tied task count per primary member — recomputed from
            // the paired cells the member read.
            let mut nt = Vec::new();
            for (member, _, _, _, _, _) in &rows {
                let test = member.clone();
                match contrast_deltas(
                    &pop,
                    PopulationKind::JointTaskPassWithoutObservedVeto,
                    &test,
                    REF_ARM,
                ) {
                    Ok(ds) => nt.push((
                        member.clone(),
                        ds.iter().filter(|(_, d)| *d != 0).count() as u64,
                    )),
                    Err(_) => nt.push((member.clone(), 0)),
                }
            }
            (rows, f.exploratory.1.len() as u64, nt)
        }
        None => (Vec::new(), 0, Vec::new()),
    };

    // The task-bootstrap interval of the measured contrast (the
    // §6.7/§10.1.3 quoted half-width's estimator path — percentile,
    // declared draws ≥ 2,000).
    let boot = contrast_deltas(
        &pop,
        PopulationKind::JointTaskPassWithoutObservedVeto,
        &last,
        REF_ARM,
    )
    .ok()
    .and_then(|ds| {
        let xs: Vec<i64> = ds.iter().map(|(_, d)| *d).collect();
        bootstrap_paired_with(
            &xs,
            950_000,
            false,
            &format!("comparative-battery/1|boot|{}|{}", s.id, data.replicate),
            BOOT_DRAWS as u32,
        )
    });

    // Tied-with-leader (descriptive).
    let tied = match tied_with_leader(
        &pop,
        PopulationKind::JointTaskPassWithoutObservedVeto,
        &arms,
        Some("sim"),
        use_descriptive,
        None,
        None,
        BOOT_DRAWS,
        data.replicate,
    ) {
        Ok(r) => (
            r.leader_id.clone(),
            r.rows
                .iter()
                .map(|row| (row.arm_id.clone(), row.tied))
                .collect(),
        ),
        Err(_) => (String::new(), Vec::new()),
    };

    // The descriptive flip/pass/cost legs on the reference arm and the
    // measured pair.
    let flip_cells = |arm: &str| -> Vec<FlipCell> {
        arm_cells(&pop, PopulationKind::JointTaskPassWithoutObservedVeto, arm)
            .iter()
            .map(|(t, &(m, s_, _))| FlipCell {
                task_id: t.clone(),
                c: s_,
                n: m,
            })
            .collect()
    };
    let rerun = rerun_flip(
        &flip_cells(REF_ARM),
        use_descriptive,
        None,
        BOOT_DRAWS,
        data.replicate,
    )
    .ok()
    .map(|r| r.point);
    let (swap_pt, excess_pt) = match swap_flip(
        &last,
        &flip_cells(&last),
        REF_ARM,
        &flip_cells(REF_ARM),
        use_descriptive,
        None,
        BOOT_DRAWS,
        data.replicate,
    ) {
        Ok(r) => (Some(r.swap_ppm), Some(r.excess_ppm)),
        Err(_) => (None, None),
    };
    let (p_all, p_any) = match pass_rates(
        &flip_cells(REF_ARM),
        K as u64,
        use_descriptive,
        None,
        BOOT_DRAWS,
        data.replicate,
    ) {
        Ok((a, b)) => (Some(a.point), Some(b.point)),
        Err(_) => (None, None),
    };
    let trials: Vec<TrialCost> = pop
        .slots
        .iter()
        .filter(|sl| sl.arm_id == *REF_ARM)
        .filter_map(|sl| {
            let (ds, ..) = contrib(PopulationKind::JointTaskPassWithoutObservedVeto, sl);
            sl.success.map(|ok| TrialCost {
                task_id: sl.task_id.clone(),
                cost_micros: COST_BASE_MICROS + if ok { 0 } else { COST_FAILURE_MICROS },
                solved: ds > 0,
            })
        })
        .collect();
    let (per_trial, per_solve) = match cost_routes(
        &trials,
        "model_spend_only",
        use_descriptive,
        None,
        BOOT_DRAWS,
        data.replicate,
    ) {
        Ok(r) => (r.per_trial_micros, r.per_solve_micros),
        Err(_) => (0, None),
    };

    COutcome::Computed {
        fm,
        rate,
        family: family_rows,
        exploratory_members: exploratory_n,
        nontied,
        boot,
        tied,
        points: (
            rerun, swap_pt, excess_pt, p_all, p_any, per_trial, per_solve,
        ),
        refusals,
    }
}

// ── per-scenario + battery records ───────────────────────────────────

/// Per-scenario record — the frozen member set.
#[derive(Debug, Clone)]
pub struct CScenarioReport {
    /// The cell id.
    pub id: String,
    /// The grid members (echoed for the record).
    pub harnesses: u64,
    /// Tasks.
    pub tasks: u64,
    /// k.
    pub k: u64,
    /// The mixture spelling.
    pub mixture: String,
    /// σ (ppm).
    pub sigma_ppm: i64,
    /// The declared Δ (ppm).
    pub effect_ppm: i64,
    /// The realized mean Δ over computed datasets (ppm).
    pub realized_delta_ppm: Option<i64>,
    /// The mechanism spelling.
    pub mechanism: String,
    /// Declared / run / computed.
    pub datasets_declared: u64,
    /// Actually run.
    pub datasets_run: u64,
    /// Datasets whose pipeline computed.
    pub computed: u64,
    /// Per-leg refusal census (`leg:variant → count`).
    pub refused: BTreeMap<String, u64>,
    /// `assemble` failures.
    pub generator_failures: u64,
    /// First generator failure detail.
    pub first_failure: Option<String>,
    /// `computed / run` (ppm).
    pub availability_ppm: i64,
    /// FM interval coverage of the realized Δ (diagnostic).
    pub fm_coverage_ppm: Option<i64>,
    /// FM interval mean / max width.
    pub fm_width_mean_ppm: Option<i64>,
    /// Max FM width.
    pub fm_width_max_ppm: Option<i64>,
    /// Bootstrap interval coverage of the realized Δ (diagnostic).
    pub boot_coverage_ppm: Option<i64>,
    /// Bootstrap mean / median half-width (the §10.1.3 quote).
    pub boot_halfwidth_mean_ppm: Option<i64>,
    /// The median bootstrap half-width.
    pub boot_halfwidth_median_ppm: Option<i64>,
    /// θ interval coverage of the realized rate (diagnostic).
    pub rate_coverage_ppm: Option<i64>,
    /// Per-primary-member verdict rows (`member → {rejected_ppm,
    /// raw_p_le_025_ppm, m_ge_7_ppm, unavailable_ppm}`).
    pub member_verdicts: Vec<(String, i64, i64, i64, i64)>,
    /// `P(any primary member rejected at Holm α)` (ppm).
    pub family_reject_ppm: Option<i64>,
    /// `P(m ≥ 7 non-tied tasks)` over the first primary member — the
    /// §10.1.3 quantity (T = 10 cells; `None` elsewhere).
    pub m_ge_7_ppm: Option<i64>,
    /// `P(observed leader == argmax true arm rate)` — the tied leg's
    /// leader selection fact.
    pub leader_correct_ppm: Option<i64>,
    /// `P(leader contested)`.
    pub leader_contested_ppm: Option<i64>,
    /// Descriptive leg means `(rerun, swap, excess, pass^5, pass@5,
    /// per_trial, per_solve availability ppm)`.
    pub points_mean: Vec<i64>,
    /// Estimand contrast-refusal census count.
    pub contrast_refusals: u64,
    /// Max binomial Monte Carlo error over the recorded probabilities.
    pub mc_error_ppm: i64,
    /// Wall time for this cell (ms).
    pub wall_ms: u64,
    /// `executed | stopped | failed`.
    pub status: crate::sim_battery::Status,
}

fn se_ppm(c: u64, n: u64) -> i64 {
    if n == 0 {
        return PPM;
    }
    let num = u128::from(c) * u128::from(n - c) * (PPM as u128) * (PPM as u128);
    isqrt(num / (u128::from(n) * u128::from(n) * u128::from(n))) as i64
}

/// `run_cb_scenario(s, cap)` — execute one frozen cell.
pub fn run_cb_scenario(s: &CScenario, cap: &crate::sim_battery::Cap) -> CScenarioReport {
    let start = Instant::now();
    let proto = materialize_for(s);
    let n = s.datasets.min(cap.max_datasets);
    let mut rep = CScenarioReport {
        id: s.id.clone(),
        harnesses: s.harnesses,
        tasks: s.tasks,
        k: K as u64,
        mixture: s.mixture.name().into(),
        sigma_ppm: s.sigma_ppm,
        effect_ppm: s.effect_ppm,
        realized_delta_ppm: None,
        mechanism: format!("{:?}", s.mechanism),
        datasets_declared: s.datasets,
        datasets_run: 0,
        computed: 0,
        refused: BTreeMap::new(),
        generator_failures: 0,
        first_failure: None,
        availability_ppm: 0,
        fm_coverage_ppm: None,
        fm_width_mean_ppm: None,
        fm_width_max_ppm: None,
        boot_coverage_ppm: None,
        boot_halfwidth_mean_ppm: None,
        boot_halfwidth_median_ppm: None,
        rate_coverage_ppm: None,
        member_verdicts: Vec::new(),
        family_reject_ppm: None,
        m_ge_7_ppm: None,
        leader_correct_ppm: None,
        leader_contested_ppm: None,
        points_mean: Vec::new(),
        contrast_refusals: 0,
        mc_error_ppm: 0,
        wall_ms: 0,
        status: crate::sim_battery::Status::Executed,
    };
    let last_arm = format!("h{}", s.harnesses - 1);
    let members: Vec<String> = (1..s.harnesses).map(|h| format!("h{h}")).collect();
    let mut member_counts: BTreeMap<String, (u64, u64, u64, u64)> =
        members.iter().map(|m| (m.clone(), (0, 0, 0, 0))).collect();
    let mut counts = Accums::default();
    let mut point_sums: BTreeMap<usize, i128> = BTreeMap::new();
    let mut point_ns: BTreeMap<usize, u64> = BTreeMap::new();
    let mut boot_halfwidths: Vec<i64> = Vec::new();
    for i in 0..n {
        if let Some(d) = cap.deadline {
            if Instant::now() >= d {
                rep.status = crate::sim_battery::Status::Stopped;
                break;
            }
        }
        rep.datasets_run += 1;
        let data = cb_dataset(s, &proto, i);
        match eval_cb_dataset(s, &proto, &data) {
            COutcome::Computed {
                fm,
                rate,
                family,
                exploratory_members: _,
                nontied,
                boot,
                tied,
                points,
                refusals,
            } => {
                rep.computed += 1;
                counts.delta_sum += i128::from(data.true_delta_ppm);
                rep.contrast_refusals += refusals.len() as u64;
                // FM leg.
                if let Some(f) = &fm {
                    counts.fm_n += 1;
                    if f.interval.lo <= data.true_delta_ppm && data.true_delta_ppm <= f.interval.hi
                    {
                        counts.fm_cov += 1;
                    }
                    let w = f.interval.hi - f.interval.lo;
                    counts.fm_wsum += i128::from(w);
                    counts.fm_wmax = counts.fm_wmax.max(w);
                }
                // Bootstrap leg.
                if let Some(b) = &boot {
                    counts.boot_n += 1;
                    if b.lo <= data.true_delta_ppm && data.true_delta_ppm <= b.hi {
                        counts.boot_cov += 1;
                    }
                    let hw = (b.hi - b.lo) / 2;
                    boot_halfwidths.push(hw);
                    counts.boot_hw_sum += i128::from(hw);
                }
                // Rate leg.
                if let Some(r) = &rate {
                    counts.rate_n += 1;
                    if r.interval.lo <= data.true_theta_ppm && data.true_theta_ppm <= r.interval.hi
                    {
                        counts.rate_cov += 1;
                    }
                }
                // Family rows.
                let mut any_reject = false;
                for (member, status, raw_p, adj, rejected, _mode) in &family {
                    let e = member_counts.entry(member.clone()).or_insert((0, 0, 0, 0));
                    if status == "computed" {
                        if *rejected {
                            e.0 += 1;
                            any_reject = true;
                        }
                        if let Some(p) = raw_p {
                            if *p <= 25_000 {
                                e.1 += 1;
                            }
                        }
                    } else {
                        e.3 += 1;
                    }
                    let _ = adj;
                }
                for (idx, (member, m)) in nontied.iter().enumerate() {
                    let e = member_counts.entry(member.clone()).or_insert((0, 0, 0, 0));
                    if *m >= 7 {
                        e.2 += 1;
                        if idx == 0 {
                            counts.m7_first += 1;
                        }
                    }
                }
                counts.fam_n += 1;
                if any_reject {
                    counts.fam_rej += 1;
                }
                // Tied leg.
                counts.tied_n += 1;
                let true_best = {
                    // The last comparator is the declared-effect arm —
                    // at Δ > 0 it is the design's best; at Δ = 0 every
                    // arm is exchangeable and the leg records the fact
                    // only.
                    if s.effect_ppm > 0 {
                        Some(last_arm.clone())
                    } else {
                        None
                    }
                };
                if let Some(best) = &true_best {
                    if &tied.0 == best {
                        counts.leader_ok += 1;
                    }
                    counts.leader_eval += 1;
                }
                // Descriptive points (rerun, swap, excess, pass^5,
                // pass@5, per_trial, per_solve).
                for (idx, v) in [points.0, points.1, points.2, points.3, points.4]
                    .iter()
                    .enumerate()
                {
                    if let Some(x) = v {
                        *point_sums.entry(idx).or_default() += i128::from(*x);
                        *point_ns.entry(idx).or_default() += 1;
                    }
                }
                *point_sums.entry(5).or_default() += i128::from(points.5);
                *point_ns.entry(5).or_default() += 1;
                if let Some(x) = points.6 {
                    *point_sums.entry(6).or_default() += i128::from(x);
                    *point_ns.entry(6).or_default() += 1;
                }
            }
            COutcome::Refused(name) => *rep.refused.entry(name.to_string()).or_default() += 1,
            COutcome::GeneratorFailure(detail) => {
                rep.generator_failures += 1;
                if rep.first_failure.is_none() {
                    rep.first_failure = Some(detail);
                }
            }
        }
    }
    rep.wall_ms = start.elapsed().as_millis() as u64;
    let run = rep.datasets_run.max(1);
    rep.availability_ppm = ((u128::from(rep.computed) * PPM as u128) / u128::from(run)) as i64;
    if rep.computed > 0 {
        rep.realized_delta_ppm = Some((counts.delta_sum / i128::from(rep.computed)) as i64);
    }
    if counts.fm_n > 0 {
        rep.fm_coverage_ppm =
            Some(((u128::from(counts.fm_cov) * PPM as u128) / u128::from(counts.fm_n)) as i64);
        rep.fm_width_mean_ppm = Some((counts.fm_wsum / i128::from(counts.fm_n)) as i64);
        rep.fm_width_max_ppm = Some(counts.fm_wmax);
    }
    if counts.boot_n > 0 {
        rep.boot_coverage_ppm =
            Some(((u128::from(counts.boot_cov) * PPM as u128) / u128::from(counts.boot_n)) as i64);
        rep.boot_halfwidth_mean_ppm = Some((counts.boot_hw_sum / i128::from(counts.boot_n)) as i64);
        rep.boot_halfwidth_median_ppm = quantile(&boot_halfwidths, 500_000);
    }
    if counts.rate_n > 0 {
        rep.rate_coverage_ppm =
            Some(((u128::from(counts.rate_cov) * PPM as u128) / u128::from(counts.rate_n)) as i64);
    }
    if counts.fam_n > 0 {
        rep.family_reject_ppm =
            Some(((u128::from(counts.fam_rej) * PPM as u128) / u128::from(counts.fam_n)) as i64);
    }
    rep.member_verdicts = members
        .iter()
        .map(|m| {
            let (rej, le25, m7, unavail) = member_counts.get(m).copied().unwrap_or((0, 0, 0, 0));
            let n = counts.fam_n.max(1);
            (
                m.clone(),
                ((u128::from(rej) * PPM as u128) / u128::from(n)) as i64,
                ((u128::from(le25) * PPM as u128) / u128::from(n)) as i64,
                ((u128::from(m7) * PPM as u128) / u128::from(n)) as i64,
                ((u128::from(unavail) * PPM as u128) / u128::from(n)) as i64,
            )
        })
        .collect();
    rep.m_ge_7_ppm = rep
        .member_verdicts
        .first()
        .map(|(_, _, _, m7, _)| *m7)
        .filter(|_| s.tasks == 10);
    if counts.leader_eval > 0 {
        rep.leader_correct_ppm = Some(
            ((u128::from(counts.leader_ok) * PPM as u128) / u128::from(counts.leader_eval)) as i64,
        );
    }
    rep.points_mean = (0..7usize)
        .map(|i| {
            let n = point_ns.get(&i).copied().unwrap_or(0);
            if n == 0 {
                -1
            } else {
                (point_sums.get(&i).copied().unwrap_or(0) / i128::from(n)) as i64
            }
        })
        .collect();
    // Monte Carlo error over the recorded proportions.
    rep.mc_error_ppm = se_ppm(counts.fm_cov, counts.fm_n)
        .max(se_ppm(counts.boot_cov, counts.boot_n))
        .max(se_ppm(counts.rate_cov, counts.rate_n))
        .max(se_ppm(counts.fam_rej, counts.fam_n))
        .max(se_ppm(rep.computed, run))
        .max(se_ppm(counts.m7_first, counts.fam_n));
    rep
}

#[derive(Default)]
struct Accums {
    delta_sum: i128,
    fm_n: u64,
    fm_cov: u64,
    fm_wsum: i128,
    fm_wmax: i64,
    boot_n: u64,
    boot_cov: u64,
    boot_hw_sum: i128,
    rate_n: u64,
    rate_cov: u64,
    fam_n: u64,
    fam_rej: u64,
    tied_n: u64,
    leader_ok: u64,
    leader_eval: u64,
    m7_first: u64,
}

/// The whole battery record.
#[derive(Debug, Clone)]
pub struct CBatteryReport {
    /// `comparative-battery/1`.
    pub schema: String,
    /// The declared offline envelope this run cites.
    pub envelope: String,
    /// The seed rule (verbatim — the reproduction contract).
    pub seed_rule: String,
    /// The scenario parameterization (the DGP citation).
    pub dgp: String,
    /// One record per frozen cell.
    pub scenarios: Vec<CScenarioReport>,
    /// `"complete"` only when every cell `Executed` at its declared
    /// count with zero generator failures — otherwise
    /// `"incomplete:<ids>"` and the battery never counts as admission.
    pub rollup: String,
}

/// `cb_rollup(scenarios)` — the admission rollup over a retained
/// scenario set (all-or-nothing; no favorable subset).
pub fn cb_rollup(scenarios: Vec<CScenarioReport>) -> CBatteryReport {
    let bad: Vec<String> = scenarios
        .iter()
        .filter(|r| {
            r.status != crate::sim_battery::Status::Executed
                || r.datasets_run < r.datasets_declared
                || r.generator_failures > 0
        })
        .map(|r| r.id.clone())
        .collect();
    CBatteryReport {
        schema: CB_VERSION.to_string(),
        envelope: "offline D2 numerical tranche: <=8h wall, 4 cores/8GiB, 32 core-hours, 2GiB"
            .into(),
        seed_rule: "H(canonical(scenario || comparative-battery/1 || replicate))".into(),
        dgp: "design/35 Appx B DGP: p_t ~ Beta(a,b) clipped [.005,.995]; p_{t,h} = expit(logit(p_t) + beta_h + g_{t,h}), g ~ N(0, sigma^2); beta_h = logit(pbar + delta) - logit(pbar)".into(),
        rollup: if bad.is_empty() {
            "complete".to_string()
        } else {
            format!("incomplete:{}", bad.join(","))
        },
        scenarios,
    }
}

/// `run_cb_battery(cap)` — execute the frozen table.
pub fn run_cb_battery(cap: &crate::sim_battery::Cap) -> CBatteryReport {
    let mut reports = Vec::new();
    for s in cb_scenarios() {
        reports.push(run_cb_scenario(&s, cap));
    }
    cb_rollup(reports)
}

fn opt(v: Option<i64>) -> Json {
    v.map(Json::Int).unwrap_or(Json::Null)
}

impl CScenarioReport {
    /// The integer-canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("id", Json::str(&self.id)),
            ("harnesses", Json::Int(self.harnesses as i64)),
            ("tasks", Json::Int(self.tasks as i64)),
            ("k", Json::Int(self.k as i64)),
            ("mixture", Json::str(&self.mixture)),
            ("sigma_ppm", Json::Int(self.sigma_ppm)),
            ("effect_ppm", Json::Int(self.effect_ppm)),
            ("realized_delta_ppm", opt(self.realized_delta_ppm)),
            ("mechanism", Json::str(&self.mechanism)),
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
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("availability_ppm", Json::Int(self.availability_ppm)),
            ("fm_coverage_ppm", opt(self.fm_coverage_ppm)),
            ("coverage_label", Json::str("diagnostic")),
            ("fm_width_mean_ppm", opt(self.fm_width_mean_ppm)),
            ("fm_width_max_ppm", opt(self.fm_width_max_ppm)),
            ("boot_coverage_ppm", opt(self.boot_coverage_ppm)),
            ("boot_halfwidth_mean_ppm", opt(self.boot_halfwidth_mean_ppm)),
            (
                "boot_halfwidth_median_ppm",
                opt(self.boot_halfwidth_median_ppm),
            ),
            ("rate_coverage_ppm", opt(self.rate_coverage_ppm)),
            (
                "member_verdicts",
                Json::Arr(
                    self.member_verdicts
                        .iter()
                        .map(|(m, rej, le25, m7, unavail)| {
                            Json::obj([
                                ("member", Json::str(m)),
                                ("rejected_ppm", Json::Int(*rej)),
                                ("raw_p_le_025_ppm", Json::Int(*le25)),
                                ("m_ge_7_ppm", Json::Int(*m7)),
                                ("unavailable_ppm", Json::Int(*unavail)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("family_reject_ppm", opt(self.family_reject_ppm)),
            ("m_ge_7_ppm", opt(self.m_ge_7_ppm)),
            ("leader_correct_ppm", opt(self.leader_correct_ppm)),
            ("leader_contested_ppm", opt(self.leader_contested_ppm)),
            (
                "points_mean",
                Json::Arr(self.points_mean.iter().map(|v| Json::Int(*v)).collect()),
            ),
            (
                "points_members",
                Json::Arr(
                    [
                        "rerun",
                        "swap",
                        "excess",
                        "pass_all5",
                        "pass_any5",
                        "per_trial",
                        "per_solve",
                    ]
                    .iter()
                    .map(|s| Json::str(*s))
                    .collect(),
                ),
            ),
            (
                "contrast_refusals",
                Json::Int(self.contrast_refusals as i64),
            ),
            ("mc_error_ppm", Json::Int(self.mc_error_ppm)),
            ("wall_ms", Json::Int(self.wall_ms as i64)),
            (
                "status",
                Json::str(match &self.status {
                    crate::sim_battery::Status::Executed => "executed",
                    crate::sim_battery::Status::Stopped => "stopped",
                    crate::sim_battery::Status::Failed(_) => "failed",
                }),
            ),
        ])
    }
}

impl CBatteryReport {
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
            ("seed_rule", Json::str(&self.seed_rule)),
            ("dgp", Json::str(&self.dgp)),
            ("rollup", Json::str(&self.rollup)),
            (
                "scenarios",
                Json::Arr(self.scenarios.iter().map(|s| s.to_json()).collect()),
            ),
        ])
    }
}
