//! R5.8 (PADR-R5-2 `DurableEffectRef`) — the Dispatcher durability bridge:
//! an already-intended coordinate (`action.effect.intended` minted by the
//! task runtime under the run's fenced writer) traverses the Dispatcher
//! pipeline exactly once — authorize → prepare → commit → execute →
//! capture → observe — and returns a `DurableEffectRef` whose members
//! resolve to the durable records instead of the driver re-appending
//! lifecycle rows.
//!
//! AC-1 positive legs and the AC-2 typed refusals (`adopt_writer_scope`,
//! `adopt_writer_stale`, `adopt_effect_scope`, `adopt_duplicate_lifecycle`,
//! `adopt_args_mismatch`, pre-prepare deny, missing execution capability,
//! executor-binding insufficiency — the tier-off shape) refuse before any
//! append.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ExecPolicy, NetMode, WritableRoot};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutionRequirement, ExecutorDeclaration, ExecutorSignal,
    InterruptSupport, ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_identity::idp::address;
use hharness_ledger::effect::EffectPhase;
use hharness_ledger::event::{Cursor, Direction, Scope};
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::decision::DecisionScope;
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hharness_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;

// ── scaffold (the acceptance battery's shape — scratch stores only) ──────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r58-{}-{tag}-{n}", std::process::id()));
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

/// Open `turn ⊃ mc ⊃ tc` scopes plus the task-runtime's
/// `action.effect.intended` for the *external* coordinate `ef` — the
/// driver-side shape (`turn`+`effect` scope members; no `ordinal`, so the
/// coordinate is a free `ef`, not the dispatcher's derived `f(...)`).
fn open_adopted_scopes(store: &mut Store, run: &str, lease: &Lease, ef: &str) {
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
    let mut i = m
        .mint(
            "action.effect.intended",
            Json::obj([(
                "effective_risk_class",
                RiskClass {
                    reversibility: RiskReversibility::Reversible,
                    repeat_safety: RiskRepeatSafety::Idempotent,
                    scope: RiskScope::WorkspaceLocal,
                }
                .to_json(),
            )]),
        )
        .unwrap();
    i.scope = Scope {
        turn_id: Some("turn-1".into()),
        effect_id: Some(ef.to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![t, c, p, i]).unwrap();
}

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_policy(ws: &std::path::Path) -> hharness_containment::policy::ContainmentPolicy {
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

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    let record = EnvironmentRecord {
        class: EnvironmentClass::LocalHost,
        image: ImageRef::ContentAddress(address(b"r5.8-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: Default::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    let roots = Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    };
    let h = driver
        .provision(
            store,
            lease,
            &record,
            roots,
            PolicySlot::Inline(Box::new(test_policy(ws))),
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

fn reversible_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
    }
}

fn capability(domain: EffectDomain, a: EffectAttributes) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [("path", "string"), ("content", "string")]
                    .iter()
                    .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                    .collect(),
            ),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain,
                attributes: Some(a),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
            ("param_path", Json::str("path")),
            ("scope_kind", Json::str("fs_path")),
        ])])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![Json::str("executor_error"), Json::str("timeout")]),
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

fn binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "write_file".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:write_file".to_string(),
        arg_map: ["path", "content"]
            .iter()
            .map(|a| {
                (
                    a.to_string(),
                    ArgMapEntry {
                        capability_param: a.to_string(),
                        transform: ArgTransform::Identity,
                        narrowing: None,
                    },
                )
            })
            .collect(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:write_file".to_string()],
        mapping: hharness_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [hharness_hir::tools::ExposureMode::Direct]
            .into_iter()
            .collect(),
        pinned: false,
        hidden: false,
    }
}

/// A root handle granting `fs_write` over `*` — the allow leg's Π floor.
fn allow_handle(domains: &[EffectDomain]) -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/test", 0),
        grants: domains
            .iter()
            .map(|d| Grant {
                effect: EffectClass::domain_only(*d),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            })
            .collect(),
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
    }
}

fn test_monitor(cap: &ToolCapabilityRecord, domains: &[EffectDomain]) -> Monitor {
    let mut table = HandleTable::default();
    let h = allow_handle(domains);
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
        "test:write_file".to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: binding(),
        },
    );
    m
}

fn input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env_handle_id: &str,
    args: Json,
    declared: EffectClass,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: ScopeChain {
            turn_id: "turn-1".to_string(),
            model_call_id: "mc-1".to_string(),
            tool_call_id: "tc-1".to_string(),
        },
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        surface_args: args,
        args_provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp-1"),
            PersistenceScope::Run,
            0,
        ),
        context_label: Label::at(AuthorityClass::Principal),
        self_report: None,
        env_handle_id: env_handle_id.to_string(),
        declared,
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
        remedy_taken: None,
        result_renderer: None,
        submission_marker: None,
    }
}

