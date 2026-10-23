//! `context_adapters` — the lane-backed `Embedder` / `Summarizer` /
//! `ProviderCompaction` adapters (R4.7b; BL-31's adapter member;
//! ADR-0253's fired trigger — the hosted/online budget lands real
//! adapters; the port signatures and refusal spellings stay).
//!
//! Each adapter arms against a lane's coordinates (dialect, endpoint,
//! provider model id, `env:NAME` credential coordinate, `tls` files) and
//! rides the *same* wire machinery the model lane does — the dialect
//! document, `HttpModelTransport`/`post_op` (the one raw-POST seam),
//! `BrokerCredentialPort`'s coordinate binding, `GatewayModel`'s
//! requested→call→terminal pipeline for the summarizer. Nothing here
//! re-implements HTTP, TLS, auth or JSON — CC1.
//!
//! Provider capability matrix — verified against the providers' own
//! documentation 2026-10-22 (the ticket's "verifies each provider's
//! current API from its docs and records the source + date" clause):
//!
//! | capability | openai-compatible@1 (local Ollama) | openai@1 | anthropic-messages@1 |
//! |---|---|---|---|
//! | `embeddings` | `POST {endpoint}/embeddings` (OpenAI-compatible) | `POST /v1/embeddings` (developers.openai.com — API Reference → Embeddings) | none — "Anthropic does not offer its own embedding model" (platform.claude.com — Build with Claude → Embeddings) → `Unsupported` |
//! | `summarize` | chat surface via `GatewayModel` | chat surface via `GatewayModel` | `POST /v1/messages` via `GatewayModel` |
//! | `provider_compact` | none → `Unsupported` | `POST /responses/compact` exists but is a *Responses*-surface op whose compaction item is opaque ("not intended to be human-interpretable") — it cannot answer the port's `summary_text` (developers.openai.com — API Guides → Compaction) → `Unsupported` | `POST /v1/messages` + `compaction:{type:"summarize"}` + `anthropic-beta: compact-2026-09-04` → `content[]{type:"compaction", content, signature}`, `stop_reason:"compaction"` (platform.claude.com — Build with Claude → Compaction on demand) |
//!
//! A family without the capability refuses at arm with a typed
//! `EmbedError::Unsupported{by}` — the port stays unbound, never a
//! fabricated embedding or summary.

use std::cell::RefCell;
use std::collections::BTreeMap;

use hh_context::compact::{
    SummarizeInput, Summarizer, SummarizerError, SummarizerFailure, SummarizerOutput,
};
use hh_context::compact_family::{
    ProviderCompactInput, ProviderCompactOutcome, ProviderCompaction,
};
use hh_context::retrieve::Embedder;
use hh_gateway::credentials::BrokerCredentialPort;
use hh_gateway::gateway::{CredentialHandle, CredentialPort};
use hh_gateway::http_transport::{Family, HttpModelTransport};
use hh_wire::json::Json;

use hh_embed_schema::errors::EmbedError;

use crate::gateway_model::{dialect_by_id, GatewayModel, ModelLane, TlsLane};

/// The fixed-point scale an embedding component quantizes to —
/// provider float lexemes (`parse_provider` preserves them verbatim as
/// `Json::Str`) land as `i64` parts-per-million so the port's canonical
/// records stay float-free (I-DET — the same wire bytes always quantize
/// to the same vector).
pub const EMBED_SCALE_PPM: i64 = 1_000_000;

/// The Anthropic on-demand-compaction beta header (verified
/// 2026-10-22 — the docs' `compact-2026-09-04` spelling).
const COMPACT_BETA: &str = "compact-2026-09-04";

/// The default `max_tokens` a provider-compaction call allows — the
/// provider's own bound (the docs advise several thousand; the whole
/// call, thinking included, bills against it).
const COMPACT_MAX_TOKENS: u64 = 4096;

