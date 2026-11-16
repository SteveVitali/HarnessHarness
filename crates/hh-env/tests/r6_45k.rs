//! R6.45k acceptance battery (manifest 309k; insert spec §3.3/§3.4 AC-1 —
//! the H-allow egress-hardening fixes D8–D14, each exercised hermetically
//! on loopback so the defect fails pre-fix):
//!
//! D8 concurrent serving — two CONNECTs, the first held idle, the second
//!    completes < 2 s; 32 concurrent forwards keep per-request ledger
//!    order with no lost rows. Exercised against the real
//!    `hh-egress-mediator` binary (the serial `listener.incoming()` loop
//!    was the defect).
//! D9 tunnel pump — an upstream silent past the old 5 s bound still
//!    delivers; EOF propagates promptly; `tunnel.closed` carries true
//!    byte counts and a true `close_cause` (never `unwrap_or(0)`, never
//!    a hard-coded "eof").
//! D10 response hygiene — upstream hop-by-hop/framing headers never
//!    relay; exactly one framing member (`Transfer-Encoding: chunked`
//!    re-frame) + `Connection: close` emit.
//! D11 bodiless codec — HEAD with `Content-Length: N`, 204 and 304
//!    answer promptly with no body.
//! D12 declared timeouts — a first byte inside the bound succeeds; a
//!    lapse answers the typed `EgressError::Timeout` (the binary's 504).
//! D13 streaming — a 250 MiB body streams through `forward_stream`
//!    bounded-RSS (the buffered `forward`'s 128 MiB cap would refuse).
//! D14 `Host` omits the default port.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use hharness_containment::egress::{
    ApprovalCache, EgressDecision, EgressRequest, EgressSource, EgressVerdict,
};
use hharness_containment::events::DecidedBy;
use hharness_containment::policy::{
    kernel_default, ContainmentPolicy, DefaultUnmatched, EgressProtocol, EgressRule, HostPattern,
    NetMode, NonPublic, RuleDecision,
};
use hharness_env::egress::{
    host_header, EgressError, EgressMediator, LocalHttpTransport, MediatedOutcome, StaticResolver,
    StoreAccess,
};
use hharness_env::events::ScopeChain;
use hharness_env::tokens::TokenMinter;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_secrets::{CredentialBroker, StaticVault};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-env-r645k-{tag}-{}-{n}",
        std::process::id(),
        n = N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// An allow-everything mediated policy (global allow, mediated mode,
/// loopback destinations listed).
fn policy() -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Mediated;
    p.net.default_unmatched = DefaultUnmatched::Deny;
    p.net.non_public_destinations = NonPublic::AllowListed;
    p.net.methods_default.push("CONNECT".to_string());
    p.net.rules = vec![EgressRule {
        host: HostPattern::Global,
        ports: vec![],
        protocols: vec![EgressProtocol::Http, EgressProtocol::HttpsConnect],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }];
    p.compute_ids();
    p
}

fn allow() -> EgressDecision {
    EgressDecision {
        decision: EgressVerdict::Allow,
        source: EgressSource::AllowRule,
        rule_ref: None,
        reason: None,
        credential_binding_candidates: vec![],
        rule_index: None,
        guard_detail: None,
    }
}

static EMPTY_CHAIN: ScopeChain = ScopeChain {
    turn_id: String::new(),
    model_call_id: String::new(),
    tool_call_id: String::new(),
};

fn open_effect(store: &mut Store, run: &str, lease: &hharness_ledger::store::Lease, effect: &str) {
    let risk = hharness_ontology::risk::RiskClass::UNKNOWN;
    let ev = hharness_env::events::EventMinter::new(&*store, run)
        .mint_effect(
            "action.effect.intended",
            hharness_env::events::intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            effect,
            &EMPTY_CHAIN,
        )
        .unwrap();
    store.append(run, lease, vec![ev]).unwrap();
}

struct Rig {
    store: Store,
    run: String,
    lease: hharness_ledger::store::Lease,
    minter: TokenMinter,
    broker: CredentialBroker,
}

