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

/// The §6.5 producer-contract `kind` vocabulary member a spec `kind`
/// produces (`custom` for anything outside the closed list — an
/// unlisted spec kind is recorded, never silently renamed).
pub(crate) fn record_kind(spec_kind: &str) -> &'static str {
    match spec_kind {
        "summarize" => "summary_report",
        "compare" => "comparison_report",
        "interaction" => "interaction_report",
        "frontier" => "frontier_report",
        "transfer" => "transfer_report",
        "benefit_decomposition" => "benefit_decomposition",
        "rank" => "rank_report",
        "reliability_profile" => "reliability_profile",
        "power" => "power_report",
        "strata_view" => "strata_view",
        "diagnostics" => "diagnostics",
        "fit_surface" => "fitted_surface",
        "attribution" => "attribution_report",
        "equivalence" => "equivalence_report",
        _ => "custom",
    }
}

/// The single bound `experiment_run_id` across the selected rows — `None`
/// for an unbound or mixed-experiment selection (never fabricated; CC3).
fn bound_experiment(input: &AnalysisInput<'_>) -> Option<String> {
    let mut bound: Option<&str> = None;
    for r in input.rows {
        let e = r
            .experiment
            .as_ref()
            .and_then(|x| x.get("experiment_run_id"))
            .and_then(Json::as_str);
        match (bound, e) {
            (None, Some(e)) => bound = Some(e),
            (Some(b), Some(e)) if b == e => {}
            (Some(_), Some(_)) | (_, None) => return None,
        }
    }
    bound.map(str::to_string)
}

/// Build the C1 `AnalysisRecord` envelope from `(spec, input, report_id,
/// status)` — shared by `assemble` (a fresh run) and the engine's serve
/// path (a recorded report is never recomputed and its envelope must be
/// *identical*: the record is content-addressed, so a re-served record
/// recomputes to the same `analysis_id`).
pub fn record_for(
    spec: &AnalysisSpec,
    input: &AnalysisInput<'_>,
    report_id: &str,
    status: AnalysisStatus,
) -> AnalysisRecord {
    // `oracle_ids` — the distinct oracle refs the analysis's cells cite
    // (sorted; `judge_snapshots` stays empty at this tier — judged oracles
    // are C2 and no row carries a judge snapshot yet).
    let mut oracle_ids: Vec<String> = input
        .rows
        .iter()
        .flat_map(|r| r.cells.iter().filter_map(|c| c.oracle_ref.clone()))
        .collect();
    oracle_ids.sort();
    oracle_ids.dedup();
    // `pre_registered` — the spec matches the experiment's pinned
    // analysis plan *by identity* (§6.5: {kind, metric refs, procedure}
    // match a registered plan — the plan ref pins the spec's content
    // address, so identity on the ref is the whole test).
    let (pre_registered, registered_analysis_ref) = match input.pre_registration {
        Some(p) if !p.analysis_plan_ref.is_empty() && p.analysis_plan_ref == spec.spec_id() => {
            (true, Some(p.analysis_plan_ref.clone()))
        }
        _ => (false, None),
    };
    // `procedure` — the estimator selection's content address (what ran,
    // not a name: declared method + floors + fallback chain are hashed).
    let procedure = hh_identity::idp::idp_id(
        "analysis.procedure",
        spec.estimator_selection
            .to_json()
            .to_canonical_string()
            .as_bytes(),
    );
    let watermarks = lab_watermarks(input.watermark_set);
    let mut record = AnalysisRecord {
        analysis_id: String::new(),
        spec_ref: spec.spec_id(),
        generated_from: watermarks.clone(),
        outputs: vec![report_id.to_string()],
        status,
        kind: Some(record_kind(&spec.kind).to_string()),
        experiment_run_id: bound_experiment(input),
        procedure: Some(procedure),
        inputs: Some(Json::obj([
            ("query", spec.query.to_json()),
            ("watermark_set", watermarks.to_json()),
        ])),
        metric_registry_version: Some(input.metric_registry_version.to_string()),
        price_table_version: Some(input.price_table_version.to_string()),
        oracle_ids,
        judge_snapshots: Vec::new(),
        report: Some(report_id.to_string()),
        pre_registered,
        registered_analysis_ref,
        post_amendment: input.post_amendment,
        provenance: Some(Json::obj([
            ("origin", Json::str("instrument")),
            ("authority", Json::str("kernel")),
        ])),
        cost: Some(Json::obj([
            ("resampling_draws", Json::Int(input.resampling_draws as i64)),
            ("seed", Json::Int(input.seed as i64)),
            ("n_rows", Json::Int(input.rows.len() as i64)),
            ("n_tasks", Json::Int(input.tasks.len() as i64)),
        ])),
    };
    record.analysis_id = record.analysis_id();
    record
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
    frontier: Option<Json>,
    sections: Vec<(&'static str, Json)>,
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
    if let Some(f) = frontier {
        body.insert("frontier".into(), f);
    }
    // Operation-specific members (A6 `benefit_decomposition`, A7
    // `surface`, A9 `rank`, A10 `attribution`, A11 `reliability`, A13
    // `power`, A14 `strata`, A15 `diagnostics`) — canonical-JSON key
    // order makes insertion order irrelevant.
    for (name, section) in sections {
        body.insert(name.into(), section);
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
        kind: Some(record_kind(&spec.kind).to_string()),
    };
    let record = record_for(spec, input, &report_id, status);

    AnalysisOutcome {
        body,
        report,
        record,
        report_id,
        excluded_not_run,
    }
}
