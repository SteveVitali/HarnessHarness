//! R6.45/Wc repair regression — the `hh-egress-mediator` **binary's** own
//! socket path, end to end. The Wc window's H-allow leg recorded
//! `denied_hosts: []` with **zero** `security.egress.*` rows on every
//! member: `serve_conn` minted the request's scope as
//! `{turn, tool_call}` with no `model_call_id`, the ledger's scope
//! grammar (`tool_call_id ⇒ model_call_id`, hh-ledger store) refused
//! the `security.egress.requested` append as a `SchemaViolation`, the
//! gate errored and the connection got a bare `500` — every request
//! died *before* `decide_egress`, upstream of any per-host decision.
//!
//! This test drives the real binary over its real unix socket: a
//! `GET http://…` (the apt/`http_proxy` form) to an allow-listed host
//! pinned to a loopback fixture by `--resolve`, plus a `CONNECT` to an
//! unlisted host. It asserts the response *and* the durable rows the
//! defect suppressed (`security.egress.requested`/`decided`). It fails
//! on the unfixed binary — every request answered `500` and no rows.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use hharness_containment::policy::{
    kernel_default, DefaultUnmatched, EgressProtocol, EgressRule, HostPattern, NetMode, NonPublic,
    RuleDecision, UnixSocketMode,
};
use hharness_ledger::store::Store;
use hharness_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r6-45-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

struct MediatorProc {
    child: Child,
}

impl Drop for MediatorProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A one-shot HTTP fixture: replies `200` with a fixed body to the
/// first connection, then exits.
fn http_fixture() -> (u16, thread::JoinHandle<()>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let h = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf);
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nok!\n")
            .unwrap();
    });
    (port, h)
}

fn socket_request(sock: &PathBuf, req: &[u8]) -> String {
    let mut s = UnixStream::connect(sock).expect("connect mediator socket");
    s.write_all(req).unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    String::from_utf8_lossy(&out).to_string()
}

#[test]
fn wc_mediator_socket_path_records_requests_and_decides() {
    let root = dir("med");
    let store_root = root.join("store");
    let sock = root.join("egress.sock");
    let policy_path = root.join("policy.json");
    let _ = std::fs::create_dir_all(&root);

    // Upstream fixture — the allow-listed host resolves to loopback via
    // `--resolve` (the declared pin seam; previously silently dropped by
    // the mediator's per-connection resolver rebuild).
    let (port, fixture) = http_fixture();

    let allow = EgressRule {
        host: HostPattern::parse("**.fixture.test").unwrap(),
        ports: vec![port],
        protocols: vec![EgressProtocol::Http],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    };
    let mut pol = kernel_default(0);
    pol.net.mode = NetMode::Mediated;
    pol.net.rules = vec![allow];
    pol.net.default_unmatched = DefaultUnmatched::Deny;
    pol.net.non_public_destinations = NonPublic::AllowListed;
    pol.net.unix_sockets.mode = UnixSocketMode::AllowListed;
    pol.net.dns.resolver = hharness_containment::policy::DnsResolver::Mediator;
    pol.compute_ids();
    let policy = pol.to_json().to_canonical_string();
    std::fs::write(&policy_path, &policy).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-egress-mediator"))
        .args([
            "--store-root".to_string(),
            store_root.display().to_string(),
            "--socket".to_string(),
            sock.display().to_string(),
            "--policy".to_string(),
            policy_path.display().to_string(),
            "--env-handle".to_string(),
            "env-test".to_string(),
            "--effect-id".to_string(),
            "eff-test".to_string(),
            "--participant".to_string(),
            "oracle".to_string(),
            "--run-id".to_string(),
            "w1-test".to_string(),
            "--resolve".to_string(),
            "fixture.test=127.0.0.1".to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mediator");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    // The ready handshake is the only synchronization — no fixed sleep.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ready = String::new();
    while !ready.starts_with("ready ") {
        ready.clear();
        if out.read_line(&mut ready).unwrap() == 0 || Instant::now() > deadline {
            panic!("mediator ready handshake failed: {ready:?}");
        }
    }
    let _guard = MediatorProc { child };
    thread::spawn(move || {
        let mut sink = String::new();
        while out.read_line(&mut sink).is_ok_and(|n| n > 0) {
            sink.clear();
        }
    });

    // The apt/`http_proxy` form — absolute-URI GET to an allow-listed
    // host pinned to the loopback fixture.
    let resp = socket_request(
        &sock,
        format!("GET http://fixture.test:{port}/ok HTTP/1.1\r\nHost: fixture.test\r\n\r\n")
            .as_bytes(),
    );
    assert!(
        resp.starts_with("HTTP/1.1 200"),
        "allowed GET must forward to the fixture, got: {resp:?}"
    );
    assert!(resp.contains("ok!"), "fixture body forwarded: {resp:?}");

    // An unlisted host — a real per-host deny the row set must show.
    let denied = socket_request(
        &sock,
        b"CONNECT denied.test:443 HTTP/1.1\r\nHost: denied.test\r\n\r\n",
    );
    assert!(
        denied.starts_with("HTTP/1.1 403"),
        "unlisted host must deny, got: {denied:?}"
    );

    // The durable rows — the exact signature Wc's leg recorded as 0.
    let store = Store::open(&store_root).expect("open mediator store");
    let events = store.events("w1-test").expect("events");
    let count = |class: &str| events.iter().filter(|e| e.class == class).count();
    assert_eq!(
        count("security.egress.requested"),
        2,
        "every mediated request must land `requested` before the decision"
    );
    let decided: Vec<&Json> = events
        .iter()
        .filter(|e| e.class == "security.egress.decided")
        .map(|e| &e.payload)
        .collect();
    assert_eq!(decided.len(), 2, "every decided request lands `decided`");
    let flat: Vec<String> = decided.iter().map(|j| j.to_canonical_string()).collect();
    assert!(
        flat.iter().any(|f| f.contains("\"allow\"")),
        "the allow-listed host must decide allow: {flat:?}"
    );
    assert!(
        flat.iter().any(|f| f.contains("\"deny\"")),
        "the unlisted host must decide deny: {flat:?}"
    );

    fixture.join().unwrap();
}