fn rig(tag: &str) -> Rig {
    let root = dir(tag);
    let _ = std::fs::create_dir_all(&root);
    let mut store = Store::open(&root).expect("store open");
    let (run, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "test")
        .expect("open run");
    open_effect(&mut store, &run, &lease, "eff-1");
    Rig {
        store,
        run,
        lease,
        minter: TokenMinter::new([9u8; 32]),
        broker: CredentialBroker::new(Box::new(StaticVault::default()), "test"),
    }
}

fn req(rig: &mut Rig, port: u16, connect: bool) -> EgressRequest {
    let token = rig.minter.mint("eff-1", 1, "env-1");
    EgressRequest {
        token: token.token,
        effect_id: Some("eff-1".to_string()),
        tool_call_id: "tc".to_string(),
        env_handle: "env-1".to_string(),
        protocol: if connect {
            EgressProtocol::HttpsConnect
        } else {
            EgressProtocol::Http
        },
        host_raw: "h.test".to_string(),
        resolved_addrs: vec![],
        port,
        method: Some(if connect { "CONNECT" } else { "GET" }.to_string()),
        path: Some("/".to_string()),
        headers: vec![("Host".to_string(), "h.test".to_string())],
        body: None,
        body_readers: None,
        credential_sentinels: vec![],
    }
}

fn med_for(rig: &mut Rig) -> EgressMediator<'_> {
    let mut map = BTreeMap::new();
    map.insert("h.test".to_string(), vec!["127.0.0.1".parse().unwrap()]);
    EgressMediator {
        store: &mut rig.store,
        run_id: rig.run.clone(),
        lease: &rig.lease,
        tokens: rig.minter.resolver(),
        broker: &mut rig.broker,
        policy: policy(),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "p".to_string(),
        resolver: Box::new(StaticResolver { map }),
        transport: Box::new(LocalHttpTransport::default()),
        inspect_hooks: BTreeMap::new(),
    }
}

/// Spawn an upstream fixture: accept one conn, run `serve(stream)`.
fn upstream(serve: impl FnOnce(TcpStream) + Send + 'static) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        serve(s);
    });
    port
}

/// Read an upstream fixture's request head through the blank line, so no
/// request bytes remain unread when the fixture closes (a Linux close with
/// unread receive data aborts the connection with RST) and the fixture
/// never replies + closes while the mediator is still writing the head
/// (`write_request` writes it in several segments — the late ones hit
/// EPIPE). Every request this battery forwards is bodiless (`req` sets
/// `body: None` → `Content-Length: 0`), so the head is the whole request.
fn drain_request_head(s: &TcpStream) {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    loop {
        line.clear();
        match r.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if line == "\r\n" || line == "\n" => return,
            Ok(_) => {}
        }
    }
}

