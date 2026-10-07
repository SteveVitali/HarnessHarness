//! Group L — the `lab.debt.*` manager-service dispatch (S6.1b; §5h.6
//! R-2.9.6; ADR-0197/0198): `manager_open`/`register`/`sweep`/`settle`/
//! `retire`/`propose` drive `hh_debt::manager::DebtManager` over the
//! caller-supplied registry run's `lifecycle.debt.*` book of record.
//! Records-in/records-out (AC-R-2.9.6-10 — there is no private verb):
//! `sweep`'s `entries[]` carry each debt record + caller-projected
//! observables + the resolved removal-test template; `retire` carries the
//! human-sealed `RetirementRecord`; `propose` carries the diff
//! projection. The manager authors `ExperimentSpec`s and proposals —
//! subject runs are never opened and diffs never applied here (D-2).
//!
//! `DebtManager`s persist across calls in `self.debt_managers`
//! (`run_id → DebtManager`); `open` re-folds the durable prefix (the
//! restart path — CC3, same convention as `lab.evolution.*`'s `ensure`).
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
fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

/// `DebtManagerError`/`LedgerError` → the boundary's typed surface —
/// `Refused{reason: <code>}` for the closed refusal table,
/// `WouldBlock`/`SchemaViolation` native (the `lab.evolution.*`
/// convention).
#[cfg(feature = "tier-c4")]
fn mgr_err(e: hh_debt::errors::DebtManagerError) -> EmbedError {
    use hh_debt::errors::DebtManagerError;
    match e {
        DebtManagerError::Store(hh_ledger::errors::LedgerError::WouldBlock { active_holder }) => {
            EmbedError::WouldBlock { active_holder }
        }
        DebtManagerError::Schema(d) => EmbedError::SchemaViolation {
            path: "/debt".to_string(),
            code: d,
        },
        other => EmbedError::Refused {
            reason: other.code(),
        },
    }
}

#[cfg(feature = "tier-c4")]
mod imp {
    use super::{bad, mgr_err, opt_str, req, req_str};
    use hh_debt::manager::DebtManager;
    use hh_debt::propose::ProposalDiff;
    use hh_debt::records::{DebtManagerRecord, SweepEntry};
    use hh_embed_schema::errors::EmbedError;
    use hh_lab::debt::DebtObservables;
    use hh_ontology::debt::{RemovalTestKind, Verdict};
    use hh_provenance::ProvenanceRecord;
    use hh_wire::json::Json;
    use std::collections::BTreeSet;

