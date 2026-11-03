//! R5.24 — Repair R0 lifecycle and prove clean-recipient verifyfixed
//! (REQ-F-007/A-204/C-003/D-208; §6.3 "Verify / optional R0" + §8.1
//! verifyfixed; A3 finding F6; the §9.3 admitted fix "R0 open/finished
//! mapping (R-2.9.3)").
//!
//! The old R0 fold read the finished row's `payload.status` — the
//! task-outcome plane — and compared it to `subject.status`
//! (`open`/`finished` by presence); every completed run drifted
//! `declared finished`/`derived succeeded`. The repair uses the
//! producer's presence rule; the battery covers the six R0 controls,
//! link/expansion refusals and the producer-cleanup verifyfixed leg.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_bundle::assemble::{assemble, AssembleInputs};
use hharness_bundle::codec::{decode_dir, encode_dir, MANIFEST_NAME, MEMBERS_DIR};
use hharness_bundle::export::decode_page;
use hharness_bundle::manifest::{BundleManifest, BundlePolicy};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_env::events::EventMinter;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r524-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Producer rig: a scratch `Store` holding one run the test controls —
/// `lifecycle.run.created` at open plus, when `outcome` is `Some`, a
/// `lifecycle.run.finished{status: outcome}` row (the outcome plane A3
/// F6 caught the old fold reading). `None` leaves the run `open`.
struct Producer {
    root: PathBuf,
    store: Store,
    run: String,
    #[allow(dead_code)]
    lease: Lease,
    def_addr: String,
    def_bytes: Vec<u8>,
}

fn producer(tag: &str, outcome: Option<&str>) -> Producer {
    let root = dir(tag);
    let mut store = Store::open(root.join("store")).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("r5.24"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = store.open_run(m, "r5.24").unwrap();
    if let Some(status) = outcome {
        let minter = EventMinter::new(&store, &run);
        // `run_finished_payload`'s members — `status` is the outcome
        // plane, not the open/finished lifecycle.
        let ev = minter
            .mint(
                "lifecycle.run.finished",
                Json::obj([
                    ("status", Json::str(status)),
                    ("stop_reason", Json::str("completed")),
                    ("outcome_class", Json::str("completed")),
                ]),
            )
            .unwrap();
        store.append(&run, &lease, vec![ev]).unwrap();
    }
    Producer {
        root,
        store,
        run,
        lease,
        def_addr,
        def_bytes,
    }
}

/// Assemble + deliver to `out` (`bundle.json` + `members/*`), outside
/// the producer root so producer cleanup cannot touch the handoff.
fn deliver(p: &Producer, out: &Path) -> PathBuf {
    let get = |a: &str| (a == p.def_addr).then(|| p.def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &p.store,
        run_id: &p.run,
        policy: &BundlePolicy::default(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("r5.24", 0).to_json(),
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
        evidence_profile: None,
    })
    .unwrap();
    encode_dir(out, &a.manifest, &a.members).unwrap();
    out.to_path_buf()
}

/// The clean recipient — a fresh `EmbedService` on its own scratch
/// store/workspace (§6.3: "empty recipient store/workspace has no
/// producer fallback").
fn recipient(tag: &str) -> EmbedService {
    let root = dir(tag);
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "recipient".into(),
    })
    .unwrap();
    let hello = call(
        &mut svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r5.24")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    assert!(hello.get("result").is_some(), "hello: {hello:?}");
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

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

/// The pinned argument file the procedure card hands a recipient —
/// `{path}` (+ `level` for reproduce) written to scratch, read back
/// and submitted verbatim (the shape `hh bundle validate|reproduce
/// <dir> [--level R0]` sends).
fn op(svc: &mut EmbedService, method: &str, params: Json, work: &Path) -> Json {
    let arg_path = work.join(format!("{method}-params.json"));
    std::fs::write(&arg_path, params.to_canonical_string()).unwrap();
    let read: Json =
        hharness_wire::json::parse(&std::fs::read_to_string(&arg_path).unwrap()).unwrap();
    call(svc, method, read)
}

fn reproduce(svc: &mut EmbedService, dir: &Path, work: &Path) -> Json {
    op(
        svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(dir.to_string_lossy().to_string())),
            ("level", Json::str("R0")),
        ]),
        work,
    )
}

