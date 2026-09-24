//! `hh-compiler` S3.2 acceptance suite (spec §3.2; ticket S3.2; R-2.1.2,
//! R-2.1.3): stage 3 `lower_profile` under the two ADR-0020 minimal profiles,
//! stage 4 provider tool-API + MCP targets with typed loss reports, `lift`'s
//! `PartialHIR` (T-LCD-11), `relower`, the executable E4 suite and E5/E6 checks,
//! `opacity`/`ablate` (AC-IR-03), the bundle opacity ratio (AC-CP-12), and the
//! two-profile × two-target matrix (T-LCD-04/AC-CP-07; AC-CP-08's behavioural
//! margin is §10's paired-runs obligation — the executable slice here is the
//! deterministic surface/plan equivalence).

mod common;

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::equiv::{self, EvidenceVerdict};
use hh_compiler::errors::CompileError;
use hh_compiler::lcd::LossKind;
use hh_compiler::profile::{minimal_profiles, profile_coordinate, profile_identity, ModelProfile};
use hh_compiler::relower::relower;
use hh_compiler::seal::ModelSurfaceState;
use hh_compiler::target::{lift, HH_META_KEY};
use hh_compiler::CompileInputs;
use hh_hir::records::*;
use hh_hir::refs::ProfileRef;
use hh_wire::json::Json;

/// The mini-SWE-agent-class reference fixture (ADR-0144 anchor): rule + budget +
/// permission + agent + `edit_file` (surfaced — the capability the two minimal
/// profiles shape differently) + `get_file` + a procedure that invokes them.
fn reference_doc(profile_pin: ProfileRef) -> hh_hir::document::HirDocument {
    let mut doc = doc_with(&stage1_assembly());
    // Re-pin the agent's profile per the caller (unbound for the two-profile arms).
    for n in doc.nodes.iter_mut() {
        if let KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let AgentProcessBody::Native(np) = &mut ap.body {
                np.profile = profile_pin.clone();
            }
        }
    }
    // `edit_file` — args `{path, edits}`; the fs_map world's apply shape.
    let mut edit = surfaced_tool_node("test:edit_file", "edit_file", &["path", "edits"], 5);
    if let KindRecord::ToolCapability(t) = &mut edit.semantic {
        t.input_schema = Json::obj([(
            "properties",
            Json::obj([
                ("path", Json::obj([("type", Json::str("string"))])),
                (
                    "edits",
                    Json::obj([(
                        "items",
                        Json::obj([(
                            "properties",
                            Json::obj([
                                ("new", Json::obj([("type", Json::str("string"))])),
                                ("old", Json::obj([("type", Json::str("string"))])),
                            ]),
                        )]),
                    )]),
                ),
            ]),
        )]);
    }
    doc.nodes.push(edit);
    // `get_file` — a second surfaced tool (deliberately not an E4-required name).
    doc.nodes.push(surfaced_tool_node(
        "test:get_file",
        "get_file",
        &["path"],
        6,
    ));
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![
            ProcedureStep::Invoke {
                tool: sel("test:get_file"),
                args: Json::Null,
            },
            ProcedureStep::Invoke {
                tool: sel("test:edit_file"),
                args: Json::Null,
            },
        ],
        7,
    ));
    doc
}

fn unbound() -> ProfileRef {
    ProfileRef::unbound()
}

/// The `fs_map` edit case — `{a.txt: "hello world"}` → `{a.txt: "hello there"}`.
fn canonical_edit() -> Json {
    Json::obj([
        ("path", Json::str("a.txt")),
        (
            "edits",
            Json::Arr(vec![Json::obj([
                ("new", Json::str("there")),
                ("old", Json::str("world")),
            ])]),
        ),
    ])
}

/// The E4 suite for `minimal-patch` — the surface form carries a patch text the
/// `parse(grammar_ref)` transform renders into `edits`.
fn patch_suite() -> Json {
    let mut grammars = std::collections::BTreeMap::new();
    grammars.insert(
        "sha256:patch-grammar".to_string(),
        Json::obj([
            ("grammar", Json::str("hh-line-patch/1")),
            ("open", Json::str("<<<< OLD")),
            ("mid", Json::str("====")),
            ("close", Json::str(">>>> NEW")),
        ]),
    );
    Json::obj([
        ("suite_id", Json::str("e4:edit_file")),
        ("capability", Json::str("edit_file")),
        ("world", Json::str("fs_map")),
        ("initial", Json::obj([("a.txt", Json::str("hello world"))])),
        ("grammars", Json::Obj(grammars)),
        (
            "cases",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("replace")),
                (
                    "surface_args",
                    Json::obj([
                        ("path", Json::str("a.txt")),
                        ("patch", Json::str("<<<< OLD\nworld\n====\nthere\n>>>> NEW")),
                    ]),
                ),
                ("canonical_params", canonical_edit()),
            ])]),
        ),
    ])
}

