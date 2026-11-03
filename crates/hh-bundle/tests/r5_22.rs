//! R5.22 — `closure_walk`/`closure_report` owner-typed recipient
//! closure (spec §6.2; PADR-R5-3∗ C.1 items 3–4; AC-1/AC-2):
//! ordinary and payload/IR/cause/lineage root enumeration with owner
//! types + `{parent, role, field, media, size}` provenance, visited
//! dedup + explicit limits, truthful member/fetch/redacted/gc
//! resolution, and the typed failure set — payload-only loss, wrong
//! digest, unsupported schema/unknown class, traversal exhaustion,
//! cycles, unauthorized/expired fetch, missing bytes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_bundle::assemble::{assemble, AssembleInputs, Assembled};
use hharness_bundle::closure::{
    self, closure_walk, ClosureLimits, FetchPermit, Resolution, WalkOpts,
};
use hharness_bundle::codec::Decoded;
use hharness_bundle::export::add_member;
use hharness_bundle::manifest::{BundlePolicy, FetchEntry, LedgerExport, MemberRef, MemberStatus};
use hharness_bundle::profile::{self, EvidenceProfile};
use hharness_bundle::trust::FetchTrustPolicy;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::manifest::{EventRef, ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-r522-{}-{tag}-{}",
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
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
    blob_addr: String,
    blob_bytes: Vec<u8>,
}

/// A finished run whose second event carries a same-run cause, an
/// offloaded-blob `refs[]` ref and an `ir_refs[]` ref to the
/// definition member.
fn rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let blob_bytes = b"sealed submission payload bytes".to_vec();
    let blob = store
        .put_blob(&blob_bytes, "application/octet-stream")
        .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    // The fixture's config refs name carried members so the ordinary
    // root class closes (the manifest codec declares them; a ref the
    // bundle does not carry resolves unresolved — T4/T7 exercise it).
    m.configuration_id = Some(def_addr.clone());
    m.configuration_version_id = Some(def_addr.clone());
    m.participant_class = ParticipantClass::Native;
    let (run, lease) = store.open_run(m, "r522.test").unwrap();
    let _ = &lease;
    let mut e2 = ev(
        "evt-2",
        "measurement.export.delivered",
        Json::obj([("sink_id", Json::str("file:/tmp/out"))]),
    );
    e2.causes = vec![EventRef {
        run_id: run.clone(),
        event_id: "evt-1".into(),
    }];
    e2.refs = vec![blob.clone()];
    e2.ir_refs = vec![hharness_identity::rowkeys::IrRef::new(
        def_addr.clone(),
        None,
    )];
    // Causes resolve at append against the *committed* prefix —
    // land the cause target first.
    store
        .append(
            &run,
            &lease,
            vec![ev("evt-1", "lifecycle.component.bound", Json::obj([]))],
        )
        .unwrap();
    store
        .append(
            &run,
            &lease,
            vec![e2, ev("evt-3", "lifecycle.run.finished", Json::obj([]))],
        )
        .unwrap();
    Rig {
        store,
        run,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
        blob_addr: blob.id(),
        blob_bytes,
    }
}

fn inputs<'a>(rig: &'a Rig, evidence_profile: Option<EvidenceProfile>) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("r522.test", 0).to_json(),
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

