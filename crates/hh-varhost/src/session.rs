//! `VariantSession` — one plugin, one process, one policy (V3). The
//! session owns the framed channel, the negotiated handshake record, the
//! sealed grant set and the live bindings; the [`crate::host`] drives the
//! verbs over it.
//!
//! Launch path (the `subprocess_confined` placement, §8.4 §3): the host
//! binds a unix listener whose path is the *only* member of the spawned
//! process's `unix_sockets.allow`; the helper execs the variant binary
//! under the lowered `ContainmentPolicy` with a cleared environment
//! (`env_clear`) and the declared env projection; the variant connects to
//! the socket and probes `hello`. Everything the process can do beyond
//! that channel is what the policy allows — nothing ambient (V4).

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hh_embed_schema::plugin_abi::{
    plugin_abi_schema_hash, AbiError, AbiPayload, HelloParams, HelloResult, TriState,
};
use hh_env::helper::HelperClient;
use hh_helper::protocol::{CommitProof, HelperRequest, OnKernelLoss};
use hh_hir::refs::Ref;
use hh_registry::extension::plugin::Requests;
use hh_wire::json::Json;

use crate::channel::{AbiChannel, FrameIo, SocketIo};
use crate::lower::{lower_requests, view_kinds_for, LowerContext, LoweredRequests};
use crate::package::VariantPackage;
use crate::screen::{screen_inbound, ScreenView, ScreenViolation};
use crate::HostError;

/// One bound slot — the `BindingRecord` the host keeps (§8.4 §3).
#[derive(Debug, Clone)]
pub struct BindingRecord {
    /// The binding id (`binding_id` on the wire).
    pub binding_id: String,
    /// The slot.
    pub slot: String,
    /// The class.
    pub class_id: String,
    /// The negotiated contract version.
    pub contract_version: String,
    /// The placement (`subprocess_confined` on this host).
    pub placement: String,
}

/// What a session knows for screening and mediation.
pub struct VariantSession {
    /// `host_id` — the session identity (`hello`'s result member).
    pub session_id: String,
    /// `namespace/name`.
    pub plugin_id: String,
    /// The sealed `version_id` pin (V5).
    pub version_id: String,
    /// The sealed package content address (V5).
    pub content: String,
    /// The plugin's offered capabilities (tri-state — `UNKNOWN` stays
    /// `UNKNOWN`, T-LCD-07).
    pub capabilities: BTreeMap<String, TriState>,
    /// The negotiated contract versions (`hello_ack`'s record).
    pub contract_versions_chosen: BTreeMap<String, String>,
    /// The sealed grant domains (`{domain, scope}` spellings) — the
    /// callback grant checks' operand.
    pub grants: BTreeSet<String>,
    /// The view kinds `read_view` may serve.
    pub view_kinds: BTreeSet<String>,
    /// Live bindings.
    pub bindings: BTreeMap<String, BindingRecord>,
    /// Open stream invocation ids.
    pub open_streams: BTreeSet<String>,
    /// The in-flight invocation id (`cancel`'s operand).
    pub in_flight: Option<String>,
    /// Set while a `guard` invocation is in flight (the
    /// `propose_effect`-in-guard refusal).
    pub in_guard: bool,
    /// The screening view.
    pub view: ScreenView,
    /// The framed channel.
    pub chan: AbiChannel,
    /// The helper session + execution id (None on the in-memory lane).
    pub helper: Option<HelperGuard>,
    /// `detached` — set on desync/crash; a detached session refuses every
    /// verb `SessionDetached` until re-`hello`ed.
    pub detached: bool,
    /// Lowering losses (recorded at spawn, surfaced in `hello_ack`'s host
    /// view and the spawn report).
    pub losses: Vec<String>,
}

/// The live-helper half of a session — kept for `cancel(kill)`/teardown.
pub struct HelperGuard {
    /// The helper client (the variant process's jailer).
    pub client: HelperClient,
    /// The execution id under the helper.
    pub execution_id: String,
    /// The socket path the variant connected on.
    pub channel_socket: PathBuf,
}

impl std::fmt::Debug for VariantSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VariantSession{{{}/{}@{}}}",
            self.plugin_id, self.session_id, self.version_id
        )
    }
}