fn validate(svc: &mut EmbedService, dir: &Path, work: &Path) -> Json {
    op(
        svc,
        "kernel.check_completeness",
        Json::obj([("path", Json::str(dir.to_string_lossy().to_string()))]),
        work,
    )
}

/// What the unfixed fold derived — recomputed over the bundle's own
/// bytes as the labelled diagnostic equivalent of the unavailable A3
/// original (§5.3 REQ-D-208): never an original rerun, never a verdict
/// input.
fn old_fold_derivation(dir: &Path) -> Option<String> {
    let decoded = decode_dir(dir).unwrap();
    let mut envelopes = Vec::new();
    for export in decoded.manifest.traces.values() {
        for page_addr in &export.pages {
            envelopes.extend(decode_page(decoded.members.get(page_addr).unwrap()).unwrap());
        }
    }
    envelopes
        .iter()
        .filter(|e| e.class.starts_with("lifecycle.run."))
        .filter_map(|e| {
            e.payload
                .get("status")
                .and_then(Json::as_str)
                .map(String::from)
                .or_else(|| (e.class == "lifecycle.run.finished").then(|| "finished".to_string()))
        })
        .next_back()
}

/// Digest over the delivered tree — proves verify writes nothing back
/// (old bytes unchanged).
fn tree_digest(dir: &Path) -> String {
    let mut acc = String::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let bytes = std::fs::read(&p).unwrap();
                acc.push_str(&format!(
                    "{}:{}\n",
                    p.strip_prefix(dir).unwrap().to_string_lossy(),
                    hharness_wire::sha256::sha256_hex(&bytes)
                ));
            }
        }
    }
    hharness_wire::sha256::sha256_hex(acc.as_bytes())
}

/// Rewrite `bundle.json` with `edit` applied (and, when `reseal`, a
/// recomputed `version_id` — the self-consistent re-seal case: identity
/// verifies; only the semantic fold can catch it).
fn edit_manifest(dir: &Path, reseal: bool, edit: impl Fn(&mut BundleManifest)) {
    let decoded = decode_dir(dir).unwrap();
    let mut m = decoded.manifest.clone();
    edit(&mut m);
    if reseal {
        m.version_id = m.compute_id();
    }
    std::fs::write(dir.join(MANIFEST_NAME), m.to_json().to_canonical_string()).unwrap();
}

fn first_member(dir: &Path) -> PathBuf {
    let mdir = dir.join(MEMBERS_DIR);
    std::fs::read_dir(&mdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_file())
        .expect("bundle carries members")
}

// ── AC-1 positive — the three lifecycle cases ────────────────────────────

#[test]
fn successful_finished_derives_finished() {
    let p = producer("succ", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    let mut svc = recipient("succ");
    let work = dir("succ-work");

    let v = ok(&validate(&mut svc, &bundle, &work));
    assert_eq!(v.get("complete"), Some(&Json::Bool(true)), "{v:?}");

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    // The manifest records the lifecycle, not the outcome.
    assert_eq!(
        old_fold_derivation(&bundle).as_deref(),
        Some("succeeded"),
        "fixture must carry the F6 signature — the outcome plane on the finished row"
    );
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("pass"),
        "succeeded task outcome ⇒ finished lifecycle ⇒ R0 pass: {r:?}"
    );
    assert_eq!(
        r.get("verdict").and_then(Json::as_str),
        Some("reproduced"),
        "{r:?}"
    );
}

#[test]
fn failed_finished_derives_finished() {
    let p = producer("fail", Some("failed_honest"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    let mut svc = recipient("fail");
    let work = dir("fail-work");

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("pass"),
        "task failure can validate — the lifecycle is still finished: {r:?}"
    );
}

#[test]
fn open_run_derives_open() {
    let p = producer("open", None);
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    let mut svc = recipient("open");
    let work = dir("open-work");

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("pass"),
        "open→open: {r:?}"
    );
}

