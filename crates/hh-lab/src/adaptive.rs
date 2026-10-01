//! Adaptive validation allocation — the `voi_weighted` arm of
//! [`crate::experiment::ValidationStrategy`] (§6.3 §2.5; §5e.4 "Lab-side
//! value-of-information"; ADR-0190 D7).
//!
//! `full_set` never touches this module. Under `voi_weighted` the experiment
//! engine picks the next dispatchable plan by value-of-information weight
//! rather than recorded `order_pos`, and records the choice as an
//! [`InclusionProbabilities`] row (`measurement.experiment
//! .inclusion_probabilities`) — AC-F4-13's executable half. The invariants
//! (§6.3 §5e.4 V-1…V-6):
//!
//! - **V-1.** Every dispatched plan is still a member of the declared
//!   `CellPlan` — the allocator only ever picks among `dispatchable`
//!   plans; it never invents cells.
//! - **V-2.** The draw is deterministic: `H(seed ‖ round ‖ strategy)` mod
//!   the weight sum, so the recorded table replays the pick.
//! - **V-3/V-5.** The allocator's own spend is charged to `instrument` by
//!   the caller (the engine appends `measurement.cost.attributed`).
//! - **V-4.** The `OrderPlan` itself is never rewritten — the inclusion
//!   table is the recorded permutation (drift brackets stay intact).
//! - **V-6.** `full_set` arms produce no inclusion rows at all.
//!
//! All arithmetic is integer (ppm / fixed-point) — replay equality cannot
//! drift on float rounding.

use std::collections::BTreeMap;

use hh_identity::idp::identify_bytes;
use hh_identity::kinds::RecordKind;
use hh_wire::json::Json;

/// Fixed-point scale for probabilities and weights (parts per million).
pub const PPM: u64 = 1_000_000;

/// Per-cell observation statistics the allocator reads. `completed` counts
/// settled attempts in the cell; `scored` counts the ones whose outcome was
/// `scored` — the disagreement estimator's `p̄` is `scored/completed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStats {
    /// Settled attempts in the cell.
    pub completed: u32,
    /// Settled attempts whose outcome class was `scored`.
    pub scored: u32,
}

/// The recorded inclusion table for one allocation round — the
/// `measurement.experiment.inclusion_probabilities` payload body. The
/// `record_id` is a content address ([`RecordKind::InclusionProbabilities`])
/// over the canonical body *excluding* `record_id` itself.
#[derive(Debug, Clone, PartialEq)]
pub struct InclusionProbabilities {
    /// The content address of the canonical body.
    pub record_id: String,
    /// The strategy (`voi_weighted`).
    pub strategy: String,
    /// The estimator (`disagreement | expected_information_gain`).
    pub estimator: String,
    /// The allocation round (0-based — the count of prior inclusion rows).
    pub round: u64,
    /// The per-plan inclusion probability in ppm (`run_plan_id → ppm`).
    /// Under-replicated cells are forced to `PPM` (probation).
    pub per_plan: BTreeMap<String, u64>,
    /// The plan the deterministic draw picked.
    pub picked: String,
}

impl InclusionProbabilities {
    /// The canonical payload body (`record_id` included).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("record_id", Json::str(&self.record_id)),
            ("strategy", Json::str(&self.strategy)),
            ("estimator", Json::str(&self.estimator)),
            ("round", Json::Int(self.round as i64)),
            (
                "per_plan",
                Json::Obj(
                    self.per_plan
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                        .collect(),
                ),
            ),
            ("picked", Json::str(&self.picked)),
        ])
    }
}

/// The `voi_weighted` parameter set, lifted off the declared
/// [`crate::experiment::ValidationStrategy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiParams {
    /// The estimator (`disagreement | expected_information_gain`).
    pub estimator: &'static str,
    /// λ — the exploration weight (ppm-scaled by the allocator).
    pub lambda: u64,
    /// The inclusion-probability floor (ppm) and the variance floor `ℓ`.
    pub min_inclusion_fraction_ppm: u64,
    /// Cells with fewer settled attempts are probed before weighting.
    pub min_replicates: u32,
}

