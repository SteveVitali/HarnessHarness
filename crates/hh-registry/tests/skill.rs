//! S2.8 skill-lift evidence — `R-2.4.5⁰` (§5c.5; ADR-0086 D1).
//!
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.5-1** — `lift_skill` on a spec-conformant skill tree yields a
//!   HIR/1 `Procedure` node with exactly one `Instruction` step per body,
//!   `scripts/*` as `CompiledPayload`s behind `Opaque` steps with declared
//!   interfaces, `references/*`/`assets/*` as content-addressed `Artifact`s,
//!   `text_authority = external`, `allowed_capabilities = ∅` (claims, never
//!   grants) and the `ProcedureProfile/1` ext block.
//! - **AC-R-2.4.5-3** — render purity: a body carrying the declared exec
//!   marker lifts faithfully but fails `hh_hir::validate` with
//!   `RenderTimeExecution` (the lift is honest; the gate refuses).

use std::collections::BTreeMap;

use hh_hir::errors::HirError;
use hh_hir::kinds::EntityKind;
use hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY;
use hh_hir::records::{KindRecord, ProcedureStep};
use hh_registry::extension::{ExtensionKind, ExtensionRecord, ExtensionTrustRecord, SourceLocator};
use hh_registry::skill::{lift_skill, LiftError, SkillFile, SkillTree};
use hh_wire::json::Json;

fn skill_ext(name: &str) -> ExtensionRecord {
    ExtensionRecord {
        kind: ExtensionKind::Skill,
        name: name.into(),
        content: hh_identity::idp::address(b"skill-tree", "application/octet-stream"),
        manifest: Json::Null,
        contributes: vec![],
        locator: SourceLocator {
            scheme: "directory_scan".into(),
            credential_free_uri: "file:///skills/demo".into(),
            selector: None,
            resolved: Some("sha256:pin".into()),
            fetched_at: Some(1),
        },
        trust: ExtensionTrustRecord::unresolved_default(hh_provenance::PersistenceScope::Project),
        provenance: hh_provenance::ProvenanceRecord::kernel("test", 0),
        ext: BTreeMap::new(),
    }
}

fn demo_tree() -> SkillTree {
    SkillTree {
        name: "demo-skill".into(),
        description: "Demonstrates the lift.".into(),
        license: Some("MIT".into()),
        compatibility: Some("requires bash".into()),
        requires_env: vec!["DEMO_HOME".into()],
        metadata: [("version".to_string(), "1.2.3".to_string())]
            .into_iter()
            .collect(),
        allowed_tools: vec!["fs_read".into(), "shell_exec".into()],
        body: "# Demo\nDo the demo thing.".into(),
        scripts: vec![SkillFile {
            path: "scripts/run.sh".into(),
            bytes: b"#!/bin/sh\nexit 0".to_vec(),
            media_type: "text/x-shellscript".into(),
        }],
        references: vec![SkillFile {
            path: "references/spec.md".into(),
            bytes: b"# spec".to_vec(),
            media_type: "text/markdown".into(),
        }],
        assets: vec![],
        triggers: vec![hh_registry::skill::SkillTrigger::PathGlob {
            pattern: "src/**".into(),
        }],
        tree_path: Some("skills/demo".into()),
    }
}

