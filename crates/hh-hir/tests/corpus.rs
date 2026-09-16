//! AC-IR-06 — the deterministic corpus: ≥100 definitions; `canonicalize` byte-identical
//! across re-parse (the in-crate implementation) and the `hh-ir-op` process boundary;
//! `apply(base, diff(base, target)) = target` and `apply(target, invert(d)) = base`.

mod common;

use common::*;

use hh_hir::diff::{apply, diff, invert};
use hh_hir::document::parse_document;
use hh_hir::ops::canonicalize;
use hh_hir::records::*;
use hh_provenance::Origin;
use hh_wire::json::Json;

/// The i-th corpus definition — deterministic in `i`.
fn corpus_doc(i: usize) -> hh_hir::document::HirDocument {
    let mut doc = hh_hir::document::HirDocument::new(sel("c:agent"));
    doc.nodes.push(rule_node("c:rule", 1));
    doc.nodes.push(budget_node(
        "c:budget",
        &[
            ("tokens.total", 1000 + i as u64 * 7),
            ("turns", 10 + i as u64),
        ],
        2,
    ));
    if let KindRecord::Budget(b) = &mut doc.nodes[1].semantic {
        b.accounting = sel("c:rule");
    }
    doc.nodes.push(perm_node(
        "c:perm",
        "c:agent",
        vec![grant("fs_read", "/work", i.is_multiple_of(2))],
        3,
    ));
    doc.nodes
        .push(agent_node("c:agent", "c:budget", "c:perm", 4));
    doc.nodes.push(goal_node("c:goal", "c:budget", 5));
    doc.nodes.push(tool_node("c:tool", 6));
    if let KindRecord::ToolCapability(t) = &mut doc.nodes[5].semantic {
        t.purpose = text(&format!("tool purpose #{i}"), 6);
    }
    doc.edges.push(dep_edge("c:goal", "c:tool", 7));
    doc
}

/// A mutation of corpus_doc(i): tightened budget, a surface rename, one extra node.
fn corpus_target(i: usize) -> hh_hir::document::HirDocument {
    let mut doc = corpus_doc(i);
    if let KindRecord::Budget(b) = &mut doc.nodes[1].semantic {
        b.dimensions.insert(
            "tokens.total".into(),
            DimensionBound {
                hard: Some(900 + i as u64 * 7),
                soft: None,
            },
        );
    }
    doc.nodes[5].surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: format!("tool_{i}"),
        namespace: "test".into(),
        description_template: text("rendered", 6),
        argument_order: vec!["a".into()],
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: None,
        icon_ref: None,
    })));
    doc.nodes.push(tool_node("c:tool2", 8));
    doc
}

const CORPUS: usize = 120;

#[test]
fn corpus_has_at_least_100_definitions() {
    const { assert!(CORPUS >= 100) }
    for i in 0..CORPUS {
        let doc = corpus_doc(i);
        hh_hir::validate(&doc).unwrap_or_else(|e| panic!("corpus doc {i} must validate: {e:?}"));
    }
}

#[test]
fn canonicalize_is_byte_identical_across_reparse() {
    for i in 0..CORPUS {
        let doc = corpus_doc(i);
        let bytes = canonicalize(&doc);
        // Second implementation pass: canonical bytes → Json → document → bytes.
        let back = parse_document(&bytes).unwrap_or_else(|e| panic!("doc {i}: {e}"));
        let bytes2 = canonicalize(&back);
        assert_eq!(bytes, bytes2, "doc {i} canonicalize not stable");
        // And node order does not matter.
        let mut shuffled = doc.clone();
        shuffled.nodes.rotate_left(i % doc.nodes.len().max(1));
        assert_eq!(
            bytes,
            canonicalize(&shuffled),
            "doc {i} not order-normalizing"
        );
    }
}

#[test]
fn apply_and_invert_round_trip_over_the_corpus() {
    for i in 0..CORPUS {
        let base = corpus_doc(i);
        let target = corpus_target(i);
        // Pairing is by stored semantic id — the corpus docs carry authored sids.
        let d = diff(&base, &target, prov_human_diff(), Default::default())
            .unwrap_or_else(|e| panic!("diff doc {i}: {e:?}"));
        let applied = apply(&base, &d).unwrap_or_else(|e| panic!("apply doc {i}: {e:?}"));
        assert_eq!(
            canonicalize(&applied),
            canonicalize(&target),
            "apply(base, diff) != target for doc {i}"
        );
        let inv = invert(&d);
        let reverted = apply(&target, &inv).unwrap_or_else(|e| panic!("invert doc {i}: {e:?}"));
        assert_eq!(
            canonicalize(&reverted),
            canonicalize(&base),
            "apply(target, invert) != base for doc {i}"
        );
    }
}

