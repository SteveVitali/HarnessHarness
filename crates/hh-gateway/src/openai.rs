//! `openai` — the `openai-compatible@1` family adapter (R4.5b; ADR-0364 D2).
//!
//! Two halves, both pure and dialect-vocabulary-shaped:
//!
//! - **Request render** — [`chat_request_body`] assembles the
//!   `chat/completions` request JSON from the plan's lowered messages +
//!   tool entries (the kernel's transcript render lives in `hh-embed`'s
//!   `GatewayModel`; this module owns only the wire shape).
//! - **Response translate** — [`SseDecoder`] splits `text/event-stream`
//!   bytes into events; [`ChunkTranslator`] folds `chat.completion.chunk`s
//!   into the dialect's normalized `WireFrame` vocabulary
//!   (`stream_start`/`block_start`/`block_delta`/`block_stop`/`stream_end`/
//!   `error`) that `grammar::decode_frame` already owns (G1–G7), and
//!   [`completion_frames`] does the same for a non-streaming
//!   `chat.completion` body.
//!
//! `finish_reason` is *held* (`pending_stop`) until a trailing usage-only
//! chunk or `data: [DONE]`/EOF — OpenAI-compat serves the usage chunk after
//! the finish chunk, and grammar G4 treats anything after `stream_end` as a
//! violation. The terminal `stream_end` frame carries `stop_reason` +
//! `usage` together.

use crate::codec::WireFrame;
use crate::vocab::{ModelError, ModelErrorClass};
use hh_wire::json::Json;

// ── request render ───────────────────────────────────────────────────────────

/// One tool entry the request body carries — `{type:"function",
/// function:{name, description, parameters}}`. `name` is the wire spelling
/// (the kernel's `hh.submit` renders `hh_submit` — provider names are
/// `[A-Za-z0-9_-]`; the surface-mapping member `wire_to_surface` on the
/// plan maps back).
pub fn tool_entry(name: &str, description: &str, parameters: Json) -> Json {
    let mut f = std::collections::BTreeMap::new();
    f.insert("name".into(), Json::str(name));
    if !description.is_empty() {
        f.insert("description".into(), Json::str(description));
    }
    f.insert("parameters".into(), parameters);
    Json::obj([("type", Json::str("function")), ("function", Json::Obj(f))])
}

/// `chat_request_body(model, messages, tools, stream, provider_params)` —
/// the plan's `body` member: the chat.completions request. `provider_params`
/// members merge verbatim (the dialect's `plan_schema.allowed_keys` bounded
/// the set at `validate_plan`; `stream`/`model`/`messages`/`tools` are
/// kernel-owned and always overwritten by the renderer).
pub fn chat_request_body(
    model: &str,
    messages: &[Json],
    tools: &[Json],
    stream: bool,
    provider_params: &std::collections::BTreeMap<String, Json>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    for (k, v) in provider_params {
        m.insert(k.clone(), v.clone());
    }
    m.insert("model".into(), Json::str(model));
    m.insert("messages".into(), Json::Arr(messages.to_vec()));
    if !tools.is_empty() {
        m.insert("tools".into(), Json::Arr(tools.to_vec()));
        m.insert("tool_choice".into(), Json::str("auto"));
    }
    m.insert("stream".into(), Json::Bool(stream));
    Json::Obj(m)
}

/// `text_msg(role, content)` — one rendered transcript message.
pub fn text_msg(role: &str, content: &str) -> Json {
    Json::obj([("role", Json::str(role)), ("content", Json::str(content))])
}

/// `assistant_tool_call_msg(calls)` — the prior assistant turn carrying
/// `tool_calls` — `{id, type:"function", function:{name, arguments}}`;
/// `arguments` is the canonical args *string* verbatim (the provider's own
/// transcript spelling, never re-decoded).
pub fn assistant_tool_call_msg(content: Option<&str>, calls: &[(String, String, String)]) -> Json {
    let tcs: Vec<Json> = calls
        .iter()
        .map(|(id, name, args)| {
            let mut f = std::collections::BTreeMap::new();
            f.insert("name".into(), Json::str(name.clone()));
            f.insert("arguments".into(), Json::str(args.clone()));
            Json::obj([
                ("id", Json::str(id.clone())),
                ("type", Json::str("function")),
                ("function", Json::Obj(f)),
            ])
        })
        .collect();
    Json::obj([
        ("role", Json::str("assistant")),
        ("content", content.map(Json::str).unwrap_or(Json::Null)),
        ("tool_calls", Json::Arr(tcs)),
    ])
}

