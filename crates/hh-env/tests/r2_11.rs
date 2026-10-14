//! R-2.11 acceptance — the C2 `modify` arm's durable trail, the
//! `action.intent.anchored` subject `label.endorsed{basis: approval}`
//! pins, monitor check 8 (`check_write`) at `prepare` over the C3
//! ownership machinery (incl. the depth-3 chain cell and the `Fenced`
//! verdict), and the `security.permission.pending` offered-set/remedy
//! record the R-2.12 consume path reads (ADR-0343 D1–D4; §5e.5;
//! DF-S1.11-2, DF-S1.23-1).
//!
//! Each test fails if the behaviour it covers is removed. The scaffold
//! reuses the `acceptance.rs` shape: a real `Store`, a real `Monitor`, a
//! real `EnvDriver` over `Ep2Model::reference()`, a scripted executor.

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
use hh_monitor::ownership::{check_write, OwnedObject, OwnershipTable, WriteVerdict};
use hh_monitor::policy::{default_table, Mode};
use hh_monitor::table::HandleTable;
use hh_provenance::{AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::DetectorSet;
use hh_wire::json::Json;

// ── scaffold (the `acceptance.rs` shape) ─────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r211-{}-{tag}-{n}", std::process::id()));
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
        image: ImageRef::ContentAddress(address(b"r2.11-image", "application/octet-stream")),
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

/// `write_file{path, content}` with a caller-chosen domain/resources — the
/// `Resources::Declared` keys arm the prepare check's `resource(key)` arm.
fn capability(
    domain: EffectDomain,
    a: EffectAttributes,
    sb: ScopeBindings,
    resources: Resources,
) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("test tool", "test:owner", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::Obj(
                [("path", "string"), ("content", "string")]
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
        resources,
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
        arg_map: ["path", "content"]
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
    }
}

struct MockExecutor {
    decl: ExecutorDeclaration,
    calls: std::cell::Cell<u64>,
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
                    EffectDomain::PermissionRequest,
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

/// A `permission_request`-domain dispatch — the attended floor's `ask`
/// trigger (the ask trail's canonical fixture).
fn ask_input<'a>(
    cap: &'a ToolCapabilityRecord,
    bind: &'a SurfaceBinding,
    sb: &'a ScopeBindings,
    env: &str,
    ws: &std::path::Path,
) -> DispatchInput<'a> {
    input(
        cap,
        bind,
        sb,
        env,
        "tc-1",
        Json::obj([
            ("path", Json::str(ws.join("perm-req.txt").to_str().unwrap())),
            ("content", Json::str("hi")),
        ]),
        EffectClass {
            domain: EffectDomain::PermissionRequest,
            attributes: Some(EffectAttributes {
                mutability: Mutability::Additive,
                repeat_safety: RepeatSafety::Idempotent,
                world: World::Closed,
                reversibility: Reversibility::Irreversible,
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
        Json::obj([("path", Json::str(path)), ("content", Json::str("hi"))]),
        EffectClass {
            domain: EffectDomain::FsWrite,
            attributes: Some(reversible_attrs()),
        },
    )
}

// ── D2 — `action.intent.anchored` lands durable *beside* `pending` ──────────
//
// The ask trail: `decided{ask}` → `anchor` → `pending` → `requested` →
// `wakeup.scheduled` → `suspended`. The anchor carries the intent's own
// provenance (the model-origin proposal label — never the kernel stamp)
// and `content_kind = effect_intent`, so the produce-time
// `label.endorsed{basis: approval}` can pin it as its subject.
#[test]
fn r211_ask_mints_intent_anchor_beside_pending() {
    let (mut store, run, lease, _clock) = open("anchor");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("anchor");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(
        EffectDomain::PermissionRequest,
        EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Closed,
            reversibility: Reversibility::Irreversible,
        },
        scope_bindings(),
        Resources::NoneDeclared,
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = ask_input(&cap, &bind, &sb, &env, &ws);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    let DispatchOutcome::Suspended { permission_id } = out else {
        panic!("an ask suspends awaiting_approval, got {out:?}")
    };

    let envs = read_all(disp.store_mut(), &run);
    let anchor = envs
        .iter()
        .find(|e| e.class == "action.intent.anchored")
        .expect("action.intent.anchored is durable beside pending");
    let pending = envs
        .iter()
        .find(|e| e.class == "security.permission.pending")
        .expect("durable pending");
    assert!(
        anchor.seq < pending.seq,
        "the anchor lands before the pending (append-time check-5's subject)"
    );
    // The anchor's members name the intent the endorsement pins.
    assert_eq!(
        jstr(&anchor.payload, "permission_id"),
        Some(permission_id.as_str())
    );
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
    assert_eq!(jstr(&anchor.payload, "effect_id"), Some(effect_id.as_str()));
    assert_eq!(jstr(&anchor.payload, "subject_ref"), Some("test:agent"));
    // `args_canonical_hash` and `capability_ref` echo the pending's request.
    let req = pending.payload.get("request").expect("pending.request");
    assert_eq!(
        jstr(&anchor.payload, "args_canonical_hash"),
        jstr(req, "args_canonical_hash"),
        "the anchor's hash is the request's canonical-args hash"
    );
    assert_eq!(
        anchor
            .payload
            .get("capability_ref")
            .and_then(|c| jstr(c, "semantic_id")),
        Some("test:write_file"),
    );
    // The anchor's provenance is the *intent's* record — model-origin,
    // delegate authority — never the kernel stamp.
    let prov = anchor.provenance.as_ref().expect("anchor provenance");
    assert!(
        matches!(prov.origin, Origin::Model { .. }),
        "the anchor carries the proposer's intent record: {:?}",
        prov.origin
    );
    assert!(
        prov.authority < AuthorityClass::Principal,
        "the intent's label sits below the approval's rise ({:?})",
        prov.authority
    );
    // The pending's offered set is durable — `modify` among them (the
    // C2 arm's offer is a ledger fact, never the surface's say-so).
    let options = req
        .get("options")
        .and_then(|o| match o {
            Json::Arr(rows) => Some(rows),
            _ => None,
        })
        .expect("durable request.options");
    assert!(
        options.iter().any(|o| o.as_str() == Some("modify")),
        "the offered set records `modify`: {options:?}"
    );
    assert!(
        options.iter().any(|o| o.as_str() == Some("allow_once")),
        "the offered set records `allow_once`: {options:?}"
    );
}

// ── D1 — a recorded `decided{modified}` refuses the as-proposed effect ──────
//
// The C2 `modify` arm end to end at the dispatch stage: the ask suspends,
// the surface answers `modified` (the durable `decided` carries
// `amended_args`), and a re-dispatch serves the recorded terminal —
// `action.effect.refused{reason: modified}` + `action.tool.rejected`, the
// executor never runs.
#[test]
fn r211_recorded_modified_refuses_as_proposed_effect() {
    let (mut store, run, lease, _clock) = open("modified");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("modified");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(
        EffectDomain::PermissionRequest,
        EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Closed,
            reversibility: Reversibility::Irreversible,
        },
        scope_bindings(),
        Resources::NoneDeclared,
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    let mut exec = MockExecutor::ok();
    let inp = ask_input(&cap, &bind, &sb, &env, &ws);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    let DispatchOutcome::Suspended { permission_id } = out else {
        panic!("expected Suspended, got {out:?}")
    };
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);

    // The surface answers `modified` — the durable `decided` carries the
    // amended args (the re-entry material, never an in-place allow).
    let amended = Json::obj([("path", Json::str(ws.join("narrowed.txt").to_str().unwrap()))]);
    let m = EventMinter::new(disp.store_mut(), &run);
    let mut dec = m
        .mint(
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str(permission_id.clone())),
                ("effect_id", Json::str(effect_id.clone())),
                ("decision", Json::str("modified")),
                ("amended_args", amended.clone()),
                ("decider", Json::str("human")),
                ("decision_scope", Json::str("once")),
                ("attempt_no", Json::Int(1)),
            ]),
        )
        .unwrap();
    dec.scope.effect_id = Some(effect_id.clone());
    disp.store_mut().append(&run, &lease, vec![dec]).unwrap();

    // The fold restores `Decision::Modified{amended_args}` from the row —
    // remove the fold arm and this assertion fails.
    let events = disp.store_mut().events(&run).unwrap().to_vec();
    let head = events.last().map(|e| e.seq).unwrap_or(0);
    let st = hh_monitor::approval::ApprovalState::project(&events, head);
    let rec = st
        .decisions
        .get(&permission_id)
        .expect("the recorded modified decision folds");
    match &rec.decision {
        hh_monitor::decision::Decision::Modified { amended_args: a } => {
            assert_eq!(a, &amended, "the amendment rides the durable row verbatim");
        }
        other => panic!("expected Decision::Modified, folded {other:?}"),
    }

    // Re-dispatch — the recorded terminal serves: refused, never executed.
    let mut mon2 = test_monitor(&cap);
    mon2.approvals = st;
    let mut disp2 = Dispatcher::new(&mut store, &mon2, &run, [8u8; 32], DetectorSet::default());
    let out2 = disp2
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(&out2, DispatchOutcome::Refused { reason } if reason == "modified"),
        "the recorded modified refuses the as-proposed effect: {out2:?}"
    );
    assert_eq!(exec.calls.get(), 0, "a modified effect never executes");
    let envs = read_all(disp2.store_mut(), &run);
    assert!(
        envs.iter().any(|e| e.class == "action.effect.refused"
            && jstr(&e.payload, "reason") == Some("modified")),
        "action.effect.refused{{reason: modified}} is durable"
    );
    assert!(
        envs.iter().any(|e| e.class == "action.tool.rejected"),
        "the tool_call closes rejected"
    );
    assert!(
        !envs.iter().any(|e| e.class == "action.effect.committed"),
        "a modified effect never commits"
    );
}

