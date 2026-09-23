//! `multiplicity` — A12 (ADR-0157 D5; §6.4 §2.2).
//!
//! Holm across the pre-registered primary family
//! (`PreRegistration.primary_metrics` ∩ the report's comparisons);
//! Benjamini–Hochberg q-values across everything else; `exploratory`
//! labels where no pre-registration matches. **Nothing is suppressed** —
//! the correction is a labelled report fact (`multiplicity{family, raw,
//! adjusted_ppm, label}` on every `ComparisonReport`), never a filter.
//!
//! The raw p-value is the two-sided sign-flip permutation p over the
//! paired per-task deltas (`hh_eval::stats::permutation_signflip_p` —
//! exact ≤ 20 tasks, deterministic seeded Monte-Carlo above).

use std::collections::BTreeSet;

use hh_eval::stats;
use hh_lab::analysis::ComparisonReport;
use hh_ontology::eval::PreRegistration;

/// `apply_multiplicity(comparisons, delta_sets, prereg, draws, seed)` —
/// mutate every report's `multiplicity` member in place. `delta_sets[i]`
/// is the concrete per-task delta list for `comparisons[i]` (parallel).
pub fn apply_multiplicity(
    comparisons: &mut [ComparisonReport],
    delta_sets: &[Vec<i64>],
    prereg: Option<&PreRegistration>,
    draws: u64,
    seed: u64,
) {
    // Raw sign-flip p per comparison (ppm).
    let raw: Vec<Option<i64>> = comparisons
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let deltas = delta_sets.get(i).cloned().unwrap_or_default();
            let material = format!("analysis.mult|{seed}|{}|{}|{}", r.arm_a, r.arm_b, r.metric);
            stats::permutation_signflip_p(&deltas, draws, &material)
        })
        .collect();

    // The primary family: comparisons on `primary_metrics`. With no
    // pre-registration every comparison is exploratory.
    let primaries: BTreeSet<usize> = match prereg {
        Some(p) => comparisons
            .iter()
            .enumerate()
            .filter(|(_, r)| p.primary_metrics.contains(&r.metric))
            .map(|(i, _)| i)
            .collect(),
        None => BTreeSet::new(),
    };
    // Holm over primaries; BH over the exploratory remainder. Family
    // sizes are the *family's* size, never the report's total.
    let p_idx: Vec<usize> = primaries.iter().copied().collect();
    let e_idx: Vec<usize> = (0..comparisons.len())
        .filter(|i| !primaries.contains(i))
        .collect();
    let p_raw: Vec<i64> = p_idx
        .iter()
        .map(|&i| raw[i].unwrap_or(stats::PPM))
        .collect();
    let e_raw: Vec<i64> = e_idx
        .iter()
        .map(|&i| raw[i].unwrap_or(stats::PPM))
        .collect();
    let p_adj = stats::holm(&p_raw);
    let e_adj = stats::benjamini_hochberg(&e_raw);

    for (i, r) in comparisons.iter_mut().enumerate() {
        let (adj, method, family, label) = if let Some(pos) = p_idx.iter().position(|&x| x == i) {
            (p_adj[pos], "holm", p_idx.len() as u32, "confirmatory")
        } else {
            let pos = e_idx.iter().position(|&x| x == i).unwrap_or(0);
            (
                e_adj[pos],
                "benjamini_hochberg",
                e_idx.len() as u32,
                "exploratory",
            )
        };
        r.multiplicity.family_size = family;
        r.multiplicity.adjusted = method.to_string();
        r.multiplicity.raw_ppm = raw[i];
        r.multiplicity.adjusted_ppm = Some(adj);
        r.multiplicity.label = Some(label.to_string());
    }
}
