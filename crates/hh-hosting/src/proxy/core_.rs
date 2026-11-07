//! `proxy::core_` — the `Proxy` orchestrator: the phase proxy run's
//! sole writer, the slot/token gate, the FIFO upstream gate, the HTTP
//! surface loop and the Json control channel (§4.1–§4.3; AR-08).
//!
//! `Store` is not `Send` (its `AuditKeyResolver`/`BlobCipher` seams are
//! boxed trait objects), so the proxy is deliberately single-threaded:
//! the calling thread owns `Core` and the listener and runs the
//! accept+control loop (the binary feeds it stdin lines over an
//! `mpsc` channel; the runner is the only speaker). Serial handling
//! keeps the FIFO upstream order trivially exact — the "ledger actor"
//! of design/33 §1.1 is the one writer thread.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use hharness_budget::account::Account;
use hharness_budget::attribution::{Attribution, ModelRef};
use hharness_budget::pricing::{self, SpendSource};
use hharness_budget::quantity::ResourceVector;
use hharness_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hharness_budget::usage::TokenDecomposition;
use hharness_gateway::dialect::WireDialect;
use hharness_gateway::gateway::CredentialHandle;
use hharness_gateway::http_transport::{HttpModelTransport, RawExchange};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{EventRef, ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::dimensions::{DimensionId, DimensionKey};
use hharness_provenance::ProvenanceRecord;
use hharness_secrets::events::{bound_payload, revoked_payload};
use hharness_secrets::redact::{leak_scan, DetectorSet, ScanTarget};
use hharness_secrets::tokens::{mint_token, token_matches};
use hharness_secrets::{CompositeResolver, MaskEntry, MaskSet, SecretSource, SecretSourceResolver};
use hharness_tls::ClientTls;
use hharness_wire::http::{read_request, split_target, write_response};
use hharness_wire::json::{parse_provider, Json, PJson};

use super::apply::{apply_policy, error_body, filter_headers, Mutations, Refusal};
use super::lane::{ProxyLane, Reservation, Route, Surface};
use super::{FilteredHeaders, PROXY_VERSION, RECEIPT_MEDIA, RECEIPT_SCHEMA};

/// The component variant spelling stamped on every row.
const COMPONENT: &str = "hh-model-proxy/1";
/// The slot-token channel id (one channel per phase run).
const SLOT_CHANNEL: &str = "hh.proxy.slot";
/// The upstream credential channel id in the mask set.
const KEY_CHANNEL: &str = "ollama.key";
/// The blob media type (canonical JSON).
const BLOB_MEDIA: &str = "application/json";

/// The proxy's typed error — every failure is a spelled refusal, never
/// a fabricated success.
#[derive(Debug)]
pub enum ProxyError {
    /// Store/ledger failure.
    Ledger(String),
    /// Configuration refused (lane parse, budget allocation, credential).
    Config(String),
    /// I/O failure.
    Io(String),
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::Ledger(d) => write!(f, "ledger: {d}"),
            ProxyError::Config(d) => write!(f, "config: {d}"),
            ProxyError::Io(d) => write!(f, "io: {d}"),
        }
    }
}
impl std::error::Error for ProxyError {}

/// `configure` input — the phase run declaration (the control channel
/// carries exactly this shape under `op: "configure"`).
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// The phase proxy run's own store root (C-03 — the proxy is sole
    /// writer; nothing else opens it).
    pub store_root: PathBuf,
    /// The phase proxy run id (the runner's coordinate).
    pub run_id: String,
    /// The declared lane document.
    pub lane: ProxyLane,
    /// The loopback listen address (`127.0.0.1:<port>`; `:0` binds
    /// ephemeral and reports the bound port on `configure`'s reply).
    pub listen: String,
    /// The upstream credential's launch-env var (its `launch_env:<var>`
    /// coordinate resolves only inside `forward_raw`'s send and inside
    /// the mask-set build — never a record, X-05).
    pub credential_env: Option<String>,
    /// The phase root pool cap in µ$ (`spend` hard ceiling; one slice
    /// per slot under it — REQ-XC-112's shape on fixture lanes).
    pub root_cap_micros: i64,
}

/// One registered slot.
struct Slot {
    token: String,
    budget_id: String,
    binding_id: String,
    effort: String,
    calls: u64,
    live: bool,
}

/// The core — `Store` (sole writer), the run's lease, slots, and the
/// `leak_scan` mask set.
struct Core {
    store: Store,
    lease: Lease,
    cfg: ProxyConfig,
    slots: BTreeMap<String, Slot>,
    mask: MaskSet,
    /// `true` once an Ollama-key `leak_scan` hit refused a deposit —
    /// every later admit refuses `refused_phase_stopped` (REQ-XC-109).
    phase_stopped: bool,
    /// The phase-level turn — scope validation nests every
    /// `model.call.*` row under an open `turn_id`; the phase proxy run
    /// opens exactly one turn at configure (`lifecycle.turn.started`)
    /// and every call's scope carries it.
    turn_id: String,
}

impl Core {
    fn detectors(&self) -> DetectorSet {
        DetectorSet::standard(self.mask.clone())
    }
}

/// The FIFO upstream gate — admission order is the queue order; at
/// most `limit` calls hold an in-flight slot (the account's declared
/// concurrency bound).
struct Gate {
    order: VecDeque<u64>,
    in_flight: usize,
    limit: usize,
    next: u64,
}

impl Gate {
    fn new(limit: usize) -> Gate {
        Gate {
            order: VecDeque::new(),
            in_flight: 0,
            limit,
            next: 0,
        }
    }

    /// Admit — under serial handling the queue is trivially FIFO; the
    /// gate still records the admitted order for the ledger fields.
    fn admit(&mut self) -> u64 {
        let start = Instant::now();
        self.next += 1;
        self.order.push_back(self.next);
        if self.order.front() == Some(&self.next) && self.in_flight < self.limit {
            self.order.pop_front();
            self.in_flight += 1;
        }
        start.elapsed().as_millis() as u64
    }

