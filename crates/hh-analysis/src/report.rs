//! `report` — the `analysis_report_body/1` assembly + the `AnalysisReport`
//! / `AnalysisRecord` envelopes (§6.4 §3; ADR-0157 D1).
//!
//! `report_id = H(spec_hash ∥ generated_from)` under the
//! `analysis.report` domain (KA-12: identical `(spec, row watermark,
//! run-id set, metric_registry_version, price_table_version, seed,
//! draws)` ⇒ identical `report_id`). The body is a canonical-JSON
//! document; the envelopes are `hh_lab::analysis` records (CC7 — one
//! schema home).

use std::collections::BTreeMap;

use hh_lab::analysis::{
    AnalysisRecord, AnalysisReport, AnalysisSpec, AnalysisStatus, ComparisonReport,
    ConfigurationSummary, WatermarkSet as LabWatermarkSet,
};
use hh_wire::Json;

use crate::kernel::AnalysisInput;

/// The kernel's deterministic output: the report body + the two envelope
/// records (the engine persists them; `analyze` is pure).
#[derive(Debug, Clone)]
pub struct AnalysisOutcome {
    /// The `analysis_report_body/1` canonical JSON.
    pub body: Json,
    /// The `AnalysisReport` envelope (`result_ref` is filled by the
    /// engine once the body's content address is known).
    pub report: AnalysisReport,
    /// The `AnalysisRecord` (`analysis_id` content-addressed).
    pub record: AnalysisRecord,
    /// The report id (`H(spec_hash ∥ generated_from)`).
    pub report_id: String,
    /// Runs excluded for being non-terminal (`not_run`).
    pub excluded_not_run: Vec<String>,
}

/// `results` `{run_id → seq}` → the lab `watermark_set` (`{store →
/// watermark}` — the run id is the store coordinate at C0).
pub(crate) fn lab_watermarks(w: &hh_results::watermark::WatermarkSet) -> LabWatermarkSet {
    LabWatermarkSet {
        watermarks: w
            .runs
            .iter()
            .map(|(r, s)| (r.clone(), s.to_string()))
            .collect(),
    }
}

/// The `generated_from` member — the input-set identity.
fn generated_from(input: &AnalysisInput<'_>, run_ids: &[String]) -> Json {
    let run_ids_hash = hh_identity::idp_id(
        "analysis.run_ids",
        Json::Arr(run_ids.iter().map(Json::str).collect())
            .to_canonical_string()
            .as_bytes(),
    );
    Json::obj([
        ("row_watermark", input.watermark_set.to_json()),
        ("run_ids_hash", Json::str(run_ids_hash)),
        (
            "run_ids",
            Json::Arr(run_ids.iter().map(Json::str).collect()),
        ),
        (
            "metric_registry_version",
            Json::str(input.metric_registry_version),
        ),
        ("price_table_version", Json::str(input.price_table_version)),
        ("seed", Json::Int(input.seed as i64)),
        ("draws", Json::Int(input.resampling_draws as i64)),
    ])
}

/// `generated_from` for the input's row set (run_ids sorted — the
/// ordering the record hashes).
pub fn generated_from_for(input: &AnalysisInput<'_>) -> Json {
    let mut run_ids: Vec<String> = input.rows.iter().map(|r| r.key.run_id.clone()).collect();
    run_ids.sort();
    generated_from(input, &run_ids)
}

/// `report_id = H(spec_hash ∥ generated_from)` — pure over the input
/// coordinates, computable *before* the estimators run (the engine's
/// never-recompute check reads exactly this).
pub fn report_id_for(spec: &AnalysisSpec, generated_from: &Json) -> String {
    let spec_hash = spec.spec_id();
    hh_identity::idp_id(
        "analysis.report",
        format!("{}|{}", spec_hash, generated_from.to_canonical_string()).as_bytes(),
    )
}

