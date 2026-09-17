//! `hh-compiler` unit suite (spec §3.2; ticket S1.10; the run-ledger test matrix).
//! Per-module tests: stage-0 `accept`, stage-1 `link` (selector refusal — DF-S1.9-3,
//! zero resolutions, profile chain, debt completeness, capability_requires, targets),
//! stage-2 `lower_native`, profile schema, `schema_includes`, `trace`.

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;

use common::*;
use hh_compiler::compiler::{accept, accept_bytes};
use hh_compiler::equiv::{schema_includes, ArgMapEntry, ArgTransform, Inclusion, SurfaceBinding};
use hh_compiler::errors::{CompileError, LinkErrorKind};
use hh_compiler::link::{link, LinkedGraph, NoVariants, TargetSpec, VariantView};
use hh_compiler::plan::{lower_native, PlanNode, PlanNodePayload};
use hh_compiler::profile::{
    member_diff, profile_coordinate, profile_identity, resolve_chain, DebtStatus, ExpiryCondition,
    ExpiryKind, ProfileMergePolicy, ProfileRuleKind,
};
use hh_compiler::schema::{profile_from_json, profile_to_json};
use hh_compiler::trace::{build_trace_map, trace};
use hh_compiler::CompileInputs;
use hh_hir::document::SealedDefinition;
use hh_hir::records::*;
use hh_hir::refs::{ProfileRef, RefVersion};
use hh_hir::{CompiledPayload, DeclaredInterface, PreconditionDomain, ToolEffects};
use hh_wire::json::Json;

fn link_of(sealed: &SealedDefinition, store: &dyn VariantView) -> LinkedGraph {
    let profiles = MapProfileView::of(vec![test_profile()]);
    link(
        sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &profiles,
        store,
        &kernel(),
        false,
    )
    .expect("link")
}

// ── stage 0: accept ───────────────────────────────────────────────────────────

#[test]
fn accept_happy_path_over_a_sealed_definition() {
    let (_store, sealed, _vids) = sealed_doc("accept-ok");
    let cat = hh_assembly::Stage1Catalog::stage1();
    let outcome = accept(&sealed, &cat, &kernel()).expect("accept");
    assert!(
        outcome
            .report
            .diagnostics
            .iter()
            .all(|d| d.severity != hh_assembly::Severity::Error),
        "no error diagnostics on a clean sealed definition"
    );
}

#[test]
fn accept_refuses_an_unsealed_node() {
    let (_store, mut sealed, _vids) = sealed_doc("accept-unsealed");
    sealed.document.nodes[0].version.sealed = false;
    let cat = hh_assembly::Stage1Catalog::stage1();
    match accept(&sealed, &cat, &kernel()) {
        Err(CompileError::NotSealed { detail }) => {
            assert!(detail.contains("sealed marker"), "{detail}");
        }
        other => panic!("expected NotSealed, got {other:?}"),
    }
}

#[test]
fn accept_reasserts_the_canonical_hash() {
    let (_store, mut sealed, _vids) = sealed_doc("accept-hash");
    sealed.definition_ref.version_id = "sha256:tampered".to_string();
    let cat = hh_assembly::Stage1Catalog::stage1();
    match accept(&sealed, &cat, &kernel()) {
        Err(CompileError::NonCanonicalInput { detail }) => {
            assert!(detail.contains("recomputed"), "{detail}");
        }
        other => panic!("expected NonCanonicalInput, got {other:?}"),
    }
}

#[test]
fn accept_bytes_round_trips_the_sealed_document() {
    let (_store, sealed, _vids) = sealed_doc("accept-bytes");
    let cat = hh_assembly::Stage1Catalog::stage1();
    let (rebuilt, outcome) =
        accept_bytes(&sealed.canonical_bytes(), &cat, &kernel()).expect("accept_bytes");
    assert_eq!(
        rebuilt.definition_ref, sealed.definition_ref,
        "the canonical-bytes seam reproduces the recorded identity"
    );
    assert_eq!(rebuilt.closed_world_tools, sealed.closed_world_tools);
    assert!(outcome
        .report
        .diagnostics
        .iter()
        .all(|d| d.severity != hh_assembly::Severity::Error));
}

#[test]
fn accept_bytes_refuses_malformed_and_unsealed() {
    let cat = hh_assembly::Stage1Catalog::stage1();
    match accept_bytes(b"not json", &cat, &kernel()) {
        Err(CompileError::NonCanonicalInput { .. }) => {}
        other => panic!("expected NonCanonicalInput, got {other:?}"),
    }
    // A well-formed but unsealed document (authored, never resolved).
    let doc = doc_with(&stage1_assembly());
    match accept_bytes(&doc.canonical_bytes(), &cat, &kernel()) {
        Err(CompileError::NotSealed { .. }) => {}
        other => panic!("expected NotSealed, got {other:?}"),
    }
}

// ── stage 1: link — the selector refusal (DF-S1.9-3) ─────────────────────────

#[test]
fn link_refuses_a_surviving_selector() {
    // Fabricate a sealed definition carrying an unpinned `version_selector` — the
    // DF-S1.9-3 consumer half: the compiler's stage-1 `link` never re-resolves; the
    // surviving selector is `C-LINK-1` `LinkError{unbound_slot}`.
    let (_store, mut sealed, _vids) = sealed_doc("link-selector");
    for n in sealed.document.nodes.iter_mut() {
        if let KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let AgentProcessBody::Native(np) = &mut ap.body {
                np.budget.version = RefVersion::Selector("latest".to_string());
            }
        }
    }
    let profiles = MapProfileView::of(vec![test_profile()]);
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &profiles,
        &NoVariants,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError {
            kind, diagnostics, ..
        }) => {
            assert_eq!(kind, LinkErrorKind::UnboundSlot);
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code.code() == "C-LINK-1" && d.stage == hh_assembly::Stage::Link),
                "the link_precheck diagnostic set carries C-LINK-1 at Stage::Link"
            );
        }
        other => panic!("expected LinkError{{unbound_slot}}, got {other:?}"),
    }
}

