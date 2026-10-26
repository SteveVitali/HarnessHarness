//! R4.7a acceptance battery — the WS-H3 credential mediator's real
//! exchange legs (spec §7.3; DF-S4.11-2; the 2026-10-20 "GitHub OAuth
//! App" ruling; ADR-0351 D5's fired trigger):
//!
//! - **oauth** — the provider exchange over the one HTTPS stack
//!   (`hharness_wire::http` atop the admitted TLS client): the device-flow
//!   branches (`authorization_pending`, `slow_down`, `access_denied`,
//!   `expired_token`, grant-window expiry) against a loopback issuer
//!   fixture, the bearer→subject verify a declared `issuer_ref` runs,
//!   and the mediator's typed `401`s — a forged bearer is
//!   `credential_invalid`, an unreachable issuer `issuer_unreachable`,
//!   an undeclared `issuer_ref` keeps the kernel `minted_scoped` leg;
//! - **mtls** — the Lab HTTP surface behind `ServerTls` requiring
//!   client certs (the committed test PKI): the handshake's verified
//!   `PeerIdentity` resolves through the declared subject→binding map;
//!   no cert / wrong CA / unmapped subject refuse typed — never a
//!   guessed subject.
//!
//! Each test fails if the behaviour is removed.

#![allow(clippy::unwrap_used)]

use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use hharness_embed::service::{
    ClientTls, EmbedService, PeerIdentity, ServerTls, ServiceConfig, TlsStream,
};
use hharness_mcp_lab::binding::{
    stdio_launch_binding, CallerBinding, CallerCredential, CallerKind, CredentialRefusal,
    SubjectKind,
};
use hharness_mcp_lab::oauth::{
    self, BearerVerdict, DeviceFlow, OAuthIssuer, PollOutcome, Transport,
};
use hharness_mcp_lab::server::LabServer;

// ── store/service + binding fixtures ─────────────────────────────────

fn service() -> EmbedService {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-r47a-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    EmbedService::open(ServiceConfig {
        store_root: d.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: d.join("ws"),
        holder: "r4-7a-lab".into(),
    })
    .unwrap()
}

fn lab() -> LabServer {
    LabServer::with_default_exposure(service())
}

fn oauth_binding(binding_id: &str, issuer_ref: &str) -> CallerBinding {
    let mut b = stdio_launch_binding("test-agent", CallerKind::Agent);
    b.binding_id = binding_id.to_string();
    b.credential = CallerCredential::OAuth {
        issuer_ref: issuer_ref.to_string(),
        subject_kind: SubjectKind::User,
        audience: "mcp://lab".to_string(),
    };
    b
}

fn mtls_binding(binding_id: &str) -> CallerBinding {
    let mut b = stdio_launch_binding("test-agent", CallerKind::Agent);
    b.binding_id = binding_id.to_string();
    b.credential = CallerCredential::Mtls {
        cert_ref: "cert:test-client".to_string(),
    };
    b
}

// ── the loopback issuer fixture (the hermetic GitHub) ────────────────

/// The scripted issuer: `/dc` hands out device grants; `/tok` counts
/// polls per `device_code` and walks a per-grant script
/// (`pending×k → slow_down×1 → grant|deny|expire`); `/user` resolves
/// issued tokens to `{"login":"fixture-user"}` and 401s the rest.
struct Issuer {
    /// `device_code → remaining scripted answers` (front = next poll).
    scripts: Mutex<std::collections::BTreeMap<String, Vec<String>>>,
    /// Tokens the fixture actually issued (forgery detection).
    issued: Mutex<std::collections::BTreeSet<String>>,
}

