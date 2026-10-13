//! R2.4 — the environment lifecycle family (`env.open`/`attach`/`close`/
//! `diff`/`restore`/`upload`/`download`; DF-S2.10-1) and the declared
//! `snapshot_cadence` producer (DF-S2.9-3), driven over binding (a).
//!
//! Coverage per the ticket:
//! - every verb succeeds on `local_host` through declared capabilities;
//! - the capability tri-state stays typed at the boundary — `Unsupported`
//!   vs `UnknownCapability` vs `EnvironmentUnavailable`/`Refused` — never
//!   a fabricated result;
//! - `upload`/`download` are content-addressed (a `ContentAddress`
//!   round-trips; raw bytes never cross);
//! - `diff`/`restore` consume run-scoped `fs_tree` snapshot refs — the
//!   successor restore verifies and rebinds the session (the parent
//!   detaches, durable);
//! - the cadence producer's `taken_by:"cadence"` row lands inside the
//!   sink — before `lifecycle.run.finished` — on `on_turn_end`, on a
//!   parked (`on_idle`) drive, and on settled effects
//!   (`every_n_effects:N`);
//! - `env.suspend{on_idle:"hibernate"}`'s memory-snapshot batch keeps its
//!   honest refusal on a class that never declared `snapshot.memory`.
//!
//! The CAP.2 leg (`cap2_fork_at_turn_boundary_without_explicit_snapshot`)
//! is un-ignored green — the fork composes on the cadence take.

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

// ── fixture: the conformance hir/1 document (same shape) ────────────────────

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

/// One dir per tag per process — the service root and the workspace
/// probe must name the same path (tests that need a second service get
/// a second tag).
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
                std::env::temp_dir().join(format!("hh-embed-r24-{}-{tag}", std::process::id(),));
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
        holder: "r2.4".into(),
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

fn err_kind(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
        .to_string()
}

/// `error.data.reason` — the refusal's own text (never the display name).
fn err_reason(resp: &Json) -> String {
    resp.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or_default()
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
                ("name", Json::str("r2.4")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        ("capabilities", caps),
    ])
}

/// The Group M env surface needs `serves_measurement`; `fork` is
/// experimental-tier; the parked-drive leg needs `serves_host_executor`.
fn hello(svc: &mut EmbedService) {
    let r = call(
        svc,
        "hello",
        hello_params(caps_json(&[
            ("experimental", true),
            ("serves_measurement", true),
            ("serves_host_executor", true),
            ("serves_permission_channel", true),
            ("accepts_ephemeral_frames", true),
        ])),
    );
    ok(&r);
}

fn new_spec() -> Json {
    new_spec_with(None, None)
}

/// `new_spec(cadence, doc)` — `cadence` lands as the declared
/// `connection_info{snapshot_cadence}` member (§5a.1).
fn new_spec_with(cadence: Option<&str>, doc: Option<Json>) -> Json {
    let mut ci = vec![("class", Json::str("local_host"))];
    if let Some(c) = cadence {
        ci.push(("snapshot_cadence", Json::str(c)));
    }
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", doc.unwrap_or_else(document_json)),
            ]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                ("connection_info", Json::obj(ci)),
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

