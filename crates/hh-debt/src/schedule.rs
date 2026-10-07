//! `schedule_removal_test` — the §5h.6 §2 scheduling op (S6.1b;
//! ADR-0197 D6): instantiates the record's removal-test template with the
//! record's `scope` beneficiaries and `predicted_effect.metric_ref` as
//! primary metric, produces an `ExperimentSpec{kind: retirement}` (or
//! `retirement_batch` for a compatible group) for the caller to register
//! through §06's `register` with `charged_to = instrument`, and answers
//! `Deferred{reason}` when the instrument budget node cannot reserve or
//! `max_open_removal_tests` is reached.
//!
//! D-2/D-3 hold by construction: the manager authors a spec, never opens
//! a run, never applies a diff; every executable removal test is an
//! ordinary §06 experiment under the removal match shape.

use hh_hir::records::AssumptionDebtRecord;
use hh_lab::experiment::{ExperimentKind, ExperimentSpec};
use hh_ontology::debt::{DebtPolicy, DebtStatus, RemovalTestKind};
use hh_wire::json::Json;

use crate::errors::{DebtManagerError, Refusal};
use crate::records::SchedulableEntry;

/// The `DebtPolicy.priority` spellings the manager implements (closed —
/// the ratified default is `expiry_urgency`: `expired` and used >
/// `expiring` and used > `hypothesized` past probation > scheduled
/// cadence).
pub const PRIORITY_SPELLINGS: &[&str] = &["expiry_urgency"];

/// `Deferred{reason}` — the closed `schedule_removal_test` non-outcome
/// (§5h.6 §2's `Deferred{reason}` column; a scheduling miss is data, never
/// an error).
#[derive(Debug, Clone, PartialEq)]
pub enum DeferredReason {
    /// `max_open_removal_tests` reached — the schedule bound holds.
    MaxOpenRemovalTests {
        /// The policy bound.
        max: u32,
        /// The open count at schedule time.
        open: u32,
    },
    /// The instrument budget node could not reserve for the spec's
    /// `budgets.instrument` ref (§08 ADR-0040 reserve-before-spend).
    InstrumentBudgetUnreservable {
        /// The instrument budget ref that would not reserve.
        budget_ref: String,
    },
    /// The record's removal-test kind is not an executable experiment
    /// (the static kinds — `inspection`/`schema`/`documentation`/
    /// `attestation`/`evidence_superseded` — "cost nothing" and never
    /// schedule an experiment; §5h.6 §5).
    StaticKind {
        /// The kind spelling.
        kind: String,
    },
    /// The record's removal test does not instantiate.
    NotExecutable {
        /// The missing-payload detail.
        reason: String,
    },
    /// A `retirement_experiment` whose template did not resolve
    /// (records-in: the caller supplies the resolved `ExperimentSpec`).
    TemplateUnresolved {
        /// The unresolved `template_ref`.
        template_ref: String,
    },
}

impl DeferredReason {
    /// The canonical reason spelling (rides `Deferred{reason}` and the
    /// sweep report).
    pub fn name(&self) -> String {
        match self {
            DeferredReason::MaxOpenRemovalTests { .. } => "max_open_removal_tests".to_string(),
            DeferredReason::InstrumentBudgetUnreservable { .. } => {
                "instrument_budget_unreservable".to_string()
            }
            DeferredReason::StaticKind { .. } => "static_kind".to_string(),
            DeferredReason::NotExecutable { .. } => "not_executable".to_string(),
            DeferredReason::TemplateUnresolved { .. } => "template_unresolved".to_string(),
        }
    }
}

/// `ScheduleOutcome` — `experiment_id` (the instantiated spec, registered
/// by the caller) or `Deferred{reason}`.
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleOutcome {
    /// The test scheduled — the instantiated spec (its content-derived
    /// `experiment_id` is the answer; the caller registers it and mints
    /// `lifecycle.debt.removal_test.scheduled`).
    Scheduled {
        /// The instantiated spec (`charged_to = instrument` rides
        /// `ext.charged_to`; `budgets.instrument` is the budget node the
        /// reservation checked).
        spec: Box<ExperimentSpec>,
        /// The scheduling priority class (0 = expired+used … 3 = cadence).
        priority_class: u8,
    },
    /// The test deferred (the reason is data — the sweep reports it).
    Deferred {
        /// The closed reason.
        reason: DeferredReason,
    },
}

/// The scheduling context — what the caller's environment can assert
/// (records-in; the manager reads no store of record).
#[derive(Default)]
pub struct ScheduleContext<'a> {
    /// The count of already-open removal tests (the `ManagerView` fold).
    pub open_tests: u32,
    /// The instrument-budget reservation check (`budget_ref → reserved`) —
    /// the caller binds its §08 budget view; `None` = the caller asserts
    /// the instrument node reserves (the documented offline default).
    pub reserve: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether the sweep is running on cadence (the `scheduled cadence`
    /// bucket fires regardless of status urgency).
    pub on_cadence: bool,
}

