//! R6.37 acceptance battery (manifest 295; REQ-XE-109; `AC-7.3-7`;
//! ADR-0440 D2) — G18: durable model-view rendering of exec
//! observations, re-served on replay:
//!
//! - **AC-1** — live-model-over-replay: an armed exec + `GatewayModel`
//!   over R6.12's `RecordedTransport` ⇒ the `tool` message equals the
//!   string the capability's `ResultRenderSpec` renders from the raw
//!   output (one case above the threshold, one below).
//! - **AC-2** — durable before visible: the `context.observation.
//!   recorded` row carrying `model_view.ref` precedes the
//!   `model.call.requested`/`model_io` of the request that carries it.
//! - **AC-3** — kill-point: drop + real store reopen between the
//!   recording and the carrying request ⇒ byte-identical `tool`
//!   content re-served from the recorded address; a changed renderer
//!   parameter never re-renders.
//! - **AC-4** — opt-in: unarmed and renderer-less paths are
//!   byte-identical (the `no_observation_recorded` ledger record);
//!   `classes.rs` registers no new class; `model_view` is
//!   absent-not-null.
//!
//! USD $0 — real `Store`, stub `ToolExecutor`, `RecordedTransport`
//! only. Goldens are `provenance: hand_derived` (REQ-XE-114) — the
//! assertion oracle is the renderer itself (the AC's own definition),
//! never a second rendering path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::equiv::SurfaceBinding;
use hharness_compiler::lower::CompiledToolSurface;
use hharness_compiler::plan::PinnedRef;
use hharness_compiler::result_render::render;
use hharness_compiler::seal::ModelSurface;
use hharness_compiler::surface::{
    HeadTailUnit, RenderMode, ResultRenderSpec, ResultRenderer, TruncateDirection,
};
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ExecPolicy, NetMode, WritableRoot};
use hharness_control::driver::{DispatchSeam, EffectGate, ModelPort};
use hharness_control::vocab::SettledOutcome;
use hharness_embed::gateway_model::{decode_model_lane, GatewayModel};
use hharness_embed::runtime::{DispatchGate, EmbedGate, ExecCap};
use hharness_env::capture::{CaptureKind, OutputPolicy};
use hharness_env::deadline::DeadlineLadder;
use hharness_env::driver::EnvDriver;
use hharness_env::errors::EnvError;
use hharness_env::events::EventMinter;
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_gateway::http_transport::RawExchange;
use hharness_gateway::replay::{recorded_call, RecordedTransport};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_identity::idp::{self, address};
use hharness_ledger::event::Scope;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::decision::DecisionScope;
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::{AuthorityClass, Label, ProvenanceRecord};
use hharness_wire::json::Json;

// ── scratch rig (the r5_8 battery's shape) ───────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r637-{}-{tag}-{n}", std::process::id()));
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