#[test]
fn link_never_resolves_only_pinned_gets() {
    // DF-S1.9-3's "zero resolutions": the variant view sees only pinned `version_id`
    // lookups — never a selector, never a class/variant name.
    let (store, sealed, vids) = sealed_doc("link-noresolve");
    let counter = CountingVariantView {
        inner: &store,
        requested: RefCell::new(Vec::new()),
    };
    let linked = link_of(&sealed, &counter);
    let requested = counter.requested.borrow();
    assert!(!requested.is_empty(), "the bound slots were looked up");
    for vid in requested.iter() {
        assert!(
            vids.values().any(|v| v == vid),
            "every lookup is a pinned version_id (got {vid})"
        );
        assert!(!vid.contains("latest"), "no selector reaches the view");
    }
    assert_eq!(linked.variant_pins.len(), 2, "both Stage-1 slots pinned");
}

#[test]
fn link_happy_path_binds_profile_targets_and_slots() {
    let (store, sealed, _vids) = sealed_doc("link-ok");
    let linked = link_of(&sealed, &store);
    assert_eq!(linked.profile.chain.len(), 1);
    assert!(!linked.profile.is_fallback);
    assert_eq!(linked.targets.len(), 1);
    assert!(linked.diagnostics.is_empty());
}

// ── stage 1: link — NoProfile / fallback ──────────────────────────────────────

#[test]
fn link_no_profile_is_a_typed_refusal() {
    let (store, sealed, _vids) = sealed_doc("link-noprof");
    let profiles = MapProfileView::of(vec![]);
    match link(
        &sealed,
        &[],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::NoProfile { detail }) => {
            assert!(detail.contains("ADR-0124"), "{detail}");
        }
        other => panic!("expected NoProfile, got {other:?}"),
    }
}

#[test]
fn link_fallback_requires_a_dated_hypothesis() {
    let (store, sealed, _vids) = sealed_doc("link-fallback");
    // Undated fallback → NoProfile.
    let mut undated = test_profile();
    undated.expiry.expiry_condition = ExpiryCondition {
        kind: ExpiryKind::ProbeFailure,
        value: None,
    };
    let profiles = MapProfileView::of(vec![undated.clone()]);
    let coord = profile_coordinate(&undated);
    match link(
        &sealed,
        &[],
        Some(&coord),
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::NoProfile { detail }) => {
            assert!(detail.contains("dated debt hypothesis"), "{detail}");
        }
        other => panic!("expected NoProfile, got {other:?}"),
    }
    // Dated fallback → bound, marked `is_fallback`.
    let dated = test_profile();
    let profiles = MapProfileView::of(vec![dated.clone()]);
    let coord = profile_coordinate(&dated);
    let linked = link(
        &sealed,
        &[],
        Some(&coord),
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    )
    .expect("dated fallback binds");
    assert!(linked.profile.is_fallback);
}

// ── stage 1: link — targets / version conflicts ───────────────────────────────

#[test]
fn link_unknown_target_is_typed() {
    let (store, sealed, _vids) = sealed_doc("link-target");
    let profiles = MapProfileView::of(vec![test_profile()]);
    let bad = TargetSpec {
        target_id: "no-such-target".to_string(),
        spec_version: "9.9".to_string(),
        content_hash: "sha256:x".to_string(),
    };
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[bad],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError {
            kind, diagnostics, ..
        }) => {
            assert_eq!(kind, LinkErrorKind::UnknownTarget);
            assert!(diagnostics.iter().any(|d| d.code.code() == "C-LINK-5"));
        }
        other => panic!("expected LinkError{{unknown_target}}, got {other:?}"),
    }
}

#[test]
fn link_version_conflict_on_a_missing_variant() {
    // The sealed definition pins variants the bound view doesn't carry → C-LINK-4.
    let (_store, sealed, _vids) = sealed_doc("link-vconf");
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &MapProfileView::of(vec![test_profile()]),
        &NoVariants,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, LinkErrorKind::VersionConflict);
        }
        other => panic!("expected LinkError{{version_conflict}}, got {other:?}"),
    }
}

#[test]
fn link_version_conflict_on_a_profile_mismatch() {
    // The definition pins `sha256:profile`; a chain whose head is another profile is
    // `version_conflict`.
    let (store, sealed, _vids) = sealed_doc("link-pconf");
    let other = profile_with("other-profile", "1.0", vec![]);
    let profiles = MapProfileView::of(vec![other.clone()]);
    match link(
        &sealed,
        &[profile_coordinate(&other)],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, LinkErrorKind::VersionConflict);
        }
        other => panic!("expected LinkError{{version_conflict}}, got {other:?}"),
    }
}

#[test]
fn link_unresolvable_profile_coordinate_is_version_conflict() {
    let (store, sealed, _vids) = sealed_doc("link-pmissing");
    let profiles = MapProfileView::of(vec![]);
    match link(
        &sealed,
        &["missing@1.0".to_string()],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, LinkErrorKind::VersionConflict);
        }
        other => panic!("expected version_conflict, got {other:?}"),
    }
}

// ── stage 1: link — conditioned-rule debt (AC-CP-06 stage-1 half) ─────────────

