//! R5.20 — retained view → existing renderer → sealed request → actual
//! provider bytes (REQ-E-206 owner slice, §2.3/AC-2-2; REQ-A-007;
//! REQ-D-005 contributing; the ticket's AC-1/AC-2).
//!
//! The join is byte-exact at every step and every artifact is real:
//! `hharness_context::compact::compact` mints the durable
//! `retained_selection/1` member through the production ladder (never a
//! fixture-asserted label); `GatewayModel` — the existing renderer —
//! serializes exactly the selected view; the loopback provider captures
//! the literal socket bytes (`plan.body` verbatim); `prepared::prepare`
//! seals `codec::serialize(plan)` into `request_bytes`/`request_ref` and
//! `PreparedCall::dispatch` drives those sealed bytes through a
//! capturing `Transport`. The `view_hash` the render stamps and the
//! prepared call's `view_ref` are one derivation over the same member —
//! the policy→selection→assembly→sealed-request→actual-wire join.
//!
//! Hermetic: loopback TCP + scratch `Store::open_with`/`ManualClock` —
//! deterministic, offline, USD 0 (no live-provider claim; ADR-0363).

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_budget::account::Account;
use hharness_budget::{
    BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec, DimensionId, DimensionKey, ResourceVector,
};
use hharness_context::compact::{
    self, ClearToolResults, CompactInput, CompactionTrigger, EvictOldest,
};
use hharness_context::events::CollectSink;
use hharness_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill,
};
use hharness_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hharness_control::driver::ModelPort;
use hharness_embed::gateway_model::{decode_model_lane, GatewayModel};
use hharness_gateway::codec::{self, WireFrame};
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, EndpointAllowlist, NoCredentials, Transport};
use hharness_gateway::plan::{Capabilities, ModelRef, ProviderRequestPlan};
use hharness_gateway::prepared::{prepare, DispatchVerdict, PrepareInputs, PrepareRefusal};
use hharness_gateway::vocab::{AttemptPolicy, ModelError, ModelErrorClass, Purpose};
use hharness_hir::records::Validity;
use hharness_identity::idp::{self, idp_id};
use hharness_ledger::classes::Durability;
use hharness_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_wire::json::Json;

// ── prefix envelopes (the durable rows the render reads) ────────────────────

fn ev(seq: u64, class: &str, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("e{seq}"),
        run_id: "run".into(),
        seq,
        ts: "2026-11-03T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Control),
        class: class.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: Default::default(),
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: hharness_ledger::ids::ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: Default::default(),
        provenance: None,
        payload,
        prev_hash: "h".into(),
        hash: format!("h{seq}"),
    }
}

/// A `model.call.completed` row carrying one tool call pair
/// (`{tool_call_id, surface, args_raw}` — the render's pair source).
fn completed(seq: u64, tool_call_id: &str) -> EventEnvelope {
    ev(
        seq,
        "model.call.completed",
        Json::obj([(
            "calls",
            Json::Arr(vec![Json::obj([
                ("tool_call_id", Json::str(tool_call_id)),
                ("surface", Json::str("read_file")),
                ("args_raw", Json::str("{}")),
            ])]),
        )]),
    )
}

/// A `context.observation.recorded` row joined to `tool_call_id` —
/// the render's tool-result source and the context item itself.
fn observation(seq: u64, tool_call_id: &str, marker: &str) -> EventEnvelope {
    let mut e = ev(
        seq,
        "context.observation.recorded",
        Json::obj([
            ("kind", Json::str("tool_result")),
            ("text", Json::str(marker)),
        ]),
    );
    e.scope.tool_call_id = Some(tool_call_id.to_string());
    e
}

/// The `context_item_id` derivation — the assembler's own
/// (`hh.embed.context_item/1` over the canonical payload).
fn item_id(payload: &Json) -> String {
    idp_id(
        "hh.embed.context_item/1",
        payload.to_canonical_string().as_bytes(),
    )
}

// ── the real compaction ladder (r5_18 fixture discipline) ───────────────────

fn toolprov(cap: &str, inv: &str) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: Origin::tool(cap, inv),
        authority: AuthorityClass::Delegate,
        taint: BTreeSet::new(),
        readers: ReaderSet::Public,
        scope: PersistenceScope::Run,
        derived_from: Vec::new(),
        created_at: 0,
        attestation: None,
    }
}

fn cand(
    item: &str,
    tokens: u64,
    priority: PriorityClass,
    seq: u64,
    tool: Option<(&str, &str)>,
) -> Candidate {
    Candidate {
        candidate_id: format!("cand-{item}"),
        context_item_id: Some(item.to_string()),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(priority),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".into(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: tool
            .map(|(c, i)| toolprov(c, i))
            .unwrap_or_else(|| ProvenanceRecord::kernel("kernel:context", 0)),
        validity: Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

fn plan_of(cands: &[Candidate], occupancy: u64) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let candidates: BTreeMap<String, Candidate> = cands
        .iter()
        .map(|c| (c.candidate_id.clone(), c.clone()))
        .collect();
    let items: Vec<PlannedItem> = cands
        .iter()
        .map(|c| PlannedItem {
            candidate_id: c.candidate_id.clone(),
            context_item_id: c.context_item_id.clone().unwrap(),
            artefact_id: None,
            delivery_id: format!("del:{}", c.candidate_id),
            authority: c.label.authority,
            label: c.label.clone(),
            tokens: c.estimate.tokens,
            state: c.state,
            delivered_by_reference: false,
            derived_from: None,
        })
        .collect();
    (
        ContextPlan {
            plan_id: "plan:r5_20".into(),
            model_call_id: "mc.prev".into(),
            derived_from: DerivedFrom {
                run_id: "run".into(),
                seq: 10,
                view_hash: "sha256:view".into(),
            },
            slots: vec![SlotFill {
                slot_id: "transcript".into(),
                items,
            }],
            omitted: vec![],
            context_label: Label::top(),
            occupancy_estimate: occupancy,
            legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
            reserved: 0,
            estimator_ref: "est/pinned".into(),
            static_hash: "sha256:static".into(),
        },
        candidates,
    )
}

fn cinput<'a>(
    plan: &'a ContextPlan,
    candidates: BTreeMap<String, Candidate>,
    trigger: CompactionTrigger,
    cap: u64,
    needed: u64,
) -> CompactInput<'a> {
    CompactInput {
        plan,
        candidates,
        window_cap: cap,
        trigger,
        needed,
        target_fraction_ppm: compact::DEFAULT_TARGET_FRACTION_PPM,
        scope: PersistenceScope::Run,
        at: 20,
        run_id: "run".into(),
        summarizer: None,
        slot_min_authority: BTreeMap::new(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    }
}

