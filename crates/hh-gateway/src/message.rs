//! `ModelMessage{blocks[]}` — the gateway's output record (§5b.1 data model;
//! ADR-0118 d.5): typed blocks — `TextBlock{text, signature?}` ·
//! `ReasoningBlock{text?, signature?, redacted, owner}` · `ToolCallBlock{…,
//! arguments: Parsed | Unparseable | Raw, provider_index, truncated}` ·
//! `ProviderOpaqueBlock{kind, payload, owner}` — with `stop_reason`,
//! `stop_details?`, `raw_stop_reason?`, `usage`, `served_model?`,
//! `snapshot_id?`, `surface_ids`, `provenance`.
//!
//! Opaque leaves ride as `OpaqueLeaf{format_tag, bytes_hash, owner{provider,
//! model}, provenance}` — never interpreted by the gateway (D1 decides
//! replay/drop on re-lowering); the codec emits them byte-for-byte, never
//! fabricates them.

use hh_wire::json::Json;

use crate::errors::CodecError;
use crate::vocab::{BlockKind, StopDetails, StopReason};

/// `Text(content, authority, trust, visibility)` — provider bytes the gateway
/// moves are `authority = external` (model text is `authority = delegate`
/// under ADR-0033 minting — the gateway stamps the record; the runtime's
/// minting is upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    /// The content.
    pub content: String,
    /// `authority` — who wrote it.
    pub authority: TextAuthority,
    /// `trust` — how the kernel may treat it.
    pub trust: TextTrust,
    /// `visibility`.
    pub visibility: TextVisibility,
}

/// `authority ∈ {external, internal, delegate}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAuthority {
    /// Provider-produced — `external` (explanation strings, notices).
    External,
    /// Kernel-produced — `internal`.
    Internal,
    /// Model-produced — `delegate` (ADR-0033 minting).
    Delegate,
}

impl TextAuthority {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TextAuthority::External => "external",
            TextAuthority::Internal => "internal",
            TextAuthority::Delegate => "delegate",
        }
    }
}

/// `trust ∈ {ground, external, unknown}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTrust {
    /// Kernel ground truth.
    Ground,
    /// External — read as text, never trusted as fact.
    External,
    /// Unattributed.
    Unknown,
}

impl TextTrust {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TextTrust::Ground => "ground",
            TextTrust::External => "external",
            TextTrust::Unknown => "unknown",
        }
    }
}

/// `visibility ∈ {public, internal, redacted}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVisibility {
    /// Public.
    Public,
    /// Internal.
    Internal,
    /// Redacted — the bytes are withheld; the fact they existed is not.
    Redacted,
}

impl TextVisibility {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TextVisibility::Public => "public",
            TextVisibility::Internal => "internal",
            TextVisibility::Redacted => "redacted",
        }
    }
}

/// `OpaqueLeaf{format_tag, bytes_hash, owner{provider, model}, provenance}` —
/// the WS-A3 `CompiledPayload`-class opaque block (§5b.1 `ModelMessage` row).
/// The gateway never interprets it; the in-memory form carries `bytes` (the
/// ledgered form carries the hash — the bytes are blob-addressed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueLeaf {
    /// `format_tag` — the provider's declared payload tag.
    pub format_tag: String,
    /// The payload bytes (moved byte-for-byte; never fabricated).
    pub bytes: Vec<u8>,
    /// `owner ∈ {provider, model}` — who the leaf belongs to.
    pub owner: String,
    /// `provenance` — a `ProvenanceRecord` reference.
    pub provenance: Option<String>,
}

impl OpaqueLeaf {
    /// `bytes_hash` — the content address (the ledgered form).
    pub fn bytes_hash(&self) -> String {
        hh_identity::idp_id("opaque_leaf.1", &self.bytes)
    }
}

