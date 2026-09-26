//! The `measurement.experiment.*` payload builders and the engine's
//! boundary records (`RunOutcome`, `ExperimentReport`) — spec §6.3 §2.2/§2.3
//! (R-2.10.3⁰ᵇ; ADR-0155 D2/D5/D7; the closed family of ADR-0183 §C.2 as
//! registered in `hh-ledger::classes`).
//!
//! Payloads are canonical `Json` — the fold ([`crate::view`]) parses the same
//! members these builders write, so a rebuild and the live path see one truth.

use std::collections::BTreeMap;

use hh_ontology::control::OutcomeClass;
use hh_wire::json::Json;

use hh_lab::experiment::{CellPlan, ExperimentSpec, RunPlan};

/// The class spellings (registered in `hh-ledger::classes`).
pub mod class {
    /// `measurement.experiment.declared` — exactly one, before any `run_bound`.
    pub const DECLARED: &str = "measurement.experiment.declared";
    /// `measurement.experiment.run_planned` — one per `RunPlan`.
    pub const RUN_PLANNED: &str = "measurement.experiment.run_planned";
    /// `measurement.experiment.run_claimed` — the `resource(run_plan_id)` claim.
    pub const RUN_CLAIMED: &str = "measurement.experiment.run_claimed";
    /// `measurement.experiment.claim_expired` — expiry returns the plan to
    /// eligible.
    pub const CLAIM_EXPIRED: &str = "measurement.experiment.claim_expired";
    /// `measurement.experiment.run_launched` — the engine's launch record.
    pub const RUN_LAUNCHED: &str = "measurement.experiment.run_launched";
    /// `measurement.experiment.bound` — the subject run's own binding stamp
    /// (`{experiment_id, arm_id, configuration_id, cell_id, replicate_index,
    /// attempt_no, budget_id}`; §6.3 `launch` step 4 — its chain is the proof
    /// the experiment run's `run_bound` mirror cites).
    pub const BOUND: &str = "measurement.experiment.bound";
    /// `measurement.experiment.run_bound` — stamped on the experiment run
    /// (the mirror, `{run_id, cell_id, …, subject_head}`).
    pub const RUN_BOUND: &str = "measurement.experiment.run_bound";
    /// `measurement.experiment.run_settled` — the outcome-class settlement.
    pub const RUN_SETTLED: &str = "measurement.experiment.run_settled";
    /// `measurement.experiment.run_excluded` — `{reason, evidence?, authority}`.
    pub const RUN_EXCLUDED: &str = "measurement.experiment.run_excluded";
    /// `measurement.experiment.run_replanned` — a superseded plan returns to
    /// eligible at `attempt_no + 1`.
    pub const RUN_REPLANNED: &str = "measurement.experiment.run_replanned";
    /// `measurement.experiment.cell_completed` — all of a cell's replicates
    /// accepted.
    pub const CELL_COMPLETED: &str = "measurement.experiment.cell_completed";
    /// `measurement.experiment.amended` — `{diff_ref, reason, authority}`.
    pub const AMENDED: &str = "measurement.experiment.amended";
    /// `measurement.experiment.closed` — `{status, coverage, outcome_counts,
    /// watermark_set, summary_ref?}`.
    pub const CLOSED: &str = "measurement.experiment.closed";
    /// `measurement.experiment.paused` — `{reason}` (audit-grade; `operator`
    /// pauses are the attended surface).
    pub const PAUSED: &str = "measurement.experiment.paused";
    /// `measurement.experiment.resumed`.
    pub const RESUMED: &str = "measurement.experiment.resumed";
    /// `measurement.experiment.drift_bracket` — `{phase ∈ {opened, closed},
    /// fingerprints{level_ref → fingerprint}, provider_drift}`.
    pub const DRIFT_BRACKET: &str = "measurement.experiment.drift_bracket";
    /// `measurement.experiment.bundle_assembled` — `{bundle_id, kind}`.
    pub const BUNDLE_ASSEMBLED: &str = "measurement.experiment.bundle_assembled";
    /// `measurement.analysis.recorded` — the producer contract's analysis
    /// record stamp on the experiment run (§6.5 §2.3 `record_analysis`;
    /// §6.4 §6 audit events — ADR-0162 D3/D6).
    pub const ANALYSIS_RECORDED: &str = "measurement.analysis.recorded";
}

