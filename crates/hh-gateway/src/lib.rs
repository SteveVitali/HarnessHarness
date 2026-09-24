//! `hh-gateway` — the C0/Stage-1 model adapter / gateway (spec §5b.1–§5b.4;
//! R-2.3.1, the R-2.3.2 C0 static slice, R-2.3.4⁰; ADR-0118…0128; ticket
//! S1.18).
//!
//! The gateway is a boundary component (`home = model_boundary`):
//! model-blind, dialect-parameterized, transport-only. It accepts a sealed
//! `ProviderRequestPlan` plus its `InferenceRequest` control envelope, never
//! branches on a model identifier, and is the sole emitter of usage, cost
//! provenance, attempt timing, served-model drift and transport capability
//! facts.
//!
//! Module map:
//! - [`vocab`] — the closed vocabularies (`StopReason`, `ModelErrorClass`
//!   with the kernel `retry_class` defaults, `RetryClass`, `BlockKind`,
//!   `NoticeKind`, `UsageArrival`, `Purpose`, `AttemptPolicy`).
//! - [`dialect`] — `WireDialect/1` (strict codec; every textual rule carries
//!   a complete `DebtRecord`; a `retry_class_override` may only narrow).
//! - [`plan`] — `InferenceRequest/1`, `ProviderRequestPlan/1`, `Sampling`,
//!   `Capabilities`, the cache plan.
//! - [`message`] — `ModelMessage{blocks[]}`, `ModelBlock`, `Text`,
//!   `OpaqueBlock`, `ToolCall`, the strict tool-call parse.
//! - [`codec`] — `serialize`/`decode`/`estimate_tokens`, HTTP→class
//!   classification, usage normalization (delegates to `hh-telemetry`'s one
//!   convention), opaque pass-through.
//! - [`grammar`] — the `ModelEvent` grammar G1–G7 and the stream decoder.
//! - [`attempts`] — the R-RT-1…6 attempt driver (identical bytes; the policy
//!   is data; the sleeper is a port).
//! - [`gateway`] — `ModelGateway` (`open_call`/`stream`/`complete`/`cancel`/
//!   `estimate_tokens`/`discover`) over `Transport`/`CredentialPort`/
//!   `EventSink` ports.
//! - [`router`] — the C0 `ModelRoleTable`/`RoutingPolicy` slice with the
//!   closed `RoutingRefusal` sum.
//! - [`cache`] — `CacheSemantics` as data, the affinity key, expected/
//!   observed cache state, `place_markers`, `model.cache.resolved` vocabulary.
//! - [`layout`] — the LC-1…LC-9 layout invariants.
//! - [`events`] — the `model.*` payload builders (the kernel appends them).
//! - [`errors`] — `GatewayError`, `CodecError` (typed refusals, never
//!   warnings).

pub mod attempts;
pub mod cache;
pub mod codec;
pub mod dialect;
pub mod errors;
pub mod events;
pub mod gateway;
pub mod grammar;
pub mod layout;
pub mod message;
pub mod plan;
pub mod router;
pub mod vocab;

pub use attempts::{
    retryable, run_attempts, AttemptOutcome, CollectSink, EventSink, NoSleep, SharedSink, Sleeper,
};
pub use cache::{
    cache_agreement, cold_start_scope, derive_affinity_key, expect_cache_state, observe_cache,
    place_markers, AffinityKey, AffinitySupport, CacheCallFact, CacheKind, CacheObservation,
    CacheOutcome, CacheSemantics, CacheTier, CarrierBlock, DropReason, DroppedMarker,
    ExpectedBasis, ExpectedCacheState, ExpectedState, ImplicitStrictness, IsolationScope,
    MarkerPlacement, MarkerPosition, MarkerPositionRule, MarkerSubstitution, MissReason,
    ObservedState, PlacedMarker, RetentionClass,
};
pub use codec::{
    classify_http_error, collect, decode_b64, detect_provider_usage, estimate_tokens,
    is_metadata_frame, kernel_status_class, map_stop_reason, normalize_usage, notice_kind,
    parse_provider_opaque, read_retry_after, serialize, validate_plan, Estimate,
    EstimateConfidence, EstimateMethod, WireFrame,
};
pub use dialect::{
    AuthKind, DebtEvidence, DebtRecord, DialectRule, Framing, PlanSchema, RetryAfterSource,
    UsageMapping, UsageMappingDefault, WireDialect,
};
pub use errors::{CodecError, GatewayError};
pub use gateway::{
    CallHandle, CredentialHandle, CredentialPort, EndpointAllowlist, ModelGateway, NoCredentials,
    Transport,
};
pub use grammar::{
    decode_frame, DecodeState, Delta, GrammarViolation, ModelEvent, ModelEventKind, Timing,
    UsageArrivalKind,
};
pub use layout::{
    check_layout, check_marker_carriers, check_prefix_affecting, check_purpose_isolation,
    check_static_stability, check_tool_order_prefix, check_transcript_append, is_volatile_kind,
    LayoutViolation, SlotFacts, VOLATILE_KINDS,
};
pub use message::{
    ModelBlock, ModelMessage, OpaqueLeaf, Text, TextAuthority, TextTrust, TextVisibility, ToolArgs,
    ToolCallBlock, UnparseableReason,
};
pub use plan::{
    CachePlan, CallObservability, Capabilities, Extensions, InferenceRequest, Modality, ModelRef,
    PlanBlock, PlanMessage, PlanTool, ProviderRequestPlan, RequestClass, RequestSpec, Sampling,
    EFFORT_RUNGS,
};
pub use router::{
    error_action, reroute, select, AccountBudget, BudgetPort, Candidate, CandidateRejectReason,
    CandidateVerdict, ErrorAction, HealthView, MigrationLossBound, ModelRoleTable, NoHealth,
    PolicyConditionedRule, RoleBinding, RouteCandidate, RoutingDecision, RoutingPolicy,
    RoutingPolicyKind, RoutingRefusal, RoutingRequest,
};
pub use vocab::{
    AttemptPolicy, BlockKind, ErrorAttribution, InvalidResponseKind, ModelError, ModelErrorClass,
    NotFoundKind, NoticeKind, Purpose, RerouteReason, RetryClass, StopCategory, StopDetails,
    StopReason, Substitution, TimeoutKind, UsageArrival, DEFAULT_ATTEMPT_POLICY,
};
