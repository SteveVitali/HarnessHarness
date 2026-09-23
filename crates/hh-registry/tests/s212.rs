//! S2.12 registry-enforcement tests — `record_conformance` + derived staleness,
//! transitive `StaleIndex` propagation, `catalog`, `sameness`, `update_available`,
//! the widening ⇒ attestation + MAJOR-bump gate, and `diff_ref` minting/verify.
//! Every behavior has a fail-if-removed test (spec §6.2; AC-R-2.12.1-9,
//! AC-R-2.12.2-{12,13,19}).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;

use hh_hir::leaves::Text;
use hh_identity::idp::address;
use hh_identity::names::{NameStatus, ResolveMode};
use hh_identity::sameness::SamenessLevel;
use hh_identity::supersede::SupersedeReason;
use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, HumanRole, Origin, PersistenceScope,
    ProvenanceRecord,
};
use hh_registry::kinds::{
    Cardinality, ConformanceVerdict, Placement, ProducedBy, RequireConformance, SubjectKind,
};
use hh_registry::records::{
    AppliesTo, ClassRecord, ConformanceReport, ConformanceSuite, ContractOperation, Implementation,
    RegistryPolicy, RegistryRecord, ReportHost, ReportResult, SuiteTest, VariantRecord,
};
use hh_registry::store::{
    record_apply, record_diff, QueryPredicate, RegistryStore, ResolveInput, ResolveRequest,
    SlotConstraints,
};
use hh_registry::RegistryError;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-registry-s212-{}-{}", std::process::id(), tag));
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

fn delegate() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::model("model:m", "run:r", "resp:1"),
        PersistenceScope::Run,
        0,
    )
}

fn attested_principal(name: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted_attested(
        Origin::human(name, HumanRole::Principal),
        PersistenceScope::Run,
        0,
        Attestation {
            kind: AttestationKind::HashChain,
            subject_hash: "sha256:subject".into(),
            anchor: AttestationAnchor::Chain("sha256:head".into()),
            verified_by: "kernel:chain".into(),
            verified_at: 1,
        },
    )
}

fn text(s: &str, prov: &ProvenanceRecord) -> Text {
    Text::new(s, "test", prov.clone())
}

