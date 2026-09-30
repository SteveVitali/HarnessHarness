//! Group L — `lab.eval.*` dispatch (S3.3; R-2.9.2/R-2.9.4⁰ᵇ). Records-in,
//! records-out over the `hh-eval` kernel: the ops carry canonical-JSON
//! `eval_run/1` rows, `task_context` rows, `Design`/`PreRegistration`
//! records, and catalogue metric names — the service never invents a second
//! schema (CC7: `EvalRun::from_json`, `Design::from_json`,
//! `MetricDeclaration::from_json` are the decoders).
//!
//! Ops:
//! - `lab.eval.catalogue` — the catalogue conformance report
//!   (`check_catalogue`);
//! - `lab.eval.compare` — `compare` over records-in runs;
//! - `lab.eval.render_scorecard` — `render_scorecard`;
//! - `lab.eval.equivalence_run` — the three-valued equivalence verdict;
//! - `lab.eval.loss_report` — export a `LoweringLossReport` verbatim.
//!
//! `arm_specs` params decode `{match_spec, caps{dim→int}, native}` — the
//! `ArmSpec` wiring the match check consumes (budgets are hard caps at the
//! eval boundary; richer budget forms stay records in `hh-budget`).

use hh_budget::matchspec::{ArmSpec, BudgetEnforcement, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::benefits;
use hh_eval::catalogue;
use hh_eval::compare::{compare, CompareInput};
use hh_eval::runs::{EvalRun, SuiteContext, TaskContext};
use hh_eval::scorecard::{render_scorecard, ScorecardInput};
use hh_ontology::dimensions::DimensionKey;
use hh_ontology::eval::{Design, MetricValueKind, PreRegistration};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_wire::json::Json;

use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, EmbedError> {
    req(j, k)?
        .as_str()
        .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn arr<'a>(j: &'a Json, k: &str) -> Result<&'a Vec<Json>, EmbedError> {
    match req(j, k)? {
        Json::Arr(a) => Ok(a),
        _ => Err(bad(&format!("/{k}"), "type_mismatch")),
    }
}

fn refused(e: impl std::fmt::Display) -> EmbedError {
    EmbedError::Refused {
        reason: e.to_string(),
    }
}

/// Decode one `eval_run/1` member.
fn decode_runs(j: &Json) -> Result<Vec<EvalRun>, EmbedError> {
    arr(j, "runs")?
        .iter()
        .map(|r| EvalRun::from_json(r).map_err(|e| bad("/runs", &format!("{e}"))))
        .collect()
}

/// Decode `task_context` params (`{task_id, suite_id, split_label,
/// split_hash, stratum}`).
fn decode_tasks(j: &Json) -> Result<Vec<TaskContext>, EmbedError> {
    arr(j, "tasks")?
        .iter()
        .map(|t| {
            Ok(TaskContext {
                task_id: req_str(t, "task_id")?.to_string(),
                suite_id: req_str(t, "suite_id")?.to_string(),
                split_label: SplitLabel::parse(req_str(t, "split_label")?)
                    .ok_or_else(|| bad("/tasks/split_label", "unknown"))?,
                split_hash: req_str(t, "split_hash")?.to_string(),
                stratum: ContaminationStratum::parse(req_str(t, "stratum")?)
                    .ok_or_else(|| bad("/tasks/stratum", "unknown"))?,
            })
        })
        .collect()
}

/// Decode `suite_context` params.
fn decode_suites(j: &Json) -> Result<Vec<SuiteContext>, EmbedError> {
    arr(j, "suites")?
        .iter()
        .map(|s| {
            Ok(SuiteContext {
                suite_id: req_str(s, "suite_id")?.to_string(),
                retired_for_headline: matches!(
                    s.get("retired_for_headline"),
                    Some(Json::Bool(true))
                ),
                family: EnvironmentFamily::parse(req_str(s, "family")?)
                    .ok_or_else(|| bad("/suites/family", "unknown"))?,
            })
        })
        .collect()
}

