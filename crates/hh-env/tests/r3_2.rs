//! R3.2 — the `heal(env_handle)` surface (R-2.5.5; DF-S2.3-1; BL-44):
//!
//! `EnvDriver::heal_handle` is the env-handle-level heal procedure — the
//! `reattach → resume → replace per on_loss` ladder (ADR-0136 §6) the
//! §5a.3 `heal(run, lease, handle, policy)` op composes over — callable
//! directly on a real `EnvHandle` without a session binding. Every rung
//! mints its `action.environment.*` row under the run's writer lease
//! through the real path (`heal`/`resume`/`replace` — CC1,
//! durable-before-visible); a handle outside the ladder's domain is the
//! typed `InvalidState` refusal, never a silent no-op.
//!
//! Covered here: `suspended → resumed` (provider class), `unreachable →
//! replaced` (`on_loss = replace_from_image`), `unreachable → failed`
//! (`on_loss = fail_run` — the binding's declared posture mints
//! `action.environment.failed`, and the policy surface surfaces it as the
//! spec's `refused{reason}` arm), the `ready`/`provisioning` typed
//! refusal, and the `heal_with_policy` composition staying honest (the
//! successor hand-off + `restored` members unchanged).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ResourceLimits, WritableRoot};
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::handle::{HandleState, OnLoss, Roots};
use hharness_env::provider::{ProviderAdapter, ProviderHandle, ProviderMeter, ProvisionRequest};
use hharness_env::record::{
    CanaryChannel, EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe,
};
use hharness_env::recovery::{HandleHealOutcome, HealOutcome, HealingPolicy, OnLost};
use hharness_identity::idp::address;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r32-{}-{tag}-{n}", std::process::id()));
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

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn roots_for(ws: &std::path::Path) -> Roots {
    Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    }
}

fn test_policy(ws: &std::path::Path) -> hharness_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = hharness_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = hharness_containment::policy::NetMode::None;
    p.compute_ids();
    p
}

