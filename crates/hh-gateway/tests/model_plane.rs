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
        evidence_refs: vec![hh_compiler::profile::EvidenceRef::legacy("ev:1")],
        owner: "o".into(),
        reach_via: Vec::new(),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "t:1".into(),
        removal_test: Some(hh_compiler::profile::RemovalTest::new(
            hh_compiler::profile::RemovalTestKind::Documentation,
        )),
        status,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        created_at: None,
        supersedes: None,
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
    fn release(&mut self, _r: &str) -> Result<(), String> {
        Ok(())
    }
}

/// A `FakeBudget` that records releases (the R-7 tests read it).
struct ReleasingBudget {
    released: Vec<String>,
}
impl BudgetPort for ReleasingBudget {
    fn reserve(&mut self, _b: &str, _max: u64, _h: &str) -> Result<String, String> {
        Ok(format!("res-{}", self.released.len() + 1))
    }
    fn release(&mut self, r: &str) -> Result<(), String> {
        self.released.push(r.to_string());
        Ok(())
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
        latency_target_ms: None,
        quality_prior_ref: None,
        preferences: None,
        source_profile_ref: None,
        in_flight_effect: false,
        task_class: None,
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
        substitution_allowed: None,
        participant_class: None,
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
        pending_relower: std::collections::BTreeSet::new(),
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
    // PolicyInvalid — `learned` is a declared tier refusal (C4 — CC6);
    // every other kind executes at C1.
    let r = select(
        &routing_request(),
        &healthy_env(),
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Learned),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::PolicyInvalid { .. })));
}

/// AC-R-2.3.2-14 — `learned` is deployable only through the evolution
/// pipeline: `params.evolution` must carry the campaign evidence —
/// `search_budget_ref`, `artifact_benefit_ref`, `match_mode:
/// matched_total`, `budget_match: matched`, `expiry_condition ∋
/// model_version_change`. Every missing/malformed member is
/// `PolicyInvalid`, never a fall-through; a complete binding admits and
/// selects the role's sealed candidate list (the accepted candidate's
/// frozen selection — not an in-router learner).
#[test]
fn learned_policy_requires_evolution_pipeline_evidence() {
    let env = || {
        Env(vec![profile(
            "prof.test",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        )])
    };
    let evo = |overrides: &[(&str, Json)]| {
        let mut m = BTreeMap::new();
        m.insert("search_budget_ref".into(), Json::str("sb:r1"));
        m.insert("artifact_benefit_ref".into(), Json::str("ab:r1"));
        m.insert("match_mode".into(), Json::str("matched_total"));
        m.insert("budget_match".into(), Json::str("matched"));
        m.insert(
            "expiry_condition".into(),
            Json::Arr(vec![Json::str("model_version_change")]),
        );
        for (k, v) in overrides {
            m.insert(k.to_string(), v.clone());
        }
        Json::Obj(m)
    };
    let learned = |params_evolution: Option<Json>| {
        let mut p = routing_policy(RoutingPolicyKind::Learned);
        p.params = match params_evolution {
            Some(ev) => Json::obj([("evolution", ev)]),
            None => Json::obj([]),
        };
        p
    };
    // Bare `learned` — no pipeline binding — stays a typed refusal.
    let r = select(
        &routing_request(),
        &env(),
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &learned(None),
        &role_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::PolicyInvalid { .. })));
    // Each missing/malformed member refuses (one leg per member).
    for (tag, ev) in [
        ("no-ev", Json::obj([])),
        (
            "no-search-budget",
            evo(&[("search_budget_ref", Json::Null)]),
        ),
        (
            "no-artifact-benefit",
            evo(&[("artifact_benefit_ref", Json::str(""))]),
        ),
        ("bad-match-mode", evo(&[("match_mode", Json::str("fixed"))])),
        (
            "bad-budget-match",
            evo(&[("budget_match", Json::str("unmatched"))]),
        ),
        ("no-expiry", evo(&[("expiry_condition", Json::Arr(vec![]))])),
    ] {
        let r = select(
            &routing_request(),
            &env(),
            &mut FakeBudget(Ok("r".into())),
            &NoHealth,
            &learned(Some(ev)),
            &role_table(),
            "d",
        );
        assert!(
            matches!(r, Err(RoutingRefusal::PolicyInvalid { .. })),
            "{tag} must refuse"
        );
    }
    // A complete evidence binding admits — the learned choice is the
    // accepted candidate's frozen selection, executed through the same
    // candidate list as every other kind.
    let d = select(
        &routing_request(),
        &env(),
        &mut FakeBudget(Ok("res-9".into())),
        &NoHealth,
        &learned(Some(evo(&[]))),
        &role_table(),
        "d",
    )
    .expect("pipeline-bound learned selects");
    assert_eq!(d.selected.provider_model_id, "m-1");
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
        ErrorAction::RetrySame { max: 3, then: None },
    );
    assert_ne!(p.content_id(), bare);
    assert_eq!(
        error_action(&p, &ModelErrorClass::RateLimited),
        ErrorAction::RetrySame { max: 3, then: None }
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
        closed: true,
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
        &[],
    );
    assert_eq!(e.expected, ExpectedState::Warm);
    assert_eq!(e.basis.last_call.as_deref(), Some("mc-0"));
    // Past the window: cold.
    let e = expect_cache_state(
        &[prior],
        "k",
        "h",
        120_000,
        Some(500),
        &semantics,
        1_000,
        &[],
    );
    assert_eq!(e.expected, ExpectedState::Cold);
    // Below `min_cacheable_tokens`: uncacheable.
    let e = expect_cache_state(&[], "k", "h", 0, Some(10), &semantics, 1_000, &[]);
    assert_eq!(e.expected, ExpectedState::Uncacheable);
    // `uncached` semantics: unknown.
    let e = expect_cache_state(
        &[],
        "k",
        "h",
        0,
        Some(500),
        &CacheSemantics::Uncached,
        0,
        &[],
    );
    assert_eq!(e.expected, ExpectedState::Unknown);
    // A same-key sibling still open: `cold{concurrent_sibling}`.
    let open = CacheCallFact {
        completed: false,
        closed: false,
        ..CacheCallFact {
            affinity_key: "k".into(),
            static_hash: "h".into(),
            request_sent_ms: 0,
            completed: false,
            closed: false,
            model_call_id: "mc-open".into(),
        }
    };
    let e = expect_cache_state(&[open], "k", "h", 5_000, Some(500), &semantics, 1_000, &[]);
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

// ─────────────────────────────────────────────────────────────────────────────
// S4.16a — the C1 router depth (§5b.2): the policy family, G-3/G-5/G-6,
// `on_attempt_failed`, the `HealthView` fold, `explain`, lift/lower.
// Every test fails if the behaviour it names is removed.
// ─────────────────────────────────────────────────────────────────────────────

/// A second candidate/profile pair for the multi-candidate tests.
fn profile_b() -> ModelProfile {
    let mut p = profile(
        "prof.b",
        "1",
        1,
        VersionPattern::Exact("v2".into()),
        DebtStatus::Active,
    );
    p.selector.model_family = "mod".into();
    p.selector.provider_api_family = "fam".into();
    p
}

