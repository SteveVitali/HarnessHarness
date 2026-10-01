//! The three benefit kinds + `equivalence_run` (spec §5h.2 §2.1; ADR-0046
//! D4/D5; ADR-0143 L3; R-2.9.2; S3.3).
//!
//! Each benefit kind is a `compare` under a stricter precondition set:
//!
//! - **`artifact_benefit`** — paired Δ of the *frozen* artifact on held-out
//!   splits only; evaluation-phase search spend must be `0` on every run; the
//!   pre-registration's `task_split_hash` must predate the arm's first
//!   `measurement.evolution.candidate.transitioned{to: proposed}` (`LeakedSplit`
//!   — AC-R-2.9.4-8). The report carries `benefit_kind = artifact_benefit`,
//!   `held_out = true`.
//! - **`search_time_benefit`** — performance during search vs a baseline
//!   realized as `oracle_best_of_n` (labelled upper bound) or
//!   `selected_best_of_n` when a selector was recorded; the four-stage
//!   realized-benefit decomposition is reported (`estimated` member) with
//!   `n/a{observability}` on judged stages. `UnmatchedSearchBudget` when the
//!   arms' search budgets are absent.
//! - **`transfer`** — paired Δ on a held-out level of a declared factor
//!   (`model_snapshot` | `environment` | `task_family`); `transfer_ratio` is
//!   derived and reported in the report's `estimated` member.
//! - **`equivalence_run`** — a `compare` with a pre-registered per-dimension
//!   margin producing the three-valued verdict `{equivalent, not_equivalent,
//!   inconclusive}`; margins come from `PreRegistration` **only** — a
//!   caller-supplied margin is refused (OQ-113).

use std::collections::BTreeMap;

use hh_ontology::eval::PreRegistration;
use hh_ontology::lab::SplitLabel;
use hh_wire::Json;

use hh_lab::analysis::{BenefitKind, ComparisonReport};

use crate::compare::{compare, CompareError, CompareInput, CompareOutcome};

/// The benefit/equivalence typed refusals.
#[derive(Debug, Clone, PartialEq)]
pub enum BenefitError {
    /// The underlying compare refused.
    Compare(CompareError),
    /// `artifact_benefit` over a non-held-out split.
    NotHeldOut {
        /// The offending task.
        task_id: String,
    },
    /// `artifact_benefit` with nonzero evaluation-phase search spend.
    SearchSpendInEval {
        /// The offending run.
        run_id: String,
    },
    /// The split hash postdates the first `transitioned{to: proposed}` — the
    /// split was assigned after the search began (AC-R-2.9.4-8).
    LeakedSplit {
        /// The first `proposed` transition seq.
        first_proposed_at: u64,
        /// The split's registration seq.
        split_registered_at: u64,
    },
    /// `search_time_benefit` without a complete search budget.
    UnmatchedSearchBudget {
        /// The detail.
        detail: String,
    },
    /// `transfer` over a factor this operation does not admit.
    BadTransferFactor {
        /// The factor spelling.
        factor: String,
    },
    /// `equivalence_run` margins not drawn from the pre-registration.
    MarginNotPreRegistered,
    /// The pre-registration carries no margins.
    NoRegisteredMargins,
    /// The margins name a dimension the comparison did not produce.
    MarginDimensionMissing {
        /// The dimension.
        dimension: String,
    },
    /// A run's `split_hash` disagrees with the pre-registered hash (L3).
    SplitHashMismatch {
        /// The offending run.
        run_id: String,
    },
}

