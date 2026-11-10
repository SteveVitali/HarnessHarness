//! R5.32 — the confined observer apparatus and Kp4 controls (evidence spec
//! §5.2 / D3-F3; the O1/O2/O8 fixture slice). A real `Store`/WAL, real Π
//! monitor and real `Dispatcher` drive one fixed non-idempotent marker
//! append; the `OpsObserver`/`MarkerSink`/`Sentinel` apparatus (all durable,
//! all outside the run store) records the required intervals. O2 arms the
//! declared `kp-4` commit-before-entry injection; O8 seeds the
//! double-append defect and must read `defect_detected` 3/3 — never a pass.
//!
//! Fixture scope only: this proves the apparatus and the recovery fold, not
//! the confined live backend — actual O×3 backend trials stay gated
//! (R5.44/GATE-G18/G11). Probe and `unknown{worker_lost}` outcomes are
//! retained; an external sink-zero read authorizes nothing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, NetMode, ResourceLimits, WritableRoot};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::events::EventMinter;
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::ops::{
    kp4_outcome, o1_outcome, o8_outcome, probe_authorization, CellVerdict, Kp4Cell, Kp4Outcome,
    MarkerSink, O1Cell, O8Cell, ObservationPoint, ObservedEvent, OpsObserver, ProbeAuthorization,
    ProbeSource, RecordedProbe, Sentinel, TrialFailure, TrialMarker,
};
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
use hharness_ledger::event::{Cursor, Direction, Scope};
use hharness_ledger::fault::KillPoint;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::table::HandleTable;
use hharness_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;

const TTL_MS: u64 = 60_000;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-r5_32-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open_at(root: &std::path::Path, ms: u64, ids: u64) -> (Store, String, Lease) {
    let mut s = Store::open_with(
        root,
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::starting_at(ids))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

/// Reopen the crashed store past the dead writer's lease expiry so the
/// takeover fences it (runtime death + restart; the WAL is what survives).
fn reopen(root: &std::path::Path, ms: u64, ids: u64) -> Store {
    Store::open_with(
        root,
        Box::new(ManualClock::at(ms)),
        Some(Box::new(SeqIds::starting_at(ids))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
}

fn open_scopes(store: &mut Store, run: &str, lease: &Lease) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".to_string());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".to_string()),
        model_call_id: Some("mc-1".to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![t, c]).unwrap();
}

fn test_record() -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalHost,
        image: ImageRef::ContentAddress(hharness_identity::idp::address(
            b"ops-image",
            "application/octet-stream",
        )),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: std::collections::BTreeSet::new(),
        image_attestation: None,
        ext: std::collections::BTreeMap::new(),
        canary_channels: vec![],
    }
}

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    let roots = Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    };
    let mut policy = kernel_default(0);
    policy.fs.write.allow.push(WritableRoot {
        root: ws.display().to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    policy.net.mode = NetMode::None;
    policy.compute_ids();
    let h = driver
        .provision(
            store,
            lease,
            &test_record(),
            roots,
            PolicySlot::Inline(Box::new(policy)),
            OnLoss::FailRun,
            None,
        )
        .unwrap();
    driver
        .attach(
            store,
            lease,
            &h.env_handle_id,
            Some(&Ep2Model::reference()),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    h.env_handle_id
}

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

/// Rows of `class` scoped to one effect.
fn class_count(store: &Store, run: &str, class: &str, effect_id: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class && e.scope.effect_id.as_deref() == Some(effect_id))
        .count()
}

/// The one fixed marker tool's declared class: `fs_write` + `non_idempotent`
/// + `irreversible` (the O-slice's frozen risk).
fn declared_class() -> EffectClass {
    EffectClass {
        domain: EffectDomain::FsWrite,
        attributes: Some(EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::NonIdempotent,
            world: World::Closed,
            reversibility: Reversibility::Irreversible,
        }),
    }
}

