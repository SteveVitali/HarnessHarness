//! `hh-receipt-index/1` — the committed record of the §4.10 archive
//! (experiments spec §4.10; R6.12 / ARCHIVE (a); REQ-XF-124,
//! REQ-XH-101; AR-20). The index names every archived byte by
//! `{role, path, sha256, bytes}`: per slot-attempt `files[]` (roles
//! `raw_receipts | transcript | atif | egress_log | native_log |
//! store_segment`), per pass `artifacts[]` (roles `layer_export |
//! bundle | binary_set`, `upload ∈ archived | digest_only` — a
//! `digest_only` entry carries the digest of bytes that stay on the
//! operator controller, no uploaded path), `scans{secret_scan_ref,
//! contamination_scan_ref}`, and supersession (`supersedes[]`) and
//! tombstone (`tombstone{reason}`) entries (REQ-XH-104/107's shapes —
//! the acts themselves are R6.23's).
//!
//! The index and transcripts are derived views over durable receipts
//! and store segments — never an authoritative store (X-04). The codec
//! is closed: unknown members and wrong shapes refuse.

use std::collections::BTreeMap;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

/// The schema spelling.
pub const RECEIPT_INDEX_SCHEMA: &str = "hh-receipt-index/1";

/// The per-attempt file roles (REQ-XF-124).
pub const FILE_ROLES: &[&str] = &[
    "raw_receipts",
    "transcript",
    "atif",
    "egress_log",
    "native_log",
    "store_segment",
];

/// The pass-level artifact roles (REQ-XF-124, revised O3/RD-13).
pub const ARTIFACT_ROLES: &[&str] = &["layer_export", "bundle", "binary_set"];

/// A strict-decode refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexError {
    /// What failed.
    pub detail: String,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{RECEIPT_INDEX_SCHEMA}: {}", self.detail)
    }
}
impl std::error::Error for IndexError {}

fn err(detail: impl Into<String>) -> IndexError {
    IndexError {
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

fn req_obj<'a>(
    m: &'a BTreeMap<String, Json>,
    k: &str,
) -> Result<&'a BTreeMap<String, Json>, IndexError> {
    as_obj(m.get(k).unwrap_or(&Json::Null)).ok_or_else(|| err(format!("{k} must be an object")))
}

fn req_str(m: &BTreeMap<String, Json>, k: &str) -> Result<String, IndexError> {
    m.get(k)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| err(format!("{k} must be a string")))
}

fn opt_str(m: &BTreeMap<String, Json>, k: &str) -> Result<Option<String>, IndexError> {
    match m.get(k) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| err(format!("{k} must be a string or null"))),
    }
}

fn sha_ok(m: &BTreeMap<String, Json>, k: &str) -> Result<(), IndexError> {
    let s = req_str(m, k)?;
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(err(format!("{k} must be a 64-hex sha256")))
    }
}

fn check_only(m: &BTreeMap<String, Json>, allowed: &[&str], what: &str) -> Result<(), IndexError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(err(format!("{what} unknown member {k}")));
        }
    }
    Ok(())
}

/// `file_entry(role, path, bytes)` — one `files[]`/`store_segment`
/// member: the sha256 + byte count computed from the archived bytes.
pub fn file_entry(role: &str, path: &str, bytes: &[u8]) -> Result<Json, IndexError> {
    if !FILE_ROLES.contains(&role) {
        return Err(err(format!("file role {role} not in the closed set")));
    }
    Ok(Json::obj([
        ("role", Json::str(role)),
        ("path", Json::str(path)),
        ("sha256", Json::str(sha256_hex(bytes))),
        ("bytes", Json::Int(bytes.len() as i64)),
    ]))
}

