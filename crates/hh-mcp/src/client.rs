//! The dual-era, probe-first MCP stdio **client** (§5d.4; ADR-0097 D2,
//! ADR-0099 N1–N4; ticket S3.9).
//!
//! Negotiation (N1 — probe first, then pin):
//! 1. `server/discover` goes out before any other request. A peer
//!    answering it reports `supportedVersions`; the client pins the
//!    highest-preference member of the pinned set it admits
//!    (`2026-07-28` modern, `2025-06-18` legacy).
//! 2. A peer that answers `server/discover` with `-32601` is driven
//!    through the compatibility probe — `initialize` at the modern pin;
//!    the `protocolVersion` echo decides the era.
//! 3. Anything outside the pinned set is `ProtocolVersionMismatch` — a
//!    typed error, never a silent fallback (N2). A peer that never
//!    answers is `PeerUnreachable`.
//!
//! In the modern era every request carries
//! `_meta.io.modelcontextprotocol/{protocolVersion, clientCapabilities,
//! clientInfo}` (D2); the legacy era sends bare params. Caller `_meta`
//! the *peer* returns is preserved byte-for-byte in `ext` by the lift
//! path (N4) — never interpreted (T2).
//!
//! `tools/list` is bounded: `MAX_PAGES`/`MAX_LISTING_BYTES` ceilings and
//! duplicate-cursor detection fail `IndexOverflow` (ADR-0095 D1) — a
//! hostile or buggy server can never pin the client in a paging loop.
//!
//! `tools/call` maps to the closed [`ToolOutcome`] sum — a JSON-RPC
//! error frame is `protocol_error`, an `isError` result is `failed`,
//! `resultType: "input_required"` is `paused` (the effect stays
//! non-terminal; `requestState` is an opaque blob echoed unmodified on
//! retry), and an untyped result is `unknown` — the two failure shapes
//! are never conflated (ADR-0097 D3–D4).

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use hh_wire::json::Json;

use crate::protocol::{
    pin_version, reconcile_capabilities, schema_content_hash, CapabilitySupport, Era,
    NegotiateError, ProtocolBinding, PINNED_LEGACY, PINNED_MODERN,
};

/// The `server/discover` method (the fixture/extension probe — sent
/// before any other request on stdio, ADR-0097 D2).
const DISCOVER_METHOD: &str = "server/discover";

/// The compatibility-probe method a discover-less peer answers.
const INITIALIZE_METHOD: &str = "initialize";

/// `tools/list` paging ceiling — `IndexOverflow{pages}` past it.
pub const MAX_PAGES: u64 = 64;

/// `tools/list` byte ceiling — `IndexOverflow{bytes}` past it.
pub const MAX_LISTING_BYTES: usize = 4 * 1024 * 1024;

/// This client's identity (display-only on the wire; T2).
const CLIENT_NAME: &str = "hh-mcp-client";

/// The client's declared capabilities — `elicitation.form` only at C0
/// (roots/sampling deprecated, ADR-0097 D2).
fn client_capabilities() -> Json {
    Json::obj([("elicitation", Json::obj([("form", Json::obj([]))]))])
}