/// Drive the production `compact()` and return the emitted
/// `context.compaction.completed` payload (the durable row) plus the
/// decoded record — the row is what the prefix carries.
fn compaction_completed(
    input: &CompactInput,
    variants: &[&dyn compact::CompactionStrategy],
) -> Json {
    let mut sink = CollectSink::default();
    let _ = compact::compact(input, variants, &mut sink).unwrap();
    sink.events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .expect("completed row emitted")
        .1
        .clone()
}

// ── the loopback fake provider (r4_5b pattern — literal socket bytes) ────────

struct FakeProvider {
    endpoint: String,
    got: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

fn head_complete(buf: &[u8]) -> bool {
    let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..p]);
    let mut want = 0usize;
    for line in head.split("\r\n") {
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            want = v.trim().parse().unwrap_or(0);
        }
    }
    buf.len() >= p + 4 + want
}

fn fake_provider(response: String) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let got2 = got.clone();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 8192];
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while !head_complete(&buf) && std::time::Instant::now() <= deadline {
                match sock.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            *got2.lock().unwrap() = Some(buf);
            let _ = sock.write_all(response.as_bytes());
            let _ = sock.flush();
        }
    });
    FakeProvider {
        endpoint: format!("http://127.0.0.1:{port}"),
        got,
    }
}

impl FakeProvider {
    /// The literal request body the provider received (post-call).
    fn body_bytes(&self) -> Vec<u8> {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                return buf[p + 4..].to_vec();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }
    /// `true` when no connection ever arrived (a refused call must not
    /// touch the wire).
    fn silent(&self) -> bool {
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.got.lock().unwrap().is_none()
    }
}

/// An SSE answer (`content: done`, usage) the chat-completions grammar
/// decodes to one completed call.
fn sse_ok(model: &str) -> String {
    let chunk = |delta: Json, finish: Json, usage: Json| {
        let mut c = std::collections::BTreeMap::new();
        c.insert("id".into(), Json::str("chatcmpl-1"));
        c.insert("object".into(), Json::str("chat.completion.chunk"));
        c.insert("model".into(), Json::str(model));
        c.insert(
            "choices".into(),
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("delta", delta),
                ("finish_reason", finish),
            ])]),
        );
        if !matches!(usage, Json::Null) {
            c.insert("usage".into(), usage);
        }
        format!("data: {}\n\n", Json::Obj(c).to_canonical_string())
    };
    let usage = Json::obj([
        ("prompt_tokens", Json::Int(41)),
        ("completion_tokens", Json::Int(7)),
        ("total_tokens", Json::Int(48)),
    ]);
    let body = format!(
        "{}{}data: [DONE]\n\n",
        chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                ("content", Json::str("done")),
            ]),
            Json::Null,
            Json::Null,
        ),
        chunk(
            Json::obj([("role", Json::str("assistant"))]),
            Json::str("stop"),
            usage,
        ),
    );
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

// ── the prepared-call leg (r5_19 fixture discipline) ────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r520-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str, at_ms: u64) -> (ManualClock, Store, String, Lease) {
    let clock = ManualClock::at(at_ms);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (clock, s, run, lease)
}

fn primary(d: DimensionId) -> DimensionKey {
    DimensionKey::Primary(d)
}

fn fund(acc: &mut Account, lease: &Lease) -> String {
    let spec = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (primary(DimensionId::TokensInputUncached), 10_000_000),
            (primary(DimensionId::TokensInputCacheRead), 10_000_000),
            (primary(DimensionId::TokensInputCacheWrite), 10_000_000),
            (primary(DimensionId::TokensOutputVisible), 10_000_000),
            (primary(DimensionId::TokensOutputReasoning), 10_000_000),
            (primary(DimensionId::ModelCalls), 100),
            (primary(DimensionId::Retries), 100),
            (primary(DimensionId::TimeWallMs), 100_000_000),
        ],
    );
    acc.allocate(
        lease,
        None,
        BudgetScope {
            kind: BudgetScopeKind::AgentProcess,
            target: "proc:main".into(),
        },
        spec,
    )
    .unwrap()
}

fn bounds() -> ResourceVector {
    let mut v = ResourceVector::zero();
    v.add(DimensionId::TokensInputUncached, 100_000);
    v.add(DimensionId::TokensInputCacheRead, 10_000);
    v.add(DimensionId::TokensInputCacheWrite, 10_000);
    v.add(DimensionId::TokensOutputVisible, 10_000);
    v.add(DimensionId::TokensOutputReasoning, 10_000);
    v.add(DimensionId::ModelCalls, 2);
    v.add(DimensionId::Retries, 2);
    v.add(DimensionId::TimeWallMs, 3_600_000);
    v
}

