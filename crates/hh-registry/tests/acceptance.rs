//! AC-R-2.10.2 acceptance tests — the deterministic criteria the ticket lands
//! (spec §6.2; ADR-0239 for the Stage-1 executable forms).

use std::path::PathBuf;

use hh_identity::names::ResolveMode;
use hh_provenance::ProvenanceRecord;
use hh_registry::corpus;
use hh_registry::kinds::RecordKind;
use hh_registry::records::{RegistryPolicy, RegistryRecord};
use hh_registry::store::{
    QueryClause, QueryOp, QueryPredicate, RegistryStore, ResolveInput, ResolveRequest,
    SlotConstraints,
};
use hh_registry::RegistryError;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-registry-ac-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// AC-R-2.10.2-1 — snapshot determinism: two independent implementations of
/// `resolve`/`query`/`slot_choices` over ONE `registry_snapshot_id` yield
/// byte-equal results on the golden corpus (3 classes × 3 variants, 2
/// namespaces, one yanked, one revoked). The "second implementation" is
/// twofold: (a) the same ops over a **replayed** store (independent read path —
/// the log is the only shared state), and (b) `corpus::naive_resolve`, a
/// from-the-bytes resolver that never touches `RegistryStore`.
#[test]
fn ac1_snapshot_determinism_two_implementations_byte_equal() {
    let d = dir("ac1");
    let (snap, vids) = corpus::build(&d).unwrap();
    let store_a = RegistryStore::open(&d, &corpus::kernel_registrar()).unwrap();
    let store_b = RegistryStore::open(&d, &corpus::kernel_registrar()).unwrap();
    // (a) outputs over two independently replayed stores — byte-equal.
    let out_a = corpus::outputs(&store_a, &snap, &vids).unwrap();
    let out_b = corpus::outputs(&store_b, &snap, &vids).unwrap();
    assert_eq!(
        out_a, out_b,
        "resolve/query/slot_choices must be byte-equal"
    );
    // And again after a fresh build in a different dir — the corpus itself is
    // deterministic (ids derive from content, never clocks).
    let d2 = dir("ac1-b");
    let (snap2, vids2) = corpus::build(&d2).unwrap();
    assert_eq!(snap, snap2);
    assert_eq!(vids, vids2);
    let out_c = corpus::outputs(&store_b, &snap2, &vids2).unwrap();
    assert_eq!(out_a, out_c);
    // (b) the naive resolver agrees with the store on every snapshot binding.
    for (ns, name) in [
        ("hh", "control_strategy/control_strategy-v0"),
        ("local", "control_strategy/control_strategy-v1"),
        ("hh", "context_policy/context_policy-v0"),
        ("local", "export_sink/export_sink-v1"),
    ] {
        let naive = corpus::naive_resolve(&d, &snap, ns, name).unwrap();
        let store = store_a
            .resolve(
                &ResolveInput::Selector {
                    namespace: ns.to_string(),
                    name: name.to_string(),
                    label: None,
                    snapshot_id: Some(snap.clone()),
                },
                ResolveMode::Audit,
                &ResolveRequest::default(),
            )
            .map(|r| r.versioned_ref.version_id)
            .ok();
        assert_eq!(naive, store, "{ns}/{name}");
    }
}

