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
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use hharness_budget::account::Account;
use hharness_budget::attribution::{Attribution, ModelRef};
use hharness_budget::errors::BudgetError;
use hharness_budget::pricing::{
    self, Confidence, CostProvenance, Derivation, ProvenanceClass, SpendRow, SpendSource,
};
use hharness_budget::quantity::{Money, ResourceVector};
use hharness_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hharness_budget::usage::TokenDecomposition;
use hharness_gateway::dialect::WireDialect;
use hharness_gateway::gateway::CredentialHandle;
use hharness_gateway::http_transport::{HttpModelTransport, RawExchange, RawFailure};
use hharness_gateway::snapshot::ModelSnapshotRecord;
use hharness_gateway::vocab::ModelErrorClass;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{EventRef, ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::dimensions::{DimensionId, DimensionKey};
use hharness_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::events::{bound_payload, revoked_payload};
use hharness_secrets::redact::{leak_scan, DetectorSet, ScanTarget};
use hharness_secrets::tokens::{mint_token, token_matches};
use hharness_secrets::{CompositeResolver, MaskEntry, MaskSet, SecretSource, SecretSourceResolver};
use hharness_tls::ClientTls;
use hharness_wire::http::{
    finish_chunked, read_request, split_target, write_chunk, write_response,
    write_response_head_chunked,
};
use hharness_wire::json::{parse_provider, Json, PJson};

use super::apply::{apply_policy, error_body, filter_headers, Mutations, Refusal};
use super::budget::{
    parse_rfc3339_ms, refusal_is_premature, reservation_micros, ProxyAuthorization,
};
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
    /// The HH-recorded, operator-authorized spend basis (REQ-XC-004):
    /// the root pool derives as `carried(A_eff × (1 − m) − Σ priors)`
    /// (REQ-XC-112, REQ-XH-005, REQ-XH-120). A missing authorization
    /// refuses typed at `configure`.
    pub authorization: ProxyAuthorization,
}