/// `AdapterLane` — the lane coordinates an adapter arms from: the same
/// members a `model_port` slot's `ModelLane` carries, spelled as plain
/// values (the caller resolves `env:NAME` coordinates for endpoint/model
/// exactly as `decode_model_lane` does — or hands literals through).
/// `credential_ref` is the `env:NAME` *coordinate* only — the value is
/// resolved inside the transport at send, never here.
#[derive(Debug, Clone, Default)]
pub struct AdapterLane {
    /// `{dialect_id}@{version}` — the admitted coordinates.
    pub dialect_ref: String,
    /// `http(s)://host[:port][/base]`.
    pub endpoint_ref: String,
    /// The provider's model spelling (embedding model id for an
    /// `LaneEmbedder` lane — the capability models are their own ids).
    pub provider_model_id: String,
    /// `env:NAME` credential coordinate (absent = the local lane's
    /// `AuthKind::None` path).
    pub credential_ref: Option<String>,
    /// The bound profile coordinate (`model_ref.profile_ref`).
    pub profile_ref: String,
    /// Slot `provider_params` (validated against the dialect's
    /// `plan_schema` — same admitted-keys rule the lane decode applies).
    pub provider_params: BTreeMap<String, Json>,
    /// `tls.extra_roots_file` (PEM path — never the PEM body).
    pub extra_roots_file: Option<String>,
    /// `tls.client_cert_file` (mTLS chain path).
    pub client_cert_file: Option<String>,
    /// `tls.client_key_file` (mTLS key path — pairs with the chain).
    pub client_key_file: Option<String>,
}

impl From<&ModelLane> for AdapterLane {
    /// The armed `model_port` lane → the adapter coordinates (the session
    /// wires this into `KernelContext` so `Similarity` retrieval and the
    /// model-calling compaction surfaces run over the run's own lane).
    fn from(l: &ModelLane) -> Self {
        AdapterLane {
            dialect_ref: l.dialect_ref.clone(),
            endpoint_ref: l.endpoint_ref.clone(),
            provider_model_id: l.provider_model_id.clone(),
            credential_ref: l.credential_ref.clone(),
            profile_ref: l.profile_ref.clone(),
            provider_params: l.provider_params.clone(),
            extra_roots_file: l.tls.extra_roots_file.clone(),
            client_cert_file: l.tls.client_cert_file.clone(),
            client_key_file: l.tls.client_key_file.clone(),
        }
    }
}

impl AdapterLane {
    /// The `ModelLane` view (the `model_port` machinery's own shape —
    /// CC1, never a second lane type).
    fn model_lane(&self) -> ModelLane {
        ModelLane {
            dialect_ref: self.dialect_ref.clone(),
            endpoint_ref: self.endpoint_ref.clone(),
            provider_model_id: self.provider_model_id.clone(),
            credential_ref: self.credential_ref.clone(),
            substitution_allowed: false,
            profile_ref: self.profile_ref.clone(),
            provider_params: self.provider_params.clone(),
            tls: TlsLane {
                extra_roots_file: self.extra_roots_file.clone(),
                client_cert_file: self.client_cert_file.clone(),
                client_key_file: self.client_key_file.clone(),
            },
        }
    }

    /// Arm `(dialect, transport)` — the dialect document the coordinate
    /// names (unknown → `InvalidDefinition`) plus the lane's TLS client
    /// config (arm-time `Refused`, never a silently weaker session).
    fn arm_transport(
        &self,
    ) -> Result<(hh_gateway::dialect::WireDialect, HttpModelTransport), EmbedError> {
        let dialect =
            dialect_by_id(&self.dialect_ref).ok_or_else(|| EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "adapter lane dialect `{}` unknown",
                    self.dialect_ref
                )],
            })?;
        let tls = self.model_lane().tls.client_tls()?;
        Ok((dialect.clone(), HttpModelTransport::with_tls(dialect, tls)))
    }

    /// Bind the lane's credential coordinate — `binding_id` + `provided`
    /// only (the value resolves inside the transport at send). An
    /// unset `env:NAME` is a typed refusal at arm, never a mid-call
    /// surprise.
    fn bind_credential(
        &self,
        auth_kinds: &[hh_gateway::dialect::AuthKind],
    ) -> Result<CredentialHandle, EmbedError> {
        BrokerCredentialPort::default()
            .bind(
                self.credential_ref.as_deref(),
                &self.endpoint_ref,
                auth_kinds,
            )
            .map_err(|e| EmbedError::EnvironmentUnavailable {
                reason: format!("adapter lane credential: {e}"),
            })
    }
}

