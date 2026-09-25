//! S4.1 — the C1 multi-namespace registry service (spec §6.2; R-2.10.2;
//! ADR-0153 D3/D4). Deterministic acceptance rows AC-R-2.10.2-{9,10,11} plus
//! the signer/authorization and conformance-mode halves:
//!
//! - AC-9: third-party unsigned out-of-process variants resolve with
//!   `external`-bounded authority and admissible placement; `in_process` is
//!   refused; signature-required variants quarantine until `pin` endorsement;
//!   declared capabilities do not contribute to `probed` until an admissible
//!   `registry_ci` report exists.
//! - AC-10: revocation prevents execute-resolution of the revoked version and
//!   the sealed definitions in its stale index; referencing rows stay visible
//!   with `depends_on_revoked`.
//! - AC-11: ACP/MCP imports produce quarantined `foreign_import` records with
//!   structured loss reports; ACP manifests lift to `participant_candidate`
//!   (never a variant); a first-party `variant` export → `plugin_manifest/1`
//!   → reimport round-trips every non-lost field (byte-equal body ⇒ the same
//!   `version_id`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_hir::leaves::Text;
use hh_identity::idp::address;
use hh_identity::names::ResolveMode;
use hh_identity::supersede::SupersedeReason;
use hh_provenance::origin::HumanRole;
use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, AuthorityClass, Origin, PersistenceScope,
    ProvenanceRecord,
};
use hh_registry::foreign::{self, ForeignRef, ForeignSystem};
use hh_registry::kinds::{
    Admission, ConformanceVerdict, OwnerRef, Placement, ProducedBy, PublishRule,
    RequireConformance, SubjectKind,
};
use hh_registry::records::{
    AppliesTo, ClassRecord, ConformanceReport, ConformanceSuite, ContractOperation, Implementation,
    NamespaceRecord, RegistryPolicy, RegistryRecord, ReportHost, ReportResult, SuiteTest,
    TrustRootPolicy, VariantRecord,
};
use hh_registry::store::{RegistryStore, ResolveInput, ResolveRequest};
use hh_registry::RegistryError;
use hh_wire::json::Json;

// ── fixtures ─────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-registry-s41-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("test", 0)
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

/// A third-party publisher — a `tool` origin mints `external` authority (the
/// out-of-process publisher class §6.2 describes).
fn third_party(cap: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(Origin::tool(cap, "inv:1"), PersistenceScope::Run, 0)
}

const SIGNER: &str = "did:example:alice";
const SIGNER_B: &str = "did:example:mallory";

fn signature_attestation(signer: &str) -> Attestation {
    Attestation {
        kind: AttestationKind::Signature,
        subject_hash: "sha256:signed-body".into(),
        anchor: AttestationAnchor::Signer(signer.to_string()),
        verified_by: "kernel:sig".into(),
        verified_at: 1,
    }
}

/// A third-party registrar carrying a `Signature` attestation anchored on
/// `signer` — `external` authority + a provable signer coordinate.
fn signed_third_party(cap: &str, signer: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted_attested(
        Origin::tool(cap, "inv:1"),
        PersistenceScope::Run,
        0,
        signature_attestation(signer),
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
        cardinality: hh_registry::kinds::Cardinality::ExactlyOne,
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
    variant_by(class_ref, tag, placement, &kernel())
}

fn variant_by(
    class_ref: &str,
    tag: &str,
    placement: Placement,
    summary_prov: &ProvenanceRecord,
) -> VariantRecord {
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
        summary: text(&format!("variant {tag}"), summary_prov),
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
        hosted_entries: Vec::new(),
    }
}

fn trust_root(signers: &[&str], require_for: &[&str]) -> TrustRootPolicy {
    TrustRootPolicy {
        allowed_sources: BTreeSet::from(["signature".to_string(), "pin".to_string()]),
        accepted_signers: signers.iter().map(|s| s.to_string()).collect(),
        required_predicates: BTreeSet::new(),
        require_signature_for: require_for.iter().map(|s| s.to_string()).collect(),
        hash_only_ceiling: AuthorityClass::External,
        max_age: None,
        drift_policy: "quarantine".to_string(),
        scanner_policy: "advisory".to_string(),
        model_install: hh_registry::records::ModelInstallRule::Deny,
        archive_policy: "candidate_only".to_string(),
        ext: BTreeMap::new(),
    }
}

