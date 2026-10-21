//! R3.3 — the control-plane residual legs (docs/tickets/139; DF-S1.20-1;
//! BL-14; R-2.6.1/R-2.6.3/R-2.2.3):
//!
//! - **`woken{peer_message}` interpreted distinctly** — the steerable
//!   interpreter answers a delivered `peer_message{from}` wakeup with
//!   `propose{plan, context_request{peer_message_ref, from}}` — a context
//!   artefact, never the steer/follow-up interpretation; the arm selects
//!   on the trigger's content class even under
//!   `steer_mode = unsupported` (M-1: a peer message never widens
//!   authority, budget or a validator).
//! - **steerable delegation legs** — a model call on the declared
//!   delegation surface mints `control.delegate.proposed{tool_call_id,
//!   spec}` (the `hh.plan` analogue); the steerable fold emits the
//!   `delegate` decision the react preset's `delegate = model` point owns
//!   — the spec's own `budget`/`requested_grants`/`delegation_reason`
//!   ride the audit row (a missing reason refuses
//!   `missing_delegation_reason` at `check`, never a default).
//! - **the `DelegatePort`/`StoreSeam` dispatch** — no seam or no port ⇒
//!   `UnbackedPort{delegate}`; the port's rows land durable under the
//!   fenced writer before visibility (CC3); a substrate refusal returns
//!   to β as `envelope_signal{refused}` (F2).
//! - **`hh/workflow@1`/`hh/program@1` real runs under
//!   `compute_policy = static`** — the `PlanWalker` interprets the closed
//!   `PlanNodePayload` set: `step{invoke}` → `act`, `step{instruction}` →
//!   `propose`, `branch-on-validator` → `verify` + a fail-closed verdict
//!   fold, `delegate` → the substrate seam, `stop-rule` skipped (the
//!   envelope's own declarations), exhausted → `stop{completed}`. The
//!   staged `wait{human_input}` park is gone — a plan-required variant
//!   with no `ctx.plan` refuses `MissingProcedure`, and
//!   `plan_provenance = model_emitted` refuses typed.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_compiler::plan::{
    BranchOnValidatorNode, DelegateNode, PinnedRef, PlanNode, PlanNodePayload, StepAction,
    StepMode, StepNode,
};
use hh_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, DelegateOutcome, DelegatePort, Driver,
    DriverConfig, DriverError, EffectGate, GateOutcome, LedgerSink, ModelOutcome, ModelPort,
    StoreSeam,
};
use hh_control::output::{ParamKind, ParamSpec, SurfaceSpec};
use hh_control::policy::EnvelopePolicy;
use hh_control::react::{self, ReactSteerable};
use hh_control::state::ControlState;
use hh_control::strategy::{
    ConcurrentInput, ControlContext, ControlError, ControlStrategy, SteerMode, StrategyParams,
};
use hh_control::vocab::{
    ControlDecision, Cue, DecisionKind, DeliveryMode, EffectOutcome, EnvelopeSignal, HumanInput,
    SettledOutcome, WokenTrigger,
};
use hh_control::walk::{plan_walker_caps, PlanWalker};
use hh_ledger::classes::Durability;
use hh_ledger::event::{Event, EventEnvelope, EventPlane, Producer, Scope};
use hh_ledger::manifest::{ParticipantClass, RunManifest};
use hh_ledger::store::Store;
use hh_ontology::control::{DecisionPoint, Owner, StopReason};
use hh_wire::json::Json;

// ── fixtures ────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ctl-r33-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn ctx_with_plan(plan: Vec<PlanNode>, steering: (SteerMode, ConcurrentInput)) -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan,
        boundary: plan_walker_caps().boundary_preset,
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec!["hh.submit".into(), "hh.delegate".into()],
        steering,
    }
}

fn react_ctx() -> ControlContext {
    ControlContext {
        boundary: react::react_preset(),
        ..ctx_with_plan(vec![], (SteerMode::Unsupported, ConcurrentInput::QueueOnly))
    }
}

fn ev(seq: u64, class: &str, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("e{seq}"),
        run_id: "run".into(),
        seq,
        ts: "2026-10-14T00:00:00.000Z".into(),
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
        parent_event_id: hh_ledger::ids::ROOT_EVENT.into(),
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

/// The `hh.submit` binding — the envelope's submission gate reads the
/// delivered surface set (`config.surfaces`), not the ctx's capability
/// list.
fn submit_surface() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: "hh.submit".into(),
        semantic_id: "sem/hh.submit".into(),
        params: [(
            "text".into(),
            ParamSpec {
                required: true,
                kind: ParamKind::Str,
                enum_values: vec![],
                domain: vec![],
            },
        )]
        .into_iter()
        .collect(),
        risk_class: None,
    }
}