/// One provider endpoint component → the port's `i64` fixed-point
/// component (`× EMBED_SCALE_PPM`, round-half-away). `Json::Int` scales
/// exactly; `Json::Str` is `parse_provider`'s verbatim float lexeme —
/// non-finite or out-of-i64-range values are a typed failure, never a
/// clamped lie.
fn quantize_component(v: &Json) -> Result<i64, String> {
    match v {
        Json::Int(i) => i
            .checked_mul(EMBED_SCALE_PPM)
            .ok_or_else(|| format!("embedding component {i} overflows i64 at ppm scale")),
        Json::Str(s) => {
            let f: f64 = s
                .parse()
                .map_err(|e| format!("embedding component {s:?} is not a number lexeme: {e}"))?;
            if !f.is_finite() {
                return Err(format!("embedding component {s:?} is non-finite"));
            }
            let scaled = f * EMBED_SCALE_PPM as f64;
            if scaled.abs() > i64::MAX as f64 {
                return Err(format!(
                    "embedding component {s:?} overflows i64 at ppm scale"
                ));
            }
            Ok(scaled.round() as i64)
        }
        other => Err(format!(
            "embedding component is not a number lexeme ({:?})",
            other
        )),
    }
}

/// What a live `embeddings` call observed — dimension + the provider's
/// reported model (the ticket's "dimension + model id recorded"; the
/// caller lands it in its own evidence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedObservation {
    /// The provider-reported model id (`doc.model`), when carried.
    pub model_reported: Option<String>,
    /// The vector dimension the provider answered.
    pub dimension: u64,
}

/// `LaneEmbedder` — the `Embedder` port over an OpenAI-compatible
/// `embeddings` endpoint. `openai-compatible@1` (the local lane) and
/// `openai@1` arm; `anthropic-messages@1` refuses `Unsupported` at arm —
/// the provider publishes no embeddings API (see the module's verified
/// matrix). The port pin is the caller-declared `embedder_ref`.
pub struct LaneEmbedder {
    lane: AdapterLane,
    embedder_ref: String,
    transport: RefCell<HttpModelTransport>,
    credential: CredentialHandle,
    observed: RefCell<Option<EmbedObservation>>,
    /// The last call's verbatim provider `usage` member (the caller's
    /// charge record — `Null` before the first call).
    last_usage: RefCell<Json>,
}

impl LaneEmbedder {
    /// `new(lane, embedder_ref)` — arm the port over the lane. `by`
    /// names the refusing capability set on a family without an
    /// embeddings API.
    pub fn new(lane: AdapterLane, embedder_ref: &str) -> Result<LaneEmbedder, EmbedError> {
        let (dialect, transport) = lane.arm_transport()?;
        if hh_gateway::http_transport::family_of(&dialect) != Family::ChatCompletions {
            return Err(EmbedError::Unsupported {
                by: format!(
                    "{} has no embeddings API — the provider publishes none",
                    lane.dialect_ref
                ),
            });
        }
        let credential = lane.bind_credential(&dialect.auth_kinds)?;
        Ok(LaneEmbedder {
            lane,
            embedder_ref: embedder_ref.to_string(),
            transport: RefCell::new(transport),
            credential,
            observed: RefCell::new(None),
            last_usage: RefCell::new(Json::Null),
        })
    }

    /// The last call's observation (`None` before the first).
    pub fn observed(&self) -> Option<EmbedObservation> {
        self.observed.borrow().clone()
    }

    /// The last call's verbatim provider `usage` document.
    pub fn last_usage(&self) -> Json {
        self.last_usage.borrow().clone()
    }

