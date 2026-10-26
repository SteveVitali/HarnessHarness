//! R4.15d — the adopted `FetchTrustPolicy` MUST-data and the
//! `manifest.locations[]` admission/fetch fold (OQ-332; DF-S4.2-1; the
//! GATE-G7-adopted bl33 §trust-fields set).
//!
//! Hermetic cells per admission class — accept, refusal, expiry — plus
//! the `transport_pending`-era placeholder's removal (every refusal is a
//! typed arm), `expires` enforcement, cleartext `http:` refusal,
//! credentialed-locator refusal, the `bundle.member.fetched` payload per
//! remote attempt, and S3 digest re-verification on every
//! materialization. No wire fires here — the remote seam is a
//! `LocationFetcher` closure; the real TLS leg lives in
//! `hh-embed/tests/r4_15d.rs`.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::error::BundleError;
use hh_bundle::fetch;
use hh_bundle::manifest::{BundlePolicy, FetchEntry, MemberStatus};
use hh_bundle::trust::{
    admit, member_fetched_payload, AdmissionClass, FetchRefusal, FetchTrustPolicy, LocationLeg,
    FETCH_TRUST_SCHEMA,
};
use hh_bundle::Decoded;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-r415d-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A `BundleManifest` over one tiny member — the assembled run bundle
/// minus the members map (the fetch fold resolves through the seams).
struct Rig {
    manifest: hh_bundle::manifest::BundleManifest,
    member_addr: String,
    member_bytes: Vec<u8>,
}

fn rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, _l) = store.open_run(m, "r415d.test").unwrap();
    let policy = BundlePolicy::default();
    let get = |a: &str| (a == def_addr).then(|| def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &policy,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("r415d.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: &get,
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
    })
    .unwrap();
    let member = a.manifest.members[0].clone();
    let bytes = a
        .members
        .get(&member.address)
        .cloned()
        .expect("assembled member bytes");
    Rig {
        manifest: a.manifest,
        member_addr: member.address,
        member_bytes: bytes,
    }
}

/// `decoded(locations, expires)` — the rig manifest with `members[0]`
/// flipped to `status = fetch` and one `FetchEntry` (the member map
/// keeps no bytes for it — the seam supplies them).
fn decoded_with(rig: &Rig, locations: &[&str], expires: Option<&str>) -> Decoded {
    let mut manifest = rig.manifest.clone();
    manifest.members[0].status = MemberStatus::Fetch;
    manifest.fetch.push(FetchEntry {
        address: rig.member_addr.clone(),
        size: None,
        locations: locations.iter().map(|s| s.to_string()).collect(),
        expires: expires.map(String::from),
    });
    manifest.version_id = manifest.compute_id();
    let mut members = rig
        .manifest
        .members
        .iter()
        .map(|m| (m.address.clone(), Vec::new()))
        .collect::<std::collections::BTreeMap<_, _>>();
    members.clear();
    Decoded { manifest, members }
}

fn entry(rig: &Rig, locations: &[&str], expires: Option<&str>) -> FetchEntry {
    FetchEntry {
        address: rig.member_addr.clone(),
        size: None,
        locations: locations.iter().map(|s| s.to_string()).collect(),
        expires: expires.map(String::from),
    }
}

/// The fetch site's declared policy — all three classes, the one
/// allowlisted origin, ledgered arrival required (the adopted shape).
fn policy() -> FetchTrustPolicy {
    FetchTrustPolicy {
        admission_classes: vec![
            "first_party".into(),
            "signed_url".into(),
            "content_addressed".into(),
        ],
        admitted_origins: vec!["assets.example.com".into()],
        accepted_signers: vec!["hh/signer/test".into()],
        require_ledgered_arrival: true,
    }
}

// ── the MUST-data record ─────────────────────────────────────────────────

