//! The lab surface's Streamable HTTP arm — POST `/mcp` carries
//! JSON-RPC frames one-request/one-response (`application/json`),
//! `GET /mcp` opens the server→client SSE push channel
//! (`text/event-stream`; DF-S4.11-1), and every request runs `Origin`
//! gating through [`hh_mcp::http::OriginPolicy`] plus the
//! `Authorization: Bearer` → [`LabServer::resolve_credential`]
//! mediation (DF-S4.11-2 — the WS-H3 credential seam).
//!
//! Session contract (MCP Streamable HTTP, `2025-03-26`):
//! - `initialize` answers `200` + a fresh `Mcp-Session-Id` header;
//! - every subsequent request (POST **and** GET) carries that header —
//!   an unknown/stale id answers `404` (the client re-initializes);
//! - `GET` without a live session answers `400` — the stream is a
//!   session-owned channel, never an anonymous one.
//!
//! SSE contract (R2.19 — the deferral's stream half):
//! - `GET /mcp` answers `200` + `content-type: text/event-stream` and
//!   stays open — a single-threaded nonblocking multiplex serves the
//!   request loop and the open streams off the one `&mut LabServer`
//!   (`LabServer` is `!Send` by construction — the surface never moves
//!   to worker threads);
//! - each notification [`LabServer::push_notification`] minted — the
//!   same mint `subscriptions/listen` drains — streams as `id: <seq>` +
//!   `data: <json-rpc-notification>` to the binding the frame was
//!   scoped to (`binding_id` scoping is per-connection, never
//!   cross-caller);
//! - `Last-Event-ID: <seq>` on a reconnect replays the retained log's
//!   backlog for that binding starting after `<seq>` (no loss inside
//!   the retained window, never a duplicate); a missing header starts
//!   the stream at the log's tail — live events only.
//!
//! Auth: `Authorization: Bearer <token>` resolves through
//! [`LabServer::resolve_credential`] — the grant table plus the
//! credential-kind mediation (`oauth` verifies the kernel broker's
//! `minted_scoped` leg; `mtls` typed-refuses `transport_absent`). An
//! absent grant, a bad token, or a refused mediation answers the `401`
//! challenge carrying the typed `refusal` code — never a guess.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use hh_mcp::http::OriginPolicy;
use hh_wire::json::Json;

use crate::binding::{CallerBinding, CredentialRefusal};
use crate::server::{error_frame, LabServer, ServerEvent};

/// The served path (Streamable HTTP's single endpoint).
pub const MCP_PATH: &str = "/mcp";
/// The session header.
pub const SESSION_HEADER: &str = "mcp-session-id";
/// The SSE resume header.
pub const LAST_EVENT_ID_HEADER: &str = "last-event-id";

/// A serve failure (transport-level only).
#[derive(Debug)]
pub enum ServeHttpError {
    /// I/O.
    Io(std::io::Error),
    /// The listener failed to bind.
    Bind(String),
}

impl std::fmt::Display for ServeHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeHttpError::Io(e) => write!(f, "http io: {e}"),
            ServeHttpError::Bind(e) => write!(f, "http bind: {e}"),
        }
    }
}
impl std::error::Error for ServeHttpError {}
impl From<std::io::Error> for ServeHttpError {
    fn from(e: std::io::Error) -> Self {
        ServeHttpError::Io(e)
    }
}

/// One parsed HTTP/1.1 request — request line, headers (lowercased
/// keys), body.
struct HttpRequest {
    /// The method token (`POST`, `GET`, …).
    method: String,
    /// The request URI (path + query).
    uri: String,
    /// Lowercased header name → value.
    headers: BTreeMap<String, String>,
    /// The `content-length` body.
    body: String,
}

/// One live connection — `Read` accumulates bytes until a full request
/// buffers; `Sse` is an open event stream pushing its binding's events.
enum ConnState {
    /// Request-side: bytes accumulate into `buf` until a full head +
    /// `content-length` body parses.
    Read { buf: Vec<u8>, dead: bool },
    /// An open SSE stream — `next_seq` is the lowest log position not
    /// yet sent to `binding_id`.
    Sse { binding_id: String, next_seq: u64 },
}

