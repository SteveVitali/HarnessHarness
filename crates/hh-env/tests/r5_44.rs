//! R5.44 — the confined operational proof matrix (GATE-G11's admitted
//! O1–O8×3 scope). Two machinery repairs land first (R5.43's carried
//! predicates, fails-without): `exec_backend_hello_clobber` (policy hello
//! must not clobber the spawned `--backend podman`) and
//! `probe_verb_enoent` (no `/bin/true` placeholder — the verb reaches the
//! journal on the live channel and on the severed-channel reconnect).
//! Then the frozen 24-trial matrix: exec-bearing cells (O1,O2,O3,O4,O5,
//! O8) dispatch the one fixed non-idempotent marker append through the
//! real podman HelperExecutor (`/work/ops/marker.log`, host-visible via
//! the mount); O6/O7 drive the spec-declared native Account+prepare path
//! (deterministic diagnostic transport). Fresh store/workspace/marker/
//! sink/approval state per repetition; surviving observer+sentinel;
//! ≤1 setup retry per cell (new identity); all attempts retained. Typed
//! outcomes only — inconclusive/failed stays that way. Offline,
//! `target/` scratch, USD 0.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_budget::account::{Account, ChargeRequest};
use hharness_budget::attribution::Attribution;
use hharness_budget::{
    BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec, DimensionId, DimensionKey,
    ResourceQuantity, ResourceVector,
};
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
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::{
    ExecutionRequest, ExecutorDeclaration, ExecutorSignal, ProbeVerdict, TerminalReport,
    ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::helper::{HelperClient, HelperExecutor};
use hharness_env::ops::{
    kp4_outcome, o1_outcome, o3_outcome, o4_outcome, o5_outcome, o6_outcome, o7_outcome,
    o8_outcome, Kp4Cell, MarkerSink, O1Cell, O3Cell, O6Cell, O7Cell, O8Cell, ObservationPoint,
    ObservedEvent, OpsCell, OpsObserver, OpsReport, PermissionDenyCell, PermissionWaitCell,
    Sentinel, TrialMarker, TrialRecord,
};
use hharness_env::opslive::{
    assemble, evaluate_cell, evaluate_repairs, kp4_outcome_str, Disposition, RepairsFacts,
    TrialOutcomeRecord, DISCHARGED, LEGS,
};
use hharness_env::preflight::{evaluate_worksheet, Leg, LegVerdict, WorksheetFacts};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, EnvelopeQty, InputState, PrereqStatus, Prerequisite,
    QuoteDecl, ResourceLine, StageDecl, COMPONENTS, ENVELOPE_COMPONENTS, PINS, PREREQUISITES,
};
use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, EndpointAllowlist, NoCredentials, Transport};
use hharness_gateway::openai::ChunkTranslator;
use hharness_gateway::plan::{Capabilities, ModelRef, ProviderRequestPlan};
use hharness_gateway::prepared::{prepare, DispatchVerdict, PrepareInputs, PrepareRefusal};
use hharness_gateway::vocab::{AttemptPolicy, ModelError, ModelErrorClass, Purpose};
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
use hharness_ledger::ids::{effect_id, Clock, ManualClock, SeqIds};
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::approval::{
    ApprovalError, ApprovalResponse, ApprovalState, EndorserRef, RespondCtx, ResponseChoice,
};
use hharness_monitor::decision::{Decision, DecisionScope};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;

const SEED: u64 = 0x5244;
const TTL_MS: u64 = 60_000;

