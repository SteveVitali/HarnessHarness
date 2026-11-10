//! R5.8 (PADR-R5-2 `DurableEffectRef`) — the embed-injected bridge:
//! `DispatchGate` routes surfaces armed with real execution capability
//! through the `Dispatcher` under the session's fenced writer — one
//! authorization→settlement traversal whose durable rows land once — and
//! `arm_exec_capabilities` refuses an executor declaration below the
//! capability's requirement (the tier-off / no-`LocalExecutor`-
//! substitution leg). Unarmed surfaces and the host path fall through to
//! the byte-identical `EmbedGate` semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ExecPolicy, NetMode, WritableRoot};
use hharness_control::driver::{DispatchSeam, EffectGate};
use hharness_control::vocab::SettledOutcome;
use hharness_embed::runtime::{DispatchGate, EmbedGate, ExecCap};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::events::EventMinter;
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::local::LocalExecutor;
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
use hharness_provenance::{AuthorityClass, Label, ProvenanceRecord};
use hharness_wire::json::Json;

// ── scratch rig (the env acceptance battery's shape) ─────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r58-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

/// `turn ⊃ mc ⊃ tc` scopes plus the task-runtime's
/// `action.effect.intended` for the external coordinate `ef` — the
/// driver-side shape (`turn`+`effect` scope members).
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
    let mut p = m
        .mint(
            "action.tool.proposed",
            Json::obj([("tool_call_id", Json::str("tc-1"))]),
        )
        .unwrap();
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
                hharness_ontology::risk::RiskClass {
                    reversibility: hharness_ontology::risk::RiskReversibility::Reversible,
                    repeat_safety: hharness_ontology::risk::RepeatSafety::Idempotent,
                    scope: hharness_ontology::risk::RiskScope::WorkspaceLocal,
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

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
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
            PolicySlot::Inline(Box::new(p)),
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

fn write_cap() -> ToolCapabilityRecord {
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
                domain: EffectDomain::FsWrite,
                attributes: Some(reversible_attrs()),
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

fn write_binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "exec.write".to_string(),
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
        surface_id: "exec.write".to_string(),
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

fn allow_monitor(cap: &ToolCapabilityRecord) -> Monitor {
    let mut table = HandleTable::default();
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-embed/test", 0),
        grants: vec![Grant {
            effect: EffectClass::domain_only(EffectDomain::FsWrite),
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
        "test:write_file".to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: write_binding(),
        },
    );
    m
}

struct CountingExec {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
}

impl CountingExec {
    fn new() -> Self {
        CountingExec {
            decl: ExecutorDeclaration {
                executor_id: "counting/1".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error".to_string()].into_iter().collect(),
                streams: false,
                domains: [EffectDomain::FsWrite].into_iter().collect(),
            },
            calls: std::cell::Cell::new(0),
        }
    }
}

impl ToolExecutor for CountingExec {
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

fn exec_cap(exec: Box<dyn ToolExecutor>) -> ExecCap {
    ExecCap {
        surface_id: "exec.write".to_string(),
        binding: write_binding(),
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
            ("param_path", Json::str("path")),
            ("scope_kind", Json::str("fs_path")),
        ])])),
        capability: write_cap(),
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        declared: EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
        proposer: "test:agent".to_string(),
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
        executor: exec,
        monitor: allow_monitor(&write_cap()),
    }
}

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

fn effect_scoped(store: &Store, run: &str, ef: &str, class: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class && e.scope.effect_id.as_deref() == Some(ef))
        .count()
}

// ── AC-1: the armed surface traverses the real dispatcher once ───────────────

