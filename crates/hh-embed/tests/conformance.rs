//! S1.25 — `hh-embed/1` conformance (ticket 029).
//!
//! Binding (a) drives [`EmbedService::handle`] in-process; binding (b) drives
//! the newline-delimited JSON-RPC 2.0 loop ([`hh_embed::stdio::serve`]) over
//! memory buffers. The AC-1 flow `hello → open_session(new) → submit →
//! stream_events → close`, the session lifecycle (`new` / `resume{continue,
//! takeover}` / `attach` read-only / `close`), the closed error sum's stable
//! variants, the frame model, injection refusals, capability/experimental
//! gates, idempotency, and binding byte parity.
//!
//! The `open_session{kind:"new"}` path resolves a real `hir/1` document
//! (agent + rule + budget + permissions — the same shape as the
//! hh-assembly acceptance fixture) through `load → resolve → validate →
//! seal` against the embedded Stage-1 registry.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::{AttendanceDeclaration, ClientDescriptor, HostCapabilities};
use hh_embed_schema::{ops, Direction};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::{parse, Json};
use hh_wire::jsonrpc::Request;

// ── fixture: a minimal conforming hir/1 document ────────────────────────────
// The same shape the hh-assembly acceptance suite resolves
// (`tests/common/mod.rs`): `{HarnessRule, Budget, Permission, AgentProcess}`
// + the Stage-1 assembly (control_strategy → hh/round_robin, context_policy →
// hh/full_window — the two variants the embedded registry seeds).

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
    document_json_with("hh/round_robin")
}

/// The same fixture bound to a different `control_strategy` slot
/// (`hh/react-steerable` — seeded into the embedded registry with the
/// `steer_mode`/`concurrent_input` declarations; S2.11).
fn document_json_with(control_variant: &str) -> Json {
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
            control_variant,
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

// ── service helpers ─────────────────────────────────────────────────────────

/// A unique store/workspace root under the OS temp dir (no `tempfile` dep —
/// the path is pid+counter unique and the store creates it lazily).
fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-conf-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap()
}

fn service_in(dir: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: dir.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: dir.join("ws"),
        holder: "conformance".into(),
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

/// `result` or die with the error payload.
fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

/// The typed error kind (`error.data.kind`).
fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

fn caps_json(extra: &[(&str, bool)]) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in extra {
        m.insert((*k).to_string(), Json::Bool(*v));
    }
    Json::Obj(m)
}

fn hello_params(caps: Json) -> Json {
    Json::obj(vec![
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj(vec![
                ("name", Json::str("conformance")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        ("capabilities", caps),
    ])
}

fn hello(svc: &mut EmbedService) -> Json {
    let r = call(
        svc,
        "hello",
        hello_params(caps_json(&[
            ("serves_host_executor", true),
            ("serves_permission_channel", true),
            ("accepts_ephemeral_frames", true),
        ])),
    );
    ok(&r)
}

/// The base document plus a `HarnessRule{pre_authorize{grants}}` — the
/// sealed rule `open_session` mints `policy_rule`-basis handles over
/// (§5g.1 §9 Stage-2).
fn document_json_preauth() -> Json {
    let mut doc = document_json();
    // The document fixture's nodes are `vec![rule, budget, perm, agent]` —
    // append a pre-authorize rule granting `fs_read` at `workspace/**`.
    if let Json::Obj(m) = &mut doc {
        if let Some(Json::Arr(nodes)) = m.get_mut("nodes") {
            let grant = hh_hir::grant_json(
                &hh_hir::records::Grant {
                    effect: hh_hir::EffectClass::domain_only(hh_hir::kinds::EffectDomain::FsRead),
                    scope: "workspace/**".to_string(),
                    constraints: hh_hir::records::GrantConstraints::default(),
                    delegable: false,
                },
                false,
            );
            let rule = sid(
                node(
                    EntityKind::HarnessRule,
                    KindRecord::HarnessRule(HarnessRuleRecord {
                        rule_id: "test:rule.preauth".into(),
                        trigger: Json::Null,
                        action: RuleAction::PreAuthorize(Json::obj(vec![
                            ("grants", Json::Arr(vec![grant])),
                            ("scope", Json::str("run")),
                        ])),
                        scope: Json::Null,
                        conditioned_on: None,
                        assumption_debt: None,
                    }),
                    9,
                ),
                "test:rule.preauth",
            );
            nodes.push(hh_hir::wire::node_to_json(&rule));
        }
    }
    doc
}

fn new_spec_with_doc(doc: Json, supplies: Option<Json>) -> Json {
    let mut spec = new_spec(supplies);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "definition".to_string(),
            Json::obj(vec![("kind", Json::str("document")), ("document", doc)]),
        );
    }
    spec
}

fn new_spec(supplies: Option<Json>) -> Json {
    let mut m = vec![
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
        Json::obj(vec![
            ("spec", new_spec(None)),
            ("idempotency_key", Json::str("open-1")),
        ]),
    );
    ok(&r)
}

fn submit(svc: &mut EmbedService, session: &str, input: Vec<Json>) -> Json {
    call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            ("input", Json::Arr(input)),
            ("idempotency_key", Json::str("submit-1")),
        ]),
    )
}

fn text_input(t: &str) -> Vec<Json> {
    vec![Json::obj(vec![
        ("kind", Json::str("text")),
        ("text", Json::str(t)),
    ])]
}

fn close(svc: &mut EmbedService, session: &str) -> Json {
    call(
        svc,
        "close",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            ("reason", Json::str("done")),
        ]),
    )
}

/// `stream_events` → `subscription_id`, then drain the adapter's queued
/// `stream.frame` notifications → the `frame` payloads.
fn open_stream(svc: &mut EmbedService, session: &str) -> String {
    let r = call(
        svc,
        "stream_events",
        Json::obj(vec![("session_id", Json::str(session))]),
    );
    ok(&r)
        .get("subscription_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string()
}

fn frames_of(svc: &mut EmbedService, sub: &str) -> Vec<Json> {
    svc.poll_frames(sub)
        .iter()
        .filter_map(|n| n.get("params").and_then(|p| p.get("frame")).cloned())
        .collect()
}

fn all_frames(svc: &mut EmbedService, sub: &str) -> Vec<Json> {
    let mut out = frames_of(svc, sub);
    // Drain anything pending (upcalls ride along in the same queue).
    for n in svc.drain_notifications() {
        if let Some(f) = n.get("params").and_then(|p| p.get("frame")) {
            out.push(f.clone());
        }
    }
    out
}

fn frame_kinds(frames: &[Json]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|f| f.get("kind").and_then(Json::as_str).map(String::from))
        .collect()
}

// ── Group H — handshake ─────────────────────────────────────────────────────

#[test]
fn h_hello_negotiates_identity_and_capabilities() {
    let mut svc = service();
    let h = hello(&mut svc);
    let k = h.get("kernel").unwrap();
    assert_eq!(k.get("contract_major"), Some(&Json::Int(1)));
    assert_eq!(
        k.get("schema_hash").and_then(Json::as_str),
        Some(hh_embed_schema::schema_hash().as_str())
    );
    assert_eq!(
        h.get("negotiated")
            .and_then(|n| n.get("serves_host_executor")),
        Some(&Json::Bool(true))
    );
    assert_eq!(h.get("experimental_enabled"), Some(&Json::Bool(false)));
    // The stability table names every declared op.
    let stability = h.get("stability").unwrap();
    for op in ops::registry() {
        assert!(
            stability.get(op.name).is_some(),
            "stability table missing {}",
            op.name
        );
    }
}

#[test]
fn h_hello_typed_refusals() {
    let mut svc = service();
    // contract_major we do not serve → ContractMajorUnsupported.
    let mut bad = hello_params(caps_json(&[]));
    if let Json::Obj(m) = &mut bad {
        m.insert("contract_major".into(), Json::Int(99));
    }
    assert_eq!(
        err_kind(&call(&mut svc, "hello", bad)),
        "ContractMajorUnsupported"
    );
    // Asserted schema hash the kernel does not hold → SchemaMismatch.
    let mut bad = hello_params(caps_json(&[]));
    if let Json::Obj(m) = &mut bad {
        m.insert("schema_hash".into(), Json::str("sha256:deadbeef"));
    }
    assert_eq!(err_kind(&call(&mut svc, "hello", bad)), "SchemaMismatch");
    // Kernel below the client's floor → KernelBelowFloor.
    let mut bad = hello_params(caps_json(&[]));
    if let Json::Obj(m) = &mut bad {
        m.insert("kernel_floor".into(), Json::str("99.0.0"));
    }
    assert_eq!(err_kind(&call(&mut svc, "hello", bad)), "KernelBelowFloor");
    // The correct hash negotiates.
    let mut good = hello_params(caps_json(&[]));
    if let Json::Obj(m) = &mut good {
        m.insert(
            "schema_hash".into(),
            Json::str(hh_embed_schema::schema_hash()),
        );
    }
    assert!(call(&mut svc, "hello", good).get("result").is_some());
}

#[test]
fn h_gates_before_and_after_hello() {
    let mut svc = service();
    // Any call before hello → NotInitialized.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "describe",
            Json::obj(vec![("session_id", Json::str("s"))])
        )),
        "NotInitialized"
    );
    hello(&mut svc);
    // Unknown verb → SchemaViolation{unknown_method}.
    let e = call(&mut svc, "no_such_verb", Json::Obj(BTreeMap::new()));
    assert_eq!(err_kind(&e), "SchemaViolation");
    // A kernel→host upcall name as a request → SchemaViolation{upcall_direction}.
    let e = call(&mut svc, "request_permission", Json::Obj(BTreeMap::new()));
    assert_eq!(err_kind(&e), "SchemaViolation");
    // Missing required member → SchemaViolation.
    let e = call(&mut svc, "close", Json::Obj(BTreeMap::new()));
    assert_eq!(err_kind(&e), "SchemaViolation");
    // Unknown member → UnknownField.
    let e = call(
        &mut svc,
        "close",
        Json::obj(vec![
            ("session_id", Json::str("s")),
            ("reason", Json::str("done")),
            ("bogus", Json::Int(1)),
        ]),
    );
    assert_eq!(err_kind(&e), "UnknownField");
    // Unknown session → UnknownSession.
    let e = call(
        &mut svc,
        "close",
        Json::obj(vec![
            ("session_id", Json::str("sess_missing")),
            ("reason", Json::str("done")),
        ]),
    );
    assert_eq!(err_kind(&e), "UnknownSession");
}

// ── Group S — session lifecycle ─────────────────────────────────────────────

#[test]
fn s_open_submit_stream_close_the_ac1_flow() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    assert!(session_id.starts_with("sess"));
    assert!(s
        .get("manifest_ref")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
    assert!(s.get("cursor").and_then(|c| c.get("seq")).is_some());

    // stream_events → subscription ticket.
    let sub = open_stream(&mut svc, &session_id);

    // submit → Accepted{turn_id}.
    let r = submit(&mut svc, &session_id, text_input("hi"));
    let acc = ok(&r);
    assert!(acc.get("turn_id").and_then(Json::as_str).is_some());

    // close → Closed{final:RunSummaryRef} — the run ended after the
    // scripted turn (hh.submit completes → lifecycle.run.finished).
    let closed = ok(&close(&mut svc, &session_id));
    let fin = closed.get("final").unwrap();
    assert_eq!(fin.get("run_id"), Some(&Json::str(run_id.clone())));

    // Frames: durable rows, a sync marker, item_started, and a terminal
    // closed frame (run_ended — the close drained the finished run).
    let frames = all_frames(&mut svc, &sub);
    let kinds = frame_kinds(&frames);
    assert!(kinds.iter().any(|k| k == "durable"), "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "sync"), "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "closed"), "{kinds:?}");
    // Durable frames carry the ledger hash chain.
    let d = frames
        .iter()
        .find(|f| f.get("kind") == Some(&Json::str("durable")))
        .unwrap();
    assert!(d
        .get("hash")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
    assert!(d.get("event").is_some());
    assert_eq!(d.get("durability"), Some(&Json::str("ledger")));
    // The closed frame's reason is a member of the closed sum.
    let reason = frames
        .iter()
        .find(|f| f.get("kind") == Some(&Json::str("closed")))
        .and_then(|f| f.get("reason"))
        .and_then(Json::as_str)
        .unwrap();
    assert!(
        hh_embed_schema::frames::CLOSED_REASONS.contains(&reason),
        "{reason}"
    );

    // The session is gone; a second close is UnknownSession.
    assert_eq!(err_kind(&close(&mut svc, &session_id)), "UnknownSession");
    // The run persists — an attach session reads it.
    let r = call(
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
    );
    assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
}

#[test]
fn s_open_session_idempotent_replay() {
    let mut svc = service();
    hello(&mut svc);
    let params = Json::obj(vec![
        ("spec", new_spec(None)),
        ("idempotency_key", Json::str("same-key")),
    ]);
    let a = ok(&call(&mut svc, "open_session", params.clone()));
    let b = ok(&call(&mut svc, "open_session", params));
    assert_eq!(a.to_canonical_string(), b.to_canonical_string());
}

#[test]
fn s_attach_is_read_only_by_construction() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let writer_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
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
    assert_ne!(attach_id, writer_id);
    // Read ops work on the attach session…
    let r = call(
        &mut svc,
        "read",
        Json::obj(vec![
            ("session_id", Json::str(attach_id.clone())),
            (
                "cursor",
                Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("limit", Json::Int(16)),
        ]),
    );
    assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
    // …but every writer verb refuses — read-only by construction.
    assert_eq!(
        err_kind(&submit(&mut svc, &attach_id, text_input("x"))),
        "Refused"
    );
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "cancel",
            Json::obj(vec![
                ("session_id", Json::str(attach_id.clone())),
                ("scope", Json::obj(vec![("kind", Json::str("run"))])),
            ]),
        )),
        "Refused"
    );
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "steer",
            Json::obj(vec![("session_id", Json::str(attach_id.clone()))]),
        )),
        "Refused"
    );
    // A second attach coexists (no lease is taken).
    let a2 = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id)),
                ]),
            ),
            ("idempotency_key", Json::str("att-2")),
        ]),
    );
    assert!(a2.get("result").is_some());
}

#[test]
fn s_resume_continue_would_block_then_takeover_fences() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let old_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let spec = |mode: &str| {
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run_id.clone())),
                    ("mode", Json::str(mode)),
                ]),
            ),
            ("idempotency_key", Json::str(format!("res-{mode}"))),
        ])
    };
    // continue while the writer holds the lease → WouldBlock.
    assert_eq!(
        err_kind(&call(&mut svc, "open_session", spec("continue"))),
        "WouldBlock"
    );
    // takeover fences the old writer → a new session on the same run.
    let s2 = ok(&call(&mut svc, "open_session", spec("takeover")));
    let new_id = s2
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert_eq!(s2.get("run_id"), Some(&Json::str(run_id)));
    assert_ne!(new_id, old_id);
    // The fenced session's next verb → SessionDetached.
    assert_eq!(
        err_kind(&submit(&mut svc, &old_id, text_input("x"))),
        "SessionDetached"
    );
    // The taking-over session writes.
    assert!(submit(&mut svc, &new_id, text_input("after takeover"))
        .get("result")
        .is_some());
}

#[test]
fn s_resume_unknown_run_and_attach_missing() {
    let mut svc = service();
    hello(&mut svc);
    for spec in [
        Json::obj(vec![
            ("kind", Json::str("attach")),
            ("run_id", Json::str("run_missing")),
        ]),
        Json::obj(vec![
            ("kind", Json::str("resume")),
            ("run_id", Json::str("run_missing")),
            ("mode", Json::str("continue")),
        ]),
        Json::obj(vec![
            ("kind", Json::str("resume")),
            ("run_id", Json::str("run_missing")),
            ("mode", Json::str("takeover")),
        ]),
    ] {
        let e = call(
            &mut svc,
            "open_session",
            Json::obj(vec![("spec", spec), ("idempotency_key", Json::str("k"))]),
        );
        assert_eq!(err_kind(&e), "UnknownRun", "{}", e.to_canonical_string());
    }
}

