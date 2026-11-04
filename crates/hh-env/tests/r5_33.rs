//! R5.33 — the real permission wait/answer/resume matrix (evidence spec
//! §5.2 / D3-F4; the O4/O5 fixture slice). A real `Store`/WAL, real Π
//! monitor and Dispatcher produce the ask → `Suspended` trail (never a
//! fabricated pending); the answer runs the monitor's offered-answer
//! authority (`ApprovalState::respond_with_ctx`) with the durable
//! `decided` mint under the fenced writer; the `permission_decided`
//! wakeup fires through `deliver_wakeup`; the same input re-dispatches to
//! the recorded decision — the common O1 path landing exactly one marker.
//! The `MarkerSink`/`OpsObserver`/`Sentinel` apparatus records the two
//! observed no-mutation intervals and the deny path's zero delta.

use std::collections::{BTreeMap, BTreeSet};
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
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::ops::{
    o4_outcome, o5_outcome, MarkerSink, ObservationPoint, ObservedEvent, OpsObserver,
    PermissionDenyCell, PermissionOutcome, PermissionWaitCell, Sentinel, TrialMarker,
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
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
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
use hharness_provenance::authority::AuthorityClass;
use hharness_provenance::{Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;

const TTL_MS: u64 = 60_000;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-r5_33-{tag}-{n}-{}", std::process::id()));
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
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
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

fn class_count(store: &Store, run: &str, class: &str, effect_id: &str) -> usize {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == class && e.scope.effect_id.as_deref() == Some(effect_id))
        .count()
}

/// The fixed ask tool's declared class: `permission_request` +
/// `irreversible` — the attended Π floor's `ask` arm (the acceptance
/// battery's ask-trail shape).
fn declared_class() -> EffectClass {
    EffectClass {
        domain: EffectDomain::PermissionRequest,
        attributes: Some(EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::NonIdempotent,
            world: World::Closed,
            reversibility: Reversibility::Irreversible,
        }),
    }
}

/// The O1-path class for the `input_required` control: `fs_write` +
/// `non_idempotent` + `irreversible` — the allow-armed shape the marker
/// executor runs under (R5.32's cell capability).
fn fs_class() -> EffectClass {
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
    cap_for("test:perm_marker", declared_class())
}

/// The allow-armed `fs_write` capability for the `input_required` cell.
fn fs_capability() -> ToolCapabilityRecord {
    cap_for("test:fs_marker", fs_class())
}

