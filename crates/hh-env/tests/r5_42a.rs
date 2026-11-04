//! R5.42a — the bounded backend feasibility preflight (GATE-G18's
//! admitted scope). The six legs execute for real: the nine pinned
//! artifacts hash-verify byte-for-byte; the actual HelperExecutor/Podman
//! candidate is probed; one real `Dispatcher`+Π+`HelperExecutor` mediated
//! marker append plus the surviving observer/probe drive runs on the
//! available lane (the carrier is recorded verbatim — `inconclusive`
//! when the backend is absent, never a skip counted as pass); the
//! acquisition/network policy facts are checked; the declared §9.2
//! minimum-admission worksheet builds through the real `build_quote`.
//! Typed verdicts only — an unsupported/inconclusive packet returns for
//! scope decision and is never an S1/S4 acceptance. Offline, scratch
//! paths, USD 0.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
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
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::ToolExecutor;
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::helper::helper_binary;
use hharness_env::ops::{MarkerSink, ObservationPoint, OpsObserver, Sentinel, TrialMarker};
use hharness_env::preflight::{
    assemble, check_pins, evaluate_backend, evaluate_mediation, evaluate_observer, evaluate_policy,
    evaluate_worksheet, BackendProbe, Disposition, Leg, LegVerdict, MediationOutcome,
    ObserverOutcome, PinnedArtifact, PolicyFacts, WorksheetFacts, LEGS,
};
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
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_ledger::event::Scope;
use hharness_ledger::ids::{effect_id, ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::Json;

// ── scaffold (mirrors helper_live.rs) ───────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-pf-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
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
    std::fs::canonicalize(&p).unwrap()
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

fn ready_env(store: &mut Store, lease: &Lease, driver: &mut EnvDriver, ws: &Path) -> String {
    let record = EnvironmentRecord {
        class: EnvironmentClass::LocalSandboxed,
        image: ImageRef::ContentAddress(hharness_identity::idp::address(
            b"preflight-image",
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

// ── leg drivers (the actual backend/facts, records-in) ──────────────────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The nine pinned artifacts — the concrete pins fleshed at 2026-11-03
/// (sha256s re-verified against HUMAN-H8's recorded digests).
fn pinned_artifacts() -> Vec<PinnedArtifact> {
    let root = repo_root();
    [
        (
            "a-count",
            "crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-count.json",
            "9f6d4f11695efff5671745015f7f7c87d9e4e95f049364e027e6a60fc4124ce6",
        ),
        (
            "a-echo",
            "crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-echo.json",
            "9fdc327157877c3fdbb124a0a5f98b5a2df0d22a7889c4da9e11319559977b40",
        ),
        (
            "a-hello",
            "crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json",
            "5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344",
        ),
        (
            "a-sum",
            "crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-sum.json",
            "e30c65b99203d604f9a843e72c31fd1a053bf31c8ab901bb295c2773b0f8e349",
        ),
        (
            "a-upper",
            "crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-upper.json",
            "3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4",
        ),
        (
            "ex-01",
            "scripts/examples/ex-01.json",
            "0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3",
        ),
        (
            "ex-02",
            "scripts/examples/ex-02.json",
            "8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285",
        ),
        (
            "ex-02-fixture",
            "scripts/examples/ex-02-fixture.json",
            "f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8",
        ),
        (
            "fixture-definition",
            "clients/ts/fixture-definition.json",
            "732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8",
        ),
    ]
    .iter()
    .map(|(id, rel, sha)| PinnedArtifact {
        artifact_id: id.to_string(),
        path: root.join(rel),
        sha256: sha.to_string(),
    })
    .collect()
}

/// The actual HelperExecutor/Podman candidate probe — a live handshake
/// + declaration read only when the engine answers (never simulated).
fn probe_backend() -> BackendProbe {
    let available = hharness_helper::podman::is_available();
    let binary = helper_binary().is_some();
    let mut probe = BackendProbe {
        backend: "podman".to_string(),
        runtime_available: available,
        helper_binary: binary,
        handshake: None,
        handshake_error: None,
        declaration: None,
    };
    if !(available && binary) {
        return probe;
    }
    let sock_dir = dir("pf-podman");
    let workspace = repo_root().join("target/pf-podman-work");
    std::fs::create_dir_all(&workspace).unwrap();
    let args = vec![
        "--container".to_string(),
        "hh-env-pf-probe".to_string(),
        "--workspace".to_string(),
        workspace.display().to_string(),
    ];
    match hharness_env::helper::HelperClient::spawn(&sock_dir, "podman", &args) {
        Ok(mut client) => {
            match client.hello(
                hharness_helper::protocol::OnKernelLoss::PreserveUntil { ttl_ms: 60_000 },
                "pf-nonce",
                Some(30_000),
                None,
                None,
                false,
            ) {
                Ok(_) => {
                    probe.handshake = Some(true);
                    let exec = hharness_env::helper::HelperExecutor::new(client, vec![]);
                    let d = exec.declaration();
                    probe.declaration = Some(
                        [
                            ("isolation", d.isolation_support.as_str()),
                            ("probe", d.probe_support.as_str()),
                            ("dedup", d.dedup_support.as_str()),
                            ("interrupt", d.interrupt.as_str()),
                        ]
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    );
                }
                Err(e) => {
                    probe.handshake = Some(false);
                    probe.handshake_error = Some(format!("{e}"));
                }
            }
        }
        Err(e) => {
            probe.handshake = Some(false);
            probe.handshake_error = Some(format!("{e}"));
        }
    }
    probe
}

/// One real mediated marker append + surviving observer/probe drive on
/// the seatbelt lane (the only live `HelperExecutor` backend this host
/// can actually spawn; the carrier is recorded verbatim).
fn drive_cell(tag: &str) -> (MediationOutcome, ObserverOutcome) {
    let mut med = MediationOutcome {
        carrier_backend: "seatbelt".to_string(),
        attempted: false,
        outcome: None,
        markers: None,
        failure: None,
    };
    let mut obs = ObserverOutcome {
        attempted: false,
        intervals: 0,
        sentinel_intact: None,
        survived: false,
        probe_verdict: None,
        probe_source: None,
    };
    if !hharness_helper::seatbelt::is_available() {
        med.carrier_backend = "none".to_string();
        return (med, obs);
    }
    med.attempted = true;
    obs.attempted = true;
    let (mut store, run, lease, _clock) = open(tag);
    open_scopes(&mut store, &run, &lease);
    let mut driver = EnvDriver::new(&run);
    let ws = workspace(tag);
    let env_id = ready_env(&mut store, &lease, &mut driver, &ws);
    let ops_dir = ws.join("ops");
    let sink = MarkerSink::open(&ops_dir).unwrap();
    let sentinel = Sentinel::create(&ops_dir, b"denied").unwrap();
    let mut observer = OpsObserver::open(&ops_dir).unwrap();
    observer
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();
    obs.intervals += 1;
    let cap = capability();
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let marker = TrialMarker::new("pf-t1", &effect_id(&run, "mc-1", "tc-1", 0), 0);
    let line = Json::obj([
        ("digest", Json::str(&marker.digest())),
        ("effect_id", Json::str(&marker.effect_id)),
        ("marker_id", Json::str(&marker.marker_id)),
        ("ordinal", Json::Int(marker.ordinal as i64)),
        ("trial_id", Json::str(&marker.trial_id)),
    ])
    .to_canonical_string();
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
                    "printf '%s\\n' '{}' >> {}/marker.log",
                    line,
                    ops_dir.display()
                )),
            ]),
        )]),
    );
    let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    observer
        .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
        .unwrap();
    obs.intervals += 1;
    med.outcome = Some(match &out {
        DispatchOutcome::Observed(o) => format!("observed:{}", o.outcome.as_str()),
        DispatchOutcome::Refused { .. } => "refused".to_string(),
        DispatchOutcome::Unknown { .. } => "unknown".to_string(),
        _ => "other".to_string(),
    });
    // The read-only instrument probe + the third interval after return.
    let eid = effect_id(&run, "mc-1", "tc-1", 0);
    match exec.probe(&eid, 0) {
        Ok(v) => {
            obs.probe_verdict = Some(v.as_str().to_string());
            obs.probe_source = Some("instrument".to_string());
        }
        // The lane's live probe answering `Err` is a recorded host fact —
        // probe stays absent (inconclusive), never a simulated verdict.
        Err(e) => obs.probe_verdict = Some(format!("error:{e}")),
    }
    if obs
        .probe_verdict
        .as_deref()
        .is_some_and(|v| v.starts_with("error:"))
    {
        obs.probe_verdict = None;
        obs.probe_source = None;
    }
    driver.return_executor(&env_id, exec);
    observer
        .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
        .unwrap();
    obs.intervals += 1;
    med.markers = sink.markers().ok().map(|m| m.len() as u64);
    obs.sentinel_intact = Some(sentinel.intact());
    obs.survived = observer.rows().map(|r| !r.is_empty()).unwrap_or(false);
    driver.teardown(&mut store, &lease, &env_id).unwrap();
    (med, obs)
}