fn capability() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("ops marker", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [("path", "string")]
                    .iter()
                    .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                    .collect(),
            ),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(vec![declared_class()].into_iter().collect()),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
            ("param_path", Json::str("path")),
            ("scope_kind", Json::str("fs_path")),
        ])])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([("error_classes", Json::Arr(vec![]))]),
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
        surface_name: "ops_marker".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:ops_marker".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:ops_marker".to_string(),
        arg_map: [(
            "path".to_string(),
            ArgMapEntry {
                capability_param: "path".to_string(),
                transform: ArgTransform::Identity,
                narrowing: None,
            },
        )]
        .into_iter()
        .collect(),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:ops_marker".to_string()],
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

fn ops_monitor(cap: &ToolCapabilityRecord) -> Monitor {
    let mut table = HandleTable::default();
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/ops", 0),
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
        scope: hharness_monitor::decision::DecisionScope::Session,
    };
    table.handles.insert(h.handle_id.clone(), h);
    // fs_write rides the default Π rows (Π-1/Π-2 cover the file domains).
    let mut m = Monitor::new(
        table,
        hharness_monitor::policy::default_table("pol-v1", hharness_monitor::policy::Mode::Attended),
    );
    m.proposers.insert(
        "test:agent".to_string(),
        Label::at(AuthorityClass::Principal),
    );
    m.run_id = "run-1".to_string();
    m.turn_id = "turn-1".to_string();
    m.effect_id = "e1".to_string();
    m.capabilities.insert(
        "test:ops_marker".to_string(),
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
    env: &str,
    ws: &std::path::Path,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: hharness_env::events::ScopeChain {
            turn_id: "turn-1".to_string(),
            model_call_id: "mc-1".to_string(),
            tool_call_id: "tc-1".to_string(),
        },
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: &cap.scope_bindings,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: "test:ops_marker".to_string(),
            version_id: "v-cap".to_string(),
        },
        surface_args: Json::obj([("path", Json::str(format!("{}/marker-target", ws.display())))]),
        args_provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp-1"),
            PersistenceScope::Run,
            0,
        ),
        context_label: Label::at(AuthorityClass::Principal),
        self_report: None,
        env_handle_id: env.to_string(),
        declared: declared_class(),
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
    }
}

/// The confined diagnostic executor: appends the unique trial/effect marker
/// to `MarkerSink` (`dup` appends a second — the O8 seeded defect, despite
/// the `Ok` report) and emits entry/bytes/termination notes to the
/// supervisor's durable record. The probe answers the configured verdict;
/// the world stays the sink file, never runtime memory.
struct MarkerExec {
    decl: ExecutorDeclaration,
    sink_dir: PathBuf,
    obs_dir: PathBuf,
    trial: String,
    dup: bool,
    probe: ProbeVerdict,
}

impl MarkerExec {
    fn new(
        sink_dir: PathBuf,
        obs_dir: PathBuf,
        trial: &str,
        dup: bool,
        probe: ProbeVerdict,
    ) -> Self {
        MarkerExec {
            decl: ExecutorDeclaration {
                executor_id: "ops/marker".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::None,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error"].iter().map(|s| s.to_string()).collect(),
                streams: true,
                domains: [EffectDomain::FsWrite].into_iter().collect(),
            },
            sink_dir,
            obs_dir,
            trial: trial.to_string(),
            dup,
            probe,
        }
    }
}