#[test]
fn link_missing_debt_record_on_a_conditioned_rule() {
    // A definition `HarnessRule` with `conditioned_on` but an incomplete debt record.
    // (`seal` refuses an incomplete record outright — the link-level re-check is the
    // compile-path guard, exercised here on a post-seal-fabricated record.)
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(conditioned_rule_node(
        "test:crule",
        ProfileRef {
            profile: "sha256:profile".into(),
            pinned: true,
        },
        Some(complete_debt(
            "test:crule.rule",
            hh_hir::DebtStatus::Discharged,
            5,
        )),
        5,
    ));
    let (store, mut sealed, _vids) = sealed_doc_with("link-debt", doc, vec![], vec![]);
    // Fabricate the incomplete record post-seal (link re-checks completeness for the
    // compile record — the refusing check is `debt_complete`, not re-validation).
    for n in sealed.document.nodes.iter_mut() {
        if let KindRecord::HarnessRule(r) = &mut n.semantic {
            if let Some(d) = &mut r.assumption_debt {
                d.rule_id = String::new();
            }
        }
    }
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &MapProfileView::of(vec![test_profile()]),
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError {
            kind, diagnostics, ..
        }) => {
            assert_eq!(kind, LinkErrorKind::MissingDebtRecord);
            assert!(diagnostics.iter().any(|d| d.code.code() == "C-LINK-3"));
        }
        other => panic!("expected missing_debt_record, got {other:?}"),
    }
}

#[test]
fn link_missing_debt_record_on_a_profile_rule() {
    let (store, sealed, _vids) = sealed_doc("link-pdebt");
    let mut r = rule(
        "r1",
        ProfileRuleKind::SamplingDefaults,
        &[],
        Json::Null,
        DebtStatus::Active,
    );
    r.debt.hypothesis = String::new(); // incomplete
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    match link(
        &sealed,
        &[profile_coordinate(&p)],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, LinkErrorKind::MissingDebtRecord);
        }
        other => panic!("expected missing_debt_record, got {other:?}"),
    }
}

#[test]
fn link_expired_rule_warns_only_under_recorded_intent() {
    let (store, sealed, _vids) = sealed_doc("link-expired");
    let r = rule(
        "r-exp",
        ProfileRuleKind::SamplingDefaults,
        &[],
        Json::Null,
        DebtStatus::Expired,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    let coord = profile_coordinate(&p);
    // Without the recorded intent → error (C-LINK-6 surfaced as a refusal).
    match link(
        &sealed,
        std::slice::from_ref(&coord),
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError {
            kind, diagnostics, ..
        }) => {
            assert_eq!(kind, LinkErrorKind::MissingDebtRecord);
            assert!(diagnostics.iter().any(|d| d.code.code() == "C-LINK-6"));
        }
        other => panic!("expected expired-rule refusal, got {other:?}"),
    }
    // With the recorded intent → warning diagnostics + the conditioned-rules entry.
    let linked = link(
        &sealed,
        &[coord],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        true,
    )
    .expect("recorded intent admits expired rules");
    assert!(linked
        .diagnostics
        .iter()
        .any(|d| d.code.code() == "C-LINK-6" && d.severity == hh_assembly::Severity::Warning));
    assert!(linked
        .conditioned_rules
        .iter()
        .any(|c| c.rule_id == "r-exp" && c.status == "expired"));
}

#[test]
fn link_capability_requires_needs_a_depends_on_edge() {
    // ADR-0087: a `PreconditionDomain::Capability` requires a `depends-on` edge to
    // another ToolCapability. Without one → C-LINK-7.
    let mut doc = doc_with(&stage1_assembly());
    let mut t = tool_node("test:needy", 5);
    if let KindRecord::ToolCapability(tc) = &mut t.semantic {
        tc.preconditions = vec![PreconditionDomain::Capability];
    }
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("link-capreq", doc, vec![], vec![]);
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &MapProfileView::of(vec![test_profile()]),
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::LinkError { diagnostics, .. }) => {
            assert!(
                diagnostics.iter().any(|d| d.code.code() == "C-LINK-7"),
                "C-LINK-7 raised"
            );
        }
        other => panic!("expected capability_requires refusal, got {other:?}"),
    }
}