/// Reopen an existing store root — the resume leg (the durable prefix
/// and the blob pool fold from the WAL; no new run is minted).
fn reopen(root: &std::path::Path) -> Store {
    Store::open_with(
        root.to_path_buf(),
        Box::new(ManualClock::at(1_000)),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap()
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
/// coordinate `ef` under `tc` — the driver-side shape (r5_8's
/// `open_adopted_scopes`, parameterized on tc).
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
        image: ImageRef::ContentAddress(address(b"r6.37-image", "application/octet-stream")),
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

fn allow_monitor(cap: &ToolCapabilityRecord) -> Monitor {
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
            expires_at: Some(HandleExpiry::Run("run-1".into())),
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
    m.run_id = "run-1".to_string();
    m.turn_id = "turn-1".to_string();
    m.effect_id = "e1".to_string();
    m.capabilities.insert(
        "test:bash".to_string(),
        CapabilityEntry {
            record: cap.clone(),
            binding: bash_binding(),
        },
    );
    m
}

fn exec_cap(exec: Box<dyn ToolExecutor>) -> ExecCap {
    ExecCap {
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
        output_policy: OutputPolicy {
            retain_bytes_cap: 1 << 20,
            model_view: "full".to_string(),
            offload_above: 1 << 20,
            max_deltas: 64,
        },
        executor: exec,
        monitor: allow_monitor(&exec_cap_record()),
    }
}

/// A stub executor emitting each call's stdout chunk then an `ok`
/// terminal — the queued outputs model the armed exec's raw output.
struct OutputExec {
    decl: ExecutorDeclaration,
    outputs: VecDeque<String>,
}

impl OutputExec {
    fn new(outputs: &[&str]) -> Self {
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

/// The declared `text_head_tail@1` renderer — threshold 10 chars,
/// head 4 / tail 4, a `hh-tmpl/1` marker leaf.
fn renderer() -> ResultRenderer {
    ResultRenderer::TextHeadTail {
        threshold: 10,
        head: 4,
        tail: 4,
        unit: HeadTailUnit::Chars,
        marker: Text::new(
            "...[{{omitted}} omitted]...".to_string(),
            "test:owner".to_string(),
            ProvenanceRecord::kernel("t", 0),
        ),
    }
}

/// The armed `ModelSurface` — one compiled tool surface `bash` →
/// `exec.bash` carrying the declared `ResultRenderSpec`.
fn armed_surface(r: Option<ResultRenderer>) -> ModelSurface {
    ModelSurface {
        profile: "sha256:p@1".into(),
        layout: vec![],
        tools: vec![CompiledToolSurface {
            binding: bash_binding(),
            schema: Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("command", Json::obj([("type", Json::str("string"))]))]),
                ),
                ("required", Json::Arr(vec![Json::str("command")])),
            ]),
            description: "Execute a bash command".to_string(),
            error_format: None,
            result_render: Some(ResultRenderSpec {
                mode: RenderMode::Truncate {
                    max_lines: None,
                    max_bytes: None,
                    max_tokens: None,
                    direction: TruncateDirection::Tail,
                },
                declared_loss: None,
                validator_reads: vec![],
                renderer: r,
            }),
            equivalence: None,
        }],
        interaction_mode: "native_fc".into(),
        params: Json::obj([]),
        transcript_renderer: Json::obj([]),
        dialects: BTreeMap::new(),
    }
}

/// The `surface_id → renderer` map `service.rs::drive` builds for the
/// gate — the same pure read the session applies.
fn renderers_of(surface: &ModelSurface) -> BTreeMap<String, ResultRenderer> {
    surface
        .tools
        .iter()
        .filter_map(|t| {
            t.result_render
                .as_ref()
                .and_then(|s| s.renderer.clone())
                .map(|r| (t.binding.surface_id.clone(), r))
        })
        .collect()
}

fn lane() -> hharness_embed::gateway_model::ModelLane {
    let mut m = BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str("http://127.0.0.1:1"));
    m.insert("provider_model_id".to_string(), Json::str("fixture-model"));
    decode_model_lane(&Json::Obj(m), "profile:test").unwrap()
}

/// A recorded `chat.completion` answering with the two `bash` tool
/// calls (call_0 → `mc.1.tc0`, call_1 → `mc.1.tc1`).
fn tool_call_exchange() -> RawExchange {
    let body = r#"{"choices":[{"finish_reason":"tool_calls","index":0,"message":{"content":"","role":"assistant","tool_calls":[{"function":{"arguments":"{\"command\":\"big\"}","name":"bash"},"id":"call_0","type":"function"},{"function":{"arguments":"{\"command\":\"small\"}","name":"bash"},"id":"call_1","type":"function"}]}}],"id":"chatcmpl-1","model":"fixture-model","object":"chat.completion"}"#;
    RawExchange {
        status: 200,
        headers: Default::default(),
        body: body.as_bytes().to_vec(),
        chunk_stamps: vec![(0, body.len())],
    }
}

/// A plain end-turn `chat.completion` for the carrying request.
fn end_turn_exchange() -> RawExchange {
    let body = r#"{"choices":[{"finish_reason":"stop","index":0,"message":{"content":"done","role":"assistant"}}],"id":"chatcmpl-2","model":"fixture-model","object":"chat.completion"}"#;
    RawExchange {
        status: 200,
        headers: Default::default(),
        body: body.as_bytes().to_vec(),
        chunk_stamps: vec![(0, body.len())],
    }
}