#[test]
fn s_open_ref_definition_unresolved_and_bad_attendance() {
    let mut svc = service();
    hello(&mut svc);
    // `ref` definitions have no publish path at Stage 1 → UnresolvedRef.
    let e = call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("new")),
                    (
                        "definition",
                        Json::obj(vec![
                            ("kind", Json::str("ref")),
                            ("ref", Json::str("h:core@1.0.0")),
                        ]),
                    ),
                    ("overrides", Json::Arr(vec![])),
                    (
                        "environment",
                        Json::obj(vec![
                            ("kind", Json::str("connection_info")),
                            ("connection_info", Json::Obj(BTreeMap::new())),
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
                ]),
            ),
            ("idempotency_key", Json::str("k")),
        ]),
    );
    assert_eq!(err_kind(&e), "UnresolvedRef");
    // A hosted environment class is unavailable → EnvironmentUnavailable.
    let mut spec = new_spec(None);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "environment".into(),
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj(vec![("class", Json::str("remote_acp"))]),
                ),
            ]),
        );
    }
    let e = call(
        &mut svc,
        "open_session",
        Json::obj(vec![("spec", spec), ("idempotency_key", Json::str("k2"))]),
    );
    assert_eq!(
        err_kind(&e),
        "EnvironmentUnavailable",
        "{}",
        e.to_canonical_string()
    );
    // `unattended` + `approval_mode:"sync"` → UnattendedRequiresInput.
    let mut spec = new_spec(None);
    if let Json::Obj(m) = &mut spec {
        m.insert("approval_mode".into(), Json::str("sync"));
        m.insert(
            "attendance".into(),
            AttendanceDeclaration {
                value: "unattended".into(),
                source: "declared".into(),
            }
            .to_json(),
        );
    }
    let e = call(
        &mut svc,
        "open_session",
        Json::obj(vec![("spec", spec), ("idempotency_key", Json::str("k3"))]),
    );
    assert_eq!(err_kind(&e), "UnattendedRequiresInput");
    // A malformed document → InvalidDefinition (diagnostics, never a panic).
    let mut spec = new_spec(None);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "definition".into(),
            Json::obj(vec![
                ("kind", Json::str("document")),
                (
                    "document",
                    Json::obj(vec![("hir_version", Json::str("HIR/0"))]),
                ),
            ]),
        );
    }
    let e = call(
        &mut svc,
        "open_session",
        Json::obj(vec![("spec", spec), ("idempotency_key", Json::str("k4"))]),
    );
    assert_eq!(err_kind(&e), "InvalidDefinition");
}

// ── Group W — work ops ──────────────────────────────────────────────────────

#[test]
fn w_submit_idempotent_turn_active_and_draining() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let p = |key: &str| {
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("input", Json::Arr(text_input("hi"))),
            ("idempotency_key", Json::str(key)),
        ])
    };
    let a = ok(&call(&mut svc, "submit", p("dup")));
    let b = ok(&call(&mut svc, "submit", p("dup")));
    assert_eq!(a.to_canonical_string(), b.to_canonical_string());
    // A distinct key after the run finished → Draining (the scripted
    // hh.submit turn completes the run — no second turn exists).
    assert_eq!(err_kind(&call(&mut svc, "submit", p("next"))), "Draining");
}

#[test]
fn w_cancel_run_scope_and_turn_mismatch() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // Cancelling a turn that is not active → TurnMismatch.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "cancel",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                (
                    "scope",
                    Json::obj(vec![
                        ("kind", Json::str("turn")),
                        ("turn_id", Json::str("turn-99")),
                    ]),
                ),
            ]),
        )),
        "TurnMismatch"
    );
    // Run-scope cancel is acknowledged.
    let r = call(
        &mut svc,
        "cancel",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("scope", Json::obj(vec![("kind", Json::str("run"))])),
        ]),
    );
    assert_eq!(
        r.get("result").and_then(|x| x.get("acknowledged")),
        Some(&Json::Bool(true)),
        "{}",
        r.to_canonical_string()
    );
}

#[test]
fn w_steer_declares_unsupported_and_checks_turn() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // Wrong expected_turn_id → TurnMismatch (the check precedes the refusal).
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "steer",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("expected_turn_id", Json::str("turn-99")),
            ]),
        )),
        "TurnMismatch"
    );
    // Correctly addressed → Unsupported{by:"control_strategy"} — honest
    // Stage-1 surface, never a silent queue.
    let e = call(
        &mut svc,
        "steer",
        Json::obj(vec![
            ("session_id", Json::str(id)),
            ("input", Json::Arr(text_input("steer"))),
        ]),
    );
    assert_eq!(err_kind(&e), "Unsupported");
}

#[test]
fn w_permission_flow_pending_decided_already_decided() {
    let mut svc = service();
    // Hello declares the permission channel AND the run's supplies declare a
    // capability that requires approval — a durable `pending` ask lands at
    // open, with the `upcall.request_permission` notification.
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[
            ("serves_permission_channel", true),
            ("accepts_ephemeral_frames", true),
        ])),
    );
    assert!(r.get("result").is_some());
    let supplies = Json::obj(vec![(
        "host_capabilities",
        Json::Arr(vec![Json::obj(vec![
            ("capability_id", Json::str("cap:shell")),
            ("surface", Json::str("host.exec.shell")),
            ("requires_approval", Json::Bool(true)),
            (
                "options",
                Json::Arr(vec![Json::str("allow_once"), Json::str("deny_once")]),
            ),
        ])]),
    )]);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec(Some(supplies))),
            ("idempotency_key", Json::str("k")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // The upcall notification is queued — drain it to find the ask id.
    let notes = svc.drain_notifications();
    let ask = notes.iter().find_map(|n| {
        if n.get("method").and_then(Json::as_str) == Some("upcall.request_permission") {
            n.get("params").cloned()
        } else {
            None
        }
    });
    // The pending id also lands durable — find it on the run.
    let pending_id = ask
        .as_ref()
        .and_then(|p| {
            p.get("permission_id")
                .or_else(|| p.get("id"))
                .or_else(|| p.get("ask_id"))
                .and_then(Json::as_str)
                .map(String::from)
        })
        .or_else(|| {
            svc.store()
                .events(s.get("run_id").and_then(Json::as_str).unwrap())
                .ok()
                .and_then(|evs| {
                    evs.iter()
                        .find(|e| e.class == "security.permission.pending")
                        .and_then(|e| {
                            e.payload
                                .get("permission_id")
                                .or_else(|| e.payload.get("id"))
                                .and_then(Json::as_str)
                                .map(String::from)
                        })
                })
        });
    // If the boundary produced no ask, the pending table is empty and the
    // honest surface is UnknownPermission — assert the flow end-to-end only
    // when an ask exists.
    if let Some(pid) = pending_id {
        // An option that was not offered → OptionNotOffered.
        assert_eq!(
            err_kind(&call(
                &mut svc,
                "respond_permission",
                Json::obj(vec![
                    ("session_id", Json::str(id.clone())),
                    ("permission_id", Json::str(pid.clone())),
                    (
                        "outcome",
                        Json::obj(vec![
                            ("kind", Json::str("selected")),
                            ("option_id", Json::str("not_offered")),
                        ]),
                    ),
                    ("idempotency_key", Json::str("r1")),
                ]),
            )),
            "OptionNotOffered"
        );
        // A real decision → Recorded; replay is idempotent; a new key is
        // AlreadyDecided.
        let r = call(
            &mut svc,
            "respond_permission",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("permission_id", Json::str(pid.clone())),
                (
                    "outcome",
                    Json::obj(vec![
                        ("kind", Json::str("selected")),
                        ("option_id", Json::str("allow_once")),
                    ]),
                ),
                ("idempotency_key", Json::str("r2")),
            ]),
        );
        assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
        assert_eq!(
            err_kind(&call(
                &mut svc,
                "respond_permission",
                Json::obj(vec![
                    ("session_id", Json::str(id.clone())),
                    ("permission_id", Json::str(pid)),
                    ("outcome", Json::obj(vec![("kind", Json::str("cancelled"))])),
                    ("idempotency_key", Json::str("r3")),
                ]),
            )),
            "AlreadyDecided"
        );
    }
    // An unknown ask id → UnknownPermission.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "respond_permission",
            Json::obj(vec![
                ("session_id", Json::str(id)),
                ("permission_id", Json::str("perm_missing")),
                ("outcome", Json::obj(vec![("kind", Json::str("cancelled"))])),
                ("idempotency_key", Json::str("r4")),
            ]),
        )),
        "UnknownPermission"
    );
}

#[test]
fn w_permission_allow_lease_mints_lease_row() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("serves_permission_channel", true)])),
    );
    assert!(r.get("result").is_some());
    // A capability asking approval and offering the canonical lease option —
    // the pending carries `request{capability_ref, args_canonical_hash,
    // subject_ref}`, the material `allow_lease` mints the lease over.
    let supplies = Json::obj(vec![(
        "host_capabilities",
        Json::Arr(vec![Json::obj(vec![
            ("capability_id", Json::str("cap:shell")),
            ("surface", Json::str("host.exec.shell")),
            ("requires_approval", Json::Bool(true)),
            (
                "options",
                Json::Arr(vec![
                    Json::str("allow_once"),
                    Json::str("allow_lease"),
                    Json::str("deny"),
                ]),
            ),
        ])]),
    )]);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec(Some(supplies))),
            ("idempotency_key", Json::str("k")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let pid = svc
        .store()
        .events(&run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.permission.pending")
        .and_then(|e| e.payload.get("permission_id").and_then(Json::as_str))
        .map(String::from)
        .expect("the declared capability ask landed durable");
    let r = call(
        &mut svc,
        "respond_permission",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("permission_id", Json::str(pid.clone())),
            (
                "outcome",
                Json::obj(vec![
                    ("kind", Json::str("selected")),
                    ("option_id", Json::str("allow_lease")),
                ]),
            ),
            ("idempotency_key", Json::str("r-lease")),
        ]),
    );
    assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
    let evs = svc.store().events(&run_id).unwrap();
    let decided = evs
        .iter()
        .find(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid.as_str())
        })
        .expect("the final decided row");
    assert_eq!(
        decided.payload.get("decision").and_then(Json::as_str),
        Some("allow")
    );
    assert_eq!(
        decided.payload.get("decider").and_then(Json::as_str),
        Some("human")
    );
    assert_eq!(
        decided.payload.get("decision_scope").and_then(Json::as_str),
        Some("session")
    );
    let lease = evs
        .iter()
        .find(|e| e.class == "security.permission.lease.granted")
        .expect("allow_lease minted the lease row");
    assert_eq!(
        lease.payload.get("permission_id").and_then(Json::as_str),
        Some(pid.as_str())
    );
    assert_eq!(
        lease
            .payload
            .get("capability_ref")
            .and_then(|c| c.get("semantic_id"))
            .and_then(Json::as_str),
        Some("cap:shell")
    );
    assert_eq!(
        lease.payload.get("scope").and_then(Json::as_str),
        Some("run")
    );
    // The fold reads the row back — the lease is live for the run.
    let st = hh_monitor::approval::ApprovalState::project(evs, u64::MAX);
    assert!(!st.leases.is_empty(), "the lease folded live");
    let (rpid, rec) = st
        .decisions
        .iter()
        .next()
        .expect("the decided record folded");
    assert_eq!(rpid, &pid);
    assert!(matches!(
        rec.decision,
        hh_monitor::decision::Decision::Allow
    ));
    assert!(rec.lease_id.is_some(), "the decided↔lease link folded");
    // The `approval`-basis handle — `granted` landed in the same batch as
    // `decided` + `lease.granted` (the decision's authority record, §5g.1
    // §9): `origin_basis = approval`, `basis_ref = permission_id`,
    // `delegable = false`, the run-scoped expiry the `allow_lease{run}`
    // response conferred.
    let granted = evs
        .iter()
        .find(|e| e.class == "security.permission.granted")
        .expect("the approval minted a granted row");
    assert_eq!(
        granted.payload.get("origin_basis").and_then(Json::as_str),
        Some("approval")
    );
    assert_eq!(
        granted.payload.get("basis_ref").and_then(Json::as_str),
        Some(pid.as_str())
    );
    assert_eq!(granted.payload.get("delegable"), Some(&Json::Bool(false)));
    assert_eq!(
        granted
            .payload
            .get("validity")
            .and_then(|v| v.get("expires_at"))
            .and_then(|e| e.get("run_id"))
            .and_then(Json::as_str),
        Some(run_id.as_str())
    );
    // The fold's decode side re-pins the holder to the row's event id and
    // the handle lands in the table.
    let handle =
        hh_monitor::events::handle_from_granted_payload(&granted.payload, &granted.event_id)
            .expect("the granted row decodes");
    assert_eq!(handle.holder.semantic_id, "conformance");
    assert_eq!(
        handle.validity.issued_at, granted.event_id,
        "issued_at is the granted event's own id"
    );
    let mut table = hh_monitor::table::HandleTable::default();
    table.handles.insert(handle.handle_id.clone(), handle);
    assert_eq!(table.handles.len(), 1);
}

/// `allow_once` mints the `approval`-basis handle too — `scope = once`,
/// effect-scoped expiry (H-7: the single `effect_id` the ask covered; a
/// capability-level host ask has no effect — the `effect` member is the
/// empty coordinate and the handle confers nothing past the record).
#[test]
fn w_permission_allow_once_mints_approval_handle() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("serves_permission_channel", true)])),
    );
    assert!(r.get("result").is_some());
    let supplies = Json::obj(vec![(
        "host_capabilities",
        Json::Arr(vec![Json::obj(vec![
            ("capability_id", Json::str("cap:shell")),
            ("surface", Json::str("host.exec.shell")),
            ("requires_approval", Json::Bool(true)),
        ])]),
    )]);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec(Some(supplies))),
            ("idempotency_key", Json::str("k")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let pid = svc
        .store()
        .events(&run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.permission.pending")
        .and_then(|e| e.payload.get("permission_id").and_then(Json::as_str))
        .map(String::from)
        .expect("the declared capability ask landed durable");
    let r = call(
        &mut svc,
        "respond_permission",
        Json::obj(vec![
            ("session_id", Json::str(id)),
            ("permission_id", Json::str(pid.clone())),
            (
                "outcome",
                Json::obj(vec![
                    ("kind", Json::str("selected")),
                    ("option_id", Json::str("allow_once")),
                ]),
            ),
            ("idempotency_key", Json::str("r-once")),
        ]),
    );
    assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
    let evs = svc.store().events(&run_id).unwrap();
    let granted = evs
        .iter()
        .find(|e| e.class == "security.permission.granted")
        .expect("allow_once minted the approval-basis handle row");
    assert_eq!(
        granted.payload.get("origin_basis").and_then(Json::as_str),
        Some("approval")
    );
    assert_eq!(
        granted.payload.get("scope").and_then(Json::as_str),
        Some("once")
    );
    assert_eq!(
        granted.payload.get("basis_ref").and_then(Json::as_str),
        Some(pid.as_str())
    );
    // The batch ordering — `decided` precedes `granted` (the decision's
    // artifact) and no `lease.granted` exists for `allow_once`.
    let decided_seq = evs
        .iter()
        .find(|e| e.class == "security.permission.decided")
        .map(|e| e.seq)
        .unwrap();
    assert!(granted.seq > decided_seq);
    assert!(
        !evs.iter()
            .any(|e| e.class == "security.permission.lease.granted"),
        "allow_once mints no lease"
    );
}

/// `open_session` mints `policy_rule`-basis handles for every sealed
/// `pre_authorize` HarnessRule — the `security.permission.granted` rows the
/// `pre_authorized` leg and the chain's `policy_rule` stage read.
#[test]
fn w_open_session_mints_preauthorization_handles() {
    let mut svc = service();
    hello(&mut svc);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec_with_doc(document_json_preauth(), None)),
            ("idempotency_key", Json::str("k-preauth")),
        ]),
    ));
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let evs = svc.store().events(&run_id).unwrap();
    let rows: Vec<_> = evs
        .iter()
        .filter(|e| e.class == "security.permission.granted")
        .collect();
    assert_eq!(rows.len(), 1, "one pre_authorize rule → one granted row");
    let p = &rows[0].payload;
    assert_eq!(
        p.get("origin_basis").and_then(Json::as_str),
        Some("policy_rule")
    );
    assert_eq!(
        p.get("basis_ref").and_then(Json::as_str),
        Some("test:rule.preauth")
    );
    assert_eq!(p.get("delegable"), Some(&Json::Bool(false)));
    assert_eq!(
        p.get("ceiling").and_then(Json::as_str),
        Some("definition"),
        "a pre-authorization mints at `definition` — §5g.1's row reads \"issued at `definition`; bounded by `authority_cap`\" (ADR-0053 D5)"
    );
    assert_eq!(
        p.get("validity")
            .and_then(|v| v.get("expires_at"))
            .and_then(|e| e.get("run_id"))
            .and_then(Json::as_str),
        Some(run_id.as_str()),
        "a pre-authorization never outlives the run"
    );
    // The grant material round-trips through the canonical codec.
    let handle = hh_monitor::events::handle_from_granted_payload(p, &rows[0].event_id)
        .expect("the granted row decodes");
    assert_eq!(handle.grants.len(), 1);
    assert_eq!(
        handle.grants[0].effect.domain,
        hh_hir::kinds::EffectDomain::FsRead
    );
}