/// The parked-drive fixture: interactive attendance + `model_calls:1` —
/// the staged call's second hop exhausts and the loop parks mid-turn
/// (conformance.rs's steerable shape).
fn interactive_spec_with(doc: Json, cadence: Option<&str>) -> Json {
    let mut spec = new_spec_with(cadence, Some(doc));
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

/// The provisioned `local_host` workspace for a service rooted at
/// `test_dir(tag)` — `ServiceConfig::with_root` puts it at `<root>/ws`.
/// Canonicalized: the driver resolves `/var` → `/private/var` before the
/// writable-roots check.
fn ws_dir(tag: &str) -> String {
    let ws = test_dir(tag).join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::canonicalize(&ws).unwrap().display().to_string()
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

fn rows(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.surface_events(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

fn env_op(svc: &mut EmbedService, sid: &str, method: &str, extra: Json) -> Json {
    let mut params = Json::obj([("session_id", Json::str(sid))]);
    if let (Json::Obj(p), Json::Obj(x)) = (&mut params, extra) {
        for (k, v) in x {
            p.insert(k, v);
        }
    }
    call(svc, method, params)
}

// ── the lifecycle verbs ─────────────────────────────────────────────────────

/// `env.open` provisions+attaches a new handle and selects it;
/// `env.close{detach}`/`env.attach` round-trip the survivable leg;
/// `env.close{teardown}` is terminal — the session unbinds and later env
/// ops refuse `no_environment`, never a stale write.
#[test]
fn env_open_attach_close_lifecycle() {
    let mut svc = service("lifecycle");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, run) = session_run(&s);

    // env.open{} — defaults to local_host; the session rebinds.
    let r = ok(&env_op(&mut svc, &sid, "env.open", Json::obj([])));
    assert_eq!(r.get("opened"), Some(&Json::Bool(true)));
    assert_eq!(r.get("state").and_then(Json::as_str), Some("ready"));
    // The selection is durable — `action.environment.selected` minted.
    assert!(rows(&svc, &run)
        .iter()
        .any(|(_, c, _)| c == "action.environment.selected"));

    // env.close{mode:"detach"} → the survivable leg.
    let r = ok(&env_op(
        &mut svc,
        &sid,
        "env.close",
        Json::obj([("mode", Json::str("detach"))]),
    ));
    assert_eq!(r.get("state").and_then(Json::as_str), Some("detached"));
    assert!(rows(&svc, &run)
        .iter()
        .any(|(_, c, _)| c == "action.environment.detached"));

    // env.attach → ready again (the `ready` row lands durable).
    let r = ok(&env_op(&mut svc, &sid, "env.attach", Json::obj([])));
    assert_eq!(r.get("attached"), Some(&Json::Bool(true)));
    assert_eq!(r.get("state").and_then(Json::as_str), Some("ready"));

    // env.close{mode:"teardown"} → terminal; the session unbinds.
    let r = ok(&env_op(&mut svc, &sid, "env.close", Json::obj([])));
    assert_eq!(r.get("state").and_then(Json::as_str), Some("torn_down"));
    assert!(rows(&svc, &run)
        .iter()
        .any(|(_, c, _)| c == "action.environment.torn_down"));
    // Every env op now answers `no_environment` — never a stale write.
    for m in [
        "env.attach",
        "env.close",
        "env.diff",
        "env.restore",
        "env.upload",
        "env.download",
        "env.snapshot",
    ] {
        let e = env_op(&mut svc, &sid, m, Json::obj([]));
        let kind = err_kind(&e);
        assert!(
            kind == "Refused" || kind == "SchemaViolation",
            "{m} after teardown: {e:?}"
        );
        if kind == "Refused" {
            assert!(
                err_reason(&e).contains("no_environment"),
                "{m}: {}",
                err_reason(&e)
            );
        }
    }
}

/// `env.upload`/`env.download` are content-addressed both directions —
/// bytes land in a writable root AND the blob pool; the CA round-trips.
#[test]
fn env_upload_download_content_addressed() {
    let mut svc = service("ud");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, run) = session_run(&s);
    let ws_dir = ws_dir("ud");
    let target = format!("{ws_dir}/r24-up.txt");
    let r = ok(&env_op(
        &mut svc,
        &sid,
        "env.upload",
        Json::obj([
            ("path", Json::str(&target)),
            ("content", Json::str("hello r2.4")),
        ]),
    ));
    let ca = r
        .get("content_address")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert!(ca.starts_with("sha256:"));
    assert_eq!(std::fs::read(&target).unwrap(), b"hello r2.4");
    assert!(rows(&svc, &run)
        .iter()
        .any(|(_, c, _)| c == "action.environment.uploaded"));
    // download → the same ContentAddress.
    let r = ok(&env_op(
        &mut svc,
        &sid,
        "env.download",
        Json::obj([("path", Json::str(&target))]),
    ));
    assert_eq!(
        r.get("content_address").and_then(Json::as_str),
        Some(ca.as_str())
    );
    assert!(rows(&svc, &run)
        .iter()
        .any(|(_, c, _)| c == "action.environment.downloaded"));
    // upload-by-address lands the same bytes under a new name.
    let copy = format!("{ws_dir}/r24-copy.txt");
    let r = ok(&env_op(
        &mut svc,
        &sid,
        "env.upload",
        Json::obj([
            ("path", Json::str(&copy)),
            ("content_address", Json::str(&ca)),
        ]),
    ));
    assert_eq!(
        r.get("content_address").and_then(Json::as_str),
        Some(ca.as_str())
    );
    assert_eq!(std::fs::read(&copy).unwrap(), b"hello r2.4");
    // Honest refusals.
    assert_eq!(
        err_kind(&env_op(
            &mut svc,
            &sid,
            "env.upload",
            Json::obj([("path", Json::str(&target)),])
        )),
        "SchemaViolation"
    );
    assert!(env_op(
        &mut svc,
        &sid,
        "env.upload",
        Json::obj([
            ("path", Json::str("/etc/hh-r24-escape.txt")),
            ("content", Json::str("x")),
        ]),
    )
    .get("error")
    .is_some());
    assert!(env_op(
        &mut svc,
        &sid,
        "env.download",
        Json::obj([("path", Json::str(format!("{ws_dir}/absent")))]),
    )
    .get("error")
    .is_some());
}

/// `env.diff` sees the change-set against a named `fs_tree` baseline;
/// `env.restore` (successor, the default) materialises the baseline in a
/// fresh env, verifies, rebinds the session — and the parent detaches.
#[test]
fn env_diff_restore_successor() {
    let mut svc = service("dr");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, run) = session_run(&s);
    let ws_dir = ws_dir("dr");
    // Baseline: present.txt in, then snapshot.
    let present = format!("{ws_dir}/present.txt");
    let up = ok(&env_op(
        &mut svc,
        &sid,
        "env.upload",
        Json::obj([
            ("path", Json::str(&present)),
            ("content", Json::str("kept")),
        ]),
    ));
    let present_ca = up
        .get("content_address")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let snap = ok(&env_op(&mut svc, &sid, "env.snapshot", Json::obj([])));
    let baseline = snap
        .get("snapshot_ref")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // Change the live tree.
    let added = format!("{ws_dir}/added.txt");
    ok(&env_op(
        &mut svc,
        &sid,
        "env.upload",
        Json::obj([("path", Json::str(&added)), ("content", Json::str("new"))]),
    ));
    // diff — the addition is named.
    let d = ok(&env_op(
        &mut svc,
        &sid,
        "env.diff",
        Json::obj([("baseline", Json::str(&baseline))]),
    ));
    let entries = d.get("entries").and_then(|e| match e {
        Json::Arr(a) => Some(a.clone()),
        _ => None,
    });
    assert!(
        entries.as_ref().map(|e| e.iter().any(|x| {
            x.get("relpath").and_then(Json::as_str) == Some("added.txt")
                && x.get("change").and_then(Json::as_str) == Some("added")
        })) == Some(true),
        "diff: {d:?}"
    );
    // restore — successor, verified; the session rebinds.
    let r = ok(&env_op(
        &mut svc,
        &sid,
        "env.restore",
        Json::obj([("snapshot_ref", Json::str(&baseline))]),
    ));
    assert_eq!(r.get("restored"), Some(&Json::Bool(true)));
    assert_eq!(r.get("mode").and_then(Json::as_str), Some("successor"));
    assert_eq!(r.get("verified"), Some(&Json::Bool(true)));
    let evs = rows(&svc, &run);
    assert!(evs.iter().any(|(_, c, p)| {
        c == "action.environment.restored"
            && p.get("mode").and_then(Json::as_str) == Some("successor")
    }));
    assert!(evs.iter().any(|(_, c, p)| {
        c == "action.environment.detached"
            && p.get("reason").and_then(Json::as_str) == Some("replaced_by_successor")
    }));
    // The successor diffs empty against its restore baseline (positional
    // addressing — the new workspace is a different path).
    let d = ok(&env_op(
        &mut svc,
        &sid,
        "env.diff",
        Json::obj([("baseline", Json::str(&baseline))]),
    ));
    assert_eq!(d.get("count").and_then(Json::as_int), Some(0), "{d:?}");
    // `added.txt` is gone; `present.txt` round-trips content-addressed —
    // in the *successor's* workspace (`<store>/envs/<id>/workspace`).
    let envs_dir = test_dir("dr").join("store").join("envs");
    let mut succ_ws = None;
    if let Ok(rd) = std::fs::read_dir(&envs_dir) {
        for e in rd.flatten() {
            let p = e.path().join("workspace");
            if p.is_dir() {
                succ_ws = Some(std::fs::canonicalize(&p).unwrap());
            }
        }
    }
    let succ_ws = succ_ws.expect("successor workspace exists");
    let dl = ok(&env_op(
        &mut svc,
        &sid,
        "env.download",
        Json::obj([(
            "path",
            Json::str(format!("{}/present.txt", succ_ws.display())),
        )]),
    ));
    assert_eq!(
        dl.get("content_address").and_then(Json::as_str),
        Some(present_ca.as_str())
    );
    // A bogus baseline → typed refusal (SnapshotMissing rides
    // EnvironmentUnavailable's reason — never an empty diff).
    let e = env_op(
        &mut svc,
        &sid,
        "env.diff",
        Json::obj([("baseline", Json::str("sha256:bogus"))]),
    );
    assert_eq!(err_kind(&e), "EnvironmentUnavailable");
}

/// `env.restore{mode:"in_place"}` on `local_host` — `restore_in_place`
/// was never declared for the class → the typed `UnknownCapability`,
/// never a fabricated in-place revert. Tri-state stays tri-state.
#[test]
fn env_restore_in_place_is_typed_unknown_capability() {
    let mut svc = service("rip");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, _run) = session_run(&s);
    let snap = ok(&env_op(&mut svc, &sid, "env.snapshot", Json::obj([])));
    let baseline = snap
        .get("snapshot_ref")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let e = env_op(
        &mut svc,
        &sid,
        "env.restore",
        Json::obj([
            ("snapshot_ref", Json::str(&baseline)),
            ("mode", Json::str("in_place")),
        ]),
    );
    assert_eq!(err_kind(&e), "UnknownCapability", "{e:?}");
}

