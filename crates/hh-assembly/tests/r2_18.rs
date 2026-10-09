//! `hh-assembly` R2.18 acceptance suite (spec §3.3.2/§3.3.13, §5b; ticket
//! R2.18; R-2.1.4, R-2.3.3): the document-side `profile_binding`
//! `ProfileConstraint` grammar — parse/validate/round-trip with typed
//! diagnostics — and the C2 organisation layers: the declared
//! `organisation > user > project` `authority_cap` ordering (org caps bound
//! the lower-authority kinds; `packaged-default` is kernel-authored and
//! outside the ordering) plus the `layers[]`/`resolved` authorship refusal.

mod common;

use common::*;
use hh_assembly::compose::{compose, Layer};
use hh_assembly::diagnostics::{Code, Stage};
use hh_assembly::grammar::{
    profile_binding_from_json, Assembly, Constraint, ConstraintKind, ConstraintSelector,
    LayerProvenance, LayerSourceKind, ProfileBinding, ProfileConstraint,
};
use hh_ontology::eval::{ModelRole, VersionPattern};
use hh_wire::json::Json;

fn layer(kind: LayerSourceKind, id: &str, precedence: i64, fragment: Assembly) -> Layer {
    Layer {
        provenance: LayerProvenance {
            source_kind: kind,
            id: id.to_string(),
            version: "1".to_string(),
            precedence,
        },
        fragment,
    }
}

fn cap(of: &str, ceiling: &str) -> Constraint {
    Constraint {
        kind: ConstraintKind::AuthorityCap,
        subject: Json::obj([("of", Json::str(of)), ("ceiling", Json::str(ceiling))]),
        source: None,
    }
}

/// Decode one authored `assembly` JSON, returning `(assembly, diagnostics)`.
fn decode(
    j: &Json,
) -> (
    Option<Assembly>,
    Vec<hh_assembly::diagnostics::AssemblyDiagnostic>,
) {
    let mut diags = Vec::new();
    let a = Assembly::from_json(j, "/assembly", &kernel(), &mut diags);
    (a, diags)
}

