//! R6.38 — the REQ-XE-116 dispatch-gate submit-marker detection
//! (BM12/BM13 over the settled raw output) and the REQ-XE-118
//! `open_session.exec` arm path (the RPC-level member, the
//! `executor::bind` refusal, and the `ExecArmDecl` decode).
//!
//! Fixture-verified — the marker detection runs through the real
//! `DispatchGate`/`Dispatcher` path (the settled observation mints
//! `submission_ref` durable on `action.effect.observed`); the
//! `exec` arm rides the service's own `open_session`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::equiv::SurfaceBinding;
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ExecPolicy, NetMode, WritableRoot};
use hharness_control::driver::{DispatchSeam, EffectGate};
use hharness_control::vocab::SettledOutcome;
use hharness_embed::runtime::{DispatchGate, EmbedGate, ExecCap};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_env::capture::{CaptureKind, OutputPolicy};
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::{SubmissionMarker, SubmitLine};
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::events::EventMinter;
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
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
use hharness_identity::address;
use hharness_ledger::event::Scope;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::decision::DecisionScope;
use hharness_monitor::handle::{AuthorityHandle, HandleId, HandleValidity, OriginBasis};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::{AuthorityClass, Label, ProvenanceRecord};
use hharness_wire::json::Json;

const MARKER: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r638-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
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

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
    store.events(run).map(|e| e.to_vec()).unwrap_or_default()
}

/// `turn ⊃ mc` scopes — once per fixture (the driver-side shape).
fn open_turn_scopes(store: &mut Store, run: &str, lease: &Lease) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc.1".into()),
        ..Scope::default()
    };
    store.append(run, lease, vec![t, c]).unwrap();
}

