//! `dialects` — the built-in `WireDialect` documents (R4.5b; ADR-0364 D2).
//!
//! The bound dialect is a *document* — a `WireDialect` descriptor value the
//! run's `model_port` slot names by `{dialect_id}@{version}` and the armed
//! `ModelGateway` resolves from its loaded set. The OpenAI-compatible family
//! (`chat/completions` + SSE + `/models` listing — Ollama's `/v1` surface
//! included) is the one dialect R4.5b admits: its document lives here so the
//! wire contract is a loadable record (`dialect.to_json()` is the canonical
//! body; `content_id()` its `dialect.1` address), not scattered constants.

use crate::cache::CacheSemantics;
use crate::dialect::{
    DebtEvidence, DebtRecord, DialectRule, Framing, PlanSchema, RetryAfterSource,
    UsageMappingDefault, WireDialect,
};
use crate::vocab::{ModelErrorClass, NotFoundKind, StopReason, UsageArrival};

/// `OPENAI_COMPATIBLE_DIALECT_ID` — the coordinate the `model_port` slot's
/// `dialect` member spells (`openai-compatible@1`).
pub const OPENAI_COMPATIBLE_ID: &str = "openai-compatible@1";

/// The debt record the family's class-mapping rules carry (ADR-0119 d.5 —
/// every `error_*_to_class`/`retry_class_override` rule ships its complete
/// debt row: assumption, counter-case, pinned evidence, detection signal,
/// and the executable removal test).
fn provider_debt(what: &str, wrong: &str, removal_test: &str) -> DebtRecord {
    DebtRecord {
        what_we_assume: what.to_string(),
        how_we_could_be_wrong: wrong.to_string(),
        evidence: vec![DebtEvidence {
            reference: "https://platform.openai.com/docs/api-reference".to_string(),
            accessed: "2026-10-21".to_string(),
        }],
        detection_signal:
            "live-run model.call.failed rows whose class disagrees with the provider's intent"
                .to_string(),
        removal_test: removal_test.to_string(),
    }
}

