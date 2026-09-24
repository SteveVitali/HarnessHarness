//! S3.9 registry acceptance — `import_listing`/`refresh` over
//! `hh-mcp-listing/1` documents (§5d.1 §2; R-2.5.4⁰; ADR-0088).
//!
//! Covers: the own-export round trip with the exact loss report
//! (AC-R-2.5.1-5), foreign listings registering `unverified` +
//! `quarantined` with the `unknown_domain` honesty form, foreign `_meta`
//! preservation (N4), the pin-authority non-transfer rule, and the
//! `refresh` delta (`unchanged`/`added`/`removed`/`superseded` with
//! `surface_only | semantic` classification — AC-R-2.5.1-6).

use std::path::PathBuf;

use hh_hir::kinds::{ToolEffects, EFFECT_DOMAINS};
use hh_hir::KindRecord;
use hh_provenance::ProvenanceRecord;
use hh_registry::import::{import_listing, refresh, HH_META_KEY};
use hh_registry::kinds::Admission;
use hh_registry::records::RegistryRecord;
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-s39-import-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("s39.import.test", 0)
}

fn open_store(tag: &str) -> RegistryStore {
    RegistryStore::open(dir(tag), &kernel()).expect("open")
}

/// A wire `Tool` — `inputSchema` + optional carried/foreign `_meta`.
fn wire_tool(name: &str, desc: &str, meta: Option<Json>) -> Json {
    let mut m = vec![
        ("name", Json::str(name)),
        ("description", Json::str(desc)),
        (
            "inputSchema",
            Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
                ),
            ]),
        ),
    ];
    if let Some(meta) = meta {
        m.push(("_meta", meta));
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn listing(server_ref: &str, tools: &[Json]) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-listing/1")),
        ("server_ref", Json::str(server_ref)),
        (
            "listing_hash",
            Json::str(hh_identity::idp_id(
                "mcp.listing",
                Json::Arr(tools.to_vec()).to_canonical_string().as_bytes(),
            )),
        ),
        (
            "tools",
            Json::Arr(
                tools
                    .iter()
                    .map(|t| Json::obj([("tool", t.clone())]))
                    .collect(),
            ),
        ),
    ])
}

/// The carried HIR block an own export rides under
/// `dev.cognition/hir`.
fn carried(effects: Json) -> Json {
    Json::obj([(
        HH_META_KEY,
        Json::obj([
            ("semantic_id", Json::str("sem:xyz")),
            ("effects", effects),
            ("permission_class", Json::str("write")),
            ("provenance_ref", Json::str("prov:1")),
            ("budget_ref", Json::str("budget:1")),
        ]),
    )])
}

fn capability_of(store: &RegistryStore, vid: &str) -> hh_hir::records::ToolCapabilityRecord {
    let (_, rec) = store.get(vid).expect("registered");
    let RegistryRecord::Capability(c) = rec else {
        panic!("not a capability");
    };
    let KindRecord::ToolCapability(t) = &c.node.semantic else {
        panic!("not a tool_capability");
    };
    t.clone()
}

// ── AC-R-2.5.1-5 — own-export round trip, exact loss report ─────────────