#[test]
fn w_experimental_and_capability_gates() {
    let mut svc = service();
    // No experimental opt-in, no host-executor/permission services.
    let r = call(&mut svc, "hello", hello_params(caps_json(&[])));
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // respond_permission requires serves_permission_channel.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "respond_permission",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("permission_id", Json::str("p")),
                ("outcome", Json::obj(vec![("kind", Json::str("cancelled"))])),
                ("idempotency_key", Json::str("k")),
            ]),
        )),
        "CapabilityNotDeclared"
    );
    // fork/respond_elicitation/report_host_effect/list_leases are
    // experimental — the experimental gate precedes the capability gate.
    for (m, p) in [
        (
            "fork",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                (
                    "at",
                    Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
                ),
            ]),
        ),
        (
            "respond_elicitation",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("elicitation_id", Json::str("e")),
                ("outcome", Json::obj(vec![("kind", Json::str("cancelled"))])),
            ]),
        ),
        (
            "report_host_effect",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("effect_id", Json::str("e")),
                ("attempt_no", Json::Int(1)),
                (
                    "outcome",
                    Json::obj(vec![
                        ("kind", Json::str("refused")),
                        ("reason", Json::str("x")),
                    ]),
                ),
            ]),
        ),
        (
            "list_leases",
            Json::obj(vec![("session_id", Json::str(id.clone()))]),
        ),
        (
            "navigate",
            Json::obj(vec![("session_id", Json::str(id.clone()))]),
        ),
    ] {
        assert_eq!(
            err_kind(&call(&mut svc, m, p)),
            "ExperimentalRequired",
            "{m}"
        );
    }
}

#[test]
fn w_experimental_ops_under_opt_in() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[
            ("experimental", true),
            ("serves_host_executor", true),
            ("accepts_ephemeral_frames", true),
        ])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    // fork at seq 0 → a new Session on a child run bound forked_from.
    let f = ok(&call(
        &mut svc,
        "fork",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "at",
                Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
        ]),
    ));
    let child_id = f
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let child_run = f.get("run_id").and_then(Json::as_str).unwrap().to_string();
    assert_ne!(child_run, run_id);
    assert_ne!(child_id, id);
    // The child's lineage binds the parent (forked_from edge).
    let l = ok(&call(
        &mut svc,
        "lineage",
        Json::obj(vec![("session_id", Json::str(child_id.clone()))]),
    ));
    let links = l
        .get("events")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap();
    assert!(links.len() >= 2, "{l:?}");
    // report_host_effect for an unknown effect id → UnknownEffect.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "report_host_effect",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("effect_id", Json::str("eff_missing")),
                ("attempt_no", Json::Int(1)),
                (
                    "outcome",
                    Json::obj(vec![
                        ("kind", Json::str("refused")),
                        ("reason", Json::str("no")),
                    ]),
                ),
            ]),
        )),
        "UnknownEffect"
    );
    // respond_elicitation — no elicitation is open at Stage 1 → Refused.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "respond_elicitation",
            Json::obj(vec![
                ("session_id", Json::str(id)),
                ("elicitation_id", Json::str("el_1")),
                ("outcome", Json::obj(vec![("kind", Json::str("cancelled"))])),
            ]),
        )),
        "Refused"
    );
    // navigate — S2.9 HEAD navigation: to "root" rewinds logical HEAD to the
    // genesis sentinel (the WAL is untouched); a missing `to` is a schema
    // violation, not a refusal.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "navigate",
            Json::obj(vec![("session_id", Json::str(child_id.clone()))]),
        )),
        "SchemaViolation"
    );
    let n = ok(&call(
        &mut svc,
        "navigate",
        Json::obj(vec![
            ("session_id", Json::str(child_id.clone())),
            ("to", Json::obj(vec![("kind", Json::str("root"))])),
        ]),
    ));
    assert_eq!(
        n.get("head_moved").and_then(|v| match v {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(true)
    );
    // list_leases → honestly empty at Stage 1.
    let r = call(
        &mut svc,
        "list_leases",
        Json::obj(vec![("session_id", Json::str(child_id))]),
    );
    assert_eq!(
        r.get("result").and_then(|x| x.get("leases")),
        Some(&Json::Arr(vec![]))
    );
}

// ── Group R — read ops ──────────────────────────────────────────────────────

#[test]
fn r_read_head_project_account_describe_lineage_artifact() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    assert!(submit(&mut svc, &id, text_input("hi"))
        .get("result")
        .is_some());

    // head — the run's coordinate.
    let h = ok(&call(
        &mut svc,
        "head",
        Json::obj(vec![("session_id", Json::str(id.clone()))]),
    ));
    assert!(h.get("seq").and_then(Json::as_int).unwrap() >= 1);
    assert!(h
        .get("hash")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));

    // read — a durable page; `now` is subscribe-only → SchemaViolation.
    let page = ok(&call(
        &mut svc,
        "read",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "cursor",
                Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("limit", Json::Int(64)),
        ]),
    ));
    assert!(!page
        .get("events")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap()
        .is_empty());
    let e = call(
        &mut svc,
        "read",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("cursor", Json::obj(vec![("kind", Json::str("now"))])),
            ("limit", Json::Int(1)),
        ]),
    );
    assert_eq!(err_kind(&e), "SchemaViolation");

    // project — run_summary + context_view; a bad kind → SchemaViolation.
    for kind in ["run_summary", "context_view", "checkpoint"] {
        let v = ok(&call(
            &mut svc,
            "project",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("view_kind", Json::str(kind)),
            ]),
        ));
        assert!(
            v.get("view_hash").and_then(Json::as_str).is_some(),
            "{kind}"
        );
        assert!(v.get("payload").is_some());
    }
    let e = call(
        &mut svc,
        "project",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("view_kind", Json::str("bogus")),
        ]),
    );
    assert_eq!(err_kind(&e), "SchemaViolation");

    // account — the honest Stage-1 projection (ref + watermark, no usage).
    let a = ok(&call(
        &mut svc,
        "account",
        Json::obj(vec![("session_id", Json::str(id.clone()))]),
    ));
    assert!(a.get("account").and_then(|x| x.get("run_id")).is_some());

    // describe — manifest ref + realized settings + environment health.
    let d = ok(&call(
        &mut svc,
        "describe",
        Json::obj(vec![("session_id", Json::str(id.clone()))]),
    ));
    assert_eq!(
        d.get("manifest_ref"),
        s.get("manifest_ref"),
        "describe reports the open-time manifest ref (I4)"
    );
    assert!(d.get("realized").is_some());
    // `DescribeResult.environment` is the nested `EnvDescribe` record —
    // `{connection_info, health, meters}` (schema `EnvDescribe`).
    let env = d.get("environment").expect("EnvDescribe");
    assert!(env.get("health").and_then(Json::as_str).is_some());

    // lineage — the root run's chain is itself.
    let l = ok(&call(
        &mut svc,
        "lineage",
        Json::obj(vec![("session_id", Json::str(id.clone()))]),
    ));
    let links = l
        .get("events")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].get("run_id"), Some(&Json::str(run_id)));

    // get_artifact — the sealed definition blob landed at open (the
    // manifest_ref is a sha256 id); a malformed address → SchemaViolation;
    // a missing one → the ledger's typed not-found lowers to Refused.
    let manifest = s.get("manifest_ref").and_then(Json::as_str).unwrap();
    let g = ok(&call(
        &mut svc,
        "get_artifact",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("address", Json::str(manifest)),
        ]),
    ));
    assert!(g.get("size").and_then(Json::as_int).unwrap() > 0);
    assert!(g.get("content").and_then(Json::as_str).is_some());
    let e = call(
        &mut svc,
        "get_artifact",
        Json::obj(vec![
            ("session_id", Json::str(id)),
            ("address", Json::str("not-an-address")),
        ]),
    );
    assert_eq!(err_kind(&e), "SchemaViolation");
}

// ── injection refusals at the schema ────────────────────────────────────────

#[test]
fn inject_handle_keys_secrets_and_override_widening() {
    let mut svc = service();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // A runtime-handle member name anywhere in the params → SchemaViolation.
    let e = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![("env_handle_id", Json::str("env_1"))])]),
            ),
            ("idempotency_key", Json::str("k")),
        ]),
    );
    assert_eq!(err_kind(&e), "SchemaViolation");
    // A secret byte-pattern in a payload → SecretInPayload (never echoed).
    let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSk\n-----END PRIVATE KEY-----";
    let e = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id)),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("text")),
                    ("text", Json::str(pem)),
                ])]),
            ),
            ("idempotency_key", Json::str("k2")),
        ]),
    );
    let kind = err_kind(&e);
    assert!(
        kind == "SecretInPayload" || kind == "SchemaViolation",
        "{kind}"
    );
    let data = e.get("error").and_then(|x| x.get("data")).unwrap();
    assert!(
        !data.to_canonical_string().contains("MIIE"),
        "never echo secret bytes"
    );
    // Override widening — a pointer outside the declared coordinate
    // prefixes → AuthorityViolation{layer:"override"}.
    let mut spec = new_spec(None);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "overrides".into(),
            Json::Arr(vec![Json::obj(vec![
                ("pointer", Json::str("/permissions")),
                ("value", Json::str("allow_everything")),
            ])]),
        );
    }
    let e = call(
        &mut svc,
        "open_session",
        Json::obj(vec![("spec", spec), ("idempotency_key", Json::str("k"))]),
    );
    let kind = err_kind(&e);
    assert_eq!(kind, "AuthorityViolation", "{kind}");
    let layer = e
        .get("error")
        .and_then(|x| x.get("data"))
        .and_then(|d| d.get("layer"))
        .and_then(Json::as_str);
    assert_eq!(layer, Some("override"));
}

// ── binding parity: (a) handle() ≡ (b) stdio ────────────────────────────────

#[test]
fn binding_a_and_b_are_byte_identical() {
    // The same request tape through handle() and through the stdio codec
    // yields identical response bytes — the bindings share the one dispatch.
    let tape = [
        r#"{"jsonrpc":"2.0","id":"1","method":"hello","params":{"contract_major":1,"client":{"name":"t","version":"1","kind":"test"},"capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","id":"2","method":"describe","params":{"session_id":"sess_x"}}"#,
        r#"{"jsonrpc":"2.0","id":"3","method":"no_such_verb","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":"4","method":"close","params":{"session_id":"sess_x","reason":"done"}}"#,
    ];
    // Binding (b): the stdio loop over memory buffers.
    let dir_b = test_dir("bind-b");
    let mut out_b = Vec::new();
    {
        let mut svc = service_in(&dir_b);
        let input = tape.join("\n") + "\n";
        let mut r = std::io::BufReader::new(input.as_bytes());
        hh_embed::stdio::serve(&mut svc, &mut r, &mut out_b).unwrap();
    }
    // Binding (a): the same tape, one handle() per line.
    let dir_a = test_dir("bind-a");
    let mut out_a = Vec::new();
    {
        let mut svc = service_in(&dir_a);
        for line in &tape {
            let req = hh_wire::jsonrpc::parse_request(line).unwrap();
            let resp = svc.handle(&req);
            out_a.extend_from_slice(resp.to_canonical_string().as_bytes());
            out_a.push(b'\n');
            for n in svc.drain_notifications() {
                out_a.extend_from_slice(n.to_canonical_string().as_bytes());
                out_a.push(b'\n');
            }
        }
    }
    let a = String::from_utf8(out_a).unwrap();
    let b = String::from_utf8(out_b).unwrap();
    assert_eq!(a, b, "binding (a) and binding (b) disagree:\n{a}\n{b}");
    // And the tape produced four responses (one per request line).
    assert_eq!(b.lines().count(), 4);
}

#[test]
fn binding_b_framing_failures_are_per_line_typed_errors() {
    let dir = test_dir("framing");
    let mut svc = service_in(&dir);
    let input = concat!(
        "not json\n",
        "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"hello\",\"params\":{\"contract_major\":1,\"client\":{\"name\":\"t\",\"version\":\"1\",\"kind\":\"test\"},\"capabilities\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":\"2\",\"method\":\"close\",\"params\":\"oops\"}\n",
    );
    let mut r = std::io::BufReader::new(input.as_bytes());
    let mut out = Vec::new();
    hh_embed::stdio::serve(&mut svc, &mut r, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    // Line 1 — a framing failure lowers to the closed sum (typed, never a
    // transport-shaped error).
    let j = parse(lines[0]).unwrap();
    assert!(j.get("error").is_some());
    // Line 2 — a real HelloResult.
    let j = parse(lines[1]).unwrap();
    assert!(j.get("result").is_some(), "{lines:?}");
    // Line 3 — strict-decode failure (params not an object) → typed error.
    let j = parse(lines[2]).unwrap();
    assert!(j.get("error").is_some());
}

// ── verb parity: every declared op routes through the table ─────────────────

#[test]
fn every_declared_call_op_routes() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[
            ("experimental", true),
            ("serves_host_executor", true),
            ("serves_permission_channel", true),
            ("serves_hook_observer", true),
            ("serves_elicitation", true),
            ("serves_measurement", true),
            ("serves_principal_channel", true),
        ])),
    );
    assert!(r.get("result").is_some());
    // Every Call-direction op is dispatched by name — none falls through to
    // `unknown_method` (params decode failures are fine; they prove routing).
    for op in ops::registry() {
        if op.direction != Direction::Call {
            continue;
        }
        let e = call(&mut svc, op.name, Json::Obj(BTreeMap::new()));
        // A `result` is fine — an implemented op whose empty params are valid
        // (e.g. `lab.registry.query`/`catalog`/`verify`, S2.12) proves routing
        // by answering. Otherwise the error must not be `unknown_method`.
        if e.get("error").is_none() {
            assert!(
                e.get("result").is_some(),
                "op {} returned neither result nor error",
                op.name
            );
            continue;
        }
        let kind = err_kind(&e);
        assert_ne!(
            kind.as_str(),
            "unknown_method_marker",
            "op {} unrouted",
            op.name
        );
        let data = e.get("error").and_then(|x| x.get("data")).unwrap();
        let is_unknown_method = data
            .get("code")
            .and_then(Json::as_str)
            .map(|c| c == "unknown_method")
            .unwrap_or(false);
        assert!(
            !is_unknown_method,
            "op {} fell through to unknown_method",
            op.name
        );
    }
    // And every Upcall-direction name refuses as a request.
    for op in ops::registry() {
        if op.direction == Direction::Upcall {
            let e = call(&mut svc, op.name, Json::Obj(BTreeMap::new()));
            assert_eq!(err_kind(&e), "SchemaViolation", "{}", op.name);
        }
    }
}

#[test]
fn declared_error_subsets_are_members_of_the_closed_sum() {
    // The op table's `errors` entries name real `EmbedError` variants —
    // verified structurally: each declared tag is constructible kind().
    for op in ops::registry() {
        for tag in op.errors {
            assert!(
                !tag.is_empty(),
                "op {} declares an empty error tag",
                op.name
            );
        }
    }
}

#[test]
fn unused_warning_silencers() {
    // Referenced-but-optional imports kept honest.
    let _ = format!("{:?}", EmbedError::Disconnected);
    let _ = format!(
        "{:?}",
        ClientDescriptor {
            name: "n".into(),
            version: "v".into(),
            kind: "test".into(),
        }
    );
    let _ = HostCapabilities::default();
}

// ── S2.9 — the branch-model ops (R-2.2.4⁰ᵃ; ADR-0271) ───────────────────────

#[test]
fn s2_9_coherent_fork_points_and_shared_live_refusal() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // coherent_fork_points — the projection over the live run.
    let p = ok(&call(
        &mut svc,
        "coherent_fork_points",
        Json::obj(vec![("session_id", Json::str(id.clone()))]),
    ));
    let points = p
        .get("points")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no points: {p:?}"));
    assert!(!points.is_empty(), "seq 0 is always coherent");
    // `env: shared_live` — a mutable live env is never shared (typed refusal).
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "fork",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                (
                    "at",
                    Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
                ),
                ("env", Json::str("shared_live")),
            ]),
        )),
        "Refused"
    );
}

#[test]
fn s2_9_trace_only_fork_is_read_only_by_construction() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let f = ok(&call(
        &mut svc,
        "fork",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "at",
                Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("env", Json::str("trace_only")),
        ]),
    ));
    // The branch record rides the result — `env: trace_only`, read-only.
    let rec = f.get("branch_record").unwrap();
    assert_eq!(rec.get("env").and_then(Json::as_str), Some("trace_only"));
    assert_eq!(
        rec.get("read_only"),
        Some(&Json::Bool(true)),
        "trace_only forces read_only: {rec:?}"
    );
    // The child session is observation-only — a mutation op refuses.
    let child_id = f
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "navigate",
            Json::obj(vec![
                ("session_id", Json::str(child_id.clone())),
                ("to", Json::obj(vec![("kind", Json::str("root"))])),
            ]),
        )),
        "Refused",
        "navigate on a read-only session must refuse"
    );
}

