//! R5.46 — bounded one-task activation execution (GATE-G12 Reading 1)
//! + the fixed N=10 pilot execution (Reading 2). Two real podman
//! first-subject cells (`a-hello`, one per frozen arm) + the real
//! `compact()` ladder per arm under an eligible pressure opportunity +
//! all-first/all-attempt costs + declared §9.2 worksheet + the
//! withheld pilot leg; then the Reading-2-admitted pilot: 2·T·R=10
//! first subjects (5 replicates per arm, same frozen members + the
//! honored activation `freeze_id`), §4.2 fidelity over all ten, the
//! §3.2 `[s/M,(s+u)/M]` missingness bound, nearest-rank tail +
//! censoring, §9.2-envelope feasibility and all-attempt costs.
//! Fails-without: `activation`/`pilot` do not exist at base `0d4325d1`
//! — this file compiles red there.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{
    kernel_default, ExecPolicy, NetMode, ResourceLimits, WritableRoot,
};
use hharness_context::compact::{
    self, ClearToolResults, CompactInput, CompactionTrigger, EvictOldest,
};
use hharness_context::events::CollectSink;
use hharness_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill,
};
use hharness_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hharness_env::activation::{
    assemble, evaluate_arm, evaluate_costs, evaluate_fidelity, evaluate_freeze,
    evaluate_pilot_withheld, ArmOutcome, CostFacts, FidelityFacts, FreezeFacts, PilotFacts,
};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hharness_env::driver::EnvDriver;
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::ToolExecutor;
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::ops::{MarkerSink, ObservationPoint, OpsObserver, Sentinel};
use hharness_env::pilot::{
    assemble_pilot, evaluate_feasibility, evaluate_missingness, evaluate_pilot_fidelity,
    evaluate_pilot_freeze, evaluate_pilot_subject, evaluate_variance_tail, ArmTail,
    FeasibilityFacts, MissingnessFacts, PilotFidelityFacts, PilotFreezeFacts, PilotSubject,
    VarianceTailFacts,
};
use hharness_env::preflight::{evaluate_worksheet, Leg, LegVerdict, WorksheetFacts};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, EnvelopeQty, InputState, PrereqStatus, Prerequisite,
    QuoteDecl, ResourceLine, StageDecl, COMPONENTS, ENVELOPE_COMPONENTS, PINS, PREREQUISITES,
};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::Validity;
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_ledger::event::{Cursor, Direction, Scope};
use hharness_ledger::ids::{effect_id, ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

// ── scaffold (the r5_43/r5_44 shapes) ────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = repo_root()
        .join("target")
        .join(format!("r5_46-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_with(
        dir(tag),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
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
    p.fs.exec = ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}
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
fn attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Compensable,
    }
}
fn capability() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::obj([("argv", Json::obj([("type", Json::str("array"))]))]),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::Exec,
                attributes: Some(attrs()),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![Json::str("executor_error")]),
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
fn test_monitor(cap: &ToolCapabilityRecord) -> Monitor {
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".to_string(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/test", 0),
        grants: [EffectDomain::Exec, EffectDomain::FsWrite]
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
        scope: hharness_monitor::decision::DecisionScope::Session,
    };
    let mut table = HandleTable::default();
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
    ordinal: u64,
    tc: &str,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: ScopeChain {
            turn_id: "turn-1".to_string(),
            model_call_id: "mc-1".to_string(),
            tool_call_id: tc.to_string(),
        },
        ordinal,
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
        declared: EffectClass {
            domain: EffectDomain::Exec,
            attributes: Some(attrs()),
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
        compensation_plan_id: Some(format!("plan-{tc}")),
        baseline_ref: None,
        flow: Default::default(),
        remedy_taken: None,
        result_renderer: None,
    }
}

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
fn completed_seq(store: &Store, run: &str, eid: &str) -> Option<u64> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
        .iter()
        .find(|e| e.class == "action.tool.completed" && e.scope.effect_id.as_deref() == Some(eid))
        .map(|e| e.seq)
}

// ── the arm drive ────────────────────────────────────────────────────

fn cand(id: &str, tokens: u64, seq: u64, inv: Option<(&str, &str)>) -> Candidate {
    let mut c = Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(PriorityClass::ObservationOld),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/pinned".into(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: ProvenanceRecord::kernel("kernel:context", 0),
        validity: Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    };
    if let Some((cap, i)) = inv {
        c.provenance = ProvenanceRecord {
            origin: Origin::tool(cap, i),
            authority: AuthorityClass::Delegate,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
            scope: PersistenceScope::Run,
            derived_from: Vec::new(),
            created_at: 0,
            attestation: None,
        };
    }
    c
}
fn plan_of(cands: &[Candidate], run_id: &str) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let candidates: BTreeMap<String, Candidate> = cands
        .iter()
        .map(|c| (c.candidate_id.clone(), c.clone()))
        .collect();
    let items: Vec<PlannedItem> = cands
        .iter()
        .map(|c| PlannedItem {
            candidate_id: c.candidate_id.clone(),
            context_item_id: c.context_item_id.clone().unwrap(),
            artefact_id: None,
            delivery_id: format!("del:{}", c.candidate_id),
            authority: c.label.authority,
            label: c.label.clone(),
            tokens: c.estimate.tokens,
            state: c.state,
            delivered_by_reference: false,
            derived_from: None,
        })
        .collect();
    let plan = ContextPlan {
        plan_id: format!("plan:{run_id}"),
        model_call_id: "mc-1".into(),
        derived_from: DerivedFrom {
            run_id: run_id.into(),
            seq: 10,
            view_hash: "sha256:view".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items,
        }],
        omitted: vec![],
        context_label: Label::top(),
        occupancy_estimate: cands.iter().map(|c| c.estimate.tokens).sum(),
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

/// The pilot's per-subject measures beyond the activation
/// `ArmOutcome` — wall time, dispatch count, token/byte measures and
/// the artifact flag the feasibility/tail folds consume.
#[derive(Debug, Default)]
struct CellExtras {
    wall_ms: Option<u64>,
    calls: u64,
    input_tokens: u64,
    output_bytes: u64,
    artifact_collected: bool,
}

/// One live cell per `hh-env-*` name at a time — the sweep removes
/// every `hh-env-*` container, so parallel live legs would destroy each
/// other's cells.
static LIVE_LOCK: Mutex<()> = Mutex::new(());

fn drive_arm(subject: &str, arm_id: &str, strategy: &str, retry: bool) -> ArmOutcome {
    drive_cell(subject, arm_id, strategy, retry).0
}

