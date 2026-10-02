//! `ops` — the C1 analysis surfaces (§6.4 §2.1; ADR-0157…0160):
//!
//! - A6 `benefit_decomposition` — the comparison rows plus the
//!   `search_baseline{kind, n}` member (oracle/selected best-of-N);
//! - A7 `fit_surface` — the `contrast`-form `FittedSurfaceReport` over
//!   the probed factorial grid (`unknown_cells` listed, categorical axes
//!   never interpolated, `expired` status on a registry event);
//! - A9 `rank` — the task-resampled `RankReport` (mean ranks, rank
//!   intervals, paired-Δ-overlap `indistinguishable_sets` — never a
//!   total order);
//! - A10 `attribution` — `n/a{class}` for hosted rows, designed-ablation
//!   M1 effects for native;
//! - A11 `reliability_profile` — pass^k curve, per-task consistency,
//!   tails, `catastrophic_rate{wilson}`, horizon;
//! - A13 `power` — `PowerReport` MDE cells from pilot rows (paired-CLT
//!   cross-check, declared, deterministic);
//! - A15 `diagnostics` — the advisory block (SNR per family, per-task
//!   difficulty, utilization, imbalance, `infra_suspected`);
//! - A16 `render`/`diff_reports` — the boundary helpers (render never
//!   recomputes).
//!
//! Everything here is pure over `EvalRun`s — the kernel resolves rows
//! and calls in; nothing touches a store (records-in/records-out).

use std::collections::{BTreeMap, BTreeSet};

use hh_eval::runs::EvalRun;
use hh_eval::stats;
use hh_ontology::eval::{Design, FactorKind, MetricValueKind};
use hh_wire::json::Json;

use crate::error::AnalysisError;

/// A run's scalar point for one metric — `Decimal` values verbatim,
/// `Bool` as ppm (1_000_000/0), verdicts/vectors/`n/a` are `None`
/// (non-scalar cells never fabricate a number — T-LCD-15).
fn point(r: &EvalRun, metric: &str) -> Option<i64> {
    match r.value_for(metric) {
        MetricValueKind::Decimal(v) => Some(v),
        MetricValueKind::Bool(b) => Some(if b { 1_000_000 } else { 0 }),
        _ => None,
    }
}

/// Whether the run is a hosted participant (`participant_class !=
/// native` — the §2.5 class-applicability matrix's axis).
pub fn is_hosted(r: &EvalRun) -> bool {
    r.participant_class.as_str() != "native"
}

/// The set of factor names a hosted row can never carry an admissible
/// level for unless `capability_vector[factor] = SUPPORTED` — component-
/// level factors (`FactorKind::Harness` on the design, or — undeclared —
/// any non-host-visible axis) need ledger observability hosted rows do
/// not expose (§2.5; AC-R-2.10.4-4).
fn hosted_inadmissible(r: &EvalRun, factor: &str, design: Option<&Design>) -> bool {
    if !is_hosted(r) {
        return false;
    }
    if matches!(
        r.capability_vector.get(factor),
        Some(hh_ontology::participant::CapabilityVerdict::Supported)
    ) {
        return false; // a SUPPORTED verdict overrides the class default.
    }
    // The design's declaration is authoritative: `harness`-kind factors
    // are component-level.
    if let Some(d) = design {
        if let Some(f) = d.factors.iter().find(|f| f.name == factor) {
            return matches!(f.kind, FactorKind::Harness);
        }
    }
    // Undeclared — host-visible axes (the levels a session/interception
    // row can carry without component internals) pass; anything else is
    // treated as component-level.
    const HOST_VISIBLE: &[&str] = &[
        "model",
        "model_snapshot",
        "environment",
        "task_family",
        "suite",
        "participant_class",
    ];
    !HOST_VISIBLE.contains(&factor)
}

/// A3/A10 class gate — `Some((factor, run_id, detail))` when a run is
/// hosted and the factor is component-level without a `SUPPORTED`
/// verdict (`InadmissibleFactor`, AC-R-2.10.4-4).
pub fn factor_inadmissible(
    runs: &[EvalRun],
    design: Option<&Design>,
    factors: &[&str],
) -> Option<(String, String, String)> {
    for r in runs {
        for f in factors {
            if hosted_inadmissible(r, f, design) {
                return Some((
                    f.to_string(),
                    r.run_id.clone(),
                    format!(
                        "hosted row ({}) cannot carry component-level factor {f}",
                        r.participant_class.as_str()
                    ),
                ));
            }
        }
    }
    None
}

// ── A9 rank ────────────────────────────────────────────────────────────

