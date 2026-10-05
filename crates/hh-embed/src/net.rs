//! Binding (c) — the `local_network` transport (§7.2; ADR-0301 D1).
//!
//! JSON-RPC 2.0 over loopback HTTP/1.1: `POST /hh-embed/1` carries one
//! canonical request and returns the canonical response (the same
//! `EmbedService::handle` — byte-identical envelopes by construction);
//! `GET /hh-embed/1/events[?wait_ms=]` drains the pending notification
//! queue as NDJSON (one `{jsonrpc, method, params}` per line — durable
//! `stream.frame` deliveries and `upcall.*` asks; the ephemeral/durable
//! split is the frame's own `delivery` member, never the transport).
//!
//! The profile is the loopback end of the §7.2 P1–P9 posture, shared with
//! `hh-web`'s gate vocabulary (CC1):
//!
//! - **loopback only** — [`serve_net`] refuses a non-loopback bound
//!   socket outright (P5/OQ-401: wider exposure is a separate binding
//!   decision, not a flag);
//! - **bearer capability** — every request needs
//!   `Authorization: Bearer <token>`, compared constant-time; missing or
//!   wrong → `401` with `WWW-Authenticate` and no diagnostic detail (P2);
//! - **exact Host** — `Host` must equal the bound authority
//!   (`127.0.0.1:<port>` or `[::1]:<port>`) → `400` (P3/DNS-rebinding);
//! - **Origin refused** — any `Origin`/`Sec-Fetch-*` header → `403` (the
//!   binding speaks to the generated client and the surface, never to a
//!   browser origin; a cross-site fetch's metadata is a refusal signal,
//!   not a CORS negotiation — P3/P4);
//! - **no query tokens** — `?token=` (any case) on the target → `400`
//!   (P9 — a capability never rides a URL).
//!
//! Concurrency is deliberately serial: `EmbedService` is not `Sync` (the
//! control driver holds boxed ports), and the canonical contract's
//! result order is the append order — one connection is handled at a
//! time on the accept thread. The events endpoint's `wait_ms` is the
//! only stall the loop admits; callers keep it small and re-poll (the
//! surface relays with its own cadence).

use std::io::{BufReader, BufWriter, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use hh_wire::http::{read_request, split_target, write_response, HttpError};
use hh_wire::json::Json;
use hh_wire::jsonrpc::{err_response, parse_request};

use crate::service::EmbedService;

/// The RPC path — `POST` only.
pub const RPC_PATH: &str = "/hh-embed/1";
/// The notification drain path — `GET` only.
pub const EVENTS_PATH: &str = "/hh-embed/1/events";
/// Maximum request body — canonical requests can carry context blocks;
/// 64 MiB is generous and bounded (P7's hardening is useless without it).
pub const NET_MAX_BODY: usize = 64 * 1024 * 1024;
/// The notification-drain wait cap (a `wait_ms` query member is clamped
/// here — a serial loop must not park on one client).
pub const EVENTS_MAX_WAIT_MS: u64 = 5_000;

/// Whether a socket address is loopback (`127.0.0.0/8` or `::1`).
pub fn is_loopback(addr: &std::net::SocketAddr) -> bool {
    addr.ip().is_loopback()
}

/// The `Authorization: Bearer` acceptance check — constant-time over the
/// padded bytes so the compare does not leak length or prefix (P2).
pub fn token_matches(expected: &str, presented: &str) -> bool {
    let e = expected.as_bytes();
    let p = presented.as_bytes();
    // Fold the length mismatch into the accumulator — comparing
    // `max(len)` bytes with out-of-range reads mapped to a fixed
    // nonmatching byte keeps the scan length-independent.
    let n = e.len().max(p.len());
    let mut acc = (e.len() ^ p.len()) as u8;
    for i in 0..n {
        let a = e.get(i).copied().unwrap_or(0xFF);
        let b = p.get(i).copied().unwrap_or(0x00);
        acc |= a ^ b;
    }
    acc == 0
}

/// Mint a 128-bit capability token (P2 — the kernel's id-source
/// discipline doesn't cover a per-process secret; `/dev/urandom` is the
/// std-only portable seam and the generation is one-shot per server
/// start).
pub fn mint_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The headers every response carries (P7 — even the RPC binding gets
/// the no-cache/no-referrer posture so a misrouted browser can never
/// cache or sniff the body).
fn base_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
        ("Connection", "close"),
    ]
}