impl ToolExecutor for MarkerExec {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        _sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        let mut obs =
            OpsObserver::open(&self.obs_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        obs.note(&ObservedEvent::ExecutorEntry {
            effect_id: request.effect_id.clone(),
        })
        .map_err(|e| EnvError::Blob(e.to_string()))?;
        let mut sink =
            MarkerSink::open(&self.sink_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        sink.append(&TrialMarker::new(&self.trial, &request.effect_id, 0))
            .map_err(|e| EnvError::Blob(e.to_string()))?;
        if self.dup {
            sink.append(&TrialMarker::new(&self.trial, &request.effect_id, 1))
                .map_err(|e| EnvError::Blob(e.to_string()))?;
        }
        let bytes = sink
            .markers()
            .map_err(|e| EnvError::Blob(e.to_string()))?
            .len() as u64;
        obs.note(&ObservedEvent::Bytes { count: bytes })
            .map_err(|e| EnvError::Blob(e.to_string()))?;
        obs.note(&ObservedEvent::Termination {
            effect_id: request.effect_id.clone(),
            status: "ok".to_string(),
        })
        .map_err(|e| EnvError::Blob(e.to_string()))?;
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
        Ok(self.probe)
    }
}

/// One cell's surfaces after the drive — what the folds consume.
struct Cell {
    effect_id: String,
    obs_dir: PathBuf,
    sink: MarkerSink,
    sentinel: Sentinel,
    outcome: Result<DispatchOutcome, EnvError>,
    committed: usize,
    observed_rows: usize,
    unknowned: Vec<String>,
    probe_scheduled: bool,
    new_fence: bool,
    extra_commits: usize,
    extra_dispatches: usize,
}

/// Drive one cell: fresh store/workspace/marker/sink/sentinel/approval
/// state, `before_launch` snapshot, the dispatch under an optional
/// declared kill point, `at_boundary` observation, and — when the runtime
/// died — reopen under a new fence, `restore` twice (repeated restoration
/// must add nothing), `after_quiescence` observation.
fn run_cell(tag: &str, kill: Option<KillPoint>, dup: bool) -> Cell {
    let root = dir(tag);
    let ws = dir(&format!("{tag}-ws"));
    let sink_dir = dir(&format!("{tag}-sink"));
    let obs_dir = dir(&format!("{tag}-obs"));
    std::fs::create_dir_all(&ws).unwrap();
    let (mut store, run, lease) = open_at(&root, 1_000, 0);
    open_scopes(&mut store, &run, &lease);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability();
    let bind = binding();
    let mon = ops_monitor(&cap);
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);

    let sink = MarkerSink::open(&sink_dir).unwrap();
    let sentinel = Sentinel::create(&dir(&format!("{tag}-sent")), b"denied").unwrap();
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();

    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MarkerExec::new(
        sink_dir,
        obs_dir.clone(),
        tag,
        dup,
        ProbeVerdict::NotApplied,
    );
    let inp = input(&cap, &bind, &env, &ws);
    if let Some(kp) = kill {
        disp.inject_kill_point(kp);
    }
    let outcome = disp.dispatch(&mut driver, &mut exec, None, &inp, &lease);

    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
        .unwrap();

    let (
        committed,
        observed_rows,
        unknowned,
        probe_scheduled,
        new_fence,
        extra_commits,
        extra_dispatches,
    ) = if matches!(outcome, Err(EnvError::FaultInjected { .. })) {
        // Runtime death — drop the store mid-run; the WAL survives.
        drop(disp);
        drop(driver);
        drop(store);
        let mut store2 = reopen(&root, 120_000, 1_000_000);
        let report = store2.restore(&run, "writer-b", TTL_MS).unwrap();
        let new_fence = report.fenced_lease_id.is_some();
        let unknowned = report.unknowned.clone();
        let probe_scheduled = read_all(&store2, &run).iter().any(|e| {
            e.class == "control.retry.scheduled"
                && (e.payload.get("scope_id").and_then(Json::as_str) == Some(&effect_id)
                    || e.payload.get("effect_id").and_then(Json::as_str) == Some(&effect_id))
        });
        let commits1 = class_count(&store2, &run, "action.effect.committed", &effect_id);
        let dispatched1 = read_all(&store2, &run)
            .iter()
            .filter(|e| e.class == "action.tool.dispatched")
            .count();
        // Repeated restoration adds no blind redispatch / duplicate
        // commit obligation.
        let mut store3 = reopen(&root, 240_000, 2_000_000);
        store3.restore(&run, "writer-c", TTL_MS).unwrap();
        let commits2 = class_count(&store3, &run, "action.effect.committed", &effect_id);
        let dispatched2 = read_all(&store3, &run)
            .iter()
            .filter(|e| e.class == "action.tool.dispatched")
            .count();
        let observed2 = class_count(&store3, &run, "action.effect.observed", &effect_id);
        (
            commits2,
            observed2,
            unknowned,
            probe_scheduled,
            new_fence,
            commits2 - commits1,
            dispatched2 - dispatched1,
        )
    } else {
        let c = class_count(&store, &run, "action.effect.committed", &effect_id);
        let o = class_count(&store, &run, "action.effect.observed", &effect_id);
        (c, o, Vec::new(), false, false, 0, 0)
    };
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
        .unwrap();