fn open_turn(store: &mut Store, run: &str, lease: &Lease) {
    use hharness_ledger::event::Event;
    let ev = Event {
        event_id: store.alloc_id("evt"),
        class: "lifecycle.turn.started".to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-gateway/1"),
        scope: Scope {
            turn_id: Some("turn.1".to_string()),
            ..Scope::default()
        },
        parent_event_id: store.head_event_id(run).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-gateway/1", store.now_ms())),
        content_kind: None,
        payload: Json::obj([("turn_id", Json::str("turn.1"))]),
    };
    store.append(run, lease, vec![ev]).unwrap();
}

struct CaptureTransport {
    sends: Vec<Vec<u8>>,
    queued: std::collections::VecDeque<Result<Vec<(u64, WireFrame)>, ModelError>>,
}

impl Transport for CaptureTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        _credential: &CredentialHandle,
        _endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        self.sends.push(bytes.to_vec());
        self.queued
            .pop_front()
            .unwrap_or_else(|| Err(ModelError::new(ModelErrorClass::Network, "unscripted send")))
    }
}

fn ok_frames(model: &str, text: &str) -> Vec<(u64, WireFrame)> {
    let mut t = hharness_gateway::openai::ChunkTranslator::default();
    let chunk = |delta: Json, finish: Json, usage: Json| {
        let mut c = std::collections::BTreeMap::new();
        c.insert("id".into(), Json::str("chatcmpl-1"));
        c.insert("object".into(), Json::str("chat.completion.chunk"));
        c.insert("model".into(), Json::str(model));
        c.insert(
            "choices".into(),
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("delta", delta),
                ("finish_reason", finish),
            ])]),
        );
        if !matches!(usage, Json::Null) {
            c.insert("usage".into(), usage);
        }
        Json::Obj(c)
    };
    let mut frames = Vec::new();
    frames.extend(
        t.push_chunk(&chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                ("content", Json::str(text)),
            ]),
            Json::Null,
            Json::Null,
        ))
        .unwrap(),
    );
    frames.extend(
        t.push_chunk(&chunk(
            Json::obj([("role", Json::str("assistant"))]),
            Json::str("stop"),
            Json::obj([
                ("prompt_tokens", Json::Int(41)),
                ("completion_tokens", Json::Int(7)),
                ("total_tokens", Json::Int(48)),
            ]),
        ))
        .unwrap(),
    );
    frames.extend(t.finish());
    frames.into_iter().map(|f| (10u64, f)).collect()
}

fn model_ref() -> ModelRef {
    ModelRef {
        profile_ref: "profile:test".into(),
        provider_model_id: "fixture-model".into(),
        snapshot_id: None,
        serving_route: None,
        effort: None,
    }
}

fn endpoints(ep: &str) -> EndpointAllowlist {
    let mut e = EndpointAllowlist {
        policy_ref: "ep:test".into(),
        ..Default::default()
    };
    e.endpoints.insert(ep.to_string());
    e
}

fn policy(max_attempts: u32) -> AttemptPolicy {
    AttemptPolicy {
        max_attempts,
        base_delay_ms: 100,
        max_delay_ms: 1_000,
        jitter_min_ppm: 1_000_000,
        jitter_max_ppm: 1_000_000,
        honour_retry_after: false,
        retry_after_cap_ms: 5_000,
        retry_on: vec![
            hharness_gateway::vocab::RetryClass::Transient,
            hharness_gateway::vocab::RetryClass::RateLimit,
        ],
        stream_idle_timeout_ms: 5_000,
        attempt_timeout_ms: 10_000,
        connect_timeout_ms: 5_000,
    }
}

// ── the drive: render + capture through the real port ────────────────────────

fn lane(endpoint: &str) -> hharness_embed::gateway_model::ModelLane {
    decode_model_lane(
        &Json::obj([
            ("dialect", Json::str("openai-compatible@1")),
            ("endpoint", Json::str(endpoint)),
            ("provider_model_id", Json::str("fixture-model")),
        ]),
        "profile:test",
    )
    .unwrap()
}

