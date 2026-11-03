//! R5.21 — `evidence-package/1` typed member/role profile inside
//! `hh-bundle/1` (spec §6.1; PADR-R5-3∗ Packet C; AC-1/AC-2): the
//! admitted versioned profile, create-if-absent attempt/grade/export
//! identities, typed head/watermark snapshot coordinates, and the
//! closed refusal set — conflicting identity, wrong head/last exported
//! event, omitted required metadata, grade retry adding n, build-note
//! as WAL, unknown mandatory version. Legacy manifests validate
//! unchanged.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_bundle::assemble::{assemble, AssembleInputs};
use hharness_bundle::error::BundleError;
use hharness_bundle::manifest::{BundlePolicy, MemberStatus};
use hharness_bundle::profile::{
    self, AttemptRef, Coord, EvidenceProfile, ExportRef, GradeAttemptRef, IdentityOutcome,
    NativeHead, ProfileIndex, Watermark,
};
use hharness_bundle::validate::{self, CheckStatus};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-r521-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

struct Rig {
    store: Store,
    run: String,
    lease: Lease,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

fn rig_open(tag: &str, events: Vec<Event>) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.participant_class = ParticipantClass::Native;
    let (run, lease) = store.open_run(m, "r521.test").unwrap();
    store.append(&run, &lease, events).unwrap();
    Rig {
        store,
        run,
        lease,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn rig(tag: &str) -> Rig {
    rig_open(
        tag,
        vec![
            ev("evt-1", "lifecycle.run.finished", Json::obj([])),
            ev(
                "evt-2",
                "measurement.export.delivered",
                Json::obj([("sink_id", Json::str("file:/tmp/out"))]),
            ),
        ],
    )
}

fn inputs<'a>(rig: &'a Rig, evidence_profile: Option<EvidenceProfile>) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("r521.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
        evidence_profile,
    }
}

fn head_coord(rig: &Rig) -> Coord {
    let h = rig.store.head(&rig.run).unwrap();
    Coord {
        seq: h.seq,
        event_id: h.event_id.clone(),
        hash: h.hash.clone(),
    }
}

fn attempt(id: &str, rig: &Rig, ordinal: u64, predecessor: Option<&str>) -> AttemptRef {
    AttemptRef {
        attempt_id: id.to_string(),
        plan_id: "plan-1".into(),
        subject_run: Some(rig.run.clone()),
        launch_binding: Some("launch_bound:evt-1".into()),
        ordinal,
        predecessor: predecessor.map(String::from),
        reason: if predecessor.is_some() {
            "retry:fixture".into()
        } else {
            "first".into()
        },
        settlement: Some("run_settled:evt-2".into()),
    }
}

fn grade(id: &str, attempt_id: &str, ordinal: u64, counts: bool) -> GradeAttemptRef {
    GradeAttemptRef {
        grade_attempt_id: id.to_string(),
        attempt_id: attempt_id.to_string(),
        instrument_event: Some("instrument:evt-1".into()),
        sealed_subject_digest: Some("sha256:subj".into()),
        sealed_submission_digest: Some("sha256:sub".into()),
        oracle: Some("oracle:exact".into()),
        oracle_version: Some("sha256:oracle-v1".into()),
        ordinal,
        selection: Some("selection:evt-2".into()),
        supersedes: vec![],
        counts_subject: counts,
    }
}

fn export(id: &str, rig: &Rig) -> ExportRef {
    ExportRef {
        export_id: id.to_string(),
        run_id: rig.run.clone(),
        target: "file:/tmp/out".into(),
        last_event: head_coord(rig),
        watermark: head_coord(rig).seq,
    }
}

