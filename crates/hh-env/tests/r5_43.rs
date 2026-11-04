//! R5.43 — the two fixed S1 useful-task subjects (GATE-G11's admitted
//! scope). Each frozen subject runs a real podman cell end-to-end:
//! provision/attach → mediated Dispatcher write of `submission.json`
//! inside the container → surviving observer (3 intervals + intact
//! sentinel) → bind-time `probe()` exercise (carried predicate — ENOENT
//! ⇒ withheld, never counted) → `seal_workspace` → collect → the real
//! `hh-bench-adapter` oracle through `OracleRunner` (invoked durable
//! before verdict). Controls diverge for real; the declared §9.2
//! worksheet builds through `build_quote`. Typed verdicts only —
//! inconclusive backend/binaries leave typed inconclusive legs, never a
//! skip counted as pass. Offline, `target/` scratch, USD 0.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_bench::benchset::Benchset;
use hharness_bench::oracle::{
    AttemptSpec, InstrumentPolicy, OracleExecutor, OracleObservation, OracleRunner,
    SealedSubmission,
};
use hharness_bench::records::Submission;
use hharness_bench::BenchmarkAdapter;
use hharness_bench::OraclePin;
use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, NetMode, ResourceLimits, WritableRoot};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::DispatchInput;
use hharness_env::dispatch::{DispatchOutcome, Dispatcher};
use hharness_env::driver::EnvDriver;
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::ToolExecutor;
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::ops::{MarkerSink, ObservationPoint, OpsObserver, Sentinel};
use hharness_env::preflight::{evaluate_worksheet, Leg, LegVerdict, WorksheetFacts};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::useful::{
    assemble, evaluate_controls, evaluate_join, evaluate_observer_cell, evaluate_oracle,
    evaluate_subject, ControlsOutcome, Disposition, JoinFacts, ObserverCell, OracleOutcome,
    SubjectOutcome, EXEC_BACKEND_CLOBBER, LEGS, PROBE_CARRIED,
};
use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, EnvelopeQty, InputState, PrereqStatus, Prerequisite,
    QuoteDecl, ResourceLine, StageDecl, COMPONENTS, ENVELOPE_COMPONENTS, PINS, PREREQUISITES,
};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_ledger::event::Scope;
use hharness_ledger::ids::{effect_id, ManualClock, SeqIds};
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_ontology::lab::VerifierIsolation;
use hharness_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

// ── scaffold (mirrors r5_42a.rs / helper_live.rs) ──────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = repo_root()
        .join("target")
        .join(format!("r5_43-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn open_instrument(store: &mut Store) -> (String, Lease) {
    let mut m = RunManifest::minimal(RunKind::Experiment);
    m.configuration_id = None;
    m.configuration_version_id = None;
    store.open_run(m, "s1-instrument").unwrap()
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

fn compensable_attrs() -> EffectAttributes {
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
                attributes: Some(compensable_attrs()),
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
            version_id: "v-perm".into(),
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
        declared: EffectClass {
            domain: EffectDomain::Exec,
            attributes: Some(compensable_attrs()),
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
        compensation_plan_id: Some("plan-1".to_string()),
        baseline_ref: None,
        flow: Default::default(),
        remedy_taken: None,
    }
}

// ── the executed oracle — the committed `hh-bench-adapter` binary ──────

fn bench_adapter_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HH_BENCH_ADAPTER_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent().and_then(|d| d.parent()) {
            let c = d.join("hh-bench-adapter");
            if c.exists() {
                return Some(c);
            }
        }
    }
    let c = repo_root().join("target/debug/hh-bench-adapter");
    c.exists().then_some(c)
}

/// `AdapterOracle` — the `OracleExecutor` port that runs the unchanged
/// `adapter_a` verifier for real: the committed `hh-bench-adapter`
/// binary grades the sealed artifact bytes in a separate process
/// (`oracle_digest` pins the actual binary bytes executed).
struct AdapterOracle {
    bin: PathBuf,
    digest: String,
    root: PathBuf,
    /// `Some(bad_task)` drives the broken control — a task the adapter
    /// refuses (exit 2 envelope ⇒ `oracle_failure`), honestly.
    bad_task: Option<String>,
}

impl AdapterOracle {
    fn new(root: &Path, bad_task: Option<String>) -> Option<AdapterOracle> {
        let bin = bench_adapter_bin()?;
        let bytes = std::fs::read(&bin).ok()?;
        Some(AdapterOracle {
            bin,
            digest: sha256_hex(&bytes),
            root: root.to_path_buf(),
            bad_task,
        })
    }
}