/// `arguments ∈ {Parsed | Unparseable{raw, reason} | Raw}` (T2/T7).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolArgs {
    /// `Parsed` — the strict parse succeeded; canonical form.
    Parsed(Json),
    /// `Unparseable{raw, reason}` — never dropped, never repaired.
    Unparseable {
        /// The raw argument bytes.
        raw: String,
        /// `reason ∈ {invalid_json, truncated, empty}`.
        reason: UnparseableReason,
    },
    /// `Raw(str)` — reserved for the C2 `freeform` interaction mode (T7).
    Raw(String),
}

/// `Unparseable.reason ∈ {invalid_json, truncated, empty}` (T2/T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnparseableReason {
    /// The bytes are not strict JSON.
    InvalidJson,
    /// `stop_reason = max_output` left an open tool block (T3).
    Truncated,
    /// The argument bytes are empty.
    Empty,
}

impl UnparseableReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UnparseableReason::InvalidJson => "invalid_json",
            UnparseableReason::Truncated => "truncated",
            UnparseableReason::Empty => "empty",
        }
    }
}

/// `ToolCallBlock{tool_call_id, surface_ids{provider_call_id, item_id?},
/// surface_name, namespace?, arguments, provider_index, truncated,
/// thought_signature?}` (§5b.1 `ModelMessage` row; T1–T5).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallBlock {
    /// `tool_call_id` — allocated per ADR-0027, nested under `model_call_id`
    /// (T1); provider ids live in `surface_ids`, never rewritten.
    pub tool_call_id: String,
    /// `surface_ids{provider_call_id, item_id?}` — provider-native aliases.
    pub surface_ids: std::collections::BTreeMap<String, String>,
    /// `surface_name` — carried verbatim (T4; an unknown name is E3's
    /// `UnknownSurface`, not a gateway error).
    pub surface_name: String,
    /// `namespace?`.
    pub namespace: Option<String>,
    /// `arguments`.
    pub arguments: ToolArgs,
    /// `provider_index` — the provider's own ordering (T5).
    pub provider_index: u32,
    /// `truncated` — `max_output` left the block open (T3).
    pub truncated: bool,
    /// `thought_signature?` — the reasoning signature leaf (T6).
    pub thought_signature: Option<OpaqueLeaf>,
}

/// `ModelBlock` — the typed block sum (`TextBlock | ReasoningBlock |
/// ToolCallBlock | ProviderOpaqueBlock`).
#[derive(Debug, Clone, PartialEq)]
pub enum ModelBlock {
    /// `TextBlock{text, signature?}`.
    Text {
        /// The text.
        text: Text,
        /// An optional signature leaf.
        signature: Option<OpaqueLeaf>,
    },
    /// `ReasoningBlock{text?, signature?, redacted, owner}`.
    Reasoning {
        /// The reasoning text (absent under `redacted`).
        text: Option<Text>,
        /// The signature leaf.
        signature: Option<OpaqueLeaf>,
        /// `redacted` — the provider redacted this block.
        redacted: bool,
        /// `owner`.
        owner: String,
    },
    /// `ToolCallBlock`.
    ToolCall(ToolCallBlock),
    /// `ProviderOpaqueBlock{kind, payload, owner}`.
    ProviderOpaque {
        /// The provider's block kind spelling.
        kind: String,
        /// The payload leaf.
        payload: OpaqueLeaf,
        /// `owner`.
        owner: String,
    },
}

impl ModelBlock {
    /// The block kind.
    pub fn kind(&self) -> BlockKind {
        match self {
            ModelBlock::Text { .. } => BlockKind::Text,
            ModelBlock::Reasoning { .. } => BlockKind::Reasoning,
            ModelBlock::ToolCall(_) => BlockKind::ToolCall,
            ModelBlock::ProviderOpaque { .. } => BlockKind::ProviderOpaque,
        }
    }
}

