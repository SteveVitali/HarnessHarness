//! `hh-fleet-adapter` — the `work_source_adapter` plugin class (S5.6;
//! §5i.1; ADR-0205 D4, ADR-0180/0181). One crate, three halves:
//!
//! - [`abi`] — the class contract: op names, the `{result:
//!   "<canonical-json>"}` envelope (the same trick `memory_store` uses —
//!   records carry members the V1 inbound screen forbids as *envelope*
//!   members, so outputs cross as one opaque canonical string), and the
//!   strict codecs.
//! - [`adapter`] — `PluginSourceAdapter<C: SourceChannel>`: the
//!   host-side `WorkSourceAdapter` whose every method is one `invoke`.
//!   A channel fault latches: `fault()` thereafter answers
//!   `Some(source_ref)` — the reconciler's `SourceUnavailable` arm skips
//!   the adapter-reading passes and keeps activations (§5i.1 #5).
//! - [`tracker`] — `TrackerLogic`: the plugin-side surface a tracker
//!   variant implements, plus `dispatch` (the one decode/encode
//!   boundary — a malformed input is the ABI's `SchemaViolation`,
//!   never a panicking glue layer) and `FixtureTracker`, the reference
//!   logic backed by the Stage-4 fixture document (the HUMAN-H2 proxy
//!   lane: the real tracker variant plugs the same ops).
//!
//! Removable with the C4 tier: nothing below it depends on this crate
//! (CC6) and the crate holds no authority — the adapter answers records
//! the *engine* decides over (CC2/CC3).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod abi;
pub mod adapter;
pub mod tracker;

pub use abi::{OPS, WORK_SOURCE_CLASS, WORK_SOURCE_CONTRACT};
pub use adapter::{PluginSourceAdapter, SourceChannel, SourceFault};
pub use tracker::{dispatch, FixtureTracker, TrackerLogic};
