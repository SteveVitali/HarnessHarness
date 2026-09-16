//! The §3.1.4 validation battery — every invariant lands as a typed `HirError`
//! (AC-IR-01 second half, AC-IR-07; DF-S1.3-1 `MissingProvenance`).

mod common;

use common::*;

use hh_hir::document::{parse_document, HirDocument};
use hh_hir::errors::HirError;
use hh_hir::kinds::{EdgeKind, EntityKind, JudgeProfile, ValidatorKind};
use hh_hir::records::*;
use hh_hir::refs::Ref;
use hh_hir::validate;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope, ProvenanceRecord, TaintTag};
use hh_wire::json::Json;

fn errs(doc: &HirDocument) -> Vec<HirError> {
    validate(doc).expect_err("document must not validate")
}

fn has<F: Fn(&HirError) -> bool>(errs: &[HirError], f: F, what: &str) {
    assert!(errs.iter().any(&f), "expected {what}; got {errs:?}");
}

#[test]
fn the_minimal_document_validates() {
    let report = validate(&valid_doc()).expect("valid doc");
    assert_eq!(report.node_count, 4);
    assert!(report.checks_run.len() >= 10, "check battery is named");
}

#[test]
fn missing_provenance_fails_parse_and_validate() {
    // DF-S1.3-1: `provenance` absent on a node → `MissingProvenance` at parse.
    let doc = valid_doc();
    let bytes = doc.canonical_bytes();
    let mut j = hh_wire::canonical::parse_canonical(&bytes).unwrap();
    if let Json::Obj(m) = &mut j {
        let nodes = m.get_mut("nodes").unwrap();
        if let Json::Arr(ns) = nodes {
            if let Json::Obj(n0) = &mut ns[0] {
                n0.remove("provenance");
            }
        }
    }
    let bytes = j.to_canonical_string().into_bytes();
    match parse_document(&bytes) {
        Err(HirError::MissingProvenance { .. }) => {}
        other => panic!("expected MissingProvenance, got {other:?}"),
    }
}

#[test]
fn noncanonical_input_is_refused() {
    // Whitespace / unsorted keys are not the canonical form.
    let doc = valid_doc();
    let mut bytes = doc.canonical_bytes();
    bytes.insert(1, b' ');
    match parse_document(&bytes) {
        Err(HirError::NonCanonicalInput { .. }) => {}
        other => panic!("expected NonCanonicalInput, got {other:?}"),
    }
}

#[test]
fn unknown_kind_and_dialect_are_refused() {
    let mut doc = valid_doc();
    doc.nodes[0].version.dialect = "HIR/0".into();
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::DialectUnsupported { .. }),
        "DialectUnsupported",
    );

    let mut doc = valid_doc();
    doc.hir_version = "HIR/9".into();
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::DialectUnsupported { .. }),
        "DialectUnsupported",
    );
}

#[test]
fn unresolved_ref_and_wrong_target_kind_fail() {
    let mut doc = valid_doc();
    if let KindRecord::Budget(b) = &mut doc.nodes[1].semantic {
        b.accounting = sel("test:missing");
    }
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::UnresolvedRef { .. }),
        "UnresolvedRef",
    );

    let mut doc = valid_doc();
    if let KindRecord::Budget(b) = &mut doc.nodes[1].semantic {
        // accounting → wrong kind is allowed (allowed=[]), but parent→Goal is not.
        b.parent = Some(sel("test:perm"));
    }
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::UnresolvedRef { detail } if detail.contains("parent")),
        "parent kind mismatch",
    );
}

#[test]
fn depends_on_cycle_is_cycle_detected() {
    let mut doc = valid_doc();
    doc.edges.push(dep_edge("test:budget", "test:rule", 5));
    doc.edges.push(dep_edge("test:rule", "test:budget", 6));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::CycleDetected { .. }),
        "CycleDetected",
    );
}

#[test]
fn conditioned_rule_without_debt_is_incomplete() {
    // AC-IR-07: `conditioned_on` set without a complete `assumption_debt` →
    // `ConditionedRuleIncomplete`.
    let mut doc = valid_doc();
    doc.nodes.push(conditioned_rule_node("test:crule", 7, None));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::ConditionedRuleIncomplete { .. }),
        "ConditionedRuleIncomplete",
    );
    // With the complete record it validates.
    let mut doc = valid_doc();
    doc.nodes.push(conditioned_rule_node(
        "test:crule",
        7,
        Some(debt_record("test:crule.rule", 7)),
    ));
    validate(&doc).expect("conditioned rule with complete debt validates");
}