// ── AC-2 negative — semantic verdicts fail even at exit zero ─────────────

#[test]
fn changed_bytes_fail_the_verdict_not_the_call() {
    let p = producer("tamper", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    // Tamper the manifest — identity no longer recomputes
    // (`manifest_version` drift) while the call still returns `result`.
    edit_manifest(&bundle, false, |m| {
        m.created_at = "2026-01-01T00:00:00.001Z".into();
    });
    let mut svc = recipient("tamper");
    let work = dir("tamper-work");

    let resp = reproduce(&mut svc, &bundle, &work);
    let r = ok(&resp); // the call succeeds — the VERDICT fails.
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("drift"),
        "changed bytes → drift: {r:?}"
    );
    assert_eq!(
        r.get("verdict").and_then(Json::as_str),
        Some("not_reproduced"),
        "{r:?}"
    );
    let drift = r.get("drift").and_then(|d| match d {
        Json::Arr(a) => Some(a.clone()),
        _ => None,
    });
    assert!(
        drift
            .unwrap_or_default()
            .iter()
            .any(|d| d.get("kind") == Some(&Json::str("manifest_version"))),
        "expected a manifest_version drift row: {r:?}"
    );

    let v = ok(&validate(&mut svc, &bundle, &work));
    assert_ne!(
        v.get("complete"),
        Some(&Json::Bool(true)),
        "a tampered bundle cannot validate complete: {v:?}"
    );
}

#[test]
fn absent_member_fails_the_verdict() {
    let p = producer("absent", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    // Drop a ledger-page member → `ledger_page_missing` drift.
    let page = {
        let decoded = decode_dir(&bundle).unwrap();
        let export = decoded.manifest.traces.values().next().unwrap();
        bundle
            .join(MEMBERS_DIR)
            .join(hharness_bundle::codec::member_filename(&export.pages[0]))
    };
    std::fs::remove_file(&page).unwrap();
    let mut svc = recipient("absent");
    let work = dir("absent-work");

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("drift"),
        "absent member → drift, not pass: {r:?}"
    );
    let drift = r.get("drift").and_then(|d| match d {
        Json::Arr(a) => Some(a.clone()),
        _ => None,
    });
    assert!(
        drift
            .unwrap_or_default()
            .iter()
            .any(|d| d.get("kind") == Some(&Json::str("ledger_page_missing"))),
        "expected a ledger_page_missing drift row: {r:?}"
    );
}

#[test]
fn resealed_wrong_lifecycle_fails() {
    let p = producer("reseal", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    // Self-consistent reseal: `subject.status` flipped + `version_id`
    // recomputed — identity verifies; only the fold catches it.
    edit_manifest(&bundle, true, |m| {
        m.subject.status = "open".into();
    });
    let mut svc = recipient("reseal");
    let work = dir("reseal-work");

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("drift"),
        "resealed wrong lifecycle → drift: {r:?}"
    );
    let drift = r.get("drift").and_then(|d| match d {
        Json::Arr(a) => Some(a.clone()),
        _ => None,
    });
    assert!(
        drift
            .unwrap_or_default()
            .iter()
            .any(|d| d.get("kind") == Some(&Json::str("lifecycle_status"))
                && d.get("declared") == Some(&Json::str("open"))
                && d.get("derived") == Some(&Json::str("finished"))),
        "expected lifecycle_status{{declared:open, derived:finished}}: {r:?}"
    );

    // The mirror — an open bundle resealed `finished` — fails the same
    // way (derived `open`).
    let p2 = producer("reseal2", None);
    let bundle2 = dir("bundle2");
    deliver(&p2, &bundle2);
    edit_manifest(&bundle2, true, |m| {
        m.subject.status = "finished".into();
    });
    let r2 = ok(&reproduce(&mut svc, &bundle2, &work));
    assert_eq!(
        r2.get("outcome").and_then(Json::as_str),
        Some("drift"),
        "open bundle resealed finished → drift: {r2:?}"
    );
}

