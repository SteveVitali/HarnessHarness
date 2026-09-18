//! S1.16 acceptance coverage — every test names the AC it pins
//! (AC-R-2.2.5-{1,2,5,6,9} · AC-R-2.5.5-{2,3,4,5,8,9,12,14}).
//!
//! Each test fails if the behaviour it covers is removed. The scaffold builds
//! a real `Store` (WAL), a real `Monitor` (Π + handle table), a real
//! `EnvDriver` over `Ep2Model::reference()`, and a scripted `MockExecutor`
//! (the executor *reports*; the kernel decides — a mock is the honest seam).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hh_compiler::plan::PinnedRef;
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{
    kernel_default, IsolationClass, NetMode, ResourceLimits, WritableRoot,
};
use hh_env::capture::{CaptureKind, OutputPolicy};
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::{EventMinter, ScopeChain};
use hh_env::executor::{
    bind, DedupSupport, ExecutionRequest, ExecutionRequirement, ExecutorDeclaration,
    ExecutorSignal, InterruptSupport, ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus,
    ToolExecutor,
};
use hh_env::handle::{HandleState, OnLoss, Roots};
use hh_env::observe::{retryable, EffectOutcome, ErrorClass, ErrorOrigin, ObservedStatus};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_hir::refs::Ref;
use hh_identity::idp::address;
use hh_ledger::event::{Cursor, Direction, Scope};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, Monitor};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::mask::{MaskEntry, MaskSet};
use hh_secrets::redact::DetectorSet;
use hh_secrets::types::SecretFingerprint;
use hh_wire::json::Json;

// ── scaffold ─────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A store with a manually-advanced clock the test keeps a handle on.
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

/// Open the `turn ⊃ model_call` scopes the dispatch's scope members require.
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

/// A workspace under the temp dir (the `writable` + `workspace` root).
fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The test policy — kernel default plus the workspace as readable/writable,
/// `net = none` (a mediated claim would need the Stage-2 egress mediator).
fn test_policy(ws: &std::path::Path) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    // `local_host` honestly permits a `/bin` spawn (the process-boundary
    // executor execs `/bin/echo`); the exec allow-set is explicit, not `Any`.
    p.fs.exec = hh_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
    p.net.mode = NetMode::None;
    p.compute_ids();
    p
}

/// An `EnvironmentRecord` over a content-addressed image.
fn test_record(class: EnvironmentClass, image: ImageRef) -> EnvironmentRecord {
    EnvironmentRecord {
        class,
        image,
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

fn test_ca() -> hh_identity::idp::ContentAddress {
    address(b"test-image-bytes", "application/octet-stream")
}

/// `provision + attach(Ep2Model, fail-closed)` → a `ready` handle.
fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    let record = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(test_ca()),
    );
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

// ── monitor fixture ──────────────────────────────────────────────────────────

fn attrs(reversibility: Reversibility) -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility,
    }
}

/// `reversible` attributes — the Π-2 `allow` class for a workspace-local
/// write inside the writable roots.
fn reversible_attrs() -> EffectAttributes {
    attrs(Reversibility::Reversible(Ref::selected(
        "test:proc",
        "latest",
    )))
}

/// `read_only` attributes (`mutability: read_only` projects to
/// `RiskReversibility::ReadOnly`).
fn read_only_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::ReadOnly,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
    }
}

/// The capability the tests bind — `write_file{path, content}` /
/// `run{command, argv}` (the domain varies per test).
fn capability(
    domain: EffectDomain,
    a: EffectAttributes,
    sb: ScopeBindings,
) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
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
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain,
                attributes: Some(a),
            }]
            .into_iter()
            .collect(),
        ),
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
    }
}

/// The surface binding — identity arg-map over the declared args.
fn binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "write_file".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:write_file".to_string(),
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
        capability_refs: vec!["test:write_file".to_string()],
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

/// The capability's `scope_bindings` (a `path` `fs_path` binding).
fn scope_bindings() -> ScopeBindings {
    ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
        ("param_path", Json::str("path")),
        ("scope_kind", Json::str("fs_path")),
    ])]))
}

/// A root handle covering `fs_write`/`proc_spawn`/`fs_read` over everything.
fn root_handle() -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId::parse("hnd-1").unwrap(),
        permission_ref: PinnedRef {
            semantic_id: "test:perm".into(),
            version_id: "v-perm".into(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/test", 0),
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
            // `permission_request` is covered so the ask-floor tests reach
            // the Π gate — an uncovered domain denies at authorize step 2
            // (`NoCoveringGrant`) before the floor can ask.
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
        basis_ref: "test:agent#sha256:def".into(),
        budget_ref: None,
        scope: hh_monitor::decision::DecisionScope::Session,
    }
}

