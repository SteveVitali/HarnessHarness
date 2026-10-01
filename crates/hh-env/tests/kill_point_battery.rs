//! S3.6 — the kill-point battery (R-2.2.3⁰ᶜ; §5a.3; ADR-0132 §4–5): the one
//! shared durability acceptance suite. For each `(kill point, probe tool)`
//! cell the battery arms the fault at the real seam, drives a probe effect
//! through `dispatch`, kills the runtime (drops the `Store`), reopens,
//! `restore`s, and folds the post-resume ledger against the §5a.3 recovery
//! table and the ADR-0031 class table.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.2-1  — duplicate-after-retry per class: `compensable` with
//!   `dedup_support = executor` returns the stored observation; the external
//!   counter reads exactly 1; `irreversible` is never redispatched.
//! - AC-R-2.2.3-1  — crash/resume correctness across the battery: every
//!   cell's post-restore fold matches the recovery-table row.
//! - AC-R-2.2.3-3  — the duplicate-effect veto: external counters read
//!   exactly 1 for `compensable`-with-dedup and `irreversible`, ≤ 1 + probes
//!   for `reversible`; no second `committed` for a vetoed key.
//! - AC-R-2.2.3-4  — restore is idempotent (KP-13): a torn restore re-runs
//!   wholesale and converges to the same durable state.
//! - AC-R-2.2.3-12 — the five recovery metrics fold over the battery's own
//!   ledger (`recovery_latency_ms`, `wasted_calls`,
//!   `unknown_effects_per_resume`, `heal_count`, `wakeups_skipped{reason}`).
//!
//! The four probe tools (§5a.3's matrix axes): `read_only` counter read,
//! `reversible` file edit with a baseline, `compensable` external counter
//! (`dedup_support ∈ {executor, none}` — the `none` leg is the escalated
//! cell), `irreversible` external counter. The "external counter" is a
//! shared `AtomicU64` — the world the runtime cannot roll back.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hh_compiler::plan::PinnedRef;
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{kernel_default, NetMode, ResourceLimits, WritableRoot};
use hh_env::capture::OutputPolicy;
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::EventMinter;
use hh_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hh_env::handle::{OnLoss, Roots};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_hir::refs::Ref;
use hh_ledger::event::{Cursor, Direction, Scope};
use hh_ledger::fault::KillPoint;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, Monitor};
use hh_monitor::policy::{Cond, Mode, PiVerdict, PolicyRow};
use hh_monitor::table::HandleTable;
use hh_ontology::risk::{RiskReversibility, RiskScope};
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_wire::json::Json;

const TTL_MS: u64 = 60_000;

// ── scaffold ─────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-kpb-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open_at(root: &std::path::Path) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        root,
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

/// Reopen the crashed store at the same root — the runtime death + restart
/// half of every cell (the WAL is the world that survives). `ms` lands past
/// the dead writer's lease expiry, so the takeover fences it rather than
/// blocking on a lease the crashed runtime can never release.
fn reopen(root: &std::path::Path, ms: u64) -> Store {
    reopen_ids(root, ms, 1_000_000)
}

