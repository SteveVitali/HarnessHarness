//! The closed vocabularies the model boundary is built on (§5b.1 §3; ADR-0119
//! d.1/d.2/d.4): `StopReason`, `ModelErrorClass` with the kernel-fixed default
//! `retry_class` table (a dialect rule may only *narrow* transient → permanent,
//! never widen), `ErrorAttribution`, `ModelError`, `AttemptPolicy`, `BlockKind`,
//! `NoticeKind`, `UsageArrival`, `Purpose`.
//!
//! Every sum here is closed per `WireDialect` version (ADR-0015): growth is a
//! dialect bump, never an open string. Nothing in this module names a provider
//! or a model — it is kernel vocabulary (T-LCD-01).

use hh_wire::json::Json;

/// `StopReason` (closed — ADR-0119 d.1). `pause_turn` ("resubmit the same
/// call") is a control-strategy decision, never a gateway retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StopReason {
    /// The model ended its turn.
    EndTurn,
    /// The model wants a tool run.
    ToolUse,
    /// Output hit `sampling.max_output`.
    MaxOutput,
    /// A stop sequence matched.
    StopSequence,
    /// A content filter fired.
    ContentFilter,
    /// The model refused.
    Refusal,
    /// `pause_turn` — resubmit is a control decision, never a gateway retry.
    PauseTurn,
    /// The request was deferred (background).
    Deferred,
    /// The call was cancelled.
    Cancelled,
    /// The call ended in error.
    Error,
    /// Unmapped — the `raw_stop_reason` alias is preserved.
    Unknown,
}

impl StopReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::EndTurn => "end_turn",
            StopReason::ToolUse => "tool_use",
            StopReason::MaxOutput => "max_output",
            StopReason::StopSequence => "stop_sequence",
            StopReason::ContentFilter => "content_filter",
            StopReason::Refusal => "refusal",
            StopReason::PauseTurn => "pause_turn",
            StopReason::Deferred => "deferred",
            StopReason::Cancelled => "cancelled",
            StopReason::Error => "error",
            StopReason::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<StopReason> {
        Some(match s {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_output" => StopReason::MaxOutput,
            "stop_sequence" => StopReason::StopSequence,
            "content_filter" => StopReason::ContentFilter,
            "refusal" => StopReason::Refusal,
            "pause_turn" => StopReason::PauseTurn,
            "deferred" => StopReason::Deferred,
            "cancelled" => StopReason::Cancelled,
            "error" => StopReason::Error,
            "unknown" => StopReason::Unknown,
            _ => return None,
        })
    }

    /// Every member, in declaration order.
    pub const ALL: [StopReason; 11] = [
        StopReason::EndTurn,
        StopReason::ToolUse,
        StopReason::MaxOutput,
        StopReason::StopSequence,
        StopReason::ContentFilter,
        StopReason::Refusal,
        StopReason::PauseTurn,
        StopReason::Deferred,
        StopReason::Cancelled,
        StopReason::Error,
        StopReason::Unknown,
    ];
}

/// `StopDetails{categories[{category, level?}], explanation?}` — the structured
/// detail a `content_filter`/`refusal` terminal may carry. The explanation is
/// provider text — `authority = external`, never read by the kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct StopDetails {
    /// `categories[]` — the closed-form classifier rows the provider returned.
    pub categories: Vec<StopCategory>,
    /// The provider's explanation string (external text; the gateway moves it,
    /// never interprets it).
    pub explanation: Option<String>,
}

/// One `categories[]` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopCategory {
    /// The category spelling (opaque to the gateway — moved as data).
    pub category: String,
    /// An optional level.
    pub level: Option<String>,
}

/// `RetryClass ∈ {transient, rate_limit, permanent, ambiguous}` — the gateway's
/// only retry input (ADR-0119 d.2/d.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetryClass {
    /// Transient — retryable under `AttemptPolicy`.
    Transient,
    /// Rate-limited — retryable under `AttemptPolicy`, honour `retry_after`.
    RateLimit,
    /// Permanent — never retried by the gateway.
    Permanent,
    /// Ambiguous (`unknown`, `invalid_response`) — never retried.
    Ambiguous,
}

impl RetryClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RetryClass::Transient => "transient",
            RetryClass::RateLimit => "rate_limit",
            RetryClass::Permanent => "permanent",
            RetryClass::Ambiguous => "ambiguous",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RetryClass> {
        Some(match s {
            "transient" => RetryClass::Transient,
            "rate_limit" => RetryClass::RateLimit,
            "permanent" => RetryClass::Permanent,
            "ambiguous" => RetryClass::Ambiguous,
            _ => return None,
        })
    }
}

