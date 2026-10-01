//! `R-2.3.3` — the profile test contract (ADR-0125; ticket S3.7): the V1–V9
//! validity suite, golden surfaces (V5), the capability-consistency check
//! (V4 + derived constraints), the C0 probe verdicts (AC-5), the link gate
//! (`profile_untested`/`profile_invalid`/`capability_drift` — AC-13),
//! selector grammar + `fallback_used` (AC-2), and the reflexive null-profile
//! compile (AC-14 shape).

mod common;

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::errors::{CompileError, LinkErrorKind};
use hh_compiler::profile::{
    minimal_profiles, null_profile, profile_coordinate, CapabilityState, ModelProfile,
    ProfileRuleKind,
};
use hh_compiler::profile_test::{
    evaluate_probe, test_profile as run_test_profile, ConformanceRecord, ConformanceVerdict,
    GoldenSurfaces, ProbeKind, ProbeOutcome, ProbeSpec, ProfileTestFixtures, ProfileTestReport,
    ValidityId, ValidityVerdict,
};
use hh_compiler::CompileInputs;
use hh_wire::json::Json;

/// A sealed doc whose agent node leaves `native.profile` unbound — the link
/// binds whatever `profile_refs`/`fallback_profile` names (the minimal and
/// null profiles are not the fixture's `sha256:profile` pin).
fn unpinned_doc(
    tag: &str,
) -> (
    hh_registry::store::RegistryStore,
    hh_hir::SealedDefinition,
    std::collections::BTreeMap<String, String>,
) {
    let mut doc = doc_with(&stage1_assembly());
    for n in &mut doc.nodes {
        if let hh_hir::records::KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let hh_hir::records::AgentProcessBody::Native(nat) = &mut ap.body {
                nat.profile = hh_hir::refs::ProfileRef::unbound();
            }
        }
    }
    sealed_doc_with(tag, doc, vec![], vec![])
}

fn compile_with(
    sealed: &hh_hir::SealedDefinition,
    profile: &ModelProfile,
    view: &dyn hh_compiler::profile::ProfileView,
    store: &hh_registry::store::RegistryStore,
) -> Result<hh_compiler::seal::CompiledBundle, CompileError> {
    compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![profile_coordinate(profile)],
            fallback_profile: None,
            targets: vec![mcp_target()],
            compile_for_expired: false,
        },
        view,
        store,
        &hh_assembly::Stage1Catalog::stage1(),
        &kernel(),
    )
}

fn ok_fixtures() -> ProfileTestFixtures {
    ProfileTestFixtures {
        determinism_witnesses: vec!["sha256:w1".to_string(), "sha256:w1".to_string()],
        ..ProfileTestFixtures::default()
    }
}

fn section(report: &ProfileTestReport, id: ValidityId) -> &ValidityVerdict {
    &report
        .validity
        .iter()
        .find(|s| s.id == id)
        .expect("section present")
        .verdict
}

// ── AC-R-2.3.3-13 — the link gate ────────────────────────────────────────────

#[test]
fn ac_2_3_3_13_untested_refuses_reported_compiles() {
    let (store, sealed, _v) = sealed_doc("pt13");
    let p = test_profile();

    // No report → LinkError{profile_untested}.
    let untested = MapProfileView::untested(vec![p.clone()]);
    match compile_with(&sealed, &p, &untested, &store) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, LinkErrorKind::ProfileUntested);
            assert_eq!(kind.name(), "profile_untested");
        }
        other => panic!("expected profile_untested, got {other:?}"),
    }

    // A failing validity section → LinkError{profile_invalid}.
    let mut bad_report = ProfileTestReport::passing_for(&profile_coordinate(&p), &p.content_hash);
    bad_report.validity[4].verdict = ValidityVerdict::Fail {
        diagnostics: vec!["v5: golden drift".to_string()],
    };
    let invalid = MapProfileView::untested(vec![p.clone()]).with_report(bad_report, &p);
    match compile_with(&sealed, &p, &invalid, &store) {
        Err(CompileError::LinkError { kind, detail, .. }) => {
            assert_eq!(kind, LinkErrorKind::ProfileInvalid);
            assert!(detail.contains("v5"), "{detail}");
        }
        other => panic!("expected profile_invalid, got {other:?}"),
    }

    // A passing report → the bundle carries `profile_test_report_ref`.
    let tested = MapProfileView::of(vec![p.clone()]);
    let bundle = compile_with(&sealed, &p, &tested, &store).expect("compiles");
    let report_ref = bundle
        .profile_test_report_ref
        .as_ref()
        .expect("diagnostics.profile_test_report_ref present");
    assert!(report_ref.starts_with("sha256:"));
    assert!(!bundle.fallback_used);
}

