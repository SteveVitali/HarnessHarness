//! AC-R-2.5.2-{1,2} and AC-R-2.5.3-{1,2,3,12} — the C0 surface-schema and
//! exposure slices (§5d.2/§5d.3; S1.17): the full `SurfaceBinding` member set
//! with canonical round-trip, `capability_refs` stability under rename, the
//! `direct_all`/`check_callable`/`SurfaceNotRevealed` kernel, I-DISCOVERY,
//! I-NARROW `PolicyViolation`, and `hidden` exclusion.
//!
//! AC-R-2.5.2-12's declaration half lives in `hh-telemetry` (the
//! `surface_rejection_rate` catalogue row).

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::exposure::{
    build_catalog, check_callable, direct_all, evict, index_query, reveal, select_surfaces,
    CallProposal, CallRefusal, CatalogIndex, DiscoveryForm, DiscoveryQuery, EvictCause,
    ExposurePolicy, ExposurePolicyParams, OmitReason, Retention, RevealCause, RevealedSet,
    SelectError, TurnState,
};
use hh_compiler::schema::{bundle_from_json, bundle_to_json};
use hh_compiler::CompileInputs;
use hh_hir::records::{KindRecord, SurfaceRecord};
use hh_hir::tools::ExposureMode;
use hh_wire::json::Json;

fn compile_tool_doc(
    tag: &str,
    name: &str,
    exposure_mode: Json,
) -> (
    hh_registry::store::RegistryStore,
    hh_compiler::seal::CompiledBundle,
) {
    compile_tool_doc_with(tag, name, exposure_mode, false)
}

/// `with_discovery` adds a `discover_surfaces` capability node — required by
/// I-DISCOVERY (validate) whenever a surface admits `deferred`/`indexed`.
fn compile_tool_doc_with(
    tag: &str,
    name: &str,
    exposure_mode: Json,
    with_discovery: bool,
) -> (
    hh_registry::store::RegistryStore,
    hh_compiler::seal::CompiledBundle,
) {
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", name, &["path"], 5);
    if let Some(SurfaceRecord::Tool(ts)) = t.surface.as_mut() {
        ts.exposure_mode = exposure_mode;
    }
    doc.nodes.push(t);
    if with_discovery {
        let mut d = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 6);
        if let KindRecord::ToolCapability(tc) = &mut d.semantic {
            tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
        }
        doc.nodes.push(d);
    }
    let (store, sealed, _vids) = sealed_doc_with(tag, doc, vec![], vec![]);
    let p = test_profile();
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![hh_compiler::profile::profile_coordinate(&p)],
            fallback_profile: None,
            targets: vec![mcp_target()],
            compile_for_expired: false,
        },
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect("compile");
    (store, bundle)
}

fn turn_state(model_call_id: &str) -> TurnState {
    TurnState {
        model_call_id: model_call_id.into(),
        revealed: RevealedSet {
            run_id: "run:test".into(),
            entries: vec![],
        },
        recent_calls: vec![],
        goal_ref: None,
        window_cap: 1_000_000,
        prior_order: vec![],
    }
}

fn all_modes() -> BTreeSet<ExposureMode> {
    ExposureMode::ALL.iter().copied().collect()
}

// ── AC-R-2.5.2-2 — 100 % of surfaces bound; rename never touches refs ────────

#[test]
fn ac_e2_2_every_surface_bound_and_rename_changes_no_refs() {
    let (_s, bundle) = compile_tool_doc("e2-2", "tool_a", Json::Null);
    assert_eq!(bundle.runtime_plan.tools.len(), 1);
    let b = bundle.runtime_plan.tools[0]
        .surface
        .as_ref()
        .expect("every surface has a SurfaceBinding");
    // The §5d.2 member set is populated.
    assert_eq!(b.surface_name, "tool_a");
    assert!(!b.surface_id.is_empty());
    assert_eq!(
        b.exposure_mode,
        hh_compiler::surface::CompileExposureMode::Primitive
    );
    assert_eq!(
        b.capability_refs,
        vec![b.capability_ref.semantic_id.clone()]
    );
    assert_eq!(b.hir_node_id, b.capability_ref.semantic_id);
    assert_eq!(
        b.mapping,
        hh_compiler::surface::BindingMapping::SurfaceArgMap
    );
    assert!(b.rule_ids.is_empty());
    assert_eq!(b.family_id, None);
    assert_eq!(b.variant_id, None);
    // Canonical round-trip preserves the record verbatim.
    let j = bundle_to_json(&bundle);
    let bundle2 = bundle_from_json(&j).expect("bundle_from_json");
    let b2 = bundle2.runtime_plan.tools[0].surface.as_ref().unwrap();
    assert_eq!(b, b2, "SurfaceBinding survives canonical bytes");

    // A rename re-mints `surface_id` but changes no `capability_refs` member
    // and no other `configuration_id` component (the profile hash aside).
    let (_s2, bundle_r) = compile_tool_doc("e2-2r", "tool_a_renamed", Json::Null);
    let br = bundle_r.runtime_plan.tools[0].surface.as_ref().unwrap();
    assert_eq!(
        br.capability_refs, b.capability_refs,
        "capability_refs stable"
    );
    // `capability_ref` is the *version* coordinate — a rename re-mints the
    // node's `version_id` (surface is version-level) while `semantic_id` —
    // the member `capability_refs` carries — is untouched (E7).
    assert_eq!(br.capability_ref.semantic_id, b.capability_ref.semantic_id);
    assert_ne!(br.surface_id, b.surface_id, "rename re-mints the surface");
}

