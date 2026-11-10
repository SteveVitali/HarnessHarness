//! `gateway_model` — `GatewayModel`, the production `ModelPort` for the
//! `model_port` slot's live binding (R4.5b; ADR-0364 D4; DF-S1.18-1).
//!
//! The driver owns the emission fence: `model.call.requested`/
//! `completed`/`failed` land in `model_round` exactly as the scripted
//! lane writes them — this port contributes members (R4.5b's
//! `requested_members`/`terminal_members` hooks) and `hh.model_io/1`
//! blobs (drained + deposited through the fenced store seam before the
//! terminal row lands — the `response_ref` resolves on any later read).
//!
//! Assembly per call: the durable `prefix` renders the transcript
//! (instruction + prior `model.call.completed` tool-call turns + the
//! `context.observation.recorded` tool results + refusal notes), the
//! armed surfaces render `tools[]`, the bound `WireDialect` document's
//! `plan_schema` admits the slot's `provider_params`, and the plan's
//! `body` is the openai-compatible render — `validate_plan`, endpoint
//! admission (H4), credential bind, and the wire send all run through
//! `hharness_gateway`'s own machinery (`validate_plan`/`EndpointAllowlist::
//! admit`/`CredentialPort::bind`/`Transport::send` →
//! `grammar::decode_frame` → `collect`); nothing in this module
//! re-implements classification, grammar, or framing.

use std::collections::{BTreeMap, BTreeSet};

use hharness_context::retained::{self, RetainedSelection, SelectionVerdict};
use hharness_control::driver::{ModelOutcome, ModelPort};
use hharness_control::output::ParsedCall;
use hharness_control::output::SurfaceSpec;
use hharness_gateway::codec::{self, WireFrame};
use hharness_gateway::credentials::BrokerCredentialPort;
use hharness_gateway::dialect::WireDialect;
use hharness_gateway::gateway::{CredentialHandle, CredentialPort, EndpointAllowlist, Transport};
use hharness_gateway::grammar::{self, DecodeState, ModelEventKind};
use hharness_gateway::http_transport::Family;
use hharness_gateway::message::{ModelBlock, ToolArgs};
use hharness_gateway::plan::{CachePlan, Capabilities, ProviderRequestPlan};
use hharness_gateway::vocab::{ModelError, ModelErrorClass, StopReason};
use hharness_identity::idp::{self, idp_id};
use hharness_ledger::event::EventEnvelope;
use hharness_wire::json::Json;

use hharness_embed_schema::errors::EmbedError;

/// `idp/1` domain for the projection's `context_item_id`s — the content
/// address of the observation payload (`hh.embed.context_item/1`). One
/// home for the assembler's mint (runtime.rs) and this render's join:
/// the retained selection's ids are derived over the same payload
/// bytes, so the join is exact by construction.
pub(crate) const OBSERVATION_ITEM_IDP: &str = "hh.embed.context_item/1";

/// `selected_view(prefix)` — the durable retained view the wire must
/// serialize (R5.20; REQ-E-206; Packet B B.2 "retained-view→wire |
/// gateway + hh-embed render"). The *latest* `context.compaction.
/// completed` `selection` member in the prefix is strict-decoded
/// (`retained_selection/1`) and replay-verified through the R5.18
/// machinery — a malformed record, an unknown/unbound variant, or a
/// recorded selection the production path cannot reproduce is a typed
/// `MalformedHistory` plan refusal (a "strategy label without
/// execution" never dispatches), never a silently-resurrected
/// transcript. `Ok(None)` — the member is absent: the legacy
/// full-view render (fidelity unavailable — no treatment is claimed,
/// and no `view_hash` is stamped). On `Reproduced` the returned pair
/// is the member's own `view_hash` (the idp address of its canonical
/// bytes — the same derivation `prepared::prepare` computes as
/// `view_ref` over the same member) plus the recorded `omitted`
/// `context_item_id`s.
fn selected_view(
    prefix: &[EventEnvelope],
) -> Result<Option<(String, BTreeSet<String>)>, ModelError> {
    let Some(member) = prefix
        .iter()
        .rev()
        .filter(|e| e.class == "context.compaction.completed")
        .find_map(|e| e.payload.get("selection"))
    else {
        return Ok(None);
    };
    let refuse = |detail: String| {
        ModelError::new(
            ModelErrorClass::MalformedHistory,
            format!("retained_selection: {detail}"),
        )
    };
    let sel = RetainedSelection::from_json(member).map_err(|r| refuse(r.to_string()))?;
    match retained::verify_selection(&sel) {
        SelectionVerdict::Reproduced { .. } => {
            let view_hash =
                idp::address(member.to_canonical_string().as_bytes(), "application/json").id();
            Ok(Some((
                view_hash,
                sel.omitted.iter().cloned().collect::<BTreeSet<String>>(),
            )))
        }
        SelectionVerdict::Refused(r) => Err(refuse(r.to_string())),
        SelectionVerdict::Diverged { detail } => Err(refuse(format!("diverged: {detail}"))),
    }
}

/// `ModelLane` — the realized live binding a `model_port` slot declares:
/// the dialect coordinate, the resolved endpoint, the provider model id,
/// and the substitution rule. Persisted verbatim on
/// `manifest.extra["model_lane"]` (a durable-resume re-binds the same
/// lane — never re-derived from a definition that may have drifted).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelLane {
    /// `{dialect_id}@{version}` — the admitted coordinates at R4.5d:
    /// `openai-compatible@1` (local family), `openai@1`, and
    /// `anthropic-messages@1` (the hosted pair — ADR-0366).
    pub dialect_ref: String,
    /// The resolved endpoint base URL (`http(s)://host[:port][/base]`).
    pub endpoint_ref: String,
    /// The provider's model spelling (`HH_LOCAL_MODEL` for the local leg).
    pub provider_model_id: String,
    /// `credential_ref` — the broker credential *coordinate* (`env:NAME` —
    /// the `launch_env` `SecretSource` spelling). Carried literally:
    /// resolving it to a value is the credential port's bind-time work
    /// inside the transport, never this member's (the value lands in no
    /// lane document, manifest member, or ledger row — ADR-0366 D4).
    pub credential_ref: Option<String>,
    /// `substitution_allowed` — a served-model mismatch is a terminal
    /// `served_model_mismatch` error unless declared `true`.
    pub substitution_allowed: bool,
    /// The bound profile coordinate (`model.call.requested` `model_ref`'s
    /// `profile_ref` leg).
    pub profile_ref: String,
    /// Slot-declared `provider_params` (validated against the dialect's
    /// `plan_schema` at arm).
    pub provider_params: BTreeMap<String, Json>,
    /// The R4.5c TLS lane declaration (`tls{}` — file *names*, never
    /// PEM bodies; the manifest re-binds the same lane).
    pub tls: TlsLane,
    /// `transport ∈ {stream, non_stream}` — R6.34 (REQ-XE-107, G5).
    /// `non_stream` decodes through the existing `completion_frames`
    /// decoder — no second decoder (CC1).
    pub transport: PortTransport,
    /// `stream_member ∈ {send, omit}` — the `stream` member's presence
    /// on the wire (REQ-XE-107). `omit` is admissible only under
    /// `non_stream` (a `stream` transport that never declares `stream`
    /// is a self-contradictory declaration — refused at arm).
    pub stream_member: StreamMember,
    /// `tool_choice ∈ {auto, omit}` — whether `tool_choice` is sent
    /// (REQ-XE-106 contributing, G4). `omit` is a declaration, not a
    /// downgrade: the request carries no `tool_choice` member.
    pub tool_choice: ToolChoiceDecl,
    /// `transcript_render{…}` — the declared replay choices (REQ-XE-108;
    /// G6/G7/G19/G20). All-absent ⇒ the legacy render, byte-identical.
    pub transcript_render: TranscriptRender,
    /// `model_surface ∈ {compiled}` — R6.35 (REQ-XE-103/104): the
    /// session arms the definition's compiled `ModelSurface` at open and
    /// the request renders from it (one renderer; absent ⇒
    /// byte-identical to the pre-change path).
    pub model_surface: bool,
}

/// `transport` — the declared request transport (REQ-XE-107).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortTransport {
    /// Streaming (`text/event-stream`) — the pre-R6.34 default.
    #[default]
    Stream,
    /// Non-streaming — the request asks `application/json`; the body
    /// decodes through the family's `completion_frames` (the transport
    /// already reads by content-type — the same decoder, CC1).
    NonStream,
}

/// `stream_member` — the `stream` member's declared presence (G5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamMember {
    /// Send `stream: <transport>` — the pre-R6.34 default.
    #[default]
    Send,
    /// Omit the member entirely (O's recorded shape — whether LiteLLM
    /// sends `stream` at all is unobserved, design 38 F7; the member's
    /// value is fixed from O-recorded bytes).
    Omit,
}

/// `tool_choice` — the declared `tool_choice` handling (G4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolChoiceDecl {
    /// `tool_choice: "auto"` when tools are present — the pre-R6.34
    /// render (the C2 proxy neutralizes it upstream either way, C-30).
    #[default]
    Auto,
    /// Omit `tool_choice` from the request body.
    Omit,
}

/// `transcript_render` — the model port's declared replay members
/// (REQ-XE-108; §7.3 G6/G7/G19/G20). Every member is opt-in: a
/// declaration carrying none renders byte-identically to the legacy
/// path (AC-5's backward-compat golden). Admitted under the
/// chat-completions family only at this stage — the member spellings
/// are chat-completions-shaped; a Messages-family declaration is a
/// typed `InvalidDefinition` refusal, never a silently different
/// replay.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TranscriptRender {
    /// `assistant_replay.content ∈ {drop, verbatim}` — `verbatim`
    /// re-sends the recorded assistant text ("" when none was
    /// recorded); `drop` (default) renders `content: null` (legacy).
    pub replay_content: bool,
    /// `assistant_replay.reasoning ∈ {drop, verbatim}` — `verbatim`
    /// re-sends the recorded `reasoning_content` member (absent when
    /// none was recorded — never a fabricated empty member).
    pub replay_reasoning: bool,
    /// `assistant_replay.extra_members[]` — provider message members
    /// re-sent verbatim when the durable `message_extra` record
    /// preserved them (a recorded-absent member stays absent — the
    /// replay serves only what the blob carried).
    pub extra_members: Vec<String>,
    /// `text_only_turn ∈ {drop, replay}` — `replay` renders a completed
    /// turn that carried no tool calls as its recorded assistant
    /// message; `drop` (default) skips the turn (legacy).
    pub text_only_replay: bool,
    /// `format_error_turn ∈ {kernel_note, omit}` — `omit` drops the
    /// format-error turn's `kernel_note` AND the offending completed
    /// call's assistant turn (mini's bad-turn-omitted leg; the
    /// error-text itself is the `error_format` renderer's —
    /// ADR-0442/R6.35+); `kernel_note` (default) renders both (legacy).
    pub format_error_omit: bool,
    /// `id_normalisation ∈ {kernel, provider_verbatim}` —
    /// `provider_verbatim` renders the provider's call id in both
    /// `tool_calls[].id` and the result's `tool_call_id` (G19).
    pub provider_ids: bool,
    /// `tool_arguments ∈ {canonical, verbatim}` — `verbatim` replays the
    /// argument bytes exactly as the provider sent them (G20), read
    /// from the durable `hh.model_io/1` frames — never re-synthesized.
    pub verbatim_args: bool,
}