#[test]
fn unknown_budget_dimension_key_fails() {
    // DF-S1.4-1 (closed at S1.6): `Budget.dimensions` keys resolve against the
    // closed kernel registry — an unknown spelling is SchemaViolation, never
    // silently carried.
    let mut doc = valid_doc();
    doc.nodes.push(budget_node(
        "test:budget-unknown",
        &[("tokens.total", 5000)],
        9,
    ));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { detail } if detail.contains("kernel dimension registry")),
        "unknown dimension key SchemaViolation",
    );
}

#[test]
fn child_budget_exceeding_parent_fails() {
    // AC-IR-07: budget containment — child hard bound above parent's fails.
    let mut doc = valid_doc();
    doc.nodes.push(child_budget_node(
        "test:child",
        "test:budget",
        &[("tokens.blended", 5000)],
        8,
    ));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::BudgetExceedsParent { .. }),
        "BudgetExceedsParent",
    );
}

#[test]
fn delegation_attenuation_is_enforced() {
    // AC-IR-07: a `delegated-to` edge carrying a grant the parent's permission does not
    // cover, or a budget above the parent's, fails.
    let mut doc = valid_doc();
    // Parent agent's permission grants fs_read only, non-delegable matters for Delegate
    // steps — for delegated-to edges the check is coverage.
    doc.nodes.push(perm_node(
        "test:perm2",
        "test:agent",
        vec![grant("fs_read", "/a", true)],
        9,
    ));
    doc.nodes
        .push(agent_node("test:agent2", "test:budget", "test:perm2", 10));
    doc.edges.push(hh_hir::document::Edge::new(
        EdgeKind::DelegatedTo,
        "test:agent",
        "test:agent2",
        EdgeRecord::DelegatedTo {
            permission: sel("test:perm2"),
            budget: sel("test:budget"),
        },
        prov(11),
    ));
    let es = errs(&doc);
    has(
        &es,
        |e| {
            matches!(
                e,
                HirError::AuthorityWidening { .. }
                    | HirError::UnresolvedRef { .. }
                    | HirError::BudgetExceedsParent { .. }
            )
        },
        "delegation attenuation",
    );
}

#[test]
fn uncovered_procedure_effect_is_effect_uncovered() {
    // AC-IR-07: a procedure deriving an effect no `authorizes` edge covers →
    // `EffectUncovered`.
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 9));
    // Give the tool a declared fs_write effect.
    if let KindRecord::ToolCapability(t) = &mut doc.nodes.last_mut().unwrap().semantic {
        let mut set = std::collections::BTreeSet::new();
        set.insert(hh_hir::kinds::EffectClass::domain_only(
            hh_hir::kinds::EffectDomain::FsWrite,
        ));
        t.effects = hh_hir::kinds::ToolEffects::Declared(set);
    }
    let mut steps_proc = node(
        EntityKind::Procedure,
        KindRecord::Procedure(ProcedureRecord {
            preconditions: Json::Null,
            steps: vec![ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::Null,
            }],
            expected_evidence: Json::Null,
            allowed_capabilities: vec![sel("test:tool")],
            failure_handlers: Json::Null,
        }),
        10,
    );
    steps_proc = sid(steps_proc, "test:proc");
    doc.nodes.push(steps_proc);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::EffectUncovered { .. }),
        "EffectUncovered",
    );
}

#[test]
fn authority_above_minted_ceiling_fails() {
    // A human-authored member stamped `kernel` exceeds its minted ceiling.
    let mut doc = valid_doc();
    doc.nodes[0].provenance.authority = AuthorityClass::Kernel;
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::AuthorityExceedsOrigin { .. }),
        "AuthorityExceedsOrigin",
    );
}

#[test]
fn tainted_above_external_fails() {
    let mut doc = valid_doc();
    doc.nodes[0].provenance.taint.insert(TaintTag::Tool {
        capability: "cap:x".into(),
        inner_source: None,
    });
    doc.nodes[0].provenance.authority = AuthorityClass::Principal;
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::TaintedAboveExternal { .. }),
        "TaintedAboveExternal",
    );
}