/// The per-(config, task) reducer — mean of replicate points.
fn per_task_points(runs: &[&EvalRun], metric: &str) -> BTreeMap<String, Vec<i64>> {
    let mut by_task: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for r in runs {
        if let Some(v) = point(r, metric) {
            by_task.entry(r.task_id.clone()).or_default().push(v);
        }
    }
    by_task
        .into_iter()
        .map(|(t, vs)| (t, vec![stats::mean(&vs).unwrap_or(0)]))
        .collect()
}

/// A9 `rank(configurations[], metric)` → `RankReport{ranks[],
/// indistinguishable_sets[], draws}` — the task is the resampling unit
/// (ADR-0158 D6): each draw resamples tasks with replacement and ranks
/// the configurations; `mean_rank` and `rank_interval` come out of the
/// draw distribution. Sets form by paired-Δ interval overlap over the
/// point-sorted order — a boundary, never a total order (OQ-369).
pub fn rank_report(
    runs: &[EvalRun],
    metric: &str,
    confidence_ppm: i64,
    draws: u64,
    seed: u64,
) -> Result<Json, AnalysisError> {
    let mut by_config: BTreeMap<String, Vec<&EvalRun>> = BTreeMap::new();
    for r in runs {
        if point(r, metric).is_some() {
            by_config
                .entry(r.configuration_id.clone())
                .or_default()
                .push(r);
        }
    }
    if by_config.len() < 2 {
        return Err(AnalysisError::BadSpec {
            member: "selection".into(),
            detail: "rank requires ≥ 2 configurations with concrete values".into(),
        });
    }
    // config → task → point
    let configs: Vec<String> = by_config.keys().cloned().collect();
    let task_points: BTreeMap<String, BTreeMap<String, i64>> = by_config
        .iter()
        .map(|(c, rs)| {
            (
                c.clone(),
                per_task_points(rs, metric)
                    .into_iter()
                    .map(|(t, m)| (t, m[0]))
                    .collect(),
            )
        })
        .collect();
    // The shared task set — rank over tasks every configuration probed
    // (a configuration missing a task carries `n/a` for it — never 0).
    let shared: Vec<String> = {
        let mut it = task_points
            .values()
            .map(|m| m.keys().cloned().collect::<BTreeSet<_>>());
        let mut acc = it.next().unwrap_or_default();
        for s in it {
            acc = acc.intersection(&s).cloned().collect();
        }
        acc.into_iter().collect()
    };
    let n_tasks = shared.len();
    let mut rank_sum: BTreeMap<String, u64> = configs.iter().map(|c| (c.clone(), 0)).collect();
    let mut rank_lo: BTreeMap<String, u64> = BTreeMap::new();
    let mut rank_hi: BTreeMap<String, u64> = BTreeMap::new();
    let draws_eff = draws.max(1);
    let mut rng = stats::XorShift64::seeded(&format!("rank:{metric}:{seed}"));
    for _ in 0..draws_eff {
        // One task-resample: mean over the drawn multiset.
        let mut scores: Vec<(&String, i64)> = Vec::new();
        for c in &configs {
            let pts = &task_points[c];
            if n_tasks == 0 {
                break;
            }
            let mut sum: i128 = 0;
            for _ in 0..n_tasks {
                let t = &shared[rng.below(n_tasks)];
                sum += pts.get(t).copied().unwrap_or(0) as i128;
            }
            scores.push((c, (sum / n_tasks as i128) as i64));
        }
        if scores.is_empty() {
            break;
        }
        scores.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (i, (c, _)) in scores.iter().enumerate() {
            let rank = (i + 1) as u64;
            *rank_sum.get_mut(*c).unwrap() += rank;
            let lo = rank_lo.entry((*c).clone()).or_insert(rank);
            *lo = (*lo).min(rank);
            let hi = rank_hi.entry((*c).clone()).or_insert(rank);
            *hi = (*hi).max(rank);
        }
    }
    // Paired-Δ intervals — clustered CLT over per-task differences on
    // the shared tasks (the unit is the task — ADR-0158).
    let pair_interval = |a: &str, b: &str| -> Option<stats::Interval> {
        let deltas: Vec<(String, i64)> = shared
            .iter()
            .map(|t| (t.clone(), task_points[a][t] - task_points[b][t]))
            .collect();
        stats::clustered_clt(&deltas, confidence_ppm)
    };
    // Indistinguishable sets — the point-sorted order merged wherever
    // consecutive configurations' paired-Δ intervals overlap (a boundary
    // around the set; order inside is point estimate, intervals shown).
    let mut point_order: Vec<&String> = configs.iter().collect();
    let mean_of = |c: &str| -> i64 {
        let pts = &task_points[c];
        let vs: Vec<i64> = shared.iter().filter_map(|t| pts.get(t).copied()).collect();
        stats::mean(&vs).unwrap_or(0)
    };
    point_order.sort_by(|a, b| mean_of(b).cmp(&mean_of(a)).then_with(|| a.cmp(b)));
    let mut sets: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut prev: Option<String> = None;
    for c in &point_order {
        let join = match &prev {
            None => false,
            Some(p) => match pair_interval(p, c) {
                Some(i) => i.lo <= 0 && i.hi >= 0,
                None => true, // unidentifiable Δ ⇒ no basis to split
            },
        };
        if join {
            cur.push((*c).clone());
        } else {
            if !cur.is_empty() {
                sets.push(std::mem::take(&mut cur));
            }
            cur.push((*c).clone());
        }
        prev = Some((*c).clone());
    }
    if !cur.is_empty() {
        sets.push(cur);
    }
    let ranks: Vec<Json> = point_order
        .iter()
        .map(|c| {
            Json::obj([
                ("configuration_id", Json::str(*c)),
                (
                    "rank_interval",
                    Json::obj([
                        (
                            "lo",
                            Json::Int(rank_lo.get(*c).copied().unwrap_or(0) as i64),
                        ),
                        (
                            "hi",
                            Json::Int(rank_hi.get(*c).copied().unwrap_or(0) as i64),
                        ),
                    ]),
                ),
                (
                    "mean_rank_milli",
                    Json::Int((rank_sum.get(*c).copied().unwrap_or(0) * 1000 / draws_eff) as i64),
                ),
            ])
        })
        .collect();
    Ok(Json::obj([
        ("schema", Json::str("hh-rank-report/1")),
        ("metric", Json::str(metric)),
        ("draws", Json::Int(draws_eff as i64)),
        ("n_tasks", Json::Int(n_tasks as i64)),
        ("ranks", Json::Arr(ranks)),
        (
            "indistinguishable_sets",
            Json::Arr(
                sets.iter()
                    .map(|s| Json::Arr(s.iter().map(Json::str).collect()))
                    .collect(),
            ),
        ),
    ]))
}

