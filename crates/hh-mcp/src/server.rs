//! The newline-delimited JSON-RPC serve loop the `hh-mcp-serve` binary
//! wraps (binding (b)-style framing: one JSON-RPC message per line —
//! the same framing `hh-embed`'s stdio binding uses, so the conformance
//! driver speaks one dialect).
//!
//! Served surface (Stage 3; R-2.11.3⁰ + the R-2.5.4⁰ edge slice, S3.9):
//! - `initialize` → `{protocolVersion, capabilities{tools{listChanged}},
//!   serverInfo}` — the negotiated echo is the client's requested
//!   version when pinned, else the server's preferred pin.
//! - `server/discover` → `{artifact, binding, protocol_version,
//!   supportedVersions, capabilities, ttlMs, cacheScope}` — byte-
//!   identical per bundle (AC-R-2.11.3-1). **Modern era only** — a
//!   legacy-era peer answers `-32601` (the client's compatibility probe
//!   detects it, ADR-0099 N1).
//! - `tools/list` → `{tools[], nextCursor, ttlMs, cacheScope: "private",
//!   resultType: "complete"}` — the canonical catalogue; the legacy
//!   projection drops `resultType`/`ttlMs`/`cacheScope` (the D2 loss
//!   class `narrowed`).
//! - `tools/call` → typed `isError` refusals (`NoCoveringGrant`,
//!   `HandleExpired`, `unknown_tool`, `stage_pending`), the fixture's
//!   `input_required` paused shape and the `requestState` resume echo
//!   (ADR-0097 D3) — execution is not a Stage-3 verb.
//! - `ping` → `{}`. `notifications/*` → no response.
//! - unknown method → `-32601`; a line that is not a request → `-32700`.
//!
//! `_meta` on any request is parsed and *dropped* — claims never
//! decide (AC-R-2.11.3-6: the scripted-call property is byte-identical
//! under arbitrary caller `_meta`).
//!
//! [`serve_dynamic`] re-lowers the artifact per iteration through the
//! caller's loader; a `catalogue_hash` change emits
//! `notifications/tools/list_changed` **before** the next answer —
//! `listChanged` only on bundle change (AC-R-2.5.4-2).

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, Write};

use hh_wire::json::Json;

use crate::artifact::{tools_call, ServedArtifact};
use crate::protocol::{PINNED_MODERN, PINNED_VERSIONS};

/// The server's reported name/version.
const SERVER_NAME: &str = "hh-mcp-serve";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The served era — `Modern` answers `server/discover` and the full
/// `tools/list` freshness/type metadata; `Legacy` is the compatibility
/// profile (`initialize`-only handshake, bare list shape — the loss
/// class `narrowed`, §5d.4's legacy-era projection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeMode {
    /// The pinned modern revision.
    Modern,
    /// The pinned legacy revision — `server/discover` is not a legacy
    /// method; `initialize` echoes the legacy pin.
    Legacy,
}

/// A serve-loop failure (transport only — request-level failures are
/// JSON-RPC errors on the wire, never a loop abort).
#[derive(Debug)]
pub enum ServeError {
    /// The stdio pair failed.
    Io(std::io::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::Io(e) => write!(f, "stdio: {e}"),
        }
    }
}

impl std::error::Error for ServeError {}

/// One JSON-RPC error frame (the `pub(crate)` spelling `error_frame_pub`
/// exists for the HTTP transport's `-32700` path).
fn error_frame(id: Json, code: i64, message: &str) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        (
            "error",
            Json::obj([("code", Json::Int(code)), ("message", Json::str(message))]),
        ),
    ])
}

/// One JSON-RPC result frame.
/// `error_frame` for the HTTP transport (crate-internal).
pub(crate) fn error_frame_pub(id: Json, code: i64, message: &str) -> Json {
    error_frame(id, code, message)
}

fn result_frame(id: Json, result: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        ("result", result),
    ])
}

/// One notification frame (no `id`).
fn notification(method: &str) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("method", Json::str(method.to_string())),
        ("params", Json::obj([])),
    ])
}

/// `server/discover` — the discovery document: the served artifact
/// verbatim, the `stdio_launch` binding (test principal), the pinned
/// `supportedVersions`, the capability declaration and the cache
/// hints. Byte-identical per bundle — every member is derived from
/// `(artifact, mode)` only.
fn discover(artifact: &ServedArtifact, list_changed: bool) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-discover/1")),
        ("artifact", artifact.to_json()),
        ("binding", crate::binding::stdio_launch_binding()),
        ("protocol_version", Json::str(PINNED_MODERN)),
        (
            "supportedVersions",
            Json::Arr(
                PINNED_VERSIONS
                    .iter()
                    .map(|v| Json::str(v.to_string()))
                    .collect(),
            ),
        ),
        (
            "capabilities",
            Json::obj([(
                "tools",
                Json::obj([("listChanged", Json::Bool(list_changed))]),
            )]),
        ),
        ("ttlMs", Json::Int(artifact.ttl_ms as i64)),
        ("cacheScope", Json::str("private")),
    ])
}

