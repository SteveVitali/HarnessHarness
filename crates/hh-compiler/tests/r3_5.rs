//! `hh-compiler` R3.5 acceptance — the tool-exposure residual (ticket R3.5;
//! BL-16 / DF-S1.17-1; ADR-0272 revisit legs; AC-R-2.5.2-12's profile half):
//!
//! * `provider_tool_api` deferral flags — `deferred_tools`/`tool_search` are
//!   declared on both minimal profiles (`unknown` — the C0 spelling of
//!   unsupported, never coerced), the provider artefact carries the
//!   `deferral{deferred_tools, tool_search}` block verbatim, each tool entry
//!   carries its `deferrable` flag, and a deferrable surface under an
//!   unestablished `deferred_tools` records a declared `no_slot` loss (the
//!   refusal shape §3.2.5 owns — never a silent direct-only narrowing).
//! * ADR-0272's catalog leg — `sync_source` computes the real
//!   `CatalogDelta` off a fixture listing and runs the *existing* `adopt`
//!   path end-to-end: the epoch advances, the dropped surface leaves the
//!   adopted catalog, the source row records the new snapshot, and the
//!   `action.tool.catalog.delta` + epoch events emit. The S3.9 matrix
//!   covers the six triggers; this battery pins the adopt leg so removing
//!   it fails here.

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use hh_compiler::compiler::compile;
use hh_compiler::exposure::{
    build_catalog, sync_source, AdoptOutcome, Catalog, CatalogSource, DriftPolicy, SourceState,
    SyncTrigger,
};
use hh_compiler::link::TargetSpec;
use hh_compiler::profile::{
    minimal_profiles, profile_coordinate, profile_identity, CapabilityState, ModelProfile,
};
use hh_compiler::target::PROVIDER_ARTEFACT_DIALECT;
use hh_compiler::CompileInputs;
use hh_hir::records::{AgentProcessBody, KindRecord, ProcedureStep};
use hh_hir::refs::ProfileRef;
use hh_wire::json::Json;

// ── fixtures ─────────────────────────────────────────────────────────────────

fn provider_target() -> TargetSpec {
    TargetSpec {
        target_id: "provider_tool_api".to_string(),
        spec_version: "1.0".to_string(),
        content_hash: "sha256:target-provider".to_string(),
    }
}

/// The E4 suite for `minimal-patch` (`patch` text → `edits` via `parse`) —
/// the s3_9 fixture shape; Stage-3 closed-world requires a declared suite.
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

fn with_suite(profile: &ModelProfile, suite: Json) -> ModelProfile {
    let mut p = profile.clone();
    p.tests = Json::obj([("e4_suites", Json::Arr(vec![suite]))]);
    p.content_hash = profile_identity(&p);
    p
}

/// `edit_file` ({path, edits}) — the capability the two minimal profiles
/// shape differently (the s3_9 fixture shape).
fn edit_doc() -> hh_hir::document::HirDocument {
    let mut doc = doc_with(&stage1_assembly());
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

/// The doc with `edit_file` admitting `{direct, deferred}` (a deferred-
/// admitting surface needs a discovery capability at seal — I-DISCOVERY).
fn edit_doc_deferrable() -> hh_hir::document::HirDocument {
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
    let mut disc = surfaced_tool_node("test:disc", "discover_surfaces", &["path"], 7);
    if let KindRecord::ToolCapability(tc) = &mut disc.semantic {
        tc.exposure_hint = Json::obj([("discovery", Json::Bool(true))]);
    }
    doc.nodes.push(disc);
    doc
}

fn compile_provider(
    sealed: &hh_hir::SealedDefinition,
    store: &hh_registry::RegistryStore,
    profile: &ModelProfile,
) -> hh_compiler::seal::CompiledBundle {
    let profiles = MapProfileView::of(vec![profile.clone()]);
    let cat = hh_assembly::Stage1Catalog::stage1();
    compile(
        &CompileInputs {
            sealed: sealed.clone(),
            profile_refs: vec![profile_coordinate(profile)],
            fallback_profile: None,
            targets: vec![provider_target()],
            compile_for_expired: false,
            intent_ref: None,
        },
        &profiles,
        store,
        &cat,
        &kernel(),
    )
    .expect("compile")
}

fn provider_artefact(bundle: &hh_compiler::seal::CompiledBundle) -> &Json {
    &bundle.target_artefacts["provider_tool_api"]
}

fn deferral_block(artefact: &Json) -> &Json {
    artefact
        .get("deferral")
        .expect("provider artefact carries the deferral block")
}

fn tools(artefact: &Json) -> &Vec<Json> {
    match artefact.get("tools") {
        Some(Json::Arr(t)) => t,
        other => panic!("provider artefact carries tools[]: {other:?}"),
    }
}

fn tool_entry<'a>(artefact: &'a Json, name: &str) -> &'a Json {
    tools(artefact)
        .iter()
        .find(|t| t.get("name").and_then(Json::as_str) == Some(name))
        .unwrap_or_else(|| panic!("tool {name} lowered"))
}

// ── the flags on the profiles' declared surface ──────────────────────────────

