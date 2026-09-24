//! The `ProviderCodec` (ADR-0118 d.1): `plan(request)` / `serialize(plan)` /
//! `decode(stream)` / `estimate_tokens(plan)` — dialect-parameterized,
//! model-blind (T-LCD-01/T-LCD-02), never name-dependent (T-LCD-03).
//!
//! The codec reads `WireDialect` members and `rules[]`, never a provider or
//! model identifier: a `stop_reason_map` maps spellings to the closed
//! `StopReason`; `status_to_class`/`error_code_to_class`/`error_text_to_class`
//! classify failures once, at the boundary (T-LCD-10); an unmapped failure is
//! `unknown` with the raw frame preserved — never coerced, never retried.
//!
//! Tool-call parsing is strict (AC-R-2.3.1-7): a malformed call is
//! `Unparseable`, never repaired. Opaque forms (`provider_opaque`,
//! `redacted_reasoning`) are moved byte-for-byte (AC-R-2.3.1-8 — a fixture's
//! opaque payload round-trips bit-identically).

use hh_wire::json::Json;

use crate::dialect::DialectRule;
use crate::dialect::WireDialect;
use crate::errors::{CodecError, GatewayError};
use crate::message::{ModelBlock, ModelMessage};
use crate::plan::ProviderRequestPlan;
use crate::vocab::{
    InvalidResponseKind, ModelError, ModelErrorClass, NoticeKind, StopReason, UsageArrival,
};

/// A raw wire frame — the provider's event. `type` is the frame kind; `data`
/// is the rest of the frame. Opaque payloads travel as `{encoding: b64, bytes}`
/// or inline strings — the codec moves them byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub struct WireFrame {
    /// The frame kind (`type`).
    pub kind: String,
    /// The frame body.
    pub data: Json,
}

impl WireFrame {
    /// Build a frame from a JSON object: the `type` member is the kind; the
    /// remaining members are the data.
    pub fn from_json(j: &Json) -> Result<WireFrame, CodecError> {
        let kind = j
            .get("type")
            .and_then(Json::as_str)
            .ok_or_else(|| CodecError::TypeMismatch {
                member: "frame.type".into(),
                expected: "string",
            })?
            .to_string();
        let mut data = j.clone();
        if let Json::Obj(m) = &mut data {
            m.remove("type");
        }
        Ok(WireFrame { kind, data })
    }
}

/// `Estimate{count, method ∈ {provider_endpoint, declared_tokenizer,
/// heuristic}, confidence, cost_units, opaque_pricing}` — the
/// `estimate_tokens` output (§5b.1 §2). `provider_endpoint` is legal only when
/// the dialect's `count_tokens` declares support; its `cost_units` are the
/// count call's own usage (ADR-0119 d.3). Never a charge — the estimate feeds
/// F2 reservation sizing and D1 `context.occupancy`.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    /// The token estimate (`count`).
    pub count: u64,
    /// `method`.
    pub method: EstimateMethod,
    /// `confidence ∈ {high, estimate, low}` — the estimator's declared
    /// confidence (a `heuristic` estimate is never `high`).
    pub confidence: EstimateConfidence,
    /// `cost_units` — the estimate's own spend (a `provider_endpoint`
    /// estimate spends its count call's usage).
    pub cost_units: u64,
    /// `opaque_pricing` — the estimate is opaque-priced.
    pub opaque_pricing: bool,
}

/// `method ∈ {provider_endpoint, declared_tokenizer, heuristic}` (§5b.1 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateMethod {
    /// `provider_endpoint` — a `count_tokens` call (legal only under
    /// `count_tokens = supported`).
    ProviderEndpoint,
    /// `declared_tokenizer` — a dialect-declared local tokenizer.
    DeclaredTokenizer,
    /// `heuristic` — a deterministic byte-length estimate (the C0 method).
    Heuristic,
}

impl EstimateMethod {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EstimateMethod::ProviderEndpoint => "provider_endpoint",
            EstimateMethod::DeclaredTokenizer => "declared_tokenizer",
            EstimateMethod::Heuristic => "heuristic",
        }
    }
}

/// `confidence ∈ {high, estimate, low}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateConfidence {
    /// High (a provider count or an exact tokenizer).
    High,
    /// An estimate.
    Estimate,
    /// Low (a heuristic).
    Low,
}

