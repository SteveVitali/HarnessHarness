//! R-2.12 acceptance — DF-S2.7-1's three members (ADR-0269; ADR-0343 D4;
//! §5g.2 §2/§3):
//!
//! (a) the `labels_leaves` per-leaf admission walk — a `flow_contract`
//!     capability's shaped `structured_result` walks the declared
//!     `output_schema` leaf grammar; every leaf records
//!     `{path, admission, label}` on `context.observation.recorded`, and a
//!     non-admissible leaf refuses typed `conflict{admission_leaf:*}` on
//!     the observation plane (post-`committed` `action.effect.refused` is
//!     lifecycle-illegal — the refusal rides `observed{status: error}`).
//! (b) the remedy consume path — `DispatchInput.remedy_taken` is verified
//!     against the durable `security.permission.decided{remedy_taken}`
//!     attestation (never a caller claim), restated on
//!     `action.effect.authorized{remedy_taken}`, and consumed exactly once.
//! (c) the produce-time `FlowInputs.shape_endorsed` stamp — a
//!     `shape_endorse` take mints `action.param.anchored` +
//!     `security.label.endorsed{basis: validator, robustness_inputs[]}`,
//!     and the fold stamps the discharged params on real dispatch inputs.
//!
//! Each test fails if the behaviour it covers is removed. The scaffold
//! reuses the `r2_11.rs` shape: a real `Store`, a real `Monitor`, a real
//! `EnvDriver` over `Ep2Model::reference()`, a scripted executor.

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
use hh_env::dispatch::{shape_endorsed_params, DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::{EventMinter, ScopeChain};
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
use hh_identity::idp::address;
use hh_ledger::event::{Cursor, Direction, Scope};
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::monitor::{CapabilityEntry, Monitor};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_provenance::flow::Remedy;
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_wire::json::Json;

// ── scaffold (the `r2_11.rs` shape) ──────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r212-{}-{tag}-{n}", std::process::id()));
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

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_policy(ws: &std::path::Path) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    p.fs.write.allow.push(WritableRoot {
        root: ws.display().to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = hh_containment::policy::ExecPolicy::Allow(vec!["/bin".to_string()]);
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
        image: ImageRef::ContentAddress(address(b"r2.12-image", "application/octet-stream")),
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

fn reversible_attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
    }
}

/// `read_attr{flag}` — a `read_only` fs_read tool with a per-leaf-shaped
/// `output_schema` (`{ok: boolean, note: string}`) — the `labels_leaves`
/// fixture. `flow_contract{labels_leaves}` arms the walk. The declared
/// world is `open` — a tool-produced result is `unverified` provenance
/// (the interesting admission kind the walk stamps per leaf).
fn leaf_capability(output_schema: Option<Json>, labels_leaves: bool) -> ToolCapabilityRecord {
    let mut flow_contract = Json::obj([(
        "contribution",
        Json::obj([("readers_from", Json::str("reads"))]),
    )]);
    if let Json::Obj(m) = &mut flow_contract {
        m.insert("labels_leaves".to_string(), Json::Bool(labels_leaves));
    }
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [("flag", "boolean")]
                    .iter()
                    .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                    .collect(),
            ),
        )]),
        output_schema,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::FsRead,
                attributes: Some(EffectAttributes {
                    mutability: Mutability::ReadOnly,
                    repeat_safety: RepeatSafety::Idempotent,
                    world: World::Open,
                    reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
                }),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![
                Json::str("executor_error"),
                Json::str("timeout"),
                Json::str("signalled"),
                Json::str("invalid_arguments"),
                Json::str("conflict"),
            ]),
        )]),
        cost_model: None,
        execution_requirement: Json::Null,
        source: Json::Null,
        exposure_hint: Json::Null,
        postconditions: vec![],
        flow_contract: Some(flow_contract),
        action_patterns: Vec::new(),
    }
}

/// `write_file{path, content}` — the remedy-take fixture (the r2_11
/// scaffold's shape, unchanged).
fn write_capability() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [
                    ("path", "string"),
                    ("content", "string"),
                    ("flag", "boolean"),
                ]
                .iter()
                .map(|(k, t)| (k.to_string(), Json::obj([("type", Json::str(*t))])))
                .collect(),
            ),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::FsWrite,
                attributes: Some(reversible_attrs()),
            }]
            .into_iter()
            .collect(),
        ),
        preconditions: vec![],
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
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
        surface_name: "write_file".to_string(),
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        hir_node_id: "test:write_file".to_string(),
        arg_map: ["path", "content", "flag"]
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
                effect: EffectClass::domain_only(EffectDomain::FsRead),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
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
        flow: Default::default(),
        remedy_taken: None,
    }
}

