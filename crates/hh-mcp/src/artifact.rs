//! `hh-mcp-target/1` → `hh-mcp-artifact/1` — the compiled-bundle →
//! canonical MCP tool-catalogue lowering (§7.3 R-2.11.3⁰; ADR-0097 D7).
//!
//! The catalogue is a **pure function of the bundle member bytes** —
//! `tools/list` and `server/discover` are byte-identical for every
//! connection serving one bundle (AC-R-2.11.3-1), ordered by
//! `(semantic_id, name)` (§3.2.5's canonical-order rule). A tool's
//! `_meta` carries `dev.cognition/hir` (`{semantic_id, bundle_id}` — the
//! §3.2.5 minimum carried set) and every unknown member `_meta` key is
//! preserved verbatim (AC-R-2.11.3-6's extension rule).
//!
//! `<hh-prefix>` is unallocated upstream (OQ-068; ADR-0210) — the
//! fixture freezes `dev.cognition/hir` and the ADR-0275 ruling records
//! the placeholder.

use hh_wire::json::Json;

/// The served-artifact record's schema id.
pub const MCP_ARTIFACT_SCHEMA: &str = "hh-mcp-artifact/1";

/// The member document's schema id — a `target:mcp` bundle member is a
/// `hh-mcp-target/1` tool-catalogue document `{schema, target:"mcp",
/// tools[]}` (the S3.2 compile output's shape, hand-authored for the
/// fixture — the lowering is the same either way).
pub const MCP_TARGET_SCHEMA: &str = "hh-mcp-target/1";

/// The MCP protocol revision the fixture negotiates (`initialize`'s
/// `protocolVersion` echo — the fixture serves exactly one).
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The `_meta` key carrying the HIR semantic identity
/// (`{semantic_id, bundle_id}` — §3.2.5's minimum carried set; the
/// `<hh-prefix>` slot is OQ-068-unallocated — this is the fixture's
/// frozen spelling).
pub const HH_META_KEY: &str = "dev.cognition/hir";

/// The typed refusal/error sum the artifact + call path produce.
#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    /// The member bytes are not a well-formed `hh-mcp-target/1` document.
    Malformed {
        /// What was wrong.
        detail: String,
    },
    /// `tools/call` named a tool the catalogue does not carry.
    UnknownTool {
        /// The requested name.
        name: String,
    },
    /// A handle-bearing argument with no covering grant for this caller
    /// (every handle is foreign to the fixed test principal — R-4).
    NoCoveringGrant {
        /// The member path the handle rode in on.
        at: String,
    },
    /// A handle-bearing argument whose declared expiry has passed
    /// (`isError`, never a protocol error — AC-R-2.11.3-2).
    HandleExpired {
        /// The member path the handle rode in on.
        at: String,
    },
    /// `tools/call` is not a Stage-3 verb — the catalogue is served,
    /// execution is not (the typed `stage_pending` refusal).
    StagePending,
}

impl McpError {
    /// The typed refusal spelling the `isError` result carries.
    pub fn refusal(&self) -> &'static str {
        match self {
            McpError::Malformed { .. } => "malformed",
            McpError::UnknownTool { .. } => "unknown_tool",
            McpError::NoCoveringGrant { .. } => "NoCoveringGrant",
            McpError::HandleExpired { .. } => "HandleExpired",
            McpError::StagePending => "stage_pending",
        }
    }
}

/// One catalogue entry — an MCP `Tool` descriptor (`{name,
/// description?, inputSchema, _meta}`) with the HIR block pinned.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactTool {
    /// The surface name.
    pub name: String,
    /// The human description, when the member declared one.
    pub description: Option<String>,
    /// The JSON-schema input shape (`{}` when undeclared).
    pub input_schema: Json,
    /// The HIR semantic id (the `_meta[dev.cognition/hir].semantic_id`
    /// member — stable across renames; AC-R-2.11.3-1).
    pub semantic_id: String,
    /// The member's `_meta` verbatim minus the HIR block — unknown keys
    /// preserved, never interpreted.
    pub ext_meta: Json,
}