// ── scaffold ──────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    // Stores/socket dirs stay short — the helper's `helper.sock` bind is
    // AF_UNIX and SUN_LEN-bounded; deep `target/` paths refuse to bind.
    let p = std::env::temp_dir().join(format!("r544-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `ws_dir(tag)` — the podman workspace: the VM mounts `$HOME`-only
/// paths, so the cell workspace lives under `target/` (not `/tmp`).
fn ws_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = repo_root()
        .join("target")
        .join(format!("r5_44-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh deterministic id base per cell — `env-<base>` ⇒ a unique
/// `hh-env-<env>` container name (parallel/repeat cells never collide).
fn cell_base() -> u64 {
    static B: AtomicU64 = AtomicU64::new(1);
    B.fetch_add(1, Ordering::Relaxed) * 1_000_000
}

fn open_at(root: &Path, ms: u64, ids: u64) -> (Store, String, Lease) {
    let mut s = Store::open_with(
        root.to_path_buf(),
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

fn open_at_clk(root: &Path, ms: u64, ids: u64) -> (ManualClock, Store, String, Lease) {
    let clk = ManualClock::at(ms);
    let mut s = Store::open_with(
        root.to_path_buf(),
        Box::new(clk.clone()),
        Some(Box::new(SeqIds::starting_at(ids))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (clk, s, run, lease)
}

fn reopen(root: &Path, ms: u64, ids: u64) -> Store {
    Store::open_with(
        root.to_path_buf(),
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

fn test_policy(ws: &Path) -> hharness_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    p.fs.write.allow.push(WritableRoot {
        root: ws.display().to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = hharness_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}

/// The podman cell — `LocalContainer` on the local alpine:3.20 image
/// (unpinned tag, admitted honestly in `unpinned[]`).
fn ready_cell(store: &mut Store, lease: &Lease, driver: &mut EnvDriver, ws: &Path) -> String {
    let image = hharness_helper::podman::PodmanBackend::DEFAULT_IMAGE.to_string();
    let mut record = EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag(image.clone()),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    record.unpinned.insert(image);
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

/// `sweep_hh_env_containers` — remove leaked `hh-env-*` containers before
/// a cell (a SIGKILLed helper leaves its `--name` container; the sweep is
/// hygiene, never a hidden retry — every setup attempt is recorded).
fn sweep_hh_env_containers() {
    let out = std::process::Command::new("podman")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output();
    if let Ok(o) = out {
        for name in String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|n| n.starts_with("hh-env-"))
        {
            let _ = std::process::Command::new("podman")
                .args(["rm", "-f", name])
                .output();
        }
    }
}

fn marker_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::NonIdempotent,
        world: World::Closed,
        reversibility: Reversibility::Irreversible,
    }
}

fn fs_class() -> EffectClass {
    EffectClass {
        domain: EffectDomain::FsWrite,
        attributes: Some(marker_attrs()),
    }
}

fn perm_class() -> EffectClass {
    EffectClass {
        domain: EffectDomain::PermissionRequest,
        attributes: Some(marker_attrs()),
    }
}

fn cap_for(sid: &str, class: EffectClass) -> ToolCapabilityRecord {
    let _ = sid;
    ToolCapabilityRecord {
        purpose: Text::new("ops marker", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::obj([("argv", Json::obj([("type", Json::str("array"))]))]),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(vec![class].into_iter().collect()),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
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

fn binding(sid: &str, surface: &str) -> SurfaceBinding {
    SurfaceBinding {
        surface_name: surface.to_string(),
        capability_ref: PinnedRef {
            semantic_id: sid.to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: sid.to_string(),
        arg_map: BTreeMap::from([(
            "argv".to_string(),
            ArgMapEntry {
                capability_param: "argv".to_string(),
                transform: ArgTransform::Identity,
                narrowing: None,
            },
        )]),
        dialect: "json-schema-2020-12".to_string(),
        surface_id: String::new(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec![sid.to_string()],
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

/// The monitor — grants every domain the cells dispatch (`domains`), or
/// the O3 deny shape when `domains` lacks the dispatched one.
fn ops_monitor(
    cap: &ToolCapabilityRecord,
    sid: &str,
    surface: &str,
    domains: &[EffectDomain],
) -> Monitor {
    let mut table = HandleTable::default();
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".to_string(),
            version_id: "v-perm".to_string(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/ops", 0),
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
        basis_ref: "test:agent#sha256:def".to_string(),
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
        sid.to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: binding(sid, surface),
        },
    );
    m
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".to_string(),
        model_call_id: "mc-1".to_string(),
        tool_call_id: "tc-1".to_string(),
    }
}

fn input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env_handle_id: &str,
    sid: &str,
    args: Json,
    declared: EffectClass,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: chain(),
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: sid.to_string(),
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
        compensation_plan_id: Some("plan-1".to_string()),
        baseline_ref: None,
        flow: Default::default(),
        remedy_taken: None,
        result_renderer: None,
        submission_marker: None,
    }
}

/// The canonical marker line the in-container exec appends — the same
/// record `MarkerSink` re-reads (one codec; the sink parses it back).
fn marker_line(trial: &str, effect_id: &str, ordinal: u64) -> String {
    let m = TrialMarker::new(trial, effect_id, ordinal);
    Json::obj([
        ("effect_id", Json::str(&m.effect_id)),
        ("marker_id", Json::str(&m.marker_id)),
        ("ordinal", Json::Int(m.ordinal as i64)),
        ("trial_id", Json::str(&m.trial_id)),
    ])
    .to_canonical_string()
}

fn marker_argv(trial: &str, effect_id: &str, dup: bool) -> Json {
    let line0 = marker_line(trial, effect_id, 0);
    let cmd = if dup {
        let line1 = marker_line(trial, effect_id, 1);
        format!("printf '%s\\n' '{line0}' >> /work/ops/marker.log; printf '%s\\n' '{line1}' >> /work/ops/marker.log")
    } else {
        format!("printf '%s\\n' '{line0}' >> /work/ops/marker.log")
    };
    Json::obj([(
        "argv",
        Json::Arr(vec![Json::str("/bin/sh"), Json::str("-c"), Json::str(cmd)]),
    )])
}

/// `ObservedExec` — the instrument executor: forwards the real
/// `HelperExecutor` and durably notes executor entry/termination/bytes to
/// the surviving `OpsObserver` at the actual exec boundary (the
/// supervisor's own record — the marker append itself is the real
/// in-container argv). Advertises `PermissionRequest` in addition to the
/// helper's domains so an O4/O5 suspended effect's resume can reach the
/// real executor — the instrument's declaration, recorded honestly.
struct ObservedExec {
    decl: ExecutorDeclaration,
    inner: HelperExecutor,
    obs_dir: PathBuf,
    sink_dir: PathBuf,
}

impl ObservedExec {
    fn take(
        driver: &mut EnvDriver,
        env_id: &str,
        obs_dir: &Path,
        sink_dir: &Path,
        extra_domains: &[EffectDomain],
    ) -> Result<ObservedExec, EnvError> {
        let inner = driver.take_executor(env_id, vec![])?;
        let mut decl = inner.declaration().clone();
        for d in extra_domains {
            decl.domains.insert(*d);
        }
        Ok(ObservedExec {
            decl,
            inner,
            obs_dir: obs_dir.to_path_buf(),
            sink_dir: sink_dir.to_path_buf(),
        })
    }
    /// The session's verbatim backend (`container` post-repair).
    fn backend(&self) -> String {
        self.inner.client.backend.clone()
    }
}

impl ToolExecutor for ObservedExec {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        let mut obs =
            OpsObserver::open(&self.obs_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        obs.note(&ObservedEvent::ExecutorEntry {
            effect_id: request.effect_id.clone(),
        })
        .map_err(|e| EnvError::Blob(e.to_string()))?;
        let r = self.inner.execute(request, sink);
        let status = match &r {
            Ok(t) => format!("{:?}", t.status),
            Err(e) => format!("error:{e}"),
        };
        if let Ok(s) = MarkerSink::open(&self.sink_dir) {
            if let Ok(n) = s.markers().map(|m| m.len() as u64) {
                let _ = obs.note(&ObservedEvent::Bytes { count: n });
            }
        }
        let _ = obs.note(&ObservedEvent::Termination {
            effect_id: request.effect_id.clone(),
            status,
        });
        r
    }
    fn probe(&self, effect_id: &str, attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        self.inner.probe(effect_id, attempt_no)
    }
}

/// One podman cell's live surfaces — every field the folds consume.
struct Cell {
    root: PathBuf,
    ops_dir: PathBuf,
    store: Store,
    run: String,
    lease: Lease,
    driver: EnvDriver,
    env_id: String,
    sink: MarkerSink,
    sentinel: Sentinel,
}

/// Provision one fresh podman cell + its ops apparatus (fresh
/// store/workspace/marker/sink/sentinel per repetition).
fn new_cell(tag: &str) -> Result<Cell, String> {
    let base = cell_base();
    let root = dir(tag);
    let ws = ws_dir(&format!("{tag}-ws"));
    let ws = std::fs::canonicalize(&ws).map_err(|e| e.to_string())?;
    let ops_dir = ws.join("ops");
    std::fs::create_dir_all(&ops_dir).map_err(|e| e.to_string())?;
    let (mut store, run, lease) = open_at(&root, 1_000, base);
    open_scopes(&mut store, &run, &lease);
    let mut driver = EnvDriver::new(&run);
    let env_id = ready_cell(&mut store, &lease, &mut driver, &ws);
    let sink = MarkerSink::open(&ops_dir).map_err(|e| e.to_string())?;
    let sentinel = Sentinel::create(&ops_dir, b"denied").map_err(|e| e.to_string())?;
    OpsObserver::open(&ops_dir)
        .map_err(|e| e.to_string())?
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .map_err(|e| e.to_string())?;
    Ok(Cell {
        root,
        ops_dir,
        store,
        run,
        lease,
        driver,
        env_id,
        sink,
        sentinel,
    })
}

fn effect_of(run: &str) -> String {
    effect_id(run, "mc-1", "tc-1", 0)
}

fn snapshot(ops_dir: &Path, point: ObservationPoint, sink: &MarkerSink, sentinel: &Sentinel) {
    OpsObserver::open(ops_dir)
        .unwrap()
        .snapshot(point, sink, sentinel)
        .unwrap();
}

fn point_count(ops_dir: &Path) -> usize {
    OpsObserver::open(ops_dir)
        .unwrap()
        .rows()
        .unwrap()
        .iter()
        .filter(|r| r.get("kind").and_then(Json::as_str) == Some("point"))
        .count()
}

fn note_count(ops_dir: &Path, note: &str, eid: &str) -> usize {
    OpsObserver::open(ops_dir)
        .unwrap()
        .rows()
        .unwrap()
        .iter()
        .filter(|r| {
            r.get("kind").and_then(Json::as_str) == Some("note")
                && r.get("note").and_then(Json::as_str) == Some(note)
                && r.get("effect_id").and_then(Json::as_str) == Some(eid)
        })
        .count()
}

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

fn class_count(store: &Store, run: &str, class: &str, eid: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class && e.scope.effect_id.as_deref() == Some(eid))
        .count()
}

fn recorded_sink_digest(ops_dir: &Path) -> Option<String> {
    OpsObserver::open(ops_dir)
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

fn sink_delta(ops_dir: &Path) -> i64 {
    let rows = OpsObserver::open(ops_dir).unwrap().rows().unwrap();
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

fn observer_complete(ops_dir: &Path, eid: &str) -> bool {
    point_count(ops_dir) == 3
        && note_count(ops_dir, "executor_entry", eid) == 1
        && note_count(ops_dir, "termination", eid) == 1
}

fn blank_trial(tid: &str) -> TrialOutcomeRecord {
    TrialOutcomeRecord {
        trial_id: tid.to_string(),
        invoked: false,
        boundary_observed: false,
        fully_observable: false,
        outcome: "unattempted".to_string(),
        violation: false,
        probe: None,
        executor_backend: None,
        setup_retries: 0,
        failure: None,
    }
}

fn report_record(
    t: &TrialOutcomeRecord,
    cell: OpsCell,
    rep: u8,
    outcome: hharness_env::ops::TrialOutcome,
) -> TrialRecord {
    TrialRecord {
        trial_id: t.trial_id.clone(),
        cell,
        rep,
        invoked: t.invoked,
        boundary_observed: t.boundary_observed,
        fully_observable: t.fully_observable,
        outcome,
        violation: t.violation,
        negative_control: cell == OpsCell::O3 || cell == OpsCell::O8,
        control_failed: false,
    }
}

// ── the cell drivers ───────────────────────────────────────────────────

fn drive_o1o8(cell: &mut Cell, tid: &str, dup: bool) -> (TrialOutcomeRecord, ()) {
    let mut t = blank_trial(tid);
    let eid = effect_of(&cell.run);
    let cap = cap_for("test:ops_marker", fs_class());
    let bind = binding("test:ops_marker", "ops_marker");
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let monitor = ops_monitor(
        &cap,
        "test:ops_marker",
        "ops_marker",
        &[EffectDomain::FsWrite, EffectDomain::Exec],
    );
    let mut disp = Dispatcher::new(
        &mut cell.store,
        &monitor,
        &cell.run,
        [7; 32],
        DetectorSet::default(),
    );
    let inp = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:ops_marker",
        marker_argv(tid, &eid, dup),
        fs_class(),
    );
    t.invoked = true;
    let mut exec = match ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[],
    ) {
        Ok(x) => x,
        Err(e) => {
            t.failure = Some(format!("take_executor:{e}"));
            return (t, ());
        }
    };
    t.executor_backend = Some(exec.backend());
    let out = disp.dispatch(&mut cell.driver, &mut exec, None, &inp, &cell.lease);
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AtBoundary,
        &cell.sink,
        &cell.sentinel,
    );
    // The instrument probe — the repaired verb on the live channel.
    t.probe = Some(match exec.probe(&eid, 1) {
        Ok(v) => v.as_str().to_string(),
        Err(e) => format!("error:{e}"),
    });
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AfterQuiescence,
        &cell.sink,
        &cell.sentinel,
    );
    let observed = matches!(out, Ok(DispatchOutcome::Observed(_)));
    let verdict = if dup {
        o8_outcome(
            &O8Cell {
                observer_complete: observer_complete(&cell.ops_dir, &eid),
                sentinel_intact: cell.sentinel.intact(),
                detected: cell.sink.duplicates().unwrap().into_iter().collect(),
            },
            &eid,
        )
    } else {
        o1_outcome(&O1Cell {
            observer_complete: observer_complete(&cell.ops_dir, &eid),
            observed,
            marker_count: cell.sink.count_for(&eid).unwrap(),
            recorded_digest: recorded_sink_digest(&cell.ops_dir),
            actual_digest: cell.sink.digest(),
            sentinel_intact: cell.sentinel.intact(),
        })
    };
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = match &verdict {
        hharness_env::ops::CellVerdict::Pass => "pass".to_string(),
        hharness_env::ops::CellVerdict::DefectDetected { count, .. } => {
            format!("defect_detected:{count}")
        }
        hharness_env::ops::CellVerdict::Failed(f) => format!("failed:{}", f.as_str()),
    };
    t.violation = matches!(verdict, hharness_env::ops::CellVerdict::Failed(_));
    (t, ())
}

fn drive_o1(cell: &mut Cell, tid: &str) -> TrialOutcomeRecord {
    drive_o1o8(cell, tid, false).0
}

fn drive_o8(cell: &mut Cell, tid: &str) -> TrialOutcomeRecord {
    drive_o1o8(cell, tid, true).0
}

fn drive_o2(cell: &mut Cell, tid: &str) -> TrialOutcomeRecord {
    let mut t = blank_trial(tid);
    let eid = effect_of(&cell.run);
    let cap = cap_for("test:ops_marker", fs_class());
    let bind = binding("test:ops_marker", "ops_marker");
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let monitor = ops_monitor(
        &cap,
        "test:ops_marker",
        "ops_marker",
        &[EffectDomain::FsWrite, EffectDomain::Exec],
    );
    let mut disp = Dispatcher::new(
        &mut cell.store,
        &monitor,
        &cell.run,
        [7; 32],
        DetectorSet::default(),
    );
    disp.inject_kill_point(KillPoint::Kp4);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:ops_marker",
        marker_argv(tid, &eid, false),
        fs_class(),
    );
    t.invoked = true;
    let mut exec = match ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[],
    ) {
        Ok(x) => x,
        Err(e) => {
            t.failure = Some(format!("take_executor:{e}"));
            return t;
        }
    };
    t.executor_backend = Some(exec.backend());
    let out = disp.dispatch(&mut cell.driver, &mut exec, None, &inp, &cell.lease);
    let reached = matches!(out, Err(EnvError::FaultInjected { ref at }) if at == "kp-4");
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AtBoundary,
        &cell.sink,
        &cell.sentinel,
    );
    // The instrument probe — the effect never entered (kp-4), so the
    // journal holds no execution: the repaired verb answers
    // `undeterminable` (typed, never simulated).
    t.probe = Some(match exec.probe(&eid, 1) {
        Ok(v) => v.as_str().to_string(),
        Err(e) => format!("error:{e}"),
    });
    drop(disp);
    drop(exec);
    // Kernel loss — the runtime dies (the store closes); the WAL
    // survives on disk while the driver/helper keep the environment.
    let (swap, _, _) = open_at(&dir(&format!("{tid}-swap")), 1, cell_base());
    drop(std::mem::replace(&mut cell.store, swap));
    let mut store2 = reopen(&cell.root, 120_000, cell_base());
    let report = store2.restore(&cell.run, "writer-b", TTL_MS).unwrap();
    let new_fence = report.fenced_lease_id.is_some();
    let unknowned = report.unknowned.clone();
    let probe_scheduled = read_all(&store2, &cell.run).iter().any(|e| {
        e.class == "control.retry.scheduled"
            && (e.payload.get("scope_id").and_then(Json::as_str) == Some(&eid)
                || e.payload.get("effect_id").and_then(Json::as_str) == Some(&eid))
    });
    let commits1 = class_count(&store2, &cell.run, "action.effect.committed", &eid);
    let dispatched1 = read_all(&store2, &cell.run)
        .iter()
        .filter(|e| e.class == "action.tool.dispatched")
        .count();
    let mut store3 = reopen(&cell.root, 240_000, cell_base());
    store3.restore(&cell.run, "writer-c", TTL_MS).unwrap();
    let commits2 = class_count(&store3, &cell.run, "action.effect.committed", &eid);
    let dispatched2 = read_all(&store3, &cell.run)
        .iter()
        .filter(|e| e.class == "action.tool.dispatched")
        .count();
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AfterQuiescence,
        &cell.sink,
        &cell.sentinel,
    );
    let kp = Kp4Cell {
        observer_present: point_count(&cell.ops_dir) == 3,
        reached,
        sink_delta: sink_delta(&cell.ops_dir),
        sentinel_intact: cell.sentinel.intact(),
        new_fence,
        unknowned: unknowned.clone(),
        probe_scheduled,
        extra_commits: commits2 - commits1,
        extra_dispatches: dispatched2 - dispatched1,
        probe: None,
    };
    let verdict = kp4_outcome(&kp, &eid);
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = kp4_outcome_str(&verdict);
    t.violation = matches!(verdict, hharness_env::ops::Kp4Outcome::Failed(_));
    t
}

fn drive_o3(cell: &mut Cell, tid: &str) -> (TrialOutcomeRecord, O3Cell) {
    let mut t = blank_trial(tid);
    let eid = effect_of(&cell.run);
    let cap = cap_for("test:ops_marker", fs_class());
    let bind = binding("test:ops_marker", "ops_marker");
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    // The deny: the handle grants Exec only — the dispatched `fs_write`
    // effect is uncovered and the monitor refuses at authorize.
    let monitor = ops_monitor(&cap, "test:ops_marker", "ops_marker", &[EffectDomain::Exec]);
    let mut disp = Dispatcher::new(
        &mut cell.store,
        &monitor,
        &cell.run,
        [7; 32],
        DetectorSet::default(),
    );
    let inp = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:ops_marker",
        marker_argv(tid, &eid, false),
        fs_class(),
    );
    t.invoked = true;
    let mut exec = match ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[],
    ) {
        Ok(x) => x,
        Err(e) => {
            t.failure = Some(format!("take_executor:{e}"));
            return (
                t,
                O3Cell {
                    observer_complete: false,
                    refused: false,
                    pre_commit_rows: 0,
                    executor_entries: 0,
                    marker_count: 0,
                    sentinel_intact: cell.sentinel.intact(),
                    same_path_positive: false,
                },
            );
        }
    };
    t.executor_backend = Some(exec.backend());
    let out = disp.dispatch(&mut cell.driver, &mut exec, None, &inp, &cell.lease);
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AtBoundary,
        &cell.sink,
        &cell.sentinel,
    );
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AfterQuiescence,
        &cell.sink,
        &cell.sentinel,
    );
    let refused = matches!(out, Ok(DispatchOutcome::Refused { .. }));
    // The fold waits for the post-pass — the block's O1 comparator may
    // run after O3 in the frozen permutation.
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = "deferred".to_string();
    (
        t,
        O3Cell {
            observer_complete: point_count(&cell.ops_dir) == 3,
            refused,
            pre_commit_rows: class_count(&cell.store, &cell.run, "action.effect.prepared", &eid)
                + class_count(&cell.store, &cell.run, "action.effect.committed", &eid),
            executor_entries: note_count(&cell.ops_dir, "executor_entry", &eid),
            marker_count: cell.sink.count_for(&eid).unwrap(),
            sentinel_intact: cell.sentinel.intact(),
            same_path_positive: false,
        },
    )
}

// ── O4/O5 — the real permission wait/answer/resume chain ───────────────

fn answer(
    store: &mut Store,
    run: &str,
    lease: &Lease,
    pid: &str,
    choice: ResponseChoice,
    scope: DecisionScope,
    decided_by: EndorserRef,
) -> Result<hharness_monitor::approval::RespondOutcome, ApprovalError> {
    let events = store.events(run).unwrap().to_vec();
    let head = events.last().map(|e| e.seq).unwrap_or(0);
    let mut st = ApprovalState::project(&events, head);
    let asked_risk = events
        .iter()
        .rev()
        .find(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
                && e.payload.get("decision").and_then(Json::as_str) == Some("ask")
        })
        .and_then(|e| e.payload.get("effective_risk_class"))
        .and_then(hharness_ontology::risk::RiskClass::from_json);
    let irreversible = asked_risk
        .map(hharness_monitor::approval::never_auto)
        .unwrap_or(false);
    let requested_at = events
        .iter()
        .find(|e| {
            e.class == "security.permission.pending"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
        })
        .and_then(|e| e.payload.get("requested_at").and_then(Json::as_int))
        .unwrap_or(0)
        .max(0) as u64;
    let now = store.now_ms();
    let ctx = RespondCtx {
        policy_fingerprint: hharness_monitor::approval::policy_fingerprint(
            "pol-v1",
            "attended",
            &[],
        ),
        scope_ref: run.to_string(),
        risk_ceiling: asked_risk.unwrap_or(hharness_ontology::risk::RiskClass::UNKNOWN),
        grant_authority: AuthorityClass::Principal,
        grants: st.grants.values().cloned().collect(),
        denial_policy: None,
    };
    let out = st.respond_with_ctx(
        &ApprovalResponse {
            permission_id: pid.to_string(),
            choice,
            scope,
            max_uses: None,
            justification: None,
            decided_by: decided_by.clone(),
            decided_at: now,
        },
        irreversible,
        now,
        &ctx,
    )?;
    if out.already_decided || matches!(out.decision, Decision::Ask { .. }) {
        return Ok(out);
    }
    let tag = match &out.decision {
        Decision::Allow => "allow",
        Decision::Deny { .. } => "deny",
        Decision::Modified { .. } => "modified",
        Decision::Ask { .. } => unreachable!(),
    };
    let eid = Store::effect_id(run, "mc-1", "tc-1", 0);
    let decider_ref = match &decided_by {
        EndorserRef::Human { subject_ref, .. } => subject_ref.clone(),
        EndorserRef::ApproverGrant { grant_ref } => grant_ref.clone(),
    };
    let mut decided = EventMinter::new(store, run)
        .mint(
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str(pid)),
                ("effect_id", Json::str(eid.clone())),
                ("decision", Json::str(tag)),
                ("decider", Json::str("human")),
                ("decider_ref", Json::str(decider_ref)),
                ("decision_scope", Json::str(scope.as_str())),
                ("requested_at", Json::Int(requested_at as i64)),
                (
                    "wait_ms",
                    Json::Int(now.saturating_sub(requested_at) as i64),
                ),
                ("attempt_no", Json::Int(1)),
            ]),
        )
        .unwrap();
    decided.scope.effect_id = Some(eid);
    store.append(run, lease, vec![decided]).unwrap();
    Ok(out)
}