// ── D3 — check 8 at `prepare`: `NotOwner` fails closed ──────────────────────
//
// `set_ownership_roots` arms the projection; a rival holder's record over
// the workspace refuses the `fs_write` with the
// `security.policy.evaluated{check: ownership, verdict: deny}` audit row.
#[test]
fn r211_prepare_check_write_not_owner_refuses() {
    let (mut store, run, lease, _clock) = open("notowner");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("notowner");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(
        EffectDomain::FsWrite,
        reversible_attrs(),
        scope_bindings(),
        Resources::NoneDeclared,
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    // The rival holder owns the workspace tree outright.
    disp.set_ownership_roots(vec![
        (run.clone(), OwnedObject::FsPathPrefix("/never".to_string())),
        (
            "run-rival".to_string(),
            OwnedObject::FsPathPrefix(ws.display().to_string()),
        ),
    ]);
    let mut exec = MockExecutor::ok();
    let path = ws.join("victim.txt").display().to_string();
    let inp = write_input(&cap, &bind, &sb, &env, "tc-1", &path);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(&out, DispatchOutcome::Refused { reason } if reason.starts_with("NotOwner")),
        "a rival-held write refuses at prepare: {out:?}"
    );
    assert_eq!(exec.calls.get(), 0, "a NotOwner write never executes");
    let envs = read_all(disp.store_mut(), &run);
    let evaluated = envs
        .iter()
        .find(|e| {
            e.class == "security.policy.evaluated" && jstr(&e.payload, "check") == Some("ownership")
        })
        .expect("the ownership deny audit row");
    assert_eq!(jstr(&evaluated.payload, "verdict"), Some("deny"));
    let detail = evaluated.payload.get("detail").expect("evaluated.detail");
    assert_eq!(jstr(detail, "reason"), Some("not_owner"));
    assert_eq!(jstr(detail, "owner"), Some("run-rival"));
    assert!(
        envs.iter().any(|e| e.class == "action.effect.refused"),
        "the effect closes refused"
    );
    assert!(
        envs.iter().any(|e| e.class == "action.tool.rejected"),
        "the tool_call closes rejected"
    );
    assert!(
        !envs.iter().any(|e| e.class == "action.tool.started"),
        "the tool never starts"
    );
}

