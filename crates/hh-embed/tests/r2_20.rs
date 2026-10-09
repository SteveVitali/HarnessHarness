//! R2.20 boundary battery — the `lab.extension.{scan, seal, activate}` ops
//! plus the `payload`/`manifest`/`events` members on `resolve`/`install`
//! (DF-S1.23-2, BL-28; spec §5g.5). The lifecycle semantics live in
//! `hh-registry/tests/r2_20.rs`; here the JSON plumbing is the verdict —
//! every op routes, decodes, and returns the minted event payloads.

use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

static N: AtomicU64 = AtomicU64::new(0);

fn test_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r2-20-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r2_20".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

fn lab_hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r2_20-test")),
                    ("version", Json::str("0.0.0")),
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
    assert!(r.get("result").is_some(), "hello refused: {r:?}");
}

fn registrar() -> Json {
    ProvenanceRecord::kernel("hh-embed", 0).to_json()
}

fn candidate(name: &str) -> Json {
    Json::obj([
        ("name", Json::str(name)),
        ("kind", Json::str("skill")),
        ("uri", Json::str("https://git.example/x.git")),
        (
            "source",
            Json::obj([
                ("kind", Json::str("git")),
                ("url", Json::str("https://git.example/x.git")),
                ("ref", Json::str("main")),
            ]),
        ),
    ])
}

fn outcome(payload: &[u8]) -> Json {
    let pin = hh_identity::idp::address(payload, "application/octet-stream");
    Json::obj([
        ("resolved", Json::str("git:x@abc")),
        ("content", Json::str(pin.id())),
        ("fetched_at", Json::Int(7)),
    ])
}

fn scan_params(entries: Json) -> Json {
    Json::obj([(
        "scans",
        Json::Arr(vec![Json::obj([
            (
                "source",
                Json::obj([
                    ("kind", Json::str("directory_scan")),
                    ("root", Json::str("vendor/skills")),
                    ("scope", Json::str("run")),
                ]),
            ),
            ("entries", entries),
        ])]),
    )])
}

/// `lab.extension.scan` routes and folds a declared directory listing into
/// `refs[]` — locator scheme + joined URI visible over the wire.
#[test]
fn scan_enumerates_declared_sources_over_the_boundary() {
    let mut svc = service();
    lab_hello(&mut svc);
    let r = ok(&call(
        &mut svc,
        "lab.extension.scan",
        scan_params(Json::Arr(vec![Json::obj([
            ("name", Json::str("lint")),
            ("kind", Json::str("skill")),
            ("member", Json::obj([("path", Json::str("lint"))])),
        ])])),
    ));
    let refs = match r.get("refs") {
        Some(Json::Arr(rs)) => rs,
        other => panic!("refs missing: {other:?}"),
    };
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0]
            .get("locator")
            .and_then(|l| l.get("scheme"))
            .and_then(Json::as_str),
        Some("directory_scan")
    );
    assert_eq!(
        refs[0]
            .get("locator")
            .and_then(|l| l.get("credential_free_uri"))
            .and_then(Json::as_str),
        Some("vendor/skills/lint")
    );

    // A `..` escape is the typed `ScanDenied` refusal, not a ref.
    let r = call(
        &mut svc,
        "lab.extension.scan",
        scan_params(Json::Arr(vec![Json::obj([
            ("name", Json::str("evil")),
            ("kind", Json::str("skill")),
            ("member", Json::obj([("path", Json::str("../escape"))])),
        ])])),
    );
    assert_eq!(err_kind(&r), "Refused");
    assert!(
        r.to_canonical_string().contains("ScanDenied"),
        "the typed refusal rides the reason: {r:?}"
    );
}

/// `lab.extension.resolve` accepts `payload` + `manifest` and returns the
/// minted `events[]` — `security.extension.resolved` is the first row; the
/// manifest claims lift into the registered record's `declared_claims`.
#[test]
fn resolve_mints_the_resolved_event_over_the_boundary() {
    let mut svc = service();
    lab_hello(&mut svc);
    let r = ok(&call(
        &mut svc,
        "lab.extension.resolve",
        Json::obj([
            ("candidate", candidate("demo")),
            ("outcome", outcome(b"clean payload")),
            ("payload", Json::str("clean payload")),
            (
                "manifest",
                Json::obj([("allowed_tools", Json::Arr(vec![Json::str("fs.read:*")]))]),
            ),
            ("registrar", registrar()),
        ]),
    ));
    let events = match r.get("events") {
        Some(Json::Arr(es)) => es,
        other => panic!("events missing: {other:?}"),
    };
    assert_eq!(
        events[0].get("class").and_then(Json::as_str),
        Some("security.extension.resolved")
    );
    let vid = r
        .get("version_id")
        .and_then(Json::as_str)
        .expect("version_id")
        .to_string();

    // seal: resolved → sealed over the boundary; the event rides the result.
    let r = ok(&call(
        &mut svc,
        "lab.extension.seal",
        Json::obj([("version_id", Json::str(vid.clone()))]),
    ));
    assert_eq!(
        r.get("sealed_event")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("security.extension.sealed")
    );
    let sealed_vid = r
        .get("version_id")
        .and_then(Json::as_str)
        .expect("sealed version_id")
        .to_string();

    // activate: the sealed record loads — `security.extension.loaded` with
    // the pin attestation members.
    let r = ok(&call(
        &mut svc,
        "lab.extension.activate",
        Json::obj([("version_id", Json::str(sealed_vid.clone()))]),
    ));
    let loaded = r.get("loaded_event").expect("loaded_event member");
    assert_eq!(
        loaded.get("class").and_then(Json::as_str),
        Some("security.extension.loaded")
    );
    assert_eq!(
        loaded.get("pin_check").and_then(Json::as_str),
        Some("pinned")
    );
    assert!(
        loaded.get("content").and_then(Json::as_str).is_some(),
        "the pin attestation rides the loaded row"
    );

    // A second seal on the sealed record refuses — status ≠ resolved.
    let r = call(
        &mut svc,
        "lab.extension.seal",
        Json::obj([("version_id", Json::str(sealed_vid))]),
    );
    assert_eq!(err_kind(&r), "Refused");
    assert!(r.to_canonical_string().contains("resolved"));
}

/// A grant-shaped claim member refuses `LegCrossing` at the boundary —
/// the typed detail survives `reg_err`.
#[test]
fn grant_shaped_manifest_claim_refuses_leg_crossing() {
    let mut svc = service();
    lab_hello(&mut svc);
    let r = call(
        &mut svc,
        "lab.extension.resolve",
        Json::obj([
            ("candidate", candidate("evil")),
            ("outcome", outcome(b"pkg")),
            (
                "manifest",
                Json::obj([("allowed_tools", Json::obj([("grant", Json::str("perm:x"))]))]),
            ),
            ("registrar", registrar()),
        ]),
    );
    assert_eq!(err_kind(&r), "Refused");
    assert!(
        r.to_canonical_string().contains("LegCrossing"),
        "the LegCrossing detail surfaces: {r:?}"
    );
}
