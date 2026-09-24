//! `WireDialect/1` — the `ModelBoundary`'s contract with a wire format
//! (§5b.1 §3/§4; ADR-0119 d.5): a canonical record the gateway loads; every
//! provider-specific fact lives in its members, never in gateway code
//! (T-LCD-01).
//!
//! A dialect carries no model knowledge. Its `rules[]` are its assumption
//! surface — every textual rule (`error_code_to_class`,
//! `error_text_to_class`, `notice_map`, `reasoning_in_text`,
//! `metadata_frame`) carries a complete [`DebtRecord`], and the document
//! refuses to load when any is incomplete (T-LCD-05 reflexive — no `unknown`
//! escape at load).

use hh_wire::json::Json;

use crate::cache::CacheSemantics;
use crate::errors::CodecError;
use crate::vocab::{ModelErrorClass, NoticeKind, RetryClass, StopReason, UsageArrival};

/// `framing ∈ {http_json, sse, websocket, jsonrpc_stdio}` — the wire framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Framing {
    /// Request/response JSON over HTTP.
    HttpJson,
    /// Server-sent events.
    Sse,
    /// WebSocket frames.
    Websocket,
    /// JSON-RPC over stdio.
    JsonrpcStdio,
}

impl Framing {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Framing::HttpJson => "http_json",
            Framing::Sse => "sse",
            Framing::Websocket => "websocket",
            Framing::JsonrpcStdio => "jsonrpc_stdio",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Framing> {
        Some(match s {
            "http_json" => Framing::HttpJson,
            "sse" => Framing::Sse,
            "websocket" => Framing::Websocket,
            "jsonrpc_stdio" => Framing::JsonrpcStdio,
            _ => return None,
        })
    }
}

/// `auth_kinds` — how a credential binds to the wire (never a credential).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthKind {
    /// No credential.
    None,
    /// `Authorization: Bearer …`.
    Bearer,
    /// An api-key header.
    ApiKeyHeader,
    /// OAuth.
    Oauth,
    /// AWS SigV4.
    Sigv4,
    /// Command-backed (a brokered external command).
    CommandBacked,
}

impl AuthKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::Bearer => "bearer",
            AuthKind::ApiKeyHeader => "api_key_header",
            AuthKind::Oauth => "oauth",
            AuthKind::Sigv4 => "sigv4",
            AuthKind::CommandBacked => "command_backed",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<AuthKind> {
        Some(match s {
            "none" => AuthKind::None,
            "bearer" => AuthKind::Bearer,
            "api_key_header" => AuthKind::ApiKeyHeader,
            "oauth" => AuthKind::Oauth,
            "sigv4" => AuthKind::Sigv4,
            "command_backed" => AuthKind::CommandBacked,
            _ => return None,
        })
    }
}

/// `retry_after_sources` — where the dialect reads a `Retry-After` hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetryAfterSource {
    /// The response headers.
    Header,
    /// The response body.
    Body,
}

impl RetryAfterSource {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RetryAfterSource::Header => "header",
            RetryAfterSource::Body => "body",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RetryAfterSource> {
        Some(match s {
            "header" => RetryAfterSource::Header,
            "body" => RetryAfterSource::Body,
            _ => return None,
        })
    }
}

/// `Json` array access (`hh_wire::Json` carries no `as_arr` — pattern-match).
pub(crate) fn json_arr(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

/// `usage_mapping_default ∈ {prefer_provider, prefer_estimate, provider_only}`
/// (ADR-0119 d.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageMappingDefault {
    /// Provider usage preferred; estimate when absent.
    PreferProvider,
    /// Estimate preferred; provider fact when absent.
    PreferEstimate,
    /// Only provider usage counts.
    ProviderOnly,
}