    fn release(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}

/// The `Proxy` — `configure`d once per phase run, then driven by
/// `tick`/`run` on the owning thread.
pub struct Proxy {
    core: Core,
    listener: TcpListener,
    gate: Gate,
    bound: std::net::SocketAddr,
}

/// One control-channel op's reply (the Json body).
pub type ControlOutcome = Json;

/// The released downstream response: `(body, status, headers)`.
type Released = (Vec<u8>, u16, Vec<(String, String)>);

fn err_json(detail: &str) -> Json {
    Json::obj([("error", Json::obj([("detail", Json::str(detail))]))])
}

/// A locally-minted mask fingerprint — used only for channel
/// attribution of `leak_scan` hits (the broker's keyed form is its
/// own; ours is never recorded outside the mask set).
fn fp(value: &str, channel_id: &str) -> hharness_secrets::types::SecretFingerprint {
    hharness_secrets::types::SecretFingerprint {
        channel_id: channel_id.to_string(),
        revision: 1,
        digest: hharness_wire::sha256::sha256_hex(
            format!("proxy.mask\u{1f}{channel_id}\u{1f}{value}").as_bytes(),
        )[..32]
            .to_string(),
    }
}

fn mint(core: &Core, class: &str, payload: Json, scope: Scope, causes: Vec<EventRef>) -> Event {
    Event {
        event_id: core.store.alloc_id("evt"),
        class: class.to_string(),
        ts: core.store.ts_now(),
        hlc: None,
        producer: Producer::kernel(COMPONENT),
        scope,
        parent_event_id: core
            .store
            .head_event_id(&core.cfg.run_id)
            .unwrap_or_default(),
        causes,
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(COMPONENT, core.store.now_ms())),
        content_kind: None,
        payload,
    }
}

