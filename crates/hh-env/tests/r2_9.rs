//! R2.9a acceptance battery — the egress/containment legs the split ticket
//! lands (DF-S1.12-1/2, DF-S2.4-1). Every cell names the contract it pins
//! and fails if the behaviour is removed:
//!
//! - `resources → BudgetNode` (DF-S1.12-2) — `resource_budget_spec` maps the
//!   eight `ResourceLimits` members onto registered dimensions and provision
//!   allocates a **pool child under the run's root** (`budget_node_refs`);
//!   no root ⇒ typed `BudgetRefused`, a widening child refuses (CC9), and
//!   nothing installs an unbudgeted bound set.
//! - `endorse_gate` (DF-S2.4-1b) — the monitor's answer resolves at the
//!   *gate* stage: `allow_lease` mints `security.containment.amended`,
//!   returns `GateAllow{decided_by: monitor, amended_policy}`, and the wire
//!   never fired; `deny` terminates with `decided{deny, monitor}` durable.
//! - the dispatch resume leg (DF-S2.4-1b) — an answered egress pending
//!   enters through `endorse_gate` (never a second `requested`/pending
//!   trail); unanswered, the dispatch re-suspends behind the same wakeup.
//! - `apply_amended_containment` — the amended policy lands on the live
//!   handle so the next `decide_egress` sees the new `version_id` (CC3).
//! - fork credential custody (DF-S2.4-1c; LT-09) — the cross-run rebind
//!   takes the parent *handle record* (the child driver's table never holds
//!   it), mints fresh placeholder spellings, preserves the expiry ceiling,
//!   and refuses a non-derive parent edge typed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_budget::account::Account;
use hh_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hh_compiler::plan::PinnedRef;
use hh_compiler::surface::{BindingMapping, CompileExposureMode};
use hh_containment::admit::ContainmentDiff;
use hh_containment::amend::amend;
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::egress::{ApprovalCache, EgressRequest};
use hh_containment::events::DecidedBy;
use hh_containment::policy::{
    kernel_default, AmendmentBasis, ContainmentPolicy, DefaultUnmatched, EgressProtocol,
    ExecPolicy, HostPattern, IsolationClass, NetMode, NonPublic, ResourceLimits, RuleDecision,
    WritableRoot,
};
use hh_env::capture::OutputPolicy;
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::{resource_budget_spec, EnvDriver};
use hh_env::egress::{
    EgressMediator, EgressTransport, GateOutcome, MediatedOutcome, StaticResolver, WireResponse,
};
use hh_env::errors::EnvError;
use hh_env::events::{intended_payload, EventMinter, ScopeChain};
use hh_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hh_env::handle::{OnLoss, Roots};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_env::tokens::TokenMinter;
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_hir::refs::Ref;
use hh_hir::tools::ExposureMode;
use hh_identity::idp::address;
use hh_ledger::event::{Cursor, Direction, Event, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_monitor::approval::{ApprovalResponse, EndorserRef, LeaseSpec, ResponseChoice};
use hh_monitor::assess::SecretTransport;
use hh_monitor::decision::DecisionScope;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, Monitor};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hh_provenance::origin::HumanRole;
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_secrets::{
    AccessClass, AuthCarrier, BindRequest, CredentialBroker, CredentialKind, DestinationBinding,
    SecretChannel, SecretChannelSpec, SecretSource, SenderConstraint, StaticVault,
};
use hh_wire::json::Json;

// ── scaffold (the acceptance battery's shape) ────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r29-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease, clock)
}

fn open_scopes(store: &mut Store, run: &str, lease: &Lease, turn: &str, mc: &str) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some(turn.to_string());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some(turn.to_string()),
        model_call_id: Some(mc.to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![t, c]).unwrap();
}

/// The scope members the mediated rows carry — `turn ⊃ mc ⊃ tc` plus an
/// `action.effect.intended` opener for the derived effect id (the ledger's
/// scope discipline: a member an event names must already be open).
fn open_egress_scopes(store: &mut Store, run: &str, lease: &Lease) -> String {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    let mut p = m.mint("action.tool.proposed", Json::obj([])).unwrap();
    p.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        tool_call_id: Some("tc-1".into()),
        ..Scope::default()
    };
    let risk = RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RiskRepeatSafety::Idempotent,
        scope: RiskScope::External,
    };
    let eid = Store::effect_id(run, "mc-1", "tc-1", 0);
    let intended = m
        .mint_effect(
            "action.effect.intended",
            intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            &eid,
            &chain(),
        )
        .unwrap();
    store.append(run, lease, vec![t, c, p, intended]).unwrap();
    eid
}