#[cfg(unix)]
#[test]
fn link_escape_refuses() {
    let p = producer("escape", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    // A `members/` entry that is a symlink — an archive link escape —
    // refuses before its bytes are materialized.
    std::os::unix::fs::symlink(
        bundle.join(MANIFEST_NAME),
        bundle.join(MEMBERS_DIR).join("blob-deadbeef"),
    )
    .unwrap();
    let mut svc = recipient("escape");
    let work = dir("escape-work");

    let e = reproduce(&mut svc, &bundle, &work);
    assert_eq!(err_kind(&e), "Refused", "link escape → Refused: {e:?}");
    assert!(
        e.to_canonical_string().contains("member_escape"),
        "typed escape detail: {e:?}"
    );
    let e2 = validate(&mut svc, &bundle, &work);
    assert_eq!(err_kind(&e2), "Refused", "{e2:?}");
}

#[test]
fn member_expansion_refuses() {
    let p = producer("expand", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    // Grow a member past its declared `size` → expansion refuses
    // before the bytes are read.
    let member = first_member(&bundle);
    let mut bytes = std::fs::read(&member).unwrap();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&member, &bytes).unwrap();
    let mut svc = recipient("expand");
    let work = dir("expand-work");

    let e = reproduce(&mut svc, &bundle, &work);
    assert_eq!(
        err_kind(&e),
        "Refused",
        "expansion → Refused before materialization: {e:?}"
    );
    assert!(
        e.to_canonical_string().contains("member_expansion"),
        "typed expansion detail: {e:?}"
    );
}

// ── AC-1/AC-2 — the clean-recipient verifyfixed leg ──────────────────────

#[test]
fn clean_recipient_verifyfixed() {
    // §8.1 verifyfixed: pinned procedure, scratch evidence copy, the
    // original failure preserved, comparable candidate evidence.
    let p = producer("verifyfixed", Some("succeeded"));
    let bundle = dir("bundle");
    deliver(&p, &bundle);
    let before = tree_digest(&bundle);

    // Producer cleanup — the producer store is deleted before the
    // recipient runs; the dir alone is the handoff.
    let producer_root = p.root.clone();
    drop(p);
    std::fs::remove_dir_all(&producer_root).unwrap();

    let mut svc = recipient("verifyfixed");
    let work = dir("verifyfixed-work");

    let v = ok(&validate(&mut svc, &bundle, &work));
    assert_eq!(v.get("complete"), Some(&Json::Bool(true)), "{v:?}");
    assert_eq!(
        v.get("max_supported_level").and_then(Json::as_str),
        Some("R0"),
        "the fixture pins R0 like the A3 audit: {v:?}"
    );

    let r = ok(&reproduce(&mut svc, &bundle, &work));
    assert_eq!(
        r.get("outcome").and_then(Json::as_str),
        Some("pass"),
        "repaired fold over the same bytes: {r:?}"
    );
    assert_eq!(
        r.get("verdict").and_then(Json::as_str),
        Some("reproduced"),
        "{r:?}"
    );

    // Old bytes remain unchanged — verify wrote nothing back.
    assert_eq!(tree_digest(&bundle), before, "verify is read-only");

    // The original failure is preserved as a *labelled diagnostic
    // equivalent*: the finished row still carries `status: succeeded`
    // on the outcome plane — what the unfixed fold read and the A3
    // receipt recorded (`derived succeeded`) — while the delivered
    // bytes themselves are the candidate evidence, not an original
    // A3 rerun.
    assert_eq!(
        old_fold_derivation(&bundle).as_deref(),
        Some("succeeded"),
        "diagnostic equivalent of the preserved F6 signature"
    );
    let declared = decode_dir(&bundle).unwrap().manifest.subject.status;
    assert_eq!(declared, "finished");
}