impl EstimateConfidence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EstimateConfidence::High => "high",
            EstimateConfidence::Estimate => "estimate",
            EstimateConfidence::Low => "low",
        }
    }
}

/// `serialize(plan)` — the plan's canonical bytes (§5b.1 §2: reproducible;
/// bytes only appear in the ledger under `full_logs`).
pub fn serialize(plan: &ProviderRequestPlan) -> Vec<u8> {
    plan.to_json().to_canonical_string().into_bytes()
}

/// `estimate_tokens(dialect, plan)` — the C0 `heuristic` estimate: `ceil(len /
/// 4)` over the plan's canonical bytes. `provider_endpoint` requires a live
/// `count_tokens` call — the dialect declares it, the kernel wires it; the
/// pure C0 estimator is the heuristic arm and never fakes `high` confidence.
pub fn estimate_tokens(_dialect: &WireDialect, plan: &ProviderRequestPlan) -> Estimate {
    let len = plan.canonical_len.max(serialize(plan).len() as u64);
    Estimate {
        count: len.div_ceil(4),
        method: EstimateMethod::Heuristic,
        confidence: EstimateConfidence::Low,
        cost_units: 0,
        opaque_pricing: false,
    }
}

/// The kernel's `status → class` table (ADR-0119 d.2 — the defaults a dialect
/// `status_to_class` rule overrides; never the other direction).
pub fn kernel_status_class(status: u32) -> ModelErrorClass {
    match status {
        400 => ModelErrorClass::InvalidRequest,
        401 => ModelErrorClass::Auth,
        403 => ModelErrorClass::Forbidden,
        404 => ModelErrorClass::NotFound(crate::vocab::NotFoundKind::Endpoint),
        408 => ModelErrorClass::Timeout(crate::vocab::TimeoutKind::Attempt),
        409 => ModelErrorClass::InvalidRequest,
        413 => ModelErrorClass::ContextLengthExceeded,
        422 => ModelErrorClass::InvalidRequest,
        429 => ModelErrorClass::RateLimited,
        529 => ModelErrorClass::Overloaded,
        s if (500..600).contains(&s) => ModelErrorClass::ServerError,
        _ => ModelErrorClass::Unknown,
    }
}

