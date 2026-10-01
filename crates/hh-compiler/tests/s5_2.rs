//! S5.2 — the split/composite tool-surface family (§5d.2; R-2.5.2 C2):
//!
//! - AC-R-2.5.2-3 — `SurfaceFamily`/`VariantSelector`/`PlanMap`, the split
//!   and static-composite modes, per-branch E1–E3 evidence.
//! - AC-R-2.5.2-4 — the composite/shim/code-mode loss entries (the MCP
//!   container has no slot for nested surfaces — recorded, never silent).
//! - AC-R-2.5.2-5 — `SchemaDialectTransform` declares every narrowing;
//!   undeclared invention/nullability loss refuses.
//! - AC-R-2.5.2-8 — the S1–S4 synthesis gates + `synthesize_surface` /
//!   `retire_surface`.
//! - AC-R-2.5.2-9 — `freeform` (parse through a declared grammar) and
//!   `code_mode` (per-call nested effects, catalogue membership).
//! - AC-R-2.5.2-10 — `shim` resolution is exact-table only; a miss is
//!   `UnknownSurface`, never a best-effort match; interpreter calls meter
//!   as `model_call` under the calling role's token role.

use std::collections::{BTreeMap, BTreeSet};

use hh_compiler::equiv::{
    ArgMapEntry, ArgTransform, EvidenceVerdict, SurfaceArgMap, SurfaceBinding,
};
use hh_compiler::family::{
    self, check_code_mode_sandbox, check_dialect_transform, check_family, check_plan_map,
    code_mode_catalogue, code_mode_nested_call, freeform_map, retire_surface, select_variant,
    shim_metering, shim_resolve, split_bindings, synthesize_surface, AdmissionResult,
    CodeModeSandbox, DialectOp, EnvironmentDeclaration, FreeformSpec, PlanMap, PlanStep,
    SafetyInput, SchemaDialectTransform, SurfaceFamily, SurfaceProposal, SurfaceVariant,
    TaskClassDeclaration, VariantSelector,
};
use hh_compiler::plan::PinnedRef;
use hh_compiler::surface::{BindingMapping, CompileExposureMode, SurfaceFailure};
use hh_compiler::CompileError;
use hh_provenance::authority::AuthorityClass;
use hh_wire::json::Json;

// ── helpers ──────────────────────────────────────────────────────────────────

fn binding(name: &str, cap: &str, mode: CompileExposureMode) -> SurfaceBinding {
    let mut b = SurfaceBinding {
        surface_name: name.to_string(),
        surface_id: String::new(),
        exposure_mode: mode,
        capability_ref: PinnedRef {
            semantic_id: cap.to_string(),
            version_id: format!("sha256:cap-{cap}"),
        },
        capability_refs: vec![cap.to_string()],
        hir_node_id: cap.to_string(),
        arg_map: BTreeMap::new(),
        mapping: BindingMapping::SurfaceArgMap,
        rule_ids: Vec::new(),
        evidence_ref: None,
        safety_ref: None,
        effects_bound: Vec::new(),
        family_id: None,
        variant_id: None,
        dialect: hh_compiler::equiv::DEFAULT_SCHEMA_DIALECT.to_string(),
        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    };
    b.surface_id = hh_compiler::surface::surface_id(&b);
    b
}

fn family_fixture() -> (SurfaceFamily, BTreeMap<String, SurfaceBinding>) {
    let mut primitive = binding("read_file", "cap/read_file", CompileExposureMode::Primitive);
    primitive.family_id = Some("family/fs".into());
    primitive.variant_id = Some("v:reference".into());
    let mut composite = binding(
        "file_editor",
        "cap/read_file",
        CompileExposureMode::Composite,
    );
    composite.family_id = Some("family/fs".into());
    composite.variant_id = Some("v:composite".into());
    composite.capability_refs = vec!["cap/read_file".into(), "cap/edit_file".into()];
    let family = SurfaceFamily {
        family_id: "family/fs".to_string(),
        capability_refs: vec!["cap/read_file".to_string(), "cap/edit_file".to_string()],
        reference_variant: "v:reference".to_string(),
        variants: vec![
            SurfaceVariant {
                variant_id: "v:composite".to_string(),
                exposure_mode: CompileExposureMode::Composite,
                surfaces: vec!["file_editor".to_string()],
                selector: VariantSelector::TaskClass("coding".to_string()),
                rule_ids: vec![],
            },
            SurfaceVariant {
                variant_id: "v:reference".to_string(),
                exposure_mode: CompileExposureMode::Primitive,
                surfaces: vec!["read_file".to_string()],
                selector: VariantSelector::Always,
                rule_ids: vec![],
            },
        ],
    };
    let bindings: BTreeMap<String, SurfaceBinding> = [
        ("read_file".to_string(), primitive),
        ("file_editor".to_string(), composite),
    ]
    .into_iter()
    .collect();
    (family, bindings)
}