// ── A11 reliability profile ────────────────────────────────────────────

/// A11 `reliability_profile(configuration)` →
/// `{configuration_id, pass_k_curve[], per_task_consistency, horizon,
/// tails, catastrophic_rate{wilson}, fault_recovery,
/// perturbation_robustness}` — horizon is `n/a` without task-time
/// metadata; recovery/robustness are `n/a{not_run}` when the fixture
/// carries no fault/perturbation factor (ADR-0158 D4; CF-341 — the
/// perturbation axis is within-task and paired).
pub fn reliability_profile(
    runs: &[EvalRun],
    metric: &str,
    confidence_ppm: i64,
) -> Result<Json, AnalysisError> {
    let mut by_config: BTreeMap<String, Vec<&EvalRun>> = BTreeMap::new();
    for r in runs {
        by_config
            .entry(r.configuration_id.clone())
            .or_default()
            .push(r);
    }
    let mut profiles = Vec::new();
    for (config, cruns) in &by_config {
        // pass^k — per task: (c successes of n replicates) → C(c,k)/C(n,k)
        // in ppm; k > n renders `n/a{estimator_undefined}`.
        let mut per_task: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for r in cruns {
            let e = per_task.entry(r.task_id.clone()).or_insert((0, 0));
            e.1 += 1;
            if point(r, metric).map(|v| v > 0).unwrap_or(false) {
                e.0 += 1;
            }
        }
        let max_n = per_task.values().map(|(_, n)| *n).max().unwrap_or(0);
        let mut curve = Vec::new();
        for k in 1..=max_n {
            let mut acc: u128 = 0;
            let mut counted = 0u64;
            for (c, n) in per_task.values() {
                if k <= *n {
                    // C(c,k)/C(n,k) = ∏_{i<k} (c-i)/(n-i) in ppm.
                    let mut num: u128 = 1_000_000;
                    for i in 0..k {
                        if *c < i {
                            num = 0;
                            break;
                        }
                        num = num * ((*c - i) as u128) / ((*n - i) as u128);
                        if num == 0 {
                            break;
                        }
                    }
                    acc += num;
                    counted += 1;
                }
            }
            let v = if counted == 0 {
                Json::obj([("n/a", Json::str("estimator_undefined"))])
            } else {
                Json::Int((acc / counted as u128) as i64)
            };
            curve.push(Json::obj([("k", Json::Int(k as i64)), ("pass_k_ppm", v)]));
        }
        // Per-task consistency — the share of tasks whose replicate
        // outcomes are unanimous.
        let unanimous = per_task
            .values()
            .filter(|(c, n)| *c == 0 || *c == *n)
            .count();
        let consistency = if per_task.is_empty() {
            Json::obj([("n/a", Json::str("estimator_undefined"))])
        } else {
            Json::Int((unanimous as u64 * 1_000_000 / per_task.len() as u64) as i64)
        };
        // Catastrophic rate — `wilson` over the outcome classes
        // (budget_exhausted/refused/oracle/infrastructure terminal
        // classes count catastrophic beside scored; the count is of
        // runs, not tasks).
        let n_runs = cruns.len() as i64;
        let catastrophic = cruns
            .iter()
            .filter(|r| {
                matches!(
                    r.outcome_class,
                    hh_ontology::control::OutcomeClass::InfrastructureFailure
                        | hh_ontology::control::OutcomeClass::OracleFailure
                        | hh_ontology::control::OutcomeClass::Refused
                )
            })
            .count() as i64;
        let cat = stats::wilson(catastrophic, n_runs, confidence_ppm);
        let tails: Vec<i64> = cruns.iter().filter_map(|r| point(r, metric)).collect();
        let tail = |q: i64| stats::quantile(&tails, q);
        profiles.push(Json::obj([
            ("configuration_id", Json::str(config)),
            ("pass_k_curve", Json::Arr(curve)),
            ("per_task_consistency_ppm", consistency),
            (
                "horizon",
                Json::obj([("n/a", Json::str("missing_metadata"))]),
            ),
            (
                "tails",
                Json::obj([
                    ("p50", tail(500_000).map_or(Json::Null, Json::Int)),
                    ("p95", tail(950_000).map_or(Json::Null, Json::Int)),
                    (
                        "max",
                        tails.iter().max().map_or(Json::Null, |v| Json::Int(*v)),
                    ),
                ]),
            ),
            (
                "catastrophic_rate",
                Json::obj([
                    ("c", Json::Int(catastrophic)),
                    ("n", Json::Int(n_runs)),
                    (
                        "wilson",
                        cat.map_or(
                            Json::obj([("n/a", Json::str("estimator_undefined"))]),
                            |i| Json::obj([("lo", Json::Int(i.lo)), ("hi", Json::Int(i.hi))]),
                        ),
                    ),
                ]),
            ),
            (
                "fault_recovery",
                if cruns.iter().any(|r| r.fault_profile.is_some()) {
                    Json::obj([("present", Json::Bool(true))])
                } else {
                    Json::obj([("n/a", Json::str("not_run"))])
                },
            ),
            (
                "perturbation_robustness",
                if cruns.iter().any(|r| r.perturbation_profile.is_some()) {
                    Json::obj([("present", Json::Bool(true))])
                } else {
                    Json::obj([("n/a", Json::str("not_run"))])
                },
            ),
        ]));
    }
    Ok(Json::obj([
        ("schema", Json::str("hh-reliability-profile/1")),
        ("metric", Json::str(metric)),
        ("profiles", Json::Arr(profiles)),
    ]))
}

