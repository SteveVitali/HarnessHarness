//! The `ModelEvent` grammar (§5b.1 ADR-0118 d.4) — the sole output contract of
//! `decode`:
//!
//! - **G1** exactly one `attempt.started` per attempt; `message.started`
//!   precedes any block event of that attempt.
//! - **G2** each `index` opened once and completed once; deltas only between.
//! - **G3** `tool_call` blocks complete with `arguments ∈ {Parsed,
//!   Unparseable{raw, reason}}` — never dropped, never repaired.
//! - **G4** exactly one of `completed | failed` closes the stream.
//! - **G5** `completed.message` is reconstructible from `block.completed`
//!   events alone (deltas are `ephemeral`, `model.stream.delta`).
//! - **G6** `provider.notice` never changes classification or accounting by
//!   itself; unknown notice kinds are preserved byte-for-byte (ADR-0015).
//! - **G7** events carry `{model_call_id, attempt_no, seq_in_attempt}`; a
//!   provider sequence number is a surface alias.
//!
//! The decoder is a state machine over [`WireFrame`]s — deterministic,
//! content-free (it moves bytes, never reads meaning).

use hh_wire::json::Json;

use crate::codec::{self, WireFrame};
use crate::dialect::WireDialect;
use crate::message::{
    ModelBlock, ModelMessage, OpaqueLeaf, Text, TextAuthority, TextTrust, TextVisibility, ToolArgs,
    ToolCallBlock, UnparseableReason,
};
use crate::vocab::{
    BlockKind, ModelError, ModelErrorClass, NoticeKind, StopDetails, StopReason, UsageArrival,
};

/// `delta ∈ {Text | Reasoning | ToolArgs(partial_json_str)}` (ADR-0118 d.4).
#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    /// `Text` — a visible-text delta.
    Text(String),
    /// `Reasoning` — a thinking/reasoning delta.
    Reasoning(String),
    /// `ToolArgs` — a partial-json argument fragment (verbatim — T2).
    ToolArgs(String),
}

/// `usage.updated.arrival ∈ {cumulative, delta}` — how the frame's usage
/// counts (the dialect's `usage_arrival` narrows which are legal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageArrivalKind {
    /// Cumulative (the frame's counts supersede prior frames').
    Cumulative,
    /// Delta (the frame's counts add to prior frames').
    Delta,
}

impl UsageArrivalKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UsageArrivalKind::Cumulative => "cumulative",
            UsageArrivalKind::Delta => "delta",
        }
    }
}

/// `timing{latency_ms, attempts, ttft_ms?, queue_wait_ms, measured_at}` — the
/// attempt-timing record on a terminal event (ADR-0118 d.7; `measured_at` =
/// `adapter`/`external_gateway`, CF-254).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    /// `latency_ms` — open to terminal.
    pub latency_ms: u64,
    /// `attempts`.
    pub attempts: u32,
    /// `ttft_ms` — time to first byte/frame.
    pub ttft_ms: Option<u64>,
    /// `queue_wait_ms` — the attempt's queue wait (R-RT-3/R-RT-6).
    pub queue_wait_ms: u64,
    /// `measured_at` — the measurement point (`adapter`/`external_gateway`).
    pub measured_at: String,
}

/// `ModelEvent` — the closed kernel sum per dialect (ADR-0118 d.4), stamped
/// `{model_call_id, attempt_no, seq_in_attempt}` (G7).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelEvent {
    /// `model_call_id` — the logical call's scope.
    pub model_call_id: String,
    /// `attempt_no`.
    pub attempt_no: u32,
    /// `seq_in_attempt` — the kernel's per-attempt sequence (a provider
    /// sequence number is a surface alias, never this).
    pub seq_in_attempt: u32,
    /// The event kind.
    pub kind: ModelEventKind,
}