#[test]
fn ac_e2_1_binding_members_are_closed_sums() {
    let (_s, bundle) = compile_tool_doc("e2-1", "tool_a", Json::Null);
    // The canonical body spells every spec member.
    let j = bundle_to_json(&bundle);
    let binding_json = j
        .get("runtime_plan")
        .and_then(|p| p.get("tools"))
        .and_then(|t| match t {
            Json::Arr(ts) => ts.first(),
            _ => None,
        })
        .and_then(|t| t.get("surface"))
        .expect("surface binding in bundle json");
    for member in [
        "surface_id",
        "exposure_mode",
        "capability_refs",
        "mapping",
        "rule_ids",
        "evidence_ref",
        "safety_ref",
        "effects_bound",
        "family_id",
        "variant_id",
    ] {
        assert!(
            binding_json.get(member).is_some(),
            "missing member {member}"
        );
    }
    // `exposure_mode`/`mapping` are closed sums — unknown spellings refuse.
    assert_eq!(
        binding_json.get("exposure_mode").and_then(Json::as_str),
        Some("primitive")
    );
    assert_eq!(
        binding_json
            .get("mapping")
            .and_then(|m| m.get("kind"))
            .and_then(Json::as_str),
        Some("surface_arg_map")
    );
}

// ── AC-R-2.5.3-1 — SurfaceNotRevealed ────────────────────────────────────────

#[test]
fn ac_e3_1_deferred_surface_is_not_callable() {
    let (_s, bundle) = compile_tool_doc_with(
        "e3-1",
        "tool_a",
        Json::obj([(
            "admitted_modes",
            Json::Arr(vec![Json::str("direct"), Json::str("deferred")]),
        )]),
        true,
    );
    let catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    // A policy that defers the surfaced tool and keeps the discovery
    // capability `direct` (I-DISCOVERY is satisfied; the refusal under test
    // is `SurfaceNotRevealed`, not `NoDiscoverySurface`).
    struct Defer;
    impl ExposurePolicy for Defer {
        fn rank(
            &self,
            entries: &[hh_compiler::exposure::CatalogEntry],
            _a: &BTreeMap<String, BTreeSet<ExposureMode>>,
            _t: &TurnState,
            _p: &ExposurePolicyParams,
        ) -> Vec<(String, ExposureMode)> {
            entries
                .iter()
                .map(|e| {
                    (
                        e.surface_id.clone(),
                        if e.is_discovery {
                            ExposureMode::Direct
                        } else {
                            ExposureMode::Deferred
                        },
                    )
                })
                .collect()
        }
    }

    let plan = select_surfaces(
        &catalog,
        &turn_state("mc:1"),
        &all_modes(),
        &ExposurePolicyParams::default(),
        Some(&Defer),
    )
    .expect("select");
    // The deferred surface is in the plan, not direct.
    let entry = catalog.entries.iter().find(|e| e.name == "tool_a").unwrap();
    assert_eq!(
        plan.entries
            .iter()
            .find(|(s, _)| *s == entry.surface_id)
            .map(|(_, m)| *m),
        Some(ExposureMode::Deferred)
    );
    // `check_callable` refuses — the payload carries no schema/description.
    let refusal = check_callable(
        &plan,
        &catalog,
        &CallProposal {
            surface_name: "tool_a".into(),
            from_code: false,
        },
    )
    .expect_err("deferred is not callable");
    match refusal {
        CallRefusal::SurfaceNotRevealed { surface_id, hint } => {
            assert_eq!(surface_id.as_deref(), Some(entry.surface_id.as_str()));
            assert_eq!(hint, hh_compiler::exposure::RevealHint::Search);
        }
        other => panic!("expected SurfaceNotRevealed, got {other:?}"),
    }
    // An unknown name refuses the same way — never a definition leak.
    let refusal = check_callable(
        &plan,
        &catalog,
        &CallProposal {
            surface_name: "no_such".into(),
            from_code: false,
        },
    )
    .expect_err("unknown name");
    assert!(matches!(
        refusal,
        CallRefusal::SurfaceNotRevealed {
            surface_id: None,
            hint: hh_compiler::exposure::RevealHint::None
        }
    ));
}

