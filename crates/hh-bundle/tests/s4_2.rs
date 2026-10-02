//! `hh-bundle` S4.2 integration tests — the scoped kinds
//! (`arm`/`experiment`/`lineage`), the S9 `cross_section` checks
//! (`ContainmentMismatch`, `UnmatchedBudget`, `PreRegistrationLate`,
//! `RowOrphan`, `DeclarationMissing`), `diff`, the lifecycle surface
//! (`supersede`, `migrate`, `lineage`), attestation + audit, the export
//! targets, and `fetch` refusal/materialization
//! (§5h.3 §2/§3/§6; AC-R-2.9.3-{7,8,10,11,12}, AC-R-2.10.3-11).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::attestation::{attest, sign_attestation, BundleAttestation};
use hh_bundle::audit::audit_bundle;
use hh_bundle::codec::{decode_container, decode_dir, encode_container, encode_dir};
use hh_bundle::diff::bundle_diff;
use hh_bundle::error::BundleError;
use hh_bundle::export::{export_target, PublicationPolicy};
use hh_bundle::fetch::{fetch, member_bytes_or_refused};
use hh_bundle::lifecycle::{lineage_edges, migrate_bundle, supersede_bundle};
use hh_bundle::manifest::{BundleManifest, BundlePolicy, MemberStatus};
use hh_bundle::scoped::{
    assemble_scoped, ChildBundle, ScopedInputs, KIND_ARM, KIND_EXPERIMENT, KIND_LINEAGE,
};
use hh_bundle::validate::{validate_publication, CheckStatus};
use hh_ledger::manifest::{ExperimentBinding, RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-s42-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A store with one experiment-bound subject run (`arm:a` under
/// `run-exp`) plus a second unbound run for the negative cases.
struct Rig {
    store: Store,
    run: String,
    other: String,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

fn rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    // A sealed-definition blob every run manifest pins (assemble
    // requires `harness_def_ref` to resolve).
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.experiment = Some(ExperimentBinding {
        experiment_run_id: Some("run-exp".into()),
        arm_id: Some("a".into()),
        cell_id: Some("cell:0".into()),
        replicate_index: Some(0),
        attempt_no: Some(1),
        comparable: Some(true),
        ..Default::default()
    });
    let (run, _l) = store.open_run(m, "s42.test").unwrap();
    let mut m2 = RunManifest::minimal(RunKind::Agent);
    m2.harness_def_ref = Some(def_addr.clone());
    let (other, _l2) = store.open_run(m2, "s42.test").unwrap();
    Rig {
        store,
        run,
        other,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn run_inputs<'a>(
    rig: &'a Rig,
    run_id: &'a str,
    budget: Json,
    artifact_bytes: &'a dyn Fn(&str) -> Option<Vec<u8>>,
) -> AssembleInputs<'a> {
    AssembleInputs {
        store: &rig.store,
        run_id,
        policy: &rig.policy,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("s42.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget,
        profile: Json::str("none"),
        nondeterminism: vec![],
        participant_class: None,
    }
}

/// `ScopedInputs` for an `arm`/`experiment` bundle over `children`.
#[allow(clippy::too_many_arguments)]
fn scoped<'a>(
    rig: &'a Rig,
    kind: &'a str,
    children: Vec<ChildBundle>,
    subject_runs: Vec<String>,
    arms: Vec<Json>,
) -> ScopedInputs<'a> {
    ScopedInputs {
        store: &rig.store,
        policy: &rig.policy,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("s42.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        kind: kind.to_string(),
        experiment_run_id: Some("run-exp".into()),
        arm: (kind == KIND_ARM).then(|| {
            Json::obj([
                ("arm_id", Json::str("a")),
                ("definition_ref", Json::str("idp:def")),
            ])
        }),
        design: Some(Json::obj([("design", Json::str("d1"))])),
        pre_registration: Some(Json::obj([("registered_at", Json::Int(0))])),
        pre_registration_seq: Some(1),
        first_plan_seq: Some(2),
        match_spec: Some(Json::obj([
            ("dimensions", Json::Arr(vec![Json::str("spend")])),
            ("mode", Json::str("matched_total")),
        ])),
        search_budget: Some(Json::obj([("trials", Json::Int(0))])),
        arms,
        subject_runs,
        children,
        row_refs: vec![],
        row_payloads: vec![],
        participant_class: None,
    }
}

/// A decoded `run` bundle for `run_id`.
fn run_bundle(rig: &Rig, run_id: &str, budget: Json) -> BundleManifest {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    assemble(&run_inputs(rig, run_id, budget, &get))
        .unwrap()
        .manifest
}

fn child_of(m: &BundleManifest, kind: &str) -> ChildBundle {
    ChildBundle {
        bundle_id: m.version_id.clone(),
        kind: kind.to_string(),
        manifest_bytes: m.to_json().to_canonical_string().into_bytes(),
    }
}

/// S9's rows (the `cross_section` stage).
fn s9(report: &hh_bundle::validate::BundleValidationReport) -> &[hh_bundle::validate::CheckRow] {
    &report
        .stages
        .iter()
        .find(|s| s.stage == 9)
        .expect("S9 stage present")
        .checks
}

fn has_code(report: &hh_bundle::validate::BundleValidationReport, code: &str) -> bool {
    s9(report)
        .iter()
        .any(|c| c.status == CheckStatus::Fail && c.code.as_deref() == Some(code))
}

// ── scoped kinds ────────────────────────────────────────────────────────────

#[test]
fn arm_bundle_contains_its_run_bundle_verbatim() {
    let rig = rig("arm");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let assembled = assemble_scoped(&scoped(
        &rig,
        KIND_ARM,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    let m = &assembled.manifest;
    assert_eq!(m.bundle_kind, "arm");
    assert!(!m.version_id.is_empty());
    let contains = m.composition.get("contains").expect("contains[]");
    let Json::Arr(cs) = contains else {
        panic!("contains must be an array")
    };
    assert_eq!(
        cs[0].get("bundle_id").and_then(Json::as_str),
        Some(rb.version_id.as_str())
    );
    assert!(
        m.members
            .iter()
            .any(|mm| mm.role == format!("contains:{}", rb.version_id)),
        "no contains:<bundle_id> member"
    );
}

#[test]
fn experiment_bundle_composes_arm_bundles() {
    let rig = rig("exp");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let arm = assemble_scoped(&scoped(
        &rig,
        KIND_ARM,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    let exp = assemble_scoped(&scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&arm.manifest, "arm")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    assert_eq!(exp.manifest.bundle_kind, "experiment");
    assert_eq!(
        exp.manifest
            .subject
            .experiment
            .get(&rig.run)
            .and_then(|b| b.get("arm_id"))
            .and_then(Json::as_str),
        Some("a")
    );
}

#[test]
fn scoped_assembly_refuses_missing_scope_docs() {
    let rig = rig("refuse");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let mut i = scoped(
        &rig,
        KIND_ARM,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![],
    );
    i.arm = None;
    match assemble_scoped(&i) {
        Err(BundleError::ScopeInvalid { detail }) => assert!(detail.contains("arm")),
        _other => panic!("expected ScopeInvalid"),
    }
    let mut i2 = scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![],
    );
    i2.subject_runs = vec![];
    match assemble_scoped(&i2) {
        Err(BundleError::ScopeInvalid { .. }) => {}
        _other => panic!("expected ScopeInvalid"),
    }
}

#[test]
fn lineage_bundle_needs_no_runs() {
    let rig = rig("lin");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let assembled = assemble_scoped(&scoped(
        &rig,
        KIND_LINEAGE,
        vec![child_of(&rb, "run")],
        vec![],
        vec![],
    ))
    .unwrap();
    assert_eq!(assembled.manifest.bundle_kind, "lineage");
}

// ── S9 cross_section ────────────────────────────────────────────────────────

#[test]
fn s9_passes_for_a_well_formed_experiment_bundle() {
    let rig = rig("s9-ok");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let exp = assemble_scoped(&scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    let report = validate_publication(&exp.manifest, &exp.members);
    for row in s9(&report) {
        assert_eq!(row.status, CheckStatus::Pass, "S9 row failed: {:?}", row);
    }
}

#[test]
fn s9_declaration_missing_and_pre_registration_late_and_row_orphan() {
    let rig = rig("s9-neg");
    let rb = run_bundle(&rig, &rig.run, Json::Null);

    // DeclarationMissing — remove `results.design` post-assembly.
    let mut i = scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    );
    let mut exp = assemble_scoped(&i).unwrap();
    if let Json::Obj(m) = &mut exp.manifest.results {
        m.remove("design");
    }
    let report = validate_publication(&exp.manifest, &exp.members);
    assert!(has_code(&report, "DeclarationMissing"), "{:?}", s9(&report));

    // PreRegistrationLate — the registration lands at/after the first plan.
    i.pre_registration_seq = Some(2);
    i.first_plan_seq = Some(2);
    let exp = assemble_scoped(&i).unwrap();
    let report = validate_publication(&exp.manifest, &exp.members);
    assert!(
        has_code(&report, "PreRegistrationLate"),
        "{:?}",
        s9(&report)
    );

    // RowOrphan — a declared row with no `row:<vid>` member.
    i.pre_registration_seq = Some(1);
    i.row_refs = vec!["row:missing".into()];
    let exp = assemble_scoped(&i).unwrap();
    let report = validate_publication(&exp.manifest, &exp.members);
    assert!(has_code(&report, "RowOrphan"), "{:?}", s9(&report));
}

#[test]
fn s9_containment_mismatch_and_unmatched_budget() {
    let rig = rig("s9-mm");
    // Containment — the child covers a run the parent does not.
    let rb_other = run_bundle(&rig, &rig.other, Json::Null);
    let exp = assemble_scoped(&scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb_other, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    let report = validate_publication(&exp.manifest, &exp.members);
    assert!(
        has_code(&report, "ContainmentMismatch"),
        "{:?}",
        s9(&report)
    );

    // UnmatchedBudget — the arm's declared eval_budget ≠ the run's
    // recorded configuration.budget.
    let budget_a = Json::obj([("eval", Json::Int(100))]);
    let budget_b = Json::obj([("eval", Json::Int(50))]);
    let rb = run_bundle(&rig, &rig.run, budget_a);
    let exp = assemble_scoped(&scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([
            ("arm_id", Json::str("a")),
            ("eval_budget", budget_b),
        ])],
    ))
    .unwrap();
    let report = validate_publication(&exp.manifest, &exp.members);
    assert!(has_code(&report, "UnmatchedBudget"), "{:?}", s9(&report));
}

fn run_assembled(rig: &Rig) -> hh_bundle::assemble::Assembled {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    assemble(&run_inputs(rig, &rig.run, Json::Null, &get)).unwrap()
}

// ── diff ────────────────────────────────────────────────────────────────────

#[test]
fn diff_reports_member_deltas_and_sameness() {
    let rig = rig("diff");
    let a = run_assembled(&rig);
    let mut b = run_assembled(&rig);
    // Same content, new stamp → same fact, different version_id.
    b.manifest.created_at = "2026-01-02T00:00:00.000Z".into();
    b.manifest.version_id = String::new();
    b.manifest.version_id = b.manifest.compute_id();
    let da = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let db = hh_bundle::codec::Decoded {
        manifest: b.manifest.clone(),
        members: b.members.clone(),
    };
    let diff = bundle_diff(&da, &db);
    let j = diff.to_json();
    // Identical content, distinct stamps → the `equivalent` verdict
    // (same subject, same results, same definition version).
    assert_eq!(
        j.get("verdict").and_then(Json::as_str),
        Some("equivalent"),
        "{j:?}"
    );
}

// ── lifecycle ───────────────────────────────────────────────────────────────

#[test]
fn supersede_stamps_derived_from_and_recomputes_version_id() {
    let rig = rig("super");
    let old = run_bundle(&rig, &rig.run, Json::Null);
    let mut new = old.clone();
    new.created_at = "2026-01-02T00:00:00.000Z".into();
    let old_id = old.version_id.clone();
    supersede_bundle(&mut new, &old, "correction").unwrap();
    assert_ne!(new.version_id, old_id);
    assert_eq!(
        new.composition
            .get("derived_from")
            .and_then(|d| d.get("bundle_id"))
            .and_then(Json::as_str),
        Some(old_id.as_str())
    );
    match supersede_bundle(&mut new.clone(), &old, "bogus") {
        Err(BundleError::Malformed { .. }) => {}
        _other => panic!("expected Malformed"),
    }
    // The lineage edges read the chain back.
    let edges = lineage_edges(&[old.clone(), new.clone()]);
    assert_eq!(edges.len(), 1);
    assert_eq!(
        edges[0].get("derived_from").and_then(Json::as_str),
        Some(old_id.as_str())
    );
}

#[test]
fn migrate_is_supersession_with_new_version_id() {
    let rig = rig("mig");
    let a = run_assembled(&rig);
    let decoded = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let migrated = migrate_bundle(
        &decoded,
        "hh-bundle/1",
        ProvenanceRecord::kernel("s42.test", 0).to_json(),
        "2026-01-02T00:00:00.000Z".into(),
    )
    .unwrap();
    assert_ne!(migrated.manifest.version_id, a.manifest.version_id);
    assert_eq!(
        migrated
            .manifest
            .composition
            .get("derived_from")
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str),
        Some("migration")
    );
    match migrate_bundle(&decoded, "hh-bundle/2", Json::Null, String::new()) {
        Err(BundleError::FormatUnknown { .. }) => {}
        _other => panic!("expected FormatUnknown"),
    }
}

// ── attestation + audit ─────────────────────────────────────────────────────

#[test]
fn attestation_verifies_under_the_key_table() {
    let rig = rig("att");
    let a = run_assembled(&rig);
    let decoded = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let signer = hh_ledger::audit::FixedSigner::new("k1", b"key-1".to_vec());
    let att = sign_attestation(
        &a.manifest.version_id,
        Json::obj([("statement", Json::str("validated"))]),
        "s42.attester",
        "k1",
        b"key-1",
        0,
    );
    let doc = attest(&decoded, &att, &signer).unwrap();
    assert_eq!(
        doc.get("bundle_id").and_then(Json::as_str),
        Some(a.manifest.version_id.as_str())
    );
    // A signature under an unheld key refuses.
    let mut rogue = att.clone();
    rogue.key_id = Some("k2".into());
    match attest(&decoded, &rogue, &signer) {
        Err(BundleError::ProvenanceInvalid { .. }) => {}
        _other => panic!("expected ProvenanceInvalid"),
    }
    // An attestation naming another bundle refuses.
    let other_att = BundleAttestation {
        bundle_id: "idp:other".into(),
        ..att
    };
    match attest(&decoded, &other_att, &signer) {
        Err(BundleError::ProvenanceInvalid { .. }) => {}
        _other => panic!("expected ProvenanceInvalid"),
    }
}

#[test]
fn audit_flags_empty_subject_and_bad_links() {
    let rig = rig("audit");
    let a = run_assembled(&rig);
    let decoded = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let report = audit_bundle(&decoded, &BTreeSet::new());
    assert!(report.ok, "{:?}", report.findings);

    let mut broken = decoded.manifest.clone();
    broken.subject.run_ids = vec![];
    broken.subject.experiment = BTreeMap::new();
    let d2 = hh_bundle::codec::Decoded {
        manifest: broken,
        members: decoded.members.clone(),
    };
    let report = audit_bundle(&d2, &BTreeSet::new());
    assert!(!report.ok);
    assert!(report
        .findings
        .iter()
        .any(|f| f.get("code").and_then(Json::as_str) == Some("subject_empty")));
}

// ── export + fetch ──────────────────────────────────────────────────────────

#[test]
fn export_targets_lower_and_policy_withholds() {
    let rig = rig("exp-t");
    let a = run_assembled(&rig);
    let decoded = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let policy = PublicationPolicy::default();

    let native = export_target(&decoded, "ledger_native", &policy).unwrap();
    assert!(native.files.contains_key("bundle.hhb1"));
    assert_eq!(native.granularity_ceiling, "run");

    let harbor = export_target(&decoded, "harbor_job_dir", &policy).unwrap();
    assert!(harbor.files.contains_key("job.json"));
    assert!(harbor.files.contains_key("digests.json"));
    assert_eq!(harbor.granularity_ceiling, "product");

    match export_target(&decoded, "bogus", &policy) {
        Err(BundleError::FormatUnknown { .. }) => {}
        _other => panic!("expected FormatUnknown"),
    }
}

#[test]
fn fetch_is_digest_checked_and_missing_bytes_refuse() {
    let rig = rig("fetch");
    let a = run_assembled(&rig);
    let mut decoded = hh_bundle::codec::Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    // Flip one member to `fetch` — unmaterialized reads refuse.
    let member_addr = decoded.manifest.members[0].address.clone();
    decoded.manifest.members[0].status = MemberStatus::Fetch;
    decoded
        .manifest
        .fetch
        .push(hh_bundle::manifest::FetchEntry {
            address: member_addr.clone(),
            size: None,
            locations: vec!["mem://pool".into()],
            expires: None,
        });
    decoded.members.remove(&member_addr);
    match member_bytes_or_refused(&decoded, &member_addr) {
        Err(BundleError::FetchRequired { address }) => assert_eq!(address, member_addr),
        _other => panic!("expected FetchRequired"),
    }
    // A fetcher returning wrong bytes fails the digest re-check.
    let out = fetch(
        &decoded,
        std::slice::from_ref(&member_addr),
        Some(&|_| Ok(b"wrong-bytes".to_vec())),
    );
    assert!(out.materialized.is_empty());
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("digest_mismatch")
    );
    // A fetcher that cannot reach the bytes reports a fetch error.
    let out = fetch(
        &decoded,
        std::slice::from_ref(&member_addr),
        Some(&|_| Err("no_transport".to_string())),
    );
    assert!(out.failed[0]
        .get("reason")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("fetch_error:"));
    // No fetcher → not materialized.
    let out = fetch(&decoded, std::slice::from_ref(&member_addr), None);
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("no_fetcher")
    );
}

// ── codec round-trip of a scoped bundle ─────────────────────────────────────

#[test]
fn scoped_bundle_round_trips_through_both_codecs() {
    let rig = rig("codec");
    let rb = run_bundle(&rig, &rig.run, Json::Null);
    let exp = assemble_scoped(&scoped(
        &rig,
        KIND_EXPERIMENT,
        vec![child_of(&rb, "run")],
        vec![rig.run.clone()],
        vec![Json::obj([("arm_id", Json::str("a"))])],
    ))
    .unwrap();
    let out_dir = dir("codec-out");
    encode_dir(&out_dir, &exp.manifest, &exp.members).unwrap();
    let d = decode_dir(&out_dir).unwrap();
    assert_eq!(d.manifest.version_id, exp.manifest.version_id);
    let bytes = encode_container(&exp.manifest, &exp.members);
    let d2 = decode_container(&bytes).unwrap();
    assert_eq!(d2.manifest.version_id, exp.manifest.version_id);
}

// ── foreign import — `harbor_trial_dir` → hosted bundle (AC-R-2.9.3-9) ───────

#[test]
fn harbor_trial_dir_lifts_to_a_hosted_unverified_bundle() {
    let rig = rig("foreign");
    // A fabricated foreign trial tree: a trajectory, a lock file
    // (provider-opaque dependency pin), and a `digests.json` declaring one
    // correct digest and one that mismatches the supplied bytes.
    let traj = b"{\"steps\":[]}".to_vec();
    let lock = b"foreign-dep==9.9.9\n".to_vec();
    let ok_digest = hh_identity::idp_id("blob", &traj);
    let files: BTreeMap<String, Vec<u8>> = [
        ("trial/traj.json".to_string(), traj.clone()),
        ("trial/requirements.txt".to_string(), lock.clone()),
        (
            "trial/digests.json".to_string(),
            Json::obj([
                ("trial/traj.json", Json::str(ok_digest)),
                ("trial/requirements.txt", Json::str("sha256:wrong")),
            ])
            .to_canonical_string()
            .into_bytes(),
        ),
    ]
    .into_iter()
    .collect();
    let lift = hh_bundle::import::lift_foreign(
        &rig.store,
        &rig.other,
        &files,
        "harbor_trial_dir",
        ProvenanceRecord::kernel("s42.test", 0).to_json(),
        "2026-01-01T00:00:00.000Z".into(),
        "sha256:kernel",
        0,
    )
    .unwrap();
    let m = &lift.bundle.manifest;
    // Hosted, R0, foreign-only unpinned (AC-9: claims unverified,
    // provider-opaque content unpinned).
    assert_eq!(m.participant_class, "hosted");
    assert_eq!(
        m.reproducibility
            .get("claimed_level")
            .and_then(Json::as_str),
        Some("R0")
    );
    for role in ["environment", "model"] {
        assert!(m
            .unpinned
            .iter()
            .any(|u| u.role == role && u.reason == "foreign_only"));
    }
    // Every foreign file is a `foreign:<path>` member with a digest claim.
    assert!(m
        .members
        .iter()
        .any(|mm| mm.role == "foreign:trial/traj.json"));
    assert!(m
        .claims
        .iter()
        .any(|c| c.role == "foreign_digest:trial/traj.json"));
    assert!(m
        .claims
        .iter()
        .any(|c| c.role == "foreign_lock_digest:trial/requirements.txt"));
    // The bad declared digest is a recorded conflict — never coerced.
    let conflicts = lift
        .mapping_report
        .get("conflicts")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap();
    assert!(conflicts.iter().any(|c| {
        c.get("code").and_then(Json::as_str) == Some("ForeignIntegrityMismatch")
            && c.get("member").and_then(Json::as_str) == Some("trial/requirements.txt")
    }));
    // ImportRecord is content-addressed, hosted, imported provenance.
    assert_eq!(
        lift.import_record.get("schema").and_then(Json::as_str),
        Some("hh-import/1")
    );
    assert_eq!(
        lift.import_record
            .get("participant_class")
            .and_then(Json::as_str),
        Some("hosted")
    );
    assert!(lift
        .import_record
        .get("mapping_report_ref")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
    let origin = lift
        .import_record
        .get("provenance")
        .and_then(|p| p.get("origin"))
        .unwrap();
    assert_eq!(origin.get("kind").and_then(Json::as_str), Some("import"));
    assert_eq!(
        origin.get("source_system").and_then(Json::as_str),
        Some("harbor_trial_dir")
    );
    // Imported material is `unverified` — never promoted (AC-9).
    assert_eq!(
        lift.import_record
            .get("provenance")
            .and_then(|p| p.get("authority"))
            .and_then(Json::as_str),
        Some("unverified")
    );
    // Unknown formats refuse typed.
    let err = hh_bundle::import::lift_foreign(
        &rig.store,
        &rig.other,
        &files,
        "swebench_submission",
        ProvenanceRecord::kernel("s42.test", 0).to_json(),
        "2026-01-01T00:00:00.000Z".into(),
        "sha256:kernel",
        0,
    );
    match err {
        Err(BundleError::FormatUnknown { .. }) => {}
        Err(other) => panic!("unexpected lift error: {other}"),
        Ok(_) => panic!("unknown format lifted"),
    }
}