#[test]
fn r3_5_minimal_profiles_declare_provider_tool_api_deferral_flags() {
    for profile in [minimal_profiles().0, minimal_profiles().1] {
        assert_eq!(
            profile.capabilities.capability_state("deferred_tools"),
            Some(CapabilityState::Unknown),
            "{} declares deferred_tools: unknown (the fail-closed C0 \
             spelling — never a guessed supported)",
            profile.profile_id,
        );
        assert_eq!(
            profile.capabilities.capability_state("tool_search"),
            Some(CapabilityState::Unknown),
            "{} declares tool_search: unknown",
            profile.profile_id,
        );
    }
}

// ── the artefact's deferral block ────────────────────────────────────────────

#[test]
fn r3_5_provider_artefact_carries_deferral_flags() {
    let (store, sealed, _v) = sealed_doc_with("r35-flags", edit_doc(), vec![], vec![]);
    let p = with_suite(&minimal_profiles().0, patch_suite());
    let bundle = compile_provider(&sealed, &store, &p);
    let artefact = provider_artefact(&bundle);
    assert_eq!(
        artefact.get("dialect").and_then(Json::as_str),
        Some(PROVIDER_ARTEFACT_DIALECT)
    );
    let deferral = deferral_block(artefact);
    assert_eq!(
        deferral.get("deferred_tools").and_then(Json::as_str),
        Some("unknown"),
        "minimal-patch leaves deferred_tools unestablished — the flag \
         reads `unknown`, never coerced"
    );
    assert_eq!(
        deferral.get("tool_search").and_then(Json::as_str),
        Some("unknown")
    );
    // No surface admits `deferred` — every tool's flag is `false`.
    for t in tools(artefact) {
        assert_eq!(
            t.get("deferrable"),
            Some(&Json::Bool(false)),
            "{} admits direct only",
            t.get("name").and_then(Json::as_str).unwrap_or("?")
        );
    }
}

// ── the gate — deferrable under unestablished capability is a declared loss ──

#[test]
fn r3_5_deferrable_under_unknown_declares_no_slot_loss() {
    let (store, sealed, _v) =
        sealed_doc_with("r35-defer-unknown", edit_doc_deferrable(), vec![], vec![]);
    let p = with_suite(&minimal_profiles().0, patch_suite());
    let bundle = compile_provider(&sealed, &store, &p);
    let artefact = provider_artefact(&bundle);

    let edit = tool_entry(artefact, "patch__edit_file");
    assert_eq!(
        edit.get("deferrable"),
        Some(&Json::Bool(true)),
        "the deferred-admitting surface carries its deferral flag"
    );
    assert_eq!(
        deferral_block(artefact)
            .get("deferred_tools")
            .and_then(Json::as_str),
        Some("unknown")
    );

    // The refused leg is declared on the loss report — a `no_slot`/`narrowed`
    // entry naming the surface, never a silent direct-only narrowing.
    let report = bundle
        .loss_reports
        .iter()
        .find(|r| r.target == "provider_tool_api")
        .expect("provider loss report");
    let loss = report
        .entries
        .iter()
        .find(|e| e.field == "deferral.patch__edit_file")
        .expect("the deferral leg declares its loss");
    assert_eq!(loss.class, hh_compiler::lcd::LossKind::NoSlot);
    assert_eq!(loss.severity, hh_compiler::lcd::LossSeverity::Narrowed);
    assert!(
        loss.detail.contains("deferred_tools"),
        "the loss names the unestablished capability"
    );
}

// ── the gate — declared capability carries the leg clean ─────────────────────

#[test]
fn r3_5_deferrable_under_declared_capability_carries_flag_clean() {
    let (store, sealed, _v) =
        sealed_doc_with("r35-defer-declared", edit_doc_deferrable(), vec![], vec![]);
    let mut p = with_suite(&minimal_profiles().0, patch_suite());
    p.capabilities.deferred_tools = CapabilityState::Declared;
    p.capabilities.tool_search = CapabilityState::Declared;
    p.content_hash = profile_identity(&p);
    let bundle = compile_provider(&sealed, &store, &p);
    let artefact = provider_artefact(&bundle);

    let deferral = deferral_block(artefact);
    assert_eq!(
        deferral.get("deferred_tools").and_then(Json::as_str),
        Some("declared")
    );
    assert_eq!(
        deferral.get("tool_search").and_then(Json::as_str),
        Some("declared")
    );
    assert_eq!(
        tool_entry(artefact, "patch__edit_file").get("deferrable"),
        Some(&Json::Bool(true))
    );

    // The declared capability expresses the leg — no deferral loss lands.
    let report = bundle
        .loss_reports
        .iter()
        .find(|r| r.target == "provider_tool_api")
        .expect("provider loss report");
    assert!(
        !report
            .entries
            .iter()
            .any(|e| e.field.starts_with("deferral.")),
        "declared deferred_tools expresses the deferred leg — no loss entry"
    );
}

// ── codec — the flags round-trip the profile record ──────────────────────────

