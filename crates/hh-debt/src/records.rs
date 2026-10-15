//! The `hh-debt` record types (S6.1b; §5h.6 §3): the
//! `DebtManagerRecord` service record (AC-R-2.9.6-12 — `maturity`, the
//! home-16 reflexive `AssumptionDebtRecord`, `DebtPolicy.schedule`), the
//! sweep input/output rows, and the scheduled-test bookkeeping the
//! `ManagerView` fold rebuilds.

use hh_hir::records::AssumptionDebtRecord;
use hh_lab::debt::{DebtObservables, DebtTransition};
use hh_lab::experiment::ExperimentSpec;
use hh_ontology::debt::{DebtPolicy, RemovalTestKind, RemovalVerdict};
use hh_wire::json::Json;

use crate::errors::DebtManagerError;

fn bad(detail: impl Into<String>) -> DebtManagerError {
    DebtManagerError::Schema(detail.into())
}

/// The closed maturity spellings a service record may claim (§9.7's
/// stage-6a row: the pipeline-as-Lab-recipe slice is `instrument-grade`;
/// the vocabulary is the ADR-0002 (e) conditionality ladder).
pub const MATURITY_SPELLINGS: &[&str] = &["instrument-grade", "research-grade", "candidate"];

/// `DebtManagerRecord` — the manager's service record (AC-R-2.9.6-12;
/// §5h.6 §3's home-16 row): the service's declared `maturity`, the
/// `DebtPolicy` it schedules under (carrying `schedule` — the check
/// cadence spelling), and the reflexive `AssumptionDebtRecord` the
/// discipline applies to the manager itself.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtManagerRecord {
    /// The manager's stable id (`debt_manager:<id>` — content-derived on
    /// registration when empty).
    pub manager_id: String,
    /// The maturity flag (a [`MATURITY_SPELLINGS`] member).
    pub maturity: String,
    /// The policy the manager schedules/monitor under (`policy.schedule`
    /// is the standing-monitor cadence spelling — AC-R-2.9.6-12).
    pub policy: DebtPolicy,
    /// The reflexive `AssumptionDebtRecord` (home 16 —
    /// `assumption_debt_manager.debt`; `removal_test` must instantiate as
    /// `no_dead_weight_found{window}`).
    pub reflexive_debt: AssumptionDebtRecord,
}

impl DebtManagerRecord {
    /// Validate the service record: a known maturity spelling and a
    /// reflexive debt record that validates for home 16 (member-level —
    /// `no_dead_weight_found{window}` instantiates, required fields
    /// present).
    pub fn validate(&self) -> Result<(), DebtManagerError> {
        if !MATURITY_SPELLINGS.contains(&self.maturity.as_str()) {
            return Err(bad(format!(
                "maturity `{}` is not in {MATURITY_SPELLINGS:?}",
                self.maturity
            )));
        }
        if self.manager_id.trim().is_empty() {
            return Err(bad("manager_id is empty"));
        }
        // The policy members the manager actually executes must be
        // declared at admission (AC-R-2.9.6-12): `schedule` is the
        // standing-monitor cadence spelling the sweep's durable row
        // attributes, and `priority` must be a spelling the scheduler
        // implements — fail fast at `register`, never at the sweep.
        if self.policy.schedule.trim().is_empty() {
            return Err(bad("policy.schedule is empty — the check cadence spelling"));
        }
        if !crate::schedule::PRIORITY_SPELLINGS.contains(&self.policy.priority.as_str()) {
            return Err(bad(format!(
                "policy.priority `{}` is not in {:?}",
                self.policy.priority,
                crate::schedule::PRIORITY_SPELLINGS
            )));
        }
        hh_hir::debt::validate_for_home(
            &self.reflexive_debt,
            "assumption_debt_manager",
            "debt",
            &self.policy,
            &hh_hir::debt::RemovalTestContext::member_level(),
        )
        .map_err(|e| bad(format!("reflexive_debt: {e:?}")))?;
        if self.reflexive_debt.removal_test.as_ref().map(|t| t.kind)
            != Some(RemovalTestKind::NoDeadWeightFound)
        {
            return Err(bad(
                "reflexive_debt.removal_test.kind must be no_dead_weight_found \
                 (the manager's honest end state is retirement when it finds \
                 no dead weight — §5h.6 §3's reflexive row)",
            ));
        }
        Ok(())
    }

    /// The reflexive record's debt ref (`debt:assumption_debt_manager:<id>`).
    pub fn reflexive_debt_ref(&self) -> String {
        format!(
            "debt:assumption_debt_manager:{}:{}",
            self.manager_id, self.reflexive_debt.rule_id
        )
    }

    /// The canonical JSON (`lifecycle.debt.service.registered`'s `record`
    /// member).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("manager_id", Json::str(&self.manager_id)),
            ("maturity", Json::str(&self.maturity)),
            ("policy", self.policy.to_json()),
            (
                "reflexive_debt",
                hh_hir::debt_json(&self.reflexive_debt, false),
            ),
            ("reflexive_debt_ref", Json::str(self.reflexive_debt_ref())),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<DebtManagerRecord, DebtManagerError> {
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(bad("DebtManagerRecord is not an object")),
        };
        let str_at = |k: &str| -> Result<String, DebtManagerError> {
            m.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad(format!("DebtManagerRecord.{k} missing")))
        };
        let policy = m
            .get("policy")
            .ok_or_else(|| bad("DebtManagerRecord.policy missing"))?;
        let reflexive = m
            .get("reflexive_debt")
            .ok_or_else(|| bad("DebtManagerRecord.reflexive_debt missing"))?;
        Ok(DebtManagerRecord {
            manager_id: str_at("manager_id")?,
            maturity: str_at("maturity")?,
            policy: DebtPolicy::from_json(policy, "/policy")
                .map_err(|e| bad(format!("policy: {}", e.detail)))?,
            reflexive_debt: hh_hir::debt_from_json(reflexive, "/reflexive_debt")
                .map_err(|e| bad(format!("reflexive_debt: {e:?}")))?,
        })
    }
}