fn blobs_for<'a>(store: &'a Store) -> impl Fn(&str) -> Option<Vec<u8>> + 'a {
    move |id: &str| {
        let pid = idp::parse_id(id).ok()?;
        store
            .get_blob(&idp::ContentAddress {
                idp: "idp/1",
                algorithm: "sha256",
                digest: pid.digest_hex,
                media_type: String::new(),
                size: 0,
            })
            .ok()
    }
}

fn read_all(store: &Store, run: &str) -> Vec<hharness_ledger::event::EventEnvelope> {
    store
        .read(
            run,
            hharness_ledger::event::Cursor::Seq(0),
            None,
            hharness_ledger::event::Direction::Fwd,
            usize::MAX,
        )
        .unwrap()
        .events
}

/// Mint the driver-side `model.call.completed` row from the port's
/// outcome — the same `calls[]`/`response_ref` shape the driver folds.
fn mint_completed(
    store: &mut Store,
    run: &str,
    lease: &Lease,
    mc: &str,
    outcome: &hharness_control::driver::ModelOutcome,
) {
    let m = EventMinter::new(store, run);
    let mut e = m
        .mint(
            "model.call.completed",
            Json::obj([
                ("model_call_id", Json::str(mc)),
                ("attempt_no", Json::Int(1)),
                ("stop_reason", Json::str("tool_calls")),
                ("text_empty", Json::Bool(true)),
                ("response_ref", Json::str(outcome.response_ref.clone())),
                (
                    "calls",
                    Json::Arr(
                        outcome
                            .calls
                            .iter()
                            .map(|c| {
                                Json::obj([
                                    ("tool_call_id", Json::str(c.tool_call_id.clone())),
                                    ("surface", Json::str(c.surface.clone())),
                                    ("args_raw", Json::str(c.args_raw.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        )
        .unwrap();
    e.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some(mc.to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![e]).unwrap();
}

fn mint_requested(store: &mut Store, run: &str, lease: &Lease, mc: &str) {
    let m = EventMinter::new(store, run);
    let mut e = m
        .mint(
            "model.call.requested",
            Json::obj([("model_call_id", Json::str(mc))]),
        )
        .unwrap();
    e.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some(mc.to_string()),
        ..Scope::default()
    };
    store.append(run, lease, vec![e]).unwrap();
}

fn tool_messages(body: &Json) -> Vec<(String, String)> {
    let Json::Arr(msgs) = body.get("messages").unwrap() else {
        panic!("messages")
    };
    msgs.iter()
        .filter(|m| m.get("role").and_then(Json::as_str) == Some("tool"))
        .map(|m| {
            (
                m.get("tool_call_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                m.get("content")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

// ── AC-1 + AC-2 — live model over replay; durable before visible ─────────────

#[test]
fn ac1_ac2_tool_message_is_the_durable_rendering() {
    let (mut store, run, lease) = open("ac1");
    let ws = dir("ac1-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    open_turn_scopes(&mut store, &run, &lease);
    // The adopted coordinates the driver opened for the two calls.
    open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");
    open_adopted_scopes(&mut store, &run, &lease, "ef-2", "mc.1.tc1");

    let surface = armed_surface(Some(renderer()));
    let big = "0123456789ABCDEF"; // 16 chars ≥ threshold 10 → head+marker+tail
    let small = "short"; // 5 chars < 10 → whole
    let mut gate = DispatchGate {
        embed: EmbedGate {
            host_surfaces: BTreeMap::new(),
            host_asks: Vec::new(),
        },
        exec_caps: [(
            "exec.bash".to_string(),
            exec_cap(Box::new(OutputExec::new(&[big, small]))),
        )]
        .into_iter()
        .collect(),
        run_id: run.clone(),
        env_handle_id: Some(env),
        seed: [7u8; 32],
        result_renderers: renderers_of(&surface),
        submission_marker: None,
    };
    // The live model leg — mc.1's request is built by the armed port
    // and the recorded upstream answer decodes through the family's
    // own translators (R6.12's `RecordedTransport`).
    let dialect = hharness_gateway::dialects::openai_compatible();
    let transport = RecordedTransport::new(
        &dialect,
        "openai.chat",
        vec![
            recorded_call(vec![], tool_call_exchange(), None),
            recorded_call(vec![], end_turn_exchange(), None),
        ],
    );
    let mut port = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(surface.clone())
        .with_transport(Box::new(transport));
    let prefix0: Vec<hharness_ledger::event::EventEnvelope> = read_all(&store, &run);
    let ms1 = port.requested_members("mc.1", &Json::Null, &prefix0, &|_: &str| None);
    assert!(ms1.iter().all(|(k, _)| *k != "plan_error"), "{ms1:?}");
    let outcome = port.call("mc.1", &Json::Null);
    assert_eq!(outcome.calls.len(), 2, "{outcome:?}");
    // The driver deposits the port's `hh.model_io/1` blobs (the
    // driver's own fence — mirrored here).
    for (media, bytes) in port.take_model_io() {
        store.put_blob(&bytes, &media).unwrap();
    }

    // The two armed exec dispatches settle through the real
    // `Dispatcher` — `context.observation.recorded{model_view}` lands
    // in the fenced batch (the render's blob precedes any request).
    for (ef, tc) in [("ef-1", "mc.1.tc0"), ("ef-2", "mc.1.tc1")] {
        let intent = Json::obj([
            ("surface_id", Json::str("exec.bash")),
            ("tool_call_id", Json::str(tc)),
            ("args", Json::obj([("command", Json::str("x"))])),
        ]);
        let seam = DispatchSeam {
            store: &mut store,
            lease: &lease,
            envs: Some(&mut driver),
        };
        let out = gate
            .dispatch_durable(seam, ef, 1, &intent)
            .expect("armed surface");
        assert!(
            matches!(out.outcome, SettledOutcome::Observed { .. }),
            "{ef}: {out:?}"
        );
    }
    // `model.call.completed` closes `mc.1` — it lands after the calls
    // it spawned settle (the driver's own order).
    mint_completed(&mut store, &run, &lease, "mc.1", &outcome);
    let events = read_all(&store, &run);
    let recorded: Vec<&hharness_ledger::event::EventEnvelope> = events
        .iter()
        .filter(|e| e.class == "context.observation.recorded")
        .collect();
    assert_eq!(recorded.len(), 2, "one row per armed observation");
    // `model_view{ref, renderer}` on each row — absent-not-null
    // admission/label (a contract-less capability never minted them).
    let mut view_refs: BTreeMap<String, String> = BTreeMap::new();
    for r in &recorded {
        let tc = r.scope.tool_call_id.clone().expect("scope");
        assert!(r.payload.get("admission").is_none());
        assert!(r.payload.get("label").is_none());
        let mv = r.payload.get("model_view").expect("model_view member");
        assert_eq!(
            mv.get("renderer").and_then(Json::as_str),
            Some("text_head_tail@1")
        );
        let rref = mv.get("ref").and_then(Json::as_str).expect("ref");
        view_refs.insert(tc, rref.to_string());
    }
    // The durable blob's bytes are the renderer's own output — the one
    // pure home computes the expectation (never a second render path).
    for (tc, out) in [("mc.1.tc0", big), ("mc.1.tc1", small)] {
        let expected = render(
            &renderer(),
            &BTreeMap::from([
                ("output".to_string(), Json::str(out.to_string())),
                ("returncode".to_string(), Json::Int(0)),
            ]),
        )
        .unwrap();
        let blob = blobs_for(&store)(&view_refs[tc]).expect("durable blob");
        assert_eq!(
            String::from_utf8_lossy(&blob),
            expected.as_str(),
            "{tc} blob bytes"
        );
    }

    // mc.2 — the request that carries the observations: the `tool`
    // messages equal the recorded rendered strings verbatim.
    let prefix = read_all(&store, &run);
    let ms2 = port.requested_members("mc.2", &Json::Null, &prefix, &blobs_for(&store));
    assert!(ms2.iter().all(|(k, _)| *k != "plan_error"), "{ms2:?}");
    let body = port.pending_body("mc.2").unwrap().clone();
    let tools = tool_messages(&body);
    assert_eq!(tools.len(), 2, "{tools:?}");
    let expected_big = render(
        &renderer(),
        &BTreeMap::from([
            ("output".to_string(), Json::str(big.to_string())),
            ("returncode".to_string(), Json::Int(0)),
        ]),
    )
    .unwrap();
    let expected_small = render(
        &renderer(),
        &BTreeMap::from([
            ("output".to_string(), Json::str(small.to_string())),
            ("returncode".to_string(), Json::Int(0)),
        ]),
    )
    .unwrap();
    assert_eq!(tools[0], ("mc.1.tc0".to_string(), expected_big.clone()));
    assert_eq!(tools[1], ("mc.1.tc1".to_string(), expected_small.clone()));

    // AC-2 — the recording row precedes the `model.call.requested` and
    // the `model_io` deposit of the carrying request: mint the
    // request row now (the driver's own order), drive `call` (the
    // `model_io` blob lands inside it), and pin the seqs.
    mint_requested(&mut store, &run, &lease, "mc.2");
    let outcome2 = port.call("mc.2", &Json::Null);
    for (media, bytes) in port.take_model_io() {
        store.put_blob(&bytes, &media).unwrap();
    }
    mint_completed(&mut store, &run, &lease, "mc.2", &outcome2);
    let events = read_all(&store, &run);
    let seq = |class: &str, mc: &str| {
        events
            .iter()
            .find(|e| {
                e.class == class
                    && (mc.is_empty()
                        || e.scope.model_call_id.as_deref() == Some(mc)
                        || e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc))
            })
            .map(|e| e.seq)
            .unwrap()
    };
    let rec_seqs: Vec<u64> = recorded.iter().map(|r| r.seq).collect();
    let req2 = seq("model.call.requested", "mc.2");
    let cmp2 = seq("model.call.completed", "mc.2");
    for s in &rec_seqs {
        assert!(
            *s < req2,
            "recorded seq {s} must precede request seq {req2}"
        );
        assert!(
            *s < cmp2,
            "recorded seq {s} must precede the model_io terminal {cmp2}"
        );
    }
    // The `model_io` blob the mc.2 call deposited carries the rendered
    // request bytes — the carried rendering, recorded.
    let io_blob = blobs_for(&store)(&outcome2.response_ref).expect("mc.2 model_io");
    let io_doc = hharness_wire::json::parse(&String::from_utf8_lossy(&io_blob)).unwrap();
    let req_body = io_doc.get("request_body").unwrap();
    assert_eq!(*req_body, body, "model_io records the carrying request");
}

// ── AC-3 — kill after the recording; the resume re-serves the bytes ──────────

#[test]
fn ac3_kill_and_resume_reserves_the_recorded_bytes() {
    let (mut store, run, lease) = open("ac3");
    let ws = dir("ac3-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    open_turn_scopes(&mut store, &run, &lease);
    open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");

    let surface = armed_surface(Some(renderer()));
    let out_str = "0123456789ABCDEF";
    let mut gate = DispatchGate {
        embed: EmbedGate {
            host_surfaces: BTreeMap::new(),
            host_asks: Vec::new(),
        },
        exec_caps: [(
            "exec.bash".to_string(),
            exec_cap(Box::new(OutputExec::new(&[out_str]))),
        )]
        .into_iter()
        .collect(),
        run_id: run.clone(),
        env_handle_id: Some(env),
        seed: [7u8; 32],
        result_renderers: renderers_of(&surface),
        submission_marker: None,
    };
    let intent = Json::obj([
        ("surface_id", Json::str("exec.bash")),
        ("tool_call_id", Json::str("mc.1.tc0")),
        ("args", Json::obj([("command", Json::str("x"))])),
    ]);
    let seam = DispatchSeam {
        store: &mut store,
        lease: &lease,
        envs: Some(&mut driver),
    };
    let out = gate
        .dispatch_durable(seam, "ef-1", 1, &intent)
        .expect("armed surface");
    assert!(
        matches!(out.outcome, SettledOutcome::Observed { .. }),
        "{out:?}"
    );
    // The call row the transcript pairs on — minted after the dispatch
    // (it closes `mc.1`).
    let m = EventMinter::new(&mut store, &run);
    let mut e = m
        .mint(
            "model.call.completed",
            Json::obj([
                ("model_call_id", Json::str("mc.1")),
                ("attempt_no", Json::Int(1)),
                ("stop_reason", Json::str("tool_calls")),
                ("response_ref", Json::str("sha256:00")),
                (
                    "calls",
                    Json::Arr(vec![Json::obj([
                        ("tool_call_id", Json::str("mc.1.tc0")),
                        ("surface", Json::str("exec.bash")),
                        ("args_raw", Json::str("{}")),
                    ])]),
                ),
            ]),
        )
        .unwrap();
    e.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc.1".into()),
        ..Scope::default()
    };
    store.append(&run, &lease, vec![e]).unwrap();
    // The durable `model_view.ref` — the recording before the kill.
    let events = read_all(&store, &run);
    let rref = events
        .iter()
        .find(|e| e.class == "context.observation.recorded")
        .and_then(|e| {
            e.payload
                .get("model_view")
                .and_then(|mv| mv.get("ref"))
                .and_then(Json::as_str)
        })
        .expect("model_view.ref")
        .to_string();
    let recorded_bytes = blobs_for(&store)(&rref).expect("recorded blob");

    // ── kill: drop every handle, reopen the root — the WAL folds the
    // recorded row + the blob pool (the resume leg). ──
    let root = store.root().to_path_buf();
    drop(gate);
    drop(driver);
    drop(lease);
    drop(store);
    let store2 = reopen(&root);
    let prefix = read_all(&store2, &run);

    // A *changed* renderer parameter set on the resumed side — replay
    // must not consult it (the recorded bytes are the serve).
    let mut drifted = renderer();
    let ResultRenderer::TextHeadTail { threshold, .. } = &mut drifted else {
        unreachable!()
    };
    *threshold = 1;
    let mut resumed = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(armed_surface(Some(drifted)));
    let ms = resumed.requested_members("mc.2", &Json::Null, &prefix, &blobs_for(&store2));
    assert!(ms.iter().all(|(k, _)| *k != "plan_error"), "{ms:?}");
    let tools = tool_messages(resumed.pending_body("mc.2").unwrap());
    assert_eq!(tools.len(), 1, "{tools:?}");
    assert_eq!(tools[0].0, "mc.1.tc0");
    assert_eq!(
        tools[0].1.as_bytes(),
        recorded_bytes.as_slice(),
        "the resume re-serves the recorded bytes, byte-identical"
    );
    // The drifted threshold would have rendered differently — proof
    // the serve came from the record, never a re-render.
    let drifted_render = render(
        &ResultRenderer::TextHeadTail {
            threshold: 1,
            head: 4,
            tail: 4,
            unit: HeadTailUnit::Chars,
            marker: Text::new(
                "...[{{omitted}} omitted]...".to_string(),
                "test:owner".to_string(),
                ProvenanceRecord::kernel("t", 0),
            ),
        },
        &BTreeMap::from([
            ("output".to_string(), Json::str(out_str.to_string())),
            ("returncode".to_string(), Json::Int(0)),
        ]),
    )
    .unwrap();
    assert_ne!(tools[0].1, drifted_render);
    let _ = store2;
}

// ── AC-4 — unarmed and renderer-less paths are byte-identical ────────────────

#[test]
fn ac4_unarmed_and_rendererless_stay_byte_identical() {
    for armed in [false, true] {
        let (mut store, run, lease) = open(if armed { "ac4-no-rend" } else { "ac4-unarmed" });
        let ws = dir(if armed { "ac4b-ws" } else { "ac4a-ws" });
        std::fs::create_dir_all(&ws).unwrap();
        let mut driver = EnvDriver::new(&run);
        let env = ready_env(&mut store, &lease, &mut driver, &ws);
        open_turn_scopes(&mut store, &run, &lease);
        open_adopted_scopes(&mut store, &run, &lease, "ef-1", "mc.1.tc0");
        // `armed=true` carries a surface whose `result_render` declares
        // no `renderer` — the renderer-less path (legacy, byte-identical).
        let surface = armed_surface(if armed { None } else { Some(renderer()) });
        let mut gate = DispatchGate {
            embed: EmbedGate {
                host_surfaces: BTreeMap::new(),
                host_asks: Vec::new(),
            },
            exec_caps: [(
                "exec.bash".to_string(),
                exec_cap(Box::new(OutputExec::new(&["0123456789ABCDEF"]))),
            )]
            .into_iter()
            .collect(),
            run_id: run.clone(),
            env_handle_id: Some(env),
            seed: [7u8; 32],
            // The *unarmed* leg never populates the map; the
            // renderer-less leg's map is empty either way.
            result_renderers: if armed {
                renderers_of(&surface)
            } else {
                BTreeMap::new()
            },
            submission_marker: None,
        };
        let intent = Json::obj([
            ("surface_id", Json::str("exec.bash")),
            ("tool_call_id", Json::str("mc.1.tc0")),
            ("args", Json::obj([("command", Json::str("x"))])),
        ]);
        let seam = DispatchSeam {
            store: &mut store,
            lease: &lease,
            envs: Some(&mut driver),
        };
        let out = gate
            .dispatch_durable(seam, "ef-1", 1, &intent)
            .expect("armed surface");
        assert!(
            matches!(out.outcome, SettledOutcome::Observed { .. }),
            "{out:?}"
        );
        // No `context.observation.recorded` minted — the legacy row
        // set is byte-identical (a contract-less capability emits none).
        let events = read_all(&store, &run);
        assert!(
            events
                .iter()
                .all(|e| e.class != "context.observation.recorded"),
            "armed={armed}: legacy mint set changed"
        );
        // The transcript renders the honest `no_observation_recorded`
        // record — the pre-change golden.
        let mut port = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
            .unwrap()
            .with_surface(surface.clone());
        let completed = Json::obj([
            ("model_call_id", Json::str("mc.1")),
            ("attempt_no", Json::Int(1)),
            ("stop_reason", Json::str("tool_calls")),
            ("response_ref", Json::str("sha256:00")),
            (
                "calls",
                Json::Arr(vec![Json::obj([
                    ("tool_call_id", Json::str("mc.1.tc0")),
                    ("surface", Json::str("exec.bash")),
                    ("args_raw", Json::str("{}")),
                ])]),
            ),
        ]);
        // Mint the completed row so the fold pairs the call.
        let m = EventMinter::new(&mut store, &run);
        let mut e = m.mint("model.call.completed", completed).unwrap();
        e.scope = Scope {
            turn_id: Some("turn-1".into()),
            model_call_id: Some("mc.1".into()),
            ..Scope::default()
        };
        store.append(&run, &lease, vec![e]).unwrap();
        let prefix = read_all(&store, &run);
        let ms = port.requested_members("mc.2", &Json::Null, &prefix, &|_: &str| None);
        assert!(ms.iter().all(|(k, _)| *k != "plan_error"), "{ms:?}");
        let tools = tool_messages(port.pending_body("mc.2").unwrap());
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].1, "{\"kind\":\"no_observation_recorded\"}",
            "armed={armed}: the pre-change tool message"
        );
    }
    // The class table registers no new class — `context.observation.
    // recorded` is the reused class (CC10).
    assert!(hharness_ledger::classes::lookup("context.observation.recorded").is_some());
    assert_eq!(
        hharness_ledger::classes::registered_classes()
            .filter(|s| s.class == "context.observation.recorded")
            .count(),
        1
    );
}
