//! `hh-wire::http` — the one minimal HTTP/1.1 codec: the `local_network`
//! binding (§7.2 binding (c); ADR-0301 D1), the `hh-env` mediated-egress
//! proxy leg, the `hh-mcp` boundary helpers, and R4.5b's model-lane
//! `HttpModelTransport` (ADR-0364 D1 — CC1: the wire shape has exactly one
//! implementation).
//!
//! Deliberately small: request/response head parse + write,
//! `Content-Length`/`chunked`/close-delimited bodies — no trailers
//! semantics, no pipelining, no 1xx, no TLS *in the codec*. Every
//! admission refusal (bad head, bad framing, too large, a scheme outside
//! `http`/`https`) is a typed [`HttpError`], never a partial read or a
//! silent upgrade. `https` parses — the TLS session lives in `hh-tls`
//! beneath this codec (R4.5c; ADR-0365); a caller that cannot terminate
//! TLS answers its own typed refusal, never a downgrade.

use std::io::{BufRead, Write};
use std::net::ToSocketAddrs;

/// Maximum request/response head bytes (request/status line + headers).
/// Generous for loopback but bounded — an unbounded head is a wedge.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

/// Maximum request-line + header count beyond the start line.
pub const MAX_HEADERS: usize = 64;

/// The parsed request head: method, raw target (`/path?query`) and headers
/// with lowercased names (first value wins on repeats — the binding's use
/// sites treat a repeated header as one value).
#[derive(Debug, Clone, PartialEq)]
pub struct RequestHead {
    /// The method (`GET`, `POST`, …) — ASCII, case-preserved.
    pub method: String,
    /// The raw request target (path + query, undecoded).
    pub target: String,
    /// Lowercased header names → values.
    pub headers: std::collections::BTreeMap<String, String>,
}

/// The parsed response head: status code + headers.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseHead {
    /// The numeric status (200, 401, …).
    pub status: u16,
    /// Lowercased header names → values.
    pub headers: std::collections::BTreeMap<String, String>,
}

/// One typed refusal on the HTTP codec boundary. `read_*` maps malformed
/// input to a variant — callers emit the right status and close.
#[derive(Debug, Clone, PartialEq)]
pub enum HttpError {
    /// I/O failure on the socket.
    Io(String),
    /// The start line was malformed (`<m> <target> HTTP/1.x` /
    /// `HTTP/1.x <status> <reason>` required).
    BadStartLine,
    /// A header line was malformed (`name: value` required).
    BadHeaderLine,
    /// The head exceeded `MAX_HEAD_BYTES` or `MAX_HEADERS`.
    HeadTooLarge,
    /// `Content-Length` was absent/duplicated/malformed on a message that
    /// needs it, or exceeded the caller's bound.
    BadContentLength,
    /// The body exceeded the caller's `max_body` bound.
    BodyTooLarge,
    /// The peer closed cleanly before any bytes (`read_request` → `Ok(None)`
    /// is the cleaner signal; this variant covers mid-message EOF).
    Eof,
    /// A socket deadline fired (`TimedOut`/`WouldBlock`) — the caller maps
    /// it to its own timeout class (connect/attempt/stream-idle), never a
    /// generic `Io`.
    Timeout,
    /// A body-bearing message arrived on a verb the binding does not
    /// body-parse (reserved — callers may map to 400).
    Unsupported,
    /// The URL's scheme is neither `http` nor `https` (TLS terminates in
    /// `hh-tls` beneath this codec — R4.5c/ADR-0365).
    UnsupportedScheme(String),
    /// The URL did not parse (`http://host[:port]/target` required).
    BadUrl,
    /// A chunked frame was malformed (bad size line, missing CRLF, bad
    /// trailer line).
    BadChunk,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Io(e) => write!(f, "io: {e}"),
            HttpError::BadStartLine => write!(f, "bad start line"),
            HttpError::BadHeaderLine => write!(f, "bad header line"),
            HttpError::HeadTooLarge => write!(f, "head too large"),
            HttpError::BadContentLength => write!(f, "bad content-length"),
            HttpError::BodyTooLarge => write!(f, "body too large"),
            HttpError::Eof => write!(f, "unexpected eof"),
            HttpError::Timeout => write!(f, "deadline fired"),
            HttpError::Unsupported => write!(f, "unsupported message"),
            HttpError::UnsupportedScheme(s) => write!(f, "unsupported scheme: {s}"),
            HttpError::BadUrl => write!(f, "bad url"),
            HttpError::BadChunk => write!(f, "bad chunk"),
        }
    }
}

