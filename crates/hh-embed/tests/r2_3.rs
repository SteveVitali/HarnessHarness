//! R2.3 — the §5a.3 durable-execution boundary ops (DF-S2.3-1):
//!
//! - `suspend{reasons[], subscription_ids?, resume_policy?, release_lease?}` —
//!   the writer-only protocol entry point: S-1 checked at the ledger, each
//!   `awaiting_*` reason mints (or reuses) its producing subscription —
//!   `control.wakeup.scheduled` rows land *before* `lifecycle.run.suspended`
//!   (durable-before-visible), `release_lease` detaches the session, and an
//!   `attach` session is the `session_is_read_only` refusal.
//! - `compensate{after_seq?, outcomes?}` — the compensation saga's boundary
//!   arm: reverse-commit-order walk, host-declared outcomes, the honest
//!   `CompensatorMissing` → `abandoned` + `lifecycle.escalation.raised` leg.
//! - `heal{policy?, policy_ref?}` — the policy-bound environment heal:
//!   `environment_attached` refusal on a live handle, the
//!   `healed|ask|refused` outcome sum, `lost_items_ref` carried, the session
//!   rebound to the healed handle, and `healing_policy_ref` resolution
//!   through the one RegistryStore (unresolvable/malformed = typed refusal,
//!   never a substituted default).
//!
//! The legs the boundary cannot produce between calls (an open
//! `prepared`/`deferred`/`committed` effect, a `control.retry.scheduled`
//! row, a `deliver_after`-withheld fire, a registry-resident
//! `healing_policy` record) are covered by the in-crate unit battery in
//! `durability_ops.rs` (`mod r2_3_tests`), which drives the same handler
//! code over a direct store mint.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_hir::{SlotBinding, SlotBindings};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixture: the conformance document (mirrors tests/conformance.rs) ────────

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

fn document_json() -> Json {
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
                        hard: Some(1000),
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
                    slots: BTreeMap::new(),
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

// ── service plumbing (mirrors tests/conformance.rs) ─────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r23-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    service_in(&test_dir("svc"))
}

fn service_in(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r23".into(),
    })
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

