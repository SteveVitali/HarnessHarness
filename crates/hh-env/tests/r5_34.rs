//! R5.34 — Account refusal, exhaustion and operational reporting machinery
//! (evidence spec §5.2–§5.3 / D3-F4–F5; the O6/O7 fixture slice + the
//! fixed 8×3 stage-count report).
//!
//! A real `Store`/WAL + `ManualClock`, a real `Account` and the landed
//! `prepared` retained-view→wire path drive the two cells: O6 starves the
//! Account of `model_calls` and proves the *identical* positive transport
//! path refuses at reserve before any independently counted transport
//! entry; O7 commits an effect behind a declared bounded barrier, lets the
//! durable-origin `time.wall_ms` deadline lapse, and proves new dispatch is
//! blocked, the claim drains, exhaustion stops and committed work is
//! preserved on rebuild. The `OpsReport` machinery folds the 24-trial
//! P/A/R/T/V stage counts, ppm ratios (`undefined` on a zero denominator),
//! the `[V/P,(V+U)/P]` missingness bound and O8's separate `detected/3`.
//!
//! Fixture scope only: this proves the apparatus, the folds and the
//! report, not the confined live 24-trial run — actual O×3 backend trials
//! stay gated (R5.44/GATE-G18/G11). Scratch stores, offline, USD 0.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_budget::account::{Account, ChargeRequest};
use hharness_budget::attribution::Attribution;
use hharness_budget::{
    BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec, DimensionId, DimensionKey,
    ResourceQuantity, ResourceVector,
};
use hharness_env::ops::{
    missingness_bound, o6_outcome, o7_outcome, stage_ratio, AccountFailure, AccountOutcome,
    MarkerSink, O6Cell, O7Cell, ObservationPoint, OpsCell, OpsObserver, OpsReport, Ratio,
    ReportError, Sentinel, StageCounts, TrialOutcome, TrialRecord, OPS_REPETITIONS,
};
use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, EndpointAllowlist, NoCredentials, Transport};
use hharness_gateway::openai::ChunkTranslator;
use hharness_gateway::plan::{Capabilities, ModelRef, ProviderRequestPlan};
use hharness_gateway::prepared::{prepare, DispatchVerdict, PrepareInputs, PrepareRefusal};
use hharness_gateway::vocab::{AttemptPolicy, ModelError, ModelErrorClass, Purpose};
use hharness_ledger::ids::{Clock, ManualClock};
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_wire::json::Json;

// ── harness ───────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-r5_34-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A deterministic scratch store + run + writer lease + shared clock, at
/// a named root (a reopen binds the same WAL).
fn open_at(root: &std::path::Path, at_ms: u64) -> (ManualClock, Store, String, Lease) {
    let clock = ManualClock::at(at_ms);
    let mut s =
        Store::open_with(root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (clock, s, run, lease)
}

/// A deterministic scratch store + run + writer lease + shared clock.
fn open(tag: &str, at_ms: u64) -> (ManualClock, Store, String, Lease) {
    open_at(&dir(tag), at_ms)
}

fn primary(d: DimensionId) -> DimensionKey {
    DimensionKey::Primary(d)
}

/// The run root: a `model_calls` cap the O6 starve/O7 barrier tune, a
/// `time.wall_ms` cap that fixes the durable-origin deadline.
fn fund_root(acc: &mut Account, lease: &Lease, model_calls: i64, wall_cap_ms: i64) -> String {
    let spec = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (primary(DimensionId::TokensInputUncached), 1_000_000),
            (primary(DimensionId::TokensInputCacheRead), 1_000_000),
            (primary(DimensionId::TokensInputCacheWrite), 1_000_000),
            (primary(DimensionId::TokensOutputVisible), 1_000_000),
            (primary(DimensionId::TokensOutputReasoning), 1_000_000),
            (primary(DimensionId::ModelCalls), model_calls),
            (primary(DimensionId::Retries), 100),
            (primary(DimensionId::TimeWallMs), wall_cap_ms),
        ],
    );
    acc.allocate(
        lease,
        None,
        BudgetScope {
            kind: BudgetScopeKind::AgentProcess,
            target: "proc:ops".into(),
        },
        spec,
    )
    .unwrap()
}

