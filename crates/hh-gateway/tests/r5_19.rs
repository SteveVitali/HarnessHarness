//! R5.19 — prepared gateway reservations and persistent deadline
//! (PADR-R5-2 B.1 item 5; REQ-E-207 / REQ-E-006 / REQ-D-006
//! contributing; AC-1/AC-2).
//!
//! The fixed order over the landed owners: retained view → existing
//! serialization + primary bounds → whole-vector `Account::reserve`
//! (R5.12) → sealed provider plan → durable `model.call.requested` →
//! deadline-bounded dispatch → durable actual/unknown usage →
//! charge/release through the owning Account (R5.13 fold) → the driver's
//! guard downstream. Scratch `Store::open_with` + `ManualClock` runs —
//! deterministic, offline, USD 0.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_budget::account::Account;
use hharness_budget::{
    BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec, DimensionId, DimensionKey, ResourceVector,
};
use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, EndpointAllowlist, NoCredentials, Transport};
use hharness_gateway::openai::ChunkTranslator;
use hharness_gateway::plan::{Capabilities, ModelRef, ProviderRequestPlan};
use hharness_gateway::prepared::{
    prepare, DispatchVerdict, PrepareInputs, PrepareRefusal, PreparedCall,
};
use hharness_gateway::vocab::{AttemptPolicy, ModelError, ModelErrorClass, Purpose, TimeoutKind};
use hharness_ledger::ids::ManualClock;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_wire::json::Json;

// ── harness ───────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-gw-r519-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A deterministic scratch store + run + writer lease + shared clock.
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

/// A budget covering the bound vector plus a `time.wall_ms` cap.
fn fund(acc: &mut Account, lease: &Lease, wall_cap_ms: i64) -> String {
    let spec = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (primary(DimensionId::TokensInputUncached), 1_000_000),
            (primary(DimensionId::TokensInputCacheRead), 1_000_000),
            (primary(DimensionId::TokensInputCacheWrite), 1_000_000),
            (primary(DimensionId::TokensOutputVisible), 1_000_000),
            (primary(DimensionId::TokensOutputReasoning), 1_000_000),
            (primary(DimensionId::ModelCalls), 100),
            (primary(DimensionId::Retries), 100),
            (primary(DimensionId::TimeWallMs), wall_cap_ms),
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
    v.add(DimensionId::TokensInputUncached, 400);
    v.add(DimensionId::TokensInputCacheRead, 50);
    v.add(DimensionId::TokensInputCacheWrite, 50);
    v.add(DimensionId::TokensOutputVisible, 200);
    v.add(DimensionId::TokensOutputReasoning, 50);
    v.add(DimensionId::ModelCalls, 2);
    v.add(DimensionId::Retries, 2);
    v.add(DimensionId::TimeWallMs, 30_000);
    v
}

