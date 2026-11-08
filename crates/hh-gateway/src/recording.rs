//! `recording` — `hh.model_proxy.recording/1`, the one recording
//! format (experiments spec §4.9; R6.12 / REPLAY; REQ-XC-122, absorbs
//! REQ-XD-108; design/33 F15; design/36 §1.3, §1.6 P1–P9).
//!
//! A recording is the ordered set of raw upstream exchanges of a slot
//! plus the downstream request bytes: per call `k`, the downstream
//! request (capture point 1), the upstream-bound request after the
//! lane's transforms (capture point 2 — what the model conditions on),
//! the upstream response (status, headers, body, per-chunk arrival
//! stamps), the released downstream bytes and the §4.3 disposition.
//! `provenance` ∈ `recorded | synthetic` — a synthetic sequence is the
//! same document authored by hand (P6).
//!
//! The codec is closed: unknown members refuse, never silently carry.
//! The document codec's one home is this module (CC1): the proxy is
//! the sole *writer* of captured manifests but every consumer —
//! `hharness-hosting` (record/seal/replay) and `hharness-runner` (the
//! L1 corpus and gate, R6.28; the runner may not edge into hosting
//! under REQ-XB-103) — reads and authors through it.

use std::collections::BTreeMap;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

/// The recording schema spelling.
pub const RECORDING_SCHEMA: &str = "hh.model_proxy.recording/1";

/// A strict-decode or shape refusal on the recording format.
#[derive(Debug, Clone)]
pub struct RecordingError {
    /// What failed.
    pub detail: String,
}

impl std::fmt::Display for RecordingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{RECORDING_SCHEMA}: {}", self.detail)
    }
}
impl std::error::Error for RecordingError {}

fn err(detail: impl Into<String>) -> RecordingError {
    RecordingError {
        detail: detail.into(),
    }
}

fn as_obj(j: &Json) -> Option<&BTreeMap<String, Json>> {
    match j {
        Json::Obj(m) => Some(m),
        _ => None,
    }
}

fn as_arr(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

/// One recorded upstream response (verbatim capture).
#[derive(Debug, Clone)]
pub struct RecordedResponse {
    /// The upstream status (`0` = the head never arrived).
    pub status: u16,
    /// The upstream response headers (lowercased names).
    pub headers: BTreeMap<String, String>,
    /// The body bytes verbatim.
    pub body: Vec<u8>,
    /// `(arrival_ms, byte_len)` per body chunk — replay re-chunks a
    /// streaming release by these lengths (P7's declared rule).
    pub chunk_stamps: Vec<(u64, usize)>,
}

/// One recorded downstream release (what the harness's socket read).
#[derive(Debug, Clone)]
pub struct RecordedReleased {
    /// The released status.
    pub status: u16,
    /// The released headers (the outbound allowlist's kept set).
    pub headers: BTreeMap<String, String>,
    /// The released body bytes.
    pub body: Vec<u8>,
}

/// One recorded call — `hh.model_proxy.recording/1`'s `calls[k]`.
#[derive(Debug, Clone)]
pub struct RecordedCall {
    /// The sequence index.
    pub k: u64,
    /// Capture point 1 — the downstream request line + body bytes.
    pub downstream_method: String,
    /// The downstream path.
    pub downstream_path: String,
    /// The downstream request body bytes.
    pub downstream_body: Vec<u8>,
    /// Capture point 2 — the upstream-bound body after the lane's
    /// transforms (`None` when the call refused before dispatch).
    pub upstream_request: Option<Vec<u8>>,
    /// The recorded upstream exchange (`None` when no response was
    /// captured — a refusal or a head that never arrived).
    pub upstream_response: Option<RecordedResponse>,
    /// A recorded transport failure `(kind, detail)` — `kind` ∈
    /// `timeout | network | other`.
    pub failure: Option<(String, String)>,
    /// The recorded downstream release (`None` when the call never
    /// reached release — e.g. `client_gone` wrote nothing).
    pub released: Option<RecordedReleased>,
    /// The §4.3 receipt disposition (`forwarded`, `upstream_error`,
    /// `refused_*`, …).
    pub disposition: String,
    /// The recorded call latency in ms (P9 input).
    pub latency_ms: u64,
}

/// A `hh.model_proxy.recording/1` manifest — one slot's ordered calls.
#[derive(Debug, Clone)]
pub struct Recording {
    /// The slot the sequence belongs to.
    pub slot_id: String,
    /// `recorded` (a `record`-mode capture) | `synthetic` (P6).
    pub provenance: String,
    /// The ordered calls.
    pub calls: Vec<RecordedCall>,
}

/// `body_json(bytes)` — the closed byte codec: UTF-8 bodies carry
/// `utf8` (the common case — every lane body is JSON or SSE), anything
/// else carries lowercase `hex`.
pub fn body_json(bytes: &[u8]) -> Json {
    match std::str::from_utf8(bytes) {
        Ok(s) => Json::obj([("utf8", Json::str(s))]),
        Err(_) => Json::obj([("hex", Json::str(hex(bytes)))]),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, RecordingError> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(err("body.hex: odd or non-hex literal"));
    }
    Ok((0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0))
        .collect())
}

