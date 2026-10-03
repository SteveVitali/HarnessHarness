//! The Streamable HTTP transport (§5d.4 D4 — the C1 HTTP binding):
//! one JSON-RPC message per POST, a `Mcp-Session-Id` minted at
//! `initialize`, `subscriptions/listen` as the server→client stream
//! drain, and the `Origin` allowlist (the DNS-rebinding guard) the
//! serve side enforces before any dispatch.
//!
//! The transport is a *client* [`Transport`] + a `serve_http` loop —
//! hermetic fixtures only (loopback sockets, never a live service).
//! OAuth rides the same HTTP form: a `401` + `WWW-Authenticate`
//! challenge is a transport-level fact the [`crate::oauth`] flow
//! answers through the H3 mediator; the bearer itself travels only in
//! the `Authorization` header — never in a payload or a ledger row.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use hh_wire::json::Json;

use crate::artifact::ServedArtifact;
use crate::client::Transport;
use crate::server::ServeMode;

/// `OriginPolicy` — the allowlist the DNS-rebinding guard checks (§5d.4
/// D4). `require` controls whether an absent `Origin` header is a
/// refusal (the strict posture — an `Origin` must always be present
/// and trusted) or permitted for non-browser clients.
#[derive(Debug, Clone)]
pub struct OriginPolicy {
    /// Trusted origins (`scheme://host[:port]` exact-match strings).
    pub allowed: BTreeSet<String>,
    /// Whether a missing `Origin` refuses (default true — fail closed).
    pub require: bool,
}

/// The typed `Origin` refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginRefusal {
    /// `Origin` absent under `require`.
    Missing,
    /// `Origin` present but not allowlisted.
    Untrusted(String),
}

impl std::fmt::Display for OriginRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OriginRefusal::Missing => write!(f, "origin_missing"),
            OriginRefusal::Untrusted(o) => write!(f, "origin_untrusted: {o}"),
        }
    }
}

impl OriginPolicy {
    /// `OriginPolicy{allowed, require}` — exact-match strings only.
    pub fn new(allowed: &[&str], require: bool) -> OriginPolicy {
        OriginPolicy {
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
            require,
        }
    }

    /// Check a request's `Origin` header — `Ok` admits, else the typed
    /// refusal (the caller answers `403`).
    pub fn check(&self, origin: Option<&str>) -> Result<(), OriginRefusal> {
        match origin {
            None if self.require => Err(OriginRefusal::Missing),
            None => Ok(()),
            Some(o) if self.allowed.contains(o) => Ok(()),
            Some(o) => Err(OriginRefusal::Untrusted(o.to_string())),
        }
    }
}

/// One parsed HTTP response (status + lower-cased headers + body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// Lower-cased header map.
    pub headers: BTreeMap<String, String>,
    /// The body.
    pub body: String,
}

/// `HttpTransport` — the client's half: `send` POSTs one JSON-RPC line,
/// `recv` returns the POST response body (a single JSON-RPC message —
/// canonical JSON carries no newlines, so the line discipline holds).
/// `subscriptions/listen` drains the server→client queue on the same
/// POST channel.
pub struct HttpTransport {
    /// `host:port` the POSTs dial.
    addr: String,
    /// The request URI — `/` for the fixture form (RFC 8707's
    /// `resource` indicator and `binding.request_target` spell it).
    target: String,
    /// The client's own `Origin` header (declared, never suppressed).
    origin: String,
    /// `Mcp-Session-Id` captured at `initialize` — echoed on later POSTs.
    session_id: Option<String>,
    /// The bearer the H3-mediated OAuth flow delivered — wire header
    /// only (never ledgered, never logged).
    bearer: Option<String>,
    /// The buffered response body `recv` hands out.
    last: Option<String>,
}

impl HttpTransport {
    /// `HttpTransport::new("127.0.0.1:PORT", "/mcp", "http://test.local")`.
    pub fn new(addr: &str, target: &str, origin: &str) -> HttpTransport {
        HttpTransport {
            addr: addr.to_string(),
            target: target.to_string(),
            origin: origin.to_string(),
            session_id: None,
            bearer: None,
            last: None,
        }
    }

    /// The request URI — the OAuth `resource` indicator (RFC 8707) and
    /// `binding.request_target` member.
    pub fn request_target(&self) -> String {
        format!("http://{}{}", self.addr, self.target)
    }

    /// The minted session id (post-`initialize`).
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Install the H3-mediated bearer — the token lives in the
    /// `Authorization` header only.
    pub fn set_bearer(&mut self, token: &str) {
        self.bearer = Some(token.to_string());
    }

