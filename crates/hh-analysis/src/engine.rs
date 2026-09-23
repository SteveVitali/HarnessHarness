//! `engine` — `analyze_and_record`: the kernel + the `AnalysisRecord` /
//! report-body deposit in LabDocs (§6.4: reports are recorded through
//! `record_analysis`, never silently recomputed — AC-R-2.10.4-12).
//!
//! The persist path is *idempotent and non-recomputing*: `report_id` is
//! pure over `(spec_hash, generated_from)` — both known before any
//! estimator runs — so a hit on the named `analysis_report/<report_id>`
//! doc returns the stored body verbatim; the estimators never run twice
//! for the same `(spec, rows @ watermark, seed, draws)`.

use hh_experiment::docs::{kind as doc_kind, LabDocs};
use hh_lab::analysis::{AnalysisRecord, AnalysisReport, AnalysisSpec, AnalysisStatus};
use hh_wire::Json;

use crate::error::AnalysisError;
use crate::kernel::{analyze, AnalysisInput};
use crate::report::{generated_from_for, lab_watermarks, report_id_for, AnalysisOutcome};

/// `analyze_and_record(docs, spec, input)` — run the kernel and persist
/// `{AnalysisRecord, analysis_report_body/1}` idempotently. A recorded
/// report is served, never recomputed.
pub fn analyze_and_record(
    docs: &LabDocs,
    spec: &AnalysisSpec,
    input: &AnalysisInput<'_>,
) -> Result<AnalysisOutcome, AnalysisError> {
    // The id is computable before the estimators run — the
    // never-recompute check is cheap and total.
    let gf = generated_from_for(input);
    let report_id = report_id_for(spec, &gf);
    if let Some(body) = docs.get_named(doc_kind::ANALYSIS_REPORT, &report_id)? {
        // Served, not recomputed — the envelopes rebuild from the
        // (spec, input) coordinates alone. The status re-derives from the
        // stored body's label (an exploratory report stays exploratory —
        // re-serving never upgrades it).
        let status = match body
            .get("labels")
            .and_then(|l| l.get("report"))
            .and_then(Json::as_str)
        {
            Some("confirmatory") => AnalysisStatus::Final,
            _ => AnalysisStatus::Exploratory,
        };
        let watermarks = lab_watermarks(input.watermark_set);
        let mut record = AnalysisRecord {
            analysis_id: String::new(),
            spec_ref: spec.spec_id(),
            generated_from: watermarks.clone(),
            outputs: vec![report_id.clone()],
            status,
        };
        record.analysis_id = record.analysis_id();
        let report = AnalysisReport {
            report_id: report_id.clone(),
            spec_hash: spec.spec_id(),
            generated_from: watermarks,
            status,
            result_ref: Some(report_id.clone()),
        };
        return Ok(AnalysisOutcome {
            body,
            report,
            record,
            report_id,
            excluded_not_run: Vec::new(),
        });
    }

    let mut outcome = analyze(spec, input)?;
    docs.put_named(doc_kind::ANALYSIS_REPORT, &report_id, &outcome.body)?;
    outcome.report.result_ref = Some(report_id.clone());
    docs.put(doc_kind::ANALYSIS, &outcome.record.to_json())?;
    Ok(outcome)
}
