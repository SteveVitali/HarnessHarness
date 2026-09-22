//! Newline-delimited JSON-RPC 2.0 framing over a byte stream — binding (b) transport facts
//! (ADR-0179 D1(b)): UTF-8, one message per line, no embedded newlines, `stderr` for logs
//! only. This module owns *framing only*; it defines no method, field or error variant (a
//! binding may add transport facts, never a verb — ADR-0179 D2).

use crate::json::{parse, Json, JsonError};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

/// A JSON-RPC 2.0 request read off the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: Json,
    pub method: String,
    pub params: Json,
}

/// Build a JSON-RPC 2.0 request object.
pub fn request(id: Json, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

/// Build a JSON-RPC 2.0 success response.
pub fn ok_response(id: Json, result: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        ("result", result),
    ])
}

/// Build a JSON-RPC 2.0 error response. `data` is a typed, machine-readable payload — the
/// closed error sum of the contract lowers into it, so a mismatch is *typed, never a silent
/// fallback* (ADR-0178 D2 / ADR-0099 N2 inward).
pub fn err_response(id: Json, code: i64, message: &str, data: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        (
            "error",
            Json::obj([
                ("code", Json::Int(code)),
                ("message", Json::str(message)),
                ("data", data),
            ]),
        ),
    ])
}

/// Parse a single wire line into a `Request`. Enforces `jsonrpc == "2.0"` and the presence of
/// `method`; a missing/typed-wrong field is a framing error the caller lowers to the closed
/// error sum.
pub fn parse_request(line: &str) -> Result<Request, JsonError> {
    let v = parse(line)?;
    let obj = match v {
        Json::Obj(m) => m,
        _ => return Err(JsonError("request is not an object".into())),
    };
    check_version(&obj)?;
    let method = obj
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or_else(|| JsonError("missing 'method'".into()))?
        .to_string();
    let id = obj.get("id").cloned().unwrap_or(Json::Null);
    let params = obj.get("params").cloned().unwrap_or(Json::Null);
    Ok(Request { id, method, params })
}

fn check_version(obj: &BTreeMap<String, Json>) -> Result<(), JsonError> {
    match obj.get("jsonrpc").and_then(|v| v.as_str()) {
        Some("2.0") => Ok(()),
        _ => Err(JsonError("jsonrpc must be \"2.0\"".into())),
    }
}

/// Serialize a message and write it as one newline-delimited frame. Rejects a message whose
/// canonical form contains an embedded newline (it never should — canonical JSON is compact —
/// but the invariant is enforced, not assumed).
pub fn write_message<W: Write>(w: &mut W, msg: &Json) -> std::io::Result<()> {
    let s = msg.to_canonical_string();
    debug_assert!(
        !s.contains('\n'),
        "canonical JSON must not contain newlines"
    );
    if s.contains('\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "framed message contains an embedded newline",
        ));
    }
    w.write_all(s.as_bytes())?;
    w.write_all(b"\n")?;
    w.flush()
}

/// Read one newline-delimited frame and parse it. Returns `Ok(None)` at clean end-of-stream.
pub fn read_message<R: BufRead>(r: &mut R) -> std::io::Result<Option<Json>> {
    let mut line = String::new();
    let n = r.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return Ok(Some(Json::Null));
    }
    parse(trimmed)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn request_round_trips_over_a_framed_stream() {
        let req = request(Json::Int(1), "hello", Json::obj([("k", Json::Int(9))]));
        let mut buf = Vec::new();
        write_message(&mut buf, &req).unwrap();
        assert!(buf.ends_with(b"\n"));
        let mut cur = Cursor::new(buf);
        let got = read_message(&mut cur).unwrap().unwrap();
        let parsed = parse_request(&got.to_canonical_string()).unwrap();
        assert_eq!(parsed.method, "hello");
        assert_eq!(parsed.id, Json::Int(1));
    }

    #[test]
    fn rejects_wrong_protocol_version() {
        let line = r#"{"jsonrpc":"1.0","id":1,"method":"hello","params":null}"#;
        assert!(parse_request(line).is_err());
    }

    #[test]
    fn missing_method_is_an_error() {
        let line = r#"{"jsonrpc":"2.0","id":1,"params":null}"#;
        assert!(parse_request(line).is_err());
    }

    #[test]
    fn end_of_stream_is_none() {
        let mut cur = Cursor::new(Vec::new());
        assert_eq!(read_message(&mut cur).unwrap(), None);
    }
}