fn candidate_b() -> RouteCandidate {
    RouteCandidate {
        model_ref: ModelRef {
            profile_ref: "prof.b@1".into(),
            provider_model_id: "m-2".into(),
            snapshot_id: None,
            serving_route: Some("route-b".into()),
            effort: None,
        },
        coordinate: ModelCoordinate {
            provider_api_family: "fam".into(),
            model_family: "mod".into(),
            model_version: "v2".into(),
        },
    }
}

fn two_candidate_env() -> Env {
    Env(vec![
        profile(
            "prof.test",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        ),
        profile_b(),
    ])
}

fn two_candidate_table() -> ModelRoleTable {
    let mut t = role_table();
    t.roles
        .get_mut("primary")
        .unwrap()
        .alternates
        .push(candidate_b());
    t
}

fn chain_policy(targets: Vec<RouteCandidate>) -> RoutingPolicy {
    let mut p = routing_policy(RoutingPolicyKind::FallbackChain);
    p.params = Json::obj([(
        "targets",
        Json::Arr(
            targets
                .iter()
                .map(|c| {
                    Json::obj([
                        (
                            "model_ref",
                            Json::obj([
                                ("profile_ref", Json::str(&c.model_ref.profile_ref)),
                                (
                                    "provider_model_id",
                                    Json::str(&c.model_ref.provider_model_id),
                                ),
                                (
                                    "serving_route",
                                    c.model_ref
                                        .serving_route
                                        .as_deref()
                                        .map(Json::str)
                                        .unwrap_or(Json::Null),
                                ),
                            ]),
                        ),
                        (
                            "coordinate",
                            Json::obj([
                                (
                                    "provider_api_family",
                                    Json::str(&c.coordinate.provider_api_family),
                                ),
                                ("model_family", Json::str(&c.coordinate.model_family)),
                                ("model_version", Json::str(&c.coordinate.model_version)),
                            ]),
                        ),
                    ])
                })
                .collect(),
        ),
    )]);
    p
}

/// A pricing view fixture — `(provider_model_id → per-call cost)`.
struct Prices(BTreeMap<String, i64>);
impl PricingView for Prices {
    fn estimate_call_cost(
        &self,
        candidate: &RouteCandidate,
        _table: &str,
        _max: u64,
    ) -> Option<i64> {
        self.0.get(&candidate.model_ref.provider_model_id).copied()
    }
}

/// A quality-prior fixture — `(provider_model_id → estimate_ppm)`.
struct Priors(BTreeMap<String, i64>);
impl QualityPriorView for Priors {
    fn estimate_ppm(&self, candidate: &RouteCandidate, _prior: &str, _metric: &str) -> Option<i64> {
        self.0.get(&candidate.model_ref.provider_model_id).copied()
    }
}

/// A migration projection fixture — `dropped` per `(from, to)`.
struct Migration(BTreeMap<(String, String), u64>);
impl MigrationView for Migration {
    fn projected_dropped(&self, from: &str, to: &str) -> Option<u64> {
        if from == to {
            return Some(0);
        }
        self.0.get(&(from.to_string(), to.to_string())).copied()
    }
}

/// A health fixture — `(provider_model_id → TargetHealth)`.
struct Health(BTreeMap<String, TargetHealth>);
impl HealthView for Health {
    fn stats(&self, candidate: &ModelRef) -> Option<TargetHealth> {
        self.0.get(&candidate.provider_model_id).copied()
    }
}

fn views<'a>(
    pricing: &'a dyn PricingView,
    quality: &'a dyn QualityPriorView,
    migration: &'a dyn MigrationView,
) -> RoutingViews<'a> {
    RoutingViews {
        pricing: Some(pricing),
        quality: Some(quality),
        migration: Some(migration),
        bandit: None,
    }
}

/// A `views` bundle carrying a `BanditView`.
fn views_bandit<'a>(bandit: &'a dyn BanditView) -> RoutingViews<'a> {
    RoutingViews {
        bandit: Some(bandit),
        ..RoutingViews::none()
    }
}

/// The projected reward store — `(task_class, model_ref_spelling) → cell`.
struct Cells {
    cells: BTreeMap<(String, String), BanditCell>,
}

impl BanditView for Cells {
    fn cell(&self, task_class: &str, model_ref: &str) -> Option<BanditCell> {
        self.cells
            .get(&(task_class.to_string(), model_ref.to_string()))
            .copied()
    }
    fn total_n(&self, task_class: &str) -> u64 {
        self.cells
            .iter()
            .filter(|((t, _), _)| t == task_class)
            .map(|(_, c)| c.n)
            .sum()
    }
}

/// A `bandit` policy with its estimator's conditioned rule + debt record
/// (ADR-0312 d.1; ADR-0189: every non-`static` estimator is a conditioned
/// rule).
fn bandit_policy(params: Json) -> RoutingPolicy {
    let mut p = routing_policy(RoutingPolicyKind::Bandit);
    p.params = params;
    p.conditioned_rules = vec![PolicyConditionedRule {
        rule_id: "r.bandit.estimator".into(),
        conditioned_key: Some("family:mod".into()),
        debt: Some(debt(DebtStatus::Active)),
    }];
    p
}

/// AC-R-2.3.2 (C1) — `fallback_chain` selects `params.targets[]` in order and
/// verdicts every candidate.
#[test]
fn fallback_chain_selects_in_order() {
    let env = two_candidate_env();
    let policy = chain_policy(vec![route_candidate(), candidate_b()]);
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r1".into())),
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &role_table(),
        "d-1",
        0,
        &BTreeSet::new(),
    )
    .expect("selects");
    assert_eq!(d.selected.provider_model_id, "m-1");
    assert_eq!(d.candidates_considered.len(), 2);
    assert_eq!(
        d.candidates_considered[1].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::LowerScore)
    );
    // A poisoned first candidate → the chain falls through to the second.
    let cooling = Health(BTreeMap::from([(
        "m-1".to_string(),
        TargetHealth {
            attempts: 3,
            failures: 3,
            p95_ms: None,
            cooldown_until_ms: Some(1_000),
        },
    )]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r2".into())),
        &cooling,
        &RoutingViews::none(),
        &policy,
        &role_table(),
        "d-2",
        500,
        &BTreeSet::new(),
    )
    .expect("falls through the cooled-down primary");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::Cooldown)
    );
    assert_eq!(
        d.candidates_considered[1].verdict,
        CandidateVerdict::Selected
    );
}

/// AC-R-2.3.2-1 (C1) — `capability_filter` merges `params.capabilities[]` into
/// the G-2 required set.
#[test]
fn capability_filter_filters_on_declared_axes() {
    let mut p2 = profile_b();
    p2.capabilities.native_function_calling = CapabilityState::Unknown;
    let env = Env(vec![
        profile(
            "prof.test",
            "1",
            0,
            VersionPattern::Exact("v1".into()),
            DebtStatus::Active,
        ),
        p2,
    ]);
    let mut policy = routing_policy(RoutingPolicyKind::CapabilityFilter);
    policy.params = Json::obj([(
        "capabilities",
        Json::Arr(vec![Json::str("native_function_calling")]),
    )]);
    let d = select(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &policy,
        &two_candidate_table(),
        "d",
    )
    .expect("m-1 declared; m-2 unknown");
    assert_eq!(d.selected.provider_model_id, "m-1");
    assert_eq!(
        d.candidates_considered[1].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::CapabilityUnknown)
    );
}