impl std::error::Error for HttpError {}

/// `io::Error → HttpError` — deadline kinds (`TimedOut`/`WouldBlock`) are
/// the typed [`HttpError::Timeout`]; `UnexpectedEof` is [`HttpError::Eof`].
fn io_map(e: std::io::Error) -> HttpError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => HttpError::Timeout,
        std::io::ErrorKind::UnexpectedEof => HttpError::Eof,
        _ => HttpError::Io(e.to_string()),
    }
}

fn read_head_lines(r: &mut impl BufRead) -> Result<Vec<String>, HttpError> {
    let mut lines = Vec::new();
    let mut buf = String::new();
    let mut total = 0usize;
    loop {
        buf.clear();
        let n = r.read_line(&mut buf).map_err(io_map)?;
        if n == 0 {
            if lines.is_empty() {
                // Clean EOF before the start line — the caller decides this
                // is a closed connection, not an error.
                return Err(HttpError::Eof);
            }
            return Err(HttpError::Eof);
        }
        total += n;
        if total > MAX_HEAD_BYTES {
            return Err(HttpError::HeadTooLarge);
        }
        let line = buf.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return Ok(lines);
        }
        lines.push(line.to_string());
        if lines.len() > MAX_HEADERS + 1 {
            return Err(HttpError::HeadTooLarge);
        }
    }
}

fn parse_header_lines(
    lines: &[String],
) -> Result<std::collections::BTreeMap<String, String>, HttpError> {
    let mut headers = std::collections::BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(HttpError::BadHeaderLine)?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(HttpError::BadHeaderLine);
        }
        // First wins — a duplicated header is the sender's ambiguity, not
        // something the codec silently concatenates (P2: no smuggling path).
        headers
            .entry(name)
            .or_insert_with(|| value.trim().to_string());
    }
    Ok(headers)
}

/// Read one `<hex-size>[;ext]\r\n` line's chunk length (extensions ignored —
/// the codec drops them, never forwards). `Ok(0)` is the terminal chunk.
fn read_chunk_size(r: &mut impl BufRead) -> Result<usize, HttpError> {
    let mut line = String::new();
    let n = r.read_line(&mut line).map_err(io_map)?;
    if n == 0 {
        return Err(HttpError::Eof);
    }
    let digits = line
        .trim_end_matches(['\r', '\n'])
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if digits.is_empty() || digits.len() > 16 {
        return Err(HttpError::BadChunk);
    }
    usize::from_str_radix(digits, 16).map_err(|_| HttpError::BadChunk)
}

/// Read the trailer block after the terminal chunk (terminated by the empty
/// line). Trailers are consumed and dropped — the codec carries no trailer
/// semantics (the doc header's `no trailers` clause).
fn read_trailers(r: &mut impl BufRead) -> Result<(), HttpError> {
    // Members are validated as header-shaped lines and dropped — the codec
    // carries no trailer semantics (the doc header's `no trailers` clause).
    let _ = read_head_lines(r)?;
    Ok(())
}

/// `BodyMode` — the framing a response head declares for its body.
/// `Fixed(n)` = `Content-Length`; `Chunked` = `Transfer-Encoding: chunked`;
/// `Close` = neither (the body is delimited by connection close — HTTP/1.0
/// style and one-shot loopback fixtures).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BodyMode {
    /// `Content-Length` bytes follow.
    Fixed(usize),
    /// `Transfer-Encoding: chunked` framing.
    Chunked,
    /// Delimited by connection close (reads to EOF).
    Close,
}

/// The body framing `head` declares. `Transfer-Encoding: chunked` takes the
/// body when present (a sender mixing both is refused upstream by the proxy
/// contract; here chunked wins — the declared length is untrusted framing).
pub fn body_mode(head: &ResponseHead) -> BodyMode {
    if head
        .headers
        .get("transfer-encoding")
        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    {
        return BodyMode::Chunked;
    }
    match head.headers.get("content-length") {
        Some(v) => match v.parse::<usize>() {
            Ok(n) => BodyMode::Fixed(n),
            Err(_) => BodyMode::Fixed(0),
        },
        None => BodyMode::Close,
    }
}