/// `tool_result_msg(call_id, content)` — the observation the kernel recorded
/// for one tool call, rendered `role:"tool"`. The family's `content` member
/// is a *string* on the wire (OpenAI-compat endpoints 400 on an object) —
/// a non-string payload renders its canonical form, verbatim and
/// replay-stable.
pub fn tool_result_msg(call_id: &str, content: Json) -> Json {
    let content = match content {
        Json::Str(_) => content,
        other => Json::str(other.to_canonical_string()),
    };
    Json::obj([
        ("role", Json::str("tool")),
        ("tool_call_id", Json::str(call_id)),
        ("content", content),
    ])
}

/// `wire_name(surface)` — the outbound tool spelling (`hh.submit` →
/// `hh_submit`). **Outbound only** — the inverse is not injective
/// (`fs_read`/`fs.read` collide), so the port keeps an explicit
/// `wire→surface` map built at plan time (the plan's own member), never a
/// string guess.
pub fn wire_name(surface: &str) -> String {
    surface.replace('.', "_")
}

// ── SSE event decode ─────────────────────────────────────────────────────────

/// One SSE event: `Data(payload)` for `data:` lines; `Done` for the
/// family's `[DONE]` sentinel.
#[derive(Debug, Clone, PartialEq)]
pub enum SseEvent {
    /// The concatenated `data:` payload of one event block.
    Data(String),
    /// `data: [DONE]` — the stream's own terminal (distinct from EOF: a
    /// stream closed without it still flushes via [`SseDecoder::finish`]).
    Done,
}

/// `SseDecoder` — accumulates wire bytes and yields complete events (an
/// event ends at a blank line; comment/`event:`/`id:`/`retry:` lines are
/// carried but only `data:` contributes payload — SSE semantics, bounded by
/// `max_event_bytes`).
pub struct SseDecoder {
    /// The undelivered byte tail.
    buf: Vec<u8>,
    /// `data:` lines of the event currently accumulating.
    data_lines: Vec<String>,
    /// The byte bound per event.
    max_event: usize,
    /// Set once `[DONE]` arrived — later bytes are ignored.
    terminated: bool,
}

impl SseDecoder {
    /// `SseDecoder::new(max_event)` — `max_event` bounds one event's payload.
    pub fn new(max_event: usize) -> SseDecoder {
        SseDecoder {
            buf: Vec::new(),
            data_lines: Vec::new(),
            max_event,
            terminated: false,
        }
    }

    /// Whether `[DONE]` has been consumed.
    pub fn terminated(&self) -> bool {
        self.terminated
    }