/// The capability gate — Group M env ops under a hello that never
/// declared `serves_measurement` answer `CapabilityNotDeclared`.
#[test]
fn env_ops_capability_gated_on_hello() {
    let mut svc = service("nogate");
    let r = call(
        &mut svc,
        "hello",
        hello_params(caps_json(&[("experimental", true)])),
    );
    ok(&r);
    let s = open_new(&mut svc, new_spec());
    let (sid, _run) = session_run(&s);
    for m in [
        "env.open",
        "env.attach",
        "env.close",
        "env.diff",
        "env.restore",
        "env.upload",
        "env.download",
    ] {
        assert_eq!(
            err_kind(&env_op(&mut svc, &sid, m, Json::obj([]))),
            "CapabilityNotDeclared",
            "{m}"
        );
    }
}

/// Schema discipline — missing members and unknown spellings are
/// `SchemaViolation`, never a guess.
#[test]
fn env_ops_schema_violations() {
    let mut svc = service("sv");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, _run) = session_run(&s);
    let snap = ok(&env_op(&mut svc, &sid, "env.snapshot", Json::obj([])));
    let real_ref = snap
        .get("snapshot_ref")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    for (m, extra) in [
        ("env.diff", Json::obj([])),
        ("env.restore", Json::obj([])),
        (
            "env.close",
            Json::obj([("mode", Json::str("hibernate-ish"))]),
        ),
        (
            "env.restore",
            Json::obj([
                ("snapshot_ref", Json::str(&real_ref)),
                ("mode", Json::str("sideways")),
            ]),
        ),
        ("env.upload", Json::obj([("path", Json::str("/tmp/x"))])),
        ("env.download", Json::obj([])),
    ] {
        assert_eq!(
            err_kind(&env_op(&mut svc, &sid, m, extra)),
            "SchemaViolation",
            "{m}"
        );
    }
    // An unknown class is the environment-unavailable refusal, not a
    // fabricated provision.
    assert_eq!(
        err_kind(&env_op(
            &mut svc,
            &sid,
            "env.open",
            Json::obj([(
                "connection_info",
                Json::obj([("class", Json::str("bogus"))])
            )])
        )),
        "EnvironmentUnavailable"
    );
}

