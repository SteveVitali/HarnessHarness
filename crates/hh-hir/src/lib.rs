//! `hh-hir` — the **Harness IR** (spec §3.1; ticket S1.4; R-2.1.2).
//!
//! The dialect `HIR/1`: a `HirDocument` is a DAG of thirteen entity kinds over seven edge
//! kinds with two opaque leaves (`Text`, `CompiledPayload`), a provenance record and a
//! version record on every node and edge (§3.1.1), and the §3.1.7 operation set —
//! `validate`, `canonicalize`, `identity`, `seal`, `diff`/`apply`/`invert`/`classify`,
//! identity-only `migrate`, and `project` — each executable over the canonical encoding
//! out-of-process (AC-IR-10; see the `hh-ir-op` binary).
//!
//! This crate sits above `hh-ontology` (the per-kind home table — DF-S1.1-1), `hh-identity`
//! (the `idp/1` two-coordinate identity), `hh-provenance` (the mandatory record and the
//! `seal`-basis endorsement) and `hh-wire` (the one canonicalizer + `idp/1` hashing). It has
//! **no** dependency on the Hosting ABI — `OpaqueProcess` is a HIR record (AC-IR-04).
//!
//! The wire schema is defined once in [`schema`] (CC7): parse via
//! [`document::parse_document`] (`NonCanonicalInput` / `MissingProvenance` / `UnknownKind` /
//! `DialectUnsupported`), encode via `to_json`/`canonical_bytes`.

pub mod debt;
pub mod diff;
pub mod document;
pub mod errors;
pub mod identity;
pub mod kinds;
pub mod leaves;
pub mod ops;
pub mod records;
pub mod refs;
pub mod risk;
pub(crate) mod schema;
pub mod tools;

// The canonical `AssumptionDebtRecord` codec is re-exported for `hh-registry`
// (`VariantRecord.conditioned_rules` — CC7: the schema source owns both
// directions; additive — CC8).
pub use schema::{debt_from_json, debt_json, grant_from_json, grant_json};
pub mod validate;

/// The one dialect this crate implements (`HIR/1` — CC8: growth is an additive bump).
pub const DIALECT: &str = "HIR/1";

pub use diff::{
    apply, classify, diff, invert, AuthorityDelta, Delta, DiffClassification, DiffDerivation,
    DiffOp, DiffOpTag, HirDiff,
};
pub use document::{
    parse_document, DefinitionVersionRef, Edge, HirDocument, Node, SealedDefinition,
};
pub use errors::{HirError, ValidationReport};
pub use identity::{
    compute_edge_ids, compute_ids, compute_node_ids, document_identity, edge_semantic_id,
    edge_version_id, opaque_version_identity, semantic_id, version_id,
};
pub use kinds::*;
pub use leaves::{CompiledPayload, DeclaredInterface, Text};
pub use ops::{
    apply_bytes, canonicalize, canonicalize_bytes, closed_world_tools, diff_bytes, migrate,
    migrate_bytes, project, project_bytes, seal, seal_bytes, validate_bytes, validate_doc,
    ProjectSelector, SubGraph,
};
pub use records::*;
pub use refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref, RefVersion, RunRef};
pub use validate::validate;

/// The canonical-codec entry points the out-of-process seam needs (`hh-ir-op`; AC-IR-10) —
/// thin wrappers over the [`schema`] source of truth.
pub mod wire {
    use hh_wire::json::Json;

    use crate::diff::HirDiff;
    use crate::errors::HirError;

    /// Parse a `ProvenanceRecord` from its canonical JSON.
    pub fn provenance_from_json(j: &Json) -> Result<hh_provenance::ProvenanceRecord, HirError> {
        crate::schema::provenance_from_json(j, "$")
    }

    /// The canonical JSON of a [`HirDiff`].
    pub fn diff_to_json(d: &HirDiff) -> Json {
        crate::schema::diff_to_json(d)
    }

    /// Parse a [`HirDiff`] from its canonical JSON.
    pub fn diff_from_json(j: &Json) -> Result<HirDiff, HirError> {
        crate::schema::diff_from_json(j)
    }

    /// The canonical JSON of a `slots` map — the §3.3.2 slot grammar shared by
    /// `native.slots` and `assembly.slots` (CC7; consumed by `hh-assembly`).
    pub fn slots_json(
        slots: &std::collections::BTreeMap<String, crate::records::SlotBindings>,
        semantic: bool,
    ) -> Json {
        crate::schema::slots_json(slots, semantic)
    }

    /// Parse a `slots` map (CC7; consumed by `hh-assembly`'s `load`).
    pub fn slots_from_json(
        j: &Json,
        path: &str,
    ) -> Result<std::collections::BTreeMap<String, crate::records::SlotBindings>, HirError> {
        crate::schema::slots_from_json(j, path)
    }

    /// The canonical JSON of one `SlotBinding`.
    pub fn slot_binding_json(b: &crate::records::SlotBinding, semantic: bool) -> Json {
        crate::schema::slot_binding_json(b, semantic)
    }

    /// Parse one `SlotBinding`.
    pub fn slot_binding_from_json(
        j: &Json,
        path: &str,
    ) -> Result<crate::records::SlotBinding, HirError> {
        crate::schema::slot_binding_from_json(j, path)
    }

    /// Parse a [`crate::document::HirDocument`] from its canonical JSON — the
    /// out-of-process seam's read direction (`hh-compile` — CC7: the schema source
    /// owns the decoding; `parse_document` owns the byte-level entry).
    pub fn document_from_json(j: &Json) -> Result<crate::document::HirDocument, HirError> {
        crate::schema::document_from_json(j)
    }

    /// The canonical JSON of a [`crate::document::Node`] (consumed by `hh-assembly`'s
    /// stage-7 LCD walks — CC7: the schema source owns the encoding).
    pub fn node_to_json(n: &crate::document::Node) -> Json {
        crate::schema::node_to_json(n)
    }

    /// A node's canonical **semantic projection** (no surface/provenance/version —
    /// the `semantic_id` basis, §3.1.2).
    pub fn node_semantic_projection(n: &crate::document::Node) -> Json {
        crate::schema::node_semantic_projection(n)
    }

    /// Parse a [`crate::document::Node`] from its canonical JSON (consumed by
    /// `hh-registry`'s `CapabilityRecord` body — CC7: the schema source owns the
    /// decoding).
    pub fn node_from_json(j: &Json) -> Result<crate::document::Node, HirError> {
        crate::schema::node_from_json(j, "$")
    }

    /// The canonical JSON of a `ControlBoundary` (the CC11 record — consumed by
    /// `hh-compiler`'s `BudgetEnvelope` codec; the schema source owns the encoding).
    pub fn boundary_json(b: &hh_ontology::control::ControlBoundary) -> Json {
        crate::schema::boundary_json(b)
    }

    /// Parse a `ControlBoundary` (the codec's read direction).
    pub fn boundary_from_json(
        j: &Json,
        path: &str,
    ) -> Result<hh_ontology::control::ControlBoundary, HirError> {
        crate::schema::boundary_from_json(j, path)
    }
}