    fn line(&mut self, line: &str, out: &mut Vec<SseEvent>) -> Result<(), ModelError> {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            // Blank line — dispatch the accumulated event.
            if !self.data_lines.is_empty() {
                let data = self.data_lines.join("\n");
                self.data_lines.clear();
                if data.trim() == "[DONE]" {
                    self.terminated = true;
                    out.push(SseEvent::Done);
                } else {
                    out.push(SseEvent::Data(data));
                }
            }
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(()); // comment/heartbeat
        }
        if let Some(rest) = line.strip_prefix("data:") {
            let data = rest.strip_prefix(' ').unwrap_or(rest);
            self.data_lines.push(data.to_string());
            let len: usize = self.data_lines.iter().map(|l| l.len()).sum();
            if len > self.max_event {
                return Err(ModelError::new(
                    ModelErrorClass::StreamDecode,
                    "sse event exceeds bound",
                ));
            }
        }
        // `event:`/`id:`/`retry:` lines are inert for this family.
        Ok(())
    }

    /// Fold `bytes` into complete events.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, ModelError> {
        let mut out = Vec::new();
        if self.terminated {
            return Ok(out);
        }
        self.buf.extend_from_slice(bytes);
        // Lines split on \n; an event ends at a blank line.
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let line = self.buf.drain(..=pos).collect::<Vec<u8>>();
            let text = String::from_utf8_lossy(&line);
            self.line(text.trim_end_matches('\n'), &mut out)?;
            if self.terminated {
                self.buf.clear();
                break;
            }
        }
        Ok(out)
    }

    /// Flush at EOF — a buffered tail is the final event (or nothing).
    pub fn finish(&mut self) -> Result<Vec<SseEvent>, ModelError> {
        let mut out = Vec::new();
        if !self.buf.is_empty() {
            let text = String::from_utf8_lossy(&self.buf).to_string();
            self.buf.clear();
            for l in text.split('\n') {
                self.line(l, &mut out)?;
            }
        }
        if !self.data_lines.is_empty() {
            let data = self.data_lines.join("\n");
            self.data_lines.clear();
            out.push(SseEvent::Data(data));
        }
        Ok(out)
    }
}

// ── chunk → normalized frames ────────────────────────────────────────────────

/// `ChunkTranslator` — folds `chat.completion.chunk` JSON into the dialect's
/// normalized `WireFrame` vocabulary (the kinds `grammar::decode_frame`
/// owns). `push_chunk` is fed each SSE `data` payload's JSON;
/// [`ChunkTranslator::finish`] is called at `[DONE]`/EOF to emit the
/// terminal `stream_end`.
#[derive(Debug, Default)]
pub struct ChunkTranslator {
    /// `stream_start` emitted.
    started: bool,
    /// The allocated text-block index (first content-bearing block).
    text_index: Option<u32>,
    /// tool-call `index` → allocated block index.
    call_blocks: std::collections::BTreeMap<u32, u32>,
    /// The next block index to allocate.
    next_index: u32,
    /// Open block indexes (a `block_stop` owed).
    open: std::collections::BTreeSet<u32>,
    /// `finish_reason` held until the terminal frame.
    pending_stop: Option<String>,
    /// The last `usage` object seen.
    usage: Option<Json>,
    /// The response's observed `served_model` alias (the chunk `model`).
    served_model: Option<String>,
    /// `true` after [`Self::finish`] emitted `stream_end`.
    done: bool,
}

impl ChunkTranslator {
    /// The response's observed `served_model` alias, when the stream carried it.
    pub fn served_model(&self) -> Option<&str> {
        self.served_model.as_deref()
    }

    fn start_frame(&mut self, chunk: &Json, out: &mut Vec<WireFrame>) {
        if self.started {
            return;
        }
        self.started = true;
        let mut d = std::collections::BTreeMap::new();
        if let Some(id) = chunk.get("id").and_then(Json::as_str) {
            d.insert("id".into(), Json::str(id));
        }
        if let Some(m) = chunk.get("model").and_then(Json::as_str) {
            self.served_model = Some(m.to_string());
            d.insert("model".into(), Json::str(m));
        }
        out.push(WireFrame {
            kind: "stream_start".to_string(),
            data: Json::Obj(d),
        });
    }

    fn open_block(&mut self, index: u32, block: Json, out: &mut Vec<WireFrame>) {
        self.open.insert(index);
        out.push(WireFrame {
            kind: "block_start".to_string(),
            data: Json::obj([("index", Json::Int(index as i64)), ("content_block", block)]),
        });
    }

    fn close_open(&mut self, out: &mut Vec<WireFrame>) {
        let idxs: Vec<u32> = self.open.iter().copied().collect();
        for index in idxs {
            self.open.remove(&index);
            out.push(WireFrame {
                kind: "block_stop".to_string(),
                data: Json::obj([("index", Json::Int(index as i64))]),
            });
        }
    }

    fn alloc(&mut self) -> u32 {
        let i = self.next_index;
        self.next_index += 1;
        i
    }

