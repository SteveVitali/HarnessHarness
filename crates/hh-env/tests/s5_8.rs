//! S5.8 integration tests — the §5a.5 C2 slice (R-2.2.5²): `memory`
//! snapshots through the `ProviderAdapter` seam (`quiesced` — a
//! `suspended` handle only — and recorded as `foreign_digest` claims),
//! `restore_in_place` behind the tri-state capability gate, and
//! adapter-declared capability honesty (`supported` ⇒ the mechanism is
//! exercised; `unknown`/`unsupported` stay the distinct typed refusals —
//! AC-R-2.2.5-10, T-LCD-07). The attested-microVM legs live in
//! `hh-containment/tests/s4_14b.rs` (`attestation_missing` fail-closed);
//! this file adds the `EnvironmentRecord`-level pairing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_containment::attach::PolicySlot;
use hh_containment::policy::ResourceLimits;
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::handle::{EnvCapabilityDeclaration, HandleState, OnLoss, Roots, Tri};
use hh_env::provider::{ProviderAdapter, ProviderHandle, ProvisionRequest};
use hh_env::record::{
    EnvironmentClass, EnvironmentRecord, ImageAttestation, ImageRef, ProvisioningRecipe,
};
use hh_env::snapshot::{SnapshotKind, TakenBy};
use hh_identity::idp::address;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-s58-{}-{tag}-{n}", std::process::id()));
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