#[test]
fn s2_9_navigate_streams_a_rewind_frame_and_rollback_appends() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[
            ("experimental", true),
            ("accepts_ephemeral_frames", true),
        ])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    // Subscribe first — the rewind frame must reach live subscribers.
    let ticket = ok(&call(
        &mut svc,
        "stream_events",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("from", Json::obj(vec![("kind", Json::str("now"))])),
        ]),
    ));
    let sub_id = ticket
        .get("subscription_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let _ = svc.poll_frames(&sub_id); // drain the sync frame
                                      // navigate to a durable seq — head.moved lands, Rewind streams.
    let n = ok(&call(
        &mut svc,
        "navigate",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "to",
                Json::obj(vec![("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("reason", Json::str("test rewind")),
        ]),
    ));
    assert_eq!(n.get("head_moved"), Some(&Json::Bool(true)));
    let frames = svc.poll_frames(&sub_id);
    let mut saw_rewind = false;
    for f in &frames {
        // `stream.frame` notifications — the frame rides `params.frame`.
        let fr = f.get("params").and_then(|p| p.get("frame")).unwrap_or(f);
        if fr.get("kind").and_then(Json::as_str) == Some("rewind") {
            assert_eq!(fr.get("to_seq").and_then(Json::as_int), Some(0));
            assert_eq!(
                fr.get("durability").and_then(Json::as_str),
                Some("ephemeral")
            );
            saw_rewind = true;
        }
    }
    assert!(saw_rewind, "no rewind frame: {frames:?}");
    // Navigate back to the tip so `submit` can drive the run — then rollback
    // lands `rolled_back` + `head.moved` and returns the rewind note.
    let tip = svc.store().envelopes(&run_id).unwrap().last().unwrap().seq;
    let _ = ok(&call(
        &mut svc,
        "navigate",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "to",
                Json::obj(vec![
                    ("kind", Json::str("seq")),
                    ("seq", Json::Int(tip as i64)),
                ]),
            ),
        ]),
    ));
    let rb = ok(&call(
        &mut svc,
        "rollback",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("to_seq", Json::Int(0)),
            ("reason", Json::str("operator")),
        ]),
    ));
    assert_eq!(
        rb.get("kind").and_then(Json::as_str),
        Some("rollback.record"),
        "the rewind note is the rollback result: {rb:?}"
    );
    assert_eq!(rb.get("head_seq").and_then(Json::as_int), Some(0));
    // The audit pair is durable on the run.
    let evs = svc.store().envelopes(&run_id).unwrap();
    assert!(evs.iter().any(|e| e.class == "lifecycle.run.rolled_back"));
    assert!(evs.iter().any(|e| e.class == "lifecycle.head.moved"));
}

// ── S2.11 — steerable boundary + pending-timeout sweep ──────────────────────

/// A spec fragment: `interactive` attendance + a `model_calls` ceiling of 1
/// — the second model call exhausts the budget and, under `interactive`,
/// escalates → the loop parks mid-turn (`turn_active`), which is exactly
/// where `steer`/`concurrent_input` matter.
fn interactive_spec_with_doc(doc: Json, supplies: Option<Json>) -> Json {
    let mut spec = new_spec_with_doc(doc, supplies);
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "attendance".into(),
            AttendanceDeclaration {
                value: "interactive".into(),
                source: "declared".into(),
            }
            .to_json(),
        );
        m.insert(
            "budget".into(),
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

/// `open_session` on a definition binding `control_strategy →
/// hh/react-steerable` arms `react/steerable` — `steer` is honoured at the
/// declared `interrupt_at_decision_point` mode and ledgered (the artefact
/// delivery row + the `propose{steer_ref}` decision), never a UI-side note
/// (AC-R-2.6.1-10; CF-371).
#[test]
fn w_steerable_boundary_steers_and_ledgers() {
    let mut svc = service();
    hello(&mut svc);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                interactive_spec_with_doc(document_json_with("hh/react-steerable"), None),
            ),
            ("idempotency_key", Json::str("open-steer")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    // Park mid-turn: call 1 completes nothing (the parked invoke path) —
    // actually the first call's `hh.submit` finishes the run at call 1, so
    // steer must arrive *while the turn is still live*: submit an input
    // whose staged call is NOT `hh.submit`. With `model_calls:1`, the
    // second call exhausts → escalate → the loop parks mid-turn.
    let r = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("host.exec.shell")),
                ])]),
            ),
            ("idempotency_key", Json::str("invoke-1")),
        ]),
    );
    assert!(
        r.get("result").is_some(),
        "invoke submit: {}",
        r.to_canonical_string()
    );

    let decisions_before = svc
        .store()
        .envelopes(&run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.decision")
        .count();
    // `steer` under `interrupt_at_decision_point` → Accepted{queued_at}.
    let st = call(
        &mut svc,
        "steer",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("input", Json::Arr(text_input("steer the loop"))),
        ]),
    );
    let st = ok(&st);
    assert_eq!(
        st.get("queued_at").and_then(Json::as_str),
        Some("decision_point"),
        "steer result: {st:?}"
    );

    // Ledgered, not UI-side: the input artefact delivery lands, and the
    // loop answers the steer cue at the decision point — under the
    // exhausted budget the honest answer is the re-escalation, so a new
    // `control.decision` row must exist beyond the pre-steer prefix.
    let evs = svc.store().envelopes(&run_id).unwrap();
    assert!(
        evs.iter().any(|e| e.class == "context.artefact.delivered"),
        "the steer input's delivery row"
    );
    let decisions_after = evs.iter().filter(|e| e.class == "control.decision").count();
    assert!(
        decisions_after > decisions_before,
        "the loop answered the steer at a decision point ({decisions_before} → {decisions_after})"
    );
    // And the same boundary's `steer` is TurnMismatch-checked first.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "steer",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("expected_turn_id", Json::str("turn-99")),
            ]),
        )),
        "TurnMismatch"
    );
}

/// `submit` during an active turn under `concurrent_input = steer` is
/// admitted as a `steer` cue — `TurnActive` is the `queue_only` answer
/// only (AC-R-2.6.1-10).
#[test]
fn w_steerable_submit_mid_turn_admitted_as_steer() {
    let mut svc = service();
    hello(&mut svc);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                interactive_spec_with_doc(document_json_with("hh/react-steerable"), None),
            ),
            ("idempotency_key", Json::str("open-midturn")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // Park mid-turn on the escalated budget exhaustion.
    let r = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("host.exec.shell")),
                ])]),
            ),
            ("idempotency_key", Json::str("invoke-1")),
        ]),
    );
    assert!(
        r.get("result").is_some(),
        "invoke submit: {}",
        r.to_canonical_string()
    );
    // `queue_only` would answer TurnActive; the `steer` declaration admits it.
    let mid = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id)),
            ("input", Json::Arr(text_input("mid-turn input"))),
            ("idempotency_key", Json::str("mid-1")),
        ]),
    );
    assert!(
        mid.get("result").is_some(),
        "mid-turn submit under concurrent_input=steer: {}",
        mid.to_canonical_string()
    );
}

/// The pending-timeout sweep (§5g recovery): a `security.permission.pending`
/// past its declared `timeout` resolves `decided{decision: timed_out,
/// decider: kernel}` — the kind-fixed terminal refusal, never `unknown` —
/// and a later `respond_permission` finds it already decided.
#[test]
fn w_permission_pending_times_out() {
    let root = test_dir("timeout");
    let clock = hh_ledger::ids::ManualClock::at(0);
    let mut svc = EmbedService::open_with(
        ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "conformance".into(),
        },
        Box::new(clock.clone()),
        None,
    )
    .unwrap();
    hello(&mut svc);
    let supplies = Json::obj(vec![(
        "host_capabilities",
        Json::Arr(vec![Json::obj(vec![
            ("capability_id", Json::str("cap:shell")),
            ("surface_id", Json::str("host.exec.shell")),
            ("requires_approval", Json::Bool(true)),
            ("timeout", Json::Int(5)),
        ])]),
    )]);
    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            ("spec", new_spec_with_doc(document_json(), Some(supplies))),
            ("idempotency_key", Json::str("open-timeout")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let permission_id = svc
        .store()
        .envelopes(&run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.permission.pending")
        .and_then(|e| e.payload.get("permission_id").and_then(Json::as_str))
        .expect("the capability ask's pending row")
        .to_string();

    // Past the declared deadline, the next drive sweeps the ask.
    clock.advance(10);
    let r = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("input", Json::Arr(text_input("go"))),
            ("idempotency_key", Json::str("s-1")),
        ]),
    );
    assert!(r.get("result").is_some(), "{}", r.to_canonical_string());
    let decided = svc
        .store()
        .envelopes(&run_id)
        .unwrap()
        .iter()
        .find(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str)
                    == Some(permission_id.as_str())
        })
        .expect("the timed-out decided row");
    assert_eq!(
        decided.payload.get("decision").and_then(Json::as_str),
        Some("timed_out")
    );
    assert_eq!(
        decided.payload.get("decider").and_then(Json::as_str),
        Some("kernel")
    );
    // A late answer finds the ask already decided (idempotent terminal).
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "respond_permission",
            Json::obj(vec![
                ("session_id", Json::str(id)),
                ("permission_id", Json::str(permission_id)),
                (
                    "outcome",
                    Json::obj(vec![
                        ("kind", Json::str("selected")),
                        ("option_id", Json::str("allow_once")),
                    ]),
                ),
                ("idempotency_key", Json::str("late")),
            ]),
        )),
        "AlreadyDecided"
    );
}

// ── Group L — `lab.registry.*` boundary round-trip (S2.12) ──────────────────

/// A minimal `class` record body (canonical spelling via `body_json`).
fn lab_class_body(class_id: &str) -> Json {
    use hh_registry::kinds::Cardinality;
    use hh_registry::records::{ClassRecord, ContractOperation, RegistryRecord};
    use std::collections::BTreeSet;
    hh_registry::schema::body_json(
        &RegistryRecord::Class(ClassRecord {
            class_id: class_id.to_string(),
            contract: vec![ContractOperation {
                name: "run".to_string(),
                inputs: Json::Null,
                outputs: Json::Null,
                invariants: vec![],
                failure_modes: vec![],
            }],
            cardinality: Cardinality::ExactlyOne,
            required_inputs: BTreeSet::from([
                "ModelProfile".to_string(),
                "ResourceAccount".to_string(),
            ]),
            base_param_schema: BTreeMap::new(),
            hot_path: false,
            dialect_introduced: "registry/1".to_string(),
            contract_version: "1.0".to_string(),
            home: "kernel".to_string(),
            declaration_schema: Json::obj([("additionalProperties", Json::Bool(false))]),
            conformance_suite_ref: None,
            decision_points: vec![],
            metrics_declared: vec![],
            slot_key: class_id.to_string(),
            tier: "C0".to_string(),
            depends_on: Vec::new(),
        }),
        false,
    )
}

/// A minimal `variant` record body pinned to `class_ref` (S4.1 export path).
fn lab_variant_body(class_ref: &str, variant_id: &str) -> Json {
    use hh_registry::kinds::Placement;
    use hh_registry::records::{AppliesTo, Implementation, RegistryRecord, VariantRecord};
    use std::collections::{BTreeMap, BTreeSet};
    hh_registry::schema::body_json(
        &RegistryRecord::Variant(VariantRecord {
            variant_id: variant_id.to_string(),
            class_ref: class_ref.to_string(),
            contract_range: "1.0".to_string(),
            version_label: Some("1.0.0".to_string()),
            param_schema: BTreeMap::new(),
            implementation: Implementation {
                content: hh_identity::idp::address(b"lab-variant-impl", "application/octet-stream"),
                placement: Placement::SubprocessConfined,
                host_requirements: Json::Null,
            },
            // `lab_class` declares `additionalProperties: false` — the
            // capability map must stay empty.
            capability_declaration: BTreeMap::new(),
            conditioned_rules: vec![],
            applies_to: AppliesTo {
                participant_classes: BTreeSet::from(["native".to_string()]),
                families: vec![],
            },
            declared_costs: None,
            summary: hh_hir::leaves::Text::new(
                "lab variant",
                "test",
                ProvenanceRecord::kernel("hh-embed", 0),
            ),
            dialect_range: "registry/1".to_string(),
        }),
        false,
    )
}

fn lab_hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        hello_params(caps_json(&[
            ("experimental", true),
            ("serves_measurement", true),
        ])),
    );
    assert!(r.get("result").is_some(), "hello refused: {r:?}");
}

fn registrar() -> Json {
    ProvenanceRecord::kernel("hh-embed", 0).to_json()
}

#[test]
fn lab_registry_register_publish_resolve_revoke_round_trip() {
    let mut svc = service();
    lab_hello(&mut svc);

    // register
    let r = call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("class")),
            ("body", lab_class_body("lab_class")),
            ("registrar", registrar()),
        ]),
    );
    let vref = ok(&r);
    let version_id = vref
        .get("version_id")
        .and_then(Json::as_str)
        .expect("register returns version_id")
        .to_string();

    // publish under a name
    let r = call(
        &mut svc,
        "lab.registry.publish",
        Json::obj([
            ("namespace", Json::str("hh")),
            ("name", Json::str("lab_class")),
            ("version_id", Json::str(version_id.clone())),
            ("registrar", registrar()),
        ]),
    );
    ok(&r);

    // resolve by name under execute
    let r = call(
        &mut svc,
        "lab.registry.resolve",
        Json::obj([
            ("namespace", Json::str("hh")),
            ("name", Json::str("lab_class")),
        ]),
    );
    let resolved = ok(&r);
    assert_eq!(
        resolved
            .get("versioned_ref")
            .and_then(|v| v.get("version_id"))
            .and_then(Json::as_str),
        Some(version_id.as_str())
    );

    // revoke — subsequent execute-mode resolution must refuse
    let r = call(
        &mut svc,
        "lab.registry.revoke",
        Json::obj([
            ("version_id", Json::str(version_id.clone())),
            ("reason", Json::str("edit")),
            ("registrar", registrar()),
        ]),
    );
    ok(&r);
    let e = call(
        &mut svc,
        "lab.registry.resolve",
        Json::obj([("version_id", Json::str(version_id.clone()))]),
    );
    assert!(
        e.get("error").is_some(),
        "revoked record resolved under execute"
    );

    // lineage exposes the revocation row (append-only, never deleted)
    let r = call(
        &mut svc,
        "lab.registry.lineage",
        Json::obj([("version_id", Json::str(version_id.clone()))]),
    );
    let view = ok(&r);
    assert_eq!(
        view.get("revocations").and_then(|v| match v {
            Json::Arr(l) => Some(l.len()),
            _ => None,
        }),
        Some(1)
    );
}

#[test]
fn lab_registry_gates_still_hold() {
    let mut svc = service();
    // No hello yet — every Group L op refuses NotInitialized.
    let e = call(&mut svc, "lab.registry.query", Json::Obj(BTreeMap::new()));
    assert_eq!(err_kind(&e), "NotInitialized");
    lab_hello(&mut svc);
    // `export` is live at S4.1 — a malformed call refuses with a typed
    // boundary error (missing `/refs`, `/target`), never a stage_pending stub.
    let e = call(&mut svc, "lab.registry.export", Json::Obj(BTreeMap::new()));
    assert_eq!(err_kind(&e), "SchemaViolation");
}

/// S4.1 (§6.2 R-2.10.2; AC-R-2.10.2-{9,11}) — the live boundary halves of the
/// general `import`/`export`/`pin`/`publisher_claims` ops: a foreign
/// `acp_registry` document imports quarantined with a structured loss report;
/// a first-party variant exports a `plugin_manifest/1` document that
/// re-decodes through the manifest codec.
#[test]
fn lab_registry_foreign_import_export_pin_round_trip() {
    let mut svc = service();
    lab_hello(&mut svc);

    // register the class + a variant (first-party, kernel registrar).
    let r = call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("class")),
            ("body", lab_class_body("lab_class")),
            ("registrar", registrar()),
        ]),
    );
    let class_vid = ok(&r)
        .get("version_id")
        .and_then(Json::as_str)
        .expect("class version_id")
        .to_string();
    let r = call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("variant")),
            ("body", lab_variant_body(&class_vid, "v1")),
            ("registrar", registrar()),
        ]),
    );
    let variant_vid = ok(&r)
        .get("version_id")
        .and_then(Json::as_str)
        .expect("variant version_id")
        .to_string();

    // The general `import` — a `foreign_ref` head routes to the governed
    // importer (never the listing path). The deployment policy's
    // `allowed_foreign_systems` is empty by construction (fail-closed:
    // the operator narrows the closed vocabulary by listing what this
    // deployment reads), so the `acp_registry` head refuses typed —
    // `ForeignSystemRefused`, never a silent lift. (The admitted,
    // quarantined-import + loss-report legs are covered store-side in
    // `hh-registry/tests/s4_1.rs`.)
    let e = call(
        &mut svc,
        "lab.registry.import",
        Json::obj([
            (
                "foreign_ref",
                Json::obj([
                    ("system", Json::str("acp_registry")),
                    ("locator", Json::str("https://agents.example/a1")),
                    ("digest", Json::str("sha256:abc")),
                ]),
            ),
            (
                "document",
                Json::obj([
                    ("name", Json::str("agent-a")),
                    ("signature", Json::str("foreign-sig-blob")),
                ]),
            ),
            ("registrar", registrar()),
        ]),
    );
    assert_eq!(err_kind(&e), "Refused");
    assert!(
        e.get("error")
            .and_then(|x| x.get("data"))
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str)
            .map(|r| r.contains("acp_registry"))
            .unwrap_or(false),
        "the refusal names the refused foreign system: {e:?}"
    );

    // `pin` — the variant is `resolved` (first-party, admissible), and `pin`
    // lifts only a `quarantined` record → a typed refusal, never a no-op lift.
    let e = call(
        &mut svc,
        "lab.registry.pin",
        Json::obj([
            ("version_id", Json::str(variant_vid.clone())),
            ("registrar", registrar()),
        ]),
    );
    assert_eq!(err_kind(&e), "Refused");

    // `publisher_claims` — the review projection over the variant.
    let r = call(
        &mut svc,
        "lab.registry.publisher_claims",
        Json::obj([("version_id", Json::str(variant_vid.clone()))]),
    );
    let out = ok(&r);
    assert_eq!(
        out.get("claims").and_then(|v| match v {
            Json::Arr(l) => Some(l.len()),
            _ => None,
        }),
        Some(0),
        "no claims yet — an empty list, never a refusal"
    );

    // `export` — the variant lowers to a canonical `plugin_manifest/1`
    // document that re-decodes through `manifest_from_json`.
    let r = call(
        &mut svc,
        "lab.registry.export",
        Json::obj([
            ("refs", Json::Arr(vec![Json::str(variant_vid.clone())])),
            ("target", Json::str("plugin_manifest/1")),
        ]),
    );
    let out = ok(&r);
    let docs = match out.get("documents") {
        Some(Json::Arr(l)) => l.clone(),
        other => panic!("export documents: {other:?}"),
    };
    assert_eq!(docs.len(), 1);
    let manifest = docs[0].get("document").expect("document member").clone();
    hh_registry::extension::plugin::manifest_from_json(&manifest, "test")
        .expect("the emitted document re-decodes as plugin_manifest/1");
    // And a foreign target refuses typed.
    let e = call(
        &mut svc,
        "lab.registry.export",
        Json::obj([
            ("refs", Json::Arr(vec![Json::str(variant_vid)])),
            ("target", Json::str("oci_image/1")),
        ]),
    );
    assert_eq!(err_kind(&e), "Refused");
}