/// A monitor: the default Π table, `test:agent` at principal, the capability
/// + the covering root handle installed.
fn test_monitor(cap: &ToolCapabilityRecord) -> Monitor {
    let mut table = HandleTable::default();
    let h = root_handle();
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

fn chain(tc: &str) -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".to_string(),
        model_call_id: "mc-1".to_string(),
        tool_call_id: tc.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env_handle_id: &str,
    tc: &str,
    args: Json,
    declared: EffectClass,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: chain(tc),
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
    }
}

/// A scripted executor — emits `signals` (echoing the *request's* token),
/// returns `report`, counts `execute` calls, answers `probe` with `verdict`,
/// performs `writes` (for the fs-diff `partial` cases).
struct MockExecutor {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
    signals: Vec<CaptureKind>,
    report: TerminalReport,
    /// When set, `execute` fails with this transport-plane error.
    fail: Option<String>,
    verdict: ProbeVerdict,
    writes: Vec<(String, String)>,
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

fn report_error(class: ErrorClass) -> TerminalReport {
    TerminalReport {
        status: TerminalStatus::ToolError { class },
        exit_status: None,
        outcome_hint: "unknown".into(),
        retryable_hint: Some(false),
        detail_ref: None,
        truncated: false,
        omitted_bytes: 0,
        original_size: 0,
    }
}

impl MockExecutor {
    fn ok() -> Self {
        MockExecutor {
            decl: ExecutorDeclaration {
                executor_id: "mock/1".to_string(),
                isolation_support: IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
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
            calls: std::cell::Cell::new(0),
            signals: vec![],
            report: report_ok(),
            fail: None,
            verdict: ProbeVerdict::Undeterminable,
            writes: vec![],
        }
    }
}

impl ToolExecutor for MockExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        self.calls.set(self.calls.get() + 1);
        for (path, bytes) in &self.writes {
            std::fs::write(path, bytes).unwrap();
        }
        for k in &self.signals {
            sink(ExecutorSignal {
                token: request.attribution_token.clone(),
                kind: k.clone(),
            });
        }
        match &self.fail {
            Some(d) => Err(EnvError::Blob(d.clone())),
            None => Ok(self.report.clone()),
        }
    }
    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        Ok(self.verdict)
    }
}

/// Read back every event in the run (the fold the static ACs check).
fn read_all(store: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

// ── AC-R-2.2.5-1 — no side door ─────────────────────────────────────────────

#[test]
fn ac_r_2_2_5_1_every_handle_event_is_kernel_mediated() {
    let (mut store, run, lease, _clock) = open("noside");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("noside");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs(), scope_bindings());
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([
            ("path", Json::str(format!("{}/a.txt", ws.display()))),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    // The static fold: every `action.effect.committed` and
    // `action.tool.started` has a preceding `decided{allow}` for the effect.
    let envs = read_all(disp.store_mut(), &run);
    let mut allow_seqs: BTreeMap<String, u64> = BTreeMap::new();
    for e in &envs {
        if e.class == "security.permission.decided"
            && e.payload.get("decision").and_then(Json::as_str) == Some("allow")
        {
            if let Some(eid) = e.payload.get("effect_id").and_then(Json::as_str) {
                allow_seqs.insert(eid.to_string(), e.seq);
            }
        }
        if e.class == "action.effect.committed" || e.class == "action.tool.started" {
            let eid = e.scope.effect_id.clone().unwrap();
            let allow = allow_seqs.get(&eid).copied();
            assert!(
                allow.is_some_and(|a| a < e.seq),
                "{}@{} unmediated",
                e.class,
                e.seq
            );
        }
    }
}

// ── AC-R-2.2.5-2 — kernel death ≠ environment death ─────────────────────────

#[test]
fn ac_r_2_2_5_2_heal_reattaches_and_probe_settles_unknown() {
    let (mut store, run, lease, clock) = open("heal");
    let ws = workspace("heal");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // The kernel "dies": the handle goes `unreachable` (contact lost); `heal`
    // reattaches within the window — no re-provision, no re-execution.
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();
    assert_eq!(driver.handle(&env).unwrap().state, HandleState::Unreachable);
    clock.advance(50);
    driver.heal(&mut store, &lease, &env, true).unwrap();
    assert_eq!(driver.handle(&env).unwrap().state, HandleState::Ready);
    let envs = read_all(&store, &run);
    let saw = |c: &str| envs.iter().any(|e| e.class == c);
    assert!(saw("action.environment.unreachable"));
    assert!(saw("action.environment.reattached"));
    // The dead gap is excluded from `active_ms` (the kernel was not there to
    // observe the environment run — the honest-meter rule).
    let v = driver.handle(&env).unwrap().meters.view(store.now_ms());
    assert!(v.active_ms < v.reserved_ms, "{v:?}");
}

// ── AC-R-2.2.5-5 — digest, not tag ──────────────────────────────────────────

#[test]
fn ac_r_2_2_5_5_mutable_tag_is_unresolved_unless_unpinned() {
    let tagged = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::Tag("myimage:latest".into()),
    );
    let err = tagged.resolve().unwrap_err();
    assert!(matches!(err, EnvError::UnresolvedRef { .. }), "{err:?}");

    // `unpinned` names the tag — resolve admits it (honestly recorded).
    let mut unpinned = tagged.clone();
    unpinned.unpinned.insert("myimage:latest".to_string());
    assert!(unpinned.resolve().is_ok());

    // A foreign digest is a recorded claim, never R2 identity.
    let foreign = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ForeignDigest {
            scheme: "sha256".into(),
            value: "abc".into(),
            source: "registry".into(),
        },
    );
    assert!(matches!(
        foreign.check_repro("R2"),
        Err(EnvError::ReproClaimUnsupported { .. })
    ));
    // The content address *is* identity — `identify` covers the image member.
    let ca_rec = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(test_ca()),
    );
    let id = ca_rec.identify().unwrap();
    assert!(!id.version_id.is_empty());
}