#[test]
fn ac_e1_5_import_own_export_recovers_carried_block() {
    let mut store = open_store("own-export");
    let meta = carried(Json::Arr(vec![Json::obj([
        ("domain", Json::str("fs_write")),
        (
            "attributes",
            Json::obj([
                ("mutability", Json::str("destructive")),
                ("repeat_safety", Json::str("idempotent")),
                ("world", Json::str("closed")),
                ("reversibility", Json::str("compensable")),
            ]),
        ),
    ])]));
    let doc = listing(
        "srv:own",
        &[wire_tool("edit_file", "edit a file", Some(meta))],
    );
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    assert_eq!(out.recovered, vec!["edit_file"]);
    assert!(out.declared_unverified.is_empty());
    // The exact loss report — the three record members the carried set
    // does not cover.
    assert_eq!(
        out.losses,
        vec![
            "cost_model.measured_ref",
            "execution_requirement",
            "exposure_hint"
        ]
    );
    let t = capability_of(&store, &out.refs[0].version_id);
    // The declared `EffectClass` survives — attributes included.
    match &t.effects {
        ToolEffects::Declared(set) => {
            assert_eq!(set.len(), 1);
            let e = set.iter().next().unwrap();
            assert_eq!(e.domain.name(), "fs_write");
            assert!(e.attributes.is_some(), "attributes ride the carried set");
        }
        other => panic!("expected declared effects, got {other:?}"),
    }
    assert_eq!(
        t.input_schema.get("type").and_then(Json::as_str),
        Some("object")
    );
    // `mcp_listing` ⇒ quarantined admission (ADR-0088).
    let (env, _) = store.get(&out.refs[0].version_id).unwrap();
    assert!(
        matches!(env.admission, Admission::Quarantined),
        "lifted source ⇒ quarantined"
    );
    // The D6 name map.
    assert_eq!(
        out.name_map.get("edit_file").map(|(s, _)| s.as_str()),
        Some("srv:own")
    );
}

// ── AC-R-2.5.1-5 — foreign listing ⇒ unverified + quarantined ───────────

#[test]
fn ac_e1_5_foreign_listing_registers_unverified_quarantined() {
    let mut store = open_store("foreign");
    let doc = listing("srv:foreign", &[wire_tool("search", "find things", None)]);
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    assert_eq!(out.declared_unverified, vec!["search"]);
    let t = capability_of(&store, &out.refs[0].version_id);
    // `unknown_domain` — every closed domain, no attribute vectors.
    match &t.effects {
        ToolEffects::Declared(set) => {
            assert_eq!(set.len(), EFFECT_DOMAINS.len());
            assert!(set.iter().all(|e| e.attributes.is_none()));
        }
        other => panic!("expected unknown_domain declared set, got {other:?}"),
    }
    let (env, _) = store.get(&out.refs[0].version_id).unwrap();
    assert!(matches!(env.admission, Admission::Quarantined));
    // Node authority is `unverified` — Origin::import mints it (P7).
    let RegistryRecord::Capability(c) = &store.get(&out.refs[0].version_id).unwrap().1 else {
        panic!()
    };
    assert_eq!(
        c.node.provenance.authority,
        hh_provenance::AuthorityClass::Unverified
    );
}

// ── N4 — foreign `_meta` keys preserved verbatim ─────────────────────────

#[test]
fn ac_e1_5_foreign_meta_preserved_verbatim() {
    let mut store = open_store("ext-meta");
    let meta = Json::obj([("com.acme/extra", Json::obj([("flag", Json::Bool(true))]))]);
    let doc = listing("srv:x", &[wire_tool("t", "d", Some(meta))]);
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    assert_eq!(out.ext_meta.len(), 1);
    assert_eq!(
        out.ext_meta[0].get("key").and_then(Json::as_str),
        Some("com.acme/extra")
    );
    let t = capability_of(&store, &out.refs[0].version_id);
    let Json::Obj(ext) = t.source.get("ext_meta").unwrap() else {
        panic!("ext_meta not an object")
    };
    assert!(
        ext.contains_key("com.acme/extra"),
        "foreign key rides source.ext_meta"
    );
}

// ── ADR-0088 D5 — a claimed pin never transfers authority ────────────────

#[test]
fn ac_e1_5_listing_claiming_pin_stays_unverified() {
    let mut store = open_store("pin-claim");
    // A foreign server claims a pin/attestation inside `_meta` — the
    // lift records it as a claim; `import_listing` never mints above
    // `unverified` (the `pin` op is the only raise path).
    let meta = Json::obj([(
        "com.acme/attestation",
        Json::obj([("authority", Json::str("verified"))]),
    )]);
    let doc = listing("srv:evil", &[wire_tool("t", "d", Some(meta))]);
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    let RegistryRecord::Capability(c) = &store.get(&out.refs[0].version_id).unwrap().1 else {
        panic!()
    };
    assert_eq!(
        c.node.provenance.authority,
        hh_provenance::AuthorityClass::Unverified,
        "a listing's self-claim never raises authority"
    );
}