fn instruction_node(id: &str) -> PlanNode {
    PlanNode {
        node_id: id.into(),
        hir_node_id: "hir:n".into(),
        hir_version_id: "v1".into(),
        payload: PlanNodePayload::Step(StepNode {
            mode: StepMode::Sequential,
            output_schema: None,
            action: StepAction::Instruction {
                content_hash: "sha256:instr".into(),
                owner: "owner:p".into(),
            },
        }),
    }
}

fn invoke_node(id: &str) -> PlanNode {
    PlanNode {
        node_id: id.into(),
        hir_node_id: "hir:n".into(),
        hir_version_id: "v1".into(),
        payload: PlanNodePayload::Step(StepNode {
            mode: StepMode::Sequential,
            output_schema: None,
            action: StepAction::Invoke {
                capability: PinnedRef {
                    semantic_id: "cap:test".into(),
                    version_id: "v1".into(),
                },
                args: Json::obj([("path", Json::str("x"))]),
            },
        }),
    }
}

/// A `step{invoke}` against a named surface (the submit leg is an
/// ordinary invoke — the code-owned walk `act`s it; the gate's
/// `submission_ref` is what `stop{completed}` grounds on).
fn invoke_surface_node(id: &str, surface: &str, args: Json) -> PlanNode {
    PlanNode {
        node_id: id.into(),
        hir_node_id: "hir:n".into(),
        hir_version_id: "v1".into(),
        payload: PlanNodePayload::Step(StepNode {
            mode: StepMode::Sequential,
            output_schema: None,
            action: StepAction::Invoke {
                capability: PinnedRef {
                    semantic_id: surface.into(),
                    version_id: "v1".into(),
                },
                args,
            },
        }),
    }
}

fn delegate_node(id: &str, spec: Json) -> PlanNode {
    PlanNode {
        node_id: id.into(),
        hir_node_id: "hir:n".into(),
        hir_version_id: "v1".into(),
        payload: PlanNodePayload::Delegate(DelegateNode {
            spec,
            budget: PinnedRef {
                semantic_id: "budget:child".into(),
                version_id: "v1".into(),
            },
            permission: PinnedRef {
                semantic_id: "perm:child".into(),
                version_id: "v1".into(),
            },
        }),
    }
}

/// The in-memory sink — `store_seam` stays `None` (the `UnbackedPort`
/// arm); the store-backed sink below carries the seam.
struct MemSink {
    events: Vec<EventEnvelope>,
    seq: u64,
}

impl MemSink {
    fn new() -> Self {
        MemSink {
            events: vec![],
            seq: 0,
        }
    }
    fn classes(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.class.as_str()).collect()
    }
    fn find(&self, class: &str) -> Vec<&EventEnvelope> {
        self.events.iter().filter(|e| e.class == class).collect()
    }
}

fn push_event(sink: &mut Vec<EventEnvelope>, seq: &mut u64, e: Event) {
    *seq += 1;
    sink.push(EventEnvelope {
        event_id: e.event_id,
        run_id: "run".into(),
        seq: *seq,
        ts: e.ts,
        hlc: None,
        plane: EventPlane::of_class(&e.class).unwrap_or(EventPlane::Control),
        class: e.class,
        schema_version: 1,
        producer: e.producer,
        participant_class: ParticipantClass::Native,
        observability_level: Default::default(),
        durability: Durability::Ledger,
        scope: e.scope,
        lease_generation: 1,
        parent_event_id: e.parent_event_id,
        causes: e.causes,
        refs: vec![],
        ir_refs: vec![],
        surface_ids: Default::default(),
        provenance: e.provenance,
        payload: e.payload,
        prev_hash: "h".into(),
        hash: format!("h{seq}"),
    });
}

impl LedgerSink for MemSink {
    fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
        for e in events {
            push_event(&mut self.events, &mut self.seq, e);
        }
        Ok(())
    }
    fn prefix(&self) -> &[EventEnvelope] {
        &self.events
    }
}

/// The store-backed sink — a real `Store` + writer lease under the seam
/// (the `DelegatePort` legs the driver hands `StoreSeam` to).
struct StoreSink {
    store: Store,
    run_id: String,
    lease: hh_ledger::store::Lease,
}

