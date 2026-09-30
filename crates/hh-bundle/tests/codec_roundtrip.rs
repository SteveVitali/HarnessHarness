//! `hh-bundle/1` codec + validation round-trips (ticket S3.1;
//! AC-R-2.9.3-{1,2,13}): a hand-authored manifest + member set encodes
//! to a dir and a container, both decode to the *same* manifest bytes
//! (deterministic — the codec is a pure function of the records), and
//! `check_completeness` folds over the decoded form.

use std::path::{Path, PathBuf};

use hh_bundle::codec::{decode_container, decode_dir, encode_container, encode_dir};
use hh_bundle::export::MemberBytes;
use hh_bundle::manifest::BundleManifest;
use hh_bundle::validate::check_completeness;
use hh_wire::json::Json;

fn tmpdir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-test-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A minimal self-contained `run` bundle: every required section role
/// present as a member, one empty ledger export, `version_id`
/// recomputed over the preimage.
fn fixture(_dir: &Path) -> (BundleManifest, MemberBytes) {
    let member = |bytes: &[u8]| hh_identity::idp_id("blob", bytes);
    let doc = |v: Json| v.to_canonical_string().into_bytes();
    let mut members = MemberBytes::new();
    let mut refs = Vec::new();
    let mut put = |role: &str, bytes: Vec<u8>| {
        let addr = member(&bytes);
        members.insert(addr.clone(), bytes.clone());
        refs.push(Json::obj([
            ("role", Json::str(role)),
            ("ref", Json::str(addr)),
            ("media_type", Json::str("application/json")),
            ("size", Json::Int(bytes.len() as i64)),
            ("status", Json::str("present")),
        ]));
    };
    for role in [
        "subject",
        "definition",
        "environment",
        "model",
        "nondeterminism",
        "instrument",
        "configuration",
        "resolved_dependencies",
        "results",
    ] {
        // Distinct bytes per member — identical payloads would share a
        // content address and make the materialization tests ambiguous.
        put(role, doc(Json::obj([("role", Json::str(role))])));
    }
    // One empty ledger page + the tree document.
    let page = doc(Json::obj([("events", Json::Arr(vec![]))]));
    let page_addr = member(&page);
    members.insert(page_addr.clone(), page);
    refs.push(Json::obj([
        ("role", Json::str("ledger_page:run-t:0")),
        ("ref", Json::str(page_addr.clone())),
        (
            "media_type",
            Json::str("application/vnd.hh.ledger-page+json"),
        ),
        ("size", Json::Int(0)),
        ("status", Json::str("present")),
    ]));
    let tree_doc = Json::obj([
        ("run_id", Json::str("run-t")),
        ("pages", Json::Arr(vec![Json::str(page_addr.clone())])),
    ]);
    let ledger_tree =
        hh_bundle::manifest::LedgerExport::tree_address(std::slice::from_ref(&page_addr));
    let tree_bytes = doc(tree_doc);
    let tree_addr = member(&tree_bytes);
    members.insert(tree_addr.clone(), tree_bytes);
    refs.push(Json::obj([
        ("role", Json::str("ledger_tree:run-t")),
        ("ref", Json::str(tree_addr)),
        (
            "media_type",
            Json::str("application/vnd.hh.ledger-tree+json"),
        ),
        ("size", Json::Int(0)),
        ("status", Json::str("present")),
    ]));
    let manifest_doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("created_at", Json::str("2026-09-20T00:00:00.000Z")),
        (
            "producer",
            Json::obj([
                (
                    "origin",
                    Json::obj([
                        ("kind", Json::str("kernel")),
                        ("component_ref", Json::str("hh-bundle-test")),
                    ]),
                ),
                ("authority", Json::str("kernel")),
                ("scope", Json::str("run")),
                ("created_at", Json::Int(0)),
            ]),
        ),
        ("participant_class", Json::str("native")),
        ("observability_levels", Json::Arr(vec![])),
        ("claims", Json::Arr(vec![])),
        ("name_bindings", Json::Arr(vec![])),
        ("fetch_policy", Json::str("self_contained")),
        ("fetch", Json::Arr(vec![])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![Json::str("run-t")])),
                ("heads", Json::obj([])),
                ("lineage", Json::Arr(vec![])),
                ("watermarks", Json::obj([("run-t", Json::Int(0))])),
                ("status", Json::str("finished")),
            ]),
        ),
        ("definition", Json::obj([])),
        ("configuration", Json::obj([])),
        ("resolved_dependencies", Json::obj([])),
        ("model", Json::obj([("snapshots", Json::Arr(vec![]))])),
        ("instrument", Json::obj([])),
        (
            "traces",
            Json::obj([(
                "run-t",
                Json::obj([
                    ("run_id", Json::str("run-t")),
                    ("page_size", Json::Int(256)),
                    ("pages", Json::Arr(vec![Json::str(page_addr)])),
                    ("tree", Json::str(ledger_tree)),
                    ("checkpoints", Json::Arr(vec![])),
                    ("blob_index", Json::Arr(vec![])),
                    ("head", Json::obj([])),
                    ("lineage_prefixes", Json::Arr(vec![])),
                ]),
            )]),
        ),
        ("results", Json::obj([("rows", Json::Arr(vec![]))])),
        (
            "reproducibility",
            Json::obj([
                ("max_supported_level", Json::str("R0")),
                ("claimed_level", Json::str("R0")),
            ]),
        ),
        ("members", Json::Arr(refs)),
        ("unpinned", Json::Arr(vec![])),
        ("ext", Json::obj([])),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = BundleManifest::from_json(&manifest_doc).unwrap();
    manifest.version_id = manifest.compute_id();
    (manifest, members)
}