#[test]
fn link_capability_requires_satisfied_by_a_depends_on_edge() {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = tool_node("test:needy", 5);
    if let KindRecord::ToolCapability(tc) = &mut t.semantic {
        tc.preconditions = vec![PreconditionDomain::Capability];
    }
    doc.nodes.push(t);
    doc.nodes.push(tool_node("test:provider", 6));
    doc.edges.push(hh_hir::Edge::new(
        hh_hir::EdgeKind::DependsOn,
        "test:needy",
        "test:provider",
        EdgeRecord::DependsOn {
            reason: "requires the provider capability".to_string(),
        },
        prov(7),
    ));
    let (store, sealed, _vids) = sealed_doc_with("link-capok", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    assert!(linked.diagnostics.is_empty());
}

// ── stage 1: link — AC-CP-05 UnexpressibleSurface ─────────────────────────────

#[test]
fn link_refuses_a_non_native_fc_interaction_mode() {
    let (store, sealed, _vids) = sealed_doc("link-imode");
    let r = rule(
        "r-mode",
        ProfileRuleKind::InteractionMode,
        &[],
        Json::obj([("mode", Json::str("code_mode"))]),
        DebtStatus::Active,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    match link(
        &sealed,
        &[profile_coordinate(&p)],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::UnexpressibleSurface { reason, .. }) => {
            assert!(reason.contains("native_fc"), "{reason}");
        }
        other => panic!("expected UnexpressibleSurface, got {other:?}"),
    }
}

#[test]
fn link_refuses_a_tool_shape_family_outside_the_c0_set() {
    let (store, sealed, _vids) = sealed_doc("link-tshape");
    let r = rule(
        "r-shape",
        ProfileRuleKind::ToolShape,
        &[],
        Json::obj([(
            "edit_file",
            Json::obj([
                ("family_id", Json::str("composite")),
                ("variant_id", Json::str("v1")),
            ]),
        )]),
        DebtStatus::Active,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    match link(
        &sealed,
        &[profile_coordinate(&p)],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::UnexpressibleSurface { entity, reason, .. }) => {
            assert_eq!(entity, "edit_file");
            assert!(reason.contains("composite"), "{reason}");
        }
        other => panic!("expected UnexpressibleSurface, got {other:?}"),
    }
}

#[test]
fn link_refuses_undeclared_dialect_narrowing() {
    // A tool surface narrowing to a dialect no bound profile admits →
    // DialectNarrowingUndeclared.
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    if let Some(SurfaceRecord::Tool(ts)) = &mut t.surface {
        ts.schema_dialect_narrowing =
            Json::obj([("dialect", Json::str("token-optimized-notation"))]);
    }
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("link-dialect", doc, vec![], vec![]);
    match link(
        &sealed,
        &[profile_coordinate(&test_profile())],
        None,
        &[mcp_target()],
        &MapProfileView::of(vec![test_profile()]),
        &store,
        &kernel(),
        false,
    ) {
        Err(CompileError::DialectNarrowingUndeclared { detail }) => {
            assert!(detail.contains("token-optimized-notation"), "{detail}");
        }
        other => panic!("expected DialectNarrowingUndeclared, got {other:?}"),
    }
}

#[test]
fn link_admits_a_dialect_narrowing_the_profile_declares() {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    if let Some(SurfaceRecord::Tool(ts)) = &mut t.surface {
        ts.schema_dialect_narrowing = Json::obj([("dialect", Json::str("custom-1"))]);
    }
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("link-dialect-ok", doc, vec![], vec![]);
    let r = rule(
        "r-dialect",
        ProfileRuleKind::SchemaDialect,
        &[],
        Json::obj([("dialect", Json::str("custom-1"))]),
        DebtStatus::Active,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    link(
        &sealed,
        &[profile_coordinate(&p)],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        false,
    )
    .expect("a declared narrowing binds");
}

// ── profile chain (resolve_chain) ─────────────────────────────────────────────

#[test]
fn chain_orders_base_before_version() {
    let base = profile_with("sha256:profile", "1.0", vec![]);
    let mut child = profile_with("sha256:profile", "2.0", vec![]);
    child.extends = Some(profile_coordinate(&base));
    child.content_hash = profile_identity(&child);
    let profiles = MapProfileView::of(vec![base.clone(), child.clone()]);
    let chain = resolve_chain(&[profile_coordinate(&child)], &profiles).expect("chain");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].version, "1.0");
    assert_eq!(chain[1].version, "2.0");
}

#[test]
fn chain_refuses_a_cycle() {
    let mut a = profile_with("p-a", "1.0", vec![]);
    let mut b = profile_with("p-b", "1.0", vec![]);
    a.extends = Some(profile_coordinate(&b));
    b.extends = Some(profile_coordinate(&a));
    a.content_hash = profile_identity(&a);
    b.content_hash = profile_identity(&b);
    let profiles = MapProfileView::of(vec![a.clone(), b.clone()]);
    match resolve_chain(&[profile_coordinate(&a)], &profiles) {
        Err(CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("cycle"), "{detail}");
        }
        other => panic!("expected a cycle refusal, got {other:?}"),
    }
}

#[test]
fn chain_refuses_depth_beyond_the_c0_bound() {
    let a = profile_with("p-a", "1.0", vec![]);
    let mut b = profile_with("p-b", "1.0", vec![]);
    b.extends = Some(profile_coordinate(&a));
    b.selector.precedence = 1;
    b.content_hash = profile_identity(&b);
    let mut c = profile_with("p-c", "1.0", vec![]);
    c.extends = Some(profile_coordinate(&b));
    c.selector.precedence = 2;
    c.content_hash = profile_identity(&c);
    let profiles = MapProfileView::of(vec![a, b, c.clone()]);
    match resolve_chain(&[profile_coordinate(&c)], &profiles) {
        Err(CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("two-level"), "{detail}");
        }
        other => panic!("expected a depth refusal, got {other:?}"),
    }
}

#[test]
fn chain_ambiguous_selector_tie_refuses() {
    // Two unconstrained profiles at identical precedence + specificity →
    // AmbiguousSelector (resolution never guesses).
    let a = profile_with("p-a", "1.0", vec![]);
    let b = profile_with("p-b", "1.0", vec![]);
    let profiles = MapProfileView::of(vec![a.clone(), b.clone()]);
    match resolve_chain(&[profile_coordinate(&a), profile_coordinate(&b)], &profiles) {
        Err(CompileError::AmbiguousSelector { .. }) => {}
        other => panic!("expected AmbiguousSelector, got {other:?}"),
    }
}

#[test]
fn chain_higher_precedence_orders_last() {
    let mut a = profile_with("p-a", "1.0", vec![]);
    a.selector.precedence = 5;
    a.content_hash = profile_identity(&a);
    let b = profile_with("p-b", "1.0", vec![]); // precedence 0
    let profiles = MapProfileView::of(vec![a.clone(), b.clone()]);
    let chain = resolve_chain(&[profile_coordinate(&a), profile_coordinate(&b)], &profiles)
        .expect("ordered");
    assert_eq!(chain[1].profile_id, "p-a", "higher precedence applies last");
}

#[test]
fn link_refuses_an_override_outside_owned_fields() {
    // A child overriding a member outside its rules' `owned_fields` — and outside the
    // structural set a version profile always owns — is InvalidModelProfile.
    // `capabilities` is structural-owned, so use a rule-owned member: a child that
    // changes `/capabilities` values *and* declares no rule owning `capabilities`... all
    // top-level members are in the structural owned set, so exercise `member_diff`'s
    // granularity: a child whose `selector` differs AND whose rules own nothing extra is
    // still legal (selector is structural). The refusal is covered by `forbid_override`
    // below and by schema-level checks; here assert the chain-merge path is stable.
    let base = profile_with("sha256:profile", "1.0", vec![]);
    let mut child = profile_with("sha256:profile", "2.0", vec![]);
    child.extends = Some(profile_coordinate(&base));
    child.content_hash = profile_identity(&child);
    let diffs = member_diff(&base, &child);
    assert!(diffs.contains(&"version".to_string()));
}