/// `artifact_entry(role, name, window, id, path?, bytes, upload)` —
/// one pass-level `artifacts[]` member. `id` is the image id or digest
/// (the REQ-XC-330 identity the slot records); `upload: digest_only`
/// carries no uploaded path (the bytes stay on the controller).
pub fn artifact_entry(
    role: &str,
    name: &str,
    window: &str,
    id_member: (&str, &str),
    path: Option<&str>,
    bytes: &[u8],
    upload: &str,
) -> Result<Json, IndexError> {
    if !ARTIFACT_ROLES.contains(&role) {
        return Err(err(format!("artifact role {role} not in the closed set")));
    }
    if !["archived", "digest_only"].contains(&upload) {
        return Err(err(format!("upload {upload} not archived|digest_only")));
    }
    if upload == "archived" && path.is_none() {
        return Err(err("archived artifact requires a path"));
    }
    if upload == "digest_only" && path.is_some() {
        return Err(err("digest_only carries no uploaded path"));
    }
    let mut m = BTreeMap::new();
    m.insert("role".into(), Json::str(role));
    m.insert("name".into(), Json::str(name));
    m.insert("window".into(), Json::str(window));
    m.insert(id_member.0.into(), Json::str(id_member.1));
    m.insert("path".into(), path.map(Json::str).unwrap_or(Json::Null));
    m.insert("sha256".into(), Json::str(sha256_hex(bytes)));
    m.insert("bytes".into(), Json::Int(bytes.len() as i64));
    m.insert("upload".into(), Json::str(upload));
    Ok(Json::Obj(m))
}

/// `validate_index(doc)` — the closed-codec check: required members,
/// closed role/upload/visibility sets, sha256 shapes, the `digest_only`
/// no-uploaded-path rule, and unknown-member refusals.
pub fn validate_index(doc: &Json) -> Result<(), IndexError> {
    let m = as_obj(doc).ok_or_else(|| err("index must be an object"))?;
    check_only(
        m,
        &[
            "schema",
            "dataset",
            "attempts",
            "artifacts",
            "scans",
            "supersedes",
            "tombstone",
        ],
        "index",
    )?;
    if m.get("schema").and_then(Json::as_str) != Some(RECEIPT_INDEX_SCHEMA) {
        return Err(err(format!("schema must be {RECEIPT_INDEX_SCHEMA}")));
    }
    let ds = req_obj(m, "dataset")?;
    check_only(ds, &["repo", "revision", "visibility"], "dataset")?;
    req_str(ds, "repo")?;
    opt_str(ds, "revision")?;
    match req_str(ds, "visibility")?.as_str() {
        "private" | "public" => {}
        v => return Err(err(format!("visibility {v} not private|public"))),
    }
    for (ai, a) in as_arr(m.get("attempts").unwrap_or(&Json::Null))
        .ok_or_else(|| err("attempts must be an array"))?
        .iter()
        .enumerate()
    {
        let am = as_obj(a).ok_or_else(|| err(format!("attempts[{ai}] must be an object")))?;
        check_only(am, &["slot_id", "attempt", "files"], "attempt")?;
        req_str(am, "slot_id")?;
        if am.get("attempt").and_then(Json::as_int).is_none() {
            return Err(err(format!("attempts[{ai}].attempt must be an int")));
        }
        for (fi, f) in as_arr(am.get("files").unwrap_or(&Json::Null))
            .ok_or_else(|| err(format!("attempts[{ai}].files must be an array")))?
            .iter()
            .enumerate()
        {
            let fm = as_obj(f).ok_or_else(|| err(format!("files[{fi}] must be an object")))?;
            check_only(fm, &["role", "path", "sha256", "bytes"], "file")?;
            if !FILE_ROLES.contains(&req_str(fm, "role")?.as_str()) {
                return Err(err(format!("files[{fi}].role not in the closed set")));
            }
            req_str(fm, "path")?;
            sha_ok(fm, "sha256")?;
            if fm.get("bytes").and_then(Json::as_int).is_none() {
                return Err(err(format!("files[{fi}].bytes must be an int")));
            }
        }
    }
    match m.get("artifacts") {
        None | Some(Json::Null) => {}
        Some(v) => {
            for (ai, a) in as_arr(v)
                .ok_or_else(|| err("artifacts must be an array"))?
                .iter()
                .enumerate()
            {
                let am =
                    as_obj(a).ok_or_else(|| err(format!("artifacts[{ai}] must be an object")))?;
                check_only(
                    am,
                    &[
                        "role", "name", "window", "image_id", "digest", "path", "sha256", "bytes",
                        "upload",
                    ],
                    "artifact",
                )?;
                if !ARTIFACT_ROLES.contains(&req_str(am, "role")?.as_str()) {
                    return Err(err(format!("artifacts[{ai}].role not in the closed set")));
                }
                req_str(am, "name")?;
                req_str(am, "window")?;
                if am.get("image_id").is_none() && am.get("digest").is_none() {
                    return Err(err(format!("artifacts[{ai}] needs image_id or digest")));
                }
                let upload = req_str(am, "upload")?;
                if !["archived", "digest_only"].contains(&upload.as_str()) {
                    return Err(err(format!(
                        "artifacts[{ai}].upload not archived|digest_only"
                    )));
                }
                match (upload.as_str(), am.get("path")) {
                    ("archived", None) | ("archived", Some(Json::Null)) => {
                        return Err(err(format!("artifacts[{ai}] archived requires a path")))
                    }
                    ("digest_only", Some(p)) if p.as_str().is_some() => {
                        return Err(err(format!(
                            "artifacts[{ai}] digest_only carries no uploaded path"
                        )))
                    }
                    _ => {}
                }
                sha_ok(am, "sha256")?;
                if am.get("bytes").and_then(Json::as_int).is_none() {
                    return Err(err(format!("artifacts[{ai}].bytes must be an int")));
                }
            }
        }
    }
    let sc = req_obj(m, "scans")?;
    check_only(sc, &["secret_scan_ref", "contamination_scan_ref"], "scans")?;
    opt_str(sc, "secret_scan_ref")?;
    opt_str(sc, "contamination_scan_ref")?;
    if let Some(s) = m.get("supersedes") {
        for e in as_arr(s).ok_or_else(|| err("supersedes must be an array"))? {
            let em = as_obj(e).ok_or_else(|| err("supersedes entries are objects"))?;
            check_only(em, &["ref"], "supersedes")?;
            req_str(em, "ref")?;
        }
    }
    if let Some(t) = m.get("tombstone") {
        let tm = as_obj(t).ok_or_else(|| err("tombstone must be an object"))?;
        check_only(tm, &["reason"], "tombstone")?;
        let r = req_str(tm, "reason")?;
        if r.trim().is_empty() {
            return Err(err("tombstone.reason must not be empty"));
        }
    }
    Ok(())
}

