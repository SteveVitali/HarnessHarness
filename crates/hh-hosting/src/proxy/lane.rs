//! `proxy::lane` — the declared lane document `hh.proxy.lane/1`
//! (§4.2's route table + field policy + `GenParamPolicy` pins; the
//! behaviour per field is *declared*, never implicit — REQ-XC-106).

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::adapter_b::GenParamPolicy;

/// The downstream surfaces the §4.2 table admits with an upstream POST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// `POST /v1/chat/completions` (OpenAI chat dialect).
    OpenAiChat,
    /// `POST /v1/messages` (Anthropic Messages dialect).
    AnthropicMessages,
}

impl Surface {
    /// The receipt + lane-doc spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::OpenAiChat => "openai.chat",
            Surface::AnthropicMessages => "anthropic.messages",
        }
    }

    /// The surface's effort pin path (§4.2 column "effort pinned at").
    pub fn effort_path(self) -> &'static str {
        match self {
            Surface::OpenAiChat => "reasoning_effort",
            Surface::AnthropicMessages => "output_config.effort",
        }
    }

    /// The upstream op `forward_raw` joins to the endpoint.
    pub fn upstream_op(self) -> &'static str {
        match self {
            Surface::OpenAiChat => "chat/completions",
            Surface::AnthropicMessages => "messages",
        }
    }

    /// Parse the spelling.
    pub fn parse(s: &str) -> Option<Surface> {
        match s {
            "openai.chat" => Some(Surface::OpenAiChat),
            "anthropic.messages" => Some(Surface::AnthropicMessages),
            _ => None,
        }
    }
}

/// A matched route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A forwarded model surface.
    Surface(Surface),
    /// `POST /v1/messages/count_tokens` — typed refusal, never forwarded.
    CountTokens,
    /// `GET /v1/models` — forwarded verbatim (no body).
    Models,
    /// Everything else — 404 `refused_route`.
    Refused,
}

/// One field-policy operation (§4.2 Field policy column).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldOp {
    /// Strip the top-level member, recording `(path, value_sha256)`.
    Strip,
    /// Strip the member at every depth (`cache_control` — "all paths").
    StripRecursive,
    /// Strip when the value is provider-default `auto`
    /// (`"auto"` or `{type:"auto"}`), refuse otherwise.
    StripAutoElseRefuse,
    /// Refuse when the integer value exceeds the bound (`n > 1`);
    /// in-bound provider-default equivalents strip with notice.
    RefuseIfGt(i64),
    /// Refuse whenever the member is present.
    Refuse,
    /// Forward verbatim and record `not_enforced` (`budget_tokens`).
    ForwardRecorded,
}

/// One declared field rule: `{member, op}`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRule {
    /// The member name the rule binds (top-level; `strip_recursive`
    /// applies at any depth).
    pub member: String,
    /// The operation.
    pub op: FieldOp,
}

/// The step-2 reservation form. Until R6.11's §4.4 formula lands, the
/// only admitted form is the fixture lane's declared bound — any other
/// lane refuses typed at ledger step 2 (`unsupported{reservation_formula}`).
#[derive(Debug, Clone, PartialEq)]
pub enum Reservation {
    /// A declared per-call bound in µ$ — exists only on fixture lanes.
    FixtureBound {
        /// The bound reserved per call on the slot's slice.
        micros: i64,
    },
    /// No formula declared — step-2 typed refusal.
    Unsupported,
}

/// `hh.proxy.lane/1` — the declared lane: upstream coordinate, release
/// mode, field policy, pins, params scope, reservation form.
#[derive(Debug, Clone)]
pub struct ProxyLane {
    /// The lane id (a receipt + manifest fact).
    pub lane_id: String,
    /// The pinned provider model (`model` pin's applied value).
    pub model: String,
    /// The upstream endpoint ref (`forward_raw`'s dial coordinate —
    /// `https://ollama.com/v1` live, an `http://127.0.0.1:<port>`
    /// fixture offline).
    pub endpoint_ref: String,
    /// The upstream host fact recorded on receipts.
    pub upstream_host: String,
    /// The release mode — `buffered` until R6.11.
    pub release: String,
    /// `surface|"*" → rules`, applied `*` first then surface.
    pub field_policy: BTreeMap<String, Vec<FieldRule>>,
    /// The generation-parameter policy (AR-10: `pin` path-qualified,
    /// `params_scope` per surface).
    pub gen_param_policy: GenParamPolicy,
    /// The step-2 reservation form.
    pub reservation: Reservation,
    /// The upstream concurrency bound (the account's 3-way limit).
    pub upstream_concurrency: usize,
    /// The runtime root budget id (set at `configure`; `""` until then).
    pub root_budget_id: String,
}

