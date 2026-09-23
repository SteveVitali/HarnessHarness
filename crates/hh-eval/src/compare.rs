//! Matched-budget `compare` (spec §5h.2 §2.1; ADR-0046 D2/D3/D5; R-2.9.2;
//! S3.3).
//!
//! `compare` is matched-budget-or-refuse (CC9): the *declared* match rules
//! live in `hh_budget::matchspec::validate_match` — the single source
//! (CC1) — and this module adds the §5h.2 runtime preconditions on top:
//!
//! - task set + split labels identical across arms;
//! - environment image, fault profile and perturbation profile identical
//!   unless that field is the varied factor;
//! - per-role model snapshots identical unless `model` is varied;
//! - realised cache state uniform across arms (mixed warmth refused —
//!   ADR-0041 as amended);
//! - every run `comparable` (an exploratory/non-comparable run refuses the
//!   comparison — it is never silently dropped);
//! - pairing by task; by replicate only where every run declares
//!   `seed_honoured` (CF-095);
//! - `n ≥ design.replicates_per_cell` per task — short cells render
//!   `n/a{not_run}` (counted, never dropped);
//! - consumption medians beyond the tolerance yield
//!   `budget_match.status = imbalanced` (reported, never refused);
//! - every claim is a labelled `ComparisonReport`.
//!
//! Non-scored outcome classes never enter the metric's denominator unless the
//! declaration's `outcome_class_policy` admits them — and are counted beside,
//! never coerced to 0.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_budget::errors::MatchError;
use hh_budget::matchspec::{ArmSpec, MatchMode};
use hh_ontology::compliance::{MetricDeclaration, NaReason};
use hh_ontology::control::OutcomeClass;
use hh_ontology::eval::{
    BootstrapPairedMethod, Design, EstimatorSelection, IntervalMethod, LatticeValue,
    MetricValueKind, Pairing, ReplicateReducer,
};
use hh_ontology::lab::{ContaminationStratum, SplitLabel};
use hh_wire::Json;

use hh_lab::analysis::{
    BenefitKind, BudgetMatch, BudgetMatchStatus, ComparisonReport, Multiplicity, OutcomeBounds,
    OutcomeBoundsVerdict, PairedEffect, ReportLabelKind, SignProfile, TailEffects, TestKind,
    TestRecord,
};

use crate::runs::{EvalRun, TaskContext};
use crate::stats;

/// The OQ-124 MUST-data default consumption-imbalance tolerance (10 %, ppm).
pub const DEFAULT_IMBALANCE_TOLERANCE_PPM: u64 = 100_000;

/// The comparison's typed refusals (never warnings — CC9/T-LCD-14).
#[derive(Debug, Clone, PartialEq)]
pub enum CompareError {
    /// `hh_budget::matchspec::validate_match` refused (MissingMatchSpec,
    /// UnbudgetedArm, IncommensurableMatch, MissingPricingTable).
    Match(MatchError),
    /// A run in the comparison is marked non-comparable (exploratory).
    ExploratoryRun {
        /// The offending run.
        run_id: String,
    },
    /// A compatibility field differs across arms without being the varied
    /// factor (`{field, detail}`).
    IncompatibleArms {
        /// The field (`task_set`, `split`, `environment`, `fault_profile`,
        /// `perturbation_profile`, `model_snapshot`, `cache_state`, `stratum`).
        field: String,
        /// The detail.
        detail: String,
    },
    /// A named metric is not in the resolved declarations.
    UnknownMetric {
        /// The metric name.
        name: String,
    },
    /// An arm has no runs at all.
    NoRuns {
        /// The arm id.
        arm_id: String,
    },
    /// The design's pairing is unsatisfiable (`by_task_and_replicate`
    /// declared but a run lacks `seed_honoured`).
    SeedNotHonoured {
        /// The offending run.
        run_id: String,
    },
    /// `contrast` could not pair a task across both models (a typed
    /// refusal — never a zero-filled interaction).
    ContrastUndefined {
        /// The detail.
        detail: String,
    },
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareError::Match(e) => write!(f, "match refused: {:?}", e.refusal),
            CompareError::ExploratoryRun { run_id } => {
                write!(f, "non-comparable run in comparison: {run_id}")
            }
            CompareError::IncompatibleArms { field, detail } => {
                write!(f, "incompatible arms: {field}: {detail}")
            }
            CompareError::UnknownMetric { name } => write!(f, "unknown metric: {name}"),
            CompareError::NoRuns { arm_id } => write!(f, "no runs for arm {arm_id}"),
            CompareError::SeedNotHonoured { run_id } => {
                write!(f, "by_task_and_replicate requires seed_honoured: {run_id}")
            }
            CompareError::ContrastUndefined { detail } => {
                write!(f, "contrast undefined: {detail}")
            }
        }
    }
}

