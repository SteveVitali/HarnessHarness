//! `dialects` — the built-in `WireDialect` documents (R4.5b; ADR-0364 D2;
//! R4.5d's hosted pair — ADR-0366).
//!
//! The bound dialect is a *document* — a `WireDialect` descriptor value the
//! run's `model_port` slot names by `{dialect_id}@{version}` and the armed
//! `ModelGateway` resolves from its loaded set. The documents live here so
//! the wire contract is a loadable record (`dialect.to_json()` is the
//! canonical body; `content_id()` its `dialect.1` address), not scattered
//! constants:
//!
//! - `openai-compatible@1` — the OpenAI chat-completions wire surface any
//!   conforming local endpoint serves (Ollama `/v1`, vLLM, llama.cpp, LM
//!   Studio): `none`/`bearer` auth, `uncached` semantics (R4.5b).
//! - `openai@1` — hosted OpenAI's canonical Chat Completions surface
//!   (`api.openai.com/v1`): `bearer` only, `implicit_prefix` cache
//!   semantics (`prompt_cache_key`, ≥1024-token eligibility, 128-token
//!   reporting granularity). Chat Completions — not Responses — is the
//!   admitted surface: the corpus-proven `ChatChunkTranslator` already
//!   carries the SSE delta/tool/usage grammar this family spells
//!   (ADR-0366 D2).
//! - `anthropic-messages@1` — Anthropic's Messages API
//!   (`api.anthropic.com/v1`): `x-api-key` + `anthropic-version` headers,
//!   `explicit_breakpoints` cache semantics (max 4 `cache_control`
//!   markers, 5m/1h TTL classes, 1024-token floor), `cumulative_stream`
//!   usage (input counters at `message_start`, the output counter
//!   cumulative per `message_delta`), `max_tokens` a required
//!   `provider_params` key (ADR-0366 D3).

use crate::cache::{
    AffinitySupport, CacheSemantics, ImplicitStrictness, IsolationScope, MarkerPositionRule,
    RetentionClass,
};
use crate::dialect::{
    AuthKind, DebtEvidence, DebtRecord, DialectRule, Framing, PlanSchema, RetryAfterSource,
    UsageMappingDefault, WireDialect,
};
use crate::vocab::{
    BlockKind, ModelErrorClass, NotFoundKind, StopReason, TimeoutKind, UsageArrival,
};

/// `OPENAI_COMPATIBLE_DIALECT_ID` — the coordinate the `model_port` slot's
/// `dialect` member spells (`openai-compatible@1`).
pub const OPENAI_COMPATIBLE_ID: &str = "openai-compatible@1";

/// `OPENAI_DIALECT_ID` — hosted OpenAI Chat Completions (`openai@1`).
pub const OPENAI_ID: &str = "openai@1";

/// `ANTHROPIC_MESSAGES_DIALECT_ID` — Anthropic Messages
/// (`anthropic-messages@1`).
pub const ANTHROPIC_MESSAGES_ID: &str = "anthropic-messages@1";

/// The debt record a family's class-mapping rules carry (ADR-0119 d.5 —
/// every `error_*_to_class`/`retry_class_override`/`metadata_frame` rule
/// ships its complete debt row: assumption, counter-case, pinned evidence,
/// detection signal, and the executable removal test). `reference` is the
/// pinned provider doc the rule's evidence names.
fn provider_debt_at(reference: &str, what: &str, wrong: &str, removal_test: &str) -> DebtRecord {
    DebtRecord {
        what_we_assume: what.to_string(),
        how_we_could_be_wrong: wrong.to_string(),
        evidence: vec![DebtEvidence {
            reference: reference.to_string(),
            accessed: "2026-10-21".to_string(),
        }],
        detection_signal:
            "live-run model.call.failed rows whose class disagrees with the provider's intent"
                .to_string(),
        removal_test: removal_test.to_string(),
    }
}

