//! `hh-compiler` R2.18 acceptance suite (spec §3.2.2/§3.3.2, §5b; ticket
//! R2.18; R-2.1.3, R-2.3.3): the link-stage half of the document-side
//! `profile_binding` `ProfileConstraint` — the bound profile must satisfy
//! every declared narrowing member (`C-LINK-4 VersionConflict` when it does
//! not), and the constraint's `fallback_profile` member is the document-side
//! spelling of the ADR-0124 §5 escape `link` consults when nothing is bound.

mod common;

use common::*;
use hh_assembly::grammar::{ConstraintSelector, ProfileBinding, ProfileConstraint};
use hh_compiler::errors::{CompileError, LinkErrorKind};
use hh_compiler::link::{link, LinkedGraph};
use hh_compiler::profile::{profile_coordinate, profile_identity, CapabilityState};
use hh_ontology::eval::{ModelRole, VersionPattern};

/// A sealed doc whose `assembly.profile_binding` is `Constraint(c)` — the
/// grammar decode/encode path is exercised by `resolve` re-materialising the
/// assembly (the same JSON `definition_profile_constraint` reads at link).
fn constrained_doc(
    tag: &str,
    c: ProfileConstraint,
) -> (
    hh_registry::store::RegistryStore,
    hh_hir::document::SealedDefinition,
    std::collections::BTreeMap<String, String>,
) {
    let mut a = stage1_assembly();
    a.profile_binding = ProfileBinding::Constraint(c);
    sealed_doc_with(tag, doc_with(&a), vec![], vec![])
}

fn link_with(
    sealed: &hh_hir::document::SealedDefinition,
    store: &dyn hh_compiler::link::VariantView,
    profile: &hh_compiler::profile::ModelProfile,
) -> Result<LinkedGraph, CompileError> {
    let profiles = MapProfileView::of(vec![profile.clone()]);
    link(
        sealed,
        &[profile_coordinate(profile)],
        None,
        &[mcp_target()],
        &profiles,
        store,
        &kernel(),
        None,
    )
}

/// `test_profile` re-shaped: `profile_id` stays `sha256:profile` so the
/// definition's pin still binds it; the closure re-shapes the selector /
/// capabilities and the `content_hash` is recomputed.
fn reshaped(
    f: impl Fn(&mut hh_compiler::profile::ModelProfile),
) -> hh_compiler::profile::ModelProfile {
    let mut p = test_profile();
    f(&mut p);
    p.content_hash = profile_identity(&p);
    p
}

fn constraint() -> ProfileConstraint {
    ProfileConstraint::default()
}

fn expect_version_conflict(r: Result<LinkedGraph, CompileError>, what: &str) {
    match r {
        Err(CompileError::LinkError {
            kind, diagnostics, ..
        }) => {
            assert_eq!(kind, LinkErrorKind::VersionConflict, "{what}");
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code.code() == "C-LINK-4" && d.stage == hh_assembly::Stage::Link),
                "{what}: the typed C-LINK-4 diagnostic rides the refusal: {diagnostics:?}"
            );
        }
        other => panic!("{what}: expected LinkError{{version_conflict}}, got {other:?}"),
    }
}

// ── the constraint check itself (§5b narrowed binding) ───────────────────────

#[test]
fn link_binds_a_profile_satisfying_every_declared_member() {
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            provider_api_family: Some("test-api".into()),
            model_family: Some("test-model".into()),
            version_pattern: Some(VersionPattern::Any),
            roles_admitted: vec![ModelRole::Primary],
        },
        required_capabilities: vec![],
        fallback_profile: None,
    };
    let (store, sealed, _vids) = constrained_doc("r218-sat", c);
    link_with(&sealed, &store, &test_profile()).expect("a satisfying profile links");
}

#[test]
fn link_refuses_a_profile_outside_the_declared_family() {
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            provider_api_family: Some("other-api".into()),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store, sealed, _vids) = constrained_doc("r218-fam", c);
    expect_version_conflict(
        link_with(&sealed, &store, &test_profile()),
        "provider_api_family mismatch",
    );

    let c2 = ProfileConstraint {
        selector: ConstraintSelector {
            model_family: Some("other-model".into()),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store2, sealed2, _) = constrained_doc("r218-mfam", c2);
    expect_version_conflict(
        link_with(&sealed2, &store2, &test_profile()),
        "model_family mismatch",
    );
}

