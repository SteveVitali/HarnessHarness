//! S2.1 integration coverage — the *live* `hh-helper` boundary
//! (AC-R-2.2.5-{3,4,7,8,10} · AC-R-2.5.5-{6,7,10} plus the helper-specific
//! behaviors: commit-proof admission, token echo, deadline enforcement,
//! term→kill cancellation, executor-side dedup, `fs_tree`, `derive`,
//! unreachable→heal/replace, detached-child detection, malformed frames,
//! helper crash, and the `local_container` podman lane when the runtime is
//! available).
//!
//! Every test fails if the behavior it pins is removed. The scaffold builds
//! a real `Store` (WAL), a real `Monitor`, a real `EnvDriver`, and — unlike
//! the Stage-1 acceptance suite — a **real spawned `hh-helper` process** per
//! contained environment (`driver.attach` → `start_helper`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use hh_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hh_compiler::plan::PinnedRef;
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::backend::Ep2Model;
use hh_containment::policy::{kernel_default, NetMode, ResourceLimits, WritableRoot};
use hh_env::capture::OutputPolicy;
use hh_env::deadline::DeadlineLadder;
use hh_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hh_env::driver::EnvDriver;
use hh_env::errors::EnvError;
use hh_env::events::{EventMinter, ScopeChain};
use hh_env::handle::{DeriveMode, HandleState, OnLoss, OnParentEnd, Roots};
use hh_env::helper::{helper_binary, HelperClient};
use hh_env::observe::{EffectOutcome, ObservedStatus};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hh_hir::leaves::Text;
use hh_hir::records::{Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_hir::refs::Ref;
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

// ── scaffold (mirrors acceptance.rs) ────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-live-{}-{tag}-{n}", std::process::id()));
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

/// A *canonicalized* workspace — seatbelt resolves symlinks before the
/// subpath match, so `/var/folders/…` must be recorded as its
/// `/private/var/folders/…` truth or the writable-root grant misses.
fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    std::fs::canonicalize(&p).unwrap()
}