    /// The `DebtManager` for `run` — cached, or folded fresh from the
    /// durable prefix (restart equality is structural). The borrow is
    /// field-split (`managers` + `store` are disjoint) so the engine and
    /// the ledger are both live across the call.
    fn manager<'m>(
        managers: &'m mut std::collections::BTreeMap<String, DebtManager>,
        store: &hh_ledger::store::Store,
        run: &str,
    ) -> Result<&'m mut DebtManager, EmbedError> {
        if !managers.contains_key(run) {
            let mgr = DebtManager::open(
                store,
                &hh_hir::refs::RunRef {
                    run: run.to_string(),
                },
            )
            .map_err(mgr_err)?;
            managers.insert(run.to_string(), mgr);
        }
        managers
            .get_mut(run)
            .ok_or_else(|| bad("/run_id", "engine_absent"))
    }

    /// One `sweep` entry → `SweepEntry` (records-in: `{debt_ref, home?,
    /// version_id?, record, observables?, template?, used_by[]?}`).
    fn sweep_entry(j: &Json) -> Result<SweepEntry, EmbedError> {
        let Json::Obj(m) = j else {
            return Err(bad("/entries", "type_mismatch"));
        };
        let record = m
            .get("record")
            .ok_or_else(|| bad("/entries/record", "missing"))
            .and_then(crate::debt_ops::debt_record)?;
        Ok(SweepEntry {
            debt_ref: opt_str(j, "debt_ref").unwrap_or_else(|| format!("debt:{}", record.rule_id)),
            home: m.get("home").and_then(Json::as_int).map(|v| v as u8),
            version_id: opt_str(j, "version_id"),
            record,
            observables: m
                .get("observables")
                .map(debt_observables)
                .transpose()?
                .unwrap_or_default(),
            template: m
                .get("template")
                .map(|t| {
                    hh_lab::experiment::ExperimentSpec::from_json(t)
                        .map_err(|e| bad("/entries/template", &format!("{e:?}")))
                })
                .transpose()?,
            used_by: m
                .get("used_by")
                .and_then(|v| match v {
                    Json::Arr(a) => Some(a.clone()),
                    _ => None,
                })
                .map(|a| {
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// `observables` → `DebtObservables` — one decoder with
    /// `lab.debt.evaluate` (CC1/CC7; `debt_ops::observables` carries every
    /// member, `probation` included).
    fn debt_observables(j: &Json) -> Result<DebtObservables, EmbedError> {
        crate::debt_ops::observables(Some(j))
    }

    /// The `lab.debt.manager_*`/`lab.debt.{sweep,settle,retire,propose}`
    /// dispatch (tier-c4 build).
    pub(crate) fn dispatch(
        svc: &mut crate::service::EmbedService,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        match method {
            // `lab.debt.manager_open{run_id?}` → `{run_id}` — fold the
            // registry run's `lifecycle.debt.*` prefix into the service's
            // view; absent `run_id` binds the service's registry audit
            // run (the S5.4 `ensure_registry_run` convention — one book
            // of record per service).
            "lab.debt.manager_open" => {
                let run = match opt_str(params, "run_id") {
                    Some(r) => r,
                    None => svc.ensure_registry_run().map(|(r, _)| r)?,
                };
                let _ = manager(&mut svc.debt_managers, &svc.store, &run)?;
                Ok(Json::obj([("run_id", Json::str(&run))]))
            }
            // `lab.debt.register{run_id, record}` → `{manager_id,
            // reflexive_debt_ref}` — the service record mints
            // `lifecycle.debt.service.registered` (validated member-level:
            // maturity spelling + the home-16 reflexive record).
            "lab.debt.register" => {
                let run = req_str(params, "run_id")?.to_string();
                let record = DebtManagerRecord::from_json(req(params, "record")?)
                    .map_err(|e| bad("/record", &e.code()))?;
                let reflexive = record.reflexive_debt_ref();
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                mgr.register(&mut svc.store, &record).map_err(mgr_err)?;
                Ok(Json::obj([
                    ("manager_id", Json::str(&record.manager_id)),
                    ("reflexive_debt_ref", Json::str(&reflexive)),
                ]))
            }
            // `lab.debt.sweep{run_id, manager_id, entries[], now_ms?}` →
            // `{report}` — the standing monitor (probation + evaluate +
            // schedule + the reflexive verdict + `sweep.completed`).
            "lab.debt.sweep" => {
                let run = req_str(params, "run_id")?.to_string();
                let manager_id = req_str(params, "manager_id")?.to_string();
                let entries: Vec<SweepEntry> = match req(params, "entries")? {
                    Json::Arr(a) => a.iter().map(sweep_entry).collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(bad("/entries", "type_mismatch")),
                };
                let now_ms = params
                    .get("now_ms")
                    .and_then(Json::as_int)
                    .map(|v| v as u64)
                    .unwrap_or_else(|| svc.store.now_ms());
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                let report = mgr
                    .sweep(&mut svc.store, &manager_id, &entries, now_ms, None)
                    .map_err(mgr_err)?;
                Ok(sweep_report_json(&report))
            }
            // `lab.debt.settle{run_id, debt_ref, kind, report_ref,
            // verdict, reason?, now_ms?}` → `{verdict, transitions[]}`.
            "lab.debt.settle" => {
                let run = req_str(params, "run_id")?.to_string();
                let debt_ref = req_str(params, "debt_ref")?.to_string();
                let kind = RemovalTestKind::parse(req_str(params, "kind")?)
                    .ok_or_else(|| bad("/kind", "unknown_kind"))?;
                let report_ref = req_str(params, "report_ref")?.to_string();
                let verdict = Verdict::parse(req_str(params, "verdict")?)
                    .ok_or_else(|| bad("/verdict", "unknown_verdict"))?;
                let reason = opt_str(params, "reason");
                let now_ms = params
                    .get("now_ms")
                    .and_then(Json::as_int)
                    .map(|v| v as u64)
                    .unwrap_or_else(|| svc.store.now_ms());
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                let (v, transitions) = mgr
                    .settle(
                        &mut svc.store,
                        &debt_ref,
                        kind,
                        &report_ref,
                        verdict,
                        reason,
                        now_ms,
                    )
                    .map_err(mgr_err)?;
                Ok(Json::obj([
                    ("verdict", v.to_json()),
                    (
                        "transitions",
                        Json::Arr(transitions.iter().map(|t| t.to_json()).collect()),
                    ),
                ]))
            }
            // `lab.debt.retire{run_id, debt_ref, record}` → `{outcome}` —
            // the human-sealed gate (`hh_lab::debt::retire`; the record's
            // `decided_by` must carry `origin = human` provenance).
            "lab.debt.retire" => {
                let run = req_str(params, "run_id")?.to_string();
                let debt_ref = req_str(params, "debt_ref")?.to_string();
                let rec = req(params, "record")?;
                let Json::Obj(rm) = rec else {
                    return Err(bad("/record", "type_mismatch"));
                };
                let record = hh_hir::debt::RetirementRecord {
                    removal_test_report_ref: rm
                        .get("removal_test_report_ref")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("/record/removal_test_report_ref", "missing"))?
                        .to_string(),
                    verdict_ref: rm
                        .get("verdict_ref")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("/record/verdict_ref", "missing"))?
                        .to_string(),
                    decided_by: rm
                        .get("decided_by")
                        .ok_or_else(|| bad("/record/decided_by", "missing"))
                        .and_then(|p| {
                            ProvenanceRecord::from_json(p)
                                .map_err(|e| bad("/record/decided_by", &format!("{e:?}")))
                        })?,
                    rationale: rm
                        .get("rationale")
                        .ok_or_else(|| bad("/record/rationale", "missing"))
                        .and_then(|t| {
                            hh_hir::leaves::Text::from_json(t, "/record/rationale")
                                .map_err(|e| bad("/record/rationale", &format!("{e:?}")))
                        })?,
                };
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                let outcome = mgr
                    .retire(&mut svc.store, &debt_ref, &record)
                    .map_err(mgr_err)?;
                Ok(Json::obj([
                    ("debt_ref", Json::str(&outcome.debt_ref)),
                    ("supersedes_reason", Json::str(outcome.supersedes_reason)),
                    ("transition", outcome.transition.to_json()),
                ]))
            }
            // `lab.debt.post_import_sweep{run_id, manager_id,
            // snapshot_ref, covered[]?, scheduled[]?, entries[],
            // now_ms?}` → `{report}` — the S6.4 import-driven sweep
            // (§5h.8 §2.3): the Lab's `post_import_sweep` partition
            // minted into the book of record — covered →
            // `model_version_change` transitions; scheduled →
            // `removal_test.scheduled`; the `sweep.completed` row is
            // attributed `kind: post_import`, never cadence.
            "lab.debt.post_import_sweep" => {
                let run = req_str(params, "run_id")?.to_string();
                let manager_id = req_str(params, "manager_id")?.to_string();
                let snapshot_ref = req_str(params, "snapshot_ref")?.to_string();
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
                let sweep = hh_lab::coevolution::PostImportSweep {
                    covered: strs("covered"),
                    scheduled: strs("scheduled"),
                    unchanged: strs("unchanged"),
                };
                let entries: Vec<SweepEntry> = match req(params, "entries")? {
                    Json::Arr(a) => a.iter().map(sweep_entry).collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(bad("/entries", "type_mismatch")),
                };
                let now_ms = params
                    .get("now_ms")
                    .and_then(Json::as_int)
                    .map(|v| v as u64)
                    .unwrap_or_else(|| svc.store.now_ms());
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                let report = mgr
                    .post_import_sweep(
                        &mut svc.store,
                        &manager_id,
                        &snapshot_ref,
                        &sweep,
                        &entries,
                        now_ms,
                        None,
                    )
                    .map_err(mgr_err)?;
                Ok(sweep_report_json(&report))
            }
            // `lab.debt.propose{run_id, debt_ref, rule_id, diff{…},
            // proposed_by}` → `{proposal}` — the evolution-origin gate;
            // `state: proposed`, never deployed.
            "lab.debt.propose" => {
                let run = req_str(params, "run_id")?.to_string();
                let debt_ref = req_str(params, "debt_ref")?.to_string();
                let rule_id = req_str(params, "rule_id")?.to_string();
                let diff = req(params, "diff")?;
                let Json::Obj(dm) = diff else {
                    return Err(bad("/diff", "type_mismatch"));
                };
                let pdiff = ProposalDiff {
                    removed_rules: dm
                        .get("removed_rules")
                        .and_then(|v| match v {
                            Json::Arr(a) => Some(a.clone()),
                            _ => None,
                        })
                        .map(|a| {
                            a.iter()
                                .filter_map(Json::as_str)
                                .map(str::to_string)
                                .collect::<BTreeSet<String>>()
                        })
                        .unwrap_or_default(),
                    widens_authority: matches!(dm.get("widens_authority"), Some(Json::Bool(true))),
                    loosens_budget: matches!(dm.get("loosens_budget"), Some(Json::Bool(true))),
                    diff_ref: dm
                        .get("diff_ref")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                };
                let proposed_by = ProvenanceRecord::from_json(req(params, "proposed_by")?)
                    .map_err(|e| bad("/proposed_by", &format!("{e:?}")))?;
                let mgr = manager(&mut svc.debt_managers, &svc.store, &run)?;
                let proposal = mgr
                    .propose(&debt_ref, &rule_id, &pdiff, proposed_by)
                    .map_err(mgr_err)?;
                Ok(Json::obj([("proposal", proposal.to_json())]))
            }
            _ => Err(EmbedError::SchemaViolation {
                path: "/method".to_string(),
                code: format!("unknown debt-manager op {method}"),
            }),
        }
    }

    /// The `SweepReport` result member.
    fn sweep_report_json(r: &hh_debt::records::SweepReport) -> Json {
        Json::obj([
            ("sweep_seq", Json::Int(r.sweep_seq as i64)),
            ("now_ms", Json::Int(r.now_ms as i64)),
            ("evaluated", Json::Int(r.evaluated as i64)),
            (
                "transitions",
                Json::Arr(r.transitions.iter().map(|t| t.to_json()).collect()),
            ),
            (
                "probation_opened",
                Json::Arr(r.probation_opened.iter().map(Json::str).collect()),
            ),
            (
                "scheduled",
                Json::Arr(
                    r.scheduled
                        .iter()
                        .map(|t| {
                            let mut j = t.to_json();
                            if let Json::Obj(m) = &mut j {
                                m.insert("spec".into(), t.spec.to_json());
                            }
                            j
                        })
                        .collect(),
                ),
            ),
            (
                "deferred",
                Json::Arr(
                    r.deferred
                        .iter()
                        .map(|(d, r)| {
                            Json::obj([("debt_ref", Json::str(d)), ("reason", Json::str(r))])
                        })
                        .collect(),
                ),
            ),
            (
                "reflexive_verdict",
                r.reflexive_verdict
                    .as_ref()
                    .map(|v| v.to_json())
                    .unwrap_or(Json::Null),
            ),
        ])
    }
}

impl EmbedService {
    /// The `lab.debt.*` manager-service ops (tier-c4 build).
    #[cfg(feature = "tier-c4")]
    pub(crate) fn debt_manager_dispatch(
        &mut self,
        method: &str,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        imp::dispatch(self, method, params)
    }

    /// Tier absent — the ops remain in the schema (CC7) and answer the
    /// typed `tier_unavailable` refusal (CC6 removability).
    #[cfg(not(feature = "tier-c4"))]
    pub(crate) fn debt_manager_dispatch(
        &mut self,
        _method: &str,
        _params: &Json,
    ) -> Result<Json, EmbedError> {
        Err(EmbedError::Unsupported {
            by: "tier-c4".to_string(),
        })
    }
}
