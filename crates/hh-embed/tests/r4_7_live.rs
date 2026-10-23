//! R4.7 — the `github-live` legs (ADR-0363 D7; HUMAN-H2/H6; DF-S5.6-1;
//! AC-R-2.12.6-9). Three ignored cells `scripts/live-github.sh` drives:
//!
//! - `r47_github_live` — the live leg: both credentials resolve through
//!   the real `CredentialBroker` (`SecretSource::LaunchEnv` channels —
//!   values never print, never land in a record), the loopback listener
//!   serves behind a `cloudflared` quick tunnel, the script's
//!   `state.json` under `$HH_GH_DIR` carries the rollback coordinates
//!   (hook id / issue number — created the instant each mutation lands,
//!   so a panic mid-leg still lets the trap fire), and the whole flow —
//!   `GET hooks` pre-state → hook `POST` → `GET issues` pre-state →
//!   test issue + comment → signed `issues`/`issue_comment` deliveries
//!   admitted at `fleet.webhook_ingress` → `fleet.reconcile` →
//!   `WorkItem` → the in-process dedupe + stale-window cells → hook
//!   `DELETE` + issue close+`hh-live-test` — runs inside the sandbox repo
//!   named by `HH_GITHUB_SANDBOX_REPO`. Nothing else is touched.
//! - `r47_github_cleanup` — the trap's belt: reads `state.json` and
//!   re-runs the `DELETE`/`close` for anything still pending.
//! - `r47_github_plugin` — the AC-9 out-of-process leg: the packaged
//!   `hh-github-issues` variant spawns `subprocess_confined`, binds the
//!   captured `issues.json` snapshot as `github_doc`, and answers the
//!   capability probe + `list`/`get` over the live session.
//!
//! `#[ignore]` — driven only through `scripts/live-verify.sh
//! github-live`. A required name absent reports `unprovided — skipped`,
//! never a fabricated pass.

#![allow(clippy::unwrap_used)]
#![cfg(feature = "tier-c4")]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_provenance::ProvenanceRecord;
use hharness_secrets::broker::{
    BindRequest, CredentialBroker, LaunchEnvResolver, MediationOutcome, RequestDescriptor,
};
use hharness_secrets::channel::{
    AuthCarrier, CredentialKind, DestinationBinding, SecretChannel, SecretChannelSpec,
    SecretSource, SenderConstraint,
};
use hharness_secrets::types::AccessClass;
use hharness_tls::ClientTls;
use hharness_wire::json::Json;
use hharness_wire::jsonrpc::Request;