// The same armed check passes a write under the run's own record —
// removing the check's allow leg (or the root) turns this refusal-side
// green test red.
#[test]
fn r211_prepare_check_write_own_root_allows() {
    let (mut store, run, lease, _clock) = open("ownroot");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("ownroot");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let cap = capability(
        EffectDomain::FsWrite,
        reversible_attrs(),
        scope_bindings(),
        Resources::NoneDeclared,
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_ownership_roots(vec![
        (
            run.clone(),
            OwnedObject::FsPathPrefix(ws.display().to_string()),
        ),
        (
            "run-rival".to_string(),
            OwnedObject::FsPathPrefix("/elsewhere".to_string()),
        ),
    ]);
    let mut exec = MockExecutor::ok();
    let path = ws.join("mine.txt").display().to_string();
    let inp = write_input(&cap, &bind, &sb, &env, "tc-1", &path);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(out, DispatchOutcome::Observed(_)),
        "the run's own write passes check 8: {out:?}"
    );
    assert_eq!(exec.calls.get(), 1);
    // Unarmed → the ownership regime is not in play: no `ownership` verdict
    // rows and no refusals even though a rival record exists nowhere.
    let envs = read_all(disp.store_mut(), &run);
    assert!(
        !envs.iter().any(|e| {
            e.class == "security.policy.evaluated" && jstr(&e.payload, "check") == Some("ownership")
        }),
        "an allowed write mints no deny row"
    );
}

