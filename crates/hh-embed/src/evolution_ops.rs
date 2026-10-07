//! Group L — `lab.evolution.*` dispatch (S6.1a; §05h R-2.9.5). The
//! boundary is records-in/records-out like `lab.experiment.*`: the caller
//! supplies the campaign spec, the proposal + base `HirDocument`, the
//! stage evidence reports, and the human endorsement — the campaign
//! engine (`hh-evolution::EvolutionCampaign`) folds every decision from
//! the durable prefix and mints `measurement.evolution.*` kernel rows on
//! the campaign's own `run_kind = experiment` run.
//!
//! Campaign engines persist across calls in `self.evolution_campaigns`
//! (`run_id → EvolutionCampaign`); the fence still lives in the ledger —
//! `ensure` rebuilds the fold and re-acquires the writer (RC-8's
//! restart path, same convention as `fleet.*`).
//!
//! A `--no-default-features` build answers `Unsupported{by: "tier-c4"}`
//! — the ops stay in the schema (CC7) and the tier is removable (CC6).

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
mod imp {
    use super::{bad, req, req_str};
    use hh_embed_schema::errors::EmbedError;
    use hh_evolution::campaign::{doc_kind, EvolutionCampaign};
    use hh_evolution::errors::EvolutionError;
    use hh_evolution::records::{
        CandidateProposal, EvolutionAcceptanceReport, EvolutionCampaignSpec, FailureHypothesis,
        ScreenReport, SecurityInvarianceReport, TransferRow,
    };
    use hh_hir::document::HirDocument;
    use hh_ledger::store::Store;
    use hh_provenance::ProvenanceRecord;
    use hh_wire::json::Json;
    use std::collections::BTreeMap;

    /// The writer-lease holder evolution campaigns take (`hh-embed`'s
    /// pipeline facade — the campaign's own spec names the principal).
    const EVO_HOLDER: &str = "hh-embed:evolution";
    /// The writer TTL (the `fleet.*` convention).
    const WRITER_TTL_MS: u64 = 60_000;

    /// `EvolutionError` → the boundary's typed surface — `Refusal` codes
    /// render verbatim as `Refused{reason: <code>}` (the closed refusal
    /// table; T-LCD-14); store `WouldBlock` maps native; schema/docs
    /// failures surface as `SchemaViolation`.
    pub(crate) fn evo_err(e: EvolutionError) -> EmbedError {
        match e {
            EvolutionError::Store(hh_ledger::errors::LedgerError::WouldBlock { active_holder }) => {
                EmbedError::WouldBlock { active_holder }
            }
            EvolutionError::Schema(d) => EmbedError::SchemaViolation {
                path: "/evolution".to_string(),
                code: d,
            },
            other => EmbedError::Refused {
                reason: other.code(),
            },
        }
    }