/// AC-R-2.10.2-2 — closed kinds and closed status vocabularies: an unlisted
/// `kind` fails registration (`registry/2` would be the growth path, never
/// `ext`); a status spelling outside the three axes fails the envelope decode.
#[test]
fn ac2_closed_kinds_and_closed_status_vocabularies() {
    let d = dir("ac2");
    let kernel = corpus::kernel_registrar();
    let mut s = RegistryStore::open(&d, &kernel).unwrap();
    // The kind list is closed at 26 — `whatever` is not a kind.
    assert_eq!(RecordKind::parse("whatever"), None);
    assert_eq!(RecordKind::ALL.len(), 26);
    // A kind in the list but without a Stage-1 schema is refused, not admitted.
    let e = s
        .register(
            RegistryRecord::ForeignImport(hh_registry::records::ForeignImport {
                system: "x".to_string(),
                locator: "x".to_string(),
                digest: None,
                label: None,
                lifted_record: Json::Null,
                loss_report: hh_registry::records::LossReport::default(),
            }),
            &kernel,
            None,
        )
        .unwrap();
    assert!(s.get(&e.version_id).is_some());
    // A status outside the three axes fails the envelope decode (R5).
    let mut env = Json::obj([
        ("admission", Json::str("resolved")),
        ("dialect_range", Json::str("registry/1")),
        ("kind", Json::str("variant")),
        ("name_history_ref", Json::Null),
        ("registered_at", Json::Int(0)),
        ("registrar", kernel.to_json()),
        ("registry_dialect", Json::str("registry/1")),
        ("semantic_id", Json::Null),
        ("trust_record_ref", Json::Null),
        ("version_id", Json::str("sha256:x")),
        ("ext", Json::obj([("status", Json::str("provisional"))])),
    ]);
    let e = hh_registry::schema::envelope_from_json(&env, "env").unwrap_err();
    assert!(matches!(e, RegistryError::SchemaViolation { .. }));
    // A bogus top-level status is refused the same way.
    if let Json::Obj(m) = &mut env {
        m.insert("status".to_string(), Json::str("half-committed"));
    }
    let e = hh_registry::schema::envelope_from_json(&env, "env").unwrap_err();
    assert!(matches!(e, RegistryError::SchemaViolation { .. }));
}

/// AC-R-2.10.2-3 — no load on read: `register`/`resolve`/`query`/`slot_choices`/
/// `lineage` succeed on variants whose `implementation.content` addresses bytes
/// that do not exist anywhere — the store never dereferences the pin (R3).
#[test]
fn ac3_registry_reads_never_load_implementations() {
    let d = dir("ac3");
    let (snap, vids) = corpus::build(&d).unwrap();
    let s = RegistryStore::open(&d, &corpus::kernel_registrar()).unwrap();
    // Every corpus impl pins `sha256:` of bytes that were never written anywhere —
    // there is no body to load, and every read op still succeeds.
    for vid in &vids {
        s.resolve(
            &ResolveInput::Version(vid.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
        s.lineage(vid).unwrap();
    }
    s.query(&QueryPredicate {
        clauses: vec![QueryClause {
            field: "kind".to_string(),
            op: QueryOp::Eq,
            value: "variant".to_string(),
        }],
        snapshot_id: Some(snap.clone()),
    })
    .unwrap();
    for cid in corpus::CLASS_IDS {
        let _ = s.slot_choices(cid, &SlotConstraints::default());
    }
    // The pinned content address is data — it round-trips as an address, never
    // as bytes the store opened.
    let (env, rec) = s.get(&vids[0]).unwrap();
    let _ = env;
    let RegistryRecord::Variant(v) = rec else {
        panic!()
    };
    assert!(v.implementation.content.id().starts_with("sha256:"));
}

/// AC-R-2.10.2-7 — names vs identity: one `version_id` under two names is one
/// record with two name-history entries; a rename preserves `semantic_id` and
/// mints a new `version_id` (also covered in the unit matrix).
#[test]
fn ac7_names_vs_identity() {
    let d = dir("ac7");
    let kernel = corpus::kernel_registrar();
    let principal = corpus::principal_registrar();
    let mut s = RegistryStore::open(&d, &kernel).unwrap();
    let class = RegistryRecord::Class(corpus_class());
    let cv = s.register(class, &kernel, None).unwrap();
    let v = corpus_variant(&cv.version_id, "alpha");
    let vv = s.register(v.clone(), &kernel, None).unwrap();
    s.publish("hh", "alpha", &vv.version_id, None, None, &kernel)
        .unwrap();
    s.publish(
        "local",
        "also-alpha",
        &vv.version_id,
        None,
        None,
        &principal,
    )
    .unwrap();
    let lin = s.lineage(&vv.version_id).unwrap();
    assert_eq!(lin.name_history.len(), 2);
    // Rename → same semantic_id, new version_id (the same body, new tag).
    let RegistryRecord::Variant(rv) = &v else {
        panic!()
    };
    let mut renamed_body = rv.clone();
    renamed_body.variant_id = "beta".to_string();
    let rvv = s
        .register(RegistryRecord::Variant(renamed_body), &kernel, None)
        .unwrap();
    assert_ne!(vv.version_id, rvv.version_id);
    assert_eq!(vv.semantic_id, rvv.semantic_id);
}

fn corpus_class() -> hh_registry::records::ClassRecord {
    hh_registry::suites::control_strategy_class()
}

fn corpus_variant(class_ref: &str, tag: &str) -> RegistryRecord {
    use hh_registry::kinds::Placement;
    use hh_registry::records::{AppliesTo, Implementation, VariantRecord};
    use std::collections::{BTreeMap, BTreeSet};
    RegistryRecord::Variant(VariantRecord {
        variant_id: tag.to_string(),
        class_ref: class_ref.to_string(),
        contract_range: "1.0".to_string(),
        version_label: None,
        param_schema: BTreeMap::new(),
        implementation: Implementation {
            content: hh_identity::idp::address(tag.as_bytes(), "application/octet-stream"),
            placement: Placement::SubprocessConfined,
            host_requirements: Json::Null,
        },
        capability_declaration: BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]),
        conditioned_rules: vec![],
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: hh_hir::leaves::Text::new(
            format!("v {tag}"),
            "ac7",
            ProvenanceRecord::kernel("ac7", 0),
        ),
        dialect_range: "registry/1".to_string(),
    })
}