/// The spawn specification (what `spawn` needs beyond the package).
pub struct SpawnSpec {
    /// The sealed+verified package.
    pub package: VariantPackage,
    /// The session id to assign (`host_id`).
    pub session_id: String,
    /// The helper backend (`seatbelt` live; `direct` for the unsandboxed
    /// test lane).
    pub backend: String,
    /// The dir the listener + helper socket live in (per-session — V3).
    pub socket_dir: PathBuf,
    /// Extra argv after `--socket <path>` (mode flags).
    pub exec_args: Vec<String>,
    /// The `requests` ceiling (the sealing principal's narrowed cap).
    pub cap: Requests,
    /// The ambient environment `env_keys` may project from.
    pub ambient_env: Vec<(String, String)>,
    /// The sealing principal's identity coordinate.
    pub issuer_ref: String,
    /// The plugin's holder ref (the Permission's holder).
    pub holder: Ref,
    /// `registry_snapshot_id` reported at `hello_ack`.
    pub registry_snapshot_id: String,
    /// The kernel capability map reported at `hello_ack`.
    pub kernel_capabilities: BTreeMap<String, TriState>,
    /// The contract versions the host offers (for `hello_ack`'s
    /// `contract_versions_chosen` — computed from the class catalog).
    pub contract_versions_offered: BTreeMap<String, String>,
    /// The handshake + connect deadline.
    pub timeout: Duration,
    /// The logical mint time for the lowered policy/permission.
    pub at: u64,
}

impl VariantSession {
    /// The screenable view (for `screen_inbound`).
    pub fn screen_view(&self) -> ScreenView {
        ScreenView {
            bindings: self.bindings.keys().cloned().collect(),
            open_streams: self.open_streams.clone(),
            in_flight: self.in_flight.clone(),
            handshaken: true,
        }
    }

    /// Screen one inbound envelope against the session view (V1/V5).
    pub fn screen(
        &self,
        env: &hh_embed_schema::plugin_abi::AbiEnvelope,
    ) -> Result<(), ScreenViolation> {
        screen_inbound(env, &self.screen_view())
    }

    /// Mark the session detached (desync, crash, violation that ends the
    /// channel). The helper side is killed if present.
    pub fn detach(&mut self) {
        self.detached = true;
        self.chan.close();
        if let Some(h) = &mut self.helper {
            let _ = h
                .client
                .request(&hh_helper::protocol::HelperRequest::Cancel {
                    execution_id: h.execution_id.clone(),
                    phase: "kill".to_string(),
                });
        }
    }
}

