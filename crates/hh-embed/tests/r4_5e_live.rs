//! R4.5e — the *live* `routing-probe` leg (ADR-0363 / DF-S1.18-1 /
//! ADR-0284 (b) fired): `probe_profile`/`ProbeSpec` executed against all
//! three admitted families — local Ollama (`openai-compatible@1`),
//! Anthropic (`anthropic-messages@1`), OpenAI (`openai@1`) — then one
//! routed run whose `health_aware` lane selects from the live probe
//! results the durable prefix carries.
//!
//! The `router` slot's `probe_exec[]` declarations execute at `open`
//! through `hharness_gateway::probes::exec_probe_decl`: the authenticated
//! model listing folds the `served_model.<id>` pin, the declared
//! capability walk rides the one measured tiny probe call per family
//! (cached across `cap.streaming`/`cap.snapshot_id_exposed`/
//! `cap.cache_state_visible`), and every row mints
//! `model.profile.probed{records[], observed.usage}` durable before the
//! first `model.call.requested`. A dependency-set `DRIFT`/`UNSUPPORTED`
//! verdict mints `model.profile.status.changed{trigger: probe_failure}`
//! — never silently.
//!
//! Credentials ride `env:NAME` *coordinates* — the value resolves inside
//! the transport and lands in the request header only; the ledger
//! carries `binding_id`/`provided`, never the secret.
//! `scripts/live-verify.sh routing-probe` drives this `#[ignore]`d cell
//! and gates the spend (USD 5 ticket allocation).
//!
//! Markers the leg parser reads:
//!   * `LIVE_PROBE_USAGE: <provider> <json>` — the measured probe-call
//!     usage per family (`{input_tokens, output_tokens, calls}`), the
//!     spend leg prices it against the pinned table;
//!   * `LIVE_PROBE: <provider> <json>` — `{probe_run_id, verdicts}`;
//!   * `LIVE_SELECTED: <model_ref>` — the live-signal arm's pick;
//!   * `LIVE_USAGE:`/`LIVE_MODEL:` — the routed call's completed row.
//!
//! Assertions (all honest, never fabricated): every family's
//! `model.profile.probed` row lands durable before the first
//! `model.call.requested` with derived-verdict `records[]`;
//! `model.route.decided` lands with `kind: health_aware` ahead of the
//! call it binds; `model.call.completed` carries usage; the planted env
//! *values* appear in no payload and no blob; `verify` folds clean. A
//! failed leg records `skipped(<reason>)`/`fail:` — never a pass.

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

