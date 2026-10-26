//! R4.15d — the live `r4-15d-fetch` leg (ADR-0363; the R4.15d live
//! registry entry): ONE real `kernel.fetch` over the public Internet —
//! the committed Harbor `README.md` member at its pinned commit,
//! carried as a `manifest.locations[]` entry under the adopted
//! `FetchTrustPolicy` (`first_party` class, `raw.githubusercontent.com`
//! allowlisted, `require_ledgered_arrival`). The fetch rides the real
//! seam end to end: `admit` → `security.egress.{requested,decided}` on
//! the kernel fetch run → `hh_tls` webpki-roots verification → the one
//! `hh_wire::http` codec → content-address re-verify →
//! `bundle.member.fetched` durable before the bytes deposit. USD 0,
//! read-only, no secrets, no `tls_extra_roots_file` (the public chain
//! verifies against the Mozilla roots).
//!
//! `#[ignore]`d — `scripts/live-verify.sh r4-15d-fetch` drives it; an
//! unreachable network prints `SKIPPED-UNREACHABLE`, never a pass.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::manifest::{BundlePolicy, FetchEntry, MemberStatus};
use hh_bundle::trust::FetchTrustPolicy;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

/// The pinned live member — `harbor-framework/harbor` `README.md` at
/// commit `dd8815755a61e849ad0f146ae85eb4aaff4fe61f` (2990 bytes; the
/// committed fixture is the same object — the fetched bytes must
/// rehash to the member's content address).
const LIVE_URL: &str = "https://raw.githubusercontent.com/harbor-framework/harbor/\
dd8815755a61e849ad0f146ae85eb4aaff4fe61f/README.md";
const LIVE_ORIGIN: &str = "raw.githubusercontent.com";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r415d-live-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/r4_15d/harbor-README-dd88157.md"),
    )
    .unwrap()
}

fn member_addr() -> String {
    hh_identity::idp_id("blob", &fixture_bytes())
}

/// The bundle dir — `members[0]` is `status = fetch` at the member's
/// content address with one `locations[]` entry: the live URL.
fn fetch_bundle_dir(root: &Path) -> PathBuf {
    let mut store = Store::open(root.join("fixture-store")).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, _l) = store.open_run(m, "r415d.live").unwrap();
    let get = |a: &str| (a == def_addr).then(|| def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &BundlePolicy::default(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("r415d.live", 0).to_json(),
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
    let mut manifest = a.manifest.clone();
    manifest.members[0].status = MemberStatus::Fetch;
    manifest.members[0].address = member_addr();
    manifest.fetch.push(FetchEntry {
        address: member_addr(),
        size: Some(fixture_bytes().len() as u64),
        locations: vec![LIVE_URL.to_string()],
        expires: None,
    });
    manifest.version_id = manifest.compute_id();
    let mut members = a.members.clone();
    members.remove(&a.manifest.members[0].address);
    let dir = dir("bundle");
    hh_bundle::codec::encode_dir(&dir, &manifest, &members).unwrap();
    dir
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh r4-15d-fetch"]
fn r415d_live_member_fetch() {
    let root = dir("live");
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.15d-live".into(),
    })
    .unwrap();
    call(
        &mut svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r4.15d-live")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    let bundle = fetch_bundle_dir(&root);
    let trust = FetchTrustPolicy {
        admission_classes: vec![
            "first_party".into(),
            "signed_url".into(),
            "content_addressed".into(),
        ],
        admitted_origins: vec![LIVE_ORIGIN.into()],
        accepted_signers: vec![],
        require_ledgered_arrival: true,
    };
    let mut params = std::collections::BTreeMap::new();
    params.insert(
        "path".to_string(),
        Json::str(bundle.to_string_lossy().to_string()),
    );
    params.insert("trust".to_string(), trust.to_json());
    // No `tls_extra_roots_file` — the public chain verifies against the
    // Mozilla webpki roots, the exact live posture.
    let r = call(&mut svc, "kernel.fetch", Json::Obj(params));
    let outcome = r
        .get("result")
        .and_then(|x| x.get("outcome"))
        .cloned()
        .unwrap_or_else(|| panic!("kernel.fetch failed: {}", r.to_canonical_string()));
    let materialized: Vec<String> = match outcome.get("materialized") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => vec![],
    };
    if materialized.is_empty() {
        let reason = outcome
            .get("failed")
            .and_then(|f| match f {
                Json::Arr(a) => a.first().cloned(),
                _ => None,
            })
            .and_then(|f| f.get("reason").and_then(Json::as_str).map(String::from))
            .unwrap_or_default();
        // Network-unreachable is a skip, never a fabricated pass; a
        // typed refusal is a real failure.
        if reason.starts_with("fetch_error") {
            eprintln!("SKIPPED-UNREACHABLE: {reason}");
            return;
        }
        panic!("kernel.fetch refused/failed: {reason} — {outcome:?}");
    }
    assert_eq!(materialized, vec![member_addr()]);
    // The materialized member reads back from the blob pool under its
    // verified address — and equals the pinned fixture object.
    let parsed = hh_identity::idp::parse_id(&member_addr()).unwrap();
    let ca = hh_identity::idp::ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest: parsed.digest_hex,
        media_type: String::new(),
        size: 0,
    };
    assert_eq!(
        svc.store().get_blob(&ca).unwrap(),
        fixture_bytes(),
        "the live-fetched member is the pinned README object"
    );
    // The mediated trail: `requested` → `decided{allow}` →
    // `bundle.member.fetched{verified}` on the fetch run, in order.
    let run = svc
        .store()
        .run_ids()
        .into_iter()
        .find(|r| {
            svc.store()
                .manifest(r)
                .ok()
                .and_then(|m| m.extra.get("purpose").and_then(Json::as_str))
                == Some("fetch")
        })
        .expect("fetch run");
    let evs = svc.store().events(&run).unwrap();
    let pos = |class: &str| {
        evs.iter()
            .position(|e| e.class == class)
            .unwrap_or_else(|| panic!("{class} missing"))
    };
    assert!(
        pos("security.egress.requested") < pos("security.egress.decided")
            && pos("security.egress.decided") < pos("bundle.member.fetched")
    );
    let fetched = &evs[pos("bundle.member.fetched")].payload;
    assert_eq!(
        fetched.get("outcome").and_then(Json::as_str),
        Some("verified")
    );
    assert_eq!(
        fetched.get("location_uri").and_then(Json::as_str),
        Some(LIVE_URL)
    );
    println!(
        "LIVE_FETCH: {} bytes={} member={}",
        LIVE_URL,
        fixture_bytes().len(),
        member_addr()
    );
}