/// `spawn(spec)` — the `subprocess_confined` launch: lower the requests,
/// bind the channel listener, spawn the helper under the lowered policy,
/// exec the variant with a cleared environment, accept its connection and
/// run the `hello` handshake. On any failure nothing survives — the
/// listener, the helper and the exec are all torn down (fail-closed).
pub fn spawn(spec: &SpawnSpec) -> Result<(VariantSession, LoweredRequests), HostError> {
    let pkg = &spec.package;
    let chan_sock = spec.socket_dir.join("variant.sock");
    let chan_sock_s = chan_sock.to_string_lossy().to_string();

    // 1. Lower — claims → Permission + ContainmentPolicy (cap-checked).
    let exec_paths: Vec<String> = pkg.exec_paths();
    let lowered = lower_requests(
        &pkg.manifest,
        &spec.cap,
        &LowerContext {
            package_root: &pkg.root.to_string_lossy(),
            channel_socket: &chan_sock_s,
            exec_paths: &exec_paths,
            ambient_env: &spec.ambient_env,
            issuer_ref: spec.issuer_ref.as_str(),
            holder: spec.holder.clone(),
            at: spec.at,
        },
    )
    .map_err(HostError::Lower)?;

    // 2. The channel listener — the path the sandboxed child may connect to.
    std::fs::create_dir_all(&spec.socket_dir).map_err(|e| HostError::Io(e.to_string()))?;
    let _ = std::fs::remove_file(&chan_sock);
    let listener = UnixListener::bind(&chan_sock).map_err(|e| HostError::Io(e.to_string()))?;

    // 3. The helper under the lowered policy.
    let mut client = HelperClient::spawn(&spec.socket_dir, &spec.backend, &[])
        .map_err(|e| HostError::Helper(format!("{e:?}")))?;
    let nonce = format!("vh-{}", spec.session_id);
    client
        .hello(
            OnKernelLoss::Terminate,
            &nonce,
            None,
            Some(&lowered.policy.to_json()),
            None,
            false,
        )
        .map_err(|e| HostError::Helper(format!("{e:?}")))?;

    // 4. Exec the variant — `argv = [bin, --socket, chan_sock, …exec_args]`,
    // cleared env + the declared projection; no deadline (the session, not
    // the call, is the lifetime — per-invocation deadlines are the host's
    // recv bounds + `cancel`).
    let mut argv = vec![
        pkg.exec_binary().to_string_lossy().to_string(),
        "--socket".to_string(),
        chan_sock_s.clone(),
    ];
    argv.extend(spec.exec_args.iter().cloned());
    let execution_id = format!("{}-exec", spec.session_id);
    let reply = client
        .request(&HelperRequest::Exec {
            execution_id: execution_id.clone(),
            effect_id: format!("{}-spawn", spec.session_id),
            attempt_no: 1,
            capability_ref: (pkg.manifest.identity.id(), pkg.version_id().to_string()),
            args: Json::obj([(
                "argv",
                Json::Arr(argv.iter().map(|a| Json::str(a.clone())).collect()),
            )]),
            cwd: pkg.root.to_string_lossy().to_string(),
            env: lowered.env.clone(),
            deadline_ms: None,
            retain_bytes_cap: 1 << 20,
            attribution_token: format!("varhost:{}", spec.session_id),
            commit_proof: CommitProof::ReadOnly,
            idempotency_key: None,
            env_clear: true,
        })
        .map_err(|e| HostError::Helper(format!("{e:?}")))?;
    if reply.get("error").is_some() {
        return Err(HostError::Helper(format!("exec refused: {reply:?}")));
    }

    // 5. Accept the variant's connection (bounded — a variant that never
    //    connects is a spawn failure, never a hang).
    let deadline = Instant::now() + spec.timeout;
    let stream = loop {
        // Non-blocking accept with a poll loop — the listener stays
        // blocking for the peer (connect is instant once spawned).
        listener
            .set_nonblocking(true)
            .map_err(|e| HostError::Io(e.to_string()))?;
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    // Read back the child's journal — a variant that never
                    // connected usually died on launch; its stderr is the
                    // diagnosis (surfaced verbatim, never silent).
                    let journal = client
                        .request(&HelperRequest::Read {
                            execution_id: execution_id.clone(),
                            after_seq: 0,
                            max_bytes: 4096,
                            wait_ms: 0,
                        })
                        .ok();
                    let _ = client.request(&HelperRequest::Cancel {
                        execution_id: execution_id.clone(),
                        phase: "kill".to_string(),
                    });
                    return Err(HostError::Helper(format!(
                        "variant never connected to the ABI channel: {journal:?}"
                    )));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(HostError::Io(e.to_string())),
        }
    };
    let chan = AbiChannel::new(Box::new(SocketIo::new(stream)));

    // 6. The handshake — `expect_hello` is the protocol-half; a handshake
    //    failure tears everything down.
    let mut session = VariantSession {
        session_id: spec.session_id.clone(),
        plugin_id: String::new(),
        version_id: String::new(),
        content: String::new(),
        capabilities: BTreeMap::new(),
        contract_versions_chosen: BTreeMap::new(),
        grants: BTreeSet::new(),
        view_kinds: BTreeSet::new(),
        bindings: BTreeMap::new(),
        open_streams: BTreeSet::new(),
        in_flight: None,
        in_guard: false,
        view: ScreenView::default(),
        chan,
        helper: Some(HelperGuard {
            client,
            execution_id,
            channel_socket: chan_sock,
        }),
        detached: false,
        losses: lowered.losses.clone(),
    };
    let grants: BTreeSet<String> = lowered
        .permission
        .grants
        .iter()
        .map(|g| format!("{}:{}", g.effect.domain.name(), g.scope))
        .collect();
    session.grants = grants;
    session.view_kinds = view_kinds_for(&lowered.permission);
    if let Err(e) = expect_hello(&mut session, spec) {
        session.detach();
        return Err(e);
    }
    session.view.handshaken = true;
    Ok((session, lowered))
}

