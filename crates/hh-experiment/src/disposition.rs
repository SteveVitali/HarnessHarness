//! `disposition` — the one admitted plan-disposition read, `plan_disposition_at
//! (scoring_ref)` (PADR-R5-1 A.1 items 4/6, A.9 item 5; spec §2.2 step 6;
//! REQ-E-202/REQ-E-008 primary). Every consumer — scheduler, results/query,
//! export, compare, analysis and non-partial close — composes this read;
//! none re-selects (X6).
//!
//! The read is pure over durable records: the plan's `launch_bound.
//! grade_binding` freeze (R5.6/ADR-0385: `mode:"required"` +
//! `selection_policy_ref`), the spec's `ext.required_grade` per-task
//! `{oracle_ref, eligible[]}` pins, the subject run's `submission_seal`
//! end-state pin + stop-derived outcome class + veto axes (R5.10), the
//! instrument run's `verification.oracle.verdict` rows (R5.11) read at the
//! pinned watermarks, and the `measurement.experiment.selection_revision`
//! rows (the durable revision record home this ticket lands, ADR-0388's
//! consumer-ticket decision). `grade_pending` is an **admission state** —
//! never a seventh outcome class: pending blocks every scored-success/
//! completion interpretation until a `plan_disposition` row resolves it.
//!
//! Legacy: `mode:"legacy"` (or a plan with no required freeze) reads
//! `DispositionRead { mode: Legacy }` — consumers keep the stop-derived
//! interpretation byte-for-byte (A.7). An unknown mode/member refuses —
//! never a silent downgrade (A.6).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hharness_eval::selection::{
    plan_disposition, select_grade_at, GradeAttempt, GradeOutcome, PlanDisposition, RevisionLog,
    SelectionRevision, ValidGradeSet,
};
use hharness_ledger::store::Store;
use hharness_ontology::control::{OutcomeClass, StopReason};
use hharness_ontology::eval::MetricValueKind;
use hharness_wire::json::Json;

use crate::docs::LabDocs;
use crate::errors::ExperimentError;
use crate::view::ExperimentView;
use hharness_lab::experiment::ExperimentSpec;

/// The `spec.ext` member carrying the required-grade opt-in freeze.
pub const REQUIRED_GRADE_MEMBER: &str = "required_grade";
/// The required-mode spelling the freeze and `grade_binding` carry.
pub const MODE_REQUIRED: &str = "required";
/// The legacy-mode spelling (absence preserves behavior).
pub const MODE_LEGACY: &str = "legacy";
/// The instrument-run verdict class (R5.11 `hh-bench` producer).
pub const ORACLE_VERDICT: &str = "verification.oracle.verdict";
/// The subject run's end-state pin purpose marker (R5.10).
pub const SEAL_PURPOSE: &str = "submission_seal";
/// The experiment-run revision record class this ticket lands.
pub const SELECTION_REVISION: &str = "measurement.experiment.selection_revision";

/// `scoring_ref` — the read pins a `plan_disposition_at` call composes
/// under: the experiment run's watermark and the instrument/overlay runs
/// each read to their `until_seq` (`None` = head at read time).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DispositionPins {
    /// The experiment run's read watermark (`None` = head).
    pub experiment_until: Option<u64>,
    /// `(instrument_run_id, until_seq)` — each overlay run's pin.
    pub overlay_runs: Vec<(String, Option<u64>)>,
}

/// The frozen per-task grade pins from `spec.ext["required_grade"]`.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskGradePins {
    /// The pinned unchanged oracle identity.
    pub oracle_ref: String,
    /// The eligible grade members (metric refs).
    pub eligible: BTreeSet<String>,
}

/// The spec-level required-grade freeze `{mode:"required",
/// selection_policy_ref, tasks:{task_id → {oracle_ref, eligible[]}}}`.
#[derive(Debug, Clone, PartialEq)]
pub struct RequiredGradeFreeze {
    /// The frozen selection/regrade policy's content address.
    pub selection_policy_ref: String,
    /// Per-task `{oracle_ref, eligible[]}` pins.
    pub tasks: BTreeMap<String, TaskGradePins>,
}