fn issuer() -> (u16, std::sync::Arc<Issuer>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let st = std::sync::Arc::new(Issuer {
        scripts: Mutex::new(std::collections::BTreeMap::new()),
        issued: Mutex::new(std::collections::BTreeSet::new()),
    });
    let st2 = st.clone();
    std::thread::spawn(move || loop {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let st = st2.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(&mut sock);
            let Ok(Some((head, _))) = hharness_wire::http::read_request(&mut r, 64 * 1024) else {
                return;
            };
            let path = hharness_wire::http::split_target(&head.target).0;
            let (status, body) = match (head.method.as_str(), path) {
                ("POST", "/dc") => (
                    200,
                    "{\"device_code\":\"dc-1\",\"user_code\":\"FXTR-0001\",\
                     \"verification_uri\":\"https://fixture/device\",\
                     \"interval\":5,\"expires_in\":900}"
                        .to_string(),
                ),
                ("POST", "/tok") => {
                    let mut sc = st.scripts.lock().unwrap();
                    let ans = sc
                        .get_mut("dc-1")
                        .and_then(|v| {
                            if v.is_empty() {
                                None
                            } else {
                                Some(v.remove(0))
                            }
                        })
                        .unwrap_or_else(|| "pending".to_string());
                    match ans.as_str() {
                        "grant" => {
                            st.issued.lock().unwrap().insert("tok-1".to_string());
                            (200, "{\"access_token\":\"tok-1\"}".to_string())
                        }
                        "deny" => (200, "{\"error\":\"access_denied\"}".to_string()),
                        "expire" => (200, "{\"error\":\"expired_token\"}".to_string()),
                        "slow" => (200, "{\"error\":\"slow_down\"}".to_string()),
                        _ => (200, "{\"error\":\"authorization_pending\"}".to_string()),
                    }
                }
                ("GET", "/user") => {
                    let ok = head
                        .headers
                        .get("authorization")
                        .and_then(|a| a.strip_prefix("Bearer "))
                        .map(|t| st.issued.lock().unwrap().contains(t))
                        .unwrap_or(false);
                    if ok {
                        (200, "{\"login\":\"fixture-user\"}".to_string())
                    } else {
                        (401, "{\"message\":\"bad credentials\"}".to_string())
                    }
                }
                _ => (404, "{}".to_string()),
            };
            let _ = hharness_wire::http::write_response(&mut sock, status, &[], body.as_bytes());
        });
    });
    (port, st)
}

fn fixture_flow(port: u16) -> DeviceFlow {
    DeviceFlow {
        device_code_url: format!("http://127.0.0.1:{port}/dc"),
        token_url: format!("http://127.0.0.1:{port}/tok"),
        user_url: format!("http://127.0.0.1:{port}/user"),
    }
}

fn issuer_of(flow: &DeviceFlow) -> OAuthIssuer {
    OAuthIssuer {
        user_url: flow.user_url.clone(),
        transport: Transport::Loopback,
    }
}

fn no_sleep() -> impl FnMut(Duration) {
    |_| {}
}

/// `poll_until` under a one-second deadline and an injected no-op sleep.
fn poll(flow: &DeviceFlow, device_code: &str) -> Result<PollOutcome, oauth::OAuthError> {
    oauth::poll_until(
        Transport::Loopback,
        flow,
        "fixture-client",
        device_code,
        5,
        Instant::now() + Duration::from_secs(1),
        &mut no_sleep(),
    )
}

// ── oauth — the device-flow branches ─────────────────────────────────

#[test]
fn device_flow_happy_path_resolves_subject() {
    let (port, st) = issuer();
    st.scripts
        .lock()
        .unwrap()
        .insert("dc-1".into(), vec!["pending".into(), "grant".into()]);
    let flow = fixture_flow(port);
    let grant = oauth::initiate(Transport::Loopback, &flow, "fixture-client").unwrap();
    assert_eq!(grant.user_code, "FXTR-0001");
    assert_eq!(grant.verification_uri, "https://fixture/device");
    assert_eq!(grant.interval_s, 5);
    let out = poll(&flow, &grant.device_code).unwrap();
    let PollOutcome::Granted(token) = out else {
        panic!("expected Granted, got {out:?}");
    };
    // The verified bearer names the fixture's subject.
    assert_eq!(
        issuer_of(&flow).verify_bearer(&token),
        BearerVerdict::Valid {
            subject: "fixture-user".to_string()
        }
    );
    // A forged bearer — never issued — is typed `Invalid`, never a
    // guessed subject.
    assert_eq!(
        issuer_of(&flow).verify_bearer("tok-forged"),
        BearerVerdict::Invalid
    );
}