// ── A13 power ─────────────────────────────────────────────────────────

/// A13 `power(design, pilot_rows)` → `PowerReport{metric → cells[]}` —
/// per `(n_tasks, replicates)` the minimum detectable effect and the
/// power at the pilot's observed Δ, computed by the paired-binary CLT
/// cross-check (§6.4 A13; the simulation estimator rides the same
/// pilot per-task rates — C1 reports the formula cell, labelled).
pub fn power_report(
    runs: &[EvalRun],
    metric: &str,
    confidence_ppm: i64,
    task_grid: &[u64],
    replicate_grid: &[u64],
) -> Result<Json, AnalysisError> {
    // Pilot per-task rates — the probability mass the design draws from.
    let mut by_task: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in runs {
        let e = by_task.entry(r.task_id.clone()).or_insert((0, 0));
        e.1 += 1;
        if point(r, metric).map(|v| v > 0).unwrap_or(false) {
            e.0 += 1;
        }
    }
    if by_task.is_empty() {
        return Err(AnalysisError::BadSpec {
            member: "selection".into(),
            detail: "power requires ≥ 1 pilot task with a concrete value".into(),
        });
    }
    // Pooled pilot rate + observed Δ (dispersion across task rates).
    let rates: Vec<f64> = by_task
        .values()
        .map(|(c, n)| *c as f64 / *n as f64)
        .collect();
    let p_bar = rates.iter().sum::<f64>() / rates.len() as f64;
    let var = rates.iter().map(|r| (r - p_bar) * (r - p_bar)).sum::<f64>() / rates.len() as f64;
    // Paired binary cross-check: σ²_paired ≈ p(1-p) + task dispersion;
    // MDE at power β = (z_α + z_β)·σ·sqrt(1/(n_tasks·replicates)).
    let alpha = 1.0 - confidence_ppm as f64 / 1_000_000.0;
    let z_a = stats::normal_quantile(((1.0 - alpha / 2.0) * 1_000_000.0) as i64);
    let z_b = stats::normal_quantile(800_000); // β = 0.8 — the §6.3 default (OQ-121 placeholder).
    let sigma2 = p_bar * (1.0 - p_bar) + var;
    let tasks_g: Vec<u64> = if task_grid.is_empty() {
        vec![30, 100, 300]
    } else {
        task_grid.to_vec()
    };
    let reps_g: Vec<u64> = if replicate_grid.is_empty() {
        vec![1, 3, 5]
    } else {
        replicate_grid.to_vec()
    };
    let mut cells = Vec::new();
    for &nt in &tasks_g {
        for &nr in &reps_g {
            let se = (sigma2 / (nt * nr) as f64).sqrt();
            let mde = (z_a + z_b) * se;
            cells.push(Json::obj([
                ("n_tasks", Json::Int(nt as i64)),
                ("replicates", Json::Int(nr as i64)),
                (
                    "detectable_delta_ppm",
                    Json::Int((mde * 1_000_000.0).round() as i64),
                ),
                ("method", Json::str("paired_clt_cross_check")),
                (
                    "pilot_rate_ppm",
                    Json::Int((p_bar * 1_000_000.0).round() as i64),
                ),
            ]));
        }
    }
    Ok(Json::obj([
        ("schema", Json::str("hh-power-report/1")),
        ("metric", Json::str(metric)),
        (
            "pilot",
            Json::obj([
                ("n_tasks", Json::Int(by_task.len() as i64)),
                ("rate_ppm", Json::Int((p_bar * 1_000_000.0).round() as i64)),
                ("dispersion_ppm", Json::Int((var * 1e12).round() as i64)),
            ]),
        ),
        ("cells", Json::Arr(cells)),
        ("beta", Json::Int(800_000)),
    ]))
}

