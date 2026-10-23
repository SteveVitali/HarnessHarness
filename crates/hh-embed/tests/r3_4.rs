//! R3.4 — the §5c context/memory producer residuals (BL-15; DF-S1.19-1;
//! R-2.4.1…R-2.4.5). The battery covers the wiring halves the round lands:
//!
//! - the sealed `context_policy` slot's non-`default` variants bind at arm
//!   (`policy_for_variant` — `hh/<name>` registry spellings map to the
//!   kernel's first-party `ContextPolicy` halves; `context.assembled`'s
//!   `policy_ref` is the variant's own `declare().variant_id`);
//! - `HarnessRule{action: consolidate_memories}` declarations project to
//!   `DriverConfig.consolidation_rules` at arm — `session_start` fires at
//!   the `open` boundary (once per session — the durable watermark row is
//!   the record it ran), `idle`/`rate_limit` at the `finish` boundary
//!   (after the scope floors so just-expired versions rebind);
//! - `context.memory.consolidated` is the durable, audit-grade watermark
//!   row — `{consolidation_id, rule_id, scope, cadence, trigger,
//!   input_watermark, last_success_watermark, applied[], usage, …}`;
//! - a rule's declared `retention` runs the landed `evaluate_retention`
//!   scheduler — `memory_usage`/`stale_index` fold the bound set
//!   (MemoryUsage-fed reclamation: a stale body stays bound while read
//!   after the last watermark; every non-memory pool member stays bound);
//! - the `memory_store` slot's proxy half (`RemoteStore` over a
//!   `plugin_abi/1` `StoreChannel`) serves the same `MemoryStorePort`
//!   surface — remote-first writes, mirror reads, one `record_read`
//!   stamp per side;
//! - indexed retrieval runs `retrieve_indexed` over the owner-projected
//!   `lexical`/`structural` views — `by_run` enumerates the artifact-layer
//!   projection every `context.memory.written` row registers; the resume
//!   fold re-resolves `content_ref` bodies (full-body rehydrate, never a
//!   skeleton that lies).
//!
//! Fixture-only (the ticket's offline ceiling): `EmbedModel` is the
//! scripted model, `local_host` the env, the loopback `StoreChannel` the
//! remote half — the packaged `hh-memory-store` subprocess leg is
//! `hharness-varhost`'s conformance battery over the same contract. No live
//! verification claims.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hharness_assembly::grammar::Assembly;
use hharness_context::consolidate::{ConsolidationCadence, ConsolidationRule};
use hharness_control::driver::{AssembleInputs, AssemblerPort, MemoryPort};
use hharness_embed::runtime::{KernelAssembler, KernelContext, KernelMemory};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::types::AttendanceDeclaration;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

// ── fixture: the conformance hir/1 document (r2_5's shape) ──────────────────

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

/// A `consolidate_memories` rule node — `{scope, cadence}` with the
/// optional `guards`/`retention` members.
fn consolidate_rule_node(seq: u64, rule_id: &str, action: Json) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: rule_id.into(),
                trigger: Json::Null,
                action: RuleAction::ConsolidateMemories(action),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            seq,
        ),
        rule_id,
    )
}

fn document_json(policy_variant: &str, extra_rules: Vec<Node>) -> Json {
    let mut nodes = vec![
        sid(
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
                    accounting: sel("test:agent"),
                }),
                2,
            ),
            "test:budget",
        ),
        sid(
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
        ),
        sid(
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
        ),
    ];
    nodes.extend(extra_rules);
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
            policy_variant,
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = nodes;
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

// ── service helpers (r2_5's shape) ──────────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::{Mutex, OnceLock};
    static DIRS: OnceLock<Mutex<BTreeMap<String, std::path::PathBuf>>> = OnceLock::new();
    let mut m = DIRS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    m.entry(tag.to_string())
        .or_insert_with(|| {
            let d =
                std::env::temp_dir().join(format!("hh-embed-r34-{}-{tag}", std::process::id(),));
            let _ = std::fs::remove_dir_all(&d);
            d
        })
        .clone()
}

fn service(tag: &str) -> EmbedService {
    let root = test_dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r3.4".into(),
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

fn hello(svc: &mut EmbedService) {
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    caps.insert("serves_host_executor".to_string(), Json::Bool(true));
    caps.insert("serves_measurement".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r3.4")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    ok(&r);
}

fn new_spec(definition: Json) -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", definition),
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
            "supplies",
            Json::obj(vec![
                ("context", Json::Arr(vec![])),
                ("host_capabilities", Json::Arr(vec![])),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(vec![])),
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

fn open_new(svc: &mut EmbedService, spec: Json) -> Json {
    ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", spec),
            ("idempotency_key", Json::str("open-1")),
        ]),
    ))
}

fn session_run(s: &Json) -> (String, String) {
    (
        s.get("session_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        s.get("run_id").and_then(Json::as_str).unwrap().to_string(),
    )
}

fn submit(svc: &mut EmbedService, sid: &str, key: &str) -> Json {
    ok(&call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(sid)),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("text")),
                    ("text", Json::str("hello")),
                ])]),
            ),
            ("idempotency_key", Json::str(key)),
        ]),
    ))
}