fn test_record(class: EnvironmentClass) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image: ImageRef::ContentAddress(address(b"s58-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
    }
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

#[derive(Default)]
struct Calls {
    memory_snapshots: usize,
    restores: Vec<String>,
}

/// An adapter that genuinely backs `memory` snapshots and
/// `restore_in_place` — and *declares* it (`supported ⇒ exercised`,
/// AC-R-2.2.5-10).
struct MemProvider {
    calls: Arc<Mutex<Calls>>,
    restore_cap: Tri,
}

impl ProviderAdapter for MemProvider {
    fn class(&self) -> EnvironmentClass {
        EnvironmentClass::RemoteEphemeral
    }
    fn capability_declaration(&self) -> EnvCapabilityDeclaration {
        let mut c = EnvCapabilityDeclaration::provider_class();
        c.snapshot.insert("memory".to_string(), Tri::Supported);
        c.restore_in_place = self.restore_cap;
        c
    }
    fn provision(&mut self, req: &ProvisionRequest) -> Result<ProviderHandle, EnvError> {
        Ok(ProviderHandle {
            remote_id: format!("remote:{}", req.env_handle_id),
            region: None,
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
    fn memory_snapshot(&mut self, h: &ProviderHandle) -> Result<String, EnvError> {
        self.calls.lock().unwrap().memory_snapshots += 1;
        Ok(format!("provider-snap:{}", h.remote_id))
    }
    fn restore_in_place(
        &mut self,
        _h: &ProviderHandle,
        snapshot_ref: &str,
    ) -> Result<(), EnvError> {
        self.calls
            .lock()
            .unwrap()
            .restores
            .push(snapshot_ref.to_string());
        Ok(())
    }
}

/// The default-declaration adapter — every provider-class claim stays
/// `unknown` (the honest baseline).
struct BareProvider;

impl ProviderAdapter for BareProvider {
    fn class(&self) -> EnvironmentClass {
        EnvironmentClass::RemoteEphemeral
    }
    fn provision(&mut self, req: &ProvisionRequest) -> Result<ProviderHandle, EnvError> {
        Ok(ProviderHandle {
            remote_id: format!("remote:{}", req.env_handle_id),
            region: None,
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
}

fn provision_remote(s: &mut Store, lease: &Lease, d: &mut EnvDriver, tag: &str) -> String {
    let h = d
        .provision(
            s,
            lease,
            &test_record(EnvironmentClass::RemoteEphemeral),
            roots_for(tag),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
        )
        .unwrap();
    let id = h.env_handle_id.clone();
    let hh = d.handle_mut(&id).unwrap();
    if hh.state == HandleState::Declared {
        hh.transition(HandleState::Provisioning, 1_000).unwrap();
    }
    if hh.state == HandleState::Provisioning {
        hh.transition(HandleState::Ready, 1_001).unwrap();
    }
    id
}

#[test]
fn memory_snapshot_through_adapter_on_suspended_handle() {
    let (mut s, _run, lease) = open("mem-snap");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
        restore_cap: Tri::Supported,
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "ms-ws");
    // The adapter's declaration reached the handle — `memory` supported.
    assert_eq!(
        d.handle(&id)
            .unwrap()
            .capabilities
            .snapshot
            .get("memory")
            .cloned()
            .unwrap_or(Tri::Unknown),
        Tri::Supported
    );
    d.suspend(&mut s, &lease, &id, None).unwrap();
    let rec = d
        .memory_snapshot(&mut s, &lease, &id, TakenBy::Subject)
        .unwrap();
    assert_eq!(rec.kind, SnapshotKind::Memory);
    assert!(rec.quiesced, "memory snapshots are quiesced captures");
    assert!(rec.snapshot_ref.starts_with("sha256:"));
    // The content member is a `foreign_digest` claim — the provider's
    // opaque snapshot id, never a fabricated local address.
    let fd = rec.content.get("foreign_digest").unwrap();
    assert_eq!(fd.get("scheme").unwrap().as_str().unwrap(), "provider");
    assert!(fd
        .get("value")
        .unwrap()
        .as_str()
        .unwrap()
        .starts_with("provider-snap:remote:"));
    assert_eq!(calls.lock().unwrap().memory_snapshots, 1);
    // The durable row is the authority.
    let events = s.events(&_run).unwrap();
    let row = events
        .iter()
        .rfind(|e| e.class == "action.environment.snapshot")
        .expect("snapshot row minted");
    assert_eq!(row.payload.get("kind").unwrap().as_str().unwrap(), "memory");
    assert_eq!(
        row.payload.get("snapshot_ref").unwrap().as_str().unwrap(),
        rec.snapshot_ref
    );
    assert!(d.handle(&id).unwrap().snapshots.contains(&rec.snapshot_ref));
}

#[test]
fn memory_snapshot_requires_quiesced() {
    let (mut s, _run, lease) = open("mem-live");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls,
        restore_cap: Tri::Supported,
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "mq-ws");
    // `ready`, not suspended — a `memory` capture over a live environment
    // is a lie; the kernel refuses InvalidState, not a snapshot.
    let e = d
        .memory_snapshot(&mut s, &lease, &id, TakenBy::Subject)
        .unwrap_err();
    assert!(
        matches!(
            e,
            EnvError::InvalidState {
                op: "snapshot.memory",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn memory_snapshot_undeclared_is_unknown_capability() {
    // A class that never declared `snapshot.memory` (the conservative
    // `provider_class` baseline: every kind `unknown`) refuses
    // `UnknownCapability` — never coerced to `unsupported`, never a
    // fabricated record (S1).
    let (mut s, _run, lease) = open("mem-undecl");
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(BareProvider));
    let id = provision_remote(&mut s, &lease, &mut d, "mu-ws");
    d.suspend(&mut s, &lease, &id, None).unwrap();
    let e = d
        .memory_snapshot(&mut s, &lease, &id, TakenBy::Subject)
        .unwrap_err();
    assert!(matches!(e, EnvError::UnknownCapability { .. }), "{e:?}");
}

#[test]
fn restore_in_place_declared_and_minted() {
    let (mut s, _run, lease) = open("rip-ok");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
        restore_cap: Tri::Supported,
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "ri-ws");
    d.restore_in_place(&mut s, &lease, &id, "provider-snap:remote:x")
        .unwrap();
    assert_eq!(
        calls.lock().unwrap().restores,
        vec!["provider-snap:remote:x".to_string()]
    );
    let row = s
        .events(&_run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.restored")
        .expect("restored row minted")
        .payload
        .clone();
    assert_eq!(row.get("mode").unwrap().as_str().unwrap(), "in_place");
    assert_eq!(
        row.get("reverts").unwrap().as_str().unwrap(),
        "provider-snap:remote:x"
    );
}

#[test]
fn restore_in_place_tri_state_refusals() {
    // `unknown` (never declared) → `UnknownCapability`…
    let (mut s, _run, lease) = open("rip-unk");
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(BareProvider));
    let id = provision_remote(&mut s, &lease, &mut d, "ru-ws");
    let e = d
        .restore_in_place(&mut s, &lease, &id, "snap:x")
        .unwrap_err();
    assert!(matches!(e, EnvError::UnknownCapability { .. }), "{e:?}");

    // …and `unsupported` stays the distinct refusal.
    let (mut s, _run, lease) = open("rip-uns");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls,
        restore_cap: Tri::Unsupported,
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "rus-ws");
    let e = d
        .restore_in_place(&mut s, &lease, &id, "snap:x")
        .unwrap_err();
    assert!(matches!(e, EnvError::Unsupported { .. }), "{e:?}");
}

/// `hibernate` — the hibernation-aware suspend (R-2.2.3²): the provider's
/// memory checkpoint lands inside the same durable batch as the suspend
/// pair, `suspended{suspend_reason:"hibernated", hibernation_snapshot}`
/// names the checkpoint, and `suspended_ms` keeps accruing (no third
/// counter — hibernation is `reserved_ms` under the suspend meter).
#[test]
fn hibernate_takes_memory_snapshot_in_the_suspend_batch() {
    let (mut s, _run, lease) = open("hib-ok");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
        restore_cap: Tri::Supported,
    }));
    let id = provision_remote(&mut s, &lease, &mut d, "hib-ws");
    let rec = d.hibernate(&mut s, &lease, &id).unwrap();
    assert_eq!(rec.kind, SnapshotKind::Memory);
    assert!(rec.quiesced);
    assert_eq!(calls.lock().unwrap().memory_snapshots, 1);
    let h = d.handle(&id).unwrap();
    assert_eq!(h.state, HandleState::Suspended);
    // One batch: suspend.requested + snapshot{memory} + suspended — the
    // suspended row pins the checkpoint the heal restores.
    let events = s.events(&_run).unwrap();
    let suspended = events
        .iter()
        .find(|e| e.class == "action.environment.suspended")
        .expect("suspended row")
        .payload
        .clone();
    assert_eq!(
        suspended.get("on_idle").unwrap().as_str().unwrap(),
        "hibernate"
    );
    assert_eq!(
        suspended.get("suspend_reason").unwrap().as_str().unwrap(),
        "hibernated"
    );
    assert_eq!(
        suspended
            .get("hibernation_snapshot")
            .unwrap()
            .as_str()
            .unwrap(),
        rec.snapshot_ref
    );
    let snap_row = events
        .iter()
        .find(|e| e.class == "action.environment.snapshot")
        .expect("snapshot row")
        .payload
        .clone();
    assert_eq!(snap_row.get("kind").unwrap().as_str().unwrap(), "memory");
}

/// `hibernate` keeps the tri-state refusals — an undeclared `memory`
/// capability is `UnknownCapability`, never a silent fs fallback.
#[test]
fn hibernate_undeclared_memory_is_unknown_capability() {
    let (mut s, _run, lease) = open("hib-unk");
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(BareProvider));
    let id = provision_remote(&mut s, &lease, &mut d, "hu-ws");
    let e = d.hibernate(&mut s, &lease, &id).unwrap_err();
    assert!(matches!(e, EnvError::UnknownCapability { .. }), "{e:?}");
    // Nothing durable — the handle never left `ready` (the refusal ran
    // before the provider legs).
    assert_eq!(d.handle(&id).unwrap().state, HandleState::Ready);
    assert!(s
        .events(&_run)
        .unwrap()
        .iter()
        .all(|e| e.class != "action.environment.suspended"));
}

/// Snapshot-first healing (R-2.2.3²): a `replace` on a hibernated
/// handle restores the recorded `memory` snapshot through the provider's
/// `restore_in_place` — the durable `restored{mode:"snapshot_first"}`
/// row names the kernel `snapshot_ref`, the workspace copy never runs.
#[test]
fn heal_replace_prefers_memory_snapshot() {
    let (mut s, _run, lease) = open("heal-snap");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(&_run);
    d.register_provider(Box::new(MemProvider {
        calls: calls.clone(),
        restore_cap: Tri::Supported,
    }));
    // Provision with `on_loss = replace_from_snapshot`.
    let rec = test_record(EnvironmentClass::RemoteEphemeral);
    let h = d
        .provision(
            &mut s,
            &lease,
            &rec,
            roots_for("hr-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::ReplaceFromSnapshot,
        )
        .unwrap();
    let id = h.env_handle_id.clone();
    let hh = d.handle_mut(&id).unwrap();
    if hh.state == HandleState::Declared {
        hh.transition(HandleState::Provisioning, 1_000).unwrap();
    }
    if hh.state == HandleState::Provisioning {
        hh.transition(HandleState::Ready, 1_001).unwrap();
    }
    let snap = d.hibernate(&mut s, &lease, &id).unwrap();
    // The handle goes unreachable (suspended → unreachable is the
    // hibernated-loss leg the state machine admits).
    d.handle_mut(&id)
        .unwrap()
        .transition(HandleState::Unreachable, 2_000)
        .unwrap();
    let child = d.replace(&mut s, &lease, &id).unwrap();
    assert_ne!(child.env_handle_id, id);
    // The provider's restore ran with its own opaque id (the record's
    // foreign_digest value), and the durable row names the kernel ref.
    let remote = snap
        .content
        .get("foreign_digest")
        .and_then(|fd| fd.get("value"))
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    assert_eq!(calls.lock().unwrap().restores, vec![remote]);
    let events = s.events(&_run).unwrap();
    let restored = events
        .iter()
        .rfind(|e| e.class == "action.environment.restored")
        .expect("restored row")
        .payload
        .clone();
    assert_eq!(
        restored.get("mode").unwrap().as_str().unwrap(),
        "snapshot_first"
    );
    assert_eq!(
        restored.get("reverts").unwrap().as_str().unwrap(),
        snap.snapshot_ref
    );
    assert_eq!(
        restored.get("env_handle").unwrap().as_str().unwrap(),
        child.env_handle_id
    );
}

/// `EnvironmentRecord::verify_attested` — the attested-microVM leg at
/// record resolution (R-2.2.5²): attesting isolation classes require a
/// well-formed `image_attestation`; the refusal is `AttestationMissing`,
/// fail-closed.
#[test]
fn attested_microvm_image_verified_at_resolve() {
    use hh_containment::policy::IsolationClass;
    let mut rec = test_record(EnvironmentClass::RemoteEphemeral);
    // Non-attesting classes pass unconditionally — a declared
    // attestation on a weaker class is a recorded claim, not a gate.
    rec.verify_attested(IsolationClass::ProcessSandbox).unwrap();
    rec.verify_attested(IsolationClass::Namespaces).unwrap();
    // An attesting class without the member refuses.
    let e = rec.verify_attested(IsolationClass::Microvm).unwrap_err();
    assert!(matches!(e, EnvError::AttestationMissing { .. }), "{e:?}");
    let e = rec
        .verify_attested(IsolationClass::UserSpaceKernel)
        .unwrap_err();
    assert!(matches!(e, EnvError::AttestationMissing { .. }), "{e:?}");
    // A malformed member (empty ref) is the same refusal.
    rec.image_attestation = Some(ImageAttestation {
        method: "guest_quote".to_string(),
        attestation_ref: String::new(),
    });
    assert!(matches!(
        rec.verify_attested(IsolationClass::Microvm),
        Err(EnvError::AttestationMissing { .. })
    ));
    // The real pair passes — and the member is identity-bearing on the
    // record (the semantic core carries `image_attestation`).
    rec.image_attestation = Some(ImageAttestation {
        method: "sev_snp_report".to_string(),
        attestation_ref: "sha256:attest-1".to_string(),
    });
    rec.verify_attested(IsolationClass::Microvm).unwrap();
    let record = rec.to_record();
    let semantic = format!("{:?}", record.semantic);
    assert!(semantic.contains("sev_snp_report"), "{semantic:?}");
}