fn final_decided(store: &Store, run: &str, pid: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
                && e.payload.get("decision").and_then(Json::as_str) != Some("ask")
        })
        .count()
}

fn pending_row(store: &Store, run: &str, pid: &str) -> bool {
    read_all(store, run).iter().any(|e| {
        e.class == "security.permission.pending"
            && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
    })
}

fn wait_interval_clean(store: &Store, run: &str, sink: &MarkerSink, eid: &str) -> bool {
    sink.count_for(eid).unwrap() == 0
        && class_count(store, run, "action.effect.prepared", eid) == 0
        && class_count(store, run, "action.effect.committed", eid) == 0
        && class_count(store, run, "action.tool.started", eid) == 0
}

/// Suspend one ask on a fresh podman cell — durable pending + offered
/// set + suspended{awaiting_approval}, never a fabricated pending.
fn suspend_cell(
    cell: &mut Cell,
    tid: &str,
) -> (
    String,
    String,
    ToolCapabilityRecord,
    SurfaceBinding,
    ScopeBindings,
) {
    let eid = effect_of(&cell.run);
    let cap = cap_for("test:perm_marker", perm_class());
    let bind = binding("test:perm_marker", "perm_marker");
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let monitor = ops_monitor(
        &cap,
        "test:perm_marker",
        "perm_marker",
        &[
            EffectDomain::FsWrite,
            EffectDomain::PermissionRequest,
            EffectDomain::Exec,
        ],
    );
    let mut disp = Dispatcher::new(
        &mut cell.store,
        &monitor,
        &cell.run,
        [7; 32],
        DetectorSet::default(),
    );
    let inp = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:perm_marker",
        marker_argv(tid, &eid, false),
        perm_class(),
    );
    let mut exec = match ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[EffectDomain::PermissionRequest],
    ) {
        Ok(x) => x,
        Err(e) => panic!("suspend exec: {e}"),
    };
    let out = disp
        .dispatch(&mut cell.driver, &mut exec, None, &inp, &cell.lease)
        .unwrap();
    let DispatchOutcome::Suspended { permission_id } = out else {
        panic!("the ask suspends awaiting_approval, got {out:?}")
    };
    // The session survives the suspend — the executor goes back (a
    // dropped executor kills the helper the resume needs).
    let _ = cell.driver.return_executor(&cell.env_id, exec.inner);
    (permission_id, eid, cap, bind, sb)
}