fn test_policy(ws: &Path) -> hh_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    let root = ws.display().to_string();
    p.fs.write.allow.push(WritableRoot {
        root: root.clone(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    // Exec is scoped to `/bin` (the test workloads run `sh`/`echo`/`sleep`).
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

fn test_ca() -> hh_identity::idp::ContentAddress {
    address(b"test-image-bytes", "application/octet-stream")
}

/// `provision + attach` for `class` → a `ready` handle whose live helper
/// session sits in the driver's session table (`local_host` excluded — no
/// helper).
fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &Path,
    class: EnvironmentClass,
    on_loss: OnLoss,
    image: ImageRef,
) -> String {
    let record = test_record(class, image);
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
            on_loss,
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

// ── monitor fixture ─────────────────────────────────────────────────────────

fn attrs(reversibility: Reversibility) -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility,
    }
}

fn reversible_attrs() -> EffectAttributes {
    attrs(Reversibility::Reversible(Ref::selected(
        "test:proc",
        "latest",
    )))
}

/// Exec dispatches declare `compensable` + a registered plan — the Π-4
/// `allow` row (a `command` arg is never `inside_writable_roots`, so Π-3
/// would `ask`; Π-4's covering check is `compensation_plan_id.is_some()`).
fn compensable_attrs() -> EffectAttributes {
    attrs(Reversibility::Compensable)
}

fn capability(domain: EffectDomain, a: EffectAttributes) -> ToolCapabilityRecord {
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
                    ("cwd", "string"),
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
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([(
            "error_classes",
            Json::Arr(vec![
                Json::str("executor_error"),
                Json::str("timeout"),
                Json::str("signalled"),
                Json::str("invalid_arguments"),
                Json::str("cancelled"),
                Json::str("containment_denied"),
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
        arg_map: ["path", "content", "command", "argv", "cwd"]
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
                effect: EffectClass::domain_only(EffectDomain::SpawnProcess),
                scope: "*".to_string(),
                constraints: GrantConstraints::default(),
                delegable: true,
            },
            Grant {
                effect: EffectClass::domain_only(EffectDomain::Exec),
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

fn no_scope_bindings() -> ScopeBindings {
    ScopeBindings::Bindings(Json::Arr(vec![]))
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

fn sandboxed_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    tag: &str,
) -> (String, PathBuf) {
    let ws = workspace(tag);
    let id = ready_env(
        store,
        lease,
        driver,
        &ws,
        EnvironmentClass::LocalSandboxed,
        OnLoss::FailRun,
        ImageRef::ContentAddress(test_ca()),
    );
    (id, ws)
}

// ── the live-boundary tests ─────────────────────────────────────────────────

/// AC-R-2.2.5-3 + AC-R-2.5.5-6 (live): a `local_sandboxed` attach spawns the
/// real `hh-helper`; an `exec` dispatch streams journaled frames through the
/// `read` long-poll into capture items whose tokens resolve (the echo is
/// verbatim — an unresolvable echo would land `unattributed`); the terminal
/// maps to `observed{applied}`.
#[test]
fn ac_s2_live_exec_streams_and_settles_observed() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-exec");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-exec");
    assert_eq!(driver.handle(&env_id).unwrap().state, HandleState::Ready);
    // The committed session record names the helper identity.
    let sess = driver.handle(&env_id).unwrap().session.clone().unwrap();
    assert!(sess.helper_identity.starts_with("hh-helper/1:"));

    let cap = capability(EffectDomain::Exec, compensable_attrs());
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let declared = EffectClass {
        domain: EffectDomain::Exec,
        attributes: Some(compensable_attrs()),
    };
    // `argv` keeps the command_parse assessor `Parsed` — a `command` string
    // carrying `>`/`;` projects `Partial` → `UNKNOWN` → Π ask.
    let args = Json::obj([(
        "argv",
        Json::Arr(vec![
            Json::str("/bin/sh"),
            Json::str("-c"),
            Json::str(format!("echo s2-live > {}/out.txt", ws.display())),
        ]),
    )]);
    let sb = no_scope_bindings();
    let mut inp = input(&cap, &bind, &sb, &env_id, "tc-1", args, declared);
    inp.compensation_plan_id = Some("plan-1".to_string());
    let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    driver.return_executor(&env_id, exec);
    match &out {
        DispatchOutcome::Observed(o) => {
            assert_eq!(o.outcome, EffectOutcome::Applied, "{o:?}");
            assert_eq!(o.exit_status, Some(0));
        }
        other => panic!("expected observed(applied), got {other:?}"),
    }
    // The world actually changed — the fs_change observer saw it.
    assert_eq!(
        std::fs::read_to_string(ws.join("out.txt")).unwrap().trim(),
        "s2-live"
    );
    // The per-class meter accrued (helper.spawns measured).
    let h = driver.handle(&env_id).unwrap();
    assert_eq!(
        h.meters.extra.get("helper.spawns").map(|s| s.value),
        Some(1)
    );
}

/// AC-R-2.5.5-7 (live): the helper enforces the deadline *itself* — a
/// `sleep` past `effect_deadline_ms` is killed by the helper and lands
/// `observed{error{timeout}}` fast (no kernel-side timer needed).
#[test]
fn ac_s2_deadline_is_helper_enforced() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-deadline");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, _ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-deadline");

    // `reversible` + argv: the vacuous inside-roots check lands Π-2 `allow`,
    // and a tool-origin error on a reversible class settles
    // `observed{error}` (compensable would land `unknown`+probe instead).
    let cap = capability(EffectDomain::Exec, reversible_attrs());
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let declared = EffectClass {
        domain: EffectDomain::Exec,
        attributes: Some(reversible_attrs()),
    };
    let sb = no_scope_bindings();
    let mut inp = input(
        &cap,
        &bind,
        &sb,
        &env_id,
        "tc-1",
        Json::obj([(
            "argv",
            Json::Arr(vec![
                Json::str("/bin/sh"),
                Json::str("-c"),
                Json::str("sleep 30"),
            ]),
        )]),
        declared,
    );
    inp.ladder.effect_deadline_ms = Some(400);
    let started_at = Instant::now();
    let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    driver.return_executor(&env_id, exec);
    assert!(
        started_at.elapsed() < std::time::Duration::from_secs(15),
        "the helper must kill at the deadline, not at sleep's end"
    );
    match &out {
        DispatchOutcome::Observed(o) => match &o.status {
            ObservedStatus::Error { class, .. } => {
                assert_eq!(class.as_str(), "timeout", "{:?}", o.status)
            }
            ObservedStatus::Ok => panic!("a deadline-exceeded exec is not ok"),
        },
        other => panic!("expected observed(error timeout), got {other:?}"),
    }
}

/// AC-R-2.5.5-10 (live): `exec` without a valid `commit_proof` is refused
/// `NotCommitted` — the helper recomputes over the session nonce and a
/// bogus/forged/mismatched proof never launches a process.
#[test]
fn ac_s2_exec_without_commit_is_refused() {
    let sock_dir = dir("live-commit");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    client
        .hello(
            hh_helper::protocol::OnKernelLoss::Terminate,
            "nonce-1",
            None,
            None,
            None,
            false,
        )
        .unwrap();
    // Forged proof — never committed.
    let err = client
        .request(&hh_helper::protocol::HelperRequest::Exec {
            execution_id: "exec-x".into(),
            effect_id: "e-x".into(),
            attempt_no: 1,
            capability_ref: ("test:write_file".into(), "v-cap".into()),
            args: Json::obj([("command", Json::str("echo hi"))]),
            cwd: ".".into(),
            env: vec![],
            deadline_ms: None,
            retain_bytes_cap: 1 << 20,
            attribution_token: "tok-x".into(),
            commit_proof: hh_helper::protocol::CommitProof::Token {
                proof: "sha256:forged".into(),
                effect_id: "e-x".into(),
                attempt_no: 1,
                fencing_token: 1,
                commit_event_id: "evt-never".into(),
                commit_seq: 1,
            },
            idempotency_key: None,
            env_clear: false,
        })
        .unwrap_err();
    match err {
        EnvError::HelperRefused { class, .. } => assert_eq!(class, "NotCommitted"),
        other => panic!("expected HelperRefused(NotCommitted), got {other}"),
    }
    client.shutdown();
}

/// AC-R-2.2.5-4 + AC-R-2.5.5-10 (live): every journaled frame echoes the
/// *request's* attribution token verbatim — the helper echoes, never mints.
/// A token mismatch lands `unattributed` kernel-side (the resolver drops
/// it); here we assert the echo is exact end-to-end.
#[test]
fn ac_s2_token_echo_is_verbatim() {
    let sock_dir = dir("live-token");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    client
        .hello(
            hh_helper::protocol::OnKernelLoss::Terminate,
            "nonce-1",
            None,
            None,
            None,
            false,
        )
        .unwrap();
    let r = client
        .request(&hh_helper::protocol::HelperRequest::Exec {
            execution_id: "exec-tok".into(),
            effect_id: "e-tok".into(),
            attempt_no: 1,
            capability_ref: ("test:write_file".into(), "v-cap".into()),
            args: Json::obj([("command", Json::str("echo tok-check"))]),
            cwd: ".".into(),
            env: vec![],
            deadline_ms: Some(10_000),
            retain_bytes_cap: 1 << 20,
            attribution_token: "att-token-xyz".into(),
            commit_proof: hh_helper::protocol::CommitProof::ReadOnly,
            idempotency_key: None,
            env_clear: false,
        })
        .unwrap();
    assert_eq!(
        r.get("execution_id").and_then(Json::as_str),
        Some("exec-tok")
    );
    // Drain the journal — every frame echoes `att-token-xyz`.
    let mut saw_chunk = false;
    let mut polls_after_exit = 0;
    loop {
        let r = client
            .request(&hh_helper::protocol::HelperRequest::Read {
                execution_id: "exec-tok".into(),
                after_seq: 0,
                max_bytes: 0,
                wait_ms: 200,
            })
            .unwrap();
        if let Json::Arr(chunks) = r.get("chunks").cloned().unwrap_or(Json::Arr(vec![])) {
            for c in &chunks {
                let f = hh_helper::protocol::HelperFrame::from_json(c).unwrap();
                let tok = match &f {
                    hh_helper::protocol::HelperFrame::Chunk { token, .. }
                    | hh_helper::protocol::HelperFrame::Progress { token, .. }
                    | hh_helper::protocol::HelperFrame::Process { token, .. } => token.clone(),
                    // `terminal` members ride on the `read` reply, not a frame.
                    hh_helper::protocol::HelperFrame::Terminal { .. } => continue,
                };
                assert_eq!(tok, "att-token-xyz", "frame echoed a foreign token");
                if matches!(f, hh_helper::protocol::HelperFrame::Chunk { .. }) {
                    saw_chunk = true;
                }
            }
        }
        if matches!(r.get("exited"), Some(Json::Bool(true))) {
            // The stdout chunk can be journaled just after the process is
            // reaped, so `exited` may lead the last `Chunk` frame. Keep
            // draining (after_seq:0 re-reads every frame) until it lands or a
            // bounded budget elapses — the assertion below still requires a
            // real chunk, this only removes the exit/journal race (flaky on
            // fast Linux runners).
            if saw_chunk || polls_after_exit >= 25 {
                break;
            }
            polls_after_exit += 1;
        }
    }
    assert!(saw_chunk, "the echo check ran on real frames");
    client.shutdown();
}

/// Executor-side dedup (R-2.5.5¹, tier-c1): a replayed `idempotency_key`
/// inside the window returns the recorded verdict — the command is NOT
/// re-executed (the second `exec` produces no output side-effect).
#[test]
fn ac_s2_executor_dedup_replays_the_recorded_verdict() {
    let sock_dir = dir("live-dedup");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    client
        .hello(
            hh_helper::protocol::OnKernelLoss::PreserveUntil { ttl_ms: 60_000 },
            "nonce-1",
            Some(60_000),
            None,
            None,
            false,
        )
        .unwrap();
    let marker = sock_dir.join("dedup-marker");
    let cmd = format!("echo run >> {}", marker.display());
    let mk_exec = |exec_id: &str| hh_helper::protocol::HelperRequest::Exec {
        execution_id: exec_id.into(),
        effect_id: "e-dedup".into(),
        attempt_no: 1,
        capability_ref: ("test:write_file".into(), "v-cap".into()),
        args: Json::obj([("command", Json::str(cmd.clone()))]),
        cwd: sock_dir.display().to_string(),
        env: vec![],
        deadline_ms: Some(10_000),
        retain_bytes_cap: 1 << 20,
        attribution_token: "tok-dedup".into(),
        commit_proof: hh_helper::protocol::CommitProof::ReadOnly,
        idempotency_key: Some("dedup-key-1".into()),
        env_clear: false,
    };
    client.request(&mk_exec("exec-d1")).unwrap();
    // Drain to terminal — the verdict binds the key.
    loop {
        let r = client
            .request(&hh_helper::protocol::HelperRequest::Read {
                execution_id: "exec-d1".into(),
                after_seq: 0,
                max_bytes: 0,
                wait_ms: 200,
            })
            .unwrap();
        if matches!(r.get("exited"), Some(Json::Bool(true))) {
            break;
        }
    }
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap().trim(),
        "run",
        "the first exec ran once"
    );
    // The replay hits the executor-side store — no re-execution.
    let r = client.request(&mk_exec("exec-d2")).unwrap();
    assert!(
        r.get("dedup_verdict").is_some(),
        "a replayed key returns the recorded verdict: {r:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap().trim(),
        "run",
        "the dedup hit must not re-execute"
    );
    client.shutdown();
}

/// A malformed frame is `protocol_error` (transport origin) — the helper
/// never silently tolerates garbage on the channel.
#[test]
fn ac_s2_malformed_frame_is_protocol_error() {
    let sock_dir = dir("live-malformed");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    let err = client
        .request_json(&Json::obj([
            ("v", Json::str("hh-helper/1")),
            ("op", Json::str("frobnicate")),
        ]))
        .unwrap_err();
    match err {
        EnvError::HelperRefused { class, .. } => {
            assert!(
                class.contains("protocol") || class.contains("version") || class == "unknown_verb",
                "malformed/unknown op must be a typed refusal, got {class}"
            );
        }
        other => panic!("expected a typed refusal, got {other}"),
    }
    client.shutdown();
}

/// Helper crash ⇒ transport-plane failure ⇒ `unknown{executor_error}` —
/// never a silent redispatch (AC-R-2.2.5-8's lapsed-window honesty).
#[test]
fn ac_s2_helper_crash_lands_unknown() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-crash");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, _ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-crash");

    // Kill the helper — a crash, not a shutdown.
    let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
    exec.client.kill_now();

    let cap = capability(EffectDomain::Exec, compensable_attrs());
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let declared = EffectClass {
        domain: EffectDomain::Exec,
        attributes: Some(compensable_attrs()),
    };
    let sb = no_scope_bindings();
    let mut inp = input(
        &cap,
        &bind,
        &sb,
        &env_id,
        "tc-1",
        Json::obj([("command", Json::str("echo never"))]),
        declared,
    );
    inp.compensation_plan_id = Some("plan-1".to_string());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    drop(exec); // the dead session doesn't return to the table
    match out {
        DispatchOutcome::Unknown { cause } => assert_eq!(cause, "executor_error"),
        other => panic!("a helper crash must land unknown, got {other:?}"),
    }
    // The environment is unreachable — the honest mark; heal_live then
    // decides (a dead session fails the liveness probe).
    driver
        .mark_unreachable(&mut store, &lease, &env_id)
        .unwrap();
    let r = driver.heal_live(&mut store, &lease, &env_id);
    assert!(r.is_err(), "a dead session cannot heal");
    assert_eq!(driver.handle(&env_id).unwrap().state, HandleState::Failed);
}

