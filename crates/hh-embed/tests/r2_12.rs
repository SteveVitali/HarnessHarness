//! R-2.12 acceptance (boundary leg) — the remedy *consume* arm of the
//! approval-ingress surface (ADR-0344 D5; DF-S2.7-1 member (b)):
//! `PermissionOutcome::Remedy` decodes the closed `Remedy` sum, gates
//! membership against the *durable* `pending.request.remedies` offer
//! (`OptionNotOffered`, never a session cache), and mints
//! `security.permission.decided{remedy_taken}` — the attestation the
//! re-dispatch's `remedy_taken_for` reads. `approval`-kind takes decide
//! `allow`; every other kind decides `deny{reason: remedy:<kind>}` —
//! the as-proposed effect is refused and the remedy's re-dispatch
//! re-enters as a fresh effect under the take (the `modified` arm's
//! terminal/re-enter shape, ADR-0343 D1).
//!
//! The surface seam is the honest driver (the r2_11 battery's shape):
//! a surface run, dispatch-shape `pending` rows appended through the
//! append gate, answers through `surface_respond_permission` — the
//! same op `hh-mcp-lab`'s `respond_approval` tool calls. The dispatch-
//! side consume (attestation verify + replay + the endorsement mint)
//! is `hh-env/tests/r2_12.rs`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::PermissionOutcome;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_monitor::approval::{
    ApprovalMode, ApprovalOption, ApprovalOptionId, ApprovalRequest, Explanation, PermissionRequest,
};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r212-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service(tag: &str) -> EmbedService {
    let root = dir(tag);
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r212".into(),
    })
    .unwrap()
}

/// A surface run carries no `configuration_id`/`environment_ref` (the
/// r2_11 scaffold's shape — `open_run` refuses them).
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

/// The dispatch-shape `ApprovalRequest` — `remedies` are the canonical
/// `Remedy` records the offer carries (the R-2.11 ingress member).
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
        remedies: remedies
            .iter()
            .enumerate()
            .map(|(i, r)| {
                hh_provenance::flow::Remedy::from_json(r, &format!("remedies[{i}]")).unwrap()
            })
            .collect(),
    }
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

fn jstr<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

/// A `sanitize` offer — the non-`approval` arm of the consume sum.
fn sanitize_remedy() -> Json {
    Json::obj([
        ("kind", Json::str("sanitize")),
        ("param", Json::str("path")),
        ("sanitizer_ref", Json::str("san:path-clean")),
    ])
}

// ── D5 — a taken remedy mints `decided{deny, remedy_taken}` ────────────────
//
// The offered `sanitize` take answers the ask with a *deciding* deny —
// the as-proposed args refuse and the take is durable on the same row
// (the `remedies.taken` the re-dispatch verifies against).
#[test]
fn r212_remedy_take_mints_decided_with_remedy_taken() {
    let mut svc = service("take");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-take-1".to_string();
    let remedy = sanitize_remedy();
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-take",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![remedy.clone()],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();

    let out = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Remedy {
                remedy: remedy.clone(),
            },
            "human:op",
            None,
            "req-take-1",
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
        .find(|e| e.class == "security.permission.decided")
        .expect("decided durable");
    let p = &decided.payload;
    assert_eq!(
        jstr(p, "decision"),
        Some("deny"),
        "a non-`approval` take refuses the as-proposed args"
    );
    assert_eq!(
        jstr(p, "reason"),
        Some("remedy:sanitize"),
        "the refusal reason names the taken kind"
    );
    assert_eq!(
        p.get("remedy_taken"),
        Some(&remedy),
        "the take is durable — the re-dispatch's attestation"
    );
    assert_eq!(
        p.get("remedies"),
        Some(&Json::Arr(vec![remedy])),
        "the offered set echoes verbatim beside the take"
    );
    // A deny confers nothing — no grant, no endorsement.
    assert!(
        !envs
            .iter()
            .any(|e| e.class == "security.permission.granted"),
        "a remedy deny mints no grant"
    );
    assert!(
        !envs.iter().any(|e| e.class == "security.label.endorsed"),
        "a remedy deny mints no endorsement"
    );
    // The replay under the same request_id serves the recorded row.
    let again = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Remedy {
                remedy: sanitize_remedy(),
            },
            "human:op",
            None,
            "req-take-1",
        )
        .unwrap();
    assert_eq!(
        again.get("recorded").map(|r| matches!(r, Json::Bool(true))),
        Some(true)
    );
}