#[test]
fn r3_5_minimal_profile_flags_round_trip_the_codec() {
    use hh_compiler::schema::{profile_from_json, profile_to_json};
    for profile in [minimal_profiles().0, minimal_profiles().1] {
        let wire = profile_to_json(&profile);
        let caps = wire
            .get("capabilities")
            .expect("capabilities member on the wire");
        for axis in ["deferred_tools", "tool_search"] {
            assert_eq!(
                caps.get(axis).and_then(Json::as_str),
                Some("unknown"),
                "{axis} is on the wire for {}",
                profile.profile_id
            );
        }
        let back = profile_from_json(&wire, "/profile").expect("decode");
        assert_eq!(
            back.capabilities.capability_state("deferred_tools"),
            Some(CapabilityState::Unknown)
        );
        assert_eq!(
            back.capabilities.capability_state("tool_search"),
            Some(CapabilityState::Unknown)
        );
    }
}

// ── ADR-0272's catalog leg — sync_source → adopt over a real delta ───────────

/// A listing derived from a catalog — entries re-keyed to `source_ref`
/// (the s3_9 fixture shape).
fn listing_from(
    catalog: &Catalog,
    source_ref: &str,
    drop: &BTreeSet<&str>,
) -> Vec<hh_compiler::exposure::CatalogEntry> {
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

/// `sync_source` computes the delta and runs the *existing* `adopt` leg
/// (§5d.3 §2; ADR-0272 revisit at R3.5): a listing that drops one surface
/// and lifts one new one adopts a new epoch — `epoch` advances, the
/// removed surface leaves the catalog, the added surface joins (lifted,
/// `authority = unverified` on the epoch's `bundle_delta`), the source
/// row records the new snapshot, and both ledger rows emit. Removing the
/// adopt leg fails this test — the delta alone never rewrites the
/// catalog.
#[test]
fn r3_5_sync_source_adopts_catalog_delta() {
    let (store, sealed, _v) = sealed_doc_with("r35-sync", edit_doc(), vec![], vec![]);
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
            intent_ref: None,
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
    for e in &mut catalog.entries {
        e.source = CatalogSource::Mcp("srv:1".to_string());
    }

    // The new listing: one surface dropped, one surface lifted in.
    let removed_id = catalog.entries[0].surface_id.clone();
    let mut listing = listing_from(&catalog, "srv:1", &BTreeSet::from([removed_id.as_str()]));
    let mut added = catalog.entries[0].clone();
    added.surface_id = "srv:1:lifted-new".to_string();
    added.name = "lifted_new".to_string();
    added.lifted = true; // the MCP-edge lift marks its imports (ADR-0095 D2)
    added.pinned = false;
    listing.push(added);

    let out = sync_source(
        &catalog,
        "mcp:srv:1",
        SyncTrigger::ListChanged,
        &listing,
        &source_state("mcp:srv:1", "hash:v2"),
        DriftPolicy::Adopt,
        "test",
    );

    // The computed delta names both halves.
    assert_eq!(out.delta.removed, vec![removed_id.clone()]);
    assert_eq!(
        out.delta
            .added
            .iter()
            .map(|e| e.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["srv:1:lifted-new"]
    );
    // `action.tool.catalog.delta` emits — every sync, delta or not.
    assert_eq!(
        out.delta_event.get("source_ref").and_then(Json::as_str),
        Some("mcp:srv:1")
    );
    assert_eq!(
        out.delta_event.get("cause").and_then(Json::as_str),
        Some("list_changed")
    );

    // The adopt leg minted the next epoch over the delta.
    let Ok(AdoptOutcome::Adopted {
        catalog: adopted,
        epoch,
    }) = out.outcome
    else {
        panic!("drift_policy = adopt must land Adopted")
    };
    assert_eq!(epoch.epoch, catalog.epoch + 1, "the epoch advances");
    assert_eq!(epoch.catalog_id_prev, catalog.catalog_id);
    assert_ne!(adopted.catalog_id, catalog.catalog_id);
    assert_eq!(epoch.catalog_id, adopted.catalog_id);
    assert_eq!(epoch.adopted_by, "test");
    // The epoch's bundle_delta descriptors stamp authority = unverified.
    let added_descs = match epoch.bundle_delta.get("added") {
        Some(Json::Arr(a)) => a.clone(),
        other => panic!("bundle_delta.added: {other:?}"),
    };
    assert_eq!(
        added_descs[0].get("authority").and_then(Json::as_str),
        Some("unverified"),
        "an adopted lift is unverified, never coerced"
    );
    // The removed surface is gone; the lifted surface joined.
    assert!(adopted.entries.iter().all(|e| e.surface_id != removed_id));
    let joined = adopted
        .entries
        .iter()
        .find(|e| e.surface_id == "srv:1:lifted-new")
        .expect("the added surface joined the epoch");
    assert!(joined.lifted);
    // The source row refreshed to the new snapshot.
    let row = adopted
        .sources
        .iter()
        .find(|r| r.source_ref == "mcp:srv:1")
        .expect("the source row persists");
    assert_eq!(row.snapshot_hash, "hash:v2");
    // `action.tool.catalog.epoch` emits only on adopt.
    assert!(out.epoch_event.is_some());
}