    /// `embed_batch(texts)` — one `POST {endpoint}/embeddings` for the
    /// whole batch (the OpenAI-compatible `input: [..]` member);
    /// returns the vectors in request order. An empty batch makes no
    /// call (zero inputs, zero outputs — no fabricated embedding).
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<i64>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = Json::obj([
            ("model", Json::str(self.lane.provider_model_id.clone())),
            (
                "input",
                Json::Arr(texts.iter().map(|t| Json::str(t.clone())).collect()),
            ),
        ])
        .to_canonical_string()
        .into_bytes();
        let doc = self
            .transport
            .borrow_mut()
            .post_op(
                &self.credential,
                &self.lane.endpoint_ref,
                "embeddings",
                &body,
                &[],
            )
            .map_err(|e| format!("embeddings: {e}"))?;
        let data = match doc.get("data") {
            Some(Json::Arr(rows)) => rows,
            _ => return Err("embeddings: response carries no `data[]`".to_string()),
        };
        if data.len() != texts.len() {
            return Err(format!(
                "embeddings: {} rows for {} inputs — a partial answer is never reordered",
                data.len(),
                texts.len()
            ));
        }
        let mut ordered: Vec<Option<Vec<i64>>> = vec![None; texts.len()];
        let mut dimension = 0u64;
        for (pos, row) in data.iter().enumerate() {
            let idx = row
                .get("index")
                .and_then(Json::as_int)
                .map(|i| i as usize)
                .unwrap_or(pos);
            if idx >= texts.len() {
                return Err(format!("embeddings: row index {idx} out of range"));
            }
            let vec = match row.get("embedding") {
                Some(Json::Arr(cs)) => cs
                    .iter()
                    .map(quantize_component)
                    .collect::<Result<Vec<i64>, String>>()?,
                _ => return Err("embeddings: row carries no `embedding[]`".to_string()),
            };
            if pos == 0 {
                dimension = vec.len() as u64;
            } else if vec.len() as u64 != dimension {
                return Err("embeddings: ragged vectors across the batch".to_string());
            }
            ordered[idx] = Some(vec);
        }
        *self.observed.borrow_mut() = Some(EmbedObservation {
            model_reported: doc.get("model").and_then(Json::as_str).map(str::to_string),
            dimension,
        });
        *self.last_usage.borrow_mut() = doc.get("usage").cloned().unwrap_or(Json::Null);
        ordered
            .into_iter()
            .enumerate()
            .map(|(i, v)| v.ok_or_else(|| format!("embeddings: no row answered index {i}")))
            .collect()
    }
}

impl Embedder for LaneEmbedder {
    fn embedder_ref(&self) -> &str {
        &self.embedder_ref
    }

    fn embed(&self, text: &str) -> Result<Vec<i64>, String> {
        self.embed_batch(&[text.to_string()])
            .map(|mut v| v.remove(0))
    }
}

/// `LaneSummarizer` — the `Summarizer` port over `GatewayModel` (the
/// lane's full machinery: dialect render → plan validate → endpoint
/// admission → credential bind → wire → decode). The port contract's
/// provenance halves land verbatim: `SummarizerOutput.usage` carries
/// `{view, raw, served_model, model_io_ref}` — the normalized charge the
/// caller posts plus the provider's own document — and a refused/failed
/// wire leg maps onto the port's typed `SummarizerError` set, never a
/// fabricated summary.
///
/// `resolved_texts` is the caller-joined `{Text ref → body}` map the
/// port's `instructions_ref`/`previous_summary_ref` resolve through —
/// the adapter never dereferences a ref itself, and a declared-but-
/// unarmed ref is a typed `Failed{error}`, never a silently dropped
/// prompt (CC3).
pub struct LaneSummarizer {
    lane: AdapterLane,
    resolved_texts: BTreeMap<String, String>,
    /// The summarizer's own input-window bound (`SummarizerOverflow`
    /// when the offered items exceed it — §5c.2's typed row).
    input_cap_tokens: u64,
    /// The output bound when `SummarizeInput.max_summary_tokens` is
    /// absent (`max_tokens`/`max_completion_tokens` per family).
    max_output_tokens: u64,
}

impl LaneSummarizer {
    /// `new(lane, resolved_texts, input_cap_tokens, max_output_tokens)`.
    /// Any family arms — summarization rides the lane's chat surface.
    pub fn new(
        lane: AdapterLane,
        resolved_texts: BTreeMap<String, String>,
        input_cap_tokens: u64,
        max_output_tokens: u64,
    ) -> Result<LaneSummarizer, EmbedError> {
        // Arm-time validation only — the wire machinery builds per call.
        let (dialect, _t) = lane.arm_transport()?;
        let _ = lane.bind_credential(&dialect.auth_kinds)?;
        Ok(LaneSummarizer {
            lane,
            resolved_texts,
            input_cap_tokens,
            max_output_tokens,
        })
    }

    /// The resolve-or-refuse read for the port's `*_ref` members.
    fn text_for(&self, member: &str, r: &str) -> Result<&str, SummarizerError> {
        self.resolved_texts
            .get(r)
            .map(String::as_str)
            .ok_or_else(|| SummarizerError::Failed {
                kind: SummarizerFailure::Error,
                detail: format!("{member} `{r}` names no armed text — never silently dropped"),
            })
    }
}