impl StoreSink {
    fn open(tag: &str) -> Self {
        let mut store = Store::open_test(dir(tag), 1_000).unwrap();
        let (run_id, lease) = store
            .open_run(
                RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
                "writer",
            )
            .unwrap();
        StoreSink {
            store,
            run_id,
            lease,
        }
    }
    fn classes(&self) -> Vec<String> {
        self.store
            .events(&self.run_id)
            .unwrap_or(&[])
            .iter()
            .map(|e| e.class.clone())
            .collect()
    }
}

impl LedgerSink for StoreSink {
    fn append(&mut self, mut events: Vec<Event>) -> Result<(), String> {
        for e in events.iter_mut() {
            if e.provenance.is_none() {
                e.provenance = Some(hh_provenance::ProvenanceRecord::kernel(
                    "hh-test/sink",
                    self.store.now_ms(),
                ));
            }
        }
        self.store
            .append(&self.run_id, &self.lease, events)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
    fn prefix(&self) -> &[EventEnvelope] {
        self.store.events(&self.run_id).unwrap_or(&[])
    }
    fn store_seam(&mut self) -> Option<StoreSeam<'_>> {
        Some(StoreSeam {
            store: &mut self.store,
            lease: &self.lease,
        })
    }
}

/// A scripted model — pops outcomes in order.
struct ScriptedModel {
    script: VecDeque<ModelOutcome>,
}

impl ModelPort for ScriptedModel {
    fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
        self.script.pop_front().unwrap_or(ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-empty".into(),
            text_empty: true,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        })
    }
}

/// Every intent answers `observed{applied}`.
struct ScriptedGate;

impl EffectGate for ScriptedGate {
    fn dispatch(&mut self, _ef: &str, _a: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        GateOutcome {
            outcome: SettledOutcome::Observed {
                outcome: "applied".into(),
            },
            submission_ref: (surface == "hh.submit").then(|| "sub-1".to_string()),
            error_class: None,
            emitted: Vec::new(),
        }
    }
}

/// The minimal assembler.
struct FakeAssembler;

impl AssemblerPort for FakeAssembler {
    fn assemble(&mut self, inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
        AssembledRequest {
            request: Json::obj([("model_call_id", Json::str(inputs.model_call_id))]),
            assembled_payload: None,
            side_events: vec![],
            pre_events: vec![],
        }
    }
}

/// `ScriptedStrategy` — `ReactSteerable` underneath; `(cue tag, decide
/// fn)` rules — first match wins (the driver's F1 arms run honestly).
struct ScriptedStrategy {
    inner: ReactSteerable,
    script: Vec<(&'static str, DecisionKind)>,
    fired: RefCell<Vec<bool>>,
    /// The last `envelope_signal{refused}.reason` the strategy saw.
    saw_refused: Arc<Mutex<Option<String>>>,
}

impl ScriptedStrategy {
    fn new(script: Vec<(&'static str, DecisionKind)>) -> Self {
        ScriptedStrategy {
            inner: ReactSteerable::new(),
            fired: RefCell::new(vec![false; script.len()]),
            script,
            saw_refused: Arc::new(Mutex::new(None)),
        }
    }
}

impl ControlStrategy for ScriptedStrategy {
    fn capabilities(&self) -> &hh_control::strategy::ControlCapabilities {
        self.inner.capabilities()
    }
    fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
        self.inner.open(ctx)
    }
    fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
        self.inner.observe(state, events);
    }
    fn decide(&self, state: &mut ControlState, cue: &Cue) -> ControlDecision {
        if let Cue::EnvelopeSignal(EnvelopeSignal::Refused { reason, .. }) = cue {
            *self.saw_refused.lock().unwrap() = Some(reason.clone());
        }
        let tag = match cue {
            Cue::RunOpened { .. } => "run_opened",
            Cue::EnvelopeSignal(_) => "envelope_signal",
            Cue::HumanInput(_) => "human_input",
            _ => "other",
        };
        for (i, (t, k)) in self.script.iter().enumerate() {
            if *t == tag && !self.fired.borrow()[i] {
                self.fired.borrow_mut()[i] = true;
                let d = ControlDecision {
                    stamp: hh_control::strategy::stamp_for(
                        state,
                        match k {
                            DecisionKind::Delegate { .. } => DecisionPoint::Delegate,
                            DecisionKind::Stop { .. } => DecisionPoint::Stop,
                            _ => DecisionPoint::Act,
                        },
                        None,
                    ),
                    kind: k.clone(),
                };
                state.record_decision(d.stamp.decision_point, d.stamp.owner);
                return d;
            }
        }
        self.inner.decide(state, cue)
    }
    fn terminate(
        &self,
        state: &ControlState,
        reason: &StopReason,
    ) -> hh_control::strategy::FinalReport {
        self.inner.terminate(state, reason)
    }
    fn restore(
        &mut self,
        bytes: &[u8],
        ctx: &ControlContext,
    ) -> Result<ControlState, hh_control::strategy::RestoreError> {
        self.inner.restore(bytes, ctx)
    }
}

