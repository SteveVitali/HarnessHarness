//! R2.7 — the `router` slot's machine half at the `hh-embed/1` boundary
//! (DF-S1.18-1; R-2.3.1–R-2.3.4; ADR-0121 d.4/d.5, ADR-0122 d.3,
//! ADR-0128 d.3).
//!
//! A `native.slots["router"]` binding carries the sealed declaration —
//! `policy` (a `RoutingPolicy` document), `lane` overrides,
//! `response_cache` (the K5 store leg), `probes[]` and `model_fail[]`.
//! Arming it wires the driver's `RoutingPort`/`ResponseCachePort`: the
//! ledger gains `model.route.decided`, `model.call.attempt.*`,
//! `model.cache.resolved`, `model.profile.probed`, and the consult rows —
//! durable-before-visible under the writer lease (CC3). No slot ⇒ the
//! legacy scripted lane stays byte-identical.
//!
//! Fixture-only (the ticket's offline ceiling): the scripted kernel model,
//! `local_host` env, an in-process registry profile — no live provider
//! claims anywhere (BL-31 stays open for the transport legs).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixture: the conformance document + a bound `router` slot ───────────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

/// The registered `ModelProfile` — the scripted candidate's coordinate
/// `hh-embed/kernel-scripted@1` resolves to it (the selector's exact
/// match; `max_output` declared so the reservation sizes honestly).
fn scripted_profile() -> hh_compiler::profile::ModelProfile {
    use hh_compiler::profile::*;
    let mut p = ModelProfile {
        profile_id: "prof.r27".into(),
        version: "1".into(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "hh-embed".into(),
            model_family: "kernel-scripted".into(),
            version_pattern: VersionPattern::Exact("1".into()),
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: ProfileCapabilities {
            native_function_calling: CapabilityState::Declared,
            max_output: Some(8_192),
            ..ProfileCapabilities::default()
        },
        rules: vec![],
        ext: BTreeMap::new(),
        expiry: ProfileDebtRecord {
            rule_id: "prof.r27.expiry".into(),
            hypothesis: "the scripted kernel model honours the contract".into(),
            evidence_refs: vec![EvidenceRef::legacy("sha256:ev")],
            owner: "test:owner".into(),
            reach_via: vec![],
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::ModelVersionChange,
                value: None,
            },
            removal_test_ref: "t:r27".into(),
            removal_test: Some(RemovalTest::new(RemovalTestKind::Documentation)),
            status: DebtStatus::Active,
            debt_class: None,
            hypothesis_typed: None,
            scope: None,
            expiry: None,
            runway_ms: None,
            revalidation: None,
            created_at: None,
            supersedes: None,
        },
        compatibility: ProfileCompatibility {
            inventory_version: "1".into(),
            min_compiler_version: "0.0.0".into(),
        },
        tests: Json::Null,
    };
    p.content_hash = hh_compiler::profile::profile_identity(&p);
    p
}

/// The `RoutingPolicy` document the slot seals — `static` over the
/// scripted candidate; `error_actions` is the default table (the consult
/// reads it — `server_error → retry_same{2} → reroute`,
/// `auth → reroute`).
fn policy_doc() -> Json {
    use hh_gateway::router::*;
    let policy = RoutingPolicy {
        policy_id: "policy.r27".into(),
        version: "1".into(),
        content_hash: String::new(),
        role_scope: std::collections::BTreeSet::new(),
        kind: RoutingPolicyKind::Static,
        params: Json::obj([(
            "target",
            Json::obj([
                (
                    "candidate",
                    Json::obj([
                        ("profile_ref", Json::str("prof.r27@1")),
                        ("provider_model_id", Json::str("hh-embed/kernel-scripted")),
                    ]),
                ),
                (
                    "coordinate",
                    Json::obj([
                        ("provider_api_family", Json::str("hh-embed")),
                        ("model_family", Json::str("kernel-scripted")),
                        ("model_version", Json::str("1")),
                    ]),
                ),
            ]),
        )]),
        error_actions: RoutingPolicy::default_error_actions(),
        max_migration_loss: MigrationLossBound {
            dropped_items: 0,
            no_in_flight_tool_call: true,
        },
        allow_unknown: None,
        conditioned_rules: vec![],
        ext: BTreeMap::new(),
    };
    // The slot spelling — `version` → `policy_version` (T-10), the
    // target's `model_ref` → `candidate` (T-LCD-01).
    let mut j = policy.to_json();
    if let Json::Obj(m) = &mut j {
        if let Some(v) = m.remove("version") {
            m.insert("policy_version".into(), v);
        }
    }
    j
}