fn hello_params(caps: Json) -> Json {
    Json::obj([
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj([
                ("name", Json::str("r23")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        ("capabilities", caps),
    ])
}

fn hello_experimental(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        hello_params(Json::obj([("experimental", Json::Bool(true))])),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn new_spec_with(environment: Json, supplies: Option<Json>) -> Json {
    let mut m = vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj([
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
        ),
        ("overrides", Json::Arr(vec![])),
        ("environment", environment),
        (
            "attendance",
            AttendanceDeclaration {
                value: "async".into(),
                source: "declared".into(),
            }
            .to_json(),
        ),
    ];
    if let Some(s) = supplies {
        m.push(("supplies", s));
    }
    Json::obj(m)
}

fn open_new(svc: &mut EmbedService) -> Json {
    let r = call(
        svc,
        "open_session",
        Json::obj([
            ("spec", new_spec_with(local_host_env(BTreeMap::new()), None)),
            ("idempotency_key", Json::str("open-1")),
        ]),
    );
    ok(&r)
}

fn local_host_env(info: BTreeMap<String, Json>) -> Json {
    let mut m = info;
    m.entry("class".to_string())
        .or_insert(Json::str("local_host"));
    Json::obj([
        ("kind", Json::str("connection_info")),
        ("connection_info", Json::Obj(m)),
    ])
}

fn session_id(s: &Json) -> String {
    s.get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

fn run_id(s: &Json) -> String {
    s.get("run_id").and_then(Json::as_str).unwrap().to_string()
}

/// The class-seq pairs of a run's durable prefix, for ordering asserts.
fn seqs_of(svc: &EmbedService, run: &str) -> Vec<(u64, String)> {
    svc.store()
        .envelopes(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone()))
        .collect()
}

fn seq_of(svc: &EmbedService, run: &str, class: &str) -> Option<u64> {
    seqs_of(svc, run)
        .iter()
        .find(|(_, c)| c == class)
        .map(|(s, _)| *s)
}

// ── `suspend` ───────────────────────────────────────────────────────────────

#[test]
fn r2_3_suspend_gates_and_typed_refusals() {
    // The experimental gate precedes everything.
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(Json::obj([("serves_measurement", Json::Bool(true))])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    for (m, p) in [
        (
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("reasons", Json::Arr(vec![])),
            ]),
        ),
        (
            "compensate",
            Json::obj([("session_id", Json::str(sid.clone()))]),
        ),
        ("heal", Json::obj([("session_id", Json::str(sid.clone()))])),
    ] {
        assert_eq!(
            err_kind(&call(&mut svc, m, p)),
            "ExperimentalRequired",
            "{m}"
        );
    }

    let mut svc = service();
    hello_experimental(&mut svc);
    for m in ["suspend", "compensate", "heal"] {
        let p = match m {
            "suspend" => Json::obj([
                ("session_id", Json::str("sess_missing")),
                ("reasons", Json::Arr(vec![])),
            ]),
            _ => Json::obj([("session_id", Json::str("sess_missing"))]),
        };
        assert_eq!(err_kind(&call(&mut svc, m, p)), "UnknownSession", "{m}");
    }
    // `suspend` — `reasons` is required and must decode.
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "suspend",
            Json::obj([("session_id", Json::str(sid.clone()))]),
        )),
        "SchemaViolation"
    );
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([("type", Json::str("bogus"))])]),
                ),
            ]),
        )),
        "SchemaViolation"
    );
    // `awaiting_event` / `subscription_ids` name live subscriptions —
    // a made-up id is the typed refusal, never recorded.
    for reasons in [
        Json::Arr(vec![Json::obj([
            ("type", Json::str("awaiting_event")),
            ("subscription_id", Json::str("sub_missing")),
        ])]),
        Json::Arr(vec![Json::obj([("type", Json::str("operator_pause"))])]),
    ] {
        let params = Json::obj([
            ("session_id", Json::str(sid.clone())),
            ("reasons", reasons.clone()),
            (
                "subscription_ids",
                Json::Arr(vec![Json::str("sub_missing")]),
            ),
        ]);
        assert_eq!(err_kind(&call(&mut svc, "suspend", params)), "Refused");
    }
    // `awaiting_effect` names a real effect fold — `eff_missing` refuses.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([
                        ("type", Json::str("awaiting_effect")),
                        ("effect_id", Json::str("eff_missing")),
                    ])]),
                ),
            ]),
        )),
        "Refused"
    );
    // An attach session is read-only by construction — the typed refusal
    // precedes any durable write.
    let run = run_id(&s);
    let a = ok(&call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([("kind", Json::str("attach")), ("run_id", Json::str(run))]),
            ),
            ("idempotency_key", Json::str("att-1")),
        ]),
    ));
    let attach = session_id(&a);
    for (m, p) in [
        (
            "suspend",
            Json::obj([
                ("session_id", Json::str(attach.clone())),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([("type", Json::str("operator_pause"))])]),
                ),
            ]),
        ),
        (
            "compensate",
            Json::obj([("session_id", Json::str(attach.clone()))]),
        ),
        (
            "heal",
            Json::obj([("session_id", Json::str(attach.clone()))]),
        ),
    ] {
        assert_eq!(err_kind(&call(&mut svc, m, p)), "Refused", "{m}");
    }
}

