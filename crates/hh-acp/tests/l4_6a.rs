//! L4.6a — the `hh-acp` surface leg (ADR-0363; ticket 164w;
//! R-2.5.4²/R-2.11.4): the *shipped* `hh acp` verb driven live over its
//! real stdio channel by a real ACP client — `initialize` →
//! `session/new` → `session/prompt` → a `session/request_permission`
//! round-trip → `state.turn.completed` — on the provisioned local lane
//! (`env:HH_LOCAL_*` coordinates, USD 0), then `verify` +
//! `replay{deterministic}` on the run's durable record.
//!
//! The transport shape is load-bearing: `hh`'s entry point reads piped
//! stdin to EOF up front (M-1 — a pipe can never carry the interactive
//! permission round-trip), so the leg spawns `script -q /dev/null`
//! (BSD `script` — a real pty) wrapping
//! `stty -echo; exec hh acp <def> --profile <coord>` — the same channel
//! an editor's spawn presents: a TTY stdin the verb never pre-reads and
//! a frame-clean stdout (the child's stderr rides a file, never the
//! frame stream). `hh acp` itself spawns `hh-kernel serve` over
//! `HH_KERNEL_CMD` — a real spawned kernel, never an in-process seam.
//!
//! The official ACP SDK arm runs first when `HH_LIVE_ACP_SDK` names a
//! populated temp install dir holding `@zed-industries/agent-client-
//! protocol` + the copied `scripts/live-capstone-acp.mjs` driver; the
//! in-repo `AcpClient` over the same pty transport is the fallback —
//! the `ACP_CLIENT:` marker names which client actually ran (a pin
//! refusal or install failure records honestly, never a silent pass).
//!
//! `#[ignore]`d — `scripts/live-verify.sh surface-acp` drives it;
//! unprovisioned lanes print `unprovided — skipped`.

#![allow(clippy::unwrap_used)]

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use hharness_acp::{AcpClient, AcpEra, PermissionOutcome, StaticPi, StdioSessionTransport};
use hharness_assembly::grammar::Assembly;
use hharness_embed_client_generated::Client as KernelClient;
use hharness_hir::document::{HirDocument, Node};
use hharness_hir::kinds::EntityKind;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;

/// The capstone task — `hh.submit`'s compiled surface spec takes no
/// parameters and the output validator is closed-world strict, so the
/// honest call is `hh_submit` with an empty args object (a `text` arg
/// is a `schema_violation`, never a dispatch). The permission
/// round-trip is exercised by the declared `requires_approval`
/// capability below — its open-time `security.permission.{pending,
/// requested}` rows reach the client as `session/request_permission`
/// under `approval_mode: manual` (the wire leg is what's proven, never
/// the completion text).
const REFERENCE_TASK: &str = "You have exactly one tool available: hh_submit, which takes no arguments. Call hh_submit now with an empty arguments object {}. If you cannot call tools, reply with the single word done.";

/// The open-time permission ask's capability — declared on the `hh acp`
/// invocation (`--capability`, the same record `hh run` parses). `hh acp`
/// opens sessions `approval_mode: manual` + attended, so a
/// `requires_approval` declaration mints
/// `security.permission.{pending,requested}` at `session/new` and the
/// driver surfaces it as `session/request_permission` — the round-trip
/// this leg proves. The model is instructed to call only `hh_submit`;
/// the probe surface exists so the ask is durable, not to be invoked.
const PROBE_CAP: &str = r#"{"capability_id":"hh.approval_probe","requires_approval":true,"options":["allow_once","deny_once"]}"#;

fn provisioned() -> bool {
    std::env::var("HH_LOCAL_BASE_URL").is_ok() && std::env::var("HH_LOCAL_MODEL").is_ok()
}

fn test_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-l46a-acp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn bin(name: &str) -> PathBuf {
    // target/debug/deps/<test>-<hash> → target/debug/<name>
    let exe = std::env::current_exe().unwrap();
    let dbg = exe.parent().unwrap().parent().unwrap();
    let p = dbg.join(name);
    assert!(p.exists(), "{name} not built — the leg builds it first");
    p
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

// ── the hir/1 document (the l4_6 live-lane shape, CC1) ─────────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}
fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}
fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}
fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

/// `native.profile` pins the registered profile's coordinate
/// (`profile_id@version`) — `lab.serve`'s compile resolves it through
/// the kernel's registry catalog, never a fixture substitution.
const PROFILE_ID: &str = "test:l46a-local";
const PROFILE_VERSION: &str = "1.0";

fn profile_coordinate() -> String {
    format!("{PROFILE_ID}@{PROFILE_VERSION}")
}