#[test]
fn r58_gate_settles_armed_surface_through_real_dispatcher() {
    let (mut store, run, lease) = open("gate-armed");
    let ef = "ef-1";
    open_adopted_scopes(&mut store, &run, &lease, ef);
    let ws = workspace("gate-armed");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let mut gate = DispatchGate {
        embed: EmbedGate {
            host_surfaces: BTreeMap::new(),
            host_asks: Vec::new(),
        },
        exec_caps: [(
            "exec.write".to_string(),
            exec_cap(Box::new(CountingExec::new())),
        )]
        .into_iter()
        .collect(),
        run_id: run.clone(),
        env_handle_id: Some(env),
        seed: [7u8; 32],
        result_renderers: BTreeMap::new(),
        submission_marker: None,
    };
    let intent = Json::obj([
        ("surface_id", Json::str("exec.write")),
        ("tool_call_id", Json::str("tc-1")),
        (
            "args",
            Json::obj([
                ("path", Json::str(format!("{}/a.txt", ws.display()))),
                ("content", Json::str("hi")),
            ]),
        ),
    ]);
    let seam = DispatchSeam {
        store: &mut store,
        lease: &lease,
        envs: Some(&mut driver),
    };
    let out = gate
        .dispatch_durable(seam, ef, 1, &intent)
        .expect("the armed surface answers the durable leg");
    assert!(
        matches!(out.outcome, SettledOutcome::Observed { .. }),
        "{out:?}"
    );
    let dref = out.durable_effect_ref.expect("the durable ref is cited");
    assert_eq!(dref.effect_id, ef);
    assert!(out.emitted.is_empty(), "the durable path never emits");
    // Exactly one traversal under `ef` — the dispatcher's own rows, never
    // a duplicated driver lifecycle.
    for class in [
        "action.effect.intended",
        "action.effect.authorized",
        "action.effect.prepared",
        "action.effect.committed",
        "action.effect.observed",
    ] {
        assert_eq!(effect_scoped(&store, &run, ef, class), 1, "{class}");
    }
    let events = read_all(&store, &run);
    assert_eq!(
        events
            .iter()
            .find(|e| e.event_id == dref.coordinate_ref)
            .map(|e| e.class.as_str()),
        Some("action.effect.intended")
    );
    assert_eq!(
        events
            .iter()
            .find(|e| e.event_id == dref.settlement_ref)
            .map(|e| e.class.as_str()),
        Some("action.effect.observed")
    );
}

// ── fallback: an unarmed surface keeps the host path ─────────────────────────

#[test]
fn r58_gate_unarmed_surface_falls_through_to_embed() {
    let mut gate = DispatchGate {
        embed: EmbedGate {
            host_surfaces: BTreeMap::new(),
            host_asks: Vec::new(),
        },
        exec_caps: BTreeMap::new(),
        run_id: "run-1".to_string(),
        env_handle_id: None,
        seed: [0u8; 32],
        result_renderers: BTreeMap::new(),
        submission_marker: None,
    };
    let intent = Json::obj([("surface_id", Json::str("exec.write"))]);
    let mut store_scope = open("gate-fallback");
    let seam = DispatchSeam {
        store: &mut store_scope.0,
        lease: &store_scope.2,
        envs: None,
    };
    assert!(
        gate.dispatch_durable(seam, "ef-1", 1, &intent).is_none(),
        "an unarmed surface never answers the durable leg"
    );
    // The `dispatch` fallback is the `EmbedGate` verbatim — an unbound
    // surface refuses the same way the host path does.
    let out = gate.dispatch("ef-1", 1, &intent);
    assert_eq!(out.outcome, SettledOutcome::Refused);
    assert_eq!(
        out.error_class.as_deref(),
        Some("surface_not_bound"),
        "{out:?}"
    );
}

// ── arm-time binding: insufficient executor declaration refuses ───────────────

#[test]
fn r58_arm_refuses_executor_below_capability_floor() {
    let root = dir("arm-refuse");
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.clone(),
        holder: "r5-8-arm".to_string(),
    })
    .unwrap();
    // A capability requiring namespaces cannot arm on `local_host`
    // (`isolation_support = none`) — the tier-off / no-substitution leg.
    let mut cap = exec_cap(Box::new(LocalExecutor::new(vec![])));
    cap.capability.execution_requirement =
        Json::obj([("minimum_isolation", Json::str("namespaces"))]);
    let err = svc
        .arm_exec_capabilities("sess-none", vec![cap])
        .expect_err("an insufficient executor must refuse at arm");
    let spelling = format!("{err:?}");
    assert!(spelling.contains("exec_cap_unbound"), "{spelling}");
    // A capability needing nothing binds — the same arm accepts it.
    let ok = exec_cap(Box::new(CountingExec::new()));
    // The session itself does not exist — arming reaches the session
    // check only after the bind verdict (a typed refusal either way).
    let err = svc
        .arm_exec_capabilities("sess-none", vec![ok])
        .expect_err("unknown session refuses after the bind passes");
    let spelling = format!("{err:?}");
    assert!(
        !spelling.contains("exec_cap_unbound"),
        "the bound cap passed its bind check: {spelling}"
    );
}