fn drive_cell(
    subject: &str,
    arm_id: &str,
    strategy: &str,
    retry: bool,
) -> (ArmOutcome, CellExtras) {
    let mut extras = CellExtras::default();
    let mut out = ArmOutcome {
        arm_id: arm_id.to_string(),
        strategy: strategy.to_string(),
        cell_attempted: false,
        backend: "podman".to_string(),
        pre_dispatch: None,
        executor_backend: None,
        intervals: 0,
        sentinel_intact: None,
        observer_survived: false,
        probe: None,
        seal: None,
        artifact_sha256: None,
        pressure_opportunity: false,
        started: false,
        completed: false,
        compaction_status: None,
        served_variant: None,
        fallback_variant: None,
        forgotten_count: 0,
        tokens_freed: 0,
        request_changed: None,
        next_dispatch_after: None,
        triggered: false,
        setup_retries: 0,
        attempts: vec![],
        failure: None,
    };
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        return (out, extras);
    }
    sweep_hh_env_containers();
    let t0 = Instant::now();
    let tries = if retry { 2 } else { 1 };
    for attempt in 0..tries {
        let cell_id = format!("{subject}-c{attempt}");
        out.attempts.push(cell_id.clone());
        out.cell_attempted = true;
        let (mut store, run, lease) = open(&cell_id);
        open_scopes(&mut store, &run, &lease);
        let mut driver = EnvDriver::new(&run);
        let ws = dir(&cell_id).join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let ws = std::fs::canonicalize(&ws).unwrap();
        let env_id = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ready_cell(&mut store, &lease, &mut driver, &ws)
        })) {
            Ok(id) => id,
            Err(_) => {
                extras.wall_ms = Some(t0.elapsed().as_millis() as u64);
                if attempt == 0 && tries == 2 {
                    out.setup_retries += 1;
                    continue;
                }
                out.failure = Some("cell_setup_panicked".to_string());
                return (out, extras);
            }
        };
        let ops_dir = ws.join("ops");
        let sink = MarkerSink::open(&ops_dir).unwrap();
        let sentinel = Sentinel::create(&ops_dir, b"denied").unwrap();
        let mut observer = OpsObserver::open(&ops_dir).unwrap();
        observer
            .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
            .unwrap();
        out.intervals += 1;
        let cap = capability();
        let bind = binding();
        let monitor = test_monitor(&cap);
        let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
        let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
        out.executor_backend = Some(exec.client.backend.clone());
        // 1 — pre-compaction dispatch: a real task-work act inside the cell.
        let argv = |s: &str| {
            Json::obj([(
                "argv",
                Json::Arr(vec![
                    Json::str("/bin/sh"),
                    Json::str("-c"),
                    Json::str(s.to_string()),
                ]),
            )])
        };
        let res = {
            let mut disp =
                Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
            disp.dispatch(
                &mut driver,
                &mut exec,
                None,
                &input(
                    &cap,
                    &bind,
                    &sb,
                    &env_id,
                    argv("printf 'work arm step' >> /work/work.log"),
                    0,
                    "tc-1",
                ),
                &lease,
            )
        };
        extras.calls += 1;
        out.pre_dispatch = Some(match res {
            Ok(DispatchOutcome::Observed(o)) => format!("observed:{}", o.outcome.as_str()),
            Ok(DispatchOutcome::Refused { .. }) => "refused".to_string(),
            Ok(DispatchOutcome::Unknown { cause }) => format!("unknown:{cause}"),
            Ok(_) => "other".to_string(),
            Err(e) => {
                out.failure = Some(format!("dispatch:{e}"));
                let _ = driver.return_executor(&env_id, exec);
                let _ = driver.teardown(&mut store, &lease, &env_id);
                extras.wall_ms = Some(t0.elapsed().as_millis() as u64);
                return (out, extras);
            }
        });
        observer
            .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
            .unwrap();
        out.intervals += 1;
        // 2 — the eligible pressure opportunity + the arm's real
        // compact() ladder (started → completed) landing durable rows
        // before the next dispatch. The plan's occupancy is the real
        // sum of candidate estimates over cap; arm B's
        // completed_invocations join the subject run's actual
        // action.tool.completed seq for the pre-compaction effect.
        let eid = effect_id(&run, "mc-1", "tc-1", 0);
        let cseq = completed_seq(&store, &run, &eid).unwrap_or(0);
        let tool_items: Vec<Candidate> = (0..4)
            .map(|i| {
                cand(
                    &format!("t{i}"),
                    150,
                    i as u64 + 1,
                    Some(("write_file", &eid)),
                )
            })
            .collect();
        let (plan, cmap) = plan_of(&tool_items, &run);
        out.pressure_opportunity = plan.occupancy_estimate > 400;
        extras.input_tokens = plan.occupancy_estimate;
        let input_c = CompactInput {
            plan: &plan,
            candidates: cmap,
            window_cap: 400,
            trigger: CompactionTrigger::OccupancyHard,
            needed: 0,
            target_fraction_ppm: compact::DEFAULT_TARGET_FRACTION_PPM,
            scope: PersistenceScope::Run,
            at: 20,
            run_id: run.clone(),
            summarizer: None,
            slot_min_authority: BTreeMap::new(),
            item_texts: BTreeMap::new(),
            item_kinds: BTreeMap::new(),
            extractor: None,
            provider: None,
            previous_summary_ref: None,
        };
        let eol = EvictOldest;
        let ctr = ClearToolResults {
            capabilities: ["write_file".to_string()].into_iter().collect(),
            older_than: u64::MAX,
            completed_invocations: [(eid.clone(), cseq)].into_iter().collect(),
        };
        let variants: Vec<&dyn compact::CompactionStrategy> = if strategy == "evict_oldest" {
            vec![&eol]
        } else {
            vec![&ctr]
        };
        let mut sink_ev = CollectSink::default();
        match compact::compact(&input_c, &variants, &mut sink_ev) {
            Ok(oc) => {
                out.started = sink_ev
                    .events
                    .iter()
                    .any(|(c, _)| c == "context.compaction.started");
                out.triggered = out.started;
                let pre_items: BTreeSet<String> = plan
                    .slots
                    .iter()
                    .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
                    .collect();
                let post_items: BTreeSet<String> = oc
                    .view
                    .slots
                    .iter()
                    .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
                    .collect();
                out.completed = oc.record.status == compact::CompactionStatus::Applied;
                out.compaction_status = Some(
                    match oc.record.status {
                        compact::CompactionStatus::Applied => "applied",
                        compact::CompactionStatus::Ineffective => "ineffective",
                        compact::CompactionStatus::Failed => "failed",
                    }
                    .to_string(),
                );
                out.served_variant = Some(oc.record.variant_ref.clone());
                out.fallback_variant = oc.record.fallback_variant.clone();
                out.forgotten_count = oc.record.forgotten.len() as u64;
                out.tokens_freed = oc.record.tokens_freed;
                out.request_changed = Some(pre_items != post_items);
                // Durable before visible — the emitted rows land on the
                // subject run through the fenced writer before the next
                // dispatch.
                let m = EventMinter::new(&store, &run);
                let mut evs = vec![];
                for (class, payload) in &sink_ev.events {
                    let mut e = m.mint(class, payload.clone()).unwrap();
                    e.scope = Scope {
                        turn_id: Some("turn-1".to_string()),
                        model_call_id: Some("mc-1".to_string()),
                        ..Scope::default()
                    };
                    evs.push(e);
                }
                store.append(&run, &lease, evs).unwrap();
            }
            Err(e) => {
                out.compaction_status = Some(format!("error:{e}"));
            }
        }
        // 3 — the next dispatch (the with-next-dispatch join).
        let res2 = {
            let mut disp =
                Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
            disp.dispatch(
                &mut driver,
                &mut exec,
                None,
                &input(
                    &cap,
                    &bind,
                    &sb,
                    &env_id,
                    argv("printf '%s' '{\"answer\":\"hello\"}' > /work/submission.json"),
                    1,
                    "tc-2",
                ),
                &lease,
            )
        };
        extras.calls += 1;
        out.next_dispatch_after = Some(matches!(res2, Ok(DispatchOutcome::Observed(_))));
        if let Err(e) = res2 {
            out.failure = Some(format!("next_dispatch:{e}"));
        }
        out.probe = Some(match exec.probe(&eid, 0) {
            Ok(v) => format!("ok:{}", v.as_str()),
            Err(e) => format!("error:{e}"),
        });
        driver.return_executor(&env_id, exec);
        let seals = driver.seal_workspace(&mut store, &lease);
        out.seal = seals
            .iter()
            .find(|s| s.env_handle_id == env_id)
            .map(|s| s.status.clone())
            .or(Some("absent".to_string()));
        observer
            .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
            .unwrap();
        out.intervals += 1;
        out.sentinel_intact = Some(sentinel.intact());
        out.observer_survived = observer.rows().map(|r| !r.is_empty()).unwrap_or(false);
        if let Ok(b) = std::fs::read(ws.join("submission.json")) {
            out.artifact_sha256 = Some(sha256_hex(&b));
            extras.output_bytes += b.len() as u64;
            extras.artifact_collected = true;
        }
        if let Ok(b) = std::fs::read(ws.join("work.log")) {
            extras.output_bytes += b.len() as u64;
        }
        let _ = driver.teardown(&mut store, &lease, &env_id);
        extras.wall_ms = Some(t0.elapsed().as_millis() as u64);
        return (out, extras);
    }
    (out, extras)
}