fn assembly_json(profile_binding: Json) -> Json {
    let mut a = stage1_assembly();
    a.profile_binding = ProfileBinding::Unbound;
    let mut j = match a.to_json() {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    j.insert("profile_binding".into(), profile_binding);
    Json::Obj(j)
}

// ── ProfileConstraint grammar (§5b narrowed binding) ────────────────────────

#[test]
fn profile_constraint_parses_every_member() {
    let j = Json::obj([(
        "constraint",
        Json::obj([
            (
                "selector",
                Json::obj([
                    ("provider_api_family", Json::str("test-api")),
                    ("model_family", Json::str("test-model")),
                    ("version_pattern", Json::obj([("prefix", Json::str("1."))])),
                    (
                        "roles_admitted",
                        Json::Arr(vec![Json::str("primary"), Json::str("judge")]),
                    ),
                ]),
            ),
            (
                "required_capabilities",
                Json::Arr(vec![Json::str("image_input"), Json::str("tool_search")]),
            ),
            ("fallback_profile", Json::str("sha256:fallback@1.0")),
        ]),
    )]);
    let b = profile_binding_from_json(&j).expect("parses");
    match b {
        ProfileBinding::Constraint(c) => {
            assert_eq!(c.selector.provider_api_family.as_deref(), Some("test-api"));
            assert_eq!(c.selector.model_family.as_deref(), Some("test-model"));
            assert_eq!(
                c.selector.version_pattern,
                Some(VersionPattern::Prefix("1.".into()))
            );
            assert_eq!(
                c.selector.roles_admitted,
                vec![ModelRole::Primary, ModelRole::Judge]
            );
            assert_eq!(
                c.required_capabilities,
                vec!["image_input".to_string(), "tool_search".to_string()]
            );
            assert_eq!(c.fallback_profile.as_deref(), Some("sha256:fallback@1.0"));
        }
        other => panic!("expected Constraint, got {other:?}"),
    }
}

#[test]
fn profile_constraint_round_trips_through_the_canonical_codec() {
    let c = ProfileConstraint {
        selector: ConstraintSelector {
            provider_api_family: Some("test-api".into()),
            model_family: None,
            version_pattern: Some(VersionPattern::Range {
                family: "test-model".into(),
                lo: Some("1.0".into()),
                hi: None,
            }),
            roles_admitted: vec![ModelRole::Primary],
        },
        required_capabilities: vec!["image_input".into()],
        fallback_profile: Some("sha256:fallback@1.0".into()),
    };
    let mut a = stage1_assembly();
    a.profile_binding = ProfileBinding::Constraint(c.clone());
    let (decoded, diags) = decode(&a.to_json());
    assert!(
        diags.is_empty(),
        "a well-formed constraint emits no diagnostics: {diags:?}"
    );
    assert_eq!(
        decoded.expect("decodes").profile_binding,
        ProfileBinding::Constraint(c),
        "the canonical codec round-trips the typed constraint (CC7)"
    );
}

#[test]
fn profile_constraint_empty_object_is_vacuous_but_typed() {
    // `{}` narrows nothing — still a Constraint (never a pin), so the
    // definition stays portable (OQ-078).
    let b = profile_binding_from_json(&Json::obj([("constraint", Json::obj([]))])).unwrap();
    match b {
        ProfileBinding::Constraint(c) => {
            assert!(c.selector.is_vacuous());
            assert!(c.required_capabilities.is_empty());
            assert!(c.fallback_profile.is_none());
        }
        other => panic!("expected Constraint, got {other:?}"),
    }
}

#[test]
fn profile_binding_exclusive_constraint_and_pin_refuse() {
    // `profile_binding ∈ {unbound | ProfileRef | ProfileConstraint}` — a member
    // carrying both forms is malformed, never silently disambiguated.
    let j = Json::obj([
        ("constraint", Json::obj([])),
        ("profile_ref", Json::str("sha256:profile")),
        ("pinned", Json::Bool(true)),
    ]);
    let e = profile_binding_from_json(&j).expect_err("exclusive");
    assert!(e.contains("exclusive"), "{e}");
}

#[test]
fn profile_constraint_unknown_members_are_typed_load_diagnostics() {
    for (member, spelling) in [
        (
            "constraint member",
            Json::obj([("bogus", Json::Bool(true))]),
        ),
        (
            "selector member",
            Json::obj([("selector", Json::obj([("bogus", Json::Bool(true))]))]),
        ),
    ] {
        let j = Json::obj([("constraint", spelling)]);
        let e = profile_binding_from_json(&j).expect_err(member);
        assert!(e.contains("unknown"), "{member}: {e}");
    }
    // And the same spellings surface as `C-LOAD-1` diagnostics through
    // `Assembly::from_json` (never fail-fast — the rest of the assembly
    // still decodes).
    let (a, diags) = decode(&assembly_json(Json::obj([(
        "constraint",
        Json::obj([("bogus", Json::Bool(true))]),
    )])));
    let d = diags
        .iter()
        .find(|d| d.code == Code::LoadParse)
        .expect("C-LOAD-1");
    assert_eq!(d.path, "/assembly/profile_binding");
    assert!(
        a.expect("the assembly still decodes")
            .slots
            .contains_key("control_strategy"),
        "the malformed member does not sink the rest of the assembly"
    );
}

#[test]
fn profile_constraint_bad_member_shapes_refuse_typed() {
    let cases: Vec<(Json, &str)> = vec![
        // `version_pattern` outside the closed grammar.
        (
            Json::obj([(
                "selector",
                Json::obj([("version_pattern", Json::str("regex"))]),
            )]),
            "version_pattern",
        ),
        // An unknown `ModelRole` spelling.
        (
            Json::obj([(
                "selector",
                Json::obj([("roles_admitted", Json::Arr(vec![Json::str("overlord")]))]),
            )]),
            "roles_admitted",
        ),
        // `required_capabilities` not a string list.
        (
            Json::obj([("required_capabilities", Json::Arr(vec![Json::Int(1)]))]),
            "required_capabilities",
        ),
        // `fallback_profile` not a string.
        (
            Json::obj([("fallback_profile", Json::Int(7))]),
            "fallback_profile",
        ),
    ];
    for (constraint, what) in cases {
        let j = Json::obj([("constraint", constraint)]);
        let e = profile_binding_from_json(&j).expect_err(what);
        assert!(e.contains(what), "{what}: {e}");
    }
}

#[test]
fn profile_binding_unbound_and_pinned_spellings_unchanged() {
    // The landed spellings are untouched — the constraint form is additive.
    assert_eq!(
        profile_binding_from_json(&Json::str("unbound")).unwrap(),
        ProfileBinding::Unbound
    );
    match profile_binding_from_json(&Json::obj([
        ("profile_ref", Json::str("sha256:profile")),
        ("pinned", Json::Bool(true)),
    ]))
    .unwrap()
    {
        ProfileBinding::Pinned(p) => {
            assert_eq!(p.profile, "sha256:profile");
            assert!(p.pinned);
        }
        other => panic!("expected Pinned, got {other:?}"),
    }
    assert!(profile_binding_from_json(&Json::str("bogus")).is_err());
}

#[test]
fn bound_profile_coordinates_names_the_constraint_fallback() {
    // Stage 6b consults the document's bound coordinates — a constraint's
    // `fallback_profile` is a coordinate the document can bind, so it must
    // appear (the constraint itself names no concrete profile).
    let mut a = stage1_assembly();
    a.profile_binding = ProfileBinding::Constraint(ProfileConstraint {
        fallback_profile: Some("sha256:fallback@1.0".into()),
        ..ProfileConstraint::default()
    });
    let doc = doc_with(&a);
    let coords = hh_assembly::validate::bound_profile_coordinates(&doc, Some(&a));
    assert!(
        coords.iter().any(|c| c == "sha256:fallback@1.0"),
        "the constraint's fallback is a bound coordinate: {coords:?}"
    );
    // A vacuous constraint names nothing.
    let mut a2 = stage1_assembly();
    a2.profile_binding = ProfileBinding::Constraint(ProfileConstraint::default());
    let doc2 = doc_with(&a2);
    let coords2 = hh_assembly::validate::bound_profile_coordinates(&doc2, Some(&a2));
    assert!(
        !coords2.iter().any(|c| c.contains("fallback")),
        "no fallback → no named coordinate: {coords2:?}"
    );
}

// ── C2 organisation layers (§3.3.13) ─────────────────────────────────────────

#[test]
fn organisation_layer_narrowing_cap_is_admissible_and_recorded() {
    // `organisation > user > project may only narrow` — an org ceiling over a
    // subject bounds lower-authority kinds; a narrower user cap below the org
    // layer's precedence composes.
    let mut org = Assembly::empty();
    org.constraints.push(cap("budget", "principal"));
    let mut user = Assembly::empty();
    user.constraints.push(cap("budget", "external")); // narrower
    let out = compose(
        &[
            layer(LayerSourceKind::Organisation, "org", 50, org),
            layer(LayerSourceKind::User, "user", 10, user),
        ],
        &kernel(),
    )
    .expect("a narrowing user cap below the org layer composes");
    assert_eq!(out.constraints.len(), 2, "both caps are recorded");
    // `layers[]` records the organisation participant (OQ-076).
    let layers = out.layers.expect("layers recorded");
    assert_eq!(layers[0].source_kind, LayerSourceKind::Organisation);
    assert_eq!(
        out.constraints
            .iter()
            .map(|c| c.source.as_ref().unwrap().id.as_str())
            .collect::<Vec<_>>(),
        vec!["user", "org"]
    );
}

#[test]
fn organisation_cap_outranked_by_a_user_cap_is_authority_violation() {
    // A `user` cap at-or-above the organisation layer's precedence over the
    // same subject is the inversion the ordering forbids — `C-COMP-1` naming
    // the inverted layer.
    let mut org = Assembly::empty();
    org.constraints.push(cap("budget", "principal"));
    let mut user = Assembly::empty();
    user.constraints.push(cap("budget", "external")); // narrower, but outranks
    let errs = compose(
        &[
            layer(LayerSourceKind::Organisation, "org", 10, org),
            layer(LayerSourceKind::User, "user", 50, user),
        ],
        &kernel(),
    )
    .expect_err("the inverted ordering refuses");
    let d = errs
        .iter()
        .find(|d| d.code == Code::CompAuthorityViolation)
        .expect("C-COMP-1");
    assert_eq!(d.stage, Stage::Compose);
    assert_eq!(
        d.source_layer.as_deref(),
        Some("user"),
        "the diagnostic names the inverted layer"
    );
}

#[test]
fn organisation_cap_outranked_by_a_project_cap_is_authority_violation() {
    let mut org = Assembly::empty();
    org.constraints.push(cap("tools", "definition"));
    let mut project = Assembly::empty();
    project.constraints.push(cap("tools", "external"));
    let errs = compose(
        &[
            layer(LayerSourceKind::Organisation, "org", 20, org),
            layer(LayerSourceKind::Project, "proj", 20, project), // equal → still outranked
        ],
        &kernel(),
    )
    .expect_err("equal precedence is still at-or-above");
    assert!(errs.iter().any(
        |d| d.code == Code::CompAuthorityViolation && d.source_layer.as_deref() == Some("proj")
    ));
}

#[test]
fn organisation_cap_on_a_different_subject_is_not_an_inversion() {
    // The ordering is per-subject: a user cap over a subject the org layer
    // does not cap is free to sit anywhere in the precedence order.
    let mut org = Assembly::empty();
    org.constraints.push(cap("budget", "principal"));
    let mut user = Assembly::empty();
    user.constraints.push(cap("tools", "external"));
    let out = compose(
        &[
            layer(LayerSourceKind::Organisation, "org", 10, org),
            layer(LayerSourceKind::User, "user", 50, user),
        ],
        &kernel(),
    )
    .expect("a different subject is outside the ordering");
    assert_eq!(out.constraints.len(), 2);
}

#[test]
fn packaged_default_above_an_org_cap_is_not_an_inversion() {
    // `packaged-default` is kernel-authored, not user authority — it is
    // outside the `organisation > user > project` ordering (its own ceiling
    // still faces the monotone `authority_cap` check where applicable).
    let mut org = Assembly::empty();
    org.constraints.push(cap("budget", "principal"));
    let mut packaged = Assembly::empty();
    packaged.constraints.push(cap("budget", "principal")); // equal — not a widening
    let out = compose(
        &[
            layer(LayerSourceKind::Organisation, "org", 10, org),
            layer(LayerSourceKind::PackagedDefault, "pkg", 90, packaged),
        ],
        &kernel(),
    )
    .expect("kernel-authored defaults are outside the user-authority ordering");
    // The two identical caps dedupe under the AppendSet merge; what the test
    // proves is the *absence* of `C-COMP-1` — no inversion is diagnosed.
    assert_eq!(out.constraints.len(), 1);
    assert_eq!(
        out.constraints[0].source.as_ref().unwrap().id.as_str(),
        "org",
        "the surviving cap is attributed to its first authoring layer"
    );
}

#[test]
fn two_organisation_layers_compose_under_the_monotone_cap_rule() {
    // Org layers are inside the ordering for each other only through the
    // generic monotone-cap check: a lower-precedence org layer narrowing is
    // admissible; widening is `C-COMP-1` naming the capping layer.
    let mut hi = Assembly::empty();
    hi.constraints.push(cap("budget", "principal"));
    let mut lo = Assembly::empty();
    lo.constraints.push(cap("budget", "external"));
    let out = compose(
        &[
            layer(LayerSourceKind::Organisation, "org-hi", 50, hi.clone()),
            layer(LayerSourceKind::Organisation, "org-lo", 10, lo.clone()),
        ],
        &kernel(),
    )
    .expect("org narrowing under a higher org cap is admissible");
    assert_eq!(out.constraints.len(), 2);

    let mut wide = Assembly::empty();
    wide.constraints.push(cap("budget", "kernel")); // wider
    let errs = compose(
        &[
            layer(LayerSourceKind::Organisation, "org-hi", 50, hi),
            layer(LayerSourceKind::Organisation, "org-lo", 10, wide),
        ],
        &kernel(),
    )
    .expect_err("a widening lower org cap refuses");
    assert!(errs
        .iter()
        .any(|d| d.code == Code::CompAuthorityViolation
            && d.source_layer.as_deref() == Some("org-lo")));
    let _ = lo;
}

#[test]
fn fragment_authoring_layers_or_resolved_refuses_typed() {
    // `layers[]`/`resolved` are `compose`/`resolve`-derived — a fragment that
    // authors them refuses `C-COMP-4` rather than silently dropping them.
    let mut authored = Assembly::empty();
    authored.layers = Some(vec![LayerProvenance {
        source_kind: LayerSourceKind::User,
        id: "self".into(),
        version: "1".into(),
        precedence: 0,
    }]);
    let errs = compose(
        &[layer(LayerSourceKind::User, "user", 5, authored)],
        &kernel(),
    )
    .expect_err("authoring `layers` refuses");
    let d = errs
        .iter()
        .find(|d| d.code == Code::CompForbiddenBelow)
        .expect("C-COMP-4");
    assert_eq!(d.source_layer.as_deref(), Some("user"));

    let mut authored2 = Assembly::empty();
    authored2.resolved = Some(hh_assembly::grammar::ResolvedInfo {
        registry_snapshot_id: "sha256:snap".into(),
        resolved_at: 7,
    });
    let errs2 = compose(
        &[layer(LayerSourceKind::Organisation, "org", 5, authored2)],
        &kernel(),
    )
    .expect_err("authoring `resolved` refuses");
    assert!(errs2.iter().any(|d| d.code == Code::CompForbiddenBelow));
}
