//! s4-cell — the S4 confirmation slot driver. One invocation drives ONE
//! frozen-manifest slot (task × arm × round) through the HH subject
//! machinery on a real podman cell: provision/attach on the task's
//! derived image → staged mediated dispatches (stage → oracle solution →
//! verifier verbatim → verifier vendored when the bootstrap can't run
//! isolated) → the slot arm's REAL `compact()` over the actual
//! transcript → seal → collect → teardown. Emits the per-slot JSON
//! record; every fact is observed verbatim — never a fabricated verdict.
//!
//! Scaffold shape: crates/hh-env/tests/r5_46.rs (the pilot's drive_cell).
//!
//! argv[1] = slot spec JSON:
//!   {slot_ref, task_id, arm_id, strategy, round, first, lane_id,
//!    image, task_dir, work_root, solution_kind: sh|yaml,
//!    max_agent_timeout_sec, max_test_timeout_sec,
//!    pytest_target, vendored_cmd|null, projected_env: [[k,v]…]}
//! stdout: the per-slot record JSON (canonical, single line).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use hharness_compiler::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use hharness_compiler::plan::PinnedRef;
use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{
    kernel_default, ExecPolicy, ResourceLimits,
    WritableRoot,
};
use hharness_context::compact::{
    self, ClearToolResults, CompactInput, CompactionTrigger, EvictOldest,
};
use hharness_context::events::CollectSink;
use hharness_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill,
};
use hharness_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hharness_env::capture::OutputPolicy;
use hharness_env::deadline::DeadlineLadder;
use hharness_env::dispatch::{DispatchInput, DispatchOutcome, Dispatcher};
use hharness_env::driver::EnvDriver;
use hharness_env::egress_profile::{self, DeclaredDeps};
use hharness_env::events::{EventMinter, ScopeChain};
use hharness_env::executor::ToolExecutor;
use hharness_env::handle::{OnLoss, Roots};
use hharness_env::ops::{MarkerSink, ObservationPoint, OpsObserver, Sentinel};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::verifier_bound::{self, VerifierBoundAdmission};
use hharness_hir::kinds::{
    EffectAttributes, EffectClass, EffectDomain, Mutability, RepeatSafety, Reversibility,
    ToolEffects, World,
};
use hharness_hir::leaves::Text;
use hharness_hir::records::Validity;
use hharness_hir::records::{
    Grant, GrantConstraints, Resources, ScopeBindings, ToolCapabilityRecord,
};
use hharness_hir::refs::Ref;
use hharness_ledger::event::{Cursor, Direction, Scope};
use hharness_ledger::ids::{effect_id, ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_monitor::handle::{
    AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis,
};
use hharness_monitor::monitor::{CapabilityEntry, Monitor};
use hharness_monitor::policy::{default_table, Mode};
use hharness_monitor::table::HandleTable;
use hharness_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_secrets::redact::DetectorSet;
use hharness_wire::json::{self, Json};
use hharness_wire::sha256::sha256_hex;

// ── spec ───────────────────────────────────────────────────────────

struct Spec {
    slot_ref: String,
    task_id: String,
    arm_id: String,
    strategy: String,
    round: i64,
    first: bool,
    lane_id: String,
    image: String,
    task_dir: String,
    work_root: String,
    solution_kind: String,
    max_agent_timeout_sec: f64,
    max_test_timeout_sec: f64,
    pytest_target: String,
    vendored_cmd: Option<String>,
    solution_file: String,
    projected_env: Vec<(String, String)>,
    // ── egress-supplement admission fields (absent = stock posture) ──
    /// `declared_deps` → the slot's cell is admitted through the
    /// first-class R5.56 machinery (`egress_profile::admit_declared_deps`
    /// → policy `net.mode = public` + declared `host_bridge` residual →
    /// stock `hh-helper --network bridge`); anything else → none.
    egress_profile: String,
    /// Helper binary override (the stock repo `hh-helper` carrying the
    /// R5.56 `--network` flag); empty = the `HH_HELPER_BIN`/`hh-helper`
    /// resolution.
    helper_bin: String,
    /// The containment policy's honest net declaration for this slot:
    /// `none` (stock) or `public` (egress-admitted — the policy records
    /// there is no net boundary to rely on).
    policy_net_mode: String,
    /// The task's declared runtime egress deps (the mechanical scan's
    /// positive signature keys — the admission's `deps` verbatim).
    declared_deps: Vec<String>,
    /// R5.58 GCE backend: the ssh target the cell's `/work` tree is
    /// mirrored to (`host` — `gcloud compute ssh host --project --zone`);
    /// empty = local podman (no mirroring).
    remote_host: String,
    remote_project: String,
    remote_zone: String,
    /// R5.57 verifier-bound admission members for the astropy slots —
    /// present only on the ten `swe-bench-astropy-2` slot specs.
    verifier_admission: Option<VerifierBoundAdmission>,
    /// s4-30slot-repairs machinery repair (recorded verbatim): the task
    /// image's declared WORKDIR read from `podman image inspect` of the
    /// built cell image — the cwd the task harness execs the oracle
    /// solution + verifier under. The prior epochs hardcoded `cd /app`,
    /// which broke every task whose repo lives elsewhere (verbatim
    /// recorded cause of the astropy bootstrap failure). Empty = the
    /// prior `/app` default, echoed verbatim in the slot record.
    task_workdir: String,
}

fn sj(j: &Json, k: &str) -> String {
    j.get(k).and_then(Json::as_str).unwrap_or("").to_string()
}
fn parse_spec(path: &str) -> Spec {
    let j = json::parse(&std::fs::read_to_string(path).expect("spec file")).expect("spec json");
    let mut penv = vec![];
    if let Some(Json::Arr(a)) = j.get("projected_env") {
        for p in a {
            if let Json::Arr(kv) = p {
                if kv.len() == 2 {
                    penv.push((
                        kv[0].as_str().unwrap_or("").to_string(),
                        kv[1].as_str().unwrap_or("").to_string(),
                    ));
                }
            }
        }
    }
    let jb = |k: &str| matches!(j.get(k), Some(Json::Bool(true)));
    let jf = |k: &str, d: f64| match j.get(k) {
        Some(Json::Int(i)) => *i as f64,
        Some(Json::Str(s)) => s.parse().unwrap_or(d),
        _ => d,
    };
    Spec {
        slot_ref: sj(&j, "slot_ref"),
        task_id: sj(&j, "task_id"),
        arm_id: sj(&j, "arm_id"),
        strategy: sj(&j, "strategy"),
        round: j.get("round").and_then(Json::as_int).unwrap_or(0),
        first: jb("first"),
        lane_id: sj(&j, "lane_id"),
        image: sj(&j, "image"),
        task_dir: sj(&j, "task_dir"),
        work_root: sj(&j, "work_root"),
        solution_kind: sj(&j, "solution_kind"),
        max_agent_timeout_sec: jf("max_agent_timeout_sec", 360.0),
        max_test_timeout_sec: jf("max_test_timeout_sec", 60.0),
        pytest_target: sj(&j, "pytest_target"),
        vendored_cmd: j
            .get("vendored_cmd")
            .and_then(Json::as_str)
            .map(|s| s.to_string()),
        solution_file: sj(&j, "solution_file"),
        projected_env: penv,
        egress_profile: sj(&j, "egress_profile"),
        helper_bin: sj(&j, "helper_bin"),
        policy_net_mode: sj(&j, "policy_net_mode"),
        declared_deps: match j.get("declared_deps") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(|s| s.to_string())
                .collect(),
            _ => vec![],
        },
        remote_host: sj(&j, "remote_host"),
        remote_project: sj(&j, "remote_project"),
        remote_zone: sj(&j, "remote_zone"),
        verifier_admission: j.get("verifier_admission").map(|a| {
            VerifierBoundAdmission {
                task_id: sj(a, "task_id"),
                enforced_before_sec: a
                    .get("enforced_before_sec")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
                admitted_sec: a
                    .get("admitted_sec")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
                basis: sj(a, "basis"),
                prior_disposition: sj(a, "prior_disposition"),
                supplement_epoch: sj(a, "supplement_epoch"),
                authorization: sj(a, "authorization"),
                scope: sj(a, "scope"),
            }
        }),
        task_workdir: sj(&j, "task_workdir"),
    }
}