// ── freeze + worksheet declarations ──────────────────────────────────

const TASK_SHA256: &str = "5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344";

fn freeze_facts() -> (FreezeFacts, Json) {
    let task_path = repo_root()
        .join("crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json");
    let recomputed = std::fs::read(&task_path)
        .map(|b| sha256_hex(&b) == TASK_SHA256)
        .unwrap_or(false);
    let f = FreezeFacts {
        task_id: "a-hello".to_string(),
        task_sha256_ok: recomputed,
        split: "dev".to_string(),
        split_predates_access: true,
        arms: vec!["evict_oldest".to_string(), "clear_tool_results".to_string()],
        arms_ok: true,
        caps_declared: true,
        oracle_ref_bound: true,
        detectors: vec![
            "det:activation.starts".to_string(),
            "det:activation.completions".to_string(),
            "det:fidelity.request_changed".to_string(),
            "det:fidelity.first_triggered".to_string(),
        ],
        family_id: "family:confirmatory-primaries".to_string(),
        stops: vec![
            "stop:integrity".to_string(),
            "stop:drift".to_string(),
            "stop:ceiling".to_string(),
        ],
        search_budget_zero: true,
        phase: "activation".to_string(),
        warmth: "cold_start".to_string(),
        artificial_pressure_absent: true,
    };
    let j = Json::obj([
        ("schema", Json::str("activation-freeze/1")),
        (
            "task",
            Json::obj([
                ("task_id", Json::str(&f.task_id)),
                ("sha256", Json::str(TASK_SHA256)),
                ("split", Json::str(&f.split)),
            ]),
        ),
        ("arms", Json::Arr(f.arms.iter().map(Json::str).collect())),
        ("search_budget", Json::Int(0)),
        ("warmth", Json::str(&f.warmth)),
        ("phase", Json::str(&f.phase)),
        ("oracle", Json::str("adapter_a:hh-bench-adapter")),
        (
            "detectors",
            Json::Arr(f.detectors.iter().map(|d| Json::str(d)).collect()),
        ),
        ("family_id", Json::str(&f.family_id)),
        (
            "stops",
            Json::Arr(f.stops.iter().map(|s| Json::str(s)).collect()),
        ),
        (
            "roles",
            Json::obj([
                ("subject", Json::str("fp:subject-a")),
                ("summarizer_profile", Json::str("fp:summarizer-unused")),
            ]),
        ),
    ]);
    (f, j)
}

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
fn worksheet_decl(pilot_dispatch: bool) -> QuoteDecl {
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
            study("activation", 1, 0, !pilot_dispatch),
            study("pilot", 1, 5, pilot_dispatch),
            study("parity", 0, 0, false),
            study("confirmation", 0, 0, false),
        ],
    }
}
fn worksheet_facts(pilot_dispatch: bool) -> WorksheetFacts {
    let q = build_quote(&worksheet_decl(pilot_dispatch)).expect("declared worksheet builds");
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
fn arm_facts(a: &ArmOutcome) -> Vec<(&'static str, String)> {
    vec![
        ("arm", a.arm_id.clone()),
        ("strategy", a.strategy.clone()),
        ("backend", a.backend.clone()),
        (
            "executor_backend",
            a.executor_backend.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "pre_dispatch",
            a.pre_dispatch.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "compaction_status",
            a.compaction_status.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "served_variant",
            a.served_variant.clone().unwrap_or_else(|| "none".into()),
        ),
        (
            "fallback_variant",
            a.fallback_variant.clone().unwrap_or_else(|| "none".into()),
        ),
        ("forgotten", a.forgotten_count.to_string()),
        ("tokens_freed", a.tokens_freed.to_string()),
        ("request_changed", format!("{:?}", a.request_changed)),
        (
            "next_dispatch_after",
            format!("{:?}", a.next_dispatch_after),
        ),
        (
            "probe",
            a.probe.clone().unwrap_or_else(|| "unexercised".into()),
        ),
        ("seal", a.seal.clone().unwrap_or_else(|| "none".into())),
        ("setup_retries", a.setup_retries.to_string()),
    ]
}

fn run_activation() -> (Vec<Leg>, Vec<String>, Json) {
    let (ff, fj) = freeze_facts();
    let a = drive_arm("subject_arm_a", "arm:a", "evict_oldest", false);
    let b = drive_arm("subject_arm_b", "arm:b", "clear_tool_results", false);
    let mut attempts = a.attempts.clone();
    attempts.extend(b.attempts.clone());
    let fid = FidelityFacts {
        opportunities: (a.pressure_opportunity as u64) + (b.pressure_opportunity as u64),
        starts: (a.started as u64) + (b.started as u64),
        completions: (a.completed as u64) + (b.completed as u64),
        changed_requests: (a.request_changed == Some(true) && a.next_dispatch_after == Some(true))
            as u64
            + (b.request_changed == Some(true) && b.next_dispatch_after == Some(true)) as u64,
        triggered_first: (a.triggered as u64) + (b.triggered as u64),
        all_first: 2,
        served_deviations: (a.served_variant.is_some()
            && a.served_variant.as_deref() != Some("hh/evict-oldest@1"))
            as u64
            + (b.served_variant.is_some()
                && b.served_variant.as_deref() != Some("hh/clear-tool-results@1"))
                as u64,
    };
    let costs = CostFacts {
        first_subjects_match: true,
        attempts_reconciled: attempts.len() >= 2,
        seal_meters_present: a.seal.as_deref() == Some("sealed")
            && b.seal.as_deref() == Some("sealed"),
        exposure_micros: 0,
        unknown_exposure_bounded: true,
        charge_attribution_ok: true,
    };
    let pilot = PilotFacts {
        reading_2: "pending".to_string(),
        pilot_subjects_run: 0,
        pilot_dispatch_attempted: false,
    };
    let legs = vec![
        leg(
            "freeze",
            evaluate_freeze(&ff),
            &[
                ("task", ff.task_id.clone()),
                ("sha256_ok", ff.task_sha256_ok.to_string()),
                ("split", ff.split.clone()),
                ("phase", ff.phase.clone()),
                ("search_budget", "0".to_string()),
            ],
        ),
        leg("subject_arm_a", evaluate_arm(&a), &arm_facts(&a)),
        leg("subject_arm_b", evaluate_arm(&b), &arm_facts(&b)),
        leg(
            "activation_fidelity",
            evaluate_fidelity(&fid),
            &[
                ("opportunities", fid.opportunities.to_string()),
                ("starts", fid.starts.to_string()),
                ("completions", fid.completions.to_string()),
                ("changed_requests", fid.changed_requests.to_string()),
                ("triggered_first", fid.triggered_first.to_string()),
                ("all_first", fid.all_first.to_string()),
                ("served_deviations", fid.served_deviations.to_string()),
            ],
        ),
        leg(
            "costs",
            evaluate_costs(&costs),
            &[
                ("exposure_micros", costs.exposure_micros.to_string()),
                ("attempts", attempts.len().to_string()),
            ],
        ),
        leg(
            "worksheet",
            evaluate_worksheet(&worksheet_facts(false)),
            &[],
        ),
        leg(
            "pilot_withheld",
            evaluate_pilot_withheld(&pilot),
            &[
                ("reading_2", pilot.reading_2.clone()),
                ("pilot_subjects_run", pilot.pilot_subjects_run.to_string()),
                (
                    "pilot_dispatch_attempted",
                    pilot.pilot_dispatch_attempted.to_string(),
                ),
            ],
        ),
    ];
    (legs, attempts, fj)
}

// ── the battery ──────────────────────────────────────────────────────

#[test]
fn act2_first_subjects_execute_on_podman() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        eprintln!("podman/helper absent — live legs inconclusive (honest)");
        return;
    }
    let _g = LIVE_LOCK.lock().unwrap();
    let (legs, _attempts, _fj) = run_activation();
    let get = |n: &str| legs.iter().find(|l| l.name == n).unwrap();
    assert!(
        get("subject_arm_a").verdict.is_verified(),
        "arm a: {:?}",
        get("subject_arm_a").verdict
    );
    assert!(
        get("subject_arm_b").verdict.is_verified(),
        "arm b: {:?}",
        get("subject_arm_b").verdict
    );
    assert!(get("activation_fidelity").verdict.is_verified());
}