    fn delta(&mut self, index: u32, delta: Json, out: &mut Vec<WireFrame>) {
        out.push(WireFrame {
            kind: "block_delta".to_string(),
            data: Json::obj([("index", Json::Int(index as i64)), ("delta", delta)]),
        });
    }

    /// Fold one `chat.completion.chunk` (or an SSE `error` payload).
    pub fn push_chunk(&mut self, chunk: &Json) -> Result<Vec<WireFrame>, ModelError> {
        let mut out = Vec::new();
        if self.done {
            return Ok(out);
        }
        // An in-stream error payload (`{error:{…}}`) → the `error` frame —
        // the grammar classifies through the dialect's rules.
        if chunk.get("error").is_some() && chunk.get("choices").is_none() {
            let mut d = std::collections::BTreeMap::new();
            if let Some(e) = chunk.get("error") {
                d.insert("error".into(), e.clone());
            }
            if let Some(s) = chunk
                .get("status")
                .or_else(|| chunk.get("error").and_then(|e| e.get("status")))
            {
                d.insert("status".into(), s.clone());
            }
            out.push(WireFrame {
                kind: "error".to_string(),
                data: Json::Obj(d),
            });
            self.done = true;
            return Ok(out);
        }
        self.start_frame(chunk, &mut out);
        if let Some(u) = chunk.get("usage") {
            if !matches!(u, Json::Null) {
                self.usage = Some(u.clone());
            }
        }
        if let Some(Json::Arr(choices)) = chunk.get("choices") {
            for choice in choices {
                let delta = choice.get("delta").cloned().unwrap_or(Json::Null);
                // Text content delta.
                if let Some(text) = delta.get("content").and_then(Json::as_str) {
                    if !text.is_empty() {
                        let idx = match self.text_index {
                            Some(i) => i,
                            None => {
                                let i = self.alloc();
                                self.text_index = Some(i);
                                self.open_block(
                                    i,
                                    Json::obj([("type", Json::str("text"))]),
                                    &mut out,
                                );
                                i
                            }
                        };
                        self.delta(
                            idx,
                            Json::obj([
                                ("type", Json::str("text_delta")),
                                ("text", Json::str(text)),
                            ]),
                            &mut out,
                        );
                    }
                }
                // Tool-call deltas — `{index, id?, function:{name?, arguments?}}`.
                if let Some(Json::Arr(tcs)) = delta.get("tool_calls") {
                    for tc in tcs {
                        let tc_index =
                            tc.get("index").and_then(Json::as_int).unwrap_or(0).max(0) as u32;
                        let f = tc.get("function").cloned().unwrap_or(Json::Null);
                        let idx = match self.call_blocks.get(&tc_index) {
                            Some(i) => *i,
                            None => {
                                let i = self.alloc();
                                self.call_blocks.insert(tc_index, i);
                                let mut block = std::collections::BTreeMap::new();
                                block.insert("type".into(), Json::str("tool_call"));
                                if let Some(n) = f.get("name").and_then(Json::as_str) {
                                    block.insert("name".into(), Json::str(n));
                                }
                                if let Some(cid) = tc.get("id").and_then(Json::as_str) {
                                    block.insert("id".into(), Json::str(cid));
                                }
                                self.open_block(i, Json::Obj(block), &mut out);
                                i
                            }
                        };
                        if let Some(args) = f.get("arguments").and_then(Json::as_str) {
                            if !args.is_empty() {
                                self.delta(
                                    idx,
                                    Json::obj([
                                        ("type", Json::str("input_json_delta")),
                                        ("partial_json", Json::str(args)),
                                    ]),
                                    &mut out,
                                );
                            }
                        }
                        // A name arriving after the block opened is dropped
                        // with its presence preserved — no frame can carry it;
                        // `surface_name` empty decodes to a typed
                        // `unexpected_tool_call` refusal downstream.
                    }
                }
                if let Some(fr) = choice.get("finish_reason").and_then(Json::as_str) {
                    // Held, not emitted — a trailing usage chunk may follow
                    // (grammar G4 refuses frames after `stream_end`).
                    self.pending_stop = Some(fr.to_string());
                }
            }
        }
        Ok(out)
    }

