//! `hh-compiler` acceptance suite (spec §3.2.12; ticket S1.10; R-2.1.3).
//! The deterministic AC-CP-* halves: byte-identity in-process and out-of-process
//! (AC-CP-01/11), trace totality (AC-CP-03), `UnexpressibleSurface` (AC-CP-05), the
//! debt-record gate + expired-rule warnings (AC-CP-06 stage-1 half), per-surface
//! equivalence evidence (AC-CP-10), and the composed static `lcd_report` (CF-050).
//!
//! S3.2 rows (this ticket): AC-CP-02 (two-profile stage-3 diff), AC-CP-06
//! stage-3 half + AC-CP-09 (`relower`), AC-CP-04/-07/-08/-12 (lower/lift/loss/
//! relower evidence) and AC-CP-13 (MCP catalogue) — see `tests/s3_2.rs`.

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::equiv::EvidenceVerdict;
use hh_compiler::errors::CompileError;
use hh_compiler::link::NoVariants;
use hh_compiler::profile::profile_coordinate;
use hh_compiler::schema::{bundle_to_json, compile_inputs_json, WireCompileInputs};
use hh_compiler::trace::trace;
use hh_compiler::CompileInputs;
use hh_hir::records::*;
use hh_hir::ToolEffects;
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

fn inputs_for(
    sealed: &hh_hir::SealedDefinition,
    profile: &hh_compiler::profile::ModelProfile,
) -> CompileInputs {
    CompileInputs {
        sealed: sealed.clone(),
        profile_refs: vec![profile_coordinate(profile)],
        fallback_profile: None,
        targets: vec![mcp_target()],
        compile_for_expired: false,
    }
}

fn compile_ok(
    sealed: &hh_hir::SealedDefinition,
    store: &hh_registry::store::RegistryStore,
    profile: &hh_compiler::profile::ModelProfile,
) -> hh_compiler::seal::CompiledBundle {
    let profiles = MapProfileView::of(vec![profile.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    compile(
        &inputs_for(sealed, profile),
        &profiles,
        store,
        &cat,
        &kernel(),
    )
    .expect("compile")
}

// ── AC-CP-01 — byte-identical bundles ─────────────────────────────────────────

#[test]
fn ac_cp_01_two_compiles_are_byte_identical() {
    let (store, sealed, _vids) = sealed_doc("cp01");
    let p = test_profile();
    let a = compile_ok(&sealed, &store, &p);
    let b = compile_ok(&sealed, &store, &p);
    assert_eq!(a.bundle_id, b.bundle_id, "bundle_id is content-addressed");
    assert_eq!(a.derivation_key, b.derivation_key);
    assert_eq!(
        bundle_to_json(&a).to_canonical_string(),
        bundle_to_json(&b).to_canonical_string(),
        "canonical bundle bytes are identical"
    );
}

#[test]
fn ac_cp_01_inputs_change_the_derivation_key() {
    let (store, sealed, _vids) = sealed_doc("cp01-key");
    let p = test_profile();
    let a = compile_ok(&sealed, &store, &p);
    // A different bound target changes the derivation key and the bundle id.
    let mut inputs = inputs_for(&sealed, &p);
    inputs.targets = vec![hh_compiler::link::TargetSpec {
        target_id: "provider_tool_api".to_string(),
        spec_version: "1.0".to_string(),
        content_hash: "sha256:target-provider".to_string(),
    }];
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let b = compile(&inputs, &profiles, &store, &cat, &kernel()).expect("compile");
    assert_ne!(a.derivation_key, b.derivation_key, "inputs are in the key");
    assert_ne!(a.bundle_id, b.bundle_id);
    // A different profile content changes it too.
    let mut p2 = test_profile();
    p2.capabilities.tool_search = hh_compiler::profile::CapabilityState::Declared;
    p2.content_hash = hh_compiler::profile::profile_identity(&p2);
    let profiles2 = MapProfileView::of(vec![p2.clone()]);
    let c = compile(
        &inputs_for(&sealed, &p2),
        &profiles2,
        &store,
        &cat,
        &kernel(),
    )
    .expect("compile");
    assert_ne!(
        a.derivation_key, c.derivation_key,
        "profile hashes are in the key"
    );
}

#[test]
fn ac_cp_01_no_clock_or_env_nondeterminism() {
    // The compile signature carries no clock/env; the only time-like input is the
    // caller-supplied provenance `created_at` seq — held fixed here, so a compile is
    // reproducible across *processes* (AC-CP-11 is the out-of-process witness).
    let (store, sealed, _vids) = sealed_doc("cp01-hermetic");
    let p = test_profile();
    let a = compile_ok(&sealed, &store, &p);
    // A second kernel record with a different component name produces the same
    // bundle — the provenance context mints diagnostics, never bundle content.
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let other_kernel = hh_provenance::ProvenanceRecord::kernel("other-component", 0);
    let b = compile(
        &inputs_for(&sealed, &p),
        &profiles,
        &store,
        &cat,
        &other_kernel,
    )
    .expect("compile");
    assert_eq!(a.bundle_id, b.bundle_id);
    assert_eq!(a.derivation_key, b.derivation_key);
}

// ── AC-CP-03 — trace totality ─────────────────────────────────────────────────

#[test]
fn ac_cp_03_every_emitted_element_traces() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes.push(tool_node("test:tool", 5));
    doc.nodes.push(validator_node("test:val", 6));
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![
            ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::Null,
            },
            ProcedureStep::Verify {
                validator: sel("test:val"),
            },
        ],
        7,
    ));
    let (store, sealed, _vids) = sealed_doc_with("cp03", doc, vec![], vec![]);
    let p = test_profile();
    let bundle = compile_ok(&sealed, &store, &p);
    // Every plan element resolves through the map.
    let plan = &bundle.runtime_plan;
    for (i, _) in plan.control.iter().enumerate() {
        trace(&bundle.trace_map, &format!("control/{i}")).expect("control");
    }
    for i in 0..plan.tools.len() {
        let e = trace(&bundle.trace_map, &format!("tools/{i}")).expect("tool");
        assert_eq!(e.hir_node_ids, vec!["test:tool".to_string()]);
        if plan.tools[i].surface.is_some() {
            trace(&bundle.trace_map, &format!("tools/{i}/surface")).expect("surface");
        }
    }
    for i in 0..plan.validators.len() {
        let e = trace(&bundle.trace_map, &format!("validators/{i}")).expect("validator");
        assert_eq!(e.hir_node_ids, vec!["test:val".to_string()]);
    }
    for name in plan.bound_slots.keys() {
        trace(&bundle.trace_map, &format!("slots/{name}")).expect("slot");
    }
    trace(&bundle.trace_map, "budget").expect("budget envelope");
    trace(&bundle.trace_map, "context_policy").expect("context_policy");
    // The map is exactly the emitted set — every entry's locator resolves.
    assert!(!bundle.trace_map.entries.is_empty());
}