fn document_json() -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: std::collections::BTreeMap::from([
                    (
                        "tokens.blended".to_string(),
                        DimensionBound {
                            hard: Some(500_000),
                            soft: None,
                        },
                    ),
                    // The loop bound — a live model that keeps calling
                    // escalates + parks, never spins.
                    (
                        "model_calls".to_string(),
                        DimensionBound {
                            hard: Some(6),
                            soft: None,
                        },
                    ),
                ]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let mut slots = std::collections::BTreeMap::new();
    slots.insert(
        "model_port".to_string(),
        SlotBindings::One(SlotBinding {
            variant: ComponentVariantRef::selected("model_port", "hh/gateway_http", "latest"),
            params: std::collections::BTreeMap::from([
                ("dialect".to_string(), Json::str("openai-compatible@1")),
                ("endpoint".to_string(), Json::str("env:HH_LOCAL_BASE_URL")),
                (
                    "provider_model_id".to_string(),
                    Json::str("env:HH_LOCAL_MODEL"),
                ),
            ]),
            enabled: true,
            locality: None,
        }),
    );
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: profile_coordinate(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

// ── the bound profile's registry admission ──────────────────────────

use hharness_compiler::profile::{
    DebtStatus, ExpiryCondition, ExpiryKind, ModelProfile, ModelRole, ProfileCompatibility,
    ProfileDebtRecord, ProfileSelector, VersionPattern,
};

fn profile_debt(rule_id: &str) -> ProfileDebtRecord {
    ProfileDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: "the model honours the declared contract".to_string(),
        evidence_refs: vec![hharness_hir::EvidenceRef::legacy("sha256:ev")],
        owner: "test:owner".to_string(),
        reach_via: Vec::new(),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::Date,
            value: Some("2099-01-01".to_string()),
        },
        removal_test_ref: "sha256:test".to_string(),
        removal_test: Some(hharness_hir::RemovalTest::new(
            hharness_hir::RemovalTestKind::Inspection,
        )),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        created_at: None,
        supersedes: None,
    }
}

fn local_profile() -> ModelProfile {
    let mut p = ModelProfile {
        profile_id: PROFILE_ID.to_string(),
        version: PROFILE_VERSION.to_string(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "openai-compatible".to_string(),
            model_family: "local".to_string(),
            version_pattern: VersionPattern::Any,
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: Default::default(),
        rules: vec![],
        ext: std::collections::BTreeMap::new(),
        expiry: profile_debt("test:l46a-local.expiry"),
        compatibility: ProfileCompatibility {
            inventory_version: "1.0".to_string(),
            min_compiler_version: "0.0.0".to_string(),
        },
        tests: Json::obj([]),
    };
    p.content_hash = hharness_compiler::profile::profile_identity(&p);
    p
}

fn hello_params() -> Json {
    Json::obj(vec![
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj(vec![
                ("name", Json::str("l4.6a-acp")),
                ("version", Json::str("1")),
                ("kind", Json::str("test")),
            ]),
        ),
        (
            "capabilities",
            Json::obj(vec![
                ("experimental", Json::Bool(true)),
                ("serves_measurement", Json::Bool(true)),
            ]),
        ),
    ])
}

fn registrar() -> Json {
    ProvenanceRecord::kernel("hh-acp-l46a", 0).to_json()
}

/// Spawn `hh-kernel serve` on the leg's store and admit the profile
/// record + its passing `ProfileTestReport` (AC-R-2.3.3-13 — a bound
/// profile with no report is `ProfileUntested`, a hard link refusal).
/// The boundary's `lab.registry.register` is the one admission path —
/// the `hh acp` verb's own `lab.serve` compile then resolves the
/// coordinate from the same store.
fn admit_profile(store: &Path, ws: &Path) {
    let mut cmd = Command::new(bin("hh-kernel"));
    cmd.env("HH_STORE_ROOT", store)
        .env("HH_WORKSPACE_ROOT", ws)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child: Child = cmd.spawn().expect("spawn hh-kernel serve");
    let mut client = KernelClient::new(
        BufReader::new(child.stdout.take().unwrap()),
        child.stdin.take().unwrap(),
    );
    client.call("hello", hello_params()).expect("hello");
    let p = local_profile();
    let r = client
        .call(
            "lab.registry.register",
            Json::obj([
                ("kind", Json::str("model_profile")),
                (
                    "body",
                    Json::obj([
                        ("kind", Json::str("model_profile")),
                        ("profile", hharness_compiler::schema::profile_to_json(&p)),
                    ]),
                ),
                ("registrar", registrar()),
            ]),
        )
        .unwrap_or_else(|e| panic!("model_profile register: {e:?}"));
    println!(
        "ACP_PROFILE: {} version_id={}",
        profile_coordinate(),
        r.get("version_id").and_then(Json::as_str).unwrap_or("?")
    );
    let report = hharness_compiler::profile_test::ProfileTestReport::passing_for(
        &profile_coordinate(),
        &p.content_hash,
    );
    client
        .call(
            "lab.registry.register",
            Json::obj([
                ("kind", Json::str("profile_test_report")),
                (
                    "body",
                    Json::obj([
                        ("kind", Json::str("profile_test_report")),
                        (
                            "report",
                            hharness_compiler::profile_test::test_report_json(&report),
                        ),
                    ]),
                ),
                ("registrar", registrar()),
            ]),
        )
        .unwrap_or_else(|e| panic!("profile_test_report register: {e:?}"));
    drop(client);
    let _ = child.kill();
    let _ = child.wait();
}

