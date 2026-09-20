//! `compensate_run` — the audited best-effort saga (§5a.2; ADR-0032 — `R-2.2.2¹`,
//! C1·S2). For every effect that reached `observed{applied}` under a
//! `compensable` class, `compensate_run` drives a **compensating effect** — a
//! full lifecycle row set (`intended → authorized → decided{allow} → prepared
//! → committed → observed`) whose `compensates` member names the original —
//! then marks the original `action.effect.compensated{original_effect_id}`
//! (the scope-free marker form).
//!
//! Order is **reverse application order** (descending commit seq). A failing
//! compensator lands `lifecycle.escalation.raised` + `action.effect.abandoned`
//! on the compensator (never silently dropped — ADR-0032 §4) and the saga
//! reports it; the original stays `observed{applied}` (uncompensated — the
//! completion gate reads it).
//!
//! Idempotent across a mid-saga crash (KP-safe): the compensating effect's id
//! is *derived* (`<original>~comp`) and every step is its own durable batch —
//! a re-run folds the committed prefix, skips compensated originals and
//! terminal/abandoned compensators, resumes an in-flight compensator only by
//! the normal recovery path, and never re-dispatches an applied compensator.

use std::collections::BTreeMap;

use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
use hh_wire::json::Json;

use crate::effect::{idempotency_key, EffectPhase, ObservedOutcome};
use crate::errors::LedgerError;
use crate::event::Scope;
use crate::recovery::kernel_ev;
use crate::store::{Lease, Store};

/// The derived compensating-effect id — `f(original)` only, so a torn saga
/// re-derives the same id (the `DuplicateEventId`-free re-entry is the
/// idempotence).
pub fn compensating_effect_id(original: &str) -> String {
    format!("{original}~comp")
}

/// One dispatch unit — what the caller executes for the compensator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompensationIntent {
    /// The applied effect being compensated.
    pub original_effect_id: String,
    /// The compensating effect (the new lifecycle's scope id).
    pub compensating_effect_id: String,
    /// The original's `capability_id`, when declared.
    pub capability_id: Option<String>,
    /// The original's `args_canonical_hash`, when declared.
    pub args_canonical_hash: Option<String>,
}

/// The saga report — the rows are the record; this is the greppable summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SagaReport {
    /// Originals newly (or already) marked `compensated` this pass.
    pub compensated: Vec<String>,
    /// Compensators that failed — `abandoned` + escalation (per original).
    pub abandoned: Vec<String>,
    /// Originals skipped — a compensator is still in flight (non-terminal).
    pub in_flight: Vec<String>,
    /// The `lifecycle.escalation.raised` event ids this pass minted.
    pub escalations: Vec<String>,
}

impl Store {
    /// `compensate_run(run, lease, dispatch)` — walk the applied+compensable
    /// effects in reverse commit order; for each, drive the compensating
    /// lifecycle and mark the original. `dispatch` performs the compensator's
    /// external act: `Ok(payload)` ⇒ `observed{applied}`; `Err(reason)` ⇒
    /// `abandoned` + `lifecycle.escalation.raised`.
    pub fn compensate_run(
        &mut self,
        run_id: &str,
        lease: &Lease,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
    ) -> Result<SagaReport, LedgerError> {
        self.compensate_scoped(run_id, lease, None, dispatch)
    }

    /// `compensate_run_after(run, lease, at_seq, dispatch)` — the rollback-scoped
    /// saga (§5a.1 `rollback`; ADR-0271): only effects whose latest `committed`
    /// landed **after** `at_seq` are compensated — the shared prefix's effects
    /// are never touched (the prefix is immutable by append-only construction
    /// and the saga honours it by selection).
    pub fn compensate_run_after(
        &mut self,
        run_id: &str,
        lease: &Lease,
        at_seq: u64,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
    ) -> Result<SagaReport, LedgerError> {
        self.compensate_scoped(run_id, lease, Some(at_seq), dispatch)
    }