#[test]
fn link_constraint_version_pattern_is_conservative_subsumption() {
    // Constraint `any` admits the bound `any` (vacuous narrowing).
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            version_pattern: Some(VersionPattern::Any),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-pany", c);
    link_with(&sealed, &store, &test_profile()).expect("any ⊆ any");

    // Constraint `exact(1.0)` does NOT admit the bound `any` — an
    // unprovable narrowing is refused, never silently admitted.
    let c2 = ProfileConstraint {
        selector: ConstraintSelector {
            version_pattern: Some(VersionPattern::Exact("1.0".into())),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store2, sealed2, _) = constrained_doc("r218-pexact", c2);
    expect_version_conflict(link_with(&sealed2, &store2, &test_profile()), "exact ⊉ any");

    // Constraint `prefix(1.)` DOES admit a bound `exact(1.0)`.
    let c3 = ProfileConstraint {
        selector: ConstraintSelector {
            version_pattern: Some(VersionPattern::Prefix("1.".into())),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store3, sealed3, _) = constrained_doc("r218-pprefix", c3);
    let exact = reshaped(|p| p.selector.version_pattern = VersionPattern::Exact("1.0".into()));
    link_with(&sealed3, &store3, &exact).expect("exact(1.0) ⊆ prefix(1.)");
}

#[test]
fn link_constraint_roles_admitted_must_cover_the_declaration() {
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            roles_admitted: vec![ModelRole::Judge],
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-role", c);
    expect_version_conflict(
        link_with(&sealed, &store, &test_profile()),
        "judge ∉ bound roles_admitted [primary]",
    );

    // A profile admitting both roles satisfies the declaration.
    let c2 = ProfileConstraint {
        selector: ConstraintSelector {
            roles_admitted: vec![ModelRole::Primary, ModelRole::Judge],
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store2, sealed2, _) = constrained_doc("r218-roles", c2);
    let wide = reshaped(|p| p.selector.roles_admitted = vec![ModelRole::Primary, ModelRole::Judge]);
    link_with(&sealed2, &store2, &wide).expect("roles covered");
}

#[test]
fn link_constraint_required_capabilities_never_coerce_unknown() {
    // `required_capabilities` demands `declared | probed` on the bound
    // profile — `unknown` (the fixture default) is refused (the router's
    // G-2 rule: unknown never coerces, AC-CP-04).
    let c = ProfileConstraint {
        required_capabilities: vec!["image_input".into()],
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-cap", c);
    expect_version_conflict(
        link_with(&sealed, &store, &test_profile()),
        "image_input is Unknown on the bound profile",
    );

    let declared = reshaped(|p| p.capabilities.image_input = CapabilityState::Declared);
    link_with(&sealed, &store, &declared).expect("image_input declared");
}

#[test]
fn link_constraint_violation_names_the_bound_profile() {
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            provider_api_family: Some("other-api".into()),
            ..ConstraintSelector::default()
        },
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-named", c);
    match link_with(&sealed, &store, &test_profile()) {
        Err(CompileError::LinkError { diagnostics, .. }) => {
            let d = diagnostics
                .iter()
                .find(|d| d.code.code() == "C-LINK-4")
                .expect("C-LINK-4");
            assert_eq!(
                d.subject,
                profile_coordinate(&test_profile()),
                "the diagnostic subjects the violating bound profile"
            );
            assert_eq!(d.path, "/assembly/profile_binding/constraint");
        }
        other => panic!("expected LinkError, got {other:?}"),
    }
}

// ── `fallback_profile` — the document-side ADR-0124 §5 escape ────────────────

#[test]
fn link_constraint_fallback_binds_when_nothing_is_bound() {
    let fb = test_profile();
    let c = ProfileConstraint {
        fallback_profile: Some(profile_coordinate(&fb)),
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-fb", c);
    let profiles = MapProfileView::of(vec![fb]);
    let linked = link(
        &sealed,
        &[], // nothing bound — the constraint's fallback applies
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        None,
    )
    .expect("the document-side fallback binds");
    assert!(
        linked.profile.is_fallback,
        "the bound chain is the fallback"
    );
    assert_eq!(linked.profile.chain.len(), 1);
}

#[test]
fn link_constraint_fallback_must_carry_a_dated_hypothesis() {
    // The ADR-0124 §5 rule applies to the document-side spelling too — an
    // undated fallback is `NoProfile`, never a silent bind.
    let mut undated = test_profile();
    undated.expiry.expiry_condition = hh_compiler::profile::ExpiryCondition {
        kind: hh_compiler::profile::ExpiryKind::ProbeFailure,
        value: None,
    };
    undated.content_hash = profile_identity(&undated);
    let c = ProfileConstraint {
        fallback_profile: Some(profile_coordinate(&undated)),
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-undated", c);
    let profiles = MapProfileView::of(vec![undated]);
    match link(
        &sealed,
        &[],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        None,
    ) {
        Err(CompileError::NoProfile { detail }) => {
            assert!(detail.contains("dated"), "{detail}");
        }
        other => panic!("expected NoProfile, got {other:?}"),
    }
}

#[test]
fn link_constraint_fallback_absent_from_the_view_refuses() {
    let c = ProfileConstraint {
        fallback_profile: Some("sha256:ghost@9.9".into()),
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-ghost", c);
    let profiles = MapProfileView::of(vec![]);
    match link(
        &sealed,
        &[],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        None,
    ) {
        Err(CompileError::NoProfile { detail }) => {
            assert!(detail.contains("sha256:ghost@9.9"), "{detail}");
        }
        other => panic!("expected NoProfile, got {other:?}"),
    }
}

#[test]
fn link_constraint_fallback_must_satisfy_the_constraint_too() {
    // The fallback binds through the same chain path — a fallback outside
    // its own constraint is `C-LINK-4`, not a free pass.
    let fb = test_profile(); // provider_api_family "test-api"
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            provider_api_family: Some("other-api".into()),
            ..ConstraintSelector::default()
        },
        fallback_profile: Some(profile_coordinate(&fb)),
        ..constraint()
    };
    let (store, sealed, _v) = constrained_doc("r218-fbviol", c);
    let profiles = MapProfileView::of(vec![fb]);
    expect_version_conflict(
        link(
            &sealed,
            &[],
            None,
            &[mcp_target()],
            &profiles,
            &store,
            &kernel(),
            None,
        ),
        "the bound fallback violates its own constraint",
    );
}

#[test]
fn link_no_constraint_and_no_fallback_still_refuses_no_profile() {
    // The landed rule is unchanged: `profile_binding` `unbound` + no bound
    // refs + no explicit fallback is `NoProfile` (ADR-0124 §5).
    let (store, sealed, _v) = sealed_doc("r218-plain");
    let profiles = MapProfileView::of(vec![]);
    match link(
        &sealed,
        &[],
        None,
        &[mcp_target()],
        &profiles,
        &store,
        &kernel(),
        None,
    ) {
        Err(CompileError::NoProfile { detail }) => {
            assert!(detail.contains("ADR-0124"), "{detail}");
        }
        other => panic!("expected NoProfile, got {other:?}"),
    }
}