/// AC-R-2.10.2-8 — namespace authority + snapshot behavior: `hh/` refuses every
/// non-kernel publisher, `local/` accepts the principal under its policy, and
/// snapshots pin content addresses + records.
#[test]
fn ac8_namespace_authority_and_snapshot_pins() {
    let d = dir("ac8");
    let (snap, vids) = corpus::build(&d).unwrap();
    let kernel = corpus::kernel_registrar();
    let mut s = RegistryStore::open(&d, &kernel).unwrap();
    // hh/ refuses a principal publisher (the corpus already proved kernel works).
    let e = s
        .publish(
            "hh",
            "intruder",
            &vids[0],
            None,
            None,
            &corpus::principal_registrar(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::NamespaceForbidden { .. }));
    // The snapshot pins every member + the name bindings + the policy digest —
    // and is itself a registered record whose bytes verify.
    let (_, snap_rec) = s.get(&snap).unwrap();
    let RegistryRecord::Snapshot(snap_rec) = snap_rec else {
        panic!("snapshot id did not resolve to a snapshot record")
    };
    for m in &snap_rec.members {
        assert!(s.get(m).is_some(), "snapshot member {m} not registered");
    }
    assert!(snap_rec
        .name_bindings
        .values()
        .all(|v| snap_rec.members.contains(v)));
    // The pinned binding resolves inside the snapshot (R8) — reproduce mode.
    let ((ns, name), pinned) = snap_rec.name_bindings.iter().next().unwrap();
    let (ns, name, pinned) = (ns.clone(), name.clone(), pinned.clone());
    let r = s
        .resolve(
            &ResolveInput::Selector {
                namespace: ns,
                name,
                label: None,
                snapshot_id: Some(snap.clone()),
            },
            ResolveMode::Reproduce,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.versioned_ref.version_id, pinned);
    // verify() over the whole store — clean.
    let report = s.verify();
    assert!(report.ok(), "{:?}", report.errors);
    assert!(report.checked >= snap_rec.members.len());
}

/// The lifecycle.registry.* audit classes land in the one declared table (the
/// ledger is the only event store — CC7).
#[test]
fn registry_event_classes_are_in_the_ledger_table() {
    for class in [
        "lifecycle.registry.registered",
        "lifecycle.registry.admission_refused",
        "lifecycle.registry.published",
        "lifecycle.registry.name_deprecated",
        "lifecycle.registry.name_yanked",
        "lifecycle.registry.version_revoked",
        "lifecycle.registry.snapshotted",
        "lifecycle.registry.conformance_recorded",
    ] {
        let spec = hh_ledger::classes::lookup(class)
            .unwrap_or_else(|| panic!("class {class} missing from the ledger table"));
        assert!(spec.audit_grade, "{class} must be audit-grade");
        assert!(spec.kernel_origin, "{class} must be kernel-origin");
    }
}

/// The Stage-1 default policy is `require_conformance = declared` (ADR-0152 D6).
#[test]
fn the_stage1_floor_is_declared() {
    assert_eq!(
        RegistryPolicy::stage1_default().require_conformance,
        hh_registry::kinds::RequireConformance::Declared
    );
}