/// `env.suspend{on_idle:"hibernate"}` — the hibernation batch takes a
/// provider `memory` snapshot; `local_host` never declared
/// `snapshot.memory` → the honest `UnknownCapability`, never a skipped
/// snapshot claiming hibernation.
#[test]
fn env_suspend_hibernate_refuses_undeclared_memory() {
    let mut svc = service("hib");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, _run) = session_run(&s);
    let e = env_op(
        &mut svc,
        &sid,
        "env.suspend",
        Json::obj([("on_idle", Json::str("hibernate"))]),
    );
    assert_eq!(err_kind(&e), "UnknownCapability", "{e:?}");
}

// ── the cadence producer (DF-S2.9-3) ────────────────────────────────────────

/// Default `never` — no declared cadence, no snapshot rows at turn end.
#[test]
fn cadence_default_never_produces_no_rows() {
    let mut svc = service("cad-never");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, run) = session_run(&s);
    ok(&call(
        &mut svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("hello")),
                ])]),
            ),
            ("idempotency_key", Json::str("sub-1")),
        ]),
    ));
    assert!(
        rows(&svc, &run)
            .iter()
            .all(|(_, c, _)| c != "action.environment.snapshot"),
        "never means never"
    );
}

/// `on_turn_end` — the take lands inside the sink between
/// `lifecycle.turn.finished` and `lifecycle.run.finished`.
#[test]
fn cadence_on_turn_end_lands_before_run_finished() {
    let mut svc = service("cad-tt");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec_with(Some("on_turn_end"), None));
    let (sid, run) = session_run(&s);
    ok(&call(
        &mut svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("hello")),
                ])]),
            ),
            ("idempotency_key", Json::str("sub-1")),
        ]),
    ));
    let evs = rows(&svc, &run);
    let (snap_seq, snap_payload) = evs
        .iter()
        .find(|(_, c, p)| {
            c == "action.environment.snapshot"
                && p.get("taken_by").and_then(Json::as_str) == Some("cadence")
        })
        .map(|(s, _, p)| (*s, p.clone()))
        .expect("the cadence take landed durable");
    let finished = evs
        .iter()
        .find(|(_, c, _)| c == "lifecycle.run.finished")
        .map(|(s, _, _)| *s)
        .expect("run finished");
    let turn_finished = evs
        .iter()
        .find(|(_, c, _)| c == "lifecycle.turn.finished")
        .map(|(s, _, _)| *s)
        .expect("turn finished");
    assert!(
        snap_seq > turn_finished && snap_seq < finished,
        "turn@{turn_finished} snap@{snap_seq} run@{finished}"
    );
    // The take's `at_seq` covers the turn boundary it fired on.
    assert_eq!(
        snap_payload.get("at_seq").and_then(Json::as_int),
        Some(turn_finished as i64)
    );
}