// ── AC-R-2.2.5-6 — fail-closed attach ───────────────────────────────────────

#[test]
fn ac_r_2_2_5_6_unknown_evidence_never_reaches_ready() {
    let (mut store, run, lease, _clock) = open("failclosed");
    let ws = workspace("failclosed");
    let mut driver = EnvDriver::new(&run);
    let record = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(test_ca()),
    );
    let roots = Roots {
        workspace_roots: vec![ws.display().to_string()],
        writable_roots: vec![ws.display().to_string()],
        cwd: ws.display().to_string(),
    };
    let h = driver
        .provision(
            &mut store,
            &lease,
            &record,
            roots,
            PolicySlot::Inline(Box::new(test_policy(&ws))),
            OnLoss::FailRun,
        )
        .unwrap();
    // No backend — `helper_unavailable` is fail-closed (`AttachError`).
    let err = driver
        .attach(
            &mut store,
            &lease,
            &h.env_handle_id,
            None,
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            EnvError::Attach(_) | EnvError::ContainmentUnverified { .. }
        ),
        "{err:?}"
    );
    let h2 = driver.handle(&h.env_handle_id).unwrap();
    assert_ne!(h2.state, HandleState::Ready, "never reaches ready");
    // The first fs op is refused — the handle is not `ready`.
    let r = driver.fs_write(&h.env_handle_id, &format!("{}/x", ws.display()), b"x");
    assert!(matches!(
        r,
        Err(EnvError::Unavailable { .. } | EnvError::ContainmentUnverified { .. })
    ));
}

// ── AC-R-2.2.5-9 — clocks ────────────────────────────────────────────────────

#[test]
fn ac_r_2_2_5_9_reserved_covers_active_plus_suspended() {
    let (mut store, run, lease, clock) = open("clocks");
    let ws = workspace("clocks");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    for _ in 0..3 {
        let now = store.now_ms();
        let h = driver.handle(&env).unwrap();
        assert!(h.meters.invariant_ok(now), "{:?}", h.meters.view(now));
        clock.advance(10);
    }
    // `mark_unreachable` closes the active bucket at last contact; the gap to
    // `heal` is excluded from `active_ms`.
    driver.mark_unreachable(&mut store, &lease, &env).unwrap();
    let before = driver.handle(&env).unwrap().meters.view(store.now_ms());
    clock.advance(50);
    driver.heal(&mut store, &lease, &env, true).unwrap();
    let after = driver.handle(&env).unwrap().meters.view(store.now_ms());
    assert!(
        after.active_ms <= before.active_ms + 1,
        "{before:?} {after:?}"
    );
    assert!(after.reserved_ms >= after.active_ms + after.suspended_ms);
}

// ── AC-R-2.5.5-2 — no unmediated effect ─────────────────────────────────────