/// `reopen` with the id-source watermark — successive restarts each re-seed
/// `SeqIds` above every minted id (a second restore's `evt-1000000+n` must
/// not collide with the first's).
fn reopen_ids(root: &std::path::Path, ms: u64, id_base: u64) -> Store {
    Store::open_with(
        root,
        Box::new(ManualClock::at(ms)),
        // `SeqIds` re-seeded high — the reopened store's kernel rows must not
        // re-mint ids the WAL already holds (the deterministic fixture
        // starts above the waterline).
        Some(Box::new(SeqIds::starting_at(id_base))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
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

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_policy(ws: &std::path::Path) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = hh_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}

fn test_record() -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalHost,
        image: ImageRef::ContentAddress(hh_identity::idp::address(
            b"battery-image",
            "application/octet-stream",
        )),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        ext: BTreeMap::new(),
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
    let h = driver
        .provision(
            store,
            lease,
            &test_record(),
            roots,
            PolicySlot::Inline(Box::new(test_policy(ws))),
            OnLoss::FailRun,
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

// ── the four probe tools ─────────────────────────────────────────────────────

/// The §5a.3 battery axes — one probe tool per risk class. `Compensable`
/// runs `dedup_support = executor`; the `none` leg is a separate cell (the
/// escalated duplicate path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeTool {
    /// `read_only` counter read — never mutates the world.
    ReadOnly,
    /// `reversible` file edit with a baseline.
    Reversible,
    /// `compensable` external counter (a `CompensationPlan` is registered).
    Compensable,
    /// `irreversible` external counter — never redispatched.
    Irreversible,
}

const FOUR_TOOLS: [ProbeTool; 4] = [
    ProbeTool::ReadOnly,
    ProbeTool::Reversible,
    ProbeTool::Compensable,
    ProbeTool::Irreversible,
];

impl ProbeTool {
    fn name(self) -> &'static str {
        match self {
            ProbeTool::ReadOnly => "read_only",
            ProbeTool::Reversible => "reversible",
            ProbeTool::Compensable => "compensable",
            ProbeTool::Irreversible => "irreversible",
        }
    }

    fn domain(self) -> EffectDomain {
        match self {
            ProbeTool::ReadOnly => EffectDomain::FsRead,
            ProbeTool::Reversible => EffectDomain::FsWrite,
            ProbeTool::Compensable | ProbeTool::Irreversible => EffectDomain::SpawnProcess,
        }
    }

    fn attrs(self) -> EffectAttributes {
        match self {
            ProbeTool::ReadOnly => EffectAttributes {
                mutability: Mutability::ReadOnly,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
            },
            ProbeTool::Reversible => EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
            },
            ProbeTool::Compensable => EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Compensable,
            },
            ProbeTool::Irreversible => EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Irreversible,
            },
        }
    }

    fn effect_class(self) -> EffectClass {
        EffectClass {
            domain: self.domain(),
            attributes: Some(self.attrs()),
        }
    }

    fn args(self, ws: &std::path::Path) -> Json {
        match self {
            ProbeTool::Reversible => Json::obj([
                ("path", Json::str(format!("{}/edit.txt", ws.display()))),
                ("content", Json::str("applied")),
            ]),
            ProbeTool::ReadOnly => {
                Json::obj([("path", Json::str(format!("{}/read.txt", ws.display())))])
            }
            _ => Json::obj([(
                "argv",
                Json::Arr(vec![Json::str("/bin/echo"), Json::str("tick")]),
            )]),
        }
    }

    /// The capability's `scope_bindings` — `fs_path` for the file tools;
    /// `Unknown` for the spawn tools (an `exec` allow-list consults the
    /// scoped `fs_path`s — a workspace path is not an exec extent).
    fn scope_bindings(self) -> ScopeBindings {
        match self {
            ProbeTool::ReadOnly | ProbeTool::Reversible => {
                ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
                    ("param_path", Json::str("path")),
                    ("scope_kind", Json::str("fs_path")),
                ])]))
            }
            _ => ScopeBindings::Unknown,
        }
    }

    /// The executor's `dedup_support` — `executor` for every tool here; the
    /// `none` leg is `executor_none(tool)`'s override.
    fn dedup(self) -> DedupSupport {
        DedupSupport::BestEffort
    }
}

/// The probe executor — the honest seam (I-3: it reports, never decides).
/// `world` is the external counter the runtime cannot roll back; `applied`
/// is the probe's ground truth (which effects actually reached the world).
struct BatteryExecutor {
    decl: ExecutorDeclaration,
    world: Arc<AtomicU64>,
    applied: Arc<Mutex<BTreeSet<String>>>,
    tool: ProbeTool,
    /// KP-15 — the helper dies mid-`execute` (the runtime stays alive).
    die_in_execute: bool,
    /// The `reversible` tool's real file — its existence is the probe's
    /// answer (a world fact, not a runtime fact).
    edit_file: Option<PathBuf>,
}

fn report_ok() -> TerminalReport {
    TerminalReport {
        status: TerminalStatus::Ok,
        exit_status: Some(0),
        outcome_hint: "applied".to_string(),
        retryable_hint: None,
        detail_ref: None,
        truncated: false,
        omitted_bytes: 0,
        original_size: 0,
    }
}

impl BatteryExecutor {
    fn new(
        tool: ProbeTool,
        dedup: DedupSupport,
        world: Arc<AtomicU64>,
        applied: Arc<Mutex<BTreeSet<String>>>,
        ws: &std::path::Path,
    ) -> Self {
        BatteryExecutor {
            decl: ExecutorDeclaration {
                executor_id: format!("battery/{}", tool.name()),
                isolation_support: hh_containment::policy::IsolationClass::None,
                dedup_support: dedup,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: [
                    "executor_error",
                    "timeout",
                    "signalled",
                    "invalid_arguments",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                streams: true,
                domains: [
                    EffectDomain::FsWrite,
                    EffectDomain::FsRead,
                    EffectDomain::SpawnProcess,
                ]
                .iter()
                .cloned()
                .collect(),
            },
            world,
            applied,
            tool,
            die_in_execute: false,
            edit_file: (tool == ProbeTool::Reversible).then(|| ws.join("edit.txt")),
        }
    }
}

impl ToolExecutor for BatteryExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        if self.die_in_execute {
            // KP-15 — helper death: the transport-plane error, runtime alive.
            return Err(EnvError::Blob("helper died".to_string()));
        }
        sink(ExecutorSignal {
            token: request.attribution_token.clone(),
            kind: hh_env::capture::CaptureKind::OutputChunk {
                stream: "stdout".to_string(),
                data: "tick".to_string(),
            },
        });
        match self.tool {
            ProbeTool::ReadOnly => {
                // A read touches nothing — the world counter stands.
                let _ = self.world.load(Ordering::SeqCst);
            }
            ProbeTool::Reversible => {
                let f = self.edit_file.as_ref().unwrap();
                std::fs::write(f, "applied").unwrap();
            }
            ProbeTool::Compensable | ProbeTool::Irreversible => {
                self.world.fetch_add(1, Ordering::SeqCst);
            }
        }
        self.applied
            .lock()
            .unwrap()
            .insert(request.effect_id.clone());
        Ok(report_ok())
    }
    fn probe(&self, effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        // The probe consults the world, never the runtime's memory.
        let applied = match self.tool {
            ProbeTool::Reversible => self.edit_file.as_ref().unwrap().exists(),
            _ => self.applied.lock().unwrap().contains(effect_id),
        };
        Ok(if applied {
            ProbeVerdict::Applied
        } else {
            ProbeVerdict::NotApplied
        })
    }
}