// ── the pty spawn (BSD `script` — the real interactive channel) ─────

/// `script -q /dev/null sh -c 'stty -echo; exec <hh> acp <def>
/// --profile <coord> 2><err>'` — `hh` sees a TTY stdin (interactive
/// serve, never the piped-stdin pre-read), `stty -echo` keeps the
/// client's request bytes off the frame stream, and the child's
/// stderr lands in `err_log` so progress lines never poison the
/// newline-delimited ACP frames.
struct PtyAcp {
    child: Child,
    transport:
        StdioSessionTransport<BufReader<std::process::ChildStdout>, std::process::ChildStdin>,
    err_log: PathBuf,
}

fn spawn_acp(def: &Path, coord: &str, root: &Path) -> Option<PtyAcp> {
    // Presence probe — `script -q /dev/null true` exits 0 when the pty
    // wrapper works; any spawn/status failure skips honestly.
    let ok = Command::new("script")
        .args(["-q", "/dev/null", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("script(1) unavailable — cannot give hh a pty");
        return None;
    }
    let err_log = root.join("acp.stderr.log");
    let inner = "stty -echo; exec \"$0\" acp \"$1\" --profile \"$2\" --capability \"$4\" 2>\"$3\""
        .to_string();
    let mut cmd = Command::new("script");
    cmd.args(["-q", "/dev/null", "sh", "-c", &inner])
        .arg(bin("hh"))
        .arg(def)
        .arg(coord)
        .arg(&err_log)
        .arg(PROBE_CAP)
        .env("HH_KERNEL_CMD", bin("hh-kernel"))
        .env("HH_STORE_ROOT", root.join("store"))
        .env("HH_WORKSPACE_ROOT", root.join("ws"))
        .env(
            "HH_LOCAL_BASE_URL",
            std::env::var("HH_LOCAL_BASE_URL").unwrap(),
        )
        .env("HH_LOCAL_MODEL", std::env::var("HH_LOCAL_MODEL").unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("spawn script(1) pty for hh acp");
    // The pty settles before the first frame goes out: bytes written
    // while `sh -c` is still reaching `stty -echo` echo back through
    // `script`'s stdout — the client would read its own request as an
    // inbound frame, answer it `-32601`, and that reply echoes back as
    // the (forged) response. A bounded settle keeps the first write
    // behind the tty's cooked-mode setup.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let transport = StdioSessionTransport::new(
        BufReader::new(child.stdout.take().unwrap()),
        child.stdin.take().unwrap(),
    );
    Some(PtyAcp {
        child,
        transport,
        err_log,
    })
}

// ── the leg ─────────────────────────────────────────────────────────

/// Drive the ACP exchange through an in-repo `AcpClient` over the pty
/// transport. Returns `(session_id, updates, stop_reason)` — the
/// caller re-opens the store for verify/replay.
fn drive_in_repo(
    client: &mut AcpClient<
        StdioSessionTransport<BufReader<std::process::ChildStdout>, std::process::ChildStdin>,
    >,
) -> (String, String) {
    let binding = client
        .attach_session(
            &Json::obj([("request_target", Json::str("l46a-acp"))]),
            Some(AcpEra::V2),
        )
        .unwrap_or_else(|e| panic!("initialize/attach: {e:?}"));
    println!(
        "ACP_BINDING: era={:?} version={}",
        binding.era, binding.negotiated_version
    );
    let news = client
        .new_session(&Json::obj([
            ("cwd", Json::str(".")),
            ("mcpServers", Json::Arr(vec![])),
        ]))
        .expect("session/new");
    let sid = news
        .get("sessionId")
        .and_then(Json::as_str)
        .expect("sessionId")
        .to_string();
    println!("ACP_SESSION: {sid}");

    let mut pi = StaticPi::new(PermissionOutcome::Allow { option_id: None });
    let outcome = client
        .prompt(
            &sid,
            &Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                ("text", Json::str(REFERENCE_TASK)),
            ])]),
            &mut pi,
        )
        .expect("session/prompt");
    println!(
        "ACP_PROMPT: stop={} updates={}",
        outcome.stop_reason,
        outcome.updates.len()
    );
    (sid, outcome.stop_reason)
}

