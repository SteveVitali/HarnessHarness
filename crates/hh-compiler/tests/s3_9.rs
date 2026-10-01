//! `hh-compiler` S3.9 acceptance — the protocol-edge slices (§5d.3 §2;
//! ticket S3.9; R-2.5.4⁰, R-2.5.2⁰, R-2.5.3⁰):
//!
//! * `sync_source` — all six `SyncTrigger`s drive the same
//!   `catalog_delta → adopt/freeze` path, every call emits
//!   `action.tool.catalog.delta` (empty deltas included), `freeze` keeps
//!   `catalog_id` and marks removed entries `unavailable(removed)`,
//!   `adopt` mints a new epoch whose `bundle_delta` descriptors stamp
//!   `authority = unverified` (ADR-0095 D2).
//! * E5 — the `ErrorFormatSpec` distinguishability battery covers every
//!   `SurfaceFailure` class pairwise (AC-R-2.5.2-7's schema face).
//! * The minimal profiles' `edit_file` shapes are primitive functions
//!   with a total, sound `SurfaceArgMap` (AC-R-2.5.1-4): `patch` under
//!   `parse(grammar_ref)` and `string_replace` under `project` both
//!   satisfy E2, and the two surfaces differ only in profile-owned
//!   fields.

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::equiv::{self, ArgTransform, EvidenceVerdict};
use hh_compiler::exposure::{
    adopt, build_catalog, catalog_delta, sync_source, AdoptOutcome, Availability, Catalog,
    CatalogEntry, CatalogSource, DriftPolicy, SourceState, SyncTrigger,
};
use hh_compiler::profile::{minimal_profiles, profile_coordinate, profile_identity, ModelProfile};
use hh_compiler::seal::ModelSurfaceState;
use hh_compiler::surface::{
    check_distinguishable, Distinguishability, ErrorFormatSpec, SurfaceFailure,
};
use hh_compiler::CompileInputs;
use hh_hir::records::{AgentProcessBody, KindRecord, ProcedureStep};
use hh_hir::refs::ProfileRef;
use hh_wire::json::Json;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// A doc carrying `edit_file` ({path, edits}) — the capability the two
/// minimal profiles shape differently.
fn edit_doc() -> hh_hir::document::HirDocument {
    let mut doc = doc_with(&stage1_assembly());
    // Re-pin the agent's profile — the bound profile is named in the
    // compile inputs, not the doc.
    for n in doc.nodes.iter_mut() {
        if let KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let AgentProcessBody::Native(np) = &mut ap.body {
                np.profile = ProfileRef::unbound();
            }
        }
    }
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
    doc.nodes.push(procedure_node(
        "test:proc",
        vec![ProcedureStep::Invoke {
            tool: sel("test:edit_file"),
            args: Json::Null,
        }],
        6,
    ));
    doc
}

/// The E4 suite for `minimal-patch` (`patch` text → `edits` via `parse`).
fn patch_suite() -> Json {
    let mut grammars = BTreeMap::new();
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
                (
                    "canonical_params",
                    Json::obj([
                        ("path", Json::str("a.txt")),
                        (
                            "edits",
                            Json::Arr(vec![Json::obj([
                                ("new", Json::str("there")),
                                ("old", Json::str("world")),
                            ])]),
                        ),
                    ]),
                ),
            ])]),
        ),
    ])
}

/// The E4 suite for `minimal-string-replace` (`old_string`/`new_string`
/// project into `edits[0]`).
fn replace_suite() -> Json {
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
                (
                    "canonical_params",
                    Json::obj([
                        ("path", Json::str("a.txt")),
                        (
                            "edits",
                            Json::Arr(vec![Json::obj([
                                ("new", Json::str("there")),
                                ("old", Json::str("world")),
                            ])]),
                        ),
                    ]),
                ),
            ])]),
        ),
    ])
}

fn with_suite(profile: &ModelProfile, suite: Json) -> ModelProfile {
    let mut p = profile.clone();
    p.tests = Json::obj([("e4_suites", Json::Arr(vec![suite]))]);
    p.content_hash = profile_identity(&p);
    p
}