/// `adapter_task_id(task)` — the `hh-bench-adapter` fixture id matching
/// the corpus record's simple name (`suite.tb2/a-hello` → `a-hello`), so
/// the executed binary's `grade` op resolves the task. `None` when the
/// adapter has no fixture by that name (honest fallback to the corpus id).
fn adapter_task_id(task: &hharness_lab::bench::TaskRecord) -> Option<String> {
    let simple = task.foreign.name.rsplit('/').next().unwrap_or("");
    let adapter = hharness_bench::adapters::FixtureAdapter::adapter_a();
    adapter.task_ids().into_iter().find(|id| {
        adapter
            .task(id)
            .map(|t| t.foreign.name == simple)
            .unwrap_or(false)
    })
}

impl OracleExecutor for AdapterOracle {
    fn oracle_digest(&self) -> &str {
        &self.digest
    }
    fn execute(
        &self,
        task: &hharness_lab::bench::TaskRecord,
        submission: &Submission,
    ) -> OracleObservation {
        let start = std::time::Instant::now();
        // The sealed artifact is what the verifier reads — write the
        // collected bytes into the verifier root's submission slot.
        let payload: Vec<u8> = submission
            .payload_hex
            .as_bytes()
            .chunks(2)
            .filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap_or("00"), 16).ok())
            .collect();
        let _ = std::fs::write(self.root.join("submission.json"), &payload);
        // The committed `hh-bench-adapter` resolves only its own fixture
        // `task_id`s (`sha256:*`); the corpus record's semantic id is a
        // distinct record identity (suite-scoped name/instruction). The
        // request carries the adapter-resolvable fixture id for the same
        // task name — the frozen bind stays the corpus record upstream
        // (`AttemptSpec.task`, task sha256 facts) while `oracle_task_id`
        // records the executed id verbatim.
        let task_id = self
            .bad_task
            .clone()
            .or_else(|| adapter_task_id(task))
            .unwrap_or_else(|| task.task_id.clone());
        let req = Json::obj([
            ("adapter", Json::str("adapter_a")),
            ("op", Json::str("grade")),
            (
                "handle",
                Json::obj([
                    ("handle_id", Json::str("s1-oracle")),
                    ("task_id", Json::str(task_id)),
                    ("family", Json::str("coding_terminal")),
                    ("root", Json::str(self.root.display().to_string())),
                    ("network_mode", Json::str("none")),
                    ("surface", Json::str("search")),
                ]),
            ),
        ])
        .to_canonical_string();
        let spawn = std::process::Command::new(&self.bin)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match spawn {
            Ok(c) => c,
            Err(e) => {
                return OracleObservation {
                    exit_code: None,
                    timed_out: false,
                    stdout: String::new(),
                    stderr: format!("oracle spawn: {e}"),
                    environment_error: Some(format!("oracle spawn: {e}")),
                    wall_ms: start.elapsed().as_millis() as u64,
                }
            }
        };
        let _ = child.stdin.take().map(|mut s| s.write_all(req.as_bytes()));
        let out = child.wait_with_output().unwrap();
        let wall_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let code = out.status.code();
        // The real verifier output → the typed-reward shape the grade
        // pipeline parses. Exit 0 carries `{"grade":{"reward_ppm":…}}`;
        // anything else is the verbatim error envelope/exit signature.
        let reward_out = if code == Some(0) {
            hharness_wire::json::parse(stdout.trim())
                .ok()
                .and_then(|j| {
                    j.get("grade")
                        .and_then(|g| g.get("reward_ppm"))
                        .and_then(Json::as_int)
                })
                .map(|ppm| format!("{{\"reward\":{ppm}}}"))
                .unwrap_or(stdout.clone())
        } else {
            stdout.clone()
        };
        OracleObservation {
            exit_code: code,
            timed_out: false,
            stdout: reward_out,
            stderr,
            environment_error: None,
            wall_ms,
        }
    }
}

// ── the subject cell drive ───────────────────────────────────────────

/// One subject's real podman cell: provision/attach → mediated write of
/// the instruction-declared submission → observer intervals → bind-time
/// `probe()` exercise (recorded verbatim — never counted) → seal →
/// collect the artifact. `≤1` diagnostic setup retry, new identity —
/// every attempt recorded in `out.attempts`.
/// `sweep_hh_env_containers` — remove leaked `hh-env-*` containers before
/// a drive. A helper SIGKILLed mid-teardown (or a panicked cell) leaves
/// its `--name hh-env-<env_id>` container behind; env ids are
/// deterministic per fresh store, so residue collides on the next
/// `podman create` (recorded machinery gap — the sweep is hygiene, never
/// a hidden retry: every setup attempt is still recorded verbatim).
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