/// The event kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelEventKind {
    /// `attempt.started{attempt_no}` — G1: exactly one per attempt.
    AttemptStarted,
    /// `message.started{surface_ids{response_id?}, served_model?}`.
    MessageStarted {
        /// `response_id` — the provider's response alias.
        response_id: Option<String>,
        /// `served_model` — the drift fact.
        served_model: Option<String>,
    },
    /// `block.started{index, kind}`.
    BlockStarted {
        /// The block index.
        index: u32,
        /// The block kind.
        kind: BlockKind,
    },
    /// `block.delta{index, delta}` — ephemeral (`model.stream.delta`).
    BlockDelta {
        /// The block index.
        index: u32,
        /// The delta.
        delta: Delta,
    },
    /// `block.completed{index, block: NormalizedBlock}` — G5: the message is
    /// reconstructible from these alone.
    BlockCompleted {
        /// The block index.
        index: u32,
        /// The normalized block (a `tool_call` carries
        /// `arguments ∈ {Parsed, Unparseable}` — G3).
        block: ModelBlock,
    },
    /// `usage.updated{partial: TokenVector, arrival}`.
    UsageUpdated {
        /// The normalized vector (partial — in-stream).
        usage: hh_telemetry::TokenVector,
        /// `arrival`.
        arrival: UsageArrivalKind,
    },
    /// `provider.notice{kind, payload: opaque(owner = dialect)}` — G6:
    /// preserved verbatim; never changes classification or accounting.
    ProviderNotice {
        /// The mapped notice kind (`other` preserves the raw kind on the
        /// payload's `format_tag`).
        kind: NoticeKind,
        /// The verbatim payload.
        payload: OpaqueLeaf,
    },
    /// `attempt.failed{error, will_retry, next_delay_ms?}`.
    AttemptFailed {
        /// The classified error.
        error: ModelError,
        /// `will_retry`.
        will_retry: bool,
        /// `next_delay_ms`.
        next_delay_ms: Option<u64>,
    },
    /// `completed{message, stop_reason, stop_details?, usage, timing,
    /// surface_ids}` — G4: exactly one terminal.
    Completed {
        /// The collected `ModelMessage`.
        message: Box<ModelMessage>,
        /// The stop reason (redundant with `message.stop_reason` — the
        /// event-level member is what the payload reads).
        stop_reason: StopReason,
        /// `stop_details?`.
        stop_details: Option<StopDetails>,
        /// The terminal usage.
        usage: Option<hh_telemetry::TokenVector>,
        /// `timing`.
        timing: Timing,
        /// `surface_ids`.
        surface_ids: std::collections::BTreeMap<String, String>,
    },
    /// `failed{error, usage?, timing}`.
    Failed {
        /// The classified error.
        error: ModelError,
        /// Usage-so-far where the dialect delivers it.
        usage: Option<hh_telemetry::TokenVector>,
        /// `timing`.
        timing: Timing,
    },
}

/// A grammar violation — G1–G7 enforcement reports (errors, never warnings).
#[derive(Debug, Clone, PartialEq)]
pub enum GrammarViolation {
    /// G1 — a block event preceded `message.started`, or `attempt.started`/
    /// `message.started` repeated.
    OutOfOrderStart,
    /// G2 — a `block.delta`/`block.completed` named an index with no open
    /// block, or an index opened twice / completed twice.
    BlockMisuse {
        /// The index.
        index: u32,
        /// The detail.
        detail: &'static str,
    },
    /// G4 — an event followed the terminal.
    AfterTerminal,
    /// G6 — an unknown frame kind (preserved as `provider.notice`; recorded).
    UnknownFrame {
        /// The frame kind.
        kind: String,
    },
    /// G7 — a gap exceeded `stream_idle_timeout_ms` (the stream's idle
    /// timeout — recorded; the *error* is `timeout{stream_idle}`).
    IdleTimeout {
        /// The gap.
        gap_ms: u64,
    },
}

impl GrammarViolation {
    /// The invariant tag.
    pub fn g(&self) -> &'static str {
        match self {
            GrammarViolation::OutOfOrderStart => "g1",
            GrammarViolation::BlockMisuse { .. } => "g2",
            GrammarViolation::AfterTerminal => "g4",
            GrammarViolation::UnknownFrame { .. } => "g6",
            GrammarViolation::IdleTimeout { .. } => "g7",
        }
    }
}