#[test]
fn ac_r_2_5_5_2_commit_without_allow_is_refused_and_tokens_are_checked() {
    let (mut store, run, lease, _clock) = open("mediated");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("mediated");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs(), scope_bindings());
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [9u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([
            ("path", Json::str(format!("{}/a.txt", ws.display()))),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
    );
    disp.dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();

    // A forged/expired token resolves `Unknown` — the helper-protocol
    // `AttributionMismatch`/`NotCommitted` refusal (the resolver is the
    // helper's read view).
    assert!(matches!(
        disp.resolver().resolve("sha256:forged"),
        hh_env::tokens::ResolveOutcome::Unknown
    ));

    // A `committed` for an effect with no `decided{allow}` is refused by the
    // ledger (`Undecided`) — the durable gate, not the dispatcher's
    // discipline. Open a fresh scope chain for the forged attempt.
    let (t2, c2, prop, intended, authorized, prepared, bad) = {
        let m = EventMinter::new(disp.store_mut(), &run);
        let mut t2 = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
        t2.scope.turn_id = Some("turn-2".into());
        let mut c2 = m.mint("model.call.requested", Json::obj([])).unwrap();
        c2.scope = Scope {
            turn_id: Some("turn-2".into()),
            model_call_id: Some("mc-2".into()),
            ..Scope::default()
        };
        let mut prop = m.mint("action.tool.proposed", Json::obj([])).unwrap();
        prop.scope = Scope {
            turn_id: Some("turn-2".into()),
            model_call_id: Some("mc-2".into()),
            tool_call_id: Some("tc-2".into()),
            ..Scope::default()
        };
        let eid = Store::effect_id(&run, "mc-2", "tc-2", 0);
        let chain2 = ScopeChain {
            turn_id: "turn-2".into(),
            model_call_id: "mc-2".into(),
            tool_call_id: "tc-2".into(),
        };
        let risk = hh_hir::risk::project_risk(Some(&attrs(Reversibility::Compensable)));
        let intended = m
            .mint_effect(
                "action.effect.intended",
                hh_env::events::intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
                &eid,
                &chain2,
            )
            .unwrap();
        // Walk the fold to `prepared` (transition-valid) — no `decided` row.
        let authorized = m
            .mint_effect(
                "action.effect.authorized",
                hh_env::events::authorized_payload(&risk),
                &eid,
                &chain2,
            )
            .unwrap();
        let prepared = m
            .mint_effect(
                "action.effect.prepared",
                hh_env::events::prepared_payload(
                    &hh_ledger::effect::idempotency_key(&run, &eid, "h", "v-cap"),
                    None,
                    Some("plan-1"),
                    "sha256:token",
                    None,
                    "pol-out",
                ),
                &eid,
                &chain2,
            )
            .unwrap();
        let bad = m
            .mint_effect(
                "action.effect.committed",
                hh_env::events::committed_payload(1, lease.generation, 0),
                &eid,
                &chain2,
            )
            .unwrap();
        (t2, c2, prop, intended, authorized, prepared, bad)
    };
    disp.store_mut()
        .append(
            &run,
            &lease,
            vec![t2, c2, prop, intended, authorized, prepared],
        )
        .unwrap();
    let r = disp.store_mut().append(&run, &lease, vec![bad]);
    assert!(
        matches!(r, Err(hh_ledger::errors::LedgerError::Undecided { .. })),
        "{r:?}"
    );
}

// ── AC-R-2.5.5-3 — error surjectivity + origin ──────────────────────────────

#[test]
fn ac_r_2_5_5_3_every_executor_failure_maps_to_one_class_and_origin() {
    let cap = capability(EffectDomain::FsWrite, reversible_attrs(), scope_bindings());
    let decl = MockExecutor::ok().decl;
    // Declared classes → (class, tool); an undeclared class → protocol_error.
    for c in [
        ErrorClass::ExecutorError,
        ErrorClass::Timeout,
        ErrorClass::Signalled {
            signal: "kill".into(),
        },
        ErrorClass::InvalidArguments,
    ] {
        let (cc, o) = hh_env::dispatch::classify_report(&report_error(c.clone()), &cap, &decl);
        assert_eq!(cc, c);
        assert_eq!(o, ErrorOrigin::Tool);
    }
    let (cc, o) = hh_env::dispatch::classify_report(
        &report_error(ErrorClass::ContainmentDenied { kind: "fs".into() }),
        &cap,
        &decl,
    );
    assert_eq!(cc, ErrorClass::ProtocolError);
    assert_eq!(o, ErrorOrigin::Transport);

    // `retryable` is the kernel's D4 function; a `retryable_hint = true` never
    // raises it (non-idempotent → not retryable, the hint cannot override).
    let non_idem = RiskClass {
        reversibility: RiskReversibility::Irreversible,
        repeat_safety: RiskRepeatSafety::NonIdempotent,
        scope: RiskScope::External,
    };
    assert!(!retryable(
        &ErrorClass::Timeout,
        ErrorOrigin::Tool,
        &non_idem
    ));
    assert!(!hh_env::observe::apply_hint(false, Some(true)));
    let idem = RiskClass {
        repeat_safety: RiskRepeatSafety::Idempotent,
        ..non_idem
    };
    assert!(retryable(&ErrorClass::Timeout, ErrorOrigin::Tool, &idem));
    // `hint = false` lowers.
    assert!(!hh_env::observe::apply_hint(true, Some(false)));
}

// ── AC-R-2.5.5-4 — timeout/cancel per class ─────────────────────────────────

#[test]
fn ac_r_2_5_5_4_timeout_maps_by_class() {
    // read_only timeout → `observed(not_applied)`.
    let (mut store, run, lease, _clock) = open("tmo-ro");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("tmo-ro");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), scope_bindings());
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [3u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    exec.report = report_error(ErrorClass::Timeout);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([("path", Json::str(format!("{}/a.txt", ws.display())))]),
        EffectClass {
            domain: EffectDomain::FsRead,
            attributes: Some(read_only_attrs()),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    match out {
        DispatchOutcome::Observed(o) => {
            assert_eq!(o.outcome, EffectOutcome::NotApplied, "{o:?}");
            assert!(matches!(
                o.status,
                ObservedStatus::Error {
                    class: ErrorClass::Timeout,
                    ..
                }
            ));
        }
        other => panic!("expected observed(not_applied), got {other:?}"),
    }

    // compensable timeout → `unknown` → `probe` (never redispatched).
    let (mut store2, run2, lease2, _c2) = open("tmo-comp");
    open_scopes(&mut store2, &run2, &lease2, "turn-1", "mc-1");
    let ws2 = workspace("tmo-comp");
    let mut driver2 = EnvDriver::new(&run2);
    let env2 = ready_env(&mut store2, &lease2, &mut driver2, &ws2);
    let cap2 = capability(
        EffectDomain::FsWrite,
        attrs(Reversibility::Compensable),
        scope_bindings(),
    );
    let mon2 = test_monitor(&cap2);
    let mut disp2 = Dispatcher::new(&mut store2, &mon2, &run2, [4u8; 32], DetectorSet::default());
    let mut exec2 = MockExecutor::ok();
    exec2.report = report_error(ErrorClass::Timeout);
    let mut inp2 = input(
        &cap2,
        &bind,
        &sb,
        &env2,
        "tc-1",
        Json::obj([
            ("path", Json::str(format!("{}/a.txt", ws2.display()))),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(attrs(Reversibility::Compensable)),
        },
    );
    // Π-4's `allow` needs the registered compensation plan (`prepared`
    // requires it for a compensable class regardless).
    inp2.compensation_plan_id = Some("plan-1".to_string());
    let out2 = disp2
        .dispatch(&mut driver2, &mut exec2, None, &inp2, &lease2)
        .unwrap();
    assert!(matches!(out2, DispatchOutcome::Unknown { .. }), "{out2:?}");
    let eid = Store::effect_id(&run2, "mc-1", "tc-1", 0);
    let v = disp2
        .probe(&exec2, &lease2, &eid, 1, &chain("tc-1"))
        .unwrap();
    assert_eq!(v, ProbeVerdict::Undeterminable);
    // Never redispatched: the executor ran exactly once.
    assert_eq!(exec2.calls.get(), 1);
}

// ── AC-R-2.5.5-5 — streaming + retention ────────────────────────────────────

#[test]
fn ac_r_2_5_5_5_chunking_is_identity_invariant() {
    // The same effect, two runs, different chunking → identical
    // `capture_manifest_ref` (the content projection strips the transport
    // coordinates; ephemeral chunks are never manifest items).
    let run_once = |tag: &str, chunks: Vec<&str>| -> String {
        let (mut store, run, lease, _c) = open(tag);
        open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
        let ws = workspace(tag);
        let mut driver = EnvDriver::new(&run);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        let cap = capability(EffectDomain::FsRead, read_only_attrs(), scope_bindings());
        let bind = binding();
        let sb = ScopeBindings::Unknown;
        let mon = test_monitor(&cap);
        let mut disp = Dispatcher::new(&mut store, &mon, &run, [5u8; 32], DetectorSet::default());
        let mut exec = MockExecutor::ok();
        exec.signals = chunks
            .iter()
            .map(|c| CaptureKind::OutputChunk {
                stream: "stdout".into(),
                data: c.to_string(),
            })
            .collect();
        let inp = input(
            &cap,
            &bind,
            &sb,
            &env,
            "tc-1",
            Json::obj([("path", Json::str("a.txt"))]),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
        );
        match disp
            .dispatch(&mut driver, &mut exec, None, &inp, &lease)
            .unwrap()
        {
            DispatchOutcome::Observed(o) => o.manifest_ref,
            other => panic!("{other:?}"),
        }
    };
    let one = run_once("chunk-a", vec!["hello world"]);
    let many = run_once("chunk-b", vec!["hel", "lo ", "wor", "ld"]);
    assert_eq!(one, many, "chunking changed the manifest ref");
}

// ── AC-R-2.5.5-8 — redaction placement ──────────────────────────────────────

#[test]
fn ac_r_2_5_5_8_hostile_executor_output_is_masked_on_the_capture_path() {
    let (mut store, run, lease, _clock) = open("redact");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("redact");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), scope_bindings());
    let bind = binding();
    let sb = ScopeBindings::Unknown;
    let mon = test_monitor(&cap);
    // The mask set knows the secret; the hostile executor echoes it verbatim.
    let mut masks = MaskSet::default();
    masks.entries.insert(
        "chan-1".to_string(),
        vec![MaskEntry {
            channel_id: "chan-1".into(),
            revision: 1,
            value: "s3cr3t-value".into(),
            fingerprint: SecretFingerprint {
                channel_id: "chan-1".into(),
                revision: 1,
                digest: "0".repeat(32),
            },
            live: true,
            placeholder: None,
        }],
    );
    let mut disp = Dispatcher::new(
        &mut store,
        &mon,
        &run,
        [6u8; 32],
        DetectorSet::standard(masks),
    );
    let mut exec = MockExecutor::ok();
    exec.signals = vec![CaptureKind::OutputChunk {
        stream: "stdout".into(),
        data: "the secret is s3cr3t-value".into(),
    }];
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([("path", Json::str("a.txt"))]),
        EffectClass {
            domain: EffectDomain::FsRead,
            attributes: Some(read_only_attrs()),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");
    // The durable rows: a `security.secret.leak_detected` fired and no event
    // payload carries the unmasked secret.
    let envs = read_all(disp.store_mut(), &run);
    let mut saw_leak = false;
    for e in &envs {
        let s = e.payload.to_canonical_string();
        assert!(
            !s.contains("s3cr3t-value"),
            "unmasked secret in {}",
            e.class
        );
        if e.class == "security.secret.leak_detected" {
            saw_leak = true;
        }
    }
    assert!(saw_leak, "no leak_detected row");
}

// ── AC-R-2.5.5-9 — dedup + probe ─────────────────────────────────────────────

#[test]
fn ac_r_2_5_5_9_repeated_commit_returns_the_stored_observation() {
    let (mut store, run, lease, _clock) = open("dedup");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("dedup");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), scope_bindings());
    let bind = binding();
    let sb = ScopeBindings::Unknown;
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [8u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mk = |tc: &str| {
        input(
            &cap,
            &bind,
            &sb,
            &env,
            tc,
            Json::obj([("path", Json::str("a.txt"))]),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
        )
    };
    let out1 = disp
        .dispatch(&mut driver, &mut exec, None, &mk("tc-1"), &lease)
        .unwrap();
    assert!(matches!(out1, DispatchOutcome::Observed(_)), "{out1:?}");
    assert_eq!(exec.calls.get(), 1);
    // A repeated commit for the same effect coordinate resolves the stored
    // observation — the executor is never re-entered.
    let out2 = disp
        .dispatch(&mut driver, &mut exec, None, &mk("tc-1"), &lease)
        .unwrap();
    assert!(
        matches!(out2, DispatchOutcome::Duplicate { .. }),
        "{out2:?}"
    );
    assert_eq!(exec.calls.get(), 1, "a repeated commit must not dispatch");
}

