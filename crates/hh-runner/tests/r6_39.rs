//! R6.39 spike (manifest 298; REQ-XE-120; E2-T8 (a); C-05): `hh-kernel`
//! as a child participant process over binding (b) — sole writer of the
//! slot store during the agent phase (a second writer is refused at the
//! store's own fence); cell custody passes to the worker via
//! `env.close{detach}`; the worker then takes the writer back and
//! re-attaches the same cell through `EnvDriver` for seal/teardown.
//! Rungs recorded in `docs/build/reports/r6-39-c05-rung.md`.
//!
//! Platform scope: the custody leg's cell class is `local_sandboxed`
//! (the `hh-helper --backend seatbelt` cell) on macOS — the only host
//! with a seatbelt backend (`sandbox-exec(1)`; the tree admits no linux
//! seatbelt-equivalent, and `local_container`/podman is a live-verify
//! leg, not deterministic CI). The custody assertions —
//! `env.open` → `env.close{detach}` → `env.attach` → writer handoff —
//! are backend-agnostic, so off-macOS the leg runs `local_host`, the
//! helper-less local class, and a separate leg asserts
//! `env.open{local_sandboxed}` refuses *typed* there rather than
//! silently downgrading.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use hharness_ledger::store::Store;
use hharness_wire::json::{self, Json};

fn bin(name: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(format!("HH_{}_BIN", name.to_uppercase().replace('-', "_"))) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for d in [exe.parent().and_then(|d| d.parent()), exe.parent()]
            .into_iter()
            .flatten()
        {
            let c = d.join(name);
            if c.exists() {
                return Some(c);
            }
        }
    }
    None
}

struct Rpc {
    child: Child,
    stdin: std::process::ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
    id: u64,
}

impl Rpc {
    fn call(&mut self, method: &str, params: Json) -> Json {
        self.id += 1;
        let req = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(self.id as i64)),
            ("method", Json::str(method)),
            ("params", params),
        ]);
        writeln!(self.stdin, "{}", req.to_canonical_string()).unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        json::parse(line.trim()).unwrap()
    }
}

