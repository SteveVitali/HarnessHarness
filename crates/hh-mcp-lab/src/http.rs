//! The Streamable HTTP arm — POST `/mcp` JSON-RPC per request, the
//! `Mcp-Session-Id` header contract, `Origin` gating via
//! [`hh_mcp::http::OriginPolicy`], and bearer resolution through the
//! server's `BindingTable` (AC-R-2.11.3-3's oauth/stdio parity: the
//! same `tools/call` chain runs under whichever binding the credential
//! resolves).
//!
//! The transport is deliberately identical in shape to
//! `hh-mcp::http::serve_http` — the lab server's difference lives at
//! `handle_message` (the surface-session chain), never in the wire.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use hh_mcp::http::OriginPolicy;
use hh_wire::json::Json;

use crate::server::{error_frame, LabServer};

/// The served path (Streamable HTTP's single endpoint).
pub const MCP_PATH: &str = "/mcp";
/// The session header.
pub const SESSION_HEADER: &str = "mcp-session-id";

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

/// `respond` — one HTTP/1.1 answer.
fn respond(
    stream: &mut impl Write,
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(), ServeHttpError> {
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\ncontent-type: application/json\r\n",
        body.len()
    );
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    stream
        .write_all(out.as_bytes())
        .map_err(ServeHttpError::Io)?;
    stream.flush().map_err(ServeHttpError::Io)
}

/// The Streamable HTTP loop — one POST per JSON-RPC message, the
/// response body is the JSON-RPC frame (the C1 slice keeps the
// plain-JSON exchange; the SSE downgrade stays unstaged).
///
/// Auth: `Authorization: Bearer <token>` resolves through
/// [`LabServer::resolve_bearer`]; an absent/unknown token answers
/// `401` + `WWW-Authenticate` (the OAuth challenge — never a body
/// guess). `Mcp-Session-Id` is issued on `initialize` and required on
/// later frames (a stale id answers `404`).
pub fn serve_http(
    srv: &mut LabServer,
    listener: &TcpListener,
    origins: &OriginPolicy,
) -> Result<(), ServeHttpError> {
    // session_id → (issued?) — the contract record (a session exists
    // iff the table holds it; a missing id on a non-initialize POST
    // refuses `404`).
    let mut sessions: BTreeMap<String, ()> = BTreeMap::new();
    let mut next_session = 0u64;
    for conn in listener.incoming() {
        let mut stream = conn.map_err(ServeHttpError::Io)?;
        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(s) => s,
            Err(e) => return Err(ServeHttpError::Io(e)),
        });
        let mut request_line = String::new();
        if reader
            .read_line(&mut request_line)
            .map_err(ServeHttpError::Io)?
            == 0
        {
            continue;
        }
        let mut headers: BTreeMap<String, String> = BTreeMap::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).map_err(ServeHttpError::Io)?;
            if n == 0 || line.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            respond(&mut stream, 400, "Bad Request", &[], "{}")?;
            continue;
        }
        let (method, uri) = (parts[0], parts[1]);
        if method != "POST" {
            respond(&mut stream, 405, "Method Not Allowed", &[], "{}")?;
            continue;
        }
        if uri.split('?').next() != Some(MCP_PATH) {
            respond(&mut stream, 404, "Not Found", &[], "{}")?;
            continue;
        }
        // Origin gate — the CORS/loopback rule the deployment declares.
        if origins
            .check(headers.get("origin").map(String::as_str))
            .is_err()
        {
            respond(&mut stream, 403, "Forbidden", &[], "{}")?;
            continue;
        }
        let body_len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; body_len];
        reader.read_exact(&mut body).map_err(ServeHttpError::Io)?;
        let body_str = String::from_utf8_lossy(&body);

        // ── bearer resolution ───────────────────────────────────────
        let token = headers
            .get("authorization")
            .and_then(|h| h.strip_prefix("Bearer "))
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|h| h.strip_prefix("bearer "))
            });
        let Some(token) = token else {
            respond(
                &mut stream,
                401,
                "Unauthorized",
                &[("www-authenticate", "Bearer realm=\"hh-lab\"")],
                "{\"error\":\"unauthorized\"}",
            )?;
            continue;
        };
        let Some(binding) = srv.resolve_bearer(token) else {
            respond(
                &mut stream,
                401,
                "Unauthorized",
                &[("www-authenticate", "Bearer realm=\"hh-lab\"")],
                "{\"error\":\"unauthorized\"}",
            )?;
            continue;
        };

        // ── parse ────────────────────────────────────────────────────
        let msg = match hh_wire::json::parse(&body_str) {
            Ok(m) => m,
            Err(_) => {
                respond(
                    &mut stream,
                    400,
                    "Bad Request",
                    &[],
                    &error_frame(Json::Null, -32700, "parse_error").to_canonical_string(),
                )?;
                continue;
            }
        };
        let method_name = msg.get("method").and_then(Json::as_str).unwrap_or("");

        // ── session gate ────────────────────────────────────────────
        let session_id = headers.get(SESSION_HEADER).cloned();
        let mut issued: Option<String> = None;
        if method_name == "initialize" {
            if session_id.is_none() {
                next_session += 1;
                let sid = format!("mcp-session-{next_session}");
                sessions.insert(sid.clone(), ());
                issued = Some(sid);
            }
        } else if let Some(sid) = &session_id {
            if !sessions.contains_key(sid) {
                respond(
                    &mut stream,
                    404,
                    "Not Found",
                    &[],
                    "{\"error\":\"unknown session\"}",
                )?;
                continue;
            }
        } else {
            respond(
                &mut stream,
                400,
                "Bad Request",
                &[],
                "{\"error\":\"missing Mcp-Session-Id\"}",
            )?;
            continue;
        }

        // ── dispatch ────────────────────────────────────────────────
        let out_headers: Vec<(String, String)> = issued
            .iter()
            .map(|s| (SESSION_HEADER.to_string(), s.clone()))
            .collect();
        match srv.handle_message(&binding, &msg) {
            Some(frame) => {
                let hdrs: Vec<(&str, &str)> = out_headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                respond(&mut stream, 200, "OK", &hdrs, &frame.to_canonical_string())?;
            }
            None => {
                // A notification — `202` empty (Streamable's ack frame).
                let hdrs: Vec<(&str, &str)> = out_headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                respond(&mut stream, 202, "Accepted", &hdrs, "")?;
            }
        }
    }
    #[allow(unreachable_code)]
    Ok(())
}
