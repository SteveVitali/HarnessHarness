//! CAP.2 — the capstone's composed surface-approval seam (DF-S4.11-3).
//!
//! The additive tickets each proved their own slice: AC-R-2.11.3-9
//! (`ac9_supply_surface_pi`) proves a supply-surface Π `ask` mints the
//! durable `security.permission.pending` on the caller's surface run, and
//! AC-R-2.11.3-3 proves the `respond_approval` caller-kind gate. This
//! battery drives the *round trip* through the real boundary and pins the
//! composed state honestly:
//!
//! - **Green pin** — the ask side is complete and durable: `pending`
//!   lands on the surface run with a `permission_id`. The *answer* side
//!   is not wired: the hosted caller's `respond_approval` is `unknown_tool`
//!   (its catalogue is its artifact — `callable ⇔ revealed`), and the
//!   run-less `respond_approval{permission_id}` under a Lab-catalogue
//!   `human_principal` binding refuses `schema_violation` — `respond_permission`
//!   needs a `session_id`, and the surface run is never minted as a
//!   `hnd-run-*` handle. The pending row stays open; no serving
//!   `security.permission.decided` names it.
//! - **DF-S4.11-3 xfail** — the deferral's check: a hosted caller's
//!   `respond_approval{permission_id}` resolves the pending row on its
//!   own surface run, mints `security.permission.decided`, and the
//!   retried `tools/call` applies. Routed to CAP.3.

#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_mcp_lab::binding::{stdio_launch_binding, CallerBinding, CallerKind};
use hh_mcp_lab::exposure;
use hh_mcp_lab::server::LabServer;
use hh_wire::json::Json;

// ── store/service fixtures ──────────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-mcp-lab-cap2-{tag}-{}-{}",
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
        holder: "cap-2".into(),
    })
    .unwrap()
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

fn refusal_kind(result: &Json) -> String {
    result
        .get("structuredContent")
        .and_then(|s| s.get("surface_error"))
        .and_then(|e| e.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
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

// ── the supply surface (verbatim the S4.11 fixture shape — `deny
// hammer | ask spicy | deny+hidden secret | allow *`) ────────────────

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
            ]),
        ),
    ])
}

/// The lab document + one supply surface on `binding` — Π rows
/// `deny hammer | ask spicy | allow *`.
fn supply_doc(binding: &CallerBinding) -> Json {
    let mut doc = match exposure::default_lab_document() {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    doc.get_mut("caller_bindings")
        .and_then(|b| match b {
            Json::Arr(a) => {
                a.push(binding.to_json());
                Some(())
            }
            _ => None,
        })
        .expect("caller_bindings[]");
    doc.insert(
        "supply_surfaces".into(),
        Json::Arr(vec![Json::obj([
            ("surface_id", Json::str("sup-1")),
            ("binding_id", Json::str(binding.binding_id.as_str())),
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
                    Json::obj([("match", Json::str("*")), ("decision", Json::str("allow"))]),
                ]),
            ),
        ])]),
    );
    Json::Obj(doc)
}

/// The hosted `human_principal` binding — `respond_approval` is the
/// `permission_request` domain, R-2 principals only; a *human* hosted
/// caller is the binding the deferral's round trip names.
fn hosted_human() -> CallerBinding {
    let mut b = stdio_launch_binding("hosted-proc", CallerKind::HumanPrincipal);
    b.binding_id = "bind-hosted".to_string();
    b
}

/// `security.permission.pending{permission_id}` on the surface run — the
/// ask's durable id.
fn pending_id(rows: &[(String, Json)]) -> String {
    rows.iter()
        .find(|(c, _)| c == "security.permission.pending")
        .and_then(|(_, p)| p.get("permission_id").and_then(Json::as_str))
        .map(String::from)
        .expect("a durable pending row carries permission_id")
}

/// A serving `security.permission.decided` — the row a real answer mints —
/// names the pending's `permission_id`.
fn serving_decided(rows: &[(String, Json)], permission_id: &str) -> bool {
    rows.iter().any(|(c, p)| {
        c == "security.permission.decided"
            && p.get("permission_id").and_then(Json::as_str) == Some(permission_id)
    })
}

