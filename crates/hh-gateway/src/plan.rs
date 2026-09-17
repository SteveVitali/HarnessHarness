//! `InferenceRequest/1` — the full control envelope the `Kernel`/`ControlPlane`
//! seals and hands the gateway (§5b.1 §3) — and `ProviderRequestPlan/1`, the
//! lowered form the `ProviderCodec` produces (ADR-0118 d.1). The gateway
//! validates a plan before dispatch, binds the credential, admits the
//! endpoint, checks the budget reservation — then dispatch happens; an
//! `InferenceRequest` never touches a wire.
//!
//! `ModelParams` stays *standard* — sampling, modalities, extensions — and the
//! `prefix_affecting` marking lives on the `WireDialect` (`prefix_affecting_
//! params[]`, data not code, LC-5).

use hh_wire::json::Json;

use crate::errors::CodecError;
use crate::vocab::Purpose;

/// `sampling{...}` — the standard sampling members (§5b.1 §3 — closed at the
/// envelope; `extensions` are the profile-declared seam for anything more).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sampling {
    /// `max_output` — the output bound.
    pub max_output: Option<u64>,
    /// `temperature` in ppm (integer canonical — `t = temperature_ppm /
    /// 1_000_000`).
    pub temperature_ppm: Option<u64>,
    /// `top_p` in ppm.
    pub top_p_ppm: Option<u64>,
    /// `top_k`.
    pub top_k: Option<u64>,
    /// `seed`.
    pub seed: Option<u64>,
    /// `stop_sequences`.
    pub stop_sequences: Vec<String>,
    /// `reasoning {effort, budget}` — profile-declared (the `effort` rung and
    /// the reasoning budget).
    pub reasoning_effort: Option<String>,
    /// The reasoning budget (tokens).
    pub reasoning_budget: Option<u64>,
}