// ── D3 — the depth-3 ownership/delegation chain cell (DF-S1.11-2) ───────────
//
// Parent owns `ws` (the declared root); `control.ownership.transferred`
// moves `ws/sub` to the child and `ws/sub/deep` to the grandchild; a
// `control.ownership.granted` conferral covers `ws/granted`. The fold's
// nearest-covering-record rule resolves all three levels, and the
// dispatch's `prepare` gate refuses each write with the *deepest* live
// holder — never the root, never silently.
#[test]
fn r211_depth3_ownership_chain_gates_at_prepare() {
    let (mut store, run, lease, _clock) = open("depth3");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("depth3");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);

    // The three-level chain as durable ownership rows on the parent run:
    //   run owns ws (root) → ws/sub to run-child → ws/sub/deep to
    //   run-grandchild; ws/granted conferred to run-grantee.
    let sub = ws.join("sub").display().to_string();
    let deep = ws.join("sub/deep").display().to_string();
    let granted = ws.join("granted").display().to_string();
    let m = EventMinter::new(&store, &run);
    let t1 = m
        .mint(
            "control.ownership.transferred",
            Json::obj([
                (
                    "object",
                    Json::obj([
                        ("kind", Json::str("fs_path_prefix")),
                        ("key", Json::str(sub.clone())),
                    ]),
                ),
                ("from", Json::str(run.clone())),
                ("to", Json::str("run-child")),
                ("basis", Json::str(format!("{run}:evt-basis-1"))),
            ]),
        )
        .unwrap();
    let t2 = m
        .mint(
            "control.ownership.transferred",
            Json::obj([
                (
                    "object",
                    Json::obj([
                        ("kind", Json::str("fs_path_prefix")),
                        ("key", Json::str(deep.clone())),
                    ]),
                ),
                ("from", Json::str("run-child")),
                ("to", Json::str("run-grandchild")),
                ("basis", Json::str(format!("{run}:evt-basis-2"))),
            ]),
        )
        .unwrap();
    let t3 = m
        .mint(
            "control.ownership.granted",
            Json::obj([
                (
                    "objects",
                    Json::Arr(vec![Json::obj([
                        ("kind", Json::str("fs_path_prefix")),
                        ("key", Json::str(granted.clone())),
                    ])]),
                ),
                ("from", Json::str(run.clone())),
                ("to", Json::str("run-grantee")),
                ("basis", Json::str(format!("{run}:evt-basis-3"))),
            ]),
        )
        .unwrap();
    store.append(&run, &lease, vec![t1, t2, t3]).unwrap();

    // The fold over roots + rows resolves the deepest covering record at
    // every level — the pure half of the cell.
    let ownerships = OwnershipTable::project(
        &store,
        &store.run_ids(),
        vec![(
            run.clone(),
            OwnedObject::FsPathPrefix(ws.display().to_string()),
        )],
    )
    .unwrap();
    assert_eq!(
        ownerships.owner_of(&OwnedObject::FsPathPrefix(format!("{deep}/x.txt"))),
        Some("run-grandchild"),
        "depth 3 — the grandchild's record is nearest"
    );
    assert_eq!(
        ownerships.owner_of(&OwnedObject::FsPathPrefix(format!("{sub}/y.txt"))),
        Some("run-child"),
        "depth 2 — the child's record covers below the deep subtree"
    );
    assert_eq!(
        ownerships.owner_of(&OwnedObject::FsPathPrefix(format!(
            "{}/z.txt",
            ws.display()
        ))),
        Some(run.as_str()),
        "depth 1 — the root covers the rest of the tree"
    );
    assert_eq!(
        ownerships.owner_of(&OwnedObject::FsPathPrefix(format!("{granted}/g.txt"))),
        Some("run-grantee"),
        "the `granted` conferral folds too"
    );

    // The armed dispatch refuses each level with its live holder.
    let cap = capability(
        EffectDomain::FsWrite,
        reversible_attrs(),
        scope_bindings(),
        Resources::NoneDeclared,
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_ownership_roots(vec![(
        run.clone(),
        OwnedObject::FsPathPrefix(ws.display().to_string()),
    )]);
    let mut exec = MockExecutor::ok();

    let cases = [
        (format!("{deep}/x.txt"), "run-grandchild"),
        (format!("{sub}/y.txt"), "run-child"),
        (format!("{granted}/g.txt"), "run-grantee"),
    ];
    for (i, (path, owner)) in cases.iter().enumerate() {
        let inp = write_input(&cap, &bind, &sb, &env, &format!("tc-d{}", i + 1), path);
        let out = disp
            .dispatch(&mut driver, &mut exec, None, &inp, &lease)
            .unwrap();
        assert!(
            matches!(&out, DispatchOutcome::Refused { reason } if reason == &format!("NotOwner{{owner: {owner}}}")),
            "depth-{i} write refuses with the deepest holder {owner}: {out:?}"
        );
    }
    // The run's own uncovered write still passes.
    let inp = write_input(
        &cap,
        &bind,
        &sb,
        &env,
        "tc-d9",
        &format!("{}/z.txt", ws.display()),
    );
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(out, DispatchOutcome::Observed(_)),
        "the root's own write passes: {out:?}"
    );
    assert_eq!(exec.calls.get(), 1, "only the owned write executed");

    // Every refusal minted the typed audit row — three `deny` verdicts,
    // one per gated level.
    let envs = read_all(disp.store_mut(), &run);
    let denies: Vec<_> = envs
        .iter()
        .filter(|e| {
            e.class == "security.policy.evaluated"
                && jstr(&e.payload, "check") == Some("ownership")
                && jstr(&e.payload, "verdict") == Some("deny")
        })
        .collect();
    assert_eq!(denies.len(), 3, "three ownership denies — one per level");
}

