//! R3.2 — the DF-S2.3-1 residual's boundary legs (BL-44):
//!
//! - **R-2.12.6 non-fleet ingress** — `connection_info{ingress_adapters}`
//!   is stamped verbatim into `manifest.extra.ingress_adapters` at
//!   `open_session` (a malformed declaration is a schema error at open,
//!   never a silent non-declaration); `subscribe{external}`/`{manual}`
//!   on `run_kind != fleet` manifests admits only where the declaration
//!   covers it — the undeclared leg is the typed refusal — and the
//!   admitted subscription's `record_occurrence` lands the durable
//!   `control.wakeup.occurred`/`fired` rows under the run's writer lease
//!   (CC3, durable-before-visible).
//! - **R-2.4.3 `resume_set`** — `continue_goal` with no
//!   `carried.resume_set_heads` derives the set from the kernel's
//!   layer-S fold of the durable `context.memory.written` prefix — the
//!   continuation manifest records what the ledger proves, never what
//!   the caller claims (an explicit set, `[]` included, rides verbatim).
//!
//! The producer legs that need no boundary (the `resume_set` view fold,
//! the `wakeup_subscribe` admission matrix) live in
//! `hh-ledger/tests/r3_2.rs`; the env-handle heal surface lives in
//! `hh-env/tests/r3_2.rs`; the `observe_run` adapter seam lives in
//! `hh-fleet/tests/r3_2.rs`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hharness_assembly::grammar::Assembly;
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_hir::{SlotBinding, SlotBindings};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

// ── fixture: the conformance document (mirrors tests/r2_3.rs) ───────────────

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

// ── service plumbing (mirrors tests/r2_3.rs) ────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r32-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: test_dir("svc").join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: test_dir("svc").join("ws"),
        holder: "r32".into(),
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

fn hello_experimental(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r32")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([("experimental", Json::Bool(true))]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn new_spec_with(environment: Json) -> Json {
    Json::obj([
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
    ])
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

fn open_with_info(svc: &mut EmbedService, info: BTreeMap<String, Json>) -> Json {
    call(
        svc,
        "open_session",
        Json::obj([
            ("spec", new_spec_with(local_host_env(info))),
            ("idempotency_key", Json::str("open-1")),
        ]),
    )
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

fn ingress_decl(triggers: &[&str]) -> Json {
    Json::Arr(vec![Json::obj([
        ("source_ref", Json::str("ing:ci")),
        (
            "triggers",
            Json::Arr(triggers.iter().map(|t| Json::str(*t)).collect()),
        ),
    ])])
}

// ── R-2.12.6 — the declaration channel ──────────────────────────────────────

#[test]
fn r3_2_ingress_adapters_stamp_the_manifest() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let declared = ingress_decl(&["external", "manual"]);
    let s = ok(&open_with_info(
        &mut svc,
        BTreeMap::from([("ingress_adapters".to_string(), declared.clone())]),
    ));
    let run = run_id(&s);
    // The declaration is durable on the manifest — `wakeup_subscribe`
    // reads it, never the caller's claim (CC3).
    let stamped = svc
        .store()
        .manifest(&run)
        .unwrap()
        .extra
        .get("ingress_adapters")
        .cloned();
    assert_eq!(
        stamped,
        Some(declared),
        "the manifest carries the declaration"
    );
    // An undeclared session stamps nothing.
    let s2 = ok(&call(
        &mut svc,
        "open_session",
        Json::obj([
            ("spec", new_spec_with(local_host_env(BTreeMap::new()))),
            ("idempotency_key", Json::str("open-2")),
        ]),
    ));
    assert!(
        !svc.store()
            .manifest(&run_id(&s2))
            .unwrap()
            .extra
            .contains_key("ingress_adapters"),
        "no declaration, no member"
    );
}

#[test]
fn r3_2_malformed_ingress_declaration_refuses_at_open() {
    let mut svc = service();
    hello_experimental(&mut svc);
    for bad in [
        // A member that is not an array.
        Json::str("external"),
        // An entry without `source_ref`.
        Json::Arr(vec![Json::obj([(
            "triggers",
            Json::Arr(vec![Json::str("external")]),
        )])]),
        // A trigger outside the closed {external, manual} set.
        Json::Arr(vec![Json::obj([
            ("source_ref", Json::str("ing:ci")),
            ("triggers", Json::Arr(vec![Json::str("timer")])),
        ])]),
    ] {
        let r = open_with_info(
            &mut svc,
            BTreeMap::from([("ingress_adapters".to_string(), bad.clone())]),
        );
        assert_eq!(
            err_kind(&r),
            "SchemaViolation",
            "{bad:?} must refuse at open"
        );
    }
}

// ── R-2.12.6 — admission + the durable occurrence path ──────────────────────

#[test]
fn r3_2_declared_ingress_subscribes_and_occurs_durably() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = ok(&open_with_info(
        &mut svc,
        BTreeMap::from([("ingress_adapters".to_string(), ingress_decl(&["external"]))]),
    ));
    let sid = session_id(&s);
    let run = run_id(&s);

    // `subscribe{external}` admits where the manifest declares the adapter.
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
    let sub_id = sub
        .get("subscription_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // `record_occurrence` — the ingress producer's durable row lands
    // *before* the fire (CC3); a replayed key is the audited skip.
    let o1 = ok(&call(
        &mut svc,
        "record_occurrence",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            ("subscription_id", Json::str(sub_id.clone())),
            ("occurrence_key", Json::str("wh-1")),
        ]),
    ));
    assert_eq!(o1.get("outcome").and_then(Json::as_str), Some("occurred"));
    let classes: Vec<&str> = svc
        .store()
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.as_str())
        .collect();
    let occurred_at = classes
        .iter()
        .position(|c| *c == "control.wakeup.occurred")
        .expect("occurred row");
    let scheduled_at = classes
        .iter()
        .position(|c| *c == "control.wakeup.scheduled")
        .expect("scheduled row");
    assert!(
        scheduled_at < occurred_at,
        "the subscription precedes its occurrence (durable-before-visible)"
    );

    // A duplicate occurrence key is the audited skip — never a second
    // durable `occurred` row (OQ-313 idempotence).
    let o2 = ok(&call(
        &mut svc,
        "record_occurrence",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            ("subscription_id", Json::str(sub_id.clone())),
            ("occurrence_key", Json::str("wh-1")),
        ]),
    ));
    assert_eq!(o2.get("outcome").and_then(Json::as_str), Some("skipped"));
    assert_eq!(
        svc.store()
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "control.wakeup.occurred")
            .count(),
        1
    );
}

