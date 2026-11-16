//! `hh-egress-mediator` — the phase egress mediator binary (R6.15;
//! AR-05; REQ-XB-104/113/114/116; experiments-spec §3.2/§3.5). The
//! runner spawns it (R6.16); it is the sole writer of its own store
//! root and serves the slot's egress unix socket: HTTP forward +
//! CONNECT, every request decided by `decide_egress` and recorded
//! `security.egress.requested`/`decided` before any byte flows.
//!
//! ```text
//! hh-egress-mediator --store-root <dir> --socket <path>
//!     --policy <file> --env-handle <id> --effect-id <id>
//!     --participant <ref> [--run-id <id>] [--seed <hex>]
//!     [--resolve <host>=<ip>]...
//! ```
//!
//! The policy file is re-read per connection — a phase swap is the
//! host writing the next sealed layer's effective policy (each decided
//! row records `policy_version_id`). `--resolve` seeds the resolver
//! table (loopback fixtures; absent entries resolve via the system
//! resolver). The slot's phase effect token is minted internally and
//! never enters the cell (X-05).

use std::io::BufReader;
use std::net::IpAddr;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::exit;
use std::sync::{Arc, Condvar, Mutex};

use hharness_containment::egress::{ApprovalCache, EgressRequest};
use hharness_containment::policy::{ContainmentPolicy, EgressProtocol};
use hharness_env::egress::{
    EgressMediator, GateOutcome, LocalHttpTransport, MediatedOutcome, StaticResolver,
    SystemResolver,
};
use hharness_env::events::ScopeChain;
use hharness_env::tokens::TokenMinter;
use hharness_ledger::manifest::RunManifest;
use hharness_ledger::store::Store;
use hharness_secrets::{CredentialBroker, StaticVault};
use hharness_wire::http;
use hharness_wire::json;

fn usage() -> ! {
    eprintln!(
        "usage: hh-egress-mediator --store-root <dir> --socket <path> --policy <file> \
         --env-handle <id> --effect-id <id> --participant <ref> [--run-id <id>] \
         [--seed <hex>] [--resolve <host>=<ip>]..."
    );
    exit(2);
}

struct Cfg {
    store_root: PathBuf,
    socket: PathBuf,
    policy: PathBuf,
    env_handle: String,
    effect_id: String,
    participant: String,
    run_id: Option<String>,
    seed: [u8; 32],
    resolve: Vec<(String, IpAddr)>,
}

fn parse() -> Cfg {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |i: usize| args.get(i).cloned().unwrap_or_else(|| usage());
    let mut c = Cfg {
        store_root: PathBuf::new(),
        socket: PathBuf::new(),
        policy: PathBuf::new(),
        env_handle: String::new(),
        effect_id: String::new(),
        participant: String::new(),
        run_id: None,
        seed: [7u8; 32],
        resolve: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--store-root" => {
                c.store_root = PathBuf::from(get(i + 1));
                i += 2;
            }
            "--socket" => {
                c.socket = PathBuf::from(get(i + 1));
                i += 2;
            }
            "--policy" => {
                c.policy = PathBuf::from(get(i + 1));
                i += 2;
            }
            "--env-handle" => {
                c.env_handle = get(i + 1);
                i += 2;
            }
            "--effect-id" => {
                c.effect_id = get(i + 1);
                i += 2;
            }
            "--participant" => {
                c.participant = get(i + 1);
                i += 2;
            }
            "--run-id" => {
                c.run_id = Some(get(i + 1));
                i += 2;
            }
            "--seed" => {
                let h = get(i + 1);
                let b = hex32(&h).unwrap_or_else(|| usage());
                c.seed = b;
                i += 2;
            }
            "--resolve" => {
                let v = get(i + 1);
                let (h, ip) = v.split_once('=').unwrap_or_else(|| usage());
                c.resolve.push((
                    hharness_containment::policy::normalize_host(h),
                    ip.parse().unwrap_or_else(|_| usage()),
                ));
                i += 2;
            }
            _ => usage(),
        }
    }
    if c.env_handle.is_empty()
        || c.effect_id.is_empty()
        || c.participant.is_empty()
        || c.store_root.as_os_str().is_empty()
        || c.socket.as_os_str().is_empty()
        || c.policy.as_os_str().is_empty()
    {
        usage();
    }
    c
}

