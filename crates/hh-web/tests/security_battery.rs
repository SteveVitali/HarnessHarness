//! AC-R-2.11.2-6 — the full security battery (§7.2; not deferrable).
//! Each case drives the complete `serve_request` pipeline (gate → route
//! → sink/scrub) on a parsed head/body, plus the socket-level checks the
//! head/body path cannot express (startup refusal, response framing).
//!
//! Legs: (a) foreign Host → 400 · (b) disallowed/null/cross-site Origin
//! → 403 · (c) missing/invalid token → 401 even on loopback ·
//! (d) `?token=` refused · (e) simple-form POST → 403 ·
//! (f) off-loopback bind without auth+origins refuses to start ·
//! (g) LT-01…12 canary values leak through no response ·
//! (h) framing/hardening headers present · (i) DNS-rebind fails at (a).

mod common;

use common::*;
use hh_web::gate::GateConfig;
use hh_web::server::{serve_n, serve_request, Served};
use hh_wire::json::Json;

fn status_of(s: &Served) -> u16 {
    match s {
        Served::Json(c, _) => *c,
        Served::Bare(c) => *c,
        Served::Static(..) => 200,
    }
}

// (a) — a foreign Host refuses before anything else, on every route.
#[test]
fn ac6a_foreign_host_is_400() {
    let (gate, mut svc, det) = fixture(None);
    for target in ["/api", "/", "/anything"] {
        let mut h = api_head();
        h.target = target.into();
        if target == "/" {
            h.method = "GET".into();
        }
        h.headers.insert("host".into(), "evil.example.com".into());
        let s = serve_request(&gate, &mut svc, &det, &h, &[]);
        assert_eq!(status_of(&s), 400, "foreign host on {target}");
    }
}

// (b) — a disallowed / null / cross-site Origin refuses 403; the
// surface's own origin passes.
#[test]
fn ac6b_origin_rules() {
    let (gate, mut svc, det) = fixture(None);
    for (origin, want) in [
        ("https://evil.example.com", 403u16),
        ("null", 403),
        // Own origin passes P3 — the dressed api_head carries a valid
        // token, so the request reaches decode (empty body → bad_json).
        ("http://127.0.0.1:7777", 400),
    ] {
        let mut h = api_head();
        h.headers.insert("origin".into(), origin.into());
        let s = serve_request(&gate, &mut svc, &det, &h, &[]);
        assert_eq!(status_of(&s), want, "origin {origin}");
    }
    // Cross-site fetch metadata refuses outright.
    for site in ["cross-site", "same-site"] {
        let mut h = api_head();
        h.headers.insert("sec-fetch-site".into(), site.into());
        let s = serve_request(&gate, &mut svc, &det, &h, &[]);
        assert_eq!(status_of(&s), 403, "sec-fetch-site {site}");
    }
}

// (c) — missing and invalid tokens refuse 401 even on loopback; the
// refusal carries no diagnostic detail (P2).
#[test]
fn ac6c_token_required_even_on_loopback() {
    let (gate, mut svc, det) = fixture(None);
    for auth in [
        None,
        Some("Bearer wrong-token"),
        Some("bearer surface-token-32-hex-chars-0000000"), // wrong scheme case
        Some("Token surface-token-32-hex-chars-0000000"),  // wrong scheme
    ] {
        let mut h = api_head();
        match auth {
            Some(a) => {
                h.headers.insert("authorization".into(), a.into());
            }
            None => {
                h.headers.remove("authorization");
            }
        }
        // A well-formed envelope so the *token* leg is what refuses.
        let s = serve_request(
            &gate,
            &mut svc,
            &det,
            &h,
            &api_body("run_index", Json::Null),
        );
        assert_eq!(status_of(&s), 401, "auth {auth:?}");
        if let Served::Json(_, body) = &s {
            assert!(!body.contains("token"), "refusal leaks detail: {body}");
        }
    }
    // The right token passes the gate leg (the op then fails transport
    // to the dead kernel — a 200 envelope carrying the typed error).
    let h = api_head();
    let s = serve_request(&gate, &mut svc, &det, &h, &api_body("head", Json::Null));
    assert_eq!(status_of(&s), 200);
}

// (d) — `?token=` on the target refuses before dispatch.
#[test]
fn ac6d_query_token_refused() {
    let (gate, mut svc, det) = fixture(None);
    for target in ["/api?token=x", "/?token=abc", "/api?a=1&TOKEN=x"] {
        let mut h = api_head();
        h.target = target.into();
        if target.starts_with("/?") {
            h.method = "GET".into();
        }
        let s = serve_request(&gate, &mut svc, &det, &h, &[]);
        assert_eq!(status_of(&s), 400, "target {target}");
    }
}

// (e) — a simple HTML form cannot mint a write: urlencoded body, no
// custom headers → 403 before any body byte is interpreted.
#[test]
fn ac6e_simple_form_post_refused() {
    let (gate, mut svc, det) = fixture(None);
    let mut h = head(
        "POST",
        "/api",
        &[
            ("authorization", "Bearer surface-token-32-hex-chars-0000000"),
            ("content-type", "application/x-www-form-urlencoded"),
            ("sec-fetch-site", "same-origin"),
            ("origin", "http://127.0.0.1:7777"),
        ],
    );
    let body = b"op=respond_permission&permission_id=p1&outcome=allow".to_vec();
    let s = serve_request(&gate, &mut svc, &det, &h, &body);
    assert_eq!(status_of(&s), 403);
    // A mutating op in an otherwise-valid envelope but without the
    // script header refuses identically.
    h.headers.remove("x-hh-script");
    h.headers
        .insert("content-type".into(), "application/json".into());
    let s = serve_request(
        &gate,
        &mut svc,
        &det,
        &h,
        &api_body("respond_permission", Json::Null),
    );
    assert_eq!(status_of(&s), 403);
}