fn s(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

/// Strict `spec.ext["required_grade"]` decode — `None` when the member is
/// absent (legacy). An unknown mode, unknown member or malformed pin is a
/// typed `UnknownRequiredFeature` refusal — the negotiation surface (A.6),
/// never a silent downgrade to legacy.
pub fn required_grade_freeze(
    spec: &ExperimentSpec,
) -> Result<Option<RequiredGradeFreeze>, ExperimentError> {
    let Some(j) = spec.ext.get(REQUIRED_GRADE_MEMBER) else {
        return Ok(None);
    };
    if matches!(j, Json::Null) {
        return Ok(None);
    }
    let bad = |member: &str, detail: &str| ExperimentError::UnknownRequiredFeature {
        member: format!("{REQUIRED_GRADE_MEMBER}.{member}"),
        detail: detail.to_string(),
    };
    let Json::Obj(m) = j else {
        return Err(bad("", "not an object"));
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "mode" | "selection_policy_ref" | "tasks") {
            return Err(bad(k, "unknown member"));
        }
    }
    match s(j, "mode").as_deref() {
        Some(MODE_REQUIRED) => {}
        Some(other) => return Err(bad("mode", &format!("unknown mode `{other}`"))),
        None => return Err(bad("mode", "missing")),
    }
    let selection_policy_ref = s(j, "selection_policy_ref")
        .filter(|r| !r.is_empty())
        .ok_or_else(|| bad("selection_policy_ref", "empty or missing"))?;
    let Some(Json::Obj(tasks)) = m.get("tasks") else {
        return Err(bad("tasks", "missing or not an object"));
    };
    let mut out = BTreeMap::new();
    for (task_id, tv) in tasks {
        let Some(oracle_ref) = s(tv, "oracle_ref").filter(|r| !r.is_empty()) else {
            return Err(bad(
                "tasks",
                &format!("task `{task_id}` oracle_ref missing"),
            ));
        };
        let eligible: BTreeSet<String> = match tv.get("eligible") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => {
                return Err(bad(
                    "tasks",
                    &format!("task `{task_id}` eligible[] missing"),
                ))
            }
        };
        if eligible.is_empty() {
            return Err(bad("tasks", &format!("task `{task_id}` eligible[] empty")));
        }
        out.insert(
            task_id.clone(),
            TaskGradePins {
                oracle_ref,
                eligible,
            },
        );
    }
    Ok(Some(RequiredGradeFreeze {
        selection_policy_ref,
        tasks: out,
    }))
}

/// What a `plan_disposition_at` read resolved — the disposition plus the
/// pins/refs every consumer must report (watermarks pin exactly what was
/// read; `eligible_set_ref` binds the frozen set).
#[derive(Debug, Clone, PartialEq)]
pub struct DispositionRead {
    /// The frozen grade mode (`legacy`/`required`).
    pub mode: String,
    /// The admitted disposition (`None` in legacy mode — the consumer's
    /// stop-derived interpretation stands).
    pub disposition: Option<PlanDisposition>,
    /// The resolved watermarks the selection read (`instrument_run → seq`).
    pub watermarks: BTreeMap<String, u64>,
    /// The content-addressed frozen valid-grade set (`None` in legacy).
    pub eligible_set_ref: Option<String>,
    /// Why the set could not materialise (unsealed/unsettled) while the
    /// admission state stays pending — diagnostic only.
    pub pending_detail: Option<String>,
}

impl DispositionRead {
    /// `true` when the plan's admission is `grade_pending`.
    pub fn is_pending(&self) -> bool {
        matches!(
            (&self.mode[..], &self.disposition),
            (MODE_REQUIRED, Some(PlanDisposition::Pending { .. })) | (MODE_REQUIRED, None)
        )
    }
}