/// An executor that reports a shaped `structured_result` capture item —
/// `value_ref` is the blob address of the canonical value bytes (the
/// emit-side record the leaf walk consumes).
struct ResultExecutor {
    value_ref: String,
    decl: ExecutorDeclaration,
}

impl ResultExecutor {
    fn new(value_ref: String) -> Self {
        ResultExecutor {
            value_ref,
            decl: ExecutorDeclaration {
                executor_id: "mock-result/1".to_string(),
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
                domains: [EffectDomain::FsRead, EffectDomain::FsWrite]
                    .iter()
                    .cloned()
                    .collect(),
            },
        }
    }
}

impl ToolExecutor for ResultExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        sink(ExecutorSignal {
            token: request.attribution_token.clone(),
            kind: CaptureKind::StructuredResult {
                schema_ref: "test:result-schema".to_string(),
                value_ref: self.value_ref.clone(),
            },
        });
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

struct MockExecutor {
    decl: ExecutorDeclaration,
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
                domains: [EffectDomain::FsRead, EffectDomain::FsWrite]
                    .iter()
                    .cloned()
                    .collect(),
            },
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

fn read_all(store: &Store, run: &str) -> Vec<hh_ledger::event::EventEnvelope> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
}

fn jstr<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

fn leaf_input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env: &str,
    tc: &str,
) -> DispatchInput<'a> {
    input(
        cap,
        bind,
        sb,
        env,
        tc,
        Json::obj([("flag", Json::Bool(true))]),
        EffectClass {
            domain: EffectDomain::FsRead,
            attributes: Some(EffectAttributes {
                mutability: Mutability::ReadOnly,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Open,
                reversibility: Reversibility::Reversible(Ref::selected("test:proc", "latest")),
            }),
        },
    )
}

fn write_input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env: &str,
    tc: &str,
    path: &str,
) -> DispatchInput<'a> {
    input(
        cap,
        bind,
        sb,
        env,
        tc,
        Json::obj([
            ("path", Json::str(path)),
            ("content", Json::str("hi")),
            ("flag", Json::Bool(true)),
        ]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
    )
}

/// Blob the canonical result value; answer the `value_ref` spelling the
/// `structured_result` capture item carries (`sha256:<digest>` — the
/// `blob_by_id` form).
fn blob_result(store: &mut Store, value: &Json) -> String {
    let addr = store
        .put_blob(value.to_canonical_string().as_bytes(), "application/json")
        .unwrap();
    format!("{}:{}", addr.algorithm, addr.digest)
}

/// Mint the durable `security.permission.decided{remedy_taken}` row — the
/// ingress surface's consume attestation (the member R-2.11 landed).
fn attest_take(store: &mut Store, run: &str, lease: &Lease, remedy: &Remedy) {
    let m = EventMinter::new(store, run);
    let ev = m
        .mint(
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str("perm-take-1")),
                ("decision", Json::str("deny")),
                ("remedy_taken", remedy.to_json()),
            ]),
        )
        .unwrap();
    store.append(run, lease, vec![ev]).unwrap();
}

// ── (a) `labels_leaves` — the per-leaf walk ─────────────────────────────────

/// Every leaf the declared `output_schema` covers records
/// `{path, admission, label}` on `context.observation.recorded` — the
/// structural position, never the value bytes.
#[test]
fn r212_labels_leaves_records_every_leaf() {
    let (mut store, run, lease, _clock) = open("leaves-ok");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("leaves-ok");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let schema = Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([
                ("ok", Json::obj([("type", Json::str("boolean"))])),
                ("note", Json::obj([("type", Json::str("string"))])),
            ]),
        ),
    ]);
    let cap = leaf_capability(Some(schema), true);
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let value_ref = blob_result(
        &mut store,
        &Json::obj([("ok", Json::Bool(true)), ("note", Json::str("done"))]),
    );
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = ResultExecutor::new(value_ref);
    let inp = leaf_input(&cap, &bind, &sb, &env, "tc-1");
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    let envs = read_all(disp.store_mut(), &run);
    let recorded = envs
        .iter()
        .find(|e| {
            e.class == "context.observation.recorded"
                && jstr(&e.payload, "kind") == Some("tool_observation")
        })
        .expect("recorded row");
    let leaves = recorded
        .payload
        .get("leaves")
        .and_then(|l| match l {
            Json::Arr(v) => Some(v),
            _ => None,
        })
        .expect("leaves member");
    let paths: Vec<&str> = leaves.iter().filter_map(|l| jstr(l, "path")).collect();
    assert_eq!(
        paths,
        vec!["/note", "/ok"],
        "every leaf position: {leaves:?}"
    );
    for leaf in leaves {
        assert_eq!(
            jstr(leaf, "admission"),
            Some("unverified"),
            "each leaf carries the contract's admitted kind"
        );
        assert!(leaf.get("label").is_some(), "each leaf carries L(r)");
    }
    // No refusal — the walk covered every leaf.
    assert!(
        !envs.iter().any(|e| e.class == "action.effect.refused"),
        "a fully-covered result never refuses"
    );
}

