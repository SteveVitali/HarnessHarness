//! The closed `FleetError` sum — every failure is a typed variant (§5i.1
//! #4–#6's refusal spellings; `xerr` maps them onto the boundary's
//! `WouldBlock`/`Refused`/`SchemaViolation`/`InsufficientBudget`).

use hh_budget::errors::BudgetError;
use hh_ledger::errors::LedgerError;
use std::fmt;

/// One canonical spelling per variant (the `code` member the boundary
/// records — closed, stable, never a free-form message).
#[derive(Debug)]
pub enum FleetError {
    /// The store call failed.
    Store(LedgerError),
    /// The account call failed (RC-6's ledgered refusal included).
    Account(BudgetError),
    /// The caller's read cursor no longer matches the durable FleetSpec —
    /// `WouldBlock{stale_spec}` at the boundary (RC-1).
    StaleSpec {
        /// The durable spec fingerprint.
        expected: String,
        /// The cursor the caller presented.
        seen: String,
    },
    /// The caller is not the run's write authority (lease/owner) — `xerr`
    /// refuses `not_owner`.
    NotOwner {
        /// The activation run.
        run: String,
        /// The work item the call named.
        item: String,
    },
    /// An ownership target outside this activation's `agents` set —
    /// `Refused{cross_fleet}` (a `GraphError`, §5i.1 #4).
    CrossFleet {
        /// The activation run.
        run: String,
        /// The work item the call named.
        item: String,
        /// The out-of-fleet owner ref.
        owner: String,
    },
    /// An ownership edge would close a cycle — `Refused{cycle}`.
    Cycle {
        /// The activation run.
        run: String,
        /// The work item the call named.
        item: String,
        /// The cyclic owner path (`a>b>…>a`).
        path: String,
    },
    /// The work item names an agent not in `spec.agents` — `Refused{owner_unknown}`.
    OwnerUnknown {
        /// The work item.
        item: String,
        /// The agent ref.
        agent: String,
    },
    /// An owner was required for the transition but none is in force —
    /// `Refused{owner_required}` (`owner = none` refuses dispatch to a
    /// network effect unless a narrowed Π grants `allow`).
    OwnerRequired {
        /// The work item.
        item: String,
    },
    /// The dispatched effect requires owner acknowledgement the durable
    /// fold does not show — `Refused{owner_ack_required}`.
    OwnerAckRequired {
        /// The work item.
        item: String,
    },
    /// The work item id is unknown in this activation — `Refused{unknown_item}`.
    UnknownItem {
        /// The work item.
        item: String,
    },
    /// `admit`/`occurred` produced an item whose durable fields conflict
    /// with the admitted record for the same id — `SourceConflict{field}`
    /// (RC-3).
    SourceConflict {
        /// The work item.
        item: String,
        /// The conflicting member.
        field: String,
    },
    /// `activate_run ≤ 0` — `Refused{capacity}` (RC-6).
    CapacityFull {
        /// The activation run.
        run: String,
    },
    /// The lease fold is stale for this work item — `Refused{stale_dispatch}`
    /// (the second `fire` skips; the dispatch row the first fire wrote is the
    /// truth — RC-2).
    StaleDispatch {
        /// The work item.
        item: String,
    },
    /// The activation carries no `budget_ref` and the work item is not
    /// declared out-of-scope — `Refused{missing_budget_ref}` (the
    /// matched-budget conditional's refused arm, RC-6).
    MissingBudgetRef,
    /// The dispatch lease was released but the effect never terminalised —
    /// `Blocked{stale_lease}` under `no_lease_no_dispatch` (RC-2).
    StaleLease {
        /// The work item.
        item: String,
    },
    /// `escalation` is not open on this item — `Refused{no_open_escalation}`.
    NoOpenEscalation {
        /// The work item.
        item: String,
    },
    /// The resolving link violates `legitimate` — `Refused{illegitimate_resolution}`.
    IllegitimateResolution {
        /// The work item.
        item: String,
        /// The `issue_ref` the resolver presented.
        issue_ref: String,
    },
    /// `stop` on an item that is not stoppable — `Refused{not_stoppable}`.
    NotStoppable {
        /// The work item.
        item: String,
    },
    /// A `settle` precondition failed — `Refused{settle_precondition}`.
    SettlePrecondition {
        /// The work item.
        item: String,
        /// The failing precondition.
        detail: String,
    },
    /// `dispatch_note` mismatched the durable `dispatch.spec_ref` —
    /// `Refused{stale_dispatch}` (host calls must not contradict the row).
    StaleDispatchNote {
        /// The work item.
        item: String,
        /// The spec_ref the durable row carries.
        expected: String,
    },
    /// The run is not a `run_kind = fleet` activation — `Refused{not_fleet}`.
    NotFleet {
        /// The run id.
        run: String,
    },
    /// The run holds no `lifecycle.fleet.activated` spec — `Refused{activation_not_found}`.
    ActivationNotFound {
        /// The run id.
        run: String,
    },
    /// The named trigger rule is not in `spec.triggers` or carries an
    /// unsupported `Trigger` member — `Refused{unsupported_trigger}` /
    /// `Refused{unknown_trigger}`.
    UnsupportedTrigger {
        /// The trigger spelling.
        trigger: String,
    },
    /// The op is declared but unsupported in this slice — `Unsupported{by}`.
    Unsupported {
        /// The op.
        op: &'static str,
        /// The reason code.
        reason: String,
    },
    /// A malformed durable record — `Refused{invalid_payload}` (panic-free
    /// surface for boundary tests; the in-process fold still panics).
    InvalidPayload {
        /// Where the malformed member was seen.
        detail: String,
    },
    /// A malformed input — `SchemaViolation` at the boundary.
    SchemaViolation {
        /// The offending member path/detail.
        detail: String,
    },
}