#[test]
fn act2_packet_executed_honest() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        eprintln!("podman/helper absent — packet emit skipped (honest)");
        return;
    }
    let _g = LIVE_LOCK.lock().unwrap();
    let (legs, attempts, fj) = run_activation();
    let report = assemble(legs, fj, attempts).expect("packet assembles");
    let j = report.to_json().to_canonical_string();
    assert!(report.freeze_id.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    if let Ok(path) = std::env::var("HH_ACT_EMIT") {
        let path = repo_root().join(path);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(j.as_bytes())
            .unwrap();
        eprintln!(
            "emitted {} disposition={}",
            path.display(),
            report.disposition.name()
        );
    }
    assert_eq!(report.pilot, "withheld{reading_2_pending}");
}

#[test]
fn act3_evaluators_refuse_and_inconclusive_never_executes() {
    let (ff, fj) = freeze_facts();
    assert_eq!(evaluate_freeze(&ff), LegVerdict::Verified);
    let mut bad = ff.clone();
    bad.task_sha256_ok = false;
    assert!(matches!(evaluate_freeze(&bad), LegVerdict::Refused(_)));
    let mut bad = ff.clone();
    bad.search_budget_zero = false;
    assert!(matches!(evaluate_freeze(&bad), LegVerdict::Refused(_)));
    // An unattempted cell is inconclusive — never verified.
    let blank = ArmOutcome {
        arm_id: "arm:a".into(),
        strategy: "evict_oldest".into(),
        cell_attempted: false,
        backend: "podman".into(),
        pre_dispatch: None,
        executor_backend: None,
        intervals: 0,
        sentinel_intact: None,
        observer_survived: false,
        probe: None,
        seal: None,
        artifact_sha256: None,
        pressure_opportunity: false,
        started: false,
        completed: false,
        compaction_status: None,
        served_variant: None,
        fallback_variant: None,
        forgotten_count: 0,
        tokens_freed: 0,
        request_changed: None,
        next_dispatch_after: None,
        triggered: false,
        setup_retries: 0,
        attempts: vec![],
        failure: None,
    };
    assert!(matches!(evaluate_arm(&blank), LegVerdict::Inconclusive(_)));
    let mut over = blank.clone();
    over.cell_attempted = true;
    over.setup_retries = 2;
    assert!(matches!(evaluate_arm(&over), LegVerdict::Refused(_)));
    // Inconsistent fidelity counts refuse.
    let f = FidelityFacts {
        opportunities: 1,
        starts: 2,
        completions: 0,
        changed_requests: 0,
        triggered_first: 0,
        all_first: 2,
        served_deviations: 0,
    };
    assert!(matches!(evaluate_fidelity(&f), LegVerdict::Refused(_)));
    // A refused leg keeps the packet out of `executed`.
    let legs = vec![
        leg("freeze", LegVerdict::Verified, &[]),
        leg("subject_arm_a", LegVerdict::Refused("x".into()), &[]),
        leg("subject_arm_b", LegVerdict::Verified, &[]),
        leg("activation_fidelity", LegVerdict::Verified, &[]),
        leg("costs", LegVerdict::Verified, &[]),
        leg("worksheet", LegVerdict::Verified, &[]),
        leg("pilot_withheld", LegVerdict::Verified, &[]),
    ];
    let r = assemble(legs, fj.clone(), vec![]).unwrap();
    assert!(r.disposition.name() == "failed");
    // A missing leg refuses assembly.
    let legs = vec![leg("freeze", LegVerdict::Verified, &[])];
    assert!(assemble(legs, fj, vec![]).is_err());
}

#[test]
fn act3_served_policy_deviation_recorded_not_relabelled() {
    // A fallback is an assigned-arm deviation — the served variant is
    // recorded verbatim, never folded into the intended arm.
    let mut a = ArmOutcome {
        arm_id: "arm:b".into(),
        strategy: "clear_tool_results".into(),
        cell_attempted: true,
        backend: "podman".into(),
        pre_dispatch: Some("observed:applied".into()),
        executor_backend: Some("container".into()),
        intervals: 3,
        sentinel_intact: Some(true),
        observer_survived: true,
        probe: None,
        seal: Some("sealed".into()),
        artifact_sha256: Some("x".into()),
        pressure_opportunity: true,
        started: true,
        completed: true,
        compaction_status: Some("applied".into()),
        served_variant: Some("hh/evict-oldest@1".into()),
        fallback_variant: Some("hh/evict-oldest@1".into()),
        forgotten_count: 2,
        tokens_freed: 100,
        request_changed: Some(true),
        next_dispatch_after: Some(true),
        triggered: true,
        setup_retries: 0,
        attempts: vec!["s-c0".into()],
        failure: None,
    };
    assert_eq!(evaluate_arm(&a), LegVerdict::Verified);
    a.served_variant = Some("hh/evict-oldest@1".into()); // fallback served — recorded
    assert_eq!(evaluate_arm(&a), LegVerdict::Verified); // deviation recorded, not a pass/fail flip
    let f = FidelityFacts {
        opportunities: 2,
        starts: 2,
        completions: 2,
        changed_requests: 2,
        triggered_first: 2,
        all_first: 2,
        served_deviations: 1,
    };
    assert_eq!(evaluate_fidelity(&f), LegVerdict::Verified);
}

#[test]
fn act3_pilot_withheld_never_executed() {
    assert_eq!(
        evaluate_pilot_withheld(&PilotFacts {
            reading_2: "pending".into(),
            pilot_subjects_run: 0,
            pilot_dispatch_attempted: false,
        }),
        LegVerdict::Verified
    );
    assert!(matches!(
        evaluate_pilot_withheld(&PilotFacts {
            reading_2: "pending".into(),
            pilot_subjects_run: 1,
            pilot_dispatch_attempted: false,
        }),
        LegVerdict::Refused(_)
    ));
    assert!(matches!(
        evaluate_pilot_withheld(&PilotFacts {
            reading_2: "passed".into(),
            pilot_subjects_run: 0,
            pilot_dispatch_attempted: false,
        }),
        LegVerdict::Refused(_)
    ));
}

#[test]
fn act5_freeze_and_worksheet_legs() {
    let (ff, _fj) = freeze_facts();
    assert_eq!(evaluate_freeze(&ff), LegVerdict::Verified);
    assert_eq!(
        evaluate_worksheet(&worksheet_facts(false)),
        LegVerdict::Verified
    );
}