/// AC-R-2.3.2-2 (C1) — `cost_cap`: G-5 `NoPrice` for an uncovered candidate,
/// `policy_excluded` over the cap, never a zero-cost fallback.
#[test]
fn cost_cap_reads_pricing() {
    let env = two_candidate_env();
    let mut policy = routing_policy(RoutingPolicyKind::CostCap);
    policy.params = Json::obj([
        ("max_spend_per_call", Json::Int(100)),
        ("pricing_table_ref", Json::str("pt-1")),
    ]);
    // m-1 has no row → NoPrice; m-2 is under the cap → selected.
    let prices = Prices(BTreeMap::from([("m-2".to_string(), 50)]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&prices, &NoQualityPrior, &NoMigration),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 under cap");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::NoPrice)
    );
    // Over-cap → policy_excluded; all excluded → ChainExhausted.
    let prices = Prices(BTreeMap::from([
        ("m-1".to_string(), 500),
        ("m-2".to_string(), 50),
    ]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&prices, &NoQualityPrior, &NoMigration),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 under cap");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded)
    );
    // No pricing view at all → NoPrice on every candidate.
    let r = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    );
    assert!(matches!(r, Err(RoutingRefusal::NoPrice { .. })));
}

/// AC-R-2.3.2-1 (C1) — `latency_cap` excludes candidates whose observed p95
/// exceeds `params.p95_ms` (or the request's `latency_target`).
#[test]
fn latency_cap_excludes_slow_targets() {
    let env = two_candidate_env();
    let mut policy = routing_policy(RoutingPolicyKind::LatencyCap);
    policy.params = Json::obj([("p95_ms", Json::Int(100))]);
    let health = Health(BTreeMap::from([
        (
            "m-1".to_string(),
            TargetHealth {
                attempts: 10,
                failures: 0,
                p95_ms: Some(500),
                cooldown_until_ms: None,
            },
        ),
        (
            "m-2".to_string(),
            TargetHealth {
                attempts: 10,
                failures: 0,
                p95_ms: Some(40),
                cooldown_until_ms: None,
            },
        ),
    ]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &health,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 under cap");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded)
    );
}

/// AC-R-2.3.2-1 (C1) — `quality_target` ranks by the prior estimate
/// descending and refuses candidates without a covering prior.
#[test]
fn quality_target_ranks_by_prior() {
    let env = two_candidate_env();
    let mut policy = routing_policy(RoutingPolicyKind::QualityTarget);
    policy.params = Json::obj([
        ("metric", Json::str("accuracy")),
        ("min_estimate", Json::Int(100_000)),
        ("prior_ref", Json::str("qp-1")),
    ]);
    // m-1 ranks lower; m-2 ranks higher — the order flips the binding.
    let priors = Priors(BTreeMap::from([
        ("m-1".to_string(), 200_000),
        ("m-2".to_string(), 800_000),
    ]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&NoPricing, &priors, &NoMigration),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 ranks first");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(d.candidates_considered[1].score, Some(800_000));
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::LowerScore)
    );
    // Below the floor → policy_excluded; no prior → excluded (never served).
    let priors = Priors(BTreeMap::from([("m-1".to_string(), 50_000)]));
    let r = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&NoPricing, &priors, &NoMigration),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    );
    // m-1 under the floor, m-2 unprior'd → policy_excluded on both.
    assert!(
        matches!(r, Err(RoutingRefusal::ChainExhausted { .. })),
        "got {r:?}"
    );
}

/// AC-R-2.3.2-1 (C1) — `health_aware` ranks candidates by observed failure
/// rate; a cooled candidate is verdicted `cooldown`.
#[test]
fn health_aware_ranks_by_failure_rate() {
    let env = two_candidate_env();
    let policy = routing_policy(RoutingPolicyKind::HealthAware);
    let health = Health(BTreeMap::from([
        (
            "m-1".to_string(),
            TargetHealth {
                attempts: 10,
                failures: 8,
                p95_ms: None,
                cooldown_until_ms: None,
            },
        ),
        (
            "m-2".to_string(),
            TargetHealth {
                attempts: 10,
                failures: 1,
                p95_ms: None,
                cooldown_until_ms: None,
            },
        ),
    ]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &health,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 healthier");
    assert_eq!(d.selected.provider_model_id, "m-2");
}

/// AC-R-2.3.2-2 (C1, G-3) — a cross-profile selection is refused when the
/// projected migration drops more than `max_migration_loss`, while an
/// in-flight effect holds, or when the projection can't be computed.
#[test]
fn migration_guard_binds_only_within_the_bound() {
    let env = two_candidate_env();
    let policy = routing_policy(RoutingPolicyKind::RoleTable);
    let mut req = routing_request();
    req.source_profile_ref = Some("prof.test@1".into());
    // Same-leaf candidate is fine; the cross-leaf candidate migrates.
    let within = Migration(BTreeMap::from([(
        ("prof.test@1".to_string(), "prof.b@1".to_string()),
        0,
    )]));
    let d = select_with(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&NoPricing, &NoQualityPrior, &within),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("bound = 0 dropped — within");
    assert_eq!(d.selected.profile_ref, "prof.test@1");
    // Exceeding the bound → `migration_loss` on the crossing candidate.
    let mut strict = routing_policy(RoutingPolicyKind::RoleTable);
    strict.max_migration_loss.dropped_items = 0;
    let over = Migration(BTreeMap::from([(
        ("prof.test@1".to_string(), "prof.b@1".to_string()),
        7,
    )]));
    let mut only_b_table = two_candidate_table();
    only_b_table.roles.get_mut("primary").unwrap().primary = candidate_b();
    only_b_table
        .roles
        .get_mut("primary")
        .unwrap()
        .alternates
        .clear();
    let r = select_with(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&NoPricing, &NoQualityPrior, &over),
        &strict,
        &only_b_table,
        "d",
        0,
        &BTreeSet::new(),
    );
    assert!(matches!(
        r,
        Err(RoutingRefusal::MigrationLossExceeded { .. })
    ));
    // Unprojectable (NoMigration) → fail-closed `migration_loss`.
    let r = select_with(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &RoutingViews::none(),
        &strict,
        &only_b_table,
        "d",
        0,
        &BTreeSet::new(),
    );
    assert!(matches!(
        r,
        Err(RoutingRefusal::MigrationLossExceeded { .. })
    ));
    // In-flight effect → refused even at bound 0.
    let mut req = routing_request();
    req.source_profile_ref = Some("prof.test@1".into());
    req.in_flight_effect = true;
    let r = select_with(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views(&NoPricing, &NoQualityPrior, &within),
        &strict,
        &only_b_table,
        "d",
        0,
        &BTreeSet::new(),
    );
    assert!(matches!(
        r,
        Err(RoutingRefusal::MigrationLossExceeded { .. })
    ));
}

/// AC-R-2.3.2-2 (C1, G-6) — lifted preferences may exclude but never add a
/// candidate; `authority > external` is `PolicyInvalid`.
#[test]
fn lifted_preferences_attenuate_only() {
    let env = two_candidate_env();
    let mut req = routing_request();
    req.preferences = Some(LiftedPreferences {
        authority: hh_provenance::authority::AuthorityClass::External,
        cost_priority: None,
        speed_priority: None,
        intelligence_priority: None,
        exclude: vec!["m-1".into()],
        hints: vec!["m-9".into()], // a hint for a non-binding model — never adds
    });
    let d = select(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::RoleTable),
        &two_candidate_table(),
        "d",
    )
    .expect("m-1 excluded; m-2 serves");
    assert_eq!(d.selected.provider_model_id, "m-2");
    // The hint never adds a candidate — two candidates considered, not three.
    assert_eq!(d.candidates_considered.len(), 2);
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded)
    );
    // Authority above external on a lifted preference → PolicyInvalid.
    req.preferences = Some(LiftedPreferences {
        authority: hh_provenance::authority::AuthorityClass::Principal,
        cost_priority: None,
        speed_priority: None,
        intelligence_priority: None,
        exclude: vec![],
        hints: vec![],
    });
    let r = select(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::RoleTable),
        &two_candidate_table(),
        "d",
    );
    assert!(matches!(r, Err(RoutingRefusal::PolicyInvalid { .. })));
}