impl FleetError {
    /// The canonical refusal `reason`/`code` spelling — the member the
    /// boundary stamps and the durable rows record (closed set).
    pub fn code(&self) -> String {
        match self {
            FleetError::Store(e) => format!("store_{}", ledger_code(e)),
            FleetError::Account(e) => format!("account_{:?}", e),
            FleetError::StaleSpec { .. } => "stale_spec".to_string(),
            FleetError::NotOwner { .. } => "not_owner".to_string(),
            FleetError::CrossFleet { .. } => "cross_fleet".to_string(),
            FleetError::Cycle { .. } => "cycle".to_string(),
            FleetError::OwnerUnknown { .. } => "owner_unknown".to_string(),
            FleetError::OwnerRequired { .. } => "owner_required".to_string(),
            FleetError::OwnerAckRequired { .. } => "owner_ack_required".to_string(),
            FleetError::UnknownItem { .. } => "unknown_item".to_string(),
            FleetError::SourceConflict { field, .. } => {
                format!("source_conflict{{field:{field}}}")
            }
            FleetError::CapacityFull { .. } => "capacity".to_string(),
            FleetError::StaleDispatch { .. } => "stale_dispatch".to_string(),
            FleetError::MissingBudgetRef => "missing_budget_ref".to_string(),
            FleetError::StaleLease { .. } => "stale_lease".to_string(),
            FleetError::NoOpenEscalation { .. } => "no_open_escalation".to_string(),
            FleetError::IllegitimateResolution { .. } => {
                "illegitimate_resolution".to_string()
            }
            FleetError::NotStoppable { .. } => "not_stoppable".to_string(),
            FleetError::SettlePrecondition { .. } => "settle_precondition".to_string(),
            FleetError::StaleDispatchNote { .. } => "stale_dispatch".to_string(),
            FleetError::NotFleet { .. } => "not_fleet".to_string(),
            FleetError::ActivationNotFound { .. } => "activation_not_found".to_string(),
            FleetError::UnsupportedTrigger { trigger } => {
                format!("unsupported_trigger{{trigger:{trigger}}}")
            }
            FleetError::Unsupported { reason, .. } => reason.clone(),
            FleetError::InvalidPayload { .. } => "invalid_payload".to_string(),
            FleetError::SchemaViolation { .. } => "schema_violation".to_string(),
        }
    }
}

/// A short ledger-error coordinate for `store_*` codes (debug name, stable).
fn ledger_code(e: &LedgerError) -> String {
    format!("{:?}", e)
}

impl fmt::Display for FleetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FleetError::Store(e) => write!(f, "store: {e}"),
            FleetError::Account(e) => write!(f, "account: {e}"),
            FleetError::StaleSpec { expected, seen } => write!(
                f,
                "StaleSpec: cursor {seen} vs durable spec {expected}"
            ),
            FleetError::NotOwner { run, item } => {
                write!(f, "NotOwner: {item} under {run}")
            }
            FleetError::CrossFleet { run, item, owner } => {
                write!(f, "CrossFleet: {owner} for {item} under {run}")
            }
            FleetError::Cycle { run, item, path } => {
                write!(f, "Cycle: {item} under {run} closes {path}")
            }
            FleetError::OwnerUnknown { item, agent } => {
                write!(f, "OwnerUnknown: {agent} for {item}")
            }
            FleetError::OwnerRequired { item } => write!(f, "OwnerRequired: {item}"),
            FleetError::OwnerAckRequired { item } => {
                write!(f, "OwnerAckRequired: {item}")
            }
            FleetError::UnknownItem { item } => write!(f, "UnknownItem: {item}"),
            FleetError::SourceConflict { item, field } => {
                write!(f, "SourceConflict: {item} field {field}")
            }
            FleetError::CapacityFull { run } => write!(f, "CapacityFull: {run}"),
            FleetError::StaleDispatch { item } => write!(f, "StaleDispatch: {item}"),
            FleetError::MissingBudgetRef => write!(f, "MissingBudgetRef"),
            FleetError::StaleLease { item } => write!(f, "StaleLease: {item}"),
            FleetError::NoOpenEscalation { item } => {
                write!(f, "NoOpenEscalation: {item}")
            }
            FleetError::IllegitimateResolution { item, issue_ref } => {
                write!(f, "IllegitimateResolution: {item} issue {issue_ref}")
            }
            FleetError::NotStoppable { item } => write!(f, "NotStoppable: {item}"),
            FleetError::SettlePrecondition { item, detail } => {
                write!(f, "SettlePrecondition: {item} — {detail}")
            }
            FleetError::StaleDispatchNote { item, expected } => {
                write!(f, "StaleDispatchNote: {item} expects {expected}")
            }
            FleetError::NotFleet { run } => write!(f, "NotFleet: {run}"),
            FleetError::ActivationNotFound { run } => {
                write!(f, "ActivationNotFound: {run}")
            }
            FleetError::UnsupportedTrigger { trigger } => {
                write!(f, "UnsupportedTrigger: {trigger}")
            }
            FleetError::Unsupported { op, reason } => {
                write!(f, "Unsupported: {op} — {reason}")
            }
            FleetError::InvalidPayload { detail } => {
                write!(f, "InvalidPayload: {detail}")
            }
            FleetError::SchemaViolation { detail } => {
                write!(f, "SchemaViolation: {detail}")
            }
        }
    }
}

impl std::error::Error for FleetError {}

impl From<LedgerError> for FleetError {
    fn from(e: LedgerError) -> Self {
        FleetError::Store(e)
    }
}

impl From<BudgetError> for FleetError {
    fn from(e: BudgetError) -> Self {
        FleetError::Account(e)
    }
}