/// The `slots["router"].params` record (the `router_arm_from_params`
/// grammar): `policy` + the optional `lane`/`response_cache`/`probes`/
/// `model_fail` members.
fn router_params(extra: &[(&str, Json)]) -> BTreeMap<String, Json> {
    let mut p = BTreeMap::from([("policy".to_string(), policy_doc())]);
    for (k, v) in extra {
        p.insert((*k).to_string(), v.clone());
    }
    p
}

/// The conformance document with `native.slots["router"]` bound —
/// `extra` members ride the slot params.
fn document_json_routed(extra: &[(&str, Json)]) -> Json {
    document_json_slots(Some(extra))
}

/// `None` ⇒ the legacy scripted lane — `native.slots` empty.
fn document_json_slots(extra: Option<&[(&str, Json)]>) -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1_000),
                        soft: None,
                    },
                )]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let mut slots = BTreeMap::new();
    if let Some(extra) = extra {
        slots.insert(
            "router".to_string(),
            SlotBindings::One(SlotBinding {
                variant: ComponentVariantRef::selected(
                    "routing_policy",
                    "hh/router_static",
                    "latest",
                ),
                params: router_params(extra),
                enabled: true,
                locality: None,
            }),
        );
    }
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

// ── service helpers (the conformance battery's shape) ───────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r27-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service_in(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r2.7".into(),
    })
    .unwrap()
}

/// `service_in` over a `ManualClock` — the restart leg steps the clock
/// past the writer-lease TTL so the takeover fences the dead process's
/// record by *expiry* (ADR-0130; the S5.8/R2.6 restart discipline).
fn service_at(root: &std::path::Path, now_ms: u64) -> EmbedService {
    EmbedService::open_with(
        ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "r2.7".into(),
        },
        Box::new(hh_ledger::ids::ManualClock::at(now_ms)),
        None,
    )
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r2.7")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                    ("serves_host_executor", Json::Bool(true)),
                    ("serves_permission_channel", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    ok(&r);
}

/// The park-mid-turn shape — `interactive` attendance + a
/// `model_calls: 1` ceiling: the first call lands its rows, the next
/// exhausts the envelope and the escalation parks the loop (the R2.6
/// restart fixture's discipline).
fn parked_spec_for(doc: Json) -> Json {
    let mut spec = spec_for(doc);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "attendance".to_string(),
            AttendanceDeclaration {
                value: "interactive".into(),
                source: "declared".into(),
            }
            .to_json(),
        );
        m.insert(
            "budget".to_string(),
            Json::obj(vec![
                ("kind", Json::str("node")),
                (
                    "node",
                    Json::obj(vec![(
                        "dimensions",
                        Json::obj(vec![(
                            "model_calls",
                            Json::obj(vec![("hard", Json::Int(1))]),
                        )]),
                    )]),
                ),
            ]),
        );
    }
    spec
}

fn spec_for(doc: Json) -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![("kind", Json::str("document")), ("document", doc)]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj(vec![("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "attendance",
            AttendanceDeclaration {
                value: "async".into(),
                source: "declared".into(),
            }
            .to_json(),
        ),
    ])
}

fn registrar() -> Json {
    ProvenanceRecord::kernel("hh-embed", 0).to_json()
}