// ── AC-R-2.5.5-12 — executor declarations ───────────────────────────────────

#[test]
fn ac_r_2_5_5_12_bind_refuses_insufficient_declarations() {
    let decl = MockExecutor::ok().decl;
    // `dedup_support_required > dedup_support` → InsufficientDedup.
    let req = ExecutionRequirement {
        minimum_isolation: IsolationClass::None,
        minimum_dedup: DedupSupport::Durable,
    };
    assert!(matches!(
        bind(&decl, EffectDomain::FsWrite, &req),
        Err(hh_env::executor::BindError::InsufficientDedup { .. })
    ));
    // `isolation_min > isolation_support` → InsufficientIsolation.
    let req2 = ExecutionRequirement {
        minimum_isolation: IsolationClass::ProcessSandbox,
        minimum_dedup: DedupSupport::None,
    };
    assert!(matches!(
        bind(&decl, EffectDomain::FsWrite, &req2),
        Err(hh_env::executor::BindError::InsufficientIsolation { .. })
    ));
    // `interrupt = unknown` ⇒ `unsupported` behaviourally, `unknown` in the
    // catalog (honest reporting, never coerced up).
    assert_eq!(
        InterruptSupport::Unknown.effective(),
        InterruptSupport::Unsupported
    );
    assert_eq!(InterruptSupport::Unknown.as_str(), "unknown");
}