impl TranscriptRender {
    /// Whether any declared member needs the durable `hh.model_io/1`
    /// blob to render a `model.call.completed` row. `false` ⇒ the
    /// legacy render (byte-identical, no blob reads).
    pub fn requires_blob(&self) -> bool {
        self.replay_content
            || self.replay_reasoning
            || !self.extra_members.is_empty()
            || self.provider_ids
            || self.verbatim_args
            || self.text_only_replay
    }
    /// `true` when every member holds its default (legacy) value —
    /// the opt-in rule (AC-5).
    pub fn is_default(&self) -> bool {
        *self == TranscriptRender::default()
    }
    /// Canonical JSON — the manifest member (non-default members only).
    fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if self.replay_content || self.replay_reasoning || !self.extra_members.is_empty() {
            let mut ar = BTreeMap::new();
            if self.replay_content {
                ar.insert("content".into(), Json::str("verbatim"));
            }
            if self.replay_reasoning {
                ar.insert("reasoning".into(), Json::str("verbatim"));
            }
            if !self.extra_members.is_empty() {
                ar.insert(
                    "extra_members".into(),
                    Json::Arr(self.extra_members.iter().map(Json::str).collect()),
                );
            }
            m.insert("assistant_replay".into(), Json::Obj(ar));
        }
        if self.text_only_replay {
            m.insert("text_only_turn".into(), Json::str("replay"));
        }
        if self.format_error_omit {
            m.insert("format_error_turn".into(), Json::str("omit"));
        }
        if self.provider_ids {
            m.insert("id_normalisation".into(), Json::str("provider_verbatim"));
        }
        if self.verbatim_args {
            m.insert("tool_arguments".into(), Json::str("verbatim"));
        }
        Json::Obj(m)
    }
}

/// `TlsLane` — the `model_port` slot's `tls{}` declaration: the
/// local-CA extra-roots file and/or the mTLS client identity files.
/// Members resolve like every lane member (`env:NAME` indirection
/// admitted — a path/name is never a secret *value*).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TlsLane {
    /// The explicitly named extra-roots file (PEM CERTIFICATE blocks —
    /// the only path to a private issuer; ADR-0365).
    pub extra_roots_file: Option<String>,
    /// The client certificate chain file (mTLS).
    pub client_cert_file: Option<String>,
    /// The client private-key file (mTLS — required with the chain).
    pub client_key_file: Option<String>,
}

impl TlsLane {
    /// `params(Some(TlsLane))` → the `tls{}` member for `to_json`
    /// (absent ⇒ the member is omitted).
    fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(p) = &self.extra_roots_file {
            m.insert("extra_roots_file".into(), Json::str(p.clone()));
        }
        if let Some(p) = &self.client_cert_file {
            m.insert("client_cert_file".into(), Json::str(p.clone()));
        }
        if let Some(p) = &self.client_key_file {
            m.insert("client_key_file".into(), Json::str(p.clone()));
        }
        Json::Obj(m)
    }

    /// `is_empty()` — the all-absent lane is the webpki-roots default.
    fn is_empty(&self) -> bool {
        self.extra_roots_file.is_none()
            && self.client_cert_file.is_none()
            && self.client_key_file.is_none()
    }

    /// `client_tls()` — arm the `hharness_tls::ClientTls` the lane declares.
    /// Arm-time refusals are typed (`Refused` — the file is gone or
    /// unparseable at drive), never a silently smaller root set.
    pub(crate) fn client_tls(&self) -> Result<hharness_tls::ClientTls, EmbedError> {
        let mut tls = hharness_tls::ClientTls::webpki_roots();
        if let Some(p) = &self.extra_roots_file {
            tls =
                tls.extra_roots_file(std::path::Path::new(p))
                    .map_err(|e| EmbedError::Refused {
                        reason: format!("model_port.tls.extra_roots_file `{p}`: {e}"),
                    })?;
        }
        match (&self.client_cert_file, &self.client_key_file) {
            (Some(c), Some(k)) => {
                let chain = hharness_tls::pem::read_pem(std::path::Path::new(c)).map_err(|e| {
                    EmbedError::Refused {
                        reason: format!("model_port.tls.client_cert_file `{c}`: {e}"),
                    }
                })?;
                let key = hharness_tls::pem::read_pem(std::path::Path::new(k)).map_err(|e| {
                    EmbedError::Refused {
                        reason: format!("model_port.tls.client_key_file `{k}`: {e}"),
                    }
                })?;
                tls = tls
                    .client_identity(&chain, &key)
                    .map_err(|e| EmbedError::Refused {
                        reason: format!("model_port.tls client identity: {e}"),
                    })?;
            }
            (None, None) => {}
            _ => {
                return Err(EmbedError::Refused {
                    reason: "model_port.tls: client_cert_file and client_key_file pair up"
                        .to_string(),
                });
            }
        }
        Ok(tls)
    }
}

impl ModelLane {
    /// Canonical JSON — the manifest's `model_lane` member.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("dialect".into(), Json::str(self.dialect_ref.clone()));
        m.insert("endpoint".into(), Json::str(self.endpoint_ref.clone()));
        m.insert(
            "provider_model_id".into(),
            Json::str(self.provider_model_id.clone()),
        );
        m.insert(
            "substitution_allowed".into(),
            Json::Bool(self.substitution_allowed),
        );
        m.insert("profile_ref".into(), Json::str(self.profile_ref.clone()));
        if let Some(cr) = &self.credential_ref {
            m.insert("credential_ref".into(), Json::str(cr.clone()));
        }
        if !self.tls.is_empty() {
            m.insert("tls".into(), self.tls.to_json());
        }
        if !self.provider_params.is_empty() {
            m.insert(
                "provider_params".into(),
                Json::Obj(self.provider_params.clone()),
            );
        }
        // R6.34 members serialize only when non-default — a lane that
        // declares none persists byte-identically (the opt-in rule).
        if self.transport == PortTransport::NonStream {
            m.insert("transport".into(), Json::str("non_stream"));
        }
        if self.stream_member == StreamMember::Omit {
            m.insert("stream_member".into(), Json::str("omit"));
        }
        if self.tool_choice == ToolChoiceDecl::Omit {
            m.insert("tool_choice".into(), Json::str("omit"));
        }
        if !self.transcript_render.is_default() {
            m.insert("transcript_render".into(), self.transcript_render.to_json());
        }
        if self.model_surface {
            m.insert("model_surface".into(), Json::str("compiled"));
        }
        Json::Obj(m)
    }

    /// The manifest member's decode — absent ⇒ `None` (the scripted lane).
    pub fn from_manifest(manifest: &hharness_ledger::manifest::RunManifest) -> Option<ModelLane> {
        let j = manifest.extra.get("model_lane")?;
        Some(ModelLane {
            dialect_ref: j.get("dialect")?.as_str()?.to_string(),
            endpoint_ref: j.get("endpoint")?.as_str()?.to_string(),
            provider_model_id: j.get("provider_model_id")?.as_str()?.to_string(),
            credential_ref: j
                .get("credential_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            substitution_allowed: matches!(j.get("substitution_allowed"), Some(Json::Bool(true))),
            profile_ref: j
                .get("profile_ref")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            provider_params: match j.get("provider_params") {
                Some(Json::Obj(m)) => m.clone(),
                _ => BTreeMap::new(),
            },
            tls: match j.get("tls") {
                Some(Json::Obj(t)) => TlsLane {
                    extra_roots_file: t
                        .get("extra_roots_file")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                    client_cert_file: t
                        .get("client_cert_file")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                    client_key_file: t
                        .get("client_key_file")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                },
                _ => TlsLane::default(),
            },
            transport: match j.get("transport").and_then(Json::as_str) {
                Some("non_stream") => PortTransport::NonStream,
                _ => PortTransport::Stream,
            },
            stream_member: match j.get("stream_member").and_then(Json::as_str) {
                Some("omit") => StreamMember::Omit,
                _ => StreamMember::Send,
            },
            tool_choice: match j.get("tool_choice").and_then(Json::as_str) {
                Some("omit") => ToolChoiceDecl::Omit,
                _ => ToolChoiceDecl::Auto,
            },
            transcript_render: match j.get("transcript_render") {
                Some(tr) => decode_transcript_render(tr).unwrap_or_default(),
                None => TranscriptRender::default(),
            },
            model_surface: j.get("model_surface").and_then(Json::as_str) == Some("compiled"),
        })
    }
}

/// Resolve a slot member that is either a literal or `env:NAME` — the env
/// form is the sanctioned indirection for provisioned names
/// (`HH_LOCAL_BASE_URL`/`HH_LOCAL_MODEL` — `provided: yes/no` only; the
/// *value* never lands in a definition or ledger, only the resolved
/// coordinate does).
fn resolve_value(member: &str, v: Option<&Json>) -> Result<String, EmbedError> {
    let s = v
        .and_then(Json::as_str)
        .ok_or_else(|| EmbedError::InvalidDefinition {
            diagnostics: vec![format!("model_port.{member} is required (string)")],
        })?;
    if let Some(name) = s.strip_prefix("env:") {
        return std::env::var(name).map_err(|_| EmbedError::EnvironmentUnavailable {
            reason: format!("model_port.{member}: env {name} unset"),
        });
    }
    Ok(s.to_string())
}

/// The admitted `model_port.dialect` coordinates at R4.5d — the local
/// family plus the two hosted dialects (ADR-0366).
const ADMITTED_DIALECTS: [&str; 3] = [
    hharness_gateway::dialects::OPENAI_COMPATIBLE_ID,
    hharness_gateway::dialects::OPENAI_ID,
    hharness_gateway::dialects::ANTHROPIC_MESSAGES_ID,
];

/// `dialect_by_id(ref)` — bind a dialect coordinate to its document. An
/// unknown coordinate answers `None` (the caller names it
/// `InvalidDefinition` — never a silent fallback to another dialect).
pub(crate) fn dialect_by_id(dialect_ref: &str) -> Option<WireDialect> {
    match dialect_ref {
        hharness_gateway::dialects::OPENAI_COMPATIBLE_ID => {
            Some(hharness_gateway::dialects::openai_compatible())
        }
        hharness_gateway::dialects::OPENAI_ID => Some(hharness_gateway::dialects::openai()),
        hharness_gateway::dialects::ANTHROPIC_MESSAGES_ID => {
            Some(hharness_gateway::dialects::anthropic_messages())
        }
        _ => None,
    }
}