/// A `DelegatePort` that appends a `control.subagent.spawned` row under
/// the seam's fenced writer and answers `Spawned`.
struct RecordingDelegatePort {
    calls: usize,
}

impl DelegatePort for RecordingDelegatePort {
    fn delegate(
        &mut self,
        seam: StoreSeam<'_>,
        decision_ref: &hh_ledger::manifest::EventRef,
        _owner: Owner,
        _spec: &Json,
        _budget_slice: &Json,
        _permissions: &Json,
    ) -> DelegateOutcome {
        self.calls += 1;
        let ev = Event {
            event_id: seam.store.alloc_id("evt"),
            class: "control.subagent.spawned".into(),
            ts: seam.store.ts_now(),
            hlc: None,
            producer: Producer::kernel("kernel:test"),
            scope: Scope::default(),
            parent_event_id: decision_ref.event_id.clone(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: Default::default(),
            provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 1)),
            content_kind: None,
            payload: Json::obj([("child_run_id", Json::str("sub-1"))]),
        };
        seam.store
            .append(&decision_ref.run_id, seam.lease, vec![ev])
            .unwrap();
        DelegateOutcome::Spawned {
            child_run_id: "sub-1".into(),
        }
    }
}

/// A `DelegatePort` answering the typed substrate refusal.
struct RefusingDelegatePort;

impl DelegatePort for RefusingDelegatePort {
    fn delegate(
        &mut self,
        _seam: StoreSeam<'_>,
        _decision_ref: &hh_ledger::manifest::EventRef,
        _owner: Owner,
        _spec: &Json,
        _budget_slice: &Json,
        _permissions: &Json,
    ) -> DelegateOutcome {
        DelegateOutcome::Refused {
            class: "authority_widening".into(),
            detail: "fs_write".into(),
        }
    }
}

fn delegate_decision(reason: Option<Json>) -> DecisionKind {
    DecisionKind::Delegate {
        spec: Json::obj([("delegation_reason", reason.clone().unwrap_or(Json::Null))]),
        budget_slice: Json::Null,
        permissions: Json::Arr(vec![]),
        delegation_reason: reason,
    }
}

// ── steerable — woken{peer_message} is its own interpretation ───────────

#[test]
fn peer_message_woken_is_a_context_artefact_not_a_steer() {
    let mut v = ReactSteerable::new();
    let mut st = v.open(&react_ctx()).unwrap();
    let d = v.decide(
        &mut st,
        &Cue::Woken {
            trigger: WokenTrigger::PeerMessage {
                from: "run-peer".into(),
            },
            payload_ref: "sha256:msg".into(),
            delivery_mode: DeliveryMode::FollowUp,
        },
    );
    match &d.kind {
        DecisionKind::Propose {
            context_request, ..
        } => {
            assert_eq!(
                context_request
                    .get("peer_message_ref")
                    .and_then(Json::as_str),
                Some("sha256:msg"),
                "the peer-message leg carries the delivery ref — {context_request:?}"
            );
            assert_eq!(
                context_request.get("from").and_then(Json::as_str),
                Some("run-peer")
            );
            assert!(
                context_request.get("steer_ref").is_none()
                    && context_request.get("follow_up_ref").is_none(),
                "a peer message is never the steer/follow-up interpretation"
            );
        }
        other => panic!("peer_message must propose under the delivery, got {other:?}"),
    }
}