fn shared_ns(ns: &str, owners: Vec<OwnerRef>, require_signature: bool) -> NamespaceRecord {
    NamespaceRecord {
        namespace: ns.to_string(),
        owners,
        who_may_publish: PublishRule::OwnersOrPrincipal,
        who_may_deprecate: PublishRule::OwnersOrPrincipal,
        who_may_yank: PublishRule::OwnersOrPrincipal,
        who_may_revoke: PublishRule::OwnersOrPrincipal,
        require_signature,
    }
}

fn policy_with_foreign(systems: &[&str]) -> RegistryPolicy {
    let mut p = RegistryPolicy::stage1_default();
    p.allowed_foreign_systems = systems.iter().map(|s| s.to_string()).collect();
    p
}

fn resolve_execute(
    store: &RegistryStore,
    vid: &str,
) -> Result<hh_registry::store::ResolvedRecord, RegistryError> {
    store.resolve(
        &ResolveInput::Version(vid.to_string()),
        ResolveMode::Execute,
        &ResolveRequest::default(),
    )
}

// ── AC-R-2.10.2-9 ────────────────────────────────────────────────────────────

/// AC-9 (a): a third-party *unsigned* out-of-process variant resolves at
/// `external` authority with an admissible placement; `in_process` is refused.
#[test]
fn ac9_third_party_out_of_process_resolves_in_process_refused() {
    let d = dir("ac9-resolve");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let tp = third_party("registry:third-party");

    // subprocess_confined / container / remote are admissible third-party
    // placements.
    for (tag, placement) in [
        ("v-sub", Placement::SubprocessConfined),
        ("v-ctr", Placement::Container),
        ("v-rmt", Placement::Remote),
    ] {
        let v = s
            .register(
                RegistryRecord::Variant(variant_by(&c.version_id, tag, placement, &tp)),
                &tp,
                Some("idp:trust/tp".to_string()),
            )
            .unwrap();
        let r = resolve_execute(&s, &v.version_id).unwrap();
        assert_eq!(r.admission, Admission::Resolved);
        assert_eq!(r.envelope.registrar.authority, AuthorityClass::External);
    }

    // `in_process` third-party code is never admissible (CF-141).
    let e = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v-bad",
                Placement::InProcess,
                &tp,
            )),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::LocalityInadmissible { .. }));

    // `component_model` is reserved — inadmissible here either way.
    let e = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v-cm",
                Placement::ComponentModel,
                &tp,
            )),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::LocalityInadmissible { .. }));

    // A third-party summary above the hash-only ceiling is refused unless its
    // leaf carries a verified signature attestation.
    let mut hi = variant_by(
        &c.version_id,
        "v-hi",
        Placement::SubprocessConfined,
        &principal("p1"),
    );
    hi.summary = text("claimed-authority summary", &principal("p1"));
    let e = s
        .register(
            RegistryRecord::Variant(hi),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap_err();
    assert!(matches!(e, RegistryError::SchemaViolation { .. }));
}

