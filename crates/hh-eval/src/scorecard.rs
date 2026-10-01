//! `render_scorecard` (spec §5h.2 §2.1; ADR-0045 D4/D5/D6; R-2.9.2; S3.3).
//!
//! The render is **pure over the source records + watermark** and
//! hash-stable: `scorecard_id = H(canonical(report minus id))` under
//! `idp/1`. Rules:
//!
//! - every catalogue metric has a cell for every configuration — `value |
//!   n/a{reason}`, never a coerced 0 (T-LCD-15);
//! - capability renders **per contamination stratum**; pooling strata without
//!   annotation is refused (`StrataPooledUnannotated` — ADR-0143 L5);
//! - veto-tripped runs are excluded from headline cells and counted beside
//!   (`vetoed` on the cell, `vetoed_successes` on the summary);
//! - outcome classes outside the metric's denominator are counted beside in
//!   `excluded{outcome_class → n}`, never dropped;
//! - per-task `(c, n)` counts are preserved on every cell so pass^k/pass@k
//!   stay recomputable; `k > n` renders `n/a{estimator_undefined}`;
//! - `clt` is refused on heavy-tailed units by the declaration (ADR-0158);
//!   the `T_clt`/`T_bca` sample floors substitute a declared fallback and
//!   record the substitution;
//! - a suite `retired_for_headline` demotes the report label — the rows still
//!   render, labelled.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ontology::compliance::{MetricDeclaration, NaReason};
use hh_ontology::eval::{
    BootstrapPairedMethod, EstimatorSelection, IntervalMethod, MetricValueKind, OutcomePolicy,
    ReplicateReducer, Substitution,
};
use hh_ontology::participant::ParticipantDescriptor;
use hh_ontology::MetricValueType;
use hh_wire::Json;

use hh_lab::analysis::{
    ConfigurationSummary, MetricCell, ReportLabelKind, ScorecardReport, Tails, TaskCount,
};

use crate::runs::{EvalRun, SuiteContext, TaskContext};
use crate::stats;

/// The ADR-0158 estimator floors (Stage-3 MUST-data values): `clt` requires
/// `n ≥ T_CLT` non-`n/a` per-task values; `bootstrap_paired{bca}` requires
/// `n ≥ T_BCA`. A floor violation substitutes the first admissible fallback
/// and records it in `EstimatorSelection.substituted`.
///
/// `T_CLT` is the ADR-0158/OQ-367 Stage-3 placeholder **100** (S3.3 landed an
/// interim `30`; corrected at S3.4c — ADR-0280).
pub const T_CLT: u64 = 100;
/// The BCa floor (ADR-0158).
pub const T_BCA: u64 = 30;

/// `render_scorecard`'s typed refusals.
#[derive(Debug, Clone, PartialEq)]
pub enum ScorecardError {
    /// Pooling across contamination strata was requested without the
    /// `annotate` flag (L5 — `StrataPooledUnannotated`).
    StrataPooledUnannotated {
        /// The strata that would have pooled.
        strata: Vec<String>,
    },
}

impl std::fmt::Display for ScorecardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScorecardError::StrataPooledUnannotated { strata } => {
                write!(f, "StrataPooledUnannotated: {strata:?}")
            }
        }
    }
}

impl std::error::Error for ScorecardError {}