    Cell {
        committed,
        observed_rows,
        effect_id,
        obs_dir,
        sink,
        sentinel,
        outcome,
        unknowned,
        probe_scheduled,
        new_fence,
        extra_commits,
        extra_dispatches,
    }
}

/// The supervisor-record completeness check the folds need: all three
/// intervals plus entry and termination notes for the effect.
fn observer_complete(obs_dir: &std::path::Path, effect_id: &str) -> bool {
    let rows = OpsObserver::open(obs_dir).unwrap().rows().unwrap();
    let points = ["before_launch", "at_boundary", "after_quiescence"]
        .iter()
        .all(|p| {
            rows.iter().any(|r| {
                r.get("kind").and_then(Json::as_str) == Some("point")
                    && r.get("point").and_then(Json::as_str) == Some(*p)
            })
        });
    let note = |name: &str| {
        rows.iter().any(|r| {
            r.get("kind").and_then(Json::as_str) == Some("note")
                && r.get("note").and_then(Json::as_str) == Some(name)
                && r.get("effect_id").and_then(Json::as_str) == Some(effect_id)
        })
    };
    points && note("executor_entry") && note("termination")
}

/// The sink digest the record carried at `after_quiescence`.
fn recorded_sink_digest(obs_dir: &std::path::Path) -> Option<String> {
    OpsObserver::open(obs_dir)
        .unwrap()
        .rows()
        .unwrap()
        .iter()
        .rev()
        .find(|r| {
            r.get("kind").and_then(Json::as_str) == Some("point")
                && r.get("point").and_then(Json::as_str) == Some("after_quiescence")
        })
        .and_then(|r| {
            r.get("sink_digest")
                .and_then(Json::as_str)
                .map(String::from)
        })
}

/// The observer-measured boundary delta (before_launch → at_boundary).
fn sink_delta(obs_dir: &std::path::Path) -> i64 {
    let rows = OpsObserver::open(obs_dir).unwrap().rows().unwrap();
    let count = |p: &str| -> i64 {
        rows.iter()
            .find(|r| {
                r.get("kind").and_then(Json::as_str) == Some("point")
                    && r.get("point").and_then(Json::as_str) == Some(p)
            })
            .and_then(|r| r.get("sink_count").and_then(Json::as_int))
            .unwrap_or(0)
    };
    count("at_boundary") - count("before_launch")
}

// ── the cells ────────────────────────────────────────────────────────────────

/// AC-1 / O1: the allowed control — one marker, matching observed digest,
/// full supervisor record, `Pass`.
#[test]
fn o1_allow_cell_passes_with_full_evidence() {
    let c = run_cell("o1", None, false);
    assert!(
        matches!(c.outcome, Ok(DispatchOutcome::Observed(_))),
        "outcome: {:?}",
        c.outcome
    );
    assert_eq!(c.committed, 1);
    assert_eq!(c.observed_rows, 1);
    let verdict = o1_outcome(&O1Cell {
        observer_complete: observer_complete(&c.obs_dir, &c.effect_id),
        observed: true,
        marker_count: c.sink.count_for(&c.effect_id).unwrap(),
        recorded_digest: recorded_sink_digest(&c.obs_dir),
        actual_digest: c.sink.digest(),
        sentinel_intact: c.sentinel.intact(),
    });
    assert_eq!(verdict, CellVerdict::Pass, "{verdict:?}");
    assert!(c.sink.duplicates().unwrap().is_empty());
}