#[test]
fn r2_3_suspend_mints_producing_subscriptions_then_suspends() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    let run = run_id(&s);

    // A caller-named subscription — minted by `subscribe` — rides the
    // `subscription_ids` member verbatim.
    let sub = ok(&call(
        &mut svc,
        "subscribe",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "trigger",
                Json::obj([
                    ("type", Json::str("external")),
                    ("kind", Json::str("webhook")),
                    ("source_ref", Json::str("ing:ci")),
                ]),
            ),
        ]),
    ));
    let webhook_sub = sub
        .get("subscription_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    let out = ok(&call(
        &mut svc,
        "suspend",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "reasons",
                Json::Arr(vec![
                    Json::obj([
                        ("type", Json::str("awaiting_timer")),
                        ("at", Json::Int(9_999_999)),
                    ]),
                    Json::obj([
                        ("type", Json::str("awaiting_approval")),
                        ("permission_id", Json::str("perm-1")),
                    ]),
                    Json::obj([
                        ("type", Json::str("awaiting_event")),
                        ("subscription_id", Json::str(webhook_sub.clone())),
                    ]),
                    Json::obj([("type", Json::str("operator_pause"))]),
                ]),
            ),
            (
                "subscription_ids",
                Json::Arr(vec![Json::str(webhook_sub.clone())]),
            ),
        ]),
    ));
    assert_eq!(
        out.get("released_lease"),
        Some(&Json::Bool(true)),
        "{out:?}"
    );
    let subs: Vec<String> = out
        .get("subscription_ids")
        .and_then(|v| match v {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap();
    // webhook + timer + permission_decided producer legs.
    assert_eq!(subs.len(), 3, "{out:?}");
    assert!(subs.contains(&webhook_sub), "{subs:?}");

    // Durable-before-visible: every `control.wakeup.scheduled` producer
    // row lands *before* `lifecycle.run.suspended`; the lease release is
    // the tail (KP-10's resume-completable order).
    let suspended_seq = seq_of(&svc, &run, "lifecycle.run.suspended").expect("suspended row");
    let sched_seqs: Vec<u64> = seqs_of(&svc, &run)
        .iter()
        .filter(|(_, c)| c == "control.wakeup.scheduled")
        .map(|(s, _)| *s)
        .collect();
    assert!(sched_seqs.len() >= 2, "{sched_seqs:?}");
    assert!(
        sched_seqs.iter().all(|s| *s < suspended_seq),
        "{sched_seqs:?} vs {suspended_seq}"
    );
    let released_seq = seq_of(&svc, &run, "lifecycle.lease.released").expect("released row");
    assert!(
        released_seq > suspended_seq,
        "{released_seq} vs {suspended_seq}"
    );

    // The suspended row records the reasons + the live subscriptions.
    let suspended = svc
        .store()
        .envelopes(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "lifecycle.run.suspended")
        .unwrap();
    let reasons = suspended
        .payload
        .get("reasons")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.len()),
            _ => None,
        })
        .unwrap_or(0);
    assert_eq!(reasons, 4, "{:?}", suspended.payload);

    // The released lease detaches the session — a later writer verb is
    // the typed refusal, and a second `suspend` never replays.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([("type", Json::str("hibernated"))])]),
                ),
            ]),
        )),
        "SessionDetached"
    );

    // `resume{continue}` — the released writer lease admits the new
    // session; the durable resume mints `lifecycle.run.resumed`.
    let s2 = ok(&call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run.clone())),
                    ("mode", Json::str("continue")),
                ]),
            ),
            ("idempotency_key", Json::str("res-1")),
        ]),
    ));
    assert_eq!(s2.get("run_id").and_then(Json::as_str), Some(run.as_str()));
    assert!(seq_of(&svc, &run, "lifecycle.run.resumed").is_some());
}

#[test]
fn r2_3_suspend_release_lease_false_keeps_the_session() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    let run = run_id(&s);
    let out = ok(&call(
        &mut svc,
        "suspend",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "reasons",
                Json::Arr(vec![Json::obj([("type", Json::str("operator_pause"))])]),
            ),
            ("release_lease", Json::Bool(false)),
        ]),
    ));
    assert_eq!(out.get("released_lease"), Some(&Json::Bool(false)));
    assert!(seq_of(&svc, &run, "lifecycle.run.suspended").is_some());
    assert!(seq_of(&svc, &run, "lifecycle.lease.released").is_none());
    // The session is still the writer — a held lease keeps `head` live.
    let r = call(
        &mut svc,
        "head",
        Json::obj([("session_id", Json::str(sid))]),
    );
    assert!(r.get("result").is_some(), "head on held lease: {r:?}");
}

// ── `compensate` ────────────────────────────────────────────────────────────

#[test]
fn r2_3_compensate_boundary_surface() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    // A run with no applied+compensable effects compensates nothing —
    // the empty report is honest, never fabricated.
    let out = ok(&call(
        &mut svc,
        "compensate",
        Json::obj([("session_id", Json::str(sid.clone()))]),
    ));
    for k in ["compensated", "abandoned", "in_flight", "escalations"] {
        assert_eq!(
            out.get(k),
            Some(&Json::Arr(vec![])),
            "{k} should be empty: {out:?}"
        );
    }
    // `outcomes` must be an object — a stray shape is a schema error.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "compensate",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("outcomes", Json::str("nope")),
            ]),
        )),
        "SchemaViolation"
    );
}

// ── `heal` ──────────────────────────────────────────────────────────────────