fn policy_facts() -> PolicyFacts {
    let p = kernel_default(0);
    PolicyFacts {
        network_default: p.net.mode.as_str().to_string(),
        image_ref_kind: "content_address".to_string(),
        acquisition: "local_only".to_string(),
        writable_roots_confined: true,
    }
}

// ── the declared §9.2 minimum-admission worksheet (HUMAN-H8, verbatim) ────────

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
            "pilot" => Some(10), // 2·T·R at T=1, R=5 — the declared floor
            "activation" => Some(2),
            _ => None,
        })
        .into_iter()
        .chain(if stage == "parity" {
            // Both runners quoted — one free target line each.
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
        pilot_floor_met: 2 * 1 * 5 >= 10 * 1, // declared T=1,R=5 floor
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
            let o = r;
            let comp = o.get("component").and_then(Json::as_str).unwrap_or("");
            if comp == "provider_unknown_exposure"
                && o.get("quantity").and_then(Json::as_int) != Some(0)
            {
                facts.unknown_exposure_bounded = false;
            }
            if comp == "subjects_first" && s.stage == "pilot" {
                facts.pilot_subjects = o.get("quantity").and_then(Json::as_int).map(|v| v as u64);
            }
            if comp != "provider_unknown_exposure"
                && o.get("exposure_micros").and_then(Json::as_str) == Some("unpriced")
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

/// All six legs executed — the assembled packet (each leg's facts echo
/// the actual supplied evidence).
fn run_preflight() -> Vec<Leg> {
    let pins = check_pins(&pinned_artifacts());
    let probe = probe_backend();
    let backend = evaluate_backend(&probe);
    let (med, obs) = drive_cell("pf-cell");
    let mediation = evaluate_mediation(&med);
    let observer = evaluate_observer(&obs);
    let policy = evaluate_policy(&policy_facts());
    let worksheet = evaluate_worksheet(&worksheet_facts());
    vec![
        leg("tb2_lifecycle", pins, &[("artifacts", "9".to_string())]),
        leg(
            "backend_candidate",
            backend,
            &[
                ("backend", probe.backend.clone()),
                ("runtime_available", probe.runtime_available.to_string()),
                ("helper_binary", probe.helper_binary.to_string()),
            ],
        ),
        leg(
            "mediation",
            mediation,
            &[("carrier_backend", med.carrier_backend.clone())],
        ),
        leg("observer_probe", observer, &[]),
        leg("acquisition_network", policy, &[]),
        leg("worksheet", worksheet, &[]),
    ]
}

// ── the battery ─────────────────────────────────────────────────────────────

#[test]
fn ac2_tb2_lifecycle_pins_verify() {
    assert_eq!(check_pins(&pinned_artifacts()), LegVerdict::Verified);
}

#[test]
fn ac3_pin_mismatch_is_refused() {
    let mut pins = pinned_artifacts();
    pins[0].sha256 = "0".repeat(64);
    match check_pins(&pins) {
        LegVerdict::Refused(r) => assert_eq!(r, "pin_mismatch:a-count"),
        v => panic!("expected refused pin_mismatch, got {v:?}"),
    }
}

#[test]
fn ac2_backend_candidate_probe_is_typed() {
    let probe = probe_backend();
    let v = evaluate_backend(&probe);
    if !probe.runtime_available {
        // The honest answer on a host without a reachable engine.
        assert_eq!(
            v,
            LegVerdict::Inconclusive("backend_unavailable:podman".to_string())
        );
    } else {
        // A reachable engine must carry the full honest declaration.
        assert!(v.is_verified(), "probe {probe:?} verdict {v:?}");
    }
}

#[test]
fn ac2_mediation_observer_real_drive() {
    if !hharness_helper::seatbelt::is_available() {
        eprintln!("seatbelt unavailable — drive legs evaluated inconclusive");
        let (m, o) = drive_cell("pf-none");
        assert_eq!(m.carrier_backend, "none");
        assert!(!evaluate_mediation(&m).is_verified());
        assert!(!evaluate_observer(&o).is_verified());
        return;
    }
    let (m, o) = drive_cell("pf-drive");
    assert_eq!(m.carrier_backend, "seatbelt");
    assert_eq!(
        evaluate_mediation(&m),
        LegVerdict::Verified,
        "mediation {m:?}"
    );
    assert_eq!(m.outcome.as_deref(), Some("observed:applied"));
    assert_eq!(m.markers, Some(1));
    assert_eq!(o.intervals, 3);
    assert_eq!(o.sentinel_intact, Some(true));
    assert!(o.survived);
    // The surviving-observer folds verified on this host; the live
    // `probe()` re-hello returned `Err` (recorded `probe_absent`) — a
    // typed inconclusive leg, never a simulated verdict.
    assert_eq!(
        evaluate_observer(&o),
        LegVerdict::Inconclusive("probe_absent".to_string()),
        "observer {o:?}"
    );
}

#[test]
fn ac3_mediation_negatives() {
    for (m, want) in [
        (
            MediationOutcome {
                carrier_backend: "none".into(),
                attempted: false,
                outcome: None,
                markers: None,
                failure: None,
            },
            LegVerdict::Inconclusive("mediation_not_attempted".into()),
        ),
        (
            MediationOutcome {
                carrier_backend: "podman".into(),
                attempted: true,
                outcome: Some("refused".into()),
                markers: None,
                failure: None,
            },
            LegVerdict::Refused("mediation_denied".into()),
        ),
        (
            MediationOutcome {
                carrier_backend: "podman".into(),
                attempted: true,
                outcome: Some("observed:applied".into()),
                markers: Some(2),
                failure: None,
            },
            LegVerdict::Refused("marker_count:2".into()),
        ),
    ] {
        assert_eq!(evaluate_mediation(&m), want);
    }
    let o = ObserverOutcome {
        attempted: true,
        intervals: 2,
        sentinel_intact: Some(true),
        survived: true,
        probe_verdict: Some("applied".into()),
        probe_source: Some("instrument".into()),
    };
    assert!(!evaluate_observer(&o).is_verified());
}

#[test]
fn ac2_acquisition_network_policy() {
    assert_eq!(evaluate_policy(&policy_facts()), LegVerdict::Verified);
    let mut bad = policy_facts();
    bad.network_default = "bridged".into();
    assert!(matches!(evaluate_policy(&bad), LegVerdict::Refused(_)));
}

#[test]
fn ac2_worksheet_minimum_admission() {
    let q = build_quote(&worksheet_decl()).expect("declared worksheet builds");
    assert_eq!(q.currency.as_deref(), Some("USD"));
    assert_eq!(q.total_micros, 0);
    let pilot = q.stages.iter().find(|s| s.stage == "pilot").unwrap();
    assert_eq!(pilot.state, "ready_for_gate");
    assert_eq!(pilot.dispatch, "gate_pending:GATE-G12");
    for s in ["parity", "confirmation"] {
        let p = q.stages.iter().find(|x| x.stage == s).unwrap();
        assert_eq!(p.dispatch, "not_requested");
        assert_eq!(p.exposure_micros, 0);
    }
    let facts = worksheet_facts();
    assert_eq!(facts.pilot_subjects, Some(10));
    assert!(facts.pilot_floor_met);
    assert!(facts.unknown_exposure_bounded);
    assert!(facts.unpriced.is_empty());
    assert_eq!(evaluate_worksheet(&facts), LegVerdict::Verified);
}

#[test]
fn ac3_worksheet_unset_currency_is_zero_and_blocked() {
    let mut d = worksheet_decl();
    d.currency = None;
    let q = build_quote(&d).unwrap();
    assert_eq!(q.total_micros, 0);
    let mut f = worksheet_facts();
    f.currency = None;
    assert_eq!(
        evaluate_worksheet(&f),
        LegVerdict::Inconclusive("worksheet_currency_unset".to_string())
    );
    f.currency = Some("USD".into());
    f.pilot_floor_met = false;
    assert!(matches!(evaluate_worksheet(&f), LegVerdict::Refused(_)));
}

#[test]
fn ac3_disposition_never_feasible_on_unverified_leg() {
    let verified = || Leg {
        name: "x".to_string(),
        verdict: LegVerdict::Verified,
        facts: BTreeMap::new(),
    };
    let mk = |i: usize, v: LegVerdict| {
        LEGS.iter()
            .enumerate()
            .map(|(j, n)| {
                let mut l = verified();
                l.name = n.to_string();
                if j == i {
                    l.verdict = v.clone();
                }
                l
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        assemble(mk(0, LegVerdict::Verified)).unwrap().disposition,
        Disposition::Feasible
    );
    let inconclusive = assemble(mk(2, LegVerdict::Inconclusive("x".into()))).unwrap();
    assert_eq!(inconclusive.disposition, Disposition::Inconclusive);
    assert!(inconclusive.scope_return);
    let infeasible = assemble(mk(4, LegVerdict::Unsupported("y".into()))).unwrap();
    assert_eq!(infeasible.disposition, Disposition::Infeasible);
    assert!(infeasible.scope_return);
    // A definitive failure dominates an undetermined leg.
    let mut legs = mk(2, LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Infeasible);
    // Malformed leg sets refuse — never a partial packet.
    assert!(assemble(mk(0, LegVerdict::Verified)[..5].to_vec()).is_err());
}

#[test]
fn ac2_ac3_executed_packet_is_honest() {
    let legs = run_preflight();
    let report = assemble(legs).unwrap();
    assert_eq!(report.admission, "not_s1_s4_acceptance");
    assert_eq!(report.version, "hh-backend-preflight/1");
    assert!(!report.preflight_id.is_empty());
    assert_eq!(
        report.scope_return,
        report.disposition != Disposition::Feasible
    );
    // The record is canonical + deterministic.
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\""));
    // Emit the executed record when asked (the committed artifact is
    // produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_PREFLIGHT_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
    }
}