/// `requested_members` → `call` through `GatewayModel` over `provider`;
/// returns `(requested members, captured socket body bytes)`.
fn drive(
    provider: &FakeProvider,
    prefix: &[EventEnvelope],
) -> (Vec<(&'static str, Json)>, Vec<u8>) {
    let mut port = GatewayModel::new(
        lane(&provider.endpoint),
        "do the thing".into(),
        None,
        vec![],
    )
    .unwrap();
    let members = port.requested_members("mc.1", &Json::Null, prefix);
    let outcome = port.call("mc.1", &Json::Null);
    assert!(
        !matches!(
            outcome.error_class.as_deref(),
            Some(c) if c != "unknown"
        ) || outcome.stop_reason != hharness_gateway::vocab::StopReason::Error,
        "call failed: {outcome:?}"
    );
    (members, provider.body_bytes())
}

fn member_of<'a>(members: &'a [(&'static str, Json)], name: &str) -> Option<&'a Json> {
    members.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
}

/// The retained-selection member of a `context.compaction.completed`
/// payload (the consumed member — the `view_hash` join's basis).
fn selection_member(completed_payload: &Json) -> Json {
    completed_payload.get("selection").unwrap().clone()
}

fn member_view_hash(member: &Json) -> String {
    idp::address(member.to_canonical_string().as_bytes(), "application/json").id()
}

/// The prepared leg over the *rendered* plan body — reconstructs the
/// plan the renderer serialized (the socket carries `plan.body`
/// verbatim; the seal wraps it in the plan envelope), runs
/// `prepare`/`dispatch` and proves the sealed request bytes and the
/// dispatched bytes carry that same body.
fn prepared_leg(
    tag: &str,
    endpoint: &str,
    selection: Json,
    rendered_body: Json,
) -> (PreparedCallShim, Store, String) {
    let (_clk, mut store, run, lease) = open(tag, 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut params = BTreeMap::new();
    params.insert(
        "stream_options".into(),
        Json::obj([("include_usage", Json::Bool(true))]),
    );
    let mut plan = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: endpoint.to_string(),
        model_ref: "fixture-model".to_string(),
        body: rendered_body,
        provider_params: params,
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: Default::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    };
    plan.canonical_len = codec::serialize(&plan).len() as u64;
    let mut creds = NoCredentials::default();
    let mut acc = Account::open(&mut store, &run).unwrap();
    let call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(endpoint),
        &mut creds,
        PrepareInputs {
            model_call_id: "mc.1".into(),
            turn_id: "turn.1".into(),
            budget_id: budget_id.clone(),
            retained_view: selection,
            plan: plan.clone(),
            bounds: bounds(),
            model_ref: model_ref(),
            role: "subject".into(),
            purpose: Purpose::Main,
            binding_ref: "binding:test".into(),
            credential_ref: None,
            substitution_allowed: Some(false),
            participant_class: None,
            reservation_ttl_ms: 60_000,
        },
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let mut transport = CaptureTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done"))].into(),
    };
    let presented = {
        let mut p = plan;
        p.budget_reservation_id = Some(call.reservation_id.clone());
        p
    };
    let mut call = call;
    let v = call.dispatch(
        &mut acc,
        &lease,
        &presented,
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(
        matches!(v, DispatchVerdict::Completed { .. }),
        "dispatch: {v:?}"
    );
    assert_eq!(transport.sends.len(), 1);
    assert_eq!(transport.sends[0], call.request_bytes);
    (
        PreparedCallShim {
            view_ref: call.view_ref.clone(),
            request_bytes: call.request_bytes.clone(),
            request_ref: call.request_ref.clone(),
            sent: transport.sends[0].clone(),
        },
        store,
        run,
    )
}

/// The prepared leg's captured facts, returned for the join asserts.
struct PreparedCallShim {
    view_ref: String,
    request_bytes: Vec<u8>,
    request_ref: String,
    sent: Vec<u8>,
}

// ── AC-1 (positive) — both policies, byte-exact joins ────────────────────────

/// `ac1_evict_oldest` — the durable `evict_oldest` selection changes the
/// expected payload bytes and the omitted items never reappear: the
/// render's captured socket bytes carry every retained item's content,
/// no omitted byte, and the honest `no_observation_recorded` pair
/// placeholder; the `view_hash` member names the consumed selection; the
/// prepared leg seals those same bytes into `request_bytes` and the
/// capturing transport receives exactly them (the durable
/// `model.call.requested` carries `view_hash`/`request_ref` —
/// policy→selection→assembly→sealed-request→actual-wire).
#[test]
fn ac1_evict_oldest_view_reaches_exact_provider_bytes() {
    // Six items — five evictable + one protected (`Required`): the
    // rung omits the evictable mass and the required group stays.
    let obs: Vec<Json> = (1..=6)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("tool_result")),
                ("text", Json::str(format!("MARKER-RETAIN-{i}"))),
            ])
        })
        .collect();
    let cands: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let mut c = cand(
                &item_id(o),
                100,
                if i < 5 {
                    PriorityClass::ObservationOld
                } else {
                    PriorityClass::TranscriptTail
                },
                (i + 1) as u64,
                Some(("read_file", &format!("inv-{}", i + 1))),
            );
            // The required group is protected — the newest item stays.
            if i == 5 {
                c.retention = Retention::Required;
            }
            c
        })
        .collect();
    let (plan, cmap) = plan_of(&cands, 600);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 400, 0);
    let row = compaction_completed(&input, &[]);
    let member = selection_member(&row);
    let sel = hharness_context::retained::RetainedSelection::from_json(&member).unwrap();
    assert_eq!(sel.input.policy.variant_ref, "hh/evict-oldest@1");
    assert!(!sel.omitted.is_empty() && !sel.retained.is_empty());
    assert!(
        !sel.omitted.contains(&item_id(&obs[5])),
        "the required item is protected"
    );

    let mut prefix: Vec<EventEnvelope> = Vec::new();
    for (i, o) in obs.iter().enumerate() {
        let tc = format!("tc{}", i + 1);
        prefix.push(completed((i * 2 + 1) as u64, &tc));
        let mut o_ev = observation((i * 2 + 2) as u64, &tc, &format!("MARKER-RETAIN-{}", i + 1));
        o_ev.payload = o.clone();
        prefix.push(o_ev);
    }
    prefix.push(ev(9, "context.compaction.completed", row));

    let provider = fake_provider(sse_ok("fixture-model"));
    let (members, body) = drive(&provider, &prefix);
    let body_s = String::from_utf8_lossy(&body);

    // The stamped `view_hash` is the consumed member's own address.
    assert_eq!(
        member_of(&members, "view_hash").and_then(Json::as_str),
        Some(member_view_hash(&member).as_str())
    );
    // Retained items reach the wire; omitted items never reappear.
    let body_json = hharness_wire::json::parse(&body_s).unwrap();
    let messages = body_json.get("messages").unwrap();
    let text_of = |m: &Json| m.to_canonical_string();
    let present: Vec<usize> = (0..6)
        .filter(|&i| {
            messages
                .to_canonical_string()
                .contains(&format!("MARKER-RETAIN-{}", i + 1))
        })
        .collect();
    let omitted_ids: BTreeSet<&String> = sel.omitted.iter().collect();
    let expected: Vec<usize> = (0..6)
        .filter(|&i| !omitted_ids.contains(&item_id(&obs[i])))
        .collect();
    assert_eq!(present, expected, "exactly the retained set rendered");
    for i in 0..6 {
        if !expected.contains(&i) {
            assert!(
                !body_s.contains(&format!("MARKER-RETAIN-{}", i + 1)),
                "omitted content reappeared on the wire"
            );
        }
    }
    assert!(
        text_of(&body_json).contains("no_observation_recorded"),
        "an omitted pair's result is the honest placeholder"
    );

    // The prepared leg: the sealed request and the dispatched bytes
    // carry exactly the rendered body — byte-exact joins.
    let (shim, store, run) =
        prepared_leg("eol", &provider.endpoint, member.clone(), body_json.clone());
    assert_eq!(shim.view_ref, member_view_hash(&member));
    assert_eq!(
        hharness_wire::json::parse(std::str::from_utf8(&shim.request_bytes).unwrap())
            .unwrap()
            .get("body")
            .unwrap()
            .to_canonical_string(),
        String::from_utf8_lossy(&body),
        "the sealed request's body is the rendered view's bytes"
    );
    assert_eq!(shim.sent, shim.request_bytes);
    let req = store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "model.call.requested")
        .map(|e| e.payload.clone())
        .unwrap();
    assert_eq!(
        req.get("view_hash").and_then(Json::as_str),
        Some(shim.view_ref.as_str())
    );
    assert_eq!(
        req.get("request_ref").and_then(Json::as_str),
        Some(shim.request_ref.as_str())
    );
}