    /// The shared body — `after_seq = None` ⇒ the whole run; `Some(at)` ⇒ only
    /// effects committed after `at`.
    fn compensate_scoped(
        &mut self,
        run_id: &str,
        lease: &Lease,
        after_seq: Option<u64>,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
    ) -> Result<SagaReport, LedgerError> {
        self.tier_c1("compensate_run")?;
        let gen = lease.generation;
        // The saga targets: `observed{applied}` + `compensable`, reverse
        // commit order (descending commit seq).
        let mut targets: Vec<(u64, String)> = self
            .run(run_id)?
            .effects
            .values()
            .filter(|f| {
                f.phase == EffectPhase::Observed
                    && f.outcome == Some(ObservedOutcome::Applied)
                    && f.risk_class.reversibility == RiskReversibility::Compensable
            })
            .map(|f| {
                let commit_seq = f.commits.values().map(|(_, s)| *s).max().unwrap_or(0);
                (commit_seq, f.effect_id.clone())
            })
            .filter(|(commit_seq, _)| after_seq.map(|at| *commit_seq > at).unwrap_or(true))
            .collect();
        targets.sort_by(|a, b| b.0.cmp(&a.0));

        let mut report = SagaReport::default();
        for (_, original) in targets {
            self.saga_step(run_id, lease, gen, &original, dispatch, &mut report)?;
        }
        Ok(report)
    }