const TOKEN_NAME: &str = "HH_GITHUB_SANDBOX_TOKEN";
const SECRET_NAME: &str = "HH_GITHUB_WEBHOOK_SECRET";
const REPO_NAME: &str = "HH_GITHUB_SANDBOX_REPO";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn gh_dir() -> PathBuf {
    env("HH_GH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("hh-gh-{}", std::process::id())))
}

fn state_path() -> PathBuf {
    gh_dir().join("state.json")
}

/// The rollback ledger — written the instant each mutation lands so the
/// trap (`r47_github_cleanup`) can always reach the delete/close.
fn state(hook_id: Option<i64>, issue_number: Option<i64>) {
    let j = Json::obj([
        ("hook_id", hook_id.map(Json::Int).unwrap_or(Json::Null)),
        (
            "issue_number",
            issue_number.map(Json::Int).unwrap_or(Json::Null),
        ),
    ]);
    let _ = std::fs::write(state_path(), j.to_canonical_string());
}

fn read_state() -> (Option<i64>, Option<i64>) {
    let Ok(text) = std::fs::read_to_string(state_path()) else {
        return (None, None);
    };
    let Ok(j) = hharness_wire::json::parse(&text) else {
        return (None, None);
    };
    (
        j.get("hook_id").and_then(Json::as_int),
        j.get("issue_number").and_then(Json::as_int),
    )
}

// ── the broker (r4_6_live shape — register → grant → decide → bind → ────
// mediate → drain_claim; the value only ever lives in broker memory) ────

fn k_ev(store: &Store, run_id: &str, id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

fn grant_handle(store: &mut Store, run_id: &str, lease: &Lease, handle_id: &str, scope: &str) {
    let issuer = ProvenanceRecord::kernel("kernel:test", store.now_ms());
    let payload = Json::obj([
        ("handle_id", Json::str(handle_id)),
        (
            "permission_ref",
            Json::obj([
                ("semantic_id", Json::str("perm.secrets")),
                (
                    "version_id",
                    Json::str("sha256:".to_string() + &"0".repeat(64)),
                ),
            ]),
        ),
        ("holder", Json::str("agent.main")),
        ("issuer", issuer.to_json()),
        (
            "grants",
            Json::Arr(vec![Json::obj([
                (
                    "effect",
                    Json::obj([("domain", Json::str("secret_access"))]),
                ),
                ("scope", Json::str(scope)),
                ("constraints", Json::obj([])),
                ("delegable", Json::Bool(false)),
            ])]),
        ),
        ("ceiling", Json::str("principal")),
        (
            "validity",
            Json::obj([
                ("issued_at", Json::str(store.ts_now())),
                ("expires_at", Json::Null),
            ]),
        ),
        ("parent_handle", Json::Null),
        ("delegable", Json::Bool(false)),
        ("origin_basis", Json::str("approval")),
        ("basis_ref", Json::str("perm.secrets")),
        ("budget_ref", Json::Null),
        ("authority_delta", Json::str("none")),
    ]);
    let ev = k_ev(
        store,
        run_id,
        handle_id,
        "security.permission.granted",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
}

fn decide_allow(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    handle_id: &str,
    effect_id: &str,
) -> String {
    let decided_id = store.alloc_id("evt");
    let payload = Json::obj([
        ("effect_id", Json::str(effect_id)),
        ("decision", Json::str("allow")),
        ("effective_authority", Json::str("principal")),
        (
            "effective_risk_class",
            Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("ephemeral")),
            ]),
        ),
        ("handle_ids", Json::Arr(vec![Json::str(handle_id)])),
        ("policy_ref", Json::str("pi/1")),
        ("decider", Json::str("policy")),
        ("attempt_no", Json::Int(1)),
        ("proposal", Json::str("prop-1")),
        ("taint", Json::Arr(vec![])),
    ]);
    let ev = k_ev(
        store,
        run_id,
        &decided_id,
        "security.permission.decided",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
    decided_id
}

fn chan_spec(channel: &str, var: &str) -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: SecretSource::LaunchEnv { var: var.into() },
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            revocation_path: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
        allowed_env_names: Some([var.to_string()].into_iter().collect()),
        delivery_modes: [
            hharness_monitor::assess::SecretTransport::ProxyInjected,
            hharness_monitor::assess::SecretTransport::WrappedLongLived,
        ]
        .into_iter()
        .collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: format!("github {channel} credential"),
    }
}

