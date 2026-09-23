//! `hh-registry` store tests — every behavior has a fail-if-removed test (the
//! ticket's test matrix; spec §6.2 R1–R10, ADR-0239). Determinism: every id is
//! content-derived; `tempdir` paths are the only non-canonical input.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_identity::idp::address;
use hh_identity::names::{NameStatus, ResolveMode};
use hh_identity::supersede::SupersedeReason;
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::kinds::{
    Admission, Cardinality, ConformanceVerdict, OracleClass, Placement, ProducedBy,
    RequireConformance, SubjectKind, TestDriver, TestKind,
};
use hh_registry::records::{
    AppliesTo, ClassRecord, ConformanceReport, ConformanceSuite, ContractOperation, ForeignImport,
    Implementation, ParamDecl, RegistryPolicy, RegistryRecord, ReportHost, ReportResult, SuiteTest,
    VariantRecord,
};
use hh_registry::store::{
    QueryClause, QueryOp, QueryPredicate, RegistryStore, ResolveInput, ResolveRequest,
    SlotConstraints,
};
use hh_registry::RegistryError;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-registry-test-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("registry.test", 0)
}

fn principal(name: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(name, HumanRole::Principal),
        PersistenceScope::Run,
        0,
    )
}

fn third_party() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::import("mcp_registry", "1.0"),
        PersistenceScope::Run,
        0,
    )
}

fn text(s: &str, prov: &ProvenanceRecord) -> Text {
    Text::new(s, "test", prov.clone())
}

fn class(id: &str) -> ClassRecord {
    ClassRecord {
        class_id: id.to_string(),
        contract: vec![ContractOperation {
            name: "run".to_string(),
            inputs: Json::Null,
            outputs: Json::Null,
            invariants: vec![],
            failure_modes: vec![],
        }],
        cardinality: Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            ("additionalProperties", Json::Bool(false)),
            (
                "properties",
                Json::obj([("deterministic", Json::Null), ("supports_x", Json::Null)]),
            ),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec![],
        slot_key: id.to_string(),
        tier: "C0".to_string(),
        depends_on: Vec::new(),
    }
}

fn variant(class_ref: &str, tag: &str, placement: Placement) -> VariantRecord {
    VariantRecord {
        variant_id: tag.to_string(),
        class_ref: class_ref.to_string(),
        contract_range: "1.0".to_string(),
        version_label: Some("1.0.0".to_string()),
        param_schema: BTreeMap::new(),
        implementation: Implementation {
            content: address(format!("impl-{tag}").as_bytes(), "application/octet-stream"),
            placement,
            host_requirements: Json::Null,
        },
        capability_declaration: BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]),
        conditioned_rules: vec![],
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: text(&format!("v {tag}"), &kernel()),
        dialect_range: "registry/1".to_string(),
    }
}

fn register_class(store: &mut RegistryStore, id: &str) -> String {
    store
        .register(RegistryRecord::Class(class(id)), &kernel(), None)
        .unwrap()
        .version_id
}

#[test]
fn register_is_idempotent_on_equal_canonical_bytes() {
    let d = dir("idem");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = variant(&c, "v1", Placement::SubprocessConfined);
    let a = s
        .register(RegistryRecord::Variant(v.clone()), &kernel(), None)
        .unwrap();
    let b = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap();
    assert_eq!(a.version_id, b.version_id);
    assert_eq!(s.version_ids().count(), 2 + 2); // 2 namespaces + class + variant
}

#[test]
fn register_refuses_a_variant_of_an_unknown_class() {
    let d = dir("unknown-class");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let v = variant("sha256:deadbeef", "v1", Placement::SubprocessConfined);
    let e = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::UnknownClass { .. }));
    assert_eq!(s.diagnostics().len(), 1);
    assert_eq!(s.diagnostics()[0].reason, "UnknownClass");
}

#[test]
fn register_refuses_empty_contract_range_intersection() {
    let d = dir("contract-range");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let mut v = variant(&c, "v1", Placement::SubprocessConfined);
    v.contract_range = "2.0".to_string();
    let e = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::ContractIncompatible { .. }));
}

#[test]
fn register_refuses_conditioned_rule_without_complete_debt() {
    let d = dir("debt");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let mut v = variant(&c, "v1", Placement::SubprocessConfined);
    v.conditioned_rules = vec![(
        "r1".to_string(),
        AssumptionDebtRecord {
            rule_id: "r1".to_string(),
            hypothesis: text("", &kernel()),
            evidence_refs: vec![],
            owner: hh_hir::OwnerRef::principal(""), // incomplete: empty owner
            expiry_condition: hh_hir::ExpiryCondition {
                kind: hh_hir::ExpiryKind::EvidenceRefreshDue,
                value: None,
            },
            removal_test_ref: "t".to_string(),
            status: hh_hir::records::DebtStatus::Active,
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
        },
    )];
    let e = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::ConditionedRuleIncomplete { .. }));
}