fn hex32(h: &str) -> Option<[u8; 32]> {
    let h = h.strip_prefix("sha256:").unwrap_or(h);
    if h.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn reply(stream: &mut impl std::io::Write, status: u16, body: &[u8]) {
    let _ = http::write_response(stream, status, &[], body);
}

/// The mediator's request scope: no turn/model_call/tool_call ids —
/// the standalone run opens only the slot's phase effect scope.
static EMPTY_CHAIN: ScopeChain = ScopeChain {
    turn_id: String::new(),
    model_call_id: String::new(),
    tool_call_id: String::new(),
};

/// Whether `effect_id` is already an opened effect scope in `run_id`
/// (an `action.effect.intended` row carrying it; the mediator never
/// emits the terminal classes that would close it).
fn effect_scope_open(store: &Store, run_id: &str, effect_id: &str) -> bool {
    store
        .events(run_id)
        .map(|evs| {
            evs.iter().any(|e| {
                e.class == "action.effect.intended"
                    && e.scope.effect_id.as_deref() == Some(effect_id)
            })
        })
        .unwrap_or(false)
}

fn sha256(bytes: &[u8]) -> String {
    hharness_wire::sha256::sha256_hex(bytes)
}

fn load_policy(path: &PathBuf) -> Result<ContainmentPolicy, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("policy read: {e}"))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let j = json::parse(&text).map_err(|e| format!("policy parse: {e}"))?;
    ContainmentPolicy::from_json(&j).map_err(|e| format!("policy decode: {e}"))
}

/// `MAX_IN_FLIGHT` — the concurrency bound (R6.45k; D8): each accepted
/// connection serves on its own thread, at most this many in flight —
/// excess connections wait in the listener backlog, never dropped.
const MAX_IN_FLIGHT: usize = 64;

/// `Slots` — the in-flight permit set: `acquire` blocks while the bound
/// is full; the returned guard releases on drop.
struct Slots {
    inner: Mutex<usize>,
    free: Condvar,
    max: usize,
}
struct Slot {
    slots: Arc<Slots>,
}
impl Drop for Slot {
    fn drop(&mut self) {
        let mut n = self.slots.inner.lock().unwrap_or_else(|p| p.into_inner());
        *n = n.saturating_sub(1);
        self.slots.free.notify_one();
    }
}
impl Slots {
    fn acquire(self: &Arc<Self>) -> Slot {
        let mut n = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        while *n >= self.max {
            n = self.free.wait(n).unwrap_or_else(|p| p.into_inner());
        }
        *n += 1;
        Slot {
            slots: Arc::clone(self),
        }
    }
}