/// `timeout{connect | attempt | stream_idle}` — the timeout sub-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeoutKind {
    /// The connect timed out.
    Connect,
    /// The attempt timed out.
    Attempt,
    /// The stream went idle beyond `stream_idle_timeout_ms`.
    StreamIdle,
}

impl TimeoutKind {
    /// The canonical sub-spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TimeoutKind::Connect => "connect",
            TimeoutKind::Attempt => "attempt",
            TimeoutKind::StreamIdle => "stream_idle",
        }
    }

    /// Parse a sub-spelling.
    pub fn parse(s: &str) -> Option<TimeoutKind> {
        Some(match s {
            "connect" => TimeoutKind::Connect,
            "attempt" => TimeoutKind::Attempt,
            "stream_idle" => TimeoutKind::StreamIdle,
            _ => return None,
        })
    }
}

/// `not_found{endpoint | model}` — the not-found sub-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotFoundKind {
    /// The endpoint does not exist.
    Endpoint,
    /// The model does not exist.
    Model,
}

impl NotFoundKind {
    /// The canonical sub-spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            NotFoundKind::Endpoint => "endpoint",
            NotFoundKind::Model => "model",
        }
    }

    /// Parse a sub-spelling.
    pub fn parse(s: &str) -> Option<NotFoundKind> {
        Some(match s {
            "endpoint" => NotFoundKind::Endpoint,
            "model" => NotFoundKind::Model,
            _ => return None,
        })
    }
}

/// `invalid_response{no_stop_reason | thinking_only | malformed_tool_call |
/// unexpected_tool_call}` — the response-validity sub-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvalidResponseKind {
    /// No stop reason was delivered.
    NoStopReason,
    /// The response carried reasoning and nothing else.
    ThinkingOnly,
    /// A tool call was malformed beyond `Unparseable`.
    MalformedToolCall,
    /// A tool call arrived that no tool declared.
    UnexpectedToolCall,
}

impl InvalidResponseKind {
    /// The canonical sub-spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            InvalidResponseKind::NoStopReason => "no_stop_reason",
            InvalidResponseKind::ThinkingOnly => "thinking_only",
            InvalidResponseKind::MalformedToolCall => "malformed_tool_call",
            InvalidResponseKind::UnexpectedToolCall => "unexpected_tool_call",
        }
    }

    /// Parse a sub-spelling.
    pub fn parse(s: &str) -> Option<InvalidResponseKind> {
        Some(match s {
            "no_stop_reason" => InvalidResponseKind::NoStopReason,
            "thinking_only" => InvalidResponseKind::ThinkingOnly,
            "malformed_tool_call" => InvalidResponseKind::MalformedToolCall,
            "unexpected_tool_call" => InvalidResponseKind::UnexpectedToolCall,
            _ => return None,
        })
    }
}

/// `ModelErrorClass` — the closed error vocabulary (ADR-0119 d.2). Families:
/// transport (`network`, `timeout{…}`, `stream_decode`, `server_error`,
/// `overloaded`, `rate_limited`); request (`auth`, `forbidden`,
/// `not_found{…}`, `invalid_request`, `context_length_exceeded`,
/// `malformed_history`, `transcript_incompatible`, `unsupported_feature`,
/// `permanent_request`); account (`quota_exhausted`, `credits_exhausted`,
/// `usage_not_included`, `verification_required`); policy (`content_policy`,
/// `safety_blocked`, `refusal`); response (`empty_response`,
/// `invalid_response{…}`, `served_model_mismatch`); control (`cancelled`);
/// `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelErrorClass {
    // ── transport ──
    /// A network failure.
    Network,
    /// A timeout (`connect | attempt | stream_idle`).
    Timeout(TimeoutKind),
    /// The stream could not be decoded.
    StreamDecode,
    /// A 5xx server error.
    ServerError,
    /// The provider is overloaded.
    Overloaded,
    /// Rate-limited.
    RateLimited,
    // ── request ──
    /// Authentication failed.
    Auth,
    /// The account may not make this request.
    Forbidden,
    /// `not_found{endpoint | model}`.
    NotFound(NotFoundKind),
    /// The request was invalid.
    InvalidRequest,
    /// The transcript exceeds the context window.
    ContextLengthExceeded,
    /// The history is malformed for this provider.
    MalformedHistory,
    /// The transcript cannot be replayed (a signed-block/opaque-item
    /// rejection — reached only through a debt-recorded
    /// `error_text_to_class` rule).
    TranscriptIncompatible,
    /// A requested feature is unsupported.
    UnsupportedFeature,
    /// A permanent request error.
    PermanentRequest,
    // ── account ──
    /// Quota exhausted.
    QuotaExhausted,
    /// Credits exhausted.
    CreditsExhausted,
    /// Usage not included in the plan.
    UsageNotIncluded,
    /// Account verification required.
    VerificationRequired,
    // ── policy ──
    /// A content policy fired.
    ContentPolicy,
    /// A safety block fired.
    SafetyBlocked,
    /// A refusal.
    Refusal,
    // ── response ──
    /// The response carried no content.
    EmptyResponse,
    /// `invalid_response{…}` — content-invalid.
    InvalidResponse(InvalidResponseKind),
    /// The served model differs from the requested one and the binding
    /// declared `substitution_allowed = false`.
    ServedModelMismatch,
    // ── control ──
    /// Cancelled.
    Cancelled,
    /// Unclassifiable — `raw_*` is preserved; never retried.
    Unknown,
}