/// A scripted executor — writes nothing outside the workspace, counts calls.
struct MockExecutor {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
}

impl MockExecutor {
    fn new(domains: &[EffectDomain]) -> Self {
        MockExecutor {
            decl: ExecutorDeclaration {
                executor_id: "mock/1".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error".to_string()].into_iter().collect(),
                streams: false,
                domains: domains.iter().copied().collect(),
            },
            calls: std::cell::Cell::new(0),
        }
    }
}

impl ToolExecutor for MockExecutor {
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

// ── assertions ───────────────────────────────────────────────────────────────

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
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

fn effect_scoped(store: &Store, run: &str, ef: &str, class: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class && e.scope.effect_id.as_deref() == Some(ef))
        .count()
}

fn declared(domain: EffectDomain) -> EffectClass {
    EffectClass {
        domain,
        attributes: Some(reversible_attrs()),
    }
}

// ── AC-1: one intended coordinate, one traversal, validated durable refs ─────

#[test]
fn r58_dispatch_adopted_settles_the_intended_coordinate_once() {
    let (mut store, run, lease, _clock) = open("adopted");
    let ef = "ef-1";
    open_adopted_scopes(&mut store, &run, &lease, ef);
    let ws = workspace("adopted");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let path = format!("{}/a.txt", ws.display());
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(&path)), ("content", Json::str("hi"))]),
        declared(EffectDomain::FsWrite),
    );

    let (out, dref) = disp
        .dispatch_adopted(ef, &mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    // Exactly one traversal under `ef`: intended → authorized → prepared →
    // committed → observed — each row exactly once, all under the adopted
    // coordinate.
    for class in [
        "action.effect.intended",
        "action.effect.authorized",
        "action.effect.prepared",
        "action.effect.committed",
        "action.effect.observed",
    ] {
        assert_eq!(
            effect_scoped(disp.store_mut(), &run, ef, class),
            1,
            "{class} must land exactly once under {ef}"
        );
    }
    assert_eq!(exec.calls.get(), 1, "the executor ran once");

    // The `DurableEffectRef` members resolve to the durable records.
    assert_eq!(dref.effect_id, ef);
    assert_eq!(
        dref.writer_lease_ref,
        format!("{}@{}", lease.lease_id, lease.generation)
    );
    let events = read_all(disp.store_mut(), &run);
    let coord = events
        .iter()
        .find(|e| e.event_id == dref.coordinate_ref)
        .expect("coordinate_ref resolves");
    assert_eq!(coord.class, "action.effect.intended");
    assert_eq!(coord.scope.effect_id.as_deref(), Some(ef));
    let settle = events
        .iter()
        .find(|e| e.event_id == dref.settlement_ref)
        .expect("settlement_ref resolves");
    assert_eq!(settle.class, "action.effect.observed");
    assert_eq!(settle.scope.effect_id.as_deref(), Some(ef));

    // The fold shows the coordinate terminal — a second adopt is a
    // duplicate-lifecycle refusal, not a second traversal.
    let folds = disp.store_mut().effect_folds(&run).unwrap();
    let f = folds
        .iter()
        .find(|(id, _)| id == ef)
        .map(|(_, f)| f)
        .unwrap();
    assert_eq!(f.phase, EffectPhase::Observed);

    let mon2 = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp2 = Dispatcher::new(
        disp.store_mut(),
        &mon2,
        &run,
        [8u8; 32],
        DetectorSet::default(),
    );
    let err = disp2
        .dispatch_adopted(ef, &mut driver, &mut exec, None, &inp, &lease)
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_duplicate_lifecycle")),
        "{err:?}"
    );
    // Still exactly one lifecycle — no re-append on the refused leg.
    for class in [
        "action.effect.authorized",
        "action.effect.prepared",
        "action.effect.committed",
        "action.effect.observed",
    ] {
        assert_eq!(effect_scoped(disp2.store_mut(), &run, ef, class), 1);
    }
}

// ── AC-2: typed refusals before any append ───────────────────────────────────

/// Wrong writer scope — a lease scoped to another run refuses.
#[test]
fn r58_adopted_refuses_wrong_writer_scope() {
    let (mut store, run, lease, _clock) = open("wrong-writer");
    let (_run2, lease2) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-b")
        .unwrap();
    open_adopted_scopes(&mut store, &run, &lease, "ef-1");
    let ws = workspace("wrong-writer");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let rows_before = read_all(disp.store_mut(), &run).len();
    let err = disp
        .dispatch_adopted("ef-1", &mut driver, &mut exec, None, &inp, &lease2)
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_writer_scope")),
        "{err:?}"
    );
    assert_eq!(
        read_all(disp.store_mut(), &run).len(),
        rows_before,
        "refusal lands no rows"
    );
    assert_eq!(exec.calls.get(), 0);
}

