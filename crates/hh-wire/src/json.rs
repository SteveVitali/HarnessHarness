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

/// `PJson` — the **provider-bytes** tree (R6.10; design/33 F6;
/// REQ-XC-103). Same grammar and the same one scanner as [`Json`], but a
/// non-integer number lexeme is preserved as `Num(lexeme)` — *distinct
/// from* `Str`, so a pass-through codec can mutate a request body and
/// re-serialize without quoting `0.7` into `"0.7"`. Objects ride a
/// `BTreeMap`, so `to_provider_string` is canonical (sorted keys,
/// compact) — the canonicalizer's own spelling, never a second one
/// (CC1). `PJson` never content-addresses: the canonical `Json`
/// projection (`to_json` / [`parse`]/[`parse_provider`]) stays
/// float-free and remains the only hashable form.
#[derive(Debug, Clone, PartialEq)]
pub enum PJson {
    /// `null`.
    Null,
    /// `true` | `false`.
    Bool(bool),
    /// An integer literal.
    Int(i64),
    /// A non-integer number lexeme, verbatim (`0.7`, `1e-3`).
    Num(String),
    /// A string.
    Str(String),
    /// An array.
    Arr(Vec<PJson>),
    /// An object (sorted-key by construction).
    Obj(BTreeMap<String, PJson>),
}

impl PJson {
    /// `get(key)` — object member lookup.
    pub fn get(&self, key: &str) -> Option<&PJson> {
        match self {
            PJson::Obj(m) => m.get(key),
            _ => None,
        }
    }

    /// Mutable object-member lookup.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut PJson> {
        match self {
            PJson::Obj(m) => m.get_mut(key),
            _ => None,
        }
    }

    /// The object map, when this is an object.
    pub fn as_obj(&self) -> Option<&BTreeMap<String, PJson>> {
        match self {
            PJson::Obj(m) => Some(m),
            _ => None,
        }
    }

    /// The object map mutably, when this is an object.
    pub fn as_obj_mut(&mut self) -> Option<&mut BTreeMap<String, PJson>> {
        match self {
            PJson::Obj(m) => Some(m),
            _ => None,
        }
    }

    /// The items, when this is an array.
    pub fn as_arr(&self) -> Option<&Vec<PJson>> {
        match self {
            PJson::Arr(v) => Some(v),
            _ => None,
        }
    }

    /// The items mutably, when this is an array.
    pub fn as_arr_mut(&mut self) -> Option<&mut Vec<PJson>> {
        match self {
            PJson::Arr(v) => Some(v),
            _ => None,
        }
    }

    /// The string, when this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PJson::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The boolean, when this is a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PJson::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The numeric value as `f64` — `Int` or `Num` (the lexeme parsed;
    /// an unparseable lexeme is `None`, never fabricated).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PJson::Int(i) => Some(*i as f64),
            PJson::Num(lex) => lex.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// The integer value — `Int` directly; a `Num` lexeme only when it
    /// parses back to the same integer it spells (a usage count the
    /// provider spelled `123.0` reads as `123`, never truncated).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PJson::Int(i) => Some(*i),
            PJson::Num(lex) => lex
                .parse::<f64>()
                .ok()
                .filter(|f| f.fract() == 0.0 && f.is_finite())
                .map(|f| f as i64),
            _ => None,
        }
    }

    /// Serialize for the wire: canonical (sorted-key, compact) and
    /// `Num` lexemes emitted **unquoted** — the byte-faithful
    /// pass-through form (upstream bytes = downstream bytes + declared
    /// mutations, REQ-XC-103).
    pub fn to_provider_string(&self) -> String {
        let mut out = String::new();
        self.write_provider(&mut out);
        out
    }

    fn write_provider(&self, out: &mut String) {
        match self {
            PJson::Null => out.push_str("null"),
            PJson::Bool(true) => out.push_str("true"),
            PJson::Bool(false) => out.push_str("false"),
            PJson::Int(i) => {
                let _ = write!(out, "{i}");
            }
            PJson::Num(lex) => out.push_str(lex),
            PJson::Str(s) => write_json_string(s, out),
            PJson::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_provider(out);
                }
                out.push(']');
            }
            PJson::Obj(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_string(k, out);
                    out.push(':');
                    v.write_provider(out);
                }
                out.push('}');
            }
        }
    }

    /// The canonical-`Json` projection under provider semantics: every
    /// `Num` lexeme becomes `Json::Str` (the `parse_provider` rule — no
    /// float ever enters the hashable form).
    pub fn to_json(&self) -> Json {
        match self {
            PJson::Null => Json::Null,
            PJson::Bool(b) => Json::Bool(*b),
            PJson::Int(i) => Json::Int(*i),
            PJson::Num(lex) | PJson::Str(lex) => Json::str(lex.clone()),
            PJson::Arr(items) => Json::Arr(items.iter().map(PJson::to_json).collect()),
            PJson::Obj(m) => Json::Obj(
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect::<BTreeMap<_, _>>(),
            ),
        }
    }
}