impl Sampling {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        if let Some(v) = self.max_output {
            m.insert("max_output".into(), Json::Int(v as i64));
        }
        if let Some(v) = self.temperature_ppm {
            m.insert("temperature_ppm".into(), Json::Int(v as i64));
        }
        if let Some(v) = self.top_p_ppm {
            m.insert("top_p_ppm".into(), Json::Int(v as i64));
        }
        if let Some(v) = self.top_k {
            m.insert("top_k".into(), Json::Int(v as i64));
        }
        if let Some(v) = self.seed {
            m.insert("seed".into(), Json::Int(v as i64));
        }
        if !self.stop_sequences.is_empty() {
            m.insert(
                "stop_sequences".into(),
                Json::Arr(
                    self.stop_sequences
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if self.reasoning_effort.is_some() || self.reasoning_budget.is_some() {
            let mut r = std::collections::BTreeMap::new();
            if let Some(e) = &self.reasoning_effort {
                r.insert("effort".into(), Json::str(e.clone()));
            }
            if let Some(b) = self.reasoning_budget {
                r.insert("budget".into(), Json::Int(b as i64));
            }
            m.insert("reasoning".into(), Json::Obj(r));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<Sampling, CodecError> {
        let u64of = |k: &str| j.get(k).and_then(Json::as_int).map(|v| v as u64);
        let reasoning = j.get("reasoning");
        Ok(Sampling {
            max_output: u64of("max_output"),
            temperature_ppm: u64of("temperature_ppm"),
            top_p_ppm: u64of("top_p_ppm"),
            top_k: u64of("top_k"),
            seed: u64of("seed"),
            stop_sequences: j
                .get("stop_sequences")
                .and_then(crate::dialect::json_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            reasoning_effort: reasoning
                .and_then(|r| r.get("effort"))
                .and_then(Json::as_str)
                .map(str::to_string),
            reasoning_budget: reasoning
                .and_then(|r| r.get("budget"))
                .and_then(Json::as_int)
                .map(|v| v as u64),
        })
    }
}

/// `effort ∈ {low, medium, high, max}` — the closed rung vocabulary the
/// `effort` profile rule carries (a null value is the dialect's absent).
pub const EFFORT_RUNGS: &[&str] = &["low", "medium", "high", "max"];

/// `extensions{...}` — the profile-declared extension map (provider parameters
/// no kernel member covers; each entry was declared by the resolved profile's
/// `extensions{...}` member).
pub type Extensions = std::collections::BTreeMap<String, Json>;

/// `Modalities ∈ {text, image, audio, document}` — the closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Modality {
    /// Text.
    Text,
    /// Image.
    Image,
    /// Audio.
    Audio,
    /// Document.
    Document,
}

impl Modality {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Modality::Text => "text",
            Modality::Image => "image",
            Modality::Audio => "audio",
            Modality::Document => "document",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Modality> {
        Some(match s {
            "text" => Modality::Text,
            "image" => Modality::Image,
            "audio" => Modality::Audio,
            "document" => Modality::Document,
            _ => return None,
        })
    }
}

/// `tools[]` — a lowered tool surface the plan carries: `{name, schema_hash,
/// resource_refs?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanTool {
    /// The tool's surface name.
    pub name: String,
    /// The surface schema's content address.
    pub schema_hash: String,
    /// `resource_refs[]` — `resource.<digest>` rows the tool's schema admits.
    pub resource_refs: Vec<String>,
}

/// A lowered message — `{role, blocks[]}` where the blocks are the plan's
/// carrier units (LC-9 markers attach to them).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanMessage {
    /// The role.
    pub role: String,
    /// The block kinds in order (the placement input; the block content lives
    /// in `blocks`).
    pub blocks: Vec<PlanBlock>,
}

/// A plan block — `{kind, content?, opaque_ref?, marker?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanBlock {
    /// The block kind.
    pub kind: crate::vocab::BlockKind,
    /// The block's content (opaque text the gateway moves, never reads).
    pub content: Option<String>,
    /// An opaque content-addressed payload (`provider_opaque` /
    /// `redacted_reasoning`).
    pub opaque_ref: Option<String>,
    /// A placed marker (`Some` when `place_markers` attached one — the
    /// retention class, when the dialect names one).
    pub marker: Option<String>,
}

/// `ModelRef{profile_ref, provider_model_id, snapshot_id?, serving_route?,
/// effort?}` — the one model coordinate (ADR-0039 d.3; ADR-0121 d.3): the
/// router selects it, the gateway targets it, charges carry it.
/// `provider_model_id`/`serving_route` are surface aliases; `profile_ref` is
/// the bound `ModelProfile` coordinate (`model.call.requested` stamps it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    /// `profile_ref` — the role's bound profile coordinate.
    pub profile_ref: String,
    /// `provider_model_id` — the provider's model spelling (a surface alias).
    pub provider_model_id: String,
    /// `snapshot_id?` — the pinned snapshot, when the binding names one.
    pub snapshot_id: Option<String>,
    /// `serving_route?` — the serving-route coordinate (external-gateway).
    pub serving_route: Option<String>,
    /// `effort?` — the per-call effort rung (within `capabilities.effort`).
    pub effort: Option<String>,
}

impl ModelRef {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("profile_ref".into(), Json::str(self.profile_ref.clone()));
        m.insert(
            "provider_model_id".into(),
            Json::str(self.provider_model_id.clone()),
        );
        if let Some(s) = &self.snapshot_id {
            m.insert("snapshot_id".into(), Json::str(s.clone()));
        }
        if let Some(s) = &self.serving_route {
            m.insert("serving_route".into(), Json::str(s.clone()));
        }
        if let Some(e) = &self.effort {
            m.insert("effort".into(), Json::str(e.clone()));
        }
        Json::Obj(m)
    }

    /// `provider_model_id@serving_route` — the pricing/attribution key
    /// (`serving_route` absent ⇒ the bare `provider_model_id`).
    pub fn pricing_key(&self) -> String {
        match &self.serving_route {
            Some(r) => format!("{}@{}", self.provider_model_id, r),
            None => self.provider_model_id.clone(),
        }
    }

    /// The charge-side `hh_budget::ModelRef` (`serving_route` required —
    /// `None` when the route hasn't resolved; ADR-0039 d.3).
    pub fn charge_ref(&self) -> Option<hh_budget::ModelRef> {
        self.serving_route.as_ref().map(|r| hh_budget::ModelRef {
            profile_ref: self.profile_ref.clone(),
            provider_model_id: self.provider_model_id.clone(),
            serving_route: r.clone(),
            effort: self.effort.clone(),
        })
    }
}

