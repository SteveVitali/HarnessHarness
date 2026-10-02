//! `DisclosureSummary` — the exact disclosure counts derivable per
//! experiment (§6.5 §2.3; ADR-0162 D5):
//!
//! ```text
//! DisclosureSummary{arms_registered, arms_published, arms_restricted,
//!                   exclusions, analyses_pre_registered,
//!                   analyses_exploratory}
//! ```
//!
//! The summary is a *derived view* — it counts the experiment run's
//! committed ledger facts (`view.plans`/`exclusions`/`analyses`) plus the
//! bundle catalogue's reader restrictions, never a sampled or inferred
//! number (CC3). A restricted/private arm still counts as *registered* —
//! disclosure is the point.
//!
//! A search arm additionally carries `selection{n_candidates_registered,
//! rule, search_budget_ref}` — L4's best-of-N disclosure (§6.5 §2.4; the
//! leaderboard form of search-vs-artifact separation): an arm with a
//! declared `search_budget` never ranks as a plain artifact.

use hh_experiment::view::ExperimentView;
use hh_lab::experiment::ExperimentSpec;
use hh_wire::json::Json;

use crate::catalogue::BundleCatalogue;

/// The per-experiment disclosure record (§6.5 §2.3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisclosureSummary {
    /// Arms declared on the experiment spec (every arm counts — restricted
    /// or unpublished arms are still registered).
    pub arms_registered: u64,
    /// Arms whose covering bundles admit publication (a bundle whose
    /// `readers` set is unrestricted — empty or naming `public`/`*`).
    pub arms_published: u64,
    /// Arms whose covering bundles are all restricted (`readers` bounded
    /// and not containing `public`/`*`).
    pub arms_restricted: u64,
    /// `run_excluded` rows committed on the experiment run.
    pub exclusions: u64,
    /// `measurement.analysis.recorded` rows with `pre_registered = true`.
    pub analyses_pre_registered: u64,
    /// `measurement.analysis.recorded` rows with `pre_registered = false`.
    pub analyses_exploratory: u64,
    /// Per search-arm selection disclosure — `{arm_id, selection{
    /// n_candidates_registered, n_candidates, rule, search_budget_ref}}`
    /// (L4; AC-R-2.10.5-8's best-of-N arm).
    pub selections: Vec<Json>,
}

impl DisclosureSummary {
    /// Canonical JSON — the leaderboard snapshot embeds this verbatim.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("arms_registered", Json::Int(self.arms_registered as i64)),
            ("arms_published", Json::Int(self.arms_published as i64)),
            ("arms_restricted", Json::Int(self.arms_restricted as i64)),
            ("exclusions", Json::Int(self.exclusions as i64)),
            (
                "analyses_pre_registered",
                Json::Int(self.analyses_pre_registered as i64),
            ),
            (
                "analyses_exploratory",
                Json::Int(self.analyses_exploratory as i64),
            ),
            ("selections", Json::Arr(self.selections.clone())),
        ])
    }
}

/// `restricted` — the covering bundle is restricted-store evidence: either
/// its recorded `readers` are bounded (non-empty, naming neither `public`
/// nor `*`) or its status book entry is `restricted` itself (the §5h.3
/// audience-re-scoping terminal-side status; an unbounded/public reader
/// set or an unrestricted status is publishable).
fn restricted(e: &crate::catalogue::BundleCatalogueEntry) -> bool {
    e.status == crate::catalogue::BundleStatus::Restricted
        || (!e.readers.is_empty() && !e.readers.iter().any(|r| r == "public" || r == "*"))
}

/// `disclosure_summary(view, spec, catalogue)` — derive the counts:
///
/// - `arms_registered` = `spec.arms.len()`;
/// - `exclusions` = `view.exclusions.len()`;
/// - `analyses_*` = the `measurement.analysis.recorded` payloads' `pre_registered`
///   flag split;
/// - arm → restricted/published: an arm's *bound runs* (`view.plans`
///   `arm_id` → attempt run ids) covered by catalogue entries; an arm with
///   at least one non-restricted covering bundle is `published`, an arm
///   every covering bundle of which is restricted is `restricted` (an arm
///   with no bundle at all is registered but neither published nor
///   restricted — it counts in `arms_registered` only, CC3-honest).
/// - `selections[]` — every arm carrying `search_budget` emits the L4
///   disclosure row (`n_candidates_registered` = the arm's distinct
///   planned cells; `n_candidates` the AC-spelling alias of the same
///   number; `rule` labels the declared selection rule).
pub fn disclosure_summary(
    view: &ExperimentView,
    spec: &ExperimentSpec,
    catalogue: &BundleCatalogue,
) -> DisclosureSummary {
    let mut s = DisclosureSummary {
        arms_registered: spec.arms.len() as u64,
        exclusions: view.exclusions.len() as u64,
        ..Default::default()
    };
    for a in &view.analyses {
        if a.get("pre_registered") == Some(&Json::Bool(true)) {
            s.analyses_pre_registered += 1;
        } else {
            s.analyses_exploratory += 1;
        }
    }
    // arm_id → the run ids bound under it (accepted or not — disclosure
    // counts the registered truth, never the outcome).
    let mut arm_runs: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for p in view.plans.values() {
        for a in &p.attempts {
            arm_runs
                .entry(p.arm_id.clone())
                .or_default()
                .push(a.run_id.clone());
        }
    }
    for arm in &spec.arms {
        let runs = arm_runs.get(&arm.arm_id).cloned().unwrap_or_default();
        let covering: Vec<&crate::catalogue::BundleCatalogueEntry> = catalogue
            .entries
            .iter()
            .filter(|e| e.subject_runs.iter().any(|r| runs.contains(r)))
            .collect();
        if covering.is_empty() {
            // Registered but uncovered — counts in `arms_registered` only.
        } else if covering.iter().all(|e| restricted(e)) {
            s.arms_restricted += 1;
        } else {
            s.arms_published += 1;
        }
        if let Some(search_ref) = &arm.search_budget {
            // `n_candidates_registered` — the arm's distinct declared
            // cells (the candidates the experiment registered for search;
            // the count is the plan's truth, never inferred).
            let candidates: std::collections::BTreeSet<&String> = view
                .plans
                .values()
                .filter(|p| p.arm_id == arm.arm_id)
                .map(|p| &p.cell_id)
                .collect();
            s.selections.push(Json::obj([
                ("arm_id", Json::str(&arm.arm_id)),
                (
                    "selection",
                    Json::obj([
                        (
                            "n_candidates_registered",
                            Json::Int(candidates.len() as i64),
                        ),
                        ("n_candidates", Json::Int(candidates.len() as i64)),
                        ("rule", Json::str("best_of_n")),
                        ("search_budget_ref", Json::str(search_ref)),
                    ]),
                ),
            ]));
        }
    }
    s
}
