//! R4.5c acceptance battery — the admitted rustls stack end-to-end over
//! the committed TEST-ONLY PKI (`fixtures/pki/`; `scripts/gen-test-pki.sh`;
//! ADR-0365). Every leg names the invariant it pins; each fails if the
//! behaviour is removed:
//!
//! - a real TLS session carries the `hharness_wire::http` codec (the ADR-0364
//!   mount: one codec, TLS beneath);
//! - the local-CA hook admits an explicitly named roots file — never an
//!   implicit one;
//! - untrusted issuer / wrong SAN / missing client cert are typed
//!   refusals — never silently-accepted sessions;
//! - mTLS presents the client cert and the server extracts the verified
//!   peer identity (subject CN + SAN) — the R4.7a mediator's input.

use std::io::BufReader;
use std::net::TcpStream;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Duration;

use hharness_tls::{ClientTls, PeerIdentity, ServerTls, ServerTlsStream, TlsError, TlsStream};

fn pki(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/pki/{name}"))
}

fn pem(name: &str) -> String {
    std::fs::read_to_string(pki(name)).unwrap()
}

fn client() -> ClientTls {
    ClientTls::webpki_roots()
        .extra_roots_file(&pki("hh-test-ca.pem"))
        .unwrap()
}

/// `respond_ok` — the fixture's one-shot reply: read the request (a
/// server that closes with unread peer data earns the client a TCP RST —
/// the fixture drains before it answers), write the response.
fn respond_ok(s: &mut ServerTlsStream) {
    let mut r = BufReader::new(&mut *s);
    let _ = hharness_wire::http::read_request(&mut r, 64 * 1024);
    hharness_wire::http::write_response(&mut *s, 200, &[], b"ok").unwrap();
}

/// Serve one TLS-terminated request: `f` receives the established
/// `ServerTlsStream` (handshake already complete) plus, on handshake
/// failure, nothing — the refusal is returned through the join handle.
fn serve(
    tls: ServerTls,
    f: impl FnOnce(&mut ServerTlsStream) + Send + 'static,
) -> (u16, JoinHandle<Result<(), String>>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (sock, _) = l.accept().map_err(|e| format!("accept: {e}"))?;
        sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
        sock.set_write_timeout(Some(Duration::from_secs(5))).ok();
        let mut stream = tls.accept(sock).map_err(|e| format!("tls accept: {e}"))?;
        f(&mut stream);
        Ok(())
    });
    (port, h)
}

fn dial(port: u16) -> TcpStream {
    let s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    s
}

/// One HTTP/1.1 GET over the TLS stream through the ONE codec.
/// `unwrap_stream` — a typed refusal or the stream (unwrap_err needs no
/// `Debug` on the Ok half this way).
fn expect_stream(r: Result<TlsStream, TlsError>) -> TlsStream {
    match r {
        Ok(s) => s,
        Err(e) => panic!("expected a TLS session, got refusal {e:?}"),
    }
}

/// `expect_refusal` — the refusal or panic (never a session).
fn expect_refusal(r: Result<TlsStream, TlsError>) -> TlsError {
    match r {
        Err(e) => e,
        Ok(_) => panic!("expected a typed refusal, got a TLS session"),
    }
}

fn get(stream: &mut TlsStream, path: &str) -> (u16, Vec<u8>) {
    hharness_wire::http::write_request(stream, "GET", path, &[("Host", "localhost")], &[]).unwrap();
    let mut r = BufReader::new(stream);
    let (head, body) = hharness_wire::http::read_response_full(&mut r, 64 * 1024).unwrap();
    (head.status, body)
}

/// (a) The happy path — a real TLS session carries the codec, the
/// client-side `peer_identity` reports the verified server's CN+SAN.
#[test]
fn tls_get_roundtrip_over_one_codec() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (port, join) = serve(tls, |s| {
        let mut r = BufReader::new(&mut *s);
        let (head, _body) = hharness_wire::http::read_request(&mut r, 64 * 1024)
            .unwrap()
            .unwrap();
        assert_eq!(head.method, "GET");
        hharness_wire::http::write_response(&mut *s, 200, &[("X-Test", "tls")], b"ok").unwrap();
    });

    let mut stream = expect_stream(client().connect(dial(port), "localhost"));
    let id = stream.peer_identity().unwrap();
    assert_eq!(id.subject_common_name.as_deref(), Some("hh-test-server"));
    assert!(id.sans_dns.contains(&"hh.test".to_string()));
    assert!(id.sans_dns.contains(&"localhost".to_string()));

    let (status, body) = get(&mut stream, "/data");
    assert_eq!(status, 200);
    assert_eq!(body, b"ok");
    join.join().unwrap().unwrap();
}

/// The IP-SAN arm — dialing 127.0.0.1 with server_name `127.0.0.1`
/// verifies against the cert's `IP:127.0.0.1` SAN.
#[test]
fn tls_ip_san_verifies() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (port, join) = serve(tls, respond_ok);
    let mut stream = expect_stream(client().connect(dial(port), "127.0.0.1"));
    let (status, _) = get(&mut stream, "/");
    assert_eq!(status, 200);
    join.join().unwrap().unwrap();
}

