//! `identity` (§3.1.2/§3.1.7) — the two-coordinate identity of every member, under the one
//! `idp/1` scheme (CC1: `hh_identity` is the only identity construction):
//!
//! - `semantic_id = H({kind ∥ semantic ∥ refs-by-semantic_id})` — the semantic projection
//!   ([`crate::schema::node_semantic_projection`]); surface, provenance, `ext` and the
//!   version record are excluded (N6). A rename/reorder/reword-template edit changes no
//!   `semantic_id` (AC-IR-05).
//! - `version_id = H(canonical(member))` — the canonical form with the computed id members
//!   removed ([`crate::schema::node_identity_basis`]), under `RecordKind::HirNode` /
//!   `RecordKind::HirEdge`.
//!
//! Edges carry a semantic id too — the version record shape is uniform across nodes and
//! edges (§3.1.1); an edge's projection is `{kind, from, to, fields(refs-by-semantic_id)}`
//! under `RecordKind::HirEdge`'s semantic domain.

use hh_identity::idp::{identify_bytes, idp_id};
use hh_identity::RecordKind;
use hh_wire::json::Json;

use crate::document::{DefinitionVersionRef, Edge, HirDocument, Node};
use crate::schema;

/// The semantic-projection domain of a record kind — the same construction
/// `hh_identity::record` uses (`<domain_tag>#semantic`; N3 keeps the two coordinates in
/// separate domains even over equal bytes).
fn semantic_domain(kind: RecordKind) -> String {
    format!("{}#semantic", kind.domain_tag())
}

fn hash(domain: &str, json: &Json) -> String {
    idp_id(domain, json.to_canonical_string().as_bytes())
}

/// The node's `semantic_id` (§3.1.2).
pub fn semantic_id(node: &Node) -> String {
    hash(
        &semantic_domain(RecordKind::HirNode),
        &schema::node_semantic_projection(node),
    )
}

/// The node's `version_id` — `H(canonical(node))` with the id members stripped (§3.1.2).
pub fn version_id(node: &Node) -> String {
    identify_bytes(
        RecordKind::HirNode,
        schema::node_identity_basis(node)
            .to_canonical_string()
            .as_bytes(),
    )
}

/// The edge's `semantic_id` — `{kind, from, to, fields}` with `fields`' refs by
/// `semantic_id` (§3.1.2 refs-by-semantic_id).
pub fn edge_semantic_id(edge: &Edge) -> String {
    hash(
        &semantic_domain(RecordKind::HirEdge),
        &schema::edge_semantic_projection(edge),
    )
}

/// The edge's `version_id` — `H(canonical(edge))` with the id members stripped.
pub fn edge_version_id(edge: &Edge) -> String {
    identify_bytes(
        RecordKind::HirEdge,
        schema::edge_identity_basis(edge)
            .to_canonical_string()
            .as_bytes(),
    )
}

/// Fill both coordinates on a node (`identity` — §3.1.7). Idempotent.
pub fn compute_node_ids(node: &mut Node) {
    node.version.semantic_id = Some(semantic_id(node));
    node.version.version_id = Some(version_id(node));
}

/// Fill both coordinates on an edge. Idempotent.
pub fn compute_edge_ids(edge: &mut Edge) {
    edge.version.semantic_id = Some(edge_semantic_id(edge));
    edge.version.version_id = Some(edge_version_id(edge));
}

/// Compute both coordinates on every member of the document (§3.1.7 `identity`). Order
/// independent — each member's ids are a pure function of its own canonical bytes.
pub fn compute_ids(doc: &mut HirDocument) {
    for n in &mut doc.nodes {
        compute_node_ids(n);
    }
    for e in &mut doc.edges {
        compute_edge_ids(e);
    }
}

/// The document's identity — the root's `{semantic_id, version_id}` (§3.1.2: a document is
/// identified by its sealed definition version). Requires the root to resolve; `seal`
/// checks this before calling.
pub fn document_identity(doc: &HirDocument) -> Option<DefinitionVersionRef> {
    doc.node(&doc.root.semantic_id)
        .map(|root| DefinitionVersionRef {
            semantic_id: root.semantic_id(),
            version_id: root.version_id(),
        })
}

/// `H(declaration ∥ participant_version)` — the `OpaqueProcess.version_identity`
/// (§3.1.6; a claim pinned by `seal`, never a participant's self-declaration).
pub fn opaque_version_identity(
    declaration: &crate::records::CapabilityDeclarationRecord,
    participant_version: &str,
) -> String {
    let payload = Json::obj([
        ("declaration", crate::schema::cap_decl_json(declaration)),
        ("participant_version", Json::str(participant_version)),
    ]);
    hash(RecordKind::CapabilityDeclaration.domain_tag(), &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::EntityKind;
    use crate::records::{BudgetRecord, KindRecord};
    use crate::refs::Ref;
    use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
    use std::collections::BTreeMap;

    fn budget_node() -> Node {
        let prov =
            ProvenanceRecord::minted(Origin::kernel("test"), PersistenceScope::Definition, 1);
        Node::new(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::new(),
                scope: "*".into(),
                parent: None,
                accounting: Ref::selected("sha256:acc", "latest"),
            }),
            prov,
        )
    }

    #[test]
    fn identity_is_idempotent() {
        let mut n = budget_node();
        compute_node_ids(&mut n);
        let (s1, v1) = (
            n.version.semantic_id.clone().unwrap(),
            n.version.version_id.clone().unwrap(),
        );
        compute_node_ids(&mut n);
        assert_eq!(n.version.semantic_id.as_deref(), Some(s1.as_str()));
        assert_eq!(n.version.version_id.as_deref(), Some(v1.as_str()));
        assert!(s1.starts_with("sha256:"));
        assert!(v1.starts_with("sha256:"));
        assert_ne!(s1, v1);
    }

    #[test]
    fn ext_and_provenance_do_not_change_semantic_id() {
        let mut a = budget_node();
        let mut b = a.clone();
        b.ext.insert("org.example/note".into(), Json::str("x"));
        b.provenance.created_at = 99;
        assert_eq!(semantic_id(&a), semantic_id(&b));
        assert_ne!(version_id(&a), version_id(&b));
        a.ext = b.ext.clone();
        a.provenance = b.provenance.clone();
        assert_eq!(version_id(&a), version_id(&b));
    }
}
