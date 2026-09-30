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