// (f) — an off-loopback/wildcard bind without trusted origins refuses
// to start (P5 — the startup check, exercised on the config).
#[test]
fn ac6f_off_loopback_requires_origins() {
    let bad = GateConfig {
        authority: "0.0.0.0:7777".into(),
        loopback: false,
        token: TOKEN.into(),
        trusted_origins: Vec::new(),
        allow_missing_fetch_metadata: false,
    };
    assert!(
        bad.validate().is_err(),
        "wildcard bind without origins must refuse"
    );
    let ok = GateConfig {
        trusted_origins: vec!["https://ops.example.com".into()],
        ..bad
    };
    assert!(ok.validate().is_ok(), "origins declared → start allowed");
}

// (g) — the LT fixture sweep: a canary value inside a served payload
// tombstones, never echoes (AC-6(g); LT-01…12's surface leg — the
// kernel masks at source, the surface sweeps at the wire).
#[test]
fn ac6g_canary_never_leaves() {
    // The dispatch path: a stub kernel whose result *contains* the
    // canary — the served body must tombstone it, never echo it.
    let (addr, _rx, _jh) = stub_kernel(
        |_m, _p| {
            Json::obj([
                ("note", Json::str("leak: lt01-secret-value-9f3e")),
                ("fine", Json::str("clean")),
            ])
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let det = {
        let mut d = det;
        d.canaries.push(hh_secrets::Canary {
            id: "lt-canary-1".into(),
            value: "lt01-secret-value-9f3e".into(),
        });
        d
    };
    let s = serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("run_index", Json::Null),
    );
    let Served::Json(200, body) = s else {
        panic!("dispatch path must serve: {:?}", status_of(&s));
    };
    assert!(
        !body.contains("lt01-secret-value-9f3e"),
        "canary left the surface: {body}"
    );
    assert!(body.contains("REDACTED"), "tombstone expected: {body}");
    assert!(body.contains("clean"));
    // The scrub is uniform — gate refusals sweep too (defence in depth).
    let (gate2, mut svc2, _det2) = fixture(None);
    let det2 = canary_detectors("lt-canary-1", "lt01-secret-value-9f3e");
    let mut h = api_head();
    h.headers.insert("host".into(), "foreign".into());
    let s = serve_request(&gate2, &mut svc2, &det2, &h, &[]);
    assert_eq!(status_of(&s), 400);
}

// (h) — the hardening header set rides every response (P7: CSP,
// frame-ancestors, referrer policy, nosniff, no-store, close).
#[test]
fn ac6h_framing_headers_present() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let bound = listener.local_addr().unwrap();
    let (mut gate, svc, det) = fixture(None);
    gate.authority = bound.to_string();
    // `serve_n` runs one connection on this thread while the client runs
    // on a spawned thread (svc is Send — plain sockets + Strings).
    let token = gate.token.clone();
    let client = std::thread::spawn(move || {
        use std::io::Write;
        let mut c = std::net::TcpStream::connect(bound).unwrap();
        let body = r#"{"op":"head","params":{}}"#;
        write!(
            c,
            "POST /api HTTP/1.1\r\nHost: {bound}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nx-hh-script: 1\r\nsec-fetch-site: same-origin\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut r = std::io::BufReader::new(c.try_clone().unwrap());
        let (head, _b) = hh_wire::http::read_response(&mut r, 1 << 20).unwrap();
        head.headers.clone()
    });
    let handled = serve_n(listener, gate, svc, det, 1).unwrap();
    assert_eq!(handled, 1);
    let headers = client.join().unwrap();
    for k in [
        "content-security-policy",
        "referrer-policy",
        "x-content-type-options",
        "cache-control",
        "connection",
    ] {
        assert!(headers.contains_key(k), "missing hardening header {k}");
    }
    let csp = &headers["content-security-policy"];
    assert!(csp.contains("frame-ancestors 'none'"), "csp: {csp}");
    assert!(csp.contains("default-src 'self'"), "csp: {csp}");
    assert_eq!(headers["cache-control"], "no-store");
}

// (i) — DNS-rebinding simulation: the request arrives at the loopback
// IP but carries a rebound Host — refused identically to (a).
#[test]
fn ac6i_dns_rebind_fails_at_host_check() {
    let (gate, mut svc, det) = fixture(None);
    let mut h = api_head();
    h.headers
        .insert("host".into(), "rebound.attacker.example".into());
    let s = serve_request(&gate, &mut svc, &det, &h, &[]);
    assert_eq!(status_of(&s), 400, "rebound host must fail the (a) leg");
}

/// P2's auxiliary legs — the public shell is unauthenticated but the
// API never is; and a wrong-path request is still gated first.
#[test]
fn aux_public_shell_and_gated_404() {
    let (gate, mut svc, det) = fixture(None);
    // The shell serves without a token (it carries no data — P2's one
    // exception), but still enforces Host/Origin.
    let h = head("GET", "/", &[]);
    let s = serve_request(&gate, &mut svc, &det, &h, &[]);
    assert!(matches!(s, Served::Static("text/html; charset=utf-8", _)));
    // An unknown path refuses 404 *after* auth — the tokenless see 401.
    let h = head("GET", "/secret", &[]);
    let s = serve_request(&gate, &mut svc, &det, &h, &[]);
    assert_eq!(status_of(&s), 401);
    let h = head(
        "GET",
        "/secret",
        &[("authorization", "Bearer surface-token-32-hex-chars-0000000")],
    );
    let s = serve_request(&gate, &mut svc, &det, &h, &[]);
    assert_eq!(status_of(&s), 404);
}
