//! R2.7 — the `RoutingPolicy`/`RoutingDecision` durable codecs and the
//! `check_reroute_order` oracle (DF-S1.18-1's machine cells; ADR-0121
//! d.4/d.5, ADR-0122 d.3, AC-R-2.3.2-4).
//!
//! The pair is the boundary's CC1 spine: `RoutingPolicy::from_json`
//! decodes the sealed `router` slot's policy document;
//! `RoutingDecision::from_json` reads a `model.route.decided` payload back
//! off the durable tail (the driver's reroute fold replays it — a ledger
//! row that can't be read is never silently defaulted).
//! `check_reroute_order` is the emitted-row oracle for the ADR-0122 d.3
//! sequence `attempt.failed → (compaction?) → surface.relowered →
//! rerouted → attempt.started`.
//!
//! Fixture-only (the ticket's offline ceiling): live transport/provider
//! arms stay typed refusals — the last cells pin that honesty.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use hh_compiler::profile::{
    self as prof, DebtStatus, EvidenceRef, ModelCoordinate, ModelProfile, SelectorView,
    VersionPattern,
};
use hh_gateway::*;
use hh_wire::json::Json;

// ── fixtures ────────────────────────────────────────────────────────────────

fn debt(status: DebtStatus) -> prof::ProfileDebtRecord {
    prof::ProfileDebtRecord {
        rule_id: "p.expiry".into(),
        hypothesis: "declared contract".into(),
        evidence_refs: vec![EvidenceRef::legacy("sha256:ev")],
        owner: "test:owner".into(),
        reach_via: vec![],
        expiry_condition: prof::ExpiryCondition {
            kind: prof::ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "t:1".into(),
        removal_test: Some(prof::RemovalTest::new(prof::RemovalTestKind::Documentation)),
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

fn profile(id: &str, version: &str, pattern: VersionPattern) -> ModelProfile {
    let caps = prof::ProfileCapabilities {
        native_function_calling: prof::CapabilityState::Declared,
        max_output: Some(8_192),
        ..prof::ProfileCapabilities::default()
    };
    let mut p = ModelProfile {
        profile_id: id.into(),
        version: version.into(),
        content_hash: String::new(),
        selector: prof::ProfileSelector {
            provider_api_family: "fam".into(),
            model_family: "mod".into(),
            version_pattern: pattern,
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![prof::ModelRole::Primary],
        },
        extends: None,
        capabilities: caps,
        rules: vec![],
        ext: BTreeMap::new(),
        expiry: debt(DebtStatus::Active),
        compatibility: prof::ProfileCompatibility {
            inventory_version: "1".into(),
            min_compiler_version: "0.0.0".into(),
        },
        tests: Json::Null,
    };
    p.content_hash = prof::profile_identity(&p);
    p
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

/// A migration view fixture — `(from_profile_ref, to_profile_ref) →
/// dropped` projections; unprojectable pairs answer `None` (fail-closed).
struct Migration(BTreeMap<(String, String), u64>);
impl MigrationView for Migration {
    fn projected_dropped(&self, from: &str, to: &str) -> Option<u64> {
        if from == to {
            return Some(0);
        }
        self.0.get(&(from.to_string(), to.to_string())).copied()
    }
}

struct FakeBudget;
impl BudgetPort for FakeBudget {
    fn reserve(&mut self, _b: &str, _max: u64, _h: &str) -> Result<String, String> {
        Ok("res-1".into())
    }
    fn release(&mut self, _r: &str) -> Result<(), String> {
        Ok(())
    }
}

fn coordinate(version: &str) -> ModelCoordinate {
    ModelCoordinate {
        provider_api_family: "fam".into(),
        model_family: "mod".into(),
        model_version: version.into(),
    }
}

fn candidate_a() -> RouteCandidate {
    RouteCandidate {
        model_ref: ModelRef {
            profile_ref: "prof.a@1".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        },
        coordinate: coordinate("v1"),
    }
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
        coordinate: coordinate("v2"),
    }
}

fn table() -> ModelRoleTable {
    let mut roles = BTreeMap::new();
    roles.insert(
        "primary".to_string(),
        RoleBinding {
            primary: candidate_a(),
            alternates: vec![candidate_b()],
            policy_ref: "policy.test@1".into(),
            profile_ref: "prof.a@1".into(),
        },
    );
    ModelRoleTable { roles }
}

fn request() -> RoutingRequest {
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

/// A `static` policy — plus the members the codec must carry verbatim:
/// `role_scope`, a chained `error_actions` row, `allow_unknown`,
/// `conditioned_rules[]`, `ext`.
fn full_policy() -> RoutingPolicy {
    let mut error_actions = RoutingPolicy::default_error_actions();
    error_actions.insert(
        "overloaded".into(),
        ErrorAction::RetrySame {
            max: 3,
            then: Some(Box::new(ErrorAction::CompactThenRetry {
                then: Some(Box::new(ErrorAction::Reroute)),
            })),
        },
    );
    let rule = PolicyConditionedRule {
        rule_id: "r.fam".into(),
        conditioned_key: Some("family:mod".into()),
        debt: Some(debt(DebtStatus::Active)),
    };
    RoutingPolicy {
        policy_id: "policy.test".into(),
        version: "1".into(),
        content_hash: String::new(),
        role_scope: BTreeSet::from(["primary".to_string(), "utility".to_string()]),
        kind: RoutingPolicyKind::Static,
        params: Json::obj([(
            "target",
            Json::obj([
                (
                    "model_ref",
                    Json::obj([
                        ("profile_ref", Json::str("prof.a@1")),
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
        error_actions,
        max_migration_loss: MigrationLossBound {
            dropped_items: 2,
            no_in_flight_tool_call: true,
        },
        allow_unknown: Some(PolicyConditionedRule {
            rule_id: "r.unknown".into(),
            conditioned_key: None,
            debt: Some(debt(DebtStatus::Active)),
        }),
        conditioned_rules: vec![rule],
        ext: BTreeMap::from([("sealed".to_string(), Json::Bool(true))]),
    }
}

fn decision() -> RoutingDecision {
    RoutingDecision {
        decision_id: "d-1".into(),
        model_call_id: Some("mc-1".into()),
        role: "primary".into(),
        selected: candidate_a().model_ref,
        reservation_id: Some("res-1".into()),
        policy_ref: "policy.test@1".into(),
        policy_version_id: "policy.test@1:1".into(),
        rule_ids_fired: vec!["r.fam".into()],
        candidates_considered: vec![
            router::Candidate {
                model_ref: "route-a".into(),
                verdict: CandidateVerdict::Selected,
                score: Some(9),
            },
            router::Candidate {
                model_ref: "route-b".into(),
                verdict: CandidateVerdict::Rejected(CandidateRejectReason::PolicyExcluded),
                score: None,
            },
        ],
        inputs_read: vec![
            "profile_resolver".into(),
            "budget_account".into(),
            "health_view".into(),
            "role_table".into(),
        ],
        deviation: false,
        relower_required: false,
    }
}

fn failed_row(seq: u64, mc: &str) -> (u64, String, Json) {
    (
        seq,
        "model.call.attempt.failed".to_string(),
        Json::obj([
            ("model_call_id", Json::str(mc)),
            ("attempt_no", Json::Int(1)),
            ("will_retry", Json::Bool(true)),
        ]),
    )
}

fn started_row(seq: u64, mc: &str) -> (u64, String, Json) {
    (
        seq,
        "model.call.attempt.started".to_string(),
        Json::obj([
            ("model_call_id", Json::str(mc)),
            ("attempt_no", Json::Int(2)),
        ]),
    )
}

fn relower_row(seq: u64, mc: &str) -> (u64, String, Json) {
    (
        seq,
        "model.surface.relowered".to_string(),
        Json::obj([
            ("model_call_id", Json::str(mc)),
            ("old_profile_ref", Json::str("prof.a@1")),
            ("new_profile_ref", Json::str("prof.b@1")),
            ("reason", Json::str("profile_boundary")),
        ]),
    )
}

fn rerouted_row(seq: u64, mc: &str, relowered: bool) -> (u64, String, Json) {
    (
        seq,
        "model.rerouted".to_string(),
        crate_rerouted(mc, relowered),
    )
}

fn crate_rerouted(mc: &str, relowered: bool) -> Json {
    Json::obj([
        ("model_call_id", Json::str(mc)),
        ("from", Json::str("m-1")),
        ("to", Json::str("m-2")),
        ("reason", Json::str("overloaded")),
        ("attempt_no", Json::Int(1)),
        ("relowered", Json::Bool(relowered)),
        ("decision_ref", Json::str("d-2")),
    ])
}

// ── AC-R-2.3.2-6/CC1 — `RoutingPolicy` JSON round-trip ───────────────────────

/// `to_json`/`from_json` is a round-trip pair: every member survives, and
/// an absent `content_hash` recomputes `content_id()` — the declared
/// address of the rest (the hash rides the record, never the other way).
#[test]
fn r2_7_routing_policy_json_round_trip() {
    let mut p = full_policy();
    p.content_hash = p.content_id();
    let j = p.to_json();
    // `content_hash` is excluded from the payload — it is the address.
    assert!(j.get("content_hash").is_none());
    let back = RoutingPolicy::from_json(&j).expect("decode");
    assert_eq!(back, p, "every member round-trips");
    assert_eq!(back.content_hash, p.content_id());
    // A present `content_hash` carries verbatim for the caller's seal check.
    let mut j2 = j.clone();
    if let Json::Obj(m) = &mut j2 {
        m.insert("content_hash".into(), Json::str("sha256:declared"));
    }
    let back2 = RoutingPolicy::from_json(&j2).expect("decode with hash");
    assert_eq!(back2.content_hash, "sha256:declared");
    // The optional members absent round-trip too (a changed document
    // re-addresses — `content_hash` is the address of the rest).
    let mut minimal = RoutingPolicy {
        allow_unknown: None,
        conditioned_rules: vec![],
        ext: BTreeMap::new(),
        ..p.clone()
    };
    minimal.content_hash = minimal.content_id();
    let back_min = RoutingPolicy::from_json(&minimal.to_json()).expect("decode minimal");
    assert_eq!(back_min, minimal);
}

/// `from_json` refuses malformed documents — a sealed policy never coerces.
#[test]
fn r2_7_routing_policy_from_json_refuses_malformed() {
    let j = full_policy().to_json();
    let drop_member = |k: &str| {
        let mut j = j.clone();
        if let Json::Obj(m) = &mut j {
            m.remove(k);
        }
        j
    };
    for k in ["policy_id", "version", "kind", "max_migration_loss"] {
        assert!(
            RoutingPolicy::from_json(&drop_member(k)).is_err(),
            "missing {k} refuses"
        );
    }
    let mut bad_kind = j.clone();
    if let Json::Obj(m) = &mut bad_kind {
        m.insert("kind".into(), Json::str("not_a_kind"));
    }
    assert!(RoutingPolicy::from_json(&bad_kind).is_err(), "unknown kind");
    let mut bad_action = j.clone();
    if let Json::Obj(m) = &mut bad_action {
        m.insert(
            "error_actions".into(),
            Json::obj([("overloaded", Json::str("retry_a_bit"))]),
        );
    }
    assert!(RoutingPolicy::from_json(&bad_action).is_err());
    // Not even an object.
    assert!(RoutingPolicy::from_json(&Json::Int(3)).is_err());
}

// ── CC1 — `RoutingDecision` ↔ `model.route.decided` payload ─────────────────

/// `events::route_decided`/`RoutingDecision::from_json` is the durable
/// codec pair — the driver's reroute fold replays a pending decision off
/// the emitted payload verbatim.
#[test]
fn r2_7_routing_decision_payload_round_trip() {
    let d = decision();
    let payload = events::route_decided(&d);
    let back = RoutingDecision::from_json(&payload).expect("decode");
    assert_eq!(back, d, "the payload is the record");
    // `model_call_id`/`reservation_id` nullables round-trip as Null.
    let d2 = RoutingDecision {
        model_call_id: None,
        reservation_id: None,
        deviation: true,
        relower_required: true,
        ..decision()
    };
    let back2 = RoutingDecision::from_json(&events::route_decided(&d2)).expect("decode nullables");
    assert_eq!(back2, d2);
}

/// A malformed payload member reads `None` — never a defaulted record.
#[test]
fn r2_7_routing_decision_from_json_refuses_malformed() {
    let payload = events::route_decided(&decision());
    for k in [
        "decision_id",
        "role",
        "selected",
        "policy_ref",
        "rule_ids_fired",
        "candidates_considered",
        "inputs_read",
        "deviation",
        "relower_required",
    ] {
        let mut j = payload.clone();
        if let Json::Obj(m) = &mut j {
            m.remove(k);
        }
        assert!(
            RoutingDecision::from_json(&j).is_none(),
            "missing {k} refuses"
        );
    }
    let mut bad = payload.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("deviation".into(), Json::str("yes"));
    }
    assert!(RoutingDecision::from_json(&bad).is_none(), "typed member");
}

// ── AC-R-2.3.2-4 — `check_reroute_order` (ADR-0122 d.3 sequence) ────────────

/// The happy cross-profile sequence: `attempt.failed` → (optional
/// `context.compaction.*` under the old profile) → `surface.relowered`
/// carrying the derivation members → `rerouted{relowered}` → a restarted
/// `attempt.started`.
#[test]
fn r2_7_reroute_order_accepts_cross_profile() {
    let mc = "mc-1";
    let compaction = (
        2u64,
        "context.compaction.completed".to_string(),
        Json::obj([("model_call_id", Json::str(mc))]),
    );
    let rows = vec![
        failed_row(1, mc),
        compaction,
        relower_row(3, mc),
        rerouted_row(4, mc, true),
        started_row(5, mc),
    ];
    router::check_reroute_order(&rows, mc).expect("the d.3 sequence orders");
    // Without the compaction leg the sequence still orders (it is optional).
    let rows2 = vec![
        failed_row(1, mc),
        relower_row(2, mc),
        rerouted_row(3, mc, true),
        started_row(4, mc),
    ];
    router::check_reroute_order(&rows2, mc).expect("no compaction leg");
    // Rows for *other* calls never satisfy this call's sequence.
    let mut rows3 = rows2.clone();
    rows3.push(failed_row(9, "mc-2"));
    router::check_reroute_order(&rows3, mc).expect("other-call rows ignored");
}

/// A same-profile reroute (`relowered = false`) carries no relower event.
#[test]
fn r2_7_reroute_order_accepts_same_profile() {
    let mc = "mc-1";
    let rows = vec![
        failed_row(1, mc),
        rerouted_row(2, mc, false),
        started_row(3, mc),
    ];
    router::check_reroute_order(&rows, mc).expect("same-profile sequence");
}

#[test]
fn r2_7_reroute_order_typed_violations() {
    let mc = "mc-1";
    // No preceding `attempt.failed`.
    let rows = vec![
        relower_row(1, mc),
        rerouted_row(2, mc, true),
        started_row(3, mc),
    ];
    assert_eq!(
        router::check_reroute_order(&rows, mc),
        Err(RerouteOrderError::NoFailedAttempt {
            model_call_id: mc.into()
        })
    );
    // Cross-profile without the relower row.
    let rows = vec![
        failed_row(1, mc),
        rerouted_row(2, mc, true),
        started_row(3, mc),
    ];
    assert_eq!(
        router::check_reroute_order(&rows, mc),
        Err(RerouteOrderError::MissingRelower {
            model_call_id: mc.into()
        })
    );
    // Same-profile with a relower row — a relower without a boundary
    // crossing is a lie.
    let rows = vec![
        failed_row(1, mc),
        relower_row(2, mc),
        rerouted_row(3, mc, false),
        started_row(4, mc),
    ];
    assert_eq!(
        router::check_reroute_order(&rows, mc),
        Err(RerouteOrderError::UnexpectedRelower {
            model_call_id: mc.into()
        })
    );
    // The relower row lacks the derivation members.
    let thin = (
        2u64,
        "model.surface.relowered".to_string(),
        Json::obj([("model_call_id", Json::str(mc))]),
    );
    let rows = vec![
        failed_row(1, mc),
        thin,
        rerouted_row(3, mc, true),
        started_row(4, mc),
    ];
    assert_eq!(
        router::check_reroute_order(&rows, mc),
        Err(RerouteOrderError::MissingDerivation {
            model_call_id: mc.into()
        })
    );
    // No restarted attempt after the reroute.
    let rows = vec![
        failed_row(1, mc),
        relower_row(2, mc),
        rerouted_row(3, mc, true),
    ];
    assert_eq!(
        router::check_reroute_order(&rows, mc),
        Err(RerouteOrderError::MissingRestart {
            model_call_id: mc.into()
        })
    );
}

// ── The honesty ceiling — live-dependent arms answer typed refusals ─────────

/// A `learned` (C4) policy with no pipeline evidence is a declared tier
/// refusal — `PolicyInvalid`, never a fall-through to the binding.
#[test]
fn r2_7_learned_policy_refuses_offline() {
    let env = Env(vec![profile(
        "prof.a",
        "1",
        VersionPattern::Exact("v1".into()),
    )]);
    let mut p = full_policy();
    p.kind = RoutingPolicyKind::Learned;
    p.params = Json::Null;
    p.conditioned_rules = vec![]; // no pipeline evidence
    let out = select(
        &request(),
        &env,
        &mut FakeBudget,
        &NoHealth,
        &p,
        &table(),
        "d-0",
    );
    assert!(
        matches!(out, Err(RoutingRefusal::PolicyInvalid { .. })),
        "learned unbound is PolicyInvalid, got {out:?}"
    );
}

/// A `cost_cap` policy reading a price with no pricing view is `NoPrice` —
/// the G-5 honesty rule, never a zero-cost fallback.
#[test]
fn r2_7_cost_cap_without_pricing_refuses() {
    let env = Env(vec![
        profile("prof.a", "1", VersionPattern::Exact("v1".into())),
        profile("prof.b", "1", VersionPattern::Exact("v2".into())),
    ]);
    let mut p = full_policy();
    p.kind = RoutingPolicyKind::CostCap;
    p.params = Json::obj([("max_cost_millis", Json::Int(1))]);
    let out = select(
        &request(),
        &env,
        &mut FakeBudget,
        &NoHealth,
        &p,
        &table(),
        "d-0",
    );
    match out {
        Err(RoutingRefusal::NoPrice { .. }) | Err(RoutingRefusal::ChainExhausted { .. }) => {}
        other => panic!("NoPrice or a chain of them, got {other:?}"),
    }
}

/// An end-to-end consult cell: `select` → staged `on_attempt_failed`
/// → `Reroute` across profiles — the fresh decision's
/// `relower_required`/`deviation` flags are set, and its payload
/// round-trips back through `RoutingDecision::from_json` (the driver's
/// fold would read it verbatim).
#[test]
fn r2_7_consult_produces_cross_profile_reroute() {
    let env = Env(vec![
        profile("prof.a", "1", VersionPattern::Exact("v1".into())),
        profile("prof.b", "1", VersionPattern::Exact("v2".into())),
    ]);
    let mut p = full_policy();
    p.kind = RoutingPolicyKind::FallbackChain;
    p.params = Json::Null; // absent targets binds the role's own row
    let mut state = AttemptState::default();
    let mut budget = FakeBudget;
    // prof.a → prof.b projects a zero-loss migration (G-3 satisfied).
    let migration = Migration(BTreeMap::from([(
        ("prof.a@1".to_string(), "prof.b@1".to_string()),
        0,
    )]));
    let d0 = select(
        &request(),
        &env,
        &mut budget,
        &NoHealth,
        &p,
        &table(),
        "d-0",
    )
    .expect("initial select");
    let out = on_attempt_failed(
        &request(),
        &d0,
        &ModelErrorClass::Auth, // `error_actions` ⇒ Reroute directly
        1,
        None,
        &mut state,
        &env,
        &mut budget,
        &NoHealth,
        &RoutingViews {
            migration: Some(&migration),
            ..RoutingViews::none()
        },
        &p,
        &table(),
        "d-1",
        0,
    );
    match out {
        AttemptDisposition::Reroute(d) => {
            assert_eq!(d.selected.provider_model_id, "m-2");
            assert!(d.relower_required, "prof.a → prof.b crosses profiles");
            assert!(d.deviation, "m-2 differs from the role's primary");
            // The decision is a ledger-faithful record — decodes verbatim.
            let back =
                RoutingDecision::from_json(&events::route_decided(&d)).expect("decision decodes");
            assert_eq!(back, d);
        }
        other => panic!("expected Reroute, got {other:?}"),
    }
}

// ── WireDialect fixture corpus codec leg (AC-R-2.3.1-2, R-2.3.1) ─────────────
//
// The 13-shape × 2-family golden stream corpus landed at S3.7
// (`golden_streams.rs::golden_corpus_thirteen_shapes_two_families`). This cell
// pins the ticket's codec AC in-battery: the provider-faithful dialect
// documents for both corpus families round-trip through the declared
// `WireDialect/1` codec verbatim — the document a lane decodes is the
// document that was sealed — and the strict codec refuses an unknown member
// rather than coercing it.

fn corpus_retention() -> Vec<RetentionClass> {
    vec![RetentionClass {
        class_id: "short".into(),
        nominal_ms: 60_000,
        guaranteed: false,
    }]
}

fn corpus_debt(what: &str) -> DebtRecord {
    DebtRecord {
        what_we_assume: what.to_string(),
        how_we_could_be_wrong: "the provider changes the spelling".into(),
        evidence: vec![DebtEvidence {
            reference: "fixture:family".into(),
            accessed: "2025-01-01".into(),
        }],
        detection_signal: "decode violation".into(),
        removal_test: "drop the rule; the corpus test fails".into(),
    }
}

/// Family A (`d.test@1`) — Anthropic-spelling faithful: `end_turn`/`tool_use`/
/// `max_tokens` stop reasons, `ExplicitBreakpoints` cache semantics, bearer
/// auth, `ping` metadata frames, `cancelled` error code.
fn corpus_dialect_a() -> WireDialect {
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
            retention_classes: corpus_retention(),
            min_cacheable_tokens: Some(1_024),
            isolation: IsolationScope::Workspace,
            eligible_carriers: vec![BlockKind::Text],
        },
        cache_state_visible: true,
        count_tokens: Some("count_tokens".into()),
        deferred_requests: false,
        model_listing: None,
        snapshot_id_exposed: Some("snapshot_id".into()),
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 30_000,
        prefix_affecting_params: vec!["system".into()],
        rules: vec![
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
            DialectRule::MetadataFrame {
                kind: "ping".into(),
                debt: corpus_debt("family-A interleaves `ping` frames"),
            },
            DialectRule::ErrorCodeToClass {
                code: "cancelled".into(),
                class: ModelErrorClass::Cancelled,
                debt: corpus_debt("family-A cancels with `cancelled`"),
            },
        ],
    }
}

/// Family B (`d.budget@1`) — OpenAI-spelling faithful: `stop`/`tool_calls`/
/// `length`/`content_filter` stop reasons, `ImplicitPrefix` cache semantics,
/// `meta` housekeeping frames, `request_cancelled` error code, `models`
/// listing.
fn corpus_dialect_b() -> WireDialect {
    let mut d = corpus_dialect_a();
    d.dialect_id = "d.budget".into();
    d.cache_semantics = CacheSemantics::ImplicitPrefix {
        strictness: ImplicitStrictness::Strict,
        affinity_key: AffinitySupport::Supported { max_key_length: 64 },
        retention_classes: corpus_retention(),
        min_cacheable_tokens: Some(1_024),
        reporting_granularity_tokens: Some(128),
    };
    d.cache_state_visible = false;
    d.model_listing = Some("models".into());
    d.rules = vec![
        DialectRule::StopReasonMap {
            provider: "stop".into(),
            reason: StopReason::EndTurn,
        },
        DialectRule::StopReasonMap {
            provider: "tool_calls".into(),
            reason: StopReason::ToolUse,
        },
        DialectRule::StopReasonMap {
            provider: "length".into(),
            reason: StopReason::MaxOutput,
        },
        DialectRule::StopReasonMap {
            provider: "content_filter".into(),
            reason: StopReason::ContentFilter,
        },
        DialectRule::ErrorCodeToClass {
            code: "request_cancelled".into(),
            class: ModelErrorClass::Cancelled,
            debt: corpus_debt("family-B cancels with `request_cancelled`"),
        },
        DialectRule::MetadataFrame {
            kind: "meta".into(),
            debt: corpus_debt("family-B interleaves `meta` housekeeping frames"),
        },
    ];
    d
}

#[test]
fn r2_7_dialect_documents_codec_round_trip() {
    for d in [corpus_dialect_a(), corpus_dialect_b()] {
        let j = d.to_json();
        let back = WireDialect::from_json(&j)
            .unwrap_or_else(|e| panic!("{} decodes: {e:?}", d.dialect_id));
        assert_eq!(back, d, "{} round-trips verbatim", d.dialect_id);
        // The content id is a pure function of the canonical body — a decoded
        // document re-seals to the same identity (no field is lost or
        // re-ordered on the codec path).
        assert_eq!(back.content_id(), d.content_id());
    }
    // Strict decode: an unknown member refuses rather than coercing.
    let mut bad = corpus_dialect_a().to_json();
    if let Json::Obj(m) = &mut bad {
        m.insert("surprise_member".into(), Json::str("x"));
    }
    assert!(WireDialect::from_json(&bad).is_err());
}