impl std::fmt::Display for BenefitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenefitError::Compare(e) => write!(f, "{e}"),
            BenefitError::NotHeldOut { task_id } => {
                write!(f, "artifact_benefit over non-held-out task {task_id}")
            }
            BenefitError::SearchSpendInEval { run_id } => {
                write!(f, "eval-phase search spend on {run_id}")
            }
            BenefitError::LeakedSplit {
                first_proposed_at,
                split_registered_at,
            } => write!(
                f,
                "LeakedSplit: split registered at {split_registered_at} after first proposal at {first_proposed_at}"
            ),
            BenefitError::UnmatchedSearchBudget { detail } => {
                write!(f, "UnmatchedSearchBudget: {detail}")
            }
            BenefitError::BadTransferFactor { factor } => {
                write!(f, "bad transfer factor: {factor}")
            }
            BenefitError::MarginNotPreRegistered => {
                write!(f, "equivalence margins must come from the pre-registration")
            }
            BenefitError::NoRegisteredMargins => {
                write!(f, "the pre-registration carries no equivalence margins")
            }
            BenefitError::MarginDimensionMissing { dimension } => {
                write!(f, "no comparison produced for margin dimension {dimension}")
            }
            BenefitError::SplitHashMismatch { run_id } => {
                write!(f, "split hash disagrees with the pre-registration: {run_id}")
            }
        }
    }
}

impl std::error::Error for BenefitError {}

impl From<CompareError> for BenefitError {
    fn from(e: CompareError) -> BenefitError {
        BenefitError::Compare(e)
    }
}

/// The `search_time_benefit` baseline realization (ADR-0159 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineRealization {
    /// `oracle_best_of_n` — pass@N of the baseline replicates, an upper bound.
    OracleBestOfN,
    /// `selected_best_of_n` — a recorded selector's pick.
    SelectedBestOfN,
}

/// `artifact_benefit(arm, baseline_arm, held_out_split)` — §5h.2 §2.1 D4.
///
/// Preconditions (all refusal, never warning): every run is on a `held_out`/
/// `private` split task; `eval_search_spend = 0` on every run; the
/// `SplitAssignmentRecord`'s `registered_at` precedes the earliest
/// `transitioned{to: proposed}` in either arm's run facts (the split hash
/// predates the search).
pub fn artifact_benefit(
    input: &CompareInput,
    split_registered_at: u64,
) -> Result<CompareOutcome, BenefitError> {
    for r in input.runs {
        if r.arm_id != input.arm_a && r.arm_id != input.arm_b {
            continue;
        }
        if !matches!(r.split_label, SplitLabel::HeldOut | SplitLabel::Private) {
            return Err(BenefitError::NotHeldOut {
                task_id: r.task_id.clone(),
            });
        }
        if r.eval_search_spend != 0 {
            return Err(BenefitError::SearchSpendInEval {
                run_id: r.run_id.clone(),
            });
        }
        if let Some(at) = r.facts.first_proposed_at {
            if split_registered_at > at {
                return Err(BenefitError::LeakedSplit {
                    first_proposed_at: at,
                    split_registered_at,
                });
            }
        }
    }
    let mut held = input.clone();
    held.benefit_kind = BenefitKind::ArtifactBenefit;
    held.held_out = true;
    compare(&held).map_err(BenefitError::Compare)
}

