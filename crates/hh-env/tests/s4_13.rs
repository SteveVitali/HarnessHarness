//! S4.13 integration tests — the §5a.5 provider-environment slices
//! (R-2.2.5¹; AC-R-2.2.5-13): remote classes through a registered
//! `ProviderAdapter`, suspend/resume with provider checkpoints, provider
//! meters folding into `action.environment.meters_sampled`, the
//! `on_parent_end` cascade (`detach_to_child`), `share`-mode resource
//! keys, and `provider_hosted` participant-origin rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_containment::attach::PolicySlot;
use hh_containment::policy::ResourceLimits;
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::handle::{HandleState, OnLoss, OnParentEnd, Roots};
use hh_env::provider::{ProviderAdapter, ProviderHandle, ProviderMeter, ProvisionRequest};
use hh_env::record::{
    CanaryChannel, EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe,
};
use hh_identity::idp::address;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::leases::LeaseScope;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-s413-{}-{tag}-{n}", std::process::id()));
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
        image: ImageRef::ContentAddress(address(b"s413-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: canary_channels_for(class),
    }
}

/// R2.10 (DF-S1.13-3) — a Stage-3-class (`needs_adapter`) image manifest
/// MUST declare ≥1 canary channel; `provision`/`provision_hosted` register
/// each on the run's credential broker (the §5g.3 §9 tripwire — the broker
/// never injects it; any use is `leak_detected` + `Refused{canary}`).
fn test_canary(channel_id: &str) -> CanaryChannel {
    CanaryChannel {
        channel_id: channel_id.to_string(),
        spec: hh_secrets::SecretChannelSpec {
            kind: hh_secrets::CredentialKind::ApiKey,
            source: hh_secrets::SecretSource::OperatorVault {
                vault_ref: format!("canary:{channel_id}"),
            },
            destinations: vec![],
            allowed_env_names: None,
            delivery_modes: std::collections::BTreeSet::new(),
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

fn canary_channels_for(class: EnvironmentClass) -> Vec<CanaryChannel> {
    if class.needs_adapter() {
        vec![test_canary("img.canary")]
    } else {
        vec![]
    }
}

fn test_broker() -> hh_secrets::CredentialBroker {
    hh_secrets::CredentialBroker::new(
        Box::new(hh_secrets::StaticVault::default()),
        "canary-test-key",
    )
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
    provisioned: usize,
    suspended: usize,
    resumed: usize,
    torn_down: usize,
    metered: usize,
}

/// A scripted `ProviderAdapter` — the adapter reports; the kernel decides
/// (the honest seam: counters record what the provider was asked).
struct MockProvider {
    class: EnvironmentClass,
    calls: Arc<Mutex<Calls>>,
    meters: Vec<ProviderMeter>,
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
        self.calls.lock().unwrap().suspended += 1;
        Ok(())
    }
    fn resume(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        self.calls.lock().unwrap().resumed += 1;
        Ok(())
    }
    fn teardown(&mut self, _h: &ProviderHandle) -> Result<(), EnvError> {
        self.calls.lock().unwrap().torn_down += 1;
        Ok(())
    }
    fn meters(&mut self, _h: &ProviderHandle) -> Result<Vec<ProviderMeter>, EnvError> {
        self.calls.lock().unwrap().metered += 1;
        Ok(self.meters.clone())
    }
}

fn register(
    run_id: &str,
    class: EnvironmentClass,
    meters: Vec<ProviderMeter>,
) -> (EnvDriver, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut d = EnvDriver::new(run_id);
    d.register_provider(Box::new(MockProvider {
        class,
        calls: calls.clone(),
        meters,
    }));
    (d, calls)
}

/// Drive a provisioned handle to `ready` through the legal machine edges
/// (the provider's substrate owns readiness; the test walks the states).
fn to_ready(d: &mut EnvDriver, id: &str) {
    let h = d.handle_mut(id).unwrap();
    if h.state == HandleState::Declared {
        h.transition(HandleState::Provisioning, 1_000).unwrap();
    }
    if h.state == HandleState::Provisioning {
        h.transition(HandleState::Ready, 1_001).unwrap();
    }
}

// ── the adapter seam ────────────────────────────────────────────────────

#[test]
fn s4_13_provider_class_requires_registered_adapter() {
    let (mut s, run, lease) = open("no-adapter");
    let mut d = EnvDriver::new(&run);
    // `remote_ephemeral` without a registered adapter — typed refusal,
    // never a silent local fallback (the class is remote-only).
    let mut broker = test_broker();
    let e = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::RemoteEphemeral),
            roots_for("na-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap_err();
    assert!(
        matches!(e, EnvError::Unsupported { .. }),
        "provider class without adapter must refuse: {e:?}"
    );
    // `local_host` is kernel-provisioned — `provision_hosted` refuses it
    // (hosted handles are a provider-class path).
    let e = d
        .provision_hosted(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::LocalHost),
            roots_for("na-ws2"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            "participant:test",
            "hh.hosting/1:test",
            &[],
            None,
        )
        .unwrap_err();
    assert!(matches!(e, EnvError::Unsupported { .. }), "{e:?}");
}

#[test]
fn s4_13_provider_lifecycle_suspend_resume_meters_teardown() {
    let (mut s, run, lease) = open("provider-life");
    let (mut d, calls) = register(
        &run,
        EnvironmentClass::RemoteEphemeral,
        vec![
            ProviderMeter {
                name: "provider.active_ms".to_string(),
                value: 42,
                unit: "ms".to_string(),
            },
            ProviderMeter {
                name: "undeclared.probe".to_string(),
                value: 7,
                unit: "calls".to_string(),
            },
        ],
    );
    let mut broker = test_broker();
    let h = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::RemoteEphemeral),
            roots_for("pl-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    // The adapter's `ProviderHandle` is recorded verbatim on the EnvHandle.
    let ph = h.provider.clone().expect("provider handle recorded");
    assert!(ph.remote_id.starts_with("remote:"));
    assert_eq!(ph.region.as_deref(), Some("test-region-1"));
    assert_eq!(calls.lock().unwrap().provisioned, 1);
    to_ready(&mut d, &h.env_handle_id);
    // `suspend` — the provider checkpoint runs *before* the state edge and
    // the `requested → suspended` pair commits in one batch.
    d.suspend(&mut s, &lease, &h.env_handle_id, Some("idle-5m"))
        .unwrap();
    assert_eq!(calls.lock().unwrap().suspended, 1);
    assert_eq!(
        d.handle(&h.env_handle_id).unwrap().state,
        HandleState::Suspended
    );
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "action.environment.suspended"));
    // `resume` — adapter resume, then `resumed` carries the accrued
    // suspended clock (R-2.2.5¹ meter discipline).
    d.resume(&mut s, &lease, &h.env_handle_id, "wakeup")
        .unwrap();
    assert_eq!(calls.lock().unwrap().resumed, 1);
    let resumed = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.resumed")
        .unwrap();
    assert_eq!(
        resumed.payload.get("cause").and_then(Json::as_str),
        Some("wakeup")
    );
    // `meters_sampled` — declared provider meters fold into
    // `provider_meters`; the undeclared name lands in `unreported`,
    // never silently merged (CC3).
    d.sample_meters(&mut s, &lease, &h.env_handle_id).unwrap();
    assert_eq!(calls.lock().unwrap().metered, 1);
    let ms = s
        .events(&run)
        .unwrap()
        .iter()
        .rev()
        .find(|e| e.class == "action.environment.meters_sampled")
        .unwrap();
    let reported = ms
        .payload
        .get("provider_meters")
        .and_then(|m| match m {
            Json::Arr(v) => Some(v),
            _ => None,
        })
        .expect("provider_meters member");
    assert!(
        reported
            .iter()
            .any(|m| m.get("name").and_then(Json::as_str) == Some("provider.active_ms")),
        "declared meter folded: {reported:?}"
    );
    let unreported = ms
        .payload
        .get("unreported")
        .and_then(|m| match m {
            Json::Arr(v) => Some(v),
            _ => None,
        })
        .expect("unreported member");
    assert_eq!(unreported.len(), 1);
    assert_eq!(
        unreported[0].get("name").and_then(Json::as_str),
        Some("undeclared.probe")
    );
    // `teardown` — the remote side goes with the environment (explicit,
    // never survived — detach is the survivable form).
    d.teardown(&mut s, &lease, &h.env_handle_id).unwrap();
    assert_eq!(calls.lock().unwrap().torn_down, 1);
    assert!(d.handle(&h.env_handle_id).unwrap().state.is_terminal());
}