// ── D5 — `approval{effect_id}` is the human's allow ────────────────────────
//
// The approval-kind take decides `allow` (the endorsement endorses
// only the named effect — I-F6 R4) and still stamps `remedy_taken` —
// the consume record is the same member either way.
#[test]
fn r212_remedy_approval_decides_allow() {
    let mut svc = service("take-allow");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-appr-1".to_string();
    let effect_id = "ef-appr".to_string();
    let remedy = Json::obj([
        ("kind", Json::str("approval")),
        ("effect_id", Json::str(effect_id.as_str())),
    ]);
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        &effect_id,
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![remedy.clone()],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();

    svc.surface_respond_permission(
        &run_id,
        &lease,
        &pid,
        &PermissionOutcome::Remedy {
            remedy: remedy.clone(),
        },
        "human:op",
        None,
        "req-appr-1",
    )
    .unwrap();
    let envs = events(&svc, &run_id);
    let decided = envs
        .iter()
        .find(|e| e.class == "security.permission.decided")
        .expect("decided durable");
    assert_eq!(jstr(&decided.payload, "decision"), Some("allow"));
    assert_eq!(
        decided.payload.get("remedy_taken"),
        Some(&remedy),
        "the approval take carries the same consume attestation"
    );
}

// ── D5 — the durable offer governs the consume gate ────────────────────────
//
// A remedy the pending never offered is `OptionNotOffered` — the gate
// reads the ledgered `request.remedies`, never a fabricated set.
#[test]
fn r212_remedy_not_offered_refuses() {
    let mut svc = service("take-gate");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-gate-1".to_string();
    // The offer names `sanitize`; the answer claims `shape_endorse`.
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-gate",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![sanitize_remedy()],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();

    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Remedy {
                remedy: Json::obj([
                    ("kind", Json::str("shape_endorse")),
                    ("param", Json::str("path")),
                ]),
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
        "an unoffered remedy refuses OptionNotOffered: {err:?}"
    );
    // A pending that offered *no* remedies gates a remedy answer the
    // same way — absent offer, never a default.
    let pid2 = "perm-gate-2".to_string();
    let pending2 = pending_event(
        &svc,
        &run_id,
        &pid2,
        "ef-gate2",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![],
    );
    svc.surface_append(&run_id, &lease, vec![pending2]).unwrap();
    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid2,
            &PermissionOutcome::Remedy {
                remedy: sanitize_remedy(),
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
        "a remedy answer on a no-offer pending refuses: {err:?}"
    );
    assert!(
        !events(&svc, &run_id)
            .iter()
            .any(|e| e.class == "security.permission.decided"),
        "no decided row lands for a refused take"
    );
}

// ── D5 — the closed sum decodes, never guesses ─────────────────────────────
//
// A `remedy` member that doesn't decode the closed `Remedy` sum is a
// `SchemaViolation{malformed_remedy}` — the boundary refuses before the
// offer gate ever runs.
#[test]
fn r212_remedy_malformed_refuses() {
    let mut svc = service("take-malformed");
    let (run_id, lease) = svc.surface_open_run(surface_manifest()).unwrap();
    let pid = "perm-mal-1".to_string();
    let pending = pending_event(
        &svc,
        &run_id,
        &pid,
        "ef-mal",
        vec![ApprovalOptionId::AllowOnce, ApprovalOptionId::Deny],
        vec![sanitize_remedy()],
    );
    svc.surface_append(&run_id, &lease, vec![pending]).unwrap();

    let err = svc
        .surface_respond_permission(
            &run_id,
            &lease,
            &pid,
            &PermissionOutcome::Remedy {
                remedy: Json::obj([("kind", Json::str("teleport"))]),
            },
            "human:op",
            None,
            "req-mal-1",
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            hh_embed_schema::errors::EmbedError::SchemaViolation { .. }
        ),
        "a non-decode is SchemaViolation, never a guessed kind: {err:?}"
    );
}
