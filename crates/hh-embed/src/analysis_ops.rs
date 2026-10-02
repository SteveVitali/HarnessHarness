//! Group L — `lab.analysis.analyze` dispatch (S3.4c; R-2.10.4⁰ᵇ). The
//! boundary is records-in/records-out like `lab.eval.*` and
//! `lab.experiment.*`: the caller supplies the `AnalysisSpec`, the row
//! selectors, the task/suite contexts, and (through `spec_ref`) the
//! experiment spec whose design/arms/pre-registration the estimator kernel
//! binds. Rows resolve against the `ResultsStore` at
//! `<store_root>/results` (the S3.4b derived-state store); each row's
//! `RunManifest` and `LedgerFacts` project from the run's durable ledger
//! prefix (`Store::manifest` / `Store::events` — the same sources
//! `project_row` reads, never a second schema). Arm budget refs resolve
//! against the named `budget` docs `lab.experiment.register` deposits.
//!
//! The op persists the `AnalysisRecord` + `analysis_report_body` through
//! `LabDocs` (`kind::ANALYSIS`/`kind::ANALYSIS_REPORT`) and returns the
//! record + report — `report_id` is deterministic for an identical
//! `(spec, watermark set, run set, registry/pricing, seed, draws)` tuple
//! (KA-12).

use std::collections::BTreeMap;

use hh_analysis::{analyze_and_record, AnalysisInput};
use hh_budget::matchspec::{ArmSpec as MatchArmSpec, BudgetEnforcement};
use hh_budget::spec::BudgetSpec;
use hh_eval::facts::LedgerFacts;
use hh_experiment::docs::{kind as doc_kind, LabDocs};
use hh_lab::analysis::AnalysisSpec;
use hh_lab::experiment::ExperimentSpec;
use hh_ledger::manifest::RunManifest;
use hh_results::row::ResultsRow;
use hh_results::store::ResultsStore;
use hh_results::watermark::WatermarkSet;
use hh_wire::json::Json;

use crate::eval_ops::{bad, decls_for, decode_suites, decode_tasks, req};
use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

/// `AnalysisError` → the boundary's typed surface — refusals render
/// verbatim, spec-member problems render `SchemaViolation`.
fn xerr(e: hh_analysis::AnalysisError) -> EmbedError {
    use hh_analysis::AnalysisError as A;
    match e {
        A::BadSpec { member, detail } => EmbedError::SchemaViolation {
            path: format!("/{member}"),
            code: detail,
        },
        other => EmbedError::Refused {
            reason: other.to_string(),
        },
    }
}

fn refused(e: impl std::fmt::Display) -> EmbedError {
    EmbedError::Refused {
        reason: format!("{e}"),
    }
}

/// One named budget doc → the pinned `BudgetSpec` (the register-time
/// `budgets{}` deposit is the resolver; an absent ref is a refusal, never a
/// default — CC3).
fn budget(docs: &LabDocs, r: &str) -> Result<BudgetSpec, EmbedError> {
    let body = docs
        .get_named(doc_kind::BUDGET, r)
        .map_err(refused)?
        .ok_or_else(|| bad("/spec_ref/arms/budget", &format!("unresolvable ref {r}")))?;
    BudgetSpec::from_json(&body)
        .ok_or_else(|| bad("/spec_ref/arms/budget", &format!("decode failed for {r}")))
}

/// `spec.arms[]` (the lab record) → `(arm_id, matchspec ArmSpec)` pairs —
/// the match-check inputs the kernel's matched-budget binding consumes.
/// `limits_enforced = full` maps to `native()`; `partial`/`none` map to
/// `hosted(&[])` — the ADR-0046 (d) mapping; research-grade match specs
/// stay refused inside `hh-eval` (CC9).
fn resolve_arms(
    docs: &LabDocs,
    espec: &ExperimentSpec,
) -> Result<Vec<(String, MatchArmSpec)>, EmbedError> {
    espec
        .arms
        .iter()
        .map(|a| {
            Ok((
                a.arm_id.clone(),
                MatchArmSpec {
                    search_budget: a
                        .search_budget
                        .as_deref()
                        .map(|r| budget(docs, r))
                        .transpose()?,
                    eval_budget: Some(budget(docs, &a.eval_budget)?),
                    inference_budget: None,
                    match_spec: a.match_spec.clone(),
                    enforcement: if a.limits_enforced == "full" {
                        BudgetEnforcement::native()
                    } else {
                        BudgetEnforcement::hosted(&[])
                    },
                    spend_confidence: None,
                    coverage_ppm: None,
                },
            ))
        })
        .collect()
}

