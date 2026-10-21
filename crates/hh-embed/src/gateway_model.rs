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
//! `hh_gateway`'s own machinery (`validate_plan`/`EndpointAllowlist::
//! admit`/`CredentialPort::bind`/`Transport::send` →
//! `grammar::decode_frame` → `collect`); nothing in this module
//! re-implements classification, grammar, or framing.

use std::collections::BTreeMap;

use hh_control::driver::{ModelOutcome, ModelPort};
use hh_control::output::ParsedCall;
use hh_control::output::SurfaceSpec;
use hh_gateway::codec::{self, WireFrame};
use hh_gateway::credentials::BrokerCredentialPort;
use hh_gateway::dialect::WireDialect;
use hh_gateway::gateway::{CredentialHandle, CredentialPort, EndpointAllowlist, Transport};
use hh_gateway::grammar::{self, DecodeState, ModelEventKind};
use hh_gateway::http_transport::Family;
use hh_gateway::message::{ModelBlock, ToolArgs};
use hh_gateway::plan::{CachePlan, Capabilities, ProviderRequestPlan};
use hh_gateway::vocab::{ModelError, ModelErrorClass, StopReason};
use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;

use hh_embed_schema::errors::EmbedError;

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

    /// `client_tls()` — arm the `hh_tls::ClientTls` the lane declares.
    /// Arm-time refusals are typed (`Refused` — the file is gone or
    /// unparseable at drive), never a silently smaller root set.
    fn client_tls(&self) -> Result<hh_tls::ClientTls, EmbedError> {
        let mut tls = hh_tls::ClientTls::webpki_roots();
        if let Some(p) = &self.extra_roots_file {
            tls =
                tls.extra_roots_file(std::path::Path::new(p))
                    .map_err(|e| EmbedError::Refused {
                        reason: format!("model_port.tls.extra_roots_file `{p}`: {e}"),
                    })?;
        }
        match (&self.client_cert_file, &self.client_key_file) {
            (Some(c), Some(k)) => {
                let chain = hh_tls::pem::read_pem(std::path::Path::new(c)).map_err(|e| {
                    EmbedError::Refused {
                        reason: format!("model_port.tls.client_cert_file `{c}`: {e}"),
                    }
                })?;
                let key = hh_tls::pem::read_pem(std::path::Path::new(k)).map_err(|e| {
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
        Json::Obj(m)
    }

    /// The manifest member's decode — absent ⇒ `None` (the scripted lane).
    pub fn from_manifest(manifest: &hh_ledger::manifest::RunManifest) -> Option<ModelLane> {
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
    hh_gateway::dialects::OPENAI_COMPATIBLE_ID,
    hh_gateway::dialects::OPENAI_ID,
    hh_gateway::dialects::ANTHROPIC_MESSAGES_ID,
];

/// `dialect_by_id(ref)` — bind a dialect coordinate to its document. An
/// unknown coordinate answers `None` (the caller names it
/// `InvalidDefinition` — never a silent fallback to another dialect).
fn dialect_by_id(dialect_ref: &str) -> Option<WireDialect> {
    match dialect_ref {
        hh_gateway::dialects::OPENAI_COMPATIBLE_ID => {
            Some(hh_gateway::dialects::openai_compatible())
        }
        hh_gateway::dialects::OPENAI_ID => Some(hh_gateway::dialects::openai()),
        hh_gateway::dialects::ANTHROPIC_MESSAGES_ID => {
            Some(hh_gateway::dialects::anthropic_messages())
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
        .unwrap_or(hh_gateway::dialects::OPENAI_COMPATIBLE_ID)
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
    Ok(ModelLane {
        dialect_ref,
        endpoint_ref,
        provider_model_id,
        credential_ref,
        substitution_allowed: matches!(params.get("substitution_allowed"), Some(Json::Bool(true))),
        profile_ref: profile_ref.to_string(),
        provider_params,
        tls,
    })
}

/// `model_port_slot_params(doc)` — the `native.slots["model_port"]`
/// declaration (same read discipline as `router_slot_params`).
pub fn model_port_slot_params(doc: &hh_hir::document::HirDocument) -> Option<Json> {
    use hh_hir::records::{AgentProcessBody, KindRecord, SlotBindings};
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

// ── the port ─────────────────────────────────────────────────────────────────

/// One staged plan — built at `requested_members`, consumed by `call`.
struct PendingPlan {
    plan: ProviderRequestPlan,
    /// `wire name → surface_id` — the request's own tool-name map
    /// (`wire_name` is not injective; the map is the honest inverse).
    wire_to_surface: BTreeMap<String, String>,
    /// The request body (re-recorded on the `model_io` blob).
    body: Json,
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
    transport: hh_gateway::http_transport::HttpModelTransport,
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
        let family = hh_gateway::family_of(&dialect);
        let client_tls = lane.tls.client_tls()?;
        let transport =
            hh_gateway::http_transport::HttpModelTransport::with_tls(dialect.clone(), client_tls);
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
        })
    }

    /// `text_msg(role, text)` — a family-rendered plain-text turn.
    /// Both families' user/kernel-note messages carry one text body; the
    /// Messages spelling wraps it in a `text` content block.
    fn text_msg(&self, role: &str, text: &str) -> Json {
        match self.family {
            Family::Messages => hh_gateway::anthropic::text_msg(role, text),
            Family::ChatCompletions => hh_gateway::openai::text_msg(role, text),
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
            Family::Messages => hh_gateway::anthropic::assistant_tool_use_msg(calls),
            Family::ChatCompletions => hh_gateway::openai::assistant_tool_call_msg(None, calls),
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
            Family::Messages => hh_gateway::anthropic::wire_call_id(id),
            Family::ChatCompletions => id.to_string(),
        }
    }

    /// `tool_result_msg(id, content)` — the observation the kernel
    /// recorded for one tool call: `role:"tool"` under chat-completions,
    /// a `user` turn carrying a `tool_result` block under Messages.
    fn tool_result_msg(&self, id: &str, content: Json) -> Json {
        match self.family {
            Family::Messages => hh_gateway::anthropic::tool_result_msg(id, content),
            Family::ChatCompletions => hh_gateway::openai::tool_result_msg(id, content),
        }
    }

    /// `tool_entry(name, desc, schema)` — one `tools[]` member:
    /// `{type:"function", function:{…}}` under chat-completions,
    /// `{name, description, input_schema}` under Messages.
    fn tool_entry(&self, name: &str, desc: &str, schema: Json) -> Json {
        match self.family {
            Family::Messages => hh_gateway::anthropic::tool_entry(name, desc, schema),
            Family::ChatCompletions => hh_gateway::openai::tool_entry(name, desc, schema),
        }
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
        match self.family {
            Family::Messages => hh_gateway::anthropic::messages_request_body(
                &self.lane.provider_model_id,
                messages,
                tools,
                true,
                params,
            ),
            Family::ChatCompletions => hh_gateway::openai::chat_request_body(
                &self.lane.provider_model_id,
                messages,
                tools,
                true,
                params,
            ),
        }
    }

    /// The transcript render — instruction + recorded rounds from the
    /// durable prefix. `model.call.completed` rows replay as
    /// `assistant{tool_calls}` + `tool{}` pairs (the kernel tool_call_id
    /// carries the pair — each request stands alone, the wire id is
    /// consistent within the body); `context.observation.recorded` joins
    /// on `scope.tool_call_id`; `model.call.refused`/`format_failure`/
    /// `call.failed` render `kernel_note` user messages — a declined call
    /// is explained, never silently resubmitted.
    fn transcript(&self, prefix: &[EventEnvelope]) -> Vec<Json> {
        let mut messages = Vec::new();
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
        // Observation rows keyed by the kernel tool_call_id.
        let mut observations: BTreeMap<String, Vec<Json>> = BTreeMap::new();
        for e in prefix {
            if e.class == "context.observation.recorded" {
                if let Some(tc) = &e.scope.tool_call_id {
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
                    if calls.is_empty() {
                        continue;
                    }
                    // The wire call id is the family's render of the
                    // kernel id (Messages lowers to its `[a-zA-Z0-9_-]`
                    // alphabet); the result pairs on the *wire* id.
                    let wire_calls: Vec<(String, String, String)> = calls
                        .iter()
                        .map(|(id, surface, args)| {
                            (
                                self.wire_call_id(id),
                                hh_gateway::openai::wire_name(surface),
                                args.clone(),
                            )
                        })
                        .collect();
                    messages.push(self.assistant_tool_msg(&wire_calls));
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
                "model.format_failure" | "model.call.refused" => {
                    let kind = e
                        .payload
                        .get("kind")
                        .or_else(|| e.payload.get("reason"))
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
        messages
    }

    /// Render `tools[]` from the armed surfaces + build the `wire→surface`
    /// map. A wire-name collision (two surfaces spelling the same
    /// `wire_name`) refuses `InvalidRequest` — never a silent merge.
    fn render_tools(&self) -> Result<(Vec<Json>, BTreeMap<String, String>), ModelError> {
        let mut tools = Vec::new();
        let mut map = BTreeMap::new();
        for s in &self.surfaces {
            let wire = hh_gateway::openai::wire_name(&s.surface_id);
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
                    hh_control::output::ParamKind::Str => Some("string"),
                    hh_control::output::ParamKind::Int => Some("integer"),
                    hh_control::output::ParamKind::Bool => Some("boolean"),
                    hh_control::output::ParamKind::Arr => Some("array"),
                    hh_control::output::ParamKind::Obj => Some("object"),
                    hh_control::output::ParamKind::Any => None,
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
    ) -> Result<PendingPlan, ModelError> {
        let messages = self.transcript(prefix);
        let (tools, wire_to_surface) = self.render_tools()?;
        let mut params = self.lane.provider_params.clone();
        // The family's own augmentations: chat-completions requests
        // `stream_options.include_usage` (the trailing usage chunk the
        // grammar expects); Messages usage rides `message_start`/
        // `message_delta` natively — `stream_options` is outside the
        // Messages `plan_schema` and never inserted.
        if self.family == Family::ChatCompletions && !params.contains_key("stream_options") {
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
                streaming: true,
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
        }
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
        message: &hh_gateway::message::ModelMessage,
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
        let addr = hh_identity::idp::address(&bytes, "application/json");
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

impl ModelPort for GatewayModel {
    fn requested_members(
        &mut self,
        mc: &str,
        _request: &Json,
        prefix: &[EventEnvelope],
    ) -> Vec<(&'static str, Json)> {
        match self.build_plan(mc, prefix) {
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
                self.pending.insert(mc.to_string(), p);
                vec![
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
                ]
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