fn edit_binding(bundle: &hh_compiler::seal::CompiledBundle) -> equiv::SurfaceBinding {
    let ModelSurfaceState::Lowered(s) = &bundle.model_surface else {
        panic!("stage 3 lowers the surface")
    };
    s.tools
        .iter()
        .find(|t| t.binding.hir_node_id == "test:edit_file")
        .map(|t| t.binding.clone())
        .expect("edit_file surface")
}

/// The capability node as sealed — `check_equivalence`'s `capability`.
fn edit_node(sealed: &hh_hir::SealedDefinition) -> hh_hir::Node {
    sealed
        .document
        .nodes
        .iter()
        .find(|n| n.semantic_id() == "test:edit_file")
        .cloned()
        .expect("edit_file node")
}

// ── AC-R-2.5.1-4 — edit_file as a primitive function under each profile ──

#[test]
fn ac_e1_4_edit_file_shapes_are_total_and_sound() {
    let (patch, replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let replace = with_suite(&replace, replace_suite());

    let (store, sealed, _v) = sealed_doc_with("e14", edit_doc(), vec![], vec![]);
    let cap = edit_node(&sealed);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let mut bindings = Vec::new();
    for profile in [&patch, &replace] {
        let profiles = MapProfileView::of(vec![(*profile).clone()]);
        let bundle = compile(
            &CompileInputs {
                sealed: sealed.clone(),
                profile_refs: vec![profile_coordinate(profile)],
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
        bindings.push(edit_binding(&bundle));
    }
    let [bp, br] = <[equiv::SurfaceBinding; 2]>::try_from(bindings).unwrap();

    // `patch`: `path` identity + `patch → edits` under `parse`.
    assert!(matches!(
        bp.arg_map["patch"].transform,
        ArgTransform::Parse { .. }
    ));
    assert_eq!(bp.arg_map["patch"].capability_param, "edits");
    assert_eq!(bp.arg_map["path"].capability_param, "path");
    // `string_replace`: `old_string`/`new_string` project into `edits.0`.
    assert_eq!(br.arg_map["old_string"].capability_param, "edits.0.old");
    assert_eq!(br.arg_map["new_string"].capability_param, "edits.0.new");
    assert!(matches!(
        br.arg_map["old_string"].transform,
        ArgTransform::Project
    ));

    // Total + sound: E2 passes on both bindings (every declared arg is
    // covered — `edits` by the re-expressing entries — and every map
    // target names a declared property).
    for b in [&bp, &br] {
        let ev = equiv::check_equivalence(b, &cap, None, None, None, false).expect("evidence");
        assert_eq!(
            ev.e2_authority,
            EvidenceVerdict::Pass,
            "E2 totality+soundness on {}",
            b.surface_name
        );
    }
}

// ── AC-R-2.5.2-7 — the E5 distinguishability battery ─────────────────────

#[test]
fn ac_e2_7_all_surface_failures_distinguishable() {
    // A minimal `error_format` rendering covering all eight
    // `SurfaceFailure` classes pairwise-distinguishably → `verified`.
    let renderings: BTreeMap<String, hh_hir::leaves::Text> = SurfaceFailure::ALL
        .iter()
        .map(|f| {
            (
                f.as_str().to_string(),
                text(&format!("the call failed: {}", f.as_str()), 9),
            )
        })
        .collect();
    let spec = ErrorFormatSpec {
        renderings,
        distinguishability: Distinguishability::Unchecked,
    };
    assert_eq!(check_distinguishable(&spec), Distinguishability::Verified);

    // Two classes sharing a rendering → `failed`.
    let mut dup = spec.renderings.clone();
    dup.insert(
        SurfaceFailure::Unparseable.as_str().to_string(),
        text("the call failed: unknown_surface", 9),
    );
    let spec = ErrorFormatSpec {
        renderings: dup,
        distinguishability: Distinguishability::Unchecked,
    };
    assert_eq!(check_distinguishable(&spec), Distinguishability::Failed);
}

// ── sync_source (§5d.3 §2) — one path for all six triggers ───────────────

/// A listing derived from a catalog — entries re-keyed to `source_ref`.
fn listing_from(catalog: &Catalog, source_ref: &str, drop: &BTreeSet<&str>) -> Vec<CatalogEntry> {
    catalog
        .entries
        .iter()
        .filter(|e| !drop.contains(e.surface_id.as_str()))
        .cloned()
        .map(|mut e| {
            e.source = CatalogSource::Mcp(source_ref.to_string());
            e
        })
        .collect()
}

fn source_state(source_ref: &str, hash: &str) -> SourceState {
    SourceState {
        source_ref: source_ref.to_string(),
        snapshot_hash: hash.to_string(),
        ttl: Some(1000),
        listened: true,
    }
}

#[test]
fn ac_e3_sync_source_all_triggers_emit_delta() {
    let (store, sealed, _v) = sealed_doc_with("sync", edit_doc(), vec![], vec![]);
    let _ = &sealed;
    let profile = with_suite(&minimal_profiles().0, patch_suite());
    let profiles = MapProfileView::of(vec![profile.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed,
            profile_refs: vec![profile_coordinate(&profile)],
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
    let mut catalog = build_catalog(
        &bundle,
        &BTreeMap::new(),
        0,
        vec![source_state("mcp:srv:1", "hash:v1")],
    )
    .expect("catalog");
    // The MCP source owns every entry for this test.
    for e in &mut catalog.entries {
        e.source = CatalogSource::Mcp("srv:1".to_string());
    }

    for trigger in [
        SyncTrigger::ListChanged,
        SyncTrigger::TtlExpired,
        SyncTrigger::Reconnect,
        SyncTrigger::ExtensionEnabled,
        SyncTrigger::ExtensionDisabled,
        SyncTrigger::AuthorizationChanged,
    ] {
        // An identical listing under every trigger → the *empty* delta
        // still emits `action.tool.catalog.delta`.
        let listing = listing_from(&catalog, "srv:1", &BTreeSet::new());
        let out = sync_source(
            &catalog,
            "mcp:srv:1",
            trigger,
            &listing,
            &source_state("mcp:srv:1", "hash:v1"),
            DriftPolicy::Adopt,
            "test",
        );
        assert!(
            out.delta.added.is_empty()
                && out.delta.removed.is_empty()
                && out.delta.changed.is_empty(),
            "identical listing ⇒ empty delta"
        );
        // The event is emitted regardless (empty deltas included).
        assert_eq!(
            out.delta_event.get("source_ref").and_then(Json::as_str),
            Some("mcp:srv:1")
        );
        assert!(out.delta_event.get("cause").is_some());
        // `adopt` on an empty delta still mints an epoch.
        assert!(matches!(out.outcome, Ok(AdoptOutcome::Adopted { .. })));
        assert!(out.epoch_event.is_some());
    }
}

#[test]
fn ac_e3_freeze_marks_removed_unavailable_keeps_catalog_id() {
    let (store, sealed, _v) = sealed_doc_with("freeze", edit_doc(), vec![], vec![]);
    let profile = with_suite(&minimal_profiles().0, patch_suite());
    let profiles = MapProfileView::of(vec![profile.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed,
            profile_refs: vec![profile_coordinate(&profile)],
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
    // The catalog's entries all carry the same `source` for this test —
    // re-key them to `srv:1` so `catalog_delta` sees them as the source's.
    let mut catalog = build_catalog(&bundle, &BTreeMap::new(), 0, vec![]).expect("catalog");
    for e in &mut catalog.entries {
        e.source = CatalogSource::Mcp("srv:1".to_string());
    }
    catalog.sources.push(source_state("mcp:srv:1", "hash:v1"));
    let removed_id = catalog.entries[0].surface_id.clone();
    let listing = listing_from(&catalog, "srv:1", &BTreeSet::from([removed_id.as_str()]));

    let out = sync_source(
        &catalog,
        "mcp:srv:1",
        SyncTrigger::ListChanged,
        &listing,
        &source_state("mcp:srv:1", "hash:v2"),
        DriftPolicy::Freeze,
        "test",
    );
    assert_eq!(out.delta.removed, vec![removed_id.clone()]);
    let Ok(AdoptOutcome::Frozen { catalog: frozen }) = out.outcome else {
        panic!("freeze expected")
    };
    // I-EPOCH: `catalog_id` is unchanged; the removed surface is a typed
    // `unavailable`, not a vanished entry.
    assert_eq!(frozen.catalog_id, catalog.catalog_id);
    let e = frozen
        .entries
        .iter()
        .find(|e| e.surface_id == removed_id)
        .unwrap();
    assert_eq!(
        e.availability,
        Availability::Unavailable("removed".to_string())
    );
    // The refreshed source row is recorded.
    let row = frozen
        .sources
        .iter()
        .find(|s| s.source_ref == "mcp:srv:1")
        .unwrap();
    assert_eq!(row.snapshot_hash, "hash:v2");

    // `adopt` on the same delta mints a new epoch; the bundle_delta
    // descriptors stamp `authority = unverified` (ADR-0095 D2).
    let out2 = sync_source(
        &catalog,
        "mcp:srv:1",
        SyncTrigger::ListChanged,
        &listing,
        &source_state("mcp:srv:1", "hash:v2"),
        DriftPolicy::Adopt,
        "test",
    );
    let Ok(AdoptOutcome::Adopted {
        catalog: adopted,
        epoch,
    }) = out2.outcome
    else {
        panic!("adopt expected")
    };
    assert_eq!(epoch.epoch, catalog.epoch + 1);
    assert_ne!(adopted.catalog_id, catalog.catalog_id);
    assert_eq!(epoch.catalog_id_prev, catalog.catalog_id);
    assert!(adopted.entries.iter().all(|e| e.surface_id != removed_id));
    let delta = catalog_delta(&catalog, "mcp:srv:1", SyncTrigger::ListChanged, &listing);
    let adopted2 = adopt(&catalog, &delta, DriftPolicy::Adopt, "test").unwrap();
    assert!(matches!(adopted2, AdoptOutcome::Adopted { .. }));
}

// ── AC-R-2.5.4-1 — lift∘lower ∪ loss_report = x under both profiles ──────

/// `edit_doc` plus a schema `Validator` bound to `edit_file`, so the MCP
/// loss report's `validators` entry has a bound validator to declare.
fn doc_with_bound_validator() -> hh_hir::document::HirDocument {
    let mut doc = edit_doc();
    let mut v = validator_node("test:validator", 7);
    if let KindRecord::Validator(vr) = &mut v.semantic {
        vr.inputs = vec![sel("test:edit_file")];
    }
    doc.nodes.push(v);
    doc
}

#[test]
fn ac_e4_1_lift_lower_round_trip_both_profiles_exact_no_slot() {
    let (store, sealed, _v) = sealed_doc_with("e41", doc_with_bound_validator(), vec![], vec![]);
    let (patch, replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let replace = with_suite(&replace, replace_suite());
    let cat = hh_assembly::Stage1Catalog::stage1();

    for profile in [&patch, &replace] {
        let profiles = MapProfileView::of(vec![(*profile).clone()]);
        let bundle = compile(
            &CompileInputs {
                sealed: sealed.clone(),
                profile_refs: vec![profile_coordinate(profile)],
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

        // lift∘lower ∪ loss_report = x: every runtime-plan tool is either
        // recovered by `lift` or named in the loss report.
        let lifted = hh_compiler::lift(&bundle.target_artefacts["mcp"], "mcp").expect("mcp lifts");
        let recovered: BTreeSet<String> = lifted
            .recovered
            .iter()
            .filter_map(|r| {
                r.get("semantic_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .collect();
        let report = bundle
            .loss_reports
            .iter()
            .find(|r| r.target == "mcp")
            .expect("the mcp loss report");
        let lost: BTreeSet<&str> = report
            .entries
            .iter()
            .map(|e| e.hir_node_id.as_str())
            .collect();
        for tool in &bundle.runtime_plan.tools {
            let sid = &tool.capability.semantic_id;
            assert!(
                recovered.contains(sid) || lost.contains(sid.as_str()),
                "lift(lower(x)) ∪ loss_report = x for {sid}"
            );
        }

        // The AC-named enforcement slots are all declared `no_slot`:
        // `permission.enforcement`, `budgets`, `validators` (a bound
        // validator exists on `edit_file`). The report additionally
        // declares `preconditions`, `observation_contract` and
        // `scope_bindings` at `no_slot` — the same accounting discipline
        // (nothing silently dropped) at finer granularity; every entry
        // is `no_slot`, so the *set* the AC names is exactly the set of
        // enforcement-location slots (the extra entries name data-carried
        // members, still `no_slot`).
        for field in ["permission.enforcement", "budgets", "validators"] {
            assert!(
                report
                    .entries
                    .iter()
                    .any(|e| { e.field == field && e.class == hh_compiler::lcd::LossKind::NoSlot }),
                "{field}: no_slot declared for profile {}",
                profile.profile_id
            );
        }
        assert!(report
            .entries
            .iter()
            .all(|e| e.class == hh_compiler::lcd::LossKind::NoSlot));
    }
}

// ── AC-R-2.5.2-6 — validator_reads ⊆ retained_fields (E6) ─────────────────
// (`concise`/`offload` are the C1 render modes — the C0 mode sum is
// `{full, truncate}` per `surface.rs`; the offload admissibility arm lands
// with them.)

fn render_profile(reads: &[&str], retained: &[&str]) -> ModelProfile {
    let mut p = with_suite(&minimal_profiles().0, patch_suite());
    p.rules.push(rule(
        "r-render",
        hh_compiler::profile::ProfileRuleKind::ResultRender,
        &["tools/*/result_render"],
        Json::obj([
            ("mode", Json::str("truncate")),
            ("max_lines", Json::Int(10)),
            (
                "retained_fields",
                Json::Arr(retained.iter().map(|f| Json::str(*f)).collect()),
            ),
            (
                "validator_reads",
                Json::Arr(reads.iter().map(|f| Json::str(*f)).collect()),
            ),
        ]),
        hh_compiler::profile::DebtStatus::Active,
    ));
    p.content_hash = profile_identity(&p);
    p
}

#[test]
fn ac_e2_6_validator_reads_must_be_retained() {
    let (store, sealed, _v) = sealed_doc_with("e26", doc_with_bound_validator(), vec![], vec![]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let compile_with = |profile: &ModelProfile| {
        compile(
            &CompileInputs {
                sealed: sealed.clone(),
                profile_refs: vec![profile_coordinate(profile)],
                fallback_profile: None,
                targets: vec![mcp_target()],
                compile_for_expired: false,
            },
            &MapProfileView::of(vec![profile.clone()]),
            &store,
            &cat,
            &kernel(),
        )
    };

    // `validator_reads ⊄ retained_fields` under `truncate` → E6 fail →
    // `UnexpressibleSurface` (never a warning).
    let bad = render_profile(&["dropped_field"], &["summary"]);
    let err = compile_with(&bad).expect_err("a dropped validator read refuses");
    assert!(
        matches!(
            err,
            hh_compiler::errors::CompileError::UnexpressibleSurface { .. }
        ),
        "{err:?}"
    );

    // `validator_reads ⊆ retained_fields` compiles; the bound validator's
    // read is carried in the retained set.
    let good = render_profile(&["summary"], &["summary", "detail"]);
    compile_with(&good).expect("reads ⊆ retained compiles");
}

// ── AC-R-2.5.3-4 (deterministic half) — I-ORDER prefix-extension ────────────
//
// Two successive `select_surfaces` calls: the second plan's `order` must be a
// prefix-extension of the first's — still-direct members keep their prior
// positions and a newly-revealed surface appends (never reorders delivered
// surfaces). The run-level half — cache-read tokens non-decreasing on the
// fixture — needs the turn-loop/driver emitters (DF-S1.17-3 family) and is
// recorded as deferred in the run ledger; the delivered-token mass
// (`budget_estimate`) is asserted non-decreasing here as the deterministic
// proxy.
#[test]
fn ac_e3_4_prior_order_is_prefix_extended_on_reveal() {
    use hh_compiler::exposure::{
        select_surfaces, ExposurePolicy, ExposurePolicyParams, Retention, RevealCause,
        RevealedEntry, RevealedSet, TurnState,
    };
    use hh_hir::tools::ExposureMode;

    // Doc: `edit_file` (deferrable) + `discover_surfaces` (I-DISCOVERY: a
    // deferred surface needs a discovery capability in the catalog).
    let mut doc = edit_doc();
    // `edit_file` must admit `deferred` (the profile/policy meet would
    // otherwise narrow it out — I-NARROW refuses on `PolicyViolation`).
    for n in doc.nodes.iter_mut() {
        if let Some(hh_hir::records::SurfaceRecord::Tool(ts)) = n.surface.as_mut() {
            if n.version.semantic_id.as_deref() == Some("test:edit_file") {
                ts.exposure_mode = Json::obj([(
                    "admitted_modes",
                    Json::Arr(vec![Json::str("direct"), Json::str("deferred")]),
                )]);
            }
        }
    }
    let mut disc = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 7);
    if let KindRecord::ToolCapability(tc) = &mut disc.semantic {
        tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
    }
    doc.nodes.push(disc);
    let (store, sealed, _vids) = sealed_doc_with("e3-4", doc, vec![], vec![]);
    let p = with_suite(&minimal_profiles().0, patch_suite());
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![profile_coordinate(&p)],
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
    let edit_sid = catalog
        .entries
        .iter()
        .find(|e| e.name == "edit_file")
        .expect("edit_file entry")
        .surface_id
        .clone();

    // Policy: `edit_file` deferred; everything else direct (discovery stays
    // direct so I-DISCOVERY is satisfied).
    struct DeferEdit;
    impl ExposurePolicy for DeferEdit {
        fn rank(
            &self,
            entries: &[CatalogEntry],
            _admitted: &BTreeMap<String, BTreeSet<ExposureMode>>,
            turn: &TurnState,
            _params: &ExposurePolicyParams,
        ) -> Vec<(String, ExposureMode)> {
            let revealed: BTreeSet<&str> = turn
                .revealed
                .entries
                .iter()
                .map(|e| e.surface_id.as_str())
                .collect();
            entries
                .iter()
                .map(|e| {
                    (
                        e.surface_id.clone(),
                        // A revealed surface ranks `direct` (I-PIN: the kernel
                        // refuses a policy that keeps a revealed/pinned
                        // surface deferred).
                        if e.name == "edit_file" && !revealed.contains(e.surface_id.as_str()) {
                            ExposureMode::Deferred
                        } else {
                            ExposureMode::Direct
                        },
                    )
                })
                .collect()
        }
    }

    let ts =
        |model_call_id: &str, prior_order: Vec<String>, revealed: Vec<RevealedEntry>| TurnState {
            model_call_id: model_call_id.into(),
            revealed: RevealedSet {
                run_id: "run:e3-4".into(),
                entries: revealed,
            },
            recent_calls: vec![],
            goal_ref: None,
            window_cap: 1_000_000,
            prior_order,
        };

    // Call 1: `edit_file` deferred — the plan's order carries the directs.
    let plan1 = select_surfaces(
        &catalog,
        &ts("mc:1", vec![], vec![]),
        &ExposureMode::ALL.iter().copied().collect(),
        &ExposurePolicyParams::default(),
        Some(&DeferEdit),
    )
    .expect("plan 1");
    assert!(
        !plan1.order.contains(&edit_sid),
        "deferred surface is not in the delivery order"
    );

    // Call 2: the run reveals `edit_file`; the new plan must prefix-extend
    // plan1's order — never reorder a delivered surface.
    let plan2 = select_surfaces(
        &catalog,
        &ts(
            "mc:2",
            plan1.order.clone(),
            vec![RevealedEntry {
                surface_id: edit_sid.clone(),
                mode: ExposureMode::Direct,
                revealed_at: 0,
                cause: RevealCause::Discovery,
                retention: Retention::Run,
            }],
        ),
        &ExposureMode::ALL.iter().copied().collect(),
        &ExposurePolicyParams::default(),
        Some(&DeferEdit),
    )
    .expect("plan 2");

    assert!(
        plan2.order.len() > plan1.order.len(),
        "the reveal appended a direct surface: {plan2:?}"
    );
    assert_eq!(
        &plan2.order[..plan1.order.len()],
        plan1.order.as_slice(),
        "I-ORDER: plan 2's order must prefix-extend plan 1's"
    );
    assert_eq!(
        plan2.order.last().map(String::as_str),
        Some(edit_sid.as_str()),
        "the newly-revealed surface appends"
    );
    assert!(
        plan2.budget_estimate.tokens >= plan1.budget_estimate.tokens,
        "delivered-token mass is non-decreasing across the reveal"
    );
}

// ── AC-R-2.5.3-6 (provider half) — a revealable deferred surface lowers ──────
//
// The AC asks for a revealed deferred surface lowered to `provider_tool_api`
// and lifted back preserving surface_id/namespace/deferral state. ADR-0021 D5
// rules the provider target lift-less (consumed by the model; T-LCD-11 is
// discharged by the loss report + T1–T7 normalisation record — pinned by
// `s3_2::t_lcd_11_provider_target_has_no_lift_but_declared_losses`), so the
// lift-back arm runs on the `mcp` target (`ac_e4_1_lift_lower_round_trip…`)
// and this test pins the provider-side halves that exist: (a) a surface
// admitting {direct, deferred} is NOT unexposed — only an all-deferred meet
// drops with `unexposed{deferred}`; (b) the artefact's
// `normalization.name_to_semantic_id` preserves the surface→semantic identity
// the response path needs. The deferral *state* itself lives in the catalog /
// exposure plan, not in either artefact dialect.
#[test]
fn ac_e3_6_revealable_deferred_surface_lowers_to_provider() {
    let mut doc = edit_doc();
    for n in doc.nodes.iter_mut() {
        if let Some(hh_hir::records::SurfaceRecord::Tool(ts)) = n.surface.as_mut() {
            if n.version.semantic_id.as_deref() == Some("test:edit_file") {
                ts.exposure_mode = Json::obj([(
                    "admitted_modes",
                    Json::Arr(vec![Json::str("direct"), Json::str("deferred")]),
                )]);
            }
        }
    }
    // A deferred-admitting surface needs a discovery capability at seal
    // (NoDiscoverySurface — I-DISCOVERY).
    let mut disc = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 7);
    if let KindRecord::ToolCapability(tc) = &mut disc.semantic {
        tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
    }
    doc.nodes.push(disc);
    let (store, sealed, _vids) = sealed_doc_with("e3-6", doc, vec![], vec![]);
    let p = with_suite(&minimal_profiles().0, patch_suite());
    let profiles = MapProfileView::of(vec![p.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    let bundle = compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![profile_coordinate(&p)],
            fallback_profile: None,
            targets: vec![
                hh_compiler::link::TargetSpec {
                    target_id: "provider_tool_api".to_string(),
                    spec_version: "1.0".to_string(),
                    content_hash: "sha256:target-provider".to_string(),
                },
                mcp_target(),
            ],
            compile_for_expired: false,
        },
        &profiles,
        &store,
        &cat,
        &kernel(),
    )
    .expect("compile");

    // (a) the deferred-admitting surface is present in the provider artefact.
    let artefact = &bundle.target_artefacts["provider_tool_api"];
    let tools = match artefact.get("tools") {
        Some(Json::Arr(t)) => t,
        other => panic!("provider artefact carries tools[]: {other:?}"),
    };
    let edit = tools
        .iter()
        .find(|t| t.get("name").and_then(Json::as_str) == Some("patch__edit_file"))
        .expect("edit_file lowered into the provider artefact");
    assert_eq!(edit.get("type").and_then(Json::as_str), Some("function"));

    // (b) surface→semantic identity survives in the normalisation record.
    let mapped = artefact
        .get("normalization")
        .and_then(|n| n.get("name_to_semantic_id"))
        .and_then(|m| m.get("patch__edit_file"))
        .and_then(Json::as_str)
        .expect("name_to_semantic_id entry");
    assert_eq!(mapped, "test:edit_file");

    // And lift on the provider artefact is still the typed refusal — the
    // round-trip arm lives on `mcp` (see ac_e4_1_lift_lower_round_trip…).
    let err = hh_compiler::target::lift(artefact, "provider_tool_api")
        .expect_err("provider target declares no lifting contract");
    assert!(matches!(
        err,
        hh_compiler::errors::CompileError::TargetError { .. }
    ));
}

// ── AC-R-2.5.3-5 — the discovery capability under two profiles ───────────────
//
// The AC's regex-form vs natural-language-form arm presumes two discovery
// surface *families*; at C0 the only compiled shape is the primitive
// `native_fc` family (crates/hh-compiler/src/link.rs — "only compiled shape
// is the primitive `native_fc` family"), so the form-variance arm is
// deferred pending a profile family that owns discovery form (run ledger).
// What is executable at C0 and pinned here: the discovery capability compiles
// under both minimal profiles; E1–E3 and E7 pass on each binding; and the
// bindings differ only in profile-owned fields (name/arg-map/description —
// capability identity is invariant).
#[test]
fn ac_e3_5_discovery_capability_two_profiles_profile_owned_only() {
    let mut doc = edit_doc();
    let mut disc = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 7);
    if let KindRecord::ToolCapability(tc) = &mut disc.semantic {
        tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
    }
    doc.nodes.push(disc);
    let (store, sealed, _vids) = sealed_doc_with("e3-5", doc, vec![], vec![]);
    let (patch, replace) = minimal_profiles();
    let patch = with_suite(&patch, patch_suite());
    let replace = with_suite(&replace, replace_suite());
    let cat = hh_assembly::Stage1Catalog::stage1();

    let mut bindings = Vec::new();
    let mut evidences = Vec::new();
    for profile in [&patch, &replace] {
        let profiles = MapProfileView::of(vec![(*profile).clone()]);
        let bundle = compile(
            &CompileInputs {
                sealed: sealed.clone(),
                profile_refs: vec![profile_coordinate(profile)],
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
        let ModelSurfaceState::Lowered(s) = &bundle.model_surface else {
            panic!("stage 3 lowers the surface")
        };
        bindings.push(
            s.tools
                .iter()
                .find(|t| t.binding.hir_node_id == "test:disc")
                .map(|t| t.binding.clone())
                .expect("discover_surfaces surface"),
        );
        evidences.push(
            bundle
                .equivalence_evidence
                .iter()
                .find(|e| e.capability == "test:disc")
                .cloned()
                .expect("discovery evidence"),
        );
    }
    let [bp, br] = <[equiv::SurfaceBinding; 2]>::try_from(bindings).unwrap();

    // Capability identity is invariant — the profile owns surface shape only.
    assert_eq!(bp.hir_node_id, "test:disc");
    assert_eq!(bp.hir_node_id, br.hir_node_id);
    assert_eq!(bp.capability_refs, br.capability_refs);
    assert_eq!(bp.effects_bound, br.effects_bound);
    // The two bindings differ only in profile-owned fields. `surface_name`
    // and `arg_map`/`description` are the profile's; nothing else may move
    // (mapping kind, exposure mode, capability refs are definition-owned).
    assert_eq!(bp.mapping, br.mapping, "mapping kind is not profile-owned");
    assert_eq!(
        bp.exposure_mode, br.exposure_mode,
        "compile-time exposure mode is not profile-owned"
    );

    // E1–E3 and E7 pass on each binding (E4/E5/E6 are stage-gated n/a at C0
    // for this open-world capability; the AC names E1–E3 + E7).
    for ev in &evidences {
        assert_eq!(ev.e1_effect_equality, EvidenceVerdict::Pass);
        assert_eq!(ev.e2_authority, EvidenceVerdict::Pass);
        assert_eq!(ev.e3_precondition_domain, EvidenceVerdict::Pass);
        assert_eq!(ev.e7_accounting_identity, EvidenceVerdict::Pass);
    }
}