/// `decode_model_lane(params, profile_ref)` — the `model_port` slot's
/// `params` → the lane. Typed refusals at arm, never silent defaults:
/// unknown dialect, missing members, non-`http(s):` endpoints, an
/// unpaired or unknown-keyed `tls{}` member (R4.5c — `tls_pending`
/// retired), a malformed or unset `credential_ref` coordinate (R4.5d —
/// coordinate checked, value never touched), and `provider_params`
/// outside the dialect's `plan_schema.allowed_keys`.
pub fn decode_model_lane(params: &Json, profile_ref: &str) -> Result<ModelLane, EmbedError> {
    let dialect_ref = params
        .get("dialect")
        .and_then(Json::as_str)
        .unwrap_or(hharness_gateway::dialects::OPENAI_COMPATIBLE_ID)
        .to_string();
    let dialect = dialect_by_id(&dialect_ref).ok_or_else(|| EmbedError::InvalidDefinition {
        diagnostics: vec![format!(
            "model_port.dialect `{dialect_ref}` unknown — admitted: {}",
            ADMITTED_DIALECTS.join(", ")
        )],
    })?;
    let endpoint_ref = resolve_value("endpoint", params.get("endpoint"))?;
    if !(endpoint_ref.starts_with("http://") || endpoint_ref.starts_with("https://")) {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec![format!(
                "model_port.endpoint `{endpoint_ref}` — `http(s)://host[:port][/base]` required"
            )],
        });
    }
    // `tls{}` — the R4.5c lane declaration: extra-roots file and/or
    // mTLS client identity. Members resolve through the sanctioned
    // `env:NAME` indirection; unknown keys and an unpaired
    // cert/key are typed refusals at arm (no silent defaults).
    let mut tls = TlsLane::default();
    match params.get("tls") {
        None | Some(Json::Null) => {}
        Some(Json::Obj(m)) => {
            for k in m.keys() {
                if !matches!(
                    k.as_str(),
                    "extra_roots_file" | "client_cert_file" | "client_key_file"
                ) {
                    return Err(EmbedError::InvalidDefinition {
                        diagnostics: vec![format!("model_port.tls key `{k}` unknown")],
                    });
                }
            }
            tls.extra_roots_file = match m.get("extra_roots_file") {
                Some(v) => Some(resolve_value("tls.extra_roots_file", Some(v))?),
                None => None,
            };
            tls.client_cert_file = match m.get("client_cert_file") {
                Some(v) => Some(resolve_value("tls.client_cert_file", Some(v))?),
                None => None,
            };
            tls.client_key_file = match m.get("client_key_file") {
                Some(v) => Some(resolve_value("tls.client_key_file", Some(v))?),
                None => None,
            };
            match (&tls.client_cert_file, &tls.client_key_file) {
                (Some(_), None) | (None, Some(_)) => {
                    return Err(EmbedError::InvalidDefinition {
                        diagnostics: vec![
                            "model_port.tls: client_cert_file and client_key_file pair up"
                                .to_string(),
                        ],
                    });
                }
                _ => {}
            }
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.tls is not an object".to_string()],
            });
        }
    }
    // `credential_ref` — the hosted families' key coordinate, a *name*
    // never a value. Two lanes admitted: absent (the local family's
    // `AuthKind::None` path) or `env:NAME`. The `env:` prefix is the
    // `launch_env` SecretSource spelling — the arm checks the coordinate's
    // shape and the name's presence (`EnvironmentUnavailable` before the
    // run, never a mid-call surprise), but the *value* is not resolved
    // here — that is `BrokerCredentialPort::bind`'s work at call time,
    // and only the transport reads it. A non-`env:` coordinate refuses
    // `InvalidDefinition` (R4.5d admits `launch_env` only — keyring/
    // vault sources are future lanes, not silent defaults).
    let credential_ref = match params.get("credential_ref") {
        None | Some(Json::Null) => None,
        Some(Json::Str(s)) => {
            let name = s
                .strip_prefix("env:")
                .ok_or_else(|| EmbedError::InvalidDefinition {
                    diagnostics: vec![format!(
                        "model_port.credential_ref `{s}` — `env:NAME` coordinate required"
                    )],
                })?;
            if name.is_empty() {
                return Err(EmbedError::InvalidDefinition {
                    diagnostics: vec![
                        "model_port.credential_ref `env:` names an empty variable".to_string()
                    ],
                });
            }
            if std::env::var_os(name).is_none() {
                return Err(EmbedError::EnvironmentUnavailable {
                    reason: format!("model_port.credential_ref: env {name} unset"),
                });
            }
            Some(s.clone())
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.credential_ref is not a string".to_string()],
            })
        }
    };
    let provider_model_id = resolve_value("provider_model_id", params.get("provider_model_id"))?;
    if provider_model_id.is_empty() {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec!["model_port.provider_model_id resolves empty".to_string()],
        });
    }
    let provider_params = match params.get("provider_params") {
        Some(Json::Obj(m)) => m.clone(),
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.provider_params is not an object".to_string()],
            })
        }
        None => BTreeMap::new(),
    };
    for k in provider_params.keys() {
        if !dialect.plan_schema.admits(k) {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "model_port.provider_params key `{k}` outside dialect plan_schema"
                )],
            });
        }
    }
    // R6.34 — the opt-in wire-fidelity members (REQ-XE-106/107/108).
    // Absent ⇒ defaults ⇒ the render is byte-identical to the legacy
    // lane (AC-5). Unknown members/values refuse typed, never silently
    // coerce.
    let transport = match params.get("transport") {
        None | Some(Json::Null) => PortTransport::Stream,
        Some(Json::Str(s)) if s == "stream" => PortTransport::Stream,
        Some(Json::Str(s)) if s == "non_stream" => PortTransport::NonStream,
        Some(Json::Str(s)) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "model_port.transport `{s}` — admitted: stream, non_stream"
                )],
            })
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.transport is not a string".to_string()],
            })
        }
    };
    let stream_member = match params.get("stream_member") {
        None | Some(Json::Null) => StreamMember::Send,
        Some(Json::Str(s)) if s == "send" => StreamMember::Send,
        Some(Json::Str(s)) if s == "omit" => StreamMember::Omit,
        Some(Json::Str(s)) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "model_port.stream_member `{s}` — admitted: send, omit"
                )],
            })
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.stream_member is not a string".to_string()],
            })
        }
    };
    if transport == PortTransport::Stream && stream_member == StreamMember::Omit {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec![
                "model_port.stream_member `omit` under `transport: stream` — the declaration \
                 contradicts itself (the wire would carry no `stream` member on a streaming lane)"
                    .to_string(),
            ],
        });
    }
    let tool_choice = match params.get("tool_choice") {
        None | Some(Json::Null) => ToolChoiceDecl::Auto,
        Some(Json::Str(s)) if s == "auto" => ToolChoiceDecl::Auto,
        Some(Json::Str(s)) if s == "omit" => ToolChoiceDecl::Omit,
        Some(Json::Str(s)) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "model_port.tool_choice `{s}` — admitted: auto, omit"
                )],
            })
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.tool_choice is not a string".to_string()],
            })
        }
    };
    let model_surface = match params.get("model_surface") {
        None | Some(Json::Null) => false,
        Some(Json::Str(s)) if s == "compiled" => true,
        Some(Json::Str(s)) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "model_port.model_surface `{s}` — admitted: compiled"
                )],
            })
        }
        Some(_) => {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: vec!["model_port.model_surface is not a string".to_string()],
            })
        }
    };
    if model_surface && hharness_gateway::family_of(&dialect) != Family::ChatCompletions {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec![
                "model_port.model_surface `compiled` is admitted under the chat-completions \
                 family only at this stage"
                    .to_string(),
            ],
        });
    }
    let transcript_render = match params.get("transcript_render") {
        None | Some(Json::Null) => TranscriptRender::default(),
        Some(tr) => decode_transcript_render(tr)?,
    };
    // The `transcript_render` spellings are chat-completions-shaped at
    // this stage (`reasoning_content`, `tool_calls[].id`,
    // `tool_call_id`) — a Messages-family lane declaring them is a
    // typed refusal, never a silently different replay (R6.34).
    if !transcript_render.is_default()
        && hharness_gateway::family_of(&dialect) != Family::ChatCompletions
    {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec![
                "model_port.transcript_render is admitted under the chat-completions family \
                 only at this stage"
                    .to_string(),
            ],
        });
    }
    Ok(ModelLane {
        dialect_ref,
        endpoint_ref,
        provider_model_id,
        credential_ref,
        substitution_allowed: matches!(params.get("substitution_allowed"), Some(Json::Bool(true))),
        profile_ref: profile_ref.to_string(),
        provider_params,
        tls,
        transport,
        stream_member,
        tool_choice,
        transcript_render,
        model_surface,
    })
}

/// `decode_transcript_render(j)` — the `transcript_render{…}` sub-object's
/// strict decode: unknown members and undeclared values refuse typed.
fn decode_transcript_render(j: &Json) -> Result<TranscriptRender, EmbedError> {
    let bad = |d: &str| EmbedError::InvalidDefinition {
        diagnostics: vec![format!("model_port.transcript_render: {d}")],
    };
    let Json::Obj(m) = j else {
        return Err(bad("not an object"));
    };
    let mut tr = TranscriptRender::default();
    let str_member = |o: &Json, key: &str, vals: &[&str]| -> Result<Option<String>, EmbedError> {
        match o.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Str(s)) if vals.contains(&s.as_str()) => Ok(Some(s.clone())),
            Some(Json::Str(s)) => Err(bad(&format!("{key} `{s}` — admitted: {}", vals.join(", ")))),
            Some(_) => Err(bad(&format!("{key} is not a string"))),
        }
    };
    for (k, v) in m {
        match k.as_str() {
            "assistant_replay" => {
                let Json::Obj(ar) = v else {
                    return Err(bad("assistant_replay is not an object"));
                };
                for (ak, _) in ar {
                    match ak.as_str() {
                        "content" => {
                            tr.replay_content = str_member(v, ak, &["drop", "verbatim"])?.as_deref()
                                == Some("verbatim")
                        }
                        "reasoning" => {
                            tr.replay_reasoning = str_member(v, ak, &["drop", "verbatim"])?
                                .as_deref()
                                == Some("verbatim")
                        }
                        "extra_members" => match ar.get(ak) {
                            Some(Json::Arr(xs)) => {
                                for x in xs {
                                    match x {
                                        Json::Str(s) => tr.extra_members.push(s.clone()),
                                        _ => return Err(bad("extra_members[] is not a string")),
                                    }
                                }
                            }
                            _ => return Err(bad("extra_members is not an array")),
                        },
                        other => {
                            return Err(bad(&format!(
                                "assistant_replay.{other} unknown — admitted: content, \
                                 reasoning, extra_members"
                            )))
                        }
                    }
                }
            }
            "text_only_turn" => {
                tr.text_only_replay =
                    str_member(j, k, &["drop", "replay"])?.as_deref() == Some("replay")
            }
            "format_error_turn" => {
                tr.format_error_omit =
                    str_member(j, k, &["kernel_note", "omit"])?.as_deref() == Some("omit")
            }
            "id_normalisation" => {
                tr.provider_ids = str_member(j, k, &["kernel", "provider_verbatim"])?.as_deref()
                    == Some("provider_verbatim")
            }
            "tool_arguments" => {
                tr.verbatim_args =
                    str_member(j, k, &["canonical", "verbatim"])?.as_deref() == Some("verbatim")
            }
            other => {
                return Err(bad(&format!(
                    "{other} unknown — admitted: assistant_replay, text_only_turn, \
                     format_error_turn, id_normalisation, tool_arguments"
                )))
            }
        }
    }
    Ok(tr)
}