// ── AC-R-2.5.3-2 — I-DISCOVERY ────────────────────────────────────────────────

#[test]
fn ac_e3_2_deferred_without_discovery_fails() {
    // The validate half: a definition admitting `deferred` without a bound
    // discovery capability is a `NoDiscoverySurface` diagnostic.
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    if let Some(SurfaceRecord::Tool(ts)) = t.surface.as_mut() {
        ts.exposure_mode = Json::obj([(
            "admitted_modes",
            Json::Arr(vec![Json::str("direct"), Json::str("deferred")]),
        )]);
    }
    doc.nodes.push(t);
    let errs = hh_hir::validate::validate(&doc).expect_err("validate must refuse");
    assert!(errs.contains(&hh_hir::errors::HirError::NoDiscoverySurface));

    // The run half: a plan that defers with no `direct` discovery surface is
    // `NoDiscoverySurface`, never a silent demotion. The discovery capability
    // exists (validate passes) but admits `deferred` only — the policy
    // selects `deferred` for everything inside every meet.
    let mut doc = doc_with(&stage1_assembly());
    let mut t = surfaced_tool_node("test:tool", "tool_a", &["path"], 5);
    if let Some(SurfaceRecord::Tool(ts)) = t.surface.as_mut() {
        ts.exposure_mode = Json::obj([("admitted_modes", Json::Arr(vec![Json::str("deferred")]))]);
    }
    let mut d = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 6);
    if let KindRecord::ToolCapability(tc) = &mut d.semantic {
        tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
    }
    if let Some(SurfaceRecord::Tool(ts)) = d.surface.as_mut() {
        ts.exposure_mode = Json::obj([("admitted_modes", Json::Arr(vec![Json::str("deferred")]))]);
    }
    doc.nodes.push(t);
    doc.nodes.push(d);
    let (store, sealed, _vids) = sealed_doc_with("e3-2", doc, vec![], vec![]);
    let p = test_profile();
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed,
            profile_refs: vec![hh_compiler::profile::profile_coordinate(&p)],
            fallback_profile: None,
            targets: vec![mcp_target()],
            compile_for_expired: false,
        },
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect("compile");
    let catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    struct DeferAll;
    impl ExposurePolicy for DeferAll {
        fn rank(
            &self,
            entries: &[hh_compiler::exposure::CatalogEntry],
            _a: &BTreeMap<String, BTreeSet<ExposureMode>>,
            _t: &TurnState,
            _p: &ExposurePolicyParams,
        ) -> Vec<(String, ExposureMode)> {
            entries
                .iter()
                .map(|e| (e.surface_id.clone(), ExposureMode::Deferred))
                .collect()
        }
    }
    let err = select_surfaces(
        &catalog,
        &turn_state("mc:1"),
        &all_modes(),
        &ExposurePolicyParams::default(),
        Some(&DeferAll),
    )
    .expect_err("NoDiscoverySurface");
    assert_eq!(err, SelectError::NoDiscoverySurface);
}

// ── AC-R-2.5.3-3 — I-NARROW PolicyViolation ───────────────────────────────────

#[test]
fn ac_e3_3_policy_outside_admitted_is_policy_violation() {
    let (_s, bundle) = compile_tool_doc("e3-3", "tool_a", Json::Null);
    let catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    // The tool admits only `direct` (the authored default). A policy that
    // returns `deferred` violates I-NARROW — refused before any delivery.
    struct BadPolicy;
    impl ExposurePolicy for BadPolicy {
        fn rank(
            &self,
            entries: &[hh_compiler::exposure::CatalogEntry],
            _a: &BTreeMap<String, BTreeSet<ExposureMode>>,
            _t: &TurnState,
            _p: &ExposurePolicyParams,
        ) -> Vec<(String, ExposureMode)> {
            entries
                .iter()
                .map(|e| (e.surface_id.clone(), ExposureMode::Deferred))
                .collect()
        }
    }
    let err = select_surfaces(
        &catalog,
        &turn_state("mc:1"),
        &all_modes(),
        &ExposurePolicyParams::default(),
        Some(&BadPolicy),
    )
    .expect_err("PolicyViolation");
    assert!(matches!(err, SelectError::PolicyViolation { .. }));
}

