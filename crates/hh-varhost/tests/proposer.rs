//! The `evolution_proposer` out-of-process lane (S6.2, §05h §2.4) —
//! the `hh-plugin-fixture --mode proposer` stub spawned for real
//! through `hh-helper` (`direct` plumbing lane — no sandbox claims),
//! driven through `bind`/`invoke` for the class's three ops:
//! `declare` (the canned instrument-grade declaration), `propose` (the
//! typed `no_addressable_failure` outcome — never `[]`), and
//! `select_parent` (a pure pick over the handed lineage).
//!
//! This is the fixture leg of the class's `executable` conformance
//! kind: the port the real `run_proposer_suite` drives out-of-process
//! is this same `invoke` channel (the suite's `InProcessPort` leg runs
//! in `hh-evolution`; the fixture leg proves the ABI surface a
//! `plugin_abi/1` proposer variant rides).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hh_embed_schema::plugin_abi::BindParams;
use hh_registry::extension::plugin::{PluginIdentity, PluginManifest, Requests, Requires};
use hh_varhost::{
    spawn, InvokeOutcome, RecordingPorts, SpawnSpec, VariantHost, VariantPackage, VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

/// `target/{profile}/hh-plugin-fixture` beside the test binary.
fn fixture_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-plugin-fixture");
    p.exists().then_some(p)
}

fn helper_present() -> bool {
    hh_env::helper::helper_binary().is_some()
}

fn manifest() -> PluginManifest {
    PluginManifest {
        identity: PluginIdentity {
            namespace: "local".into(),
            name: "fixture".into(),
            version_label: None,
        },
        tier: "C0".into(),
        depends_on: vec![],
        requires: Requires {
            hir_dialect: "1.0".into(),
            registry_dialect: "1.0".into(),
            plugin_abi: "1.0".into(),
            contracts: vec![],
            records: vec![],
        },
        contributions: vec![],
        requests: Requests::default(),
        claims: Json::obj([]),
        conformance_claims: vec![],
        parameters: None,
        summary: Json::Null,
        ext: BTreeMap::new(),
    }
}

fn package(bin: &Path, manifest: PluginManifest) -> VariantPackage {
    VariantPackage {
        root: bin.parent().unwrap().to_path_buf(),
        manifest,
        content: "content:fixture".into(),
        version_id: "pin:fixture".into(),
        execs: vec![bin.to_path_buf()],
    }
}

fn spec(pkg: VariantPackage, exec_args: Vec<String>) -> SpawnSpec {
    let mut pinned_args = vec![
        "--plugin-id".into(),
        "local/fixture".into(),
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
    ];
    pinned_args.extend(exec_args);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vh-proposer-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let cap = pkg.manifest.requests.clone();
    SpawnSpec {
        package: pkg,
        session_id: format!("proposer-{}", std::process::id()),
        backend: "direct".into(),
        placement: hh_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir: dir,
        exec_args: pinned_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture", "pin:fixture"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:evolution_proposer".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

fn bind_params() -> BindParams {
    BindParams {
        slot: "slot-1".into(),
        class_id: "evolution_proposer".into(),
        contract_version: "1.0".into(),
        variant: Json::Null,
        params: Json::Null,
        profile: Json::Null,
        account: Json::Null,
        budget: Json::Null,
        placement: "subprocess_confined".into(),
    }
}

#[test]
fn out_of_process_proposer_ops() {
    let Some(bin) = fixture_bin() else { return };
    if !helper_present() {
        eprintln!("proposer lane: hh-helper binary not found — skipped");
        return;
    }
    let pkg = package(&bin, manifest());
    let s = spec(pkg, vec!["--mode".into(), "proposer".into()]);
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).expect("bind");

    // `declare` — the instrument-grade ahe-family declaration document.
    let out = host
        .invoke(&mut session, &b, "declare", vec![], "r1", TIMEOUT)
        .expect("declare");
    let InvokeOutcome::Outputs(o) = out else {
        panic!("declare: {out:?}")
    };
    let d = &o[0];
    assert_eq!(d.get("family"), Some(&Json::str("ahe")));
    assert_eq!(d.get("uses_judge"), Some(&Json::str("no")));
    assert_eq!(d.get("maturity"), Some(&Json::str("instrument-grade")));

    // `propose` — the typed `no_addressable_failure` arm (never `[]`).
    let out = host
        .invoke(
            &mut session,
            &b,
            "propose",
            vec![Json::obj([])],
            "r2",
            TIMEOUT,
        )
        .expect("propose");
    let InvokeOutcome::Outputs(o) = out else {
        panic!("propose: {out:?}")
    };
    assert_eq!(o[0].get("kind"), Some(&Json::str("no_addressable_failure")));

    // `select_parent` — picks the handed lineage's head; ∅ lineage → null.
    let lineage = Json::Arr(vec![Json::obj([
        ("semantic_id", Json::str("def:parent")),
        ("version_id", Json::str("ver:parent")),
    ])]);
    let out = host
        .invoke(
            &mut session,
            &b,
            "select_parent",
            vec![Json::obj([("lineage", lineage)])],
            "r3",
            TIMEOUT,
        )
        .expect("select_parent");
    let InvokeOutcome::Outputs(o) = out else {
        panic!("select_parent: {out:?}")
    };
    assert_eq!(o[0].get("semantic_id"), Some(&Json::str("def:parent")));
    let out = host
        .invoke(
            &mut session,
            &b,
            "select_parent",
            vec![Json::obj([("lineage", Json::Arr(vec![]))])],
            "r4",
            TIMEOUT,
        )
        .expect("select_parent empty");
    let InvokeOutcome::Outputs(o) = out else {
        panic!("select_parent empty: {out:?}")
    };
    // Object outputs are stamped in place; a `null` output wraps as the
    // stamped envelope's `value` member.
    assert_eq!(o[0].get("value"), Some(&Json::Null));

    host.close(&mut session).unwrap();
    assert!(session.detached);
}
