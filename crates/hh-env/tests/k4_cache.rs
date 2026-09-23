//! S2.10 acceptance coverage — the K4 tool-result cache through the real
//! dispatcher (R-2.3.4¹; §5b.4; ADR-0128 d.3 / ADR-0129 d.1/d.4).
//!
//! Each test fails if the behaviour it covers is removed. The scaffold is the
//! acceptance.rs fold: a real `Store` (WAL), a real `Monitor`, a real
//! `EnvDriver` over `Ep2Model::reference()`, a scripted `MockExecutor` that
//! counts `execute` calls — and the run's `K4Cache` over a real
//! `MemoryStore`. The dispatch exercises the full pipeline: `authorized` →
//! K4 lookup → `prepared` → (no `committed`/`started` on a hit) → `observed`
//! → `completed` → K4 deposit.

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
use hh_context::k4::K4Cache;
use hh_context::memory::MemoryStore;
use hh_env::capture::OutputPolicy;
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::{EventMinter, ScopeChain};
use hh_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hh_env::handle::{OnLoss, Roots};
use hh_env::observe::ObservedStatus;
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
use hh_provenance::authority::ReaderSet;
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_wire::json::Json;

// ── scaffold (the acceptance.rs fold) ────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-k4-{}-{tag}-{n}", std::process::id()));
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

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    let record = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(address(b"k4-test-image", "application/octet-stream")),
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

fn read_only_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::ReadOnly,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
    }
}

fn reversible_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
    }
}

/// The capability record — `read_file{path}` (FsRead, `cacheable: true`) or
/// `write_file{path, content}` (FsWrite; never admissible — not read_only).
fn capability(domain: EffectDomain, a: EffectAttributes, cacheable: bool) -> ToolCapabilityRecord {
    let mut contract = BTreeMap::new();
    contract.insert(
        "error_classes".to_string(),
        Json::Arr(vec![
            Json::str("executor_error"),
            Json::str("timeout"),
            Json::str("signalled"),
            Json::str("invalid_arguments"),
        ]),
    );
    if cacheable {
        contract.insert("cacheable".to_string(), Json::Bool(true));
    }
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
        scope_bindings: scope_bindings(),
        resources: Resources::NoneDeclared,
        observation_contract: Json::Obj(contract),
        cost_model: None,
        execution_requirement: Json::Null,
        source: Json::Null,
        exposure_hint: Json::Null,
        postconditions: vec![],
        flow_contract: None,
        action_patterns: Vec::new(),
    }
}

fn binding_named(sem: &str) -> SurfaceBinding {
    SurfaceBinding {
        surface_name: sem.to_string(),
        capability_ref: PinnedRef {
            semantic_id: sem.to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: sem.to_string(),
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
        capability_refs: vec![sem.to_string()],
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

/// A monitor over the default Π table with every passed capability
/// registered under its semantic id — `authorize` re-evals the args against
/// the *registered* record, so the record must match the dispatch's
/// declared class (a read-registered record + a write dispatch is a Π ask).
fn test_monitor(caps: &[(&str, &ToolCapabilityRecord)]) -> Monitor {
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
    for (sem, cap) in caps {
        m.capabilities.insert(
            sem.to_string(),
            CapabilityEntry {
                record: (*cap).clone(),
                binding: binding_named(sem),
            },
        );
    }
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
    readers: ReaderSet,
    sem: &str,
) -> DispatchInput<'a> {
    let mut context_label = Label::at(AuthorityClass::Principal);
    context_label.readers = readers;
    DispatchInput {
        chain: chain(tc),
        ordinal: 0,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: sem.to_string(),
            version_id: "v-cap".to_string(),
        },
        surface_args: args,
        args_provenance: ProvenanceRecord::minted(
            Origin::model("model:test", "run:test", "resp-1"),
            PersistenceScope::Run,
            0,
        ),
        context_label,
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
        flow: Default::default(),
    }
}

/// A scripted executor — counts `execute` calls; reports `ok`.
struct MockExecutor {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
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
        }
    }
}