/// AC-R-2.2.5-7 (live): unreachable → heal_live with a *live* helper session
/// reattaches (kernel death ≠ environment death) — the same session keeps
/// serving execs after reattach.
#[test]
fn ac_s2_unreachable_heal_live_reattaches() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-heal");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, _ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-heal");

    driver
        .mark_unreachable(&mut store, &lease, &env_id)
        .unwrap();
    assert_eq!(
        driver.handle(&env_id).unwrap().state,
        HandleState::Unreachable
    );
    driver.heal_live(&mut store, &lease, &env_id).unwrap();
    assert_eq!(driver.handle(&env_id).unwrap().state, HandleState::Ready);
    assert_eq!(driver.handle(&env_id).unwrap().heal_count, 1);
    // And the session still works.
    let mut exec = driver.take_executor(&env_id, vec![]).unwrap();
    let cap = capability(EffectDomain::Exec, compensable_attrs());
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let declared = EffectClass {
        domain: EffectDomain::Exec,
        attributes: Some(compensable_attrs()),
    };
    let sb = no_scope_bindings();
    let mut inp = input(
        &cap,
        &bind,
        &sb,
        &env_id,
        "tc-2",
        Json::obj([("command", Json::str("echo still-alive"))]),
        declared,
    );
    inp.compensation_plan_id = Some("plan-1".to_string());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    driver.return_executor(&env_id, exec);
    assert!(matches!(out, DispatchOutcome::Observed(_)));
}