/// A child (separate instrument) budget under the root — O7's "real root
/// and separate instrument budget": child hard ≤ parent remaining.
fn fund_instrument(acc: &mut Account, lease: &Lease, parent: &str) -> String {
    let spec = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (primary(DimensionId::TokensInputUncached), 500_000),
            (primary(DimensionId::TokensInputCacheRead), 500_000),
            (primary(DimensionId::TokensInputCacheWrite), 500_000),
            (primary(DimensionId::TokensOutputVisible), 500_000),
            (primary(DimensionId::TokensOutputReasoning), 500_000),
            (primary(DimensionId::ModelCalls), 10),
            (primary(DimensionId::Retries), 10),
        ],
    );
    acc.allocate(
        lease,
        Some(parent),
        BudgetScope {
            kind: BudgetScopeKind::Procedure,
            target: "proc:ops/o7-instrument".into(),
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
    v.add(DimensionId::ModelCalls, 1);
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

/// A `retained_selection/1`-shaped member — consumed unchanged.
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

fn inputs(budget_id: &str, mc: &str) -> PrepareInputs {
    inputs_wall(budget_id, mc, 30_000)
}

/// The call inputs with a declared `time.wall_ms` bound the reserve can
/// actually fund (O7's account bound is 5_000).
fn inputs_wall(budget_id: &str, mc: &str, wall: i64) -> PrepareInputs {
    let mut b = bounds();
    b.add(DimensionId::TimeWallMs, wall - 30_000);
    PrepareInputs {
        model_call_id: mc.to_string(),
        turn_id: "turn.1".into(),
        budget_id: budget_id.to_string(),
        retained_view: retained_view(),
        plan: plan(),
        bounds: b,
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

/// The deterministic diagnostic model transport: every `send` is an
/// independently counted transport entry (`entries`), with the dispatched
/// byte strings retained — a refused/expired call must leave `entries`
/// untouched. Scripted queued replies; never the live provider path.
struct CountingTransport {
    entries: u64,
    sends: Vec<Vec<u8>>,
    queued: std::collections::VecDeque<Result<Vec<(u64, WireFrame)>, ModelError>>,
}

impl Transport for CountingTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        _credential: &CredentialHandle,
        _endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        self.entries += 1;
        self.sends.push(bytes.to_vec());
        self.queued
            .pop_front()
            .unwrap_or_else(|| Err(ModelError::new(ModelErrorClass::Network, "unscripted send")))
    }
}

/// A completed stream through the real chunk→frame translator.
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

fn usage() -> Json {
    Json::obj([
        ("prompt_tokens", Json::Int(41)),
        ("completion_tokens", Json::Int(7)),
        ("total_tokens", Json::Int(48)),
    ])
}

/// Open the caller's turn scope (the driver owns turns in production; the
/// battery mints its own — the r5_19 precedent).
fn open_turn(store: &mut Store, run: &str, lease: &Lease) {
    use hharness_ledger::event::{Event, Producer, Scope};
    use hharness_provenance::ProvenanceRecord;
    let ev = Event {
        event_id: store.alloc_id("evt"),
        class: "lifecycle.turn.started".to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-env/1"),
        scope: Scope {
            turn_id: Some("turn.1".to_string()),
            ..Scope::default()
        },
        parent_event_id: store.head_event_id(run).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-env/1", store.now_ms())),
        content_kind: None,
        payload: Json::obj([("turn_id", Json::str("turn.1"))]),
    };
    store.append(run, lease, vec![ev]).unwrap();
}

fn call_plan_with(reservation_id: &str) -> ProviderRequestPlan {
    let mut p = plan();
    p.budget_reservation_id = Some(reservation_id.to_string());
    p
}

fn rows<'a>(
    store: &'a Store,
    run: &str,
    class: &str,
) -> Vec<&'a hharness_ledger::event::EventEnvelope> {
    store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .collect()
}