/// AC-R-2.9.3-1/2 — dir and container forms are deterministic and
/// mutually decodable: `decode(encode(m))` reproduces `m`'s canonical
/// bytes on both forms, and the two encoded forms agree.
#[test]
fn dir_and_container_forms_round_trip_identically() {
    let dir = tmpdir("roundtrip");
    let (manifest, members) = fixture(&dir);
    encode_dir(&dir, &manifest, &members).unwrap();
    let a = decode_dir(&dir).unwrap();
    let container = encode_container(&manifest, &members);
    let b = decode_container(&container).unwrap();
    // Same canonical manifest on both forms.
    assert_eq!(
        a.manifest.to_json().to_canonical_string(),
        b.manifest.to_json().to_canonical_string()
    );
    assert_eq!(a.manifest.version_id, b.manifest.version_id);
    // Member sets agree.
    let mut a_keys: Vec<&String> = a.members.keys().collect();
    let mut b_keys: Vec<&String> = b.members.keys().collect();
    a_keys.sort();
    b_keys.sort();
    assert_eq!(a_keys, b_keys);
    for k in a_keys {
        assert_eq!(a.members.get(k), b.members.get(k));
    }
    // Encoding is byte-deterministic — a second encode is identical.
    assert_eq!(container, encode_container(&manifest, &members));
}

/// AC-R-2.9.3-13 (completeness half) — a self-contained bundle passes
/// `check_completeness`; dropping a member fails S1/S2 with the typed
/// row, never a panic.
#[test]
fn completeness_folds_over_the_decoded_form() {
    let dir = tmpdir("complete");
    let (manifest, members) = fixture(&dir);
    encode_dir(&dir, &manifest, &members).unwrap();
    let d = decode_dir(&dir).unwrap();
    let report = check_completeness(&d.manifest, &d.members);
    assert!(
        report.complete,
        "{}",
        report.to_json().to_canonical_string()
    );

    // A missing member is a typed failure.
    let mut short = members.clone();
    let victim = short.keys().next().unwrap().clone();
    short.remove(&victim);
    let report = check_completeness(&manifest, &short);
    assert!(!report.complete);
}

/// The manifest's `version_id` is `idp/1` over its own preimage — a
/// tampered `version_id` fails the manifest-identity check at validate.
#[test]
fn version_id_recomputation_detects_tamper() {
    let dir = tmpdir("tamper");
    let (mut manifest, members) = fixture(&dir);
    encode_dir(&dir, &manifest, &members).unwrap();
    assert_eq!(manifest.version_id, manifest.compute_id());
    manifest.version_id = "sha256:deadbeef".into();
    encode_dir(&dir, &manifest, &members).unwrap();
    let d = decode_dir(&dir).unwrap();
    let report = check_completeness(&d.manifest, &d.members);
    assert!(
        !report.complete,
        "a forged version_id must fail: {}",
        report.to_json().to_canonical_string()
    );
}

