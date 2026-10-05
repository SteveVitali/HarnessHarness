//! S4.10 — binding (c) `local_network` conformance (§7.2; ADR-0301 D1).
//!
//! The kernel-side WS-H3/K2 posture over the real `EmbedService` +
//! [`serve_net_n`] (the service is not `Send` — it stays on the test
//! thread; clients are plain sockets on spawned threads):
//!
//! - loopback refusal (P5), bearer required even on loopback (P2),
//!   foreign `Host` → 400 (P3), any `Origin`/`Sec-Fetch-*` → 403 (P3/P4),
//!   `?token=` → 400 (P9), `GET /hh-embed/1/events` drains NDJSON,
//!   `POST /hh-embed/1` is byte-parity with `svc.handle` (binding (a)).
#![allow(clippy::unwrap_used)]

use std::io::BufReader;
use std::net::{SocketAddr, TcpListener, TcpStream};

use hh_embed::net::{rpc_once, serve_net_n};
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::http::read_response;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-net-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn svc(tag: &str) -> EmbedService {
    let root = dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("s"),
        kernel_version_id: "k/0.1".into(),
        workspace_root: root.join("w"),
        holder: "net".into(),
    })
    .unwrap()
}

const TOK: &str = "binding-c-token-0123456789abcdef";

/// One raw request against the (c) listener (spawned — the server runs
/// on the test thread).
fn raw_request(addr: SocketAddr, request: String) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    use std::io::Write;
    s.write_all(request.as_bytes()).unwrap();
    s.flush().unwrap();
    let mut r = BufReader::new(s.try_clone().unwrap());
    let (h, b) = read_response(&mut r, 1 << 20).unwrap();
    (h.status, String::from_utf8_lossy(&b).to_string())
}

/// P5 — a non-loopback listener refuses before serving.
#[test]
fn net_refuses_non_loopback() {
    // Bind 0.0.0.0 — the socket is reachable off-loopback; the binding
    // must refuse it at serve time (OQ-401).
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let mut s = svc("nonlocal");
    let r = serve_net_n(listener, &mut s, TOK, 1);
    assert!(r.is_err(), "non-loopback socket must refuse");
}

/// P2/P3/P4/P9 — the four refusal legs, on the real socket.
#[test]
fn net_gate_refusals() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let req =
        format!("POST /hh-embed/1 HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer wrong\r\nContent-Length: 2\r\n\r\n{{}}");
    let client = std::thread::spawn(move || raw_request(addr, req));
    let handled = serve_net_n(listener, &mut svc("g1"), TOK, 1).unwrap();
    assert_eq!(handled, 1);
    assert_eq!(client.join().unwrap().0, 401);
}

/// The battery legs in one bounded server run — each connection gets
/// its refusal; the kernel's dispatch never sees them.
#[test]
fn net_gate_all_legs() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let cases: Vec<(String, u16)> = vec![
        // (a) foreign Host → 400
        (
            format!("POST /hh-embed/1 HTTP/1.1\r\nHost: evil.example\r\nAuthorization: Bearer {TOK}\r\nContent-Length: 2\r\n\r\n{{}}"),
            400,
        ),
        // (b) Origin present → 403 (browsers never speak here)
        (
            format!("POST /hh-embed/1 HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {TOK}\r\nOrigin: http://localhost\r\nContent-Length: 2\r\n\r\n{{}}"),
            403,
        ),
        // (c) missing token → 401
        (
            format!("POST /hh-embed/1 HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 2\r\n\r\n{{}}"),
            401,
        ),
        // (d) query token → 400
        (
            format!("GET /hh-embed/1/events?token=x HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {TOK}\r\n\r\n"),
            400,
        ),
        // sec-fetch metadata → 403
        (
            format!("POST /hh-embed/1 HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {TOK}\r\nsec-fetch-site: cross-site\r\nContent-Length: 2\r\n\r\n{{}}"),
            403,
        ),
    ];
    let n = cases.len();
    let client = std::thread::spawn(move || {
        cases
            .into_iter()
            .map(|(req, _)| raw_request(addr, req).0)
            .collect::<Vec<u16>>()
    });
    let handled = serve_net_n(listener, &mut svc("g2"), TOK, n).unwrap();
    assert_eq!(handled, n);
    let got = client.join().unwrap();
    let want: Vec<u16> = vec![400, 403, 401, 400, 403];
    assert_eq!(got, want);
}

/// Byte parity — `svc.handle` (binding a) and `POST /hh-embed/1` (c)
/// return identical canonical bytes for the same request.
#[test]
fn net_post_parity_with_handle() {
    let mut a = svc("pa");
    let req = Request {
        id: Json::Int(1),
        method: "hello".into(),
        params: Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("n")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("web")),
                ]),
            ),
            ("capabilities", Json::Obj(Default::default())),
        ]),
    };
    let direct = a.handle(&req).to_canonical_string();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let body = Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(1)),
        ("method", Json::str("hello")),
        (
            "params",
            Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str("n")),
                        ("version", Json::str("1")),
                        ("kind", Json::str("web")),
                    ]),
                ),
                ("capabilities", Json::Obj(Default::default())),
            ]),
        ),
    ])
    .to_canonical_string();
    let client = std::thread::spawn(move || rpc_once(&addr, TOK, &body).unwrap());
    let mut b = svc("pb");
    let handled = serve_net_n(listener, &mut b, TOK, 1).unwrap();
    assert_eq!(handled, 1);
    let (status, wire) = client.join().unwrap();
    assert_eq!(status, 200);
    // The result member is byte-equal to the binding-(a) response's.
    let wa = hh_wire::json::parse(&direct).unwrap();
    let wb = hh_wire::json::parse(&wire).unwrap();
    assert_eq!(
        wa.get("result").map(Json::to_canonical_string),
        wb.get("result").map(Json::to_canonical_string),
    );
}