#[test]
fn ac_r_2_4_5_1_lift_produces_a_valid_procedure_hir() {
    let lift = lift_skill(&skill_ext("local/demo"), &demo_tree(), 7).unwrap();

    // One Procedure node carrying the `hh/procedure` profile ext.
    let p = &lift.procedure;
    assert_eq!(p.kind, EntityKind::Procedure);
    assert!(p.ext.contains_key(PROCEDURE_PROFILE_EXT_KEY));

    let rec = match &p.semantic {
        KindRecord::Procedure(r) => r,
        other => panic!("expected a Procedure record, got {other:?}"),
    };

    // The body is one Instruction(Text) step; the script adds one Opaque.
    assert_eq!(rec.steps.len(), 2);
    assert!(matches!(rec.steps[0], ProcedureStep::Instruction(_)));
    assert!(matches!(rec.steps[1], ProcedureStep::Opaque(_)));

    // Claims, never grants: `allowed_capabilities` is empty until seal.
    assert!(rec.allowed_capabilities.is_empty());
    assert_eq!(lift.declared_claims.len(), 2 + 1); // 2 allowed-tools + version_label

    // The script's CompiledPayload declares an `exec` interface.
    assert_eq!(lift.payloads.len(), 1);
    let iface = lift.payloads[0].declared_interface.as_ref().unwrap();
    assert!(iface
        .effects
        .iter()
        .any(|e| e.domain == hh_hir::kinds::EffectDomain::Exec));
    assert_eq!(iface.target, "subprocess_confined");

    // references/* land as content-addressed Artifacts produced_by the procedure.
    assert_eq!(lift.artifacts.len(), 1);
    assert_eq!(lift.artifacts[0].kind, EntityKind::Artifact);

    // The profile parses: typed preconditions (env_requires + path_glob)
    // and the source back-reference are present.
    let profile = hh_hir::procedure::ProcedureProfile::from_node(p)
        .unwrap()
        .expect("the lift always writes a profile");
    assert_eq!(profile.source_extension_ref.as_deref(), Some("local/demo"));
    let pre = hh_hir::procedure::parse_preconditions(&rec.preconditions, "pre").unwrap();
    assert_eq!(pre.len(), 2, "path_glob + env_requires");

    // The lifted procedure validates under `hh_hir::validate` inside a doc.
    let mut doc =
        hh_hir::document::HirDocument::new(hh_hir::refs::Ref::selected(p.semantic_id(), "latest"));
    doc.nodes.push(p.clone());
    for a in &lift.artifacts {
        doc.nodes.push(a.clone());
    }
    // A raw lift carries the seal-time gaps as typed errors — `allowed-tools`
    // claims are never grants (the `exec` effect is `EffectUncovered` until
    // an `authorizes` edge lands at seal) and handler completeness is the
    // author's job. Anything else would be a lift bug.
    match hh_hir::validate::validate(&doc) {
        Ok(_) => {}
        Err(es) => {
            for e in &es {
                assert!(
                    matches!(
                        e,
                        HirError::HandlerIncomplete { .. }
                            | HirError::EffectUncovered { .. }
                            | HirError::UnresolvedRef { .. } // doc root is the defn's, not the lift's
                    ),
                    "only seal-time gaps may remain on a raw lift: {e:?}"
                );
            }
            assert!(
                es.iter()
                    .any(|e| matches!(e, HirError::EffectUncovered { .. })),
                "the exec claim must surface as EffectUncovered — claims never grant"
            );
        }
    }

    // The loss report is complete — nothing silently dropped.
    assert!(lift.loss.lost.is_empty());
    assert!(lift.loss.mapped.iter().any(|m| m.contains("allowed-tools")));
}

#[test]
fn ac_r_2_4_5_1_wrong_kind_is_a_typed_refusal() {
    let mut ext = skill_ext("local/not-a-skill");
    ext.kind = ExtensionKind::Hook;
    assert!(matches!(
        lift_skill(&ext, &SkillTree::default(), 1),
        Err(LiftError::WrongKind { .. })
    ));
}

#[test]
fn ac_r_2_4_5_3_render_exec_marker_survives_lift_and_fails_validate() {
    // The lift is faithful (no rewriting); `validate` is the render-purity
    // gate — the marker body fails with `RenderTimeExecution`.
    let mut tree = demo_tree();
    tree.body = "run !`evil` inline".into();
    tree.scripts.clear();
    let lift = lift_skill(&skill_ext("local/evil"), &tree, 7).unwrap();
    let mut doc = hh_hir::document::HirDocument::new(hh_hir::refs::Ref::selected(
        lift.procedure.semantic_id(),
        "latest",
    ));
    doc.nodes.push(lift.procedure);
    let es = validate::validate(&doc).expect_err("the marker body must fail");
    assert!(es
        .iter()
        .any(|e| matches!(e, HirError::RenderTimeExecution { .. })));
}

#[test]
fn ac_r_2_4_5_1_text_authority_caps_at_external() {
    // A trust record minted above external caps at lift — re-endorsement is
    // a `seal`-time act (R-TEXT), never the lift's.
    let mut ext = skill_ext("local/capped");
    ext.trust.text_authority = hh_provenance::AuthorityClass::Principal;
    let lift = lift_skill(&ext, &demo_tree(), 7).unwrap();
    assert!(lift
        .loss
        .notes
        .iter()
        .any(|n| n.contains("capped at external")));
}

use hh_hir::validate;

