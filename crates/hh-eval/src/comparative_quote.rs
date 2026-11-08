//! `comparative_quote` — the `comparative-quote/1` artifact
//! (experiments spec §6.7 REQ-XD-050/051, REQ-XH-030; §10.5 AR-13;
//! manifest 271/R6.18 stage 2 D1-a).
//!
//! The single quote artifact serving every paid gate in phases 2–4
//! (tranche, G-L2, G-P4-X, G-P4-C — C-11: no other quote routine
//! exists). A sibling of `exposure_quote`, never an edit (D1 F2).
//!
//! `build_quote` consumes the **supplied** measurement and compute
//! records — every value an external gated input — and yields the
//! deterministic menu: one row per declared gate-passing harness
//! subset at `N = H·T·k`, carrying every REQ-XD-050 column — expected
//! model spend `Σ_h mean_h · T·k · (1 + measured infra allowance)`,
//! the planning ceiling `Σ_h max(mean, P90)_h · T·k · 1.2`, wall and
//! instance hours from measured model-occupied time and queue wait at
//! the declared concurrency, the simulated half-widths/power/
//! P(≥ 1 unresolved cell) read from battery records, and the verdict
//! against the declared precision target (bootstrap half-width
//! ≤ .10, FM ≤ .08).
//!
//! Model lines are computed from the declared measurement records
//! (REQ-XD-043's nuisance fields only). Compute lines are **verbatim
//! citations** of `hh-compute-quote/1` records — priced only by
//! R6.14's §3.10 formula in `receipt_lib.py::quote_doc`; this module
//! re-prices nothing (C-11). Every packet cites AR-13 (`envelope_adr:
//! ADR-0435`); a quote built before provisioned `exposure_quote`
//! quantities exist carries the `pre-quote` label (R6.13, REQ-XD-051),
//! a provisioned one `provisioned:{ref}`.
//!
//! All arithmetic is integer µ$ (R6.7's one rounding rule: exact sum,
//! one ceil per line). The quote computes amounts; it authorizes
//! nothing (X-02). Simulated cells are inputs — labelled fixtures
//! until R6.27's battery records exist and GATE-G23 admits them; an
//! absent simulation is `unresolved{sim_absent}`, never fabricated.
//! No spend, no provider calls — engineered quote machinery only.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::idp::idp_digest;
use hharness_wire::Json;

use crate::population::MethodMetadata;

/// The quote name (the S.1 `estimator` member).
pub const KIND: &str = "comparative_quote";
/// The durable payload version (the spec's spelling, verbatim).
pub const VERSION: &str = "comparative-quote/1";
/// The declared precision target — bootstrap half-width ≤ .10 (ppm).
pub const TARGET_BOOTSTRAP_HW_PPM: u64 = 100_000;
/// The declared precision target — FM half-width ≤ .08 (ppm).
pub const TARGET_FM_HW_PPM: u64 = 80_000;
/// The planning-ceiling factor ×1.2 (ppm of 1.0).
pub const CEILING_FACTOR_PPM: u64 = 1_200_000;
/// The envelope amendment every packet cites (AR-13).
pub const ENVELOPE_ADR: &str = "ADR-0435";
/// The R6.13 pre-quote label (pre-AR-13 provisioned quantities).
pub const LABEL_PRE_QUOTE: &str = "pre-quote";

const DOMAIN: &str = "comparative_quote";

fn ceil_div(num: i128, den: i128) -> i64 {
    debug_assert!(den > 0 && num >= 0);
    ((num + den - 1) / den) as i64
}

// ── declared inputs (records-in) ───────────────────────────────────

/// One arm's declared measurement fields (REQ-XD-043's nuisance set
/// only — mean/P90 $/trial, model-occupied and queue-wait time — read
/// from the cited measurement record; never an outcome field).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmMeasure {
    /// The arm id.
    pub arm_id: String,
    /// The measurement record the fields were read from.
    pub measure_ref: String,
    /// Measured mean $/trial (µ$).
    pub mean_trial_micros: i64,
    /// Measured P90 $/trial (µ$).
    pub p90_trial_micros: i64,
    /// Measured model-occupied time per trial (ms).
    pub model_occupied_ms: u64,
    /// Measured queue wait per trial (ms).
    pub queue_wait_ms: u64,
}