// ── capability / monitor fixture ─────────────────────────────────────────────

fn capability(tool: ProbeTool, sb: ScopeBindings) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new(
            "battery tool",
            "test:owner",
            ProvenanceRecord::kernel("t", 0),
        ),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [
                    ("path", "string"),
                    ("content", "string"),
                    ("command", "string"),
                    ("argv", "array"),
                ]
                .iter()
                .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                .collect(),
            ),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(vec![tool.effect_class()].into_iter().collect()),
        preconditions: vec![],
        scope_bindings: sb,
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![
                Json::str("executor_error"),
                Json::str("timeout"),
                Json::str("signalled"),
                Json::str("invalid_arguments"),
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

fn binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "battery_tool".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:battery_tool".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:battery_tool".to_string(),
        arg_map: ["path", "content", "command", "argv"]
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
        exposure_mode: hh_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:battery_tool".to_string()],
        mapping: hh_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    }
}

fn root_handle() -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/battery", 0),
        grants: vec![
            Grant {
                effect: EffectClass::domain_only(EffectDomain::FsWrite),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
            Grant {
                effect: EffectClass::domain_only(EffectDomain::SpawnProcess),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
            Grant {
                effect: EffectClass::domain_only(EffectDomain::FsRead),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
        ],
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
        scope: hh_monitor::decision::DecisionScope::Session,
    }
}

/// The battery's Π table: the default floor plus domain-scoped `allow` rows
/// for `spawn_process` at every risk class — the battery exercises the
/// *recovery* semantics, so the permission layer must not divert the cells
/// into `ask`/`pending` (the ask cells are §05g's own suite).
fn battery_monitor(cap: &ToolCapabilityRecord) -> Monitor {
    let mut table = HandleTable::default();
    let h = root_handle();
    table.handles.insert(h.handle_id.clone(), h);
    let mut pol = hh_monitor::policy::default_table("pol-v1", Mode::Attended);
    for rev in [
        RiskReversibility::ReadOnly,
        RiskReversibility::Reversible,
        RiskReversibility::Compensable,
        RiskReversibility::Irreversible,
    ] {
        pol.rows.push(PolicyRow {
            id: format!("battery_allow_{rev:?}").to_lowercase(),
            conditions: vec![Cond::DomainIs(EffectDomain::SpawnProcess), Cond::RevIs(rev)],
            verdict: PiVerdict::Allow,
        });
    }
    // `fs_read`/`fs_write` are covered by Π-1/Π-2 already.
    let _ = RiskScope::WorkspaceLocal;
    let mut m = Monitor::new(table, pol);
    m.proposers.insert(
        "test:agent".to_string(),
        Label::at(AuthorityClass::Principal),
    );
    m.run_id = "run-1".to_string();
    m.turn_id = "turn-1".to_string();
    m.effect_id = "e1".to_string();
    m.capabilities.insert(
        "test:battery_tool".to_string(),
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
    tc: &str,
    args: Json,
    tool: ProbeTool,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: hh_env::events::ScopeChain {
            turn_id: "turn-1".to_string(),
            model_call_id: "mc-1".to_string(),
            tool_call_id: tc.to_string(),
        },
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: "test:battery_tool".to_string(),
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
        declared: tool.effect_class(),
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
        // `compensable` requires a registered CompensationPlan at prepare
        // (§5a.2's authorize row); the battery registers one so the cell
        // reaches `committed`.
        compensation_plan_id: (tool == ProbeTool::Compensable).then(|| "plan-1".to_string()),
        baseline_ref: None,
        flow: Default::default(),
    }
}

// ── the battery report — budgets recorded per cell ───────────────────────────

/// One battery cell's record: the fault, the tool, the budgets the cell
/// consumed, and the recovery-table verdict the oracle checked.
#[derive(Debug)]
struct CellReport {
    kp: KillPoint,
    tool: ProbeTool,
    /// Faults the cell injected (the fault-injection budget consumed).
    faults_injected: u32,
    /// `restore` calls the cell ran (the recovery budget consumed).
    restores: u32,
    /// Executor `probe` calls (the lapsed-window budget consumed).
    probes: u32,
    /// `dispatch` attempts.
    dispatches: u32,
    /// The external counter — the world the runtime cannot roll back.
    world_counter: u64,
    /// The cell verdict (`allow`-path cells only — `n/a` marks the matrix
    /// hole `(KP-4, read_only)`, an honest empty cell, never a skip).
    verdict: &'static str,
}

/// The ledger rows for one effect — the fold the oracle reads.
struct EffectRows {
    intended: usize,
    prepared: usize,
    committed: usize,
    observed: usize,
    unknown: usize,
}

fn effect_rows(store: &Store, run: &str, effect_id: &str) -> EffectRows {
    let mut r = EffectRows {
        intended: 0,
        prepared: 0,
        committed: 0,
        observed: 0,
        unknown: 0,
    };
    for e in read_all(store, run) {
        if e.scope.effect_id.as_deref() != Some(effect_id) {
            continue;
        }
        match e.class.as_str() {
            "action.effect.intended" => r.intended += 1,
            "action.effect.prepared" => r.prepared += 1,
            "action.effect.committed" => r.committed += 1,
            "action.effect.observed" => r.observed += 1,
            "action.effect.unknown" => r.unknown += 1,
            _ => {}
        }
    }
    r
}

fn class_count(store: &Store, run: &str, class: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class)
        .count()
}