impl ArtifactTool {
    /// The MCP `Tool` JSON — `{name, description?, inputSchema, _meta}`.
    /// `_meta` = `ext_meta` ∪ `{dev.cognition/hir: {semantic_id,
    /// bundle_id}}` — the HIR block is always present and the member's
    /// own unknown `_meta` keys survive untouched.
    pub fn to_mcp_json(&self, bundle_id: &str) -> Json {
        let mut meta = match &self.ext_meta {
            Json::Obj(m) => m.clone(),
            _ => std::collections::BTreeMap::new(),
        };
        meta.insert(
            HH_META_KEY.to_string(),
            Json::obj([
                ("semantic_id", Json::str(self.semantic_id.clone())),
                ("bundle_id", Json::str(bundle_id.to_string())),
            ]),
        );
        let mut t = std::collections::BTreeMap::new();
        t.insert("name".to_string(), Json::str(self.name.clone()));
        if let Some(d) = &self.description {
            t.insert("description".to_string(), Json::str(d.clone()));
        }
        t.insert("inputSchema".to_string(), self.input_schema.clone());
        t.insert("_meta".to_string(), Json::Obj(meta));
        Json::Obj(t)
    }
}

/// The `hh-mcp-artifact/1` served-artifact record — what `lab.serve`
/// returns and `hh-mcp-serve` serves.
#[derive(Debug, Clone, PartialEq)]
pub struct ServedArtifact {
    /// The bundle id (`version_id`).
    pub bundle_id: String,
    /// The bundle's manifest version (same coordinate — recorded for
    /// the discover payload's readability).
    pub bundle_version: String,
    /// The canonical tool catalogue (`(semantic_id, name)` order).
    pub tools: Vec<ArtifactTool>,
    /// `idp/1` over the canonical `tools[]` projection — the
    /// listChanged/diff coordinate.
    pub catalogue_hash: String,
}

impl ServedArtifact {
    /// Canonical record JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(MCP_ARTIFACT_SCHEMA)),
            (
                "bundle",
                Json::obj([
                    ("id", Json::str(self.bundle_id.clone())),
                    ("version_id", Json::str(self.bundle_version.clone())),
                ]),
            ),
            (
                "tools",
                Json::Arr(
                    self.tools
                        .iter()
                        .map(|t| t.to_mcp_json(&self.bundle_id))
                        .collect(),
                ),
            ),
            ("catalogue_hash", Json::str(self.catalogue_hash.clone())),
        ])
    }
}

/// `lower(target:mcp member bytes, bundle_id, version_id) →
/// ServedArtifact` — the pure lowering. The member document is
/// `{schema: "hh-mcp-target/1", target?: "mcp", tools: [{name,
/// description?, inputSchema?, _meta?}]}`; anything else is `Malformed`.
pub fn lower_mcp_target(
    member_bytes: &[u8],
    bundle_id: &str,
    bundle_version: &str,
) -> Result<ServedArtifact, McpError> {
    let text = std::str::from_utf8(member_bytes).map_err(|_| McpError::Malformed {
        detail: "member is not utf-8".to_string(),
    })?;
    let doc = hh_wire::json::parse(text).map_err(|e| McpError::Malformed {
        detail: format!("member is not canonical json: {e}"),
    })?;
    let schema = doc
        .get("schema")
        .and_then(Json::as_str)
        .ok_or_else(|| McpError::Malformed {
            detail: "no schema member".to_string(),
        })?;
    if schema != MCP_TARGET_SCHEMA {
        return Err(McpError::Malformed {
            detail: format!("schema `{schema}` — expected {MCP_TARGET_SCHEMA}"),
        });
    }
    if let Some(t) = doc.get("target").and_then(Json::as_str) {
        if t != "mcp" {
            return Err(McpError::Malformed {
                detail: format!("target `{t}` — expected `mcp`"),
            });
        }
    }
    let tools_json = match doc.get("tools") {
        Some(Json::Arr(t)) => t.clone(),
        _ => {
            return Err(McpError::Malformed {
                detail: "no tools[] member".to_string(),
            })
        }
    };
    let mut tools = Vec::with_capacity(tools_json.len());
    for (i, t) in tools_json.iter().enumerate() {
        let name = t
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| McpError::Malformed {
                detail: format!("tools[{i}].name missing"),
            })?
            .to_string();
        let description = t
            .get("description")
            .and_then(Json::as_str)
            .map(String::from);
        let input_schema = t
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| Json::obj([]));
        let meta = t.get("_meta").cloned().unwrap_or_else(|| Json::obj([]));
        let (semantic_id, ext_meta) = match &meta {
            Json::Obj(m) => {
                let hir = m.get(HH_META_KEY);
                let sid = hir
                    .and_then(|h| h.get("semantic_id"))
                    .and_then(Json::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| name.clone());
                let mut ext = m.clone();
                ext.remove(HH_META_KEY);
                (sid, Json::Obj(ext))
            }
            _ => (name.clone(), Json::obj([])),
        };
        tools.push(ArtifactTool {
            name,
            description,
            input_schema,
            semantic_id,
            ext_meta,
        });
    }
    // Canonical order — `(semantic_id, name)` (§3.2.5's catalogue rule;
    // byte-identity across connections follows from it).
    tools.sort_by(|a, b| {
        a.semantic_id
            .cmp(&b.semantic_id)
            .then_with(|| a.name.cmp(&b.name))
    });
    let projection = Json::Arr(tools.iter().map(|t| t.to_mcp_json(bundle_id)).collect());
    let catalogue_hash =
        hh_identity::idp_id("mcp.catalogue", projection.to_canonical_string().as_bytes());
    Ok(ServedArtifact {
        bundle_id: bundle_id.to_string(),
        bundle_version: bundle_version.to_string(),
        tools,
        catalogue_hash,
    })
}

