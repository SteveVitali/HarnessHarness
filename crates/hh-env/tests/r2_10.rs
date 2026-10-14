//! R2.10 — DF-S1.13-3's last cell: every Stage-3-class (`needs_adapter`)
//! environment image manifest registers ≥1 canary channel on the run's
//! credential broker at `provision`/`provision_hosted` (§5g.3 §9 — the §05h
//! I3 `environment` bundle member), and the tripwire fires through the
//! broker's existing machinery: any use attempt is a durable
//! `security.secret.leak_detected` + `Refused{canary}`.
//!
//! Fail-closed admissions checked here: no declared canary on a Stage-3
//! class (`missing`), a declared member that is not `canary: true`
//! (`non_canary`), a declared canary with no broker to register against
//! (`broker_absent`), and a channel-id collision under a different spec
//! (`register:*`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_containment::attach::PolicySlot;
use hh_containment::policy::ResourceLimits;
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::handle::{OnLoss, Roots};
use hh_env::provider::{ProviderAdapter, ProviderHandle, ProviderMeter, ProvisionRequest};
use hh_env::record::{
    CanaryChannel, EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe,
};
use hh_identity::idp::address;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_secrets::{CredentialBroker, RefusedCode, SecretChannelSpec, StaticVault};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r210-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn roots_for(tag: &str) -> Roots {
    let ws = dir(tag);
    std::fs::create_dir_all(&ws).unwrap();
    Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    }
}

fn test_broker() -> CredentialBroker {
    CredentialBroker::new(Box::new(StaticVault::default()), "canary-test-key")
}

/// The manifest's canary channel declaration — `canary: true`,
/// `kernel_only`/`bindable: false`, no destinations (it is never injected;
/// the source coordinate never resolves because every access refuses
/// first).
fn test_canary(channel_id: &str) -> CanaryChannel {
    CanaryChannel {
        channel_id: channel_id.to_string(),
        spec: SecretChannelSpec {
            kind: hh_secrets::CredentialKind::ApiKey,
            source: hh_secrets::SecretSource::OperatorVault {
                vault_ref: format!("canary:{channel_id}"),
            },
            destinations: vec![],
            allowed_env_names: None,
            delivery_modes: BTreeSet::new(),
            max_lifetime_ms: None,
            rotation_policy: None,
            sender_constraint: hh_secrets::SenderConstraint::None,
            constraints: Default::default(),
            bindable: false,
            access_class: hh_secrets::AccessClass::KernelOnly,
            canary: true,
            description: "image-manifest tripwire".into(),
        },
    }
}

fn test_record(class: EnvironmentClass, canaries: Vec<CanaryChannel>) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image: ImageRef::ContentAddress(address(b"r210-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        canary_channels: canaries,
        ext: BTreeMap::new(),
    }
}

#[derive(Default)]
struct Calls {
    provisioned: usize,
}

/// A scripted `ProviderAdapter` — the minimum honest Stage-3 substrate seam.
struct MockProvider {
    class: EnvironmentClass,
    calls: Arc<Mutex<Calls>>,
}

impl ProviderAdapter for MockProvider {
    fn class(&self) -> EnvironmentClass {
        self.class
    }
    fn provision(&mut self, req: &ProvisionRequest) -> Result<ProviderHandle, EnvError> {
        self.calls.lock().unwrap().provisioned += 1;
        Ok(ProviderHandle {
            remote_id: format!("remote:{}", req.env_handle_id),
            region: Some("test-region-1".to_string()),
            extra: BTreeMap::new(),
        })
    }
    fn suspend(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn resume(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn teardown(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        Ok(())
    }
    fn meters(&mut self, _h: &ProviderHandle) -> Result<Vec<ProviderMeter>, EnvError> {
        Ok(vec![])
    }
}

fn register(run_id: &str, class: EnvironmentClass) -> EnvDriver {
    let mut d = EnvDriver::new(run_id);
    d.register_provider(Box::new(MockProvider {
        class,
        calls: Arc::new(Mutex::new(Calls::default())),
    }));
    d
}

fn provisioned_payload(s: &Store, run_id: &str) -> Json {
    s.events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "action.environment.provisioned")
        .map(|e| e.payload.clone())
        .last()
        .expect("a provisioned row")
}

fn leak_rows(s: &Store, run_id: &str) -> Vec<Json> {
    s.events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.secret.leak_detected")
        .map(|e| e.payload.clone())
        .collect()
}

// ── fail-closed admissions ──────────────────────────────────────────────

/// A Stage-3-class image manifest with **no** `canary_channels` member
/// refuses `CanaryManifest{missing}` — the obligation is enforced at
/// provision, not by convention.
#[test]
fn r2_10_stage3_image_without_canary_refuses() {
    let (mut s, run, lease) = open("missing");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::RemoteEphemeral, vec![]),
            roots_for("m-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap_err();
    match e {
        EnvError::CanaryManifest { detail } => assert!(detail.starts_with("missing:")),
        other => panic!("expected CanaryManifest, got {other:?}"),
    }
    // The refusal is before any durable row — no `declared` was minted.
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .all(|e| !e.class.starts_with("action.environment.")));
}