/// Decode one `arm_specs[]` member — `{match_spec, caps{dim→int}, native?}`;
/// search/eval budgets are the same hard caps (the eval boundary's form).
fn decode_arms(j: &Json) -> Result<Vec<ArmSpec>, EmbedError> {
    arr(j, "arm_specs")?
        .iter()
        .map(|a| {
            let spec = MatchSpec::from_json(
                a.get("match_spec")
                    .ok_or_else(|| bad("/arm_specs/match_spec", "missing_field"))?,
            )
            .ok_or_else(|| bad("/arm_specs/match_spec", "decode_failed"))?;
            let mut caps = Vec::new();
            if let Some(Json::Obj(cs)) = a.get("caps") {
                for (d, v) in cs {
                    let key = DimensionKey::parse(d)
                        .ok_or_else(|| bad("/arm_specs/caps", "unknown dimension"))?;
                    let limit = v
                        .as_int()
                        .ok_or_else(|| bad("/arm_specs/caps", "cap must be int"))?;
                    caps.push((key, limit));
                }
            }
            let budget = BudgetSpec::hard_caps(BudgetMode::Pool, &caps);
            Ok(ArmSpec {
                search_budget: Some(budget.clone()),
                eval_budget: Some(budget),
                inference_budget: None,
                match_spec: Some(spec),
                enforcement: if matches!(a.get("native"), Some(Json::Bool(false))) {
                    BudgetEnforcement::hosted(&[])
                } else {
                    BudgetEnforcement::native()
                },
                spend_confidence: None,
                coverage_ppm: None,
            })
        })
        .collect()
}

/// The shared `compare` params → `CompareInput` (declarations resolve
/// against the catalogue; the metric names are params).
fn compare_input<'a>(
    params: &'a Json,
    runs: &'a [EvalRun],
    tasks: &'a [TaskContext],
    design: &'a Design,
    arms: &'a [ArmSpec],
    decls: &'a [hh_ontology::compliance::MetricDeclaration],
    metrics: &'a [String],
) -> Result<CompareInput<'a>, EmbedError> {
    Ok(CompareInput {
        arm_a: req_str(params, "arm_a")?,
        arm_b: req_str(params, "arm_b")?,
        metrics,
        declarations: decls,
        runs,
        tasks,
        design,
        arm_specs: arms,
        varied_factor: opt_str(params, "varied_factor")
            .map(String::leak)
            .map(|s| &*s),
        confidence_ppm: req(params, "confidence_ppm")
            .ok()
            .and_then(Json::as_int)
            .unwrap_or(950_000),
        benefit_kind: opt_str(params, "benefit_kind")
            .and_then(|s| hh_lab::analysis::BenefitKind::parse(&s))
            .unwrap_or(hh_lab::analysis::BenefitKind::ArtifactBenefit),
        held_out: matches!(params.get("held_out"), Some(Json::Bool(true))),
        family_size: params
            .get("family_size")
            .and_then(Json::as_int)
            .map(|f| f as u32),
    })
}

/// Resolve metric names against the catalogue (a name with no row is a
/// refusal, never a silent skip).
fn decls_for(
    metrics: &[String],
) -> Result<Vec<hh_ontology::compliance::MetricDeclaration>, EmbedError> {
    metrics
        .iter()
        .map(|m| {
            catalogue::metric(m).ok_or_else(|| bad("/metrics", &format!("unknown metric {m}")))
        })
        .collect()
}

fn metric_names(params: &Json) -> Result<Vec<String>, EmbedError> {
    arr(params, "metrics")?
        .iter()
        .map(|m| {
            m.as_str()
                .map(str::to_string)
                .ok_or_else(|| bad("/metrics", "type_mismatch"))
        })
        .collect()
}