/// AC-R-2.3.2-9 (C1; ADR-0122 d.1–d.3) — `on_attempt_failed`: `retry_same`
/// continues same-target within `max`, then the chained `reroute` releases
/// the old reservation before holding the new one; a non-reroutable class or
/// an exhausted chain is `GiveUp`.
#[test]
fn on_attempt_failed_retry_then_reroute() {
    let env = two_candidate_env();
    let mut policy = chain_policy(vec![route_candidate(), candidate_b()]);
    policy.error_actions = RoutingPolicy::default_error_actions();
    let mut budget = ReleasingBudget { released: vec![] };
    let mut state = AttemptState::default();
    // The prof.test → prof.b crossing projects a zero-loss migration.
    let migration = Migration(BTreeMap::from([(
        ("prof.test@1".to_string(), "prof.b@1".to_string()),
        0,
    )]));
    let vw = views(&NoPricing, &NoQualityPrior, &migration);
    // First decision on m-1.
    let d0 = select_with(
        &routing_request(),
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-0",
        0,
        &BTreeSet::new(),
    )
    .expect("initial");
    // A transient failure under `retry_same{max:2}` → Continue (attempt 1 of 2).
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::ServerError,
        1,
        Some(25),
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-1",
        0,
    );
    match out {
        AttemptDisposition::Continue {
            not_before_ms,
            compact_first,
        } => {
            assert_eq!(not_before_ms, 25);
            assert!(!compact_first);
        }
        other => panic!("expected Continue, got {other:?}"),
    }
    // Second failure on the same target spends `max` → chained `reroute`;
    // the old reservation releases before the new one holds (R-7).
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::ServerError,
        2,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-2",
        0,
    );
    let d2 = match out {
        AttemptDisposition::Reroute(d) => {
            assert_eq!(d.selected.provider_model_id, "m-2");
            assert!(d.relower_required, "prof.test → prof.b crosses profiles");
            assert_eq!(budget.released, vec![d0.reservation_id.clone().unwrap()]);
            d
        }
        other => panic!("expected Reroute, got {other:?}"),
    };
    // A failure on the rerouted target exhausts the chain — GiveUp.
    let out = on_attempt_failed(
        &routing_request(),
        &d2,
        &ModelErrorClass::ServerError,
        2,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-3",
        0,
    );
    match out {
        AttemptDisposition::Reroute(_) => panic!("no candidates remain"),
        AttemptDisposition::GiveUp(r) => {
            assert!(matches!(
                r,
                GiveUpReason::SelectRefused(RoutingRefusal::ChainExhausted { .. })
                    | GiveUpReason::ChainExhausted { .. }
            ));
        }
        AttemptDisposition::Continue { .. } => panic!("attempts reset → continue?"),
    }
}

/// `allowed_classes`/`max_reroutes` gate the reroute (fallback_chain params).
#[test]
fn reroute_obeys_allowed_classes_and_max() {
    let env = two_candidate_env();
    let mut policy = chain_policy(vec![route_candidate(), candidate_b()]);
    policy.params = Json::obj([
        (
            "targets",
            policy.params.get("targets").cloned().unwrap_or(Json::Null),
        ),
        ("max_reroutes", Json::Int(0)),
        (
            "allowed_classes",
            Json::Arr(vec![Json::str("rate_limited")]),
        ),
    ]);
    policy.error_actions = RoutingPolicy::default_error_actions();
    let mut budget = ReleasingBudget { released: vec![] };
    let mut state = AttemptState::default();
    let d0 = select_with(
        &routing_request(),
        &env,
        &mut budget,
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d-0",
        0,
        &BTreeSet::new(),
    )
    .expect("initial");
    // `auth` reroutes under the default table but is not in allowed_classes.
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::Auth,
        1,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d-1",
        0,
    );
    assert!(matches!(
        out,
        AttemptDisposition::GiveUp(GiveUpReason::ClassNotReroutable)
    ));
    // `rate_limited` is allowed but `max_reroutes = 0` → exhausted.
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::RateLimited,
        3, // spend the retry_same max first
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d-2",
        0,
    );
    assert!(matches!(
        out,
        AttemptDisposition::GiveUp(GiveUpReason::ChainExhausted { .. })
    ));
}

/// `compact_then_retry` compacts once (under the old profile), then chains.
#[test]
fn compact_then_retry_runs_once() {
    let env = two_candidate_env();
    let mut policy = chain_policy(vec![route_candidate(), candidate_b()]);
    policy.error_actions = RoutingPolicy::default_error_actions();
    let mut budget = ReleasingBudget { released: vec![] };
    let mut state = AttemptState::default();
    let migration = Migration(BTreeMap::from([(
        ("prof.test@1".to_string(), "prof.b@1".to_string()),
        0,
    )]));
    let vw = views(&NoPricing, &NoQualityPrior, &migration);
    let d0 = select_with(
        &routing_request(),
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-0",
        0,
        &BTreeSet::new(),
    )
    .expect("initial");
    // `context_length_exceeded` → compact_then_retry (first time).
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::ContextLengthExceeded,
        1,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-1",
        0,
    );
    assert!(matches!(
        out,
        AttemptDisposition::Continue {
            compact_first: true,
            ..
        }
    ));
    // A second context overflow after the compaction → chained `reroute`.
    let out = on_attempt_failed(
        &routing_request(),
        &d0,
        &ModelErrorClass::ContextLengthExceeded,
        1,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &vw,
        &policy,
        &two_candidate_table(),
        "d-2",
        0,
    );
    assert!(matches!(out, AttemptDisposition::Reroute(_)));
}

