//! `kernel` — `analyze(spec, input) → AnalysisOutcome` (§6.4 §1).
//!
//! The kernel is pure: every input is records-in (the *caller* — the
//! `lab.analysis.analyze` op or a test — resolves rows, manifests, facts,
//! contexts, design and declarations). Nothing here touches a store.
//!
//! Kinds at C0/Stage-3 (`AnalysisSpec.kind`):
//! - `summarize` — A1: one `ConfigurationSummary` per selected
//!   configuration (per-stratum cells; strata are never silently pooled);
//! - `compare` — A2: `hh_eval::compare::compare` between the `arm_a` /
//!   `arm_b` named in `query.filters`;
//! - `interaction` — A3 `contrast`: the default difference-of-differences
//!   over the two arm pairs named `filters.contrast.{pair_a,pair_b}`;
//! - `transfer` — transfer rows: `benefits::transfer` for the arm pair
//!   (paired held-out comparison + `transfer_ratio`);
//! - `equivalence` — A8: `benefits::equivalence_run` under the
//!   pre-registration's margins;
//! - `frontier` — the M2 capability–cost Pareto frontier with provenance
//!   bands (spec §8.2; AC-R-2.1.6-8): per-`(arm, provenance_class,
//!   currency)` cells, per-class + combined non-dominated frontiers,
//!   `estimate`/`unknown`/partial-coverage rows rendered as bands,
//!   never points; the same `validate_match` refusal vocabulary as
//!   `compare`.
//!
//! A12 `multiplicity` is not a kind — it is a pass over every produced
//! comparison (Holm on the pre-registered primary family, BH on the
//! exploratory remainder; nothing suppressed).
//!
//! `filters` keys (`query.filters` is the spec's record of what the
//! analysis computes — CC3: it is data, hashed into `spec_id`):
//! - `arm_a`, `arm_b` — the A2 pair;
//! - `varied_factor` — the factor allowed to differ (`model` |
//!   `environment` | `fault_profile` | `perturbation_profile` |
//!   `task_family`);
//! - `contrast.pair_a`/`contrast.pair_b` — `{arm_a, arm_b}` objects;
//! - `transfer.held_out_factor` — `model_snapshot` | `environment` |
//!   `task_family`; `transfer.main_plane = true` runs the main-plane
//!   comparison first so `transfer_ratio` lands on the held-out row;
//! - `reference`, `candidate`, `suite_ref` — the A8 operands.

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::matchspec::ArmSpec;
use hh_eval::benefits;
use hh_eval::compare::{compare, contrast, CompareError, CompareInput, CompareOutcome};
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{EvalRun, SuiteContext, TaskContext};
use hh_eval::scorecard::render_cell;
use hh_eval::stats;
use hh_ledger::manifest::RunManifest;
use hh_ontology::compliance::{MetricDeclaration, NaReason};
use hh_ontology::eval::{Design, MetricValueKind, PreRegistration};
use hh_results::row::ResultsRow;
use hh_results::watermark::WatermarkSet;
use hh_wire::Json;

use hh_lab::analysis::{AnalysisSpec, ComparisonReport, ConfigurationSummary};

use crate::error::AnalysisError;
use crate::multiplicity::apply_multiplicity;
use crate::project::eval_run;
use crate::report::{assemble, AnalysisOutcome};