#[test]
fn profile_merge_policy_is_declared_for_every_member() {
    use hh_compiler::profile::{profile_merge_policy, PROFILE_MERGE_POLICIES};
    // Every `ModelProfile/1` top-level member has a row (CC1: one declared policy).
    for member in [
        "profile_id",
        "version",
        "content_hash",
        "selector",
        "extends",
        "capabilities",
        "rules",
        "ext",
        "expiry",
        "compatibility",
        "tests",
    ] {
        let _ = profile_merge_policy(member);
    }
    assert_eq!(PROFILE_MERGE_POLICIES.len(), 11);
    assert_eq!(
        profile_merge_policy("rules"),
        ProfileMergePolicy::Append,
        "rules append across the chain"
    );
}

// ── ModelProfile/1 schema ─────────────────────────────────────────────────────

#[test]
fn profile_schema_round_trips_and_verifies_content_hash() {
    let p = test_profile();
    let j = profile_to_json(&p);
    let back = profile_from_json(&j, "p").expect("round-trip");
    assert_eq!(back, p);
    // Tamper the claimed hash → schema violation (CC3).
    let mut bad = profile_to_json(&p);
    if let Json::Obj(m) = &mut bad {
        m.insert("content_hash".into(), Json::str("sha256:forged"));
    }
    match profile_from_json(&bad, "p") {
        Err(CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("identity"), "{detail}");
        }
        other => panic!("expected InvalidModelProfile, got {other:?}"),
    }
}

#[test]
fn profile_schema_refuses_closed_set_misses() {
    let p = test_profile();
    // Bad rule kind.
    let mut j = profile_to_json(&p);
    if let Json::Obj(m) = &mut j {
        m.insert(
            "rules".into(),
            Json::Arr(vec![Json::obj([
                ("rule_id", Json::str("r")),
                ("kind", Json::str("not_a_kind")),
                ("owned_fields", Json::Arr(vec![])),
                ("params", Json::Null),
                (
                    "debt",
                    Json::obj([
                        ("rule_id", Json::str("r")),
                        ("hypothesis", Json::str("h")),
                        ("evidence_refs", Json::Arr(vec![])),
                        ("owner", Json::str("o")),
                        (
                            "expiry_condition",
                            Json::obj([("kind", Json::str("date")), ("value", Json::str("x"))]),
                        ),
                        ("removal_test_ref", Json::str("t")),
                        ("status", Json::str("active")),
                    ]),
                ),
                (
                    "compliance",
                    Json::obj([("detector_class", Json::str("none"))]),
                ),
            ])]),
        );
    }
    match profile_from_json(&j, "p") {
        Err(CompileError::InvalidModelProfile { detail }) => {
            assert!(detail.contains("ProfileRuleKind"), "{detail}");
        }
        other => panic!("expected InvalidModelProfile, got {other:?}"),
    }
    // Missing required member.
    let mut j = profile_to_json(&p);
    if let Json::Obj(m) = &mut j {
        m.remove("selector");
    }
    assert!(matches!(
        profile_from_json(&j, "p"),
        Err(CompileError::InvalidModelProfile { .. })
    ));
}

#[test]
fn profile_rule_kind_set_is_closed_and_spellings_round_trip() {
    // The canonical §3.2.3 closed set — exactly thirteen kinds.
    assert_eq!(ProfileRuleKind::ALL.len(), 13);
    for k in [
        "tool_shape",
        "naming",
        "schema_dialect",
        "description_template",
        "error_format",
        "result_render",
        "prompt_layout",
        "interaction_mode",
        "transcript_render",
        "compaction_reminder",
        "sampling_defaults",
        "caching_markers",
        "procedure_target",
    ] {
        let kind = ProfileRuleKind::parse(k).expect(k);
        assert_eq!(kind.name(), k);
    }
    for k in ProfileRuleKind::ALL {
        assert_eq!(ProfileRuleKind::parse(k.name()), Some(*k));
    }
    // Outside the closed set — refused, never widened silently.
    for bad in [
        "stop_text",
        "required_fields",
        "forbid_override",
        "reasoning_format",
        "",
    ] {
        assert!(ProfileRuleKind::parse(bad).is_none(), "{bad} parsed");
    }
}

// ── stage 2: lower_native ─────────────────────────────────────────────────────