// ── AC-CP-05 — UnexpressibleSurface is an error through `compile` ─────────────

#[test]
fn ac_cp_05_inexpressible_surface_is_a_compile_error() {
    let (store, sealed, _vids) = sealed_doc("cp05");
    let r = rule(
        "r-mode",
        hh_compiler::profile::ProfileRuleKind::InteractionMode,
        &[],
        Json::obj([("mode", Json::str("freeform"))]),
        hh_compiler::profile::DebtStatus::Active,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    match compile(&inputs_for(&sealed, &p), &profiles, &store, &cat, &kernel()) {
        Err(CompileError::UnexpressibleSurface { reason, .. }) => {
            assert!(reason.contains("native_fc"), "{reason}");
        }
        other => panic!("expected UnexpressibleSurface, got {other:?}"),
    }
}

// ── AC-CP-06 — debt completeness + expired rules (stage-1 half) ───────────────

#[test]
fn ac_cp_06_incomplete_debt_refuses_expired_lands_in_lcd_report() {
    // (i) incomplete debt → missing_debt_record refusal.
    let (store, sealed, _vids) = sealed_doc("cp06a");
    let mut r = rule(
        "r-gap",
        hh_compiler::profile::ProfileRuleKind::SamplingDefaults,
        &[],
        Json::Null,
        hh_compiler::profile::DebtStatus::Active,
    );
    r.debt.removal_test_ref = String::new();
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    match compile(&inputs_for(&sealed, &p), &profiles, &store, &cat, &kernel()) {
        Err(CompileError::LinkError { kind, .. }) => {
            assert_eq!(kind, hh_compiler::LinkErrorKind::MissingDebtRecord);
        }
        other => panic!("expected missing_debt_record, got {other:?}"),
    }
    // (ii) expired rule under recorded intent → warning diagnostic + conditioned_rules.
    let r = rule(
        "r-exp",
        hh_compiler::profile::ProfileRuleKind::SamplingDefaults,
        &[],
        Json::Null,
        hh_compiler::profile::DebtStatus::Expired,
    );
    let p = profile_with("sha256:profile", "1.0", vec![r]);
    let mut inputs = inputs_for(&sealed, &p);
    inputs.compile_for_expired = true;
    let profiles = MapProfileView::of(vec![p.clone()]);
    let bundle = compile(&inputs, &profiles, &store, &cat, &kernel()).expect("compile");
    assert!(bundle
        .diagnostics
        .iter()
        .any(|d| d.code.code() == "C-LINK-6" && d.severity == hh_assembly::Severity::Warning));
    assert!(bundle
        .lcd_report
        .conditioned_rules
        .iter()
        .any(|c| c.rule_id == "r-exp"
            && c.home == hh_compiler::link::ConditionedRuleHome::Profile
            && c.status == "expired"));
}

// ── AC-CP-10 — per-surface equivalence evidence ───────────────────────────────

#[test]
fn ac_cp_10_every_compiled_surface_carries_e1_e3_e7_and_typed_e4() {
    let mut doc = doc_with(&stage1_assembly());
    doc.nodes
        .push(surfaced_tool_node("test:tool", "tool_a", &["path"], 5));
    let (store, sealed, _vids) = sealed_doc_with("cp10", doc, vec![], vec![]);
    let p = test_profile();
    let bundle = compile_ok(&sealed, &store, &p);
    assert_eq!(bundle.equivalence_evidence.len(), 1);
    let e = &bundle.equivalence_evidence[0];
    assert_eq!(e.surface_name, "tool_a");
    assert_eq!(e.capability, "test:tool");
    assert_eq!(e.e1_effect_equality, EvidenceVerdict::Pass);
    assert_eq!(e.e2_authority, EvidenceVerdict::Pass);
    assert_eq!(e.e3_precondition_domain, EvidenceVerdict::Pass);
    assert_eq!(e.e7_accounting_identity, EvidenceVerdict::Pass);
    // E4–E6 are typed n/a, never pass/fail/0 at C0 (T-LCD-15; §3.2.14).
    for v in [
        &e.e4_differential,
        &e.e5_error_surjectivity,
        &e.e6_result_observation,
    ] {
        assert!(matches!(v, EvidenceVerdict::NotApplicable { .. }));
    }
    // E4 for a pure (closed-world) capability with no declared suite is
    // `n/a{no_declared_suite}` (the executable E4 lands at S3.2 — a profile's
    // `tests.e4_suites[]` flips it to a real verdict).
    match &e.e4_differential {
        EvidenceVerdict::NotApplicable { reason } => {
            assert_eq!(reason, "no_declared_suite")
        }
        _ => unreachable!(),
    }
}

#[test]
fn ac_cp_10_open_world_capability_records_e4_na_open_world() {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    if let KindRecord::ToolCapability(tc) = &mut t.semantic {
        tc.effects = ToolEffects::Declared(
            vec![hh_hir::EffectClass::domain_only(
                hh_hir::EffectDomain::NetEgress,
            )]
            .into_iter()
            .collect(),
        );
        // R-2.8.2: a gated (egress) capability must carry a `flow_contract`
        // to seal — the minimal declared contribution.
        tc.flow_contract = Some(Json::obj([(
            "contribution",
            Json::obj([("readers_from", Json::str("public"))]),
        )]));
    }
    doc.nodes.push(t);
    let (store, sealed, _vids) = sealed_doc_with("cp10-ow", doc, vec![], vec![]);
    let p = test_profile();
    let bundle = compile_ok(&sealed, &store, &p);
    let e = &bundle.equivalence_evidence[0];
    match &e.e4_differential {
        EvidenceVerdict::NotApplicable { reason } => {
            assert_eq!(reason, "open-world", "T-LCD-15: never 0/fail")
        }
        other => panic!("expected n/a(open-world), got {other:?}"),
    }
}

// ── lcd_report — the static composition (CF-050) ──────────────────────────────

#[test]
fn lcd_report_composes_stage0_derived_results_never_recomputes() {
    let (store, sealed, _vids) = sealed_doc("lcd");
    let p = test_profile();
    let bundle = compile_ok(&sealed, &store, &p);
    let report = &bundle.lcd_report;
    // Composed members present (empty is a legitimate composed value — the report is
    // *carried*, not recomputed).
    assert!(
        report.benchmark_conditioned_rules.is_empty(),
        "7-L2 clean fixture"
    );
    assert!(report.hosting_edges.is_empty());
    // Stage 4 lands at S3.2: one `LoweringLossReport` per bound target (`mcp`
    // here); the entries may be empty — the report is *carried*.
    assert_eq!(report.lowering_loss.len(), 1);
    assert_eq!(report.lowering_loss[0].target, "mcp");
    // per_profile_diff_fields covers the bound chain.
    assert!(report
        .per_profile_diff_fields
        .contains_key(&profile_coordinate(&p)));
    // opacity composed when the derived results carry one.
    assert_eq!(
        report.opacity,
        bundle
            .opacity_report
            .as_ref()
            .map(|o| (o.opaque_ratio_num, o.total))
    );
    // The bundle record itself is complete.
    assert!(!bundle.bundle_id.is_empty());
    assert!(!bundle.derivation_key.is_empty());
    // Stage 3 lands at S3.2 — the surface is produced.
    assert!(matches!(
        bundle.model_surface,
        hh_compiler::seal::ModelSurfaceState::Lowered(_)
    ));
    assert!(bundle.target_artefacts.contains_key("mcp"));
    assert_eq!(bundle.loss_reports.len(), 1);
}

// ── AC-CP-11 — the out-of-process seam ────────────────────────────────────────

#[test]
fn ac_cp_11_hh_compile_produces_identical_bytes() {
    let (store, sealed, vids) = sealed_doc("cp11");
    let p = test_profile();
    let bundle = compile_ok(&sealed, &store, &p);

    // Carry the bound views as data (the binary has no registry).
    let variants: Vec<(String, hh_registry::records::VariantRecord)> = vids
        .values()
        .map(|vid| {
            let (_, rec) = store.get(vid).expect("registered");
            match rec {
                RegistryRecord::Variant(v) => (vid.clone(), v.clone()),
                _ => panic!("not a variant"),
            }
        })
        .collect();
    let wire = WireCompileInputs {
        document: sealed.document.to_json(),
        profile_refs: vec![profile_coordinate(&p)],
        fallback_profile: None,
        profiles: vec![p.clone()],
        variants,
        targets: vec![mcp_target()],
        compile_for_expired: false,
        // The registry's report for the bound profile travels with the inputs
        // (the binary has no registry — AC-CP-11 carries the bound views).
        test_reports: vec![hh_compiler::profile_test::ProfileTestReport::passing_for(
            &profile_coordinate(&p),
            &p.content_hash,
        )],
    };
    let stdin_bytes = compile_inputs_json(&wire).to_canonical_string();

    let exe = env!("CARGO_BIN_EXE_hh-compile");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hh-compile");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin_bytes.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "hh-compile exited {:?}", out.status);
    let expected = bundle_to_json(&bundle).to_canonical_string();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        expected,
        "out-of-process compile is byte-identical (AC-CP-11/T-LCD-12)"
    );
}