impl Summarizer for LaneSummarizer {
    fn summarize(&self, input: &SummarizeInput) -> Result<SummarizerOutput, SummarizerError> {
        let input_tokens: u64 = input.items.iter().map(|i| i.tokens).sum();
        if input_tokens > self.input_cap_tokens {
            return Err(SummarizerError::Overflow {
                input_tokens,
                cap: self.input_cap_tokens,
            });
        }
        if input.items.is_empty() {
            return Err(SummarizerError::Failed {
                kind: SummarizerFailure::Empty,
                detail: "summarize: no input items".to_string(),
            });
        }
        let instructions = match &input.instructions_ref {
            Some(r) => self.text_for("instructions_ref", r)?.to_string(),
            None => "Summarize the following context items faithfully: keep \
                 decisions, facts and open obligations; drop dead ends."
                .to_string(),
        };
        let mut prompt = instructions;
        if let Some(r) = &input.previous_summary_ref {
            let prev = self.text_for("previous_summary_ref", r)?;
            prompt.push_str("\n\nPrevious summary (continue it, do not repeat it verbatim):\n");
            prompt.push_str(prev);
        }
        prompt.push_str("\n\nItems to summarize:\n");
        for it in &input.items {
            prompt.push_str(&format!("- {}\n", it.text));
        }

        // Per-call `GatewayModel` over the lane — the family's output
        // bound lands in `provider_params` (`max_tokens` /
        // `max_completion_tokens`, both inside each dialect's
        // `plan_schema`).
        let mut lane = self.lane.model_lane();
        let cap = input.max_summary_tokens.unwrap_or(self.max_output_tokens);
        let dialect = dialect_by_id(&lane.dialect_ref).ok_or_else(|| SummarizerError::Failed {
            kind: SummarizerFailure::Error,
            detail: format!("summarizer: dialect `{}` unbindable", lane.dialect_ref),
        })?;
        // The output bound lands on the member the *dialect* admits —
        // `max_completion_tokens` where the dialect names it (`openai@1`),
        // `max_tokens` elsewhere (`openai-compatible@1`; also the Messages
        // `required_keys` member). Neither admitted → typed failure.
        let cap_key = if dialect.plan_schema.admits("max_completion_tokens") {
            "max_completion_tokens"
        } else if dialect.plan_schema.admits("max_tokens") {
            "max_tokens"
        } else {
            return Err(SummarizerError::Failed {
                kind: SummarizerFailure::Error,
                detail: "summarizer: the dialect admits no output-token bound".to_string(),
            });
        };
        lane.provider_params
            .insert(cap_key.to_string(), Json::Int(cap as i64));
        let mut port = GatewayModel::new(lane, prompt, None, Vec::new()).map_err(|e| {
            SummarizerError::Failed {
                kind: SummarizerFailure::Error,
                detail: format!("summarizer arm: {e:?}"),
            }
        })?;
        let shot = port.one_shot("mc.summarize");
        if let Some(class) = &shot.outcome.error_class {
            // `empty_response` (the boundary's typed no-content class) maps
            // to the port's `empty` failure — never an empty summary_text.
            let kind = if shot.outcome.stop_reason == hh_gateway::vocab::StopReason::Cancelled {
                SummarizerFailure::Aborted
            } else if class == hh_gateway::vocab::ModelErrorClass::EmptyResponse.as_str() {
                SummarizerFailure::Empty
            } else {
                SummarizerFailure::Error
            };
            return Err(SummarizerError::Failed {
                kind,
                detail: format!("summarize: {class}"),
            });
        }
        if shot.text.is_empty() {
            return Err(SummarizerError::Failed {
                kind: SummarizerFailure::Empty,
                detail: "summarize: the provider answered no text".to_string(),
            });
        }
        // `tokens` — the provider's own output count when reported; else
        // the deterministic chars/4 estimate (the port member is
        // "estimated tokens" — declared, never fabricated).
        let output_tokens = shot
            .usage
            .get("record")
            .and_then(|r| r.get("view"))
            .and_then(|v| v.get("output_total"))
            .and_then(Json::as_int)
            .map(|t| t.max(0) as u64);
        let tokens = output_tokens.unwrap_or_else(|| (shot.text.len() as u64).div_ceil(4));
        // The charge record: the normalized view + the provider's raw
        // document + the served-model drift fact + the `model_io` blob's
        // content id (the caller deposits the bytes through its own
        // fenced seam — the ref is computable from the bytes, never a
        // fabricated pointer).
        let io_refs: Vec<Json> = shot
            .model_io
            .iter()
            .map(|(media, bytes)| Json::str(hh_identity::idp::address(bytes, media.as_str()).id()))
            .collect();
        let usage = Json::obj([
            ("view", shot.usage),
            ("raw", shot.usage_raw),
            (
                "served_model",
                shot.served_model.map(Json::str).unwrap_or(Json::Null),
            ),
            ("model_io_refs", Json::Arr(io_refs)),
        ]);
        Ok(SummarizerOutput {
            text: shot.text,
            tokens,
            usage,
        })
    }
}