/// `on_idle` — a parked drive is the idle boundary: the steerable
/// fixture's second model call exhausts `model_calls:1` and the loop
/// parks mid-turn; the declared cadence takes the `fs_tree` there.
#[test]
fn cadence_on_idle_fires_at_the_park() {
    let mut svc = service("cad-idle");
    hello(&mut svc);
    let s = open_new(
        &mut svc,
        interactive_spec_with(document_json_with("hh/react-steerable"), Some("on_idle")),
    );
    let (sid, run) = session_run(&s);
    ok(&call(
        &mut svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("host.exec.shell")),
                ])]),
            ),
            ("idempotency_key", Json::str("sub-1")),
        ]),
    ));
    let evs = rows(&svc, &run);
    assert!(
        evs.iter().any(|(_, c, p)| {
            c == "action.environment.snapshot"
                && p.get("taken_by").and_then(Json::as_str) == Some("cadence")
        }),
        "the parked drive produced the on_idle take: {:?}",
        evs.iter().map(|(_, c, _)| c.clone()).collect::<Vec<_>>()
    );
}

/// `every_n_effects:N` counts the durable `action.effect.*` terminal
/// rows — the boundary's honest leg is the negative: a declared
/// `every_n_effects` cadence on a run that settles *no* effect rows (the
/// check fixture never intends one — the scripted `hh.submit` settles
/// inside the model port, not the effect lifecycle) produces nothing.
/// The positive count leg is pinned at the driver layer
/// (`hh-env/tests/r2_4.rs::cadence_every_n_effects_counts_the_durable_
/// prefix`) — minting fabricated effect rows here would test the fixture,
/// not the cadence.
#[test]
fn cadence_every_n_effects_fires_only_on_settled_effects() {
    let mut svc = service("cad-n");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec_with(Some("every_n_effects:1"), None));
    let (sid, run) = session_run(&s);
    ok(&call(
        &mut svc,
        "submit",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "input",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("text")),
                    ("text", Json::str("hello")),
                ])]),
            ),
            ("idempotency_key", Json::str("sub-1")),
        ]),
    ));
    let evs = rows(&svc, &run);
    assert!(
        evs.iter().all(|(_, c, _)| !c.starts_with("action.effect.")),
        "the fixture settles no effects"
    );
    assert!(
        evs.iter()
            .all(|(_, c, _)| c != "action.environment.snapshot"),
        "no settled effects → no every_n_effects take"
    );
}