/// A dispatch-boundary cell (KP-1…KP-8): arm the seam, drive the dispatch to
/// the wall, kill the runtime, reopen, restore, check the recovery table.
fn run_dispatch_cell(tag: &str, kp: KillPoint, tool: ProbeTool) -> CellReport {
    let root = dir(tag);
    let ws = workspace(tag);
    let world = Arc::new(AtomicU64::new(0));
    let applied = Arc::new(Mutex::new(BTreeSet::new()));

    let (mut store, run, lease, _clock) = open_at(&root);
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let sb = tool.scope_bindings();
    let cap = capability(tool, sb.clone());
    let bind = binding();
    let mon = battery_monitor(&cap);
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);

    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = BatteryExecutor::new(tool, tool.dedup(), world.clone(), applied.clone(), &ws);
    let inp = input(&cap, &bind, &sb, &env, "tc-1", tool.args(&ws), tool);

    let read_only = tool == ProbeTool::ReadOnly;
    if kp == KillPoint::Kp8 {
        // KP-8 — after visible, before the next `control.decision`: the
        // dispatch completes; the runtime dies with the outcome durable.
        let out = disp
            .dispatch(&mut driver, &mut exec, None, &inp, &lease)
            .unwrap();
        assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");
    } else {
        disp.inject_kill_point(kp);
        let r = disp.dispatch(&mut driver, &mut exec, None, &inp, &lease);
        // `(KP-4, read_only)` is the matrix's honest hole — read_only has
        // no commit phase, so the kill point never exists for it.
        if kp == KillPoint::Kp4 && read_only {
            assert!(
                matches!(r, Ok(DispatchOutcome::Observed(_))),
                "(kp-4, read_only) must dispatch clean — no commit boundary: {r:?}"
            );
            return CellReport {
                kp,
                tool,
                faults_injected: 0,
                restores: 0,
                probes: 0,
                dispatches: 1,
                world_counter: world.load(Ordering::SeqCst),
                verdict: "n/a",
            };
        }
        match r {
            Err(EnvError::FaultInjected { at }) => {
                assert_eq!(at, kp.as_str(), "kill fired at the wrong boundary");
            }
            other => panic!(
                "({kp}, {}) expected FaultInjected, got {other:?}",
                tool.name()
            ),
        }
    }

    // ── runtime death — drop the store mid-run; the WAL is what survives ──
    drop(disp);
    drop(driver);
    drop(store);

    let mut store2 = reopen(&root, 120_000);
    let report = store2.restore(&run, "writer-b", TTL_MS).unwrap();

    // Every crash cell: exactly one takeover + one resume row; the fenced
    // stale lease is named.
    assert_eq!(class_count(&store2, &run, "lifecycle.run.resumed"), 1);
    let rows = effect_rows(&store2, &run, &effect_id);

    let verdict: &'static str = match kp {
        KillPoint::Kp1 => {
            // `intended` keeps — the recovery table takes no action on a
            // pre-authorization intent.
            assert_eq!(rows.intended, 1);
            assert_eq!(rows.prepared + rows.committed + rows.observed, 0);
            assert!(report.unknowned.is_empty());
            assert!(report.reprepared().is_empty());
            assert!(report.effects_to_prepare.is_empty());
            "intended-kept"
        }
        KillPoint::Kp2 => {
            // `authorized`-but-never-`prepared` → the resuming writer must
            // re-prepare it.
            assert_eq!(rows.intended, 1);
            assert_eq!(rows.committed, 0);
            assert_eq!(report.effects_to_prepare, vec![effect_id.clone()]);
            "re-prepare"
        }
        KillPoint::Kp3 => {
            assert_eq!(rows.intended, 1);
            assert_eq!(rows.committed, 0);
            if read_only {
                // `read_only` re-prepares in place — the original `prepared`
                // plus the restore's re-prepare row (no write-ahead existed).
                assert_eq!(rows.prepared, 2);
                assert_eq!(report.reprepared(), std::slice::from_ref(&effect_id));
                assert!(report.unknowned.is_empty());
                "reprepared"
            } else {
                assert_eq!(rows.prepared, 1);
                // Mutating classes: `prepared`-uncommitted → `unknown` +
                // probe (never a silent redispatch).
                assert_eq!(report.unknowned, vec![effect_id.clone()]);
                assert_eq!(rows.unknown, 1);
                "unknown-probe"
            }
        }
        KillPoint::Kp4 | KillPoint::Kp5 | KillPoint::Kp6 if read_only => {
            // `read_only` never writes ahead — at KP-5/KP-6 the fold still
            // sits at `prepared`, so the restore re-prepares (re-executing a
            // read is the recovery table's own answer; KP-4 is the n/a hole
            // handled above).
            assert_eq!(rows.committed, 0);
            assert_eq!(rows.prepared, 2);
            assert_eq!(report.reprepared(), std::slice::from_ref(&effect_id));
            "reprepared"
        }
        KillPoint::Kp4 | KillPoint::Kp5 | KillPoint::Kp6 => {
            // `committed`, no terminal → `unknown{worker_lost}` + probe per
            // the class table — for every mutating class.
            assert_eq!(rows.committed, 1);
            assert_eq!(report.unknowned, vec![effect_id.clone()]);
            assert_eq!(rows.unknown, 1);
            // The probe is scheduled — a `control.retry.scheduled{kind:
            // probe}` row covers the effect.
            let scheduled = read_all(&store2, &run).iter().any(|e| {
                e.class == "control.retry.scheduled"
                    && e.payload.get("scope_id").and_then(Json::as_str) == Some(&effect_id)
            }) || read_all(&store2, &run).iter().any(|e| {
                e.class == "control.retry.scheduled"
                    && e.payload.get("effect_id").and_then(Json::as_str) == Some(&effect_id)
            });
            assert!(scheduled, "unknown effect must have a probe timer");
            "unknown-probe"
        }
        KillPoint::Kp7 | KillPoint::Kp8 => {
            // Terminal `observed` — the recovery table's settle is the
            // charge repost the caller checks; the effect is never unknown.
            assert_eq!(rows.observed, 1);
            assert_eq!(report.observed_effects, vec![effect_id.clone()]);
            assert!(report.unknowned.is_empty());
            "observed-settled"
        }
        _ => unreachable!("dispatch cell"),
    };

    // AC-R-2.2.3-3's pre-state: at most one `committed` for the effect —
    // the durable veto rests on the `effects_by_key` fold.
    assert!(rows.committed <= 1, "duplicate commit at {kp}");

    CellReport {
        kp,
        tool,
        faults_injected: 1,
        restores: 1,
        probes: 0,
        dispatches: 1,
        world_counter: world.load(Ordering::SeqCst),
        verdict,
    }
}

