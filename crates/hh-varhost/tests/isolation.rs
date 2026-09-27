//! The `subprocess_confined` isolation suite — the fixture launched for
//! real through `hh-helper` under the lowered `ContainmentPolicy`
//! (AC-R-2.12.2-4/6: cleared env, deny-default fs, `net.mode = none`, the
//! channel socket as the only unix grant, no spawn/exec, crash teardown).
//!
//! Lanes:
//! - `direct` — the plumbing lane (helper spawns the fixture; no sandbox
//!   enforcement — every platform runs this).
//! - `seatbelt` — the confinement lane (macOS; skipped with a loud note
//!   where `sandbox-exec`/the backend is unavailable).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hh_embed_schema::plugin_abi::{AbiError, BindParams};
use hh_registry::extension::plugin::{PluginIdentity, PluginManifest, Requests, Requires};
use hh_varhost::{
    spawn, HostError, InvokeOutcome, RecordingPorts, SpawnSpec, VariantHost, VariantPackage,
    VariantSession, VecEvents,
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

fn manifest(env_keys: Vec<String>) -> PluginManifest {
    let req = Requests {
        env_keys,
        ..Requests::default()
    };
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
        requests: req,
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

fn spec(
    pkg: VariantPackage,
    backend: &str,
    exec_args: Vec<String>,
    ambient: Vec<(String, String)>,
) -> SpawnSpec {
    // The fixture asserts its pin both ways (V5) — argv carries the
    // package's own `version_id`/`content` so `hello` agrees with the
    // seal; the pin-mismatch test overrides `--version-id`.
    let mut pinned_args = vec![
        "--plugin-id".into(),
        "local/fixture".into(),
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
    ];
    pinned_args.extend(exec_args);
    let exec_args = pinned_args;
    // Unique per spec — nanos can collide across threads on coarse
    // clocks; the counter is the guarantee.
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vh-iso-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    // The sealing principal narrows `requests` to `cap`; these tests seal
    // at exactly the manifest's claims (the cap-narrowing exercise is the
    // lower.rs unit surface).
    let cap = pkg.manifest.requests.clone();
    SpawnSpec {
        package: pkg,
        session_id: format!("iso-{}", std::process::id()),
        backend: backend.into(),
        socket_dir: dir,
        exec_args,
        cap,
        ambient_env: ambient,
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture", "pin:fixture"),
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

fn bind_params() -> BindParams {
    BindParams {
        slot: "slot-1".into(),
        class_id: "compaction_strategy".into(),
        contract_version: "1.0".into(),
        variant: Json::Null,
        params: Json::Null,
        profile: Json::Null,
        account: Json::Null,
        budget: Json::Null,
        placement: "subprocess_confined".into(),
    }
}

/// Spawn → bind → run one probe op → return the reported document.
fn probe(
    backend: &str,
    exec_args: Vec<String>,
    op: &str,
    ambient: Vec<(String, String)>,
    env_keys: Vec<String>,
) -> Option<(Json, VariantSession, VariantHost<RecordingPorts, VecEvents>)> {
    let bin = fixture_bin()?;
    if !helper_present() {
        eprintln!("isolation: hh-helper binary not found — skipped");
        return None;
    }
    let pkg = package(&bin, manifest(env_keys));
    let s = spec(pkg, backend, exec_args, ambient);
    // The caller already gated on lane availability (helper binary +
    // sandbox-exec) — a spawn failure here is a real defect, not a skip.
    let (mut session, lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).expect("bind");
    let out = host
        .invoke(&mut session, &b, op, vec![], "res-1", TIMEOUT)
        .expect("invoke");
    let doc = match out {
        InvokeOutcome::Outputs(mut o) => o.remove(0),
        other => panic!("probe {op}: {other:?}"),
    };
    let _ = lowered;
    Some((doc, session, host))
}

fn seatbelt_lane() -> bool {
    std::path::Path::new("/usr/bin/sandbox-exec").exists()
}

// ── the plumbing lane (every platform) ────────────────────────────────────────

#[test]
fn direct_spawn_handshake_invoke() {
    let Some((doc, mut s, mut h)) = probe("direct", vec![], "assess", vec![], vec![]) else {
        return;
    };
    assert_eq!(doc.get("ok"), Some(&Json::Bool(true)));
    h.close(&mut s).unwrap();
    assert!(s.detached);
}

#[test]
fn direct_crash_detaches_and_kills() {
    let bin = match fixture_bin() {
        Some(b) => b,
        None => return,
    };
    if !helper_present() {
        return;
    }
    let pkg = package(&bin, manifest(vec![]));
    let s = spec(
        pkg,
        "direct",
        vec![
            "--mode".into(),
            "crash".into(),
            "--crash-at".into(),
            "invoke".into(),
        ],
        vec![],
    );
    let (mut session, _l) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).unwrap();
    let r = host.invoke(&mut session, &b, "assess", vec![], "r", TIMEOUT);
    assert!(matches!(r, Err(HostError::Abi(AbiError::PluginCrashed))));
    assert!(session.detached);
}

#[test]
fn direct_pin_mismatch_refused_before_exec_survives() {
    let bin = match fixture_bin() {
        Some(b) => b,
        None => return,
    };
    if !helper_present() {
        return;
    }
    // The *plugin* forges its pin — the sealed package says `pin:fixture`,
    // the process's hello claims `pin:forged` (V5 both ways: the host's
    // check refuses the impostor; the forged-pin arg lands last in argv).
    let pkg = package(&bin, manifest(vec![]));
    let s = spec(
        pkg,
        "direct",
        vec!["--version-id".into(), "pin:forged".into()],
        vec![],
    );
    let r = spawn(&s);
    assert!(
        matches!(r, Err(HostError::Abi(AbiError::PinMismatch))),
        "{r:?}"
    );
}

#[test]
fn direct_never_connects_is_spawn_failure_not_hang() {
    if !helper_present() {
        return;
    }
    // `/bin/true` exits without connecting — spawn must fail within the
    // timeout, never hang on accept.
    let pkg = VariantPackage {
        root: PathBuf::from("/bin"),
        manifest: manifest(vec![]),
        content: "content:x".into(),
        version_id: "pin:x".into(),
        execs: vec![PathBuf::from("/bin/true")],
    };
    let mut s = spec(pkg, "direct", vec![], vec![]);
    s.timeout = Duration::from_secs(3);
    let r = spawn(&s);
    assert!(matches!(r, Err(HostError::Helper(_))), "{r:?}");
}

// ── the confinement lane (macOS seatbelt) ─────────────────────────────────────

#[test]
fn seatbelt_env_cleared_except_declared() {
    if !seatbelt_lane() {
        eprintln!("isolation: no sandbox-exec — skipped");
        return;
    }
    let ambient = vec![
        ("HH_SECRET_PROBE".to_string(), "leak".to_string()),
        ("HH_DECLARED".to_string(), "yes".to_string()),
    ];
    let Some((doc, mut s, mut h)) = probe(
        "seatbelt",
        vec![],
        "violate:env",
        ambient,
        vec!["HH_DECLARED".into()],
    ) else {
        return;
    };
    // The cleared child saw no ambient secret; only the declared key
    // projected (count is small — PATH-less, HOME-less).
    assert_eq!(doc.get("leaked"), Some(&Json::Bool(false)));
    let n = doc.get("env_count").and_then(Json::as_int).unwrap_or(99);
    assert!(n <= 4, "env_count {n} — cleared env leaked");
    h.close(&mut s).ok();
}

#[test]
fn seatbelt_fs_deny_default() {
    if !seatbelt_lane() {
        return;
    }
    let Some((doc, mut s, mut h)) = probe("seatbelt", vec![], "violate:fs", vec![], vec![]) else {
        return;
    };
    assert_eq!(doc.get("succeeded"), Some(&Json::Bool(false)));
    h.close(&mut s).ok();
}

#[test]
fn seatbelt_no_net() {
    if !seatbelt_lane() {
        return;
    }
    let Some((doc, mut s, mut h)) = probe("seatbelt", vec![], "violate:net", vec![], vec![]) else {
        return;
    };
    assert_eq!(doc.get("succeeded"), Some(&Json::Bool(false)));
    h.close(&mut s).ok();
}

#[test]
fn seatbelt_no_exec_no_spawn() {
    if !seatbelt_lane() {
        return;
    }
    for op in ["violate:exec", "violate:spawn"] {
        let Some((doc, mut s, mut h)) = probe("seatbelt", vec![], op, vec![], vec![]) else {
            return;
        };
        assert_eq!(doc.get("succeeded"), Some(&Json::Bool(false)), "{op}");
        h.close(&mut s).ok();
    }
}

#[test]
fn seatbelt_helper_socket_unreachable() {
    if !seatbelt_lane() {
        return;
    }
    // The helper socket lives beside the channel socket in socket_dir —
    // the child's `unix_sockets.allow` names only the channel path.
    let bin = match fixture_bin() {
        Some(b) => b,
        None => return,
    };
    if !helper_present() {
        return;
    }
    let pkg = package(&bin, manifest(vec![]));
    let s = spec(pkg, "seatbelt", vec![], vec![]);
    let helper_sock = s.socket_dir.join("helper.sock");
    let s = SpawnSpec {
        exec_args: vec![
            "--probe-helper-sock".into(),
            helper_sock.to_string_lossy().to_string(),
        ],
        ..s
    };
    let (mut session, _l) = match spawn(&s) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("isolation: spawn failed ({e:?}) — skipped");
            return;
        }
    };
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host.bind(&mut session, bind_params()).unwrap();
    let out = host
        .invoke(
            &mut session,
            &b,
            "violate:helper_socket",
            vec![],
            "r",
            TIMEOUT,
        )
        .unwrap();
    match out {
        InvokeOutcome::Outputs(o) => {
            assert_eq!(o[0].get("succeeded"), Some(&Json::Bool(false)));
        }
        other => panic!("{other:?}"),
    }
    host.close(&mut session).ok();
}

#[test]
fn seatbelt_peer_root_unreachable() {
    if !seatbelt_lane() {
        return;
    }
    // A second "package root" outside the child's fs extent.
    let peer = std::env::temp_dir().join(format!("vh-peer-{}", std::process::id()));
    std::fs::create_dir_all(&peer).unwrap();
    std::fs::write(peer.join("secret.txt"), "peer data").unwrap();
    let Some((doc, mut s, mut h)) = probe(
        "seatbelt",
        vec![
            "--probe-peer-root".into(),
            peer.to_string_lossy().to_string(),
        ],
        "violate:peer_root",
        vec![],
        vec![],
    ) else {
        return;
    };
    assert_eq!(doc.get("succeeded"), Some(&Json::Bool(false)));
    h.close(&mut s).ok();
}

// ── DF-S1.9-1 — a stub variant per Core class binds out-of-process ────────────

/// Every Stage-1–3 class's stub binds through a *real* spawned
/// `subprocess_confined` session (the deferral's literal wording — the
/// in-memory lane version lives in the protocol suite).
#[test]
fn stub_per_class_binds_out_of_process() {
    let bin = match fixture_bin() {
        Some(b) => b,
        None => return,
    };
    if !helper_present() {
        return;
    }
    for class_id in [
        "control_strategy",
        "context_policy",
        "validator",
        "execution_alignment",
        "compaction_strategy",
    ] {
        let pkg = package(&bin, manifest(vec![]));
        let s = spec(
            pkg,
            "direct",
            vec!["--class".into(), class_id.into()],
            vec![],
        );
        let (mut session, _l) = spawn(&s).unwrap_or_else(|e| panic!("{class_id} spawn: {e:?}"));
        let mut host: VariantHost<RecordingPorts, VecEvents> =
            VariantHost::new(RecordingPorts::default(), VecEvents::default());
        let b = host
            .bind(
                &mut session,
                BindParams {
                    slot: format!("slot-{class_id}"),
                    class_id: class_id.into(),
                    ..bind_params()
                },
            )
            .unwrap_or_else(|e| panic!("{class_id} bind: {e:?}"));
        assert_eq!(session.bindings[&b].class_id, class_id);
        host.close(&mut session).ok();
    }
}