/// A handle-shaped argument value — a JSON object member named
/// `handle`/`handle_id`/`*_handle` or a bare `hnd-*`/`hnd_*` string.
/// Returns `(at_path, expired)` — `expired` when the handle document's
/// `expires_at` is in the past (or its `expired` marker is set — the
/// fixture's expiry spelling; AC-R-2.11.3-2).
fn scan_handles(j: &Json, path: &str, now_ms: u64) -> Option<(String, bool)> {
    fn handle_value(v: &Json, at: &str, now_ms: u64) -> Option<(String, bool)> {
        match v {
            Json::Str(s) if s.starts_with("hnd-") || s.starts_with("hnd_") => {
                Some((at.to_string(), false))
            }
            Json::Obj(m)
                if m.contains_key("handle")
                    || m.contains_key("handle_id")
                    || m.keys().any(|k| k.ends_with("_handle")) =>
            {
                let expired = m.get("expired") == Some(&Json::Bool(true))
                    || m.get("expires_at_ms")
                        .and_then(Json::as_int)
                        .map(|e| (e as u64) < now_ms)
                        .unwrap_or(false);
                Some((at.to_string(), expired))
            }
            _ => None,
        }
    }
    if let Some(hit) = handle_value(j, path, now_ms) {
        return Some(hit);
    }
    match j {
        Json::Obj(m) => m
            .iter()
            .find_map(|(k, v)| scan_handles(v, &format!("{path}/{k}"), now_ms)),
        Json::Arr(a) => a
            .iter()
            .enumerate()
            .find_map(|(i, v)| scan_handles(v, &format!("{path}[{i}]"), now_ms)),
        _ => None,
    }
}

/// `tools/call` — the Stage-3 refusal path (R-2.11.3⁰: the fixture
/// serves the catalogue; execution is not a Stage-3 verb — every call
/// is a typed `isError`, never a protocol error).
///
/// Decision order (never `_meta`-influenced — caller `_meta` is parsed
/// and dropped): unknown tool → `InvalidParams`-shaped refusal; a
/// handle-bearing argument → `HandleExpired` then `NoCoveringGrant`
/// (the fixed test principal holds no covering grant for any presented
/// handle — R-4); otherwise `stage_pending`.
///
/// Returns the `CallToolResult` JSON — `{isError: true, content:
/// [{type:"text", text}], structuredContent: {refusal, detail}}`; no
/// `HandleId` string ever appears in it (I-H1).
pub fn tools_call(artifact: &ServedArtifact, params: &Json, now_ms: u64) -> Json {
    let name = params.get("name").and_then(Json::as_str).unwrap_or("");
    let refusal = if !artifact.tools.iter().any(|t| t.name == name) {
        McpError::UnknownTool {
            name: name.to_string(),
        }
    } else {
        let args = params.get("arguments").cloned().unwrap_or(Json::Null);
        match scan_handles(&args, "/arguments", now_ms) {
            Some((at, true)) => McpError::HandleExpired { at },
            Some((at, false)) => McpError::NoCoveringGrant { at },
            None => McpError::StagePending,
        }
    };
    let detail = match &refusal {
        McpError::Malformed { detail } => detail.clone(),
        McpError::UnknownTool { name } => format!("unknown tool `{name}`"),
        McpError::NoCoveringGrant { at } => {
            format!("handle-bearing argument at {at} has no covering grant for this caller")
        }
        McpError::HandleExpired { at } => {
            format!("handle-bearing argument at {at} is expired")
        }
        McpError::StagePending => "execution is not a Stage-3 verb".to_string(),
    };
    // The typed refusal — `structuredContent` carries the record; the
    // text block is its canonical encoding (no handle id inside).
    let record = Json::obj([
        ("refusal", Json::str(refusal.refusal())),
        ("detail", Json::str(detail)),
    ]);
    Json::obj([
        ("isError", Json::Bool(true)),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                ("text", Json::str(record.to_canonical_string())),
            ])]),
        ),
        ("structuredContent", record),
    ])
}