/// AC-R-2.9.3-2 — materialization states are honest: a `fetch`-status
/// member with a `fetch[]` entry reports `unmaterialized` (the bundle
/// is complete-shaped but not valid until fetched — the spec's
/// `complete_not_valid`), while a `present`-claimed member without
/// bytes is `incomplete{missing_member}`.
#[test]
fn fetch_member_is_unmaterialized_not_failed_and_missing_is_incomplete() {
    let dir = tmpdir("fetch");
    let (manifest, members) = fixture(&dir);
    encode_dir(&dir, &manifest, &members).unwrap();
    let mut d = decode_dir(&dir).unwrap();

    // Flip the `subject` member to fetch-only and drop its bytes.
    let victim_addr = d.manifest.members[0].address.clone();
    d.manifest.members[0].status = hh_bundle::manifest::MemberStatus::Fetch;
    d.manifest.fetch.push(hh_bundle::manifest::FetchEntry {
        address: victim_addr.clone(),
        size: Some(d.manifest.members[0].size),
        locations: vec!["https://example.invalid/member".into()],
        expires: None,
    });
    d.members.remove(&victim_addr);
    d.manifest.version_id = d.manifest.compute_id();
    let report = check_completeness(&d.manifest, &d.members);
    // No stage *fails* on the fetch member — the unmaterialized row is
    // reported, and S3's fetch-consistency check is satisfied.
    assert!(
        report.complete,
        "fetch member must not fail completeness: {}",
        report.to_json().to_canonical_string()
    );
    let has_unmaterialized = report
        .to_json()
        .to_canonical_string()
        .contains("\"unmaterialized\"");
    assert!(has_unmaterialized, "fetch member must be reported");

    // A `present` claim without bytes is a hard `missing_member`.
    d.manifest.members[0].status = hh_bundle::manifest::MemberStatus::Present;
    d.manifest.fetch.clear();
    d.manifest.version_id = d.manifest.compute_id();
    let report = check_completeness(&d.manifest, &d.members);
    assert!(!report.complete, "{:?}", report.to_json());
    assert!(
        report
            .to_json()
            .to_canonical_string()
            .contains("missing_member"),
        "{}",
        report.to_json().to_canonical_string()
    );
}

/// AC-R-2.9.3-14 — a secret value in member bytes or a credentialed
/// `fetch[]` locator makes the bundle invalid (S8
/// `secret_or_credentialed_locator`); the stage is part of
/// `validate_bundle` (S1..S8), not the completeness subset.
#[test]
fn secret_material_and_credentialed_locators_are_invalid() {
    let dir = tmpdir("secret");
    let (manifest, mut members) = fixture(&dir);
    // Inject a credential-shaped payload as an extra member.
    let leaked = br#"{"token":"AKIAIOSFODNN7EXAMPLE"}"#.to_vec();
    let addr = hh_identity::idp_id("blob", &leaked);
    members.insert(addr.clone(), leaked.clone());
    let mut m = manifest.clone();
    m.members.push(hh_bundle::manifest::MemberRef {
        role: "results".into(),
        address: addr,
        media_type: "application/json".into(),
        size: leaked.len() as u64,
        status: hh_bundle::manifest::MemberStatus::Present,
        foreign: None,
    });
    m.version_id = m.compute_id();
    let report = hh_bundle::validate::validate(&m, &members);
    let doc = report.to_json().to_canonical_string();
    assert!(
        doc.contains("secret_or_credentialed_locator"),
        "secret member must fail S8: {doc}"
    );

    // A credentialed fetch locator is invalid the same way.
    let (manifest2, members2) = fixture(&dir);
    let mut m2 = manifest2.clone();
    m2.members[0].status = hh_bundle::manifest::MemberStatus::Fetch;
    m2.fetch.push(hh_bundle::manifest::FetchEntry {
        address: m2.members[0].address.clone(),
        size: None,
        locations: vec!["https://user:hunter2@example.invalid/blob".into()],
        expires: None,
    });
    m2.version_id = m2.compute_id();
    let report2 = hh_bundle::validate::validate(&m2, &members2);
    let doc2 = report2.to_json().to_canonical_string();
    assert!(
        doc2.contains("credentialed") || doc2.contains("secret_or_credentialed_locator"),
        "credentialed locator must fail: {doc2}"
    );
}