/// `RequestSpec` — the pre-lowering request content: what the compiler-side
/// lowerer renders into `ProviderRequestPlan.body` (sampling, tool surfaces,
/// carrier messages, dialect-scoped `provider_params`). This is *not* the
/// gateway envelope — the gateway never reads it (the plan is already
/// lowered when the call opens).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RequestSpec {
    /// `model_params` — sampling/modalities.
    pub sampling: Sampling,
    /// `extensions` — profile-declared provider parameters.
    pub extensions: Extensions,
    /// `tool_surface` — the tool surfaces in plan order.
    pub tools: Vec<PlanTool>,
    /// `messages` — the lowered carrier blocks.
    pub messages: Vec<PlanMessage>,
    /// `snapshot` — `unresolved | resolved{id} | request{snapshot_id}`.
    pub snapshot: Option<String>,
    /// `provider_params` — dialect-scoped parameters (validated against
    /// `WireDialect.plan_schema`).
    pub provider_params: Extensions,
}

/// `observability{capture_request, capture_response}` — the per-call capture
/// flags (`request_ref`/`response_ref` blobs exist iff `model_io`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CallObservability {
    /// `capture_request` — retain the request blob.
    pub capture_request: bool,
    /// `capture_response` — retain the response blob.
    pub capture_response: bool,
}

/// `InferenceRequest/1` — the control envelope `open_call` accepts
/// (§5b.1 §3; ADR-0118 d.3): `{model_call_id, model_ref{profile_ref,
/// provider_model_id, snapshot_id?, serving_route?}, plan:
/// ProviderRequestPlan, context_label, budget_reservation_id, attempt_policy,
/// deadline_ms?, cache_state_hint ∈ {warm, cold, uncacheable, unknown},
/// seed_request?, observability{capture_request, capture_response}}` plus the
/// ledger stamps the call owns (`role`, `purpose`, `binding_ref`,
/// `credential_ref`, `request_class`, `stream`). All identity members are
/// references, never credentials.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceRequest {
    /// `model_call_id` — allocated by B1 (ADR-0027); the call's ledger scope.
    pub model_call_id: String,
    /// `model_ref` — the router's `selected` coordinate.
    pub model_ref: ModelRef,
    /// `plan` — the sealed `ProviderRequestPlan` (immutable for the life of
    /// `model_call_id`).
    pub plan: ProviderRequestPlan,
    /// `role` — the `ModelRole` spelling this call serves (the
    /// `profile_binding` key).
    pub role: String,
    /// `purpose` — the affinity-key/prefix-extension scope.
    pub purpose: Purpose,
    /// `context_label` — the kernel-stamped label (ADR-0053 CF-118).
    pub context_label: Option<String>,
    /// `view_hash` — the assembled context view's hash
    /// (`context.assembled.view_hash`, ADR-0072) stamped on `requested`.
    pub view_hash: Option<String>,
    /// `binding_ref` — the `EstablishedBinding` coordinate.
    pub binding_ref: String,
    /// `credential_ref` — a credential **reference** (never a secret).
    pub credential_ref: Option<String>,
    /// `budget_reservation_id` — required before bytes leave
    /// (reserve-before-spend, ADR-0040).
    pub budget_reservation_id: Option<String>,
    /// `attempt_policy` — data supplied by the control envelope F2.
    pub attempt_policy: crate::vocab::AttemptPolicy,
    /// `deadline_ms?` — the call's deadline.
    pub deadline_ms: Option<u64>,
    /// `cache_state_hint ∈ {warm, cold, uncacheable, unknown}` — filled from
    /// `expected_state` (ADR-0128); the [`crate::cache::ExpectedState`]
    /// spelling is the same closed set.
    pub cache_state_hint: crate::cache::ExpectedState,
    /// `seed_request?` — the caller's seed request (the profile's
    /// `seed_honoured` governs whether it reaches the wire).
    pub seed_request: Option<u64>,
    /// `observability{capture_request, capture_response}`.
    pub observability: CallObservability,
    /// `request_class ∈ {interactive, background}` — a `background` call is
    /// legal only under `deferred_requests`.
    pub request_class: RequestClass,
    /// `deferred_deadline_ms` — `deferred{response_deadline_ms?}` when the
    /// call is background.
    pub deferred_deadline_ms: Option<u64>,
    /// `stream` — whether the caller wants a stream.
    pub stream: bool,
    /// `identity` — the caller's `Identity` record coordinate.
    pub identity: Option<String>,
}

/// `request_class ∈ {interactive, background}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    /// An interactive call.
    Interactive,
    /// A deferred/background call.
    Background,
}