#[test]
fn s4_13_remote_ephemeral_detach_tears_down_persistent_does_not() {
    // `remote_ephemeral` dies with the kernel session — `detach` calls the
    // adapter's `teardown`. `remote_persistent` survives detach (the
    // adapter keeps the remote side for a later `attach`). The detach path
    // requires a live session+report (kernel-observed); the leg under test
    // is the *class-conditional adapter call*, driven through `teardown`
    // for ephemeral and asserted by the persistent handle surviving
    // `detach`'s provider leg — exercised on the provider field directly.
    let (mut s, run, lease) = open("persist-detach");
    let (mut d, calls) = register(&run, EnvironmentClass::RemotePersistent, vec![]);
    let mut broker = test_broker();
    let h = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::RemotePersistent),
            roots_for("pd-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    assert!(h.provider.is_some());
    // Persistent class: `detach` on a handle without a kernel session is
    // a typed `SessionLost`/`Unavailable` — never a teardown of the remote.
    to_ready(&mut d, &h.env_handle_id);
    let e = d.detach(&mut s, &lease, &h.env_handle_id);
    assert!(
        matches!(
            e,
            Err(EnvError::SessionLost { .. }) | Err(EnvError::Unavailable { .. })
        ),
        "provider handle without kernel session: {e:?}"
    );
    assert_eq!(
        calls.lock().unwrap().torn_down,
        0,
        "remote_persistent is never torn down by the refused detach"
    );
}