/// `ac1_clear_tool_results` — the pinned `clear_tool_results`
/// parameters (`capabilities`, `older_than`,
/// `completed_invocations`) select which tool results reach the wire:
/// completed invocations older than the bound leave the view; the
/// in-bound and non-tool items stay; required-group members are
/// protected.
#[test]
fn ac1_clear_tool_results_view_reaches_exact_provider_bytes() {
    let obs: Vec<Json> = (1..=4)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("tool_result")),
                ("text", Json::str(format!("MARKER-CTR-{i}"))),
            ])
        })
        .collect();
    let cands: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            cand(
                &item_id(o),
                100,
                PriorityClass::ObservationOld,
                (i + 1) as u64,
                Some(("read_file", &format!("inv-{}", i + 1))),
            )
        })
        .collect();
    let (plan, cmap) = plan_of(&cands, 400);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 300, 0);
    let ctr = ClearToolResults {
        capabilities: ["read_file".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: [
            ("inv-1".to_string(), 5u64),
            ("inv-2".to_string(), 7),
            ("inv-3".to_string(), 9),
            ("inv-4".to_string(), 20),
        ]
        .into_iter()
        .collect(),
    };
    let eol = EvictOldest;
    let row = compaction_completed(&input, &[&ctr, &eol]);
    let member = selection_member(&row);
    let sel = hharness_context::retained::RetainedSelection::from_json(&member).unwrap();
    assert_eq!(sel.input.policy.variant_ref, "hh/clear-tool-results@1");
    let p = &sel.input.policy.parameters;
    assert_eq!(p.get("older_than").and_then(Json::as_int), Some(10));
    assert!(!sel.omitted.is_empty(), "aged-out results left the view");
    assert!(
        !sel.omitted.contains(&item_id(&obs[3])),
        "the in-bound invocation stays"
    );

    let mut prefix: Vec<EventEnvelope> = Vec::new();
    for (i, o) in obs.iter().enumerate() {
        let tc = format!("tc{}", i + 1);
        prefix.push(completed((i * 2 + 1) as u64, &tc));
        let mut o_ev = observation((i * 2 + 2) as u64, &tc, &format!("MARKER-CTR-{}", i + 1));
        o_ev.payload = o.clone();
        prefix.push(o_ev);
    }
    prefix.push(ev(9, "context.compaction.completed", row));

    let provider = fake_provider(sse_ok("fixture-model"));
    let (members, body) = drive(&provider, &prefix);
    let body_s = String::from_utf8_lossy(&body);
    assert_eq!(
        member_of(&members, "view_hash").and_then(Json::as_str),
        Some(member_view_hash(&member).as_str())
    );
    // Aged-out results never reappear; in-bound items reach the wire.
    for i in 0..4 {
        if sel.omitted.contains(&item_id(&obs[i])) {
            assert!(
                !body_s.contains(&format!("MARKER-CTR-{}", i + 1)),
                "aged-out result reappeared"
            );
        } else {
            assert!(body_s.contains(&format!("MARKER-CTR-{}", i + 1)));
        }
    }

    let (shim, _store, _run) = prepared_leg(
        "ctr",
        &provider.endpoint,
        member.clone(),
        hharness_wire::json::parse(&body_s).unwrap(),
    );
    assert_eq!(shim.sent, shim.request_bytes);
    assert_eq!(shim.view_ref, member_view_hash(&member));
}