/// Incremental response-body reader — yields bytes per [`BodyMode`]:
/// `Chunked` → one wire chunk per step; `Fixed`/`Close` → bounded reads.
/// `Ok(None)` is the terminal step (last chunk consumed / length filled /
/// EOF on close-delimited). This is the streaming primitive the model
/// transport's SSE decode rides on (R4.5b) — arrival stamps are taken per
/// step, never per assembled body.
pub struct BodyReader<'a, R: BufRead> {
    r: &'a mut R,
    mode: BodyMode,
    /// Bytes still owed under `Fixed`.
    remaining: usize,
    /// Bytes delivered so far (the `max_body` guard's count).
    delivered: usize,
    /// `max_body` — the byte bound across all modes.
    max_body: usize,
    /// A mid-chunk leftover under `Chunked`.
    pending: usize,
    done: bool,
}

impl<'a, R: BufRead> BodyReader<'a, R> {
    /// Wrap `r` for the body `head` declares; `max_body` bounds the total.
    pub fn new(r: &'a mut R, head: &ResponseHead, max_body: usize) -> BodyReader<'a, R> {
        let mode = body_mode(head);
        BodyReader {
            r,
            mode,
            remaining: match mode {
                BodyMode::Fixed(n) => n,
                _ => 0,
            },
            delivered: 0,
            max_body,
            pending: 0,
            done: false,
        }
    }

    /// Whether the terminal step has been observed.
    pub fn done(&self) -> bool {
        self.done
    }

    fn take(&mut self, bytes: &[u8]) -> Result<Vec<u8>, HttpError> {
        self.delivered += bytes.len();
        if self.delivered > self.max_body {
            return Err(HttpError::BodyTooLarge);
        }
        Ok(bytes.to_vec())
    }

    /// Read the next body segment — `Ok(None)` at the terminal boundary.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, HttpError> {
        if self.done {
            return Ok(None);
        }
        match self.mode {
            BodyMode::Fixed(_) => {
                if self.remaining == 0 {
                    self.done = true;
                    return Ok(None);
                }
                let mut buf = vec![0u8; self.remaining.min(8192)];
                let n = self.r.read(&mut buf).map_err(io_map)?;
                if n == 0 {
                    self.done = true;
                    return Err(HttpError::Eof);
                }
                buf.truncate(n);
                self.remaining -= n;
                if self.remaining == 0 {
                    self.done = true;
                }
                self.take(&buf).map(Some)
            }
            BodyMode::Close => {
                // Close-delimited: `read` until the peer's close. `BufReader`
                // surfaces buffered bytes first — an EOF before any body is
                // a clean terminal (`Ok(None)`), mid-body EOF is also the
                // terminal (the peer's `Connection: close` is the delimiter).
                let mut buf = vec![0u8; 8192];
                let n = self.r.read(&mut buf).map_err(io_map)?;
                if n == 0 {
                    self.done = true;
                    return Ok(None);
                }
                buf.truncate(n);
                self.take(&buf).map(Some)
            }
            BodyMode::Chunked => {
                if self.pending == 0 {
                    let size = read_chunk_size(self.r)?;
                    if size == 0 {
                        read_trailers(self.r)?;
                        self.done = true;
                        return Ok(None);
                    }
                    self.pending = size;
                }
                let mut buf = vec![0u8; self.pending.min(8192)];
                self.r.read_exact(&mut buf).map_err(io_map)?;
                self.pending -= buf.len();
                if self.pending == 0 {
                    // The chunk's trailing CRLF.
                    let mut crlf = [0u8; 2];
                    self.r.read_exact(&mut crlf).map_err(io_map)?;
                    if &crlf != b"\r\n" {
                        return Err(HttpError::BadChunk);
                    }
                }
                self.take(&buf).map(Some)
            }
        }
    }

    /// Drain to the terminal step — the assembled body bytes.
    pub fn read_all(&mut self) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        while let Some(c) = self.next_chunk()? {
            out.extend_from_slice(&c);
        }
        Ok(out)
    }
}