fn k_ev(store: &Store, run_id: &str, id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
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

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A run-root budget the env children pool under — `network.calls` covers
/// the wire leg's charge, the `hh.env.*` dims cover the declared bounds.
fn root_budget(store: &mut Store, run: &str, lease: &Lease, cpu_ms: i64) -> String {
    let mut acct = Account::open(store, run).unwrap();
    acct.allocate(
        lease,
        None,
        BudgetScope {
            kind: BudgetScopeKind::AgentProcess,
            target: "run:main".to_string(),
        },
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[
                (DimensionKey::Primary(DimensionId::ExtEnvCpuMs), cpu_ms),
                (
                    DimensionKey::Primary(DimensionId::ExtEnvMemoryBytes),
                    1 << 30,
                ),
                (DimensionKey::Primary(DimensionId::NetworkCalls), 100),
            ],
        ),
    )
    .unwrap()
}

fn test_record(limits: ResourceLimits) -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalHost,
        image: ImageRef::ContentAddress(address(b"r2.9-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits,
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

/// The test policy — kernel default + the workspace writable, `net = none`.
fn test_policy(ws: &std::path::Path) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}

/// A `mediated` policy that *asks* on unmatched hosts and admits
/// `approval`-basis amendments — the ask/endorse legs' floor.
fn ask_policy(ws: &std::path::Path) -> ContainmentPolicy {
    let mut p = test_policy(ws);
    p.net.mode = NetMode::Mediated;
    p.net.default_unmatched = DefaultUnmatched::Ask;
    p.net.non_public_destinations = NonPublic::AllowListed;
    p.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Approval]);
    p.amendment.session_cache = true;
    p.amendment.persist_scope_ceiling = PersistenceScope::Run;
    p.compute_ids();
    p
}

fn roots_for(ws: &std::path::Path) -> Roots {
    Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    }
}

/// `provision + attach(Ep2Model, fail-closed)` → a `ready` handle id.
fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
    limits: ResourceLimits,
    policy: ContainmentPolicy,
) -> String {
    let h = driver
        .provision(
            store,
            lease,
            &test_record(limits),
            roots_for(ws),
            PolicySlot::Inline(Box::new(policy)),
            OnLoss::FailRun,
            None,
        )
        .unwrap();
    let backend = Ep2Model::reference();
    driver
        .attach(
            store,
            lease,
            &h.env_handle_id,
            Some(&backend),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    h.env_handle_id
}

fn read_all(store: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

fn class_count(store: &Store, run: &str, class: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class)
        .count()
}

fn jstr(j: &Json, key: &str) -> Option<String> {
    j.get(key).and_then(|v| v.as_str().map(str::to_string))
}

// ── egress mediator scaffold (the S2.4 battery's shape) ──────────────────────

#[derive(Clone, Default)]
struct CaptureTransport {
    log: Arc<Mutex<Vec<String>>>,
}

impl EgressTransport for CaptureTransport {
    fn forward(
        &self,
        _addr: std::net::IpAddr,
        _port: u16,
        host: &str,
        _method: &str,
        _path: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> Result<WireResponse, String> {
        self.log.lock().unwrap().push(host.to_string());
        Ok(WireResponse {
            status: 200,
            headers: vec![],
            body: b"ok".to_vec(),
        })
    }
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-1".into(),
        tool_call_id: "tc-1".into(),
    }
}

fn egress_req(token: &str, effect_id: &str, env: &str, host: &str) -> EgressRequest {
    EgressRequest {
        token: token.to_string(),
        effect_id: Some(effect_id.to_string()),
        tool_call_id: format!("call-{effect_id}"),
        env_handle: env.to_string(),
        protocol: EgressProtocol::Http,
        host_raw: host.to_string(),
        resolved_addrs: vec![],
        port: 443,
        method: Some("GET".to_string()),
        path: Some("/".to_string()),
        headers: vec![],
        body: None,
        body_readers: None,
        credential_sentinels: vec![],
    }
}

fn mediated<'a>(
    store: &'a mut Store,
    run: &str,
    lease: &'a Lease,
    broker: &'a mut CredentialBroker,
    tokens: hh_env::tokens::TokenResolver,
    policy: ContainmentPolicy,
    transport: CaptureTransport,
) -> EgressMediator<'a> {
    EgressMediator {
        store,
        run_id: run.to_string(),
        lease,
        tokens,
        broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(StaticResolver {
            map: BTreeMap::from([(
                "svc.example".to_string(),
                vec!["93.184.216.34".parse().unwrap()],
            )]),
        }),
        transport: Box::new(transport),
        inspect_hooks: std::collections::BTreeMap::new(),
    }
}

fn human_endorser() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("human:op", HumanRole::Principal),
        PersistenceScope::Run,
        1_500,
    )
}

