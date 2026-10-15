//! R3.3 — the steerable delegation leg end to end (docs/tickets/139;
//! DF-S1.20-1; R-2.6.3): a real `Driver<ReactSteerable>` run over a real
//! `Store`, the model's `hh.delegate` call minting
//! `control.delegate.proposed`, the fold emitting `delegate` under the
//! react preset's `delegate = model` point, and `SpawnDelegatePort`
//! executing through `hh-subagent`'s one `spawn` path — the
//! `control.subagent.spawned` + grant rows land durable under the parent's
//! fenced writer before the next visible cue (CC3), the spec's own
//! `delegation_reason` rides the audit row, and a malformed/attenuation-
//! breaking ask answers the typed refusal (never a coerced spawn).

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::plan::PinnedRef;
use hh_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, DelegateOutcome, DelegatePort, Driver,
    DriverConfig, EffectGate, GateOutcome, LedgerSink, ModelOutcome, ModelPort, StoreSeam,
};
use hh_control::output::ParsedCall;
use hh_control::policy::EnvelopePolicy;
use hh_control::react::{self, ReactSteerable};
use hh_control::vocab::{Cue, HumanInput, SettledOutcome};
use hh_env::handle::OnParentEnd;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_monitor::decision::DecisionScope;
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::ownership::OwnedObject;
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_orchestrator::{ParentBinding, SpawnDelegatePort};
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::types::{
    ChildProcess, ChildRole, ContextIsolation, DelegationReason, EnvIsolation, ReturnContract,
    SubagentSpec, Supplies, WaitMode,
};
use hh_wire::json::Json;

// ── fixtures (the s4_6 seam set — the real store, never a mock) ─────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-orch-r33-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn kernel_ev(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
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

fn grant(domain: EffectDomain, scope: &str, delegable: bool) -> Grant {
    Grant {
        effect: EffectClass::domain_only(domain),
        scope: scope.to_string(),
        constraints: GrantConstraints {
            budget: None,
            time: None,
            count: None,
        },
        delegable,
    }
}

fn parent_handle(run_id: &str) -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId("hnd-parent".to_string()),
        permission_ref: PinnedRef {
            semantic_id: "perm:root".to_string(),
            version_id: "v1".to_string(),
        },
        holder: Ref::pinned("agent_process", run_id.to_string()),
        issuer: ProvenanceRecord::kernel("kernel:test", 0),
        grants: vec![
            grant(EffectDomain::FsRead, "*", true),
            grant(EffectDomain::FsWrite, "*", true),
            grant(EffectDomain::ModelCall, "*", true),
            grant(EffectDomain::SpawnProcess, "*", true),
        ],
        ceiling: AuthorityClass::Delegate,
        validity: HandleValidity {
            issued_at: "evt-mint".to_string(),
            expires_at: Some(HandleExpiry::Run(run_id.to_string())),
            revoked_by: None,
        },
        parent_handle: None,
        delegable: true,
        origin_basis: OriginBasis::Seal,
        basis_ref: "def:test#v1".to_string(),
        budget_ref: None,
        scope: DecisionScope::Session,
    }
}

fn root_spec() -> BudgetSpec {
    BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
            (DimensionKey::Primary(DimensionId::Spawns), 16),
            (DimensionKey::Primary(DimensionId::FanOut), 16),
            (DimensionKey::Primary(DimensionId::DelegationDepth), 4),
        ],
    )
}

struct Parent {
    store: Store,
    run_id: String,
    lease: hh_ledger::store::Lease,
    budget_id: String,
    handle_id: HandleId,
    ownerships: OwnershipTable,
    handles: HandleTable,
}

impl Parent {
    /// The binding the port owns — rebuilt per call (`ParentBinding`
    /// carries the lease by value; the seam set stays cheap data).
    fn binding(&self, owner: Owner) -> ParentBinding {
        ParentBinding {
            parent_run_id: self.run_id.clone(),
            parent_lease: self.lease.clone(),
            parent_handle_id: self.handle_id.clone(),
            parent_budget_id: self.budget_id.clone(),
            parent_env_id: None,
            ownerships: self.ownerships.clone(),
            tool_table: Vec::new(),
            parent_depth: 0,
            holder: "writer-child".to_string(),
            coords: LiveCoords {
                effect_id: "eff-0".into(),
                turn_id: "turn-0".into(),
                run_id: self.run_id.clone(),
                session: String::new(),
                seq: 0,
            },
            reserve_ttl_ms: 30_000,
            owner,
        }
    }
}