fn drive_o4(cell: &mut Cell, tid: &str) -> TrialOutcomeRecord {
    let mut t = blank_trial(tid);
    let (pid, eid, cap, bind, sb) = suspend_cell(cell, tid);
    t.invoked = true;
    let run = cell.run.clone();
    let clean1 = wait_interval_clean(&cell.store, &run, &cell.sink, &eid);
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AtBoundary,
        &cell.sink,
        &cell.sentinel,
    );
    let clean2 = wait_interval_clean(&cell.store, &run, &cell.sink, &eid);
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AfterQuiescence,
        &cell.sink,
        &cell.sentinel,
    );
    let principal = || EndorserRef::Human {
        subject_ref: "host:operator".to_string(),
        authority: AuthorityClass::Principal,
    };
    let out = answer(
        &mut cell.store,
        &run,
        &cell.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    let decided_ok = matches!(out.decision, Decision::Allow) && !out.already_decided;
    let now = cell.store.now_ms();
    cell.store.deliver_wakeup(&run, &cell.lease, now).unwrap();
    let envs = read_all(&cell.store, &run);
    let wakeup_fired = envs.iter().any(|e| e.class == "control.wakeup.occurred")
        && envs.iter().any(|e| e.class == "control.wakeup.fired");
    // Resume: rebuilt monitor folding the recorded decision; the same
    // input re-dispatches through the podman cell — exactly one marker.
    let mut mon2 = ops_monitor(
        &cap,
        "test:perm_marker",
        "perm_marker",
        &[
            EffectDomain::FsWrite,
            EffectDomain::PermissionRequest,
            EffectDomain::Exec,
        ],
    );
    mon2.approvals = ApprovalState::project(&envs, envs.last().unwrap().seq);
    let mut disp2 = Dispatcher::new(
        &mut cell.store,
        &mon2,
        &run,
        [8; 32],
        DetectorSet::default(),
    );
    let inp2 = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:perm_marker",
        marker_argv(tid, &eid, false),
        perm_class(),
    );
    let mut exec = ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[EffectDomain::PermissionRequest],
    )
    .unwrap();
    t.executor_backend = Some(exec.backend());
    let out2 = disp2
        .dispatch(&mut cell.driver, &mut exec, None, &inp2, &cell.lease)
        .unwrap();
    let resumed = matches!(out2, DispatchOutcome::Observed(_));
    t.probe = Some(match exec.probe(&eid, 1) {
        Ok(v) => v.as_str().to_string(),
        Err(e) => format!("error:{e}"),
    });
    // Idempotent identical answer; conflicting deny never authorizes.
    let again = answer(
        &mut cell.store,
        &run,
        &cell.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    let idempotent = again.already_decided && matches!(again.decision, Decision::Allow);
    let conflict = answer(
        &mut cell.store,
        &run,
        &cell.lease,
        &pid,
        ResponseChoice::Deny {
            reason: "changed mind".into(),
        },
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    let conflicting_refused = conflict.already_decided
        && matches!(conflict.decision, Decision::Allow)
        && final_decided(&cell.store, &run, &pid) == 1
        && cell.sink.count_for(&eid).unwrap() == 1;
    // Restart leg: a second suspended podman cell dies mid-wait; restore
    // preserves the pending.
    let mut restart_preserved = false;
    if let Ok(mut b) = new_cell(&format!("{tid}-restart")) {
        let (bpid, _beid, _c, _d, _e) = suspend_cell(&mut b, tid);
        let root = b.root.clone();
        drop(b.store);
        let mut store2 = reopen(&root, 120_000, cell_base());
        let report = store2.restore(&b.run, "writer-b", TTL_MS).unwrap();
        restart_preserved = report.pending_permissions.iter().any(|p| p == &bpid);
    }
    let marker = TrialMarker::new(tid, &eid, 0);
    let digest_matches = cell
        .sink
        .markers()
        .unwrap()
        .iter()
        .any(|m| m.digest() == marker.digest());
    let verdict = o4_outcome(&PermissionWaitCell {
        pending_durable: pending_row(&cell.store, &run, &pid),
        wait_intervals_clean: [clean1, clean2].iter().filter(|c| **c).count() as u8,
        restart_preserved,
        wakeup_fired,
        decided_durable: decided_ok && final_decided(&cell.store, &run, &pid) >= 1,
        marker_count: cell.sink.count_for(&eid).unwrap(),
        digest_matches: resumed && digest_matches,
        idempotent_replay: idempotent,
        conflicting_refused,
        sentinel_intact: cell.sentinel.intact(),
    });
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = match &verdict {
        hharness_env::ops::PermissionOutcome::Pass => "pass".to_string(),
        hharness_env::ops::PermissionOutcome::Failed(f) => format!("failed:{}", f.as_str()),
    };
    t.violation = matches!(verdict, hharness_env::ops::PermissionOutcome::Failed(_));
    t
}

fn drive_o5(cell: &mut Cell, tid: &str) -> TrialOutcomeRecord {
    let mut t = blank_trial(tid);
    let (pid, eid, _cap, _bind, _sb) = suspend_cell(cell, tid);
    t.invoked = true;
    let run = cell.run.clone();
    let out = answer(
        &mut cell.store,
        &run,
        &cell.lease,
        &pid,
        ResponseChoice::Deny {
            reason: "refused".into(),
        },
        DecisionScope::Once,
        EndorserRef::Human {
            subject_ref: "host:operator".to_string(),
            authority: AuthorityClass::Principal,
        },
    )
    .unwrap();
    let deny_ok = matches!(out.decision, Decision::Deny { .. });
    let now = cell.store.now_ms();
    cell.store.deliver_wakeup(&run, &cell.lease, now).unwrap();
    let mut mon2 = ops_monitor(
        &cap_for("test:perm_marker", perm_class()),
        "test:perm_marker",
        "perm_marker",
        &[
            EffectDomain::FsWrite,
            EffectDomain::PermissionRequest,
            EffectDomain::Exec,
        ],
    );
    let envs = read_all(&cell.store, &run);
    mon2.approvals = ApprovalState::project(&envs, envs.last().unwrap().seq);
    let cap = cap_for("test:perm_marker", perm_class());
    let bind = binding("test:perm_marker", "perm_marker");
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mut disp2 = Dispatcher::new(
        &mut cell.store,
        &mon2,
        &run,
        [9; 32],
        DetectorSet::default(),
    );
    let inp2 = input(
        &cap,
        &bind,
        &sb,
        &cell.env_id,
        "test:perm_marker",
        marker_argv(tid, &eid, false),
        perm_class(),
    );
    let mut exec = ObservedExec::take(
        &mut cell.driver,
        &cell.env_id,
        &cell.ops_dir,
        &cell.ops_dir,
        &[EffectDomain::PermissionRequest],
    )
    .unwrap();
    t.executor_backend = Some(exec.backend());
    let out2 = disp2
        .dispatch(&mut cell.driver, &mut exec, None, &inp2, &cell.lease)
        .unwrap();
    let refused = matches!(out2, DispatchOutcome::Refused { .. });
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AtBoundary,
        &cell.sink,
        &cell.sentinel,
    );
    snapshot(
        &cell.ops_dir,
        ObservationPoint::AfterQuiescence,
        &cell.sink,
        &cell.sentinel,
    );
    let no_mutation = wait_interval_clean(&cell.store, &run, &cell.sink, &eid);
    let conflict = answer(
        &mut cell.store,
        &run,
        &cell.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        EndorserRef::Human {
            subject_ref: "host:operator".to_string(),
            authority: AuthorityClass::Principal,
        },
    )
    .unwrap();
    let conflicting_refused = conflict.already_decided
        && matches!(conflict.decision, Decision::Deny { .. })
        && final_decided(&cell.store, &run, &pid) == 1
        && !read_all(&cell.store, &run)
            .iter()
            .any(|e| e.class == "security.permission.granted")
        && cell.sink.count_for(&eid).unwrap() == 0;
    let verdict = o5_outcome(&PermissionDenyCell {
        pending_durable: pending_row(&cell.store, &run, &pid),
        deny_decided: deny_ok && final_decided(&cell.store, &run, &pid) >= 1,
        refused,
        no_mutation,
        conflicting_refused,
        sentinel_intact: cell.sentinel.intact(),
    });
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = match &verdict {
        hharness_env::ops::PermissionOutcome::Pass => "pass".to_string(),
        hharness_env::ops::PermissionOutcome::Failed(f) => format!("failed:{}", f.as_str()),
    };
    t.violation = matches!(verdict, hharness_env::ops::PermissionOutcome::Failed(_));
    t
}