// ── scaffold (r5_46 shapes) ────────────────────────────────────────

fn open(tag: &str, spec: &Spec) -> (Store, String, Lease) {
    let p = Path::new(&spec.work_root).join("stores").join(tag);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    let mut s = Store::open_with(
        p,
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
fn test_policy(ws: &Path) -> hharness_containment::policy::ContainmentPolicy {
    let mut p = kernel_default(0);
    p.fs.write.allow.push(WritableRoot {
        root: ws.display().to_string(),
        read_only_subpaths: vec![],
        protected_metadata_names: vec![],
    });
    p.fs.exec = ExecPolicy::Allow(vec!["/bin".to_string()]);
    // Net posture is declared by the first-class admissions, never by
    // hand-editing the policy here: `none` is the kernel default and
    // stays unless `egress_profile::admit_declared_deps` mutates the
    // policy to `public` with the declared `host_bridge` residual (the
    // R5.56 sanctioned machinery — a `public` mode without the residual
    // fails `PublicWithoutResidual` inside the admission's own
    // `validate`, so a half-admitted cell can never form).
    p.compute_ids();
    p
}
/// Returns `(env_id, enforced_test_decl_sec, slot_echo, cell_network)`.
/// `slot_echo` is the verbatim `ext` echo (`verifier_bound_admitted`,
/// admission `prior_disposition`/`supplement_epoch`) the R5.57
/// machinery emits when the stamped admission is present.
fn ready_cell(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &Path,
    spec: &Spec,
) -> Result<(String, u64, Vec<(String, Json)>, String), String> {
    let mut record = EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag(spec.image.clone()),
        build_context: None,
        platform: "s4-slot".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("s4:policy".to_string(), "v-pol".to_string()),
        limits: ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    record.unpinned.insert(spec.image.clone());
    // The spec surface fields the R5.57 admission verifies against —
    // the enforced decl verbatim, pre-admission.
    record.ext.insert(
        "task_id".to_string(),
        Json::str(spec.task_id.clone()),
    );
    record.ext.insert(
        verifier_bound::SPEC_FIELD.to_string(),
        Json::Int(spec.max_test_timeout_sec as i64),
    );
    let mut policy = test_policy(ws);
    // The egress admission (R5.56 first-class): mutates the policy to
    // the honest `public` declaration and stamps the record's ext —
    // transactional; a refused admission produces no half-labelled
    // cell and is recorded verbatim.
    if spec.egress_profile == egress_profile::EGRESS_PROFILE_DECLARED_DEPS {
        let decl = DeclaredDeps {
            deps: spec.declared_deps.clone(),
            owner: "s4-completion supplement (operator-authorized bounded \
                    egress profile; docs/tickets/249_R5.58__s4-completion.md)"
                .to_string(),
            closer: "cell teardown at slot end (the helper owns the \
                     container's lifecycle)"
                .to_string(),
        };
        egress_profile::admit_declared_deps(&mut record, &mut policy, &decl, 0)
            .map_err(|e| format!("egress_admission_refused:{e}"))?;
    }
    // The verifier-bound admission (R5.57 first-class): stamps the
    // admitted decl + the verbatim record; enforced_before is verified
    // against the spec's actually enforced decl, never presumed.
    if let Some(adm) = &spec.verifier_admission {
        verifier_bound::admit_verifier_bound(&mut record, adm)
            .map_err(|e| format!("verifier_bound_refused:{e}"))?;
    }
    let enforced_test_decl = verifier_bound::enforced_decl_sec(&record)
        .map_err(|e| format!("enforced_decl:{e}"))?;
    let cell_network = match record.ext.get("cell_network") {
        Some(Json::Str(s)) => s.clone(),
        _ => "none".to_string(),
    };
    let echo: Vec<(String, Json)> = verifier_bound::slot_echo(&record)
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
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
            PolicySlot::Inline(Box::new(policy)),
            OnLoss::FailRun,
            None,
        )
        .map_err(|e| format!("provision:{e}"))?;
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
        .map_err(|e| format!("attach:{e}"))?;
    Ok((h.env_handle_id, enforced_test_decl, echo, cell_network))
}
fn attrs() -> EffectAttributes {
    EffectAttributes {
        mutability: Mutability::Additive,
        repeat_safety: RepeatSafety::Idempotent,
        world: World::Closed,
        reversibility: Reversibility::Compensable,
    }
}
fn capability() -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose: Text::new("s4 task exec", "s4:driver", ProvenanceRecord::kernel("t", 0)),
        input_schema: Json::obj([(
            "properties",
            Json::obj([("argv", Json::obj([("type", Json::str("array"))]))]),
        )]),
        output_schema: None,
        effects: ToolEffects::Declared(
            vec![EffectClass {
                domain: EffectDomain::Exec,
                attributes: Some(attrs()),
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
            version_id: "v-perm".to_string(),
        },
        holder: Ref::selected("test:agent", "latest"),
        issuer: ProvenanceRecord::kernel("hh-env/s4", 0),
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
    command: &str,
    ordinal: u64,
    tc: &str,
    deadline_ms: Option<u64>,
) -> DispatchInput<'a> {
    DispatchInput {
        chain: ScopeChain {
            turn_id: "turn-1".to_string(),
            model_call_id: "mc-1".to_string(),
            tool_call_id: tc.to_string(),
        },
        ordinal,
        proposer: "test:agent".to_string(),
        binding: bind,
        scope_bindings: sb,
        capability: cap,
        capability_ref: PinnedRef {
            semantic_id: "test:write_file".to_string(),
            version_id: "v-cap".to_string(),
        },
        surface_args: Json::obj([("argv", Json::Arr(vec![Json::str("/bin/sh"), Json::str("-c"), Json::str(command)]))]),
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
            attributes: Some(attrs()),
        },
        requested_grants: vec![],
        ladder: DeadlineLadder {
            run_deadline_ms: None,
            phase_deadline_ms: None,
            effect_deadline_ms: None,
            attempt_deadline_ms: deadline_ms,
        },
        output_policy: OutputPolicy {
            retain_bytes_cap: 1 << 20,
            model_view: "tail".to_string(),
            offload_above: 1 << 20,
            max_deltas: 64,
        },
        reserve: None,
        compensation_plan_id: Some(format!("plan-{tc}")),
        baseline_ref: None,
        flow: Default::default(),
        remedy_taken: None,
    }
}

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