fn drive_subject(
    subject_id: &str,
    task: &str,
    submission_text: &str,
    retry_setup: bool,
) -> (
    SubjectOutcome,
    Option<Vec<u8>>,
    Option<(Store, String, String)>,
) {
    let mut out = SubjectOutcome {
        subject_id: subject_id.to_string(),
        task: task.to_string(),
        attempted: false,
        backend: "podman".to_string(),
        dispatch: None,
        executor_backend: None,
        artifact_sha256: None,
        artifact_bytes: None,
        intervals: 0,
        sentinel_intact: None,
        observer_survived: false,
        probe: None,
        seal: None,
        seal_event_id: None,
        meters_event_id: None,
        setup_retries: 0,
        attempts: vec![],
        failure: None,
    };
    let mut artifact: Option<Vec<u8>> = None;
    let mut keep: Option<(Store, String, String)> = None;
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        return (out, artifact, keep);
    }
    sweep_hh_env_containers();
    let tries = if retry_setup { 2 } else { 1 };
    for attempt in 0..tries {
        let cell_id = format!("{subject_id}-c{attempt}");
        out.attempts.push(cell_id.clone());
        out.attempted = true;
        let (mut store, run, lease, _clock) = open(&cell_id);
        open_scopes(&mut store, &run, &lease);
        let mut driver = EnvDriver::new(&run);
        let ws = dir(&cell_id).join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let ws = std::fs::canonicalize(&ws).unwrap();
        // Setup leg — provision+attach spawns the real container.
        let env_id = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ready_cell(&mut store, &lease, &mut driver, &ws)
        })) {
            Ok(id) => id,
            Err(_) => {
                if attempt == 0 && tries == 2 {
                    out.setup_retries += 1;
                    continue;
                }
                out.failure = Some("cell_setup_panicked".to_string());
                return (out, artifact, keep);
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
        let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
        let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
        let inp = input(
            &cap,
            &bind,
            &sb,
            &env_id,
            Json::obj([(
                "argv",
                Json::Arr(vec![
                    Json::str("/bin/sh"),
                    Json::str("-c"),
                    Json::str(format!(
                        "printf '%s' '{submission_text}' > /work/submission.json"
                    )),
                ]),
            )]),
        );
        let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
        // The live session's backend verbatim — the helper's hello reply
        // is the packet's evidence when the spawned `podman` backend was
        // re-derived (`exec_backend_hello_clobber` ⇒ `seatbelt`).
        out.executor_backend = Some(exec.client.backend.clone());
        let res = disp.dispatch(&mut driver, &mut exec, None, &inp, &lease);
        observer
            .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
            .unwrap();
        out.intervals += 1;
        match res {
            Ok(DispatchOutcome::Observed(o)) => {
                out.dispatch = Some(format!("observed:{}", o.outcome.as_str()));
            }
            Ok(DispatchOutcome::Refused { .. }) => {
                out.dispatch = Some("refused".to_string());
            }
            Ok(DispatchOutcome::Unknown { cause }) => {
                out.dispatch = Some(format!("unknown:{cause}"));
            }
            Ok(_) => out.dispatch = Some("other".to_string()),
            Err(e) => {
                out.failure = Some(format!("dispatch:{e}"));
                let _ = driver.return_executor(&env_id, exec);
                let _ = driver.teardown(&mut store, &lease, &env_id);
                return (out, artifact, keep);
            }
        }
        // The bind-time probe exercise — the verb's verbatim result is
        // recorded; ENOENT (the carried machinery gap) stays withheld.
        let eid = effect_id(&run, "mc-1", "tc-1", 0);
        out.probe = Some(match exec.probe(&eid, 0) {
            Ok(v) => format!("ok:{}", v.as_str()),
            Err(e) => format!("error:{e}"),
        });
        // Seal: end-state pin + access revocation + meters on the
        // subject run.
        driver.return_executor(&env_id, exec);
        let seals = driver.seal_workspace(&mut store, &lease);
        if let Some(s) = seals.iter().find(|s| s.env_handle_id == env_id) {
            out.seal = Some(s.status.clone());
            out.seal_event_id = s.end_state_event_ref.clone();
            out.meters_event_id = s.usage_event_ref.clone();
        } else {
            out.seal = Some("absent".to_string());
        }
        observer
            .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
            .unwrap();
        out.intervals += 1;
        out.sentinel_intact = Some(sentinel.intact());
        out.observer_survived = observer.rows().map(|r| !r.is_empty()).unwrap_or(false);
        artifact = std::fs::read(ws.join("submission.json")).ok();
        if let Some(b) = &artifact {
            out.artifact_sha256 = Some(sha256_hex(b));
            out.artifact_bytes = Some(b.len() as u64);
        }
        let _ = driver.teardown(&mut store, &lease, &env_id);
        if let Some(id) = out.seal_event_id.clone() {
            keep = Some((store, run, id));
        }
        return (out, artifact, keep);
    }
    (out, artifact, keep)
}