/// `ac1_restart_fallback_retry` — the durable selection is the only
/// authority: a fresh port over the same prefix (the post-restart
/// re-drive) emits byte-identical bytes; a second call (the retry)
/// emits byte-identical bytes; and a ladder that falls back to the C0
/// `evict_oldest` rung (the declared CTR ineffective under a hard
/// requirement) renders exactly the fallback rung's recorded view.
#[test]
fn ac1_restart_fallback_and_retry_preserve_the_wire() {
    // Base selection — one omitted item.
    let obs: Vec<Json> = (1..=3)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("tool_result")),
                ("text", Json::str(format!("MARKER-R-{}", i))),
            ])
        })
        .collect();
    let cands: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            cand(
                &item_id(o),
                100,
                if i == 0 {
                    PriorityClass::ObservationOld
                } else {
                    PriorityClass::TranscriptTail
                },
                (i + 1) as u64,
                Some(("read_file", &format!("inv-{}", i + 1))),
            )
        })
        .collect();
    let (plan, cmap) = plan_of(&cands, 300);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 200, 0);
    let row = compaction_completed(&input, &[]);
    let _member = selection_member(&row);

    let prefix_of = |row: &Json| -> Vec<EventEnvelope> {
        let mut prefix: Vec<EventEnvelope> = Vec::new();
        for (i, o) in obs.iter().enumerate() {
            let tc = format!("tc{}", i + 1);
            prefix.push(completed((i * 2 + 1) as u64, &tc));
            let mut o_ev = observation((i * 2 + 2) as u64, &tc, &format!("MARKER-R-{}", i + 1));
            o_ev.payload = o.clone();
            prefix.push(o_ev);
        }
        prefix.push(ev(9, "context.compaction.completed", row.clone()));
        prefix
    };
    let prefix = prefix_of(&row);

    // Restart: a fresh port over the same durable prefix renders
    // byte-identical bytes.
    let p1 = fake_provider(sse_ok("fixture-model"));
    let (_, b1) = drive(&p1, &prefix);
    let p2 = fake_provider(sse_ok("fixture-model"));
    let (_, b2) = drive(&p2, &prefix);
    assert_eq!(b1, b2, "resume renders identical bytes");
    // Retry: the same call re-presented emits identical bytes.
    let p3 = fake_provider(sse_ok("fixture-model"));
    let (_, b3) = drive(&p3, &prefix);
    assert_eq!(b1, b3, "a retry sends byte-identical bytes");

    // Fallback: the declared CTR is ineffective under the hard
    // requirement → the C0 `evict_oldest` rung applies at index 1 and
    // the wire honors the recorded fallback selection.
    let cands2: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            cand(
                &item_id(o),
                100,
                if i == 0 {
                    PriorityClass::ObservationOld
                } else {
                    PriorityClass::TranscriptTail
                },
                (i + 1) as u64,
                // No `read_file` completions in the bound map → CTR
                // targets nothing.
                Some(("other_cap", &format!("inv-{}", i + 1))),
            )
        })
        .collect();
    let (plan2, cmap2) = plan_of(&cands2, 300);
    let input2 = cinput(&plan2, cmap2, CompactionTrigger::OccupancyHard, 200, 50);
    let ctr = ClearToolResults {
        capabilities: ["read_file".to_string()].into_iter().collect(),
        older_than: 10,
        completed_invocations: BTreeMap::new(),
    };
    let eol = EvictOldest;
    let mut sink = CollectSink::default();
    let out = compact::compact(&input2, &[&ctr, &eol], &mut sink).unwrap();
    let row2 = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .unwrap()
        .1
        .clone();
    let member2 = selection_member(&row2);
    let sel2 = hharness_context::retained::RetainedSelection::from_json(&member2).unwrap();
    assert_eq!(sel2.input.policy.variant_ref, "hh/evict-oldest@1");
    assert_eq!(sel2.pipeline_index, 1, "the fallback rung applied");
    assert!(!sel2.omitted.is_empty(), "the fallback rung omitted items");
    let _ = out;

    let prefix2 = prefix_of(&row2);
    let p4 = fake_provider(sse_ok("fixture-model"));
    let (members4, b4) = drive(&p4, &prefix2);
    let body4 = String::from_utf8_lossy(&b4);
    assert_eq!(
        member_of(&members4, "view_hash").and_then(Json::as_str),
        Some(member_view_hash(&member2).as_str())
    );
    for i in 0..3 {
        let marker = format!("MARKER-R-{}", i + 1);
        if sel2.omitted.contains(&item_id(&obs[i])) {
            assert!(!body4.contains(&marker), "fallback-omitted on the wire");
        } else {
            assert!(body4.contains(&marker), "fallback-retained missing");
        }
    }
}

// ── AC-2 (negative) — resurrection, label, capability, mutation ─────────────