fn rows(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.surface_events(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

fn seq_of(evs: &[(u64, String, Json)], class: &str) -> Option<u64> {
    evs.iter().find(|(_, c, _)| c == class).map(|(s, _, _)| *s)
}

/// A `ModelProfile` fixture carrying a declared `context_window` (the
/// §5c.1 `window_cap` source — a measured bound, never a tri-state).
fn windowed_profile(
    id: &str,
    version: &str,
    context_window: u64,
) -> hharness_compiler::profile::ModelProfile {
    use hharness_compiler::profile::{
        ExpiryCondition, ExpiryKind, ModelProfile, ModelRole, ProfileCompatibility,
        ProfileDebtRecord, ProfileSelector, VersionPattern,
    };
    let mut p = ModelProfile {
        profile_id: id.to_string(),
        version: version.to_string(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "test-api".to_string(),
            model_family: "test-model".to_string(),
            version_pattern: VersionPattern::Any,
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: hharness_compiler::profile::ProfileCapabilities {
            context_window: Some(context_window),
            ..Default::default()
        },
        rules: vec![],
        ext: std::collections::BTreeMap::new(),
        expiry: ProfileDebtRecord {
            rule_id: format!("{id}.expiry"),
            hypothesis: "the model honours the declared contract".to_string(),
            evidence_refs: vec![hharness_hir::EvidenceRef::legacy("sha256:ev")],
            owner: "test:owner".to_string(),
            reach_via: Vec::new(),
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::Date,
                value: Some("2099-01-01".to_string()),
            },
            removal_test_ref: "sha256:test".to_string(),
            removal_test: Some(hharness_hir::RemovalTest::new(
                hharness_hir::RemovalTestKind::Inspection,
            )),
            status: hharness_compiler::profile::DebtStatus::Active,
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
            inventory_version: "1.0".to_string(),
            min_compiler_version: "0.0.0".to_string(),
        },
        tests: Json::obj([]),
    };
    p.content_hash = hharness_compiler::profile::profile_identity(&p);
    p
}

/// Register a `model_profile` through the boundary — returns the
/// record's `version_id` (a resolvable `profile_binding` coordinate).
fn register_profile(
    svc: &mut EmbedService,
    p: &hharness_compiler::profile::ModelProfile,
) -> String {
    let r = call(
        svc,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("model_profile")),
            (
                "body",
                Json::obj([
                    ("kind", Json::str("model_profile")),
                    ("profile", hharness_compiler::schema::profile_to_json(p)),
                ]),
            ),
            (
                "registrar",
                hharness_provenance::ProvenanceRecord::kernel("hharness-embed", 0).to_json(),
            ),
        ]),
    );
    ok(&r)
        .get("version_id")
        .and_then(Json::as_str)
        .expect("register returns version_id")
        .to_string()
}

// ── the §5c producer legs ───────────────────────────────────────────────────

/// DF-S1.19-1 (non-`default` `ContextPolicy` runtime wiring): the sealed
/// `context_policy` slot binds `hh/priority_eviction`; the durable
/// `context.assembled` row's `policy_ref` is the variant's own declared
/// id — the policy the assembler ran, not a substituted default.
#[test]
fn policy_slot_binds_nondefault_variant() {
    let mut svc = service("policy");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec(document_json("hh/priority_eviction", vec![])),
    );
    let (sid, run) = session_run(&s);
    submit(&mut svc, &sid, "sub-1");
    let evs = rows(&svc, &run);
    let assembled = evs
        .iter()
        .find(|(_, c, _)| c == "context.assembled")
        .unwrap_or_else(|| panic!("no context.assembled: {evs:?}"));
    assert_eq!(
        assembled.2.get("policy_ref").and_then(Json::as_str),
        Some(hharness_context::policy_family::PRIORITY_EVICTION_REF),
        "the assembled row names the bound variant: {:?}",
        assembled.2
    );
}

/// The slot's variant is runtime-bound, never silently substituted: an
/// unresolvable ref is `Refused` at the fold constructor (the arm path —
/// `for_policy` is the only mapping).
#[test]
fn unknown_policy_variant_refuses() {
    let err = KernelContext::for_policy("run-x", "hh/bogus_variant")
        .err()
        .expect("unknown variant must refuse");
    assert!(err.contains("bogus_variant"), "{err}");
    // The declared first-party set resolves — `hh/` and `context_policy/`
    // spellings map to the same half.
    for v in [
        "hh/full_window",
        "context_policy/default",
        "hh/jit_handles",
        "hh/world_state_diff",
    ] {
        assert!(
            KernelContext::for_policy("run-x", v).is_ok(),
            "{v} must resolve"
        );
    }
}

/// DF-S1.19-1 (memory-consolidation watermark events): a declared
/// `session_start` rule fires at the `open` boundary — the durable
/// `context.memory.consolidated` row lands ahead of the first
/// `control.decision`, carrying the watermark members the next cadence
/// gate reads.
#[test]
fn consolidation_session_start_fires_at_open() {
    let mut svc = service("consolidate-open");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec(document_json(
            "hh/full_window",
            vec![consolidate_rule_node(
                5,
                "test:rule.consolidate",
                Json::obj(vec![
                    ("scope", Json::str("session")),
                    ("cadence", Json::str("session_start")),
                ]),
            )],
        )),
    );
    let (sid, run) = session_run(&s);
    submit(&mut svc, &sid, "sub-1");
    let evs = rows(&svc, &run);
    let row = evs
        .iter()
        .find(|(_, c, _)| c == "context.memory.consolidated")
        .unwrap_or_else(|| panic!("no context.memory.consolidated: {evs:?}"));
    let p = &row.2;
    assert_eq!(
        p.get("rule_id").and_then(Json::as_str),
        Some("test:rule.consolidate"),
        "{p:?}"
    );
    assert_eq!(
        p.get("cadence").and_then(Json::as_str),
        Some("session_start"),
        "{p:?}"
    );
    assert_eq!(
        p.get("trigger").and_then(Json::as_str),
        Some("open"),
        "{p:?}"
    );
    assert!(
        p.get("consolidation_id").and_then(Json::as_str).is_some(),
        "{p:?}"
    );
    assert!(
        p.get("input_watermark").and_then(Json::as_int).is_some(),
        "{p:?}"
    );
    assert_eq!(
        p.get("consolidator").and_then(Json::as_str),
        Some("dedupe"),
        "{p:?}"
    );
    // Durable-before-visible: the watermark row lands ahead of the first
    // decision it precedes.
    let decision = seq_of(&evs, "control.decision");
    assert!(
        decision.map(|d| row.0 < d).unwrap_or(true),
        "consolidated precedes the first decision: {evs:?}"
    );
}

