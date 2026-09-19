//! The packaged first-party variant end-to-end (AC-R-2.12.2-6/13):
//! `plugins/hh-compact-evict-oldest` is sealed at pack time, `load`
//! pin-verifies it, the host spawns it `subprocess_confined` through the
//! helper, binds `compaction_strategy`, and the `evict_oldest` contract
//! ops return deterministic, host-stamped results. A tampered package is
//! `PinMismatch` at `load` — never silently resealed (AC-13).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hh_embed_schema::plugin_abi::{AbiError, BindParams};
use hh_registry::extension::plugin::{manifest_from_json, manifest_to_json};
use hh_varhost::{
    load_package, seal_package, spawn, variant_class, HostError, InvokeOutcome, RecordingPorts,
    SpawnSpec, VariantHost, VecEvents,
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

fn compact_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-compact-evict-oldest");
    p.exists().then_some(p)
}

#[test]
fn packaged_manifest_decodes_and_declares() {
    let text = std::fs::read_to_string(
        repo_root().join("plugins/hh-compact-evict-oldest/plugin.manifest.json"),
    )
    .expect("packaged manifest readable");
    let j = hh_wire::json::parse(&text).expect("manifest json");
    let m = manifest_from_json(&j, "plugin.manifest.json").expect("manifest decodes");
    assert_eq!(m.identity.id(), "hh/hh-compact-evict-oldest");
    // The variant contribution declares the compaction_strategy class.
    let c = m.contributions.iter().find(|c| {
        c.declaration.get("class_id").and_then(Json::as_str) == Some("compaction_strategy")
    });
    assert!(c.is_some(), "no compaction_strategy contribution");
    // Declared placement is the only legal one for a variant here.
    let exe = c.unwrap().executable.as_ref().expect("executable");
    assert_eq!(
        exe.isolation,
        hh_registry::extension::IsolationClass::SubprocessConfined
    );
}