/// The closed `pause` reason set (§6.3 §2.2; ADR-0155 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PauseReason {
    /// `budget_exhausted{node}` — the pool cannot fund the next slice.
    BudgetExhausted,
    /// `infrastructure_suspected` — the re-attempt fraction tripped.
    InfrastructureSuspected,
    /// `operator` — the attended pause (the only pause that may cancel
    /// in-flight runs).
    Operator,
    /// `drift_detected` — a fingerprint probe observed a change.
    DriftDetected,
}

impl PauseReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PauseReason::BudgetExhausted => "budget_exhausted",
            PauseReason::InfrastructureSuspected => "infrastructure_suspected",
            PauseReason::Operator => "operator",
            PauseReason::DriftDetected => "drift_detected",
        }
    }

    /// Parse; `None` on any other spelling.
    pub fn parse(s: &str) -> Option<PauseReason> {
        match s {
            "budget_exhausted" => Some(PauseReason::BudgetExhausted),
            "infrastructure_suspected" => Some(PauseReason::InfrastructureSuspected),
            "operator" => Some(PauseReason::Operator),
            "drift_detected" => Some(PauseReason::DriftDetected),
            _ => None,
        }
    }
}

/// The closed `run_excluded` reason set (§06.3 `exclude`; ADR-0162 D5).
pub mod exclude_reason {
    /// A run superseded for an infrastructure re-attempt.
    pub const INFRASTRUCTURE_RETRY: &str = "infrastructure_retry";
    /// A duplicate attempt on an already-settled plan.
    pub const DUPLICATE_ATTEMPT: &str = "duplicate_attempt";
    /// Superseded by a fork lineage.
    pub const SUPERSEDED_BY_FORK: &str = "superseded_by_fork";
    /// A pre-registered exclusion.
    pub const PRE_REGISTERED_EXCLUSION: &str = "pre_registered_exclusion";
    /// The arm's match failed at settle.
    pub const BUDGET_UNMATCHED: &str = "budget_unmatched";
    /// Consent withdrawn.
    pub const CONSENT_WITHDRAWN: &str = "consent_withdrawn";
    /// An analyst exclusion (`origin = human` required).
    pub const ANALYST_EXCLUSION: &str = "analyst_exclusion";
}

// ── Payload builders ────────────────────────────────────────────────────────

/// `declared{experiment_id, plan_id, kind, comparable, suite_manifest_ref,
/// registry_snapshot_id?, arms[], n_cells, n_run_plans, opened_ms}` — the
/// declaration row. `opened_ms` is the open wall-clock stamp the
/// `start_stagger_ms` schedule offsets from (a ledger fact — S-1). `experiment_id`/`plan_id` *are* the content addresses of the spec and
/// `CellPlan` documents (LabDocs); the documents are not inlined — Rule C
/// bounds audit members at 512 canonical bytes each / the class offload
/// threshold in total, and a full `CellPlan` dwarfs that at scale. The fold
/// keeps the refs; the engine reloads the documents from `LabDocs`.
pub fn declared(spec: &ExperimentSpec, plan: &CellPlan, opened_ms: u64) -> Json {
    let mut m = BTreeMap::new();
    m.insert("experiment_id".into(), Json::str(&spec.experiment_id));
    m.insert("plan_id".into(), Json::str(&plan.plan_id));
    m.insert("kind".into(), Json::str(spec.kind.name()));
    m.insert("comparable".into(), Json::Bool(spec.kind.requires_match()));
    m.insert(
        "suite_manifest_ref".into(),
        Json::str(&spec.suite.suite_ref),
    );
    if let Some(snap) = &spec.design.registry_snapshot_id {
        m.insert("registry_snapshot_id".into(), Json::str(snap));
    }
    m.insert(
        "arms".into(),
        Json::Arr(spec.arms.iter().map(|a| Json::str(&a.arm_id)).collect()),
    );
    m.insert("n_cells".into(), Json::Int(plan.cells.len() as i64));
    m.insert("n_run_plans".into(), Json::Int(plan.run_plans.len() as i64));
    m.insert("opened_ms".into(), Json::Int(opened_ms as i64));
    Json::Obj(m)
}