/// `replace` — an unreachable env with `on_loss = replace_from_image`
/// yields a successor handle with a `ParentEdge`; the old handle is
/// terminal `replaced` (AC-R-2.2.5-8's replace rung).
#[test]
fn ac_s2_replace_lands_successor() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-replace");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let ws = workspace("live-replace");
    let env_id = ready_env(
        &mut store,
        &lease,
        &mut driver,
        &ws,
        EnvironmentClass::LocalSandboxed,
        OnLoss::ReplaceFromImage,
        ImageRef::ContentAddress(test_ca()),
    );
    driver
        .mark_unreachable(&mut store, &lease, &env_id)
        .unwrap();
    let child = driver.replace(&mut store, &lease, &env_id).unwrap();
    assert_eq!(driver.handle(&env_id).unwrap().state, HandleState::Replaced);
    assert_eq!(child.state, HandleState::Provisioning);
    assert!(matches!(
        child.parent.as_ref().map(|p| p.mode),
        Some(DeriveMode::FreshFromImage)
    ));
    // The successor attaches to `ready` — a fresh helper spawns for it.
    let backend = Ep2Model::reference();
    driver
        .attach(
            &mut store,
            &lease,
            &child.env_handle_id,
            Some(&backend),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    assert_eq!(
        driver.handle(&child.env_handle_id).unwrap().state,
        HandleState::Ready
    );
}