/// An `idle`-cadence rule fires at the `finish` boundary — after the
/// scope-floor `context.memory.invalidated` rows (the just-expired
/// versions are the candidates it would rebind) and ahead of
/// `lifecycle.turn.finished`.
#[test]
fn consolidation_idle_fires_at_finish() {
    let mut svc = service("consolidate-finish");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        new_spec(document_json(
            "hh/full_window",
            vec![consolidate_rule_node(
                5,
                "test:rule.idle",
                Json::obj(vec![
                    ("scope", Json::str("run")),
                    ("cadence", Json::str("idle")),
                ]),
            )],
        )),
    );
    let (sid, run) = session_run(&s);
    submit(&mut svc, &sid, "sub-1");
    let evs = rows(&svc, &run);
    let row = evs
        .iter()
        .find(|(_, c, _)| c == "context.memory.consolidated")
        .unwrap_or_else(|| panic!("no context.memory.consolidated: {evs:?}"));
    assert_eq!(
        row.2.get("trigger").and_then(Json::as_str),
        Some("finish"),
        "{:?}",
        row.2
    );
    let turn_finished = seq_of(&evs, "lifecycle.turn.finished")
        .unwrap_or_else(|| panic!("no turn.finished: {evs:?}"));
    assert!(
        row.0 < turn_finished,
        "consolidated lands inside the closing turn: {evs:?}"
    );
    // The run-scope floor's invalidated rows precede the consolidation —
    // the candidates it reads are post-expiry.
    if let Some(inv) = seq_of(&evs, "context.memory.invalidated") {
        assert!(inv < row.0, "scope floor precedes the idle run: {evs:?}");
    }
}

/// Port leg — the `dedupe` consolidator rebinds a stale version's names
/// onto its live replacement and the watermark row records the applied
/// diff (the same `context.memory.consolidated` shape the driver lands).
#[test]
fn consolidate_rebinds_stale_names() {
    let mut ctx = KernelContext::new("run-p");
    let wctx = |seq: u64, lease: u64| hharness_context::WriteContext {
        context_label: hharness_provenance::label::Label::at(AuthorityClass::Principal),
        lease_generation: lease,
        at_seq: seq,
        run_id: "run-p".to_string(),
    };
    let lease_gen = ctx.store.mirror().lease(PersistenceScope::Run);
    // v1 then v2 on the same semantic line — v1 is stale{superseded}.
    let draft =
        |body: &str, semantic: &str| hharness_context::MemoryDraft {
            kind: hharness_context::MemoryKind::Fact,
            subject_key: None,
            content: hharness_context::MemoryContent::Text(Box::new(
                hharness_hir::leaves::Text::new(body, "test", ProvenanceRecord::kernel("test", 0)),
            )),
            contract: None,
            scope: PersistenceScope::Run,
            declared_inputs: vec![],
            justifications: vec![],
            supersedes: None,
            validity: None,
            provenance: Some(ProvenanceRecord::kernel("test", 0)),
            semantic_id: Some(semantic.to_string()),
            validator_endorsed: false,
        };
    let v1 = ctx
        .store
        .port()
        .put(draft("old fact", "facts/a"), &wctx(1, lease_gen))
        .unwrap()
        .version
        .version_id;
    let mut d2 = draft("new fact", "facts/a");
    d2.supersedes = Some(hharness_context::memory::SupersedeClaim {
        version_id: v1.clone(),
        reason: hharness_context::memory::SupersedeClaimReason::Correction,
    });
    let v2 = ctx
        .store
        .port()
        .put(d2, &wctx(2, lease_gen))
        .unwrap()
        .version
        .version_id;
    ctx.store
        .port()
        .bind(PersistenceScope::Run, "facts/a", &v1, None, "seed", 3)
        .unwrap();
    let rule = ConsolidationRule {
        rule_id: "rule/dedupe".into(),
        scope: PersistenceScope::Run,
        cadence: ConsolidationCadence::SessionStart,
        guards: hharness_context::consolidate::ConsolidationGuards::default(),
        consolidator: hharness_context::consolidate::ConsolidatorKind::Dedupe,
        retention: None,
    };
    let mem = std::rc::Rc::new(std::cell::RefCell::new(ctx));
    let mut port = KernelMemory::new(mem.clone());
    let evs = port
        .consolidate(std::slice::from_ref(&rule), "open", None)
        .unwrap();
    let row = evs
        .iter()
        .find(|(c, _)| c == "context.memory.consolidated")
        .unwrap_or_else(|| panic!("no consolidated row: {evs:?}"));
    let applied = match row.1.get("applied") {
        Some(Json::Arr(a)) => a.clone(),
        _ => panic!("no applied member: {:?}", row.1),
    };
    assert_eq!(applied.len(), 1, "{applied:?}");
    assert_eq!(
        applied[0].get("from_version").and_then(Json::as_str),
        Some(v1.as_str())
    );
    assert_eq!(
        applied[0].get("to_version").and_then(Json::as_str),
        Some(v2.as_str())
    );
    // The name resolves to the live replacement, and the watermark is set
    // — a second `session_start` call never re-fires (the durable row is
    // the record the first run already made).
    let manifest = mem
        .borrow()
        .store
        .mirror()
        .manifest(PersistenceScope::Run, 10);
    assert_eq!(manifest.entries.get("facts/a"), Some(&v2));
    let again = port.consolidate(&[rule], "open", None).unwrap();
    assert!(
        again.is_empty(),
        "session_start never re-fires after its watermark: {again:?}"
    );
}

