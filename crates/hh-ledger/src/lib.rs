//! `hh-ledger` — the §5a.1 event store / run ledger (S1.5, R-2.2.1).
//!
//! The authoritative record of every run: `open_run` / `append` / `read` / `head` /
//! `subscribe` / `put_blob` / `get_blob` / `project` / `verify` / `lineage` over a
//! content-hashed, dense-sequenced, append-only WAL; a single fenced writer lease;
//! durable-before-visible ordering; a content-addressed blob pool; and pure,
//! rebuildable projections (`context_view`, `run_summary`).
//!
//! Everything the ledger stores flows through the one canonical form (`hh-wire`),
//! the one identity scheme (`idp/1` in `hh-identity`), the one provenance schema
//! (`hh-provenance`) and the one home-plane rule (`hh-ontology`) — CC1/CC7/CC10.
//!
//! Layout:
//! - [`manifest`] — the `RunManifest` (the immutable seq-0 fact) and the run-kind rules.
//! - [`ids`] — the allocated-id model (`RunId`, `lease_id`, `effect_id`, cursors).
//! - [`classes`] — the C0 persistence-policy table (one registry — durability, offload
//!   threshold, observability floor, producer constraints, provenance rule, scope
//!   open/close, lowering).
//! - [`event`] — the envelope vocabulary (`Event`, `EventEnvelope`, `EphemeralRecord`,
//!   `Cursor`, `ReadFilter`, `Page`, `EventFrame`).
//! - [`schema`] — the canonical envelope form, the `idp/1` event hash, the schema gate.
//! - [`views`] — the pure projections (`context_view`, `run_summary`).
//! - [`store`] — the `Store` itself (WAL, lease file, blob pool, fan-out).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod classes;
pub mod effect;
pub mod errors;
pub mod event;
pub mod ids;
pub mod manifest;
pub mod recovery;
pub mod schema;
pub mod store;
pub mod views;

pub use effect::{CommitOutcome, CommitToken, EffectFold, EffectPhase};
pub use errors::{LedgerError, MissingReason, Tampered, TamperedKind};
pub use recovery::{RestoreAction, RestoreReport, RetryTimer};
pub use store::{LineageEntry, Store, Subscription, DEFAULT_BLOB_MAX_BYTES};
pub use views::{View, ViewKind, VIEW_POLICY_VERSION};
