//! The lab server — the `hh-lab/1` instrument's live state + the
//! per-message JSON-RPC dispatch shared by stdio and Streamable HTTP.
//!
//! `LabServer` owns exactly four things:
//! - `svc` — the `EmbedService` (the kernel boundary the `surface_*`
//!   seam gates every durable write through);
//! - `exposure` + the lowered `ServedArtifact` (the pinned `hh-lab/1`
//!   catalogue — `catalogue_hash` is the lowerer's own hash);
//! - `sessions` — `binding_id → SurfaceSession` (lazy-opened on the
//!   first `tools/call`; a persisted `run_kind = surface` run for the
//!   same binding re-acquires, never respawns);
//! - `dedup`/`pending` — the `target`-keyed executor-side dedup and
//!   the notification queue (both are transports' contracts, not
//!   ledger state).
//!
//! The dispatch mirrors `hh-mcp::server::dispatch` — same methods, same
//! frames — with `tools/call` routed through [`crate::dispatch`] (the
//! surface-session chain) instead of the fixture's `stage_pending`.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, Write};

use hh_embed::service::EmbedService;
use hh_embed_schema::errors::EmbedError;
use hh_mcp::artifact::ServedArtifact;
use hh_mcp::protocol::{PINNED_MODERN, PINNED_VERSIONS};
use hh_mcp::server::ServeError;
use hh_wire::json::Json;

use crate::binding::{BindingTable, CallerAuth, CallerBinding};
use crate::exposure::{self, ExposureDef};
use crate::session::SurfaceSession;

/// The server name `initialize`/`serverInfo` reports.
pub const SERVER_NAME: &str = "hh-mcp-lab";
/// The package version.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The live server.
pub struct LabServer {
    /// The kernel boundary — every durable write is `surface_*`.
    pub svc: EmbedService,
    /// The linked `hh-lab/1` exposure definition.
    pub exposure: ExposureDef,
    /// The lowered catalogue (computed once at open).
    pub artifact: ServedArtifact,
    /// Bearer-token grants for HTTP callers (`stdio` resolves by
    /// inheritance — the OS identity IS the grant).
    pub bindings: BindingTable,
    /// `binding_id → surface session`.
    pub sessions: BTreeMap<String, SurfaceSession>,
    /// Executor-side dedup (`target` → recorded answer).
    pub dedup: BTreeMap<String, Json>,
    /// The notification queue (`subscriptions/listen` drains it).
    pub pending: VecDeque<Json>,
}

impl LabServer {
    /// Open over a service + exposure document (linked — a malformed
    /// document fails before anything durable exists).
    pub fn new(svc: EmbedService, exposure_doc: &Json) -> Result<LabServer, String> {
        let def = exposure::parse_exposure(exposure_doc).map_err(|e| e.to_string())?;
        Ok(Self::with_def(svc, def))
    }

    /// Open over an already-linked definition (the shipped default or a
    /// deployment's sealed document). HTTP grants register explicitly
    /// via [`LabServer::grant_bearer`] — a stdio binding needs none
    /// (the OS identity IS the grant).
    pub fn with_def(svc: EmbedService, def: ExposureDef) -> LabServer {
        let artifact = exposure::lower(&def);
        LabServer {
            svc,
            exposure: def,
            artifact,
            bindings: BindingTable::new(),
            sessions: BTreeMap::new(),
            dedup: BTreeMap::new(),
            pending: VecDeque::new(),
        }
    }

    /// Register one bearer→binding grant (the HTTP transport's
    /// `Authorization: Bearer` resolution — the deployment's own grant
    /// table; a grant is only ever a member of `exposure.bindings`,
    /// never an invented identity).
    pub fn grant_bearer(&mut self, token: &str, binding: CallerBinding) {
        self.bindings.grant(token, binding);
    }

    /// Resolve a bearer token — `None` ⇒ the HTTP arm answers the
    /// `401` challenge.
    pub fn resolve_bearer(&self, token: &str) -> Option<CallerBinding> {
        self.bindings.resolve(token)
    }

    /// The shipped `hh-lab/1` default over `svc` — tests + the binary.
    pub fn with_default_exposure(svc: EmbedService) -> LabServer {
        Self::with_def(svc, exposure::default_exposure())
    }

    /// The catalogue hash (the lowerer's own — the manifest records it).
    pub fn catalogue_hash(&self) -> &str {
        &self.artifact.catalogue_hash
    }

