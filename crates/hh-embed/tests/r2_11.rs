//! R-2.11 acceptance (boundary leg) — the produce-time
//! `security.label.endorsed{basis: approval}` mint over the durable
//! `action.intent.anchored` subject, the C2 `modify` arm's
//! `decided{modified, amended_args}` trail through
//! `surface_respond_permission`, the `OptionNotOffered` gate over the
//! *recorded* offered set, and the remedy-ingress echo R-2.12 reads
//! (ADR-0343 D1–D4; DF-S1.23-1).
//!
//! The surface seam is the honest driver: the test opens a surface run,
//! appends the dispatch-shape `anchor + pending` rows through the append
//! gate, then answers through `surface_respond_permission` — the same
//! path `hh-mcp-lab`'s `respond_approval` tool calls.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::PermissionOutcome;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_monitor::approval::{
    ApprovalMode, ApprovalOption, ApprovalOptionId, ApprovalRequest, Explanation, PermissionRequest,
};
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r211-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service(tag: &str) -> EmbedService {
    let root = dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r211".into(),
    })
    .unwrap()
}

/// A surface run carries no `configuration_id`/`environment_ref` — the
/// `open_run` invariant refuses them (`hh-mcp-lab`'s session does the
/// same clearing).
fn surface_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Surface);
    m.configuration_id = None;
    m.configuration_version_id = None;
    m.environment_ref = None;
    m.environment_version_id = None;
    m
}

fn option(id: ApprovalOptionId) -> ApprovalOption {
    ApprovalOption {
        id,
        label: id.as_str().to_string(),
    }
}

/// The dispatch-shape `ApprovalRequest` the `pending_payload` builder
/// spells — `options`/`remedies` the caller chooses (the R-2.11 ingress
/// members ride the durable row).
fn request(pid: &str, options: Vec<ApprovalOptionId>, remedies: Vec<Json>) -> ApprovalRequest {
    ApprovalRequest {
        permission_id: pid.to_string(),
        request: PermissionRequest {
            subject_ref: "test:agent".to_string(),
            capability_ref: hh_compiler::plan::PinnedRef {
                semantic_id: "test:fetch".to_string(),
                version_id: "v-cap".to_string(),
            },
            args_canonical_hash: "sha256:args-1".to_string(),
            reason: "pi ask".to_string(),
            requested_grants: vec![],
        },
        options: options.into_iter().map(option).collect(),
        mode: ApprovalMode::Sync,
        timeout: None,
        explanation: Explanation {
            display: "pi asked".to_string(),
            rows: vec![],
            model_justification: None,
        },
        batch_id: None,
        // `remedies` are `hh_provenance::flow::Remedy` — the test spells
        // the closed sum directly (from_json over the canonical shape).
        remedies: remedies
            .iter()
            .enumerate()
            .map(|(i, r)| {
                hh_provenance::flow::Remedy::from_json(r, &format!("remedies[{i}]")).unwrap()
            })
            .collect(),
    }
}

/// Mint the dispatch-shape `action.intent.anchored` — the intent's own
/// provenance (model-origin, delegate authority), never the kernel stamp.
fn anchor_event(
    svc: &EmbedService,
    run_id: &str,
    pid: &str,
    effect_id: &str,
) -> hh_ledger::event::Event {
    let m = hh_env::events::EventMinter::new(svc.store(), run_id);
    let mut ev = m
        .mint(
            "action.intent.anchored",
            Json::obj([
                ("permission_id", Json::str(pid)),
                ("effect_id", Json::str(effect_id)),
                ("subject_ref", Json::str("test:agent")),
                (
                    "capability_ref",
                    Json::obj([
                        ("semantic_id", Json::str("test:fetch")),
                        ("version_id", Json::str("v-cap")),
                    ]),
                ),
                ("args_canonical_hash", Json::str("sha256:args-1")),
            ]),
        )
        .unwrap();
    ev.provenance = Some(ProvenanceRecord::minted(
        Origin::model("model:test", "run:test", "resp-1"),
        PersistenceScope::Run,
        0,
    ));
    ev.content_kind = Some(hh_provenance::ContentKind::EffectIntent);
    ev
}

/// Mint the `security.permission.pending` row the dispatch's
/// `pending_payload` produces.
fn pending_event(
    svc: &EmbedService,
    run_id: &str,
    pid: &str,
    effect_id: &str,
    options: Vec<ApprovalOptionId>,
    remedies: Vec<Json>,
) -> hh_ledger::event::Event {
    let m = hh_env::events::EventMinter::new(svc.store(), run_id);
    m.mint(
        "security.permission.pending",
        hh_monitor::approval::pending_payload(
            &request(pid, options, remedies),
            effect_id,
            svc.store().now_ms(),
        ),
    )
    .unwrap()
}