/// `LaneProviderCompaction` — the `ProviderCompaction` port over the
/// Messages family's server-side compaction
/// (`POST {endpoint}/messages` + `compaction:{type:"summarize"}` under the
/// `compact-2026-09-04` beta). Only `anthropic-messages@1` arms — see the
/// module's verified capability matrix for the `Unsupported` reasons the
/// other families refuse with.
pub struct LaneProviderCompaction {
    lane: AdapterLane,
    /// The armed capability ref — a `provider_compact` input naming any
    /// other ref is a typed refusal, never a silent rebind.
    capability_ref: String,
    transport: RefCell<HttpModelTransport>,
    credential: CredentialHandle,
    /// `max_tokens` bound on the compaction call.
    max_tokens: u64,
}

impl LaneProviderCompaction {
    /// `new(lane, capability_ref)` — arm the port. Non-Messages families
    /// refuse `EmbedError::Unsupported{by}` at arm (the verified matrix).
    pub fn new(
        lane: AdapterLane,
        capability_ref: &str,
    ) -> Result<LaneProviderCompaction, EmbedError> {
        let (dialect, transport) = lane.arm_transport()?;
        if hh_gateway::http_transport::family_of(&dialect) != Family::Messages {
            return Err(EmbedError::Unsupported {
                by: format!(
                    "{} has no lane-native compaction API that answers a summary \
                     (verified 2026-10-22 — the openai compact endpoint's item is \
                     opaque and on the Responses surface; the local family has none)",
                    lane.dialect_ref
                ),
            });
        }
        let credential = lane.bind_credential(&dialect.auth_kinds)?;
        Ok(LaneProviderCompaction {
            lane,
            capability_ref: capability_ref.to_string(),
            transport: RefCell::new(transport),
            credential,
            max_tokens: COMPACT_MAX_TOKENS,
        })
    }

    /// The armed `max_tokens` bound — operator-tunable before `provider_compact`.
    pub fn with_max_tokens(mut self, max_tokens: u64) -> LaneProviderCompaction {
        self.max_tokens = max_tokens;
        self
    }
}

