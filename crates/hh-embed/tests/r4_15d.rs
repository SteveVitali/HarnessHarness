//! R4.15d — `kernel.fetch`'s remote `locations[]` leg end to end over
//! the real `EmbedService` (OQ-332; DF-S4.2-1; the GATE-G7-adopted
//! §trust-fields set).
//!
//! The remote seam is the ticket's one path: the declared
//! `FetchTrustPolicy` (`trust` MUST-data) admits each location, the
//! `hh_env` `EgressMediator` mediates every admitted GET
//! (`security.egress.{requested,decided}` land durable-before-wire on the
//! kernel's fetch run), `hh_tls` terminates TLS with verification on
//! (the hermetic leg dials the committed test-CA fixture through
//! `tls_extra_roots_file` — verification stays on; a private issuer is
//! a declared root, never an insecure skip), and every remote attempt
//! appends `bundle.member.fetched` before the verified bytes deposit
//! (durable-before-visible, `require_ledgered_arrival`). Refusals are
//! typed — `transport_pending` never appears.
//!
//! The public-Internet arm is the ignored live cell
//! (`#[ignore]` + `scripts/live-verify.sh r4-15d-fetch`, ADR-0363).

#![allow(clippy::unwrap_used)]

use std::io::BufReader;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::manifest::{BundlePolicy, FetchEntry, MemberStatus};
use hh_bundle::trust::FetchTrustPolicy;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r415d-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service_in(root: &Path) -> EmbedService {
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.15d".into(),
    })
    .unwrap();
    let r = call(
        &mut svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r4.15d")),
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
    ok(&r);
    svc
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

fn pki(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../hh-tls/fixtures/pki/{name}"))
}

/// The committed fixture member — the real Harbor README at its pinned
/// commit (the live leg's `locations[]` member; the hermetic cells serve
/// the same bytes over the test CA).
fn fixture_bytes() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/r4_15d/harbor-README-dd88157.md"),
    )
    .unwrap()
}

fn fixture_addr() -> String {
    hh_identity::idp_id("blob", &fixture_bytes())
}

/// A bundle dir whose `members[0]` is `status = fetch` at `address =
/// idp(blob, fixture_bytes)` with one `FetchEntry` over `locations`.
/// The member's bytes stay off disk — the remote leg must supply them.
fn fetch_bundle_dir(root: &Path, locations: &[String], expires: Option<&str>) -> PathBuf {
    let mut store = Store::open(root.join("fixture-store")).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, _l) = store.open_run(m, "r415d.test").unwrap();
    let get = |a: &str| (a == def_addr).then(|| def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &BundlePolicy::default(),
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
    let mut manifest = a.manifest.clone();
    manifest.members[0].status = MemberStatus::Fetch;
    manifest.members[0].address = fixture_addr();
    manifest.fetch.push(FetchEntry {
        address: fixture_addr(),
        size: None,
        locations: locations.to_vec(),
        expires: expires.map(String::from),
    });
    manifest.version_id = manifest.compute_id();
    // Every member's bytes except the fetch member's — the fetch leg's
    // materialization is the only honest source.
    let mut members = a.members.clone();
    members.remove(&a.manifest.members[0].address);
    let dir = dir("bundle");
    hh_bundle::codec::encode_dir(&dir, &manifest, &members).unwrap();
    dir
}

/// The fetch site's declared policy — all three classes admitted, one
/// allowlisted origin, ledgered arrival required (the adopted shape).
fn trust(origins: &[&str]) -> Json {
    FetchTrustPolicy {
        admission_classes: vec![
            "first_party".into(),
            "signed_url".into(),
            "content_addressed".into(),
        ],
        admitted_origins: origins.iter().map(|s| s.to_string()).collect(),
        accepted_signers: vec![],
        require_ledgered_arrival: true,
    }
    .to_json()
}