impl UsageMappingDefault {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UsageMappingDefault::PreferProvider => "prefer_provider",
            UsageMappingDefault::PreferEstimate => "prefer_estimate",
            UsageMappingDefault::ProviderOnly => "provider_only",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<UsageMappingDefault> {
        Some(match s {
            "prefer_provider" => UsageMappingDefault::PreferProvider,
            "prefer_estimate" => UsageMappingDefault::PreferEstimate,
            "provider_only" => UsageMappingDefault::ProviderOnly,
            _ => return None,
        })
    }
}

/// `plan_schema` — the dialect's `provider_params` schema: the closed set of
/// keys a plan may carry plus those it must.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanSchema {
    /// The keys `provider_params` may carry.
    pub allowed_keys: Vec<String>,
    /// The keys it must carry.
    pub required_keys: Vec<String>,
}

impl PlanSchema {
    /// Whether `key` is admitted.
    pub fn admits(&self, key: &str) -> bool {
        self.allowed_keys.is_empty() || self.allowed_keys.iter().any(|k| k == key)
    }

    /// The missing required keys.
    pub fn missing<'a>(&'a self, present: &[String]) -> Vec<&'a str> {
        self.required_keys
            .iter()
            .filter(|k| !present.iter().any(|p| p == *k))
            .map(String::as_str)
            .collect()
    }
}

/// `UsageMapping` — an optional per-dialect vector-shape declaration; the
/// absent members default to `usage_mapping_default` (ADR-0119 d.3).
#[derive(Debug, Clone, PartialEq)]
pub struct UsageMapping {
    /// How `cache_read` maps.
    pub cache_read: Option<UsageMappingDefault>,
    /// How `cache_write` maps.
    pub cache_write: Option<UsageMappingDefault>,
    /// How `thinking` maps.
    pub thinking: Option<UsageMappingDefault>,
    /// How `reasoning_output` maps.
    pub reasoning_output: Option<UsageMappingDefault>,
}