// ── oracle chain + controls ─────────────────────────────────────────

fn task(name: &str) -> hharness_lab::bench::TaskRecord {
    Benchset::load_default()
        .expect("committed benchset loads")
        .suites
        .values()
        .flat_map(|s| s.tasks.iter())
        .find(|t| t.name == name)
        .map(|t| t.record().clone())
        .expect("corpus task record")
}

fn submission(task: &hharness_lab::bench::TaskRecord, payload: &[u8]) -> Submission {
    let mut s = Submission {
        submission_id: String::new(),
        task_id: task.task_id.clone(),
        artifact_refs: vec![],
        payload_hex: payload.iter().map(|b| format!("{b:02x}")).collect(),
        applied: true,
        apply_error: None,
    };
    s.submission_id = s.compute_id();
    s
}

fn policy() -> InstrumentPolicy {
    InstrumentPolicy {
        policy_ref: "policy/instrument-s1".to_string(),
        max_attempts: 3,
        wall_ms_bound: 30_000,
        output_bytes_bound: 64 * 1024,
    }
}

/// Mint the `submission_seal` pin on a run and return its EventRef —
/// the control path's seal (the subjects' seals come from the real
/// `seal_workspace`).
fn mint_seal(store: &mut Store, run: &str, lease: &Lease) -> EventRef {
    let m = EventMinter::new(store, run);
    let e = m
        .mint(
            "action.environment.snapshot",
            Json::obj([("purpose", Json::str("submission_seal"))]),
        )
        .unwrap();
    let id = e.event_id.clone();
    store.append(run, lease, vec![e]).unwrap();
    EventRef {
        run_id: run.to_string(),
        event_id: id,
    }
}

/// One real oracle attempt over a sealed submission — `OracleRunner` on
/// the instrument run, `AdapterOracle` executing `hh-bench-adapter` for
/// real. Returns the outcome fold for the packet leg.
fn grade_attempt(
    store: &mut Store,
    instrument: &str,
    instrument_lease: &Lease,
    subject_run: &str,
    seal_ref: EventRef,
    sub: &Submission,
    task: &hharness_lab::bench::TaskRecord,
    oracle: &AdapterOracle,
) -> (OracleOutcome, Option<hharness_bench::oracle::OracleAttempt>) {
    let mut out = OracleOutcome {
        subject_id: subject_run.to_string(),
        attempted: true,
        invoked_before_verdict: None,
        outcome: None,
        reward_ppm: None,
        attempt_ref: None,
        verdict_ref: None,
        charged_to: None,
        failure: None,
    };
    let spec = AttemptSpec {
        subject_run_id: subject_run.to_string(),
        submission_ref: sub.compute_id(),
        oracle: OraclePin {
            oracle_ref: task.instrument.grader.adapter_ref.clone(),
            oracle_digest: oracle.digest.clone(),
        },
        policy: policy(),
        instrument_budget_ref: "budget/instrument-s1".to_string(),
        task: task.clone(),
        isolation: VerifierIsolation::Separate,
    };
    let sealed = SealedSubmission {
        submission_ref: sub.compute_id(),
        seal_ref,
        submission: sub.clone(),
    };
    let mut runner = OracleRunner::new(store, instrument, instrument_lease, spec);
    match runner.attempt(&sealed, oracle) {
        Ok(a) => {
            out.invoked_before_verdict = Some(
                store
                    .events(instrument)
                    .map(|ev| {
                        let inv = ev
                            .iter()
                            .find(|e| e.event_id == a.invocation_ref.event_id)
                            .map(|e| e.seq);
                        let ver = ev
                            .iter()
                            .find(|e| e.event_id == a.verdict_ref.event_id)
                            .map(|e| e.seq);
                        inv.is_some() && ver.is_some() && inv < ver
                    })
                    .unwrap_or(false),
            );
            match &a.outcome {
                hharness_bench::oracle::AttemptOutcome::Decided { reward_ppm, .. } => {
                    out.outcome = Some("decided".to_string());
                    out.reward_ppm = Some(*reward_ppm);
                }
                hharness_bench::oracle::AttemptOutcome::OracleFailure { .. } => {
                    out.outcome = Some("oracle_failure".to_string());
                }
            }
            out.attempt_ref = Some(a.attempt_ref.clone());
            out.verdict_ref = Some(format!(
                "{}:{}",
                a.verdict_ref.run_id, a.verdict_ref.event_id
            ));
            out.charged_to = Some("instrument".to_string());
            (out, Some(a))
        }
        Err(e) => {
            out.failure = Some(format!("{e}"));
            (out, None)
        }
    }
}