/// `AnalysisInput` — everything `analyze` reads (records-in).
pub struct AnalysisInput<'a> {
    /// The selected head rows (the `QuerySpec` result at the watermark).
    pub rows: &'a [ResultsRow],
    /// `run_id → RunManifest` for every selected row's subject run.
    pub manifests: &'a BTreeMap<String, RunManifest>,
    /// `run_id → LedgerFacts` projected over each row's source prefix.
    pub facts: &'a BTreeMap<String, LedgerFacts>,
    /// Task contexts (records-in — the suite plane's rows).
    pub tasks: &'a [TaskContext],
    /// Suite contexts.
    pub suites: &'a [SuiteContext],
    /// The pinned `Design` (resolved from `spec.spec_ref` by the caller).
    /// Required for `compare`/`interaction`/`transfer`/`equivalence`.
    pub design: Option<&'a Design>,
    /// `(arm_id, ArmSpec)` pairs — the resolved match inputs
    /// (`hh_budget::matchspec::ArmSpec`: `eval_budget`/`search_budget`
    /// bodies + `match_spec` + enforcement), keyed by the experiment's
    /// arm ids.
    pub arm_specs: &'a [(String, ArmSpec)],
    /// The metric declarations (`spec.query.metrics` resolve here).
    pub declarations: &'a [MetricDeclaration],
    /// The experiment's `PreRegistration` (A8 margins; A12 primary
    /// family; the `confirmatory` label gate).
    pub pre_registration: Option<&'a PreRegistration>,
    /// The serving watermark set the rows were read at.
    pub watermark_set: &'a WatermarkSet,
    /// The metric registry version (report provenance).
    pub metric_registry_version: &'a str,
    /// The pricing-table version spend metrics read against.
    pub price_table_version: &'a str,
    /// The interval confidence (ppm; `950_000` = 95 %).
    pub confidence_ppm: i64,
    /// The resampling draws (preview 200 / confirmatory 2000 — OQ-366).
    pub resampling_draws: u64,
    /// The declared resampling seed.
    pub seed: u64,
}

/// Decode a required `filters` string member.
fn filter_str<'f>(filters: &'f Json, key: &str) -> Result<&'f str, AnalysisError> {
    filters
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| AnalysisError::BadSpec {
            member: format!("query.filters.{key}"),
            detail: "missing or not a string".into(),
        })
}

fn opt_filter_str<'f>(filters: &'f Json, key: &str) -> Option<&'f str> {
    filters.get(key).and_then(Json::as_str)
}

/// `filters` as an object (`{}` when absent).
fn filters_of(spec: &AnalysisSpec) -> Json {
    spec.query.filters.clone().unwrap_or(Json::Null)
}

/// Resolve the `ArmSpec` for `arm` (the match input `compare` reads).
fn arm_spec<'i>(input: &'i AnalysisInput<'i>, arm: &str) -> Result<&'i ArmSpec, AnalysisError> {
    input
        .arm_specs
        .iter()
        .find(|(id, _)| id == arm)
        .map(|(_, s)| s)
        .ok_or_else(|| AnalysisError::BadSpec {
            member: "arm_specs".into(),
            detail: format!("no resolved ArmSpec for arm {arm}"),
        })
}

/// The declarations for the spec's metric names (`UnknownMetric` is a
/// typed refusal — a name that resolves to nothing never silently drops).
fn decls_for<'i>(
    input: &'i AnalysisInput<'i>,
    metrics: &[String],
) -> Result<Vec<&'i MetricDeclaration>, AnalysisError> {
    metrics
        .iter()
        .map(|name| {
            input
                .declarations
                .iter()
                .find(|d| &d.name == name)
                .ok_or_else(|| AnalysisError::UnknownMetric { name: name.clone() })
        })
        .collect()
}

/// Project every selected row into an `EvalRun`. Non-terminal (`open`)
/// rows are *excluded and recorded* — returned in `(runs, not_run)` —
/// never silently dropped (CC3); every other projection failure is a
/// typed refusal.
pub fn project_runs(
    input: &AnalysisInput<'_>,
) -> Result<(Vec<EvalRun>, Vec<String>), AnalysisError> {
    let mut runs = Vec::new();
    let mut not_run = Vec::new();
    for row in input.rows {
        let run_id = row.key.run_id.clone();
        let manifest = input.manifests.get(&run_id);
        let facts = input.facts.get(&run_id).cloned().unwrap_or_default();
        let task_ctx = input.tasks.iter().find(|t| {
            row.coordinates
                .task
                .as_ref()
                .and_then(|c| c.get("task_id"))
                .and_then(Json::as_str)
                == Some(t.task_id.as_str())
                || manifest
                    .and_then(|m| m.task_ref.as_ref())
                    .map(|tr| tr.task_id == t.task_id)
                    .unwrap_or(false)
        });
        let suite_ctx = input.suites.iter().find(|s| {
            row.coordinates
                .task
                .as_ref()
                .and_then(|c| c.get("suite_id"))
                .and_then(Json::as_str)
                == Some(s.suite_id.as_str())
                || manifest
                    .and_then(|m| m.task_ref.as_ref())
                    .map(|tr| tr.suite_id == s.suite_id)
                    .unwrap_or(false)
        });
        match eval_run(row, manifest, facts, task_ctx, suite_ctx) {
            Ok(r) => runs.push(r),
            Err(AnalysisError::RowNotTerminal { .. }) => not_run.push(run_id),
            Err(e) => return Err(e),
        }
    }
    Ok((runs, not_run))
}

