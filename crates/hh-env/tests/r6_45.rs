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

/// A loop-accept HTTP fixture: replies `200` with a fixed body to every
/// connection until the test ends — a raced early connection must not
/// consume the fixture and reset the real forward.
fn http_fixture() -> (u16, thread::JoinHandle<()>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let h = thread::spawn(move || {
        for conn in l.incoming() {
            let Ok(mut s) = conn else { break };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nok!\n");
        }
    });
    (port, h)
}

/// A loop-accept fixture answering every request with a `body_len`-byte
/// body — the apt-index scale the pre-repair 8 MiB forward cap rejected.
/// The request head is consumed to its blank-line terminator first: a
/// single short `read` can return a partial head, and dropping the
/// socket with request bytes still unread answers the client `RST`
/// mid-body (`Connection reset by peer`), not a clean close.
fn big_body_fixture(body_len: usize) -> (u16, thread::JoinHandle<()>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let h = thread::spawn(move || {
        let head =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n");
        let block = vec![b'x'; 1 << 20];
        for conn in l.incoming() {
            let Ok(s) = conn else { break };
            let mut r = BufReader::new(s);
            let mut line = String::new();
            loop {
                line.clear();
                match r.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line == "\r\n" => break,
                    Ok(_) => {}
                }
            }
            let mut w = r.into_inner();
            if w.write_all(head.as_bytes()).is_err() {
                continue;
            }
            let mut left = body_len;
            while left > 0 {
                let n = left.min(block.len());
                if w.write_all(&block[..n]).is_err() {
                    break;
                }
                left -= n;
            }
        }
    });
    (port, h)
}

fn allow_rule(pattern: &str, port: u16) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(pattern).unwrap(),
        ports: vec![port],
        protocols: vec![EgressProtocol::Http],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

/// Spawn the real `hh-egress-mediator` over a temp store/socket under a
/// mediated policy carrying `rules` and the `--resolve` pins. Returns
/// `(store_root, sock, guard)` — the guard's drop kills the child.
fn spawn_mediator(
    tag: &str,
    rules: Vec<EgressRule>,
    resolves: &[&str],
) -> (PathBuf, PathBuf, MediatorProc) {
    let root = dir(tag);
    let store_root = root.join("store");
    let sock = root.join("egress.sock");
    let policy_path = root.join("policy.json");
    let _ = std::fs::create_dir_all(&root);

    let mut pol = kernel_default(0);
    pol.net.mode = NetMode::Mediated;
    pol.net.rules = rules;
    pol.net.default_unmatched = DefaultUnmatched::Deny;
    pol.net.non_public_destinations = NonPublic::AllowListed;
    pol.net.unix_sockets.mode = UnixSocketMode::AllowListed;
    pol.net.dns.resolver = hharness_containment::policy::DnsResolver::Mediator;
    pol.compute_ids();
    let policy = pol.to_json().to_canonical_string();
    std::fs::write(&policy_path, &policy).unwrap();

    let mut args = vec![
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
    ];
    for r in resolves {
        args.push("--resolve".to_string());
        args.push((*r).to_string());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_hh-egress-mediator"))
        .args(args)
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
    let guard = MediatorProc { child };
    thread::spawn(move || {
        let mut sink = String::new();
        while out.read_line(&mut sink).is_ok_and(|n| n > 0) {
            sink.clear();
        }
    });
    (store_root, sock, guard)
}

fn socket_request_bytes(sock: &PathBuf, req: &[u8]) -> Vec<u8> {
    let mut s = UnixStream::connect(sock).expect("connect mediator socket");
    s.write_all(req).unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    out
}

fn socket_request(sock: &PathBuf, req: &[u8]) -> String {
    String::from_utf8_lossy(&socket_request_bytes(sock, req)).to_string()
}

#[test]
fn wc_mediator_socket_path_records_requests_and_decides() {
    // Upstream fixture — the allow-listed host resolves to loopback via
    // `--resolve` (the declared pin seam; previously silently dropped by
    // the mediator's per-connection resolver rebuild).
    let (port, fixture) = http_fixture();

    let (store_root, sock, _guard) = spawn_mediator(
        "med",
        vec![allow_rule("**.fixture.test", port)],
        &["fixture.test=127.0.0.1"],
    );

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

    drop(fixture); // detached — the loop-accept thread dies at test exit.
}

/// R6.45h — the seventh Wc defect, end to end through the real binary.
/// The mediated forward buffered each response under
/// `read_response_full(_, 8 * 1024 * 1024)` (pre-existing since R4.5c,
/// `a7292296`): Ubuntu `noble/universe`'s `Packages.xz` (~15 MB) crosses
/// it, the body read fails `body too large`, `EgressMediator::forward`
/// surfaces `EgressError::Transport`, and `serve_conn` answers the
/// *allowed* request `502 Bad Gateway` — `apt-get update` dead under
/// mediation → `verifier_unavailable:bootstrap:signature` with
/// `denied_hosts: []` (a cap, not a deny). This leg drives a `GET` for a
/// >8 MiB body through the socket: pre-repair the reply head is `502`;
/// repaired it is `200` and every forwarded byte lands, with the
/// `security.egress.decided{allow}` row intact.
#[test]
fn wc_forward_passes_apt_scale_response_bodies() {
    // The real `noble/universe` Packages.xz scale — above the pre-repair
    // 8 MiB cap.
    let big_len = 16 * 1024 * 1024usize;
    let (port, fixture) = big_body_fixture(big_len);
    let (store_root, sock, _guard) = spawn_mediator(
        "big",
        vec![allow_rule("**.big.test", port)],
        &["big.test=127.0.0.1"],
    );

    let resp = socket_request_bytes(
        &sock,
        format!("GET http://big.test:{port}/Packages HTTP/1.1\r\nHost: big.test\r\n\r\n")
            .as_bytes(),
    );
    assert!(
        resp.starts_with(b"HTTP/1.1 200"),
        "allowed apt-scale forward must answer 200, not 502, got: {:?}",
        String::from_utf8_lossy(&resp[..resp.len().min(512)])
    );
    let head_end = resp
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("response head terminator");
    assert_eq!(
        resp.len() - (head_end + 4),
        big_len,
        "the full fixture body must relay to the cell"
    );

    // The decided row — the allow is recorded, not a deny (the pre-repair
    // 502 followed an `allow` decision: `denied_hosts: []` verbatim).
    let store = Store::open(&store_root).expect("open mediator store");
    let events = store.events("w1-test").expect("events");
    let decided: Vec<String> = events
        .iter()
        .filter(|e| e.class == "security.egress.decided")
        .map(|e| e.payload.to_canonical_string())
        .collect();
    assert_eq!(decided.len(), 1);
    assert!(
        decided[0].contains("\"allow\""),
        "the apt-scale forward decided allow: {decided:?}"
    );

    drop(fixture); // detached — the loop-accept thread dies at test exit.
}