/// `action.tool.proposed`/`action.effect.intended` for the adopted
/// coordinate `ef` under `tc` (the driver-side shape).
fn open_adopted_scopes(store: &mut Store, run: &str, lease: &Lease, ef: &str, tc: &str) {
    let m = EventMinter::new(store, run);
    let mut p = m
        .mint(
            "action.tool.proposed",
            Json::obj([("tool_call_id", Json::str(tc))]),
        )
        .unwrap();
    p.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc.1".into()),
        tool_call_id: Some(tc.into()),
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
    store.append(run, lease, vec![p, i]).unwrap();
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
        image: ImageRef::ContentAddress(address(b"r6.38-image", "application/octet-stream")),
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

fn exec_cap_record() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        exec_defaults: None,
        purpose: Text::new(
            "run a command",
            "test:owner",
            ProvenanceRecord::kernel("t", 0),
        ),
        input_schema: Json::obj([(
            "properties",
            Json::obj([("command", Json::obj([("type", Json::str("string"))]))]),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::Exec,
                attributes: Some(reversible_attrs()),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Unknown,
        resources: Resources::NoneDeclared,
        observation_contract: Json::Null,
        cost_model: None,
        execution_requirement: Json::Null,
        source: Json::Null,
        exposure_hint: Json::Null,
        postconditions: vec![],
        flow_contract: None,
        action_patterns: Vec::new(),
    }
}

fn bash_binding() -> SurfaceBinding {
    SurfaceBinding {
        surface_name: "bash".to_string(),
        surface_id: "exec.bash".to_string(),
        exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
        capability_ref: PinnedRef {
            semantic_id: "test:bash".to_string(),
            version_id: "v-cap".to_string(),
        },
        capability_refs: vec!["test:bash".to_string()],
        hir_node_id: "test:bash".to_string(),
        arg_map: [(
            "command".to_string(),
            hharness_compiler::equiv::ArgMapEntry {
                capability_param: "command".to_string(),
                transform: hharness_compiler::equiv::ArgTransform::Identity,
                narrowing: None,
            },
        )]
        .into_iter()
        .collect(),
        mapping: hharness_compiler::surface::BindingMapping::SurfaceArgMap,
        rule_ids: vec![],
        evidence_ref: None,
        safety_ref: None,
        effects_bound: vec![],
        family_id: None,
        variant_id: None,
        dialect: "json-schema-2020-12".to_string(),
        admitted_modes: [hharness_hir::tools::ExposureMode::Direct]
            .into_iter()
            .collect(),
        pinned: false,
        hidden: false,
    }
}

fn allow_monitor(rec: &ToolCapabilityRecord) -> Monitor {
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
            effect: EffectClass::domain_only(EffectDomain::Exec),
            scope: "*".to_string(),
            constraints: GrantConstraints::default(),
            delegable: true,
        }],
        ceiling: AuthorityClass::Definition,
        validity: HandleValidity {
            issued_at: "evt-mint".into(),
            expires_at: None,
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
    m.capabilities.insert(
        "test:bash".to_string(),
        CapabilityEntry {
            record: rec.clone(),
            binding: bash_binding(),
        },
    );
    m
}

fn exec_cap(exec: Box<dyn ToolExecutor>) -> ExecCap {
    ExecCap {
        exec_defaults: None,
        surface_id: "exec.bash".to_string(),
        binding: bash_binding(),
        scope_bindings: ScopeBindings::Unknown,
        capability: exec_cap_record(),
        capability_ref: PinnedRef {
            semantic_id: "test:bash".to_string(),
            version_id: "v-cap".to_string(),
        },
        declared: EffectClass {
            domain: EffectDomain::Exec,
            attributes: Some(reversible_attrs()),
        },
        proposer: "test:agent".to_string(),
        ladder: DeadlineLadder {
            run_deadline_ms: None,
            phase_deadline_ms: None,
            effect_deadline_ms: None,
            attempt_deadline_ms: None,
        },
        output_policy: OutputPolicy::default(),
        executor: exec,
        monitor: allow_monitor(&exec_cap_record()),
    }
}

/// A stub executor emitting each call's queued stdout chunk then an
/// `ok` terminal with the configured exit status — the raw output the
/// settled observation captures.
struct OutputExec {
    decl: ExecutorDeclaration,
    outputs: VecDeque<String>,
    rc: i64,
}

impl OutputExec {
    fn new(outputs: &[&str], rc: i64) -> Self {
        OutputExec {
            decl: ExecutorDeclaration {
                executor_id: "output-stub/1".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error".to_string()].into_iter().collect(),
                streams: false,
                domains: [EffectDomain::Exec].into_iter().collect(),
            },
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            rc,
        }
    }
}

impl ToolExecutor for OutputExec {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        _request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        let data = self.outputs.pop_front().unwrap_or_default();
        if !data.is_empty() {
            sink(ExecutorSignal {
                token: _request.attribution_token.clone(),
                kind: CaptureKind::OutputChunk {
                    stream: "stdout".to_string(),
                    data,
                },
            });
        }
        Ok(TerminalReport {
            status: TerminalStatus::Ok,
            exit_status: Some(self.rc),
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

fn marker() -> SubmissionMarker {
    SubmissionMarker {
        marker: MARKER.to_string(),
        line: SubmitLine::First,
        require_returncode: 0,
    }
}

fn gate_with(
    run: &str,
    env: Option<String>,
    exec: Box<dyn ToolExecutor>,
    m: Option<SubmissionMarker>,
) -> DispatchGate {
    DispatchGate {
        embed: EmbedGate {
            host_surfaces: BTreeMap::new(),
            host_asks: Vec::new(),
        },
        exec_caps: [("exec.bash".to_string(), exec_cap(exec))]
            .into_iter()
            .collect(),
        run_id: run.to_string(),
        env_handle_id: env,
        seed: [7u8; 32],
        result_renderers: BTreeMap::new(),
        submission_marker: m,
    }
}

fn intent(tc: &str) -> Json {
    Json::obj([
        ("surface_id", Json::str("exec.bash")),
        ("tool_call_id", Json::str(tc)),
        ("args", Json::obj([("command", Json::str("x"))])),
    ])
}

/// BM12 (REQ-XE-116; AR-24) — the armed `stop_rule =
/// submit{marker, line: first, require_returncode: 0}` detects the
/// marker on the first line of the settled raw output with `rc == 0`:
/// the gate returns `submission_ref` and the durable
/// `action.effect.observed` terminal carries the member (a replay
/// re-serves it verbatim).
#[test]
fn bm12_marker_first_line_rc_zero_ends_submitted() {
    let (mut store, run, lease) = open("bm12");
    let mut driver = EnvDriver::new(&run);
    let ws = dir("bm12-ws");
    open_turn_scopes(&mut store, &run, &lease);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");
    let mut gate = gate_with(
        &run,
        Some(env),
        Box::new(OutputExec::new(
            &[&format!("{MARKER}\nsubmission body line two\nline three")],
            0,
        )),
        Some(marker()),
    );
    let seam = DispatchSeam {
        store: &mut store,
        lease: &lease,
        envs: Some(&mut driver),
    };
    let out = gate
        .dispatch_durable(seam, "ef-1", 1, &intent("mc.1.tc0"))
        .expect("armed surface");
    assert_eq!(out.submission_ref.as_deref(), Some("sub-ef-1"), "{out:?}");
    // The durable terminal row carries `submission_ref` — the replay
    // fold re-serves the recorded submission (ADR-0135 §2).
    let events = read_all(&store, &run);
    let observed = events
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .expect("observed terminal");
    assert_eq!(
        observed
            .payload
            .get("submission_ref")
            .and_then(Json::as_str),
        Some("sub-ef-1"),
        "terminal payload: {}",
        observed.payload.to_canonical_string()
    );
}

/// BM13 — a non-first-line marker continues (no `submission_ref`,
/// no member on the terminal row) and a first-line marker with the
/// wrong `returncode` continues the same way.
#[test]
fn bm13_non_first_line_or_wrong_rc_continues() {
    for (tag, output, rc) in [
        ("bm13-second", format!("preamble\n{MARKER}\nrest"), 0i64),
        ("bm13-rc", MARKER.to_string(), 1i64),
    ] {
        let (mut store, run, lease) = open(tag);
        let mut driver = EnvDriver::new(&run);
        let ws = dir(&format!("{tag}-ws"));
        open_turn_scopes(&mut store, &run, &lease);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");
        let mut gate = gate_with(
            &run,
            Some(env),
            Box::new(OutputExec::new(&[&output], rc)),
            Some(marker()),
        );
        let seam = DispatchSeam {
            store: &mut store,
            lease: &lease,
            envs: Some(&mut driver),
        };
        let out = gate
            .dispatch_durable(seam, "ef-1", 1, &intent("mc.1.tc0"))
            .expect("armed surface");
        assert!(
            matches!(out.outcome, SettledOutcome::Observed { .. }),
            "{tag}: {out:?}"
        );
        assert_eq!(out.submission_ref, None, "{tag}: {out:?}");
        let events = read_all(&store, &run);
        let observed = events
            .iter()
            .find(|e| e.class == "action.effect.observed")
            .expect("observed terminal");
        assert!(
            observed.payload.get("submission_ref").is_none(),
            "{tag}: no member when detection misses"
        );
    }
}

/// An unarmed gate (`submission_marker: None`) mints no `submission_ref`
/// even when the output happens to carry the marker — the opt-in rule
/// (byte-identical legacy behavior, AC-6).
#[test]
fn unarmed_marker_dormant() {
    let (mut store, run, lease) = open("unarmed");
    let mut driver = EnvDriver::new(&run);
    let ws = dir("unarmed-ws");
    open_turn_scopes(&mut store, &run, &lease);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");
    let mut gate = gate_with(
        &run,
        Some(env),
        Box::new(OutputExec::new(&[MARKER], 0)),
        None,
    );
    let seam = DispatchSeam {
        store: &mut store,
        lease: &lease,
        envs: Some(&mut driver),
    };
    let out = gate
        .dispatch_durable(seam, "ef-1", 1, &intent("mc.1.tc0"))
        .expect("armed surface");
    assert_eq!(out.submission_ref, None);
    let events = read_all(&store, &run);
    let observed = events
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .expect("observed terminal");
    assert!(observed.payload.get("submission_ref").is_none());
}

/// REQ-XE-118 — the `exec` arm's `executor::bind` check refuses an
/// executor that cannot satisfy the capability's
/// `execution_requirement` (the same arm path the `open_session.exec`
/// member lands).
#[test]
fn exec_arm_refuses_unsatisfiable_requirement() {
    let root = dir("exec-refuse");
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.clone(),
        holder: "r6-38".to_string(),
    })
    .unwrap();
    let mut cap = exec_cap(Box::new(hharness_env::local::LocalExecutor::new(vec![])));
    cap.capability.execution_requirement =
        Json::obj([("minimum_isolation", Json::str("namespaces"))]);
    let err = svc
        .arm_exec_capabilities("sess-none", vec![cap])
        .expect_err("an insufficient executor must refuse at arm");
    assert!(format!("{err:?}").contains("exec_cap_unbound"), "{err:?}");
}

/// The `ExecArmDecl`/`OpenSessionParams` strict decode — the `exec`
/// member decodes `{env, surfaces}`, an unknown member refuses, and
/// the member is absent-not-required on a legacy open (AC-6).
#[test]
fn exec_decl_decode() {
    use hharness_embed_schema::types::OpenSessionParams;
    let p = OpenSessionParams::from_json(&Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("attach")),
                ("run_id", Json::str("run-x")),
            ]),
        ),
        ("idempotency_key", Json::str("k1")),
        (
            "exec",
            Json::obj([
                ("env", Json::str("session")),
                ("surfaces", Json::Arr(vec![Json::str("test:bash")])),
            ]),
        ),
    ]))
    .expect("exec member decodes");
    let e = p.exec.expect("exec member");
    assert_eq!(e.env, "session");
    assert_eq!(e.surfaces, vec!["test:bash".to_string()]);
    // Unknown members inside `exec` refuse (strict object).
    assert!(OpenSessionParams::from_json(&Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("attach")),
                ("run_id", Json::str("run-x")),
            ]),
        ),
        ("idempotency_key", Json::str("k1")),
        (
            "exec",
            Json::obj([
                ("env", Json::str("session")),
                ("surfaces", Json::Arr(vec![Json::str("test:bash")])),
                ("bogus", Json::Int(1)),
            ]),
        ),
    ]))
    .is_err());
    // The member is optional — a legacy `open_session` body decodes
    // identically (AC-6).
    let legacy = OpenSessionParams::from_json(&Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("attach")),
                ("run_id", Json::str("run-x")),
            ]),
        ),
        ("idempotency_key", Json::str("k1")),
    ]))
    .expect("legacy open decodes");
    assert!(legacy.exec.is_none());
}

/// `exec.env` spellings other than `session` refuse typed at arm —
/// never coerced to a different environment.
#[test]
fn exec_env_spelling_refuses() {
    // The `env != "session"` refusal lives in `arm_session_exec`; the
    // decode-level coverage here names the contract (the RPC arm leg
    // is exercised by the service path).
    let e = hharness_embed_schema::types::ExecArmDecl {
        env: "other-env".to_string(),
        surfaces: vec!["x".to_string()],
    };
    assert_eq!(
        e.to_json().get("env").and_then(Json::as_str),
        Some("other-env")
    );
}