fn rows_for_call<'a>(
    store: &'a Store,
    run: &str,
    class: &str,
    mc: &str,
) -> Vec<&'a hharness_ledger::event::EventEnvelope> {
    store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class && e.scope.model_call_id.as_deref() == Some(mc))
        .collect()
}

fn event_id_of(store: &Store, run: &str, class: &str) -> String {
    rows(store, run, class)
        .first()
        .map(|e| e.event_id.clone())
        .unwrap_or_default()
}

/// The duplicate-charge detector surface: durable `consumed` rows grouped
/// by (source event, dimension, budget node) — any count > 1 is a
/// reconciliation double-charge.
fn duplicate_charge_present(store: &Store, run: &str) -> bool {
    let mut groups: BTreeMap<(String, String, String), u64> = BTreeMap::new();
    for e in rows(store, run, "control.budget.consumed") {
        let src = e
            .payload
            .get("source_event")
            .and_then(|s: &Json| s.get("event_id"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let dim = e
            .payload
            .get("dimension")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let bid = e
            .payload
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        *groups.entry((src, dim, bid)).or_insert(0) += 1;
    }
    groups.values().any(|n| *n > 1)
}

fn record(trial: &str, cell: OpsCell, rep: u8, outcome: TrialOutcome) -> TrialRecord {
    TrialRecord {
        trial_id: trial.to_string(),
        cell,
        rep,
        invoked: true,
        boundary_observed: true,
        fully_observable: true,
        outcome,
        violation: false,
        negative_control: false,
        control_failed: false,
    }
}

// ── AC-1 (positive) ───────────────────────────────────────────────────

#[test]
fn o6_zero_model_calls_refuses_before_transport() {
    let (_clk, mut store, run, lease) = open("o6", 1_000);
    let sink_dir = dir("o6-sink");
    let obs_dir = dir("o6-obs");
    let sentinel = Sentinel::create(&dir("o6-sent"), b"denied").unwrap();
    let sink = MarkerSink::open(&sink_dir).unwrap();
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund_root(&mut acc, &lease, 1, 3_600_000)
    };
    open_turn(&mut store, &run, &lease);
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();

    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials;
    let mut transport = CountingTransport {
        entries: 0,
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done", Some(usage())))].into(),
    };

    // The identical positive path: a funded prepare+dispatch completes —
    // the O1 comparator proving the ungated path was reachable.
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call1 = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id, "mc.1"),
    )
    .unwrap_or_else(|e| panic!("positive prepare refused: {e:?}"));
    let v = call1.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call1.reservation_id),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &policy(1),
    );
    assert!(matches!(v, DispatchVerdict::Completed { .. }), "{v:?}");
    assert_eq!(transport.entries, 1, "positive path entered transport once");
    assert_eq!(
        acc.remaining(&budget_id, primary(DimensionId::ModelCalls)),
        Some(0)
    );

    drop(acc);
    // Zero remaining model_calls: the identical attempted reserve refuses.
    let head = store.events(&run).unwrap().len();
    let refused = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        prepare(
            &mut acc,
            &lease,
            &dialect,
            &endpoints(),
            &mut creds,
            inputs(&budget_id, "mc.2"),
        )
    };
    assert!(
        matches!(refused, Err(PrepareRefusal::Unfunded { .. })),
        "{refused:?}"
    );
    assert_eq!(transport.entries, 1, "refused call never entered transport");
    // The owner ledgered the refusal for the starved holder.
    let refused_rows: Vec<_> = rows(&store, &run, "control.budget.reserved")
        .into_iter()
        .filter(|e| e.payload.get("outcome").and_then(Json::as_str) == Some("refused"))
        .collect();
    assert_eq!(refused_rows.len(), 1);
    assert_eq!(
        refused_rows[0].payload.get("holder").and_then(Json::as_str),
        Some("mc.2")
    );
    // No dependent mutation: no request row, no marker, sink delta zero;
    // refusal is not terminal exhaustion (no exceeded/stop).
    assert!(rows_for_call(&store, &run, "model.call.requested", "mc.2").is_empty());
    assert_eq!(sink.markers().unwrap().len(), 0);
    assert!(rows(&store, &run, "control.budget.exceeded").is_empty());
    assert!(rows(&store, &run, "control.decision").is_empty());
    assert_eq!(
        store.events(&run).unwrap().len(),
        head + 1,
        "the refused reserve is the only new row"
    );
    assert!(sentinel.intact());
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
        .unwrap();

    let verdict = o6_outcome(&O6Cell {
        observer_present: !OpsObserver::open(&obs_dir)
            .unwrap()
            .rows()
            .unwrap()
            .is_empty(),
        sentinel_intact: sentinel.intact(),
        same_path_positive: true,
        reserve_refused: true,
        transport_entries: 0,
        dependent_mutation: false,
        terminal_exhaustion: false,
    });
    assert_eq!(verdict, AccountOutcome::Pass);
}