/// The `_meta` block modern-era requests carry (D2).
fn request_meta(version: &str) -> Json {
    Json::obj([
        (
            "io.modelcontextprotocol/protocolVersion",
            Json::str(version.to_string()),
        ),
        (
            "io.modelcontextprotocol/clientCapabilities",
            client_capabilities(),
        ),
        (
            "io.modelcontextprotocol/clientInfo",
            Json::obj([
                ("name", Json::str(CLIENT_NAME)),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
    ])
}

/// A line transport — one newline-delimited JSON-RPC message per line
/// (the same framing [`crate::server::serve`] speaks). `recv` returns
/// `Ok(None)` on EOF; `Err` on a transport fault.
pub trait Transport {
    /// Write one line.
    fn send(&mut self, line: &str) -> Result<(), String>;
    /// Read one line (`None` = the peer closed).
    fn recv(&mut self) -> Result<Option<String>, String>;
}

/// The stdio transport — the client half of a spawned server process.
/// Reads are blocking: a server that never answers surfaces as EOF
/// (`PeerUnreachable`) once its stdout closes; a hung server is the
/// caller's timeout, not the edge's (T6 — the edge itself holds no
/// timer state).
pub struct StdioTransport {
    /// The server process (kept alive for the session's length).
    pub child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdioTransport {
    /// Spawn `cmd` piped on stdin/stdout (stderr inherited by the
    /// caller's policy — diagnostics never interleave with protocol
    /// bytes).
    pub fn spawn(cmd: &mut Command) -> std::io::Result<StdioTransport> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(StdioTransport {
            child,
            stdin,
            stdout,
        })
    }
}

impl Transport for StdioTransport {
    fn send(&mut self, line: &str) -> Result<(), String> {
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| format!("stdin: {e}"))
    }

    fn recv(&mut self) -> Result<Option<String>, String> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line)),
            Err(e) => Err(format!("stdout: {e}")),
        }
    }
}

/// The client's typed failure sum — `negotiate`'s three refuse shapes
/// plus the transport/codec/paging halves (never a panic, never an
/// untyped string).
#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// The transport failed (spawn/write/read).
    Transport {
        /// What happened.
        detail: String,
    },
    /// A frame arrived that isn't a well-formed JSON-RPC response.
    Malformed {
        /// What was wrong.
        detail: String,
    },
    /// A JSON-RPC error frame answered a non-`tools/call` request.
    JsonRpc {
        /// The error code.
        code: i64,
        /// The message.
        message: String,
    },
    /// The negotiated version is outside the pinned set (N2).
    ProtocolVersionMismatch {
        /// What was requested.
        requested: String,
        /// What the peer supports.
        supported: Vec<String>,
    },
    /// A capability the client requires is `unsupported`/`unknown`.
    RequiredExtensionUnsupported {
        /// The capability field.
        uri: String,
    },
    /// The peer never answered — EOF or a transport fault mid-probe.
    PeerUnreachable {
        /// What happened.
        detail: String,
    },
    /// Listing pagination exceeded the page/byte ceilings or repeated
    /// a cursor (`sync_source`'s bounded-pagination rule, ADR-0095 D1).
    IndexOverflow {
        /// `pages` | `bytes` | `duplicate_cursor`.
        detail: String,
    },
}