/// A multiplexed connection slot.
struct Conn {
    /// The socket (nonblocking).
    stream: TcpStream,
    /// The conn's state machine.
    state: ConnState,
}

/// The request's routing verdict — an HTTP reply to write then close,
/// or an upgrade to the SSE arm.
enum Outcome {
    /// Write `response`, close the connection.
    Reply(String),
    /// `GET /mcp` accepted — open the event stream for `binding_id`.
    /// `last_id` is the parsed `Last-Event-ID` cursor (replay backlog
    /// after it) or `None` (start at the log tail — live only).
    Sse {
        /// The caller binding the stream serves.
        binding_id: String,
        /// The `Last-Event-ID` resume cursor (`None` → tail).
        last_id: Option<u64>,
    },
}

/// Parse a buffered request — `Some` when `buf` holds a complete head
/// (`\r\n\r\n`) plus the declared `content-length` body.
fn parse_request(buf: &[u8]) -> Option<HttpRequest> {
    let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let body_start = head_end + 4;
    let mut headers = BTreeMap::new();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    let body_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if buf.len() < body_start + body_len {
        return None;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let (method, uri) = if parts.len() >= 2 {
        (parts[0].to_string(), parts[1].to_string())
    } else {
        // A malformed request line — the empty method refuses 400 at
        // dispatch below.
        (String::new(), String::new())
    };
    Some(HttpRequest {
        method,
        uri,
        headers,
        body: String::from_utf8_lossy(&buf[body_start..body_start + body_len]).to_string(),
    })
}

/// Serialize an HTTP/1.1 reply.
fn reply(status: u16, reason: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\ncontent-type: application/json\r\n",
        body.len()
    );
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out
}

/// The `401` challenge — `refusal` names the mediator's typed refusal
/// (observable in the body; never a guessed identity).
fn challenge(refusal: Option<CredentialRefusal>) -> String {
    let mut body = BTreeMap::new();
    body.insert("error".to_string(), Json::str("unauthorized"));
    if let Some(r) = refusal {
        body.insert("refusal".to_string(), Json::str(r.as_str()));
    }
    reply(
        401,
        "Unauthorized",
        &[("www-authenticate", "Bearer realm=\"hh-lab\"")],
        &Json::Obj(body).to_canonical_string(),
    )
}

/// Serialize one SSE frame — `id:` carries the log position (the
/// `Last-Event-ID` resume key); `data:` is the JSON-RPC notification
/// (the `{method, params}` mint plus the `jsonrpc` member the wire
/// requires).
fn sse_frame(ev: &ServerEvent) -> String {
    let mut map = match &ev.frame {
        Json::Obj(m) => m.clone(),
        _ => BTreeMap::new(),
    };
    map.insert("jsonrpc".to_string(), Json::str("2.0"));
    format!(
        "id: {}\r\ndata: {}\r\n\r\n",
        ev.seq,
        Json::Obj(map).to_canonical_string()
    )
}

/// Route one parsed request — the shared POST dispatch plus the GET
/// SSE open. The reply (or stream-open verdict) is what the caller
/// writes.
fn handle_request(
    srv: &mut LabServer,
    req: &HttpRequest,
    origins: &OriginPolicy,
    sessions: &mut BTreeMap<String, ()>,
    next_session: &mut u64,
) -> Outcome {
    if req.method.is_empty() {
        return Outcome::Reply(reply(400, "Bad Request", &[], "{}"));
    }
    if req.uri.split('?').next() != Some(MCP_PATH) {
        return Outcome::Reply(reply(404, "Not Found", &[], "{}"));
    }
    // Origin gate — the CORS/loopback rule the deployment declares.
    if origins
        .check(req.headers.get("origin").map(String::as_str))
        .is_err()
    {
        return Outcome::Reply(reply(403, "Forbidden", &[], "{}"));
    }
    // Bearer → the WS-H3 mediation (DF-S4.11-2): the grant table plus
    // the credential-kind check. A refusal carries its typed code.
    let token = req
        .headers
        .get("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .or_else(|| {
            req.headers
                .get("authorization")
                .and_then(|h| h.strip_prefix("bearer "))
        });
    let Some(token) = token else {
        return Outcome::Reply(challenge(None));
    };
    let binding = match srv.resolve_credential(token) {
        Ok(b) => b,
        Err(refusal) => return Outcome::Reply(challenge(Some(refusal))),
    };
    match req.method.as_str() {
        "POST" => post(srv, req, &binding, sessions, next_session),
        "GET" => get(req, &binding, sessions),
        _ => Outcome::Reply(reply(405, "Method Not Allowed", &[], "{}")),
    }
}