/// `GET /events` drains the notification queue as NDJSON — empty when
/// nothing is pending; `wait_ms` clamps.
#[test]
fn net_events_drain() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || hh_embed::net::drain_events(&addr, TOK, 0).unwrap());
    let handled = serve_net_n(listener, &mut svc("ev"), TOK, 1).unwrap();
    assert_eq!(handled, 1);
    let (status, body) = client.join().unwrap();
    assert_eq!(status, 200);
    assert!(body.trim().is_empty(), "no pending → empty drain: {body}");
}

/// S4.10 regression — the surface's *real* `client{kind:"web",
/// surface_ref, sink, ui_caps}` declaration must decode and mint the
/// `lifecycle.session.attached` row verbatim. The stub kernels in
/// `hh-web/tests` never strict-decode; this drives the exact member set
/// `hh-web`'s `client_decl()` sends against the real `EmbedService`.
#[test]
fn net_open_attach_with_full_web_client_decl() {
    let root = dir("client-decl");
    let run_id = {
        let mut st = hh_ledger::store::Store::open(root.join("s")).unwrap();
        st.open_run(
            hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
            "holder-a",
        )
        .unwrap()
        .0
    };
    let mut a = EmbedService::open(ServiceConfig {
        store_root: root.join("s"),
        kernel_version_id: "k/0.1".into(),
        workspace_root: root.join("w"),
        holder: "net".into(),
    })
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    // hello + open_session — two connections.
    let run_id_c = run_id.clone();
    let client = std::thread::spawn(move || {
        rpc_once(
            &addr,
            TOK,
            &Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", Json::Int(1)),
                ("method", Json::str("hello")),
                (
                    "params",
                    Json::obj([
                        ("contract_major", Json::Int(1)),
                        (
                            "client",
                            Json::obj([
                                ("name", Json::str("hh-web")),
                                ("version", Json::str("0.1")),
                                ("kind", Json::str("web")),
                            ]),
                        ),
                        ("capabilities", Json::Obj(Default::default())),
                    ]),
                ),
            ])
            .to_canonical_string(),
        )
        .unwrap();
        rpc_once(
            &addr,
            TOK,
            &Json::obj([
                ("jsonrpc", Json::str("2.0")),
                ("id", Json::Int(2)),
                ("method", Json::str("open_session")),
                (
                    "params",
                    Json::obj([
                        (
                            "spec",
                            Json::obj([
                                ("kind", Json::str("attach")),
                                ("run_id", Json::str(run_id_c.clone())),
                                ("read_only", Json::Bool(true)),
                            ]),
                        ),
                        ("idempotency_key", Json::str("web:open:1")),
                        (
                            "client",
                            Json::obj([
                                ("kind", Json::str("web")),
                                ("surface_ref", Json::str("127.0.0.1:9999")),
                                (
                                    "sink",
                                    Json::obj([
                                        (
                                            "content_classes",
                                            Json::Arr(vec![Json::str("accounting")]),
                                        ),
                                        ("max_field_bytes", Json::Int(4096)),
                                    ]),
                                ),
                                (
                                    "ui_caps",
                                    Json::Arr(vec![
                                        Json::str("read_only"),
                                        Json::str("declared_writes"),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ])
            .to_canonical_string(),
        )
        .unwrap()
    });
    let handled = serve_net_n(listener, &mut a, TOK, 2).unwrap();
    assert_eq!(handled, 2);
    let (status, wire) = client.join().unwrap();
    assert_eq!(status, 200, "{wire}");
    let j = hh_wire::json::parse(&wire).unwrap();
    assert!(
        j.get("result")
            .and_then(|r| r.get("session_id"))
            .and_then(Json::as_str)
            .is_some(),
        "open_session must land, not UnknownField: {wire}"
    );
    // The durable row carries the declaration verbatim.
    let rows: Vec<String> = a
        .store()
        .events(&run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.session.attached")
        .map(|e| e.payload.to_canonical_string())
        .collect();
    assert_eq!(rows.len(), 1, "one attached row: {rows:?}");
    assert!(rows[0].contains("\"kind\":\"web\""), "{rows:?}");
    assert!(
        rows[0].contains("\"surface_ref\":\"127.0.0.1:9999\""),
        "{rows:?}"
    );
    assert!(rows[0].contains("\"ui_caps\""), "{rows:?}");
}

/// `token_matches` — the constant-time compare's truth table.
#[test]
fn net_token_match_truth_table() {
    use hh_embed::net::token_matches;
    assert!(token_matches("abc", "abc"));
    assert!(!token_matches("abc", "abd"));
    assert!(!token_matches("abc", "ab"));
    assert!(!token_matches("abc", "abcd"));
    // Empty-vs-empty compares equal — never a reachable state (the
    // minted token is ≥128 bits by construction).
    assert!(token_matches("", ""));
    assert!(!token_matches("abc", ""));
}