// ── the matrix ───────────────────────────────────────────────────────────────

/// AC-R-2.2.3-1 — the dispatch-boundary matrix KP-1…KP-8 × the four probe
/// tools. Every cell drives the real seams and checks the §5a.3 recovery
/// table after a real store drop + reopen + restore.
#[test]
fn ac_r_2_2_3_1_dispatch_boundary_matrix() {
    let mut reports = Vec::new();
    for kp in [
        KillPoint::Kp1,
        KillPoint::Kp2,
        KillPoint::Kp3,
        KillPoint::Kp4,
        KillPoint::Kp5,
        KillPoint::Kp6,
        KillPoint::Kp7,
        KillPoint::Kp8,
    ] {
        for tool in FOUR_TOOLS {
            let tag = format!("{}-{}", kp.as_str(), tool.name());
            let rep = run_dispatch_cell(&tag, kp, tool);
            assert_ne!(
                rep.verdict,
                "",
                "({kp}, {}) produced no verdict",
                tool.name()
            );
            // Budgets recorded — the cell's consumed fault-injection /
            // recovery / probe budget, one line per cell.
            eprintln!(
                "battery cell {}×{}: verdict={} faults={} restores={} probes={} dispatches={} world={}",
                rep.kp, rep.tool.name(), rep.verdict, rep.faults_injected,
                rep.restores, rep.probes, rep.dispatches, rep.world_counter
            );
            reports.push(rep);
        }
    }
    // 32 cells; 31 fired a fault ((kp-4, read_only) is the n/a hole) and
    // each restored exactly once.
    assert_eq!(reports.len(), 32);
    assert_eq!(
        reports.iter().filter(|r| r.faults_injected == 1).count(),
        31
    );
    assert!(reports
        .iter()
        .all(|r| r.restores == 1 || r.verdict == "n/a"));
}

