//! `hh-fleet` — the C4 human-agent organizational layer (canonical
//! §5i.1): `run_kind = fleet`, `WorkItem` + `control.work_item.*`, the
//! durable wakeup trigger rules, the ownership graph, the RC-1…8
//! reconciler over the fixture adapter, the Π state map, ownership and
//! escalation, and the matched-budget-conditional accountability record.
//!
//! The crate is a **fold + kernel-row layer** — every fact lives in the
//! activation run's durable prefix, every write goes through
//! `Store::commit_kernel_row_for` (kernel producer, audit-grade), and a
//! restart rebuilds byte-identical state from the same prefix. No
//! process-local authority, no second store, no live directory (the
//! `ApproverGrant`/static-`EscalationTarget` substitutes are documented
//! under D-1/D-2 in `docs/tickets/DEFERRALS.md`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capabilities;
pub mod engine;
pub mod errors;
pub mod identity;
pub mod ingress;
pub mod ownership;
pub mod payloads;
pub mod source;
pub mod spec;
pub mod view;
pub mod work_item;

pub use capabilities::{AdapterCapabilities, CapState, CAPABILITY_NAMES};
pub use engine::{
    AdmitOutcome, FleetEngine, ReconcileReport, COMPONENT, ISSUES, OUTCOMES, RESOLUTION_KINDS,
    RUN_KIND,
};
pub use errors::FleetError;
pub use identity::{
    agent_ref, budget_ref, escalation_ref, handoff_ref, idempotency_key, issue_ref,
    lease_agent_ref, occurrence_key, rule_ref, run_item_id, source_ref, spec_ref, work_item_ref,
    FIXTURE_SCHEMA, PAYLOAD_SCHEMA, SPEC_SCHEMA,
};
pub use ingress::{
    poll_occurrence_id, push_occurrence_id, IngressError, IngressOutcome, IngressPolicy,
    SignedWebhookIngress, WebhookAdapter, DEFAULT_REPLAY_WINDOW_MS, SIG_ALG, WEBHOOK_SCHEMA,
};
pub use ownership::{owner_chain, OwnershipGraph};
pub use source::{FixtureAdapter, SourceOccurrence, WorkSourceAdapter};
pub use spec::{fleet_trigger_admissible, Capacity, Defaults, FleetSpec, TriggerRule};
pub use view::{Cursor, FleetCue, FleetView};
pub use work_item::{
    derive_state, transition_ok, BlockedEscalate, DispatchState, EscalationState, ItemOn,
    RetryState, Settlement, WorkItemInit, WorkItemView, STATES,
};