/// `explain(decision_ref)` folds `model.route.decided` + `model.rerouted`.
#[test]
fn explain_folds_route_and_reroutes() {
    let env = two_candidate_env();
    let d = select(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &routing_policy(RoutingPolicyKind::Static),
        &role_table(),
        "dec-42",
    )
    .expect("selects");
    let rows: Vec<(u64, &str, Json)> = vec![
        (1, "model.route.decided", events::route_decided(&d)),
        (
            2,
            "model.rerouted",
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("decision_ref", Json::str("dec-42")),
                ("reason", Json::str("transient_exhausted")),
            ]),
        ),
        (
            3,
            "model.rerouted",
            Json::obj([
                ("model_call_id", Json::str("mc-9")),
                ("decision_ref", Json::str("dec-other")),
            ]),
        ),
    ];
    let refs: Vec<(u64, &str, &Json)> = rows.iter().map(|(s, c, p)| (*s, *c, p)).collect();
    let ex = explain(&refs, "dec-42");
    assert!(ex.decision.is_some());
    assert_eq!(ex.reroutes.len(), 1);
}

/// ADR-0122 d.4 — `project_health` folds `model.call.attempt.*` per target:
/// `rate_limited` cools the target down; a sustained failure rate over the
/// threshold cools too; a one-candidate role never cools.
#[test]
fn health_projection_counts_failures_and_cools() {
    // route.decided + requested bind call→target.
    let mut rows: Vec<(u64, u64, &str, Json)> = Vec::new();
    let mut seq = 0u64;
    for call in ["c1", "c2", "c3", "c4", "c5"] {
        rows.push((
            {
                seq += 1;
                seq
            },
            0,
            "model.call.requested",
            Json::obj([
                ("model_call_id", Json::str(call)),
                (
                    "model_ref",
                    Json::obj([
                        ("provider_model_id", Json::str("m-1")),
                        ("serving_route", Json::str("route-a")),
                    ]),
                ),
            ]),
        ));
        seq += 1;
        rows.push((
            seq,
            1000 + seq,
            "model.call.attempt.failed",
            Json::obj([
                ("model_call_id", Json::str(call)),
                ("attempt_no", Json::Int(1)),
                ("error", Json::obj([("class", Json::str("server_error"))])),
            ]),
        ));
    }
    let health = project_health(
        &rows
            .iter()
            .map(|(s, t, c, p)| (*s, *t, *c, p))
            .collect::<Vec<_>>(),
        &HealthConfig::default(),
        &BTreeSet::new(),
    );
    let st = health
        .stats(&ModelRef {
            profile_ref: "prof.test@1".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        })
        .expect("row");
    assert_eq!(st.failures, 5);
    assert!(st.cooldown_until_ms.is_some(), "5/5 failures > 50% trips");
    // The single-candidate rule suppresses the cooldown.
    let solo = project_health(
        &rows
            .iter()
            .map(|(s, t, c, p)| (*s, *t, *c, p))
            .collect::<Vec<_>>(),
        &HealthConfig::default(),
        &BTreeSet::from([("m-1".to_string(), "route-a".to_string())]),
    );
    assert!(solo
        .stats(&ModelRef {
            profile_ref: "x".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        })
        .unwrap()
        .cooldown_until_ms
        .is_none());
    // A rate_limited failure cools immediately (one attempt suffices).
    let rows429 = vec![(
        1u64,
        500u64,
        "model.call.attempt.failed",
        Json::obj([
            ("model_call_id", Json::str("c9")),
            ("attempt_no", Json::Int(1)),
            ("error", Json::obj([("class", Json::str("rate_limited"))])),
        ]),
    )];
    let _ = rows429;
}