fn fetch_call(svc: &mut EmbedService, bundle: &Path, params_extra: Vec<(String, Json)>) -> Json {
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "path".to_string(),
        Json::str(bundle.to_string_lossy().to_string()),
    );
    for (k, v) in params_extra {
        map.insert(k, v);
    }
    ok(&call(svc, "kernel.fetch", Json::Obj(map)))
}

fn failed_reason(outcome: &Json) -> String {
    outcome
        .get("failed")
        .and_then(|f| match f {
            Json::Arr(a) => a.first().cloned(),
            _ => None,
        })
        .and_then(|f| f.get("reason").and_then(Json::as_str).map(String::from))
        .unwrap_or_else(|| panic!("no failed row: {}", outcome.to_canonical_string()))
}

fn materialized(outcome: &Json) -> Vec<String> {
    match outcome.get("materialized") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

/// The kernel fetch run's envelopes — the run the service opened with
/// `extra.purpose = "fetch"`. `None` when no remote leg ran (a
/// policy-absent refusal never builds the wire machinery).
fn fetch_run_events(svc: &EmbedService) -> Option<Vec<(u64, String, Json)>> {
    let run = svc.store().run_ids().into_iter().find(|r| {
        svc.store()
            .manifest(r)
            .ok()
            .and_then(|m| m.extra.get("purpose").and_then(Json::as_str))
            == Some("fetch")
    })?;
    Some(
        svc.store()
            .events(&run)
            .unwrap()
            .iter()
            .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
            .collect(),
    )
}

// ── typed refusals through the boundary ──────────────────────────────────

#[test]
fn kernel_fetch_policy_absent_refuses_remote_leg() {
    let root = dir("absent");
    let mut svc = service_in(&root);
    let bundle = fetch_bundle_dir(&root, &["https://assets.example.com/m".into()], None);
    let r = fetch_call(&mut svc, &bundle, vec![]);
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:policy_absent"
    );
    // The refusal is still a ledgered attempt — `bundle.member.fetched`
    // lands on the fetch run even though no wire machinery ever built
    // (the policy's `transport` arm never fired).
    let evs = fetch_run_events(&svc).expect("fetch run");
    let fetched: Vec<&Json> = evs
        .iter()
        .filter(|(_, c, _)| c == "bundle.member.fetched")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(fetched.len(), 1);
    assert_eq!(
        fetched[0].get("outcome").and_then(Json::as_str),
        Some("refused:policy_absent")
    );
    // No mediated request was ever made.
    assert!(evs.iter().all(|(_, c, _)| c != "security.egress.requested"));
}

#[test]
fn kernel_fetch_cleartext_refused() {
    let root = dir("cleartext");
    let mut svc = service_in(&root);
    let bundle = fetch_bundle_dir(&root, &["http://assets.example.com/m".into()], None);
    let r = fetch_call(
        &mut svc,
        &bundle,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:transport_refused"
    );
    // The refusal is ledgered — `require_ledgered_arrival` covers every
    // attempt, admitted or refused.
    let evs = fetch_run_events(&svc).expect("fetch run");
    let fetched: Vec<&Json> = evs
        .iter()
        .filter(|(_, c, _)| c == "bundle.member.fetched")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(fetched.len(), 1);
    assert_eq!(
        fetched[0].get("outcome").and_then(Json::as_str),
        Some("refused:transport_refused")
    );
}