/// `parse_preserve(input)` — the pass-through read: the one scanner,
/// `Num` lexemes preserved typed (R6.10). Malformed input refuses
/// exactly as [`parse`] does.
pub fn parse_preserve(input: &str) -> Result<PJson, JsonError> {
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
    parse_preserve(input).and_then(|v| project(v, false))
}

/// `parse_provider(input)` — the *inbound* provider-bytes read (R4.5b):
/// the same grammar, but a non-integer number lexeme is preserved
/// verbatim as `Json::Str` — `8.782312925170068` lands as the string
/// `"8.782312925170068"`, never truncated to an integer and never
/// silently dropped (provider-opaque data is carried, not coerced; the
/// canonical `Json` form stays float-free so every downstream hash +
/// `to_canonical_string` stays deterministic). The *outbound* direction
/// keeps `parse`'s strictness — the kernel never emits a float.
pub fn parse_provider(input: &str) -> Result<Json, JsonError> {
    parse_preserve(input).and_then(|v| project(v, true))
}

/// Project a parsed [`PJson`] to the canonical [`Json`]: strict mode
/// refuses `Num` (the `parse` rule); provider mode folds it to
/// `Json::Str` (the `parse_provider` rule). One scanner, two
/// projections — the projection is the only difference between the two
/// public parses (CC1).
fn project(v: PJson, provider: bool) -> Result<Json, JsonError> {
    match v {
        PJson::Null => Ok(Json::Null),
        PJson::Bool(b) => Ok(Json::Bool(b)),
        PJson::Int(i) => Ok(Json::Int(i)),
        PJson::Num(lex) => {
            if provider {
                // The raw lexeme preserved as a string — a value the
                // model plane can surface verbatim (e.g. usage
                // `timings`) without inventing a float type into the
                // canonical form.
                Ok(Json::Str(lex))
            } else {
                Err(JsonError("non-integer numbers are not permitted".into()))
            }
        }
        PJson::Str(s) => Ok(Json::Str(s)),
        PJson::Arr(items) => Ok(Json::Arr(
            items
                .into_iter()
                .map(|i| project(i, provider))
                .collect::<Result<_, _>>()?,
        )),
        PJson::Obj(m) => Ok(Json::Obj(
            m.into_iter()
                .map(|(k, v)| project(v, provider).map(|v| (k, v)))
                .collect::<Result<_, _>>()?,
        )),
    }
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

    fn value(&mut self) -> Result<PJson, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(PJson::Str(self.string()?)),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => self.null(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(b) => Err(JsonError(format!("unexpected byte {:?}", b as char))),
            None => Err(JsonError("unexpected end of input".into())),
        }
    }

    fn object(&mut self) -> Result<PJson, JsonError> {
        self.pos += 1; // {
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(PJson::Obj(map));
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
                    return Ok(PJson::Obj(map));
                }
                _ => return Err(JsonError("expected ',' or '}'".into())),
            }
        }
    }

    fn array(&mut self) -> Result<PJson, JsonError> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(PJson::Arr(items));
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
                    return Ok(PJson::Arr(items));
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

    fn number(&mut self) -> Result<PJson, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        // The fraction/exponent tail — `.`/`e`/`E` opens it, a `+`/`-`
        // binds only immediately after an exponent marker. The whole
        // lexeme scans under every mode; strictness lives in the
        // `project` step (`parse` refuses `Num`, `parse_provider`
        // folds it to `Str`, `parse_preserve` keeps it typed).
        let mut is_float = false;
        while let Some(b) = self.peek() {
            let opens = b == b'.' || b == b'e' || b == b'E';
            let exp_sign = (b == b'+' || b == b'-')
                && is_float
                && matches!(
                    self.bytes.get(self.pos.wrapping_sub(1)),
                    Some(b'e') | Some(b'E')
                );
            if !(opens || exp_sign) {
                break;
            }
            is_float = true;
            self.pos += 1;
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        if is_float {
            return Ok(PJson::Num(text.to_string()));
        }
        text.parse::<i64>()
            .map(PJson::Int)
            .map_err(|_| JsonError(format!("bad integer {text:?}")))
    }

    fn boolean(&mut self) -> Result<PJson, JsonError> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(PJson::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(PJson::Bool(false))
        } else {
            Err(JsonError("bad literal".into()))
        }
    }

    fn null(&mut self) -> Result<PJson, JsonError> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(PJson::Null)
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