/// A profile whose declared refs all name carried content — claim
/// `native_refs`/`report_refs` address members, coordinate refs name
/// decoded events/runs, sealed digests name deposited members.
fn profile(rig: &Rig, extra: &BTreeMap<String, Vec<u8>>) -> EvidenceProfile {
    let h = rig.store.head(&rig.run).unwrap();
    let coord = profile::Coord {
        seq: h.seq,
        event_id: h.event_id.clone(),
        hash: h.hash.clone(),
    };
    let member_addr = |tag: &str| extra.get(tag).unwrap().clone();
    let sealed_addr =
        hharness_identity::address(&member_addr("subj"), "application/octet-stream").id();
    let sub_addr = hharness_identity::address(&member_addr("sub"), "application/octet-stream").id();
    let oracle_addr =
        hharness_identity::address(&member_addr("oracle"), "application/octet-stream").id();
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
            ("text", Json::str("the run's evidence closes")),
            ("scope", Json::str("run")),
            ("class", Json::str("native")),
            ("population", Json::str("fixture")),
            (
                "native_refs",
                Json::Arr(vec![Json::str(sealed_addr.clone())]),
            ),
            ("report_refs", Json::Arr(vec![Json::str(sub_addr.clone())])),
        ])],
        protocol_ref: Json::Null,
        phase: "pilot".into(),
        slot_set: Json::obj([
            ("plans", Json::Arr(vec![Json::str("plan-1")])),
            ("hash", Json::str("sha256:slotset")),
            ("count", Json::Int(1)),
            ("close_reason", Json::Null),
            ("watermark", Json::Int(h.seq as i64)),
        ]),
        attempts: vec![profile::AttemptRef {
            attempt_id: "att-1".into(),
            plan_id: "plan-1".into(),
            subject_run: Some(rig.run.clone()),
            launch_binding: Some("lifecycle.component.bound:evt-1".into()),
            ordinal: 1,
            predecessor: None,
            reason: "first".into(),
            settlement: Some("measurement.export.delivered:evt-2".into()),
        }],
        grade_attempts: vec![profile::GradeAttemptRef {
            grade_attempt_id: "ga-1".into(),
            attempt_id: "att-1".into(),
            instrument_event: Some("measurement.export.delivered:evt-2".into()),
            sealed_subject_digest: Some(sealed_addr),
            sealed_submission_digest: Some(sub_addr),
            oracle: Some(oracle_addr.clone()),
            oracle_version: Some(oracle_addr),
            ordinal: 1,
            selection: Some("lifecycle.run.finished:evt-3".into()),
            supersedes: vec![],
            counts_subject: true,
        }],
        exports: vec![profile::ExportRef {
            export_id: "exp-1".into(),
            run_id: rig.run.clone(),
            target: "file:/tmp/out".into(),
            last_event: coord.clone(),
            watermark: h.seq,
        }],
        native_heads: vec![profile::NativeHead {
            run_id: rig.run.clone(),
            kind: "subject".into(),
            coord,
        }],
        watermarks: vec![profile::Watermark {
            run_id: rig.run.clone(),
            projection: "ledger".into(),
            seq: h.seq,
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

/// Assemble + deposit the offloaded blob and the declared-ref members
/// (what a producer carrying its payload refs would do), then hand the
/// recipient `Decoded{manifest, members}` — no store, no pool, no
/// credentials: the walk's whole world is the recipient's bytes.
fn bundle(rig: &Rig) -> Decoded {
    let mut extra = BTreeMap::new();
    extra.insert("subj".to_string(), b"sealed subject".to_vec());
    extra.insert("sub".to_string(), b"sealed submission".to_vec());
    extra.insert("oracle".to_string(), b"oracle v1".to_vec());
    let Assembled {
        mut manifest,
        mut members,
        ..
    } = assemble(&inputs(rig, Some(profile(rig, &extra)))).unwrap();
    for (tag, bytes) in &extra {
        let (addr, size) = add_member(&mut members, bytes.clone(), "application/octet-stream");
        manifest.members.push(MemberRef::present(
            format!("declared:{tag}"),
            addr,
            "application/octet-stream",
            size,
        ));
    }
    let (baddr, bsize) = add_member(
        &mut members,
        rig.blob_bytes.clone(),
        "application/octet-stream",
    );
    assert_eq!(
        baddr, rig.blob_addr,
        "offloaded blob address is content-derived"
    );
    manifest.members.push(MemberRef::present(
        "blob:submission",
        baddr,
        "application/octet-stream",
        bsize,
    ));
    manifest.version_id = manifest.compute_id();
    Decoded { manifest, members }
}

fn walk(d: &Decoded) -> hharness_bundle::closure::ClosureReport {
    closure_walk(
        d,
        WalkOpts {
            procedure: "verify",
            recipient_class: "controlled",
            checked_at: TS,
            limits: ClosureLimits::default(),
            fetch: None,
        },
    )
}

fn failure_codes(r: &hharness_bundle::closure::ClosureReport) -> Vec<String> {
    r.failures
        .iter()
        .filter_map(|f| f.get("code").and_then(Json::as_str).map(String::from))
        .collect()
}

// ── AC-1 ───────────────────────────────────────────────────────────

#[test]
fn full_closure_resolves_with_owner_typed_provenance() {
    let rig = rig("t1");
    let d = bundle(&rig);
    let r = walk(&d);
    assert_eq!(r.verdict, "closed", "failures: {:?}", r.failures);
    assert_eq!(r.unresolved, 0);
    assert_eq!(r.bundle_id, d.manifest.version_id);
    assert_eq!(r.profile_version, Some(profile::PROFILE_VERSION));
    assert_eq!(r.procedure, "verify");
    assert_eq!(r.recipient_class, "controlled");
    assert_eq!(r.required, r.edges.len());
    assert_eq!(r.resolved, r.required);
    // Every enumerated root class is present with its owner.
    let kinds: Vec<&str> = r.edges.iter().map(|e| e.kind).collect();
    for k in [
        "member", "section", "trace", "tree", "page", "blob", "head", "cause", "payload", "ir",
        "ordinary", "declared",
    ] {
        assert!(kinds.contains(&k), "missing root class {k}");
    }
    for owner in [
        "hh-bundle.manifest",
        "evidence-package/1",
        "hh-ledger.blob_index",
        "hh-bundle.run_refs",
    ] {
        assert!(
            r.edges.iter().any(|e| e.owner == owner),
            "missing owner {owner}"
        );
    }
    assert!(r
        .edges
        .iter()
        .any(|e| e.owner.starts_with("hh-ledger.envelope:")));
    // Edge provenance is the dedup tuple.
    let cause = r.edges.iter().find(|e| e.kind == "cause").unwrap();
    assert!(cause.parent.starts_with("event:"));
    assert_eq!(cause.target, format!("{}:evt-1", rig.run));
    // Report round-trips to canonical JSON with the contract fields.
    let j = r.to_json();
    for k in [
        "schema",
        "bundle_id",
        "profile_version",
        "procedure",
        "recipient_class",
        "checked_at",
        "required",
        "resolved",
        "withheld",
        "unresolved",
        "verdict",
        "failures",
    ] {
        assert!(j.get(k).is_some(), "report missing {k}");
    }
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some(closure::CLOSURE_REPORT_SCHEMA)
    );
}

#[test]
fn visited_dedup_and_explicit_limits() {
    let rig = rig("t2");
    let d = bundle(&rig);
    // Two edges naming the def member (member row + section ref +
    // run_ref + ir edge) stay distinct by field but the same member's
    // `members[i]` edge dedups against a second identical row.
    let r = walk(&d);
    let member_edges: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.kind == "member" && e.target == rig.def_addr)
        .collect();
    assert_eq!(
        member_edges.len(),
        1,
        "the dedup key collapses exact repeats"
    );
    // Limits: edge bound.
    let r = closure_walk(
        &d,
        WalkOpts {
            procedure: "verify",
            recipient_class: "controlled",
            checked_at: TS,
            limits: ClosureLimits {
                max_edges: 4,
                ..Default::default()
            },
            fetch: None,
        },
    );
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::TRAVERSAL_LIMIT.to_string()));
    // Limits: event bound.
    let r = closure_walk(
        &d,
        WalkOpts {
            procedure: "verify",
            recipient_class: "controlled",
            checked_at: TS,
            limits: ClosureLimits {
                max_events: 1,
                ..Default::default()
            },
            fetch: None,
        },
    );
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::TRAVERSAL_LIMIT.to_string()));
}