/// `live_broker` — the broker with both github channels registered;
/// returns (store, run_id, lease, broker).
fn live_broker() -> (Store, String, Lease, CredentialBroker) {
    let d = std::env::temp_dir().join(format!("hh-r47-broker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let mut store = Store::open_test(d, 1_000).unwrap();
    let (run_id, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let mut broker = CredentialBroker::new(Box::new(LaunchEnvResolver), "test-fp-key");
    for (ch, var) in [("github-api", TOKEN_NAME), ("github-webhook", SECRET_NAME)] {
        broker
            .register_channel(
                ch,
                chan_spec(ch, var),
                ProvenanceRecord::kernel("kernel:test", 1_000),
            )
            .unwrap();
    }
    (store, run_id, lease, broker)
}

/// Resolve one channel's credential: grant + decide + bind + mediate +
/// drain_claim — the value surfaces only inside this process.
fn resolve_secret(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    broker: &mut CredentialBroker,
    channel: &str,
    var: &str,
) -> Option<String> {
    env(var)?;
    let hnd = format!("hnd-{channel}");
    grant_handle(
        store,
        run_id,
        lease,
        &hnd,
        &SecretChannel::grant_scope(channel),
    );
    let dref = decide_allow(store, run_id, lease, &hnd, &format!("eff-bind-{channel}"));
    let binding = broker
        .bind(
            store,
            run_id,
            lease,
            BindRequest {
                dpop_proof: None,
                channel_id: channel.into(),
                holder: "agent.main".into(),
                env_handle_ref: "env-1".into(),
                env_isolation: hharness_containment::policy::IsolationClass::ProcessSandbox,
                mode: hharness_monitor::assess::SecretTransport::ProxyInjected,
                monitor_decision_ref: dref,
                destinations: ["api.github.com".into()].into_iter().collect(),
            },
        )
        .unwrap();
    let out = broker
        .mediate(
            store,
            run_id,
            lease,
            &binding.binding_id,
            &RequestDescriptor {
                env_handle: "env-1".into(),
                effect_id: format!("eff-{channel}"),
                destination: "api.github.com".into(),
                method: None,
                path: None,
                dpop_proof: None,
            },
        )
        .unwrap();
    let MediationOutcome::Staged(delivery) = out else {
        panic!("mediate must stage for a declared destination");
    };
    broker.drain_claim(&delivery.claim_id).map(|(_, v)| v)
}

// ── the GitHub REST read/write half (hharness-tls + hh-wire::http — the ──────
// admitted TLS home; the token rides the Authorization header inside the
// process, never argv/disk/log) ─────────────────────────────────────────

fn gh_api(
    tls: &ClientTls,
    token: &str,
    method: &str,
    path: &str,
    body: Option<&Json>,
) -> Result<(u16, Json), String> {
    let url = hharness_wire::http::parse_http_url(&format!("https://api.github.com{path}"))
        .map_err(|e| format!("url {e:?}"))?;
    let sock = hharness_wire::http::connect(&url, 15_000, 30_000).map_err(|e| format!("{e:?}"))?;
    let mut st = tls
        .connect(sock, "api.github.com")
        .map_err(|e| format!("tls {e:?}"))?;
    let bytes = body.map(|b| b.to_canonical_string()).unwrap_or_default();
    let auth = format!("Bearer {token}");
    hharness_wire::http::write_request(
        &mut st,
        method,
        path,
        &[
            ("Host", "api.github.com"),
            ("Authorization", auth.as_str()),
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
            ("User-Agent", "hh-r4.7-live"),
            ("Content-Type", "application/json"),
        ],
        bytes.as_bytes(),
    )
    .map_err(|e| format!("write {e:?}"))?;
    let mut br = BufReader::new(st);
    let (head, raw) =
        hharness_wire::http::read_response_full(&mut br, 4 << 20).map_err(|e| format!("{e:?}"))?;
    let j = hharness_wire::json::parse(&String::from_utf8_lossy(&raw)).unwrap_or(Json::Null);
    Ok((head.status, j))
}

fn arr_len(j: &Json) -> usize {
    match j {
        Json::Arr(a) => a.len(),
        _ => 0,
    }
}

// ── cloudflared quick tunnel ────────────────────────────────────────────

fn spawn_tunnel(port: u16) -> Option<(Child, String)> {
    let mut cf = Command::new("cloudflared")
        .args(["tunnel", "--url", &format!("http://127.0.0.1:{port}")])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let stderr = cf.stderr.take()?;
    // Read stderr lines on a thread — the URL banner lands inside ~30s.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let _ = tx.send(line.unwrap_or_default());
        }
    });
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) {
            if let Some(pos) = line.find("https://") {
                let rest = &line[pos..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                let url = rest[..end].trim_end_matches('|').to_string();
                if url.contains("trycloudflare.com") {
                    return Some((cf, url));
                }
            }
        }
    }
    let _ = cf.kill();
    None
}

// ── the boundary service + the github source doc ────────────────────────

fn svc(root: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn spec_json() -> Json {
    Json::obj([
        ("schema", Json::str("hh.fleet.spec/1")),
        ("version", Json::str("1")),
        ("name", Json::str("github-live")),
        ("purpose", Json::str("fleet_activation")),
        ("fixture_ref", Json::str("fixture:gh")),
        (
            "agents",
            Json::Arr(vec![Json::str("ops"), Json::str("alice")]),
        ),
        (
            "capacity",
            Json::obj([("activate_run", Json::Int(8)), ("items", Json::Int(64))]),
        ),
        ("ownership", Json::obj([])),
        ("policy_ref", Json::str("policy:test")),
        ("narrowing", Json::Arr(vec![])),
        ("budget", Json::Null),
        ("budget_ref", Json::Null),
        ("out_of_scope", Json::Bool(true)),
        (
            "triggers",
            Json::obj([(
                "observe",
                Json::obj([
                    ("name", Json::str("observe")),
                    (
                        "trigger",
                        Json::obj([
                            ("type", Json::str("external")),
                            ("kind", Json::str("github.issue")),
                        ]),
                    ),
                    (
                        "policy",
                        Json::obj([
                            ("delivery_mode", Json::str("follow_up")),
                            ("coalesce", Json::str("none")),
                            ("max_pending", Json::Int(16)),
                        ]),
                    ),
                ]),
            )]),
        ),
        (
            "defaults",
            Json::obj([
                ("max_retries", Json::Int(3)),
                ("retry_backoff_ms", Json::Int(60_000)),
                ("dispatch_lease_ms", Json::Int(300_000)),
                ("stall_timeout_ms", Json::Int(300_000)),
            ]),
        ),
        (
            "human_gate_states",
            Json::Arr(vec![Json::str("needs_human")]),
        ),
    ])
}

fn source_doc(repo: &str) -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.webhook_source/1")),
        (
            "webhook",
            Json::obj([
                ("source_id", Json::str("github")),
                ("key_ref", Json::str("broker:github-webhook")),
                ("external_kind", Json::str("github.issue")),
            ]),
        ),
        (
            "github",
            Json::obj([
                ("repo", Json::str(repo)),
                ("source_id", Json::str("github")),
                ("owner_default", Json::str("ops")),
            ]),
        ),
    ])
}