/// A minimal valid profile bound to the rig's run + current head.
fn profile(rig: &Rig) -> EvidenceProfile {
    let head = head_coord(rig);
    EvidenceProfile {
        profile_version: profile::PROFILE_VERSION,
        source_versions: Json::obj([
            ("code_commit", Json::str("abc123")),
            ("dirty_patch_member", Json::Null),
            ("dirty_unknown", Json::Bool(false)),
            ("runtime", Json::str("hh-kernel/0.1.0")),
            ("toolchain", Json::str("rustc 1.94")),
            ("features", Json::Arr(vec![])),
        ]),
        claims: vec![Json::obj([
            ("id", Json::str("claim-1")),
            ("text", Json::str("the bundle validates")),
            ("scope", Json::str("bundle")),
            ("class", Json::str("native")),
            ("population", Json::str("fixture")),
            ("native_refs", Json::Arr(vec![])),
            ("report_refs", Json::Arr(vec![])),
        ])],
        protocol_ref: Json::Null,
        phase: "pilot".into(),
        slot_set: Json::obj([
            ("plans", Json::Arr(vec![Json::str("plan-1")])),
            ("hash", Json::str("sha256:slotset")),
            ("count", Json::Int(1)),
            ("close_reason", Json::Null),
            ("watermark", Json::Int(head.seq as i64)),
        ]),
        attempts: vec![attempt("att-1", rig, 1, None)],
        grade_attempts: vec![grade("ga-1", "att-1", 1, true)],
        exports: vec![export("exp-1", rig)],
        native_heads: vec![NativeHead {
            run_id: rig.run.clone(),
            kind: "subject".into(),
            coord: head.clone(),
        }],
        watermarks: vec![Watermark {
            run_id: rig.run.clone(),
            projection: "ledger".into(),
            seq: head.seq,
        }],
        pins: Json::obj([
            ("task", Json::Null),
            ("models", Json::Null),
            ("treatment", Json::Null),
            ("usage", Json::Null),
        ]),
        access: Json::obj([
            ("supersedes", Json::Arr(vec![])),
            ("losses", Json::Arr(vec![])),
            ("policy", Json::str("")),
            ("rights", Json::str("")),
            ("recipient_class", Json::str("")),
            ("retention", Json::str("")),
            ("owner", Json::str("")),
        ]),
        legacy_refs: vec![],
    }
}

fn refused_code(e: &BundleError) -> &str {
    match e {
        BundleError::ProfileRefused { code, .. } => code,
        other => panic!("expected ProfileRefused, got {other}"),
    }
}

/// Restamp `manifest` so `version_id` recomputes over the mutated
/// preimage (the validate-side tamper path — a manifest that *arrived*
/// with those bytes).
fn restamp(m: &mut hharness_bundle::manifest::BundleManifest) {
    m.version_id = m.compute_id();
}

fn fails_with_code(report: &validate::BundleValidationReport, code: &str) -> bool {
    report
        .stages
        .iter()
        .flat_map(|s| s.checks.iter())
        .any(|c| c.status == CheckStatus::Fail && c.code.as_deref() == Some(code))
}

// ── AC-1: admitted versioned profile; create-if-absent identities;
//        typed coordinates; same-day retries retained; immutable
//        snapshot under later append. ────────────────────────────────