/// `derive(fork_snapshot | scoped_subtree)` — the fork's workspace carries
/// the parent's files; the subtree child is narrowed to the scope.
#[test]
fn ac_s2_derive_modes() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-derive");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-derive");
    driver
        .fs_write(
            &env_id,
            &format!("{}/parent.txt", ws.display()),
            b"from-parent",
        )
        .unwrap();
    std::fs::create_dir_all(ws.join("sub")).unwrap();
    driver
        .fs_write(
            &env_id,
            &format!("{}/sub/inner.txt", ws.display()),
            b"inner",
        )
        .unwrap();

    // fork_snapshot — the child's workspace is a copy.
    let fork = driver
        .derive(
            &mut store,
            &lease,
            &env_id,
            DeriveMode::ForkSnapshot,
            None,
            OnParentEnd::Teardown,
        )
        .unwrap();
    let fork_ws = fork.roots.cwd.clone();
    assert_eq!(
        std::fs::read_to_string(format!("{fork_ws}/parent.txt")).unwrap(),
        "from-parent"
    );
    assert_eq!(
        std::fs::read_to_string(format!("{fork_ws}/sub/inner.txt")).unwrap(),
        "inner"
    );

    // scoped_subtree — narrowed roots.
    let sub = driver
        .derive(
            &mut store,
            &lease,
            &env_id,
            DeriveMode::ScopedSubtree,
            Some("sub"),
            OnParentEnd::Teardown,
        )
        .unwrap();
    assert_eq!(sub.roots.writable_roots.len(), 1);
    assert!(sub.roots.cwd.ends_with("/sub"));
    // A scope outside the writable root refuses.
    let refused = driver.derive(
        &mut store,
        &lease,
        &env_id,
        DeriveMode::ScopedSubtree,
        Some("../escape"),
        OnParentEnd::Teardown,
    );
    assert!(matches!(refused, Err(EnvError::OutsideRoots { .. })));
}