impl ModelErrorClass {
    /// The canonical spelling — the parameterised kinds spell
    /// `kind{sub}` (the spec's own notation).
    pub fn as_str(self) -> &'static str {
        match self {
            ModelErrorClass::Network => "network",
            ModelErrorClass::Timeout(TimeoutKind::Connect) => "timeout{connect}",
            ModelErrorClass::Timeout(TimeoutKind::Attempt) => "timeout{attempt}",
            ModelErrorClass::Timeout(TimeoutKind::StreamIdle) => "timeout{stream_idle}",
            ModelErrorClass::StreamDecode => "stream_decode",
            ModelErrorClass::ServerError => "server_error",
            ModelErrorClass::Overloaded => "overloaded",
            ModelErrorClass::RateLimited => "rate_limited",
            ModelErrorClass::Auth => "auth",
            ModelErrorClass::Forbidden => "forbidden",
            ModelErrorClass::NotFound(NotFoundKind::Endpoint) => "not_found{endpoint}",
            ModelErrorClass::NotFound(NotFoundKind::Model) => "not_found{model}",
            ModelErrorClass::InvalidRequest => "invalid_request",
            ModelErrorClass::ContextLengthExceeded => "context_length_exceeded",
            ModelErrorClass::MalformedHistory => "malformed_history",
            ModelErrorClass::TranscriptIncompatible => "transcript_incompatible",
            ModelErrorClass::UnsupportedFeature => "unsupported_feature",
            ModelErrorClass::PermanentRequest => "permanent_request",
            ModelErrorClass::QuotaExhausted => "quota_exhausted",
            ModelErrorClass::CreditsExhausted => "credits_exhausted",
            ModelErrorClass::UsageNotIncluded => "usage_not_included",
            ModelErrorClass::VerificationRequired => "verification_required",
            ModelErrorClass::ContentPolicy => "content_policy",
            ModelErrorClass::SafetyBlocked => "safety_blocked",
            ModelErrorClass::Refusal => "refusal",
            ModelErrorClass::EmptyResponse => "empty_response",
            ModelErrorClass::InvalidResponse(InvalidResponseKind::NoStopReason) => {
                "invalid_response{no_stop_reason}"
            }
            ModelErrorClass::InvalidResponse(InvalidResponseKind::ThinkingOnly) => {
                "invalid_response{thinking_only}"
            }
            ModelErrorClass::InvalidResponse(InvalidResponseKind::MalformedToolCall) => {
                "invalid_response{malformed_tool_call}"
            }
            ModelErrorClass::InvalidResponse(InvalidResponseKind::UnexpectedToolCall) => {
                "invalid_response{unexpected_tool_call}"
            }
            ModelErrorClass::ServedModelMismatch => "served_model_mismatch",
            ModelErrorClass::Cancelled => "cancelled",
            ModelErrorClass::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ModelErrorClass> {
        Some(match s {
            "network" => ModelErrorClass::Network,
            "timeout{connect}" => ModelErrorClass::Timeout(TimeoutKind::Connect),
            "timeout{attempt}" => ModelErrorClass::Timeout(TimeoutKind::Attempt),
            "timeout{stream_idle}" => ModelErrorClass::Timeout(TimeoutKind::StreamIdle),
            "stream_decode" => ModelErrorClass::StreamDecode,
            "server_error" => ModelErrorClass::ServerError,
            "overloaded" => ModelErrorClass::Overloaded,
            "rate_limited" => ModelErrorClass::RateLimited,
            "auth" => ModelErrorClass::Auth,
            "forbidden" => ModelErrorClass::Forbidden,
            "not_found{endpoint}" => ModelErrorClass::NotFound(NotFoundKind::Endpoint),
            "not_found{model}" => ModelErrorClass::NotFound(NotFoundKind::Model),
            "invalid_request" => ModelErrorClass::InvalidRequest,
            "context_length_exceeded" => ModelErrorClass::ContextLengthExceeded,
            "malformed_history" => ModelErrorClass::MalformedHistory,
            "transcript_incompatible" => ModelErrorClass::TranscriptIncompatible,
            "unsupported_feature" => ModelErrorClass::UnsupportedFeature,
            "permanent_request" => ModelErrorClass::PermanentRequest,
            "quota_exhausted" => ModelErrorClass::QuotaExhausted,
            "credits_exhausted" => ModelErrorClass::CreditsExhausted,
            "usage_not_included" => ModelErrorClass::UsageNotIncluded,
            "verification_required" => ModelErrorClass::VerificationRequired,
            "content_policy" => ModelErrorClass::ContentPolicy,
            "safety_blocked" => ModelErrorClass::SafetyBlocked,
            "refusal" => ModelErrorClass::Refusal,
            "empty_response" => ModelErrorClass::EmptyResponse,
            "invalid_response{no_stop_reason}" => {
                ModelErrorClass::InvalidResponse(InvalidResponseKind::NoStopReason)
            }
            "invalid_response{thinking_only}" => {
                ModelErrorClass::InvalidResponse(InvalidResponseKind::ThinkingOnly)
            }
            "invalid_response{malformed_tool_call}" => {
                ModelErrorClass::InvalidResponse(InvalidResponseKind::MalformedToolCall)
            }
            "invalid_response{unexpected_tool_call}" => {
                ModelErrorClass::InvalidResponse(InvalidResponseKind::UnexpectedToolCall)
            }
            "served_model_mismatch" => ModelErrorClass::ServedModelMismatch,
            "cancelled" => ModelErrorClass::Cancelled,
            "unknown" => ModelErrorClass::Unknown,
            _ => return None,
        })
    }

    /// The kernel-fixed default `retry_class` (ADR-0119 d.2): transient =
    /// `network, timeout, stream_decode, server_error, overloaded,
    /// empty_response`; rate_limit = `rate_limited`; permanent = all
    /// request/account/policy classes + `served_model_mismatch` +
    /// `cancelled`; ambiguous = `unknown, invalid_response`. A dialect rule may
    /// only **narrow** transient → permanent, never widen.
    pub fn default_retry_class(self) -> RetryClass {
        match self {
            ModelErrorClass::Network
            | ModelErrorClass::Timeout(_)
            | ModelErrorClass::StreamDecode
            | ModelErrorClass::ServerError
            | ModelErrorClass::Overloaded
            | ModelErrorClass::EmptyResponse => RetryClass::Transient,
            ModelErrorClass::RateLimited => RetryClass::RateLimit,
            ModelErrorClass::Auth
            | ModelErrorClass::Forbidden
            | ModelErrorClass::NotFound(_)
            | ModelErrorClass::InvalidRequest
            | ModelErrorClass::ContextLengthExceeded
            | ModelErrorClass::MalformedHistory
            | ModelErrorClass::TranscriptIncompatible
            | ModelErrorClass::UnsupportedFeature
            | ModelErrorClass::PermanentRequest
            | ModelErrorClass::QuotaExhausted
            | ModelErrorClass::CreditsExhausted
            | ModelErrorClass::UsageNotIncluded
            | ModelErrorClass::VerificationRequired
            | ModelErrorClass::ContentPolicy
            | ModelErrorClass::SafetyBlocked
            | ModelErrorClass::Refusal
            | ModelErrorClass::ServedModelMismatch
            | ModelErrorClass::Cancelled => RetryClass::Permanent,
            ModelErrorClass::Unknown | ModelErrorClass::InvalidResponse(_) => RetryClass::Ambiguous,
        }
    }

    /// The `attribution` family the class belongs to (§5b.1 §3 —
    /// `{provider, transport, request, account, policy, response}`).
    pub fn attribution(self) -> ErrorAttribution {
        match self {
            ModelErrorClass::Network
            | ModelErrorClass::Timeout(_)
            | ModelErrorClass::StreamDecode
            | ModelErrorClass::ServerError
            | ModelErrorClass::Overloaded
            | ModelErrorClass::RateLimited => ErrorAttribution::Transport,
            ModelErrorClass::Auth
            | ModelErrorClass::Forbidden
            | ModelErrorClass::NotFound(_)
            | ModelErrorClass::InvalidRequest
            | ModelErrorClass::ContextLengthExceeded
            | ModelErrorClass::MalformedHistory
            | ModelErrorClass::TranscriptIncompatible
            | ModelErrorClass::UnsupportedFeature
            | ModelErrorClass::PermanentRequest => ErrorAttribution::Request,
            ModelErrorClass::QuotaExhausted
            | ModelErrorClass::CreditsExhausted
            | ModelErrorClass::UsageNotIncluded
            | ModelErrorClass::VerificationRequired => ErrorAttribution::Account,
            ModelErrorClass::ContentPolicy
            | ModelErrorClass::SafetyBlocked
            | ModelErrorClass::Refusal => ErrorAttribution::Policy,
            ModelErrorClass::EmptyResponse
            | ModelErrorClass::InvalidResponse(_)
            | ModelErrorClass::ServedModelMismatch => ErrorAttribution::Response,
            ModelErrorClass::Cancelled | ModelErrorClass::Unknown => ErrorAttribution::Provider,
        }
    }

    /// The family tag (the six providers the families group under).
    pub fn family(self) -> &'static str {
        match self {
            ModelErrorClass::Network
            | ModelErrorClass::Timeout(_)
            | ModelErrorClass::StreamDecode
            | ModelErrorClass::ServerError
            | ModelErrorClass::Overloaded
            | ModelErrorClass::RateLimited => "transport",
            ModelErrorClass::Auth
            | ModelErrorClass::Forbidden
            | ModelErrorClass::NotFound(_)
            | ModelErrorClass::InvalidRequest
            | ModelErrorClass::ContextLengthExceeded
            | ModelErrorClass::MalformedHistory
            | ModelErrorClass::TranscriptIncompatible
            | ModelErrorClass::UnsupportedFeature
            | ModelErrorClass::PermanentRequest => "request",
            ModelErrorClass::QuotaExhausted
            | ModelErrorClass::CreditsExhausted
            | ModelErrorClass::UsageNotIncluded
            | ModelErrorClass::VerificationRequired => "account",
            ModelErrorClass::ContentPolicy
            | ModelErrorClass::SafetyBlocked
            | ModelErrorClass::Refusal => "policy",
            ModelErrorClass::EmptyResponse
            | ModelErrorClass::InvalidResponse(_)
            | ModelErrorClass::ServedModelMismatch => "response",
            ModelErrorClass::Cancelled => "control",
            ModelErrorClass::Unknown => "unknown",
        }
    }
}

