//! R4.15e acceptance coverage — the OQ-170 signer custody end-to-end
//! through `EmbedService` (ADR-0376): boot-time `<store_root>/secrets/`
//! loading, `kernel_use(KernelPurpose::LedgerSigning)` provisioning with
//! its durable `security.credential.used` row, `signer_key_ids` stamped
//! on every manifest the kernel opens, the `kind = final` checkpoint at
//! `lifecycle.run.finished`, the co-signed custody rotation, the typed
//! boot refusal on a mis-permissioned key file, and the no-leak property
//! (key bytes never reach a ledger payload).
//!
//! Each test fails if the behaviour it covers is removed.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::LedgerSignerCustody;
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

// ── service helpers ─────────────────────────────────────────────────────────

fn root_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r415e-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// The custody directory the service scans at boot —
/// `<store_root>/secrets/` (OQ-170's per-installation home).
fn custody_dir(root: &Path) -> PathBuf {
    root.join("store").join("secrets")
}

fn service_in(root: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.15e".into(),
    })
    .unwrap()
}

fn service_open_result(root: &Path) -> Result<EmbedService, hh_embed_schema::errors::EmbedError> {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r4.15e".into(),
    })
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
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r4.15e")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj(vec![
                    ("serves_host_executor", Json::Bool(true)),
                    ("accepts_ephemeral_frames", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    ok(&r);
}

fn open_new(svc: &mut EmbedService) -> Json {
    ok(&call(
        svc,
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
            ("idempotency_key", Json::str("open-1")),
        ]),
    ))
}

fn submit(svc: &mut EmbedService, session: &str, text: &str) -> Json {
    call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("text")),
                    ("text", Json::str(text)),
                ])]),
            ),
            ("idempotency_key", Json::str("submit-1")),
        ]),
    )
}

/// The kernel-internal custody run — its manifest's `extra.purpose` is
/// `ledger_signing` (never user-visible, never model-facing).
fn custody_run(svc: &EmbedService) -> String {
    svc.store()
        .run_ids()
        .into_iter()
        .find(|r| {
            svc.store()
                .manifest(r)
                .ok()
                .and_then(|m| m.extra.get("purpose"))
                .and_then(Json::as_str)
                == Some("ledger_signing")
        })
        .expect("the custody run exists under a provisioned boot")
}

fn checkpoint_kinds(svc: &EmbedService, run: &str) -> Vec<String> {
    svc.store()
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.audit.checkpoint")
        .filter_map(|e| {
            e.payload
                .get("kind")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect()
}

// ── the honest postures ─────────────────────────────────────────────────────

#[test]
fn unprovisioned_boot_stays_honestly_unsigned() {
    let root = root_dir("unsigned");
    let mut svc = service_in(&root);
    assert!(
        svc.signer_key_ids().is_empty(),
        "no custody → no signer ids"
    );
    assert!(svc
        .mint_custody_checkpoint(hh_ledger::audit::CheckpointKind::OnDemand)
        .is_err());
}

#[test]
fn provisioned_boot_loads_provisions_and_signs() {
    let root = root_dir("boot");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let svc = service_in(&root);
    let kids = svc.signer_key_ids();
    assert_eq!(kids.len(), 1, "the provisioned key id is the signer set");
    assert!(kids[0].starts_with("ed25519-"));
    // The custody run: kernel-internal, `purpose = ledger_signing`,
    // manifest declares the signer set.
    let crun = custody_run(&svc);
    let manifest = svc.store().manifest(&crun).unwrap();
    assert_eq!(manifest.signer_key_ids, kids);
    // The durable kernel_use row — `security.credential.used{destination
    // = kernel:ledger_signing}` — landed before the value materialised.
    let used = svc
        .store()
        .events(&crun)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.credential.used")
        .collect::<Vec<_>>();
    assert_eq!(used.len(), 1, "one kernel_use row at boot");
    assert_eq!(
        used[0].payload.get("destination").and_then(Json::as_str),
        Some("kernel:ledger_signing")
    );
    // Sign-on-boot: the custody run's first checkpoint is an `on_demand`
    // claim signed under the Ed25519 alg, and `verify` accepts it.
    let kinds = checkpoint_kinds(&svc, &crun);
    assert!(kinds.contains(&"on_demand".to_string()), "{kinds:?}");
    let cp = svc
        .store()
        .events(&crun)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.audit.checkpoint")
        .unwrap();
    let sig_entry = match cp.payload.get("signatures") {
        Some(Json::Arr(items)) => items.first().expect("the boot checkpoint is signed"),
        _ => panic!("the boot checkpoint carries signatures[]"),
    };
    assert_eq!(
        sig_entry.get("alg_ref").and_then(Json::as_str),
        Some("ed25519")
    );
    svc.store().verify(&crun).unwrap();
}

#[test]
fn a_mispermissioned_key_file_refuses_the_boot() {
    let root = root_dir("perms");
    let cdir = custody_dir(&root);
    LedgerSignerCustody::provision(&cdir).unwrap();
    let key_file = std::fs::read_dir(&cdir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            n.ends_with(".pkcs8").then(|| cdir.join(n))
        })
        .next()
        .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o644)).unwrap();
    match service_open_result(&root) {
        Err(e) => {
            let text = format!("{e:?}");
            assert!(text.contains("signer_custody"), "{text}");
        }
        Ok(_) => panic!("a 0644 key file must refuse the boot, never silently unsigned"),
    }
}

// ── manifests + the final checkpoint ────────────────────────────────────────

#[test]
fn session_manifest_stamps_the_kernel_signer_set() {
    let root = root_dir("stamp");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let mut svc = service_in(&root);
    hello(&mut svc);
    let s = open_new(&mut svc);
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let manifest = svc.store().manifest(&run).unwrap();
    assert_eq!(
        manifest.signer_key_ids,
        svc.signer_key_ids(),
        "the kernel confers its signer set — never caller-declared"
    );
    assert!(!manifest.signer_key_ids.is_empty());
}