fn append(core: &mut Core, mut events: Vec<Event>) -> Result<(), ProxyError> {
    let mut parent = core
        .store
        .head_event_id(&core.cfg.run_id)
        .unwrap_or_default();
    for e in &mut events {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    core.store
        .append(&core.cfg.run_id, &core.lease, events)
        .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
    Ok(())
}

fn scope_mc(core: &Core, mc: &str) -> Scope {
    Scope {
        turn_id: Some(core.turn_id.clone()),
        model_call_id: Some(mc.to_string()),
        ..Default::default()
    }
}

/// Deposit `bytes` under `media` through the store seam after a
/// `leak_scan` (REQ-XC-109) — `(address, slot_token_hit_ids)`. An
/// Ollama-key hit refuses the write and flips `phase_stopped`.
fn deposit(
    core: &mut Core,
    label: &str,
    bytes: &[u8],
    media: &str,
) -> Result<(String, Vec<String>), ProxyError> {
    let text = String::from_utf8_lossy(bytes);
    let items = [(
        ScanTarget::Mediation(format!("proxy.blob.{label}")),
        text.as_ref(),
    )];
    let leaks = leak_scan(&items, &core.detectors());
    let mut token_hits = Vec::new();
    for l in &leaks {
        if let Some(fp_render) = &l.fingerprint {
            for (cid, entries) in &core.mask.entries {
                if entries.iter().any(|e| e.fingerprint.render() == *fp_render) {
                    if cid == KEY_CHANNEL {
                        core.phase_stopped = true;
                        return Err(ProxyError::Config(format!(
                            "leak_scan: {KEY_CHANNEL} hit in {label} — deposit refused, phase stopped"
                        )));
                    }
                    token_hits.push(cid.clone());
                }
            }
        }
    }
    let addr = core
        .store
        .put_blob(bytes, media)
        .map_err(|e| ProxyError::Ledger(format!("put_blob {label}: {e}")))?;
    Ok((addr.id(), token_hits))
}

/// The response-header outbound allowlist (§4.2; ADR-0363 D3 shape).
fn out_headers(h: &BTreeMap<String, String>) -> Vec<(String, String)> {
    h.iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "content-type" | "retry-after" | "request-id" | "x-request-id"
            ) || k.starts_with("x-ratelimit-")
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Per-call timing record (§4.3 `timing`).
#[derive(Default)]
struct Timing {
    received_ms: u64,
    reserved_ms: Option<u64>,
    dequeued_ms: Option<u64>,
    first_byte_ms: Option<u64>,
    upstream_done_ms: Option<u64>,
    receipted_ms: Option<u64>,
    released_ms: Option<u64>,
    queue_wait_ms: u64,
}

impl Timing {
    fn to_json(&self) -> Json {
        let opt = |v: Option<u64>| v.map(|x| Json::Int(x as i64)).unwrap_or(Json::Null);
        Json::obj([
            ("received_ms", Json::Int(self.received_ms as i64)),
            ("reserved_ms", opt(self.reserved_ms)),
            ("dequeued_ms", opt(self.dequeued_ms)),
            ("first_byte_ms", opt(self.first_byte_ms)),
            ("upstream_done_ms", opt(self.upstream_done_ms)),
            ("receipted_ms", opt(self.receipted_ms)),
            ("released_ms", opt(self.released_ms)),
            ("queue_wait_ms", Json::Int(self.queue_wait_ms as i64)),
        ])
    }
}

/// The extracted usage + cost fact for one exchange (REQ-XC-007).
struct UsageCost {
    decomp: TokenDecomposition,
    cache_read_available: bool,
    reasoning_available: bool,
    source: String,
    derivation: String,
    micros: Option<i64>,
}

/// Extract the provider usage vector per surface and derive the cost
/// fact: a provider cached count ⇒ `estimated_from_pricing` with the
/// actual split; none ⇒ an upper-bound `estimate` whose `cache_read`
/// is `available: false`, never `0` (REQ-XC-007).
fn usage_cost(surface: &Surface, ex: &RawExchange, lane: &ProxyLane) -> UsageCost {
    let body = parse_provider(&String::from_utf8_lossy(&ex.body)).unwrap_or(Json::Null);
    let u = body.get("usage").cloned().unwrap_or(Json::Null);
    let get_i = |k: &str| u.get(k).and_then(Json::as_int).unwrap_or(0);
    let mut d = TokenDecomposition::default();
    let mut cache_avail = false;
    let mut reasoning_avail = false;
    match surface {
        Surface::OpenAiChat => {
            let input = get_i("prompt_tokens");
            if let Some(c) = u
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(Json::as_int)
            {
                d.input_uncached = input - c;
                d.cache_read = c;
                cache_avail = true;
            } else {
                d.input_uncached = input;
            }
            if let Some(r) = u
                .get("completion_tokens_details")
                .and_then(|d| d.get("reasoning_tokens"))
                .and_then(Json::as_int)
            {
                d.output_reasoning = r;
                d.output_visible = get_i("completion_tokens") - r;
                reasoning_avail = true;
            } else {
                d.output_visible = get_i("completion_tokens");
            }
        }
        Surface::AnthropicMessages => {
            let input = get_i("input_tokens");
            if let Some(c) = u.get("cache_read_input_tokens").and_then(Json::as_int) {
                d.input_uncached = input - c;
                d.cache_read = c;
                cache_avail = true;
            } else {
                d.input_uncached = input;
            }
            d.output_visible = get_i("output_tokens");
        }
    }
    let table = pricing::ollama_glm_5_3_flash();
    let model_ref = ModelRef {
        profile_ref: format!("hh.proxy.lane:{}", lane.lane_id),
        provider_model_id: lane.model.clone(),
        // The pricing key is `{model}@{provider}` — the lane's
        // provider id is `ollama` (the §4.1 lane family), never the
        // op path.
        serving_route: "ollama".to_string(),
        effort: None,
    };
    let micros = pricing::price(&d, &model_ref, &table)
        .map(|(m, _)| m.micro_units)
        .ok();
    UsageCost {
        decomp: d,
        cache_read_available: cache_avail,
        reasoning_available: reasoning_avail,
        source: if u == Json::Null {
            "absent".to_string()
        } else {
            "provider_per_call".to_string()
        },
        derivation: if cache_avail {
            "estimated_from_pricing".to_string()
        } else {
            "estimate".to_string()
        },
        micros,
    }
}

impl Proxy {
    /// `configure` — open the store root, open the phase proxy run
    /// under the fenced writer, allocate the root pool, build the mask
    /// set, and bind the loopback listener. Returns the bound address.
    pub fn configure(cfg: ProxyConfig) -> Result<(Proxy, std::net::SocketAddr), ProxyError> {
        if cfg.lane.endpoint_ref.is_empty() {
            return Err(ProxyError::Config("lane endpoint_ref empty".into()));
        }
        let mut store =
            Store::open(&cfg.store_root).map_err(|e| ProxyError::Ledger(format!("{e}")))?;
        let mut manifest = RunManifest::minimal(RunKind::Experiment);
        // Non-agent runs carry no configuration/environment cells
        // (ADR-0183 §C) — `minimal` seeds the agent-shaped pins; the
        // phase proxy run declares none.
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest.environment_ref = None;
        manifest.environment_version_id = None;
        manifest.participant_class = ParticipantClass::Native;
        manifest.lease_ttl.writer_ms = 3_600_000;
        manifest.extra.insert(
            "proxy".into(),
            Json::obj([
                ("phase_run_id", Json::str(&cfg.run_id)),
                ("proxy_version", Json::str(PROXY_VERSION)),
                ("lane_id", Json::str(&cfg.lane.lane_id)),
            ]),
        );
        let (_, lease) = store
            .open_run_with_id(&cfg.run_id, manifest, COMPONENT)
            .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
        // The upstream credential's *value* enters only the mask set —
        // resolved once from the launch-env coordinate and held
        // kernel-side only (X-05); `provided` is the recorded fact.
        let mut mask = MaskSet::default();
        if let Some(var) = &cfg.credential_env {
            let source = SecretSource::LaunchEnv { var: var.clone() };
            if let Ok(v) = CompositeResolver::default().resolve(&source) {
                mask.insert(MaskEntry {
                    channel_id: KEY_CHANNEL.to_string(),
                    revision: 1,
                    value: v.clone(),
                    fingerprint: fp(&v, KEY_CHANNEL),
                    live: true,
                    placeholder: None,
                });
            }
        }
        let mut core = Core {
            store,
            lease,
            cfg: cfg.clone(),
            slots: BTreeMap::new(),
            mask,
            phase_stopped: false,
            turn_id: String::new(),
        };
        // The phase turn — one enclosing `lifecycle.turn.started`
        // scope under which every proxied call nests.
        let turn_id = core.store.alloc_id("turn");
        {
            let sc = Scope {
                turn_id: Some(turn_id.clone()),
                ..Default::default()
            };
            let ev = mint(
                &core,
                "lifecycle.turn.started",
                Json::obj([
                    ("turn_id", Json::str(&turn_id)),
                    ("lane_id", Json::str(&core.cfg.lane.lane_id)),
                ]),
                sc,
                vec![],
            );
            append(&mut core, vec![ev])?;
            core.turn_id = turn_id;
        }
        // The root pool — `spend` hard ceiling = the declared phase cap
        // (fixture lane values; the §4.4 formula is R6.11's).
        let root = {
            let mut acct = Account::open(&mut core.store, &core.cfg.run_id)
                .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
            acct.allocate(
                &core.lease,
                None,
                BudgetScope {
                    kind: BudgetScopeKind::Experiment,
                    target: core.cfg.run_id.clone(),
                },
                BudgetSpec::hard_caps(
                    BudgetMode::Pool,
                    &[(
                        DimensionKey::parse("spend")
                            .ok_or_else(|| ProxyError::Config("spend key".into()))?,
                        cfg.root_cap_micros,
                    )],
                ),
            )
            .map_err(|e| ProxyError::Config(format!("root pool: {e}")))?
        };
        core.cfg.lane.root_budget_id = root;
        let listener = TcpListener::bind(&cfg.listen)
            .map_err(|e| ProxyError::Io(format!("bind {}: {e}", cfg.listen)))?;
        let bound = listener
            .local_addr()
            .map_err(|e| ProxyError::Io(format!("addr: {e}")))?;
        if !bound.ip().is_loopback() {
            return Err(ProxyError::Config(format!(
                "listen {bound} is not loopback — refused (P5)"
            )));
        }
        listener
            .set_nonblocking(true)
            .map_err(|e| ProxyError::Io(format!("nonblocking: {e}")))?;
        Ok((
            Proxy {
                gate: Gate::new(core.cfg.lane.upstream_concurrency),
                core,
                listener,
                bound,
            },
            bound,
        ))
    }

    /// The bound listen address (`configure`'s reply).
    pub fn bound_addr(&self) -> std::net::SocketAddr {
        self.bound
    }

    /// `control(op, params)` — the Json control channel's dispatch
    /// (the binary's stdin loop calls exactly this; the runner is the
    /// only speaker).
    pub fn control(&mut self, op: &str, params: &Json) -> ControlOutcome {
        match op {
            "register_slot" => {
                let slot_id = params.get("slot_id").and_then(Json::as_str).unwrap_or("");
                let effort = params
                    .get("effort")
                    .and_then(Json::as_str)
                    .unwrap_or("high");
                let micros = params
                    .get("slice_micros")
                    .and_then(Json::as_int)
                    .unwrap_or(0);
                match self.register_slot(slot_id, effort, micros) {
                    Ok(token) => Json::obj([("ok", Json::obj([("token", Json::str(token))]))]),
                    Err(e) => err_json(&format!("{e}")),
                }
            }
            "revoke_slot" => {
                let slot_id = params.get("slot_id").and_then(Json::as_str).unwrap_or("");
                match self.revoke_slot(slot_id) {
                    Ok(()) => Json::obj([("ok", Json::obj([]))]),
                    Err(e) => err_json(&format!("{e}")),
                }
            }
            "status" => Json::obj([(
                "ok",
                Json::obj([
                    ("run_id", Json::str(&self.core.cfg.run_id)),
                    ("lane_id", Json::str(&self.core.cfg.lane.lane_id)),
                    ("phase_stopped", Json::Bool(self.core.phase_stopped)),
                    (
                        "slots",
                        Json::Arr(
                            self.core
                                .slots
                                .iter()
                                .map(|(id, s)| {
                                    Json::obj([
                                        ("slot_id", Json::str(id)),
                                        ("live", Json::Bool(s.live)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ]),
            )]),
            "stop" => Json::obj([("ok", Json::obj([]))]),
            other => err_json(&format!("unknown op {other}")),
        }
    }

    /// `register_slot(slot_id, effort, slice_micros) → token` — mints
    /// the slot token (the one `mint_token` home), allocates the slot
    /// slice under the root pool, adds the token to the mask set and
    /// lands `security.credential.bound` (§4.3's per-slot row).
    pub fn register_slot(
        &mut self,
        slot_id: &str,
        effort: &str,
        slice_micros: i64,
    ) -> Result<String, ProxyError> {
        let core = &mut self.core;
        if core.slots.contains_key(slot_id) {
            return Err(ProxyError::Config(format!(
                "slot {slot_id} already registered"
            )));
        }
        let token = mint_token().map_err(|e| ProxyError::Io(format!("mint: {e}")))?;
        let binding_id = core.store.alloc_id("bind");
        let budget_id = {
            let mut acct = Account::open(&mut core.store, &core.cfg.run_id)
                .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
            acct.allocate(
                &core.lease,
                Some(&core.cfg.lane.root_budget_id),
                BudgetScope {
                    kind: BudgetScopeKind::AgentProcess,
                    target: slot_id.to_string(),
                },
                BudgetSpec::hard_caps(
                    BudgetMode::Slice,
                    &[(
                        DimensionKey::parse("spend")
                            .ok_or_else(|| ProxyError::Config("spend key".into()))?,
                        slice_micros,
                    )],
                ),
            )
            .map_err(|e| ProxyError::Config(format!("slot slice: {e}")))?
        };
        let ev = mint(
            core,
            "security.credential.bound",
            bound_payload(
                &binding_id,
                SLOT_CHANNEL,
                1,
                slot_id,
                &format!("slot:{slot_id}"),
                "slot_token",
                "configure:proxy",
                "never",
                None,
            ),
            Scope::default(),
            vec![],
        );
        append(core, vec![ev])?;
        core.mask.insert(MaskEntry {
            channel_id: format!("{SLOT_CHANNEL}.{slot_id}"),
            revision: 1,
            value: token.clone(),
            fingerprint: fp(&token, &format!("{SLOT_CHANNEL}.{slot_id}")),
            live: true,
            placeholder: None,
        });
        core.slots.insert(
            slot_id.to_string(),
            Slot {
                token: token.clone(),
                budget_id,
                binding_id,
                effort: effort.to_string(),
                calls: 0,
                live: true,
            },
        );
        Ok(token)
    }

    /// `revoke_slot(slot_id)` — token dies (`security.credential.revoked`)
    /// + the slice's remainder returns to the pool.
    pub fn revoke_slot(&mut self, slot_id: &str) -> Result<(), ProxyError> {
        let core = &mut self.core;
        let slot = core
            .slots
            .get_mut(slot_id)
            .ok_or_else(|| ProxyError::Config(format!("unknown slot {slot_id}")))?;
        if !slot.live {
            return Ok(());
        }
        slot.live = false;
        let binding_id = slot.binding_id.clone();
        let budget_id = slot.budget_id.clone();
        let ev = mint(
            core,
            "security.credential.revoked",
            revoked_payload(
                Some(&binding_id),
                Some(SLOT_CHANNEL),
                std::slice::from_ref(&binding_id),
                "slot_close",
            ),
            Scope::default(),
            vec![],
        );
        append(core, vec![ev])?;
        let mut acct = Account::open(&mut core.store, &core.cfg.run_id)
            .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
        acct.complete(&core.lease, &budget_id)
            .map_err(|e| ProxyError::Ledger(format!("{e}")))?;
        Ok(())
    }

    /// `tick` — accept and fully handle at most one connection;
    /// returns `true` when a connection was handled.
    pub fn tick(&mut self) -> bool {
        match self.listener.accept() {
            Ok((stream, _)) => {
                // Accepted sockets inherit the listener's nonblocking
                // flag (macOS) — request handling wants blocking I/O.
                let _ = stream.set_nonblocking(false);
                handle_conn(&mut self.core, &mut self.gate, stream);
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(_) => false,
        }
    }

    /// `run(lines)` — the binary's loop: interleave control-channel
    /// lines with the accept loop until a `stop` op or channel close.
    /// Each line is one Json op; each reply is one Json line on `out`.
    pub fn run(&mut self, lines: &Receiver<String>, out: &mut impl Write) {
        loop {
            let mut stopping = false;
            while let Ok(line) = lines.try_recv() {
                let doc = hharness_wire::json::parse(line.trim());
                let reply = match doc {
                    Ok(d) => {
                        let op = d.get("op").and_then(Json::as_str).unwrap_or("");
                        if op == "stop" {
                            stopping = true;
                            Json::obj([("ok", Json::obj([]))])
                        } else {
                            self.control(op, &d)
                        }
                    }
                    Err(e) => err_json(&format!("parse: {e}")),
                };
                let _ = writeln!(out, "{}", reply.to_canonical_string());
                let _ = out.flush();
            }
            if stopping {
                return;
            }
            if !self.tick() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    /// `stop` — the accept loop exits after in-flight work drains.
    pub fn stop(&mut self) {}

    /// The KP-9 durability seam — tests arm the next `n` durable
    /// writes to fail so the durable-before-visible invariant is
    /// exercised at the release boundary.
    pub fn inject_durability_faults(&mut self, n: u32) {
        self.core.store.inject_durability_faults(n);
    }
}

/// One downstream connection — one request (`Connection: close`; the
/// full §4.3 ledger order runs here and the response bytes are written
/// only after the terminal row is durable).
fn handle_conn(core: &mut Core, gate: &mut Gate, mut stream: TcpStream) {
    let mut r = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let (head, body) = match read_request(&mut r, 16 * 1024 * 1024) {
        Ok(Some(v)) => v,
        _ => return,
    };
    let mut timing = Timing {
        received_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        ..Default::default()
    };
    let mc = core.store.alloc_id("mc");
    let (path, _q) = split_target(&head.target);
    // ── deposit the downstream body blob (the `request_ref` the
    // `requested` row cites — input bytes, never withheld) ──
    let (down_ref, mut token_hits) = match deposit(core, "downstream", &body, BLOB_MEDIA) {
        Ok(v) => v,
        Err(e) => {
            refuse_now(&mut stream, &e.to_string());
            return;
        }
    };
    // ── step 1: model.call.requested ──
    let ev = mint(
        core,
        "model.call.requested",
        Json::obj([
            ("model_call_id", Json::str(&mc)),
            ("request_ref", Json::str(&down_ref)),
            ("method", Json::str(&head.method)),
            ("path", Json::str(path)),
        ]),
        scope_mc(core, &mc),
        vec![],
    );
    if let Err(e) = append(core, vec![ev]) {
        refuse_now(&mut stream, &format!("{e}"));
        return;
    }
    // ── gate: slot token → slot ──
    let presented = head
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer ").or(v.strip_prefix("bearer ")))
        .or_else(|| head.headers.get("x-api-key").map(|s| s.as_str()))
        .map(|s| s.to_string());
    let slot_id = presented.and_then(|t| {
        core.slots
            .iter()
            .find(|(_, s)| s.live && token_matches(&s.token, &t))
            .map(|(id, _)| id.clone())
    });
    let kept = filter_headers(&head.headers);
    let route = match (head.method.as_str(), path) {
        ("POST", "/v1/chat/completions") => Route::Surface(Surface::OpenAiChat),
        ("POST", "/v1/messages") => Route::Surface(Surface::AnthropicMessages),
        ("POST", "/v1/messages/count_tokens") => Route::CountTokens,
        ("GET", "/v1/models") => Route::Models,
        _ => Route::Refused,
    };
    let mut call = CallCtx {
        mc: mc.clone(),
        down_ref,
        method: head.method.clone(),
        path: path.to_string(),
        kept,
        body: body.clone(),
        token_hits: std::mem::take(&mut token_hits),
        surface: route_surface(&route),
        route_label: route_label(&route),
        mutations: Mutations::default(),
        upstream_refs: None,
        released_headers: None,
        usage: None,
    };
    macro_rules! finish {
        ($disposition:expr, $refusal:expr, $upstream:expr, $released:expr, $reservation:expr, $slot:expr) => {{
            let released_v = $released;
            finish(
                core,
                &mut stream,
                &mut timing,
                &mut call,
                $disposition,
                $refusal,
                $upstream,
                released_v,
                $reservation,
                $slot,
            )
        }};
    }
    if core.phase_stopped {
        finish!(
            "refused_phase_stopped",
            Some(Refusal {
                status: 402,
                kind: "refused_phase_stopped",
                path: None,
                detail: "phase stopped by a credential leak".into(),
            }),
            None,
            Some((
                error_body(
                    call.surface,
                    "hh_proxy",
                    "refused_phase_stopped",
                    "phase stopped by a credential leak",
                )
                .into_bytes(),
                402u16,
                vec![("content-type".to_string(), "application/json".to_string())],
            )),
            None,
            slot_id.as_deref()
        );
        return;
    }
    let Some(slot_id) = slot_id else {
        finish!(
            "refused_auth",
            Some(Refusal {
                status: 401,
                kind: "refused_auth",
                path: None,
                detail: "no live slot token matched".into(),
            }),
            None,
            Some((
                error_body(
                    call.surface,
                    "authentication_error",
                    "refused_auth",
                    "invalid slot token",
                )
                .into_bytes(),
                401u16,
                vec![("content-type".to_string(), "application/json".to_string())],
            )),
            None,
            None
        );
        return;
    };
    if matches!(route, Route::Refused | Route::CountTokens) {
        let count_tokens = matches!(route, Route::CountTokens);
        finish!(
            "refused_route",
            Some(Refusal {
                status: 404,
                kind: "refused_route",
                path: Some(path.to_string()),
                detail: if count_tokens {
                    "count_tokens unsupported upstream — never a fabricated count".into()
                } else {
                    "route not in the §4.2 table".into()
                },
            }),
            None,
            Some((
                if count_tokens {
                    error_body(
                        Some(Surface::AnthropicMessages),
                        "not_found_error",
                        "hh_proxy: unsupported_upstream",
                        "count_tokens is not served by this lane",
                    )
                } else {
                    error_body(
                        None,
                        "not_found_error",
                        "refused_route",
                        "unsupported route",
                    )
                }
                .into_bytes(),
                404u16,
                vec![("content-type".to_string(), "application/json".to_string())],
            )),
            None,
            Some(&slot_id)
        );
        return;
    }
    // body parse (Models forwards with no body)
    let mut pbody = if matches!(route, Route::Models) {
        PJson::Null
    } else {
        match hharness_wire::json::parse_preserve(&String::from_utf8_lossy(&body)) {
            Ok(v) => v,
            Err(e) => {
                finish!(
                    "refused_policy",
                    Some(Refusal {
                        status: 400,
                        kind: "refused_policy",
                        path: None,
                        detail: format!("body: {e}"),
                    }),
                    None,
                    Some((
                        error_body(
                            call.surface,
                            "invalid_request_error",
                            "invalid_body",
                            "request body is not JSON",
                        )
                        .into_bytes(),
                        400u16,
                        vec![("content-type".to_string(), "application/json".to_string())],
                    )),
                    None,
                    Some(&slot_id)
                );
                return;
            }
        }
    };
    // streaming — `Unsupported{release_mode: buffered_heartbeat}` until
    // R6.11 (typed, surface-shaped, receipted).
    if pbody
        .get("stream")
        .and_then(PJson::as_bool)
        .unwrap_or(false)
    {
        finish!(
            "refused_policy",
            Some(Refusal {
                status: 400,
                kind: "refused_policy",
                path: Some("stream".into()),
                detail: "unsupported{release_mode:buffered_heartbeat}".into(),
            }),
            None,
            Some((
                error_body(
                    call.surface,
                    "invalid_request_error",
                    "unsupported{release_mode:buffered_heartbeat}",
                    "streaming lands with R6.11's buffered_heartbeat mode",
                )
                .into_bytes(),
                400u16,
                vec![("content-type".to_string(), "application/json".to_string())],
            )),
            None,
            Some(&slot_id)
        );
        return;
    }
    // declared mutations + field policy + pins
    if let Some(s) = call.surface {
        let effort = core
            .slots
            .get(&slot_id)
            .map(|s| s.effort.clone())
            .unwrap_or_default();
        match apply_policy(&mut pbody, s, &core.cfg.lane, &effort) {
            Ok(m) => call.mutations = m,
            Err(r) => {
                finish!(
                    "refused_policy",
                    Some(r.clone()),
                    None,
                    Some((
                        error_body(call.surface, "invalid_request_error", r.kind, &r.detail)
                            .into_bytes(),
                        r.status,
                        vec![("content-type".to_string(), "application/json".to_string())],
                    )),
                    None,
                    Some(&slot_id)
                );
                return;
            }
        }
    }
    // ── step 2: reservation (fixture bound only, pre-R6.11) ──
    let slot_budget = core
        .slots
        .get(&slot_id)
        .map(|s| s.budget_id.clone())
        .unwrap_or_default();
    let reservation = match &core.cfg.lane.reservation {
        Reservation::FixtureBound { micros } => {
            let mut acct = match Account::open(&mut core.store, &core.cfg.run_id) {
                Ok(a) => a,
                Err(e) => {
                    refuse_now(&mut stream, &format!("{e}"));
                    return;
                }
            };
            let mut q = ResourceVector::zero();
            q.add(DimensionId::Spend, *micros);
            match acct.reserve(&core.lease, &slot_budget, &q, &format!("mc:{mc}"), 300_000) {
                Ok(rid) => Some(Json::obj([
                    ("id", Json::str(rid)),
                    ("micros", Json::Int(*micros)),
                    ("bound", Json::obj([("kind", Json::str("fixture_bound"))])),
                ])),
                Err(e) => {
                    finish!(
                        "refused_budget",
                        Some(Refusal {
                            status: 402,
                            kind: "refused_budget",
                            path: None,
                            detail: format!("{e}"),
                        }),
                        None,
                        Some((
                            error_body(
                                call.surface,
                                "invalid_request_error",
                                "budget_refused",
                                "slot budget refused the reservation",
                            )
                            .into_bytes(),
                            402u16,
                            vec![("content-type".to_string(), "application/json".to_string())],
                        )),
                        None,
                        Some(&slot_id)
                    );
                    return;
                }
            }
        }
        Reservation::Unsupported => {
            finish!(
                "refused_budget",
                Some(Refusal {
                    status: 402,
                    kind: "refused_budget",
                    path: None,
                    detail: "unsupported{reservation_formula} — R6.11 owns the §4.4 formula".into(),
                }),
                None,
                Some((
                    error_body(
                        call.surface,
                        "invalid_request_error",
                        "unsupported{reservation_formula}",
                        "lane declares no fixture bound and the §4.4 formula is R6.11's",
                    )
                    .into_bytes(),
                    402u16,
                    vec![("content-type".to_string(), "application/json".to_string())],
                )),
                None,
                Some(&slot_id)
            );
            return;
        }
    };
    timing.reserved_ms = Some(core.store.now_ms());
    // ── step 3: attempt started after the FIFO admit ──
    let upstream_body = if matches!(route, Route::Models) {
        Vec::new()
    } else {
        pbody.to_provider_string().into_bytes()
    };
    let q_wait = gate.admit();
    timing.queue_wait_ms = q_wait;
    timing.dequeued_ms = Some(core.store.now_ms());
    let ev = mint(
        core,
        "model.call.attempt.started",
        Json::obj([
            ("model_call_id", Json::str(&mc)),
            ("attempt_no", Json::Int(1)),
            ("queue_wait_ms", Json::Int(q_wait as i64)),
        ]),
        Scope::default(),
        vec![],
    );
    if let Err(e) = append(core, vec![ev]) {
        gate.release();
        refuse_now(&mut stream, &format!("{e}"));
        return;
    }
    // ── upstream dispatch ──
    let op = match route {
        Route::Surface(s) => s.upstream_op(),
        Route::Models => "models",
        _ => "models",
    };
    let dialect: WireDialect = match route {
        Route::Surface(Surface::AnthropicMessages) => hharness_gateway::dialects::ollama_messages(),
        _ => hharness_gateway::dialects::ollama_chat(),
    };
    let mut t = HttpModelTransport::with_credentials(
        dialect,
        ClientTls::webpki_roots(),
        Box::<CompositeResolver>::default(),
    );
    let cred = CredentialHandle {
        binding_id: core
            .cfg
            .credential_env
            .clone()
            .map(|v| format!("launch_env:{v}")),
        provided: core.cfg.credential_env.is_some(),
    };
    let kept_headers: Vec<(&str, &str)> = call
        .kept
        .kept
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let endpoint = core.cfg.lane.endpoint_ref.clone();
    let dispatched = if matches!(route, Route::Models) {
        t.forward_raw_method("GET", &cred, &endpoint, op, &kept_headers, &[], None)
    } else {
        t.forward_raw(&cred, &endpoint, op, &kept_headers, &upstream_body, None)
    };
    gate.release();
    timing.upstream_done_ms = Some(core.store.now_ms());
    match dispatched {
        Err(e) => {
            let detail = format!("{e}");
            let timeout = detail.contains("timeout") || detail.contains("deadline");
            let ev = mint(
                core,
                "model.call.attempt.failed",
                Json::obj([
                    ("model_call_id", Json::str(&mc)),
                    ("attempt_no", Json::Int(1)),
                    ("error", Json::str(&detail)),
                ]),
                Scope::default(),
                vec![],
            );
            let _ = append(core, vec![ev]);
            let disposition = if timeout {
                "upstream_timeout"
            } else {
                "upstream_error"
            };
            finish!(
                disposition,
                None,
                None,
                Some((
                    error_body(
                        call.surface,
                        "api_error",
                        disposition,
                        "upstream transport failed"
                    )
                    .into_bytes(),
                    if timeout { 504u16 } else { 502u16 },
                    vec![("content-type".to_string(), "application/json".to_string())],
                )),
                reservation,
                Some(&slot_id)
            );
        }
        Ok(ex) => {
            timing.first_byte_ms = ex
                .chunk_stamps
                .first()
                .map(|(s, _)| timing.dequeued_ms.unwrap_or(0) + *s);
            let ev = mint(
                core,
                "model.call.attempt.completed",
                Json::obj([
                    ("model_call_id", Json::str(&mc)),
                    ("attempt_no", Json::Int(1)),
                    ("status", Json::Int(ex.status as i64)),
                ]),
                Scope::default(),
                vec![],
            );
            let _ = append(core, vec![ev]);
            // ── step 5 deposits: the upstream request and response
            // blobs (leak_scan'd like every blob) ──
            let req_ref = match deposit(core, "upstream_request", &upstream_body, BLOB_MEDIA) {
                Ok((r, h)) => {
                    call.token_hits.extend(h);
                    r
                }
                Err(e) => {
                    refuse_now(&mut stream, &e.to_string());
                    return;
                }
            };
            let resp_ref = match deposit(core, "upstream_response", &ex.body, BLOB_MEDIA) {
                Ok((r, h)) => {
                    call.token_hits.extend(h);
                    r
                }
                Err(e) => {
                    refuse_now(&mut stream, &e.to_string());
                    return;
                }
            };
            call.usage = call.surface.map(|s| usage_cost(&s, &ex, &core.cfg.lane));
            call.released_headers = Some(out_headers(&ex.headers));
            call.upstream_refs = Some((req_ref, resp_ref, ex.status));
            finish!(
                "forwarded",
                None,
                None,
                Some((
                    ex.body.clone(),
                    ex.status,
                    call.released_headers.clone().unwrap_or_default()
                )),
                reservation,
                Some(&slot_id)
            );
        }
    }
}

/// The §4.3 tail for every call — deposit the `hh.model_io/1` receipt
/// blob (step 5), land the terminal `model.call.*` row (step 6), settle
/// the reservation (step 7), and only then write the response bytes
/// (durable-before-visible, X-04).
#[allow(clippy::too_many_arguments)]
fn finish(
    core: &mut Core,
    stream: &mut TcpStream,
    timing: &mut Timing,
    call: &mut CallCtx,
    disposition: &str,
    refusal: Option<Refusal>,
    _upstream: Option<()>,
    released: Option<Released>,
    reservation: Option<Json>,
    slot_id: Option<&str>,
) {
    let run_id = core.cfg.run_id.clone();
    let seq = slot_id
        .and_then(|id| core.slots.get(id))
        .map(|s| s.calls)
        .unwrap_or(0);
    let uc = call.usage.take();
    let doc = receipt_doc(
        core,
        &call.mc,
        slot_id,
        call.route_label,
        &call.method,
        &call.path,
        &call.kept,
        &call.down_ref,
        &call.body,
        &call.mutations,
        call.upstream_refs.clone(),
        released
            .as_ref()
            .map(|(_, s, _)| ("buffered".to_string(), *s)),
        released.as_ref().and_then(
            |(_, _, h)| {
                if h.is_empty() {
                    None
                } else {
                    Some(h.clone())
                }
            },
        ),
        timing,
        uc.as_ref(),
        reservation.clone(),
        disposition,
        refusal.as_ref().map(Refusal::to_json),
        &call.token_hits,
        seq,
    );
    let receipt_bytes = doc.to_canonical_string().into_bytes();
    let receipt_ref = match deposit(core, "receipt", &receipt_bytes, RECEIPT_MEDIA) {
        Ok((r, h)) => {
            call.token_hits.extend(h);
            r
        }
        Err(e) => {
            refuse_now(stream, &e.to_string());
            return;
        }
    };
    timing.receipted_ms = Some(core.store.now_ms());
    let class = if disposition == "forwarded" {
        "model.call.completed"
    } else {
        "model.call.failed"
    };
    let mut p = BTreeMap::new();
    p.insert("model_call_id".into(), Json::str(&call.mc));
    p.insert("response_ref".into(), Json::str(&receipt_ref));
    p.insert("disposition".into(), Json::str(disposition));
    p.insert("release_mode".into(), Json::str("buffered"));
    p.insert("timing".into(), timing.to_json());
    if let Some(u) = &uc {
        p.insert(
            "usage".into(),
            Json::obj([
                ("available", Json::Bool(true)),
                ("derivation", Json::str(&u.derivation)),
                ("micros", u.micros.map(Json::Int).unwrap_or(Json::Null)),
                (
                    "cache_read",
                    if u.cache_read_available {
                        Json::Int(u.decomp.cache_read)
                    } else {
                        Json::obj([("available", Json::Bool(false))])
                    },
                ),
            ]),
        );
    }
    let ev = mint(core, class, Json::Obj(p), scope_mc(core, &call.mc), vec![]);
    if append(core, vec![ev]).is_err() {
        return;
    }
    // ── step 7: settlement — `measurement.cost.attributed` +
    // `control.budget.released` land in one batch: the priced charge
    // and the reservation's remaining-claim release are a single
    // durable transition, so `consumed + reserved ≤ hard` never sees
    // an intermediate prefix ──
    if let (Some(u), Some(res)) = (uc.as_ref(), reservation.as_ref()) {
        let rid = res
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let bound = res.get("micros").and_then(Json::as_int).unwrap_or(0);
        if let (Some(micros), Some(slot)) = (u.micros, slot_id) {
            let slot_budget = core
                .slots
                .get(slot)
                .map(|s| s.budget_id.clone())
                .unwrap_or_default();
            let table = pricing::ollama_glm_5_3_flash();
            let model_ref = ModelRef {
                profile_ref: format!("hh.proxy.lane:{}", core.cfg.lane.lane_id),
                provider_model_id: core.cfg.lane.model.clone(),
                serving_route: "ollama".to_string(),
                effort: core.slots.get(slot).map(|s| s.effort.clone()),
            };
            let mut attribution =
                Attribution::subject(run_id.clone(), slot_budget.clone(), slot.to_string());
            attribution.model_ref = Some(model_ref.clone());
            attribution.over_reservation = micros > bound;
            let terminal_ref = EventRef {
                run_id: run_id.clone(),
                event_id: core.store.head_event_id(&run_id).unwrap_or_default(),
            };
            if let Ok(row) = pricing::attribute_spend(
                terminal_ref,
                &u.decomp,
                &model_ref,
                &table,
                None,
                &SpendSource::Measured,
                1_000_000,
                attribution,
            ) {
                let a_ev = mint(
                    core,
                    "measurement.cost.attributed",
                    hharness_budget::events::cost_attributed_payload(&row),
                    Scope::default(),
                    vec![],
                );
                let mut left = ResourceVector::zero();
                left.add(DimensionId::Spend, bound);
                let r_ev = mint(
                    core,
                    "control.budget.released",
                    hharness_budget::events::released_payload(
                        &slot_budget,
                        Some(&rid),
                        &left,
                        "release",
                    ),
                    Scope::default(),
                    vec![],
                );
                let _ = append(core, vec![a_ev, r_ev]);
            }
        }
    }
    if let Some(id) = slot_id {
        if let Some(s) = core.slots.get_mut(id) {
            s.calls += 1;
        }
    }
    // ── release: only now do bytes reach the downstream ──
    if let Some((rbody, status, mut hdrs)) = released {
        timing.released_ms = Some(core.store.now_ms());
        hdrs.push(("connection".into(), "close".into()));
        let refs: Vec<(&str, &str)> = hdrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let _ = write_response(stream, status, &refs, &rbody);
    }
}

/// One call's assembled context — carried so `finish` can build the
/// §4.3 receipt from any exit point.
struct CallCtx {
    mc: String,
    down_ref: String,
    method: String,
    path: String,
    kept: FilteredHeaders,
    body: Vec<u8>,
    token_hits: Vec<String>,
    surface: Option<Surface>,
    route_label: &'static str,
    mutations: Mutations,
    upstream_refs: Option<(String, String, u16)>,
    released_headers: Option<Vec<(String, String)>>,
    usage: Option<UsageCost>,
}

fn route_surface(route: &Route) -> Option<Surface> {
    match route {
        Route::Surface(s) => Some(*s),
        Route::CountTokens => Some(Surface::AnthropicMessages),
        _ => None,
    }
}

fn route_label(route: &Route) -> &'static str {
    match route {
        Route::Surface(s) => s.as_str(),
        Route::CountTokens => "count_tokens",
        Route::Models => "models",
        Route::Refused => "refused_route",
    }
}

/// A hard-fail path that never mints rows — used only when the ledger
/// or the leak gate itself refused (a `model.call.requested` is
/// already durable for every reachable call).
fn refuse_now(stream: &mut TcpStream, detail: &str) {
    let _ = write_response(
        stream,
        500,
        &[
            ("content-type", "application/json"),
            ("connection", "close"),
        ],
        error_body(None, "hh_proxy", "refused", detail).as_bytes(),
    );
}

/// The assembled receipt — `hh.model_io/1` with the additive
/// `hh.model_proxy.receipt/1` `proxy` member (§4.3).
#[allow(clippy::too_many_arguments)]
fn receipt_doc(
    core: &Core,
    mc: &str,
    slot_id: Option<&str>,
    surface_label: &str,
    method: &str,
    path: &str,
    kept: &FilteredHeaders,
    downstream_ref: &str,
    downstream_bytes: &[u8],
    mutations: &Mutations,
    upstream_refs: Option<(String, String, u16)>,
    released: Option<(String, u16)>,
    released_headers: Option<Vec<(String, String)>>,
    timing: &Timing,
    usage: Option<&UsageCost>,
    reservation: Option<Json>,
    disposition: &str,
    refused: Option<Json>,
    token_hits: &[String],
    seq: u64,
) -> Json {
    let sha = |b: &[u8]| hharness_wire::sha256::sha256_hex(b);
    let mut proxy = BTreeMap::new();
    proxy.insert("schema".into(), Json::str(RECEIPT_SCHEMA));
    proxy.insert("phase_run_id".into(), Json::str(&core.cfg.run_id));
    proxy.insert(
        "slot_id".into(),
        slot_id.map(Json::str).unwrap_or(Json::Null),
    );
    proxy.insert("slot_call_seq".into(), Json::Int(seq as i64));
    proxy.insert("mode".into(), Json::str("live"));
    proxy.insert("proxy_version".into(), Json::str(PROXY_VERSION));
    proxy.insert("surface".into(), Json::str(surface_label));
    let mut dn = BTreeMap::new();
    dn.insert("method".into(), Json::str(method));
    dn.insert("path".into(), Json::str(path));
    dn.insert(
        "header_names".into(),
        Json::Arr(kept.recorded_names.iter().map(Json::str).collect()),
    );
    dn.insert(
        "headers_kept".into(),
        Json::Obj(
            kept.kept
                .iter()
                .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                .collect(),
        ),
    );
    dn.insert("body_ref".into(), Json::str(downstream_ref));
    dn.insert("body_sha256".into(), Json::str(sha(downstream_bytes)));
    dn.insert(
        "body_bytes".into(),
        Json::Int(downstream_bytes.len() as i64),
    );
    proxy.insert("downstream".into(), Json::Obj(dn));
    proxy.insert("mutations".into(), mutations.to_json());
    let mut up = BTreeMap::new();
    match upstream_refs {
        Some((req_ref, resp_ref, status)) => {
            up.insert("dispatched".into(), Json::Bool(true));
            up.insert(
                "endpoint_host".into(),
                Json::str(&core.cfg.lane.upstream_host),
            );
            up.insert("path".into(), Json::str(path));
            up.insert("request_body_ref".into(), Json::str(req_ref));
            up.insert("status".into(), Json::Int(status as i64));
            up.insert("response_body_ref".into(), Json::str(resp_ref));
            up.insert("complete".into(), Json::Bool(true));
        }
        None => {
            up.insert("dispatched".into(), Json::Bool(false));
        }
    }
    proxy.insert("upstream".into(), Json::Obj(up));
    let mut rel = BTreeMap::new();
    match released {
        Some((mode, status)) => {
            rel.insert("status".into(), Json::Int(status as i64));
            rel.insert("release_mode".into(), Json::str(mode));
            rel.insert("identical_to_upstream".into(), Json::Bool(true));
            if let Some(h) = released_headers {
                rel.insert(
                    "headers".into(),
                    Json::Obj(h.into_iter().map(|(k, v)| (k, Json::str(v))).collect()),
                );
            }
        }
        None => {
            rel.insert("status".into(), Json::Null);
            rel.insert("release_mode".into(), Json::Null);
        }
    }
    proxy.insert("released".into(), Json::Obj(rel));
    proxy.insert("timing".into(), timing.to_json());
    let mut uj = BTreeMap::new();
    match usage {
        Some(u) => {
            let mut v = BTreeMap::new();
            v.insert(
                "tokens.input.total".into(),
                Json::Int(u.decomp.input_total()),
            );
            v.insert(
                "tokens.output.total".into(),
                Json::Int(u.decomp.output_total()),
            );
            v.insert(
                "cache_read".into(),
                Json::obj([("available", Json::Bool(u.cache_read_available))]),
            );
            v.insert(
                "reasoning".into(),
                Json::obj([("available", Json::Bool(u.reasoning_available))]),
            );
            uj.insert("view".into(), Json::Obj(v));
            uj.insert("source".into(), Json::str(&u.source));
            let mut c = BTreeMap::new();
            c.insert("derivation".into(), Json::str(&u.derivation));
            c.insert(
                "micros".into(),
                u.micros.map(Json::Int).unwrap_or(Json::Null),
            );
            c.insert(
                "cache_read".into(),
                if u.cache_read_available {
                    Json::Int(u.decomp.cache_read)
                } else {
                    Json::obj([("available", Json::Bool(false))])
                },
            );
            uj.insert("cost".into(), Json::Obj(c));
        }
        None => {
            uj.insert("source".into(), Json::str("absent"));
        }
    }
    proxy.insert("usage".into(), Json::Obj(uj));
    proxy.insert("reservation".into(), reservation.unwrap_or(Json::Null));
    proxy.insert("replay".into(), Json::Null);
    proxy.insert("disposition".into(), Json::str(disposition));
    if let Some(r) = refused {
        proxy.insert("refusal".into(), r);
    }
    if !token_hits.is_empty() {
        proxy.insert(
            "leak_scan".into(),
            Json::obj([(
                "slot_token_hits",
                Json::Arr(token_hits.iter().map(Json::str).collect()),
            )]),
        );
    }
    Json::obj([
        ("schema", Json::str("hh.model_io/1")),
        ("model_call_id", Json::str(mc)),
        (
            "request_body",
            parse_provider(&String::from_utf8_lossy(downstream_bytes)).unwrap_or(Json::Null),
        ),
        ("frames", Json::Arr(vec![])),
        (
            "outcome",
            Json::obj([("disposition", Json::str(disposition))]),
        ),
        ("proxy", Json::Obj(proxy)),
    ])
}
