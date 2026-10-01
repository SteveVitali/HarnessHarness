//! End-to-end — a real `EmbedService` serving binding (c) (`serve_net_n`
//! on this thread — the service is not `Send`, the sockets are) while a
//! real `hh-web` surface pipeline runs on a spawned thread against it.
//! Asserts the browser-facing path (gate → op → sink/scrub) lands on the
//! real kernel with byte parity to what binding (a)/`handle` would say.

mod common;

use std::net::TcpListener;

use common::*;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_web::server::{serve_request, Served};
use hh_wire::json::Json;

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-web-e2e-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// `view.v1_runs` → `run_index` against the real kernel — one
/// connection, an empty store, a real canonical result.
#[test]
fn e2e_run_index_over_binding_c() {
    let root = test_dir("ridx");
    let mut kernel = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "e2e".into(),
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let kaddr = listener.local_addr().unwrap();

    // The surface side: real gate + Sessions + serve_request on a
    // spawned thread (Send-safe). `run_index` is session-free → exactly
    // one kernel connection.
    let surface = std::thread::spawn(move || {
        let (gate, mut svc, det) = fixture(Some(kaddr));
        let s = serve_request(
            &gate,
            &mut svc,
            &det,
            &api_head(),
            &api_body("view.v1_runs", Json::Null),
        );
        match s {
            Served::Json(c, b) => (c, b),
            _ => panic!("expected json"),
        }
    });
    // hello + run_index — two connections (one per call, `Connection:
    // close`).
    let handled = hh_embed::net::serve_net_n(listener, &mut kernel, KERNEL_TOKEN, 2).unwrap();
    assert_eq!(handled, 2);
    let (code, body) = surface.join().unwrap();
    assert_eq!(code, 200, "{body}");
    let j = hh_wire::json::parse(&body).unwrap();
    assert_eq!(
        j.get("view").and_then(Json::as_str),
        Some("v1_runs"),
        "{body}"
    );
    assert!(
        j.get("entries").is_some() || j.get("runs").is_some(),
        "the real run_index result rides the view envelope: {body}"
    );
}

/// `view.v11_run` against the real kernel — exercises the session-bearing
/// path end-to-end (`hello` → `open_session{attach, client{kind:"web",
/// surface_ref, sink, ui_caps}}` → five session-scoped reads). The stub
/// kernels never strict-decode the client declaration; the real
/// `EmbedService` does — this is the coverage that keeps the declaration
/// schema honest.
#[test]
fn e2e_v11_run_over_binding_c() {
    let root = test_dir("v11");
    let store_root = root.join("store");
    let run_id = {
        let mut st = hh_ledger::store::Store::open(&store_root).unwrap();
        st.open_run(
            hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
            "holder-a",
        )
        .unwrap()
        .0
    };
    let mut kernel = EmbedService::open(ServiceConfig {
        store_root: store_root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "e2e".into(),
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let kaddr = listener.local_addr().unwrap();

    let rid = run_id.clone();
    let surface = std::thread::spawn(move || {
        let (gate, mut svc, det) = fixture(Some(kaddr));
        let s = serve_request(
            &gate,
            &mut svc,
            &det,
            &api_head(),
            &api_body(
                "view.v11_run",
                Json::obj([("run_id", Json::str(rid.clone()))]),
            ),
        );
        match s {
            Served::Json(c, b) => (c, b),
            _ => panic!("expected json"),
        }
    });
    // hello + open_session + project + account + head + describe +
    // list_leases — seven connections.
    let handled = hh_embed::net::serve_net_n(listener, &mut kernel, KERNEL_TOKEN, 7).unwrap();
    assert_eq!(handled, 7);
    let (code, body) = surface.join().unwrap();
    assert_eq!(code, 200, "{body}");
    let j = hh_wire::json::parse(&body).unwrap();
    assert_eq!(
        j.get("view").and_then(Json::as_str),
        Some("v11_run"),
        "{body}"
    );
    // The session record landed durable on the subject run, verbatim.
    let attached: Vec<String> = kernel
        .store()
        .events(&run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.session.attached")
        .map(|e| e.payload.to_canonical_string())
        .collect();
    assert_eq!(attached.len(), 1, "{attached:?}");
    assert!(attached[0].contains("\"kind\":\"web\""), "{attached:?}");
    assert!(attached[0].contains("\"ui_caps\""), "{attached:?}");
}

/// Gate refusals never reach the kernel — a foreign-Host POST against
/// the live socket still gets 400 and the kernel sees nothing.
#[test]
fn e2e_gate_refusal_never_dispatches() {
    let root = test_dir("gate");
    let mut kernel = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "e2e".into(),
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let kaddr = listener.local_addr().unwrap();
    // The surface refuses in-process — the kernel socket sees zero
    // connections (serve_net_n handles 0; the test drives the surface
    // directly, not through a socket).
    let surface = std::thread::spawn(move || {
        let (gate, mut svc, det) = fixture(Some(kaddr));
        let mut h = api_head();
        h.headers.insert("host".into(), "rebound.evil".into());
        match serve_request(&gate, &mut svc, &det, &h, &[]) {
            Served::Json(c, _) => c,
            _ => 0,
        }
    });
    let code = surface.join().unwrap();
    assert_eq!(code, 400);
    // A loopback listener that accepts zero connections — prove no
    // conn arrived by checking the kernel still answers `run_index`
    // (through a Sessions, which greets first) as its first traffic.
    let probe = std::thread::spawn(move || {
        let (gate, mut svc, _d) = fixture(Some(kaddr));
        let _ = gate;
        svc.call("run_index", Json::Obj(Default::default()))
            .unwrap()
    });
    let handled = hh_embed::net::serve_net_n(listener, &mut kernel, KERNEL_TOKEN, 2).unwrap();
    assert_eq!(handled, 2);
    let j = probe.join().unwrap();
    assert!(
        j.get("entries").is_some() || j.get("runs").is_some(),
        "run_index answers on the kernel's first traffic: {j:?}"
    );
}
