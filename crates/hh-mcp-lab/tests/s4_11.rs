//! S4.11 — the C1 `hh-lab/1` MCP-server slice conformance
//! (AC-R-2.11.3-{3–5,7–9,12–13}; R-2.11.3¹; ADR-0173/0174/0175,
//! ADR-0184 D2, ADR-0303).
//!
//! The battery drives [`LabServer::handle_message`] in-process over a
//! real `EmbedService` + ledger store (a tempdir per test — the same
//! fixture pattern as `hh-embed/tests/conformance.rs`), plus the two
//! transports: `serve_stdio` over memory buffers and `serve_http` over
//! a loopback listener with bearer grants.
//!
//! Coverage map:
//! - AC-3: `respond_approval` under an `agent` binding refuses
//!   `IllegitimateEndorsement` with the durable `refused` row; a
//!   `human_principal` binding passes the caller-kind gate.
//! - AC-4: `launch_run` without `budget` → `MissingBudget`; over the
//!   pool → `InsufficientBudget` before a child run exists; a
//!   successful launch carries `spawn_event` onto the child manifest.
//! - AC-5: every surface-session turn posts `control.budget.consumed`
//!   `charged_to = instrument`; `run_status.account` is the `Account`
//!   projection.
//! - AC-7: `read_ledger` serves the policy-folded page and mints
//!   `measurement.export.delivered` on the read run; a non-run is
//!   `UnknownHandle`, never an empty page.
//! - AC-8: a `launch_run` replay under the same `idempotency_key`
//!   returns the recorded handle — no second `open_session`.
//! - AC-9: a supply-surface binding's `deny`/`ask`/`hidden` Π rows —
//!   `callable ⇔ revealed` over its own `tools/list`.
//! - AC-12: `link` refuses an exposure document missing a declared
//!   carrier's assumption-debt record.
//! - AC-13: every Lab tool wraps a real `hh-embed/1` op or is a
//!   declared run-envelope lowering with a loss class.
//! - AC-11 (in-process halves): the stdio loop and the Streamable HTTP
//!   arm run the identical dispatch (frames, session header, `401`).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{BufReader, Cursor, Read, Write};

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_mcp_lab::binding::{stdio_launch_binding, CallerBinding, CallerKind};
use hh_mcp_lab::exposure;
use hh_mcp_lab::server::{serve_stdio, LabServer};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── store/service fixtures ──────────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-{}-{tag}-{}",
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
        holder: "s4-11".into(),
    })
    .unwrap()
}

fn lab() -> LabServer {
    LabServer::with_default_exposure(service())
}

fn agent() -> CallerBinding {
    stdio_launch_binding("test-agent", CallerKind::Agent)
}

fn human() -> CallerBinding {
    stdio_launch_binding("test-human", CallerKind::HumanPrincipal)
}

// ── the hir/1 fixture document (verbatim the hh-embed conformance
// shape — agent + rule + budget + permission + the Stage-1 assembly) ─

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
                    "tool_calls".to_string(),
                    DimensionBound {
                        hard: Some(1_000),
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

// ── protocol helpers ────────────────────────────────────────────────

fn msg(id: i64, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::Int(id)),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

/// `tools/call{name, arguments}` → the `result` member.
fn call(srv: &mut LabServer, binding: &CallerBinding, name: &str, arguments: Json) -> Json {
    let frame = srv
        .handle_message(
            binding,
            &msg(
                7,
                "tools/call",
                Json::obj([("name", Json::str(name)), ("arguments", arguments)]),
            ),
        )
        .expect("request frame");
    frame
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("no result: {}", frame.to_canonical_string()))
}

fn is_err(result: &Json) -> bool {
    matches!(result.get("isError"), Some(Json::Bool(true)))
}