/// A `hh-mcp-listing/1` document over one wire `Tool`.
fn mcp_listing(server_ref: &str, tools: Vec<Json>) -> Json {
    Json::obj([
        ("schema", Json::str("hh-mcp-listing/1")),
        ("server_ref", Json::str(server_ref)),
        (
            "listing_hash",
            Json::str(hh_identity::idp_id(
                "mcp.listing",
                Json::Arr(tools.clone()).to_canonical_string().as_bytes(),
            )),
        ),
        (
            "tools",
            Json::Arr(
                tools
                    .into_iter()
                    .map(|t| Json::obj([("tool", t)]))
                    .collect(),
            ),
        ),
    ])
}

fn wire_tool(name: &str, desc: &str) -> Json {
    Json::obj([
        ("name", Json::str(name)),
        ("description", Json::str(desc)),
        (
            "inputSchema",
            Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
                ),
            ]),
        ),
    ])
}

/// AC-R-2.5.1-11 — the live `lab.registry.import`/`refresh` boundary:
/// a `hh-mcp-listing/1` document registers one `ToolCapabilityRecord`
/// per wire tool (`unverified`, quarantined — the `mcp_listing`
/// admission rule), publishes `local/mcp/{server_ref}/{name}` and
/// `refresh` diffs name-keyed with `surface_only | semantic`
/// classification.
#[test]
fn lab_registry_import_refresh_round_trip() {
    let mut svc = service();
    lab_hello(&mut svc);

    let listing = mcp_listing("srv:lab", vec![wire_tool("search", "v1")]);
    let r = call(
        &mut svc,
        "lab.registry.import",
        Json::obj([("listing", listing), ("registrar", registrar())]),
    );
    let out = ok(&r);
    assert_eq!(
        out.get("declared_unverified").and_then(|v| match v {
            Json::Arr(l) => l.first().and_then(Json::as_str),
            _ => None,
        }),
        Some("search"),
        "foreign tool lifts unverified"
    );
    assert_eq!(
        out.get("losses").and_then(|v| match v {
            Json::Arr(l) => Some(l.len()),
            _ => None,
        }),
        Some(3),
        "the exact AC-R-2.5.1-5 loss list"
    );
    let vid = out
        .get("refs")
        .and_then(|v| match v {
            Json::Arr(l) => l.first().cloned(),
            _ => None,
        })
        .and_then(|v| {
            v.get("version_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .expect("import returns refs");
    // The publish row is on the lineage view.
    let r = call(
        &mut svc,
        "lab.registry.lineage",
        Json::obj([("version_id", Json::str(vid.clone()))]),
    );
    let view = ok(&r);
    assert!(
        view.get("name_history").is_some(),
        "the publish row is on the lineage view"
    );

    // refresh — an identical listing is a no-op.
    let listing = mcp_listing("srv:lab", vec![wire_tool("search", "v1")]);
    let r = call(
        &mut svc,
        "lab.registry.refresh",
        Json::obj([("listing", listing), ("registrar", registrar())]),
    );
    let out = ok(&r);
    assert_eq!(
        out.get("unchanged").and_then(|v| match v {
            Json::Arr(l) => l.first().and_then(Json::as_str),
            _ => None,
        }),
        Some("search")
    );

    // refresh — a semantic change (inputSchema) supersedes as `semantic`.
    let changed = Json::obj([
        ("name", Json::str("search")),
        ("description", Json::str("v1")),
        (
            "inputSchema",
            Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("other", Json::obj([("type", Json::str("string"))]))]),
                ),
            ]),
        ),
    ]);
    let listing = mcp_listing("srv:lab", vec![changed]);
    let r = call(
        &mut svc,
        "lab.registry.refresh",
        Json::obj([("listing", listing), ("registrar", registrar())]),
    );
    let out = ok(&r);
    let sup = out
        .get("superseded")
        .and_then(|v| match v {
            Json::Arr(l) => l.first().cloned(),
            _ => None,
        })
        .expect("superseded row");
    assert_eq!(
        sup.get("classification").and_then(Json::as_str),
        Some("semantic")
    );
    assert_eq!(
        sup.get("prev_version_id").and_then(Json::as_str),
        Some(vid.as_str())
    );
}

// ── S3.1 — Groups M/L bundle ops + lab.serve (R-2.9.3⁰, R-2.11.3⁰) ─────────
//
// `kernel.bundle`/`check_completeness`/`reproduce`/`import`,
// `measurement.emit_metric` and `lab.serve` over binding (a) — the same
// dispatch binding (b) reaches over `hh-kernel serve` (AC-K4-2), which
// is what AC-R-2.11.4-10's "the Lab runs over Groups S/R/M/L" means.

/// One metric row for `emit_metric`.
fn metric_value() -> Json {
    Json::obj([
        ("metric_ref", Json::str("hh/test.metric")),
        (
            "value",
            Json::obj([("kind", Json::str("decimal")), ("value", Json::Int(7))]),
        ),
        ("applies_to", Json::str("run:test")),
        ("oracle_ref", Json::str("oracle:test")),
        ("detector", Json::str("deterministic")),
    ])
}

/// A `run` bundle, delivered to a scratch dir — the assembly +
/// `export.delivered` row + the dir on disk.
fn deliver_bundle(svc: &mut EmbedService, run_id: &str, dir: &std::path::Path) -> Json {
    let r = call(
        svc,
        "kernel.bundle",
        Json::obj([
            ("run_id", Json::str(run_id)),
            ("deliver_sink", Json::str(dir.to_string_lossy().to_string())),
        ]),
    );
    ok(&r)
}

#[test]
fn s31_emit_metric_writes_and_replays() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();

    let params = Json::obj([
        ("session_id", Json::str(session_id.clone())),
        ("metrics", Json::Arr(vec![metric_value()])),
        ("idempotency_key", Json::str("m1")),
    ]);
    let a = ok(&call(&mut svc, "measurement.emit_metric", params.clone()));
    assert_eq!(a.get("emitted"), Some(&Json::Int(1)));
    // Idempotent replay — the recorded ack, no second row.
    let b = ok(&call(&mut svc, "measurement.emit_metric", params));
    assert_eq!(a.to_canonical_string(), b.to_canonical_string());

    // The row landed on the run's ledger.
    let evs = svc.store().events(&run_id).unwrap();
    assert!(
        evs.iter().any(|e| e.class == "measurement.metric.emitted"),
        "no metric.emitted row"
    );

    // An attach session may not write.
    let att = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id)),
                ]),
            ),
            ("idempotency_key", Json::str("att")),
        ]),
    ));
    let att_id = att
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let e = call(
        &mut svc,
        "measurement.emit_metric",
        Json::obj([
            ("session_id", Json::str(att_id)),
            ("metrics", Json::Arr(vec![metric_value()])),
            ("idempotency_key", Json::str("m2")),
        ]),
    );
    assert_eq!(err_kind(&e), "Refused");
}

#[test]
fn s31_kernel_bundle_records_contract_identity_and_exports() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("bundle me"));
    close(&mut svc, &session_id);

    let dir = test_dir("bundle-out");
    let r = deliver_bundle(&mut svc, &run_id, &dir);
    let manifest = r.get("manifest").cloned().unwrap();
    assert_eq!(
        manifest.get("schema").and_then(Json::as_str),
        Some("hh-bundle/1")
    );
    assert_eq!(
        manifest.get("bundle_kind").and_then(Json::as_str),
        Some("run")
    );
    // ContractIdentity rides in `instrument.component_versions` — the
    // bundle's contract is carried, never recomputed by consumers
    // (R-3.1; AC-R-2.9.3-1's instrument leg).
    let cv = manifest
        .get("instrument")
        .and_then(|i| i.get("component_versions"))
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .expect("instrument.component_versions");
    let expected_hash = hh_embed_schema::schema_hash();
    assert!(
        cv.iter()
            .any(|c| c.get("schema_hash").and_then(Json::as_str) == Some(expected_hash.as_str())),
        "no ContractIdentity in component_versions: {cv:?}"
    );
    // The delivery row landed (`export.delivered`, §5h.5).
    assert_ne!(r.get("delivered_seq"), Some(&Json::Null));
    // The dir on disk decodes.
    let decoded = hh_bundle::codec::decode_dir(&dir).expect("delivered dir decodes");
    assert_eq!(decoded.manifest.bundle_kind, "run");
    // And the ledger carries the `bundle_assembled` row.
    let evs = svc.store().events(&run_id).unwrap();
    assert!(
        evs.iter()
            .any(|e| e.class == "measurement.experiment.bundle_assembled"),
        "no bundle_assembled row"
    );
    assert!(
        evs.iter()
            .any(|e| e.class == "measurement.export.delivered"),
        "no export.delivered row"
    );
}

#[test]
fn s31_check_completeness_and_reproduce_over_boundary() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("repro me"));
    close(&mut svc, &session_id);
    let dir = test_dir("repro");
    deliver_bundle(&mut svc, &run_id, &dir);

    // validate_bundle (S1/S2/S4/S7 staged gate) — ok over the boundary.
    let v = ok(&call(
        &mut svc,
        "kernel.check_completeness",
        Json::obj([("path", Json::str(dir.to_string_lossy().to_string()))]),
    ));
    assert_eq!(v.get("complete"), Some(&Json::Bool(true)), "{v:?}");

    // R0 reproduces: the ledger-export fold meets the recorded status.
    let r = ok(&call(
        &mut svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(dir.to_string_lossy().to_string())),
            ("level", Json::str("R0")),
        ]),
    ));
    assert!(
        r.get("bundle_id").and_then(Json::as_str).is_some(),
        "no ReproReport: {r:?}"
    );
    assert!(r.get("basis").is_some(), "no basis: {r:?}");

    // R3 over an R0-max bundle is a *reported* refusal —
    // `ReproClaimUnsupported` (the claim exceeds the bundle's own
    // `max_supported_level`, never silently promoted).
    let e = ok(&call(
        &mut svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(dir.to_string_lossy().to_string())),
            ("level", Json::str("R3")),
            ("eval_budget", Json::Int(999_999)),
        ]),
    ));
    assert_eq!(
        e.get("outcome").and_then(Json::as_str),
        Some("refused"),
        "{e:?}"
    );
    assert!(
        e.get("refusal")
            .and_then(Json::as_str)
            .unwrap_or("")
            .contains("ReproClaimUnsupported"),
        "{e:?}"
    );
}

// ── S3.12 — AC-R-2.12.1-9: bit-exact view derivation from the bundle ────────
//
// "Any view or UI surface derivable from the run ledger MUST be derivable
// bit-for-bit from the bundle's own ledger export." The fold is `idp/1`
// digests over `{run_id, watermark, view_policy_version, payload}` — decode
// the bundle's ledger pages, re-run the same view functions the live store
// projects, and the `view_hash`es must be identical; the decoded manifest's
// recomputed `version_id` equals the recorded one (CC3/CC7).