#[test]
fn peer_message_woken_selects_on_trigger_not_delivery_lane() {
    // Under `steer_mode = unsupported` the steer/follow-up arms are
    // disabled — the peer-message arm still interprets (the trigger's
    // content class selects, not the subscription's lane).
    let mut v = ReactSteerable::new();
    let mut st = v.open(&react_ctx()).unwrap();
    let d = v.decide(
        &mut st,
        &Cue::Woken {
            trigger: WokenTrigger::PeerMessage {
                from: "run-peer".into(),
            },
            payload_ref: "sha256:msg".into(),
            delivery_mode: DeliveryMode::Steer,
        },
    );
    match &d.kind {
        DecisionKind::Propose {
            context_request, ..
        } => assert!(
            context_request.get("peer_message_ref").is_some(),
            "peer_message under a steer lane still reads the artefact arm: {context_request:?}"
        ),
        other => panic!("peer_message must propose under the delivery, got {other:?}"),
    }
}

// ── steerable — the delegation ask fold ─────────────────────────────────

#[test]
fn steerable_folds_delegate_proposed_into_the_delegate_decision() {
    let mut v = ReactSteerable::new();
    let mut st = v.open(&react_ctx()).unwrap();
    // The driver minted `control.delegate.proposed` at G-INTERPRET — the
    // strategy folds it at `observe` (records-in/records-out, I2).
    v.observe(
        &mut st,
        &[ev(
            1,
            "control.delegate.proposed",
            Json::obj([
                ("tool_call_id", Json::str("tc-1")),
                (
                    "spec",
                    Json::obj([
                        ("delegation_reason", Json::str("parallelism")),
                        ("budget", Json::obj([("budget_ref", Json::str("b-c"))])),
                        ("requested_grants", Json::Arr(vec![Json::str("fs.read")])),
                    ]),
                ),
            ]),
        )],
    );
    let d = v.decide(
        &mut st,
        &Cue::ModelCompleted {
            model_call_id: "mc-1".into(),
            response_ref: "r-1".into(),
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        },
    );
    match &d.kind {
        DecisionKind::Delegate {
            spec,
            budget_slice,
            permissions,
            delegation_reason,
        } => {
            assert_eq!(
                delegation_reason.as_ref().and_then(Json::as_str),
                Some("parallelism"),
                "the spec's own delegation_reason rides the decision"
            );
            assert_eq!(
                spec.get("delegation_reason").and_then(Json::as_str),
                Some("parallelism")
            );
            assert!(budget_slice.get("budget_ref").is_some(), "{budget_slice:?}");
            assert_eq!(permissions, &Json::Arr(vec![Json::str("fs.read")]));
        }
        other => panic!("the folded ask must emit delegate, got {other:?}"),
    }
    assert_eq!(d.stamp.decision_point, DecisionPoint::Delegate);
    // The pending set drains — a second ToolUse completion does not
    // re-emit the same ask (idempotent on `tool_call_id`).
    let d2 = v.decide(
        &mut st,
        &Cue::ModelCompleted {
            model_call_id: "mc-2".into(),
            response_ref: "r-2".into(),
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        },
    );
    assert!(
        !matches!(d2.kind, DecisionKind::Delegate { .. }),
        "a drained ask must not re-delegate: {d2:?}"
    );
}

// ── the delegate execution seam ─────────────────────────────────────────

