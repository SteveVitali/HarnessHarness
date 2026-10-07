//! S4.14b coverage — `set_phase` re-derives the effective policy through
//! `meet::effective()` over the sealed `policy_rule` phase layer
//! (R-2.8.4¹; CF-318; ADR-0307): the `net_mode` shorthand mints a layer at
//! the declared `authority`, a full `layer` document meets over the live
//! policy, a non-`policy_rule` `basis` refuses `basis_not_allowed`, and an
//! unentitled loosening refuses `ContainmentWidening` — never a silent swap.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{kernel_default, NetMode, WritableRoot};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::handle::{OnLoss, Roots, Tri};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_identity::idp::address;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-s414b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn test_record() -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalHost,
        image: ImageRef::ContentAddress(address(b"img", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: hh_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
    }
}

/// A base policy carrying a sealed `phase_schedule`.
fn scheduled_policy(
    ws: &std::path::Path,
    base_mode: NetMode,
    schedule: Json,
) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    p.provenance.authority = AuthorityClass::Kernel;
    p.net.mode = base_mode;
    p.fs.write.allow.push(WritableRoot {
        root: ws.display().to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.ext.insert("phase_schedule".to_string(), schedule);
    p.compute_ids();
    p
}

/// Provision + attach a `local_host` handle declared
/// `per_phase_network_policy = supported`.
fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
    policy: hh_containment::policy::ContainmentPolicy,
) -> String {
    let record = test_record();
    let roots = Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    };
    let h = driver
        .provision(
            store,
            lease,
            &record,
            roots,
            PolicySlot::Inline(Box::new(policy)),
            OnLoss::FailRun,
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
    // The test fixture declares the capability (a benchmark-family handle
    // would declare it in its class's EnvCapabilityDeclaration).
    driver
        .handle_mut(&h.env_handle_id)
        .unwrap()
        .capabilities
        .per_phase_network_policy = Tri::Supported;
    h.env_handle_id
}

fn phase_net(_store: &Store, _run: &str, driver: &EnvDriver, env: &str) -> NetMode {
    let h = driver.handle(env).unwrap();
    match &h.containment {
        PolicySlot::Inline(p) => p.net.mode,
        PolicySlot::ResolvedRef { policy, .. } => policy.net.mode,
    }
}

#[test]
fn set_phase_applies_the_sealed_net_mode_layer_through_the_meet() {
    let (mut store, run, lease) = open("phase");
    let ws = dir("phase-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let schedule = Json::obj([("agent", Json::obj([("net_mode", Json::str("none"))]))]);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        scheduled_policy(&ws, NetMode::Mediated, schedule),
    );
    assert_eq!(phase_net(&store, &run, &driver, &env), NetMode::Mediated);
    driver.set_phase(&mut store, &lease, &env, "agent").unwrap();
    assert_eq!(phase_net(&store, &run, &driver, &env), NetMode::None);
    assert_eq!(driver.handle(&env).unwrap().phase.as_deref(), Some("agent"));
}

#[test]
fn set_phase_full_layer_document_meets_over_the_live_policy() {
    let (mut store, run, lease) = open("phase-layer");
    let ws = dir("phase-layer-ws");
    std::fs::create_dir_all(&ws).unwrap();
    // A `definition`-authority layer narrowing `net.mode` to `none`.
    let mut layer = kernel_default(0);
    layer.provenance.authority = AuthorityClass::Definition;
    layer.net.mode = NetMode::None;
    layer.compute_ids();
    let schedule = Json::obj([(
        "verify",
        Json::obj([
            ("basis", Json::str("policy_rule")),
            ("layer", layer.to_json()),
        ]),
    )]);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        scheduled_policy(&ws, NetMode::Mediated, schedule),
    );
    driver
        .set_phase(&mut store, &lease, &env, "verify")
        .unwrap();
    assert_eq!(phase_net(&store, &run, &driver, &env), NetMode::None);
}

#[test]
fn set_phase_refuses_a_non_policy_rule_basis() {
    let (mut store, run, lease) = open("phase-basis");
    let ws = dir("phase-basis-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let schedule = Json::obj([(
        "agent",
        Json::obj([
            ("basis", Json::str("approval")),
            ("net_mode", Json::str("none")),
        ]),
    )]);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        scheduled_policy(&ws, NetMode::Mediated, schedule),
    );
    match driver.set_phase(&mut store, &lease, &env, "agent") {
        Err(EnvError::Unsupported { detail, .. }) => {
            assert!(detail.contains("basis_not_allowed"), "{detail}")
        }
        other => panic!("expected basis_not_allowed refusal, got {other:?}"),
    }
    // The live policy is untouched — no silent swap.
    assert_eq!(phase_net(&store, &run, &driver, &env), NetMode::Mediated);
}

#[test]
fn set_phase_unentitled_loosening_is_containment_widening() {
    let (mut store, run, lease) = open("phase-widen");
    let ws = dir("phase-widen-ws");
    std::fs::create_dir_all(&ws).unwrap();
    // A `definition`-authority layer may not loosen a `kernel`-entitled
    // field — `exec` → `any` is `ContainmentWidening` (net.mode loosening
    // is `principal`-entitled, which `definition` outranks).
    let mut layer = kernel_default(0);
    layer.provenance.authority = AuthorityClass::Definition;
    layer.fs.exec = hh_containment::policy::ExecPolicy::Any;
    layer.compute_ids();
    let schedule = Json::obj([("agent", Json::obj([("layer", layer.to_json())]))]);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        scheduled_policy(&ws, NetMode::None, schedule),
    );
    match driver.set_phase(&mut store, &lease, &env, "agent") {
        Err(EnvError::Unsupported { detail, .. }) => {
            assert!(detail.contains("ContainmentWidening"), "{detail}")
        }
        other => panic!("expected ContainmentWidening, got {other:?}"),
    }
}

#[test]
fn set_phase_undeclared_phase_refuses() {
    let (mut store, run, lease) = open("phase-none");
    let ws = dir("phase-none-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let schedule = Json::obj([("agent", Json::obj([("net_mode", Json::str("none"))]))]);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        scheduled_policy(&ws, NetMode::Mediated, schedule),
    );
    assert!(driver
        .set_phase(&mut store, &lease, &env, "verify")
        .is_err());
}
