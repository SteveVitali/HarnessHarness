//! The packaged `work_source_adapter` GitHub Issues variant end-to-end
//! (§5i.1, R-2.12.6/AC-9; R4.7; DF-S5.6-1; HUMAN-H2 "GitHub Issues +
//! tunnel"): `plugins/hh-github-issues` is sealed at pack time, `load`
//! pin-verifies it, the host spawns it `subprocess_confined` through the
//! helper, binds `work_source_adapter` `1.0` with the
//! `hh.fleet.github_source/1` document (the `github_doc` bind parameter —
//! the live issue snapshot the host pins in), and `PluginSourceAdapter`
//! drives the `WorkSourceAdapter` seam over the live session — the real
//! tracker variant's out-of-process leg (records-in/records-out; the
//! confined child has no network egress).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hharness_embed_schema::plugin_abi::BindParams;
use hharness_fleet::capabilities::CapState;
use hharness_fleet::source::WorkSourceAdapter;
use hharness_fleet_adapter::adapter::{PluginSourceAdapter, SourceChannel, SourceFault};
use hharness_fleet_adapter::{WORK_SOURCE_CLASS, WORK_SOURCE_CONTRACT};
use hharness_registry::extension::plugin::{manifest_from_json, manifest_to_json};
use hharness_varhost::{
    load_package, seal_package, spawn, unstamp_json, variant_class, InvokeOutcome, RecordingPorts,
    SpawnSpec, VariantHost, VecEvents,
};
use hharness_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn github_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-github-issues");
    p.exists().then_some(p)
}

/// Materialize the package: manifest + `bin/` copy of the built variant
/// binary, `seal` (real code_pointer pins), write, `load` (verify).
fn materialize() -> Option<(tempfile::TempDir, hharness_varhost::VariantPackage)> {
    let bin = github_bin()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::copy(&bin, root.join("bin/hh-github-issues")).unwrap();
    let text =
        std::fs::read_to_string(repo_root().join("plugins/hh-github-issues/plugin.manifest.json"))
            .unwrap();
    let j = hharness_wire::json::parse(&text).unwrap();
    let m = manifest_from_json(&j, "plugin.manifest.json").unwrap();
    let sealed = seal_package(root, &m).expect("seal");
    let sealed_text = manifest_to_json(&sealed).to_canonical_string();
    std::fs::write(root.join("plugin.manifest.json"), &sealed_text).unwrap();
    let pkg = load_package(root).expect("load verifies pins");
    Some((dir, pkg))
}

fn spec(pkg: hharness_varhost::VariantPackage, socket_dir: PathBuf) -> SpawnSpec {
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
        session_id: "gh-pkg-1".into(),
        backend: "direct".into(),
        placement: hharness_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir,
        exec_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hharness_hir::refs::Ref::pinned("plugin:hh/hh-github-issues", "pin:sealed"),
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
    session: &'a mut hharness_varhost::VariantSession,
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
                &format!("gh-r{}", self.seq),
                TIMEOUT,
            )
            .map_err(|e| SourceFault::Transport(format!("{e:?}")))?;
        match out {
            InvokeOutcome::Outputs(docs) => Ok(docs.iter().map(unstamp_json).collect()),
            InvokeOutcome::Failed(e) => Err(SourceFault::Transport(format!("abi failure: {e:?}"))),
        }
    }
}