/// The registered `ModelProfile` the bound `primary` role resolves
/// (`prof.r45e@1`) — the scripted candidate's coordinate
/// `hh-embed/kernel-scripted@1` matches the selector; `max_output`
/// declared so the G-4 reservation sizes honestly.
fn live_profile() -> hharness_compiler::profile::ModelProfile {
    use hharness_compiler::profile::*;
    let mut p = ModelProfile {
        profile_id: "prof.r45e".into(),
        version: "1".into(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "hharness-embed".into(),
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
            rule_id: "prof.r45e.expiry".into(),
            hypothesis: "the bound lane honours the declared contract".into(),
            evidence_refs: vec![EvidenceRef::legacy("sha256:ev")],
            owner: "test:owner".into(),
            reach_via: vec![],
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::ModelVersionChange,
                value: None,
            },
            removal_test_ref: "t:r45e".into(),
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
    p.content_hash = hharness_compiler::profile::profile_identity(&p);
    p
}

fn registrar() -> Json {
    ProvenanceRecord::kernel("hharness-embed", 0).to_json()
}

/// The `health_aware` `RoutingPolicy` document the slot seals — the
/// live-signal arm: it ranks admitted candidates by the durable
/// prefix's observed failure rate (the probe verdicts fold into health
/// through `probe_targets`), and refuses `PolicyInvalid` when the
/// admitted set carries no observation — never a default-healthy pick.
fn health_aware_policy() -> Json {
    use hharness_gateway::router::*;
    let policy = RoutingPolicy {
        policy_id: "policy.r45e".into(),
        version: "1".into(),
        content_hash: String::new(),
        role_scope: std::collections::BTreeSet::new(),
        kind: RoutingPolicyKind::HealthAware,
        params: Json::obj([("table_ref", Json::str("role-table.r45e"))]),
        error_actions: RoutingPolicy::default_error_actions(),
        max_migration_loss: MigrationLossBound {
            dropped_items: 0,
            no_in_flight_tool_call: true,
        },
        allow_unknown: None,
        conditioned_rules: vec![],
        ext: BTreeMap::new(),
    };
    let mut j = policy.to_json();
    if let Json::Obj(m) = &mut j {
        if let Some(v) = m.remove("version") {
            m.insert("policy_version".into(), v);
        }
    }
    j
}

/// One `probe_exec[]` lane — `{profile_ref, dialect, endpoint,
/// credential_ref?, provider_model_id, pinned{}, canaries[],
/// dependency_capabilities[], status_declared}` verbatim (the arm resolves
/// `env:` on endpoint/provider_model_id; `credential_ref` keeps its
/// coordinate spelling; `provider_model_id` spells the T-LCD-01-admitted
/// member — `model` is a denied model-identity member).
fn probe_exec(
    dialect: &str,
    endpoint: &str,
    credential_ref: Option<&str>,
    model: &str,
    pinned: BTreeMap<String, Json>,
    deps: &[&str],
) -> Json {
    let mut m = BTreeMap::from([
        ("profile_ref".to_string(), Json::str("prof.r45e@1")),
        ("dialect".to_string(), Json::str(dialect)),
        ("endpoint".to_string(), Json::str(endpoint)),
        ("provider_model_id".to_string(), Json::str(model)),
        ("pinned".to_string(), Json::Obj(pinned)),
        (
            "canaries".to_string(),
            Json::Arr(vec![Json::str("cap.unadmitted_probe")]),
        ),
        (
            "dependency_capabilities".to_string(),
            Json::Arr(deps.iter().map(|d| Json::str(*d)).collect()),
        ),
        ("status_declared".to_string(), Json::str("active")),
    ]);
    if let Some(c) = credential_ref {
        m.insert("credential_ref".to_string(), Json::str(c));
    }
    Json::Obj(m)
}

/// The routed live-lane document — `model_port` binds the local Ollama
/// lane (`env:` indirections; the family admits `none` auth) and
/// `router` binds the `health_aware` policy + the three `probe_exec`
/// lanes.
fn document_json(local_model: &str) -> Json {
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
    let mut slots = BTreeMap::new();
    // The dispatch lane — local Ollama (the zero-spend leg).
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
    // The routed lane — the live-signal arm + the three-family
    // `probe_exec` set.
    let served_local = format!("served_model.{local_model}");
    slots.insert(
        "router".to_string(),
        SlotBindings::One(SlotBinding {
            variant: ComponentVariantRef::selected("routing_policy", "hh/router_static", "latest"),
            params: BTreeMap::from([
                ("policy".to_string(), health_aware_policy()),
                ("budget_ceiling".to_string(), Json::Int(500_000)),
                (
                    "probe_exec".to_string(),
                    Json::Arr(vec![
                        // Local Ollama — `openai-compatible@1`, anonymous.
                        probe_exec(
                            "openai-compatible@1",
                            "env:HH_LOCAL_BASE_URL",
                            None,
                            "env:HH_LOCAL_MODEL",
                            BTreeMap::from([
                                ("cap.model_listing".to_string(), Json::Bool(true)),
                                ("cap.streaming".to_string(), Json::Bool(true)),
                            ]),
                            &["cap.model_listing", served_local.as_str()],
                        ),
                        // Anthropic — `anthropic-messages@1`, x-api-key.
                        probe_exec(
                            "anthropic-messages@1",
                            "https://api.anthropic.com/v1",
                            Some("env:ANTHROPIC_API_KEY"),
                            "claude-haiku-4-5-20251001",
                            BTreeMap::from([
                                ("cap.model_listing".to_string(), Json::Bool(true)),
                                ("cap.streaming".to_string(), Json::Bool(true)),
                                ("cap.count_tokens".to_string(), Json::Bool(true)),
                            ]),
                            &[
                                "cap.model_listing",
                                "served_model.claude-haiku-4-5-20251001",
                            ],
                        ),
                        // OpenAI — `openai@1`, bearer.
                        probe_exec(
                            "openai@1",
                            "https://api.openai.com/v1",
                            Some("env:OPENAI_API_KEY"),
                            "gpt-4.1-mini-2025-04-14",
                            BTreeMap::from([
                                ("cap.model_listing".to_string(), Json::Bool(true)),
                                ("cap.streaming".to_string(), Json::Bool(true)),
                            ]),
                            &["cap.model_listing", "served_model.gpt-4.1-mini-2025-04-14"],
                        ),
                    ]),
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
        "control_strategy".to_string(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".to_string(),
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

/// `routing-probe` — probe all three families at open, then one routed
/// call the `health_aware` arm selects from the probe-derived health.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh routing-probe"]
fn r45e_live_routing_probe() {
    // Missing ⇒ `need_names` already refused upstream; inside the test
    // the absence is a clean early exit (never a fabricated pass).
    for name in [
        "HH_LOCAL_BASE_URL",
        "HH_LOCAL_MODEL",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
    ] {
        match std::env::var(name) {
            Ok(v) if !v.is_empty() => {}
            _ => {
                eprintln!("{name} unprovided — skipped");
                return;
            }
        }
    }
    // The sweep's secrets are the *credential* values only — the
    // endpoint/model coordinates are non-secret members the records
    // legitimately carry (`endpoint_ref`, `model_ref.provider_model_id`).
    let secrets: Vec<String> = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
        .iter()
        .filter_map(|n| std::env::var(n).ok())
        .collect();
    let local_model = std::env::var("HH_LOCAL_MODEL").unwrap();
    let root = std::env::temp_dir().join(format!("hh-r45e-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.5e-live".into(),
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
                    ("name", Json::str("r4.5e-live")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    ok(&r);
    // Register the bound profile — `resolve_profile` reads the
    // registry's `registered()` snapshot (CF-046).
    ok(&call(
        &mut svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("model_profile")),
            (
                "body",
                Json::obj([
                    ("kind", Json::str("model_profile")),
                    (
                        "profile",
                        hharness_compiler::schema::profile_to_json(&live_profile()),
                    ),
                ]),
            ),
            ("registrar", registrar()),
        ]),
    ));
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
                            ("document", document_json(&local_model)),
                        ]),
                    ),
                    ("overrides", Json::Arr(vec![])),
                    (
                        "profile_binding",
                        Json::obj([("primary", Json::str("prof.r45e@1"))]),
                    ),
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

    // The arm-time rows — all three `model.profile.probed` minted
    // durable at open (before the first visible drive — CC3).
    let evs_at_open: Vec<(u64, String, Json)> = svc
        .store()
        .envelopes(&run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect();
    let probed: Vec<&Json> = evs_at_open
        .iter()
        .filter(|(_, c, _)| c == "model.profile.probed")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(
        probed.len(),
        3,
        "one probed row per probe_exec lane: {}",
        evs_at_open
            .iter()
            .map(|(_, c, _)| c.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    let dialects = ["openai-compatible@1", "anthropic-messages@1", "openai@1"];
    let verdict_ok = |v: &str| {
        matches!(
            v,
            "SUPPORTED"
                | "UNSUPPORTED"
                | "PARTIAL"
                | "NOT_APPLICABLE"
                | "UNKNOWN"
                | "SKIPPED"
                | "DRIFT"
        )
    };
    for (i, p) in probed.iter().enumerate() {
        assert_eq!(
            p.get("profile_ref").and_then(Json::as_str),
            Some("prof.r45e@1"),
            "probed row {i} binds the profile"
        );
        assert!(p.get("probe_run_id").and_then(Json::as_str).is_some());
        let records = match p.get("records") {
            Some(Json::Arr(a)) => a,
            _ => panic!("probed row carries records[]"),
        };
        assert!(!records.is_empty(), "probed row {i} records observations");
        for r in records {
            let v = r
                .get("verdict")
                .and_then(Json::as_str)
                .expect("every record carries a derived verdict");
            assert!(verdict_ok(v), "record verdict {v} outside vocabulary");
            assert!(
                r.get("observed_in").and_then(Json::as_str).is_some(),
                "the observation's provenance is recorded"
            );
        }
        // The measured probe-call usage rides the row — the leg's
        // spend record reads it, never an estimate.
        if let Some(u) = p.get("observed").and_then(|o| o.get("usage")) {
            println!(
                "LIVE_PROBE_USAGE: {} {}",
                dialects[i],
                u.to_canonical_string()
            );
        }
        let verdicts: Vec<String> = records
            .iter()
            .map(|r| {
                format!(
                    "{}={}",
                    r.get("dimension").and_then(Json::as_str).unwrap_or("?"),
                    r.get("verdict").and_then(Json::as_str).unwrap_or("?")
                )
            })
            .collect();
        println!(
            "LIVE_PROBE: {} {}",
            dialects[i],
            Json::obj([
                (
                    "probe_run_id",
                    Json::str(p.get("probe_run_id").and_then(Json::as_str).unwrap())
                ),
                ("verdicts", Json::str(verdicts.join(",")))
            ])
            .to_canonical_string()
        );
        eprintln!("probe {}: {}", dialects[i], verdicts.join(" "));
    }
    // A dependency-set contradiction mints `model.profile.status.changed`
    // — present only on real drift (recorded either way, never asserted
    // absent: an honest probe that drifts flips status).
    for (_, c, p) in &evs_at_open {
        if c == "model.profile.status.changed" {
            eprintln!("status flip recorded: {}", p.to_canonical_string());
        }
    }

    // One routed call — the `health_aware` arm reads the probe-derived
    // health off the durable prefix.
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
    let seq_of = |class: &str| evs.iter().find(|(_, c, _)| c == class).map(|(s, _, _)| *s);
    let decided = evs
        .iter()
        .find(|(_, c, _)| c == "model.route.decided")
        .expect("model.route.decided lands");
    // The sealed `health_aware` policy decided — and the candidate's
    // recorded `score` proves the arm read a live health observation
    // (the guard refuses `PolicyInvalid` when no observation exists —
    // a decision here could only have come from the probe fold).
    assert_eq!(
        decided
            .2
            .get("policy_ref")
            .and_then(|p| p.get("variant_ref"))
            .and_then(Json::as_str),
        Some("policy.r45e@1"),
        "the live-signal arm decided"
    );
    let considered = match decided.2.get("candidates_considered") {
        Some(Json::Arr(a)) => a,
        _ => panic!("route.decided carries candidates_considered[]"),
    };
    assert!(
        considered.iter().any(|c| c.get("score").is_some()),
        "the arm scored from the live probe-derived health: {}",
        decided.2.to_canonical_string()
    );
    let selected = decided
        .2
        .get("selected")
        .expect("the arm selected under its guard set");
    println!("LIVE_SELECTED: {}", selected.to_canonical_string());
    let requested_seq = seq_of("model.call.requested").expect("model.call.requested lands");
    assert!(
        probed.iter().all(|p| {
            evs.iter()
                .any(|(s, c, q)| c == "model.profile.probed" && q == *p && *s < requested_seq)
        }),
        "every probed row is durable before the first model.call.requested"
    );
    assert!(
        decided.0 < requested_seq,
        "route.decided precedes the call it binds"
    );
    let completed = evs
        .iter()
        .find(|(_, c, _)| c == "model.call.completed")
        .expect("model.call.completed lands (the live stream decoded)");
    let usage = completed.2.get("usage").expect("usage member");
    assert_eq!(
        usage.get("available"),
        Some(&Json::Bool(true)),
        "the provider reports usage"
    );
    let view = usage
        .get("record")
        .and_then(|r| r.get("view"))
        .expect("usage.record.view");
    println!("LIVE_USAGE: {}", view.to_canonical_string());
    println!(
        "LIVE_MODEL: {}",
        completed
            .2
            .get("served_model")
            .and_then(Json::as_str)
            .unwrap_or(&local_model)
    );

    // The leak sweep — every provisioned value in no payload, no blob.
    let payloads: String = evs
        .iter()
        .map(|(_, _, p)| p.to_canonical_string())
        .collect();
    for secret in &secrets {
        assert!(
            !payloads.contains(secret),
            "no ledger payload carries a provisioned value"
        );
    }
    let blobs = svc.store().root().join("blobs");
    if blobs.exists() {
        for e in std::fs::read_dir(&blobs).unwrap() {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            for secret in &secrets {
                assert!(
                    !String::from_utf8_lossy(&b).contains(secret),
                    "no blob carries a provisioned value"
                );
            }
        }
    }
    eprintln!("leak sweep clean — no provisioned value in any payload/blob");

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
