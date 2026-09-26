//! `frontier` — the M2 capability–cost Pareto frontier with provenance
//! bands (spec §8.2 §6.4 A4; AC-R-2.1.6-8: "a comparison containing
//! reported and measured spend rows renders separate frontiers plus a
//! combined band; a coverage<1 row is never summed as complete";
//! ADR-0043 D4 — `confidence ∈ {estimate, unknown}` or `coverage < 1`
//! renders a band, never a point).
//!
//! The frontier is a *comparison* surface: it runs under the same
//! `validate_match` gate as `compare` (M1 `matched_cap` / M2 `iso_cost`
//! arms only — an exploratory `MatchSpec{mode:none}` analysis may
//! execute but never reports a frontier, matching §8.2's "may execute but
//! may not report comparisons").
//!
//! Aggregation is per `(arm_id, provenance_class, currency)`: spend rows
//! are summed *within* a currency (never across), the capability axis is
//! the mean of the spec's first metric over the runs contributing rows to
//! the cell, `confidence_min`/`coverage_min_ppm` carry the weakest row
//! (the minimum, never an average — the same rule `cost_view` applies).
//! A cell is frontier-point admissible only when every row is
//! `exact`/`bounded` with `coverage = 1`; anything less renders a band.
//! Dominance (cost ↓, capability ↑) is computed among point-admissible
//! cells of equal currency only — cross-currency cells never dominate
//! each other (`incommensurable`, not silently converted).
//!
//! C1 additions (spec §6.4 A4; S4.3): per-point `configuration_id`/
//! `class`, `capability{point, interval}` (clustered over tasks),
//! `cost{provenance, confidence}`, `on_frontier`, `dominated_by[]`,
//! `membership_probability` (task-resampled, shared `seed`/`draws`),
//! `cost_of_pass[]` (`n/a` when the capability interval includes 0 — the
//! pass probability is unidentified), the `origin` marker, the
//! `strata{confidence, class}` decomposition, and the
//! `match_spec_ref`/`price_table_version`/`seed`/`draws` identity
//! members.

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::pricing::Confidence;
use hh_budget::quantity::PPM_SCALE;
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::EvalRun;
use hh_eval::scorecard::run_numeric;
use hh_eval::stats::{self, Interval};
use hh_ontology::compliance::MetricDeclaration;
use hh_wire::Json;

/// One aggregated frontier candidate — an arm's realized
/// `(cost, capability)` over one `(provenance_class, currency)` slice.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontierCandidate {
    /// The experiment arm.
    pub arm_id: String,
    /// `measured | reported | reconstructed | unknown`.
    pub provenance_class: String,
    /// `money.currency` (`"unknown"` when the rows carry none).
    pub currency: String,
    /// Summed spend micro-units (within-currency only).
    pub cost_micros: i64,
    /// The mean capability metric value (ppm) over contributing runs.
    pub capability_ppm: i64,
    /// The weakest row confidence (`None` members count `unknown`).
    pub confidence_min: Confidence,
    /// The weakest row coverage (`None` members count `0` — absent
    /// coverage is never asserted complete).
    pub coverage_min_ppm: i64,
    /// The runs that contributed spend rows to the cell.
    pub run_ids: Vec<String>,
    /// The spend-row count.
    pub spend_rows: usize,
    /// The cell's configuration ids (sorted — the entry key axis).
    pub configuration_ids: Vec<String>,
    /// The cell's participant class (the runs agree — the arm binds it).
    pub participant_class: String,
    /// `(task_id, per-run capability value)` — the resample/interval unit
    /// (clustered by task — never flattened runs).
    pub task_values: Vec<(String, i64)>,
    /// Runs whose capability value resolved numeric.
    pub runs_valued: u64,
    /// Runs scoring positive (the cost-of-pass numerator count).
    pub runs_successful: u64,
}