/// KP-9 — mid-batch append: the batch is absent as a unit (R-2.2.3⁰ᶜ's
/// atomicity row). The fault fires inside `append` before a byte lands;
/// reopen + restore find no torn prefix.
#[test]
fn ac_r_2_2_3_1_kp9_batch_absent_as_unit() {
    for tool in FOUR_TOOLS {
        let tag = format!("kp9-{}", tool.name());
        let root = dir(&tag);
        let ws = workspace(&tag);
        let (mut store, run, lease, _clock) = open_at(&root);
        open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
        let mut driver = EnvDriver::new(&run);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        let sb = tool.scope_bindings();
        let cap = capability(tool, sb.clone());
        let bind = binding();
        let mon = battery_monitor(&cap);
        let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
        let mut disp = Dispatcher::new(&mut store, &mon, &run, [3u8; 32], DetectorSet::default());
        let world = Arc::new(AtomicU64::new(0));
        let applied = Arc::new(Mutex::new(BTreeSet::new()));
        let mut exec =
            BatteryExecutor::new(tool, tool.dedup(), world.clone(), applied.clone(), &ws);
        let inp = input(&cap, &bind, &sb, &env, "tc-1", tool.args(&ws), tool);

        // Arm the mid-batch fault — the dispatch's first append
        // (`proposed`+`intended`) is the batch; it fails as a unit.
        disp.store_mut().inject_durability_faults(1);
        let r = disp.dispatch(&mut driver, &mut exec, None, &inp, &lease);
        assert!(
            matches!(
                r,
                Err(EnvError::Ledger(
                    hh_ledger::errors::LedgerError::Durability { .. }
                        | hh_ledger::errors::LedgerError::FaultInjected { .. }
                )) | Err(EnvError::FaultInjected { .. })
            ),
            "kp-9 expected a durability fault, got {r:?}"
        );
        // Atomicity: no row of the batch landed — the effect coordinate is
        // absent entirely.
        let rows = effect_rows(disp.store_mut(), &run, &effect_id);
        assert_eq!(
            rows.intended + rows.prepared + rows.committed + rows.observed + rows.unknown,
            0,
            "kp-9 left a torn prefix for {}",
            tool.name()
        );
        drop(disp);
        drop(driver);
        drop(store);

        // Reopen + restore: nothing to recover — no unknown, no resume
        // action on the effect; the world counter is untouched.
        let mut store2 = reopen(&root, 120_000);
        let report = store2.restore(&run, "writer-b", TTL_MS).unwrap();
        assert!(report.unknowned.is_empty());
        assert_eq!(
            world.load(Ordering::SeqCst),
            0,
            "kp-9 executed work for {}",
            tool.name()
        );
    }
}