// ── AC-R-2.4.5-7: export round trip ──────────────────────────────────────────
// `lift(export(P, skill_tree)) ∪ loss_report = P` — the loss report lists at
// least `preconditions`, `expected_evidence`, `tests`, `handlers`, `outputs`
// as `no_slot` (§5c.5 row 7; ADR-0086).

use hh_registry::skill::{export_skill, ExportError};

#[test]
fn ac_r_2_4_5_7_export_lift_round_trip() {
    // Lift a spec-conformant tree, export the procedure back, lift again —
    // the re-lifted body/scripts/name equal the first lift's, and the loss
    // report names every member the skill format cannot carry.
    let first = lift_skill(&skill_ext("local/demo"), &demo_tree(), 7).unwrap();
    let p = &first.procedure;

    // The blob resolver — the caller's content store, keyed by bytes_hash.
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for f in demo_tree().scripts.iter() {
        let addr = hh_identity::idp::address(&f.bytes, f.media_type.clone());
        blobs.insert(addr.id(), f.bytes.clone());
    }
    let resolve = |h: &str| blobs.get(h).cloned();

    let export = export_skill(p, &resolve).expect("a procedure exports");
    // The mandated no_slot rows.
    for member in [
        "preconditions",
        "expected_evidence",
        "tests",
        "failure_handlers",
        "outputs",
    ] {
        assert!(
            export
                .loss
                .lost
                .iter()
                .any(|(f, r)| f == member && r == "no_slot"),
            "loss report must list {member} as no_slot"
        );
    }
    // The carried members map.
    assert_eq!(export.tree.name, p.semantic_id());
    assert_eq!(export.tree.body, demo_tree().body);
    assert_eq!(export.tree.scripts.len(), 1);

    // Re-lift — the recovered members equal the originals.
    let second = lift_skill(&skill_ext("local/demo"), &export.tree, 9).unwrap();
    let KindRecord::Procedure(rec) = &second.procedure.semantic else {
        panic!("lift yields a Procedure");
    };
    let bodies: Vec<&str> = rec
        .steps
        .iter()
        .filter_map(|s| match s {
            ProcedureStep::Instruction(t) => t.content.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(bodies, vec![demo_tree().body.as_str()]);
    assert_eq!(
        rec.steps
            .iter()
            .filter(|s| matches!(s, ProcedureStep::Opaque(_)))
            .count(),
        1,
        "the script round-trips through scripts/*"
    );
}

#[test]
fn ac_r_2_4_5_7_export_declares_losses_never_drops() {
    // A procedure carrying members with no skill slot: Invoke/Branch steps,
    // preconditions — all surface in the loss report.
    let lift = lift_skill(&skill_ext("local/demo"), &demo_tree(), 7).unwrap();
    let mut p = lift.procedure.clone();
    let KindRecord::Procedure(rec) = &mut p.semantic else {
        unreachable!()
    };
    rec.steps.push(ProcedureStep::Invoke {
        tool: hh_hir::refs::Ref::selected("test:tool", "latest"),
        args: Json::Null,
    });
    rec.preconditions = Json::Arr(vec![Json::obj([("kind", Json::str("env_requires"))])]);

    let export = export_skill(&p, &|_| None).unwrap();
    assert!(export
        .loss
        .lost
        .iter()
        .any(|(f, r)| f.starts_with("steps[") && f.contains("invoke") && r == "no_slot"));
    // The Opaque payload's bytes were unresolvable — a `lost` row, never a
    // silent omission.
    assert!(export.loss.lost.iter().any(|(f, _)| f.contains("opaque")));
}

#[test]
fn ac_r_2_4_5_7_export_refuses_non_procedure() {
    let not_proc = hh_hir::document::Node::new(
        EntityKind::Goal,
        KindRecord::Goal(hh_hir::records::GoalRecord {
            statement: hh_hir::leaves::Text::new(
                "g",
                "test",
                hh_provenance::ProvenanceRecord::kernel("test", 0),
            ),
            success_criteria: vec![],
            unverifiable_reason: Some(hh_hir::leaves::Text::new(
                "unverifiable",
                "test",
                hh_provenance::ProvenanceRecord::kernel("test", 0),
            )),
            budget: hh_hir::refs::Ref::selected("test:budget", "latest"),
            origin: hh_hir::records::GoalOrigin::Human,
            parent: None,
        }),
        hh_provenance::ProvenanceRecord::kernel("test", 0),
    );
    assert!(matches!(
        export_skill(&not_proc, &|_| None),
        Err(ExportError::NotAProcedure { .. })
    ));
}