fn main() {
    let c = parse();
    let store = Mutex::new(Store::open(&c.store_root).unwrap_or_else(|e| {
        eprintln!("hh-egress-mediator: store open: {e}");
        exit(3)
    }));
    let mut manifest = RunManifest::minimal(hharness_ledger::manifest::RunKind::Experiment);
    manifest.configuration_id = None;
    manifest.configuration_version_id = None;
    manifest.environment_ref = None;
    manifest.environment_version_id = None;
    manifest.participant_class = hharness_ledger::manifest::ParticipantClass::Native;
    manifest.lease_ttl.writer_ms = 3_600_000;
    let (run_id, lease) = {
        let mut store = store.lock().unwrap_or_else(|p| p.into_inner());
        match &c.run_id {
            Some(id) if store.has_run(id) => {
                let l = store
                    .acquire_writer("hh-egress-mediator", id, 3_600_000)
                    .unwrap_or_else(|e| {
                        eprintln!("hh-egress-mediator: resume lease: {e}");
                        exit(3)
                    });
                (id.clone(), l)
            }
            Some(id) => store
                .open_run_with_id(id, manifest, "hh-egress-mediator")
                .unwrap_or_else(|e| {
                    eprintln!("hh-egress-mediator: open_run: {e}");
                    exit(3)
                }),
            None => store
                .open_run(manifest, "hh-egress-mediator")
                .unwrap_or_else(|e| {
                    eprintln!("hh-egress-mediator: open_run: {e}");
                    exit(3)
                }),
        }
    };
    let _ = std::fs::remove_file(&c.socket);
    if let Some(d) = c.socket.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let listener = UnixListener::bind(&c.socket).unwrap_or_else(|e| {
        eprintln!("hh-egress-mediator: bind {}: {e}", c.socket.display());
        exit(2)
    });

    // The scope grammar (hh-ledger store `check_scopes`): every scope id
    // on an appended event must name an *opened* scope — and the chain
    // requires `tool_call ⇒ model_call ⇒ turn`. The mediator's own run
    // holds no turn/model_call/tool_call scopes: its egress rows carry
    // only the slot's phase effect scope — which the declared
    // `--effect-id` names and this run must therefore open with the
    // registered `opens_scope: effect` event (`action.effect.intended`)
    // before the first `security.egress.requested` can append. Without
    // the opener every append refused `ScopeNotOpen{effect_id}` (and a
    // fabricated turn/tool_call refused `ScopeNotOpen`/`SchemaViolation`)
    // — the Wc defect: 500s for every request, zero egress rows.
    if !effect_scope_open(
        &store.lock().unwrap_or_else(|p| p.into_inner()),
        &run_id,
        &c.effect_id,
    ) {
        let mut store_g = store.lock().unwrap_or_else(|p| p.into_inner());
        let minter = hharness_env::events::EventMinter::new(&*store_g, &run_id);
        let risk = hharness_ontology::risk::RiskClass::UNKNOWN;
        let ev = minter
            .mint_effect(
                "action.effect.intended",
                hharness_env::events::intended_payload(
                    &risk,
                    Some(&risk),
                    "hh-egress-mediator/1",
                    &format!("sha256:{}", sha256(c.effect_id.as_bytes())),
                    0,
                ),
                &c.effect_id,
                &EMPTY_CHAIN,
            )
            .unwrap_or_else(|e| {
                eprintln!("hh-egress-mediator: mint effect scope: {e}");
                exit(3)
            });
        store_g
            .append(&run_id, &lease, vec![ev])
            .unwrap_or_else(|e| {
                eprintln!("hh-egress-mediator: open effect scope: {e}");
                exit(3)
            });
        drop(store_g);
    }
    println!("ready socket={} run={run_id}", c.socket.display());
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    // The slot's phase effect token — minted internally, never in-cell.
    let mut minter = TokenMinter::new(c.seed);
    let token = minter.mint(&c.effect_id, 1, &c.env_handle);
    let resolver_tokens = minter.resolver();

    // D8 — one thread per accepted connection, bounded at MAX_IN_FLIGHT
    // (excess conns wait in the backlog). The store + lease are shared:
    // the store behind the Mutex — held only inside the mediator's
    // `with_store` critical sections (pre_wire's requested→decided and
    // the close/charge legs), never during byte pumping or the upstream
    // exchange; the lease is an immutable `&Lease`. Each connection
    // builds its own mediator (fresh broker + approval cache) and the
    // policy re-reads per connection, as before — never per request
    // inside the lock.
    let store = Arc::new(store);
    let lease = Arc::new(lease);
    let cfg = Arc::new(c);
    let token_str = Arc::new(token.token);
    let slots = Arc::new(Slots {
        inner: Mutex::new(0),
        free: Condvar::new(),
        max: MAX_IN_FLIGHT,
    });
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let _slot = slots.acquire();
        let store = Arc::clone(&store);
        let lease = Arc::clone(&lease);
        let c = Arc::clone(&cfg);
        let token = Arc::clone(&token_str);
        let tokens = resolver_tokens.clone();
        let run_id = run_id.clone();
        let _ = std::thread::Builder::new()
            .name("egress-conn".to_string())
            .spawn(move || {
                let _slot = _slot; // the permit lives for the conn's life
                let policy = match load_policy(&c.policy) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("hh-egress-mediator: {e}");
                        return;
                    }
                };
                let mut broker =
                    CredentialBroker::new(Box::new(StaticVault::default()), "mediator");
                let mut med = EgressMediator {
                    store: &*store,
                    run_id: run_id.clone(),
                    lease: &*lease,
                    tokens,
                    broker: &mut broker,
                    policy,
                    cache: ApprovalCache::default(),
                    budget_id: None,
                    participant_ref: c.participant.clone(),
                    resolver: make_resolver(&c),
                    transport: Box::new(LocalHttpTransport::default()),
                    inspect_hooks: std::collections::BTreeMap::new(),
                };
                serve_conn(stream, &mut med, &c, &token);
            });
    }
}

// `EgressResolver` isn't Clone — rebuild the resolver per connection so
// the declared `--resolve` pins actually apply (previously the
// StaticResolver table was built once and then discarded: the per-
// connection clone always fell back to `SystemResolver`, silently
// ignoring every pin).
fn make_resolver(c: &Cfg) -> Box<dyn hharness_env::egress::EgressResolver> {
    if c.resolve.is_empty() {
        return Box::new(SystemResolver);
    }
    let mut map = std::collections::BTreeMap::new();
    for (h, ip) in &c.resolve {
        map.insert(h.clone(), vec![*ip]);
    }
    Box::new(StaticResolver { map })
}

/// `err_status(e)` — the typed failure status (D12): a declared timeout
/// bound's lapse answers **504 Gateway Timeout**, everything else 502.
fn err_status(e: &hharness_env::egress::EgressError) -> u16 {
    match e {
        hharness_env::egress::EgressError::Timeout { .. } => 504,
        _ => 502,
    }
}

