//! R2.13 boundary coverage — `lab.receipt.record` (§5g.6 §2 C2; ADR-0345
//! D4): the receiver-receipt lift over `hh-embed/1`. The durable-positive
//! paths (binding, idempotence, audit_view accounting) live in
//! `hh-ledger/tests/r2_13.rs`; the boundary legs here prove the op
//! dispatches through the one `handle` path with typed refusals — a
//! writer-session requirement, the `serves_measurement` capability gate,
//! member-shape validation, and the effect binding.

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

// ── fixture: a minimal conforming hir/1 document (the conformance shape) ────

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
        "hh-embed-r213-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service(tag: &str) -> EmbedService {
    let root = test_dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r213".into(),
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
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    caps.insert("serves_measurement".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r213-test")),
                    ("version", Json::str("0")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn new_spec() -> Json {
    Json::obj(vec![
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
    ])
}

fn writer_session(svc: &mut EmbedService) -> Json {
    ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec()),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ))
}

fn receipt_params(session_id: &str, effect_id: &str) -> Json {
    Json::obj(vec![
        ("session_id", Json::str(session_id)),
        ("effect_id", Json::str(effect_id)),
        ("receiver_ref", Json::str("receiver:hosted-1")),
        ("receipt_id", Json::str("rcpt-1")),
        ("attested_at", Json::Int(1_700_000_000_000)),
        (
            "attestation",
            Json::obj(vec![
                ("key_id", Json::str("receiver-key-7")),
                ("alg_ref", Json::str("receiver-local-scheme")),
                ("sig", Json::str("hex:0123456789abcdef")),
            ]),
        ),
    ])
}

// ── AC: the boundary lift ────────────────────────────────────────────────────

#[test]
fn lab_receipt_record_is_a_typed_boundary_op() {
    let mut svc = service("rcpt");
    hello_experimental(&mut svc);
    let s = writer_session(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();

    // Missing required members → SchemaViolation, never a default.
    let missing = call(
        &mut svc,
        "lab.receipt.record",
        Json::obj(vec![("session_id", Json::str(session_id.clone()))]),
    );
    assert_eq!(err_kind(&missing), "SchemaViolation", "{missing:?}");

    // A structurally-valid receipt against an unknown effect — the fold
    // binding refuses (`UnknownEffect` surfaces through `Refused`).
    let unknown = call(
        &mut svc,
        "lab.receipt.record",
        receipt_params(&session_id, "eff-none"),
    );
    assert_eq!(err_kind(&unknown), "Refused", "{unknown:?}");
    assert!(unknown
        .get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .contains("UnknownEffect"));

    // A malformed attestation is a SchemaViolation through `ledger_err`.
    let mut bad = receipt_params(&session_id, "eff-1");
    if let Json::Obj(m) = &mut bad {
        m.insert("attestation".to_string(), Json::Null);
    }
    let bad = call(&mut svc, "lab.receipt.record", bad);
    assert_eq!(err_kind(&bad), "SchemaViolation", "{bad:?}");

    // Read-only attach session — `session_is_read_only`.
    let a = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id.clone())),
                ]),
            ),
            ("idempotency_key", Json::str("att-1")),
        ]),
    ));
    let attach_id = a
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let ro = call(
        &mut svc,
        "lab.receipt.record",
        receipt_params(&attach_id, "eff-1"),
    );
    assert_eq!(err_kind(&ro), "Refused", "{ro:?}");

    // The run's durable prefix carries no fabricated receipt rows.
    let events = svc.store().events(&run_id).unwrap();
    assert!(!events.iter().any(|e| e.class == "lifecycle.ledger.receipt"));
}

#[test]
fn lab_receipt_record_needs_the_measurement_capability() {
    // Without `serves_measurement` negotiated at hello, the capability
    // gate refuses before dispatch — the op is a Lab-surface lift.
    let mut svc = service("rcpt-cap");
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    let r = call(
        &mut svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r213-test")),
                    ("version", Json::str("0")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
    let s = writer_session(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let resp = call(
        &mut svc,
        "lab.receipt.record",
        receipt_params(&session_id, "eff-1"),
    );
    let kind = err_kind(&resp);
    assert_eq!(kind, "CapabilityNotDeclared", "{resp:?}");
}
