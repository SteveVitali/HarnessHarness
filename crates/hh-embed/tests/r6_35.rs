//! R6.35 acceptance battery, Stage 2 (manifest 293; REQ-XE-103/104/113;
//! AC-7.3-2/10/11) — the single compiled-`ModelSurface` renderer:
//! AC-3 two-path goldens (armed bytes equal the compiled spec; unarmed
//! bytes equal the pre-change path — `provenance: hand_derived` until an
//! O-recorded corpus exists), AC-4 the mini-shaped turn-1 fixture (two
//! definition-authority leaves → one `system` + one `user` message),
//! AC-5 the single-path pin (an armed session's messages are built only
//! through the compiled-surface path), and AC-2's probe legs (rows
//! durable before the first request; probe values == the rendered
//! substitution; an unfilled slot is a typed refusal before any wire
//! send). USD $0, fixtures and a dead loopback endpoint only.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_compiler::lower::CompiledSection;
use hharness_compiler::seal::ModelSurface;
use hharness_control::driver::ModelPort;
use hharness_embed::gateway_model::{decode_model_lane, GatewayModel};
use hharness_embed::runtime::{probe_uname_slots, ExecCap, UNAME_PROBE};
use hharness_env::capture::{CaptureKind, OutputPolicy};
use hharness_env::deadline::DeadlineLadder;
use hharness_env::errors::EnvError;
use hharness_env::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::{Resources, ScopeBindings, ToolCapabilityRecord};
use hharness_ledger::classes::Durability;
use hharness_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{ParticipantClass, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

fn ev(seq: u64, class: &str, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("e{seq}"),
        run_id: "run".into(),
        seq,
        ts: "2026-11-10T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Control),
        class: class.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: Default::default(),
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: hharness_ledger::ids::ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: Default::default(),
        provenance: None,
        payload,
        prev_hash: "h".into(),
        hash: format!("h{seq}"),
    }
}

/// The pinned `mini.yaml` templates (sha256 `b539a896…` — one source:
/// the compiler's own fixture files).
const SYS_TMPL: &str = include_str!("../../hh-compiler/tests/fixtures/r6_35/system_template.tmpl");
const INST_TMPL: &str =
    include_str!("../../hh-compiler/tests/fixtures/r6_35/instance_template.tmpl");

fn section(id: &str, node: &str, role: &str, slots: &[&str], text: &str) -> CompiledSection {
    CompiledSection {
        section_id: id.into(),
        source_node_ids: vec![node.into()],
        slots: slots.iter().map(|s| s.to_string()).collect(),
        text: Some(text.into()),
        rule_ids: vec!["r.layout".into()],
        role: Some(role.into()),
        grammar: Some("hh-tmpl/1".into()),
    }
}

/// The mini-shaped surface: two `definition`-authority template leaves
/// in different sections (`system` + `task`) — AC-4's fixture shape.
fn mini_surface() -> ModelSurface {
    ModelSurface {
        profile: "sha256:p@1".into(),
        layout: vec![
            section("system", "def:sys", "system", &[], SYS_TMPL),
            section(
                "task",
                "def:inst",
                "user",
                &["task", "system", "release", "version", "machine"],
                INST_TMPL,
            ),
        ],
        tools: vec![],
        interaction_mode: "native_fc".into(),
        params: Json::obj([]),
        transcript_renderer: Json::obj([]),
        dialects: BTreeMap::new(),
    }
}

fn lane() -> hharness_embed::gateway_model::ModelLane {
    let mut m = BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str("http://127.0.0.1:1"));
    m.insert("provider_model_id".to_string(), Json::str("fixture-model"));
    decode_model_lane(&Json::Obj(m), "profile:test").unwrap()
}

fn probe_row(seq: u64) -> EventEnvelope {
    ev(
        seq,
        "action.environment.probe.completed",
        Json::obj([
            ("probe", Json::str(UNAME_PROBE)),
            ("outcome", Json::str("applied")),
            (
                "values",
                Json::obj([
                    ("uname.system", Json::str("TestOS")),
                    ("uname.release", Json::str("9.9")),
                    ("uname.version", Json::str("#1")),
                    ("uname.machine", Json::str("test64")),
                ]),
            ),
        ]),
    )
}

fn body_of(port: &mut GatewayModel, prefix: &[EventEnvelope]) -> Json {
    let ms = port.requested_members("mc.1", &Json::Null, prefix, &|_: &str| None);
    port.pending_body("mc.1")
        .filter(|b| !matches!(b, Json::Null))
        .unwrap_or_else(|| panic!("plan refused: {ms:?}"))
        .clone()
}

fn arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(v) => v,
        _ => panic!("not an array: {j:?}"),
    }
}

// ── AC-3 + AC-4 + AC-5 — the two-path golden, the turn-1 fixture, the pin ────