fn serve_conn(
    stream: std::os::unix::net::UnixStream,
    med: &mut EgressMediator<'_, &Mutex<Store>>,
    c: &Cfg,
    token: &str,
) {
    let mut stream = stream;
    let mut rd = BufReader::new(stream.try_clone().expect("unix clone"));
    let head = match http::read_request(&mut rd, 4 * 1024 * 1024) {
        Ok(Some(h)) => h,
        _ => return,
    };
    let (req, connect) = parse_request(&head, c, token);
    let Some(req) = req else {
        reply(&mut stream, 400, b"bad proxy request");
        return;
    };
    // The minted scope carries no turn/model_call/tool_call ids — the
    // mediator's run opens no such scopes and the grammar refuses an
    // unopened id (`ScopeNotOpen`) as well as a `tool_call` without its
    // `model_call` parent (`SchemaViolation`). `req.tool_call_id` still
    // rides the `security.egress.requested` payload member; the row's
    // only scope is the slot's phase effect (opened at startup).
    let chain = EMPTY_CHAIN.clone();
    let started = med.store.lock().unwrap_or_else(|p| p.into_inner()).now_ms();
    match med.gate(&req, &chain) {
        Ok(GateOutcome::Terminal(outcome)) => match outcome {
            MediatedOutcome::Refused { .. } | MediatedOutcome::RefusedCredential { .. } => {
                reply(&mut stream, 403, b"egress denied")
            }
            MediatedOutcome::Asked { .. } => reply(&mut stream, 403, b"egress pending"),
            _ => reply(&mut stream, 500, b"protocol"),
        },
        Ok(GateOutcome::Allowed(g)) => {
            if connect {
                match med.forward_tunnel(
                    &req,
                    &g.decision,
                    g.decided_by,
                    &g.effect_id,
                    g.started,
                    &chain,
                    &mut stream,
                ) {
                    Ok(_) => {}
                    // A Timeout lapses at dial — before the 200 — so the
                    // typed 504 answers cleanly; post-200 errors (emit)
                    // are logged, the client sees the close.
                    Err(e @ hharness_env::egress::EgressError::Timeout { .. }) => {
                        reply(&mut stream, 504, format!("{e:?}").as_bytes())
                    }
                    Err(e) => eprintln!("hh-egress-mediator: tunnel: {e:?}"),
                }
            } else {
                // The streaming forward (D13): the upstream head streams
                // through D10's response hygiene — exactly one framing
                // member (`Transfer-Encoding: chunked`) + `Connection:
                // close` — and the body re-chunks into the client with
                // bounded resident memory, never the 128 MiB buffer.
                match med.forward_stream(
                    &req,
                    &g.decision,
                    g.decided_by,
                    &g.effect_id,
                    g.started,
                    &chain,
                    &mut stream,
                ) {
                    Ok(MediatedOutcome::Forwarded { .. }) => {}
                    Ok(MediatedOutcome::Refused { .. })
                    | Ok(MediatedOutcome::RefusedCredential { .. }) => {
                        reply(&mut stream, 403, b"egress denied")
                    }
                    Ok(_) => reply(&mut stream, 500, b"protocol"),
                    Err(e) => reply(&mut stream, err_status(&e), format!("{e:?}").as_bytes()),
                }
            }
        }
        Err(e) => reply(&mut stream, 500, format!("{e:?}").as_bytes()),
    }
    let _ = started;
}

fn parse_request(
    head: &(hharness_wire::http::RequestHead, Vec<u8>),
    c: &Cfg,
    token: &str,
) -> (Option<EgressRequest>, bool) {
    let (h, body) = head;
    let base = |protocol, host: String, port: u16| EgressRequest {
        token: token.to_string(),
        effect_id: Some(c.effect_id.clone()),
        tool_call_id: format!("med-{}", c.env_handle),
        env_handle: c.env_handle.clone(),
        protocol,
        host_raw: host,
        resolved_addrs: vec![],
        port,
        method: Some(h.method.clone()),
        path: None,
        headers: h
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        body: if body.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(body).to_string())
        },
        body_readers: None,
        credential_sentinels: vec![],
    };
    if h.method == "CONNECT" {
        let (host, port) = h.target.split_once(':').unwrap_or(("", "443"));
        return (
            Some(base(
                EgressProtocol::HttpsConnect,
                host.to_string(),
                port.parse().unwrap_or(443),
            )),
            true,
        );
    }
    // absolute-form: GET http://host[:port]/path
    if let Some(rest) = h.target.strip_prefix("http://") {
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = authority.split_once(':').unwrap_or((authority, "80"));
        let mut r = base(
            EgressProtocol::Http,
            host.to_string(),
            port.parse().unwrap_or(80),
        );
        r.path = Some(path.to_string());
        return (Some(r), false);
    }
    (None, false)
}