    /// Whether a bearer is installed.
    pub fn has_bearer(&self) -> bool {
        self.bearer.is_some()
    }

    /// `POST` one JSON-RPC line — the raw response (status/headers/body)
    /// for the OAuth path's `401` challenge handling. Non-2xx statuses
    /// are returned, not flattened — the caller decides.
    pub fn post(&mut self, body: &str) -> Result<HttpResponse, String> {
        let mut s = TcpStream::connect(&self.addr).map_err(|e| format!("connect: {e}"))?;
        let mut req = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nOrigin: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.target,
            self.addr,
            self.origin,
            body.len(),
        );
        if let Some(sid) = &self.session_id {
            req.push_str(&format!("Mcp-Session-Id: {sid}\r\n"));
        }
        if let Some(b) = &self.bearer {
            req.push_str(&format!("Authorization: Bearer {b}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes())
            .and_then(|()| s.write_all(body.as_bytes()))
            .map_err(|e| format!("write: {e}"))?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).map_err(|e| format!("read: {e}"))?;
        let text = String::from_utf8_lossy(&buf).to_string();
        parse_response(&text)
    }

    /// `subscriptions/listen` — the server→client drain: returns the
    /// queued notifications (each verbatim).
    pub fn listen(&mut self) -> Result<Vec<Json>, String> {
        let req = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(-1)),
            ("method", Json::str("subscriptions/listen")),
            ("params", Json::obj([])),
        ]);
        let resp = self.post(&req.to_canonical_string())?;
        if resp.status != 200 {
            return Err(format!("subscriptions/listen: http {}", resp.status));
        }
        let parsed = hh_wire::json::parse(&resp.body).map_err(|e| format!("parse: {e}"))?;
        let items = parsed
            .get("result")
            .and_then(|r| r.get("notifications"))
            .and_then(|n| match n {
                Json::Arr(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();
        Ok(items)
    }
}

impl Transport for HttpTransport {
    fn send(&mut self, line: &str) -> Result<(), String> {
        let resp = self.post(line)?;
        if let Some(sid) = resp.headers.get("mcp-session-id") {
            self.session_id = Some(sid.clone());
        }
        if resp.status != 200 {
            return Err(format!("http {}", resp.status));
        }
        self.last = Some(resp.body);
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<String>, String> {
        Ok(self.last.take())
    }
}

/// Parse `HTTP/1.1 <status>` + headers + body (Connection: close
/// fixture form — the body is everything after the blank line).
fn parse_response(text: &str) -> Result<HttpResponse, String> {
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed response".to_string())?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "no status".to_string())?;
    let mut headers = BTreeMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    Ok(HttpResponse {
        status,
        headers,
        body: body.to_string(),
    })
}

/// `ServeHttpError` — the serve loop's failures.
#[derive(Debug)]
pub enum ServeHttpError {
    /// The socket failed.
    Io(std::io::Error),
}

impl std::fmt::Display for ServeHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeHttpError::Io(e) => write!(f, "http: {e}"),
        }
    }
}

impl std::error::Error for ServeHttpError {}