#[test]
fn third_party_register_requires_trust_record_and_confines_placement() {
    let d = dir("third-party");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    // No trust record → TrustRecordRequired.
    let v = variant(&c, "v1", Placement::SubprocessConfined);
    let e = s
        .register(RegistryRecord::Variant(v), &third_party(), None)
        .unwrap_err();
    assert_eq!(e, RegistryError::TrustRecordRequired);
    // With trust record, in_process third-party code → LocalityInadmissible.
    let v = variant(&c, "v1", Placement::InProcess);
    let e = s
        .register(
            RegistryRecord::Variant(v),
            &third_party(),
            Some("sha256:trust".to_string()),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::LocalityInadmissible { .. }));
    // Confined placement + trust → registers quarantined.
    let v = variant(&c, "v1", Placement::SubprocessConfined);
    let r = s
        .register(
            RegistryRecord::Variant(v),
            &third_party(),
            Some("sha256:trust".to_string()),
        )
        .unwrap();
    let (env, _) = s.get(&r.version_id).unwrap();
    assert_eq!(env.admission, Admission::Quarantined);
}

#[test]
fn reserved_component_model_placement_is_inadmissible_for_everyone() {
    let d = dir("component-model");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = variant(&c, "v1", Placement::ComponentModel);
    let e = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::LocalityInadmissible { .. }));
}

#[test]
fn declaration_schema_refuses_undeclared_and_missing_fields() {
    let d = dir("decl-schema");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let mut v = variant(&c, "v1", Placement::SubprocessConfined);
    v.capability_declaration
        .insert("undeclared_field".to_string(), Json::Bool(true));
    let e = s
        .register(RegistryRecord::Variant(v.clone()), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::SchemaViolation { .. }));
    v.capability_declaration.clear(); // missing required `deterministic`
    let e = s
        .register(RegistryRecord::Variant(v), &kernel(), None)
        .unwrap_err();
    assert!(matches!(e, RegistryError::SchemaViolation { .. }));
}

#[test]
fn publish_hh_requires_kernel_and_local_accepts_principal() {
    let d = dir("namespaces");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    // A principal may not write hh/.
    let e = s
        .publish(
            "hh",
            "c1/v1",
            &v.version_id,
            None,
            None,
            &principal("alice"),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::NamespaceForbidden { .. }));
    // Kernel writes hh/.
    s.publish("hh", "c1/v1", &v.version_id, None, None, &kernel())
        .unwrap();
    // Principal writes local/.
    s.publish(
        "local",
        "c1/v1",
        &v.version_id,
        None,
        None,
        &principal("alice"),
    )
    .unwrap();
}

#[test]
fn publish_under_another_publishers_name_collides() {
    let d = dir("collision");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "thing",
        &v.version_id,
        None,
        None,
        &principal("alice"),
    )
    .unwrap();
    // A different principal publishing the same name → NameCollision.
    let e = s
        .publish(
            "local",
            "thing",
            &v.version_id,
            None,
            None,
            &principal("bob"),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::NameCollision { .. }));
    // The owner may republish (rename/version bump path).
    s.publish(
        "local",
        "thing",
        &v.version_id,
        Some("1.0.0".to_string()),
        Some(v.version_id.clone()),
        &principal("alice"),
    )
    .unwrap();
}

#[test]
fn labels_are_unique_per_name() {
    let d = dir("labels");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "n",
        &v.version_id,
        Some("1.0".to_string()),
        None,
        &principal("alice"),
    )
    .unwrap();
    let e = s
        .publish(
            "local",
            "n",
            &v.version_id,
            Some("1.0".to_string()),
            None,
            &principal("alice"),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::LabelReused { .. }));
}