#[test]
fn o7_wall_expiry_drains_then_stops() {
    let root = dir("o7");
    let (clk, mut store, run, lease) = open_at(&root, 1_000);
    let sink_dir = dir("o7-sink");
    let obs_dir = dir("o7-obs");
    let sentinel = Sentinel::create(&dir("o7-sent"), b"denied").unwrap();
    let sink = MarkerSink::open(&sink_dir).unwrap();
    // Real root + separate instrument budget (D3-F4); the wall cap on the
    // path (root) fixes the durable-origin deadline: origin 1_000 + 5_000.
    let (root_id, instrument_id) = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let root = fund_root(&mut acc, &lease, 100, 5_000);
        let child = fund_instrument(&mut acc, &lease, &root);
        (root, child)
    };
    open_turn(&mut store, &run, &lease);
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();

    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials;
    let mut transport = CountingTransport {
        entries: 0,
        sends: vec![],
        queued: vec![].into(),
    };
    // The committed effect: the whole-vector reserve is held (the
    // committed budget fact) and the sealed request bytes exist.
    let mut call = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        prepare(
            &mut acc,
            &lease,
            &dialect,
            &endpoints(),
            &mut creds,
            inputs_wall(&instrument_id, "mc.1", 5_000),
        )
        .unwrap_or_else(|e| panic!("prepare refused: {e:?}"))
    };
    assert_eq!(call.deadline_ms, Some(6_000), "created-origin deadline");
    let committed_seq = store.head(&run).unwrap().seq;

    // The declared bounded barrier: 5 s bound, 1 s observation interval,
    // 10 s fail-safe, grace = 0 — the committed effect waits while the
    // supervisor observes each interval.
    for step in 1..=5u64 {
        clk.advance(1_000);
        OpsObserver::open(&obs_dir)
            .unwrap()
            .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
            .unwrap();
        assert!(
            clk.now_ms() <= 6_000 + 10_000,
            "within the 10 s fail-safe {step}"
        );
    }
    assert_eq!(clk.now_ms(), 6_000, "actual wall reached the bound");

    // Expiry: new dispatch is blocked and the committed claim drains.
    let v = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let v = call.dispatch(
            &mut acc,
            &lease,
            &call_plan_with(&call.reservation_id),
            &mut transport,
            &mut hharness_gateway::attempts::NoSleep::default(),
            &policy(1),
        );
        // A new prepare on the same bound refuses — new dispatch stays blocked.
        let blocked = prepare(
            &mut acc,
            &lease,
            &dialect,
            &endpoints(),
            &mut creds,
            inputs_wall(&instrument_id, "mc.2", 5_000),
        );
        assert!(
            matches!(blocked, Err(PrepareRefusal::DeadlineExpired { .. })),
            "{blocked:?}"
        );
        assert!(acc
            .tree
            .reservation(&call.reservation_id)
            .map(|r| !r.outstanding)
            .unwrap_or(true));
        v
    };
    let DispatchVerdict::DeadlineExpired {
        deadline_ms,
        now_ms,
    } = v
    else {
        panic!("expected DeadlineExpired, got {v:?}");
    };
    assert_eq!((deadline_ms, now_ms), (6_000, 6_000));
    assert_eq!(transport.entries, 0, "expired call never entered transport");
    assert!(rows_for_call(&store, &run, "model.call.requested", "mc.1").is_empty());
    // The drain row: `released{release}` closed the committed claim.
    let drain = rows(&store, &run, "control.budget.released")
        .into_iter()
        .find(|e| e.payload.get("reason").and_then(Json::as_str) == Some("release"));
    assert!(drain.is_some(), "committed claim drained to released");

    // The actual wall span charges to the root; the bound trips the stop.
    let source = EventRef {
        run_id: run.clone(),
        event_id: event_id_of(&store, &run, "control.budget.released"),
    };
    let req = ChargeRequest {
        budget_id: root_id.clone(),
        quantity: ResourceQuantity {
            dimension: DimensionId::TimeWallMs,
            amount: 5_000,
            unit: "ms".to_string(),
            model_ref: None,
            measured_at: source.clone(),
        },
        source: source.clone(),
        attribution: Attribution::subject(run.clone(), root_id.clone(), "instrument:o7"),
        reservation_id: None,
        cache_ttl: None,
    };
    let consumed_before = rows(&store, &run, "control.budget.consumed").len();
    {
        let mut acc = Account::open(&mut store, &run).unwrap();
        acc.charge(&lease, &req).unwrap();
        // Repeated reconciliation cannot double charge — the landed
        // (source, dim) dedup makes the replay a no-op.
        acc.charge(&lease, &req).unwrap();
        // The stop: exceeded + decision{stop, budget_exhausted{time.wall_ms}}.
        let exceeded = acc.check(&root_id).unwrap();
        assert!(exceeded
            .iter()
            .any(|x| x.dimension == primary(DimensionId::TimeWallMs)));
        acc.exhaust(
            &lease,
            &root_id,
            primary(DimensionId::TimeWallMs),
            Some(&source),
        )
        .unwrap();
    }
    assert_eq!(
        rows(&store, &run, "control.budget.consumed").len(),
        consumed_before + 1,
        "repeated reconciliation added no second charge"
    );
    assert!(!duplicate_charge_present(&store, &run));
    assert!(rows(&store, &run, "control.budget.exceeded")
        .iter()
        .any(|e| e.payload.get("dimension").and_then(Json::as_str) == Some("time.wall_ms")));
    assert!(rows(&store, &run, "control.decision").iter().any(|e| e
        .payload
        .get("kind")
        .and_then(Json::as_str)
        == Some("stop")));
    // No later marker; the committed prefix is preserved.
    assert_eq!(
        sink.markers().unwrap().len(),
        0,
        "no later marker after the stop"
    );
    assert!(store.head(&run).unwrap().seq > committed_seq);
    assert!(sentinel.intact());
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
        .unwrap();

    // Rebuild: committed/incurred work survives — root/child holds,
    // deduplicated charges and the stop all refold identically.
    {
        let rebuilt = Account::open(&mut store, &run).unwrap();
        assert_eq!(
            rebuilt.physical_use().incurred.get(DimensionId::TimeWallMs),
            5_000,
            "incurred wall use survives the rebuild"
        );
        assert!(rebuilt.tree.stop_decision.is_some(), "the stop refolds");
    }
    assert!(!duplicate_charge_present(&store, &run));

    // Restart: the WAL root is dropped and reopened past the bound (and
    // past the dead writer's 60 s lease so the takeover fences it); the
    // durable-origin deadline still binds a new prepare — elapsed
    // deadline retained across restart, never session arithmetic.
    drop(store);
    let mut store2 = Store::open_with(
        &root,
        Box::new(ManualClock::at(120_000)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let lease2 = store2.restore(&run, "writer-b", 60_000).unwrap().lease;
    let mut acc2 = Account::open(&mut store2, &run).unwrap();
    let post_restart = prepare(
        &mut acc2,
        &lease2,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs_wall(&instrument_id, "mc.3", 5_000),
    );
    assert!(
        matches!(post_restart, Err(PrepareRefusal::DeadlineExpired { .. })),
        "the persisted deadline survives the restart: {post_restart:?}"
    );
    assert_eq!(
        acc2.physical_use().incurred.get(DimensionId::TimeWallMs),
        5_000,
        "rebuild on the reopened WAL preserves incurred use"
    );

    let verdict = o7_outcome(&O7Cell {
        observer_present: !OpsObserver::open(&obs_dir)
            .unwrap()
            .rows()
            .unwrap()
            .is_empty(),
        sentinel_intact: sentinel.intact(),
        barrier_bounded: true,
        committed_before_expiry: true,
        expiry_before_commit: false,
        deadline_enforced: true,
        bound_reached: true,
        new_dispatch_blocked: true,
        drain_landed: true,
        stop_landed: true,
        later_marker: false,
        duplicate_charge: false,
        committed_preserved: true,
        detector_present: true,
    });
    assert_eq!(verdict, AccountOutcome::Pass);
}

// ── AC-2 (negative) ───────────────────────────────────────────────────

#[test]
fn o6_negative_matrix_fails() {
    let with = |f: fn(&mut O6Cell)| {
        let mut c = O6Cell {
            observer_present: true,
            sentinel_intact: true,
            same_path_positive: true,
            reserve_refused: true,
            transport_entries: 0,
            dependent_mutation: false,
            terminal_exhaustion: false,
        };
        f(&mut c);
        match o6_outcome(&c) {
            AccountOutcome::Failed(x) => x,
            v => panic!("expected failure, got {v:?}"),
        }
    };
    assert_eq!(
        with(|c| c.reserve_refused = false),
        AccountFailure::MissingRefusal
    );
    assert_eq!(
        with(|c| c.transport_entries = 1),
        AccountFailure::DispatchOnRefusal,
        "a dispatch on the refused reserve fails"
    );
    assert_eq!(
        with(|c| c.dependent_mutation = true),
        AccountFailure::DependentMutation
    );
    assert_eq!(
        with(|c| c.terminal_exhaustion = true),
        AccountFailure::ExhaustionMislabel,
        "O6 is refusal, never terminal exhaustion"
    );
    assert_eq!(
        with(|c| c.same_path_positive = false),
        AccountFailure::SamePathUnproven
    );
    assert_eq!(
        with(|c| c.observer_present = false),
        AccountFailure::MissingObserver
    );
    assert_eq!(
        with(|c| c.sentinel_intact = false),
        AccountFailure::SentinelMutation
    );
}

#[test]
fn o7_expiry_before_commit_is_unreachable() {
    // The bound passes before anything commits — the cell's precondition
    // is unreachable, never a pass; the report retains `unreachable`.
    let (clk, mut store, run, lease) = open("o7-pre", 1_000);
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund_root(&mut acc, &lease, 100, 5_000)
    };
    open_turn(&mut store, &run, &lease);
    clk.advance(6_000); // past the durable-origin deadline before prepare
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials;
    let mut acc = Account::open(&mut store, &run).unwrap();
    let refused = prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id, "mc.1"),
    );
    assert!(matches!(
        refused,
        Err(PrepareRefusal::DeadlineExpired { .. })
    ));
    let verdict = o7_outcome(&O7Cell {
        observer_present: true,
        sentinel_intact: true,
        barrier_bounded: true,
        committed_before_expiry: false,
        expiry_before_commit: true,
        deadline_enforced: true,
        bound_reached: true,
        new_dispatch_blocked: true,
        drain_landed: true,
        stop_landed: true,
        later_marker: false,
        duplicate_charge: false,
        committed_preserved: true,
        detector_present: true,
    });
    assert_eq!(
        verdict,
        AccountOutcome::Failed(AccountFailure::ExpiryBeforeCommit),
        "expiry before commit is unreachable, never a pass"
    );
}