fn events(svc: &EmbedService, run_id: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    svc.surface_events(run_id).unwrap().to_vec()
}

// ── D2 — `label.endorsed{basis: approval}` mints at the produce-time ────────
//
// The ask's anchor is the durable `EffectIntent` subject; the allow's
// `granted` batch carries the endorsement row — `basis: approval`,
// `basis_ref = permission_id`, `subject_ref = anchor.event_id`, the rise
// from the intent's delegate label to the responder's principal.
#[test]
fn r211_allow_mints_approval_endorsement_over_anchor() {
    let mut svc = service("endorse");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-endorse-1".to_string();
    let effect_id = format!("ef-{pid}");
    let anchor = anchor_event(&svc, &run_id, &pid, &effect_id);
    let anchor_id = anchor.event_id.clone();
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        &effect_id,
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Modify],
        vec![],
    );
    svc.surface_append(&run_id, &lease, vec![anchor, pending])
        .unwrap();

    let out = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Selected {
                option_id: "allow_once".to_string(),
            },
            "human:op",
            None,
            "req-endorse-1",
        )
        .unwrap();
    assert_eq!(
        out.get("recorded").map(|r| matches!(r, Json::Bool(true))),
        Some(true),
        "{out:?}"
    );

    let envs = events(&svc, &run_id);
    let decided = envs
        .iter()
        .find(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid.as_str())
                && e.payload.get("decision").and_then(Json::as_str) == Some("allow")
        })
        .expect("decided{allow} durable");
    assert!(decided.seq > 0);
    assert!(
        envs.iter()
            .any(|e| e.class == "security.permission.granted"),
        "the allow mints the durable ApproverGrant-side granted row"
    );
    let endorsed = envs
        .iter()
        .find(|e| e.class == "security.label.endorsed")
        .expect("label.endorsed{basis: approval} mints at produce time");
    let p = &endorsed.payload;
    assert_eq!(p.get("basis").and_then(Json::as_str), Some("approval"));
    assert_eq!(
        p.get("basis_ref").and_then(Json::as_str),
        Some(pid.as_str()),
        "basis_ref = permission_id"
    );
    assert_eq!(
        p.get("subject_ref").and_then(Json::as_str),
        Some(anchor_id.as_str()),
        "the subject is the durable intent anchor, never content"
    );
    // The rise: `from` is the intent's delegate label; `to` is the
    // responder's principal — a label that does not rise never mints.
    let from = p.get("from").expect("endorsed.from");
    let to = p.get("to").expect("endorsed.to");
    assert_eq!(
        from.get("authority").and_then(Json::as_str),
        Some("delegate"),
        "the intent's own label — the endorsement's honest `from`"
    );
    assert_eq!(
        to.get("authority").and_then(Json::as_str),
        Some("principal"),
        "the approval's rise ends at the responder's authority"
    );
}

// ── D1 — `modify` produces `decided{modified, amended_args}` ───────────────
//
// The durable offered set names `modify`; the answer's `modified`
// outcome mints the terminal decided row carrying the amendment
// verbatim (the as-proposed effect's dispatch refuses on it — the
// env-side battery pins that leg), and the replay serves the recorded
// result idempotently.
#[test]
fn r211_modified_outcome_mints_decided_with_amended_args() {
    let mut svc = service("modified");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-mod-1".to_string();
    let effect_id = format!("ef-{pid}");
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        &effect_id,
        vec![
            ApprovalOptionId::AllowOnce,
            ApprovalOptionId::Deny,
            ApprovalOptionId::Modify,
        ],
        vec![],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();

    let amended = Json::obj([("url", Json::str("https://narrowed.example/x"))]);
    let out = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Modified {
                amended_args: amended.clone(),
            },
            "human:op",
            None,
            "req-mod-1",
        )
        .unwrap();
    assert_eq!(
        out.get("recorded").map(|r| matches!(r, Json::Bool(true))),
        Some(true)
    );

    let envs = events(&svc, &run_id);
    let decided = envs
        .iter()
        .find(|e| e.class == "security.permission.decided")
        .expect("decided durable");
    assert_eq!(
        decided.payload.get("decision").and_then(Json::as_str),
        Some("modified"),
        "the terminal tag is `modified`"
    );
    assert_eq!(
        decided.payload.get("amended_args"),
        Some(&amended),
        "amended_args rides the durable row verbatim"
    );
    // No conferral — a modified decision mints no grant and no
    // endorsement (the rise belongs to an `allow`, never to an amend).
    assert!(
        !envs
            .iter()
            .any(|e| e.class == "security.permission.granted"),
        "a modified decision mints no grant"
    );
    assert!(
        !envs.iter().any(|e| e.class == "security.label.endorsed"),
        "a modified decision mints no endorsement"
    );

    // Replay under the same request_id → the recorded result; a fresh
    // request_id → AlreadyDecided (exactly-one).
    let again = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Modified {
                amended_args: amended.clone(),
            },
            "human:op",
            None,
            "req-mod-1",
        )
        .unwrap();
    assert_eq!(
        again.get("recorded").map(|r| matches!(r, Json::Bool(true))),
        Some(true)
    );
    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Cancelled,
            "human:op",
            None,
            "req-mod-2",
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            hh_embed_schema::errors::EmbedError::AlreadyDecided { .. }
        ),
        "a second answer refuses typed: {err:?}"
    );
}