/// `ac2_label_capability_mutation_fail` — a recorded selection that the
/// production path cannot reproduce ("a strategy label without
/// execution"), an unknown variant, or a malformed member refuse typed
/// at plan build — the provider sees zero requests; a plan declaring an
/// unadmitted capability or an unlisted endpoint refuses `InvalidPlan`
/// at `prepare`; a mutated presented plan refuses
/// `PreparedBytesMutated` at `dispatch`.
#[test]
fn ac2_label_capability_and_mutated_bytes_fail() {
    // Base: a real applied selection over two items.
    let obs: Vec<Json> = (1..=2)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("tool_result")),
                ("text", Json::str(format!("MARKER-N-{}", i))),
            ])
        })
        .collect();
    let cands: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            cand(
                &item_id(o),
                100,
                PriorityClass::ObservationOld,
                (i + 1) as u64,
                Some(("read_file", &format!("inv-{}", i + 1))),
            )
        })
        .collect();
    let (plan, cmap) = plan_of(&cands, 200);
    let input = cinput(&plan, cmap, CompactionTrigger::OccupancyHard, 100, 0);
    let row = compaction_completed(&input, &[]);
    let member = selection_member(&row);
    let sel = hharness_context::retained::RetainedSelection::from_json(&member).unwrap();

    // The prefix a selection member drives — calls + observations +
    // one `context.compaction.completed` row carrying the member.
    let prefix_of = |m: &Json| -> Vec<EventEnvelope> {
        let mut prefix: Vec<EventEnvelope> = Vec::new();
        for (i, o) in obs.iter().enumerate() {
            let tc = format!("tc{}", i + 1);
            prefix.push(completed((i * 2 + 1) as u64, &tc));
            let mut o_ev = observation((i * 2 + 2) as u64, &tc, &format!("MARKER-N-{}", i + 1));
            o_ev.payload = o.clone();
            prefix.push(o_ev);
        }
        prefix.push(ev(
            9,
            "context.compaction.completed",
            Json::obj([("selection", m.clone())]),
        ));
        prefix
    };

    // (a) A strategy label without execution: the recorded `retained`
    //     set was extended by an already-omitted id — the replay
    //     diverges → typed `plan_error`, zero bytes on the wire.
    let mut tampered = sel.clone();
    tampered.retained.push(tampered.omitted[0].clone());
    let p = fake_provider(sse_ok("fixture-model"));
    let mut port =
        GatewayModel::new(lane(&p.endpoint), "do the thing".into(), None, vec![]).unwrap();
    let ms_bad = port.requested_members("mc.1", &Json::Null, &prefix_of(&tampered.to_json()));
    assert!(
        member_of(&ms_bad, "plan_error").is_some(),
        "diverged label refused"
    );
    let out = port.call("mc.1", &Json::Null);
    assert_eq!(
        out.stop_reason,
        hharness_gateway::vocab::StopReason::Error,
        "the call fails typed"
    );
    assert!(p.silent(), "a refused plan never touches the wire");

    // (b) Unknown variant — `variant_ref` outside the admitted pair.
    let mut unknown = sel.clone();
    unknown.input.policy.variant_ref = "hh/unknown@9".into();
    let p2 = fake_provider(sse_ok("fixture-model"));
    let mut port2 =
        GatewayModel::new(lane(&p2.endpoint), "do the thing".into(), None, vec![]).unwrap();
    let ms2 = port2.requested_members("mc.1", &Json::Null, &prefix_of(&unknown.to_json()));
    assert!(
        member_of(&ms2, "plan_error").is_some(),
        "unknown variant refused"
    );
    let out2 = port2.call("mc.1", &Json::Null);
    assert_eq!(out2.stop_reason, hharness_gateway::vocab::StopReason::Error);
    assert!(p2.silent());

    // (c) Full-transcript resurrection is impossible *and* checked: a
    //     selection whose `omitted` names an input row drops exactly
    //     that content — re-verified here so the negative stands on
    //     its own (the AC-1 cells already prove it positively).
    let resurrect_probe = prefix_of(&member);
    let p3 = fake_provider(sse_ok("fixture-model"));
    let (_m3, body3) = drive(&p3, &resurrect_probe);
    let body3_s = String::from_utf8_lossy(&body3);
    for (i, o) in obs.iter().enumerate() {
        if sel.omitted.contains(&item_id(o)) {
            assert!(
                !body3_s.contains(&format!("MARKER-N-{}", i + 1)),
                "omitted item resurrected"
            );
        }
    }

    // (c) Unknown provider capability / unlisted endpoint — the
    //     existing gates refuse at `prepare`, zero dispatch.
    let (_clk, mut store, run, lease) = open("neg", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut acc = Account::open(&mut store, &run).unwrap();
    let bad_cap_plan = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: "http://provider.test".into(),
        model_ref: "fixture-model".into(),
        body: Json::obj([("messages", Json::Arr(vec![]))]),
        provider_params: BTreeMap::new(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: Some(true), // the dialect admits no deferred requests
        },
        cache: Default::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    };
    let r = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints("http://provider.test"),
        &mut creds,
        PrepareInputs {
            model_call_id: "mc.1".into(),
            turn_id: "turn.1".into(),
            budget_id: budget_id.clone(),
            retained_view: member.clone(),
            plan: bad_cap_plan,
            bounds: bounds(),
            model_ref: model_ref(),
            role: "subject".into(),
            purpose: Purpose::Main,
            binding_ref: "binding:test".into(),
            credential_ref: None,
            substitution_allowed: Some(false),
            participant_class: None,
            reservation_ttl_ms: 60_000,
        },
    );
    assert!(
        matches!(r, Err(PrepareRefusal::InvalidPlan { .. })),
        "unadmitted capability refused at prepare"
    );
    // (d) Endpoint outside the allowlist — REQ-A-007's port pin:
    //     `endpoints.admit` refuses at `prepare`, zero dispatch.
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut offlist = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: "http://other.test".into(),
        model_ref: "fixture-model".into(),
        body: Json::obj([("messages", Json::Arr(vec![]))]),
        provider_params: BTreeMap::new(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: Default::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    };
    offlist.canonical_len = codec::serialize(&offlist).len() as u64;
    let r2 = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints("http://provider.test"),
        &mut creds,
        PrepareInputs {
            model_call_id: "mc.2".into(),
            turn_id: "turn.1".into(),
            budget_id: budget_id.clone(),
            retained_view: member.clone(),
            plan: offlist,
            bounds: bounds(),
            model_ref: model_ref(),
            role: "subject".into(),
            purpose: Purpose::Main,
            binding_ref: "binding:test".into(),
            credential_ref: None,
            substitution_allowed: Some(false),
            participant_class: None,
            reservation_ttl_ms: 60_000,
        },
    );
    assert!(
        matches!(r2, Err(PrepareRefusal::InvalidPlan { .. })),
        "an unlisted endpoint refuses at prepare"
    );

    // (e) Changed sealed bytes: a presented plan that no longer
    //     serializes to the sealed request refuses
    //     `PreparedBytesMutated` — zero bytes leave.
    let good_plan = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: "http://provider.test".into(),
        model_ref: "fixture-model".into(),
        body: Json::obj([("messages", Json::Arr(vec![]))]),
        provider_params: BTreeMap::new(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: Default::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints("http://provider.test"),
        &mut creds,
        PrepareInputs {
            model_call_id: "mc.3".into(),
            turn_id: "turn.1".into(),
            budget_id: budget_id.clone(),
            retained_view: member.clone(),
            plan: good_plan.clone(),
            bounds: bounds(),
            model_ref: model_ref(),
            role: "subject".into(),
            purpose: Purpose::Main,
            binding_ref: "binding:test".into(),
            credential_ref: None,
            substitution_allowed: Some(false),
            participant_class: None,
            reservation_ttl_ms: 60_000,
        },
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let mut mutated = good_plan.clone();
    mutated.budget_reservation_id = Some(call.reservation_id.clone());
    mutated.model_ref = "other-model".to_string();
    let mut transport = CaptureTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done"))].into(),
    };
    let v = call.dispatch(
        &mut acc,
        &lease,
        &mutated,
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(
        matches!(v, DispatchVerdict::PreparedBytesMutated { .. }),
        "mutated presented plan refused: {v:?}"
    );
    assert!(transport.sends.is_empty(), "mutated bytes never dispatch");
}