/// `run_planned{run_plan_id, cell_id, arm_id, task_id, replicate_index,
/// order_pos, seed_material, configuration_version_id, split_label}` — the
/// fold's scheduling record (S-6: `order_pos` is derived from
/// `permutation_seed`; recording it makes the order an auditable ledger fact).
pub fn run_planned(
    rp: &RunPlan,
    arm_id: &str,
    task_id: &str,
    split_label: &str,
    configuration_version_id: &str,
    order_pos: u64,
) -> Json {
    Json::obj([
        ("run_plan_id", Json::str(&rp.run_plan_id)),
        ("cell_id", Json::str(&rp.cell_id)),
        ("arm_id", Json::str(arm_id)),
        ("task_id", Json::str(task_id)),
        ("replicate_index", Json::Int(rp.replicate_index as i64)),
        ("order_pos", Json::Int(order_pos as i64)),
        ("seed_material", rp.seed_material.clone()),
        (
            "configuration_version_id",
            Json::str(configuration_version_id),
        ),
        ("split_label", Json::str(split_label)),
    ])
}

/// `run_claimed{run_plan_id, holder, lease_id, expires_at_ms}`.
pub fn run_claimed(run_plan_id: &str, holder: &str, lease_id: &str, expires_at_ms: u64) -> Json {
    Json::obj([
        ("run_plan_id", Json::str(run_plan_id)),
        ("holder", Json::str(holder)),
        ("lease_id", Json::str(lease_id)),
        ("expires_at_ms", Json::Int(expires_at_ms as i64)),
    ])
}

/// `claim_expired{run_plan_id, lease_id}`.
pub fn claim_expired(run_plan_id: &str, lease_id: &str) -> Json {
    Json::obj([
        ("run_plan_id", Json::str(run_plan_id)),
        ("lease_id", Json::str(lease_id)),
    ])
}

/// The launch-time dispatch stamps `run_launched` records (§6.3 §2.2;
/// AC-R-2.10.3-3/-10/-14) — the consumed `SchedulingPolicy` pool keys and
/// the participant-class/`limits_enforced` stamps the bound run carries.
#[derive(Debug, Clone, Default)]
pub struct LaunchStamp<'a> {
    /// The pool keys this dispatch consumes (`run_launched{pool_consumed}`).
    pub pool_consumed: &'a [String],
    /// `native | hosted` — the subject run's participant class.
    pub participant_class: &'a str,
    /// The derived `limits_enforced` stamp (`full | partial | none`) —
    /// the enforcement the dispatch actually carries, never the spec's
    /// unverified claim (ADR-0046 (d)).
    pub limits_enforced: &'a str,
    /// The hosting adapter's session record ref (hosted launches).
    pub hosted_session_ref: Option<&'a str>,
}

/// `run_launched{run_plan_id, run_id, attempt_no, budget_id,
/// pool_consumed[], participant_class, limits_enforced,
/// hosted_session_ref?}` — additive members are the C1 dispatch record.
pub fn run_launched(
    run_plan_id: &str,
    run_id: &str,
    attempt_no: u32,
    budget_id: &str,
    stamp: &LaunchStamp<'_>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("run_plan_id".into(), Json::str(run_plan_id));
    m.insert("run_id".into(), Json::str(run_id));
    m.insert("attempt_no".into(), Json::Int(attempt_no as i64));
    m.insert("budget_id".into(), Json::str(budget_id));
    m.insert(
        "pool_consumed".into(),
        Json::Arr(stamp.pool_consumed.iter().map(Json::str).collect()),
    );
    m.insert(
        "participant_class".into(),
        Json::str(stamp.participant_class),
    );
    m.insert("limits_enforced".into(), Json::str(stamp.limits_enforced));
    if let Some(s) = stamp.hosted_session_ref {
        m.insert("hosted_session_ref".into(), Json::str(s));
    }
    Json::Obj(m)
}