#[test]
fn a_finished_run_earns_a_signed_final_checkpoint() {
    let root = root_dir("final");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let mut svc = service_in(&root);
    hello(&mut svc);
    let s = open_new(&mut svc);
    let sid = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    ok(&submit(&mut svc, &sid, "hi"));
    // The scripted turn completes → `lifecycle.run.finished` durable, and
    // the scan's emission leg mints `kind = final` covering it.
    let events = svc.store().events(&run).unwrap();
    assert!(events.iter().any(|e| e.class == "lifecycle.run.finished"));
    let kinds = checkpoint_kinds(&svc, &run);
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("final"),
        "the terminal claim is the run's last checkpoint: {kinds:?}"
    );
    svc.store().verify(&run).unwrap();
    // The run's verify through the boundary also passes (the op is
    // session-scoped — `session_id`, not `run_id`).
    ok(&call(
        &mut svc,
        "verify",
        Json::obj([("session_id", Json::str(sid))]),
    ));
}

// ── rotation end-to-end ─────────────────────────────────────────────────────

#[test]
fn rotation_cosigns_and_preserves_history() {
    let root = root_dir("rotate");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let mut svc = service_in(&root);
    let incumbent = svc.signer_key_ids()[0].clone();
    let successor = svc.rotate_ledger_signer().unwrap();
    assert_ne!(incumbent, successor);
    // Both ids ride the conferred signer set — retired stays declared.
    let ids = svc.signer_key_ids();
    assert!(
        ids.contains(&incumbent) && ids.contains(&successor),
        "{ids:?}"
    );
    // The co-signed `kind = rotation` claim is durable on the custody run.
    let crun = custody_run(&svc);
    let rot = svc
        .store()
        .events(&crun)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.audit.checkpoint" && e.payload.get("key_rotation").is_some())
        .expect("the key_rotation checkpoint");
    let kr = rot.payload.get("key_rotation").unwrap();
    assert_eq!(
        kr.get("from_key_id").and_then(Json::as_str),
        Some(incumbent.as_str())
    );
    assert_eq!(
        kr.get("to_key_id").and_then(Json::as_str),
        Some(successor.as_str())
    );
    let signers: Vec<&str> = match rot.payload.get("signatures") {
        Some(Json::Arr(items)) => items
            .iter()
            .filter_map(|s| s.get("key_id").and_then(Json::as_str))
            .collect(),
        _ => panic!("the rotation claim carries signatures[]"),
    };
    assert!(signers.contains(&incumbent.as_str()), "incumbent co-signs");
    assert!(signers.contains(&successor.as_str()), "successor co-signs");
    // A second `kernel_use` row for the successor provision.
    let used = svc
        .store()
        .events(&crun)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.credential.used")
        .count();
    assert_eq!(used, 2, "boot + rotation each provision through kernel_use");
    // The whole custody run — boot checkpoint, rotation, both sig algs —
    // verifies end-to-end.
    svc.store().verify(&crun).unwrap();
}

#[test]
fn a_new_session_after_rotation_declares_both_keys() {
    let root = root_dir("rotafter");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let mut svc = service_in(&root);
    svc.rotate_ledger_signer().unwrap();
    hello(&mut svc);
    let s = open_new(&mut svc);
    let run = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let manifest = svc.store().manifest(&run).unwrap();
    assert_eq!(manifest.signer_key_ids.len(), 2);
    ok(&submit(
        &mut svc,
        s.get("session_id").and_then(Json::as_str).unwrap(),
        "hi",
    ));
    // The final checkpoint mints under the *successor* — and verifies.
    svc.store().verify(&run).unwrap();
}

// ── custody survives the host resolver swap ─────────────────────────────────

#[test]
fn set_credential_resolver_preserves_the_signer_channel() {
    let root = root_dir("swap");
    LedgerSignerCustody::provision(&custody_dir(&root)).unwrap();
    let mut svc = service_in(&root);
    // A host installing its own resolver must not orphan the signer
    // channel — the custody source composes in front of it.
    svc.set_credential_resolver(Box::new(hh_secrets::DenyAllResolver));
    let new_id = svc.rotate_ledger_signer().unwrap();
    assert!(svc.signer_key_ids().contains(&new_id));
    let crun = custody_run(&svc);
    svc.store().verify(&crun).unwrap();
}

// ── no leak through the embed surface ───────────────────────────────────────

#[test]
fn no_key_material_reaches_any_ledger_payload() {
    let root = root_dir("noleak");
    let cdir = custody_dir(&root);
    LedgerSignerCustody::provision(&cdir).unwrap();
    let mut svc = service_in(&root);
    hello(&mut svc);
    let s = open_new(&mut svc);
    let sid = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    ok(&submit(&mut svc, &sid, "hi"));
    svc.rotate_ledger_signer().unwrap();
    // Every key file's raw bytes + hex — none may appear in ANY event
    // payload across ALL runs (custody run, session runs, system runs).
    let mut forbidden: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&cdir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) == Some("pkcs8") {
            let bytes = std::fs::read(&p).unwrap();
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            forbidden.push(hex);
            let frag: String = bytes[..32.min(bytes.len())]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            forbidden.push(frag);
        }
    }
    assert!(!forbidden.is_empty());
    for run in svc.store().run_ids() {
        for e in svc.store().events(&run).unwrap() {
            let text = e.payload.to_canonical_string();
            for needle in &forbidden {
                assert!(
                    !text.contains(needle),
                    "key material leaked into {} on run {run}",
                    e.class
                );
            }
        }
    }
}