/// AC-9 (b): signature-required variants register `quarantined` until a `pin`
/// endorsement lifts them; a `pin` under an unaccepted signer refuses; a
/// delegate cannot endorse.
#[test]
fn ac9_signature_required_quarantines_until_pin() {
    let d = dir("ac9-pin");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    // The trust root names the accepted signer and the signature-required
    // spelling. Registered at `principal`+ → `resolved` → live.
    s.register(
        RegistryRecord::TrustRootPolicy(trust_root(&[SIGNER], &["component_variant"])),
        &kernel(),
        None,
    )
    .unwrap();
    assert!(s.accepted_signers().contains(SIGNER));

    // Unsigned (unattested) third-party variant → quarantined, not refused.
    let tp = third_party("registry:tp");
    let v = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v-sig",
                Placement::SubprocessConfined,
                &tp,
            )),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    assert_eq!(
        s.get(&v.version_id).unwrap().0.admission,
        Admission::Quarantined
    );
    // Quarantined records never execute-resolve.
    assert!(resolve_execute(&s, &v.version_id).is_err());
    // …but remain readable (audit visibility, never hidden).
    assert_eq!(
        s.lookup(&v.version_id).unwrap().admission,
        Admission::Quarantined
    );

    // A delegate cannot endorse a pin (a trust act ≥ principal).
    assert!(matches!(
        s.pin(&v.version_id, &delegate()).unwrap_err(),
        RegistryError::AuthorityInsufficient { .. }
    ));
    // A principal endorses, but the record's registrar carries no attestation
    // → AttestationFailed (nothing to verify against the trust root).
    assert!(matches!(
        s.pin(&v.version_id, &principal("p1")).unwrap_err(),
        RegistryError::AttestationFailed { .. }
    ));

    // Signed registration under an *unaccepted* signer quarantines too, and
    // `pin` refuses — the anchor is not in the trust set.
    let tp_bad = signed_third_party("registry:tp", SIGNER_B);
    let v_bad = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v-sigbad",
                Placement::SubprocessConfined,
                &tp_bad,
            )),
            &tp_bad,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    assert_eq!(
        s.get(&v_bad.version_id).unwrap().0.admission,
        Admission::Quarantined
    );
    assert!(matches!(
        s.pin(&v_bad.version_id, &principal("p1")).unwrap_err(),
        RegistryError::AttestationFailed { .. }
    ));

    // Signed registration under the accepted signer: quarantined at register
    // (the signature did not decide admission), lifted by `pin`.
    let tp_ok = signed_third_party("registry:tp", SIGNER);
    let v_ok = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v-sigok",
                Placement::SubprocessConfined,
                &tp_ok,
            )),
            &tp_ok,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    assert_eq!(
        s.get(&v_ok.version_id).unwrap().0.admission,
        Admission::Quarantined
    );
    s.pin(&v_ok.version_id, &principal("p1")).unwrap();
    assert_eq!(
        s.get(&v_ok.version_id).unwrap().0.admission,
        Admission::Resolved
    );
    let r = resolve_execute(&s, &v_ok.version_id).unwrap();
    assert_eq!(r.admission, Admission::Resolved);
}

/// AC-9 (c): declared capabilities never count toward `probed`; a
/// `publisher_claim` report is review evidence only — only an admissible
/// `registry_ci` report over a durable run satisfies the floor.
#[test]
fn ac9_publisher_claim_never_probed() {
    let d = dir("ac9-probed");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let mut pol = RegistryPolicy::stage1_default();
    pol.require_conformance = RequireConformance::Probed;
    s.set_policy(pol).unwrap();

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

    // Declarations alone are not probed evidence — the floor refuses the
    // projection and the refusal carries the vector (`unknown`, never
    // coerced).
    let e = s.lookup(&v.version_id).unwrap_err();
    let RegistryError::ConformanceBelowFloor { vector } = &e else {
        panic!("expected ConformanceBelowFloor, got {e:?}")
    };
    assert_eq!(
        vector.get("deterministic"),
        Some(&ConformanceVerdict::Unknown)
    );
    assert!(matches!(
        s.resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest {
                conformance_floor: Some(RequireConformance::Probed),
                ..ResolveRequest::default()
            },
        )
        .unwrap_err(),
        RegistryError::ConformanceBelowFloor { .. }
    ));

    // A publisher_claim report is review evidence — it never fills the vector
    // and never satisfies `probed`.
    s.record_conformance(
        report(
            &v.version_id,
            &sv.version_id,
            ProducedBy::PublisherClaim,
            "pub-run",
        ),
        &kernel(),
    )
    .unwrap();
    let claims = s.publisher_claims(&v.version_id);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].produced_by, ProducedBy::PublisherClaim);
    // Still below floor — the claim is review evidence, never `probed`; the
    // refusal still carries `unknown` for the declared field.
    let e = s.lookup(&v.version_id).unwrap_err();
    let RegistryError::ConformanceBelowFloor { vector } = &e else {
        panic!("expected ConformanceBelowFloor, got {e:?}")
    };
    assert_eq!(
        vector.get("deterministic"),
        Some(&ConformanceVerdict::Unknown)
    );

    // registry_ci evidence over a durable run lifts the vector.
    s.mark_run_durable("ci-run", &kernel()).unwrap();
    s.record_conformance(
        report(
            &v.version_id,
            &sv.version_id,
            ProducedBy::RegistryCi,
            "ci-run",
        ),
        &kernel(),
    )
    .unwrap();
    let r = s.lookup(&v.version_id).unwrap();
    assert_eq!(
        r.conformance_vector.get("deterministic"),
        Some(&ConformanceVerdict::Supported)
    );
}