fn body_for(
    r: &mut impl BufRead,
    headers: &std::collections::BTreeMap<String, String>,
    max_body: usize,
    present_ok: bool,
) -> Result<Vec<u8>, HttpError> {
    // `Transfer-Encoding: chunked` request bodies decode through the same
    // framed path as responses (the `hh-mcp` boundary's SSE-post client
    // uses it — R4.5b's codec fold).
    if headers
        .get("transfer-encoding")
        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    {
        if !present_ok {
            return Err(HttpError::Unsupported);
        }
        let head = ResponseHead {
            status: 0,
            headers: headers.clone(),
        };
        return BodyReader::new(r, &head, max_body).read_all();
    }
    match headers.get("content-length") {
        Some(v) => {
            let n: usize = v.parse().map_err(|_| HttpError::BadContentLength)?;
            if n > max_body {
                return Err(HttpError::BodyTooLarge);
            }
            if !present_ok && n > 0 {
                return Err(HttpError::Unsupported);
            }
            let mut body = vec![0u8; n];
            r.read_exact(&mut body).map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    HttpError::Eof
                } else {
                    HttpError::Io(e.to_string())
                }
            })?;
            Ok(body)
        }
        None => Ok(Vec::new()),
    }
}

/// Read one HTTP request from `r`. `Ok(None)` = clean EOF before any head
/// bytes (peer closed). `max_body` bounds `Content-Length`.
pub fn read_request(
    r: &mut impl BufRead,
    max_body: usize,
) -> Result<Option<(RequestHead, Vec<u8>)>, HttpError> {
    let lines = match read_head_lines(r) {
        Ok(l) => l,
        Err(HttpError::Eof) => return Ok(None),
        Err(e) => return Err(e),
    };
    if lines.is_empty() {
        return Ok(None);
    }
    let start = &lines[0];
    let mut parts = start.splitn(3, ' ');
    let (method, target) = match (parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v)) if v.starts_with("HTTP/1.") => (m.to_string(), t.to_string()),
        _ => return Err(HttpError::BadStartLine),
    };
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(HttpError::BadStartLine);
    }
    let headers = parse_header_lines(&lines[1..])?;
    let has_body = matches!(method.as_str(), "POST" | "PUT" | "PATCH");
    let body = body_for(r, &headers, max_body, has_body)?;
    Ok(Some((
        RequestHead {
            method,
            target,
            headers,
        },
        body,
    )))
}

/// Read one HTTP response from `r` (client side of the binding).
pub fn read_response(
    r: &mut impl BufRead,
    max_body: usize,
) -> Result<(ResponseHead, Vec<u8>), HttpError> {
    let lines = read_head_lines(r)?;
    if lines.is_empty() {
        return Err(HttpError::BadStartLine);
    }
    let start = &lines[0];
    let mut parts = start.splitn(3, ' ');
    let status: u16 = match (parts.next(), parts.next()) {
        (Some(v), Some(s)) if v.starts_with("HTTP/1.") => {
            s.parse().map_err(|_| HttpError::BadStartLine)?
        }
        _ => return Err(HttpError::BadStartLine),
    };
    let headers = parse_header_lines(&lines[1..])?;
    let body = body_for(r, &headers, max_body, true)?;
    Ok((ResponseHead { status, headers }, body))
}

/// The canonical reason phrase for the statuses the binding emits.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

/// Write one complete HTTP response (head + `Content-Length` + body).
/// `Connection: close` is the caller's choice via `headers` — the codec does
/// not manage keep-alive policy.
pub fn write_response(
    w: &mut impl Write,
    status: u16,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(), HttpError> {
    let head = format!("HTTP/1.1 {} {}\r\n", status, reason(status));
    w.write_all(head.as_bytes())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    for (k, v) in headers {
        w.write_all(format!("{k}: {v}\r\n").as_bytes())
            .map_err(|e| HttpError::Io(e.to_string()))?;
    }
    w.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    w.write_all(body)
        .map_err(|e| HttpError::Io(e.to_string()))?;
    w.flush().map_err(|e| HttpError::Io(e.to_string()))?;
    Ok(())
}

/// Split a request target into path + query (both still percent-encoded).
pub fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (target, None),
    }
}