/// AC-1 / O2: commit → declared kp-4 injection before executor entry; sink
/// delta 0; restart under a new fence classifies `unknown{worker_lost}` +
/// scheduled probe; repeated restoration adds nothing. Outcome retained —
/// `unknown`, never a pass.
#[test]
fn o2_kp4_cell_retains_unknown_and_probe() {
    let c = run_cell("kp4", Some(KillPoint::Kp4), false);
    assert!(matches!(
        c.outcome,
        Err(EnvError::FaultInjected { ref at }) if at == "kp-4"
    ));
    assert_eq!(sink_delta(&c.obs_dir), 0, "boundary delta must be 0");
    assert!(c.new_fence, "resuming writer must fence the stale lease");
    assert_eq!(c.unknowned, vec![c.effect_id.clone()]);
    assert!(c.probe_scheduled);
    assert_eq!(c.extra_commits, 0, "repeated restoration must add nothing");
    let cell = Kp4Cell {
        observer_present: true,
        reached: true,
        sink_delta: sink_delta(&c.obs_dir),
        sentinel_intact: c.sentinel.intact(),
        new_fence: c.new_fence,
        unknowned: c.unknowned.clone(),
        probe_scheduled: c.probe_scheduled,
        extra_commits: c.extra_commits,
        extra_dispatches: c.extra_dispatches,
        probe: None,
    };
    assert_eq!(
        kp4_outcome(&cell, &c.effect_id),
        Kp4Outcome::UnknownUnprobed
    );
    let not_applied = RecordedProbe {
        verdict: ProbeVerdict::NotApplied,
        source: ProbeSource::Instrument,
    };
    let probed = Kp4Cell {
        probe: Some(not_applied),
        ..cell
    };
    assert_eq!(
        kp4_outcome(&probed, &c.effect_id),
        Kp4Outcome::ResolvedNotApplied
    );
    assert_eq!(
        probe_authorization(Some(&not_applied)),
        ProbeAuthorization::Admissible(ProbeVerdict::NotApplied)
    );
}

/// AC-1 / O8: three seeded repetitions — the confined diagnostic executor
/// appends twice despite an `Ok` report; the observer flags the duplicate
/// each time. `defect_detected` 3/3 — detector sensitivity, never a pass.
#[test]
fn o8_seeded_duplicate_detected_three_of_three() {
    let mut detected = 0;
    for i in 0..3 {
        let c = run_cell(&format!("o8-{i}"), None, true);
        assert!(matches!(c.outcome, Ok(DispatchOutcome::Observed(_))));
        let verdict = o8_outcome(
            &O8Cell {
                observer_complete: observer_complete(&c.obs_dir, &c.effect_id),
                sentinel_intact: c.sentinel.intact(),
                detected: c.sink.duplicates().unwrap().into_iter().collect(),
            },
            &c.effect_id,
        );
        match verdict {
            CellVerdict::DefectDetected { count, .. } if count == 2 => detected += 1,
            other => panic!("trial {i} expected defect_detected, got {other:?}"),
        }
    }
    assert_eq!(detected, 3, "seeded duplicates detected 3/3");
}

/// AC-1: the supervisor is independently surviving — its record replays
/// after the subject's store is dropped (file fact, not process fact).
#[test]
fn observer_record_survives_subject_death() {
    let d = dir("survival");
    OpsObserver::open(&d)
        .unwrap()
        .note(&ObservedEvent::ExecutorEntry {
            effect_id: "e-1".to_string(),
        })
        .unwrap();
    let rows = OpsObserver::open(&d).unwrap().rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("note").and_then(Json::as_str),
        Some("executor_entry")
    );
}

// ── AC-2 negatives — none of these may pass ─────────────────────────────────

fn good_kp4() -> Kp4Cell {
    Kp4Cell {
        observer_present: true,
        reached: true,
        sink_delta: 0,
        sentinel_intact: true,
        new_fence: true,
        unknowned: vec!["e-1".to_string()],
        probe_scheduled: true,
        extra_commits: 0,
        extra_dispatches: 0,
        probe: None,
    }
}

fn good_o1() -> O1Cell {
    O1Cell {
        observer_complete: true,
        observed: true,
        marker_count: 1,
        recorded_digest: Some("d".to_string()),
        actual_digest: "d".to_string(),
        sentinel_intact: true,
    }
}