/// `POST /mcp` — one JSON-RPC frame per request (the unchanged S4.11
/// arm: parse → session gate → `handle_message` → 200/202).
fn post(
    srv: &mut LabServer,
    req: &HttpRequest,
    binding: &CallerBinding,
    sessions: &mut BTreeMap<String, ()>,
    next_session: &mut u64,
) -> Outcome {
    let msg = match hh_wire::json::parse(&req.body) {
        Ok(m) => m,
        Err(_) => {
            return Outcome::Reply(reply(
                400,
                "Bad Request",
                &[],
                &error_frame(Json::Null, -32700, "parse_error").to_canonical_string(),
            ))
        }
    };
    let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
    // Session gate — `initialize` issues; everything else must carry.
    let session_id = req.headers.get(SESSION_HEADER).cloned();
    let mut issued: Option<String> = None;
    if method == "initialize" {
        if session_id.is_none() {
            *next_session += 1;
            let sid = format!("mcp-session-{next_session}");
            sessions.insert(sid.clone(), ());
            issued = Some(sid);
        }
    } else if let Some(sid) = &session_id {
        if !sessions.contains_key(sid) {
            return Outcome::Reply(reply(
                404,
                "Not Found",
                &[],
                "{\"error\":\"unknown session\"}",
            ));
        }
    } else {
        return Outcome::Reply(reply(
            400,
            "Bad Request",
            &[],
            "{\"error\":\"missing Mcp-Session-Id\"}",
        ));
    }
    let issued_hdr: Vec<(String, String)> = issued
        .iter()
        .map(|s| (SESSION_HEADER.to_string(), s.clone()))
        .collect();
    match srv.handle_message(binding, &msg) {
        Some(frame) => {
            let hdrs: Vec<(&str, &str)> = issued_hdr
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            Outcome::Reply(reply(200, "OK", &hdrs, &frame.to_canonical_string()))
        }
        None => {
            // A notification — `202` empty (Streamable's ack frame).
            let hdrs: Vec<(&str, &str)> = issued_hdr
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            Outcome::Reply(reply(202, "Accepted", &hdrs, ""))
        }
    }
}

/// `GET /mcp` — the SSE arm's open (DF-S4.11-1). The stream is
/// session-owned: `Mcp-Session-Id` carries the same gates as POST
/// (missing → `400`, stale → `404`); `Last-Event-ID` sets the resume
/// cursor (`None` → the log tail — live events only).
fn get(req: &HttpRequest, binding: &CallerBinding, sessions: &BTreeMap<String, ()>) -> Outcome {
    match req.headers.get(SESSION_HEADER) {
        Some(sid) if sessions.contains_key(sid) => {}
        Some(_) => {
            return Outcome::Reply(reply(
                404,
                "Not Found",
                &[],
                "{\"error\":\"unknown session\"}",
            ))
        }
        None => {
            return Outcome::Reply(reply(
                400,
                "Bad Request",
                &[],
                "{\"error\":\"missing Mcp-Session-Id\"}",
            ))
        }
    }
    // A malformed resume cursor can't be honored — start live (never
    // replay from the head: a guessed resume is worse than a loss the
    // client re-reads via resources/tasks).
    let last_id = req
        .headers
        .get(LAST_EVENT_ID_HEADER)
        .and_then(|v| v.trim().parse::<u64>().ok());
    Outcome::Sse {
        binding_id: binding.binding_id.clone(),
        last_id,
    }
}