/// One A1 per-cell detail row — the `pass^k` table and
/// `per_task_consistency` the `MetricCell` record has no member for
/// (§6.4 A1's output list; carried on the report body, never dropped).
fn cell_detail(
    decl: &MetricDeclaration,
    cell: &hh_lab::analysis::MetricCell,
    stratum: &str,
    sruns: &[&EvalRun],
) -> Json {
    // Per-task reducer values (the same rule `render_cell` applies) —
    // recomputed here only to expose the distribution the cell summarizes
    // (the estimator itself runs once, inside `render_cell`).
    let mut task_values: Vec<(String, i64)> = Vec::new();
    let mut by_task: BTreeMap<&str, Vec<&EvalRun>> = BTreeMap::new();
    for r in sruns {
        if r.veto_tripped.is_empty() && decl.outcome_class_policy.in_denominator(r.outcome_class) {
            by_task.entry(r.task_id.as_str()).or_default().push(*r);
        }
    }
    let mut per_task_rows = Vec::new();
    for (task_id, truns) in &by_task {
        let mut c = 0u64;
        let mut n = 0u64;
        let mut vals = Vec::new();
        for r in truns {
            n += 1;
            if let Some(v) = hh_eval::scorecard::run_numeric(decl, r) {
                vals.push(v);
                if v > 0 {
                    c += 1;
                }
            }
        }
        let task_value = match decl.replicate_reducer {
            hh_ontology::eval::ReplicateReducer::PassK(k) => hh_eval::stats::pass_k(c, n, k as u64),
            hh_ontology::eval::ReplicateReducer::PassAt(k) => {
                hh_eval::stats::pass_at_k(c, n, k as u64)
            }
            hh_ontology::eval::ReplicateReducer::Mean => hh_eval::stats::mean(&vals),
            hh_ontology::eval::ReplicateReducer::Median => hh_eval::stats::median(&vals),
            hh_ontology::eval::ReplicateReducer::Max => vals.iter().copied().max(),
            hh_ontology::eval::ReplicateReducer::AtLeast(k) => {
                Some(if c >= k as u64 { stats::PPM } else { 0 })
            }
            hh_ontology::eval::ReplicateReducer::Collect => hh_eval::stats::median(&vals),
        };
        if let Some(v) = task_value {
            task_values.push((task_id.to_string(), v));
        }
        // `pass^k` for `k ≤ n` — the full table for small n, the declared
        // reducer's k otherwise (n > 8 lists the stepped grid; the (c,n)
        // pair keeps every k recomputable).
        let ks: Vec<u64> = if n <= 8 {
            (1..=n).collect()
        } else {
            vec![1, 2, 3, 5, 8]
        };
        let pass_k_rows: Vec<Json> = ks
            .iter()
            .map(|&k| {
                Json::obj([
                    ("k", Json::Int(k as i64)),
                    (
                        "pass_k",
                        hh_eval::stats::pass_k(c, n, k).map_or(
                            Json::obj([("n/a", Json::str("estimator_undefined"))]),
                            Json::Int,
                        ),
                    ),
                ])
            })
            .collect();
        per_task_rows.push(Json::obj([
            ("task_id", Json::str(*task_id)),
            ("c", Json::Int(c as i64)),
            ("n", Json::Int(n as i64)),
            ("pass_k", Json::Arr(pass_k_rows)),
        ]));
    }
    // `per_task_consistency` — the share of probed tasks whose reducer
    // value agrees with the cell point's majority direction
    // (`≥ 0.5 ⇔ point ≥ 0.5` for ppm-bounded metrics; the honest
    // "does every task tell the same story" check).
    let point_pos = match &cell.point {
        MetricValueKind::Decimal(v) => Some(*v >= stats::PPM / 2),
        MetricValueKind::Bool(b) => Some(*b),
        _ => None,
    };
    let consistency = point_pos.map(|pp| {
        let agree = task_values
            .iter()
            .filter(|(_, v)| (*v >= stats::PPM / 2) == pp)
            .count() as i128;
        (agree * stats::PPM as i128 / task_values.len().max(1) as i128) as i64
    });
    Json::obj([
        ("metric", Json::str(&decl.name)),
        ("stratum", Json::str(stratum)),
        ("per_task", Json::Arr(per_task_rows)),
        (
            "per_task_consistency_ppm",
            consistency.map_or(
                Json::obj([("n/a", Json::str("estimator_undefined"))]),
                Json::Int,
            ),
        ),
    ])
}