/// Write one HTTP response (Connection: close — the fixture form).
fn respond(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(), ServeHttpError> {
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .map_err(ServeHttpError::Io)
}

/// `serve_http(listener, load, mode, origins, bearer)` — the Streamable
/// HTTP serve loop: one connection at a time (the fixture form). Every
/// POST passes the `Origin` guard *before* dispatch (`403` on refusal);
/// a `bearer` requirement answers `401` +
/// `WWW-Authenticate: Bearer realm="hh-mcp", resource_metadata="<uri>"`
/// (the OAuth entry point — the edge's H3 flow). `initialize` mints the
/// `Mcp-Session-Id` header. `load` re-lowers the artefact per request —
/// a `catalogue_hash` change queues `notifications/tools/list_changed`
/// for `subscriptions/listen` (AC-R-2.5.4-2 — before the next answer,
/// never eagerly on the wire).
///
/// `GET`/`DELETE`/other methods → `405`. Malformed requests → `400`.
/// JSON-RPC `tools/call` `target` dedup lives in
/// [`crate::server::handle_message`].
pub fn serve_http(
    listener: &TcpListener,
    load: &mut dyn FnMut() -> ServedArtifact,
    mode: ServeMode,
    origins: &OriginPolicy,
    bearer: Option<&str>,
) -> Result<(), ServeHttpError> {
    let mut artifact = load();
    let mut dedup: BTreeMap<String, Json> = BTreeMap::new();
    let mut pending: VecDeque<Json> = VecDeque::new();
    let mut session_id: Option<String> = None;
    for conn in listener.incoming() {
        let mut stream = conn.map_err(ServeHttpError::Io)?;
        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(s) => s,
            Err(e) => return Err(ServeHttpError::Io(e)),
        });
        // Request line + headers + Content-Length body.
        let mut request_line = String::new();
        if reader
            .read_line(&mut request_line)
            .map_err(ServeHttpError::Io)?
            == 0
        {
            continue;
        }
        let mut headers = BTreeMap::new();
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
        let (method, _uri) = (parts[0], parts[1]);
        if method != "POST" {
            respond(&mut stream, 405, "Method Not Allowed", &[], "{}")?;
            continue;
        }
        // Read the request body BEFORE any rejection — a socket closed
        // with unread inbound data resets (RST) and the client loses the
        // response. The guards still run before any *dispatch*; the body
        // bytes are inert until parsed below.
        let len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).map_err(ServeHttpError::Io)?;
        // The Origin guard — before any dispatch (DNS-rebinding).
        if let Err(r) = origins.check(headers.get("origin").map(String::as_str)) {
            let body = Json::obj([("error", Json::str(r.to_string()))]).to_canonical_string();
            respond(&mut stream, 403, "Forbidden", &[], &body)?;
            continue;
        }
        // The OAuth gate — a bearer requirement challenges unauthenticated
        // POSTs (the `WWW-Authenticate` member the edge's H3 flow answers).
        if let Some(expected) = bearer {
            let ok = headers
                .get("authorization")
                .map(|h| h == &format!("Bearer {expected}"))
                .unwrap_or(false);
            if !ok {
                let challenge = format!(
                    "Bearer realm=\"hh-mcp\", resource_metadata=\"{}\"",
                    headers
                        .get("host")
                        .map(|h| format!("http://{h}"))
                        .unwrap_or_else(|| "http://localhost".to_string())
                );
                let body = Json::obj([("error", Json::str("unauthorized"))]).to_canonical_string();
                respond(
                    &mut stream,
                    401,
                    "Unauthorized",
                    &[("WWW-Authenticate", &challenge)],
                    &body,
                )?;
                continue;
            }
        }
        let text = String::from_utf8_lossy(&body).to_string();
        // A catalogue change queues `list_changed` before the answer
        // (AC-R-2.5.4-2) — the `subscriptions/listen` drain serves it.
        let next = load();
        if next.catalogue_hash != artifact.catalogue_hash {
            pending.push_back(Json::obj([
                ("jsonrpc", Json::str("2.0")),
                (
                    "method",
                    Json::str("notifications/tools/list_changed".to_string()),
                ),
                ("params", Json::obj([])),
            ]));
            artifact = next;
        }
        let msg = match hh_wire::json::parse(&text) {
            Ok(m) => m,
            Err(_) => {
                let frame = crate::server::error_frame_pub(Json::Null, -32700, "parse_error");
                respond(&mut stream, 200, "OK", &[], &frame.to_canonical_string())?;
                continue;
            }
        };
        let is_initialize = msg.get("method").and_then(Json::as_str) == Some("initialize");
        if let Some(frame) =
            crate::server::handle_message(&artifact, mode, true, &msg, &mut dedup, &mut pending)
        {
            // Mint the session id at `initialize` (the Streamable HTTP
            // session binding) — echoed as a response header.
            let mut extra: Vec<(&str, &str)> = Vec::new();
            if is_initialize && session_id.is_none() {
                let sid = hh_identity::idp::idp_id("mcp_session", text.as_bytes());
                session_id = Some(sid);
            }
            if let Some(sid) = &session_id {
                extra.push(("Mcp-Session-Id", sid.as_str()));
            }
            respond(&mut stream, 200, "OK", &extra, &frame.to_canonical_string())?;
        }
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{ArtifactTool, ServedArtifact};

    fn artifact() -> ServedArtifact {
        ServedArtifact {
            bundle_id: "bundle:test".to_string(),
            bundle_version: "v1".to_string(),
            tools: vec![ArtifactTool {
                name: "fs_read".to_string(),
                description: Some("read".to_string()),
                input_schema: Json::obj([]),
                output_schema: None,
                annotations: None,
                semantic_id: "test:fs_read".to_string(),
                hir_meta: Json::obj([]),
                ext_meta: Json::obj([]),
                input_requests: None,
            }],
            catalogue_hash: "cat-1".to_string(),
            ttl_ms: 0,
        }
    }

    #[test]
    fn origin_policy_exact_match_and_fail_closed() {
        // The allowlist is exact-match — never a wildcard.
        let p = OriginPolicy::new(&["http://test.local"], true);
        assert!(p.check(Some("http://test.local")).is_ok());
        assert_eq!(
            p.check(Some("http://evil.local")),
            Err(OriginRefusal::Untrusted("http://evil.local".to_string()))
        );
        assert_eq!(p.check(None), Err(OriginRefusal::Missing));
        // Substring/suffix tricks never pass.
        assert!(p.check(Some("http://test.local.evil.com")).is_err());
        assert!(p.check(Some("https://test.local")).is_err());
        // `require: false` admits absent Origin (non-browser clients)
        // but still refuses an untrusted one.
        let lax = OriginPolicy::new(&["http://test.local"], false);
        assert!(lax.check(None).is_ok());
        assert!(lax.check(Some("http://evil.local")).is_err());
    }

    /// Spawn `serve_http` on a fresh loopback listener; return its addr.
    /// The serve thread outlives the test (the fixture's accept loop —
    /// Connection: close per request, the process reaps it at exit).
    fn spawn_server(origins: OriginPolicy, bearer: Option<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let mut load = artifact;
            let _ = serve_http(
                &listener,
                &mut load,
                ServeMode::Modern,
                &origins,
                bearer.as_deref(),
            );
        });
        addr
    }

    #[test]
    fn untrusted_origin_is_403_before_dispatch() {
        let addr = spawn_server(OriginPolicy::new(&["http://good.local"], true), None);
        // Wrong Origin → 403 with the typed refusal, never dispatched.
        let mut bad = HttpTransport::new(&addr, "/mcp", "http://evil.local");
        let resp = bad.post("{}").unwrap();
        assert_eq!(resp.status, 403);
        let body = hh_wire::json::parse(&resp.body).unwrap();
        assert_eq!(
            body.get("error"),
            Some(&Json::str("origin_untrusted: http://evil.local"))
        );
        // Trusted Origin → reaches dispatch (a real request frame —
        // `ping` answers 200; the guard passed first).
        let mut good = HttpTransport::new(&addr, "/mcp", "http://good.local");
        let ping = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(1)),
            ("method", Json::str("ping")),
            ("params", Json::obj([])),
        ]);
        let resp = good.post(&ping.to_canonical_string()).unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn initialize_mints_session_header() {
        let addr = spawn_server(OriginPolicy::new(&["http://t.local"], true), None);
        let mut t = HttpTransport::new(&addr, "/mcp", "http://t.local");
        let init = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(1)),
            ("method", Json::str("initialize")),
            ("params", Json::obj([])),
        ]);
        let resp = t.post(&init.to_canonical_string()).unwrap();
        assert_eq!(resp.status, 200);
        assert!(
            resp.headers.contains_key("mcp-session-id"),
            "initialize mints the session header"
        );
    }

    #[test]
    fn bearer_requirement_challenges_401_with_resource_metadata() {
        let addr = spawn_server(
            OriginPolicy::new(&["http://t.local"], true),
            Some("the-token".to_string()),
        );
        let mut t = HttpTransport::new(&addr, "/mcp", "http://t.local");
        // No bearer → 401 + the WWW-Authenticate challenge the OAuth
        // flow answers (the resource indicator rides along).
        let ping = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(1)),
            ("method", Json::str("ping")),
            ("params", Json::obj([])),
        ]);
        let resp = t.post(&ping.to_canonical_string()).unwrap();
        assert_eq!(resp.status, 401);
        let challenge = resp
            .headers
            .get("www-authenticate")
            .expect("challenge header");
        assert!(challenge.starts_with("Bearer realm="));
        assert!(challenge.contains("resource_metadata="));
        // With the bearer installed the request reaches dispatch.
        t.set_bearer("the-token");
        let resp = t.post(&ping.to_canonical_string()).unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn non_post_is_405() {
        let addr = spawn_server(OriginPolicy::new(&["http://t.local"], true), None);
        // A raw GET — never a dispatch.
        let mut s = TcpStream::connect(&addr).unwrap();
        s.write_all(b"GET /mcp HTTP/1.1\r\nHost: x\r\nOrigin: http://t.local\r\n\r\n")
            .unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        assert!(buf.starts_with("HTTP/1.1 405"), "{buf}");
    }
}