#[test]
fn execute_resolve_skips_a_yanked_head_and_audit_sees_it() {
    let d = dir("yank");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let p = principal("alice");
    s.publish("local", "n", &v1.version_id, None, None, &p)
        .unwrap();
    s.publish(
        "local",
        "n",
        &v2.version_id,
        None,
        Some(v1.version_id.clone()),
        &p,
    )
    .unwrap();
    s.set_name_status("local", "n", NameStatus::Yanked, &p)
        .unwrap();
    // execute → falls back to the non-yanked entry (v1).
    let r = s
        .resolve(
            &ResolveInput::Selector {
                namespace: "local".to_string(),
                name: "n".to_string(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.versioned_ref.version_id, v1.version_id);
    // audit → the yanked head (v2), status attached.
    let r = s
        .resolve(
            &ResolveInput::Selector {
                namespace: "local".to_string(),
                name: "n".to_string(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.versioned_ref.version_id, v2.version_id);
    assert_eq!(r.name_status, Some(NameStatus::Yanked));
}

#[test]
fn revoke_refuses_execute_resolves_audit_and_marks_dependants_stale() {
    let d = dir("revoke");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    // A dependant (a report naming the variant) is stale-by-dependency, never hidden.
    let suite = ConformanceSuite {
        suite_id: "s1".to_string(),
        class_ref: c.clone(),
        contract_version: "1.0".to_string(),
        tests: vec![],
        required_for_status: BTreeSet::new(),
    };
    let sv = s
        .register(RegistryRecord::Suite(suite), &kernel(), None)
        .unwrap();
    let report = ConformanceReport {
        report_id: "r1".to_string(),
        subject_kind: SubjectKind::Variant,
        subject_ref: v.version_id.clone(),
        suite_ref: sv.version_id.clone(),
        host: ReportHost {
            placement: Placement::SubprocessConfined,
            isolation: "x".to_string(),
            instrument: hh_identity::repro::InstrumentRecord::new("1", "abc", false),
        },
        produced_by: ProducedBy::RegistryCi,
        results: vec![],
        probed_declaration: BTreeMap::new(),
        run_id: "run1".to_string(),
        stale: false,
        hosted_entries: Vec::new(),
    };
    let rv = s
        .register(RegistryRecord::Report(report), &kernel(), None)
        .unwrap();
    s.revoke(&v.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    // execute → Revoked.
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::Revoked { .. }));
    // audit → resolves with the derived revoked admission.
    let r = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.admission, Admission::Revoked);
    // The dependant report is stale-by-dependency (S4 — annotated, never hidden).
    let lin = s.lineage(&rv.version_id).unwrap();
    assert_eq!(lin.stale.len(), 1);
    assert_eq!(lin.stale[0].revoked_member, v.version_id);
    let e = s
        .resolve(
            &ResolveInput::Version(rv.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::Stale { .. }));
}

#[test]
fn quarantined_records_never_execute() {
    let d = dir("quarantine");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &third_party(),
            Some("sha256:trust".to_string()),
        )
        .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::Unresolved { .. }));
    let r = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.admission, Admission::Quarantined);
}

#[test]
fn a_rename_keeps_semantic_id_and_mints_a_new_version_id() {
    let d = dir("rename");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = variant(&c, "alpha", Placement::SubprocessConfined);
    let a = s
        .register(RegistryRecord::Variant(v.clone()), &kernel(), None)
        .unwrap();
    // Rename (variant_id) → new version_id, SAME semantic_id (AC-7).
    let mut renamed = v.clone();
    renamed.variant_id = "beta".to_string();
    let b = s
        .register(RegistryRecord::Variant(renamed), &kernel(), None)
        .unwrap();
    assert_ne!(a.version_id, b.version_id);
    assert_eq!(a.semantic_id, b.semantic_id);
    assert!(a.semantic_id.is_some());
    // Summary-only change → also a new version_id, same semantic_id.
    let mut edited = v.clone();
    edited.summary = text("edited", &kernel());
    let e = s
        .register(RegistryRecord::Variant(edited), &kernel(), None)
        .unwrap();
    assert_ne!(a.version_id, e.version_id);
    assert_eq!(a.semantic_id, e.semantic_id);
    // A core change (param_schema) → new semantic_id too.
    let mut core = v.clone();
    core.param_schema.insert(
        "depth".to_string(),
        ParamDecl {
            value_type: "int".to_string(),
            domain: None,
            default: None,
            unit: None,
            sweepable: true,
            budget_relevant: false,
            affects: vec![],
        },
    );
    let k = s
        .register(RegistryRecord::Variant(core), &kernel(), None)
        .unwrap();
    assert_ne!(a.semantic_id, k.semantic_id);
}

#[test]
fn one_version_id_lives_under_two_names_with_two_history_entries() {
    let d = dir("two-names");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("hh", "a", &v.version_id, None, None, &kernel())
        .unwrap();
    s.publish("local", "b", &v.version_id, None, None, &principal("alice"))
        .unwrap();
    // One record, two name-history entries (AC-7).
    let lin = s.lineage(&v.version_id).unwrap();
    assert_eq!(lin.name_history.len(), 2);
    for input in [
        ResolveInput::Selector {
            namespace: "hh".to_string(),
            name: "a".to_string(),
            label: None,
            snapshot_id: None,
        },
        ResolveInput::Selector {
            namespace: "local".to_string(),
            name: "b".to_string(),
            label: None,
            snapshot_id: None,
        },
    ] {
        let r = s
            .resolve(&input, ResolveMode::Execute, &ResolveRequest::default())
            .unwrap();
        assert_eq!(r.versioned_ref.version_id, v.version_id);
    }
}