/// `render_scorecard`'s input (records-in).
pub struct ScorecardInput<'a> {
    /// The runs (the results-plane projection).
    pub runs: &'a [EvalRun],
    /// The task contexts.
    pub tasks: &'a [TaskContext],
    /// The suite contexts (retirement flags).
    pub suites: &'a [SuiteContext],
    /// The catalogue declarations to render.
    pub declarations: &'a [MetricDeclaration],
    /// The catalogue's content address (`metric_registry_version`).
    pub metric_registry_version: &'a str,
    /// The pricing-table version spend metrics read against.
    pub price_table_version: &'a str,
    /// The results watermark.
    pub watermark: u64,
    /// The interval confidence (ppm).
    pub confidence_ppm: i64,
    /// Pool strata into one cell per (configuration, metric)? Requires
    /// `annotate_pooling`; otherwise per-stratum cells are emitted.
    pub pool_strata: bool,
    /// The pooling annotation (L5's `annotated`).
    pub annotate_pooling: bool,
    /// The declared snapshot → family map the portability cell reads
    /// (AC-R-2.9.2-10; CF-417 — `snapshot_id → family`, e.g. provider). A
    /// snapshot absent from the map is `unknown` — it can never evidence a
    /// family leg of the claim.
    pub model_families: &'a std::collections::BTreeMap<String, String>,
}

/// A run → `ParticipantDescriptor` for `MetricDeclaration::applicability`.
/// Public for the §6.4 kernel (`hh-analysis`) — one applicability
/// reconstruction, never two (CC1).
pub fn descriptor(run: &EvalRun) -> ParticipantDescriptor {
    // A run's descriptor is already validated at bind time; reconstruct it
    // field-wise (the observability/class invariant is the ledger's, not the
    // renderer's, to re-derive).
    ParticipantDescriptor {
        class: run.participant_class,
        hosting_mechanism: if run.participant_class
            == hh_ontology::participant::ParticipantClass::Native
        {
            hh_ontology::participant::HostingMechanism::None
        } else {
            hh_ontology::participant::HostingMechanism::SessionAbi
        },
        observability_level: run.observability_level.clone(),
        capability_vector: run.capability_vector.clone(),
    }
}