/// The single-threaded multiplex loop — accept, read, dispatch and
/// push off the one `&mut LabServer`. `LabServer` is `!Send` (the
/// `EmbedService`'s `!Send` ports), so the server never moves to a
/// worker thread: a nonblocking round-robin is the honest shape —
/// `GET` streams stay open while POSTs dispatch through the same loop.
pub fn serve_http(
    srv: &mut LabServer,
    listener: &TcpListener,
    origins: &OriginPolicy,
) -> Result<(), ServeHttpError> {
    // session_id → issued — the contract record (a session exists iff
    // the table holds it; a missing id on a non-initialize request
    // refuses `404`).
    let mut sessions: BTreeMap<String, ()> = BTreeMap::new();
    let mut next_session = 0u64;
    listener.set_nonblocking(true).map_err(ServeHttpError::Io)?;
    let mut conns: Vec<Conn> = Vec::new();
    loop {
        // Accept every queued connection.
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).map_err(ServeHttpError::Io)?;
                    conns.push(Conn {
                        stream,
                        state: ConnState::Read {
                            buf: Vec::new(),
                            dead: false,
                        },
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(ServeHttpError::Io(e)),
            }
        }
        // Read request-side conns; serve every complete request.
        for conn in conns.iter_mut() {
            if let ConnState::Read { buf, dead } = &mut conn.state {
                let mut tmp = [0u8; 16_384];
                loop {
                    match conn.stream.read(&mut tmp) {
                        Ok(0) => {
                            *dead = true;
                            break;
                        }
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if n < tmp.len() {
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            *dead = true;
                            break;
                        }
                    }
                }
                if *dead {
                    continue;
                }
                if let Some(req) = parse_request(buf) {
                    match handle_request(srv, &req, origins, &mut sessions, &mut next_session) {
                        Outcome::Reply(resp) => {
                            // One request → one response → close (the
                            // keep-alive leg is the SSE arm, never a
                            // pipelined POST).
                            let _ = conn.stream.write_all(resp.as_bytes());
                            let _ = conn.stream.flush();
                            *dead = true;
                        }
                        Outcome::Sse {
                            binding_id,
                            last_id,
                        } => {
                            let mut out = String::from(
                                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\n\r\n",
                            );
                            // Replay the retained backlog — events after
                            // `last_id` scoped to this binding (`None`
                            // starts at the tail: live only).
                            let mut next_seq = last_id.map_or_else(
                                || srv.events.back().map(|e| e.seq + 1).unwrap_or(0),
                                |l| l + 1,
                            );
                            for ev in srv.events.iter() {
                                if ev.seq >= next_seq && ev.binding_id == binding_id {
                                    out.push_str(&sse_frame(ev));
                                    next_seq = ev.seq + 1;
                                }
                            }
                            if conn.stream.write_all(out.as_bytes()).is_ok()
                                && conn.stream.flush().is_ok()
                            {
                                conn.state = ConnState::Sse {
                                    binding_id,
                                    next_seq,
                                };
                            } else {
                                *dead = true;
                            }
                        }
                    }
                }
            }
        }
        conns.retain(|c| !matches!(c.state, ConnState::Read { dead: true, .. }));
        // Push queued events to open streams; probe each for liveness
        // (a client-dropped stream only surfaces on read/write).
        for conn in conns.iter_mut() {
            if let ConnState::Sse {
                binding_id,
                next_seq,
            } = &mut conn.state
            {
                let mut out = String::new();
                for ev in srv.events.iter() {
                    if ev.seq >= *next_seq && &ev.binding_id == binding_id {
                        out.push_str(&sse_frame(ev));
                        *next_seq = ev.seq + 1;
                    }
                }
                let mut dead = false;
                if !out.is_empty() {
                    dead = conn.stream.write_all(out.as_bytes()).is_err()
                        || conn.stream.flush().is_err();
                }
                if !dead {
                    let mut probe = [0u8; 1];
                    match conn.stream.read(&mut probe) {
                        Ok(0) => dead = true,
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => dead = true,
                    }
                }
                if dead {
                    conn.state = ConnState::Read {
                        buf: Vec::new(),
                        dead: true,
                    };
                }
            }
        }
        conns.retain(|c| !matches!(c.state, ConnState::Read { dead: true, .. }));
        std::thread::sleep(Duration::from_millis(2));
    }
}