/// `attribution ∈ {provider, transport, request, account, policy, response}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorAttribution {
    /// The provider produced the failure.
    Provider,
    /// The transport failed.
    Transport,
    /// The request was invalid.
    Request,
    /// The account state failed.
    Account,
    /// A policy fired.
    Policy,
    /// The response was invalid.
    Response,
}

impl ErrorAttribution {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorAttribution::Provider => "provider",
            ErrorAttribution::Transport => "transport",
            ErrorAttribution::Request => "request",
            ErrorAttribution::Account => "account",
            ErrorAttribution::Policy => "policy",
            ErrorAttribution::Response => "response",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ErrorAttribution> {
        Some(match s {
            "provider" => ErrorAttribution::Provider,
            "transport" => ErrorAttribution::Transport,
            "request" => ErrorAttribution::Request,
            "account" => ErrorAttribution::Account,
            "policy" => ErrorAttribution::Policy,
            "response" => ErrorAttribution::Response,
            _ => return None,
        })
    }
}

/// `ModelError{class, retry_class, attribution, retry_after_ms?, http_status?,
/// provider_code? (surface), message: Text(external, redacted), raw_ref?}`
/// (ADR-0119 d.2) — classification happens once, at the boundary (T-LCD-10).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelError {
    /// The closed class.
    pub class: ModelErrorClass,
    /// The retry class — `class.default_retry_class()` unless a dialect rule
    /// narrowed transient → permanent.
    pub retry_class: RetryClass,
    /// The attribution family.
    pub attribution: ErrorAttribution,
    /// A provider `retry-after` hint (ms).
    pub retry_after_ms: Option<u64>,
    /// The HTTP status, when the failure carried one.
    pub http_status: Option<u32>,
    /// The provider's error code — a surface alias, never a join key.
    pub provider_code: Option<String>,
    /// The message — `Text(external, redacted)`: the provider's explanation,
    /// never interpreted by the kernel, redacted before any ledger write.
    pub message: String,
    /// A content-addressed reference to the raw error frame, when retained.
    pub raw_ref: Option<String>,
}