fn open_parent(tag: &str) -> Parent {
    let mut store = Store::open_test(dir(tag), 1_000).unwrap();
    let (run_id, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-parent")
        .unwrap();
    let mut account = Account::open(&mut store, &run_id).unwrap();
    let budget_id = account
        .allocate(
            &lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: run_id.clone(),
            },
            root_spec(),
        )
        .unwrap();
    let handle = parent_handle(&run_id);
    let handle_id = handle.handle_id.clone();
    let mut handles = HandleTable::default();
    handles.handles.insert(handle_id.clone(), handle);
    let ownerships = OwnershipTable::project(
        &store,
        &[],
        vec![(run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    Parent {
        store,
        run_id,
        lease,
        budget_id,
        handle_id,
        ownerships,
        handles,
    }
}

fn stage_spec(tag: &str) -> SubagentSpec {
    SubagentSpec {
        process: ChildProcess::Native {
            harness_def: Ref::pinned("def:child", format!("sha256:{}", "d".repeat(64))),
            profile_binding: None,
            control_strategy: None,
            slots: None,
        },
        goal: GoalRecord {
            statement: Text::new(
                format!("stage {tag}"),
                "owner:parent",
                ProvenanceRecord::kernel("kernel:test", 0),
            ),
            success_criteria: vec![Ref::pinned("validator:noop", "v1")],
            unverifiable_reason: None,
            budget: Ref::pinned("budget:child", "v1"),
            origin: GoalOrigin::Delegated,
            parent: Some(Ref::pinned("goal:parent", "v1")),
        },
        role: ChildRole::Subagent,
        requested_grants: vec![grant(EffectDomain::FsRead, "/src", true)],
        ceiling: None,
        budget_spec: BudgetSpec::hard_caps(
            BudgetMode::Slice,
            &[
                (DimensionKey::Primary(DimensionId::ModelCalls), 10),
                (DimensionKey::Primary(DimensionId::Spawns), 0),
                (DimensionKey::Primary(DimensionId::FanOut), 0),
                (DimensionKey::Primary(DimensionId::DelegationDepth), 0),
            ],
        ),
        budget_mode: BudgetMode::Slice,
        context: ContextIsolation::Fresh,
        environment: EnvIsolation::None,
        supplies: Supplies::default(),
        return_contract: ReturnContract::default(),
        wait_mode: WaitMode::Await,
        wait_timeout_ms: 60_000,
        on_parent_end: OnParentEnd::Teardown,
        delegation_reason: DelegationReason::Parallelism,
        topology_ref: None,
        stage_index: None,
        ownership_grants: Vec::new(),
        reserved_keys: Vec::new(),
        consistency_declarations: Vec::new(),
        merge_policy_ref: None,
        messaging_policy: None,
    }
}

/// The store-backed `LedgerSink` — `store_seam` hands the
/// `DelegatePort` the parent's fenced writer (the CC3 leg).
struct StoreSink<'a> {
    store: &'a mut Store,
    run_id: String,
    lease: &'a hh_ledger::store::Lease,
}

impl LedgerSink for StoreSink<'_> {
    fn append(&mut self, mut events: Vec<Event>) -> Result<(), String> {
        for e in events.iter_mut() {
            if e.provenance.is_none() {
                e.provenance = Some(ProvenanceRecord::kernel(
                    "hh-test/sink",
                    self.store.now_ms(),
                ));
            }
        }
        self.store
            .append(&self.run_id, self.lease, events)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
    fn prefix(&self) -> &[EventEnvelope] {
        self.store.events(&self.run_id).unwrap_or(&[])
    }
    fn store_seam(&mut self) -> Option<StoreSeam<'_>> {
        Some(StoreSeam {
            store: &mut *self.store,
            lease: self.lease,
        })
    }
}

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

struct FakeAssembler;

impl AssemblerPort for FakeAssembler {
    fn assemble(&mut self, inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
        AssembledRequest {
            request: Json::obj([("model_call_id", Json::str(inputs.model_call_id))]),
            assembled_payload: None,
            side_events: vec![],
            pre_events: vec![],
            overflow: None,
        }
    }
}

/// The `hh.delegate` delegation surface — the bounded ask
/// `{spec_ref, delegation_reason}` (`spec` inline for small asks); the
/// 512 B `model.call.completed` audit bound keeps the spec record
/// content-addressed, never inline.
fn delegate_surface() -> hh_control::output::SurfaceSpec {
    let p = |required| hh_control::output::ParamSpec {
        required,
        kind: hh_control::output::ParamKind::Any,
        enum_values: vec![],
        domain: vec![],
    };
    hh_control::output::SurfaceSpec {
        surface_id: "hh.delegate".into(),
        semantic_id: "sem/hh.delegate".into(),
        params: [
            ("spec".into(), p(false)),
            ("spec_ref".into(), p(false)),
            ("delegation_reason".into(), p(false)),
        ]
        .into_iter()
        .collect(),
        risk_class: None,
        family: None,
    }
}

/// Stage the `SubagentSpec` content-addressed — the model's bounded ask
/// names it `{"spec_ref": "sha256:<digest>"}`.
fn stage_blob(store: &mut Store, spec: &SubagentSpec) -> String {
    store
        .put_blob(
            spec.to_json().to_canonical_string().as_bytes(),
            "application/json",
        )
        .unwrap()
        .id()
}

fn classes(parent: &Parent) -> Vec<String> {
    parent
        .store
        .events(&parent.run_id)
        .unwrap_or(&[])
        .iter()
        .map(|e| e.class.clone())
        .collect()
}

// ── the real-run delegation leg ─────────────────────────────────────────

#[test]
fn steerable_run_delegates_through_the_spawn_port() {
    // The model answers a tool_use turn with a `hh.delegate` call — the
    // driver mints `control.delegate.proposed` (never an ordinary
    // `action.tool.proposed`), the steerable fold emits `delegate` under
    // `delegate = model`, and the port spawns through the child-lease
    // substrate. Ordering is durable-before-visible.
    let mut parent = open_parent("leg");
    let spec_json = stage_spec("leg").to_json();
    let spec_ref = stage_blob(&mut parent.store, &stage_spec("leg"));
    let args_raw = Json::obj([
        ("spec_ref", Json::str(&spec_ref)),
        ("delegation_reason", Json::str("parallelism")),
    ])
    .to_canonical_string();
    let _ = spec_json;
    let policy = EnvelopePolicy::stage1_default(&parent.budget_id)
        .seal()
        .unwrap();
    let ctx = hh_control::strategy::ControlContext {
        process_ref: parent.run_id.clone(),
        plan: vec![],
        boundary: react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: parent.budget_id.clone(),
        envelope_ref: "env-1".into(),
        parameters: Default::default(),
        capabilities_available: vec!["hh.delegate".into()],
        steering: (
            hh_control::strategy::SteerMode::Unsupported,
            hh_control::strategy::ConcurrentInput::QueueOnly,
        ),
    };
    // The port's binding + handles clone before `parent.store` is
    // mutably borrowed by the sink.
    let port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    let mut sink = StoreSink {
        store: &mut parent.store,
        run_id: parent.run_id.clone(),
        lease: &parent.lease,
    };
    let mut driver = Driver::open(
        ReactSteerable::new(),
        &ctx,
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![delegate_surface()],
            delegate_surface_id: Some("hh.delegate".into()),
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    driver.set_delegate_port(Box::new(port));
    let mut model = ScriptedModel {
        script: VecDeque::from([ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            response_ref: "r-1".into(),
            text_empty: false,
            calls: vec![ParsedCall {
                tool_call_id: "tc-delegate".into(),
                surface: "hh.delegate".into(),
                args_raw,
            }],
            error_class: None,
            retry_after_ms: None,
        }]),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    // The run parks once the delegate executes (the child's terminal is
    // the substrate's own wakeup — the test ends it with an interrupt).
    let first = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    assert!(
        matches!(first, Err(hh_control::driver::DriverError::Port { port, .. }) if port == "inbox"),
        "the run parks once the delegate executes: {first:?}"
    );
    driver.submit(Cue::HumanInput(HumanInput::Interrupt));
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let cls = classes(&parent);
    let proposed = cls
        .iter()
        .position(|c| c == "control.delegate.proposed")
        .expect("the model's hh.delegate call minted the durable ask");
    let events = parent.store.events(&parent.run_id).unwrap();
    let decision = events
        .iter()
        .position(|e| {
            e.class == "control.decision"
                && e.payload.get("kind").and_then(Json::as_str) == Some("delegate")
        })
        .expect("the admitted delegate decision landed");
    let spawned = cls
        .iter()
        .position(|c| c == "control.subagent.spawned")
        .expect("the spawn landed durable under the fenced writer");
    assert!(
        proposed < decision && decision < spawned,
        "durable-before-visible: proposed < decision < spawned: {cls:?}"
    );
    // The delegation surface is never an ordinary tool proposal.
    assert!(
        !cls.iter().any(|c| c == "action.tool.proposed"),
        "a `hh.delegate` call is never an ordinary tool effect: {cls:?}"
    );
    // The decision row records the spec's own delegation_reason.
    let decision_row = events
        .iter()
        .find(|e| {
            e.class == "control.decision"
                && e.payload.get("kind").and_then(Json::as_str) == Some("delegate")
        })
        .unwrap();
    assert_eq!(
        decision_row
            .payload
            .get("delegation_reason")
            .and_then(Json::as_str),
        Some("parallelism"),
        "the declared reason rides the audit row: {}",
        decision_row.payload.to_canonical_string()
    );
    // The child run exists — the deterministic `sub-<idp>` id.
    let spawned_row = events
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .unwrap();
    let child = spawned_row
        .payload
        .get("child_run_id")
        .and_then(Json::as_str)
        .expect("the spawned row names the child run")
        .to_string();
    assert!(
        !parent.store.events(&child).unwrap_or(&[]).is_empty(),
        "the child's own ledger opened: {child}"
    );
}

#[test]
fn missing_delegation_reason_refuses_before_the_port() {
    // A spec whose `delegation_reason` member is absent — the fold emits
    // `delegate` with the member absent, `check` refuses
    // `missing_delegation_reason`, and the port never runs (no spawn).
    let mut parent = open_parent("noreason");
    let spec_ref = stage_blob(&mut parent.store, &stage_spec("noreason"));
    // The bounded ask omits `delegation_reason` — the fold emits the
    // delegate with the member absent; `check` refuses typed.
    let args_raw = Json::obj([("spec_ref", Json::str(&spec_ref))]).to_canonical_string();
    let policy = EnvelopePolicy::stage1_default(&parent.budget_id)
        .seal()
        .unwrap();
    let ctx = hh_control::strategy::ControlContext {
        process_ref: parent.run_id.clone(),
        plan: vec![],
        boundary: react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: parent.budget_id.clone(),
        envelope_ref: "env-1".into(),
        parameters: Default::default(),
        capabilities_available: vec!["hh.delegate".into()],
        steering: (
            hh_control::strategy::SteerMode::Unsupported,
            hh_control::strategy::ConcurrentInput::QueueOnly,
        ),
    };
    // The port's binding + handles clone before `parent.store` is
    // mutably borrowed by the sink.
    let port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    let mut sink = StoreSink {
        store: &mut parent.store,
        run_id: parent.run_id.clone(),
        lease: &parent.lease,
    };
    let mut driver = Driver::open(
        ReactSteerable::new(),
        &ctx,
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![delegate_surface()],
            delegate_surface_id: Some("hh.delegate".into()),
            delegation_available: true,
            ..DriverConfig::default()
        },
    )
    .unwrap();
    driver.set_delegate_port(Box::new(port));
    let mut model = ScriptedModel {
        script: VecDeque::from([ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            response_ref: "r-1".into(),
            text_empty: false,
            calls: vec![ParsedCall {
                tool_call_id: "tc-delegate".into(),
                surface: "hh.delegate".into(),
                args_raw,
            }],
            error_class: None,
            retry_after_ms: None,
        }]),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    driver.submit(Cue::HumanInput(HumanInput::Interrupt));
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let cls = classes(&parent);
    assert!(
        cls.iter().any(|c| c == "control.delegate.proposed"),
        "the ask is durable: {cls:?}"
    );
    assert!(
        !cls.iter().any(|c| c == "control.subagent.spawned"),
        "a reason-less ask never reaches the port: {cls:?}"
    );
    let delegate_admitted = parent
        .store
        .events(&parent.run_id)
        .unwrap_or(&[])
        .iter()
        .any(|e| {
            e.class == "control.decision"
                && e.payload.get("kind").and_then(Json::as_str) == Some("delegate")
        });
    assert!(
        !delegate_admitted,
        "a refused proposal mints no admitted decision row"
    );
}

// ── the port's own typed refusals ───────────────────────────────────────

#[test]
fn malformed_spec_is_a_typed_refusal_never_a_spawn() {
    let mut parent = open_parent("malformed");
    let ev = kernel_ev(
        &parent.store,
        &parent.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
        ]),
    );
    let decision_ref = EventRef {
        run_id: parent.run_id.clone(),
        event_id: ev.event_id.clone(),
    };
    parent
        .store
        .append(&parent.run_id, &parent.lease, vec![ev])
        .unwrap();
    let mut port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    // A reasoned-but-malformed ask — the codec's typed refusal.
    let bad = Json::obj([
        ("delegation_reason", Json::str("parallelism")),
        ("not_a_spec", Json::Bool(true)),
    ]);
    let lease = parent.lease.clone();
    let out = port.delegate(
        StoreSeam {
            store: &mut parent.store,
            lease: &lease,
        },
        &decision_ref,
        Owner::Model,
        &bad,
        &Json::Null,
        &Json::Null,
    );
    match out {
        DelegateOutcome::Refused { class, .. } => {
            assert_eq!(class, "malformed_subagent_spec")
        }
        other => panic!("a malformed spec is a typed refusal: {other:?}"),
    }
    assert!(
        !classes(&parent)
            .iter()
            .any(|c| c == "control.subagent.spawned"),
        "a malformed ask mints no child"
    );
}

