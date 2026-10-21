//! §5b.1 R4.5d — the Anthropic Messages wire adapter
//! (`anthropic-messages@1`; ADR-0366). Two halves, both provider-owned
//! facts in adapter code (the dialect document declares *what* the facts
//! are; this file renders and reshapes them):
//!
//! - **Request render** — `POST {endpoint}/messages`: `messages[]` with
//!   `content[]` blocks (`text`/`tool_use`/`tool_result`), top-level
//!   `system`, `tools[]{name,description,input_schema}`,
//!   `tool_choice{type:"auto"}`, and the protocol-mandatory `max_tokens`
//!   (the dialect's `plan_schema.required_keys` makes its absence a typed
//!   refusal upstream).
//! - **Response translate** — the SSE event stream (`event:`/`data:`) is
//!   reshaped into `WireFrame`s. The canonical grammar already speaks the
//!   Messages vocabulary natively (`message_start`, `content_block_*`,
//!   `message_delta`, `message_stop`, `tool_use`, `thinking_delta`,
//!   `signature_delta`), so [`MessagesTranslator`] is a thin reshape:
//!   usage members are held off `message_delta` and merged into the
//!   terminal `message_stop` frame (input counters live on
//!   `message_start.message.usage`, the output counter on
//!   `message_delta.usage` — a single `usage` document is what
//!   `usage_map` reads); `ping` rides through as a declared
//!   `metadata_frame` (provider notice, never a violation); the `error`
//!   event's `error.type` is copied to `error.code` so the shared
//!   `error_code_to_class` rules classify it (the raw `type` member is
//!   preserved); unknown event/delta kinds become `ProviderNotice`
//!   frames with their payload verbatim — preserved, never dropped.
//!
//! `finish()` at connection close emits the pending `content_block_stop`s
//! and a terminal `message_stop` carrying the merged usage — the same
//! EOF discipline the OpenAI translator keeps (a stream that ended
//! without `message_stop` still terminates; a missing `stop_reason`
//! fails `invalid_response{no_stop_reason}` downstream, never
//! fabricated).

use crate::codec::WireFrame;
use crate::vocab::{ModelError, ModelErrorClass};
use hh_wire::json::Json;
use std::collections::{BTreeMap, BTreeSet};

// ── request render ───────────────────────────────────────────────────────────

/// `text_msg(role, text)` — a Messages `messages[]` entry whose content is
/// one text block.
pub fn text_msg(role: &str, text: &str) -> Json {
    Json::obj([
        ("role", Json::str(role)),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                ("text", Json::str(text)),
            ])]),
        ),
    ])
}

/// `assistant_tool_use_msg(calls)` — the assistant turn carrying
/// `tool_use` blocks back to the provider (the Messages multi-turn tool
/// carrier form — a result follows as a `user` message, never an
/// `assistant` continuation on the same turn's wire shape).
/// `calls` are `(id, name, arguments_json_text)`; `arguments` is the
/// canonical-JSON fragment the ledger recorded — parsed back into the
/// `input` object (an unparseable fragment lowers to `input:{}` — the
/// provider's own validation is the honesty boundary, never a repair).
pub fn assistant_tool_use_msg(calls: &[(String, String, String)]) -> Json {
    let blocks: Vec<Json> = calls
        .iter()
        .map(|(id, name, args)| {
            let input = hh_wire::json::parse(args).unwrap_or_else(|_| Json::obj([]));
            Json::obj([
                ("type", Json::str("tool_use")),
                ("id", Json::str(id.clone())),
                ("name", Json::str(name.clone())),
                ("input", input),
            ])
        })
        .collect();
    Json::obj([
        ("role", Json::str("assistant")),
        ("content", Json::Arr(blocks)),
    ])
}

