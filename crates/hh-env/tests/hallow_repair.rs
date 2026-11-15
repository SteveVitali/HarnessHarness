//! The Wc6 H-allow repair battery (runner_infeasible:allowlist_gap{unknown}
//! on every mediated slot): `LocalHttpTransport::forward` used to relay the
//! client's headers verbatim after its own `Host:`/`Connection:` pair —
//! every absolute-form request carrying a client `Host:` reached the
//! origin with **two** `Host:` headers, which RFC 7230 §5.4 requires the
//! origin to answer `400 Bad Request`. apt's index and `.deb` fetches all
//! carried a `Host:` header, so the verifier's `apt-get install -y curl`
//! bootstrap could never fetch through the mediator (observed live:
//! `E: Failed to fetch … 400 Bad Request [IP: 127.0.0.1 3128]`), `uvx`
//! never landed, and `test.sh` fell through to `echo 0` — with zero
//! denied hosts because every request was *allowed*.
//!
//! These legs pin the repaired wire shape: exactly one `Host:`, no
//! hop-by-hop/authority headers forwarded, end-to-end headers preserved —
//! plus the `first_pass` verify grant for github's release-asset mirror
//! (`release-assets.githubusercontent.com`, the redirect host a `github.com`
//! grant can never reach without it — the next denial behind the 400s).

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use hharness_env::egress::{EgressTransport, LocalHttpTransport};
use hharness_env::egress_allowlist::{Allowlist, ImageFamily, VERIFY_MIRROR_HOSTS};

/// Serve one captured request: record the verbatim head bytes, answer a
/// minimal 200. Returns the raw head text.
fn serve_once(listener: TcpListener) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        let mut r = BufReader::new(s);
        let mut head = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = r.read_line(&mut line).unwrap();
            head.push_str(&line);
            if n == 0 || line == "\r\n" {
                break;
            }
        }
        let mut w = r.into_inner();
        let _ = w.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        head
    })
}

#[test]
fn forward_writes_one_host_and_no_hop_by_hop() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = serve_once(listener);
    let t = LocalHttpTransport::default();
    let client_headers: Vec<(String, String)> = [
        ("Host", "archive.ubuntu.com"),
        ("Connection", "keep-alive"),
        ("Proxy-Connection", "keep-alive"),
        ("Content-Length", "0"),
        ("User-Agent", "Debian APT-HTTP/1.3 (2.7.7)"),
        ("X-Custom", "present"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let resp = t
        .forward(
            "127.0.0.1".parse().unwrap(),
            port,
            "archive.ubuntu.com",
            "GET",
            "/ubuntu/dists/noble/InRelease",
            &client_headers,
            None,
        )
        .unwrap();
    assert_eq!(resp.status, 200);
    let head = server.join().unwrap();
    let hosts: Vec<&str> = head
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("host:"))
        .collect();
    assert_eq!(hosts.len(), 1, "exactly one Host header, got: {head}");
    assert_eq!(hosts[0], format!("Host: archive.ubuntu.com:{port}"));
    let lower = head.to_ascii_lowercase();
    for banned in [
        "proxy-connection:",
        "keep-alive:",
        "transfer-encoding:",
        "upgrade:",
        "proxy-authorization:",
    ] {
        assert!(
            !lower.contains(banned),
            "{banned} must not reach the origin: {head}"
        );
    }
    let cl: Vec<&str> = head
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .collect();
    assert_eq!(cl.len(), 1, "exactly one Content-Length, got: {head}");
    assert!(head.contains("User-Agent: Debian APT-HTTP/1.3 (2.7.7)"));
    assert!(head.contains("X-Custom: present"));
    assert!(head.contains("Connection: close"));
}

#[test]
fn first_pass_verify_grants_release_assets_mirror() {
    let list = Allowlist::first_pass(ImageFamily::Ubuntu, BTreeSet::new(), &[], vec![]);
    for h in VERIFY_MIRROR_HOSTS {
        assert!(
            list.verify.contains(h),
            "verify base must cover {h} (github release-asset redirect)"
        );
        // The mirror grant is verify-phase only — never an agent grant.
        assert!(!list.agent.contains(h));
    }
}