fn endorse_response(
    permission_id: &str,
    choice: ResponseChoice,
    decided_at: u64,
) -> ApprovalResponse {
    ApprovalResponse {
        permission_id: permission_id.to_string(),
        choice,
        scope: DecisionScope::Session,
        max_uses: None,
        justification: None,
        decided_by: EndorserRef::Human {
            subject_ref: "human:op".into(),
            authority: AuthorityClass::Principal,
        },
        decided_at,
    }
}

// ── 1. `resources → BudgetNode` (DF-S1.12-2) ─────────────────────────────────

/// The mapping: every `ResourceLimits` member lands on its registered
/// dimension as a hard ceiling — `hh.env.*` for the env-capacity set (the
/// R2.9a registrations), the kernel metered dims for wall/network.
#[test]
fn r29_resource_budget_spec_maps_every_bound() {
    let limits = ResourceLimits {
        cpu_ms: Some(10),
        memory_bytes: Some(20),
        disk_bytes: Some(30),
        pids: Some(40),
        open_files: Some(50),
        wall_ms: Some(60),
        network_bytes_out: Some(70),
        network_calls: Some(80),
    };
    let spec = resource_budget_spec(&limits).expect("bounds present");
    let hard = |d: DimensionId| spec.hard(DimensionKey::Primary(d));
    assert_eq!(hard(DimensionId::ExtEnvCpuMs), Some(10));
    assert_eq!(hard(DimensionId::ExtEnvMemoryBytes), Some(20));
    assert_eq!(hard(DimensionId::ExtEnvDiskBytes), Some(30));
    assert_eq!(hard(DimensionId::ExtEnvPids), Some(40));
    assert_eq!(hard(DimensionId::ExtEnvOpenFiles), Some(50));
    assert_eq!(hard(DimensionId::TimeWallMs), Some(60));
    assert_eq!(hard(DimensionId::NetworkBytesOut), Some(70));
    assert_eq!(hard(DimensionId::NetworkCalls), Some(80));
    assert_eq!(spec.dimensions.len(), 8);
    assert_eq!(spec.mode, BudgetMode::Pool);
    // No bounds ⇒ nothing to allocate (never an empty node).
    assert!(resource_budget_spec(&ResourceLimits::default()).is_none());
}

/// DF-S1.12-2: provision allocates the env's resource budget as a **pool
/// child under the run's root** and lands the id on
/// `handle.budget_node_refs` — the `control.budget.allocated` row records
/// the parent edge and the per-dimension hard ceilings.
#[test]
fn r29_provision_allocates_env_budget_pool_child() {
    let (mut store, run, lease, _clock) = open("budget-child");
    let root = root_budget(&mut store, &run, &lease, 10_000);
    let ws = workspace("budget-child");
    let mut driver = EnvDriver::new(&run);
    let limits = ResourceLimits {
        cpu_ms: Some(500),
        memory_bytes: Some(1 << 20),
        ..Default::default()
    };
    let h = driver
        .provision(
            &mut store,
            &lease,
            &test_record(limits),
            roots_for(&ws),
            PolicySlot::Inline(Box::new(test_policy(&ws))),
            OnLoss::FailRun,
            None,
        )
        .unwrap();
    assert_eq!(
        h.budget_node_refs.len(),
        1,
        "the env's resource node lands on the handle"
    );
    let child = &h.budget_node_refs[0];
    let rows = read_all(&store, &run);
    let alloc = rows
        .iter()
        .find(|e| {
            e.class == "control.budget.allocated"
                && jstr(&e.payload, "budget_id").as_deref() == Some(child.as_str())
        })
        .expect("the child allocation is durable");
    // Pool child under the run root — never a second root (CC9).
    assert_eq!(
        jstr(&alloc.payload, "parent").as_deref(),
        Some(root.as_str()),
        "the env node is subordinate to the run's root"
    );
    // The live handle copy carries the same ref.
    let live = driver.handle(&h.env_handle_id).unwrap();
    assert_eq!(live.budget_node_refs, h.budget_node_refs);
}

/// A run with **no** allocated root cannot honour declared bounds —
/// provision refuses typed (`BudgetRefused`), never provisions an
/// unbudgeted bound set.
#[test]
fn r29_provision_with_bounds_and_no_root_refuses() {
    let (mut store, run, lease, _clock) = open("budget-noroot");
    let ws = workspace("budget-noroot");
    let mut driver = EnvDriver::new(&run);
    let limits = ResourceLimits {
        cpu_ms: Some(500),
        ..Default::default()
    };
    let err = driver
        .provision(
            &mut store,
            &lease,
            &test_record(limits),
            roots_for(&ws),
            PolicySlot::Inline(Box::new(test_policy(&ws))),
            OnLoss::FailRun,
            None,
        )
        .unwrap_err();
    assert!(
        matches!(err, EnvError::BudgetRefused { .. }),
        "no root ⇒ typed refusal, got {err:?}"
    );
    assert!(
        driver.handle_ids().is_empty(),
        "a refused budget allocation installs no handle"
    );
}

