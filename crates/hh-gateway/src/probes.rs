//! §5b.3 — drift probes: the transport probe, the fingerprint probe, and
//! discovery → `ConformanceRecord`/DRIFT rows (AC-R-2.3.1-{9b,12,14}).
//!
//! Every probe is a *measured* instrument call: probe calls ride the normal
//! `open_call`/`stream` path with `purpose = probe`, so their usage is
//! metered and the eval-side accounting projection charges them to
//! `instrument` (§5b.1's accounting row — probe/discovery calls never
//! inherit the arm's budget). A probe produces facts and records — it never
//! mutates the pinned `ModelProfile`/`ModelSnapshotRecord` (the observation
//! record and the `SnapshotClaim` are new rows).

use std::collections::BTreeMap;

use hh_provenance::ProvenanceRecord;
use hh_registry::records::{ConformanceRecord, ObservedIn};
use hh_wire::json::Json;

use crate::attempts::EventSink;
use crate::dialect::WireDialect;
use crate::errors::GatewayError;
use crate::gateway::ModelGateway;
use crate::plan::InferenceRequest;
use crate::snapshot::{
    self, FingerprintVerdict, ModelSnapshotRecord, SnapshotClaim, SnapshotClaimKind,
};
use crate::vocab::{ModelError, ModelErrorClass, Purpose};

/// The adapter's own version pin — stamped on every probe-produced
/// `ConformanceRecord.adapter_version_id` (a build-time constant, never an
/// env read).
pub const ADAPTER_VERSION: &str = concat!("hh-gateway/", env!("CARGO_PKG_VERSION"));

// ─────────────────────────────────────────────────────────────────────────────
// Transport probes (AC-R-2.3.1-12)
// ─────────────────────────────────────────────────────────────────────────────

/// `TransportCapabilityDeclaration` — the capability claims a loaded dialect
/// declares (`describe(dialect)`; §5b.3). `entries` is `(capability,
/// declared_value)` in declaration order — the closed row set a transport
/// probe walks.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportCapabilityDeclaration {
    /// The dialect coordinate (`{dialect_id}@{version}`).
    pub dialect_coordinate: String,
    /// The declared rows.
    pub entries: Vec<(String, Json)>,
}

/// `describe(dialect)` — the dialect's declared capability surface. Declared
/// values come from the descriptor, never from observation; the probe's job
/// is to elicit the *observed* side.
pub fn describe(dialect: &WireDialect) -> TransportCapabilityDeclaration {
    let entries = vec![
        ("cap.streaming".to_string(), Json::Bool(dialect.streaming)),
        (
            "cap.deferred_requests".to_string(),
            Json::Bool(dialect.deferred_requests),
        ),
        (
            "cap.model_listing".to_string(),
            Json::Bool(dialect.model_listing.is_some()),
        ),
        (
            "cap.count_tokens".to_string(),
            Json::Bool(dialect.count_tokens.is_some()),
        ),
        (
            "cap.cache_state_visible".to_string(),
            Json::Bool(dialect.cache_state_visible),
        ),
        (
            "cap.snapshot_id_exposed".to_string(),
            Json::Bool(dialect.snapshot_id_exposed.is_some()),
        ),
        (
            "cache.kind".to_string(),
            Json::str(dialect.cache_semantics.kind()),
        ),
    ];
    TransportCapabilityDeclaration {
        dialect_coordinate: format!("{}@{}", dialect.dialect_id, dialect.version),
        entries,
    }
}

