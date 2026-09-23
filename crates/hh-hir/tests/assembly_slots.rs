//! The §3.3.2 slot-binding carriage on `AgentProcess.native.slots` (S1.9, R-2.1.4): the
//! `ComponentVariantRef{class_id, variant_id, version}` shape, `params`/`enabled`/`locality`
//! round-trip, the semantic projection by name, and the diff ops assembly edits map onto
//! (`Rebind` for a variant change, `ReplaceField` for `params`/`enabled` — §3.3.4 `diff`).

mod common;

use std::collections::BTreeMap;

use common::*;
use hh_hir::diff::{diff, DiffDerivation, DiffOp};
use hh_hir::document::{parse_document, HirDocument};
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, RefVersion};
use hh_hir::seal;
use hh_wire::json::Json;

fn root_binding_mut<'a>(doc: &'a mut HirDocument, slot: &str) -> &'a mut SlotBinding {
    let agent = doc.nodes.len() - 1;
    match &mut doc.nodes[agent].semantic {
        KindRecord::AgentProcess(a) => match &mut a.body {
            AgentProcessBody::Native(n) => match n.slots.get_mut(slot).unwrap() {
                SlotBindings::One(b) => b,
                SlotBindings::Many(_) => panic!("fixture uses one-bindings"),
            },
            _ => panic!("native"),
        },
        _ => panic!("agent"),
    }
}

fn human_prov() -> hh_provenance::ProvenanceRecord {
    hh_provenance::ProvenanceRecord::minted(
        hh_provenance::Origin::human("test:author", hh_provenance::HumanRole::Author),
        hh_provenance::PersistenceScope::Definition,
        9,
    )
}