#[test]
fn o7_negative_matrix_fails() {
    let pass = O7Cell {
        observer_present: true,
        sentinel_intact: true,
        barrier_bounded: true,
        committed_before_expiry: true,
        expiry_before_commit: false,
        deadline_enforced: true,
        bound_reached: true,
        new_dispatch_blocked: true,
        drain_landed: true,
        stop_landed: true,
        later_marker: false,
        duplicate_charge: false,
        committed_preserved: true,
        detector_present: true,
    };
    let with = |f: fn(&mut O7Cell)| {
        let mut c = O7Cell {
            observer_present: pass.observer_present,
            sentinel_intact: pass.sentinel_intact,
            barrier_bounded: pass.barrier_bounded,
            committed_before_expiry: pass.committed_before_expiry,
            expiry_before_commit: pass.expiry_before_commit,
            deadline_enforced: pass.deadline_enforced,
            bound_reached: pass.bound_reached,
            new_dispatch_blocked: pass.new_dispatch_blocked,
            drain_landed: pass.drain_landed,
            stop_landed: pass.stop_landed,
            later_marker: pass.later_marker,
            duplicate_charge: pass.duplicate_charge,
            committed_preserved: pass.committed_preserved,
            detector_present: pass.detector_present,
        };
        f(&mut c);
        match o7_outcome(&c) {
            AccountOutcome::Failed(x) => x,
            v => panic!("expected failure, got {v:?}"),
        }
    };
    assert_eq!(with(|c| c.later_marker = true), AccountFailure::LaterMarker);
    assert_eq!(
        with(|c| c.duplicate_charge = true),
        AccountFailure::DuplicateCharge
    );
    assert_eq!(
        with(|c| c.detector_present = false),
        AccountFailure::MissingDetector
    );
    assert_eq!(
        with(|c| c.drain_landed = false),
        AccountFailure::MissingDrain
    );
    assert_eq!(with(|c| c.stop_landed = false), AccountFailure::MissingStop);
    assert_eq!(
        with(|c| c.committed_preserved = false),
        AccountFailure::CommittedReverted
    );
    assert_eq!(
        with(|c| c.new_dispatch_blocked = false),
        AccountFailure::NewDispatchAfterStop
    );
    assert_eq!(
        with(|c| c.barrier_bounded = false),
        AccountFailure::UnboundedBarrier
    );
    assert_eq!(
        with(|c| c.deadline_enforced = false),
        AccountFailure::MissingDeadline
    );
    assert_eq!(
        with(|c| c.bound_reached = false),
        AccountFailure::UnreachedBound
    );
    assert_eq!(
        with(|c| c.committed_before_expiry = false),
        AccountFailure::CommittedMissing
    );
}