// ── O6/O7 — the native Account path (deterministic diagnostic transport) ──

fn primary(d: DimensionId) -> DimensionKey {
    DimensionKey::Primary(d)
}

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
        provider_model_id: "fixture-model".to_string(),
        snapshot_id: None,
        serving_route: None,
        effort: None,
    }
}

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

fn apolicy(max_attempts: u32) -> AttemptPolicy {
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

/// The deterministic diagnostic transport: every `send` is an
/// independently counted entry (a refused/expired call must leave it
/// untouched). Scripted replies; never the live provider path.
struct CountingTransport {
    entries: u64,
    sends: Vec<Vec<u8>>,
    queued: std::collections::VecDeque<Result<Vec<(u64, WireFrame)>, ModelError>>,
}

impl Transport for CountingTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        _c: &CredentialHandle,
        _e: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        self.entries += 1;
        self.sends.push(bytes.to_vec());
        self.queued
            .pop_front()
            .unwrap_or_else(|| Err(ModelError::new(ModelErrorClass::Network, "unscripted send")))
    }
}

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

fn open_turn(store: &mut Store, run: &str, lease: &Lease) {
    use hharness_ledger::event::{Event, Producer};
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

fn drive_o6(tid: &str) -> TrialOutcomeRecord {
    let mut t = blank_trial(tid);
    let root = dir(tid);
    let (_clk, mut store, run, lease) = open_at_clk(&root, 1_000, cell_base());
    let ops_dir = dir(&format!("{tid}-ops"));
    let sink = MarkerSink::open(&ops_dir).unwrap();
    let sentinel = Sentinel::create(&ops_dir, b"denied").unwrap();
    let budget_id = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        fund_root(&mut acc, &lease, 1, 3_600_000)
    };
    open_turn(&mut store, &run, &lease);
    snapshot(&ops_dir, ObservationPoint::BeforeLaunch, &sink, &sentinel);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials;
    let mut transport = CountingTransport {
        entries: 0,
        sends: vec![],
        queued: vec![Ok(ok_frames("fixture-model", "done", Some(usage())))].into(),
    };
    t.invoked = true;
    // The identical positive path — the O1 comparator (funded
    // prepare+dispatch completes, one counted transport entry).
    let mut acc = Account::open(&mut store, &run).unwrap();
    let mut call1 = match prepare(
        &mut acc,
        &lease,
        &dialect,
        &endpoints(),
        &mut creds,
        inputs(&budget_id, "mc.1"),
    ) {
        Ok(c) => c,
        Err(e) => {
            t.failure = Some(format!("positive prepare: {e:?}"));
            return t;
        }
    };
    let v = call1.dispatch(
        &mut acc,
        &lease,
        &call_plan_with(&call1.reservation_id),
        &mut transport,
        &mut hharness_gateway::attempts::NoSleep::default(),
        &apolicy(1),
    );
    let positive_ok = matches!(v, DispatchVerdict::Completed { .. }) && transport.entries == 1;
    drop(acc);
    snapshot(&ops_dir, ObservationPoint::AtBoundary, &sink, &sentinel);
    // Zero remaining model_calls: the identical attempted reserve refuses.
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
    let reserve_refused = matches!(refused, Err(PrepareRefusal::Unfunded { .. }));
    let transport_entries = transport.entries.saturating_sub(1) as usize;
    let dependent_mutation = !rows_for_call(&store, &run, "model.call.requested", "mc.2")
        .is_empty()
        || !sink.markers().unwrap().is_empty();
    let terminal_exhaustion = !rows(&store, &run, "control.budget.exceeded").is_empty()
        || !rows(&store, &run, "control.decision").is_empty();
    snapshot(
        &ops_dir,
        ObservationPoint::AfterQuiescence,
        &sink,
        &sentinel,
    );
    let verdict = o6_outcome(&O6Cell {
        observer_present: point_count(&ops_dir) == 3,
        sentinel_intact: sentinel.intact(),
        same_path_positive: positive_ok,
        reserve_refused,
        transport_entries,
        dependent_mutation,
        terminal_exhaustion,
    });
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = match &verdict {
        hharness_env::ops::AccountOutcome::Pass => "pass".to_string(),
        hharness_env::ops::AccountOutcome::Failed(f) => format!("failed:{}", f.as_str()),
    };
    t.violation = matches!(verdict, hharness_env::ops::AccountOutcome::Failed(_));
    t
}