#[test]
fn member_statuses_resolve_truthfully() {
    let rig = rig("t3");
    let mut d = bundle(&rig);
    // A fetch member + its entry: materializes only through the
    // caller-supplied fetch permit.
    let fbytes = b"remote doc bytes".to_vec();
    let faddr = hharness_identity::address(&fbytes, "application/octet-stream").id();
    let mut fetch_member =
        MemberRef::present("extra_doc", faddr.clone(), "application/octet-stream", 0);
    fetch_member.status = MemberStatus::Fetch;
    fetch_member.size = fbytes.len() as u64;
    d.manifest.members.push(fetch_member);
    d.manifest.fetch.push(FetchEntry {
        address: faddr.clone(),
        size: Some(fbytes.len() as u64),
        locations: vec![],
        expires: None,
    });
    // Explicit losses.
    for (role, status) in [
        ("held_doc", MemberStatus::Redacted),
        ("gone_doc", MemberStatus::Gc),
    ] {
        let mut m = MemberRef::present(
            role,
            format!("sha256:{role}"),
            "application/octet-stream",
            0,
        );
        m.status = status;
        d.manifest.members.push(m);
    }
    // No permit → the fetch member withholds; verdict `withheld`.
    let r = walk(&d);
    assert_eq!(r.verdict, "withheld", "failures: {:?}", r.failures);
    assert_eq!(r.unresolved, 0);
    assert_eq!(r.withheld, 3);
    let fe = r.edges.iter().find(|e| e.target == faddr).unwrap();
    assert_eq!(fe.resolution, Resolution::Withheld { status: "fetch" });
    // Permitted → the pool leg materializes + verifies.
    let pool = |_: &FetchEntry| Ok(fbytes.clone());
    let r = closure_walk(
        &d,
        WalkOpts {
            procedure: "verify",
            recipient_class: "controlled",
            checked_at: TS,
            limits: ClosureLimits::default(),
            fetch: Some(FetchPermit {
                pool: Some(&pool),
                trust: None,
                remote: None,
                now_ms: 1_700_000_000_000,
            }),
        },
    );
    assert_eq!(r.verdict, "withheld", "failures: {:?}", r.failures);
    let fe = r.edges.iter().find(|e| e.target == faddr).unwrap();
    assert_eq!(fe.resolution, Resolution::Resolved { via: "fetch" });
}