/// `fs_tree` — snapshot → modify → `verify` fails → restore brings it back;
/// `diff` is deterministic (AC-R-2.2.5-10's tree identity).
#[test]
fn ac_s2_fs_tree_snapshot_verify_restore() {
    if !hh_helper::seatbelt::is_available() {
        eprintln!("SKIP: seatbelt unavailable (no /usr/bin/sandbox-exec)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-fstree");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let (env_id, ws) = sandboxed_env(&mut store, &lease, &mut driver, "live-fstree");
    driver
        .fs_write(&env_id, &format!("{}/a.txt", ws.display()), b"alpha")
        .unwrap();
    let (rec, snap) = driver
        .fs_tree_snapshot(&mut store, &lease, &env_id)
        .unwrap();
    assert!(!rec.snapshot_ref.is_empty());
    assert!(driver.fs_tree_verify(&env_id, &snap).unwrap());
    // Mutate → verify fails closed.
    driver
        .fs_write(&env_id, &format!("{}/a.txt", ws.display()), b"mutated")
        .unwrap();
    driver
        .fs_write(&env_id, &format!("{}/b.txt", ws.display()), b"new")
        .unwrap();
    assert!(!driver.fs_tree_verify(&env_id, &snap).unwrap());
    // The diff is deterministic.
    let now = hh_helper::fstree::walk(&snap.roots).unwrap();
    let d1 = hh_helper::fstree::diff(&snap, &now);
    let d2 = hh_helper::fstree::diff(&snap, &now);
    assert_eq!(d1, d2);
    assert!(d1
        .iter()
        .any(|e| e.change == "modified" && e.relpath == "a.txt"));
    assert!(d1
        .iter()
        .any(|e| e.change == "added" && e.relpath == "b.txt"));
    // Restore materialises the snapshot's content from the blob pool —
    // independent of the live paths.
    std::fs::remove_file(ws.join("a.txt")).unwrap();
    std::fs::remove_file(ws.join("b.txt")).unwrap();
    let recomputed = driver.fs_tree_restore(&store, &env_id, &snap).unwrap();
    assert_eq!(std::fs::read_to_string(ws.join("a.txt")).unwrap(), "alpha");
    assert!(driver.fs_tree_verify(&env_id, &snap).unwrap() || recomputed == snap.tree_address);
}

/// Detached-child detection: `sh -c 'sleep N &'` exits with a live member
/// in the process group — the journal's `detached[]` reports it (the
/// capture's `process{detached}` evidence).
#[test]
fn ac_s2_detached_child_is_detected() {
    let sock_dir = dir("live-detached");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    client
        .hello(
            hh_helper::protocol::OnKernelLoss::Terminate,
            "nonce-1",
            None,
            None,
            None,
            false,
        )
        .unwrap();
    client
        .request(&hh_helper::protocol::HelperRequest::Exec {
            execution_id: "exec-det".into(),
            effect_id: "e-det".into(),
            attempt_no: 1,
            capability_ref: ("test:write_file".into(), "v-cap".into()),
            args: Json::obj([("command", Json::str("sleep 30 &"))]),
            cwd: ".".into(),
            env: vec![],
            deadline_ms: Some(10_000),
            retain_bytes_cap: 1 << 20,
            attribution_token: "tok-det".into(),
            commit_proof: hh_helper::protocol::CommitProof::ReadOnly,
            idempotency_key: None,
            env_clear: false,
        })
        .unwrap();
    let mut detached = Vec::new();
    loop {
        let r = client
            .request(&hh_helper::protocol::HelperRequest::Read {
                execution_id: "exec-det".into(),
                after_seq: 0,
                max_bytes: 0,
                wait_ms: 300,
            })
            .unwrap();
        if let Json::Arr(d) = r.get("detached").cloned().unwrap_or(Json::Arr(vec![])) {
            detached = d;
        }
        if matches!(r.get("exited"), Some(Json::Bool(true))) {
            break;
        }
    }
    assert!(
        !detached.is_empty(),
        "a live background child must surface in detached[]"
    );
    // Clean up the orphan.
    let _ = std::process::Command::new("pkill")
        .args(["-f", "sleep 30"])
        .status();
    client.shutdown();
}

/// `local_container` — the podman lane (live; skipped when the runtime is
/// unavailable — the operator authorized podman for S2.1; on a machine
/// without it this test reports a skip rather than a false pass).
#[test]
fn ac_s2_local_container_exec_runs_inside_podman() {
    if !hh_helper::podman::is_available() {
        eprintln!("SKIP: podman unavailable (no reachable engine)");
        return;
    }
    let (mut store, run, lease, _clock) = open("live-podman");
    open_scopes(&mut store, &run, &lease, "turn-1", "mc-1");
    let mut driver = EnvDriver::new(&run);
    let ws = workspace("live-podman");
    // The container image is a tag — the record's `unpinned[]` admits it
    // (the run records `unpinned_tag`, honestly R0).
    let mut record = test_record(
        EnvironmentClass::LocalContainer,
        ImageRef::Tag(hh_helper::podman::PodmanBackend::DEFAULT_IMAGE.to_string()),
    );
    record
        .unpinned
        .insert(hh_helper::podman::PodmanBackend::DEFAULT_IMAGE.to_string());
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
    let backend = Ep2Model::reference();
    driver
        .attach(
            &mut store,
            &lease,
            &h.env_handle_id,
            Some(&backend),
            AttachMode::FailClosed,
            true,
            &[],
        )
        .unwrap();
    let cap = capability(EffectDomain::Exec, compensable_attrs());
    let bind = binding();
    let monitor = test_monitor(&cap);
    let mut disp = Dispatcher::new(&mut store, &monitor, &run, [7; 32], DetectorSet::default());
    let declared = EffectClass {
        domain: EffectDomain::Exec,
        attributes: Some(compensable_attrs()),
    };
    // Write inside the container at /work — the host mount shows it.
    let mut exec = driver.take_executor(&h.env_handle_id, vec![]).unwrap();
    let sb = no_scope_bindings();
    let mut inp = input(
        &cap,
        &bind,
        &sb,
        &h.env_handle_id,
        "tc-1",
        Json::obj([(
            "argv",
            Json::Arr(vec![
                Json::str("/bin/sh"),
                Json::str("-c"),
                Json::str("echo in-container > /work/proof.txt; hostname"),
            ]),
        )]),
        declared,
    );
    inp.compensation_plan_id = Some("plan-1".to_string());
    let out = disp
        .dispatch(&mut driver, &mut exec, None, &inp, &lease)
        .unwrap();
    driver.return_executor(&h.env_handle_id, exec);
    match &out {
        DispatchOutcome::Observed(o) => assert_eq!(o.outcome, EffectOutcome::Applied, "{o:?}"),
        other => panic!("expected observed(applied) in container, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(ws.join("proof.txt"))
            .unwrap()
            .trim(),
        "in-container"
    );
    // Teardown removes the container.
    driver
        .teardown(&mut store, &lease, &h.env_handle_id)
        .unwrap();
}

/// The helper binary must resolve in tests (cargo builds `bin` targets to
/// `target/{profile}/`).
#[test]
fn helper_binary_resolves() {
    assert!(helper_binary().is_some(), "hh-helper binary not found");
}

/// Removability(0) — the build with tiers > 0 absent (`hh-helper` compiled
/// `--no-default-features`, i.e. no `tier-c1`) must refuse the C1 halves
/// honestly (`preserve_until`, `dedup_window` → `unsupported`) while every
/// tier-0 verb still works. Driven by `scripts/check-removability.sh` which
/// builds the tier-0 binary and sets `HH_REM0=1`; a default-build run skips.
#[test]
fn ac_s2_removability0_c1_halves_refuse_honestly() {
    if std::env::var("HH_REM0").is_err() {
        eprintln!("SKIP: removability(0) needs the no-default-features helper (HH_REM0)");
        return;
    }
    let sock_dir = dir("live-rem0");
    let mut client = HelperClient::spawn(&sock_dir, "direct", &[]).unwrap();
    // A plain tier-0 hello still opens the session.
    client
        .hello(
            hh_helper::protocol::OnKernelLoss::Terminate,
            "nonce-1",
            None,
            None,
            None,
            false,
        )
        .unwrap();
    // `preserve_until` — refused, honestly.
    let err = client
        .request(&hh_helper::protocol::HelperRequest::Hello {
            client: "hh-kernel".into(),
            resume_session_id: None,
            on_kernel_loss: hh_helper::protocol::OnKernelLoss::PreserveUntil { ttl_ms: 1000 },
            session_nonce: "nonce-1".into(),
            dedup_window_ms: None,
            policy: None,
            roots: None,
        })
        .unwrap_err();
    match err {
        EnvError::HelperRefused { class, .. } => assert_eq!(class, "unsupported"),
        other => panic!("preserve_until must refuse unsupported, got {other}"),
    }
    // `dedup_window` — refused, honestly.
    let err = client
        .request(&hh_helper::protocol::HelperRequest::Hello {
            client: "hh-kernel".into(),
            resume_session_id: None,
            on_kernel_loss: hh_helper::protocol::OnKernelLoss::Terminate,
            session_nonce: "nonce-1".into(),
            dedup_window_ms: Some(1000),
            policy: None,
            roots: None,
        })
        .unwrap_err();
    match err {
        EnvError::HelperRefused { class, .. } => assert_eq!(class, "unsupported"),
        other => panic!("dedup_window must refuse unsupported, got {other}"),
    }
    // Tier-0 exec still runs on the same binary.
    client
        .request(&hh_helper::protocol::HelperRequest::Exec {
            execution_id: "exec-r0".into(),
            effect_id: "e-r0".into(),
            attempt_no: 1,
            capability_ref: ("test:write_file".into(), "v-cap".into()),
            args: Json::obj([("command", Json::str("echo tier0-ok"))]),
            cwd: ".".into(),
            env: vec![],
            deadline_ms: Some(10_000),
            retain_bytes_cap: 1 << 20,
            attribution_token: "tok-r0".into(),
            commit_proof: hh_helper::protocol::CommitProof::ReadOnly,
            idempotency_key: None,
            env_clear: false,
        })
        .unwrap();
    loop {
        let r = client
            .request(&hh_helper::protocol::HelperRequest::Read {
                execution_id: "exec-r0".into(),
                after_seq: 0,
                max_bytes: 0,
                wait_ms: 200,
            })
            .unwrap();
        if matches!(r.get("exited"), Some(Json::Bool(true))) {
            assert_eq!(r.get("exit_status").and_then(Json::as_int), Some(0));
            break;
        }
    }
    client.shutdown();
}
