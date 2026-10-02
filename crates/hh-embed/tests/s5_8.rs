//! S5.8 — the `R-2.2.3²` surface seams (§5a.3 extension row):
//! `EmbedService::deliver_due_wakeups` (the kernel-internal trigger pass
//! a surface drain calls — `control.wakeup.occurred`/`fired` durable
//! before any delivery, `(subscription, occurrence)` dedup across
//! calls), and the hosted `session/resume` mapping's durable record —
//! `open_session{kind:"resume", mode:"takeover", cause}` lands
//! `lifecycle.run.resumed{recovery_decision.cause}` with the caller's
//! declared cause from the closed set (AC-R-2.2.3-13).

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

// ── fixture: the minimal conforming hir/1 document (the conformance
// suite's shape) ────────────────────────────────────────────────────────

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

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-s58-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// `service_in` over a `ManualClock` — the restart-resume fixture steps
/// the clock past the writer-lease TTL so the takeover fences the dead
/// process's record (writer takeover is expiry-disciplined — ADR-0130).
fn service_at(dir: &std::path::Path, now_ms: u64) -> EmbedService {
    EmbedService::open_with(
        ServiceConfig {
            store_root: dir.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: dir.join("ws"),
            holder: "s5-8".into(),
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

fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("s5-8")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![("experimental", Json::Bool(true))]),
            ),
        ]),
    );
    ok(&r);
}

fn open_new(svc: &mut EmbedService) -> Json {
    let spec = Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
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
    ]);
    let r = call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec),
            ("idempotency_key", Json::str("open-1")),
        ]),
    );
    ok(&r)
}

fn tail(svc: &mut EmbedService, session_id: &str) -> Vec<Json> {
    let page = ok(&call(
        svc,
        "read",
        Json::obj([
            ("session_id", Json::str(session_id)),
            (
                "cursor",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(1))]),
            ),
            ("direction", Json::str("fwd")),
            ("limit", Json::Int(512)),
        ]),
    ));
    match page.get("events") {
        Some(Json::Arr(v)) => v.clone(),
        _ => Vec::new(),
    }
}

/// `deliver_due_wakeups` — the surface drain seam: a due timer's
/// `control.wakeup.occurred`/`fired` rows are durable before the delivery
/// is returned, and the `(subscription, occurrence)` dedup key makes the
/// second call empty.
#[test]
fn deliver_due_wakeups_durable_first_and_deduped() {
    let mut svc = service_at(&test_dir("woken"), 1_000);
    hello(&mut svc);
    let s = open_new(&mut svc);
    let sid = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // A timer already due.
    let sub = ok(&call(
        &mut svc,
        "subscribe",
        Json::obj([
            ("session_id", Json::str(sid.clone())),
            (
                "trigger",
                Json::obj([("type", Json::str("timer")), ("at_ms", Json::Int(0))]),
            ),
        ]),
    ));
    assert!(sub.get("subscription_id").is_some(), "{sub:?}");
    // The seam fires the due occurrence — the delivery is a fresh
    // `WokenDelivery`.
    let w = svc.deliver_due_wakeups(&sid).unwrap();
    assert_eq!(w.len(), 1, "one due occurrence fires once: {w:?}");
    // Durable-first: both rows are in the record before the surface saw
    // anything.
    let events = tail(&mut svc, &sid);
    for want in ["control.wakeup.occurred", "control.wakeup.fired"] {
        assert!(
            events
                .iter()
                .any(|e| e.get("class").and_then(Json::as_str) == Some(want)),
            "{want} durable: {events:?}"
        );
    }
    // Dedup: the same drain set on a second call delivers nothing.
    assert!(svc.deliver_due_wakeups(&sid).unwrap().is_empty());
}

/// The hosted `session/resume` mapping (AC-R-2.2.3-13): a takeover resume
/// after the writer is gone mints `lifecycle.run.resumed` with the
/// declared `cause` — `wakeup` here; an omitted cause defaults to the
/// pre-S5.8 `operator` spelling.
#[test]
fn resume_cause_mints_run_resumed() {
    let root = test_dir("resume-cause");
    let run_id;
    {
        let mut svc = service_at(&root, 1_000);
        hello(&mut svc);
        let s = open_new(&mut svc);
        run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    } // `svc` drops — the session table is process-local, so the
      // restarted service hits the durable-resume path. The clock steps
      // past the 120s writer-lease TTL so `restore`'s takeover fences
      // the dead writer's record (expiry discipline, never probe).
    let mut svc = service_at(&root, 1_000 + 120_001);
    hello(&mut svc);
    let r = call(
        &mut svc,
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run_id.clone())),
                    ("mode", Json::str("takeover")),
                    ("cause", Json::str("wakeup")),
                ]),
            ),
            ("idempotency_key", Json::str("res-wake")),
        ]),
    );
    let s2 = ok(&r);
    let sid2 = s2
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let events = tail(&mut svc, &sid2);
    let resumed = events
        .iter()
        .find(|e| e.get("class").and_then(Json::as_str) == Some("lifecycle.run.resumed"))
        .unwrap_or_else(|| panic!("lifecycle.run.resumed minted: {events:?}"));
    let cause = resumed
        .get("payload")
        .and_then(|p| p.get("recovery_decision"))
        .and_then(|d| d.get("cause"))
        .and_then(Json::as_str);
    assert_eq!(cause, Some("wakeup"), "{resumed:?}");
}