fn task(id: &str) -> TaskClassDeclaration {
    TaskClassDeclaration {
        task_class_id: id.to_string(),
        labels: vec![],
        expected_capabilities: vec![],
    }
}

fn env() -> EnvironmentDeclaration {
    EnvironmentDeclaration {
        environment_count: 1,
        executor_platform: "linux".to_string(),
        session_support: false,
        capability_declaration: vec![],
    }
}

fn complete_debt(rule_id: &str) -> hh_hir::records::AssumptionDebtRecord {
    hh_hir::records::AssumptionDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: hh_hir::leaves::Text::new(
            "the surface holds while the capability does",
            "test",
            hh_provenance::record::ProvenanceRecord::kernel("kernel:test", 0),
        ),
        evidence_refs: vec![hh_hir::EvidenceRef::legacy("sha256:ev")],
        owner: hh_hir::OwnerRef::principal("test:owner"),
        expiry_condition: hh_hir::ExpiryCondition {
            kind: hh_hir::ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "sha256:removal".to_string(),
        status: hh_hir::DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: None,
        created_by: None,
        created_at: None,
        supersedes: None,
    }
}

// ── AC-R-2.5.2-3 — family + selector + PlanMap ───────────────────────────────

#[test]
fn ac_r_2_5_2_3_family_checks_and_selector_evaluation() {
    let (family, bindings) = family_fixture();
    check_family(&family, &bindings).unwrap();
    // The composite variant serves `coding` tasks; everything else falls
    // back to the `reference_variant` (the native_fc rendering).
    let declared: BTreeSet<String> = BTreeSet::new();
    assert_eq!(
        select_variant(&family, &task("coding"), &env(), &Json::Null, &declared).variant_id,
        "v:composite"
    );
    assert_eq!(
        select_variant(&family, &task("chat"), &env(), &Json::Null, &declared).variant_id,
        "v:reference"
    );
    // The selector vocabulary is closed — no `ModelIdentity` member exists;
    // a `turn_field` read of an undeclared field is `false` (never a peek).
    assert!(!VariantSelector::TurnField {
        field: "session_id".to_string(),
        equals: Json::Bool(true),
    }
    .eval(
        &task("coding"),
        &env(),
        &Json::obj([("session_id", Json::Bool(true))]),
        &declared
    ));
    let declared = ["session_id".to_string()].into_iter().collect();
    assert!(VariantSelector::TurnField {
        field: "session_id".to_string(),
        equals: Json::Bool(true),
    }
    .eval(
        &task("coding"),
        &env(),
        &Json::obj([("session_id", Json::Bool(true))]),
        &declared
    ));
    // A reference variant that is not a primitive/split rendering refuses.
    let (mut bad, bindings) = family_fixture();
    bad.reference_variant = "v:composite".to_string();
    assert!(matches!(
        check_family(&bad, &bindings),
        Err(CompileError::UncheckableSurface { .. })
    ));
    // A binding reaching outside `capability_refs` is `AuthorityWidening`.
    let (family, mut bindings) = family_fixture();
    bindings
        .get_mut("file_editor")
        .unwrap()
        .capability_refs
        .push("cap/exec".to_string());
    assert!(matches!(
        check_family(&family, &bindings),
        Err(CompileError::AuthorityWidening { .. })
    ));
}