fn body_bytes(j: &Json) -> Result<Vec<u8>, RecordingError> {
    if let Some(s) = j.get("utf8").and_then(Json::as_str) {
        return Ok(s.as_bytes().to_vec());
    }
    if let Some(s) = j.get("hex").and_then(Json::as_str) {
        return unhex(s);
    }
    Err(err("body must carry utf8 or hex"))
}

fn sha(v: &[u8]) -> Json {
    Json::str(format!("sha256:{}", sha256_hex(v)))
}

fn resp_json(r: &RecordedResponse) -> Json {
    Json::obj([
        ("status", Json::Int(r.status as i64)),
        (
            "headers",
            Json::Obj(
                r.headers
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
        ("body_sha256", sha(&r.body)),
        ("body", body_json(&r.body)),
        (
            "chunk_stamps",
            Json::Arr(
                r.chunk_stamps
                    .iter()
                    .map(|(ms, n)| Json::Arr(vec![Json::Int(*ms as i64), Json::Int(*n as i64)]))
                    .collect(),
            ),
        ),
    ])
}

fn req_json(bytes: &[u8]) -> Json {
    Json::obj([("body_sha256", sha(bytes)), ("body", body_json(bytes))])
}

fn call_json(c: &RecordedCall) -> Json {
    Json::obj([
        ("k", Json::Int(c.k as i64)),
        (
            "downstream",
            Json::obj([
                ("method", Json::str(&c.downstream_method)),
                ("path", Json::str(&c.downstream_path)),
                ("body_sha256", sha(&c.downstream_body)),
                ("body", body_json(&c.downstream_body)),
            ]),
        ),
        (
            "upstream_request",
            c.upstream_request
                .as_ref()
                .map(|b| req_json(b))
                .unwrap_or(Json::Null),
        ),
        (
            "upstream_response",
            c.upstream_response
                .as_ref()
                .map(resp_json)
                .unwrap_or(Json::Null),
        ),
        (
            "failure",
            c.failure
                .as_ref()
                .map(|(k, d)| Json::obj([("kind", Json::str(k)), ("detail", Json::str(d))]))
                .unwrap_or(Json::Null),
        ),
        (
            "released",
            c.released
                .as_ref()
                .map(|r| {
                    Json::obj([
                        ("status", Json::Int(r.status as i64)),
                        (
                            "headers",
                            Json::Obj(
                                r.headers
                                    .iter()
                                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                                    .collect(),
                            ),
                        ),
                        ("body_sha256", sha(&r.body)),
                        ("body", body_json(&r.body)),
                    ])
                })
                .unwrap_or(Json::Null),
        ),
        ("disposition", Json::str(&c.disposition)),
        ("latency_ms", Json::Int(c.latency_ms as i64)),
    ])
}

impl Recording {
    /// The canonical `hh.model_proxy.recording/1` document.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(RECORDING_SCHEMA)),
            ("slot_id", Json::str(&self.slot_id)),
            ("provenance", Json::str(&self.provenance)),
            (
                "calls",
                Json::Arr(self.calls.iter().map(call_json).collect()),
            ),
        ])
    }

    /// The strict decode — every member checked, unknown members
    /// refused (the closed codec; synthetic sequences must spell the
    /// format exactly, P6).
    pub fn parse(doc: &Json) -> Result<Recording, RecordingError> {
        let m = as_obj(doc).ok_or_else(|| err("recording must be an object"))?;
        for k in m.keys() {
            if !["schema", "slot_id", "provenance", "calls"].contains(&k.as_str()) {
                return Err(err(format!("unknown member {k}")));
            }
        }
        if m.get("schema").and_then(Json::as_str) != Some(RECORDING_SCHEMA) {
            return Err(err(format!("schema must be {RECORDING_SCHEMA}")));
        }
        let slot_id = m
            .get("slot_id")
            .and_then(Json::as_str)
            .ok_or_else(|| err("slot_id must be a string"))?
            .to_string();
        let provenance = m
            .get("provenance")
            .and_then(Json::as_str)
            .ok_or_else(|| err("provenance must be a string"))?
            .to_string();
        if !["recorded", "synthetic"].contains(&provenance.as_str()) {
            return Err(err(format!(
                "provenance {provenance} not recorded|synthetic"
            )));
        }
        let calls = m
            .get("calls")
            .and_then(as_arr)
            .ok_or_else(|| err("calls must be an array"))?;
        let mut out = Vec::with_capacity(calls.len());
        for (i, c) in calls.iter().enumerate() {
            out.push(parse_call(c, i)?);
        }
        Ok(Recording {
            slot_id,
            provenance,
            calls: out,
        })
    }
}