// ── the declared §9.2 minimum-admission worksheet (HUMAN-H8, verbatim) ─

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

// ── the full drive ───────────────────────────────────────────────────

struct Drive {
    legs: Vec<Leg>,
    oracle_outcomes: Vec<OracleOutcome>,
    attempts: Vec<String>,
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

/// Run the whole admitted execution once: both subject cells, the real
/// oracle chain, the three controls, the observer fold, the join and
/// the worksheet — honest outcomes folded into legs.
fn run_s1() -> Drive {
    let mut all_attempts: Vec<String> = vec![];
    // ── subject cells (≤1 setup retry bound, new identity each) ──────
    let (sa, art_a, seal_a) =
        drive_subject("subject_a", "a-hello", "{\"answer\":\"hello\"}", false);
    let (sb, art_b, seal_b) = drive_subject("subject_b", "a-sum", "{\"answer\":5}", false);
    all_attempts.extend(sa.attempts.clone());
    all_attempts.extend(sb.attempts.clone());
    let subject_facts = |s: &SubjectOutcome, art: &Option<Vec<u8>>| {
        vec![
            ("task", s.task.clone()),
            ("backend", s.backend.clone()),
            (
                "executor_backend",
                s.executor_backend.clone().unwrap_or_else(|| "none".into()),
            ),
            (
                "dispatch",
                s.dispatch.clone().unwrap_or_else(|| "none".into()),
            ),
            (
                "artifact_sha256",
                s.artifact_sha256.clone().unwrap_or_else(|| "none".into()),
            ),
            ("intervals", s.intervals.to_string()),
            (
                "probe",
                s.probe.clone().unwrap_or_else(|| "unexercised".into()),
            ),
            ("seal", s.seal.clone().unwrap_or_else(|| "none".into())),
            ("setup_retries", s.setup_retries.to_string()),
            (
                "artifact",
                art.as_ref()
                    .map(|_| "collected".to_string())
                    .unwrap_or_else(|| "none".into()),
            ),
        ]
    };
    // ── oracle chain on a shared instrument run ──────────────────────
    let mut oracle_outcomes: Vec<OracleOutcome> = vec![];
    let mut join = JoinFacts {
        attempted: true,
        seal_ref_resolves: true,
        submission_digest_matches: true,
        attempt_rows_join: true,
        meters_present: true,
        cost_attribution_ok: true,
    };
    // Subject oracle attempts need the cells' artifacts + seals; the
    // three controls are instrument-side — they need only the real
    // adapter binary (an honest `inconclusive` cell never starves them).
    let subjects_ok = sa.artifact_sha256.is_some()
        && sb.artifact_sha256.is_some()
        && seal_a.is_some()
        && seal_b.is_some();
    if subjects_ok && bench_adapter_bin().is_some() {
        // Each subject's own store carries its subject run (seal/detach/
        // meters) and now its instrument run (invoked/verdict) — the
        // landed Rig topology, never a second authoritative store.
        for ((s, art, keep), tname) in [
            ((&sa, &art_a, seal_a), "a-hello"),
            ((&sb, &art_b, seal_b), "a-sum"),
        ] {
            let t = task(tname);
            let payload = art.clone().unwrap();
            let sub = submission(&t, &payload);
            if Some(sha256_hex(&payload)) != s.artifact_sha256 {
                join.submission_digest_matches = false;
            }
            let (mut store, subject_run, seal_event_id) = keep.unwrap();
            let (instrument, instrument_lease) = open_instrument(&mut store);
            let seal_ref = EventRef {
                run_id: subject_run.clone(),
                event_id: seal_event_id,
            };
            let oroot = dir("s1-oracle");
            let oracle = AdapterOracle::new(&oroot, None).expect("adapter binary resolves");
            let (oo, _attempt) = grade_attempt(
                &mut store,
                &instrument,
                &instrument_lease,
                &subject_run,
                seal_ref,
                &sub,
                &t,
                &oracle,
            );
            all_attempts.push(format!("oracle-{}", s.subject_id));
            oracle_outcomes.push(oo);
        }
    }
    // The join members evaluate what actually exists: seals/meters land
    // host-side per subject; a missing artifact or absent subject oracle
    // attempt names itself (never a vacuous pass).
    join.submission_digest_matches = join.submission_digest_matches
        && sa.artifact_sha256.is_some()
        && sb.artifact_sha256.is_some();
    join.meters_present = sa.meters_event_id.is_some() && sb.meters_event_id.is_some();
    join.seal_ref_resolves = sa.seal_event_id.is_some() && sb.seal_event_id.is_some();
    join.attempt_rows_join = !oracle_outcomes.is_empty()
        && oracle_outcomes
            .iter()
            .all(|o| o.attempt_ref.is_some() && o.verdict_ref.is_some());
    let ctl_verdict = if bench_adapter_bin().is_some() {
        // ── controls through the same real oracle path ───────────────
        let (mut store, _ctl_run, ctl_lease, _c) = open("s1-ctl");
        let (instrument, instrument_lease) = open_instrument(&mut store);
        let _ = ctl_lease;
        let t = task("a-hello");
        let good = submission(&t, "{\"answer\":\"hello\"}".as_bytes());
        let seal_c = mint_seal(&mut store, &instrument, &instrument_lease);
        let oroot = dir("s1-ctl-oracle");
        let oracle = AdapterOracle::new(&oroot, None).expect("adapter binary resolves");
        let (ok, _) = grade_attempt(
            &mut store,
            &instrument,
            &instrument_lease,
            &instrument,
            seal_c.clone(),
            &good,
            &t,
            &oracle,
        );
        let wrong = submission(&t, "{\"answer\":\"HELLO\"}".as_bytes());
        let (w, _) = grade_attempt(
            &mut store,
            &instrument,
            &instrument_lease,
            &instrument,
            seal_c.clone(),
            &wrong,
            &t,
            &oracle,
        );
        let bad_root = dir("s1-broken-oracle");
        let broken = AdapterOracle::new(&bad_root, Some("no-such-task".to_string()))
            .expect("adapter binary resolves");
        let (b, _) = grade_attempt(
            &mut store,
            &instrument,
            &instrument_lease,
            &instrument,
            seal_c,
            &good,
            &t,
            &broken,
        );
        all_attempts.extend(["ctl-correct", "ctl-wrong", "ctl-broken"].map(String::from));
        let ctl = ControlsOutcome {
            attempted: true,
            correct_reward: ok.reward_ppm,
            wrong_reward: w.reward_ppm,
            broken_outcome: b.outcome.clone(),
        };
        evaluate_controls(&ctl)
    } else {
        LegVerdict::Inconclusive("controls_not_attempted".to_string())
    };
    // ── legs ─────────────────────────────────────────────────────────
    let obs = ObserverCell {
        attempted: sa.attempted || sb.attempted,
        min_intervals: sa.intervals.min(sb.intervals),
        sentinels_intact: sa.sentinel_intact == Some(true) && sb.sentinel_intact == Some(true),
        survived: sa.observer_survived && sb.observer_survived,
    };
    let oracle_verdict = if oracle_outcomes.is_empty() {
        LegVerdict::Inconclusive("oracle_not_attempted:subjects".to_string())
    } else {
        let mut v = LegVerdict::Verified;
        for o in &oracle_outcomes {
            let x = evaluate_oracle(o);
            if !x.is_verified() {
                v = x;
                break;
            }
        }
        v
    };
    let legs = vec![
        leg(
            "subject_a",
            evaluate_subject(&sa),
            &subject_facts(&sa, &art_a),
        ),
        leg(
            "subject_b",
            evaluate_subject(&sb),
            &subject_facts(&sb, &art_b),
        ),
        leg("oracle_chain", oracle_verdict, &[]),
        leg("controls", ctl_verdict, &[]),
        leg(
            "observer_probe",
            evaluate_observer_cell(&obs),
            &[
                ("probe_a", sa.probe.clone().unwrap_or_else(|| "none".into())),
                ("probe_b", sb.probe.clone().unwrap_or_else(|| "none".into())),
                ("carried", PROBE_CARRIED.to_string()),
            ],
        ),
        leg("evidence_join", evaluate_join(&join), &[]),
        leg("worksheet", evaluate_worksheet(&worksheet_facts()), &[]),
    ];
    Drive {
        legs,
        oracle_outcomes,
        attempts: all_attempts,
    }
}

// ── the battery ──────────────────────────────────────────────────────

#[test]
fn ac2_subject_cells_execute_on_podman() {
    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        let (s, _a, _e) = drive_subject("subject_a", "a-hello", "{}", false);
        assert!(!s.attempted);
        assert_eq!(
            evaluate_subject(&s),
            LegVerdict::Inconclusive("cell_not_attempted:subject_a".to_string())
        );
        return;
    }
    let (s, art, seal) = drive_subject("subject_a", "a-hello", "{\"answer\":\"hello\"}", false);
    assert_eq!(s.backend, "podman");
    assert_eq!(s.intervals, 3);
    assert_eq!(s.sentinel_intact, Some(true));
    assert!(s.observer_survived);
    assert_eq!(s.seal.as_deref(), Some("sealed"), "{s:?}");
    assert_eq!(s.setup_retries, 0);
    assert!(seal.is_some());
    // The leg's typed verdict must match the dispatch's honest terminal —
    // `observed:applied` verifies; any other outcome stays typed
    // (inconclusive/refused), never forced. On this host the diagnosed
    // `exec_backend_hello_clobber` machinery gap answers
    // `unknown:executor_error` — the designed honest leg.
    match s.dispatch.as_deref() {
        Some("observed:applied") => {
            let art = art.expect("artifact collected");
            assert_eq!(
                String::from_utf8_lossy(&art).trim(),
                "{\"answer\":\"hello\"}"
            );
            assert_eq!(evaluate_subject(&s), LegVerdict::Verified, "{s:?}");
        }
        other => {
            let v = evaluate_subject(&s);
            assert!(
                matches!(
                    v,
                    LegVerdict::Inconclusive(_)
                        | LegVerdict::Refused(_)
                        | LegVerdict::Unsupported(_)
                ),
                "dispatch {other:?} must fold to a typed non-verified verdict, got {v:?}"
            );
            assert!(art.is_none(), "no artifact without an applied dispatch");
        }
    }
    // The carried predicate: the verb is exercised and its verbatim
    // result recorded — ENOENT on this machinery stays withheld.
    let probe = s.probe.clone().unwrap_or_default();
    assert!(
        probe.starts_with("ok:") || probe.starts_with("error:"),
        "{probe}"
    );
}

