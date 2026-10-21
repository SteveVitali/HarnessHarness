//! R4.5d — the *live* hosted-provider legs (ADR-0363 / ADR-0366): one
//! minimal real call each through `GatewayModel` — Anthropic Messages
//! (`anthropic-messages@1`) and OpenAI Chat Completions (`openai@1`).
//!
//! Credentials ride `credential_ref: env:NAME` *coordinates* — the value
//! resolves inside `HttpModelTransport::send` and lands in the request
//! header only; the ledger carries `binding_id`/`provided`, never the
//! secret. `scripts/live-verify.sh {anthropic,openai}-call` drives these
//! `#[ignore]`d cells and gates the spend (USD 5 ticket allocation).
//!
//! Each cell prints two marker lines the leg parser reads:
//!   * `LIVE_USAGE: <json>` — the completed row's `usage.record.view`;
//!   * `LIVE_MODEL: <id>` — the provider-reported served model.
//!
//! Assertions (all honest, never fabricated): `model.call.requested`
//! carries `credential_binding_id` = the `launch_env:NAME` coordinate and
//! `model_ref.provider_model_id`; `model.call.completed` lands with
//! `usage{available:true, record{view,raw}}`; the planted env *value*
//! appears in no payload and no blob; `verify` folds clean. A failed leg
//! records `skipped(<reason>)`/`fail:` — never a pass.

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

/// The live-lane document — the hosted `model_port` binds `endpoint` and
/// `provider_model_id` as literal coordinates (the public endpoints are
/// not secrets); `credential_ref` is the `env:NAME` coordinate only.
fn document_json(
    dialect: &str,
    endpoint: &str,
    model: &str,
    provider_params: BTreeMap<String, Json>,
    credential_env: &str,
) -> Json {
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
                            hard: Some(2),
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
    let mut lane = BTreeMap::from([
        ("dialect".to_string(), Json::str(dialect)),
        ("endpoint".to_string(), Json::str(endpoint)),
        ("provider_model_id".to_string(), Json::str(model)),
        (
            "credential_ref".to_string(),
            Json::str(format!("env:{credential_env}")),
        ),
    ]);
    if !provider_params.is_empty() {
        lane.insert("provider_params".to_string(), Json::Obj(provider_params));
    }
    let mut slots = BTreeMap::new();
    slots.insert(
        "model_port".to_string(),
        SlotBindings::One(SlotBinding {
            variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
            params: lane,
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

/// The shared live leg — open → submit → assert → print the leg markers.
/// `credential_env` is the *name* only; the value never enters a record.
fn live_call(
    tag: &str,
    dialect: &str,
    endpoint: &str,
    model: &str,
    credential_env: &str,
    provider_params: BTreeMap<String, Json>,
) {
    // Missing ⇒ `need_names` already refused upstream; inside the test
    // the absence is a clean early exit (never a fabricated pass).
    let secret = match std::env::var(credential_env) {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("{credential_env} unprovided — skipped");
            return;
        }
    };
    let root = std::env::temp_dir().join(format!("hh-r45d-live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.5d-live".into(),
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
                    ("name", Json::str("r4.5d-live")),
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
                            (
                                "document",
                                document_json(
                                    dialect,
                                    endpoint,
                                    model,
                                    provider_params,
                                    credential_env,
                                ),
                            ),
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

    // One minimal instruction — a fixed short answer; the wire leg is
    // what this cell proves (the content itself is not asserted).
    let sub = call(
        &mut svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(sess.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("Reply with the single word: done")),
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
            .get("credential_binding_id")
            .and_then(Json::as_str),
        Some(format!("launch_env:{credential_env}").as_str()),
        "the credential coordinate — never the value"
    );
    assert_eq!(
        requested
            .2
            .get("model_ref")
            .and_then(|m| m.get("provider_model_id"))
            .and_then(Json::as_str),
        Some(model)
    );
    let completed = evs.iter().find(|(_, c, _)| c == "model.call.completed");
    if completed.is_none() {
        // Honest diagnostics — the typed error class + the blob's
        // `outcome.error` (the provider's own error document, never a
        // credential value) print so the leg transcript carries the
        // reason verbatim.
        for (_, c, p) in &evs {
            if c == "model.call.failed" {
                eprintln!("FAILED ROW: {}", p.to_canonical_string());
            }
        }
    }
    let completed = completed.expect("model.call.completed lands (the live stream decoded)");
    let stop = completed
        .2
        .get("stop_reason")
        .and_then(Json::as_str)
        .unwrap_or("?")
        .to_string();
    eprintln!("live call completed: stop_reason={stop}");
    let usage = completed.2.get("usage").expect("usage member");
    assert_eq!(
        usage.get("available"),
        Some(&Json::Bool(true)),
        "the hosted provider reports usage"
    );
    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .expect("usage.record.view");
    println!("LIVE_USAGE: {}", view.to_canonical_string());
    let served = completed
        .2
        .get("served_model")
        .and_then(Json::as_str)
        .unwrap_or(model);
    println!("LIVE_MODEL: {served}");

    // The leak sweep — the provisioned value in no payload, no blob.
    let payloads: String = evs
        .iter()
        .map(|(_, _, p)| p.to_canonical_string())
        .collect();
    assert!(
        !payloads.contains(&secret),
        "no ledger payload carries the credential value"
    );
    let blobs = svc.store().root().join("blobs");
    if blobs.exists() {
        for e in std::fs::read_dir(&blobs).unwrap() {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&b).contains(&secret),
                "no blob carries the credential value"
            );
        }
    }
    eprintln!("leak sweep clean — the value appears in no payload/blob");

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
    let _ = std::fs::remove_dir_all(&root);
}

/// `anthropic-call` — one Messages call: `claude-haiku-4-5-20251001`,
/// `max_tokens: 8` (the family's required member, pinned minimal).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh anthropic-call"]
fn r45d_live_anthropic_call() {
    live_call(
        "anthropic",
        "anthropic-messages@1",
        "https://api.anthropic.com/v1",
        "claude-haiku-4-5-20251001",
        "ANTHROPIC_API_KEY",
        BTreeMap::from([("max_tokens".to_string(), Json::Int(8))]),
    );
}

/// `openai-call` — one Chat Completions call: `gpt-4.1-mini-2025-04-14`
/// (the dated snapshot — OpenAI serves the versioned id verbatim and the
/// lane's `substitution_allowed: false` pins it), `max_completion_tokens: 8`.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh openai-call"]
fn r45d_live_openai_call() {
    live_call(
        "openai",
        "openai@1",
        "https://api.openai.com/v1",
        "gpt-4.1-mini-2025-04-14",
        "OPENAI_API_KEY",
        BTreeMap::from([("max_completion_tokens".to_string(), Json::Int(8))]),
    );
}