fn class(id: &str, contract_version: &str) -> ClassRecord {
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
        contract_version: contract_version.to_string(),
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

fn suite(class_ref: &str, contract_version: &str) -> ConformanceSuite {
    ConformanceSuite {
        suite_id: "s1".to_string(),
        class_ref: class_ref.to_string(),
        contract_version: contract_version.to_string(),
        tests: vec![SuiteTest {
            test_id: "t1".to_string(),
            kind: hh_registry::kinds::TestKind::Static,
            fixture_ref: None,
            driver: hh_registry::kinds::TestDriver::InProcess,
            oracle_class: hh_registry::kinds::OracleClass::Deterministic,
            budget: Json::Null,
            verdict_rule: "all".to_string(),
        }],
        required_for_status: BTreeSet::from(["t1".to_string()]),
    }
}

fn report(subject: &str, suite_ref: &str, by: ProducedBy, run_id: &str) -> ConformanceReport {
    ConformanceReport {
        report_id: format!("rep-{subject}-{}", by.as_str()),
        subject_kind: SubjectKind::Variant,
        subject_ref: subject.to_string(),
        suite_ref: suite_ref.to_string(),
        host: ReportHost {
            placement: Placement::SubprocessConfined,
            isolation: "x".to_string(),
            instrument: hh_identity::repro::InstrumentRecord::new("1", "abc", false),
        },
        produced_by: by,
        results: vec![ReportResult {
            subject: "t1".to_string(),
            verdict: ConformanceVerdict::Supported,
            evidence_ref: None,
        }],
        probed_declaration: BTreeMap::from([(
            "deterministic".to_string(),
            ConformanceVerdict::Supported,
        )]),
        run_id: run_id.to_string(),
        stale: false,
    }
}

fn probed_policy() -> RegistryPolicy {
    let mut p = RegistryPolicy::stage1_default();
    p.require_conformance = RequireConformance::Probed;
    p
}

// ── record_conformance (AC-R-2.12.1-9) ────────────────────────────────────────

#[test]
fn record_conformance_requires_durable_run_for_ci_and_lab() {
    let d = dir("conformance-durable");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let sv = s
        .register(
            RegistryRecord::Suite(suite(&c.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();

    // registry_ci on a run the store does not hold as durable → RunNotDurable.
    let e = s
        .record_conformance(
            report(
                &v.version_id,
                &sv.version_id,
                ProducedBy::RegistryCi,
                "run1",
            ),
            &kernel(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::RunNotDurable { .. }));

    // lab is held to the same rule.
    let e = s
        .record_conformance(
            report(&v.version_id, &sv.version_id, ProducedBy::Lab, "run1"),
            &kernel(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::RunNotDurable { .. }));

    // The boundary marks the run durable; then the report registers.
    s.mark_run_durable("run1", &kernel()).unwrap();
    let rv = s
        .record_conformance(
            report(
                &v.version_id,
                &sv.version_id,
                ProducedBy::RegistryCi,
                "run1",
            ),
            &kernel(),
        )
        .unwrap();
    assert!(s.get(&rv.version_id).is_some());

    // A publisher_claim never needs a durable run — but never counts toward probed.
    s.record_conformance(
        report(
            &v.version_id,
            &sv.version_id,
            ProducedBy::PublisherClaim,
            "any",
        ),
        &kernel(),
    )
    .unwrap();

    // mark_run_durable is authority-gated (kernel/principal only).
    let e = s.mark_run_durable("runX", &delegate()).unwrap_err();
    assert!(matches!(e, RegistryError::AuthorityInsufficient { .. }));
}

#[test]
fn record_conformance_derives_stale_on_class_bump_and_pin_revocation() {
    let d = dir("conformance-stale");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    s.set_policy(probed_policy()).unwrap();
    let c1 = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let sv = s
        .register(
            RegistryRecord::Suite(suite(&c1.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c1.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.mark_run_durable("run1", &kernel()).unwrap();
    let rv = s
        .record_conformance(
            report(
                &v.version_id,
                &sv.version_id,
                ProducedBy::RegistryCi,
                "run1",
            ),
            &kernel(),
        )
        .unwrap();
    // The derived member was recorded non-stale.
    if let Some((_, RegistryRecord::Report(r))) = s.get(&rv.version_id) {
        assert!(!r.stale);
    } else {
        panic!("report missing");
    }
    // The probed floor is satisfied while the report is fresh.
    let resolved = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(
        resolved.conformance_vector.get("deterministic"),
        Some(&ConformanceVerdict::Supported)
    );

    // A class contract_version bump stales the suite's reports — derived, so
    // the floor fails and the vector drops to UNKNOWN without touching the
    // report bytes.
    let mut c2 = class("c1", "2.0");
    c2.conformance_suite_ref = Some(sv.version_id.clone());
    s.register(RegistryRecord::Class(c2), &kernel(), None)
        .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(
        e,
        RegistryError::SuiteStale | RegistryError::ConformanceBelowFloor { .. }
    ));

    // Revoking the suite stales the report through the dependency index —
    // the same derived path (the report bytes never change).
    let d2 = dir("conformance-stale-2");
    let mut s2 = RegistryStore::open(&d2, &kernel()).unwrap();
    s2.set_policy(probed_policy()).unwrap();
    let c = s2
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let sv2 = s2
        .register(
            RegistryRecord::Suite(suite(&c.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s2
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s2.mark_run_durable("run1", &kernel()).unwrap();
    s2.record_conformance(
        report(
            &v2.version_id,
            &sv2.version_id,
            ProducedBy::RegistryCi,
            "run1",
        ),
        &kernel(),
    )
    .unwrap();
    s2.revoke(
        &sv2.version_id,
        SupersedeReason::Revocation,
        &kernel(),
        None,
    )
    .unwrap();
    let e = s2
        .resolve(
            &ResolveInput::Version(v2.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(
        matches!(
            e,
            RegistryError::SuiteStale | RegistryError::ConformanceBelowFloor { .. }
        ),
        "suite revocation must stale the report: {e:?}"
    );
}

// ── transitive stale propagation (S4/CC3) ────────────────────────────────────

#[test]
fn stale_propagates_transitively_through_the_dependency_closure() {
    let d = dir("transitive-stale");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    // The suite depends on the class; a class with a suite pin depends on the
    // suite; the variant depends on the class. variant → class → suite.
    let sv = s
        .register(
            RegistryRecord::Suite(suite(&c.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    let mut c2 = class("c1", "1.0");
    c2.conformance_suite_ref = Some(sv.version_id.clone());
    let c2v = s
        .register(RegistryRecord::Class(c2), &kernel(), None)
        .unwrap();
    let v = s
        .register(
            RegistryRecord::Variant(variant(
                &c2v.version_id,
                "v1",
                Placement::SubprocessConfined,
            )),
            &kernel(),
            None,
        )
        .unwrap();
    // Revoke the suite: the class pinned to it is stale (direct), and the
    // variant is stale *transitively* through the class.
    s.revoke(&sv.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(c2v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(
        matches!(e, RegistryError::Stale { .. }),
        "class stale: {e:?}"
    );
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    match e {
        RegistryError::Stale {
            depends_on_revoked, ..
        } => {
            assert!(depends_on_revoked.contains(&sv.version_id));
        }
        other => panic!("variant must be transitively stale: {other:?}"),
    }
    // Audit mode annotates rather than hides (S4).
    let audit = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert!(audit
        .depends_on_revoked
        .iter()
        .any(|se| se.revoked_member == sv.version_id));
}

#[test]
fn registering_onto_a_revoked_pin_is_stale_immediately() {
    let d = dir("stale-on-register");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    s.revoke(&c.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    // A record registered *after* the revocation that pins the revoked member
    // is stale-by-dependency the moment it lands — never silently clean.
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::Stale { .. }), "{e:?}");
}

// ── catalog (AC-R-2.12.2-12/-19) ──────────────────────────────────────────────

#[test]
fn catalog_lists_tombstoned_names_and_scopes_to_snapshots() {
    let d = dir("catalog");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let v = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "n",
        &v.version_id,
        Some("1.0.0".into()),
        None,
        &principal("alice"),
    )
    .unwrap();
    let snap = s.snapshot(&kernel()).unwrap();
    // Yank the name — the catalog still lists it, tombstoned.
    s.set_name_status("local", "n", NameStatus::Yanked, &principal("alice"))
        .unwrap();
    let cat = s.catalog(None).unwrap();
    let row = cat
        .iter()
        .find(|e| e.envelope.version_id == v.version_id && e.name.is_some())
        .expect("yanked binding must still list");
    assert_eq!(row.name_status, Some(NameStatus::Yanked));
    assert_eq!(row.name.as_ref().unwrap().0, "local");
    // Snapshot-scoped: same membership except the snapshot record itself
    // (minted after the member set was fixed — it is not its own member).
    let scoped = s
        .catalog(Some(&QueryPredicate {
            clauses: vec![],
            snapshot_id: Some(snap.snapshot_id.clone()),
        }))
        .unwrap();
    assert_eq!(cat.len(), scoped.len() + 1);
    assert!(scoped
        .iter()
        .all(|e| e.envelope.kind != hh_registry::kinds::RecordKind::RegistrySnapshot));
    // A record registered *after* the snapshot is out of the scoped catalog.
    s.register(
        RegistryRecord::Variant(variant(&c.version_id, "v2", Placement::SubprocessConfined)),
        &kernel(),
        None,
    )
    .unwrap();
    let scoped2 = s
        .catalog(Some(&QueryPredicate {
            clauses: vec![],
            snapshot_id: Some(snap.snapshot_id.clone()),
        }))
        .unwrap();
    assert_eq!(scoped2.len(), scoped.len());
}

// ── sameness + update_available (AC-R-2.12.2-13) ─────────────────────────────

#[test]
fn sameness_ladder_over_ids_and_lineage() {
    let d = dir("sameness");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    // L0: same version_id.
    assert_eq!(
        s.sameness(&v1.version_id, &v1.version_id).unwrap().level,
        SamenessLevel::L0
    );
    // L1: a rename — same semantic body (same impl pin), different
    // variant_id/summary.
    let mut v1r = variant(&c.version_id, "v1", Placement::SubprocessConfined);
    v1r.variant_id = "v1r".to_string();
    v1r.summary = text("renamed", &kernel());
    let v1r = s
        .register(RegistryRecord::Variant(v1r), &kernel(), None)
        .unwrap();
    assert_eq!(
        s.sameness(&v1.version_id, &v1r.version_id).unwrap().level,
        SamenessLevel::L1
    );
    // L2: same lineage, non-widening successor.
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "n",
        &v1.version_id,
        Some("1.0.0".into()),
        None,
        &principal("alice"),
    )
    .unwrap();
    s.publish(
        "local",
        "n",
        &v2.version_id,
        Some("1.1.0".into()),
        Some(v1.version_id.clone()),
        &principal("alice"),
    )
    .unwrap();
    let sam = s.sameness(&v1.version_id, &v2.version_id).unwrap();
    assert_eq!(sam.level, SamenessLevel::L2, "compatible successor");
    // L4: an unrelated record on no shared lineage.
    let c2 = s
        .register(RegistryRecord::Class(class("c2", "1.0")), &kernel(), None)
        .unwrap();
    assert_eq!(
        s.sameness(&v1.version_id, &c2.version_id).unwrap().level,
        SamenessLevel::L4
    );
}

#[test]
fn widening_publish_needs_attestation_and_a_major_bump_and_mints_diff_ref() {
    let d = dir("widening");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::Remote)),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "n",
        &v1.version_id,
        Some("1.0.0".into()),
        None,
        &principal("alice"),
    )
    .unwrap();
    // Remote → subprocess_confined widens placement — unattested → human gate.
    let e = s
        .publish(
            "local",
            "n",
            &v2.version_id,
            Some("2.0.0".into()),
            Some(v1.version_id.clone()),
            &principal("alice"),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::AuthorityWideningRequiresHuman));
    // Attested but minor bump → LabelBumpRequired.
    let e = s
        .publish(
            "local",
            "n",
            &v2.version_id,
            Some("1.5.0".into()),
            Some(v1.version_id.clone()),
            &attested_principal("alice"),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::LabelBumpRequired { .. }));
    // Attested + MAJOR bump → publishes; the entry carries a `diff_ref` whose
    // ops apply byte-for-byte over the superseded body.
    let e2 = s
        .publish(
            "local",
            "n",
            &v2.version_id,
            Some("2.0.0".into()),
            Some(v1.version_id.clone()),
            &attested_principal("alice"),
        )
        .unwrap();
    let entry = e2.supersedes.as_deref();
    assert_eq!(entry, Some(v1.version_id.as_str()));
    let view = s.lineage(&v2.version_id).unwrap();
    let hist = view
        .name_history
        .iter()
        .find(|e| e.version_id == v2.version_id)
        .expect("name entry");
    let dref = hist
        .diff_ref
        .clone()
        .expect("widening publish mints a diff_ref");
    let ops = s.diff_for(&v2.version_id).expect("diff body stored");
    // Re-derive: apply(base, ops) == target body.
    let base = &s.get(&v1.version_id).unwrap().1;
    let target = &s.get(&v2.version_id).unwrap().1;
    let base_body = hh_registry::schema::body_json(base, false);
    let target_body = hh_registry::schema::body_json(target, false);
    assert_eq!(record_apply(&base_body, ops).unwrap(), target_body);
    // The ref is the content id of the canonical ops.
    assert_eq!(
        dref,
        hh_identity::idp::idp_id("registry.diff", ops.to_canonical_string().as_bytes())
    );
    // `verify` sees a coherent log; `heal_diffs` has nothing to repair.
    assert!(s.verify().ok());
    assert_eq!(s.heal_diffs().unwrap(), 0);
    // Budget loosening is widening too: declared_costs growing a bound.
    let mut v3 = variant(&c.version_id, "v3", Placement::SubprocessConfined);
    v3.declared_costs = Some(Json::obj([("tokens", Json::Int(100))]));
    let v3 = s
        .register(RegistryRecord::Variant(v3), &kernel(), None)
        .unwrap();
    let mut v4 = variant(&c.version_id, "v4", Placement::SubprocessConfined);
    v4.declared_costs = Some(Json::obj([("tokens", Json::Int(200))]));
    let v4 = s
        .register(RegistryRecord::Variant(v4), &kernel(), None)
        .unwrap();
    s.publish(
        "local",
        "b",
        &v3.version_id,
        Some("1.0.0".into()),
        None,
        &principal("alice"),
    )
    .unwrap();
    let e = s
        .publish(
            "local",
            "b",
            &v4.version_id,
            Some("1.1.0".into()),
            Some(v3.version_id.clone()),
            &attested_principal("alice"),
        )
        .unwrap_err();
    assert!(
        matches!(e, RegistryError::LabelBumpRequired { .. }),
        "loosened budget is widening: {e:?}"
    );
    // L3: the widening successor is incompatible (never resumable, never pooled).
    let sam = s.sameness(&v3.version_id, &v4.version_id).unwrap();
    assert_eq!(sam.level, SamenessLevel::L3);
    assert!(!hh_identity::sameness::is_resumable(sam));
}

#[test]
fn update_available_notifies_without_resealing() {
    let d = dir("update");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let v1 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    let v2 = s
        .register(
            RegistryRecord::Variant(variant(&c.version_id, "v2", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.publish(
        "local",
        "n",
        &v1.version_id,
        Some("1.0.0".into()),
        None,
        &principal("alice"),
    )
    .unwrap();
    assert!(s.update_available(&v1.version_id).unwrap().is_none());
    s.publish(
        "local",
        "n",
        &v2.version_id,
        Some("1.1.0".into()),
        Some(v1.version_id.clone()),
        &principal("alice"),
    )
    .unwrap();
    let n = s.update_available(&v1.version_id).unwrap().expect("update");
    assert_eq!(n.version_id, v2.version_id);
    assert_eq!(n.label.as_deref(), Some("1.1.0"));
    assert_eq!(n.sameness.level, SamenessLevel::L2);
    // The pinned version still resolves — nothing was re-sealed.
    assert!(s
        .resolve(
            &ResolveInput::Version(v1.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .is_ok());
}

// ── environment_record arm (R-2.12.2 registry half) ──────────────────────────

/// The `hh_identity::record::Record::canonical_full()` form `hh-env` mints for
/// an `EnvironmentRecord` — the registry stores it verbatim.
fn env_body(containment_vid: &str, tag: &str) -> Json {
    Json::obj([
        ("kind", Json::str("environment")),
        (
            "semantic",
            Json::obj([
                ("class", Json::str("local_sandboxed")),
                (
                    "image",
                    Json::obj([(
                        "content_address",
                        Json::str(
                            address(format!("img-{tag}").as_bytes(), "application/octet-stream")
                                .id(),
                        ),
                    )]),
                ),
                ("platform", Json::str("linux/amd64")),
                ("provisioning", Json::obj([("hooks", Json::obj([]))])),
                (
                    "containment_policy",
                    Json::obj([
                        ("semantic_id", Json::str("sha256:sem")),
                        ("version_id", Json::str(containment_vid)),
                    ]),
                ),
                ("nondeterminism", Json::Arr(vec![])),
                ("unpinned", Json::Arr(vec![])),
                ("limits", Json::obj([])),
            ]),
        ),
        ("surface", Json::obj([])),
        ("refs", Json::Arr(vec![])),
    ])
}

#[test]
fn environment_record_registers_under_the_env_identity_tag() {
    let d = dir("env-record");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    // The pinned containment policy is any registered version — use a class.
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let body = env_body(&c.version_id, "e1");
    let expected_vid =
        hh_identity::idp::idp_id("environment", body.to_canonical_string().as_bytes());
    let v = s
        .register(
            RegistryRecord::EnvironmentRecord(body.clone()),
            &kernel(),
            None,
        )
        .unwrap();
    assert_eq!(v.version_id, expected_vid);
    // The semantic coordinate mints under `environment#semantic`.
    let sem = Json::obj([
        ("kind", Json::str("environment")),
        ("semantic", body.get("semantic").unwrap().clone()),
    ]);
    assert_eq!(
        v.semantic_id.as_deref(),
        Some(
            hh_identity::idp::idp_id("environment#semantic", sem.to_canonical_string().as_bytes())
                .as_str()
        )
    );
    // The record lists in `catalog` under its kind.
    let cat = s
        .catalog(Some(&QueryPredicate {
            clauses: vec![hh_registry::store::QueryClause {
                field: "kind".to_string(),
                op: hh_registry::store::QueryOp::Eq,
                value: "environment_record".to_string(),
            }],
            snapshot_id: None,
        }))
        .unwrap();
    assert_eq!(cat.len(), 1);
    assert_eq!(cat[0].envelope.version_id, expected_vid);
    // Revoking the pinned containment policy stales the env record.
    s.revoke(&c.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(expected_vid.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::Stale { .. }), "{e:?}");
    // A body missing the canonical_full members refuses at decode/register.
    let bad = Json::obj([("kind", Json::str("environment"))]);
    assert!(s
        .register(RegistryRecord::EnvironmentRecord(bad), &kernel(), None)
        .is_err());
}

// ── diff primitives round-trip ────────────────────────────────────────────────

#[test]
fn record_diff_apply_round_trips_byte_for_byte() {
    let a = Json::obj([
        ("x", Json::Int(1)),
        ("y", Json::str("keep")),
        ("z", Json::obj([("inner", Json::Bool(true))])),
    ]);
    let b = Json::obj([
        ("x", Json::Int(2)),
        ("y", Json::str("keep")),
        ("w", Json::Arr(vec![Json::Int(1)])),
    ]);
    let ops = record_diff(&a, &b);
    assert_eq!(record_apply(&a, &ops).unwrap(), b);
    assert_eq!(record_apply(&a, &record_diff(&a, &a)).unwrap(), a);
    // Non-object pair → whole-body replace applies.
    let ops = record_diff(&Json::Int(1), &b);
    assert_eq!(record_apply(&Json::Int(1), &ops).unwrap(), b);
}

// ── slot_choices sees derived staleness (AC-R-2.12.1-9 tail) ─────────────────

#[test]
fn slot_choices_drops_a_variant_whose_report_went_stale() {
    let d = dir("slot-stale");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    s.set_policy(probed_policy()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let sv = s
        .register(
            RegistryRecord::Suite(suite(&c.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    // A class version carrying `conformance_suite_ref` (slot_choices requires
    // the class to name its suite under a non-none floor — SuiteMissing).
    let mut cs = class("c1", "1.0");
    cs.conformance_suite_ref = Some(sv.version_id.clone());
    let cv = s
        .register(RegistryRecord::Class(cs), &kernel(), None)
        .unwrap();
    let v = s
        .register(
            RegistryRecord::Variant(variant(&cv.version_id, "v1", Placement::SubprocessConfined)),
            &kernel(),
            None,
        )
        .unwrap();
    s.mark_run_durable("run1", &kernel()).unwrap();
    s.record_conformance(
        report(
            &v.version_id,
            &sv.version_id,
            ProducedBy::RegistryCi,
            "run1",
        ),
        &kernel(),
    )
    .unwrap();
    let choices = s.slot_choices("c1", &SlotConstraints::default()).unwrap();
    assert_eq!(choices.len(), 1);
    // Revoke the suite → report stale-by-dependency → the floor fails → the
    // variant drops out of slot_choices (never silently offered — CC3).
    s.revoke(&sv.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    let choices = s.slot_choices("c1", &SlotConstraints::default()).unwrap();
    assert!(choices.is_empty());
}