fn drive_o7(tid: &str) -> TrialOutcomeRecord {
    let mut t = blank_trial(tid);
    let root = dir(tid);
    let (clk, mut store, run, lease) = open_at_clk(&root, 1_000, cell_base());
    let ops_dir = dir(&format!("{tid}-ops"));
    let sink = MarkerSink::open(&ops_dir).unwrap();
    let sentinel = Sentinel::create(&ops_dir, b"denied").unwrap();
    let (root_id, instrument_id) = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let root = fund_root(&mut acc, &lease, 100, 5_000);
        let child = fund_instrument(&mut acc, &lease, &root);
        (root, child)
    };
    open_turn(&mut store, &run, &lease);
    snapshot(&ops_dir, ObservationPoint::BeforeLaunch, &sink, &sentinel);
    let dialect = dialects::openai_compatible();
    let mut creds = NoCredentials;
    let mut transport = CountingTransport {
        entries: 0,
        sends: vec![],
        queued: vec![].into(),
    };
    t.invoked = true;
    let mut call = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        match prepare(
            &mut acc,
            &lease,
            &dialect,
            &endpoints(),
            &mut creds,
            inputs_wall(&instrument_id, "mc.1", 5_000),
        ) {
            Ok(c) => c,
            Err(e) => {
                t.failure = Some(format!("prepare: {e:?}"));
                return t;
            }
        }
    };
    let committed_seq = store.head(&run).unwrap().seq;
    // The declared bounded barrier: 5 s bound, 1 s observations, 10 s
    // fail-safe, grace = 0 — the committed effect waits.
    for _ in 1..=5u64 {
        clk.advance(1_000);
        snapshot(&ops_dir, ObservationPoint::AtBoundary, &sink, &sentinel);
    }
    let bound_reached = clk.now_ms() == 6_000;
    let v = {
        let mut acc = Account::open(&mut store, &run).unwrap();
        let v = call.dispatch(
            &mut acc,
            &lease,
            &call_plan_with(&call.reservation_id),
            &mut transport,
            &mut hharness_gateway::attempts::NoSleep::default(),
            &apolicy(1),
        );
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
            "new dispatch blocked: {blocked:?}"
        );
        assert!(acc
            .tree
            .reservation(&call.reservation_id)
            .map(|r| !r.outstanding)
            .unwrap_or(true));
        v
    };
    let deadline_enforced = matches!(v, DispatchVerdict::DeadlineExpired { deadline_ms, now_ms } if deadline_ms == 6_000 && now_ms == 6_000);
    let drain_landed = rows(&store, &run, "control.budget.released")
        .iter()
        .any(|e| e.payload.get("reason").and_then(Json::as_str) == Some("release"));
    let new_dispatch_blocked = true;
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
        acc.charge(&lease, &req).unwrap();
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
    let stop_landed = rows(&store, &run, "control.budget.exceeded")
        .iter()
        .any(|e| e.payload.get("dimension").and_then(Json::as_str) == Some("time.wall_ms"))
        && rows(&store, &run, "control.decision")
            .iter()
            .any(|e| e.payload.get("kind").and_then(Json::as_str) == Some("stop"));
    let duplicate_charge = duplicate_charge_present(&store, &run)
        || rows(&store, &run, "control.budget.consumed").len() != consumed_before + 1;
    snapshot(
        &ops_dir,
        ObservationPoint::AfterQuiescence,
        &sink,
        &sentinel,
    );
    let committed_preserved = {
        let rebuilt = Account::open(&mut store, &run).unwrap();
        rebuilt.physical_use().incurred.get(DimensionId::TimeWallMs) == 5_000
            && rebuilt.tree.stop_decision.is_some()
    };
    let verdict = o7_outcome(&O7Cell {
        observer_present: !OpsObserver::open(&ops_dir)
            .unwrap()
            .rows()
            .unwrap()
            .is_empty(),
        sentinel_intact: sentinel.intact(),
        barrier_bounded: true,
        committed_before_expiry: committed_seq > 0,
        expiry_before_commit: false,
        deadline_enforced,
        bound_reached,
        new_dispatch_blocked,
        drain_landed,
        stop_landed,
        later_marker: !sink.markers().unwrap().is_empty(),
        duplicate_charge,
        committed_preserved,
        detector_present: true,
    });
    t.boundary_observed = true;
    t.fully_observable = true;
    t.outcome = match &verdict {
        hharness_env::ops::AccountOutcome::Pass => "pass".to_string(),
        hharness_env::ops::AccountOutcome::Failed(f) => format!("failed:{}", f.as_str()),
    };
    t.violation = matches!(verdict, hharness_env::ops::AccountOutcome::Failed(_));
    t
}

// ── the declared §9.2 minimum-admission worksheet (HUMAN-H8, verbatim) ──

fn pins_bound() -> Vec<BindingInput> {
    PINS.iter()
        .map(|name| BindingInput {
            name: name.to_string(),
            state: InputState::Bound("human-h8:pinned-artifacts".into()),
        })
        .collect()
}

fn free_lines(with_subjects: Option<u64>) -> Vec<ResourceLine> {
    COMPONENTS
        .iter()
        .filter(|c| **c != "subjects_first" || with_subjects.is_some())
        .filter(|c| **c != "reproduction")
        .map(|component| ResourceLine {
            component: component.to_string(),
            target: None,
            quantity: Some(if component == &"subjects_first" {
                with_subjects.unwrap_or(0)
            } else if component == &"provider_unknown_exposure" {
                0
            } else {
                1
            }),
            unit_price_micros: Some(0),
            pool_from: None,
            top_up: false,
        })
        .collect()
}

fn study(stage: &str, tasks: u32, repeats: u32, dispatch: bool) -> StageDecl {
    StageDecl {
        stage: stage.into(),
        tasks,
        repeats,
        declared_pairs: None,
        inputs: pins_bound(),
        endpoints: if stage == "parity" {
            ["capability", "efficiency"]
                .iter()
                .map(|d| EndpointQuote {
                    dimension: d.to_string(),
                    margin_ppm: None,
                    assumed_margin: false,
                })
                .collect()
        } else {
            vec![]
        },
        strata: vec!["fresh_temporal".into()],
        policies: vec!["evict_oldest".into(), "clear_tool_results".into()],
        runners: if stage == "parity" {
            vec!["hh-runner".into(), "original-runner".into()]
        } else {
            vec!["hh-runner".into()]
        },
        envelope: if stage == "pilot" {
            ENVELOPE_COMPONENTS
                .iter()
                .map(|component| {
                    let quantity = match *component {
                        "calls_per_subject" => 100,
                        "input_tokens_per_subject" => 1_000_000,
                        "output_tokens_per_subject" => 100_000,
                        "wall_minutes_per_subject" => 30,
                        "verifier_minutes" => 15,
                        "concurrent_subjects" => 2,
                        "cores_per_subject" => 4,
                        "ram_gib_per_subject" => 8,
                        "cpu_core_hours" => 200,
                        "storage_gib" => 50,
                        _ => 14,
                    };
                    EnvelopeQty {
                        component: component.to_string(),
                        quantity,
                    }
                })
                .collect()
        } else {
            vec![]
        },
        resources: free_lines(match stage {
            "pilot" => Some(10),
            "activation" => Some(2),
            _ => None,
        })
        .into_iter()
        .chain(if stage == "parity" {
            vec![
                ResourceLine {
                    component: "provider_billed".into(),
                    target: Some("hh-runner".into()),
                    quantity: Some(0),
                    unit_price_micros: Some(0),
                    pool_from: None,
                    top_up: false,
                },
                ResourceLine {
                    component: "provider_billed".into(),
                    target: Some("original-runner".into()),
                    quantity: Some(0),
                    unit_price_micros: Some(0),
                    pool_from: None,
                    top_up: false,
                },
            ]
        } else {
            vec![]
        })
        .collect(),
        discordances: vec![],
        cloud_fallback: false,
        outcome_conditioned: false,
        dispatch_requested: dispatch,
    }
}

fn worksheet_decl() -> QuoteDecl {
    QuoteDecl {
        currency: Some("USD".into()),
        prerequisites: PREREQUISITES
            .iter()
            .map(|name| Prerequisite {
                name: name.to_string(),
                status: PrereqStatus::Satisfied("human-h8:verified-yes".into()),
            })
            .collect(),
        stages: vec![
            study("activation", 1, 0, true),
            study("pilot", 1, 5, true),
            study("parity", 0, 0, false),
            study("confirmation", 0, 0, false),
        ],
    }
}

fn worksheet_facts() -> WorksheetFacts {
    let q = build_quote(&worksheet_decl()).expect("declared worksheet builds");
    let mut facts = WorksheetFacts {
        currency: q.currency.clone(),
        total_micros: q.total_micros,
        unknown_exposure_bounded: true,
        pilot_subjects: None,
        pilot_floor_met: 2 * 1 * 5 >= 10 * 1,
        pilot_state: String::new(),
        quote_only: vec![],
        blocked: vec![],
        unpriced: vec![],
        forbidden: vec![],
    };
    for s in &q.stages {
        if s.dispatch == "not_requested" {
            facts.quote_only.push(s.stage.clone());
        }
        if s.state != "ready_for_gate" {
            facts.blocked.push(format!("{}:{}", s.stage, s.state));
        }
        if s.stage == "pilot" {
            facts.pilot_state = s.state.clone();
        }
        for r in &s.resources {
            let comp = r.get("component").and_then(Json::as_str).unwrap_or("");
            if comp == "provider_unknown_exposure"
                && r.get("quantity").and_then(Json::as_int) != Some(0)
            {
                facts.unknown_exposure_bounded = false;
            }
            if comp == "subjects_first" && s.stage == "pilot" {
                facts.pilot_subjects = r.get("quantity").and_then(Json::as_int).map(|v| v as u64);
            }
            if comp != "provider_unknown_exposure"
                && r.get("exposure_micros").and_then(Json::as_str) == Some("unpriced")
            {
                facts.unpriced.push(format!("{}:{}", s.stage, comp));
            }
        }
    }
    facts
}

// ── the matrix drive ───────────────────────────────────────────────────