#[test]
fn unresolvable_spec_ref_is_a_typed_refusal() {
    let mut parent = open_parent("noref");
    let ev = kernel_ev(
        &parent.store,
        &parent.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
        ]),
    );
    let decision_ref = EventRef {
        run_id: parent.run_id.clone(),
        event_id: ev.event_id.clone(),
    };
    parent
        .store
        .append(&parent.run_id, &parent.lease, vec![ev])
        .unwrap();
    let mut port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    let lease = parent.lease.clone();
    let ask = Json::obj([("spec_ref", Json::str(format!("sha256:{}", "0".repeat(64))))]);
    let out = port.delegate(
        StoreSeam {
            store: &mut parent.store,
            lease: &lease,
        },
        &decision_ref,
        Owner::Model,
        &ask,
        &Json::Null,
        &Json::Null,
    );
    match out {
        DelegateOutcome::Refused { class, .. } => {
            assert_eq!(class, "unresolvable_spec_ref")
        }
        other => panic!("an unresolvable spec_ref is a typed refusal: {other:?}"),
    }
}

#[test]
fn spec_beyond_parent_grants_refuses_authority_widening() {
    let mut parent = open_parent("widening");
    let ev = kernel_ev(
        &parent.store,
        &parent.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
        ]),
    );
    let decision_ref = EventRef {
        run_id: parent.run_id.clone(),
        event_id: ev.event_id.clone(),
    };
    parent
        .store
        .append(&parent.run_id, &parent.lease, vec![ev])
        .unwrap();
    // The spec asks `fs.write *` — the parent's `fs.write` grant is
    // `delegable` but the requested scope exceeds what the substrate
    // attests… the honest version: a domain the parent never granted at
    // all (`net.egress` is not on the handle).
    let mut spec = stage_spec("widening");
    spec.requested_grants = vec![grant(EffectDomain::NetEgress, "*", true)];
    let mut port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    let lease = parent.lease.clone();
    let out = port.delegate(
        StoreSeam {
            store: &mut parent.store,
            lease: &lease,
        },
        &decision_ref,
        Owner::Code,
        &spec.to_json(),
        &Json::Null,
        &Json::Null,
    );
    match out {
        DelegateOutcome::Refused { class, .. } => assert_eq!(
            class, "authority_widening",
            "an ask beyond the parent's delegable grants refuses typed"
        ),
        other => panic!("a widening ask must refuse, got {other:?}"),
    }
}