/// DF-S1.19-1 (`MemoryUsage`-fed reclamation): a rule's declared
/// `retention` runs `evaluate_retention` through the fenced store — the
/// `lifecycle.ledger.tier_transition` rows land durable and the
/// consolidated row reports `{retention_policy_ref, bound, reclaimed}`.
/// A stale body unread since the last watermark leaves the bound set
/// (it cools); a stale body still being read stays bound.
#[test]
fn retention_leg_runs_evaluate_retention() {
    let dir = test_dir("retention").join("ledger");
    let clock = ManualClock::at(1_000);
    let mut store = Store::open_with(
        dir,
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = store.open_run(m, "writer-a").unwrap();

    let mut ctx = KernelContext::new(&run);
    // Two memory bodies — one live, one stale-unread (superseded).
    let live = ctx
        .put_artifact(
            &mut store,
            &lease,
            "facts/live",
            "application/json",
            br#"{"v":"live"}"#,
            hharness_context::MemoryKind::Fact,
            PersistenceScope::Session,
        )
        .unwrap();
    let stale = ctx
        .put_artifact(
            &mut store,
            &lease,
            "facts/stale",
            "application/json",
            br#"{"v":"stale"}"#,
            hharness_context::MemoryKind::Fact,
            PersistenceScope::Session,
        )
        .unwrap();
    // Supersede the stale body — same semantic line, a correction.
    let wctx = hharness_context::WriteContext {
        context_label: hharness_provenance::label::Label::at(AuthorityClass::Principal),
        lease_generation: ctx.store.mirror().lease(PersistenceScope::Session),
        at_seq: 90,
        run_id: run.clone(),
    };
    let mut d = hharness_context::MemoryDraft {
        kind: hharness_context::MemoryKind::Fact,
        subject_key: None,
        content: hharness_context::MemoryContent::Text(Box::new(hharness_hir::leaves::Text::new(
            "replacement",
            "test",
            ProvenanceRecord::kernel("test", 0),
        ))),
        contract: Some(hharness_context::InvalidationContract {
            dependencies: vec![],
            cache_hint: hharness_context::CacheHint::Cacheable,
            validator_ref: None,
            freshness: None,
            invalidation_condition: Some(hharness_context::InvalidationCondition::ScopeEnded(
                PersistenceScope::Session,
            )),
            revalidation: hharness_context::Revalidation::Never,
        }),
        scope: PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: Some(hharness_context::memory::SupersedeClaim {
            version_id: stale.clone(),
            reason: hharness_context::memory::SupersedeClaimReason::Correction,
        }),
        validity: None,
        provenance: Some(ProvenanceRecord::kernel("test", 0)),
        semantic_id: Some("facts/stale".to_string()),
        validator_endorsed: false,
    };
    let _ = &mut d;
    let _ = ctx.store.port().put(d, &wctx).unwrap();
    assert_ne!(live, stale);

    let rule = ConsolidationRule {
        rule_id: "rule/reclaim".into(),
        scope: PersistenceScope::Session,
        cadence: ConsolidationCadence::Idle,
        guards: hharness_context::consolidate::ConsolidationGuards::default(),
        consolidator: hharness_context::consolidate::ConsolidatorKind::Dedupe,
        retention: Some(hharness_ledger::retention::RetentionPolicy {
            policy_ref: "policy:r34-test".into(),
            blob_residency_ms: 0,
            bundle_retention_ms: 0,
            compress_cold: true,
            legal_hold: false,
        }),
    };
    let mem = std::rc::Rc::new(std::cell::RefCell::new(ctx));
    let mut port = KernelMemory::new(mem);
    use hharness_control::driver::MemoryPort;
    let evs = port
        .consolidate(
            &[rule],
            "finish",
            Some(hharness_control::driver::StoreSeam {
                store: &mut store,
                lease: &lease,
            }),
        )
        .unwrap();
    let row = evs
        .iter()
        .find(|(c, _)| c == "context.memory.consolidated")
        .unwrap_or_else(|| panic!("no consolidated row: {evs:?}"));
    assert_eq!(
        row.1.get("retention_policy_ref").and_then(Json::as_str),
        Some("policy:r34-test"),
        "{:?}",
        row.1
    );
    assert!(
        row.1.get("bound").and_then(Json::as_int).is_some(),
        "{:?}",
        row.1
    );
    let reclaimed = row.1.get("reclaimed").and_then(Json::as_int).unwrap_or(-1);
    assert!(
        reclaimed >= 1,
        "the first pass observes the pool: {reclaimed}"
    );
    // The scheduler's own rows landed durable under the same writer
    // lease — the consolidated row only reports them (CC3).
    let transitions = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "lifecycle.ledger.tier_transition")
        .count();
    assert_eq!(
        transitions, reclaimed as usize,
        "every reported transition is a durable row"
    );
}