/// The simulated cells — inputs read from battery records (R6.27);
/// labelled fixtures until then and GATE-G23 admits them.
#[derive(Debug, Clone, PartialEq)]
pub struct SimCells {
    /// Simulated bootstrap half-width (ppm).
    pub bootstrap_hw_ppm: u64,
    /// Simulated FM half-width (ppm).
    pub fm_hw_ppm: u64,
    /// Simulated power (ppm of 1.0).
    pub power_ppm: u64,
    /// Simulated P(≥ 1 unresolved cell) (ppm of 1.0).
    pub p_unresolved_ppm: u64,
    /// The record the cells were read from —
    /// `fixture:{label}` (declared fixture) or `battery:{record_ref}`.
    pub source: String,
}

/// One menu row — a gate-passing harness subset priced at `N = H·T·k`.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuRowDecl {
    /// The row's name (the subset identity).
    pub row_id: String,
    /// The subset's arms (H ≥ 1, distinct).
    pub arms: Vec<ArmMeasure>,
    /// The declared measured infra allowance (ppm of 1.0).
    pub infra_allowance_ppm: u64,
    /// The simulated cells (`None` = `unresolved{sim_absent}`).
    pub sim: Option<SimCells>,
}

/// One cited `hh-compute-quote/1` record — echoed verbatim, never
/// recomputed (the only compute pricing path is R6.14's §3.10
/// formula, ADR-0432).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeLine {
    /// The window the record quotes.
    pub window: String,
    /// The `pq-` record id.
    pub quote_id: String,
    /// Its ceiling (µ$).
    pub ceiling_micros: i64,
}

/// The envelope state (AR-13).
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    /// Pre-quote — no provisioned `exposure_quote` quantities yet
    /// (R6.13's label).
    PreQuote,
    /// Provisioned — names the envelope record.
    Provisioned {
        /// The provisioned `exposure_quote` envelope record ref.
        envelope_ref: String,
    },
}

/// The packet declaration (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct ComparativeQuoteDecl {
    /// The tranche's held-out census T.
    pub t: u32,
    /// The declared replicate slots k.
    pub k: u32,
    /// The declared attained concurrency C (REQ-XB-214 inputs are
    /// recorded on the cited compute records; the quote reads the
    /// declared value for the wall-hours cell).
    pub concurrency_c: u64,
    /// The menu rows — one per gate-passing harness subset.
    pub rows: Vec<MenuRowDecl>,
    /// The cited `hh-compute-quote/1` records (non-empty — the packet
    /// carries compute lines, REQ-XH-030).
    pub compute: Vec<ComputeLine>,
    /// The envelope state.
    pub envelope: Envelope,
}

// ── the quote record ───────────────────────────────────────────────

/// One computed menu row.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuRow {
    /// The row's name.
    pub row_id: String,
    /// The subset's arm ids.
    pub arms: Vec<String>,
    /// The measurement refs the cells cite.
    pub measure_refs: Vec<String>,
    /// `H` — the subset size.
    pub h: u32,
    /// `N = H·T·k` trials.
    pub n_trials: u64,
    /// Expected model spend `ceil(Σ_h mean_h·T·k·(1+infra))` (µ$).
    pub expected_micros: i64,
    /// Planning ceiling `ceil(Σ_h max(mean,P90)_h·T·k·1.2)` (µ$).
    pub ceiling_micros: i64,
    /// Wall time `ceil(Σ_h T·k·(occupied+wait)/C)` (ms).
    pub wall_millis: u64,
    /// Instance time — one window instance; the priced figure lives on
    /// the cited compute records (ms).
    pub instance_millis: u64,
    /// The echoed simulated cells (`None` = unresolved).
    pub sim: Option<SimCells>,
    /// The verdict against the declared precision target:
    /// `meets_target` | `below_target{member}` |
    /// `unresolved{sim_absent}`.
    pub verdict: String,
}