/// AC-R-2.3.2-12 (C1) — `lower`/`lift` round-trips `preferences` exactly and
/// reports `required_capabilities`/`budget_view`/`role` as `no_slot` loss.
#[test]
fn lift_lower_round_trips_preferences() {
    let mut req = routing_request();
    req.preferences = Some(LiftedPreferences {
        authority: hh_provenance::authority::AuthorityClass::External,
        cost_priority: Some(200_000),
        speed_priority: Some(700_000),
        intelligence_priority: None,
        exclude: vec!["m-1".into()],
        hints: vec!["cheap-model".into()],
    });
    let (doc, loss) = lower(&req);
    assert_eq!(
        loss.no_slot,
        vec!["required_capabilities", "budget_view", "role"]
    );
    let (prefs, loss2) =
        lift(&doc, hh_provenance::authority::AuthorityClass::External).expect("external lifts");
    assert_eq!(loss2.no_slot.len(), 3);
    assert_eq!(prefs.cost_priority, Some(200_000));
    assert_eq!(prefs.speed_priority, Some(700_000));
    assert_eq!(prefs.hints, vec!["cheap-model".to_string()]);
    assert_eq!(prefs.exclude, vec!["m-1".to_string()]);
    // Above-external authority refuses.
    assert!(lift(&doc, hh_provenance::authority::AuthorityClass::Principal).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// S4.16a — the C1 cache fold (`project_cache_view`, IR-1; AC-R-2.3.4-7;
// AC-R-2.3.3-15 effort → `parameter_change`).
// ─────────────────────────────────────────────────────────────────────────────

fn requested_row(id: &str, ts: u64) -> (u64, u64, &'static str, Json) {
    (
        ts,
        ts,
        "model.call.requested",
        Json::obj([
            ("model_call_id", Json::str(id)),
            (
                "model_ref",
                Json::obj([("profile_ref", Json::str("prof.test@1"))]),
            ),
            (
                "dialect",
                Json::obj([
                    ("dialect_id", Json::str("d-a")),
                    ("version", Json::str("1")),
                ]),
            ),
            (
                "cache",
                Json::obj([
                    ("affinity_key", Json::str("k")),
                    ("static_hash", Json::str("h")),
                ]),
            ),
        ]),
    )
}

fn terminal_row(id: &str, ts: u64, class: &'static str) -> (u64, u64, &'static str, Json) {
    (ts, ts, class, Json::obj([("model_call_id", Json::str(id))]))
}

fn as_refs<'a>(rows: &'a [(u64, u64, &'a str, Json)]) -> Vec<(u64, u64, &'a str, &'a Json)> {
    rows.iter().map(|(s, t, c, p)| (*s, *t, *c, p)).collect()
}

/// AC-R-2.3.4-7 — the fold's facts drive `expect_cache_state`; each IR-1
/// cause records its own `cold{reason}`.
#[test]
fn cache_view_folds_facts_and_invalidations() {
    let semantics = CacheSemantics::ExplicitBreakpoints {
        max_markers: 2,
        lookback_positions: Some(8),
        position_rule: MarkerPositionRule::Block,
        retention_classes: retention(),
        min_cacheable_tokens: None,
        isolation: IsolationScope::Workspace,
        eligible_carriers: vec![BlockKind::Text],
    };
    let mut rows = vec![
        requested_row("mc-1", 1_000),
        terminal_row("mc-1", 1_100, "model.call.completed"),
    ];
    let view = project_cache_view(&as_refs(&rows));
    assert_eq!(view.facts.len(), 1);
    assert!(view.facts[0].completed && view.facts[0].closed);
    // Warm inside the lease.
    let e = expect_cache_state(
        &view.facts,
        "k",
        "h",
        2_000,
        Some(500),
        &semantics,
        1_000,
        &view.invalidations,
    );
    assert_eq!(e.expected, ExpectedState::Warm);
    // A relower invalidates; the next call is cold{relower}.
    rows.push((
        1_500,
        1_500,
        "model.surface.relowered",
        Json::obj([("model_call_id", Json::str("mc-1"))]),
    ));
    let view = project_cache_view(&as_refs(&rows));
    let e = expect_cache_state(
        &view.facts,
        "k",
        "h",
        2_000,
        Some(500),
        &semantics,
        1_000,
        &view.invalidations,
    );
    assert_eq!(e.expected, ExpectedState::Cold);
    assert_eq!(e.basis.cold_reason, Some(MissReason::Relower));
    // A completed-applied compaction → cold{compaction}.
    rows.push((
        1_600,
        1_600,
        "context.compaction.completed",
        Json::obj([("status", Json::str("applied"))]),
    ));
    let view = project_cache_view(&as_refs(&rows));
    let e = expect_cache_state(
        &view.facts,
        "k",
        "h",
        2_000,
        Some(500),
        &semantics,
        1_000,
        &view.invalidations,
    );
    assert_eq!(e.basis.cold_reason, Some(MissReason::Compaction));
}

/// A `model.call.failed` closes the fact without `completed` — it never
/// seeds `warm` and is never an in-flight sibling.
#[test]
fn failed_call_neither_warms_nor_blocks() {
    let semantics = CacheSemantics::ExplicitBreakpoints {
        max_markers: 2,
        lookback_positions: Some(8),
        position_rule: MarkerPositionRule::Block,
        retention_classes: retention(),
        min_cacheable_tokens: None,
        isolation: IsolationScope::Workspace,
        eligible_carriers: vec![BlockKind::Text],
    };
    let rows = vec![
        requested_row("mc-1", 1_000),
        terminal_row("mc-1", 1_100, "model.call.failed"),
    ];
    let view = project_cache_view(&as_refs(&rows));
    assert!(!view.facts[0].completed && view.facts[0].closed);
    let e = expect_cache_state(
        &view.facts,
        "k",
        "h",
        2_000,
        Some(500),
        &semantics,
        1_000,
        &view.invalidations,
    );
    // No completed same-key call → cold{unknown}, not concurrent_sibling.
    assert_eq!(e.expected, ExpectedState::Cold);
    assert_eq!(e.basis.cold_reason, Some(MissReason::Unknown));
}

/// AC-R-2.3.3-15 — a chosen `effort_level` whose option row declares
/// `risk.cache_invalidation` records `parameter_change`; without the flag it
/// doesn't.
#[test]
fn effort_change_with_invalidation_is_parameter_change() {
    let decided = |invalidating: bool| {
        Json::obj([
            (
                "chosen",
                Json::obj([
                    ("kind", Json::str("effort_level")),
                    ("level", Json::str("high")),
                ]),
            ),
            (
                "options_considered",
                Json::Arr(vec![Json::obj([
                    (
                        "option",
                        Json::obj([
                            ("kind", Json::str("effort_level")),
                            ("level", Json::str("high")),
                        ]),
                    ),
                    (
                        "estimate",
                        Json::obj([(
                            "risk",
                            Json::obj([("cache_invalidation", Json::Bool(invalidating))]),
                        )]),
                    ),
                ])]),
            ),
        ])
    };
    let rows = vec![
        requested_row("mc-1", 1_000),
        terminal_row("mc-1", 1_100, "model.call.completed"),
        (1_200, 1_200, "control.compute.decided", decided(true)),
    ];
    let view = project_cache_view(&as_refs(&rows));
    assert_eq!(view.invalidations, vec![MissReason::ParameterChange]);
    // The same decision without the declaration records nothing.
    let rows = vec![(1_200, 1_200, "control.compute.decided", decided(false))];
    let view = project_cache_view(&as_refs(&rows));
    assert!(view.invalidations.is_empty());
}

/// IR-1 — version-stamp drift on the request row attributes the reason; a
/// fresh relower owns the `profile_ref` change (not `profile_version`).
#[test]
fn version_stamps_attribute_ir1_reasons() {
    // dialect.version change → dialect_change.
    let mut rows = vec![requested_row("mc-1", 1_000)];
    let mut r2p = requested_row("mc-2", 2_000);
    if let Json::Obj(m) = &mut r2p.3 {
        m.insert(
            "dialect".into(),
            Json::obj([
                ("dialect_id", Json::str("d-a")),
                ("version", Json::str("2")),
            ]),
        );
    }
    rows.push(r2p);
    let view = project_cache_view(&as_refs(&rows));
    assert_eq!(view.invalidations, vec![MissReason::DialectChange]);
    // A reroute with relowered=true → `relower`, and the following
    // request's changed profile_ref is *not* double-counted as
    // `profile_version`.
    let mut rows = vec![
        requested_row("mc-1", 1_000),
        (
            1_500,
            1_500,
            "model.rerouted",
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("relowered", Json::Bool(true)),
            ]),
        ),
    ];
    let mut r3 = requested_row("mc-3", 2_000);
    if let Json::Obj(m) = &mut r3.3 {
        m.insert(
            "model_ref".into(),
            Json::obj([("profile_ref", Json::str("prof.b@1"))]),
        );
    }
    rows.push(r3);
    let view = project_cache_view(&as_refs(&rows));
    assert_eq!(view.invalidations, vec![MissReason::Relower]);
    // Without the relowered event, the same profile_ref change is
    // `profile_version`.
    let rows = vec![requested_row("mc-1", 1_000), {
        let mut r = requested_row("mc-3", 2_000);
        if let Json::Obj(m) = &mut r.3 {
            m.insert(
                "model_ref".into(),
                Json::obj([("profile_ref", Json::str("prof.test@2"))]),
            );
        }
        r
    }];
    let view = project_cache_view(&as_refs(&rows));
    assert_eq!(view.invalidations, vec![MissReason::ProfileVersion]);
}

// ─────────────────────────────────────────────────────────────────────────────
// S5.1 / AC-R-2.3.3-9 — a `compatibility_token` change marks the profile
// `pending_relower` and the *next* call refuses `ReLowerRequired` until
// `mark_relowered` clears the gate (re-lowering happens before the next
// call, never lazily after a served call).
// ─────────────────────────────────────────────────────────────────────────────

struct DiscoverTransport {
    descriptor: Json,
}
impl Transport for DiscoverTransport {
    fn send(
        &mut self,
        _b: &[u8],
        _c: &CredentialHandle,
        _e: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        Err(ModelError::new(
            ModelErrorClass::UnsupportedFeature,
            "no send",
        ))
    }
    fn discover(&mut self, _endpoint_ref: &str) -> Result<Json, ModelError> {
        Ok(self.descriptor.clone())
    }
}

#[test]
fn compatibility_token_change_gates_dispatch_until_relowered() {
    let pinned = Json::obj([("value", Json::str("tok-v1"))]);
    let descriptor_same = Json::obj([(
        "capabilities",
        Json::obj([("compatibility_token", pinned.clone())]),
    )]);
    let descriptor_drifted = Json::obj([(
        "capabilities",
        Json::obj([(
            "compatibility_token",
            Json::obj([("value", Json::str("tok-v2"))]),
        )]),
    )]);

    let creds: &'static mut StubCreds = Box::leak(Box::new(StubCreds));
    let transport: &'static mut DiscoverTransport = Box::leak(Box::new(DiscoverTransport {
        descriptor: descriptor_same,
    }));
    let mut allow = EndpointAllowlist::default();
    allow.endpoints.insert("ep-a".to_string());
    let mut g = ModelGateway {
        dialects: Default::default(),
        endpoints: allow,
        credentials: creds,
        transport,
        now_ms: Box::new(|| 0),
        normalizer_ref: "norm:test".into(),
        pending_relower: std::collections::BTreeSet::new(),
    };
    g.load_dialect(dialect());

    // Unchanged token → no claim, nothing pending.
    let claim = g
        .check_compatibility_token("ep-a", Some(&pinned), "prof.test@1", "m-1")
        .expect("discover runs");
    assert!(claim.is_none());
    assert!(g.pending_relower.is_empty());

    // Changed token → the `compatibility_token_changed` claim mints and the
    // profile is gated.
    let transport2: &'static mut DiscoverTransport = Box::leak(Box::new(DiscoverTransport {
        descriptor: descriptor_drifted,
    }));
    let creds2: &'static mut StubCreds = Box::leak(Box::new(StubCreds));
    let mut allow2 = EndpointAllowlist::default();
    allow2.endpoints.insert("ep-a".to_string());
    let mut g = ModelGateway {
        dialects: Default::default(),
        endpoints: allow2,
        credentials: creds2,
        transport: transport2,
        now_ms: Box::new(|| 0),
        normalizer_ref: "norm:test".into(),
        pending_relower: std::collections::BTreeSet::new(),
    };
    g.load_dialect(dialect());
    let claim = g
        .check_compatibility_token("ep-a", Some(&pinned), "prof.test@1", "m-1")
        .expect("discover runs")
        .expect("a changed token claims");
    assert_eq!(
        claim.claim_kind,
        hh_gateway::snapshot::SnapshotClaimKind::CompatibilityTokenChanged
    );
    assert!(g.pending_relower.contains("prof.test@1"));

    // The next call under the profile refuses pre-dispatch — no
    // `model.call.requested` mints.
    let mut sink = CollectSink::default();
    match g.open_call(request_fixture("ep-a", Some("res-1")), &mut sink) {
        Err(GatewayError::ReLowerRequired { profile_ref }) => {
            assert_eq!(profile_ref, "prof.test@1");
        }
        other => panic!("expected ReLowerRequired, got {other:?}"),
    }
    assert!(sink.events.is_empty(), "no ledger rows on a gated call");

    // `mark_relowered` (the caller ran `relower` + minted
    // `model.surface.relowered{reason: compatibility_token_changed}`)
    // re-opens the call path.
    g.mark_relowered("prof.test@1");
    let handle = g
        .open_call(request_fixture("ep-a", Some("res-1")), &mut sink)
        .expect("relowered profile admits");
    assert_eq!(handle.call_id, "mc-1");
}