impl FrontierCandidate {
    /// The stable point/band id (`arm|class|currency`).
    pub fn point_id(&self) -> String {
        format!(
            "{}|{}|{}",
            self.arm_id, self.provenance_class, self.currency
        )
    }

    /// `bands, never points` — a cell is a frontier *point* only when the
    /// weakest row is `exact`/`bounded` at full coverage (AC-R-2.1.6-8;
    /// `Confidence::is_point_admissible` is the one vocabulary).
    pub fn is_point_admissible(&self) -> bool {
        self.confidence_min.is_point_admissible() && self.coverage_min_ppm >= PPM_SCALE
    }

    /// The cost band: `bounded` confidence carries its `{lo, hi}`;
    /// anything else reports the summed figure at both ends with the
    /// `confidence`/`coverage` members carrying the uncertainty marker.
    pub fn cost_band(&self) -> (i64, i64) {
        match self.confidence_min {
            Confidence::Bounded { lo, hi } => (lo.min(hi), hi.max(lo)),
            _ => (self.cost_micros, self.cost_micros),
        }
    }

    /// The capability interval — clustered-CLT over per-task means
    /// (`k < 2` tasks → `None`, rendered `n/a`).
    pub fn capability_interval(&self, confidence_ppm: i64) -> Option<Interval> {
        stats::clustered_clt(&self.task_values, confidence_ppm)
    }

    /// `cost_of_pass` — expected micro-units per success:
    /// `(cost_micros / runs_valued) / p_success`. The Wilson interval on
    /// `p` inverts to the cost interval (`lo` binds the high-p end).
    /// `None` when the capability interval includes 0 or the success
    /// count is 0 — the pass probability is unidentified there.
    pub fn cost_of_pass(&self, confidence_ppm: i64) -> Option<(i64, Interval)> {
        if self.runs_valued == 0 || self.runs_successful == 0 {
            return None;
        }
        let interval = self.capability_interval(confidence_ppm)?;
        if interval.lo <= 0 {
            return None;
        }
        let p = stats::wilson(
            self.runs_successful as i64,
            self.runs_valued as i64,
            confidence_ppm,
        )?;
        if p.hi <= 0 {
            return None;
        }
        let per_attempt = self.cost_micros as i128 / self.runs_valued as i128;
        let point = (per_attempt * stats::PPM as i128
            / (self.runs_successful as i128 * stats::PPM as i128 / self.runs_valued as i128))
            as i64;
        Some((
            point,
            Interval {
                lo: (per_attempt * stats::PPM as i128 / p.hi as i128) as i64,
                hi: (per_attempt * stats::PPM as i128 / p.lo.max(1) as i128) as i64,
            },
        ))
    }
}

/// `a` dominates `b` on (cost ↓, capability ↑) — same currency only.
fn dominates(a: &FrontierCandidate, b: &FrontierCandidate) -> bool {
    a.currency == b.currency
        && a.cost_micros <= b.cost_micros
        && a.capability_ppm >= b.capability_ppm
        && (a.cost_micros < b.cost_micros || a.capability_ppm > b.capability_ppm)
}

/// The Pareto-non-dominated subset of the point-admissible cells,
/// cost-ascending (ties: capability-descending, then id — deterministic).
pub fn pareto_frontier(cells: &[FrontierCandidate]) -> Vec<FrontierCandidate> {
    let pts: Vec<&FrontierCandidate> = cells.iter().filter(|c| c.is_point_admissible()).collect();
    let mut out: Vec<FrontierCandidate> = pts
        .iter()
        .copied()
        .filter(|p| !pts.iter().any(|q| !std::ptr::eq(*q, *p) && dominates(q, p)))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        (
            a.cost_micros,
            std::cmp::Reverse(a.capability_ppm),
            a.point_id(),
        )
            .cmp(&(
                b.cost_micros,
                std::cmp::Reverse(b.capability_ppm),
                b.point_id(),
            ))
    });
    out
}