/// A1 `summarize` — one `ConfigurationSummary` per selected
/// configuration; per-stratum cells (strata never silently pooled —
/// ADR-0157 D3); the metric cells reuse `hh_eval::scorecard::render_cell`
/// (the one estimator implementation, CC1).
fn run_summarize(
    spec: &AnalysisSpec,
    input: &AnalysisInput<'_>,
    runs: &[EvalRun],
) -> Result<(Vec<ConfigurationSummary>, Vec<Json>), AnalysisError> {
    let decls = decls_for(input, &spec.query.metrics)?;
    let mut by_config: BTreeMap<String, Vec<&EvalRun>> = BTreeMap::new();
    for r in runs {
        by_config
            .entry(r.configuration_id.clone())
            .or_default()
            .push(r);
    }
    let mut summaries = Vec::new();
    let mut details = Vec::new();
    for (config_id, cruns) in &by_config {
        let mut cells = Vec::new();
        for decl in &decls {
            let strata: BTreeSet<String> =
                cruns.iter().map(|r| r.stratum.name().to_string()).collect();
            for stratum in strata {
                let sruns: Vec<&EvalRun> = cruns
                    .iter()
                    .filter(|r| r.stratum.name() == stratum)
                    .copied()
                    .collect();
                let tasks_map: BTreeMap<&str, &TaskContext> = input
                    .tasks
                    .iter()
                    .map(|t| (t.task_id.as_str(), t))
                    .collect();
                let cell = render_cell(
                    decl,
                    &sruns,
                    Some(stratum.clone()),
                    &tasks_map,
                    input.confidence_ppm,
                    config_id,
                );
                details.push(Json::obj([
                    ("configuration_id", Json::str(config_id)),
                    ("detail", cell_detail(decl, &cell, &stratum, &sruns)),
                ]));
                cells.push(cell);
            }
        }
        // Vetoed successes counted beside (never inside) the headline.
        let mut vetoed_successes = 0u64;
        for r in cruns {
            if !r.veto_tripped.is_empty() {
                let positive = decls.iter().any(|d| {
                    r.values
                        .iter()
                        .find(|v| v.metric_ref == d.name)
                        .map(|v| match &v.value {
                            MetricValueKind::Bool(b) => *b,
                            MetricValueKind::Decimal(v) => *v > 0,
                            _ => false,
                        })
                        .unwrap_or(false)
                });
                if positive {
                    vetoed_successes += 1;
                }
            }
        }
        let mut run_ids: Vec<String> = cruns.iter().map(|r| r.run_id.clone()).collect();
        run_ids.sort();
        summaries.push(ConfigurationSummary {
            configuration_id: config_id.clone(),
            participant_class: cruns
                .first()
                .map(|r| r.participant_class.as_str().to_string())
                .unwrap_or_default(),
            run_ids,
            cells,
            vetoed_successes,
            portability: MetricValueKind::Na(NaReason::NotRun),
        });
    }
    Ok((summaries, details))
}