fn test_record(class: EnvironmentClass) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image: ImageRef::ContentAddress(address(b"r32-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

/// `provision + attach(Ep2Model, fail-closed)` → a `ready` `local_host`
/// handle under the given `on_loss` posture.
fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
    on_loss: OnLoss,
) -> String {
    let record = test_record(EnvironmentClass::LocalHost);
    let h = driver
        .provision(
            store,
            lease,
            &record,
            roots_for(ws),
            PolicySlot::Inline(Box::new(test_policy(ws))),
            on_loss,
            None,
        )
        .unwrap();
    let backend = Ep2Model::reference();
    driver
        .attach(
            store,
            lease,
            &h.env_handle_id,
            Some(&backend),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    h.env_handle_id
}

fn seq_of(store: &Store, run: &str, class: &str) -> Option<u64> {
    store
        .events(run)
        .unwrap()
        .iter()
        .find(|e| e.class == class)
        .map(|e| e.seq)
}

// ── the provider fixture (mirrors tests/s4_13.rs) ───────────────────────────

#[derive(Default)]
struct Calls {
    provisioned: usize,
    suspended: usize,
    resumed: usize,
    torn_down: usize,
}

struct MockProvider {
    calls: Arc<Mutex<Calls>>,
}

impl ProviderAdapter for MockProvider {
    fn class(&self) -> EnvironmentClass {
        EnvironmentClass::RemoteEphemeral
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
        Ok(vec![])
    }
}

fn test_canary(channel_id: &str) -> CanaryChannel {
    CanaryChannel {
        channel_id: channel_id.to_string(),
        spec: hharness_secrets::SecretChannelSpec {
            kind: hharness_secrets::CredentialKind::ApiKey,
            source: hharness_secrets::SecretSource::OperatorVault {
                vault_ref: format!("canary:{channel_id}"),
            },
            destinations: vec![],
            delivery_modes: std::collections::BTreeSet::new(),
            max_lifetime_ms: None,
            rotation_policy: None,
            allowed_env_names: None,
            sender_constraint: hharness_secrets::SenderConstraint::None,
            constraints: Default::default(),
            bindable: false,
            access_class: hharness_secrets::AccessClass::KernelOnly,
            canary: true,
            description: "image-manifest tripwire".into(),
        },
    }
}

fn test_broker() -> hharness_secrets::CredentialBroker {
    hharness_secrets::CredentialBroker::new(
        Box::new(hharness_secrets::StaticVault::default()),
        "canary-test-key",
    )
}

// ── R-2.5.5 — the env-handle-level heal ladder ──────────────────────────────

#[test]
fn r3_2_heal_handle_replaces_unreachable_per_on_loss() {
    let (mut store, run, lease) = open("heal-handle-replace");
    let ws = workspace("heal-handle-replace");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        OnLoss::ReplaceFromImage,
    );
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();

    // `heal(env_handle)` — the dead-channel arm: `on_loss =
    // replace_from_image` mints a successor; the corpse lands `replaced`.
    let out = driver.heal_handle(&mut store, &lease, &env).unwrap();
    let HandleHealOutcome::Replaced { to } = out else {
        panic!("expected Replaced, got {out:?}")
    };
    assert_ne!(to, env);
    assert_eq!(
        driver.handle_state(&env),
        Some(HandleState::Replaced),
        "the corpse is terminal"
    );
    assert!(driver.handle_state(&to).is_some(), "the successor exists");
    // Durable-before-visible: `unreachable` precedes `replaced`.
    let un = seq_of(&store, &run, "action.environment.unreachable").expect("unreachable");
    let rp = seq_of(&store, &run, "action.environment.replaced").expect("replaced");
    assert!(un < rp);
}

#[test]
fn r3_2_heal_handle_fail_run_lands_failed() {
    let (mut store, run, lease) = open("heal-handle-fail");
    let ws = workspace("heal-handle-fail");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws, OnLoss::FailRun);
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();

    // `on_loss = fail_run` — the binding's declared posture: the handle
    // lands `failed` with its `action.environment.failed` row (the caller
    // composes `stop{infrastructure_failure{environment_lost}}` — never a
    // silent corpse).
    let out = driver.heal_handle(&mut store, &lease, &env).unwrap();
    assert_eq!(out, HandleHealOutcome::Failed);
    assert_eq!(driver.handle_state(&env), Some(HandleState::Failed));
    assert!(
        seq_of(&store, &run, "action.environment.failed").is_some(),
        "the failed row is durable"
    );
    // … and the policy surface reads the same ladder — `refused{reason}`
    // is the spec's outcome arm for a `fail_run` binding, with the
    // `failed` row already durable.
    let ws2 = workspace("heal-handle-fail2");
    let env2 = ready_env(&mut store, &lease, &mut driver, &ws2, OnLoss::FailRun);
    driver.mark_unreachable(&mut store, &lease, &env2).unwrap();
    let policy = HealingPolicy {
        on_lost: OnLost::Reprovision,
        max_heals: 4,
        verify_after: None,
        preserve_detached: false,
    };
    match driver
        .heal_with_policy(&mut store, &lease, &env2, &policy, "worker_lost")
        .unwrap()
    {
        HealOutcome::Refused { reason } => assert!(reason.contains("fail_run"), "{reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(driver.handle_state(&env2), Some(HandleState::Failed));
}

#[test]
fn r3_2_heal_handle_resumes_a_suspended_provider() {
    let (mut store, run, lease) = open("heal-handle-resume");
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut driver = EnvDriver::new(&run);
    driver.register_provider(Box::new(MockProvider {
        calls: calls.clone(),
    }));
    let ws = workspace("heal-handle-resume");
    let mut record = test_record(EnvironmentClass::RemoteEphemeral);
    record.canary_channels = vec![test_canary("img.canary")];
    let mut broker = test_broker();
    let h = driver
        .provision(
            &mut store,
            &lease,
            &record,
            roots_for(&ws),
            PolicySlot::Inline(Box::new(kernel_default(1_000))),
            OnLoss::FailRun,
            Some(&mut broker),
        )
        .unwrap();
    // Walk the provider handle to `ready` through the legal edges.
    {
        let h = driver.handle_mut(&h.env_handle_id).unwrap();
        if h.state == HandleState::Declared {
            h.transition(HandleState::Provisioning, 1_000).unwrap();
        }
        if h.state == HandleState::Provisioning {
            h.transition(HandleState::Ready, 1_001).unwrap();
        }
    }
    driver
        .suspend(&mut store, &lease, &h.env_handle_id, Some("idle"))
        .unwrap();
    assert_eq!(
        driver.handle_state(&h.env_handle_id),
        Some(HandleState::Suspended)
    );

    // `heal(env_handle)` — a parked handle is not lost: the resume rung
    // returns it `ready` (`action.environment.resumed`), never a
    // successor.
    let out = driver
        .heal_handle(&mut store, &lease, &h.env_handle_id)
        .unwrap();
    assert_eq!(out, HandleHealOutcome::Resumed);
    assert_eq!(calls.lock().unwrap().resumed, 1, "the adapter resumed");
    assert_eq!(
        driver.handle_state(&h.env_handle_id),
        Some(HandleState::Ready)
    );
    assert!(
        seq_of(&store, &run, "action.environment.resumed").is_some(),
        "the resumed row is durable"
    );
}

#[test]
fn r3_2_heal_handle_refuses_states_outside_the_ladder() {
    let (mut store, run, lease) = open("heal-handle-refuse");
    let ws = workspace("heal-handle-refuse");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws, OnLoss::FailRun);
    // `ready` — an attached environment is not a heal subject (the
    // verdict path's `environment_attached` refusal at the boundary).
    let err = driver.heal_handle(&mut store, &lease, &env).unwrap_err();
    assert!(
        matches!(err, EnvError::InvalidState { .. }),
        "ready must refuse typed: {err:?}"
    );
    // A missing handle is `Unavailable`, not a fabricated corpse.
    let err2 = driver
        .heal_handle(&mut store, &lease, "env-no-such")
        .unwrap_err();
    assert!(matches!(err2, EnvError::Unavailable { .. }), "{err2:?}");
    // No rows minted on a refused call (durable-before-visible: nothing
    // visible that isn't durable — and nothing landed).
    let before = store.events(&run).unwrap().len();
    let _ = driver.heal_handle(&mut store, &lease, &env);
    assert_eq!(store.events(&run).unwrap().len(), before);
}