/// Write one complete HTTP request (request line + headers +
/// `Content-Length` + body). `Host` is the caller's (the codec does not
/// know the authority); `Connection` policy likewise.
pub fn write_request(
    w: &mut impl Write,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(), HttpError> {
    w.write_all(format!("{method} {target} HTTP/1.1\r\n").as_bytes())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    for (k, v) in headers {
        w.write_all(format!("{k}: {v}\r\n").as_bytes())
            .map_err(|e| HttpError::Io(e.to_string()))?;
    }
    w.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .map_err(|e| HttpError::Io(e.to_string()))?;
    w.write_all(body)
        .map_err(|e| HttpError::Io(e.to_string()))?;
    w.flush().map_err(|e| HttpError::Io(e.to_string()))?;
    Ok(())
}

/// Read just the response head — the streaming half of `read_response`
/// (the body then rides [`BodyReader`] or [`read_body`]).
pub fn read_response_head(r: &mut impl BufRead) -> Result<ResponseHead, HttpError> {
    let lines = read_head_lines(r)?;
    if lines.is_empty() {
        return Err(HttpError::BadStartLine);
    }
    let start = &lines[0];
    let mut parts = start.splitn(3, ' ');
    let status: u16 = match (parts.next(), parts.next()) {
        (Some(v), Some(s)) if v.starts_with("HTTP/1.") => {
            s.parse().map_err(|_| HttpError::BadStartLine)?
        }
        _ => return Err(HttpError::BadStartLine),
    };
    let headers = parse_header_lines(&lines[1..])?;
    Ok(ResponseHead { status, headers })
}

/// Read a complete response body under the mode `head` declares.
pub fn read_body(
    r: &mut impl BufRead,
    head: &ResponseHead,
    max_body: usize,
) -> Result<Vec<u8>, HttpError> {
    BodyReader::new(r, head, max_body).read_all()
}

/// Read one HTTP response under any declared body mode —
/// `Content-Length`, `Transfer-Encoding: chunked`, or close-delimited.
/// This is the client-facing reader (`hh-env` egress, `hh-mcp` posts,
/// the R4.5b model transport); [`read_response`] stays the strict
/// `Content-Length`-only reader binding (c) documented.
pub fn read_response_full(
    r: &mut impl BufRead,
    max_body: usize,
) -> Result<(ResponseHead, Vec<u8>), HttpError> {
    let head = read_response_head(r)?;
    let body = read_body(r, &head, max_body)?;
    Ok((head, body))
}

/// The URL scheme — `http` (plain) or `https` (TLS-terminated by the
/// caller's transport, R4.5c/ADR-0365). Every other scheme refuses
/// [`HttpError::UnsupportedScheme`] at parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Plain HTTP/1.1 over TCP.
    Http,
    /// HTTP/1.1 over a TLS session the caller establishes (the codec
    /// stays transport-agnostic — `connect` still dials TCP).
    Https,
}

/// A parsed `http://`/`https://` endpoint — `scheme`, `host` (bare or
/// `[v6]`), `port` (default 80/443 by scheme), `target` (path + query,
/// `/` when absent). Every other scheme refuses
/// [`HttpError::UnsupportedScheme`] — a caller that can't terminate TLS
/// answers its own typed refusal, never a silent downgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpUrl {
    /// The scheme (`https` ⇒ the caller wraps `connect`'s stream in TLS).
    pub scheme: Scheme,
    /// The authority's host (brackets stripped on IPv6 literals).
    pub host: String,
    /// The authority's port (default per scheme).
    pub port: u16,
    /// Path + query (`/…`), the request target.
    pub target: String,
}

/// The `Host` header value for the URL (brackets preserved on IPv6).
impl HttpUrl {
    /// The `Host` header spelling (non-default port is always spelled).
    pub fn host_header(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// `parse_http_url(url)` — `http://`/`https://` `host[:port][/target]`.
/// `https` parses with default port 443 — the caller establishes TLS
/// beneath the codec (R4.5c); a transport that cannot is expected to
/// answer its own typed refusal at dial time.
pub fn parse_http_url(url: &str) -> Result<HttpUrl, HttpError> {
    let rest = url.trim();
    if let Some(scheme_end) = rest.find("://") {
        let (scheme, default_port) = match &rest[..scheme_end] {
            s if s.eq_ignore_ascii_case("http") => (Scheme::Http, 80u16),
            s if s.eq_ignore_ascii_case("https") => (Scheme::Https, 443u16),
            s => {
                return Err(HttpError::UnsupportedScheme(s.to_ascii_lowercase()));
            }
        };
        let rest = &rest[scheme_end + 3..];
        let (authority, target) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, "/".to_string()),
        };
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let end = v6.find(']').ok_or(HttpError::BadUrl)?;
            let host = v6[..end].to_string();
            let tail = &v6[end + 1..];
            let port = match tail.strip_prefix(':') {
                Some(p) => p.parse::<u16>().map_err(|_| HttpError::BadUrl)?,
                None if tail.is_empty() => default_port,
                None => return Err(HttpError::BadUrl),
            };
            (host, port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) if !h.is_empty() => (
                    h.to_string(),
                    p.parse::<u16>().map_err(|_| HttpError::BadUrl)?,
                ),
                _ if !authority.is_empty() && !authority.contains(':') => {
                    (authority.to_string(), default_port)
                }
                _ => return Err(HttpError::BadUrl),
            }
        };
        if host.is_empty() {
            return Err(HttpError::BadUrl);
        }
        return Ok(HttpUrl {
            scheme,
            host,
            port,
            target,
        });
    }
    Err(HttpError::BadUrl)
}