/// `initialize` result — the MCP handshake record. `requested` is the
/// client's offered `protocolVersion`: echoed when it is a member of
/// the pinned set; otherwise the server's preferred pin is answered
/// truthfully (the *client* owns the N2 refusal — a server never
/// guesses a version it does not speak).
fn initialize(requested: Option<&str>, mode: ServeMode, list_changed: bool) -> Json {
    let version = match mode {
        ServeMode::Legacy => crate::artifact::PROTOCOL_VERSION,
        ServeMode::Modern => match requested {
            Some(v) if PINNED_VERSIONS.contains(&v) => v,
            _ => PINNED_MODERN,
        },
    };
    Json::obj([
        ("protocolVersion", Json::str(version)),
        (
            "capabilities",
            Json::obj([(
                "tools",
                Json::obj([("listChanged", Json::Bool(list_changed))]),
            )]),
        ),
        (
            "serverInfo",
            Json::obj([
                ("name", Json::str(SERVER_NAME)),
                ("version", Json::str(SERVER_VERSION)),
            ]),
        ),
    ])
}

/// `tools/list` — `{tools[], nextCursor}` plus, in the modern era, the
/// freshness/type members (`ttlMs`, `cacheScope: "private"`,
/// `resultType: "complete"`). `nextCursor` is null (the catalogue is
/// always whole — a fixture never paginates). The legacy projection
/// drops the freshness/type members — `narrowed`, reported by the
/// binding's loss record.
fn tools_list(artifact: &ServedArtifact, mode: ServeMode) -> Json {
    let mut result = vec![
        (
            "tools",
            Json::Arr(
                artifact
                    .tools
                    .iter()
                    .map(|t| t.to_mcp_json(&artifact.bundle_id))
                    .collect(),
            ),
        ),
        ("nextCursor", Json::Null),
    ];
    if mode == ServeMode::Modern {
        result.push(("ttlMs", Json::Int(artifact.ttl_ms as i64)));
        result.push(("cacheScope", Json::str("private")));
        result.push(("resultType", Json::str("complete")));
    }
    Json::Obj(
        result
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Dispatch one request. Caller `_meta` (on `params` or the request
/// envelope) is *not read* — the two arms that could observe it
/// (`tools/call` params, the request object) never consult it, which is
/// the AC-R-2.11.3-6 property by construction.
#[allow(clippy::too_many_arguments)] // the frame's context is the record's shape.
fn dispatch(
    artifact: &ServedArtifact,
    mode: ServeMode,
    list_changed: bool,
    method: &str,
    params: &Json,
    id: Json,
    dedup: &mut BTreeMap<String, Json>,
    pending: &mut VecDeque<Json>,
) -> Json {
    match method {
        "initialize" => result_frame(
            id,
            initialize(
                params.get("protocolVersion").and_then(Json::as_str),
                mode,
                list_changed,
            ),
        ),
        "server/discover" | "discover" => match mode {
            ServeMode::Modern => result_frame(id, discover(artifact, list_changed)),
            ServeMode::Legacy => error_frame(id, -32601, "method_not_found: server/discover"),
        },
        "tools/list" => result_frame(id, tools_list(artifact, mode)),
        // `target` — the forwarded idempotency key (§5d.4 D4, R-2.2.2's
        // target-side rule): a second call under the same `target` replays
        // the recorded verdict verbatim — never a re-run (the edge's
        // executor-side dedup is a *recorded* replay, never a fresh
        // decision).
        "tools/call" => {
            let target = params.get("target").and_then(Json::as_str);
            match target {
                Some(t) if dedup.contains_key(t) => result_frame(id, dedup[t].clone()),
                Some(t) => {
                    let result = tools_call(artifact, params, now_ms());
                    dedup.insert(t.to_string(), result.clone());
                    result_frame(id, result)
                }
                None => result_frame(id, tools_call(artifact, params, now_ms())),
            }
        }
        // `subscriptions/listen` — the server→client stream drain
        // (§5d.4 D4): returns the queued notifications verbatim and
        // empties the queue (each is delivered exactly once).
        "subscriptions/listen" => result_frame(
            id,
            Json::obj([("notifications", Json::Arr(pending.drain(..).collect()))]),
        ),
        "ping" => result_frame(id, Json::obj([])),
        _ => error_frame(id, -32601, &format!("method_not_found: {method}")),
    }
}

/// `handle_message(msg) -> Option<response>` — the per-request half of
/// the serve loops (crate-internal; the stdio loop and the Streamable
/// HTTP loop share it). Notifications (`id` absent or
/// `notifications/*`) return `None`; a malformed line returns the
/// `-32700` frame.
pub(crate) fn handle_message(
    artifact: &ServedArtifact,
    mode: ServeMode,
    list_changed: bool,
    msg: &Json,
    dedup: &mut BTreeMap<String, Json>,
    pending: &mut VecDeque<Json>,
) -> Option<Json> {
    let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
    let id = msg.get("id").cloned().unwrap_or(Json::Null);
    let is_notification = !matches!(msg.get("id"), Some(Json::Int(_)) | Some(Json::Str(_)))
        || method.starts_with("notifications/");
    if is_notification {
        return None;
    }
    let params = msg.get("params").cloned().unwrap_or(Json::Null);
    Some(dispatch(
        artifact,
        mode,
        list_changed,
        method,
        &params,
        id,
        dedup,
        pending,
    ))
}

/// The wall clock for handle-expiry checks — injected for tests via
/// [`serve_with_clock`]; the binary uses the real clock.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `serve(artifact, reader, writer)` — the stdio loop: read one
/// newline-delimited JSON-RPC message per line, answer requests,
/// ignore notifications (`id` absent or `notifications/*`), EOF ends
/// the session. The loop is a pure function of `(artifact, lines)` —
/// no ledger, no kernel link, no environment reads. Modern era, fixed
/// artifact (`listChanged` is never emitted — the served bundle cannot
/// change under a static artifact).
pub fn serve(
    artifact: &ServedArtifact,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ServeError> {
    let owned = artifact.clone();
    serve_dynamic(
        &mut move || owned.clone(),
        ServeMode::Modern,
        reader,
        writer,
    )
}

/// `serve_dynamic(load, mode, reader, writer)` — the reloadable loop:
/// `load()` re-lowers the served artifact at the top of every
/// iteration; a `catalogue_hash` change writes
/// `notifications/tools/list_changed` before the next answer (the
/// `list_changed`-only-on-bundle-change rule, AC-R-2.5.4-2 — the
/// notification is a pure function of the artifact sequence, never of
/// requests). `load` returning the same artifact is the `serve` case.
///
/// The advertised `tools.listChanged` capability is `true` exactly when
/// the loader can produce a different artifact — the caller declares
/// it via `mode` (a fixed-artifact server still answers `true`: the
/// *method* is supported; the bundle simply never changes under it —
/// the served capability is honest either way).
pub fn serve_dynamic(
    load: &mut dyn FnMut() -> ServedArtifact,
    mode: ServeMode,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ServeError> {
    let mut artifact = load();
    let mut dedup: BTreeMap<String, Json> = BTreeMap::new();
    let mut pending: VecDeque<Json> = VecDeque::new();
    let mut line = String::new();
    loop {
        // A changed catalogue fires `listChanged` before anything else
        // this iteration writes (AC-R-2.5.4-2).
        let next = load();
        if next.catalogue_hash != artifact.catalogue_hash {
            let n = notification("notifications/tools/list_changed");
            writeln!(writer, "{}", n.to_canonical_string()).map_err(ServeError::Io)?;
            writer.flush().map_err(ServeError::Io)?;
            artifact = next;
        }
        line.clear();
        let n = reader.read_line(&mut line).map_err(ServeError::Io)?;
        if n == 0 {
            return Ok(()); // EOF — the session ends.
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg = match hh_wire::json::parse(trimmed) {
            Ok(m) => m,
            Err(_) => {
                let frame = error_frame(Json::Null, -32700, "parse_error");
                writeln!(writer, "{}", frame.to_canonical_string()).map_err(ServeError::Io)?;
                writer.flush().map_err(ServeError::Io)?;
                continue;
            }
        };
        let Some(frame) = handle_message(&artifact, mode, true, &msg, &mut dedup, &mut pending)
        else {
            continue; // notifications are never answered.
        };
        writeln!(writer, "{}", frame.to_canonical_string()).map_err(ServeError::Io)?;
        writer.flush().map_err(ServeError::Io)?;
    }
}

/// For tests: the same loop under a fixed clock is unnecessary — the
/// expiry probe uses `expires_at_ms` spelled against [`now_ms`]. The
/// constant is exported so tests can spell "already expired" without a
/// clock.
pub const EXPIRED_EPOCH_MS: u64 = 0;