#[test]
fn r2_3_heal_outcome_sum_and_policy_members() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = open_new(&mut svc);
    let sid = session_id(&s);
    let run = run_id(&s);

    // `policy{on_lost:"fail"}` — the policy's own refusal is the honest
    // `refused` outcome (a result member, not a boundary error); the
    // environment rows still land durable first.
    let out = ok(&call(
        &mut svc,
        "heal",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "policy",
                Json::obj([("on_lost", Json::str("fail")), ("max_heals", Json::Int(3))]),
            ),
        ]),
    ));
    match out.get("outcome").and_then(Json::as_str) {
        // A live handle refuses before the ladder runs.
        Some("environment_attached") | Some("attached") => {}
        Some("refused") => {
            assert_eq!(
                out.get("reason").and_then(Json::as_str),
                Some("healing_policy.on_lost = fail"),
                "{out:?}"
            );
        }
        other => panic!("unexpected heal outcome: {other:?} ({out:?})"),
    }

    // `policy{on_lost:"ask"}` — the durable permission ask; the
    // `security.permission.pending` row is the owed-decision record.
    let out = ok(&call(
        &mut svc,
        "heal",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "policy",
                Json::obj([("on_lost", Json::str("ask")), ("max_heals", Json::Int(3))]),
            ),
        ]),
    ));
    match out.get("outcome").and_then(Json::as_str) {
        Some("environment_attached") | Some("attached") => {}
        Some("ask") => {
            assert!(out.get("permission_id").and_then(Json::as_str).is_some());
            assert!(svc
                .store()
                .envelopes(&run)
                .unwrap()
                .iter()
                .any(|e| e.class == "security.permission.pending"));
        }
        other => panic!("unexpected heal outcome: {other:?} ({out:?})"),
    }

    // An unresolvable declared ref is the typed refusal — a default is
    // never substituted over a bad ref.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "heal",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("policy_ref", Json::str("hh/missing:never-registered")),
            ]),
        )),
        "Refused"
    );
    // A malformed inline policy is a schema error, never a default.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "heal",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("policy", Json::obj([("on_lost", Json::str("explode"))]),),
            ]),
        )),
        "SchemaViolation"
    );
    // The verdict row is durable either way.
    assert!(seq_of(&svc, &run, "action.environment.verified").is_some());
}

#[test]
fn r2_3_heal_lost_workspace_reprovisions_and_rebinds() {
    let root = test_dir("healws");
    let mut svc = service_in(&root);
    hello_experimental(&mut svc);
    // `on_loss = replace_from_image` — the declared loss posture the
    // `replace` rung requires (the Stage-1 `fail_run` default refuses,
    // per spec: a fail_run environment fails rather than heals).
    let env = local_host_env(BTreeMap::from([(
        "on_loss".to_string(),
        Json::str("replace_from_image"),
    )]));
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj([
            ("spec", new_spec_with(env, None)),
            ("idempotency_key", Json::str("open-heal")),
        ]),
    ));
    let sid = session_id(&s);
    let run = run_id(&s);
    // Force `lost{workspace_missing}` — the declared workspace root is
    // gone; a reprovisioning heal walks `mark_unreachable → replace →
    // healed`, carrying `lost_items_ref` (never silent loss).
    let ws = root.join("ws");
    if ws.exists() {
        std::fs::remove_dir_all(&ws).unwrap();
    }
    let out = ok(&call(
        &mut svc,
        "heal",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "policy",
                Json::obj([
                    ("on_lost", Json::str("reprovision")),
                    ("max_heals", Json::Int(3)),
                ]),
            ),
        ]),
    ));
    assert_eq!(
        out.get("outcome").and_then(Json::as_str),
        Some("healed"),
        "{out:?}"
    );
    assert!(out.get("restored").and_then(Json::as_str).is_some());
    assert!(out.get("lost_items_ref").and_then(Json::as_str).is_some());
    assert_eq!(out.get("heal_no"), Some(&Json::Int(1)), "{out:?}");
    // The transition rows are durable: verified → unreachable → healed,
    // in that order (durable-before-visible).
    let verified_seq = seq_of(&svc, &run, "action.environment.verified").expect("verified");
    let unreachable_seq =
        seq_of(&svc, &run, "action.environment.unreachable").expect("unreachable");
    let healed_seq = seq_of(&svc, &run, "action.environment.healed").expect("healed");
    assert!(verified_seq < unreachable_seq && unreachable_seq < healed_seq);
    let healed = svc
        .store()
        .envelopes(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.healed")
        .unwrap();
    assert!(healed
        .payload
        .get("lost_items_ref")
        .and_then(Json::as_str)
        .is_some());
    // The session rebinds to the healed handle — the result carries no
    // handle id (R-NOSIDE), but a second `heal` walks the *successor*:
    // it lands `provisioning` (the `replace` rung spawns it; the resume
    // path's attach leg owns `ready`) and `replace` on `provisioning`
    // is the typed `InvalidState` — a stale `env_handle_id` would read
    // `replaced` instead, so the state's name is the proof.
    let second = call(
        &mut svc,
        "heal",
        Json::obj([("session_id", Json::str(sid))]),
    );
    assert_eq!(err_kind(&second), "EnvironmentUnavailable", "{second:?}");
    assert!(
        second
            .get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str)
            .is_some_and(|r| r.contains("provisioning")),
        "{second:?}"
    );
}
