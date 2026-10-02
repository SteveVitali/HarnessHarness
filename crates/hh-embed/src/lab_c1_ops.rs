//! Group L — the S4.3 C1 boundary ops (R-2.10.4¹ / R-2.10.5 §6.4–§6.5):
//!
//! - `lab.producer.*` — the §6.5 §2.3 producer contract (`declare`,
//!   `bind`, `exclude`, `amend`, `record_analysis`) over the experiment
//!   engine the `lab.experiment.*` ops already bind; the producer
//!   refusals (`DeclarationLate`, `PreRegistrationLate`,
//!   `NotPreRegistered`, `UnmatchedBudget`, `MissingMatchSpec`) render
//!   through `xerr` verbatim;
//! - `lab.results.*` — the results-store read/verify/export surface
//!   (`get_row`, `row_history`, `query_rows`, `cells`, `distribution`,
//!   `catalogue`, `subscribe`, `verify_row`, `verify_snapshot`,
//!   `verify_citation`, `export_rows`) over the derived-state store at
//!   `<store_root>/results`;
//! - `lab.leaderboard.*` — `define`, `leaderboard` (the retained
//!   snapshot), `diff_snapshots`, `publish`, `retract_entry`;
//! - `lab.analysis.{render, diff_reports, power}` — the A16 helpers +
//!   the A13 surface (`power` is `analyze` gated to `kind = "power"`).
//!
//! `lab.results.subscribe` is a pull surface (the boundary is
//! request/response): `{subscription_id?}` names a `JournalSubscription`
//! the service holds across calls; each call drains the committed
//! events — after-durability only, ordering preserved (ADR-0294).

use std::collections::BTreeSet;

use hh_experiment::docs::kind as doc_kind;
use hh_lab::analysis::AnalysisRecord;
use hh_results::journal::{JournalFilter, JournalKind};
use hh_results::leaderboard::LeaderboardDefinition;
use hh_results::query::QuerySpec;
use hh_results::store::ResultsStore;
use hh_results::watermark::WatermarkSet;
use hh_wire::json::Json;

use crate::eval_ops::{bad, opt_str, req, req_str};
use crate::experiment_ops::{engine_for, park, xerr, Bag};
use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

fn refused(e: impl std::fmt::Display) -> EmbedError {
    EmbedError::Refused {
        reason: format!("{e}"),
    }
}

/// The derived-state store (`<store_root>/results`) — one per call,
/// records-in/records-out.
fn results(svc: &EmbedService) -> Result<ResultsStore, EmbedError> {
    ResultsStore::open(svc.store.root().join("results")).map_err(refused)
}

/// `params.at` → `WatermarkSet` (`None` = current heads).
fn at_of(params: &Json) -> Result<Option<WatermarkSet>, EmbedError> {
    match params.get("at") {
        None | Some(Json::Null) => Ok(None),
        Some(j) => WatermarkSet::from_json(j)
            .map(Some)
            .ok_or_else(|| bad("/at", "watermark_set decode failed")),
    }
}

/// Resolve the `LeaderboardDefinition` — inline `definition` or the
/// persisted `definition_ref` (`define` deposits at
/// `leaderboards/defs/<definition_id>.json`).
fn definition_of(
    results: &ResultsStore,
    params: &Json,
) -> Result<LeaderboardDefinition, EmbedError> {
    if let Some(d) = params.get("definition") {
        return LeaderboardDefinition::from_json(d).map_err(refused);
    }
    let r = req_str(params, "definition_ref")?;
    results.leaderboard_definition(r).map_err(refused)
}

/// `VerifyVerdict` → wire form.
fn verdict_json(v: &hh_results::store::VerifyVerdict) -> Json {
    match v {
        hh_results::store::VerifyVerdict::Ok => Json::obj([("verdict", Json::str("ok"))]),
        hh_results::store::VerifyVerdict::Mismatch { recomputed } => Json::obj([
            ("verdict", Json::str("mismatch")),
            ("recomputed", Json::str(recomputed)),
        ]),
    }
}

impl EmbedService {
    // ── lab.producer.* ─────────────────────────────────────────────────

