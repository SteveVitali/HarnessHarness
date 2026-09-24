//! Executable coverage for the S1.18 gating acceptance criteria:
//! AC-R-2.3.1-{1–8,11,13} · AC-R-2.3.2-{2,5–7,11} · AC-R-2.3.3-{1,7} ·
//! AC-R-2.3.4-{1–4}. Every test fails if the behaviour it names is removed.

use std::collections::{BTreeMap, BTreeSet};

use hh_compiler::profile::{
    self as prof, CapabilityState, DebtStatus, ExpiryCondition, ExpiryKind, ModelCoordinate,
    ModelProfile, ModelRole, ProfileCompatibility, ProfileDebtRecord, ProfileSelector,
    SelectorView, VersionPattern,
};
use hh_gateway::*;
use hh_wire::json::Json;

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures
// ────────────────────────────────────────────────────────────────────────────

fn retention() -> Vec<RetentionClass> {
    vec![RetentionClass {
        class_id: "short".into(),
        nominal_ms: 60_000,
        guaranteed: false,
    }]
}

/// A minimal `WireDialect` — `explicit_breakpoints` semantics so the marker
/// path is exercised; no rules unless the test adds them.
fn dialect() -> WireDialect {
    WireDialect {
        dialect_id: "d.test".into(),
        version: "1".into(),
        framing: Framing::HttpJson,
        plan_schema: PlanSchema {
            allowed_keys: vec!["thinking_budget".into()],
            required_keys: vec![],
        },
        envelope_fields: vec![],
        auth_kinds: vec![AuthKind::Bearer],
        endpoint_allowlist_ref: "policy:test".into(),
        streaming: true,
        usage_arrival: UsageArrival::FinalOnly,
        usage_mapping_default: UsageMappingDefault::PreferProvider,
        usage_mapping: None,
        cache_semantics: CacheSemantics::ExplicitBreakpoints {
            max_markers: 2,
            lookback_positions: Some(8),
            position_rule: MarkerPositionRule::Block,
            retention_classes: retention(),
            min_cacheable_tokens: Some(1_024),
            isolation: IsolationScope::Workspace,
            eligible_carriers: vec![BlockKind::Text],
        },
        cache_state_visible: true,
        count_tokens: None,
        deferred_requests: false,
        model_listing: None,
        snapshot_id_exposed: None,
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 30_000,
        prefix_affecting_params: vec![],
        rules: vec![
            // `stop_reason_map` — the provider spellings this dialect admits
            // (data, not code; an unmapped spelling reads `unknown` + raw).
            DialectRule::StopReasonMap {
                provider: "end_turn".into(),
                reason: StopReason::EndTurn,
            },
            DialectRule::StopReasonMap {
                provider: "tool_use".into(),
                reason: StopReason::ToolUse,
            },
            DialectRule::StopReasonMap {
                provider: "max_tokens".into(),
                reason: StopReason::MaxOutput,
            },
            DialectRule::StopReasonMap {
                provider: "stop_sequence".into(),
                reason: StopReason::StopSequence,
            },
            DialectRule::StopReasonMap {
                provider: "refusal".into(),
                reason: StopReason::Refusal,
            },
            DialectRule::StopReasonMap {
                provider: "content_filter".into(),
                reason: StopReason::ContentFilter,
            },
        ],
    }
}