#[test]
fn legacy_manifest_without_profile_still_walks() {
    let rig = rig("t8");
    let Assembled {
        manifest, members, ..
    } = assemble(&inputs(&rig, None)).unwrap();
    let d = Decoded { manifest, members };
    let r = walk(&d);
    assert_eq!(r.profile_version, None);
    // The blob_index present row has no member bytes — honest loss.
    assert!(failure_codes(&r).contains(&closure::code::MISSING_BYTES.to_string()));
    assert_eq!(r.verdict, "failed");
}

// ── AC-2 ───────────────────────────────────────────────────────────

#[test]
fn payload_only_loss_and_wrong_digest_fail() {
    let rig = rig("t4");
    let d = bundle(&rig);
    // Removing the blob member's bytes: the payload ref + blob_index
    // present rows cannot resolve.
    let mut d2 = Decoded {
        manifest: d.manifest.clone(),
        members: d.members.clone(),
    };
    d2.members.remove(&rig.blob_addr);
    let r = walk(&d2);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::MISSING_BYTES.to_string()));
    // Wrong bytes under the address.
    let mut d3 = Decoded {
        manifest: d.manifest.clone(),
        members: d.members.clone(),
    };
    d3.members.insert(rig.blob_addr.clone(), b"forged".to_vec());
    let r = walk(&d3);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::DIGEST_MISMATCH.to_string()));
    // A missing page member.
    let page_addr = d.manifest.traces[&rig.run].pages[0].clone();
    let mut d4 = Decoded {
        manifest: d.manifest.clone(),
        members: d.members.clone(),
    };
    d4.members.remove(&page_addr);
    let r = walk(&d4);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::MISSING_BYTES.to_string()));
}

#[test]
fn unsupported_schema_and_unknown_class_fail() {
    let rig = rig("t5");
    let d = bundle(&rig);
    // A contains member declaring hh-bundle/2.
    let mut d2 = Decoded {
        manifest: d.manifest.clone(),
        members: d.members.clone(),
    };
    let child = Json::obj([
        ("schema", Json::str("hh-bundle/2")),
        ("idp", Json::str("idp/1")),
    ])
    .to_canonical_string()
    .into_bytes();
    let (caddr, csize) = add_member(&mut d2.members, child, "application/json");
    d2.manifest.members.push(MemberRef::present(
        "contains:sha256:child",
        caddr.clone(),
        "application/json",
        csize,
    ));
    d2.manifest.composition = Json::obj([(
        "contains",
        Json::Arr(vec![Json::obj([
            ("bundle_id", Json::str("sha256:child")),
            ("kind", Json::str("run")),
            ("member", Json::str(caddr.clone())),
        ])]),
    )]);
    let r = walk(&d2);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::UNSUPPORTED_SCHEMA.to_string()));
    // An unregistered event class in a decoded page — minted member.
    let mut d3 = Decoded {
        manifest: d.manifest.clone(),
        members: d.members.clone(),
    };
    let page_addr = d3.manifest.traces[&rig.run].pages[0].clone();
    let mut doc =
        hharness_wire::json::parse(std::str::from_utf8(&d3.members[&page_addr]).unwrap()).unwrap();
    if let Json::Obj(map) = &mut doc {
        if let Some(Json::Arr(evs)) = map.get_mut("events") {
            if let Json::Obj(e) = &mut evs[0] {
                e.insert("class".into(), Json::str("unregistered.fake.class"));
            }
        }
    }
    let page_bytes = doc.to_canonical_string().into_bytes();
    let (paddr, psize) = add_member(
        &mut d3.members,
        page_bytes,
        "application/vnd.hh.ledger-page+json",
    );
    d3.manifest
        .members
        .iter_mut()
        .find(|m| m.address == page_addr)
        .unwrap()
        .address = paddr.clone();
    let export = d3.manifest.traces.get_mut(&rig.run).unwrap();
    export.pages[0] = paddr;
    export.tree = LedgerExport::tree_address(&export.pages);
    let _ = psize;
    let r = walk(&d3);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::UNKNOWN_CLASS.to_string()));
}