#[test]
fn profile_member_round_trips_and_bundle_validates() {
    let rig = rig("roundtrip");
    let p = profile(&rig);
    // Record-level round trip.
    assert_eq!(
        EvidenceProfile::from_json(&p.to_json()).unwrap(),
        p,
        "profile canonical JSON round-trips"
    );
    assert!(p.check().is_empty(), "valid profile has no violations");

    let a = assemble(&inputs(&rig, Some(p.clone()))).unwrap();
    // The section names the admitted member; the member carries bytes.
    let section = &a.manifest.evidence_profile;
    assert_eq!(
        section.get("schema").and_then(Json::as_str),
        Some(profile::PROFILE_SCHEMA)
    );
    let member_addr = section
        .get("member")
        .and_then(Json::as_str)
        .expect("evidence_profile.member");
    let member = a
        .manifest
        .members
        .iter()
        .find(|m| m.role == profile::PROFILE_ROLE)
        .expect("evidence_profile member ref");
    assert_eq!(member.address, member_addr);
    assert_eq!(member.status, MemberStatus::Present);
    assert!(a.members.contains_key(member_addr));

    // The member decodes to the same record.
    let decoded = EvidenceProfile::from_json(
        &hharness_wire::json::parse(std::str::from_utf8(&a.members[member_addr]).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(decoded, p);

    // The bundle validates complete; the profile's coordinate agrees
    // with the subject head the export carries.
    let report = validate::validate(&a.manifest, &a.members);
    assert!(
        report.complete,
        "bundle with profile should validate: {:?}",
        report
            .stages
            .iter()
            .flat_map(|s| s.checks.iter())
            .filter(|c| c.status == CheckStatus::Fail)
            .collect::<Vec<_>>()
    );
    let subject_head = a.manifest.subject.heads.get(&rig.run).unwrap();
    assert_eq!(
        subject_head.get("seq").and_then(Json::as_int),
        Some(head_coord(&rig).seq as i64)
    );
}

#[test]
fn create_if_absent_idempotent_and_retries_retained() {
    let rig = rig("create");
    let mut index = ProfileIndex::default();
    let a1 = attempt("att-1", &rig, 1, None);
    assert_eq!(index.create_attempt(&a1).unwrap(), IdentityOutcome::Created);
    // Same identity + identical bytes — idempotent re-mint, no second
    // record.
    assert_eq!(
        index.create_attempt(&a1).unwrap(),
        IdentityOutcome::Retained
    );
    // A same-day retry is a *distinct* identity — retained beside the
    // first, never collapsed.
    let a2 = attempt("att-2", &rig, 2, Some("att-1"));
    assert_eq!(index.create_attempt(&a2).unwrap(), IdentityOutcome::Created);
    let g1 = grade("ga-1", "att-1", 1, true);
    let e1 = export("exp-1", &rig);
    assert_eq!(
        index.create_grade_attempt(&g1).unwrap(),
        IdentityOutcome::Created
    );
    assert_eq!(index.create_export(&e1).unwrap(), IdentityOutcome::Created);
    let ids: Vec<&str> = index.ids().collect();
    assert_eq!(ids, vec!["att-1", "att-2", "exp-1", "ga-1"]);
}

#[test]
fn later_append_cannot_mutate_old_snapshot() {
    let mut rig = rig_open(
        "append",
        vec![ev("evt-1", "lifecycle.component.bound", Json::obj([]))],
    );
    let snapshot_head = head_coord(&rig);
    let a = assemble(&inputs(&rig, Some(profile(&rig)))).unwrap();
    let addr = a
        .manifest
        .evidence_profile
        .get("member")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // A later append advances the store's head.
    rig.store
        .append(
            &rig.run,
            &rig.lease,
            vec![ev("evt-3", "lifecycle.component.bound", Json::obj([]))],
        )
        .unwrap();
    let new_head = rig.store.head(&rig.run).unwrap();
    assert!(new_head.seq > snapshot_head.seq, "the append advanced head");

    // The recorded profile still asserts the snapshot head — content
    // addressing means the old snapshot's bytes cannot change.
    let old = EvidenceProfile::from_json(
        &hharness_wire::json::parse(std::str::from_utf8(&a.members[&addr]).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(old.native_heads[0].coord, snapshot_head);
    assert_eq!(old.watermarks[0].seq, snapshot_head.seq);
    assert_eq!(old.exports[0].last_event, snapshot_head);

    // A *new* snapshot records the new head; the old is untouched.
    let a2 = assemble(&inputs(&rig, Some(profile(&rig)))).unwrap();
    let head2 = a2.manifest.subject.heads.get(&rig.run).unwrap();
    assert_eq!(
        head2.get("seq").and_then(Json::as_int),
        Some(new_head.seq as i64)
    );
}

// ── AC-2: the closed refusal set. ───────────────────────────────────

#[test]
fn same_id_different_bytes_refuses() {
    let rig = rig("conflict");
    let mut index = ProfileIndex::default();
    index
        .create_attempt(&attempt("att-1", &rig, 1, None))
        .unwrap();
    let e = index
        .create_attempt(&attempt("att-1", &rig, 9, Some("att-0")))
        .err()
        .unwrap();
    assert_eq!(refused_code(&e), profile::code::CONFLICTING_IDENTITY);

    // The same conflict inside a decoded record: two attempts carry
    // one id.
    let mut p = profile(&rig);
    p.attempts.push(attempt("att-1", &rig, 9, Some("att-0")));
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::CONFLICTING_IDENTITY));
}

#[test]
fn wrong_head_and_last_exported_event_refuse() {
    let rig = rig("heads");
    // Assembly-side: declared subject head ≠ the durable prefix.
    let mut p = profile(&rig);
    p.native_heads[0].coord.seq += 1;
    let e = assemble(&inputs(&rig, Some(p))).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::HEAD_MISMATCH);

    // Export last_event ≠ the exported prefix's head.
    let mut p = profile(&rig);
    p.exports[0].last_event.seq = 0;
    let e = assemble(&inputs(&rig, Some(p))).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::HEAD_MISMATCH);

    // A projection watermark ahead of head.
    let mut p = profile(&rig);
    p.watermarks[0].seq += 1;
    let e = assemble(&inputs(&rig, Some(p))).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::WATERMARK_MISMATCH);

    // Profile-internal: export last_event ≠ recorded head.
    let mut p = profile(&rig);
    p.exports[0].last_event.hash = "sha256:forged".into();
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::HEAD_MISMATCH));
    // An export naming a run with no recorded head.
    let mut p = profile(&rig);
    p.exports[0].run_id = "run-nowhere".into();
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::MISSING_HEAD));

    // Validate-side: a member whose declared subject head disagrees
    // with the manifest's own subject.heads — tamper + restamp.
    let a = assemble(&inputs(&rig, Some(profile(&rig)))).unwrap();
    let mut m = a.manifest.clone();
    let mut members = a.members.clone();
    let addr = m
        .evidence_profile
        .get("member")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut doc =
        hharness_wire::json::parse(std::str::from_utf8(&members[&addr]).unwrap()).unwrap();
    if let Json::Obj(map) = &mut doc {
        if let Some(Json::Arr(hs)) = map.get_mut("native_heads") {
            if let Json::Obj(h) = &mut hs[0] {
                h.insert("seq".into(), Json::Int(0));
                h.insert("hash".into(), Json::str("sha256:stale"));
            }
        }
    }
    let bytes = doc.to_canonical_string().into_bytes();
    let (new_addr, _size) = (
        hharness_identity::address(&bytes, profile::PROFILE_MEDIA).id(),
        bytes.len() as u64,
    );
    members.insert(new_addr.clone(), bytes);
    if let Some(mm) = m.members.iter_mut().find(|mm| mm.address == addr) {
        mm.address = new_addr.clone();
    }
    if let Json::Obj(sec) = &mut m.evidence_profile {
        sec.insert("member".into(), Json::str(new_addr));
    }
    restamp(&mut m);
    let report = validate::validate(&m, &members);
    assert!(fails_with_code(&report, profile::code::HEAD_MISMATCH));
    assert!(!report.complete);
}