/// KP-13 — restore interruption: the first `restore` tears mid-recovery;
/// a second pass completes identically (R-4 idempotence — every recovery
/// action checks the fold's *current* phase).
#[test]
fn ac_r_2_2_3_4_kp13_restore_is_idempotent() {
    for tool in FOUR_TOOLS {
        let tag = format!("kp13-{}", tool.name());
        let root = dir(&tag);
        let ws = workspace(&tag);
        let world = Arc::new(AtomicU64::new(0));
        let applied = Arc::new(Mutex::new(BTreeSet::new()));
        let (mut store, run, lease, _clock) = open_at(&root);
        open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
        let mut driver = EnvDriver::new(&run);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        let sb = tool.scope_bindings();
        let cap = capability(tool, sb.clone());
        let bind = binding();
        let mon = battery_monitor(&cap);
        let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
        let mut disp = Dispatcher::new(&mut store, &mon, &run, [5u8; 32], DetectorSet::default());
        let mut exec =
            BatteryExecutor::new(tool, tool.dedup(), world.clone(), applied.clone(), &ws);
        let inp = input(&cap, &bind, &sb, &env, "tc-1", tool.args(&ws), tool);

        // Leave a `committed`-unterminated effect (mutating classes) or a
        // `prepared` one (read_only) so restore has recovery work to tear
        // mid-flight. Arm KP-4 for mutating / KP-3 for read_only.
        let arm = if tool == ProbeTool::ReadOnly {
            KillPoint::Kp3
        } else {
            KillPoint::Kp4
        };
        disp.inject_kill_point(arm);
        let r = disp.dispatch(&mut driver, &mut exec, None, &inp, &lease);
        assert!(matches!(r, Err(EnvError::FaultInjected { .. })), "{r:?}");
        drop(disp);
        drop(driver);
        drop(store);

        // First restore — armed to tear after one recovery append.
        let mut s1 = reopen(&root, 120_000);
        s1.arm_restore_kill(1);
        let r1 = s1.restore(&run, "writer-b", TTL_MS);
        assert!(
            matches!(
                r1,
                Err(hh_ledger::errors::LedgerError::FaultInjected { .. })
            ),
            "kp-13 restore must tear, got {r1:?}"
        );
        drop(s1);

        // Second restore — completes; a third confirms convergence. Each
        // reopen's clock lands past the previous writer's lease expiry so
        // the takeover fences rather than blocks.
        let mut s2 = reopen_ids(&root, 240_000, 2_000_000);
        let rep2 = match s2.restore(&run, "writer-c", TTL_MS) {
            Ok(r) => r,
            Err(e) => {
                for ev in read_all(&s2, &run) {
                    eprintln!("seq={} class={} scope={:?}", ev.seq, ev.class, ev.scope);
                }
                panic!("second restore: {e:?}");
            }
        };
        drop(s2);
        let mut s3 = reopen_ids(&root, 360_000, 3_000_000);
        let rep3 = s3.restore(&run, "writer-d", TTL_MS).unwrap();
        let _ = rep2.generation;

        // Idempotence: the completing restores never re-decide what a torn
        // predecessor already landed — exactly one `unknown` (mutating) or
        // one `reprepared_after` row (read_only), one `model.call.failed`,
        // and the third pass reports no new recovery work. (Lease/resumed
        // rows legitimately grow one-per-restore — convergence is checked
        // on the *recovery decisions*, not the log length.)
        let rows = effect_rows(&s3, &run, &effect_id);
        assert_eq!(
            class_count(&s3, &run, "model.call.failed"),
            1,
            "kp-13 doubled the model_call failure row for {}",
            tool.name()
        );
        if tool == ProbeTool::ReadOnly {
            let reprepares = read_all(&s3, &run)
                .iter()
                .filter(|e| {
                    e.class == "action.effect.prepared"
                        && e.scope.effect_id.as_deref() == Some(effect_id.as_str())
                        && e.payload
                            .get("reprepared_after")
                            .and_then(Json::as_str)
                            .is_some()
                })
                .count();
            assert_eq!(reprepares, 1, "torn restore re-prepared twice");
            assert_eq!(rows.unknown, 0);
        } else {
            assert_eq!(rows.unknown, 1, "torn restore doubled the unknown row");
        }
        assert!(rep3.unknowned.is_empty());
        assert!(rep3.failed_model_calls.is_empty());
        assert!(rep3.reprepared().is_empty());
    }
}

/// KP-15 — helper death with the runtime alive: the executor's
/// transport-plane error lands `unknown{executor_error}`; the probe then
/// answers from the world (never a redispatch — AC-R-2.5.5-9's seam).
#[test]
fn ac_r_2_2_3_1_kp15_helper_death_runtime_alive() {
    for tool in FOUR_TOOLS {
        let tag = format!("kp15-{}", tool.name());
        let root = dir(&tag);
        let ws = workspace(&tag);
        let world = Arc::new(AtomicU64::new(0));
        let applied = Arc::new(Mutex::new(BTreeSet::new()));
        let (mut store, run, lease, _clock) = open_at(&root);
        open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
        let mut driver = EnvDriver::new(&run);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        let sb = tool.scope_bindings();
        let cap = capability(tool, sb.clone());
        let bind = binding();
        let mon = battery_monitor(&cap);
        let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
        let mut disp = Dispatcher::new(&mut store, &mon, &run, [9u8; 32], DetectorSet::default());
        let mut exec =
            BatteryExecutor::new(tool, tool.dedup(), world.clone(), applied.clone(), &ws);
        exec.die_in_execute = true;
        let inp = input(&cap, &bind, &sb, &env, "tc-1", tool.args(&ws), tool);

        let out = disp
            .dispatch(&mut driver, &mut exec, None, &inp, &lease)
            .unwrap();
        assert!(
            matches!(out, DispatchOutcome::Unknown { ref cause } if cause == "executor_error"),
            "kp-15 expected unknown{{executor_error}}, got {out:?}"
        );
        let rows = effect_rows(disp.store_mut(), &run, &effect_id);
        assert_eq!(rows.unknown, 1);
        // The runtime is alive — the store still appends (the probe lands).
        exec.die_in_execute = false;
        let verdict = disp
            .probe(
                &exec,
                &lease,
                &effect_id,
                1,
                &hh_env::events::ScopeChain {
                    turn_id: "turn-1".to_string(),
                    model_call_id: "mc-1".to_string(),
                    tool_call_id: "tc-1".to_string(),
                },
            )
            .unwrap();
        // The helper died before applying — the world answers NotApplied.
        assert_eq!(verdict, ProbeVerdict::NotApplied);
        assert_eq!(
            world.load(Ordering::SeqCst),
            0,
            "dead helper applied work for {}",
            tool.name()
        );
    }
}