// ── AC-R-2.3.3-5 — the C0 probe suite ────────────────────────────────────────

#[test]
fn ac_2_3_3_5_probe_verdicts_and_drift_gate() {
    let spec = ProbeSpec {
        capability: "native_function_calling".to_string(),
        kind: ProbeKind::SchemaAccept,
        fixture_ref: "fixture:strict-schema".to_string(),
        samples: 1,
        budget: 1,
        verdict_rule: "default".to_string(),
    };
    // Scripted acceptance under a declared capability → SUPPORTED.
    let r = evaluate_probe(
        &spec,
        CapabilityState::Declared,
        &ProbeOutcome::Accepted,
        "p@1",
        "run-1",
    );
    assert_eq!(r.verdict, ConformanceVerdict::Supported);
    assert_eq!(r.probed, "supported");
    // Scripted rejection under a declared capability → DRIFT.
    let r = evaluate_probe(
        &spec,
        CapabilityState::Declared,
        &ProbeOutcome::Rejected,
        "p@1",
        "run-1",
    );
    assert_eq!(r.verdict, ConformanceVerdict::Drift);
    // Inconclusive → UNKNOWN, never coerced.
    let r = evaluate_probe(
        &spec,
        CapabilityState::Declared,
        &ProbeOutcome::Inconclusive {
            reason: "timeout".to_string(),
        },
        "p@1",
        "run-1",
    );
    assert_eq!(r.verdict, ConformanceVerdict::Unknown);
    // Rejection under an `unknown` declaration is observed UNSUPPORTED, not
    // drift (nothing declared to contradict).
    let r = evaluate_probe(
        &spec,
        CapabilityState::Unknown,
        &ProbeOutcome::Rejected,
        "p@1",
        "run-1",
    );
    assert_eq!(r.verdict, ConformanceVerdict::Unsupported);

    // A round-trip mismatch under a declared capability is DRIFT (the C0
    // `name_roundtrip`/`id_roundtrip` shape).
    for kind in [ProbeKind::NameRoundtrip, ProbeKind::IdRoundtrip] {
        let spec = ProbeSpec {
            kind,
            ..spec.clone()
        };
        let r = evaluate_probe(
            &spec,
            CapabilityState::Declared,
            &ProbeOutcome::RoundtripMismatch {
                sent: "edit_file".to_string(),
                got: "editfile".to_string(),
            },
            "p@1",
            "run-1",
        );
        assert_eq!(r.verdict, ConformanceVerdict::Drift);
    }

    // DRIFT on a dependency capability refuses link without intent…
    let (store, sealed, _v) = sealed_doc("pt5");
    let mut p = test_profile();
    p.rules.push(rule(
        "r.naming",
        ProfileRuleKind::Naming,
        &["tools/*/name"],
        Json::obj([("scheme", Json::str("prefix"))]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p.capabilities.native_function_calling = CapabilityState::Declared;
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    let mut report = ProfileTestReport::passing_for(&profile_coordinate(&p), &p.content_hash);
    report.probes.push(ConformanceRecord {
        profile_version: profile_coordinate(&p),
        capability: "native_function_calling".to_string(),
        declared: "declared".to_string(),
        probed: "unsupported".to_string(),
        verdict: ConformanceVerdict::Drift,
        probe_run_id: "run-drift".to_string(),
    });
    let drifted = MapProfileView::untested(vec![p.clone()]).with_report(report.clone(), &p);
    match compile_with(&sealed, &p, &drifted, &store) {
        Err(CompileError::LinkError { kind, detail, .. }) => {
            assert_eq!(kind, LinkErrorKind::CapabilityDrift);
            assert!(detail.contains("native_function_calling"), "{detail}");
        }
        other => panic!("expected capability_drift, got {other:?}"),
    }
    // …and passes with a recorded intent.
    let bundle = compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![profile_coordinate(&p)],
            fallback_profile: None,
            targets: vec![mcp_target()],
            compile_for_expired: true, // the recorded intent
        },
        &drifted,
        &store,
        &hh_assembly::Stage1Catalog::stage1(),
        &kernel(),
    )
    .expect("recorded intent admits the drifted dependency");
    assert!(!bundle.bundle_id.is_empty());
}

// ── AC-R-2.3.3-2 — selector grammar + fallback_used ─────────────────────────

#[test]
fn ac_2_3_3_2_fallback_used_recorded() {
    let (store, sealed, _v) = sealed_doc("pt2");
    let p = test_profile();
    let view = MapProfileView::of(vec![p.clone()]);
    // Bind through the explicit `fallback_profile` escape (the profile carries
    // a dated debt hypothesis, so it is admissible) — `fallback_used` is
    // recorded on the bundle.
    let bundle = compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![],
            fallback_profile: Some(profile_coordinate(&p)),
            targets: vec![mcp_target()],
            compile_for_expired: false,
        },
        &view,
        &store,
        &hh_assembly::Stage1Catalog::stage1(),
        &kernel(),
    )
    .expect("fallback binds");
    assert!(bundle.fallback_used, "fallback_used is recorded");
}