/// The ids of the point-admissible cells that dominate `c` (the
/// `dominated_by` member — `[]` on frontier members).
fn dominated_by(c: &FrontierCandidate, cells: &[FrontierCandidate]) -> Vec<String> {
    let mut ids: Vec<String> = cells
        .iter()
        .filter(|q| q.is_point_admissible() && q.point_id() != c.point_id() && dominates(q, c))
        .map(|q| q.point_id())
        .collect();
    ids.sort();
    ids
}

/// `membership_probability` — the ppm share of `draws` task-resamples in
/// which the point-admissible cell is non-dominated. Costs are measured
/// (fixed); only the capability axis resamples — per-task means are
/// resampled with replacement per draw, dominance re-evaluated over the
/// resampled means (cross-currency cells still never interact).
fn membership_probability(
    c: &FrontierCandidate,
    point_cells: &[FrontierCandidate],
    draws: u64,
    seed: u64,
) -> i64 {
    if draws == 0 {
        return stats::PPM;
    }
    // A cell with no task spread cannot move — its membership is
    // determined (resampling one task is identity).
    let tasks: BTreeSet<&str> = c.task_values.iter().map(|(t, _)| t.as_str()).collect();
    let mut member = 0u64;
    let mut rng = stats::XorShift64::seeded(&format!("frontier.membership/{seed}"));
    // The resampled capability means for every point cell (same stream
    // for every candidate — the resample is over the shared draw index).
    for _ in 0..draws {
        let mut means: BTreeMap<String, i64> = BTreeMap::new();
        for q in point_cells {
            let mut by_task: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
            for (t, v) in &q.task_values {
                by_task.entry(t.as_str()).or_default().push(*v);
            }
            let keys: Vec<&str> = by_task.keys().copied().collect();
            let resampled_mean = if keys.is_empty() {
                q.capability_ppm
            } else {
                let mut sum: i128 = 0;
                for _ in 0..keys.len() {
                    let t = keys[rng.below(keys.len())];
                    sum += stats::mean(&by_task[t]).unwrap_or(0) as i128;
                }
                ((sum + keys.len() as i128 / 2) / keys.len() as i128) as i64
            };
            means.insert(q.point_id(), resampled_mean);
        }
        let my_cap = means.get(&c.point_id()).copied().unwrap_or(c.capability_ppm);
        let dominated = point_cells.iter().any(|q| {
            q.point_id() != c.point_id()
                && q.currency == c.currency
                && q.cost_micros <= c.cost_micros
                && means.get(&q.point_id()).copied().unwrap_or(q.capability_ppm) >= my_cap
                && (q.cost_micros < c.cost_micros
                    || means.get(&q.point_id()).copied().unwrap_or(q.capability_ppm) > my_cap)
        });
        if !dominated {
            member += 1;
        }
    }
    ((member as i128 * stats::PPM as i128) / draws as i128) as i64
}