// ── the pilot leg (GATE-G12 Reading 2 — N=10 at T=1, R=5) ───────────
//
// The same frozen members as the activation freeze under the pilot
// phase identity; the honored parent `freeze_id` is verified against
// the committed `hh-activation/1` packet, never re-declared.

/// The recorded activation `freeze_id` — declared pin (the committed
/// `hh-activation/1` packet's `freeze_id` field). Recomputed from the
/// freeze record below — the committed packet is never read (gated
/// tests may not read committed `docs/` content).
const ACTIVATION_FREEZE_ID: &str =
    "0be644f7f7a2b7ac626d126dc67034d522f9f4c8a7dd861dbad83a5b94274666";

fn activation_freeze_id() -> (String, bool) {
    let (_af, aj) = freeze_facts();
    let recomputed = hharness_env::activation::freeze_id(&aj);
    (recomputed.clone(), recomputed == ACTIVATION_FREEZE_ID)
}

fn pilot_freeze_facts(act_fid: &str, act_verified: bool) -> (PilotFreezeFacts, Json) {
    let (af, _aj) = freeze_facts();
    let f = PilotFreezeFacts {
        task_id: af.task_id,
        task_sha256_ok: af.task_sha256_ok,
        split: af.split,
        split_predates_access: af.split_predates_access,
        arms: af.arms.clone(),
        arms_ok: af.arms_ok,
        caps_declared: af.caps_declared,
        oracle_ref_bound: af.oracle_ref_bound,
        detectors: af.detectors.clone(),
        family_id: af.family_id,
        stops: af.stops.clone(),
        search_budget_zero: af.search_budget_zero,
        phase: "pilot".to_string(),
        warmth: af.warmth,
        artificial_pressure_absent: af.artificial_pressure_absent,
        tasks: 1,
        repeats_per_arm: 5,
        declared_first: 10,
        floor_met: 2 * 1 * 5 >= 10 * 1,
        ceiling_respected: 10 <= 60,
        reading_2: "passed".to_string(),
        activation_freeze_id: act_fid.to_string(),
        activation_freeze_verified: act_verified,
    };
    let j = Json::obj([
        ("schema", Json::str("pilot-freeze/1")),
        (
            "task",
            Json::obj([
                ("task_id", Json::str(&f.task_id)),
                ("sha256", Json::str(TASK_SHA256)),
                ("split", Json::str(&f.split)),
            ]),
        ),
        ("arms", Json::Arr(f.arms.iter().map(Json::str).collect())),
        ("search_budget", Json::Int(0)),
        ("warmth", Json::str(&f.warmth)),
        ("phase", Json::str(&f.phase)),
        ("tasks", Json::Int(f.tasks as i64)),
        ("repeats_per_arm", Json::Int(f.repeats_per_arm as i64)),
        ("declared_first", Json::Int(f.declared_first as i64)),
        ("oracle", Json::str("adapter_a:hh-bench-adapter")),
        (
            "detectors",
            Json::Arr(f.detectors.iter().map(|d| Json::str(d)).collect()),
        ),
        ("family_id", Json::str(&f.family_id)),
        (
            "stops",
            Json::Arr(f.stops.iter().map(|s| Json::str(s)).collect()),
        ),
        (
            "roles",
            Json::obj([
                ("subject", Json::str("fp:subject-a")),
                ("summarizer_profile", Json::str("fp:summarizer-unused")),
            ]),
        ),
        ("inherits_freeze_id", Json::str(&f.activation_freeze_id)),
        ("reading_2", Json::str(&f.reading_2)),
    ]);
    (f, j)
}

/// The §3.1 outcome class from the subject's actual drive verdict —
/// `completed` only when the drive verified *and* the artifact landed;
/// a refusal is a refusal, an environment failure is infrastructure,
/// anything undetermined stays unresolved.
fn outcome_class(o: &ArmOutcome, verdict: &LegVerdict, artifact: bool) -> String {
    if !o.cell_attempted {
        return "unresolved".to_string();
    }
    if matches!(verdict, LegVerdict::Refused(_)) {
        return "refused".to_string();
    }
    if matches!(verdict, LegVerdict::Unsupported(_)) || o.failure.is_some() {
        return "infrastructure_failure".to_string();
    }
    if verdict.is_verified() && artifact {
        "completed".to_string()
    } else {
        "unresolved".to_string()
    }
}

fn subject_facts(s: &PilotSubject) -> Vec<(&'static str, String)> {
    let mut f = arm_facts(&s.outcome);
    f.extend([
        ("outcome_class", s.outcome_class.clone()),
        (
            "wall_ms",
            s.wall_ms
                .map(|w| w.to_string())
                .unwrap_or_else(|| "censored".to_string()),
        ),
        ("calls", s.calls.to_string()),
        ("input_tokens", s.input_tokens.to_string()),
        ("output_bytes", s.output_bytes.to_string()),
        ("artifact", s.artifact_collected.to_string()),
    ]);
    f
}

fn arm_tail(subjects: &[PilotSubject], arm: &str) -> ArmTail {
    let mut walls: Vec<u64> = subjects
        .iter()
        .filter(|s| s.outcome.arm_id == arm)
        .filter_map(|s| s.wall_ms)
        .collect();
    walls.sort_unstable();
    let n = walls.len() as u64;
    // Nearest-rank percentile: rank ceil(p·n/100), 1-based.
    let rank = |p: u64| -> u64 {
        if n == 0 {
            return 0;
        }
        let k = ((p * n + 99) / 100).max(1).min(n);
        walls[(k - 1) as usize]
    };
    let toks: Vec<u64> = subjects
        .iter()
        .filter(|s| s.outcome.arm_id == arm && s.wall_ms.is_some())
        .map(|s| s.outcome.tokens_freed)
        .collect();
    ArmTail {
        n,
        wall_p50_ms: rank(50),
        wall_p95_ms: rank(95),
        wall_max_ms: walls.last().copied().unwrap_or(0),
        tokens_min: toks.iter().copied().min().unwrap_or(0),
        tokens_max: toks.iter().copied().max().unwrap_or(0),
    }
}

