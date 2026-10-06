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

// ── A7 fit_surface (C2 — §6.4 §2.3; ADR-0160; S5.3) ───────────────────

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

/// `model_form ∈ {contrast (C1 default), factorial_glmm (C2; shrinkage
/// reported), curve_on_ordered_axis (≥ 3 probed levels; re-estimated per
/// resample)}` (§6.4 §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceModelForm {
    /// `contrast` — cell means against the baseline level.
    Contrast,
    /// `factorial_glmm` — fixed effects per (factor, level) plus a random
    /// task intercept estimated method-of-moments, with the task-effect
    /// shrinkage table reported.
    FactorialGlmm,
    /// `curve_on_ordered_axis` — a monotone (pool-adjacent-violators)
    /// curve per ordered axis, re-estimated per resample by the caller.
    CurveOnOrderedAxis,
}

impl SurfaceModelForm {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            SurfaceModelForm::Contrast => "contrast",
            SurfaceModelForm::FactorialGlmm => "factorial_glmm",
            SurfaceModelForm::CurveOnOrderedAxis => "curve_on_ordered_axis",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<SurfaceModelForm> {
        match s {
            "contrast" => Some(SurfaceModelForm::Contrast),
            "factorial_glmm" => Some(SurfaceModelForm::FactorialGlmm),
            "curve_on_ordered_axis" => Some(SurfaceModelForm::CurveOnOrderedAxis),
            _ => None,
        }
    }
}

/// The records-in expiry observables a fitted surface reads (§6.4 §2.3 —
/// the caller resolves the registry lifecycle events, suite-validity
/// supersessions, environment-image supersessions and profile
/// `semantic_id` changes into ref sets; the kernel never reads a store).
#[derive(Debug, Clone, Default)]
pub struct SurfaceExpiry {
    /// `fitted_at` (ms epoch) — the fit instant the `max_age` trigger and
    /// the debt record's `created_at`/`expiry.params.until` read. `None`
    /// → the report's `fitted_at` is `null` (never fabricated).
    pub fitted_at: Option<i64>,
    /// `now_ms` — the evaluation instant the `max_age` trigger fires
    /// against.
    pub now_ms: Option<i64>,
    /// Refs that fired a supersede/retire/semantic-change event — any
    /// member of the report's tracked refs (configuration ids, model
    /// snapshots, environment images, design ref, suite ids) hitting
    /// fires `expired`.
    pub superseded: BTreeSet<String>,
    /// Refs inside their warn window → `expiring` (never `expired`).
    pub expiring: BTreeSet<String>,
    /// `max_age` ms — the `fitted_at + max_age` trigger (OQ-368's 90-day
    /// placeholder; `None` = the trigger is unbound).
    pub max_age_ms: Option<i64>,
    /// The debt record's owner (records-in — `principal{id}`; absent →
    /// `lab-analysis`, the kernel's own identity coordinate).
    pub owner: Option<String>,
}

/// The `fit_surface` request (the filter bundle the kernel decodes).
pub struct FitRequest<'a> {
    /// The factor axes.
    pub factors: &'a [String],
    /// The fitted metric.
    pub metric: &'a str,
    /// The model form (default `contrast`).
    pub model_form: SurfaceModelForm,
    /// The interval confidence (ppm).
    pub confidence_ppm: i64,
    /// Axes treated as ordered beyond the design's `budget`-kind
    /// factors (`filters.ordered[]`) — the curve axes.
    pub ordered_axes: &'a [String],
    /// `adjust_for[]` — nuisance coordinates whose level effects are
    /// partialled out before the surface is fit (records-in).
    pub adjust_for: &'a [String],
    /// The expiry observables.
    pub expiry: &'a SurfaceExpiry,
    /// The design ref (`spec.spec_ref`).
    pub design_ref: Option<&'a str>,
    /// The one `MatchSpec` content address the fit ran under (the caller
    /// computes it after `validate_match`).
    pub match_spec_ref: Option<&'a str>,
    /// The report's `report_id` — the debt record's `evidence_refs`
    /// member binds it.
    pub report_id: &'a str,
    /// The report's `generated_from` (the record repeats it).
    pub generated_from: &'a Json,
}

/// The `n/a`/`unknown` JSON spellings used inside the report — cell
/// records carry `unknown{reason, n}` (a typed unknown, never a number;
/// T-LCD-15).
fn unknown_cell(reason: &str, n: Option<i64>) -> Json {
    let mut u = BTreeMap::new();
    u.insert("reason".into(), Json::str(reason));
    if let Some(n) = n {
        u.insert("n".into(), Json::Int(n));
    }
    Json::obj([("unknown", Json::Obj(u))])
}

/// Whether `axis` is ordered — a `budget`-kind design factor, or named in
/// `ordered[]` (the caller's declaration; the axis order is the declared
/// `levels[]` order).
fn ordered_axis(factor: &str, design: Option<&Design>, ordered: &[String]) -> bool {
    if ordered.iter().any(|a| a == factor) {
        return true;
    }
    design
        .and_then(|d| d.factors.iter().find(|f| f.name == factor))
        .map(|f| f.kind == FactorKind::Budget)
        .unwrap_or(false)
}