impl std::error::Error for CompareError {}

/// `compare`'s input (records-in — the eval-plane projection the op carries).
#[derive(Clone)]
pub struct CompareInput<'a> {
    /// Arm A's id.
    pub arm_a: &'a str,
    /// Arm B's id.
    pub arm_b: &'a str,
    /// The metrics to compare (resolved against `declarations`).
    pub metrics: &'a [String],
    /// The metric declarations (the catalogue rows — `compare` never resolves
    /// names itself).
    pub declarations: &'a [MetricDeclaration],
    /// The runs (any arms — filtered by `arm_a`/`arm_b`).
    pub runs: &'a [EvalRun],
    /// The task contexts (the suite plane's rows).
    pub tasks: &'a [TaskContext],
    /// The pinned design (`pairing`, `replicates_per_cell`, `seed_policy`).
    pub design: &'a Design,
    /// The two arms' `ArmSpec`s in `[a, b]` order (validate_match input).
    pub arm_specs: &'a [ArmSpec],
    /// The factor allowed to differ across arms (`model` | `environment` |
    /// `fault_profile` | `perturbation_profile` | `task_family`); `None` =
    /// nothing may differ.
    pub varied_factor: Option<&'a str>,
    /// The interval confidence (ppm; `950_000` = 95 %).
    pub confidence_ppm: i64,
    /// The comparison's benefit kind.
    pub benefit_kind: BenefitKind,
    /// Whether the comparison ran over held-out splits.
    pub held_out: bool,
    /// The family size for `multiplicity` (defaults to `metrics.len()`).
    pub family_size: Option<u32>,
}

/// One per-task effect row (`{task_id, a, b, delta | n/a{reason}, c, n}`).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEffect {
    /// The task id.
    pub task_id: String,
    /// Arm A's per-task value (`n/a{reason}` typed, never 0).
    pub a: MetricValueKind,
    /// Arm B's per-task value.
    pub b: MetricValueKind,
    /// The paired delta (`b − a` is **not** the convention — `a − b`: a
    /// positive delta means arm A reads higher; direction is the metric's).
    pub delta: Option<i64>,
    /// The typed `n/a` when the cell is unpaired.
    pub na: Option<NaReason>,
    /// The `(c, n)` success counts per arm — `((c_a, n_a), (c_b, n_b))`;
    /// pass^k/pass@k stay recomputable from the report (spec §5h.2 AC-8).
    pub counts: ((u64, u64), (u64, u64)),
}

impl TaskEffect {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("task_id".into(), Json::str(&self.task_id));
        m.insert("a".into(), self.a.to_json());
        m.insert("b".into(), self.b.to_json());
        match self.na {
            Some(r) => {
                m.insert("n/a".into(), Json::str(r.as_str()));
            }
            None => {
                if let Some(d) = self.delta {
                    m.insert("delta".into(), Json::Int(d));
                }
            }
        }
        m.insert(
            "counts".into(),
            Json::obj([
                (
                    "a",
                    Json::obj([
                        ("c", Json::Int(self.counts.0 .0 as i64)),
                        ("n", Json::Int(self.counts.0 .1 as i64)),
                    ]),
                ),
                (
                    "b",
                    Json::obj([
                        ("c", Json::Int(self.counts.1 .0 as i64)),
                        ("n", Json::Int(self.counts.1 .1 as i64)),
                    ]),
                ),
            ]),
        );
        Json::Obj(m)
    }
}