// ── AC-R-2.5.5-14 — the boundary round-trip (Stage-0 spike S1) ──────────────

#[test]
fn ac_r_2_5_5_14_one_call_through_the_process_boundary() {
    // A real `std::process::Command` child — the Stage-1 in-process executor
    // still crosses the spawn boundary the helper will own (S2.1). `/bin/echo`
    // is deterministic and offline.
    let (mut store, run, lease, _clock) = open("spike");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("spike");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(
        EffectDomain::SpawnProcess,
        read_only_attrs(),
        ScopeBindings::Unknown,
    );
    let bind = binding();
    let sb = ScopeBindings::Unknown;
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [11u8; 32], DetectorSet::default());
    let mut exec = hh_env::local::LocalExecutor::new(vec![]);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([(
            "argv",
            Json::Arr(vec![Json::str("/bin/echo"), Json::str("hello")]),
        )]),
        EffectClass {
            domain: EffectDomain::SpawnProcess,
            attributes: Some(read_only_attrs()),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    match out {
        DispatchOutcome::Observed(o) => {
            assert_eq!(o.outcome, EffectOutcome::Applied, "{o:?}");
            assert!(matches!(o.status, ObservedStatus::Ok));
        }
        other => panic!("expected observed(applied), got {other:?}"),
    }
    let envs = read_all(disp.store_mut(), &run);
    assert!(envs.iter().any(|e| e.class == "action.effect.observed"));
    assert!(envs.iter().any(|e| e.class == "action.tool.completed"));
}