/// The E4 suite for `minimal-string-replace` — `old_string`/`new_string`
/// project into `edits[0]`.
fn string_replace_suite() -> Json {
    Json::obj([
        ("suite_id", Json::str("e4:edit_file")),
        ("capability", Json::str("edit_file")),
        ("world", Json::str("fs_map")),
        ("initial", Json::obj([("a.txt", Json::str("hello world"))])),
        (
            "cases",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("replace")),
                (
                    "surface_args",
                    Json::obj([
                        ("path", Json::str("a.txt")),
                        ("old_string", Json::str("world")),
                        ("new_string", Json::str("there")),
                    ]),
                ),
                ("canonical_params", canonical_edit()),
            ])]),
        ),
    ])
}

/// Attach `suite` to the profile's `tests` and recompute the content hash
/// (the bound view keys on both the coordinate and the hash).
fn with_suite(profile: &ModelProfile, suite: Json) -> ModelProfile {
    let mut p = profile.clone();
    p.tests = Json::obj([("e4_suites", Json::Arr(vec![suite]))]);
    p.content_hash = profile_identity(&p);
    p
}

fn inputs_multi(
    sealed: &hh_hir::SealedDefinition,
    profile: &ModelProfile,
    targets: Vec<&str>,
) -> CompileInputs {
    CompileInputs {
        sealed: sealed.clone(),
        profile_refs: vec![profile_coordinate(profile)],
        fallback_profile: None,
        targets: targets
            .into_iter()
            .map(|t| hh_compiler::link::TargetSpec {
                target_id: t.to_string(),
                spec_version: "1.0".to_string(),
                content_hash: format!("sha256:target-{t}"),
            })
            .collect(),
        compile_for_expired: false,
    }
}