/// One registered slot.
struct Slot {
    token: String,
    budget_id: String,
    binding_id: String,
    effort: String,
    /// The declared slice size in µ$ (REQ-XC-126's `s`).
    slice_micros: i64,
    /// Metered spend charged to this slot to date (REQ-XC-126's
    /// premature-refusal predicate input).
    consumed_micros: i64,
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
    /// R6.11 adds the other phase-stopping facts; `phase_stop_reason`
    /// names which fired.
    phase_stopped: bool,
    /// Why the phase stopped (`credential_leak` |
    /// `reservation_bound_violated` | `catalog_drift` |
    /// `budget_exhausted`) — recorded on every later refusal.
    phase_stop_reason: Option<String>,
    /// The catalog-identity baseline tuple (§4.6; canonical Json).
    /// `None` until the first successful probe fixes it.
    catalog_baseline: Option<Json>,
    /// `drift_observed` — a tuple change stopped collection
    /// (REQ-XC-119).
    drift_observed: bool,
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

/// Per-call timing record (§4.3 `timing`). `latency_ms` =
/// released-or-receipted minus received — the field the measured-stage
/// `queue_wait + latency` p99 report reads (REQ-XC-322).
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
        let end = self.released_ms.or(self.receipted_ms).unwrap_or(0);
        Json::obj([
            ("received_ms", Json::Int(self.received_ms as i64)),
            ("reserved_ms", opt(self.reserved_ms)),
            ("dequeued_ms", opt(self.dequeued_ms)),
            ("first_byte_ms", opt(self.first_byte_ms)),
            ("upstream_done_ms", opt(self.upstream_done_ms)),
            ("receipted_ms", opt(self.receipted_ms)),
            ("released_ms", opt(self.released_ms)),
            ("queue_wait_ms", Json::Int(self.queue_wait_ms as i64)),
            (
                "latency_ms",
                Json::Int(end.saturating_sub(self.received_ms) as i64),
            ),
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
    /// `configure` — open the store root, open (or resume) the phase
    /// proxy run under the fenced writer, allocate the root pool on a
    /// fresh run, build the mask set, and bind the loopback listener.
    /// A restart on an existing run acquires the writer lease and
    /// settles every orphan `bounded{0, reserved}` (REQ-XC-124).
    /// Returns the bound address.
    pub fn configure(cfg: ProxyConfig) -> Result<(Proxy, std::net::SocketAddr), ProxyError> {
        if cfg.lane.endpoint_ref.is_empty() {
            return Err(ProxyError::Config("lane endpoint_ref empty".into()));
        }
        // REQ-XC-004 — no authorization, no allocation.
        cfg.authorization.validate()?;
        let mut store =
            Store::open(&cfg.store_root).map_err(|e| ProxyError::Ledger(format!("{e}")))?;
        let resumed = store.has_run(&cfg.run_id);
        let lease = if resumed {
            store
                .acquire_writer(COMPONENT, &cfg.run_id, 3_600_000)
                .map_err(|e| ProxyError::Ledger(format!("resume: {e}")))?
        } else {
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
                    // REQ-XC-004 — the budget allocation record cites
                    // the verbatim operator authorization.
                    ("authorization", cfg.authorization.to_json()),
                ]),
            );
            store
                .open_run_with_id(&cfg.run_id, manifest, COMPONENT)
                .map_err(|e| ProxyError::Ledger(format!("{e}")))?
                .1
        };
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
            phase_stop_reason: None,
            catalog_baseline: None,
            drift_observed: false,
            turn_id: String::new(),
        };
        if resumed {
            // The restart path (REQ-XC-124): adopt the phase turn and
            // the root pool from durable rows, then settle every
            // dispatched-or-reserved call without a terminal —
            // `model.call.failed{proxy_aborted}` charged
            // `bounded{lo:0, hi:reserved}`, the remainder released.
            core.turn_id = core
                .store
                .envelopes(&core.cfg.run_id)
                .map_err(|e| ProxyError::Ledger(format!("{e}")))?
                .iter()
                .find(|e| e.class == "lifecycle.turn.started")
                .and_then(|e| e.payload.get("turn_id").and_then(Json::as_str))
                .map(|s| s.to_string())
                .unwrap_or_else(|| core.store.alloc_id("turn"));
            let root = core
                .store
                .envelopes(&core.cfg.run_id)
                .map_err(|e| ProxyError::Ledger(format!("{e}")))?
                .iter()
                .find(|e| {
                    e.class == "control.budget.allocated" && e.payload.get("parent").is_none()
                })
                .and_then(|e| e.payload.get("budget_id").and_then(Json::as_str))
                .map(|s| s.to_string())
                .ok_or_else(|| ProxyError::Config("resume: no root pool row".into()))?;
            core.cfg.lane.root_budget_id = root;
            recover_orphans(&mut core)?;
        } else {
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
            // The root pool — `spend` hard ceiling = the carried phase
            // cap `A_eff × (1 − m) − Σ priors` (REQ-XC-112, REQ-XH-005,
            // REQ-XH-120; one proxy run per paid phase, one slice per
            // slot under it).
            let pool = cfg.authorization.pool_micros()?;
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
                            pool,
                        )],
                    ),
                )
                .map_err(|e| ProxyError::Config(format!("root pool: {e}")))?
            };
            core.cfg.lane.root_budget_id = root;
        }
        // §4.6 — the run-start catalog probe (REQ-XC-119): `model.profile.probed`
        // with the `/api/tags` + `/api/show` bodies and the identity tuple.
        probe_catalog(&mut core, "run_start")?;
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
                        "phase_stop_reason",
                        self.core
                            .phase_stop_reason
                            .as_deref()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    ("drift_observed", Json::Bool(self.core.drift_observed)),
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
                                        ("consumed_micros", Json::Int(s.consumed_micros)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ]),
            )]),
            // §4.6 — a phase or block boundary re-probes the catalog
            // identity tuple (REQ-XC-119); a change sets drift_observed
            // and stops collection.
            "catalog_boundary" => {
                let label = params
                    .get("label")
                    .and_then(Json::as_str)
                    .unwrap_or("boundary")
                    .to_string();
                match probe_catalog(&mut self.core, &label) {
                    Ok(()) => Json::obj([("ok", Json::obj([]))]),
                    Err(e) => err_json(&format!("{e}")),
                }
            }
            // §4.6 — run end probes once more before teardown.
            "stop" => match probe_catalog(&mut self.core, "run_end") {
                Ok(()) => Json::obj([("ok", Json::obj([]))]),
                Err(e) => err_json(&format!("{e}")),
            },
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
                slice_micros,
                consumed_micros: 0,
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
                        }
                        self.control(op, &d)
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
        bound_violated: false,
        served_model: None,
        served_model_match: None,
    };
    macro_rules! finish {
        ($disposition:expr, $refusal:expr, $upstream:expr, $released:expr, $reservation:expr, $slot:expr, $gone:expr) => {{
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
                $gone,
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
            Some(ReleaseBody::json(
                error_body(
                    call.surface,
                    "hh_proxy",
                    "refused_phase_stopped",
                    &format!(
                        "phase stopped ({})",
                        core.phase_stop_reason
                            .as_deref()
                            .unwrap_or("credential_leak")
                    ),
                )
                .into_bytes(),
                402u16,
            )),
            None,
            slot_id.as_deref(),
            false
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
            Some(ReleaseBody::json(
                error_body(
                    call.surface,
                    "authentication_error",
                    "refused_auth",
                    "invalid slot token",
                )
                .into_bytes(),
                401u16,
            )),
            None,
            None,
            false
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
            Some(ReleaseBody::json(
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
            )),
            None,
            Some(&slot_id),
            false
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
                    Some(ReleaseBody::json(
                        error_body(
                            call.surface,
                            "invalid_request_error",
                            "invalid_body",
                            "request body is not JSON",
                        )
                        .into_bytes(),
                        400u16,
                    )),
                    None,
                    Some(&slot_id),
                    false
                );
                return;
            }
        }
    };
    // streaming — `buffered_heartbeat` (REQ-XC-110): the downstream
    // head commits `200 text/event-stream` at dispatch and `: hh-proxy`
    // comments carry the buffer wait; the buffered bytes (or the
    // surface's in-stream error event) release only after the terminal
    // row is durable. Non-streaming requests release `buffered`.
    let streaming = pbody
        .get("stream")
        .and_then(PJson::as_bool)
        .unwrap_or(false);
    let release_mode: &'static str = if streaming {
        "buffered_heartbeat"
    } else {
        "buffered"
    };
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
                    Some(ReleaseBody::json(
                        error_body(call.surface, "invalid_request_error", r.kind, &r.detail)
                            .into_bytes(),
                        r.status,
                    )),
                    None,
                    Some(&slot_id),
                    false
                );
                return;
            }
        }
    }
    // ── step 2: the worst-case reservation before queueing
    // (REQ-XC-113) — `ceil(rate_in·(body_bytes + template_overhead) +
    // rate_out·max_output_bound)` at the uncached input rate for
    // `Formula` lanes; `FixtureBound` reserves its declared bound. A
    // slot-level refusal is a ledgered 402 (REQ-XC-126 records
    // requested/available and the premature classification); only a
    // refusal the ROOT binds emits `control.budget.exceeded` +
    // escalation + `decision{stop}` and stops the phase (REQ-XC-112;
    // serial handling holds E1's drain — no in-flight call exists at
    // a reserve-refusal point under one writer thread). ──
    let slot_budget = core
        .slots
        .get(&slot_id)
        .map(|s| s.budget_id.clone())
        .unwrap_or_default();
    let slice_micros = core
        .slots
        .get(&slot_id)
        .map(|s| s.slice_micros)
        .unwrap_or(0);
    let table = pricing::ollama_glm_5_3_flash();
    // `w` — one worst-case call reservation at the declared bound
    // (REQ-XC-126); fixture lanes declare it directly.
    let w_micros = match &core.cfg.lane.reservation {
        Reservation::FixtureBound { micros } => *micros,
        Reservation::Formula(f) => reservation_micros(
            f.declared_body_bytes,
            f.template_overhead_tokens,
            f.max_output_bound,
            f.cached_share_ppm,
            &table,
        )
        .unwrap_or(0),
        Reservation::Unsupported => 0,
    };
    let (call_micros, bound_json): (i64, Json) = match &core.cfg.lane.reservation {
        Reservation::FixtureBound { micros } => {
            (*micros, Json::obj([("kind", Json::str("fixture_bound"))]))
        }
        Reservation::Formula(f) => {
            match reservation_micros(
                body.len() as i64,
                f.template_overhead_tokens,
                f.max_output_bound,
                f.cached_share_ppm,
                &table,
            ) {
                Ok(m) => (
                    m,
                    Json::obj([
                        ("kind", Json::str("worst_case")),
                        (
                            "formula",
                            Json::str(
                                "ceil(rate_in*(body_bytes+template_overhead)+rate_out*max_output_bound)",
                            ),
                        ),
                        ("body_bytes", Json::Int(body.len() as i64)),
                        (
                            "template_overhead_tokens",
                            Json::Int(f.template_overhead_tokens),
                        ),
                        ("max_output_bound", Json::Int(f.max_output_bound)),
                        (
                            "input_tokens_bound",
                            Json::Int(body.len() as i64 + f.template_overhead_tokens),
                        ),
                        (
                            "cached_share_ppm",
                            f.cached_share_ppm.map(Json::Int).unwrap_or(Json::Null),
                        ),
                        ("max_tokens_policy", Json::str(&f.max_tokens_policy)),
                    ]),
                ),
                Err(e) => {
                    refuse_now(&mut stream, &format!("{e}"));
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
                    detail: "unsupported{reservation_formula}".into(),
                }),
                None,
                Some(ReleaseBody::json(
                    error_body(
                        call.surface,
                        "invalid_request_error",
                        "unsupported{reservation_formula}",
                        "lane declares no reservation form",
                    )
                    .into_bytes(),
                    402u16,
                )),
                None,
                Some(&slot_id),
                false
            );
            return;
        }
    };
    let reservation: Option<Json> = match do_reserve(core, &slot_budget, &mc, call_micros) {
        Ok(rid) => Some(Json::obj([
            ("id", Json::str(rid)),
            ("micros", Json::Int(call_micros)),
            ("bound", bound_json),
        ])),
        Err(fail) => {
            let premature = refusal_is_premature(fail.slot_consumed, slice_micros, w_micros);
            // REQ-XC-126 — every refusal records requested/available and
            // the premature classification (`inconclusive{infra:
            // budget_reservation}` below `s − w`, `budget_refused` at
            // or above it).
            let rec = Json::obj([
                ("refused", Json::Bool(true)),
                ("requested_micros", Json::Int(fail.requested)),
                ("available_micros", Json::Int(fail.available)),
                ("slot_consumed_micros", Json::Int(fail.slot_consumed)),
                ("premature", Json::Bool(premature)),
                (
                    "classification",
                    Json::str(if premature {
                        "inconclusive{infra:budget_reservation}"
                    } else {
                        "budget_refused"
                    }),
                ),
            ]);
            if fail.root_binds {
                exhaust_root(core, &fail);
            }
            finish!(
                "refused_budget",
                Some(Refusal {
                    status: 402,
                    kind: "refused_budget",
                    path: None,
                    detail: fail.detail.clone(),
                }),
                None,
                Some(ReleaseBody::json(
                    error_body(
                        call.surface,
                        "invalid_request_error",
                        "budget_refused",
                        "slot budget refused the reservation",
                    )
                    .into_bytes(),
                    402u16,
                )),
                Some(rec),
                Some(&slot_id),
                false
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
    // ── upstream dispatch — one spawned transport per call (the
    // upstream socket is the only blocking leg; `Store` stays on the
    // writer thread). Non-streaming rides the lane's declared
    // `nonstream_read_deadline_ms` (REQ-XC-111); streaming commits the
    // SSE head and heartbeats downstream while buffering (F9). ──
    let op = match route {
        Route::Surface(s) => s.upstream_op(),
        _ => "models",
    };
    let dialect: WireDialect = match route {
        Route::Surface(Surface::AnthropicMessages) => hharness_gateway::dialects::ollama_messages(),
        _ => hharness_gateway::dialects::ollama_chat(),
    };
    let rx = dispatch_upstream(DispatchPlan {
        dialect,
        endpoint: core.cfg.lane.endpoint_ref.clone(),
        method: if matches!(route, Route::Models) {
            "GET".to_string()
        } else {
            "POST".to_string()
        },
        op: op.to_string(),
        kept_headers: call.kept.kept.clone(),
        body: upstream_body.clone(),
        credential_env: core.cfg.credential_env.clone(),
        rw_ms: if streaming {
            None
        } else {
            core.cfg.lane.nonstream_read_deadline_ms
        },
    });
    let mut gone_hint = false;
    let dispatched = if streaming {
        // `buffered_heartbeat` — the `200 text/event-stream` head
        // commits now (it carries no response bytes; X-04 holds for
        // the body); `: hh-proxy` comments carry the wait.
        if write_response_head_chunked(
            &mut stream,
            200,
            &[
                ("content-type", "text/event-stream"),
                ("cache-control", "no-cache"),
                ("connection", "close"),
            ],
        )
        .is_err()
        {
            gone_hint = true;
        }
        let hb = Duration::from_millis(core.cfg.lane.heartbeat_ms.max(1));
        loop {
            match rx.recv_timeout(hb) {
                Ok(r) => break Some(r),
                Err(RecvTimeoutError::Timeout) => {
                    if !gone_hint && write_chunk(&mut stream, b": hh-proxy\n\n").is_err() {
                        gone_hint = true;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break None,
            }
        }
    } else {
        rx.recv().ok()
    };
    gate.release();
    timing.upstream_done_ms = Some(core.store.now_ms());
    let dispatched = match dispatched {
        Some(v) => v,
        None => {
            refuse_now(&mut stream, "upstream dispatch thread died");
            return;
        }
    };
    match dispatched {
        Err(f) => {
            let detail = format!("{}", f.error);
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
            // §4.8 — a head that never arrived is `upstream_error` /
            // `upstream_timeout`; a failure after the head is a broken
            // stream. Partial bytes are kept in the blob either way.
            let timeout = matches!(f.error.class, ModelErrorClass::Timeout(_));
            let broken = f.partial.status != 0;
            let disposition = if timeout {
                "upstream_timeout"
            } else if broken {
                "stream_broken"
            } else {
                "upstream_error"
            };
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
            if f.partial.status != 0 || !f.partial.body.is_empty() {
                match deposit(core, "upstream_response", &f.partial.body, BLOB_MEDIA) {
                    Ok((r, h)) => {
                        call.token_hits.extend(h);
                        call.upstream_refs = Some((req_ref, Some(r), f.partial.status, false));
                    }
                    Err(e) => {
                        refuse_now(&mut stream, &e.to_string());
                        return;
                    }
                }
            } else {
                call.upstream_refs = Some((req_ref, None, 0, false));
            }
            let status = if timeout { 504u16 } else { 502u16 };
            let (rbytes, ctype) = if streaming {
                (
                    in_stream_error(call.surface, disposition, "upstream transport failed"),
                    "text/event-stream",
                )
            } else {
                (
                    error_body(
                        call.surface,
                        "api_error",
                        disposition,
                        "upstream transport failed",
                    )
                    .into_bytes(),
                    "application/json",
                )
            };
            finish!(
                disposition,
                None,
                None,
                Some(ReleaseBody {
                    bytes: rbytes,
                    status,
                    headers: vec![("content-type".to_string(), ctype.to_string())],
                    release_mode,
                    identical_to_upstream: false,
                    head_committed: streaming,
                }),
                reservation,
                Some(&slot_id),
                gone_hint
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
            call.upstream_refs = Some((req_ref, Some(resp_ref), ex.status, true));
            // REQ-XC-114 — reported input tokens above the reservation
            // bound halt new admissions phase-wide.
            if let (Reservation::Formula(f), Some(u)) = (&core.cfg.lane.reservation, &call.usage) {
                if u.decomp.input_total() > body.len() as i64 + f.template_overhead_tokens {
                    call.bound_violated = true;
                    core.phase_stopped = true;
                    core.phase_stop_reason = Some("reservation_bound_violated".to_string());
                }
            }
            // REQ-XC-119 — the served model checks against the
            // lane-declared expected set (recorded, never guessed).
            served_model_check(core, &mut call, &ex);
            // §4.8 — an upstream error status is still a completed
            // attempt; the call is `upstream_error`. `buffered`
            // passes status/body/`Retry-After` verbatim; the committed
            // heartbeat head forces the in-stream error event.
            if ex.status >= 400 {
                let rbytes = if streaming {
                    in_stream_error(
                        call.surface,
                        "upstream_error",
                        &format!("upstream {}", ex.status),
                    )
                } else {
                    ex.body.clone()
                };
                finish!(
                    "upstream_error",
                    None,
                    None,
                    Some(ReleaseBody {
                        bytes: rbytes,
                        status: ex.status,
                        headers: if streaming {
                            vec![("content-type".to_string(), "text/event-stream".to_string())]
                        } else {
                            call.released_headers.clone().unwrap_or_default()
                        },
                        release_mode,
                        identical_to_upstream: !streaming,
                        head_committed: streaming,
                    }),
                    reservation,
                    Some(&slot_id),
                    gone_hint
                );
                return;
            }
            // `stream_broken` — a streaming call whose captured body
            // is no SSE stream at all (empty or no events).
            if streaming && !looks_like_sse(&ex.body) {
                finish!(
                    "stream_broken",
                    None,
                    None,
                    Some(ReleaseBody {
                        bytes: in_stream_error(
                            call.surface,
                            "stream_broken",
                            "upstream served no SSE stream",
                        ),
                        status: 502u16,
                        headers: vec![(
                            "content-type".to_string(),
                            "text/event-stream".to_string(),
                        )],
                        release_mode,
                        identical_to_upstream: false,
                        head_committed: true,
                    }),
                    reservation,
                    Some(&slot_id),
                    gone_hint
                );
                return;
            }
            finish!(
                "forwarded",
                None,
                None,
                Some(ReleaseBody {
                    bytes: ex.body.clone(),
                    status: ex.status,
                    headers: call.released_headers.clone().unwrap_or_default(),
                    release_mode,
                    identical_to_upstream: true,
                    head_committed: streaming,
                }),
                reservation,
                Some(&slot_id),
                gone_hint
            );
        }
    }
}

/// The released downstream response (the §4.3 `released` member's
/// content): `buffered` writes a complete `Content-Length` response;
/// `buffered_heartbeat` already committed the chunked SSE head, so the
/// body (the buffered upstream bytes or the in-stream error event)
/// goes out as chunks + the terminal zero-chunk.
struct ReleaseBody {
    bytes: Vec<u8>,
    status: u16,
    headers: Vec<(String, String)>,
    release_mode: &'static str,
    identical_to_upstream: bool,
    head_committed: bool,
}

impl ReleaseBody {
    /// A refusal's JSON error body — released `buffered`, unrelated to
    /// any upstream response.
    fn json(bytes: Vec<u8>, status: u16) -> ReleaseBody {
        ReleaseBody {
            bytes,
            status,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            release_mode: "buffered",
            identical_to_upstream: false,
            head_committed: false,
        }
    }
}

/// The §4.8 client-abandonment leg: the peer closed the connection's
/// request channel before release — `peek` sees the pending FIN.
fn client_gone(stream: &TcpStream) -> bool {
    match stream.try_clone() {
        Ok(s) => {
            let _ = s.set_nonblocking(true);
            let mut b = [0u8; 1];
            match s.peek(&mut b) {
                Ok(0) => true,
                Ok(_) => false,
                Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
            }
        }
        Err(_) => true,
    }
}

/// The §4.3 tail for every call — deposit the `hh.model_io/1` receipt
/// blob (step 5), land the terminal `model.call.*` row (step 6), settle
/// the reservation (step 7), and only then write the response bytes
/// (durable-before-visible, X-04). A client that abandoned before
/// release is receipted `released: client_gone` and still charged
/// (REQ-XC-322).
#[allow(clippy::too_many_arguments)]
fn finish(
    core: &mut Core,
    stream: &mut TcpStream,
    timing: &mut Timing,
    call: &mut CallCtx,
    disposition: &str,
    refusal: Option<Refusal>,
    _upstream: Option<()>,
    released: Option<ReleaseBody>,
    reservation: Option<Json>,
    slot_id: Option<&str>,
    gone_hint: bool,
) {
    let run_id = core.cfg.run_id.clone();
    let seq = slot_id
        .and_then(|id| core.slots.get(id))
        .map(|s| s.calls)
        .unwrap_or(0);
    // §4.8 — the client abandoned before release: the receipt says
    // `client_gone`, the charge still lands.
    let gone = gone_hint || (released.is_some() && client_gone(stream));
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
        released.as_ref(),
        gone,
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
    p.insert(
        "release_mode".into(),
        released
            .as_ref()
            .map(|r| Json::str(r.release_mode))
            .unwrap_or(Json::Null),
    );
    if gone {
        p.insert("client_gone".into(), Json::Bool(true));
    }
    if call.bound_violated {
        p.insert("reservation_bound_violated".into(), Json::Bool(true));
    }
    if let Some(s) = &call.served_model {
        p.insert("served_model".into(), Json::str(s));
    }
    if let Some(m) = call.served_model_match {
        p.insert("served_model_match".into(), Json::Bool(m));
    }
    if matches!(
        disposition,
        "upstream_error" | "upstream_timeout" | "stream_broken" | "proxy_aborted"
    ) {
        // REQ-XC-115 — receipted infrastructure events (the REQ-XC-323
        // slot classifier is R6.25's).
        p.insert("infra".into(), Json::Bool(true));
    }
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
    // an intermediate prefix. A failed call with a held reservation
    // releases its claim without a charge (usage unavailable →
    // unknown exposure, never an implied zero). ──
    if let Some(res) = reservation.as_ref() {
        let rid = res
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let bound = res.get("micros").and_then(Json::as_int).unwrap_or(0);
        // Refusal records carry no `id`/`micros` — nothing was held,
        // nothing to release.
        if !rid.is_empty() && bound > 0 {
            let micros = uc.as_ref().and_then(|u| u.micros);
            if let Some(slot) = slot_id {
                let slot_budget = core
                    .slots
                    .get(slot)
                    .map(|s| s.budget_id.clone())
                    .unwrap_or_default();
                let mut batch = vec![];
                if let Some(micros) = micros {
                    let u = uc.as_ref().unwrap();
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
                        batch.push(mint(
                            core,
                            "measurement.cost.attributed",
                            hharness_budget::events::cost_attributed_payload(&row),
                            Scope::default(),
                            vec![],
                        ));
                        if let Some(s) = core.slots.get_mut(slot) {
                            s.consumed_micros += micros;
                        }
                    }
                }
                let mut left = ResourceVector::zero();
                left.add(DimensionId::Spend, bound);
                batch.push(mint(
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
                ));
                let _ = append(core, batch);
            }
        }
    }
    if let Some(id) = slot_id {
        if let Some(s) = core.slots.get_mut(id) {
            s.calls += 1;
        }
    }
    // ── release: only now do bytes reach the downstream — and only
    // when the client is still there to read them. ──
    if let Some(rb) = released {
        if !gone {
            timing.released_ms = Some(core.store.now_ms());
            if rb.head_committed {
                // `buffered_heartbeat` — the SSE head went out at
                // dispatch; the buffered body + the terminal
                // zero-chunk close the stream.
                let _ = write_chunk(stream, &rb.bytes);
                let _ = finish_chunked(stream);
            } else {
                let mut hdrs = rb.headers.clone();
                hdrs.push(("connection".into(), "close".into()));
                let refs: Vec<(&str, &str)> =
                    hdrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                let _ = write_response(stream, rb.status, &refs, &rb.bytes);
            }
        }
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
    /// `(request_ref, response_ref?, status, complete)` — a head that
    /// never arrived carries `status = 0`, `response_ref = None`,
    /// `complete = false` (§4.8's stream-broken leg).
    upstream_refs: Option<(String, Option<String>, u16, bool)>,
    released_headers: Option<Vec<(String, String)>>,
    usage: Option<UsageCost>,
    /// REQ-XC-114 — the reported input bound was violated (the row is
    /// flagged and the phase stops admitting).
    bound_violated: bool,
    /// The response's served-model spelling (recorded, never guessed).
    served_model: Option<String>,
    /// `Some(false)` when the lane declared `served_model_expected`
    /// and the served spelling is outside it.
    served_model_match: Option<bool>,
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
    upstream_refs: Option<(String, Option<String>, u16, bool)>,
    released: Option<&ReleaseBody>,
    client_gone_flag: bool,
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
        Some((req_ref, resp_ref, status, complete)) => {
            up.insert("dispatched".into(), Json::Bool(true));
            up.insert(
                "endpoint_host".into(),
                Json::str(&core.cfg.lane.upstream_host),
            );
            up.insert("path".into(), Json::str(path));
            up.insert("request_body_ref".into(), Json::str(req_ref));
            up.insert(
                "status".into(),
                if status == 0 {
                    Json::Null
                } else {
                    Json::Int(status as i64)
                },
            );
            up.insert(
                "response_body_ref".into(),
                resp_ref.map(Json::str).unwrap_or(Json::Null),
            );
            up.insert("complete".into(), Json::Bool(complete));
        }
        None => {
            up.insert("dispatched".into(), Json::Bool(false));
        }
    }
    proxy.insert("upstream".into(), Json::Obj(up));
    let mut rel = BTreeMap::new();
    match released {
        Some(rb) => {
            // §4.8 — `released: client_gone` (charged either way).
            rel.insert(
                "status".into(),
                if client_gone_flag {
                    Json::str("client_gone")
                } else {
                    Json::Int(rb.status as i64)
                },
            );
            rel.insert("release_mode".into(), Json::str(rb.release_mode));
            rel.insert(
                "identical_to_upstream".into(),
                Json::Bool(rb.identical_to_upstream && !client_gone_flag),
            );
            if !rb.headers.is_empty() {
                rel.insert(
                    "headers".into(),
                    Json::Obj(
                        rb.headers
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v)))
                            .collect(),
                    ),
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

// ─────────────────────────────────────────────────────────────────────
// R6.11 — reservation, exhaustion, dispatch, catalog and recovery
// helpers (§4.4, §4.6, §4.8; REQ-XC-110…115/119/123/124/126/210/322,
// REQ-XC-004, REQ-XH-005/120).
// ─────────────────────────────────────────────────────────────────────

/// A reserve failure's recorded facts (REQ-XC-126's `requested` /
/// `available` + the premature-refusal predicate inputs).
struct ReserveFail {
    requested: i64,
    available: i64,
    slot_consumed: i64,
    /// The refusal binds at the root (not the slot slice) — the
    /// caller emits the root-exhaustion rows (REQ-XC-112).
    root_binds: bool,
    root_level: i64,
    root_hard: i64,
    detail: String,
}

/// `do_reserve` — the step-2 reservation on the slot's slice. The
/// `reserved{refused}` row lands inside `Account::reserve` either way.
fn do_reserve(
    core: &mut Core,
    slot_budget: &str,
    mc: &str,
    micros: i64,
) -> Result<String, ReserveFail> {
    let key = DimensionKey::parse("spend").unwrap_or(DimensionKey::Primary(DimensionId::Spend));
    let fail = |requested: i64, available: i64, detail: String| ReserveFail {
        requested,
        available,
        slot_consumed: 0,
        root_binds: false,
        root_level: 0,
        root_hard: 0,
        detail,
    };
    let mut acct = match Account::open(&mut core.store, &core.cfg.run_id) {
        Ok(a) => a,
        Err(e) => return Err(fail(micros, 0, format!("{e}"))),
    };
    let slot_consumed = acct
        .tree
        .nodes
        .get(slot_budget)
        .map(|n| n.consumed.key_total(key))
        .unwrap_or(0);
    let mut q = ResourceVector::zero();
    q.add(DimensionId::Spend, micros);
    match acct.reserve(&core.lease, slot_budget, &q, &format!("mc:{mc}"), 300_000) {
        Ok(rid) => Ok(rid),
        Err(e) => {
            let (requested, available) = match &e {
                BudgetError::InsufficientBudget {
                    requested,
                    available,
                    ..
                } => (*requested, *available),
                _ => (micros, 0),
            };
            let (root_level, root_hard) = acct
                .tree
                .nodes
                .get(&core.cfg.lane.root_budget_id)
                .map(|n| (acct.tree.level(n, key), n.node.spec.hard(key).unwrap_or(0)))
                .unwrap_or((0, 0));
            Err(ReserveFail {
                requested,
                available,
                slot_consumed,
                root_binds: root_hard > 0 && root_level + requested > root_hard,
                root_level,
                root_hard,
                detail: format!("{e}"),
            })
        }
    }
}

/// `exhaust_root` — REQ-XC-112's root-only stop sequence:
/// `control.budget.exceeded` + `lifecycle.escalation.raised` +
/// `control.decision{stop, budget_exhausted{spend}}`, one batch.
/// Serial handling holds E1's drain (no in-flight call exists at a
/// reserve-refusal point — the refusing call is pre-dispatch and
/// every dispatched call is already terminal).
fn exhaust_root(core: &mut Core, fail: &ReserveFail) {
    let root = core.cfg.lane.root_budget_id.clone();
    let key = DimensionKey::parse("spend").expect("spend key");
    let exceeded = hharness_budget::tree::Exceeded {
        budget_id: root.clone(),
        dimension: key,
        value: fail.root_level + fail.requested,
        limit: fail.root_hard,
    };
    let e1 = mint(
        core,
        "control.budget.exceeded",
        hharness_budget::events::exceeded_payload(&exceeded),
        Scope::default(),
        vec![],
    );
    let e2 = mint(
        core,
        "lifecycle.escalation.raised",
        Json::obj([
            ("subject", Json::str(&root)),
            ("budget_id", Json::str(&root)),
            ("kind", Json::str("budget_hard")),
            ("reason", Json::str("spend")),
        ]),
        Scope::default(),
        vec![],
    );
    let e3 = mint(
        core,
        "control.decision",
        hharness_budget::events::decision_payload(
            "stop",
            COMPONENT,
            Some(&hharness_budget::events::stop_reason_budget_exhausted(key)),
            Some(&root),
            Some(key),
            None,
            None,
        ),
        Scope::default(),
        vec![],
    );
    let _ = append(core, vec![e1, e2, e3]);
    core.phase_stopped = true;
    core.phase_stop_reason = Some("budget_exhausted".to_string());
}

/// The spawned upstream call's parameter pack.
struct DispatchPlan {
    dialect: WireDialect,
    endpoint: String,
    method: String,
    op: String,
    kept_headers: Vec<(String, String)>,
    body: Vec<u8>,
    credential_env: Option<String>,
    /// `Some(ms)` overrides the dialect's stream-idle read deadline —
    /// the lane-declared `nonstream_read_deadline_ms` (REQ-XC-111).
    rw_ms: Option<u64>,
}

/// `dispatch_upstream(plan)` — one spawned transport per call. The
/// credential resolves inside the spawned frame and drops on return
/// (X-05); `Store` never leaves the writer thread.
fn dispatch_upstream(plan: DispatchPlan) -> Receiver<Result<RawExchange, Box<RawFailure>>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut t = HttpModelTransport::with_credentials(
            plan.dialect,
            ClientTls::webpki_roots(),
            Box::<CompositeResolver>::default(),
        );
        if let Some(ms) = plan.rw_ms {
            t.set_rw_ms(ms);
        }
        let cred = CredentialHandle {
            binding_id: plan
                .credential_env
                .clone()
                .map(|v| format!("launch_env:{v}")),
            provided: plan.credential_env.is_some(),
        };
        let hdrs: Vec<(&str, &str)> = plan
            .kept_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let _ = tx.send(t.forward_raw_capture(
            &plan.method,
            &cred,
            &plan.endpoint,
            &plan.op,
            &hdrs,
            &plan.body,
            None,
        ));
    });
    rx
}

/// `looks_like_sse(body)` — a captured streaming body carries at
/// least one `data:` event; empty or event-free bodies are
/// `stream_broken` (§4.8's "broken or empty stream" leg).
fn looks_like_sse(body: &[u8]) -> bool {
    !body.is_empty() && body.windows(5).any(|w| w == b"data:")
}

/// `in_stream_error(surface, kind, detail)` — the surface's in-stream
/// error event for a late failure under `buffered_heartbeat` (§4.8):
/// `event: error\ndata: {surface-shaped error}`.
fn in_stream_error(surface: Option<Surface>, kind: &str, detail: &str) -> Vec<u8> {
    let body = error_body(surface, "api_error", kind, detail);
    format!("event: error\ndata: {body}\n\n").into_bytes()
}

/// `served_model_check` — REQ-XC-119's per-call served-model check:
/// record the response's `model` spelling and evaluate it against the
/// lane-declared expected set; a mismatch is ledgered
/// (`model.profile.status.changed{served_model_mismatch}`), never
/// hidden.
fn served_model_check(core: &mut Core, call: &mut CallCtx, ex: &RawExchange) {
    let doc = parse_provider(&String::from_utf8_lossy(&ex.body)).unwrap_or(Json::Null);
    let served = doc
        .get("model")
        .and_then(Json::as_str)
        .map(|s| s.to_string());
    call.served_model = served.clone();
    if !core.cfg.lane.served_model_expected.is_empty() {
        if let Some(s) = &served {
            let hit = core.cfg.lane.served_model_expected.iter().any(|e| e == s);
            call.served_model_match = Some(hit);
            if !hit {
                let ev = mint(
                    core,
                    "model.profile.status.changed",
                    Json::obj([
                        (
                            "profile_ref",
                            Json::str(format!("hh.proxy.lane:{}", core.cfg.lane.lane_id)),
                        ),
                        ("trigger", Json::str("served_model_mismatch")),
                        ("served_model", Json::str(s)),
                        ("model_call_id", Json::str(&call.mc)),
                    ]),
                    Scope::default(),
                    vec![],
                );
                let _ = append(core, vec![ev]);
            }
        }
    }
}

/// `catalog_tuple(tags_body, show_body, model)` — the §4.6 catalog
/// identity tuple: `{name, tags digest, modified_at (compared as an
/// instant), parameter_count, quantization_level, context_length,
/// thinking{values, default}}` (REQ-XC-119). Fields outside the tuple
/// are recorded descriptively, never alarmed.
fn catalog_tuple(tags: &[u8], show: &[u8], model: &str) -> Option<Json> {
    let t = parse_provider(&String::from_utf8_lossy(tags)).unwrap_or(Json::Null);
    let s = parse_provider(&String::from_utf8_lossy(show)).unwrap_or(Json::Null);
    let models = match t.get("models") {
        Some(Json::Arr(v)) => v,
        _ => return None,
    };
    let entry = models.iter().find(|m| {
        m.get("name").and_then(Json::as_str) == Some(model)
            || m.get("model").and_then(Json::as_str) == Some(model)
    })?;
    let digest = entry
        .get("digest")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let modified = entry
        .get("modified_at")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let modified_ms = parse_rfc3339_ms(&modified).unwrap_or(-1);
    let details = s.get("details").cloned().unwrap_or(Json::Null);
    let member = |k: &str| details.get(k).cloned().unwrap_or(Json::Null);
    // context length — any `model_info` member ending `.context_length`
    // (GLM-family keys are `<arch>.context_length`).
    let ctx = match s.get("model_info") {
        Some(Json::Obj(m)) => m
            .iter()
            .find(|(k, _)| k.ends_with(".context_length") || k.as_str() == "context_length")
            .map(|(_, v)| v.clone())
            .unwrap_or(Json::Null),
        _ => Json::Null,
    };
    let thinking = s
        .get("thinking")
        .or_else(|| s.get("think"))
        .cloned()
        .unwrap_or(Json::Null);
    Some(Json::obj([
        ("name", Json::str(model)),
        ("digest", Json::str(&digest)),
        ("modified_at_ms", Json::Int(modified_ms)),
        ("parameter_count", member("parameter_size")),
        ("quantization_level", member("quantization_level")),
        ("context_length", ctx),
        ("thinking", thinking),
    ]))
}

/// `probe_catalog(core, boundary)` — REQ-XC-119: fetch `/api/tags` +
/// `/api/show`, deposit both bodies (leak-scanned like every blob),
/// extract the identity tuple into `ModelSnapshotRecord.snapshot_id`
/// (`pinned = false`, `provenance = reported`) and land
/// `model.profile.probed`. A tuple change sets `drift_observed` and
/// stops collection; a probe failure is recorded, never hidden.
fn probe_catalog(core: &mut Core, boundary: &str) -> Result<(), ProxyError> {
    let Some(base) = core.cfg.lane.catalog_base() else {
        return Ok(());
    };
    let fetch = |method: &str, op: &str, body: &[u8]| -> Result<RawExchange, ProxyError> {
        dispatch_upstream(DispatchPlan {
            dialect: hharness_gateway::dialects::ollama_chat(),
            endpoint: base.clone(),
            method: method.to_string(),
            op: op.to_string(),
            kept_headers: vec![],
            body: body.to_vec(),
            credential_env: core.cfg.credential_env.clone(),
            rw_ms: None,
        })
        .recv()
        .map_err(|_| ProxyError::Io("catalog dispatch thread died".into()))?
        .map_err(|f| ProxyError::Io(format!("catalog {op}: {}", f.error)))
    };
    let show_body = format!("{{\"model\":\"{}\"}}", core.cfg.lane.model);
    let results = (
        fetch("GET", "api/tags", &[]),
        fetch("POST", "api/show", show_body.as_bytes()),
    );
    let (tags_ex, show_ex) = match results {
        (Ok(t), Ok(s)) => (t, s),
        (t, s) => {
            let detail = match (t, s) {
                (Err(e), _) | (_, Err(e)) => format!("{e}"),
                _ => "unreachable".to_string(),
            };
            let rec = Json::obj([
                ("kind", Json::str("ollama_catalog")),
                ("boundary", Json::str(boundary)),
                ("probe_error", Json::str(&detail)),
            ]);
            let payload = hharness_gateway::events::profile_probed(
                &format!("hh.proxy.lane:{}", core.cfg.lane.lane_id),
                &core.cfg.run_id,
                &[rec],
                None,
            );
            let ev = mint(
                core,
                "model.profile.probed",
                payload,
                Scope::default(),
                vec![],
            );
            return append(core, vec![ev]);
        }
    };
    let (tags_ref, _) = deposit(core, "catalog_tags", &tags_ex.body, BLOB_MEDIA)?;
    let (show_ref, _) = deposit(core, "catalog_show", &show_ex.body, BLOB_MEDIA)?;
    let tuple = catalog_tuple(&tags_ex.body, &show_ex.body, &core.cfg.lane.model);
    let drift = match (&core.catalog_baseline, &tuple) {
        (Some(b), Some(t)) => *b != *t,
        _ => false,
    };
    if core.catalog_baseline.is_none() && tuple.is_some() {
        core.catalog_baseline = tuple.clone();
    }
    if drift {
        core.drift_observed = true;
        core.phase_stopped = true;
        core.phase_stop_reason = Some("catalog_drift".to_string());
    }
    // The tuple goes in `ModelSnapshotRecord.snapshot_id`
    // (`pinned = false`, `provenance = reported` — the provider's
    // claim lands as an imported origin, CC2).
    let snapshot = tuple.as_ref().map(|t| {
        let digest = t.get("digest").and_then(Json::as_str).unwrap_or("");
        let modified = t
            .get("modified_at_ms")
            .and_then(Json::as_int)
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        let d12: String = digest.chars().take(12).collect();
        ModelSnapshotRecord {
            provider: "ollama-cloud".to_string(),
            model_id: core.cfg.lane.model.clone(),
            snapshot_id: Some(format!("tags:{d12}@{modified}")),
            pinned: false,
            observed_fingerprint: None,
            provenance: ProvenanceRecord::minted(
                Origin::Import {
                    source_system: "provider:ollama-cloud".to_string(),
                    mapping_version: "ollama-catalog/1".to_string(),
                },
                PersistenceScope::Run,
                core.store.now_ms(),
            ),
        }
        .to_json()
    });
    let rec = Json::obj([
        ("kind", Json::str("ollama_catalog")),
        ("boundary", Json::str(boundary)),
        ("tuple", tuple.clone().unwrap_or(Json::Null)),
        ("snapshot", snapshot.unwrap_or(Json::Null)),
        ("tags_ref", Json::str(&tags_ref)),
        ("show_ref", Json::str(&show_ref)),
        ("drift_observed", Json::Bool(drift)),
        (
            "served_model_expected",
            Json::Arr(
                core.cfg
                    .lane
                    .served_model_expected
                    .iter()
                    .map(Json::str)
                    .collect(),
            ),
        ),
    ]);
    let payload = hharness_gateway::events::profile_probed(
        &format!("hh.proxy.lane:{}", core.cfg.lane.lane_id),
        &core.cfg.run_id,
        &[rec],
        None,
    );
    let ev = mint(
        core,
        "model.profile.probed",
        payload,
        Scope::default(),
        vec![],
    );
    append(core, vec![ev])
}

/// `recover_orphans(core)` — REQ-XC-124's restart settlement: every
/// held `control.budget.reserved` whose `mc:*` holder has no terminal
/// `model.call.*` row gets a synthesized
/// `model.call.failed{proxy_aborted}`, is charged
/// `bounded{lo:0, hi:reserved}` (metered at `hi`), and the remainder
/// released — one batch per orphan, so the budget invariant holds at
/// every durable prefix.
fn recover_orphans(core: &mut Core) -> Result<(), ProxyError> {
    let envs: Vec<(String, String, Json)> = core
        .store
        .envelopes(&core.cfg.run_id)
        .map_err(|e| ProxyError::Ledger(format!("{e}")))?
        .iter()
        .map(|e| (e.class.clone(), e.event_id.clone(), e.payload.clone()))
        .collect();
    let terminals: std::collections::BTreeSet<String> = envs
        .iter()
        .filter(|(c, _, _)| c == "model.call.completed" || c == "model.call.failed")
        .filter_map(|(_, _, p)| {
            p.get("model_call_id")
                .and_then(Json::as_str)
                .map(|s| s.to_string())
        })
        .collect();
    for (class, _eid, p) in &envs {
        if class != "control.budget.reserved"
            || p.get("outcome").and_then(Json::as_str) != Some("held")
        {
            continue;
        }
        let rid = p
            .get("reservation_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let holder = p
            .get("holder")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let mc = holder.strip_prefix("mc:").unwrap_or(&holder).to_string();
        if terminals.contains(&mc) {
            continue;
        }
        // A prior recovery pass already settled this claim.
        let already = envs.iter().any(|(c, _, q)| {
            c == "control.budget.released"
                && q.get("reservation_id").and_then(Json::as_str) == Some(rid.as_str())
        });
        if already {
            continue;
        }
        let micros = p
            .get("quantity")
            .and_then(|q| q.get("spend"))
            .and_then(Json::as_int)
            .unwrap_or(0);
        let budget_id = p
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        // (a) the synthesized terminal — `usage` absent folds as
        // unknown exposure, never an implied zero.
        let f_ev = mint(
            core,
            "model.call.failed",
            Json::obj([
                ("model_call_id", Json::str(&mc)),
                ("disposition", Json::str("proxy_aborted")),
                ("infra", Json::Bool(true)),
                (
                    "bounded",
                    Json::obj([("lo", Json::Int(0)), ("hi", Json::Int(micros))]),
                ),
            ]),
            scope_mc(core, &mc),
            vec![],
        );
        // (b) the bounded charge — metered at `hi` so `consumed`
        // conservatively covers the reservation claim.
        let mut attribution =
            Attribution::subject(core.cfg.run_id.clone(), budget_id.clone(), mc.clone());
        attribution.model_ref = Some(ModelRef {
            profile_ref: format!("hh.proxy.lane:{}", core.cfg.lane.lane_id),
            provider_model_id: core.cfg.lane.model.clone(),
            serving_route: "ollama".to_string(),
            effort: None,
        });
        let table = pricing::ollama_glm_5_3_flash();
        let row = SpendRow {
            resource: DimensionId::Spend,
            money: Money {
                micro_units: micros,
                currency: "USD".to_string(),
            },
            provenance: CostProvenance::ReconstructedFromNativeLog,
            provenance_class: ProvenanceClass::Reconstructed,
            derivation: Derivation::TablePriced {
                table_ref: table.table.table_id.clone(),
                table_version: table.table.version.clone(),
                tier: None,
                ttl_class: None,
            },
            confidence: Confidence::Bounded { lo: 0, hi: micros },
            coverage_ppm: 0,
            pricing_ref: None,
            source_event: EventRef {
                run_id: core.cfg.run_id.clone(),
                event_id: f_ev.event_id.clone(),
            },
            model_ref: attribution.model_ref.clone().unwrap(),
            attribution,
            roles: BTreeMap::new(),
        };
        let a_ev = mint(
            core,
            "measurement.cost.attributed",
            hharness_budget::events::cost_attributed_payload(&row),
            Scope::default(),
            vec![],
        );
        // (c) the remainder released — `lease_lost` (the dead writer's
        // claim folds identically to `release`).
        let mut left = ResourceVector::zero();
        left.add(DimensionId::Spend, micros);
        let r_ev = mint(
            core,
            "control.budget.released",
            hharness_budget::events::released_payload(&budget_id, Some(&rid), &left, "lease_lost"),
            Scope::default(),
            vec![],
        );
        append(core, vec![f_ev, a_ev, r_ev])?;
    }
    Ok(())
}