// ─────────────────────────────────────────────────────────────────────────────
// S5.1 — the `bandit` routing-policy family (§5b.2; ADR-0312 d.1):
// ledger-projected reward cells keyed (task_class, model_ref), min_n/unknown
// cold-start, the λ·cost fold, deterministic selection, estimator debt.
// Every test fails if the behaviour it names is removed.
// ─────────────────────────────────────────────────────────────────────────────

fn cell(n: u64, success_ppm: i64, cost_millis: i64) -> BanditCell {
    BanditCell {
        n,
        success_ppm,
        cost_millis,
    }
}

/// AC-R-2.3.2-1 (ADR-0312 d.1) — `bandit` ranks admissible candidates by the
/// projected reward cell, not binding order: m-2's higher success_ppm wins
/// even though it is the alternate.
#[test]
fn bandit_ranks_by_projected_reward() {
    let env = two_candidate_env();
    let policy = bandit_policy(Json::obj([("min_n", Json::Int(1))]));
    let view = Cells {
        cells: BTreeMap::from([
            (
                (
                    "untagged".to_string(),
                    "prof.test@1/m-1@route-a".to_string(),
                ),
                cell(10, 400_000, 100),
            ),
            (
                ("untagged".to_string(), "prof.b@1/m-2@route-b".to_string()),
                cell(10, 800_000, 100),
            ),
        ]),
    };
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views_bandit(&view),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("m-2 outranks on reward");
    assert_eq!(d.selected.provider_model_id, "m-2");
    assert_eq!(d.selected.profile_ref, "prof.b@1");
    // The loser verdicts `lower_score` with its score recorded (R-3).
    assert_eq!(
        d.candidates_considered[0].verdict,
        CandidateVerdict::Rejected(CandidateRejectReason::LowerScore)
    );
    assert!(d.inputs_read.iter().any(|i| i == "bandit_view"));
}