/// `provider_debt` — the OpenAI-referenced spelling (the R4.5b family's
/// evidence ref; kept so the `openai-compatible@1` document's bytes are
/// unchanged).
fn provider_debt(what: &str, wrong: &str, removal_test: &str) -> DebtRecord {
    provider_debt_at(
        "https://platform.openai.com/docs/api-reference",
        what,
        wrong,
        removal_test,
    )
}

/// The evidence ref the Anthropic-family rules pin (the Messages reference
/// the rules' class mappings were read against).
const ANTHROPIC_REF: &str = "https://docs.anthropic.com/en/api/messages";

/// The evidence ref the hosted-OpenAI rules pin (the Chat Completions
/// reference — the admitted OpenAI surface, ADR-0366 D2).
const OPENAI_REF: &str = "https://platform.openai.com/docs/api-reference/chat";

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
        prefix_affecting_params: [
            "model",
            "messages",
            "tools",
            "stream",
            "temperature",
            "seed",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rules: chat_completions_rules(),
    }
}

/// `chat_completions_rules()` — the rule set the Chat Completions surface
/// shares across its serving forms (the R4.5b local family and R4.5d's
/// hosted `openai@1`): the family's documented `status → class` rows, the
/// `error.code` spellings, and the `finish_reason` stop map (CC1 — one
/// spelling per fact; hosted-only rows are appended by `openai()`).
fn chat_completions_rules() -> Vec<DialectRule> {
    vec![
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
    ]
}

/// The descriptor's canonical document body (`dialect.to_json()` — the
/// committed-record form of the bound dialect).
pub fn openai_compatible_doc() -> hh_wire::json::Json {
    openai_compatible().to_json()
}