/// `compare`'s output — the reports plus the per-task effect tables (the
/// tables' content addresses are the reports' `per_task_effects_ref`).
#[derive(Debug, Clone)]
pub struct CompareOutcome {
    /// One `ComparisonReport` per metric (every claim is labelled).
    pub reports: Vec<ComparisonReport>,
    /// The per-task effect tables, one per metric (parallel to `reports`).
    pub per_task: Vec<Vec<TaskEffect>>,
}

/// A `MetricValueKind` → numeric projection for paired deltas: `bool` and
/// `decimal` are numeric (`bool` as ppm); `verdict` maps `N→1M, P→500k,
/// I→500k?` — no: verdict/vector/`n/a` metrics never produce a numeric delta
/// (a lattice value is compared by equality, listed per task).
fn numeric(v: &MetricValueKind) -> Option<i64> {
    match v {
        MetricValueKind::Bool(b) => Some(if *b { stats::PPM } else { 0 }),
        MetricValueKind::Decimal(d) => Some(*d),
        MetricValueKind::Verdict(LatticeValue::N) => Some(stats::PPM),
        MetricValueKind::Verdict(LatticeValue::P) => Some(stats::PPM / 2),
        MetricValueKind::Verdict(_) | MetricValueKind::Vector(_) | MetricValueKind::Na(_) => None,
    }
}

/// Whether the run's outcome class enters the metric's denominator.
fn in_denominator(decl: &MetricDeclaration, run: &EvalRun) -> bool {
    decl.outcome_class_policy.in_denominator(run.outcome_class)
}

/// A run's numeric contribution for `metric` — the emitted value when the
/// run's outcome class is in the denominator and the value is numeric;
/// `None` otherwise (`n/a`, never 0). A `budget_exhausted` run *is* in the
/// capability denominator — but a `count_as_failure` run without an emitted
/// value contributes **0** to the rate's `(c, n)` (that is what
/// `count_as_failure` means — distinct from `n/a`).
fn run_value(decl: &MetricDeclaration, run: &EvalRun) -> Option<i64> {
    // AC-R-2.9.2-13: a detector the declaration does not admit is a typed
    // non-value, never a number.
    if let Some(mv) = run.metric_value(&decl.name) {
        if !decl.detector_classes_allowed.contains(&mv.detector) {
            return None;
        }
    }
    let v = run.value_for(&decl.name);
    match &v {
        MetricValueKind::Na(_) => {
            if decl.outcome_class_policy.policy_for(run.outcome_class)
                == hh_ontology::eval::OutcomePolicy::CountAsFailure
                && run.outcome_class != OutcomeClass::Scored
            {
                Some(0)
            } else {
                None
            }
        }
        _ if in_denominator(decl, run) => numeric(&v),
        _ => None,
    }
}

/// The per-task `(c, n)` counts a rate metric needs — `c` = successes
/// (numeric value > 0 for bool/ppm; for decimal metrics `n` counts
/// denominator runs and `c` counts values > 0).
fn run_counts(decl: &MetricDeclaration, run: &EvalRun) -> (u64, u64) {
    if !in_denominator(decl, run) {
        return (0, 0);
    }
    match run_value(decl, run) {
        Some(v) => ((v > 0) as u64, 1),
        None => (0, 1),
    }
}