/// The `structuredContent.surface_error.kind` spelling.
fn refusal_kind(result: &Json) -> String {
    result
        .get("structuredContent")
        .and_then(|s| s.get("surface_error"))
        .and_then(|e| e.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

fn structured(result: &Json) -> &Json {
    result.get("structuredContent").unwrap_or(&Json::Null)
}

/// The durable `(class, payload)` rows on `run_id`.
fn rows(srv: &mut LabServer, run_id: &str) -> Vec<(String, Json)> {
    srv.svc
        .surface_events(run_id)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

fn surface_run(srv: &LabServer, binding: &CallerBinding) -> String {
    srv.sessions
        .get(&binding.binding_id)
        .expect("session opened")
        .run_id
        .clone()
}

fn launch_args(idem: &str) -> Json {
    Json::obj([
        (
            "definition",
            Json::obj([
                ("kind", Json::str("document")),
                ("document", document_json()),
            ]),
        ),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "budget",
            Json::obj([(
                "dimensions",
                Json::obj([("tool_calls", Json::obj([("hard", Json::Int(10))]))]),
            )]),
        ),
        ("idempotency_key", Json::str(idem)),
    ])
}

/// Launch a run through the surface; return `(child_run_id, handle)`.
fn launch(srv: &mut LabServer, binding: &CallerBinding, idem: &str) -> (String, String) {
    let r = call(srv, binding, "launch_run", launch_args(idem));
    assert!(
        !is_err(&r),
        "launch_run refused: {}",
        r.to_canonical_string()
    );
    let sc = structured(&r);
    let run_id = sc
        .get("launched")
        .and_then(|l| l.get("run_id"))
        .and_then(Json::as_str)
        .expect("launched.run_id")
        .to_string();
    let handle = sc
        .get("handles")
        .and_then(|h| match h {
            Json::Arr(a) => a.first(),
            _ => None,
        })
        .and_then(|h| h.get("handle"))
        .and_then(Json::as_str)
        .expect("handles[0].handle")
        .to_string();
    (run_id, handle)
}

// ── AC-3 — the caller-kind gate ─────────────────────────────────────

/// AC-R-2.11.3-3 — an `agent` binding's `respond_approval` refuses
/// `IllegitimateEndorsement` *with the durable refused row*; the same
/// call from a `human_principal` binding passes the caller-kind gate
/// (reaches the op — its refusal is the op's own, never the gate's).
#[test]
fn ac3_respond_approval_caller_kind_gate() {
    let mut srv = lab();
    let a = agent();
    let r = call(
        &mut srv,
        &a,
        "respond_approval",
        Json::obj([
            ("permission_id", Json::str("perm-1")),
            ("outcome", Json::str("approved")),
            ("idempotency_key", Json::str("resp-1")),
        ]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "IllegitimateEndorsement");
    // The durable record: the agent's surface run carries the turn +
    // intended + refused{reason: IllegitimateEndorsement}.
    let srun = surface_run(&srv, &a);
    let classes: Vec<String> = rows(&mut srv, &srun)
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    assert!(
        classes.contains(&"lifecycle.turn.started".to_string()),
        "{classes:?}"
    );
    assert!(
        classes.contains(&"action.effect.intended".to_string()),
        "{classes:?}"
    );
    let refused = rows(&mut srv, &srun)
        .into_iter()
        .filter(|(c, _)| c == "action.effect.refused")
        .collect::<Vec<_>>();
    assert!(
        refused
            .iter()
            .any(|(_, p)| p.get("reason").and_then(Json::as_str)
                == Some("IllegitimateEndorsement")),
        "refused row names the endorsement refusal: {refused:?}"
    );

    // The human leg — the gate passes; the refusal the op returns is
    // its own (`schema_violation`/`unknown_session` — never the
    // caller-kind name).
    let h = human();
    let r = call(
        &mut srv,
        &h,
        "respond_approval",
        Json::obj([
            ("permission_id", Json::str("perm-1")),
            ("outcome", Json::str("approved")),
            ("idempotency_key", Json::str("resp-2")),
        ]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_ne!(
        refusal_kind(&r),
        "IllegitimateEndorsement",
        "a human_principal binding passes the caller-kind gate: {r:?}"
    );
}

// ── AC-4 — budget gate + launch causality ───────────────────────────

/// AC-R-2.11.3-4 — `launch_run` sans `budget` refuses `MissingBudget`;
/// a head over the pool's remaining refuses `InsufficientBudget`;
/// neither lands a child run nor an `environment.*` row.
#[test]
fn ac4_missing_and_insufficient_budget() {
    let mut srv = lab();
    let a = agent();
    let runs_before = srv.svc.surface_run_ids().len();

    // No `budget` member at all.
    let mut args = launch_args("m-1");
    if let Json::Obj(m) = &mut args {
        m.remove("budget");
    }
    let r = call(&mut srv, &a, "launch_run", args);
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "MissingBudget");

    // A dimension the pool does not declare.
    let mut args = launch_args("m-2");
    if let Json::Obj(m) = &mut args {
        m.insert(
            "budget".into(),
            Json::obj([(
                "dimensions",
                Json::obj([("gpu_seconds", Json::obj([("hard", Json::Int(5))]))]),
            )]),
        );
    }
    let r = call(&mut srv, &a, "launch_run", args);
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "MissingBudget");

    // A declared head the pool cannot cover (pool: tool_calls 100_000,
    // minus the calls already consumed).
    let mut args = launch_args("m-3");
    if let Json::Obj(m) = &mut args {
        m.insert(
            "budget".into(),
            Json::obj([(
                "dimensions",
                Json::obj([("tool_calls", Json::obj([("hard", Json::Int(999_999))]))]),
            )]),
        );
    }
    let r = call(&mut srv, &a, "launch_run", args);
    assert!(is_err(&r), "{r:?}");
    assert_eq!(
        refusal_kind(&r),
        "InsufficientBudget",
        "over-pool head refuses: {r:?}"
    );

    // No child run, no `environment.*` anywhere — the refusal precedes
    // `open_session` entirely.
    assert_eq!(
        srv.svc.surface_run_ids().len(),
        runs_before + 1,
        "only the caller's own surface run exists"
    );
    let srun = surface_run(&srv, &a);
    let classes: Vec<String> = rows(&mut srv, &srun)
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    assert!(
        !classes.iter().any(|c| c.starts_with("environment.")),
        "a refused launch touches no environment rows: {classes:?}"
    );
}

/// AC-R-2.11.3-4/-8 — a successful `launch_run` carries the
/// `spawn_event` onto the child's manifest `{surface run, committed
/// ref}`; the replay under the same `idempotency_key` answers the
/// recorded handle with no second `open_session`.
#[test]
fn ac4_ac8_launch_spawn_event_and_resume() {
    let mut srv = lab();
    let a = agent();
    let runs_before = srv.svc.surface_run_ids().len();

    let (child, handle) = launch(&mut srv, &a, "l-1");
    assert!(handle.starts_with("hnd-run-"), "{handle}");

    // The child's manifest records the launch causality — the surface
    // run + the committed effect event. (The manifest borrow ends
    // before the next `&mut srv` read.)
    let srun = surface_run(&srv, &a);
    let (se, budget_ref) = {
        let manifest = srv.svc.surface_manifest(&child).expect("child manifest");
        (
            manifest
                .spawn_event
                .clone()
                .expect("spawn_event on the child manifest"),
            manifest.budget.clone().expect("child budget"),
        )
    };
    assert_eq!(se.run_id, srun, "spawn_event names the surface run");
    // The cited event is a real `action.effect.*` row on the surface
    // run — the committed write-ahead the launch rode.
    let se_events = rows(&mut srv, &srun);
    let (class, _) = se_events
        .iter()
        .find(|(c, _)| c == "action.effect.committed")
        .expect("committed row on the surface run");
    assert_eq!(class, "action.effect.committed");
    let committed_id = srv
        .svc
        .surface_events(&srun)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.effect.committed")
        .map(|e| e.event_id.clone())
        .unwrap();
    assert_eq!(
        se.event_id, committed_id,
        "spawn_event cites the committed row"
    );

    // The child's root budget is the request sealed — `manifest.budget`
    // pins the spec id (`open_session` seals the `BudgetInput::Node`
    // verbatim; the dimension-wise check is the pool gate's, exercised
    // by the MissingBudget/InsufficientBudget legs).
    assert!(
        budget_ref.starts_with("sha256:"),
        "child budget is a pinned spec ref: {budget_ref}"
    );

    // ── the resume window (AC-8) — same idempotency_key answers the
    // recorded handle; no second open_session, no second run.
    let r = call(&mut srv, &a, "launch_run", launch_args("l-1"));
    assert!(!is_err(&r), "replay refused: {r:?}");
    let sc = structured(&r);
    assert_eq!(
        sc.get("launched")
            .and_then(|l| l.get("run_id"))
            .and_then(Json::as_str),
        Some(child.as_str()),
        "the recorded run_handle answers the replay"
    );
    assert_eq!(
        srv.svc.surface_run_ids().len(),
        runs_before + 2,
        "the replay opens nothing new (surface run + one child)"
    );
}

// ── AC-5 — charges and the account projection ───────────────────────

/// AC-R-2.11.3-5 — every surface-session turn posts exactly one
/// `control.budget.consumed{tool_calls}` with `charged_to =
/// instrument`; the charge's `(source, dimension)` idempotency means a
/// rebuilt counter still matches the rows.
#[test]
fn ac5_per_turn_instrument_charges() {
    let mut srv = lab();
    let a = agent();
    let (child, handle) = launch(&mut srv, &a, "c-1");
    let _ = child;
    // Two read turns on the launched run.
    for _ in 0..2 {
        let r = call(
            &mut srv,
            &a,
            "run_status",
            Json::obj([("run", Json::str(handle.clone()))]),
        );
        assert!(!is_err(&r), "run_status refused: {r:?}");
    }
    let srun = surface_run(&srv, &a);
    let consumed: Vec<Json> = rows(&mut srv, &srun)
        .into_iter()
        .filter(|(c, _)| c == "control.budget.consumed")
        .map(|(_, p)| p)
        .collect();
    // 1 launch + 2 reads = 3 turn charges.
    assert_eq!(consumed.len(), 3, "one charge per turn: {consumed:?}");
    for p in &consumed {
        let t = p.to_canonical_string();
        assert!(
            t.contains("instrument"),
            "server work charges `instrument`: {t}"
        );
    }
}

// ── AC-7 — the ExposurePolicy read + delivered mint ─────────────────

/// AC-R-2.11.3-7 — `read_ledger` serves the admitted page and commits
/// `measurement.export.delivered` on the READ run; a run that is not a
/// minted handle answers `UnknownHandle`, never an empty page.
#[test]
fn ac7_read_ledger_policy_fold_and_delivered() {
    let mut srv = lab();
    let a = agent();
    let (child, handle) = launch(&mut srv, &a, "r-1");

    let r = call(
        &mut srv,
        &a,
        "read_ledger",
        Json::obj([("run", Json::str(handle.clone()))]),
    );
    assert!(!is_err(&r), "read_ledger refused: {r:?}");
    let sc = structured(&r);
    let served = sc
        .get("events")
        .and_then(|e| match e {
            Json::Arr(v) => Some(v.len()),
            _ => None,
        })
        .expect("events page");

    // The delivered mint on the read run — `sink_id` names the
    // surface, `seq_range` IS the served item count.
    let child_rows = rows(&mut srv, &child);
    let delivered: Vec<&Json> = child_rows
        .iter()
        .filter(|(c, _)| c == "measurement.export.delivered")
        .map(|(_, p)| p)
        .collect();
    assert!(!delivered.is_empty(), "a served page mints delivered");
    let d = delivered.last().unwrap();
    assert_eq!(
        d.get("sink_id").and_then(Json::as_str),
        Some(format!("surface:mcp:{}", a.binding_id).as_str())
    );
    let from = d
        .get("seq_range")
        .and_then(|s| s.get("from_seq"))
        .and_then(Json::as_int)
        .unwrap_or(-9);
    let to = d
        .get("seq_range")
        .and_then(|s| s.get("to_seq"))
        .and_then(Json::as_int)
        .unwrap_or(-9);
    assert_eq!(
        (to - from + 1).max(0),
        served as i64,
        "seq_range width = served items ({served})"
    );

    // A plausible-but-never-minted handle — `UnknownHandle`, not an
    // empty page.
    let r = call(
        &mut srv,
        &a,
        "read_ledger",
        Json::obj([("run", Json::str("hnd-run-deadbeef00000000000000"))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "UnknownHandle");
    // A raw (non-handle) run name is the same refusal.
    let r = call(
        &mut srv,
        &a,
        "run_status",
        Json::obj([("run", Json::str("run-nonexistent"))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "UnknownHandle");
}

/// The caller's own minted handle resolves for the caller and refuses
/// `NoCoveringGrant` under a second binding — handles are names, never
/// capabilities (the AC-2 shape the handle carrier owes at C1).
#[test]
fn handle_is_a_name_not_a_capability() {
    let mut srv = lab();
    let a = agent();
    let (_child, handle) = launch(&mut srv, &a, "h-1");

    // Caller A resolves its own handle.
    let r = call(
        &mut srv,
        &a,
        "run_status",
        Json::obj([("run", Json::str(handle.clone()))]),
    );
    assert!(!is_err(&r), "owner resolves its minted handle: {r:?}");

    // Caller B presents A's handle — `UnknownHandle` under B's own
    // table (handles are per-surface-session names; the foreign mint
    // was never admitted into B's table — `NoCoveringGrant` names the
    // owner-mismatch leg inside `resolve` when the alias IS present).
    let mut b = agent();
    b.binding_id = "bind-stdio-b".to_string();
    let r = call(
        &mut srv,
        &b,
        "run_status",
        Json::obj([("run", Json::str(handle.clone()))]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(
        refusal_kind(&r),
        "UnknownHandle",
        "a handle confers nothing across bindings: {r:?}"
    );
}

// ── AC-9 — the supply-surface Π ─────────────────────────────────────

/// A `hh-mcp-target/1` member doc for the hosted bundle.
fn supply_artifact() -> Json {
    let t = |name: &str, sid: &str| {
        Json::obj([
            ("name", Json::str(name)),
            ("description", Json::str(format!("{name} tool"))),
            ("inputSchema", Json::obj([("type", Json::str("object"))])),
            (
                "_meta",
                Json::obj([(
                    hh_mcp::artifact::HH_META_KEY,
                    Json::obj([("semantic_id", Json::str(sid))]),
                )]),
            ),
        ])
    };
    Json::obj([
        ("schema", Json::str(hh_mcp::artifact::MCP_TARGET_SCHEMA)),
        ("target", Json::str("mcp")),
        (
            "tools",
            Json::Arr(vec![
                t("safe", "t.safe"),
                t("hammer", "t.hammer"),
                t("spicy", "t.spicy"),
                t("secret", "t.secret"),
            ]),
        ),
    ])
}

/// The lab document + one supply surface (`bind-hosted`) with Π rows
/// `deny hammer | ask spicy | deny+hidden secret | allow *`.
fn supply_doc(hosted: &CallerBinding) -> Json {
    let mut doc = match exposure::default_lab_document() {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    doc.get_mut("caller_bindings")
        .and_then(|b| match b {
            Json::Arr(a) => {
                a.push(hosted.to_json());
                Some(())
            }
            _ => None,
        })
        .expect("caller_bindings[]");
    doc.insert(
        "supply_surfaces".into(),
        Json::Arr(vec![Json::obj([
            ("surface_id", Json::str("sup-1")),
            ("binding_id", Json::str("bind-hosted")),
            ("bundle_id", Json::str("b-1")),
            ("artifact", supply_artifact()),
            (
                "pi",
                Json::Arr(vec![
                    Json::obj([
                        ("match", Json::str("hammer")),
                        ("decision", Json::str("deny")),
                    ]),
                    Json::obj([
                        ("match", Json::str("spicy")),
                        ("decision", Json::str("ask")),
                    ]),
                    Json::obj([
                        ("match", Json::str("secret")),
                        ("decision", Json::str("deny")),
                        ("hidden", Json::Bool(true)),
                    ]),
                    Json::obj([("match", Json::str("*")), ("decision", Json::str("allow"))]),
                ]),
            ),
        ])]),
    );
    Json::Obj(doc)
}

/// AC-R-2.11.3-9 — the hosted binding's `deny` row never executes and
/// answers `isError`; `ask` answers `pending` and lands the durable
/// `security.permission.pending`; `callable ⇔ revealed` holds over the
/// binding's own `tools/list`.
#[test]
fn ac9_supply_surface_pi() {
    let mut hosted = stdio_launch_binding("hosted-proc", CallerKind::Agent);
    hosted.binding_id = "bind-hosted".to_string();
    let doc = supply_doc(&hosted);
    let mut srv = LabServer::new(service(), &doc).expect("supply doc links");

    // tools/list under the hosted binding serves ITS artifact —
    // `secret` (hidden) is never revealed.
    let frame = srv
        .handle_message(&hosted, &msg(1, "tools/list", Json::obj([])))
        .unwrap();
    let names: Vec<String> = frame
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| match t {
            Json::Arr(v) => Some(
                v.iter()
                    .filter_map(|t| t.get("name").and_then(Json::as_str).map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .expect("tools[]");
    assert!(
        names.contains(&"safe".to_string())
            && names.contains(&"hammer".to_string())
            && names.contains(&"spicy".to_string()),
        "revealed set: {names:?}"
    );
    assert!(
        !names.contains(&"secret".to_string()),
        "hidden is never revealed: {names:?}"
    );
    // …and not callable either.
    let r = call(&mut srv, &hosted, "secret", Json::obj([]));
    assert!(is_err(&r) && refusal_kind(&r) == "unknown_tool", "{r:?}");

    // allow — the declared stub executes (no invented effects).
    let r = call(&mut srv, &hosted, "safe", Json::obj([("x", Json::Int(1))]));
    assert!(!is_err(&r), "allow executes: {r:?}");

    // deny — never executes; the refused row + isError.
    let r = call(&mut srv, &hosted, "hammer", Json::obj([]));
    assert!(is_err(&r), "{r:?}");
    assert_eq!(refusal_kind(&r), "DeniedByPolicy");

    // ask — `pending` + the durable `security.permission.pending` on
    // the hosted surface run.
    let r = call(&mut srv, &hosted, "spicy", Json::obj([]));
    assert!(is_err(&r), "{r:?}");
    assert_eq!(
        refusal_kind(&r),
        "PermissionAskRequired",
        "ask arm answers pending: {r:?}"
    );
    assert!(
        r.get("pending_effects").is_some(),
        "pending_effects[]: {r:?}"
    );
    let srun = surface_run(&srv, &hosted);
    let pend = rows(&mut srv, &srun)
        .into_iter()
        .filter(|(c, _)| c == "security.permission.pending")
        .count();
    assert_eq!(pend, 1, "the ask lands one durable pending row");

    // A name the surface's own artefact does not carry is
    // `unknown_tool` — even when the Lab group exposes it.
    let r = call(&mut srv, &hosted, "launch_run", Json::obj([]));
    assert!(is_err(&r) && refusal_kind(&r) == "unknown_tool", "{r:?}");
}

// ── AC-12 — assumption debt at link ─────────────────────────────────

/// AC-R-2.11.3-12 — `link` refuses a definition missing a declared
/// carrier's assumption-debt record; the shipped `hh-lab/1` carries
/// every required carrier.
#[test]
fn ac12_assumption_debt_required_at_link() {
    let doc = exposure::default_lab_document();
    let def = exposure::parse_exposure(&doc).expect("shipped doc links");
    assert_eq!(def.semantic_id, "hh-lab/1");

    // Strip all debt — `tasks_carrier` is the first missing.
    let mut m = match doc.clone() {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    m.insert("assumption_debt".into(), Json::Arr(vec![]));
    match exposure::parse_exposure(&Json::Obj(m)) {
        Err(exposure::ExposureError::MissingDebt { carrier }) => {
            assert_eq!(carrier, "tasks_carrier")
        }
        other => panic!("expected MissingDebt: {other:?}"),
    }

    // An `mtls` binding without its `caller_binding.mtls` record —
    // strip that carrier's row only.
    let mut m = match doc {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    if let Some(Json::Arr(a)) = m.get_mut("assumption_debt") {
        a.retain(|d| d.get("carrier").and_then(Json::as_str) != Some("caller_binding.mtls"));
    }
    let mut mtls_b = stdio_launch_binding("m", CallerKind::Agent).to_json();
    if let Json::Obj(b) = &mut mtls_b {
        b.insert("binding_id".into(), Json::str("bind-mtls"));
        b.insert(
            "credential".into(),
            Json::obj([
                ("kind", Json::str("mtls")),
                ("cert_ref", Json::str("secret:test-cert")),
            ]),
        );
    }
    if let Some(Json::Arr(a)) = m.get_mut("caller_bindings") {
        a.push(mtls_b);
    }
    match exposure::parse_exposure(&Json::Obj(m)) {
        Err(exposure::ExposureError::MissingDebt { carrier }) => {
            assert_eq!(carrier, "caller_binding.mtls")
        }
        other => panic!("expected MissingDebt: {other:?}"),
    }
}

// ── AC-13 — every tool wraps a real op or a declared lowering ───────

/// AC-R-2.11.3-13 — every Lab tool's `op` names a real `hh-embed/1`
/// method (the generated-schema method table), and every `op = ""`
/// verb is a declared run-envelope lowering with a loss class — no
/// undeclared surface verbs exist.
#[test]
fn ac13_every_tool_wraps_a_real_op_or_declared_lowering() {
    let schema = hh_embed_schema::export_schema();
    let ops: std::collections::BTreeSet<String> = schema
        .get("methods")
        .and_then(|m| match m {
            Json::Obj(mm) => Some(mm.keys().cloned().collect()),
            _ => None,
        })
        .expect("schema methods");
    let def = exposure::default_exposure();
    for t in &def.tools {
        if t.op.is_empty() {
            let class = t
                .lowering
                .as_ref()
                .and_then(|l| l.get("class"))
                .and_then(Json::as_str)
                .unwrap_or("");
            assert!(
                matches!(class, "exact" | "narrowed"),
                "tool `{}` is a surface verb with no declared loss class",
                t.name
            );
        } else {
            assert!(
                ops.contains(t.op.as_str()),
                "tool `{}` wraps unknown op `{}`",
                t.name,
                t.op
            );
        }
    }
}

// ── AC-11 halves — the two transports, one dispatch ─────────────────

/// AC-R-2.11.3-11 (stdio half) — the newline-JSON-RPC loop answers
/// `initialize`/`server/discover`/`tools/list`/`tools/call` frames and
/// `-32700` for a malformed line; two loops over the same server shape
/// answer identically.
#[test]
fn ac11_stdio_loop() {
    let lines = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2026-07-28\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"0\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"server/discover\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\",\"params\":{}}\n",
        "not json at all\n",
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"run_status\",\"arguments\":{\"run\":\"hnd-run-nope\"}}}\n"
    );
    let run = |srv: &mut LabServer| -> Vec<String> {
        let mut reader = BufReader::new(Cursor::new(lines.as_bytes().to_vec()));
        let mut out = Vec::new();
        serve_stdio(srv, &agent(), &mut reader, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    };
    let mut srv_a = lab();
    let a = run(&mut srv_a);
    let mut srv_b = lab();
    let b = run(&mut srv_b);
    assert_eq!(a.len(), 5, "{a:?}");
    // The discovery frames are byte-identical across services; the
    // call answer differs only in the allocated ids (run/session).
    assert_eq!(a[0], b[0]);
    assert_eq!(a[1], b[1]);
    assert_eq!(a[2], b[2]);
    assert_eq!(a[3], b[3]);
    let f0 = hh_wire::json::parse(&a[0]).unwrap();
    assert_eq!(
        f0.get("result")
            .and_then(|r| r.get("serverInfo"))
            .and_then(|s| s.get("name"))
            .and_then(Json::as_str),
        Some("hh-mcp-lab")
    );
    let f3 = hh_wire::json::parse(&a[3]).unwrap();
    assert_eq!(
        f3.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_int),
        Some(-32700),
        "malformed line answers parse_error"
    );
    let f4 = hh_wire::json::parse(&a[4]).unwrap();
    assert_eq!(
        f4.get("result")
            .and_then(|r| r.get("structuredContent"))
            .and_then(|s| s.get("surface_error"))
            .and_then(|e| e.get("kind"))
            .and_then(Json::as_str),
        Some("UnknownHandle"),
        "tools/call flows through the surface chain"
    );
}

/// AC-R-2.11.3-11 (HTTP half) — Streamable HTTP: `initialize` issues
/// `Mcp-Session-Id`, the session-gated frames answer `200`, an
/// unknown/missing bearer answers `401`, a stale session answers
/// `404`, and `tools/call` under the bearer-resolved binding runs the
/// same surface chain.
#[test]
fn ac11_http_transport() {
    let mut srv = lab();
    srv.grant_bearer("tok-test-1", agent());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let origins = hh_mcp::http::OriginPolicy::new(&[], false);
    // `LabServer` is `!Send` (EmbedService's dyn ports are not `Send`) —
    // the fixture spawns the accept loop anyway: the serve thread is the
    // ONLY owner of `srv` for the rest of the test (the main thread only
    // ever does client-side TcpStream IO and never reads server state),
    // so the pointer hand-off is sound here. The server is leaked for
    // the detached thread; the process reaps it at exit (the same shape
    // as hh-mcp's own http fixture).
    let srv_ptr = Box::into_raw(Box::new(srv)) as usize;
    let listener_ptr = Box::into_raw(Box::new(listener)) as usize;
    let origins_ptr = Box::into_raw(Box::new(origins)) as usize;
    std::thread::spawn(move || {
        let srv = unsafe { &mut *(srv_ptr as *mut LabServer) };
        let listener = unsafe { &*(listener_ptr as *const std::net::TcpListener) };
        let origins = unsafe { &*(origins_ptr as *const hh_mcp::http::OriginPolicy) };
        let _ = hh_mcp_lab::http::serve_http(srv, listener, origins);
    });

    fn post(addr: &std::net::SocketAddr, headers: &[(&str, &str)], body: &str) -> String {
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        let mut req = format!(
            "POST /mcp HTTP/1.1\r\nhost: {addr}\r\ncontent-length: {}\r\n",
            body.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(body);
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).unwrap();
        String::from_utf8_lossy(&buf).to_string()
    }

    // No bearer → 401 + the challenge.
    let r = post(
        &addr,
        &[],
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert!(r.starts_with("HTTP/1.1 401"), "{r}");

    // Bearer → initialize → `Mcp-Session-Id` issued.
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-test-1")],
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let sid = r
        .lines()
        .find(|l| l.to_lowercase().starts_with("mcp-session-id:"))
        .map(|l| l.split(':').nth(1).unwrap().trim().to_string())
        .expect("session header");

    // The same dispatch runs under the bearer-resolved binding.
    let r = post(
        &addr,
        &[
            ("authorization", "Bearer tok-test-1"),
            ("mcp-session-id", &sid),
        ],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_status","arguments":{"run":"hnd-run-nope"}}}"#,
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let body = r.split("\r\n\r\n").nth(1).unwrap_or("");
    let f = hh_wire::json::parse(body).unwrap();
    assert_eq!(
        f.get("result")
            .and_then(|x| x.get("structuredContent"))
            .and_then(|s| s.get("surface_error"))
            .and_then(|e| e.get("kind"))
            .and_then(Json::as_str),
        Some("UnknownHandle"),
        "the HTTP arm runs the same surface chain: {body}"
    );

    // A stale session id → 404.
    let r = post(
        &addr,
        &[
            ("authorization", "Bearer tok-test-1"),
            ("mcp-session-id", "mcp-session-999"),
        ],
        r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}"#,
    );
    assert!(r.starts_with("HTTP/1.1 404"), "{r}");

    // A bad bearer → 401 (the grant table decides, never a guess).
    let r = post(
        &addr,
        &[("authorization", "Bearer tok-forged")],
        r#"{"jsonrpc":"2.0","id":4,"method":"ping","params":{}}"#,
    );
    assert!(r.starts_with("HTTP/1.1 401"), "{r}");
}