/// A member the schema does not cover is a non-admissible leaf — the typed
/// refusal is `observed{status: error{conflict{admission_leaf:uncovered:*}}}`
/// + `recorded{admission: leaf_refused, leaf}`, and no post-commit
///   `action.effect.refused` is minted (the lifecycle rule).
#[test]
fn r212_uncovered_leaf_refuses_typed() {
    let (mut store, run, lease, _clock) = open("leaves-uncovered");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("leaves-uncovered");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let schema = Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([("ok", Json::obj([("type", Json::str("boolean"))]))]),
        ),
        ("additionalProperties", Json::Bool(false)),
    ]);
    let cap = leaf_capability(Some(schema), true);
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let value_ref = blob_result(
        &mut store,
        &Json::obj([("ok", Json::Bool(true)), ("extra", Json::Int(1))]),
    );
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = ResultExecutor::new(value_ref);
    let inp = leaf_input(&cap, &bind, &sb, &env, "tc-1");
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    let envs = read_all(disp.store_mut(), &run);
    let observed = envs
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .expect("observed");
    let err = observed
        .payload
        .get("status")
        .and_then(|s| s.get("error"))
        .expect("status.error — the refusal rides the observation plane");
    assert_eq!(jstr(err, "class"), Some("conflict"));
    assert_eq!(
        jstr(err, "kind"),
        Some("admission_leaf:uncovered:/extra"),
        "the failing leaf position is named"
    );
    let recorded = envs
        .iter()
        .find(|e| {
            e.class == "context.observation.recorded"
                && jstr(&e.payload, "kind") == Some("tool_observation")
        })
        .expect("recorded refusal row");
    assert_eq!(jstr(&recorded.payload, "admission"), Some("leaf_refused"));
    assert_eq!(
        jstr(&recorded.payload, "leaf"),
        Some("admission_leaf:uncovered:/extra")
    );
    assert!(
        recorded.payload.get("label").is_none(),
        "a refused leaf mints no admitted label"
    );
    assert!(
        !envs.iter().any(|e| e.class == "action.effect.refused"),
        "post-commit `refused` is lifecycle-illegal — never minted"
    );
}

/// A schema keyword outside the OQ-219 admitted subset makes the leaf
/// grammar unreadable — `admission_leaf:unchecked_keyword:*`, never a
/// guess.
#[test]
fn r212_unchecked_keyword_refuses_typed() {
    let (mut store, run, lease, _clock) = open("leaves-kw");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("leaves-kw");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let schema = Json::obj([
        ("type", Json::str("object")),
        (
            "patternProperties",
            Json::obj([("^x", Json::obj([("type", Json::str("string"))]))]),
        ),
    ]);
    let cap = leaf_capability(Some(schema), true);
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let value_ref = blob_result(&mut store, &Json::obj([("x1", Json::str("v"))]));
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = ResultExecutor::new(value_ref);
    let inp = leaf_input(&cap, &bind, &sb, &env, "tc-1");
    disp.dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();

    let envs = read_all(disp.store_mut(), &run);
    let observed = envs
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .unwrap();
    let err = observed
        .payload
        .get("status")
        .and_then(|s| s.get("error"))
        .expect("status.error");
    assert_eq!(jstr(err, "class"), Some("conflict"));
    assert_eq!(
        jstr(err, "kind"),
        Some("admission_leaf:unchecked_keyword:patternProperties")
    );
}

/// A `labels_leaves` contract whose result has no `structured_result`
/// capture — the leaf grammar has nothing to bind.
#[test]
fn r212_no_shaped_result_refuses_typed() {
    let (mut store, run, lease, _clock) = open("leaves-none");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("leaves-none");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let schema = Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([("ok", Json::obj([("type", Json::str("boolean"))]))]),
        ),
    ]);
    let cap = leaf_capability(Some(schema), true);
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok(); // emits no structured_result
    let inp = leaf_input(&cap, &bind, &sb, &env, "tc-1");
    disp.dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();

    let envs = read_all(disp.store_mut(), &run);
    let observed = envs
        .iter()
        .find(|e| e.class == "action.effect.observed")
        .unwrap();
    let err = observed
        .payload
        .get("status")
        .and_then(|s| s.get("error"))
        .expect("status.error");
    assert_eq!(jstr(err, "class"), Some("conflict"));
    assert_eq!(jstr(err, "kind"), Some("admission_leaf:no_shaped_result"));
}

