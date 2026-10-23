//! L4.6a (ticket 164w; ADR-0363) — the live surface & plugin sweep's
//! packaged-variant out-of-process legs. Three legs, each independently
//! runnable through `scripts/live-verify.sh <leg>` (ADR-0363: an ignored
//! test is executed only by the evidence runner, under `L4_6A_LIVE=1`):
//!
//! - `l46a_plugin_memory_store` — `plugins/hh-memory-store` bound
//!   `memory_store` `1.0`; the shared `memory_abi` conformance battery +
//!   a typed-refusal leg run over a live `subprocess_confined` session.
//! - `l46a_plugin_compact_evict_oldest` — `plugins/hh-compact-evict-oldest`
//!   bound `compaction_strategy` `1.0`; `assess → propose → execute` driven
//!   end-to-end out of process (the V1 host-stamped provenance checked on
//!   the wire).
//! - `l46a_plugin_tracker_fixture` — `plugins/hh-tracker-fixture` bound
//!   `work_source_adapter` `1.0`; `PluginSourceAdapter` drives the real
//!   `WorkSourceAdapter` seam (poll → occurrences → activate_run →
//!   list/get) over the live session.
//!
//! Every leg proves the OOP contract honestly: `pack → seal → load`
//! (real `code_pointer` pin verification), `spawn` `subprocess_confined`
//! through the live `hh-helper` boundary, and the host's `SessionEvents`
//! sink wired to a *real* `hharness-ledger::Store` run — the
//! `lifecycle.component.bound` / `lifecycle.component.invoked` rows the
//! varhost emits land on a hash-chained run ledger, exactly the seam the
//! kernel's ledger writer occupies in the kernel-driven path. A leg that
//! cannot run (env unset, binaries unbuilt) returns early — the runner
//! records the honest `skipped(reason)`, never a pass (ADR-0363; rule 5).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hharness_embed_schema::plugin_abi::BindParams;
use hharness_ledger::{manifest::RunManifest, store::Store};
use hharness_registry::extension::plugin::{manifest_from_json, manifest_to_json};
use hharness_varhost::{
    load_package, seal_package, spawn, unstamp_json, variant_class, InvokeOutcome, RecordingPorts,
    SessionEvents, SpawnSpec, VariantHost, VariantPackage, VariantSession, VecEvents,
};
use hharness_wire::json::Json;

/// The live-leg gate (ADR-0363): `scripts/live-verify.sh` exports
/// `L4_6A_LIVE=1`; any other state is an honest skip.
const LIVE_ENV: &str = "L4_6A_LIVE";
const TIMEOUT: Duration = Duration::from_secs(30);