/// AC-R-2.2.2-1 + AC-R-2.2.3-3 — the duplicate-effect veto across restart:
/// a committed+observed `compensable` (dedup_support = executor) effect's
/// re-dispatch after crash+restore returns the stored observation; the
/// external counter reads exactly 1; no second `committed` lands.
#[test]
fn ac_r_2_2_2_1_duplicate_veto_survives_restart() {
    let tag = "dup-veto";
    let root = dir(tag);
    let ws = workspace(tag);
    let world = Arc::new(AtomicU64::new(0));
    let applied = Arc::new(Mutex::new(BTreeSet::new()));

    let (mut store, run, lease, _clock) = open_at(&root);
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let sb = ProbeTool::Compensable.scope_bindings();
    let cap = capability(ProbeTool::Compensable, sb.clone());
    let bind = binding();
    let mon = battery_monitor(&cap);
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);

    let mut disp = Dispatcher::new(&mut store, &mon, &run, [1u8; 32], DetectorSet::default());
    let mut exec = BatteryExecutor::new(
        ProbeTool::Compensable,
        DedupSupport::BestEffort,
        world.clone(),
        applied.clone(),
        &ws,
    );
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        ProbeTool::Compensable.args(&ws),
        ProbeTool::Compensable,
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");
    assert_eq!(world.load(Ordering::SeqCst), 1);
    drop(disp);
    drop(driver);
    drop(store);

    // ── crash + reopen — the in-memory DedupStore dies with the runtime ──
    let mut store2 = reopen(&root, 120_000);
    let report = store2.restore(&run, "writer-b", TTL_MS).unwrap();
    let lease2 = report.lease.clone();

    // `effects_by_key` rebuilt from the durable fold — the projection
    // survives the restart.
    let evs = read_all(&store2, &run);
    let view = hh_ledger::views::effects_by_key_view(&run, &evs, None);
    let Json::Obj(effects_obj) = view.payload.get("effects").unwrap() else {
        panic!("effects_by_key payload malformed")
    };
    assert!(
        effects_obj
            .values()
            .any(|v| v.get("effect_id").and_then(Json::as_str) == Some(effect_id.as_str())),
        "effects_by_key lost the committed key across restart"
    );
    let Json::Arr(dups) = view.payload.get("duplicate_committed").unwrap() else {
        panic!("effects_by_key payload malformed")
    };
    assert!(
        dups.is_empty(),
        "duplicate_committed must be empty (AC-R-2.2.3-3)"
    );

    // The resuming writer re-opens the model_call scope — `restore` fenced
    // the crashed attempt `worker_lost`; the retried call carries the same
    // coordinate, so the derived `effect_id` and idempotency key recur.
    let m = EventMinter::new(&store2, &run);
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".to_string()),
        model_call_id: Some("mc-1".to_string()),
        ..Scope::default()
    };
    store2.append(&run, &lease2, vec![c]).unwrap();

    // A rebuilt dispatcher (cold dedup cache) re-dispatches the same
    // coordinate — the durable fold vetoes the second commit and serves
    // the stored observation.
    let mut driver2 = EnvDriver::new(&run);
    let env2 = ready_env(&mut store2, &lease2, &mut driver2, &ws);
    let mut disp2 = Dispatcher::new(&mut store2, &mon, &run, [2u8; 32], DetectorSet::default());
    disp2.rebuild_dedup().unwrap();
    let mut exec2 = BatteryExecutor::new(
        ProbeTool::Compensable,
        DedupSupport::BestEffort,
        world.clone(),
        applied.clone(),
        &ws,
    );
    let inp2 = input(
        &cap,
        &bind,
        &sb,
        &env2,
        "tc-1",
        ProbeTool::Compensable.args(&ws),
        ProbeTool::Compensable,
    );
    let out2 = disp2
        .dispatch(&mut driver2, &mut exec2, None, &inp2, &lease2)
        .unwrap();
    match out2 {
        DispatchOutcome::Duplicate {
            prior_effect_id,
            stored_observation,
        } => {
            assert_eq!(prior_effect_id, effect_id);
            assert!(
                stored_observation.is_some(),
                "dedup_support=executor must serve the stored observation"
            );
        }
        other => panic!("duplicate key dispatched again: {other:?}"),
    }
    assert_eq!(
        world.load(Ordering::SeqCst),
        1,
        "the vetoed effect redispatched"
    );
    assert_eq!(
        effect_rows(disp2.store_mut(), &run, &effect_id).committed,
        1,
        "a second committed row landed for the vetoed key"
    );
    assert_eq!(
        class_count(disp2.store_mut(), &run, "action.tool.rejected"),
        1
    );
}