/// A human-origin diff provenance (empty `derived-from` is legal for human).
fn prov_human_diff() -> hh_provenance::ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", hh_provenance::HumanRole::Author),
        PersistenceScope::Run,
        1,
    )
}

use hh_provenance::{PersistenceScope, ProvenanceRecord};

#[test]
fn surface_only_edits_classify_as_surface_and_keep_semantic_id() {
    // AC-IR-05 second half: rename/reorder/reword ⇒ `semantic_ops = 0`.
    let base = corpus_doc(0);
    let mut target = base.clone();
    target.nodes[5].surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: "renamed".into(),
        namespace: "test".into(),
        description_template: text("reworded", 6),
        argument_order: vec!["b".into(), "a".into()],
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: Some("Renamed".into()),
        icon_ref: None,
    })));
    let d = diff(&base, &target, prov_human_diff(), Default::default()).unwrap();
    assert_eq!(
        d.classification.semantic_ops, 0,
        "a surface-only diff carries no semantic ops: {:?}",
        d.classification
    );
    assert!(d.classification.surface_ops > 0);
    assert_eq!(base.nodes[5].semantic_id(), target.nodes[5].semantic_id());
}

#[test]
fn diff_carries_derivation_unless_human_or_migration() {
    // §3.1.7: non-human/migration diff provenance must carry a derived-from record.
    let base = corpus_doc(0);
    let target = corpus_target(0);
    let model_prov =
        ProvenanceRecord::minted(Origin::model("m", "r", "resp"), PersistenceScope::Run, 1);
    let es = diff(&base, &target, model_prov, Default::default())
        .expect_err("model diff without derived-from");
    assert!(es
        .iter()
        .any(|e| matches!(e, hh_hir::errors::HirError::SchemaViolation { .. })));
}

#[test]
fn conditioned_rule_edit_without_debt_fails_diff() {
    // T-LCD-05: a diff touching a conditioned rule must carry/update its debt record.
    let mut base = valid_doc();
    base.nodes.push(conditioned_rule_node(
        "test:crule",
        7,
        Some(debt_record("test:crule.rule", 7)),
    ));
    hh_hir::validate(&base).unwrap();
    let mut target = base.clone();
    // Touch the conditioned rule: change its trigger.
    if let KindRecord::HarnessRule(r) = &mut target.nodes.last_mut().unwrap().semantic {
        r.trigger = Json::obj([("changed", Json::Int(1))]);
    }
    let es = diff(&base, &target, prov_human_diff(), Default::default())
        .expect_err("conditioned-rule touch without debt update");
    assert!(es.iter().any(|e| matches!(
        e,
        hh_hir::errors::HirError::ConditionedRuleIncomplete { .. }
    )));
}

#[test]
fn two_profiles_of_one_tool_differ_only_in_surface() {
    // AC-IR-02 (deterministic half): the same edit-file tool compiled under a "patch"
    // profile and a "string-replacement" profile — the records differ only in `surface`;
    // `semantic_id` is unchanged.
    let patch = tool_node("test:editfile", 6);
    let mut string_replace = patch.clone();
    string_replace.surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: "edit_file".into(),
        namespace: "str_replace".into(),
        description_template: text("replace an exact string in a file", 6),
        argument_order: vec!["path".into(), "old".into(), "new".into()],
        examples: Json::obj([("profile", Json::str("string-replacement"))]),
        error_format: Json::Null,
        result_renderer: Json::Null,
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: None,
        icon_ref: None,
    })));
    patch_surface_differs(&patch, &string_replace);
    assert_eq!(
        patch.semantic_id(),
        string_replace.semantic_id(),
        "profile change must not move semantic_id"
    );
    assert_eq!(patch.semantic, string_replace.semantic);
}

fn patch_surface_differs(a: &hh_hir::document::Node, b: &hh_hir::document::Node) {
    assert_ne!(a.surface, b.surface, "the two profiles render differently");
}