fn respond(
    w: &mut impl Write,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), HttpError> {
    let mut h = base_headers();
    h.push((
        "Content-Type",
        match content_type {
            "json" => "application/json",
            "ndjson" => "application/x-ndjson",
            _ => "text/plain",
        },
    ));
    if status == 401 {
        h.push(("WWW-Authenticate", "Bearer"));
    }
    write_response(w, status, &h, body)
}

/// The P2/P3/P4/P9 request gate. `authority` is the bound `Host` value
/// (`127.0.0.1:port` / `[::1]:port`). Returns the status to emit when
/// the request is refused (the caller writes a bare error body).
fn gate(head: &hh_wire::http::RequestHead, authority: &str, token: &str) -> Result<(), u16> {
    // P9 — a token on the target is refused before anything reads it.
    if let Some(q) = split_target(&head.target).1 {
        let ql = q.to_ascii_lowercase();
        if ql.split('&').any(|kv| kv.starts_with("token=")) {
            return Err(400);
        }
    }
    // P3 — the Host must be exactly the bound authority (a DNS-rebind
    // arrives at 127.0.0.1 with a foreign Host).
    match head.headers.get("host") {
        Some(h) if h == authority => {}
        _ => return Err(400),
    }
    // P3/P4 — no browser-origin metadata is ever legitimate on this
    // binding (cross-site fetch metadata present ⇒ refuse).
    if head.headers.contains_key("origin")
        || head.headers.keys().any(|k| k.starts_with("sec-fetch-"))
    {
        return Err(403);
    }
    // P2 — the bearer, constant-time, no detail on failure.
    let presented = head
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !token_matches(token, presented) {
        return Err(401);
    }
    Ok(())
}

/// One connection: read → gate → dispatch → respond → close.
fn connection(stream: TcpStream, svc: &mut EmbedService, authority: &str, token: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let mut r = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut w = BufWriter::new(stream);
    let req = match read_request(&mut r, NET_MAX_BODY) {
        Ok(Some(r)) => r,
        _ => return, // clean close or malformed head — nothing to say
    };
    let (head, body) = req;
    if let Err(status) = gate(&head, authority, token) {
        let _ = respond(&mut w, status, "text", b"");
        return;
    }
    let (path, query) = split_target(&head.target);
    match (head.method.as_str(), path) {
        ("POST", RPC_PATH) => {
            // A framing failure lowers to the closed `SchemaViolation`
            // sum exactly as binding (b) does (ADR-0178 D2 — never a
            // transport error).
            let text = String::from_utf8_lossy(&body);
            let resp = match parse_request(&text) {
                Ok(req) => svc.handle(&req).to_canonical_string(),
                Err(e) => {
                    let err = hh_embed_schema::errors::EmbedError::SchemaViolation {
                        path: "/".to_string(),
                        code: format!("framing:{e}"),
                    };
                    err_response(Json::Null, err.code(), err.kind(), err.to_data_json())
                        .to_canonical_string()
                }
            };
            let _ = respond(&mut w, 200, "json", resp.as_bytes());
        }
        ("GET", EVENTS_PATH) => {
            let wait_ms = query
                .and_then(|q| {
                    q.split('&')
                        .find_map(|kv| kv.strip_prefix("wait_ms="))
                        .and_then(|v| v.parse::<u64>().ok())
                })
                .unwrap_or(0)
                .min(EVENTS_MAX_WAIT_MS);
            let deadline = Instant::now() + Duration::from_millis(wait_ms);
            let mut out = String::new();
            loop {
                let frames = svc.drain_notifications();
                for f in &frames {
                    out.push_str(&f.to_canonical_string());
                    out.push('\n');
                }
                if !frames.is_empty() || Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = respond(&mut w, 200, "ndjson", out.as_bytes());
        }
        _ => {
            let _ = respond(&mut w, 404, "text", b"");
        }
    }
}

/// Serve binding (c) on `listener` until it errors or the process ends.
/// `listener` MUST be loopback-bound — anything else is `Err` before the
/// first accept (P5/OQ-401). Serial by design (see the module doc); the
/// service stamps `binding: local_network` on session rows.
pub fn serve_net(
    listener: TcpListener,
    svc: &mut EmbedService,
    token: &str,
) -> std::io::Result<()> {
    let _ = serve_net_n(listener, svc, token, usize::MAX)?;
    Ok(())
}

/// [`serve_net`] bounded to `n` connections — the integration-test
/// variant: the *client* runs on a spawned thread (plain sockets,
/// `Send`), the service stays on the caller's thread (it is not
/// `Send`), so a test drives `serve_net_n(.., n)` to completion while
/// the client makes exactly `n` requests.
pub fn serve_net_n(
    listener: TcpListener,
    svc: &mut EmbedService,
    token: &str,
    n: usize,
) -> std::io::Result<usize> {
    let local = listener.local_addr()?;
    if !is_loopback(&local) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "binding (c) refuses a non-loopback socket (P5/OQ-401)",
        ));
    }
    svc.set_binding_label("local_network");
    let authority = local.to_string();
    let mut handled = 0usize;
    // `take(n)` yields at most n connections — exits after the nth
    // without a trailing accept.
    for stream in listener.incoming().take(n).flatten() {
        connection(stream, svc, &authority, token);
        handled += 1;
    }
    Ok(handled)
}