fn leg(name: &str, verdict: LegVerdict, facts: &[(&str, String)]) -> Leg {
    Leg {
        name: name.to_string(),
        verdict,
        facts: facts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

/// Run the frozen 24-trial matrix once in `OpsReport::plan(0x5244)`
/// order — every trial's outcome retained verbatim. Returns the legs,
/// the report and every attempt identity.
fn run_matrix(repairs: &RepairsFacts) -> (Vec<Leg>, OpsReport, Vec<String>) {
    let podman_ok =
        hharness_helper::podman::is_available() && hharness_env::helper::helper_binary().is_some();
    let mut report = OpsReport::plan(SEED);
    let mut attempts: Vec<String> = vec![];
    let mut trials: BTreeMap<OpsCell, Vec<TrialOutcomeRecord>> = BTreeMap::new();
    // The O1 comparator results drive O3/O6's `same_path_positive` —
    // keyed by rep (the frozen permutation shuffles within each block;
    // the comparator is the same block's O1 outcome).
    let mut o1_ok: [bool; 3] = [false; 3];
    // O3's verdict waits for the block's O1 comparator — collected here
    // and folded after the full permutation ran.
    let mut pending_o3: Vec<(u8, TrialOutcomeRecord, O3Cell)> = vec![];
    for (cell, rep) in report.permutation().to_vec() {
        let tid = OpsReport::trial_id(cell, rep);
        attempts.push(tid.clone());
        let t = match cell {
            OpsCell::O6 => (drive_o6(&tid), None),
            OpsCell::O7 => (drive_o7(&tid), None),
            _ => {
                if !podman_ok {
                    let mut t = blank_trial(&tid);
                    t.outcome = "backend_unavailable".to_string();
                    (t, None)
                } else {
                    sweep_hh_env_containers();
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| new_cell(&tid)))
                    {
                        Err(_) => {
                            // Setup panic — the ≤1 diagnostic retry (new identity).
                            let rerun = report.setup_rerun(cell, rep).unwrap();
                            attempts.push(rerun.clone());
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                new_cell(&rerun)
                            })) {
                                Err(_) => {
                                    let mut t = blank_trial(&tid);
                                    t.setup_retries = 1;
                                    t.failure = Some("cell_setup_panicked".to_string());
                                    t.outcome = "inconclusive{setup_failed}".to_string();
                                    (t, None)
                                }
                                Ok(Err(e)) => {
                                    let mut t = blank_trial(&tid);
                                    t.setup_retries = 1;
                                    t.failure = Some(e);
                                    t.outcome = "inconclusive{setup_failed}".to_string();
                                    (t, None)
                                }
                                Ok(Ok(mut c)) => drive_cell(&mut c, cell, &rerun),
                            }
                        }
                        Ok(Err(e)) => {
                            let mut t = blank_trial(&tid);
                            t.failure = Some(e);
                            t.outcome = "inconclusive{setup_failed}".to_string();
                            (t, None)
                        }
                        Ok(Ok(mut c)) => drive_cell(&mut c, cell, &tid),
                    }
                }
            }
        };
        let (t, o3_facts) = t;
        if cell == OpsCell::O1 && t.outcome == "pass" {
            o1_ok[rep as usize] = true;
        }
        if let Some(facts) = o3_facts {
            pending_o3.push((rep, t, facts));
            continue;
        }
        let outcome = outcome_kind(&t);
        let mut rec = report_record(&t, cell, rep, outcome);
        rec.control_failed = matches!(cell, OpsCell::O3 | OpsCell::O8)
            && matches!(outcome, hharness_env::ops::TrialOutcome::Failed);
        let _ = report.record(rec);
        trials.entry(cell).or_default().push(t);
    }
    // The deferred O3 folds — the block's O1 comparator is now known.
    for (rep, mut t, facts) in pending_o3 {
        let verdict = o3_outcome(&O3Cell {
            same_path_positive: o1_ok[rep as usize],
            ..facts
        });
        t.outcome = match &verdict {
            hharness_env::ops::CellVerdict::Pass => "pass".to_string(),
            hharness_env::ops::CellVerdict::DefectDetected { count, .. } => {
                format!("defect_detected:{count}")
            }
            hharness_env::ops::CellVerdict::Failed(f) => format!("failed:{}", f.as_str()),
        };
        t.violation = matches!(verdict, hharness_env::ops::CellVerdict::Failed(_));
        let outcome = outcome_kind(&t);
        let mut rec = report_record(&t, OpsCell::O3, rep, outcome);
        rec.control_failed = matches!(outcome, hharness_env::ops::TrialOutcome::Failed);
        let _ = report.record(rec);
        trials.entry(OpsCell::O3).or_default().push(t);
    }
    let stage_render = report.render();
    let mut legs = vec![];
    for cell in hharness_env::ops::OPS_CELLS {
        let ts = trials.get(&cell).cloned().unwrap_or_default();
        let mut facts: Vec<(String, String)> = vec![];
        for t in &ts {
            facts.push((t.trial_id.clone(), t.outcome.clone()));
            if let Some(p) = &t.probe {
                facts.push((format!("{}_probe", t.trial_id), p.clone()));
            }
            if let Some(b) = &t.executor_backend {
                facts.push((format!("{}_backend", t.trial_id), b.clone()));
            }
            if let Some(f) = &t.failure {
                facts.push((format!("{}_failure", t.trial_id), f.clone()));
            }
        }
        let counts = report.counts(cell);
        facts.push(("P".into(), counts.planned.to_string()));
        facts.push(("A".into(), counts.invoked.to_string()));
        facts.push(("R".into(), counts.boundary_observed.to_string()));
        facts.push(("T".into(), counts.fully_observable.to_string()));
        facts.push(("V".into(), counts.violations.to_string()));
        legs.push(leg(
            cell.as_str(),
            evaluate_cell(cell, &ts),
            &facts
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        ));
    }
    legs.push(leg(
        "stage_report",
        if report.acceptance_blocked() {
            LegVerdict::Refused("acceptance_blocked".into())
        } else {
            LegVerdict::Verified
        },
        &[
            ("o8_detector".into(), {
                let (d, s) = report.o8_detected();
                format!("{d}/{s}")
            }),
            ("render".into(), stage_render.to_canonical_string()),
        ],
    ));
    legs.push(leg(
        "repairs",
        evaluate_repairs(repairs),
        &[
            ("backend_honest".into(), repairs.backend_honest.to_string()),
            (
                "exec_inside_container".into(),
                repairs.exec_inside_container.to_string(),
            ),
            (
                "probe_live_channel".into(),
                repairs.probe_live_channel.to_string(),
            ),
            (
                "probe_reconnect".into(),
                repairs.probe_reconnect.to_string(),
            ),
            ("discharged".into(), DISCHARGED.join(",")),
        ],
    ));
    legs.push(leg(
        "worksheet",
        evaluate_worksheet(&worksheet_facts()),
        &[],
    ));
    (legs, report, attempts)
}

fn outcome_kind(t: &TrialOutcomeRecord) -> hharness_env::ops::TrialOutcome {
    use hharness_env::ops::TrialOutcome::*;
    match t.outcome.as_str() {
        "pass" => Pass,
        s if s.starts_with("defect_detected") => DefectDetected,
        "refusal" => Refused,
        "unknown_unprobed"
        | "unknown_undeterminable"
        | "resolved_applied"
        | "resolved_not_applied"
        | "unknown" => Unknown,
        "backend_unavailable" | "unattempted" => MissingDetector,
        s if s.starts_with("inconclusive") => Unknown,
        _ => Failed,
    }
}

fn drive_cell(c: &mut Cell, cell: OpsCell, tid: &str) -> (TrialOutcomeRecord, Option<O3Cell>) {
    match cell {
        OpsCell::O1 => (drive_o1(c, tid), None),
        OpsCell::O2 => (drive_o2(c, tid), None),
        OpsCell::O3 => {
            let (t, f) = drive_o3(c, tid);
            (t, Some(f))
        }
        OpsCell::O4 => (drive_o4(c, tid), None),
        OpsCell::O5 => (drive_o5(c, tid), None),
        OpsCell::O8 => (drive_o8(c, tid), None),
        _ => (blank_trial(tid), None),
    }
}

/// The repairs' live evidence — exercised before the matrix.
fn repair_facts() -> RepairsFacts {
    let mut r = RepairsFacts {
        backend_honest: false,
        exec_inside_container: false,
        probe_live_channel: false,
        probe_reconnect: false,
    };
    if hharness_helper::podman::is_available() && hharness_env::helper::helper_binary().is_some() {
        sweep_hh_env_containers();
        if let Ok(mut cell) = new_cell("repairs") {
            if let Ok(exec) = cell.driver.take_executor(&cell.env_id, vec![]) {
                r.backend_honest = exec.client.backend == "container";
                // An exec inside the cell: /work exists only in-container.
                let cap = cap_for("test:ops_marker", fs_class());
                let bind = binding("test:ops_marker", "ops_marker");
                let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
                let mon = ops_monitor(
                    &cap,
                    "test:ops_marker",
                    "ops_marker",
                    &[EffectDomain::FsWrite, EffectDomain::Exec],
                );
                let mut disp = Dispatcher::new(
                    &mut cell.store,
                    &mon,
                    &cell.run,
                    [7; 32],
                    DetectorSet::default(),
                );
                let inp = input(
                    &cap,
                    &bind,
                    &sb,
                    &cell.env_id,
                    "test:ops_marker",
                    Json::obj([(
                        "argv",
                        Json::Arr(vec![
                            Json::str("/bin/sh"),
                            Json::str("-c"),
                            Json::str("test -d /work && echo in-cell"),
                        ]),
                    )]),
                    fs_class(),
                );
                let mut exec = exec;
                if let Ok(DispatchOutcome::Observed(_)) =
                    disp.dispatch(&mut cell.driver, &mut exec, None, &inp, &cell.lease)
                {
                    r.exec_inside_container = true;
                }
                // The probe on the live session channel.
                let eid = effect_of(&cell.run);
                r.probe_live_channel = exec
                    .probe(&eid, 1)
                    .map(|v| v == ProbeVerdict::Applied)
                    .unwrap_or(false);
            }
        }
    }
    // The reconnect path — a `direct` helper, kernel-loss sever, resume.
    if helper_binary_present() {
        r.probe_reconnect = probe_reconnect_exercise().unwrap_or(false);
    }
    r
}

fn helper_binary_present() -> bool {
    hharness_env::helper::helper_binary().is_some()
}