// ── AC-R-2.10.2-10 ───────────────────────────────────────────────────────────

/// AC-10: revocation prevents execute-resolution of the revoked version and
/// the dependants in its stale index; the rows stay visible (audit/reproduce
/// return them pinned with status).
#[test]
fn ac10_revocation_blocks_execute_keeps_visibility() {
    let d = dir("ac10");
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
        "hh",
        "widgets/v1",
        &v.version_id,
        Some("1.0.0".to_string()),
        None,
        &kernel(),
    )
    .unwrap();

    // A report pinning the revoked subject is a dependant (its `subject_ref`/
    // `suite_ref` are declared dependencies at register).
    let sv = s
        .register(
            RegistryRecord::Suite(suite(&c.version_id, "1.0")),
            &kernel(),
            None,
        )
        .unwrap();
    s.mark_run_durable("ci-run", &kernel()).unwrap();
    let rep = s
        .register(
            RegistryRecord::Report(report(
                &v.version_id,
                &sv.version_id,
                ProducedBy::RegistryCi,
                "ci-run",
            )),
            &kernel(),
            None,
        )
        .unwrap();

    s.revoke(&v.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();

    // Execute-resolution of the revoked version refuses, dated, with reason.
    assert!(matches!(
        resolve_execute(&s, &v.version_id).unwrap_err(),
        RegistryError::Revoked { .. }
    ));
    // The stale dependant refuses execute-resolution too.
    assert!(matches!(
        resolve_execute(&s, &rep.version_id).unwrap_err(),
        RegistryError::Stale { .. }
    ));
    // Audit-mode resolution keeps the revoked row visible with status.
    let r = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert_eq!(r.admission, Admission::Revoked);
    // The dependant row is visible with `depends_on_revoked`.
    let r = s
        .resolve(
            &ResolveInput::Version(rep.version_id.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
        .unwrap();
    assert!(r
        .depends_on_revoked
        .iter()
        .any(|e| e.revoked_member == v.version_id));
    // The name binding stays visible in the catalog with the revoked flag.
    let catalog = s.catalog(None).unwrap();
    let row = catalog
        .iter()
        .find(|e| e.envelope.version_id == v.version_id)
        .expect("revoked rows are never deleted");
    assert!(row.revoked);
}

/// AC-10 (authz half): only the publisher / namespace owner / ≥principal may
/// revoke a third-party version.
#[test]
fn ac10_revoke_authorization() {
    let d = dir("ac10-authz");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let tp = third_party("registry:tp");
    let v = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v1",
                Placement::SubprocessConfined,
                &tp,
            )),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    // A different third-party publisher may not revoke it.
    let other = third_party("registry:other");
    assert!(matches!(
        s.revoke(&v.version_id, SupersedeReason::Revocation, &other, None)
            .unwrap_err(),
        RegistryError::AuthorityInsufficient { .. }
    ));
    // Its own registrar may.
    s.revoke(&v.version_id, SupersedeReason::Revocation, &tp, None)
        .unwrap();
}

// ── AC-R-2.10.2-11 ───────────────────────────────────────────────────────────