impl From<NegotiateError> for ClientError {
    fn from(e: NegotiateError) -> ClientError {
        match e {
            NegotiateError::ProtocolVersionMismatch {
                requested,
                supported,
            } => ClientError::ProtocolVersionMismatch {
                requested,
                supported,
            },
            NegotiateError::RequiredExtensionUnsupported { uri } => {
                ClientError::RequiredExtensionUnsupported { uri }
            }
            NegotiateError::PeerUnreachable { detail } => ClientError::PeerUnreachable { detail },
        }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Transport { detail } => write!(f, "transport: {detail}"),
            ClientError::Malformed { detail } => write!(f, "malformed: {detail}"),
            ClientError::JsonRpc { code, message } => write!(f, "json-rpc {code}: {message}"),
            ClientError::ProtocolVersionMismatch {
                requested,
                supported,
            } => write!(
                f,
                "ProtocolVersionMismatch{{requested: {requested}, supported: {supported:?}}}"
            ),
            ClientError::RequiredExtensionUnsupported { uri } => {
                write!(f, "RequiredExtensionUnsupported{{{uri}}}")
            }
            ClientError::PeerUnreachable { detail } => write!(f, "PeerUnreachable{{{detail}}}"),
            ClientError::IndexOverflow { detail } => write!(f, "IndexOverflow{{{detail}}}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// The closed `ToolOutcome` sum (`call_tool`'s result — §5d.4 §2;
/// ADR-0097 D3–D4).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolOutcome {
    /// A completed call — `resultType ∈ {observed, complete}` or
    /// untyped; the result body verbatim.
    Observed {
        /// The `CallToolResult` verbatim.
        result: Json,
    },
    /// `resultType: "input_required"` — a **paused** effect, never
    /// terminal: `inputRequests` is the server's ask list,
    /// `requestState` the opaque blob the retry echoes unmodified.
    Paused {
        /// The `inputRequests` array (verbatim).
        input_requests: Json,
        /// The opaque `requestState` blob.
        request_state: Json,
    },
    /// `isError: true` — a typed refusal/result failure; the content
    /// becomes an `Observation` (`action.tool.completed{status: failed}`
    /// — never a protocol error).
    Failed {
        /// The `CallToolResult` verbatim.
        result: Json,
    },
    /// A JSON-RPC **error frame** — `action.tool.rejected{source:
    /// protocol}`; never conflated with `Failed`.
    ProtocolError {
        /// The error code.
        code: i64,
        /// The message.
        message: String,
    },
    /// A result the client cannot classify (unknown `resultType`,
    /// malformed shape) — honest `unknown`, never guessed.
    Unknown {
        /// The `CallToolResult` verbatim.
        result: Json,
    },
}

/// One bounded `tools/list` read — the merged tool array plus the
/// freshness/type metadata the last page carried.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    /// The merged `tools[]` (canonical order as served).
    pub tools: Vec<Json>,
    /// `ttlMs` (freshness hint — `None` when the peer omits it).
    pub ttl_ms: Option<i64>,
    /// `cacheScope` (`private` when the plan depends on the caller's
    /// authorization — ADR-0095 D5).
    pub cache_scope: Option<String>,
    /// `resultType` (modern-era marker; absent in the legacy era).
    pub result_type: Option<String>,
    /// Pages consumed.
    pub pages: u64,
}

/// The negotiated client — probe-first (N1), pinned (N2), honest about
/// capability silence (N3). The `surface_name → (server_ref,
/// semantic_id)` map the caller keeps is a lookup over `tools[]` —
/// names are never parsed (D6).
pub struct McpClient<T: Transport> {
    transport: T,
    next_id: u64,
    /// The minted binding record (`negotiated_at` stays `None` — the
    /// caller stamps the `EventRef` when the ledger row lands).
    binding: ProtocolBinding,
    /// The `server/discover` document verbatim (carries the artifact +
    /// freshness hints the caller projects).
    discover_doc: Option<Json>,
    /// Server-initiated notifications received while awaiting a
    /// response (`notifications/tools/list_changed` et al) — drained by
    /// [`Self::drain_notifications`]; never answered, never dropped.
    notifications: Vec<Json>,
}

impl<T: Transport> McpClient<T> {
    /// `negotiate(edge_spec, peer_locator)` over an open transport —
    /// probe-first: `server/discover` before any other request; the
    /// compatibility probe (`initialize` at the modern pin) detects a
    /// legacy server; the minted [`ProtocolBinding`] records the era,
    /// the negotiated version and the reconciled capability map.
    pub fn connect(transport: T) -> Result<McpClient<T>, ClientError> {
        let mut probe = McpClient {
            transport,
            next_id: 0,
            binding: ProtocolBinding {
                protocol: "mcp".to_string(),
                role: "client".to_string(),
                spec_version_label: "mcp".to_string(),
                spec_schema_content_hash: schema_content_hash(
                    "mcp",
                    &crate::protocol::PINNED_VERSIONS,
                ),
                era: Era::Modern,
                negotiated_version: PINNED_MODERN.to_string(),
                peer_info: Json::Null,
                capabilities_declared: Json::Null,
                capabilities_observed: Default::default(),
                extensions: Vec::new(),
                transport: "stdio".to_string(),
                request_target: None,
                negotiated_at: None,
            },
            discover_doc: None,
            notifications: Vec::new(),
        };
        // N1 — `server/discover` before any other request.
        match probe.raw_request(DISCOVER_METHOD, Json::obj([]))? {
            Ok(discover) => {
                let supported = supported_versions(&discover);
                let version = pin_version(&supported)?;
                probe.binding.era = Era::of_version(&version).unwrap_or(Era::Modern);
                probe.binding.negotiated_version = version.clone();
                probe.discover_doc = Some(discover.clone());
                // The handshake confirms the pin — an echo outside the
                // pinned set is the same typed error (no silent drift).
                let init = probe.initialize(&version)?;
                let echo = init
                    .get("protocolVersion")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if echo != version {
                    return Err(ClientError::ProtocolVersionMismatch {
                        requested: version,
                        supported: vec![echo.to_string()],
                    });
                }
                probe.finish_binding(&init);
            }
            Err(err) => {
                // The compatibility probe: a discover-less peer is asked
                // `initialize` at the modern pin; the echo decides.
                if !matches!(err, ClientError::JsonRpc { code: -32601, .. }) {
                    return Err(err);
                }
                let init = probe.initialize(PINNED_MODERN)?;
                let echo = init
                    .get("protocolVersion")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let era = Era::of_version(&echo).ok_or({
                    ClientError::ProtocolVersionMismatch {
                        requested: PINNED_MODERN.to_string(),
                        supported: vec![echo.clone()],
                    }
                })?;
                probe.binding.era = era;
                probe.binding.negotiated_version = echo;
                probe.finish_binding(&init);
            }
        }
        Ok(probe)
    }