fn compile_with(
    sealed: &hh_hir::SealedDefinition,
    store: &hh_registry::store::RegistryStore,
    profile: &ModelProfile,
    targets: Vec<&str>,
) -> hh_compiler::seal::CompiledBundle {
    let profiles = MapProfileView::of(vec![profile.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    compile(
        &inputs_multi(sealed, profile, targets),
        &profiles,
        store,
        &cat,
        &kernel(),
    )
    .expect("compile")
}

fn lowered(b: &hh_compiler::seal::CompiledBundle) -> &hh_compiler::seal::ModelSurface {
    match &b.model_surface {
        ModelSurfaceState::Lowered(s) => s,
        _ => panic!("stage 3 lowers the surface"),
    }
}

// ── T-LCD-04 / AC-CP-07 — two profiles × two targets ─────────────────────────

#[test]
fn t_lcd_04_one_definition_two_profiles_two_targets() {
    let (store, sealed, _v) = sealed_doc_with("t04", reference_doc(unbound()), vec![], vec![]);
    let (patch, string_replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let string_replace = with_suite(&string_replace, string_replace_suite());
    for (profile, targets) in [
        (&patch, vec!["provider_tool_api", "mcp"]),
        (&string_replace, vec!["provider_tool_api", "mcp"]),
    ] {
        let bundle = compile_with(&sealed, &store, profile, targets);
        assert!(bundle.target_artefacts.contains_key("provider_tool_api"));
        assert!(bundle.target_artefacts.contains_key("mcp"));
        assert_eq!(bundle.loss_reports.len(), 2, "one loss report per target");
        let surface = lowered(&bundle);
        // Every surface element carries rule_ids (§3.2.8 — traceability).
        for t in &surface.tools {
            assert!(
                !t.binding.rule_ids.is_empty(),
                "rule_ids on {}",
                t.binding.surface_name
            );
            assert!(t.binding.evidence_ref.is_some(), "evidence stamped");
        }
    }
    // AC-CP-07: the permission/effect/budget tables are identical across targets
    // and profiles — they derive from the shared `RuntimePlan`.
    let a = compile_with(&sealed, &store, &patch, vec!["provider_tool_api"]);
    let b = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let c = compile_with(&sealed, &store, &string_replace, vec!["mcp"]);
    assert_eq!(a.runtime_plan, b.runtime_plan, "same plan across targets");
    assert_eq!(a.runtime_plan, c.runtime_plan, "same plan across profiles");
    // But the surfaces differ — the profile owns the difference.
    let sa = lowered(&a);
    let sc = lowered(&c);
    assert_ne!(
        sa.tools, sc.tools,
        "the two minimal profiles shape surfaces"
    );
    let names_a: Vec<&str> = sa
        .tools
        .iter()
        .map(|t| t.binding.surface_name.as_str())
        .collect();
    let names_c: Vec<&str> = sc
        .tools
        .iter()
        .map(|t| t.binding.surface_name.as_str())
        .collect();
    assert!(
        names_a.iter().any(|n| n.starts_with("patch__")),
        "{names_a:?}"
    );
    assert!(
        names_c.iter().any(|n| n.ends_with("__replace")),
        "{names_c:?}"
    );
}

// ── AC-CP-02 (T-LCD-01) — the profile diff stays inside owned fields ─────────

#[test]
fn ac_cp_02_surface_diff_stays_within_owned_fields() {
    let (store, sealed, _v) = sealed_doc_with("cp02", reference_doc(unbound()), vec![], vec![]);
    let (patch, string_replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let string_replace = with_suite(&string_replace, string_replace_suite());
    let a = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let b = compile_with(&sealed, &store, &string_replace, vec!["mcp"]);
    let diff = equiv::surface_diff(lowered(&a), lowered(&b));
    assert!(!diff.is_empty(), "the minimal profiles do differ");
    // The union of fields owned by the two profiles' rules.
    let owned: std::collections::BTreeSet<String> = [&patch, &string_replace]
        .iter()
        .flat_map(|p| hh_compiler::profile::owned_fields(p))
        .collect();
    for d in &diff {
        let ok = owned.iter().any(|o| {
            // `tools/*/name`-style globs — `*` matches one path segment.
            let ds: Vec<&str> = d.split('/').collect();
            let os: Vec<&str> = o.split('/').collect();
            ds.len() == os.len() && ds.iter().zip(os.iter()).all(|(d, o)| *o == "*" || d == o)
        });
        assert!(
            ok,
            "diff field {d} is outside the owned-field union {owned:?}"
        );
    }
}

// ── T-LCD-11 / AC-CP-04 — lift(lower(x)) ∪ loss_report = x ───────────────────

#[test]
fn t_lcd_11_mcp_lift_round_trip() {
    let (store, sealed, _v) = sealed_doc_with("t11", reference_doc(unbound()), vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let artefact = &bundle.target_artefacts["mcp"];
    let lifted = lift(artefact, "mcp").expect("mcp lifts");
    let sids: Vec<&str> = lifted
        .recovered
        .iter()
        .filter_map(|r| r.get("semantic_id").and_then(Json::as_str))
        .collect();
    assert!(sids.contains(&"test:edit_file"));
    assert!(sids.contains(&"test:get_file"));
    let report = &bundle.loss_reports[0];
    assert_eq!(report.target, "mcp");
    assert!(report
        .entries
        .iter()
        .any(|e| e.class == LossKind::NoSlot && e.field == "budgets"));
    assert!(report
        .entries
        .iter()
        .any(|e| e.field == "permission.enforcement"));
    // The artefact's `_meta` carries the lifted minimum.
    let tool_meta = artefact
        .get("tools")
        .and_then(|t| match t {
            Json::Arr(a) => a.first(),
            _ => None,
        })
        .and_then(|t| t.get("_meta"))
        .and_then(|m| m.get(HH_META_KEY))
        .expect("_meta carried");
    assert!(tool_meta.get("semantic_id").is_some());
    assert!(tool_meta.get("surface_id").is_some());
    assert!(tool_meta.get("permission_class").is_some());
}

#[test]
fn t_lcd_11_provider_target_has_no_lift_but_declared_losses() {
    let (store, sealed, _v) = sealed_doc_with("t11p", reference_doc(unbound()), vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["provider_tool_api"]);
    // ADR-0021 D5 — consumed by the model: `lift` is a typed refusal.
    let err = lift(
        &bundle.target_artefacts["provider_tool_api"],
        "provider_tool_api",
    )
    .expect_err("no lifting contract");
    assert!(matches!(err, CompileError::TargetError { .. }));
    let report = bundle
        .loss_reports
        .iter()
        .find(|r| r.target == "provider_tool_api")
        .expect("the provider loss report");
    assert_eq!(
        report.granularity_ceiling,
        hh_compiler::lcd::GranularityCeiling::Configuration
    );
}

#[test]
fn provider_out_of_subset_keywords_are_typed_losses() {
    // A schema keyword outside the provider strict subset is dropped *and
    // declared* — the `narrowed` loss entry discharges T-LCD-11 on the
    // provider path (under `prefer`; under `require` it refuses).
    let mut doc = reference_doc(unbound());
    for n in doc.nodes.iter_mut() {
        if n.semantic_id() == "test:get_file" {
            if let KindRecord::ToolCapability(t) = &mut n.semantic {
                t.input_schema = Json::obj([(
                    "properties",
                    Json::obj([(
                        "path",
                        Json::obj([
                            ("deprecated", Json::Bool(true)),
                            ("type", Json::str("string")),
                        ]),
                    )]),
                )]);
            }
        }
    }
    let (store, sealed, _v) = sealed_doc_with("t11x", doc, vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["provider_tool_api"]);
    let report = bundle
        .loss_reports
        .iter()
        .find(|r| r.target == "provider_tool_api")
        .unwrap();
    assert!(report
        .entries
        .iter()
        .any(|e| e.class == LossKind::Narrowed && e.field.contains("deprecated")));
}

#[test]
fn lift_recovers_every_carried_field_the_report_does_not_declare_lost() {
    let (store, sealed, _v) = sealed_doc_with("t11u", reference_doc(unbound()), vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let lifted = lift(&bundle.target_artefacts["mcp"], "mcp").unwrap();
    let recovered: std::collections::BTreeSet<String> = lifted
        .recovered
        .iter()
        .filter_map(|r| {
            r.get("semantic_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    let lost: std::collections::BTreeSet<String> = bundle.loss_reports[0]
        .entries
        .iter()
        .map(|e| e.hir_node_id.clone())
        .collect();
    for tool in &bundle.runtime_plan.tools {
        assert!(
            recovered.contains(&tool.capability.semantic_id)
                || lost.contains(&tool.capability.semantic_id),
            "lift(lower(x)) ∪ loss_report = x for {}",
            tool.capability.semantic_id
        );
    }
}

// ── T-LCD-03 / AC-IR-08 / AC-CP-08 — the reference harness ───────────────────

#[test]
fn t_lcd_03_reference_harness_compiles_and_reports_opacity() {
    let (store, sealed, _v) = sealed_doc_with("t03", reference_doc(unbound()), vec![], vec![]);
    // AC-IR-03 (T-02): `opacity` reports the reference definition's leaves.
    let report = hh_hir::ops::opacity(&sealed.document, None);
    assert!(
        report.by_class.total() > 0,
        "the reference definition has leaves"
    );
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["mcp", "provider_tool_api"]);
    // The round-trip's executable slice: every capability's identity survives
    // lower → lift through the MCP artefact.
    let lifted = lift(&bundle.target_artefacts["mcp"], "mcp").unwrap();
    assert_eq!(lifted.recovered.len(), 2);
    assert!(lifted.unknown.is_empty());
    assert!(lifted.declared_unverified.is_empty());
    // The behavioural margin (paired runs, pre-registered) is §10's obligation —
    // the deterministic equivalence evidence here is the compiled bundle's.
    assert_eq!(bundle.equivalence_evidence.len(), 2);
}

// ── AC-CP-12 / T-LCD-02 — the bundle's opacity equals validation's ───────────

#[test]
fn ac_cp_12_bundle_opacity_report_equals_validation() {
    let (store, sealed, _v) = sealed_doc_with("cp12", reference_doc(unbound()), vec![], vec![]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let validation = hh_assembly::validate_assembly(
        hh_assembly::Subject::Sealed(&sealed),
        &cat,
        None,
        &kernel(),
    );
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    // CF-050: the bundle carries the *composed* value — equal to the report's.
    assert_eq!(
        bundle
            .opacity_report
            .as_ref()
            .map(|o| (o.opaque_ratio_num, o.total)),
        validation
            .derived
            .opacity
            .as_ref()
            .map(|o| (o.opaque_ratio_num, o.total)),
        "the bundle's opacity ratio equals the ValidationReport's"
    );
}

// ── ablate — provenance-preserving removal (§3.1.6 / §3.2 helpers) ───────────

#[test]
fn ablate_removes_a_node_and_stamps_lineage() {
    let mut doc = reference_doc(unbound());
    doc.nodes.push(tool_node("test:spare", 8)); // unreferenced — ablates cleanly
    let (_store, sealed, _v) = sealed_doc_with("ablate", doc, vec![], vec![]);
    // Ablating a node with dependents is a typed refusal, never a silent
    // dangling ref.
    let errs = hh_hir::ops::ablate(&sealed, "test:get_file", 99)
        .expect_err("the procedure still references get_file");
    assert!(errs
        .iter()
        .any(|e| matches!(e, hh_hir::HirError::UnresolvedRef { .. })));
    // An unreferenced node ablates cleanly; the derivation is *recorded*.
    let before = sealed.document.nodes.len();
    let out = hh_hir::ops::ablate(&sealed, "test:spare", 99).expect("ablate");
    assert_eq!(out.document.nodes.len(), before - 1);
    assert!(out.document.node("test:spare").is_none());
    assert!(out
        .document
        .edges
        .iter()
        .any(|e| e.kind == hh_hir::kinds::EdgeKind::DerivedFrom));
}

// ── relower — stages 3–5 under a new profile (§3.2.2 Re-lowering) ────────────

#[test]
fn relower_issues_a_new_bundle_id_and_keeps_the_plan() {
    let (store, sealed, _v) = sealed_doc_with("relower", reference_doc(unbound()), vec![], vec![]);
    let (patch, string_replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let string_replace = with_suite(&string_replace, string_replace_suite());
    let old = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let inputs = inputs_multi(&sealed, &string_replace, vec!["mcp"]);
    let profiles = MapProfileView::of(vec![string_replace.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let (new, migration) = relower(
        &old,
        &inputs,
        &profiles,
        &store,
        &cat,
        &kernel(),
        "profile_swap",
    )
    .expect("relower");
    assert_ne!(old.bundle_id, new.bundle_id, "a new bundle id is issued");
    assert_eq!(old.runtime_plan, new.runtime_plan, "the plan is unchanged");
    assert_eq!(migration.from_bundle, old.bundle_id);
    assert_eq!(migration.to_bundle, new.bundle_id);
    assert!(!migration.rewritten_items.is_empty());
    let ev = hh_compiler::relowered_event(&migration);
    assert_eq!(
        ev.get("reason").and_then(Json::as_str),
        Some("profile_swap")
    );
    assert_eq!(
        ev.get("new_bundle_id").and_then(Json::as_str),
        Some(new.bundle_id.as_str())
    );
}

/// The E4 suite for an *unshaped* (identity) `edit_file` surface — the retire
/// arm's surface args equal the canonical params.
fn identity_suite() -> Json {
    Json::obj([
        ("suite_id", Json::str("e4:edit_file")),
        ("capability", Json::str("edit_file")),
        ("world", Json::str("fs_map")),
        ("initial", Json::obj([("a.txt", Json::str("hello world"))])),
        (
            "cases",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("replace")),
                ("surface_args", canonical_edit()),
                ("canonical_params", canonical_edit()),
            ])]),
        ),
    ])
}

#[test]
fn relower_retire_arm_and_artefact_determinism() {
    let (store, sealed, _v) = sealed_doc_with("relower2", reference_doc(unbound()), vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let old = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    // AC-CP-13 — the compile is deterministic: identical inputs produce
    // byte-identical artefacts and the same bundle id.
    let again = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    assert_eq!(old.target_artefacts, again.target_artefacts);
    assert_eq!(old.bundle_id, again.bundle_id);
    // AC-CP-06's retire arm: `relower` under `profile − tool_shape` compiles —
    // the unshaped surface is valid (the E4 suite follows the surface's shape).
    let mut retired = patch.clone();
    retired
        .rules
        .retain(|r| r.kind != hh_compiler::profile::ProfileRuleKind::ToolShape);
    retired.tests = Json::obj([("e4_suites", Json::Arr(vec![identity_suite()]))]);
    retired.content_hash = profile_identity(&retired);
    let inputs = inputs_multi(&sealed, &retired, vec!["mcp"]);
    let profiles = MapProfileView::of(vec![retired.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let (new, migration) = relower(
        &old,
        &inputs,
        &profiles,
        &store,
        &cat,
        &kernel(),
        "rule_retired",
    )
    .expect("the retire arm compiles");
    assert_eq!(old.runtime_plan, new.runtime_plan);
    assert_eq!(migration.reason, "rule_retired");
    // The unshaped surface: `edit_file`'s arg map is the identity projection.
    let edit = lowered(&new)
        .tools
        .iter()
        .find(|t| t.binding.hir_node_id == "test:edit_file")
        .unwrap();
    assert!(edit
        .binding
        .arg_map
        .values()
        .all(|e| e.transform == equiv::ArgTransform::Identity));
}

// ── E4 — the executable differential (§3.2.6) ────────────────────────────────

#[test]
fn e4_runs_the_declared_suite_and_passes() {
    let (store, sealed, _v) = sealed_doc_with("e4", reference_doc(unbound()), vec![], vec![]);
    // §3.2.6 rule i — `edit_file` is a named closed-world primitive: without a
    // declared suite the compile refuses.
    let (patch, _) = minimal_profiles();
    let profiles = MapProfileView::of(vec![patch.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let err = compile(
        &inputs_multi(&sealed, &patch, vec!["mcp"]),
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect_err("no declared suite is a fail, not an n/a");
    assert!(
        matches!(err, CompileError::UnexpressibleSurface { .. }),
        "{err:?}"
    );
    // With the suite declared, both profiles' surfaces pass the differential.
    let patch = with_suite(&patch, patch_suite());
    let (_, string_replace) = minimal_profiles();
    let string_replace = with_suite(&string_replace, string_replace_suite());
    for p in [&patch, &string_replace] {
        let bundle = compile_with(&sealed, &store, p, vec!["mcp"]);
        let ev = bundle
            .equivalence_evidence
            .iter()
            .find(|e| e.capability == "test:edit_file")
            .expect("edit_file evidence");
        assert_eq!(
            ev.e4_differential,
            EvidenceVerdict::Pass,
            "profile {}",
            p.profile_id
        );
    }
}

#[test]
fn e4_a_diverging_case_fails_the_compile() {
    let (store, sealed, _v) = sealed_doc_with("e4b", reference_doc(unbound()), vec![], vec![]);
    // A suite whose surface form resolves to different canonical parameters.
    let suite = Json::obj([
        ("suite_id", Json::str("e4:edit_file")),
        ("capability", Json::str("edit_file")),
        ("world", Json::str("fs_map")),
        ("initial", Json::obj([("a.txt", Json::str("hello world"))])),
        (
            "cases",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("diverging")),
                (
                    "surface_args",
                    Json::obj([
                        ("path", Json::str("a.txt")),
                        ("old_string", Json::str("hello")),
                        ("new_string", Json::str("bye")),
                    ]),
                ),
                ("canonical_params", canonical_edit()),
            ])]),
        ),
    ]);
    let (_, string_replace) = minimal_profiles();
    let p = with_suite(&string_replace, suite);
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let err = compile(
        &inputs_multi(&sealed, &p, vec!["mcp"]),
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect_err("a diverging E4 case refuses the surface");
    assert!(
        matches!(err, CompileError::UnexpressibleSurface { .. }),
        "{err:?}"
    );
}

// ── E5/E6 — error surjectivity and result observation (§3.2.6) ───────────────

#[test]
fn e5_declared_error_classes_need_a_covering_spec() {
    // A capability declaring error classes needs an `error_format` rule whose
    // renderings cover them distinguishably.
    let mut doc = reference_doc(unbound());
    for n in doc.nodes.iter_mut() {
        if n.semantic_id() == "test:edit_file" {
            if let KindRecord::ToolCapability(t) = &mut n.semantic {
                t.observation_contract = Json::obj([(
                    "error_classes",
                    Json::Arr(vec![Json::str("denied"), Json::str("not_found")]),
                )]);
            }
        }
    }
    let (store, sealed, _v) = sealed_doc_with("e56", doc, vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let profiles = MapProfileView::of(vec![patch.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let err = compile(
        &inputs_multi(&sealed, &patch, vec!["mcp"]),
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect_err("declared error classes without a spec refuse");
    assert!(
        matches!(err, CompileError::UnexpressibleSurface { .. }),
        "{err:?}"
    );

    // Add the error_format rule covering both classes.
    let mut p2 = patch.clone();
    p2.rules.push(rule(
        "r-err",
        hh_compiler::profile::ProfileRuleKind::ErrorFormat,
        &["tools/*/error_format"],
        Json::obj([(
            "renderings",
            Json::obj([
                ("denied", Json::str("EACCES")),
                ("not_found", Json::str("ENOENT")),
            ]),
        )]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p2.content_hash = profile_identity(&p2);
    let profiles2 = MapProfileView::of(vec![p2.clone()]);
    let b = compile(
        &inputs_multi(&sealed, &p2, vec!["mcp"]),
        &profiles2,
        &store,
        &cat,
        &kernel(),
    )
    .expect("the covering spec compiles");
    let ev = b
        .equivalence_evidence
        .iter()
        .find(|e| e.capability == "test:edit_file")
        .unwrap();
    assert_eq!(ev.e5_error_surjectivity, EvidenceVerdict::Pass);
}

// ── loss reports — granularity ceiling + every dropped field typed ───────────

#[test]
fn ac_cp_04_loss_report_shape_and_granularity_ceiling() {
    let (store, sealed, _v) = sealed_doc_with("loss", reference_doc(unbound()), vec![], vec![]);
    let (patch, _) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let bundle = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let report = &bundle.loss_reports[0];
    assert_eq!(report.target_version, "1.0");
    // MCP carries the minimum set in `_meta` — the ceiling is `component`.
    assert_eq!(
        report.granularity_ceiling,
        hh_compiler::lcd::GranularityCeiling::Component
    );
    for e in &report.entries {
        assert!(matches!(
            e.class,
            LossKind::NoSlot
                | LossKind::HintOnly
                | LossKind::UntypedSlot
                | LossKind::Narrowed
                | LossKind::Truncated
        ));
        assert!(!e.hir_node_id.is_empty());
    }
}

// ── AC-R-2.4.5-4 — two profiles, two targets (§5c.5; S3.8) ───────────────────
// The same Procedure compiles to `instruction` under two profiles with diffs
// ⊆ profile-owned fields, and to `workflow_node` under a definition whose
// control strategy declares `workflow_execution`; the permission/effect/
// budget tables are identical across the bundles.

#[test]
fn ac_r_2_4_5_4_two_profiles_two_targets() {
    let (store, sealed, _v) = sealed_doc_with("s454", reference_doc(unbound()), vec![], vec![]);
    let (patch, string_replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let string_replace = with_suite(&string_replace, string_replace_suite());

    // instruction under both profiles — the compile-time product differs
    // only in the profile-owned `procedure_render` prefix.
    let proc = sealed
        .document
        .nodes
        .iter()
        .find(|n| n.semantic_id() == "test:proc")
        .expect("the reference doc carries test:proc");
    let d1 = hh_hir::procedure::select_target(
        proc,
        None,
        &hh_hir::procedure::SelectCtx::default(),
        &std::collections::BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(d1.target, hh_hir::procedure::CompilationTarget::Instruction);
    let p1 = hh_compiler::plan::lower_procedure(
        &sealed.document,
        proc,
        hh_hir::procedure::CompilationTarget::Instruction,
        Some("profile:minimal-patch"),
    )
    .unwrap();
    let p2 = hh_compiler::plan::lower_procedure(
        &sealed.document,
        proc,
        hh_hir::procedure::CompilationTarget::Instruction,
        Some("profile:minimal-string-replace"),
    )
    .unwrap();
    let (
        hh_compiler::plan::ProcedureProduct::Instruction { body: b1 },
        hh_compiler::plan::ProcedureProduct::Instruction { body: b2 },
    ) = (p1, p2)
    else {
        panic!("instruction product")
    };
    // The diff is exactly the profile-owned prefix — the typed step listing
    // is identical.
    let strip = |b: &str| b.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(strip(&b1), strip(&b2), "diffs ⊆ profile-owned fields");
    assert_ne!(b1, b2, "the profile prefix differs");

    // workflow_node under a `workflow_execution` control strategy — the
    // two Invoke steps lower to `step` plan nodes.
    let wf_ctx = hh_hir::procedure::SelectCtx {
        workflow_execution_declared: true,
        ..Default::default()
    };
    let d2 =
        hh_hir::procedure::select_target(proc, None, &wf_ctx, &std::collections::BTreeMap::new())
            .unwrap();
    assert_eq!(
        d2.target,
        hh_hir::procedure::CompilationTarget::WorkflowNode
    );
    let wf = hh_compiler::plan::lower_procedure(
        &sealed.document,
        proc,
        hh_hir::procedure::CompilationTarget::WorkflowNode,
        None,
    )
    .unwrap();
    let hh_compiler::plan::ProcedureProduct::WorkflowNode { nodes } = wf else {
        panic!("workflow product")
    };
    assert_eq!(nodes.len(), 2, "two Invoke steps → two step nodes");
    assert!(nodes
        .iter()
        .all(|n| matches!(n.payload, hh_compiler::plan::PlanNodePayload::Step(_))));

    // Permission/effect/budget tables identical across the two-profile
    // bundles — they derive from the shared RuntimePlan.
    let a = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    let b = compile_with(&sealed, &store, &string_replace, vec!["mcp"]);
    assert_eq!(
        a.runtime_plan.policies, b.runtime_plan.policies,
        "permission/effect/budget tables identical across profiles"
    );
    // And the procedure's plan nodes are the same nodes lower_procedure
    // emits under the workflow target.
    assert!(a
        .runtime_plan
        .control
        .iter()
        .any(|n| n.hir_node_id == "test:proc"));
}

/// `subagent_task` never lowers — `DelegationUnavailable` propagates as a
/// typed `PlanError`, never a silent instruction fallback.
#[test]
fn ac_r_2_4_5_4_subagent_target_never_falls_back() {
    let (_store, sealed, _v) = sealed_doc_with("s454b", reference_doc(unbound()), vec![], vec![]);
    let proc = sealed
        .document
        .nodes
        .iter()
        .find(|n| n.semantic_id() == "test:proc")
        .unwrap();
    assert!(matches!(
        hh_compiler::plan::lower_procedure(
            &sealed.document,
            proc,
            hh_hir::procedure::CompilationTarget::SubagentTask,
            None,
        ),
        Err(hh_compiler::errors::CompileError::PlanError { .. })
    ));
}

/// AC-R-2.4.1-13 — a mid-run profile switch re-renders the same plan;
/// profile-opaque items land in `model.surface.relowered.dropped_items[]`
/// with typed placeholders; `plan_id` unchanged (the runtime plan is equal).
#[test]
fn ac_r_2_4_1_13_relower_drops_stale_signature_and_keeps_plan() {
    use hh_compiler::profile::ProfileRuleKind;
    let (store, sealed, _v) =
        sealed_doc_with("relower13", reference_doc(unbound()), vec![], vec![]);
    let (patch, string_replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let old = compile_with(&sealed, &store, &patch, vec!["mcp"]);
    // The switched-to profile renders the transcript with
    // `stale_signature = drop` — the old profile's rendered items are
    // profile-opaque and land in `dropped_items`, never silently kept.
    let mut dropped = with_suite(&string_replace, string_replace_suite());
    let mut tr = dropped.rules[0].clone();
    tr.rule_id = "minimal-string-replace.transcript_render".to_string();
    tr.kind = ProfileRuleKind::TranscriptRender;
    tr.owned_fields = vec!["transcript_render/stale_signature".to_string()];
    tr.params = Json::obj([("stale_signature", Json::str("drop"))]);
    dropped.rules.push(tr);
    dropped.content_hash = profile_identity(&dropped);
    let inputs = inputs_multi(&sealed, &dropped, vec!["mcp"]);
    let profiles = MapProfileView::of(vec![dropped.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let (new, migration) = relower(
        &old,
        &inputs,
        &profiles,
        &store,
        &cat,
        &kernel(),
        "profile_swap",
    )
    .expect("relower compiles under the drop signature");
    // The plan is unchanged — stages 3–5 only re-ran.
    assert_eq!(old.runtime_plan, new.runtime_plan);
    assert!(migration
        .dropped_items
        .iter()
        .any(|d| d.get("kind").and_then(Json::as_str) == Some("stale_signature")));
    // The event carries the dropped items under
    // `model.surface.relowered.dropped_items[]`.
    let ev = hh_compiler::relowered_event(&migration);
    match ev.get("dropped_items") {
        Some(Json::Arr(items)) => assert!(items
            .iter()
            .any(|d| d.get("kind").and_then(Json::as_str) == Some("stale_signature"))),
        other => panic!("dropped_items must be an array, got {other:?}"),
    }
}