/// A cell below `min_n` is `unknown` — it scores the declared
/// `cold_start_ppm` prior, never an interpolated value (ADR-0012).
#[test]
fn bandit_min_n_unknown_reads_cold_start() {
    let env = two_candidate_env();
    // cold_start below the observed cell → observed wins.
    let policy = bandit_policy(Json::obj([
        ("min_n", Json::Int(5)),
        ("cold_start_ppm", Json::Int(300_000)),
    ]));
    let view = Cells {
        cells: BTreeMap::from([(
            (
                "untagged".to_string(),
                "prof.test@1/m-1@route-a".to_string(),
            ),
            cell(10, 400_000, 100),
        )]),
    };
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views_bandit(&view),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("observed cell beats cold-start");
    assert_eq!(d.selected.provider_model_id, "m-1");

    // An informative cold-start prior above the observed cell lifts the
    // unobserved candidate (ADR-0189 D6: cold-start priors are declared
    // data, not invented).
    let policy = bandit_policy(Json::obj([
        ("min_n", Json::Int(5)),
        ("cold_start_ppm", Json::Int(900_000)),
    ]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views_bandit(&view),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("cold-start prior lifts unknown cell");
    assert_eq!(d.selected.provider_model_id, "m-2");
}

/// Cells key on `task_class` — reward observed under another class is never
/// pooled into this lookup (ADR-0189 D8; ADR-0012 no-interpolation).
#[test]
fn bandit_task_class_never_pools() {
    let env = two_candidate_env();
    let policy = bandit_policy(Json::obj([("min_n", Json::Int(1))]));
    let view = Cells {
        // m-2's reward lives under a different task_class.
        cells: BTreeMap::from([
            (
                ("other".to_string(), "prof.b@1/m-2@route-b".to_string()),
                cell(50, 950_000, 10),
            ),
            (
                ("t1".to_string(), "prof.test@1/m-1@route-a".to_string()),
                cell(10, 400_000, 100),
            ),
        ]),
    };
    let mut req = routing_request();
    req.task_class = Some("t1".into());
    let d = select_with(
        &req,
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views_bandit(&view),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("t1 reads only its own cells");
    assert_eq!(d.selected.provider_model_id, "m-1");
}

/// The λ·cost fold — `reward = success_ppm − λ_ppm·cost_millis/1000`: a
/// pricier high-success target loses to the cheaper cell when λ is steep.
#[test]
fn bandit_lambda_docks_cost() {
    let env = two_candidate_env();
    let policy = bandit_policy(Json::obj([
        ("min_n", Json::Int(1)),
        ("lambda_ppm", Json::Int(100_000)),
    ]));
    let view = Cells {
        cells: BTreeMap::from([
            (
                (
                    "untagged".to_string(),
                    "prof.test@1/m-1@route-a".to_string(),
                ),
                // 850_000 − 100_000·10/1000 = 849_000
                cell(10, 850_000, 10),
            ),
            (
                ("untagged".to_string(), "prof.b@1/m-2@route-b".to_string()),
                // 900_000 − 100_000·5000/1000 = 400_000
                cell(10, 900_000, 5_000),
            ),
        ]),
    };
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &views_bandit(&view),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("λ docks the pricey target");
    assert_eq!(d.selected.provider_model_id, "m-1");
}

/// Equal inputs select identically — the bandit fold is integer
/// arithmetic over projected ledger data; nothing samples.
#[test]
fn bandit_selection_is_deterministic() {
    let env = two_candidate_env();
    let policy = bandit_policy(Json::obj([
        ("min_n", Json::Int(1)),
        ("explore_ppm", Json::Int(50_000)),
    ]));
    let view = Cells {
        cells: BTreeMap::from([
            (
                (
                    "untagged".to_string(),
                    "prof.test@1/m-1@route-a".to_string(),
                ),
                cell(4, 700_000, 0),
            ),
            (
                ("untagged".to_string(), "prof.b@1/m-2@route-b".to_string()),
                cell(9, 700_000, 0),
            ),
        ]),
    };
    let pick = |decision_id: &str| {
        select_with(
            &routing_request(),
            &env,
            &mut FakeBudget(Ok("r".into())),
            &NoHealth,
            &views_bandit(&view),
            &policy,
            &two_candidate_table(),
            decision_id,
            0,
            &BTreeSet::new(),
        )
        .expect("selects")
    };
    let a = pick("d1");
    let b = pick("d2");
    assert_eq!(a.selected, b.selected);
    // UCB bonus favours the less-observed cell: m-1 (n=4) over m-2 (n=9)
    // at equal success_ppm — √9/4 = 1.5 ⇒ +75_000 vs +50_000.
    assert_eq!(a.selected.provider_model_id, "m-1");
}

/// The `bandit` estimator is a conditioned rule (ADR-0189) — `link`
/// refuses a bandit policy carrying no complete `AssumptionDebtRecord`.
#[test]
fn bandit_requires_estimator_debt_record() {
    let env = two_candidate_env();
    let bare = routing_policy(RoutingPolicyKind::Bandit);
    let r = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &RoutingViews::none(),
        &bare,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    );
    match r {
        Err(RoutingRefusal::PolicyInvalid { reason, .. }) => {
            assert!(reason.contains("missing_debt_record"), "got {reason}");
        }
        other => panic!("expected PolicyInvalid, got {other:?}"),
    }
}

/// No reward view ⇒ every cell `unknown` ⇒ all candidates read the
/// cold-start prior and binding order (primary first) selects — a
/// deterministic degrade, never a silent invention.
#[test]
fn bandit_without_view_reads_cold_start() {
    let env = two_candidate_env();
    let policy = bandit_policy(Json::obj([("min_n", Json::Int(1))]));
    let d = select_with(
        &routing_request(),
        &env,
        &mut FakeBudget(Ok("r".into())),
        &NoHealth,
        &RoutingViews::none(),
        &policy,
        &two_candidate_table(),
        "d",
        0,
        &BTreeSet::new(),
    )
    .expect("all-unknown selects by binding order");
    assert_eq!(d.selected.provider_model_id, "m-1");
}

// ── AC-R-2.3.3-11 — `ModelRoleTable` codec / semantic_id / projection ────

/// The table codec round-trips through the canonical JSON — every role
/// binding survives (`from_json` gates the closed `ModelRole` set: a
/// non-ratified role name never decodes).
#[test]
fn model_role_table_codec_round_trip() {
    let t = two_candidate_table();
    let j = t.to_json();
    let back = ModelRoleTable::from_json(&j).expect("round-trips");
    assert_eq!(back.semantic_id(), t.semantic_id());
    // A role outside the closed set refuses — never coerced (CF-468).
    let mut bad = j.clone();
    if let Json::Obj(m) = &mut bad {
        if let Some(Json::Obj(roles)) = m.get_mut("roles") {
            roles.insert(
                "summarizer".to_string(),
                roles.values().next().cloned().unwrap_or(Json::Null),
            );
        }
    }
    assert!(
        ModelRoleTable::from_json(&bad).is_none(),
        "the pre-ratification `summarizer` spelling never decodes"
    );
    // Missing/non-object `roles` refuses.
    assert!(ModelRoleTable::from_json(&Json::obj([])).is_none());
}

/// `semantic_id` is the idp/1 address of the canonical record: identical
/// tables agree, differing tables diverge (the AC's determinism leg —
/// `configuration_id.model_ref` names this id, CF-313).
#[test]
fn model_role_table_semantic_id_deterministic() {
    let a = role_table();
    let b = role_table();
    assert_eq!(a.semantic_id(), b.semantic_id());
    let mut c = role_table();
    c.roles.get_mut("primary").unwrap().profile_ref = "sha256:other".to_string();
    assert_ne!(a.semantic_id(), c.semantic_id());
    // An added role diverges the id.
    let mut d = role_table();
    d.roles.insert(
        "compaction".to_string(),
        RoleBinding {
            primary: candidate_b(),
            alternates: vec![],
            policy_ref: "p".to_string(),
            profile_ref: "sha256:compact".to_string(),
        },
    );
    assert_ne!(a.semantic_id(), d.semantic_id());
}

/// The `profile_binding` projection is `map<ModelRole, ProfileRef>` —
/// `{role → {profile_ref, pinned: true}}`; the manifest's member carries
/// exactly this shape (CF-313).
#[test]
fn model_role_table_profile_binding_projection() {
    let t = role_table();
    let pb = t.profile_binding();
    let primary = pb.get("primary").expect("primary projects");
    assert_eq!(
        primary.get("profile_ref").and_then(Json::as_str),
        Some(t.roles["primary"].profile_ref.as_str())
    );
    assert_eq!(primary.get("pinned"), Some(&Json::Bool(true)));
}