/// Materialize the package: manifest + `bin/` copy of the built variant
/// binary, `seal` (real code_pointer pins), write, `load` (verify).
fn materialize() -> Option<(tempfile::TempDir, hh_varhost::VariantPackage)> {
    let bin = compact_bin()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::copy(&bin, root.join("bin/hh-compact-evict-oldest")).unwrap();
    let text = std::fs::read_to_string(
        repo_root().join("plugins/hh-compact-evict-oldest/plugin.manifest.json"),
    )
    .unwrap();
    let j = hh_wire::json::parse(&text).unwrap();
    let m = manifest_from_json(&j, "plugin.manifest.json").unwrap();
    let sealed = seal_package(root, &m).expect("seal");
    // The zero-pin template came back with a real code_pointer.
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
        session_id: "pkg-1".into(),
        backend: "direct".into(),
        socket_dir,
        exec_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:hh/hh-compact-evict-oldest", "pin:sealed"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:compaction_strategy".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

fn context_view() -> Json {
    // 4 items, 400 tokens total; hard cap 200 → evict oldest until under.
    Json::obj([
        (
            "items",
            Json::Arr(
                [1, 2, 3, 4]
                    .iter()
                    .map(|seq| Json::obj([("seq", Json::Int(*seq)), ("tokens", Json::Int(100))]))
                    .collect(),
            ),
        ),
        ("tokens", Json::Int(400)),
    ])
}

#[test]
fn packaged_variant_runs_end_to_end() {
    let Some((_t, pkg)) = materialize() else {
        eprintln!("hh-compact-evict-oldest binary not built — skipped");
        return;
    };
    if hh_env::helper::helper_binary().is_none() {
        eprintln!("hh-helper binary not found — skipped");
        return;
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some("compaction_strategy"));
    let dir = std::env::temp_dir().join(format!(
        "vh-pkg-{}-{}",
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
                slot: "compaction".into(),
                class_id: "compaction_strategy".into(),
                contract_version: "1.0".into(),
                variant: Json::Null,
                params: Json::Null,
                profile: Json::Null,
                account: Json::Null,
                budget: Json::Null,
                placement: "subprocess_confined".into(),
            },
        )
        .expect("bind");

    // assess(view, hard-cap 200) → required, then propose → evict ops.
    let req = Json::obj([("kind", Json::str("hard")), ("cap", Json::Int(200))]);
    let out = h
        .invoke(
            &mut s,
            &b,
            "assess",
            vec![context_view(), req],
            "r1",
            TIMEOUT,
        )
        .expect("assess");
    let InvokeOutcome::Outputs(assess) = out else {
        panic!("assess: not outputs")
    };
    assert_eq!(
        assess[0].get("required"),
        Some(&Json::Bool(true)),
        "assessment: {assess:?}"
    );
    // Host-stamped provenance (V1): plugin output carries tool origin.
    assert_eq!(
        assess[0].get("origin").and_then(Json::as_str),
        Some("tool:hh/hh-compact-evict-oldest")
    );
    assert_eq!(
        assess[0].get("authority").and_then(Json::as_str),
        Some("external")
    );

    let out = h
        .invoke(
            &mut s,
            &b,
            "propose",
            vec![assess[0].clone(), context_view()],
            "r2",
            TIMEOUT,
        )
        .expect("propose");
    let InvokeOutcome::Outputs(prop) = out else {
        panic!("propose: not outputs")
    };
    let ops = prop[0].get("ops").expect("ops");
    let Json::Arr(ops) = ops else {
        panic!("ops not array")
    };
    assert!(!ops.is_empty(), "no eviction proposed: {prop:?}");
    assert!(ops
        .iter()
        .all(|o| o.get("op").and_then(Json::as_str) == Some("evict")));

    let out = h
        .invoke(
            &mut s,
            &b,
            "execute",
            vec![prop[0].clone(), context_view()],
            "r3",
            TIMEOUT,
        )
        .expect("execute");
    let InvokeOutcome::Outputs(res) = out else {
        panic!("execute: not outputs")
    };
    let remaining = res[0]
        .get("remaining_tokens")
        .and_then(Json::as_int)
        .unwrap_or(9999);
    assert!(remaining <= 200, "remaining {remaining} > cap: {res:?}");

    h.close(&mut s).unwrap();
    assert!(s.detached);
}

#[test]
fn tampered_package_is_pin_mismatch_never_resealed() {
    let Some((dir, _pkg)) = materialize() else {
        return;
    };
    // Mutate the binary after seal — `load` refuses PinMismatch; the
    // package is never re-sealed implicitly (AC-13's `update` clause: a
    // changed artifact is a *new* package, admitted by a principal).
    let bin = dir.path().join("bin/hh-compact-evict-oldest");
    let mut bytes = std::fs::read(&bin).unwrap();
    bytes[0] ^= 0xff;
    std::fs::write(&bin, bytes).unwrap();
    let r = load_package(dir.path());
    assert!(
        matches!(r, Err(HostError::Abi(AbiError::PinMismatch))),
        "{r:?}"
    );
}

#[test]
fn wrong_manifest_pin_is_refused() {
    let Some((dir, _)) = materialize() else {
        return;
    };
    // Corrupt the recorded `code_pointer` digest inside the sealed
    // manifest — the loader's recomputation disagrees: `PinMismatch`.
    let mp = dir.path().join("plugin.manifest.json");
    let text = std::fs::read_to_string(&mp).unwrap();
    let mut j = hh_wire::json::parse(&text).unwrap();
    // contributions[0].executable.code_pointer.digest ← 64×'f'
    let Json::Obj(m) = &mut j else { panic!() };
    let Json::Arr(contribs) = m.get_mut("contributions").unwrap() else {
        panic!()
    };
    let Json::Obj(c0) = &mut contribs[0] else {
        panic!()
    };
    let Json::Obj(exe) = c0.get_mut("executable").unwrap() else {
        panic!()
    };
    let Json::Obj(cp) = exe.get_mut("code_pointer").unwrap() else {
        panic!()
    };
    cp.insert("digest".into(), Json::str("f".repeat(64)));
    std::fs::write(&mp, j.to_canonical_string()).unwrap();
    let r = load_package(dir.path());
    assert!(
        matches!(r, Err(HostError::Abi(AbiError::PinMismatch))),
        "{r:?}"
    );
}