/// `model_port_slot_params(doc)` — the `native.slots["model_port"]`
/// declaration (same read discipline as `router_slot_params`).
pub fn model_port_slot_params(doc: &hharness_hir::document::HirDocument) -> Option<Json> {
    use hharness_hir::records::{AgentProcessBody, KindRecord, SlotBindings};
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                match np.slots.get("model_port") {
                    Some(SlotBindings::One(b)) => return Some(Json::Obj(b.params.clone())),
                    Some(SlotBindings::Many(v)) => {
                        return v.first().map(|b| Json::Obj(b.params.clone()))
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// `control_strategy_params(doc)` — the `native.slots["control_strategy"]`
/// binding's params (same read discipline as `model_port_slot_params`).
pub fn control_strategy_params(doc: &hharness_hir::document::HirDocument) -> Option<Json> {
    use hharness_hir::records::{AgentProcessBody, KindRecord, SlotBindings};
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                match np.slots.get("control_strategy") {
                    Some(SlotBindings::One(b)) => return Some(Json::Obj(b.params.clone())),
                    Some(SlotBindings::Many(v)) => {
                        return v.first().map(|b| Json::Obj(b.params.clone()))
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// `completion_surface_arm(doc)` — the REQ-XE-106 (G4) declaration read
/// for the compiled-surface path: `hh.submit` renders in `tools[]` only
/// when the definition declares `completion_surface = "hh.submit"` or
/// its `stop_rule` requires a claim surface — `no_action`/`either`,
/// since `submit` is the marker rule (REQ-XE-116's family), which needs
/// no call surface. An absent declaration derives from `stop_rule`;
/// an absent `stop_rule` is the variant preset (`submit` — the only
/// value today's armed `StrategyParams` produce). Unknown members/
/// spellings refuse typed, never silently coerce.
pub fn completion_surface_arm(
    doc: &hharness_hir::document::HirDocument,
) -> Result<bool, EmbedError> {
    let bad = |d: String| EmbedError::InvalidDefinition {
        diagnostics: vec![d],
    };
    let Some(Json::Obj(params)) = control_strategy_params(doc) else {
        // An unbound slot carries no params — the Stage-1 default
        // variant's preset (`stop_rule = submit`) needs no surface.
        return Ok(false);
    };
    match params.get("completion_surface") {
        None | Some(Json::Null) => {}
        Some(Json::Str(s)) if s == "none" => return Ok(false),
        Some(Json::Str(s)) if s == "hh.submit" => return Ok(true),
        Some(Json::Str(s)) => {
            return Err(bad(format!(
                "control_strategy.completion_surface `{s}` — admitted: none, hh.submit"
            )))
        }
        Some(_) => {
            return Err(bad(
                "control_strategy.completion_surface is not a string".to_string()
            ))
        }
    }
    match params.get("stop_rule") {
        None | Some(Json::Null) => Ok(false),
        Some(Json::Str(s)) => match s.as_str() {
            "submit" => Ok(false),
            "no_action" | "either" => Ok(true),
            other => Err(bad(format!(
                "control_strategy.stop_rule `{other}` — admitted: submit, no_action, either"
            ))),
        },
        // The amended object form `stop_rule = {submit: {marker, …}}`
        // (REQ-XE-116's family) — read the class head only; the member
        // body's decode is the control ticket's own scope.
        Some(Json::Obj(m)) => {
            if m.contains_key("submit") {
                Ok(false)
            } else if m.contains_key("no_action") || m.contains_key("either") {
                Ok(true)
            } else {
                Err(bad(
                    "control_strategy.stop_rule object names no admitted class".to_string(),
                ))
            }
        }
        Some(_) => Err(bad(
            "control_strategy.stop_rule is not a string or object".to_string()
        )),
    }
}

/// `ArmedSurface` — the session's compiled `ModelSurface` plus the
/// definition-derived render decision that rides it (R6.36; REQ-XE-106):
/// `completion_armed` is the `hh.submit` `tools[]` arm, computed at arm
/// and re-derived by the same read on resume (the artifact is never
/// persisted — like the surface itself, CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmedSurface {
    /// The compiled `ModelSurface`.
    pub surface: hharness_compiler::seal::ModelSurface,
    /// `hh.submit` renders in `tools[]` — armed only when the definition
    /// declares it or its `stop_rule` requires a claim surface.
    pub completion_armed: bool,
}

// ── the port ─────────────────────────────────────────────────────────────────

/// One staged plan — built at `requested_members`, consumed by `call`.
struct PendingPlan {
    plan: ProviderRequestPlan,
    /// `wire name → surface_id` — the request's own tool-name map
    /// (`wire_name` is not injective; the map is the honest inverse).
    wire_to_surface: BTreeMap<String, String>,
    /// The request body (re-recorded on the `model_io` blob).
    body: Json,
    /// The `view_hash` member `model.call.requested` carries when a
    /// durable `retained_selection/1` informed this render — the
    /// consumed member's own idp address (`None` ⇒ no selected view
    /// informed the call; fidelity unavailable, never a treatment
    /// claim).
    view_hash: Option<String>,
    /// The bound credential — `binding_id` + `provided` only, resolved at
    /// `requested_members` and handed to the transport at `call`. The
    /// handle never carries material (ADR-0366 D4).
    credential: CredentialHandle,
}

/// `GatewayModel` — the gateway-backed `ModelPort` (R4.5b). One instance
/// per `drive`; the armed lane + dialect are fixed at construction.
pub struct GatewayModel {
    /// The armed lane.
    lane: ModelLane,
    /// The bound dialect document.
    dialect: WireDialect,
    /// The HTTP transport (plain or TLS-terminated — the lane's `tls{}`
    /// declaration armed at construction; R4.5c). Auth material is
    /// injected here and only here (R4.5d — ADR-0366 D4).
    transport: hharness_gateway::http_transport::HttpModelTransport,
    /// The credential port — the broker-backed bind for every family.
    /// `credential_ref: None` answers `provided: no` (the local lane's
    /// `AuthKind::None` contract); `env:NAME` resolves through the
    /// `launch_env` source — a missing name is a `credential_unavailable`
    /// failure, never a silently-unauthenticated send.
    credentials: BrokerCredentialPort,
    /// The provider family the dialect maps to — selects the request
    /// render, the SSE translator, and the wire path.
    family: Family,
    /// The armed endpoint admission set (the bound endpoint, only).
    endpoints: EndpointAllowlist,
    /// The `normalizer_ref` stamped on usage vectors.
    normalizer_ref: String,
    /// The staged instruction (the run's `input` text — what the scripted
    /// lane staged as `completion`).
    instruction: String,
    /// The staged invoke (`{kind:"invoke"}` input — a directed call).
    invoke: Option<(String, Json)>,
    /// The armed surfaces (the `tools[]` render's source).
    surfaces: Vec<SurfaceSpec>,
    /// Plans built at `requested_members`, keyed `model_call_id`.
    pending: BTreeMap<String, PendingPlan>,
    /// A plan-build refusal staged at `requested_members` — `call`
    /// answers it typed before the wire is touched.
    plan_error: Option<(String, ModelError)>,
    /// Terminal members `call` staged — drained by `terminal_members`.
    terminal: BTreeMap<String, Vec<(&'static str, Json)>>,
    /// `hh.model_io/1` blobs `(media, bytes)` — the driver deposits them.
    model_io: Vec<(String, Vec<u8>)>,
    /// The last completed message's text — `finish_call`'s Completed arm
    /// records it (the driver reads the blob; `one_shot` callers get it
    /// here). `None` on a failed/refused call.
    last_text: Option<String>,
    /// The session's compiled `ModelSurface` (R6.35; REQ-XE-103) —
    /// `Some` ⇒ the request renders layout sections + the shared
    /// prior-turn fold through this one surface; `None` ⇒ byte-identical
    /// to the pre-change path.
    surface: Option<hharness_compiler::seal::ModelSurface>,
    /// The `hh.submit` `tools[]` arm (R6.36; REQ-XE-106) — meaningful
    /// only under the armed path: `true` ⇒ the completion surface's
    /// entry follows the compiled `tools[]` (the definition declared it
    /// or its `stop_rule` requires a claim surface). `false` ⇒ no
    /// `hh_submit` member — the marker-rule shape.
    completion_armed: bool,
}

impl GatewayModel {
    /// `new(lane, instruction, invoke, surfaces)` — the armed lane's
    /// dialect is the document `dialect_ref` names (any other ref refused
    /// at `decode_model_lane`; a mismatch here is a construction fault).
    /// The lane's `tls{}` members arm the client TLS config — a missing
    /// or unparseable file is a typed `Refused` at drive, never a
    /// silently-weak session (R4.5c; ADR-0365).
    pub fn new(
        lane: ModelLane,
        instruction: String,
        invoke: Option<(String, Json)>,
        surfaces: Vec<SurfaceSpec>,
    ) -> Result<GatewayModel, EmbedError> {
        let dialect = dialect_by_id(&lane.dialect_ref).ok_or_else(|| EmbedError::Refused {
            reason: format!("model_port: dialect `{}` unbindable", lane.dialect_ref),
        })?;
        let family = hharness_gateway::family_of(&dialect);
        let client_tls = lane.tls.client_tls()?;
        let transport = hharness_gateway::http_transport::HttpModelTransport::with_tls(
            dialect.clone(),
            client_tls,
        );
        let mut endpoints = EndpointAllowlist {
            policy_ref: dialect.endpoint_allowlist_ref.clone(),
            ..Default::default()
        };
        endpoints.endpoints.insert(lane.endpoint_ref.clone());
        // The `usage_mapping` ref stamped on every emitted vector —
        // the bound dialect's coordinate (R4.5b's spelling; the hosted
        // families stamp their own).
        let normalizer_ref = lane.dialect_ref.clone();
        Ok(GatewayModel {
            lane,
            dialect,
            transport,
            credentials: BrokerCredentialPort::default(),
            family,
            endpoints,
            normalizer_ref,
            instruction,
            invoke,
            surfaces,
            pending: BTreeMap::new(),
            plan_error: None,
            terminal: BTreeMap::new(),
            model_io: Vec::new(),
            last_text: None,
            surface: None,
            completion_armed: false,
        })
    }

    /// `with_surface(surface)` — arm the compiled-`ModelSurface` render
    /// (REQ-XE-103): the port the session's `model_port.model_surface =
    /// "compiled"` arm produces. The sections' declared `hh-tmpl/1`
    /// texts fill at render time — `task` from the instruction and the
    /// `uname.*` family from the ledgered in-cell probe rows
    /// (REQ-XE-113); an unfilled slot is a typed plan refusal before
    /// any wire send.
    pub fn with_surface(mut self, surface: hharness_compiler::seal::ModelSurface) -> Self {
        self.surface = Some(surface);
        self
    }

    /// `with_completion_arm(armed)` — the REQ-XE-106 completion-surface
    /// decision the session computed from the definition
    /// (`completion_surface_arm`): `true` ⇒ `hh.submit` renders in
    /// `tools[]` under the compiled-surface path.
    pub fn with_completion_arm(mut self, armed: bool) -> Self {
        self.completion_armed = armed;
        self
    }

    /// Whether the compiled-surface path owns this port's render
    /// (the single-path pin — armed sessions never fall through).
    pub fn surface_armed(&self) -> bool {
        self.surface.is_some()
    }

    /// `text_msg(role, text)` — a family-rendered plain-text turn.
    /// Both families' user/kernel-note messages carry one text body; the
    /// Messages spelling wraps it in a `text` content block.
    fn text_msg(&self, role: &str, text: &str) -> Json {
        match self.family {
            Family::Messages => hharness_gateway::anthropic::text_msg(role, text),
            Family::ChatCompletions => hharness_gateway::openai::text_msg(role, text),
        }
    }

    /// `assistant_tool_msg(calls)` — the prior assistant turn carrying
    /// the tool calls (`(id, wire_name, args_raw)` — the args are the
    /// provider's own argument spelling, verbatim). Chat-completions
    /// renders `tool_calls[]`; Messages renders `tool_use` content blocks.
    /// Prior-turn text is not carried — the completed row's `calls[]`
    /// member is the pair source, matching the family's tool-result
    /// pairing contract (the wire id is consistent within the body).
    fn assistant_tool_msg(&self, calls: &[(String, String, String)]) -> Json {
        match self.family {
            Family::Messages => hharness_gateway::anthropic::assistant_tool_use_msg(calls),
            Family::ChatCompletions => {
                hharness_gateway::openai::assistant_tool_call_msg(None, calls)
            }
        }
    }

    /// `wire_call_id(id)` — the kernel's `tool_call_id` rendered into the
    /// family's call-id alphabet. Chat Completions passes it verbatim;
    /// Messages lowers non-conforming chars (the kernel's `mc.t.tc0`
    /// spelling carries dots the `tool_use.id` alphabet refuses) — the
    /// render is deterministic so `tool_use.id`/`tool_result.tool_use_id`
    /// pair inside the body.
    fn wire_call_id(&self, id: &str) -> String {
        match self.family {
            Family::Messages => hharness_gateway::anthropic::wire_call_id(id),
            Family::ChatCompletions => id.to_string(),
        }
    }

    /// `tool_result_msg(id, content)` — the observation the kernel
    /// recorded for one tool call: `role:"tool"` under chat-completions,
    /// a `user` turn carrying a `tool_result` block under Messages.
    fn tool_result_msg(&self, id: &str, content: Json) -> Json {
        match self.family {
            Family::Messages => hharness_gateway::anthropic::tool_result_msg(id, content),
            Family::ChatCompletions => hharness_gateway::openai::tool_result_msg(id, content),
        }
    }

    /// `tool_entry(name, desc, schema)` — one `tools[]` member:
    /// `{type:"function", function:{…}}` under chat-completions,
    /// `{name, description, input_schema}` under Messages.
    fn tool_entry(&self, name: &str, desc: &str, schema: Json) -> Json {
        match self.family {
            Family::Messages => hharness_gateway::anthropic::tool_entry(name, desc, schema),
            Family::ChatCompletions => hharness_gateway::openai::tool_entry(name, desc, schema),
        }
    }

    /// `surface_head(surface, prefix)` — the compiled `ModelSurface`'s
    /// layout preamble (REQ-XE-103/104): each section renders under its
    /// declared provider role (`system` included) — an `hh-tmpl/1`
    /// grammar section's `text` is the template source, filled at render
    /// time from `{task: <instruction>}` and the durable
    /// `action.environment.probe.completed` `values` (`uname.*` slots —
    /// REQ-XE-113's ledgered fill). A declared but unfilled slot is a
    /// typed `InvalidRequest{template_slot_unfilled}` — the plan fails
    /// before any wire send, never an empty string. Sections are emitted
    /// in profile-declared `order`; a section whose `slots` name
    /// `transcript` marks the prior-turn fold's splice point (absent ⇒
    /// the fold appends after the last section).
    fn surface_head(
        &self,
        surface: &hharness_compiler::seal::ModelSurface,
        prefix: &[EventEnvelope],
    ) -> Result<Vec<Json>, ModelError> {
        // The run-time fills — `task` is the instruction bytes; every
        // other variable resolves from the latest completed probe row's
        // `values` (`uname.*`, tried verbatim then under the `uname.`
        // namespace the REQ-XE-113 spellings use).
        let mut vars: BTreeMap<String, String> = BTreeMap::new();
        let task = match &self.invoke {
            Some((cap, args)) => format!(
                "The caller directed: invoke `{cap}` with arguments {}",
                args.to_canonical_string()
            ),
            None => self.instruction.clone(),
        };
        vars.insert("task".to_string(), task);
        if let Some(row) = prefix
            .iter()
            .rev()
            .find(|e| e.class == "action.environment.probe.completed")
        {
            if let Some(Json::Obj(vs)) = row.payload.get("values") {
                for (k, v) in vs {
                    if let Some(s) = v.as_str() {
                        vars.insert(k.clone(), s.to_string());
                        if let Some(short) = k.strip_prefix("uname.") {
                            vars.entry(short.to_string())
                                .or_insert_with(|| s.to_string());
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        for section in &surface.layout {
            if section.slots.iter().any(|s| s == "transcript") {
                continue; // the prior-turn fold splices after the sections
            }
            let role = section.role.as_deref().unwrap_or("user");
            let src = section.text.clone().unwrap_or_default();
            let content = match section.grammar.as_deref() {
                Some("hh-tmpl/1") => {
                    let t = hharness_compiler::tmpl::compile_template(&src).map_err(|e| {
                        ModelError::new(
                            ModelErrorClass::InvalidRequest,
                            format!("surface section `{}`: {e}", section.section_id),
                        )
                    })?;
                    t.render(&vars).map_err(|e| {
                        ModelError::new(
                            ModelErrorClass::InvalidRequest,
                            format!("template_slot_unfilled:{e}"),
                        )
                    })?
                }
                _ => src,
            };
            out.push(self.text_msg(role, &content));
        }
        Ok(out)
    }

    /// `request_body(messages, tools, params)` — the plan's `body`: the
    /// family request render over the *effective* `provider_params` (the
    /// same map the plan records — `build_plan` applies the family's own
    /// augmentations before this call). The renders are the family's own
    /// `provider_params` contract: they merge verbatim and the family's
    /// `plan_schema` bounds the set.
    fn request_body(
        &self,
        messages: &[Json],
        tools: &[Json],
        params: &BTreeMap<String, Json>,
    ) -> Json {
        let stream = self.lane.transport == PortTransport::Stream;
        let mut body = match self.family {
            Family::Messages => hharness_gateway::anthropic::messages_request_body(
                &self.lane.provider_model_id,
                messages,
                tools,
                stream,
                params,
            ),
            Family::ChatCompletions => hharness_gateway::openai::chat_request_body(
                &self.lane.provider_model_id,
                messages,
                tools,
                stream,
                params,
            ),
        };
        // R6.34 — the declared member-presence choices (REQ-XE-107's
        // `stream` member; REQ-XE-106's `tool_choice`): absent
        // declarations leave the family's default bytes untouched.
        if let Json::Obj(m) = &mut body {
            if self.lane.stream_member == StreamMember::Omit {
                m.remove("stream");
            }
            if self.lane.tool_choice == ToolChoiceDecl::Omit {
                m.remove("tool_choice");
            }
        }
        body
    }

    /// `recorded_assistant(payload, blobs, leg)` — resolve a
    /// `model.call.completed` row's `response_ref` through the blob seam
    /// and fold the durable `hh.model_io/1` frames into the assistant
    /// turn's verbatim record (REQ-XE-108; X-04 — replayed bytes come
    /// from the blob, never re-synthesized). An absent/unparseable ref
    /// or an unresolvable blob is `MalformedHistory` — a declared
    /// fidelity the durable record cannot serve is refused, never
    /// fabricated.
    fn recorded_assistant(
        &self,
        payload: &Json,
        blobs: &dyn Fn(&str) -> Option<Vec<u8>>,
        leg: &str,
    ) -> Result<RecordedAssistant, ModelError> {
        let rref = payload
            .get("response_ref")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let bytes = blobs(rref).ok_or_else(|| {
            ModelError::new(
                ModelErrorClass::MalformedHistory,
                format!("transcript_render.{leg}: response_ref `{rref}` unresolved"),
            )
        })?;
        let doc =
            hharness_wire::json::parse_provider(&String::from_utf8_lossy(&bytes)).map_err(|e| {
                ModelError::new(
                    ModelErrorClass::MalformedHistory,
                    format!("transcript_render.{leg}: blob not JSON ({e})"),
                )
            })?;
        Ok(fold_assistant_frames(&doc))
    }

    /// `replay_assistant_msg(rec, calls)` — render one recorded
    /// assistant turn under the declared `transcript_render` members
    /// (chat-completions shape; Messages-family declarations refuse at
    /// arm). `calls` is the already-rendered `(id, name, args)` triple.
    fn replay_assistant_msg(
        &self,
        rec: &RecordedAssistant,
        calls: &[(String, String, String)],
    ) -> Json {
        let tr = &self.lane.transcript_render;
        let mut extra = BTreeMap::new();
        if tr.replay_reasoning && !rec.reasoning.is_empty() {
            extra.insert("reasoning_content".into(), Json::str(rec.reasoning.clone()));
        }
        for name in &tr.extra_members {
            if let Some(v) = rec.extra.get(name) {
                extra.insert(name.clone(), v.clone());
            }
        }
        let content = if tr.replay_content {
            Some(rec.text.as_str())
        } else {
            None
        };
        hharness_gateway::openai::assistant_msg(content, &extra, calls)
    }

    /// The transcript render — instruction + recorded rounds from the
    /// durable prefix, restricted to the durable retained view (R5.20;
    /// REQ-E-206): the latest `retained_selection/1` is decoded +
    /// replay-verified and its `omitted` items never re-enter the
    /// render — the wire serializes the *selected* view, never the
    /// resurrected transcript. `model.call.completed` rows replay as
    /// `assistant{tool_calls}` + `tool{}` pairs (the kernel tool_call_id
    /// carries the pair — each request stands alone, the wire id is
    /// consistent within the body); `context.observation.recorded` joins
    /// on `scope.tool_call_id`; `model.call.refused`/`format_failure`/
    /// `call.failed` render `kernel_note` user messages — a declined call
    /// is explained, never silently resubmitted.
    fn transcript(
        &self,
        prefix: &[EventEnvelope],
        blobs: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<(Vec<Json>, Option<String>), ModelError> {
        let tr = &self.lane.transcript_render;
        // The format-error set — `model_call_id`s whose completed turn
        // was refused at interpretation (`format_error_turn: omit`
        // drops both the kernel note and the offending assistant turn).
        let refused_mcs: BTreeSet<String> = if tr.format_error_omit {
            prefix
                .iter()
                .filter(|e| {
                    matches!(
                        e.class.as_str(),
                        "model.format_failure"
                            | "model.call.refused"
                            | "control.output.rejected"
                            | "action.tool.call.refused"
                    )
                })
                .filter_map(|e| {
                    e.scope
                        .model_call_id
                        .clone()
                        .or_else(|| {
                            e.payload
                                .get("model_call_id")
                                .and_then(Json::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            e.scope
                                .tool_call_id
                                .as_deref()
                                .and_then(|t| t.rsplit_once(".tc").map(|(m, _)| m.to_string()))
                        })
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        let selected = selected_view(prefix)?;
        let mut messages = Vec::new();
        match &self.surface {
            // R6.35 (REQ-XE-103/104) — the compiled `ModelSurface`
            // renders the request preamble: layout sections under their
            // declared provider roles, each `hh-tmpl/1` text filled at
            // render time (`task` + the probe's `uname.*` values from
            // the durable `action.environment.probe.completed` rows).
            // The prior-turn fold below is the same code the unarmed
            // path runs — one renderer, never a second builder.
            Some(surface) => messages.extend(self.surface_head(surface, prefix)?),
            None => {
                if let Some((cap, args)) = &self.invoke {
                    messages.push(self.text_msg(
                        "user",
                        &format!(
                            "The caller directed: invoke `{cap}` with arguments {}",
                            args.to_canonical_string()
                        ),
                    ));
                } else {
                    messages.push(self.text_msg("user", &self.instruction));
                }
            }
        }
        // Observation rows keyed by the kernel tool_call_id. An item the
        // durable selection omitted never joins — the `context_item_id`
        // derivation is the assembler's own (`OBSERVATION_ITEM_IDP` over
        // the canonical payload), so the join is exact; a pair whose
        // result was omitted renders the honest `no_observation_recorded`
        // placeholder below, never a fabricated result.
        let mut observations: BTreeMap<String, Vec<Json>> = BTreeMap::new();
        for e in prefix {
            if e.class == "context.observation.recorded" {
                if let Some(tc) = &e.scope.tool_call_id {
                    if let Some((_, omitted)) = &selected {
                        let item_id = idp_id(
                            OBSERVATION_ITEM_IDP,
                            e.payload.to_canonical_string().as_bytes(),
                        );
                        if omitted.contains(&item_id) {
                            continue;
                        }
                    }
                    observations
                        .entry(tc.clone())
                        .or_default()
                        .push(e.payload.clone());
                }
            }
        }
        for e in prefix {
            match e.class.as_str() {
                "model.call.completed" => {
                    let p = &e.payload;
                    let calls: Vec<(String, String, String)> = match p.get("calls") {
                        Some(Json::Arr(cs)) => cs
                            .iter()
                            .filter_map(|c| {
                                let id = c.get("tool_call_id")?.as_str()?.to_string();
                                let surface = c.get("surface")?.as_str()?.to_string();
                                let args = c
                                    .get("args_raw")
                                    .and_then(Json::as_str)
                                    .unwrap_or("{}")
                                    .to_string();
                                Some((id, surface, args))
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    // `format_error_turn: omit` — the refused call's
                    // assistant turn is dropped with its note.
                    let mc_id = p
                        .get("model_call_id")
                        .and_then(Json::as_str)
                        .unwrap_or_default();
                    if tr.format_error_omit && refused_mcs.contains(mc_id) {
                        continue;
                    }
                    if calls.is_empty() {
                        // `text_only_turn: replay` renders the recorded
                        // assistant message (blob-sourced bytes only).
                        if tr.text_only_replay {
                            let rec = self.recorded_assistant(p, blobs, "text_only_turn")?;
                            messages.push(self.replay_assistant_msg(&rec, &[]));
                        }
                        continue;
                    }
                    // The declared replay: blob-sourced content/
                    // reasoning/extra members, provider ids, verbatim
                    // args — every member opt-in (REQ-XE-108).
                    let rec = if tr.requires_blob() {
                        Some(self.recorded_assistant(p, blobs, "assistant replay")?)
                    } else {
                        None
                    };
                    // The wire call id is the family's render of the
                    // kernel id (Messages lowers to its `[a-zA-Z0-9_-]`
                    // alphabet); the result pairs on the *wire* id.
                    // Under the declared replay (`requires_blob`), ids
                    // and argument bytes come from the recorded frames.
                    let wire_calls: Vec<(String, String, String)> = match &rec {
                        Some(rec) => {
                            if rec.calls.len() != calls.len() {
                                return Err(ModelError::new(
                                    ModelErrorClass::MalformedHistory,
                                    format!(
                                        "transcript_render: recorded {} tool blocks vs {} row calls",
                                        rec.calls.len(),
                                        calls.len()
                                    ),
                                ));
                            }
                            calls
                                .iter()
                                .zip(&rec.calls)
                                .map(|((id, surface, args), rc)| {
                                    let wid = if tr.provider_ids {
                                        rc.provider_id.clone().ok_or_else(|| {
                                            ModelError::new(
                                                ModelErrorClass::MalformedHistory,
                                                "transcript_render: provider id unrecorded"
                                                    .to_string(),
                                            )
                                        })?
                                    } else {
                                        self.wire_call_id(id)
                                    };
                                    Ok((
                                        wid,
                                        if tr.verbatim_args || tr.provider_ids {
                                            rc.name.clone()
                                        } else {
                                            hharness_gateway::openai::wire_name(surface)
                                        },
                                        if tr.verbatim_args {
                                            rc.args.clone()
                                        } else {
                                            args.clone()
                                        },
                                    ))
                                })
                                .collect::<Result<Vec<_>, ModelError>>()?
                        }
                        None => calls
                            .iter()
                            .map(|(id, surface, args)| {
                                (
                                    self.wire_call_id(id),
                                    hharness_gateway::openai::wire_name(surface),
                                    args.clone(),
                                )
                            })
                            .collect(),
                    };
                    match &rec {
                        Some(rec) => messages.push(self.replay_assistant_msg(rec, &wire_calls)),
                        None => messages.push(self.assistant_tool_msg(&wire_calls)),
                    }
                    for ((id, _, _), (wid, _, _)) in calls.iter().zip(&wire_calls) {
                        let obs = observations.remove(id).unwrap_or_default();
                        if obs.is_empty() {
                            // No `context.observation.recorded` joined —
                            // the call was refused or interpret-rejected;
                            // the honest content is the ledger's own
                            // absence (never a fabricated result).
                            messages.push(self.tool_result_msg(
                                wid,
                                Json::obj([("kind", Json::str("no_observation_recorded"))]),
                            ));
                        } else {
                            for o in obs {
                                messages.push(self.tool_result_msg(wid, o));
                            }
                        }
                    }
                }
                "model.call.failed" => {
                    let class = e
                        .payload
                        .get("error")
                        .and_then(|er| er.get("class"))
                        .and_then(Json::as_str)
                        .unwrap_or("unknown");
                    messages.push(self.text_msg(
                        "user",
                        &format!("kernel_note: the prior model call failed ({class})"),
                    ));
                }
                // The kernel-note leg keeps the pre-R6.34 class set —
                // `format_error_turn: omit` drops the offending
                // assistant turn (via `refused_mcs`, which reads the
                // wider refusal classes) but the default path renders
                // notes for the same two classes it always has.
                "model.format_failure" | "model.call.refused" if !tr.format_error_omit => {
                    let kind = e
                        .payload
                        .get("kind")
                        .or_else(|| e.payload.get("reason"))
                        .or_else(|| e.payload.get("failure_class"))
                        .and_then(Json::as_str)
                        .unwrap_or("declined");
                    messages.push(self.text_msg(
                        "user",
                        &format!(
                            "kernel_note: the prior output was declined by the kernel ({kind}); correct it"
                        ),
                    ));
                }
                _ => {}
            }
        }
        Ok((messages, selected.map(|(view_hash, _)| view_hash)))
    }

    /// Render `tools[]` from the armed surfaces + build the `wire→surface`
    /// map. A wire-name collision (two surfaces spelling the same
    /// `wire_name`) refuses `InvalidRequest` — never a silent merge.
    fn render_tools(&self) -> Result<(Vec<Json>, BTreeMap<String, String>), ModelError> {
        let mut tools = Vec::new();
        let mut map = BTreeMap::new();
        if let Some(surface) = &self.surface {
            // R6.36 (REQ-XE-105/106) — the compiled `ModelSurface.tools`
            // render: each entry carries the compiled `description` and
            // the compiled `schema` (the capability `input_schema`
            // projection) verbatim — member descriptions included, no
            // `ParamSpec` re-derivation. `hh.submit` follows only when
            // the definition's `completion_surface`/`stop_rule` armed
            // it at open (the same entry the legacy path renders).
            for t in &surface.tools {
                let wire = hharness_gateway::openai::wire_name(&t.binding.surface_name);
                if map.contains_key(&wire) {
                    return Err(ModelError::new(
                        ModelErrorClass::InvalidRequest,
                        format!("wire-name collision on `{wire}`"),
                    ));
                }
                map.insert(wire.clone(), t.binding.surface_id.clone());
                tools.push(self.tool_entry(&wire, &t.description, t.schema.clone()));
            }
            if self.completion_armed {
                let wire = hharness_gateway::openai::wire_name(crate::runtime::SUBMIT_SURFACE);
                if map.contains_key(&wire) {
                    return Err(ModelError::new(
                        ModelErrorClass::InvalidRequest,
                        format!("wire-name collision on `{wire}`"),
                    ));
                }
                map.insert(wire.clone(), crate::runtime::SUBMIT_SURFACE.to_string());
                tools.push(self.tool_entry(
                    &wire,
                    &format!("{}/1", crate::runtime::SUBMIT_SURFACE),
                    Json::obj([
                        ("type", Json::str("object")),
                        ("properties", Json::obj([])),
                        ("required", Json::Arr(vec![])),
                    ]),
                ));
            }
            return Ok((tools, map));
        }
        for s in &self.surfaces {
            let wire = hharness_gateway::openai::wire_name(&s.surface_id);
            if map.contains_key(&wire) {
                return Err(ModelError::new(
                    ModelErrorClass::InvalidRequest,
                    format!("wire-name collision on `{wire}`"),
                ));
            }
            map.insert(wire.clone(), s.surface_id.clone());
            // The parameters schema the surface declares — `object` with
            // the ParamSpec members rendered json-schema-shaped.
            let mut props = std::collections::BTreeMap::new();
            let mut required = Vec::new();
            for (name, p) in &s.params {
                let ty = match p.kind {
                    hharness_control::output::ParamKind::Str => Some("string"),
                    hharness_control::output::ParamKind::Int => Some("integer"),
                    hharness_control::output::ParamKind::Bool => Some("boolean"),
                    hharness_control::output::ParamKind::Arr => Some("array"),
                    hharness_control::output::ParamKind::Obj => Some("object"),
                    hharness_control::output::ParamKind::Any => None,
                };
                let mut spec = std::collections::BTreeMap::new();
                if let Some(t) = ty {
                    spec.insert("type".into(), Json::str(t));
                }
                if !p.enum_values.is_empty() {
                    spec.insert("enum".into(), Json::Arr(p.enum_values.clone()));
                }
                props.insert(name.clone(), Json::Obj(spec));
                if p.required {
                    required.push(Json::str(name.clone()));
                }
            }
            tools.push(self.tool_entry(
                &wire,
                &s.semantic_id,
                Json::obj([
                    ("type", Json::str("object")),
                    ("properties", Json::Obj(props)),
                    ("required", Json::Arr(required)),
                ]),
            ));
        }
        Ok((tools, map))
    }

    /// Build the `ProviderRequestPlan` for this call.
    fn build_plan(
        &mut self,
        mc: &str,
        prefix: &[EventEnvelope],
        blobs: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<PendingPlan, ModelError> {
        let (messages, view_hash) = self.transcript(prefix, blobs)?;
        let (tools, wire_to_surface) = self.render_tools()?;
        let mut params = self.lane.provider_params.clone();
        // The family's own augmentations: chat-completions requests
        // `stream_options.include_usage` (the trailing usage chunk the
        // grammar expects); Messages usage rides `message_start`/
        // `message_delta` natively — `stream_options` is outside the
        // Messages `plan_schema` and never inserted.
        if self.lane.transport == PortTransport::Stream
            && self.family == Family::ChatCompletions
            && !params.contains_key("stream_options")
        {
            params.insert(
                "stream_options".into(),
                Json::obj([("include_usage", Json::Bool(true))]),
            );
        }
        let body = self.request_body(&messages, &tools, &params);
        let plan = ProviderRequestPlan {
            dialect_id: self.dialect.dialect_id.clone(),
            dialect_version: self.dialect.version.clone(),
            endpoint_ref: self.lane.endpoint_ref.clone(),
            model_ref: self.lane.provider_model_id.clone(),
            body,
            provider_params: params,
            capabilities: Capabilities {
                streaming: self.lane.transport == PortTransport::Stream,
                tool_use: !tools.is_empty(),
                vision: None,
                deferred: None,
            },
            cache: CachePlan::default(),
            budget_reservation_id: None,
            canonical_len: 0,
        };
        let plan = ProviderRequestPlan {
            canonical_len: codec::serialize(&plan).len() as u64,
            ..plan
        };
        codec::validate_plan(&self.dialect, &plan)
            .map_err(|e| ModelError::new(ModelErrorClass::InvalidRequest, format!("plan: {e}")))?;
        self.endpoints
            .admit(&plan.endpoint_ref)
            .map_err(|e| ModelError::new(ModelErrorClass::Forbidden, format!("{e}")))?;
        // Credential bind — the lane's `credential_ref` coordinate to a
        // safe handle (`binding_id` + `provided` only). The port resolves
        // the `env:NAME` coordinate *here* — call-site of dispatch — and a
        // missing secret is a typed `credential_unavailable` refusal the
        // plan-error channel carries to `model.call.failed{auth}`; the
        // wire is never touched (ADR-0366 D4).
        let credential = self
            .credentials
            .bind(
                self.lane.credential_ref.as_deref(),
                &self.lane.endpoint_ref,
                &self.dialect.auth_kinds,
            )
            .map_err(|e| ModelError::new(ModelErrorClass::Auth, format!("{e}")))?;
        let _ = mc;
        Ok(PendingPlan {
            body: plan.body.clone(),
            plan,
            wire_to_surface,
            credential,
            view_hash,
        })
    }

    /// Fold the decoded events into the `ModelOutcome` + stage terminal
    /// members + queue the `model_io` blob.
    fn finish_call(
        &mut self,
        mc: &str,
        pending: PendingPlan,
        frames: &[(u64, WireFrame)],
        transport_ms: u64,
    ) -> ModelOutcome {
        let mut state = DecodeState {
            model_call_id: mc.to_string(),
            attempt_no: 1,
            ..DecodeState::default()
        };
        let mut events = Vec::new();
        let mut ttft: Option<u64> = None;
        for (at, f) in frames {
            for ev in grammar::decode_frame(&self.dialect, f, &mut state, *at, &self.normalizer_ref)
            {
                if ttft.is_none() {
                    ttft = Some(*at);
                }
                events.push(ev);
            }
        }
        // The terminal event — `Completed`/`Failed` (the last kind wins).
        let terminal = events.iter().rev().find_map(|e| match &e.kind {
            k @ (ModelEventKind::Completed { .. } | ModelEventKind::Failed { .. }) => Some(k),
            _ => None,
        });
        let (outcome, usage_pair, served_model, response_id) = match terminal {
            Some(ModelEventKind::Completed {
                message,
                usage,
                surface_ids,
                ..
            }) => {
                let resp_id = surface_ids.get("response_id").cloned();
                let served = message.served_model.clone();
                // The substitution rule — a served model ≠ the bound one
                // is `served_model_mismatch` unless the lane declared it.
                if let (Some(s), false) = (&served, self.lane.substitution_allowed) {
                    if *s != self.lane.provider_model_id {
                        let err = ModelError::new(
                            ModelErrorClass::ServedModelMismatch,
                            format!(
                                "served `{}` ≠ bound `{}` (substitution_allowed=false)",
                                s, self.lane.provider_model_id
                            ),
                        );
                        let o = self.error_outcome(mc, &pending, frames, &err);
                        return o;
                    }
                }
                let mut calls = Vec::new();
                for b in &message.blocks {
                    if let ModelBlock::ToolCall(tc) = b {
                        let surface = self
                            .pending_wire_surface(&pending, &tc.surface_name)
                            .unwrap_or_else(|| tc.surface_name.clone());
                        let args_raw = match &tc.arguments {
                            ToolArgs::Parsed(j) => j.to_canonical_string(),
                            ToolArgs::Unparseable { raw, .. } => raw.clone(),
                            ToolArgs::Raw(r) => r.clone(),
                        };
                        calls.push(ParsedCall {
                            tool_call_id: tc.tool_call_id.clone(),
                            surface,
                            args_raw,
                        });
                    }
                }
                let text_empty = message.text().is_empty();
                let raw_usage = state.raw_usage.clone().unwrap_or(Json::Null);
                self.last_text = Some(message.text());
                let response_ref = self.record_io(mc, &pending, frames, message, &raw_usage);
                let up = usage.clone();
                (
                    ModelOutcome {
                        stop_reason: message.stop_reason,
                        calls,
                        text_empty,
                        response_ref,
                        error_class: None,
                        retry_after_ms: None,
                    },
                    up,
                    served,
                    resp_id,
                )
            }
            Some(ModelEventKind::Failed { error, usage, .. }) => {
                self.last_text = None;
                let response_ref = self.record_io_err(mc, &pending, frames, error);
                let up = usage.clone();
                (
                    ModelOutcome {
                        stop_reason: StopReason::Error,
                        calls: Vec::new(),
                        text_empty: true,
                        response_ref,
                        error_class: Some(error.class.as_str().to_string()),
                        retry_after_ms: error.retry_after_ms,
                    },
                    up,
                    None,
                    None,
                )
            }
            _ => {
                // No terminal — grammar violations or a truncated stream.
                let err = ModelError::new(
                    ModelErrorClass::StreamDecode,
                    format!(
                        "stream ended without a terminal event (violations: {})",
                        state.violations.len()
                    ),
                );
                return self.error_outcome(mc, &pending, frames, &err);
            }
        };
        // Terminal members — `usage{record{view,normalizer_ref},
        // arrival,available}`, `timing`, `served_model`. The provider's
        // *raw* usage document stays one hop off the row — `outcome.
        // usage_raw` on the `model_io` blob `response_ref` resolves to —
        // because `model.call.completed` is an OPEN_AUDIT class (every
        // member is an inline audit field bounded at 512 canonical
        // bytes) and a provider's verbatim usage object can exceed the
        // bound (Anthropic's cache/service-tier members push it past).
        // The blob keeps it durable and verbatim; the row keeps the
        // normalized vector + its provenance.
        let mut members: Vec<(&'static str, Json)> = Vec::new();
        let mut u = std::collections::BTreeMap::new();
        match &usage_pair {
            Some(v) => {
                u.insert("available".into(), Json::Bool(true));
                let mut rec = std::collections::BTreeMap::new();
                rec.insert("view".into(), v.to_json());
                rec.insert("normalizer_ref".into(), Json::str(v.normalizer_ref.clone()));
                u.insert("record".into(), Json::Obj(rec));
            }
            None => {
                u.insert("available".into(), Json::Bool(false));
            }
        }
        u.insert("arrival".into(), Json::str("final_frame"));
        members.push(("usage", Json::Obj(u)));
        let mut t = std::collections::BTreeMap::new();
        t.insert("latency_ms".into(), Json::Int(transport_ms as i64));
        t.insert("attempts".into(), Json::Int(1));
        if let Some(tt) = ttft {
            t.insert("ttft_ms".into(), Json::Int(tt as i64));
        }
        t.insert("queue_wait_ms".into(), Json::Int(0));
        t.insert("measured_at".into(), Json::str("adapter"));
        members.push(("timing", Json::Obj(t)));
        if let Some(sm) = &served_model {
            members.push(("served_model", Json::str(sm.clone())));
        }
        if let Some(r) = &response_id {
            members.push(("response_id", Json::str(r.clone())));
        }
        self.terminal.insert(mc.to_string(), members);
        outcome
    }

    /// The refused-plan tombstone `call` consumes — the wire is never
    /// touched, the members and `model_io` still record the refusal.
    fn empty_plan(&self) -> PendingPlan {
        PendingPlan {
            plan: ProviderRequestPlan {
                dialect_id: self.dialect.dialect_id.clone(),
                dialect_version: self.dialect.version.clone(),
                endpoint_ref: self.lane.endpoint_ref.clone(),
                model_ref: self.lane.provider_model_id.clone(),
                body: Json::Null,
                provider_params: BTreeMap::new(),
                capabilities: Capabilities {
                    streaming: false,
                    tool_use: false,
                    vision: None,
                    deferred: None,
                },
                cache: CachePlan::default(),
                budget_reservation_id: None,
                canonical_len: 0,
            },
            wire_to_surface: BTreeMap::new(),
            body: Json::Null,
            credential: CredentialHandle {
                binding_id: None,
                provided: false,
            },
            view_hash: None,
        }
    }

    /// The staged plan's `body` member for `mc` — the bytes the wire
    /// will carry (the golden/assertion seam; `None` before
    /// `requested_members` or after `call` consumed it).
    pub fn pending_body(&self, mc: &str) -> Option<&Json> {
        self.pending.get(mc).map(|p| &p.body)
    }

    /// The wire→surface resolve (the call's own map).
    fn pending_wire_surface(&self, pending: &PendingPlan, wire: &str) -> Option<String> {
        pending.wire_to_surface.get(wire).cloned()
    }

    /// Stage the `model_io` blob + answer the `response_ref` (the blob's
    /// `idp` address — the driver deposits before the terminal row lands).
    /// `raw_usage` rides `outcome.usage_raw` — the provider's verbatim
    /// usage document, durable off the audit-bounded row (see
    /// `finish_call`).
    fn record_io(
        &mut self,
        mc: &str,
        pending: &PendingPlan,
        frames: &[(u64, WireFrame)],
        message: &hharness_gateway::message::ModelMessage,
        raw_usage: &Json,
    ) -> String {
        let doc = Json::obj([
            ("schema", Json::str("hh.model_io/1")),
            ("model_call_id", Json::str(mc)),
            ("request_body", pending.body.clone()),
            ("plan_ref", Json::str(pending.plan.content_id())),
            (
                "frames",
                Json::Arr(
                    frames
                        .iter()
                        .map(|(at, f)| {
                            Json::obj([
                                ("at_ms", Json::Int(*at as i64)),
                                ("kind", Json::str(f.kind.clone())),
                                ("data", f.data.clone()),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "outcome",
                Json::obj([
                    ("stop_reason", Json::str(message.stop_reason.as_str())),
                    (
                        "served_model",
                        message
                            .served_model
                            .clone()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    ("usage_raw", raw_usage.clone()),
                ]),
            ),
        ]);
        self.push_blob(doc)
    }

    /// The error leg's `model_io` — the frames + the typed error verbatim.
    fn record_io_err(
        &mut self,
        mc: &str,
        pending: &PendingPlan,
        frames: &[(u64, WireFrame)],
        error: &ModelError,
    ) -> String {
        let doc = Json::obj([
            ("schema", Json::str("hh.model_io/1")),
            ("model_call_id", Json::str(mc)),
            ("request_body", pending.body.clone()),
            ("plan_ref", Json::str(pending.plan.content_id())),
            (
                "frames",
                Json::Arr(
                    frames
                        .iter()
                        .map(|(at, f)| {
                            Json::obj([
                                ("at_ms", Json::Int(*at as i64)),
                                ("kind", Json::str(f.kind.clone())),
                                ("data", f.data.clone()),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("outcome", Json::obj([("error", error.to_json())])),
        ]);
        self.push_blob(doc)
    }

    fn push_blob(&mut self, doc: Json) -> String {
        let bytes = doc.to_canonical_string().into_bytes();
        let addr = hharness_identity::idp::address(&bytes, "application/json");
        self.model_io.push(("application/json".to_string(), bytes));
        addr.id()
    }

    /// A transport-level error outcome — the frames that arrived (possibly
    /// none) still record `model_io` for the audit tail.
    fn error_outcome(
        &mut self,
        mc: &str,
        pending: &PendingPlan,
        frames: &[(u64, WireFrame)],
        error: &ModelError,
    ) -> ModelOutcome {
        self.last_text = None;
        let response_ref = self.record_io_err(mc, pending, frames, error);
        let mut members: Vec<(&'static str, Json)> = Vec::new();
        let mut u = std::collections::BTreeMap::new();
        u.insert("available".into(), Json::Bool(false));
        members.push(("usage", Json::Obj(u)));
        self.terminal.insert(mc.to_string(), members);
        ModelOutcome {
            stop_reason: if error.class == ModelErrorClass::Cancelled {
                StopReason::Cancelled
            } else {
                StopReason::Error
            },
            calls: Vec::new(),
            text_empty: true,
            response_ref,
            error_class: Some(error.class.as_str().to_string()),
            retry_after_ms: error.retry_after_ms,
        }
    }
}

/// `RecordedAssistant` — one assistant turn's verbatim record folded
/// from the durable `hh.model_io/1` frames (R6.34; REQ-XE-108).
struct RecordedAssistant {
    /// Concatenated `text_delta` bytes.
    text: String,
    /// Concatenated `reasoning_delta`/`thinking_delta` bytes.
    reasoning: String,
    /// One entry per recorded tool block, in frame order:
    /// `(provider_call_id, wire name, verbatim arguments)`.
    calls: Vec<RecordedCall>,
    /// `stream_end.data.message_extra` — provider message members the
    /// translator preserved verbatim (the `extra_members` source).
    extra: BTreeMap<String, Json>,
}

/// One recorded tool call's verbatim frame content.
struct RecordedCall {
    /// `content_block.id` — the provider's call id (G19).
    provider_id: Option<String>,
    /// `content_block.name` — the provider's wire spelling.
    name: String,
    /// Concatenated `input_json_delta`/`json_delta` bytes (G20).
    args: String,
}

/// `fold_assistant_frames(doc)` — fold a `hh.model_io/1` document's
/// `frames[]` into the assistant turn's verbatim record: `block_start`
/// opens the call (`content_block{id?, name?}`), `block_delta`
/// accumulates text/reasoning/argument bytes per index, and
/// `stream_end.data.message_extra` carries the preserved message-level
/// members. Frames are the translator's normalized vocabulary — the
/// same data `grammar::decode_frame` reads (one decoder, CC1).
fn fold_assistant_frames(doc: &Json) -> RecordedAssistant {
    let mut rec = RecordedAssistant {
        text: String::new(),
        reasoning: String::new(),
        calls: Vec::new(),
        extra: BTreeMap::new(),
    };
    let mut open: BTreeMap<i64, usize> = BTreeMap::new();
    let frames = match doc.get("frames") {
        Some(Json::Arr(f)) => f,
        _ => return rec,
    };
    for fr in frames {
        let kind = fr.get("kind").and_then(Json::as_str).unwrap_or_default();
        let data = fr.get("data").cloned().unwrap_or(Json::Null);
        match kind {
            "block_start" => {
                let index = data.get("index").and_then(Json::as_int).unwrap_or(0);
                let block = data.get("content_block").cloned().unwrap_or(Json::Null);
                let btype = block.get("type").and_then(Json::as_str).unwrap_or_default();
                if matches!(btype, "tool_call" | "tool_use") {
                    rec.calls.push(RecordedCall {
                        provider_id: block.get("id").and_then(Json::as_str).map(str::to_string),
                        name: block
                            .get("name")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        args: String::new(),
                    });
                    open.insert(index, rec.calls.len() - 1);
                }
            }
            "block_delta" => {
                let index = data.get("index").and_then(Json::as_int).unwrap_or(0);
                let delta = data.get("delta").cloned().unwrap_or(Json::Null);
                let dtype = delta
                    .get("type")
                    .and_then(Json::as_str)
                    .unwrap_or("text_delta");
                match dtype {
                    "text_delta" => {
                        if let Some(t) = delta.get("text").and_then(Json::as_str) {
                            rec.text.push_str(t);
                        }
                    }
                    "thinking_delta" | "reasoning_delta" => {
                        if let Some(t) = delta
                            .get("thinking")
                            .or_else(|| delta.get("text"))
                            .and_then(Json::as_str)
                        {
                            rec.reasoning.push_str(t);
                        }
                    }
                    "input_json_delta" | "json_delta" => {
                        if let (Some(&ci), Some(p)) = (
                            open.get(&index),
                            delta
                                .get("partial_json")
                                .or_else(|| delta.get("patch"))
                                .and_then(Json::as_str),
                        ) {
                            rec.calls[ci].args.push_str(p);
                        }
                    }
                    _ => {}
                }
            }
            "stream_end" => {
                if let Some(Json::Obj(extra)) = data.get("message_extra") {
                    for (k, v) in extra {
                        rec.extra.insert(k.clone(), v.clone());
                    }
                }
            }
            _ => {}
        }
    }
    rec
}

/// `OneShot` — a caller-owned single call's answer (R4.7b): the decoded
/// message text, the normalized usage record (the same shape the
/// `model.call.completed` `usage` member carries), the provider's raw
/// usage document, the served model, the terminal outcome, and the
/// `hh.model_io/1` blobs the call produced (the caller deposits them
/// through its own fenced seam — durable-before-visible stays the
/// caller's job; the adapter itself appends nothing).
pub struct OneShot {
    /// The completed message's text ("" — `text_empty` — when the
    /// provider answered no text).
    pub text: String,
    /// `usage{available, record{view, normalizer_ref}, arrival}` — the
    /// terminal member the driver would have stamped.
    pub usage: Json,
    /// The provider's verbatim usage document (the `model_io` outcome's
    /// `usage_raw`).
    pub usage_raw: Json,
    /// The provider-reported served model (drift fact; absent when the
    /// provider doesn't report one).
    pub served_model: Option<String>,
    /// The terminal `ModelOutcome` (`stop_reason`, `error_class`).
    pub outcome: ModelOutcome,
    /// The drained `hh.model_io/1` blobs `(media, bytes)`.
    pub model_io: Vec<(String, Vec<u8>)>,
}

impl GatewayModel {
    /// `one_shot(mc)` — run this port's full requested→call→terminal
    /// pipeline once, outside the driver fence (R4.7b: the lane-backed
    /// `Summarizer` adapter rides it). The transcript is the staged
    /// `instruction` alone (an empty prefix — one user message); the
    /// caller collects the answer members instead of the driver's rows.
    /// `Err` only when the plan can't even stage — a refused/failed wire
    /// leg answers inside `outcome` typed, never a fabricated text.
    pub fn one_shot(&mut self, mc: &str) -> OneShot {
        let _ = self.requested_members(mc, &Json::Null, &[], &|_: &str| None);
        let outcome = self.call(mc, &Json::Null);
        let members = self.terminal_members(mc);
        let model_io = self.take_model_io();
        let mut usage = Json::Null;
        for (k, v) in &members {
            if *k == "usage" {
                usage = v.clone();
            }
        }
        // The provider's verbatim usage rides the produced blob's
        // `outcome.usage_raw` member (the audit-bounded row carries the
        // normalized vector only — finish_call's rule).
        let usage_raw = blobs_usage_raw(&model_io);
        OneShot {
            text: self.last_text.take().unwrap_or_default(),
            usage,
            usage_raw,
            served_model: members
                .iter()
                .find(|(k, _)| *k == "served_model")
                .and_then(|(_, v)| v.as_str().map(str::to_string)),
            outcome,
            model_io,
        }
    }
}

/// The produced `model_io` docs' `outcome.usage_raw` (the last blob wins —
/// a one-shot produces exactly one).
fn blobs_usage_raw(model_io: &[(String, Vec<u8>)]) -> Json {
    model_io
        .iter()
        .rev()
        .filter_map(|(_, b)| hharness_wire::json::parse(&String::from_utf8_lossy(b)).ok())
        .find_map(|d| d.get("outcome").and_then(|o| o.get("usage_raw")).cloned())
        .unwrap_or(Json::Null)
}

impl ModelPort for GatewayModel {
    fn requested_members(
        &mut self,
        mc: &str,
        _request: &Json,
        prefix: &[EventEnvelope],
        blobs: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Vec<(&'static str, Json)> {
        match self.build_plan(mc, prefix, blobs) {
            Ok(p) => {
                let plan_hash = p.plan.content_id();
                let estimate = codec::estimate_tokens(&self.dialect, &p.plan);
                let model_ref = Json::obj([
                    ("profile_ref", Json::str(self.lane.profile_ref.clone())),
                    (
                        "provider_model_id",
                        Json::str(self.lane.provider_model_id.clone()),
                    ),
                ]);
                // The row carries the binding's *id* — the coordinate's
                // binding record, never the material. An unbound handle
                // (the local lane) records `credentials:none`.
                let binding_id = p
                    .credential
                    .binding_id
                    .clone()
                    .unwrap_or_else(|| "credentials:none".to_string());
                let view_hash = p.view_hash.clone();
                self.pending.insert(mc.to_string(), p);
                let mut members = vec![
                    ("plan_hash", Json::str(plan_hash)),
                    ("endpoint_ref", Json::str(self.lane.endpoint_ref.clone())),
                    ("model_ref", model_ref),
                    ("credential_binding_id", Json::str(binding_id)),
                    (
                        "token_estimate",
                        Json::obj([
                            ("count", Json::Int(estimate.count as i64)),
                            ("method", Json::str(estimate.method.as_str())),
                            ("confidence", Json::str(estimate.confidence.as_str())),
                        ]),
                    ),
                ];
                // §5b.1's `view_hash` member — the durable
                // `retained_selection/1` this render serialized (the
                // consumed member's own idp address — the same value the
                // prepared call's `view_ref` carries over the same
                // member). Absent ⇒ no selection informed the render;
                // fidelity is unavailable, never a treatment claim.
                if let Some(h) = view_hash {
                    members.push(("view_hash", Json::str(h)));
                }
                members
            }
            Err(e) => {
                // A plan that can't build lands the refusal on the row —
                // `call` answers the typed error outcome (no silent skip).
                let class = e.class.as_str();
                let detail = e.to_string();
                self.terminal
                    .insert(mc.to_string(), vec![("plan_error", Json::str(detail))]);
                let tomb = self.empty_plan();
                self.pending.insert(mc.to_string(), tomb);
                self.plan_error = Some((mc.to_string(), e));
                vec![("plan_error", Json::str(class))]
            }
        }
    }

    fn call(&mut self, mc: &str, _request: &Json) -> ModelOutcome {
        // A plan-build error staged at `requested_members` fails the call
        // typed — the wire is never touched.
        if let Some((emc, err)) = self.plan_error.take() {
            if emc == mc {
                let pending = self
                    .pending
                    .remove(&emc)
                    .unwrap_or_else(|| self.empty_plan());
                return self.error_outcome(&emc, &pending, &[], &err);
            }
            self.plan_error = Some((emc, err));
        }
        let Some(pending) = self.pending.remove(mc) else {
            return ModelOutcome {
                stop_reason: StopReason::Error,
                calls: Vec::new(),
                text_empty: true,
                response_ref: String::new(),
                error_class: Some("unknown".to_string()),
                retry_after_ms: None,
            };
        };
        // The credential was bound at `requested_members` (the plan's
        // `credential` member) — the handle carries `binding_id` +
        // `provided` only; `send` resolves the header material through
        // the binding's source inside the transport, immediately before
        // the write, and the value lands nowhere else.
        let bytes = codec::serialize(&pending.plan);
        let t0 = std::time::Instant::now();
        match self
            .transport
            .send(&bytes, &pending.credential, &pending.plan.endpoint_ref)
        {
            Ok(frames) => {
                let ms = t0.elapsed().as_millis() as u64;
                self.finish_call(mc, pending, &frames, ms)
            }
            Err(err) => self.error_outcome(mc, &pending, &[], &err),
        }
    }

    fn terminal_members(&mut self, mc: &str) -> Vec<(&'static str, Json)> {
        self.terminal.remove(mc).unwrap_or_default()
    }

    fn take_model_io(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.model_io)
    }
}