/// A declared canary on a manifest provisioned with **no** broker refuses
/// `broker_absent` — the tripwire is never silently dropped.
#[test]
fn r2_10_declared_canary_without_broker_refuses() {
    let (mut s, run, lease) = open("nobroker");
    let mut d = register(&run, EnvironmentClass::RemotePersistent);
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::RemotePersistent,
                vec![test_canary("img.canary")],
            ),
            roots_for("nb-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            None,
        )
        .unwrap_err();
    match e {
        EnvError::CanaryManifest { detail } => assert!(detail.starts_with("broker_absent")),
        other => panic!("expected CanaryManifest, got {other:?}"),
    }
}

/// A `canary_channels` member whose spec is not `canary: true` refuses
/// `non_canary` — the tripwire list holds tripwires only.
#[test]
fn r2_10_non_canary_member_refuses() {
    let (mut s, run, lease) = open("noncanary");
    let mut d = register(&run, EnvironmentClass::ProviderHosted);
    let mut bogus = test_canary("img.not-canary");
    bogus.spec.canary = false;
    let mut broker = test_broker();
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::ProviderHosted, vec![bogus]),
            roots_for("nc-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap_err();
    match e {
        EnvError::CanaryManifest { detail } => assert!(detail.starts_with("non_canary:")),
        other => panic!("expected CanaryManifest, got {other:?}"),
    }
}

/// A channel id already registered under a **different** spec is a manifest
/// conflict — `register:<id>` refuses rather than shadowing.
#[test]
fn r2_10_channel_id_conflict_refuses() {
    let (mut s, run, lease) = open("conflict");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    // Pre-register the id under a different (still-canary) spec.
    let mut other = test_canary("img.canary");
    other.spec.description = "different".into();
    broker
        .register_channel(
            "img.canary",
            other.spec,
            hh_provenance::ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::RemoteEphemeral,
                vec![test_canary("img.canary")],
            ),
            roots_for("c-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap_err();
    match e {
        EnvError::CanaryManifest { detail } => assert!(detail.starts_with("register:")),
        other => panic!("expected CanaryManifest, got {other:?}"),
    }
}

// ── the registered tripwire fires ───────────────────────────────────────

/// Provisioning a Stage-3-class environment registers the manifest's canary
/// channels on the run's broker; the durable `provisioned` row records the
/// ids; and any `bind` on the canary lands `leak_detected` +
/// `Refused{canary}` — the injection battery's cell.
#[test]
fn r2_10_stage3_provision_registers_canary_and_tripwire_fires() {
    let (mut s, run, lease) = open("canary");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    let h = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::RemoteEphemeral,
                vec![test_canary("img.canary")],
            ),
            roots_for("c2-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    assert_eq!(h.canary_channels, vec!["img.canary".to_string()]);

    // The broker holds the registered canary channel.
    let ch = broker.channel("img.canary").expect("canary registered");
    assert!(ch.spec.canary);

    // The durable `provisioned` row records the registered ids.
    let p = provisioned_payload(&s, &run);
    let ids: Vec<&str> = match p.get("canary_channels") {
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).collect(),
        other => panic!("provisioned.canary_channels: {other:?}"),
    };
    assert_eq!(ids, vec!["img.canary"]);

    // The tripwire: `bind` on the canary refuses `canary` and the
    // `leak_detected` row is durable — the injection leg.
    let r = broker.bind(
        &mut s,
        &run,
        &lease,
        hh_secrets::BindRequest {
            channel_id: "img.canary".into(),
            holder: "agent.main".into(),
            env_handle_ref: h.env_handle_id.clone(),
            env_isolation: hh_containment::policy::IsolationClass::External,
            mode: hh_monitor::assess::SecretTransport::ProxyInjected,
            monitor_decision_ref: "evt:dec-1".into(),
            destinations: ["api.evil.example.com".into()].into_iter().collect(),
        },
    );
    match r {
        Err(e) => assert_eq!(e.code, RefusedCode::Canary),
        Ok(_) => panic!("a canary channel must never bind"),
    }
    let leaks = leak_rows(&s, &run);
    assert_eq!(leaks.len(), 1, "one leak_detected row, durable");
    assert_eq!(
        leaks[0].get("detector").and_then(Json::as_str),
        Some("canary")
    );
}