/// `run_transport_probes(dialect, transport, endpoint_ref, pinned, canaries,
/// observed_at)` — for each declared row, issue the probe that elicits the
/// claim and fold pinned-vs-observed into a `ConformanceRecord` (the
/// `derive_verdict` rule: concrete disagreement → `Drift`; `unknown`/
/// `skipped` never drift). `pinned` is the registry's pinned claim values
/// (`capability → Json`); an unpinned capability carries `declared = null`
/// (the observed verdict projects — the pin's absence is recorded, not
/// coerced). A transport without a probe primitive yields `observed =
/// skipped` rows; a probe *failure* is an `Err`, never a silent row.
///
/// `canaries` names capabilities the declaration does **not** carry
/// (AC-R-2.3.1-14's honesty clause): an *undeclared* operation the transport
/// answers successfully produces a `Drift` row (`declared = unsupported`,
/// `observed = <the success value>`) — the record fails the suite, never
/// silently passes.
pub fn run_transport_probes(
    dialect: &WireDialect,
    transport: &mut dyn crate::gateway::Transport,
    endpoint_ref: &str,
    pinned: &BTreeMap<String, Json>,
    canaries: &[String],
    observed_at: u64,
) -> Result<Vec<ConformanceRecord>, ModelError> {
    let declaration = describe(dialect);
    let mut out = Vec::new();
    for (capability, _declared_by_dialect) in &declaration.entries {
        let observed = match transport.probe_capability(capability, endpoint_ref) {
            Ok(v) => v,
            Err(e) if e.class == ModelErrorClass::UnsupportedFeature => Json::str("skipped"),
            Err(e) => return Err(e),
        };
        out.push(ConformanceRecord::observed_entry(
            &declaration.dialect_coordinate,
            ADAPTER_VERSION,
            capability,
            pinned.get(capability).cloned().unwrap_or(Json::Null),
            observed,
            ObservedIn::Probe,
            None,
            observed_at,
        ));
    }
    for capability in canaries {
        let observed = match transport.probe_capability(capability, endpoint_ref) {
            Ok(v) => v,
            Err(e) if e.class == ModelErrorClass::UnsupportedFeature => Json::str("unsupported"),
            Err(e) => return Err(e),
        };
        out.push(ConformanceRecord::observed_entry(
            &declaration.dialect_coordinate,
            ADAPTER_VERSION,
            capability,
            Json::str("unsupported"),
            observed,
            ObservedIn::Probe,
            None,
            observed_at,
        ));
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Discovery → DRIFT (AC-R-2.3.1-14)
// ─────────────────────────────────────────────────────────────────────────────

/// `discovery_claims(descriptor)` — flatten a `discover` descriptor
/// (`{api_version?, served_models[]?, capabilities{}?, limits{}?}`) into the
/// flat claim map the conformance walk compares (`capability.<name>`,
/// `limit.<name>`, `served_model.<id>` = `true`).
pub fn discovery_claims(descriptor: &Json) -> BTreeMap<String, Json> {
    let mut out = BTreeMap::new();
    if let Json::Obj(m) = descriptor {
        if let Some(v) = m.get("api_version") {
            out.insert("api_version".to_string(), v.clone());
        }
        if let Some(Json::Obj(caps)) = m.get("capabilities") {
            for (k, v) in caps {
                out.insert(format!("capability.{k}"), v.clone());
            }
        }
        if let Some(Json::Obj(limits)) = m.get("limits") {
            for (k, v) in limits {
                out.insert(format!("limit.{k}"), v.clone());
            }
        }
        if let Some(Json::Arr(models)) = m.get("served_models") {
            for model in models {
                if let Some(id) = model.as_str() {
                    out.insert(format!("served_model.{id}"), Json::Bool(true));
                }
            }
        }
    }
    out
}

/// `discovery_conformance(participant_coordinate, descriptor, pinned,
/// observed_at)` — one `ConformanceRecord` per claim in the sorted union of
/// `discovery_claims(descriptor)` and `pinned` (AC-R-2.3.1-14): a pinned
/// claim the descriptor contradicts produces a `DRIFT` row; an unpinned or
/// unobserved claim produces its honest verdict (`Unknown`/observed
/// projection), never a fabricated drift.
pub fn discovery_conformance(
    participant_coordinate: &str,
    descriptor: &Json,
    pinned: &BTreeMap<String, Json>,
    observed_at: u64,
) -> Vec<ConformanceRecord> {
    let observed_claims = discovery_claims(descriptor);
    let keys: std::collections::BTreeSet<&String> =
        observed_claims.keys().chain(pinned.keys()).collect();
    let mut out = Vec::new();
    for claim_id in keys {
        let observed = observed_claims
            .get(claim_id)
            .cloned()
            .unwrap_or_else(|| Json::str("unknown"));
        out.push(ConformanceRecord::observed_entry(
            participant_coordinate,
            ADAPTER_VERSION,
            claim_id,
            pinned.get(claim_id).cloned().unwrap_or(Json::Null),
            observed,
            ObservedIn::Probe,
            None,
            observed_at,
        ));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// The fingerprint probe (AC-R-2.3.1-9b)
// ─────────────────────────────────────────────────────────────────────────────

/// `FingerprintProbeSpec` (§5b.3) — `{probe_id, model_id, snapshot_id?,
/// prompts[], prompt_band_min, tokenizer_expected?, semantic_expected?,
/// budget{max_units, estimated_charge}}`. `prompt_band_min < 1.0` is legal —
/// a band evaluation, not a unanimity rule.
#[derive(Debug, Clone, PartialEq)]
pub struct FingerprintProbeSpec {
    /// The probe id (the canonical request's `probe_id` member).
    pub probe_id: String,
    /// The model coordinate under test (`model_ref.provider_model_id`).
    pub model_id: String,
    /// The snapshot the pin names, when it names one.
    pub snapshot_id: Option<String>,
    /// The probe prompts — one measured call each.
    pub prompts: Vec<String>,
    /// The minimum per-prompt agreement (`< 1.0` allowed).
    pub prompt_band_min: f64,
    /// The pinned tokenizer version, when pinned.
    pub tokenizer_expected: Option<String>,
    /// The pinned semantic-identity fingerprint, when pinned.
    pub semantic_expected: Option<String>,
    /// `budget{max_units}` — the probe's spend cap.
    pub budget_max_units: Option<u64>,
    /// `budget{estimated_charge}` — the estimated-charge stamp the
    /// instrument carries.
    pub estimated_charge: Option<u64>,
}

/// `canonical_probe_request(spec)` — the probe's canonical request body
/// (the same shape `snapshot::probe_request` fixes, parameterized by the
/// spec's prompts).
pub fn canonical_probe_request(spec: &FingerprintProbeSpec) -> Json {
    Json::obj([
        ("kind", Json::str("fingerprint_probe")),
        ("probe_id", Json::str(spec.probe_id.clone())),
        (
            "prompts",
            Json::Arr(spec.prompts.iter().map(|p| Json::str(p.clone())).collect()),
        ),
        ("temperature", Json::Int(0)),
        ("max_tokens", Json::Int(128)),
    ])
}

/// The probe's per-prompt observations.
#[derive(Debug, Clone, PartialEq)]
pub struct FingerprintProbeOutcome {
    /// The verdict (`drift` | `match` | `unknown`).
    pub verdict: FingerprintVerdict,
    /// `true` when the per-prompt agreement fell under `prompt_band_min`.
    pub band_hit: bool,
    /// The measured agreement (`agreed / prompts.len()`; `0` on no
    /// observations).
    pub agreement: f64,
    /// The observed fingerprint digest (the first prompt's — the agreement
    /// baseline; per-prompt digests are derivable from `per_prompt`).
    pub observed_fingerprint: Option<String>,
    /// Per-prompt agreement flags, in prompt order.
    pub per_prompt: Vec<bool>,
    /// The observation's `ModelSnapshotRecord` (a new row — the pin is
    /// untouched).
    pub record: ModelSnapshotRecord,
    /// The `SnapshotClaim` issued on `Drift` (served-model substitution or
    /// fingerprint drift — `None` on `Match`/`Unknown`).
    pub claim: Option<SnapshotClaim>,
    /// How many measured probe calls ran (each `purpose = probe` →
    /// `charged_to = instrument` in the accounting projection).
    pub calls: usize,
}

/// `fingerprint_probe_verdict(spec, pinned, observed_fingerprint,
/// per_prompt, observed_semantic, observed_tokenizer)` — the pure verdict
/// rule (§5b.3):
///
/// - *no observation* → `Unknown` (never a fabricated drift);
/// - `agreement < prompt_band_min` → `Drift` (the band hit);
/// - a pinned semantic/tokenizer value contradicted by the observation →
///   `Drift`;
/// - the pinned fingerprint contradicted → `Drift`;
/// - a *matched* fingerprint (or all pinned comparisons agreeing with the
///   band satisfied) → `Match`;
/// - evidence observed but nothing pinned to agree with → `Unknown`.
pub fn fingerprint_probe_verdict(
    spec: &FingerprintProbeSpec,
    pinned: &ModelSnapshotRecord,
    observed_fingerprint: Option<&str>,
    per_prompt: &[bool],
    observed_semantic: Option<&str>,
    observed_tokenizer: Option<&str>,
) -> (FingerprintVerdict, bool) {
    let anything_observed = !per_prompt.is_empty()
        || observed_fingerprint.is_some()
        || observed_semantic.is_some()
        || observed_tokenizer.is_some();
    if !anything_observed {
        return (FingerprintVerdict::Unknown, false);
    }
    let agreement = if per_prompt.is_empty() {
        0.0
    } else {
        per_prompt.iter().filter(|a| **a).count() as f64 / per_prompt.len() as f64
    };
    let band_hit = !per_prompt.is_empty() && agreement < spec.prompt_band_min;
    let pinned_miss = |expected: Option<&str>, observed: Option<&str>| match (expected, observed) {
        (Some(e), Some(o)) => Some(e == o),
        _ => None,
    };
    let semantic_cmp = pinned_miss(spec.semantic_expected.as_deref(), observed_semantic);
    let tokenizer_cmp = pinned_miss(spec.tokenizer_expected.as_deref(), observed_tokenizer);
    let fp =
        snapshot::fingerprint_verdict(pinned.observed_fingerprint.as_deref(), observed_fingerprint);
    if band_hit
        || semantic_cmp == Some(false)
        || tokenizer_cmp == Some(false)
        || fp == FingerprintVerdict::Drift
    {
        return (FingerprintVerdict::Drift, band_hit);
    }
    let some_pinned_agreement = fp == FingerprintVerdict::Match
        || semantic_cmp == Some(true)
        || tokenizer_cmp == Some(true);
    if some_pinned_agreement && !band_hit {
        (FingerprintVerdict::Match, false)
    } else {
        (FingerprintVerdict::Unknown, band_hit)
    }
}

/// `fingerprint_basis(message)` — the canonical response projection the
/// fingerprint digest covers (§5b.3's "canonical response body" rendered over
/// the *decoded* `ModelMessage`: block kinds with their content/signature
/// digests, the served-model and snapshot drift facts, and the stop reason —
/// the semantic content, never transport framing).
pub fn fingerprint_basis(message: &crate::message::ModelMessage) -> Json {
    use crate::message::ModelBlock;
    let blocks = Json::Arr(
        message
            .blocks
            .iter()
            .map(|b| match b {
                ModelBlock::Text { text, signature } => Json::obj([
                    ("content", Json::str(text.content.clone())),
                    ("kind", Json::str("text")),
                    (
                        "signature",
                        signature
                            .as_ref()
                            .map(|s| Json::str(s.bytes_hash()))
                            .unwrap_or(Json::Null),
                    ),
                ]),
                ModelBlock::Reasoning {
                    text,
                    signature,
                    redacted,
                    ..
                } => Json::obj([
                    (
                        "content",
                        text.as_ref()
                            .map(|t| Json::str(t.content.clone()))
                            .unwrap_or(Json::Null),
                    ),
                    ("kind", Json::str("reasoning")),
                    ("redacted", Json::Bool(*redacted)),
                    (
                        "signature",
                        signature
                            .as_ref()
                            .map(|s| Json::str(s.bytes_hash()))
                            .unwrap_or(Json::Null),
                    ),
                ]),
                ModelBlock::ToolCall(t) => Json::obj([
                    ("kind", Json::str("tool_call")),
                    ("provider_index", Json::Int(t.provider_index as i64)),
                    ("surface_name", Json::str(t.surface_name.clone())),
                    ("truncated", Json::Bool(t.truncated)),
                ]),
                ModelBlock::ProviderOpaque { kind, payload, .. } => Json::obj([
                    ("kind", Json::str("provider_opaque")),
                    ("opaque_kind", Json::str(kind.clone())),
                    ("payload", Json::str(payload.bytes_hash())),
                ]),
            })
            .collect(),
    );
    Json::obj([
        ("blocks", blocks),
        (
            "served_model",
            message.served_model.clone().map_or(Json::Null, Json::Str),
        ),
        (
            "snapshot_id",
            message.snapshot_id.clone().map_or(Json::Null, Json::Str),
        ),
        ("stop_reason", Json::str(message.stop_reason.as_str())),
    ])
}

/// `run_fingerprint_probe(spec, pinned, make_request, gateway, sink,
/// provenance)` — the §5b.3 driver: one measured call per prompt through the
/// normal gateway path (`purpose = probe` — the instrument-charge marker),
/// the fingerprint digest over each decoded `ModelMessage`'s canonical
/// bytes, per-prompt agreement against the pin (or the first observation as
/// the self-consistency baseline when nothing is pinned), the band
/// evaluation, and — on `Drift` — the `SnapshotClaim`.
///
/// `make_request(prompt_index, prompt)` builds the probe call's
/// `InferenceRequest`; the driver forces `purpose = Probe` and stamps the
/// probe id on the plan's `provider_params` so the emitted `requested` rows
/// are identifiable as instrument calls.
pub fn run_fingerprint_probe(
    spec: &FingerprintProbeSpec,
    pinned: &ModelSnapshotRecord,
    make_request: &dyn Fn(usize, &str) -> InferenceRequest,
    gateway: &mut ModelGateway<'_>,
    sink: &mut dyn EventSink,
    provenance: ProvenanceRecord,
) -> Result<FingerprintProbeOutcome, GatewayError> {
    let mut digests: Vec<String> = Vec::new();
    let mut served_model: Option<String> = None;
    let mut snapshot_id: Option<String> = None;
    for (i, prompt) in spec.prompts.iter().enumerate() {
        let mut request = make_request(i, prompt);
        // Instrument call — the probe rides the normal path so its usage is
        // metered; `purpose = probe` is what the accounting projection keys
        // `charged_to = instrument` on (§5b.1).
        request.purpose = Purpose::Probe;
        request.plan.provider_params.insert(
            "fingerprint_probe".to_string(),
            Json::str(spec.probe_id.clone()),
        );
        let handle = gateway.open_call(request, sink)?;
        let message = gateway.complete(&handle, sink)?;
        if served_model.is_none() {
            served_model = message.served_model.clone();
        }
        if snapshot_id.is_none() {
            snapshot_id = message.snapshot_id.clone();
        }
        digests.push(snapshot::fingerprint_response(
            fingerprint_basis(&message).to_canonical_string().as_bytes(),
        ));
    }
    // Per-prompt agreement — against the pinned fingerprint when one exists,
    // else the first observation (self-consistency).
    let baseline = pinned
        .observed_fingerprint
        .clone()
        .or_else(|| digests.first().cloned());
    let per_prompt: Vec<bool> = digests
        .iter()
        .map(|d| Some(d.as_str()) == baseline.as_deref())
        .collect();
    let observed_fingerprint = digests.first().cloned();
    let (verdict, band_hit) = fingerprint_probe_verdict(
        spec,
        pinned,
        observed_fingerprint.as_deref(),
        &per_prompt,
        None,
        None,
    );
    let (record, mut claim) = snapshot::observe(
        pinned,
        served_model.as_deref(),
        snapshot_id.as_deref(),
        observed_fingerprint.as_deref(),
        provenance,
    );
    // A `Drift` verdict always issues the claim — the band-hit case has no
    // fingerprint contradiction for `observe` to raise, so the driver raises
    // it (AC-R-2.3.1-9b).
    if verdict == FingerprintVerdict::Drift && claim.is_none() {
        claim = Some(SnapshotClaim {
            claim_kind: SnapshotClaimKind::FingerprintDrift,
            snapshot_id: snapshot_id
                .clone()
                .unwrap_or_else(|| "observed".to_string()),
            pinned_model_id: pinned.model_id.clone(),
            observed: observed_fingerprint.clone().unwrap_or_default(),
        });
    }
    let agreement = if per_prompt.is_empty() {
        0.0
    } else {
        per_prompt.iter().filter(|a| **a).count() as f64 / per_prompt.len() as f64
    };
    Ok(FingerprintProbeOutcome {
        verdict,
        band_hit,
        agreement,
        observed_fingerprint,
        per_prompt,
        record,
        claim,
        calls: spec.prompts.len(),
    })
}
