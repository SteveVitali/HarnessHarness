//! R2.6 — the env-owned kill half (DF-S1.20-1's `kill` member): when a
//! tool outlives its attempt deadline the *environment* kills the
//! in-flight process and the run records the honest terminal —
//! `unknown`, never `applied`/`not_applied` guesses over a process that
//! provably ran. The mock-executor battery (`ac_r_2_5_5_4`) pins the
//! dispatcher's mapping; this battery pins the *real* kill:
//!
//! - `LocalExecutor` spawns a real `/bin/sleep` child, the deadline
//!   ladder's effective bound fires `child.kill()` inside `drain_child`,
//!   the terminal report is `tool_error{timeout}` with
//!   `outcome_hint = "unknown"` and no exit status, the captured
//!   `process{spawned}` pid is reaped (the kill is real, not a report —
//!   the call returns orders of magnitude inside the child's own
//!   runtime), and the executor never fabricates a class the caller did
//!   not see die.
//! - The same kill through `Dispatcher::dispatch` on a compensable
//!   class settles `DispatchOutcome::Unknown` with the durable
//!   `action.effect.unknown` row — the settlement the control driver
//!   folds into `effects_settled{outcome: unknown}`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use hh_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hh_compiler::plan::PinnedRef;
use hh_compiler::surface::{BindingMapping, CompileExposureMode};
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{kernel_default, ExecPolicy, NetMode, ResourceLimits, WritableRoot};
use hh_env::capture::{CaptureKind, OutputPolicy};
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::events::{EventMinter, ScopeChain};
use hh_env::executor::{ExecutionRequest, TerminalStatus, ToolExecutor};
use hh_env::handle::{OnLoss, Roots};
use hh_env::helper::CommitEvidence;
use hh_env::local::LocalExecutor;
use hh_env::observe::ErrorClass;
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_hir::refs::Ref;
use hh_hir::tools::ExposureMode;
use hh_identity::idp::address;
use hh_ledger::event::Scope;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, Monitor};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_wire::json::Json;

// ── scaffold (the acceptance battery's shape, trimmed to the kill leg) ──

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r26-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let d = dir(tag);
    std::fs::create_dir_all(&d).unwrap();
    let d = std::fs::canonicalize(&d).unwrap();
    let mut s = Store::open_with(
        d,
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
        image: ImageRef::ContentAddress(address(b"r2.6-image", "application/octet-stream")),
        build_context: None,
        platform: "test-platform".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
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

fn compensable_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Compensable,
    }
}

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
                [("path", "string"), ("command", "string"), ("argv", "array")]
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
        surface_name: "run".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:run".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:run".to_string(),
        arg_map: ["path", "command", "argv"]
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
        exposure_mode: CompileExposureMode::Primitive,
        capability_refs: vec!["test:run".to_string()],
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

fn scope_bindings() -> ScopeBindings {
    ScopeBindings::Bindings(Json::Arr(vec![Json::obj([
        ("param_path", Json::str("path")),
        ("scope_kind", Json::str("fs_path")),
    ])]))
}

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
        "test:run".to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: binding(),
        },
    );
    m
}

#[allow(clippy::too_many_arguments)]
fn dispatch_input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env_handle_id: &str,
    _ws: &std::path::Path,
    args: Json,
    declared: EffectClass,
    attempt_deadline_ms: Option<u64>,
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
            semantic_id: "test:run".to_string(),
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
            attempt_deadline_ms,
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
    }
}

/// A 60-second `sleep` (POSIX — `/bin/sleep` on every CI image).
fn hanging_argv() -> Json {
    Json::Arr(vec![Json::str("/bin/sleep"), Json::str("60")])
}

// ── the cells ───────────────────────────────────────────────────────────