/// `ac2_no_selection_no_claim` — when no `retained_selection/1`
/// informs the render (no trigger yet / no next dispatch against a
/// recorded compaction), the request rows record *unavailable
/// fidelity*: no `view_hash` member, no treatment claim — and the
/// legacy full-view render stands (nothing was omitted because no
/// selection ran). An untriggered selection's `omitted` is empty: the
/// render is the full view and `view_hash` still names the record —
/// its own `status` says the treatment did not execute.
#[test]
fn ac2_no_selection_records_unavailable_fidelity_not_treatment() {
    let obs: Vec<Json> = (1..=2)
        .map(|i| {
            Json::obj([
                ("kind", Json::str("tool_result")),
                ("text", Json::str(format!("MARKER-U-{}", i))),
            ])
        })
        .collect();

    // (a) No compaction row at all — the legacy render carries
    //     everything and stamps no `view_hash`.
    let prefix_plain: Vec<EventEnvelope> = obs
        .iter()
        .enumerate()
        .flat_map(|(i, o)| {
            let tc = format!("tc{}", i + 1);
            let mut o_ev = observation((i * 2 + 2) as u64, &tc, &format!("MARKER-U-{}", i + 1));
            o_ev.payload = o.clone();
            vec![completed((i * 2 + 1) as u64, &tc), o_ev]
        })
        .collect();
    let p = fake_provider(sse_ok("fixture-model"));
    let (members, body) = drive(&p, &prefix_plain);
    assert!(member_of(&members, "view_hash").is_none());
    let body_s = String::from_utf8_lossy(&body);
    assert!(body_s.contains("MARKER-U-1") && body_s.contains("MARKER-U-2"));

    // (b) A ladder-exhausted record (`status:"failed"`, `omitted`
    //     empty — the reclaim could not satisfy the requirement and
    //     the view is unchanged): the full view still reaches the wire
    //     and `view_hash` names the failed record — the record's
    //     `status` is the honest treatment answer, never a claim of
    //     effect.
    let cands: Vec<Candidate> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let mut c = cand(
                &item_id(o),
                100,
                PriorityClass::TranscriptTail,
                (i + 1) as u64,
                Some(("read_file", &format!("inv-{}", i + 1))),
            );
            // Every item protected — the attempted rung can forget
            // nothing; the exhausted ladder's record reproduces.
            c.retention = Retention::Required;
            c
        })
        .collect();
    let (plan, cmap) = plan_of(&cands, 200);
    // `needed` exceeds what any rung can reclaim → the ladder exhausts
    // (`CompactionImpossible`) after emitting the failed row.
    let input = cinput(
        &plan,
        cmap,
        CompactionTrigger::OccupancyHard,
        10_000,
        10_000,
    );
    let mut sink = CollectSink::default();
    let _ = compact::compact(&input, &[], &mut sink);
    let row = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.compaction.completed")
        .expect("the exhausted ladder lands its row")
        .1
        .clone();
    let member_j = selection_member(&row);
    let sel = hharness_context::retained::RetainedSelection::from_json(&member_j).unwrap();
    assert!(sel.omitted.is_empty(), "a failed rung omits nothing");
    assert_eq!(sel.status, compact::CompactionStatus::Failed);
    let mut prefix_failed = prefix_plain.clone();
    prefix_failed.push(ev(9, "context.compaction.completed", row));
    let p2 = fake_provider(sse_ok("fixture-model"));
    let (members2, body2) = drive(&p2, &prefix_failed);
    let body2_s = String::from_utf8_lossy(&body2);
    assert!(body2_s.contains("MARKER-U-1") && body2_s.contains("MARKER-U-2"));
    assert_eq!(
        member_of(&members2, "view_hash").and_then(Json::as_str),
        Some(member_view_hash(&member_j).as_str()),
        "the failed record is named — its status carries the truth"
    );
}