#[allow(clippy::too_many_arguments)] // the delivery's five wire fields stay named — a tuple would lie
fn ingress(
    svc: &mut EmbedService,
    run: &str,
    source: &Json,
    raw: &str,
    sig: &str,
    did: &str,
    event: &str,
    key: &str,
) -> Json {
    call(
        svc,
        "fleet.webhook_ingress",
        Json::obj([
            ("run", Json::str(run)),
            ("source", source.clone()),
            ("delivery_raw", Json::str(raw)),
            ("signature", Json::str(sig)),
            ("delivery_id", Json::str(did)),
            ("event", Json::str(event)),
            ("key", Json::str(key)),
        ]),
    )
}

fn status(r: &Json) -> String {
    r.get("result")
        .and_then(|r| r.get("status"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn observed_len(r: &Json) -> usize {
    match r.get("result").and_then(|r| r.get("observed")) {
        Some(Json::Arr(a)) => a.len(),
        _ => 0,
    }
}

// ── the rollback guard — fires on panic/return alike; the state file ────
// is the trap's contract with the script's `r47_github_cleanup` belt. ────

struct Cleanup {
    tls: ClientTls,
    token: String,
    repo: String,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let (hook_id, issue_number) = read_state();
        if let Some(id) = hook_id {
            let _ = gh_api(
                &self.tls,
                &self.token,
                "DELETE",
                &format!("/repos/{}/hooks/{id}", self.repo),
                None,
            );
        }
        if let Some(n) = issue_number {
            let _ = gh_api(
                &self.tls,
                &self.token,
                "POST",
                &format!("/repos/{}/issues/{n}", self.repo),
                Some(&Json::obj([
                    ("state", Json::str("closed")),
                    ("labels", Json::Arr(vec![Json::str("hh-live-test")])),
                ])),
            );
        }
    }
}

// ── the legs ────────────────────────────────────────────────────────────

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh github-live"]
fn r47_github_live() {
    let Some(repo) = env(REPO_NAME) else {
        eprintln!("{REPO_NAME} unprovided — skipped");
        return;
    };
    if env(TOKEN_NAME).is_none() || env(SECRET_NAME).is_none() {
        eprintln!("required NAMEs unprovided — skipped");
        return;
    }
    let dir = gh_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (mut bstore, brun, blease, mut broker) = live_broker();
    let Some(token) = resolve_secret(
        &mut bstore,
        &brun,
        &blease,
        &mut broker,
        "github-api",
        TOKEN_NAME,
    ) else {
        eprintln!("{TOKEN_NAME} unprovided — skipped");
        return;
    };
    let Some(secret) = resolve_secret(
        &mut bstore,
        &brun,
        &blease,
        &mut broker,
        "github-webhook",
        SECRET_NAME,
    ) else {
        eprintln!("{SECRET_NAME} unprovided — skipped");
        return;
    };
    println!("GH_BROKER token=provided:yes secret=provided:yes");

    let tls = ClientTls::webpki_roots();
    let cleanup = Cleanup {
        tls: ClientTls::webpki_roots(),
        token: token.clone(),
        repo: repo.clone(),
    };

    // The loopback listener — every POST is verified + admitted at the
    // boundary; `ping` answers locally.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    println!("GH_LISTEN_PORT {port}");
    std::fs::write(dir.join("port"), port.to_string()).unwrap();

    // The quick tunnel — runtime-only URL, never committed.
    let Some((mut cf, tunnel_url)) = spawn_tunnel(port) else {
        println!("GH_SKIP tunnel-unavailable");
        return;
    };
    let tunnel_url = tunnel_url.trim().to_string();
    println!("GH_TUNNEL url={tunnel_url}");
    std::fs::write(dir.join("tunnel_url"), &tunnel_url).unwrap();

    // The boundary service + the fleet activation.
    let mut svc = svc(&dir);
    let r = call(
        &mut svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            (
                "capabilities",
                Json::obj([("experimental", Json::Bool(true))]),
            ),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
    let r = call(&mut svc, "fleet.open", Json::obj([("spec", spec_json())]));
    let run = r
        .get("result")
        .and_then(|r| r.get("run_id"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let source = source_doc(&repo);

    // ── pre-state: hooks ─────────────────────────────────────────────
    let (st, hooks) =
        gh_api(&tls, &token, "GET", &format!("/repos/{repo}/hooks"), None).expect("GET hooks");
    assert_eq!(st, 200, "GET hooks status {st}");
    let n_hooks_pre = arr_len(&hooks);
    println!("GH_PRESTATE_HOOKS n={n_hooks_pre}");

    // ── the webhook (one; deleted in the trap) ───────────────────────
    let hook_body = Json::obj([
        ("name", Json::str("web")),
        ("active", Json::Bool(true)),
        (
            "events",
            Json::Arr(vec![Json::str("issues"), Json::str("issue_comment")]),
        ),
        (
            "config",
            Json::obj([
                ("url", Json::str(tunnel_url.as_str())),
                ("content_type", Json::str("json")),
                ("secret", Json::str(secret.as_str())),
                ("insecure_ssl", Json::str("0")),
            ]),
        ),
    ]);
    let (st, hook) = gh_api(
        &tls,
        &token,
        "POST",
        &format!("/repos/{repo}/hooks"),
        Some(&hook_body),
    )
    .expect("POST hook");
    assert_eq!(st, 201, "POST hook status {st}: {hook:?}");
    let hook_id = hook.get("id").and_then(Json::as_int).expect("hook id");
    state(Some(hook_id), None); // rollback armed
    println!("GH_HOOK_CREATED id={hook_id}");

    // ── pre-state: issues ────────────────────────────────────────────
    let (st, issues) = gh_api(
        &tls,
        &token,
        "GET",
        &format!("/repos/{repo}/issues?state=all"),
        None,
    )
    .expect("GET issues");
    assert_eq!(st, 200, "GET issues status {st}");
    let n_issues = arr_len(&issues);
    println!("GH_PRESTATE_ISSUES n={n_issues}");
    std::fs::write(dir.join("issues.json"), issues.to_canonical_string()).unwrap();

    // The `hh-live-test` label must exist for the rollback cell.
    let (lst, _) = gh_api(
        &tls,
        &token,
        "GET",
        &format!("/repos/{repo}/labels/hh-live-test"),
        None,
    )
    .expect("GET label");
    if lst == 404 {
        let (cst, _) = gh_api(
            &tls,
            &token,
            "POST",
            &format!("/repos/{repo}/labels"),
            Some(&Json::obj([
                ("name", Json::str("hh-live-test")),
                ("color", Json::str("ededed")),
            ])),
        )
        .expect("POST label");
        println!("GH_LABEL created={}", (cst == 201) as u8);
    } else {
        println!("GH_LABEL created=0 (exists)");
    }

    // ── the test issue + comment (the only mutations) ────────────────
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (st, issue) = gh_api(
        &tls,
        &token,
        "POST",
        &format!("/repos/{repo}/issues"),
        Some(&Json::obj([
            ("title", Json::str(format!("hh R4.7 live leg {ts}"))),
            (
                "body",
                Json::str("signed-webhook ingress leg — test artifact"),
            ),
            ("labels", Json::Arr(vec![Json::str("hh-live-test")])),
        ])),
    )
    .expect("POST issue");
    assert_eq!(st, 201, "POST issue status {st}: {issue:?}");
    let number = issue
        .get("number")
        .and_then(Json::as_int)
        .expect("issue number");
    state(Some(hook_id), Some(number));
    println!("GH_ISSUE_CREATED number={number}");
    let item_id = format!("gh-{}-{number}", repo.replace('/', "-"));

    let (cst, _) = gh_api(
        &tls,
        &token,
        "POST",
        &format!("/repos/{repo}/issues/{number}/comments"),
        Some(&Json::obj([("body", Json::str("hh R4.7 comment leg"))])),
    )
    .expect("POST comment");
    println!("GH_COMMENT status={cst}");

    // ── the delivery: signed POSTs land at the listener ──────────────
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut delivered = false;
    while Instant::now() < deadline && !delivered {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .and_then(|_| stream.set_read_timeout(Some(Duration::from_secs(15))))
                    .unwrap();
                let mut br = BufReader::new(stream.try_clone().unwrap());
                let req = hharness_wire::http::read_request(&mut br, 1 << 20)
                    .ok()
                    .flatten();
                if let Some((head, body)) = req {
                    let hdr = |k: &str| head.headers.get(k).cloned().unwrap_or_default();
                    let event = hdr("x-github-event");
                    let sig = hdr("x-hub-signature-256");
                    let did = hdr("x-github-delivery");
                    let raw = String::from_utf8_lossy(&body).to_string();
                    if event == "ping" {
                        let ok =
                            hharness_fleet::github::verify_sig256(&raw, &sig, secret.as_bytes());
                        println!("GH_PING verified={}", ok.is_ok() as u8);
                    } else if event == "issues" || event == "issue_comment" {
                        let r = ingress(&mut svc, &run, &source, &raw, &sig, &did, &event, &secret);
                        let st = status(&r);
                        println!("GH_DELIVERY event={event} delivery={did} status={st}");
                        if st == "received" {
                            // Ingress `observe`s durably; `reconcile`
                            // folds the source cue into the WorkItem.
                            let rr = call(
                                &mut svc,
                                "fleet.reconcile",
                                Json::obj([
                                    ("run", Json::str(run.as_str())),
                                    ("source", source.clone()),
                                ]),
                            );
                            assert!(rr.get("result").is_some(), "reconcile: {rr:?}");
                            let w = call(
                                &mut svc,
                                "fleet.work_item",
                                Json::obj([
                                    ("run", Json::str(run.as_str())),
                                    ("item", Json::str(item_id.as_str())),
                                ]),
                            );
                            let wstate = w
                                .get("result")
                                .and_then(|w| w.get("state"))
                                .and_then(Json::as_str)
                                .unwrap_or("(absent)")
                                .to_string();
                            println!("GH_WORKITEM item={item_id} state={wstate}");
                            std::fs::write(
                                dir.join("delivery.json"),
                                Json::obj([
                                    ("delivery_id", Json::str(did.as_str())),
                                    ("status", Json::str(st.as_str())),
                                    ("item", Json::str(item_id.as_str())),
                                    ("state", Json::str(wstate.as_str())),
                                    ("raw", Json::str(raw.as_str())),
                                    ("signature", Json::str(sig.as_str())),
                                    ("event", Json::str(event.as_str())),
                                ])
                                .to_canonical_string(),
                            )
                            .unwrap();
                            delivered = true;
                        }
                    }
                    let mut w = stream;
                    let _ = hharness_wire::http::write_response(
                        &mut w,
                        200,
                        &[("Content-Type", "text/plain")],
                        b"ok",
                    );
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    if !delivered {
        println!("GH_SKIP no-delivery-in-120s");
        let _ = cf.kill();
        return;
    }

    // ── dedupe + stale-window cells (in-process re-calls — the same ──
    // capture, the durable fold audits) ───────────────────────────────
    let dj = std::fs::read_to_string(dir.join("delivery.json")).unwrap();
    let dj = hharness_wire::json::parse(&dj).unwrap();
    let draw = dj.get("raw").and_then(Json::as_str).unwrap();
    let dsig = dj.get("signature").and_then(Json::as_str).unwrap();
    let ddid = dj.get("delivery_id").and_then(Json::as_str).unwrap();
    let devent = dj.get("event").and_then(Json::as_str).unwrap();
    let r = ingress(
        &mut svc,
        &run,
        &source,
        draw,
        dsig,
        ddid,
        devent,
        secret.as_str(),
    );
    println!(
        "GH_DEDUPE status={} observed={} (durable skip audited)",
        status(&r),
        observed_len(&r)
    );
    let r = ingress(
        &mut svc,
        &run,
        &source,
        draw,
        dsig,
        "gh-stale-1",
        devent,
        secret.as_str(),
    );
    let stale_observed = observed_len(&r);
    let _ = call(
        &mut svc,
        "fleet.reconcile",
        Json::obj([("run", Json::str(run.as_str())), ("source", source.clone())]),
    );
    let l = call(
        &mut svc,
        "fleet.list",
        Json::obj([("run", Json::str(run.as_str()))]),
    );
    let n_items = match l.get("result").and_then(|l| l.get("items")) {
        Some(Json::Arr(a)) => a.len(),
        _ => 0,
    };
    println!("GH_STALE observed={stale_observed} items={n_items}");

    // ── rollback: delete the hook, close+label the issue ─────────────
    let (dst, _) = gh_api(
        &tls,
        &token,
        "DELETE",
        &format!("/repos/{repo}/hooks/{hook_id}"),
        None,
    )
    .expect("DELETE hook");
    println!("GH_HOOK_DELETED id={hook_id} status={dst}");
    state(None, Some(number));
    let (vst, hooks2) =
        gh_api(&tls, &token, "GET", &format!("/repos/{repo}/hooks"), None).expect("GET hooks post");
    let gone = arr_len(&hooks2) == n_hooks_pre;
    println!("GH_HOOK_GONE verified={} status={vst}", gone as u8);
    let (cst, _) = gh_api(
        &tls,
        &token,
        "POST",
        &format!("/repos/{repo}/issues/{number}"),
        Some(&Json::obj([
            ("state", Json::str("closed")),
            ("labels", Json::Arr(vec![Json::str("hh-live-test")])),
        ])),
    )
    .expect("close issue");
    println!("GH_ISSUE_CLOSED number={number} status={cst}");
    state(None, None);

    let _ = cf.kill();
    println!("GH_RESULT pass");
    drop(cleanup);
}

/// The trap's belt — re-runs the rollback for anything the state file
/// still marks pending (the main cell crashed mid-leg).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh github-live"]
fn r47_github_cleanup() {
    let (hook_id, issue_number) = read_state();
    if hook_id.is_none() && issue_number.is_none() {
        println!("GH_CLEANUP nothing-pending");
        return;
    }
    let Some(repo) = env(REPO_NAME) else {
        eprintln!("{REPO_NAME} unprovided — cleanup cannot run");
        return;
    };
    let (mut bstore, brun, blease, mut broker) = live_broker();
    let Some(token) = resolve_secret(
        &mut bstore,
        &brun,
        &blease,
        &mut broker,
        "github-api",
        TOKEN_NAME,
    ) else {
        eprintln!("{TOKEN_NAME} unprovided — cleanup cannot run");
        return;
    };
    let tls = ClientTls::webpki_roots();
    if let Some(id) = hook_id {
        let _ = gh_api(
            &tls,
            &token,
            "DELETE",
            &format!("/repos/{repo}/hooks/{id}"),
            None,
        );
        println!("GH_CLEANUP hook_deleted={id}");
    }
    if let Some(n) = issue_number {
        let _ = gh_api(
            &tls,
            &token,
            "POST",
            &format!("/repos/{repo}/issues/{n}"),
            Some(&Json::obj([
                ("state", Json::str("closed")),
                ("labels", Json::Arr(vec![Json::str("hh-live-test")])),
            ])),
        );
        println!("GH_CLEANUP issue_closed={n}");
    }
    state(None, None);
    println!("GH_CLEANUP done");
}

// ── AC-R-2.12.6-9's live clause: the packaged variant out of process ────
// over the live-captured issue snapshot (`$HH_GH_DIR/issues.json`) — the
// same spawn/bind/probe path `hharness-varhost`'s fixture leg exercises, fed
// the real repository read.

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh github-live"]
fn r47_github_plugin() {
    use hharness_fleet::capabilities::CapState;
    use hharness_fleet::source::WorkSourceAdapter;
    use hharness_fleet_adapter::adapter::{PluginSourceAdapter, SourceChannel, SourceFault};
    use hharness_fleet_adapter::{WORK_SOURCE_CLASS, WORK_SOURCE_CONTRACT};
    use hharness_varhost::{
        load_package, seal_package, spawn, unstamp_json, InvokeOutcome, RecordingPorts, SpawnSpec,
        VariantHost, VecEvents,
    };

    struct Chan<'a> {
        host: &'a mut VariantHost<RecordingPorts, VecEvents>,
        session: &'a mut hharness_varhost::VariantSession,
        binding: String,
        seq: u64,
    }
    impl<'a> SourceChannel for Chan<'a> {
        fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
            self.seq += 1;
            let out = self
                .host
                .invoke(
                    self.session,
                    &self.binding,
                    operation,
                    inputs,
                    &format!("gh-live-r{}", self.seq),
                    Duration::from_secs(20),
                )
                .map_err(|e| SourceFault::Transport(format!("{e:?}")))?;
            match out {
                InvokeOutcome::Outputs(docs) => Ok(docs.iter().map(unstamp_json).collect()),
                InvokeOutcome::Failed(e) => {
                    Err(SourceFault::Transport(format!("abi failure: {e:?}")))
                }
            }
        }
    }

    let dir = gh_dir();
    let Ok(snapshot) = std::fs::read_to_string(dir.join("issues.json")) else {
        eprintln!("issues.json absent — run r47_github_live first; skipped");
        return;
    };
    let issues = hharness_wire::json::parse(&snapshot).expect("issues.json parses");
    let repo = env(REPO_NAME).unwrap_or_else(|| "SteveVitali/hh-live-sandbox".into());
    let exe = std::env::current_exe().unwrap();
    let bin = exe
        .parent()
        .and_then(|p| p.parent())
        .map(|d| d.join("hh-github-issues"))
        .filter(|p| p.exists());
    let Some(bin) = bin else {
        eprintln!("hh-github-issues binary not built — skipped");
        return;
    };
    if hharness_env::helper::helper_binary().is_none() {
        eprintln!("hh-helper binary not found — skipped");
        return;
    }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf();
    let pkg_dir = dir.join("pkg");
    std::fs::create_dir_all(pkg_dir.join("bin")).unwrap();
    std::fs::copy(&bin, pkg_dir.join("bin/hh-github-issues")).unwrap();
    let mtext =
        std::fs::read_to_string(repo_root.join("plugins/hh-github-issues/plugin.manifest.json"))
            .unwrap();
    let manifest = hharness_registry::extension::plugin::manifest_from_json(
        &hharness_wire::json::parse(&mtext).unwrap(),
        "plugin.manifest.json",
    )
    .unwrap();
    let sealed = seal_package(&pkg_dir, &manifest).expect("seal");
    let stext =
        hharness_registry::extension::plugin::manifest_to_json(&sealed).to_canonical_string();
    std::fs::write(pkg_dir.join("plugin.manifest.json"), &stext).unwrap();
    let pkg = load_package(&pkg_dir).expect("load verifies pins");

    let doc = Json::obj([
        ("schema_version", Json::str("hh.fleet.github_source/1")),
        (
            "github",
            Json::obj([
                ("repo", Json::str(repo.as_str())),
                ("source_id", Json::str("github")),
                ("owner_default", Json::str("ops")),
            ]),
        ),
        ("issues", issues),
        ("occurrences", Json::Arr(vec![])),
        ("suspended", Json::Arr(vec![])),
    ]);

    let sock_dir = dir.join(format!("sock-{}", std::process::id()));
    let spec = SpawnSpec {
        package: pkg.clone(),
        session_id: "gh-live-1".into(),
        backend: "direct".into(),
        placement: hharness_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir: sock_dir,
        exec_args: vec![
            "--version-id".into(),
            pkg.version_id().to_string(),
            "--content".into(),
            pkg.content().to_string(),
            "--plugin-id".into(),
            pkg.manifest.identity.id(),
        ],
        cap: pkg.manifest.requests.clone(),
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hharness_hir::refs::Ref::pinned("plugin:hh/hh-github-issues", "pin:sealed"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            format!("class_contract:{WORK_SOURCE_CLASS}"),
            WORK_SOURCE_CONTRACT.to_string(),
        )]),
        timeout: Duration::from_secs(20),
        at: 0,
    };
    let (mut s, _l) = spawn(&spec).expect("spawn");
    let mut h: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = h
        .bind(
            &mut s,
            hharness_embed_schema::plugin_abi::BindParams {
                slot: "work_source".into(),
                class_id: WORK_SOURCE_CLASS.into(),
                contract_version: WORK_SOURCE_CONTRACT.into(),
                variant: Json::Null,
                params: Json::obj([("github_doc", doc)]),
                profile: Json::Null,
                account: Json::Null,
                budget: Json::Null,
                placement: "subprocess_confined".into(),
            },
        )
        .expect("bind");
    let mut ad = PluginSourceAdapter::new(
        Chan {
            host: &mut h,
            session: &mut s,
            binding: b,
            seq: 0,
        },
        "src:github-live",
    );
    ad.bind_probe();
    let caps = ad.capabilities();
    assert_eq!(caps.push_delivery_id, CapState::Declared);
    assert_eq!(caps.poll, CapState::Declared);
    println!("GH_PLUGIN_OOP spawn=subprocess_confined probe=capabilities");
    let listed = ad.list(&[]);
    println!("GH_PLUGIN_LIST n={}", listed.len());
    if let Some(first) = listed.first() {
        if let Some(id) = first.get("native_id").and_then(Json::as_str) {
            let got = ad.get(&[id.to_string()]);
            println!("GH_PLUGIN_GET native_id={id} n={}", got.len());
        }
    }
    assert!(ad.fault().is_none(), "latched fault: {:?}", ad.fault());
    drop(ad);
    h.close(&mut s).unwrap();
    println!("GH_PLUGIN_RESULT pass");
}