/// `connect(url, connect_ms, rw_ms)` — dial the URL's authority, set the
/// read/write deadlines, return the stream. Scheme refusal rides
/// [`HttpError::UnsupportedScheme`]; resolution/connect failures are
/// [`HttpError::Io`] (the `io::Error` spelling preserved).
pub fn connect(
    url: &HttpUrl,
    connect_ms: u64,
    rw_ms: u64,
) -> Result<std::net::TcpStream, HttpError> {
    let addrs: Vec<_> = format!("{}:{}", url.host, url.port)
        .to_socket_addrs()
        .map_err(|e| HttpError::Io(format!("resolve {}: {e}", url.host_header())))?
        .collect();
    let mut last = HttpError::Io(format!("no address for {}", url.host_header()));
    let mut stream = None;
    // Walk the resolved set — a dual-stack `localhost` whose first family
    // has no listener falls through to the one that answers.
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(
            &addr,
            std::time::Duration::from_millis(connect_ms.max(1)),
        ) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => {
                last = match e.kind() {
                    std::io::ErrorKind::TimedOut => HttpError::Timeout,
                    _ => HttpError::Io(format!("connect {}: {e}", url.host_header())),
                };
            }
        }
    }
    let stream = stream.ok_or(last)?;
    let rw = std::time::Duration::from_millis(rw_ms.max(1));
    stream
        .set_read_timeout(Some(rw))
        .and_then(|_| stream.set_write_timeout(Some(rw)))
        .map_err(|e| HttpError::Io(e.to_string()))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn roundtrip_post_request() {
        let raw = b"POST /hh-embed/1 HTTP/1.1\r\nHost: 127.0.0.1:7777\r\nAuthorization: Bearer abc\r\nContent-Length: 4\r\n\r\nbody";
        let mut r = BufReader::new(&raw[..]);
        let (head, body) = read_request(&mut r, 1024).unwrap().unwrap();
        assert_eq!(head.method, "POST");
        assert_eq!(head.target, "/hh-embed/1");
        assert_eq!(head.headers["host"], "127.0.0.1:7777");
        assert_eq!(head.headers["authorization"], "Bearer abc");
        assert_eq!(body, b"body");
    }

    #[test]
    fn get_without_content_length_has_empty_body() {
        let raw = b"GET /events?wait_ms=0 HTTP/1.1\r\nHost: [::1]:1\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let (head, body) = read_request(&mut r, 1024).unwrap().unwrap();
        assert_eq!(head.method, "GET");
        assert!(body.is_empty());
        let (path, q) = split_target(&head.target);
        assert_eq!(path, "/events");
        assert_eq!(q, Some("wait_ms=0"));
    }

    #[test]
    fn eof_before_head_is_none() {
        let raw: &[u8] = b"";
        let mut r = BufReader::new(raw);
        assert!(read_request(&mut r, 1024).unwrap().is_none());
    }

    #[test]
    fn oversized_body_refused() {
        let raw = b"POST /x HTTP/1.1\r\nContent-Length: 99\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        assert_eq!(
            read_request(&mut r, 10).unwrap_err(),
            HttpError::BodyTooLarge
        );
    }

    #[test]
    fn response_roundtrip() {
        let mut buf = Vec::new();
        write_response(&mut buf, 401, &[("WWW-Authenticate", "Bearer")], b"no").unwrap();
        let s = String::from_utf8(buf.clone()).unwrap();
        assert!(s.starts_with("HTTP/1.1 401 Unauthorized\r\n"));
        let mut r = BufReader::new(&buf[..]);
        let (head, body) = read_response(&mut r, 1024).unwrap();
        assert_eq!(head.status, 401);
        assert_eq!(body, b"no");
    }

    #[test]
    fn bad_start_line_refused() {
        let raw = b"GARBAGE\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        assert_eq!(
            read_request(&mut r, 1024).unwrap_err(),
            HttpError::BadStartLine
        );
    }

    #[test]
    fn head_bound_refused() {
        let mut raw = b"GET / HTTP/1.1\r\n".to_vec();
        raw.extend(std::iter::repeat_n(b'x', MAX_HEAD_BYTES + 10));
        raw.extend(b"\r\n\r\n");
        let mut r = BufReader::new(&raw[..]);
        assert_eq!(
            read_request(&mut r, 1024).unwrap_err(),
            HttpError::HeadTooLarge
        );
    }

    // ── R4.5b codec halves (ADR-0364 D1) ──────────────────────────────

    #[test]
    fn write_request_roundtrips_through_read_request() {
        let mut buf = Vec::new();
        write_request(
            &mut buf,
            "POST",
            "/v1/chat/completions",
            &[
                ("Host", "127.0.0.1:9"),
                ("Content-Type", "application/json"),
            ],
            b"{}",
        )
        .unwrap();
        let mut r = BufReader::new(&buf[..]);
        let (head, body) = read_request(&mut r, 1024).unwrap().unwrap();
        assert_eq!(head.method, "POST");
        assert_eq!(head.target, "/v1/chat/completions");
        assert_eq!(head.headers["content-type"], "application/json");
        assert_eq!(body, b"{}");
    }

    #[test]
    fn chunked_response_decodes_incrementally() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let (head, body) = read_response_full(&mut r, 1024).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(body, b"hello");
    }

    #[test]
    fn chunked_multi_chunk_decodes() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhe\r\n3\r\nllo\r\n0\r\nTrailer: x\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let (head, body) = read_response_full(&mut r, 1024).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(body, b"hello");
    }

    #[test]
    fn chunked_bad_size_refused() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n";
        let mut r = BufReader::new(&raw[..]);
        assert_eq!(
            read_response_full(&mut r, 1024).unwrap_err(),
            HttpError::BadChunk
        );
    }

    #[test]
    fn chunked_body_bound_refused() {
        let raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\na\r\nwaytoolong\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        assert_eq!(
            read_response_full(&mut r, 4).unwrap_err(),
            HttpError::BodyTooLarge
        );
    }

    #[test]
    fn close_delimited_body_reads_to_eof() {
        let raw = b"HTTP/1.1 200 OK\r\nX: 1\r\n\r\nrest-is-body";
        let mut r = BufReader::new(&raw[..]);
        let (head, body) = read_response_full(&mut r, 1024).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(body, b"rest-is-body");
    }

    #[test]
    fn body_reader_yields_per_wire_chunk() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\none\r\n3\r\ntwo\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let head = read_response_head(&mut r).unwrap();
        let mut rd = BodyReader::new(&mut r, &head, 1024);
        assert_eq!(rd.next_chunk().unwrap().as_deref(), Some(&b"one"[..]));
        assert_eq!(rd.next_chunk().unwrap().as_deref(), Some(&b"two"[..]));
        assert_eq!(rd.next_chunk().unwrap(), None);
        assert!(rd.done());
    }

    #[test]
    fn chunked_request_body_decodes() {
        let raw = b"POST /x HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbody\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let (_head, body) = read_request(&mut r, 1024).unwrap().unwrap();
        assert_eq!(body, b"body");
    }

    #[test]
    fn url_parse_loopback_and_v6() {
        let u = parse_http_url("http://127.0.0.1:11434/v1/chat").unwrap();
        assert_eq!(
            (u.host.as_str(), u.port, u.target.as_str()),
            ("127.0.0.1", 11434, "/v1/chat")
        );
        let u = parse_http_url("http://[::1]:8080").unwrap();
        assert_eq!(
            (u.host.as_str(), u.port, u.target.as_str()),
            ("::1", 8080, "/")
        );
        let u = parse_http_url("http://localhost").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("localhost", 80));
    }

    #[test]
    fn https_refuses_with_typed_scheme() {
        // R4.5c: `https` parses (the caller terminates TLS beneath the
        // codec) — foreign schemes stay the typed refusal.
        assert_eq!(
            parse_http_url("https://api.example.com/v1").unwrap(),
            HttpUrl {
                scheme: Scheme::Https,
                host: "api.example.com".to_string(),
                port: 443,
                target: "/v1".to_string(),
            }
        );
        assert_eq!(
            parse_http_url("ftp://api.example.com/v1").unwrap_err(),
            HttpError::UnsupportedScheme("ftp".to_string())
        );
        assert!(parse_http_url("notaurl").is_err());
    }
}