#[test]
fn report_matrix_preserves_stage_counts_bounds_and_o8() {
    // The frozen seeded permutation: deterministic per seed, a different
    // seed permutes differently, each block holds all eight cells.
    let a = OpsReport::plan(0x05e3d);
    let b = OpsReport::plan(0x05e3d);
    let c = OpsReport::plan(0xbeef);
    assert_eq!(a.permutation(), b.permutation(), "frozen per seed");
    assert_ne!(a.permutation(), c.permutation(), "the seed permutes");
    assert_eq!(a.permutation().len(), 24);
    for block in a.permutation().chunks(8) {
        let mut cells: Vec<OpsCell> = block.iter().map(|(c, _)| *c).collect();
        cells.sort();
        cells.dedup();
        assert_eq!(cells.len(), 8, "balanced controls per block");
    }

    // A controlled cell fold: o6 with 3 planned — 2 invoked+boundary+T
    // (pass + refused), 1 unreachable never-invoked.
    let mut rep = OpsReport::plan(0x05e3d);
    rep.record(record("o6r0", OpsCell::O6, 0, TrialOutcome::Pass))
        .unwrap();
    rep.record(record("o6r1", OpsCell::O6, 1, TrialOutcome::Refused))
        .unwrap();
    rep.record(TrialRecord {
        invoked: false,
        boundary_observed: false,
        fully_observable: false,
        ..record("o6r2", OpsCell::O6, 2, TrialOutcome::Unreachable)
    })
    .unwrap();
    let counts = rep.counts(OpsCell::O6);
    assert_eq!(
        counts,
        StageCounts {
            planned: 3,
            invoked: 2,
            boundary_observed: 2,
            fully_observable: 2,
            violations: 0,
        }
    );
    assert_eq!(stage_ratio(2, 3), Ratio::Ppm(666_666));
    assert_eq!(
        stage_ratio(0, 0),
        Ratio::Undefined("zero_denominator"),
        "zero denominator is undefined, never a rate"
    );
    let (lo, hi) = missingness_bound(&counts);
    // U = P − T = 1 → [0/3, (0+1)/3] = [0, 333_333] ppm.
    assert_eq!(lo, Ratio::Ppm(0));
    assert_eq!(hi, Ratio::Ppm(333_333));

    // O8's separate detector sensitivity: 3 seeded duplicates, 3 detected.
    for i in 0..OPS_REPETITIONS {
        rep.record(record(
            &OpsReport::trial_id(OpsCell::O8, i),
            OpsCell::O8,
            i,
            TrialOutcome::DefectDetected,
        ))
        .unwrap();
    }
    assert_eq!(rep.o8_detected(), (3, 3), "detected/3 seeded");

    // Retained outcome kinds ride the tally — no post-outcome exclusion.
    let kinds = [
        TrialOutcome::TierOff,
        TrialOutcome::MissingDetector,
        TrialOutcome::Timeout,
        TrialOutcome::OracleFailure,
        TrialOutcome::Unknown,
        TrialOutcome::Cancelled,
    ];
    for (i, k) in kinds.iter().enumerate() {
        rep.record(record(&format!("o1r{i}"), OpsCell::O1, i as u8, *k))
            .unwrap();
    }
    let rendered = rep.render();
    let outcomes = rendered.get("outcomes").unwrap();
    for k in [
        "pass",
        "defect_detected",
        "refusal",
        "tier_off",
        "unreachable",
        "missing_detector",
        "timeout",
        "oracle_failure",
        "unknown",
        "cancellation",
    ] {
        assert!(outcomes.get(k).is_some(), "retained outcome {k}");
    }
    // A cell with no records renders undefined ratios, never panics.
    let cells = rendered.get("cells").unwrap();
    if let Json::Arr(rows) = cells {
        let o5 = rows
            .iter()
            .find(|r| r.get("cell").and_then(Json::as_str) == Some("o5"))
            .unwrap();
        assert_eq!(
            o5.get("invoked_per_planned")
                .and_then(|r| r.get("undefined"))
                .and_then(Json::as_str),
            Some("zero_denominator")
        );
    } else {
        panic!("cells must be an array");
    }
    assert!(!rep.acceptance_blocked());
}