#[test]
fn trust_policy_record_roundtrips_and_version_pins() {
    let p = policy();
    let j = p.to_json();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some(FETCH_TRUST_SCHEMA)
    );
    assert_eq!(FetchTrustPolicy::from_json(&j).unwrap(), p);
    // The record carries a stable version id (the coordinate a bundle's
    // `resolved_dependencies` pins).
    assert!(p.version_id().starts_with("sha256:"));
    // Every member is required — a missing one is Malformed, never a
    // default.
    for key in [
        "admission_classes",
        "admitted_origins",
        "accepted_signers",
        "require_ledgered_arrival",
    ] {
        let mut doc = match j.clone() {
            Json::Obj(m) => m,
            _ => panic!(),
        };
        doc.remove(key);
        match FetchTrustPolicy::from_json(&Json::Obj(doc)) {
            Err(BundleError::Malformed { .. }) => {}
            other => panic!("missing {key} must refuse: {other:?}"),
        }
    }
    // An unknown admission class refuses — the sum is closed.
    let mut doc = match j {
        Json::Obj(m) => m,
        _ => panic!(),
    };
    doc.insert(
        "admission_classes".into(),
        Json::Arr(vec![Json::str("anything_goes")]),
    );
    match FetchTrustPolicy::from_json(&Json::Obj(doc)) {
        Err(BundleError::Malformed { .. }) => {}
        other => panic!("unknown class must refuse: {other:?}"),
    }
}

// ── admission: first_party ───────────────────────────────────────────────

#[test]
fn first_party_admits_allowlisted_https_and_refuses_offlist() {
    let rig = rig("fp");
    let p = policy();
    let e = entry(&rig, &["https://assets.example.com/member"], None);
    assert_eq!(
        admit(Some(&p), &e, &e.locations[0], 1_000),
        Ok(LocationLeg::HttpsGet {
            class: AdmissionClass::FirstParty,
            url: "https://assets.example.com/member".into(),
        })
    );
    // An `admitted_origins` `host:port` row matches the same port only.
    let p2 = FetchTrustPolicy {
        admitted_origins: vec!["assets.example.com:8443".into()],
        ..policy()
    };
    let e443 = entry(&rig, &["https://assets.example.com/m"], None);
    assert_eq!(
        admit(Some(&p2), &e443, &e443.locations[0], 1_000),
        Err(FetchRefusal::OriginNotAdmitted)
    );
    let e8443 = entry(&rig, &["https://assets.example.com:8443/m"], None);
    assert_eq!(
        admit(Some(&p2), &e8443, &e8443.locations[0], 1_000),
        Ok(LocationLeg::HttpsGet {
            class: AdmissionClass::FirstParty,
            url: "https://assets.example.com:8443/m".into(),
        })
    );
    // Off-list, unsigned → `origin_not_admitted` (the typed arm —
    // `transport_pending` never appears).
    let off = entry(&rig, &["https://elsewhere.example.net/m"], None);
    assert_eq!(
        admit(Some(&p), &off, &off.locations[0], 1_000),
        Err(FetchRefusal::OriginNotAdmitted)
    );
    // On the allowlist but the class undeclared → `class_not_admitted`.
    let p3 = FetchTrustPolicy {
        admission_classes: vec!["signed_url".into()],
        ..policy()
    };
    assert_eq!(
        admit(Some(&p3), &e, &e.locations[0], 1_000),
        Err(FetchRefusal::ClassNotAdmitted)
    );
}