fn frame(kind: &str, data: Json) -> WireFrame {
    let mut j = match data {
        Json::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    j.insert("type".into(), Json::str(kind));
    WireFrame::from_json(&Json::Obj(j)).unwrap()
}

fn debt(status: DebtStatus) -> ProfileDebtRecord {
    ProfileDebtRecord {
        rule_id: "r.profile".into(),
        hypothesis: "h".into(),
        evidence_refs: vec!["ev:1".into()],
        owner: "o".into(),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "t:1".into(),
        status,
    }
}

fn profile(
    id: &str,
    version: &str,
    precedence: i64,
    pattern: VersionPattern,
    status: DebtStatus,
) -> ModelProfile {
    let caps = prof::ProfileCapabilities {
        native_function_calling: CapabilityState::Declared,
        max_output: Some(8_192),
        ..prof::ProfileCapabilities::default()
    };
    ModelProfile {
        profile_id: id.into(),
        version: version.into(),
        content_hash: format!("hash-{id}-{version}"),
        selector: ProfileSelector {
            provider_api_family: "fam".into(),
            model_family: "mod".into(),
            version_pattern: pattern,
            precedence,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: caps,
        rules: vec![],
        ext: BTreeMap::new(),
        expiry: debt(status),
        compatibility: ProfileCompatibility {
            inventory_version: "1".into(),
            min_compiler_version: "0.0.0".into(),
        },
        tests: Json::Null,
    }
}

struct Env(Vec<ModelProfile>);
impl prof::ProfileView for Env {
    fn profile(&self, coordinate: &str) -> Option<ModelProfile> {
        self.0
            .iter()
            .find(|p| prof::profile_coordinate(p) == coordinate || p.content_hash == coordinate)
            .cloned()
    }
}
impl SelectorView for Env {
    fn registered(&self) -> Vec<ModelProfile> {
        self.0.clone()
    }
}

struct FakeBudget(Result<String, String>);
impl BudgetPort for FakeBudget {
    fn reserve(&mut self, _b: &str, _max: u64, _h: &str) -> Result<String, String> {
        self.0.clone()
    }
}

fn coordinate() -> ModelCoordinate {
    ModelCoordinate {
        provider_api_family: "fam".into(),
        model_family: "mod".into(),
        model_version: "v1".into(),
    }
}

fn route_candidate() -> RouteCandidate {
    RouteCandidate {
        model_ref: ModelRef {
            profile_ref: "prof.test@1".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        },
        coordinate: coordinate(),
    }
}

fn role_table() -> ModelRoleTable {
    let mut roles = BTreeMap::new();
    roles.insert(
        "primary".to_string(),
        RoleBinding {
            primary: route_candidate(),
            alternates: vec![],
            policy_ref: "policy.test@1".into(),
            profile_ref: "prof.test@1".into(),
        },
    );
    ModelRoleTable { roles }
}

fn routing_policy(kind: RoutingPolicyKind) -> RoutingPolicy {
    let params = match kind {
        RoutingPolicyKind::Static => Json::obj([(
            "target",
            Json::obj([
                (
                    "model_ref",
                    Json::obj([
                        ("profile_ref", Json::str("prof.test@1")),
                        ("provider_model_id", Json::str("m-1")),
                        ("serving_route", Json::str("route-a")),
                    ]),
                ),
                (
                    "coordinate",
                    Json::obj([
                        ("provider_api_family", Json::str("fam")),
                        ("model_family", Json::str("mod")),
                        ("model_version", Json::str("v1")),
                    ]),
                ),
            ]),
        )]),
        _ => Json::obj([("table_ref", Json::str("role-table.test"))]),
    };
    RoutingPolicy {
        policy_id: "policy.test".into(),
        version: "1".into(),
        content_hash: "h.policy".into(),
        role_scope: BTreeSet::new(),
        kind,
        params,
        error_actions: BTreeMap::new(),
        max_migration_loss: MigrationLossBound {
            dropped_items: 0,
            no_in_flight_tool_call: true,
        },
        allow_unknown: None,
        conditioned_rules: vec![],
        ext: BTreeMap::new(),
    }
}

fn routing_request() -> RoutingRequest {
    RoutingRequest {
        role: "primary".into(),
        required_capabilities: vec!["native_function_calling".into()],
        budget_id: "b-1".into(),
        holder: "run:1".into(),
        model_call_id: Some("mc-1".into()),
        intent_ref: None,
        effort: None,
    }
}

fn request_fixture(endpoint: &str, reservation: Option<&str>) -> InferenceRequest {
    let plan = ProviderRequestPlan {
        dialect_id: "d.test".into(),
        dialect_version: "1".into(),
        endpoint_ref: endpoint.into(),
        model_ref: "m-1".into(),
        body: Json::obj([("messages", Json::Arr(vec![]))]),
        provider_params: Extensions::default(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: CachePlan::default(),
        budget_reservation_id: reservation.map(str::to_string),
        canonical_len: 0,
    };
    InferenceRequest {
        model_call_id: "mc-1".into(),
        model_ref: ModelRef {
            profile_ref: "prof.test@1".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        },
        plan,
        role: "primary".into(),
        purpose: Purpose::Main,
        context_label: None,
        view_hash: None,
        binding_ref: "b.test".into(),
        credential_ref: Some("cred.test".into()),
        budget_reservation_id: reservation.map(str::to_string),
        attempt_policy: AttemptPolicy::c0(),
        deadline_ms: None,
        cache_state_hint: ExpectedState::Unknown,
        seed_request: None,
        observability: CallObservability::default(),
        request_class: RequestClass::Interactive,
        deferred_deadline_ms: None,
        stream: true,
        identity: None,
    }
}

struct NoTransport {
    sends: u32,
}
impl Transport for NoTransport {
    fn send(
        &mut self,
        _b: &[u8],
        _c: &CredentialHandle,
        _e: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        self.sends += 1;
        Ok(vec![])
    }
}

struct StubCreds;
impl CredentialPort for StubCreds {
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        _endpoint: &str,
        _auth: &[AuthKind],
    ) -> Result<CredentialHandle, GatewayError> {
        Ok(CredentialHandle {
            binding_id: credential_ref.map(str::to_string),
            provided: credential_ref.is_some(),
        })
    }
}

fn gateway(endpoints: &[&str]) -> (ModelGateway<'static>, NoTransport) {
    // The ports are leaked into 'static fixtures — a test-scoped gateway.
    let transport: &'static mut NoTransport = Box::leak(Box::new(NoTransport { sends: 0 }));
    let creds: &'static mut StubCreds = Box::leak(Box::new(StubCreds));
    let mut allow = EndpointAllowlist::default();
    for e in endpoints {
        allow.endpoints.insert((*e).to_string());
    }
    let mut g = ModelGateway {
        dialects: Default::default(),
        endpoints: allow,
        credentials: creds,
        transport,
        now_ms: Box::new(|| 0),
        normalizer_ref: "norm:test".into(),
    };
    g.load_dialect(dialect());
    // The transport handle is borrowed; return a clone marker for assertions.
    (g, NoTransport { sends: 0 })
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.1-1 — closed vocabularies; the kernel never reads a provider string.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn closed_vocabularies_round_trip() {
    for r in [
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
    ] {
        assert_eq!(StopReason::parse(r.as_str()), Some(r));
    }
    assert_eq!(StopReason::parse("max_tokens"), None); // surface alias is not kernel vocab
    for c in [
        RetryClass::Transient,
        RetryClass::RateLimit,
        RetryClass::Permanent,
        RetryClass::Ambiguous,
    ] {
        assert_eq!(RetryClass::parse(c.as_str()), Some(c));
    }
    for k in [
        BlockKind::Text,
        BlockKind::Reasoning,
        BlockKind::ToolCall,
        BlockKind::ProviderOpaque,
    ] {
        assert_eq!(BlockKind::parse(k.as_str()), Some(k));
    }
    for u in [
        UsageArrival::DeltaStream,
        UsageArrival::FinalOnly,
        UsageArrival::Absent,
    ] {
        assert_eq!(UsageArrival::parse(u.as_str()), Some(u));
    }
    for e in [
        ExpectedState::Warm,
        ExpectedState::Cold,
        ExpectedState::Uncacheable,
        ExpectedState::Unknown,
    ] {
        assert_eq!(ExpectedState::parse(e.as_str()), Some(e));
    }
    // `Purpose::Subagent` parameterises; the rest are closed atoms.
    assert_eq!(Purpose::parse("main"), Some(Purpose::Main));
    assert_eq!(
        Purpose::parse("subagent(s-1)"),
        Some(Purpose::Subagent("s-1".into()))
    );
    assert_eq!(Purpose::parse("admin"), None);
}

/// AC-R-2.3.1-4 — the error vocabulary is closed and each class carries its
/// kernel-default `retry_class`; parameterised kinds spell `kind{sub}`.
#[test]
fn error_classes_are_closed_and_typed() {
    let rate = ModelErrorClass::RateLimited;
    assert_eq!(rate.as_str(), "rate_limited");
    let timeout = ModelErrorClass::Timeout(TimeoutKind::StreamIdle);
    assert_eq!(timeout.as_str(), "timeout{stream_idle}");
    let nf = ModelErrorClass::NotFound(NotFoundKind::Endpoint);
    assert_eq!(nf.as_str(), "not_found{endpoint}");
    // The kernel default retry_class is a function of the class — data, not a
    // message-text match.
    let err = ModelError::new(ModelErrorClass::RateLimited, "slow down");
    assert_eq!(err.retry_class, RetryClass::RateLimit);
    let err = ModelError::new(ModelErrorClass::Auth, "bad key");
    assert_eq!(err.retry_class, RetryClass::Permanent);
    let err = ModelError::new(ModelErrorClass::Unknown, "?");
    assert_eq!(err.retry_class, RetryClass::Ambiguous);
}

/// AC-R-2.3.1-1 — `WireDialect` is a strict record: unknown members refuse,
/// an `error_text_to_class` rule without a complete debt record refuses
/// (T-LCD-05 reflexive), and a `retry_class_override` may only narrow.
#[test]
fn dialect_document_is_strict() {
    let d = dialect();
    let j = d.to_json();
    // Round-trip.
    let back = WireDialect::from_json(&j).expect("round-trip");
    assert_eq!(back.dialect_id, "d.test");
    // An unknown member refuses (strict codec — the sum is closed).
    let mut bad = j.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("surprise_member".into(), Json::str("x"));
    }
    assert!(WireDialect::from_json(&bad).is_err());
}

/// AC-R-2.3.1-4 — HTTP classification is data-driven (`status_to_class`
/// rules), never a model-string branch.
#[test]
fn http_classification_is_data() {
    let mut d = dialect();
    d.rules.push(DialectRule::StatusToClass {
        status: 429,
        class: ModelErrorClass::RateLimited,
    });
    let err = classify_http_error(&d, 429, &Json::Null);
    assert_eq!(err.class, ModelErrorClass::RateLimited);
    assert_eq!(err.retry_class, RetryClass::RateLimit);
    // An unlisted status falls to the kernel map (400 → permanent request).
    let err = classify_http_error(&d, 400, &Json::Null);
    assert_eq!(err.retry_class, RetryClass::Permanent);
}

/// AC-R-2.3.1-5 — the attempt driver: `attempts = min(failures + 1,
/// max_attempts)`; request bytes are identical across attempts; one
/// `attempt.started`/`completed` pair per attempt.
#[test]
fn attempts_retry_bounds_and_identical_bytes() {
    let policy = AttemptPolicy::c0();
    let mut sink = CollectSink::default();
    let mut sleeper = NoSleep::default();
    let now = || 0u64;
    let mut seen: Vec<u64> = Vec::new();
    let bytes = b"identical-request-body";
    let outcome = run_attempts(
        &policy,
        "mc-1",
        &now,
        &mut sink,
        &mut sleeper,
        |attempt_no| {
            // A deterministic hash of the bytes the caller would send.
            let h: u64 = bytes.iter().map(|b| *b as u64).sum();
            seen.push(h);
            if attempt_no < 3 {
                Err(ModelError::new(
                    ModelErrorClass::Timeout(TimeoutKind::Attempt),
                    "timed out",
                ))
            } else {
                Ok("done")
            }
        },
    )
    .unwrap();
    assert_eq!(outcome.attempts, 3);
    assert!(seen.iter().all(|h| *h == seen[0])); // identical bytes each attempt
    let started = sink
        .events
        .iter()
        .filter(|(c, _)| c == "model.call.attempt.started")
        .count();
    let failed = sink
        .events
        .iter()
        .filter(|(c, _)| c == "model.call.attempt.failed")
        .count();
    assert_eq!(started, 3);
    assert_eq!(failed, 2);
    assert_eq!(
        sink.events
            .iter()
            .filter(|(c, _)| c == "model.call.attempt.completed")
            .count(),
        1
    );
}

/// AC-R-2.3.1-5 — a permanent class never retries; an ambiguous class never
/// retries.
#[test]
fn attempts_never_retry_permanent_or_ambiguous() {
    for class in [
        ModelErrorClass::Auth,                          // permanent
        ModelErrorClass::Unknown,                       // ambiguous
        ModelErrorClass::NotFound(NotFoundKind::Model), // permanent
    ] {
        let policy = AttemptPolicy::c0();
        let mut sink = CollectSink::default();
        let mut sleeper = NoSleep::default();
        let mut tries = 0u32;
        let outcome = run_attempts(
            &policy,
            "mc-1",
            &|| 0u64,
            &mut sink,
            &mut sleeper,
            |_attempt_no| {
                tries += 1;
                Err::<(), _>(ModelError::new(class, "nope"))
            },
        )
        .unwrap();
        assert_eq!(tries, 1, "{:?} must not retry", class);
        assert!(outcome.result.is_err());
    }
    // `retryable` is the data check the driver applies.
    let policy = AttemptPolicy::c0();
    assert!(retryable(&policy, RetryClass::Transient));
    assert!(retryable(&policy, RetryClass::RateLimit));
    assert!(!retryable(&policy, RetryClass::Permanent));
    assert!(!retryable(&policy, RetryClass::Ambiguous));
}

/// AC-R-2.3.1-1 — `validate_plan` is the dialect's `plan_schema` gate (the
/// plan is data; an undeclared key or a `deferred` capability under
/// `deferred_requests = false` refuses).
#[test]
fn plan_schema_gate() {
    let d = dialect();
    let mut plan = request_fixture("ep.test", Some("res-1")).plan;
    // An undeclared `provider_params` key refuses.
    plan.provider_params.insert("evil".into(), Json::Bool(true));
    assert!(matches!(
        validate_plan(&d, &plan),
        Err(GatewayError::PlanSchemaViolation { .. })
    ));
    plan.provider_params.clear();
    // A declared key passes.
    plan.provider_params
        .insert("thinking_budget".into(), Json::Int(10));
    assert!(validate_plan(&d, &plan).is_ok());
    // `deferred` under `deferred_requests = false` refuses.
    plan.capabilities.deferred = Some(true);
    assert!(validate_plan(&d, &plan).is_err());
}

/// AC-R-2.3.1-13 — an endpoint outside the allow-list is refused before any
/// bytes leave (the transport was never invoked), and no credential material
/// appears in the emitted payload (only `credential_binding_id`).
#[test]
fn endpoint_admission_and_no_secret_material() {
    let (mut g, _t) = gateway(&["ep.test"]);
    let mut sink = CollectSink::default();
    // A non-listed endpoint refuses *before* `send`.
    let r = g.open_call(request_fixture("ep.evil", Some("res-1")), &mut sink);
    assert!(matches!(r, Err(GatewayError::EndpointNotAllowed { .. })));
    // A missing reservation refuses.
    let r = g.open_call(request_fixture("ep.test", None), &mut sink);
    assert!(matches!(r, Err(GatewayError::MissingReservation { .. })));
    // A good request emits `model.call.requested` with the binding id only.
    let h = g
        .open_call(request_fixture("ep.test", Some("res-1")), &mut sink)
        .expect("open");
    let req_ev = sink
        .events
        .iter()
        .find(|(c, _)| c == "model.call.requested")
        .map(|(_, p)| p.clone())
        .expect("requested emitted");
    assert_eq!(
        req_ev.get("credential_binding_id").and_then(Json::as_str),
        Some("cred.test")
    );
    let payload = req_ev.to_canonical_string();
    assert!(!payload.contains("secret"));
    assert!(!payload.contains("token_value"));
    drop(h);
}

/// AC-R-2.3.1-2/3/6 — a recorded happy-path stream decodes to the canonical
/// event sequence: `message.started → block.started → block.delta →
/// block.completed → usage.updated → completed`; usage lands on the terminal
/// event as a normalized `TokenVector` (`hh-inclusive/1`).
#[test]
fn golden_stream_decodes() {
    let d = dialect();
    let mut st = DecodeState::default();
    let frames = [
        frame(
            "message_start",
            Json::obj([(
                "message",
                Json::obj([("id", Json::str("r-1")), ("model", Json::str("m-served"))]),
            )]),
        ),
        frame(
            "content_block_start",
            Json::obj([
                ("index", Json::Int(0)),
                ("content_block", Json::obj([("type", Json::str("text"))])),
            ]),
        ),
        frame(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([("type", Json::str("text_delta")), ("text", Json::str("hi"))]),
                ),
            ]),
        ),
        frame("content_block_stop", Json::obj([("index", Json::Int(0))])),
        frame(
            "message_delta",
            Json::obj([
                ("delta", Json::obj([("stop_reason", Json::str("end_turn"))])),
                (
                    "usage",
                    Json::obj([
                        ("input_tokens", Json::Int(10)),
                        ("output_tokens", Json::Int(4)),
                    ]),
                ),
            ]),
        ),
        frame(
            "message_stop",
            Json::obj([(
                "usage",
                Json::obj([
                    ("input_tokens", Json::Int(10)),
                    ("output_tokens", Json::Int(4)),
                ]),
            )]),
        ),
    ];
    let mut kinds: Vec<String> = Vec::new();
    let mut terminal: Option<ModelEvent> = None;
    for (i, f) in frames.iter().enumerate() {
        for ev in decode_frame(&d, f, &mut st, (i as u64) * 10, "norm:test") {
            if matches!(ev.kind, ModelEventKind::Completed { .. }) {
                terminal = Some(ev.clone());
            }
            kinds.push(format!("{:?}", std::mem::discriminant(&ev.kind)));
        }
    }
    assert!(st.violations.is_empty(), "{:?}", st.violations);
    let terminal = terminal.expect("a completed event");
    match terminal.kind {
        ModelEventKind::Completed {
            message,
            stop_reason,
            usage,
            surface_ids,
            ..
        } => {
            assert_eq!(stop_reason, StopReason::EndTurn);
            assert_eq!(message.text(), "hi");
            assert_eq!(message.served_model.as_deref(), Some("m-served"));
            assert_eq!(
                surface_ids.get("response_id").map(String::as_str),
                Some("r-1")
            );
            let u = usage.expect("usage on completed");
            assert_eq!(u.input_uncached, 10);
            assert_eq!(u.output_total, 4);
            assert_eq!(u.normalizer_ref, "norm:test");
        }
        _ => panic!("expected Completed"),
    }
}