impl ToolExecutor for MockExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        _request: &ExecutionRequest,
        _sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        self.calls.set(self.calls.get() + 1);
        Ok(report_ok())
    }
    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        Ok(ProbeVerdict::Undeterminable)
    }
}

fn read_all(store: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

/// The `model.cache.resolved` rows' `outcome` members in ledger order.
fn cache_outcomes(store: &Store, run: &str) -> Vec<String> {
    read_all(store, run)
        .iter()
        .filter(|e| e.class == "model.cache.resolved")
        .map(|e| {
            e.payload
                .get("outcome")
                .and_then(Json::as_str)
                .unwrap_or("?")
                .to_string()
        })
        .collect()
}

/// The read-args the tests share (one canonical args hash).
fn read_args(ws: &std::path::Path) -> Json {
    Json::obj([("path", Json::str(format!("{}/a.txt", ws.display())))])
}

// ── R-2.3.4¹ — miss → execute → deposit → hit (no second execution) ──────────

#[test]
fn k4_miss_then_hit_serves_without_executor() {
    let (mut store, run, lease, _clock) = open("k4hit");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("k4hit");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), true);
    let bind = binding_named("test:read_file");
    let sb = scope_bindings();
    let mon = test_monitor(&[("test:read_file", &cap)]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_cache(K4Cache::new(MemoryStore::new("mem:k4")));
    let mut exec = MockExecutor::ok();

    // First dispatch: miss → the executor runs → the entry is deposited at
    // `action.tool.completed`.
    let inp1 = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-1",
        read_args(&ws),
        EffectClass {
            domain: EffectDomain::FsRead,
            attributes: Some(read_only_attrs()),
        },
        ReaderSet::Public,
        "test:read_file",
    );
    let out1 = disp
        .dispatch(&mut driver, &mut exec, None, &inp1, &lease)
        .unwrap();
    assert!(matches!(out1, DispatchOutcome::Observed(_)), "{out1:?}");
    assert_eq!(exec.calls.get(), 1);

    // Second dispatch — identical call coordinate, new tool_call: the K4 hit
    // serves the recorded observation; the executor never runs again.
    let inp2 = input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-2",
        read_args(&ws),
        EffectClass {
            domain: EffectDomain::FsRead,
            attributes: Some(read_only_attrs()),
        },
        ReaderSet::Public,
        "test:read_file",
    );
    let out2 = disp
        .dispatch(&mut driver, &mut exec, None, &inp2, &lease)
        .unwrap();
    let DispatchOutcome::ObservedCached {
        observation,
        entry_ref,
        provenance,
        ..
    } = out2
    else {
        panic!("expected ObservedCached, got {out2:?}");
    };
    assert_eq!(exec.calls.get(), 1, "the hit must not invoke the executor");
    assert!(matches!(observation.status, ObservedStatus::Ok));
    // Served provenance: `origin = cache(entry_ref)`, derived_from the entry.
    assert_eq!(
        provenance.origin,
        Origin::Cache {
            entry_ref: entry_ref.clone()
        }
    );
    assert_eq!(provenance.derived_from[0].inputs, vec![entry_ref.clone()]);
    // Authority copied from the entry — never raised (I-NOAUTH's cache face).
    assert!(provenance.authority <= AuthorityClass::Environment);

    // The ledger tells the story: miss, then hit — plus the deposit's
    // `context.memory.written` row between them.
    assert_eq!(
        cache_outcomes(disp.store_mut(), &run),
        vec!["miss".to_string(), "hit".to_string()]
    );
    let envs = read_all(disp.store_mut(), &run);
    let written = envs
        .iter()
        .filter(|e| e.class == "context.memory.written")
        .count();
    assert_eq!(written, 1, "the deposit lands one memory.written row");
    // The hit dispatch carries the ordinary lifecycle terminals — and never
    // `started`/`committed` (no executor ran).
    let tc2_rows: Vec<_> = envs
        .iter()
        .filter(|e| e.scope.tool_call_id.as_deref() == Some("tc-2"))
        .map(|e| e.class.clone())
        .collect();
    assert!(tc2_rows.iter().any(|c| c == "action.effect.observed"));
    assert!(tc2_rows.iter().any(|c| c == "action.tool.completed"));
    assert!(!tc2_rows.iter().any(|c| c == "action.tool.started"));
}

