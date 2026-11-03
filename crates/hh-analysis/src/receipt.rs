//! `receipt` — `population-receipt/1`: the §6.1 all-planned closure
//! reconcile over the pinned owner views (R5.23; REQ-F-003/004/008,
//! REQ-E-208; Packet C C.9 checks 2/6).
//!
//! Set-equality reconciles the chain
//! `slots→plans→first/all attempts→settlements/selected grades→rows→
//! analysis`. The *expected* side is derived from the pinned
//! `ExperimentView`/`EvalRun` projections through the R5.15 population
//! contract — [`crate::population::planned_population`] does the
//! contract fold (one member/counting rule, X7; this module never
//! re-implements it). The *declared* side is the records the receipt
//! asserts — records-in; producers pass records in and no crate edge is
//! added to hh-bundle (Packet C C.1 item 6 / C.10).
//!
//! Retention is the contract (§6.1 "retain not-run/refused/pending/
//! orphans/deviations, no retrospective shrink"): a dropped unrun slot
//! is `dropped_slot`, never a smaller denominator; an unbound record is
//! `orphan_launch`, never silently joined. A reconcile mismatch is a
//! typed [`ReceiptFailure`] + `verdict: failed` — a recorded finding,
//! not a thrown error; contract refusals (`PopulationError`) propagate
//! verbatim. Per slot the four §6.1 axes stay separate — prerequisite,
//! invocation, task, veto — a finished invocation never erases a
//! skipped/format failure and never promotes an evidence class: the
//! record carries the fixed [`EVIDENCE_CLASS`] label only.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::population::PlannedPopulation;
use hharness_eval::runs::{EvalRun, TaskContext};
use hharness_experiment::view::ExperimentView;
use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

use crate::error::AnalysisError;

/// The one receipt schema id.
pub const RECEIPT_SCHEMA: &str = "population-receipt/1";
/// The admitted record version.
pub const RECEIPT_VERSION: u64 = 1;
/// The fixed evidence label — a derived index, never a promoted class
/// (AC-2: no evidence-class promotion from hash or process success).
pub const EVIDENCE_CLASS: &str = "derived_index";

/// The closed failure-code set [`ReceiptFailure::code`] carries.
pub mod code {
    /// A frozen/universe slot is absent from the declared set —
    /// not-run/refused/pending members are still required members.
    pub const DROPPED_SLOT: &str = "dropped_slot";
    /// A declared slot outside the frozen universe — extras make no
    /// producer decisions.
    pub const EXTRA_SLOT: &str = "extra_slot";
    /// A declared attempt/settlement/grade/row whose run binds no plan.
    pub const ORPHAN_LAUNCH: &str = "orphan_launch";
    /// A bound attempt missing from the declared attempt set.
    pub const DROPPED_ATTEMPT: &str = "dropped_attempt";
    /// A settled attempt with no settlement/cost record.
    pub const MISSING_SETTLEMENT: &str = "missing_settlement";
    /// A settled subject with no row record.
    pub const MISSING_ROW: &str = "missing_row";
    /// A repeated grade_ref or a second counting grade on one attempt.
    pub const DUPLICATE_GRADE: &str = "duplicate_grade";
    /// A second settlement/row record for one member.
    pub const DUPLICATE_RECORD: &str = "duplicate_record";
    /// `counts_subject` on a retry (`ordinal > 1`) — a grade retry never
    /// adds subject n (I-03).
    pub const GRADE_RETRY_ADDS_N: &str = "grade_retry_adds_n";
    /// `counts_subject`/`Selected` on a skipped, format-failure,
    /// unavailable or pending grade — a process pass never erases it.
    pub const GRADE_STATUS: &str = "grade_status_violation";
    /// A selected grade reading a non-current frozen selection ref.
    pub const STALE_SELECTION: &str = "stale_selection";
    /// A settlement reading a non-current cost-view version.
    pub const STALE_COST_VERSION: &str = "stale_cost_version";
    /// A declared record joined to the wrong member/plan, or a record
    /// for an unsettled member.
    pub const WRONG_JOIN: &str = "wrong_member_join";
    /// A required analysis pin absent — protocol/prices/method/seed/
    /// draws/weights bind exactly or not at all.
    pub const MISSING_PIN: &str = "missing_analysis_pin";
}

