//! Group L — the S5.4 debt + model-compat + attribution-design ops
//! (R-2.9.6¹/R-2.9.7¹/R-2.9.8¹). Records-in/records-out like the rest of
//! Group L: the caller supplies the `AssumptionDebtRecord`/`DebtIndexRow`/
//! observables documents; the ops fold them through `hh_lab::debt` /
//! `hh_lab::model` / `hh_analysis::ops` — the manager is out-of-process by
//! construction (AC-R-2.9.6-10: `DebtIndexRow`, `RemovalVerdict`,
//! `DebtReport` documents move through `hh-embed/1`, no private verb).

use hh_ontology::debt::DebtPolicy;
use hh_wire::json::Json;

use crate::eval_ops::{bad, opt_str, req};
use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

/// Decode a `DebtObservables` doc (`{expiry{…}, profile_change?,
/// evidence_superseded[]?, revalidation_grade?}` — every member optional;
/// `{}` is the empty observable set).
pub(crate) fn observables(j: Option<&Json>) -> Result<hh_lab::debt::DebtObservables, EmbedError> {
    let mut obs = hh_lab::debt::DebtObservables::default();
    let Some(j) = j else { return Ok(obs) };
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(bad("/observables", "type_mismatch")),
    };
    if let Some(e) = m.get("expiry") {
        let em = match e {
            Json::Obj(em) => em,
            _ => return Err(bad("/observables/expiry", "type_mismatch")),
        };
        let bool_of = |k: &str| {
            em.get(k)
                .and_then(|v| matches!(v, Json::Bool(b) if *b).then_some(()))
                .is_some()
        };
        obs.expiry.served_model_mismatch = bool_of("served_model_mismatch");
        obs.expiry.compatibility_token_changed = bool_of("compatibility_token_changed");
        obs.expiry.fingerprint_drift = bool_of("fingerprint_drift");
        obs.expiry.unresolved_beyond_grace = bool_of("unresolved_beyond_grace");
        obs.expiry.retirement_at_passed = bool_of("retirement_at_passed");
        if let Some(Json::Arr(recs)) = em.get("probe_records") {
            for r in recs {
                let cap = r.get("capability").and_then(Json::as_str);
                let v = r.get("verdict").and_then(Json::as_str);
                let (Some(cap), Some(v)) = (cap, v) else {
                    return Err(bad("/observables/expiry/probe_records", "type_mismatch"));
                };
                let verdict = match v {
                    "SUPPORTED" => hh_compiler::profile_test::ConformanceVerdict::Supported,
                    "UNSUPPORTED" => hh_compiler::profile_test::ConformanceVerdict::Unsupported,
                    "PARTIAL" => hh_compiler::profile_test::ConformanceVerdict::Partial,
                    "NOT_APPLICABLE" => {
                        hh_compiler::profile_test::ConformanceVerdict::NotApplicable
                    }
                    "UNKNOWN" => hh_compiler::profile_test::ConformanceVerdict::Unknown,
                    "SKIPPED" => hh_compiler::profile_test::ConformanceVerdict::Skipped,
                    "DRIFT" => hh_compiler::profile_test::ConformanceVerdict::Drift,
                    _ => {
                        return Err(bad(
                            "/observables/expiry/probe_records/verdict",
                            "unknown_conformance_verdict",
                        ))
                    }
                };
                obs.expiry.probe_records.push((cap.to_string(), verdict));
            }
        }
        if let Some(v) = em.get("max_evidence_age_ms").and_then(Json::as_int) {
            obs.expiry.max_evidence_age_ms = Some(v as u64);
        }
        if let Some(v) = em.get("experiment_non_inferior") {
            obs.expiry.experiment_non_inferior = match v {
                Json::Bool(b) => Some(*b),
                _ => None,
            };
        }
        if let Some(Json::Obj(rv)) = em.get("revalidation") {
            let eref = rv
                .get("evidence_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("/observables/expiry/revalidation", "missing evidence_ref"))?;
            let probe_passed = matches!(rv.get("probe_passed"), Some(Json::Bool(true)));
            obs.expiry.revalidation = Some(hh_compiler::expiry::RevalidationObs {
                evidence_ref: eref.to_string(),
                probe_passed,
            });
        }
        if let Some(Json::Arr(rules)) = em.get("regression_drifted_rules") {
            obs.expiry.regression_drifted_rules = rules
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect();
        }
    }
    if let Some(Json::Obj(pc)) = m.get("profile_change") {
        let kind = pc
            .get("kind")
            .and_then(Json::as_str)
            .and_then(hh_lab::debt::ProfileChangeKind::parse)
            .ok_or_else(|| bad("/observables/profile_change/kind", "unknown_profile_change"))?;
        obs.profile_change = Some(hh_lab::debt::ProfileChange {
            kind,
            grace_elapsed: matches!(pc.get("grace_elapsed"), Some(Json::Bool(true))),
            changed_at_ms: pc
                .get("changed_at_ms")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            profile_ref: pc
                .get("profile_ref")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    if let Some(Json::Arr(refs)) = m.get("evidence_superseded") {
        obs.evidence_superseded = refs
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect();
    }
    if let Some(g) = m.get("revalidation_grade").and_then(Json::as_str) {
        obs.revalidation_grade = match g {
            "hypothesized" => Some(hh_ontology::debt::EvidenceGrade::Hypothesized),
            "evidenced" => Some(hh_ontology::debt::EvidenceGrade::Evidenced),
            "confirmed" => Some(hh_ontology::debt::EvidenceGrade::Confirmed),
            _ => return Err(bad("/observables/revalidation_grade", "unknown_grade")),
        };
    }
    // S6.1b (DF-S5.4-1): `probation` — an already-ledgered probation entry
    // the caller passes through when the evaluation runs outside the
    // manager's fold (`lab.debt.evaluate`'s records-in path; the
    // `lab.debt.sweep` fold derives this member itself).
    if let Some(Json::Obj(pm)) = m.get("probation") {
        let u64_at = |k: &str| {
            pm.get(k)
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
                .unwrap_or(0)
        };
        obs.probation = Some(hh_lab::debt::LedgeredProbation {
            debt_ref: pm
                .get("debt_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            opened_at_ms: u64_at("opened_at_ms"),
            due_at_ms: u64_at("due_at_ms"),
            source_ref: pm
                .get("source_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(obs)
}

/// Decode a `DebtPolicy` doc (`{}` → the proposed defaults).
fn policy_of(j: Option<&Json>) -> Result<DebtPolicy, EmbedError> {
    match j {
        None | Some(Json::Null) => Ok(DebtPolicy::default()),
        Some(v) => DebtPolicy::from_json(v, "/policy").map_err(|e| bad("/policy", &e.detail)),
    }
}

/// Decode an `AssumptionDebtRecord` member (`hh_hir::debt_from_json` —
/// the one codec, CC1).
pub(crate) fn debt_record(j: &Json) -> Result<hh_hir::records::AssumptionDebtRecord, EmbedError> {
    hh_hir::debt_from_json(j, "/record").map_err(|e| bad("/record", &format!("{e:?}")))
}

/// Decode a `DebtIndexRow` member.
fn index_row(j: &Json) -> Result<hh_lab::debt::DebtIndexRow, EmbedError> {
    hh_lab::debt::DebtIndexRow::from_json(j).map_err(|e| bad("/rows", &format!("{e:?}")))
}

/// Decode a `DebtTransition` doc (`{debt_ref, from, to, trigger,
/// evidence_ref?, causes[]?}`).
fn transition(j: &Json) -> Result<hh_lab::debt::DebtTransition, EmbedError> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(bad("/transitions", "type_mismatch")),
    };
    let status = |k: &str| -> Result<hh_ontology::debt::DebtStatus, EmbedError> {
        let s = m
            .get(k)
            .and_then(Json::as_str)
            .ok_or_else(|| bad("/transitions", "missing from/to"))?;
        hh_ontology::debt::DebtStatus::parse(s)
            .ok_or_else(|| bad("/transitions", "unknown debt status"))
    };
    Ok(hh_lab::debt::DebtTransition {
        debt_ref: m
            .get("debt_ref")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        from: status("from")?,
        to: status("to")?,
        trigger: m
            .get("trigger")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        evidence_ref: m
            .get("evidence_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
        causes: match m.get("causes") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        },
    })
}

impl EmbedService {
    /// `lab.debt.evaluate{record, debt_ref?, home?, observables?, now_ms?,
    /// policy?}` → `{transitions[]}` — the live all-home trigger evaluation
    /// (§5h.6 §2; the caller supplies the registry_snapshot/
    /// ledger_watermark projections as `observables`).
    pub(crate) fn lab_debt_evaluate(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let record = debt_record(req(params, "record")?)?;
        let obs = observables(params.get("observables"))?;
        let policy = policy_of(params.get("policy"))?;
        let now_ms = params
            .get("now_ms")
            .and_then(Json::as_int)
            .map(|v| v as u64)
            .unwrap_or_else(|| self.store.now_ms());
        let home_id = params.get("home").and_then(Json::as_int).map(|v| v as u8);
        let debt_ref =
            opt_str(params, "debt_ref").unwrap_or_else(|| format!("debt:{}", record.rule_id));
        let transitions =
            hh_lab::debt::evaluate_debt(&debt_ref, &record, home_id, &obs, now_ms, &policy);
        Ok(Json::obj([(
            "transitions",
            Json::Arr(transitions.iter().map(|t| t.to_json()).collect()),
        )]))
    }

    /// `lab.debt.index{entries[]}` → `{rows[]}` — the `DebtIndex` fold
    /// (§5h.6 §3): each entry is `{home?, version_id?, record,
    /// transitions[]?, removal_test?, used_by[]?, expired_used_runs?}`;
    /// rebuild equality — the same entries produce the same rows.
    pub(crate) fn lab_debt_index(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let entries = match req(params, "entries")? {
            Json::Arr(a) => a,
            _ => return Err(bad("/entries", "type_mismatch")),
        };
        let mut input = Vec::with_capacity(entries.len());
        for e in entries {
            let em = match e {
                Json::Obj(em) => em,
                _ => return Err(bad("/entries", "type_mismatch")),
            };
            let record = debt_record(
                em.get("record")
                    .ok_or_else(|| bad("/entries/record", "missing"))?,
            )?;
            let transitions = match em.get("transitions") {
                Some(Json::Arr(ts)) => ts.iter().map(transition).collect::<Result<Vec<_>, _>>()?,
                _ => Vec::new(),
            };
            let removal_test_state = match em.get("removal_test") {
                Some(v) => Some(
                    hh_lab::debt::RemovalTestState::from_json(v)
                        .map_err(|e| bad("/entries/removal_test", &format!("{e:?}")))?,
                ),
                None => None,
            };
            input.push(hh_lab::debt::DebtIndexEntry {
                home: em.get("home").and_then(Json::as_int).map(|v| v as u8),
                version_id: em
                    .get("version_id")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                record,
                transitions,
                removal_test_state,
                used_by: match em.get("used_by") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                },
                expired_used_runs: em
                    .get("expired_used_runs")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
            });
        }
        let rows = hh_lab::debt::debt_index(&input);
        Ok(Json::obj([(
            "rows",
            Json::Arr(rows.iter().map(|r| r.to_json()).collect()),
        )]))
    }

    /// `lab.debt.report{rows[], now_ms?, policy?, scope?}` → `{report,
    /// routed_notices}` — the periodic health view + notice routing
    /// (§5h.6 §5–§6): every notice lands under its owner's declared sinks
    /// intersected with the policy's `notice_sinks`; an unreachable owner
    /// lands under `unrouted`, never silently dropped.
    pub(crate) fn lab_debt_report(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let rows: Vec<hh_lab::debt::DebtIndexRow> = match req(params, "rows")? {
            Json::Arr(a) => a.iter().map(index_row).collect::<Result<Vec<_>, _>>()?,
            _ => return Err(bad("/rows", "type_mismatch")),
        };
        let policy = policy_of(params.get("policy"))?;
        let now_ms = params
            .get("now_ms")
            .and_then(Json::as_int)
            .map(|v| v as u64)
            .unwrap_or_else(|| self.store.now_ms());
        let health = hh_lab::debt::AssumptionDebtHealth {
            warn_within_ms: policy.warn_within_ms,
        };
        let mut report = health.report(&rows, now_ms);
        report.priority = vec![
            "expired_used".into(),
            "expiring".into(),
            "open".into(),
            "not_yet_testable".into(),
        ];
        report.scope = params.get("scope").cloned();
        let notices = health.notices(&rows, &report);
        let routed = hh_lab::debt::route_notices(&notices, &policy);
        Ok(Json::obj([
            ("report", report.to_json()),
            (
                "routed_notices",
                Json::Obj(routed.into_iter().map(|(k, v)| (k, Json::Arr(v))).collect()),
            ),
        ]))
    }

    /// `lab.model.snapshot_claim{provider, model_id, drift_ref}` →
    /// `{claim}` — the synthetic `SnapshotClaim` a fingerprint `DRIFT`
    /// record mints (AC-R-2.9.8-10; the provider-drift path runs with no
    /// trainer present).
    pub(crate) fn lab_model_snapshot_claim(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let provider = req(params, "provider")?
            .as_str()
            .ok_or_else(|| bad("/provider", "type_mismatch"))?;
        let model_id = req(params, "model_id")?
            .as_str()
            .ok_or_else(|| bad("/model_id", "type_mismatch"))?;
        let drift_ref = req(params, "drift_ref")?
            .as_str()
            .ok_or_else(|| bad("/drift_ref", "type_mismatch"))?;
        let claim = hh_lab::model::synthetic_snapshot_claim(provider, model_id, drift_ref);
        Ok(Json::obj([("claim", claim.to_json())]))
    }

    /// `lab.model.regression{suite{suite_id, checks[]}, results[],
    /// evidence_ref}` → `{status}` — the `run_regression_suite` fold
    /// (§5h.8; `results[]` rows are `{rule_id, verdict}` with the closed
    /// `pass|drift|fail|unsupported` spellings).
    pub(crate) fn lab_model_regression(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let suite_j = req(params, "suite")?;
        let suite_id = suite_j
            .get("suite_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("/suite/suite_id", "missing"))?;
        let checks: Vec<hh_lab::model::RegressionCheck> = match suite_j.get("checks") {
            Some(Json::Arr(cs)) => cs
                .iter()
                .map(|c| {
                    let rule_id = c
                        .get("rule_id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("/suite/checks", "missing rule_id"))?;
                    Ok(hh_lab::model::RegressionCheck {
                        rule_id: rule_id.to_string(),
                        capability: c
                            .get("capability")
                            .and_then(Json::as_str)
                            .map(str::to_string),
                    })
                })
                .collect::<Result<_, EmbedError>>()?,
            _ => Vec::new(),
        };
        // The suite's provenance rides the record the caller deposits —
        // the op mints the check-list view only (the claim boundary is
        // the `CompatibilityRecord`).
        let prov =
            hh_provenance::ProvenanceRecord::kernel("hh-embed.lab.model", self.store.now_ms());
        let suite = hh_lab::model::HarnessRegressionSuite {
            suite_id: suite_id.to_string(),
            checks,
            provenance: prov,
        };
        let results: Vec<(String, hh_lab::model::RegressionVerdict)> = match req(params, "results")?
        {
            Json::Arr(rs) => rs
                .iter()
                .map(|r| {
                    let rule_id = r
                        .get("rule_id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("/results", "missing rule_id"))?;
                    let v = r
                        .get("verdict")
                        .and_then(Json::as_str)
                        .and_then(hh_lab::model::RegressionVerdict::parse)
                        .ok_or_else(|| bad("/results/verdict", "unknown_verdict"))?;
                    Ok((rule_id.to_string(), v))
                })
                .collect::<Result<_, EmbedError>>()?,
            _ => return Err(bad("/results", "type_mismatch")),
        };
        let evidence_ref = req(params, "evidence_ref")?
            .as_str()
            .ok_or_else(|| bad("/evidence_ref", "type_mismatch"))?;
        let status = hh_lab::model::run_regression_suite(&suite, &results, evidence_ref);
        Ok(Json::obj([
            ("status", status.to_json()),
            (
                "regression_drifted_rules",
                Json::Arr(
                    hh_lab::model::regression_drifted_rules(&status)
                        .iter()
                        .map(Json::str)
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.analysis.component_targets{definition, filter?}` →
    /// `{targets[]}` — the typed `ComponentTarget` enumeration
    /// (§5h.7; R-2.9.7¹).
    pub(crate) fn lab_analysis_component_targets(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let definition = req(params, "definition")?;
        let targets = hh_analysis::ops::component_targets(definition, params.get("filter"));
        Ok(Json::obj([("targets", Json::Arr(targets))]))
    }

    /// `lab.analysis.attribution_design{design_ref, targets[], arms[],
    /// match_spec_ref?, budget_allocation?}` → `{design}` — the
    /// `AttributionDesign` document the M1 kernel consumes.
    pub(crate) fn lab_analysis_attribution_design(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let design_ref = req(params, "design_ref")?
            .as_str()
            .ok_or_else(|| bad("/design_ref", "type_mismatch"))?;
        let targets = match req(params, "targets")? {
            Json::Arr(t) => t.clone(),
            _ => return Err(bad("/targets", "type_mismatch")),
        };
        let arms: Vec<String> = match req(params, "arms")? {
            Json::Arr(a) => a
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| bad("/arms", "type_mismatch"))
                })
                .collect::<Result<_, _>>()?,
            _ => return Err(bad("/arms", "type_mismatch")),
        };
        let design = hh_analysis::ops::attribution_design(
            design_ref,
            targets,
            &arms,
            params.get("match_spec_ref").and_then(Json::as_str),
            params.get("budget_allocation").cloned(),
        );
        Ok(Json::obj([("design", design)]))
    }
}

// ── Live trigger wiring (§5h.6 §2's "live trigger evaluation") ──────────────
//
// A registry lifecycle event on a `DebtHomes/1`-listed record kind
// auto-evaluates the record's debt member: a superseding publish fires
// `profile_change{kind: superseded_l2}`, a deprecate/yank fires
// `profile_change{kind: retired}` (grace handled by the caller's
// re-evaluation — `grace_elapsed` is the observable row). Each emitted
// `DebtTransition` lands as a `lifecycle.debt.status.changed` row on the
// registry audit run (the registered class — the ledger carries the
// transition, the manager carries no authority handle).

use hh_registry::events::RegistryEvent;

impl EmbedService {
    /// Evaluate the debt member of a registered record under a
    /// `profile_change` observable — the lifecycle-wiring half of S5.4.
    /// Returns the emitted transitions (`[]` when the record is not a
    /// debt-home record or carries no debt member — never a refusal).
    pub(crate) fn evaluate_debt_on_lifecycle(
        &mut self,
        version_id: &str,
        change: hh_lab::debt::ProfileChangeKind,
    ) -> Result<Vec<hh_lab::debt::DebtTransition>, EmbedError> {
        let Some((envelope, record)) = self.registry.get(version_id) else {
            return Ok(Vec::new());
        };
        let kind = envelope.kind.as_str();
        let Some(home) = hh_ontology::debt::DEBT_HOMES
            .iter()
            .find(|h| h.record_kind == kind)
        else {
            return Ok(Vec::new());
        };
        let body = hh_registry::schema::body_json(record, true);
        let Some(member) = body.get(home.field) else {
            return Ok(Vec::new());
        };
        let debt = hh_hir::debt_from_json(member, &format!("/{kind}.{}", home.field))
            .map_err(|e| bad("/debt", &format!("{e:?}")))?;
        let now_ms = self.store.now_ms();
        let obs = hh_lab::debt::DebtObservables {
            profile_change: Some(hh_lab::debt::ProfileChange {
                kind: change,
                grace_elapsed: false,
                // Stamp the change time — a later evaluation under the
                // operative policy derives the grace leg itself
                // (`changed_at_ms + grace_period_ms <= now`), so the
                // `retired`-grace hard transition is a policy fact, never
                // a caller-supplied flag alone (R2.17).
                changed_at_ms: Some(now_ms),
                profile_ref: version_id.to_string(),
            }),
            ..Default::default()
        };
        let policy = DebtPolicy::default();
        let debt_ref = format!("debt:{}:{version_id}:{}", home.id, debt.rule_id);
        let transitions =
            hh_lab::debt::evaluate_debt(&debt_ref, &debt, Some(home.id), &obs, now_ms, &policy);
        if !transitions.is_empty() {
            let (run_id, lease) = self.ensure_registry_run()?;
            let kernel = self.kernel_prov.clone();
            let events: Vec<RegistryEvent> = transitions
                .iter()
                .map(|t| RegistryEvent {
                    class: "lifecycle.debt.status.changed",
                    payload: t.to_json(),
                    provenance: None,
                    anchor_tag: None,
                })
                .collect();
            hh_registry::events::emit(&mut self.store, &run_id, &lease, &kernel, events)
                .map_err(crate::service::ledger_err)?;
        }
        Ok(transitions)
    }
}