#[test]
fn r3_2_undeclared_ingress_refuses_typed_over_the_boundary() {
    let mut svc = service();
    hello_experimental(&mut svc);
    // A plain `local_host` session declares nothing.
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj([
            ("spec", new_spec_with(local_host_env(BTreeMap::new()))),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ));
    let sid = session_id(&s);
    for trig in [
        Json::obj([
            ("type", Json::str("external")),
            ("kind", Json::str("webhook")),
            ("source_ref", Json::str("ing:ci")),
        ]),
        Json::obj([
            ("type", Json::str("manual")),
            ("principal", Json::str("op")),
        ]),
    ] {
        let r = call(
            &mut svc,
            "subscribe",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("trigger", trig.clone()),
                (
                    "policy",
                    Json::obj([("delivery_mode", Json::str("follow_up"))]),
                ),
            ]),
        );
        assert_eq!(
            err_kind(&r),
            "Refused",
            "undeclared {trig:?} must refuse typed"
        );
    }
    // No durable subscription landed.
    let run = run_id(&s);
    assert!(
        svc.store()
            .events(&run)
            .unwrap()
            .iter()
            .all(|e| e.class != "control.wakeup.scheduled"),
        "nothing durable on a refused subscribe"
    );
}

#[test]
fn r3_2_declared_manual_follow_up_admits_over_the_boundary() {
    let mut svc = service();
    hello_experimental(&mut svc);
    let s = ok(&open_with_info(
        &mut svc,
        BTreeMap::from([("ingress_adapters".to_string(), ingress_decl(&["manual"]))]),
    ));
    let sid = session_id(&s);
    // `manual` + `follow_up` — the S4.9 fleet boundary generalized: a
    // non-fleet run declares `manual` ingress and the attended surface
    // rides it (the `steer` leg is independent — R2.6).
    let r = call(
        &mut svc,
        "subscribe",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "trigger",
                Json::obj([
                    ("type", Json::str("manual")),
                    ("principal", Json::str("op")),
                ]),
            ),
            (
                "policy",
                Json::obj([("delivery_mode", Json::str("follow_up"))]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "declared manual admits: {r:?}");
}
