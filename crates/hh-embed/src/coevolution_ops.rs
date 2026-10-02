//! Group L — the `lab.coevolution.*` + `lab.org_policy.*` dispatch
//! (S6.4; §5h.8 R-2.9.8; §5h.5/§5h.6 6d; §5i.1 6d; ADR-0325). Every op
//! is records-in/records-out over the Lab's own vocabulary —
//! HarnessHarness never trains (N9): the ops mint claims, projections,
//! exports, sidecar records and typed refusals, never weights.
//!
//! - `export_training` lowers `training_export/1` through
//!   `hh_bundle::export::export_training` with the typed loss report;
//!   `measurement.export.delivered{TrainingExposureRecord}` lands on
//!   each subject run (CF-460).
//! - `import_snapshot` is claims-only (I-1..I-4): `pinned = false`,
//!   `trained_under`/`unknown` compatibility set, the
//!   `lifecycle.registry.imported` row minted on the registry audit
//!   run. The reverse sweep rides `lab.debt.post_import_sweep`.
//! - `consolidation_candidates` is the R-2.9.5 6d view over the folded
//!   campaign; `propose_consolidation`/`consolidation_retirement_record`
//!   carry the proposal → human-seal path (the retirement gate is
//!   `lab.debt.retire` — no second authority path).
//! - `cycle_*` drives the `CoEvolutionCycleRecord` sidecar
//!   (`hh_evolution::cycle::CycleDriver` — a LabDocs sidecar, no new
//!   run kind).
//! - `lab.org_policy.*` lands §5i.1 6d: the `lab/org-policy-v1` recipe
//!   and the fleet defaults' removal-test debt records.
//!
//! A `--no-default-features` build answers `Unsupported{by: "tier-c4"}`
//! — the ops stay in the schema (CC7) and the tier is removable (CC6;
//! AC-R-2.9.8-11).

use hh_wire::json::Json;

use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

#[cfg(feature = "tier-c4")]
fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

#[cfg(feature = "tier-c4")]
fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

#[cfg(feature = "tier-c4")]
fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, EmbedError> {
    req(j, k)?
        .as_str()
        .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
}

#[cfg(feature = "tier-c4")]
fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

#[cfg(feature = "tier-c4")]
fn coe_err(e: hh_lab::coevolution::CoEvolutionError) -> EmbedError {
    EmbedError::Refused { reason: e.code() }
}

#[cfg(feature = "tier-c4")]
mod imp {
    use super::{bad, coe_err, opt_str, req, req_str};
    use hh_embed_schema::errors::EmbedError;
    use hh_evolution::cycle::CycleDriver;
    use hh_lab::coevolution as coe;
    use hh_lab::model::{CompatibilityRecord, SnapshotClaim};
    use hh_wire::json::Json;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    /// The kernel producer the import's `lifecycle.registry.imported`
    /// row carries (one authority path — `commit_kernel_row_for`).
    const COE_COMPONENT: &str = "hh-embed:lab.coevolution";

    /// `debts[]` → `AssumptionDebtRecord`s (the `lab.debt.*` decoder —
    /// one codec, CC1/CC7).
    fn debts(j: Option<&Json>) -> Result<Vec<hh_hir::records::AssumptionDebtRecord>, EmbedError> {
        match j {
            Some(Json::Arr(a)) => a
                .iter()
                .map(crate::debt_ops::debt_record)
                .collect::<Result<Vec<_>, _>>(),
            None | Some(Json::Null) => Ok(Vec::new()),
            _ => Err(bad("/debts", "type_mismatch")),
        }
    }

    /// `records[]` → `CompatibilityRecord`s.
    fn compat_records(j: Option<&Json>) -> Result<Vec<CompatibilityRecord>, EmbedError> {
        match j {
            Some(Json::Arr(a)) => a
                .iter()
                .map(|c| {
                    CompatibilityRecord::from_json(c)
                        .map_err(|e| bad("/records", &format!("{e:?}")))
                })
                .collect(),
            None | Some(Json::Null) => Ok(Vec::new()),
            _ => Err(bad("/records", "type_mismatch")),
        }
    }

    /// The `ConsolidationReport` decoder (the record's canonical JSON —
    /// `to_json`'s exact inverse for the members the retirement names).
    fn consolidation_report(j: &Json) -> Result<coe::ConsolidationReport, EmbedError> {
        let verdict = match j.get("verdict").and_then(Json::as_str) {
            Some("absorbed") => coe::ConsolidationVerdict::Absorbed,
            Some("rejected") => coe::ConsolidationVerdict::Rejected,
            Some("superseded") => coe::ConsolidationVerdict::Superseded,
            _ => return Err(bad("/report/verdict", "missing_or_unknown")),
        };
        let strs = |k: &str| -> Vec<String> {
            match j.get(k) {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            }
        };
        Ok(coe::ConsolidationReport {
            report_id: opt_str(j, "report_id").unwrap_or_default(),
            proposal_ref: opt_str(j, "proposal_ref"),
            target_rule: req_str(j, "target_rule")?.to_string(),
            snapshot_ref: req_str(j, "snapshot_ref")?.to_string(),
            lessons: strs("lessons"),
            verdict,
            experiment_ref: req_str(j, "experiment_ref")?.to_string(),
            evidence_refs: strs("evidence_refs"),
            debt_refs: strs("debt_refs"),
        })
    }