/// Register the scripted-lane profile so `resolve_profile` binds it (the
/// router's `SelectorView` reads the registry's `registered()` snapshot —
/// CF-046's snapshot-confined read).
fn register_profile(svc: &mut EmbedService) {
    let r = call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("model_profile")),
            (
                "body",
                Json::obj([
                    ("kind", Json::str("model_profile")),
                    (
                        "profile",
                        hh_compiler::schema::profile_to_json(&scripted_profile()),
                    ),
                ]),
            ),
            ("registrar", registrar()),
        ]),
    );
    ok(&r);
}

fn open_routed(svc: &mut EmbedService, doc: Json, key: &str) -> (String, String) {
    let s = ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc)),
            ("idempotency_key", Json::str(key)),
        ]),
    ));
    (
        s.get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        s.get("run_id").and_then(Json::as_str).unwrap().to_string(),
    )
}

fn submit(svc: &mut EmbedService, session: &str, input: Vec<Json>, key: &str) -> Json {
    call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            ("input", Json::Arr(input)),
            ("idempotency_key", Json::str(key)),
        ]),
    )
}

fn text_input(t: &str) -> Vec<Json> {
    vec![Json::obj(vec![
        ("kind", Json::str("text")),
        ("text", Json::str(t)),
    ])]
}