#[test]
fn kp4_negatives_cannot_pass() {
    let mut c = good_kp4();
    c.observer_present = false;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::MissingObserver)
    );
    let mut c = good_kp4();
    c.reached = false;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::UnreachedFault)
    );
    let mut c = good_kp4();
    c.sink_delta = 1;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::SinkDelta)
    );
    let mut c = good_kp4();
    c.sentinel_intact = false;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::SentinelMutation)
    );
    let mut c = good_kp4();
    c.new_fence = false;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::StaleFence)
    );
    let mut c = good_kp4();
    c.extra_commits = 1;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::DuplicateRestoration)
    );
    let mut c = good_kp4();
    c.extra_dispatches = 1;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::DuplicateRestoration)
    );
    let mut c = good_kp4();
    c.unknowned = vec![];
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::MissingUnknown)
    );
    let mut c = good_kp4();
    c.probe_scheduled = false;
    assert_eq!(
        kp4_outcome(&c, "e-1"),
        Kp4Outcome::Failed(TrialFailure::MissingProbeObligation)
    );
    // No valid probe / undeterminable / external zero → retained unknown,
    // authorization refused — never a pass.
    let mut c = good_kp4();
    c.probe = Some(RecordedProbe {
        verdict: ProbeVerdict::Undeterminable,
        source: ProbeSource::Instrument,
    });
    assert_eq!(kp4_outcome(&c, "e-1"), Kp4Outcome::UnknownUndeterminable);
    let mut c = good_kp4();
    c.probe = Some(RecordedProbe {
        verdict: ProbeVerdict::NotApplied,
        source: ProbeSource::ExternalSinkZero,
    });
    assert_eq!(kp4_outcome(&c, "e-1"), Kp4Outcome::UnknownUnprobed);
    assert_eq!(
        probe_authorization(None),
        ProbeAuthorization::Refused("no_valid_probe")
    );
    assert_eq!(
        probe_authorization(Some(&RecordedProbe {
            verdict: ProbeVerdict::NotApplied,
            source: ProbeSource::ExternalSinkZero,
        })),
        ProbeAuthorization::Refused("external_zero")
    );
    assert_eq!(
        probe_authorization(Some(&RecordedProbe {
            verdict: ProbeVerdict::Undeterminable,
            source: ProbeSource::Instrument,
        })),
        ProbeAuthorization::Refused("undeterminable")
    );
}

#[test]
fn o_cell_negatives_cannot_pass() {
    // O8: a missed seeded duplicate is `detector_miss`, not a pass.
    assert_eq!(
        o8_outcome(
            &O8Cell {
                observer_complete: true,
                sentinel_intact: true,
                detected: vec![("other".to_string(), 2)],
            },
            "e-1"
        ),
        CellVerdict::Failed(TrialFailure::DetectorMiss)
    );
    let mut c = good_o1();
    c.observer_complete = false;
    assert_eq!(
        o1_outcome(&c),
        CellVerdict::Failed(TrialFailure::MissingObserver)
    );
    let mut c = good_o1();
    c.observed = false;
    assert_eq!(
        o1_outcome(&c),
        CellVerdict::Failed(TrialFailure::Unobserved)
    );
    let mut c = good_o1();
    c.marker_count = 0;
    assert_eq!(
        o1_outcome(&c),
        CellVerdict::Failed(TrialFailure::MissingMarker)
    );
    let mut c = good_o1();
    c.recorded_digest = Some("other".to_string());
    assert_eq!(
        o1_outcome(&c),
        CellVerdict::Failed(TrialFailure::DigestMismatch)
    );
    let mut c = good_o1();
    c.marker_count = 2;
    assert!(matches!(
        o1_outcome(&c),
        CellVerdict::DefectDetected { count: 2, .. }
    ));
}

/// AC-2 (real leg): mutating the denied/read-only target flips `intact()`
/// — the fold then refuses, never passes.
#[test]
fn sentinel_mutation_detected() {
    let d = dir("sent-mut");
    let s = Sentinel::create(&d, b"denied").unwrap();
    assert!(s.intact());
    // A stray write is exactly what the sentinel exists to detect.
    let p = d.join("sentinel.bin");
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&p, perms).unwrap();
    std::fs::write(&p, b"tampered").unwrap();
    assert!(!s.intact());
    let mut c = good_o1();
    c.sentinel_intact = s.intact();
    assert_eq!(
        o1_outcome(&c),
        CellVerdict::Failed(TrialFailure::SentinelMutation)
    );
}
