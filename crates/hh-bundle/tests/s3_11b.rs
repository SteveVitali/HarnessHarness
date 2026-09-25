//! S3.11b — the §5g.5 §3 bundle-side extension executable
//! (AC-R-2.8.5-10): `resolved_dependencies.extensions[]` members deposit
//! their payload bytes content-addressably (the `content` pin *is* the
//! digest — a mismatch refuses `MemberMismatch`), an uncaptured payload
//! lands `unpinned{reason: not_captured}` rather than a silent ref, and
//! every member ref `section_member_refs` names resolves to a member.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs, ExtensionInput};
use hh_bundle::error::BundleError;
use hh_bundle::levels::section_member_refs;
use hh_bundle::manifest::{BundlePolicy, MemberStatus, Unpinned};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-bundle-s311b-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// The `TrustSnapshotEntry` member shape `{extension_id, content,
/// trust_record, attestation_refs[], surface_pin?}`.
fn ext_entry(id: &str, content: &str, surface_pin: Option<&str>) -> Json {
    let mut m = vec![
        ("extension_id", Json::str(id)),
        ("content", Json::str(content)),
        ("trust_record", Json::str(format!("sha256:trust-{id}"))),
        ("attestation_refs", Json::Arr(vec![])),
    ];
    if let Some(sp) = surface_pin {
        m.push(("surface_pin", Json::str(sp)));
    }
    Json::obj(m)
}

struct Fixture {
    store: Store,
    run_id: String,
    policy: BundlePolicy,
    /// `address → bytes` — the sealed-artifact pool the assembler reads.
    blobs: BTreeMap<String, Vec<u8>>,
}

/// Open a store, start a minimal agent run, and pin a sealed definition.
fn fixture(tag: &str) -> Fixture {
    let mut store = Store::open(dir(tag)).unwrap();
    let def = Json::obj([("sealed", Json::str("definition"))]);
    let def_bytes = def.to_canonical_string().into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut manifest = RunManifest::minimal(RunKind::Agent);
    manifest.harness_def_ref = Some(def_addr.clone());
    let (run_id, _lease) = store.open_run(manifest, "s311b.test").unwrap();
    let mut blobs = BTreeMap::new();
    blobs.insert(def_addr, def_bytes);
    Fixture {
        store,
        run_id,
        policy: BundlePolicy::default(),
        blobs,
    }
}

/// A `section_member_refs` entry is closed when it resolves to member
/// bytes, or is a trace tree coordinate whose pages all do (R-ID-3: the
/// tree coordinate's preimage is the page-address list, never a member).
fn ref_closed(a: &hh_bundle::assemble::Assembled, r: &str) -> bool {
    if a.members.contains_key(r) {
        return true;
    }
    a.manifest
        .traces
        .values()
        .any(|t| t.tree == r && t.pages.iter().all(|p| a.members.contains_key(p)))
}