impl ProviderCompaction for LaneProviderCompaction {
    fn provider_compact(
        &self,
        input: &ProviderCompactInput,
    ) -> Result<ProviderCompactOutcome, String> {
        if input.capability_ref != self.capability_ref {
            return Err(format!(
                "unsupported{{capability_ref:`{}` ≠ armed `{}`}}",
                input.capability_ref, self.capability_ref
            ));
        }
        if input.items.is_empty() {
            return Err("provider_compact: no input items".to_string());
        }
        let messages: Vec<Json> = input
            .items
            .iter()
            .map(|it| {
                Json::obj([
                    ("role", Json::str("user")),
                    (
                        "content",
                        Json::Arr(vec![Json::obj([
                            ("type", Json::str("text")),
                            ("text", Json::str(it.text.clone())),
                        ])]),
                    ),
                ])
            })
            .collect();
        let body = Json::obj([
            ("model", Json::str(self.lane.provider_model_id.clone())),
            ("max_tokens", Json::Int(self.max_tokens as i64)),
            ("messages", Json::Arr(messages)),
            ("compaction", Json::obj([("type", Json::str("summarize"))])),
        ])
        .to_canonical_string()
        .into_bytes();
        let doc = self
            .transport
            .borrow_mut()
            .post_op(
                &self.credential,
                &self.lane.endpoint_ref,
                "messages",
                &body,
                &[("anthropic-beta", COMPACT_BETA)],
            )
            .map_err(|e| format!("provider_compact: {e}"))?;
        // The provider's own shape is the contract: `stop_reason` must be
        // `compaction` and `content[]` must carry the `compaction` block —
        // anything else is a typed failure, never a coerced summary.
        let stop = doc.get("stop_reason").and_then(Json::as_str);
        if stop != Some("compaction") {
            return Err(format!(
                "provider_compact: stop_reason `{}` ≠ `compaction` — the provider did not compact",
                stop.unwrap_or("absent")
            ));
        }
        let block = doc
            .get("content")
            .and_then(|c| match c {
                Json::Arr(blocks) => blocks
                    .iter()
                    .find(|b| b.get("type").and_then(Json::as_str) == Some("compaction")),
                _ => None,
            })
            .ok_or_else(|| "provider_compact: no `compaction` block in content[]".to_string())?;
        let summary_text = block
            .get("content")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if summary_text.is_empty() {
            return Err(
                "provider_compact: the compaction block carries no summary text".to_string(),
            );
        }
        let usage_doc = doc.get("usage").cloned().unwrap_or(Json::Null);
        // The provider reports no structured forgotten ranges — `None`
        // keeps the kernel's `forgotten = {range: all_prior}` leg and the
        // lowering-loss entry (ADR-0313 (b): the marker narrows only when
        // a provider reports the ranges — verified 2026-10-22 it does not).
        // The billed counts live under `usage.iterations[type:"compaction"]`
        // — top-level input/output are 0 on a compaction answer (verified
        // live 2026-10-22); fall back to the top-level member, then the
        // declared chars/4 estimate.
        let iter_tokens = |key: &str| -> Option<i64> {
            match usage_doc.get("iterations") {
                Some(Json::Arr(rows)) => {
                    let mut total = 0i64;
                    let mut seen = false;
                    for r in rows {
                        if r.get("type").and_then(Json::as_str) == Some("compaction") {
                            if let Some(v) = r.get(key).and_then(Json::as_int) {
                                total += v;
                                seen = true;
                            }
                        }
                    }
                    seen.then_some(total)
                }
                _ => None,
            }
        };
        let summary_tokens = iter_tokens("output_tokens")
            .or_else(|| usage_doc.get("output_tokens").and_then(Json::as_int))
            .map(|t| t.max(0) as u64)
            .unwrap_or_else(|| (summary_text.len() as u64).div_ceil(4));
        let usage = Json::obj([
            ("usage", usage_doc),
            (
                "signature",
                block.get("signature").cloned().unwrap_or(Json::Null),
            ),
            ("stop_reason", Json::str("compaction")),
            ("model", doc.get("model").cloned().unwrap_or(Json::Null)),
        ]);
        Ok(ProviderCompactOutcome {
            summary_text,
            summary_tokens,
            forgotten_ids: None,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AdapterLane::from(&ModelLane)` — the armed `model_port` lane
    /// carries every member verbatim into the adapter coordinates (the
    /// boundary wiring never re-derives a lane).
    #[test]
    fn adapter_lane_from_model_lane_carries_every_member() {
        let mut ml = ModelLane {
            dialect_ref: "openai-compatible@1".to_string(),
            endpoint_ref: "http://127.0.0.1:9/v1".to_string(),
            provider_model_id: "m".to_string(),
            credential_ref: Some("env:HH_TEST_NONE".to_string()),
            substitution_allowed: false,
            profile_ref: "profile/p".to_string(),
            provider_params: BTreeMap::from([("seed".to_string(), Json::Int(1))]),
            tls: TlsLane {
                extra_roots_file: Some("roots.pem".to_string()),
                client_cert_file: None,
                client_key_file: None,
            },
        };
        let al = AdapterLane::from(&ml);
        assert_eq!(al.dialect_ref, ml.dialect_ref);
        assert_eq!(al.endpoint_ref, ml.endpoint_ref);
        assert_eq!(al.provider_model_id, ml.provider_model_id);
        assert_eq!(al.credential_ref, ml.credential_ref);
        assert_eq!(al.profile_ref, ml.profile_ref);
        assert_eq!(al.provider_params, ml.provider_params);
        assert_eq!(al.extra_roots_file, ml.tls.extra_roots_file);
        // and the round-trip back is lossless.
        ml.substitution_allowed = false;
        let back = al.model_lane();
        assert_eq!(back.dialect_ref, ml.dialect_ref);
        assert_eq!(back.endpoint_ref, ml.endpoint_ref);
        assert_eq!(back.tls.extra_roots_file, ml.tls.extra_roots_file);
    }
}