/// A `labels_leaves = false` contract is byte-identical to the pre-R2.12
/// shape — no `leaves` member, no walk (absent-not-null).
#[test]
fn r212_no_labels_leaves_is_unchanged() {
    let (mut store, run, lease, _clock) = open("leaves-off");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("leaves-off");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = leaf_capability(None, false);
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = leaf_input(&cap, &bind, &sb, &env, "tc-1");
    disp.dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    let envs = read_all(disp.store_mut(), &run);
    let recorded = envs
        .iter()
        .find(|e| {
            e.class == "context.observation.recorded"
                && jstr(&e.payload, "kind") == Some("tool_observation")
        })
        .expect("recorded row (contract present, no leaves)");
    assert!(recorded.payload.get("leaves").is_none());
}

// ── (b) `remedy_taken` — the consume path ────────────────────────────────────

/// An unattested `remedy_taken` claim is a typed refusal
/// (`remedy_not_attested`) — the dispatcher reads the durable record,
/// never the caller's word (CC3).
#[test]
fn r212_remedy_claim_without_attestation_refuses() {
    let (mut store, run, lease, _clock) = open("take-none");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("take-none");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = write_capability();
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mut inp = write_input(&cap, &bind, &sb, &env, "tc-1", "/tmp/x");
    inp.remedy_taken = Some(Remedy::Approval {
        effect_id: "ef-other".to_string(),
    });
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    match out {
        DispatchOutcome::Refused { reason } => {
            assert_eq!(reason, "remedy_not_attested:approval", "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    let envs = read_all(disp.store_mut(), &run);
    let refused = envs
        .iter()
        .find(|e| e.class == "action.effect.refused")
        .expect("refused durable");
    assert_eq!(
        jstr(&refused.payload, "reason"),
        Some("remedy_not_attested:approval")
    );
    assert!(
        !envs.iter().any(|e| e.class == "action.effect.authorized"),
        "an unattested claim never authorizes"
    );
}

/// An attested take rides the re-dispatch end-to-end: the `authorized` row
/// carries `remedy_taken` — the consume record's effect-side half.
#[test]
fn r212_attested_remedy_reaches_authorized() {
    let (mut store, run, lease, _clock) = open("take-ok");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("take-ok");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = write_capability();
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let remedy = Remedy::Substitute {
        param: "content".to_string(),
        min_authority: AuthorityClass::Principal,
    };
    attest_take(&mut store, &run, &lease, &remedy);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mut inp = write_input(&cap, &bind, &sb, &env, "tc-1", "/tmp/x");
    inp.remedy_taken = Some(remedy.clone());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");
    let envs = read_all(disp.store_mut(), &run);
    let authorized = envs
        .iter()
        .find(|e| e.class == "action.effect.authorized")
        .expect("authorized durable");
    assert_eq!(
        authorized.payload.get("remedy_taken"),
        Some(&remedy.to_json()),
        "the authorized row restates the consumed remedy"
    );
}

/// A remedy is consumed exactly once — a *second* effect claiming the same
/// take after the first's `authorized` consume record is `remedy_replayed`.
#[test]
fn r212_remedy_replay_refuses() {
    let (mut store, run, lease, _clock) = open("take-replay");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("take-replay");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = write_capability();
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let remedy = Remedy::Substitute {
        param: "content".to_string(),
        min_authority: AuthorityClass::Principal,
    };
    attest_take(&mut store, &run, &lease, &remedy);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mut inp = write_input(&cap, &bind, &sb, &env, "tc-1", "/tmp/x");
    inp.remedy_taken = Some(remedy.clone());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    // A second effect re-claims the identical take — replayed.
    let mut inp2 = write_input(&cap, &bind, &sb, &env, "tc-2", "/tmp/y");
    inp2.remedy_taken = Some(remedy);
    let out2 = disp
        .dispatch(&mut driver, &mut exec, None, &inp2, &lease)
        .unwrap();
    match out2 {
        DispatchOutcome::Refused { reason } => {
            assert_eq!(reason, "remedy_replayed:substitute", "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ── (c) `shape_endorsed` — the produce-time stamp ────────────────────────────

/// A `shape_endorse` take mints `action.param.anchored` +
/// `security.label.endorsed{basis: validator, robustness_inputs: [param],
/// capacity_bits}` durable — and the produce-time fold discharges the
/// param on the next dispatch's `FlowInputs.shape_endorsed`.
#[test]
fn r212_shape_endorse_mints_validator_endorsement_and_stamps() {
    let (mut store, run, lease, _clock) = open("endorse");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("endorse");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let mut cap = write_capability();
    cap.flow_contract = Some(Json::obj([(
        "contribution",
        Json::obj([("readers_from", Json::str("reads"))]),
    )]));
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let remedy = Remedy::ShapeEndorse {
        validator_ref: Some("test:validator".to_string()),
        param: "flag".to_string(),
    };
    attest_take(&mut store, &run, &lease, &remedy);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mut inp = write_input(&cap, &bind, &sb, &env, "tc-1", "/tmp/x");
    // The param sits at `external` — the validator basis's only raisable
    // source (§5g.2 §2.2: external → environment, taint → ∅).
    inp.flow
        .param_labels
        .insert("flag".to_string(), Label::at(AuthorityClass::External));
    inp.remedy_taken = Some(remedy.clone());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(matches!(out, DispatchOutcome::Observed(_)), "{out:?}");

    let envs = read_all(disp.store_mut(), &run);
    let anchor = envs
        .iter()
        .find(|e| e.class == "action.param.anchored")
        .expect("param anchor durable");
    assert_eq!(jstr(&anchor.payload, "param"), Some("flag"));
    assert_eq!(
        anchor.provenance.as_ref().map(|p| p.authority),
        Some(AuthorityClass::External),
        "the anchor carries the param's own label (the honest `from`)"
    );
    let endorsed = envs
        .iter()
        .find(|e| e.class == "security.label.endorsed")
        .expect("endorsed durable");
    let p = &endorsed.payload;
    assert_eq!(jstr(p, "basis"), Some("validator"));
    assert_eq!(jstr(p, "basis_ref"), Some("test:validator"));
    assert_eq!(
        jstr(p, "subject_ref"),
        Some(anchor.event_id.as_str()),
        "the subject is the durable param anchor"
    );
    assert_eq!(p.get("capacity_bits"), Some(&Json::Int(1)));
    assert_eq!(
        p.get("robustness_inputs"),
        Some(&Json::Arr(vec![Json::str("flag")])),
        "the endorsement discharges `flag` for D-ROBUST"
    );
    assert_eq!(jstr(p.get("to").unwrap(), "authority"), Some("environment"));

    // The produce-time fold (the next dispatch's stamp): the durable row's
    // `robustness_inputs` discharge `flag` *for these args* — the binding
    // is args-scoped through the anchor's `args_canonical_hash`.
    let args_hash = jstr(&anchor.payload, "args_canonical_hash").unwrap();
    let stamped = shape_endorsed_params(&envs, args_hash);
    assert!(
        stamped.contains("flag"),
        "the fold discharges the endorsed param: {stamped:?}"
    );
    assert!(
        !shape_endorsed_params(&envs, "sha256:different-args").contains("flag"),
        "an endorsement never launders across re-supplied args"
    );
}

/// A `shape_endorse` take on a param whose label sits above `external` is
/// inapplicable — `validator` raises only `external → environment`; the
/// dispatch refuses `remedy_inapplicable`, never mints a lie.
#[test]
fn r212_shape_endorse_inapplicable_refuses() {
    let (mut store, run, lease, _clock) = open("endorse-no");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("endorse-no");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = write_capability();
    let bind = binding();
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mon = test_monitor(&cap);
    let remedy = Remedy::ShapeEndorse {
        validator_ref: None,
        param: "flag".to_string(),
    };
    attest_take(&mut store, &run, &lease, &remedy);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let mut inp = write_input(&cap, &bind, &sb, &env, "tc-1", "/tmp/x");
    // `principal` — already above `external`; the validator basis cannot
    // raise it (`external → environment` only).
    inp.flow
        .param_labels
        .insert("flag".to_string(), Label::at(AuthorityClass::Principal));
    inp.remedy_taken = Some(remedy);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    match out {
        DispatchOutcome::Refused { reason } => {
            assert!(reason.starts_with("remedy_inapplicable:"), "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    let envs = read_all(disp.store_mut(), &run);
    assert!(
        !envs.iter().any(|e| e.class == "security.label.endorsed"),
        "an inapplicable take mints no endorsement"
    );
}