    /// The minted binding.
    pub fn binding(&self) -> &ProtocolBinding {
        &self.binding
    }

    /// The `server/discover` document verbatim (`None` for a
    /// compatibility-probed legacy peer).
    pub fn discover_doc(&self) -> Option<&Json> {
        self.discover_doc.as_ref()
    }

    /// Drain queued server notifications — `notifications/tools/
    /// list_changed` is a `sync_source` trigger (ADR-0095 D1); the
    /// caller decides what to do with each.
    pub fn drain_notifications(&mut self) -> Vec<Json> {
        std::mem::take(&mut self.notifications)
    }

    /// `tools/list` — bounded pagination: `nextCursor` follows until
    /// absent; a repeated cursor or a listing past `MAX_PAGES`/
    /// `MAX_LISTING_BYTES` is `IndexOverflow`.
    pub fn list_tools(&mut self) -> Result<Listing, ClientError> {
        let mut tools = Vec::new();
        let mut pages = 0u64;
        let mut bytes = 0usize;
        let mut seen_cursors: BTreeSet<String> = BTreeSet::new();
        let mut cursor: Option<String> = None;
        let mut ttl_ms = None;
        let mut cache_scope = None;
        let mut result_type = None;
        loop {
            let mut params = Json::obj([]);
            if let Some(c) = &cursor {
                params = Json::obj([("cursor", Json::str(c.clone()))]);
            }
            let result = self.request("tools/list", params)?;
            pages += 1;
            if pages > MAX_PAGES {
                return Err(ClientError::IndexOverflow {
                    detail: format!("pages>{MAX_PAGES}"),
                });
            }
            bytes += result.to_canonical_string().len();
            if bytes > MAX_LISTING_BYTES {
                return Err(ClientError::IndexOverflow {
                    detail: format!("bytes>{MAX_LISTING_BYTES}"),
                });
            }
            match result.get("tools") {
                Some(Json::Arr(t)) => tools.extend(t.iter().cloned()),
                _ => {
                    return Err(ClientError::Malformed {
                        detail: "tools/list result carries no tools[]".to_string(),
                    })
                }
            }
            if let Some(v) = result.get("ttlMs").and_then(Json::as_int) {
                ttl_ms = Some(v);
            }
            if let Some(v) = result.get("cacheScope").and_then(Json::as_str) {
                cache_scope = Some(v.to_string());
            }
            if let Some(v) = result.get("resultType").and_then(Json::as_str) {
                result_type = Some(v.to_string());
            }
            match result.get("nextCursor").and_then(Json::as_str) {
                None => break,
                Some(next) => {
                    // A cursor must be new and non-empty — a repeated
                    // cursor is a protocol fault, not a slow peer.
                    if !seen_cursors.insert(next.to_string()) || Some(next) == cursor.as_deref() {
                        return Err(ClientError::IndexOverflow {
                            detail: format!("duplicate_cursor {next}"),
                        });
                    }
                    cursor = Some(next.to_string());
                }
            }
        }
        Ok(Listing {
            tools,
            ttl_ms,
            cache_scope,
            result_type,
            pages,
        })
    }