impl ModelError {
    /// Construct with the kernel-default retry class and attribution.
    pub fn new(class: ModelErrorClass, message: impl Into<String>) -> ModelError {
        ModelError {
            class,
            retry_class: class.default_retry_class(),
            attribution: class.attribution(),
            retry_after_ms: None,
            http_status: None,
            provider_code: None,
            message: message.into(),
            raw_ref: None,
        }
    }

    /// Narrow `transient → permanent` (the only direction a dialect rule may
    /// take — ADR-0119 d.2). A widening request is refused (`None` returned;
    /// the caller keeps the default).
    pub fn narrowed(&self, to: RetryClass) -> Option<ModelError> {
        match (self.retry_class, to) {
            (RetryClass::Transient, RetryClass::Permanent) => {
                let mut e = self.clone();
                e.retry_class = RetryClass::Permanent;
                Some(e)
            }
            // Widening or a no-op is never a rule's business — the kernel
            // default stands.
            _ => None,
        }
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("class".into(), Json::str(self.class.as_str()));
        m.insert("retry_class".into(), Json::str(self.retry_class.as_str()));
        m.insert("attribution".into(), Json::str(self.attribution.as_str()));
        if let Some(ms) = self.retry_after_ms {
            m.insert("retry_after_ms".into(), Json::Int(ms as i64));
        }
        if let Some(s) = self.http_status {
            m.insert("http_status".into(), Json::Int(s as i64));
        }
        if let Some(c) = &self.provider_code {
            m.insert("provider_code".into(), Json::str(c.clone()));
        }
        m.insert("message".into(), Json::str(self.message.clone()));
        if let Some(r) = &self.raw_ref {
            m.insert("raw_ref".into(), Json::str(r.clone()));
        }
        Json::Obj(m)
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ModelError{{class: {}, retry_class: {}}}: {}",
            self.class.as_str(),
            self.retry_class.as_str(),
            self.message
        )
    }
}
impl std::error::Error for ModelError {}