/// Decode one instrument-run `verification.oracle.verdict` row into the
/// owner-validated `GradeAttempt` facts (one per eligible member).
fn verdict_attempts(
    e: &hharness_ledger::event::EventEnvelope,
    subject: &str,
    eligible: &BTreeSet<String>,
    out: &mut Vec<GradeAttempt>,
) {
    if e.class != ORACLE_VERDICT {
        return;
    }
    let p = &e.payload;
    if s(p, "subject_run").as_deref() != Some(subject) {
        return;
    }
    let Some(attempt_ref) = s(p, "attempt_ref") else {
        return;
    };
    let outcome = match p.get("outcome").and_then(|o| s(o, "status")).as_deref() {
        Some("decided") => GradeOutcome::Decided(MetricValueKind::Decimal(
            p.get("outcome")
                .and_then(|o| o.get("reward_ppm"))
                .and_then(Json::as_int)
                .unwrap_or(0),
        )),
        _ => GradeOutcome::OracleFailure,
    };
    let mut causes = Vec::new();
    for k in ["seal_ref", "invocation_ref"] {
        if let Some(c) = s(p, k) {
            causes.push(c);
        }
    }
    causes.push(format!("{}:{}", e.run_id, e.event_id));
    // One `GradeAttempt` per eligible member — a multi-member eligible set
    // presents same-coordinate facts and the R5.9 contract refuses
    // `ConflictingCoordinate` (typed refusal, never a silent merge — a
    // member-qualified coordinate is an owed R5.9 revisit, ADR-0388).
    for member in eligible {
        out.push(GradeAttempt {
            attempt_ref: attempt_ref.clone(),
            instrument_run_id: e.run_id.clone(),
            seq: e.seq,
            subject_ref: s(p, "subject_run").unwrap_or_default(),
            submission_ref: s(p, "submission_ref").unwrap_or_default(),
            oracle_ref: s(p, "oracle_ref").unwrap_or_default(),
            metric_ref: member.clone(),
            cause_refs: causes.clone(),
            outcome: outcome.clone(),
        });
    }
}