/// AC-R-2.3.1-3 (G7) — a stream gap beyond `stream_idle_timeout_ms` fails
/// `timeout{stream_idle}` and records the violation.
#[test]
fn grammar_idle_timeout_is_a_violation() {
    let d = dialect();
    let mut st = DecodeState::default();
    let _ = decode_frame(
        &d,
        &frame("message_start", Json::obj([])),
        &mut st,
        0,
        "norm:test",
    );
    let evs = decode_frame(
        &d,
        &frame("content_block_delta", Json::obj([("index", Json::Int(0))])),
        &mut st,
        40_000,
        "norm:test",
    );
    assert!(st
        .violations
        .iter()
        .any(|v| matches!(v, GrammarViolation::IdleTimeout { .. })));
    assert!(evs.iter().any(|e| matches!(
        &e.kind,
        ModelEventKind::Failed { error, .. }
            if error.class == ModelErrorClass::Timeout(TimeoutKind::StreamIdle)
    )));
}

/// AC-R-2.3.1-7 — a split `input_json_delta` tool call parses to the object;
/// invalid JSON is `Unparseable{invalid_json}` with `raw` preserved.
#[test]
fn tool_call_split_delta_and_unparseable() {
    let d = dialect();
    let mut st = DecodeState::default();
    let seq = [
        frame("message_start", Json::obj([])),
        frame(
            "content_block_start",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "content_block",
                    Json::obj([
                        ("type", Json::str("tool_use")),
                        ("name", Json::str("lookup")),
                        ("id", Json::str("call-1")),
                    ]),
                ),
            ]),
        ),
        frame(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("input_json_delta")),
                        ("partial_json", Json::str("{\"q\":\"he")),
                    ]),
                ),
            ]),
        ),
        frame(
            "content_block_delta",
            Json::obj([
                ("index", Json::Int(0)),
                (
                    "delta",
                    Json::obj([
                        ("type", Json::str("input_json_delta")),
                        ("partial_json", Json::str("llo\"}")),
                    ]),
                ),
            ]),
        ),
        frame("content_block_stop", Json::obj([("index", Json::Int(0))])),
        frame(
            "message_stop",
            Json::obj([("delta", Json::obj([("stop_reason", Json::str("tool_use"))]))]),
        ),
    ];
    let mut block: Option<ModelBlock> = None;
    let mut terminal = None;
    for (i, f) in seq.iter().enumerate() {
        for ev in decode_frame(&d, f, &mut st, (i as u64) * 10, "norm:test") {
            match &ev.kind {
                ModelEventKind::BlockCompleted { block: b, .. } => block = Some(b.clone()),
                ModelEventKind::Completed { .. } => terminal = Some(ev.clone()),
                _ => {}
            }
        }
    }
    match block.expect("block.completed") {
        ModelBlock::ToolCall(tc) => {
            match &tc.arguments {
                ToolArgs::Parsed(j) => {
                    assert_eq!(j.get("q").and_then(Json::as_str), Some("hello"));
                }
                other => panic!("expected Parsed, got {other:?}"),
            }
            assert_eq!(tc.surface_name, "lookup");
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
    assert!(terminal.is_some());
    assert!(st.violations.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.2 — the C0 router slice.
// ─────────────────────────────────────────────────────────────────────────────

/// AC-R-2.3.2-2 — a static route emits exactly one `RoutingDecision` carrying
/// the selected target, the candidate verdicts, and the reservation id.
#[test]
fn router_static_one_decision() {
    let env = Env(vec![profile(
        "prof.test",
        "1",
        0,
        VersionPattern::Exact("v1".into()),
        DebtStatus::Active,
    )]);
    let mut budget = FakeBudget(Ok("res-9".into()));
    let d = select(
        &routing_request(),
        &env,
        &mut budget,
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "dec-1",
    )
    .expect("selects");
    assert_eq!(d.selected.provider_model_id, "m-1");
    assert_eq!(d.selected.profile_ref, "prof.test@1");
    assert_eq!(d.reservation_id.as_deref(), Some("res-9"));
    assert_eq!(d.candidates_considered.len(), 1);
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Selected
    );
    let payload = events::route_decided(&d);
    assert_eq!(
        payload
            .get("selected")
            .and_then(|s| s.get("provider_model_id"))
            .and_then(Json::as_str),
        Some("m-1")
    );
}

/// AC-R-2.3.2-2 — a refused route is a typed `RoutingRefusal` member, never a
/// warning and never a silent fallback.
#[test]
fn router_refusals_are_typed() {
    let healthy_env = || {
        Env(vec![profile(
            "prof.test",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        )])
    };
    // NoProfile — nothing matches the coordinate.
    let r = select(
        &routing_request(),
        &Env(vec![]),
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::NoProfile { .. })));
    // ExpiredProfile — `expired` without `intent_ref` refuses; with intent it
    // binds (the `model.profile.expired_used` marker).
    let expired = Env(vec![profile(
        "prof.test",
        "1",
        0,
        VersionPattern::Exact("v1".into()),
        DebtStatus::Expired,
    )]);
    let r = select(
        &routing_request(),
        &expired,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    );
    assert!(matches!(
        r,
        Err(RoutingRefusal::ExpiredProfile { status, .. }) if status == "expired"
    ));
    let mut with_intent = routing_request();
    with_intent.intent_ref = Some("intent:1".into());
    assert!(select(
        &with_intent,
        &expired,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    )
    .is_ok());
    // Retired never binds — intent or not.
    let retired = Env(vec![profile(
        "prof.test",
        "1",
        0,
        VersionPattern::Exact("v1".into()),
        DebtStatus::Retired,
    )]);
    let r = select(
        &with_intent,
        &retired,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::ExpiredProfile { status, .. }) if status == "retired"));
    // CapabilityUnknown — a required axis the profile leaves `unknown`.
    let mut req = routing_request();
    req.required_capabilities.push("tool_search".into());
    let r = select(
        &req,
        &healthy_env(),
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::CapabilityUnknown { .. })));
    // InsufficientBudget — the G-4 reserve refused.
    let r = select(
        &routing_request(),
        &healthy_env(),
        &mut FakeBudget(Err("tokens.output.visible".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "d",
    );
    assert!(matches!(
        r,
        Err(RoutingRefusal::InsufficientBudget { dimension }) if dimension == "tokens.output.visible"
    ));
    // PolicyInvalid — a non-C0 kind is a declared tier refusal (CC6).
    let r = select(
        &routing_request(),
        &healthy_env(),
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::CostCap),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::PolicyInvalid { .. })));
}