/// The executor-level kill: a real child process outlives the deadline;
/// `drain_child` SIGKILLs it and the terminal report is honest —
/// `timeout`, `outcome_hint = "unknown"` (the run can not know what the
/// killed process did), `retryable = false`, no exit status. The captured
/// `process{spawned}` pid is verified dead — the kill is a kill, not a
/// spelling.
#[test]
fn r2_6_local_executor_kills_the_hanging_tool() {
    if !PathBuf::from("/bin/sleep").exists() {
        eprintln!("r2_6 kill cell: /bin/sleep absent — skipped");
        return;
    }
    let mut exec = LocalExecutor::new(vec![]);
    let mut spawned_pid: Option<String> = None;
    let mut sink = |sig: hh_env::executor::ExecutorSignal| {
        if let CaptureKind::Process { process_ref, .. } = &sig.kind {
            if let Some(pid) = process_ref.strip_prefix("pid:") {
                spawned_pid = Some(pid.to_string());
            }
        }
    };
    let req = ExecutionRequest {
        execution_id: "x-1".into(),
        effect_id: "e-1".into(),
        attempt_no: 1,
        capability_ref: ("test:run".into(), "v-cap".into()),
        effect: EffectClass::domain_only(EffectDomain::SpawnProcess),
        args: Json::obj([("argv", hanging_argv())]),
        env_handle_id: "env-1".into(),
        attribution_token: "tok-1".into(),
        deadline_ms: Some(150),
        ladder: DeadlineLadder {
            run_deadline_ms: None,
            phase_deadline_ms: None,
            effect_deadline_ms: None,
            attempt_deadline_ms: Some(150),
        },
        retain_bytes_cap: 1 << 20,
        idempotency_key: "ik-1".into(),
        commit_evidence: CommitEvidence::ReadOnly,
        request_state: None,
    };
    let started = Instant::now();
    let report = exec.execute(&req, &mut sink).expect("execute");
    let elapsed = started.elapsed();

    // The kill fired — the call returned orders of magnitude inside the
    // child's own 60-second runtime.
    assert!(
        elapsed.as_secs() < 10,
        "the deadline kill fired promptly (elapsed {elapsed:?})"
    );
    assert!(
        matches!(
            report.status,
            TerminalStatus::ToolError {
                class: ErrorClass::Timeout
            }
        ),
        "deadline kill → tool_error{{timeout}}: {:?}",
        report.status
    );
    // The honest terminal — `unknown`, never a guess about a process
    // that provably ran.
    assert_eq!(report.outcome_hint, "unknown");
    assert_eq!(report.retryable_hint, Some(false));
    assert!(
        report.exit_status.is_none(),
        "a killed child has no exit code"
    );

    // The spawned pid is really dead — reaped, not orphaned.
    let pid = spawned_pid.expect("the spawn signal carried the pid");
    let alive = std::process::Command::new("/bin/kill")
        .args(["-0", &pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(!alive, "the killed child pid {pid} is reaped");
}

/// The same kill through the dispatcher on a compensable class: the
/// settled row is `action.effect.unknown` — durable, honest, and the
/// shape the control driver's `effects_settled{outcome: unknown}` fold
/// reads (DF-S1.20-1's contract: env owns the kill, the ledger owns the
/// truth, the driver records `unknown`).
#[test]
fn r2_6_deadline_kill_settles_unknown_through_dispatch() {
    if !PathBuf::from("/bin/sleep").exists() {
        eprintln!("r2_6 kill cell: /bin/sleep absent — skipped");
        return;
    }
    let (mut store, run, lease) = open("kill-dispatch");
    open_scopes(&mut store, &run, &lease);
    let ws = workspace("kill-dispatch");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let sb = scope_bindings();
    let cap = capability(EffectDomain::FsWrite, compensable_attrs(), sb.clone());
    let bind = binding();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    // The real executor — no mock. The environment kills, not the test.
    let mut exec = LocalExecutor::new(vec![]);
    let inp = dispatch_input(
        &cap,
        &bind,
        &sb,
        &env,
        &ws,
        Json::obj([
            ("argv", hanging_argv()),
            ("path", Json::str(format!("{}/a.txt", ws.display()))),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(compensable_attrs()),
        },
        Some(150),
    );
    let started = Instant::now();
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        started.elapsed().as_secs() < 10,
        "the env killed the hanging tool promptly"
    );
    let DispatchOutcome::Unknown { cause } = &out else {
        panic!("a deadline-killed compensable effect settles unknown: {out:?}")
    };
    // The report channel carries no origin member — a declared `timeout`
    // class classifies `tool`-origin (AC-R-2.5.5-4 pins that rule), so a
    // compensable kill settles `unknown{executor_error}`. The honest
    // terminal is what matters: `unknown`, never `applied`.
    assert_eq!(
        cause, "executor_error",
        "the kill's honest cause, not a guess"
    );

    // Durable: `action.effect.unknown` appended — the settlement the
    // driver's settled-fold reads.
    let rows: Vec<_> = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "action.effect.unknown")
        .collect();
    assert_eq!(rows.len(), 1, "one durable unknown settlement: {rows:?}");
    assert_eq!(
        rows[0].payload.get("cause").and_then(Json::as_str),
        Some("executor_error"),
        "{:?}",
        rows[0].payload
    );
    // Never `applied` — the killed process's truth is unknowable.
    assert!(
        store
            .events(&run)
            .unwrap()
            .iter()
            .all(|e| e.class != "action.effect.observed"
                || e.payload.get("outcome").and_then(Json::as_str) != Some("applied")),
        "a killed tool never settles applied"
    );
}