/// `openai_compatible()` — the `openai-compatible@1` dialect document:
/// the OpenAI chat-completions wire surface any conforming local endpoint
/// serves (Ollama `/v1`, vLLM, llama.cpp server, LM Studio).
///
/// - `framing: sse` — streaming responses are `text/event-stream`
///   `chat.completion.chunk` JSON lines terminated by `data: [DONE]`.
/// - `usage_arrival: final_only` — usage rides the terminal chunk
///   (`stream_options.include_usage`) or the non-streaming body.
/// - `auth_kinds: [none, bearer]` — `none` for loopback; a brokered bearer
///   is declared admissible for R4.5c/d's hosted half.
/// - `cache_semantics: uncached` — the family exposes no explicit cache
///   contract to this layer (Ollama's internal prefix reuse is
///   provider-internal — never claimed).
/// - `retry_after_cap_ms: 60_000` — R-RT-4's bound; a hint above it fails
///   the call `rate_limited` with `will_retry = false`.
pub fn openai_compatible() -> WireDialect {
    WireDialect {
        dialect_id: "openai-compatible".to_string(),
        version: "1".to_string(),
        framing: Framing::Sse,
        // `provider_params` are the request body's *extension* members the
        // lowering may add — the closed admit set (anything else the slot
        // declares is a plan-schema refusal, not a silent drop).
        plan_schema: PlanSchema {
            allowed_keys: [
                "temperature",
                "top_p",
                "max_tokens",
                "seed",
                "stream_options",
                "tool_choice",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            required_keys: vec![],
        },
        envelope_fields: ["model", "messages", "tools", "stream"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        auth_kinds: vec![
            crate::dialect::AuthKind::None,
            crate::dialect::AuthKind::Bearer,
        ],
        endpoint_allowlist_ref: "hh-embed/model_port.slot".to_string(),
        streaming: true,
        usage_arrival: UsageArrival::FinalOnly,
        usage_mapping_default: UsageMappingDefault::PreferProvider,
        usage_mapping: None,
        cache_semantics: CacheSemantics::Uncached,
        cache_state_visible: false,
        count_tokens: None,
        deferred_requests: false,
        model_listing: Some("data".to_string()),
        snapshot_id_exposed: None,
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 120_000,
        prefix_affecting_params: ["model", "messages", "tools", "stream", "temperature", "seed"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rules: vec![
            // `status_to_class` — the family's documented statuses, mapped
            // once at the boundary (T-LCD-10).
            DialectRule::StatusToClass {
                status: 400,
                class: ModelErrorClass::InvalidRequest,
            },
            DialectRule::StatusToClass {
                status: 401,
                class: ModelErrorClass::Auth,
            },
            DialectRule::StatusToClass {
                status: 403,
                class: ModelErrorClass::Forbidden,
            },
            DialectRule::StatusToClass {
                status: 404,
                class: ModelErrorClass::NotFound(NotFoundKind::Endpoint),
            },
            DialectRule::StatusToClass {
                status: 422,
                class: ModelErrorClass::InvalidRequest,
            },
            DialectRule::StatusToClass {
                status: 429,
                class: ModelErrorClass::RateLimited,
            },
            DialectRule::StatusToClass {
                status: 500,
                class: ModelErrorClass::ServerError,
            },
            DialectRule::StatusToClass {
                status: 503,
                class: ModelErrorClass::ServerError,
            },
            DialectRule::StatusToClass {
                status: 529,
                class: ModelErrorClass::Overloaded,
            },
            // `error_code_to_class` — the documented `error.code` spellings.
            DialectRule::ErrorCodeToClass {
                code: "context_length_exceeded".to_string(),
                class: ModelErrorClass::ContextLengthExceeded,
                debt: provider_debt(
                    "context overflow is spelled `context_length_exceeded` in error.code",
                    "a conforming endpoint may spell it differently — the kernel's status table or `unknown` then stands",
                    "crates/hh-embed/tests/r4_5b.rs::context_length_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "model_not_found".to_string(),
                class: ModelErrorClass::NotFound(NotFoundKind::Model),
                debt: provider_debt(
                    "a missing model is spelled `model_not_found` in error.code",
                    "a conforming endpoint may answer bare 404 — the status table's `not_found{endpoint}` stands",
                    "crates/hh-embed/tests/r4_5b.rs::not_found_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "insufficient_quota".to_string(),
                class: ModelErrorClass::QuotaExhausted,
                debt: provider_debt(
                    "quota exhaustion is spelled `insufficient_quota` in error.code",
                    "local endpoints never emit it — the rule is dormant for loopback",
                    "crates/hh-embed/tests/r4_5b.rs::quota_class_maps",
                ),
            },
            // `retry_class_override` — a 500-class failure stays transient
            // only while the policy allows; this family's no narrowing —
            // none declared (narrowing is the only direction a rule may
            // take; ADR-0119 d.2).
            // `stop_reason_map` — chat.completions `finish_reason` spellings.
            DialectRule::StopReasonMap {
                provider: "stop".to_string(),
                reason: StopReason::EndTurn,
            },
            DialectRule::StopReasonMap {
                provider: "tool_calls".to_string(),
                reason: StopReason::ToolUse,
            },
            DialectRule::StopReasonMap {
                provider: "function_call".to_string(),
                reason: StopReason::ToolUse,
            },
            DialectRule::StopReasonMap {
                provider: "length".to_string(),
                reason: StopReason::MaxOutput,
            },
            DialectRule::StopReasonMap {
                provider: "content_filter".to_string(),
                reason: StopReason::ContentFilter,
            },
            // No `retry_class_override` — narrowing is the only direction
            // a rule may take (ADR-0119 d.2) and the family declares none.
        ],
    }
}

/// The descriptor's canonical document body (`dialect.to_json()` — the
/// committed-record form of the bound dialect).
pub fn openai_compatible_doc() -> hh_wire::json::Json {
    openai_compatible().to_json()
}