// ── AC-R-2.5.3-12 — hidden + direct_all ──────────────────────────────────────

#[test]
fn ac_e3_12_hidden_never_appears_and_direct_all_covers_admitted() {
    let (_s, bundle) = compile_tool_doc(
        "e3-12",
        "tool_a",
        Json::obj([("mode", Json::str("hidden"))]),
    );
    let catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    let entry = catalog.entries.iter().find(|e| e.name == "tool_a").unwrap();
    assert!(entry.hidden);

    // `direct_all` over an all-hidden catalog is empty; the omission is
    // accounted (`omitted{reason: hidden}`), never delivered.
    let plan = select_surfaces(
        &catalog,
        &turn_state("mc:1"),
        &all_modes(),
        &ExposurePolicyParams::default(),
        None,
    )
    .expect("select");
    assert!(plan.entries.is_empty());
    assert!(plan
        .omitted
        .iter()
        .any(|o| o.surface_id == entry.surface_id && o.reason == OmitReason::Hidden));

    // Hidden never indexes or discovers.
    let idx = CatalogIndex::ExactName;
    let hits = index_query(
        &idx,
        &catalog,
        &DiscoveryQuery {
            form: DiscoveryForm::Regex,
            text: "tool_a".into(),
            limit: None,
        },
        8,
    )
    .expect("query");
    assert!(hits.hits.is_empty(), "hidden is never a hit");

    // `check_callable` refuses a hidden surface — `SurfaceNotRevealed` (the
    // name resolves in the catalog; the surface is simply never direct).
    let refusal = check_callable(
        &plan,
        &catalog,
        &CallProposal {
            surface_name: "tool_a".into(),
            from_code: false,
        },
    )
    .expect_err("hidden is not callable");
    assert!(matches!(refusal, CallRefusal::SurfaceNotRevealed { .. }));

    // And with a visible sibling, `direct_all` covers exactly the admitted
    // non-hidden set.
    let (_s2, bundle2) = compile_tool_doc("e3-12b", "tool_b", Json::Null);
    let catalog2 = build_catalog(&bundle2, &BTreeMap::new(), 0, vec![]).expect("catalog");
    let mut merged = catalog.clone();
    merged.entries.extend(catalog2.entries.clone());
    let admitted: BTreeMap<String, BTreeSet<ExposureMode>> = merged
        .entries
        .iter()
        .map(|e| (e.surface_id.clone(), e.admitted_modes.clone()))
        .collect();
    let all = direct_all(&merged.entries, &admitted);
    let visible = catalog2.entries[0].surface_id.clone();
    assert_eq!(all, vec![(visible, ExposureMode::Direct)]);
}

// ── reveal/evict round-trip (D6) ──────────────────────────────────────────────

#[test]
fn reveal_then_evict_round_trip() {
    let (_s, bundle) = compile_tool_doc_with(
        "e3-rev",
        "tool_a",
        Json::obj([(
            "admitted_modes",
            Json::Arr(vec![Json::str("direct"), Json::str("deferred")]),
        )]),
        true,
    );
    let catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    let sid = catalog
        .entries
        .iter()
        .find(|e| e.name == "tool_a")
        .unwrap()
        .surface_id
        .clone();
    let rs = RevealedSet {
        run_id: "run:t".into(),
        entries: vec![],
    };
    // Reveal a catalog member; a non-member refuses (I-CLOSED/I-EPOCH).
    let rs = reveal(
        &rs,
        &catalog,
        std::slice::from_ref(&sid),
        RevealCause::Discovery,
        Retention::Run,
        1,
    )
    .expect("reveal");
    assert_eq!(rs.entries.len(), 1);
    assert!(reveal(
        &rs,
        &catalog,
        &["surface:nope".to_string()],
        RevealCause::Discovery,
        Retention::Run,
        2
    )
    .is_err());
    let rs = evict(&rs, &catalog, &[sid], EvictCause::Policy);
    assert!(rs.entries.is_empty());
}