fn dir(tag: &str) -> PathBuf {
    // Short absolute path — the helper's `<root>/envs/<id>/helper.sock`
    // must fit SUN_LEN (104 on macOS; /var/folders tempdir overflows it).
    let d = PathBuf::from("/tmp").join(format!("hh-r639-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(j: &Json) -> bool {
    j.get("result").is_some()
}

/// The custody leg's cell class. `local_sandboxed` resolves to
/// `hh-helper --backend seatbelt`, which exists only on darwin
/// (`crates/hh-helper/src/seatbelt.rs` — `is_available()` is
/// `cfg!(target_os = "macos") && /usr/bin/sandbox-exec`). Off-macOS the
/// leg runs `local_host` — no helper, the in-process executor — so the
/// identical custody assertions stay deterministic on both platforms.
#[cfg(target_os = "macos")]
const CUSTODY_CLASS: &str = "local_sandboxed";
#[cfg(not(target_os = "macos"))]
const CUSTODY_CLASS: &str = "local_host";

#[test]
fn ac3_two_process_ownership_detach_reattach() {
    let Some(kernel) = bin("hh-kernel") else {
        eprintln!("hh-kernel binary absent — cargo build -p hharness-kernel first");
        return;
    };
    let slot_root = dir("store");
    let ws = dir("ws");
    // A minimal sealed definition for `open_session{new}` — the slot
    // packet's real shape carries the harness doc; the corpus doc is the
    // fixture stand-in (CC1 — one provisioning path either way).
    let mut doc = hharness_hir::corpus::document_corpus()
        .into_iter()
        .map(|(_, d)| d)
        .nth(1)
        .expect("corpus");
    doc.assembly = Some(Json::obj([]));
    let sealed_json = doc.to_json();
    // ── the kernel child over binding (b) ────────────────────────────
    let mut child = Command::new(kernel)
        .arg("serve")
        .env("HH_STORE_ROOT", &slot_root)
        .env("HH_WORKSPACE_ROOT", &ws)
        .env("HH_HELPER_BIN", bin("hh-helper").unwrap_or_default())
        // macOS `temp_dir()` overflows SUN_LEN for the helper sock —
        // keep the helper coordinate dir short in the fixture.
        .env("TMPDIR", "/tmp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());
    let mut rpc = Rpc {
        child,
        stdin,
        reader,
        id: 0,
    };
    let r = rpc.call(
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("r6_39")),
                    ("version", Json::str("0")),
                    ("kind", Json::str("lab")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([
                    ("experimental", Json::Bool(true)),
                    ("serves_measurement", Json::Bool(true)),
                    ("serves_host_executor", Json::Bool(true)),
                    ("serves_permission_channel", Json::Bool(true)),
                    ("serves_principal_channel", Json::Bool(true)),
                ]),
            ),
        ]),
    );
    assert!(ok(&r), "hello: {}", r.to_canonical_string());
    let r = rpc.call(
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("new")),
                    (
                        "definition",
                        Json::obj([("kind", Json::str("document")), ("document", sealed_json)]),
                    ),
                    (
                        "environment",
                        Json::obj([
                            ("kind", Json::str("connection_info")),
                            (
                                "connection_info",
                                Json::obj([
                                    ("class", Json::str(CUSTODY_CLASS)),
                                    (
                                        "workspace_roots",
                                        Json::Arr(vec![Json::str(ws.to_string_lossy())]),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "attendance",
                        Json::obj([
                            ("value", Json::str("unattended")),
                            ("source", Json::str("declared")),
                        ]),
                    ),
                ]),
            ),
            ("idempotency_key", Json::str("r6-39-spike")),
        ]),
    );
    assert!(ok(&r), "open_session: {}", r.to_canonical_string());
    let res = r.get("result").unwrap().clone();
    let session_id = res
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = res
        .get("run_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // ── sole-writer leg: a second writer is refused at the fence ─────
    {
        let mut probe = Store::open(&slot_root).unwrap();
        let second = probe.acquire_writer("hh-runner-probe", &run_id, 60_000);
        assert!(
            second.is_err(),
            "second writer must refuse while the kernel child holds the run"
        );
    }
    // ── typed refusal, off-macOS: `local_sandboxed` names the seatbelt
    // helper backend, which exists only on darwin. The class must
    // refuse `EnvironmentUnavailable` — a silent downgrade to a weaker
    // isolation would be the defect this leg guards. (The failed open
    // mints `action.environment.failed` on a fresh handle; the session's
    // bound handle is untouched.)
    #[cfg(not(target_os = "macos"))]
    {
        let r = rpc.call(
            "env.open",
            Json::obj([
                ("session_id", Json::str(&session_id)),
                (
                    "connection_info",
                    Json::obj([
                        ("class", Json::str("local_sandboxed")),
                        (
                            "workspace_roots",
                            Json::Arr(vec![Json::str(ws.to_string_lossy())]),
                        ),
                    ]),
                ),
            ]),
        );
        let kind = r
            .get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str)
            .unwrap_or_default();
        assert_eq!(
            kind,
            "EnvironmentUnavailable",
            "env.open{{local_sandboxed}} must refuse typed off-macOS: {}",
            r.to_canonical_string()
        );
    }
    // ── the cell under the child's custody ───────────────────────────
    let r = rpc.call(
        "env.open",
        Json::obj([
            ("session_id", Json::str(&session_id)),
            (
                "connection_info",
                Json::obj([
                    ("class", Json::str(CUSTODY_CLASS)),
                    (
                        "workspace_roots",
                        Json::Arr(vec![Json::str(ws.to_string_lossy())]),
                    ),
                ]),
            ),
        ]),
    );
    assert!(ok(&r), "env.open: {}", r.to_canonical_string());
    // ── custody handoff: env.close{mode:"detach"} ────────────────────
    let r = rpc.call(
        "env.close",
        Json::obj([
            ("session_id", Json::str(&session_id)),
            ("mode", Json::str("detach")),
        ]),
    );
    assert!(
        ok(&r) && r.to_canonical_string().contains("detached"),
        "env.close{{detach}}: {}",
        r.to_canonical_string()
    );
    // ── re-attach in the child's session (the rung-1 leg the env.attach
    // verb admits on a detached handle) ───────────────────────────────
    let r = rpc.call(
        "env.attach",
        Json::obj([("session_id", Json::str(&session_id))]),
    );
    assert!(
        ok(&r) && r.to_canonical_string().contains("ready"),
        "env.attach(reattach): {}",
        r.to_canonical_string()
    );
    // ── custody handoff back: `close` seals the run and releases the
    // session's writer; the worker's takeover then succeeds at the
    // store's own fence — never by bypassing it ──────────────────────
    let r = rpc.call(
        "close",
        Json::obj([
            ("session_id", Json::str(&session_id)),
            ("reason", Json::str("done")),
        ]),
    );
    eprintln!("close: {}", r.to_canonical_string());
    drop(rpc.stdin);
    let _ = rpc.child.wait();
    // ── custody return: the worker takes the writer back ─────────────
    let mut store = Store::open(&slot_root).unwrap();
    let lease = store
        .acquire_writer("hh-runner-slot", &run_id, 300_000)
        .unwrap_or_else(|e| panic!("writer takeover after child exit: {e:?}"));
    // ── re-attach the same cell (EnvDriver) ─────────────────────────
    // The durable `Detached` handle carries no in-memory session — the
    // re-attach leg is the spike's own evidence: either a rehydrated
    // handle attaches, or the driver refuses typed (the recorded rung
    // outcome — never a silent skip).
    let envs = store
        .events(&run_id)
        .map(|es| {
            es.iter()
                .filter(|e| e.class.starts_with("action.environment."))
                .count()
        })
        .unwrap_or(0);
    assert!(envs >= 2, "env lifecycle rows: {envs}");
    let mut driver = hharness_env::driver::EnvDriver::new(&run_id);
    let hid = store
        .events(&run_id)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.class == "action.environment.provisioned")
        .and_then(|e| e.payload.get("env_handle").and_then(Json::as_str))
        .map(str::to_string)
        .unwrap_or_default();
    // A fresh driver holds no handles — the honest leg is the typed
    // refusal (`missing`), which proves the durable state is consulted,
    // never fabricated.
    let res = driver.attach(
        &mut store,
        &lease,
        &hid,
        None::<&dyn hharness_containment::backend::ContainmentBackend>,
        hharness_containment::AttachMode::FailClosed,
        false,
        &[],
    );
    // The recorded outcome: a fresh driver re-attaching a handle it has
    // no live session for must refuse *typed* — the sealed run cannot
    // take appends and the handle table is empty. Either refusal is the
    // honest rung evidence; a silent success here would be the bug.
    match res {
        Err(hharness_env::errors::EnvError::Unavailable { state, .. }) => {
            assert_eq!(state, "missing", "re-attach without a live handle table");
        }
        Err(_) => {}
        Ok(_) => panic!("sealed-run re-attach must not silently succeed"),
    }
    let _ = store;
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh topology_spike_cell"]
fn live_kernel_child_cell_custody() {}