#[test]
fn device_flow_slow_down_widens_then_grants() {
    let (port, st) = issuer();
    st.scripts.lock().unwrap().insert(
        "dc-1".into(),
        vec!["pending".into(), "slow".into(), "grant".into()],
    );
    let flow = fixture_flow(port);
    let grant = oauth::initiate(Transport::Loopback, &flow, "fixture-client").unwrap();
    // Record the sleeps — the slow_down arm widens the interval by the
    // mandated +5s (5 → 10 → granted).
    let mut slept: Vec<u64> = Vec::new();
    let out = oauth::poll_until(
        Transport::Loopback,
        &flow,
        "fixture-client",
        &grant.device_code,
        grant.interval_s,
        Instant::now() + Duration::from_secs(1),
        &mut |d: Duration| slept.push(d.as_secs()),
    )
    .unwrap();
    assert!(matches!(out, PollOutcome::Granted(_)), "{out:?}");
    assert_eq!(slept, vec![5, 10], "slow_down widens the interval");
}

#[test]
fn device_flow_denied_and_expired_are_terminal() {
    let (port, st) = issuer();
    let flow = fixture_flow(port);
    st.scripts
        .lock()
        .unwrap()
        .insert("dc-1".into(), vec!["deny".into()]);
    assert_eq!(
        poll(&flow, "dc-1"),
        Ok(PollOutcome::Denied("access_denied".to_string()))
    );
    st.scripts
        .lock()
        .unwrap()
        .insert("dc-1".into(), vec!["pending".into(), "expire".into()]);
    assert_eq!(poll(&flow, "dc-1"), Ok(PollOutcome::Expired));
    // The grant window itself closing before authorization — Expired.
    st.scripts
        .lock()
        .unwrap()
        .insert("dc-1".into(), vec!["pending".into()]);
    assert_eq!(
        oauth::poll_until(
            Transport::Loopback,
            &flow,
            "fixture-client",
            "dc-1",
            5,
            Instant::now(), // deadline already passed
            &mut no_sleep(),
        ),
        Ok(PollOutcome::Expired)
    );
}

#[test]
fn loopback_transport_refuses_non_loopback() {
    let u = hharness_wire::http::parse_http_url("http://github.com/login/device/code").unwrap();
    assert!(matches!(
        oauth::open(Transport::Loopback, &u),
        Err(oauth::OAuthError::Refused(_))
    ));
    // An `http` URL under the TLS transport refuses — never a silent
    // plaintext downgrade.
    assert!(matches!(
        oauth::open(Transport::Tls, &u),
        Err(oauth::OAuthError::Refused(_))
    ));
}

// ── oauth — the mediator's declared-issuer leg ───────────────────────

#[test]
fn mediator_oauth_resolves_via_declared_issuer() {
    let (port, st) = issuer();
    st.issued.lock().unwrap().insert("tok-live".to_string());
    let mut srv = lab();
    let flow = fixture_flow(port);
    srv.oauth_issuers
        .insert("issuer:fixture".to_string(), issuer_of(&flow));
    // A provider-verified bearer resolves to the granted binding — the
    // same CallerBinding fields the stdio path produces.
    srv.grant_bearer("tok-live", oauth_binding("bind-oauth-gh", "issuer:fixture"));
    let b = srv.resolve_credential("tok-live").unwrap();
    assert_eq!(b.binding_id, "bind-oauth-gh");
    assert_eq!(b.principal_ref, "principal:test");
    // A forged bearer under the same declared issuer — the issuer's own
    // 401 answers `credential_invalid`.
    srv.grant_bearer(
        "tok-forged",
        oauth_binding("bind-oauth-x", "issuer:fixture"),
    );
    assert_eq!(
        srv.resolve_credential("tok-forged").unwrap_err(),
        CredentialRefusal::CredentialInvalid
    );
    // An undeclared `issuer_ref` keeps the kernel `minted_scoped` leg —
    // the landed behaviour is untouched (kernel mint resolves; a
    // provider token under it is `credential_invalid`).
    let ktok = srv.mint_caller_token("mcp://lab", 60_000).unwrap();
    srv.grant_bearer(&ktok, oauth_binding("bind-oauth-k", "issuer:test"));
    assert_eq!(
        srv.resolve_credential(&ktok).unwrap().binding_id,
        "bind-oauth-k"
    );
    srv.grant_bearer("tok-live", oauth_binding("bind-oauth-k2", "issuer:test"));
    assert_eq!(
        srv.resolve_credential("tok-live").unwrap_err(),
        CredentialRefusal::CredentialInvalid
    );
}