#[test]
fn s312_bit_exact_view_derivation_from_bundle_export() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("bit-exact"));
    close(&mut svc, &session_id);

    // Live-store projections — what the kernel serves.
    let live_ctx = svc
        .store()
        .project(&run_id, hh_ledger::views::ViewKind::ContextView, None)
        .unwrap();
    let live_eff = svc
        .store()
        .project(&run_id, hh_ledger::views::ViewKind::EffectLedger, None)
        .unwrap();

    let dir = test_dir("bitexact");
    let delivered = deliver_bundle(&mut svc, &run_id, &dir);
    let recorded_version = delivered
        .get("manifest")
        .and_then(|m| m.get("version_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // Decode the bundle alone — no live-store access. The ledger export's
    // pages are bundle members; `decode_page` is the one decode rule (CC7).
    let decoded = hh_bundle::codec::decode_dir(&dir).unwrap();
    let manifest = &decoded.manifest;
    let mut envelopes = Vec::new();
    let export = manifest
        .traces
        .get(&run_id)
        .expect("the subject run's ledger export is a bundle member");
    for page_addr in &export.pages {
        envelopes.extend(
            hh_bundle::export::decode_page(decoded.members.get(page_addr).unwrap()).unwrap(),
        );
    }

    // The same view functions fold the decoded envelopes to identical
    // `view_hash`es (AC-R-2.12.1-9 — bit-for-bit).
    let re_ctx = hh_ledger::views::context_view(&run_id, &envelopes, None);
    let re_eff = hh_ledger::views::effect_ledger(&run_id, &envelopes, None);
    assert_eq!(live_ctx.view_hash, re_ctx.view_hash, "ContextView diverged");
    assert_eq!(
        live_eff.view_hash, re_eff.view_hash,
        "EffectLedger diverged"
    );
    assert_eq!(
        live_ctx.payload.to_canonical_string(),
        re_ctx.payload.to_canonical_string(),
        "ContextView payload diverged"
    );

    // Bundle identity recomputes equal to the recorded `version_id`.
    assert_eq!(manifest.compute_id(), recorded_version);

    // Deterministic reproduction over the decoded bundle (S3.12: the R0/R1
    // paths must reproduce without the live store). R0 always reproduces;
    // R1 reproduces when the bundle's own `max_supported_level` reaches it,
    // else the report is a *reported* `ReproClaimUnsupported` refusal —
    // never a silent promotion.
    let max_level = delivered
        .get("manifest")
        .and_then(|m| m.get("reproducibility"))
        .and_then(|r| r.get("max_supported_level"))
        .and_then(Json::as_str)
        .unwrap_or("R0")
        .to_string();
    for level in ["R0", "R1"] {
        let r = ok(&call(
            &mut svc,
            "kernel.reproduce",
            Json::obj([
                ("path", Json::str(dir.to_string_lossy().to_string())),
                ("level", Json::str(level)),
            ]),
        ));
        let outcome = r.get("outcome").and_then(Json::as_str);
        let supported = max_level.as_str() >= level;
        if supported {
            assert_eq!(outcome, Some("pass"), "{level}: {r:?}");
        } else {
            assert_eq!(outcome, Some("refused"), "{level}: {r:?}");
            assert!(
                r.get("refusal")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .contains("ReproClaimUnsupported"),
                "{level}: {r:?}"
            );
        }
    }
}

// ── S3.12 — AC-R-2.12.1-13: `reproduce(R3, seeds[])` ─────────────────────
//
// `kernel.bundle` today emits no `configuration.budget`/model snapshots
// (the conformance run declares neither), so the bundle it delivers caps
// below R3. To exercise the R3 legs the test lifts the *delivered*
// manifest the way a bundler that captured them would — declaring
// `budget`, a fingerprinted `model` member + `snapshots[]`, and the
// compiled plan's `validators[]` — then re-seals `version_id` so the
// identity leg stays clean. The seeds gate and the report members are
// the new code under test.

#[test]
fn s312_reproduce_r3_seeds_unbudgeted_arm_distributions_validators() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("seeded repro"));
    close(&mut svc, &session_id);
    let dir = test_dir("r3-seeds");
    deliver_bundle(&mut svc, &run_id, &dir);

    // Lift to R3 capability: declared budget + one fingerprinted,
    // pinned snapshot + its `model` member + a bound validator set.
    let decoded = hh_bundle::codec::decode_dir(&dir).unwrap();
    let mut m = decoded.manifest.clone();
    let mut members = decoded.members.clone();
    let budget_doc = Json::obj([("limits", Json::obj([("calls", Json::Int(10))]))]);
    if let Json::Obj(c) = &mut m.configuration {
        c.insert("budget".into(), budget_doc.clone());
    }
    let snap = Json::obj([
        ("model_id", Json::str("model:x")),
        ("snapshot_id", Json::str("sha256:snap")),
        ("weights", Json::str("sha256:weights")),
        ("pinned", Json::Bool(true)),
        ("observed_fingerprint", Json::str("sha256:fp")),
    ]);
    let snap_bytes = snap.to_canonical_string().into_bytes();
    let snap_addr = hh_identity::address(&snap_bytes, "application/json").id();
    members.insert(snap_addr.clone(), snap_bytes.clone());
    m.members.push(hh_bundle::manifest::MemberRef::present(
        "model",
        snap_addr,
        "application/json",
        snap_bytes.len() as u64,
    ));
    m.model = Json::obj([("snapshots", Json::Arr(vec![snap]))]);
    if let Json::Obj(d) = &mut m.resolved_dependencies {
        d.insert(
            "validators".into(),
            Json::Arr(vec![Json::str("validator:pinned-v1")]),
        );
    }
    // Re-seal — the identity leg's recompute must meet the recorded id.
    m.version_id = m.compute_id();
    let r3dir = test_dir("r3-seeds-r3");
    hh_bundle::codec::encode_dir(&r3dir, &m, &members).unwrap();
    let r3path = r3dir.to_string_lossy().to_string();

    // R3 with two budgeted seed legs — per-leg distributions, the
    // validator set named, `budget_match` true (no fingerprint drift:
    // declared `observed_fingerprint` meets no contradicting member).
    let r = ok(&call(
        &mut svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(r3path.clone())),
            ("level", Json::str("R3")),
            ("eval_budget", budget_doc.clone()),
            (
                "seeds",
                Json::Arr(vec![
                    Json::obj([("seed", Json::Int(1)), ("eval_budget", budget_doc.clone())]),
                    Json::obj([("seed", Json::Int(2)), ("eval_budget", budget_doc.clone())]),
                ]),
            ),
        ]),
    ));
    assert_ne!(
        r.get("outcome").and_then(Json::as_str),
        Some("refused"),
        "R3 must run on the lifted bundle: {r:?}"
    );
    let ev = r.get("evidence").expect("evidence member");
    assert_eq!(
        ev.get("validators"),
        Some(&Json::Arr(vec![Json::str("validator:pinned-v1")])),
        "an R2+ verdict names its validator set: {r:?}"
    );
    let Json::Arr(dist) = ev.get("distributions").cloned().unwrap_or(Json::Null) else {
        panic!("distributions must be an array: {r:?}")
    };
    assert_eq!(dist.len(), 2, "one row per declared seed leg: {r:?}");
    for (row, seed) in dist.iter().zip([1i64, 2]) {
        assert_eq!(row.get("seed"), Some(&Json::Int(seed)), "{row:?}");
        assert_eq!(
            row.get("outcome").and_then(Json::as_str),
            Some("pass"),
            "the per-leg verdict, never a mean: {row:?}"
        );
        assert_eq!(row.get("drift"), Some(&Json::Int(0)), "{row:?}");
    }
    assert_eq!(ev.get("budget_match"), Some(&Json::Bool(true)), "{r:?}");

    // A seed leg the manifest cannot budget-match refuses the whole
    // reproduce — `UnbudgetedArm{seed}`; the leg never enters a
    // distribution (matched-budget-or-refuse, CC9).
    let e = ok(&call(
        &mut svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(r3path.clone())),
            ("level", Json::str("R3")),
            (
                "seeds",
                Json::Arr(vec![
                    Json::obj([("seed", Json::Int(3)), ("eval_budget", budget_doc.clone())]),
                    // No eval_budget — unbudgeted.
                    Json::obj([("seed", Json::Int(4))]),
                ]),
            ),
        ]),
    ));
    assert_eq!(
        e.get("outcome").and_then(Json::as_str),
        Some("refused"),
        "{e:?}"
    );
    assert!(
        e.get("refusal")
            .and_then(Json::as_str)
            .unwrap_or("")
            .contains("UnbudgetedArm{seed 4}"),
        "{e:?}"
    );
    assert_eq!(
        e.get("evidence").and_then(|ev| ev.get("distributions")),
        Some(&Json::Arr(vec![])),
        "the refused leg never lands a distribution row: {e:?}"
    );

    // A mismatched seed budget refuses the same way (not just a missing
    // one — equality, not presence).
    let e2 = ok(&call(
        &mut svc,
        "kernel.reproduce",
        Json::obj([
            ("path", Json::str(r3path)),
            ("level", Json::str("R3")),
            (
                "seeds",
                Json::Arr(vec![Json::obj([
                    ("seed", Json::Int(5)),
                    (
                        "eval_budget",
                        Json::obj([("limits", Json::obj([("calls", Json::Int(11))]))]),
                    ),
                ])]),
            ),
        ]),
    ));
    assert_eq!(
        e2.get("outcome").and_then(Json::as_str),
        Some("refused"),
        "{e2:?}"
    );
    assert!(
        e2.get("refusal")
            .and_then(Json::as_str)
            .unwrap_or("")
            .contains("UnbudgetedArm{seed 5}"),
        "{e2:?}"
    );
}

#[test]
fn s31_import_lifts_refs_only_with_unverified_authority() {
    let mut svc = service();
    lab_hello(&mut svc);
    let s = open_new(&mut svc);
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("import me"));
    close(&mut svc, &session_id);
    let dir = test_dir("import-src");
    deliver_bundle(&mut svc, &run_id, &dir);

    let r = ok(&call(
        &mut svc,
        "kernel.import",
        Json::obj([
            ("path", Json::str(dir.to_string_lossy().to_string())),
            ("holder", Json::str("importer")),
        ]),
    ));
    let new_run = r.get("run_id").and_then(Json::as_str).unwrap().to_string();
    assert_ne!(new_run, run_id);
    // The receipt row names the source coordinates.
    let evs = svc.store().events(&new_run).unwrap();
    assert!(
        evs.iter().any(|e| e.class == "lifecycle.run.imported"),
        "no imported row: {:?}",
        evs.iter().map(|e| &e.class).collect::<Vec<_>>()
    );
    assert!(
        evs.iter().any(|e| e.class == "lifecycle.run.created"),
        "no created row"
    );
    // The imported run is readable through the boundary (attach+read).
    let att = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(new_run)),
                ]),
            ),
            ("idempotency_key", Json::str("att-i")),
        ]),
    ));
    let att_id = att
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let page = ok(&call(
        &mut svc,
        "read",
        Json::obj(vec![
            ("session_id", Json::str(att_id)),
            (
                "cursor",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("limit", Json::Int(64)),
        ]),
    ));
    assert!(page.get("events").is_some(), "{page:?}");
}

/// A hand-authored `hh-bundle/1` dir carrying a `target:mcp` member —
/// `lab.serve`'s positive leg (the kernel-side `serve(bundle)` half:
/// decode → lower → `{artifact, stdio_launch binding, launch}`).
#[test]
fn s31_lab_serve_lowers_target_mcp_and_binds_test_principal() {
    let mut svc = service();
    lab_hello(&mut svc);

    let dir = test_dir("serve-bundle");
    let member = br#"{"schema":"hh-mcp-target/1","target":"mcp","tools":[{"name":"echo","inputSchema":{"type":"object"}}]}"#
        .to_vec();
    let addr = hh_identity::idp_id("member", &member);
    let doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("producer", Json::obj([])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![])),
                ("status", Json::str("finished")),
            ]),
        ),
        (
            "members",
            Json::Arr(vec![Json::obj([
                ("role", Json::str("target:mcp")),
                ("ref", Json::str(addr.clone())),
                ("media_type", Json::str("application/json")),
                ("size", Json::Int(member.len() as i64)),
                ("status", Json::str("present")),
            ])]),
        ),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = hh_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    manifest.version_id = manifest.compute_id();
    let mut members = hh_bundle::export::MemberBytes::new();
    members.insert(addr, member);
    hh_bundle::codec::encode_dir(&dir, &manifest, &members).unwrap();

    let r = ok(&call(
        &mut svc,
        "lab.serve",
        Json::obj([("path", Json::str(dir.to_string_lossy().to_string()))]),
    ));
    let binding = r.get("binding").expect("binding");
    assert_eq!(
        binding.get("kind").and_then(Json::as_str),
        Some("stdio_launch")
    );
    assert_eq!(
        binding.get("principal").and_then(Json::as_str),
        Some(hh_mcp::binding::TEST_PRINCIPAL),
        "the binding is fixed to the test principal (R-3)"
    );
    let artifact = r.get("artifact").expect("artifact");
    assert_eq!(
        artifact.get("schema").and_then(Json::as_str),
        Some("hh-mcp-artifact/1")
    );
    assert!(r.get("launch").is_some(), "launch descriptor: {r:?}");

    // A bundle without the member refuses typed.
    let e = call(
        &mut svc,
        "lab.serve",
        Json::obj([(
            "path",
            Json::str(test_dir("empty").to_string_lossy().to_string()),
        )]),
    );
    assert_eq!(err_kind(&e), "Refused");
}

/// Group M/L ops are capability-gated: without `serves_measurement`
/// they refuse `NotAuthorized`-class, and unimplemented Group L ops
/// still answer `stage_pending` — the boundary is honest either way.
#[test]
fn s31_group_ml_capability_gate() {
    let mut svc = service();
    // hello WITHOUT serves_measurement.
    call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    let e = call(
        &mut svc,
        "kernel.check_completeness",
        Json::obj([("path", Json::str("/nonexistent"))]),
    );
    let kind = err_kind(&e);
    assert!(
        kind == "NotAuthorized"
            || kind == "CapabilityNotDeclared"
            || kind == "CapabilityNotNegotiated"
            || kind == "Refused",
        "ungated answer: {e:?}"
    );

    let mut svc = service();
    lab_hello(&mut svc);
    // A still-pending Group L op answers `stage_pending`.
    let e = call(&mut svc, "lab.results.query_rows", Json::obj([]));
    assert_eq!(err_kind(&e), "Refused");
    assert_eq!(
        e.get("error")
            .and_then(|x| x.get("data"))
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str),
        Some("stage_pending")
    );
    // `lab.experiment.register` is live at S3.4a — a missing `spec` member is
    // a typed schema violation, and a refusal renders the closed E-1 code.
    let e = call(&mut svc, "lab.experiment.register", Json::obj([]));
    assert_eq!(err_kind(&e), "SchemaViolation", "{e:?}");
}

// ── S3.3 — `lab.eval.*` records-in/records-out (R-2.9.2/R-2.9.4⁰ᵇ) ─────────
//
// The eval ops carry `eval_run/1` rows, `task_context`/`suite_context`
// members, `Design`/`MatchSpec` records and catalogue metric names — the
// service decodes through the record codecs, never a second schema.

/// One eval_run/1 row (mirrors the hh-eval acceptance fixture).
fn eval_run_json(arm: &str, task: &str, rep: u64, pass: bool) -> Json {
    use hh_eval::runs::{CacheState, EvalRun};
    use hh_ontology::compliance::Detector;
    use hh_ontology::control::OutcomeClass;
    use hh_ontology::eval::{MetricValue, MetricValueKind};
    use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
    use hh_ontology::participant::{Observability, ParticipantClass};
    use hh_ontology::DimensionId;
    let r = EvalRun {
        run_id: format!("r-{arm}-{task}-{rep}"),
        arm_id: arm.into(),
        cell_id: Some(format!("{arm}:{task}")),
        configuration_id: format!("cfg-{arm}"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
            Observability::Ledger,
        ]
        .into_iter()
        .collect(),
        mediation: Default::default(),
        capability_vector: Default::default(),
        task_id: task.into(),
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: rep,
        attempt_no: 1,
        seed: Some(42 + rep),
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: [(DimensionId::ModelCalls, 5)].into_iter().collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(pass),
            applies_to: format!("r-{arm}-{task}-{rep}"),
            oracle_ref: "oracle/executable".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
        }],
        environment_version_id: Some("env-1".into()),
        environment_family: EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: [("default".to_string(), "snap-1".to_string())]
            .into_iter()
            .collect(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        split_hash: Some("sha256:split-1".into()),
        facts: Default::default(),
    };
    r.to_json()
}

/// The shared compare params: two arms over tasks t1/t2, paired design,
/// matched-cap budgets on `model_calls`.
fn eval_compare_params() -> Json {
    use hh_ontology::eval::{
        Design, DesignKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
    };
    use hh_ontology::DimensionId;
    let design = Design {
        id: "design-1".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 2,
        pairing: Pairing::ByTaskAndReplicate,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: true,
            seed_honoured_required: true,
        },
        held_out_split_ref: Some("split-1".into()),
        pre_registration: PreRegistration {
            registered_at: 1,
            hypothesis: "A ≥ B".into(),
            primary_metrics: vec!["task_success".into()],
            equivalence_margin: None,
            min_n: 1,
            analysis_plan_ref: "plan-1".into(),
            task_split_hash: "sha256:split-1".into(),
            interactions: vec![],
        },
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    };
    let spec = hh_budget::matchspec::MatchSpec::matched_cap(&[DimensionId::ModelCalls]).to_json();
    let caps = Json::obj([(DimensionId::ModelCalls.as_str(), Json::Int(100))]);
    let arm = Json::obj([
        ("match_spec", spec),
        ("caps", caps),
        ("native", Json::Bool(true)),
    ]);
    let runs = Json::Arr(vec![
        eval_run_json("A", "t1", 0, true),
        eval_run_json("A", "t1", 1, true),
        eval_run_json("A", "t2", 0, true),
        eval_run_json("A", "t2", 1, false),
        eval_run_json("B", "t1", 0, false),
        eval_run_json("B", "t1", 1, true),
        eval_run_json("B", "t2", 0, false),
        eval_run_json("B", "t2", 1, false),
    ]);
    let tasks = Json::Arr(
        ["t1", "t2"]
            .iter()
            .map(|t| {
                Json::obj([
                    ("task_id", Json::str(*t)),
                    ("suite_id", Json::str("suite-test")),
                    ("split_label", Json::str("held_out")),
                    ("split_hash", Json::str("sha256:split-1")),
                    ("stratum", Json::str("private_held_out")),
                ])
            })
            .collect(),
    );
    Json::obj([
        ("arm_a", Json::str("A")),
        ("arm_b", Json::str("B")),
        ("metrics", Json::Arr(vec![Json::str("task_success")])),
        ("runs", runs),
        ("tasks", tasks),
        ("design", design.to_json()),
        ("arm_specs", Json::Arr(vec![arm.clone(), arm])),
    ])
}

#[test]
fn lab_eval_catalogue_conformant() {
    let mut svc = service();
    // No hello — Group L gates hold for eval ops too.
    let e = call(&mut svc, "lab.eval.catalogue", Json::obj([]));
    assert_eq!(err_kind(&e), "NotInitialized");
    lab_hello(&mut svc);
    let r = ok(&call(&mut svc, "lab.eval.catalogue", Json::obj([])));
    assert_eq!(
        r.get("schema").and_then(Json::as_str),
        Some("catalogue_report/1")
    );
    assert_eq!(r.get("conformant"), Some(&Json::Bool(true)));
    assert!(r.get("metrics").and_then(Json::as_int).unwrap_or(0) > 0);
}

#[test]
fn lab_eval_compare_produces_reports() {
    let mut svc = service();
    lab_hello(&mut svc);
    let r = ok(&call(&mut svc, "lab.eval.compare", eval_compare_params()));
    let reports = match r.get("reports") {
        Some(Json::Arr(rs)) => rs,
        _ => panic!("compare returned no reports: {r:?}"),
    };
    assert_eq!(reports.len(), 1);
    let rep = &reports[0];
    assert_eq!(
        rep.get("metric").and_then(Json::as_str),
        Some("task_success")
    );
    assert_eq!(rep.get("arm_a").and_then(Json::as_str), Some("A"));
    // Paired effects retained per task (per_task[metric][task]).
    let per_task = match r.get("per_task") {
        Some(Json::Arr(ts)) => ts,
        _ => panic!("no per_task: {r:?}"),
    };
    assert_eq!(per_task.len(), 1);
    match &per_task[0] {
        Json::Arr(effects) => assert_eq!(effects.len(), 2),
        _ => panic!("per_task[0] not a task table"),
    }
}

#[test]
fn lab_eval_compare_typed_refusals() {
    let mut svc = service();
    lab_hello(&mut svc);
    // Unknown metric name — a typed schema violation, never a silent skip.
    let mut p = eval_compare_params();
    if let Json::Obj(m) = &mut p {
        m.insert(
            "metrics".into(),
            Json::Arr(vec![Json::str("no_such_metric")]),
        );
    }
    let e = call(&mut svc, "lab.eval.compare", p);
    assert_eq!(err_kind(&e), "SchemaViolation");
    // A non-comparable run refuses (AC-R-2.9.2-4 through the boundary).
    let mut p = eval_compare_params();
    if let Json::Obj(m) = &mut p {
        if let Some(Json::Arr(runs)) = m.get_mut("runs") {
            for r in runs.iter_mut() {
                if let Json::Obj(rm) = r {
                    rm.insert("comparable".into(), Json::Bool(false));
                }
            }
        }
    }
    let e = call(&mut svc, "lab.eval.compare", p);
    assert_eq!(err_kind(&e), "Refused");
}