/// The `comparative-quote/1` packet — `quote_id` is the `idp_digest`
/// commitment the later paid-gate readouts cite (REQ-XH-030).
#[derive(Debug, Clone, PartialEq)]
pub struct ComparativeQuote {
    /// `H(canonical(record minus quote_id))` under `comparative_quote`.
    pub quote_id: String,
    /// The declared T.
    pub t: u32,
    /// The declared k.
    pub k: u32,
    /// The declared concurrency.
    pub concurrency_c: u64,
    /// The menu rows (the model lines).
    pub rows: Vec<MenuRow>,
    /// The echoed compute lines.
    pub compute: Vec<ComputeLine>,
    /// `Σ compute ceilings` (µ$).
    pub compute_ceiling_micros: i64,
    /// The envelope label (`pre-quote` | `provisioned:{ref}`).
    pub envelope_label: String,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

impl ComparativeQuote {
    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("quote_id");
        }
        self.quote_id == idp_digest(DOMAIN, j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("quote_id", Json::str(&self.quote_id)),
            ("t", Json::Int(self.t as i64)),
            ("k", Json::Int(self.k as i64)),
            ("concurrency_c", Json::Int(self.concurrency_c as i64)),
            (
                "rows",
                Json::Arr(
                    self.rows
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("row_id", Json::str(&r.row_id)),
                                ("arms", Json::Arr(r.arms.iter().map(Json::str).collect())),
                                (
                                    "measure_refs",
                                    Json::Arr(r.measure_refs.iter().map(Json::str).collect()),
                                ),
                                ("h", Json::Int(r.h as i64)),
                                ("n_trials", Json::Int(r.n_trials as i64)),
                                (
                                    "model",
                                    Json::obj([
                                        ("expected_micros", Json::Int(r.expected_micros)),
                                        ("ceiling_micros", Json::Int(r.ceiling_micros)),
                                    ]),
                                ),
                                ("wall_millis", Json::Int(r.wall_millis as i64)),
                                ("instance_millis", Json::Int(r.instance_millis as i64)),
                                (
                                    "sim",
                                    r.sim.as_ref().map_or(Json::Null, |s| {
                                        Json::obj([
                                            (
                                                "bootstrap_hw_ppm",
                                                Json::Int(s.bootstrap_hw_ppm as i64),
                                            ),
                                            ("fm_hw_ppm", Json::Int(s.fm_hw_ppm as i64)),
                                            ("power_ppm", Json::Int(s.power_ppm as i64)),
                                            (
                                                "p_unresolved_ppm",
                                                Json::Int(s.p_unresolved_ppm as i64),
                                            ),
                                            ("source", Json::str(&s.source)),
                                        ])
                                    }),
                                ),
                                ("verdict", Json::str(&r.verdict)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "compute",
                Json::Arr(
                    self.compute
                        .iter()
                        .map(|c| {
                            Json::obj([
                                ("window", Json::str(&c.window)),
                                ("quote_id", Json::str(&c.quote_id)),
                                ("ceiling_micros", Json::Int(c.ceiling_micros)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "compute_ceiling_micros",
                Json::Int(self.compute_ceiling_micros),
            ),
            ("envelope_label", Json::str(&self.envelope_label)),
            ("envelope_adr", Json::str(ENVELOPE_ADR)),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

// ── the refusal set ────────────────────────────────────────────────

/// The closed refusal set — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum ComparativeQuoteError {
    /// A required member is absent/zero.
    MissingKey { member: String },
    /// A row carries no arms.
    EmptyRow { row_id: String },
    /// A row id declared twice.
    DuplicateRow { row_id: String },
    /// An arm appears twice in one row.
    DuplicateArm { row_id: String, arm_id: String },
    /// A negative declared price — malformed.
    NegativePrice { row_id: String, arm_id: String },
    /// A measurement field lacks its record ref.
    MissingMeasureRef { arm_id: String },
    /// The simulated cells lack their source record spelling.
    MissingSimSource { row_id: String },
    /// A compute line lacks a member — the packet cites complete
    /// `hh-compute-quote/1` records only.
    ComputeLineMissing { member: &'static str },
    /// No compute lines — REQ-XH-030 requires both model and compute
    /// lines.
    NoComputeLines,
    /// A `provisioned` envelope without its record ref.
    MissingEnvelopeRef,
}

impl std::fmt::Display for ComparativeQuoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ComparativeQuoteError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::EmptyRow { row_id } => write!(f, "menu row {row_id} carries no arms"),
            E::DuplicateRow { row_id } => write!(f, "menu row {row_id} declared twice"),
            E::DuplicateArm { row_id, arm_id } => {
                write!(f, "arm {arm_id} twice in row {row_id}")
            }
            E::NegativePrice { row_id, arm_id } => {
                write!(f, "negative price on {arm_id} in row {row_id}")
            }
            E::MissingMeasureRef { arm_id } => {
                write!(f, "arm {arm_id} lacks its measurement record ref")
            }
            E::MissingSimSource { row_id } => {
                write!(f, "row {row_id} sim cells lack a source record")
            }
            E::ComputeLineMissing { member } => {
                write!(f, "compute line member {member} absent")
            }
            E::NoComputeLines => {
                write!(f, "no compute lines — model and compute both required")
            }
            E::MissingEnvelopeRef => write!(f, "provisioned envelope lacks its record ref"),
        }
    }
}

impl std::error::Error for ComparativeQuoteError {}

// ── the builder ────────────────────────────────────────────────────

/// `build_quote(decl)` — validate the supplied measurement/compute
/// records and emit the deterministic `comparative-quote/1` packet.
/// Pure and total: same inputs ⇒ byte-identical packet and `quote_id`.
pub fn build_quote(decl: &ComparativeQuoteDecl) -> Result<ComparativeQuote, ComparativeQuoteError> {
    use ComparativeQuoteError as E;

    if decl.t == 0 {
        return Err(E::MissingKey {
            member: "t".to_string(),
        });
    }
    if decl.k == 0 {
        return Err(E::MissingKey {
            member: "k".to_string(),
        });
    }
    if decl.concurrency_c == 0 {
        return Err(E::MissingKey {
            member: "concurrency_c".to_string(),
        });
    }
    if decl.rows.is_empty() {
        return Err(E::MissingKey {
            member: "rows".to_string(),
        });
    }
    if decl.compute.is_empty() {
        return Err(E::NoComputeLines);
    }
    for c in &decl.compute {
        if c.window.is_empty() {
            return Err(E::ComputeLineMissing { member: "window" });
        }
        if c.quote_id.is_empty() {
            return Err(E::ComputeLineMissing { member: "quote_id" });
        }
        if c.ceiling_micros < 0 {
            return Err(E::ComputeLineMissing {
                member: "ceiling_micros",
            });
        }
    }
    let envelope_label = match &decl.envelope {
        Envelope::PreQuote => LABEL_PRE_QUOTE.to_string(),
        Envelope::Provisioned { envelope_ref } => {
            if envelope_ref.is_empty() {
                return Err(E::MissingEnvelopeRef);
            }
            format!("provisioned:{envelope_ref}")
        }
    };

    let tk = (decl.t as i128) * (decl.k as i128);
    let mut rows: Vec<MenuRow> = Vec::with_capacity(decl.rows.len());
    let mut row_ids: BTreeSet<&str> = BTreeSet::new();
    for r in &decl.rows {
        if r.row_id.is_empty() || !row_ids.insert(r.row_id.as_str()) {
            return Err(E::DuplicateRow {
                row_id: r.row_id.clone(),
            });
        }
        if r.arms.is_empty() {
            return Err(E::EmptyRow {
                row_id: r.row_id.clone(),
            });
        }
        let mut arm_ids: BTreeSet<&str> = BTreeSet::new();
        let mut sum_mean: i128 = 0;
        let mut sum_max: i128 = 0;
        let mut sum_ms: i128 = 0;
        for a in &r.arms {
            if a.arm_id.is_empty() || !arm_ids.insert(a.arm_id.as_str()) {
                return Err(E::DuplicateArm {
                    row_id: r.row_id.clone(),
                    arm_id: a.arm_id.clone(),
                });
            }
            if a.measure_ref.is_empty() {
                return Err(E::MissingMeasureRef {
                    arm_id: a.arm_id.clone(),
                });
            }
            if a.mean_trial_micros < 0 || a.p90_trial_micros < 0 {
                return Err(E::NegativePrice {
                    row_id: r.row_id.clone(),
                    arm_id: a.arm_id.clone(),
                });
            }
            sum_mean += a.mean_trial_micros as i128;
            sum_max += (a.mean_trial_micros.max(a.p90_trial_micros)) as i128;
            sum_ms += (a.model_occupied_ms + a.queue_wait_ms) as i128;
        }
        if let Some(s) = &r.sim {
            if !(s.source.starts_with("fixture:") || s.source.starts_with("battery:")) {
                return Err(E::MissingSimSource {
                    row_id: r.row_id.clone(),
                });
            }
        }
        let h = r.arms.len() as u128;
        let n_trials = (h as u64) * (decl.t as u64) * (decl.k as u64);
        // Expected model spend: Σ_h mean_h · T·k · (1 + infra) — one
        // ceil per line (R6.7's rounding rule).
        let expected = ceil_div(
            sum_mean * tk * (1_000_000i128 + r.infra_allowance_ppm as i128),
            1_000_000,
        );
        // Planning ceiling: Σ_h max(mean, P90)_h · T·k · 1.2.
        let ceiling = ceil_div(sum_max * tk * (CEILING_FACTOR_PPM as i128), 1_000_000);
        // Wall: Σ_h T·k·(occupied + wait) at the declared concurrency.
        let wall = ceil_div(sum_ms * tk, decl.concurrency_c as i128) as u64;
        let verdict = match &r.sim {
            None => "unresolved{sim_absent}".to_string(),
            Some(s) if s.bootstrap_hw_ppm > TARGET_BOOTSTRAP_HW_PPM => {
                "below_target{bootstrap_hw}".to_string()
            }
            Some(s) if s.fm_hw_ppm > TARGET_FM_HW_PPM => "below_target{fm_hw}".to_string(),
            Some(_) => "meets_target".to_string(),
        };
        rows.push(MenuRow {
            row_id: r.row_id.clone(),
            arms: r.arms.iter().map(|a| a.arm_id.clone()).collect(),
            measure_refs: r.arms.iter().map(|a| a.measure_ref.clone()).collect(),
            h: h as u32,
            n_trials,
            expected_micros: expected,
            ceiling_micros: ceiling,
            wall_millis: wall,
            instance_millis: wall,
            sim: r.sim.clone(),
            verdict,
        });
    }

    let compute_ceiling: i64 = decl.compute.iter().map(|c| c.ceiling_micros).sum();

    let metadata = MethodMetadata {
        estimator: KIND.to_string(),
        version: VERSION.to_string(),
        target: "comparative tranche menu quote (model + compute lines)".into(),
        assumptions: vec![
            "model lines recompute from declared measurement records only".into(),
            "compute lines are verbatim hh-compute-quote/1 citations (§3.10, R6.14)".into(),
            "simulated cells are declared inputs read from battery records".into(),
            "the quote computes amounts; it authorizes nothing".into(),
        ],
        prior: None,
        keys: BTreeMap::from([
            ("t".to_string(), decl.t.to_string()),
            ("k".to_string(), decl.k.to_string()),
            ("envelope_adr".to_string(), ENVELOPE_ADR.to_string()),
        ]),
        stratum_membership: vec![],
        seed: 0,
        draws_requested: 0,
        draws_used: 0,
        floors: vec![
            format!("bootstrap_hw_ppm<={TARGET_BOOTSTRAP_HW_PPM}"),
            format!("fm_hw_ppm<={TARGET_FM_HW_PPM}"),
        ],
        interval: ("descriptive".into(), None),
        refusal: None,
    };

    let mut out = ComparativeQuote {
        quote_id: String::new(),
        t: decl.t,
        k: decl.k,
        concurrency_c: decl.concurrency_c,
        rows,
        compute: decl.compute.clone(),
        compute_ceiling_micros: compute_ceiling,
        envelope_label,
        metadata,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("quote_id");
    }
    out.quote_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