/// `priority_class` — the §5h.6 §2 default order (`expiry_urgency`):
/// `expired` and used > `expiring` and used > `hypothesized` past
/// probation > scheduled cadence. Returns the class index (lower schedules
/// first); a record with no removal test is class 4 — never scheduled.
pub fn priority_class(entry: &SchedulableEntry) -> u8 {
    if entry.removal_kind.is_none() {
        return 4;
    }
    match entry.current_status {
        DebtStatus::Expired if entry.used => 0,
        DebtStatus::Expiring if entry.used => 1,
        _ if entry.evidence_grade == hh_ontology::debt::EvidenceGrade::Hypothesized
            && entry.probation_due =>
        {
            2
        }
        _ => 3,
    }
}

/// Whether a removal-test kind schedules an executable experiment under
/// the manager (the executable kinds per §5h.6 §3; every other kind is
/// static — "static kinds are not comparisons and carry no claim",
/// AC-R-2.9.6-11).
pub fn executable_kind(kind: RemovalTestKind) -> bool {
    matches!(kind, RemovalTestKind::RetirementExperiment)
}

/// `instantiate` — bind the resolved template spec to the debt record:
/// `kind = retirement`, `predicted_effect.metric_ref` as the primary
/// metric, the record's `removal_test.beneficiaries` (⊆ scope — the
/// seal/register half already checked the bound), `debt_ref`/`rule_id`
/// lineage on `ext`, `charged_to = instrument`, and the content-derived
/// `experiment_id` recomputed.
pub fn instantiate(
    debt_ref: &str,
    record: &AssumptionDebtRecord,
    mut template: ExperimentSpec,
) -> ExperimentSpec {
    template.kind = ExperimentKind::Retirement;
    let metric = record
        .hypothesis_typed
        .as_ref()
        .and_then(|h| h.metric_ref.clone());
    if let Some(mr) = &metric {
        if let Some(pr) = template.pre_registration.as_mut() {
            pr.primary_metrics = vec![mr.clone()];
        }
        template.design.pre_registration.primary_metrics = vec![mr.clone()];
    }
    template
        .ext
        .insert("debt_ref".to_string(), Json::str(debt_ref));
    template
        .ext
        .insert("debt_rule_id".to_string(), Json::str(&record.rule_id));
    template
        .ext
        .insert("charged_to".to_string(), Json::str("instrument"));
    if let Some(t) = &record.removal_test {
        if !t.beneficiaries.is_empty() {
            template.ext.insert(
                "debt_beneficiaries".to_string(),
                Json::Arr(
                    t.beneficiaries
                        .iter()
                        .map(|b| Json::str(b.clone()))
                        .collect(),
                ),
            );
        }
    }
    template.experiment_id = String::new();
    template.experiment_id = template.experiment_id();
    template
}

/// `schedule_removal_test(debt_ref, entry, record, policy, ctx)` —
/// the §5h.6 §2 op: a `Scheduled{spec}` on admission or
/// `Deferred{reason}`.
pub fn schedule_removal_test(
    entry: &SchedulableEntry,
    record: &AssumptionDebtRecord,
    template: Option<&ExperimentSpec>,
    policy: &DebtPolicy,
    ctx: &ScheduleContext<'_>,
) -> Result<ScheduleOutcome, DebtManagerError> {
    if !PRIORITY_SPELLINGS.contains(&policy.priority.as_str()) {
        return Err(Refusal::Unsupported {
            detail: format!(
                "DebtPolicy.priority `{}` is not in {PRIORITY_SPELLINGS:?}",
                policy.priority
            ),
        }
        .into());
    }
    let class = priority_class(entry);
    let test = match &record.removal_test {
        Some(t) => t,
        None => {
            return Ok(ScheduleOutcome::Deferred {
                reason: DeferredReason::NotExecutable {
                    reason: "record carries no removal_test".to_string(),
                },
            })
        }
    };
    if !executable_kind(test.kind) {
        return Ok(ScheduleOutcome::Deferred {
            reason: DeferredReason::StaticKind {
                kind: test.kind.name().to_string(),
            },
        });
    }
    if !test.instantiates() {
        return Ok(ScheduleOutcome::Deferred {
            reason: DeferredReason::NotExecutable {
                reason: format!(
                    "removal_test.kind {} lacks its mandatory payload",
                    test.kind.name()
                ),
            },
        });
    }
    // The schedule bound and the instrument-budget reservation — both are
    // `Deferred`, never a refusal (§5h.6 §5's failure row: a rule without
    // a design never reaches `retired` but never goes silently stale).
    if ctx.open_tests >= policy.max_open_removal_tests {
        return Ok(ScheduleOutcome::Deferred {
            reason: DeferredReason::MaxOpenRemovalTests {
                max: policy.max_open_removal_tests,
                open: ctx.open_tests,
            },
        });
    }
    let template = match template {
        Some(t) => t.clone(),
        None => {
            return Ok(ScheduleOutcome::Deferred {
                reason: DeferredReason::TemplateUnresolved {
                    template_ref: test.template_ref.clone().unwrap_or_default(),
                },
            })
        }
    };
    let spec = instantiate(&entry.debt_ref, record, template);
    if let Some(reserve) = ctx.reserve {
        if !reserve(&spec.budgets.instrument) {
            return Ok(ScheduleOutcome::Deferred {
                reason: DeferredReason::InstrumentBudgetUnreservable {
                    budget_ref: spec.budgets.instrument.clone(),
                },
            });
        }
    }
    Ok(ScheduleOutcome::Scheduled {
        spec: Box::new(spec),
        priority_class: class,
    })
}