/// The client side of binding (c) — POST one canonical request, read the
/// canonical response. Kept beside the server so the wire shape has
/// exactly one implementation (CC1); the generated client uses it.
pub fn rpc_once(
    addr: &std::net::SocketAddr,
    token: &str,
    body: &str,
) -> Result<(u16, String), HttpError> {
    let mut stream =
        TcpStream::connect(addr).map_err(|e| HttpError::Io(format!("connect: {e}")))?;
    rpc_on(&mut stream, addr, token, body)
}

/// POST on an already-connected stream (`Connection: close` — the
/// server closes after each response, so reuse is not offered at C1).
pub fn rpc_on(
    stream: &mut TcpStream,
    addr: &std::net::SocketAddr,
    token: &str,
    body: &str,
) -> Result<(u16, String), HttpError> {
    let req = format!(
        "POST {RPC_PATH} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .and_then(|_| stream.write_all(body.as_bytes()))
        .and_then(|_| stream.flush())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    let mut r = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| HttpError::Io(e.to_string()))?,
    );
    let (head, resp) = hh_wire::http::read_response(&mut r, NET_MAX_BODY)?;
    String::from_utf8(resp)
        .map(|s| (head.status, s))
        .map_err(|_| HttpError::Unsupported)
}

/// A GET drain against a running (c) server (`wait_ms` long-polls).
pub fn drain_events(
    addr: &std::net::SocketAddr,
    token: &str,
    wait_ms: u64,
) -> Result<(u16, String), HttpError> {
    let mut stream =
        TcpStream::connect(addr).map_err(|e| HttpError::Io(format!("connect: {e}")))?;
    let req = format!(
        "GET {EVENTS_PATH}?wait_ms={wait_ms} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    let mut r = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| HttpError::Io(e.to_string()))?,
    );
    let (head, resp) = hh_wire::http::read_response(&mut r, NET_MAX_BODY)?;
    String::from_utf8(resp)
        .map(|s| (head.status, s))
        .map_err(|_| HttpError::Unsupported)
}

/// The shutdown marker for tests/daemons — `serve_net`'s accept loop
/// exits on listener close (dropping the `TcpListener` is the stop
/// signal; this helper exists for the doc contract).
pub fn shutdown(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Both);
}