fn classes(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.store()
        .envelopes(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

fn seq_of(evs: &[(u64, String, Json)], class: &str, mc: &str) -> Vec<u64> {
    evs.iter()
        .filter(|(_, c, p)| c == class && p.get("model_call_id").and_then(Json::as_str) == Some(mc))
        .map(|(s, _, _)| *s)
        .collect()
}

// ── the cells ───────────────────────────────────────────────────────────────

/// The routed lane emits the model-plane set: `model.profile.probed` +
/// `model.profile.status.changed` mint durable at arm (before the first
/// drive — CC3), `model.route.decided` lands ahead of the
/// `model.call.requested` it binds (durable-before-visible), the K5 lookup
/// writes `model.cache.resolved`, and the attempt pair brackets the call.
#[test]
fn r2_7_routed_lane_emits_model_plane_rows() {
    let root = test_dir("routed");
    let mut svc = service_in(&root);
    hello(&mut svc);
    register_profile(&mut svc);
    let doc = document_json_routed(&[
        (
            "response_cache",
            Json::obj([
                ("provider_model_id", Json::str("hh-embed/kernel-scripted")),
                ("profile_version_id", Json::str("prof.r27@1")),
            ]),
        ),
        (
            "probes",
            Json::Arr(vec![Json::obj([
                ("profile_ref", Json::str("prof.r27@1")),
                ("probe_run_id", Json::str("probe-0")),
                (
                    "records",
                    Json::Arr(vec![Json::obj([
                        ("name", Json::str("capability_probe")),
                        ("status", Json::str("degraded")),
                    ])]),
                ),
            ])]),
        ),
    ]);
    let (sess, run) = open_routed(&mut svc, doc, "open-r27-1");
    let r = submit(&mut svc, &sess, text_input("route me"), "submit-1");
    assert!(
        r.get("result").is_some(),
        "submit: {}",
        r.to_canonical_string()
    );
    let evs = classes(&svc, &run);

    // The probe rows minted at arm — durable before any model call.
    let probed = evs
        .iter()
        .find(|(_, c, _)| c == "model.profile.probed")
        .map(|(s, _, p)| (*s, p.clone()))
        .unwrap_or_else(|| panic!("no model.profile.probed: {evs:?}"));
    assert_eq!(
        probed.1.get("profile_ref").and_then(Json::as_str),
        Some("prof.r27@1")
    );
    // The probe's declared `status` contradicts the profile's `active` —
    // the arm mints `model.profile.status.changed{trigger: probe}`.
    let changed = evs
        .iter()
        .find(|(_, c, _)| c == "model.profile.status.changed")
        .map(|(_, _, p)| p.clone())
        .expect("probe-status contradiction mints status.changed");
    assert_eq!(changed.get("to").and_then(Json::as_str), Some("degraded"));
    assert_eq!(changed.get("trigger").and_then(Json::as_str), Some("probe"));

    // `model.route.decided` before the `model.call.requested` it binds —
    // and the request carries the bound `model_ref`/`route_decision_ref`.
    let decided = evs
        .iter()
        .find(|(_, c, _)| c == "model.route.decided")
        .map(|(s, _, p)| (*s, p.clone()))
        .expect("model.route.decided");
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .map(|(s, _, p)| (*s, p.clone()))
        .expect("model.call.requested");
    assert!(
        decided.0 < requested.0,
        "the decision lands durable before the call it binds"
    );
    assert_eq!(
        requested.1.get("route_decision_ref").and_then(Json::as_str),
        decided.1.get("decision_id").and_then(Json::as_str),
        "the request names the decision it runs under"
    );
    assert_eq!(
        requested
            .1
            .get("model_ref")
            .and_then(|m| m.get("provider_model_id"))
            .and_then(Json::as_str),
        Some("hh-embed/kernel-scripted")
    );

    // The K5 lookup — exactly one `model.cache.resolved` per lookup; the
    // first call's is a `miss` (the store was empty).
    let mc = requested
        .1
        .get("model_call_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let resolved = seq_of(&evs, "model.cache.resolved", &mc);
    assert_eq!(resolved.len(), 1, "one resolved row per lookup");
    let outcome = evs
        .iter()
        .find(|(_, c, _)| c == "model.cache.resolved")
        .and_then(|(_, _, p)| p.get("outcome"))
        .and_then(Json::as_str);
    assert_eq!(outcome, Some("miss"));

    // The attempt pair brackets the call.
    assert_eq!(seq_of(&evs, "model.call.attempt.started", &mc).len(), 1);
    assert_eq!(seq_of(&evs, "model.call.attempt.completed", &mc).len(), 1);
    assert!(evs.iter().any(|(_, c, _)| c == "model.call.completed"));
}

/// The unbound lane is byte-identical — no model-plane rows land when the
/// `router` slot is absent.
#[test]
fn r2_7_unbound_lane_is_byte_identical() {
    let root = test_dir("unbound");
    let mut svc = service_in(&root);
    hello(&mut svc);
    // The plain conformance document — no `router` slot.
    let doc = document_json_slots(None);
    let (sess, run) = open_routed(&mut svc, doc, "open-r27-0");
    submit(&mut svc, &sess, text_input("plain"), "submit-0");
    let evs = classes(&svc, &run);
    for class in [
        "model.route.decided",
        "model.cache.resolved",
        "model.call.attempt.started",
        "model.call.attempt.completed",
        "model.call.attempt.failed",
        "model.rerouted",
        "model.surface.relowered",
        "model.profile.probed",
    ] {
        assert!(
            !evs.iter().any(|(_, c, _)| c == class),
            "unbound lane must not emit {class}"
        );
    }
    assert!(evs.iter().any(|(_, c, _)| c == "model.call.requested"));
    assert!(evs.iter().any(|(_, c, _)| c == "model.call.completed"));
}

/// A staged `server_error` consults `retry_same{max:2}` →
/// `attempt.failed{will_retry:true}` → `control.retry.scheduled` → a
/// restarted `attempt.started` → the pass completes the call. The
/// durable-fold cursor (`attempt_state`, `route_decisions`) is what the
/// consult reads — never session memory.
#[test]
fn r2_7_staged_failure_retries_same_target() {
    let root = test_dir("retry");
    let mut svc = service_in(&root);
    hello(&mut svc);
    register_profile(&mut svc);
    let doc = document_json_routed(&[(
        "model_fail",
        Json::Arr(vec![
            Json::obj([("class", Json::str("server_error"))]),
            Json::obj([("pass", Json::Bool(true))]),
        ]),
    )]);
    let (sess, run) = open_routed(&mut svc, doc, "open-r27-2");
    let r = submit(&mut svc, &sess, text_input("fail once"), "submit-1");
    assert!(
        r.get("result").is_some(),
        "submit: {}",
        r.to_canonical_string()
    );
    let evs = classes(&svc, &run);
    let mc = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .and_then(|(_, _, p)| p.get("model_call_id"))
        .and_then(Json::as_str)
        .expect("a call ran")
        .to_string();

    let failed = seq_of(&evs, "model.call.attempt.failed", &mc);
    assert_eq!(failed.len(), 1, "one failed attempt");
    let failed_payload = &evs
        .iter()
        .find(|(_, c, _)| c == "model.call.attempt.failed")
        .unwrap()
        .2;
    assert_eq!(
        failed_payload.get("will_retry"),
        Some(&Json::Bool(true)),
        "retry_same answers will_retry"
    );
    assert_eq!(
        failed_payload
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("server_error")
    );
    // `model.call.failed` records the failed attempt (the scope closes),
    // then the envelope schedules the retry and the second attempt runs.
    let scheduled = evs
        .iter()
        .find(|(_, c, p)| {
            c == "control.retry.scheduled"
                && p.get("scope_id").and_then(Json::as_str) == Some(mc.as_str())
        })
        .map(|(s, _, _)| *s)
        .expect("control.retry.scheduled for the call");
    assert!(failed[0] < scheduled, "failed before scheduled");
    let started = seq_of(&evs, "model.call.attempt.started", &mc);
    assert_eq!(started.len(), 2, "the retry restarts the attempt span");
    assert!(started[1] > scheduled, "the retry drives the restart");
    assert_eq!(seq_of(&evs, "model.call.attempt.completed", &mc).len(), 1);
    // No reroute — the consult's `Continue` keeps the same target.
    assert!(!evs.iter().any(|(_, c, _)| c == "model.rerouted"));
}

/// A staged `auth` failure is `reroute` per the default error table; the
/// scripted binding has no alternates, so the chain exhausts — `GiveUp`
/// mints the refused audit row and the call closes failed. Typed, never
/// silent.
#[test]
fn r2_7_staged_failure_gives_up_when_chain_exhausted() {
    let root = test_dir("giveup");
    let mut svc = service_in(&root);
    hello(&mut svc);
    register_profile(&mut svc);
    let doc = document_json_routed(&[(
        "model_fail",
        Json::Arr(vec![Json::obj([("class", Json::str("auth"))])]),
    )]);
    let (sess, run) = open_routed(&mut svc, doc, "open-r27-3");
    let r = submit(&mut svc, &sess, text_input("fail hard"), "submit-1");
    assert!(
        r.get("result").is_some(),
        "submit: {}",
        r.to_canonical_string()
    );
    let evs = classes(&svc, &run);
    let mc = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .and_then(|(_, _, p)| p.get("model_call_id"))
        .and_then(Json::as_str)
        .expect("a call ran")
        .to_string();
    let failed = &evs
        .iter()
        .find(|(_, c, _)| c == "model.call.attempt.failed")
        .unwrap()
        .2;
    assert_eq!(failed.get("will_retry"), Some(&Json::Bool(false)));
    assert!(evs.iter().any(|(_, c, p)| c == "model.call.failed"
        && p.get("model_call_id").and_then(Json::as_str) == Some(mc.as_str())));
    // The refused audit row — `route_attempt`'s guard fired.
    let guard = evs
        .iter()
        .find(|(_, c, p)| {
            c == "control.guard.fired"
                && p.get("model_call_id").and_then(Json::as_str) == Some(mc.as_str())
        })
        .map(|(_, _, p)| p.clone())
        .expect("guard.fired audit row");
    let required = guard.get("required").and_then(Json::as_str).unwrap_or("");
    assert!(
        ["chain_exhausted", "select_refused"].contains(&required),
        "the typed give-up reason: {required}"
    );
    // No restarted attempt — the terminal is the honest close.
    assert_eq!(seq_of(&evs, "model.call.attempt.started", &mc).len(), 1);
}

/// A malformed `router` slot (a `policy` member that isn't a sealed
/// `RoutingPolicy` document) refuses the open — `Refused`, never a
/// defaulted lane.
#[test]
fn r2_7_malformed_router_slot_refuses_open() {
    let root = test_dir("malformed");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let doc = document_json_routed(&[("policy", Json::obj([("kind", Json::str("bogus"))]))]);
    // `router_params` puts the fixture policy first — a test-supplied
    // `policy` member overwrites it in the BTreeMap insert order (same
    // key → the extra wins).
    let r = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec_for(doc)),
            ("idempotency_key", Json::str("open-bad")),
        ]),
    );
    assert_eq!(
        err_kind(&r),
        "Refused",
        "a malformed sealed policy refuses the open: {}",
        r.to_canonical_string()
    );
}