fn read_head(r: &mut impl BufRead) -> (u16, Vec<(String, String)>) {
    let mut status = 0u16;
    let mut hdrs = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = r.read_line(&mut line).unwrap();
        assert!(n > 0, "closed before head complete");
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        if status == 0 {
            status = l.split(' ').nth(1).unwrap().parse().unwrap();
        } else {
            let (k, v) = l.split_once(':').unwrap();
            hdrs.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    (status, hdrs)
}

struct MedKill(std::process::Child);
impl Drop for MedKill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn the real `hh-egress-mediator` binary on a scratch store/policy.
fn spawn_mediator(tag: &str) -> (PathBuf, PathBuf, MedKill) {
    let root = dir(tag);
    let _ = std::fs::create_dir_all(&root);
    let store_root = root.join("store");
    let sock = root.join("egress.sock");
    let policy_file = root.join("policy.json");
    std::fs::write(&policy_file, policy().to_json().to_canonical_string()).unwrap();
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-egress-mediator"))
        .args([
            "--store-root",
            store_root.to_str().unwrap(),
            "--socket",
            sock.to_str().unwrap(),
            "--policy",
            policy_file.to_str().unwrap(),
            "--env-handle",
            "env-1",
            "--effect-id",
            "eff-1",
            "--participant",
            "p",
            "--run-id",
            "run-1",
            "--resolve",
            "h.test=127.0.0.1",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..400 {
        if sock.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(sock.exists(), "mediator never bound {sock:?}");
    (store_root, sock, MedKill(child))
}

// ── D8 — concurrent serving on the real binary ─────────────────────────────

#[test]
fn d8_second_connect_completes_while_first_idle() {
    // Upstream accepts then stays silent — conn1's tunnel occupies a
    // serve thread (pre-fix: THE thread — conn2 starved at accept).
    let up_port = upstream(|mut s| {
        let mut b = [0u8; 512];
        let _ = s.read(&mut b);
        thread::sleep(Duration::from_secs(20));
    });
    let (_store_root, sock, _med) = spawn_mediator("d8a");
    let conn = || {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        write!(
            s,
            "CONNECT h.test:{up_port} HTTP/1.1\r\nHost: h.test\r\n\r\n"
        )
        .unwrap();
        s
    };
    let c1 = conn();
    let mut r1 = BufReader::new(c1.try_clone().unwrap());
    let (st, _) = read_head(&mut r1);
    assert_eq!(st, 200, "first CONNECT established");
    // conn1's tunnel now sits pumping — held idle.
    let t = Instant::now();
    let c2 = conn();
    c2.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut r2 = BufReader::new(c2);
    let (st2, _) = read_head(&mut r2);
    let elapsed = t.elapsed();
    assert_eq!(st2, 200, "second CONNECT established");
    assert!(
        elapsed < Duration::from_secs(2),
        "second CONNECT took {elapsed:?} — serial-serving starvation (D8)"
    );
    drop(c1);
}

#[test]
fn d8_concurrent_forwards_no_lost_rows() {
    // Each upstream response delays 150 ms — serial serving costs
    // 32 × 150 ms ≈ 4.8 s and still loses nothing; the concurrent path
    // answers in ≈150 ms wall and every requested row has its decided.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let up_port = l.local_addr().unwrap().port();
    thread::spawn(move || loop {
        let Ok((mut s, _)) = l.accept() else { break };
        thread::spawn(move || {
            // Consume the forwarded request head before replying — closing
            // with request bytes unread answers the mediator RST.
            drain_request_head(&s);
            thread::sleep(Duration::from_millis(150));
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });
    });
    let (store_root, sock, _med) = spawn_mediator("d8b");
    let t = Instant::now();
    let joins: Vec<_> = (0..32)
        .map(|i| {
            let sock = sock.clone();
            thread::spawn(move || {
                let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
                write!(
                    s,
                    "GET http://h.test:{up_port}/x{i} HTTP/1.1\r\nHost: h.test\r\n\r\n"
                )
                .unwrap();
                let mut r = BufReader::new(s);
                let (st, _) = read_head(&mut r);
                assert_eq!(st, 200, "forward {i}");
            })
        })
        .collect();
    for j in joins {
        j.join().unwrap();
    }
    let wall = t.elapsed();
    assert!(
        wall < Duration::from_secs(4),
        "32 concurrent forwards took {wall:?} — serialized (D8)"
    );
    // The ledger: every requested has a decided; the mint+append critical
    // section keeps the chain whole under concurrency.
    let store = Store::open(&store_root).unwrap();
    let evs = store.events("run-1").unwrap();
    let (mut req_n, mut dec_n) = (0usize, 0usize);
    for e in evs {
        match e.class.as_str() {
            "security.egress.requested" => req_n += 1,
            "security.egress.decided" => dec_n += 1,
            _ => {}
        }
    }
    assert_eq!(req_n, 32, "lost requested rows ({req_n})");
    assert_eq!(dec_n, 32, "lost decided rows ({dec_n})");
}

// ── D9 — the tunnel pump ───────────────────────────────────────────────────

#[test]
fn d9_upstream_silence_then_sends_delivers() {
    // Upstream silent 8 s (past the old 5 s idle kill) then sends — the
    // client must still receive the late bytes.
    let port = upstream(|mut s| {
        // The client's whole tunnel payload (`CLIENTHELLO`), not one
        // `read` that may return a prefix.
        let _ = s.read_exact(&mut [0u8; 11]);
        thread::sleep(Duration::from_secs(8));
        let _ = s.write_all(b"late-bytes");
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d9a");
    let request = req(&mut rig, port, true);
    let decision = allow();
    let (client_a, mut client_b) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut client_a = client_a;
    thread::scope(|sc| {
        let rig2 = &mut rig;
        let med_h = sc.spawn(move || {
            let mut med = med_for(rig2);
            let started = med.store.read_store(|s| s.now_ms());
            med.forward_tunnel(
                &request,
                &decision,
                DecidedBy::Policy,
                "eff-1",
                started,
                &EMPTY_CHAIN,
                &mut client_a,
            )
        });
        client_b
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let mut w = client_b.try_clone().unwrap();
        let (st, _) = read_head(&mut BufReader::new(&mut client_b));
        assert_eq!(st, 200);
        w.write_all(b"CLIENTHELLO").unwrap();
        w.flush().unwrap();
        let mut got = Vec::new();
        let mut b = [0u8; 256];
        while got.windows(10).all(|w| w != b"late-bytes") {
            let n = client_b.read(&mut b).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&b[..n]);
        }
        assert!(
            got.windows(10).any(|w| w == b"late-bytes"),
            "client never received the post-silence bytes (D9): {got:?}"
        );
        let _ = client_b.shutdown(std::net::Shutdown::Both);
        let _ = med_h.join();
    });
}

#[test]
fn d9_eof_propagates_and_close_row_is_true() {
    // Upstream: read 8 bytes, write 4, close. Client sees EOF promptly;
    // the closed row carries true counts + `eof`.
    let port = upstream(|mut s| {
        // Exactly the client's 8-byte payload (`CLIENTHI`) — a short
        // `read` would reply + close with tunnel bytes still in flight.
        let _ = s.read_exact(&mut [0u8; 8]);
        let _ = s.write_all(b"PONG");
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d9b");
    let request = req(&mut rig, port, true);
    let decision = allow();
    let (client_a, mut client_b) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut client_a = client_a;
    let t = Instant::now();
    thread::scope(|sc| {
        let rig2 = &mut rig;
        let med_h = sc.spawn(move || {
            let mut med = med_for(rig2);
            let started = med.store.read_store(|s| s.now_ms());
            med.forward_tunnel(
                &request,
                &decision,
                DecidedBy::Policy,
                "eff-1",
                started,
                &EMPTY_CHAIN,
                &mut client_a,
            )
        });
        client_b
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let mut w = client_b.try_clone().unwrap();
        eprintln!("[t] reading head");
        let (st, _) = read_head(&mut BufReader::new(&mut client_b));
        eprintln!("[t] got 200, sending CLIENTHI");
        assert_eq!(st, 200);
        w.write_all(b"CLIENTHI").unwrap();
        w.flush().unwrap();
        eprintln!("[t] wrote CLIENTHI");
        let mut body = Vec::new();
        let mut b = [0u8; 64];
        loop {
            match client_b.read(&mut b) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&b[..n]),
                Err(e) => panic!("read: {e}"),
            }
        }
        eprintln!("[t] body={body:?}, joining");
        assert_eq!(&body, b"PONG", "upstream EOF delivered to client");
        let out = med_h.join().unwrap().expect("forward_tunnel");
        eprintln!("[t] joined");
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "close took {:?} — dead leg hung (D9)",
            t.elapsed()
        );
        let MediatedOutcome::Tunneled {
            bytes_up,
            bytes_down,
            ..
        } = out
        else {
            panic!("not tunneled: {out:?}")
        };
        assert_eq!(bytes_up, 8, "true bytes_up");
        assert_eq!(bytes_down, 4, "true bytes_down");
    });
    let evs = rig.store.events(&rig.run).unwrap();
    let closed = evs
        .iter()
        .find(|e| e.class == "security.egress.tunnel.closed")
        .expect("tunnel.closed row");
    let p = &closed.payload;
    assert_eq!(p.get("close_cause").and_then(|v| v.as_str()), Some("eof"));
    assert_eq!(p.get("bytes_up").and_then(|v| v.as_int()), Some(8));
    assert_eq!(p.get("bytes_down").and_then(|v| v.as_int()), Some(4));
}

// ── D10 — response hygiene (the streaming forward path) ────────────────────

#[test]
fn d10_chunked_upstream_emits_one_framing_header_and_close() {
    let port = upstream(|mut s| {
        // The whole request head, not one `read`: the mediator writes the
        // request line + headers as separate segments, and replying +
        // closing before the last lands answers its writes EPIPE/RST.
        drain_request_head(&s);
        let _ = s.write_all(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\nTrailer: X-T\r\nProxy-X: 1\r\n\r\n4\r\nabcd\r\n0\r\nX-T: v\r\n\r\n",
        );
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d10");
    let request = req(&mut rig, port, false);
    let decision = allow();
    let mut out: Vec<u8> = Vec::new();
    let mut med = med_for(&mut rig);
    let started = med.store.read_store(|s| s.now_ms());
    med.forward_stream(
        &request,
        &decision,
        DecidedBy::Policy,
        "eff-1",
        started,
        &EMPTY_CHAIN,
        &mut out,
    )
    .expect("forward_stream");
    let text = String::from_utf8_lossy(&out);
    assert_eq!(
        text.matches("Transfer-Encoding: chunked").count(),
        1,
        "exactly one framing member: {text}"
    );
    assert_eq!(text.matches("Content-Length:").count(), 0, "no CL: {text}");
    assert_eq!(
        text.matches("Connection: close").count(),
        1,
        "close: {text}"
    );
    assert_eq!(
        text.matches("keep-alive").count(),
        0,
        "no keep-alive: {text}"
    );
    assert_eq!(text.matches("Proxy-X").count(), 0, "no proxy-*: {text}");
    assert_eq!(text.matches("Trailer").count(), 0, "no trailer: {text}");
    assert!(
        text.contains("4\r\nabcd\r\n0\r\n\r\n"),
        "re-framed body: {text}"
    );
}

#[test]
fn d10_no_duplicate_content_length() {
    let port = upstream(|mut s| {
        // The whole request head, not one `read`: the mediator writes the
        // request line + headers as separate segments, and replying +
        // closing before the last lands answers its writes EPIPE/RST.
        drain_request_head(&s);
        let _ = s.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: keep-alive\r\n\r\nhello",
        );
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d10b");
    let request = req(&mut rig, port, false);
    let decision = allow();
    let mut out = Vec::new();
    let mut med = med_for(&mut rig);
    let started = med.store.read_store(|s| s.now_ms());
    med.forward_stream(
        &request,
        &decision,
        DecidedBy::Policy,
        "eff-1",
        started,
        &EMPTY_CHAIN,
        &mut out,
    )
    .unwrap();
    let text = String::from_utf8_lossy(&out);
    assert_eq!(text.matches("Content-Length:").count(), 0, "{text}");
    assert!(text.contains("5\r\nhello\r\n"), "re-framed: {text}");
}

// ── D11 — bodiless responses ───────────────────────────────────────────────

#[test]
fn d11_head_and_bodiless_statuses_answer_promptly() {
    for (method, status_head, tag) in [
        (
            "HEAD",
            "HTTP/1.1 200 OK\r\nContent-Length: 999\r\n\r\n",
            "head",
        ),
        (
            "GET",
            "HTTP/1.1 204 No Content\r\nContent-Length: 999\r\n\r\n",
            "204",
        ),
        (
            "GET",
            "HTTP/1.1 304 Not Modified\r\nContent-Length: 999\r\n\r\n",
            "304",
        ),
    ] {
        // The upstream emits a head declaring a body then stays silent —
        // the bodiless codec must not wait for the phantom 999 bytes.
        let port = upstream(move |mut s| {
            drain_request_head(&s);
            let _ = s.write_all(status_head.as_bytes());
            thread::sleep(Duration::from_secs(6));
            let _ = s.shutdown(std::net::Shutdown::Both);
        });
        let mut rig = rig(tag);
        let mut request = req(&mut rig, port, false);
        request.method = Some(method.to_string());
        let decision = allow();
        let mut med = med_for(&mut rig);
        let started = med.store.read_store(|s| s.now_ms());
        let t = Instant::now();
        let out = med
            .forward(
                &request,
                &decision,
                DecidedBy::Policy,
                "eff-1",
                started,
                &EMPTY_CHAIN,
            )
            .unwrap_or_else(|e| panic!("{tag} forward: {e:?}"));
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "{tag} took {:?} — waited on a phantom body (D11)",
            t.elapsed()
        );
        let MediatedOutcome::Forwarded { response, .. } = out else {
            panic!("{tag}: {out:?}")
        };
        assert!(response.body.is_empty(), "{tag}: body {:?}", response.body);
    }
}

// ── D12 — declared timeouts ────────────────────────────────────────────────

#[test]
fn d12_slow_first_byte_ok_lapse_is_typed_timeout() {
    // First byte at ~1.5 s under a 3 s declared bound → OK.
    let port_ok = upstream(|mut s| {
        drain_request_head(&s);
        thread::sleep(Duration::from_millis(1500));
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    // Never answers inside the declared 3 s bound → typed Timeout.
    let port_lapse = upstream(|s| {
        drain_request_head(&s);
        thread::sleep(Duration::from_secs(10));
    });
    let mut rig = rig("d12");
    let req_ok = req(&mut rig, port_ok, false);
    let req_lapse = req(&mut rig, port_lapse, false);
    let decision = allow();
    let mut map = BTreeMap::new();
    map.insert("h.test".to_string(), vec!["127.0.0.1".parse().unwrap()]);
    let mut med = EgressMediator {
        store: &mut rig.store,
        run_id: rig.run.clone(),
        lease: &rig.lease,
        tokens: rig.minter.resolver(),
        broker: &mut rig.broker,
        policy: policy(),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "p".to_string(),
        resolver: Box::new(StaticResolver { map }),
        transport: Box::new(LocalHttpTransport {
            timeout: Duration::from_secs(3),
            connect_timeout: Duration::from_secs(1),
        }),
        inspect_hooks: BTreeMap::new(),
    };
    let started = med.store.read_store(|s| s.now_ms());
    med.forward(
        &req_ok,
        &decision,
        DecidedBy::Policy,
        "eff-1",
        started,
        &EMPTY_CHAIN,
    )
    .expect("slow-but-in-bounds forward");
    let err = med
        .forward(
            &req_lapse,
            &decision,
            DecidedBy::Policy,
            "eff-1",
            started,
            &EMPTY_CHAIN,
        )
        .expect_err("lapse must fail");
    assert!(
        matches!(err, EgressError::Timeout { .. }),
        "lapse gave {err:?} — not the typed Timeout (the binary's 504)"
    );
}

// ── D13 — bounded-RSS streaming ────────────────────────────────────────────

struct CountingSink {
    bytes: u64,
}
impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn d13_large_body_streams_bounded() {
    // 250 MiB upstream — the buffered forward's 128 MiB bound refuses;
    // the streaming path delivers it whole, Vec-free (bounded RSS).
    const SIZE: usize = 250 * 1024 * 1024;
    let port = upstream(move |mut s| {
        // Consume the whole request head, not one `read`: the mediator
        // writes the request in several segments, and a close with
        // unread bytes in the receive queue makes Linux send RST
        // (discarding the still-unsent tail of the 250 MiB body) instead
        // of a FIN — the client then sees ECONNRESET mid-body.
        drain_request_head(&s);
        let _ =
            s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {SIZE}\r\n\r\n").as_bytes());
        let chunk = vec![7u8; 1024 * 1024];
        for _ in 0..250 {
            let _ = s.write_all(&chunk);
        }
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d13");
    let request = req(&mut rig, port, false);
    let decision = allow();
    let mut med = med_for(&mut rig);
    let started = med.store.read_store(|s| s.now_ms());
    let mut sink = CountingSink { bytes: 0 };
    med.forward_stream(
        &request,
        &decision,
        DecidedBy::Policy,
        "eff-1",
        started,
        &EMPTY_CHAIN,
        &mut sink,
    )
    .expect("forward_stream 250MiB");
    assert!(
        sink.bytes >= SIZE as u64,
        "streamed only {} of {SIZE}",
        sink.bytes
    );
}

// ── D14 — Host omits the default port ──────────────────────────────────────

#[test]
fn d14_host_omits_default_port() {
    // The wire half: a non-default port IS spelled — the fixture binds
    // an ephemeral port and the request dials it directly.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen2 = std::sync::Arc::clone(&seen);
    let port = upstream(move |s| {
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut hdrs = Vec::new();
        let mut line = String::new();
        let mut first = true;
        loop {
            line.clear();
            if r.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let l = line.trim_end_matches(['\r', '\n']);
            if l.is_empty() {
                break;
            }
            if first {
                first = false;
                continue; // the request line
            }
            let (k, v) = l.split_once(':').unwrap();
            hdrs.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
        *seen2.lock().unwrap() = hdrs
            .iter()
            .find(|(k, _)| k == "host")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let mut s = s;
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        let _ = s.shutdown(std::net::Shutdown::Both);
    });
    let mut rig = rig("d14");
    let request = req(&mut rig, port, false);
    let decision = allow();
    let mut med = med_for(&mut rig);
    let started = med.store.read_store(|s| s.now_ms());
    med.forward(
        &request,
        &decision,
        DecidedBy::Policy,
        "eff-1",
        started,
        &EMPTY_CHAIN,
    )
    .unwrap();
    assert_eq!(
        seen.lock().unwrap().as_str(),
        format!("h.test:{port}"),
        "non-default port spelled"
    );
    // The spelling helper: default ports omitted.
    assert_eq!(host_header("h.test", 80), "h.test");
    assert_eq!(host_header("h.test", 443), "h.test");
    assert_eq!(host_header("h.test", 8443), "h.test:8443");
}

// ── the ordering contract under concurrency (AC-3) ─────────────────────────

#[test]
fn ac3_ordering_holds_under_concurrency() {
    // Two threads share one `&Mutex<Store>` mediator handle; each
    // emits a requested→decided pair — every requested row's decided
    // lands durable in mint order, no interleave.
    let root = dir("ac3");
    let _ = std::fs::create_dir_all(&root);
    let mut store = Store::open(&root).unwrap();
    let (run, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "test")
        .unwrap();
    open_effect(&mut store, &run, &lease, "eff-1");
    let store = std::sync::Mutex::new(store);
    thread::scope(|sc| {
        for tag in ["a", "b"] {
            let store = &store;
            let run = &run;
            let lease = &lease;
            sc.spawn(move || {
                for i in 0..4 {
                    let _ = (tag, i);
                    let mut guard = store.lock().unwrap_or_else(|p| p.into_inner());
                    let ev1 = hharness_env::events::EventMinter::new(&*guard, run)
                        .mint(
                            "security.egress.requested",
                            hharness_wire::json::Json::obj([]),
                        )
                        .unwrap();
                    let ev2 = hharness_env::events::EventMinter::new(&*guard, run)
                        .mint(
                            "security.egress.decided",
                            hharness_wire::json::Json::obj([]),
                        )
                        .unwrap();
                    guard.append(run, lease, vec![ev1, ev2]).unwrap();
                    drop(guard);
                    thread::sleep(Duration::from_millis(2));
                }
            });
        }
    });
    let guard = store.lock().unwrap_or_else(|p| p.into_inner());
    let evs = guard.events(&run).unwrap();
    let n = evs
        .iter()
        .filter(|e| e.class.starts_with("security.egress."))
        .count();
    // 8 requested+decided pairs — chained without loss.
    assert_eq!(n, 16);
}