#[test]
fn omitted_required_empty_metadata_refuses() {
    let rig = rig("required");
    let doc = profile(&rig).to_json();
    // Each R member is required even when its legal value is empty.
    for key in [
        "claims",
        "access",
        "watermarks",
        "native_heads",
        "pins",
        "slot_set",
        "source_versions",
    ] {
        let Json::Obj(map) = doc.clone() else {
            panic!()
        };
        let mut map = map;
        map.remove(key);
        let e = EvidenceProfile::from_json(&Json::Obj(map)).err().unwrap();
        assert_eq!(
            refused_code(&e),
            profile::code::MISSING_REQUIRED,
            "absent {key}"
        );
    }
    // A missing required *submember* refuses the same way —
    // `supersedes: []` may be empty, it may not be absent.
    let Json::Obj(mut map) = doc.clone() else {
        panic!()
    };
    let mut access = match map.remove("access").unwrap() {
        Json::Obj(a) => a,
        _ => panic!(),
    };
    access.remove("supersedes");
    map.insert("access".into(), Json::Obj(access));
    let e = EvidenceProfile::from_json(&Json::Obj(map)).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::MISSING_REQUIRED);
}

#[test]
fn grade_retry_adding_n_refuses() {
    let rig = rig("n");
    // A retry (ordinal > 1) that counts n.
    let mut p = profile(&rig);
    p.grade_attempts.push(grade("ga-2", "att-1", 2, true));
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::GRADE_RETRY_ADDS_N));
    let e = assemble(&inputs(&rig, Some(p))).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::GRADE_RETRY_ADDS_N);

    // A superseding grade that counts n.
    let mut p = profile(&rig);
    let mut g2 = grade("ga-2", "att-1", 1, true);
    g2.supersedes = vec!["ga-1".into()];
    p.grade_attempts[0].counts_subject = false;
    p.grade_attempts.push(g2);
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::GRADE_RETRY_ADDS_N));

    // Two counting grades on one attempt — the second adds n.
    let mut p = profile(&rig);
    p.grade_attempts.push(grade("ga-2", "att-1", 1, true));
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::GRADE_RETRY_ADDS_N));

    // The legal retry — ordinal 2, counts_subject false — stands.
    let mut p = profile(&rig);
    let mut g2 = grade("ga-2", "att-1", 2, false);
    g2.supersedes = vec!["ga-1".into()];
    p.grade_attempts.push(g2);
    assert!(p.check().is_empty());
}