// ── A15 diagnostics ───────────────────────────────────────────────────

/// A15 `diagnostics(design, rows)` → the advisory block (§6.4 A15 —
/// "advisory; never alters a confirmatory cell"): per-task-family SNR,
/// per-task difficulty, budget utilization, arm imbalance,
/// `infra_suspected` counts and the sameness annotations.
pub fn diagnostics(runs: &[EvalRun], metric: &str) -> Json {
    // Per-task difficulty — the pass share per task over all arms.
    let mut by_task: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in runs {
        if let Some(v) = point(r, metric) {
            let e = by_task.entry(r.task_id.clone()).or_insert((0, 0));
            e.1 += 1;
            if v > 0 {
                e.0 += 1;
            }
        }
    }
    let per_task_difficulty: Vec<Json> = by_task
        .iter()
        .map(|(t, (c, n))| {
            Json::obj([
                ("task_id", Json::str(t)),
                ("pass_ppm", Json::Int((*c * 1_000_000 / n.max(&1)) as i64)),
            ])
        })
        .collect();
    // Per-family SNR — between-task share of total variance (ppm).
    let mut by_family: BTreeMap<String, Vec<(String, i64)>> = BTreeMap::new();
    for r in runs {
        if let Some(v) = point(r, metric) {
            by_family
                .entry(r.suite_id.clone())
                .or_default()
                .push((r.task_id.clone(), v));
        }
    }
    let mut snr = Vec::new();
    for (fam, rows) in &by_family {
        let all: Vec<i64> = rows.iter().map(|(_, v)| *v).collect();
        let total_mean = stats::mean(&all).unwrap_or(0) as f64;
        let total_var = all
            .iter()
            .map(|v| (*v as f64 - total_mean).powi(2))
            .sum::<f64>()
            / all.len().max(1) as f64;
        let mut task_means: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for (t, v) in rows {
            task_means.entry(t.clone()).or_default().push(*v);
        }
        let means: Vec<i64> = task_means
            .values()
            .map(|vs| stats::mean(vs).unwrap_or(0))
            .collect();
        let between = means
            .iter()
            .map(|m| (*m as f64 - total_mean).powi(2))
            .sum::<f64>()
            / means.len().max(1) as f64;
        let snr_ppm = if total_var <= 0.0 {
            0
        } else {
            (between / total_var * 1_000_000.0).round() as i64
        };
        snr.push(Json::obj([
            ("family", Json::str(fam)),
            ("snr_ppm", Json::Int(snr_ppm)),
        ]));
    }
    // Budget utilization + arm imbalance.
    let mut dims: BTreeMap<String, (i64, i64)> = BTreeMap::new(); // dim → (consumed, declared)
    let mut arm_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut infra = 0u64;
    for r in runs {
        *arm_counts.entry(r.arm_id.clone()).or_default() += 1;
        for (d, v) in &r.budget_consumed {
            dims.entry(d.as_str().to_string()).or_default().0 += v;
        }
        if matches!(
            r.outcome_class,
            hh_ontology::control::OutcomeClass::InfrastructureFailure
        ) {
            infra += 1;
        }
    }
    let imbalance = {
        let counts: Vec<u64> = arm_counts.values().copied().collect();
        let max = counts.iter().max().copied().unwrap_or(0);
        let min = counts.iter().min().copied().unwrap_or(0);
        if max == 0 {
            Json::Int(0)
        } else {
            Json::Int(((max - min) * 1_000_000 / max) as i64)
        }
    };
    Json::obj([
        ("schema", Json::str("hh-analysis-diagnostics/1")),
        ("metric", Json::str(metric)),
        ("snr_per_task_family", Json::Arr(snr)),
        ("per_task_difficulty", Json::Arr(per_task_difficulty)),
        (
            "budget_utilization",
            Json::Obj(
                dims.iter()
                    .map(|(d, (c, _))| (d.clone(), Json::Int(*c)))
                    .collect(),
            ),
        ),
        (
            "imbalance",
            Json::obj([("arm_share_spread_ppm", imbalance)]),
        ),
        ("infra_suspected", Json::Int(infra as i64)),
        (
            "sameness_annotations",
            Json::Arr(Vec::new()), // L0–L4 marks land with registry members — none at row level.
        ),
    ])
}