#[test]
fn spawn_is_idempotent_on_decision_and_spec() {
    // `sub-<idp(decision, H(spec))>` — the pair is the idempotency unit:
    // a retried `delegate` returns the same child, never a second spawn.
    let mut parent = open_parent("idem");
    let ev = kernel_ev(
        &parent.store,
        &parent.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
        ]),
    );
    let decision_ref = EventRef {
        run_id: parent.run_id.clone(),
        event_id: ev.event_id.clone(),
    };
    parent
        .store
        .append(&parent.run_id, &parent.lease, vec![ev])
        .unwrap();
    let spec = stage_spec("idem").to_json();
    let mut port = SpawnDelegatePort {
        binding: parent.binding(Owner::Model),
        handles: parent.handles.clone(),
        env_driver: None,
    };
    let lease = parent.lease.clone();
    let first = port.delegate(
        StoreSeam {
            store: &mut parent.store,
            lease: &lease,
        },
        &decision_ref,
        Owner::Code,
        &spec,
        &Json::Null,
        &Json::Null,
    );
    let child = match first {
        DelegateOutcome::Spawned { child_run_id } => child_run_id,
        other => panic!("the first spawn lands: {other:?}"),
    };
    let second = port.delegate(
        StoreSeam {
            store: &mut parent.store,
            lease: &lease,
        },
        &decision_ref,
        Owner::Code,
        &spec,
        &Json::Null,
        &Json::Null,
    );
    match second {
        DelegateOutcome::Spawned { child_run_id } => assert_eq!(
            child_run_id, child,
            "the (decision, spec) pair is one idempotent unit"
        ),
        other => panic!("a replayed spawn answers the same child: {other:?}"),
    }
    let spawned = classes(&parent)
        .iter()
        .filter(|c| *c == "control.subagent.spawned")
        .count();
    assert_eq!(spawned, 1, "one durable spawn row for the pair");
}