/// AC-11 (a): ACP + MCP imports land as quarantined `foreign_import` records
/// with structured loss reports; an ACP `agent.json` lifts to a
/// `participant_candidate` (never a variant); the import refuses systems the
/// policy does not admit.
#[test]
fn ac11_foreign_imports_quarantined_with_loss_reports() {
    let d = dir("ac11-import");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    let tp = third_party("registry:importer");

    // Fail-closed: no admitted foreign systems → refused.
    let fref = ForeignRef {
        system: ForeignSystem::AcpRegistry,
        locator: "https://agents.example/a1".to_string(),
        digest: Some("sha256:abc".to_string()),
        label: Some("1.2.3".to_string()),
    };
    let doc = Json::obj([
        ("name", Json::str("agent-a")),
        ("description", Json::str("an ACP agent")),
        ("capabilities", Json::obj([("streaming", Json::Bool(true))])),
        ("signature", Json::str("foreign-sig-blob")),
    ]);
    assert!(matches!(
        foreign::import_foreign(&mut s, &fref, &doc, &tp, Some("idp:trust/tp".to_string()))
            .unwrap_err(),
        RegistryError::ForeignSystemRefused { .. }
    ));

    s.set_policy(policy_with_foreign(&[
        "acp_registry",
        "mcp_registry",
        "marketplace",
        "git",
        "archive",
    ]))
    .unwrap();

    // ACP `agent.json` → participant candidate (never a variant), quarantined,
    // with the unverifiable trust leg + claim-only capabilities named.
    let out = foreign::import_foreign(&mut s, &fref, &doc, &tp, Some("idp:trust/tp".to_string()))
        .unwrap();
    assert_eq!(out.admission, Admission::Quarantined);
    assert!(!out.loss_report.capabilities.is_empty());
    assert!(!out.loss_report.trust_legs.is_empty());
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::ForeignImport(fi) = rec else {
        panic!("expected a foreign_import record")
    };
    assert_eq!(fi.system, "acp_registry");
    assert_eq!(
        fi.lifted_record.get("lifted_kind").and_then(|v| v.as_str()),
        Some("participant_candidate")
    );
    // The declared claims are preserved verbatim — nothing silently dropped.
    assert!(fi
        .lifted_record
        .get("declared_claims")
        .and_then(|c| c.get("capabilities"))
        .is_some());
    // Quarantined imports never execute-resolve.
    assert!(resolve_execute(&s, &out.version_id).is_err());

    // MCP `server.json` → `mcp_server_candidate`, source claims + ext
    // metadata preserved.
    let mcp_ref = ForeignRef {
        system: ForeignSystem::McpRegistry,
        locator: "https://registry.modelcontextprotocol.io/servers/ex".to_string(),
        digest: None,
        label: None,
    };
    let mcp_doc = Json::obj([
        ("name", Json::str("io.example/server")),
        ("version", Json::str("0.1.0")),
        ("tools", Json::Arr(vec![Json::str("search")])),
        ("_meta", Json::obj([("x", Json::Int(1))])),
    ]);
    let out = foreign::import_foreign(
        &mut s,
        &mcp_ref,
        &mcp_doc,
        &tp,
        Some("idp:trust/tp".to_string()),
    )
    .unwrap();
    let (_, rec) = s.get(&out.version_id).unwrap();
    let RegistryRecord::ForeignImport(fi) = rec else {
        panic!("expected a foreign_import record")
    };
    assert_eq!(fi.system, "mcp_registry");
    assert_eq!(
        fi.lifted_record.get("lifted_kind").and_then(|v| v.as_str()),
        Some("mcp_server_candidate")
    );
    assert!(fi
        .lifted_record
        .get("declared_claims")
        .and_then(|c| c.get("_meta"))
        .is_some());
    // No digest anywhere → the loss report names it.
    assert!(!fi.loss_report.digest.is_empty());
}