#[test]
fn ac_cp_11_hh_compile_refuses_canonically() {
    // A refusal on the out-of-process path is the same typed error envelope.
    let (store, sealed, vids) = sealed_doc("cp11-err");
    let mut bad = test_profile();
    bad.expiry.expiry_condition = hh_compiler::profile::ExpiryCondition {
        kind: hh_compiler::profile::ExpiryKind::ProbeFailure,
        value: None,
    };
    bad.content_hash = hh_compiler::profile::profile_identity(&bad);
    let variants: Vec<(String, hh_registry::records::VariantRecord)> = vids
        .values()
        .map(|vid| {
            let (_, rec) = store.get(vid).expect("registered");
            match rec {
                RegistryRecord::Variant(v) => (vid.clone(), v.clone()),
                _ => panic!("not a variant"),
            }
        })
        .collect();
    // No profiles bound, undated fallback → NoProfile.
    let wire = WireCompileInputs {
        document: sealed.document.to_json(),
        profile_refs: vec![],
        fallback_profile: Some(profile_coordinate(&bad)),
        profiles: vec![bad],
        variants,
        targets: vec![],
        compile_for_expired: false,
        test_reports: vec![],
    };
    let stdin_bytes = compile_inputs_json(&wire).to_canonical_string();
    let exe = env!("CARGO_BIN_EXE_hh-compile");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hh-compile");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin_bytes.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(!out.status.success(), "a refusal exits non-zero");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\"NoProfile\""), "{text}");
}

// ── no-registry compile (the view is data — CF-046/047) ───────────────────────

#[test]
fn compile_runs_without_a_registry_store() {
    // The bound views are caller-supplied data: `NoVariants` + a map. A definition
    // whose slots the view cannot satisfy is `version_conflict` — the compiler never
    // reaches a live registry.
    let (_store, sealed, _vids) = sealed_doc("noreg");
    let p = test_profile();
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let err = compile(
        &inputs_for(&sealed, &p),
        &profiles,
        &NoVariants,
        &cat,
        &kernel(),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CompileError::LinkError {
            kind: hh_compiler::LinkErrorKind::VersionConflict,
            ..
        }
    ));
}