#[test]
fn delegate_without_reason_refuses_typed_at_check() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let strategy = ScriptedStrategy::new(vec![
        ("run_opened", delegate_decision(None)),
        (
            "envelope_signal",
            DecisionKind::Stop {
                proposed_reason: StopReason::Refused {
                    blocking_effect_id: "d-1".into(),
                },
                submission_ref: None,
            },
        ),
    ]);
    let saw = strategy.saw_refused.clone();
    let mut driver = Driver::open(
        strategy,
        &react_ctx(),
        policy,
        &mut sink,
        DriverConfig {
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: VecDeque::new(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver
        .run(&mut model, &mut gate, &mut asm, &mut sink)
        .unwrap();
    assert!(
        matches!(r.report.stop_reason, StopReason::Refused { .. }),
        "{:?}",
        r.report.stop_reason
    );
    assert_eq!(
        saw.lock().unwrap().as_deref(),
        Some("missing_delegation_reason"),
        "a reason-less delegate refuses the typed reason at check"
    );
    // The refused proposal mints no `control.decision` row (the audit
    // table records admitted decisions); the durable refusal evidence is
    // `lifecycle.run.finished{stop_reason}` plus the strategy's admitted
    // stop — never a fabricated `delegation_reason` member.
    assert!(
        !sink.find("lifecycle.run.finished").is_empty(),
        "the refused run finishes durable: {:?}",
        sink.classes()
    );
}

#[test]
fn delegate_without_store_seam_is_unbacked_port() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut driver = Driver::open(
        ScriptedStrategy::new(vec![(
            "run_opened",
            delegate_decision(Some(Json::str("parallelism"))),
        )]),
        &react_ctx(),
        policy,
        &mut sink,
        DriverConfig {
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: VecDeque::new(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let err = driver
        .run(&mut model, &mut gate, &mut asm, &mut sink)
        .unwrap_err();
    assert!(
        matches!(err, DriverError::UnbackedPort { kind } if kind == "delegate"),
        "an admitted delegate with no seam fails typed: {err:?}"
    );
}

#[test]
fn delegate_port_rows_land_durable_before_visibility() {
    let mut sink = StoreSink::open("spawn");
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let strategy = ScriptedStrategy::new(vec![
        (
            "run_opened",
            delegate_decision(Some(Json::str("parallelism"))),
        ),
        (
            "human_input",
            DecisionKind::Stop {
                proposed_reason: StopReason::Cancelled {
                    by: hh_ontology::control::CancelledBy::Principal,
                },
                submission_ref: None,
            },
        ),
    ]);
    let mut driver = Driver::open(
        strategy,
        &react_ctx(),
        policy,
        &mut sink,
        DriverConfig {
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    driver.set_delegate_port(Box::new(RecordingDelegatePort { calls: 0 }));
    let mut model = ScriptedModel {
        script: VecDeque::new(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    // The inbox parks after the delegate executes — the caller submits
    // the interrupt that finishes the run.
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    driver.submit(Cue::HumanInput(HumanInput::Interrupt));
    let r = driver
        .run(&mut model, &mut gate, &mut asm, &mut sink)
        .unwrap();
    assert!(
        matches!(r.report.stop_reason, StopReason::Cancelled { .. }),
        "{:?}",
        r.report.stop_reason
    );
    let classes = sink.classes();
    let decision_pos = classes
        .iter()
        .position(|c| c == "control.decision")
        .expect("the delegate decision row landed");
    let spawned_pos = classes
        .iter()
        .position(|c| c == "control.subagent.spawned")
        .expect("the port's spawn row landed durable");
    assert!(
        decision_pos < spawned_pos,
        "durable-before-visible: the decision precedes the spawn row: {classes:?}"
    );
}

#[test]
fn delegate_port_refusal_returns_to_beta_as_refused() {
    let mut sink = StoreSink::open("refused");
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let strategy = ScriptedStrategy::new(vec![
        (
            "run_opened",
            delegate_decision(Some(Json::str("parallelism"))),
        ),
        (
            "envelope_signal",
            DecisionKind::Stop {
                proposed_reason: StopReason::Refused {
                    blocking_effect_id: "d-1".into(),
                },
                submission_ref: None,
            },
        ),
    ]);
    let saw = strategy.saw_refused.clone();
    let mut driver = Driver::open(
        strategy,
        &react_ctx(),
        policy,
        &mut sink,
        DriverConfig {
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    driver.set_delegate_port(Box::new(RefusingDelegatePort));
    let mut model = ScriptedModel {
        script: VecDeque::new(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver
        .run(&mut model, &mut gate, &mut asm, &mut sink)
        .unwrap();
    assert!(
        matches!(r.report.stop_reason, StopReason::Refused { .. }),
        "{:?}",
        r.report.stop_reason
    );
    assert!(
        saw.lock()
            .unwrap()
            .as_deref()
            .is_some_and(|r| r.starts_with("authority_widening")),
        "the substrate refusal returns to β verbatim: {:?}",
        saw.lock().unwrap()
    );
    assert!(
        !sink
            .classes()
            .iter()
            .any(|c| c == "control.subagent.spawned"),
        "a refused spawn mints no child row"
    );
}

// ── the PlanWalker — hh/workflow@1 / hh/program@1 real runs ─────────────

#[test]
fn workflow_driver_run_walks_the_plan_to_completed() {
    // A real run under `compute_policy = static`: invoke → act →
    // effects_settled → instruction → propose → model_completed →
    // invoke{hh.submit} → act → settled → exhausted → stop{completed}
    // (the submission grounds the completed stop — the walker's own
    // invoke, never a fabricated member). No scheduler rows mint
    // (AC-F4-1).
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let ctx = ctx_with_plan(
        vec![
            invoke_node("n1"),
            instruction_node("n2"),
            invoke_surface_node("n3", "hh.submit", Json::obj([("text", Json::str("done"))])),
        ],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    let mut driver = Driver::open(
        PlanWalker::new("hh/workflow@1", plan_walker_caps()),
        &ctx,
        policy,
        &mut sink,
        DriverConfig {
            compute_policy_ref: "static".into(),
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: VecDeque::from([ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-1".into(),
            text_empty: false,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        }]),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver
        .run(&mut model, &mut gate, &mut asm, &mut sink)
        .unwrap();
    assert!(
        matches!(r.report.stop_reason, StopReason::Completed),
        "the walked plan ends completed: {:?}",
        r.report.stop_reason
    );
    let kinds: Vec<&str> = sink
        .find("control.decision")
        .iter()
        .filter_map(|e| e.payload.get("kind").and_then(Json::as_str))
        .collect();
    assert_eq!(
        kinds,
        vec!["act", "propose", "act", "stop"],
        "the walk's decision sequence is the plan's own order: {kinds:?}"
    );
    assert!(
        sink.find("control.compute.decided").is_empty(),
        "compute_policy = static mints no scheduler rows"
    );
}

#[test]
fn workflow_branch_on_validator_is_fail_closed() {
    let mut v = PlanWalker::new("hh/workflow@1", plan_walker_caps());
    let ctx = ctx_with_plan(
        vec![
            PlanNode {
                node_id: "n1".into(),
                hir_node_id: "hir:n".into(),
                hir_version_id: "v1".into(),
                payload: PlanNodePayload::BranchOnValidator(BranchOnValidatorNode {
                    validator: PinnedRef {
                        semantic_id: "validator:test".into(),
                        version_id: "v1".into(),
                    },
                    then_body: vec![instruction_node("t1")],
                    else_body: vec![invoke_node("e1")],
                }),
            },
            instruction_node("n2"),
        ],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    let mut st = v.open(&ctx).unwrap();
    // `run_opened` → verify (the branch asks the validator first).
    let d = v.decide(
        &mut st,
        &Cue::RunOpened {
            goal_ref: "g".into(),
            inputs: Json::Null,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Verify { .. }),
        "the branch's first decision is verify: {d:?}"
    );
    // The verdict fold — a non-affirmative verdict walks `else`
    // (fail-closed: `decided`+affirmative is the only `then` pass).
    v.observe(
        &mut st,
        &[ev(
            9,
            "verification.validator.verdict",
            Json::obj([
                ("status", Json::str("inconclusive")),
                ("value", Json::obj([("bool", Json::Bool(false))])),
            ]),
        )],
    );
    let d = v.decide(&mut st, &Cue::VerificationCompleted);
    assert!(
        matches!(d.kind, DecisionKind::Act { .. }),
        "a non-affirmative verdict takes the else arm: {d:?}"
    );
    // The else body is one invoke — it settles, then `n2` proposes.
    let d = v.decide(
        &mut st,
        &Cue::EffectsSettled {
            settled: vec![EffectOutcome {
                effect_id: "ef-1".into(),
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
            }],
            all_terminal: true,
            submission_ref: None,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Propose { .. }),
        "the else arm's tail is the next plan node: {d:?}"
    );
}

#[test]
fn workflow_delegate_node_emits_delegate_and_awaits_terminal() {
    let mut v = PlanWalker::new("hh/workflow@1", plan_walker_caps());
    let spec = Json::obj([
        ("delegation_reason", Json::str("parallelism")),
        ("wait", Json::obj([("mode", Json::str("await"))])),
    ]);
    let ctx = ctx_with_plan(
        vec![delegate_node("n1", spec.clone()), instruction_node("n2")],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    let mut st = v.open(&ctx).unwrap();
    let d = v.decide(
        &mut st,
        &Cue::RunOpened {
            goal_ref: "g".into(),
            inputs: Json::Null,
        },
    );
    match &d.kind {
        DecisionKind::Delegate {
            spec: s,
            delegation_reason,
            ..
        } => {
            assert_eq!(s, &spec, "the node's own spec rides the audit row");
            assert_eq!(
                delegation_reason.as_ref().and_then(Json::as_str),
                Some("parallelism")
            );
        }
        other => panic!("the delegate node must emit delegate, got {other:?}"),
    }
    // `wait.mode = await` — a mismatched cue parks `wait{woken}`; the
    // child-terminal wakeup resumes the walk.
    let d = v.decide(
        &mut st,
        &Cue::Woken {
            trigger: WokenTrigger::PeerMessage {
                from: "peer".into(),
            },
            payload_ref: "sha256:m".into(),
            delivery_mode: DeliveryMode::FollowUp,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Wait { .. }),
        "await mode parks until the child terminal: {d:?}"
    );
    let d = v.decide(
        &mut st,
        &Cue::Woken {
            trigger: WokenTrigger::ChildTerminal {
                child_run_id: "sub-1".into(),
            },
            payload_ref: "".into(),
            delivery_mode: DeliveryMode::FollowUp,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Propose { .. }),
        "the child terminal resumes the walk at n2: {d:?}"
    );
}

#[test]
fn workflow_checkpoint_restore_round_trips_the_position() {
    let mut v = PlanWalker::new("hh/workflow@1", plan_walker_caps());
    let ctx = ctx_with_plan(
        vec![invoke_node("n1"), instruction_node("n2")],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    let mut st = v.open(&ctx).unwrap();
    let _ = v.decide(
        &mut st,
        &Cue::RunOpened {
            goal_ref: "g".into(),
            inputs: Json::Null,
        },
    );
    // The checkpoint bytes carry the walk position — restore re-binds the
    // plan from ctx and continues at `await_effect`, never re-opening.
    let bytes = st.to_json().to_canonical_string().into_bytes();
    let mut v2 = PlanWalker::new("hh/workflow@1", plan_walker_caps());
    let mut st2 = v2.restore(&bytes, &ctx).unwrap();
    let d = v2.decide(
        &mut st2,
        &Cue::EffectsSettled {
            settled: vec![EffectOutcome {
                effect_id: "ef-1".into(),
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
            }],
            all_terminal: true,
            submission_ref: None,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Propose { .. }),
        "the restored walk resumes at the next node: {d:?}"
    );
    // A checkpoint written against a different variant refuses typed.
    let mut v3 = PlanWalker::new("hh/program@1", plan_walker_caps());
    assert!(
        matches!(
            v3.restore(&bytes, &ctx),
            Err(hh_control::strategy::RestoreError::VariantMismatch { .. })
        ),
        "cross-variant restore refuses typed"
    );
}

#[test]
fn workflow_model_emitted_plan_refuses_typed() {
    let mut v = PlanWalker::new("hh/workflow@1", plan_walker_caps());
    let mut ctx = ctx_with_plan(
        vec![instruction_node("n1")],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    ctx.parameters.plan_provenance = hh_control::strategy::PlanProvenance::ModelEmitted;
    let err = v.open(&ctx).unwrap_err();
    assert!(
        matches!(err, ControlError::UnsupportedPlanNode { .. }),
        "model_emitted under a compiled interpreter refuses typed: {err:?}"
    );
}

#[test]
fn program_walks_the_same_interpreter() {
    let mut v = react::variant_for("hh/program@1").unwrap();
    let ctx = ctx_with_plan(
        vec![invoke_node("n1")],
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
    );
    let mut st = v.open(&ctx).unwrap();
    let d = v.decide(
        &mut st,
        &Cue::RunOpened {
            goal_ref: "g".into(),
            inputs: Json::Null,
        },
    );
    assert!(
        matches!(d.kind, DecisionKind::Act { .. }),
        "hh/program@1 walks the same RuntimePlan/1 interpreter: {d:?}"
    );
    // `stop-rule` nodes are skipped — the envelope's own declarations,
    // never sequential steps (the tail rules the compiler appends).
    let d = v.decide(
        &mut st,
        &Cue::EffectsSettled {
            settled: vec![],
            all_terminal: true,
            submission_ref: None,
        },
    );
    assert!(
        matches!(
            d.kind,
            DecisionKind::Stop {
                proposed_reason: StopReason::Completed,
                ..
            }
        ),
        "an exhausted plan proposes completed: {d:?}"
    );
}

#[test]
fn workflow_missing_plan_is_a_typed_refusal_never_a_park() {
    let mut v = react::variant_for("hh/workflow@1").unwrap();
    let ctx = ctx_with_plan(vec![], (SteerMode::Unsupported, ConcurrentInput::QueueOnly));
    let err = v.open(&ctx).unwrap_err();
    assert!(
        matches!(err, ControlError::MissingProcedure),
        "an empty ctx.plan refuses MissingProcedure — the staged park is gone: {err:?}"
    );
}