#[test]
fn ac2_ac3_controls_and_oracle_chain_real() {
    if !hharness_helper::podman::is_available()
        || hharness_env::helper::helper_binary().is_none()
        || bench_adapter_bin().is_none()
    {
        eprintln!("podman/binaries unavailable — typed inconclusive legs expected");
        return;
    }
    let d = run_s1();
    for o in &d.oracle_outcomes {
        assert_eq!(o.outcome.as_deref(), Some("decided"), "{o:?}");
        assert_eq!(o.charged_to.as_deref(), Some("instrument"));
        assert_eq!(o.invoked_before_verdict, Some(true));
    }
    let report = assemble(d.legs, &d.oracle_outcomes, d.attempts).unwrap();
    // The packet is honest whatever the cells produced: when both
    // subjects applied, `executed` + `marked`; when a machinery gap
    // (`exec_backend_hello_clobber` on this host) leaves the cells
    // inconclusive, the fold stays `inconclusive`/`failed` with
    // `useful_acceptance` withheld — never a forced `executed`.
    let controls_leg = report.legs.iter().find(|l| l.name == "controls").unwrap();
    assert_eq!(controls_leg.verdict, LegVerdict::Verified, "{report:?}");
    if d.oracle_outcomes
        .iter()
        .any(|o| o.reward_ppm == Some(1_000_000))
    {
        assert_eq!(report.disposition, Disposition::Executed, "{report:?}");
        assert_eq!(report.useful_acceptance, "marked");
    } else {
        assert_ne!(report.disposition, Disposition::Executed, "{report:?}");
        assert!(
            report.useful_acceptance.starts_with("withheld"),
            "{report:?}"
        );
    }
    assert_eq!(report.admission, "bounded_s1_execution");
    assert!(report
        .carried_predicates
        .contains(&PROBE_CARRIED.to_string()));
    assert!(report
        .carried_predicates
        .contains(&EXEC_BACKEND_CLOBBER.to_string()));
}