/// CC9: a child spec wider than the parent's remaining refuses — the env
/// can never widen the run's envelope.
#[test]
fn r29_env_budget_child_cannot_widen_past_parent() {
    let (mut store, run, lease, _clock) = open("budget-widen");
    let _root = root_budget(&mut store, &run, &lease, 100);
    let ws = workspace("budget-widen");
    let mut driver = EnvDriver::new(&run);
    let limits = ResourceLimits {
        cpu_ms: Some(500), // exceeds the root's remaining 100
        ..Default::default()
    };
    let err = driver
        .provision(
            &mut store,
            &lease,
            &test_record(limits),
            roots_for(&ws),
            PolicySlot::Inline(Box::new(test_policy(&ws))),
            OnLoss::FailRun,
            None,
        )
        .unwrap_err();
    assert!(
        matches!(err, EnvError::BudgetRefused { .. }),
        "a widening child refuses typed: {err:?}"
    );
}

// ── 2. `amend` → handle write-back (DF-S1.12-2 + DF-S2.4-1b) ─────────────────

/// `apply_amended_containment` installs the amended policy on the live
/// handle — the next `decide_egress` reads the new `version_id` (CC3: the
/// durable `security.containment.amended` and the handle never diverge).
#[test]
fn r29_apply_amended_containment_swaps_handle_policy() {
    let (mut store, run, lease, _clock) = open("amended-handle");
    let ws = workspace("amended-handle");
    let mut driver = EnvDriver::new(&run);
    let policy = ask_policy(&ws);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        ResourceLimits::default(),
        policy.clone(),
    );
    let outcome = amend(
        &policy,
        &ContainmentDiff::AddEgressAllow {
            host: "svc.example".to_string(),
        },
        AmendmentBasis::Approval,
        &human_endorser(),
        PersistenceScope::Run,
    )
    .unwrap();
    assert_ne!(outcome.to_version_id, outcome.from_version_id);
    driver
        .apply_amended_containment(&env, outcome.policy.clone())
        .unwrap();
    let live = driver.handle(&env).unwrap();
    assert_eq!(
        live.containment.policy().version_id,
        outcome.to_version_id,
        "the handle's live policy is the amended version"
    );
    assert!(live
        .containment
        .policy()
        .net
        .rules
        .iter()
        .any(|r| r.decision == RuleDecision::Allow
            && r.host == HostPattern::parse("svc.example").unwrap()));
}

// ── 3. `endorse_gate` — gate-stage endorsement (DF-S2.4-1b) ──────────────────

/// `endorse_gate(allow_lease)` decides at the **gate**: the amended row is
/// durable, the `GateAllow` carries `decided_by = monitor` + the amended
/// policy, and the wire never fired — `forward` remains the wire point.
#[test]
fn r29_endorse_gate_lease_amends_and_defers_wire() {
    let (mut store, run, lease, _clock) = open("endorse-gate");
    let eid = open_egress_scopes(&mut store, &run, &lease);
    let ws = workspace("endorse-gate");
    let transport = CaptureTransport::default();
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "k");
    let mut tm = TokenMinter::new([7u8; 32]);
    let tok = tm.mint(&eid, 1, "env-1");
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        ask_policy(&ws),
        transport.clone(),
    );
    let r = egress_req(&tok.token, &eid, "env-1", "svc.example");
    let out = med.gate(&r, &chain()).unwrap();
    let GateOutcome::Terminal(MediatedOutcome::Asked { permission_id, .. }) = out else {
        panic!("expected the ask, got {out:?}")
    };

    // The surface's answer arrives at the *gate* — `endorse_gate`, never a
    // second `requested`/ask trail.
    let decided_at = med.store.now_ms();
    let response = endorse_response(
        &permission_id,
        ResponseChoice::AllowLease(LeaseSpec {
            pattern: None,
            scope: PersistenceScope::Run,
            max_uses: None,
        }),
        decided_at,
    );
    let pre_version = med.policy.version_id.clone();
    let GateOutcome::Allowed(g) = med
        .endorse_gate(&r, &response, &human_endorser(), &chain())
        .unwrap()
    else {
        panic!("allow_lease endorsement allows at the gate")
    };
    assert_eq!(g.decided_by, DecidedBy::Monitor);
    let amended = g
        .amended_policy
        .as_ref()
        .expect("allow_lease carries the amended policy for the handle write-back");
    assert_ne!(amended.version_id, pre_version);
    // The mediator's live policy is already the amended version — the
    // handle write-back keeps them equal (CC3).
    assert_eq!(med.policy.version_id, amended.version_id);
    // The amendment is durable BEFORE any wire; the transport never ran.
    assert_eq!(
        class_count(med.store, &run, "security.containment.amended"),
        1
    );
    assert!(transport.log.lock().unwrap().is_empty());
    // No second `requested` — the endorsement re-enters at the gate.
    assert_eq!(class_count(med.store, &run, "security.egress.requested"), 1);

    // The wire leg still runs through `forward` under monitor attribution.
    let out = med
        .forward(
            &r,
            &g.decision,
            g.decided_by,
            &g.effect_id,
            g.started,
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }));
    let monitor_allows = read_all(med.store, &run)
        .iter()
        .filter(|e| {
            e.class == "security.egress.decided"
                && jstr(&e.payload, "decision").as_deref() == Some("allow")
                && jstr(&e.payload, "decided_by").as_deref() == Some("monitor")
        })
        .count();
    assert_eq!(monitor_allows, 1, "decided{{allow, monitor}} is durable");
    assert_eq!(transport.log.lock().unwrap().len(), 1);
}