#[test]
fn kernel_fetch_expired_and_offlist_and_ipfs_refuse_typed() {
    let root = dir("refusals");
    let mut svc = service_in(&root);
    // Expired entry.
    let b1 = fetch_bundle_dir(
        &root.join("a"),
        &["https://assets.example.com/m".into()],
        Some("2020-01-01T00:00:00Z"),
    );
    let r = fetch_call(
        &mut svc,
        &b1,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:location_expired"
    );
    // Off-allowlist origin.
    let b2 = fetch_bundle_dir(
        &root.join("b"),
        &["https://elsewhere.example.net/m".into()],
        None,
    );
    let r = fetch_call(
        &mut svc,
        &b2,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:origin_not_admitted"
    );
    // `ipfs:` — an admitted class the GATE-G7 field set declares but the
    // build carries no transport for → honest `transport_unsupported`,
    // still ledgered.
    let b3 = fetch_bundle_dir(&root.join("c"), &["ipfs:bafybeigdCID".into()], None);
    let r = fetch_call(
        &mut svc,
        &b3,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:transport_unsupported"
    );
    // `credential_absent`-adjacent: a credentialed URI refuses before
    // classification even with the class admitted.
    let b4 = fetch_bundle_dir(
        &root.join("d"),
        &["https://assets.example.com/m?sig=literal-secret".into()],
        None,
    );
    let r = fetch_call(
        &mut svc,
        &b4,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    assert_eq!(
        failed_reason(r.get("outcome").unwrap()),
        "refused:transport_refused"
    );
}

// ── the hermetic TLS leg — the real mediated wire over the test CA ───────

/// One-shot TLS fixture: accept, drain the request, answer the body.
fn serve_once(body: Vec<u8>) -> (u16, std::thread::JoinHandle<()>) {
    let tls = hh_tls::ServerTls::new(
        &std::fs::read_to_string(pki("server.pem")).unwrap(),
        &std::fs::read_to_string(pki("server-key.pem")).unwrap(),
    )
    .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        if let Ok((sock, _)) = l.accept() {
            sock.set_read_timeout(Some(Duration::from_secs(15))).ok();
            sock.set_write_timeout(Some(Duration::from_secs(15))).ok();
            if let Ok(mut s) = tls.accept(sock) {
                let mut r = BufReader::new(&mut s);
                let _ = hh_wire::http::read_request(&mut r, 64 * 1024);
                let _ = hh_wire::http::write_response(&mut s, 200, &[], &body);
                let _ = s.flush();
            }
        }
    });
    (port, h)
}

use std::io::Write;

#[test]
fn kernel_fetch_materializes_over_tls_through_the_mediator() {
    let root = dir("tls-ok");
    let mut svc = service_in(&root);
    let (port, join) = serve_once(fixture_bytes());
    let origin = format!("127.0.0.1:{port}");
    let location = format!("https://{origin}/member");
    let bundle = fetch_bundle_dir(&root, std::slice::from_ref(&location), None);
    let r = fetch_call(
        &mut svc,
        &bundle,
        vec![
            ("trust".into(), trust(&[&origin])),
            (
                "tls_extra_roots_file".into(),
                Json::str(pki("hh-test-ca.pem").display().to_string()),
            ),
        ],
    );
    let outcome = r.get("outcome").unwrap();
    assert_eq!(materialized(outcome), vec![fixture_addr()]);
    join.join().unwrap();

    // The mediated trail on the kernel fetch run — `requested` →
    // `decided` → wire → `bundle.member.fetched` — all durable before
    // the bytes read present, in that order.
    let evs = fetch_run_events(&svc).expect("fetch run");
    let pos = |class: &str| {
        evs.iter()
            .find(|(_, c, _)| c == class)
            .map(|(s, _, _)| *s)
            .unwrap_or_else(|| panic!("{class} missing: {evs:?}"))
    };
    let (req_s, dec_s, fet_s) = (
        pos("security.egress.requested"),
        pos("security.egress.decided"),
        pos("bundle.member.fetched"),
    );
    assert!(req_s < dec_s && dec_s < fet_s, "{req_s}<{dec_s}<{fet_s}");
    let fetched = &evs
        .iter()
        .find(|(_, c, _)| c == "bundle.member.fetched")
        .unwrap()
        .2;
    assert_eq!(
        fetched.get("address").and_then(Json::as_str),
        Some(fixture_addr().as_str())
    );
    assert_eq!(
        fetched.get("location_uri").and_then(Json::as_str),
        Some(location.as_str())
    );
    assert_eq!(
        fetched.get("transport").and_then(Json::as_str),
        Some("https")
    );
    assert_eq!(
        fetched.get("outcome").and_then(Json::as_str),
        Some("verified")
    );
    // The egress decision is a policy-derived allow — the containment
    // floor the trust policy lowers to.
    let decided = &evs
        .iter()
        .find(|(_, c, _)| c == "security.egress.decided")
        .unwrap()
        .2;
    assert_eq!(
        decided.get("decision").and_then(Json::as_str),
        Some("allow")
    );

    // Durable-before-visible, the other direction: the deposit's bytes
    // are the verified fixture — the blob pool holds them under their
    // recomputed address (S3).
    let parsed = hh_identity::idp::parse_id(&fixture_addr()).unwrap();
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
        "the materialized member reads back byte-identical"
    );
}

