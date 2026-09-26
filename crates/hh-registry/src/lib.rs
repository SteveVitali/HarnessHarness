//! `hh-registry` — the §6.2 component-variation registry **store** (S1.8, `R-2.10.2⁰`).
//!
//! The one typed record store for every registry-shaped kind: component classes and
//! their variants, conformance suites and reports, namespaces, name-history entries,
//! snapshots, foreign imports and the rest of the closed `registry/1` kind list —
//! under the **one** `idp/1` identity model (`hh-identity`), the **one** canonical
//! `ProvenanceRecord` (`hh-provenance`), the **one** canonicalizer (`hh-wire`) and the
//! **one** event store (`hh-ledger`) — CC1/CC2/CC7.
//!
//! This is the **C0/Stage-1 slice** (ADR-0151 decisions 1–5, 7; ADR-0152 Stage-1
//! shapes; ADR-0153 `hh/`+`local/` namespaces): file/ledger-backed store, the closed
//! kinds, the envelope and its three orthogonal status axes (R5: *admission* per
//! record, *publication* per name-history entry, *conformance* per declaration
//! field — derived), `register`/`publish`/`resolve`/`snapshot`/`verify`/`lineage`,
//! `NamespaceRecord` shape + `LocalityInadmissible` at `register`, suite/report
//! record shapes with suites for `control_strategy` and `context_policy`,
//! `require_conformance = declared`, and the `lifecycle.registry.*` audit events.
//! The deterministic `query`/`slot_choices`/`substitutable` projections land in
//! their minimal form because AC-R-2.10.2-1 names them (their floors/vector/catalog
//! semantics are Stage 2–3 — ADR-0239). The C1 multi-namespace *service* is S4.1 —
//! never this crate.
//!
//! Layout:
//! - [`kinds`] — the closed `RecordKind` list and the status/vocabulary sums.
//! - [`records`] — the typed record bodies (`ClassRecord`, `VariantRecord`,
//!   `ConformanceSuite`, `ConformanceReport`, `NamespaceRecord`, `RegistryPolicy`,
//!   `RegistrySnapshot`, `RegistryDiagnostic`, `RegistryEnvelope`).
//! - [`schema`] — the `registry/1` canonical encode/decode (the single schema
//!   source — CC7) including the per-kind semantic/surface identity split (N6).
//! - [`identity`] — `identify`/`verify` over the one `idp/1` construction.
//! - [`errors`] — the closed `RegistryError` sum (spec §6.2 §2 + §5).
//! - [`store`] — the `RegistryStore` and the operations.
//! - [`events`] — the `lifecycle.registry.*` payload builders (appended through the
//!   run's single fenced writer).
//! - [`suites`] — the two Stage-1 `ClassRecord`s + `ConformanceSuite`s and the
//!   minimal in-crate suite driver (the reference-runtime test-harness role,
//!   ADR-0152 (d)).
//! - [`corpus`] — the deterministic AC-1 golden-corpus builder.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capability;
pub mod corpus;
pub mod errors;
pub mod events;
pub mod extension;
pub mod foreign;
pub mod hosted;
pub mod identity;
pub mod import;
pub mod kinds;
pub mod records;
pub mod schema;
pub mod skill;
pub mod store;
pub mod suites;

pub use errors::RegistryError;
pub use store::{
    QueryClause, QueryOp, QueryPredicate, RegistryStore, ResolveInput, ResolveRequest,
    ResolvedRecord, SlotConstraints,
};