// ── D3 — `Fenced`: a resource lease minted under a superseded generation ────
//
// The `resource(key)` arm end to end: the run holds `shared-counter`
// under writer generation g; the writer generation moves to g+1; the
// dispatch's declared-key check reads the stale record and refuses
// `Fenced` — the one generation fence, at prepare.
#[test]
fn r211_prepare_check_write_fenced_refuses() {
    let (mut store, run, lease, clock) = open("fenced");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let ws = workspace("fenced");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);

    // The run holds the resource key under writer generation 1.
    let scope = hh_ledger::leases::LeaseScope::Resource("shared-counter".to_string());
    store
        .lease_acquire(&run, &lease, &scope, &format!("subagent:{run}"), 300_000)
        .unwrap();

    // The writer generation moves — the held record is now stale.
    clock.advance(61_000); // past the 60 s writer ttl
    let lease2 = store.acquire_writer("writer-a", &run, 60_000).unwrap();
    assert!(lease2.generation > lease.generation, "the re-acquire bumps");

    let cap = capability(
        EffectDomain::FsWrite,
        reversible_attrs(),
        scope_bindings(),
        Resources::Declared(BTreeSet::from(["shared-counter".to_string()])),
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    // Armed — the run owns the workspace so only the resource key gates.
    disp.set_ownership_roots(vec![(
        run.clone(),
        OwnedObject::FsPathPrefix(ws.display().to_string()),
    )]);
    let mut exec = MockExecutor::ok();
    let path = ws.join("with-key.txt").display().to_string();
    let inp = write_input(&cap, &bind, &sb, &env, "tc-1", &path);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease2)
        .unwrap();
    assert!(
        matches!(&out, DispatchOutcome::Refused { reason } if reason.starts_with("Fenced")),
        "a superseded-generation write fences at prepare: {out:?}"
    );
    assert_eq!(exec.calls.get(), 0, "a fenced write never executes");
    let envs = read_all(disp.store_mut(), &run);
    let evaluated = envs
        .iter()
        .find(|e| {
            e.class == "security.policy.evaluated" && jstr(&e.payload, "check") == Some("ownership")
        })
        .expect("the ownership deny audit row");
    let detail = evaluated.payload.get("detail").expect("evaluated.detail");
    assert_eq!(jstr(detail, "reason"), Some("fenced"));
}