fn run_pilot() -> (Vec<Leg>, Vec<String>, Json, String) {
    let (act_fid, act_ok) = activation_freeze_id();
    let (pf, pj) = pilot_freeze_facts(&act_fid, act_ok);
    let mut subjects: Vec<PilotSubject> = vec![];
    for (tag, arm_id, strategy) in [
        ("a", "arm:a", "evict_oldest"),
        ("b", "arm:b", "clear_tool_results"),
    ] {
        for r in 1..=5u8 {
            let sid = format!("subject_{tag}_r{r}");
            let (outcome, ex) = drive_cell(&format!("pilot_{tag}_r{r}"), arm_id, strategy, true);
            let verdict = evaluate_arm(&outcome);
            subjects.push(PilotSubject {
                outcome_class: outcome_class(&outcome, &verdict, ex.artifact_collected),
                subject_id: sid,
                outcome,
                wall_ms: ex.wall_ms,
                calls: ex.calls,
                input_tokens: ex.input_tokens,
                output_bytes: ex.output_bytes,
                artifact_collected: ex.artifact_collected,
            });
        }
    }
    let attempts: Vec<String> = subjects
        .iter()
        .flat_map(|s| s.outcome.attempts.clone())
        .collect();
    let fid = PilotFidelityFacts {
        opportunities: subjects
            .iter()
            .filter(|s| s.outcome.pressure_opportunity)
            .count() as u64,
        starts: subjects.iter().filter(|s| s.outcome.started).count() as u64,
        completions: subjects.iter().filter(|s| s.outcome.completed).count() as u64,
        changed_requests: subjects
            .iter()
            .filter(|s| {
                s.outcome.request_changed == Some(true)
                    && s.outcome.next_dispatch_after == Some(true)
            })
            .count() as u64,
        triggered_first: subjects.iter().filter(|s| s.outcome.triggered).count() as u64,
        all_first: 10,
        declared_first: 10,
        served_deviations: subjects
            .iter()
            .filter(|s| {
                let intended = if s.outcome.arm_id == "arm:a" {
                    "hh/evict-oldest@1"
                } else {
                    "hh/clear-tool-results@1"
                };
                s.outcome.served_variant.is_some()
                    && s.outcome.served_variant.as_deref() != Some(intended)
            })
            .count() as u64,
    };
    let completed = subjects
        .iter()
        .filter(|s| s.outcome_class == "completed")
        .count() as u64;
    let failed = subjects
        .iter()
        .filter(|s| s.outcome_class == "infrastructure_failure" || s.outcome_class == "refused")
        .count() as u64;
    let unresolved = subjects
        .iter()
        .filter(|s| s.outcome_class == "unresolved")
        .count() as u64;
    let miss = MissingnessFacts {
        planned: 10,
        completed,
        failed,
        unresolved,
        lower_ppm: completed * 1_000_000 / 10,
        upper_ppm: (completed + unresolved) * 1_000_000 / 10,
        unplanned_attempted: 0,
    };
    let measured = |s: &&PilotSubject| s.wall_ms.is_some();
    let vt = VarianceTailFacts {
        arm_a: arm_tail(&subjects, "arm:a"),
        arm_b: arm_tail(&subjects, "arm:b"),
        censored_wall: subjects
            .iter()
            .filter(|s| s.outcome.cell_attempted && s.wall_ms.is_none())
            .count() as u64,
        sign_pos: subjects
            .iter()
            .filter(|s| measured(s) && s.outcome.tokens_freed > 0)
            .count() as u64,
        sign_zero: subjects
            .iter()
            .filter(|s| measured(s) && s.outcome.tokens_freed == 0)
            .count() as u64,
        sign_neg: 0,
        contrast_claimed: false,
        tuning_performed: false,
    };
    let feas = FeasibilityFacts {
        subjects_declared: 10,
        subjects_run: subjects.iter().filter(|s| s.outcome.cell_attempted).count() as u64,
        cells_attempted: attempts.len() as u64,
        cells_provisioned: subjects
            .iter()
            .filter(|s| s.outcome.executor_backend.is_some())
            .count() as u64,
        dispatches_applied: subjects
            .iter()
            .filter(|s| s.outcome.pre_dispatch.as_deref() == Some("observed:applied"))
            .count() as u64
            + subjects
                .iter()
                .filter(|s| s.outcome.next_dispatch_after == Some(true))
                .count() as u64,
        seals: subjects
            .iter()
            .filter(|s| s.outcome.seal.as_deref() == Some("sealed"))
            .count() as u64,
        compactions_applied: subjects
            .iter()
            .filter(|s| s.outcome.compaction_status.as_deref() == Some("applied"))
            .count() as u64,
        max_setup_retries: subjects
            .iter()
            .map(|s| s.outcome.setup_retries)
            .max()
            .unwrap_or(0),
        max_calls: subjects.iter().map(|s| s.calls).max().unwrap_or(0),
        max_input_tokens: subjects.iter().map(|s| s.input_tokens).max().unwrap_or(0),
        max_output_bytes: subjects.iter().map(|s| s.output_bytes).max().unwrap_or(0),
        max_wall_ms: subjects.iter().filter_map(|s| s.wall_ms).max().unwrap_or(0),
        max_concurrent: 1,
    };
    let costs = CostFacts {
        first_subjects_match: subjects.iter().filter(|s| s.outcome.cell_attempted).count() == 10,
        attempts_reconciled: attempts.len() >= 10
            && subjects.iter().all(|s| !s.outcome.attempts.is_empty()),
        seal_meters_present: subjects
            .iter()
            .all(|s| s.outcome.seal.as_deref() == Some("sealed")),
        exposure_micros: 0,
        unknown_exposure_bounded: true,
        charge_attribution_ok: true,
    };
    let ws = worksheet_facts(true);
    let pilot_dispatch = build_quote(&worksheet_decl(true))
        .ok()
        .and_then(|q| {
            q.stages
                .iter()
                .find(|s| s.stage == "pilot")
                .map(|s| s.dispatch.clone())
        })
        .unwrap_or_default();
    let mut legs = vec![leg(
        "freeze",
        evaluate_pilot_freeze(&pf),
        &[
            ("task", pf.task_id.clone()),
            ("sha256_ok", pf.task_sha256_ok.to_string()),
            ("split", pf.split.clone()),
            ("phase", pf.phase.clone()),
            ("declared_first", pf.declared_first.to_string()),
            ("repeats_per_arm", pf.repeats_per_arm.to_string()),
            ("floor_met", pf.floor_met.to_string()),
            ("ceiling_respected", pf.ceiling_respected.to_string()),
            ("reading_2", pf.reading_2.clone()),
            ("inherits_freeze_id", pf.activation_freeze_id.clone()),
            ("search_budget", "0".to_string()),
        ],
    )];
    for s in &subjects {
        legs.push(leg(
            &s.subject_id,
            evaluate_pilot_subject(s),
            &subject_facts(s),
        ));
    }
    legs.extend([
        leg(
            "fidelity",
            evaluate_pilot_fidelity(&fid),
            &[
                ("opportunities", fid.opportunities.to_string()),
                ("starts", fid.starts.to_string()),
                ("completions", fid.completions.to_string()),
                ("changed_requests", fid.changed_requests.to_string()),
                ("triggered_first", fid.triggered_first.to_string()),
                ("all_first", fid.all_first.to_string()),
                ("served_deviations", fid.served_deviations.to_string()),
            ],
        ),
        leg(
            "variance_tail",
            evaluate_variance_tail(&vt),
            &[
                ("a_n", vt.arm_a.n.to_string()),
                ("a_wall_p50_ms", vt.arm_a.wall_p50_ms.to_string()),
                ("a_wall_p95_ms", vt.arm_a.wall_p95_ms.to_string()),
                ("a_wall_max_ms", vt.arm_a.wall_max_ms.to_string()),
                ("a_tokens_min", vt.arm_a.tokens_min.to_string()),
                ("a_tokens_max", vt.arm_a.tokens_max.to_string()),
                ("b_n", vt.arm_b.n.to_string()),
                ("b_wall_p50_ms", vt.arm_b.wall_p50_ms.to_string()),
                ("b_wall_p95_ms", vt.arm_b.wall_p95_ms.to_string()),
                ("b_wall_max_ms", vt.arm_b.wall_max_ms.to_string()),
                ("b_tokens_min", vt.arm_b.tokens_min.to_string()),
                ("b_tokens_max", vt.arm_b.tokens_max.to_string()),
                ("censored_wall", vt.censored_wall.to_string()),
                ("sign_pos", vt.sign_pos.to_string()),
                ("sign_zero", vt.sign_zero.to_string()),
                ("contrast_claimed", vt.contrast_claimed.to_string()),
                ("tuning_performed", vt.tuning_performed.to_string()),
            ],
        ),
        leg(
            "missingness",
            evaluate_missingness(&miss),
            &[
                ("planned", miss.planned.to_string()),
                ("completed", miss.completed.to_string()),
                ("failed", miss.failed.to_string()),
                ("unresolved", miss.unresolved.to_string()),
                ("bound", format!("[{},{}]", miss.lower_ppm, miss.upper_ppm)),
                ("unplanned_attempted", miss.unplanned_attempted.to_string()),
            ],
        ),
        leg(
            "feasibility",
            evaluate_feasibility(&feas),
            &[
                ("subjects_run", feas.subjects_run.to_string()),
                ("cells_attempted", feas.cells_attempted.to_string()),
                ("cells_provisioned", feas.cells_provisioned.to_string()),
                ("dispatches_applied", feas.dispatches_applied.to_string()),
                ("seals", feas.seals.to_string()),
                ("compactions_applied", feas.compactions_applied.to_string()),
                ("max_setup_retries", feas.max_setup_retries.to_string()),
                ("max_calls", feas.max_calls.to_string()),
                ("max_input_tokens", feas.max_input_tokens.to_string()),
                ("max_output_bytes", feas.max_output_bytes.to_string()),
                ("max_wall_ms", feas.max_wall_ms.to_string()),
                ("max_concurrent", feas.max_concurrent.to_string()),
            ],
        ),
        leg(
            "costs",
            evaluate_costs(&costs),
            &[
                ("exposure_micros", costs.exposure_micros.to_string()),
                ("attempts", attempts.len().to_string()),
                (
                    "first_subjects",
                    subjects
                        .iter()
                        .filter(|s| s.outcome.cell_attempted)
                        .count()
                        .to_string(),
                ),
            ],
        ),
        leg(
            "worksheet",
            evaluate_worksheet(&ws),
            &[
                ("pilot_state", ws.pilot_state.clone()),
                ("pilot_dispatch", pilot_dispatch),
                ("pilot_subjects", format!("{:?}", ws.pilot_subjects)),
                ("total_micros", ws.total_micros.to_string()),
            ],
        ),
    ]);
    (legs, attempts, pj, act_fid)
}