fn plan() -> ProviderRequestPlan {
    let d = dialects::openai_compatible();
    let mut p = ProviderRequestPlan {
        dialect_id: d.dialect_id.clone(),
        dialect_version: d.version.clone(),
        endpoint_ref: "http://provider.test".to_string(),
        model_ref: "fixture-model".to_string(),
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
    p.canonical_len = hharness_gateway::codec::serialize(&p).len() as u64;
    p
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

/// A `retained_selection/1`-shaped member — the owner document, consumed
/// unchanged (the unit only serializes it).
fn retained_view() -> Json {
    Json::obj([
        ("schema", Json::str("retained_selection/1")),
        (
            "input",
            Json::obj([
                (
                    "policy",
                    Json::obj([
                        ("variant_ref", Json::str("hh/evict_oldest@1")),
                        ("parameters", Json::obj([])),
                    ]),
                ),
                ("window_cap", Json::Int(8000)),
                ("needed", Json::Int(500)),
                ("target_fraction_ppm", Json::Int(800_000)),
                ("occupancy", Json::Int(7200)),
                ("cut_points", Json::Arr(vec![])),
                ("items", Json::Arr(vec![])),
            ]),
        ),
        (
            "retained",
            Json::Arr(vec![Json::str("item:a"), Json::str("item:b")]),
        ),
        ("omitted", Json::Arr(vec![Json::str("item:c")])),
        ("status", Json::str("applied")),
        ("pipeline_index", Json::Int(0)),
    ])
}

fn inputs(budget_id: &str) -> PrepareInputs {
    PrepareInputs {
        model_call_id: "mc.1".into(),
        turn_id: "turn.1".into(),
        budget_id: budget_id.to_string(),
        retained_view: retained_view(),
        plan: plan(),
        bounds: bounds(),
        model_ref: model_ref(),
        role: "subject".into(),
        purpose: Purpose::Main,
        binding_ref: "binding:test".into(),
        credential_ref: None,
        substitution_allowed: Some(false),
        participant_class: None,
        reservation_ttl_ms: 60_000,
    }
}

fn endpoints() -> EndpointAllowlist {
    let mut e = EndpointAllowlist {
        policy_ref: "ep:test".into(),
        ..Default::default()
    };
    e.endpoints.insert("http://provider.test".to_string());
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

/// A scripted transport — returns queued per-send results and records
/// every dispatched byte string (the B.9 capturing transport).
struct FakeTransport {
    sends: Vec<Vec<u8>>,
    queued: std::collections::VecDeque<Result<Vec<(u64, WireFrame)>, ModelError>>,
}

impl Transport for FakeTransport {
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

/// A completed stream through the real chunk→frame translator — the same
/// fold the HTTP transport runs (usage optional).
fn ok_frames(model: &str, text: &str, usage: Option<Json>) -> Vec<(u64, WireFrame)> {
    let mut t = ChunkTranslator::default();
    let chunk = |delta: Json, finish: Json, u: Json| {
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
        if !matches!(u, Json::Null) {
            c.insert("usage".into(), u);
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
            usage.unwrap_or(Json::Null),
        ))
        .unwrap(),
    );
    frames.extend(t.finish());
    frames.into_iter().map(|f| (10u64, f)).collect()
}

fn usage_41_7() -> Json {
    Json::obj([
        ("prompt_tokens", Json::Int(41)),
        ("completion_tokens", Json::Int(7)),
        ("total_tokens", Json::Int(48)),
    ])
}

/// Open the caller's turn scope (`lifecycle.turn.started` opens `turn` —
/// the driver owns turns in production; the battery mints its own).
fn open_turn(store: &mut Store, run: &str, lease: &Lease) {
    use hharness_ledger::event::{Event, Producer, Scope};
    use hharness_provenance::ProvenanceRecord;
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

fn classes(store: &Store, run: &str) -> Vec<String> {
    store
        .events(run)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect()
}

fn payload(store: &Store, run: &str, class: &str) -> Json {
    store
        .events(run)
        .unwrap()
        .iter()
        .find(|e| e.class == class)
        .map(|e| e.payload.clone())
        .unwrap_or(Json::Null)
}

// ── AC-1 (positive) ───────────────────────────────────────────────────

#[test]
fn ac1_prepared_order_reconciles_through_owner() {
    let (_clk, mut store, run, lease) = open("order", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 3_600_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done", Some(usage_41_7())))].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    // Leg 1–5 facts: serialized view, whole-vector hold, sealed refs,
    // created-origin deadline.
    assert!(!call.view_bytes.is_empty() && !call.view_ref.is_empty());
    assert!(
        call.request_bytes
            == hharness_gateway::codec::serialize(&{
                let mut p = plan();
                p.budget_reservation_id = Some(call.reservation_id.clone());
                p
            })
    );
    assert_eq!(call.deadline_ms, Some(1_000 + 3_600_000));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    let DispatchVerdict::Completed { usage, .. } = v else {
        panic!("expected Completed, got {v:?}");
    };
    assert!(usage.is_some(), "fixture stream carries usage");
    // The transport saw exactly the sealed bytes.
    assert_eq!(transport.sends.len(), 1);
    assert_eq!(transport.sends[0], call.request_bytes);
    // Durable order: reserved → requested → attempt.* → completed →
    // consumed → released.
    let cls = classes(&store, &run);
    let pos = |c: &str| cls.iter().position(|k| k == c).unwrap_or(usize::MAX);
    assert!(pos("control.budget.reserved") < pos("model.call.requested"));
    assert!(pos("model.call.requested") < pos("model.call.completed"));
    assert!(pos("model.call.completed") < pos("control.budget.consumed"));
    let req = payload(&store, &run, "model.call.requested");
    assert_eq!(
        req.get("view_hash").and_then(Json::as_str),
        Some(call.view_ref.as_str())
    );
    let consumed = payload(&store, &run, "control.budget.consumed");
    assert_eq!(
        consumed.get("reservation_id").and_then(Json::as_str),
        Some(call.reservation_id.as_str()),
        "the durable charge joins the held reservation"
    );
    assert_eq!(
        req.get("request_ref").and_then(Json::as_str),
        Some(call.request_ref.as_str())
    );
    let done = payload(&store, &run, "model.call.completed");
    assert_eq!(
        done.get("usage").and_then(|u| u.get("available")),
        Some(&Json::Bool(true))
    );
    // Reconciled through the owner: the reservation is released and the
    // physical fold shows the actual use once.
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    assert_eq!(phys.incurred.get(DimensionId::ModelCalls), 1);
    assert!(phys.incurred.get(DimensionId::TokensInputUncached) > 0);
    assert!(phys.unknown.is_empty());
    assert!(acc
        .tree
        .reservation(&call.reservation_id)
        .map(|r| !r.outstanding)
        .unwrap_or(true));
}

fn call_plan_with(call: &PreparedCall) -> ProviderRequestPlan {
    // The caller's intended plan — identical to the prepared one.
    let mut p = plan();
    p.budget_reservation_id = Some(call.reservation_id.clone());
    p
}

#[test]
fn ac1_deadline_persists_across_resume_approval_and_drain() {
    // origin = 1_000, wall cap = 60_000 → deadline 61_000. An approval
    // wait advances the clock but the deadline is recomputed from the
    // durable origin — identical at every gate point.
    let (clk, mut store, run, lease) = open("deadline", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    assert_eq!(call.deadline_ms, Some(61_000));
    // Approval wait / resume: clock advances, deadline unchanged.
    clk.advance(30_000);
    assert_eq!(call.deadline_ms, Some(61_000));
    // Committed drain at expiry: zero dispatch, the claim releases.
    clk.advance(40_000);
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    let DispatchVerdict::DeadlineExpired {
        deadline_ms,
        now_ms,
    } = v
    else {
        panic!("expected DeadlineExpired, got {v:?}");
    };
    assert_eq!((deadline_ms, now_ms), (61_000, 71_000));
    assert!(transport.sends.is_empty(), "expired call dispatches zero");
    assert!(matches!(
        payload(&store, &run, "model.call.requested"),
        Json::Null
    ));
    // The committed drain released the claim when the lease lived; under
    // an expired lease the hold dies with the holder (B.5) — either way
    // no physical capacity is created and no charge lands.
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    assert_eq!(phys.incurred.get(DimensionId::ModelCalls), 0);
    assert!(phys.unknown.is_empty());
}

// ── AC-2 (negative) ───────────────────────────────────────────────────

#[test]
fn ac2_unbounded_and_unfunded_dispatch_zero() {
    let (_clk, mut store, run, lease) = open("unfunded", 1_000);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let head = store.events(&run).unwrap().len();
    // Missing a required role → Unbounded, zero rows, zero bytes.
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let mut inp = inputs(&budget_id);
        let mut v = ResourceVector::zero();
        for (d, a) in inp.bounds.iter() {
            if d != DimensionId::TokensOutputReasoning {
                v.add(d, a);
            }
        }
        inp.bounds = v;
        let r = prepare(&mut acc, &lease, &dialect, &endpoints(), &mut creds, inp);
        assert!(matches!(r, Err(PrepareRefusal::Unbounded { .. })), "{r:?}");
    }
    // Input roles under the estimate floor → Underbounded.
    {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let mut inp = inputs(&budget_id);
        inp.bounds = ResourceVector::zero();
        for d in [
            DimensionId::TokensInputUncached,
            DimensionId::TokensInputCacheRead,
            DimensionId::TokensInputCacheWrite,
            DimensionId::TokensOutputVisible,
            DimensionId::TokensOutputReasoning,
            DimensionId::ModelCalls,
        ] {
            inp.bounds.add(d, 1);
        }
        let r = prepare(&mut acc, &lease, &dialect, &endpoints(), &mut creds, inp);
        assert!(
            matches!(r, Err(PrepareRefusal::Underbounded { .. })),
            "{r:?}"
        );
    }
    // A vector the budget cannot fund → Unfunded + `reserved{refused}`
    // ledgered by the owner, zero dispatch.
    {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let mut inp = inputs(&budget_id);
        inp.bounds.add(DimensionId::TokensInputUncached, 99_000_000);
        let r = prepare(&mut acc, &lease, &dialect, &endpoints(), &mut creds, inp);
        assert!(matches!(r, Err(PrepareRefusal::Unfunded { .. })), "{r:?}");
        let refused = store
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "control.budget.reserved")
            .any(|e| e.payload.get("outcome").and_then(Json::as_str) == Some("refused"));
        assert!(refused, "the owner ledgers the refused reserve");
    }
    assert_eq!(store.events(&run).unwrap().len(), head + 3); // allocated + turn + refused only
}

#[test]
fn ac2_mutated_prepared_bytes_refuse() {
    let (_clk, mut store, run, lease) = open("mutated", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "ok", Some(usage_41_7())))].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    // Mutate the presented plan — the sealed bytes refuse.
    let mut mutated = plan();
    mutated.budget_reservation_id = Some(call.reservation_id.clone());
    mutated.model_ref = "other-model".to_string();
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
        "{v:?}"
    );
    assert!(transport.sends.is_empty());
    drop(acc);
    assert!(matches!(
        payload(&store, &run, "model.call.requested"),
        Json::Null
    ));
    // The sealed plan itself still dispatches (the mutation was the
    // presented bytes, not the held record).
    let mut acc = Account::open(&mut store, &run).unwrap();
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(matches!(v, DispatchVerdict::Completed { .. }), "{v:?}");
}

#[test]
fn ac2_timeout_not_cancellation_and_retries_reuse_reservation() {
    let (_clk, mut store, run, lease) = open("timeout", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    // attempt 1: socket timeout (transient) → attempt 2: completes. One
    // reservation, one `model.call.requested`, attempt joins durable.
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![
            Err(ModelError::new(
                ModelErrorClass::Timeout(TimeoutKind::Attempt),
                "socket deadline fired",
            )),
            Ok(ok_frames("fixture-model", "done", Some(usage_41_7()))),
        ]
        .into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(2),
    );
    let DispatchVerdict::Completed { attempts, .. } = v else {
        panic!("expected Completed, got {v:?}");
    };
    assert_eq!(attempts, 2);
    assert_eq!(transport.sends.len(), 2);
    assert_eq!(transport.sends[0], transport.sends[1]);
    let cls = classes(&store, &run);
    assert_eq!(
        cls.iter()
            .filter(|c| c.as_str() == "model.call.requested")
            .count(),
        1,
        "one durable request across retries"
    );
    let failed_attempt = payload(&store, &run, "model.call.attempt.failed");
    assert_eq!(
        failed_attempt
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("timeout{attempt}"),
        "a socket timeout keeps its typed class — never cancelled"
    );
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    assert_eq!(phys.incurred.get(DimensionId::Retries), 1);
}

#[test]
fn ac2_failed_timeout_is_unknown_exposure_never_cancelled() {
    let (_clk, mut store, run, lease) = open("unknown", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Err(ModelError::new(
            ModelErrorClass::Timeout(TimeoutKind::Connect),
            "connect deadline fired",
        ))]
        .into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    let DispatchVerdict::Failed {
        error,
        usage_charged,
        ..
    } = v
    else {
        panic!("expected Failed, got {v:?}");
    };
    assert_eq!(error.class, ModelErrorClass::Timeout(TimeoutKind::Connect));
    assert!(!usage_charged);
    let failed = payload(&store, &run, "model.call.failed");
    assert_eq!(
        failed
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("timeout{connect}"),
        "socket timeout is not remote cancellation"
    );
    // Ambiguous physical outcome → unknown exposure, never zero.
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    assert_eq!(phys.incurred.get(DimensionId::TokensInputUncached), 0);
    assert_eq!(phys.incurred.get(DimensionId::ModelCalls), 1);
    assert_eq!(phys.unknown.len(), 1);
    assert_eq!(phys.unknown[0].reason.as_str(), "ambiguous_failure");
    // The terminal closed the scope — a second dispatch is typed.
    let mut acc = Account::open(&mut store, &run).unwrap();
    let v2 = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(matches!(v2, DispatchVerdict::AlreadyTerminal), "{v2:?}");
}

#[test]
fn ac2_unavailable_usage_is_unknown_not_zero() {
    let (_clk, mut store, run, lease) = open("unavail", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    // The stream completes but reports no usage member.
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done", None))].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    let DispatchVerdict::Completed { usage, .. } = v else {
        panic!("expected Completed, got {v:?}");
    };
    assert!(usage.is_none());
    let done = payload(&store, &run, "model.call.completed");
    assert_eq!(
        done.get("usage").and_then(|u| u.get("available")),
        Some(&Json::Bool(false))
    );
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    assert_eq!(phys.unknown[0].reason.as_str(), "usage_unavailable");
}

#[test]
fn ac2_served_model_drift_refuses() {
    let (_clk, mut store, run, lease) = open("drift", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("other-model", "done", Some(usage_41_7())))].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    let DispatchVerdict::Failed { error, .. } = v else {
        panic!("expected Failed, got {v:?}");
    };
    assert_eq!(error.class, ModelErrorClass::ServedModelMismatch);
    let failed = payload(&store, &run, "model.call.failed");
    assert_eq!(
        failed
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("served_model_mismatch")
    );
}

#[test]
fn ac2_release_creates_no_physical_capacity() {
    // Logical reversal (releasing the unclaimed remainder) never refunds
    // physical incurred use — the R5.13 fold reads the same durable rows.
    let (_clk, mut store, run, lease) = open("norefund", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund(&mut acc, &lease, 60_000)
    };
    open_turn(&mut store, &run, &lease);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials::default();
    let mut transport = FakeTransport {
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done", Some(usage_41_7())))].into(),
    };
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id),
    )
    .unwrap_or_else(|e| panic!("prepare refused: {e:?}"));
    let v = call.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(matches!(v, DispatchVerdict::Completed { .. }), "{v:?}");
    let acc = Account::open(&mut store, &run).unwrap();
    let phys = acc.physical_use();
    let incurred = phys.incurred.get(DimensionId::TokensInputUncached);
    assert!(incurred > 0);
    assert!(
        store
            .events(&run)
            .unwrap()
            .iter()
            .any(|e| e.class == "control.budget.released"),
        "the remainder released — incurred stays {incurred}"
    );
    assert_eq!(
        phys.incurred.get(DimensionId::TokensInputUncached),
        incurred
    );
    assert_eq!(phys.incurred.get(DimensionId::ModelCalls), 1);
}