fn cap_for(sid: &str, class: EffectClass) -> ToolCapabilityRecord {
    let _ = sid;
    ToolCapabilityRecord {
        purpose: Text::new(
            "perm marker",
            "test:owner",
            ProvenanceRecord::kernel("t", 0),
        ),
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
        effects: ToolEffects::Declared(vec![class].into_iter().collect()),
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
        surface_name: "perm_marker".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:perm_marker".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:perm_marker".to_string(),
        arg_map: ["path"]
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
        surface_id: "perm_marker".to_string(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_refs: vec!["test:perm_marker".to_string()],
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

fn ops_monitor(cap: &ToolCapabilityRecord, sid: &str) -> Monitor {
    let mut table = HandleTable::default();
    let h = AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".to_string(),
            version_id: "v-perm".to_string(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/ops", 0),
        grants: vec![
            Grant {
                effect: EffectClass::domain_only(EffectDomain::FsWrite),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
            // `permission_request` is covered so the ask reaches the Π
            // gate — an uncovered domain denies at authorize step 2.
            Grant {
                effect: EffectClass::domain_only(EffectDomain::PermissionRequest),
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
            binding: binding(),
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
    env: &str,
    ws: &std::path::Path,
    sid: &str,
    declared: EffectClass,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: chain(),
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: &cap.scope_bindings,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: sid.to_string(),
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
    }
}

/// The confined diagnostic executor: appends the unique trial/effect
/// marker to `MarkerSink` and emits entry/bytes/termination notes — the
/// O1 path's mutation the suspended/denied cells must *lack*.
struct MarkerExec {
    decl: ExecutorDeclaration,
    sink_dir: PathBuf,
    obs_dir: PathBuf,
    trial: String,
    paused: bool,
}

impl MarkerExec {
    fn new(sink_dir: PathBuf, obs_dir: PathBuf, trial: &str, paused: bool) -> Self {
        MarkerExec {
            decl: ExecutorDeclaration {
                executor_id: "ops/marker".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::None,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error"].iter().map(|s| s.to_string()).collect(),
                streams: true,
                domains: [EffectDomain::FsWrite, EffectDomain::PermissionRequest]
                    .into_iter()
                    .collect(),
            },
            sink_dir,
            obs_dir,
            trial: trial.to_string(),
            paused,
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
        if self.paused {
            // The protocol edge's `input_required` — a pause marker, never
            // a permission mediation; the effect is already committed.
            return Ok(TerminalReport {
                status: TerminalStatus::Paused {
                    detail: Json::str("input_required"),
                },
                exit_status: None,
                outcome_hint: "paused".to_string(),
                retryable_hint: None,
                detail_ref: None,
                truncated: false,
                omitted_bytes: 0,
                original_size: 0,
            });
        }
        let mut sink =
            MarkerSink::open(&self.sink_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        sink.append(&TrialMarker::new(&self.trial, &request.effect_id, 0))
            .map_err(|e| EnvError::Blob(e.to_string()))?;
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
        Ok(ProbeVerdict::Undeterminable)
    }
}

/// The host's answer through the offered-answer authority: fold the
/// durable prefix, `respond_with_ctx` (offered-set + legitimacy + the
/// exactly-one rule run over the record), then mint the admitted
/// `security.permission.decided` for a fresh terminal — the produce step
/// the boundary's respond performs.
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
    let effect_id = Store::effect_id(run, "mc-1", "tc-1", 0);
    let decider_ref = match &decided_by {
        EndorserRef::Human { subject_ref, .. } => subject_ref.clone(),
        EndorserRef::ApproverGrant { grant_ref } => grant_ref.clone(),
    };
    let mut decided = EventMinter::new(store, run)
        .mint(
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str(pid)),
                ("effect_id", Json::str(effect_id.clone())),
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
    // Scope: the (still-open) effect only — a suspended ask left the
    // `intended` effect scope open while `model_call`/`tool_call` are not
    // appendable across the suspend boundary.
    decided.scope.effect_id = Some(effect_id);
    store.append(run, lease, vec![decided]).unwrap();
    Ok(out)
}

/// Final `decided` rows (non-`ask`) for one permission id.
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

/// The folded offered option ids on the durable `pending` row.
fn offered_ids(store: &Store, run: &str, pid: &str) -> Vec<String> {
    read_all(store, run)
        .iter()
        .find(|e| {
            e.class == "security.permission.pending"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
        })
        .and_then(|e| e.payload.get("request").cloned())
        .and_then(|r| match r.get("options") {
            Some(Json::Arr(rows)) => Some(
                rows.iter()
                    .filter_map(|o| o.as_str().map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// The wait-interval check: zero markers and no
/// `prepared`/`committed`/`tool.started`/executor entry for the effect.
fn wait_interval_clean(store: &Store, run: &str, sink: &MarkerSink, effect_id: &str) -> bool {
    sink.count_for(effect_id).unwrap() == 0
        && class_count(store, run, "action.effect.prepared", effect_id) == 0
        && class_count(store, run, "action.effect.committed", effect_id) == 0
        && class_count(store, run, "action.tool.started", effect_id) == 0
}

/// One O4/O5 cell's surfaces after the suspend.
struct Ask {
    root: PathBuf,
    ws: PathBuf,
    obs_dir: PathBuf,
    run: String,
    lease: Lease,
    store: Store,
    exec: MarkerExec,
    cap: ToolCapabilityRecord,
    bind: SurfaceBinding,
    effect_id: String,
    permission_id: String,
    sentinel: Sentinel,
    sink: MarkerSink,
}

/// Drive the real ask: dispatch → `Suspended{permission_id}` with the
/// durable `decided{ask}`/`anchor`/`pending`/`requested`/wakeup-sub/
/// `suspended` trail — never a fabricated pending.
fn suspend_cell(tag: &str) -> Ask {
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
    let mon = ops_monitor(&cap, "test:perm_marker");
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
    let sentinel = Sentinel::create(&dir(&format!("{tag}-sent")), b"denied").unwrap();
    let sink = MarkerSink::open(&sink_dir).unwrap();
    OpsObserver::open(&obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MarkerExec::new(sink_dir.clone(), obs_dir.clone(), tag, false);
    let inp = input(&cap, &bind, &env, &ws, "test:perm_marker", declared_class());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    let DispatchOutcome::Suspended { permission_id } = out else {
        panic!("the ask suspends awaiting_approval, got {out:?}")
    };
    Ask {
        root,
        ws,
        obs_dir,
        run,
        lease,
        store,
        exec,
        cap,
        bind,
        effect_id,
        permission_id,
        sentinel,
        sink,
    }
}

fn pending_row(store: &Store, run: &str, pid: &str) -> bool {
    read_all(store, run).iter().any(|e| {
        e.class == "security.permission.pending"
            && e.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
    })
}

// ── AC-1 — O4: pending → two clean waits → offered allow → wakeup → resume ──
#[test]
fn o4_allow_wait_answer_resume_one_mutation() {
    let mut a = suspend_cell("o4-allow");
    let (run, pid, effect_id) = (a.run.clone(), a.permission_id.clone(), a.effect_id.clone());
    assert!(pending_row(&a.store, &run, &pid), "durable pending landed");
    // The offered set is durable — the folded row carries the real ids
    // (the ask's spelling set maps to {allow_once, allow_lease, modify}).
    let offered = offered_ids(&a.store, &run, &pid);
    assert_eq!(
        offered,
        vec!["allow_once", "allow_lease", "modify"],
        "{offered:?}"
    );
    // The suspend cause is durable: `awaiting_approval{permission_id}`.
    let suspended = read_all(&a.store, &run).iter().any(|e| {
        e.class == "lifecycle.run.suspended"
            && e.payload.to_canonical_string().contains(pid.as_str())
    });
    assert!(suspended, "suspended{{awaiting_approval:{pid}}} durable");

    // Two observed no-mutation intervals while the permission is owed —
    // temporal snapshots, each re-reading sink + effect rows.
    OpsObserver::open(&a.obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AtBoundary, &a.sink, &a.sentinel)
        .unwrap();
    let clean1 = wait_interval_clean(&a.store, &run, &a.sink, &effect_id);
    assert!(clean1, "interval 1: no prepare/commit/entry/mutation");
    let _ = read_all(&a.store, &run);
    OpsObserver::open(&a.obs_dir)
        .unwrap()
        .snapshot(ObservationPoint::AfterQuiescence, &a.sink, &a.sentinel)
        .unwrap();
    let clean2 = wait_interval_clean(&a.store, &run, &a.sink, &effect_id);
    assert!(clean2, "interval 2: still no mutation");

    // The offered, authority-checked answer — `allow_once` voiced by
    // `human{authority: principal}`.
    let principal = || EndorserRef::Human {
        subject_ref: "host:operator".to_string(),
        authority: AuthorityClass::Principal,
    };
    let out = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    assert!(matches!(out.decision, Decision::Allow) && !out.already_decided);
    assert_eq!(final_decided(&a.store, &run, &pid), 1, "one final decided");

    // The permission_decided wakeup: occurred durable, then fired.
    a.store
        .deliver_wakeup(&run, &a.lease, a.store.now_ms())
        .unwrap();
    let envs = read_all(&a.store, &run);
    let wakeup_fired = envs.iter().any(|e| e.class == "control.wakeup.occurred")
        && envs.iter().any(|e| e.class == "control.wakeup.fired");
    assert!(wakeup_fired, "occurred→fired landed");

    // Same-effect wakeup resume: a rebuilt monitor folds the recorded
    // decision; the same input re-dispatches to the common O1 path.
    let ws = a.ws.clone();
    let mut mon2 = ops_monitor(&a.cap, "test:perm_marker");
    mon2.approvals = ApprovalState::project(&envs, envs.last().unwrap().seq);
    let mut driver2 = EnvDriver::new(&run);
    let env2 = ready_env(&mut a.store, &a.lease, &mut driver2, &ws);
    let inp2 = input(
        &a.cap,
        &a.bind,
        &env2,
        &ws,
        "test:perm_marker",
        declared_class(),
    );
    let mut disp2 = Dispatcher::new(&mut a.store, &mon2, &run, [8u8; 32], DetectorSet::default());
    let out2 = disp2
        .dispatch(&mut driver2, &mut a.exec, None, &inp2, &a.lease)
        .unwrap();
    assert!(
        matches!(out2, DispatchOutcome::Observed(_)),
        "resume: {out2:?}"
    );
    assert_eq!(
        a.sink.count_for(&effect_id).unwrap(),
        1,
        "exactly one marker on the same effect"
    );
    assert_eq!(
        class_count(&a.store, &run, "action.effect.observed", &effect_id),
        1
    );

    // Idempotent identical answer; a conflicting new key cannot authorize.
    let again = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    let idempotent = again.already_decided && matches!(again.decision, Decision::Allow);
    let conflict = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::Deny {
            reason: "changed mind".to_string(),
        },
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    let conflicting_refused = conflict.already_decided
        && matches!(conflict.decision, Decision::Allow)
        && final_decided(&a.store, &run, &pid) == 1
        && a.sink.count_for(&effect_id).unwrap() == 1;
    assert!(idempotent, "identical answer returns the recorded decision");
    assert!(conflicting_refused, "no second grant/mutation");

    // Restart preserves the pending — a second suspended cell's runtime
    // dies mid-wait; restore reports the id owed and stays answerable.
    let mut b = suspend_cell("o4-restart");
    let (brun, bpid) = (b.run.clone(), b.permission_id.clone());
    let root = b.root.clone();
    drop(b.store); // close just the store — the WAL survives subject death
    let mut store2 = reopen(&root, 120_000, 1_000_000);
    let report = store2.restore(&brun, "writer-b", TTL_MS).unwrap();
    let restart_preserved = report.pending_permissions.iter().any(|p| p == &bpid);
    assert!(restart_preserved, "restore keeps {bpid} owed: {report:?}");
    assert!(
        read_all(&store2, &brun)
            .iter()
            .any(|e| e.class == "lifecycle.run.resumed"),
        "resumed row durable"
    );
    let still_clean = wait_interval_clean(&store2, &brun, &b.sink, &b.effect_id);
    assert!(still_clean, "post-restart: no mutation slipped in");
    let out = answer(
        &mut store2,
        &brun,
        &report.lease,
        &bpid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    assert!(
        matches!(out.decision, Decision::Allow),
        "post-restart answer: {out:?}"
    );
    assert_eq!(final_decided(&store2, &brun, &bpid), 1);

    let marker = TrialMarker::new("o4-allow", &effect_id, 0);
    let digest_matches = a
        .sink
        .markers()
        .unwrap()
        .iter()
        .any(|m| m.digest() == marker.digest());
    let verdict = o4_outcome(&PermissionWaitCell {
        pending_durable: true,
        wait_intervals_clean: [clean1, clean2].iter().filter(|c| **c).count() as u8,
        restart_preserved,
        wakeup_fired,
        decided_durable: true,
        marker_count: a.sink.count_for(&effect_id).unwrap(),
        digest_matches,
        idempotent_replay: idempotent,
        conflicting_refused,
        sentinel_intact: a.sentinel.intact() && b.sentinel.intact(),
    });
    assert_eq!(verdict, PermissionOutcome::Pass, "{verdict:?}");
}

// ── AC-2 — O5: offered deny → refusal; nothing prepares/commits/enters ──
#[test]
fn o5_deny_no_mutation_and_no_conflicting_grant() {
    let mut a = suspend_cell("o5-deny");
    let (run, pid, effect_id) = (a.run.clone(), a.permission_id.clone(), a.effect_id.clone());
    assert_eq!(a.sink.count_for(&effect_id).unwrap(), 0);
    let out = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::Deny {
            reason: "refused".to_string(),
        },
        DecisionScope::Once,
        EndorserRef::Human {
            subject_ref: "host:operator".to_string(),
            authority: AuthorityClass::Principal,
        },
    )
    .unwrap();
    assert!(matches!(out.decision, Decision::Deny { .. }));
    a.store
        .deliver_wakeup(&run, &a.lease, a.store.now_ms())
        .unwrap();
    // The resume serves the recorded deny: refused — never prepared,
    // committed, entered or mutated.
    let mut mon2 = ops_monitor(&a.cap, "test:perm_marker");
    let envs = read_all(&a.store, &run);
    mon2.approvals = ApprovalState::project(&envs, envs.last().unwrap().seq);
    let ws = a.ws.clone();
    let mut driver2 = EnvDriver::new(&run);
    let env2 = ready_env(&mut a.store, &a.lease, &mut driver2, &ws);
    let inp2 = input(
        &a.cap,
        &a.bind,
        &env2,
        &ws,
        "test:perm_marker",
        declared_class(),
    );
    let mut disp2 = Dispatcher::new(&mut a.store, &mon2, &run, [9u8; 32], DetectorSet::default());
    let out2 = disp2
        .dispatch(&mut driver2, &mut a.exec, None, &inp2, &a.lease)
        .unwrap();
    assert!(
        matches!(out2, DispatchOutcome::Refused { .. }),
        "deny serves refused: {out2:?}"
    );
    let no_mutation = wait_interval_clean(&a.store, &run, &a.sink, &effect_id);
    // A conflicting later answer cannot authorize — the recorded deny
    // stands; no second decided, no grant, no marker.
    let conflict = answer(
        &mut a.store,
        &run,
        &a.lease,
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
        && final_decided(&a.store, &run, &pid) == 1
        && !read_all(&a.store, &run)
            .iter()
            .any(|e| e.class == "security.permission.granted")
        && a.sink.count_for(&effect_id).unwrap() == 0;
    let verdict = o5_outcome(&PermissionDenyCell {
        pending_durable: pending_row(&a.store, &run, &pid),
        deny_decided: true,
        refused: matches!(out2, DispatchOutcome::Refused { .. }),
        no_mutation,
        conflicting_refused,
        sentinel_intact: a.sentinel.intact(),
    });
    assert_eq!(verdict, PermissionOutcome::Pass, "{verdict:?}");
}

// ── AC-2 — unoffered and unauthorized answers cannot grant/mutate ──────
#[test]
fn unoffered_and_unauthorized_answers_refuse() {
    let mut a = suspend_cell("o4-neg");
    let (run, pid, effect_id) = (a.run.clone(), a.permission_id.clone(), a.effect_id.clone());
    let principal = || EndorserRef::Human {
        subject_ref: "host:operator".to_string(),
        authority: AuthorityClass::Principal,
    };
    // Unoffered: `more_info` is not in the durable offer
    // {allow_once, allow_lease, modify} — typed refusal, pending survives.
    let e = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::MoreInfo,
        DecisionScope::Once,
        principal(),
    )
    .unwrap_err();
    assert!(
        matches!(e, ApprovalError::UnofferedOption { ref option_id } if option_id == "more_info"),
        "unoffered more_info: {e:?}"
    );
    // Unoffered remedy — the pending's remedies set is empty.
    let remedy = hharness_provenance::flow::Remedy::Approval {
        effect_id: effect_id.clone(),
    };
    let e = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::Remedy(remedy),
        DecisionScope::Once,
        principal(),
    )
    .unwrap_err();
    assert!(
        matches!(e, ApprovalError::UnofferedOption { .. }),
        "unoffered remedy: {e:?}"
    );
    // Unauthorized: a `delegate`-voiced allow and an ApproverGrant that
    // no live grant covers — both `IllegitimateEndorsement`.
    let e = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        EndorserRef::Human {
            subject_ref: "delegate:rogue".to_string(),
            authority: AuthorityClass::Delegate,
        },
    )
    .unwrap_err();
    assert!(
        matches!(e, ApprovalError::IllegitimateEndorsement { .. }),
        "delegate endorser: {e:?}"
    );
    let e = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        EndorserRef::ApproverGrant {
            grant_ref: "grant-bogus".to_string(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(e, ApprovalError::IllegitimateEndorsement { .. }),
        "uncovered grant: {e:?}"
    );
    // Nothing landed: no final decided, no grant, pending still folded
    // open, zero mutation.
    assert_eq!(final_decided(&a.store, &run, &pid), 0);
    assert!(!read_all(&a.store, &run)
        .iter()
        .any(|e| e.class == "security.permission.granted"));
    let events = a.store.events(&run).unwrap().to_vec();
    let head = events.last().unwrap().seq;
    let st = ApprovalState::project(&events, head);
    assert!(st.pending.contains_key(&pid), "the pending survived");
    assert!(wait_interval_clean(&a.store, &run, &a.sink, &effect_id));
    // The offered answer still resolves — the owed decision was owed, not burned.
    let out = answer(
        &mut a.store,
        &run,
        &a.lease,
        &pid,
        ResponseChoice::AllowOnce,
        DecisionScope::Once,
        principal(),
    )
    .unwrap();
    assert!(matches!(out.decision, Decision::Allow));
}

// ── AC-2 — `input_required` is not permission mediation ────────────────
#[test]
fn paused_input_required_is_no_permission_wait() {
    // The protocol edge's `input_required` pauses *after* commit — the
    // label alone cannot stand in for O4's no-mutation permission wait:
    // its effect may already be committed (§5.2 O4 note; §5d.4 D3).
    let root = dir("paused");
    let ws = dir("paused-ws");
    let sink_dir = dir("paused-sink");
    let obs_dir = dir("paused-obs");
    std::fs::create_dir_all(&ws).unwrap();
    let (mut store, run, lease) = open_at(&root, 1_000, 0);
    open_scopes(&mut store, &run, &lease);
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // An allow-armed fs_write capability (the O1 path) whose executor
    // reports `input_required` at the observe stage.
    let cap = fs_capability();
    let bind = binding();
    let mon = ops_monitor(&cap, "test:fs_marker");
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MarkerExec::new(sink_dir, obs_dir, "paused", true);
    let inp = input(&cap, &bind, &env, &ws, "test:fs_marker", fs_class());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Paused { .. }), "{out:?}");
    // The paused effect is already *committed* — proof that the label
    // cannot carry the O4 wait's no-mutation claim.
    assert_eq!(
        class_count(&store, &run, "action.effect.committed", &effect_id),
        1
    );
    assert_eq!(
        class_count(&store, &run, "action.effect.input_required", &effect_id),
        1
    );
    assert!(
        read_all(&store, &run)
            .iter()
            .all(|e| e.class != "security.permission.pending"),
        "a protocol pause mints no permission pending"
    );
    let suspended = read_all(&store, &run)
        .iter()
        .any(|e| e.class == "lifecycle.run.suspended");
    assert!(!suspended, "a pause never suspends awaiting_approval");
}