#[test]
fn ac3_evaluators_refuse_and_inconclusive_never_executes() {
    // Controls divergence failures refuse.
    let mut c = ControlsOutcome {
        attempted: true,
        correct_reward: Some(1_000_000),
        wrong_reward: Some(0),
        broken_outcome: Some("oracle_failure".to_string()),
    };
    assert_eq!(evaluate_controls(&c), LegVerdict::Verified);
    c.wrong_reward = Some(1_000_000);
    assert_eq!(
        evaluate_controls(&c),
        LegVerdict::Refused("control_wrong:1000000".to_string())
    );
    c.wrong_reward = Some(0);
    c.broken_outcome = Some("decided".to_string());
    assert!(matches!(evaluate_controls(&c), LegVerdict::Refused(_)));
    // Subject retry bound refuses.
    let s = SubjectOutcome {
        subject_id: "subject_a".into(),
        task: "a-hello".into(),
        attempted: true,
        backend: "podman".into(),
        dispatch: Some("observed:applied".into()),
        executor_backend: Some("container".into()),
        artifact_sha256: Some("x".into()),
        artifact_bytes: Some(1),
        intervals: 3,
        sentinel_intact: Some(true),
        observer_survived: true,
        probe: Some("error:blob: ENOENT".into()),
        seal: Some("sealed".into()),
        seal_event_id: Some("e".into()),
        meters_event_id: Some("m".into()),
        setup_retries: 2,
        attempts: vec![],
        failure: None,
    };
    assert_eq!(
        evaluate_subject(&s),
        LegVerdict::Refused("retry_bound:2".to_string())
    );
    // A substitute backend refuses.
    let mut s2 = s.clone();
    s2.setup_retries = 0;
    s2.backend = "seatbelt".to_string();
    assert!(matches!(evaluate_subject(&s2), LegVerdict::Refused(_)));
    // An inconclusive leg ⇒ disposition Inconclusive, never Executed.
    let verified = |n: &str| Leg {
        name: n.to_string(),
        verdict: LegVerdict::Verified,
        facts: BTreeMap::new(),
    };
    let mk = |i: usize, v: LegVerdict| {
        LEGS.iter()
            .enumerate()
            .map(|(j, n)| {
                if j == i {
                    Leg {
                        name: n.to_string(),
                        verdict: v.clone(),
                        facts: BTreeMap::new(),
                    }
                } else {
                    verified(n)
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        assemble(mk(0, LegVerdict::Inconclusive("x".into())), &[], vec![])
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    assert_eq!(
        assemble(mk(3, LegVerdict::Refused("y".into())), &[], vec![])
            .unwrap()
            .disposition,
        Disposition::Failed
    );
    let ok = assemble(mk(0, LegVerdict::Verified), &[], vec![]).unwrap();
    assert_eq!(ok.disposition, Disposition::Executed);
    // withheld acceptance when no subject decided 1e6.
    assert!(ok.useful_acceptance.starts_with("withheld"));
    // Malformed leg sets refuse.
    assert!(assemble(mk(0, LegVerdict::Verified)[..6].to_vec(), &[], vec![]).is_err());
}

#[test]
fn ac3_wrong_is_valid_zero_broken_is_oracle_failure() {
    if bench_adapter_bin().is_none() {
        eprintln!("hh-bench-adapter unavailable — typed inconclusive expected");
        return;
    }
    let (mut store, _run, _lease, _c) = open("s1-neg");
    let (instrument, instrument_lease) = open_instrument(&mut store);
    let t = task("a-hello");
    let seal = mint_seal(&mut store, &instrument, &instrument_lease);
    let oroot = dir("s1-neg-oracle");
    let oracle = AdapterOracle::new(&oroot, None).unwrap();
    // Wrong-but-valid ⇒ a scored zero, never oracle_failure.
    let wrong = submission(&t, "{\"answer\":\"HELLO\"}".as_bytes());
    let (w, _) = grade_attempt(
        &mut store,
        &instrument,
        &instrument_lease,
        &instrument,
        seal.clone(),
        &wrong,
        &t,
        &oracle,
    );
    assert_eq!(w.outcome.as_deref(), Some("decided"));
    assert_eq!(w.reward_ppm, Some(0));
    // Broken oracle ⇒ OracleFailure outcome, never a fabricated reward.
    let bad_root = dir("s1-neg-broken");
    let broken = AdapterOracle::new(&bad_root, Some("no-such-task".to_string())).unwrap();
    let good = submission(&t, "{\"answer\":\"hello\"}".as_bytes());
    let (b, _) = grade_attempt(
        &mut store,
        &instrument,
        &instrument_lease,
        &instrument,
        seal,
        &good,
        &t,
        &broken,
    );
    assert_eq!(b.outcome.as_deref(), Some("oracle_failure"), "{b:?}");
}

#[test]
fn ac5_worksheet_leg_verified() {
    let q = build_quote(&worksheet_decl()).expect("declared worksheet builds");
    assert_eq!(q.currency.as_deref(), Some("USD"));
    assert_eq!(q.total_micros, 0);
    let f = worksheet_facts();
    assert_eq!(f.pilot_subjects, Some(10));
    assert!(f.pilot_floor_met && f.unknown_exposure_bounded && f.unpriced.is_empty());
    assert_eq!(evaluate_worksheet(&f), LegVerdict::Verified);
}

#[test]
fn ac2_ac3_executed_packet_is_honest() {
    let d = run_s1();
    let report = assemble(d.legs, &d.oracle_outcomes, d.attempts).unwrap();
    assert_eq!(report.version, "hh-s1-useful/1");
    assert_eq!(report.admission, "bounded_s1_execution");
    assert!(!report.packet_id.is_empty());
    // Honest dispositions only: every leg's verdict is a typed member.
    for l in &report.legs {
        assert!(LEGS.contains(&l.name.as_str()));
        assert!(l.verdict.name() != "verified" || l.verdict.is_verified());
    }
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\""));
    if let Ok(out) = std::env::var("HH_S1_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
    }
}
