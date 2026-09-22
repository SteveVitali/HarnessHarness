//! `seal` / `identity` / `canonicalize` / `migrate` / `project` (§3.1.6/§3.1.7;
//! AC-IR-05 surface-stability, DF-S1.3-3 closed-world minting).

mod common;

use common::*;

use hh_hir::document::parse_document;
use hh_hir::errors::HirError;
use hh_hir::kinds::EntityKind;
use hh_hir::ops::{canonicalize, migrate, project, seal, ProjectSelector};
use hh_hir::records::*;
use hh_hir::refs::RefVersion;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope};

#[test]
fn seal_pins_every_ref_and_marks_members() {
    // The fixture carries the `Permission.holder ↔ AgentProcess.permissions` cycle —
    // seal must still terminate (pins bind member identities, not member content).
    let def = seal(&valid_doc(), 100).expect("seal succeeds on the cyclic fixture");
    for n in &def.document.nodes {
        assert!(n.version.sealed, "node sealed");
        assert!(n.version.version_id.is_some(), "version id computed");
        assert!(n.version.semantic_id.is_some());
        assert_eq!(
            n.provenance.authority,
            hh_provenance::AuthorityClass::Definition,
            "seal-conferred definition authority"
        );
    }
    // Every Ref is pinned.
    for n in &def.document.nodes {
        let j = crate::semantic_json_has_no_selectors(n);
        assert!(j, "node {} carries an unpinned ref", n.semantic_id());
    }
    // The root ref is pinned to the root member's version id.
    let root = def.document.node(&def.document.root.semantic_id).unwrap();
    assert_eq!(
        def.document.root.version,
        RefVersion::Pinned(root.version_id())
    );
    assert_eq!(def.definition_ref.semantic_id, root.semantic_id());
}

/// Scan a node's canonical JSON for `version_selector` members.
fn semantic_json_has_no_selectors(n: &hh_hir::document::Node) -> bool {
    fn has_selector(j: &Json) -> bool {
        match j {
            Json::Obj(m) => m.contains_key("version_selector") || m.values().any(has_selector),
            Json::Arr(items) => items.iter().any(has_selector),
            _ => false,
        }
    }
    !has_selector(&n.to_json())
}

use hh_wire::json::Json;

#[test]
fn seal_is_deterministic() {
    let a = seal(&valid_doc(), 100).unwrap();
    let b = seal(&valid_doc(), 100).unwrap();
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
}

#[test]
fn seal_refuses_assembly_section() {
    let mut doc = valid_doc();
    doc.assembly = Some(Json::Null);
    let es = seal(&doc, 1).expect_err("assembly doc is not sealable");
    assert!(es
        .iter()
        .any(|e| matches!(e, HirError::UnresolvedRef { .. })));
}

#[test]
fn seal_refuses_unresolvable_refs() {
    let mut doc = valid_doc();
    if let KindRecord::Budget(b) = &mut doc.nodes[1].semantic {
        b.accounting = sel("test:missing");
    }
    let es = seal(&doc, 1).expect_err("unresolved ref");
    assert!(es
        .iter()
        .any(|e| matches!(e, HirError::UnresolvedRef { .. })));
}

#[test]
fn sealed_document_revalidates_and_round_trips() {
    let def = seal(&valid_doc(), 7).unwrap();
    hh_hir::validate(&def.document).expect("sealed doc validates");
    let reparsed = parse_document(&def.canonical_bytes()).expect("canonical round trip");
    hh_hir::validate(&reparsed).expect("reparsed sealed doc validates");
}

#[test]
fn semantic_id_is_stable_under_surface_edits() {
    // AC-IR-05: rename + argument reorder + reworded template ⇒ unchanged semantic_id.
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 9));
    hh_hir::compute_ids(&mut doc);
    let t_idx = doc.nodes.len() - 1;
    let before = doc.nodes[t_idx].semantic_id();

    doc.nodes[t_idx].surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: "edit_file".into(),
        namespace: "fs".into(),
        description_template: text("desc v1", 9),
        argument_order: vec!["path".into(), "patch".into()],
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: Some("Edit File".into()),
        icon_ref: None,
    })));
    let after_surface = doc.nodes[t_idx].semantic_id();
    assert_eq!(before, after_surface, "surface never enters semantic_id");

    // Rename + reorder + reword.
    if let Some(SurfaceRecord::Tool(t)) = &mut doc.nodes[t_idx].surface {
        t.name = "apply_patch".into();
        t.argument_order = vec!["patch".into(), "path".into()];
        t.description_template = text("desc v2 — reworded", 10);
        t.display_title = Some("Apply Patch".into());
    }
    assert_eq!(before, doc.nodes[t_idx].semantic_id());
}

#[test]
fn semantic_id_changes_with_semantic_content() {
    let mut a = tool_node("test:tool", 9);
    let mut b = a.clone();
    if let KindRecord::ToolCapability(t) = &mut b.semantic {
        t.purpose = text("a different purpose", 10);
    }
    assert_ne!(
        hh_hir::identity::semantic_id(&a),
        hh_hir::identity::semantic_id(&b)
    );
    a.ext.insert("org.example/k".into(), Json::Int(1));
    assert_eq!(
        hh_hir::identity::semantic_id(&a),
        hh_hir::identity::semantic_id(&tool_node("test:tool", 9))
    );
}