/// A superseded or expired writer refuses — stale generation and a lapsed
/// expiry both fail before any append.
#[test]
fn r58_adopted_refuses_stale_writer() {
    let (mut store, run, lease, _clock) = open("stale-writer");
    open_adopted_scopes(&mut store, &run, &lease, "ef-1");
    let ws = workspace("stale-writer");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let rows_before = read_all(disp.store_mut(), &run).len();
    let superseded = Lease {
        run_id: run.clone(),
        lease_id: lease.lease_id.clone(),
        holder: lease.holder.clone(),
        generation: lease.generation + 1,
        expires_at_ms: lease.expires_at_ms,
    };
    let err = disp
        .dispatch_adopted("ef-1", &mut driver, &mut exec, None, &inp, &superseded)
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_writer_stale")),
        "{err:?}"
    );
    let expired = Lease {
        expires_at_ms: 0,
        ..lease.clone()
    };
    let err = disp
        .dispatch_adopted("ef-1", &mut driver, &mut exec, None, &inp, &expired)
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_writer_stale")),
        "{err:?}"
    );
    assert_eq!(read_all(disp.store_mut(), &run).len(), rows_before);
    assert_eq!(exec.calls.get(), 0);
}

/// A coordinate that is not already-intended refuses — the bridge never
/// mints an unscoped lifecycle.
#[test]
fn r58_adopted_refuses_unintended_coordinate() {
    let (mut store, run, lease, _clock) = open("unintended");
    open_adopted_scopes(&mut store, &run, &lease, "ef-1");
    let ws = workspace("unintended");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let rows_before = read_all(disp.store_mut(), &run).len();
    let err = disp
        .dispatch_adopted(
            "ef-not-intended",
            &mut driver,
            &mut exec,
            None,
            &inp,
            &lease,
        )
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_effect_scope")),
        "{err:?}"
    );
    assert_eq!(read_all(disp.store_mut(), &run).len(), rows_before);
}