/// `wire_call_id(kernel_id)` — the kernel's `tool_call_id` rendered into
/// the family's tool-use id alphabet (`[a-zA-Z0-9_-]` — the same alphabet
/// `wire_name` lowers surfaces through; the provider's own `toolu_*` ids
/// pass through unchanged). A non-conforming char lowers to `_` — the
/// render is deterministic and *pair-consistent*: `tool_use.id` and its
/// `tool_result.tool_use_id` derive the same wire id from the same kernel
/// id, which is all the protocol's pairing contract requires.
pub fn wire_call_id(kernel_id: &str) -> String {
    let s: String = kernel_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "call".to_string()
    } else {
        s
    }
}

/// `tool_result_msg(tool_use_id, content)` — a `user` turn whose content is
/// one `tool_result` block. The block's `content` is a *string* on the
/// wire — a non-string payload renders its canonical form (verbatim and
/// replay-stable, the same discipline `role:"tool"` keeps).
pub fn tool_result_msg(tool_use_id: &str, content: Json) -> Json {
    let content = match content {
        Json::Str(_) => content,
        other => Json::str(other.to_canonical_string()),
    };
    Json::obj([
        ("role", Json::str("user")),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("tool_result")),
                ("tool_use_id", Json::str(tool_use_id)),
                ("content", content),
            ])]),
        ),
    ])
}

/// `tool_entry(name, description, input_schema)` — one `tools[]` member:
/// `{name, description, input_schema}` — the wire name is the surface's
/// wire spelling ([`crate::openai::wire_name`]: Anthropic and OpenAI
/// share the `[a-zA-Z0-9_-]` tool-name alphabet).
pub fn tool_entry(name: &str, description: &str, input_schema: Json) -> Json {
    let mut e = BTreeMap::new();
    e.insert("name".to_string(), Json::str(name));
    if !description.is_empty() {
        e.insert("description".to_string(), Json::str(description));
    }
    e.insert("input_schema".to_string(), input_schema);
    Json::Obj(e)
}

/// `messages_request_body(model, messages, tools, stream, provider_params)`
/// — the plan's `body` member: the Messages request. `provider_params`
/// members merge verbatim (the dialect's `plan_schema.allowed_keys`
/// bounded the set at `validate_plan`; `model`/`messages`/`tools`/
/// `stream` are kernel-owned and always overwritten by the renderer).
/// `max_tokens` is *not* filled — the dialect's
/// `plan_schema.required_keys` makes its absence a typed plan refusal,
/// never a provider 400. `tools` present ⇒ `tool_choice{type:"auto"}` —
/// the family's own object spelling (never the chat-completions string).
pub fn messages_request_body(
    model: &str,
    messages: &[Json],
    tools: &[Json],
    stream: bool,
    provider_params: &BTreeMap<String, Json>,
) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in provider_params {
        m.insert(k.clone(), v.clone());
    }
    m.insert("model".into(), Json::str(model));
    m.insert("messages".into(), Json::Arr(messages.to_vec()));
    if !tools.is_empty() {
        m.insert("tools".into(), Json::Arr(tools.to_vec()));
        m.insert(
            "tool_choice".into(),
            Json::obj([("type", Json::str("auto"))]),
        );
    }
    m.insert("stream".into(), Json::Bool(stream));
    Json::Obj(m)
}

/// `error_body_for_classify(doc)` — an HTTP error body reshaped for
/// `codec::classify_http_error`: `error.type` copied to `error.code` so
/// the dialect's `error_code_to_class` rules fire (the `type` member is
/// preserved — the classified error's `raw_ref` still names the
/// provider's own payload).
pub fn error_body_for_classify(doc: &Json) -> Json {
    match doc.get("error").and_then(|e| e.get("type")) {
        Some(Json::Str(t)) => {
            let mut e = match doc.get("error") {
                Some(Json::Obj(m)) => m.clone(),
                _ => BTreeMap::new(),
            };
            e.insert("code".to_string(), Json::str(t.clone()));
            let mut m = match doc {
                Json::Obj(m) => m.clone(),
                _ => BTreeMap::new(),
            };
            m.insert("error".to_string(), Json::Obj(e));
            Json::Obj(m)
        }
        _ => doc.clone(),
    }
}