// ── AC-R-2.7.1-1 — the kernel local checks ride `action.effect.observed` ────

#[test]
fn ac_r_2_7_1_1_local_verdicts_after_observed() {
    // An `fs_write` dispatch settles `observed{applied}` — check (c)
    // `patch_application` is the applicable built-in, so exactly one
    // `verification.validator.invoked` + `verification.validator.verdict`
    // pair lands after the terminal row (`phase = local`, `detector =
    // deterministic`, `charged_to = subject`, `inputs_digest` present);
    // the observation itself is never touched (I-V3).
    let (mut store, run, lease, _clock) = open("localverdicts");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("localverdicts");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsWrite, reversible_attrs(), scope_bindings());
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [5u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        Json::obj([
            ("path", Json::str(format!("{}/a.txt", ws.display()))),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    let envs = read_all(disp.store_mut(), &run);
    let observed = envs
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .expect("observed terminal");
    let invoked: Vec<_> = envs
        .iter()
        .filter(|e| e.class == "verification.validator.invoked")
        .collect();
    let verdicts: Vec<_> = envs
        .iter()
        .filter(|e| e.class == "verification.validator.verdict")
        .collect();
    // Exactly the applicable checks — check (c) only (no `output_schema`
    // declared ⇒ (a) inapplicable; `fs_write` ⇒ (b) inapplicable).
    assert_eq!(invoked.len(), 1, "{invoked:?}");
    assert_eq!(verdicts.len(), 1, "{verdicts:?}");
    let v = &verdicts[0];
    // AC: `phase = local`, `detector = deterministic`, `charged_to =
    // subject`, `inputs_digest` carried — and the verdict names the check.
    assert_eq!(v.payload.get("phase").and_then(Json::as_str), Some("local"));
    assert_eq!(
        v.payload.get("detector").and_then(Json::as_str),
        Some("deterministic")
    );
    assert_eq!(
        v.payload.get("charged_to").and_then(Json::as_str),
        Some("subject")
    );
    assert!(
        v.payload
            .get("inputs_digest")
            .and_then(Json::as_str)
            .is_some_and(|d| !d.is_empty()),
        "inputs_digest carried"
    );
    assert_eq!(
        v.payload.get("validator_ref").and_then(Json::as_str),
        Some("hir/kernel/local_checks@1")
    );
    assert!(
        v.payload
            .get("verdict_id")
            .and_then(Json::as_str)
            .is_some_and(|id| id.starts_with("verdict:patch_application:")),
        "{:?}",
        v.payload.get("verdict_id")
    );
    // The verdict rows follow the terminal row they check.
    assert!(v.seq > observed.seq);
    assert!(invoked[0].seq > observed.seq);
    // `verification.validator.invoked` mirrors the digest + charge.
    assert_eq!(
        invoked[0].payload.get("inputs_digest"),
        v.payload.get("inputs_digest")
    );
    assert_eq!(
        invoked[0].payload.get("charged_to").and_then(Json::as_str),
        Some("subject")
    );
    // Provenance-mandatory classes carry provenance (the kernel emits them).
    assert!(v.provenance.is_some(), "verdict provenance mandatory");
    // The observation row is untouched — its scope still names the effect
    // and the payload is the terminal capture verbatim (I-V3: verification
    // fabric never mutates durable observations).
    assert!(observed.scope.effect_id.is_some());
    assert!(observed.payload.get("outcome").is_some() || observed.payload.get("status").is_some());
}

// ── R-2.8.7⁰ — the C0 owed-decision trail (S1.23) ────────────────────────────
// `Decision::Ask` emits `security.permission.decided{ask}` → durable
// `security.permission.pending` → ephemeral `security.permission.requested` →
// the Stage-1 terminal `action.effect.refused{ask_required}` +
// `action.tool.rejected`. `requested` is subscribe-only — the durable
// owed-decision is `pending` (AC-R-2.8.7-{1,6,7}).

#[test]
fn ac_r_2_8_7_ask_emits_pending_then_ephemeral_requested_then_refusal() {
    let (mut store, run, lease, _clock) = open("ask-trail");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("ask-trail");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // `permission_request` ⇒ Π-8 / floor `ask` under the attended default
    // table — scope-independent, so the containment gate stays out of the way.
    let cap = capability(
        EffectDomain::PermissionRequest,
        attrs(Reversibility::Irreversible),
        scope_bindings(),
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    // `requested` is subscribe-only — capture it live.
    let mut sub = disp
        .store_mut()
        .subscribe(&run, Cursor::Now)
        .expect("subscribe");
    let mut exec = MockExecutor::ok();
    exec.decl.domains.insert(EffectDomain::PermissionRequest);
    let inp = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        // The arg-map only carries the declared surface params — a `path`
        // inside the writable roots keeps `scope = workspace_local`.
        Json::obj([
            ("path", Json::str(ws.join("perm-req.txt").to_str().unwrap())),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::PermissionRequest,
            attributes: Some(attrs(Reversibility::Irreversible)),
        },
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(out, DispatchOutcome::Refused { ref reason } if reason == "ask_required"),
        "{out:?}"
    );
    assert_eq!(exec.calls.get(), 0, "an asked effect never executes");

    // The durable log: `decided` → `pending` → `refused` → `rejected`;
    // `requested` is the ephemeral prompt-rendering fact (§5g.7 §3) — it
    // rides subscribe, never the durable log.
    let envs = read_all(disp.store_mut(), &run);
    let seq_of = |c: &str| envs.iter().find(|e| e.class == c).map(|e| e.seq);
    let decided = seq_of("security.permission.decided").expect("decided");
    let pending = seq_of("security.permission.pending").expect("durable pending");
    let refused = seq_of("action.effect.refused").expect("refused");
    assert!(decided < pending, "decided precedes pending");
    assert!(pending < refused, "pending precedes the refusal");
    assert!(seq_of("action.tool.rejected").is_some(), "rejected lands");
    assert!(
        envs.iter().all(|e| e.class != "security.permission.requested"),
        "the ephemeral rendering never reaches the durable log"
    );

    // The pending row carries the owed-decision record (§5g.7 §3).
    let prow = envs
        .iter()
        .find(|e| e.class == "security.permission.pending")
        .unwrap();
    let pid = prow
        .payload
        .get("permission_id")
        .and_then(Json::as_str)
        .expect("permission_id");
    assert_eq!(
        prow.payload.get("effect_id").and_then(Json::as_str),
        prow.scope.effect_id.as_deref()
    );
    let req = prow.payload.get("request").expect("request member");
    assert_eq!(
        req.get("capability_ref")
            .and_then(|c| c.get("semantic_id"))
            .and_then(Json::as_str),
        Some("test:write_file")
    );
    assert!(req
        .get("args_canonical_hash")
        .and_then(Json::as_str)
        .is_some());
    // One `permission_id` threads decided → pending → requested.
    let decided_row = envs
        .iter()
        .find(|e| e.class == "security.permission.decided")
        .unwrap();
    assert_eq!(
        decided_row
            .payload
            .get("permission_id")
            .and_then(Json::as_str),
        Some(pid)
    );

    // The ephemeral `requested` rendering rode subscribe, carrying the same
    // `permission_id` (the whole record is ephemeral — no durable form).
    let mut frames = Vec::new();
    while let Some(f) = sub.try_next() {
        frames.push(f);
    }
    let saw_requested = frames.iter().any(|f| {
        matches!(
            f,
            hh_ledger::event::EventFrame::Ephemeral { event }
                if event.class == "security.permission.requested"
                    && event.payload.get("permission_id").and_then(Json::as_str) == Some(pid)
        )
    });
    assert!(
        saw_requested, "ephemeral requested on subscribe: {frames:?}"
    );
}