#[test]
fn cleartext_http_is_never_admitted() {
    let rig = rig("http");
    let p = policy();
    let e = entry(&rig, &["http://assets.example.com/member"], None);
    assert_eq!(
        admit(Some(&p), &e, &e.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
    // Even an allowlisted origin refuses cleartext — the allowlist
    // covers `https:` origins only.
    let p2 = FetchTrustPolicy {
        admitted_origins: vec!["assets.example.com".into()],
        ..policy()
    };
    assert_eq!(
        admit(Some(&p2), &e, &e.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
    // Unknown schemes refuse the same way.
    let weird = entry(&rig, &["ftp://assets.example.com/m"], None);
    assert_eq!(
        admit(Some(&p2), &weird, &weird.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
}

// ── admission: signed_url ────────────────────────────────────────────────

#[test]
fn signed_url_admits_signature_and_placeholder_carriers() {
    let rig = rig("su");
    let p = policy();
    // A presigned carrier off the allowlist — the signature param
    // classifies it `signed_url`.
    let e = entry(
        &rig,
        &["https://objects.example.net/m?Expires=2000000000&Signature=mh_secret:sigslot:m"],
        None,
    );
    assert_eq!(
        admit(Some(&p), &e, &e.locations[0], 1_000),
        Ok(LocationLeg::HttpsGet {
            class: AdmissionClass::SignedUrl,
            url: e.locations[0].clone(),
        })
    );
    // An `mh_secret:` placeholder anywhere in the URI is a credential
    // carrier by construction — the `signed_url` class.
    let ph = entry(
        &rig,
        &["https://objects.example.net/m?auth=mh_secret:ch:x"],
        None,
    );
    assert_eq!(
        admit(Some(&p), &ph, &ph.locations[0], 1_000),
        Ok(LocationLeg::HttpsGet {
            class: AdmissionClass::SignedUrl,
            url: ph.locations[0].clone(),
        })
    );
    // The class undeclared → `class_not_admitted`.
    let p2 = FetchTrustPolicy {
        admission_classes: vec!["first_party".into()],
        ..policy()
    };
    assert_eq!(
        admit(Some(&p2), &e, &e.locations[0], 1_000),
        Err(FetchRefusal::ClassNotAdmitted)
    );
    // The carrier's own embedded expiry is enforced — `Expires=` in the
    // past refuses `location_expired` even with a live `entry.expires`.
    let stale = entry(
        &rig,
        &["https://objects.example.net/m?Expires=1&sig=mh_secret:a:b"],
        None,
    );
    assert_eq!(
        admit(Some(&p), &stale, &stale.locations[0], 1_000),
        Err(FetchRefusal::LocationExpired)
    );
}

#[test]
fn credentialed_locators_refuse_before_classification() {
    let rig = rig("cred");
    let p = policy();
    // A literal signature value — credential material in the URI —
    // refuses (the placeholder is the only safe carrier; R-ID-7).
    let lit = entry(&rig, &["https://objects.example.net/m?sig=abc123"], None);
    assert_eq!(
        admit(Some(&p), &lit, &lit.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
    // A userinfo authority refuses the same way.
    let ui = entry(&rig, &["https://user:pw@assets.example.com/m"], None);
    assert_eq!(
        admit(Some(&p), &ui, &ui.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
    // And the refusal precedes `policy_absent` — credential material is
    // refused even with no declared policy.
    assert_eq!(
        admit(None, &lit, &lit.locations[0], 1_000),
        Err(FetchRefusal::TransportRefused)
    );
}

// ── admission: content_addressed ─────────────────────────────────────────

#[test]
fn content_addressed_admits_then_refuses_transport_typed() {
    let rig = rig("ca");
    let p = policy();
    // `ipfs:` with and without the `//` authority spelling classifies.
    for loc in ["ipfs://bafybeigd/CID", "ipfs:bafybeigdCID"] {
        let e = entry(&rig, &[loc], None);
        assert_eq!(
            admit(Some(&p), &e, &e.locations[0], 1_000),
            Ok(LocationLeg::Unsupported {
                class: AdmissionClass::ContentAddressed,
                scheme: "ipfs".into(),
            }),
            "{loc}"
        );
    }
    // The class undeclared → `class_not_admitted`.
    let p2 = FetchTrustPolicy {
        admission_classes: vec!["first_party".into()],
        ..policy()
    };
    let e = entry(&rig, &["ipfs:cid"], None);
    assert_eq!(
        admit(Some(&p2), &e, &e.locations[0], 1_000),
        Err(FetchRefusal::ClassNotAdmitted)
    );
    // At the fold: the admitted class yields the typed
    // `transport_unsupported` refusal — honest, never a fabricated
    // fetch.
    let dec = decoded_with(&rig, &["ipfs:bafybeigdCID"], None);
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        None,
        Some(&p),
        None,
        1_000,
    );
    assert!(out.materialized.is_empty());
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("refused:transport_unsupported")
    );
    assert_eq!(out.records.len(), 1);
    assert_eq!(
        out.records[0].get("transport").and_then(Json::as_str),
        Some("ipfs")
    );
    assert_eq!(
        out.records[0].get("outcome").and_then(Json::as_str),
        Some("refused:transport_unsupported")
    );
}

// ── expiry + policy absence ──────────────────────────────────────────────

#[test]
fn expires_enforced_per_location_and_policy_absent_refuses() {
    let rig = rig("exp");
    let p = policy();
    // `entry.expires` in the past → `location_expired` for every scheme.
    for loc in [
        "https://assets.example.com/m",
        "ipfs:cid",
        "mem://pool",
        "pool:x",
    ] {
        let e = entry(&rig, &[loc], Some("2020-01-01T00:00:00Z"));
        assert_eq!(
            admit(Some(&p), &e, &e.locations[0], 1_800_000_000_000),
            Err(FetchRefusal::LocationExpired),
            "{loc}"
        );
    }
    // A live `expires` admits.
    let e = entry(
        &rig,
        &["https://assets.example.com/m"],
        Some("2200-01-01T00:00:00Z"),
    );
    assert!(matches!(
        admit(Some(&p), &e, &e.locations[0], 1_000),
        Ok(LocationLeg::HttpsGet { .. })
    ));
    // A remote location with no declared policy → `policy_absent`
    // (unmediated fetch is never attempted); pool legs need no policy.
    let e2 = entry(&rig, &["https://assets.example.com/m"], None);
    assert_eq!(
        admit(None, &e2, &e2.locations[0], 1_000),
        Err(FetchRefusal::PolicyAbsent)
    );
    let pool_leg = entry(&rig, &["mem://pool"], None);
    assert_eq!(
        admit(None, &pool_leg, &pool_leg.locations[0], 1_000),
        Ok(LocationLeg::Pool)
    );
    // A malformed `expires` refuses expired too (a timestamp that
    // doesn't parse is never read as "live").
    let bad = entry(&rig, &["https://assets.example.com/m"], Some("not-a-date"));
    assert_eq!(
        admit(Some(&p), &bad, &bad.locations[0], 1_000),
        Err(FetchRefusal::LocationExpired)
    );
}

// ── the fetch fold ───────────────────────────────────────────────────────

#[test]
fn fetch_materializes_remote_members_and_records_arrival() {
    let rig = rig("fold");
    let p = policy();
    let dec = decoded_with(&rig, &["https://assets.example.com/member"], None);
    let bytes = rig.member_bytes.clone();
    let mut remote = move |_url: &str| Ok(bytes.clone());
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        None,
        Some(&p),
        Some(&mut remote),
        1_000,
    );
    assert_eq!(out.materialized, vec![rig.member_addr.clone()]);
    assert!(out.failed.is_empty());
    // The arrival row — `{address, location_uri, fetched_at, transport,
    // outcome}` — is emitted for the fetch leg.
    assert_eq!(out.records.len(), 1);
    let rec = &out.records[0];
    assert_eq!(
        rec.get("address").and_then(Json::as_str),
        Some(rig.member_addr.as_str())
    );
    assert_eq!(
        rec.get("location_uri").and_then(Json::as_str),
        Some("https://assets.example.com/member")
    );
    assert_eq!(rec.get("transport").and_then(Json::as_str), Some("https"));
    assert_eq!(rec.get("outcome").and_then(Json::as_str), Some("verified"));
    assert!(rec.get("fetched_at").and_then(Json::as_str).is_some());
}

#[test]
fn fetch_reverifies_digest_on_every_materialization() {
    let rig = rig("rev");
    let p = policy();
    let dec = decoded_with(&rig, &["https://assets.example.com/member"], None);
    // Wrong bytes fail `digest_mismatch` — a location's claim is never
    // trusted (S3 on arrival; the fold recomputes every time).
    let mut remote = move |_url: &str| Ok(b"wrong-bytes".to_vec());
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        None,
        Some(&p),
        Some(&mut remote),
        1_000,
    );
    assert!(out.materialized.is_empty());
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("digest_mismatch")
    );
    assert_eq!(
        out.records[0].get("outcome").and_then(Json::as_str),
        Some("digest_mismatch")
    );
}

#[test]
fn fetch_records_every_typed_refusal_and_no_pending_arm_remains() {
    let rig = rig("ref");
    let p = policy();
    // A member whose locations all refuse — every attempt lands a
    // `bundle.member.fetched` payload with the typed arm; the member
    // fails with the last arm.
    let dec = decoded_with(
        &rig,
        &[
            "http://cleartext.example.com/m",
            "https://offlist.example.net/m",
            "https://assets.example.com/m?Expires=1&sig=mh_secret:a:b",
        ],
        Some("2200-01-01T00:00:00Z"),
    );
    let mut remote = |_: &str| -> Result<Vec<u8>, String> {
        unreachable!("no admitted location reaches the wire")
    };
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        None,
        Some(&p),
        Some(&mut remote),
        2_000_000_000_000u64,
    );
    assert!(out.materialized.is_empty());
    let reasons: Vec<String> = out
        .records
        .iter()
        .filter_map(|r| r.get("outcome").and_then(Json::as_str).map(String::from))
        .collect();
    assert_eq!(
        reasons,
        vec![
            "refused:transport_refused".to_string(),
            "refused:origin_not_admitted".to_string(),
            "refused:location_expired".to_string(),
        ]
    );
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("refused:location_expired")
    );
    // `transport_pending` never appears — the placeholder is gone.
    let doc = out.to_json().to_canonical_string();
    assert!(!doc.contains("transport_pending"), "{doc}");
}

#[test]
fn fetch_without_remote_fetcher_refuses_transport_unavailable() {
    let rig = rig("notrans");
    let p = policy();
    let dec = decoded_with(&rig, &["https://assets.example.com/member"], None);
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        None,
        Some(&p),
        None,
        1_000,
    );
    assert_eq!(
        out.failed[0].get("reason").and_then(Json::as_str),
        Some("refused:transport_unavailable")
    );
    assert_eq!(
        out.records[0].get("outcome").and_then(Json::as_str),
        Some("refused:transport_unavailable")
    );
}

#[test]
fn fetch_pool_legs_resolve_without_policy() {
    let rig = rig("pool");
    let dec = decoded_with(&rig, &["mem://pool"], None);
    let bytes = rig.member_bytes.clone();
    let pool = move |_e: &FetchEntry| Ok(bytes.clone());
    let out = fetch(
        &dec,
        std::slice::from_ref(&rig.member_addr),
        Some(&pool),
        None,
        None,
        1_000,
    );
    assert_eq!(out.materialized, vec![rig.member_addr.clone()]);
    assert!(out.failed.is_empty());
    // Pool legs emit no remote-arrival rows.
    assert!(out.records.is_empty());
}

#[test]
fn member_fetched_payload_carries_the_contract_members() {
    let rec = member_fetched_payload(
        "sha256:abc",
        "https://assets.example.com/m",
        "https",
        "verified",
        1_700_000_000_000,
    );
    for k in [
        "address",
        "location_uri",
        "fetched_at",
        "transport",
        "outcome",
    ] {
        assert!(rec.get(k).is_some(), "{k} missing: {rec:?}");
    }
    assert_eq!(
        rec.get("fetched_at").and_then(Json::as_str),
        Some("2023-11-14T22:13:20.000Z")
    );
}

#[test]
fn location_scheme_classification() {
    use hh_bundle::trust::location_scheme;
    assert_eq!(location_scheme("https://a/b"), Some("https".to_string()));
    assert_eq!(location_scheme("ipfs:cid"), Some("ipfs".to_string()));
    assert_eq!(location_scheme("pool:x"), Some("pool".to_string()));
    assert_eq!(location_scheme("mem://pool"), Some("mem".to_string()));
    // A bare content address is not a transport spelling.
    assert_eq!(location_scheme("sha256:abc123"), None);
    assert_eq!(location_scheme("member-bytes"), None);
}