#[test]
fn snapshot_pins_members_bindings_and_policy_and_is_deterministic() {
    let d = dir("snap");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("hh", "n", &v.version_id, None, None, &kernel())
        .unwrap();
    let s1 = s.snapshot(&kernel()).unwrap();
    let s2 = s.snapshot(&kernel()).unwrap();
    // State changed (the snapshot itself is a member now) → a new pin.
    assert_ne!(s1.snapshot_id, s2.snapshot_id);
    assert!(s1.members.contains(&v.version_id));
    assert_eq!(
        s1.name_bindings.get(&("hh".to_string(), "n".to_string())),
        Some(&v.version_id)
    );
    assert!(!s1.policy_digest.is_empty());
    // A snapshot is itself a registered record (pinned in the store).
    assert!(s.get(&s1.snapshot_id).is_some());
    // Deterministic mint: an identical history in another store mints the same id.
    let d2 = dir("snap-b");
    let mut t = RegistryStore::open(&d2, &kernel()).unwrap();
    let tc = register_class(&mut t, "c1");
    let tv = t
        .register(
            RegistryRecord::Variant(variant(&tc, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    t.publish("hh", "n", &tv.version_id, None, None, &kernel())
        .unwrap();
    let t1 = t.snapshot(&kernel()).unwrap();
    assert_eq!(s1.snapshot_id, t1.snapshot_id);
}

#[test]
fn resolve_inside_a_snapshot_uses_the_pinned_binding() {
    let d = dir("snap-resolve");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("local", "n", &v1.version_id, None, None, &principal("a"))
        .unwrap();
    let snap = s.snapshot(&kernel()).unwrap();
    // Move the head after the snapshot.
    s.publish(
        "local",
        "n",
        &v2.version_id,
        None,
        Some(v1.version_id.clone()),
        &principal("a"),
    )
    .unwrap();
    // Head resolve → v2; snapshot-confined → the pinned v1 (R8 determinism).
    let head = s
        .resolve(
            &ResolveInput::Selector {
                namespace: "local".to_string(),
                name: "n".to_string(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(head.versioned_ref.version_id, v2.version_id);
    let pinned = s
        .resolve(
            &ResolveInput::Selector {
                namespace: "local".to_string(),
                name: "n".to_string(),
                label: None,
                snapshot_id: Some(snap.snapshot_id.clone()),
            },
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(pinned.versioned_ref.version_id, v1.version_id);
}

#[test]
fn verify_detects_tampering_and_reopen_preserves_state() {
    let d = dir("verify");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("hh", "n", &v.version_id, None, None, &kernel())
        .unwrap();
    let rep = s.verify();
    assert!(rep.ok(), "{:?}", rep.errors);
    // Reopen: the replayed store resolves identically (persistence).
    let s2 = RegistryStore::open(&d, &kernel()).unwrap();
    let rep2 = s2.verify();
    assert!(rep2.ok(), "{:?}", rep2.errors);
    let r = s2
        .resolve(
            &ResolveInput::Selector {
                namespace: "hh".to_string(),
                name: "n".to_string(),
                label: None,
                snapshot_id: None,
            },
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.versioned_ref.version_id, v.version_id);
    // Tamper: flip a hex digit inside a sha256 field — the log still parses,
    // but the pinned content no longer verifies.
    let path = d.join("registry.json");
    let mut bytes = std::fs::read(&path).unwrap();
    let marker = b"sha256:";
    let pos = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .expect("a sha256 field exists");
    let i = pos + marker.len();
    bytes[i] = if bytes[i] == b'0' { b'1' } else { b'0' };
    std::fs::write(&path, &bytes).unwrap();
    let s3 = RegistryStore::open(&d, &kernel()).unwrap();
    let rep3 = s3.verify();
    assert!(!rep3.ok(), "tampered log must fail verify: {rep3:?}");
}

#[test]
fn foreign_import_registers_quarantined() {
    let d = dir("foreign");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let fi = ForeignImport {
        system: "mcp_registry".to_string(),
        locator: "mcp://server/x".to_string(),
        digest: Some("sha256:abc".to_string()),
        label: None,
        lifted_record: Json::obj([("name", Json::str("x"))]),
        loss_report: vec!["digest".to_string()],
    };
    let r = s
        .register(RegistryRecord::ForeignImport(fi), &kernel(), None)
        .unwrap();
    let (env, _) = s.get(&r.version_id).unwrap();
    assert_eq!(env.admission, Admission::Quarantined);
}

#[test]
fn query_filters_by_declared_fields_and_refuses_unknown_ones() {
    let d = dir("query");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    s.register(
        RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
        &kernel(),
        None,
    )
    .unwrap();
    let rows = s
        .query(&QueryPredicate {
            clauses: vec![
                QueryClause {
                    field: "kind".to_string(),
                    op: QueryOp::Eq,
                    value: "variant".to_string(),
                },
                QueryClause {
                    field: "variant_id".to_string(),
                    op: QueryOp::Eq,
                    value: "v1".to_string(),
                },
            ],
            snapshot_id: None,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    let e = s
        .query(&QueryPredicate {
            clauses: vec![QueryClause {
                field: "free_text".to_string(),
                op: QueryOp::Eq,
                value: "x".to_string(),
            }],
            snapshot_id: None,
        })
        .unwrap_err();
    assert!(matches!(e, RegistryError::UnknownField { .. }));
}

#[test]
fn slot_choices_returns_sorted_satisfying_variants() {
    let d = dir("slots");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let mut cls = class("c1");
    // A suite must exist for the default `declared` floor.
    let cv = s
        .register(RegistryRecord::Class(cls.clone()), &kernel(), None)
        .unwrap();
    let suite = ConformanceSuite {
        suite_id: "s1".to_string(),
        class_ref: cv.version_id.clone(),
        contract_version: "1.0".to_string(),
        tests: vec![SuiteTest {
            test_id: "t1".to_string(),
            kind: TestKind::Static,
            fixture_ref: None,
            driver: TestDriver::InProcess,
            oracle_class: OracleClass::Deterministic,
            budget: Json::Null,
            verdict_rule: "x".to_string(),
        }],
        required_for_status: BTreeSet::from(["t1".to_string()]),
    };
    let sv = s
        .register(RegistryRecord::Suite(suite), &kernel(), None)
        .unwrap();
    cls.conformance_suite_ref = Some(sv.version_id.clone());
    s.register(RegistryRecord::Class(cls), &kernel(), None)
        .unwrap();
    // Two variants: one with supports_x, one without.
    let mut v1 = variant(&cv.version_id, "v1", Placement::SubprocessConfined);
    v1.capability_declaration
        .insert("supports_x".to_string(), Json::Bool(true));
    s.register(RegistryRecord::Variant(v1), &kernel(), None)
        .unwrap();
    let v2 = variant(&cv.version_id, "v2", Placement::SubprocessConfined);
    s.register(RegistryRecord::Variant(v2), &kernel(), None)
        .unwrap();
    let all = s.slot_choices("c1", &SlotConstraints::default()).unwrap();
    assert_eq!(all.len(), 2);
    let filtered = s
        .slot_choices(
            "c1",
            &SlotConstraints {
                supports: BTreeSet::from(["supports_x".to_string()]),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].variant_id, "v1");
    assert!(filtered[0].version_id.starts_with("sha256:"));
}

#[test]
fn substitutable_requires_same_class_and_covered_declarations() {
    let d = dir("subst");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c1 = register_class(&mut s, "c1");
    let c2 = register_class(&mut s, "c2");
    let va = variant(&c1, "a", Placement::SubprocessConfined);
    let mut vb = variant(&c1, "b", Placement::SubprocessConfined);
    vb.capability_declaration
        .insert("supports_x".to_string(), Json::Bool(true));
    let vc = variant(&c2, "c", Placement::SubprocessConfined);
    let a = s
        .register(RegistryRecord::Variant(va.clone()), &kernel(), None)
        .unwrap();
    let b = s
        .register(RegistryRecord::Variant(vb.clone()), &kernel(), None)
        .unwrap();
    let cc = s
        .register(RegistryRecord::Variant(vc), &kernel(), None)
        .unwrap();
    // b covers a's declaration (a ⊆ b) — substitutable; not the converse.
    assert!(s.substitutable(&a.version_id, &b.version_id).unwrap());
    assert!(!s.substitutable(&b.version_id, &a.version_id).unwrap());
    // Different class → never substitutable.
    assert!(!s.substitutable(&a.version_id, &cc.version_id).unwrap());
}

#[test]
fn probed_floor_reads_only_admissible_non_stale_reports() {
    let d = dir("probed");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let mut p = RegistryPolicy::stage1_default();
    p.require_conformance = RequireConformance::Probed;
    s.set_policy(p).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    // publisher_claim never satisfies probed.
    let report = |by: ProducedBy, stale: bool| ConformanceReport {
        report_id: format!("r-{by:?}-{stale}"),
        subject_kind: SubjectKind::Variant,
        subject_ref: v.version_id.clone(),
        suite_ref: c.clone(),
        host: ReportHost {
            placement: Placement::SubprocessConfined,
            isolation: "x".to_string(),
            instrument: hh_identity::repro::InstrumentRecord::new("1", "abc", false),
        },
        produced_by: by,
        results: vec![ReportResult {
            subject: "t".to_string(),
            verdict: ConformanceVerdict::Supported,
            evidence_ref: None,
        }],
        probed_declaration: BTreeMap::from([(
            "deterministic".to_string(),
            ConformanceVerdict::Supported,
        )]),
        run_id: "run".to_string(),
        stale,
        hosted_entries: Vec::new(),
    };
    s.register(
        RegistryRecord::Report(report(ProducedBy::PublisherClaim, false)),
        &kernel(),
        None,
    )
    .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest {
                conformance_floor: Some(RequireConformance::Probed),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::ConformanceBelowFloor { .. }));
    // A stale registry_ci report satisfies nothing → SuiteStale.
    s.register(
        RegistryRecord::Report(report(ProducedBy::RegistryCi, true)),
        &kernel(),
        None,
    )
    .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest {
                conformance_floor: Some(RequireConformance::Probed),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::SuiteStale));
    // A fresh admissible report satisfies.
    s.register(
        RegistryRecord::Report(report(ProducedBy::RegistryCi, false)),
        &kernel(),
        None,
    )
    .unwrap();
    s.resolve(
        &ResolveInput::Version(v.version_id.clone()),
        ResolveMode::Execute,
        &ResolveRequest {
            conformance_floor: Some(RequireConformance::Probed),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn every_refused_operation_appends_a_diagnostic_and_an_event() {
    let d = dir("diag");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let _ = s
        .resolve(
            &ResolveInput::Version("sha256:missing".to_string()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    let _ = s.publish("hh", "n", "sha256:missing", None, None, &kernel());
    assert!(!s.diagnostics().is_empty());
    let events = s.drain_events();
    assert!(events
        .iter()
        .any(|e| e.class == "lifecycle.registry.admission_refused"));
    assert!(s.drain_events().is_empty()); // drained
                                          // Refusals persist — reopening replays the diagnostic records (R10).
    let s2 = RegistryStore::open(&d, &kernel()).unwrap();
    assert!(!s2.diagnostics().is_empty());
}

#[test]
fn the_suite_driver_produces_a_registerable_report() {
    let d = dir("driver");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let mut cls = hh_registry::suites::control_strategy_class();
    let cv = s
        .register(RegistryRecord::Class(cls.clone()), &kernel(), None)
        .unwrap();
    let mut suite = hh_registry::suites::control_strategy_suite(&cv.version_id);
    suite.class_ref = cv.version_id.clone();
    let sv = s
        .register(RegistryRecord::Suite(suite.clone()), &kernel(), None)
        .unwrap();
    cls.conformance_suite_ref = Some(sv.version_id.clone());
    s.register(RegistryRecord::Class(cls), &kernel(), None)
        .unwrap();
    let v = variant(&cv.version_id, "v1", Placement::SubprocessConfined);
    let vv = s
        .register(RegistryRecord::Variant(v.clone()), &kernel(), None)
        .unwrap();
    let instrument = hh_identity::repro::InstrumentRecord::new("0.1", "dead", false);
    let mut report =
        hh_registry::suites::run_suite(&suite, &v, &vv.version_id, "1.0", &instrument, "run-1");
    report.suite_ref = sv.version_id.clone();
    assert!(report
        .results
        .iter()
        .all(|r| r.verdict == ConformanceVerdict::Supported));
    let rv = s
        .register(RegistryRecord::Report(report), &kernel(), None)
        .unwrap();
    // The derived vector now reports the declared fields as probed-SUPPORTED.
    let r = s
        .resolve(
            &ResolveInput::Version(vv.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest {
                conformance_floor: Some(RequireConformance::Probed),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        r.conformance_vector.get("deterministic"),
        Some(&ConformanceVerdict::Supported)
    );
    assert!(s.get(&rv.version_id).is_some());
}

#[test]
fn widening_successor_requires_attested_principal() {
    let d = dir("widening");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let remote = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::Remote)),
            &kernel(),
            None,
        )
        .unwrap();
    let local_impl = s
        .register(
            RegistryRecord::Variant(variant(&c, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("hh", "n", &remote.version_id, None, None, &kernel())
        .unwrap();
    // remote → subprocess_confined widens privilege → attested principal required.
    let e = s
        .publish(
            "hh",
            "n",
            &local_impl.version_id,
            None,
            Some(remote.version_id.clone()),
            &kernel(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::AuthorityWideningRequiresHuman));
}

#[test]
fn lineage_reports_supersedes_revocations_and_name_entries() {
    let d = dir("lineage");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish("hh", "n", &v1.version_id, None, None, &kernel())
        .unwrap();
    s.publish(
        "hh",
        "n",
        &v2.version_id,
        None,
        Some(v1.version_id.clone()),
        &kernel(),
    )
    .unwrap();
    let lin = s.lineage(&v1.version_id).unwrap();
    assert_eq!(lin.superseded_by, vec![v2.version_id.clone()]);
    assert_eq!(lin.name_history.len(), 1);
    let lin2 = s.lineage(&v2.version_id).unwrap();
    assert_eq!(lin2.supersedes, vec![v1.version_id.clone()]);
    s.revoke(
        &v1.version_id,
        SupersedeReason::Revocation,
        &kernel(),
        Some(v2.version_id.clone()),
    )
    .unwrap();
    let lin3 = s.lineage(&v1.version_id).unwrap();
    assert_eq!(lin3.revocations.len(), 1);
    assert_eq!(lin3.revocations[0].replacement, Some(v2.version_id.clone()));
}

#[test]
fn environment_family_registers_and_a_family_scoped_metric_resolves() {
    // DF-S1.22-3 (closed at S1.24): `MetricDeclaration.applies_to_families`
    // carries typed `EnvironmentFamily` values; a family-scoped metric
    // resolves against the registered `environment_family` record; the
    // canonical spellings are unchanged, so old name-spelling bodies decode.
    use hh_ontology::compliance::MetricDeclaration;
    use hh_ontology::lab::{
        EnvironmentFamily, EnvironmentFamilyRecord, EpisodeModel, SubmissionKind, VerifierIsolation,
    };

    let d = dir("env-family");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let family = EnvironmentFamilyRecord {
        family_id: EnvironmentFamily::CodingTerminal,
        version: "1".into(),
        requires: BTreeMap::new(),
        oracle_classes_available: vec!["executable".into(), "end_state".into()],
        submission_kind: SubmissionKind::Patch,
        verifier_isolation_default: VerifierIsolation::Separate,
        process_metrics: vec!["wall_seconds".into()],
        fault_profiles_admissible: vec![],
        perturbation_profiles_admissible: vec![],
        episode_model: EpisodeModel::SingleEpisode,
        provenance: kernel(),
    };
    s.register(RegistryRecord::EnvironmentFamily(family), &kernel(), None)
        .unwrap();
    assert!(s
        .resolve_environment_family(EnvironmentFamily::CodingTerminal)
        .is_some());
    assert!(s
        .resolve_environment_family(EnvironmentFamily::SearchResearch)
        .is_none());

    // A family-scoped metric resolves through the registry against the
    // registered record — never a bare name.
    let metric = MetricDeclaration {
        name: "family.scoped.metric".into(),
        applies_to_families: [EnvironmentFamily::CodingTerminal].into_iter().collect(),
        ..MetricDeclaration::default()
    };
    for f in &metric.applies_to_families {
        assert!(
            s.resolve_environment_family(*f).is_some(),
            "family {f:?} must resolve against a registered record"
        );
    }
    assert!(!metric
        .applies_to_families
        .contains(&EnvironmentFamily::SearchResearch));

    // The canonical spellings are unchanged — a name-spelling body decodes
    // into the typed set (the DF-S1.22-3 migration is additive, CC8).
    let j = metric.to_json();
    let body = match &j {
        Json::Obj(m) => m.get("applies_to_families").cloned().unwrap_or(Json::Null),
        _ => Json::Null,
    };
    assert_eq!(body, Json::Arr(vec![Json::str("coding_terminal")]));
    let back = MetricDeclaration::from_json(&j).unwrap();
    assert_eq!(back.applies_to_families, metric.applies_to_families);
}

// ── S3.4d — participant/adapter opaque kinds + hosted conformance (§6.6) ────

use hh_registry::kinds::RecordKind;
use hh_registry::records::{ConformanceRecord, ObservedIn};
use hh_registry::schema::record_from_json;

fn participant_body(id: &str) -> Json {
    Json::obj([
        ("kind", Json::str("participant")),
        ("participant_id", Json::str(id)),
        ("participant_version", Json::str("1.0.0")),
        ("version_identity", Json::str("idp:test")),
        (
            "descriptor",
            Json::obj([
                ("class", Json::str("hosted")),
                ("hosting_mechanism", Json::str("session_abi")),
                (
                    "observability_level",
                    Json::Arr(vec![Json::str("events"), Json::str("end_state")]),
                ),
                ("capability_vector", Json::obj([])),
            ]),
        ),
        ("capability_declaration", Json::obj([])),
        ("ext", Json::obj([])),
    ])
}

#[test]
fn participant_and_adapter_bodies_register_opaque() {
    // The structural gate: the body's `kind` tag must match the record kind;
    // the body itself is opaque (hh-hosting owns the schema — CC7 layering).
    let p = record_from_json(RecordKind::Participant, &participant_body("hh.reference"))
        .expect("participant body");
    assert!(matches!(p, RegistryRecord::Participant(_)));
    // A mistagged body refuses — a participant body can never register as an
    // adapter (and vice versa).
    let mistagged = Json::obj([
        ("kind", Json::str("adapter")),
        ("participant_id", Json::str("x")),
    ]);
    assert!(matches!(
        record_from_json(RecordKind::Participant, &mistagged),
        Err(RegistryError::SchemaViolation { .. })
    ));
    // …and an absent tag refuses.
    let untagged = Json::obj([("participant_id", Json::str("x"))]);
    assert!(matches!(
        record_from_json(RecordKind::Adapter, &untagged),
        Err(RegistryError::SchemaViolation { .. })
    ));
    // The kinds admit no deeper structural schema at the registry layer.
    assert!(RecordKind::Participant.has_stage1_schema());
    assert!(RecordKind::Adapter.has_stage1_schema());

    // Store admission — a participant body registers and resolves.
    let d = dir("hosted-kinds");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let pv = s
        .register(
            RegistryRecord::Participant(participant_body("hh.reference")),
            &kernel(),
            None,
        )
        .unwrap();
    assert!(!pv.version_id.is_empty());
}

fn hosted_entry(subject: &str) -> ConformanceRecord {
    ConformanceRecord::observed_entry(
        subject,
        "adapter-zero/1",
        "streaming",
        Json::str("supported"),
        Json::str("supported"),
        ObservedIn::Probe,
        Some("ev:1".to_string()),
        7,
    )
}

#[test]
fn hosted_conformance_entries_gate_on_participant_subject() {
    // The §6.6 verdict is derived, never authored: `supported` when declared
    // and observed agree, `drift` when both concrete and they differ, and
    // non-concrete values never coerce into a verdict (T-LCD-07).
    let e = hosted_entry("p1");
    assert_eq!(e.verdict, ConformanceVerdict::Supported);
    let drift = ConformanceRecord::observed_entry(
        "p1",
        "a1",
        "streaming",
        Json::str("supported"),
        Json::str("unsupported"),
        ObservedIn::Run,
        None,
        0,
    );
    assert_eq!(drift.verdict, ConformanceVerdict::Drift);
    let unk = ConformanceRecord::observed_entry(
        "p1",
        "a1",
        "streaming",
        Json::str("supported"),
        Json::str("unknown"),
        ObservedIn::Run,
        None,
        0,
    );
    assert_eq!(unk.verdict, ConformanceVerdict::Unknown);
    // Staleness pins the observed version (§6.6).
    assert!(e.is_stale("other-version"));
    assert!(!e.is_stale("p1"));

    // A report carrying hosted entries on a non-participant subject refuses
    // at the codec (hosted entries pin a participant record only).
    let d = dir("hosted-conf");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = register_class(&mut s, "c1");
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let sv = s
        .register(
            RegistryRecord::Suite(ConformanceSuite {
                suite_id: "sv1".to_string(),
                class_ref: c.clone(),
                contract_version: "1.0".to_string(),
                tests: vec![],
                required_for_status: BTreeSet::new(),
            }),
            &kernel(),
            None,
        )
        .unwrap();
    let report = ConformanceReport {
        report_id: "rep-hosted".to_string(),
        subject_kind: SubjectKind::Variant,
        subject_ref: v.version_id.clone(),
        suite_ref: sv.version_id.clone(),
        host: ReportHost {
            placement: Placement::SubprocessConfined,
            isolation: "x".to_string(),
            instrument: hh_identity::repro::InstrumentRecord::new("1", "abc", false),
        },
        produced_by: ProducedBy::RegistryCi,
        results: vec![],
        probed_declaration: BTreeMap::new(),
        run_id: "run1".to_string(),
        stale: false,
        hosted_entries: vec![hosted_entry("p1")],
    };
    // …refused at admission (the codec gate is structural; the store gate
    // additionally requires the subject pin to resolve to a participant).
    assert!(matches!(
        s.register(RegistryRecord::Report(report), &kernel(), None),
        Err(RegistryError::SchemaViolation { .. })
    ));
}
