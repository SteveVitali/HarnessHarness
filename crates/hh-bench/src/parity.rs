//! The parity report (spec §5h.4 §7; AC-R-2.9.4-11; ADR-0144).
//!
//! Parity is an `artifact_benefit` comparison between the adapter's replayed
//! verdicts and the original runner's recorded verdicts — same tasks, same
//! participant, same model snapshot, same budget. The replayed runs' verdict
//! ids are listed on `ParityReport.replay_verdicts`; the underlying
//! `ComparisonReport` carries `benefit_kind = artifact_benefit` (checked by
//! `SuiteManifest::validate`).
//!
//! `parity(original_runs, replay)` never fabricates agreement: a task the
//! replay could not run contributes `unmatched`, never a pass.

use hh_lab::analysis::{ComparisonReport, ParityReport};

/// `parity(original_runs, replay_verdicts, comparison)` — wrap an
/// `artifact_benefit` `ComparisonReport` into the import-parity record.
/// The caller supplies the comparison (built by `hh_eval::benefits::
/// artifact_benefit`); this asserts the kind before wrapping.
pub fn parity_report(
    original_runner_ref: &str,
    original_runs: Vec<String>,
    replay_verdicts: Vec<String>,
    comparison: ComparisonReport,
) -> Result<ParityReport, &'static str> {
    if comparison.benefit_kind != hh_lab::analysis::BenefitKind::ArtifactBenefit {
        return Err("parity requires benefit_kind = artifact_benefit");
    }
    Ok(ParityReport {
        comparison,
        original_runner_ref: original_runner_ref.into(),
        original_runs,
        replay_verdicts,
    })
}