/// `verify` + `replay{deterministic}` on the run's durable prefix — a
/// fresh `hh-kernel serve` on the same store (cross-process read, never
/// the serving channel's memory). `read` pages the prefix; model.* row
/// count must not grow across the replay (the offline proof).
fn verify_replay(store: &Path, ws: &Path, run_id: &str) {
    let mut cmd = Command::new(bin("hh-kernel"));
    cmd.env("HH_STORE_ROOT", store)
        .env("HH_WORKSPACE_ROOT", ws)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("spawn hh-kernel serve (verify)");
    let mut client = KernelClient::new(
        BufReader::new(child.stdout.take().unwrap()),
        child.stdin.take().unwrap(),
    );
    let mut call =
        |m: &str, p: Json| -> Json { client.call(m, p).unwrap_or_else(|e| panic!("{m}: {e:?}")) };
    call("hello", hello_params());
    let s = call(
        "open_session",
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id.to_string())),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            ("idempotency_key", Json::str(format!("k-attach-{run_id}"))),
        ]),
    );
    let sess = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut events = Vec::new();
    let mut from = 0i64;
    loop {
        let page = call(
            "read",
            Json::obj([
                ("session_id", Json::str(sess.clone())),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(from))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
            ]),
        );
        match page.get("events").cloned() {
            Some(Json::Arr(evs)) => {
                let n = evs.len();
                events.extend(evs);
                if n < 256 {
                    break;
                }
                from += n as i64;
            }
            _ => break,
        }
    }
    fn class(e: &Json) -> &str {
        e.get("class").and_then(Json::as_str).unwrap_or("")
    }
    // `security.permission.requested` is §5g.7 §3's ephemeral prompt
    // rendering (subscriber frames only — `read` never returns it); the
    // durable owed-decision record is `security.permission.pending`, and
    // the wire round-trip's verdict is `security.permission.decided`.
    for want in [
        "model.call.requested",
        "model.call.completed",
        "security.permission.pending",
        "security.permission.decided",
        "lifecycle.turn.finished",
    ] {
        assert!(
            events.iter().any(|e| class(e) == want),
            "{want} absent on {run_id} — the live ACP turn is incomplete"
        );
    }
    // The round-trip's allow: the decided row carries the granted
    // decision with `decider: human` — a `policy`/`kernel` decider would
    // mean the ask never crossed the ACP wire (never a fabricated
    // decision).
    let decided = events
        .iter()
        .find(|e| class(e) == "security.permission.decided")
        .unwrap();
    assert_eq!(
        decided
            .get("payload")
            .and_then(|p| p.get("decision"))
            .and_then(Json::as_str),
        Some("allow"),
        "permission decided verbatim: {}",
        decided.to_canonical_string()
    );
    assert_eq!(
        decided
            .get("payload")
            .and_then(|p| p.get("decider"))
            .and_then(Json::as_str),
        Some("human"),
        "the wire round-trip's decider: {}",
        decided.to_canonical_string()
    );
    let model_rows = events
        .iter()
        .filter(|e| class(e).starts_with("model."))
        .count();
    let v = call(
        "verify",
        Json::obj([("session_id", Json::str(sess.clone()))]),
    );
    assert_eq!(
        v.get("verdict").and_then(Json::as_str),
        Some("ok"),
        "verify verdict"
    );
    let rep = call(
        "replay",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            ("driver_mode", Json::str("deterministic")),
        ]),
    );
    let report = rep.get("report").expect("replay report");
    assert_eq!(
        report.get("mode").and_then(Json::as_str),
        Some("deterministic"),
        "replay mode: {}",
        rep.to_canonical_string()
    );
    assert_eq!(
        report.get("reasons").cloned().unwrap_or(Json::Null),
        Json::Arr(vec![]),
        "replay diverged: {}",
        rep.to_canonical_string()
    );
    let page = call(
        "read",
        Json::obj([
            ("session_id", Json::str(sess.clone())),
            (
                "cursor",
                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
            ),
            ("direction", Json::str("fwd")),
            ("limit", Json::Int(1024)),
        ]),
    );
    let model_after = page
        .get("events")
        .and_then(|e| match e {
            Json::Arr(v) => Some(v.iter().filter(|e| class(e).starts_with("model.")).count()),
            _ => None,
        })
        .unwrap_or(0);
    assert_eq!(
        model_rows, model_after,
        "replay minted provider rows — not offline"
    );
    println!("ACP_REPLAY: run={run_id} deterministic model_rows={model_rows}");
    drop(client);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh surface-acp"]