/// DF-S1.19-1 (hosted/proxy memory halves): `KernelStore::Remote` binds a
/// `plugin_abi/1` `StoreChannel` — mutations route remote-first and the
/// deterministic mirror serves reads; the indexed retrieval pipeline
/// stamps one `record_read` per side (the far side's `memory_usage` fold
/// agrees with the client's).
#[test]
fn remote_store_round_trip() {
    use hharness_context::memory_abi::{ChannelFault, StoreChannel};
    use std::cell::RefCell;
    use std::rc::Rc;

    // The far side — an in-process `MemoryStore` behind the channel (the
    /// packaged `hh-memory-store` subprocess runs the same `dispatch`;
    /// the loopback exercises the codec + mirror discipline without a
    /// subprocess).
    struct Loop(Rc<RefCell<hharness_context::MemoryStore>>);
    impl StoreChannel for Loop {
        fn invoke(
            &mut self,
            operation: &str,
            inputs: Vec<Json>,
        ) -> Result<Vec<Json>, ChannelFault> {
            let mut far = self.0.borrow_mut();
            hharness_context::memory_abi::dispatch(&mut *far, operation, &inputs)
                .map_err(ChannelFault::Decode)
        }
    }

    let far = Rc::new(RefCell::new(hharness_context::MemoryStore::new(
        "memory_store",
    )));
    let ctx =
        KernelContext::new_remote("run-r", "hh/full_window", Box::new(Loop(far.clone()))).unwrap();
    let mem = Rc::new(RefCell::new(ctx));
    let mut port = KernelMemory::new(mem.clone());

    // A write routes remote-first — the mirror and the far side agree.
    {
        let mut c = mem.borrow_mut();
        let wctx = hharness_context::WriteContext {
            context_label: hharness_provenance::label::Label::at(AuthorityClass::Principal),
            lease_generation: 0,
            at_seq: 1,
            run_id: "run-r".to_string(),
        };
        let out = c
            .store
            .port()
            .put(
                hharness_context::MemoryDraft {
                    kind: hharness_context::MemoryKind::Fact,
                    subject_key: None,
                    content: hharness_context::MemoryContent::Text(Box::new(
                        hharness_hir::leaves::Text::new(
                            "remote fact",
                            "test",
                            ProvenanceRecord::kernel("test", 0),
                        ),
                    )),
                    contract: Some(hharness_context::InvalidationContract {
                        dependencies: vec![],
                        cache_hint: hharness_context::CacheHint::Cacheable,
                        validator_ref: None,
                        freshness: None,
                        invalidation_condition: Some(
                            hharness_context::InvalidationCondition::ScopeEnded(
                                PersistenceScope::Session,
                            ),
                        ),
                        revalidation: hharness_context::Revalidation::Never,
                    }),
                    scope: PersistenceScope::Session,
                    declared_inputs: vec![],
                    justifications: vec![],
                    supersedes: None,
                    validity: None,
                    provenance: Some(ProvenanceRecord::kernel("test", 0)),
                    semantic_id: Some("facts/remote".to_string()),
                    validator_endorsed: false,
                },
                &wctx,
            )
            .unwrap();
        let vid = out.version.version_id;
        assert!(
            c.store.mirror().version(&vid).is_some(),
            "the mirror folds the mutation"
        );
        assert!(
            far.borrow().version(&vid).is_some(),
            "the far side applied it remote-first"
        );
    }

    // The retrieval leg runs over the same port — `by_name` resolves on
    // the mirror; the read stamps both sides (one record_read each).
    {
        let mut c = mem.borrow_mut();
        let vid = c.store.mirror().version_order().last().cloned().unwrap();
        c.store
            .port()
            .bind(
                PersistenceScope::Session,
                "facts/remote",
                &vid,
                None,
                "test",
                2,
            )
            .unwrap();
    }
    let evs = port
        .retrieve(
            &Json::obj(vec![
                ("kind", Json::str("by_name")),
                ("scope", Json::str("session")),
                ("name", Json::str("facts/remote")),
            ]),
            "mc-1",
            ("run-r".to_string(), 4),
        )
        .unwrap();
    assert!(
        evs.iter().any(|(c, _)| c == "context.retrieval.completed"),
        "{evs:?}"
    );
    let reads: Vec<u64> = far
        .borrow()
        .version_order()
        .iter()
        .flat_map(|v| far.borrow().reads_of(v).to_vec())
        .collect();
    assert!(
        !reads.is_empty(),
        "the far side recorded the forwarded read"
    );
}