/// The replicate-axis reducer applied to per-replicate values.
fn reduce(decl: &MetricDeclaration, values: &[i64], c: u64, n: u64) -> Option<i64> {
    match decl.replicate_reducer {
        ReplicateReducer::Mean => stats::mean(values),
        ReplicateReducer::Median => stats::median(values),
        ReplicateReducer::Max => values.iter().copied().max(),
        ReplicateReducer::AtLeast(k) => {
            Some(if values.iter().filter(|&&v| v > 0).count() >= k as usize {
                stats::PPM
            } else {
                0
            })
        }
        ReplicateReducer::PassK(k) => stats::pass_k(c, n, k as u64),
        ReplicateReducer::PassAt(k) => stats::pass_at_k(c, n, k as u64),
        ReplicateReducer::Collect => stats::median(values),
    }
}

/// The factor-membership fields a `varied_factor` spelling permits to differ.
fn factor_member(varied: Option<&str>, field: &str) -> bool {
    matches!(
        (varied, field),
        (Some("model"), "model_snapshot")
            | (Some("environment"), "environment")
            | (Some("fault_profile"), "fault_profile")
            | (Some("perturbation_profile"), "perturbation_profile")
            | (Some("task_family"), "task_family")
            | (Some("environment"), "fault_profile")
            | (Some("environment"), "perturbation_profile")
    )
}