/// `endorse_gate(deny)` terminates at the gate — `decided{deny, monitor}`
/// durable, no wire, no amendment.
#[test]
fn r29_endorse_gate_deny_terminates() {
    let (mut store, run, lease, _clock) = open("endorse-deny");
    let eid = open_egress_scopes(&mut store, &run, &lease);
    let ws = workspace("endorse-deny");
    let transport = CaptureTransport::default();
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "k");
    let mut tm = TokenMinter::new([8u8; 32]);
    let tok = tm.mint(&eid, 1, "env-1");
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        ask_policy(&ws),
        transport.clone(),
    );
    let r = egress_req(&tok.token, &eid, "env-1", "svc.example");
    let out = med.gate(&r, &chain()).unwrap();
    let GateOutcome::Terminal(MediatedOutcome::Asked { permission_id, .. }) = out else {
        panic!("expected the ask, got {out:?}")
    };
    let decided_at = med.store.now_ms();
    let response = endorse_response(
        &permission_id,
        ResponseChoice::Deny {
            reason: "no".into(),
        },
        decided_at,
    );
    let out = med
        .endorse_gate(&r, &response, &human_endorser(), &chain())
        .unwrap();
    assert!(matches!(
        out,
        GateOutcome::Terminal(MediatedOutcome::Refused { .. })
    ));
    let denies = read_all(med.store, &run)
        .iter()
        .filter(|e| {
            e.class == "security.egress.decided"
                && jstr(&e.payload, "decision").as_deref() == Some("deny")
                && jstr(&e.payload, "decided_by").as_deref() == Some("monitor")
        })
        .count();
    assert_eq!(denies, 1, "decided{{deny, monitor}} is durable");
    assert_eq!(
        class_count(med.store, &run, "security.containment.amended"),
        0
    );
    assert!(transport.log.lock().unwrap().is_empty());
}

// ── 4. the dispatch resume leg (DF-S2.4-1b) ──────────────────────────────────

fn reversible_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Open,
        reversibility: Reversibility::Compensable,
    }
}

fn egress_capability() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [("url", "string")]
                    .iter()
                    .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                    .collect(),
            ),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::NetEgress,
                attributes: Some(reversible_attrs()),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![
                Json::str("executor_error"),
                Json::str("timeout"),
                Json::str("signalled"),
            ]),
        )]),
        cost_model: None,
        execution_requirement: Json::Null,
        source: Json::Null,
        exposure_hint: Json::Null,
        postconditions: vec![],
        flow_contract: None,
        action_patterns: Vec::new(),
    }
}

fn egress_binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "fetch".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:fetch".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:fetch".to_string(),
        arg_map: [(
            "url".to_string(),
            ArgMapEntry {
                capability_param: "url".to_string(),
                transform: ArgTransform::Identity,
                narrowing: None,
            },
        )]
        .into_iter()
        .collect(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: CompileExposureMode::Primitive,
        capability_refs: vec!["test:fetch".to_string()],
        mapping: BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    }
}

fn net_monitor(cap: &ToolCapabilityRecord, bind: &SurfaceBinding) -> Monitor {
    let mut table = HandleTable::default();
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/test", 0),
        grants: vec![Grant {
            effect: EffectClass::domain_only(EffectDomain::NetEgress),
            scope: "*".to_string(),
            constraints: GrantConstraints::default(),
            delegable: true,
        }],
        ceiling: AuthorityClass::Definition,
        validity: HandleValidity {
            issued_at: "evt-mint".into(),
            expires_at: Some(HandleExpiry::Run("run-1".into())),
            revoked_by: None,
        },
        parent_handle: None,
        delegable: true,
        origin_basis: OriginBasis::Seal,
        basis_ref: "test:agent#sha256:def".into(),
        budget_ref: None,
        scope: DecisionScope::Session,
    };
    table.handles.insert(h.handle_id.clone(), h);
    let mut m = Monitor::new(table, default_table("pol-v1", Mode::Attended));
    m.proposers.insert(
        "test:agent".to_string(),
        Label::at(AuthorityClass::Principal),
    );
    m.run_id = "run-1".to_string();
    m.turn_id = "turn-1".to_string();
    m.effect_id = "e1".to_string();
    m.capabilities.insert(
        "test:fetch".to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: bind.clone(),
        },
    );
    m
}