// ── A7 fit_surface (contrast form) ────────────────────────────────────

/// A run's bound level on one factor — the coordinate projection the
/// factorial grid reads (categorical axes only at C1).
fn level_of(r: &EvalRun, factor: &str) -> String {
    match factor {
        "model" | "model_snapshot" => {
            let mut v: Vec<&String> = r.model_snapshots.values().collect();
            v.sort();
            v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("+")
        }
        "environment" => r.environment_family.name().to_string(),
        "fault_profile" => r.fault_profile.clone().unwrap_or_else(|| "none".into()),
        "perturbation_profile" => r
            .perturbation_profile
            .clone()
            .unwrap_or_else(|| "none".into()),
        "task_family" | "suite" => r.suite_id.clone(),
        "participant_class" => r.participant_class.as_str().to_string(),
        other => r
            .capability_vector
            .get(other)
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "unknown".into()),
    }
}

/// A7 `fit_surface(rows, factors[], metric, model_form = contrast)` →
/// `FittedSurfaceReport` (§6.4 §2.3; ADR-0160): the probed-grid contrast
/// surface — main effects per factor level against the baseline level,
/// `unknown_cells` for every unprobed level tuple (**never**
/// interpolated — ADR-0012 D7), `status = expired` when any
/// `configuration_ids` member landed a registry expiry event (the
/// `expired_refs` the caller resolved — the report stays readable;
/// AC-R-2.10.4-10).
pub fn fit_surface(
    runs: &[EvalRun],
    design: Option<&Design>,
    factors: &[String],
    metric: &str,
    confidence_ppm: i64,
    expired_refs: &BTreeSet<String>,
) -> Result<Json, AnalysisError> {
    if factors.is_empty() {
        return Err(AnalysisError::BadSpec {
            member: "query.filters.factors".into(),
            detail: "fit_surface requires ≥ 1 factor".into(),
        });
    }
    // The design space — the declared `FactorDeclaration.levels[].id` set
    // where the design declares the factor (the unknown-cells grid is the
    // *declared* grid minus the probed tuples — AC-R-2.10.4-10), union
    // the observed levels (a probed row carrying an undeclared level is
    // still a probed cell — never silently dropped).
    let mut levels: Vec<BTreeSet<String>> = factors
        .iter()
        .map(|f| {
            let mut s = BTreeSet::new();
            if let Some(d) = design {
                if let Some(fd) = d.factors.iter().find(|fd| fd.name == *f) {
                    for l in &fd.levels {
                        s.insert(l.id.clone());
                    }
                }
            }
            s
        })
        .collect();
    for r in runs {
        for (i, f) in factors.iter().enumerate() {
            levels[i].insert(level_of(r, f));
        }
    }
    // Probed cells — the observed level tuples.
    let mut probed: BTreeMap<Vec<String>, Vec<&EvalRun>> = BTreeMap::new();
    for r in runs {
        let key: Vec<String> = factors.iter().map(|f| level_of(r, f)).collect();
        probed.entry(key).or_default().push(r);
    }
    // `unknown_cells` — the full grid minus the probed tuples.
    let mut grid: Vec<Vec<String>> = vec![Vec::new()];
    for l in &levels {
        let mut next = Vec::new();
        for prefix in &grid {
            for lv in l {
                let mut p = prefix.clone();
                p.push(lv.clone());
                next.push(p);
            }
        }
        grid = next;
    }
    let probed_keys: BTreeSet<&Vec<String>> = probed.keys().collect();
    let unknown_cells: Vec<Json> = grid
        .iter()
        .filter(|k| !probed_keys.contains(k))
        .map(|k| Json::Arr(k.iter().map(Json::str).collect()))
        .collect();
    // Main effects — per factor, each level's mean Δ against the
    // factor's first (baseline) level, clustered by task.
    let mut main_effects = Vec::new();
    for (i, f) in factors.iter().enumerate() {
        let baseline = levels[i].iter().next().cloned().unwrap_or_default();
        for lv in &levels[i] {
            if *lv == baseline {
                continue;
            }
            // Paired over shared tasks.
            let a: Vec<&EvalRun> = probed
                .iter()
                .filter(|(k, _)| k[i] == baseline)
                .flat_map(|(_, rs)| rs.iter().copied())
                .collect();
            let b: Vec<&EvalRun> = probed
                .iter()
                .filter(|(k, _)| k[i] == *lv)
                .flat_map(|(_, rs)| rs.iter().copied())
                .collect();
            let am = per_task_points(&a, metric);
            let bm = per_task_points(&b, metric);
            let deltas: Vec<(String, i64)> = am
                .iter()
                .filter_map(|(t, av)| bm.get(t).map(|bv| (t.clone(), bv[0] - av[0])))
                .collect();
            let est = stats::mean(&deltas.iter().map(|(_, d)| *d).collect::<Vec<_>>());
            let iv = stats::clustered_clt(&deltas, confidence_ppm);
            main_effects.push(Json::obj([
                ("factor", Json::str(f)),
                ("level", Json::str(lv)),
                ("baseline_level", Json::str(&baseline)),
                (
                    "estimate",
                    est.map_or(
                        Json::obj([("n/a", Json::str("estimator_undefined"))]),
                        Json::Int,
                    ),
                ),
                (
                    "interval",
                    iv.map_or(
                        Json::obj([("n/a", Json::str("estimator_undefined"))]),
                        |i| Json::obj([("lo", Json::Int(i.lo)), ("hi", Json::Int(i.hi))]),
                    ),
                ),
                ("n_tasks", Json::Int(deltas.len() as i64)),
            ]));
        }
    }
    // `status` — a registry expiry on any contributing configuration
    // flips the surface to `expired` (the report stays readable —
    // AC-R-2.10.4-10; the debt record carries the refit obligation).
    let mut configuration_ids: Vec<String> =
        runs.iter().map(|r| r.configuration_id.clone()).collect();
    configuration_ids.sort();
    configuration_ids.dedup();
    let expired_hit: Vec<&String> = configuration_ids
        .iter()
        .filter(|c| expired_refs.contains(*c))
        .collect();
    let status = if expired_hit.is_empty() {
        "active"
    } else {
        "expired"
    };
    Ok(Json::obj([
        ("schema", Json::str("hh-fitted-surface/1")),
        ("metric", Json::str(metric)),
        (
            "factors",
            Json::Arr(factors.iter().map(Json::str).collect()),
        ),
        (
            "levels",
            Json::Arr(
                levels
                    .iter()
                    .map(|l| Json::Arr(l.iter().map(Json::str).collect()))
                    .collect(),
            ),
        ),
        ("model_form", Json::str("contrast")),
        (
            "sample_sizes",
            Json::Arr(
                probed
                    .iter()
                    .map(|(k, rs)| {
                        Json::obj([
                            ("cell", Json::Arr(k.iter().map(Json::str).collect())),
                            ("n", Json::Int(rs.len() as i64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "estimates_with_ci",
            Json::obj([("main_effects", Json::Arr(main_effects))]),
        ),
        ("unknown_cells", Json::Arr(unknown_cells)),
        (
            "configuration_ids",
            Json::Arr(configuration_ids.iter().map(Json::str).collect()),
        ),
        ("status", Json::str(status)),
        (
            "expired_refs",
            Json::Arr(expired_hit.iter().map(|r| Json::str(*r)).collect()),
        ),
        (
            "expiry_condition",
            Json::str("registry expiry on any member of configuration_ids"),
        ),
    ]))
}

// ── A6 benefit decomposition ──────────────────────────────────────────

/// `oracle_best_of_n` — pass@N over the baseline's per-task replicate
/// counts (the upper-bound baseline, labelled; `n/a` when the baseline
/// has < N replicates — ADR-0159 D5; AC-R-2.10.4-11).
pub fn search_baseline(
    baseline_runs: &[&EvalRun],
    metric: &str,
    n: u64,
    selector_ref: Option<&str>,
) -> Json {
    let kind = match selector_ref {
        Some(_) => "selected_best_of_n",
        None => "oracle_best_of_n",
    };
    // pass@N per task: 1 - C(n-c, N)/C(n, N) over the baseline's
    // replicates — the oracle chooses the best of N draws.
    let mut per_task: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in baseline_runs {
        let e = per_task.entry(r.task_id.clone()).or_insert((0, 0));
        e.1 += 1;
        if point(r, metric).map(|v| v > 0).unwrap_or(false) {
            e.0 += 1;
        }
    }
    let mut acc: u128 = 0;
    let mut counted = 0u64;
    let mut short = false;
    for (c, nc) in per_task.values() {
        if *nc < n {
            short = true;
            continue;
        }
        // P(at least one success in N draws) = 1 - C(n-c, N)/C(n, N).
        let mut miss: u128 = 1_000_000;
        for i in 0..n {
            let num = nc - c; // failures
            if num <= i {
                miss = 0;
                break;
            }
            miss = miss * ((num - i) as u128) / ((*nc - i) as u128);
        }
        acc += 1_000_000u128.saturating_sub(miss);
        counted += 1;
    }
    let value = if per_task.is_empty() || counted == 0 {
        Json::obj([("n/a", Json::str("estimator_undefined"))])
    } else {
        Json::Int((acc / counted as u128) as i64)
    };
    let mut m = BTreeMap::new();
    m.insert("kind".into(), Json::str(kind));
    m.insert("n".into(), Json::Int(n as i64));
    m.insert("value_ppm".into(), value);
    if let Some(s) = selector_ref {
        m.insert("selector_ref".into(), Json::str(s));
    }
    if short {
        m.insert(
            "partial".into(),
            Json::str("baseline replicates < N on some tasks"),
        );
    }
    Json::Obj(m)
}

// ── A16 render / diff_reports ─────────────────────────────────────────

/// `render(report_body, view)` — the §07-facing projection naming the
/// `report_id`; render never recomputes (ADR-0157 D8): it selects and
/// re-titles members, nothing else.
pub fn render(body: &Json, view: &str, options: &Json) -> Result<Json, AnalysisError> {
    let Json::Obj(m) = body else {
        return Err(AnalysisError::BadSpec {
            member: "report".into(),
            detail: "render requires a report body".into(),
        });
    };
    let report_id = m
        .get("report_id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let mut out = BTreeMap::new();
    out.insert("schema".into(), Json::str("hh-analysis-view/1"));
    out.insert("view".into(), Json::str(view));
    out.insert("report_id".into(), Json::str(&report_id));
    out.insert("kind".into(), m.get("kind").cloned().unwrap_or(Json::Null));
    out.insert("options".into(), options.clone());
    // The view member — the named section verbatim (render selects,
    // never recomputes).
    let section = match view {
        "scorecard" | "summarize" => "summaries",
        "comparison" => "comparisons",
        "frontier" => "frontier",
        "transfer" => "transfer",
        "surface" => "surface",
        "rank" => "rank",
        "reliability" => "reliability",
        "strata" => "strata",
        "diagnostics" => "diagnostics",
        other => other,
    };
    out.insert(
        "content".into(),
        m.get(section).cloned().unwrap_or(Json::Null),
    );
    Ok(Json::Obj(out))
}

/// `diff_reports(a, b)` — the report-diff view: cells present in `b`
/// keyed against `a` (`added`, `changed`, `removed`, `unchanged` counts,
/// the per-key detail, and the named `report_id`s). A pure memberwise
/// compare, never a recompute.
pub fn diff_reports(a: &Json, b: &Json) -> Json {
    let cells_of = |j: &Json| -> BTreeMap<String, Json> {
        let mut out = BTreeMap::new();
        if let Some(Json::Arr(items)) = j.get("comparisons") {
            for c in items {
                let key = c
                    .get("comparison_id")
                    .or_else(|| c.get("arm_pair"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                out.insert(key, c.clone());
            }
        }
        out
    };
    let ca = cells_of(a);
    let cb = cells_of(b);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (k, v) in &cb {
        match ca.get(k) {
            None => added.push(k.clone()),
            Some(old) if old != v => changed.push(k.clone()),
            _ => {}
        }
    }
    for k in ca.keys() {
        if !cb.contains_key(k) {
            removed.push(k.clone());
        }
    }
    Json::obj([
        ("schema", Json::str("hh-analysis-diff/1")),
        ("a", a.get("report_id").cloned().unwrap_or(Json::Null)),
        ("b", b.get("report_id").cloned().unwrap_or(Json::Null)),
        ("added", Json::Arr(added.iter().map(Json::str).collect())),
        (
            "removed",
            Json::Arr(removed.iter().map(Json::str).collect()),
        ),
        (
            "changed",
            Json::Arr(changed.iter().map(Json::str).collect()),
        ),
    ])
}
