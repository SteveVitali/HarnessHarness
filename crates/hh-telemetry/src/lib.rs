//! `hh-telemetry` — the Stage-1 telemetry, tracing and cost/latency
//! instrumentation (spec §5h.1; R-2.9.1; ticket S1.14; ADR-0042/0043/0044).
//!
//! The architectural invariant: **a trace is a deterministic projection over
//! the run ledger — never a second store** (ADR-0042 D1/D2). Everything this
//! crate produces is either (a) a [`View`] folded from the durable prefix
//! (`trace_view`, `cost_view`, `metric_view`), (b) a payload a named component
//! appends through the canonical `Store::append` path
//! (`measurement.export.delivered`, `measurement.metric.emitted`), or (c) a
//! declaration (`SinkPolicy`, the process-metric catalogue, the closed
//! measurement-point table). There is no telemetry database.
//!
//! - [`scope`] — the closed scope-kind taxonomy (M1–M12 wired at Stage 1;
//!   `component_call`/M19 is the Stage-3 dialect bump) and the
//!   `MEASUREMENT_POINTS` table (§2.5 verbatim).
//! - [`tokens`] — `TokenVector` normalization (`hh-inclusive/1`) with
//!   mandatory `normalizer_ref`.
//! - [`clocks`] — `MeasuredAt` (which component's monotonic clock),
//!   `clock_tolerance_ms`, `clock_skew_flag`, canonical `ts` arithmetic.
//! - [`sinks`] — `SinkPolicy` and the L0–L3 `ContentClass` sum.
//! - [`propagation`] — W3C `traceparent`/`tracestate` at the subprocess seam;
//!   lowering loss is explicit (`PropagationUnsupported`), never silent.
//! - [`events`] — the measurement-emission payload builders + strict decoders.
//! - [`views`] — `trace_view`/`cost_view`/`metric_view`/`sink_deliveries`.
//! - [`catalogue`] — the process-metric declarations + the
//!   `requires_observability` registry check (DF-S1.5-2).
//! - [`export`] — the sink lowering (`content_classes`, redaction, sampling,
//!   `max_field_bytes`, consent) and the `measurement.export.delivered`
//!   append.

mod codec;

pub mod catalogue;
pub mod clocks;
pub mod errors;
pub mod events;
pub mod export;
pub mod propagation;
pub mod scope;
pub mod sinks;
pub mod tokens;
pub mod views;

pub use clocks::{
    clock_tolerance, skewed, ts_ms, ts_span_ms, MeasuredAt, DEFAULT_CLOCK_TOLERANCE_MS,
};
pub use errors::{CodecError, TelemetryError};
pub use events::{export_delivered_payload, metric_emitted_payload, ExportDelivered, Timing};
pub use export::{deliver, export_events, export_view, ExportBatch, LossEntry};
pub use propagation::{
    inbound_context, outbound_context, propagation_unsupported_loss, subprocess_env,
    PropagationContext, BAGGAGE_ENV, TRACEPARENT_ENV, TRACESTATE_ENV,
};
pub use scope::{MeasurementPoint, ScopeKind, MEASUREMENT_POINTS};
pub use sinks::{ContentClass, RateLimit, Redaction, Sampling, SamplingMode, SinkPolicy};
pub use tokens::{normalize_usage, usage_from_json, ProviderUsage, TokenVector};
pub use views::{cost_view, metric_view, sink_deliveries, trace_view, Span, SpanStatus};
