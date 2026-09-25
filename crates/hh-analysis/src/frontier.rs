//! `frontier` — the M2 capability–cost Pareto frontier with provenance
//! bands (spec §8.2; AC-R-2.1.6-8: "a comparison containing reported and
//! measured spend rows renders separate frontiers plus a combined band; a
//! coverage<1 row is never summed as complete"; ADR-0043 D4 —
//! `confidence ∈ {estimate, unknown}` or `coverage < 1` renders a band,
//! never a point).
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

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::pricing::Confidence;
use hh_budget::quantity::PPM_SCALE;
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::EvalRun;
use hh_eval::scorecard::run_numeric;
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
                if let Some(v) = cap {
                    acc.cap_sum += v as i128;
                    acc.cap_n += 1;
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

/// The `frontier` body member (§8.2 M2 output):
/// `{metric, points[], bands[], frontiers{class → [point_id]}, combined[],
/// provenance_mix{arm → class → micros}, excluded{no_spend}}`.
pub fn frontier_report(
    metric: &MetricDeclaration,
    runs: &[&EvalRun],
    facts: &BTreeMap<String, LedgerFacts>,
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

    let point_json = |c: &FrontierCandidate| {
        Json::obj([
            ("point_id", Json::str(c.point_id())),
            ("arm_id", Json::str(&c.arm_id)),
            ("provenance_class", Json::str(&c.provenance_class)),
            ("currency", Json::str(&c.currency)),
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
        ])
    };

    let points: Vec<Json> = cells
        .iter()
        .filter(|c| c.is_point_admissible())
        .map(point_json)
        .collect();
    let bands: Vec<Json> = cells
        .iter()
        .filter(|c| !c.is_point_admissible())
        .map(|c| {
            let (lo, hi) = c.cost_band();
            Json::obj([
                ("point_id", Json::str(c.point_id())),
                ("arm_id", Json::str(&c.arm_id)),
                ("provenance_class", Json::str(&c.provenance_class)),
                ("currency", Json::str(&c.currency)),
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
        ("points", Json::Arr(points)),
        ("bands", Json::Arr(bands)),
        ("frontiers", frontiers),
        (
            "combined",
            Json::Arr(combined.iter().map(|p| Json::str(p.point_id())).collect()),
        ),
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