/// `plan_disposition_at(scoring_ref)` — the shared admission read. Pure:
/// equal `(durable prefixes, pins)` produce equal results; the read never
/// writes. `docs` resolves the experiment spec's `ext.required_grade`.
pub fn plan_disposition_at(
    store: &Store,
    docs: &LabDocs,
    experiment_run_id: &str,
    run_plan_id: &str,
    pins: &DispositionPins,
) -> Result<DispositionRead, ExperimentError> {
    let head = store
        .head(experiment_run_id)
        .map_err(ExperimentError::Ledger)?;
    let until = pins.experiment_until.unwrap_or(head.seq);
    let envelopes: Vec<hharness_ledger::event::EventEnvelope> = store
        .envelopes(experiment_run_id)
        .map_err(ExperimentError::Ledger)?
        .iter()
        .filter(|e| e.seq <= until)
        .cloned()
        .collect();
    let view = ExperimentView::fold(&envelopes);
    let ps = view
        .plans
        .get(run_plan_id)
        .ok_or_else(|| ExperimentError::UnknownRunPlan {
            run_plan_id: run_plan_id.to_string(),
        })?;
    let binding = &ps.launch_binding;
    let mode = binding
        .as_ref()
        .and_then(|b| s(&b.grade_binding, "mode"))
        .unwrap_or_else(|| MODE_LEGACY.to_string());
    if mode == MODE_LEGACY {
        return Ok(DispositionRead {
            mode,
            disposition: None,
            watermarks: BTreeMap::new(),
            eligible_set_ref: None,
            pending_detail: None,
        });
    }
    if mode != MODE_REQUIRED {
        return Err(ExperimentError::UnknownRequiredFeature {
            member: "grade_binding.mode".to_string(),
            detail: format!("unknown mode `{mode}`"),
        });
    }
    let declared = view
        .declared
        .clone()
        .ok_or_else(|| ExperimentError::NotAnExperimentRun {
            run_id: experiment_run_id.to_string(),
        })?;
    let spec =
        docs.spec(&declared.experiment_id)?
            .ok_or_else(|| ExperimentError::UnknownExperiment {
                experiment_id: declared.experiment_id.clone(),
            })?;
    let freeze =
        required_grade_freeze(&spec)?.ok_or_else(|| ExperimentError::UnknownRequiredFeature {
            member: REQUIRED_GRADE_MEMBER.to_string(),
            detail: "required mode bound without a spec freeze".to_string(),
        })?;
    let tpins =
        freeze
            .tasks
            .get(&ps.task_id)
            .ok_or_else(|| ExperimentError::UnknownRequiredFeature {
                member: format!("{REQUIRED_GRADE_MEMBER}.tasks"),
                detail: format!("no frozen pins for task `{}`", ps.task_id),
            })?;
    let subject = binding
        .as_ref()
        .map(|b| b.subject_run_id.clone())
        .filter(|r| !r.is_empty())
        .or_else(|| {
            ps.attempts
                .iter()
                .rev()
                .find(|a| !a.superseded)
                .map(|a| a.run_id.clone())
        })
        .ok_or_else(|| ExperimentError::BadPlanState {
            run_plan_id: run_plan_id.to_string(),
            detail: "no subject run bound".to_string(),
        })?;

    // Subject-side facts: actual finish (stop-derived class), the sealed
    // end-state pin, and the veto axis.
    let subject_events = store
        .envelopes(&subject)
        .map_err(ExperimentError::Ledger)?
        .to_vec();
    let finished = subject_events
        .iter()
        .find(|e| e.class == "lifecycle.run.finished");
    let (stop_class, submission_ref) = match finished {
        Some(f) => {
            let stop = f
                .payload
                .get("stop_reason")
                .and_then(StopReason::from_json)
                .unwrap_or(StopReason::Completed);
            let oracle_failed =
                matches!(
                    f.payload.get("outcome_class").and_then(Json::as_str),
                    Some("oracle_failure")
                ) || matches!(f.payload.get("oracle_failed"), Some(Json::Bool(true)));
            let sealed = match f.payload.get("submission_seal") {
                Some(Json::Arr(rows)) => rows.iter().find_map(|r| {
                    let st = s(r, "status").unwrap_or_default();
                    if st == "sealed" || st == "prior" {
                        s(r, "end_state_ref")
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            (
                hharness_ontology::eval::derive_outcome_class(&stop, oracle_failed),
                sealed,
            )
        }
        None => (OutcomeClass::Scored, None),
    };
    let mut veto_tripped: Vec<String> = Vec::new();
    let mut vetoes: BTreeSet<String> = BTreeSet::new();
    for e in &subject_events {
        if e.class.starts_with("security.invariant") || e.class.starts_with("control.veto") {
            for k in ["veto_id", "metric", "invariant"] {
                if let Some(v) = s(&e.payload, k) {
                    vetoes.insert(v);
                }
            }
        }
    }
    veto_tripped.extend(vetoes);

    // A mutable/unsealed or unfinished workspace yields no admissible
    // evidence — the admission stays pending rather than fabricating pins.
    let (subject_ref, submission_ref) = match submission_ref {
        Some(r) if finished.is_some() => (subject.clone(), r),
        _ => {
            return Ok(DispositionRead {
                mode: MODE_REQUIRED.to_string(),
                disposition: Some(PlanDisposition::Pending {
                    missing: tpins.eligible.iter().cloned().collect(),
                    veto_tripped,
                }),
                watermarks: BTreeMap::new(),
                eligible_set_ref: None,
                pending_detail: Some(if finished.is_none() {
                    "subject_run unfinished".to_string()
                } else {
                    "submission unsealed — a mutable workspace is never graded".to_string()
                }),
            });
        }
    };
    let policy_ref = binding
        .as_ref()
        .and_then(|b| s(&b.grade_binding, "selection_policy_ref"))
        .unwrap_or_else(|| freeze.selection_policy_ref.clone());
    let set = ValidGradeSet {
        subject_ref,
        submission_ref,
        oracle_ref: tpins.oracle_ref.clone(),
        policy_ref,
        members: tpins.eligible.clone(),
    };

    // Instrument-run verdicts at the pinned watermarks.
    let mut attempts: Vec<GradeAttempt> = Vec::new();
    let mut watermarks: BTreeMap<String, u64> = BTreeMap::new();
    for (ov_run, ov_until) in &pins.overlay_runs {
        let ohead = store.head(ov_run).map_err(ExperimentError::Ledger)?;
        let oseq = ov_until.unwrap_or(ohead.seq);
        for e in store
            .envelopes(ov_run)
            .map_err(ExperimentError::Ledger)?
            .iter()
            .filter(|e| e.seq <= oseq)
        {
            verdict_attempts(e, &subject, &tpins.eligible, &mut attempts);
        }
        watermarks.insert(ov_run.clone(), oseq);
    }

    // Durable revisions (the `measurement.experiment.selection_revision`
    // home) bound to this eligible set.
    let mut revisions: Vec<SelectionRevision> = Vec::new();
    for e in &envelopes {
        if e.class != SELECTION_REVISION {
            continue;
        }
        let p = &e.payload;
        let mut wm = BTreeMap::new();
        if let Some(Json::Obj(rows)) = p.get("watermarks") {
            for (r, v) in rows {
                if let Some(seq) = v.as_int() {
                    wm.insert(r.clone(), seq as u64);
                }
            }
        }
        revisions.push(SelectionRevision {
            revision_id: s(p, "revision_id").unwrap_or_default(),
            reason: s(p, "reason").unwrap_or_default(),
            supersedes: s(p, "supersedes").unwrap_or_default(),
            authority: s(p, "authority").unwrap_or_default(),
            watermarks: wm,
            eligible_set_ref: s(p, "eligible_set_ref").unwrap_or_default(),
        });
    }
    let selection = select_grade_at(&set, &attempts, &revisions, &watermarks).map_err(|e| {
        ExperimentError::BadPlanState {
            run_plan_id: run_plan_id.to_string(),
            detail: format!("selection refused: {e}"),
        }
    })?;
    let eligible_set_ref = set.eligible_set_ref();
    let disposition = plan_disposition(stop_class, &veto_tripped, &selection);
    Ok(DispositionRead {
        mode: MODE_REQUIRED.to_string(),
        disposition: Some(disposition),
        watermarks,
        eligible_set_ref: Some(eligible_set_ref),
        pending_detail: None,
    })
}

/// `settle_graded`'s replay record — re-derive the admission result of an
/// already-committed `plan_disposition` row (exactly-once: the recorded
/// disposition replays verbatim, never a second write).
pub fn recorded_disposition(p: &Json) -> Option<(String, Option<OutcomeClass>)> {
    let d = s(p, "disposition")?;
    let oc = s(p, "outcome_class").and_then(|o| OutcomeClass::parse(&o));
    Some((d, oc))
}

/// Validate an authorized `SelectionRevision` against the durable log +
/// the pinned attempts (`RevisionLog::append` is the admitted semantics —
/// authority/watermarks/set/supersedes/immutability all enforced).
pub fn validate_revision(
    store: &Store,
    docs: &LabDocs,
    experiment_run_id: &str,
    run_plan_id: &str,
    pins: &DispositionPins,
    rev: &SelectionRevision,
) -> Result<ValidGradeSet, ExperimentError> {
    let head = store
        .head(experiment_run_id)
        .map_err(ExperimentError::Ledger)?;
    let until = pins.experiment_until.unwrap_or(head.seq);
    let envelopes: Vec<hharness_ledger::event::EventEnvelope> = store
        .envelopes(experiment_run_id)
        .map_err(ExperimentError::Ledger)?
        .iter()
        .filter(|e| e.seq <= until)
        .cloned()
        .collect();
    let view = ExperimentView::fold(&envelopes);
    let ps = view
        .plans
        .get(run_plan_id)
        .ok_or_else(|| ExperimentError::UnknownRunPlan {
            run_plan_id: run_plan_id.to_string(),
        })?;
    let declared = view
        .declared
        .clone()
        .ok_or_else(|| ExperimentError::NotAnExperimentRun {
            run_id: experiment_run_id.to_string(),
        })?;
    let spec =
        docs.spec(&declared.experiment_id)?
            .ok_or_else(|| ExperimentError::UnknownExperiment {
                experiment_id: declared.experiment_id.clone(),
            })?;
    let freeze =
        required_grade_freeze(&spec)?.ok_or_else(|| ExperimentError::UnknownRequiredFeature {
            member: REQUIRED_GRADE_MEMBER.to_string(),
            detail: "required mode bound without a spec freeze".to_string(),
        })?;
    let tpins =
        freeze
            .tasks
            .get(&ps.task_id)
            .ok_or_else(|| ExperimentError::UnknownRequiredFeature {
                member: format!("{REQUIRED_GRADE_MEMBER}.tasks"),
                detail: format!("no frozen pins for task `{}`", ps.task_id),
            })?;
    let binding = ps
        .launch_binding
        .as_ref()
        .ok_or_else(|| ExperimentError::BadPlanState {
            run_plan_id: run_plan_id.to_string(),
            detail: "no launch binding".to_string(),
        })?;
    let subject = binding.subject_run_id.clone();
    let subject_events = store
        .envelopes(&subject)
        .map_err(ExperimentError::Ledger)?
        .to_vec();
    let submission_ref = subject_events
        .iter()
        .find(|e| e.class == "lifecycle.run.finished")
        .and_then(|f| match f.payload.get("submission_seal") {
            Some(Json::Arr(rows)) => rows.iter().find_map(|r| {
                let st = s(r, "status").unwrap_or_default();
                if st == "sealed" || st == "prior" {
                    s(r, "end_state_ref")
                } else {
                    None
                }
            }),
            _ => None,
        })
        .unwrap_or_default();
    let set = ValidGradeSet {
        subject_ref: subject,
        submission_ref,
        oracle_ref: tpins.oracle_ref.clone(),
        policy_ref: binding
            .grade_binding
            .get("selection_policy_ref")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        members: tpins.eligible.clone(),
    };
    let mut attempts: Vec<GradeAttempt> = Vec::new();
    for (ov_run, ov_until) in &pins.overlay_runs {
        let ohead = store.head(ov_run).map_err(ExperimentError::Ledger)?;
        let oseq = ov_until.unwrap_or(ohead.seq);
        for e in store
            .envelopes(ov_run)
            .map_err(ExperimentError::Ledger)?
            .iter()
            .filter(|e| e.seq <= oseq)
        {
            verdict_attempts(e, &set.subject_ref, &tpins.eligible, &mut attempts);
        }
    }
    let mut log = RevisionLog::default();
    for e in &envelopes {
        if e.class != SELECTION_REVISION {
            continue;
        }
        let p = &e.payload;
        let mut wm = BTreeMap::new();
        if let Some(Json::Obj(rows)) = p.get("watermarks") {
            for (r, v) in rows {
                if let Some(seq) = v.as_int() {
                    wm.insert(r.clone(), seq as u64);
                }
            }
        }
        let prior = SelectionRevision {
            revision_id: s(p, "revision_id").unwrap_or_default(),
            reason: s(p, "reason").unwrap_or_default(),
            supersedes: s(p, "supersedes").unwrap_or_default(),
            authority: s(p, "authority").unwrap_or_default(),
            watermarks: wm,
            eligible_set_ref: s(p, "eligible_set_ref").unwrap_or_default(),
        };
        let known: BTreeSet<String> = attempts.iter().map(|a| a.attempt_ref.clone()).collect();
        log.append(&set, prior, &known)
            .map_err(|e| ExperimentError::BadPlanState {
                run_plan_id: run_plan_id.to_string(),
                detail: format!("durable revision log diverges: {e}"),
            })?;
    }
    let known: BTreeSet<String> = attempts.iter().map(|a| a.attempt_ref.clone()).collect();
    log.append(&set, rev.clone(), &known)
        .map_err(|e| ExperimentError::BadPlanState {
            run_plan_id: run_plan_id.to_string(),
            detail: format!("revision refused: {e}"),
        })?;
    Ok(set)
}