    /// `call_tool(handle, (server_ref, semantic_id), args, …)`'s wire
    /// half: `tools/call{name, arguments, requestState?}` → the closed
    /// [`ToolOutcome`] sum. `request_state` is the opaque blob a paused
    /// effect's retry echoes unmodified (ADR-0097 D3).
    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: &Json,
        request_state: Option<&Json>,
    ) -> Result<ToolOutcome, ClientError> {
        self.call_tool_keyed(name, arguments, request_state, None)
    }

    /// `call_tool_keyed(name, arguments, request_state, target)` — the
    /// §5d.4 `target` idempotency-key forwarding (R-2.2.2's target-side
    /// rule): `params.target` carries the kernel-derived key verbatim;
    /// the server's dedup slot replays the recorded verdict on a repeat.
    /// The kernel's `effect_id`/`attempt_no` identity is *derived*
    /// upstream (the key) — `target` is its edge-facing echo, never a
    /// second identity source.
    pub fn call_tool_keyed(
        &mut self,
        name: &str,
        arguments: &Json,
        request_state: Option<&Json>,
        target: Option<&str>,
    ) -> Result<ToolOutcome, ClientError> {
        let mut params = vec![
            ("name", Json::str(name.to_string())),
            ("arguments", arguments.clone()),
        ];
        if let Some(rs) = request_state {
            params.push(("requestState", rs.clone()));
        }
        if let Some(t) = target {
            params.push(("target", Json::str(t.to_string())));
        }
        let params = Json::Obj(
            params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        );
        match self.raw_request("tools/call", params)? {
            Ok(result) => Ok(classify_call(&result)),
            Err(ClientError::JsonRpc { code, message }) => {
                Ok(ToolOutcome::ProtocolError { code, message })
            }
            Err(e) => Err(e),
        }
    }