    /// The artefact a binding's `tools/list`/`server/discover` serves —
    /// the supply surface's own compiled `hh-mcp-target/1` minus its
    /// `hidden` Π rows for a supply binding (`callable ⇔ revealed`;
    /// AC-R-2.11.3-9), the Lab catalogue for everyone else.
    fn artifact_for(&self, binding: &CallerBinding) -> ServedArtifact {
        if let Some(surface) = self
            .exposure
            .supply_surfaces
            .iter()
            .find(|s| s.binding_id == binding.binding_id)
        {
            let hidden: std::collections::BTreeSet<&str> = surface
                .pi
                .iter()
                .filter(|r| r.hidden)
                .map(|r| r.match_.as_str())
                .collect();
            let hides_all = hidden.contains("*");
            let mut a = surface.artifact.clone();
            a.tools.retain(|t| {
                !hides_all
                    && !hidden.contains(t.name.as_str())
                    && !hidden.contains(t.semantic_id.as_str())
            });
            // The served hash follows the revealed set (two bindings
            // never share a stale catalogue hash).
            let projection = Json::Arr(
                a.tools
                    .iter()
                    .map(|t| t.to_mcp_json(&a.bundle_id))
                    .collect(),
            );
            a.catalogue_hash =
                hh_identity::idp_id("mcp.catalogue", projection.to_canonical_string().as_bytes());
            return a;
        }
        self.artifact.clone()
    }

    /// The caller's surface session — opened lazily. A persisted
    /// `run_kind = surface` run whose manifest names this binding
    /// re-acquires its writer lease (the post-crash rebind: the run is
    /// the session, never a second one). `ensure` is the open-or-rebind
    /// half; callers borrow `sessions[binding_id]` after (the field
    /// split keeps `svc` borrowable alongside).
    pub fn ensure_session(&mut self, binding: &CallerBinding) -> Result<(), EmbedError> {
        if !self.sessions.contains_key(&binding.binding_id) {
            // The persisted-run scan — `surface` runs whose manifest's
            // `caller_binding.binding_id` matches rebind to this caller.
            for run_id in self.svc.surface_run_ids() {
                let Ok(m) = self.svc.surface_manifest(&run_id) else {
                    continue;
                };
                let is_surface = matches!(m.run_kind, hh_ledger::manifest::RunKind::Surface)
                    || m.extra.get("run_kind").and_then(Json::as_str) == Some("surface");
                if !is_surface {
                    continue;
                }
                let names = m
                    .extra
                    .get("caller_binding")
                    .and_then(|c| c.get("binding_id"))
                    .and_then(Json::as_str);
                if names != Some(binding.binding_id.as_str()) {
                    continue;
                }
                // Rebind — the writer lease re-acquires (`takeover`
                // when the persisted holder differs); the table and
                // turn counter rebuild from durable rows.
                let lease = self.svc.surface_acquire_writer(&run_id, 60_000)?;
                let session =
                    SurfaceSession::resume(&mut self.svc, &run_id, binding.clone(), lease);
                self.sessions.insert(binding.binding_id.clone(), session);
                return Ok(());
            }
            let protocols: Vec<&str> = PINNED_VERSIONS.to_vec();
            let session = SurfaceSession::open(
                &mut self.svc,
                binding.clone(),
                &self.exposure.version_id,
                &self.artifact.catalogue_hash.clone(),
                &protocols,
            )?;
            self.sessions.insert(binding.binding_id.clone(), session);
        }
        Ok(())
    }

    /// One request → `Some(frame)`; notifications → `None`.
    /// `binding` is the transport-resolved caller (stdio's
    /// `stdio_launch` row or the HTTP bearer's grant).
    pub fn handle_message(&mut self, binding: &CallerBinding, msg: &Json) -> Option<Json> {
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(Json::Null);
        let is_notification = !matches!(msg.get("id"), Some(Json::Int(_)) | Some(Json::Str(_)))
            || method.starts_with("notifications/");
        if is_notification {
            return None;
        }
        let params = msg.get("params").cloned().unwrap_or(Json::Null);
        Some(self.dispatch(binding, method, &params, id))
    }

