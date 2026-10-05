//! `hh-wire::http` — the one minimal HTTP/1.1 codec for the `local_network`
//! transport (§7.2 binding (c); ADR-0301 D1).
//!
//! Deliberately small: request-head parse + `Content-Length` body, response
//! write + head parse. No chunked encoding, no trailers, no pipelining, no
//! 1xx — the loopback JSON-RPC binding and the `hh-web` surface need nothing
//! else, and every admission refusal (bad head, bad framing, too large) is a
//! typed [`HttpError`], never a partial read. Both sides share this module so
//! the wire shape has exactly one implementation (CC1).

use std::io::{BufRead, Write};

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
    /// A body-bearing message arrived on a verb the binding does not
    /// body-parse (reserved — callers may map to 400).
    Unsupported,
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
            HttpError::Unsupported => write!(f, "unsupported message"),
        }
    }
}

impl std::error::Error for HttpError {}

fn read_head_lines(r: &mut impl BufRead) -> Result<Vec<String>, HttpError> {
    let mut lines = Vec::new();
    let mut buf = String::new();
    let mut total = 0usize;
    loop {
        buf.clear();
        let n = r
            .read_line(&mut buf)
            .map_err(|e| HttpError::Io(e.to_string()))?;
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

fn body_for(
    r: &mut impl BufRead,
    headers: &std::collections::BTreeMap<String, String>,
    max_body: usize,
    present_ok: bool,
) -> Result<Vec<u8>, HttpError> {
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
}