fn member<'a>(m: &'a BTreeMap<String, Json>, name: &str) -> Result<&'a Json, RecordingError> {
    m.get(name)
        .ok_or_else(|| err(format!("call missing {name}")))
}

fn headers_of(j: &Json) -> Result<BTreeMap<String, String>, RecordingError> {
    let mut out = BTreeMap::new();
    if let Some(h) = j.get("headers") {
        let hm = as_obj(h).ok_or_else(|| err("headers must be an object"))?;
        for (k, v) in hm {
            out.insert(
                k.clone(),
                v.as_str()
                    .ok_or_else(|| err("header value must be a string"))?
                    .to_string(),
            );
        }
    }
    Ok(out)
}

fn parse_call(c: &Json, pos: usize) -> Result<RecordedCall, RecordingError> {
    let m = as_obj(c).ok_or_else(|| err(format!("call[{pos}] must be an object")))?;
    for k in m.keys() {
        if ![
            "k",
            "downstream",
            "upstream_request",
            "upstream_response",
            "failure",
            "released",
            "disposition",
            "latency_ms",
        ]
        .contains(&k.as_str())
        {
            return Err(err(format!("call[{pos}] unknown member {k}")));
        }
    }
    let k = member(m, "k")?
        .as_int()
        .ok_or_else(|| err(format!("call[{pos}].k must be an int")))? as u64;
    let dn = member(m, "downstream")?;
    let dn = as_obj(dn).ok_or_else(|| err(format!("call[{pos}].downstream must be an object")))?;
    let downstream_body = body_bytes(
        dn.get("body")
            .ok_or_else(|| err(format!("call[{pos}].downstream.body missing")))?,
    )?;
    let upstream_request = match m.get("upstream_request") {
        None | Some(Json::Null) => None,
        Some(u) => Some(body_bytes(u.get("body").ok_or_else(|| {
            err(format!("call[{pos}].upstream_request.body missing"))
        })?)?),
    };
    let upstream_response = match m.get("upstream_response") {
        None | Some(Json::Null) => None,
        Some(u) => {
            let status = u
                .get("status")
                .and_then(Json::as_int)
                .ok_or_else(|| err(format!("call[{pos}].upstream_response.status missing")))?
                as u16;
            let body = body_bytes(
                u.get("body")
                    .ok_or_else(|| err(format!("call[{pos}].upstream_response.body missing")))?,
            )?;
            let mut stamps = Vec::new();
            if let Some(Json::Arr(ss)) = u.get("chunk_stamps") {
                for s in ss {
                    let pair =
                        as_arr(s).ok_or_else(|| err("chunk_stamps entries must be pairs"))?;
                    if pair.len() != 2 {
                        return Err(err("chunk_stamps entries must be pairs"));
                    }
                    stamps.push((
                        pair[0].as_int().unwrap_or(0) as u64,
                        pair[1].as_int().unwrap_or(0) as usize,
                    ));
                }
            }
            Some(RecordedResponse {
                status,
                headers: headers_of(u)?,
                body,
                chunk_stamps: stamps,
            })
        }
    };
    let failure = match m.get("failure") {
        None | Some(Json::Null) => None,
        Some(f) => Some((
            f.get("kind")
                .and_then(Json::as_str)
                .unwrap_or("other")
                .to_string(),
            f.get("detail")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        )),
    };
    let released = match m.get("released") {
        None | Some(Json::Null) => None,
        Some(r) => Some(RecordedReleased {
            status: r
                .get("status")
                .and_then(Json::as_int)
                .ok_or_else(|| err(format!("call[{pos}].released.status missing")))?
                as u16,
            headers: headers_of(r)?,
            body: body_bytes(
                r.get("body")
                    .ok_or_else(|| err(format!("call[{pos}].released.body missing")))?,
            )?,
        }),
    };
    let disposition = m
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap_or("forwarded")
        .to_string();
    let latency_ms = m.get("latency_ms").and_then(Json::as_int).unwrap_or(0) as u64;
    Ok(RecordedCall {
        k,
        downstream_method: dn
            .get("method")
            .and_then(Json::as_str)
            .unwrap_or("POST")
            .to_string(),
        downstream_path: dn
            .get("path")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        downstream_body,
        upstream_request,
        upstream_response,
        failure,
        released,
        disposition,
        latency_ms,
    })
}