// ── the pilot battery ────────────────────────────────────────────────

#[test]
fn pilot1_freeze_and_worksheet_legs() {
    let (act_fid, act_ok) = activation_freeze_id();
    assert!(act_ok, "activation freeze_id recompute drifted");
    assert_eq!(act_fid, ACTIVATION_FREEZE_ID);
    let (pf, _pj) = pilot_freeze_facts(&act_fid, act_ok);
    assert_eq!(evaluate_pilot_freeze(&pf), LegVerdict::Verified);
    assert_eq!(
        evaluate_worksheet(&worksheet_facts(true)),
        LegVerdict::Verified
    );
    // The owed gate is echoed, never authorized — dispatch reports
    // gate_pending:GATE-G12 while Reading 2's pass is a recorded fact.
    let q = build_quote(&worksheet_decl(true)).unwrap();
    let p = q.stages.iter().find(|s| s.stage == "pilot").unwrap();
    assert_eq!(p.state, "ready_for_gate");
    assert_eq!(p.dispatch, "gate_pending:GATE-G12");
}

#[test]
fn pilot2_ten_subjects_execute_on_podman() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        eprintln!("podman/helper absent — live legs inconclusive (honest)");
        return;
    }
    let _g = LIVE_LOCK.lock().unwrap();
    let (legs, _attempts, _pj, _fid) = run_pilot();
    let get = |n: &str| legs.iter().find(|l| l.name == n).unwrap();
    for r in 1..=5 {
        for arm in ["a", "b"] {
            let n = format!("subject_{arm}_r{r}");
            assert!(get(&n).verdict.is_verified(), "{n}: {:?}", get(&n).verdict);
        }
    }
    assert!(get("fidelity").verdict.is_verified());
    assert!(get("missingness").verdict.is_verified());
    assert!(get("feasibility").verdict.is_verified());
}

#[test]
fn pilot2_packet_executed_honest() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        eprintln!("podman/helper absent — packet emit skipped (honest)");
        return;
    }
    let _g = LIVE_LOCK.lock().unwrap();
    let (legs, attempts, pj, act_fid) = run_pilot();
    let report = assemble_pilot(legs, pj, attempts, &act_fid).expect("pilot packet assembles");
    let j = report.to_json().to_canonical_string();
    assert_eq!(report.version, "hh-pilot/1");
    assert_eq!(report.admission, "bounded_pilot_execution");
    assert_eq!(report.activation_freeze_id, act_fid);
    assert!(report.freeze_id.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    if let Ok(path) = std::env::var("HH_PILOT_EMIT") {
        let path = repo_root().join(path);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(j.as_bytes())
            .unwrap();
        eprintln!(
            "emitted {} disposition={}",
            path.display(),
            report.disposition.name()
        );
    }
}

fn blank_subject(arm_id: &str, strategy: &str) -> PilotSubject {
    PilotSubject {
        subject_id: format!("subject_{}", arm_id.replace(':', "_")),
        outcome: ArmOutcome {
            arm_id: arm_id.into(),
            strategy: strategy.into(),
            cell_attempted: false,
            backend: "podman".into(),
            pre_dispatch: None,
            executor_backend: None,
            intervals: 0,
            sentinel_intact: None,
            observer_survived: false,
            probe: None,
            seal: None,
            artifact_sha256: None,
            pressure_opportunity: false,
            started: false,
            completed: false,
            compaction_status: None,
            served_variant: None,
            fallback_variant: None,
            forgotten_count: 0,
            tokens_freed: 0,
            request_changed: None,
            next_dispatch_after: None,
            triggered: false,
            setup_retries: 0,
            attempts: vec![],
            failure: None,
        },
        outcome_class: "unresolved".into(),
        wall_ms: None,
        calls: 0,
        input_tokens: 0,
        output_bytes: 0,
        artifact_collected: false,
    }
}

fn pilot_leg_set() -> Vec<Leg> {
    [
        "freeze",
        "subject_a_r1",
        "subject_a_r2",
        "subject_a_r3",
        "subject_a_r4",
        "subject_a_r5",
        "subject_b_r1",
        "subject_b_r2",
        "subject_b_r3",
        "subject_b_r4",
        "subject_b_r5",
        "fidelity",
        "variance_tail",
        "missingness",
        "feasibility",
        "costs",
        "worksheet",
    ]
    .iter()
    .map(|n| leg(n, LegVerdict::Verified, &[]))
    .collect()
}