#[test]
fn lab_eval_render_scorecard_records_in() {
    let mut svc = service();
    lab_hello(&mut svc);
    let mut p = eval_compare_params();
    if let Json::Obj(m) = &mut p {
        m.insert(
            "suites".into(),
            Json::Arr(vec![Json::obj([
                ("suite_id", Json::str("suite-test")),
                ("retired_for_headline", Json::Bool(false)),
                ("family", Json::str("coding_terminal")),
            ])]),
        );
        m.insert("pool_strata".into(), Json::Bool(false));
    }
    let r = ok(&call(&mut svc, "lab.eval.render_scorecard", p));
    assert_eq!(
        r.get("schema").and_then(Json::as_str),
        Some("scorecard_report/1")
    );
}

// ── S3.4c — `lab.analysis.analyze` (R-2.10.4⁰ᵇ) ────────────────────────────
//
// The estimator kernel is records-in/records-out through the same boundary
// discipline as `lab.eval.*`: the caller supplies the `AnalysisSpec`, the
// row selectors and the task/suite contexts; `spec_ref` binds the
// registered experiment spec's design/arms/pre-registration. Rows resolve
// through `ResultsStore` at `<store_root>/results`; each row's
// `RunManifest` + `LedgerFacts` project from the run's durable ledger
// prefix. The runs are seeded BEFORE the service opens its store —
// `Store::open` loads the run index once.

/// The arm row — `limits_enforced: "full"` maps to the native enforcement
/// view inside the boundary's match-spec binding.
fn s34c_arm(id: &str, level: &str, eval: &str) -> hh_lab::experiment::ArmSpec {
    hh_lab::experiment::ArmSpec {
        arm_id: id.into(),
        hypothesis: format!("{id} does better"),
        level_assignment: BTreeMap::from([("model".to_string(), level.to_string())]),
        eval_budget: eval.into(),
        // Comparative arms carry a search budget too — `UnbudgetedArm`
        // refuses the matched kinds without one.
        search_budget: Some("budget:search".into()),
        match_spec: Some(hh_budget::MatchSpec::matched_cap(&[
            hh_budget::DimensionId::ModelCalls,
        ])),
        artifact_ref: hh_ontology::config::Ref::new("artifact:x", "sha256:ee55"),
        limits_enforced: "full".into(),
        model_role_table_ref: None,
        response_cache: None,
    }
}

/// A registered-experiment fixture — the hh-lab member-level shape with a
/// `Paired` design and a `task_success` primary metric.
fn s34c_spec() -> hh_lab::experiment::ExperimentSpec {
    use hh_lab::experiment::*;
    use hh_ontology::eval::{
        Design, DesignKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
    };
    use hh_ontology::lab::SplitLabel;
    use hh_ontology::participant::ParticipantClass;
    let seed = || SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    };
    let prereg = || PreRegistration {
        registered_at: 1,
        hypothesis: "arm B beats arm A".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: "analysis:plan".into(),
        task_split_hash: "sha256:cc33".into(),
        interactions: vec![],
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "design:1".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 4,
            pairing: Pairing::ByTask,
            seed_policy: seed(),
            held_out_split_ref: None,
            pre_registration: prereg(),
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg()),
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: hh_ontology::eval::FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![
                LevelSpec {
                    level_id: "l1".into(),
                    ref_: "sha256:dd44".into(),
                    overrides: None,
                    label: "l1".into(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
                LevelSpec {
                    level_id: "l2".into(),
                    ref_: "sha256:dd55".into(),
                    overrides: None,
                    label: "l2".into(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
            ],
        }],
        arms: vec![
            s34c_arm("arm-A", "l1", "eval:a"),
            s34c_arm("arm-B", "l2", "eval:b"),
        ],
        suite: SuiteBinding {
            suite_ref: "suite:test".into(),
            split_labels_used: vec![SplitLabel::Dev],
            split_assignment_ref: Some("split:1".into()),
        },
        replicates_per_cell: 4,
        seed_policy: seed(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

/// The arm-level + experiment-level budget bodies `register{budgets{}}`
/// deposits (identical hard caps — `matched_cap` binds).
fn s34c_budgets() -> Json {
    let caps = || {
        hh_budget::spec::BudgetSpec::hard_caps(
            hh_budget::spec::BudgetMode::Pool,
            &[(
                hh_budget::DimensionKey::Primary(hh_budget::DimensionId::ModelCalls),
                100,
            )],
        )
    };
    Json::obj([
        ("eval:a", caps().to_json()),
        ("eval:b", caps().to_json()),
        ("budget:search", caps().to_json()),
        ("budget:exp", caps().to_json()),
        ("budget:inst", caps().to_json()),
    ])
}

/// A landed projection row for `run_id` (the `ResultsRow` record — the
/// `project_row` derivation is S3.4b's seam).
fn s34c_row(
    run_id: &str,
    arm: &str,
    task: &str,
    rep: u64,
    pass: bool,
) -> hh_results::row::ResultsRow {
    use hh_ontology::eval::MetricValueKind;
    use hh_results::audit::AuditRef;
    use hh_results::row::{
        AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection,
        ResultsRow, RowKey,
    };
    use hh_results::scoring::ScoringContext;
    use hh_results::watermark::WatermarkSet;
    let mut watermark = WatermarkSet::new();
    watermark.pin(run_id, 7);
    ResultsRow {
        key: RowKey {
            configuration_version_id: format!("cv-{arm}"),
            run_id: run_id.to_string(),
        },
        coordinates: Coordinates {
            configuration_id: Some(format!("cfg-{arm}")),
            configuration_version_id: format!("cv-{arm}"),
            model_snapshots: BTreeMap::from([("default".to_string(), "snap-1".to_string())]),
            harness_def_ref: None,
            model_profile_ref: None,
            environment_ref: None,
            environment_version_id: Some("env-1".into()),
            task: Some(Json::obj([
                ("task_id", Json::str(task)),
                ("suite_id", Json::str("suite-test")),
                ("split_label", Json::str("held_out")),
            ])),
            budget: Json::obj([]),
            replicate: Json::obj([("seed", Json::Int(42 + rep as i64))]),
            participant_class: "native".into(),
            observability_level: vec!["ledger".into()],
            hosting_mechanism: None,
            capability_vector_ref: None,
            mediation: vec![],
            registry_snapshot_id: None,
        },
        experiment: Some(Json::obj([
            ("experiment_run_id", Json::str("exp-run-1")),
            ("arm_id", Json::str(arm)),
            ("cell_id", Json::str(format!("{arm}:{task}"))),
            ("replicate_index", Json::Int(rep as i64)),
            ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ])),
        outcome: OutcomeSection {
            status: "finished".into(),
            outcome_class: Some("scored".into()),
            stop_reason: Some("completed".into()),
            veto_tripped: vec![],
            finished_at: Some("2026-01-01T00:00:00.000Z".into()),
            wall_ms: Some(100),
            activation_no: 1,
        },
        cells: vec![Cell {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(pass),
            detector: Some("deterministic".into()),
            oracle_ref: Some("oracle/executable".into()),
            confidence: None,
            evidence: vec![AuditRef {
                run_id: run_id.to_string(),
                seq: 6,
                hash: "idp:cell".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            }],
            computed_from: vec!["6".into()],
        }],
        consumption: ConsumptionSection {
            dimensions: BTreeMap::from([("model_calls".to_string(), 5i64)]),
            utilization: None,
            spend: None,
            instrument_spend: None,
        },
        cache: Json::obj([("policy", Json::str("cold_start"))]),
        lineage: Json::obj([]),
        annotations_from_ledger: Json::obj([]),
        audit: AuditSection {
            head: AuditRef {
                run_id: run_id.to_string(),
                seq: 7,
                hash: "idp:head".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            },
            checkpoint_ref: None,
        },
        scoring: ScoringContext {
            validator_set: vec![],
            overlay_runs: vec![],
            metric_registry_version: "test-registry".into(),
            pricing_ref: None,
            view_policy_version: "test-view".into(),
        },
        derived_from: DerivedFrom {
            run_id: run_id.to_string(),
            seq: 7,
            overlay_watermarks: BTreeMap::new(),
        },
        watermark_set: watermark,
        view_hash: "test-view-hash".into(),
        version_id: format!("v-{run_id}-{rep}"),
    }
}

/// `lab.analysis.analyze` end-to-end: register → rows → analyze → durable
// `{record, report}`; a repeat call serves the same `report_id` (KA-12 —
// deterministic identity over an identical input tuple, no recomputation).
#[test]
fn lab_analysis_analyze_round_trip() {
    let root = test_dir("s34c-analyze");
    // Seed the ledger + the derived-state store before the service opens.
    let (ra, rb);
    {
        let mut store = hh_ledger::store::Store::open(root.join("store")).unwrap();
        let manifest = || {
            let mut m =
                hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Experiment);
            m.configuration_id = None;
            m.configuration_version_id = None;
            m
        };
        ra = store.open_run(manifest(), "s34c").unwrap().0;
        rb = store.open_run(manifest(), "s34c").unwrap().0;
    }
    let mut keys = Vec::new();
    {
        let rs = hh_results::store::ResultsStore::open(root.join("store").join("results")).unwrap();
        let mut rec = |run_id: &str, arm: &str, rep: u64, pass: bool| {
            let row = s34c_row(run_id, arm, "t1", rep, pass);
            keys.push(row.key.key_id());
            rs.record(&row, hh_results::version::DerivedReason::Initial)
                .unwrap();
        };
        // A: 4/4 pass; B: 1/4 — a known positive delta.
        for rep in 0..4 {
            rec(&ra.clone(), "arm-A", rep, true);
        }
        for rep in 0..4 {
            rec(&rb.clone(), "arm-B", rep, rep == 0);
        }
    }
    let mut svc = service_in(&root);
    lab_hello(&mut svc);
    let r = ok(&call(
        &mut svc,
        "lab.experiment.register",
        Json::obj([("spec", s34c_spec().to_json()), ("budgets", s34c_budgets())]),
    ));
    let eid = r
        .get("experiment_id")
        .and_then(Json::as_str)
        .expect("experiment_id")
        .to_string();

    let analyze = |spec_ref: &str| {
        let mut s = hh_lab::analysis::AnalysisSpec {
            spec_id: String::new(),
            kind: "compare".into(),
            query: hh_lab::analysis::QuerySpec {
                metrics: vec!["task_success".into()],
                filters: Some(Json::obj([
                    ("arm_a", Json::str("arm-A")),
                    ("arm_b", Json::str("arm-B")),
                ])),
                grain: None,
            },
            spec_ref: Some(spec_ref.to_string()),
            label: None,
            estimator_selection: hh_ontology::eval::EstimatorSelection {
                method: hh_ontology::eval::IntervalMethod::ClusteredClt,
                selection_rule: "adr-0158.clt_floor".into(),
                floors: BTreeMap::new(),
                fallback_chain: vec![],
                substituted: None,
            },
            resample: None,
            outputs: vec!["report".into()],
        };
        s.spec_id = s.spec_id();
        Json::obj([
            ("spec", s.to_json()),
            (
                "rows",
                Json::Arr(keys.iter().map(|k| Json::str(k.as_str())).collect()),
            ),
            (
                "tasks",
                Json::Arr(vec![Json::obj([
                    ("task_id", Json::str("t1")),
                    ("suite_id", Json::str("suite-test")),
                    ("split_label", Json::str("held_out")),
                    ("split_hash", Json::str("sha256:split-1")),
                    ("stratum", Json::str("private_held_out")),
                ])]),
            ),
            (
                "suites",
                Json::Arr(vec![Json::obj([
                    ("suite_id", Json::str("suite-test")),
                    ("retired_for_headline", Json::Bool(false)),
                    ("family", Json::str("coding_terminal")),
                ])]),
            ),
            ("seed", Json::Int(11)),
        ])
    };

    let r = ok(&call(&mut svc, "lab.analysis.analyze", analyze(&eid)));
    // `{record, report, body}` — the envelopes plus the
    // `analysis_report_body/1` payload.
    let body = r.get("body").expect("body");
    assert_eq!(
        body.get("schema").and_then(Json::as_str),
        Some("analysis_report_body/1")
    );
    assert_eq!(body.get("kind").and_then(Json::as_str), Some("compare"));
    let report_id = body
        .get("report_id")
        .and_then(Json::as_str)
        .expect("report_id")
        .to_string();
    // The comparison landed with a positive paired effect and the A12
    // multiplicity record attached.
    let comparisons = match body.get("comparisons") {
        Some(Json::Arr(cs)) => cs,
        _ => panic!("no comparisons: {body:?}"),
    };
    assert_eq!(comparisons.len(), 1);
    assert!(comparisons[0].get("multiplicity").is_some());
    // The envelopes carry the report's identity: the AnalysisReport's
    // `report_id`, and the record's `outputs[]`.
    assert_eq!(
        r.get("report")
            .and_then(|p| p.get("report_id"))
            .and_then(Json::as_str),
        Some(report_id.as_str())
    );
    match r.get("record").and_then(|c| c.get("outputs")) {
        Some(Json::Arr(outs)) => {
            assert!(outs.iter().any(|o| o.as_str() == Some(&report_id)))
        }
        _ => panic!("record.outputs absent: {r:?}"),
    }
    // KA-12 — an identical re-call serves the same report body id.
    let r2 = ok(&call(&mut svc, "lab.analysis.analyze", analyze(&eid)));
    assert_eq!(
        r2.get("body")
            .and_then(|r| r.get("report_id"))
            .and_then(Json::as_str),
        Some(report_id.as_str())
    );
}

/// The boundary's typed refusals: an unresolvable `spec_ref` and an
// unknown row selector refuse — never a silent skip or a fabricated run.
#[test]
fn lab_analysis_analyze_refusals() {
    let mut svc = service();
    lab_hello(&mut svc);
    let spec = |spec_ref: &str| {
        let mut s = hh_lab::analysis::AnalysisSpec {
            spec_id: String::new(),
            kind: "summarize".into(),
            query: hh_lab::analysis::QuerySpec {
                metrics: vec!["task_success".into()],
                filters: None,
                grain: None,
            },
            spec_ref: Some(spec_ref.to_string()),
            label: None,
            estimator_selection: hh_ontology::eval::EstimatorSelection {
                method: hh_ontology::eval::IntervalMethod::ClusteredClt,
                selection_rule: "adr-0158.clt_floor".into(),
                floors: BTreeMap::new(),
                fallback_chain: vec![],
                substituted: None,
            },
            resample: None,
            outputs: vec!["report".into()],
        };
        s.spec_id = s.spec_id();
        s
    };
    let params = |spec: hh_lab::analysis::AnalysisSpec| {
        Json::obj([
            ("spec", spec.to_json()),
            ("rows", Json::Arr(vec![Json::str("no-such-row")])),
            (
                "tasks",
                Json::Arr(vec![Json::obj([
                    ("task_id", Json::str("t1")),
                    ("suite_id", Json::str("suite-test")),
                    ("split_label", Json::str("held_out")),
                    ("split_hash", Json::str("sha256:split-1")),
                    ("stratum", Json::str("private_held_out")),
                ])]),
            ),
        ])
    };
    // `spec_ref` pointing nowhere — a typed schema violation on the spec.
    let e = call(
        &mut svc,
        "lab.analysis.analyze",
        params(spec("no-such-exp")),
    );
    assert_eq!(err_kind(&e), "SchemaViolation", "{e:?}");
    // A design-free spec with an unknown row selector — the store refuses.
    let mut s = spec("x");
    s.spec_ref = None;
    s.spec_id = s.spec_id();
    let e = call(&mut svc, "lab.analysis.analyze", params(s));
    assert_eq!(err_kind(&e), "Refused", "{e:?}");
}

// ── S3.5: `lab.assembly.*` over Group L (AC-R-2.10.1-10) ─────────────────────
// The service runs at the embedding boundary exchanging only ADR-0050 D2
// records; binding (a) `handle()` and binding (b) the stdio codec produce
// byte-identical responses over the same dispatch.

/// The `AssemblySource` for the standard fixture: the authored scaffold
/// (`{root, nodes, edges}`) plus one `user` layer carrying the Stage-1 slot
/// bindings.
fn assembly_source_json() -> Json {
    let doc = document_json();
    let nodes = doc.get("nodes").cloned().unwrap_or(Json::Arr(vec![]));
    let edges = doc.get("edges").cloned().unwrap_or(Json::Arr(vec![]));
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
    let prov = hh_assembly::grammar::LayerProvenance {
        source_kind: hh_assembly::grammar::LayerSourceKind::User,
        id: "user:main".into(),
        version: "1".into(),
        precedence: 0,
    };
    Json::obj([
        ("dialect", Json::str("hir/1")),
        ("root_kind", Json::str("native")),
        (
            "document",
            Json::obj([
                ("root", Json::str("test:agent")),
                ("nodes", nodes),
                ("edges", edges),
            ]),
        ),
        (
            "layers",
            Json::Arr(vec![Json::obj([
                ("provenance", hh_assembly::grammar::layer_json(&prov)),
                ("fragment", assembly.to_json()),
            ])]),
        ),
        ("ext", Json::obj([])),
    ])
}

#[test]
fn ac10_assembly_ops_run_out_of_process_over_group_l() {
    let source = assembly_source_json();
    let reqs: Vec<(String, Json)> = vec![
        (
            "lab.assembly.assemble".to_string(),
            Json::obj([("source", source.clone()), ("mode", Json::str("seal"))]),
        ),
        (
            "lab.assembly.plan".to_string(),
            Json::obj([("source", source.clone())]),
        ),
        (
            "lab.assembly.explain".to_string(),
            Json::obj([("source", source.clone())]),
        ),
        (
            "lab.assembly.validate_batch".to_string(),
            Json::obj([(
                "points",
                Json::Arr(vec![
                    Json::obj([("source", source.clone())]),
                    Json::obj([("source", source.clone())]),
                ]),
            )]),
        ),
    ];
    let mut tape: Vec<String> = vec![Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::str("h")),
        ("method", Json::str("hello")),
        (
            "params",
            hello_params(caps_json(&[
                ("experimental", true),
                ("serves_measurement", true),
            ])),
        ),
    ])
    .to_canonical_string()];
    tape.extend(reqs.iter().enumerate().map(|(i, (m, p))| {
        Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::str(format!("a{i}"))),
            ("method", Json::str(m.clone())),
            ("params", p.clone()),
        ])
        .to_canonical_string()
    }));

    // Binding (b): the stdio loop (the separate-process boundary).
    let dir_b = test_dir("asm-b");
    let mut out_b = Vec::new();
    {
        let mut svc = service_in(&dir_b);
        let input = tape.join("\n") + "\n";
        let mut r = std::io::BufReader::new(input.as_bytes());
        hh_embed::stdio::serve(&mut svc, &mut r, &mut out_b).unwrap();
    }
    // Binding (a): in-process `handle()`.
    let dir_a = test_dir("asm-a");
    let mut out_a = Vec::new();
    {
        let mut svc = service_in(&dir_a);
        for line in &tape {
            let req = hh_wire::jsonrpc::parse_request(line).unwrap();
            let resp = svc.handle(&req);
            out_a.extend_from_slice(resp.to_canonical_string().as_bytes());
            out_a.push(b'\n');
            for n in svc.drain_notifications() {
                out_a.extend_from_slice(n.to_canonical_string().as_bytes());
                out_a.push(b'\n');
            }
        }
    }
    let a = String::from_utf8(out_a).unwrap();
    let b = String::from_utf8(out_b).unwrap();

    // Byte parity across *instances* can't hold for snapshot-cutting ops —
    // `resolved_at`/`verified_at` stamps enter `version_id` (§3.3.6 keeps
    // `semantic_id` stamp-free). The parity claim is over the stable
    // members: status, report.status, plan.exit, identity.semantic_id.
    let lines_a: Vec<&str> = a.lines().collect();
    let lines_b: Vec<&str> = b.lines().collect();
    assert_eq!(lines_a.len(), 5, "{a}");
    assert_eq!(lines_b.len(), 5, "{b}");
    for (i, (la, lb)) in lines_a.iter().zip(lines_b.iter()).enumerate() {
        let (ja, jb) = (parse(la).unwrap(), parse(lb).unwrap());
        assert!(
            ja.get("result").is_some() && jb.get("result").is_some(),
            "line {i} is a result, not a boundary error: {la} | {lb}"
        );
        let (ra, rb) = (ja.get("result").unwrap(), jb.get("result").unwrap());
        assert_eq!(
            ra.get("status").and_then(Json::as_str),
            rb.get("status").and_then(Json::as_str),
            "line {i}: status agrees across bindings"
        );
        assert_eq!(
            ra.get("report")
                .and_then(|r| r.get("status"))
                .and_then(Json::as_str),
            rb.get("report")
                .and_then(|r| r.get("status"))
                .and_then(Json::as_str),
            "line {i}: report.status agrees"
        );
        assert_eq!(
            ra.get("plan").and_then(|p| p.get("exit")),
            rb.get("plan").and_then(|p| p.get("exit")),
            "line {i}: plan.exit agrees"
        );
        assert_eq!(
            ra.get("identity").and_then(|i| i.get("semantic_id")),
            rb.get("identity").and_then(|i| i.get("semantic_id")),
            "line {i}: semantic_id agrees (stamp-free identity)"
        );
    }

    // And assemble seals through the stdio boundary.
    let assemble = parse(lines_b[1]).unwrap();
    let result = assemble.get("result").unwrap();
    assert_eq!(
        result.get("status").and_then(Json::as_str),
        Some("ok"),
        "assemble seals through the stdio boundary: {result:?}"
    );
    assert!(result.get("sealed").is_some());
    let batch = parse(lines_b[4]).unwrap();
    let reports = match batch.get("result") {
        Some(Json::Arr(v)) => v,
        other => panic!("validate_batch returns an array: {other:?}"),
    };
    assert_eq!(reports.len(), 1, "identical points validate once");
}