/// DF-S1.19-1 (`by_run` producer paths beyond `resume_set`): every
/// `context.memory.written` write registers the artifact-layer projection
/// the `by_run{run_id, classes}` query enumerates — the durable read row
/// names the hit.
#[test]
fn by_run_query_hits_artifact_projection() {
    let dir = test_dir("byrun").join("ledger");
    let mut store = Store::open_with(
        dir,
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = store.open_run(m, "writer-a").unwrap();

    let mut ctx = KernelContext::new(&run);
    ctx.put_artifact(
        &mut store,
        &lease,
        "facts/byrun",
        "application/json",
        br#"{"body":"by_run projection"}"#,
        hharness_context::MemoryKind::Fact,
        PersistenceScope::Session,
    )
    .unwrap();
    let mem = std::rc::Rc::new(std::cell::RefCell::new(ctx));
    let mut port = KernelMemory::new(mem.clone());
    let evs = port
        .retrieve(
            &Json::obj(vec![
                ("kind", Json::str("by_run")),
                ("run_id", Json::str(run.clone())),
            ]),
            "mc-2",
            (run.clone(), 8),
        )
        .unwrap();
    let completed = evs
        .iter()
        .find(|(c, _)| c == "context.retrieval.completed")
        .unwrap_or_else(|| panic!("no retrieval.completed: {evs:?}"));
    assert_eq!(
        completed.1.get("query_kind").and_then(Json::as_str),
        Some("by_run"),
        "{:?}",
        completed.1
    );
    let read = evs
        .iter()
        .find(|(c, _)| c == "context.memory.read")
        .unwrap_or_else(|| panic!("no memory.read: {evs:?}"));
    let delivered = match read.1.get("delivered") {
        Some(Json::Arr(a)) => a.clone(),
        _ => panic!("no delivered member: {:?}", read.1),
    };
    assert!(
        delivered
            .iter()
            .filter_map(Json::as_str)
            .any(|a| a.starts_with("sha256:")),
        "the artifact projection delivers: {delivered:?}"
    );
}

/// The resume fold re-resolves `content_ref` bodies (R3.4's fold half):
/// a folded version carries the real `index_text` — the lexical index
/// and a `by_name` retrieval see the seeded body across a restart, never
/// a skeleton.
#[test]
fn resume_fold_rehydrates_body_and_watermark() {
    let dir = test_dir("fold").join("ledger");
    let mut store = Store::open_with(
        dir,
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = store.open_run(m, "writer-a").unwrap();

    // The first arm — a write plus a session-start consolidation row.
    {
        let mut ctx = KernelContext::new(&run);
        ctx.put_artifact(
            &mut store,
            &lease,
            "facts/fold",
            "application/json",
            br#"{"body":"fold rehydrate marker"}"#,
            hharness_context::MemoryKind::Fact,
            PersistenceScope::Session,
        )
        .unwrap();
        let rule = ConsolidationRule {
            rule_id: "rule/wm".into(),
            scope: PersistenceScope::Session,
            cadence: ConsolidationCadence::SessionStart,
            guards: hharness_context::consolidate::ConsolidationGuards::default(),
            consolidator: hharness_context::consolidate::ConsolidatorKind::Dedupe,
            retention: None,
        };
        let mem = std::rc::Rc::new(std::cell::RefCell::new(ctx));
        let mut port = KernelMemory::new(mem);
        let evs = port.consolidate(&[rule], "open", None).unwrap();
        // Land the produced rows the way the driver would — through the
        // fenced append (the test reproduces the driver's append loop).
        for (class, payload) in evs {
            let ts = store.ts_now();
            let parent = store
                .events(&run)
                .ok()
                .and_then(|es| es.last())
                .map(|e| e.event_id.clone())
                .unwrap_or_else(|| hharness_ledger::ids::ROOT_EVENT.to_string());
            let mut ev = hharness_control::events::kernel_event(
                store.alloc_id("evt"),
                &class,
                ts,
                hharness_ledger::event::Scope::default(),
                parent,
                vec![],
                payload,
            );
            ev.provenance = Some(ProvenanceRecord::kernel("r3.4", store.now_ms()));
            store.append(&run, &lease, vec![ev]).unwrap();
        }
    }

    // The resumed arm — the fold rehydrates the body + the watermark.
    let prefix = store.events(&run).unwrap().to_vec();
    let folded = KernelContext::fold(&run, &prefix, &store, store.now_ms());
    assert!(
        folded
            .consolidation_watermarks
            .get("rule/wm")
            .copied()
            .is_some(),
        "the durable consolidated row re-derives the watermark"
    );
    let (text, applied) = {
        let mirror = folded.store.mirror();
        let vids = mirror.version_order();
        assert_eq!(vids.len(), 1, "{vids:?}");
        (
            mirror.version(&vids[0]).unwrap().content.index_text(),
            mirror.applied_seq(),
        )
    };
    assert!(
        text.contains("fold rehydrate marker"),
        "the body re-resolves through content_ref: {text:?}"
    );
    // The session-start gate sees the watermark — the resumed arm never
    // re-fires the same session's start.
    let rule = ConsolidationRule {
        rule_id: "rule/wm".into(),
        scope: PersistenceScope::Session,
        cadence: ConsolidationCadence::SessionStart,
        guards: hharness_context::consolidate::ConsolidationGuards::default(),
        consolidator: hharness_context::consolidate::ConsolidatorKind::Dedupe,
        retention: None,
    };
    let mem = std::rc::Rc::new(std::cell::RefCell::new(folded));
    let mut port = KernelMemory::new(mem.clone());
    let again = port.consolidate(&[rule], "open", None).unwrap();
    assert!(
        again.is_empty(),
        "a resumed arm's session_start never re-fires: {again:?}"
    );
    // And the lexical index folds the rehydrated body — `lexical` finds
    // the marker across the restart (the indexed pipeline's input).
    let lex = hharness_context::retrieve::lexical_index(mem.borrow().store.mirror(), applied);
    assert!(
        lex.postings
            .get("rehydrate")
            .map(|ids| !ids.is_empty())
            .unwrap_or(false),
        "the body is in the lexical fold: {:?}",
        lex.postings.keys().collect::<Vec<_>>()
    );
}

/// DF-S1.19-2 (the `ContextWindowExceeded → stop` stress leg): the
/// primary profile's declared `context_window` is the §5c.1
/// `window_cap` — a cap under the kernel's always-reserved omission
/// tokens makes the required set alone overflow. The real builder
/// answers `ContextWindowExceeded`, the assembled row records the typed
/// error, the driver routes the signal to `stop{context_exhausted}`,
/// and the model call never fires on a request the window cannot hold.
#[test]
fn context_window_exceeded_stops_the_run() {
    let mut svc = service("win_stop");
    hello(&mut svc);
    // `context_window: 1` < `OMISSION_ITEM_TOKENS` (32) — the kernel's
    // mandatory reserve alone exceeds the declared cap.
    let p = windowed_profile("test:profile.tiny", "1", 1);
    let vid = register_profile(&mut svc, &p);
    let mut spec = new_spec(document_json("hh/full_window", vec![]));
    if let Json::Obj(m) = &mut spec {
        m.insert(
            "profile_binding".to_string(),
            Json::obj([("primary", Json::str(vid))]),
        );
    }
    let s = open_new(&mut svc, spec);
    let (sid, run) = session_run(&s);
    // The durable `model_context_window` member carries the realized
    // cap — `DriverConfig.window_cap_tokens` reads it at arm.
    let m = svc.store().manifest(&run).unwrap();
    assert_eq!(
        m.extra.get("model_context_window").and_then(Json::as_int),
        Some(1),
        "{:?}",
        m.extra
    );
    submit(&mut svc, &sid, "sub-1");
    let evs = rows(&svc, &run);
    // The builder's typed overflow is durable — never a silent proceed.
    let asm = evs
        .iter()
        .find(|(_, c, _)| c == "context.assembled")
        .unwrap_or_else(|| panic!("no assembled row: {evs:?}"));
    let err = asm
        .2
        .get("assembly_error")
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(
        err.contains("ContextWindowExceeded"),
        "assembly_error: {err:?}"
    );
    // `→ stop`: the envelope's `control.decision{kind: stop}` names the
    // closed `context_exhausted{required_tokens, cap}` spelling.
    let stop = evs
        .iter()
        .rev()
        .find(|(_, c, p)| {
            c == "control.decision" && p.get("kind").and_then(Json::as_str) == Some("stop")
        })
        .unwrap_or_else(|| panic!("no stop decision: {evs:?}"));
    assert_eq!(
        stop.2
            .get("reason")
            .and_then(|r| r.get("kind"))
            .and_then(Json::as_str),
        Some("context_exhausted"),
        "{:?}",
        stop.2
    );
    assert_eq!(
        stop.2
            .get("reason")
            .and_then(|r| r.get("cap"))
            .and_then(Json::as_int),
        Some(1),
        "{:?}",
        stop.2
    );
    let fin = evs
        .iter()
        .find(|(_, c, _)| c == "lifecycle.run.finished")
        .unwrap_or_else(|| panic!("run did not finish: {evs:?}"));
    assert_eq!(
        fin.2
            .get("stop_reason")
            .and_then(|r| r.get("kind"))
            .and_then(Json::as_str),
        Some("context_exhausted"),
        "{:?}",
        fin.2
    );
    // The model call never fires — no `model.*` row lands between the
    // assembled row and the stop.
    let asm_seq = asm.0;
    assert!(
        !evs.iter()
            .any(|(sq, c, _)| *sq > asm_seq && c.starts_with("model.")),
        "a model call fired after the overflow: {evs:?}"
    );
}

// ── the DF-S1.19-2 battery cells (golden-plan / stress / occupancy) ─────────

/// Append `n` `context.observation.recorded` rows under the run's lease —
/// the durable prefix `KernelAssembler` folds its `Observation`
/// candidates from. `pad` sizes each payload (the pinned
/// `canonical_bytes/4` estimator makes the row's token cost a real
/// function of its bytes — never a synthetic estimate).
fn seed_observations(
    store: &mut Store,
    run: &str,
    lease: &hharness_ledger::store::Lease,
    n: usize,
    pad: usize,
) {
    for i in 0..n {
        let parent = store
            .events(run)
            .ok()
            .and_then(|es| es.last())
            .map(|e| e.event_id.clone())
            .unwrap_or_else(|| hharness_ledger::ids::ROOT_EVENT.to_string());
        let mut ev = hharness_control::events::kernel_event(
            store.alloc_id("evt"),
            "context.observation.recorded",
            store.ts_now(),
            hharness_ledger::event::Scope::default(),
            parent,
            vec![],
            Json::obj([
                ("obs", Json::str(format!("obs-{i:03}"))),
                ("pad", Json::str("x".repeat(pad))),
            ]),
        );
        ev.provenance = Some(ProvenanceRecord::kernel("r3.4", store.now_ms()));
        store.append(run, lease, vec![ev]).unwrap();
    }
}

fn obs_run(tag: &str, n: usize, pad: usize) -> (Store, String, hharness_ledger::store::Lease) {
    let dir = test_dir(tag).join("ledger");
    let mut store = Store::open_with(
        dir,
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = store.open_run(m, "writer-a").unwrap();
    seed_observations(&mut store, &run, &lease, n, pad);
    (store, run, lease)
}

/// DF-S1.19-2 (renderer/integration/golden-plan): the same durable
/// prefix assembles to the *byte-identical* plan through the real
/// `KernelAssembler` port — `plan_id`, `static_hash`, `context_plan`
/// body and `occupancy_estimate` are I-DET stable; the `context.assembled`
/// payload is the plan's canonical record (the renderer never invents
/// content — every `items[]` row is a planned item).
#[test]
fn golden_plan_reproduces_over_the_real_port() {
    let (store, run, _lease) = obs_run("golden", 8, 120);
    let prefix = store.events(&run).unwrap().to_vec();
    let mut asm = KernelAssembler {
        ctx: None,
        ..Default::default()
    };
    let inputs = AssembleInputs {
        model_call_id: "mc-1",
        prefix: &prefix,
        window_cap_tokens: 8_192,
    };
    let req = Json::obj([("context_request", Json::Null)]);
    let a = asm.assemble(&inputs, &req);
    let b = asm.assemble(&inputs, &req);
    let pa = a.assembled_payload.unwrap();
    let pb = b.assembled_payload.unwrap();
    assert_eq!(
        pa.get("plan_id").and_then(Json::as_str),
        pb.get("plan_id").and_then(Json::as_str),
        "plan_id is deterministic over the same prefix"
    );
    assert_eq!(
        pa.get("static_hash").and_then(Json::as_str),
        pb.get("static_hash").and_then(Json::as_str),
    );
    // The golden half — the plan body the renderer consumes is
    // byte-identical across assembles (canonical JSON string equality).
    let plan_a = a.request.get("context_plan").unwrap().to_canonical_string();
    let plan_b = b.request.get("context_plan").unwrap().to_canonical_string();
    assert_eq!(plan_a, plan_b, "the plan body is a golden projection");
    // Every `items[]` row is a planned item with a slot + a real token
    // count — the renderer reads, never fabricates.
    let items = match pa.get("items") {
        Some(Json::Arr(a)) => a.clone(),
        _ => panic!("no items member: {pa:?}"),
    };
    assert_eq!(items.len(), 8, "every observation planned: {pa:?}");
    for it in &items {
        assert!(it.get("slot_id").and_then(Json::as_str).is_some(), "{it:?}");
        assert!(
            it.get("tokens").and_then(Json::as_int).unwrap_or(0) > 0,
            "{it:?}"
        );
        assert!(
            it.get("context_item_id")
                .and_then(Json::as_str)
                .map(|s| s.starts_with("sha256:"))
                .unwrap_or(false),
            "compiled identity: {it:?}"
        );
    }
}

/// DF-S1.19-2 (the ≥N-candidate stress fixture): 48 `Observation`
/// candidates against a window that fits a strict subset — the evict
/// order is deterministic (a second assemble reproduces `plan_id`),
/// `occupancy_estimate ≤ cap`, and the durable row's accounting closes:
/// every candidate lands in `items[]` or `omitted[]`, never silently
/// dropped.
#[test]
fn stress_candidates_evict_deterministically_and_account() {
    let (store, run, _lease) = obs_run("stress", 48, 160);
    let prefix = store.events(&run).unwrap().to_vec();
    let cap = 400u64;
    let mut asm = KernelAssembler {
        ctx: None,
        ..Default::default()
    };
    let inputs = AssembleInputs {
        model_call_id: "mc-s",
        prefix: &prefix,
        window_cap_tokens: cap,
    };
    let req = Json::obj([("context_request", Json::Null)]);
    let a = asm.assemble(&inputs, &req);
    let pa = match a.assembled_payload {
        Some(p) => p,
        None => panic!("overflow under optional candidates: {a:?}"),
    };
    let b = asm.assemble(&inputs, &req);
    assert_eq!(
        pa.get("plan_id").and_then(Json::as_str),
        b.assembled_payload
            .unwrap()
            .get("plan_id")
            .and_then(Json::as_str),
        "the stress eviction is deterministic"
    );
    let occupancy = pa
        .get("occupancy_estimate")
        .and_then(Json::as_int)
        .unwrap_or(-1);
    assert!(
        occupancy >= 0 && (occupancy as u64) <= cap,
        "occupancy {occupancy} exceeds cap {cap}: {pa:?}"
    );
    let items: Vec<&Json> = match pa.get("items") {
        Some(Json::Arr(v)) => v.iter().collect(),
        _ => vec![],
    };
    let omitted: Vec<&Json> = match pa.get("omitted") {
        Some(Json::Arr(v)) => v.iter().collect(),
        _ => vec![],
    };
    assert!(
        items.len() < 48 && !omitted.is_empty(),
        "the cap forced a real eviction: {} items, {} omitted",
        items.len(),
        omitted.len()
    );
    // Accounting closure — the durable row covers all 48 candidates.
    // (The `mc-s:omission` item is the kernel's own always-planned
    // omission reserve — accounted by `reserved`, not a candidate.)
    let covered: std::collections::BTreeSet<String> = items
        .iter()
        .chain(omitted.iter())
        .filter_map(|i| i.get("candidate_id").and_then(Json::as_str))
        .filter(|c| *c != "mc-s:omission")
        .map(str::to_string)
        .collect();
    let expected: std::collections::BTreeSet<String> = prefix
        .iter()
        .filter(|e| e.class == "context.observation.recorded")
        .map(|e| format!("mc-s:obs:{}", e.event_id))
        .collect();
    assert_eq!(
        covered, expected,
        "every candidate is admitted or accounted: {pa:?}"
    );
    // Every omission names its reason + evidence — the spec's "accounted"
    // member, never an unexplained drop.
    for o in &omitted {
        assert!(o.get("reason").and_then(Json::as_str).is_some(), "{o:?}");
        assert!(o.get("evidence").and_then(Json::as_str).is_some(), "{o:?}");
    }
}

/// DF-S1.19-2 (per-slot occupancy projections): the durable
/// `context.assembled` row projects per-slot occupancy — `items[]`'s
/// `{slot_id, tokens}` fold reproduces each `plan.slots[i]` sum and the
/// `occupancy_estimate` total; `reserved` names the kernel's omission
/// reserve (the numbers come from the row, never an estimate presented
/// as a measurement).
#[test]
fn per_slot_occupancy_projects_from_the_durable_row() {
    let (store, run, _lease) = obs_run("occupancy", 10, 100);
    let prefix = store.events(&run).unwrap().to_vec();
    let mut asm = KernelAssembler {
        ctx: None,
        ..Default::default()
    };
    let inputs = AssembleInputs {
        model_call_id: "mc-o",
        prefix: &prefix,
        window_cap_tokens: 8_192,
    };
    let out = asm.assemble(&inputs, &Json::obj([("context_request", Json::Null)]));
    let payload = out.assembled_payload.unwrap();
    // The per-slot projection — a pure fold over the row's items.
    let mut slot_sums: std::collections::BTreeMap<String, i64> = Default::default();
    match payload.get("items") {
        Some(Json::Arr(items)) => {
            for it in items {
                *slot_sums
                    .entry(
                        it.get("slot_id")
                            .and_then(Json::as_str)
                            .unwrap()
                            .to_string(),
                    )
                    .or_default() += it.get("tokens").and_then(Json::as_int).unwrap_or(0);
            }
        }
        _ => panic!("no items: {payload:?}"),
    }
    // …against the plan's own per-slot sums (the canonical body's slots).
    let plan = out.request.get("context_plan").unwrap();
    let plan_slots = match plan.get("slots") {
        Some(Json::Arr(s)) => s.clone(),
        _ => panic!("no slots: {plan:?}"),
    };
    let mut expected: std::collections::BTreeMap<String, i64> = Default::default();
    for slot in &plan_slots {
        let sid = slot
            .get("slot_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        let sum: i64 = match slot.get("items") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| i.get("tokens").and_then(Json::as_int).unwrap_or(0))
                .sum(),
            _ => 0,
        };
        expected.insert(sid, sum);
    }
    assert_eq!(
        slot_sums, expected,
        "the per-slot projection equals the plan's own sums"
    );
    // …and the totals close on `occupancy_estimate`.
    let total: i64 = slot_sums.values().sum();
    assert_eq!(
        Some(total),
        payload.get("occupancy_estimate").and_then(Json::as_int),
        "{payload:?}"
    );
    assert_eq!(
        payload.get("reserved").and_then(Json::as_int),
        Some(hharness_context::OMISSION_ITEM_TOKENS as i64),
        "the kernel's omission reserve is named on the row: {payload:?}"
    );
}