// ── SSE event → normalized frames ────────────────────────────────────────────

/// `MessagesTranslator` — folds Messages SSE `data:` payloads into the
/// `WireFrame` vocabulary `grammar::decode_frame` owns. The event kinds
/// pass through under their provider spellings (the grammar's frame arms
/// name them); the translator's real work is usage coalescing —
/// `message_start.message.usage` (the input counters) plus every
/// `message_delta.usage` (the output counter, cumulative) merge into the
/// terminal `message_stop.usage`, so `usage_map` sees one complete
/// Anthropic usage document, and `message_delta`'s partial usage never
/// surfaces as a phantom interim record.
#[derive(Debug, Default)]
pub struct MessagesTranslator {
    /// `stream` already terminated (`message_stop`/`error` seen or
    /// [`Self::finish`] emitted).
    done: bool,
    /// `message_start.message.usage` — the input-side counters.
    usage_in: Option<Json>,
    /// The latest `message_delta.usage` — the cumulative output counter.
    usage_out: Option<Json>,
    /// `message_start.message.model` — the served-model alias.
    served_model: Option<String>,
    /// Open block indexes (`content_block_stop` owed at EOF).
    open: BTreeSet<u32>,
}

impl MessagesTranslator {
    /// The response's observed `served_model` alias, when the stream
    /// carried it.
    pub fn served_model(&self) -> Option<&str> {
        self.served_model.as_deref()
    }

    /// The merged usage document — `message_start`'s members overlaid by
    /// the latest `message_delta` members (Anthropic reports input
    /// counters at start and the cumulative `output_tokens` per delta).
    fn merged_usage(&self) -> Option<Json> {
        let mut m = match &self.usage_in {
            Some(Json::Obj(m)) => m.clone(),
            _ => BTreeMap::new(),
        };
        if let Some(Json::Obj(d)) = &self.usage_out {
            for (k, v) in d {
                m.insert(k.clone(), v.clone());
            }
        }
        if m.is_empty() {
            None
        } else {
            Some(Json::Obj(m))
        }
    }