/// A coordinate whose recorded intent disagrees with the presented
/// dispatch refuses — `adopt_args_mismatch` before any append.
#[test]
fn r58_adopted_refuses_args_mismatch() {
    let (mut store, run, lease, _clock) = open("args-mismatch");
    // Intended carrying a recorded `args_canonical_hash` that the
    // presented dispatch cannot match.
    let m = EventMinter::new(&mut store, &run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut i = m
        .mint(
            "action.effect.intended",
            Json::obj([
                (
                    "effective_risk_class",
                    RiskClass {
                        reversibility: RiskReversibility::Reversible,
                        repeat_safety: RiskRepeatSafety::Idempotent,
                        scope: RiskScope::WorkspaceLocal,
                    }
                    .to_json(),
                ),
                ("args_canonical_hash", Json::str("idp-recorded-other-args")),
            ]),
        )
        .unwrap();
    i.scope = Scope {
        turn_id: Some("turn-1".into()),
        effect_id: Some("ef-1".into()),
        ..Scope::default()
    };
    store.append(&run, &lease, vec![t, i]).unwrap();
    let ws = workspace("args-mismatch");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let rows_before = read_all(disp.store_mut(), &run).len();
    let err = disp
        .dispatch_adopted("ef-1", &mut driver, &mut exec, None, &inp, &lease)
        .unwrap_err();
    assert!(
        matches!(&err, EnvError::ResumeRefused { reason } if reason.starts_with("adopt_args_mismatch")),
        "{err:?}"
    );
    assert_eq!(read_all(disp.store_mut(), &run).len(), rows_before);
}

/// Pre-prepare deny — the monitor's `deny` settles `ef` as
/// `action.effect.refused` (the durable record), never an authorized or
/// prepared row.
#[test]
fn r58_adopted_monitor_deny_refuses_pre_prepare() {
    let (mut store, run, lease, _clock) = open("deny");
    let ef = "ef-1";
    open_adopted_scopes(&mut store, &run, &lease, ef);
    let ws = workspace("deny");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    // A monitor granting only `fs_read` — `fs_write` proposes `deny`.
    let mon = test_monitor(&cap, &[EffectDomain::FsRead]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let (out, dref) = disp
        .dispatch_adopted(ef, &mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Refused { .. }), "{out:?}");
    assert_eq!(
        effect_scoped(disp.store_mut(), &run, ef, "action.effect.authorized"),
        0
    );
    assert_eq!(
        effect_scoped(disp.store_mut(), &run, ef, "action.effect.prepared"),
        0
    );
    assert_eq!(
        effect_scoped(disp.store_mut(), &run, ef, "action.effect.committed"),
        0
    );
    assert_eq!(
        effect_scoped(disp.store_mut(), &run, ef, "action.effect.refused"),
        1
    );
    let events = read_all(disp.store_mut(), &run);
    let settle = events
        .iter()
        .find(|e| e.event_id == dref.settlement_ref)
        .expect("settlement ref resolves");
    assert_eq!(settle.class, "action.effect.refused");
    assert_eq!(exec.calls.get(), 0, "denied before the executor");
}

/// Missing execution capability — the executor-binding contract
/// (`executor::bind`, the arm-time and dispatch-time check the
/// `DispatchGate` runs) refuses a declaration that does not cover the
/// declared domain. A surface whose only executor cannot serve the effect
/// domain binds `UnsupportedDomain` — a typed refusal, never a silent run.
#[test]
fn r58_bind_refuses_missing_execution_capability() {
    let decl = ExecutorDeclaration {
        executor_id: "narrow/1".to_string(),
        isolation_support: hharness_containment::policy::IsolationClass::None,
        dedup_support: DedupSupport::BestEffort,
        probe_support: ProbeSupport::Check,
        interrupt: InterruptSupport::Supported,
        error_classes: BTreeSet::new(),
        streams: false,
        // The executor only declares `fs_read` — `fs_write` cannot bind.
        domains: [EffectDomain::FsRead].into_iter().collect(),
    };
    let err = hharness_env::executor::bind(
        &decl,
        EffectDomain::FsWrite,
        &ExecutionRequirement::default(),
    )
    .expect_err("an fs_read-only executor cannot serve fs_write");
    let spelling = format!("{err:?}");
    assert!(
        spelling.contains("UnsupportedDomain"),
        "expected UnsupportedDomain, got {spelling}"
    );
}

/// Tier-off / no `LocalExecutor` substitution — `executor::bind` refuses a
/// declaration below the capability's confinement requirement (a
/// `local_host` executor never stands in for an admitted isolation floor).
#[test]
fn r58_bind_refuses_insufficient_isolation() {
    let decl = ExecutorDeclaration {
        executor_id: "local/1".to_string(),
        isolation_support: hharness_containment::policy::IsolationClass::None,
        dedup_support: DedupSupport::BestEffort,
        probe_support: ProbeSupport::Check,
        interrupt: InterruptSupport::Supported,
        error_classes: BTreeSet::new(),
        streams: false,
        domains: [EffectDomain::FsWrite].into_iter().collect(),
    };
    let req = ExecutionRequirement {
        minimum_isolation: hharness_containment::policy::IsolationClass::Namespaces,
        ..ExecutionRequirement::default()
    };
    let err = hharness_env::executor::bind(&decl, EffectDomain::FsWrite, &req)
        .expect_err("a none-supporting executor cannot serve a container floor");
    let spelling = format!("{err:?}");
    assert!(
        spelling.contains("InsufficientIsolation"),
        "expected InsufficientIsolation, got {spelling}"
    );
}

/// A stale environment — the handle is gone before dispatch — refuses at
/// resolve before any lifecycle row lands.
#[test]
fn r58_adopted_refuses_stale_environment() {
    let (mut store, run, lease, _clock) = open("stale-env");
    let ef = "ef-1";
    open_adopted_scopes(&mut store, &run, &lease, ef);
    let ws = workspace("stale-env");
    let mut driver = EnvDriver::new(&run);
    let _env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs());
    let bind = binding();
    let sb = cap.scope_bindings.clone();
    let mon = test_monitor(&cap, &[EffectDomain::FsWrite]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::new(&[EffectDomain::FsWrite]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        "env-never-provisioned",
        Json::obj([("path", Json::str(format!("{}/x", ws.display())))]),
        declared(EffectDomain::FsWrite),
    );
    let rows_before = read_all(disp.store_mut(), &run).len();
    let res = disp.dispatch_adopted(ef, &mut driver, &mut exec, None, &inp, &lease);
    assert!(res.is_err(), "stale env must error, got {res:?}");
    // The resolve-stage refusal lands no effect rows.
    for class in [
        "action.effect.authorized",
        "action.effect.prepared",
        "action.effect.committed",
        "action.effect.observed",
    ] {
        assert_eq!(
            effect_scoped(disp.store_mut(), &run, ef, class),
            0,
            "{class}"
        );
    }
    assert_eq!(
        read_all(disp.store_mut(), &run).len(),
        rows_before,
        "resolve-stage refusal lands nothing"
    );
}