#[test]
fn ac3_two_path_and_ac4_turn_1_golden() {
    let prefix = vec![probe_row(1)];
    // Unarmed — the pre-change render, byte-identical: exactly one
    // `user` message carrying the instruction (the golden below is
    // `provenance: hand_derived`).
    let mut unarmed = GatewayModel::new(lane(), "do the thing".into(), None, vec![]).unwrap();
    assert!(!unarmed.surface_armed());
    let unarmed_body = body_of(&mut unarmed, &prefix);
    let msgs = arr(unarmed_body.get("messages").unwrap());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].get("role").and_then(Json::as_str), Some("user"));
    assert_eq!(
        msgs[0].get("content").and_then(Json::as_str),
        Some("do the thing")
    );
    // Armed — the compiled spec: one `system` + one `user` message from
    // the two sections, template fills applied (AC-4's turn-1 shape).
    let mut armed = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(mini_surface());
    assert!(armed.surface_armed());
    let armed_body = body_of(&mut armed, &prefix);
    let msgs = arr(armed_body.get("messages").unwrap());
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].get("role").and_then(Json::as_str), Some("system"));
    assert_eq!(
        msgs[0].get("content").and_then(Json::as_str),
        Some("You are a helpful assistant that can interact with a computer.")
    );
    let user = msgs[1].get("content").and_then(Json::as_str).unwrap();
    assert_eq!(msgs[1].get("role").and_then(Json::as_str), Some("user"));
    assert!(
        user.contains("Please solve this issue: do the thing"),
        "{user}"
    );
    // The probe's ledgered uname values are the rendered substitution.
    assert!(user.contains("TestOS 9.9 #1 test64"), "{user}");
    assert!(!user.contains("{{task}}"), "{user}");
    // The compiled spec's bytes vs. the legacy path's bytes differ —
    // the golden pair is the two bodies, pinned above.
    assert_ne!(unarmed_body, armed_body);
}

#[test]
fn ac5_armed_render_is_only_the_compiled_path() {
    // The pin: the armed port's preamble is the surface's sections —
    // the legacy `user(instruction)` builder contributes nothing. An
    // instruction string the template never names appears nowhere.
    let mut s = mini_surface();
    s.layout[1].text = Some("STATIC SECTION {{system}}".into());
    let prefix = vec![probe_row(1)];
    let mut armed = GatewayModel::new(lane(), "INSTRUCTION-NOT-RENDERED".into(), None, vec![])
        .unwrap()
        .with_surface(s);
    let body = body_of(&mut armed, &prefix);
    let msgs = arr(body.get("messages").unwrap());
    assert_eq!(msgs.len(), 2);
    let user = msgs[1].get("content").and_then(Json::as_str).unwrap();
    assert_eq!(user, "STATIC SECTION TestOS");
    assert!(!user.contains("INSTRUCTION-NOT-RENDERED"));
    // And `surface_armed` pins the route — no fall-through exists.
    assert!(armed.surface_armed());
}

// ── AC-2 (REQ-XE-113) — the ledgered probe fills; the unfilled leg refuses ────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r635-{}-{tag}-{n}", std::process::id()));
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

/// An in-cell `uname` stub — the executor's stdout is the in-cell answer
/// the probe parses (fixture evidence, never a live-service claim).
struct UnameExec {
    decl: ExecutorDeclaration,
}

impl UnameExec {
    fn new() -> Self {
        UnameExec {
            decl: ExecutorDeclaration {
                executor_id: "uname-stub/1".to_string(),
                isolation_support: hharness_containment::policy::IsolationClass::None,
                dedup_support: DedupSupport::BestEffort,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: ["executor_error".to_string()].into_iter().collect(),
                streams: false,
                domains: [EffectDomain::Exec].into_iter().collect(),
            },
        }
    }
}