// ── R5.58 GCE backend: /work tree mirroring ────────────────────────
// The podman `-v ws:/work` bind names a path on the podman *host* —
// on the remote backend that path lives on the VM (the identical
// absolute path under the `/Users/stevenvitali` mirror root). The
// staged tree is pushed before provision and pulled after each
// dispatch via a tar stream over `gcloud compute ssh` — the only
// bytes crossing are the container-visible workspace; the store,
// observer and sentinel stay local.

fn gcloud_ssh_args(spec: &Spec) -> Vec<String> {
    vec![
        "compute".to_string(),
        "ssh".to_string(),
        spec.remote_host.clone(),
        format!("--project={}", spec.remote_project),
        format!("--zone={}", spec.remote_zone),
        "--quiet".to_string(),
    ]
}

/// Push the local ws tree to the remote path (creates it first).
fn remote_push(ws: &Path, spec: &Spec) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let tar = Command::new("tar")
        .arg("-C")
        .arg(ws)
        .args(["-cf", "-", "."])
        .output()
        .map_err(|e| format!("tar local: {e}"))?;
    if !tar.status.success() {
        return Err(format!("tar local rc {:?}", tar.status.code()));
    }
    let mut cmd = Command::new("gcloud");
    cmd.args(gcloud_ssh_args(spec)).arg(format!(
        "--command=mkdir -p '{}' && tar -C '{}' -xf -",
        ws.display(),
        ws.display()
    ));
    cmd.stdin(Stdio::piped());
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("gcloud ssh spawn: {e}"))?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&tar.stdout)
        .map_err(|e| format!("push stdin: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("push wait: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "remote_push rc {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        ));
    }
    Ok(format!("pushed:{}B", tar.stdout.len()))
}