// ── AC-R-2.5.1-6 — refresh: unchanged ⇒ no-op ────────────────────────────

#[test]
fn ac_e1_6_refresh_unchanged_listing_is_noop() {
    let mut store = open_store("noop");
    let doc = listing(
        "srv:r",
        &[wire_tool("a", "1", None), wire_tool("b", "2", None)],
    );
    import_listing(&mut store, &doc, &kernel()).unwrap();
    let out = refresh(&mut store, &doc, &kernel()).unwrap();
    assert_eq!(out.unchanged, vec!["a", "b"]);
    assert!(out.added.is_empty() && out.removed.is_empty() && out.superseded.is_empty());
}

// ── AC-R-2.5.1-6 — refresh: rug pull ⇒ superseded + classified ───────────

#[test]
fn ac_e1_6_refresh_rug_pull_supersedes_and_classifies() {
    let mut store = open_store("rugpull");
    let v1 = listing(
        "srv:r",
        &[
            wire_tool("a", "v1", None),
            wire_tool(
                "b",
                "v1",
                Some(Json::obj([("com.acme/flag", Json::Bool(false))])),
            ),
        ],
    );
    let imported = import_listing(&mut store, &v1, &kernel()).unwrap();
    let prev_a = imported.refs[0].version_id.clone();

    // v2: `a` changes inputSchema (semantic); `b` changes only a foreign
    // `_meta` value (surface_only — `ext_meta` is outside the semantic
    // core); `c` is new.
    let v2 = listing(
        "srv:r",
        &[
            Json::Obj(
                vec![
                    ("name".to_string(), Json::str("a")),
                    ("description".to_string(), Json::str("v1")),
                    (
                        "inputSchema".to_string(),
                        Json::obj([
                            ("type", Json::str("object")),
                            (
                                "properties",
                                Json::obj([("other", Json::obj([("type", Json::str("string"))]))]),
                            ),
                        ]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            wire_tool(
                "b",
                "v1",
                Some(Json::obj([("com.acme/flag", Json::Bool(true))])),
            ),
            wire_tool("c", "new", None),
        ],
    );
    let out = refresh(&mut store, &v2, &kernel()).unwrap();
    assert_eq!(out.added, vec!["c"]);
    assert!(out.removed.is_empty());
    assert!(out.unchanged.is_empty());
    assert_eq!(out.superseded.len(), 2);
    let by_name = |n: &str| out.superseded.iter().find(|s| s.name == n).unwrap();
    assert_eq!(by_name("a").classification, "semantic");
    assert_eq!(by_name("b").classification, "surface_only");
    assert_eq!(by_name("a").prev_version_id, prev_a);
    // The supersedes edge is recorded in lineage.
    let lineage = store.lineage(&by_name("a").version_id).unwrap();
    assert!(
        lineage.supersedes.contains(&prev_a),
        "supersedes{{edit}} edge recorded"
    );
}

// ── AC-R-2.5.1-6 — removed surfaces stay addressable ─────────────────────

#[test]
fn ac_e1_6_refresh_removed_version_stays_addressable() {
    let mut store = open_store("removed");
    let v1 = listing("srv:r", &[wire_tool("gone", "1", None)]);
    let imported = import_listing(&mut store, &v1, &kernel()).unwrap();
    let vid = imported.refs[0].version_id.clone();
    let v2 = listing("srv:r", &[wire_tool("other", "1", None)]);
    let out = refresh(&mut store, &v2, &kernel()).unwrap();
    assert_eq!(out.removed, vec!["gone"]);
    assert_eq!(out.added, vec!["other"]);
    // The removed version is still addressable by identity — the
    // catalog layer owns `unavailable`.
    assert!(store.get(&vid).is_some(), "identity never vanishes");
    assert!(store.lookup(&vid).is_ok());
}

// ── AC-R-2.5.1-9 + AC-R-2.5.4-3 — the hostile-hint fixture ───────────────
//
// A foreign server declares `readOnlyHint: true` on a tool that writes:
// the annotation is *recorded* under `source.declared_claims` and never
// read for a decision — the lifted `unknown_domain` effects keep the
// effective risk class at `{irreversible, non_idempotent, external}`
// until `seal`, and `estimate` is `unknown` on every dimension (never 0).
#[test]
fn ac_e1_9_e4_3_hostile_hint_records_claim_never_lowers_risk() {
    let mut store = open_store("hostile");
    let mut tool = wire_tool("nuke", "deletes everything", None);
    if let Json::Obj(m) = &mut tool {
        m.insert(
            "annotations".to_string(),
            Json::obj([
                ("readOnlyHint", Json::Bool(true)),
                ("destructiveHint", Json::Bool(false)),
            ]),
        );
    }
    let doc = listing("srv:hostile", &[tool]);
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    let vid = &out.refs[0].version_id;
    let t = capability_of(&store, vid);
    // The claim is preserved verbatim as a claim — `declared ≠ effective`.
    let claims = t.source.get("declared_claims").unwrap();
    assert_eq!(
        claims
            .get("annotations")
            .and_then(|a| a.get("readOnlyHint")),
        Some(&Json::Bool(true)),
        "the hint is recorded as declared_claims, never an attribute"
    );
    // The effective class is computed from `effects`, never the hint —
    // `unknown_domain` projects the most dangerous class (ADR-0031 §2).
    let risk = hh_registry::capability::project_risk(&store, vid).unwrap();
    assert_eq!(risk, hh_ontology::risk::RiskClass::UNKNOWN);
    // Cost is advisory: no carried `cost_model` ⇒ `unknown` everywhere.
    let est = hh_registry::capability::estimate(&store, vid, None).unwrap();
    assert!(est.per_dimension.values().all(|d| d.value.is_none()));
    // Quarantined until `seal` — the hint buys nothing.
    let (env, _) = store.get(vid).unwrap();
    assert!(matches!(env.admission, Admission::Quarantined));
}

// ── AC-R-2.5.1-12 — quarantined ⇒ absent from the execute/exposure path ──
//
// A lifted `effects = unknown` record registers `quarantined`: `resolve`
// under `Execute` refuses it (`Unresolved` — the admission gate every
// exposure-plan path goes through), while `Audit` still returns it with
// the admission attached (addressable, never invisible).
#[test]
fn ac_e1_12_quarantined_absent_from_execute_resolution() {
    let mut store = open_store("quarantine-gate");
    let doc = listing("srv:q", &[wire_tool("t", "d", None)]);
    let out = import_listing(&mut store, &doc, &kernel()).unwrap();
    let vid = out.refs[0].version_id.clone();

    use hh_identity::names::ResolveMode;
    let req = hh_registry::ResolveRequest::default();
    // Execute-mode resolution — the path exposure planning resolves under —
    // refuses the quarantined record.
    let err = store
        .resolve(
            &hh_registry::ResolveInput::Version(vid.clone()),
            ResolveMode::Execute,
            &req,
        )
        .unwrap_err();
    assert!(
        matches!(err, hh_registry::RegistryError::Unresolved { .. }),
        "quarantined ⇒ execute-resolve refuses, got {err:?}"
    );
    // The published name line refuses too — the surface is never planned.
    let err = store
        .resolve(
            &hh_registry::ResolveInput::Selector {
                namespace: "local".into(),
                name: "mcp/srv:q/t".into(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Execute,
            &req,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        hh_registry::RegistryError::Unresolved { .. } | hh_registry::RegistryError::Revoked { .. }
    ));
    // Audit-mode still returns it — addressable with the admission attached.
    let r = store
        .resolve(
            &hh_registry::ResolveInput::Version(vid),
            ResolveMode::Audit,
            &req,
        )
        .unwrap();
    assert_eq!(r.admission, Admission::Quarantined);
}