    /// The method dispatch — same members `hh-mcp` answers, the lab
    /// `tools/call` goes through the surface-session chain.
    fn dispatch(&mut self, binding: &CallerBinding, method: &str, params: &Json, id: Json) -> Json {
        match method {
            "initialize" => result_frame(id, initialize_result(params)),
            "server/discover" | "discover" => {
                let artifact = self.artifact_for(binding);
                result_frame(id, discover_result(&artifact, binding, &self.exposure))
            }
            "tools/list" => {
                let artifact = self.artifact_for(binding);
                result_frame(id, tools_list_result(&artifact))
            }
            "tools/call" => {
                // `target` — the forwarded idempotency key: a replay
                // under the same `target` answers the recorded verdict
                // verbatim (never a re-run, §5d.4 D4's target rule).
                let target = params.get("target").and_then(Json::as_str);
                match target {
                    Some(t) if self.dedup.contains_key(t) => {
                        result_frame(id, self.dedup[t].clone())
                    }
                    Some(t) => {
                        let result = self.call(binding, params);
                        self.dedup.insert(t.to_string(), result.clone());
                        result_frame(id, result)
                    }
                    None => result_frame(id, self.call(binding, params)),
                }
            }
            "subscriptions/listen" => result_frame(
                id,
                Json::obj([("notifications", Json::Arr(self.pending.drain(..).collect()))]),
            ),
            "ping" => result_frame(id, Json::obj([])),
            _ => error_frame(id, -32601, &format!("method_not_found: {method}")),
        }
    }

    /// `tools/call` — `{name, arguments}` through the chain.
    fn call(&mut self, binding: &CallerBinding, params: &Json) -> Json {
        let name = params.get("name").and_then(Json::as_str).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(Json::obj([]));
        let call_id = params.get("call_id").cloned().unwrap_or(Json::Null);
        crate::dispatch::call_tool(self, binding, name, arguments, &call_id)
    }
}

/// `initialize` — the handshake record (pinned-set echo, capabilities,
/// `serverInfo`).
fn initialize_result(params: &Json) -> Json {
    let requested = params.get("protocolVersion").and_then(Json::as_str);
    let version = match requested {
        Some(v) if PINNED_VERSIONS.contains(&v) => v,
        _ => PINNED_MODERN,
    };
    Json::obj([
        ("protocolVersion", Json::str(version)),
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

/// `server/discover` — the discover record: the artifact, the CALLER's
/// resolved binding (each caller sees its own), the exposure link, the
/// pinned versions.
fn discover_result(
    artifact: &ServedArtifact,
    binding: &CallerBinding,
    exposure: &ExposureDef,
) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-discover/1")),
        ("artifact", artifact.to_json()),
        ("binding", binding.to_json()),
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
            Json::obj([("tools", Json::obj([("listChanged", Json::Bool(false))]))]),
        ),
        ("ttlMs", Json::Int(artifact.ttl_ms as i64)),
        ("cacheScope", Json::str("private")),
        (
            "exposure",
            Json::obj([
                ("semantic_id", Json::str(exposure.semantic_id.clone())),
                ("version_id", Json::str(exposure.version_id.clone())),
                (
                    "assumption_debt",
                    Json::Arr(
                        exposure
                            .assumption_debt
                            .iter()
                            .map(|d| {
                                Json::obj([
                                    ("assumption_id", Json::str(d.assumption_id.clone())),
                                    ("carrier", Json::str(d.carrier.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    ])
}

/// `tools/list` — `{tools[], nextCursor, ttlMs, cacheScope, resultType}`.
fn tools_list_result(artifact: &ServedArtifact) -> Json {
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
        ("ttlMs", Json::Int(artifact.ttl_ms as i64)),
        ("cacheScope", Json::str("private")),
        ("resultType", Json::str("complete")),
    ])
}

/// One JSON-RPC error frame.
pub fn error_frame(id: Json, code: i64, message: &str) -> Json {
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
pub fn result_frame(id: Json, result: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id),
        ("result", result),
    ])
}

/// The stdio loop — newline-delimited JSON-RPC over `reader`/`writer`,
/// every message dispatched under `binding` (the stdio caller's
/// `stdio_launch` row — the OS identity IS the grant). EOF ends the
/// session; a parse failure answers `-32700` and the loop continues.
pub fn serve_stdio(
    srv: &mut LabServer,
    binding: &CallerBinding,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ServeError> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(ServeError::Io)?;
        if n == 0 {
            return Ok(());
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
        let Some(frame) = srv.handle_message(binding, &msg) else {
            continue;
        };
        writeln!(writer, "{}", frame.to_canonical_string()).map_err(ServeError::Io)?;
        writer.flush().map_err(ServeError::Io)?;
    }
}