/// Pull the remote ws tree back over the local one (tar merge — new
/// remote-side outputs land; identical staged content is a no-op).
fn remote_pull(ws: &Path, spec: &Spec) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("gcloud");
    cmd.args(gcloud_ssh_args(spec)).arg(format!(
        "--command=cd '{}' && tar -cf - .",
        ws.display()
    ));
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("gcloud ssh pull: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "remote_pull rc {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        ));
    }
    let mut untar = Command::new("tar")
        .arg("-C")
        .arg(ws)
        .args(["-xf", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("untar spawn: {e}"))?;
    untar
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&out.stdout)
        .map_err(|e| format!("untar stdin: {e}"))?;
    let st = untar.wait().map_err(|e| format!("untar wait: {e}"))?;
    if !st.success() {
        return Err(format!("untar rc {st:?}"));
    }
    Ok(format!("pulled:{}B", out.stdout.len()))
}
fn completed_seq(store: &Store, run: &str, eid: &str) -> Option<u64> {
    store
        .read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
        .iter()
        .find(|e| e.class == "action.tool.completed" && e.scope.effect_id.as_deref() == Some(eid))
        .map(|e| e.seq)
}

fn dispatch(
    store: &mut Store,
    monitor: &Monitor,
    run: &str,
    driver: &mut EnvDriver,
    exec: &mut hharness_env::helper::HelperExecutor,
    cap: &ToolCapabilityRecord,
    bind: &SurfaceBinding,
    sb: &ScopeBindings,
    env_id: &str,
    command: &str,
    ordinal: u64,
    tc: &str,
    deadline_ms: Option<u64>,
    lease: &Lease,
) -> Result<String, String> {
    let mut disp = Dispatcher::new(store, monitor, run, [7; 32], DetectorSet::default());
    match disp.dispatch(
        driver,
        exec,
        None,
        &input(cap, bind, sb, env_id, command, ordinal, tc, deadline_ms),
        lease,
    ) {
        Ok(DispatchOutcome::Observed(o)) => Ok(format!("observed:{}", o.outcome.as_str())),
        Ok(DispatchOutcome::Refused { reason }) => Ok(format!("refused:{reason}")),
        Ok(DispatchOutcome::Unknown { cause }) => Ok(format!("unknown:{cause}")),
        Ok(DispatchOutcome::Duplicate { prior_effect_id, .. }) => {
            Ok(format!("duplicate:{prior_effect_id}"))
        }
        Ok(DispatchOutcome::Suspended { permission_id }) => {
            Ok(format!("suspended:{permission_id}"))
        }
        Ok(DispatchOutcome::Paused { effect_id, .. }) => Ok(format!("paused:{effect_id}")),
        Ok(DispatchOutcome::ObservedCached { .. }) => Ok("observed:cached".to_string()),
        Ok(_) => Ok("other".to_string()),
        Err(e) => Err(format!("dispatch:{e}")),
    }
}

// ── transcript candidates → compact() ─────────────────────────────

fn cand(id: &str, tokens: u64, seq: u64, inv: Option<(&str, &str)>) -> Candidate {
    let mut c = Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(PriorityClass::ObservationOld),
        estimate: Estimate {
            tokens,
            estimator_ref: "est/s4-bytes-div4".into(),
        },
        source_event: None,
        source_seq: seq,
        label: Label::at(AuthorityClass::Delegate),
        provenance: ProvenanceRecord::kernel("kernel:context", 0),
        validity: Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    };
    if let Some((cap, i)) = inv {
        c.provenance = ProvenanceRecord {
            origin: Origin::tool(cap, i),
            authority: AuthorityClass::Delegate,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
            scope: PersistenceScope::Run,
            derived_from: Vec::new(),
            created_at: 0,
            attestation: None,
        };
    }
    c
}
fn plan_of(cands: &[Candidate], run_id: &str) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let candidates: BTreeMap<String, Candidate> = cands
        .iter()
        .map(|c| (c.candidate_id.clone(), c.clone()))
        .collect();
    let items: Vec<PlannedItem> = cands
        .iter()
        .map(|c| PlannedItem {
            candidate_id: c.candidate_id.clone(),
            context_item_id: c.context_item_id.clone().unwrap(),
            artefact_id: None,
            delivery_id: format!("del:{}", c.candidate_id),
            authority: c.label.authority,
            label: c.label.clone(),
            tokens: c.estimate.tokens,
            state: c.state,
            delivered_by_reference: false,
            derived_from: None,
        })
        .collect();
    let plan = ContextPlan {
        plan_id: format!("plan:{run_id}"),
        model_call_id: "mc-1".into(),
        derived_from: DerivedFrom {
            run_id: run_id.into(),
            seq: 10,
            view_hash: "sha256:view".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items,
        }],
        omitted: vec![],
        context_label: Label::top(),
        occupancy_estimate: cands.iter().map(|c| c.estimate.tokens).sum(),
        legal_cut_points: (0..=64).map(|i| CutPoint { before_index: i }).collect(),
        reserved: 0,
        estimator_ref: "est/s4-bytes-div4".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

// ── per-slot record ────────────────────────────────────────────────

fn rget(b: &[u8]) -> (String, u64) {
    (sha256_hex(b), b.len() as u64)
}
fn read_ws(ws: &Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(ws.join(name)).ok()
}