fn live() -> bool {
    std::env::var(LIVE_ENV).ok().as_deref() == Some("1")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn target_bin(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join(name);
    p.exists().then_some(p)
}

/// `pack → seal → load`: the real packaged-variant admission path —
/// `code_pointer` pins are computed at seal and re-verified at load.
fn materialize(plugin_dir: &str, bin_name: &str) -> Option<(tempfile::TempDir, VariantPackage)> {
    let bin = target_bin(bin_name)?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::copy(&bin, root.join("bin").join(bin_name)).unwrap();
    let text = std::fs::read_to_string(
        repo_root()
            .join("plugins")
            .join(plugin_dir)
            .join("plugin.manifest.json"),
    )
    .unwrap();
    let j = hharness_wire::json::parse(&text).unwrap();
    let m = manifest_from_json(&j, "plugin.manifest.json").unwrap();
    let sealed = seal_package(root, &m).expect("seal");
    let sealed_text = manifest_to_json(&sealed).to_canonical_string();
    std::fs::write(root.join("plugin.manifest.json"), &sealed_text).unwrap();
    let pkg = load_package(root).expect("load verifies pins");
    Some((dir, pkg))
}

fn spec(
    pkg: VariantPackage,
    plugin_ref: &str,
    session_id: &str,
    class_id: &str,
    contract: &str,
    socket_dir: PathBuf,
) -> SpawnSpec {
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
        session_id: session_id.into(),
        backend: "direct".into(),
        placement: hharness_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir,
        exec_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:l4.6a-live".into(),
        holder: hharness_hir::refs::Ref::pinned(plugin_ref, "pin:sealed"),
        registry_snapshot_id: "snap-l46a".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            format!("class_contract:{class_id}"),
            contract.to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

/// Persist the host-emitted lifecycle rows onto a real
/// `hharness-ledger::Store` run — the same `(class, component_call_id,
/// payload)` tuples the kernel's `component_call`-scoped ledger writer
/// appends in the kernel-driven path. The varhost emits into `VecEvents`
/// in-session (the store is not `Send`, so the sink itself cannot hold
/// it); the leg then lands the rows on a hash-chained run under kernel
/// provenance and asserts them at `read(run)` depth — the OOP lifecycle
/// is provable durable, not just in-memory.
fn persist_lifecycle(tag: &str, rows: &[(String, Option<String>, Json)]) -> (Store, String) {
    let root = std::env::temp_dir().join(format!("l46a-vh-{}-{}", tag, std::process::id()));
    let mut store = Store::open(&root).expect("store open");
    // Non-agent runs carry no configuration/environment cells (ADR-0183
    // §C — `minimal` seeds the agent set; a surface run clears it or
    // `open_run`'s invariant refuses `ManifestInvalid`).
    let mut manifest = RunManifest::minimal(hharness_ledger::manifest::RunKind::Surface);
    manifest.configuration_id = None;
    manifest.configuration_version_id = None;
    manifest.environment_ref = None;
    manifest.environment_version_id = None;
    let (run_id, lease) = store.open_run(manifest, "l4.6a-live").expect("open_run");
    let mut batch = Vec::with_capacity(rows.len());
    for (class, cc, payload) in rows {
        let minter = hharness_env::events::EventMinter::new(&store, &run_id);
        let mut ev = minter
            .mint(class, payload.clone())
            .expect("mint lifecycle row");
        ev.scope.component_call_id = cc.clone();
        batch.push(ev);
    }
    store.append(&run_id, &lease, batch).expect("append batch");
    (store, run_id)
}

/// Assert the run-ledger side of an OOP leg: `lifecycle.component.bound`
/// with `bind_result:ok`, at least `invocations` `lifecycle.component.invoked`
/// rows — all durable on the committed run prefix.
fn assert_lifecycle_ledger(
    events: &VecEvents,
    store: &Store,
    run_id: &str,
    class_id: &str,
    invocations: usize,
) {
    let bound: Vec<&(String, Option<String>, Json)> = events
        .rows
        .iter()
        .filter(|r| r.0 == "lifecycle.component.bound")
        .collect();
    assert_eq!(bound.len(), 1, "one bound row: {:?}", bound);
    let b = bound[0].2.get("bind_result").and_then(Json::as_str);
    assert_eq!(b, Some("ok"), "bind_result: {:?}", bound[0].2);
    assert_eq!(
        bound[0].2.get("class_id").and_then(Json::as_str),
        Some(class_id)
    );
    assert_eq!(
        bound[0].2.get("placement").and_then(Json::as_str),
        Some("subprocess_confined"),
        "the row names the OOP placement"
    );
    let invoked_count = events
        .rows
        .iter()
        .filter(|r| r.0 == "lifecycle.component.invoked")
        .count();
    assert!(
        invoked_count >= invocations,
        "expected ≥{invocations} invoked rows, got {invoked_count}"
    );
    // The durable side: the same classes are on the committed run prefix.
    let committed = store.events(run_id).expect("read run");
    let names: Vec<&str> = committed.iter().map(|e| e.class.as_str()).collect();
    assert!(
        names.contains(&"lifecycle.component.bound"),
        "committed run lacks the bound row: {names:?}"
    );
    let committed_invocations = names
        .iter()
        .filter(|c| **c == "lifecycle.component.invoked")
        .count();
    assert_eq!(
        committed_invocations, invoked_count,
        "committed run must carry every emitted invoked row"
    );
}

// ── leg 1: hh-memory-store ────────────────────────────────────────────────

/// The `plugin_abi/1` session adapter for the `memory_store` contract —
/// identical to `memory_store_plugin.rs`'s channel, over the ledger sink.
struct MemoryChannel<'a, E: SessionEvents> {
    host: &'a mut VariantHost<RecordingPorts, E>,
    session: &'a mut VariantSession,
    binding: String,
    seq: u64,
}

impl<'a, E: SessionEvents> hharness_context::memory_abi::StoreChannel for MemoryChannel<'a, E> {
    fn invoke(
        &mut self,
        operation: &str,
        inputs: Vec<Json>,
    ) -> Result<Vec<Json>, hharness_context::memory_abi::ChannelFault> {
        use hharness_context::memory_abi::ChannelFault;
        self.seq += 1;
        let out = self
            .host
            .invoke(
                self.session,
                &self.binding,
                operation,
                inputs,
                &format!("ms-l46a-r{}", self.seq),
                TIMEOUT,
            )
            .map_err(|e| ChannelFault::Transport(format!("{e:?}")))?;
        match out {
            InvokeOutcome::Outputs(docs) => docs
                .iter()
                .map(|d| hharness_context::memory_abi::unwrap_output(&unstamp_json(d)))
                .collect(),
            InvokeOutcome::Failed(e) => Err(ChannelFault::Transport(format!(
                "plugin abi failure: {e:?}"
            ))),
        }
    }
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh plugin-memory-store"]
fn l46a_plugin_memory_store() {
    use hharness_context::memory::MemoryStorePort;
    use hharness_context::memory_abi::{RemoteStore, MEMORY_STORE_CLASS, MEMORY_STORE_CONTRACT};

    if !live() {
        eprintln!("{LIVE_ENV} unset — skipped(live gate)");
        return;
    }
    let Some((_t, pkg)) = materialize("hh-memory-store", "hh-memory-store") else {
        panic!("hh-memory-store binary not built — build the plugin set first");
    };
    if hharness_env::helper::helper_binary().is_none() {
        panic!("hh-helper binary not found — build -p hharness-helper first");
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some(MEMORY_STORE_CLASS));
    let dir = std::env::temp_dir().join(format!("vh-ms-l46a-{}", std::process::id()));
    let (mut s, lowered) = spawn(&spec(
        pkg,
        "plugin:hh/hh-memory-store",
        "ms-l46a-1",
        MEMORY_STORE_CLASS,
        MEMORY_STORE_CONTRACT,
        dir,
    ))
    .expect("spawn subprocess_confined via hh-helper");
    // Lowering losses are named, never silent (§8.4 AC-3) — record them in
    // the transcript rather than asserting zero (ambient env a leg does not
    // supply is a legitimate loss, not a failure).
    if !lowered.losses.is_empty() {
        println!("PLUGIN_OOP_LOSSES {:?}", lowered.losses);
    }
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
        .expect("bind memory_store");

    // The shared conformance battery, over the live subprocess — the same
    // suite the in-process store and the dispatch loopback pass (T-LCD-12).
    let remote = RemoteStore::new(MemoryChannel {
        host: &mut h,
        session: &mut s,
        binding: b.clone(),
        seq: 0,
    });
    let mut remote = remote;
    hharness_context::memory_abi::conformance::run(&mut remote);
    assert!(
        remote.fault().is_none(),
        "latched fault: {:?}",
        remote.fault()
    );
    // A typed domain refusal crosses the process boundary as data:
    let err = remote
        .resolve(
            hharness_provenance::authority::PersistenceScope::Run,
            Some("facts/l46a-never-bound"),
            None,
            hharness_identity::names::ResolveMode::Execute,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            hharness_context::memory::MemoryError::UnknownName { .. }
        ),
        "{err:?}"
    );
    drop(remote);

    h.close(&mut s).unwrap();
    assert!(s.detached, "close detaches the session");

    // The lifecycle rows land on a real run ledger — durable proof of the
    // OOP session (bound + every invoke, hash-chained).
    let (store, run_id) = persist_lifecycle("memory-store", &h.events.rows);
    // ≥ the battery's own invokes + the refusal probe — the ledger counts.
    assert_lifecycle_ledger(&h.events, &store, &run_id, MEMORY_STORE_CLASS, 2);
    println!(
        "PLUGIN_OOP variant=hh-memory-store class={MEMORY_STORE_CLASS} placement=subprocess_confined bound_rows={} invoked_rows={} run={}",
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.bound")
            .count(),
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.invoked")
            .count(),
        run_id
    );
    println!(r#"LIVE_USAGE: {{"input_total":0,"output_total":0}}"#);
    println!("LIVE_MODEL: none");
}

// ── leg 2: hh-compact-evict-oldest ────────────────────────────────────────

fn compact_context_view() -> Json {
    // 4 items, 400 tokens; hard cap 200 → evict-oldest until under.
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
#[ignore = "live — ADR-0363: scripts/live-verify.sh plugin-compact-evict-oldest"]
fn l46a_plugin_compact_evict_oldest() {
    if !live() {
        eprintln!("{LIVE_ENV} unset — skipped(live gate)");
        return;
    }
    let Some((_t, pkg)) = materialize("hh-compact-evict-oldest", "hh-compact-evict-oldest") else {
        panic!("hh-compact-evict-oldest binary not built — build the plugin set first");
    };
    if hharness_env::helper::helper_binary().is_none() {
        panic!("hh-helper binary not found — build -p hharness-helper first");
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some("compaction_strategy"));
    let dir = std::env::temp_dir().join(format!("vh-cp-l46a-{}", std::process::id()));
    let (mut s, _lowered) = spawn(&spec(
        pkg,
        "plugin:hh/hh-compact-evict-oldest",
        "cp-l46a-1",
        "compaction_strategy",
        "1.0",
        dir,
    ))
    .expect("spawn subprocess_confined via hh-helper");
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
        .expect("bind compaction_strategy");

    // assess → propose → execute — the three-op contract round trip, each a
    // real out-of-process invoke with the V1 host stamp on the outputs.
    let req = Json::obj([("kind", Json::str("hard")), ("cap", Json::Int(200))]);
    let out = h
        .invoke(
            &mut s,
            &b,
            "assess",
            vec![compact_context_view(), req],
            "a1",
            TIMEOUT,
        )
        .expect("assess");
    let InvokeOutcome::Outputs(assess) = out else {
        panic!("assess: not outputs")
    };
    assert_eq!(assess[0].get("required"), Some(&Json::Bool(true)));
    assert_eq!(
        assess[0].get("origin").and_then(Json::as_str),
        Some("tool:hh/hh-compact-evict-oldest"),
        "V1 host stamp on the OOP output"
    );

    let out = h
        .invoke(
            &mut s,
            &b,
            "propose",
            vec![assess[0].clone(), compact_context_view()],
            "a2",
            TIMEOUT,
        )
        .expect("propose");
    let InvokeOutcome::Outputs(prop) = out else {
        panic!("propose: not outputs")
    };
    let Json::Arr(ops) = prop[0].get("ops").expect("ops") else {
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
            vec![prop[0].clone(), compact_context_view()],
            "a3",
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
    let (store, run_id) = persist_lifecycle("compact-evict-oldest", &h.events.rows);
    assert_lifecycle_ledger(&h.events, &store, &run_id, "compaction_strategy", 3);
    println!(
        "PLUGIN_OOP variant=hh-compact-evict-oldest class=compaction_strategy placement=subprocess_confined bound_rows={} invoked_rows={} run={}",
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.bound")
            .count(),
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.invoked")
            .count(),
        run_id
    );
    println!(r#"LIVE_USAGE: {{"input_total":0,"output_total":0}}"#);
    println!("LIVE_MODEL: none");
}

// ── leg 3: hh-tracker-fixture ─────────────────────────────────────────────

/// `SourceChannel` over the bound session — `PluginSourceAdapter` drives
/// the real `WorkSourceAdapter` seam through it.
struct SourceChan<'a, E: SessionEvents> {
    host: &'a mut VariantHost<RecordingPorts, E>,
    session: &'a mut VariantSession,
    binding: String,
    seq: u64,
}

impl<'a, E: SessionEvents> hharness_fleet_adapter::adapter::SourceChannel for SourceChan<'a, E> {
    fn invoke(
        &mut self,
        operation: &str,
        inputs: Vec<Json>,
    ) -> Result<Vec<Json>, hharness_fleet_adapter::adapter::SourceFault> {
        use hharness_fleet_adapter::adapter::SourceFault;
        self.seq += 1;
        let out = self
            .host
            .invoke(
                self.session,
                &self.binding,
                operation,
                inputs,
                &format!("tf-l46a-r{}", self.seq),
                TIMEOUT,
            )
            .map_err(|e| SourceFault::Transport(format!("{e:?}")))?;
        match out {
            InvokeOutcome::Outputs(docs) => Ok(docs.iter().map(unstamp_json).collect()),
            InvokeOutcome::Failed(e) => Err(SourceFault::Transport(format!("abi failure: {e:?}"))),
        }
    }
}

fn tracker_fixture_doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("occ-l46a")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("ticket.updated")),
                (
                    "item",
                    Json::obj([
                        ("item_id", Json::str("i-l46a")),
                        ("title", Json::str("ticket i-l46a")),
                        (
                            "source",
                            Json::obj([
                                ("source_id", Json::str("tickets")),
                                ("kind", Json::str("ticket")),
                                ("state", Json::str("open")),
                            ]),
                        ),
                        ("idempotency_key", Json::str("idem:i-l46a")),
                        ("owner", Json::str("alice")),
                    ]),
                ),
                ("observed_at_ms", Json::Int(9_900)),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
        ("activate_run", Json::Arr(vec![Json::str("i-l46a")])),
        (
            "records",
            Json::Arr(vec![Json::obj([
                ("native_id", Json::str("T-L46A")),
                ("state", Json::str("open")),
            ])]),
        ),
    ])
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh plugin-tracker-fixture"]
fn l46a_plugin_tracker_fixture() {
    use hharness_fleet::source::WorkSourceAdapter;
    use hharness_fleet_adapter::adapter::PluginSourceAdapter;
    use hharness_fleet_adapter::{WORK_SOURCE_CLASS, WORK_SOURCE_CONTRACT};

    if !live() {
        eprintln!("{LIVE_ENV} unset — skipped(live gate)");
        return;
    }
    let Some((_t, pkg)) = materialize("hh-tracker-fixture", "hh-tracker-fixture") else {
        panic!("hh-tracker-fixture binary not built — build the plugin set first");
    };
    if hharness_env::helper::helper_binary().is_none() {
        panic!("hh-helper binary not found — build -p hharness-helper first");
    }
    assert_eq!(variant_class(&pkg).as_deref(), Some(WORK_SOURCE_CLASS));
    let dir = std::env::temp_dir().join(format!("vh-tf-l46a-{}", std::process::id()));
    let (mut s, _lowered) = spawn(&spec(
        pkg,
        "plugin:hh/hh-tracker-fixture",
        "tf-l46a-1",
        WORK_SOURCE_CLASS,
        WORK_SOURCE_CONTRACT,
        dir,
    ))
    .expect("spawn subprocess_confined via hh-helper");
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
                params: Json::obj([("fixture_doc", tracker_fixture_doc())]),
                profile: Json::Null,
                account: Json::Null,
                budget: Json::Null,
                placement: "subprocess_confined".into(),
            },
        )
        .expect("bind work_source_adapter");

    // The `WorkSourceAdapter` seam over the live subprocess session —
    // capabilities → occurrences → activate_run → list/get (AC-R-2.12.6's
    // out-of-process shape; the fixture is the proxy for HUMAN-H2's real
    // tracker, never a simulation of it).
    let mut ad = PluginSourceAdapter::new(
        SourceChan {
            host: &mut h,
            session: &mut s,
            binding: b.clone(),
            seq: 0,
        },
        "src:tickets",
    );
    ad.bind_probe();
    assert_eq!(
        ad.capabilities().poll,
        hharness_fleet::capabilities::CapState::Declared,
        "the fixture tracker declares poll"
    );
    let occs = ad.occurrences(None);
    assert_eq!(occs.len(), 1);
    assert_eq!(occs[0].item.item_id, "i-l46a");
    assert_eq!(
        ad.activate_run(&["i-l46a".to_string(), "other".to_string()]),
        vec!["i-l46a".to_string()]
    );
    assert_eq!(ad.list(&[]).len(), 1);
    assert_eq!(ad.get(&["T-L46A".to_string()]).len(), 1);
    assert!(ad.fault().is_none(), "latched fault: {:?}", ad.fault());
    drop(ad);

    h.close(&mut s).unwrap();
    assert!(s.detached);
    let (store, run_id) = persist_lifecycle("tracker-fixture", &h.events.rows);
    // bind_probe's capabilities invoke + occurrences + activate_run +
    // list + get — ≥4 invoked rows on the run ledger.
    assert_lifecycle_ledger(&h.events, &store, &run_id, WORK_SOURCE_CLASS, 4);
    println!(
        "PLUGIN_OOP variant=hh-tracker-fixture class={WORK_SOURCE_CLASS} placement=subprocess_confined bound_rows={} invoked_rows={} run={}",
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.bound")
            .count(),
        h.events
            .rows
            .iter()
            .filter(|r| r.0 == "lifecycle.component.invoked")
            .count(),
        run_id
    );
    println!(r#"LIVE_USAGE: {{"input_total":0,"output_total":0}}"#);
    println!("LIVE_MODEL: none");
}
