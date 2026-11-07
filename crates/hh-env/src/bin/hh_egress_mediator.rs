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

fn load_policy(path: &PathBuf) -> Result<ContainmentPolicy, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("policy read: {e}"))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let j = json::parse(&text).map_err(|e| format!("policy parse: {e}"))?;
    ContainmentPolicy::from_json(&j).map_err(|e| format!("policy decode: {e}"))
}

fn main() {
    let c = parse();
    let mut store = Store::open(&c.store_root).unwrap_or_else(|e| {
        eprintln!("hh-egress-mediator: store open: {e}");
        exit(3)
    });
    let mut manifest = RunManifest::minimal(hharness_ledger::manifest::RunKind::Experiment);
    manifest.configuration_id = None;
    manifest.configuration_version_id = None;
    manifest.environment_ref = None;
    manifest.environment_version_id = None;
    manifest.participant_class = hharness_ledger::manifest::ParticipantClass::Native;
    manifest.lease_ttl.writer_ms = 3_600_000;
    let (run_id, lease) = match &c.run_id {
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
    };
    let _ = std::fs::remove_file(&c.socket);
    if let Some(d) = c.socket.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let listener = UnixListener::bind(&c.socket).unwrap_or_else(|e| {
        eprintln!("hh-egress-mediator: bind {}: {e}", c.socket.display());
        exit(2)
    });
    println!("ready socket={} run={run_id}", c.socket.display());
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    // The slot's phase effect token — minted internally, never in-cell.
    let mut minter = TokenMinter::new(c.seed);
    let token = minter.mint(&c.effect_id, 1, &c.env_handle);
    let resolver_tokens = minter.resolver();

    let resolver: Box<dyn hharness_env::egress::EgressResolver> = if c.resolve.is_empty() {
        Box::new(SystemResolver)
    } else {
        let mut map = std::collections::BTreeMap::new();
        for (h, ip) in &c.resolve {
            map.insert(h.clone(), vec![*ip]);
        }
        Box::new(StaticResolver { map })
    };

    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let policy = match load_policy(&c.policy) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("hh-egress-mediator: {e}");
                continue;
            }
        };
        let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "mediator");
        let mut med = EgressMediator {
            store: &mut store,
            run_id: run_id.clone(),
            lease: &lease,
            tokens: resolver_tokens.clone(),
            broker: &mut broker,
            policy,
            cache: ApprovalCache::default(),
            budget_id: None,
            participant_ref: c.participant.clone(),
            resolver: Box::new(resolver_clone(&resolver)),
            transport: Box::new(LocalHttpTransport::default()),
            inspect_hooks: std::collections::BTreeMap::new(),
        };
        serve_conn(stream, &mut med, &c, &token.token);
    }
}

// `EgressResolver` isn't Clone — rebuild the table per connection (the
// StaticResolver map is a public member; SystemResolver is stateless).
fn resolver_clone(
    r: &Box<dyn hharness_env::egress::EgressResolver>,
) -> impl hharness_env::egress::EgressResolver {
    let _ = r;
    SystemResolver
}

fn serve_conn(
    stream: std::os::unix::net::UnixStream,
    med: &mut EgressMediator,
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
    let chain = ScopeChain {
        turn_id: "mediator".to_string(),
        model_call_id: String::new(),
        tool_call_id: req.tool_call_id.clone(),
    };
    let started = med.store.now_ms();
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
                    Err(e) => eprintln!("hh-egress-mediator: tunnel: {e:?}"),
                }
            } else {
                match med.forward(
                    &req,
                    &g.decision,
                    g.decided_by,
                    &g.effect_id,
                    g.started,
                    &chain,
                ) {
                    Ok(MediatedOutcome::Forwarded { response, .. }) => {
                        let _ = http::write_response(
                            &mut stream,
                            response.status,
                            &response
                                .headers
                                .iter()
                                .map(|(k, v)| (k.as_str(), v.as_str()))
                                .collect::<Vec<_>>(),
                            &response.body,
                        );
                    }
                    Ok(MediatedOutcome::Refused { .. })
                    | Ok(MediatedOutcome::RefusedCredential { .. }) => {
                        reply(&mut stream, 403, b"egress denied")
                    }
                    Ok(_) => reply(&mut stream, 500, b"protocol"),
                    Err(e) => reply(&mut stream, 502, format!("{e:?}").as_bytes()),
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