#[test]
fn slot_bindings_round_trip_with_params_enabled_and_locality_defaults() {
    let mut doc = valid_doc();
    {
        let b = root_binding_mut(&mut doc, "context_policy");
        b.params
            .insert("keep_recent_tokens".into(), Json::Int(20_000));
        b.enabled = false;
        b.locality = Some(Locality::InProcess);
    }
    let bytes = doc.canonical_bytes();
    let back = parse_document(&bytes).expect("round trip");
    assert_eq!(back.canonical_bytes(), bytes);
    assert_eq!(back.nodes.len(), doc.nodes.len());
    // Absent `params`/`enabled` parse to the defaults (`{}` / `true`) — CC8 back-compat.
    let j = doc.to_json();
    let node = match j.get("nodes") {
        Some(Json::Arr(v)) => v
            .iter()
            .find(|n| n.get("kind").and_then(Json::as_str) == Some("AgentProcess"))
            .unwrap(),
        _ => unreachable!(),
    };
    let binding = node
        .get("semantic")
        .and_then(|s| s.get("body"))
        .and_then(|b| b.get("native"))
        .and_then(|n| n.get("slots"))
        .and_then(|s| s.get("control_strategy"))
        .and_then(|s| s.get("one"))
        .cloned()
        .unwrap();
    assert_eq!(binding.get("enabled"), Some(&Json::Bool(true)));
    assert_eq!(binding.get("params"), Some(&Json::Obj(BTreeMap::new())));
    let mut stripped = j.to_canonical_string();
    stripped = stripped.replacen(r#""enabled":true,"params":{},"#, "", 1);
    let back = parse_document(stripped.as_bytes()).expect("defaults fill in");
    assert_eq!(back.canonical_bytes(), bytes);
    // An unknown SlotBinding member is refused (closed product type).
    let bad =
        j.to_canonical_string()
            .replacen(r#""enabled":true,"#, r#""enabled":true,"extra":1,"#, 1);
    assert!(parse_document(bad.as_bytes()).is_err());
}

#[test]
fn variant_pin_is_version_only_but_name_params_and_enabled_are_semantic() {
    // §3.3.2: a disabled binding still counts toward identity; a re-pin under one name
    // changes version_id only (refs-by-semantic_id). Computed ids are asserted (the fixture
    // authors `test:` sids).
    let base = valid_doc();
    let sem = |d: &HirDocument| hh_hir::identity::semantic_id(d.nodes.last().unwrap());
    let ver = |d: &HirDocument| hh_hir::identity::version_id(d.nodes.last().unwrap());
    let (sem0, ver0) = (sem(&base), ver(&base));

    let mut repinned = base.clone();
    root_binding_mut(&mut repinned, "context_policy")
        .variant
        .version = RefVersion::Pinned("sha256:other-version".into());
    assert_eq!(sem(&repinned), sem0);
    assert_ne!(ver(&repinned), ver0);

    let mut renamed = base.clone();
    root_binding_mut(&mut renamed, "context_policy")
        .variant
        .variant_id = "hh/context_policy/other".into();
    assert_ne!(sem(&renamed), sem0);

    let mut disabled = base.clone();
    root_binding_mut(&mut disabled, "context_policy").enabled = false;
    assert_ne!(sem(&disabled), sem0);

    let mut param = base.clone();
    root_binding_mut(&mut param, "context_policy")
        .params
        .insert("k".into(), Json::Int(1));
    assert_ne!(sem(&param), sem0);
}

/// The semantic ops of a diff — everything but provenance/version-record members and the
/// version-only re-pins sealing derives (`classification.semantic_ops` counts these).
fn semantic_ops(d: &hh_hir::HirDiff) -> Vec<&DiffOp> {
    d.ops
        .iter()
        .filter(|op| match op {
            DiffOp::ReplaceField { path, .. } => {
                !path.starts_with("provenance") && !path.starts_with("version")
            }
            DiffOp::Rebind {
                old_ref, new_ref, ..
            } => {
                let coord = |j: &Json| {
                    (
                        j.get("semantic_id").cloned(),
                        j.get("class_id").cloned(),
                        j.get("variant_id").cloned(),
                    )
                };
                coord(old_ref) != coord(new_ref)
            }
            _ => true,
        })
        .collect()
}

#[test]
fn ablation_is_one_replace_field_and_a_variant_swap_is_one_rebind() {
    // AC-CC-06 (second half) over **sealed** documents; §3.3.4 `diff` row. Sealing re-pins
    // every ref to the changed root and re-stamps its version/provenance records — those
    // are provenance-only consequences (`classification.provenance_only_ops`), never
    // semantic ops; the one semantic op is the edit itself.
    let base = seal(&valid_doc(), 1).unwrap().document;
    let mut ablated_src = valid_doc();
    root_binding_mut(&mut ablated_src, "context_policy").enabled = false;
    let ablated = seal(&ablated_src, 1).unwrap().document;
    let d = diff(&base, &ablated, human_prov(), DiffDerivation::default()).unwrap();
    let sem = semantic_ops(&d);
    assert_eq!(d.classification.semantic_ops, 1, "{:?}", d.ops);
    assert_eq!(sem.len(), 1, "exactly one semantic op: {:?}", d.ops);
    assert!(matches!(sem[0], DiffOp::ReplaceField { path, old, new, .. }
        if path.ends_with("slots.context_policy.one.enabled") && *old == Json::Bool(true) && *new == Json::Bool(false)));
    assert_eq!(
        hh_hir::apply(&base, &d).unwrap().canonical_bytes(),
        ablated.canonical_bytes()
    );

    let mut swapped_src = valid_doc();
    root_binding_mut(&mut swapped_src, "context_policy").variant =
        ComponentVariantRef::pinned("context_policy", "hh/context_policy/other", "sha256:v2");
    let swapped = seal(&swapped_src, 1).unwrap().document;
    let d = diff(&base, &swapped, human_prov(), DiffDerivation::default()).unwrap();
    let sem = semantic_ops(&d);
    assert_eq!(d.classification.semantic_ops, 1, "{:?}", d.ops);
    assert_eq!(sem.len(), 1, "exactly one semantic op: {:?}", d.ops);
    assert!(matches!(sem[0], DiffOp::Rebind { path, .. }
        if path.ends_with("slots.context_policy.one.variant")));
    assert_eq!(
        hh_hir::apply(&base, &d).unwrap().canonical_bytes(),
        swapped.canonical_bytes()
    );

    // A re-pin of the same variant name is version-only: zero semantic ops.
    let mut repinned_src = valid_doc();
    root_binding_mut(&mut repinned_src, "context_policy")
        .variant
        .version = RefVersion::Pinned("sha256:v9".into());
    let repinned = seal(&repinned_src, 1).unwrap().document;
    let d = diff(&base, &repinned, human_prov(), DiffDerivation::default()).unwrap();
    assert_eq!(d.classification.semantic_ops, 0, "{:?}", d.ops);
    assert!(d.classification.provenance_only_ops >= 1);
}

#[test]
fn assembly_section_diffs_at_leaf_granularity() {
    let mut a = valid_doc();
    a.assembly = Some(Json::obj([
        ("dialect", Json::str("HIR/1")),
        (
            "values",
            Json::obj([("k", Json::Int(1)), ("j", Json::Int(2))]),
        ),
    ]));
    let mut b = a.clone();
    b.assembly = Some(Json::obj([
        ("dialect", Json::str("HIR/1")),
        (
            "values",
            Json::obj([("k", Json::Int(3)), ("j", Json::Int(2))]),
        ),
    ]));
    let d = diff(&a, &b, human_prov(), DiffDerivation::default()).unwrap();
    assert_eq!(d.ops.len(), 1);
    assert!(matches!(&d.ops[0], DiffOp::ReplaceField { id, path, .. }
        if id == "$doc" && path == "assembly.values.k"));
    assert_eq!(
        hh_hir::apply(&a, &d).unwrap().canonical_bytes(),
        b.canonical_bytes()
    );
    let inv = hh_hir::invert(&d);
    assert_eq!(
        hh_hir::apply(&b, &inv).unwrap().canonical_bytes(),
        a.canonical_bytes()
    );
}