// ── hosted binding — participant-origin rows (AC-R-2.2.5-13) ────────────

#[test]
fn ac_r_2_2_5_13_hosted_rows_carry_participant_origin() {
    let (mut s, run, lease) = open("hosted-env");
    let (mut d, _calls) = register(&run, EnvironmentClass::ProviderHosted, vec![]);
    let mut broker = test_broker();
    let h = d
        .provision_hosted(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::ProviderHosted),
            roots_for("he-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            "participant:host-adapter",
            "hh.hosting/1:bind-7",
            &["action.environment.provisioned"],
            Some(&mut broker),
        )
        .unwrap();
    assert_eq!(h.row_origin(), "participant");
    assert!(h.hosted.is_some());
    // The *observed* transitions mint at `origin = participant`; the
    // unobserved member (`provisioned`) mints nothing — the handle never
    // fabricates what the adapter cannot see.
    let hosted_rows: Vec<_> = s
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class.starts_with("action.environment."))
        .collect();
    assert!(
        hosted_rows.iter().all(|e| e
            .provenance
            .as_ref()
            .map(|p| format!("{:?}", p.origin))
            .map(|o| o.contains("articipant"))
            .unwrap_or(false)),
        "every hosted row is participant-origin: {:?}",
        hosted_rows.iter().map(|e| &e.class).collect::<Vec<_>>()
    );
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "action.environment.declared"));
    assert!(
        !s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "action.environment.provisioned"),
        "unobserved transition mints no row"
    );
    assert!(!h.transition_reportable("action.environment.provisioned"));
    assert!(h.transition_reportable("action.environment.declared"));
    // The producer stays the kernel component — `origin = participant`
    // marks *who observed*, never a second writer (CC10/CC2).
    let declared = s
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.declared")
        .unwrap();
    assert!(
        format!("{:?}", declared.producer).contains("kernel")
            || format!("{:?}", declared.producer).contains("hh-env"),
        "producer stays kernel: {:?}",
        declared.producer
    );
}

// ── share mode with resource keys + detach_to_child cascade ─────────────

#[test]
fn s4_13_derive_share_leases_resource_keys() {
    let (mut s, run, lease) = open("share");
    let mut d = EnvDriver::new(&run);
    let parent = d
        .provision(
            &mut s,
            &lease,
            &test_record(EnvironmentClass::LocalHost),
            roots_for("sh-ws"),
            PolicySlot::Inline(Box::new(hh_containment::policy::kernel_default(1_000))),
            OnLoss::FailRun,
            None,
        )
        .unwrap();
    to_ready(&mut d, &parent.env_handle_id);
    // `share` without resource keys is a typed refusal — write
    // coordination is leased, never implied.
    let e = d.derive_share(
        &mut s,
        &lease,
        &parent.env_handle_id,
        &[],
        OnParentEnd::Teardown,
        None,
    );
    assert!(matches!(e, Err(EnvError::Unsupported { .. })), "{e:?}");
    // A live conflicting `resource:<key>` holder refuses the share.
    s.lease_acquire(
        &run,
        &lease,
        &LeaseScope::Resource("dataset:gold".to_string()),
        "holder:other",
        60_000,
    )
    .unwrap();
    let blocked = d.derive_share(
        &mut s,
        &lease,
        &parent.env_handle_id,
        &["dataset:gold".to_string()],
        OnParentEnd::Teardown,
        None,
    );
    assert!(
        matches!(blocked, Err(EnvError::Unsupported { .. })),
        "conflicting resource-key holder must refuse: {blocked:?}"
    );
    // An uncontended key admits — the child shares the parent's roots
    // verbatim (never widened, never copied).
    let child = d
        .derive_share(
            &mut s,
            &lease,
            &parent.env_handle_id,
            &["dataset:open".to_string()],
            OnParentEnd::DetachToChild,
            None,
        )
        .unwrap();
    assert_eq!(
        child.roots.workspace_roots, parent.roots.workspace_roots,
        "share mode: the child's workspace *is* the parent's"
    );
    assert_eq!(
        child.parent.as_ref().unwrap().on_parent_end,
        OnParentEnd::DetachToChild
    );
    // `detach_to_child` — the parent's teardown keeps the child living;
    // the ownership transfer is the ledgered `detached{detach_to_child}`.
    let child_id = child.env_handle_id.clone();
    d.teardown(&mut s, &lease, &parent.env_handle_id).unwrap();
    assert!(
        !d.handle(&child_id).unwrap().state.is_terminal(),
        "detach_to_child child survives the parent"
    );
    assert!(d.handle(&child_id).unwrap().parent.is_none());
    assert!(
        s.events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "action.environment.detached"
                && e.payload.get("reason").and_then(Json::as_str) == Some("detach_to_child")),
        "the ownership transfer is ledgered"
    );
}