/// Build the `CompareInput` for one arm pair (the resolved `ArmSpec`s are
/// `[a, b]`-ordered for `validate_match`).
fn compare_input<'i>(
    spec: &'i AnalysisSpec,
    input: &'i AnalysisInput<'i>,
    runs: &'i [EvalRun],
    arms: &'i [ArmSpec],
    arm_a: &'i str,
    arm_b: &'i str,
    varied: Option<&'i str>,
) -> CompareInput<'i> {
    CompareInput {
        arm_a,
        arm_b,
        metrics: &spec.query.metrics,
        declarations: input.declarations,
        runs,
        tasks: input.tasks,
        design: input.design.expect("caller gates design"),
        arm_specs: arms,
        varied_factor: varied,
        confidence_ppm: input.confidence_ppm,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: false,
        family_size: None,
    }
}

/// `analyze(spec, input)` — the §6.4 kernel entry point.
pub fn analyze(
    spec: &AnalysisSpec,
    input: &AnalysisInput<'_>,
) -> Result<AnalysisOutcome, AnalysisError> {
    let (runs, not_run) = project_runs(input)?;
    let filters = filters_of(spec);
    let mut summaries = Vec::new();
    let mut comparisons: Vec<ComparisonReport> = Vec::new();
    let mut contrasts: Vec<Json> = Vec::new();
    let mut equivalence: Option<Json> = None;
    let mut transfer_profile: Option<Json> = None;
    let mut per_task_tables: Vec<Json> = Vec::new();
    let mut summary_details: Vec<Json> = Vec::new();
    let mut frontier: Option<Json> = None;

    match spec.kind.as_str() {
        "summarize" => {
            let (s, d) = run_summarize(spec, input, &runs)?;
            summaries = s;
            summary_details = d;
        }
        "compare" | "transfer" | "equivalence" | "interaction" => {
            input.design.ok_or_else(|| AnalysisError::MissingSpecRef {
                kind: spec.kind.clone(),
            })?;
            // Two arm-pairs for the contrast; one for the rest.
            let pairs: Vec<(String, String)> = if spec.kind == "interaction" {
                let c = filters
                    .get("contrast")
                    .ok_or_else(|| AnalysisError::BadSpec {
                        member: "query.filters.contrast".into(),
                        detail: "interaction requires {pair_a, pair_b}".into(),
                    })?;
                let pair = |k: &str| -> Result<(String, String), AnalysisError> {
                    let p = c.get(k).ok_or_else(|| AnalysisError::BadSpec {
                        member: format!("query.filters.contrast.{k}"),
                        detail: "missing".into(),
                    })?;
                    Ok((
                        filter_str(p, "arm_a")?.to_string(),
                        filter_str(p, "arm_b")?.to_string(),
                    ))
                };
                vec![pair("pair_a")?, pair("pair_b")?]
            } else {
                vec![(
                    filter_str(&filters, "arm_a")?.to_string(),
                    filter_str(&filters, "arm_b")?.to_string(),
                )]
            };
            let varied = opt_filter_str(&filters, "varied_factor");
            let mut outcomes: Vec<CompareOutcome> = Vec::new();
            for (a, b) in &pairs {
                let arms = [arm_spec(input, a)?.clone(), arm_spec(input, b)?.clone()];
                let inp = compare_input(spec, input, &runs, &arms, a, b, varied);
                let out = match spec.kind.as_str() {
                    "transfer" => {
                        let held_factor = filters
                            .get("transfer")
                            .and_then(|t| t.get("held_out_factor"))
                            .and_then(Json::as_str)
                            .ok_or_else(|| AnalysisError::BadSpec {
                                member: "query.filters.transfer.held_out_factor".into(),
                                detail: "transfer requires a held-out factor".into(),
                            })?;
                        // `transfer.main_plane = true` runs the pooled
                        // comparison first — its `{point, interval}` is
                        // `Δ_in`, the ratio's denominator (ADR-0159 D4).
                        let main = if matches!(
                            filters.get("transfer").and_then(|t| t.get("main_plane")),
                            Some(Json::Bool(true))
                        ) {
                            Some(compare(&inp)?)
                        } else {
                            None
                        };
                        let (out, profile) =
                            run_transfer(spec, input, &runs, &inp, held_factor, main.as_ref())?;
                        transfer_profile = Some(profile);
                        out
                    }
                    "equivalence" => {
                        let prereg = input.pre_registration.ok_or_else(|| {
                            AnalysisError::MissingSpecRef {
                                kind: "equivalence (pre_registration)".into(),
                            }
                        })?;
                        let reference = filter_str(&filters, "reference")?;
                        let candidate = filter_str(&filters, "candidate")?;
                        let suite_ref = opt_filter_str(&filters, "suite_ref").unwrap_or("");
                        let (rep, out) = benefits::equivalence_run_full(
                            reference, candidate, suite_ref, prereg, &inp, None,
                        )?;
                        equivalence = Some(rep.to_json());
                        out
                    }
                    _ => compare(&inp)?,
                };
                outcomes.push(out);
            }
            // Gather reports + per-task tables.
            for out in &outcomes {
                for (i, r) in out.reports.iter().enumerate() {
                    comparisons.push(r.clone());
                    per_task_tables.push(Json::Arr(
                        out.per_task
                            .get(i)
                            .cloned()
                            .unwrap_or_default()
                            .iter()
                            .map(|e| e.to_json())
                            .collect(),
                    ));
                }
            }
            if spec.kind == "interaction" {
                let [oa, ob] = outcomes.as_slice() else {
                    return Err(AnalysisError::BadSpec {
                        member: "contrast".into(),
                        detail: "interaction needs exactly two outcomes".into(),
                    });
                };
                for metric in &spec.query.metrics {
                    let est = contrast(metric, oa, ob, input.confidence_ppm)?;
                    contrasts.push(est.to_json());
                }
            }
        }
        "frontier" => {
            // M2 capability–cost frontier (§8.2) — a comparison surface:
            // the same `validate_match` gate `compare` runs (matched_cap /
            // iso_cost arms only; an exploratory `mode:none` spec may
            // execute but never reports a frontier — ExploratoryNoMatch).
            let metric = decls_for(input, &spec.query.metrics)?
                .into_iter()
                .next()
                .ok_or_else(|| AnalysisError::BadSpec {
                    member: "query.metrics".into(),
                    detail: "frontier requires at least one metric".into(),
                })?;
            if let Some(r) = runs.iter().find(|r| !r.comparable) {
                return Err(AnalysisError::Compare(CompareError::ExploratoryRun {
                    run_id: r.run_id.clone(),
                }));
            }
            let comparable: Vec<&EvalRun> = runs.iter().collect();
            let arm_ids: BTreeSet<&str> = comparable.iter().map(|r| r.arm_id.as_str()).collect();
            if arm_ids.len() < 2 {
                return Err(AnalysisError::BadSpec {
                    member: "query.filters".into(),
                    detail: "frontier requires runs from at least two arms".into(),
                });
            }
            let arms: Vec<ArmSpec> = arm_ids
                .iter()
                .map(|a| arm_spec(input, a).cloned())
                .collect::<Result<_, _>>()?;
            hh_budget::matchspec::validate_match(&arms)
                .map_err(CompareError::Match)
                .map_err(AnalysisError::Compare)?;
            if arms[0].match_spec.as_ref().map(|s| s.mode)
                == Some(hh_budget::matchspec::MatchMode::None)
            {
                // Belt-and-braces — validate_match already refuses.
                return Err(AnalysisError::Compare(CompareError::Match(
                    hh_budget::MatchError {
                        arm: Some(0),
                        refusal: hh_budget::MatchRefusal::IncommensurableMatch {
                            reason: hh_budget::RefusalReason::ExploratoryNoMatch,
                            dimension: None,
                            enforceability: None,
                        },
                    },
                )));
            }
            frontier = Some(crate::frontier::frontier_report(
                metric,
                &comparable,
                input.facts,
            ));
        }
        other => {
            return Err(AnalysisError::KindNotImplemented {
                kind: other.to_string(),
            })
        }
    }

    // A12 — the multiplicity pass over every produced comparison.
    let delta_sets: Vec<Vec<i64>> = outcomes_deltas(&per_task_tables);
    apply_multiplicity(
        &mut comparisons,
        &delta_sets,
        input.pre_registration,
        input.resampling_draws,
        input.seed,
    );

    Ok(assemble(
        spec,
        input,
        summaries,
        summary_details,
        comparisons,
        contrasts,
        equivalence,
        transfer_profile,
        frontier,
        not_run,
    ))
}