/// The subject run's binding row — `run_bound{experiment_id, arm_id,
/// configuration_id, configuration_version_id, cell_id, replicate_index,
/// attempt_no, budget_id}` (§6.3; stamped on the subject run).
#[allow(clippy::too_many_arguments)]
pub fn subject_bound(
    experiment_id: &str,
    arm_id: &str,
    configuration_id: &str,
    configuration_version_id: &str,
    cell_id: &str,
    replicate_index: u32,
    attempt_no: u32,
    budget_id: &str,
    participant_class: &str,
    limits_enforced: &str,
) -> Json {
    Json::obj([
        ("experiment_id", Json::str(experiment_id)),
        ("arm_id", Json::str(arm_id)),
        ("configuration_id", Json::str(configuration_id)),
        (
            "configuration_version_id",
            Json::str(configuration_version_id),
        ),
        ("cell_id", Json::str(cell_id)),
        ("replicate_index", Json::Int(replicate_index as i64)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("budget_id", Json::str(budget_id)),
        ("participant_class", Json::str(participant_class)),
        ("limits_enforced", Json::str(limits_enforced)),
    ])
}

/// The experiment run's mirror — `run_bound{run_id, run_plan_id, cell_id,
/// replicate_index, attempt_no, subject_head}` (§6.3 `bind`; the subject's own
/// `bound` chain is the proof).
#[allow(clippy::too_many_arguments)] // the mirror row's fields are the record's shape.
pub fn run_bound_mirror(
    run_id: &str,
    run_plan_id: &str,
    cell_id: &str,
    replicate_index: u32,
    attempt_no: u32,
    subject_head: &str,
    participant_class: &str,
    limits_enforced: &str,
) -> Json {
    Json::obj([
        ("run_id", Json::str(run_id)),
        ("run_plan_id", Json::str(run_plan_id)),
        ("cell_id", Json::str(cell_id)),
        ("replicate_index", Json::Int(replicate_index as i64)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("subject_head", Json::str(subject_head)),
        ("participant_class", Json::str(participant_class)),
        ("limits_enforced", Json::str(limits_enforced)),
    ])
}

/// `run_settled{run_plan_id, run_id, outcome_class, accepted, superseded,
/// attempt_no, budget_utilization, veto_tripped[], regrade}` — the §2.3
/// record. `regrade = true` marks an `oracle_failure` settlement: the row is
/// final for the plan but awaits a regrade overlay — the subject is never
/// re-run (AC-R-2.10.3-6; ADR-0155 D5).
pub fn run_settled(outcome: &RunOutcome, pool_released: &[String]) -> Json {
    Json::obj([
        ("run_plan_id", Json::str(&outcome.run_plan_id)),
        ("run_id", Json::str(&outcome.run_id)),
        ("outcome_class", Json::str(outcome.outcome_class.as_str())),
        ("accepted", Json::Bool(outcome.accepted)),
        ("superseded", Json::Bool(outcome.superseded)),
        ("plan_final", Json::Bool(outcome.plan_final)),
        ("attempt_no", Json::Int(outcome.attempt_no as i64)),
        ("budget_utilization", outcome.budget_utilization.clone()),
        (
            "veto_tripped",
            Json::Arr(outcome.veto_tripped.iter().map(Json::str).collect()),
        ),
        ("regrade", Json::Bool(outcome.regrade_pending)),
        (
            "pool_released",
            Json::Arr(pool_released.iter().map(Json::str).collect()),
        ),
    ])
}

/// `run_excluded{run_plan_id, run_id, reason, evidence?, authority}`.
pub fn run_excluded(
    run_plan_id: &str,
    run_id: &str,
    reason: &str,
    evidence: Option<&Json>,
    authority: &str,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("run_plan_id".into(), Json::str(run_plan_id));
    m.insert("run_id".into(), Json::str(run_id));
    m.insert("reason".into(), Json::str(reason));
    if let Some(e) = evidence {
        m.insert("evidence".into(), e.clone());
    }
    m.insert("authority".into(), Json::str(authority));
    Json::Obj(m)
}

/// `run_replanned{run_plan_id, superseded_run_id, attempt_no, not_before_ms}`
/// — the plan returns to eligible at `attempt_no`, no earlier than
/// `not_before_ms` (the §2.3 re-attempt backoff, recorded as a ledger fact).
pub fn run_replanned(
    run_plan_id: &str,
    superseded_run_id: &str,
    attempt_no: u32,
    not_before_ms: u64,
) -> Json {
    Json::obj([
        ("run_plan_id", Json::str(run_plan_id)),
        ("superseded_run_id", Json::str(superseded_run_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("not_before_ms", Json::Int(not_before_ms as i64)),
    ])
}

/// `cell_completed{cell_id, accepted, planned}`.
pub fn cell_completed(cell_id: &str, accepted: u32, planned: u32) -> Json {
    Json::obj([
        ("cell_id", Json::str(cell_id)),
        ("accepted", Json::Int(accepted as i64)),
        ("planned", Json::Int(planned as i64)),
    ])
}

/// `paused{reason, node?, probe?}` — `node` names the exhausted budget
/// node for `budget_exhausted`; `probe` is the boundary's pause tag (the
/// §6.3 `pause{probe?}` member — a free-form audit label).
pub fn paused(reason: PauseReason, node: Option<&str>, probe: Option<&str>) -> Json {
    let mut m = BTreeMap::new();
    m.insert("reason".into(), Json::str(reason.as_str()));
    if let Some(n) = node {
        m.insert("node".into(), Json::str(n));
    }
    if let Some(t) = probe {
        m.insert("probe".into(), Json::str(t));
    }
    Json::Obj(m)
}

/// `resumed{probe?}` — the boundary's resume tag.
pub fn resumed(probe: Option<&str>) -> Json {
    match probe {
        Some(t) => Json::obj([("probe", Json::str(t))]),
        None => Json::obj([]),
    }
}

/// `drift_bracket{phase, fingerprints, provider_drift}` — `fingerprints` is
/// `{level_ref → fingerprint}`; `provider_drift` is `true` only on the
/// `closed` bracket when a fingerprint moved (OQ-362 ratified default:
/// annotate, never delete).
pub fn drift_bracket(
    phase: &str,
    fingerprints: &BTreeMap<String, String>,
    provider_drift: bool,
) -> Json {
    Json::obj([
        ("phase", Json::str(phase)),
        (
            "fingerprints",
            Json::Obj(
                fingerprints
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v)))
                    .collect(),
            ),
        ),
        ("provider_drift", Json::Bool(provider_drift)),
    ])
}

/// `closed{status, coverage, outcome_counts, watermark_set, summary_ref?,
/// under_utilised[], budget_match[], na_cells[]}` — the E-4 re-check lands
/// on the closed row (§6.3 §2.4; additive members, CC8).
pub fn closed(report: &ExperimentReport) -> Json {
    let mut m = BTreeMap::new();
    m.insert("status".into(), Json::str(report.status.as_str()));
    m.insert("coverage".into(), report.coverage.clone());
    m.insert(
        "outcome_counts".into(),
        Json::Obj(
            report
                .outcome_counts
                .iter()
                .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                .collect(),
        ),
    );
    m.insert("watermark_set".into(), report.watermark_set.clone());
    if let Some(s) = &report.summary_ref {
        m.insert("summary_ref".into(), Json::str(s));
    }
    m.insert(
        "under_utilised".into(),
        Json::Arr(report.under_utilised.iter().map(Json::str).collect()),
    );
    m.insert(
        "budget_match".into(),
        Json::Arr(report.budget_match.clone()),
    );
    m.insert("na_cells".into(), Json::Arr(report.na_cells.clone()));
    m.insert("utilization".into(), report.utilization.clone());
    Json::Obj(m)
}

/// `bundle_assembled{bundle_id, kind}`.
pub fn bundle_assembled(bundle_id: &str) -> Json {
    Json::obj([
        ("bundle_id", Json::str(bundle_id)),
        ("kind", Json::str("experiment")),
    ])
}

/// `amended{diff_ref, reason, authority}`.
pub fn amended(diff_ref: &str, reason: &str, authority: &str) -> Json {
    Json::obj([
        ("diff_ref", Json::str(diff_ref)),
        ("reason", Json::str(reason)),
        ("authority", Json::str(authority)),
    ])
}

/// `measurement.analysis.recorded{analysis_id, report, kind,
/// pre_registered, registered_analysis_ref?, post_amendment}` — the
/// producer contract's analysis stamp on the experiment run (§6.5 §2.3;
/// §6.4 §6's `measurement.analysis.recorded` audit event; ADR-0162 D3).
/// Members are the `AnalysisRecord`'s own facts — the row is the record's
/// ledger presence, never a copy of its body.
pub fn analysis_recorded(record: &hh_lab::analysis::AnalysisRecord) -> Json {
    let mut m = BTreeMap::new();
    m.insert("analysis_id".to_string(), Json::str(&record.analysis_id));
    m.insert(
        "report".to_string(),
        record.report.as_ref().map_or(Json::Null, Json::str),
    );
    m.insert(
        "kind".to_string(),
        record.kind.as_ref().map_or(Json::Null, Json::str),
    );
    m.insert(
        "pre_registered".to_string(),
        Json::Bool(record.pre_registered),
    );
    if let Some(r) = &record.registered_analysis_ref {
        m.insert("registered_analysis_ref".to_string(), Json::str(r));
    }
    m.insert(
        "post_amendment".to_string(),
        Json::Bool(record.post_amendment),
    );
    Json::Obj(m)
}

// ── Boundary records ────────────────────────────────────────────────────────

/// `RunOutcome{run_plan_id, run_id, outcome_class, accepted, superseded,
/// attempt_no, budget_utilization, veto_tripped[]}` — the `settle` result
/// (§6.3 data model; crosses the boundary as a whole record).
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutcome {
    /// The plan settled.
    pub run_plan_id: String,
    /// The subject run.
    pub run_id: String,
    /// The derived outcome class (ADR-0106/0155 D5).
    pub outcome_class: OutcomeClass,
    /// Whether the run is the plan's accepted replicate (S-2).
    pub accepted: bool,
    /// Whether the run was superseded (infrastructure retry path).
    pub superseded: bool,
    /// Whether the plan is terminally done without acceptance (a final
    /// cancellation, or the re-attempt budget exhausted without a replan).
    pub plan_final: bool,
    /// The attempt ordinal.
    pub attempt_no: u32,
    /// The budget utilisation record (`{dim → ppm of cap}`).
    pub budget_utilization: Json,
    /// Veto trips (annotate only — never change acceptance; ADR-0047).
    pub veto_tripped: Vec<String>,
    /// Whether the plan was re-planned (returned to eligible).
    pub replanned: bool,
    /// Whether the experiment paused as a side effect.
    pub paused: Option<String>,
    /// `oracle_failure` rows carry `regrade_pending = true` — the settlement
    /// is final for the plan and a regrade overlay follows; the subject is
    /// never re-run (AC-R-2.10.3-6; ADR-0155 D5).
    pub regrade_pending: bool,
}

/// The `close` result — `ExperimentReport{status, coverage, outcome_counts,
/// drift_bracket, bundle_id, watermark_set}` (§6.3; ADR-0155 D7).
#[derive(Debug, Clone, PartialEq)]
pub struct ExperimentReport {
    /// `completed | partial | aborted`.
    pub status: CloseStatus,
    /// `{cells_completed, cells_planned, runs_accepted, runs_planned}`.
    pub coverage: Json,
    /// `outcome_class → count`.
    pub outcome_counts: BTreeMap<String, u64>,
    /// `true` when the close probe observed a fingerprint change.
    pub provider_drift: bool,
    /// The assembled bundle id (when the caller assembled one).
    pub bundle_id: Option<String>,
    /// The results watermark set handed to §6.4/§6.5.
    pub watermark_set: Json,
    /// A summary ref (audit surface).
    pub summary_ref: Option<String>,
    /// E-3/E-4: arms whose median utilisation on a matched dimension fell
    /// below the declared `utilization_floor` — annotated `under_utilised`
    /// (ADR-0041 M1; §6.3 §2.4). Additive member.
    pub under_utilised: Vec<String>,
    /// E-4: the comparand-group budget re-check over realised runs —
    /// `[{arms, status ∈ {matched, imbalanced, unmatched}, tolerance_ppm,
    /// detail}]` (OQ-124 tolerance; §6.3 §2.4). Additive member.
    pub budget_match: Vec<Json>,
    /// E-4: cells rendering a typed `n/a{reason}` —
    /// `{cell_id, reason ∈ {level_ineligible, not_run}}` (`not_run` =
    /// `InsufficientReplicates` over realised runs). Additive member.
    pub na_cells: Vec<Json>,
    /// E-3: the realised utilisation distributions —
    /// `{arm_id → {dim → {median, min, max, n}}}` in ppm of the slice cap
    /// over counted runs (ADR-0041 M1). Additive member.
    pub utilization: Json,
}

/// `closed.status ∈ {completed, partial, aborted}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CloseStatus {
    /// Every eligible plan settled accepted.
    Completed,
    /// `partial = true` declared — the coverage row is honest about the gap.
    Partial,
    /// Aborted before coverage.
    Aborted,
}

impl CloseStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CloseStatus::Completed => "completed",
            CloseStatus::Partial => "partial",
            CloseStatus::Aborted => "aborted",
        }
    }
}