// ── D3 — `resource(key)` held by another run refuses `NotOwner{holder}` ─────
//
// The cross-run contention arm at prepare: a sibling run's live
// `resource:shared-counter` lease is a ledger fact — the armed dispatch
// loses to it.
#[test]
fn r211_prepare_check_write_resource_key_rival_refuses() {
    let (mut store, run, lease, _clock) = open("keyrival");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let (rival_run, rival_lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-b")
        .unwrap();
    let ws = workspace("keyrival");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);

    let scope = hh_ledger::leases::LeaseScope::Resource("shared-counter".to_string());
    store
        .lease_acquire(&rival_run, &rival_lease, &scope, "subagent:rival", 300_000)
        .unwrap();

    let cap = capability(
        EffectDomain::FsWrite,
        reversible_attrs(),
        scope_bindings(),
        Resources::Declared(BTreeSet::from(["shared-counter".to_string()])),
    );
    let bind = binding();
    let sb = scope_bindings();
    let mon = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &mon, &run, [7u8; 32], DetectorSet::default());
    disp.set_ownership_roots(vec![(
        run.clone(),
        OwnedObject::FsPathPrefix(ws.display().to_string()),
    )]);
    let mut exec = MockExecutor::ok();
    let path = ws.join("with-key.txt").display().to_string();
    let inp = write_input(&cap, &bind, &sb, &env, "tc-1", &path);
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    assert!(
        matches!(&out, DispatchOutcome::Refused { reason } if reason == "NotOwner{owner: subagent:rival}"),
        "a rival-held key refuses at prepare: {out:?}"
    );
    assert_eq!(exec.calls.get(), 0);
    let envs = read_all(disp.store_mut(), &run);
    let evaluated = envs
        .iter()
        .find(|e| {
            e.class == "security.policy.evaluated" && jstr(&e.payload, "check") == Some("ownership")
        })
        .expect("the ownership deny audit row");
    let detail = evaluated.payload.get("detail").expect("evaluated.detail");
    assert_eq!(jstr(detail, "reason"), Some("not_owner"));
    assert_eq!(jstr(detail, "owner"), Some("subagent:rival"));
    // The object member names the contended resource key.
    let object = evaluated.payload.get("object").expect("evaluated.object");
    assert_eq!(jstr(object, "kind"), Some("resource_key"));
    assert_eq!(jstr(object, "key"), Some("shared-counter"));
}

// ── D3 — the pure check_write still composes over the same rows ────────────
//
// The dispatch's prepare gate and `hh-subagent`'s merge path fold the
// same `OwnershipTable` — the pure check over the depth-3 fold agrees
// with the durable audit (one machine, CC1).
#[test]
fn r211_check_write_pure_agrees_with_prepare_gate() {
    let (mut store, run, lease, _clock) = open("pure");
    let ws = workspace("pure");
    let roots = vec![(
        run.clone(),
        OwnedObject::FsPathPrefix(ws.display().to_string()),
    )];
    let sub = ws.join("sub").display().to_string();
    let m = EventMinter::new(&store, &run);
    let t = m
        .mint(
            "control.ownership.transferred",
            Json::obj([
                (
                    "object",
                    Json::obj([
                        ("kind", Json::str("fs_path_prefix")),
                        ("key", Json::str(sub.clone())),
                    ]),
                ),
                ("from", Json::str(run.clone())),
                ("to", Json::str("run-child")),
                ("basis", Json::str(format!("{run}:evt-basis"))),
            ]),
        )
        .unwrap();
    store.append(&run, &lease, vec![t]).unwrap();
    let ownerships = OwnershipTable::project(&store, &store.run_ids(), roots).unwrap();
    let verdict = check_write(
        &store,
        &run,
        lease.generation,
        &OwnedObject::FsPathPrefix(format!("{sub}/f.txt")),
        &ownerships,
    )
    .unwrap();
    assert!(
        matches!(verdict, WriteVerdict::NotOwner { ref owner } if owner == "run-child"),
        "pure check_write agrees: {verdict:?}"
    );
    let verdict = check_write(
        &store,
        &run,
        lease.generation,
        &OwnedObject::FsPathPrefix(format!("{}/mine.txt", ws.display())),
        &ownerships,
    )
    .unwrap();
    assert!(matches!(verdict, WriteVerdict::Ok));
}