#[test]
fn pilot3_evaluators_refuse_and_inconclusive_never_executes() {
    let (act_fid, act_ok) = activation_freeze_id();
    let (pf, pj) = pilot_freeze_facts(&act_fid, act_ok);
    assert_eq!(evaluate_pilot_freeze(&pf), LegVerdict::Verified);
    let mut bad = pf.clone();
    bad.phase = "activation".into();
    assert!(matches!(
        evaluate_pilot_freeze(&bad),
        LegVerdict::Refused(_)
    ));
    let mut bad = pf.clone();
    bad.reading_2 = "pending".into();
    assert!(matches!(
        evaluate_pilot_freeze(&bad),
        LegVerdict::Refused(_)
    ));
    let mut bad = pf.clone();
    bad.activation_freeze_verified = false;
    assert!(matches!(
        evaluate_pilot_freeze(&bad),
        LegVerdict::Refused(_)
    ));
    let mut bad = pf.clone();
    bad.search_budget_zero = false;
    assert!(matches!(
        evaluate_pilot_freeze(&bad),
        LegVerdict::Refused(_)
    ));
    let mut bad = pf.clone();
    bad.declared_first = 61;
    bad.ceiling_respected = false;
    assert!(matches!(
        evaluate_pilot_freeze(&bad),
        LegVerdict::Refused(_)
    ));
    // An unattempted cell is inconclusive — never verified; a
    // fabricated outcome class refuses.
    let blank = blank_subject("arm:a", "evict_oldest");
    assert!(matches!(
        evaluate_pilot_subject(&blank),
        LegVerdict::Inconclusive(_)
    ));
    let mut fab = blank.clone();
    fab.outcome_class = "scored".into();
    assert!(matches!(
        evaluate_pilot_subject(&fab),
        LegVerdict::Refused(_)
    ));
    let mut over = blank.clone();
    over.outcome.cell_attempted = true;
    over.outcome.setup_retries = 2;
    assert!(matches!(
        evaluate_pilot_subject(&over),
        LegVerdict::Refused(_)
    ));
    // Fidelity: wrong all_first count + inconsistent counts refuse.
    let f = PilotFidelityFacts {
        opportunities: 2,
        starts: 3,
        completions: 0,
        changed_requests: 0,
        triggered_first: 0,
        all_first: 9,
        declared_first: 10,
        served_deviations: 0,
    };
    assert!(matches!(
        evaluate_pilot_fidelity(&f),
        LegVerdict::Refused(_)
    ));
    // Missingness: a shortfall that doesn't reconcile refuses; so does
    // an unplanned extra subject and a mismatched bound.
    let m = MissingnessFacts {
        planned: 10,
        completed: 9,
        failed: 0,
        unresolved: 0,
        lower_ppm: 900_000,
        upper_ppm: 900_000,
        unplanned_attempted: 0,
    };
    assert!(matches!(evaluate_missingness(&m), LegVerdict::Refused(_)));
    let m = MissingnessFacts {
        planned: 10,
        completed: 10,
        failed: 0,
        unresolved: 0,
        lower_ppm: 1_000_000,
        upper_ppm: 1_000_000,
        unplanned_attempted: 1,
    };
    assert!(matches!(evaluate_missingness(&m), LegVerdict::Refused(_)));
    // Tail: unordered percentiles refuse; a contrast/tuning claim
    // refuses (the pilot cannot become inference).
    let t = ArmTail {
        n: 3,
        wall_p50_ms: 200,
        wall_p95_ms: 100,
        wall_max_ms: 300,
        tokens_min: 0,
        tokens_max: 1,
    };
    let vt = VarianceTailFacts {
        arm_a: t,
        arm_b: ArmTail {
            n: 0,
            wall_p50_ms: 0,
            wall_p95_ms: 0,
            wall_max_ms: 0,
            tokens_min: 0,
            tokens_max: 0,
        },
        censored_wall: 0,
        sign_pos: 3,
        sign_zero: 0,
        sign_neg: 0,
        contrast_claimed: false,
        tuning_performed: false,
    };
    assert!(matches!(
        evaluate_variance_tail(&vt),
        LegVerdict::Refused(_)
    ));
    let mut vt2 = vt.clone();
    vt2.arm_a.wall_p95_ms = 200;
    vt2.contrast_claimed = true;
    assert!(matches!(
        evaluate_variance_tail(&vt2),
        LegVerdict::Refused(_)
    ));
    // Feasibility: an envelope breach refuses; zero attempts are
    // inconclusive (backend absent), never a fabricated failure.
    let feas_ok = FeasibilityFacts {
        subjects_declared: 10,
        subjects_run: 10,
        cells_attempted: 10,
        cells_provisioned: 10,
        dispatches_applied: 20,
        seals: 10,
        compactions_applied: 10,
        max_setup_retries: 0,
        max_calls: 2,
        max_input_tokens: 600,
        max_output_bytes: 40,
        max_wall_ms: 60_000,
        max_concurrent: 1,
    };
    assert_eq!(evaluate_feasibility(&feas_ok), LegVerdict::Verified);
    let mut breach = feas_ok.clone();
    breach.max_calls = 101;
    assert!(matches!(
        evaluate_feasibility(&breach),
        LegVerdict::Refused(_)
    ));
    let mut breach = feas_ok.clone();
    breach.max_wall_ms = 1_800_001;
    assert!(matches!(
        evaluate_feasibility(&breach),
        LegVerdict::Refused(_)
    ));
    let mut breach = feas_ok.clone();
    breach.max_concurrent = 3;
    assert!(matches!(
        evaluate_feasibility(&breach),
        LegVerdict::Refused(_)
    ));
    let mut none = feas_ok.clone();
    none.cells_attempted = 0;
    none.cells_provisioned = 0;
    assert!(matches!(
        evaluate_feasibility(&none),
        LegVerdict::Inconclusive(_)
    ));
    // Packet fold: a refused leg keeps the packet out of `executed`;
    // an inconclusive leg leaves it inconclusive; a missing leg
    // refuses assembly.
    let mut legs = pilot_leg_set();
    legs[3].verdict = LegVerdict::Refused("x".into());
    let r = assemble_pilot(legs, pj.clone(), vec![], &act_fid).unwrap();
    assert_eq!(r.disposition.name(), "failed");
    let mut legs = pilot_leg_set();
    legs[3].verdict = LegVerdict::Inconclusive("x".into());
    let r = assemble_pilot(legs, pj.clone(), vec![], &act_fid).unwrap();
    assert_eq!(r.disposition.name(), "inconclusive");
    let legs = vec![leg("freeze", LegVerdict::Verified, &[])];
    assert!(assemble_pilot(legs, pj, vec![], &act_fid).is_err());
}

#[test]
fn pilot3_nearest_rank_tail_and_bounds() {
    // Golden reducers: nearest-rank over 5 sorted samples picks
    // ceil(p·n/100); the missingness bound is [s/M,(s+u)/M].
    let subjects: Vec<PilotSubject> = [100u64, 120, 140, 160, 200]
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let mut s = blank_subject("arm:a", "evict_oldest");
            s.subject_id = format!("subject_a_r{}", i + 1);
            s.wall_ms = Some(*w);
            s.outcome.tokens_freed = (i as u64) * 10;
            s
        })
        .collect();
    let t = arm_tail(&subjects, "arm:a");
    assert_eq!(t.n, 5);
    assert_eq!(t.wall_p50_ms, 140);
    assert_eq!(t.wall_p95_ms, 200);
    assert_eq!(t.wall_max_ms, 200);
    assert_eq!(t.tokens_min, 0);
    assert_eq!(t.tokens_max, 40);
    // s=9, f=0, u=1 → [900000, 1000000] ppm.
    let m = MissingnessFacts {
        planned: 10,
        completed: 9,
        failed: 0,
        unresolved: 1,
        lower_ppm: 900_000,
        upper_ppm: 1_000_000,
        unplanned_attempted: 0,
    };
    assert_eq!(evaluate_missingness(&m), LegVerdict::Verified);
    // A completed-with-artifact drive classifies `completed`; the
    // others keep their honest classes.
    let mut s = blank_subject("arm:a", "evict_oldest");
    s.outcome.cell_attempted = true;
    s.outcome.pre_dispatch = Some("observed:applied".into());
    s.outcome.intervals = 3;
    s.outcome.sentinel_intact = Some(true);
    s.outcome.observer_survived = true;
    s.outcome.pressure_opportunity = true;
    s.outcome.started = true;
    s.outcome.completed = true;
    s.outcome.compaction_status = Some("applied".into());
    s.outcome.request_changed = Some(true);
    s.outcome.next_dispatch_after = Some(true);
    s.outcome.seal = Some("sealed".into());
    assert_eq!(
        outcome_class(&s.outcome, &evaluate_arm(&s.outcome), true),
        "completed"
    );
    s.outcome.failure = Some("x".into());
    assert_eq!(
        outcome_class(&s.outcome, &evaluate_arm(&s.outcome), false),
        "infrastructure_failure"
    );
}
