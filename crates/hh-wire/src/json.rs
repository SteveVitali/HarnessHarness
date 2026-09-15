//! A minimal, canonical JSON value model.
//!
//! Canonicalization rule (Stage-0 interim, superseded by `idp/1` when S1.2 lands — see
//! ADR for S0.1): object keys are emitted in sorted (byte-wise) order, no insignificant
//! whitespace, strings are minimally escaped, and numbers are integers only. Integers-only
//! keeps the content address (schema hash) free of float-formatting ambiguity at Stage 0;
//! the surfaces contract carries no floats in the Stage-0 message set.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A JSON value. Objects use a `BTreeMap` so serialization is canonical (sorted keys) by
/// construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }

    /// Build an object from key/value pairs (order-independent — canonicalized on output).
    pub fn obj(pairs: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
        let mut m = BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        Json::Obj(m)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Json::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }

    /// Serialize to canonical (sorted-key, compact) JSON. Deterministic: the same value
    /// always produces the same bytes, so it is safe to content-address.
    pub fn to_canonical_string(&self) -> String {
        let mut out = String::new();
        self.write_canonical(&mut out);
        out
    }

    fn write_canonical(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Json::Str(s) => write_json_string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_canonical(out);
                }
                out.push(']');
            }
            Json::Obj(map) => {
                out.push('{');
                // BTreeMap iterates in sorted key order — canonical.
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_string(k, out);
                    out.push(':');
                    v.write_canonical(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse errors for the minimal JSON reader.
#[derive(Debug, PartialEq, Eq)]
pub struct JsonError(pub String);

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "json parse error: {}", self.0)
    }
}
impl std::error::Error for JsonError {}

/// Parse a JSON document. Supports the subset the hh-embed/1 Stage-0 message set uses:
/// objects, arrays, strings, integers, booleans and null. Floats are rejected (integers
/// only — see the canonicalization rule above).
pub fn parse(input: &str) -> Result<Json, JsonError> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(JsonError(format!("trailing data at byte {}", p.pos)));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => self.null(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(b) => Err(JsonError(format!("unexpected byte {:?}", b as char))),
            None => Err(JsonError("unexpected end of input".into())),
        }
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.pos += 1; // {
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(JsonError("expected string key".into()));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(JsonError("expected ':'".into()));
            }
            self.pos += 1;
            let val = self.value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(map));
                }
                _ => return Err(JsonError("expected ',' or '}'".into())),
            }
        }
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            let val = self.value()?;
            items.push(val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(JsonError("expected ',' or ']'".into())),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1; // opening quote
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(JsonError("unterminated string".into())),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(s);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'n') => s.push('\n'),
                        Some(b'r') => s.push('\r'),
                        Some(b't') => s.push('\t'),
                        Some(b'b') => s.push('\u{0008}'),
                        Some(b'f') => s.push('\u{000C}'),
                        Some(b'u') => {
                            let hex = self
                                .bytes
                                .get(self.pos + 1..self.pos + 5)
                                .ok_or_else(|| JsonError("bad \\u escape".into()))?;
                            let code = u32::from_str_radix(
                                std::str::from_utf8(hex)
                                    .map_err(|_| JsonError("bad \\u".into()))?,
                                16,
                            )
                            .map_err(|_| JsonError("bad \\u hex".into()))?;
                            s.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                            self.pos += 4;
                        }
                        _ => return Err(JsonError("bad escape".into())),
                    }
                    self.pos += 1;
                }
                Some(_) => {
                    // Copy one UTF-8 char.
                    let rest = &self.bytes[self.pos..];
                    let ch_len = utf8_len(rest[0]);
                    let chunk = std::str::from_utf8(&rest[..ch_len])
                        .map_err(|_| JsonError("invalid utf-8".into()))?;
                    s.push_str(chunk);
                    self.pos += ch_len;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else if b == b'.' || b == b'e' || b == b'E' {
                return Err(JsonError("non-integer numbers are not permitted".into()));
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        text.parse::<i64>()
            .map(Json::Int)
            .map_err(|_| JsonError(format!("bad integer {text:?}")))
    }

    fn boolean(&mut self) -> Result<Json, JsonError> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err(JsonError("bad literal".into()))
        }
    }

    fn null(&mut self) -> Result<Json, JsonError> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err(JsonError("bad literal".into()))
        }
    }
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_sorts_keys() {
        let v = Json::obj([("b", Json::Int(2)), ("a", Json::Int(1))]);
        assert_eq!(v.to_canonical_string(), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn round_trip_parse_serialize_is_stable() {
        let src = r#"{"a":[1,2,3],"nested":{"x":"y"},"z":true}"#;
        let parsed = parse(src).unwrap();
        // Canonical output equals the (already canonical) source: stable content address.
        assert_eq!(parsed.to_canonical_string(), src);
    }

    #[test]
    fn floats_are_rejected() {
        assert!(parse("1.5").is_err());
        assert!(parse("[1e3]").is_err());
    }

    #[test]
    fn string_escapes_round_trip() {
        let v = Json::str("line\nbreak\t\"q\"");
        let s = v.to_canonical_string();
        assert_eq!(parse(&s).unwrap(), v);
    }
}