/// Aggregate `runs` (+ their `LedgerFacts.spend_rows`) into the
/// `(arm, class, currency)` cells. `metric` supplies the capability axis.
pub fn aggregate(
    metric: &MetricDeclaration,
    runs: &[&EvalRun],
    facts: &BTreeMap<String, LedgerFacts>,
) -> (Vec<FrontierCandidate>, Vec<String>) {
    struct Acc {
        arm_id: String,
        class: String,
        currency: String,
        cost: i64,
        run_ids: BTreeSet<String>,
        cap_sum: i128,
        cap_n: u64,
        confidence_min: Confidence,
        coverage_min: i64,
        rows: usize,
        configs: BTreeSet<String>,
        pclass: String,
        task_values: Vec<(String, i64)>,
        valued: u64,
        successes: u64,
    }
    let mut cells: BTreeMap<(String, String, String), Acc> = BTreeMap::new();
    let mut no_spend: Vec<String> = Vec::new();
    for r in runs {
        let f = facts.get(&r.run_id);
        let spend_rows: &[hh_eval::facts::SpendRowFact] =
            f.map(|f| f.spend_rows.as_slice()).unwrap_or(&[]);
        if spend_rows.is_empty() {
            no_spend.push(r.run_id.clone());
            continue;
        }
        let cap = run_numeric(metric, r);
        for row in spend_rows {
            let class = row
                .provenance_class
                .clone()
                .unwrap_or_else(|| "unknown".into());
            let currency = row.currency.clone().unwrap_or_else(|| "unknown".into());
            let key = (r.arm_id.clone(), class.clone(), currency.clone());
            let acc = cells.entry(key).or_insert_with(|| Acc {
                arm_id: r.arm_id.clone(),
                class,
                currency,
                cost: 0,
                run_ids: BTreeSet::new(),
                cap_sum: 0,
                cap_n: 0,
                confidence_min: Confidence::Exact,
                coverage_min: PPM_SCALE,
                rows: 0,
                configs: BTreeSet::new(),
                pclass: r.participant_class.as_str().to_string(),
                task_values: Vec::new(),
                valued: 0,
                successes: 0,
            });
            acc.cost += row.micro_units.unwrap_or(0);
            acc.rows += 1;
            // The weakest row governs — never an average.
            let conf = row.confidence.unwrap_or(Confidence::Unknown);
            if conf.rank() < acc.confidence_min.rank() {
                acc.confidence_min = conf;
            }
            acc.coverage_min = acc.coverage_min.min(row.coverage_ppm.unwrap_or(0));
            if acc.run_ids.insert(r.run_id.clone()) {
                acc.configs.insert(r.configuration_id.clone());
                if let Some(v) = cap {
                    acc.cap_sum += v as i128;
                    acc.cap_n += 1;
                    acc.task_values.push((r.task_id.clone(), v));
                    acc.valued += 1;
                    if v > 0 {
                        acc.successes += 1;
                    }
                }
            }
        }
    }
    let out = cells
        .into_values()
        .map(|a| FrontierCandidate {
            capability_ppm: if a.cap_n == 0 {
                0
            } else {
                (a.cap_sum / a.cap_n as i128) as i64
            },
            arm_id: a.arm_id,
            provenance_class: a.class,
            currency: a.currency,
            cost_micros: a.cost,
            confidence_min: a.confidence_min,
            coverage_min_ppm: a.coverage_min,
            run_ids: a.run_ids.into_iter().collect(),
            spend_rows: a.rows,
            configuration_ids: a.configs.into_iter().collect(),
            participant_class: a.pclass,
            task_values: a.task_values,
            runs_valued: a.valued,
            runs_successful: a.successes,
        })
        .collect();
    no_spend.sort();
    (out, no_spend)
}

fn confidence_label(c: Confidence) -> &'static str {
    match c {
        Confidence::Exact => "exact",
        Confidence::Bounded { .. } => "bounded",
        Confidence::Estimate => "estimate",
        Confidence::Unknown => "unknown",
    }
}