/// `search_time_benefit(arm, baseline_arm)` — §5h.2 §2.1 D4 as amended.
/// Requires a complete search budget on both arms (`ArmSpec.search_budget`);
/// the baseline realization is recorded on the report's `estimated` member
/// alongside the four-stage realized-benefit decomposition.
pub fn search_time_benefit(
    input: &CompareInput,
    baseline: BaselineRealization,
    // `(name, value_ppm | None)` — the four decomposition stages; `None`
    // renders `n/a{observability}` (a judged stage, ADR-0014).
    decomposition: &[(String, Option<i64>)],
) -> Result<CompareOutcome, BenefitError> {
    for (i, spec) in input.arm_specs.iter().enumerate() {
        if spec.search_budget.is_none() {
            return Err(BenefitError::UnmatchedSearchBudget {
                detail: format!("arm {i} carries no search_budget"),
            });
        }
    }
    let mut st = input.clone();
    st.benefit_kind = BenefitKind::SearchTimeBenefit;
    let mut out = compare(&st).map_err(BenefitError::Compare)?;
    for r in &mut out.reports {
        r.estimated = Some(Json::obj([
            (
                "baseline_realization",
                Json::str(match baseline {
                    BaselineRealization::OracleBestOfN => "oracle_best_of_n",
                    BaselineRealization::SelectedBestOfN => "selected_best_of_n",
                }),
            ),
            (
                "benefit_decomposition",
                Json::Arr(
                    decomposition
                        .iter()
                        .map(|(stage, v)| {
                            Json::obj([
                                ("stage", Json::str(stage)),
                                (
                                    "value",
                                    v.map_or(
                                        Json::obj([("n/a", Json::str("observability"))]),
                                        Json::Int,
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]));
    }
    Ok(out)
}

/// `transfer(arm, baseline_arm, held_out_factor)` — §5h.2 §2.1: the paired Δ
/// runs on the held-out level; `transfer_ratio` (`Δ_held_out / Δ_main`) is
/// reported in `estimated` when a main-plane delta is supplied.
pub fn transfer(
    input: &CompareInput,
    held_out_factor: &str,
    // The main-plane point delta (for `transfer_ratio`), when a paired main
    // comparison exists.
    main_delta: Option<i64>,
) -> Result<CompareOutcome, BenefitError> {
    if !matches!(
        held_out_factor,
        "model_snapshot" | "environment" | "task_family"
    ) {
        return Err(BenefitError::BadTransferFactor {
            factor: held_out_factor.into(),
        });
    }
    let mut t = input.clone();
    t.benefit_kind = BenefitKind::Transfer;
    t.held_out = true;
    let mut out = compare(&t).map_err(BenefitError::Compare)?;
    for r in &mut out.reports {
        let mut est = BTreeMap::new();
        est.insert("held_out_factor".into(), Json::str(held_out_factor));
        if let (Some(main), Some(Json::Int(hd))) = (main_delta, r.paired_effect.point.as_ref()) {
            if main != 0 {
                est.insert(
                    "transfer_ratio_ppm".into(),
                    Json::Int(((*hd as i128 * 1_000_000i128) / main as i128) as i64),
                );
            }
        }
        r.estimated = Some(Json::Obj(est));
    }
    Ok(out)
}

// ── equivalence_run ──────────────────────────────────────────────────────────

/// `equivalence_report/1` — the `equivalence_run` output (spec §5h.2 §2.1;
/// ADR-0046 D5; ADR-0047 D7's `reference_relative` three-valued verdict).
#[derive(Debug, Clone, PartialEq)]
pub struct EquivalenceReport {
    /// The reference participant ref.
    pub reference: String,
    /// The candidate arm ref.
    pub candidate: String,
    /// The suite ref.
    pub suite_ref: String,
    /// The per-dimension verdicts `{dimension, margin, delta, interval,
    /// verdict}`.
    pub per_dimension: Vec<DimensionVerdict>,
    /// The overall verdict — `equivalent` iff every dimension's interval sits
    /// inside the margin; `not_equivalent` iff any interval is entirely
    /// outside; `inconclusive` otherwise (a verdict never narrows).
    pub verdict: EquivalenceVerdict,
    /// The underlying comparison reports.
    pub comparison_refs: Vec<String>,
    /// The pre-registration ref the margins came from.
    pub pre_registration_ref: String,
}

/// One dimension's equivalence check.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionVerdict {
    /// The scorecard dimension/metric name.
    pub dimension: String,
    /// The pre-registered margin (ppm for rates; relative ppm for
    /// cost/latency — `margin_kind` records which).
    pub margin_ppm: i64,
    /// `absolute` (pp, for rates) | `relative` (ppm of the reference level,
    /// for cost/latency).
    pub margin_kind: String,
    /// The paired-delta point.
    pub point: Option<i64>,
    /// The paired-delta interval `[lo, hi]`.
    pub interval: Option<(i64, i64)>,
    /// The per-dimension verdict.
    pub verdict: EquivalenceVerdict,
}

/// The three-valued verdict (ADR-0047 D7 — `reference_relative`'s
/// `verdict_type = three_valued`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquivalenceVerdict {
    /// `equivalent` — every dimension's interval is inside the margin.
    Equivalent,
    /// `not_equivalent` — a dimension's interval is entirely outside.
    NotEquivalent,
    /// `inconclusive` — an interval straddles the margin.
    Inconclusive,
}

impl EquivalenceVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EquivalenceVerdict::Equivalent => "equivalent",
            EquivalenceVerdict::NotEquivalent => "not_equivalent",
            EquivalenceVerdict::Inconclusive => "inconclusive",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<EquivalenceVerdict> {
        match s {
            "equivalent" => Some(EquivalenceVerdict::Equivalent),
            "not_equivalent" => Some(EquivalenceVerdict::NotEquivalent),
            "inconclusive" => Some(EquivalenceVerdict::Inconclusive),
            _ => None,
        }
    }
}

/// One pre-registered margin — `{dimension, margin_ppm, kind}`; decoded from
/// `PreRegistration.equivalence_margin`'s canonical form
/// `{margins: [{dimension, margin_ppm, kind ∈ {absolute, relative}}]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredMargin {
    /// The dimension/metric the margin binds.
    pub dimension: String,
    /// The margin (ppm for `absolute`; ppm-of-reference for `relative`).
    pub margin_ppm: i64,
    /// `absolute` | `relative`.
    pub kind: String,
}

/// Decode `PreRegistration.equivalence_margin` → margins (the only margin
/// source — OQ-113).
pub fn registered_margins(prereg: &PreRegistration) -> Result<Vec<RegisteredMargin>, BenefitError> {
    let j = prereg
        .equivalence_margin
        .as_ref()
        .ok_or(BenefitError::NoRegisteredMargins)?;
    let arr = j
        .get("margins")
        .and_then(|m| match m {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .ok_or(BenefitError::NoRegisteredMargins)?;
    let mut out = Vec::new();
    for m in arr {
        out.push(RegisteredMargin {
            dimension: m
                .get("dimension")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            margin_ppm: m.get("margin_ppm").and_then(Json::as_int).unwrap_or(0),
            kind: m
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("absolute")
                .to_string(),
        });
    }
    if out.is_empty() {
        return Err(BenefitError::NoRegisteredMargins);
    }
    Ok(out)
}

/// `equivalence_run` — a `compare` under pre-registered margins producing the
/// three-valued verdict. `caller_margins` must be `None` — margins come only
/// from `prereg` (OQ-113); a `Some` is refused (`MarginNotPreRegistered`).
/// Unequal budgets refuse through `compare`'s `validate_match` (UnmatchedBudget).
pub fn equivalence_run(
    reference: &str,
    candidate: &str,
    suite_ref: &str,
    prereg: &PreRegistration,
    input: &CompareInput,
    caller_margins: Option<&[RegisteredMargin]>,
) -> Result<EquivalenceReport, BenefitError> {
    equivalence_run_full(
        reference,
        candidate,
        suite_ref,
        prereg,
        input,
        caller_margins,
    )
    .map(|(report, _)| report)
}

/// `equivalence_run_full` — [`equivalence_run`] plus the underlying
/// `CompareOutcome` (the `comparison_refs` on the report hash exactly these
/// reports — the §6.4 A8 kernel needs them in the report body, never
/// recomputed; additive at S3.4c).
pub fn equivalence_run_full(
    reference: &str,
    candidate: &str,
    suite_ref: &str,
    prereg: &PreRegistration,
    input: &CompareInput,
    caller_margins: Option<&[RegisteredMargin]>,
) -> Result<(EquivalenceReport, CompareOutcome), BenefitError> {
    if caller_margins.is_some() {
        return Err(BenefitError::MarginNotPreRegistered);
    }
    let margins = registered_margins(prereg)?;
    let out = compare(input).map_err(BenefitError::Compare)?;
    let by_metric: BTreeMap<&str, &ComparisonReport> =
        out.reports.iter().map(|r| (r.metric.as_str(), r)).collect();
    let mut dims = Vec::new();
    let mut overall = EquivalenceVerdict::Equivalent;
    for m in &margins {
        let report = by_metric.get(m.dimension.as_str()).ok_or_else(|| {
            BenefitError::MarginDimensionMissing {
                dimension: m.dimension.clone(),
            }
        })?;
        let interval = report.paired_effect.interval.as_ref().and_then(|i| {
            let lo = i.get("lo").and_then(Json::as_int)?;
            let hi = i.get("hi").and_then(Json::as_int)?;
            Some((lo, hi))
        });
        let verdict = match interval {
            Some((lo, hi)) => {
                let (m_lo, m_hi) = (-m.margin_ppm, m.margin_ppm);
                if lo >= m_lo && hi <= m_hi {
                    EquivalenceVerdict::Equivalent
                } else if hi < m_lo || lo > m_hi {
                    EquivalenceVerdict::NotEquivalent
                } else {
                    EquivalenceVerdict::Inconclusive
                }
            }
            None => EquivalenceVerdict::Inconclusive,
        };
        overall = match (overall, verdict) {
            (EquivalenceVerdict::NotEquivalent, _) | (_, EquivalenceVerdict::NotEquivalent) => {
                EquivalenceVerdict::NotEquivalent
            }
            (EquivalenceVerdict::Inconclusive, _) | (_, EquivalenceVerdict::Inconclusive) => {
                EquivalenceVerdict::Inconclusive
            }
            _ => EquivalenceVerdict::Equivalent,
        };
        dims.push(DimensionVerdict {
            dimension: m.dimension.clone(),
            margin_ppm: m.margin_ppm,
            margin_kind: m.kind.clone(),
            point: report.paired_effect.point.as_ref().and_then(Json::as_int),
            interval,
            verdict,
        });
    }
    Ok((
        EquivalenceReport {
            reference: reference.into(),
            candidate: candidate.into(),
            suite_ref: suite_ref.into(),
            per_dimension: dims,
            verdict: overall,
            comparison_refs: out
                .reports
                .iter()
                .map(|r| {
                    hh_identity::idp_id(
                        "eval.comparison_report",
                        r.to_json().to_canonical_string().as_bytes(),
                    )
                })
                .collect(),
            pre_registration_ref: prereg.analysis_plan_ref.clone(),
        },
        out,
    ))
}

impl EquivalenceReport {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("equivalence_report/1")),
            ("reference", Json::str(&self.reference)),
            ("candidate", Json::str(&self.candidate)),
            ("suite_ref", Json::str(&self.suite_ref)),
            ("verdict", Json::str(self.verdict.as_str())),
            (
                "per_dimension",
                Json::Arr(
                    self.per_dimension
                        .iter()
                        .map(|d| {
                            Json::obj([
                                ("dimension", Json::str(&d.dimension)),
                                ("margin_ppm", Json::Int(d.margin_ppm)),
                                ("margin_kind", Json::str(&d.margin_kind)),
                                ("point", d.point.map_or(Json::Null, Json::Int)),
                                (
                                    "interval",
                                    d.interval.map_or(Json::Null, |(lo, hi)| {
                                        Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))])
                                    }),
                                ),
                                ("verdict", Json::str(d.verdict.as_str())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "comparison_refs",
                Json::Arr(self.comparison_refs.iter().map(Json::str).collect()),
            ),
            (
                "pre_registration_ref",
                Json::str(&self.pre_registration_ref),
            ),
        ])
    }
}

/// The split-hash staleness check used by `artifact_benefit` at suite scope —
/// `split_hash`s present on runs must agree with the pre-registered hash.
pub fn split_hash_agrees(
    prereg: &PreRegistration,
    runs: &[crate::runs::EvalRun],
) -> Result<(), BenefitError> {
    // Every declared split hash must equal the pre-registered one — a run on a
    // different split is a contamination violation (L3), never skipped.
    for r in runs {
        if let Some(h) = &r.split_hash {
            if h != &prereg.task_split_hash {
                return Err(BenefitError::SplitHashMismatch {
                    run_id: r.run_id.clone(),
                });
            }
        }
    }
    Ok(())
}
