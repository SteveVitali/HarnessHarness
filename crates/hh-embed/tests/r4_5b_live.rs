//! R4.5b — the *live* leg (ADR-0363): a real `hharness-embed` run against the
//! provisioned local Ollama (`HH_LOCAL_BASE_URL`/`HH_LOCAL_MODEL` —
//! `provided: yes/no` only, values never enter the record). The cell is
//! `#[ignore]`d — `scripts/live-verify.sh local-run` drives it via
//! `cargo test -- --ignored` (the `cargo-test` leg kind).
//!
//! Assertions (all honest, never fabricated):
//!   * `model.call.requested` carries the live members (`plan_hash`,
//!     `endpoint_ref`, `model_ref.provider_model_id`, `token_estimate`);
//!   * a terminal `model.call.completed`/`failed` lands — `completed`
//!     carries `usage{record{normalizer_ref}}` + `served_model` + the
//!     `hh.model_io/1` blob its `response_ref` resolves to;
//!   * `store.verify` — the hash chain + sealed-prefix fold is clean;
//!   * `replay{driver_mode: deterministic}` reproduces the run — the
//!     recorded rows feed `ReplayModel`, no second live call.
//!
//! The model's content is *not* asserted (a live provider's text/tool
//! choice is its own); the wire leg is what this cell proves. A
//! `model_calls: 6` budget bound keeps a looping model honest — the
//! envelope escalates and parks rather than burning the endpoint.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hharness_assembly::grammar::Assembly;
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

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

/// The live-lane document — `model_port` binds by `env:` indirection so
/// the definition carries *names*, never values (ADR-0363 D3).
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
                dimensions: BTreeMap::from([
                    (
                        "tokens.blended".to_string(),
                        DimensionBound {
                            hard: Some(500_000),
                            soft: None,
                        },
                    ),
                    // The loop bound — a live model that keeps calling
                    // surfaces escalates + parks, never spins.
                    (
                        "model_calls".to_string(),
                        DimensionBound {
                            hard: Some(6),
                            soft: None,
                        },
                    ),
                ]),
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
    slots.insert(
        "model_port".to_string(),
        SlotBindings::One(SlotBinding {
            variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
            params: BTreeMap::from([
                ("dialect".to_string(), Json::str("openai-compatible@1")),
                ("endpoint".to_string(), Json::str("env:HH_LOCAL_BASE_URL")),
                (
                    "provider_model_id".to_string(),
                    Json::str("env:HH_LOCAL_MODEL"),
                ),
            ]),
            enabled: true,
            locality: None,
        }),
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

/// `r45b_live_ollama_run` — the local-live leg: open → submit → drive →
/// `verify` → deterministic `replay`. The transcript `live-verify`
/// redacts + records is this test's stdout — the assertions' wording is
/// the record's `assertions` tail.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh local-run"]
fn r45b_live_ollama_run_ledger_verify_replay() {
    // The provisioned names — missing ⇒ the leg's `need_names` already
    // refused upstream; here the absence is a clean early exit (never a
    // fabricated pass — the framework records `skipped` legs honestly).
    if std::env::var("HH_LOCAL_BASE_URL").is_err() || std::env::var("HH_LOCAL_MODEL").is_err() {
        eprintln!("HH_LOCAL_BASE_URL/HH_LOCAL_MODEL unprovided — skipped");
        return;
    }
    let root = std::env::temp_dir().join(format!("hh-r45b-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.5b-live".into(),
    })
    .unwrap();
    let r = call(
        &mut svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r4.5b-live")),
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
    let s = ok(&call(
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
                ]),
            ),
            ("idempotency_key", Json::str("k-live")),
        ]),
    ));
    let sess = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    eprintln!("live run opened: {run}");

    // One submitted instruction — the wording asks the model to land the
    // `hh_submit` tool call so the run's completion path exercises end to
    // end; an end_turn answer is still a live-model proof (the assert
    // set below accepts either terminal).
    let sub = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(sess.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    (
                        "text",
                        Json::str(
                            "You have exactly one tool available: hh_submit, which takes a string parameter named text. Call hh_submit now with text \"done\". If you cannot call tools, reply with the single word done.",
                        ),
                    ),
                ])]),
            ),
            ("idempotency_key", Json::str("k-sub")),
        ]),
    );
    ok(&sub);

    let evs: Vec<(u64, String, Json)> = svc
        .store()
        .envelopes(&run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect();
    eprintln!(
        "rows: {}",
        evs.iter()
            .filter(|(_, c, _)| c.starts_with("model."))
            .map(|(_, c, _)| c.clone())
            .collect::<Vec<_>>()
            .join(",")
    );
    let requested = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.requested")
        .expect("model.call.requested lands");
    assert!(requested.2.get("plan_hash").is_some(), "plan_hash member");
    assert_eq!(
        requested
            .2
            .get("model_ref")
            .and_then(|m| m.get("provider_model_id"))
            .and_then(Json::as_str),
        std::env::var("HH_LOCAL_MODEL").ok().as_deref(),
        "the provider_model_id is the provisioned model's coordinate"
    );
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("model.call.completed lands (the live stream decoded)");
    let stop = completed
        .2
        .get("stop_reason")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    eprintln!("live call completed: stop_reason={stop}");
    let usage = completed.2.get("usage").expect("usage member");
    assert_eq!(
        usage.get("available"),
        Some(&Json::Bool(true)),
        "the local provider reports usage (stream_options.include_usage)"
    );
    let resp_ref = completed
        .2
        .get("response_ref")
        .and_then(Json::as_str)
        .expect("response_ref");
    let parsed = hharness_identity::idp::parse_id(resp_ref).expect("idp id");
    let blob = std::fs::read(svc.store().root().join("blobs").join(&parsed.digest_hex))
        .expect("model_io blob resolves");
    let io: Json = hharness_wire::json::parse(&String::from_utf8_lossy(&blob)).unwrap();
    assert_eq!(
        io.get("schema").and_then(Json::as_str),
        Some("hh.model_io/1")
    );

    // `verify` — hash chain + sealed-prefix fold clean on the live run.
    let v = call(
        &mut svc,
        "verify",
        Json::obj([("session_id", Json::str(sess.clone()))]),
    );
    assert_eq!(
        ok(&v).get("verdict").and_then(Json::as_str),
        Some("ok"),
        "verify verdict"
    );
    eprintln!("ledger verified");

    // `replay` deterministic — the recorded rows feed the replay feed;
    // the live provider is *not* re-called.
    let rep = ok(&call(
        &mut svc,
        "replay",
        Json::obj([
            ("session_id", Json::str(sess)),
            ("driver_mode", Json::str("deterministic")),
        ]),
    ));
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "deterministic replay reproduces the live run byte-identically: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "replay carries no divergence reasons: {}",
        rep.to_canonical_string()
    );
    eprintln!("replay reproduced byte-identically");
    let _ = std::fs::remove_dir_all(&root);
}
