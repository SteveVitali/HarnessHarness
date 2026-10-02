//! The packaged `work_source_adapter` reference variant end-to-end
//! (§5i.1, R-2.12.6; S5.6; ADR-0180/0181): `plugins/hh-tracker-fixture`
//! is sealed at pack time, `load` pin-verifies it, the host spawns it
//! `subprocess_confined` through the helper, binds
//! `work_source_adapter` `1.0`, and `PluginSourceAdapter` drives the
//! `WorkSourceAdapter` seam over the live session — the out-of-process
//! leg of T-LCD-12 (records-in/records-out). The real tracker variant
//! plugs the same ops (HUMAN-H2, deferred — the fixture is the proxy,
//! never a simulation of it).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hh_embed_schema::plugin_abi::BindParams;
use hh_fleet::source::WorkSourceAdapter;
use hh_fleet_adapter::adapter::{PluginSourceAdapter, SourceChannel, SourceFault};
use hh_fleet_adapter::{WORK_SOURCE_CLASS, WORK_SOURCE_CONTRACT};
use hh_registry::extension::plugin::{manifest_from_json, manifest_to_json};
use hh_varhost::{
    load_package, seal_package, spawn, unstamp_json, variant_class, InvokeOutcome, RecordingPorts,
    SpawnSpec, VariantHost, VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn tracker_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-tracker-fixture");
    p.exists().then_some(p)
}

/// Materialize the package: manifest + `bin/` copy of the built variant
/// binary, `seal` (real code_pointer pins), write, `load` (verify).
fn materialize() -> Option<(tempfile::TempDir, hh_varhost::VariantPackage)> {
    let bin = tracker_bin()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::copy(&bin, root.join("bin/hh-tracker-fixture")).unwrap();
    let text = std::fs::read_to_string(
        repo_root().join("plugins/hh-tracker-fixture/plugin.manifest.json"),
    )
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
        session_id: "tf-pkg-1".into(),
        backend: "direct".into(),
        placement: hh_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir,
        exec_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:hh/hh-tracker-fixture", "pin:sealed"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            format!("class_contract:{WORK_SOURCE_CLASS}"),
            WORK_SOURCE_CONTRACT.to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

/// The `plugin_abi/1` session adapter — a `SourceChannel` over a bound
/// `VariantHost` session. V1 host stamp off before the codec sees the
/// outputs.
struct SessionChannel<'a> {
    host: &'a mut VariantHost<RecordingPorts, VecEvents>,
    session: &'a mut hh_varhost::VariantSession,
    binding: String,
    seq: u64,
}

impl<'a> SourceChannel for SessionChannel<'a> {
    fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
        self.seq += 1;
        let out = self
            .host
            .invoke(
                self.session,
                &self.binding,
                operation,
                inputs,
                &format!("tf-r{}", self.seq),
                TIMEOUT,
            )
            .map_err(|e| SourceFault::Transport(format!("{e:?}")))?;
        match out {
            InvokeOutcome::Outputs(docs) => Ok(docs.iter().map(unstamp_json).collect()),
            InvokeOutcome::Failed(e) => Err(SourceFault::Transport(format!("abi failure: {e:?}"))),
        }
    }
}

fn fixture_doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("occ-9")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("ticket.updated")),
                (
                    "item",
                    Json::obj([
                        ("item_id", Json::str("i9")),
                        ("title", Json::str("ticket i9")),
                        (
                            "source",
                            Json::obj([
                                ("source_id", Json::str("tickets")),
                                ("kind", Json::str("ticket")),
                                ("state", Json::str("open")),
                            ]),
                        ),
                        ("idempotency_key", Json::str("idem:i9")),
                        ("owner", Json::str("alice")),
                    ]),
                ),
                ("observed_at_ms", Json::Int(9_900)),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
        ("activate_run", Json::Arr(vec![Json::str("i9")])),
        (
            "records",
            Json::Arr(vec![Json::obj([
                ("native_id", Json::str("T-9")),
                ("state", Json::str("open")),
            ])]),
        ),
    ])
}

#[test]
fn packaged_manifest_decodes_and_declares() {
    let text = std::fs::read_to_string(
        repo_root().join("plugins/hh-tracker-fixture/plugin.manifest.json"),
    )
    .expect("packaged manifest readable");
    let j = hh_wire::json::parse(&text).expect("manifest json");
    let m = manifest_from_json(&j, "plugin.manifest.json").expect("manifest decodes");
    assert_eq!(m.identity.id(), "hh/hh-tracker-fixture");
    let c = m
        .contributions
        .iter()
        .find(|c| c.declaration.get("class_id").and_then(Json::as_str) == Some(WORK_SOURCE_CLASS));
    assert!(c.is_some(), "no work_source_adapter contribution");
    let exe = c.unwrap().executable.as_ref().expect("executable");
    assert_eq!(
        exe.isolation,
        hh_registry::extension::IsolationClass::SubprocessConfined
    );
}

#[test]
fn tracker_plugin_runs_the_contract_ops_end_to_end() {
    let Some((_t, pkg)) = materialize() else {
        eprintln!("hh-tracker-fixture binary not built — skipped");
        return;
    };
    if hh_env::helper::helper_binary().is_none() {
        eprintln!("hh-helper binary not found — skipped");
        return;
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some(WORK_SOURCE_CLASS));
    let dir = std::env::temp_dir().join(format!(
        "vh-tf-{}-{}",
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
                slot: "work_source".into(),
                class_id: WORK_SOURCE_CLASS.into(),
                contract_version: WORK_SOURCE_CONTRACT.into(),
                variant: Json::Null,
                params: Json::obj([("fixture_doc", fixture_doc())]),
                profile: Json::Null,
                account: Json::Null,
                budget: Json::Null,
                placement: "subprocess_confined".into(),
            },
        )
        .expect("bind");

    // `PluginSourceAdapter` over the live subprocess session — the
    // `WorkSourceAdapter` seam driven out of process (AC-9's L5 clause).
    let mut ad = PluginSourceAdapter::new(
        SessionChannel {
            host: &mut h,
            session: &mut s,
            binding: b.clone(),
            seq: 0,
        },
        "src:tickets",
    );
    // The capability probe is one out-of-process `capabilities` invoke.
    ad.bind_probe();
    let caps = ad.capabilities();
    assert_eq!(
        caps.poll,
        hh_fleet::capabilities::CapState::Declared,
        "the fixture tracker declares poll"
    );
    let occs = ad.occurrences(None);
    assert_eq!(occs.len(), 1);
    assert_eq!(occs[0].occurrence_id, "occ-9");
    assert_eq!(occs[0].item.item_id, "i9");
    assert_eq!(
        ad.activate_run(&["i9".to_string(), "other".to_string()]),
        vec!["i9".to_string()]
    );
    assert_eq!(ad.list(&[]).len(), 1);
    assert_eq!(ad.get(&["T-9".to_string()]).len(), 1);
    assert!(ad.fault().is_none(), "latched fault: {:?}", ad.fault());

    drop(ad);
    h.close(&mut s).unwrap();
    assert!(s.detached);
}