/// `DialectRule` — a boundary rule with its debt record (ADR-0119 d.5). The
/// `condition` (a member name / status / regex coordinate) matches the *frame*,
/// never a model id; `param` carries the rule's target (the class/reason/
/// kind/label).
#[derive(Debug, Clone, PartialEq)]
pub enum DialectRule {
    /// `status_to_class` — an HTTP status maps to a `ModelErrorClass`.
    StatusToClass {
        /// The HTTP status.
        status: u32,
        /// The class.
        class: ModelErrorClass,
    },
    /// `error_code_to_class` — a provider error code maps to a class. Carries
    /// a debt record (non-normative evidence — provider docs).
    ErrorCodeToClass {
        /// The provider error code.
        code: String,
        /// The class.
        class: ModelErrorClass,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
    /// `error_text_to_class` — a body-text pattern maps to a class. Carries a
    /// debt record — the only path to `transcript_incompatible`.
    ErrorTextToClass {
        /// The regex.
        pattern: String,
        /// The class.
        class: ModelErrorClass,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
    /// `stop_reason_map` — a provider stop-reason string maps to the closed
    /// `StopReason`.
    StopReasonMap {
        /// The provider spelling.
        provider: String,
        /// The canonical reason.
        reason: StopReason,
    },
    /// `retry_class_override` — the only narrowing a rule may do is
    /// `transient → permanent` (ADR-0119 d.2; widening is refused at load).
    RetryClassOverride {
        /// The class being narrowed.
        class: ModelErrorClass,
        /// The narrowed retry class.
        retry_class: RetryClass,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
    /// `reasoning_in_text` — a model family leaves reasoning in text blocks;
    /// `param` is the family pattern and `condition` the extraction rule.
    /// Carries a debt record.
    ReasoningInText {
        /// The family/condition pattern.
        pattern: String,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
    /// `metadata_frame` — a frame kind the dialect may emit mid-stream;
    /// `param` names the frame kind.
    MetadataFrame {
        /// The frame kind.
        kind: String,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
    /// `notice_map` — a provider notice frame maps to a `NoticeKind`.
    /// Carries a debt record.
    NoticeMap {
        /// The provider notice spelling.
        provider: String,
        /// The canonical kind.
        kind: NoticeKind,
        /// The assumption-debt record.
        debt: DebtRecord,
    },
}

/// `DebtRecord{what_we_assume, how_we_could_be_wrong, evidence,
/// detection_signal, removal_test}` — the required debt shape (T-LCD-05
/// reflexive; the kernel's own 5-member form, `unknown` in no member).
#[derive(Debug, Clone, PartialEq)]
pub struct DebtRecord {
    /// What the rule assumes.
    pub what_we_assume: String,
    /// How the assumption could be wrong.
    pub how_we_could_be_wrong: String,
    /// `evidence[]` — sources pinned by content address and access date.
    pub evidence: Vec<DebtEvidence>,
    /// The live signal that detects the assumption's failure.
    pub detection_signal: String,
    /// The canonical `idp` id of a committed acceptance test — a live
    /// executable reference, never prose (ADR-0119 d.5).
    pub removal_test: String,
}

/// One `evidence[]` row.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtEvidence {
    /// The evidence reference (a URL or a content address).
    pub reference: String,
    /// The access date (`YYYY-MM-DD`).
    pub accessed: String,
}

impl DebtRecord {
    /// `true` when every member is present and non-empty — a `true` return is
    /// the *only* load admission (no `unknown` escape).
    pub fn is_complete(&self) -> bool {
        !self.what_we_assume.is_empty()
            && !self.how_we_could_be_wrong.is_empty()
            && !self.evidence.is_empty()
            && !self.detection_signal.is_empty()
            && !self.removal_test.is_empty()
    }
}

impl DialectRule {
    /// The rule kind spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            DialectRule::StatusToClass { .. } => "status_to_class",
            DialectRule::ErrorCodeToClass { .. } => "error_code_to_class",
            DialectRule::ErrorTextToClass { .. } => "error_text_to_class",
            DialectRule::StopReasonMap { .. } => "stop_reason_map",
            DialectRule::RetryClassOverride { .. } => "retry_class_override",
            DialectRule::ReasoningInText { .. } => "reasoning_in_text",
            DialectRule::MetadataFrame { .. } => "metadata_frame",
            DialectRule::NoticeMap { .. } => "notice_map",
        }
    }

    /// The rule's debt record, when the kind carries one.
    pub fn debt(&self) -> Option<&DebtRecord> {
        match self {
            DialectRule::ErrorCodeToClass { debt, .. }
            | DialectRule::ErrorTextToClass { debt, .. }
            | DialectRule::RetryClassOverride { debt, .. }
            | DialectRule::ReasoningInText { debt, .. }
            | DialectRule::MetadataFrame { debt, .. }
            | DialectRule::NoticeMap { debt, .. } => Some(debt),
            _ => None,
        }
    }

    /// A stable rule id for debt-error reporting.
    pub fn rule_id(&self) -> String {
        match self {
            DialectRule::StatusToClass { status, .. } => {
                format!("status_to_class({status})")
            }
            DialectRule::ErrorCodeToClass { code, .. } => format!("error_code_to_class({code})"),
            DialectRule::ErrorTextToClass { pattern, .. } => {
                format!("error_text_to_class({pattern})")
            }
            DialectRule::StopReasonMap { provider, .. } => format!("stop_reason_map({provider})"),
            DialectRule::RetryClassOverride { class, .. } => {
                format!("retry_class_override({})", class.as_str())
            }
            DialectRule::ReasoningInText { pattern, .. } => {
                format!("reasoning_in_text({pattern})")
            }
            DialectRule::MetadataFrame { kind, .. } => format!("metadata_frame({kind})"),
            DialectRule::NoticeMap { provider, .. } => format!("notice_map({provider})"),
        }
    }
}

/// `WireDialect/1` — the descriptor (ADR-0119 d.5). The type is closed over the
/// sealed members; an unknown member or an incomplete textual-rule debt record
/// fails the load.
#[derive(Debug, Clone, PartialEq)]
pub struct WireDialect {
    /// `dialect_id` — the descriptor's name.
    pub dialect_id: String,
    /// `version` — the dialect version (a sum is closed per version).
    pub version: String,
    /// `framing`.
    pub framing: Framing,
    /// `plan_schema` — the `provider_params` schema.
    pub plan_schema: PlanSchema,
    /// `envelope_fields[]` — envelope member names the dialect wraps plans in.
    pub envelope_fields: Vec<String>,
    /// `auth_kinds[]`.
    pub auth_kinds: Vec<AuthKind>,
    /// `endpoint_allowlist_ref` — the containment policy row reference.
    pub endpoint_allowlist_ref: String,
    /// `streaming`.
    pub streaming: bool,
    /// `usage_arrival`.
    pub usage_arrival: UsageArrival,
    /// `usage_mapping_default`.
    pub usage_mapping_default: UsageMappingDefault,
    /// `usage_mapping` — per-role overrides.
    pub usage_mapping: Option<UsageMapping>,
    /// `cache_semantics` — the `CacheSemantics` sum (data, K1-1).
    pub cache_semantics: CacheSemantics,
    /// `cache_state_visible`.
    pub cache_state_visible: bool,
    /// `count_tokens` — the dialect's count-tokens member (the `Estimate`
    /// surface; a `counting` estimate's `cost_units` are the count call's own
    /// usage — ADR-0119 d.3).
    pub count_tokens: Option<String>,
    /// `deferred_requests` — a `Deferred` variant is legal only when `true`.
    pub deferred_requests: bool,
    /// `model_listing` — the endpoint's model-listing member (the `discover`
    /// primitive's source).
    pub model_listing: Option<String>,
    /// `snapshot_id_exposed` — the envelope member carrying a snapshot id.
    pub snapshot_id_exposed: Option<String>,
    /// `retry_after_sources`.
    pub retry_after_sources: Vec<RetryAfterSource>,
    /// `retry_after_cap_ms` — a server hint above this fails the call
    /// (R-RT-4).
    pub retry_after_cap_ms: u64,
    /// `stream_idle_timeout_ms` — a `gap_ms` above this is
    /// `stream_idle_timeout` (G7).
    pub stream_idle_timeout_ms: u64,
    /// `prefix_affecting_params[]` — the dialect-listed parameters that must
    /// enter `static_hash` (LC-5).
    pub prefix_affecting_params: Vec<String>,
    /// `rules[]` — the dialect's boundary rules (each textual rule with its
    /// complete `DebtRecord`).
    pub rules: Vec<DialectRule>,
}

impl WireDialect {
    /// The descriptor's content id (`dialect.1` — `idp/1` over the canonical
    /// body, minus the id members — the same identity discipline as every
    /// descriptor).
    pub fn content_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(m) = &mut j {
            m.remove("dialect_id");
        }
        hh_identity::idp_id("dialect.1", j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("schema".into(), Json::str("hh.wire_dialect/1"));
        m.insert("dialect_id".into(), Json::str(self.dialect_id.clone()));
        m.insert("version".into(), Json::str(self.version.clone()));
        m.insert("framing".into(), Json::str(self.framing.as_str()));
        m.insert(
            "plan_schema".into(),
            Json::obj([
                (
                    "allowed_keys",
                    Json::Arr(
                        self.plan_schema
                            .allowed_keys
                            .iter()
                            .map(|k| Json::str(k.clone()))
                            .collect(),
                    ),
                ),
                (
                    "required_keys",
                    Json::Arr(
                        self.plan_schema
                            .required_keys
                            .iter()
                            .map(|k| Json::str(k.clone()))
                            .collect(),
                    ),
                ),
            ]),
        );
        m.insert(
            "envelope_fields".into(),
            Json::Arr(
                self.envelope_fields
                    .iter()
                    .map(|f| Json::str(f.clone()))
                    .collect(),
            ),
        );
        m.insert(
            "auth_kinds".into(),
            Json::Arr(
                self.auth_kinds
                    .iter()
                    .map(|k| Json::str(k.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "endpoint_allowlist_ref".into(),
            Json::str(self.endpoint_allowlist_ref.clone()),
        );
        m.insert("streaming".into(), Json::Bool(self.streaming));
        m.insert(
            "usage_arrival".into(),
            Json::str(self.usage_arrival.as_str()),
        );
        m.insert(
            "usage_mapping_default".into(),
            Json::str(self.usage_mapping_default.as_str()),
        );
        if let Some(um) = &self.usage_mapping {
            let mut u = std::collections::BTreeMap::new();
            if let Some(v) = &um.cache_read {
                u.insert("cache_read".into(), Json::str(v.as_str()));
            }
            if let Some(v) = &um.cache_write {
                u.insert("cache_write".into(), Json::str(v.as_str()));
            }
            if let Some(v) = &um.thinking {
                u.insert("thinking".into(), Json::str(v.as_str()));
            }
            if let Some(v) = &um.reasoning_output {
                u.insert("reasoning_output".into(), Json::str(v.as_str()));
            }
            m.insert("usage_mapping".into(), Json::Obj(u));
        }
        m.insert("cache_semantics".into(), self.cache_semantics.to_json());
        m.insert(
            "cache_state_visible".into(),
            Json::Bool(self.cache_state_visible),
        );
        match &self.count_tokens {
            Some(ct) => {
                m.insert("count_tokens".into(), Json::str(ct.clone()));
            }
            None => {
                m.insert("count_tokens".into(), Json::Null);
            }
        }
        m.insert(
            "deferred_requests".into(),
            Json::Bool(self.deferred_requests),
        );
        match &self.model_listing {
            Some(ml) => {
                m.insert("model_listing".into(), Json::str(ml.clone()));
            }
            None => {
                m.insert("model_listing".into(), Json::Null);
            }
        }
        match &self.snapshot_id_exposed {
            Some(s) => {
                m.insert("snapshot_id_exposed".into(), Json::str(s.clone()));
            }
            None => {
                m.insert("snapshot_id_exposed".into(), Json::Null);
            }
        }
        m.insert(
            "retry_after_sources".into(),
            Json::Arr(
                self.retry_after_sources
                    .iter()
                    .map(|s| Json::str(s.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "retry_after_cap_ms".into(),
            Json::Int(self.retry_after_cap_ms as i64),
        );
        m.insert(
            "stream_idle_timeout_ms".into(),
            Json::Int(self.stream_idle_timeout_ms as i64),
        );
        m.insert(
            "prefix_affecting_params".into(),
            Json::Arr(
                self.prefix_affecting_params
                    .iter()
                    .map(|p| Json::str(p.clone()))
                    .collect(),
            ),
        );
        m.insert(
            "rules".into(),
            Json::Arr(self.rules.iter().map(rule_to_json).collect()),
        );
        Json::Obj(m)
    }

    /// Strict decode — closed members, closed sums, and every debt-carrying
    /// rule must carry a complete [`DebtRecord`] (`IncompleteDebt` — the
    /// document fails to load).
    pub fn from_json(j: &Json) -> Result<WireDialect, CodecError> {
        const REC: &str = "WireDialect/1";
        let members: Vec<&String> = match j {
            Json::Obj(m) => m.keys().collect(),
            _ => {
                return Err(CodecError::TypeMismatch {
                    member: REC.into(),
                    expected: "object",
                })
            }
        };
        for k in members {
            match k.as_str() {
                "schema"
                | "dialect_id"
                | "version"
                | "framing"
                | "plan_schema"
                | "envelope_fields"
                | "auth_kinds"
                | "endpoint_allowlist_ref"
                | "streaming"
                | "usage_arrival"
                | "usage_mapping_default"
                | "usage_mapping"
                | "cache_semantics"
                | "cache_state_visible"
                | "count_tokens"
                | "deferred_requests"
                | "model_listing"
                | "snapshot_id_exposed"
                | "retry_after_sources"
                | "retry_after_cap_ms"
                | "stream_idle_timeout_ms"
                | "prefix_affecting_params"
                | "rules" => {}
                _ => {
                    return Err(CodecError::BadMember {
                        record: REC,
                        member: k.clone(),
                    })
                }
            }
        }
        let req_str = |k: &'static str| -> Result<String, CodecError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or(CodecError::MissingMember {
                    record: REC,
                    member: k,
                })
        };
        let str_list = |k: &str| -> Result<Vec<String>, CodecError> {
            match j.get(k) {
                Some(Json::Arr(items)) => Ok(items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()),
                Some(Json::Null) | None => Ok(Vec::new()),
                _ => Err(CodecError::TypeMismatch {
                    member: k.to_string(),
                    expected: "string array",
                }),
            }
        };
        let req_u64 = |k: &'static str| -> Result<u64, CodecError> {
            j.get(k)
                .and_then(Json::as_int)
                .map(|v| v as u64)
                .ok_or(CodecError::MissingMember {
                    record: REC,
                    member: k,
                })
        };
        let req_bool = |k: &'static str| -> Result<bool, CodecError> {
            match j.get(k) {
                Some(Json::Bool(b)) => Ok(*b),
                _ => Err(CodecError::MissingMember {
                    record: REC,
                    member: k,
                }),
            }
        };
        let plan_schema = match j.get("plan_schema") {
            Some(ps @ Json::Obj(_)) => PlanSchema {
                allowed_keys: ps
                    .get("allowed_keys")
                    .and_then(json_arr)
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                required_keys: ps
                    .get("required_keys")
                    .and_then(json_arr)
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            _ => PlanSchema::default(),
        };
        let usage_mapping = j.get("usage_mapping").and_then(|u| {
            if let Json::Obj(_) = u {
                let m = |k: &str| {
                    u.get(k)
                        .and_then(Json::as_str)
                        .and_then(UsageMappingDefault::parse)
                };
                Some(UsageMapping {
                    cache_read: m("cache_read"),
                    cache_write: m("cache_write"),
                    thinking: m("thinking"),
                    reasoning_output: m("reasoning_output"),
                })
            } else {
                None
            }
        });
        let rules = match j.get("rules") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, r)| rule_from_json(r, &format!("rules[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let dialect = WireDialect {
            dialect_id: req_str("dialect_id")?,
            version: req_str("version")?,
            framing: Framing::parse(&req_str("framing")?).ok_or_else(|| {
                CodecError::UnknownVariant {
                    member: "framing".into(),
                    value: String::new(),
                }
            })?,
            plan_schema,
            envelope_fields: str_list("envelope_fields")?,
            auth_kinds: str_list("auth_kinds")?
                .iter()
                .map(|k| {
                    AuthKind::parse(k).ok_or_else(|| CodecError::UnknownVariant {
                        member: "auth_kinds".into(),
                        value: k.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            endpoint_allowlist_ref: req_str("endpoint_allowlist_ref")?,
            streaming: req_bool("streaming")?,
            usage_arrival: UsageArrival::parse(&req_str("usage_arrival")?).ok_or_else(|| {
                CodecError::UnknownVariant {
                    member: "usage_arrival".into(),
                    value: String::new(),
                }
            })?,
            usage_mapping_default: UsageMappingDefault::parse(&req_str("usage_mapping_default")?)
                .ok_or_else(|| CodecError::UnknownVariant {
                member: "usage_mapping_default".into(),
                value: String::new(),
            })?,
            usage_mapping,
            cache_semantics: CacheSemantics::from_json(
                j.get("cache_semantics").unwrap_or(&Json::Null),
                "cache_semantics",
            )?,
            cache_state_visible: req_bool("cache_state_visible")?,
            count_tokens: j
                .get("count_tokens")
                .and_then(Json::as_str)
                .map(str::to_string),
            deferred_requests: req_bool("deferred_requests")?,
            model_listing: j
                .get("model_listing")
                .and_then(Json::as_str)
                .map(str::to_string),
            snapshot_id_exposed: j
                .get("snapshot_id_exposed")
                .and_then(Json::as_str)
                .map(str::to_string),
            retry_after_sources: str_list("retry_after_sources")?
                .iter()
                .map(|s| {
                    RetryAfterSource::parse(s).ok_or_else(|| CodecError::UnknownVariant {
                        member: "retry_after_sources".into(),
                        value: s.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            retry_after_cap_ms: req_u64("retry_after_cap_ms")?,
            stream_idle_timeout_ms: req_u64("stream_idle_timeout_ms")?,
            prefix_affecting_params: str_list("prefix_affecting_params")?,
            rules,
        };
        // The debt check — a rule carrying an incomplete DebtRecord fails the
        // load (T-LCD-05 reflexive; no `unknown` escape).
        for rule in &dialect.rules {
            if let Some(debt) = rule.debt() {
                if !debt.is_complete() {
                    return Err(CodecError::IncompleteDebt {
                        rule_id: rule.rule_id(),
                    });
                }
            }
            // A retry-class override may only narrow transient → permanent.
            if let DialectRule::RetryClassOverride {
                class, retry_class, ..
            } = rule
            {
                if !(class.default_retry_class() == RetryClass::Transient
                    && *retry_class == RetryClass::Permanent)
                {
                    return Err(CodecError::NonCanonical {
                        member: format!("rules.{}", rule.rule_id()),
                        detail: "a retry_class_override may only narrow transient → permanent"
                            .into(),
                    });
                }
            }
        }
        Ok(dialect)
    }
}

fn rule_to_json(r: &DialectRule) -> Json {
    let debt_json = |d: &DebtRecord| {
        Json::obj([
            ("what_we_assume", Json::str(d.what_we_assume.clone())),
            (
                "how_we_could_be_wrong",
                Json::str(d.how_we_could_be_wrong.clone()),
            ),
            (
                "evidence",
                Json::Arr(
                    d.evidence
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("reference", Json::str(e.reference.clone())),
                                ("accessed", Json::str(e.accessed.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("detection_signal", Json::str(d.detection_signal.clone())),
            ("removal_test", Json::str(d.removal_test.clone())),
        ])
    };
    match r {
        DialectRule::StatusToClass { status, class } => Json::obj([
            ("kind", Json::str("status_to_class")),
            ("status", Json::Int(*status as i64)),
            ("class", Json::str(class.as_str())),
        ]),
        DialectRule::ErrorCodeToClass { code, class, debt } => Json::obj([
            ("kind", Json::str("error_code_to_class")),
            ("code", Json::str(code.clone())),
            ("class", Json::str(class.as_str())),
            ("debt", debt_json(debt)),
        ]),
        DialectRule::ErrorTextToClass {
            pattern,
            class,
            debt,
        } => Json::obj([
            ("kind", Json::str("error_text_to_class")),
            ("pattern", Json::str(pattern.clone())),
            ("class", Json::str(class.as_str())),
            ("debt", debt_json(debt)),
        ]),
        DialectRule::StopReasonMap { provider, reason } => Json::obj([
            ("kind", Json::str("stop_reason_map")),
            ("provider", Json::str(provider.clone())),
            ("reason", Json::str(reason.as_str())),
        ]),
        DialectRule::RetryClassOverride {
            class,
            retry_class,
            debt,
        } => Json::obj([
            ("kind", Json::str("retry_class_override")),
            ("class", Json::str(class.as_str())),
            ("retry_class", Json::str(retry_class.as_str())),
            ("debt", debt_json(debt)),
        ]),
        DialectRule::ReasoningInText { pattern, debt } => Json::obj([
            ("kind", Json::str("reasoning_in_text")),
            ("pattern", Json::str(pattern.clone())),
            ("debt", debt_json(debt)),
        ]),
        DialectRule::MetadataFrame { kind, debt } => Json::obj([
            ("kind", Json::str("metadata_frame")),
            ("frame_kind", Json::str(kind.clone())),
            ("debt", debt_json(debt)),
        ]),
        DialectRule::NoticeMap {
            provider,
            kind,
            debt,
        } => Json::obj([
            ("kind", Json::str("notice_map")),
            ("provider", Json::str(provider.clone())),
            ("notice_kind", Json::str(kind.as_str())),
            ("debt", debt_json(debt)),
        ]),
    }
}

fn rule_from_json(j: &Json, path: &str) -> Result<DialectRule, CodecError> {
    let kind = j
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| CodecError::TypeMismatch {
            member: format!("{path}.kind"),
            expected: "rule kind",
        })?;
    let class_of = |k: &str| -> Result<ModelErrorClass, CodecError> {
        ModelErrorClass::parse(j.get(k).and_then(Json::as_str).ok_or_else(|| {
            CodecError::TypeMismatch {
                member: format!("{path}.{k}"),
                expected: "ModelErrorClass spelling",
            }
        })?)
        .ok_or_else(|| CodecError::UnknownVariant {
            member: format!("{path}.{k}"),
            value: String::new(),
        })
    };
    let debt_of = || -> Result<DebtRecord, CodecError> {
        let d = j.get("debt").ok_or(CodecError::MissingMember {
            record: "DialectRule",
            member: "debt",
        })?;
        Ok(DebtRecord {
            what_we_assume: d
                .get("what_we_assume")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            how_we_could_be_wrong: d
                .get("how_we_could_be_wrong")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            evidence: d
                .get("evidence")
                .and_then(json_arr)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|e| {
                            Some(DebtEvidence {
                                reference: e.get("reference")?.as_str()?.to_string(),
                                accessed: e.get("accessed")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            detection_signal: d
                .get("detection_signal")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            removal_test: d
                .get("removal_test")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        })
    };
    match kind {
        "status_to_class" => Ok(DialectRule::StatusToClass {
            status: j.get("status").and_then(Json::as_int).ok_or({
                CodecError::MissingMember {
                    record: "DialectRule",
                    member: "status",
                }
            })? as u32,
            class: class_of("class")?,
        }),
        "error_code_to_class" => Ok(DialectRule::ErrorCodeToClass {
            code: j
                .get("code")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            class: class_of("class")?,
            debt: debt_of()?,
        }),
        "error_text_to_class" => Ok(DialectRule::ErrorTextToClass {
            pattern: j
                .get("pattern")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            class: class_of("class")?,
            debt: debt_of()?,
        }),
        "stop_reason_map" => Ok(DialectRule::StopReasonMap {
            provider: j
                .get("provider")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            reason: StopReason::parse(j.get("reason").and_then(Json::as_str).unwrap_or("unknown"))
                .ok_or_else(|| CodecError::UnknownVariant {
                    member: format!("{path}.reason"),
                    value: String::new(),
                })?,
        }),
        "retry_class_override" => Ok(DialectRule::RetryClassOverride {
            class: class_of("class")?,
            retry_class: RetryClass::parse(
                j.get("retry_class").and_then(Json::as_str).unwrap_or(""),
            )
            .ok_or_else(|| CodecError::UnknownVariant {
                member: format!("{path}.retry_class"),
                value: String::new(),
            })?,
            debt: debt_of()?,
        }),
        "reasoning_in_text" => Ok(DialectRule::ReasoningInText {
            pattern: j
                .get("pattern")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            debt: debt_of()?,
        }),
        "metadata_frame" => Ok(DialectRule::MetadataFrame {
            kind: j
                .get("frame_kind")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            debt: debt_of()?,
        }),
        "notice_map" => Ok(DialectRule::NoticeMap {
            provider: j
                .get("provider")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            kind: NoticeKind::parse(
                j.get("notice_kind")
                    .and_then(Json::as_str)
                    .unwrap_or("other"),
            )
            .ok_or_else(|| CodecError::UnknownVariant {
                member: format!("{path}.notice_kind"),
                value: String::new(),
            })?,
            debt: debt_of()?,
        }),
        other => Err(CodecError::UnknownVariant {
            member: format!("{path}.kind"),
            value: other.to_string(),
        }),
    }
}