fn egress_input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env: &str,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: chain(),
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: "test:fetch".to_string(),
            version_id: "v-cap".to_string(),
        },
        surface_args: Json::obj([("url", Json::str("https://svc.example/x"))]),
        args_provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp-1"),
            PersistenceScope::Run,
            0,
        ),
        context_label: Label::at(AuthorityClass::Principal),
        self_report: None,
        env_handle_id: env.to_string(),
        declared: EffectClass {
            domain: EffectDomain::NetEgress,
            attributes: Some(reversible_attrs()),
        },
        requested_grants: vec![],
        ladder: DeadlineLadder {
            run_deadline_ms: None,
            phase_deadline_ms: None,
            effect_deadline_ms: None,
            attempt_deadline_ms: None,
        },
        output_policy: OutputPolicy {
            retain_bytes_cap: 1 << 20,
            model_view: "tail".to_string(),
            offload_above: 1 << 20,
            max_deltas: 64,
        },
        reserve: None,
        compensation_plan_id: None,
        baseline_ref: None,
        flow: Default::default(),
    }
}

/// A no-op executor — a suspended/refused egress never reaches it.
struct NoopExecutor {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
}

impl NoopExecutor {
    fn new() -> Self {
        NoopExecutor {
            decl: ExecutorDeclaration {
                executor_id: "noop/1".to_string(),
                isolation_support: IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error".to_string()].into_iter().collect(),
                streams: false,
                domains: [EffectDomain::NetEgress].into_iter().collect(),
            },
            calls: std::cell::Cell::new(0),
        }
    }
}

impl ToolExecutor for NoopExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        _request: &ExecutionRequest,
        _sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        self.calls.set(self.calls.get() + 1);
        Ok(TerminalReport {
            status: TerminalStatus::Ok,
            exit_status: Some(0),
            outcome_hint: "applied".to_string(),
            retryable_hint: None,
            detail_ref: None,
            truncated: false,
            omitted_bytes: 0,
            original_size: 0,
        })
    }
    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        Ok(ProbeVerdict::Undeterminable)
    }
}

/// DF-S2.4-1b — the resume leg end to end: `default_unmatched = ask`
/// suspends the effect behind the mediator's `pending`/`decided{ask}`
/// trail; a re-dispatch with the ask still open re-suspends **without**
/// minting a second `requested`/pending; and the surface's
/// `security.permission.decided{deny}` (the respond row) resolves the
/// pending through `endorse_gate` — `security.egress.decided{deny,
/// monitor}` is durable and the effect refuses.
#[test]
fn r29_dispatch_egress_ask_resume_endorses_at_gate() {
    let (mut store, run, lease, _clock) = open("egress-resume");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("egress-resume");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        ResourceLimits::default(),
        ask_policy(&ws),
    );
    let cap = egress_capability();
    let bind = egress_binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = net_monitor(&cap, &bind);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = NoopExecutor::new();
    let inp = egress_input(&cap, &bind, &sb, &env);

    // First dispatch — the mediator's ask suspends the effect.
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    let DispatchOutcome::Suspended { permission_id } = out else {
        panic!("expected Suspended, got {out:?}")
    };
    assert_eq!(
        class_count(disp.store_mut(), &run, "security.permission.pending"),
        1
    );
    assert_eq!(
        class_count(disp.store_mut(), &run, "security.egress.requested"),
        1
    );

    // Re-dispatch while unanswered — re-suspends, no second ask trail.
    let mon2 = net_monitor(&cap, &bind);
    let mut disp2 = Dispatcher::new(
        disp.store_mut(),
        &mon2,
        &run,
        [8u8; 32],
        DetectorSet::default(),
    );
    let out2 = disp2
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(out2, DispatchOutcome::Suspended { .. }),
        "unanswered ask re-suspends: {out2:?}"
    );
    assert_eq!(
        class_count(disp2.store_mut(), &run, "security.permission.pending"),
        1,
        "no second pending — the ask trail is minted once"
    );
    assert_eq!(
        class_count(disp2.store_mut(), &run, "security.egress.requested"),
        1,
        "no second `requested` on re-entry"
    );

    // The surface answers `deny` — the respond row names the pending's
    // `permission_id`, never the effect's gate slot (I-H7: the monitor's
    // decided{allow} already holds it).
    let decided_id = disp2.store_mut().alloc_id("evt");
    let decided = k_ev(
        disp2.store_mut(),
        &run,
        &decided_id,
        "security.permission.decided",
        Json::obj([
            ("permission_id", Json::str(permission_id.clone())),
            ("decision", Json::str("deny")),
            ("decider", Json::str("human")),
            ("decider_ref", Json::str("human:op")),
            ("decision_scope", Json::str("once")),
            ("reason", Json::str("no")),
            ("requested_at", Json::Int(1_000)),
            ("wait_ms", Json::Int(500)),
            (
                "responder_provenance",
                Json::obj([("subject_ref", Json::str("human:op"))]),
            ),
        ]),
    );
    disp2
        .store_mut()
        .append(&run, &lease, vec![decided])
        .unwrap();

    // Resume — the decided row reaches `endorse_gate`: `decided{deny,
    // monitor}` lands and the effect refuses.
    let mon3 = net_monitor(&cap, &bind);
    let mut disp3 = Dispatcher::new(
        disp2.store_mut(),
        &mon3,
        &run,
        [9u8; 32],
        DetectorSet::default(),
    );
    let out3 = disp3
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(&out3, DispatchOutcome::Refused { reason } if reason.starts_with("egress_denied:")),
        "the endorsed deny refuses at the gate: {out3:?}"
    );
    let envs = read_all(disp3.store_mut(), &run);
    assert!(
        envs.iter().any(|e| e.class == "security.egress.decided"
            && jstr(&e.payload, "decision").as_deref() == Some("deny")
            && jstr(&e.payload, "decided_by").as_deref() == Some("monitor")),
        "decided{{deny, monitor}} — the endorsement's durable verdict"
    );
    assert!(
        envs.iter().any(|e| e.class == "action.effect.refused"),
        "the refused terminal lands"
    );
    assert_eq!(exec.calls.get(), 0, "a denied egress never executes");
}