    /// The campaign engine for `run` — cached engine or `ensure` from the
    /// durable prefix (the writer lease re-acquires under the fence).
    fn campaign<'m>(
        engines: &'m mut BTreeMap<String, EvolutionCampaign>,
        store: &mut Store,
        docs: hh_experiment::docs::LabDocs,
        run: &str,
    ) -> Result<&'m mut EvolutionCampaign, EmbedError> {
        if !engines.contains_key(run) {
            let eng = EvolutionCampaign::ensure(store, docs, run, EVO_HOLDER, WRITER_TTL_MS)
                .map_err(evo_err)?;
            engines.insert(run.to_string(), eng);
        }
        engines
            .get_mut(run)
            .ok_or_else(|| bad("/run", "engine_absent"))
    }

    /// Decode `params.candidate_id` + resolve the campaign.
    fn cid(params: &Json) -> Result<String, EmbedError> {
        Ok(req_str(params, "candidate_id")?.to_string())
    }

    /// `params.retention`/`params.vetoes` — the S5 supplementary
    /// records-in evidence.
    fn vetoes(params: &Json) -> Result<BTreeMap<String, bool>, EmbedError> {
        let mut out = BTreeMap::new();
        if let Some(Json::Obj(m)) = params.get("vetoes") {
            for (k, v) in m {
                let b = match v {
                    Json::Bool(b) => *b,
                    _ => return Err(bad("/vetoes", "type_mismatch")),
                };
                out.insert(k.clone(), b);
            }
        }
        Ok(out)
    }

    /// `params.seal`/`params.endorsement` → `ProvenanceRecord` (the
    /// human-origin seal the engine checks).
    fn provenance(params: &Json, key: &str) -> Result<ProvenanceRecord, EmbedError> {
        ProvenanceRecord::from_json(req(params, key)?)
            .map_err(|e| bad(&format!("/{key}"), &format!("{e:?}")))
    }

    /// The `lab.evolution.*` dispatch (tier-c4 build).
    pub(crate) fn dispatch(
        svc: &mut crate::service::EmbedService,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        match method {
            "lab.evolution.campaign_open" => {
                let spec = EvolutionCampaignSpec::from_json(req(params, "spec")?)
                    .map_err(|e| bad("/spec", &format!("{e:?}")))?;
                let docs = svc.lab_docs()?;
                let (run_id, eng) =
                    EvolutionCampaign::open(&mut svc.store, docs, EVO_HOLDER, WRITER_TTL_MS, spec)
                        .map_err(evo_err)?;
                svc.evolution_campaigns.insert(run_id.clone(), eng);
                Ok(Json::obj([("run_id", Json::str(&run_id))]))
            }
            "lab.evolution.campaign_ensure" => {
                let run = req_str(params, "run")?.to_string();
                let docs = svc.lab_docs()?;
                campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                Ok(Json::obj([("run_id", Json::str(&run))]))
            }
            "lab.evolution.propose" => {
                let run = req_str(params, "run")?.to_string();
                let proposal = CandidateProposal::from_json(req(params, "proposal")?)
                    .map_err(|e| bad("/proposal", &format!("{e:?}")))?;
                let base_doc = hh_hir::wire::document_from_json(req(params, "base_doc")?)
                    .map_err(|e| bad("/base_doc", &format!("{e:?}")))?;
                let _doc: &HirDocument = &base_doc;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let candidate_id = eng
                    .propose(&mut svc.store, &proposal, &base_doc)
                    .map_err(evo_err)?;
                Ok(Json::obj([("candidate_id", Json::str(&candidate_id))]))
            }
            "lab.evolution.hypothesize" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let hyp = FailureHypothesis::from_json(req(params, "hypothesis")?)
                    .map_err(|e| bad("/hypothesis", &format!("{e:?}")))?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let href = eng
                    .hypothesize(&mut svc.store, &candidate_id, &hyp)
                    .map_err(evo_err)?;
                Ok(Json::obj([("hypothesis_ref", Json::str(&href))]))
            }
            "lab.evolution.screen" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let report = ScreenReport::from_json(req(params, "report")?)
                    .map_err(|e| bad("/report", &format!("{e:?}")))?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let rref = eng
                    .screen(&mut svc.store, &candidate_id, &report)
                    .map_err(evo_err)?;
                Ok(Json::obj([("report_ref", Json::str(&rref))]))
            }
            "lab.evolution.matched_eval" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let experiment_id = req_str(params, "experiment_id")?.to_string();
                let report_ref = req_str(params, "report_ref")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.matched_eval(&mut svc.store, &candidate_id, &experiment_id, &report_ref)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("searched"))]))
            }
            "lab.evolution.held_out_eval" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let experiment_id = req_str(params, "experiment_id")?.to_string();
                let report_ref = req_str(params, "report_ref")?.to_string();
                let retention = req(params, "retention")?.clone();
                let veto = vetoes(params)?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.held_out_eval(
                    &mut svc.store,
                    &candidate_id,
                    &experiment_id,
                    &report_ref,
                    &retention,
                    &veto,
                )
                .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("validated"))]))
            }
            "lab.evolution.transfer" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let rows = match req(params, "rows")? {
                    Json::Arr(a) => a
                        .iter()
                        .map(TransferRow::from_json)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| bad("/rows", &format!("{e:?}")))?,
                    _ => return Err(bad("/rows", "type_mismatch")),
                };
                let compatibility = match params.get("compatibility") {
                    Some(Json::Arr(a)) => a.clone(),
                    _ => vec![],
                };
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let rref = eng
                    .transfer(&mut svc.store, &candidate_id, &rows, &compatibility)
                    .map_err(evo_err)?;
                Ok(Json::obj([("report_ref", Json::str(&rref))]))
            }
            "lab.evolution.security_check" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let report = SecurityInvarianceReport::from_json(req(params, "report")?)
                    .map_err(|e| bad("/report", &format!("{e:?}")))?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let rref = eng
                    .security_check(&mut svc.store, &candidate_id, &report)
                    .map_err(evo_err)?;
                Ok(Json::obj([("report_ref", Json::str(&rref))]))
            }
            "lab.evolution.seal" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let report = EvolutionAcceptanceReport::from_json(req(params, "report")?)
                    .map_err(|e| bad("/report", &format!("{e:?}")))?;
                let endorsement = provenance(params, "endorsement")?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let rref = eng
                    .seal(&mut svc.store, &candidate_id, &report, &endorsement)
                    .map_err(evo_err)?;
                Ok(Json::obj([("report_ref", Json::str(&rref))]))
            }
            "lab.evolution.canary" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let mode = req_str(params, "mode")?.to_string();
                let intervention_ref = req_str(params, "intervention_ref")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.canary(&mut svc.store, &candidate_id, &mode, &intervention_ref)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("canary"))]))
            }
            "lab.evolution.canary_settle" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let status = req_str(params, "status")?.to_string();
                let reason = params
                    .get("reason")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let conditioned_debts = match params.get("conditioned_debts") {
                    Some(Json::Arr(a)) => a.clone(),
                    _ => vec![],
                };
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.canary_settle(
                    &mut svc.store,
                    &candidate_id,
                    &status,
                    &reason,
                    &conditioned_debts,
                )
                .map_err(evo_err)?;
                Ok(Json::obj([("status", Json::str(&status))]))
            }
            "lab.evolution.expire" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let trigger = req_str(params, "trigger")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.expire(&mut svc.store, &candidate_id, &trigger)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("expiring"))]))
            }
            "lab.evolution.retire" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let experiment_id = req_str(params, "experiment_id")?.to_string();
                let report_ref = req_str(params, "report_ref")?.to_string();
                let seal = provenance(params, "seal")?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.retire(
                    &mut svc.store,
                    &candidate_id,
                    &experiment_id,
                    &report_ref,
                    &seal,
                )
                .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("retired"))]))
            }
            "lab.evolution.revalidate" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let evidence_ref = req_str(params, "evidence_ref")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.revalidate(&mut svc.store, &candidate_id, &evidence_ref)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("revalidated"))]))
            }
            "lab.evolution.reactivate" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.reactivate(&mut svc.store, &candidate_id)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("active"))]))
            }
            "lab.evolution.rebase" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let proposal = CandidateProposal::from_json(req(params, "proposal")?)
                    .map_err(|e| bad("/proposal", &format!("{e:?}")))?;
                let base_doc = hh_hir::wire::document_from_json(req(params, "base_doc")?)
                    .map_err(|e| bad("/base_doc", &format!("{e:?}")))?;
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.rebase(&mut svc.store, &candidate_id, &proposal, &base_doc)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("rebased"))]))
            }
            "lab.evolution.withdraw" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let by = req_str(params, "by")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.withdraw(&mut svc.store, &candidate_id, &by)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("withdrawn"))]))
            }
            "lab.evolution.revert" => {
                let run = req_str(params, "run")?.to_string();
                let candidate_id = cid(params)?;
                let reason = req_str(params, "reason")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.revert(&mut svc.store, &candidate_id, &reason)
                    .map_err(evo_err)?;
                Ok(Json::obj([("state", Json::str("reverted"))]))
            }
            "lab.evolution.stop" => {
                let run = req_str(params, "run")?.to_string();
                let reason = req_str(params, "reason")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.stop(&mut svc.store, &reason).map_err(evo_err)?;
                Ok(Json::obj([("status", Json::str(&eng.view.status))]))
            }
            "lab.evolution.close" => {
                let run = req_str(params, "run")?.to_string();
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                eng.close(&mut svc.store).map_err(evo_err)?;
                Ok(Json::obj([("status", Json::str(&eng.view.status))]))
            }
            "lab.evolution.view" => {
                let run = req_str(params, "run")?.to_string();
                let until_seq = params
                    .get("until_seq")
                    .and_then(Json::as_int)
                    .map(|v| v as u64);
                let docs = svc.lab_docs()?;
                let eng = campaign(&mut svc.evolution_campaigns, &mut svc.store, docs, &run)?;
                let view = eng.candidate_view(&svc.store, until_seq).map_err(evo_err)?;
                let mut candidates = BTreeMap::new();
                for (cid, rec) in &view.candidates {
                    candidates.insert(cid.clone(), rec.to_json());
                }
                Ok(Json::obj([
                    ("status", Json::str(&view.status)),
                    ("candidates", Json::Obj(candidates)),
                    (
                        "n_registered",
                        Json::Int(view.n_candidates_registered() as i64),
                    ),
                ]))
            }
            other => Err(EmbedError::Refused {
                reason: format!("unknown_evolution_op:{other}"),
            }),
        }
    }

    /// `LabDocs` — kept for parity with the engine's deposit kinds.
    #[allow(dead_code)]
    fn _doc_kinds() -> [&'static str; 5] {
        [
            doc_kind::CAMPAIGN_SPEC,
            doc_kind::PROPOSAL,
            doc_kind::HYPOTHESIS,
            doc_kind::REPORT,
            doc_kind::SPLIT,
        ]
    }
}

impl EmbedService {
    /// The `lab.evolution.*` dispatch (S6.1a) — one arm for the whole
    /// surface like `fleet_dispatch`.
    #[cfg(feature = "tier-c4")]
    pub(crate) fn evolution_dispatch(
        &mut self,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        imp::dispatch(self, method, params)
    }

    /// Tier absent — the ops remain in the schema (CC7) and answer the
    /// typed `tier_unavailable` refusal (CC6 removability).
    #[cfg(not(feature = "tier-c4"))]
    pub(crate) fn evolution_dispatch(
        &mut self,
        _method: &str,
        _params: &Json,
    ) -> Result<Json, EmbedError> {
        Err(EmbedError::Unsupported {
            by: "tier-c4".to_string(),
        })
    }
}