fn main() {
    let spec_path = std::env::args().nth(1).expect("spec path");
    let spec = parse_spec(&spec_path);
    // ── first-class egress admission (R5.56 machinery) ─────────────
    // When the slot spec carries `egress_profile: declared_deps`, the
    // attached policy is mutated by `admit_declared_deps` (net.mode =
    // public + the declared host_bridge residual) and the stock
    // hh-helper's R5.56 `--network bridge` flag is appended by
    // `podman_helper_args` — no scratch helper, no env knob. Every fact
    // is recorded in the slot record below — a named admission, never
    // a silent confinement bypass.
    let egress_admitted = spec.egress_profile == egress_profile::EGRESS_PROFILE_DECLARED_DEPS;
    if !spec.helper_bin.is_empty() {
        std::env::set_var("HH_HELPER_BIN", &spec.helper_bin);
    }
    let remote = !spec.remote_host.is_empty();
    let mut m: BTreeMap<String, Json> = BTreeMap::new();
    let put = |m: &mut BTreeMap<String, Json>, k: &str, v: Json| {
        m.insert(k.to_string(), v);
    };
    for (k, v) in [
        ("schema", Json::str("hh-s4-slot/1")),
        ("slot_ref", Json::str(&spec.slot_ref)),
        ("task_id", Json::str(&spec.task_id)),
        ("arm_id", Json::str(&spec.arm_id)),
        ("strategy", Json::str(&spec.strategy)),
        ("round", Json::Int(spec.round)),
        ("first", Json::Bool(spec.first)),
        ("lane_id", Json::str(&spec.lane_id)),
        ("image", Json::str(&spec.image)),
        ("solution_kind", Json::str(&spec.solution_kind)),
        ("backend", Json::str("podman")),
        ("egress_profile", Json::str(&spec.egress_profile)),
        ("helper_bin", Json::str(&spec.helper_bin)),
        ("helper_variant", Json::str("hh-helper --network (R5.56 first-class; stock binary)")),
        ("policy_net_mode", Json::str(&spec.policy_net_mode)),
        ("remote_host", Json::str(&spec.remote_host)),
        (
            "declared_deps",
            Json::Arr(spec.declared_deps.iter().map(|d| Json::str(d.clone())).collect()),
        ),
        // The cell's actual network is stamped below from the admitted
        // record — never asserted ahead of the admission.
        ("cell_network", Json::str("pending")),
    ] {
        put(&mut m, k, v);
    }
    m.insert("remote_ws_syncs".into(), Json::Int(0));
    m.insert("cell_attempted".into(), Json::Bool(false));
    m.insert("calls".into(), Json::Int(0));
    m.insert("intervals".into(), Json::Int(0));
    m.insert("setup_retries".into(), Json::Int(0));
    m.insert("attempts".into(), Json::Arr(vec![]));
    m.insert("pressure_opportunity".into(), Json::Bool(false));
    m.insert("compaction_started".into(), Json::Bool(false));
    m.insert("compaction_completed".into(), Json::Bool(false));
    m.insert("triggered".into(), Json::Bool(false));
    m.insert("tokens_freed".into(), Json::Int(0));
    m.insert("forgotten_count".into(), Json::Int(0));
    m.insert("output_bytes".into(), Json::Int(0));

    let t0 = Instant::now();
    let mut wall_over_cap = false;
    fn done(m: &mut BTreeMap<String, Json>, t0: Instant, over: bool) {
        let w = t0.elapsed().as_millis() as u64;
        m.insert("wall_ms".into(), Json::Int(w as i64));
        if over || w > 1_800_000 {
            m.insert("wall_cap_breach".into(), Json::Bool(true));
        }
        println!("{}", Json::Obj(m.clone()).to_canonical_string());
    }

    if !hharness_helper::podman::is_available() || hharness_env::helper::helper_binary().is_none() {
        m.insert(
            "disposition".into(),
            Json::str("inconclusive{backend_unavailable:podman_or_helper_absent}"),
        );
        done(&mut m, t0, wall_over_cap);
        return;
    }
    sweep_hh_env_containers();
    let cell_tag = format!("{}-r{}", spec.slot_ref.replace("slot:", "s"), spec.round);

    // one setup retry on provision panic (inside the frozen ≤2/plan
    // infrastructure bound — attempt identities all retained).
    let mut env_ready: Option<(
        Store,
        String,
        Lease,
        EnvDriver,
        PathBuf,
        String,
        u64,
        Vec<(String, Json)>,
        String,
    )> = None;
    let mut attempts: Vec<String> = vec![];
    let mut syncs: u64 = 0;
    for attempt in 0..2 {
        let cell_id = format!("{cell_tag}-c{attempt}");
        attempts.push(cell_id.clone());
        let (mut store, run, lease) = open(&cell_id, &spec);
        open_scopes(&mut store, &run, &lease);
        let mut driver = EnvDriver::new(&run);
        let ws = Path::new(&spec.work_root).join("ws").join(&cell_id);
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        // Stage the task materials into the workspace (the live /work
        // mount): the oracle solution, run-tests.sh, tests/.
        let td = Path::new(&spec.task_dir);
        let sol_dst = if spec.solution_kind == "yaml" {
            "stream.sh"
        } else {
            "solution.sh"
        };
        if !spec.solution_file.is_empty() {
            let _ = std::fs::copy(Path::new(&spec.solution_file), ws.join(sol_dst));
        }
        let _ = std::fs::copy(td.join("run-tests.sh"), ws.join("run-tests.sh"));
        let tsrc = td.join("tests");
        if tsrc.is_dir() {
            let _ = std::process::Command::new("sh")
                .args([
                    "-c",
                    &format!(
                        "mkdir -p '{}' && cp -a '{}'/. '{}'",
                        ws.join("tests").display(),
                        tsrc.display(),
                        ws.join("tests").display()
                    ),
                ])
                .output();
        }
        let ws = std::fs::canonicalize(&ws).unwrap();
        // The staged tree must exist at the identical path on the
        // remote podman host before `podman create -v ws:/work`.
        if remote {
            match remote_push(&ws, &spec) {
                Ok(note) => {
                    syncs += 1;
                    m.insert(
                        "remote_ws_push".into(),
                        Json::str(&format!("attempt{attempt}:{note}")),
                    );
                }
                Err(e) => {
                    m.insert("setup_retries".into(), Json::Int(attempt as i64 + 1));
                    m.insert("cell_setup_failure".into(), Json::str(&format!("remote_push:{e}")));
                    if attempt == 1 {
                        m.insert(
                            "disposition".into(),
                            Json::str("inconclusive{cell_setup_failed:remote_push_failed}"),
                        );
                    }
                    continue;
                }
            }
        }
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ready_cell(&mut store, &lease, &mut driver, &ws, &spec)
        }));
        match res {
            Ok(Ok((env_id, enforced, echo, net))) => {
                env_ready = Some((store, run, lease, driver, ws, env_id, enforced, echo, net));
                break;
            }
            Ok(Err(e)) => {
                m.insert("setup_retries".into(), Json::Int(attempt as i64 + 1));
                m.insert("cell_setup_failure".into(), Json::str(&e));
                if attempt == 1 {
                    m.insert(
                        "disposition".into(),
                        Json::str(&format!("inconclusive{{cell_setup_failed:{e}}}")),
                    );
                }
            }
            Err(_) => {
                m.insert("setup_retries".into(), Json::Int(attempt as i64 + 1));
                if attempt == 1 {
                    m.insert(
                        "cell_setup_failure".into(),
                        Json::str("cell_setup_panicked"),
                    );
                    m.insert(
                        "disposition".into(),
                        Json::str("inconclusive{cell_setup_failed:cell_setup_panicked}"),
                    );
                }
            }
        }
    }
    m.insert("cell_attempted".into(), Json::Bool(true));
    m.insert(
        "attempts".into(),
        Json::Arr(attempts.iter().map(|s| Json::str(s)).collect()),
    );
    let Some((mut store, run, lease, mut driver, ws, env_id, enforced_decl, echo, cell_net)) =
        env_ready
    else {
        m.insert("remote_ws_syncs".into(), Json::Int(syncs as i64));
        done(&mut m, t0, wall_over_cap);
        return;
    };
    m.insert("cell_network".into(), Json::str(&cell_net));
    for (k, v) in echo {
        m.insert(k, v);
    }
    let mut pull = |m: &mut BTreeMap<String, Json>, tag: &str| {
        if remote {
            match remote_pull(&ws, &spec) {
                Ok(note) => {
                    syncs += 1;
                    let _ = note;
                }
                Err(e) => {
                    m.insert(
                        format!("remote_ws_pull_failure.{tag}"),
                        Json::str(&e),
                    );
                }
            }
        }
    };

    // ── observer/sentinel + executor ───────────────────────────────
    let ops_dir = ws.join("ops");
    let sink = MarkerSink::open(&ops_dir).unwrap();
    let sentinel = Sentinel::create(&ops_dir, b"denied").unwrap();
    let mut observer = OpsObserver::open(&ops_dir).unwrap();
    observer
        .snapshot(ObservationPoint::BeforeLaunch, &sink, &sentinel)
        .unwrap();
    m.insert("intervals".into(), Json::Int(1));
    let cap = capability();
    let bind = binding();
    let monitor = test_monitor(&cap);
    let sb = ScopeBindings::Bindings(Json::Arr(vec![]));
    let mut exec = driver.take_executor(&env_id, spec.projected_env.clone()).unwrap();
    m.insert(
        "executor_backend".into(),
        Json::str(&exec.client.backend.clone()),
    );

    // ── dispatch 1 — stage the task materials inside the cell ──────
    let stage_cmd = "mkdir -p /oracle /tests && \
        cp -a /work/solution.sh /oracle/ 2>/dev/null; \
        cp -a /work/stream.sh /oracle/ 2>/dev/null; \
        cp -a /work/tests/. /tests/ 2>/dev/null; \
        cp /work/run-tests.sh /tests/ 2>/dev/null; \
        echo staged";
    let stage_out = dispatch(
        &mut store, &monitor, &run, &mut driver, &mut exec, &cap, &bind, &sb, &env_id,
        stage_cmd, 0, "tc-stage", Some(120_000), &lease,
    );
    match &stage_out {
        Ok(s) => {
            m.insert("staging_dispatch".into(), Json::str(s));
        }
        Err(e) => {
            m.insert("staging_dispatch".into(), Json::str("error"));
            m.insert("failure".into(), Json::str(e));
            m.insert(
                "disposition".into(),
                Json::str("failed{staging_dispatch:error}"),
            );
            driver.return_executor(&env_id, exec);
            let _ = driver.teardown(&mut store, &lease, &env_id);
            done(&mut m, t0, wall_over_cap);
            return;
        }
    }
    m.insert("calls".into(), Json::Int(1));

    // ── dispatch 2 — the subject's work (the committed oracle) ─────
    // s4-30slot-repairs machinery repair: exec under the task image's
    // declared WORKDIR (verbatim from `podman image inspect`; the spec's
    // `task_workdir`) — the prior `/app` hardcode is retained verbatim
    // as the empty-spec default.
    let task_cwd = if spec.task_workdir.is_empty() {
        "/app"
    } else {
        spec.task_workdir.as_str()
    };
    m.insert("task_workdir".into(), Json::str(task_cwd));
    let work_deadline = (spec.max_agent_timeout_sec.min(1800.0) * 1000.0) as u64;
    let work_cmd = if spec.solution_kind == "yaml" {
        format!(
            "cd {task_cwd} 2>/dev/null || cd /; bash -i < /oracle/stream.sh > /work/solution.log 2>&1; \
             echo $? > /work/solution.rc"
        )
    } else {
        format!(
            "cd {task_cwd} 2>/dev/null || cd /; bash /oracle/solution.sh > /work/solution.log 2>&1; \
             echo $? > /work/solution.rc"
        )
    };
    let work_out = dispatch(
        &mut store, &monitor, &run, &mut driver, &mut exec, &cap, &bind, &sb, &env_id,
        &work_cmd, 1, "tc-work", Some(work_deadline), &lease,
    );
    m.insert("calls".into(), Json::Int(2));
    match &work_out {
        Ok(s) => {
            m.insert("work_dispatch".into(), Json::str(s));
        }
        Err(e) => {
            m.insert("work_dispatch".into(), Json::str("error"));
            m.insert("failure".into(), Json::str(e));
        }
    }
    pull(&mut m, "work");
    if let Some(b) = read_ws(&ws, "solution.rc") {
        let rc = String::from_utf8_lossy(&b).trim().to_string();
        m.insert("work_exit".into(), Json::str(&rc));
    }
    let sol_log = read_ws(&ws, "solution.log").unwrap_or_default();
    {
        let (h, n) = rget(&sol_log);
        m.insert("solution_log_sha256".into(), Json::str(&h));
        m.insert("solution_log_bytes".into(), Json::Int(n as i64));
        m.insert("output_bytes".into(), Json::Int(n as i64));
    }
    observer
        .snapshot(ObservationPoint::AtBoundary, &sink, &sentinel)
        .unwrap();
    m.insert("intervals".into(), Json::Int(2));

    // ── the arm's real compact() over the actual transcript ────────
    let eid_stage = effect_id(&run, "mc-1", "tc-stage", 0);
    let eid_work = effect_id(&run, "mc-1", "tc-work", 0);
    let cseq_stage = completed_seq(&store, &run, &eid_stage).unwrap_or(0);
    let cseq_work = completed_seq(&store, &run, &eid_work).unwrap_or(0);
    let toks = |n: usize| (n as u64 / 4).max(1);
    let tool_items: Vec<Candidate> = vec![
        cand("call-stage", toks(stage_cmd.len()), 1, Some(("test:write_file", &eid_stage))),
        cand("call-work", toks(work_cmd.len()), 2, Some(("test:write_file", &eid_work))),
        cand("result-work", toks(sol_log.len()), 3, Some(("test:write_file", &eid_work))),
    ];
    let (plan, cmap) = plan_of(&tool_items, &run);
    let occupancy = plan.occupancy_estimate;
    m.insert("input_tokens".into(), Json::Int(occupancy as i64));
    m.insert(
        "pressure_opportunity".into(),
        Json::Bool(occupancy > 400),
    );
    let input_c = CompactInput {
        plan: &plan,
        candidates: cmap,
        window_cap: 400,
        trigger: CompactionTrigger::OccupancyHard,
        needed: 0,
        target_fraction_ppm: compact::DEFAULT_TARGET_FRACTION_PPM,
        scope: PersistenceScope::Run,
        at: 20,
        run_id: run.clone(),
        summarizer: None,
        slot_min_authority: BTreeMap::new(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    };
    let eol = EvictOldest;
    let ctr = ClearToolResults {
        capabilities: ["test:write_file".to_string()].into_iter().collect(),
        older_than: u64::MAX,
        completed_invocations: [
            (eid_stage.clone(), cseq_stage),
            (eid_work.clone(), cseq_work),
        ]
        .into_iter()
        .collect(),
    };
    let variants: Vec<&dyn compact::CompactionStrategy> = if spec.strategy == "evict_oldest" {
        vec![&eol]
    } else {
        vec![&ctr]
    };
    let mut sink_ev = CollectSink::default();
    match compact::compact(&input_c, &variants, &mut sink_ev) {
        Ok(oc) => {
            m.insert(
                "compaction_started".into(),
                Json::Bool(
                    sink_ev
                        .events
                        .iter()
                        .any(|(c, _)| c == "context.compaction.started"),
                ),
            );
            let started = matches!(m.get("compaction_started"), Some(Json::Bool(true)));
            m.insert(
                "triggered".into(),
                Json::Bool(started && occupancy > 400),
            );
            let pre_items: BTreeSet<String> = plan
                .slots
                .iter()
                .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
                .collect();
            let post_items: BTreeSet<String> = oc
                .view
                .slots
                .iter()
                .flat_map(|s| s.items.iter().map(|i| i.context_item_id.clone()))
                .collect();
            let status = match oc.record.status {
                compact::CompactionStatus::Applied => "applied",
                compact::CompactionStatus::Ineffective => "ineffective",
                compact::CompactionStatus::Failed => "failed",
            };
            m.insert("compaction_status".into(), Json::str(status));
            m.insert(
                "compaction_completed".into(),
                Json::Bool(status == "applied"),
            );
            m.insert("served_variant".into(), Json::str(&oc.record.variant_ref));
            if let Some(f) = &oc.record.fallback_variant {
                m.insert("fallback_variant".into(), Json::str(f));
            }
            m.insert(
                "forgotten_count".into(),
                Json::Int(oc.record.forgotten.len() as i64),
            );
            m.insert(
                "tokens_freed".into(),
                Json::Int(oc.record.tokens_freed as i64),
            );
            m.insert(
                "request_changed".into(),
                Json::Bool(pre_items != post_items),
            );
            // durable before visible
            let mint = EventMinter::new(&store, &run);
            let mut evs = vec![];
            for (class, payload) in &sink_ev.events {
                let mut e = mint.mint(class, payload.clone()).unwrap();
                e.scope = Scope {
                    turn_id: Some("turn-1".to_string()),
                    model_call_id: Some("mc-1".to_string()),
                    ..Scope::default()
                };
                evs.push(e);
            }
            store.append(&run, &lease, evs).unwrap();
        }
        Err(e) => {
            m.insert("compaction_status".into(), Json::str(&format!("error:{e}")));
        }
    }

    // ── dispatch 3 — the native verifier (the with-next-dispatch
    //    join: it lands after the compaction rows) ─────────────────
    // The deadline derives through the sanctioned R5.57 home —
    // `verifier_deadline_ms(enforced_decl)` = min(decl, 900 s) verbatim —
    // over the spec's post-admission enforced decl (the ten astropy
    // slots carry the admitted 900; every other slot its own decl).
    let test_deadline = verifier_bound::verifier_deadline_ms(enforced_decl);
    let vcmd = format!(
        "cd {task_cwd} 2>/dev/null || cd /; bash /tests/run-tests.sh > /work/verdict_verbatim.log 2>&1; \
         echo $? > /work/verdict_verbatim.rc"
    );
    let vout = dispatch(
        &mut store, &monitor, &run, &mut driver, &mut exec, &cap, &bind, &sb, &env_id,
        &vcmd, 2, "tc-verdict", Some(test_deadline), &lease,
    );
    m.insert("calls".into(), Json::Int(3));
    match &vout {
        Ok(s) => {
            m.insert("verdict_verbatim_dispatch".into(), Json::str(s));
        }
        Err(e) => {
            m.insert("verdict_verbatim_dispatch".into(), Json::str("error"));
            m.insert("failure".into(), Json::str(e));
        }
    }
    m.insert("next_dispatch_after".into(), Json::Bool(matches!(vout, Ok(_))));

    // The verbatim channel only produced a receipt if the pytest banner
    // is present — otherwise the bootstrap died (egress-bound): run the
    // vendored offline channel (the derived image's own pytest).
    pull(&mut m, "verdict");
    let vlog = read_ws(&ws, "verdict_verbatim.log").unwrap_or_default();
    let verbatim_had_pytest = String::from_utf8_lossy(&vlog).contains("test session starts")
        || String::from_utf8_lossy(&vlog).contains("PASSED")
        || String::from_utf8_lossy(&vlog).contains("FAILED");
    m.insert(
        "verifier_verbatim_produced_pytest".into(),
        Json::Bool(verbatim_had_pytest),
    );
    if !verbatim_had_pytest {
        let v2cmd = match &spec.vendored_cmd {
            Some(c) => c.clone(),
            None => format!(
                "cd {task_cwd} 2>/dev/null || cd /; TEST_DIR=/tests python3 -m pytest {} > /work/verdict.log 2>&1; \
                 echo $? > /work/verdict.rc",
                spec.pytest_target
            ),
        };
        let v2 = dispatch(
            &mut store, &monitor, &run, &mut driver, &mut exec, &cap, &bind, &sb, &env_id,
            &v2cmd, 3, "tc-vendored", Some(test_deadline), &lease,
        );
        m.insert("calls".into(), Json::Int(4));
        match &v2 {
            Ok(s) => {
            m.insert("verdict_vendored_dispatch".into(), Json::str(s));
        }
            Err(e) => {
                m.insert("verdict_vendored_dispatch".into(), Json::str("error"));
                m.insert("failure".into(), Json::str(e));
            }
        }
        pull(&mut m, "vendored");
    }
    pull(&mut m, "final");
    m.insert("remote_ws_syncs".into(), Json::Int(syncs as i64));
    for name in ["verdict_verbatim.log", "verdict.log"] {
        if let Some(b) = read_ws(&ws, name) {
            let (h, n) = rget(&b);
            m.insert(format!("{name}.sha256"), Json::str(&h));
            m.insert(format!("{name}.bytes"), Json::Int(n as i64));
            let ob = m["output_bytes"].as_int().unwrap_or(0) + n as i64;
            m.insert("output_bytes".into(), Json::Int(ob));
        }
    }
    for name in ["verdict_verbatim.rc", "verdict.rc"] {
        if let Some(b) = read_ws(&ws, name) {
            m.insert(
                name.to_string(),
                Json::str(String::from_utf8_lossy(&b).trim()),
            );
        }
    }

    // ── probe + seal + collect ─────────────────────────────────────
    m.insert(
        "probe".into(),
        Json::str(&match exec.probe(&eid_work, 0) {
            Ok(v) => format!("ok:{}", v.as_str()),
            Err(e) => format!("error:{e}"),
        }),
    );
    driver.return_executor(&env_id, exec);
    let seals = driver.seal_workspace(&mut store, &lease);
    m.insert(
        "seal".into(),
        Json::str(
            &seals
                .iter()
                .find(|s| s.env_handle_id == env_id)
                .map(|s| s.status.clone())
                .unwrap_or_else(|| "absent".to_string()),
        ),
    );
    observer
        .snapshot(ObservationPoint::AfterQuiescence, &sink, &sentinel)
        .unwrap();
    m.insert("intervals".into(), Json::Int(3));
    m.insert("sentinel_intact".into(), Json::Bool(sentinel.intact()));
    m.insert(
        "observer_survived".into(),
        Json::Bool(observer.rows().map(|r| !r.is_empty()).unwrap_or(false)),
    );
    // collected artifacts: everything the cell wrote into /work.
    let mut collected: Vec<Json> = vec![];
    if let Ok(rd) = std::fs::read_dir(&ws) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Ok(b) = std::fs::read(&p) {
                    let (h, n) = rget(&b);
                    collected.push(Json::obj([
                        ("name", Json::str(e.file_name().to_string_lossy().as_ref())),
                        ("sha256", Json::str(&h)),
                        ("bytes", Json::Int(n as i64)),
                    ]));
                }
            }
        }
    }
    m.insert("collected".into(), Json::Arr(collected));
    let _ = driver.teardown(&mut store, &lease, &env_id);
    if m.contains_key("failure") {
        m.insert("disposition".into(), Json::str("inconclusive{drive_failure}"));
    }
    wall_over_cap = t0.elapsed().as_millis() as u64 > 1_800_000;
    done(&mut m, t0, wall_over_cap);
}
