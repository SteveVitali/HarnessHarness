//! The newline-delimited JSON-RPC serve loop the `hh-mcp-serve` binary
//! wraps (binding (b)-style framing: one JSON-RPC message per line —
//! the same framing `hh-embed`'s stdio binding uses, so the conformance
//! driver speaks one dialect).
//!
//! Served surface (Stage 3; R-2.11.3⁰):
//! - `initialize` → `{protocolVersion, capabilities{tools}, serverInfo}`.
//! - `server/discover` → `{artifact, binding, protocol_version, ttlMs,
//!   cacheScope}` — byte-identical per bundle (AC-R-2.11.3-1).
//! - `tools/list` → `{tools[], nextCursor}` — the canonical catalogue.
//! - `tools/call` → typed `isError` refusals only (`NoCoveringGrant`,
//!   `HandleExpired`, `unknown_tool`, `stage_pending`) — execution is
//!   not a Stage-3 verb.
//! - `ping` → `{}`. `notifications/*` → no response.
//! - unknown method → `-32601`; a line that is not a request → `-32700`.
//!
//! `_meta` on any request is parsed and *dropped* — claims never
//! decide (AC-R-2.11.3-6: the scripted-call property is byte-identical
//! under arbitrary caller `_meta`).

use std::io::{BufRead, Write};

use hh_wire::json::Json;

use crate::artifact::{tools_call, ServedArtifact, PROTOCOL_VERSION};

/// The server's reported name/version.
const SERVER_NAME: &str = "hh-mcp-serve";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

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

/// One JSON-RPC error frame.
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
fn result_frame(id: Json, result: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        ("result", result),
    ])
}

/// `server/discover` — the discovery document: the served artifact
/// verbatim, the `stdio_launch` binding (test principal), the protocol
/// revision and the cache declaration. Byte-identical per bundle —
/// every member is derived from `(artifact, binding)` only.
fn discover(artifact: &ServedArtifact) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-discover/1")),
        ("artifact", artifact.to_json()),
        ("binding", crate::binding::stdio_launch_binding()),
        ("protocol_version", Json::str(PROTOCOL_VERSION)),
        ("ttlMs", Json::Int(0)),
        ("cacheScope", Json::str("bundle")),
    ])
}

/// `initialize` result — the MCP handshake record.
fn initialize() -> Json {
    Json::obj([
        ("protocolVersion", Json::str(PROTOCOL_VERSION)),
        (
            "capabilities",
            Json::obj([("tools", Json::obj([("listChanged", Json::Bool(false))]))]),
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

/// `tools/list` — `{tools[], nextCursor}`; `nextCursor` is null (the
/// catalogue is always whole — a fixture never paginates).
fn tools_list(artifact: &ServedArtifact) -> Json {
    Json::obj([
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
    ])
}

/// Dispatch one request. Caller `_meta` (on `params` or the request
/// envelope) is *not read* — the two arms that could observe it
/// (`tools/call` params, the request object) never consult it, which is
/// the AC-R-2.11.3-6 property by construction.
fn dispatch(artifact: &ServedArtifact, method: &str, params: &Json, id: Json) -> Json {
    match method {
        "initialize" => result_frame(id, initialize()),
        "server/discover" | "discover" => result_frame(id, discover(artifact)),
        "tools/list" => result_frame(id, tools_list(artifact)),
        "tools/call" => result_frame(id, tools_call(artifact, params, now_ms())),
        "ping" => result_frame(id, Json::obj([])),
        _ => error_frame(id, -32601, &format!("method_not_found: {method}")),
    }
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
/// no ledger, no kernel link, no environment reads.
pub fn serve(
    artifact: &ServedArtifact,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ServeError> {
    let mut line = String::new();
    loop {
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
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(Json::Null);
        // Notifications — `id` absent or `notifications/*` — never
        // answer.
        let is_notification = !matches!(msg.get("id"), Some(Json::Int(_)) | Some(Json::Str(_)))
            || method.starts_with("notifications/");
        if is_notification {
            continue;
        }
        let params = msg.get("params").cloned().unwrap_or(Json::Null);
        let frame = dispatch(artifact, method, &params, id);
        writeln!(writer, "{}", frame.to_canonical_string()).map_err(ServeError::Io)?;
        writer.flush().map_err(ServeError::Io)?;
    }
}

/// For tests: the same loop under a fixed clock is unnecessary — the
/// expiry probe uses `expires_at_ms` spelled against [`now_ms`]. The
/// constant is exported so tests can spell "already expired" without a
/// clock.
pub const EXPIRED_EPOCH_MS: u64 = 0;