    /// One saga step — `original` is `observed{applied}` + `compensable`.
    fn saga_step(
        &mut self,
        run_id: &str,
        lease: &Lease,
        gen: u64,
        original: &str,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
        report: &mut SagaReport,
    ) -> Result<(), LedgerError> {
        let comp_id = compensating_effect_id(original);
        // Resume-aware dispatch: fold the compensator's current phase.
        match self.run(run_id)?.effects.get(&comp_id).map(|f| f.phase) {
            Some(EffectPhase::Observed)
            | Some(EffectPhase::Abandoned)
            | Some(EffectPhase::Compensated)
            | Some(EffectPhase::Reverted)
            | Some(EffectPhase::Refused) => {
                // Terminal compensator — ensure the original's marker; an
                // abandoned compensator is reported, never re-dispatched.
                if self.run(run_id)?.effects[&comp_id].phase == EffectPhase::Observed
                    && self.run(run_id)?.effects[&comp_id].is_terminal()
                {
                    self.saga_mark(run_id, lease, gen, original, report)?;
                } else if !report.abandoned.contains(&original.to_string()) {
                    report.abandoned.push(original.to_string());
                }
                return Ok(());
            }
            Some(_) => {
                // Non-terminal compensator — in flight; the recovery table owns
                // its completion (idempotent, never re-dispatched here).
                report.in_flight.push(original.to_string());
                return Ok(());
            }
            None => {}
        }

        let f = self.run(run_id)?.effects[original].clone();
        let args_hash = f
            .args_canonical_hash
            .clone()
            .unwrap_or_else(|| "sha256:compensation".to_string());
        let cap_ver = f
            .capability_version
            .clone()
            .unwrap_or_else(|| "saga".to_string());
        // The compensator's own class: `irreversible` (a compensator has no
        // compensator) + `idempotent` (compensation is idempotent — §5a.2 §5);
        // the original's reach (`scope`) is inherited.
        let comp_class = RiskClass {
            reversibility: RiskReversibility::Irreversible,
            repeat_safety: RepeatSafety::Idempotent,
            scope: match f.risk_class.scope {
                RiskScope::WorkspaceLocal => RiskScope::WorkspaceLocal,
                RiskScope::External => RiskScope::External,
            },
        };
        let scope = |eid: &str| Scope {
            effect_id: Some(eid.to_string()),
            ..Scope::default()
        };
        // ── intended ────────────────────────────────────────────────────
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.intended",
            scope(&comp_id),
            Json::obj([
                ("compensates", Json::str(original)),
                ("effective_risk_class", comp_class.to_json()),
                ("declared_risk_class", comp_class.to_json()),
                ("args_canonical_hash", Json::str(&args_hash)),
                ("capability_version", Json::str(&cap_ver)),
            ]),
        )?;
        self.append(run_id, lease, vec![ev])?;
        // ── authorized + the complete-mediation `decided{allow}` (the saga
        //    runs under a Π-authorized policy basis — the rows are the
        //    record) ─────────────────────────────────────────────────────
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.authorized",
            scope(&comp_id),
            Json::obj([
                ("effective_risk_class", comp_class.to_json()),
                ("decision_ref", Json::str("saga:compensation")),
            ]),
        )?;
        let decided = kernel_ev(
            self,
            run_id,
            "security.permission.decided",
            scope(&comp_id),
            Json::obj([
                ("permission_id", Json::str(format!("saga:{original}"))),
                ("decision", Json::str("allow")),
                ("attempt_no", Json::Int(1)),
                ("decider", Json::str("policy")),
                ("policy_ref", Json::str("healing_policy|saga")),
                (
                    "reason",
                    Json::str("compensation saga — Π-authorized (ADR-0032)"),
                ),
            ]),
        )?;
        self.append(run_id, lease, vec![ev, decided])?;
        // ── prepared (derived idempotency key — AC-R-2.2.2-7) ───────────
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.prepared",
            scope(&comp_id),
            Json::obj([(
                "idempotency_key",
                Json::str(idempotency_key(run_id, &comp_id, &args_hash, &cap_ver)),
            )]),
        )?;
        self.append(run_id, lease, vec![ev])?;
        // ── committed (fencing token = the writer generation) ───────────
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.committed",
            scope(&comp_id),
            Json::obj([
                ("attempt_no", Json::Int(1)),
                ("fencing_token", Json::Int(gen as i64)),
            ]),
        )?;
        self.append(run_id, lease, vec![ev])?;
        // ── dispatch → observed | abandoned+escalation ──────────────────
        let intent = CompensationIntent {
            original_effect_id: original.to_string(),
            compensating_effect_id: comp_id.clone(),
            capability_id: f.capability_id.clone(),
            args_canonical_hash: f.args_canonical_hash.clone(),
        };
        match dispatch(&intent) {
            Ok(outcome) => {
                let mut payload = BTreeMap::from([
                    ("attempt_no".to_string(), Json::Int(1)),
                    ("outcome".to_string(), Json::str("applied")),
                    ("fencing_token".to_string(), Json::Int(gen as i64)),
                ]);
                if let Json::Obj(extra) = outcome {
                    payload.extend(extra);
                }
                let ev = kernel_ev(
                    self,
                    run_id,
                    "action.effect.observed",
                    scope(&comp_id),
                    Json::Obj(payload),
                )?;
                self.append(run_id, lease, vec![ev])?;
                self.saga_mark(run_id, lease, gen, original, report)?;
            }
            Err(reason) => {
                // The escalation precedes `abandoned` — its `escalation_ref`
                // names this row (ADR-0032 §4; ADR-0066 Rule O).
                let esc = kernel_ev(
                    self,
                    run_id,
                    "lifecycle.escalation.raised",
                    Scope::default(),
                    Json::obj([
                        ("subject", Json::str(&comp_id)),
                        ("kind", Json::str("compensation_failed")),
                        ("reason", Json::str(&reason)),
                    ]),
                )?;
                let esc_id = esc.event_id.clone();
                let abandoned = kernel_ev(
                    self,
                    run_id,
                    "action.effect.abandoned",
                    scope(&comp_id),
                    Json::obj([
                        ("reason", Json::str(&reason)),
                        ("escalation_ref", Json::str(&esc_id)),
                        ("fencing_token", Json::Int(gen as i64)),
                    ]),
                )?;
                self.append(run_id, lease, vec![esc, abandoned])?;
                report.escalations.push(esc_id);
                report.abandoned.push(original.to_string());
            }
        }
        Ok(())
    }

    /// The post-terminal marker — `action.effect.compensated{original_effect_id}`
    /// (the scope-free saga mark; idempotent by the fold's `Compensated` phase).
    fn saga_mark(
        &mut self,
        run_id: &str,
        lease: &Lease,
        gen: u64,
        original: &str,
        report: &mut SagaReport,
    ) -> Result<(), LedgerError> {
        if self.run(run_id)?.effects[original].phase == EffectPhase::Compensated {
            report.compensated.push(original.to_string());
            return Ok(());
        }
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.compensated",
            Scope::default(),
            Json::obj([
                ("original_effect_id", Json::str(original)),
                ("fencing_token", Json::Int(gen as i64)),
            ]),
        )?;
        self.append(run_id, lease, vec![ev])?;
        report.compensated.push(original.to_string());
        Ok(())
    }
}