// ── D1/D4 — `modify` requires the *recorded* offer ─────────────────────────
//
// A pending whose durable `request.options` lacks `modify` refuses the
// arm `OptionNotOffered` — the gate reads the ledgered offer, never a
// fabricated default.
#[test]
fn r211_modified_without_offered_modify_refuses() {
    let mut svc = service("modgate");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-gate-1".to_string();
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-gate",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();
    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Modified {
                amended_args: Json::obj([("url", Json::str("https://x"))]),
            },
            "human:op",
            None,
            "req-gate-1",
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            hh_embed_schema::errors::EmbedError::OptionNotOffered { .. }
        ),
        "unoffered modify refuses OptionNotOffered: {err:?}"
    );
    // And `selected` against an unoffered option stays gated too.
    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Selected {
                option_id: "allow_lease".to_string(),
            },
            "human:op",
            None,
            "req-gate-2",
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            hh_embed_schema::errors::EmbedError::OptionNotOffered { .. }
        ),
        "the recorded offer governs `selected` too: {err:?}"
    );
}

// ── D4 — the remedy-*ingress* echo lands on `decided` ──────────────────────
//
// The durable `pending.request.remedies` folds back into the
// `ApprovalRequest` and rides the `decided` row verbatim — the exact
// seam R-2.12's consume path (`remedies.taken` re-dispatch) reads.
#[test]
fn r211_pending_remedies_echo_into_decided() {
    let mut svc = service("remedy");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-remedy-1".to_string();
    let remedy = Json::obj([
        ("kind", Json::str("sanitize")),
        ("param", Json::str("path")),
        ("sanitizer_ref", Json::str("san:path-clean")),
    ]);
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-remedy",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![remedy.clone()],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();
    svc.surface_respond_permission(
        &run_id,
        &lease,
        &pid,
        &PermissionOutcome::Cancelled,
        "human:op",
        None,
        "req-remedy-1",
    )
    .unwrap();
    let envs = events(&svc, &run_id);
    let decided = envs
        .iter()
        .find(|e| e.class == "security.permission.decided")
        .expect("decided durable");
    let echoed = decided
        .payload
        .get("remedies")
        .and_then(|r| match r {
            Json::Arr(rows) => Some(rows.clone()),
            _ => None,
        })
        .expect("the offered remedies echo onto `decided`");
    assert_eq!(
        echoed,
        vec![remedy],
        "the pending's offered remedy survives verbatim for R-2.12"
    );
}

// ── D2 — no anchor, no endorsement (honest absence) ────────────────────────
//
// A pending with no `action.intent.anchored` subject mints no
// `label.endorsed` — the produce-time site degrades to the typed
// `None`, never a fabricated endorsement.
#[test]
fn r211_allow_without_anchor_mints_no_endorsement() {
    let mut svc = service("noanchor");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-bare-1".to_string();
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-bare",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();
    svc.surface_respond_permission(
        &run_id,
        &lease,
        &pid,
        &PermissionOutcome::Selected {
            option_id: "allow_once".to_string(),
        },
        "human:op",
        None,
        "req-bare-1",
    )
    .unwrap();
    let envs = events(&svc, &run_id);
    assert!(
        envs.iter()
            .any(|e| e.class == "security.permission.granted"),
        "the allow still mints its grant"
    );
    assert!(
        !envs.iter().any(|e| e.class == "security.label.endorsed"),
        "no anchor ⇒ no endorsement — never fabricated"
    );
}