/// `schedule_batch` — the `retirement_batch` design (§5h.6 §2's
/// `schedule_removal_test` row: "batch designs (`retirement_batch`, k
/// single-rule removal arms sharing one base arm) admitted by §06
/// ADR-0156 as amended"): k debts sharing one base arm fold into one
/// `ExperimentSpec{kind: retirement_batch}` — base arm + one removal arm
/// per debt, `ext.debt_refs[]`/`ext.debt_rule_ids[]` parallel to the
/// removal arms, `charged_to = instrument`, the same removal match shape
/// per arm (`register` re-checks each).
///
/// Records-in compatibility is structural: each debt's resolved template
/// is a 2-arm retirement spec (`[base, removal]`); the batch is admissible
/// only when every template's base arm, design, suite, budgets, factors,
/// policies and pre-registration are canonical-equal — one shared
/// comparison, not k different ones (else `BatchIncompatible`). The
/// removal arm each template contributes is the caller-materialized arm
/// (D-2: the manager never freezes an artifact itself). A batch counts as
/// one open test *per debt* on the ledger — each debt's own `scheduled`
/// row mints; the bound is on open obligations, not experiments.
pub fn schedule_batch(
    entries: &[(&SchedulableEntry, &AssumptionDebtRecord, &ExperimentSpec)],
    policy: &DebtPolicy,
) -> Result<ExperimentSpec, DebtManagerError> {
    if !PRIORITY_SPELLINGS.contains(&policy.priority.as_str()) {
        return Err(Refusal::Unsupported {
            detail: format!(
                "DebtPolicy.priority `{}` is not in {PRIORITY_SPELLINGS:?}",
                policy.priority
            ),
        }
        .into());
    }
    let incompatible = |detail: &str| -> DebtManagerError {
        Refusal::BatchIncompatible {
            detail: detail.to_string(),
        }
        .into()
    };
    let Some((_, first_record, first_template)) = entries.first() else {
        return Err(incompatible("batch is empty"));
    };
    if entries.len() < 2 {
        return Err(incompatible(
            "batch needs ≥ 2 debts (a single debt schedules `retirement`)",
        ));
    }
    if first_template.arms.len() != 2 {
        return Err(incompatible(
            "the batch template must be a 2-arm retirement spec [base, removal]",
        ));
    }
    let base = &first_template.arms[0];
    let design = &first_template.design;
    let suite = &first_template.suite;
    let budgets = &first_template.budgets;
    for (entry, record, template) in &entries[1..] {
        let _ = entry;
        if template.arms.len() != 2 {
            return Err(incompatible("a batch member template is not a 2-arm spec"));
        }
        if &template.arms[0] != base {
            return Err(incompatible("batch members do not share one base arm"));
        }
        if &template.design != design || &template.suite != suite || &template.budgets != budgets {
            return Err(incompatible(
                "batch members differ on design, suite, or budgets",
            ));
        }
        let Some(test) = &record.removal_test else {
            return Err(incompatible("a batch member carries no removal_test"));
        };
        if !executable_kind(test.kind) {
            return Err(incompatible(
                "a batch member's removal test is not executable",
            ));
        }
    }
    let mut spec = instantiate(
        &entries[0].0.debt_ref,
        first_record,
        (*first_template).clone(),
    );
    spec.kind = ExperimentKind::RetirementBatch;
    spec.arms = vec![spec.arms[0].clone()];
    let mut rule_ids = Vec::new();
    let mut debt_refs = Vec::new();
    for (entry, record, template) in entries {
        let _ = entry;
        spec.arms.push(template.arms[1].clone());
        rule_ids.push(Json::str(&record.rule_id));
        debt_refs.push(Json::str(&entry.debt_ref));
    }
    spec.ext
        .insert("debt_refs".to_string(), Json::Arr(debt_refs));
    spec.ext
        .insert("debt_rule_ids".to_string(), Json::Arr(rule_ids));
    spec.ext
        .insert("charged_to".to_string(), Json::str("instrument"));
    spec.experiment_id = String::new();
    spec.experiment_id = spec.experiment_id();
    Ok(spec)
}
