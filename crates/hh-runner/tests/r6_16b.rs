//! R6.16 acceptance battery — Stage 2 (manifest 269; RUNNER-SUPERVISION):
//! the model proxy and egress mediator as separate sole-writer
//! processes (AC-7/AC-4.1-14 REQ-XC-101), the Json control channel and
//! the per-slot token mint/revoke lifecycle (AC-8/AC-4.3-9 REQ-XC-109,
//! X-05), the exact sealed phase socket grants (AC-8/AC-4.1-15), the
//! credential-confinement cell environment (DF-R6.7-1's R6.16 leg),
//! the mechanical carry-in fold of prior proxy ledgers with
//! unharvested/unverified refusals (AC-9/AC-10.1-14 REQ-XH-120, X-02),
//! and the supervised drive end-to-end.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

fn arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("not an array"),
    }
}

fn boolv(j: &Json) -> Option<bool> {
    match j {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

fn obj(j: &Json) -> &BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("not an object"),
    }
}

use hharness_runner::supervision::{
    self, fold_prior_window, MediatorProc, ProxyCtl, Supervision, SupervisionDecl,
    MEDIATOR_BIN_ENV, MEDIATOR_BIN_NAME, PROXY_BIN_ENV, PROXY_BIN_NAME,
};

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r616b-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The fixture upstream credential — deliberately NOT a key-shaped
/// literal (records carry `provided: yes`, never a value; gitleaks).
const CREDENTIAL: &str = "fixture-credential-do-not-project";
const CREDENTIAL_ENV: &str = "HH_TEST_OLLAMA_KEY";

/// `ensure_bin(pkg, bin, env)` — env override → `target/debug` sibling;
/// build the binary once when absent (the r6_15 precedent: the test
/// declares its own tool, never a host-PATH guess).
fn ensure_bin(pkg: &str, bin: &str, env_var: &str) -> PathBuf {
    if let Ok(e) = std::env::var(env_var) {
        return PathBuf::from(e);
    }
    let exe = std::env::current_exe().unwrap();
    let debug = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let path = debug.join(bin);
    if !path.is_file() {
        let st = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", pkg, "--bin", bin])
            .status()
            .unwrap();
        assert!(st.success(), "cargo build -p {pkg} --bin {bin}");
    }
    assert!(path.is_file(), "{bin} did not build");
    path
}

fn proxy_bin() -> PathBuf {
    ensure_bin("hharness-hosting", PROXY_BIN_NAME, PROXY_BIN_ENV)
}

fn mediator_bin() -> PathBuf {
    ensure_bin("hharness-env", MEDIATOR_BIN_NAME, MEDIATOR_BIN_ENV)
}

/// A minimal configure doc — the lane is fixture-bound; no upstream
/// connection is ever opened in the offline battery.
fn configure_json(root: &Path, run_id: &str, prior: Vec<Json>) -> Json {
    Json::obj([
        ("op", Json::str("configure")),
        ("store_root", Json::str(root.display().to_string())),
        ("run_id", Json::str(run_id)),
        ("listen", Json::str("127.0.0.1:0")),
        (
            "lane",
            Json::obj([
                ("lane_id", Json::str("lane:r6_16")),
                ("endpoint_ref", Json::str("endpoint:fixture")),
                ("upstream_host", Json::str("127.0.0.1")),
                ("model", Json::str("model:fixture")),
                (
                    "reservation",
                    Json::obj([
                        ("kind", Json::str("fixture_bound")),
                        ("micros", Json::Int(1_000_000)),
                    ]),
                ),
            ]),
        ),
        ("credential_env", Json::str(CREDENTIAL_ENV)),
        (
            "authorization",
            Json::obj([
                ("verbatim", Json::str("auth:verbatim")),
                ("gate_record_ref", Json::str("gate:fixture")),
                ("authorized_micros", Json::Int(100_000_000)),
                ("available_credit_micros", Json::Int(100_000_000)),
                ("m_ppm", Json::Int(10_000)),
                ("prior_windows", Json::Arr(prior)),
            ]),
        ),
    ])
}

fn child_env() -> BTreeMap<String, String> {
    BTreeMap::from([(CREDENTIAL_ENV.to_string(), CREDENTIAL.to_string())])
}