#[test]
fn kernel_fetch_digest_mismatch_deposits_nothing() {
    let root = dir("mismatch");
    let mut svc = service_in(&root);
    let (port, join) = serve_once(b"not-the-member".to_vec());
    let origin = format!("127.0.0.1:{port}");
    let bundle = fetch_bundle_dir(&root, &[format!("https://{origin}/m")], None);
    let r = fetch_call(
        &mut svc,
        &bundle,
        vec![
            ("trust".into(), trust(&[&origin])),
            (
                "tls_extra_roots_file".into(),
                Json::str(pki("hh-test-ca.pem").display().to_string()),
            ),
        ],
    );
    let outcome = r.get("outcome").unwrap();
    assert!(materialized(outcome).is_empty());
    assert_eq!(failed_reason(outcome), "digest_mismatch");
    join.join().unwrap();
    // The mismatched attempt is ledgered; the blob never deposits.
    let parsed = hh_identity::idp::parse_id(&fixture_addr()).unwrap();
    let ca = hh_identity::idp::ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest: parsed.digest_hex,
        media_type: String::new(),
        size: 0,
    };
    assert!(svc.store().get_blob(&ca).is_err());
}

#[test]
fn kernel_fetch_fallback_refused_then_verified() {
    let root = dir("fallback");
    let mut svc = service_in(&root);
    let (port, join) = serve_once(fixture_bytes());
    let origin = format!("127.0.0.1:{port}");
    // First leg off the allowlist (refused), second on it (verified) —
    // the fold walks `locations[]` in order, ledgering each attempt.
    let bundle = fetch_bundle_dir(
        &root,
        &[
            "https://offlist.example.net/m".into(),
            format!("https://{origin}/m"),
        ],
        None,
    );
    let r = fetch_call(
        &mut svc,
        &bundle,
        vec![
            ("trust".into(), trust(&[&origin])),
            (
                "tls_extra_roots_file".into(),
                Json::str(pki("hh-test-ca.pem").display().to_string()),
            ),
        ],
    );
    let outcome = r.get("outcome").unwrap();
    assert_eq!(materialized(outcome), vec![fixture_addr()]);
    join.join().unwrap();
    let evs = fetch_run_events(&svc).expect("fetch run");
    let fetched: Vec<&Json> = evs
        .iter()
        .filter(|(_, c, _)| c == "bundle.member.fetched")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(fetched.len(), 2);
    assert_eq!(
        fetched[0].get("outcome").and_then(Json::as_str),
        Some("refused:origin_not_admitted")
    );
    assert_eq!(
        fetched[1].get("outcome").and_then(Json::as_str),
        Some("verified")
    );
}

#[test]
fn kernel_fetch_no_pending_arm_appears() {
    // The `transport_pending` placeholder is gone end-to-end: every
    // refusal the boundary can produce is a typed arm on the outcome's
    // rows. Sweep the refusal table across the outcome document.
    let root = dir("nopending");
    let mut svc = service_in(&root);
    let bundle = fetch_bundle_dir(&root, &["ipfs:cid".into()], None);
    let r = fetch_call(
        &mut svc,
        &bundle,
        vec![("trust".into(), trust(&["assets.example.com"]))],
    );
    let doc = r.to_canonical_string();
    assert!(!doc.contains("transport_pending"), "{doc}");
}