/// `prerequisite` — whether a slot's bound prerequisites are on record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisVerdict {
    /// The bound prerequisite record exists.
    Available,
    /// A binding exists but its prerequisite record released/absent.
    Unavailable,
    /// Nothing bound — prerequisite state unknown.
    Unknown,
}

impl AxisVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AxisVerdict::Available => "available",
            AxisVerdict::Unavailable => "unavailable",
            AxisVerdict::Unknown => "unknown",
        }
    }
}

/// `process` — the invocation axis (never the task verdict).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Nothing bound or launched.
    NotRun,
    /// Bound/launched, nothing settled.
    Started,
    /// A settled outcome is on record.
    Finished,
    /// Invocation state unknown.
    Unknown,
}

impl Invocation {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Invocation::NotRun => "not_run",
            Invocation::Started => "started",
            Invocation::Finished => "finished",
            Invocation::Unknown => "unknown",
        }
    }
}

/// `task` — the semantic axis (a process pass never writes here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskVerdict {
    /// The read member's measure is a known success.
    Pass,
    /// A known failure — wrong-but-valid is a valid zero.
    Fail,
    /// The member's outcome class is `refused`.
    Refused,
    /// The measure is unsupported for the member.
    Unsupported,
    /// The member's grade record is `skipped` — never a pass.
    Skipped,
    /// Unresolved/format-failure/pending — truth unknown.
    Unknown,
}

impl TaskVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskVerdict::Pass => "pass",
            TaskVerdict::Fail => "fail",
            TaskVerdict::Refused => "refused",
            TaskVerdict::Unsupported => "unsupported",
            TaskVerdict::Skipped => "skipped",
            TaskVerdict::Unknown => "unknown",
        }
    }
}

/// `veto` — the observed-veto axis (never folded into the class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetoVerdict {
    /// No veto observed.
    None,
    /// ≥1 veto tripped (`veto_ids` carries them).
    Observed,
    /// Required checks missing — veto state unknown.
    Unknown,
}

impl VetoVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            VetoVerdict::None => "none",
            VetoVerdict::Observed => "observed",
            VetoVerdict::Unknown => "unknown",
        }
    }
}

/// The presented status of one declared grade record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeStatus {
    /// The frozen selection's winning grade.
    Selected,
    /// Grading admitted, verdict withheld (`grade_pending`).
    Pending,
    /// Admissible evidence exhausted, no valid grade.
    Unavailable,
    /// The oracle/verdict was skipped — never a pass.
    Skipped,
    /// The grade record failed format/parse — unavailable, never zero.
    FormatFailure,
}

impl GradeStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            GradeStatus::Selected => "selected",
            GradeStatus::Pending => "pending",
            GradeStatus::Unavailable => "unavailable",
            GradeStatus::Skipped => "skipped",
            GradeStatus::FormatFailure => "format_failure",
        }
    }
    /// Whether the status may carry `counts_subject`/`selected` —
    /// only `Selected` may; a process pass never erases the rest.
    fn selectable(self) -> bool {
        matches!(self, GradeStatus::Selected)
    }
}

/// One declared settlement/cost record — budget/telemetry owner
/// (records-in). `cost_version` is the pinned cost-view version the
/// settlement read at; a stale version fails `stale_cost_version`.
#[derive(Debug, Clone, PartialEq)]
pub struct SettlementDecl {
    /// The subject run the settlement binds.
    pub run_id: String,
    /// The durable settlement ref.
    pub settlement_ref: String,
    /// The cost-view version pin the record read at.
    pub cost_version: String,
}

/// One declared grade record — eval/bench selection owner (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct GradeDecl {
    /// The subject run (attempt) the grade binds.
    pub run_id: String,
    /// The grade-attempt identity.
    pub grade_ref: String,
    /// The frozen selection ref the grade read under — a selected grade
    /// reading anything but the pinned selection fails
    /// `stale_selection`.
    pub selection_ref: String,
    /// The grade-attempt ordinal (1-based; >1 names a regrade).
    pub ordinal: u64,
    /// Whether the grade contributes subject n — only the first
    /// counting grade of an attempt may (I-03).
    pub counts_subject: bool,
    /// The presented status.
    pub status: GradeStatus,
}