/// The per-run numeric value under the declaration's outcome policy — `None`
/// for `n/a`/excluded/non-numeric (never coerced). Public for the §6.4
/// kernel (`hh-analysis`) — the same denominator rule drives A1.
pub fn run_numeric(decl: &MetricDeclaration, run: &EvalRun) -> Option<i64> {
    if !decl.outcome_class_policy.in_denominator(run.outcome_class) {
        return None;
    }
    // AC-R-2.9.2-13: a value whose detector the declaration does not admit
    // (e.g. `judged` against a deterministic-only headline metric) is a
    // typed non-value — never averaged in.
    if let Some(v) = run.metric_value(&decl.name) {
        if !decl.detector_classes_allowed.contains(&v.detector) {
            return None;
        }
    }
    match run.value_for(&decl.name) {
        MetricValueKind::Bool(b) => Some(if b { stats::PPM } else { 0 }),
        MetricValueKind::Decimal(d) => Some(d),
        MetricValueKind::Verdict(hh_ontology::eval::LatticeValue::N) => Some(stats::PPM),
        MetricValueKind::Verdict(hh_ontology::eval::LatticeValue::P) => Some(stats::PPM / 2),
        MetricValueKind::Na(_) => {
            // A denominator run with no emitted value contributes 0 when the
            // policy counts it as failure (budget_exhausted under the
            // capability policy); otherwise it is a genuine non-value.
            if decl.outcome_class_policy.policy_for(run.outcome_class)
                == OutcomePolicy::CountAsFailure
                && run.outcome_class != hh_ontology::control::OutcomeClass::Scored
            {
                Some(0)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The interval for one cell under the declaration's `interval_method`,
/// applying the ADR-0158 floors (with `substituted` recorded). Public for
/// the §6.4 kernel (`hh-analysis`) — the selection rule has one home.
pub fn cell_interval(
    decl: &MetricDeclaration,
    task_values: &[(String, i64)],
    per_task: &[(u64, u64)],
    confidence_ppm: i64,
    seed: &str,
) -> (Option<(i64, i64)>, EstimatorSelection) {
    let n = task_values.len() as u64;
    let mut selection = EstimatorSelection {
        method: decl.interval_method.clone(),
        selection_rule: "adr-0158.stage3".into(),
        floors: BTreeMap::from([("t_clt".into(), T_CLT), ("t_bca".into(), T_BCA)]),
        fallback_chain: vec![IntervalMethod::Wilson, IntervalMethod::Bootstrap],
        substituted: None,
    };
    let flat: Vec<i64> = task_values.iter().map(|(_, v)| *v).collect();
    let total_cn: (u64, u64) = per_task
        .iter()
        .fold((0, 0), |(c, n), (ci, ni)| (c + ci, n + ni));
    let rate_interval = |m: &IntervalMethod| -> Option<(i64, i64)> {
        match m {
            IntervalMethod::Wilson => {
                stats::wilson(total_cn.0 as i64, total_cn.1 as i64, confidence_ppm)
                    .map(|i| (i.lo, i.hi))
            }
            IntervalMethod::BayesianBeta(prior) => {
                let (a, b) = parse_beta_prior(prior).unwrap_or((1, 1));
                stats::bayesian_beta(total_cn.0 as i64, total_cn.1 as i64, (a, b), confidence_ppm)
                    .map(|i| (i.lo, i.hi))
            }
            _ => None,
        }
    };
    let value_interval = |m: &IntervalMethod| -> Option<(i64, i64)> {
        match m {
            IntervalMethod::Clt => stats::clt(&flat, confidence_ppm).map(|i| (i.lo, i.hi)),
            IntervalMethod::ClusteredClt => {
                stats::clustered_clt(task_values, confidence_ppm).map(|i| (i.lo, i.hi))
            }
            IntervalMethod::Bootstrap => {
                stats::bootstrap(&flat, confidence_ppm, seed).map(|i| (i.lo, i.hi))
            }
            IntervalMethod::BootstrapPaired(m) => stats::bootstrap_paired(
                &flat,
                confidence_ppm,
                *m == BootstrapPairedMethod::Bca,
                seed,
            )
            .map(|i| (i.lo, i.hi)),
            IntervalMethod::Wilson | IntervalMethod::BayesianBeta(_) => rate_interval(m),
        }
    };
    // Rate methods need (c, n); value methods need per-task values. A method
    // whose inputs are absent yields no interval (the cell reports `n/a`
    // through `interval: None`, never a fabricated range).
    let is_rate = total_cn.1 > 0
        && matches!(
            decl.value_type,
            MetricValueType::Bool | MetricValueType::Decimal
        )
        && decl.unit == "ppm";
    let interval = if is_rate {
        rate_interval(&decl.interval_method).or_else(|| value_interval(&decl.interval_method))
    } else {
        value_interval(&decl.interval_method)
    };
    // ADR-0158 floors: clt/bca below floor substitute the first admissible
    // fallback and record the substitution.
    let floor_needed = match &decl.interval_method {
        IntervalMethod::Clt | IntervalMethod::ClusteredClt => Some(T_CLT),
        IntervalMethod::BootstrapPaired(BootstrapPairedMethod::Bca) => Some(T_BCA),
        _ => None,
    };
    let mut interval = interval;
    if let Some(floor) = floor_needed {
        if n < floor || interval.is_none() {
            for f in &selection.fallback_chain.clone() {
                let alt = if is_rate {
                    rate_interval(f).or_else(|| value_interval(f))
                } else {
                    value_interval(f)
                };
                if alt.is_some() {
                    interval = alt;
                    selection.substituted = Some(Substitution {
                        from: decl.interval_method.clone(),
                        to: f.clone(),
                        reason: if n < floor {
                            "floor_unmet".into()
                        } else {
                            "undefined".into()
                        },
                    });
                    break;
                }
            }
        }
    }
    (interval, selection)
}

/// Parse `bayesian_beta{prior}` — `"beta{a,b}"` with integer params (the
/// catalogue pins `beta{1,1}`).
fn parse_beta_prior(prior: &str) -> Option<(u64, u64)> {
    let inner = prior.strip_prefix("beta{")?.strip_suffix('}')?;
    let (a, b) = inner.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// `render_scorecard` — the deterministic render (pure over inputs).
pub fn render_scorecard(input: &ScorecardInput) -> Result<ScorecardReport, ScorecardError> {
    // ── strata separation (L5) ───────────────────────────────────────────
    let mut strata: BTreeSet<String> = BTreeSet::new();
    for r in input.runs {
        strata.insert(r.stratum.name().to_string());
    }
    if input.pool_strata && !input.annotate_pooling && strata.len() > 1 {
        return Err(ScorecardError::StrataPooledUnannotated {
            strata: strata.iter().cloned().collect(),
        });
    }
    let retired_suites: BTreeSet<&str> = input
        .suites
        .iter()
        .filter(|s| s.retired_for_headline)
        .map(|s| s.suite_id.as_str())
        .collect();
    let tasks: BTreeMap<&str, &TaskContext> = input
        .tasks
        .iter()
        .map(|t| (t.task_id.as_str(), t))
        .collect();

    // ── group runs by configuration ──────────────────────────────────────
    let mut by_config: BTreeMap<&str, Vec<&EvalRun>> = BTreeMap::new();
    for r in input.runs {
        by_config.entry(&r.configuration_id).or_default().push(r);
    }

    let mut summaries = Vec::new();
    let mut any_retired = false;
    for (config_id, runs) in &by_config {
        let mut cells = Vec::new();
        let mut vetoed_successes = 0u64;
        for decl in input.declarations {
            // The strata to render: per-stratum cells unless pooling is
            // annotated.
            let run_strata: BTreeSet<String> =
                runs.iter().map(|r| r.stratum.name().to_string()).collect();
            let render_strata: Vec<Option<String>> = if input.pool_strata {
                vec![None]
            } else {
                run_strata.iter().cloned().map(Some).collect()
            };
            for stratum in render_strata {
                let stratum_runs: Vec<&EvalRun> = runs
                    .iter()
                    .filter(|r| input.pool_strata || stratum.as_deref() == Some(r.stratum.name()))
                    .copied()
                    .collect();
                cells.push(render_cell(
                    decl,
                    &stratum_runs,
                    stratum,
                    &tasks,
                    input.confidence_ppm,
                    config_id,
                ));
            }
        }
        // Vetoed successes: veto-tripped runs whose metric value was positive.
        for r in runs {
            if !r.veto_tripped.is_empty() {
                let headline = input
                    .declarations
                    .iter()
                    .find(|d| d.headline)
                    .map(|d| run_numeric(d, r).map(|v| v > 0).unwrap_or(false))
                    .unwrap_or(false);
                if headline {
                    vetoed_successes += 1;
                }
            }
            if retired_suites.contains(r.suite_id.as_str()) {
                any_retired = true;
            }
        }
        let mut run_ids: Vec<String> = runs.iter().map(|r| r.run_id.clone()).collect();
        run_ids.sort();
        // The portability cell (AC-R-2.9.2-10; CF-417): `bool{true}` only
        // when the configuration's runs evidence ≥ 2 distinct model
        // snapshots across ≥ 2 declared families and ≥ 1 held-out level —
        // otherwise `n/a{not_run}`, never a fabricated claim.
        let snapshots: BTreeSet<&str> = runs
            .iter()
            .flat_map(|r| r.model_snapshots.values().map(String::as_str))
            .collect();
        let families: BTreeSet<&str> = snapshots
            .iter()
            .filter_map(|s| input.model_families.get(*s).map(String::as_str))
            .collect();
        let held_out = runs.iter().any(|r| {
            matches!(
                r.split_label,
                hh_ontology::lab::SplitLabel::HeldOut | hh_ontology::lab::SplitLabel::Private
            )
        });
        let portability = if snapshots.len() >= 2 && families.len() >= 2 && held_out {
            MetricValueKind::Bool(true)
        } else {
            MetricValueKind::Na(NaReason::NotRun)
        };
        summaries.push(ConfigurationSummary {
            configuration_id: config_id.to_string(),
            participant_class: runs
                .first()
                .map(|r| r.participant_class.as_str().to_string())
                .unwrap_or_default(),
            run_ids,
            cells,
            vetoed_successes,
            portability,
        });
    }

    let mut report = ScorecardReport {
        scorecard_id: String::new(),
        metric_registry_version: input.metric_registry_version.to_string(),
        price_table_version: input.price_table_version.to_string(),
        watermark: input.watermark,
        configurations: summaries,
        strata: strata.iter().cloned().collect(),
        label: if any_retired {
            ReportLabelKind::Guarded
        } else {
            ReportLabelKind::Headlined
        },
    };
    report.scorecard_id = report.compute_id();
    Ok(report)
}

/// Render one `(configuration, stratum, metric)` cell — the scorecard's A1
/// primitive, reused by `hh-analysis` (`summarize`) so the cell estimator
/// has exactly one implementation (CC1; ADR-0158).
pub fn render_cell(
    decl: &MetricDeclaration,
    runs: &[&EvalRun],
    stratum: Option<String>,
    _tasks: &BTreeMap<&str, &TaskContext>,
    confidence_ppm: i64,
    config_id: &str,
) -> MetricCell {
    // Family applicability — a metric naming families the run's family is
    // outside renders `n/a{class}` (the closed NaReason set has no `family`;
    // `class` is the declared inapplicability reason).
    if !decl.applies_to_families.is_empty()
        && runs
            .iter()
            .all(|r| !decl.applies_to_families.contains(&r.environment_family))
    {
        return MetricCell {
            metric: decl.name.clone(),
            point: MetricValueKind::Na(NaReason::Class),
            interval: None,
            estimator: None,
            distribution_ref: None,
            tails: None,
            per_task: vec![],
            excluded: BTreeMap::new(),
            vetoed: 0,
            stratum,
        };
    }
    // Per-run applicability (class ∧ observability ∧ capabilities ∧ mediation
    // ∧ family — ADR-0165 D6; R-2.10.6⁰) — an inapplicable run contributes to
    // `excluded`, never a 0.
    let mut excluded: BTreeMap<String, u64> = BTreeMap::new();
    let mut applicable: Vec<&EvalRun> = Vec::new();
    for r in runs {
        match decl.applicability_at(&descriptor(r), &r.mediation, Some(r.environment_family)) {
            Ok(()) => applicable.push(r),
            Err(reason) => {
                *excluded
                    .entry(format!("n/a:{}", reason.as_str()))
                    .or_default() += 1;
            }
        }
    }
    if applicable.is_empty() && !runs.is_empty() {
        let reason = decl
            .applicability_at(
                &descriptor(runs[0]),
                &runs[0].mediation,
                Some(runs[0].environment_family),
            )
            .err()
            .unwrap_or(NaReason::Class);
        return MetricCell {
            metric: decl.name.clone(),
            point: MetricValueKind::Na(reason),
            interval: None,
            estimator: None,
            distribution_ref: None,
            tails: None,
            per_task: vec![],
            excluded,
            vetoed: 0,
            stratum,
        };
    }
    // Vetoed runs are excluded from the headline cell and counted beside.
    let clean: Vec<&EvalRun> = applicable
        .iter()
        .filter(|r| r.veto_tripped.is_empty())
        .copied()
        .collect();
    let vetoed = (applicable.len() - clean.len()) as u64;

    // Per-task (c, n) + per-task values.
    let mut by_task: BTreeMap<&str, Vec<&EvalRun>> = BTreeMap::new();
    for r in &clean {
        by_task.entry(r.task_id.as_str()).or_default().push(*r);
    }
    let mut per_task = Vec::new();
    let mut task_values: Vec<(String, i64)> = Vec::new();
    let mut undefined = 0usize;
    for (task_id, truns) in &by_task {
        let mut c = 0u64;
        let mut n = 0u64;
        let mut vals = Vec::new();
        for r in truns {
            if decl.outcome_class_policy.in_denominator(r.outcome_class) {
                n += 1;
                if let Some(v) = run_numeric(decl, r) {
                    vals.push(v);
                    if v > 0 {
                        c += 1;
                    }
                }
            } else {
                *excluded
                    .entry(r.outcome_class.as_str().to_string())
                    .or_default() += 1;
            }
        }
        per_task.push(TaskCount {
            task_id: task_id.to_string(),
            c,
            n,
        });
        let task_value = match decl.replicate_reducer {
            ReplicateReducer::PassK(k) => stats::pass_k(c, n, k as u64),
            ReplicateReducer::PassAt(k) => stats::pass_at_k(c, n, k as u64),
            ReplicateReducer::Mean => stats::mean(&vals),
            ReplicateReducer::Median => stats::median(&vals),
            ReplicateReducer::Max => vals.iter().copied().max(),
            ReplicateReducer::AtLeast(k) => Some(if c >= k as u64 { stats::PPM } else { 0 }),
            ReplicateReducer::Collect => stats::median(&vals),
        };
        match task_value {
            Some(v) => task_values.push((task_id.to_string(), v)),
            None => undefined += 1,
        }
    }
    // The cell point — the mean over per-task values (`n/a{estimator_undefined}`
    // when every task's reducer was undefined; `n/a{not_run}` when the cell
    // has no denominator runs at all).
    let total_n: u64 = per_task.iter().map(|t| t.n).sum();
    let point = if total_n == 0 && per_task.is_empty() {
        MetricValueKind::Na(NaReason::NotRun)
    } else if task_values.is_empty() {
        MetricValueKind::Na(NaReason::EstimatorUndefined)
    } else {
        let vals: Vec<i64> = task_values.iter().map(|(_, v)| *v).collect();
        MetricValueKind::Decimal(stats::mean(&vals).unwrap_or(0))
    };
    if undefined > 0 {
        *excluded
            .entry("n/a:estimator_undefined".to_string())
            .or_default() += undefined as u64;
    }

    let flat: Vec<i64> = task_values.iter().map(|(_, v)| *v).collect();
    let seed = hh_identity::idp_digest(
        "eval.scorecard",
        format!("{config_id}|{}", decl.name).as_bytes(),
    );
    let cn: Vec<(u64, u64)> = per_task.iter().map(|t| (t.c, t.n)).collect();
    let (interval, estimator) = cell_interval(decl, &task_values, &cn, confidence_ppm, &seed);
    let distribution_ref = if flat.is_empty() {
        None
    } else {
        Some(hh_identity::idp_id(
            "eval.distribution",
            Json::Arr(flat.iter().map(|v| Json::Int(*v)).collect())
                .to_canonical_string()
                .as_bytes(),
        ))
    };
    let tails = if flat.is_empty() {
        None
    } else {
        let catastrophic = flat
            .iter()
            .filter(|&&v| match decl.direction {
                hh_ontology::Direction::Higher => v <= 0,
                hh_ontology::Direction::Lower => v >= stats::PPM,
            })
            .count();
        Some(Tails {
            p50: stats::median(&flat).unwrap_or(0),
            p95: stats::quantile(&flat, 950_000).unwrap_or(0),
            max: flat.iter().copied().max().unwrap_or(0),
            catastrophic_rate_ppm: (catastrophic as i128 * stats::PPM as i128
                / flat.len().max(1) as i128) as i64,
        })
    };
    MetricCell {
        metric: decl.name.clone(),
        point,
        interval,
        estimator: Some(estimator),
        distribution_ref,
        tails,
        per_task,
        excluded,
        vetoed,
        stratum,
    }
}