impl EmbedService {
    /// `lab.analysis.analyze{spec, rows[], tasks[], suites?[],
    /// metric_registry_version?, price_table_version?, seed?,
    /// resampling_draws?, confidence_ppm?}` → `{record, report}`.
    ///
    /// `rows[]` are `ResultsStore` selectors (row keys or version ids —
    /// `get_row` resolves both); the analysis reads the durable
    /// projections, never re-derives them. The spec's `spec_ref` names the
    /// registered `hh-experiment/1` doc the design/arms/pre-registration
    /// resolve through.
    pub(crate) fn lab_analysis_analyze(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let spec = AnalysisSpec::from_json(req(params, "spec")?)
            .map_err(|e| bad("/spec", &format!("{e:?}")))?;
        // `decode_*` take the params object itself — they resolve their own
        // member (the same convention `lab.eval.*` uses).
        let tasks = decode_tasks(params)?;
        let suites = if params.get("suites").is_some() {
            decode_suites(params)?
        } else {
            Vec::new()
        };
        let docs = self.lab_docs()?;

        // The experiment spec resolves design + arms + pre-registration —
        // never re-supplied at the boundary (the registered doc is the
        // authority; a `spec_ref` pointing nowhere is a refusal).
        let (design, arm_specs, pre_registration) = match &spec.spec_ref {
            Some(r) => {
                let body = docs
                    .get(doc_kind::SPEC, r)
                    .map_err(refused)?
                    .ok_or_else(|| bad("/spec/spec_ref", &format!("unresolvable ref {r}")))?;
                let espec = ExperimentSpec::from_json(&body)
                    .map_err(|e| bad("/spec/spec_ref", &format!("{e:?}")))?;
                (
                    Some(espec.design.clone()),
                    resolve_arms(&docs, &espec)?,
                    espec.pre_registration.clone(),
                )
            }
            None => (None, Vec::new(), None),
        };

        // The rows — durable projections, resolved by key or version id —
        // plus each row's manifest and ledger-facts view (the kernel
        // refuses a row whose run the ledger does not know).
        let results = ResultsStore::open(self.store.root().join("results")).map_err(refused)?;
        let selectors = match req(params, "rows")? {
            Json::Arr(a) => a,
            _ => return Err(bad("/rows", "type_mismatch")),
        };
        let mut rows: Vec<ResultsRow> = Vec::new();
        let mut manifests: BTreeMap<String, RunManifest> = BTreeMap::new();
        let mut facts: BTreeMap<String, LedgerFacts> = BTreeMap::new();
        let mut watermark_set = WatermarkSet::new();
        for sel in selectors {
            let sel = sel.as_str().ok_or_else(|| bad("/rows", "type_mismatch"))?;
            let (row, _annotations) = results.get_row(&self.store, sel).map_err(refused)?;
            for (rid, seq) in &row.watermark_set.runs {
                watermark_set.pin(rid, *seq);
            }
            let run_id = row.key.run_id.clone();
            if !manifests.contains_key(&run_id) {
                manifests.insert(
                    run_id.clone(),
                    self.store.manifest(&run_id).map_err(refused)?.clone(),
                );
                let evs: Vec<(u64, String, Json)> = self
                    .store
                    .events(&run_id)
                    .map_err(refused)?
                    .iter()
                    .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
                    .collect();
                facts.insert(run_id, LedgerFacts::from_events(&evs));
            }
            rows.push(row);
        }

        let declarations = decls_for(&spec.query.metrics)?;
        let metric_registry_version = params
            .get("metric_registry_version")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let price_table_version = params
            .get("price_table_version")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let input = AnalysisInput {
            rows: &rows,
            manifests: &manifests,
            facts: &facts,
            tasks: &tasks,
            suites: &suites,
            design: design.as_ref(),
            arm_specs: &arm_specs,
            declarations: &declarations,
            pre_registration: pre_registration.as_ref(),
            watermark_set: &watermark_set,
            metric_registry_version: &metric_registry_version,
            price_table_version: &price_table_version,
            confidence_ppm: params
                .get("confidence_ppm")
                .and_then(Json::as_int)
                .unwrap_or(950_000),
            resampling_draws: params
                .get("resampling_draws")
                .and_then(Json::as_int)
                .unwrap_or(2000) as u64,
            seed: params.get("seed").and_then(Json::as_int).unwrap_or(0) as u64,
            // `post_amendment` — the bound experiment run's view carries
            // `measurement.experiment.amended` rows (§6.3; ADR-0162). A
            // mixed/unbound selection is never marked amended.
            post_amendment: {
                let mut bound: Option<&str> = None;
                let mut mixed = false;
                for r in &rows {
                    let e = r
                        .experiment
                        .as_ref()
                        .and_then(|x| x.get("experiment_run_id"))
                        .and_then(Json::as_str);
                    match (bound, e) {
                        (None, Some(e)) => bound = Some(e),
                        (Some(b), Some(e)) if b == e => {}
                        _ => mixed = true,
                    }
                }
                match (mixed, bound) {
                    (false, Some(eid)) => match self.store.events(eid) {
                        // The `experiment_run_id` marker is row metadata — a
                        // bound id that is not a ledger run (ad-hoc rows, a
                        // foreign store) is never an amended experiment, and
                        // the analysis is never refused for it.
                        Ok(evs) => !hh_experiment::view::ExperimentView::fold(evs)
                            .amendments
                            .is_empty(),
                        Err(_) => false,
                    },
                    _ => false,
                }
            },
        };
        let outcome = analyze_and_record(&docs, &spec, &input).map_err(xerr)?;
        Ok(Json::obj([
            ("record", outcome.record.to_json()),
            ("report", outcome.report.to_json()),
            ("body", outcome.body),
        ]))
    }
}