#[test]
fn mediator_oauth_unreachable_issuer_is_typed() {
    let mut srv = lab();
    // A declared issuer that answers nothing — a bound port that is
    // closed immediately, guaranteeing refused dials.
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);
    srv.oauth_issuers.insert(
        "issuer:dead".to_string(),
        OAuthIssuer {
            user_url: format!("http://127.0.0.1:{port}/user"),
            transport: Transport::Loopback,
        },
    );
    srv.grant_bearer("tok-any", oauth_binding("bind-oauth-d", "issuer:dead"));
    assert_eq!(
        srv.resolve_credential("tok-any").unwrap_err(),
        CredentialRefusal::IssuerUnreachable
    );
}

// ── mtls — the handshake's peer identity → declared subject map ──────

fn pki(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../hh-tls/fixtures/pki/{name}"))
}
fn pem(name: &str) -> String {
    std::fs::read_to_string(pki(name)).unwrap()
}

#[test]
fn resolve_mtls_peer_maps_verified_subjects() {
    let mut srv = lab();
    srv.grant_mtls_subject("hh-test-client", mtls_binding("bind-mtls-1"));
    srv.grant_mtls_subject("spiffe://lab/alice", mtls_binding("bind-mtls-2"));
    // CN hit.
    let id = PeerIdentity {
        subject_common_name: Some("hh-test-client".to_string()),
        sans_dns: vec![],
        sans_uri: vec![],
    };
    assert_eq!(
        srv.resolve_mtls_peer(&id).unwrap().binding_id,
        "bind-mtls-1"
    );
    // SAN-URI fallback (no CN).
    let id2 = PeerIdentity {
        subject_common_name: None,
        sans_dns: vec![],
        sans_uri: vec!["spiffe://lab/alice".to_string()],
    };
    assert_eq!(
        srv.resolve_mtls_peer(&id2).unwrap().binding_id,
        "bind-mtls-2"
    );
    // Unmapped verified peer — `no_grant`, never a guess.
    let id3 = PeerIdentity {
        subject_common_name: Some("nobody".to_string()),
        sans_dns: vec!["nobody.test".to_string()],
        sans_uri: vec![],
    };
    assert_eq!(
        srv.resolve_mtls_peer(&id3).unwrap_err(),
        CredentialRefusal::NoGrant
    );
    // A grant naming a non-`mtls` credential — declared mismatch, the
    // transport evidence can't vouch for it: `credential_invalid`.
    srv.grant_mtls_subject("hh-test-client", {
        let mut b = oauth_binding("bind-wrong-kind", "issuer:test");
        b.credential = CallerCredential::StdioLaunch {
            launched_by: "x".into(),
            launch_event: None,
        };
        b
    });
    assert_eq!(
        srv.resolve_mtls_peer(&id).unwrap_err(),
        CredentialRefusal::CredentialInvalid
    );
}

/// Spawn the mTLS arm — the same `!Send` hand-off the r2_19 fixture
/// uses; `tls` is built inside the serve thread.
fn spawn_mtls(srv: LabServer, listener: TcpListener) {
    let srv_ptr = Box::into_raw(Box::new(srv)) as usize;
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    std::thread::spawn(move || {
        let srv = unsafe { &mut *(srv_ptr as *mut LabServer) };
        let listener = unsafe { &*(listener_ptr as *const TcpListener) };
        let tls = ServerTls::requiring_client_auth(
            &pem("server.pem"),
            &pem("server-key.pem"),
            &pem("hh-test-ca.pem"),
        )
        .unwrap();
        let origins = hharness_mcp::http::OriginPolicy::new(&[], false);
        let _ = hharness_mcp_lab::http::serve_http_mtls(srv, listener, &origins, &tls);
    });
}