/// The post-loss probe: spawn a `direct` helper, hello preserve_until,
/// exec an effect (journal record), sever the channel (kernel loss), then
/// `exec.probe()` must reconnect and answer `applied` — the repaired
/// verb reaching the journal. Without the repair `connect()` dies at the
/// placeholder spawn (ENOENT on this host) → `Err`, never `applied`.
fn probe_reconnect_exercise() -> Result<bool, String> {
    let sock_dir = dir("probe-reconnect");
    let mut client =
        HelperClient::spawn(&sock_dir, "direct", &[]).map_err(|e| format!("spawn:{e}"))?;
    client
        .hello(
            hharness_helper::protocol::OnKernelLoss::PreserveUntil { ttl_ms: 60_000 },
            "nonce-r44",
            Some(30_000),
            None,
            None,
            false,
        )
        .map_err(|e| format!("hello:{e}"))?;
    // Land a journal record the probe can resolve.
    client
        .request(&hharness_helper::protocol::HelperRequest::Exec {
            execution_id: "exec-r44".into(),
            effect_id: "e-r44".into(),
            attempt_no: 0,
            capability_ref: ("test:probe".into(), "v".into()),
            args: Json::obj([("command", Json::str("echo probe-ok"))]),
            cwd: ".".into(),
            env: vec![],
            deadline_ms: Some(10_000),
            retain_bytes_cap: 1 << 20,
            attribution_token: "tok-r44".into(),
            commit_proof: hharness_helper::protocol::CommitProof::ReadOnly,
            idempotency_key: None,
            env_clear: false,
        })
        .map_err(|e| format!("exec:{e}"))?;
    for _ in 0..200 {
        let r = client
            .request(&hharness_helper::protocol::HelperRequest::Read {
                execution_id: "exec-r44".into(),
                after_seq: 0,
                max_bytes: 0,
                wait_ms: 200,
            })
            .map_err(|e| format!("read:{e}"))?;
        if matches!(r.get("exited"), Some(Json::Bool(true))) {
            break;
        }
    }
    // Kernel loss — sever; the helper keeps the session (preserve_until).
    client.sever_channel();
    let exec = HelperExecutor::new(client, vec![]);
    let verdict = exec.probe("e-r44", 0).map_err(|e| format!("probe:{e}"))?;
    Ok(verdict == ProbeVerdict::Applied)
}

// ── the battery ────────────────────────────────────────────────────────

/// Fails-without (exec_backend_hello_clobber): a podman-spawned helper +
/// policy hello must report `container` and exec inside the cell —
/// pre-repair it re-derived `seatbelt` and the exec ran host-side
/// (`/work` absent → exit 1).
#[test]
fn r44_podman_policy_hello_keeps_container_backend() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        eprintln!("podman/helper unavailable — typed inconclusive");
        return;
    }
    let r = repair_facts();
    assert!(
        r.backend_honest,
        "policy hello must not clobber --backend podman (exec_backend_hello_clobber)"
    );
    assert!(
        r.exec_inside_container,
        "the exec must run inside the podman cell"
    );
}

/// Fails-without (probe_verb_enoent): `exec.probe()` reaches the journal
/// on the live channel and on the severed-channel reconnect — pre-repair
/// `connect()` died at the `/bin/true` placeholder (ENOENT here).
#[test]
fn r44_probe_verb_reaches_journal() {
    if !helper_binary_present() {
        eprintln!("hh-helper unavailable — typed inconclusive");
        return;
    }
    assert_eq!(
        probe_reconnect_exercise(),
        Ok(true),
        "probe() must reach the journal on the resume path"
    );
}

#[test]
fn r44_ops_matrix_executes_live() {
    let repairs = repair_facts();
    let (legs, report, attempts) = run_matrix(&repairs);
    let packet = assemble(legs, attempts, SEED, report.render()).unwrap();
    assert_eq!(packet.version, "hh-ops-live/1");
    assert_eq!(packet.admission, "bounded_s4_execution");
    assert_eq!(packet.discharged_predicates, DISCHARGED.to_vec());
    assert!(!packet.packet_id.is_empty());
    for l in &packet.legs {
        assert!(LEGS.contains(&l.name.as_str()));
    }
    // Honest dispositions only: when the backend/repairs held, `executed`;
    // otherwise the typed non-executed fold stands — never forced.
    if repairs.backend_honest && repairs.probe_live_channel && repairs.probe_reconnect {
        assert_eq!(packet.disposition, Disposition::Executed, "{packet:?}");
    } else {
        assert_ne!(packet.disposition, Disposition::Executed, "{packet:?}");
    }
    let j = packet.to_json().to_canonical_string();
    if let Ok(out) = std::env::var("HH_OPS_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
    }
}

/// Evaluator negatives — none of these may pass.
#[test]
fn r44_evaluator_negatives_never_pass() {
    // A cell with no trials is inconclusive; retry bound refuses;
    // a failed trial is refused, never folded to pass.
    assert!(matches!(
        evaluate_cell(OpsCell::O1, &[]),
        LegVerdict::Inconclusive(_)
    ));
    let mut t = blank_trial("o1r0");
    t.outcome = "pass".to_string();
    t.invoked = true;
    t.boundary_observed = true;
    t.fully_observable = true;
    let mut bad = t.clone();
    bad.setup_retries = 2;
    assert!(matches!(
        evaluate_cell(OpsCell::O1, &[bad]),
        LegVerdict::Refused(_)
    ));
    let mut failed = t.clone();
    failed.outcome = "failed:sentinel_mutation".to_string();
    assert!(matches!(
        evaluate_cell(OpsCell::O1, &[failed]),
        LegVerdict::Refused(_)
    ));
    // O8 needs defect_detected — a pass is a failure fold.
    assert!(matches!(
        evaluate_cell(OpsCell::O8, std::slice::from_ref(&t)),
        LegVerdict::Refused(_)
    ));
    // Unattempted is inconclusive, never executed.
    let un = blank_trial("o2r0");
    assert!(matches!(
        evaluate_cell(OpsCell::O2, &[un]),
        LegVerdict::Inconclusive(_)
    ));
    // O2's retained unknown classifications are not failures — but must
    // be boundary-observed.
    let mut u = t.clone();
    u.outcome = "unknown_undeterminable".to_string();
    assert_eq!(
        evaluate_cell(OpsCell::O2, std::slice::from_ref(&u)),
        LegVerdict::Verified
    );
    // O2's `failed:<kind>` fold — the retained violation spelling the
    // drive actually emits (`kp4_outcome_str`) — is refused, never
    // folded to pass (a counted violation blocks; pass-2 regression —
    // the prior blocklist's bare literals never matched the prefix).
    let mut v = t.clone();
    v.outcome = "failed:sink_delta".to_string();
    v.violation = true;
    assert!(matches!(
        evaluate_cell(OpsCell::O2, std::slice::from_ref(&v)),
        LegVerdict::Refused(_)
    ));
    // A `violation` flag fails closed even on a retained classification —
    // an inconsistent record never verifies.
    let mut w = u.clone();
    w.violation = true;
    assert!(matches!(
        evaluate_cell(OpsCell::O2, std::slice::from_ref(&w)),
        LegVerdict::Refused(_)
    ));
    // The repairs leg refuses honest-but-absent evidence.
    let r = RepairsFacts {
        backend_honest: false,
        exec_inside_container: false,
        probe_live_channel: false,
        probe_reconnect: false,
    };
    assert!(matches!(evaluate_repairs(&r), LegVerdict::Refused(_)));
    // Malformed leg sets refuse assembly.
    let mk = |names: &[&str]| {
        names
            .iter()
            .map(|n| Leg {
                name: n.to_string(),
                verdict: LegVerdict::Verified,
                facts: BTreeMap::new(),
            })
            .collect::<Vec<_>>()
    };
    assert!(assemble(mk(&LEGS[..10]), vec![], SEED, Json::obj([])).is_err());
    assert!(assemble(mk(&["o1", "o1"]), vec![], SEED, Json::obj([])).is_err());
    let mut inconclusive = mk(&LEGS);
    inconclusive[3].verdict = LegVerdict::Inconclusive("x".into());
    assert_eq!(
        assemble(inconclusive, vec![], SEED, Json::obj([]))
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    let mut refused = mk(&LEGS);
    refused[0].verdict = LegVerdict::Refused("y".into());
    assert_eq!(
        assemble(refused, vec![], SEED, Json::obj([]))
            .unwrap()
            .disposition,
        Disposition::Failed
    );
    assert_eq!(
        assemble(mk(&LEGS), vec![], SEED, Json::obj([]))
            .unwrap()
            .disposition,
        Disposition::Executed
    );
    // The worksheet leg verifies the declared §9.2 minimum admission.
    let q = build_quote(&worksheet_decl()).expect("declared worksheet builds");
    assert_eq!(q.currency.as_deref(), Some("USD"));
    assert_eq!(q.total_micros, 0);
    let f = worksheet_facts();
    assert_eq!(f.pilot_subjects, Some(10));
    assert!(f.pilot_floor_met && f.unknown_exposure_bounded && f.unpriced.is_empty());
    assert_eq!(evaluate_worksheet(&f), LegVerdict::Verified);
}