impl ProxyLane {
    /// The §4.2 program lane — the closed field-policy matrix and the
    /// `model` + effort-path pins, over the declared upstream.
    /// `reservation` is caller-declared (fixture lanes pass a bound).
    pub fn program(
        lane_id: &str,
        endpoint_ref: &str,
        upstream_host: &str,
        model: &str,
        reservation: Reservation,
    ) -> ProxyLane {
        let mut field_policy: BTreeMap<String, Vec<FieldRule>> = BTreeMap::new();
        let rule = |member: &str, op: FieldOp| FieldRule {
            member: member.to_string(),
            op,
        };
        field_policy.insert(
            "*".to_string(),
            vec![
                rule("previous_response_id", FieldOp::Refuse),
                rule("conversation", FieldOp::Refuse),
                rule("store", FieldOp::Refuse),
            ],
        );
        field_policy.insert(
            Surface::OpenAiChat.as_str().to_string(),
            vec![
                rule("tool_choice", FieldOp::StripAutoElseRefuse),
                rule("n", FieldOp::RefuseIfGt(1)),
                rule("logprobs", FieldOp::Refuse),
                rule("top_logprobs", FieldOp::Refuse),
                rule("logit_bias", FieldOp::Refuse),
            ],
        );
        field_policy.insert(
            Surface::AnthropicMessages.as_str().to_string(),
            vec![
                rule("cache_control", FieldOp::StripRecursive),
                rule("metadata", FieldOp::Strip),
                rule("tool_choice", FieldOp::StripAutoElseRefuse),
                rule("n", FieldOp::RefuseIfGt(1)),
                rule("logprobs", FieldOp::Refuse),
                rule("budget_tokens", FieldOp::ForwardRecorded),
            ],
        );
        let mut pin = BTreeMap::new();
        pin.insert("model".to_string(), Json::str(model));
        let mut params_scope = BTreeMap::new();
        let chat_scope = [
            "temperature",
            "top_p",
            "seed",
            "max_tokens",
            "max_completion_tokens",
            "stop",
            "presence_penalty",
            "frequency_penalty",
        ];
        let msg_scope = [
            "temperature",
            "top_p",
            "top_k",
            "max_tokens",
            "stop_sequences",
            "thinking.budget_tokens",
            "budget_tokens",
        ];
        params_scope.insert(
            Surface::OpenAiChat.as_str().to_string(),
            chat_scope.iter().map(|s| s.to_string()).collect(),
        );
        params_scope.insert(
            Surface::AnthropicMessages.as_str().to_string(),
            msg_scope.iter().map(|s| s.to_string()).collect(),
        );
        ProxyLane {
            lane_id: lane_id.to_string(),
            model: model.to_string(),
            endpoint_ref: endpoint_ref.to_string(),
            upstream_host: upstream_host.to_string(),
            release: "buffered".to_string(),
            field_policy,
            gen_param_policy: GenParamPolicy {
                forward: chat_scope
                    .iter()
                    .chain(msg_scope.iter())
                    .map(|s| s.to_string())
                    .collect(),
                default: BTreeMap::new(),
                pin,
                params_scope,
            },
            reservation,
            upstream_concurrency: 3,
            root_budget_id: String::new(),
        }
    }

    /// The declared form (recorded on the manifest's `extra` and the
    /// conformance fixtures' input).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh.proxy.lane/1")),
            ("lane_id", Json::str(&self.lane_id)),
            ("model", Json::str(&self.model)),
            ("endpoint_ref", Json::str(&self.endpoint_ref)),
            ("upstream_host", Json::str(&self.upstream_host)),
            ("release", Json::str(&self.release)),
            (
                "field_policy",
                Json::Obj(
                    self.field_policy
                        .iter()
                        .map(|(s, rules)| {
                            (
                                s.clone(),
                                Json::Arr(
                                    rules
                                        .iter()
                                        .map(|r| {
                                            Json::obj([
                                                ("member", Json::str(&r.member)),
                                                ("op", Json::str(r.op.as_str())),
                                            ])
                                        })
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                ),
            ),
            ("gen_param_policy", self.gen_param_policy.to_json()),
            (
                "reservation",
                match &self.reservation {
                    Reservation::FixtureBound { micros } => Json::obj([
                        ("kind", Json::str("fixture_bound")),
                        ("micros", Json::Int(*micros)),
                    ]),
                    Reservation::Unsupported => Json::obj([("kind", Json::str("unsupported"))]),
                },
            ),
            (
                "upstream_concurrency",
                Json::Int(self.upstream_concurrency as i64),
            ),
        ])
    }
}

impl FieldOp {
    /// The declared spelling (`to_json`).
    pub fn as_str(&self) -> &'static str {
        match self {
            FieldOp::Strip => "strip",
            FieldOp::StripRecursive => "strip_recursive",
            FieldOp::StripAutoElseRefuse => "strip_auto_else_refuse",
            FieldOp::RefuseIfGt(_) => "refuse_if_gt",
            FieldOp::Refuse => "refuse",
            FieldOp::ForwardRecorded => "forward_recorded",
        }
    }
}