/// Resume-by-leaf preserves the lane: the routing/cache ports re-arm off
/// the manifest's `profile_binding` + the sealed document's `router` slot
/// (never carried process state) — a `takeover` resume routes the next
/// call through the same machine rows.
#[test]
fn r2_7_resume_re_arms_the_lane() {
    let root = test_dir("resume");
    let run;
    {
        let mut svc = service_at(&root, 1_000);
        hello(&mut svc);
        register_profile(&mut svc);
        // Park the run mid-turn: the invoke burns `model_calls = 1`, the
        // escalation parks the loop at a decision point.
        let doc = document_json_routed(&[]);
        let s = ok(&call(
            &mut svc,
            "open_session",
            Json::obj(vec![
                ("spec", parked_spec_for(doc)),
                ("idempotency_key", Json::str("open-r27-4")),
            ]),
        ));
        let sess = s
            .get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
        let r = submit(
            &mut svc,
            &sess,
            vec![Json::obj(vec![
                ("kind", Json::str("invoke")),
                ("capability", Json::str("host.exec.shell")),
            ])],
            "submit-1",
        );
        assert!(
            r.get("result").is_some(),
            "parked submit: {}",
            r.to_canonical_string()
        );
        let evs = classes(&svc, &run);
        assert!(
            evs.iter().any(|(_, c, _)| c == "model.route.decided"),
            "the routed lane armed at open"
        );
        // `svc` drops — every session field is gone; the WAL is the record.
    }

    // The restarted service — same store, clock stepped past the writer
    // TTL; `takeover` fences the dead writer's lease by expiry.
    let mut svc = service_at(&root, 1_000 + 120_001);
    hello(&mut svc);
    let s2 = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(&run)),
                    ("mode", Json::str("takeover")),
                    ("cause", Json::str("wakeup")),
                ]),
            ),
            ("idempotency_key", Json::str("open-r27-5")),
        ]),
    );
    if s2.get("result").is_none() {
        eprintln!("resume refused: {}", s2.to_canonical_string());
        for (seq, c, _) in classes(&svc, &run) {
            eprintln!("{seq} {c}");
        }
        panic!("resume failed");
    }
    let sess2 = ok(&s2)
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // `amend{budget}` re-solvates the parked loop and drives it — the
    // resumed driver's routing port was re-armed from the durable record.
    let r = call(
        &mut svc,
        "amend",
        Json::obj(vec![
            ("session_id", Json::str(sess2)),
            ("target", Json::str("budget")),
            (
                "value",
                Json::obj(vec![(
                    "dimensions",
                    Json::obj(vec![("model_calls", Json::Int(8))]),
                )]),
            ),
            ("idempotency_key", Json::str("amend-1")),
        ]),
    );
    assert!(
        r.get("result").is_some(),
        "post-resume amend: {}",
        r.to_canonical_string()
    );
    let evs = classes(&svc, &run);
    let decisions: Vec<&Json> = evs
        .iter()
        .filter(|(_, c, _)| c == "model.route.decided")
        .map(|(_, _, p)| p)
        .collect();
    assert!(
        decisions.len() >= 2,
        "the resumed lane routes its own calls: {decisions:?}"
    );
    assert!(
        evs.iter()
            .any(|(_, c, _)| c == "model.call.attempt.started"),
        "attempt rows continue after resume"
    );
}