/// The report's label (`confirmatory` needs a pre-registration whose
/// `primary_metrics` cover the spec's metrics — ADR-0157 D5; `preview` is
/// a reduced-draws run, never persisted as confirmatory).
pub fn report_label(spec: &AnalysisSpec, input: &AnalysisInput<'_>) -> &'static str {
    const CONFIRMATORY_DRAWS: u64 = 2000;
    if input.resampling_draws < CONFIRMATORY_DRAWS {
        return "preview";
    }
    let requested = spec.label.as_deref().unwrap_or("exploratory");
    if requested == "confirmatory" {
        let covered = input
            .pre_registration
            .map(|p| {
                spec.query
                    .metrics
                    .iter()
                    .all(|m| p.primary_metrics.contains(m))
            })
            .unwrap_or(false);
        if covered {
            return "confirmatory";
        }
    }
    "exploratory"
}

/// Assemble the outcome — body + envelopes — from the produced rows.
#[allow(clippy::too_many_arguments)] // the body's section list is the §6.4 shape — the arity is the report's.
pub fn assemble(
    spec: &AnalysisSpec,
    input: &AnalysisInput<'_>,
    summaries: Vec<ConfigurationSummary>,
    summary_details: Vec<Json>,
    comparisons: Vec<ComparisonReport>,
    contrasts: Vec<Json>,
    equivalence: Option<Json>,
    transfer_profile: Option<Json>,
    excluded_not_run: Vec<String>,
) -> AnalysisOutcome {
    let mut run_ids: Vec<String> = input.rows.iter().map(|r| r.key.run_id.clone()).collect();
    run_ids.sort();
    let gf = generated_from(input, &run_ids);
    let report_id = report_id_for(spec, &gf);
    let label = report_label(spec, input);
    let status = if label == "confirmatory" {
        AnalysisStatus::Final
    } else {
        AnalysisStatus::Exploratory
    };

    let mut body = BTreeMap::new();
    body.insert("schema".into(), Json::str("analysis_report_body/1"));
    body.insert("report_id".into(), Json::str(&report_id));
    body.insert("kind".into(), Json::str(&spec.kind));
    if let Some(r) = &spec.spec_ref {
        body.insert("spec_ref".into(), Json::str(r));
    }
    body.insert("generated_from".into(), gf.clone());
    body.insert(
        "summaries".into(),
        Json::Arr(summaries.iter().map(|s| s.to_json()).collect()),
    );
    body.insert("summary_details".into(), Json::Arr(summary_details));
    body.insert(
        "comparisons".into(),
        Json::Arr(comparisons.iter().map(|c| c.to_json()).collect()),
    );
    body.insert("contrasts".into(), Json::Arr(contrasts));
    if let Some(e) = equivalence {
        body.insert("equivalence".into(), e);
    }
    if let Some(t) = transfer_profile {
        body.insert("transfer".into(), t);
    }
    body.insert(
        "excluded".into(),
        Json::obj([(
            "not_run",
            Json::Arr(excluded_not_run.iter().map(Json::str).collect()),
        )]),
    );
    body.insert("labels".into(), Json::obj([("report", Json::str(label))]));
    body.insert(
        "provenance".into(),
        Json::obj([
            ("origin", Json::str("instrument")),
            ("authority", Json::str("kernel")),
        ]),
    );
    let body = Json::Obj(body);

    let report = AnalysisReport {
        report_id: report_id.clone(),
        spec_hash: spec.spec_id(),
        generated_from: lab_watermarks(input.watermark_set),
        status,
        result_ref: None, // the engine fills the body's content address
    };
    let mut record = AnalysisRecord {
        analysis_id: String::new(),
        spec_ref: spec.spec_id(),
        generated_from: lab_watermarks(input.watermark_set),
        outputs: vec![report_id.clone()],
        status,
    };
    record.analysis_id = record.analysis_id();

    AnalysisOutcome {
        body,
        report,
        record,
        report_id,
        excluded_not_run,
    }
}