/// The in-memory spawn — a session over an already-connected channel (the
/// protocol suite's lane; identical screening/handshake, no helper).
pub fn attach(io: Box<dyn FrameIo>, spec: &SpawnSpec) -> Result<VariantSession, HostError> {
    let mut session = VariantSession {
        session_id: spec.session_id.clone(),
        plugin_id: String::new(),
        version_id: String::new(),
        content: String::new(),
        capabilities: BTreeMap::new(),
        contract_versions_chosen: BTreeMap::new(),
        grants: BTreeSet::new(),
        view_kinds: BTreeSet::new(),
        bindings: BTreeMap::new(),
        open_streams: BTreeSet::new(),
        in_flight: None,
        in_guard: false,
        view: ScreenView::default(),
        chan: AbiChannel::new(io),
        helper: None,
        detached: false,
        losses: Vec::new(),
    };
    if let Err(e) = expect_hello(&mut session, spec) {
        session.detach();
        return Err(e);
    }
    session.view.handshaken = true;
    Ok(session)
}

/// The `hello` half of spawn — receive the plugin's probe, check V5 (pin,
/// content, protocol), then answer `hello_ack`. A refused handshake is a
/// typed [`AbiError`], never a hang.
fn expect_hello(session: &mut VariantSession, spec: &SpawnSpec) -> Result<(), HostError> {
    let env = session
        .chan
        .recv(Some(spec.timeout))
        .map_err(HostError::Channel)?;
    screen_inbound(&env, &ScreenView::default()).map_err(HostError::Screen)?;
    let AbiPayload::Hello(hello) = env.payload else {
        // The first plugin message is `hello` — anything else is a
        // direction violation (screening let it through only because the
        // verb is legal mid-session; at handshake it is not).
        return Err(HostError::Abi(AbiError::SchemaViolation));
    };
    check_hello(&hello, spec)?;
    session.plugin_id = spec.package.manifest.identity.id();
    session.version_id = spec.package.version_id().to_string();
    session.content = hello.plugin_content.clone();
    session.capabilities = hello.capabilities.clone();
    session.contract_versions_chosen = negotiate(
        &hello.contract_versions_offered,
        &spec.contract_versions_offered,
    );
    session
        .chan
        .send(
            &plugin_abi_schema_hash(),
            AbiPayload::HelloAck(HelloResult {
                host_id: spec.session_id.clone(),
                plugin_abi_version: format!("plugin_abi/{PLUGIN_ABI}"),
                contract_versions_chosen: session.contract_versions_chosen.clone(),
                registry_snapshot_id: spec.registry_snapshot_id.clone(),
                kernel_capabilities: spec.kernel_capabilities.clone(),
            }),
        )
        .map_err(HostError::Channel)?;
    Ok(())
}

const PLUGIN_ABI: i64 = hh_embed_schema::plugin_abi::PLUGIN_ABI_MAJOR;

/// V5's handshake half — pin, content, protocol range.
fn check_hello(hello: &HelloParams, spec: &SpawnSpec) -> Result<(), HostError> {
    // The plugin's declared `plugin_abi` must intersect the host's major
    // (V5 both ways — the plugin refuses an out-of-range host, the host
    // refuses an out-of-range plugin).
    let offered = hello.plugin_abi_version.trim_start_matches("plugin_abi/");
    if offered.parse::<i64>().ok() != Some(PLUGIN_ABI) {
        return Err(HostError::Abi(AbiError::ProtocolVersionMismatch));
    }
    if hello.plugin_version_id != spec.package.version_id() {
        return Err(HostError::Abi(AbiError::PinMismatch));
    }
    if hello.plugin_content != spec.package.content() {
        return Err(HostError::Abi(AbiError::PinMismatch));
    }
    Ok(())
}

/// `hello`'s negotiation — for each host-offered contract, the version the
/// plugin also offers is chosen (highest common: the version strings here
/// are single versions; the range meet lives in `check_compatibility` at
/// admission — `hello` asserts it).
fn negotiate(offered: &[Json], host: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut chosen = BTreeMap::new();
    for o in offered {
        if let Ok(r) = hh_plugin::ContractRef::from_json(o, "hello.contract_versions_offered") {
            if let Some(v) = host.get(&r.label()) {
                chosen.insert(r.label(), v.clone());
            }
        }
    }
    chosen
}