/// The hosted path (`provision_hosted`, AC-R-2.2.5-13's lane) is the same
/// Stage-3-class admission: the canary registers with the broker.
#[test]
fn r2_10_hosted_provision_registers_canary() {
    let (mut s, run, lease) = open("hosted");
    let mut d = register(&run, EnvironmentClass::ProviderHosted);
    let mut broker = test_broker();
    let h = d
        .provision_hosted(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::ProviderHosted,
                vec![test_canary("img.canary")],
            ),
            roots_for("h-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            "participant:test",
            "hh.hosting/1:test",
            &[],
            Some(&mut broker),
        )
        .unwrap();
    assert_eq!(h.canary_channels, vec!["img.canary".to_string()]);
    assert!(broker.channel("img.canary").unwrap().spec.canary);
}

/// Re-provisioning the same image on the same broker is idempotent — the
/// `ChannelExists` leg is satisfied only by an *identical* canary spec.
#[test]
fn r2_10_reprovision_same_image_is_idempotent() {
    let (mut s, run, lease) = open("idem");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    for tag in ["i1-ws", "i2-ws"] {
        d.provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::RemoteEphemeral,
                vec![test_canary("img.canary")],
            ),
            roots_for(tag),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    }
    assert!(broker.channel("img.canary").unwrap().spec.canary);
}

/// A derived child inherits the parent's registered canary ids — a fork of
/// a bound Stage-3 environment keeps the tripwire visible on its handle.
#[test]
fn r2_10_derived_child_inherits_canary_ids() {
    let (mut s, run, lease) = open("derive");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    let parent = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::RemoteEphemeral,
                vec![test_canary("img.canary")],
            ),
            roots_for("d-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    // Walk the machine to `ready` — derive requires it (provision already
    // lands the handle in `provisioning`).
    {
        let h = d.handle_mut(&parent.env_handle_id).unwrap();
        if h.state == hh_env::handle::HandleState::Declared {
            h.transition(hh_env::handle::HandleState::Provisioning, 1_000)
                .unwrap();
        }
        if h.state == hh_env::handle::HandleState::Provisioning {
            h.transition(hh_env::handle::HandleState::Ready, 1_001)
                .unwrap();
        }
    }
    let child = d
        .derive(
            &mut s,
            &lease,
            &parent.env_handle_id,
            hh_env::handle::DeriveMode::FreshFromImage,
            None,
            hh_env::handle::OnParentEnd::Teardown,
        )
        .unwrap();
    assert_eq!(child.canary_channels, vec!["img.canary".to_string()]);
}

/// A local-class record may declare a canary — it registers the same way —
/// but declaring one with no broker refuses (never silently dropped); and
/// a local record with none stays `vec![]`.
#[test]
fn r2_10_local_class_declared_canary_registers() {
    let (mut s, run, lease) = open("local");
    let mut d = EnvDriver::new(&run);
    let mut broker = test_broker();
    let h = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::LocalHost,
                vec![test_canary("local.canary")],
            ),
            roots_for("l-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    assert_eq!(h.canary_channels, vec!["local.canary".to_string()]);

    // Declared + no broker → broker_absent.
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(
                EnvironmentClass::LocalHost,
                vec![test_canary("local.canary2")],
            ),
            roots_for("l2-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            None,
        )
        .unwrap_err();
    assert!(matches!(e, EnvError::CanaryManifest { .. }));
}

/// The durable rows carry no secret material — only channel ids (CC2).
#[test]
fn r2_10_provisioned_row_is_content_free() {
    let (mut s, run, lease) = open("contentfree");
    let mut d = register(&run, EnvironmentClass::RemoteEphemeral);
    let mut broker = test_broker();
    d.provision(
        &mut s,
        &lease,
        &test_record(
            EnvironmentClass::RemoteEphemeral,
            vec![test_canary("img.canary")],
        ),
        roots_for("cf-ws"),
        PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
        OnLoss::FailRun,
        Some(&mut broker),
    )
    .unwrap();
    // `broker_absent`-style vault refs are coordinates — no payload may
    // carry the spec's vault_ref verbatim… the canary's source coordinate
    // is declared `canary:<id>`; the durable record holds only the id.
    let all: String = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.payload.to_canonical_string())
        .collect();
    assert!(!all.contains("canary:img.canary"));
    assert!(all.contains("img.canary"));
    // And nothing broker-side resolved: no `used`/`bound` rows exist for a
    // canary (it is never injected).
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .all(|e| !e.class.starts_with("security.credential.")));
}