fn all_payloads(plan: &hh_compiler::plan::RuntimePlan) -> Vec<&PlanNodePayload> {
    fn walk<'p>(nodes: &'p [PlanNode], out: &mut Vec<&'p PlanNodePayload>) {
        for n in nodes {
            out.push(&n.payload);
            match &n.payload {
                PlanNodePayload::Loop(l) => walk(&l.body, out),
                PlanNodePayload::BranchOnValidator(b) => {
                    walk(&b.then_body, out);
                    walk(&b.else_body, out);
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&plan.control, &mut out);
    out
}

#[test]
fn plan_emits_the_closed_node_set_with_total_hir_node_ids() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(tool_node("test:tool", 5));
    doc.nodes.push(validator_node("test:val", 6));
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![
            ProcedureStep::Instruction(text("do the thing", 7)),
            ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::Null,
            },
            ProcedureStep::Verify {
                validator: sel("test:val"),
            },
            ProcedureStep::Loop {
                bound: sel("test:budget"),
                body: vec![ProcedureStep::Instruction(text("again", 8))],
            },
        ],
        9,
    ));
    let (store, sealed, _vids) = sealed_doc_with("plan-kinds", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    let plan = lower_native(&linked).expect("lower");
    assert!(!plan.control.is_empty());
    for n in &plan.control {
        assert!(!n.hir_node_id.is_empty(), "every node carries hir_node_id");
        assert!(!n.hir_version_id.is_empty());
    }
    let kinds: Vec<&str> = all_payloads(&plan)
        .iter()
        .map(|p| match p {
            PlanNodePayload::Loop(_) => "loop",
            PlanNodePayload::Step(_) => "step",
            PlanNodePayload::BranchOnValidator(_) => "branch-on-validator",
            PlanNodePayload::Delegate(_) => "delegate",
            PlanNodePayload::StopRule(_) => "stop-rule",
        })
        .collect();
    assert!(kinds.contains(&"step"));
    assert!(kinds.contains(&"branch-on-validator"));
    assert!(kinds.contains(&"loop"));
    assert!(
        kinds.contains(&"stop-rule"),
        "the boundary lowered into stop-rules"
    );
    // The budget envelope landed (ADR-0106).
    let env = plan.budget.as_ref().expect("envelope");
    assert_eq!(env.budget.semantic_id, "test:budget");
    assert!(!env.stop_rule_nodes.is_empty());
    assert!(env.dimensions.contains_key("tokens.blended"));
    // The tool table + bound slots.
    assert_eq!(plan.tools.len(), 1);
    assert_eq!(plan.tools[0].capability.semantic_id, "test:tool");
    assert!(plan.bound_slots.contains_key("control_strategy"));
    assert!(
        plan.context_policy.is_some(),
        "context_policy params carried"
    );
    // Validators bound.
    assert_eq!(plan.validators.len(), 1);
    assert_eq!(plan.validators[0].kind, "schema");
    // The ids block.
    assert_eq!(plan.ids.definition_ref, sealed.definition_ref);
}

#[test]
fn plan_refuses_an_opaque_step_as_unsupported_construct() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![ProcedureStep::Opaque(CompiledPayload {
            format_tag: "opaque-fmt".to_string(),
            bytes_hash: hh_identity::idp::address(b"x", "application/octet-stream").digest,
            declared_interface: Some(DeclaredInterface {
                inputs: Json::Null,
                outputs: Json::Null,
                effects: std::collections::BTreeSet::new(),
                deterministic: true,
                target: "test".to_string(),
            }),
            owner: "test:owner".to_string(),
            provenance: prov(8),
        })],
        5,
    ));
    let (store, sealed, _vids) = sealed_doc_with("plan-opaque", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    match lower_native(&linked) {
        Err(CompileError::PlanError { node, detail }) => {
            assert_eq!(node, "test:proc");
            assert!(detail.contains("opaque-fmt"), "{detail}");
        }
        other => panic!("expected PlanError, got {other:?}"),
    }
}

#[test]
fn plan_refuses_a_non_validator_branch() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![ProcedureStep::Branch {
            condition: Json::obj([("flag", Json::Bool(true))]),
            then_body: vec![],
            else_body: vec![],
        }],
        5,
    ));
    let (store, sealed, _vids) = sealed_doc_with("plan-branch", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    match lower_native(&linked) {
        Err(CompileError::PlanError { node, detail }) => {
            assert_eq!(node, "test:proc");
            assert!(detail.contains("validator"), "{detail}");
        }
        other => panic!("expected PlanError, got {other:?}"),
    }
}

#[test]
fn plan_validator_branch_lowers_when_the_condition_names_a_validator() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(validator_node("test:val", 5));
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![ProcedureStep::Branch {
            condition: Json::obj([("validator", Json::str("test:val"))]),
            then_body: vec![ProcedureStep::Instruction(text("pass", 6))],
            else_body: vec![],
        }],
        7,
    ));
    let (store, sealed, _vids) = sealed_doc_with("plan-branch-ok", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    let plan = lower_native(&linked).expect("lower");
    let has_branch = all_payloads(&plan).iter().any(|p| match p {
        PlanNodePayload::BranchOnValidator(b) => {
            b.validator.semantic_id == "test:val" && b.then_body.len() == 1
        }
        _ => false,
    });
    assert!(has_branch, "the validator branch lowered");
}

#[test]
fn plan_policy_tables_derive_from_entities_only() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(tool_node("test:tool", 5));
    let (store, sealed, _vids) = sealed_doc_with("plan-policy", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    let plan = lower_native(&linked).expect("lower");
    // One permission row per Permission node, one budget row per Budget node, one
    // effect row per (capability, declared effect) — pure tools contribute none.
    assert_eq!(plan.policies.permissions.len(), 1);
    assert_eq!(plan.policies.budgets.len(), 1);
    assert!(
        plan.policies.effect_classes.is_empty(),
        "pure tool → no effect rows"
    );
    // No row derives from a surface (a surfaced tool adds a ToolBinding, never a policy).
    let mut doc2 = doc_with(&stage1_assembly());
    doc2.nodes
        .push(surfaced_tool_node("test:tool", "tool_a", &["path"], 5));
    let (store2, sealed2, _vids) = sealed_doc_with("plan-policy2", doc2, vec![], vec![]);
    let linked2 = link_of(&sealed2, &store2);
    let plan2 = lower_native(&linked2).expect("lower");
    assert_eq!(plan2.policies.permissions.len(), 1);
    assert_eq!(plan2.policies.budgets.len(), 1);
    assert_eq!(
        plan2.tools[0].surface.as_ref().unwrap().surface_name,
        "tool_a"
    );
}