/// Integer square root (`floor(√n)` — `u64` domain).
fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The VOI weight of a cell at `(completed, scored)` under `params`
/// (fixed-point, scaled by `PPM²` — relative magnitudes are what matter).
///
/// - `disagreement`: `w = max(p̄·(PPM − p̄), ℓ·PPM) + λ·PPM·PPM/√(n+1)` —
///   realized outcome-variance plus an uncertainty bonus.
/// - `expected_information_gain`: `w = max(ℓ·PPM, PPM·PPM/(n+1)) +
///   λ·PPM·PPM/√(n+1)` — the least-sampled cells dominate (the cell whose
///   outcome is least resolved changes the ranking most).
///
/// Cells under `min_replicates` return `u64::MAX` — forced inclusion until
/// probed.
fn weight(params: &VoiParams, stats: &CellStats) -> u64 {
    if stats.completed < params.min_replicates {
        return u64::MAX;
    }
    let n = stats.completed as u64;
    let p_hat_ppm = (stats.scored as u64) * PPM / n;
    let floor = params.min_inclusion_fraction_ppm.saturating_mul(PPM);
    let var_term = match params.estimator {
        "expected_information_gain" => floor.max(PPM * PPM / (n + 1)),
        // `disagreement` (and the declared-estimator default): realized
        // outcome variance `p̄(1 − p̄)` in ppm².
        _ => (p_hat_ppm * (PPM - p_hat_ppm)).max(floor),
    };
    let bonus = params.lambda.saturating_mul(PPM).saturating_mul(PPM) / isqrt(n + 1).max(1);
    var_term.saturating_add(bonus)
}