fn client(identity: Option<(&str, &str)>) -> ClientTls {
    let mut c = ClientTls::webpki_roots()
        .extra_roots_file(&pki("hh-test-ca.pem"))
        .unwrap();
    if let Some((chain, key)) = identity {
        c = c.client_identity(&pem(chain), &pem(key)).unwrap();
    }
    c
}

fn dial_tls(port: u16, c: &ClientTls) -> Result<TlsStream, hharness_embed::service::TlsError> {
    let sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    sock.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    c.connect(sock, "localhost")
}

/// POST `/mcp` over the TLS stream; the full response string.
fn mtls_post(st: &mut TlsStream, body: &str) -> String {
    hharness_wire::http::write_request(
        st,
        "POST",
        "/mcp",
        &[("Host", "localhost"), ("Content-Type", "application/json")],
        body.as_bytes(),
    )
    .unwrap();
    let mut r = BufReader::new(st);
    let (head, raw) = hharness_wire::http::read_response_full(&mut r, 64 * 1024).unwrap();
    format!(
        "HTTP/1.1 {}\r\n\r\n{}",
        head.status,
        String::from_utf8_lossy(&raw)
    )
}

#[test]
fn mtls_mapped_subject_resolves_and_serves() {
    let mut srv = lab();
    srv.grant_mtls_subject("hh-test-client", mtls_binding("bind-mtls-srv"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    spawn_mtls(srv, listener);

    let mut st = dial_tls(port, &client(Some(("client.pem", "client-key.pem")))).unwrap();
    // The handshake's verified identity IS the credential — a POST
    // carrying no bearer still resolves to the mapped binding.
    let init = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}";
    let r = mtls_post(&mut st, init);
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    assert!(r.contains("serverInfo"), "{r}");
}

#[test]
fn mtls_unmapped_subject_answers_401_over_tls() {
    let srv = lab(); // no subjects granted
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    spawn_mtls(srv, listener);

    let mut st = dial_tls(port, &client(Some(("client.pem", "client-key.pem")))).unwrap();
    // The handshake verified the cert; no declared mapping names it —
    // the `401` rides the TLS channel with the typed refusal.
    let r = mtls_post(&mut st, "{}");
    assert!(r.starts_with("HTTP/1.1 401"), "{r}");
    assert!(r.contains("\"refusal\":\"no_grant\""), "{r}");
}

#[test]
fn mtls_missing_or_foreign_cert_refused_at_handshake() {
    let mut srv = lab();
    srv.grant_mtls_subject("hh-test-client", mtls_binding("bind-mtls-srv"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    spawn_mtls(srv, listener);

    // No client certificate — the verifier refuses the handshake;
    // the alert may surface at connect or on first IO.
    match dial_tls(port, &client(None)) {
        Err(_) => {}
        Ok(mut st) => {
            let r = hharness_wire::http::write_request(&mut st, "GET", "/", &[], &[]);
            let refused = r.is_err() || {
                let mut r = BufReader::new(&mut st);
                hharness_wire::http::read_response_full(&mut r, 1024).is_err()
            };
            assert!(refused, "a certificate-less peer got a usable session");
        }
    }

    // A client chain under the rogue CA — likewise a typed refusal.
    match dial_tls(
        port,
        &client(Some(("rogue-server.pem", "rogue-server-key.pem"))),
    ) {
        Err(_) => {}
        Ok(mut st) => {
            let r = hharness_wire::http::write_request(&mut st, "GET", "/", &[], &[]);
            let refused = r.is_err() || {
                let mut r = BufReader::new(&mut st);
                hharness_wire::http::read_response_full(&mut r, 1024).is_err()
            };
            assert!(refused, "a rogue-CA peer got a usable session");
        }
    }
}