    /// `PhasePlan` → its canonical JSON spelling.
    fn plan_json(p: &coe::PhasePlan) -> Json {
        match p {
            coe::PhasePlan::HarnessSearch { base_ref } => Json::obj([
                ("phase", Json::str("harness_search")),
                ("base_ref", Json::str(base_ref)),
            ]),
            coe::PhasePlan::WeightUpdate => Json::obj([("phase", Json::str("weight_update"))]),
            coe::PhasePlan::ReEvaluation => Json::obj([("phase", Json::str("re_evaluation"))]),
            coe::PhasePlan::Consolidation => Json::obj([("phase", Json::str("consolidation"))]),
            coe::PhasePlan::Stop { reason } => {
                Json::obj([("phase", Json::str("stop")), ("reason", reason.to_json())])
            }
            coe::PhasePlan::ReSearch { base_ref } => Json::obj([
                ("phase", Json::str("re_search")),
                ("base_ref", Json::str(base_ref)),
            ]),
            coe::PhasePlan::Rollback { base_ref } => Json::obj([
                ("phase", Json::str("rollback")),
                ("base_ref", Json::str(base_ref)),
            ]),
        }
    }

    /// The cycle driver for `cycle` (the caller's stable key — the
    /// first `cycle_id`); `restore_ref` re-folds a deposited sidecar
    /// (the restart path — CC3).
    fn cycle<'m>(
        svc: &'m mut crate::service::EmbedService,
        params: &Json,
    ) -> Result<&'m mut CycleDriver, EmbedError> {
        let key = req_str(params, "cycle")?.to_string();
        if !svc.coevolution_cycles.contains_key(&key) {
            if let Some(r) = opt_str(params, "restore_ref") {
                let docs = svc.lab_docs()?;
                match CycleDriver::restore(&docs, &r)
                    .map_err(|e| EmbedError::Refused { reason: e.code() })?
                {
                    Some(d) => {
                        svc.coevolution_cycles.insert(key.clone(), d);
                    }
                    None => return Err(bad("/restore_ref", "unknown_cycle_doc")),
                }
            } else {
                return Err(bad("/cycle", "unknown_cycle"));
            }
        }
        svc.coevolution_cycles
            .get_mut(&key)
            .ok_or_else(|| bad("/cycle", "unknown_cycle"))
    }

    /// Deposit + report the sidecar (one leg per mutator — the
    /// re-sealed `cycle_id` + doc ref ride the reply). `docs` is
    /// caller-fetched so `driver`'s borrow of `coevolution_cycles`
    /// never overlaps a `svc` borrow.
    fn deposited(
        docs: &hh_experiment::docs::LabDocs,
        key: &str,
        driver: &mut CycleDriver,
    ) -> Result<Json, EmbedError> {
        let deposit_ref = driver
            .deposit(docs)
            .map_err(|e| EmbedError::Refused { reason: e.code() })?;
        Ok(Json::obj([
            ("cycle", Json::str(key)),
            ("cycle_id", Json::str(&driver.record.cycle_id)),
            ("deposit_ref", Json::str(&deposit_ref)),
            ("record", driver.record.to_json()),
        ]))
    }

    /// The `lab.coevolution.*` + `lab.org_policy.*` dispatch (tier-c4).
    pub(crate) fn dispatch(
        svc: &mut crate::service::EmbedService,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        match method {
            // `lab.coevolution.export_training{path|container, policy,
            // ctx{task_splits{}, corpus_readers[], harness_constraints{},
            // provenance?}, dir?}` → `{artefact, files[], loss_report,
            // granularity_ceiling, export_id}` — the `training_export/1`
            // lowering (E-1 deterministic; the delivered rows carry the
            // `TrainingExposureRecord`).
            "lab.coevolution.export_training" => {
                let decoded = crate::bundle_ops::decode_bundle_arg(
                    params,
                    "lab.coevolution.export_training",
                )?;
                let policy = coe::TrainingExportPolicy::from_json(req(params, "policy")?)
                    .map_err(coe_err)?;
                let ctx_j = params.get("ctx").cloned().unwrap_or(Json::obj([]));
                let task_splits: BTreeMap<String, String> = match ctx_j.get("task_splits") {
                    Some(Json::Obj(m)) => m
                        .iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect(),
                    _ => BTreeMap::new(),
                };
                let corpus_readers: Vec<String> = match ctx_j.get("corpus_readers") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                let ctx = coe::TrainingExportCtx {
                    task_splits,
                    corpus_readers,
                    harness_constraints: ctx_j
                        .get("harness_constraints")
                        .cloned()
                        .unwrap_or(Json::obj([])),
                    provenance: ctx_j.get("provenance").cloned(),
                };
                let out = hh_bundle::export::export_training(&decoded, &policy, &ctx)
                    .map_err(crate::bundle_ops::bundle_err)?;
                if let Some(dir) = params.get("dir").and_then(Json::as_str) {
                    let root = Path::new(dir);
                    std::fs::create_dir_all(root).map_err(|e| EmbedError::Refused {
                        reason: format!("export_dir: {e}"),
                    })?;
                    for (rel, bytes) in &out.files {
                        let path = root.join(rel);
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| EmbedError::Refused {
                                reason: format!("export_dir: {e}"),
                            })?;
                        }
                        std::fs::write(&path, bytes).map_err(|e| EmbedError::Refused {
                            reason: format!("export_write: {e}"),
                        })?;
                    }
                }
                // The exposure record the delivered rows carry (E-7;
                // `measurement.export.delivered` per subject run).
                let exposure = out
                    .files
                    .get("exposure_record.json")
                    .and_then(|b| hh_wire::json::parse(std::str::from_utf8(b).unwrap_or("")).ok())
                    .unwrap_or(Json::Null);
                for run in &out.delivered_runs {
                    let _ = svc.store.commit_kernel_row_for(
                        COE_COMPONENT,
                        run,
                        "measurement.export.delivered",
                        Json::obj([
                            ("sink_id", Json::str("target:training_export")),
                            ("view_kind", Json::str("bundle_export")),
                            ("content_classes", Json::Arr(vec![Json::str("structural")])),
                            ("target", Json::str("training_export/1")),
                            ("training_exposure", exposure.clone()),
                        ]),
                        vec![],
                        vec![],
                    );
                }
                Ok(Json::obj([
                    ("schema", Json::str("hh-training-export/1")),
                    ("artefact", Json::str(&out.artefact)),
                    (
                        "files",
                        Json::Arr(out.files.keys().map(|k| Json::str(k.clone())).collect()),
                    ),
                    ("loss_report", out.loss_report),
                    ("granularity_ceiling", Json::str(&out.granularity_ceiling)),
                ]))
            }
            // `lab.coevolution.export_regression_suite{definition_ref,
            // snapshot_in, template<ExperimentSpec>, environment_level_id,
            // environment_level_ref, artifact_semantic_id,
            // artifact_version_id, retention_set_ref,
            // compliance_rules{}, margins{}}` → `{suite}` — the
            // pre-registered paired spec the conformance run executes
            // (`kind: comparative`, `design: paired`, `model_snapshot`
            // factor with the unbound `snapshot_out` level).
            "lab.coevolution.export_regression_suite" => {
                let template =
                    hh_lab::experiment::ExperimentSpec::from_json(req(params, "template")?)
                        .map_err(|e| bad("/template", &format!("{e:?}")))?;
                let pricing = template
                    .arms
                    .first()
                    .and_then(|a| a.match_spec.as_ref())
                    .and_then(|m| m.pricing_table_ref.clone());
                let pins = coe::RegressionSuitePins {
                    suite_ref: template.suite.suite_ref.clone(),
                    split_labels_used: template.suite.split_labels_used.clone(),
                    split_assignment_ref: template
                        .suite
                        .split_assignment_ref
                        .clone()
                        .unwrap_or_default(),
                    eval_budget_ref: template
                        .arms
                        .first()
                        .map(|a| a.eval_budget.clone())
                        .unwrap_or_default(),
                    search_budget_ref: template
                        .arms
                        .first()
                        .and_then(|a| a.search_budget.clone())
                        .unwrap_or_default(),
                    artifact_semantic_id: req_str(params, "artifact_semantic_id")?.to_string(),
                    artifact_version_id: req_str(params, "artifact_version_id")?.to_string(),
                    environment_level_id: req_str(params, "environment_level_id")?.to_string(),
                    environment_level_ref: req_str(params, "environment_level_ref")?.to_string(),
                    registry_snapshot_id: template
                        .design
                        .registry_snapshot_id
                        .clone()
                        .unwrap_or_default(),
                    analysis_plan_ref: template.design.pre_registration.analysis_plan_ref.clone(),
                    task_split_hash: template.design.pre_registration.task_split_hash.clone(),
                    pricing_table_ref: pricing,
                    registered_at: template.design.pre_registration.registered_at,
                    replicates_per_cell: template.replicates_per_cell,
                    seed_policy: template.seed_policy.clone(),
                    scheduling: template.scheduling.clone(),
                    reattempt: template.reattempt.clone(),
                    budgets: template.budgets.clone(),
                };
                let compliance_rules: BTreeMap<String, String> =
                    match req(params, "compliance_rules")? {
                        Json::Obj(m) => m
                            .iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect(),
                        _ => return Err(bad("/compliance_rules", "type_mismatch")),
                    };
                let suite = coe::export_regression_suite(
                    req_str(params, "definition_ref")?,
                    req_str(params, "snapshot_in")?,
                    &pins,
                    req_str(params, "retention_set_ref")?,
                    compliance_rules,
                    req(params, "margins")?.clone(),
                )
                .map_err(coe_err)?;
                Ok(Json::obj([("suite", suite.to_json())]))
            }
            // `lab.coevolution.export_compatibility_tags{snapshot_id,
            // records[]?, debts[]?}` → `{tag_set}` — the read-only
            // trainer-facing projection (ADR-0203 D1).
            "lab.coevolution.export_compatibility_tags" => {
                let set = coe::export_compatibility_tags(
                    req_str(params, "snapshot_id")?,
                    &compat_records(params.get("records"))?,
                    &debts(params.get("debts"))?,
                );
                Ok(Json::obj([("tag_set", set.to_json())]))
            }
            // `lab.coevolution.import_snapshot{claim,
            // sealed_definitions[]?, debts[]?, route_status?,
            // known_snapshot_ids[]?, descendant_snapshot_ids[]?,
            // registry_run?}` → `{snapshot_record, compatibility[],
            // scoped_rules[]}` — the claims-only import (I-1..I-4) +
            // the minted `lifecycle.registry.imported` row.
            "lab.coevolution.import_snapshot" => {
                let claim = SnapshotClaim::from_json(req(params, "claim")?)
                    .map_err(|e| bad("/claim", &format!("{e:?}")))?;
                let sealed: Vec<coe::SealedDefinition> = match params.get("sealed_definitions") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .map(|d| {
                            Ok(coe::SealedDefinition {
                                definition_semantic_id: d
                                    .get("definition_semantic_id")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| {
                                        bad("/sealed_definitions", "missing semantic id")
                                    })?
                                    .to_string(),
                                profile_semantic_id: d
                                    .get("profile_semantic_id")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| {
                                        bad("/sealed_definitions", "missing profile id")
                                    })?
                                    .to_string(),
                            })
                        })
                        .collect::<Result<Vec<_>, EmbedError>>()?,
                    _ => Vec::new(),
                };
                let route_status = match params.get("route_status").and_then(Json::as_str) {
                    Some("reachable") => coe::RouteStatus::Reachable,
                    Some("unreachable") => coe::RouteStatus::Unreachable,
                    _ => coe::RouteStatus::Unchecked,
                };
                let strs = |k: &str| -> Vec<String> {
                    match params.get(k) {
                        Some(Json::Arr(a)) => a
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    }
                };
                let provenance =
                    hh_provenance::ProvenanceRecord::kernel(COE_COMPONENT, svc.store.now_ms());
                let outcome = coe::import_snapshot(
                    &claim,
                    &sealed,
                    &debts(params.get("debts"))?,
                    route_status,
                    &strs("known_snapshot_ids"),
                    &strs("descendant_snapshot_ids"),
                    &provenance,
                    svc.store.now_ms(),
                )
                .map_err(coe_err)?;
                // The registry lifecycle row — minted on the registry
                // audit run (or the caller's `registry_run`).
                let run = match opt_str(params, "registry_run") {
                    Some(r) => r,
                    None => svc.ensure_registry_run().map(|(r, _)| r)?,
                };
                svc.store
                    .commit_kernel_row_for(
                        COE_COMPONENT,
                        &run,
                        "lifecycle.registry.imported",
                        outcome.lifecycle.clone(),
                        vec![],
                        vec![],
                    )
                    .map_err(|e| EmbedError::Refused {
                        reason: format!("ledger: {e}"),
                    })?;
                Ok(Json::obj([
                    ("snapshot_record", outcome.snapshot_record),
                    (
                        "compatibility",
                        Json::Arr(outcome.compatibility.iter().map(|c| c.to_json()).collect()),
                    ),
                    (
                        "scoped_rules",
                        Json::Arr(outcome.scoped_rules.iter().map(Json::str).collect()),
                    ),
                    ("maturity", Json::str("research-grade")),
                    ("label", Json::str("preview")),
                ]))
            }
            // `lab.coevolution.guard_at_bind{record?, arm_origin,
            // exploratory?, allow_unverified_snapshot?}` → `{guard}` —
            // the bind-time compatibility guard (G-1..G-3).
            "lab.coevolution.guard_at_bind" => {
                let record = match params.get("record") {
                    Some(Json::Null) | None => None,
                    Some(r) => Some(
                        CompatibilityRecord::from_json(r)
                            .map_err(|e| bad("/record", &format!("{e:?}")))?,
                    ),
                };
                let origin = match req_str(params, "arm_origin")? {
                    "evolution" => coe::ArmOrigin::Evolution,
                    "human" | "registered" => coe::ArmOrigin::Registered,
                    other => return Err(bad("/arm_origin", &format!("unknown {other}"))),
                };
                let exploratory = matches!(params.get("exploratory"), Some(Json::Bool(true)));
                let allow = matches!(
                    params.get("allow_unverified_snapshot"),
                    Some(Json::Bool(true))
                );
                match coe::guard_at_bind(record.as_ref(), origin, exploratory, allow) {
                    coe::BindGuard::Proceed => Ok(Json::obj([("guard", Json::str("proceed"))])),
                    coe::BindGuard::Annotated {
                        compatibility,
                        expiring_rules,
                    } => Ok(Json::obj([
                        ("guard", Json::str("annotated")),
                        ("compatibility", Json::str(&compatibility)),
                        (
                            "expiring_rules",
                            Json::Arr(expiring_rules.iter().map(Json::str).collect()),
                        ),
                    ])),
                    coe::BindGuard::Refused { code } => Ok(Json::obj([
                        ("guard", Json::str("refused")),
                        ("code", Json::str(code.code())),
                    ])),
                }
            }
            // `lab.coevolution.consolidation_candidates{run, lessons[],
            // economics{}, debt_complete[], policy{k_cycles,
            // economics_factor}}` → `{candidates[], never_consolidate[]}`
            // — the R-2.9.5 6d view over the folded campaign.
            "lab.coevolution.consolidation_candidates" => {
                let run = req_str(params, "run")?.to_string();
                let lessons: Vec<coe::LessonFact> = match params.get("lessons") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .map(|l| {
                            Ok(coe::LessonFact {
                                lesson_id: l
                                    .get("lesson_id")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| bad("/lessons", "missing lesson_id"))?
                                    .to_string(),
                                rule_id: l
                                    .get("rule_id")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| bad("/lessons", "missing rule_id"))?
                                    .to_string(),
                                kind: l
                                    .get("kind")
                                    .and_then(Json::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                effect: l.get("effect").and_then(Json::as_str).map(str::to_string),
                                provenance: l.get("provenance").cloned(),
                            })
                        })
                        .collect::<Result<Vec<_>, EmbedError>>()?,
                    _ => Vec::new(),
                };
                let economics: BTreeMap<String, f64> = match params.get("economics") {
                    Some(Json::Obj(m)) => m
                        .iter()
                        .filter_map(|(k, v)| v.as_int().map(|i| (k.clone(), i as f64)))
                        .collect(),
                    _ => BTreeMap::new(),
                };
                let debt_ok: BTreeSet<String> = match params.get("debt_complete") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => BTreeSet::new(),
                };
                let policy = coe::ConsolidationPolicy {
                    k_cycles: params
                        .get("policy")
                        .and_then(|p| p.get("k_cycles"))
                        .and_then(Json::as_int)
                        .map(|v| v as u64)
                        .unwrap_or(1),
                    economics_factor: params
                        .get("policy")
                        .and_then(|p| p.get("economics_factor"))
                        .and_then(Json::as_int)
                        .map(|v| v as f64)
                        .unwrap_or(1.0),
                };
                let docs = svc.lab_docs()?;
                let eng = crate::evolution_ops::imp::campaign(
                    &mut svc.evolution_campaigns,
                    &mut svc.store,
                    docs,
                    &run,
                )?;
                let view = eng
                    .candidate_view(&svc.store, None)
                    .map_err(crate::evolution_ops::imp::evo_err)?;
                let (candidates, refused) = hh_evolution::consolidation::consolidation_candidates(
                    &view,
                    &lessons,
                    &economics,
                    &|r: &str| debt_ok.contains(r),
                    &policy,
                );
                Ok(Json::obj([
                    (
                        "candidates",
                        Json::Arr(
                            candidates
                                .iter()
                                .map(|c| {
                                    Json::obj([
                                        ("rule_id", Json::str(&c.rule_id)),
                                        (
                                            "lessons",
                                            Json::Arr(c.lessons.iter().map(Json::str).collect()),
                                        ),
                                        ("rationale", Json::str(&c.rationale)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "never_consolidate",
                        Json::Arr(
                            refused
                                .iter()
                                .map(|(r, n)| {
                                    Json::obj([
                                        ("rule_id", Json::str(r)),
                                        ("reason", Json::str(n.name())),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    ("maturity", Json::str("research-grade")),
                    ("label", Json::str("preview")),
                ]))
            }
            // `lab.coevolution.propose_consolidation{target_rule,
            // lessons[], snapshot_in, debt_refs[], provenance}` →
            // `{proposal}` — a proposal record, never an applied diff.
            "lab.coevolution.propose_consolidation" => {
                let strs = |k: &str| -> Result<Vec<String>, EmbedError> {
                    match req(params, k)? {
                        Json::Arr(a) => Ok(a
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect()),
                        _ => Err(bad(&format!("/{k}"), "type_mismatch")),
                    }
                };
                let proposal = coe::ConsolidationProposal {
                    target_rule: req_str(params, "target_rule")?.to_string(),
                    lessons: strs("lessons")?,
                    snapshot_in: req_str(params, "snapshot_in")?.to_string(),
                    debt_refs: strs("debt_refs")?,
                    provenance: req(params, "provenance")?.clone(),
                };
                Ok(Json::obj([("proposal", proposal.to_json())]))
            }
            // `lab.coevolution.consolidation_retirement_record{report,
            // verdict_ref, decided_by}` → `{retirement_record}` — the
            // record `lab.debt.retire` consumes (the human seal is that
            // op's gate — this mints only the record).
            "lab.coevolution.consolidation_retirement_record" => {
                let report = consolidation_report(req(params, "report")?)?;
                let decided_by =
                    hh_provenance::ProvenanceRecord::from_json(req(params, "decided_by")?)
                        .map_err(|e| bad("/decided_by", &format!("{e:?}")))?;
                let rec = coe::consolidation_retirement_record(
                    &report,
                    req_str(params, "verdict_ref")?,
                    &decided_by,
                );
                Ok(Json::obj([("retirement_record", rec.to_json(true))]))
            }
            // `lab.coevolution.cycle_open{policy, lineage_ref}` →
            // `{cycle, cycle_id, deposit_ref, record}` — the sidecar's
            // open (the `cycle` key is the first `cycle_id`).
            "lab.coevolution.cycle_open" => {
                let policy =
                    coe::CyclePolicy::from_json(req(params, "policy")?).map_err(coe_err)?;
                let mut driver = CycleDriver::open(policy, req_str(params, "lineage_ref")?);
                let docs = svc.lab_docs()?;
                let deposit_ref = driver
                    .deposit(&docs)
                    .map_err(|e| EmbedError::Refused { reason: e.code() })?;
                let key = driver.record.cycle_id.clone();
                let cycle_id = key.clone();
                svc.coevolution_cycles.insert(key.clone(), driver);
                let d = svc.coevolution_cycles.get(&key).unwrap();
                Ok(Json::obj([
                    ("cycle", Json::str(&key)),
                    ("cycle_id", Json::str(&cycle_id)),
                    ("deposit_ref", Json::str(&deposit_ref)),
                    ("record", d.record.to_json()),
                ]))
            }
            // `cycle_begin_phase{cycle, phase, inputs{}}`.
            "lab.coevolution.cycle_begin_phase" => {
                let key = req_str(params, "cycle")?.to_string();
                let phase = coe::CyclePhase::parse(req_str(params, "phase")?)
                    .ok_or_else(|| bad("/phase", "unknown_phase"))?;
                let inputs = req(params, "inputs")?.clone();
                cycle(svc, params)?
                    .begin_phase(phase, inputs)
                    .map_err(|e| EmbedError::Refused { reason: e.code() })?;
                let docs = svc.lab_docs()?;
                let driver = svc.coevolution_cycles.get_mut(&key).unwrap();
                deposited(&docs, &key, driver)
            }
            // `cycle_complete_phase{cycle, outputs{}, experiment_ref?,
            // training_run_ref?, verdict?, budgets{}}`.
            "lab.coevolution.cycle_complete_phase" => {
                let key = req_str(params, "cycle")?.to_string();
                let outputs = req(params, "outputs")?.clone();
                let budgets = params.get("budgets").cloned().unwrap_or(Json::obj([]));
                cycle(svc, params)?
                    .complete_phase(
                        outputs,
                        opt_str(params, "experiment_ref").as_deref(),
                        opt_str(params, "training_run_ref").as_deref(),
                        opt_str(params, "verdict").as_deref(),
                        budgets,
                    )
                    .map_err(|e| EmbedError::Refused { reason: e.code() })?;
                let docs = svc.lab_docs()?;
                let driver = svc.coevolution_cycles.get_mut(&key).unwrap();
                deposited(&docs, &key, driver)
            }
            // `cycle_next{cycle}` → `{plan}`.
            "lab.coevolution.cycle_next" => {
                let driver = cycle(svc, params)?;
                Ok(Json::obj([("plan", plan_json(&driver.next()))]))
            }
            // `cycle_stop{cycle, reason, verdict?}` — `reason ∈
            // {max_cycles, search_budget_exhausted, veto_tripped,
            // compatibility_broken, consolidation_absorbed,
            // operator{reason}}`.
            "lab.coevolution.cycle_stop" => {
                let key = req_str(params, "cycle")?.to_string();
                let reason = match req(params, "reason")? {
                    Json::Str(s) => match s.as_str() {
                        "max_cycles" => coe::CycleStopReason::MaxCycles,
                        "search_budget_exhausted" => coe::CycleStopReason::SearchBudgetExhausted,
                        "veto_tripped" => coe::CycleStopReason::VetoTripped,
                        "compatibility_broken" => coe::CycleStopReason::CompatibilityBroken,
                        "consolidation_absorbed" => coe::CycleStopReason::ConsolidationAbsorbed,
                        other => return Err(bad("/reason", &format!("unknown {other}"))),
                    },
                    Json::Obj(_) => {
                        let r = params
                            .get("reason")
                            .and_then(|x| x.get("operator"))
                            .and_then(|x| x.get("reason"))
                            .and_then(Json::as_str)
                            .ok_or_else(|| bad("/reason", "operator needs reason"))?;
                        coe::CycleStopReason::Operator {
                            reason: r.to_string(),
                        }
                    }
                    _ => return Err(bad("/reason", "type_mismatch")),
                };
                let verdict = opt_str(params, "verdict");
                cycle(svc, params)?.stop(reason, verdict.as_deref());
                let docs = svc.lab_docs()?;
                let driver = svc.coevolution_cycles.get_mut(&key).unwrap();
                deposited(&docs, &key, driver)
            }
            // `cycle_record{cycle}` → `{record}`.
            "lab.coevolution.cycle_record" => {
                let driver = cycle(svc, params)?;
                Ok(Json::obj([("record", driver.record.to_json())]))
            }
            // `lab.org_policy.recipe{pins{…}, own{…}}` → `{spec}` —
            // the `lab/org-policy-v1` `ExperimentSpec` (the recipe every
            // fleet default's removal test instantiates; `register`
            // refuses an arm without the MatchSpec, T-LCD-14).
            "lab.org_policy.recipe" => {
                let pj = req(params, "pins")?;
                let s = |k: &str| req_str(pj, k).map(str::to_string);
                let pins = hh_lab::exemplars::ExemplarPins {
                    suite_ref: s("suite_ref")?,
                    held_out_split_ref: s("held_out_split_ref")?,
                    split_assignment_ref: s("split_assignment_ref")?,
                    registry_snapshot_id: s("registry_snapshot_id")?,
                    eval_budget: s("eval_budget")?,
                    search_budget: s("search_budget")?,
                    experiment_budget: s("experiment_budget")?,
                    instrument_budget: s("instrument_budget")?,
                    analysis_plan_ref: s("analysis_plan_ref")?,
                    task_split_hash: s("task_split_hash")?,
                };
                let oj = req(params, "own")?;
                let pricing = hh_budget::pricing::PricingTableRef::from_json(
                    oj.get("pricing_table_ref")
                        .ok_or_else(|| bad("/own/pricing_table_ref", "missing"))?,
                )
                .ok_or_else(|| bad("/own/pricing_table_ref", "malformed"))?;
                let factors: Vec<(String, Vec<hh_lab::exemplars::OrgPolicyLevel>)> =
                    match oj.get("factors") {
                        Some(Json::Arr(fa)) => fa
                            .iter()
                            .map(|f| {
                                let name = f
                                    .get("name")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| bad("/own/factors", "missing name"))?
                                    .to_string();
                                let levels = match f.get("levels") {
                                    Some(Json::Arr(la)) => la
                                        .iter()
                                        .map(|l| {
                                            Ok(hh_lab::exemplars::OrgPolicyLevel {
                                                level_id: l
                                                    .get("level_id")
                                                    .and_then(Json::as_str)
                                                    .ok_or_else(|| {
                                                        bad("/own/factors/levels", "missing id")
                                                    })?
                                                    .to_string(),
                                                content_ref: l
                                                    .get("content_ref")
                                                    .and_then(Json::as_str)
                                                    .ok_or_else(|| {
                                                        bad(
                                                            "/own/factors/levels",
                                                            "missing content_ref",
                                                        )
                                                    })?
                                                    .to_string(),
                                                label: l
                                                    .get("label")
                                                    .and_then(Json::as_str)
                                                    .unwrap_or("")
                                                    .to_string(),
                                            })
                                        })
                                        .collect::<Result<Vec<_>, EmbedError>>()?,
                                    _ => Vec::new(),
                                };
                                Ok((name, levels))
                            })
                            .collect::<Result<Vec<_>, EmbedError>>()?,
                        _ => Vec::new(),
                    };
                let arms: Vec<hh_lab::exemplars::OrgPolicyArmPins> = match oj.get("arms") {
                    Some(Json::Arr(aa)) => aa
                        .iter()
                        .map(|a| {
                            let levels: Vec<(String, String)> = match a.get("levels") {
                                Some(Json::Obj(m)) => m
                                    .iter()
                                    .filter_map(|(f, l)| {
                                        l.as_str().map(|s| (f.clone(), s.to_string()))
                                    })
                                    .collect(),
                                Some(Json::Arr(la)) => la
                                    .iter()
                                    .filter_map(|p| {
                                        p.as_str().and_then(|s| {
                                            s.split_once('=')
                                                .map(|(f, l)| (f.to_string(), l.to_string()))
                                        })
                                    })
                                    .collect(),
                                _ => Vec::new(),
                            };
                            Ok(hh_lab::exemplars::OrgPolicyArmPins {
                                arm_id: a
                                    .get("arm_id")
                                    .and_then(Json::as_str)
                                    .ok_or_else(|| bad("/own/arms", "missing arm_id"))?
                                    .to_string(),
                                hypothesis: a
                                    .get("hypothesis")
                                    .and_then(Json::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                levels,
                                artifact: hh_ontology::config::Ref::from_json(
                                    a.get("artifact")
                                        .ok_or_else(|| bad("/own/arms/artifact", "missing"))?,
                                )
                                .ok_or_else(|| bad("/own/arms/artifact", "malformed"))?,
                                simulated_responder_calibration_ref: a
                                    .get("simulated_responder_calibration_ref")
                                    .and_then(Json::as_str)
                                    .map(str::to_string),
                            })
                        })
                        .collect::<Result<Vec<_>, EmbedError>>()?,
                    _ => Vec::new(),
                };
                let registered_at = params
                    .get("registered_at")
                    .and_then(Json::as_int)
                    .map(|v| v as u64)
                    .unwrap_or_else(|| svc.store.now_ms());
                let spec = hh_lab::exemplars::org_policy_v1(
                    &pins,
                    &hh_lab::exemplars::OrgPolicyPins {
                        factors,
                        arms,
                        pricing_table_ref: pricing,
                    },
                    registered_at,
                );
                Ok(Json::obj([("spec", spec.to_json())]))
            }
            // `lab.org_policy.default_removal_tests{fleet, owner{team,
            // id, reach_via[]}, recipe_spec_ref, expires_at_ms?}` →
            // `{records[]}` — the fleet defaults' conditioned debt
            // records (`lab/org-policy-v1` as the removal test —
            // ADR-0207 D6; T-LCD-05).
            "lab.org_policy.default_removal_tests" => {
                let oj = req(params, "owner")?;
                let owner = hh_ontology::debt::OwnerRef {
                    team: matches!(oj.get("team"), Some(Json::Bool(true))),
                    id: oj
                        .get("id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("/owner/id", "missing"))?
                        .to_string(),
                    reach_via: match oj.get("reach_via") {
                        Some(Json::Arr(a)) => a
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    },
                };
                let prov =
                    hh_provenance::ProvenanceRecord::kernel(COE_COMPONENT, svc.store.now_ms());
                let records = hh_fleet::org_policy::fleet_default_debt_records(
                    req_str(params, "fleet")?,
                    &owner,
                    req_str(params, "recipe_spec_ref")?,
                    params
                        .get("expires_at_ms")
                        .and_then(Json::as_int)
                        .map(|v| v as u64),
                    &prov,
                    svc.store.now_ms(),
                );
                Ok(Json::obj([(
                    "records",
                    Json::Arr(
                        records
                            .iter()
                            .map(|r| hh_hir::debt_json(r, false))
                            .collect(),
                    ),
                )]))
            }
            _ => Err(EmbedError::SchemaViolation {
                path: "/method".to_string(),
                code: "unknown_method".to_string(),
            }),
        }
    }
}

impl EmbedService {
    /// The `lab.coevolution.*` + `lab.org_policy.*` dispatch (S6.4,
    /// tier-c4 build).
    #[cfg(feature = "tier-c4")]
    pub(crate) fn coevolution_dispatch(
        &mut self,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        imp::dispatch(self, method, params)
    }

    /// Tier absent — the ops remain in the schema (CC7) and answer the
    /// typed `tier_unavailable` refusal (CC6 removability;
    /// AC-R-2.9.8-11).
    #[cfg(not(feature = "tier-c4"))]
    pub(crate) fn coevolution_dispatch(
        &mut self,
        _method: &str,
        _params: &Json,
    ) -> Result<Json, EmbedError> {
        Err(EmbedError::Unsupported {
            by: "tier-c4".to_string(),
        })
    }
}
