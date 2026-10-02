//! S3.11b — the §5g.5 Stage-3 trust-snapshot executable (AC-R-2.8.5-10):
//! `trust_snapshot` projects a sealed definition's `assembly.extensions.refs[]`
//! into the pinned `{extensions[], policy_version_id, registry_snapshot_id}`
//! record the bundle's `resolved_dependencies` carries — re-derivable, sorted,
//! refusing unresolvable and wrong-kind refs (never silently skipped, CC3).

use std::path::PathBuf;

use hh_identity::idp;
use hh_provenance::{Origin, ProvenanceRecord};
use hh_registry::errors::RegistryError;
use hh_registry::extension::{
    resolve_candidate, trust_snapshot, Candidate, DeclaredSource, ExtensionKind, FetchOutcome,
    SourceLocator,
};
use hh_registry::records::{ForeignImport, RegistryRecord};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-reg-s311b-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("s311b.test", 0)
}

/// Resolve + register a skill extension; returns its `version_id` pin.
fn register_extension(store: &mut RegistryStore, name: &str, payload: &[u8]) -> String {
    let candidate = Candidate {
        name: name.into(),
        kind: ExtensionKind::Skill,
        locator: SourceLocator {
            scheme: "registry".into(),
            credential_free_uri: format!("registry://local/{name}"),
            selector: Some("latest".into()),
            resolved: None,
            fetched_at: None,
        },
        source: DeclaredSource::Registry { name: name.into() },
    };
    let outcome = FetchOutcome {
        resolved: format!("resolved:{name}@1"),
        content: idp::address(payload, "application/octet-stream"),
        fetched_at: 7,
        code_identity: vec![],
        source_snapshot: None,
        surface_pin: None,
    };
    let rec = resolve_candidate(
        &candidate,
        &outcome,
        Origin::kernel("s311b.test"),
        Some(payload),
        7,
    );
    store
        .register(RegistryRecord::Extension(rec), &kernel(), None)
        .unwrap()
        .version_id
}

/// A sealed definition document carrying `assembly.extensions.refs[]`.
fn definition(extension_ids: &[&str]) -> Json {
    Json::obj([(
        "assembly",
        Json::obj([(
            "extensions",
            Json::obj([(
                "refs",
                Json::Arr(
                    extension_ids
                        .iter()
                        .map(|id| Json::obj([("extension_id", Json::str(*id))]))
                        .collect(),
                ),
            )]),
        )]),
    )])
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.5-10 — the trust snapshot: resolves every ref, sorts entries,
// carries the policy + registry snapshot ids, re-derives to the same id.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_r_2_8_5_10_trust_snapshot_projects_sorted_and_rederives() {
    let mut s = RegistryStore::open(dir("ts"), &kernel()).unwrap();
    let ext_a = register_extension(&mut s, "ext-a", b"payload-a");
    let ext_b = register_extension(&mut s, "ext-b", b"payload-b");
    let snap = s.snapshot(&kernel()).unwrap();

    // Refs declared in reverse order — the snapshot sorts canonically.
    let def = definition(&[&ext_b, &ext_a]);
    let ts = trust_snapshot(&s, &def, &snap).unwrap();

    assert_eq!(ts.extensions.len(), 2);
    assert!(
        ts.extensions[0].extension_id < ts.extensions[1].extension_id,
        "entries must be sorted by extension_id"
    );
    let ids: Vec<&str> = ts
        .extensions
        .iter()
        .map(|e| e.extension_id.as_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);

    // Each entry carries the record's content pin + trust coordinate.
    for e in &ts.extensions {
        assert!(e.content.starts_with("sha256:"), "content pin carried");
        assert!(
            !e.trust_record.is_empty(),
            "trust_record coordinate carried"
        );
    }
    assert_eq!(ts.policy_version_id, snap.policy_digest);
    assert_eq!(ts.registry_snapshot_id, snap.snapshot_id);

    // R1 — re-derivation: the same (definition, store state, snapshot)
    // inputs yield the same snapshot_id, whatever the ref order.
    let def_shuffled = definition(&[&ext_a, &ext_b]);
    let ts2 = trust_snapshot(&s, &def_shuffled, &snap).unwrap();
    assert_eq!(ts.snapshot_id(), ts2.snapshot_id());

    // The canonical document shape — `extensions[]` entries carry the
    // §5g.5 §3 members.
    let doc = ts.to_json();
    let Json::Arr(exts) = doc.get("extensions").unwrap() else {
        panic!("extensions must be an array")
    };
    let first = &exts[0];
    for k in [
        "extension_id",
        "content",
        "trust_record",
        "attestation_refs",
    ] {
        assert!(first.get(k).is_some(), "entry missing {k}");
    }
}

#[test]
fn ac_r_2_8_5_10_unresolvable_and_wrong_kind_refs_refuse() {
    let mut s = RegistryStore::open(dir("ts-bad"), &kernel()).unwrap();
    let ext = register_extension(&mut s, "ext-ok", b"payload");
    // A non-extension record to point a ref at (KindMismatch leg).
    let foreign = s
        .register(
            RegistryRecord::ForeignImport(ForeignImport {
                system: "x".into(),
                locator: "x".into(),
                digest: None,
                label: None,
                lifted_record: Json::Null,
                loss_report: hh_registry::records::LossReport::default(),
            }),
            &kernel(),
            None,
        )
        .unwrap();
    let snap = s.snapshot(&kernel()).unwrap();

    // (i) A ref with no `extension_id` — a selector surviving into a sealed
    // form → Unresolved (never silently skipped).
    let def = Json::obj([(
        "assembly",
        Json::obj([(
            "extensions",
            Json::obj([(
                "refs",
                Json::Arr(vec![Json::obj([("name", Json::str("ext-ok"))])]),
            )]),
        )]),
    )]);
    assert!(matches!(
        trust_snapshot(&s, &def, &snap),
        Err(RegistryError::Unresolved { .. })
    ));

    // (ii) A ref naming a version_id the store does not hold →
    // UnknownVersion.
    let def = definition(&["sha256:not-registered"]);
    assert!(matches!(
        trust_snapshot(&s, &def, &snap),
        Err(RegistryError::UnknownVersion { .. })
    ));

    // (iii) A ref naming a non-extension record → KindMismatch.
    let def = definition(&[&foreign.version_id]);
    assert!(matches!(
        trust_snapshot(&s, &def, &snap),
        Err(RegistryError::KindMismatch { .. })
    ));

    // (iv) Mixed good + bad — the bad member still refuses (no partial
    // snapshot, no silent drop).
    let def = definition(&[&ext, &foreign.version_id]);
    assert!(trust_snapshot(&s, &def, &snap).is_err());
}

#[test]
fn ac_r_2_8_5_10_empty_refs_projects_an_empty_snapshot() {
    // A definition with no `assembly.extensions.refs` (or none at all)
    // projects an empty-but-real snapshot — the `extensions[]` member is
    // present and empty, never absent.
    let mut s = RegistryStore::open(dir("ts-empty"), &kernel()).unwrap();
    let snap = s.snapshot(&kernel()).unwrap();
    let ts = trust_snapshot(&s, &Json::obj([]), &snap).unwrap();
    assert!(ts.extensions.is_empty());
    assert_eq!(ts.registry_snapshot_id, snap.snapshot_id);
    assert!(ts.to_json().get("extensions").is_some());
}