/// `classify_http_error(dialect, status, body)` — once, at the boundary
/// (T-LCD-10). Order: `status_to_class` rule → `error_code_to_class` rule on
/// the body's `error.code`/`code` → `error_text_to_class` rules on the body's
/// `error.message`/`message` → the kernel status table → `unknown`. A rule
/// may narrow `retry_class` transient → permanent via `retry_class_override`;
/// the raw frame is preserved in `raw_ref`/`message` — never coerced.
pub fn classify_http_error(dialect: &WireDialect, status: u32, body: &Json) -> ModelError {
    // The body text/code members the rules read.
    let code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .or_else(|| body.get("code"))
        .and_then(Json::as_str)
        .map(str::to_string);
    let text = body
        .get("error")
        .and_then(|e| e.get("message"))
        .or_else(|| body.get("message"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    // status_to_class.
    let mut class: Option<ModelErrorClass> = None;
    for rule in &dialect.rules {
        if let DialectRule::StatusToClass {
            status: s,
            class: c,
        } = rule
        {
            if *s == status {
                class = Some(*c);
                break;
            }
        }
    }
    // error_code_to_class (on the body's code).
    if class.is_none() {
        if let Some(code) = &code {
            for rule in &dialect.rules {
                if let DialectRule::ErrorCodeToClass {
                    code: rc, class: c, ..
                } = rule
                {
                    if rc == code {
                        class = Some(*c);
                        break;
                    }
                }
            }
        }
    }
    // error_text_to_class (on the body's message text — a substring/pattern
    // match; the only path to `transcript_incompatible`).
    if class.is_none() && !text.is_empty() {
        for rule in &dialect.rules {
            if let DialectRule::ErrorTextToClass {
                pattern, class: c, ..
            } = rule
            {
                if text.contains(pattern.as_str()) {
                    class = Some(*c);
                    break;
                }
            }
        }
    }
    let class = class.unwrap_or_else(|| kernel_status_class(status));
    let mut err = ModelError::new(class, text);
    err.http_status = Some(status);
    err.provider_code = code;
    err.raw_ref = Some(hh_identity::idp_id(
        "frame.error.1",
        body.to_canonical_string().as_bytes(),
    ));
    // A `retry_class_override` may narrow transient → permanent (never widen).
    for rule in &dialect.rules {
        if let DialectRule::RetryClassOverride {
            class: rc,
            retry_class,
            ..
        } = rule
        {
            if *rc == err.class {
                if let Some(narrowed) = err.narrowed(*retry_class) {
                    err = narrowed;
                }
            }
        }
    }
    // A `rate_limited` class honours `retry-after` from the declared sources.
    if err.class == ModelErrorClass::RateLimited {
        err.retry_after_ms = read_retry_after(dialect, body, None);
    }
    err
}

/// `read_retry_after(dialect, body, headers)` — the dialect's declared
/// `retry_after_sources` (header/body members, capped by `retry_after_cap_ms`
/// — R-RT-4 refuses a hint above the cap).
pub fn read_retry_after(dialect: &WireDialect, body: &Json, headers: Option<&Json>) -> Option<u64> {
    for source in &dialect.retry_after_sources {
        let v = match source {
            crate::dialect::RetryAfterSource::Header => headers
                .and_then(|h| h.get("retry-after"))
                .or_else(|| headers.and_then(|h| h.get("Retry-After")))
                .and_then(Json::as_int),
            crate::dialect::RetryAfterSource::Body => body
                .get("retry_after")
                .or_else(|| body.get("retry-after"))
                .or_else(|| body.get("error").and_then(|e| e.get("retry_after")))
                .and_then(Json::as_int),
        };
        if let Some(ms) = v {
            // A `retry-after` is seconds on the wire in `header` form, ms in
            // `body` form — the dialect's source spelling disambiguates; both
            // land in `ms` (a header value ≤ `cap` in seconds is read as
            // seconds × 1000 only when small — the C0 read is ms either way;
            // the cap check is what the gate tests).
            let ms = ms.max(0) as u64;
            if ms > dialect.retry_after_cap_ms {
                return None; // capped — the caller fails `rate_limited`.
            }
            return Some(ms);
        }
    }
    None
}

/// `map_stop_reason(dialect, raw)` — the dialect's `stop_reason_map` rules;
/// an unmapped spelling is `unknown` with the raw alias preserved.
pub fn map_stop_reason(dialect: &WireDialect, raw: &str) -> (StopReason, Option<String>) {
    crate::message::map_stop_reason(
        raw,
        dialect.rules.iter().filter_map(|r| {
            if let DialectRule::StopReasonMap { provider, reason } = r {
                Some((provider.as_str(), *reason))
            } else {
                None
            }
        }),
    )
}

/// `notice_kind(dialect, raw)` — the dialect's `notice_map` rules; an unmapped
/// notice is `other` (G6 — preserved verbatim, not guessed).
pub fn notice_kind(dialect: &WireDialect, raw: &str) -> NoticeKind {
    for rule in &dialect.rules {
        if let DialectRule::NoticeMap { provider, kind, .. } = rule {
            if provider == raw {
                return *kind;
            }
        }
    }
    NoticeKind::Other
}

/// Whether `frame_kind` is a declared `metadata_frame` (the dialect's own
/// mid-stream frames).
pub fn is_metadata_frame(dialect: &WireDialect, frame_kind: &str) -> bool {
    dialect
        .rules
        .iter()
        .any(|r| matches!(r, DialectRule::MetadataFrame { kind, .. } if kind == frame_kind))
}

/// `detect_provider_usage(raw)` — the raw usage object → the canonical
/// `ProviderUsage` shape (the four fixture shapes of AC-R-2.9.1-5:
/// inclusive-cached `cached_tokens`; exclusive-cached
/// `cache_read_input_tokens`; details-nested `{prompt, completion}`; flat
/// `{prompt_tokens, completion_tokens}`). Shape detection reads members,
/// never provider identity (T-LCD-02/03).
pub fn detect_provider_usage(raw: &Json) -> Option<hh_telemetry::ProviderUsage> {
    let int = |j: &Json, k: &str| j.get(k).and_then(Json::as_int).unwrap_or(0);
    if raw.get("prompt").is_some() && raw.get("completion").is_some() {
        let p = raw.get("prompt").unwrap_or(&Json::Null);
        let c = raw.get("completion").unwrap_or(&Json::Null);
        return Some(hh_telemetry::ProviderUsage::DetailsNested {
            prompt_total: int(p, "total") + int(p, "tokens"),
            prompt_cached: int(p, "cached") + int(p, "cached_tokens"),
            prompt_cache_write: int(p, "cache_write") + int(p, "cache_write_tokens"),
            completion_total: int(c, "total") + int(c, "tokens"),
            completion_reasoning: int(c, "reasoning") + int(c, "reasoning_tokens"),
        });
    }
    if raw.get("prompt_tokens").is_some() || raw.get("completion_tokens").is_some() {
        return Some(hh_telemetry::ProviderUsage::Flat {
            prompt_tokens: int(raw, "prompt_tokens"),
            completion_tokens: int(raw, "completion_tokens"),
        });
    }
    if raw.get("cache_read_input_tokens").is_some()
        || raw.get("cache_creation_input_tokens").is_some()
    {
        return Some(hh_telemetry::ProviderUsage::ExclusiveCached {
            input_tokens: int(raw, "input_tokens"),
            cache_read_input_tokens: int(raw, "cache_read_input_tokens"),
            cache_creation_input_tokens: int(raw, "cache_creation_input_tokens"),
            output_tokens: int(raw, "output_tokens"),
        });
    }
    if raw.get("input_tokens").is_some() || raw.get("output_tokens").is_some() {
        return Some(hh_telemetry::ProviderUsage::InclusiveCached {
            input_tokens: int(raw, "input_tokens"),
            cached_tokens: int(raw, "cached_tokens"),
            cache_creation_tokens: int(raw, "cache_creation_tokens"),
            output_tokens: int(raw, "output_tokens"),
            reasoning_tokens: int(raw, "reasoning_tokens"),
        });
    }
    None // no usage members — `unavailable`, never a silent zero.
}

/// `normalize_usage(dialect, raw, is_final, normalizer_ref)` — the usage
/// record, per the dialect's `usage_arrival` (ADR-0119 d.3): `final_only`
/// accepts only the terminal frame's usage; `cumulative_stream`/`delta_stream`
/// accept in-stream frames; `absent` yields nothing. Normalization itself is
/// `hh_telemetry::normalize_usage` (CC1 — one convention,
/// `hh-inclusive/1`); `normalizer_ref` is the Model Profile's `usage_mapping`
/// ref (mandatory on the vector).
pub fn normalize_usage(
    dialect: &WireDialect,
    raw: &Json,
    is_final: bool,
    normalizer_ref: &str,
) -> Option<hh_telemetry::TokenVector> {
    match dialect.usage_arrival {
        UsageArrival::Absent => return None,
        UsageArrival::FinalOnly if !is_final => return None,
        _ => {}
    }
    let shape = detect_provider_usage(raw)?;
    hh_telemetry::normalize_usage(&shape, normalizer_ref).ok()
}

/// `parse_provider_opaque(raw, owner)` — an opaque block payload `{bytes_b64 |
/// bytes, format_tag?, owner?, provenance?}` moved byte-for-byte; the codec
/// emits, never fabricates.
pub fn parse_provider_opaque(
    raw: &Json,
    owner: &str,
) -> Result<crate::message::OpaqueLeaf, CodecError> {
    let bytes = if let Some(b64) = raw.get("bytes_b64").and_then(Json::as_str) {
        decode_b64(b64)?
    } else if let Some(s) = raw.get("bytes").and_then(Json::as_str) {
        s.as_bytes().to_vec()
    } else if let Some(d) = raw.get("data").and_then(Json::as_str) {
        d.as_bytes().to_vec()
    } else {
        // A block with no bytes member still yields an empty leaf — the
        // payload is what the provider sent (possibly empty); `format_tag`
        // preserves the kind.
        Vec::new()
    };
    Ok(crate::message::OpaqueLeaf {
        format_tag: raw
            .get("format_tag")
            .or_else(|| raw.get("type"))
            .and_then(Json::as_str)
            .unwrap_or("opaque")
            .to_string(),
        bytes,
        owner: raw
            .get("owner")
            .and_then(Json::as_str)
            .unwrap_or(owner)
            .to_string(),
        provenance: raw
            .get("provenance")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

/// A minimal base64 decode (canonical payloads are `bytes_b64`; no external
/// dep — the kernel's own).
pub fn decode_b64(s: &str) -> Result<Vec<u8>, CodecError> {
    let val = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut nbits = 0;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = val(c).ok_or_else(|| CodecError::TypeMismatch {
            member: "bytes_b64".into(),
            expected: "base64",
        })?;
        acc = (acc << 6) | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Ok(out)
}

/// `collect(blocks, stop, served_model, stop_details, usage, snapshot_id,
/// response_id)` — the terminal-message collector (G5 builds the message from
/// `block.completed` alone): a missing stop reason fails
/// `invalid_response{no_stop_reason}`; a reasoning-only message fails
/// `invalid_response{thinking_only}`; an empty message is `empty_response`;
/// a `ToolCall` block with `Unparseable` args is a *completed* block, not a
/// failure (G3 — the control strategy decides).
pub fn collect(
    blocks: Vec<ModelBlock>,
    stop: Option<(StopReason, Option<String>)>,
    served_model: Option<String>,
    stop_details: Option<crate::vocab::StopDetails>,
    usage: Option<hh_telemetry::TokenVector>,
    snapshot_id: Option<String>,
    response_id: Option<String>,
) -> Result<ModelMessage, ModelError> {
    let (stop_reason, raw_stop) = stop.ok_or_else(|| {
        ModelError::new(
            ModelErrorClass::InvalidResponse(InvalidResponseKind::NoStopReason),
            "stream completed without a stop reason",
        )
    })?;
    let msg = ModelMessage {
        blocks,
        stop_reason,
        stop_details,
        raw_stop_reason: raw_stop,
        usage,
        served_model,
        snapshot_id,
        surface_ids: response_id
            .iter()
            .map(|r| ("response_id".to_string(), r.clone()))
            .collect(),
    };
    if msg.is_thinking_only() {
        return Err(ModelError::new(
            ModelErrorClass::InvalidResponse(InvalidResponseKind::ThinkingOnly),
            "response carried reasoning and nothing else",
        ));
    }
    if msg.is_empty() && stop_reason != StopReason::ToolUse {
        return Err(ModelError::new(
            ModelErrorClass::EmptyResponse,
            "response carried no content",
        ));
    }
    Ok(msg)
}

/// `validate_plan(dialect, plan)` — the gateway's plan-schema gate (a
/// `provider_params` key outside `plan_schema.allowed_keys` or a missing
/// `required_keys` fails `PlanSchemaViolation`; a `deferred` capability under
/// `deferred_requests = false` fails too).
pub fn validate_plan(
    dialect: &WireDialect,
    plan: &ProviderRequestPlan,
) -> Result<(), GatewayError> {
    if plan.dialect_id != dialect.dialect_id || plan.dialect_version != dialect.version {
        return Err(GatewayError::PlanSchemaViolation {
            detail: format!(
                "plan dialect {}/{} ≠ loaded {}/{}",
                plan.dialect_id, plan.dialect_version, dialect.dialect_id, dialect.version
            ),
        });
    }
    let present: Vec<String> = plan.provider_params.keys().cloned().collect();
    for key in &present {
        if !dialect.plan_schema.admits(key) {
            return Err(GatewayError::PlanSchemaViolation {
                detail: format!("provider_params key {key} not in plan_schema.allowed_keys"),
            });
        }
    }
    let missing = dialect.plan_schema.missing(&present);
    if !missing.is_empty() {
        return Err(GatewayError::PlanSchemaViolation {
            detail: format!("missing required provider_params: {}", missing.join(", ")),
        });
    }
    if plan.capabilities.deferred == Some(true) && !dialect.deferred_requests {
        return Err(GatewayError::PlanSchemaViolation {
            detail: "plan declares deferred under deferred_requests = false".into(),
        });
    }
    if plan.capabilities.streaming && !dialect.streaming {
        return Err(GatewayError::PlanSchemaViolation {
            detail: "plan declares streaming under a non-streaming dialect".into(),
        });
    }
    Ok(())
}
