//! The `hh-hosting/1` Hosting ABI (spec §6.6; R-2.10.6; tickets S3.4d +
//! S4.5a; ADR-0164/0165/0166 (e); CF-390).
//!
//! The schema half (S3.4d):
//!
//! * [`events`] — the `HostedEvent` envelope `{seq, session, at, kind, payload,
//!   provenance{origin, authority}, mediation, event_channel, raw_ref?, ext}`
//!   with the closed kind vocabulary and the I-1…I-5 validators (lifecycle
//!   completeness, dense `seq`, extension preservation, the authority ceiling,
//!   hosted stamping).
//! * [`proj`] — `proj_ABI` (native → hosted, driven by `hh-ledger`'s
//!   `hosted_lowering` column — the class registry is the one class list,
//!   CC7) and `lift` (hosted → native [`proj::LiftedRow`]s) with the declared
//!   [`proj::LoweringLossReport`].
//! * [`records`] — the `ParticipantRecord`/`AdapterRecord` body schemas
//!   (registry kinds `participant`/`adapter`, stored opaque) and the
//!   `hh.hosting/1` extension block.
//! * [`adapter_zero`] — the deterministic reference-harness fixture over
//!   native runs; `observability = {events, end_state}` + `model_io` when
//!   intercepted, never `ledger`.
//!
//! The service half (S4.5a — the ABI proper, thin/observational, N2):
//!
//! * [`abi`] — the `hh-hosting/1` handshake ([`abi::negotiate`]), the closed
//!   baseline/capability-declared/excluded verb lists, [`abi::HostingError`],
//!   and the `open` spec types ([`abi::HostedRunSpec`], [`abi::EndState`],
//!   [`abi::ResumeCursor`]). No raw pass-through: the excluded list is
//!   refused and every other unnamed verb is `UnknownVerb`.
//! * [`adapter_a`] — Adapter A: the session-ABI adapter over a
//!   records-in/records-out [`adapter_a::SessionTransport`], with the
//!   ACP-shaped `session/update` lift table, the Lab-supplied policy decider,
//!   and the auto-approval admission check (I-6; AC-R-2.10.6-4).
//! * [`service`] — [`service::HostingService`]: the verb implementation —
//!   handshake, boundary checks (no handles, declared credential channels
//!   only, `budget_view` by CF-073), capability-vector gates, dense-seq
//!   event ingest, terminal synthesis, runtime drift annotations, and the
//!   `handle(op, params)` records-in surface the boundary's hosting plane
//!   wires (`hh-embed` never depends on this crate — the seam is Json).
//! * [`budget`] — [`budget::derive_budget_enforcement`] (the Lab derives —
//!   never the participant's claim) + the one ceiling predicate the §6.6 §8
//!   decision points share.
//! * [`probes`] — the P-01…P-16 catalogue + driver (`skipped` when the
//!   environment cannot exercise a probe — never `unsupported`; P0 =
//!   `{P-01,P-03,P-04,P-06,P-09,P-16}` — a P0 DRIFT quarantines).
//! * [`fixture`] — the in-process scripted participant the tests and probes
//!   drive (hermetic, offline-only — behavior flags, never declarations).

#![warn(missing_docs)]

pub mod abi;
pub mod adapter_a;
pub mod adapter_zero;
pub mod budget;
pub mod events;
pub mod fixture;
pub mod probes;
pub mod proj;
pub mod records;
pub mod service;

pub use abi::{
    is_admitted_verb, negotiate, refuse_handle_keys, AbiVersion, EndState, HostedRunSpec,
    HostingError, Negotiated, Opened, ResumeCursor, ABI_VERSION, BASELINE_VERBS,
    CAPABILITY_DECLARED_VERBS, EXCLUDED_VERBS,
};
pub use adapter_a::{
    adapter_a_record, allow_all_decider, ask_then_allow_decider, deny_all_decider, AdapterA,
    LiftedObservation, PendingUpcall, PolicyDecider, SessionTransport,
};
pub use adapter_zero::{adapter_zero_descriptor, adapter_zero_record, project_native_run};
pub use budget::{
    check_ceiling, derive_budget_enforcement, enforcement_json, to_budget_enforcement, CeilingCheck,
};
pub use events::{
    ensure_terminal, is_known_kind, EventChannel, HostedError, HostedEvent, HostedOrigin,
    HostedProvenance, Mediation, KNOWN_KINDS,
};
pub use fixture::{FixtureParticipant, ScriptedTool};
pub use probes::{
    drive_probe_json, is_p0, probe_spec, run_probe, ProbeOutcome, ProbeSpec, P0_DIMENSIONS,
    PROBE_CATALOGUE,
};
pub use proj::{
    lift, project, LiftedRow, LossClass, LossEntry, LossSeverity, LoweringLossReport, Projection,
};
pub use records::{
    AdapterRecord, EnforcementClaim, HostingExt, ModelIoIntercept, ParticipantRecord,
    ProcessPlacement, RecordError,
};
pub use service::{HostedSession, HostingService};