/// AC-11 (b): a first-party `variant` exports to a canonical
/// `plugin_manifest/1` document that re-decodes cleanly and re-lifts to a
/// byte-equal `VariantRecord` — every non-lost field survives the wire
/// (byte-identical body ⇒ the same `version_id`).
#[test]
fn ac11_export_round_trip_preserves_every_non_lost_field() {
    let d = dir("ac11-export");
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

    // Unknown targets and non-variant kinds refuse — never a silent
    // projection.
    assert!(matches!(
        foreign::export(&s, std::slice::from_ref(&v.version_id), "oci_image/1").unwrap_err(),
        RegistryError::UnknownExportTarget { .. }
    ));
    assert!(matches!(
        foreign::export(
            &s,
            std::slice::from_ref(&c.version_id),
            foreign::EXPORT_TARGET_PLUGIN_MANIFEST
        )
        .unwrap_err(),
        RegistryError::UnsupportedExportKind { .. }
    ));

    let out = foreign::export(&s, std::slice::from_ref(&v.version_id), "plugin_manifest/1").unwrap();
    assert_eq!(out.documents.len(), 1);
    let doc = &out.documents[0];
    // The kernel-authority summary could not occupy the manifest's
    // ≤`external` slot — the verbatim Text rides `claims.exported_summary`
    // and the loss report names the downgrade.
    assert!(!doc.loss_report.trust_legs.is_empty());
    let claims = doc.document.get("claims").expect("claims member");
    assert!(claims.get("exported_summary").is_some());
    assert_eq!(
        claims.get("export_of").and_then(|v| v.as_str()),
        Some(v.version_id.as_str())
    );

    // Re-import: decode the manifest and re-lift the variant body — the same
    // canonical body mints the same version_id.
    let lifted = foreign::lift_variant_from_manifest(&doc.document).unwrap();
    let RegistryRecord::Variant(orig) = s.get(&v.version_id).unwrap().1.clone() else {
        panic!()
    };
    // Canonical bodies are byte-equal (Text is hash-addressed — `content` is
    // elided on the wire and `content_hash` pins it).
    assert_eq!(
        hh_registry::schema::body_json(&RegistryRecord::Variant(lifted.clone()), false),
        hh_registry::schema::body_json(&RegistryRecord::Variant(orig), false),
    );
    let re = s
        .register(RegistryRecord::Variant(lifted), &kernel(), None)
        .unwrap();
    assert_eq!(re.version_id, v.version_id, "nothing lost on the wire");

    // A revoked record refuses export (exporting a revoked variant would lie
    // about availability).
    s.revoke(&v.version_id, SupersedeReason::Revocation, &kernel(), None)
        .unwrap();
    assert!(matches!(
        foreign::export(&s, std::slice::from_ref(&v.version_id), "plugin_manifest/1").unwrap_err(),
        RegistryError::Revoked { .. }
    ));
}

// ── C1 namespace / ownership authz ───────────────────────────────────────────

/// Signer ownership: a shared namespace owned by `OwnerRef::Signer` admits
/// publishes only under a *verified* attestation anchored on that signer —
/// the signer coordinate is proven by provenance, never claimed (CC2).
#[test]
fn signer_ownership_gates_shared_namespace() {
    let d = dir("signer-ns");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    // A live trust root accepting SIGNER.
    s.register(
        RegistryRecord::TrustRootPolicy(trust_root(&[SIGNER], &[])),
        &kernel(),
        None,
    )
    .unwrap();
    // The shared namespace declared by a principal, owned by the signer.
    s.register(
        RegistryRecord::Namespace(shared_ns(
            "org.example",
            vec![OwnerRef::Signer(SIGNER.to_string())],
            false,
        )),
        &principal("admin"),
        Some("idp:trust/admin".to_string()),
    )
    .unwrap();

    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let tp_signed = signed_third_party("registry:tp", SIGNER);
    let v = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v1",
                Placement::SubprocessConfined,
                &tp_signed,
            )),
            &tp_signed,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();

    // An unsigned third party is not the signer owner — refused.
    let unsigned = third_party("registry:tp");
    assert!(matches!(
        s.publish("org.example", "w1", &v.version_id, None, None, &unsigned)
            .unwrap_err(),
        RegistryError::NamespaceForbidden { .. }
    ));
    // The verified signer publishes.
    s.publish(
        "org.example",
        "w1",
        &v.version_id,
        Some("1.0.0".to_string()),
        None,
        &tp_signed,
    )
    .unwrap();
    // A different signer (unaccepted / not the owner) cannot deprecate.
    let mallory = signed_third_party("registry:tp", SIGNER_B);
    assert!(matches!(
        s.set_name_status(
            "org.example",
            "w1",
            hh_identity::names::NameStatus::Deprecated,
            &mallory,
        )
        .unwrap_err(),
        RegistryError::AuthorityInsufficient { .. }
    ));
    // The owner may.
    s.set_name_status(
        "org.example",
        "w1",
        hh_identity::names::NameStatus::Deprecated,
        &tp_signed,
    )
    .unwrap();
}