/// `ModelMessage{blocks, stop_reason, stop_details?, raw_stop_reason?, usage,
/// served_model?, snapshot_id?, surface_ids, provenance}` (§5b.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMessage {
    /// The typed blocks.
    pub blocks: Vec<ModelBlock>,
    /// `stop_reason` — the closed terminal reason.
    pub stop_reason: StopReason,
    /// `stop_details?`.
    pub stop_details: Option<StopDetails>,
    /// `raw_stop_reason` — the provider's spelling (a surface alias).
    pub raw_stop_reason: Option<String>,
    /// `usage` — the terminal `UsageRecord` (absent ⇒ `available = false` on
    /// the event payload; never a silent zero — I3).
    pub usage: Option<hh_telemetry::TokenVector>,
    /// `served_model?` — the drift fact.
    pub served_model: Option<String>,
    /// `snapshot_id?` — the provider's snapshot id, when exposed.
    pub snapshot_id: Option<String>,
    /// `surface_ids{response_id, …}` — provider-native aliases.
    pub surface_ids: std::collections::BTreeMap<String, String>,
}

impl ModelMessage {
    /// `text()` — concatenate the visible text blocks (the caller's
    /// convenience; never a parse).
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                ModelBlock::Text { text, .. } => Some(text.content.as_str()),
                _ => None,
            })
            .collect()
    }

    /// `tool_calls()` — the `ToolCall` blocks.
    pub fn tool_calls(&self) -> Vec<&ToolCallBlock> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                ModelBlock::ToolCall(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// `true` when no block is present — the `empty_response` predicate.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// `true` when every block is `Reasoning` — the
    /// `invalid_response{thinking_only}` predicate (R-RT-2/R-RT-5).
    pub fn is_thinking_only(&self) -> bool {
        !self.blocks.is_empty()
            && self
                .blocks
                .iter()
                .all(|b| matches!(b, ModelBlock::Reasoning { .. }))
    }
}

/// `stop_reason` mapping — the dialect's `stop_reason_map` rules; an unmapped
/// spelling is `unknown` with the raw alias preserved (never coerced).
pub fn map_stop_reason<'a>(
    raw: &str,
    rules: impl Iterator<Item = (&'a str, StopReason)>,
) -> (StopReason, Option<String>) {
    for (provider, reason) in rules {
        if provider == raw {
            // The raw alias is always preserved — mapped or not.
            return (reason, Some(raw.to_string()));
        }
    }
    (StopReason::Unknown, Some(raw.to_string()))
}

/// The strict `ToolCall` parse (T2): the codec accepts exactly the
/// `{name|surface_name, input|arguments}` shapes; anything else is
/// `Unparseable` — never repaired, never dropped.
pub fn parse_tool_call_args(raw: &str) -> ToolArgs {
    if raw.is_empty() {
        return ToolArgs::Unparseable {
            raw: raw.to_string(),
            reason: UnparseableReason::Empty,
        };
    }
    // The strict parse is `hh_wire`'s canonical JSON — integers only, sorted
    // members. A non-canonical fragment is `invalid_json` (T2's strictness:
    // canonical form or nothing — the provider's arg bytes may be valid JSON
    // yet non-canonical; the strict gate is the kernel's own parser).
    match hh_wire::canonical::parse_canonical(raw.as_bytes()) {
        Ok(Json::Obj(_)) => ToolArgs::Parsed(
            hh_wire::canonical::parse_canonical(raw.as_bytes()).unwrap_or(Json::Null),
        ),
        Ok(Json::Null) => ToolArgs::Parsed(Json::Null),
        _ => ToolArgs::Unparseable {
            raw: raw.to_string(),
            reason: UnparseableReason::InvalidJson,
        },
    }
}

/// `Unparseable{raw, reason}` as an error — the T2 refusal shape.
pub fn unparseable_tool_call(_raw: &str, reason: UnparseableReason) -> CodecError {
    CodecError::TypeMismatch {
        member: format!("tool_call.arguments ({})", reason.as_str()),
        expected: "strict canonical JSON",
    }
}