/// Pick the next plan among `candidates` (`(run_plan_id, cell_id)` in
/// dispatchable order) under `params`, recording the inclusion table.
/// `stats_of` resolves a cell's settled-outcome statistics; `seed` and
/// `round` make the draw replayable. Returns `None` for an empty candidate
/// set (the caller treats it like an exhausted dispatch list).
pub fn voi_allocate(
    params: &VoiParams,
    candidates: &[(String, String)],
    stats_of: &dyn Fn(&str) -> CellStats,
    seed: &str,
    round: u64,
) -> Option<InclusionProbabilities> {
    if candidates.is_empty() {
        return None;
    }
    let weights: Vec<u64> = candidates
        .iter()
        .map(|(_, cell)| weight(params, &stats_of(cell)))
        .collect();
    let total: u128 = weights.iter().map(|w| *w as u128).sum();
    // The deterministic draw — `H(seed ‖ round ‖ strategy)` mod Σw, then a
    // cumulative walk in dispatchable order.
    let draw_bytes = format!("{seed}|{round}|{}", params.estimator);
    let draw_id = hh_identity::idp::idp_id("hh/voi_draw", draw_bytes.as_bytes());
    let draw = u64::from_str_radix(&draw_id["sha256:".len().."sha256:".len() + 16], 16)
        .unwrap_or(0) as u128;
    let mut target = draw % total;
    let mut picked = candidates.len() - 1;
    for (i, w) in weights.iter().enumerate() {
        if target < *w as u128 {
            picked = i;
            break;
        }
        target -= *w as u128;
    }
    // Inclusion probabilities: `w_i/Σw` in ppm, floored at
    // `min_inclusion_fraction` (V-1 — every dispatchable plan keeps a
    // non-zero recorded probability). A probation cell (under
    // `min_replicates`) records the full remaining mass split among
    // probationers — the recorded table is the deterministic truth, not an
    // approximation of `u64::MAX` overflow.
    let n_forced = weights.iter().filter(|w| **w == u64::MAX).count() as u64;
    let n_other = candidates.len() as u64 - n_forced;
    let mut per_plan = BTreeMap::new();
    for ((pid, _), w) in candidates.iter().zip(weights.iter()) {
        let p = if n_forced > 0 {
            if *w == u64::MAX {
                PPM.saturating_sub(params.min_inclusion_fraction_ppm * n_other) / n_forced
            } else {
                params.min_inclusion_fraction_ppm
            }
        } else {
            ((*w as u128 * PPM as u128) / total) as u64
        };
        per_plan.insert(
            pid.clone(),
            p.max(params.min_inclusion_fraction_ppm).min(PPM),
        );
    }
    let body = Json::obj([
        ("strategy", Json::str("voi_weighted")),
        ("estimator", Json::str(params.estimator)),
        ("round", Json::Int(round as i64)),
        (
            "per_plan",
            Json::Obj(
                per_plan
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        ),
        ("picked", Json::str(&candidates[picked].0)),
    ]);
    let record_id = identify_bytes(
        RecordKind::InclusionProbabilities,
        body.to_canonical_string().as_bytes(),
    );
    Some(InclusionProbabilities {
        record_id,
        strategy: "voi_weighted".to_string(),
        estimator: params.estimator.to_string(),
        round,
        per_plan,
        picked: candidates[picked].0.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> VoiParams {
        VoiParams {
            estimator: "disagreement",
            lambda: PPM / 10,
            min_inclusion_fraction_ppm: 10_000,
            min_replicates: 1,
        }
    }

    fn cands() -> Vec<(String, String)> {
        vec![
            ("plan-a".to_string(), "cell-1".to_string()),
            ("plan-b".to_string(), "cell-2".to_string()),
        ]
    }

    #[test]
    fn under_replicated_cells_are_forced() {
        // A cell with zero settled attempts is probed first (min_replicates).
        let stats = |cell: &str| -> CellStats {
            match cell {
                "cell-1" => CellStats {
                    completed: 0,
                    scored: 0,
                },
                _ => CellStats {
                    completed: 9,
                    scored: 9,
                },
            }
        };
        for round in 0..8 {
            let p = voi_allocate(&params(), &cands(), &stats, "seed-x", round).unwrap();
            assert_eq!(p.picked, "plan-a");
            // The probation cell holds the whole mass less the other plan's
            // declared floor (PPM − 10_000).
            assert_eq!(p.per_plan["plan-a"], PPM - 10_000);
            assert_eq!(p.per_plan["plan-b"], 10_000);
        }
    }

    #[test]
    fn draw_is_deterministic_and_recorded() {
        let stats = |_: &str| CellStats {
            completed: 4,
            scored: 2,
        };
        let a = voi_allocate(&params(), &cands(), &stats, "seed-x", 3).unwrap();
        let b = voi_allocate(&params(), &cands(), &stats, "seed-x", 3).unwrap();
        assert_eq!(a, b);
        // The recorded probabilities floor at min_inclusion_fraction and
        // identify the picked plan.
        assert!(a.per_plan.values().all(|p| *p >= 10_000));
        assert!(a.record_id.starts_with("sha256:"));
        // A different round is a different draw (weight-equal cells split).
        let rounds: BTreeMap<_, _> = (0..6)
            .map(|r| voi_allocate(&params(), &cands(), &stats, "seed-x", r).unwrap().picked)
            .fold(BTreeMap::new(), |mut m, p| {
                *m.entry(p).or_insert(0) += 1;
                m
            });
        assert_eq!(rounds.keys().len(), 2, "equal weights must split draws");
    }

    #[test]
    fn disagreement_weights_split_cells_over_decided_ones() {
        // cell-1 is maximally split (p̄ = 0.5); cell-2 is decided (p̄ = 1).
        // With λ = 0 the disagreement weight dominates — the split cell wins
        // every round that probes weights rather than replicates.
        let mut p = params();
        p.lambda = 0;
        let stats = |cell: &str| -> CellStats {
            match cell {
                "cell-1" => CellStats {
                    completed: 8,
                    scored: 4,
                },
                _ => CellStats {
                    completed: 8,
                    scored: 8,
                },
            }
        };
        let a = voi_allocate(&p, &cands(), &stats, "seed-x", 0).unwrap();
        assert!(a.per_plan["plan-a"] > a.per_plan["plan-b"]);
    }

    #[test]
    fn eig_prefers_the_less_sampled_cell() {
        let mut p = params();
        p.estimator = "expected_information_gain";
        p.lambda = 0;
        let stats = |cell: &str| -> CellStats {
            match cell {
                "cell-1" => CellStats {
                    completed: 1,
                    scored: 1,
                },
                _ => CellStats {
                    completed: 50,
                    scored: 25,
                },
            }
        };
        let a = voi_allocate(&p, &cands(), &stats, "seed-x", 0).unwrap();
        assert!(a.per_plan["plan-a"] > a.per_plan["plan-b"]);
    }

    #[test]
    fn empty_candidates_allocate_none() {
        assert!(voi_allocate(&params(), &[], &|_| CellStats::default(), "s", 0).is_none());
    }
}
