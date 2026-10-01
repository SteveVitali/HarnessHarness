//! The `hh-hosting/1` Hosting ABI — **C0/Stage-3 schema half** (spec §6.6;
//! R-2.10.6⁰; ticket S3.4d; ADR-0164/0165/0166 (e); CF-390).
//!
//! What lands here (the schema-only slice, removable with the hosting tier —
//! T-LCD-06):
//!
//! * [`events`] — the `HostedEvent` envelope `{seq, session, at, kind, payload,
//!   provenance{origin, authority}, mediation, event_channel, raw_ref?, ext}`
//!   with the closed kind vocabulary and the I-1…I-5 validators (lifecycle
//!   completeness, dense `seq`, extension preservation, the authority ceiling,
//!   hosted stamping).
//! * [`proj`] — `proj_ABI`: the projection of a native ledger run into hosted
//!   events, driven by `hh-ledger`'s `hosted_lowering` column (the class
//!   registry is the one class list — CC7; the table is data, so this crate is
//!   removable without touching the dialect). [`proj::lift`] is the inverse
//!   direction: hosted events back to native-class [`proj::LiftedRow`]s. The
//!   [`proj::LoweringLossReport`] declares every `no_slot`/`hint_only`/
//!   `narrowed` class the projection produces — declared, never silent.
//! * [`records`] — the `ParticipantRecord`/`AdapterRecord` body schemas
//!   (registry kinds `participant`/`adapter`, stored opaque — the registry
//!   re-runs the structural gate, this crate owns the schema) and the
//!   `hh.hosting/1` extension block.
//! * [`adapter_zero`] — adapter zero: the reference runtime presented through
//!   the ABI. The deterministic projection fixture AC-R-2.10.6-2 measures
//!   against; `observability = {events, end_state}` (+ `model_io` when
//!   intercepted), never `ledger`.
//!
//! Explicitly NOT here (S4.5a — §6.6 §727): the baseline/capability-declared
//! verbs, the `hh-hosting/1` handshake, Adapter A/B/C, the probe catalogue
//! P-01…P-16, the quarantine rule, hosted launch/runtime, and `budget_enforcement`
//! derivation.

#![warn(missing_docs)]

pub mod adapter_zero;
pub mod events;
pub mod proj;
pub mod records;

pub use adapter_zero::{adapter_zero_descriptor, adapter_zero_record, project_native_run};
pub use events::{
    ensure_terminal, is_known_kind, EventChannel, HostedError, HostedEvent, HostedOrigin,
    HostedProvenance, Mediation, KNOWN_KINDS,
};
pub use proj::{
    lift, project, LiftedRow, LossClass, LossEntry, LossSeverity, LoweringLossReport, Projection,
};
pub use records::{
    AdapterRecord, EnforcementClaim, HostingExt, ModelIoIntercept, ParticipantRecord,
    ProcessPlacement, RecordError,
};