    /// One request → one result. Notifications and mismatched ids are
    /// skipped (a stray line is never mistaken for an answer); EOF is
    /// `PeerUnreachable`. `_meta` rides on every request in the modern
    /// era (D2) — absent in the legacy era.
    fn raw_request(
        &mut self,
        method: &str,
        params: Json,
    ) -> Result<Result<Json, ClientError>, ClientError> {
        self.next_id += 1;
        let id = self.next_id;
        let mut frame = vec![
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(id as i64)),
            ("method", Json::str(method.to_string())),
            ("params", params),
        ];
        if self.binding.era == Era::Modern
            && method != DISCOVER_METHOD
            && method != INITIALIZE_METHOD
        {
            frame.push(("_meta", request_meta(&self.binding.negotiated_version)));
        }
        let msg = Json::Obj(frame.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
        self.transport
            .send(&msg.to_canonical_string())
            .map_err(|detail| ClientError::Transport { detail })?;
        loop {
            let line = self
                .transport
                .recv()
                .map_err(|detail| ClientError::PeerUnreachable { detail })?;
            let Some(line) = line else {
                return Err(ClientError::PeerUnreachable {
                    detail: "eof mid-request".to_string(),
                });
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp = hh_wire::json::parse(trimmed).map_err(|e| ClientError::Malformed {
                detail: format!("response is not canonical json: {e}"),
            })?;
            // Notifications and foreign ids are not answers — a
            // server notification is queued (never dropped, N4's
            // preservation rule applies to the edge too).
            match resp.get("id") {
                Some(Json::Int(i)) if *i == id as i64 => {}
                Some(Json::Str(s)) if s.as_str() == id.to_string() => {}
                _ => {
                    if resp.get("method").is_some() {
                        self.notifications.push(resp.clone());
                    }
                    continue;
                }
            }
            if let Some(err) = resp.get("error") {
                let code = err.get("code").and_then(Json::as_int).unwrap_or(0);
                let message = err
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                return Ok(Err(ClientError::JsonRpc { code, message }));
            }
            return Ok(Ok(resp.get("result").cloned().unwrap_or(Json::Null)));
        }
    }

    /// A request whose error frame is a typed refusal, not an outcome
    /// (the `tools/call` path uses it — everything else goes through
    /// [`Self::request`]).
    fn request(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        match self.raw_request(method, params)? {
            Ok(result) => Ok(result),
            Err(e) => Err(e),
        }
    }

    /// The `initialize` handshake — params carry `protocolVersion`,
    /// `capabilities`, `clientInfo`; in the modern era the D2 `_meta`
    /// block rides alongside.
    fn initialize(&mut self, version: &str) -> Result<Json, ClientError> {
        let mut params = vec![
            ("protocolVersion", Json::str(version.to_string())),
            ("capabilities", client_capabilities()),
            (
                "clientInfo",
                Json::obj([
                    ("name", Json::str(CLIENT_NAME)),
                    ("version", Json::str(env!("CARGO_PKG_VERSION"))),
                ]),
            ),
        ];
        if self.binding.era == Era::Modern {
            params.push(("_meta", request_meta(version)));
        }
        self.request(
            INITIALIZE_METHOD,
            Json::Obj(
                params
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
        )
    }

    /// `binding.request_target` — the transport's request URI (the
    /// Streamable HTTP endpoint; RFC 8707's `resource` indicator when
    /// the edge runs OAuth). Recorded on the minted binding; never
    /// consulted for a decision.
    pub fn set_request_target(&mut self, target: &str) {
        self.binding.request_target = Some(target.to_string());
    }

    /// Fill `capabilities_declared`/`capabilities_observed`/`peer_info`
    /// on the binding from the `initialize` answer; the `tools`
    /// capability is required — `unknown`/`unsupported` fails
    /// `RequiredExtensionUnsupported` (N2, fail closed on silence).
    fn finish_binding(&mut self, init: &Json) {
        let declared = init.get("capabilities").cloned().unwrap_or(Json::obj([]));
        self.binding.peer_info = init.get("serverInfo").cloned().unwrap_or(Json::Null);
        self.binding.capabilities_declared = declared.clone();
        let probed: BTreeSet<String> = [
            "tools",
            "tools.listChanged",
            "elicitation.form",
            "elicitation.url",
            "resources",
            "prompts",
            "logging",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        self.binding.capabilities_observed = reconcile_capabilities(&declared, &probed);
    }
}

/// Map a `CallToolResult` to the closed sum (ADR-0097 D3–D4): `isError`
/// → `failed`; `resultType: "input_required"` → `paused`; typed
/// `observed`/`complete`/absent → `observed`; anything else → `unknown`
/// (never crashed on, never conflated).
fn classify_call(result: &Json) -> ToolOutcome {
    if result.get("isError") == Some(&Json::Bool(true)) {
        return ToolOutcome::Failed {
            result: result.clone(),
        };
    }
    match result.get("resultType").and_then(Json::as_str) {
        Some("input_required") => ToolOutcome::Paused {
            input_requests: result
                .get("inputRequests")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
            request_state: result.get("requestState").cloned().unwrap_or(Json::Null),
        },
        Some("observed") | Some("complete") | None => ToolOutcome::Observed {
            result: result.clone(),
        },
        Some(_) => ToolOutcome::Unknown {
            result: result.clone(),
        },
    }
}

/// The `supportedVersions` member of a `server/discover` result —
/// `supportedVersions[]` when present, else the singleton
/// `[protocol_version]` (a peer that reports one version reports it
/// here).
fn supported_versions(discover: &Json) -> Vec<String> {
    match discover.get("supportedVersions") {
        Some(Json::Arr(vs)) => vs
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => discover
            .get("protocol_version")
            .and_then(Json::as_str)
            .map(|v| vec![v.to_string()])
            .unwrap_or_default(),
    }
}

/// `CapabilitySupport` at a dotted field — `list_tools`'s caller checks
/// `tools.listChanged`/`ttlMs` this way (omitted ⇒ `unknown`, never
/// `unsupported` — N3).
pub fn observed(b: &ProtocolBinding, field: &str) -> CapabilitySupport {
    b.capabilities_observed
        .get(field)
        .copied()
        .unwrap_or(CapabilitySupport::Unknown)
}

/// The legacy pinned version — exported for tests/fixtures.
pub const LEGACY_VERSION: &str = PINNED_LEGACY;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A scripted transport — `send` records the request, `recv` pops
    /// the canned answers.
    struct Script {
        sent: Vec<String>,
        replies: VecDeque<String>,
    }

    impl Script {
        fn new(replies: &[&str]) -> Script {
            Script {
                sent: Vec::new(),
                replies: replies.iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl Transport for Script {
        fn send(&mut self, line: &str) -> Result<(), String> {
            self.sent.push(line.to_string());
            Ok(())
        }
        fn recv(&mut self) -> Result<Option<String>, String> {
            Ok(self.replies.pop_front())
        }
    }

    fn result_frame(id: u64, result: Json) -> String {
        Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(id as i64)),
            ("result", result),
        ])
        .to_canonical_string()
    }

    #[test]
    fn probe_first_then_pin_modern() {
        let discover = Json::obj([
            ("protocol_version", Json::str(PINNED_MODERN)),
            (
                "supportedVersions",
                Json::Arr(vec![Json::str(PINNED_MODERN), Json::str(PINNED_LEGACY)]),
            ),
            (
                "capabilities",
                Json::obj([("tools", Json::obj([("listChanged", Json::Bool(true))]))]),
            ),
        ]);
        let init = Json::obj([
            ("protocolVersion", Json::str(PINNED_MODERN)),
            (
                "capabilities",
                Json::obj([("tools", Json::obj([("listChanged", Json::Bool(true))]))]),
            ),
            ("serverInfo", Json::obj([("name", Json::str("srv"))])),
        ]);
        let s = Script::new(&[&result_frame(1, discover), &result_frame(2, init)]);
        let c = McpClient::connect(s).expect("connect");
        assert_eq!(c.binding.era, Era::Modern);
        assert_eq!(c.binding.negotiated_version, PINNED_MODERN);
        // Probe-first: the first line out is server/discover.
        let first: Json = hh_wire::json::parse(&c.transport.sent[0]).unwrap();
        assert_eq!(
            first.get("method").and_then(Json::as_str),
            Some("server/discover")
        );
    }

    #[test]
    fn discover_less_peer_probes_legacy() {
        // `server/discover` → -32601; `initialize` echoes the legacy pin.
        let err = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(1)),
            (
                "error",
                Json::obj([
                    ("code", Json::Int(-32601)),
                    ("message", Json::str("method_not_found")),
                ]),
            ),
        ])
        .to_canonical_string();
        let init = result_frame(
            2,
            Json::obj([
                ("protocolVersion", Json::str(PINNED_LEGACY)),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
            ]),
        );
        let s = Script::new(&[&err, &init]);
        let c = McpClient::connect(s).expect("connect");
        assert_eq!(c.binding.era, Era::Legacy);
        assert_eq!(c.binding.negotiated_version, PINNED_LEGACY);
    }

    #[test]
    fn unpinned_version_is_a_typed_refusal() {
        let discover = result_frame(
            1,
            Json::obj([(
                "supportedVersions",
                Json::Arr(vec![Json::str("1999-01-01")]),
            )]),
        );
        let s = Script::new(&[&discover]);
        let err = McpClient::connect(s).err().expect("mismatch");
        assert!(matches!(err, ClientError::ProtocolVersionMismatch { .. }));
    }

    #[test]
    fn eof_is_peer_unreachable() {
        let s = Script::new(&[]);
        let err = McpClient::connect(s).err().expect("unreachable");
        assert!(matches!(err, ClientError::PeerUnreachable { .. }));
    }

    #[test]
    fn duplicate_cursor_is_index_overflow() {
        let discover = result_frame(
            1,
            Json::obj([
                (
                    "supportedVersions",
                    Json::Arr(vec![Json::str(PINNED_MODERN)]),
                ),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
            ]),
        );
        let init = result_frame(
            2,
            Json::obj([
                ("protocolVersion", Json::str(PINNED_MODERN)),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
            ]),
        );
        // Two pages carrying the same `nextCursor` — a fault, not paging.
        let page = result_frame(
            3,
            Json::obj([
                ("tools", Json::Arr(vec![])),
                ("nextCursor", Json::str("c1")),
            ]),
        );
        let page2 = result_frame(
            4,
            Json::obj([
                ("tools", Json::Arr(vec![])),
                ("nextCursor", Json::str("c1")),
            ]),
        );
        let s = Script::new(&[&discover, &init, &page, &page2]);
        let mut c = McpClient::connect(s).expect("connect");
        let err = c.list_tools().unwrap_err();
        assert!(matches!(err, ClientError::IndexOverflow { .. }));
    }

    #[test]
    fn call_tool_outcome_sum_never_conflates() {
        let discover = result_frame(
            1,
            Json::obj([
                (
                    "supportedVersions",
                    Json::Arr(vec![Json::str(PINNED_MODERN)]),
                ),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
            ]),
        );
        let init = result_frame(
            2,
            Json::obj([
                ("protocolVersion", Json::str(PINNED_MODERN)),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
            ]),
        );
        let paused = result_frame(
            3,
            Json::obj([
                ("resultType", Json::str("input_required")),
                ("inputRequests", Json::Arr(vec![Json::str("which?")])),
                ("requestState", Json::str("opaque-blob")),
            ]),
        );
        let failed = result_frame(
            4,
            Json::obj([
                ("isError", Json::Bool(true)),
                ("content", Json::Arr(vec![])),
            ]),
        );
        let proto = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(5)),
            (
                "error",
                Json::obj([
                    ("code", Json::Int(-32602)),
                    ("message", Json::str("invalid params")),
                ]),
            ),
        ])
        .to_canonical_string();
        let weird = result_frame(6, Json::obj([("resultType", Json::str("something_new"))]));
        let s = Script::new(&[&discover, &init, &paused, &failed, &proto, &weird]);
        let mut c = McpClient::connect(s).expect("connect");
        match c.call_tool("t", &Json::obj([]), None).unwrap() {
            ToolOutcome::Paused { request_state, .. } => {
                assert_eq!(request_state, Json::str("opaque-blob"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            c.call_tool("t", &Json::obj([]), None).unwrap(),
            ToolOutcome::Failed { .. }
        ));
        assert!(matches!(
            c.call_tool("t", &Json::obj([]), None).unwrap(),
            ToolOutcome::ProtocolError { code: -32602, .. }
        ));
        assert!(matches!(
            c.call_tool("t", &Json::obj([]), None).unwrap(),
            ToolOutcome::Unknown { .. }
        ));
    }
}