    /// Fold one SSE `data:` payload (a Messages event document) into
    /// frames. `done` is set after `message_stop`/`error` — later events
    /// are ignored (the provider's own ordering is the record).
    pub fn push_event(&mut self, data: &Json) -> Result<Vec<WireFrame>, ModelError> {
        let mut out = Vec::new();
        if self.done {
            return Ok(out);
        }
        let ty = data.get("type").and_then(Json::as_str).unwrap_or("");
        match ty {
            "message_start" => {
                if let Some(msg) = data.get("message") {
                    if let Some(u) = msg.get("usage") {
                        self.usage_in = Some(u.clone());
                    }
                    if let Some(m) = msg.get("model").and_then(Json::as_str) {
                        self.served_model = Some(m.to_string());
                    }
                }
                out.push(WireFrame {
                    kind: "message_start".to_string(),
                    data: data.clone(),
                });
            }
            "content_block_start" => {
                let index = data.get("index").and_then(Json::as_int).unwrap_or(0);
                self.open.insert(index.max(0) as u32);
                out.push(WireFrame {
                    kind: "content_block_start".to_string(),
                    data: data.clone(),
                });
            }
            "content_block_delta" => {
                // The grammar's delta vocabulary: text/JSON/thinking/
                // signature ride `content_block_delta`; a kind outside it
                // (e.g. `citations_delta`) is preserved as a provider-
                // opaque frame — never coerced into a typed delta.
                let dtype = data
                    .get("delta")
                    .and_then(|d| d.get("type"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let known = matches!(
                    dtype,
                    "text_delta" | "input_json_delta" | "thinking_delta" | "signature_delta"
                );
                out.push(WireFrame {
                    kind: if known {
                        "content_block_delta".to_string()
                    } else {
                        format!("delta:{dtype}")
                    },
                    data: data.clone(),
                });
            }
            "content_block_stop" => {
                let index = data.get("index").and_then(Json::as_int).unwrap_or(0);
                self.open.remove(&(index.max(0) as u32));
                out.push(WireFrame {
                    kind: "content_block_stop".to_string(),
                    data: data.clone(),
                });
            }
            "message_delta" => {
                if let Some(u) = data.get("usage") {
                    self.usage_out = Some(u.clone());
                }
                // The delta member rides verbatim (`stop_reason`,
                // `stop_sequence`); the partial usage is held for the
                // terminal merge — never an interim usage record with
                // zeroed input counters.
                let mut d = match data {
                    Json::Obj(m) => m.clone(),
                    _ => BTreeMap::new(),
                };
                d.remove("usage");
                out.push(WireFrame {
                    kind: "message_delta".to_string(),
                    data: Json::Obj(d),
                });
            }
            "message_stop" => {
                self.done = true;
                out.push(self.terminal_frame());
            }
            "ping" => {
                out.push(WireFrame {
                    kind: "ping".to_string(),
                    data: data.clone(),
                });
            }
            "error" => {
                // `{type:"error", error:{type, message}}` → the grammar's
                // `error` frame; `error.type` copied to `error.code` so the
                // dialect's `error_code_to_class` rules classify it (the
                // raw payload is preserved on the frame).
                let mut d = match data {
                    Json::Obj(m) => m.clone(),
                    _ => BTreeMap::new(),
                };
                if let Some(Json::Obj(e)) = data.get("error") {
                    let mut e2 = e.clone();
                    if let Some(t) = e.get("type").and_then(Json::as_str) {
                        e2.insert("code".to_string(), Json::str(t));
                    }
                    d.insert("error".to_string(), Json::Obj(e2));
                }
                self.done = true;
                out.push(WireFrame {
                    kind: "error".to_string(),
                    data: Json::Obj(d),
                });
            }
            other => {
                // An event kind outside the Messages vocabulary (a newer
                // family member) — preserved verbatim as a provider
                // notice; the grammar records it, never drops it.
                out.push(WireFrame {
                    kind: other.to_string(),
                    data: data.clone(),
                });
            }
        }
        Ok(out)
    }

    /// The terminal `message_stop` frame — the merged usage rides on it so
    /// the grammar's `is_final` normalization sees the complete Anthropic
    /// usage document.
    fn terminal_frame(&self) -> WireFrame {
        let mut d = BTreeMap::new();
        d.insert("type".to_string(), Json::str("message_stop"));
        if let Some(u) = self.merged_usage() {
            d.insert("usage".to_string(), u);
        }
        WireFrame {
            kind: "message_stop".to_string(),
            data: Json::Obj(d),
        }
    }

    /// Emit the terminal frames at EOF — pending `content_block_stop`s
    /// then `message_stop{usage?}` (the OpenAI family's `[DONE]`/EOF
    /// discipline: a stream that closed without `message_stop` still
    /// terminates; `collect` answers `no_stop_reason` when the provider
    /// never said why it stopped).
    pub fn finish(&mut self) -> Vec<WireFrame> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        let mut out = Vec::new();
        for index in std::mem::take(&mut self.open) {
            out.push(WireFrame {
                kind: "content_block_stop".to_string(),
                data: Json::obj([("index", Json::Int(index as i64))]),
            });
        }
        out.push(self.terminal_frame());
        out
    }
}

/// `completion_frames(doc)` — a non-streaming `message` body folded into
/// the same normalized sequence (`message_start` → content blocks →
/// `message_delta` → `message_stop`). A body that isn't a `message` is
/// `stream_decode` — honest, never coerced.
pub fn completion_frames(doc: &Json) -> Result<Vec<WireFrame>, ModelError> {
    if doc.get("type").and_then(Json::as_str) != Some("message") || doc.get("role").is_none() {
        return Err(ModelError::new(
            ModelErrorClass::StreamDecode,
            "response body is not a message document",
        ));
    }
    let mut tr = MessagesTranslator::default();
    let mut out = Vec::new();
    // `message_start` carries the response envelope (id/model/usage) —
    // `content` is served by the block events below.
    let mut msg = match doc {
        Json::Obj(m) => m.clone(),
        _ => BTreeMap::new(),
    };
    msg.remove("content");
    msg.remove("stop_reason");
    msg.remove("stop_sequence");
    out.extend(tr.push_event(&Json::obj([
        ("type", Json::str("message_start")),
        ("message", Json::Obj(msg)),
    ]))?);
    // One start/delta/stop triple per content block — the same sequence
    // the streaming shape serves (G5 builds the message from completions).
    for (i, block) in doc
        .get("content")
        .and_then(|c| match c {
            Json::Arr(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let btype = block.get("type").and_then(Json::as_str).unwrap_or("");
        let mut start = BTreeMap::new();
        match btype {
            "text" | "thinking" | "redacted_thinking" | "tool_use" => {
                let mut b = match block {
                    Json::Obj(m) => m.clone(),
                    _ => BTreeMap::new(),
                };
                // The payload members are served by the delta, not the
                // start (the streaming shape carries them empty).
                b.remove("text");
                b.remove("thinking");
                b.remove("signature");
                b.remove("input");
                start.insert("type".to_string(), Json::str("content_block_start"));
                start.insert("index".to_string(), Json::Int(i as i64));
                start.insert("content_block".to_string(), Json::Obj(b));
            }
            _ => {
                // Unknown block kinds ride verbatim — the grammar's
                // provider-opaque arm preserves them byte-for-byte.
                start.insert("type".to_string(), Json::str("content_block_start"));
                start.insert("index".to_string(), Json::Int(i as i64));
                start.insert("content_block".to_string(), block.clone());
            }
        }
        out.extend(tr.push_event(&Json::Obj(start))?);
        // The delta serving the block's payload.
        let delta = match btype {
            "text" => Some(Json::obj([
                ("type", Json::str("text_delta")),
                ("text", block.get("text").cloned().unwrap_or(Json::Null)),
            ])),
            "thinking" => Some(Json::obj([
                ("type", Json::str("thinking_delta")),
                (
                    "thinking",
                    block.get("thinking").cloned().unwrap_or(Json::Null),
                ),
            ])),
            "tool_use" => Some(Json::obj([
                ("type", Json::str("input_json_delta")),
                (
                    "partial_json",
                    Json::str(
                        block
                            .get("input")
                            .cloned()
                            .unwrap_or_else(|| Json::obj([]))
                            .to_canonical_string(),
                    ),
                ),
            ])),
            _ => None,
        };
        if let Some(delta) = delta {
            out.extend(tr.push_event(&Json::obj([
                ("type", Json::str("content_block_delta")),
                ("index", Json::Int(i as i64)),
                ("delta", delta),
            ]))?);
        }
        out.extend(tr.push_event(&Json::obj([
            ("type", Json::str("content_block_stop")),
            ("index", Json::Int(i as i64)),
        ]))?);
    }
    // `message_delta` carries the stop members (usage is served by the
    // start's document — a non-streaming body reports the totals once).
    out.extend(tr.push_event(&Json::obj([
        ("type", Json::str("message_delta")),
        (
            "delta",
            Json::obj([
                (
                    "stop_reason",
                    doc.get("stop_reason").cloned().unwrap_or(Json::Null),
                ),
                (
                    "stop_sequence",
                    doc.get("stop_sequence").cloned().unwrap_or(Json::Null),
                ),
            ]),
        ),
    ]))?);
    out.extend(tr.push_event(&Json::obj([("type", Json::str("message_stop"))]))?);
    Ok(out)
}