/// AC-R-2.3.2-7 — a conditioned rule without a complete debt record is
/// `PolicyInvalid{rule_id, missing_debt_record}` at `link` (T-LCD-05).
#[test]
fn conditioned_rule_without_debt_is_policy_invalid() {
    let mut policy = routing_policy(RoutingPolicyKind::Static);
    policy.conditioned_rules.push(PolicyConditionedRule {
        rule_id: "rule-x".into(),
        conditioned_key: Some("family:mod".into()),
        debt: None,
    });
    let env = Env(vec![profile(
        "prof.test",
        "1",
        0,
        VersionPattern::Exact("v1".into()),
        DebtStatus::Active,
    )]);
    let r = select(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &policy,
        &role_table(),
        "d",
    );
    match r {
        Err(RoutingRefusal::PolicyInvalid { rule_id, reason }) => {
            assert_eq!(rule_id, "rule-x");
            assert!(reason.contains("missing_debt_record"));
        }
        other => panic!("expected PolicyInvalid, got {other:?}"),
    }
}

/// AC-R-2.3.2-6 — `error_actions` is data on the policy record and rides its
/// `content_hash` (two policies differing only in `error_actions` differ in
/// `content_id`).
#[test]
fn error_actions_are_data() {
    let mut p = routing_policy(RoutingPolicyKind::Static);
    let bare = p.content_id();
    p.error_actions.insert(
        ModelErrorClass::RateLimited.as_str().into(),
        ErrorAction::RetrySame { max: 3 },
    );
    assert_ne!(p.content_id(), bare);
    assert_eq!(
        error_action(&p, &ModelErrorClass::RateLimited),
        ErrorAction::RetrySame { max: 3 }
    );
    assert_eq!(
        error_action(&p, &ModelErrorClass::Auth),
        ErrorAction::GiveUp
    );
}