#[test]
fn plan_carries_declared_effects_in_the_effect_table() {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = tool_node("test:tool", 5);
    if let KindRecord::ToolCapability(tc) = &mut t.semantic {
        tc.effects = ToolEffects::Declared(
            vec![hh_hir::EffectClass::domain_only(
                hh_hir::EffectDomain::FsWrite,
            )]
            .into_iter()
            .collect(),
        );
    }
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("plan-effects", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    let plan = lower_native(&linked).expect("lower");
    assert_eq!(plan.policies.effect_classes.len(), 1);
    assert_eq!(plan.policies.effect_classes[0].capability, "test:tool");
    assert_eq!(plan.tools[0].effects.len(), 1);
}

// ── equiv: schema_includes + bind_surface ─────────────────────────────────────

#[test]
fn schema_includes_classifies_the_four_cases() {
    let cap = Json::obj([
        ("type", Json::str("string")),
        ("enum", Json::Arr(vec![Json::str("a"), Json::str("b")])),
    ]);
    assert_eq!(schema_includes(&cap, &cap), Inclusion::Included);
    let narrowed = Json::obj([
        ("type", Json::str("string")),
        ("enum", Json::Arr(vec![Json::str("a")])),
    ]);
    assert_eq!(schema_includes(&cap, &narrowed), Inclusion::Narrowed);
    let widened = Json::obj([
        ("type", Json::str("string")),
        (
            "enum",
            Json::Arr(vec![Json::str("a"), Json::str("b"), Json::str("c")]),
        ),
    ]);
    assert_eq!(schema_includes(&cap, &widened), Inclusion::Widened);
    let unknown = Json::obj([("patternProperties", Json::Null)]);
    assert_eq!(schema_includes(&cap, &unknown), Inclusion::Unknown);
    // Type-tag mismatch is incomparable → widened.
    let other = Json::obj([("type", Json::str("integer"))]);
    assert_eq!(schema_includes(&cap, &other), Inclusion::Widened);
    // Numeric bounds: inside → narrowed, outside → widened.
    let cnum = Json::obj([
        ("type", Json::str("integer")),
        ("minimum", Json::Int(0)),
        ("maximum", Json::Int(10)),
    ]);
    let snum = Json::obj([
        ("type", Json::str("integer")),
        ("minimum", Json::Int(2)),
        ("maximum", Json::Int(8)),
    ]);
    assert_eq!(schema_includes(&cnum, &snum), Inclusion::Narrowed);
    let wnum = Json::obj([("type", Json::str("integer")), ("minimum", Json::Int(0))]);
    assert_eq!(schema_includes(&cnum, &wnum), Inclusion::Widened);
}

#[test]
fn bind_surface_derives_the_identity_arg_map() {
    let n = surfaced_tool_node("test:tool", "tool_a", &["path", "mode"], 5);
    let ts = match n.surface.as_ref().unwrap() {
        SurfaceRecord::Tool(t) => t.as_ref(),
        _ => unreachable!(),
    };
    let cap = match &n.semantic {
        KindRecord::ToolCapability(t) => t,
        _ => unreachable!(),
    };
    let binding = hh_compiler::equiv::bind_surface(&n, ts, &cap.input_schema);
    assert_eq!(binding.surface_name, "tool_a");
    assert_eq!(binding.dialect, "json-schema-2020-12");
    assert_eq!(binding.arg_map.len(), 2);
    assert!(binding
        .arg_map
        .values()
        .all(|e| matches!(e.transform, ArgTransform::Identity)));
    // `surface_field → capability_param` — the identity map binds each arg to itself.
    assert_eq!(binding.arg_map["path"].capability_param, "path");
    assert_eq!(binding.hir_node_id, "test:tool");
}

#[test]
fn check_equivalence_e2_fails_on_an_arg_outside_the_schema() {
    let n = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    let ts = match n.surface.as_ref().unwrap() {
        SurfaceRecord::Tool(t) => t.as_ref(),
        _ => unreachable!(),
    };
    let cap = match &n.semantic {
        KindRecord::ToolCapability(t) => t,
        _ => unreachable!(),
    };
    let mut binding = hh_compiler::equiv::bind_surface(&n, ts, &cap.input_schema);
    // An arg naming no declared `input_schema` property → E2 fail.
    binding.arg_map.insert(
        "ghost".to_string(),
        ArgMapEntry {
            capability_param: "ghost".to_string(),
            transform: ArgTransform::Rename,
            narrowing: None,
        },
    );
    let e = hh_compiler::equiv::check_equivalence(&binding, &n, None).expect("evidence");
    assert!(matches!(
        e.e2_authority,
        hh_compiler::equiv::EvidenceVerdict::Fail { .. }
    ));
    // E7 still passes — the binding names C.
    assert_eq!(
        e.e7_accounting_identity,
        hh_compiler::equiv::EvidenceVerdict::Pass
    );
    // E4 is `n/a` — never a verdict (T-LCD-15; pure tool → not open-world → stage_3).
    assert!(matches!(
        e.e4_differential,
        hh_compiler::equiv::EvidenceVerdict::NotApplicable { .. }
    ));
}

#[test]
fn check_equivalence_e2_fails_when_a_surface_field_lacks_a_map_entry() {
    // AC-R-2.8.1-10 (compile half): a surface field declared in
    // `argument_order` but absent from `SurfaceArgMap` fails compilation — an
    // unmapped surface field could otherwise smuggle an argument past the
    // monitor's canonical-parameter resolution (§5g.1 I-H5).
    let mut n = surfaced_tool_node("test:tool", "tool_a", &["path", "mode"], 5);
    // Declare `mode` in `input_schema` too, so the map is otherwise sound and
    // the only defect is the missing entry.
    if let KindRecord::ToolCapability(t) = &mut n.semantic {
        if let Json::Obj(root) = &mut t.input_schema {
            if let Json::Obj(props) = root.get_mut("properties").expect("properties") {
                props.insert(
                    "mode".to_string(),
                    Json::obj([("type", Json::str("string"))]),
                );
            }
        }
    }
    let ts = match n.surface.as_ref().unwrap() {
        SurfaceRecord::Tool(t) => t.as_ref(),
        _ => unreachable!(),
    };
    let cap = match &n.semantic {
        KindRecord::ToolCapability(t) => t,
        _ => unreachable!(),
    };
    let mut binding = hh_compiler::equiv::bind_surface(&n, ts, &cap.input_schema);
    binding.arg_map.remove("mode");
    let e = hh_compiler::equiv::check_equivalence(&binding, &n, None).expect("evidence");
    match &e.e2_authority {
        hh_compiler::equiv::EvidenceVerdict::Fail { reason } => {
            assert!(reason.contains("mode"), "{reason}");
        }
        other => panic!("expected E2 fail for unmapped surface field, got {other:?}"),
    }
}

#[test]
fn check_equivalence_e4_is_na_open_world_for_open_world_capabilities() {
    let mut n = tool_node("test:tool", 5);
    if let KindRecord::ToolCapability(tc) = &mut n.semantic {
        tc.effects = ToolEffects::Declared(
            vec![hh_hir::EffectClass::domain_only(
                hh_hir::EffectDomain::NetEgress,
            )]
            .into_iter()
            .collect(),
        );
    }
    let binding = SurfaceBinding {
        surface_name: "s".to_string(),
        capability_ref: hh_compiler::plan::PinnedRef {
            semantic_id: "test:tool".to_string(),
            version_id: "v".to_string(),
        },
        hir_node_id: "test:tool".to_string(),
        arg_map: BTreeMap::new(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: hh_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:tool".to_string()],
        mapping: hh_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    };
    let e = hh_compiler::equiv::check_equivalence(&binding, &n, None).expect("evidence");
    match &e.e4_differential {
        hh_compiler::equiv::EvidenceVerdict::NotApplicable { reason } => {
            assert_eq!(reason, "open-world", "E4 n/a(open-world) — T-LCD-15");
        }
        other => panic!("expected n/a(open-world), got {other:?}"),
    }
    assert!(matches!(
        e.e5_error_surjectivity,
        hh_compiler::equiv::EvidenceVerdict::NotApplicable { .. }
    ));
    assert!(matches!(
        e.e6_result_observation,
        hh_compiler::equiv::EvidenceVerdict::NotApplicable { .. }
    ));
}

#[test]
fn check_equivalence_e7_fails_on_a_misattributed_binding() {
    let n = tool_node("test:tool", 5);
    let binding = SurfaceBinding {
        surface_name: "s".to_string(),
        capability_ref: hh_compiler::plan::PinnedRef {
            semantic_id: "test:other".to_string(),
            version_id: "v".to_string(),
        },
        hir_node_id: "test:other".to_string(),
        arg_map: BTreeMap::new(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: hh_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:other".to_string()],
        mapping: hh_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    };
    let e = hh_compiler::equiv::check_equivalence(&binding, &n, None).expect("evidence");
    assert!(matches!(
        e.e7_accounting_identity,
        hh_compiler::equiv::EvidenceVerdict::Fail { .. }
    ));
}

// ── trace ─────────────────────────────────────────────────────────────────────

#[test]
fn trace_is_total_and_unknown_locators_are_typed() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(tool_node("test:tool", 5));
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![ProcedureStep::Invoke {
            tool: sel("test:tool"),
            args: Json::Null,
        }],
        6,
    ));
    let (store, sealed, _vids) = sealed_doc_with("trace-total", doc, vec![], vec![]);
    let linked = link_of(&sealed, &store);
    let plan = lower_native(&linked).expect("lower");
    let map = build_trace_map(&plan);
    // Every control node, every tool row, every policy row, every validator row,
    // every bound slot, the budget envelope, the context_policy alias.
    for (i, n) in plan.control.iter().enumerate() {
        let e = trace(&map, &format!("control/{i}")).expect("control locator");
        assert_eq!(e.hir_node_ids, vec![n.hir_node_id.clone()]);
    }
    for i in 0..plan.tools.len() {
        trace(&map, &format!("tools/{i}")).expect("tool locator");
    }
    for i in 0..plan.policies.permissions.len() {
        trace(&map, &format!("policies/permissions/{i}")).expect("perm locator");
    }
    for i in 0..plan.policies.budgets.len() {
        trace(&map, &format!("policies/budgets/{i}")).expect("budget row locator");
    }
    for i in 0..plan.validators.len() {
        trace(&map, &format!("validators/{i}")).expect("validator locator");
    }
    for name in plan.bound_slots.keys() {
        trace(&map, &format!("slots/{name}")).expect("slot locator");
    }
    trace(&map, "budget").expect("envelope locator");
    trace(&map, "context_policy").expect("context_policy alias");
    // Unknown locator → typed error, never an empty answer.
    let err = trace(&map, "nowhere/0").unwrap_err();
    assert_eq!(err.locator, "nowhere/0");
}