/// `transfer` rows — the per-level held-out comparisons + the
/// `TransferProfile` body member (ADR-0159 D4; AC-R-2.10.4-6):
/// `transfer_ratio = Δ_out / Δ_in` per held-out level (`n/a` when the
/// `Δ_in` interval includes zero — a ratio off an unidentified denominator
/// is never reported), `sign_stability` = the ppm share of probed levels
/// whose `Δ_out` sign agrees with `Δ_in`, `conditionality_region` = the
/// agreeing levels (a sign-flipped family is *outside* the region, listed
/// never dropped), `unknown_levels` = levels whose comparison yielded no
/// usable paired effect.
///
/// The ratio's interval scales the `Δ_out` interval by the fixed `Δ_in`
/// point (the Stage-3 approximation — Δ_in's own variance is documented,
/// not modelled; a denominator interval crossing zero already forces
/// `n/a`).
fn run_transfer(
    _spec: &AnalysisSpec,
    _input: &AnalysisInput<'_>,
    runs: &[EvalRun],
    inp: &CompareInput<'_>,
    held_factor: &str,
    main: Option<&CompareOutcome>,
) -> Result<(CompareOutcome, Json), AnalysisError> {
    let level_of = |r: &EvalRun| -> String {
        match held_factor {
            // A run's held-out level on the model axis is its snapshot
            // join (one cell per snapshot tuple — never a single role's
            // silently).
            "model_snapshot" => r
                .model_snapshots
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("+"),
            "environment" => r.environment_family.name().to_string(),
            // `task_family` — the Stage-3 family coordinate is the suite
            // binding (task families are suite-scoped).
            _ => r.suite_id.clone(),
        }
    };
    let main_point = main
        .and_then(|m| m.reports.first())
        .and_then(|r| r.paired_effect.point.as_ref())
        .and_then(Json::as_int);
    let main_interval = main
        .and_then(|m| m.reports.first())
        .and_then(|r| r.paired_effect.interval.as_ref())
        .map(|i| {
            (
                i.get("lo").and_then(Json::as_int),
                i.get("hi").and_then(Json::as_int),
            )
        });
    // `n/a` rule: a denominator whose interval includes zero carries no
    // sign — ratios against it are `n/a{estimator_undefined}`.
    let denom_identified = matches!(
        (main_point, main_interval),
        (Some(p), Some((Some(lo), Some(hi)))) if p != 0 && !(lo <= 0 && hi >= 0)
    );
    let main_delta = if denom_identified { main_point } else { None };

    let mut by_level: BTreeMap<String, Vec<EvalRun>> = BTreeMap::new();
    for r in runs {
        by_level.entry(level_of(r)).or_default().push(r.clone());
    }
    let mut merged = CompareOutcome {
        reports: Vec::new(),
        per_task: Vec::new(),
        deviation: hh_eval::compare::DeviationReport::default(),
        outcome_counts: Default::default(),
    };
    let mut levels = Vec::new();
    let mut unknown_levels: Vec<String> = Vec::new();
    let mut agreeing = 0u64;
    let mut probed = 0u64;
    let mut agreeing_levels: Vec<String> = Vec::new();
    for (level, lruns) in &by_level {
        let lin = CompareInput {
            runs: lruns,
            ..inp.clone()
        };
        let out = match benefits::transfer(&lin, held_factor, main_delta) {
            Ok(o) => o,
            // A level with no pairable tasks is `unknown` — listed, never
            // silently dropped (CC3).
            Err(_) => {
                unknown_levels.push(level.clone());
                continue;
            }
        };
        probed += 1;
        let mut level_rows = Vec::new();
        let mut level_agrees = true;
        for rep in &out.reports {
            let point = rep.paired_effect.point.as_ref().and_then(Json::as_int);
            let interval = rep.paired_effect.interval.as_ref().map(|i| {
                (
                    i.get("lo").and_then(Json::as_int),
                    i.get("hi").and_then(Json::as_int),
                )
            });
            // Sign agreement — a zero/undefined `Δ_out` never agrees (a
            // null reading is not evidence of transfer, nor against it).
            let agrees = match (main_point, point) {
                (Some(mp), Some(p)) if p != 0 => (mp > 0) == (p > 0),
                _ => false,
            };
            level_agrees &= agrees;
            // The ratio interval: `Δ_out`'s interval scaled by the fixed
            // `Δ_in` point (sign-aware — a negative denominator swaps the
            // endpoints).
            let ratio_interval = match (main_delta, interval) {
                (Some(d), Some((Some(lo), Some(hi)))) if d != 0 => {
                    let (a, b) = if d > 0 {
                        (
                            lo as i128 * stats::PPM as i128 / d as i128,
                            hi as i128 * stats::PPM as i128 / d as i128,
                        )
                    } else {
                        (
                            hi as i128 * stats::PPM as i128 / d as i128,
                            lo as i128 * stats::PPM as i128 / d as i128,
                        )
                    };
                    Json::obj([("lo", Json::Int(a as i64)), ("hi", Json::Int(b as i64))])
                }
                _ => Json::obj([("n/a", Json::str("estimator_undefined"))]),
            };
            let ratio_point = rep
                .estimated
                .as_ref()
                .and_then(|e| e.get("transfer_ratio_ppm").cloned())
                .unwrap_or_else(|| Json::obj([("n/a", Json::str("estimator_undefined"))]));
            level_rows.push(Json::obj([
                ("metric", Json::str(&rep.metric)),
                ("delta_point", point.map(Json::Int).unwrap_or(Json::Null)),
                ("transfer_ratio_ppm", ratio_point),
                ("transfer_ratio_interval", ratio_interval),
                ("sign_agrees", Json::Bool(agrees)),
            ]));
        }
        merged.reports.extend(out.reports.iter().cloned());
        merged.per_task.extend(out.per_task.iter().cloned());
        // CC3 — the per-level deviation/outcome accounting aggregates up;
        // nothing is silently dropped in the merge.
        merged
            .deviation
            .excluded_runs
            .extend(out.deviation.excluded_runs.iter().cloned());
        for (arm, (d, t)) in &out.deviation.arm_counts {
            let e = merged
                .deviation
                .arm_counts
                .entry(arm.clone())
                .or_insert((0, 0));
            e.0 += d;
            e.1 += t;
        }
        for (arm, counts) in &out.outcome_counts {
            let e = merged.outcome_counts.entry(arm.clone()).or_default();
            for (class, n) in counts {
                *e.entry(class.clone()).or_insert(0) += n;
            }
        }
        if level_agrees {
            agreeing += 1;
            agreeing_levels.push(level.clone());
        }
        levels.push(Json::obj([
            ("level", Json::str(level)),
            ("sign_agrees", Json::Bool(level_agrees)),
            ("metrics", Json::Arr(level_rows)),
        ]));
    }
    let sign_stability = if probed == 0 {
        Json::obj([("n/a", Json::str("estimator_undefined"))])
    } else {
        Json::Int((agreeing * stats::PPM as u64 / probed) as i64)
    };
    let conditionality: Vec<Json> = agreeing_levels.iter().map(Json::str).collect();
    let profile = Json::obj([
        ("held_out_factor", Json::str(held_factor)),
        (
            "delta_in",
            Json::obj([
                ("point", main_point.map(Json::Int).unwrap_or(Json::Null)),
                (
                    "interval",
                    main_interval
                        .and_then(|(lo, hi)| lo.zip(hi))
                        .map(|(lo, hi)| Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))]))
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        ("levels", Json::Arr(levels)),
        ("sign_stability_ppm", sign_stability),
        ("conditionality_region", Json::Arr(conditionality)),
        (
            "unknown_levels",
            Json::Arr(unknown_levels.iter().map(Json::str).collect()),
        ),
    ]);
    Ok((merged, profile))
}

/// Pull the concrete per-task deltas out of the serialized effect tables
/// (the multiplicity pass's raw-p input — `n/a` cells never enter).
fn outcomes_deltas(tables: &[Json]) -> Vec<Vec<i64>> {
    tables
        .iter()
        .map(|t| match t {
            Json::Arr(rows) => rows
                .iter()
                .filter_map(|e| e.get("delta").and_then(Json::as_int))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}
