//! R4.5c — the live `https-reachability` leg (ADR-0363; the R4.5c live
//! registry entry): one unauthenticated HTTPS GET to
//! `https://api.github.com/zen` over the REAL rustls client —
//! `webpki_roots()` only, full chain + name verification, the one
//! `hharness_wire::http` codec on top. USD 0, no secrets.
//!
//! `#[ignore]`d — `scripts/live-verify.sh https-reachability` drives it;
//! an unreachable network prints the honest `SKIPPED-UNREACHABLE` marker
//! the script reads (never a fabricated pass).

#![allow(clippy::unwrap_used)]

use std::io::BufReader;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh https-reachability"]
fn r45c_live_https_zen_get() {
    let host = "api.github.com";
    let addr = match format!("{host}:443").to_socket_addrs() {
        Ok(mut it) => match it.next() {
            Some(a) => a,
            None => {
                eprintln!("SKIPPED-UNREACHABLE: {host}:443 resolved empty");
                return;
            }
        },
        Err(e) => {
            eprintln!("SKIPPED-UNREACHABLE: resolve {host}: {e}");
            return;
        }
    };
    let sock = match TcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SKIPPED-UNREACHABLE: connect {addr}: {e}");
            return;
        }
    };
    sock.set_read_timeout(Some(Duration::from_secs(15))).ok();
    sock.set_write_timeout(Some(Duration::from_secs(15))).ok();
    // The handshake itself is the claim: a public chain verifies against
    // the Mozilla roots — anything else fails the test (never insecure).
    let mut s = hharness_tls::ClientTls::webpki_roots()
        .connect(sock, host)
        .expect("the public chain must verify against webpki_roots");
    hharness_wire::http::write_request(
        &mut s,
        "GET",
        "/zen",
        &[
            ("Host", host),
            ("User-Agent", "hh-tls-live-verify/1 (HarnessHarness R4.5c)"),
            ("Accept", "application/vnd.github+json"),
            ("Connection", "close"),
        ],
        &[],
    )
    .expect("request write");
    let mut r = BufReader::new(s);
    let (head, body) =
        hharness_wire::http::read_response_full(&mut r, 1024 * 1024).expect("response read");
    assert_eq!(head.status, 200, "head: {head:?} body: {body:?}");
    assert!(!body.is_empty(), "github /zen answers a non-empty body");
    eprintln!(
        "live GET https://{host}/zen → {} ({} byte(s) body: {})",
        head.status,
        body.len(),
        String::from_utf8_lossy(&body).trim()
    );
}