/// The `hh.fleet.github_source/1` document — `github` carries the
/// dialect config; `issues[]` is the pinned issue snapshot the host
/// fetched live (records-in); `occurrences` is a verbatim occurrence
/// the host mapped.
fn github_doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.github_source/1")),
        (
            "github",
            Json::obj([
                ("repo", Json::str("acme/sandbox")),
                ("source_id", Json::str("github")),
                ("owner_map", Json::obj([("octocat", Json::str("alice"))])),
                ("owner_default", Json::str("ops")),
            ]),
        ),
        (
            "issues",
            Json::Arr(vec![
                Json::obj([
                    ("number", Json::Int(7)),
                    ("title", Json::str("Wire the webhook")),
                    ("state", Json::str("open")),
                    (
                        "html_url",
                        Json::str("https://github.com/acme/sandbox/issues/7"),
                    ),
                    ("assignee", Json::obj([("login", Json::str("octocat"))])),
                    (
                        "labels",
                        Json::Arr(vec![Json::obj([("name", Json::str("hh-live-test"))])]),
                    ),
                    ("updated_at", Json::str("2026-10-21T00:00:07Z")),
                ]),
                Json::obj([
                    ("number", Json::Int(3)),
                    ("title", Json::str("Bootstrap the sandbox")),
                    ("state", Json::str("closed")),
                    (
                        "html_url",
                        Json::str("https://github.com/acme/sandbox/issues/3"),
                    ),
                    ("assignee", Json::Null),
                    ("labels", Json::Arr(vec![])),
                    ("updated_at", Json::str("2026-10-20T23:00:00Z")),
                ]),
            ]),
        ),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("gh-occ-1")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("github.issue")),
                (
                    "item",
                    Json::obj([
                        ("item_id", Json::str("gh-acme-sandbox-7")),
                        ("title", Json::str("Wire the webhook")),
                        (
                            "source",
                            Json::obj([
                                ("source_id", Json::str("github")),
                                ("kind", Json::str("github.issue")),
                                ("native_id", Json::str("7")),
                                ("state", Json::str("open")),
                            ]),
                        ),
                        ("idempotency_key", Json::str("idem:gh-7")),
                        ("owner", Json::str("alice")),
                    ]),
                ),
                ("observed_at_ms", Json::Int(1_000)),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
    ])
}

#[test]
fn packaged_manifest_decodes_and_declares() {
    let text =
        std::fs::read_to_string(repo_root().join("plugins/hh-github-issues/plugin.manifest.json"))
            .expect("packaged manifest readable");
    let j = hharness_wire::json::parse(&text).expect("manifest json");
    let m = manifest_from_json(&j, "plugin.manifest.json").expect("manifest decodes");
    assert_eq!(m.identity.id(), "hh/hh-github-issues");
    let c = m
        .contributions
        .iter()
        .find(|c| c.declaration.get("class_id").and_then(Json::as_str) == Some(WORK_SOURCE_CLASS));
    assert!(c.is_some(), "no work_source_adapter contribution");
    let exe = c.unwrap().executable.as_ref().expect("executable");
    assert_eq!(
        exe.isolation,
        hharness_registry::extension::IsolationClass::SubprocessConfined
    );
    // The confined variant requests nothing — no egress, no env keys,
    // no fs roots (records-in/records-out).
    assert!(m.requests.egress.is_empty(), "the plugin admits no egress");
    assert!(
        m.requests.env_keys.is_empty(),
        "no env keys — no secrets in"
    );
}

#[test]
fn github_plugin_runs_the_contract_ops_end_to_end() {
    let Some((_t, pkg)) = materialize() else {
        eprintln!("hh-github-issues binary not built — skipped");
        return;
    };
    if hharness_env::helper::helper_binary().is_none() {
        eprintln!("hh-helper binary not found — skipped");
        return;
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some(WORK_SOURCE_CLASS));
    let dir = std::env::temp_dir().join(format!(
        "vh-gh-{}-{}",
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
                params: Json::obj([("github_doc", github_doc())]),
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
        "src:github",
    );
    // The capability probe is one out-of-process `capabilities` invoke.
    ad.bind_probe();
    let caps = ad.capabilities();
    assert_eq!(
        caps.push_delivery_id,
        CapState::Declared,
        "X-GitHub-Delivery is the declared durable delivery id"
    );
    assert_eq!(caps.poll, CapState::Declared);
    assert_eq!(caps.native_write_tools, CapState::DeclaredAbsent);
    let occs = ad.occurrences(None);
    assert_eq!(occs.len(), 1);
    assert_eq!(occs[0].occurrence_id, "gh-occ-1");
    assert_eq!(occs[0].item.item_id, "gh-acme-sandbox-7");
    assert_eq!(
        ad.activate_run(&[
            "gh-acme-sandbox-7".to_string(),
            "gh-acme-sandbox-3".to_string(),
        ]),
        vec!["gh-acme-sandbox-7".to_string()],
        "open issues only are dispatchable"
    );
    let listed = ad.list(&[]);
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0].get("native_id").and_then(Json::as_str),
        Some("7"),
        "the GitHub issue number is the native id"
    );
    assert_eq!(ad.get(&["3".to_string()]).len(), 1);
    assert!(!ad.suspended("github"));
    assert!(ad.fault().is_none(), "latched fault: {:?}", ad.fault());

    drop(ad);
    h.close(&mut s).unwrap();
    assert!(s.detached);
}