impl EmbedService {
    /// `lab.eval.catalogue` — the catalogue conformance report.
    pub(crate) fn lab_eval_catalogue(&mut self, _params: &Json) -> Result<Json, EmbedError> {
        let r = catalogue::check_catalogue();
        Ok(Json::obj([
            ("schema", Json::str("catalogue_report/1")),
            ("metrics", Json::Int(r.metrics as i64)),
            ("oracles", Json::Int(r.oracles as i64)),
            (
                "metric_registry_version",
                Json::str(&r.metric_registry_version),
            ),
            ("conformant", Json::Bool(r.findings.is_empty())),
            (
                "findings",
                Json::Arr(
                    r.findings
                        .iter()
                        .map(|f| Json::str(format!("{f:?}")))
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.eval.compare` — `compare` over records-in runs.
    pub(crate) fn lab_eval_compare(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let runs = decode_runs(params)?;
        let tasks = decode_tasks(params)?;
        let design = Design::from_json(req(params, "design")?)
            .map_err(|e| bad("/design", &format!("{e:?}")))?;
        let arms = decode_arms(params)?;
        let metrics = metric_names(params)?;
        let decls = decls_for(&metrics)?;
        let inp = compare_input(params, &runs, &tasks, &design, &arms, &decls, &metrics)?;
        let out = compare(&inp).map_err(refused)?;
        Ok(Json::obj([
            (
                "reports",
                Json::Arr(out.reports.iter().map(|r| r.to_json()).collect()),
            ),
            (
                "per_task",
                Json::Arr(
                    out.per_task
                        .iter()
                        .map(|t| Json::Arr(t.iter().map(|e| e.to_json()).collect()))
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.eval.render_scorecard` — `render_scorecard` over records-in.
    pub(crate) fn lab_eval_render_scorecard(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let runs = decode_runs(params)?;
        let tasks = decode_tasks(params)?;
        let suites = decode_suites(params)?;
        let metrics = metric_names(params)?;
        let decls = decls_for(&metrics)?;
        let registry = catalogue::check_catalogue().metric_registry_version;
        // `model_families{snapshot_id → family}` — the portability cell's
        // declared-family evidence (AC-R-2.9.2-10); absent members are
        // `unknown`, never coerced.
        let mut model_families = std::collections::BTreeMap::new();
        if let Some(Json::Obj(fm)) = params.get("model_families") {
            for (k, v) in fm {
                if let Some(fam) = v.as_str() {
                    model_families.insert(k.clone(), fam.to_string());
                }
            }
        }
        let inp = ScorecardInput {
            runs: &runs,
            tasks: &tasks,
            suites: &suites,
            declarations: &decls,
            metric_registry_version: &registry,
            price_table_version: &opt_str(params, "price_table_version")
                .unwrap_or_else(|| "none".into()),
            watermark: req(params, "watermark")
                .ok()
                .and_then(Json::as_int)
                .unwrap_or(0) as u64,
            confidence_ppm: req(params, "confidence_ppm")
                .ok()
                .and_then(Json::as_int)
                .unwrap_or(950_000),
            pool_strata: matches!(params.get("pool_strata"), Some(Json::Bool(true))),
            annotate_pooling: matches!(params.get("annotate_pooling"), Some(Json::Bool(true))),
            model_families: &model_families,
        };
        let report = render_scorecard(&inp).map_err(refused)?;
        Ok(report.to_json())
    }

    /// `lab.eval.equivalence_run` — the three-valued equivalence verdict.
    pub(crate) fn lab_eval_equivalence_run(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let runs = decode_runs(params)?;
        let tasks = decode_tasks(params)?;
        let design = Design::from_json(req(params, "design")?)
            .map_err(|e| bad("/design", &format!("{e:?}")))?;
        let arms = decode_arms(params)?;
        let metrics = metric_names(params)?;
        let decls = decls_for(&metrics)?;
        let prereg = PreRegistration::from_json(req(params, "pre_registration")?)
            .map_err(|e| bad("/pre_registration", &format!("{e:?}")))?;
        let inp = compare_input(params, &runs, &tasks, &design, &arms, &decls, &metrics)?;
        let report = benefits::equivalence_run(
            req_str(params, "reference")?,
            req_str(params, "candidate")?,
            req_str(params, "suite_ref")?,
            &prereg,
            &inp,
            None,
        )
        .map_err(refused)?;
        Ok(report.to_json())
    }

    /// `lab.eval.loss_report` — export a `LoweringLossReport` verbatim.
    /// Params carry the record's members (`{target, target_version,
    /// entries[], granularity_ceiling}`); nothing is summarised (CC3).
    pub(crate) fn lab_eval_loss_report(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let reports: Vec<Json> = arr(params, "reports")?.clone();
        let mut out = Vec::new();
        for r in &reports {
            let entries: Vec<hh_compiler::lcd::LossEntry> = match r.get("entries") {
                Some(Json::Arr(es)) => es
                    .iter()
                    .map(|e| {
                        Ok(hh_compiler::lcd::LossEntry {
                            hir_node_id: req_str(e, "hir_node_id")?.to_string(),
                            field: req_str(e, "field")?.to_string(),
                            class: hh_compiler::lcd::LossKind::parse(req_str(e, "class")?)
                                .ok_or_else(|| bad("/entries/class", "unknown"))?,
                            severity: hh_compiler::lcd::LossSeverity::parse(req_str(
                                e, "severity",
                            )?)
                            .ok_or_else(|| bad("/entries/severity", "unknown"))?,
                            detail: req_str(e, "detail")?.to_string(),
                            debt_ref: opt_str(e, "debt_ref"),
                        })
                    })
                    .collect::<Result<_, EmbedError>>()?,
                _ => vec![],
            };
            let rep = hh_compiler::lcd::LoweringLossReport {
                target: req_str(r, "target")?.to_string(),
                target_version: req_str(r, "target_version")?.to_string(),
                entries,
                granularity_ceiling: hh_compiler::lcd::GranularityCeiling::parse(req_str(
                    r,
                    "granularity_ceiling",
                )?)
                .ok_or_else(|| bad("/granularity_ceiling", "unknown"))?,
            };
            out.push(hh_eval::loss::export_loss_report(&rep));
        }
        Ok(Json::Arr(out))
    }
}

/// The `MetricValueKind::Na` convenience — embed responses render the typed
/// n/a directly through `MetricValueKind::to_json`.
#[allow(dead_code)]
fn na_json(r: hh_ontology::compliance::NaReason) -> Json {
    MetricValueKind::Na(r).to_json()
}