#[test]
fn report_replaced_original_excess_retry_and_failed_control() {
    let mut rep = OpsReport::plan(7);
    rep.record(record("o6r0", OpsCell::O6, 0, TrialOutcome::Pass))
        .unwrap();
    // A re-record under the same id is a replaced original — refused,
    // the original row survives unchanged.
    let err = rep
        .record(record("o6r0", OpsCell::O6, 0, TrialOutcome::Failed))
        .unwrap_err();
    assert_eq!(
        err,
        ReportError::ReplacedOriginal {
            trial_id: "o6r0".to_string()
        }
    );
    assert_eq!(
        rep.cell_records(OpsCell::O6)[0].outcome,
        TrialOutcome::Pass,
        "the original record is never replaced"
    );
    // One setup diagnostic rerun per cell, new identity — the cap.
    let rerun_id = rep.setup_rerun(OpsCell::O6, 0).unwrap();
    assert_eq!(rerun_id, "o6r0s1");
    rep.record(record(&rerun_id, OpsCell::O6, 0, TrialOutcome::Pass))
        .unwrap();
    assert_eq!(
        rep.setup_rerun(OpsCell::O6, 0),
        Err(ReportError::SetupRerunCapExceeded { cell: OpsCell::O6 }),
        "excess setup retry fails"
    );
    // A failed negative control blocks acceptance.
    rep.record(TrialRecord {
        negative_control: true,
        control_failed: true,
        ..record("o3r0", OpsCell::O3, 0, TrialOutcome::Failed)
    })
    .unwrap();
    assert!(rep.acceptance_blocked(), "a failed negative control blocks");
    // An integrity breach stops the matrix immediately.
    rep.integrity_stop("sentinel_mutation");
    let rendered = rep.render();
    assert_eq!(
        rendered.get("stopped").and_then(Json::as_str),
        Some("sentinel_mutation")
    );
}