fn inputs<'a>(
    f: &'a Fixture,
    extensions: Vec<ExtensionInput>,
    artifact_bytes: &'a dyn Fn(&str) -> Option<Vec<u8>>,
) -> AssembleInputs<'a> {
    AssembleInputs {
        store: &f.store,
        run_id: &f.run_id,
        policy: &f.policy,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("s311b.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions,
        budget: Json::Null,
        profile: Json::str("none"),
        nondeterminism: vec![],
        participant_class: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.5-10 — `extensions[]` propagate into `resolved_dependencies`,
// bytes deposit content-addressably, the manifest's member refs close.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_r_2_8_5_10_extensions_propagate_and_deposit() {
    let mut f = fixture("ext-ok");
    let payload = b"extension payload bytes".to_vec();
    let content_pin = hh_identity::address(&payload, "application/octet-stream").id();
    f.blobs.insert(content_pin.clone(), payload.clone());

    let entry = ext_entry("ext-a", &content_pin, None);
    let get = |a: &str| f.blobs.get(a).cloned();
    let inputs = inputs(
        &f,
        vec![ExtensionInput {
            entry: entry.clone(),
            bytes: Some(payload.clone()),
        }],
        &get,
    );
    let assembled = assemble(&inputs).unwrap();

    // (i) The entry propagates verbatim into `resolved_dependencies
    //     .extensions[]`.
    let Json::Arr(exts) = assembled
        .manifest
        .resolved_dependencies
        .get("extensions")
        .unwrap()
    else {
        panic!("extensions[] must be an array")
    };
    assert_eq!(exts.len(), 1);
    assert_eq!(exts[0], entry);

    // (ii) The payload is a `present` member keyed by the content pin.
    let member = assembled
        .manifest
        .members
        .iter()
        .find(|m| m.role == "extension:ext-a")
        .expect("extension member");
    assert_eq!(member.address, content_pin);
    assert_eq!(member.status, MemberStatus::Present);
    assert_eq!(assembled.members.get(&content_pin).unwrap(), &payload);

    // (iii) The pin is a member ref — S3 walks it — and every named ref
    //       resolves to member bytes (closure: nothing silently absent).
    let refs = section_member_refs(&assembled.manifest);
    assert!(refs.contains(&content_pin));
    for r in &refs {
        assert!(ref_closed(&assembled, r), "member ref {r} unclosed");
    }

    // (iv) No `not_captured` row for a captured extension.
    assert!(assembled
        .manifest
        .unpinned
        .iter()
        .all(|u| !u.role.starts_with("extension:")));
}

#[test]
fn ac_r_2_8_5_10_digest_mismatch_refuses_member_mismatch() {
    let f = fixture("ext-mismatch");
    // The declared pin is for *other* bytes — the deposit is the
    // verification, never a trust-the-caller copy.
    let declared = hh_identity::address(b"the real payload", "application/octet-stream").id();
    let entry = ext_entry("ext-b", &declared, None);
    let get = |a: &str| f.blobs.get(a).cloned();
    let inputs = inputs(
        &f,
        vec![ExtensionInput {
            entry,
            bytes: Some(b"substituted payload".to_vec()),
        }],
        &get,
    );
    assert!(matches!(
        assemble(&inputs),
        Err(BundleError::MemberMismatch { address }) if address == declared
    ));
}

#[test]
fn ac_r_2_8_5_10_uncaptured_payload_lands_not_captured_not_a_ref() {
    let f = fixture("ext-unpinned");
    // A declared pin whose bytes the caller could not capture.
    let content_pin = hh_identity::address(b"uncaptured", "application/octet-stream").id();
    let surface_pin = hh_identity::address(b"surface", "application/octet-stream").id();
    let entry = ext_entry("ext-c", &content_pin, Some(&surface_pin));
    let get = |a: &str| f.blobs.get(a).cloned();
    let inputs = inputs(&f, vec![ExtensionInput { entry, bytes: None }], &get);
    let assembled = assemble(&inputs).unwrap();

    // (i) The declaration lands `unpinned{not_captured}` with the pin as the
    //     claim — named, never silently absent (CC3). The surface pin the
    //     caller likewise could not supply lands the same way.
    let find = |role: &str| -> Option<&Unpinned> {
        assembled.manifest.unpinned.iter().find(|u| u.role == role)
    };
    let up = find("extension:ext-c").expect("content pin unpinned row");
    assert_eq!(up.reason, "not_captured");
    assert_eq!(
        up.claim.as_ref().and_then(Json::as_str),
        Some(content_pin.as_str())
    );
    let sp = find("extension:ext-c:surface").expect("surface pin unpinned row");
    assert_eq!(
        sp.claim.as_ref().and_then(Json::as_str),
        Some(surface_pin.as_str())
    );

    // (ii) No `present` member claims bytes that were never supplied.
    assert!(assembled
        .manifest
        .members
        .iter()
        .all(|m| !m.role.starts_with("extension:")));
    assert!(!assembled.members.contains_key(&content_pin));

    // (iii) The unpinned claim is *not* a member ref — S3 reads the
    //       declaration, never demands declared-absent bytes. And the
    //       remaining refs still close.
    let refs = section_member_refs(&assembled.manifest);
    assert!(!refs.contains(&content_pin));
    assert!(!refs.contains(&surface_pin));
    for r in &refs {
        assert!(ref_closed(&assembled, r), "member ref {r} unclosed");
    }

    // (iv) The entry itself still propagates — the bundle declares what it
    //      could not capture.
    let Json::Arr(exts) = assembled
        .manifest
        .resolved_dependencies
        .get("extensions")
        .unwrap()
    else {
        panic!("extensions[] must be an array")
    };
    assert_eq!(exts.len(), 1);
    assert_eq!(
        exts[0].get("content").and_then(Json::as_str),
        Some(content_pin.as_str())
    );
}

#[test]
fn ac_r_2_8_5_10_captured_and_uncaptured_mix_closes() {
    // A mixed set — one extension with bytes, one without — deposits the
    // captured member, names the uncaptured pin, and every member ref
    // still resolves.
    let mut f = fixture("ext-mix");
    let captured = b"captured payload".to_vec();
    let captured_pin = hh_identity::address(&captured, "application/octet-stream").id();
    f.blobs.insert(captured_pin.clone(), captured);
    let uncaptured_pin = hh_identity::address(b"uncaptured", "application/octet-stream").id();

    let get = |a: &str| f.blobs.get(a).cloned();
    let inputs = inputs(
        &f,
        vec![
            ExtensionInput {
                entry: ext_entry("ext-yes", &captured_pin, None),
                bytes: f.blobs.get(&captured_pin).cloned(),
            },
            ExtensionInput {
                entry: ext_entry("ext-no", &uncaptured_pin, None),
                bytes: None,
            },
        ],
        &get,
    );
    let assembled = assemble(&inputs).unwrap();

    let refs = section_member_refs(&assembled.manifest);
    assert!(refs.contains(&captured_pin));
    assert!(!refs.contains(&uncaptured_pin));
    for r in &refs {
        assert!(ref_closed(&assembled, r), "member ref {r} unclosed");
    }
    assert!(assembled
        .manifest
        .unpinned
        .iter()
        .any(|u| u.role == "extension:ext-no" && u.reason == "not_captured"));
}