// ── 5. fork credential custody (DF-S2.4-1c; LT-09) ───────────────────────────

fn channel_spec(host: &str, port: u16, env_name: &str) -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::Bearer,
        source: SecretSource::OperatorVault {
            vault_ref: format!("vault:{host}"),
        },
        destinations: vec![DestinationBinding {
            scheme: "http".into(),
            host_pattern: host.into(),
            port: Some(port),
            path_prefix: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
            revocation_path: None,
        }],
        allowed_env_names: Some(BTreeSet::from([env_name.to_string()])),
        delivery_modes: BTreeSet::from([SecretTransport::ProxyInjected]),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: GrantConstraints::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "test channel".into(),
    }
}

/// The `security.permission.granted` + `decided{allow}` rows a bind needs
/// (the S2.4 battery's `grant_handle`/`decide_allow` shape).
fn grant_and_decide(store: &mut Store, run: &str, lease: &Lease, channel_id: &str) -> String {
    let handle_id = store.alloc_id("hnd");
    let issuer = ProvenanceRecord::kernel("kernel:test", store.now_ms());
    let granted = k_ev(
        store,
        run,
        &handle_id,
        "security.permission.granted",
        Json::obj([
            ("handle_id", Json::str(handle_id.clone())),
            (
                "permission_ref",
                Json::obj([
                    ("semantic_id", Json::str("perm.secrets")),
                    (
                        "version_id",
                        Json::str("sha256:".to_string() + &"0".repeat(64)),
                    ),
                ]),
            ),
            ("holder", Json::str("agent.main")),
            ("issuer", issuer.to_json()),
            (
                "grants",
                Json::Arr(vec![Json::obj([
                    (
                        "effect",
                        Json::obj([("domain", Json::str("secret_access"))]),
                    ),
                    ("scope", Json::str(SecretChannel::grant_scope(channel_id))),
                    ("constraints", Json::obj([])),
                    ("delegable", Json::Bool(false)),
                ])]),
            ),
            ("ceiling", Json::str("principal")),
            (
                "validity",
                Json::obj([
                    ("issued_at", Json::str(store.ts_now())),
                    ("expires_at", Json::Null),
                ]),
            ),
            ("parent_handle", Json::Null),
            ("delegable", Json::Bool(false)),
            ("origin_basis", Json::str("approval")),
            ("basis_ref", Json::str("perm.secrets")),
            ("budget_ref", Json::Null),
            ("authority_delta", Json::str("none")),
        ]),
    );
    let decided_id = store.alloc_id("evt");
    let decided = k_ev(
        store,
        run,
        &decided_id,
        "security.permission.decided",
        Json::obj([
            ("effect_id", Json::str(format!("eff-{handle_id}"))),
            ("decision", Json::str("allow")),
            ("effective_authority", Json::str("principal")),
            (
                "effective_risk_class",
                Json::obj([
                    ("reversibility", Json::str("reversible")),
                    ("repeat_safety", Json::str("idempotent")),
                    ("scope", Json::str("ephemeral")),
                ]),
            ),
            ("handle_ids", Json::Arr(vec![Json::str(handle_id.clone())])),
            ("policy_ref", Json::str("pi/1")),
            ("decider", Json::str("policy")),
            ("attempt_no", Json::Int(1)),
            ("proposal", Json::str("prop-1")),
            ("taint", Json::Arr(vec![])),
        ]),
    );
    store.append(run, lease, vec![granted, decided]).unwrap();
    decided_id
}