/// The decoder's per-call state (reset per attempt — G1's `attempt.started`
/// scopes the counters).
#[derive(Debug, Default)]
pub struct DecodeState {
    /// `model_call_id` context (stamped on every event — G7).
    pub model_call_id: String,
    /// `attempt_no` context.
    pub attempt_no: u32,
    /// `seq_in_attempt` — the per-attempt counter.
    pub seq_in_attempt: u32,
    /// Whether `message.started` fired this attempt.
    pub message_started: bool,
    /// The currently-open block index (one at a time per provider stream —
    /// a second `block.started` while one is open is a G2 violation).
    pub open_block: Option<u32>,
    /// Block indexes opened this attempt.
    pub opened: Vec<u32>,
    /// Block indexes completed this attempt.
    pub completed: Vec<u32>,
    /// Per-block kinds.
    pub block_kinds: std::collections::BTreeMap<u32, BlockKind>,
    /// Per-block accumulated text/reasoning deltas.
    pub block_text: std::collections::BTreeMap<u32, String>,
    /// Per-block accumulated `ToolArgs` fragments.
    pub block_json: std::collections::BTreeMap<u32, String>,
    /// Per-block provider surface names (the `ToolCallBlock.surface_name`).
    pub block_names: std::collections::BTreeMap<u32, String>,
    /// Per-block provider call ids (`surface_ids.provider_call_id`).
    pub block_call_ids: std::collections::BTreeMap<u32, String>,
    /// Per-block opaque payloads (provider_opaque / redacted_reasoning).
    pub block_opaque: std::collections::BTreeMap<u32, OpaqueLeaf>,
    /// Per-block signatures (T6 — the `signature`/`thought_signature` leaf).
    pub block_signatures: std::collections::BTreeMap<u32, OpaqueLeaf>,
    /// The completed blocks (G5 — the message is rebuilt from these alone).
    pub completed_blocks: std::collections::BTreeMap<u32, ModelBlock>,
    /// Whether a terminal fired.
    pub terminated: bool,
    /// The last frame's arrival ms (the G7 idle check).
    pub last_frame_ms: Option<u64>,
    /// The mapped stop reason + raw alias.
    pub stop: Option<(StopReason, Option<String>)>,
    /// The stop details a terminal carried.
    pub stop_details: Option<StopDetails>,
    /// The served-model drift fact.
    pub served_model: Option<String>,
    /// `response_id`.
    pub response_id: Option<String>,
    /// `snapshot_id`.
    pub snapshot_id: Option<String>,
    /// The final usage.
    pub final_usage: Option<hh_telemetry::TokenVector>,
    /// The provider's raw usage object on the terminal frame (the
    /// `usage.record.raw` member — opaque, dialect-owned).
    pub raw_usage: Option<Json>,
    /// The grammar violations the stream accumulated (recorded, not silent).
    pub violations: Vec<GrammarViolation>,
}

impl DecodeState {
    /// Begin a new attempt — `attempt.started` fires first (G1).
    pub fn begin_attempt(&mut self, attempt_no: u32) {
        self.attempt_no = attempt_no;
        self.seq_in_attempt = 0;
        self.message_started = false;
        self.open_block = None;
        self.opened.clear();
        self.completed.clear();
        self.block_kinds.clear();
        self.block_text.clear();
        self.block_json.clear();
        self.block_names.clear();
        self.block_call_ids.clear();
        self.block_opaque.clear();
        self.block_signatures.clear();
        self.completed_blocks.clear();
        self.terminated = false;
        self.last_frame_ms = None;
        self.stop = None;
        self.stop_details = None;
        self.served_model = None;
        self.response_id = None;
        self.snapshot_id = None;
        self.final_usage = None;
        self.raw_usage = None;
    }
}