#[test]
fn text_leaf_above_external_without_pass_through_fails() {
    // R-TEXT: a tool-origin Text leaf may not claim principal authority.
    let mut doc = valid_doc();
    let mut bad = ProvenanceRecord::minted(
        Origin::tool("cap:x", "inv:1"),
        PersistenceScope::Definition,
        20,
    );
    bad.authority = AuthorityClass::Principal; // forged — validate must refuse
    let mut g = goal_node("test:g2", "test:budget", 21);
    if let KindRecord::Goal(gr) = &mut g.semantic {
        let mut t = text("hi", 21);
        t.provenance = bad;
        t.authority = AuthorityClass::Principal;
        gr.statement = t;
    }
    doc.nodes.push(g);
    let es = errs(&doc);
    has(
        &es,
        |e| {
            matches!(
                e,
                HirError::TextAboveExternal { .. } | HirError::AuthorityExceedsOrigin { .. }
            )
        },
        "text authority ceiling",
    );
}

#[test]
fn judge_validator_needs_debt_and_nondeterminism() {
    // §3.1.4: judge ⇒ deterministic=false ∧ profile_ref ∧ assumption_debt.
    let mut doc = valid_doc();
    let v = node(
        EntityKind::Validator,
        KindRecord::Validator(ValidatorRecord {
            kind: ValidatorKind::Judge(Box::new(JudgeProfile {
                rubric: text("rubric", 30),
                profile: hh_hir::refs::ProfileRef {
                    profile: "sha256:profile".into(),
                    pinned: true,
                },
                calibration_ref: None,
                charged_to: hh_hir::kinds::ChargedTo::Subject,
            })),
            inputs: vec![],
            verdict_type: "verdict".into(),
            deterministic: true, // illegal on a judge
            cost: Json::Null,
            evidence_out: Json::Null,
            assumption_debt: None, // illegal on a judge
        }),
        30,
    );
    doc.nodes.push(sid(v, "test:judge"));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::ConditionedRuleIncomplete { .. }),
        "judge debt",
    );
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { detail } if detail.contains("deterministic")),
        "judge determinism",
    );
}

#[test]
fn hosted_body_refuses_none_mechanism_and_bad_version_identity() {
    // CF-351: `none` is native-only.
    let mut doc = hosted_doc();
    if let KindRecord::AgentProcess(a) = &mut doc.nodes[3].semantic {
        if let AgentProcessBody::Hosted(h) = &mut a.body {
            h.hosting_mechanism = hh_ontology::participant::HostingMechanism::None;
        }
    }
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { detail } if detail.contains("CF-351") || detail.contains("none")),
        "hosted none mechanism",
    );

    // A carried version_identity must equal H(declaration ∥ participant_version).
    let mut doc = hosted_doc();
    if let KindRecord::AgentProcess(a) = &mut doc.nodes[3].semantic {
        if let AgentProcessBody::Hosted(h) = &mut a.body {
            h.version_identity = Some("sha256:bogus".into());
        }
    }
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { detail } if detail.contains("version_identity")),
        "version_identity mismatch",
    );
}

#[test]
fn opaque_step_without_interface_is_refused() {
    // O1: a `CompiledPayload` step needs a declared interface.
    let mut doc = valid_doc();
    let payload = hh_hir::leaves::CompiledPayload {
        format_tag: "wasm".into(),
        bytes_hash: "sha256:aa".into(),
        declared_interface: None,
        owner: "test:owner".into(),
        provenance: prov(40),
    };
    let p = node(
        EntityKind::Procedure,
        KindRecord::Procedure(ProcedureRecord {
            preconditions: Json::Null,
            steps: vec![ProcedureStep::Opaque(payload)],
            expected_evidence: Json::Null,
            allowed_capabilities: vec![],
            failure_handlers: Json::Null,
        }),
        40,
    );
    doc.nodes.push(sid(p, "test:proc"));
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::OpaqueWithoutInterface { .. }),
        "OpaqueWithoutInterface",
    );
}

#[test]
fn ext_key_discipline() {
    let mut doc = valid_doc();
    doc.nodes[0].ext.insert("noprefix".into(), Json::Null);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { detail } if detail.contains("prefixed")),
        "unprefixed ext key",
    );
    let mut doc = valid_doc();
    doc.nodes[0].ext.insert("hir/reserved".into(), Json::Null);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { .. }),
        "reserved hir/ ext key",
    );
}

#[test]
fn unclassified_kind_wiring() {
    // The boundary check: ontology refuses an unregistered identifier.
    assert!(hh_ontology::planes::classify_home_identifier("NotAKind").is_err());
    assert!(hh_ontology::planes::classify_home_identifier("Goal").is_ok());
}

#[test]
fn root_must_resolve_to_agent_process() {
    let mut doc = valid_doc();
    doc.root = Ref::selected("test:budget", "latest");
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::UnresolvedRef { detail } if detail.contains("root")),
        "root kind check",
    );
}