/// `packet_fold(events)` — the committed-packet fold over a proxy
/// run's durable events `(class, payload)`: `consumed_micros` is Σ
/// `measurement.cost.attributed` micro-units, `calls` the terminal
/// count, `receipts` the `response_ref` list, `dispositions` the
/// terminal dispositions. Recomputing it from archived store segments
/// must equal the committed packet (REQ-XH-101's recompute leg).
pub fn packet_fold(events: &[(String, Json)]) -> Json {
    let mut consumed = 0i64;
    let mut calls = 0i64;
    let mut receipts = Vec::new();
    let mut dispositions = Vec::new();
    for (class, p) in events {
        match class.as_str() {
            "measurement.cost.attributed" => {
                consumed += p
                    .get("money")
                    .and_then(|m| m.get("micro_units"))
                    .and_then(Json::as_int)
                    .unwrap_or(0);
            }
            "model.call.completed" | "model.call.failed" => {
                calls += 1;
                if let Some(r) = p.get("response_ref").and_then(Json::as_str) {
                    receipts.push(Json::str(r));
                }
                dispositions.push(
                    p.get("disposition")
                        .and_then(Json::as_str)
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                );
            }
            _ => {}
        }
    }
    Json::obj([
        ("consumed_micros", Json::Int(consumed)),
        ("calls", Json::Int(calls)),
        ("receipts", Json::Arr(receipts)),
        ("dispositions", Json::Arr(dispositions)),
    ])
}