/// `decode_frame(dialect, frame, state, at_ms, normalizer_ref)` — decode one
/// wire frame into `ModelEvent`s under G1–G7. `normalizer_ref` is the Model
/// Profile `usage_mapping` ref stamped on every emitted `TokenVector`.
pub fn decode_frame(
    dialect: &WireDialect,
    frame: &WireFrame,
    state: &mut DecodeState,
    at_ms: u64,
    normalizer_ref: &str,
) -> Vec<ModelEvent> {
    let mut out: Vec<ModelEventKind> = Vec::new();
    // G7 — the stream-idle check precedes everything (the gap is a violation;
    // the error is `timeout{stream_idle}` on the terminal path).
    if let Some(last) = state.last_frame_ms {
        let gap = at_ms.saturating_sub(last);
        if gap > dialect.stream_idle_timeout_ms && !state.terminated {
            state
                .violations
                .push(GrammarViolation::IdleTimeout { gap_ms: gap });
            state.terminated = true;
            out.push(ModelEventKind::Failed {
                error: ModelError::new(
                    ModelErrorClass::Timeout(crate::vocab::TimeoutKind::StreamIdle),
                    format!("stream idle {gap}ms > {}ms", dialect.stream_idle_timeout_ms),
                ),
                usage: state.final_usage.clone(),
                timing: Timing {
                    latency_ms: 0,
                    attempts: state.attempt_no,
                    ttft_ms: None,
                    queue_wait_ms: 0,
                    measured_at: "adapter".into(),
                },
            });
            return out
                .into_iter()
                .map(|k| {
                    let seq = state.seq_in_attempt;
                    state.seq_in_attempt += 1;
                    ModelEvent {
                        model_call_id: state.model_call_id.clone(),
                        attempt_no: state.attempt_no,
                        seq_in_attempt: seq,
                        kind: k,
                    }
                })
                .collect();
        }
    }
    state.last_frame_ms = Some(at_ms);
    // G4 — nothing after the terminal.
    if state.terminated {
        state.violations.push(GrammarViolation::AfterTerminal);
        return Vec::new();
    }
    match frame.kind.as_str() {
        "message_start" | "stream_start" => {
            // G1 — `message.started` precedes any block event; a second
            // `message_start` is a violation.
            if state.message_started {
                state.violations.push(GrammarViolation::OutOfOrderStart);
            }
            state.message_started = true;
            state.response_id = frame
                .data
                .get("id")
                .or_else(|| frame.data.get("response_id"))
                .or_else(|| frame.data.get("message").and_then(|m| m.get("id")))
                .and_then(Json::as_str)
                .map(str::to_string);
            state.served_model = frame
                .data
                .get("model")
                .or_else(|| frame.data.get("served_model"))
                .or_else(|| frame.data.get("message").and_then(|m| m.get("model")))
                .and_then(Json::as_str)
                .map(str::to_string);
            out.push(ModelEventKind::MessageStarted {
                response_id: state.response_id.clone(),
                served_model: state.served_model.clone(),
            });
        }
        "content_block_start" | "block_start" => {
            if !state.message_started {
                state.violations.push(GrammarViolation::OutOfOrderStart);
            }
            let index = frame.data.get("index").and_then(Json::as_int).unwrap_or(0) as u32;
            if state.open_block.is_some() || state.completed.contains(&index) {
                state.violations.push(GrammarViolation::BlockMisuse {
                    index,
                    detail: "opened while another index is open or already completed",
                });
            }
            if state.opened.contains(&index) {
                state.violations.push(GrammarViolation::BlockMisuse {
                    index,
                    detail: "index opened twice",
                });
            }
            let block_j = frame
                .data
                .get("content_block")
                .or_else(|| frame.data.get("block"))
                .cloned()
                .unwrap_or(Json::Null);
            let kind_str = block_j
                .get("type")
                .and_then(Json::as_str)
                .or_else(|| frame.data.get("kind").and_then(Json::as_str))
                .unwrap_or("text");
            let kind = match kind_str {
                "text" => BlockKind::Text,
                "thinking" | "reasoning" => BlockKind::Reasoning,
                "redacted_thinking" | "redacted_reasoning" => BlockKind::Reasoning,
                "tool_use" | "tool_call" => BlockKind::ToolCall,
                other => {
                    // An unknown block kind is `provider_opaque` — the payload
                    // is preserved byte-for-byte, never dropped.
                    if let Ok(leaf) = codec::parse_provider_opaque(&block_j, "provider") {
                        state.block_opaque.insert(
                            index,
                            OpaqueLeaf {
                                format_tag: other.to_string(),
                                ..leaf
                            },
                        );
                    }
                    BlockKind::ProviderOpaque
                }
            };
            if kind_str == "redacted_thinking" || kind_str == "redacted_reasoning" {
                if let Ok(leaf) = codec::parse_provider_opaque(&block_j, "model") {
                    state.block_opaque.insert(
                        index,
                        OpaqueLeaf {
                            format_tag: kind_str.to_string(),
                            ..leaf
                        },
                    );
                }
            }
            if let Some(name) = block_j
                .get("name")
                .or_else(|| block_j.get("surface_name"))
                .and_then(Json::as_str)
            {
                state.block_names.insert(index, name.to_string());
            }
            if let Some(cid) = block_j
                .get("id")
                .or_else(|| block_j.get("call_id"))
                .and_then(Json::as_str)
            {
                state.block_call_ids.insert(index, cid.to_string());
            }
            state.open_block = Some(index);
            state.opened.push(index);
            state.block_kinds.insert(index, kind);
            out.push(ModelEventKind::BlockStarted { index, kind });
        }
        "content_block_delta" | "block_delta" => {
            let index = frame.data.get("index").and_then(Json::as_int).unwrap_or(0) as u32;
            if state.open_block != Some(index) {
                state.violations.push(GrammarViolation::BlockMisuse {
                    index,
                    detail: "delta outside an open block",
                });
            }
            let delta = frame.data.get("delta").cloned().unwrap_or(Json::Null);
            let dtype = delta
                .get("type")
                .and_then(Json::as_str)
                .unwrap_or("text_delta");
            let d = match dtype {
                "text_delta" => {
                    let t = delta
                        .get("text")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    *state.block_text.entry(index).or_default() += &t;
                    Delta::Text(t)
                }
                "thinking_delta" | "reasoning_delta" => {
                    let t = delta
                        .get("thinking")
                        .or_else(|| delta.get("text"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    *state.block_text.entry(index).or_default() += &t;
                    Delta::Reasoning(t)
                }
                "input_json_delta" | "json_delta" => {
                    let p = delta
                        .get("partial_json")
                        .or_else(|| delta.get("patch"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    *state.block_json.entry(index).or_default() += &p;
                    Delta::ToolArgs(p)
                }
                "signature_delta" => {
                    // T6 — signatures ride as `OpaqueLeaf` on the block.
                    let s = delta
                        .get("signature")
                        .or_else(|| delta.get("bytes"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    state.block_signatures.insert(
                        index,
                        OpaqueLeaf {
                            format_tag: "signature".into(),
                            bytes: s.into_bytes(),
                            owner: "model".into(),
                            provenance: None,
                        },
                    );
                    return Vec::new(); // a signature delta is not a ModelEvent.
                }
                "citation_delta" => {
                    // Citations are provider-opaque deltas — preserved, not a
                    // typed delta kind.
                    return Vec::new();
                }
                _ => {
                    // Unknown delta kinds preserve their raw member text under
                    // the block (never dropped).
                    let t = delta
                        .get("text")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    *state.block_text.entry(index).or_default() += &t;
                    Delta::Text(t)
                }
            };
            out.push(ModelEventKind::BlockDelta { index, delta: d });
        }
        "content_block_stop" | "block_stop" => {
            let index = frame.data.get("index").and_then(Json::as_int).unwrap_or(0) as u32;
            if state.open_block == Some(index) {
                state.open_block = None;
            } else {
                state.violations.push(GrammarViolation::BlockMisuse {
                    index,
                    detail: "completed with no open block",
                });
            }
            if state.completed.contains(&index) {
                state.violations.push(GrammarViolation::BlockMisuse {
                    index,
                    detail: "index completed twice",
                });
            }
            state.completed.push(index);
            let block = materialize_block(state, index);
            state.completed_blocks.insert(index, block.clone());
            out.push(ModelEventKind::BlockCompleted { index, block });
        }
        "message_delta" | "message_stop" | "stream_end" => {
            if let Some(u) = frame.data.get("usage") {
                let is_final = frame.kind == "message_stop" || frame.kind == "stream_end";
                if let Some(v) = codec::normalize_usage(dialect, u, is_final, normalizer_ref) {
                    if is_final {
                        state.final_usage = Some(v.clone());
                        state.raw_usage = Some(u.clone());
                    }
                    out.push(ModelEventKind::UsageUpdated {
                        usage: v,
                        arrival: match dialect.usage_arrival {
                            UsageArrival::DeltaStream => UsageArrivalKind::Delta,
                            _ => UsageArrivalKind::Cumulative,
                        },
                    });
                }
            }
            if let Some(details) = frame
                .data
                .get("delta")
                .and_then(|d| d.get("stop_details"))
                .or_else(|| frame.data.get("stop_details"))
            {
                state.stop_details = parse_stop_details(details);
            }
            let raw_stop = frame
                .data
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .or_else(|| frame.data.get("stop_reason"))
                .and_then(Json::as_str)
                .map(str::to_string);
            if let Some(raw) = &raw_stop {
                state.stop = Some(codec::map_stop_reason(dialect, raw));
                // T3 — `max_output` with an open tool block ⇒ `truncated`.
                if state.stop.as_ref().map(|(s, _)| *s) == Some(StopReason::MaxOutput) {
                    if let Some(open) = state.open_block {
                        if state.block_kinds.get(&open) == Some(&BlockKind::ToolCall) {
                            let frag = state.block_json.get(&open).cloned().unwrap_or_default();
                            state
                                .block_json
                                .insert(open, format!("__truncated__{frag}"));
                        }
                    }
                }
            }
            if let Some(snap) = frame
                .data
                .get("snapshot_id")
                .or_else(|| {
                    dialect
                        .snapshot_id_exposed
                        .as_deref()
                        .and_then(|m| frame.data.get(m))
                })
                .and_then(Json::as_str)
            {
                state.snapshot_id = Some(snap.to_string());
            }
            if frame.kind == "message_stop" || frame.kind == "stream_end" {
                state.terminated = true;
                // G5 — the message is rebuilt from `block.completed` alone.
                let mut blocks: Vec<ModelBlock> =
                    state.completed_blocks.values().cloned().collect();
                // Any still-open block is completed at the terminal (a
                // `tool_call` left open under `max_output` is `truncated`).
                if let Some(open) = state.open_block.take() {
                    let block = materialize_block(state, open);
                    state.completed_blocks.insert(open, block.clone());
                    blocks.push(block);
                }
                match codec::collect(
                    blocks,
                    state.stop.clone(),
                    state.served_model.clone(),
                    state.stop_details.clone(),
                    state.final_usage.clone(),
                    state.snapshot_id.clone(),
                    state.response_id.clone(),
                ) {
                    Ok(message) => {
                        let stop_reason = message.stop_reason;
                        out.push(ModelEventKind::Completed {
                            message: Box::new(message),
                            stop_reason,
                            stop_details: state.stop_details.clone(),
                            usage: state.final_usage.clone(),
                            timing: Timing {
                                latency_ms: 0,
                                attempts: state.attempt_no,
                                ttft_ms: None,
                                queue_wait_ms: 0,
                                measured_at: "adapter".into(),
                            },
                            surface_ids: state
                                .response_id
                                .iter()
                                .map(|r| ("response_id".to_string(), r.clone()))
                                .collect(),
                        });
                    }
                    Err(error) => out.push(ModelEventKind::Failed {
                        error,
                        usage: state.final_usage.clone(),
                        timing: Timing {
                            latency_ms: 0,
                            attempts: state.attempt_no,
                            ttft_ms: None,
                            queue_wait_ms: 0,
                            measured_at: "adapter".into(),
                        },
                    }),
                }
            }
        }
        "usage" | "usage_update" => {
            if let Some(u) = frame.data.get("usage").or(Some(&frame.data)) {
                let is_final = matches!(frame.data.get("final"), Some(Json::Bool(true)));
                if let Some(v) = codec::normalize_usage(dialect, u, is_final, normalizer_ref) {
                    if is_final {
                        state.final_usage = Some(v.clone());
                        state.raw_usage = Some(u.clone());
                    }
                    out.push(ModelEventKind::UsageUpdated {
                        usage: v,
                        arrival: match dialect.usage_arrival {
                            UsageArrival::DeltaStream => UsageArrivalKind::Delta,
                            _ => UsageArrivalKind::Cumulative,
                        },
                    });
                }
            }
        }
        "error" => {
            state.terminated = true;
            let status = frame
                .data
                .get("status")
                .or_else(|| frame.data.get("error").and_then(|e| e.get("status")))
                .and_then(Json::as_int)
                .unwrap_or(0) as u32;
            let error = codec::classify_http_error(dialect, status, &frame.data);
            out.push(ModelEventKind::Failed {
                error,
                usage: state.final_usage.clone(),
                timing: Timing {
                    latency_ms: 0,
                    attempts: state.attempt_no,
                    ttft_ms: None,
                    queue_wait_ms: 0,
                    measured_at: "adapter".into(),
                },
            });
        }
        other => {
            // G6 — `provider.notice` preserved verbatim; `notice_map`/
            // `metadata_frame` map the kind; never classification input.
            if !codec::is_metadata_frame(dialect, other) {
                state.violations.push(GrammarViolation::UnknownFrame {
                    kind: other.to_string(),
                });
            }
            if let Some(m) = frame
                .data
                .get("model")
                .or_else(|| frame.data.get("served_model"))
                .and_then(Json::as_str)
            {
                state.served_model = Some(m.to_string());
            }
            out.push(ModelEventKind::ProviderNotice {
                kind: codec::notice_kind(dialect, other),
                payload: OpaqueLeaf {
                    format_tag: other.to_string(),
                    bytes: frame.data.to_canonical_string().into_bytes(),
                    owner: "dialect".into(),
                    provenance: None,
                },
            });
        }
    }
    out.into_iter()
        .map(|k| {
            let seq = state.seq_in_attempt;
            state.seq_in_attempt += 1;
            ModelEvent {
                model_call_id: state.model_call_id.clone(),
                attempt_no: state.attempt_no,
                seq_in_attempt: seq,
                kind: k,
            }
        })
        .collect()
}

/// Materialize a `block.completed`'s `NormalizedBlock` from the accumulated
/// state (G3/G5 — a `tool_call` completes with `arguments ∈ {Parsed,
/// Unparseable}`, never dropped, never repaired).
fn materialize_block(state: &DecodeState, index: u32) -> ModelBlock {
    let external = |content: String| Text {
        content,
        authority: TextAuthority::Delegate,
        trust: TextTrust::External,
        visibility: TextVisibility::Public,
    };
    let kind = state
        .block_kinds
        .get(&index)
        .copied()
        .unwrap_or(BlockKind::Text);
    let signature = state.block_signatures.get(&index).cloned();
    match kind {
        BlockKind::Text => ModelBlock::Text {
            text: external(state.block_text.get(&index).cloned().unwrap_or_default()),
            signature,
        },
        BlockKind::Reasoning => {
            if let Some(op) = state.block_opaque.get(&index) {
                ModelBlock::Reasoning {
                    text: None,
                    signature: Some(op.clone()),
                    redacted: true,
                    owner: op.owner.clone(),
                }
            } else {
                ModelBlock::Reasoning {
                    text: Some(external(
                        state.block_text.get(&index).cloned().unwrap_or_default(),
                    )),
                    signature,
                    redacted: false,
                    owner: "model".into(),
                }
            }
        }
        BlockKind::RedactedReasoning => ModelBlock::Reasoning {
            text: None,
            signature: state.block_opaque.get(&index).cloned().or(signature),
            redacted: true,
            owner: state
                .block_opaque
                .get(&index)
                .map(|o| o.owner.clone())
                .unwrap_or_else(|| "provider".into()),
        },
        BlockKind::ToolCall => {
            let frag = state.block_json.get(&index).cloned().unwrap_or_default();
            let (arguments, truncated) = if let Some(rest) = frag.strip_prefix("__truncated__") {
                (
                    ToolArgs::Unparseable {
                        raw: rest.to_string(),
                        reason: UnparseableReason::Truncated,
                    },
                    true,
                )
            } else {
                (crate::message::parse_tool_call_args(&frag), false)
            };
            let mut surface_ids = std::collections::BTreeMap::new();
            if let Some(cid) = state.block_call_ids.get(&index) {
                surface_ids.insert("provider_call_id".to_string(), cid.clone());
            }
            ModelBlock::ToolCall(ToolCallBlock {
                tool_call_id: format!("{}.tc{}", state.model_call_id, index),
                surface_ids,
                surface_name: state.block_names.get(&index).cloned().unwrap_or_default(),
                namespace: None,
                arguments,
                provider_index: index,
                truncated,
                thought_signature: signature,
            })
        }
        BlockKind::ProviderOpaque => {
            let op = state
                .block_opaque
                .get(&index)
                .cloned()
                .unwrap_or(OpaqueLeaf {
                    format_tag: "opaque".into(),
                    bytes: state
                        .block_text
                        .get(&index)
                        .cloned()
                        .unwrap_or_default()
                        .into_bytes(),
                    owner: "provider".into(),
                    provenance: None,
                });
            ModelBlock::ProviderOpaque {
                kind: op.format_tag.clone(),
                payload: op,
                owner: "provider".into(),
            }
        }
    }
}

/// `stop_details` — `{categories[{category, level?}], explanation?}` decoded
/// as data (the explanation is `Text(external)` — moved, never interpreted).
fn parse_stop_details(j: &Json) -> Option<StopDetails> {
    let categories = j
        .get("categories")
        .and_then(crate::dialect::json_arr)
        .map(|items| {
            items
                .iter()
                .filter_map(|c| {
                    Some(crate::vocab::StopCategory {
                        category: c.get("category")?.as_str()?.to_string(),
                        level: c.get("level").and_then(Json::as_str).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let explanation = j
        .get("explanation")
        .and_then(Json::as_str)
        .map(str::to_string);
    Some(StopDetails {
        categories,
        explanation,
    })
}