/// `AttemptPolicy` — data, supplied per call by the control envelope; the C0
/// kernel default is [`DEFAULT_ATTEMPT_POLICY`] (ADR-0119 d.4).
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptPolicy {
    /// `max_attempts` — the logical call's attempt bound.
    pub max_attempts: u32,
    /// `base_delay_ms`.
    pub base_delay_ms: u64,
    /// `max_delay_ms`.
    pub max_delay_ms: u64,
    /// `jitter ∈ [0.8, 1.2]` expressed in integer ppm (`800_000…1_200_000`) —
    /// canonical JSON is integer-only.
    pub jitter_min_ppm: u64,
    /// The upper jitter bound (ppm).
    pub jitter_max_ppm: u64,
    /// `honour_retry_after`.
    pub honour_retry_after: bool,
    /// `retry_after_cap_ms` — a server-supplied delay above this is reported
    /// (`will_retry = false`) and fails `rate_limited` (R-RT-4).
    pub retry_after_cap_ms: u64,
    /// `retry_on ⊆ {transient, rate_limit}` (R-RT-1).
    pub retry_on: Vec<RetryClass>,
    /// `stream_idle_timeout_ms`.
    pub stream_idle_timeout_ms: u64,
    /// `attempt_timeout_ms`.
    pub attempt_timeout_ms: u64,
    /// `connect_timeout_ms`.
    pub connect_timeout_ms: u64,
}

/// The C0 kernel default `AttemptPolicy` (ADR-0119 d.4 — a parameter/entity
/// rung value, data not code).
pub const DEFAULT_ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy {
    max_attempts: 3,
    base_delay_ms: 250,
    max_delay_ms: 8_000,
    jitter_min_ppm: 800_000,
    jitter_max_ppm: 1_200_000,
    honour_retry_after: true,
    retry_after_cap_ms: 60_000,
    retry_on: vec![],
    stream_idle_timeout_ms: 30_000,
    attempt_timeout_ms: 120_000,
    connect_timeout_ms: 10_000,
};

