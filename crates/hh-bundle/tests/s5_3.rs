//! S5.3 — the hosted bundle export slice (§6.6 §5; R-2.10.6 C2):
//! a hosted bundle names its lifting machinery — `adapter_version` and
//! `descriptor_version` claims — and the `lifecycle.hosted.native_record`
//! leaves ride as a first-class `hosted_native_records` member, so a
//! withholding policy (the public `PublicationPolicy`) lists them in
//! the loss report rather than silently dropping them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::codec::Decoded;
use hh_bundle::export::{export_target, PublicationPolicy};
use hh_bundle::manifest::BundlePolicy;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-s53-{}-{tag}-{}",
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
    _lease: Lease,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

/// A hosted run: `attached` (descriptor coordinate), the
/// `component.bound{hosting_adapter}` audit row (the adapter version),
/// and two `native_record` leaves (the unlifted raw rows).
fn hosted_rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.participant_class = ParticipantClass::Hosted;
    m.hosting_mechanism = Some("model_boundary_intercept".into());
    let (run, lease) = store.open_run(m, "s53.test").unwrap();
    store
        .append(
            &run,
            &lease,
            vec![
                ev(
                    "attach-1",
                    "lifecycle.hosted.attached",
                    Json::obj([
                        ("session_ref", Json::str("session:hosted-1")),
                        (
                            "participant_version_identity",
                            Json::str("idp:participant:acme-v9"),
                        ),
                        ("participant_ref", Json::str("idp:participant:acme")),
                        ("hosting_mechanism", Json::str("model_boundary_intercept")),
                        ("abi_version", Json::str("hh-hosting/1")),
                        ("mediation", Json::str("hooks+proxy")),
                    ]),
                ),
                ev(
                    "bound-1",
                    "lifecycle.component.bound",
                    Json::obj([
                        ("class_id", Json::str("hosting_adapter")),
                        ("component_ref", Json::str("adapter-b/1")),
                        ("session_ref", Json::str("session:hosted-1")),
                    ]),
                ),
                ev(
                    "nat-1",
                    "lifecycle.hosted.native_record",
                    Json::obj([("raw", Json::str("acme-log-line-1"))]),
                ),
                ev(
                    "nat-2",
                    "lifecycle.hosted.native_record",
                    Json::obj([("raw", Json::str("acme-log-line-2"))]),
                ),
            ],
        )
        .unwrap();
    Rig {
        store,
        run,
        _lease: lease,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn inputs<'a>(rig: &'a Rig) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("s53.test", 0).to_json(),
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
        nondeterminism: vec![],
        participant_class: None,
    }
}

#[test]
fn hosted_bundle_names_adapter_descriptor_and_native_records() {
    let rig = hosted_rig("hosted");
    let a = assemble(&inputs(&rig)).unwrap();
    assert_eq!(a.manifest.participant_class, "hosted");

    // `adapter_version` — the bound `hosting_adapter` component ref.
    let adapter = a
        .manifest
        .claims
        .iter()
        .find(|c| c.role == "adapter_version")
        .expect("adapter_version claim on a hosted bundle");
    assert_eq!(
        adapter.value.get("adapter_version_id"),
        Some(&Json::str("adapter-b/1"))
    );
    assert_eq!(
        adapter.provenance.get("authority").and_then(Json::as_str),
        Some("environment")
    );

    // `descriptor_version` — the negotiated descriptor coordinate off
    // the attach row (`abi_version`, `mediation`, mechanism).
    let desc = a
        .manifest
        .claims
        .iter()
        .find(|c| c.role == "descriptor_version")
        .expect("descriptor_version claim");
    assert_eq!(
        desc.value.get("abi_version"),
        Some(&Json::str("hh-hosting/1"))
    );
    assert_eq!(
        desc.value.get("hosting_mechanism"),
        Some(&Json::str("model_boundary_intercept"))
    );

    // `hosted_native_records` — the raw leaves as a first-class member.
    let member = a
        .manifest
        .members
        .iter()
        .find(|m| m.role == "hosted_native_records")
        .expect("hosted_native_records member");
    let bytes = a
        .members
        .get(&member.address)
        .expect("the member's bytes ship");
    let doc = hh_wire::json::parse(&String::from_utf8(bytes.clone()).unwrap()).unwrap();
    match doc.get("records") {
        Some(Json::Arr(recs)) => {
            assert_eq!(recs.len(), 2);
            assert!(recs.iter().all(|r| r.get("event_id").is_some()
                && r.get("seq").is_some()
                && r.get("payload").is_some()));
        }
        other => panic!("records[]: {other:?}"),
    }
}

#[test]
fn public_policy_withholds_native_records_into_the_loss_report() {
    // `PublicationPolicy::public()` — the C2 per-suite default: readers
    // = `public`, `restricted_store` payload protection, the `content`
    // class withheld. The `hosted_native_records` member is `content`
    // class → withheld → `redacted` status + a loss row (never a silent
    // drop).
    let rig = hosted_rig("public");
    let a = assemble(&inputs(&rig)).unwrap();
    let policy = PublicationPolicy::public();
    assert_eq!(policy.readers, vec!["public".to_string()]);
    assert_eq!(policy.payload_protection, "restricted_store");
    assert!(!policy.admits("content"));
    assert!(policy.admits("accounting"));

    let decoded = Decoded {
        manifest: a.manifest.clone(),
        members: a.members.clone(),
    };
    let out = export_target(&decoded, "ledger_native", &policy).unwrap();
    // The member flipped `redacted` on the export copy and a loss row
    // names it.
    let m = out
        .manifest
        .members
        .iter()
        .find(|m| m.role == "hosted_native_records")
        .expect("member row persists (status flipped)");
    assert_eq!(
        format!("{:?}", m.status).to_lowercase(),
        "redacted",
        "{m:?}"
    );
    let entries: Vec<Json> = match out.loss_report.get("entries") {
        Some(Json::Arr(v)) => v.clone(),
        _ => vec![],
    };
    assert!(
        entries
            .iter()
            .any(|e| { e.get("member").and_then(Json::as_str) == Some("hosted_native_records") }),
        "the withheld member lands in the loss report: {entries:?}"
    );
    // The source manifest is untouched (I1).
    assert_ne!(
        a.manifest
            .members
            .iter()
            .find(|m| m.role == "hosted_native_records")
            .map(|m| format!("{:?}", m.status).to_lowercase())
            .as_deref(),
        Some("redacted")
    );
}