#[test]
fn lineage_cycle_terminates_and_cannot_pass() {
    let rig = rig("t6");
    let mut d = bundle(&rig);
    // A fabricated lineage prefix naming the run itself — the walk
    // terminates (visiting set) and the edge cannot resolve.
    let export = d.manifest.traces.get_mut(&rig.run).unwrap();
    export.lineage_prefixes = vec![Json::obj([
        ("run_id", Json::str(rig.run.clone())),
        ("up_to_seq", Json::Int(0)),
        ("head_hash", Json::str("sha256:forged")),
    ])];
    let r = walk(&d);
    assert_eq!(r.verdict, "failed");
    assert!(failure_codes(&r).contains(&closure::code::CYCLE.to_string()));
}

#[test]
fn fetch_refusals_are_typed_and_never_fabricated() {
    let rig = rig("t7");
    let mut d = bundle(&rig);
    let fbytes = b"remote doc".to_vec();
    let faddr = hharness_identity::address(&fbytes, "application/octet-stream").id();
    let mut fm = MemberRef::present("extra_doc", faddr.clone(), "application/octet-stream", 0);
    fm.status = MemberStatus::Fetch;
    d.manifest.members.push(fm);
    // Expired entry — `expires` enforced per location uniformly.
    d.manifest.fetch.push(FetchEntry {
        address: faddr.clone(),
        size: None,
        locations: vec![],
        expires: Some("2000-01-01T00:00:00Z".into()),
    });
    // Unauthorized remote location — no declared policy admits it.
    let fbytes2 = b"other remote doc".to_vec();
    let faddr2 = hharness_identity::address(&fbytes2, "application/octet-stream").id();
    let mut fm2 = MemberRef::present("extra_doc2", faddr2.clone(), "application/octet-stream", 0);
    fm2.status = MemberStatus::Fetch;
    d.manifest.members.push(fm2);
    d.manifest.fetch.push(FetchEntry {
        address: faddr2.clone(),
        size: None,
        locations: vec!["https://attacker.example/x".into()],
        expires: None,
    });
    // A wrong-bytes pool leg — verified arrival refuses the claim.
    let fbytes3 = b"third doc".to_vec();
    let faddr3 = hharness_identity::address(&fbytes3, "application/octet-stream").id();
    let mut fm3 = MemberRef::present("extra_doc3", faddr3.clone(), "application/octet-stream", 0);
    fm3.status = MemberStatus::Fetch;
    d.manifest.members.push(fm3);
    d.manifest.fetch.push(FetchEntry {
        address: faddr3.clone(),
        size: None,
        locations: vec![],
        expires: None,
    });
    let pool = |e: &FetchEntry| -> Result<Vec<u8>, String> {
        if e.address == faddr3 {
            Ok(b"not the bytes".to_vec())
        } else {
            Ok(fbytes.clone())
        }
    };
    let trust = FetchTrustPolicy {
        admission_classes: vec![],
        admitted_origins: vec![],
        accepted_signers: vec![],
        require_ledgered_arrival: true,
    };
    let r = closure_walk(
        &d,
        WalkOpts {
            procedure: "verify",
            recipient_class: "controlled",
            checked_at: TS,
            limits: ClosureLimits::default(),
            fetch: Some(FetchPermit {
                pool: Some(&pool),
                trust: Some(&trust),
                remote: None,
                now_ms: 1_700_000_000_000,
            }),
        },
    );
    assert_eq!(r.verdict, "failed");
    let codes = failure_codes(&r);
    assert!(
        codes.iter().any(|c| c.contains("location_expired")),
        "{codes:?}"
    );
    assert!(
        codes
            .iter()
            .any(|c| c.contains("policy_absent") || c.contains("not_admitted")),
        "{codes:?}"
    );
    assert!(
        codes.iter().any(|c| c.contains("digest_mismatch")),
        "{codes:?}"
    );
    // The walk takes `&Decoded` — no producer home/store/cache/
    // credential fallback exists in its signature; fetch is byte
    // materialization only — there is no execution path to execute.
}

#[test]
fn report_is_sidecar_and_never_raises_levels() {
    let rig = rig("t9");
    let d = bundle(&rig);
    let basis_before = d.manifest.reproducibility.clone();
    let r = walk(&d);
    assert_eq!(r.verdict, "closed");
    // The report carries no level basis join — a closed verdict
    // mutates nothing (sidecar-class, C.1 item 4).
    assert_eq!(d.manifest.reproducibility, basis_before);
    let j = r.to_json();
    assert!(j.get("max_supported_level").is_none());
    assert!(j.get("level").is_none());
    // The dedup'd edges the report counts are the same edges it lists.
    assert_eq!(r.required as usize, r.edges.len());
    assert_eq!(r.resolved + r.withheld + r.unresolved, r.required);
}