impl AttemptPolicy {
    /// `retry_on ⊆ {transient, rate_limit}` — filled on load so the const
    /// default can stay a `vec![]` literal; callers use [`AttemptPolicy::c0`].
    pub fn c0() -> AttemptPolicy {
        let mut p = DEFAULT_ATTEMPT_POLICY;
        p.retry_on = vec![RetryClass::Transient, RetryClass::RateLimit];
        p
    }

    /// Whether `class` may be retried under this policy (R-RT-1: only
    /// `retry_class ∈ retry_on ⊆ {transient, rate_limit}`; never
    /// `permanent | ambiguous`).
    pub fn admits_retry(&self, retry_class: RetryClass) -> bool {
        matches!(retry_class, RetryClass::Transient | RetryClass::RateLimit)
            && self.retry_on.contains(&retry_class)
    }

    /// The backoff delay for `attempt_no` (1-based) before any server hint:
    /// `min(max_delay_ms, base_delay_ms * 2^(attempt_no-1))` jittered by a
    /// deterministic factor in `[jitter_min_ppm, jitter_max_ppm]` derived from
    /// `(model_call_id, attempt_no)` — deterministic replay, no RNG.
    pub fn backoff_ms(&self, model_call_id: &str, attempt_no: u32) -> u64 {
        let shift = attempt_no.saturating_sub(1).min(20);
        let base = self
            .base_delay_ms
            .saturating_mul(1u64 << shift)
            .min(self.max_delay_ms);
        let span = self.jitter_max_ppm.saturating_sub(self.jitter_min_ppm);
        let material = format!("{model_call_id}\u{1f}{attempt_no}");
        let digest = hh_wire::sha256::sha256_hex(material.as_bytes());
        let raw = u64::from_str_radix(&digest[..16], 16).unwrap_or(0);
        let ppm = self.jitter_min_ppm + (raw % (span + 1));
        base.saturating_mul(ppm) / 1_000_000
    }
}

/// `BlockKind ∈ {text, reasoning, redacted_reasoning, tool_call,
/// provider_opaque}` — the `block.started`/`block.completed` kind set
/// (ADR-0118 d.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockKind {
    /// Visible text.
    Text,
    /// Reasoning/thinking.
    Reasoning,
    /// Redacted reasoning (opaque payload).
    RedactedReasoning,
    /// A tool call.
    ToolCall,
    /// A provider-opaque island.
    ProviderOpaque,
}

impl BlockKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BlockKind::Text => "text",
            BlockKind::Reasoning => "reasoning",
            BlockKind::RedactedReasoning => "redacted_reasoning",
            BlockKind::ToolCall => "tool_call",
            BlockKind::ProviderOpaque => "provider_opaque",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<BlockKind> {
        Some(match s {
            "text" => BlockKind::Text,
            "reasoning" => BlockKind::Reasoning,
            "redacted_reasoning" => BlockKind::RedactedReasoning,
            "tool_call" => BlockKind::ToolCall,
            "provider_opaque" => BlockKind::ProviderOpaque,
            _ => return None,
        })
    }
}

/// `provider.notice` kinds (ADR-0118 d.4 — the closed set; unknown notice kinds
/// are preserved byte-for-byte, G6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoticeKind {
    /// A rate-limit snapshot.
    RateLimitSnapshot,
    /// The served model id (a substitution observation).
    ServedModel,
    /// A fallback boundary marker.
    FallbackBoundary,
    /// Safety buffering.
    SafetyBuffering,
    /// Reasoning included notice.
    ReasoningIncluded,
    /// Verification required notice.
    VerificationRequired,
    /// A models etag.
    ModelsEtag,
    /// Any other provider notice (preserved verbatim).
    Other,
}

impl NoticeKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            NoticeKind::RateLimitSnapshot => "rate_limit_snapshot",
            NoticeKind::ServedModel => "served_model",
            NoticeKind::FallbackBoundary => "fallback_boundary",
            NoticeKind::SafetyBuffering => "safety_buffering",
            NoticeKind::ReasoningIncluded => "reasoning_included",
            NoticeKind::VerificationRequired => "verification_required",
            NoticeKind::ModelsEtag => "models_etag",
            NoticeKind::Other => "other",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<NoticeKind> {
        Some(match s {
            "rate_limit_snapshot" => NoticeKind::RateLimitSnapshot,
            "served_model" => NoticeKind::ServedModel,
            "fallback_boundary" => NoticeKind::FallbackBoundary,
            "safety_buffering" => NoticeKind::SafetyBuffering,
            "reasoning_included" => NoticeKind::ReasoningIncluded,
            "verification_required" => NoticeKind::VerificationRequired,
            "models_etag" => NoticeKind::ModelsEtag,
            "other" => NoticeKind::Other,
            _ => return None,
        })
    }
}