/// An unknown `snapshot_cadence` spelling refuses at `open_session` —
/// `SchemaViolation`, never a guessed cadence.
#[test]
fn cadence_unknown_spelling_is_schema_violation() {
    let mut svc = service("cad-bad");
    hello(&mut svc);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "open_session",
            Json::obj([
                ("spec", new_spec_with(Some("on_everything"), None)),
                ("idempotency_key", Json::str("open-1")),
            ]),
        )),
        "SchemaViolation"
    );
}

/// Read-only sessions (a `fork{env:"none"}` trace child) refuse every
/// env verb — the writer-lease gate is the enforcement, `Refused` the
/// honest answer.
#[test]
fn env_ops_refuse_on_read_only_sessions() {
    let mut svc = service("ro");
    hello(&mut svc);
    let s = open_new(&mut svc, new_spec());
    let (sid, _run) = session_run(&s);
    let f = ok(&call(
        &mut svc,
        "fork",
        Json::obj([
            ("session_id", Json::str(&sid)),
            (
                "at",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(1))]),
            ),
            ("env", Json::str("trace_only")),
        ]),
    ));
    let child_sid = f
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    for m in [
        "env.open",
        "env.close",
        "env.restore",
        "env.upload",
        "env.download",
    ] {
        assert_eq!(
            err_kind(&env_op(&mut svc, &child_sid, m, Json::obj([]))),
            "Refused",
            "{m} on a trace session"
        );
    }
}