impl RequestClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RequestClass::Interactive => "interactive",
            RequestClass::Background => "background",
        }
    }
}

/// `ProviderRequestPlan/1` — the lowered wire-shape the `ProviderCodec` produces
/// and the gateway dispatches (ADR-0118 d.1). It is the dialect's render of an
/// `InferenceRequest` plus its cache plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequestPlan {
    /// `dialect` — the `{dialect_id, version}` this plan was lowered under.
    pub dialect_id: String,
    /// The dialect version.
    pub dialect_version: String,
    /// `endpoint_ref`.
    pub endpoint_ref: String,
    /// `model_ref`.
    pub model_ref: String,
    /// The request body (the dialect's canonical render — opaque to the
    /// gateway after the codec produces it).
    pub body: Json,
    /// `provider_params` (validated against `plan_schema`).
    pub provider_params: Extensions,
    /// `capabilities` — the capability declarations the plan asserts.
    pub capabilities: Capabilities,
    /// `cache` — the cache plan (`markers[]`, `markers_dropped[]`,
    /// `markers_substituted[]`, `expected_state`).
    pub cache: CachePlan,
    /// `budget_reservation_id` — carried through for the gateway's
    /// reserve-before-spend check.
    pub budget_reservation_id: Option<String>,
    /// The canonical byte length (the C0 `local_len` estimator input).
    pub canonical_len: u64,
}

/// `capabilities{streaming, tool_use, vision?, deferred?}` — the capability
/// declarations a plan asserts (`capabilities streaming` / `capabilities
/// tool_use`); `Some` members are dialect support, `None` is "not declared".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capabilities {
    /// `streaming` — the dialect streams (`WireDialect.streaming`).
    pub streaming: bool,
    /// `tool_use` — the plan carries tool surfaces.
    pub tool_use: bool,
    /// `vision` — the plan carries image blocks (declared, never assumed).
    pub vision: Option<bool>,
    /// `deferred` — the plan is a `Deferred` request (legal only under
    /// `deferred_requests`).
    pub deferred: Option<bool>,
}

impl Capabilities {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("streaming".into(), Json::Bool(self.streaming));
        m.insert("tool_use".into(), Json::Bool(self.tool_use));
        if let Some(v) = self.vision {
            m.insert("vision".into(), Json::Bool(v));
        }
        if let Some(v) = self.deferred {
            m.insert("deferred".into(), Json::Bool(v));
        }
        Json::Obj(m)
    }
}

/// `cache{markers[], markers_dropped[], markers_substituted[], expected_state,
/// affinity_key?}` — the cache plan a lowered plan carries (ADR-0127/0128).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CachePlan {
    /// `markers[]` — the placed markers.
    pub markers: Vec<crate::cache::PlacedMarker>,
    /// `markers_dropped[]`.
    pub dropped: Vec<crate::cache::DroppedMarker>,
    /// `markers_substituted[]`.
    pub substituted: Vec<crate::cache::MarkerSubstitution>,
    /// `expected_state` + basis.
    pub expected: Option<crate::cache::ExpectedCacheState>,
    /// `affinity_key` — the `wire` form (a surface alias) when the dialect
    /// supports it.
    pub affinity_wire: Option<String>,
    /// `affinity_key.full` — the ledger fact.
    pub affinity_full: Option<String>,
    /// `static_hash` — the static-tier digest (LC-2/5).
    pub static_hash: Option<String>,
}

impl ProviderRequestPlan {
    /// The plan's content id (`plan.1`).
    pub fn content_id(&self) -> String {
        hh_identity::idp_id("plan.1", self.to_json().to_canonical_string().as_bytes())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("schema".into(), Json::str("hh.provider_request_plan/1"));
        m.insert(
            "dialect".into(),
            Json::obj([
                ("dialect_id", Json::str(self.dialect_id.clone())),
                ("version", Json::str(self.dialect_version.clone())),
            ]),
        );
        m.insert("endpoint_ref".into(), Json::str(self.endpoint_ref.clone()));
        m.insert("model_ref".into(), Json::str(self.model_ref.clone()));
        m.insert("body".into(), self.body.clone());
        if !self.provider_params.is_empty() {
            m.insert(
                "provider_params".into(),
                Json::Obj(self.provider_params.clone()),
            );
        }
        m.insert("capabilities".into(), self.capabilities.to_json());
        Json::Obj(m)
    }
}