/// The `frontier` body member (§8.2 M2 + §6.4 A4 output):
/// `{metric, match_spec_ref, price_table_version, seed, draws, origin,
/// points[], bands[], frontiers{class → [point_id]}, combined[],
/// strata{confidence, class}, provenance_mix{arm → class → micros},
/// cost_of_pass[], excluded{no_spend}}`.
#[allow(clippy::too_many_arguments)] // the identity members are the report's — arity is honest.
pub fn frontier_report(
    metric: &MetricDeclaration,
    runs: &[&EvalRun],
    facts: &BTreeMap<String, LedgerFacts>,
    confidence_ppm: i64,
    seed: u64,
    draws: u64,
    match_spec_ref: &str,
    price_table_version: &str,
) -> Json {
    let (cells, no_spend) = aggregate(metric, runs, facts);

    // Per-class + combined frontiers over the point-admissible cells.
    let mut by_class: BTreeMap<String, Vec<FrontierCandidate>> = BTreeMap::new();
    for c in &cells {
        by_class
            .entry(c.provenance_class.clone())
            .or_default()
            .push(c.clone());
    }
    let frontiers: Json = Json::Obj(
        by_class
            .iter()
            .map(|(class, cs)| {
                (
                    class.clone(),
                    Json::Arr(
                        pareto_frontier(cs)
                            .iter()
                            .map(|p| Json::str(p.point_id()))
                            .collect(),
                    ),
                )
            })
            .collect(),
    );
    let combined = pareto_frontier(&cells);
    let combined_ids: BTreeSet<String> = combined.iter().map(|p| p.point_id()).collect();
    let point_cells: Vec<FrontierCandidate> = cells
        .iter()
        .filter(|c| c.is_point_admissible())
        .cloned()
        .collect();

    let capability_json = |c: &FrontierCandidate| {
        let interval = c.capability_interval(confidence_ppm);
        Json::obj([
            ("point", Json::Int(c.capability_ppm)),
            (
                "interval",
                interval
                    .map(|i| {
                        Json::obj([
                            ("lo", Json::Int(i.lo)),
                            ("hi", Json::Int(i.hi)),
                            ("method", Json::str("clustered_clt")),
                        ])
                    })
                    .unwrap_or_else(|| Json::obj([("n/a", Json::str("estimator_undefined"))])),
            ),
            (
                "capability_vector",
                Json::Obj(
                    c.task_values
                        .iter()
                        .map(|(t, v)| (t.clone(), Json::Int(*v)))
                        .collect(),
                ),
            ),
        ])
    };

    let cost_json = |c: &FrontierCandidate| {
        let (lo, hi) = c.cost_band();
        let cost_value = if lo == hi {
            Json::Int(c.cost_micros)
        } else {
            Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))])
        };
        Json::obj([
            ("micros", cost_value),
            ("currency", Json::str(&c.currency)),
            ("provenance", Json::str(&c.provenance_class)),
            (
                "confidence",
                Json::str(confidence_label(c.confidence_min)),
            ),
            ("coverage_min_ppm", Json::Int(c.coverage_min_ppm)),
        ])
    };

    let points: Vec<Json> = point_cells
        .iter()
        .map(|c| {
            let membership = membership_probability(c, &point_cells, draws, seed);
            Json::obj([
                ("point_id", Json::str(c.point_id())),
                ("arm_id", Json::str(&c.arm_id)),
                (
                    "configuration_ids",
                    Json::Arr(c.configuration_ids.iter().map(Json::str).collect()),
                ),
                ("class", Json::str(&c.participant_class)),
                ("provenance_class", Json::str(&c.provenance_class)),
                ("currency", Json::str(&c.currency)),
                ("cost", cost_json(c)),
                ("capability", capability_json(c)),
                ("cost_micros", Json::Int(c.cost_micros)),
                ("capability_ppm", Json::Int(c.capability_ppm)),
                (
                    "confidence_min",
                    Json::str(confidence_label(c.confidence_min)),
                ),
                ("coverage_min_ppm", Json::Int(c.coverage_min_ppm)),
                ("spend_rows", Json::Int(c.spend_rows as i64)),
                (
                    "run_ids",
                    Json::Arr(c.run_ids.iter().map(Json::str).collect()),
                ),
                (
                    "on_frontier",
                    Json::Bool(combined_ids.contains(&c.point_id())),
                ),
                (
                    "membership_probability_ppm",
                    Json::Int(membership),
                ),
                (
                    "dominated_by",
                    Json::Arr(
                        dominated_by(c, &cells)
                            .iter()
                            .map(Json::str)
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    let bands: Vec<Json> = cells
        .iter()
        .filter(|c| !c.is_point_admissible())
        .map(|c| {
            let (lo, hi) = c.cost_band();
            Json::obj([
                ("point_id", Json::str(c.point_id())),
                ("arm_id", Json::str(&c.arm_id)),
                (
                    "configuration_ids",
                    Json::Arr(c.configuration_ids.iter().map(Json::str).collect()),
                ),
                ("class", Json::str(&c.participant_class)),
                ("provenance_class", Json::str(&c.provenance_class)),
                ("currency", Json::str(&c.currency)),
                ("cost", cost_json(c)),
                ("capability", capability_json(c)),
                (
                    "cost_micros",
                    Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))]),
                ),
                ("capability_ppm", Json::Int(c.capability_ppm)),
                (
                    "confidence_min",
                    Json::str(confidence_label(c.confidence_min)),
                ),
                ("coverage_min_ppm", Json::Int(c.coverage_min_ppm)),
                ("spend_rows", Json::Int(c.spend_rows as i64)),
                (
                    "run_ids",
                    Json::Arr(c.run_ids.iter().map(Json::str).collect()),
                ),
            ])
        })
        .collect();

    // `strata` — the confidence/class decompositions (cells are grouped,
    // never pooled; the capability axis stays on the point members).
    let mut by_conf: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    let mut by_pclass: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    for c in &cells {
        by_conf
            .entry(confidence_label(c.confidence_min).to_string())
            .or_default()
            .push(Json::str(c.point_id()));
        by_pclass
            .entry(c.participant_class.clone())
            .or_default()
            .push(Json::str(c.point_id()));
    }

    // `cost_of_pass` — expected micro-units per success per point cell;
    // `n/a{estimator_undefined}` when the cell's capability interval
    // includes 0 (§6.4 A4: the denominator is unidentified there).
    let cost_of_pass: Vec<Json> = point_cells
        .iter()
        .map(|c| {
            let v = match c.cost_of_pass(confidence_ppm) {
                Some((point, interval)) => Json::obj([
                    ("micros_per_success", Json::Int(point)),
                    (
                        "interval",
                        Json::obj([("lo", Json::Int(interval.lo)), ("hi", Json::Int(interval.hi))]),
                    ),
                ]),
                None => Json::obj([("n/a", Json::str("estimator_undefined"))]),
            };
            Json::obj([("point_id", Json::str(c.point_id())), ("value", v)])
        })
        .collect();

    // `provenance_mix` — micro-units per class per arm (the `cost_view`
    // convention; rows in mixed currencies still sum per class — the
    // currency split stays visible on the cells themselves).
    let mut mix: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for c in &cells {
        *mix.entry(c.arm_id.clone())
            .or_default()
            .entry(c.provenance_class.clone())
            .or_default() += c.cost_micros;
    }

    Json::obj([
        ("metric", Json::str(&metric.name)),
        ("match_spec_ref", Json::str(match_spec_ref)),
        ("price_table_version", Json::str(price_table_version)),
        ("seed", Json::Int(seed as i64)),
        ("draws", Json::Int(draws as i64)),
        (
            "origin",
            Json::obj([
                ("cost", Json::Int(0)),
                ("capability", Json::Int(0)),
                ("on_frontier", Json::Bool(true)),
            ]),
        ),
        ("points", Json::Arr(points)),
        ("bands", Json::Arr(bands)),
        ("frontiers", frontiers),
        (
            "combined",
            Json::Arr(combined.iter().map(|p| Json::str(p.point_id())).collect()),
        ),
        (
            "strata",
            Json::obj([
                ("confidence", Json::Obj(by_conf)),
                ("class", Json::Obj(by_pclass)),
            ]),
        ),
        ("cost_of_pass", Json::Arr(cost_of_pass)),
        (
            "provenance_mix",
            Json::Obj(
                mix.into_iter()
                    .map(|(arm, classes)| {
                        (
                            arm,
                            Json::Obj(
                                classes
                                    .into_iter()
                                    .map(|(c, m)| (c, Json::Int(m)))
                                    .collect(),
                            ),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "excluded",
            Json::obj([(
                "no_spend",
                Json::Arr(no_spend.iter().map(Json::str).collect()),
            )]),
        ),
    ])
}