// ── S3.6 — `replay` + `counterfactual` (R-2.2.4⁰ᵇ; §5a.4; ADR-0135) ──────────

/// A parseable `MatchSpec` (matched_cap over `model_calls`) — the arm design's
/// `budget` member is this canonical document.
fn match_spec_json() -> Json {
    Json::obj(vec![
        ("dimensions", Json::Arr(vec![Json::str("model_calls")])),
        ("mode", Json::str("matched_cap")),
        ("tolerance", Json::Int(0)),
        ("model_scope", Json::str("same_snapshot")),
        ("cache_policy", Json::str("cold_start")),
    ])
}

fn counterfactual_params(session: &str, at_seq: i64, earliest: i64, design: Json) -> Json {
    Json::obj(vec![
        ("session_id", Json::str(session)),
        ("at_seq", Json::Int(at_seq)),
        (
            "intervention",
            Json::obj(vec![
                ("kind", Json::str("response_substitution")),
                ("target", Json::str("model")),
                ("earliest_affected_seq", Json::Int(earliest)),
            ]),
        ),
        ("design", design),
        ("env", Json::str("trace_only")),
    ])
}

fn valid_design() -> Json {
    Json::obj(vec![
        ("n_seeds", Json::Int(2)),
        ("factual_arm", Json::Bool(true)),
        ("budget", match_spec_json()),
    ])
}

/// AC-R-2.2.4-7/-11 — `replay` mints the durable `lifecycle.replay.started`/
/// `finished` pair on the target and lands the content-addressed
/// `ReplayValidityReport` (`report_ref`) for both driver modes.
#[test]
fn s3_6_replay_lands_the_durable_pair_and_report() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();

    // An unknown driver mode is a schema error, never a silent default.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "replay",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("driver_mode", Json::str("bogus")),
            ]),
        )),
        "SchemaViolation"
    );

    for mode in ["reconstruct", "deterministic"] {
        let out = ok(&call(
            &mut svc,
            "replay",
            Json::obj(vec![
                ("session_id", Json::str(id.clone())),
                ("driver_mode", Json::str(mode)),
            ]),
        ));
        assert_eq!(out.get("run_id").and_then(Json::as_str), Some(run.as_str()));
        assert_eq!(out.get("driver_mode").and_then(Json::as_str), Some(mode));
        assert!(
            out.get("report_ref").and_then(Json::as_str).is_some(),
            "{mode} lands the report blob: {out:?}"
        );
        let report = out.get("report").unwrap();
        assert!(
            report.get("mode").and_then(Json::as_str).is_some(),
            "{mode} report carries a validity mode: {report:?}"
        );
    }
    let classes: Vec<String> = svc
        .store()
        .envelopes(&run)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect();
    assert_eq!(
        classes
            .iter()
            .filter(|c| c.as_str() == "lifecycle.replay.started")
            .count(),
        2,
        "one started row per replay call"
    );
    assert_eq!(
        classes
            .iter()
            .filter(|c| c.as_str() == "lifecycle.replay.finished")
            .count(),
        2,
        "one finished row per replay call"
    );
}

/// AC-R-2.2.4-8 — the `counterfactual` hook refuses `UnbudgetedArm` without a
/// parseable `MatchSpec`, refuses `at > earliest_affected_seq`, and requires
/// the factual arm.
#[test]
fn s3_6_counterfactual_gates_unbudgeted_and_mistimed_arms() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // `design.budget` absent → the schema layer refuses (`required`).
    let no_budget = Json::obj(vec![
        ("n_seeds", Json::Int(2)),
        ("factual_arm", Json::Bool(true)),
    ]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "counterfactual",
            counterfactual_params(&id, 0, 0, no_budget)
        )),
        "SchemaViolation"
    );
    // `design.budget` present but unparseable → `UnbudgetedArm`.
    let bad_budget = Json::obj(vec![
        ("n_seeds", Json::Int(2)),
        ("factual_arm", Json::Bool(true)),
        ("budget", Json::obj(vec![])),
    ]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "counterfactual",
            counterfactual_params(&id, 0, 0, bad_budget)
        )),
        "UnbudgetedArm"
    );
    // `at_seq > earliest_affected_seq` — the intervention precedes the cut.
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "counterfactual",
            counterfactual_params(&id, 5, 2, valid_design())
        )),
        "Refused"
    );
    // `factual_arm` is mandatory — the noise floor is never optional.
    let no_factual = Json::obj(vec![
        ("n_seeds", Json::Int(2)),
        ("factual_arm", Json::Bool(false)),
        ("budget", match_spec_json()),
    ]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "counterfactual",
            counterfactual_params(&id, 0, 0, no_factual)
        )),
        "Refused"
    );
    // n < k — the replicate axis needs n ≥ k ≥ 1.
    let under = Json::obj(vec![
        ("n_seeds", Json::Int(1)),
        ("k", Json::Int(3)),
        ("factual_arm", Json::Bool(true)),
        ("budget", match_spec_json()),
    ]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "counterfactual",
            counterfactual_params(&id, 0, 0, under)
        )),
        "Refused"
    );
}

/// AC-R-2.2.4-8 — a valid `counterfactual` opens `n` factual + `n`
/// counterfactual arms off one fork point; every arm carries `arm_role` and
/// `charged_to = instrument`; the `ComparisonReport` blob lands the factual
/// dispersion (`comparison_ref`).
#[test]
fn s3_6_counterfactual_opens_paired_arms_with_the_factual_floor() {
    let mut svc = service();
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    assert!(r.get("result").is_some());
    let s = open_new(&mut svc);
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    let out = ok(&call(
        &mut svc,
        "counterfactual",
        counterfactual_params(&id, 0, 0, valid_design()),
    ));
    let arms = |key: &str| -> Vec<Json> {
        match out.get(key) {
            Some(Json::Arr(a)) => a.clone(),
            other => panic!("{key} arms: {other:?}"),
        }
    };
    let factual = arms("factual");
    let counterfactual = arms("counterfactual");
    assert_eq!(factual.len(), 2, "n_seeds factual arms");
    assert_eq!(counterfactual.len(), 2, "n_seeds counterfactual arms");
    for a in factual.iter().chain(counterfactual.iter()) {
        assert!(a.get("run_id").and_then(Json::as_str).is_some());
        assert!(a.get("seed").and_then(Json::as_str).is_some());
        assert_eq!(
            a.get("charged_to").and_then(Json::as_str),
            Some("instrument"),
            "arms charge to the instrument: {a:?}"
        );
    }
    for a in &factual {
        assert_eq!(a.get("arm_role").and_then(Json::as_str), Some("factual"));
    }
    for a in &counterfactual {
        assert_eq!(
            a.get("arm_role").and_then(Json::as_str),
            Some("counterfactual")
        );
    }
    // Distinct seeds per arm — the replicate axis.
    let seeds: std::collections::BTreeSet<String> = factual
        .iter()
        .chain(counterfactual.iter())
        .filter_map(|a| a.get("seed").and_then(Json::as_str).map(str::to_string))
        .collect();
    assert_eq!(seeds.len(), 4, "one seed per arm");
    // The ComparisonReport blob is content-addressed — `comparison_ref` names it.
    assert!(out.get("comparison_ref").and_then(Json::as_str).is_some());
    assert!(out.get("intervention_ref").and_then(Json::as_str).is_some());
}

// ── S3.11b — bundle `resolved_dependencies.extensions[]` (AC-R-2.8.5-10) ──
// The `kernel.bundle` path projects the sealed definition's pinned
// `assembly.extensions.refs[]` through `trust_snapshot` into the bundle
// manifest — end-to-end over the boundary, never a re-read of the registry
// at bundle time.

/// A definition document binding one skill extension (selector `latest` —
/// `resolve` pins it against the embedded registry at open).
fn document_json_with_extension(name: &str) -> Json {
    let mut doc = document_json();
    if let Json::Obj(m) = &mut doc {
        if let Some(Json::Obj(am)) = m.get_mut("assembly") {
            am.insert(
                "extensions".into(),
                Json::obj([
                    (
                        "sources",
                        Json::Arr(vec![Json::obj([
                            ("kind", Json::str("registry")),
                            ("name", Json::str(name)),
                        ])]),
                    ),
                    (
                        "refs",
                        Json::Arr(vec![Json::obj([
                            ("name", Json::str(name)),
                            ("kind", Json::str("skill")),
                            (
                                "locator",
                                Json::obj([
                                    ("scheme", Json::str("registry")),
                                    (
                                        "credential_free_uri",
                                        Json::str(format!("registry://local/{name}")),
                                    ),
                                    ("selector", Json::str("latest")),
                                ]),
                            ),
                        ])]),
                    ),
                    ("merge_policy", Json::str("exact_only")),
                ]),
            );
        }
    }
    doc
}

/// Register a skill extension record through `lab.registry.register`;
/// returns `(version_id, content_pin)`.
fn register_skill_extension(
    svc: &mut EmbedService,
    name: &str,
    payload: &[u8],
) -> (String, String) {
    use hh_registry::extension::{
        extension_body_json, resolve_candidate, Candidate, DeclaredSource, ExtensionKind,
        FetchOutcome, SourceLocator,
    };
    let rec = resolve_candidate(
        &Candidate {
            name: name.into(),
            kind: ExtensionKind::Skill,
            locator: SourceLocator {
                scheme: "registry".into(),
                credential_free_uri: format!("registry://local/{name}"),
                selector: Some("latest".into()),
                resolved: None,
                fetched_at: None,
            },
            source: DeclaredSource::Registry { name: name.into() },
        },
        &FetchOutcome {
            resolved: format!("resolved:{name}@1"),
            content: hh_identity::idp::address(payload, "application/octet-stream"),
            fetched_at: 7,
            code_identity: vec![],
            source_snapshot: None,
            surface_pin: None,
        },
        Origin::kernel("s311b.test"),
        Some(payload),
        7,
    );
    let content_pin = rec.content.id();
    let r = call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("extension")),
            ("body", extension_body_json(&rec)),
            ("registrar", registrar()),
        ]),
    );
    let v = ok(&r);
    let vid = v
        .get("version_id")
        .and_then(Json::as_str)
        .expect("register returns version_id")
        .to_string();
    (vid, content_pin)
}

#[test]
fn s311b_kernel_bundle_carries_bound_extensions() {
    let mut svc = service();
    lab_hello(&mut svc);
    let (ext_vid, content_pin) = register_skill_extension(&mut svc, "ext-s311b", b"skill payload");

    let s = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                new_spec_with_doc(document_json_with_extension("ext-s311b"), None),
            ),
            ("idempotency_key", Json::str("open-ext")),
        ]),
    ));
    let session_id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    submit(&mut svc, &session_id, text_input("bundle with extension"));
    close(&mut svc, &session_id);

    let dir = test_dir("bundle-ext");
    let r = deliver_bundle(&mut svc, &run_id, &dir);
    let manifest = r.get("manifest").cloned().unwrap();

    // `resolved_dependencies.extensions[]` carries the pinned entry —
    // resolve pinned the `latest` selector to the registered version_id at
    // open; the bundle re-projects it via `trust_snapshot` (never a second
    // resolution).
    let exts = manifest
        .get("resolved_dependencies")
        .and_then(|d| d.get("extensions"))
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .expect("resolved_dependencies.extensions[]");
    assert_eq!(exts.len(), 1, "one bound extension: {exts:?}");
    let e = &exts[0];
    assert_eq!(
        e.get("extension_id").and_then(Json::as_str),
        Some(ext_vid.as_str())
    );
    assert_eq!(
        e.get("content").and_then(Json::as_str),
        Some(content_pin.as_str())
    );
    assert!(e.get("trust_record").and_then(Json::as_str).is_some());

    // The payload bytes were never captured into the run's artifact pool —
    // the pin lands `unpinned{not_captured}`, declared, never silently
    // absent (CC3; ADR-0289 D5).
    let unpinned = manifest
        .get("unpinned")
        .and_then(|v| match v {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        unpinned.iter().any(|u| {
            u.get("role").and_then(Json::as_str) == Some(format!("extension:{ext_vid}").as_str())
                && u.get("reason").and_then(Json::as_str) == Some("not_captured")
        }),
        "the uncaptured extension pin must be named: {unpinned:?}"
    );
}