    /// `lab.producer.declare{experiment_id}` — the producer contract's
    /// `declared` (open the experiment run; the engine's
    /// `DeclarationLate`/`PreRegistrationLate` refusals ride through).
    pub(crate) fn lab_producer_declare(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let eid = req_str(params, "experiment_id")?.to_string();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let mut eng =
            hh_experiment::engine::ExperimentEngine::new(&mut self.store, docs, bag.ctx());
        let run_id = eng.open_experiment(&eid).map_err(xerr)?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("experiment_id", Json::str(eid)),
            ("experiment_run_id", Json::str(run_id)),
            ("declared", Json::Bool(true)),
        ]))
    }

    /// `lab.producer.bind{experiment_id|experiment_run_id, run_id,
    /// cell_id}` — `run_bound` on the experiment run (§6.5 §2.3).
    pub(crate) fn lab_producer_bind(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let run_id = req_str(params, "run_id")?.to_string();
        let cell_id = req_str(params, "cell_id")?.to_string();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let exp_run = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &exp_run,
        )?;
        eng.producer_bind(&run_id, &cell_id).map_err(xerr)?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("experiment_run_id", Json::str(exp_run)),
            ("run_id", Json::str(run_id)),
            ("cell_id", Json::str(cell_id)),
            ("bound", Json::Bool(true)),
        ]))
    }

    /// `lab.producer.exclude{experiment_run_id, run_plan_id, run_id,
    /// reason, authority, evidence?}` — `run_excluded{reason}`; an
    /// `analyst_exclusion` reason requires `authority = human` (the
    /// engine's refusal renders verbatim).
    pub(crate) fn lab_producer_exclude(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let run_plan_id = req_str(params, "run_plan_id")?.to_string();
        let run_id = req_str(params, "run_id")?.to_string();
        let reason = req_str(params, "reason")?.to_string();
        let authority = req_str(params, "authority")?.to_string();
        let evidence = params.get("evidence").cloned();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let exp_run = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &exp_run,
        )?;
        eng.exclude(
            &run_plan_id,
            &run_id,
            &reason,
            evidence.as_ref(),
            &authority,
        )
        .map_err(xerr)?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("run_id", Json::str(run_id)),
            ("excluded", Json::Bool(true)),
        ]))
    }

    /// `lab.producer.amend{experiment_run_id, diff_ref, reason,
    /// authority}` — `amended`; later `record_analysis` rows carry
    /// `post_amendment = true` (the view fold computes it).
    pub(crate) fn lab_producer_amend(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let diff_ref = req_str(params, "diff_ref")?.to_string();
        let reason = req_str(params, "reason")?.to_string();
        let authority = req_str(params, "authority")?.to_string();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let exp_run = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &exp_run,
        )?;
        eng.amend(&diff_ref, &reason, &authority).map_err(xerr)?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("experiment_run_id", Json::str(exp_run)),
            ("amended", Json::Bool(true)),
        ]))
    }

    /// `lab.producer.record_analysis{experiment_run_id, record,
    /// report_body?}` — validate the `AnalysisRecord` against the
    /// experiment view (missing `budget_match`/`benefit_kind` →
    /// `MissingMatchSpec`/`UnmatchedBudget`; a false `pre_registered` →
    /// `NotPreRegistered`) and stamp `measurement.analysis.recorded`.
    pub(crate) fn lab_producer_record_analysis(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let record = AnalysisRecord::from_json(req(params, "record")?)
            .map_err(|e| bad("/record", &format!("{e:?}")))?;
        let body = params.get("report_body").cloned();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let exp_run = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &exp_run,
        )?;
        eng.record_analysis(&record, body.as_ref()).map_err(xerr)?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("experiment_run_id", Json::str(exp_run)),
            ("analysis_id", Json::str(&record.analysis_id)),
            ("recorded", Json::Bool(true)),
        ]))
    }

    // ── lab.analysis.{render, diff_reports, power} ─────────────────────

    /// The stored report body — `body` inline or `report_id` (the LabDocs
    /// `analysis_report/<report_id>` doc `analyze_and_record` deposits).
    fn report_body(&self, params: &Json, member: &str) -> Result<Json, EmbedError> {
        if let Some(b) = params.get(member) {
            return Ok(b.clone());
        }
        let key = match member {
            "body" => "report_id",
            other => other,
        };
        let id = req_str(params, key)?;
        let docs = self.lab_docs()?;
        docs.get_named(doc_kind::ANALYSIS_REPORT, id)
            .map_err(refused)?
            .ok_or_else(|| bad(key, &format!("no recorded report {id}")))
    }

    /// `lab.analysis.render{report_id|body, view, options?}` — the A16
    /// projection (render never recomputes — ADR-0157 D8).
    pub(crate) fn lab_analysis_render(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let body = self.report_body(params, "body")?;
        let view = req_str(params, "view")?;
        let options = params.get("options").cloned().unwrap_or(Json::Null);
        hh_analysis::ops::render(&body, view, &options).map_err(refused)
    }

    /// `lab.analysis.diff_reports{a, b}` — the report-diff view; `a`/`b`
    /// are report ids (or inline bodies via `body_a`/`body_b`).
    pub(crate) fn lab_analysis_diff_reports(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let a = if let Some(b) = params.get("body_a") {
            b.clone()
        } else {
            self.report_body(params, "a")?
        };
        let b = if let Some(b) = params.get("body_b") {
            b.clone()
        } else {
            self.report_body(params, "b")?
        };
        Ok(hh_analysis::ops::diff_reports(&a, &b))
    }

    /// `lab.analysis.power{spec, rows[], …}` — the A13 surface: `analyze`
    /// gated to `kind = "power"` (same params, same records-out shape).
    pub(crate) fn lab_analysis_power(&mut self, params: &Json) -> Result<Json, EmbedError> {
        match params
            .get("spec")
            .and_then(|s| s.get("kind"))
            .and_then(Json::as_str)
        {
            Some("power") => {}
            other => {
                return Err(bad(
                    "/spec/kind",
                    &format!("lab.analysis.power requires kind = power (got {other:?})"),
                ))
            }
        }
        self.lab_analysis_analyze(params)
    }

    // ── lab.results.* ──────────────────────────────────────────────────

    /// `lab.results.get_row{row}` — the head (or `version_id`) row plus
    /// its annotation entry.
    pub(crate) fn lab_results_get_row(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let sel = req_str(params, "row")?;
        let (row, annotations) = results(self)?.get_row(&self.store, sel).map_err(refused)?;
        Ok(Json::obj([
            ("row", row.to_json()),
            ("annotations", annotations.to_json()),
        ]))
    }

    /// `lab.results.row_history{key_id}` — `[RowVersion]` root-first.
    pub(crate) fn lab_results_row_history(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let key = req_str(params, "key_id")?;
        let versions = results(self)?.row_history(key).map_err(refused)?;
        Ok(Json::obj([
            ("key_id", Json::str(key)),
            (
                "versions",
                Json::Arr(versions.iter().map(|v| v.to_json()).collect()),
            ),
        ]))
    }

    /// `lab.results.query_rows{filters?{}, at?, cursor?, limit?}` →
    /// `Page{rows[], next_cursor, watermark_set, view_hash}`.
    pub(crate) fn lab_results_query_rows(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let mut spec = QuerySpec::all();
        if let Some(Json::Obj(m)) = params.get("filters") {
            for (f, v) in m {
                let v = v.as_str().ok_or_else(|| bad("/filters", "type_mismatch"))?;
                spec = spec.filter(f, v).map_err(refused)?;
            }
        }
        spec.at = at_of(params)?;
        spec.cursor = opt_str(params, "cursor");
        spec.limit = params
            .get("limit")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as usize);
        let page = results(self)?
            .query_rows(&self.store, &spec)
            .map_err(refused)?;
        Ok(Json::obj([
            (
                "rows",
                Json::Arr(page.rows.iter().map(|r| r.to_json()).collect()),
            ),
            (
                "next_cursor",
                page.next_cursor.map_or(Json::Null, Json::str),
            ),
            ("watermark_set", page.watermark_set.to_json()),
            ("view_hash", Json::str(&page.view_hash)),
        ]))
    }

    /// `lab.results.cells{target, at?}` — the `build_cells` read surface.
    pub(crate) fn lab_results_cells(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let target = req_str(params, "target")?;
        let docs = self.lab_docs()?;
        let table = results(self)?
            .cells(&self.store, &docs, target, at_of(params)?.as_ref())
            .map_err(refused)?;
        Ok(table.to_json())
    }

    /// `lab.results.distribution{configuration_id, metric_ref, split?,
    /// at?}` — the `DistributionRef` view.
    pub(crate) fn lab_results_distribution(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let config = req_str(params, "configuration_id")?;
        let metric = req_str(params, "metric_ref")?;
        let d = results(self)?
            .distribution(
                &self.store,
                config,
                metric,
                opt_str(params, "split").as_deref(),
                at_of(params)?.as_ref(),
            )
            .map_err(refused)?;
        Ok(Json::obj([
            ("configuration_id", Json::str(&d.configuration_id)),
            ("metric_ref", Json::str(&d.metric_ref)),
            ("split", d.split.map_or(Json::Null, Json::str)),
            ("n", Json::Int(d.n as i64)),
            ("values", Json::Arr(d.values)),
            ("quantiles", d.quantiles.unwrap_or(Json::Null)),
            (
                "per_task_cn",
                Json::Obj(
                    d.per_task_cn
                        .iter()
                        .map(|(t, (c, n))| {
                            (
                                t.clone(),
                                Json::obj([
                                    ("c", Json::Int(*c as i64)),
                                    ("n", Json::Int(*n as i64)),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ),
            ("watermark_set", d.watermark_set.to_json()),
            ("view_hash", Json::str(&d.view_hash)),
        ]))
    }

    /// `lab.results.catalogue{refresh?}` — the persisted bundle
    /// catalogue (`refresh = true` rebuilds it first).
    pub(crate) fn lab_results_catalogue(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let r = results(self)?;
        let cat = if matches!(params.get("refresh"), Some(Json::Bool(true))) {
            r.catalogue_refresh(&self.store, None).map_err(refused)?
        } else {
            r.catalogue().map_err(refused)?
        };
        Ok(cat.to_json())
    }

    /// `lab.results.subscribe{subscription_id?, kinds?[]}` — the pull
    /// surface over the store's post-durability journal: the first call
    /// with a fresh `subscription_id` registers a `JournalSubscription`;
    /// every call drains the committed events in order (the boundary is
    /// request/response — events are never fabricated, only delivered
    /// after durability).
    pub(crate) fn lab_results_subscribe(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let id = opt_str(params, "subscription_id").unwrap_or_else(|| "default".to_string());
        let r = results(self)?;
        let sub = self
            .results_subscriptions
            .entry(id.clone())
            .or_insert_with(|| {
                let kinds: Option<BTreeSet<JournalKind>> = match params.get("kinds") {
                    Some(Json::Arr(items)) => Some(
                        items
                            .iter()
                            .filter_map(|i| i.as_str().and_then(JournalKind::parse))
                            .collect(),
                    ),
                    _ => None,
                };
                r.subscribe(JournalFilter { kinds })
            });
        let mut events = Vec::new();
        while let Some(e) = sub.try_recv() {
            events.push(e.to_json());
        }
        Ok(Json::obj([
            ("subscription_id", Json::str(&id)),
            ("events", Json::Arr(events)),
        ]))
    }

    /// `lab.results.verify_row{version_id}` — re-project and compare
    /// identities (`ok | mismatch{recomputed}`).
    pub(crate) fn lab_results_verify_row(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let version_id = req_str(params, "version_id")?;
        let docs = self.lab_docs()?;
        let v = results(self)?
            .verify_row(&self.store, Some(&docs), version_id)
            .map_err(refused)?;
        Ok(verdict_json(&v))
    }

    /// `lab.results.verify_snapshot{snapshot_id}` — re-project the
    /// leaderboard view at the snapshot's watermark set and compare.
    pub(crate) fn lab_results_verify_snapshot(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let snapshot_id = req_str(params, "snapshot_id")?;
        let docs = self.lab_docs()?;
        let v = results(self)?
            .verify_snapshot(&self.store, &docs, snapshot_id)
            .map_err(refused)?;
        Ok(verdict_json(&v))
    }

    /// `lab.results.verify_citation{audit_ref}` — the citation verdict
    /// (`ok | tampered:<seq> | missing:<reason> | proof_invalid`).
    pub(crate) fn lab_results_verify_citation(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let aref = hh_results::audit::AuditRef::from_json(req(params, "audit_ref")?)
            .ok_or_else(|| bad("/audit_ref", "decode failed"))?;
        let v = results(self)?.verify_citation(&self.store, &aref);
        Ok(Json::obj([("verdict", Json::str(v.as_str()))]))
    }

    /// `lab.results.export_rows{rows[], target, policy?}` — the §6.5
    /// export op: `measurement.export.delivered` lands on every cited
    /// run; foreign targets carry the `LoweringLossReport`.
    pub(crate) fn lab_results_export_rows(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let target = req_str(params, "target").and_then(|s| {
            hh_results::export::ExportTarget::parse(s)
                .ok_or_else(|| bad("/target", &format!("unknown export target {s}")))
        })?;
        let r = results(self)?;
        let mut rows = Vec::new();
        if let Json::Arr(selectors) = req(params, "rows")? {
            for s in selectors {
                let s = s.as_str().ok_or_else(|| bad("/rows", "type_mismatch"))?;
                rows.push(r.get_row(&self.store, s).map_err(refused)?.0);
            }
        }
        let policy = params.get("policy").cloned().unwrap_or(Json::Null);
        let out = r
            .export_rows(&mut self.store, &rows, target, &policy)
            .map_err(refused)?;
        Ok(Json::obj([
            ("artefact", Json::str(&out.artefact)),
            ("loss_report", out.loss_report.to_json()),
            (
                "delivered",
                Json::Arr(out.delivered.iter().map(Json::str).collect()),
            ),
            ("body", out.body),
        ]))
    }

    // ── lab.leaderboard.* ──────────────────────────────────────────────

    /// `lab.leaderboard.define{definition}` → `{definition_id}` —
    /// persists the canonical record and appends the name-history entry.
    pub(crate) fn lab_leaderboard_define(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let def = LeaderboardDefinition::from_json(req(params, "definition")?).map_err(refused)?;
        let id = results(self)?.define_leaderboard(&def).map_err(refused)?;
        Ok(Json::obj([
            ("definition_id", Json::str(&id)),
            (
                "history",
                Json::Arr(
                    def.name
                        .as_ref()
                        .map(|n| {
                            results(self)
                                .map(|r| r.leaderboard_definitions(n))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default()
                        .iter()
                        .map(Json::str)
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.leaderboard.leaderboard{definition|definition_ref, at?}` —
    /// the retained snapshot (`snapshot_id = view_hash`; idempotent).
    pub(crate) fn lab_leaderboard_leaderboard(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let r = results(self)?;
        let def = definition_of(&r, params)?;
        let docs = self.lab_docs()?;
        let snap = r
            .snapshot_leaderboard(&self.store, &docs, &def, at_of(params)?.as_ref())
            .map_err(refused)?;
        Ok(snap.to_json())
    }

    /// `lab.leaderboard.diff_snapshots{a, b}` — entries added, rank
    /// moves, flag changes, watermark delta (never `entries_removed`
    /// under annotate-never-hide).
    pub(crate) fn lab_leaderboard_diff_snapshots(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let a = req_str(params, "a")?;
        let b = req_str(params, "b")?;
        let d = results(self)?.diff_snapshots(a, b).map_err(refused)?;
        Ok(d.to_json())
    }

    /// `lab.leaderboard.publish{definition|definition_ref, at?,
    /// policy?}` — the export-as-publication op (loss report +
    /// `measurement.leaderboard.published` + `measurement.export.delivered`
    /// per cited run + retention pins).
    pub(crate) fn lab_leaderboard_publish(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let r = results(self)?;
        let def = definition_of(&r, params)?;
        let docs = self.lab_docs()?;
        let policy = params.get("policy").cloned().unwrap_or(Json::Null);
        let out = r
            .publish_leaderboard(
                &mut self.store,
                &docs,
                &def,
                at_of(params)?.as_ref(),
                &policy,
            )
            .map_err(refused)?;
        Ok(Json::obj([
            ("snapshot_id", Json::str(&out.snapshot_id)),
            ("view_hash", Json::str(&out.view_hash)),
            ("event_ref", Json::str(&out.event_ref)),
            (
                "pinned_addresses",
                Json::Arr(out.pinned_addresses.iter().map(Json::str).collect()),
            ),
        ]))
    }

    /// `lab.leaderboard.retract_entry{definition_ref, configuration_id,
    /// reason_ref, authority}` — the annotation (never a deletion).
    pub(crate) fn lab_leaderboard_retract_entry(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let definition_ref = req_str(params, "definition_ref")?;
        let configuration_id = req_str(params, "configuration_id")?;
        let reason_ref = req_str(params, "reason_ref")?;
        let authority = req_str(params, "authority")?;
        results(self)?
            .retract_entry(
                &mut self.store,
                definition_ref,
                configuration_id,
                reason_ref,
                authority,
            )
            .map_err(refused)
    }
}