/// CAP.2/CAP.3 (green pin): the composed ask → pending → answer seam.
///
/// The ask leg through the real `LabServer` + `EmbedService`: the Π `ask`
/// answers `PermissionAskRequired` + `pending_effects[]` and lands the
/// durable `security.permission.pending{permission_id}` on the caller's
/// surface run. CAP.3 (DF-S4.11-3; ADR-0329) made the answer reachable —
/// `respond_approval` is now the supply protocol's builtin verb:
///
/// 1. the hosted `human_principal` caller's `respond_approval` resolves
///    on its own surface run (a `deferred` outcome is recorded, the
///    pending stays open);
/// 2. a Lab-catalogue `human_principal` binding's run-less
///    `respond_approval{permission_id}` still refuses `schema_violation`
///    — the op needs the run's own live `session_id` and the surface run
///    is never minted as a `hnd-run-*` handle. The reply path is the
///    surface's own verb, not the launched-run op.
///
/// The pending stays open through both — no serving
/// `security.permission.decided` names it — and a retry of the asked
/// call asks again.
#[test]
fn cap2_supply_ask_pending_unanswerable() {
    let hosted = hosted_human();
    let doc = supply_doc(&hosted);
    let mut srv = LabServer::new(service(), &doc).expect("supply doc links");

    // The ask — `pending` + the durable `security.permission.pending` on
    // the hosted caller's surface run.
    let r = call(&mut srv, &hosted, "spicy", Json::obj([]));
    assert!(is_err(&r), "{r:?}");
    assert_eq!(
        refusal_kind(&r),
        "PermissionAskRequired",
        "the ask arm answers pending: {r:?}"
    );
    assert!(
        r.get("pending_effects").is_some(),
        "pending_effects[]: {r:?}"
    );
    let srun = surface_run(&srv, &hosted);
    let pend_rows = rows(&mut srv, &srun);
    let permission_id = pending_id(&pend_rows);
    assert!(
        !permission_id.is_empty(),
        "the durable pending names its id"
    );

    // Leg 1 (DF-S4.11-3, resolved at CAP.3; ADR-0329): `respond_approval`
    // is now the supply protocol's builtin answer verb — the hosted
    // human_principal's call resolves the pending on its own surface
    // run. (The pinned `unknown_tool` for a *non-principal* caller is
    // gone too: the verb answers `IllegitimateEndorsement` before the
    // catalogue check — the gate is the caller-kind bar, same as the
    // Lab catalogue's.)
    let r = call(
        &mut srv,
        &hosted,
        "respond_approval",
        Json::obj([
            ("permission_id", Json::str(permission_id.clone())),
            ("outcome", Json::str("deferred")),
            ("idempotency_key", Json::str("resp-1")),
        ]),
    );
    assert!(
        !is_err(&r),
        "the caller's own deferred answer is accepted (pending stays open): {r:?}"
    );

    // Leg 2 — a `human_principal` on the Lab catalogue passes the
    // caller-kind gate, but the run-less call cannot reach the pending:
    // `respond_permission` requires `session_id` — the surface run is
    // never minted as a resolvable handle. The verbatim refusal names the
    // missing member (`respond_permission/session_id`).
    let human = stdio_launch_binding("test-human", CallerKind::HumanPrincipal);
    let r = call(
        &mut srv,
        &human,
        "respond_approval",
        Json::obj([
            ("permission_id", Json::str(permission_id.clone())),
            ("outcome", Json::str("approved")),
            ("idempotency_key", Json::str("resp-2")),
        ]),
    );
    assert!(is_err(&r), "{r:?}");
    assert_eq!(
        refusal_kind(&r),
        "SchemaViolation",
        "run-less respond_approval refuses at the op's params: {r:?}"
    );
    let detail = r
        .get("structuredContent")
        .and_then(|s| s.get("surface_error"))
        .and_then(|e| e.get("detail"))
        .cloned()
        .unwrap_or(Json::Null);
    assert_eq!(
        detail.get("path").and_then(Json::as_str),
        Some("respond_permission/session_id"),
        "the missing member is the session coordinate: {r:?}"
    );

    // The pending is still open — no serving decided row names it, and a
    // retried call asks again (nothing about the pending is silently
    // cleared by the refused answers).
    let after = rows(&mut srv, &srun);
    assert!(
        !serving_decided(&after, &permission_id),
        "no decided row serves `{permission_id}`: {:?}",
        after.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
    );
    let r = call(&mut srv, &hosted, "spicy", Json::obj([]));
    assert_eq!(
        refusal_kind(&r),
        "PermissionAskRequired",
        "the unanswered ask asks again: {r:?}"
    );
}

/// DF-S4.11-3 (closed at CAP.3; ADR-0329): the deferral's done check
/// verbatim, un-ignored — a hosted caller's run-less, surface-run-scoped
/// `respond_approval{permission_id}` resolves the pending row on its own
/// surface run, mints `security.permission.decided`, and the retried
/// `tools/call` applies.
#[test]
fn cap2_supply_ask_respond_approval_round_trip() {
    let hosted = hosted_human();
    let doc = supply_doc(&hosted);
    let mut srv = LabServer::new(service(), &doc).expect("supply doc links");

    // The ask lands (this half is green — the check is the *answer*).
    let r = call(&mut srv, &hosted, "spicy", Json::obj([]));
    assert_eq!(refusal_kind(&r), "PermissionAskRequired", "{r:?}");
    let srun = surface_run(&srv, &hosted);
    let permission_id = pending_id(&rows(&mut srv, &srun));

    // The residual: the hosted caller answers its own pending by name.
    let r = call(
        &mut srv,
        &hosted,
        "respond_approval",
        Json::obj([
            ("permission_id", Json::str(permission_id.clone())),
            ("outcome", Json::str("approved")),
            ("idempotency_key", Json::str("resp-3")),
        ]),
    );
    assert!(
        !is_err(&r),
        "DF-S4.11-3 residual: the hosted caller's answer applies: {r:?}"
    );
    let after = rows(&mut srv, &srun);
    assert!(
        serving_decided(&after, &permission_id),
        "a serving decided{permission_id} lands: {:?}",
        after.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
    );
    let r = call(&mut srv, &hosted, "spicy", Json::obj([]));
    assert!(
        !is_err(&r),
        "the retried call applies once the pending is answered: {r:?}"
    );
}