// ── AC-R-2.3.3-3 — golden surfaces (V5) ─────────────────────────────────────

#[test]
fn ac_2_3_3_3_golden_surfaces_v5() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes
        .push(surfaced_tool_node("test:tool", "tool_a", &["path"], 5));
    for n in &mut doc.nodes {
        if let hh_hir::records::KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let hh_hir::records::AgentProcessBody::Native(nat) = &mut ap.body {
                nat.profile = hh_hir::refs::ProfileRef::unbound();
            }
        }
    }
    let (store, sealed, _v) = sealed_doc_with("pt3", doc, vec![], vec![]);
    let (patch, _replace) = minimal_profiles();
    let view = MapProfileView::of(vec![patch.clone()]);
    let bundle = compile_with(&sealed, &patch, &view, &store).expect("compiles");

    // The observed golden addresses come from the real lowering.
    let hh_compiler::seal::ModelSurfaceState::Lowered(surface) = &bundle.model_surface else {
        panic!("lowered surface")
    };
    let observed_surfaces: std::collections::BTreeMap<String, String> = surface
        .tools
        .iter()
        .map(|t| {
            // The golden address covers the *whole* compiled surface record
            // (id, schema, description — not just `surface_id`, whose basis
            // excludes the description leaf).
            let addr = hh_identity::idp::idp_id(
                "golden.surface",
                format!(
                    "{}∥{}∥{}∥{}",
                    t.binding.surface_id,
                    t.binding.surface_name,
                    t.schema.to_canonical_string(),
                    t.description
                )
                .as_bytes(),
            );
            (
                format!("{}∥{}", t.binding.hir_node_id, t.binding.surface_name),
                addr,
            )
        })
        .collect();
    let layout_hash = hh_identity::idp::idp_id(
        "golden.layout",
        Json::Arr(
            surface
                .layout
                .iter()
                .map(|s| Json::str(s.section_id.clone()))
                .collect(),
        )
        .to_canonical_string()
        .as_bytes(),
    );
    let renderer_hash = hh_identity::idp::idp_id(
        "golden.renderer",
        surface.transcript_renderer.to_canonical_string().as_bytes(),
    );

    // V5 pass: observed == recorded.
    let mut fixtures = ok_fixtures();
    fixtures.observed_surfaces = observed_surfaces.clone();
    fixtures.observed_layout_hash = layout_hash.clone();
    fixtures.observed_renderer_spec_hash = renderer_hash.clone();
    fixtures.recorded = GoldenSurfaces {
        surfaces: observed_surfaces.clone(),
        layout_hash: layout_hash.clone(),
        renderer_spec_hash: renderer_hash.clone(),
    };
    let report = run_test_profile(&patch, &fixtures, 1);
    assert_eq!(section(&report, ValidityId::V5), &ValidityVerdict::Pass);
    assert_eq!(
        report.golden.surfaces, observed_surfaces,
        "the report carries the golden map"
    );

    // A recorded address that does not match → V5 fail.
    let mut drifted = ok_fixtures();
    drifted.observed_surfaces = observed_surfaces;
    drifted.observed_layout_hash = layout_hash;
    drifted.observed_renderer_spec_hash = renderer_hash;
    let mut recorded = fixtures.recorded.clone();
    for v in recorded.surfaces.values_mut() {
        *v = "sha256:stale".to_string();
    }
    drifted.recorded = recorded;
    let report = run_test_profile(&patch, &drifted, 1);
    assert!(matches!(
        section(&report, ValidityId::V5),
        ValidityVerdict::Fail { .. }
    ));

    // A description edit changes the profile hash and the golden surface
    // address — never a HIR `semantic_id` (the capability's id is stable).
    let mut edited = patch.clone();
    edited.rules.push(rule(
        "r.description",
        ProfileRuleKind::DescriptionTemplate,
        &["tools/*/description_template"],
        Json::obj([("template", Json::str("edited {purpose}"))]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    edited.content_hash = hh_compiler::profile::profile_identity(&edited);
    assert_ne!(edited.content_hash, patch.content_hash);
    let edited_view = MapProfileView::of(vec![edited.clone()]);
    let edited_bundle = compile_with(&sealed, &edited, &edited_view, &store).expect("compiles");
    let hh_compiler::seal::ModelSurfaceState::Lowered(edited_surface) =
        &edited_bundle.model_surface
    else {
        panic!("lowered")
    };
    let orig_ids: Vec<String> = surface
        .tools
        .iter()
        .map(|t| t.binding.hir_node_id.clone())
        .collect();
    let edited_ids: Vec<String> = edited_surface
        .tools
        .iter()
        .map(|t| t.binding.hir_node_id.clone())
        .collect();
    assert_eq!(orig_ids, edited_ids, "HIR semantic ids unchanged");
    let golden_addr = |t: &hh_compiler::lower::CompiledToolSurface| {
        hh_identity::idp::idp_id(
            "golden.surface",
            format!(
                "{}∥{}∥{}∥{}",
                t.binding.surface_id,
                t.binding.surface_name,
                t.schema.to_canonical_string(),
                t.description
            )
            .as_bytes(),
        )
    };
    let orig_addrs: Vec<String> = surface.tools.iter().map(golden_addr).collect();
    let edited_addrs: Vec<String> = edited_surface.tools.iter().map(golden_addr).collect();
    assert_ne!(orig_addrs, edited_addrs, "the golden address moved");
}

// ── AC-R-2.3.3-4 — V4 capability consistency + derived constraints ──────────

#[test]
fn ac_2_3_3_4_v4_capability_consistency() {
    // A rule assuming an unestablished capability fails V4 (C0 spells
    // `unsupported` as `unknown`).
    let mut p = test_profile();
    p.rules.push(rule(
        "r.caching",
        ProfileRuleKind::CachingMarkers,
        &["caching_markers"],
        Json::obj([]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    let report = run_test_profile(&p, &ok_fixtures(), 1);
    match section(&report, ValidityId::V4) {
        ValidityVerdict::Fail { diagnostics } => {
            assert!(diagnostics
                .iter()
                .any(|d| d.contains("cache_control_convention")));
        }
        other => panic!("expected V4 fail, got {other:?}"),
    }

    // `*_required` capabilities derive constraints — never ruled.
    let mut p2 = test_profile();
    p2.capabilities.tool_result_name_required = CapabilityState::Declared;
    p2.capabilities.assistant_required_after_tool_result = CapabilityState::Declared;
    p2.content_hash = hh_compiler::profile::profile_identity(&p2);
    let report2 = run_test_profile(&p2, &ok_fixtures(), 1);
    assert_eq!(section(&report2, ValidityId::V4), &ValidityVerdict::Pass);
    let derived: Vec<&str> = report2
        .derived_constraints
        .iter()
        .map(|c| c.derived.as_str())
        .collect();
    assert!(derived.contains(&"renderer.emit_tool_result_name"));
    assert!(derived.contains(&"renderer.emit_assistant_after_tool_result"));
}

// ── The remaining validity sections ──────────────────────────────────────────

#[test]
fn v1_v2_v3_sections() {
    let (patch, _replace) = minimal_profiles();

    // V2: the minimal profiles' diff vs null ⊆ owned_fields.
    let report = run_test_profile(&patch, &ok_fixtures(), 1);
    assert_eq!(section(&report, ValidityId::V1), &ValidityVerdict::Pass);
    assert_eq!(section(&report, ValidityId::V2), &ValidityVerdict::Pass);
    assert_eq!(section(&report, ValidityId::V3), &ValidityVerdict::Pass);

    // V3 fails on an incomplete debt record.
    let mut broken = test_profile();
    broken.rules.push(rule(
        "r.bad",
        ProfileRuleKind::Naming,
        &[],
        Json::obj([]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    broken.rules[0].debt.removal_test = None; // incomplete
    broken.content_hash = hh_compiler::profile::profile_identity(&broken);
    let report = run_test_profile(&broken, &ok_fixtures(), 1);
    assert!(matches!(
        section(&report, ValidityId::V3),
        ValidityVerdict::Fail { .. }
    ));

    // V6 needs ≥ 2 equal witnesses.
    let mut f = ok_fixtures();
    f.determinism_witnesses = vec!["sha256:a".to_string(), "sha256:b".to_string()];
    let report = run_test_profile(&patch, &f, 1);
    assert!(matches!(
        section(&report, ValidityId::V6),
        ValidityVerdict::Fail { .. }
    ));
}

#[test]
fn v9_role_placement() {
    // A total, monotone role_map passes; a non-total one fails V9.
    let mut p = test_profile();
    let role_map = Json::Obj(
        [
            "unverified",
            "external",
            "environment",
            "delegate",
            "principal",
            "definition",
            "kernel",
        ]
        .iter()
        .enumerate()
        .map(|(i, c)| ((*c).to_string(), Json::str(format!("s{i}"))))
        .collect(),
    );
    let order = Json::Arr((0..7).map(|i| Json::str(format!("s{i}"))).collect());
    let slots = Json::Obj(
        [
            "kernel",
            "definition",
            "principal",
            "transcript",
            "external",
            "unverified",
        ]
        .iter()
        .map(|s| ((*s).to_string(), Json::obj([])))
        .collect(),
    );
    p.rules.push(rule(
        "r.layout",
        ProfileRuleKind::PromptLayout,
        &["layout"],
        Json::obj([("order", order), ("role_map", role_map), ("slots", slots)]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p.capabilities.developer_role = CapabilityState::Declared;
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    let report = run_test_profile(&p, &ok_fixtures(), 1);
    assert_eq!(section(&report, ValidityId::V9), &ValidityVerdict::Pass);

    // Drop a class → non-total → V9 fail.
    let mut p2 = p.clone();
    if let Json::Obj(m) = &mut p2.rules[0].params {
        if let Some(Json::Obj(rm)) = m.get_mut("role_map") {
            rm.remove("kernel");
        }
    }
    p2.content_hash = hh_compiler::profile::profile_identity(&p2);
    let report = run_test_profile(&p2, &ok_fixtures(), 1);
    match section(&report, ValidityId::V9) {
        ValidityVerdict::Fail { diagnostics } => {
            assert!(
                diagnostics.iter().any(|d| d.contains("kernel")),
                "{diagnostics:?}"
            );
        }
        other => panic!("expected V9 fail, got {other:?}"),
    }
}

#[test]
fn v8_error_format() {
    // Surjective + distinguishable renderings pass V8.
    let renderings = Json::Obj(
        [
            "unparseable",
            "unknown_surface",
            "unmapped_argument",
            "domain_violation",
            "missing_required",
            "multiple_calls_unsupported",
            "grammar_violation",
            "session_state_invalid",
        ]
        .iter()
        .enumerate()
        .map(|(i, f)| ((*f).to_string(), Json::str(format!("error {i}"))))
        .collect(),
    );
    let mut p = test_profile();
    p.rules.push(rule(
        "r.err",
        ProfileRuleKind::ErrorFormat,
        &["error_format"],
        Json::obj([("renderings", renderings)]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    let report = run_test_profile(&p, &ok_fixtures(), 1);
    assert_eq!(section(&report, ValidityId::V8), &ValidityVerdict::Pass);

    // A missing class fails surjectivity.
    let mut p2 = p.clone();
    if let Json::Obj(m) = &mut p2.rules[0].params {
        if let Some(Json::Obj(rm)) = m.get_mut("renderings") {
            rm.remove("unparseable");
        }
    }
    p2.content_hash = hh_compiler::profile::profile_identity(&p2);
    let report = run_test_profile(&p2, &ok_fixtures(), 1);
    assert!(matches!(
        section(&report, ValidityId::V8),
        ValidityVerdict::Fail { .. }
    ));
}

// ── AC-R-2.3.3-14 — the reflexive removal test (shape) ───────────────────────

#[test]
fn ac_2_3_3_14_null_profile_compiles_without_a_report() {
    let (store, sealed, _v) = unpinned_doc("pt14");
    let null = null_profile();
    let view = MapProfileView::untested(vec![null.clone()]);
    // The kernel null profile is exempt from the report gate — compiling every
    // registered definition under it is the Profile Compiler's own retirement
    // design (AC-R-2.3.3-14).
    let bundle = compile_with(&sealed, &null, &view, &store).expect("null profile compiles");
    assert!(bundle.profile_test_report_ref.is_none());
    // And the null profile's own test report is all-pass under honest fixtures.
    let report = run_test_profile(&null, &ok_fixtures(), 1);
    assert!(report.validity_ok());
}

// ── Report codec round-trip (the out-of-process seam carries reports) ────────

#[test]
fn test_report_codec_round_trip() {
    let mut report = ProfileTestReport::passing_for("p@1", "sha256:h");
    report.probes.push(ConformanceRecord {
        profile_version: "p@1".to_string(),
        capability: "native_function_calling".to_string(),
        declared: "declared".to_string(),
        probed: "supported".to_string(),
        verdict: ConformanceVerdict::Supported,
        probe_run_id: "run-1".to_string(),
    });
    let j = hh_compiler::profile_test::test_report_json(&report);
    let back = hh_compiler::profile_test::test_report_from_json(&j).expect("decodes");
    assert_eq!(back, report);
    // Canonical bytes are stable.
    assert_eq!(
        j.to_canonical_string(),
        hh_compiler::profile_test::test_report_json(&back).to_canonical_string()
    );
}

// ── AC-R-2.3.3 / ADR-0124 d.4 — retire_rule: the removal-test P − R arm ──────

#[test]
fn retire_rule_produces_a_valid_reduced_profile() {
    use hh_compiler::profile::{retire_rule, RetireError};
    let (patch, _replace) = minimal_profiles();
    let (reduced, diff) = retire_rule(&patch, "minimal-patch.naming").expect("retires");
    // P − R drops exactly the named rule and recomputes its content address.
    assert_eq!(reduced.rules.len(), patch.rules.len() - 1);
    assert!(reduced
        .rules
        .iter()
        .all(|r| r.rule_id != "minimal-patch.naming"));
    assert_ne!(reduced.content_hash, patch.content_hash);
    assert_eq!(
        reduced.content_hash,
        hh_compiler::profile::profile_identity(&reduced)
    );
    // The diff records what the removal dropped.
    assert_eq!(diff.removed_rule, "minimal-patch.naming");
    assert_eq!(diff.dropped_owned_fields, vec!["tools/*/name".to_string()]);
    assert_eq!(
        diff.remaining_rules,
        vec!["minimal-patch.tool_shape.edit_file".to_string()]
    );
    assert!(!diff.removal_test_ref.is_empty());
    // The reduced profile is itself valid — the V2 closure predicate holds.
    let owned = hh_compiler::profile::owned_fields(&reduced);
    for m in hh_compiler::profile::member_diff(&null_profile(), &reduced) {
        assert!(owned.contains(&m), "unowned member {m}");
    }
    // Unknown rule → typed refusal.
    assert!(matches!(
        retire_rule(&patch, "minimal-patch.nonexistent"),
        Err(RetireError::RuleNotFound { .. })
    ));
    // A rule with no removal_test refuses — deletion ≠ retirement.
    let mut no_test = patch.clone();
    no_test.rules[1].debt.removal_test = None;
    assert!(matches!(
        retire_rule(&no_test, "minimal-patch.tool_shape.edit_file"),
        Err(RetireError::RuleRequired { .. })
    ));
    // A dangling supersedes anchor refuses.
    let mut anchored = patch.clone();
    anchored.rules[1].supersedes = Some("minimal-patch.naming".to_string());
    assert!(matches!(
        retire_rule(&anchored, "minimal-patch.naming"),
        Err(RetireError::RuleRequired { .. })
    ));
}