impl ToolExecutor for UnameExec {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        assert_eq!(
            request.args.get("command").and_then(Json::as_str),
            Some("uname -s; uname -r; uname -v; uname -m")
        );
        sink(ExecutorSignal {
            token: request.attribution_token.clone(),
            kind: CaptureKind::OutputChunk {
                stream: "stdout".to_string(),
                data: "TestOS\n9.9\n#1\ntest64\n".to_string(),
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

fn exec_cap(exec: Box<dyn ToolExecutor>) -> ExecCap {
    ExecCap {
        exec_defaults: None,
        surface_id: "exec.probe".to_string(),
        binding: hharness_compiler::equiv::SurfaceBinding {
            surface_name: "exec.probe".to_string(),
            capability_ref: hharness_compiler::plan::PinnedRef {
                semantic_id: "test:exec".to_string(),
                version_id: "v-cap".to_string(),
            },
            hir_node_id: "test:exec".to_string(),
            arg_map: Default::default(),
            dialect: "json-schema-2020-12".to_string(),
            surface_id: "exec.probe".to_string(),
            exposure_mode: hharness_compiler::surface::CompileExposureMode::Primitive,
            capability_refs: vec!["test:exec".to_string()],
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
        },
        scope_bindings: ScopeBindings::Unknown,
        capability: ToolCapabilityRecord {
            exec_defaults: None,
            purpose: Text::new("t", "test:owner", ProvenanceRecord::kernel("t", 0)),
            input_schema: Json::obj([]),
            output_schema: None,
            effects: ToolEffects::Pure,
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
        },
        capability_ref: hharness_compiler::plan::PinnedRef {
            semantic_id: "test:exec".to_string(),
            version_id: "v-cap".to_string(),
        },
        declared: EffectClass {
            domain: EffectDomain::Exec,
            attributes: Some(EffectAttributes {
                mutability: Mutability::ReadOnly,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Compensable,
            }),
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
        monitor: hharness_monitor::monitor::Monitor::new(
            hharness_monitor::table::HandleTable::default(),
            hharness_monitor::policy::default_table(
                "pol",
                hharness_monitor::policy::Mode::Attended,
            ),
        ),
    }
}

#[test]
fn ac2_probe_rows_ledger_before_the_request_and_fill_it() {
    let (mut store, run, lease) = open("probe");
    let surface = mini_surface();
    let mut caps = vec![exec_cap(Box::new(UnameExec::new()))];
    probe_uname_slots(
        &mut store,
        &run,
        &lease,
        Some("env-h1"),
        &mut caps,
        &surface,
    )
    .unwrap();
    let events = store.events(&run).unwrap();
    // The durable rows: started, then completed{applied, values{uname.*}}.
    let started = events
        .iter()
        .find(|e| e.class == "action.environment.probe.started")
        .expect("probe.started");
    assert_eq!(
        started.payload.get("probe").and_then(Json::as_str),
        Some(UNAME_PROBE)
    );
    assert_eq!(started.payload.get("read_only"), Some(&Json::Bool(true)));
    let completed = events
        .iter()
        .find(|e| e.class == "action.environment.probe.completed")
        .expect("probe.completed");
    assert_eq!(
        completed.payload.get("outcome").and_then(Json::as_str),
        Some("applied")
    );
    let vs = completed.payload.get("values").unwrap();
    assert_eq!(
        vs.get("uname.system").and_then(Json::as_str),
        Some("TestOS")
    );
    // The prefix before the first request renders the probe's values —
    // and the request's substitution equals them (the AC-2 leg).
    let prefix: Vec<EventEnvelope> = events.to_vec();
    let mut armed = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(surface);
    let body = body_of(&mut armed, &prefix);
    let msgs = arr(body.get("messages").unwrap());
    let user = msgs[1].get("content").and_then(Json::as_str).unwrap();
    assert!(user.contains("TestOS 9.9 #1 test64"), "{user}");
    // Re-drive is idempotent — one completed row.
    probe_uname_slots(
        &mut store,
        &run,
        &lease,
        Some("env-h1"),
        &mut caps,
        &mini_surface(),
    )
    .unwrap();
    assert_eq!(
        store
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "action.environment.probe.completed")
            .count(),
        1
    );
}

#[test]
fn ac2_unfilled_slot_is_a_typed_refusal_before_any_call() {
    // No probe row in the ledger → the `uname.*` fill is absent → the
    // plan fails `invalid_request` with `template_slot_unfilled` — no
    // body is staged, no wire send is attempted (the endpoint is dead).
    let mut armed = GatewayModel::new(lane(), "do the thing".into(), None, vec![])
        .unwrap()
        .with_surface(mini_surface());
    let ms = armed.requested_members("mc.1", &Json::Null, &[], &|_: &str| None);
    let err = ms
        .iter()
        .find(|(k, _)| *k == "plan_error")
        .expect("a plan_error member");
    assert_eq!(err.1, Json::str("invalid_request"));
    // `call` consumes the staged refusal — the wire is never touched
    // (the endpoint is dead: a reach would hang/fail differently).
    let outcome = armed.call("mc.1", &Json::Null);
    assert_eq!(
        outcome.stop_reason,
        hharness_gateway::vocab::StopReason::Error
    );
    // An unbacked probe (no armed exec surface) records its own honest
    // row — the slot stays unfilled either way.
    let (mut store, run, lease) = open("unbacked");
    probe_uname_slots(&mut store, &run, &lease, None, &mut [], &mini_surface()).unwrap();
    let completed = store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "action.environment.probe.completed")
        .expect("completed row");
    assert_eq!(
        completed.payload.get("outcome").and_then(Json::as_str),
        Some("refused")
    );
    assert_eq!(
        completed.payload.get("reason").and_then(Json::as_str),
        Some("probe_unbacked")
    );
}