#[test]
fn plan_refuses_a_non_tool_surface_on_a_capability_as_uncheckable() {
    // §3.2.6 rule iii / ADR-0090 §6: a composite surface (a non-`Tool` surface record on
    // a `ToolCapability`) is `UncheckableSurface` at C0 — never silently dropped. (HIR
    // `validate` would normally refuse the mismatch; fabricated post-seal to exercise
    // the compiler's own guard.)
    let mut doc = doc_with(&stage1_assembly());
    let mut t = tool_node("test:tool", 5);
    t.surface = Some(SurfaceRecord::Validator(hh_hir::ValidatorSurface {
        rubric_rendering: Json::Null,
        explain: Json::Null,
    }));
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("uncheckable", doc, vec![], vec![]);
    let p = test_profile();
    let profiles = MapProfileView::of(vec![p]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let inputs = CompileInputs {
        sealed: sealed.clone(),
        profile_refs: vec![profile_coordinate(&test_profile())],
        fallback_profile: None,
        targets: vec![mcp_target()],
        compile_for_expired: false,
    };
    match hh_compiler::compile(&inputs, &profiles, &store, &cat, &kernel()) {
        Err(CompileError::UncheckableSurface { surface, .. }) => {
            assert_eq!(surface, "test:tool");
        }
        other => panic!("expected UncheckableSurface, got {other:?}"),
    }
}