// ── IR-3 — a committed mutation withholds the prior-epoch entry ──────────────

#[test]
fn k4_mutation_epoch_stale_withheld() {
    let (mut store, run, lease, _clock) = open("k4stale");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("k4stale");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let read_cap = capability(EffectDomain::FsRead, read_only_attrs(), true);
    let write_cap = capability(EffectDomain::FsWrite, reversible_attrs(), true);
    let rbind = binding_named("test:read_file");
    let wbind = binding_named("test:write_file");
    let sb = scope_bindings();
    let mon = test_monitor(&[
        ("test:read_file", &read_cap),
        ("test:write_file", &write_cap),
    ]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_cache(K4Cache::new(MemoryStore::new("mem:k4")));
    let mut exec = MockExecutor::ok();

    // Deposit the read entry (epoch 0).
    let rd = |tc: &str| {
        input(
            &read_cap,
            &rbind,
            &sb,
            &env,
            tc,
            read_args(&ws),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
            ReaderSet::Public,
            "test:read_file",
        )
    };
    disp.dispatch(&mut driver, &mut exec, None, &rd("tc-1"), &lease)
        .unwrap();
    assert_eq!(exec.calls.get(), 1);

    // A committed `fs_write` bumps `mutation_epoch(environment_ref)`.
    let wr = input(
        &write_cap,
        &wbind,
        &sb,
        &env,
        "tc-2",
        Json::obj([
            ("path", Json::str(format!("{}/b.txt", ws.display()))),
            ("content", Json::str("x")),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
        ReaderSet::Public,
        "test:write_file",
    );
    let outw = disp
        .dispatch(&mut driver, &mut exec, None, &wr, &lease)
        .unwrap();
    assert!(matches!(outw, DispatchOutcome::Observed(_)), "{outw:?}");
    assert_eq!(exec.calls.get(), 2);

    // The same read now resolves `stale_withheld` — the entry still exists
    // (nothing is cleared), the epoch stamp mismatch withholds it, and the
    // pipeline falls through to the executor.
    let out3 = disp
        .dispatch(&mut driver, &mut exec, None, &rd("tc-3"), &lease)
        .unwrap();
    assert!(matches!(out3, DispatchOutcome::Observed(_)), "{out3:?}");
    assert_eq!(exec.calls.get(), 3, "stale falls through to the executor");
    assert_eq!(
        cache_outcomes(disp.store_mut(), &run),
        vec![
            "miss".to_string(),
            "bypass".to_string(),
            "stale_withheld".to_string()
        ]
    );
}

// ── AC-R-2.3.4-9 — `readers` is a key member ─────────────────────────────────

#[test]
fn k4_readers_are_a_key_member() {
    let (mut store, run, lease, _clock) = open("k4readers");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("k4readers");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), true);
    let bind = binding_named("test:read_file");
    let sb = scope_bindings();
    let mon = test_monitor(&[("test:read_file", &cap)]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_cache(K4Cache::new(MemoryStore::new("mem:k4")));
    let mut exec = MockExecutor::ok();
    let rd = |tc: &str, readers: ReaderSet| {
        input(
            &cap,
            &bind,
            &sb,
            &env,
            tc,
            read_args(&ws),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
            readers,
            "test:read_file",
        )
    };

    // readers={A} deposits under its own name.
    let a = ReaderSet::Restricted(BTreeSet::from(["principal:a".to_string()]));
    let b = ReaderSet::Restricted(BTreeSet::from(["principal:b".to_string()]));
    disp.dispatch(&mut driver, &mut exec, None, &rd("tc-1", a.clone()), &lease)
        .unwrap();
    assert_eq!(exec.calls.get(), 1);
    // readers={B} never sees {A}'s entry — a miss, not a leak.
    let out_b = disp
        .dispatch(&mut driver, &mut exec, None, &rd("tc-2", b), &lease)
        .unwrap();
    assert!(matches!(out_b, DispatchOutcome::Observed(_)), "{out_b:?}");
    assert_eq!(exec.calls.get(), 2);
    // readers={A} again — the hit.
    let out_a = disp
        .dispatch(&mut driver, &mut exec, None, &rd("tc-3", a), &lease)
        .unwrap();
    assert!(
        matches!(out_a, DispatchOutcome::ObservedCached { .. }),
        "{out_a:?}"
    );
    assert_eq!(exec.calls.get(), 2);
    assert_eq!(
        cache_outcomes(disp.store_mut(), &run),
        vec!["miss".to_string(), "miss".to_string(), "hit".to_string()]
    );
}

// ── ADR-0129 d.1 — inadmissible calls bypass (never an error) ────────────────

#[test]
fn k4_inadmissible_bypasses_and_executes() {
    let (mut store, run, lease, _clock) = open("k4bypass");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("k4bypass");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // A read_only capability *without* `cacheable = true` — the opt-out row.
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), false);
    let bind = binding_named("test:read_file");
    let sb = scope_bindings();
    let mon = test_monitor(&[("test:read_file", &cap)]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_cache(K4Cache::new(MemoryStore::new("mem:k4")));
    let mut exec = MockExecutor::ok();
    let rd = |tc: &str| {
        input(
            &cap,
            &bind,
            &sb,
            &env,
            tc,
            read_args(&ws),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
            ReaderSet::Public,
            "test:read_file",
        )
    };
    let out1 = disp
        .dispatch(&mut driver, &mut exec, None, &rd("tc-1"), &lease)
        .unwrap();
    assert!(matches!(out1, DispatchOutcome::Observed(_)), "{out1:?}");
    let out2 = disp
        .dispatch(&mut driver, &mut exec, None, &rd("tc-2"), &lease)
        .unwrap();
    assert!(matches!(out2, DispatchOutcome::Observed(_)), "{out2:?}");
    assert_eq!(exec.calls.get(), 2, "bypassed calls always execute");
    assert_eq!(
        cache_outcomes(disp.store_mut(), &run),
        vec!["bypass".to_string(), "bypass".to_string()]
    );
    // The bypass reason is the declared opt-out, not a risk-class refusal.
    let envs = read_all(disp.store_mut(), &run);
    let reasons: Vec<_> = envs
        .iter()
        .filter(|e| e.class == "model.cache.resolved")
        .filter_map(|e| e.payload.get("reason").and_then(Json::as_str))
        .collect();
    assert!(
        reasons
            .iter()
            .all(|r| *r == "not_cacheable:capability_opt_out"),
        "{reasons:?}"
    );
}

// ── No cache attached ⇒ the pre-S2.10 pipeline verbatim ──────────────────────

#[test]
fn k4_absent_cache_is_the_plain_pipeline() {
    let (mut store, run, lease, _clock) = open("k4off");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("k4off");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(EffectDomain::FsRead, read_only_attrs(), true);
    let bind = binding_named("test:read_file");
    let sb = scope_bindings();
    let mon = test_monitor(&[("test:read_file", &cap)]);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let rd = |tc: &str| {
        input(
            &cap,
            &bind,
            &sb,
            &env,
            tc,
            read_args(&ws),
            EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(read_only_attrs()),
            },
            ReaderSet::Public,
            "test:read_file",
        )
    };
    disp.dispatch(&mut driver, &mut exec, None, &rd("tc-1"), &lease)
        .unwrap();
    disp.dispatch(&mut driver, &mut exec, None, &rd("tc-2"), &lease)
        .unwrap();
    assert_eq!(exec.calls.get(), 2);
    assert!(
        cache_outcomes(disp.store_mut(), &run).is_empty(),
        "no cache ⇒ no model.cache.resolved rows"
    );
}