#[test]
fn canonicalize_is_order_normalizing_and_parse_stable() {
    let doc = valid_doc();
    let mut reversed = doc.clone();
    reversed.nodes.reverse();
    assert_eq!(canonicalize(&doc), canonicalize(&reversed));
    let bytes = canonicalize(&doc);
    let back = parse_document(&bytes).expect("parse canonical");
    assert_eq!(canonicalize(&back), bytes, "parse∘canonicalize is stable");
}

#[test]
fn migrate_is_identity_only() {
    let doc = valid_doc();
    let out = migrate(&doc, "HIR/1", "HIR/1").expect("identity migrate");
    assert_eq!(canonicalize(&out), canonicalize(&doc));
    let es = migrate(&doc, "HIR/1", "HIR/2").expect_err("no HIR/2 yet");
    assert!(es
        .iter()
        .any(|e| matches!(e, HirError::DialectUnsupported { .. })));
    let mut wrong = doc.clone();
    wrong.hir_version = "HIR/0".into();
    let es = migrate(&wrong, "HIR/0", "HIR/1").expect_err("wrong from");
    assert!(es.iter().any(|e| matches!(
        e,
        HirError::DialectUnsupported { .. } | HirError::DialectIncompatible { .. }
    )));
}

#[test]
fn project_by_plane_and_kind() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 9));
    let sub = project(&doc, ProjectSelector::Kind(EntityKind::ToolCapability));
    assert_eq!(sub.nodes.len(), 1);
    let sub = project(
        &doc,
        ProjectSelector::Plane(hh_ontology::planes::Plane::Action),
    );
    assert_eq!(sub.nodes.len(), 1, "only the tool homes to Action");
    let sub = project(
        &doc,
        ProjectSelector::Plane(hh_ontology::planes::Plane::Measurement),
    );
    assert_eq!(sub.nodes.len(), 2, "budget + rule home to Measurement");
}

#[test]
fn closed_world_tools_mint_environment() {
    // DF-S1.3-3: a tool declared closed-world in the sealed definition mints
    // `environment` through the minting context; undeclared tools stay `external`.
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:cw_tool", 9));
    let t_idx = doc.nodes.len() - 1;
    if let KindRecord::ToolCapability(t) = &mut doc.nodes[t_idx].semantic {
        let mut set = std::collections::BTreeSet::new();
        let mut e = hh_hir::kinds::EffectClass::domain_only(hh_hir::kinds::EffectDomain::FsRead);
        e.attributes = Some(hh_hir::kinds::EffectAttributes {
            mutability: hh_hir::kinds::Mutability::ReadOnly,
            repeat_safety: hh_hir::kinds::RepeatSafety::Idempotent,
            world: hh_hir::kinds::World::Closed,
            reversibility: hh_hir::kinds::Reversibility::Reversible(sel("test:proc")),
        });
        set.insert(e);
        t.effects = hh_hir::kinds::ToolEffects::Declared(set);
    }
    // The reversibility ref must resolve — add a procedure node.
    doc.nodes.push(sid(
        node(
            EntityKind::Procedure,
            KindRecord::Procedure(ProcedureRecord {
                preconditions: Json::Null,
                steps: vec![],
                expected_evidence: Json::Null,
                allowed_capabilities: vec![],
                failure_handlers: Json::Null,
            }),
            10,
        ),
        "test:proc",
    ));
    let def = seal(&doc, 5).expect("seal");
    assert!(def.closed_world_tools.contains("test:cw_tool"));

    let ctx = def.minting_context();
    let origin = Origin::tool("test:cw_tool", "inv:1");
    assert_eq!(
        hh_provenance::default_authority_in(&origin, PersistenceScope::Definition, None, &ctx),
        AuthorityClass::Environment,
        "closed-world tool mints environment inside the sealed definition"
    );
    // Without the context (no sealed declaration) the same origin mints `external`.
    assert_eq!(
        hh_provenance::default_authority(&origin, PersistenceScope::Definition, None),
        AuthorityClass::External
    );
    // Free text never rises (R-TEXT) even for a closed-world capability.
    assert_eq!(
        hh_provenance::default_text_authority(&origin, None),
        AuthorityClass::External
    );
}

#[test]
fn opaque_process_fields_round_trip() {
    // `OpaqueProcess` is an HIR record — the full field set survives canonical round-trip.
    let doc = hosted_doc();
    let bytes = canonicalize(&doc);
    let back = parse_document(&bytes).unwrap();
    let agent = back
        .nodes
        .iter()
        .find(|n| n.kind == EntityKind::AgentProcess)
        .expect("agent node present");
    if let KindRecord::AgentProcess(a) = &agent.semantic {
        match &a.body {
            AgentProcessBody::Hosted(h) => {
                assert_eq!(h.participant_ref, "participant:test");
                assert_eq!(
                    h.hosting_mechanism,
                    hh_ontology::participant::HostingMechanism::SessionAbi
                );
                assert_eq!(
                    h.declared_capabilities.streaming,
                    CapabilityState::Unknown,
                    "unknown is never coerced"
                );
            }
            _ => panic!("hosted body expected"),
        }
    } else {
        panic!("agent node expected");
    }
}
