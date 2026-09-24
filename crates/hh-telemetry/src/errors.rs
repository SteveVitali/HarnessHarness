//! The closed error sums for `hh-telemetry` — strict-codec failures, sink-policy
//! and export refusals, token-vector invariant violations, propagation parse
//! failures and the registry check's findings. No `Box<dyn Error>`: every failure
//! a caller can act on is a typed variant (CC3 — nothing silently lost).

use std::fmt;

/// Strict-codec failures — unknown members are `BadMember`, never silently
/// ignored (the crate's closed-record rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// A member the record's closed shape does not declare.
    BadMember {
        /// The offending member name.
        member: String,
        /// The record being decoded.
        record: &'static str,
    },
    /// A required member is absent.
    MissingMember {
        /// The missing member name.
        member: &'static str,
        /// The record being decoded.
        record: &'static str,
    },
    /// A member carried the wrong JSON type.
    TypeMismatch {
        /// The member name.
        member: String,
        /// The expected type spelling.
        expected: &'static str,
    },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::BadMember { member, record } => {
                write!(f, "unknown member {member} in {record}")
            }
            CodecError::MissingMember { member, record } => {
                write!(f, "missing member {member} in {record}")
            }
            CodecError::TypeMismatch { member, expected } => {
                write!(f, "member {member} is not a {expected}")
            }
        }
    }
}

impl std::error::Error for CodecError {}

/// The telemetry-plane error sum.
#[derive(Debug, Clone, PartialEq)]
pub enum TelemetryError {
    /// A strict-codec failure.
    Codec(CodecError),
    /// An unrecognized `scope_kind` spelling.
    UnknownScopeKind {
        /// The offending spelling.
        spelling: String,
    },
    /// An unrecognized content-class spelling.
    UnknownContentClass {
        /// The offending spelling.
        spelling: String,
    },
    /// An unrecognized `measured_at` spelling.
    UnknownMeasuredAt {
        /// The offending spelling.
        spelling: String,
    },
    /// An unrecognized sampling/redaction spelling.
    UnknownSinkMember {
        /// The member + spelling.
        detail: String,
    },
    /// A `TokenVector` invariant was violated (input_total ≠
    /// input_uncached + cache_read + cache_write, output_reasoning ∉
    /// output_total, a negative member, a foreign convention, a missing
    /// `normalizer_ref`).
    TokenInvariant {
        /// What was violated.
        detail: String,
    },
    /// A `{content}`-declaring sink must carry `requires_consent = true`
    /// (AC-R-2.9.1-8 — the schema-level half).
    ContentRequiresConsent {
        /// The sink.
        sink_id: String,
    },
    /// A consent-requiring sink whose manifest consent is absent — the export
    /// refuses (AC-R-2.9.1-8 — the runtime half).
    ConsentMissing {
        /// The sink.
        sink_id: String,
    },
    /// `sampling` below `all` is refused for `metric_view` exports — a
    /// scorecard may never be computed from a sample (ADR-0044 D3).
    SamplingForbidden {
        /// The sink.
        sink_id: String,
        /// The view kind the refusal fired on.
        view_kind: &'static str,
    },
    /// `cost_view`/`export` met two currencies — never summed across
    /// (ADR-0043 D4; AC-R-2.9.1-4's refusal).
    MixedCurrency {
        /// The distinct currencies seen.
        currencies: Vec<String>,
    },
    /// An inbound trace context was malformed — ignored and logged
    /// `structural`, never parented to an unrelated span (ADR-0042 D3).
    InvalidInboundContext {
        /// What failed to parse.
        detail: String,
    },
    /// A target cannot carry propagation (`PropagationUnsupported` — recorded
    /// in the lowering loss report, never silent).
    PropagationUnsupported {
        /// The target spelling.
        target: String,
    },
    /// A metric declaration's `requires_observability` does not cover the
    /// `min_observability` of a registered class it reads (DF-S1.5-2's check).
    RegistryViolation {
        /// What the check found.
        detail: String,
    },
    /// A delivery exceeds the sink's `rate_limit.events_per_sec` (Stage-1
    /// rule: one delivery spans a ≤1s window).
    RateLimited {
        /// The sink.
        sink_id: String,
        /// The batch's row count.
        rows: u64,
        /// The declared limit.
        limit: u64,
    },
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TelemetryError::Codec(e) => write!(f, "{e}"),
            TelemetryError::UnknownScopeKind { spelling } => {
                write!(f, "unknown scope_kind {spelling}")
            }
            TelemetryError::UnknownContentClass { spelling } => {
                write!(f, "unknown content class {spelling}")
            }
            TelemetryError::UnknownMeasuredAt { spelling } => {
                write!(f, "unknown measured_at {spelling}")
            }
            TelemetryError::UnknownSinkMember { detail } => {
                write!(f, "unknown sink member: {detail}")
            }
            TelemetryError::TokenInvariant { detail } => {
                write!(f, "TokenVector invariant: {detail}")
            }
            TelemetryError::ContentRequiresConsent { sink_id } => write!(
                f,
                "sink {sink_id} declares content but requires_consent is not true"
            ),
            TelemetryError::ConsentMissing { sink_id } => {
                write!(
                    f,
                    "sink {sink_id} requires consent the manifest does not grant"
                )
            }
            TelemetryError::SamplingForbidden { sink_id, view_kind } => write!(
                f,
                "sink {sink_id} samples below all; {view_kind} exports are never sampled"
            ),
            TelemetryError::MixedCurrency { currencies } => {
                write!(f, "mixed currencies never summed: {currencies:?}")
            }
            TelemetryError::InvalidInboundContext { detail } => {
                write!(f, "invalid inbound context: {detail}")
            }
            TelemetryError::PropagationUnsupported { target } => {
                write!(f, "propagation unsupported at {target}")
            }
            TelemetryError::RegistryViolation { detail } => {
                write!(f, "metric registry check: {detail}")
            }
            TelemetryError::RateLimited {
                sink_id,
                rows,
                limit,
            } => write!(f, "sink {sink_id}: {rows} rows exceed rate_limit {limit}/s"),
        }
    }
}

impl std::error::Error for TelemetryError {}

impl From<CodecError> for TelemetryError {
    fn from(e: CodecError) -> TelemetryError {
        TelemetryError::Codec(e)
    }
}