/// DF-S2.4-1c / LT-09 — cross-run fork custody: the parent's live bindings
/// rebind onto the child with **fresh** placeholder spellings; the
/// `security.credential.bound` rows land on the *child's* run (the driver
/// never consults its own handle table for the parent — the caller passes
/// the record).
#[test]
fn r29_rebind_credentials_for_fork_cross_run() {
    let (mut store, run, lease, _clock) = open("fork-cred");
    let ws = workspace("fork-cred-parent");
    let mut driver = EnvDriver::new(&run);
    let parent_env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        ResourceLimits::default(),
        test_policy(&ws),
    );
    std::fs::write(ws.join("pinned.txt"), b"at-cut").unwrap();
    let (_rec, snap) = driver
        .fs_tree_snapshot(&mut store, &lease, &parent_env)
        .unwrap();

    // A live binding on the parent (the service-level broker holds it).
    let mut vault = StaticVault::default();
    vault.put(
        &SecretSource::OperatorVault {
            vault_ref: "vault:127.0.0.1".to_string(),
        },
        "tok-fork",
    );
    let mut broker = CredentialBroker::new(Box::new(vault), "test-fp-key");
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", 8443, "GH_TOKEN"),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let dref = grant_and_decide(&mut store, &run, &lease, "github");
    let binding = broker
        .bind(
            &mut store,
            &run,
            &lease,
            BindRequest {
                channel_id: "github".into(),
                holder: "agent.main".into(),
                env_handle_ref: parent_env.clone(),
                env_isolation: IsolationClass::ProcessSandbox,
                mode: SecretTransport::ProxyInjected,
                monitor_decision_ref: dref,
                destinations: BTreeSet::from(["127.0.0.1".to_string()]),
            },
        )
        .unwrap();
    let parent_spelling = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    // The child run + driver — the parent handle never enters `cdrv`'s
    // table; custody passes the record.
    let (child_run, child_lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let mut cdrv = EnvDriver::new(&child_run);
    let parent = driver.handle(&parent_env).unwrap().clone();
    let ch = cdrv
        .derive_from_snapshot(&mut store, &child_lease, &parent, &snap)
        .unwrap();
    let virt = cdrv
        .rebind_credentials_for_fork(
            &mut store,
            &child_lease,
            &mut broker,
            &parent,
            &ch.env_handle_id,
        )
        .unwrap();
    assert_eq!(virt.bindings.len(), 1);
    // Fresh spelling — the parent's placeholder never crosses (SV-10).
    let child_spelling = virt.rewrites.get(&parent_spelling).unwrap().clone();
    assert_ne!(child_spelling, parent_spelling);
    // The child's handle carries the new binding id.
    let child_h = cdrv.handle(&ch.env_handle_id).unwrap();
    assert_eq!(child_h.credential_bindings, virt.bindings);
    // The bound row is durable on the *child* run.
    let bound_rows = read_all(&store, &child_run)
        .iter()
        .filter(|e| e.class == "security.credential.bound")
        .count();
    assert_eq!(bound_rows, 1, "the child's bound row is durable");
    // The parent's row count is untouched — custody never shares a
    // credential coordinate across runs.
    assert_eq!(
        read_all(&store, &run)
            .iter()
            .filter(|e| e.class == "security.credential.bound")
            .count(),
        1,
        "the parent's original bound row; no duplicate"
    );
}

/// The parent edge check binds the rebind to a real derive child — a
/// handle that was never derived from `parent` refuses typed.
#[test]
fn r29_rebind_rejects_non_derive_parent() {
    let (mut store, run, lease, _clock) = open("fork-badparent");
    let ws_a = workspace("fork-bp-a");
    let ws_b = workspace("fork-bp-b");
    let mut driver = EnvDriver::new(&run);
    let parent_env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws_a,
        ResourceLimits::default(),
        test_policy(&ws_a),
    );
    let other_env = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws_b,
        ResourceLimits::default(),
        test_policy(&ws_b),
    );
    let parent = driver.handle(&parent_env).unwrap().clone();
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "k");
    let err = driver
        .rebind_credentials_for_fork(&mut store, &lease, &mut broker, &parent, &other_env)
        .unwrap_err();
    assert!(
        matches!(
            err,
            EnvError::InvalidState {
                op: "rebind_credentials_for_fork",
                ..
            }
        ),
        "a non-derive child refuses typed: {err:?}"
    );
}