/// `DebtSweepEntry` — one debt-bearing record the sweep evaluates
/// (records-in: the caller projects the registry snapshot into these rows;
/// the manager reads no store-of-record itself).
#[derive(Debug, Clone)]
pub struct SweepEntry {
    /// The debt ref (`debt:<kind>:<version_id>:<home>:<rule_id>` or the
    /// caller's canonical spelling).
    pub debt_ref: String,
    /// The `DebtHomes/1` home id (`None` = no home-specific triggers).
    pub home: Option<u8>,
    /// The registry `version_id` the record rides (provenance of the row).
    pub version_id: Option<String>,
    /// The debt record itself.
    pub record: AssumptionDebtRecord,
    /// The caller-projected observables for this record.
    pub observables: DebtObservables,
    /// The resolved removal-test template (`retirement_experiment`'s
    /// `template_ref` resolution — the instantiated spec's base shape).
    /// Absent ⇒ the schedule leg answers `Deferred`.
    pub template: Option<ExperimentSpec>,
    /// `used_by[]` — the sealed definition version ids still using the rule
    /// (the `expired and used`/`expiring and used` priority legs).
    pub used_by: Vec<String>,
}

/// `DebtSweepEntry::priority_class` input — projected once per entry so the
/// ordering is a pure function of the fold (CC3).
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulableEntry {
    /// The debt ref.
    pub debt_ref: String,
    /// The derived current status spelling (`active`/`expiring`/`expired`/
    /// `retired` — folded, not stored).
    pub current_status: hh_ontology::debt::DebtStatus,
    /// The derived evidence grade.
    pub evidence_grade: hh_ontology::debt::EvidenceGrade,
    /// Whether the record is used by a live definition (`used_by` non-empty).
    pub used: bool,
    /// Whether the ledgered probation entry is past due (DF-S5.4-1 — the
    /// book of record, not the caller's clock).
    pub probation_due: bool,
    /// The next time-based expiry (ordering tiebreak within a class).
    pub next_time_expiry_ms: Option<u64>,
    /// The removal-test kind, when the record carries one.
    pub removal_kind: Option<RemovalTestKind>,
    /// The `DebtHomes/1` home id, when the caller projects it (R2.17 —
    /// the schedule leg resolves it to the `DebtHome` row so the
    /// `validate_removal_test` context checks can name the home).
    pub home: Option<u8>,
}

/// `SweepReport` — the sweep's outcome (rides
/// `lifecycle.debt.sweep.completed` and the `lab.debt.sweep` result): what
/// was evaluated, what transitioned, what probation entries opened, what
/// scheduled or deferred, and the reflexive verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepReport {
    /// The sweep sequence number (the durable-prefix sweep count + 1).
    pub sweep_seq: u64,
    /// The sweep's transaction time.
    pub now_ms: u64,
    /// Entries evaluated.
    pub evaluated: u64,
    /// Transitions minted (`lifecycle.debt.status.changed` payloads).
    pub transitions: Vec<DebtTransition>,
    /// Probation entries opened (`debt_ref`s — the rows carry the due_at).
    pub probation_opened: Vec<String>,
    /// Scheduled removal tests (`{debt_ref, experiment_id, kind}` rows the
    /// `lifecycle.debt.removal_test.scheduled` mints carried).
    pub scheduled: Vec<ScheduledTest>,
    /// Deferred schedule decisions (`{debt_ref, reason}`).
    pub deferred: Vec<(String, String)>,
    /// The reflexive `no_dead_weight_found` verdict for this sweep's window
    /// (`None` when the window is still open — `inconclusive{window_open}`).
    pub reflexive_verdict: Option<RemovalVerdict>,
}

/// One scheduled removal test — the durable `scheduled` row's content.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledTest {
    /// The debt the test discharges.
    pub debt_ref: String,
    /// The removal-test kind.
    pub kind: RemovalTestKind,
    /// The instantiated `ExperimentSpec` id (`experiment_id` — content
    /// derived; `charged_to = instrument` rides `spec.ext`).
    pub experiment_id: String,
    /// The instantiated spec (deposited through LabDocs/registered through
    /// §06's `register` by the caller — D-2: the manager never opens a run).
    pub spec: ExperimentSpec,
    /// The scheduling priority class (0 = expired+used … 3 = cadence).
    pub priority_class: u8,
}

impl ScheduledTest {
    /// The `lifecycle.debt.removal_test.scheduled` payload.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("debt_ref", Json::str(&self.debt_ref)),
            ("kind", Json::str(self.kind.name())),
            ("experiment_id", Json::str(&self.experiment_id)),
            ("priority_class", Json::Int(self.priority_class as i64)),
        ])
    }
}