/// Pool-adjacent-violators (non-decreasing) over `points` — the monotone
/// curve's deterministic estimator (integer ppm arithmetic).
fn pava_nondecreasing(points: &[i64]) -> Vec<i64> {
    // (sum, count) blocks; merge while a block's mean exceeds its
    // successor's.
    let mut blocks: Vec<(i128, u64)> = points.iter().map(|p| (*p as i128, 1u64)).collect();
    let mut i = 0;
    while i + 1 < blocks.len() {
        let (s0, n0) = blocks[i];
        let (s1, n1) = blocks[i + 1];
        if s0 * n1 as i128 > s1 * n0 as i128 {
            blocks[i] = (s0 + s1, n0 + n1);
            blocks.remove(i + 1);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
    let mut fitted = Vec::with_capacity(points.len());
    for (s, n) in blocks {
        let mean = (s / n.max(1) as i128) as i64;
        for _ in 0..n {
            fitted.push(mean);
        }
    }
    fitted
}

/// `fit_surface(rows, design, request)` → `FittedSurfaceReport` (§6.4
/// §2.3; ADR-0160; AC-R-2.10.4-10):
///
/// - the declared ∪ probed grid; unprobed tuples land in
///   `unknown_cells` (**never** interpolated — ADR-0012 D7);
/// - the minimum design per cell: ≥ 2 levels per included factor,
///   `replicates_per_cell ≥ 3`, `n_tasks ≥ 30` — cells below the
///   minimum are `unknown{reason = insufficient, n}` (kept, never
///   dropped, never estimated);
/// - `model_form` — `contrast` (default) | `factorial_glmm` (fixed
///   effects + task random intercepts + `shrinkage` + two-factor
///   `interactions[]`, method-of-moments) | `curve_on_ordered_axis`
///   (a monotone PAVA curve per ordered axis);
/// - hosted coordinates — a component-level axis makes the surface
///   native-only (hosted runs listed `n/a{class}`, never fitted);
///   a hosted run on a varied configuration-level axis without a
///   `supported`/`partial` capability verdict on that coordinate is
///   listed `n/a{capability}` (T-LCD-07 — never coerced);
/// - expiry wiring — the report carries `status ∈ {active, expiring,
///   expired}` evaluated from the `SurfaceExpiry` observables plus the
///   minted home-11 `AssumptionDebtRecord` (`refit{min_design}`) whose
///   `debt_record_ref` the report names; an expired report stays
///   readable.
pub fn fit_surface(
    runs: &[EvalRun],
    design: Option<&Design>,
    req: &FitRequest<'_>,
) -> Result<Json, AnalysisError> {
    let factors = req.factors;
    let metric = req.metric;
    if factors.is_empty() {
        return Err(AnalysisError::BadSpec {
            member: "query.filters.factors".into(),
            detail: "fit_surface requires ≥ 1 factor".into(),
        });
    }
    // ── Hosted coordinates (§2.3/§2.5) ────────────────────────────────
    // A component-level axis makes the surface native-only — the hosted
    // rows are listed `n/a{class}`, never fitted. On a varied
    // configuration-level axis a hosted row needs a `supported|partial`
    // verdict on that coordinate — `unknown` is never coerced; mixed
    // verdicts across participants on the same varied coordinate are
    // `InadmissibleFactor` (T-LCD-07).
    let mut na_rows = Vec::new();
    let mut eligible: Vec<&EvalRun> = Vec::new();
    let component_level = factors.iter().any(|f| {
        design
            .and_then(|d| d.factors.iter().find(|fd| &fd.name == f))
            .map(|fd| {
                matches!(fd.kind, FactorKind::Harness)
                    || matches!(
                        fd.granularity,
                        Some(hh_ontology::participant::Granularity::ComponentLevel)
                    )
            })
            == Some(true)
            || (design
                .map(|d| !d.factors.iter().any(|fd| &fd.name == f))
                .unwrap_or(true)
                && ![
                    "model",
                    "model_snapshot",
                    "environment",
                    "task_family",
                    "suite",
                    "participant_class",
                ]
                .contains(&f.as_str()))
    });
    for r in runs {
        if !is_hosted(r) {
            eligible.push(r);
            continue;
        }
        if component_level {
            na_rows.push(Json::obj([
                ("run_id", Json::str(&r.run_id)),
                ("n/a", Json::str("class")),
            ]));
            continue;
        }
        // Configuration-level axes — the hosted row joins only where its
        // declared coordinate is a `supported`/`partial` verdict on that
        // axis when the vector speaks to it; a `model`-axis join needs
        // `model_override` (the §2.5 A5/A7 rule).
        let mut blocked = false;
        for f in factors {
            let verdict = r.capability_vector.get(f).copied();
            let model_axis = f == "model" || f == "model_snapshot";
            let needs = model_axis
                || design
                    .and_then(|d| d.factors.iter().find(|fd| &fd.name == f))
                    .map(|fd| fd.kind == FactorKind::ModelSnapshot)
                    .unwrap_or(false);
            let v = if needs && verdict.is_none() {
                r.capability_vector.get("model_override").copied()
            } else {
                verdict
            };
            if let Some(v) = v {
                use hh_ontology::participant::CapabilityVerdict as Cv;
                if matches!(v, Cv::Unknown | Cv::Unsupported | Cv::Drift) {
                    na_rows.push(Json::obj([
                        ("run_id", Json::str(&r.run_id)),
                        ("n/a", Json::str("capability")),
                        ("factor", Json::str(f)),
                    ]));
                    blocked = true;
                    break;
                }
            }
        }
        if !blocked {
            eligible.push(r);
        }
    }
    // T-LCD-07 — comparisons across participants with different
    // `unknown`/`SUPPORTED` entries on a varied coordinate refuse.
    for f in factors {
        let verdicts: BTreeSet<String> = eligible
            .iter()
            .filter(|r| is_hosted(r))
            .map(|r| {
                r.capability_vector
                    .get(f)
                    .map(|v| format!("{v:?}").to_lowercase())
                    .unwrap_or_else(|| "absent".into())
            })
            .collect();
        if verdicts.len() > 1 {
            return Err(AnalysisError::InadmissibleFactor {
                factor: f.clone(),
                run_id: eligible
                    .iter()
                    .find(|r| is_hosted(r))
                    .map(|r| r.run_id.clone())
                    .unwrap_or_default(),
                detail: format!(
                    "hosted rows disagree on the varied coordinate {f} ({verdicts:?}) — never coerced"
                ),
            });
        }
    }

    // ── The grid: declared levels ∪ probed levels ─────────────────────
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
    for r in &eligible {
        for (i, f) in factors.iter().enumerate() {
            levels[i].insert(level_of(r, f));
        }
    }
    let mut probed: BTreeMap<Vec<String>, Vec<&EvalRun>> = BTreeMap::new();
    for r in &eligible {
        let key: Vec<String> = factors.iter().map(|f| level_of(r, f)).collect();
        probed.entry(key).or_default().push(*r);
    }
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

    // ── The minimum design (§2.3) ─────────────────────────────────────
    // ≥ 2 levels per included factor; `replicates_per_cell ≥ 3`;
    // `n_tasks ≥ 30`. A below-minimum cell is `unknown{insufficient, n}`
    // — kept, never dropped, never estimated.
    const MIN_LEVELS: usize = 2;
    const MIN_REPLICATES: i64 = 3;
    const MIN_TASKS: i64 = 30;
    let levels_ok = levels.iter().all(|l| l.len() >= MIN_LEVELS);
    let declared_reps = design.map(|d| d.replicates_per_cell).unwrap_or(0) as i64;
    let replicates_ok = design.map(|d| d.replicates_per_cell >= 3).unwrap_or(false);
    let n_tasks: i64 = {
        let t: BTreeSet<&str> = eligible.iter().map(|r| r.task_id.as_str()).collect();
        t.len() as i64
    };
    let tasks_ok = n_tasks >= MIN_TASKS;
    let design_met = levels_ok && replicates_ok && tasks_ok;
    // A probed cell is estimable iff the design minimum is met and its
    // landed replicate count reaches the floor.
    let estimable: BTreeSet<Vec<String>> = if design_met {
        probed
            .iter()
            .filter(|(_, rs)| rs.len() as i64 >= MIN_REPLICATES)
            .map(|(k, _)| k.clone())
            .collect()
    } else {
        BTreeSet::new()
    };
    let insufficient_cells: Vec<Json> = probed
        .iter()
        .filter(|(k, _)| !estimable.contains(*k))
        .map(|(k, rs)| {
            Json::obj([
                ("cell", Json::Arr(k.iter().map(Json::str).collect())),
                ("reason", Json::str("insufficient")),
                ("n", Json::Int(rs.len() as i64)),
            ])
        })
        .collect();

    // `adjust_for` — nuisance coordinates partialled out before the fit:
    // each run's point shifts by `−(level_mean − grand_mean)` per named
    // axis (the additive projection; the adjusted points feed every
    // estimator below).
    let adjusted: BTreeMap<String, i64> = {
        let mut out = BTreeMap::new();
        let raw: Vec<(String, i64)> = eligible
            .iter()
            .filter_map(|r| point(r, metric).map(|v| (r.run_id.clone(), v)))
            .collect();
        let grand = stats::mean(&raw.iter().map(|(_, v)| *v).collect::<Vec<_>>()).unwrap_or(0);
        for r in &eligible {
            if let Some(v) = point(r, metric) {
                let mut adj = v;
                for axis in req.adjust_for {
                    let lv = level_of(r, axis);
                    let lvs: Vec<i64> = eligible
                        .iter()
                        .filter(|x| level_of(x, axis) == lv)
                        .filter_map(|x| point(x, metric))
                        .collect();
                    if let Some(m) = stats::mean(&lvs) {
                        adj = adj.saturating_sub(m.saturating_sub(grand));
                    }
                }
                out.insert(r.run_id.clone(), adj);
            }
        }
        out
    };
    let adj_point = |r: &EvalRun| -> Option<i64> {
        if req.adjust_for.is_empty() {
            point(r, metric)
        } else {
            adjusted.get(&r.run_id).copied()
        }
    };

    // ── Estimates ─────────────────────────────────────────────────────
    // Per (factor, level ≠ baseline): the paired per-task Δ of that
    // level's sufficient cells against the baseline's sufficient cells,
    // clustered by task. Only estimable cells contribute (a below-min
    // cell never feeds an estimate).
    let mut main_effects = Vec::new();
    for (i, f) in factors.iter().enumerate() {
        if levels[i].len() < MIN_LEVELS {
            main_effects.push(Json::obj([
                ("factor", Json::str(f)),
                (
                    "estimate",
                    unknown_cell("insufficient", Some(levels[i].len() as i64)),
                ),
            ]));
            continue;
        }
        let baseline = levels[i].iter().next().cloned().unwrap_or_default();
        for lv in &levels[i] {
            if *lv == baseline {
                continue;
            }
            let a: Vec<&EvalRun> = probed
                .iter()
                .filter(|(k, _)| k[i] == baseline && estimable.contains(*k))
                .flat_map(|(_, rs)| rs.iter().copied())
                .collect();
            let b: Vec<&EvalRun> = probed
                .iter()
                .filter(|(k, _)| k[i] == *lv && estimable.contains(*k))
                .flat_map(|(_, rs)| rs.iter().copied())
                .collect();
            let am: BTreeMap<String, Vec<i64>> = {
                let mut m: BTreeMap<String, Vec<i64>> = BTreeMap::new();
                for r in &a {
                    if let Some(v) = adj_point(r) {
                        m.entry(r.task_id.clone()).or_default().push(v);
                    }
                }
                m.into_iter()
                    .map(|(t, vs)| (t, vec![stats::mean(&vs).unwrap_or(0)]))
                    .collect()
            };
            let bm: BTreeMap<String, Vec<i64>> = {
                let mut m: BTreeMap<String, Vec<i64>> = BTreeMap::new();
                for r in &b {
                    if let Some(v) = adj_point(r) {
                        m.entry(r.task_id.clone()).or_default().push(v);
                    }
                }
                m.into_iter()
                    .map(|(t, vs)| (t, vec![stats::mean(&vs).unwrap_or(0)]))
                    .collect()
            };
            let deltas: Vec<(String, i64)> = am
                .iter()
                .filter_map(|(t, av)| bm.get(t).map(|bv| (t.clone(), bv[0] - av[0])))
                .collect();
            let est = stats::mean(&deltas.iter().map(|(_, d)| *d).collect::<Vec<_>>());
            let iv = stats::clustered_clt(&deltas, req.confidence_ppm);
            main_effects.push(Json::obj([
                ("factor", Json::str(f)),
                ("level", Json::str(lv)),
                ("baseline_level", Json::str(&baseline)),
                (
                    "estimate",
                    est.map_or(unknown_cell("insufficient", None), Json::Int),
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

    // `factorial_glmm` — the additive fit's residuals decompose into the
    // task random intercept (`u_t`) and the task × model-axis term;
    // `shrinkage` reports the per-task `var_u/(var_u + var_e/n_t)`
    // factor the posterior mean applies (method-of-moments, integer
    // ppm — the estimator is declared in `method`).
    let mut random_effects = Json::Null;
    let mut interactions = Vec::new();
    if req.model_form == SurfaceModelForm::FactorialGlmm {
        // The fitted fixed part per run: grand mean + Σ level effects.
        let level_effect = |f: &str, lv: &str| -> i64 {
            main_effects
                .iter()
                .find(|e| {
                    e.get("factor").and_then(Json::as_str) == Some(f)
                        && e.get("level").and_then(Json::as_str) == Some(lv)
                })
                .and_then(|e| e.get("estimate").and_then(Json::as_int))
                .unwrap_or(0)
        };
        let all_pts: Vec<i64> = eligible.iter().filter_map(|r| adj_point(r)).collect();
        let grand = stats::mean(&all_pts).unwrap_or(0);
        let mut task_resid: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for r in &eligible {
            if !estimable.contains(&factors.iter().map(|f| level_of(r, f)).collect::<Vec<_>>()) {
                continue;
            }
            if let Some(v) = adj_point(r) {
                let fitted: i64 = grand
                    + factors
                        .iter()
                        .map(|f| level_effect(f, &level_of(r, f)))
                        .sum::<i64>();
                task_resid
                    .entry(r.task_id.clone())
                    .or_default()
                    .push(v.saturating_sub(fitted));
            }
        }
        // `u_t` = the task's mean residual; `var_u` the across-task
        // spread; `var_e` the within-task residual spread.
        let mut task_rows = Vec::new();
        let mut u_sq: i128 = 0;
        let mut e_sq: i128 = 0;
        let mut e_n: i128 = 0;
        let n_tasks_r = task_resid.len() as i128;
        for (t, vs) in &task_resid {
            let u = stats::mean(vs).unwrap_or(0);
            u_sq += u as i128 * u as i128;
            for v in vs {
                e_sq += (*v as i128 - u as i128).pow(2);
            }
            e_n += vs.len() as i128;
            task_rows.push((t.clone(), u, vs.len() as i64));
        }
        let var_u = if n_tasks_r > 0 { u_sq / n_tasks_r } else { 0 };
        let var_e = if e_n > 0 { e_sq / e_n } else { 0 };
        let shrinkage_rows: Vec<Json> = task_rows
            .iter()
            .map(|(t, u, n)| {
                // shrinkage_t = var_u / (var_u + var_e / n_t) — the BLUP
                // factor, ppm.
                let denom = var_u + var_e / (*n).max(1) as i128;
                let s = if denom > 0 {
                    (var_u * stats::PPM as i128 / denom) as i64
                } else {
                    0
                };
                Json::obj([
                    ("task_id", Json::str(t)),
                    ("u_hat", Json::Int(*u)),
                    (
                        "shrunk_u_hat",
                        Json::Int((*u as i128 * s as i128 / stats::PPM as i128) as i64),
                    ),
                    ("shrinkage_ppm", Json::Int(s)),
                ])
            })
            .collect();
        // task × model-axis residuals — the second random term.
        let model_axis = factors.iter().enumerate().find(|(i, f)| {
            let _ = i;
            f.as_str() == "model"
                || f.as_str() == "model_snapshot"
                || design
                    .and_then(|d| d.factors.iter().find(|fd| &fd.name == *f))
                    .map(|fd| fd.kind == FactorKind::ModelSnapshot)
                    .unwrap_or(false)
        });
        let mut txm_sq: i128 = 0;
        let mut txm_n: i128 = 0;
        if let Some((mi, _)) = model_axis {
            let mut by_tl: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();
            for r in &eligible {
                if let Some(v) = adj_point(r) {
                    by_tl
                        .entry((r.task_id.clone(), level_of(r, factors[mi].as_str())))
                        .or_default()
                        .push(v);
                }
            }
            // w_{t,l} = cell mean − (task mean + level effect).
            let task_mean: BTreeMap<String, i64> = eligible
                .iter()
                .filter_map(|r| adj_point(r).map(|v| (r.task_id.clone(), v)))
                .fold(BTreeMap::<String, Vec<i64>>::new(), |mut m, (t, v)| {
                    m.entry(t).or_default().push(v);
                    m
                })
                .into_iter()
                .map(|(t, vs)| (t, stats::mean(&vs).unwrap_or(0)))
                .collect();
            for ((t, l), vs) in &by_tl {
                let m = stats::mean(vs).unwrap_or(0);
                let fitted =
                    task_mean.get(t).copied().unwrap_or(0) + level_effect(factors[mi].as_str(), l);
                let w = m - fitted;
                txm_sq += w as i128 * w as i128;
                txm_n += 1;
            }
        }
        let var_txm = if txm_n > 0 { txm_sq / txm_n } else { 0 };
        random_effects = Json::obj([
            ("method", Json::str("method_of_moments")),
            ("task_variance_ppm", Json::Int(var_u as i64)),
            ("task_x_model_variance_ppm", Json::Int(var_txm as i64)),
            ("shrinkage", Json::Arr(shrinkage_rows)),
        ]);
        // Two-factor interactions — per factor pair, the probed level
        // pairs' residual against the additive fit (the `interactions[]`
        // member the GLMM form reports).
        for i in 0..factors.len() {
            for j in (i + 1)..factors.len() {
                for la in &levels[i] {
                    for lb in &levels[j] {
                        let cell_runs: Vec<&EvalRun> = probed
                            .iter()
                            .filter(|(k, _)| k[i] == *la && k[j] == *lb && estimable.contains(*k))
                            .flat_map(|(_, rs)| rs.iter().copied())
                            .collect();
                        if cell_runs.is_empty() {
                            continue;
                        }
                        let pts: Vec<i64> = cell_runs.iter().filter_map(|r| adj_point(r)).collect();
                        let cm = stats::mean(&pts).unwrap_or(0);
                        let inter = cm
                            - (grand
                                + level_effect(factors[i].as_str(), la)
                                + level_effect(factors[j].as_str(), lb));
                        let iv = stats::bootstrap(
                            &pts,
                            req.confidence_ppm,
                            &format!("{}:{}:{}:{}", factors[i], la, factors[j], lb),
                        );
                        interactions.push(Json::obj([
                            (
                                "factors",
                                Json::Arr(vec![
                                    Json::str(factors[i].as_str()),
                                    Json::str(factors[j].as_str()),
                                ]),
                            ),
                            ("levels", Json::Arr(vec![Json::str(la), Json::str(lb)])),
                            ("estimate", Json::Int(inter)),
                            (
                                "interval",
                                iv.map_or(
                                    Json::obj([("n/a", Json::str("estimator_undefined"))]),
                                    |i| {
                                        Json::obj([
                                            ("lo", Json::Int(i.lo)),
                                            ("hi", Json::Int(i.hi)),
                                        ])
                                    },
                                ),
                            ),
                        ]));
                    }
                }
            }
        }
    }

    // `curve_on_ordered_axis` — a monotone curve per ordered axis (the
    // design's `budget`-kind factors + `ordered[]`); < 3 probed levels
    // renders `n/a{estimator_undefined}` on the axis's entry.
    let mut curves = Vec::new();
    let wants_curves =
        req.model_form == SurfaceModelForm::CurveOnOrderedAxis || !req.ordered_axes.is_empty();
    if wants_curves {
        for (i, f) in factors.iter().enumerate() {
            if !ordered_axis(f, design, req.ordered_axes) {
                continue;
            }
            // Axis order — the declared `levels[]` order; undeclared
            // probed levels append in sorted order.
            let declared_order: Vec<String> = design
                .and_then(|d| d.factors.iter().find(|fd| &fd.name == f))
                .map(|fd| fd.levels.iter().map(|l| l.id.clone()).collect())
                .unwrap_or_default();
            let mut axis_levels = declared_order;
            for lv in &levels[i] {
                if !axis_levels.contains(lv) {
                    axis_levels.push(lv.clone());
                }
            }
            let probed_lv: Vec<String> = axis_levels
                .iter()
                .filter(|lv| {
                    probed
                        .iter()
                        .any(|(k, _)| &k[i] == *lv && estimable.contains(k))
                })
                .cloned()
                .collect();
            if probed_lv.len() < 3 {
                curves.push(Json::obj([
                    ("factor", Json::str(f)),
                    (
                        "curve",
                        unknown_cell("insufficient", Some(probed_lv.len() as i64)),
                    ),
                ]));
                continue;
            }
            let mut means = Vec::new();
            let mut fitted_rows = Vec::new();
            for lv in &probed_lv {
                let lruns: Vec<&EvalRun> = probed
                    .iter()
                    .filter(|(k, _)| &k[i] == lv && estimable.contains(*k))
                    .flat_map(|(_, rs)| rs.iter().copied())
                    .collect();
                let pts: Vec<i64> = lruns.iter().filter_map(|r| adj_point(r)).collect();
                let m = stats::mean(&pts).unwrap_or(0);
                means.push(m);
                let by_task: Vec<(String, i64)> = {
                    let mut t: BTreeMap<String, Vec<i64>> = BTreeMap::new();
                    for r in &lruns {
                        if let Some(v) = adj_point(r) {
                            t.entry(r.task_id.clone()).or_default().push(v);
                        }
                    }
                    t.into_iter()
                        .map(|(k, vs)| (k, stats::mean(&vs).unwrap_or(0)))
                        .collect()
                };
                let iv = stats::clustered_clt(&by_task, req.confidence_ppm);
                fitted_rows.push((lv.clone(), m, iv));
            }
            // Direction — the observed trend's sign (non-decreasing when
            // last ≥ first, else non-increasing, folded through PAVA).
            let increasing = means.last().copied().unwrap_or(0) >= means[0];
            let fitted = if increasing {
                pava_nondecreasing(&means)
            } else {
                let mut flipped: Vec<i64> = means.iter().map(|m| -*m).collect();
                flipped = pava_nondecreasing(&flipped);
                flipped.iter().map(|m| -*m).collect()
            };
            curves.push(Json::obj([
                ("factor", Json::str(f)),
                ("form", Json::str("monotone")),
                (
                    "direction",
                    Json::str(if increasing {
                        "nondecreasing"
                    } else {
                        "nonincreasing"
                    }),
                ),
                (
                    "fitted",
                    Json::Arr(
                        fitted_rows
                            .iter()
                            .zip(fitted.iter())
                            .map(|((lv, obs, iv), fv)| {
                                Json::obj([
                                    ("level", Json::str(lv)),
                                    ("observed", Json::Int(*obs)),
                                    ("fitted", Json::Int(*fv)),
                                    (
                                        "interval",
                                        iv.map_or(
                                            Json::obj([("n/a", Json::str("estimator_undefined"))]),
                                            |i| {
                                                Json::obj([
                                                    ("lo", Json::Int(i.lo)),
                                                    ("hi", Json::Int(i.hi)),
                                                ])
                                            },
                                        ),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]));
        }
    }

    // ── conditionality / portability ─────────────────────────────────
    // `conditionality_region` — the level tuples whose cells met the
    // minimum (the region the estimates hold on). `portability` reports
    // on the model axis only: ≥ 2 families + ≥ 1 held-out level, and any
    // `unknown` model-axis cell ⇒ `inconclusive` (§2.3).
    let conditionality_region: Vec<Json> = estimable
        .iter()
        .map(|k| Json::Arr(k.iter().map(Json::str).collect()))
        .collect();
    let portability = {
        let model_axis = factors.iter().enumerate().find(|(_, f)| {
            f.as_str() == "model"
                || f.as_str() == "model_snapshot"
                || design
                    .and_then(|d| d.factors.iter().find(|fd| &fd.name == *f))
                    .map(|fd| fd.kind == FactorKind::ModelSnapshot)
                    .unwrap_or(false)
        });
        match model_axis {
            None => Json::obj([("n/a", Json::str("no_model_axis"))]),
            Some((mi, _)) => {
                let mut families: BTreeSet<String> = BTreeSet::new();
                for r in &eligible {
                    for s in r.model_snapshots.values() {
                        families.insert(s.clone());
                    }
                }
                let unknown_model_cells = grid
                    .iter()
                    .filter(|k| !probed_keys.contains(k) || !estimable.contains(*k))
                    .count();
                let held_out = unknown_model_cells;
                if families.len() < 2 {
                    Json::obj([
                        ("n/a", Json::str("insufficient")),
                        ("n_families", Json::Int(families.len() as i64)),
                    ])
                } else if held_out == 0 {
                    Json::obj([
                        ("n/a", Json::str("insufficient")),
                        ("n_families", Json::Int(families.len() as i64)),
                        ("held_out_levels", Json::Int(0)),
                    ])
                } else if unknown_model_cells > 0 {
                    // An `unknown` cell on the model axis renders the
                    // verdict `inconclusive` (§2.3).
                    Json::obj([
                        ("verdict", Json::str("inconclusive")),
                        ("n_families", Json::Int(families.len() as i64)),
                        ("unknown_model_cells", Json::Int(unknown_model_cells as i64)),
                    ])
                } else {
                    // sign_stability — the ppm share of probed model-axis
                    // levels whose per-level effect sign agrees with the
                    // axis's first main effect's sign.
                    let main_sign = main_effects
                        .iter()
                        .find(|e| {
                            e.get("factor").and_then(Json::as_str) == Some(factors[mi].as_str())
                        })
                        .and_then(|e| e.get("estimate").and_then(Json::as_int))
                        .map(|v| v >= 0)
                        .unwrap_or(true);
                    let axis_effects: Vec<i64> = main_effects
                        .iter()
                        .filter(|e| {
                            e.get("factor").and_then(Json::as_str) == Some(factors[mi].as_str())
                        })
                        .filter_map(|e| e.get("estimate").and_then(Json::as_int))
                        .collect();
                    let agreeing = axis_effects
                        .iter()
                        .filter(|v| (**v >= 0) == main_sign)
                        .count();
                    let stability = if axis_effects.is_empty() {
                        Json::obj([("n/a", Json::str("estimator_undefined"))])
                    } else {
                        Json::Int(agreeing as i64 * stats::PPM / axis_effects.len() as i64)
                    };
                    Json::obj([
                        (
                            "verdict",
                            Json::str(
                                if agreeing == axis_effects.len() && !axis_effects.is_empty() {
                                    "portable"
                                } else {
                                    "not_portable"
                                },
                            ),
                        ),
                        ("sign_stability_ppm", stability),
                        ("n_families", Json::Int(families.len() as i64)),
                    ])
                }
            }
        }
    };

    // ── Expiry wiring (§2.3; AC-R-2.10.4-10) ──────────────────────────
    // The tracked refs = configuration ids ∪ model snapshot ids ∪
    // environment images ∪ design_ref ∪ suite ids — a `superseded` hit
    // fires `expired`; an `expiring` hit (warn window) or an unexpired
    // `max_age` leaves `active`/`expiring`; `fitted_at + max_age ≤ now`
    // fires `expired`.
    let mut tracked: BTreeSet<String> = BTreeSet::new();
    for r in &eligible {
        tracked.insert(r.configuration_id.clone());
        for s in r.model_snapshots.values() {
            tracked.insert(s.clone());
        }
        if let Some(e) = &r.environment_version_id {
            tracked.insert(e.clone());
        }
        tracked.insert(r.suite_id.clone());
    }
    if let Some(d) = req.design_ref {
        tracked.insert(d.to_string());
    }
    let mut configuration_ids: Vec<String> = eligible
        .iter()
        .map(|r| r.configuration_id.clone())
        .collect();
    configuration_ids.sort();
    configuration_ids.dedup();
    let expired_hit: Vec<String> = req
        .expiry
        .superseded
        .iter()
        .filter(|s| tracked.contains(*s))
        .cloned()
        .collect();
    let expiring_hit: Vec<String> = req
        .expiry
        .expiring
        .iter()
        .filter(|s| tracked.contains(*s))
        .cloned()
        .collect();
    let aged = match (
        req.expiry.fitted_at,
        req.expiry.now_ms,
        req.expiry.max_age_ms,
    ) {
        (Some(f), Some(n), Some(m)) => n.saturating_sub(f) >= m,
        _ => false,
    };
    let status = if !expired_hit.is_empty() || aged {
        "expired"
    } else if !expiring_hit.is_empty() {
        "expiring"
    } else {
        "active"
    };

    // The home-11 debt record (ADR-0197; CF-429): `refit{min_design}`
    // discharges; `evidence_refs` binds the report id; the expiry
    // condition names the full trigger union.
    let owner = req
        .expiry
        .owner
        .clone()
        .unwrap_or_else(|| "lab-analysis".to_string());
    let min_design = Json::obj([
        ("min_levels_per_factor", Json::Int(MIN_LEVELS as i64)),
        ("replicates_per_cell", Json::Int(MIN_REPLICATES)),
        ("min_tasks", Json::Int(MIN_TASKS)),
    ]);
    let debt_body = Json::obj([
        ("kind", Json::str("assumption_debt_record")),
        ("record_kind", Json::str("fitted_surface_report")),
        ("field", Json::str("debt_record_ref")),
        ("rule_id", Json::str(req.report_id)),
        ("hypothesis", Json::str("Ψ_θ over these levels is stable")),
        ("debt_class", Json::str("empirical")),
        (
            "evidence_refs",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("fitted_surface_report")),
                ("ref", Json::str(req.report_id)),
            ])]),
        ),
        ("owner", Json::obj([("principal", Json::str(&owner))])),
        (
            "expiry_condition",
            Json::obj([
                ("kind", Json::str("evidence_refresh_due")),
                (
                    "triggers",
                    Json::Arr(vec![
                        Json::str("model_snapshot_superseded_or_retired"),
                        Json::str("profile_semantic_id_changed"),
                        Json::str("suite_validity_superseded"),
                        Json::str("environment_image_superseded"),
                        Json::str("fitted_at_plus_max_age"),
                    ]),
                ),
            ]),
        ),
        (
            "expiry",
            Json::obj([
                ("condition", Json::str("evidence_refresh_due")),
                (
                    "params",
                    Json::obj([
                        (
                            "until",
                            req.expiry
                                .fitted_at
                                .zip(req.expiry.max_age_ms)
                                .map(|(f, m)| Json::Int(f + m))
                                .unwrap_or(Json::Null),
                        ),
                        (
                            "design_ref",
                            req.design_ref.map(Json::str).unwrap_or(Json::Null),
                        ),
                        ("min_design", min_design.clone()),
                    ]),
                ),
            ]),
        ),
        (
            "removal_test",
            Json::obj([
                ("kind", Json::str("refit")),
                (
                    "template_ref",
                    req.design_ref.map(Json::str).unwrap_or(Json::Null),
                ),
                ("criteria", Json::str(min_design.to_canonical_string())),
            ]),
        ),
        ("status", Json::str(status)),
        (
            "created_by",
            Json::obj([
                ("origin", Json::str("kernel")),
                ("component_ref", Json::str("hh-analysis/fit_surface")),
            ]),
        ),
        (
            "created_at",
            req.expiry.fitted_at.map(Json::Int).unwrap_or(Json::Null),
        ),
        (
            "revalidation",
            Json::obj([
                (
                    "on",
                    Json::Arr(vec![
                        Json::str("evidence_stale"),
                        Json::str("model_change"),
                        Json::str("schedule"),
                    ]),
                ),
                ("action", Json::str("re_experiment")),
            ]),
        ),
    ]);
    let debt_record_ref = hh_identity::idp_id(
        "debt.assumption",
        debt_body.to_canonical_string().as_bytes(),
    );

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
        (
            "design_ref",
            req.design_ref.map(Json::str).unwrap_or(Json::Null),
        ),
        ("model_form", Json::str(req.model_form.name())),
        (
            "match_spec_ref",
            req.match_spec_ref.map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "adjust_for",
            Json::Arr(req.adjust_for.iter().map(Json::str).collect()),
        ),
        (
            "design_minimum",
            Json::obj([
                ("min_levels_per_factor", Json::Int(MIN_LEVELS as i64)),
                ("replicates_per_cell", Json::Int(MIN_REPLICATES)),
                ("min_tasks", Json::Int(MIN_TASKS)),
                ("declared_replicates_per_cell", Json::Int(declared_reps)),
                ("n_tasks", Json::Int(n_tasks)),
                ("levels_ok", Json::Bool(levels_ok)),
                ("replicates_ok", Json::Bool(replicates_ok)),
                ("tasks_ok", Json::Bool(tasks_ok)),
                ("met", Json::Bool(design_met)),
            ]),
        ),
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
        ("insufficient_cells", Json::Arr(insufficient_cells)),
        (
            "estimates_with_ci",
            Json::obj([
                ("main_effects", Json::Arr(main_effects)),
                ("interactions", Json::Arr(interactions)),
                ("curves", Json::Arr(curves)),
                ("random_effects", random_effects),
            ]),
        ),
        ("conditionality_region", Json::Arr(conditionality_region)),
        ("portability", portability),
        ("unknown_cells", Json::Arr(unknown_cells)),
        ("na_rows", Json::Arr(na_rows)),
        (
            "configuration_ids",
            Json::Arr(configuration_ids.iter().map(Json::str).collect()),
        ),
        (
            "fitted_at",
            req.expiry.fitted_at.map(Json::Int).unwrap_or(Json::Null),
        ),
        (
            "expiry_condition",
            Json::obj([
                ("kind", Json::str("evidence_refresh_due")),
                (
                    "triggers",
                    Json::Arr(vec![
                        Json::str("model_snapshot_superseded_or_retired"),
                        Json::str("profile_semantic_id_changed"),
                        Json::str("suite_validity_superseded"),
                        Json::str("environment_image_superseded"),
                        Json::str("fitted_at_plus_max_age"),
                    ]),
                ),
                (
                    "max_age_ms",
                    req.expiry.max_age_ms.map(Json::Int).unwrap_or(Json::Null),
                ),
            ]),
        ),
        ("status", Json::str(status)),
        (
            "expired_refs",
            Json::Arr(expired_hit.iter().map(Json::str).collect()),
        ),
        (
            "expiring_refs",
            Json::Arr(expiring_hit.iter().map(Json::str).collect()),
        ),
        ("debt_record_ref", Json::str(&debt_record_ref)),
        ("debt_record", debt_body),
        ("generated_from", req.generated_from.clone()),
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

// ── A14 strata_view (C2 — §2.3/§2.5; ADR-0012 D6; S5.3) ───────────────

/// The A14 stratum axis (`filters.by` — §2.3's closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrataAxis {
    /// `contamination_stratum` (the C1 default).
    ContaminationStratum,
    /// `capability_vector` — the hosted strata axis (the participant's
    /// reconciled vector, canonical-spelling key); native rows land in
    /// the `native` stratum.
    CapabilityVector,
    /// `cost_confidence` — the run's spend-confidence class
    /// (`exact | bounded | estimate | unknown`, the *minimum* over its
    /// `measurement.cost.attributed` rows — the §8.2 reporting rule).
    CostConfidence,
    /// `suite_validity` — the suite's validity state the run's task sits
    /// in (`{suite_id}:{valid|retired}`).
    SuiteValidity,
    /// `mediation` — the participant's declared mediation channel set.
    Mediation,
}

impl StrataAxis {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            StrataAxis::ContaminationStratum => "contamination_stratum",
            StrataAxis::CapabilityVector => "capability_vector",
            StrataAxis::CostConfidence => "cost_confidence",
            StrataAxis::SuiteValidity => "suite_validity",
            StrataAxis::Mediation => "mediation",
        }
    }

    /// Parse; `None` on any other spelling (the caller refuses with
    /// `unknown stratum`).
    pub fn parse(s: &str) -> Option<StrataAxis> {
        match s {
            "contamination_stratum" => Some(StrataAxis::ContaminationStratum),
            "capability_vector" => Some(StrataAxis::CapabilityVector),
            "cost_confidence" => Some(StrataAxis::CostConfidence),
            "suite_validity" => Some(StrataAxis::SuiteValidity),
            "mediation" => Some(StrataAxis::Mediation),
            _ => None,
        }
    }
}

/// The stratum key a run lands in for `axis` (hosted strata by
/// `capability_vector`/`mediation`/cost `confidence` per §2.5 — a hosted
/// run's vector is canonicalised, never pooled into a neighbour).
fn stratum_key(r: &EvalRun, axis: StrataAxis, suites: &[hh_eval::runs::SuiteContext]) -> String {
    match axis {
        StrataAxis::ContaminationStratum => r.stratum.name().to_string(),
        StrataAxis::CapabilityVector => {
            if !is_hosted(r) {
                "native".to_string()
            } else {
                // The canonical `{coordinate:verdict}` spelling — two
                // runs with equal vectors share a stratum; a differing
                // `unknown`/`SUPPORTED` entry is a *different* stratum
                // (never coerced — T-LCD-07).
                let mut parts: Vec<String> = r
                    .capability_vector
                    .iter()
                    .map(|(k, v)| format!("{k}:{v:?}").to_lowercase())
                    .collect();
                parts.sort();
                format!("{{{}}}", parts.join(","))
            }
        }
        StrataAxis::CostConfidence => {
            // The minimum spend-row confidence on the run (the §8.2
            // "report the minimum" rule); no spend rows → `unknown`.
            let mut best = "unknown";
            let mut best_rank = 0u8;
            let mut seen = false;
            for s in &r.facts.spend_rows {
                let (rank, label) = match &s.confidence {
                    Some(hh_budget::pricing::Confidence::Exact) => (3, "exact"),
                    Some(hh_budget::pricing::Confidence::Bounded { .. }) => (2, "bounded"),
                    Some(hh_budget::pricing::Confidence::Estimate) => (1, "estimate"),
                    _ => (0, "unknown"),
                };
                if !seen || rank < best_rank {
                    best_rank = rank;
                    best = label;
                    seen = true;
                }
            }
            // A `reconstructed`/`participant_reported` provenance class
            // is a distinct stratum label (never folded into `exact`).
            let recon = r
                .facts
                .spend_rows
                .iter()
                .any(|s| s.provenance_class.as_deref() == Some("reconstructed"));
            if recon {
                "reconstructed".to_string()
            } else {
                best.to_string()
            }
        }
        StrataAxis::SuiteValidity => {
            let retired = suites
                .iter()
                .find(|s| s.suite_id == r.suite_id)
                .map(|s| s.retired_for_headline)
                .unwrap_or(false);
            format!(
                "{}:{}",
                r.suite_id,
                if retired { "retired" } else { "valid" }
            )
        }
        StrataAxis::Mediation => {
            if r.mediation.is_empty() {
                "none".to_string()
            } else {
                let mut parts: Vec<&str> = r.mediation.iter().map(|m| m.as_str()).collect();
                parts.sort();
                parts.join("+")
            }
        }
    }
}

/// A14 `strata_view(rows, metric, by)` — the strata table: per-stratum
/// run counts, per-stratum metric means, hosted rows split by the named
/// axis. Strata are **never silently pooled** — the report carries
/// `pooled: never` and the kernel refuses `aggregate = pooled` requests
/// with `StrataPooledUnannotated` before calling (ADR-0012 D6).
pub fn strata_view(
    runs: &[EvalRun],
    metric: &str,
    by: StrataAxis,
    suites: &[hh_eval::runs::SuiteContext],
) -> Json {
    let mut strata: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut stratum_runs: BTreeMap<String, u64> = BTreeMap::new();
    let mut stratum_classes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in runs {
        let s = stratum_key(r, by, suites);
        *stratum_runs.entry(s.clone()).or_default() += 1;
        stratum_classes
            .entry(s.clone())
            .or_default()
            .insert(r.participant_class.as_str().to_string());
        if let MetricValueKind::Decimal(v) = r.value_for(metric) {
            strata.entry(s).or_default().push(v);
        } else if let MetricValueKind::Bool(b) = r.value_for(metric) {
            strata
                .entry(s)
                .or_default()
                .push(if b { 1_000_000 } else { 0 });
        }
    }
    Json::obj([
        ("schema", Json::str("hh-strata-view/1")),
        ("metric", Json::str(metric)),
        ("by", Json::str(by.name())),
        ("pooled", Json::str("never")),
        (
            "strata",
            Json::Arr(
                stratum_runs
                    .iter()
                    .map(|(s, n)| {
                        let vals = strata.get(s).cloned().unwrap_or_default();
                        Json::obj([
                            ("stratum", Json::str(s)),
                            (
                                "classes",
                                Json::Arr(
                                    stratum_classes
                                        .get(s)
                                        .map(|c| c.iter().map(Json::str).collect())
                                        .unwrap_or_default(),
                                ),
                            ),
                            ("runs", Json::Int(*n as i64)),
                            (
                                "mean_ppm",
                                stats::mean(&vals).map_or(
                                    Json::obj([("n/a", Json::str("estimator_undefined"))]),
                                    Json::Int,
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

// ── A6 realized-benefit stages (C2 — §2.3/§5h.2 §2.1; S5.3) ──────────

/// The four-stage realized-benefit decomposition (§2.3:
/// `P(valid)·P(activated|delivered,valid)·P(followed|activated)·
/// E[Δ|followed]`, per detector) plus the zeroth `delivered` observability
/// row. Every stage renders a confidence-band row (`wilson` for the
/// proportions, clustered-CLT for the conditional Δ — no point without a
/// band at C2). A hosted row's stages beyond `delivered` render
/// `n/a{observability}` (§2.5); an `end_state`-only hosted row's
/// `delivered` row is `n/a{observability}` too; a judged stage on a row
/// without `model_io` is `n/a{observability}` — never 0, never a proxy.
pub fn realized_stages(
    arm_runs: &[&EvalRun],
    baseline_runs: &[&EvalRun],
    metric: &str,
    confidence_ppm: i64,
) -> Json {
    use hh_ontology::participant::Observability as Obs;
    let native: Vec<&&EvalRun> = arm_runs.iter().filter(|r| !is_hosted(r)).collect();
    let hosted: Vec<&&EvalRun> = arm_runs.iter().filter(|r| is_hosted(r)).collect();
    let mut rows = Vec::new();
    let na_obs = || Json::obj([("n/a", Json::str("observability"))]);
    // `delivered` — runs with ≥ 1 `context.artefact.delivered` row
    // (observable wherever `events` is).
    let delivered = |rs: &[&&EvalRun]| -> Json {
        let n = rs.len() as i64;
        if n == 0 {
            return na_obs();
        }
        if rs
            .iter()
            .all(|r| !r.observability_level.contains(&Obs::Events))
        {
            return na_obs();
        }
        let c = rs
            .iter()
            .filter(|r| !r.facts.artefacts_delivered.is_empty())
            .count() as i64;
        stats::wilson(c, n, confidence_ppm).map_or(na_obs(), |iv| {
            Json::obj([
                ("point_ppm", Json::Int(c * stats::PPM / n)),
                ("n", Json::Int(n)),
                (
                    "interval",
                    Json::obj([("lo", Json::Int(iv.lo)), ("hi", Json::Int(iv.hi))]),
                ),
            ])
        })
    };
    rows.push(Json::obj([
        ("stage", Json::str("delivered")),
        ("detector", Json::Null),
        ("native", delivered(&native)),
        (
            "hosted",
            if hosted.is_empty() {
                Json::Null
            } else {
                delivered(&hosted)
            },
        ),
    ]));
    // The hosted-side stage rows — `n/a{observability}` beyond
    // `delivered` (§2.5); container-installed rows (no `events`) mark
    // even `delivered` `n/a` above.
    let hosted_stage = |detector: &str| -> Json {
        if hosted.is_empty() {
            return Json::Null;
        }
        // A `judged` stage on rows lacking `model_io` is `n/a` — and the
        // §2.5 rule marks every post-`delivered` hosted stage `n/a`.
        let _ = detector;
        na_obs()
    };
    // `valid` per detector — the pass rate over `verification.validator.
    // verdict` rows (a row with no detector lands in `deterministic`).
    let mut by_detector: BTreeMap<String, Vec<(bool,)>> = BTreeMap::new();
    for r in &native {
        for v in &r.facts.verdicts {
            let d = v.detector.clone().unwrap_or_else(|| "deterministic".into());
            by_detector
                .entry(d)
                .or_default()
                .push((v.status == "pass",));
        }
    }
    for (det, vs) in &by_detector {
        let n = vs.len() as i64;
        let c = vs.iter().filter(|(p,)| *p).count() as i64;
        let row = stats::wilson(c, n, confidence_ppm).map_or(
            Json::obj([("n/a", Json::str("estimator_undefined"))]),
            |iv| {
                Json::obj([
                    ("point_ppm", Json::Int(c * stats::PPM / n)),
                    ("n", Json::Int(n)),
                    (
                        "interval",
                        Json::obj([("lo", Json::Int(iv.lo)), ("hi", Json::Int(iv.hi))]),
                    ),
                ])
            },
        );
        rows.push(Json::obj([
            ("stage", Json::str("valid")),
            ("detector", Json::str(det)),
            ("native", row),
            ("hosted", hosted_stage(det)),
        ]));
    }
    if by_detector.is_empty() {
        rows.push(Json::obj([
            ("stage", Json::str("valid")),
            ("detector", Json::Null),
            (
                "native",
                Json::obj([("n/a", Json::str("estimator_undefined"))]),
            ),
            ("hosted", hosted_stage("")),
        ]));
    }
    // `activated | delivered ∧ valid` — the activated artefact ids
    // meeting a delivered one, per detector of the *activated* row.
    let mut act_by_det: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in &native {
        let delivered_ids: BTreeSet<&str> = r
            .facts
            .artefacts_delivered
            .iter()
            .map(|a| a.artefact_id.as_str())
            .collect();
        for a in &r.facts.artefacts_activated {
            let d = a.detector.clone().unwrap_or_else(|| "deterministic".into());
            let e = act_by_det.entry(d).or_default();
            e.1 += 1;
            if delivered_ids.contains(a.artefact_id.as_str()) {
                e.0 += 1;
            }
        }
    }
    let mut fol_by_det: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in &native {
        let activated_ids: BTreeSet<&str> = r
            .facts
            .artefacts_activated
            .iter()
            .map(|a| a.artefact_id.as_str())
            .collect();
        for a in &r.facts.artefacts_followed {
            let d = a.detector.clone().unwrap_or_else(|| "deterministic".into());
            let e = fol_by_det.entry(d).or_default();
            e.1 += 1;
            if activated_ids.contains(a.artefact_id.as_str()) {
                e.0 += 1;
            }
        }
    }
    let stage_row = |stage: &str, det: &str, c: u64, n: u64| -> Json {
        let cell = if n == 0 {
            Json::obj([("n/a", Json::str("estimator_undefined"))])
        } else {
            stats::wilson(c as i64, n as i64, confidence_ppm).map_or(
                Json::obj([("n/a", Json::str("estimator_undefined"))]),
                |iv| {
                    Json::obj([
                        ("point_ppm", Json::Int(c as i64 * stats::PPM / n as i64)),
                        ("n", Json::Int(n as i64)),
                        (
                            "interval",
                            Json::obj([("lo", Json::Int(iv.lo)), ("hi", Json::Int(iv.hi))]),
                        ),
                    ])
                },
            )
        };
        Json::obj([
            ("stage", Json::str(stage)),
            ("detector", Json::str(det)),
            ("native", cell),
            ("hosted", hosted_stage(det)),
        ])
    };
    for (det, (c, n)) in &act_by_det {
        rows.push(stage_row("activated_given_delivered_valid", det, *c, *n));
    }
    if act_by_det.is_empty() {
        rows.push(Json::obj([
            ("stage", Json::str("activated_given_delivered_valid")),
            ("detector", Json::Null),
            (
                "native",
                Json::obj([("n/a", Json::str("estimator_undefined"))]),
            ),
            ("hosted", hosted_stage("")),
        ]));
    }
    for (det, (c, n)) in &fol_by_det {
        rows.push(stage_row("followed_given_activated", det, *c, *n));
    }
    if fol_by_det.is_empty() {
        rows.push(Json::obj([
            ("stage", Json::str("followed_given_activated")),
            ("detector", Json::Null),
            (
                "native",
                Json::obj([("n/a", Json::str("estimator_undefined"))]),
            ),
            ("hosted", hosted_stage("")),
        ]));
    }
    // `E[Δ | followed]` — the per-task paired delta restricted to arm
    // runs carrying ≥ 1 `followed` row (per detector when the followed
    // rows carry one); a clustered-CLT band, never a bare point.
    let mut det_set: BTreeSet<String> = fol_by_det.keys().cloned().collect();
    det_set.insert("deterministic".into());
    for det in &det_set {
        let mut deltas: Vec<(String, i64)> = Vec::new();
        let mut a_map: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for r in &native {
            let has = r
                .facts
                .artefacts_followed
                .iter()
                .any(|a| a.detector.as_deref().unwrap_or("deterministic") == det);
            if has {
                if let Some(v) = point(r, metric) {
                    a_map.entry(r.task_id.clone()).or_default().push(v);
                }
            }
        }
        let mut b_map: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for r in baseline_runs {
            if let Some(v) = point(r, metric) {
                b_map.entry(r.task_id.clone()).or_default().push(v);
            }
        }
        for (t, avs) in &a_map {
            if let Some(bvs) = b_map.get(t) {
                if let (Some(a), Some(b)) = (stats::mean(avs), stats::mean(bvs)) {
                    deltas.push((t.clone(), a - b));
                }
            }
        }
        let est = stats::mean(&deltas.iter().map(|(_, d)| *d).collect::<Vec<_>>());
        let iv = stats::clustered_clt(&deltas, confidence_ppm);
        let cell = match (est, iv) {
            (Some(e), Some(iv)) => Json::obj([
                ("point_ppm", Json::Int(e)),
                ("n_tasks", Json::Int(deltas.len() as i64)),
                (
                    "interval",
                    Json::obj([("lo", Json::Int(iv.lo)), ("hi", Json::Int(iv.hi))]),
                ),
            ]),
            _ => Json::obj([("n/a", Json::str("estimator_undefined"))]),
        };
        rows.push(Json::obj([
            ("stage", Json::str("delta_given_followed")),
            ("detector", Json::str(det)),
            ("native", cell),
            ("hosted", hosted_stage(det)),
        ]));
    }
    Json::obj([
        ("schema", Json::str("hh-realized-stages/1")),
        ("metric", Json::str(metric)),
        ("stages", Json::Arr(rows)),
    ])
}

// ── S5.4: component targets + the attribution design helper — R-2.9.7¹ ──────

/// `component_targets(sealed_definition, filter?) → [ComponentTarget]` —
/// the typed-target enumeration (§5h.7; R-2.9.7¹): walks the sealed
/// definition's members and emits `{kind, ref}` rows —
/// `slots[]`→`slot`, `variants[]`→`variant`, `rules[]`→`rule`,
/// `parameters`/`parameter_defaults` keys→`parameter`,
/// `procedures[]`→`procedure`, `leaves[]`/`text[]`→`text_leaf`,
/// `decision_points[]`→`decision_point`. `filter` narrows by
/// `{kinds[], pattern}` (a substring match on `ref` — records-in,
/// records-out; nothing here reads a store).
pub fn component_targets(definition: &Json, filter: Option<&Json>) -> Vec<Json> {
    let kinds: Option<Vec<String>> = filter.and_then(|f| f.get("kinds")).and_then(|k| match k {
        Json::Arr(items) => Some(
            items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    });
    let pattern = filter
        .and_then(|f| f.get("pattern"))
        .and_then(Json::as_str)
        .map(str::to_string);
    let mut out: Vec<Json> = Vec::new();
    let mut push = |kind: &'static str, reference: &str| {
        if let Some(ks) = &kinds {
            if !ks.iter().any(|k| k == kind) {
                return;
            }
        }
        if let Some(p) = &pattern {
            if !reference.contains(p.as_str()) {
                return;
            }
        }
        out.push(Json::obj([
            ("kind", Json::str(kind)),
            ("ref", Json::str(reference)),
        ]));
    };
    let arr_refs = |m: Option<&Json>| -> Vec<String> {
        match m {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| v.get("ref").and_then(Json::as_str).map(str::to_string))
                        .or_else(|| v.get("name").and_then(Json::as_str).map(str::to_string))
                        .or_else(|| v.get("rule_id").and_then(Json::as_str).map(str::to_string))
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    if let Json::Obj(m) = definition {
        for s in arr_refs(m.get("slots")) {
            push("slot", &s);
        }
        for s in arr_refs(m.get("variants")) {
            push("variant", &s);
        }
        for s in arr_refs(m.get("rules")) {
            push("rule", &s);
        }
        for member in ["parameters", "parameter_defaults"] {
            if let Some(Json::Obj(pm)) = m.get(member) {
                for k in pm.keys() {
                    push("parameter", k);
                }
            }
        }
        for s in arr_refs(m.get("procedures")) {
            push("procedure", &s);
        }
        for member in ["leaves", "text", "text_leaves"] {
            for s in arr_refs(m.get(member)) {
                push("text_leaf", &s);
            }
        }
        for s in arr_refs(m.get("decision_points")) {
            push("decision_point", &s);
        }
    }
    out.sort_by_key(|a| a.to_canonical_string());
    out.dedup();
    out
}

/// `attribution_design(design_ref, targets, arms, match_spec_ref?,
/// budget_allocation?) → AttributionDesign` — the design document the M1
/// kernel consumes (§5h.7; R-2.9.7¹): `{schema: hh-attribution-design/1,
/// design_ref, targets[], arms[], match_spec_ref?, budget_allocation?,
/// method: M1}`.
pub fn attribution_design(
    design_ref: &str,
    targets: Vec<Json>,
    arms: &[String],
    match_spec_ref: Option<&str>,
    budget_allocation: Option<Json>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("schema".into(), Json::str("hh-attribution-design/1"));
    m.insert("design_ref".into(), Json::str(design_ref));
    m.insert("method".into(), Json::str("M1"));
    m.insert("targets".into(), Json::Arr(targets));
    m.insert(
        "arms".into(),
        Json::Arr(arms.iter().map(Json::str).collect()),
    );
    if let Some(ms) = match_spec_ref {
        m.insert("match_spec_ref".into(), Json::str(ms));
    }
    if let Some(b) = budget_allocation {
        m.insert("budget_allocation".into(), b);
    }
    Json::Obj(m)
}