/// One declared results-row record — results owner (records-in). The
/// row asserts the `plan_ref` it joins; a wrong plan fails
/// `wrong_member_join`.
#[derive(Debug, Clone, PartialEq)]
pub struct RowDecl {
    /// The subject run the row projects.
    pub run_id: String,
    /// The `RowVersion` version_id at the receipt's pins.
    pub version_id: String,
    /// The plan the row claims to join.
    pub plan_ref: String,
}

/// The analysis declaration — the `rows→analysis` leg's exact pins.
/// `protocol_ref`/`spec_ref`/`record_ref`/`report_ref` may be absent
/// (declared, never fabricated); `prices`, `method`, `weights`,
/// `seed`, `draws` are required — an absent required member fails
/// `missing_analysis_pin`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisDecl {
    /// The pinned protocol/hash the analysis ran under (`None` only
    /// when the declaration honestly has none).
    pub protocol_ref: Option<String>,
    /// `{price_table_version, confidence, …}` — exact.
    pub prices: Json,
    /// `{estimator, version, assumptions?, prior?}` — exact.
    pub method: Json,
    /// The declared seed.
    pub seed: u64,
    /// The actual requested draws.
    pub draws: u64,
    /// The frozen task-weight declaration (`w_t = 1/T` family).
    pub weights: Json,
    /// The spec ref the analysis ran.
    pub spec_ref: Option<String>,
    /// The persisted `AnalysisRecord` ref.
    pub record_ref: Option<String>,
    /// The persisted `AnalysisReport` ref.
    pub report_ref: Option<String>,
}

impl Default for AnalysisDecl {
    fn default() -> Self {
        AnalysisDecl {
            protocol_ref: None,
            prices: Json::Null,
            method: Json::Null,
            seed: 0,
            draws: 0,
            weights: Json::Null,
            spec_ref: None,
            record_ref: None,
            report_ref: None,
        }
    }
}

/// The declared side — the records the receipt asserts. An empty
/// `slots`/`attempts` set reads as *partial* (the declared set is the
/// derived set — the same convention `PopulationInput.required`
/// carries); a non-empty one reconciles set-for-set.
#[derive(Debug, Clone, Default)]
pub struct DeclaredReceipt {
    /// The frozen slot set the receipt binds — every universe member,
    /// not-run/refused/pending included (§6.1 retention).
    pub slots: BTreeSet<String>,
    /// `plan_ref → declared attempt run ids` (first + all attempts).
    pub attempts: BTreeMap<String, Vec<String>>,
    /// Settlement/cost records.
    pub settlements: Vec<SettlementDecl>,
    /// Grade records.
    pub grades: Vec<GradeDecl>,
    /// Row records.
    pub rows: Vec<RowDecl>,
    /// The pinned cost-view version every settlement must read at.
    pub cost_version: String,
    /// The pinned frozen selection ref every selected grade must read.
    pub selection_ref: String,
    /// The analysis declaration (`rows→analysis`).
    pub analysis: AnalysisDecl,
}

/// One reconcile leg's record — expected members, joined members, and
/// the failure codes the leg collected.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileLeg {
    /// The leg (`slots_to_plans`, `plans_to_attempts`,
    /// `attempts_to_settlements`, `attempts_to_grades`,
    /// `attempts_to_rows`, `rows_to_analysis`).
    pub name: &'static str,
    /// The expected member ids (sorted).
    pub expected: Vec<String>,
    /// The member ids a declared record joined (sorted).
    pub joined: Vec<String>,
    /// The failure codes raised on this leg (ordered by detection).
    pub failures: Vec<&'static str>,
}

/// One slot's four-axis verdict record (§6.1 — the axes stay
/// separate; nothing folds).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotVerdicts {
    /// The plan coordinate.
    pub plan_ref: String,
    /// The read member (`None` = unresolved member — retained row).
    pub member: Option<String>,
    /// `true` when `member` is a first-admissible-retry recovery.
    pub recovered: bool,
    /// Every bound attempt (all attempts retained).
    pub all_attempts: Vec<String>,
    /// The prerequisite axis.
    pub prerequisite: AxisVerdict,
    /// The invocation/process axis.
    pub invocation: Invocation,
    /// The semantic task axis.
    pub task: TaskVerdict,
    /// The veto axis.
    pub veto: VetoVerdict,
    /// The observed veto ids when `veto == Observed`.
    pub veto_ids: Vec<String>,
}