/// (b) Untrusted issuer — a chain under `rogue-ca` is refused typed,
/// never silently accepted (the root set's boundary is the refusal).
#[test]
fn untrusted_issuer_refused_typed() {
    let tls = ServerTls::new(&pem("rogue-server.pem"), &pem("rogue-server-key.pem")).unwrap();
    let (port, join) = serve(tls, |_| {});
    let err = expect_refusal(client().connect(dial(port), "localhost"));
    assert!(
        matches!(err, TlsError::Handshake(_)),
        "expected Handshake refusal, got {err:?}"
    );
    let _ = join.join();
}

/// (c) Wrong SAN — a CA-signed cert for `other.example` is refused when
/// the dialed name is `localhost` (name verification is always on).
#[test]
fn wrong_san_refused_typed() {
    let tls = ServerTls::new(&pem("wrongname.pem"), &pem("wrongname-key.pem")).unwrap();
    let (port, join) = serve(tls, |_| {});
    let err = expect_refusal(client().connect(dial(port), "localhost"));
    assert!(
        matches!(err, TlsError::Handshake(_)),
        "expected Handshake refusal, got {err:?}"
    );
    let _ = join.join();
}

/// (d) mTLS — the client presents its certificate; the server extracts
/// the *verified* peer identity (`CN=hh-test-client`, SAN
/// `hh-client.test`) — the R4.7a mediator's peer-identity input.
#[test]
fn mtls_peer_identity_extracted() {
    let tls = ServerTls::requiring_client_auth(
        &pem("server.pem"),
        &pem("server-key.pem"),
        &pem("hh-test-ca.pem"),
    )
    .unwrap();
    let (port, join) = serve(tls, |s| {
        let id: PeerIdentity = s.peer_identity().expect("verified client chain");
        assert_eq!(id.subject_common_name.as_deref(), Some("hh-test-client"));
        assert_eq!(id.sans_dns, vec!["hh-client.test".to_string()]);
        respond_ok(s);
    });

    let c = client()
        .client_identity(&pem("client.pem"), &pem("client-key.pem"))
        .unwrap();
    let mut stream = expect_stream(c.connect(dial(port), "localhost"));
    let (status, _) = get(&mut stream, "/");
    assert_eq!(status, 200);
    join.join().unwrap().unwrap();
}

/// (e) Missing client cert — the verifier refuses the handshake (typed;
/// no unauthenticated peer is ever admitted to the stream).
#[test]
fn missing_client_cert_refused() {
    let tls = ServerTls::requiring_client_auth(
        &pem("server.pem"),
        &pem("server-key.pem"),
        &pem("hh-test-ca.pem"),
    )
    .unwrap();
    let (port, join) = serve(tls, |_| {
        panic!("the server must never yield a stream for an unauthenticated peer")
    });

    // The client's own connect may surface the alert mid-handshake or on
    // the first write — either is a typed refusal, never a session.
    let res = client().connect(dial(port), "localhost");
    match res {
        Err(TlsError::Handshake(_)) | Err(TlsError::Io(_)) => {}
        Err(other) => panic!("unexpected refusal class: {other:?}"),
        Ok(mut stream) => {
            let r = hharness_wire::http::write_request(&mut stream, "GET", "/", &[], &[]);
            let refused = r.is_err() || {
                let mut r = BufReader::new(&mut stream);
                hharness_wire::http::read_response_full(&mut r, 1024).is_err()
            };
            assert!(refused, "an unauthenticated peer got a usable session");
        }
    }
    let srv = join.join().unwrap();
    assert!(
        srv.is_err(),
        "server must refuse the unauthenticated peer: {srv:?}"
    );
}

/// (f) The extra-roots hook is explicit-only: the default webpki client
/// (no extra roots) refuses the test CA's chain — the hook is the only
/// path to a private issuer.
#[test]
fn extra_roots_explicit_only() {
    let tls = ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap();
    let (port, join) = serve(tls, |_| {});
    let err = expect_refusal(ClientTls::webpki_roots().connect(dial(port), "localhost"));
    assert!(matches!(err, TlsError::Handshake(_)), "got {err:?}");
    let _ = join.join();

    let (port2, join2) = serve(
        ServerTls::new(&pem("server.pem"), &pem("server-key.pem")).unwrap(),
        respond_ok,
    );
    let mut stream = expect_stream(client().connect(dial(port2), "localhost"));
    assert_eq!(get(&mut stream, "/").0, 200);
    join2.join().unwrap().unwrap();
}

/// (g) A roots file with no CERTIFICATE blocks is a typed `BadPem` —
/// never a silently-smaller root set.
#[test]
fn empty_roots_file_refused() {
    let f = std::env::temp_dir().join(format!("hh-tls-empty-{}.pem", std::process::id()));
    std::fs::write(&f, "# nothing here\n").unwrap();
    let err = match ClientTls::webpki_roots().extra_roots_file(&f) {
        Err(e) => e,
        Ok(_) => panic!("expected BadPem refusal"),
    };
    assert!(matches!(err, TlsError::BadPem(_)), "got {err:?}");
    std::fs::remove_file(&f).ok();
}