/// `compare` — the matched-budget paired comparison (§5h.2 §2.1).
pub fn compare(input: &CompareInput) -> Result<CompareOutcome, CompareError> {
    // ── 1. declared-match validation (hh-budget is the single source) ────
    hh_budget::matchspec::validate_match(input.arm_specs).map_err(CompareError::Match)?;
    let spec0 = input.arm_specs[0]
        .match_spec
        .clone()
        .expect("validate_match checked");
    if spec0.mode == MatchMode::None {
        // Exploratory — validate_match already refuses; belt-and-braces.
        return Err(CompareError::Match(hh_budget::MatchError {
            arm: Some(0),
            refusal: hh_budget::MatchRefusal::IncommensurableMatch {
                reason: hh_budget::RefusalReason::ExploratoryNoMatch,
                dimension: None,
                enforceability: None,
            },
        }));
    }

    // ── 2. partition runs; refuse non-comparable rows ────────────────────
    let runs_a: Vec<&EvalRun> = input
        .runs
        .iter()
        .filter(|r| r.arm_id == input.arm_a)
        .collect();
    let runs_b: Vec<&EvalRun> = input
        .runs
        .iter()
        .filter(|r| r.arm_id == input.arm_b)
        .collect();
    if runs_a.is_empty() {
        return Err(CompareError::NoRuns {
            arm_id: input.arm_a.into(),
        });
    }
    if runs_b.is_empty() {
        return Err(CompareError::NoRuns {
            arm_id: input.arm_b.into(),
        });
    }
    for r in runs_a.iter().chain(runs_b.iter()) {
        if !r.comparable {
            return Err(CompareError::ExploratoryRun {
                run_id: r.run_id.clone(),
            });
        }
    }

    // ── 3. cross-arm compatibility (everything identical unless varied) ──
    let tasks_a: BTreeSet<&str> = runs_a.iter().map(|r| r.task_id.as_str()).collect();
    let tasks_b: BTreeSet<&str> = runs_b.iter().map(|r| r.task_id.as_str()).collect();
    if tasks_a != tasks_b {
        return Err(CompareError::IncompatibleArms {
            field: "task_set".into(),
            detail: "the arms' task sets differ".into(),
        });
    }
    // Per-task split labels must agree.
    let ctx: BTreeMap<&str, &TaskContext> = input
        .tasks
        .iter()
        .map(|t| (t.task_id.as_str(), t))
        .collect();
    for t in &tasks_a {
        let splits: BTreeSet<SplitLabel> = runs_a
            .iter()
            .chain(runs_b.iter())
            .filter(|r| r.task_id == *t)
            .map(|r| r.split_label)
            .collect();
        if splits.len() != 1 {
            return Err(CompareError::IncompatibleArms {
                field: "split".into(),
                detail: format!("task {t} carries mixed split labels"),
            });
        }
        if let Some(c) = ctx.get(t) {
            if c.split_label != *splits.iter().next().expect("nonempty") {
                return Err(CompareError::IncompatibleArms {
                    field: "split".into(),
                    detail: format!("task {t} split label disagrees with the suite context"),
                });
            }
        }
        let strata: BTreeSet<ContaminationStratum> = runs_a
            .iter()
            .chain(runs_b.iter())
            .filter(|r| r.task_id == *t)
            .map(|r| r.stratum)
            .collect();
        if strata.len() != 1 {
            return Err(CompareError::IncompatibleArms {
                field: "stratum".into(),
                detail: format!("task {t} carries mixed contamination strata"),
            });
        }
    }
    let check_field =
        |field: &str, get: &dyn Fn(&EvalRun) -> Option<String>| -> Result<(), CompareError> {
            if factor_member(input.varied_factor, field) {
                return Ok(());
            }
            let va: BTreeSet<Option<String>> = runs_a.iter().map(|r| get(r)).collect();
            let vb: BTreeSet<Option<String>> = runs_b.iter().map(|r| get(r)).collect();
            if va != vb {
                return Err(CompareError::IncompatibleArms {
                    field: field.into(),
                    detail: "the arms' values differ".into(),
                });
            }
            Ok(())
        };
    check_field("environment", &|r| r.environment_version_id.clone())?;
    check_field("fault_profile", &|r| r.fault_profile.clone())?;
    check_field("perturbation_profile", &|r| r.perturbation_profile.clone())?;
    check_field("model_snapshot", &|r| {
        if r.model_snapshots.is_empty() {
            None
        } else {
            Some(
                r.model_snapshots
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(","),
            )
        }
    })?;
    check_field("cache_state", &|r| Some(r.cache_state.as_str().to_string()))?;

    // ── 4. pairing ───────────────────────────────────────────────────────
    let by_replicate = input.design.pairing == Pairing::ByTaskAndReplicate;
    if by_replicate {
        for r in runs_a.iter().chain(runs_b.iter()) {
            if !r.seed_honoured {
                return Err(CompareError::SeedNotHonoured {
                    run_id: r.run_id.clone(),
                });
            }
        }
    }
    let pairing_str = if by_replicate {
        "by_task_and_replicate"
    } else {
        "by_task"
    };

    // ── 5. budget_match — consumption medians vs tolerance ───────────────
    let tolerance = if spec0.tolerance_ppm > 0 {
        spec0.tolerance_ppm as u64
    } else {
        DEFAULT_IMBALANCE_TOLERANCE_PPM
    };
    let mut imbalance = BTreeMap::new();
    let mut status = BudgetMatchStatus::Matched;
    for d in &spec0.dimensions {
        let va: Vec<i64> = runs_a
            .iter()
            .filter_map(|r| r.budget_consumed.get(d).copied())
            .collect();
        let vb: Vec<i64> = runs_b
            .iter()
            .filter_map(|r| r.budget_consumed.get(d).copied())
            .collect();
        match (stats::median(&va), stats::median(&vb)) {
            (Some(ma), Some(mb)) => {
                let denom = ma.max(mb).max(1) as i128;
                let diff = ((ma - mb).abs() as i128 * stats::PPM as i128 / denom) as u64;
                imbalance.insert(
                    d.as_str().to_string(),
                    Json::obj([
                        ("median_a", Json::Int(ma)),
                        ("median_b", Json::Int(mb)),
                        ("imbalance_ppm", Json::Int(diff as i64)),
                    ]),
                );
                if diff > tolerance && status == BudgetMatchStatus::Matched {
                    status = BudgetMatchStatus::Imbalanced;
                }
            }
            _ => {
                // A matched dimension with no consumption evidence on one arm
                // is `unmatched` — reported, never refused.
                status = BudgetMatchStatus::Unmatched;
                imbalance.insert(d.as_str().to_string(), Json::str("unmeasured"));
            }
        }
    }

    // ── 6. per-metric reports ────────────────────────────────────────────
    let mut reports = Vec::new();
    let mut per_task_tables = Vec::new();
    let family = input.family_size.unwrap_or(input.metrics.len() as u32);
    for name in input.metrics {
        let decl = input
            .declarations
            .iter()
            .find(|d| &d.name == name)
            .ok_or_else(|| CompareError::UnknownMetric { name: name.clone() })?;
        decl.validate()
            .map_err(|e| CompareError::IncompatibleArms {
                field: "metric_declaration".into(),
                detail: format!("{name}: {e:?}"),
            })?;

        let mut effects = Vec::new();
        let mut deltas = Vec::new();
        for t in &tasks_a {
            let (ra, rb): (Vec<&EvalRun>, Vec<&EvalRun>) = (
                runs_a.iter().filter(|r| r.task_id == *t).copied().collect(),
                runs_b.iter().filter(|r| r.task_id == *t).copied().collect(),
            );
            let needed = input.design.replicates_per_cell.max(1) as usize;
            let (mut ca, mut na_, mut cb, mut nb_) = (0u64, 0u64, 0u64, 0u64);
            let (mut va, mut vb) = (Vec::new(), Vec::new());
            for r in &ra {
                let (c, n) = run_counts(decl, r);
                ca += c;
                na_ += n;
                if let Some(v) = run_value(decl, r) {
                    va.push(v);
                }
            }
            for r in &rb {
                let (c, n) = run_counts(decl, r);
                cb += c;
                nb_ += n;
                if let Some(v) = run_value(decl, r) {
                    vb.push(v);
                }
            }
            let red_a = reduce(decl, &va, ca, na_);
            let red_b = reduce(decl, &vb, cb, nb_);
            let cell = match (red_a, red_b) {
                _ if ra.len() < needed || rb.len() < needed => TaskEffect {
                    task_id: t.to_string(),
                    a: red_a
                        .map(MetricValueKind::Decimal)
                        .unwrap_or(MetricValueKind::Na(NaReason::NotRun)),
                    b: red_b
                        .map(MetricValueKind::Decimal)
                        .unwrap_or(MetricValueKind::Na(NaReason::NotRun)),
                    delta: None,
                    na: Some(NaReason::NotRun),
                    counts: ((ca, na_), (cb, nb_)),
                },
                (Some(a), Some(b)) => {
                    let d = a - b;
                    deltas.push(d);
                    TaskEffect {
                        task_id: t.to_string(),
                        a: MetricValueKind::Decimal(a),
                        b: MetricValueKind::Decimal(b),
                        delta: Some(d),
                        na: None,
                        counts: ((ca, na_), (cb, nb_)),
                    }
                }
                _ => TaskEffect {
                    task_id: t.to_string(),
                    a: red_a
                        .map(MetricValueKind::Decimal)
                        .unwrap_or(MetricValueKind::Na(NaReason::EstimatorUndefined)),
                    b: red_b
                        .map(MetricValueKind::Decimal)
                        .unwrap_or(MetricValueKind::Na(NaReason::EstimatorUndefined)),
                    delta: None,
                    na: Some(NaReason::EstimatorUndefined),
                    counts: ((ca, na_), (cb, nb_)),
                },
            };
            effects.push(cell);
        }

        // The paired effect — mean of per-task deltas + paired bootstrap.
        let seed = hh_identity::idp_digest(
            "eval.compare",
            format!("{}|{}|{}", input.arm_a, input.arm_b, name).as_bytes(),
        );
        let point = stats::mean(&deltas);
        let bca = matches!(
            decl.interval_method,
            IntervalMethod::BootstrapPaired(BootstrapPairedMethod::Bca)
        );
        let interval = stats::bootstrap_paired(&deltas, input.confidence_ppm, bca, &seed);
        let (pos, zero, neg, _flip) = stats::sign_profile(&deltas);
        let abs: Vec<i64> = deltas.iter().map(|d| d.abs()).collect();

        // outcome_bounds — worst/best case over the excluded classes: excluded
        // arm-A runs as 0 / arm-B as 1M lowers the bound; the reverse raises it.
        let excl_a = runs_a.iter().filter(|r| !in_denominator(decl, r)).count() as i128;
        let excl_b = runs_b.iter().filter(|r| !in_denominator(decl, r)).count() as i128;
        let denom_runs = (runs_a.len().max(1) + runs_b.len().max(1)) as i128;
        let swing = ((excl_a + excl_b) * stats::PPM as i128 / denom_runs) as i64;
        let lower = point.unwrap_or(0) - swing;
        let upper = point.unwrap_or(0) + swing;
        let bounds_verdict = match point {
            Some(p)
                if (lower > 0 && p > 0) || (upper < 0 && p < 0) || (lower == 0 && upper == 0) =>
            {
                OutcomeBoundsVerdict::Robust
            }
            _ => OutcomeBoundsVerdict::Sensitive,
        };

        let table_json = Json::Arr(effects.iter().map(|e| e.to_json()).collect());
        let table_ref = hh_identity::idp_id(
            "eval.per_task_effects",
            table_json.to_canonical_string().as_bytes(),
        );

        let label = if !decl.headline
            || status == BudgetMatchStatus::Unmatched
            || (input.benefit_kind == BenefitKind::ArtifactBenefit && !input.held_out)
        {
            ReportLabelKind::NotHeadlined
        } else {
            ReportLabelKind::Headlined
        };

        reports.push(ComparisonReport {
            arm_a: input.arm_a.to_string(),
            arm_b: input.arm_b.to_string(),
            metric: name.clone(),
            pairing: pairing_str.to_string(),
            paired_effect: PairedEffect {
                point: point.map(Json::Int),
                interval: interval
                    .map(|i| Json::obj([("lo", Json::Int(i.lo)), ("hi", Json::Int(i.hi))])),
                method: IntervalMethod::BootstrapPaired(if bca {
                    BootstrapPairedMethod::Bca
                } else {
                    BootstrapPairedMethod::Percentile
                }),
            },
            per_task_effects_ref: Some(table_ref),
            budget_match: BudgetMatch {
                limits_equal: true,
                consumption_imbalance: if imbalance.is_empty() {
                    None
                } else {
                    Some(Json::Obj(imbalance.clone()))
                },
                tolerance_ppm: tolerance,
                status,
            },
            benefit_kind: input.benefit_kind,
            held_out: input.held_out,
            estimated: None,
            test: TestRecord {
                kind: TestKind::PermutationSignflip,
            },
            sign_profile: SignProfile {
                helped: pos,
                hurt: neg,
                unchanged: zero,
            },
            tail_effects: TailEffects {
                p50: stats::median(&abs).map_or(Json::Null, Json::Int),
                p95: stats::quantile(&abs, 950_000).map_or(Json::Null, Json::Int),
                max: abs.iter().copied().max().map_or(Json::Null, Json::Int),
            },
            outcome_bounds: OutcomeBounds {
                lower: Json::Int(lower),
                upper: Json::Int(upper),
                verdict: bounds_verdict,
            },
            multiplicity: Multiplicity {
                family_size: family,
                adjusted: "holm".into(),
                raw_ppm: None,
                adjusted_ppm: None,
                label: None,
            },
            label,
            estimator_selection: EstimatorSelection {
                method: IntervalMethod::BootstrapPaired(if bca {
                    BootstrapPairedMethod::Bca
                } else {
                    BootstrapPairedMethod::Percentile
                }),
                selection_rule: "s3.3.paired_delta".into(),
                floors: BTreeMap::new(),
                fallback_chain: vec![],
                substituted: None,
            },
        });
        per_task_tables.push(effects);
    }

    Ok(CompareOutcome {
        reports,
        per_task: per_task_tables,
    })
}