/// A typed reconcile failure — `code` is a member of [`code`].
#[derive(Debug, Clone, PartialEq)]
pub struct ReceiptFailure {
    /// The leg that raised it.
    pub leg: &'static str,
    /// The closed code.
    pub code: &'static str,
    /// The offending member/detail.
    pub detail: String,
}

/// `population-receipt/1` — the derived index. `population` is the
/// R5.15 contract fold verbatim (the three named populations beside
/// the class/veto/availability axes); `legs`/`slots`/`failures` are
/// this module's reconcile. `verdict` is `closed` only when
/// `failures` is empty.
#[derive(Debug, Clone)]
pub struct PopulationReceipt {
    /// The contract fold (member resolution + populations).
    pub population: PlannedPopulation,
    /// The six reconcile legs, in chain order.
    pub legs: Vec<ReconcileLeg>,
    /// One verdict record per universe slot.
    pub slots: Vec<SlotVerdicts>,
    /// Every typed reconcile failure.
    pub failures: Vec<ReceiptFailure>,
    /// The bound analysis declaration (republished verbatim).
    pub analysis: AnalysisDecl,
    /// `closed | failed`.
    pub verdict: &'static str,
}

impl PopulationReceipt {
    /// Canonical JSON — the `population-receipt/1` record.
    pub fn to_json(&self) -> Json {
        let leg = |l: &ReconcileLeg| {
            Json::obj([
                ("name", Json::str(l.name)),
                (
                    "expected",
                    Json::Arr(l.expected.iter().map(Json::str).collect()),
                ),
                (
                    "joined",
                    Json::Arr(l.joined.iter().map(Json::str).collect()),
                ),
                (
                    "failures",
                    Json::Arr(l.failures.iter().map(|s| Json::str(*s)).collect()),
                ),
            ])
        };
        let slot = |s: &SlotVerdicts| {
            Json::obj([
                ("plan_ref", Json::str(&s.plan_ref)),
                (
                    "member",
                    s.member.clone().map(Json::str).unwrap_or(Json::Null),
                ),
                ("recovered", Json::Bool(s.recovered)),
                (
                    "all_attempts",
                    Json::Arr(s.all_attempts.iter().map(Json::str).collect()),
                ),
                ("prerequisite", Json::str(s.prerequisite.as_str())),
                ("invocation", Json::str(s.invocation.as_str())),
                ("task", Json::str(s.task.as_str())),
                ("veto", Json::str(s.veto.as_str())),
                (
                    "veto_ids",
                    Json::Arr(s.veto_ids.iter().map(Json::str).collect()),
                ),
            ])
        };
        let failure = |f: &ReceiptFailure| {
            Json::obj([
                ("leg", Json::str(f.leg)),
                ("code", Json::str(f.code)),
                ("detail", Json::str(&f.detail)),
            ])
        };
        let pops = Json::Obj(
            self.population
                .populations
                .iter()
                .map(|(k, c)| {
                    (
                        k.clone(),
                        Json::obj([
                            ("s", Json::Int(c.s as i64)),
                            ("f", Json::Int(c.f as i64)),
                            ("u", Json::Int(c.u as i64)),
                            ("vetoed", Json::Int(c.vetoed as i64)),
                            ("vetoed_passes", Json::Int(c.vetoed_passes as i64)),
                            ("missing_checks", Json::Int(c.missing_checks as i64)),
                        ]),
                    )
                })
                .collect(),
        );
        let counts = |m: &BTreeMap<String, u64>| {
            Json::Obj(
                m.iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            )
        };
        Json::obj([
            ("schema", Json::str(RECEIPT_SCHEMA)),
            ("version", Json::Int(RECEIPT_VERSION as i64)),
            ("evidence_class", Json::str(EVIDENCE_CLASS)),
            ("verdict", Json::str(self.verdict)),
            (
                "pins",
                Json::obj([
                    (
                        "protocol_ref",
                        self.analysis
                            .protocol_ref
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    ("prices", self.analysis.prices.clone()),
                    ("method", self.analysis.method.clone()),
                    ("seed", Json::Int(self.analysis.seed as i64)),
                    ("draws", Json::Int(self.analysis.draws as i64)),
                    ("weights", self.analysis.weights.clone()),
                    (
                        "spec_ref",
                        self.analysis
                            .spec_ref
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    (
                        "record_ref",
                        self.analysis
                            .record_ref
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    (
                        "report_ref",
                        self.analysis
                            .report_ref
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    (
                        "watermarks",
                        Json::Obj(
                            self.population
                                .watermarks
                                .iter()
                                .map(|(r, s)| (r.clone(), Json::Int(*s as i64)))
                                .collect(),
                        ),
                    ),
                ]),
            ),
            ("legs", Json::Arr(self.legs.iter().map(leg).collect())),
            ("slots", Json::Arr(self.slots.iter().map(slot).collect())),
            (
                "populations",
                Json::obj([
                    ("sets", pops),
                    ("n_slots", Json::Int(self.population.n_slots as i64)),
                    ("n_tasks", Json::Int(self.population.n_tasks as i64)),
                    ("n_attempts", Json::Int(self.population.n_attempts as i64)),
                    (
                        "first_class_counts",
                        counts(&self.population.first_class_counts),
                    ),
                    (
                        "attempt_class_counts",
                        counts(&self.population.attempt_class_counts),
                    ),
                    (
                        "availability_counts",
                        counts(&self.population.availability_counts),
                    ),
                    (
                        "pending_slots",
                        Json::Arr(
                            self.population
                                .pending_slots
                                .iter()
                                .map(Json::str)
                                .collect(),
                        ),
                    ),
                    ("complete", Json::Bool(self.population.complete)),
                ]),
            ),
            (
                "failures",
                Json::Arr(self.failures.iter().map(failure).collect()),
            ),
        ])
    }
}

/// The `run_id → plan_ref` expected binding — every `PlanState.attempts`
/// run plus the `launch_binding` subject (dedup by run id, the same
/// union `planned_population` folds).
fn expected_bindings(view: &ExperimentView) -> BTreeMap<String, String> {
    let mut bound = BTreeMap::new();
    for (rpid, ps) in &view.plans {
        for a in &ps.attempts {
            bound.insert(a.run_id.clone(), rpid.clone());
        }
        if let Some(b) = &ps.launch_binding {
            bound
                .entry(b.subject_run_id.clone())
                .or_insert_with(|| rpid.clone());
        }
    }
    bound
}

/// `assemble_receipt` — derive the all-planned index from the pinned
/// owner views and reconcile the declared records against it. `pins`
/// are the declared read watermarks (`subject_run → seq`) the R5.15
/// contract reads at (a resolved record above its pin is
/// `HistoryLeak`; an unpinned resolved record `WatermarkMissing` —
/// propagated verbatim via `AnalysisError::Population`).
pub fn assemble_receipt(
    view: &ExperimentView,
    runs: &[EvalRun],
    tasks: &[TaskContext],
    metric_ref: &str,
    pins: &BTreeMap<String, u64>,
    declared: &DeclaredReceipt,
) -> Result<PopulationReceipt, AnalysisError> {
    let population = crate::population::planned_population(view, runs, tasks, metric_ref, pins)?;
    let bound = expected_bindings(view);
    let settled: BTreeSet<&str> = view
        .plans
        .values()
        .flat_map(|ps| ps.attempts.iter())
        .filter(|a| a.outcome.is_some())
        .map(|a| a.run_id.as_str())
        .collect();

    let mut failures: Vec<ReceiptFailure> = Vec::new();
    let mut legs: Vec<ReconcileLeg> = Vec::new();
    let fail = |leg: &'static str,
                code: &'static str,
                detail: String,
                legs: &mut Vec<ReconcileLeg>,
                failures: &mut Vec<ReceiptFailure>| {
        failures.push(ReceiptFailure {
            leg,
            code,
            detail: detail.clone(),
        });
        legs.last_mut().unwrap().failures.push(code);
    };

    // ── leg 1: slots→plans ──────────────────────────────────────────
    legs.push(ReconcileLeg {
        name: "slots_to_plans",
        expected: view.plans.keys().cloned().collect(),
        joined: declared.slots.iter().cloned().collect(),
        failures: Vec::new(),
    });
    if !declared.slots.is_empty() {
        for missing in view.plans.keys().filter(|p| !declared.slots.contains(*p)) {
            let res = population.slots.iter().find(|s| s.plan_ref == *missing);
            let state = res.map(|s| s.availability.as_str()).unwrap_or("unresolved");
            fail(
                "slots_to_plans",
                code::DROPPED_SLOT,
                format!("{missing} (declared set dropped a {state} member)"),
                &mut legs,
                &mut failures,
            );
        }
        for extra in declared
            .slots
            .iter()
            .filter(|p| !view.plans.contains_key(*p))
        {
            fail(
                "slots_to_plans",
                code::EXTRA_SLOT,
                extra.clone(),
                &mut legs,
                &mut failures,
            );
        }
    }

    // ── leg 2: plans→attempts ───────────────────────────────────────
    {
        let expected: Vec<String> = bound.keys().cloned().collect();
        let joined: Vec<String> = declared.attempts.values().flatten().cloned().collect();
        legs.push(ReconcileLeg {
            name: "plans_to_attempts",
            expected,
            joined,
            failures: Vec::new(),
        });
    }
    if !declared.attempts.is_empty() {
        for (plan, runs_decl) in &declared.attempts {
            if !view.plans.contains_key(plan) {
                fail(
                    "plans_to_attempts",
                    code::EXTRA_SLOT,
                    format!("attempts declared under non-plan {plan}"),
                    &mut legs,
                    &mut failures,
                );
            }
            for run in runs_decl {
                match bound.get(run) {
                    None => fail(
                        "plans_to_attempts",
                        code::ORPHAN_LAUNCH,
                        run.clone(),
                        &mut legs,
                        &mut failures,
                    ),
                    Some(owner) if owner != plan => fail(
                        "plans_to_attempts",
                        code::WRONG_JOIN,
                        format!("{run} binds {owner}, declared under {plan}"),
                        &mut legs,
                        &mut failures,
                    ),
                    _ => {}
                }
            }
        }
        for (run, plan) in &bound {
            let listed = declared
                .attempts
                .get(plan)
                .map(|rs| rs.iter().any(|r| r == run))
                .unwrap_or(false);
            if !listed {
                fail(
                    "plans_to_attempts",
                    code::DROPPED_ATTEMPT,
                    format!("{plan}:{run}"),
                    &mut legs,
                    &mut failures,
                );
            }
        }
    }

    // ── leg 3: attempts→settlements ─────────────────────────────────
    {
        legs.push(ReconcileLeg {
            name: "attempts_to_settlements",
            expected: settled.iter().map(|s| s.to_string()).collect(),
            joined: declared
                .settlements
                .iter()
                .map(|s| s.run_id.clone())
                .collect(),
            failures: Vec::new(),
        });
    }
    let mut seen_settle: BTreeSet<&str> = BTreeSet::new();
    for s in &declared.settlements {
        if !bound.contains_key(&s.run_id) {
            fail(
                "attempts_to_settlements",
                code::ORPHAN_LAUNCH,
                s.run_id.clone(),
                &mut legs,
                &mut failures,
            );
            continue;
        }
        if !seen_settle.insert(s.run_id.as_str()) {
            fail(
                "attempts_to_settlements",
                code::DUPLICATE_RECORD,
                s.run_id.clone(),
                &mut legs,
                &mut failures,
            );
        }
        if !settled.contains(s.run_id.as_str()) {
            fail(
                "attempts_to_settlements",
                code::WRONG_JOIN,
                format!(
                    "settlement {} for unsettled attempt {}",
                    s.settlement_ref, s.run_id
                ),
                &mut legs,
                &mut failures,
            );
        }
        if !declared.cost_version.is_empty() && s.cost_version != declared.cost_version {
            fail(
                "attempts_to_settlements",
                code::STALE_COST_VERSION,
                format!(
                    "{}: {} ≠ pinned {}",
                    s.run_id, s.cost_version, declared.cost_version
                ),
                &mut legs,
                &mut failures,
            );
        }
    }
    for run in &settled {
        if !seen_settle.contains(run) {
            fail(
                "attempts_to_settlements",
                code::MISSING_SETTLEMENT,
                (*run).to_string(),
                &mut legs,
                &mut failures,
            );
        }
    }

    // ── leg 4: attempts→selected grades ─────────────────────────────
    {
        legs.push(ReconcileLeg {
            name: "attempts_to_grades",
            expected: settled.iter().map(|s| s.to_string()).collect(),
            joined: declared.grades.iter().map(|g| g.run_id.clone()).collect(),
            failures: Vec::new(),
        });
    }
    let mut seen_grade_refs: BTreeSet<&str> = BTreeSet::new();
    let mut counting: BTreeSet<&str> = BTreeSet::new();
    for g in &declared.grades {
        if !bound.contains_key(&g.run_id) {
            fail(
                "attempts_to_grades",
                code::ORPHAN_LAUNCH,
                g.run_id.clone(),
                &mut legs,
                &mut failures,
            );
            continue;
        }
        if !seen_grade_refs.insert(g.grade_ref.as_str()) {
            fail(
                "attempts_to_grades",
                code::DUPLICATE_GRADE,
                g.grade_ref.clone(),
                &mut legs,
                &mut failures,
            );
        }
        if g.counts_subject {
            if g.ordinal > 1 {
                fail(
                    "attempts_to_grades",
                    code::GRADE_RETRY_ADDS_N,
                    format!("{}:{}", g.run_id, g.grade_ref),
                    &mut legs,
                    &mut failures,
                );
            }
            if !counting.insert(g.run_id.as_str()) {
                fail(
                    "attempts_to_grades",
                    code::DUPLICATE_GRADE,
                    format!("second counting grade on {}", g.run_id),
                    &mut legs,
                    &mut failures,
                );
            }
            if !g.status.selectable() {
                fail(
                    "attempts_to_grades",
                    code::GRADE_STATUS,
                    format!("{} counts_subject under {}", g.grade_ref, g.status.as_str()),
                    &mut legs,
                    &mut failures,
                );
            }
        }
        if g.status == GradeStatus::Selected
            && !declared.selection_ref.is_empty()
            && g.selection_ref != declared.selection_ref
        {
            fail(
                "attempts_to_grades",
                code::STALE_SELECTION,
                format!(
                    "{}: {} ≠ pinned {}",
                    g.grade_ref, g.selection_ref, declared.selection_ref
                ),
                &mut legs,
                &mut failures,
            );
        }
    }

    // ── leg 5: attempts→rows ────────────────────────────────────────
    {
        legs.push(ReconcileLeg {
            name: "attempts_to_rows",
            expected: settled.iter().map(|s| s.to_string()).collect(),
            joined: declared.rows.iter().map(|r| r.run_id.clone()).collect(),
            failures: Vec::new(),
        });
    }
    let mut seen_rows: BTreeSet<&str> = BTreeSet::new();
    for r in &declared.rows {
        match bound.get(&r.run_id) {
            None => {
                fail(
                    "attempts_to_rows",
                    code::ORPHAN_LAUNCH,
                    r.run_id.clone(),
                    &mut legs,
                    &mut failures,
                );
                continue;
            }
            Some(plan) if *plan != r.plan_ref => fail(
                "attempts_to_rows",
                code::WRONG_JOIN,
                format!("{} joins {} but binds {}", r.run_id, r.plan_ref, plan),
                &mut legs,
                &mut failures,
            ),
            _ => {}
        }
        if !seen_rows.insert(r.run_id.as_str()) {
            fail(
                "attempts_to_rows",
                code::DUPLICATE_RECORD,
                r.run_id.clone(),
                &mut legs,
                &mut failures,
            );
        }
        if !settled.contains(r.run_id.as_str()) {
            fail(
                "attempts_to_rows",
                code::WRONG_JOIN,
                format!("row {} for unsettled attempt {}", r.version_id, r.run_id),
                &mut legs,
                &mut failures,
            );
        }
    }
    for run in &settled {
        if !seen_rows.contains(run) {
            fail(
                "attempts_to_rows",
                code::MISSING_ROW,
                (*run).to_string(),
                &mut legs,
                &mut failures,
            );
        }
    }

    // ── leg 6: rows→analysis — exact pins, never implied ────────────
    {
        legs.push(ReconcileLeg {
            name: "rows_to_analysis",
            expected: vec![
                "prices".to_string(),
                "method".to_string(),
                "seed".to_string(),
                "draws".to_string(),
                "weights".to_string(),
            ],
            joined: Vec::new(),
            failures: Vec::new(),
        });
        let a = &declared.analysis;
        let non_empty_str = |j: &Json, k: &str| {
            j.get(k)
                .and_then(Json::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        };
        for (member, present) in [
            (
                "prices.price_table_version",
                non_empty_str(&a.prices, "price_table_version"),
            ),
            ("method.estimator", non_empty_str(&a.method, "estimator")),
            ("method.version", non_empty_str(&a.method, "version")),
            ("weights", !matches!(a.weights, Json::Null)),
        ] {
            if !present {
                fail(
                    "rows_to_analysis",
                    code::MISSING_PIN,
                    member.to_string(),
                    &mut legs,
                    &mut failures,
                );
            }
        }
    }

    // ── per-slot verdicts — four separate axes ───────────────────────
    // The member's worst surviving grade status: `Selected` outranks the
    // rest only because a *winning* grade resolved the slot — a skipped
    // or format-failure record beside it stays a non-winning attempt.
    let grade_status_of = |run: &str| -> Option<GradeStatus> {
        declared
            .grades
            .iter()
            .filter(|g| g.run_id == run)
            .map(|g| g.status)
            .max_by_key(|s| match s {
                GradeStatus::Selected => 4,
                GradeStatus::Skipped => 3,
                GradeStatus::FormatFailure => 2,
                GradeStatus::Pending => 1,
                GradeStatus::Unavailable => 0,
            })
    };
    let mut slots = Vec::with_capacity(population.slots.len());
    for res in &population.slots {
        let ps = view.plans.get(&res.plan_ref);
        let bound_runs: Vec<String> = bound
            .iter()
            .filter(|(_, p)| **p == res.plan_ref)
            .map(|(r, _)| r.clone())
            .collect();
        let prerequisite = match ps.and_then(|p| p.launch_binding.as_ref()) {
            Some(b) if b.phase == "aborted" => AxisVerdict::Unavailable,
            _ if bound_runs.is_empty() => AxisVerdict::Unknown,
            _ => AxisVerdict::Available,
        };
        let any_settled = bound_runs.iter().any(|r| settled.contains(r.as_str()));
        let invocation = if bound_runs.is_empty() {
            Invocation::NotRun
        } else if any_settled {
            Invocation::Finished
        } else {
            Invocation::Started
        };
        let task = match res.member.as_deref().and_then(grade_status_of) {
            Some(GradeStatus::Skipped) => TaskVerdict::Skipped,
            Some(GradeStatus::FormatFailure | GradeStatus::Pending | GradeStatus::Unavailable) => {
                TaskVerdict::Unknown
            }
            _ => match res.success {
                Some(true) => TaskVerdict::Pass,
                Some(false) => TaskVerdict::Fail,
                None if res.outcome == Some(OutcomeClass::Refused) => TaskVerdict::Refused,
                None => TaskVerdict::Unknown,
            },
        };
        let veto = if !res.veto_tripped.is_empty() {
            VetoVerdict::Observed
        } else if res.missing_required_checks || res.member.is_none() {
            // Missing checks — or an unresolved member (pending/not-run)
            // whose veto state cannot be asserted — read `unknown`,
            // never a silent `none`.
            VetoVerdict::Unknown
        } else {
            VetoVerdict::None
        };
        slots.push(SlotVerdicts {
            plan_ref: res.plan_ref.clone(),
            member: res.member.clone(),
            recovered: res.recovered,
            all_attempts: bound_runs,
            prerequisite,
            invocation,
            task,
            veto,
            veto_ids: res.veto_tripped.clone(),
        });
    }

    Ok(PopulationReceipt {
        population,
        legs,
        slots,
        analysis: declared.analysis.clone(),
        verdict: if failures.is_empty() {
            "closed"
        } else {
            "failed"
        },
        failures,
    })
}