/// `openai()` — the `openai@1` dialect document: hosted OpenAI's Chat
/// Completions surface (`POST /v1/chat/completions` + SSE chunks +
/// `GET /v1/models`). The wire grammar is the family's own — what differs
/// from the local-compatible sibling is *hosting facts*, recorded here
/// once:
///
/// - `auth_kinds: [bearer]` — the hosted endpoint refuses unauthenticated
///   requests; `none` is not admissible (a credential-less bind fails
///   `CredentialUnavailable`, never a silent bare call).
/// - `cache_semantics: implicit_prefix` — OpenAI prompt caching is
///   automatic over strict prefix matches (≥1024-token eligibility,
///   128-token reporting granularity); `prompt_cache_key` is the
///   provider's affinity-key surface (64-char bound).
/// - `plan_schema` admits the hosted body's member names —
///   `max_completion_tokens` (the current spelling) beside `max_tokens`,
///   `reasoning_effort`, `prompt_cache_key`, `parallel_tool_calls` —
///   anything else is a plan-schema refusal, never a silent drop.
/// - hosted error codes ride `error_code_to_class` additions
///   (`rate_limit_exceeded`, `invalid_api_key`, `billing_*`).
pub fn openai() -> WireDialect {
    let mut rules = chat_completions_rules();
    rules.extend([
        // Hosted-only `error.code` spellings (the OpenAI error reference;
        // local-compatible endpoints never emit them).
        DialectRule::ErrorCodeToClass {
            code: "rate_limit_exceeded".to_string(),
            class: ModelErrorClass::RateLimited,
            debt: provider_debt_at(
                OPENAI_REF,
                "rate limiting is spelled `rate_limit_exceeded` in error.code",
                "some paths spell `rate_limit` — the shared family's rule covers that spelling too",
                "crates/hh-gateway/tests/r4_5d.rs::openai_error_code_class_maps",
            ),
        },
        DialectRule::ErrorCodeToClass {
            code: "invalid_api_key".to_string(),
            class: ModelErrorClass::Auth,
            debt: provider_debt_at(
                OPENAI_REF,
                "a rejected key is spelled `invalid_api_key` in error.code",
                "a 401 without the code falls back to the status table's `auth` — the class stands either way",
                "crates/hh-gateway/tests/r4_5d.rs::openai_error_code_class_maps",
            ),
        },
        DialectRule::ErrorCodeToClass {
            code: "billing_hard_limit_reached".to_string(),
            class: ModelErrorClass::CreditsExhausted,
            debt: provider_debt_at(
                OPENAI_REF,
                "billing-cap failures spell `billing_hard_limit_reached` in error.code",
                "a bare 402/429 answers via the status table instead",
                "crates/hh-gateway/tests/r4_5d.rs::openai_error_code_class_maps",
            ),
        },
    ]);
    WireDialect {
        dialect_id: "openai".to_string(),
        version: "1".to_string(),
        framing: Framing::Sse,
        plan_schema: PlanSchema {
            allowed_keys: [
                "temperature",
                "top_p",
                "max_tokens",
                "max_completion_tokens",
                "seed",
                "stream_options",
                "tool_choice",
                "parallel_tool_calls",
                "prompt_cache_key",
                "reasoning_effort",
                "presence_penalty",
                "frequency_penalty",
                "stop",
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
        auth_kinds: vec![AuthKind::Bearer],
        endpoint_allowlist_ref: "hh-embed/model_port.slot".to_string(),
        streaming: true,
        usage_arrival: UsageArrival::FinalOnly,
        usage_mapping_default: UsageMappingDefault::PreferProvider,
        usage_mapping: None,
        // OpenAI prompt caching — automatic strict prefix match; the
        // provider's eligibility floor is 1024 tokens and its usage
        // reports quantize `cached_tokens` to 128. `prompt_cache_key`
        // is the family's affinity-key surface.
        cache_semantics: CacheSemantics::ImplicitPrefix {
            strictness: ImplicitStrictness::Strict,
            affinity_key: AffinitySupport::Supported { max_key_length: 64 },
            retention_classes: vec![RetentionClass {
                class_id: "short".into(),
                nominal_ms: 300_000,
                guaranteed: false,
            }],
            min_cacheable_tokens: Some(1_024),
            reporting_granularity_tokens: Some(128),
        },
        cache_state_visible: true,
        count_tokens: None,
        deferred_requests: false,
        model_listing: Some("data".to_string()),
        snapshot_id_exposed: None,
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 120_000,
        prefix_affecting_params: [
            "model",
            "messages",
            "tools",
            "stream",
            "temperature",
            "seed",
            "prompt_cache_key",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rules,
    }
}

/// `openai_doc()` — the `openai@1` document body.
pub fn openai_doc() -> hh_wire::json::Json {
    openai().to_json()
}

/// `anthropic_messages()` — the `anthropic-messages@1` dialect document:
/// Anthropic's Messages API (`POST /v1/messages` + SSE event stream +
/// `GET /v1/models`). The wire-faithful facts:
///
/// - `framing: sse` — `event:`/`data:` pairs; the stream ends at
///   `message_stop` then connection close (no `[DONE]` sentinel).
/// - `usage_arrival: cumulative_stream` — `message_start` reports the
///   input counters (`input_tokens`, `cache_creation_input_tokens`,
///   `cache_read_input_tokens`); `message_delta` reports the cumulative
///   `output_tokens`.
/// - `auth_kinds: [x-api-key]` — `x-api-key` + `anthropic-version`
///   headers; unauthenticated binds fail `CredentialUnavailable`.
/// - `plan_schema.required_keys: ["max_tokens"]` — the Messages request
///   is malformed without it; the schema enforces presence upstream of
///   the wire (a typed refusal, never a provider 400).
/// - `cache_semantics: explicit_breakpoints` — `cache_control` markers on
///   content blocks (max 4 breakpoints, block-boundary placement, 5m/1h
///   TTL classes, the documented 1024-token eligibility floor,
///   organization-scoped isolation).
/// - `metadata_frame{ping}` — the SSE `ping` event is a declared
///   mid-stream frame (preserved as `provider.notice`, never a grammar
///   violation); `delta:<kind>` covers a content-block delta kind the
///   codec doesn't type (e.g. `citations_delta` — opaque, preserved).
/// - `stop_reason_map` — `end_turn`/`tool_use`/`max_tokens`/
///   `stop_sequence`/`pause_turn`/`refusal` spellings; `stop_sequence`'s
///   matched string rides `stop_details`.
pub fn anthropic_messages() -> WireDialect {
    let anthropic_debt = |what: &str, wrong: &str, removal_test: &str| {
        provider_debt_at(ANTHROPIC_REF, what, wrong, removal_test)
    };
    WireDialect {
        dialect_id: "anthropic-messages".to_string(),
        version: "1".to_string(),
        framing: Framing::Sse,
        plan_schema: PlanSchema {
            allowed_keys: [
                "max_tokens",
                "temperature",
                "top_p",
                "top_k",
                "stop_sequences",
                "tool_choice",
                "metadata",
                "service_tier",
                "thinking",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            required_keys: vec!["max_tokens".to_string()],
        },
        envelope_fields: ["model", "messages", "max_tokens", "stream"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        auth_kinds: vec![AuthKind::ApiKeyHeader],
        endpoint_allowlist_ref: "hh-embed/model_port.slot".to_string(),
        streaming: true,
        usage_arrival: UsageArrival::CumulativeStream,
        usage_mapping_default: UsageMappingDefault::PreferProvider,
        usage_mapping: None,
        cache_semantics: CacheSemantics::ExplicitBreakpoints {
            max_markers: 4,
            lookback_positions: None,
            position_rule: MarkerPositionRule::Block,
            // Anthropic's two TTL classes — `5m`/`1h` ride the closed
            // `class_id` vocabulary (`nominal_ms` carries the duration):
            // `short` is the 5-minute breakpoint, `long` the 1-hour.
            retention_classes: vec![
                RetentionClass {
                    class_id: "short".into(),
                    nominal_ms: 300_000,
                    guaranteed: false,
                },
                RetentionClass {
                    class_id: "long".into(),
                    nominal_ms: 3_600_000,
                    guaranteed: false,
                },
            ],
            min_cacheable_tokens: Some(1_024),
            isolation: IsolationScope::Organization,
            eligible_carriers: vec![BlockKind::Text, BlockKind::ToolCall],
        },
        cache_state_visible: true,
        count_tokens: Some("count_tokens".to_string()),
        deferred_requests: false,
        model_listing: Some("data".to_string()),
        snapshot_id_exposed: None,
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 120_000,
        prefix_affecting_params: [
            "model",
            "messages",
            "system",
            "tools",
            "tool_choice",
            "max_tokens",
            "temperature",
            "top_p",
            "top_k",
            "stop_sequences",
            "stream",
            "metadata",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rules: vec![
            // No `status_to_class` rows — the kernel status table already
            // covers the Messages error reference's statuses (400/401/403/
            // 404/413/429/5xx/529) and Anthropic's `error.type` is the
            // *finer* signal: `billing_error` narrows a 400 to
            // `credits_exhausted`, `not_found_error` distinguishes the
            // model case from a bare 404's `not_found{endpoint}`. A status
            // rule would shadow the code — declaring none is the
            // provider-faithful order (the code rules are first-class,
            // not a fallback the table occludes).
            // `error_code_to_class` — `error.type` spellings (the adapter
            // copies `type`→`code`; the raw payload is preserved).
            DialectRule::ErrorCodeToClass {
                code: "authentication_error".to_string(),
                class: ModelErrorClass::Auth,
                debt: anthropic_debt(
                    "a rejected key is spelled `authentication_error` in error.type",
                    "a 401 without the type falls back to the status table's `auth` — same class either way",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "permission_error".to_string(),
                class: ModelErrorClass::Forbidden,
                debt: anthropic_debt(
                    "an unauthorized scope is spelled `permission_error` in error.type",
                    "a 403 without the type falls back to the status table — same class",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "not_found_error".to_string(),
                class: ModelErrorClass::NotFound(NotFoundKind::Model),
                debt: anthropic_debt(
                    "a missing model is spelled `not_found_error` in error.type",
                    "the status table reads 404 as `not_found{endpoint}` — the code distinguishes the model case",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "rate_limit_error".to_string(),
                class: ModelErrorClass::RateLimited,
                debt: anthropic_debt(
                    "rate limiting is spelled `rate_limit_error` in error.type",
                    "a 429 without the type falls back to the status table — same class",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "overloaded_error".to_string(),
                class: ModelErrorClass::Overloaded,
                debt: anthropic_debt(
                    "overload is spelled `overloaded_error` in error.type (and in-stream `error` events)",
                    "a 529 without the type falls back to the status table — same class",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "api_error".to_string(),
                class: ModelErrorClass::ServerError,
                debt: anthropic_debt(
                    "a provider-internal failure is spelled `api_error` in error.type",
                    "a 5xx without the type falls back to the status table — same class",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "invalid_request_error".to_string(),
                class: ModelErrorClass::InvalidRequest,
                debt: anthropic_debt(
                    "a malformed request is spelled `invalid_request_error` in error.type",
                    "a 400 without the type falls back to the status table — same class",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "billing_error".to_string(),
                class: ModelErrorClass::CreditsExhausted,
                debt: anthropic_debt(
                    "a credit/balance failure is spelled `billing_error` in error.type",
                    "billing failures may arrive as `invalid_request_error` — that rule's class then stands",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "request_too_large".to_string(),
                class: ModelErrorClass::ContextLengthExceeded,
                debt: anthropic_debt(
                    "an over-window request is spelled `request_too_large` in error.type",
                    "the 413 status row classifies the same shape — the code is the fine-grained twin",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            DialectRule::ErrorCodeToClass {
                code: "timeout_error".to_string(),
                class: ModelErrorClass::Timeout(TimeoutKind::Attempt),
                debt: anthropic_debt(
                    "a provider-side deadline is spelled `timeout_error` in error.type",
                    "the shape may arrive as `api_error` — the server_error class then stands",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_error_code_class_maps",
                ),
            },
            // `stop_reason_map` — the Messages `stop_reason` spellings.
            DialectRule::StopReasonMap {
                provider: "end_turn".to_string(),
                reason: StopReason::EndTurn,
            },
            DialectRule::StopReasonMap {
                provider: "tool_use".to_string(),
                reason: StopReason::ToolUse,
            },
            DialectRule::StopReasonMap {
                provider: "max_tokens".to_string(),
                reason: StopReason::MaxOutput,
            },
            DialectRule::StopReasonMap {
                provider: "stop_sequence".to_string(),
                reason: StopReason::StopSequence,
            },
            DialectRule::StopReasonMap {
                provider: "pause_turn".to_string(),
                reason: StopReason::PauseTurn,
            },
            DialectRule::StopReasonMap {
                provider: "refusal".to_string(),
                reason: StopReason::Refusal,
            },
            // `metadata_frame` — Messages SSE kinds the codec keeps as
            // provider notices: the keepalive `ping` and untyped
            // content-block delta kinds (`citations_delta` et al. ride
            // `delta:<kind>`).
            DialectRule::MetadataFrame {
                kind: "ping".to_string(),
                debt: anthropic_debt(
                    "SSE `ping` events are in-stream keepalives, not model content",
                    "a future event kind reusing the `ping` spelling would still surface as a provider notice — never classified",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_ping_is_notice",
                ),
            },
            DialectRule::MetadataFrame {
                kind: "delta:citations_delta".to_string(),
                debt: anthropic_debt(
                    "citation deltas carry provider-opaque citation documents, not typed text",
                    "if a later profile types citations, the frame becomes a declared delta — the notice record disappears",
                    "crates/hh-gateway/tests/r4_5d.rs::anthropic_unknown_delta_is_notice",
                ),
            },
        ],
    }
}

/// `anthropic_messages_doc()` — the `anthropic-messages@1` document body.
pub fn anthropic_messages_doc() -> hh_wire::json::Json {
    anthropic_messages().to_json()
}