#[test]
fn ac_r_2_5_2_3_plan_map_checkable_steps_and_s1_inclusion() {
    // The canonical fixtures check clean against their allowed sets.
    let allowed: BTreeSet<String> = ["read_file", "write_file", "edit_file"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let map = family::str_replace_editor_plan();
    check_plan_map(&map, &allowed).unwrap();
    let invoked = family::capabilities_invoked(&map);
    assert_eq!(
        invoked,
        ["edit_file", "read_file", "write_file"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    let ue = family::unified_exec_plan();
    let allowed2: BTreeSet<String> = ["process_write"].iter().map(|s| s.to_string()).collect();
    check_plan_map(&ue, &allowed2).unwrap();
    // An `Invoke` outside `allowed_capabilities` is `AuthorityWidening` (S1).
    let bad = PlanMap {
        steps: vec![PlanStep::Invoke {
            capability_ref: "exec_shell".to_string(),
            arg_map: SurfaceArgMap::new(),
        }],
        session_state: None,
    };
    assert!(matches!(
        check_plan_map(&bad, &allowed),
        Err(CompileError::AuthorityWidening { .. })
    ));
    // A `Loop`/`Delegate`/`Opaque`/`Instruction` step is
    // `UncheckableSurface` — the shape is code-mode by definition.
    for step in [
        PlanStep::Loop(vec![]),
        PlanStep::Delegate,
        PlanStep::Opaque("x".into()),
        PlanStep::Instruction("y".into()),
    ] {
        let m = PlanMap {
            steps: vec![step],
            session_state: None,
        };
        assert!(matches!(
            check_plan_map(&m, &allowed),
            Err(CompileError::UncheckableSurface { .. })
        ));
    }
}

#[test]
fn ac_r_2_5_2_3_split_bindings_expand_one_capability() {
    let base = {
        let mut b = binding("exec", "cap/exec", CompileExposureMode::Primitive);
        b.arg_map = [
            (
                "cmd".to_string(),
                ArgMapEntry {
                    capability_param: "cmd".to_string(),
                    transform: ArgTransform::Identity,
                    narrowing: None,
                },
            ),
            (
                "stream".to_string(),
                ArgMapEntry {
                    capability_param: "stream".to_string(),
                    transform: ArgTransform::Identity,
                    narrowing: None,
                },
            ),
        ]
        .into_iter()
        .collect();
        b
    };
    let splits = Json::Arr(vec![Json::obj([
        ("name", Json::str("write_stdin")),
        ("const_arg", Json::str("stream")),
        ("const_value", Json::str("stdin")),
        ("drop_args", Json::Arr(vec![Json::str("cmd")])),
    ])]);
    let out = split_bindings(&base, &splits).unwrap();
    assert_eq!(out.len(), 1);
    let b = &out[0];
    assert_eq!(b.surface_name, "write_stdin");
    assert_eq!(b.exposure_mode, CompileExposureMode::Split);
    // The const arg binds `const{value}`; the dropped arg leaves the map.
    assert!(!b.arg_map.contains_key("cmd"));
    assert!(matches!(
        b.arg_map.get("stream").map(|e| &e.transform),
        Some(ArgTransform::Const(v)) if v.as_str() == Some("stdin")
    ));
    // Each split mints its own `surface_id` (E7 — the rename changes only
    // the surface coordinate).
    assert_ne!(b.surface_id, base.surface_id);
    // A split without `splits[]` is `UnexpressibleSurface`, never a silent
    // pass-through.
    assert!(matches!(
        split_bindings(&base, &Json::Null),
        Err(CompileError::UnexpressibleSurface { .. })
    ));
}

// ── AC-R-2.5.2-5 — dialect transforms declare every narrowing ────────────────

#[test]
fn ac_r_2_5_2_5_dialect_narrowing_must_be_declared() {
    let cap_schema = Json::obj([
        ("type", Json::str("string")),
        (
            "enum",
            Json::Arr(vec![Json::str("a"), Json::str("b"), Json::str("c")]),
        ),
    ]);
    // A widened surface schema refuses regardless of declarations.
    let wide = Json::obj([("type", Json::str("string"))]);
    let t = SchemaDialectTransform {
        dialect: "openapi-3.0".to_string(),
        ops: vec![DialectOp {
            op: "enum_narrow".to_string(),
            keyword: "enum".to_string(),
            narrowed: vec!["enum_members_dropped".to_string()],
        }],
    };
    assert!(matches!(
        check_dialect_transform(&t, &cap_schema, &wide),
        Err(CompileError::DialectNarrowingUndeclared { .. })
    ));
    // A narrowing with no declared effect refuses.
    let narrow = Json::obj([
        ("type", Json::str("string")),
        ("enum", Json::Arr(vec![Json::str("a")])),
    ]);
    let silent = SchemaDialectTransform {
        dialect: "openapi-3.0".to_string(),
        ops: vec![DialectOp {
            op: "enum_narrow".to_string(),
            keyword: "enum".to_string(),
            narrowed: vec![],
        }],
    };
    assert!(matches!(
        check_dialect_transform(&silent, &cap_schema, &narrow),
        Err(CompileError::DialectNarrowingUndeclared { .. })
    ));
    // The declared narrowing reports the declared effect set.
    let declared = check_dialect_transform(&t, &cap_schema, &narrow).unwrap();
    assert!(declared.iter().any(|d| d == "enum_members_dropped"));
}

// ── AC-R-2.5.2-8 — the S1–S4 gates + synthesize/retire ───────────────────────

fn safety_input<'a>(
    binding: &'a SurfaceBinding,
    plan_map: Option<&'a PlanMap>,
    cap_fx: &'a BTreeSet<String>,
    grants: &'a BTreeSet<String>,
    allowed: &'a BTreeSet<String>,
    nested_mediated: bool,
) -> SafetyInput<'a> {
    SafetyInput {
        binding,
        plan_map,
        capability_effects: cap_fx,
        ceiling_grants: grants,
        allowed_capabilities: allowed,
        nested_mediated,
        synthesized_authority: None,
        target_slot_min_authority: None,
    }
}

#[test]
fn ac_r_2_5_2_8_synthesis_gates_and_retire() {
    let allowed: BTreeSet<String> = ["read_file", "edit_file"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let cap_fx: BTreeSet<String> = ["fs.write"].iter().map(|s| s.to_string()).collect();
    let grants: BTreeSet<String> = ["fs.write", "fs.read"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let b = binding(
        "file_editor",
        "cap/read_file",
        CompileExposureMode::Primitive,
    );
    // S1 fails when the declared effects bound escapes the capability's.
    let mut wide = b.clone();
    wide.effects_bound = vec!["net.egress".to_string()];
    let ev = family::check_safety(&safety_input(&wide, None, &cap_fx, &grants, &allowed, true));
    assert!(matches!(
        ev.s1_no_authority_widening,
        EvidenceVerdict::Fail { .. }
    ));
    // A clean primitive surface passes S1; S2 is `n/a` (not composite);
    // synthesize admits with a complete debt record.
    let mut good = b.clone();
    good.effects_bound = vec!["fs.write".to_string()];
    let input = safety_input(&good, None, &cap_fx, &grants, &allowed, true);
    let ev = family::check_safety(&input);
    assert!(matches!(ev.s1_no_authority_widening, EvidenceVerdict::Pass));
    let proposal = SurfaceProposal {
        binding: good.clone(),
        plan_map: None,
        origin: "model".to_string(),
        hypothesis: complete_debt("rule/syn-1"),
        target_home: "profile_rule".to_string(),
    };
    let res = synthesize_surface(&proposal, &input);
    assert!(matches!(res, AdmissionResult::Admitted { .. }));
    // An incomplete debt record refuses at the `debt` gate.
    let mut bad_debt = complete_debt("rule/syn-2");
    bad_debt.removal_test_ref = String::new();
    let proposal = SurfaceProposal {
        hypothesis: bad_debt,
        ..proposal
    };
    let res = synthesize_surface(&proposal, &input);
    assert!(matches!(
        res,
        AdmissionResult::Refused { ref gate, .. } if gate == "debt"
    ));
    // A composite without per-step mediation evidence fails S2.
    let map = PlanMap {
        steps: vec![PlanStep::Invoke {
            capability_ref: "read_file".to_string(),
            arg_map: SurfaceArgMap::new(),
        }],
        session_state: None,
    };
    let mut composite = binding("composite", "cap/read_file", CompileExposureMode::Composite);
    composite.mapping = BindingMapping::PlanMap(family::plan_map_ref(&map));
    composite.effects_bound = vec!["fs.write".to_string()];
    let ev = family::check_safety(&safety_input(
        &composite,
        Some(&map),
        &cap_fx,
        &grants,
        &allowed,
        false,
    ));
    assert!(matches!(
        ev.s2_nested_mediation,
        EvidenceVerdict::Fail { .. }
    ));
    // S3 — synthesized text above `delegate` refuses.
    let mut input3 = safety_input(&good, None, &cap_fx, &grants, &allowed, true);
    input3.synthesized_authority = Some(AuthorityClass::Definition);
    let ev = family::check_safety(&input3);
    assert!(matches!(ev.s3_labelled_text, EvidenceVerdict::Fail { .. }));
    // `retire_surface` — the admitted rule leaves; the residual is the arm.
    let rules = vec!["rule:syn-1".to_string(), "rule:syn-2".to_string()];
    let residual = retire_surface(&rules, "rule:syn-1").unwrap();
    assert_eq!(residual, vec!["rule:syn-2".to_string()]);
    assert!(retire_surface(&rules, "rule:absent").is_none());
}

// ── AC-R-2.5.2-9/-10 — freeform, code_mode, shim ─────────────────────────────

#[test]
fn ac_r_2_5_2_9_freeform_and_code_mode() {
    // `freeform` — the single `parse{grammar_ref}` map (the mode's shape).
    let map = freeform_map(&FreeformSpec {
        grammar_ref: "grammar/patch@1".to_string(),
        capability_param: "edits".to_string(),
        surface_arg: "patch_text".to_string(),
    });
    assert_eq!(map.len(), 1);
    assert!(matches!(
        map.get("patch_text").map(|e| &e.transform),
        Some(ArgTransform::Parse { grammar_ref }) if grammar_ref == "grammar/patch@1"
    ));
    // `code_mode` — a nested call is its own `Effect` under the parent's id.
    let nested = code_mode_nested_call(
        "effect:parent",
        "surface:x",
        &Json::obj([("arg", Json::str("v"))]),
        0,
    );
    assert_eq!(nested.parent_effect_id, "effect:parent");
    assert_ne!(nested.effect_id, "effect:parent");
    // The catalogue admits only `code_mode`-declared bindings — tools
    // lacking the hint are absent (never an error entry).
    let mut cm = binding("nested", "cap/x", CompileExposureMode::Primitive);
    cm.admitted_modes
        .insert(hh_hir::tools::ExposureMode::CodeMode);
    let plain = binding("plain", "cap/y", CompileExposureMode::Primitive);
    let items = [cm, plain];
    let catalogue = code_mode_catalogue(&items);
    assert_eq!(catalogue.len(), 1);
    assert_eq!(catalogue[0].surface_name, "nested");
    // The sandbox declaration cannot hold credentials (structural S2).
    assert!(check_code_mode_sandbox(&CodeModeSandbox {
        holds_credentials: false,
        ambient_authority: false,
    })
    .is_ok());
    assert!(matches!(
        check_code_mode_sandbox(&CodeModeSandbox {
            holds_credentials: true,
            ambient_authority: false,
        }),
        Err(CompileError::AuthorityWidening { .. })
    ));
}

#[test]
fn ac_r_2_5_2_10_shim_resolves_exact_names_only_and_meters_as_model_call() {
    let table: BTreeMap<String, String> =
        [("apply_patch".to_string(), "surface:patch".to_string())]
            .into_iter()
            .collect();
    let call = shim_resolve(
        "apply_patch",
        Json::obj([("patch", Json::str("@@"))]),
        &table,
    )
    .unwrap();
    assert_eq!(call.surface_id, "surface:patch");
    assert_eq!(call.authority, AuthorityClass::Delegate);
    // A miss is `UnknownSurface` — never a best-effort/fuzzy match (S4).
    assert!(matches!(
        shim_resolve("apply-pathc", Json::Null, &table),
        Err(SurfaceFailure::UnknownSurface)
    ));
    // The interpreter call meters as a `model_call` under the *calling*
    // role's token role (never a fourth bucket).
    let m = shim_metering("utility");
    assert_eq!(m.effect_kind, "model_call");
    assert_eq!(m.token_role, "utility");
    assert_eq!(m.attribution, "harness_overhead.shim_interpreter");
}