fn mk_event(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("r6_16-test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap_or_default(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("r6_16-test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// A finished prior proxy window: one clean call (100µ), one
/// `proxy_aborted` orphan (250µ at bounded-hi) — the fold's expected
/// `consumed=350, orphan_calls=1, orphan_hi=250`.
fn prior_ledger(root: &Path, run_id: &str) {
    let mut store = Store::open(root).unwrap();
    let m = RunManifest::minimal(RunKind::Agent);
    let (_r, lease) = store.open_run_with_id(run_id, m, "r6_16-test").unwrap();
    let ok_ev = mk_event(
        &store,
        run_id,
        "model.call.completed",
        Json::obj([("call", Json::str("c1"))]),
    );
    let ok_id = ok_ev.event_id.clone();
    let abort_ev = mk_event(
        &store,
        run_id,
        "model.call.failed",
        Json::obj([
            ("call", Json::str("c2")),
            ("disposition", Json::str("proxy_aborted")),
        ]),
    );
    let abort_id = abort_ev.event_id.clone();
    store.append(run_id, &lease, vec![ok_ev, abort_ev]).unwrap();
    let cost = |src: &str, micros: i64| {
        mk_event(
            &store,
            run_id,
            "measurement.cost.attributed",
            Json::obj([
                ("money", Json::obj([("micro_units", Json::Int(micros))])),
                ("source_event", Json::obj([("event_id", Json::str(src))])),
            ]),
        )
    };
    store
        .append(
            run_id,
            &lease,
            vec![cost(&ok_id, 100), cost(&abort_id, 250)],
        )
        .unwrap();
}

fn phases() -> Json {
    let grants = |paths: &[&str]| {
        Json::obj([(
            "layer",
            Json::obj([(
                "net",
                Json::obj([(
                    "unix_sockets",
                    Json::obj([(
                        "allow",
                        Json::Arr(paths.iter().map(|x| Json::str(*x)).collect()),
                    )]),
                )]),
            )]),
        )])
    };
    let both = grants(&[
        "\u{2f}run\u{2f}hh\u{2f}model.sock",
        "\u{2f}run\u{2f}hh\u{2f}egress.sock",
    ]);
    let egress_only = grants(&["\u{2f}run\u{2f}hh\u{2f}egress.sock"]);
    Json::obj([
        ("setup", both.clone()),
        ("agent", both),
        ("verify", egress_only),
    ])
}

/// `ac7` — the proxy is a separate sole-writer process on its Json
/// control channel: `status` round-trips, `register_slot` mints a fresh
/// token, `revoke_slot` lands `security.credential.revoked`; the rows
/// read back read-only from the proxy's own store root.
#[test]
fn ac7_proxy_control_channel_and_token_lifecycle() {
    let root = tmp("ctl");
    let proxy_root = root.join("proxy");
    let bin = proxy_bin();
    let cfg = configure_json(&proxy_root, "proxy-W", vec![]);
    let mut ctl = ProxyCtl::spawn(&bin, &cfg, &child_env()).unwrap();
    assert_eq!(ctl.store_root, PathBuf::new()); // caller stamps it
    assert!(!ctl.listen.is_empty());
    let st = ctl.status().unwrap();
    assert_eq!(st.get("run_id").and_then(|v| v.as_str()), Some("proxy-W"));
    // Mint — a fresh token per slot open.
    let t1 = ctl.register_slot("slot-x", "high", 1_000).unwrap();
    let t2 = ctl.register_slot("slot-y", "high", 1_000).unwrap();
    assert!(!t1.is_empty() && !t2.is_empty() && t1 != t2);
    let st = ctl.status().unwrap();
    let slots = st.get("slots").map(arr).unwrap();
    assert!(slots
        .iter()
        .all(|s| s.get("live").and_then(boolv) == Some(true)));
    // Revoke — the token dies; the slice remainder returns.
    ctl.revoke_slot("slot-x").unwrap();
    let st = ctl.status().unwrap();
    let slots = st.get("slots").map(arr).unwrap();
    let x = slots
        .iter()
        .find(|s| s.get("slot_id").and_then(|v| v.as_str()) == Some("slot-x"))
        .unwrap();
    assert_eq!(x.get("live").and_then(boolv), Some(false));
    let proxy_run = "proxy-W".to_string();
    let root_path = proxy_root.clone();
    ctl.stop();
    // Read-only fold of the proxy's own ledger — bound + revoked land.
    let store = Store::open(&root_path).unwrap();
    let evs = store.events(&proxy_run).unwrap();
    assert_eq!(
        evs.iter()
            .filter(|e| e.class == "security.credential.bound")
            .count(),
        2
    );
    assert_eq!(
        evs.iter()
            .filter(|e| e.class == "security.credential.revoked")
            .count(),
        1
    );
}

/// `ac7` — the sole-writer fence holds across processes: while the
/// proxy lives, a second writer on its run is `WouldBlock`, and a
/// foreign process still reads the root the proxy is writing.
#[test]
fn ac7_sole_writer_topology_and_cross_process_read() {
    let root = tmp("sole");
    let proxy_root = root.join("proxy");
    let bin = proxy_bin();
    let cfg = configure_json(&proxy_root, "proxy-W2", vec![]);
    let mut ctl = ProxyCtl::spawn(&bin, &cfg, &child_env()).unwrap();
    ctl.register_slot("slot-live", "high", 500).unwrap();
    // A second process's writer on the same run is fenced out.
    let mut reader = Store::open(&proxy_root).unwrap();
    let second = reader.acquire_writer("other-holder", "proxy-W2", 60_000);
    assert!(
        matches!(
            second,
            Err(hharness_ledger::errors::LedgerError::WouldBlock { .. })
        ),
        "a second writer must fence: {second:?}"
    );
    // The cross-process read still sees what the writer already landed.
    let evs = reader.events("proxy-W2").unwrap();
    assert!(evs.iter().any(|e| e.class == "security.credential.bound"));
    ctl.stop();
}

/// `ac8` — the carry-in fold is mechanical: harvested+verified rows
/// only, consumed Σ attributed micro-units, orphans at bounded-hi; an
/// absent run is the typed `unharvested ledger` refusal (X-02).
#[test]
fn ac8_carry_in_fold_and_unharvested_refusal() {
    let root = tmp("carry");
    let prior_root = root.join("prior-proxy");
    prior_ledger(&prior_root, "proxy-W0");
    let spend = fold_prior_window(&prior_root, "proxy-W0").unwrap();
    assert_eq!(spend.consumed_micros, 350);
    assert_eq!(spend.orphan_calls, 1);
    assert_eq!(spend.orphan_bounded_hi_micros, 250);
    assert_eq!(spend.to_json().get("verified").and_then(boolv), Some(true));
    // Unharvested — the run is absent; no estimate substitutes.
    let e = fold_prior_window(&prior_root, "proxy-nope").unwrap_err();
    assert!(e.detail.contains("unharvested ledger"), "{}", e.detail);
}

/// `ac7` — the mediator is a separate sole-writer process; the `ready`
/// handshake names its socket and run, and the kill is a clean reap.
#[test]
fn ac7_mediator_spawn_ready_and_stop() {
    let root = tmp("med");
    let policy = root.join("policy.json");
    let doc = hharness_containment::policy::kernel_default(1).to_json();
    std::fs::write(&policy, doc.to_canonical_string()).unwrap();
    let args = vec![
        "--store-root".to_string(),
        root.join("egress").display().to_string(),
        "--socket".to_string(),
        root.join("egress.sock").display().to_string(),
        "--policy".to_string(),
        policy.display().to_string(),
        "--env-handle".to_string(),
        "env:fixture".to_string(),
        "--effect-id".to_string(),
        "effect:fixture".to_string(),
        "--participant".to_string(),
        "participant:fixture".to_string(),
        "--run-id".to_string(),
        "med-run-1".to_string(),
    ];
    let proc_ = MediatorProc::spawn(&mediator_bin(), &args).unwrap();
    assert_eq!(proc_.run_id, "med-run-1");
    assert!(proc_.socket.ends_with("egress.sock"));
    proc_.stop();
}

/// `ac9` + DF-R6.7-1 — the supervised drive end-to-end: the proxy
/// process mints one fresh token per attempt and revokes each at close
/// (`security.credential.revoked` × slots); the sealed phase schedule's
/// socket grants land in the slot's cell record; the cell env carries
/// the slot token and NEVER the upstream credential — the forwarding
/// process alone holds it.
#[test]
fn ac9_supervised_drive_tokens_grants_and_credential_confinement() {
    let root = tmp("sup");
    let prior_root = root.join("prior-proxy");
    prior_ledger(&prior_root, "proxy-W0");
    // The supervision declaration — as a Json member, the same spelling
    // the drive packet decodes.
    let decl_json = Json::obj([
        (
            "proxy",
            Json::obj([
                ("bin", Json::str(proxy_bin().display().to_string())),
                (
                    "store_root",
                    Json::str(root.join("proxy").display().to_string()),
                ),
                ("run_id", Json::str("proxy-W1")),
                ("listen", Json::str("127.0.0.1:0")),
                (
                    "lane",
                    Json::obj([
                        ("lane_id", Json::str("lane:r6_16")),
                        ("endpoint_ref", Json::str("endpoint:fixture")),
                        ("upstream_host", Json::str("127.0.0.1")),
                        ("model", Json::str("model:fixture")),
                        (
                            "reservation",
                            Json::obj([
                                ("kind", Json::str("fixture_bound")),
                                ("micros", Json::Int(1_000_000)),
                            ]),
                        ),
                    ]),
                ),
                ("credential_env", Json::str(CREDENTIAL_ENV)),
                (
                    "authorization",
                    Json::obj([
                        ("verbatim", Json::str("auth:verbatim")),
                        ("gate_record_ref", Json::str("gate:fixture")),
                        ("authorized_micros", Json::Int(100_000_000)),
                        ("available_credit_micros", Json::Int(100_000_000)),
                        ("m_ppm", Json::Int(10_000)),
                    ]),
                ),
                (
                    "prior_ledgers",
                    Json::Arr(vec![Json::obj([
                        ("store_root", Json::str(prior_root.display().to_string())),
                        ("run_id", Json::str("proxy-W0")),
                    ])]),
                ),
                ("slice_micros", Json::Int(1_000)),
                ("effort", Json::str("high")),
                (
                    "child_env",
                    Json::obj([(CREDENTIAL_ENV, Json::str(CREDENTIAL))]),
                ),
            ]),
        ),
        ("phase_schedule", phases()),
    ]);
    let decl = SupervisionDecl::from_json(&decl_json).unwrap();
    let pbin = proxy_bin();
    let mut sup = Supervision::open(&decl, &pbin, None).unwrap();
    // The folded carry-in is the declared ledger's mechanical sum —
    // refused rows would have stopped the open.
    assert_eq!(sup.prior_windows.len(), 1);
    assert_eq!(sup.prior_windows[0].consumed_micros, 350);
    // Slot open — the token + grants + env projection.
    let stamp = sup.slot_open("slot-live-1").unwrap();
    let env = stamp.get("env").map(obj).unwrap();
    // DF-R6.7-1: no cell-env member carries the upstream credential;
    // the model routing credential is the slot token only.
    for (k, v) in env {
        let val = v.as_str().unwrap_or("");
        assert_ne!(
            val, CREDENTIAL,
            "cell env member {k} must never carry the upstream credential"
        );
    }
    let slot_token = env.get("OPENAI_API_KEY").and_then(|v| v.as_str()).unwrap();
    assert!(!slot_token.is_empty() && slot_token != CREDENTIAL);
    assert!(env
        .get("no_proxy")
        .and_then(|v| v.as_str())
        .map(|v| v.contains("127.0.0.1") && v.contains("localhost"))
        .unwrap_or(false));
    // Grants — setup/agent carry model+egress, verify egress only.
    let grants = stamp.get("grants").map(obj).unwrap();
    let allow = |phase: &str| -> Vec<String> {
        grants
            .get(phase)
            .map(arr)
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    assert_eq!(allow("setup").len(), 2);
    assert_eq!(allow("agent").len(), 2);
    assert_eq!(allow("verify").len(), 1);
    assert!(allow("verify")[0].ends_with("egress.sock"));
    // Slot close — revocation lands; a second revoke is idempotent at
    // the proxy (unknown/dead slot is a typed refusal, recorded).
    sup.slot_close("slot-live-1").unwrap();
    let proxy_run = sup.proxy.run_id.clone();
    let proxy_root = sup.proxy.store_root.clone();
    sup.close();
    let store = Store::open(&proxy_root).unwrap();
    let evs = store.events(&proxy_run).unwrap();
    assert_eq!(
        evs.iter()
            .filter(|e| e.class == "security.credential.bound")
            .count(),
        1
    );
    assert_eq!(
        evs.iter()
            .filter(|e| e.class == "security.credential.revoked")
            .count(),
        1
    );
}

/// `ac9` — a fold refusal stops the window before any process spawns:
/// a declared prior ledger that does not exist is `unharvested`, never
/// estimated (X-02).
#[test]
fn ac9_unharvested_prior_refuses_before_spawn() {
    let root = tmp("unharvested");
    let decl_json = Json::obj([
        (
            "proxy",
            Json::obj([
                ("bin", Json::str(proxy_bin().display().to_string())),
                (
                    "store_root",
                    Json::str(root.join("proxy").display().to_string()),
                ),
                ("run_id", Json::str("proxy-W1")),
                (
                    "lane",
                    Json::obj([
                        ("lane_id", Json::str("lane:r6_16")),
                        ("endpoint_ref", Json::str("endpoint:fixture")),
                        ("upstream_host", Json::str("127.0.0.1")),
                        ("model", Json::str("model:fixture")),
                        (
                            "reservation",
                            Json::obj([
                                ("kind", Json::str("fixture_bound")),
                                ("micros", Json::Int(1_000_000)),
                            ]),
                        ),
                    ]),
                ),
                (
                    "authorization",
                    Json::obj([
                        ("verbatim", Json::str("auth:verbatim")),
                        ("gate_record_ref", Json::str("gate:fixture")),
                        ("authorized_micros", Json::Int(100_000_000)),
                        ("available_credit_micros", Json::Int(100_000_000)),
                        ("m_ppm", Json::Int(10_000)),
                    ]),
                ),
                (
                    "prior_ledgers",
                    Json::Arr(vec![Json::obj([
                        (
                            "store_root",
                            Json::str(root.join("missing").display().to_string()),
                        ),
                        ("run_id", Json::str("proxy-W0")),
                    ])]),
                ),
                ("slice_micros", Json::Int(1_000)),
            ]),
        ),
        ("phase_schedule", phases()),
    ]);
    let decl = SupervisionDecl::from_json(&decl_json).unwrap();
    let e = match Supervision::open(&decl, &proxy_bin(), None) {
        Ok(_) => panic!("open should refuse an unharvested prior"),
        Err(e) => e,
    };
    assert!(e.detail.contains("unharvested ledger"), "{}", e.detail);
    // The window refused BEFORE the proxy spawned — its root was never
    // created by the runner (the proxy is the only writer, and it never
    // started).
    assert_eq!(
        std::fs::read_to_string(root.join("proxy").join("store.tag")).ok(),
        None
    );
}

/// The declared member decodes strictly — unknown members refuse.
#[test]
fn ac9_supervision_decl_strict() {
    let bad = Json::obj([("proxy", Json::obj([])), ("surprise", Json::Int(1))]);
    assert!(SupervisionDecl::from_json(&bad).is_err());
    let bad = Json::obj([("proxy", Json::obj([("store_root", Json::str("x"))]))]);
    assert!(SupervisionDecl::from_json(&bad).is_err());
}

/// A stderr-visible check that the supervision module exposes the
/// declared constants (the host-tool seam names).
#[test]
fn ac9_declared_bin_env_names() {
    assert_eq!(PROXY_BIN_ENV, "HH_MODEL_PROXY_BIN");
    assert_eq!(MEDIATOR_BIN_ENV, "HH_EGRESS_MEDIATOR_BIN");
    assert_eq!(supervision::PROXY_BIN_NAME, "hh-model-proxy");
    assert_eq!(supervision::MEDIATOR_BIN_NAME, "hh-egress-mediator");
}
