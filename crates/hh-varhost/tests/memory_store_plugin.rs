//! The packaged `memory_store` variant end-to-end (§5c.3, R-2.4.3¹;
//! ADR-0078 d2; S4.16b): `plugins/hh-memory-store` is sealed at pack time,
//! `load` pin-verifies it, the host spawns it `subprocess_confined` through
//! the helper, binds `memory_store` `1.0`, and the shared
//! `hh_context::memory_abi::conformance` battery runs against the
//! `RemoteStore` binding — the same suite the in-process `MemoryStore` and
//! the dispatch loopback pass (T-LCD-12). Typed domain refusals cross the
//! process boundary as `MemoryError`s, never faults (the refusal is data).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hh_context::memory::MemoryStorePort;
use hh_context::memory_abi::{
    ChannelFault, RemoteStore, StoreChannel, MEMORY_STORE_CLASS, MEMORY_STORE_CONTRACT,
};
use hh_embed_schema::plugin_abi::BindParams;
use hh_registry::extension::plugin::{manifest_from_json, manifest_to_json};
use hh_varhost::{
    load_package, seal_package, spawn, unstamp_json, variant_class, HostError, InvokeOutcome,
    RecordingPorts, SpawnSpec, VariantHost, VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

fn repo_root() -> PathBuf {
    // tests run with cwd = crate dir → ../../..
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn memory_store_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-memory-store");
    p.exists().then_some(p)
}

/// Materialize the package: manifest + `bin/` copy of the built variant
/// binary, `seal` (real code_pointer pins), write, `load` (verify).
fn materialize() -> Option<(tempfile::TempDir, hh_varhost::VariantPackage)> {
    let bin = memory_store_bin()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::copy(&bin, root.join("bin/hh-memory-store")).unwrap();
    let text =
        std::fs::read_to_string(repo_root().join("plugins/hh-memory-store/plugin.manifest.json"))
            .unwrap();
    let j = hh_wire::json::parse(&text).unwrap();
    let m = manifest_from_json(&j, "plugin.manifest.json").unwrap();
    let sealed = seal_package(root, &m).expect("seal");
    let sealed_text = manifest_to_json(&sealed).to_canonical_string();
    std::fs::write(root.join("plugin.manifest.json"), &sealed_text).unwrap();
    let pkg = load_package(root).expect("load verifies pins");
    Some((dir, pkg))
}

fn spec(pkg: hh_varhost::VariantPackage, socket_dir: PathBuf) -> SpawnSpec {
    let exec_args = vec![
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
        "--plugin-id".into(),
        pkg.manifest.identity.id(),
    ];
    let cap = pkg.manifest.requests.clone();
    SpawnSpec {
        package: pkg,
        session_id: "ms-pkg-1".into(),
        backend: "direct".into(),
        placement: hh_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir,
        exec_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:hh/hh-memory-store", "pin:sealed"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            format!("class_contract:{MEMORY_STORE_CLASS}"),
            MEMORY_STORE_CONTRACT.to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

/// The `plugin_abi/1` session adapter — a `StoreChannel` over a bound
/// `VariantHost` session. Un-stamps the V1 host provenance before the
/// strict codec sees the outputs (the stamp is host-side provenance, not a
/// contract member).
struct SessionChannel<'a> {
    host: &'a mut VariantHost<RecordingPorts, VecEvents>,
    session: &'a mut hh_varhost::VariantSession,
    binding: String,
    seq: u64,
}

impl<'a> StoreChannel for SessionChannel<'a> {
    fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, ChannelFault> {
        self.seq += 1;
        let out = self
            .host
            .invoke(
                self.session,
                &self.binding,
                operation,
                inputs,
                &format!("ms-r{}", self.seq),
                TIMEOUT,
            )
            .map_err(|e| ChannelFault::Transport(format!("{e:?}")))?;
        match out {
            // V1 stamp off (`unstamp_json`), canonical envelope off
            // (`unwrap_output`) — the contract document inside is what
            // `RemoteStore` decodes.
            InvokeOutcome::Outputs(docs) => docs
                .iter()
                .map(|d| hh_context::memory_abi::unwrap_output(&unstamp_json(d)))
                .collect(),
            InvokeOutcome::Failed(e) => Err(ChannelFault::Transport(format!(
                "plugin abi failure: {e:?}"
            ))),
        }
    }
}

#[test]
fn packaged_manifest_decodes_and_declares() {
    let text =
        std::fs::read_to_string(repo_root().join("plugins/hh-memory-store/plugin.manifest.json"))
            .expect("packaged manifest readable");
    let j = hh_wire::json::parse(&text).expect("manifest json");
    let m = manifest_from_json(&j, "plugin.manifest.json").expect("manifest decodes");
    assert_eq!(m.identity.id(), "hh/hh-memory-store");
    let c = m
        .contributions
        .iter()
        .find(|c| c.declaration.get("class_id").and_then(Json::as_str) == Some(MEMORY_STORE_CLASS));
    assert!(c.is_some(), "no memory_store contribution");
    let exe = c.unwrap().executable.as_ref().expect("executable");
    assert_eq!(
        exe.isolation,
        hh_registry::extension::IsolationClass::SubprocessConfined
    );
}

#[test]
fn memory_store_plugin_runs_the_contract_battery_end_to_end() {
    let Some((_t, pkg)) = materialize() else {
        eprintln!("hh-memory-store binary not built — skipped");
        return;
    };
    if hh_env::helper::helper_binary().is_none() {
        eprintln!("hh-helper binary not found — skipped");
        return;
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some(MEMORY_STORE_CLASS));
    let dir = std::env::temp_dir().join(format!(
        "vh-ms-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (mut s, _lowered) = spawn(&spec(pkg, dir)).expect("spawn");
    let mut h: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = h
        .bind(
            &mut s,
            BindParams {
                slot: "memory".into(),
                class_id: MEMORY_STORE_CLASS.into(),
                contract_version: MEMORY_STORE_CONTRACT.into(),
                variant: Json::Null,
                params: Json::Null,
                profile: Json::Null,
                account: Json::Null,
                budget: Json::Null,
                placement: "subprocess_confined".into(),
            },
        )
        .expect("bind");

    // The same battery the in-process store and the loopback pass — now
    // over a live subprocess session through `RemoteStore`.
    let mut remote = RemoteStore::new(SessionChannel {
        host: &mut h,
        session: &mut s,
        binding: b.clone(),
        seq: 0,
    });
    hh_context::memory_abi::conformance::run(&mut remote);
    assert!(
        remote.fault().is_none(),
        "latched fault: {:?}",
        remote.fault()
    );

    // Typed refusals cross the boundary: an unknown name is `UnknownName`,
    // not a transport fault.
    let err = remote
        .resolve(
            hh_provenance::authority::PersistenceScope::Run,
            Some("facts/never-bound"),
            None,
            hh_identity::names::ResolveMode::Execute,
        )
        .unwrap_err();
    assert!(
        matches!(err, hh_context::memory::MemoryError::UnknownName { .. }),
        "{err:?}"
    );

    drop(remote);
    h.close(&mut s).unwrap();
    assert!(s.detached);
}

#[test]
fn memory_store_tampered_package_is_pin_mismatch() {
    let Some((dir, _pkg)) = materialize() else {
        return;
    };
    let bin = dir.path().join("bin/hh-memory-store");
    let mut bytes = std::fs::read(&bin).unwrap();
    bytes[0] ^= 0xff;
    std::fs::write(&bin, bytes).unwrap();
    let r = load_package(dir.path());
    assert!(
        matches!(
            r,
            Err(HostError::Abi(
                hh_embed_schema::plugin_abi::AbiError::PinMismatch
            ))
        ),
        "{r:?}"
    );
}