fn l46a_live_acp_stdio() {
    if !provisioned() {
        eprintln!("unprovided — skipped");
        return;
    }
    let root = test_dir("live");
    let store = root.join("store");
    let ws = root.join("ws");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::create_dir_all(&ws).unwrap();

    // 1) Admit the bound profile + its passing test report on the
    //    leg's store — `hh acp`'s `lab.serve` resolves the coordinate
    //    at compile time.
    admit_profile(&store, &ws);

    // 2) The served definition — the same hir/1 document the verb
    //    reads from the operand path.
    let def = root.join("def.json");
    std::fs::write(&def, document_json().to_canonical_string()).unwrap();

    // 3) Spawn `hh acp` on the pty — the official-SDK arm first when
    //    the leg populated a temp install, then the in-repo client on
    //    the same transport shape.
    let mut client_kind = "in-repo".to_string();
    if let Ok(sdk) = std::env::var("HH_LIVE_ACP_SDK") {
        let src = repo_root().join("scripts/live-capstone-acp.mjs");
        let dst = PathBuf::from(&sdk).join("live-capstone-acp.mjs");
        let _ = std::fs::copy(&src, &dst);
        let out = Command::new("node")
            .arg(&dst)
            .arg(bin("hh"))
            .arg(&def)
            .arg(profile_coordinate())
            .arg(&root)
            .arg(PROBE_CAP)
            .env("HH_KERNEL_CMD", bin("hh-kernel"))
            .env("HH_STORE_ROOT", &store)
            .env("HH_WORKSPACE_ROOT", &ws)
            .env(
                "HH_LOCAL_BASE_URL",
                std::env::var("HH_LOCAL_BASE_URL").unwrap(),
            )
            .env("HH_LOCAL_MODEL", std::env::var("HH_LOCAL_MODEL").unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("node driver spawn");
        let official_out = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        print!("{official_out}");
        if out.status.success() {
            client_kind = "official-sdk".to_string();
        }
    }

    if client_kind == "in-repo" {
        let Some(mut pty) = spawn_acp(&def, &profile_coordinate(), &root) else {
            eprintln!("skipped — no pty transport for hh acp");
            let _ = std::fs::remove_dir_all(&root);
            return;
        };
        let mut client = AcpClient::new(pty.transport);
        let (sid, stop) = drive_in_repo(&mut client);
        // The `_hh/account` + `_hh/ledger/read` v2 extensions ride the
        // same channel — exercised here; the run id itself comes off
        // the durable store below (one agent run on the leg's store).
        let account = client
            .account()
            .unwrap_or_else(|e| panic!("account: {e:?}"));
        println!("ACP_ACCOUNT: {}", account.to_canonical_string());
        let tail = client
            .ledger_read(0)
            .unwrap_or_else(|e| panic!("_hh/ledger/read: {e:?}"));
        println!(
            "ACP_LEDGER_READ: {} bytes",
            tail.to_canonical_string().len()
        );
        let _ = client.close(&sid);
        drop(client);
        println!("ACP_SESSION_DONE: {sid} stop={stop}");
        let _ = pty.child.kill();
        let _ = pty.child.wait();
        if pty.err_log.exists() {
            if let Ok(tail) = std::fs::read_to_string(&pty.err_log) {
                for line in tail.lines().take(8) {
                    println!("ACP_ERR: {line}");
                }
            }
        }
        println!("ACP_CLIENT: in-repo");
    } else {
        println!("ACP_CLIENT: official-sdk");
    }

    // The run id comes off the durable store whichever client ran —
    // one agent run on the leg's store.
    let ledger = hharness_ledger::store::Store::open(&store).expect("open store");
    let run_id = ledger
        .run_ids()
        .into_iter()
        .find(|r| {
            ledger
                .manifest(r)
                .map(|m| matches!(m.run_kind, hharness_ledger::manifest::RunKind::Agent))
                .unwrap_or(false)
        })
        .unwrap_or_default();
    assert!(!run_id.is_empty(), "no agent run on the leg's store");
    println!("ACP_RUN: {run_id} client={client_kind}");
    verify_replay(&store, &root.join("ws-verify"), &run_id);
    let _ = std::fs::remove_dir_all(&root);
}