/// `usage_arrival ∈ {final_only, cumulative_stream, delta_stream, absent}`
/// (ADR-0119 d.3 — how the dialect delivers usage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageArrival {
    /// Usage only on the terminal frame.
    FinalOnly,
    /// Cumulative usage frames in-stream.
    CumulativeStream,
    /// Delta usage frames in-stream.
    DeltaStream,
    /// No usage is delivered.
    Absent,
}

impl UsageArrival {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UsageArrival::FinalOnly => "final_only",
            UsageArrival::CumulativeStream => "cumulative_stream",
            UsageArrival::DeltaStream => "delta_stream",
            UsageArrival::Absent => "absent",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<UsageArrival> {
        Some(match s {
            "final_only" => UsageArrival::FinalOnly,
            "cumulative_stream" => UsageArrival::CumulativeStream,
            "delta_stream" => UsageArrival::DeltaStream,
            "absent" => UsageArrival::Absent,
            _ => return None,
        })
    }
}

/// `model.rerouted.reason ∈ {transient_exhausted, permanent{class}, budget,
/// cost, latency, capability_unmet, operator, policy}` (§5b.2; ADR-0122 d.3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RerouteReason {
    /// The transient retries were exhausted.
    TransientExhausted,
    /// A `permanent{class}` failure.
    Permanent(ModelErrorClass),
    /// Budget-driven.
    Budget,
    /// Cost-driven.
    Cost,
    /// Latency-driven.
    Latency,
    /// A capability the call needs is unmet.
    CapabilityUnmet,
    /// An operator ordered the reroute.
    Operator,
    /// The routing policy ordered it (e.g. `successor_ref` after
    /// `retirement_at` — ADR-0121 G-3).
    Policy,
}

impl RerouteReason {
    /// The canonical spelling (`permanent{class}` parameterised).
    pub fn as_str(&self) -> String {
        match self {
            RerouteReason::TransientExhausted => "transient_exhausted".to_string(),
            RerouteReason::Permanent(c) => format!("permanent{{{}}}", c.as_str()),
            RerouteReason::Budget => "budget".to_string(),
            RerouteReason::Cost => "cost".to_string(),
            RerouteReason::Latency => "latency".to_string(),
            RerouteReason::CapabilityUnmet => "capability_unmet".to_string(),
            RerouteReason::Operator => "operator".to_string(),
            RerouteReason::Policy => "policy".to_string(),
        }
    }
}

/// `substitution ∈ {none, provider_fallback, safety_routing, unknown}` — the
/// served-model substitution kind on a terminal payload (ADR-0120 d.4; spend
/// is priced at the served model regardless).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Substitution {
    /// No substitution — `served_model == model_ref.provider_model_id`.
    None,
    /// The provider fell back to another model.
    ProviderFallback,
    /// A safety router redirected the call.
    SafetyRouting,
    /// A `served_model` was observed with no declared reason.
    Unknown,
}

impl Substitution {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Substitution::None => "none",
            Substitution::ProviderFallback => "provider_fallback",
            Substitution::SafetyRouting => "safety_routing",
            Substitution::Unknown => "unknown",
        }
    }
}

/// `purpose ∈ {main, compaction, probe, judge, subagent(id)}` — the affinity-key
/// and prefix-extension scope (ADR-0128 d.1; LC-6).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Purpose {
    /// The main call.
    Main,
    /// A compaction summary call.
    Compaction,
    /// A probe call.
    Probe,
    /// A judge call.
    Judge,
    /// A subagent root (`purpose = subagent(id)` — F3 gives each child root
    /// its own id).
    Subagent(String),
}

impl Purpose {
    /// The canonical spelling (`subagent(id)` parameterised).
    pub fn as_str(&self) -> String {
        match self {
            Purpose::Main => "main".to_string(),
            Purpose::Compaction => "compaction".to_string(),
            Purpose::Probe => "probe".to_string(),
            Purpose::Judge => "judge".to_string(),
            Purpose::Subagent(id) => format!("subagent({id})"),
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Purpose> {
        match s {
            "main" => Some(Purpose::Main),
            "compaction" => Some(Purpose::Compaction),
            "probe" => Some(Purpose::Probe),
            "judge" => Some(Purpose::Judge),
            _ => {
                let inner = s.strip_prefix("subagent(")?.strip_suffix(')')?;
                Some(Purpose::Subagent(inner.to_string()))
            }
        }
    }
}