    /// Emit the terminal frames — call at `[DONE]`/EOF: pending
    /// `block_stop`s then `stream_end{stop_reason, usage?}`. A stream that
    /// closed without a `finish_reason` yields `stream_end` with no
    /// `stop_reason` — `collect` answers `invalid_response{no_stop_reason}`,
    /// never a fabricated stop.
    pub fn finish(&mut self) -> Vec<WireFrame> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        let mut out = Vec::new();
        self.close_open(&mut out);
        let mut d = std::collections::BTreeMap::new();
        if let Some(s) = &self.pending_stop {
            d.insert("stop_reason".into(), Json::str(s.clone()));
        }
        if let Some(u) = &self.usage {
            d.insert("usage".into(), u.clone());
        }
        out.push(WireFrame {
            kind: "stream_end".to_string(),
            data: Json::Obj(d),
        });
        out
    }
}

/// `completion_frames(doc)` — a non-streaming `chat.completion` body folded
/// into the same normalized sequence (`stream_start` → blocks →
/// `stream_end`). A body that isn't a completion is `stream_decode` —
/// honest, never coerced.
pub fn completion_frames(doc: &Json) -> Result<Vec<WireFrame>, ModelError> {
    let mut tr = ChunkTranslator::default();
    let mut out = Vec::new();
    if doc.get("error").is_some() && doc.get("choices").is_none() {
        // Error body — the transport classifies these by status; reaching
        // here on a 2xx is a malformed-body error class.
        return Err(ModelError::new(
            ModelErrorClass::StreamDecode,
            "response body carries error member",
        ));
    }
    let choices = doc
        .get("choices")
        .and_then(|c| match c {
            Json::Arr(v) => Some(v.clone()),
            _ => None,
        })
        .ok_or_else(|| ModelError::new(ModelErrorClass::StreamDecode, "no choices[]"))?;
    // stream_start carries the response aliases.
    out.extend(tr.push_chunk(&Json::obj([
        ("id", doc.get("id").cloned().unwrap_or(Json::Null)),
        ("model", doc.get("model").cloned().unwrap_or(Json::Null)),
    ]))?);
    for choice in &choices {
        let msg = choice.get("message").cloned().unwrap_or(Json::Null);
        // `message` → the same delta walk the streaming path takes.
        let mut delta = std::collections::BTreeMap::new();
        if let Some(t) = msg.get("content") {
            delta.insert("content".into(), t.clone());
        }
        if let Some(Json::Arr(tcs)) = msg.get("tool_calls") {
            // chat.completion tool_calls are complete — render each as a
            // single delta carrying the full arguments payload.
            let rendered: Vec<Json> = tcs
                .iter()
                .enumerate()
                .map(|(i, tc)| {
                    let f = tc.get("function").cloned().unwrap_or(Json::Null);
                    Json::obj([
                        ("index", Json::Int(i as i64)),
                        ("id", tc.get("id").cloned().unwrap_or(Json::Null)),
                        (
                            "function",
                            Json::obj([
                                ("name", f.get("name").cloned().unwrap_or(Json::Null)),
                                (
                                    "arguments",
                                    f.get("arguments").cloned().unwrap_or(Json::Null),
                                ),
                            ]),
                        ),
                    ])
                })
                .collect();
            delta.insert("tool_calls".into(), Json::Arr(rendered));
        }
        let chunk = Json::obj([(
            "choices",
            Json::Arr(vec![Json::obj([
                ("delta", Json::Obj(delta)),
                (
                    "finish_reason",
                    choice.get("finish_reason").cloned().unwrap_or(Json::Null),
                ),
            ])]),
        )]);
        out.extend(tr.push_chunk(&chunk)?);
    }
    if let Some(u) = doc.get("usage") {
        tr.usage = Some(u.clone());
    }
    out.extend(tr.finish());
    Ok(out)
}