/// `require_signature` namespaces refuse unsigned publishes (the unsigned
/// record is not rejected at register — the *publish* gate carries the
/// signature rule).
#[test]
fn require_signature_namespace_gates_publish() {
    let d = dir("sig-ns");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    s.register(
        RegistryRecord::TrustRootPolicy(trust_root(&[SIGNER], &[])),
        &kernel(),
        None,
    )
    .unwrap();
    s.register(
        RegistryRecord::Namespace(shared_ns(
            "com.vendor",
            vec![OwnerRef::Principal("alice".to_string())],
            true,
        )),
        &principal("admin"),
        Some("idp:trust/admin".to_string()),
    )
    .unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let tp_signed = signed_third_party("registry:tp", SIGNER);
    let v = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v1",
                Placement::SubprocessConfined,
                &tp_signed,
            )),
            &tp_signed,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    // An otherwise-admitted publisher (a principal) without a verified
    // signature → SignatureRequired (refused, never relaxed).
    assert!(matches!(
        s.publish(
            "com.vendor",
            "w1",
            &v.version_id,
            None,
            None,
            &principal("alice")
        )
        .unwrap_err(),
        RegistryError::SignatureRequired { .. }
    ));
    // A verified signer attestation on the publisher carries the publish.
    let alice_signed = ProvenanceRecord::minted_attested(
        Origin::human("alice", HumanRole::Principal),
        PersistenceScope::Run,
        0,
        signature_attestation(SIGNER),
    );
    s.publish("com.vendor", "w1", &v.version_id, None, None, &alice_signed)
        .unwrap();
}

/// `exp/<id>` namespaces admit ≥principal registrars; shared namespaces owned
/// by a principal coordinate admit that principal and refuse other origins.
#[test]
fn experiment_and_principal_owned_namespaces() {
    let d = dir("exp-ns");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    // A delegate cannot declare an experiment namespace.
    assert!(matches!(
        s.register(
            RegistryRecord::Namespace(shared_ns(
                "exp/e1",
                vec![OwnerRef::Principal("m".to_string())],
                false
            )),
            &delegate(),
            Some("idp:trust/del".to_string()),
        )
        .unwrap_err(),
        RegistryError::AuthorityInsufficient { .. }
    ));
    // A principal declares and publishes into it.
    s.register(
        RegistryRecord::Namespace(shared_ns(
            "exp/e1",
            vec![OwnerRef::Principal("alice".to_string())],
            false,
        )),
        &principal("alice"),
        Some("idp:trust/alice".to_string()),
    )
    .unwrap();
    let c = s
        .register(RegistryRecord::Class(class("c1", "1.0")), &kernel(), None)
        .unwrap();
    let tp = third_party("registry:tp");
    let v = s
        .register(
            RegistryRecord::Variant(variant_by(
                &c.version_id,
                "v1",
                Placement::SubprocessConfined,
                &tp,
            )),
            &tp,
            Some("idp:trust/tp".to_string()),
        )
        .unwrap();
    s.publish(
        "exp/e1",
        "w1",
        &v.version_id,
        None,
        None,
        &principal("alice"),
    )
    .unwrap();
    // A non-owner third party cannot publish or yank in the namespace.
    assert!(matches!(
        s.publish("exp/e1", "w2", &v.version_id, None, None, &tp)
            .unwrap_err(),
        RegistryError::NamespaceForbidden { .. }
    ));
    assert!(matches!(
        s.set_name_status("exp/e1", "w1", hh_identity::names::NameStatus::Yanked, &tp,)
            .unwrap_err(),
        RegistryError::AuthorityInsufficient { .. }
    ));
    // The owner can.
    s.set_name_status(
        "exp/e1",
        "w1",
        hh_identity::names::NameStatus::Yanked,
        &principal("alice"),
    )
    .unwrap();
}

/// Lifecycle audit rows: the `imported` + `admission_refused` classes land
/// through `drain_events` (the ledger append path consumes them — event
/// payloads stay inside the declared audit partitions).
#[test]
fn import_and_pin_emit_lifecycle_events() {
    let d = dir("events");
    let mut s = RegistryStore::open(&d, &kernel()).unwrap();
    s.set_policy(policy_with_foreign(&["acp_registry"]))
        .unwrap();
    let tp = third_party("registry:importer");
    let fref = ForeignRef {
        system: ForeignSystem::AcpRegistry,
        locator: "https://agents.example/a1".to_string(),
        digest: Some("sha256:abc".to_string()),
        label: None,
    };
    foreign::import_foreign(
        &mut s,
        &fref,
        &Json::obj([("name", Json::str("a"))]),
        &tp,
        Some("idp:trust/tp".to_string()),
    )
    .unwrap();
    let events = s.drain_events();
    assert!(events
        .iter()
        .any(|e| e.class == "lifecycle.registry.imported"));
}