/// `ContrastEstimate` — the AC-R-2.9.2-1 interaction estimate ("does
/// variant V help model M₁ more than M₂?"): the per-task
/// difference-of-differences of the two models' paired deltas — point =
/// the mean DiD, interval = the paired bootstrap over them (ADR-0158 D5;
/// deterministic seed over the canonical input, same as `compare`).
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastEstimate {
    /// The metric the contrast is over.
    pub metric: String,
    /// The interaction point estimate (the metric's unit).
    pub point: i64,
    /// The interval `{lo, hi}`.
    pub interval: stats::Interval,
    /// The tasks paired across both models.
    pub n_tasks: usize,
    /// The interval method used (`bootstrap_paired` — same as `compare`).
    pub method: IntervalMethod,
}

impl ContrastEstimate {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("contrast_estimate/1")),
            ("metric", Json::str(&self.metric)),
            ("point", Json::Int(self.point)),
            (
                "interval",
                Json::obj([
                    ("lo", Json::Int(self.interval.lo)),
                    ("hi", Json::Int(self.interval.hi)),
                ]),
            ),
            ("n_tasks", Json::Int(self.n_tasks as i64)),
            ("method", self.method.to_json()),
        ])
    }
}

/// `contrast(metric, model_a_outcome, model_b_outcome)` — the canned
/// interaction analysis: two `compare` outcomes over the same arms, one per
/// model. Per task, the DiD is `delta_M1(t) − delta_M2(t)`; tasks without
/// a paired delta on both sides contribute a typed skip, never a zero.
/// `ContrastUndefined` when no task pairs or the interval is undefined.
pub fn contrast(
    metric: &str,
    model_a: &CompareOutcome,
    model_b: &CompareOutcome,
    confidence_ppm: i64,
) -> Result<ContrastEstimate, CompareError> {
    fn table<'o>(out: &'o CompareOutcome, metric: &str) -> Result<&'o [TaskEffect], CompareError> {
        let idx = out
            .reports
            .iter()
            .position(|r| r.metric == metric)
            .ok_or_else(|| CompareError::UnknownMetric {
                name: metric.into(),
            })?;
        Ok(&out.per_task[idx])
    }
    let ta = table(model_a, metric)?;
    let tb = table(model_b, metric)?;
    let deltas_b: BTreeMap<&str, i64> = tb
        .iter()
        .filter_map(|t| t.delta.map(|d| (t.task_id.as_str(), d)))
        .collect();
    let mut dids: Vec<(String, i64)> = Vec::new();
    for t in ta {
        if let (Some(da), Some(db)) = (t.delta, deltas_b.get(t.task_id.as_str())) {
            dids.push((t.task_id.clone(), da - *db));
        }
    }
    if dids.is_empty() {
        return Err(CompareError::ContrastUndefined {
            detail: format!("no task pairs both models' deltas for {metric}"),
        });
    }
    let vals: Vec<i64> = dids.iter().map(|(_, d)| *d).collect();
    let point = stats::mean(&vals).ok_or_else(|| CompareError::ContrastUndefined {
        detail: "mean over empty DiD set".into(),
    })?;
    let seed = hh_identity::idp_digest(
        "eval.contrast",
        format!(
            "{metric}|{}",
            vals.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
        .as_bytes(),
    );
    let interval =
        stats::bootstrap_paired(&vals, confidence_ppm, false, &seed).ok_or_else(|| {
            CompareError::ContrastUndefined {
                detail: "paired bootstrap undefined".into(),
            }
        })?;
    Ok(ContrastEstimate {
        metric: metric.into(),
        point,
        interval,
        n_tasks: vals.len(),
        method: IntervalMethod::BootstrapPaired(BootstrapPairedMethod::Percentile),
    })
}