#[test]
fn build_note_replacing_wal_refuses() {
    let rig = rig("buildnote");
    // A build-note label in the WAL coordinate slot.
    let mut p = profile(&rig);
    p.native_heads.push(NativeHead {
        run_id: "build-ledger-1".into(),
        kind: "build-note".into(),
        coord: head_coord(&rig),
    });
    assert!(p
        .check()
        .iter()
        .any(|v| v.code == profile::code::BUILD_NOTE_AS_WAL));
    let e = assemble(&inputs(&rig, Some(p))).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::BUILD_NOTE_AS_WAL);

    // A legacy ref is legal only as the labelled build-note.
    let mut p = profile(&rig);
    p.legacy_refs = vec![Json::obj([
        ("label", Json::str("native_head")),
        ("member", Json::str("sha256:doc")),
        ("digest", Json::str("sha256:x")),
    ])];
    assert!(p.check().iter().any(|v| v.code == profile::code::MALFORMED));
    p.legacy_refs = vec![Json::obj([
        ("label", Json::str(profile::BUILD_NOTE_LABEL)),
        ("member", Json::str("sha256:doc")),
        ("digest", Json::str("sha256:x")),
    ])];
    assert!(p.check().is_empty());
}

#[test]
fn legacy_stays_valid_and_unknown_version_refuses() {
    let rig = rig("legacy");
    // No profile — the manifest carries no section, decodes
    // byte-identically, and validates complete (legacy bytes/fields/
    // levels remain valid).
    let a = assemble(&inputs(&rig, None)).unwrap();
    assert_eq!(a.manifest.evidence_profile, Json::Null);
    let doc = a.manifest.to_json();
    assert!(doc.get("evidence_profile").is_none());
    let back = hharness_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    assert_eq!(back.compute_id(), a.manifest.version_id);
    let report = validate::validate(&a.manifest, &a.members);
    assert!(report.complete);

    // An unknown mandatory version refuses the new claim while
    // inspection is preserved — the decode refusal names the version,
    // `peek_version` still reports it.
    let mut p = profile(&rig);
    p.profile_version = 2;
    let e = EvidenceProfile::from_json(&p.to_json()).err().unwrap();
    assert_eq!(refused_code(&e), profile::code::UNKNOWN_PROFILE_VERSION);
    assert_eq!(profile::peek_version(&p.to_json()), Some(2));

    // An additive unknown member decodes — optional additions cannot
    // strengthen old evidence, they never break inspection.
    let Json::Obj(mut map) = profile(&rig).to_json() else {
        panic!()
    };
    map.insert("future_field".into(), Json::Int(42));
    assert!(EvidenceProfile::from_json(&Json::Obj(map)).is_ok());

    // A bundle whose evidence_profile section declares an unknown
    // family member: the manifest still decodes (inspection) while the
    // validation fails the claim.
    let a = assemble(&inputs(&rig, Some(profile(&rig)))).unwrap();
    let mut m = a.manifest.clone();
    if let Json::Obj(sec) = &mut m.evidence_profile {
        sec.insert("schema".into(), Json::str("evidence-package/2"));
    }
    restamp(&mut m);
    let report = validate::validate(&m, &a.members);
    assert!(fails_with_code(
        &report,
        profile::code::UNKNOWN_PROFILE_VERSION
    ));
    assert!(!report.complete);
}