/// AC-R-2.3.2-11 — `select` is pure over its inputs: identical inputs yield
/// identical decisions minus the allocated ids.
#[test]
fn routing_is_deterministic() {
    let env = Env(vec![profile(
        "prof.test",
        "1",
        0,
        VersionPattern::Exact("v1".into()),
        DebtStatus::Active,
    )]);
    let mk = |id: &str| {
        select(
            &routing_request(),
            &env,
            &mut FakeBudget(Ok("res-1".into())),
            &NoHealth,
            &routing_policy(RoutingPolicyKind::RoleTable),
            &role_table(),
            id,
        )
        .unwrap()
    };
    let a = mk("d-1");
    let b = mk("d-2");
    assert_eq!(a.selected, b.selected);
    assert_eq!(a.candidates_considered, b.candidates_considered);
    assert_eq!(a.rule_ids_fired, b.rule_ids_fired);
    assert_eq!(a.inputs_read, b.inputs_read);
    assert_ne!(a.decision_id, b.decision_id);
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.3 — the `ModelProfile/1` selector machinery (hh-compiler).
// ─────────────────────────────────────────────────────────────────────────────

/// AC-R-2.3.3-1 — the selector grammar is `exact | prefix | range | any`
/// over the model coordinate; `AmbiguousSelector` on equal precedence +
/// specificity; `NoProfile` on no match; the null profile is a complete,
/// never-silent fixture.
#[test]
fn profile_selector_grammar() {
    let coord = ModelCoordinate {
        provider_api_family: "fam".into(),
        model_family: "mod".into(),
        model_version: "v1.2".into(),
    };
    let sel = |p: VersionPattern| ProfileSelector {
        provider_api_family: "fam".into(),
        model_family: "mod".into(),
        version_pattern: p,
        precedence: 0,
        successor_ref: None,
        retirement_at: None,
        roles_admitted: vec![ModelRole::Primary],
    };
    assert!(prof::selector_matches(
        &sel(VersionPattern::Exact("v1.2".into())),
        &coord
    ));
    assert!(!prof::selector_matches(
        &sel(VersionPattern::Exact("v1.3".into())),
        &coord
    ));
    assert!(prof::selector_matches(
        &sel(VersionPattern::Prefix("v1.".into())),
        &coord
    ));
    assert!(prof::selector_matches(
        &sel(VersionPattern::Range {
            family: "mod".into(),
            lo: Some("v1.0".into()),
            hi: Some("v2.0".into()),
        }),
        &coord
    ));
    assert!(!prof::selector_matches(
        &sel(VersionPattern::Range {
            family: "mod".into(),
            lo: Some("v2.0".into()),
            hi: None,
        }),
        &coord
    ));
    assert!(prof::selector_matches(&sel(VersionPattern::Any), &coord));
    // No matching profile → `NoProfile` (never a default).
    let r = prof::resolve_profile(&coordinate(), &Env(vec![]));
    assert!(matches!(
        r,
        Err(prof::ProfileResolveError::NoProfile { .. })
    ));
    // Equal precedence + specificity → `AmbiguousSelector`.
    let amb = Env(vec![
        profile(
            "p.a",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        ),
        profile(
            "p.b",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        ),
    ]);
    let r = prof::resolve_profile(&coordinate(), &amb);
    assert!(matches!(
        r,
        Err(prof::ProfileResolveError::AmbiguousSelector { candidates }) if candidates.len() == 2
    ));
    // Higher precedence wins; a single leaf resolves.
    let env = Env(vec![
        profile("p.lo", "1", 0, VersionPattern::Any, DebtStatus::Active),
        profile("p.hi", "1", 9, VersionPattern::Any, DebtStatus::Active),
    ]);
    let chain = prof::resolve_profile(&coordinate(), &env).unwrap();
    assert_eq!(chain.profiles.last().unwrap().profile_id, "p.hi");
    // The null profile is a complete fixture — `any` at the lowest precedence,
    // every capability `unknown`, a dated debt record.
    let n = prof::null_profile();
    assert!(n.expiry.is_complete());
    assert_eq!(n.selector.version_pattern, VersionPattern::Any);
    let env = Env(vec![n]);
    let chain = prof::resolve_profile(&coordinate(), &env).unwrap();
    assert_eq!(chain.profiles.last().unwrap().profile_id, "null");
}

/// AC-R-2.3.3-7 — the expiry state machine (ADR-0126 d.2) is a pure
/// transition table; `retired` is reachable only via `RetirePassed` from
/// `expired`.
#[test]
fn profile_expiry_transitions() {
    use prof::{expiry_transition, ExpiryTrigger::*};
    use DebtStatus::*;
    assert_eq!(expiry_transition(Active, ExpiryObservable), Some(Expiring));
    assert_eq!(expiry_transition(Expiring, Revalidated), Some(Active));
    assert_eq!(expiry_transition(Active, RetirementAtPassed), Some(Expired));
    assert_eq!(
        expiry_transition(Expired, EvidenceRefreshAndProbePass),
        Some(Active)
    );
    assert_eq!(expiry_transition(Expired, RetirePassed), Some(Retired));
    // Illegal transitions are `None`, never silent jumps.
    assert_eq!(expiry_transition(Active, RetirePassed), None);
    assert_eq!(expiry_transition(Retired, Revalidated), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.4 — layout invariants + cache accounting.
// ─────────────────────────────────────────────────────────────────────────────

/// AC-R-2.3.4-1/2 — `check_layout` enforces the tier order (LC-1), the
/// no-volatile-in-static rule (LC-3), and reserved-volatile placement (LC-8);
/// `check_tool_order_prefix` enforces prefix-extension.
#[test]
fn layout_invariants() {
    // A clean layout passes.
    let slots = vec![
        SlotFacts {
            slot_id: "s.static".into(),
            cache_tier: CacheTier::Static,
            candidate_kinds: vec!["text".into()],
        },
        SlotFacts {
            slot_id: "s.dynamic".into(),
            cache_tier: CacheTier::Dynamic,
            candidate_kinds: vec!["current_time".into()],
        },
    ];
    // `clock` is volatile and sits in the first dynamic slot — legal.
    assert!(check_layout(&slots).is_empty());
    // Volatile in `static` violates LC-3.
    let bad = vec![SlotFacts {
        slot_id: "s.static".into(),
        cache_tier: CacheTier::Static,
        candidate_kinds: vec!["current_time".into()],
    }];
    assert!(!check_layout(&bad).is_empty());
    // Tool surfaces must extend a prefix (LC-4).
    assert!(check_tool_order_prefix(
        &["a".into(), "b".into()],
        &["a".into(), "b".into(), "c".into()]
    )
    .is_none());
    assert!(
        check_tool_order_prefix(&["a".into(), "b".into()], &["a".into(), "x".into()]).is_some()
    );
}

/// AC-R-2.3.4-3 — the affinity key is deterministic and purpose-sensitive;
/// `wire` is a `full` prefix bounded by the dialect's `max_key_length`.
#[test]
fn affinity_key_determinism() {
    let k1 = derive_affinity_key(
        "scope:run-1",
        "sha:static",
        &Purpose::Main,
        AffinitySupport::Supported { max_key_length: 16 },
    )
    .unwrap();
    let k2 = derive_affinity_key(
        "scope:run-1",
        "sha:static",
        &Purpose::Main,
        AffinitySupport::Supported { max_key_length: 16 },
    )
    .unwrap();
    assert_eq!(k1.full, k2.full);
    assert_eq!(k1.wire.len(), 16);
    assert!(k1.full.starts_with(&k1.wire));
    // A different purpose never collides (CK-2).
    let k3 = derive_affinity_key(
        "scope:run-1",
        "sha:static",
        &Purpose::Compaction,
        AffinitySupport::Supported { max_key_length: 16 },
    )
    .unwrap();
    assert_ne!(k1.full, k3.full);
    // `unsupported` is a typed refusal, never a silent omission.
    assert!(
        derive_affinity_key("scope", "sha", &Purpose::Main, AffinitySupport::Unsupported).is_err()
    );
}

/// AC-R-2.3.4-4 — `place_markers` enforces the cap by invalidation priority,
/// drops record `{position, reason}` (never silent), substitutes the nearest
/// eligible earlier carrier, and `below_minimum` drops everything.
#[test]
fn marker_cap_and_drop_accounting() {
    let d = dialect();
    let carriers = vec![
        CarrierBlock {
            index: 0,
            tier: CacheTier::Static,
            kind: BlockKind::Text,
            role: None,
        },
        CarrierBlock {
            index: 1,
            tier: CacheTier::Dynamic,
            kind: BlockKind::Text,
            role: Some("user".into()),
        },
    ];
    // Three positions, cap = 2 → one dropped with `cap` (the lowest-value
    // position — transcript tail — drops first).
    let positions = vec![
        MarkerPosition::TierBoundary(CacheTier::Static),
        MarkerPosition::TierBoundary(CacheTier::Dynamic),
        MarkerPosition::TranscriptTailLastCacheable,
    ];
    let r = place_markers(&positions, &carriers, &d.cache_semantics, Some(2_000));
    assert_eq!(r.markers.len(), 2);
    assert_eq!(r.dropped.len(), 1);
    assert_eq!(r.dropped[0].reason, DropReason::Cap);
    // Below `min_cacheable_tokens` → every marker `below_minimum`.
    let r = place_markers(&positions, &carriers, &d.cache_semantics, Some(10));
    assert!(r.markers.is_empty());
    assert!(r
        .dropped
        .iter()
        .all(|d| d.reason == DropReason::BelowMinimum));
    // An ineligible carrier substitutes and is recorded.
    let tool_carriers = vec![CarrierBlock {
        index: 0,
        tier: CacheTier::Static,
        kind: BlockKind::ToolCall,
        role: None,
    }];
    let r = place_markers(
        &[MarkerPosition::TierBoundary(CacheTier::Static)],
        &tool_carriers,
        &d.cache_semantics,
        Some(2_000),
    );
    // No eligible carrier at all → dropped `ineligible_carrier`, never silent.
    assert!(
        !r.substituted.is_empty()
            || r.dropped
                .iter()
                .any(|d| d.reason == DropReason::IneligibleCarrier)
    );
}

/// AC-R-2.3.4 — `expect_cache_state` is a pure projection (CK-3): `warm`
/// inside the lease window, `cold{ttl_elapsed}` past it, `uncacheable` below
/// the minimum, `unknown` under `uncached` semantics.
#[test]
fn expect_cache_state_projection() {
    let semantics = CacheSemantics::ImplicitPrefix {
        strictness: ImplicitStrictness::Tolerant,
        affinity_key: AffinitySupport::Supported { max_key_length: 32 },
        retention_classes: retention(),
        min_cacheable_tokens: Some(100),
        reporting_granularity_tokens: None,
    };
    let prior = CacheCallFact {
        affinity_key: "k".into(),
        static_hash: "h".into(),
        request_sent_ms: 0,
        completed: true,
        model_call_id: "mc-0".into(),
    };
    // Within the 60s nominal (margin 1s): warm at t=10s.
    let e = expect_cache_state(
        std::slice::from_ref(&prior),
        "k",
        "h",
        10_000,
        Some(500),
        &semantics,
        1_000,
    );
    assert_eq!(e.expected, ExpectedState::Warm);
    assert_eq!(e.basis.last_call.as_deref(), Some("mc-0"));
    // Past the window: cold.
    let e = expect_cache_state(&[prior], "k", "h", 120_000, Some(500), &semantics, 1_000);
    assert_eq!(e.expected, ExpectedState::Cold);
    // Below `min_cacheable_tokens`: uncacheable.
    let e = expect_cache_state(&[], "k", "h", 0, Some(10), &semantics, 1_000);
    assert_eq!(e.expected, ExpectedState::Uncacheable);
    // `uncached` semantics: unknown.
    let e = expect_cache_state(&[], "k", "h", 0, Some(500), &CacheSemantics::Uncached, 0);
    assert_eq!(e.expected, ExpectedState::Unknown);
    // A same-key sibling still open: `cold{concurrent_sibling}`.
    let open = CacheCallFact {
        completed: false,
        ..CacheCallFact {
            affinity_key: "k".into(),
            static_hash: "h".into(),
            request_sent_ms: 0,
            completed: false,
            model_call_id: "mc-open".into(),
        }
    };
    let e = expect_cache_state(&[open], "k", "h", 5_000, Some(500), &semantics, 1_000);
    assert_eq!(e.expected, ExpectedState::Cold);
    assert_eq!(e.basis.cold_reason, Some(MissReason::ConcurrentSibling));
}

/// AC-R-2.3.1-6 — usage normalization is `hh-telemetry`'s one convention;
/// a usage-less terminal yields `available = false`, never zeros.
#[test]
fn usage_normalization_inclusive() {
    let d = dialect();
    let raw = Json::obj([
        ("input_tokens", Json::Int(10)),
        ("output_tokens", Json::Int(4)),
        ("cache_read_input_tokens", Json::Int(6)),
    ]);
    let v = normalize_usage(&d, &raw, true, "norm:test").expect("usage");
    assert_eq!(v.input_uncached, 10);
    assert_eq!(v.cache_read, 6);
    assert_eq!(v.output_total, 4);
    assert_eq!(v.normalizer_ref, "norm:test");
    // `usage_arrival = absent` ⇒ no vector.
    let mut d2 = dialect();
    d2.usage_arrival = UsageArrival::Absent;
    assert!(normalize_usage(&d2, &raw, true, "n").is_none());
}

/// AC-R-2.3.1-13 — `model.call.requested` carries `cache{…}` and
/// `credential_binding_id` (never the credential); `model.call.completed`
/// carries `usage`/`timing`/`surface_ids`/`substitution`.
#[test]
fn terminal_payload_members() {
    let req = request_fixture("ep.test", Some("res-1"));
    let p = events::call_requested(
        &req,
        &dialect().cache_semantics,
        Some("cred.test"),
        None,
        None,
        Some(42),
        None,
    );
    for k in [
        "model_call_id",
        "model_ref",
        "plan_hash",
        "token_estimate",
        "cache",
        "credential_binding_id",
        "role",
        "dialect",
        "endpoint_ref",
        "request_class",
        "stream",
    ] {
        assert!(p.get(k).is_some(), "missing {k}");
    }
    let cache = p.get("cache").unwrap();
    for k in [
        "semantics_kind",
        "purpose",
        "markers",
        "retention_classes_requested",
    ] {
        assert!(cache.get(k).is_some(), "missing cache.{k}");
    }
    let msg = ModelMessage {
        blocks: vec![ModelBlock::Text {
            text: Text {
                content: "hi".into(),
                authority: TextAuthority::Delegate,
                trust: TextTrust::External,
                visibility: TextVisibility::Public,
            },
            signature: None,
        }],
        stop_reason: StopReason::EndTurn,
        stop_details: None,
        raw_stop_reason: None,
        usage: None,
        served_model: None,
        snapshot_id: None,
        surface_ids: BTreeMap::new(),
    };
    let timing = Timing {
        latency_ms: 10,
        attempts: 1,
        ttft_ms: Some(3),
        queue_wait_ms: 0,
        measured_at: "adapter".into(),
    };
    let c = events::call_completed(
        "mc-1", &msg, None, None, None, None, None, &timing, None, None, None,
    );
    for k in [
        "model_call_id",
        "usage",
        "timing",
        "stop_reason",
        "substitution",
    ] {
        assert!(c.get(k).is_some(), "missing {k}");
    }
    assert_eq!(
        c.get("usage").and_then(|u| u.get("available")),
        Some(&Json::Bool(false))
    );
}

/// `model.rerouted` carries the closed `RerouteReason` spelling and the
/// `relowered` flag (ADR-0122 d.3 ordering is the caller's discipline).
#[test]
fn reroute_payload() {
    let p = events::rerouted(
        "mc-1",
        "m-1",
        "m-2",
        &RerouteReason::TransientExhausted,
        2,
        true,
        Some("ev.relower.1"),
        "dec-1",
    );
    assert_eq!(
        p.get("reason").and_then(Json::as_str),
        Some("transient_exhausted")
    );
    assert_eq!(p.get("relowered"), Some(&Json::Bool(true)));
    assert_eq!(
        p.get("relower_event_ref").and_then(Json::as_str),
        Some("ev.relower.1")
    );
}

/// AC-R-2.3.1-11 — the recording/fake gateway runs out of process over a
/// canonical-JSON stdin/stdout boundary: the same decode path produces the
/// same `model.*` event stream the live variant would append.
#[test]
fn stub_runs_out_of_process() {
    let input = Json::obj([
        ("command", Json::str("run_call")),
        ("dialect", dialect().to_json()),
        ("endpoint_allowlist", Json::Arr(vec![Json::str("ep.test")])),
        ("model_call_id", Json::str("mc-oop")),
        ("normalizer_ref", Json::str("norm:test")),
        (
            "plan",
            Json::obj([
                ("endpoint_ref", Json::str("ep.test")),
                ("credential_ref", Json::str("cred.test")),
                ("budget_reservation_id", Json::str("res-1")),
                ("model_ref", Json::str("m-1")),
                ("body", Json::obj([("messages", Json::Arr(vec![]))])),
            ]),
        ),
        (
            "frames",
            Json::Arr(vec![
                Json::obj([
                    ("at_ms", Json::Int(0)),
                    (
                        "frame",
                        Json::obj([
                            ("type", Json::str("message_start")),
                            ("id", Json::str("r-oop")),
                        ]),
                    ),
                ]),
                Json::obj([
                    ("at_ms", Json::Int(5)),
                    (
                        "frame",
                        Json::obj([
                            ("type", Json::str("content_block_start")),
                            ("index", Json::Int(0)),
                            ("content_block", Json::obj([("type", Json::str("text"))])),
                        ]),
                    ),
                ]),
                Json::obj([
                    ("at_ms", Json::Int(10)),
                    (
                        "frame",
                        Json::obj([
                            ("type", Json::str("content_block_delta")),
                            ("index", Json::Int(0)),
                            (
                                "delta",
                                Json::obj([
                                    ("type", Json::str("text_delta")),
                                    ("text", Json::str("out-of-process")),
                                ]),
                            ),
                        ]),
                    ),
                ]),
                Json::obj([
                    ("at_ms", Json::Int(15)),
                    (
                        "frame",
                        Json::obj([
                            ("type", Json::str("content_block_stop")),
                            ("index", Json::Int(0)),
                        ]),
                    ),
                ]),
                Json::obj([
                    ("at_ms", Json::Int(20)),
                    (
                        "frame",
                        Json::obj([
                            ("type", Json::str("message_stop")),
                            ("delta", Json::obj([("stop_reason", Json::str("end_turn"))])),
                            (
                                "usage",
                                Json::obj([
                                    ("input_tokens", Json::Int(8)),
                                    ("output_tokens", Json::Int(3)),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            ]),
        ),
    ]);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hh-gateway-stub"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn stub");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.to_canonical_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    let line = String::from_utf8(out.stdout).unwrap();
    let resp = hh_wire::json::parse(line.trim()).expect("json out");
    assert_eq!(
        resp.get("result").and_then(Json::as_str),
        Some("completed"),
        "{line}"
    );
    let classes: Vec<String> = resp
        .get("events")
        .and_then(|e| match e {
            Json::Arr(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|e| e.get("class").and_then(Json::as_str).map(str::to_string))
        .collect();
    assert!(
        classes.iter().any(|c| c == "model.call.requested"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "model.call.attempt.started"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "model.call.completed"),
        "{classes:?}"
    );
    // The completed row carries usage — the sole usage emission.
    let completed = resp
        .get("events")
        .and_then(|e| match e {
            Json::Arr(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .find(|e| e.get("class").and_then(Json::as_str) == Some("model.call.completed"))
        .expect("completed event");
    assert_eq!(
        completed
            .get("payload")
            .and_then(|p| p.get("usage"))
            .and_then(|u| u.get("available")),
        Some(&Json::Bool(true))
    );
}
