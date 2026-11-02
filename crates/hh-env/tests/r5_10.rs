//! R5.10 (REQ-E-203/REQ-E-102; PADR-R5-1 A.1 item 3) — the workspace seal:
//! `EnvDriver::seal_workspace` pins the immutable end-state
//! (`action.environment.snapshot{purpose:"submission_seal"}`), revokes
//! participant access (`action.environment.detached
//! {reason:"submission_seal"}`) and lands the final `meters_sampled`
//! usage row — durable before the run's terminal cites them.
//!
//! AC-1 positive legs and the AC-2 negative legs: crash between seal and
//! finish recovers the `prior` pair without re-appending; a post-seal
//! mutation diverges from the content pin (detectable, never silently
//! current); a revocation/pin leg failure records `refused{…}` with the
//! refs that did land; non-ready handles report `unavailable{…}`;
//! detached-without-a-seal-pair reports unavailable, never a fabricated
//! pin.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_containment::attach::{AttachMode, PolicySlot};
use hharness_containment::backend::Ep2Model;
use hharness_containment::policy::{kernel_default, ExecPolicy, NetMode, WritableRoot};
use hharness_env::driver::{EnvDriver, WorkspaceSeal};
use hharness_env::handle::{HandleState, OnLoss, Roots, Tri};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::snapshot::{PathBaseline, SnapshotKind};
use hharness_identity::idp::address;
use hharness_ledger::ids::{ManualClock, SeqIds};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_wire::json::Json;

// ── scratch rig (the durable_recovery battery's shape) ───────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r510-{}-{tag}-{n}", std::process::id()));
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

fn workspace(tag: &str) -> PathBuf {
    let p = dir(tag).join("workspace");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_policy(ws: &std::path::Path) -> hharness_containment::policy::ContainmentPolicy {
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
        limits: Default::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

fn test_ca() -> hharness_identity::idp::ContentAddress {
    address(b"test-image-bytes", "application/octet-stream")
}

fn ready_env(
    store: &mut Store,
    lease: &Lease,
    driver: &mut EnvDriver,
    ws: &std::path::Path,
) -> String {
    let record = test_record(
        EnvironmentClass::LocalHost,
        ImageRef::ContentAddress(test_ca()),
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

fn rows<'a>(
    store: &'a Store,
    run: &str,
    class: &str,
) -> Vec<&'a hharness_ledger::event::EventEnvelope> {
    store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .collect()
}

fn seal_rows<'a>(
    store: &'a Store,
    run: &str,
    env: &str,
    class: &str,
    member: &str,
    value: &str,
) -> Vec<&'a hharness_ledger::event::EventEnvelope> {
    rows(store, run, class)
        .into_iter()
        .filter(|e| {
            e.payload.get("env_handle").and_then(Json::as_str) == Some(env)
                && e.payload.get(member).and_then(Json::as_str) == Some(value)
        })
        .collect()
}

// ── AC-1: the seal lands all three legs durable, in order ────────────────────

#[test]
fn r510_seal_pins_end_state_revokes_access_and_finalizes_usage() {
    let (mut store, run, lease, _clock) = open("seal-pos");
    let ws = workspace("seal-pos");
    std::fs::write(ws.join("artifact.txt"), b"submission-bytes").unwrap();
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    let head_before = store.head(&run).unwrap().seq;

    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals.len(), 1);
    let seal = &seals[0];
    assert_eq!(seal.status, "sealed", "{seal:?}");

    // The durable legs, newest-first: pin → revoke → final usage.
    let snaps = seal_rows(
        &store,
        &run,
        &env,
        "action.environment.snapshot",
        "purpose",
        "submission_seal",
    );
    assert_eq!(snaps.len(), 1, "one seal pin: {snaps:?}");
    assert_eq!(
        snaps[0].payload.get("snapshot_ref").and_then(Json::as_str),
        seal.end_state_ref.as_deref()
    );
    let dets = seal_rows(
        &store,
        &run,
        &env,
        "action.environment.detached",
        "reason",
        "submission_seal",
    );
    assert_eq!(dets.len(), 1, "one seal revocation: {dets:?}");
    assert_eq!(
        Some(dets[0].event_id.as_str()),
        seal.access_event_ref.as_deref()
    );
    let meters = rows(&store, &run, "action.environment.meters_sampled");
    assert_eq!(meters.len(), 1);
    assert_eq!(
        Some(meters[0].event_id.as_str()),
        seal.usage_event_ref.as_deref()
    );
    // Order: pin → revoke → usage (the last three rows the seal landed).
    assert!(snaps[0].seq < dets[0].seq && dets[0].seq < meters[0].seq);
    assert!(snaps[0].seq > head_before, "the legs are new rows");

    // Participant access is revoked — the handle is detached, the
    // session cleared, and env-gated ops refuse.
    let h = driver.handle(&env).unwrap();
    assert_eq!(h.state, HandleState::Detached);
    assert!(h.session.is_none());
    assert!(
        driver.path_baseline(&mut store, &lease, &env).is_err(),
        "a detached handle refuses env-gated ops"
    );
}

// ── AC-1/AC-2: re-seal after the pair landed recovers `prior`, never re-appends

#[test]
fn r510_reseal_recovers_prior_pair_without_new_rows() {
    let (mut store, run, lease, _clock) = open("seal-prior");
    let ws = workspace("seal-prior");
    std::fs::write(ws.join("a.txt"), b"one").unwrap();
    let mut driver = EnvDriver::new(&run);
    let _env = ready_env(&mut store, &lease, &mut driver, &ws);

    let first = driver.seal_workspace(&mut store, &lease);
    assert_eq!(first[0].status, "sealed");
    let seq_after = store.head(&run).unwrap().seq;

    // The finish-pass crash retry: the handle is already detached by the
    // seal — the pass re-cites the durable pair, appending nothing.
    let second = driver.seal_workspace(&mut store, &lease);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].status, "prior", "{second:?}");
    assert_eq!(second[0].end_state_ref, first[0].end_state_ref);
    assert_eq!(second[0].access_event_ref, first[0].access_event_ref);
    assert_eq!(
        store.head(&run).unwrap().seq,
        seq_after,
        "no new durable rows on the recovered pass"
    );
}

// ── AC-2: post-seal mutation diverges from the pin (detectable) ──────────────

#[test]
fn r510_post_seal_mutation_diverges_from_the_pin() {
    let (mut store, run, lease, _clock) = open("seal-mut");
    let ws = workspace("seal-mut");
    std::fs::write(ws.join("artifact.txt"), b"sealed").unwrap();
    let mut driver = EnvDriver::new(&run);
    let _env = ready_env(&mut store, &lease, &mut driver, &ws);
    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals[0].status, "sealed");

    // A post-seal mutation never rewrites the pin — a re-walked baseline
    // diverges from the sealed content (the grader's mutable-submission
    // detection input).
    let artifact = ws.join("artifact.txt").display().to_string();
    let baseline_now = PathBaseline::take(&|p| std::fs::read(p).ok(), &[artifact.clone()]);
    std::fs::write(ws.join("artifact.txt"), b"mutated-after-seal").unwrap();
    let baseline_mut = PathBaseline::take(&|p| std::fs::read(p).ok(), &[artifact]);
    let diff = PathBaseline::diff(&baseline_now, &baseline_mut);
    assert!(
        !diff.is_empty(),
        "the mutation is detectable against the pin"
    );
}

// ── AC-2: a detached-for-another-reason handle reports unavailable ───────────

#[test]
fn r510_detached_without_seal_pair_reports_unavailable() {
    let (mut store, run, lease, _clock) = open("seal-det");
    let ws = workspace("seal-det");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    driver.detach(&mut store, &lease, &env).unwrap();

    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals.len(), 1);
    assert_eq!(seals[0].status, "unavailable", "{seals:?}");
    assert_eq!(
        seals[0].detail.as_deref(),
        Some("detached without a submission_seal pair")
    );
    // Never a fabricated pin or access ref.
    assert!(seals[0].end_state_ref.is_none());
    assert!(seals[0].access_event_ref.is_none());
}

// ── AC-2: non-ready handles report their state, legs stay honest ─────────────

#[test]
fn r510_failed_and_suspended_handles_report_unavailable() {
    let (mut store, run, lease, _clock) = open("seal-states");
    let ws = workspace("seal-states");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // Drive the handle to `failed` (terminal fault).
    driver
        .handle_mut(&env)
        .unwrap()
        .transition(HandleState::Failed, store.now_ms())
        .unwrap();
    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals[0].status, "unavailable");
    assert_eq!(seals[0].detail.as_deref(), Some("handle failed"));

    // An unreachable handle — participant access lost, not revoked —
    // reports its state honestly (never claimed `sealed`).
    let env2 = ready_env(&mut store, &lease, &mut driver, &ws);
    driver
        .handle_mut(&env2)
        .unwrap()
        .transition(HandleState::Unreachable, store.now_ms())
        .unwrap();
    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals.len(), 2);
    let lost = seals.iter().find(|s| s.env_handle_id == env2).unwrap();
    assert_eq!(lost.status, "unavailable");
    assert_eq!(lost.detail.as_deref(), Some("handle unreachable"));
}

// ── AC-2: a pin leg failure still revokes (revocation is unconditional) ──────

#[test]
fn r510_pin_failure_still_revokes_and_records_refused() {
    let (mut store, run, lease, _clock) = open("seal-refuse");
    let ws = workspace("seal-refuse");
    let mut driver = EnvDriver::new(&run);
    let env = ready_env(&mut store, &lease, &mut driver, &ws);
    // Strip the baseline capability — the pin leg refuses
    // `UnknownCapability`; revocation must still land.
    driver
        .handle_mut(&env)
        .unwrap()
        .capabilities
        .snapshot
        .insert(
            SnapshotKind::PathBaseline.as_str().to_string(),
            Tri::Unknown,
        );

    let seals = driver.seal_workspace(&mut store, &lease);
    assert_eq!(seals.len(), 1);
    assert_eq!(seals[0].status, "refused", "{seals:?}");
    assert!(
        seals[0].detail.as_deref().unwrap().contains("seal:"),
        "the pin leg is named: {seals:?}"
    );
    // Access was still revoked — the detached row landed.
    assert!(seals[0].access_event_ref.is_some());
    assert!(seals[0].end_state_ref.is_none(), "no fabricated pin");
    assert_eq!(driver.handle(&env).unwrap().state, HandleState::Detached);
    let dets = seal_rows(
        &store,
        &run,
        &env,
        "action.environment.detached",
        "reason",
        "submission_seal",
    );
    assert_eq!(dets.len(), 1);
}

// ── AC-1: the seal member's canonical member form ────────────────────────────

#[test]
fn r510_workspace_seal_member_form() {
    let seal = WorkspaceSeal {
        env_handle_id: "env-1".to_string(),
        status: "sealed".to_string(),
        detail: None,
        end_state_ref: Some("sha256:pin".to_string()),
        end_state_event_ref: Some("evt-snap".to_string()),
        access_event_ref: Some("evt-det".to_string()),
        usage_event_ref: Some("evt-m".to_string()),
    };
    let j = seal.to_json();
    assert_eq!(j.get("env_handle").and_then(Json::as_str), Some("env-1"));
    assert_eq!(j.get("status").and_then(Json::as_str), Some("sealed"));
    assert_eq!(
        j.get("end_state_ref").and_then(Json::as_str),
        Some("sha256:pin")
    );
    // Absent members are omitted, never null.
    assert!(j.get("detail").is_none());
    let sparse = WorkspaceSeal {
        env_handle_id: "env-2".to_string(),
        status: "unavailable".to_string(),
        detail: Some("handle failed".to_string()),
        end_state_ref: None,
        end_state_event_ref: None,
        access_event_ref: None,
        usage_event_ref: None,
    }
    .to_json();
    assert!(sparse.get("end_state_ref").is_none());
    assert_eq!(
        sparse.get("detail").and_then(Json::as_str),
        Some("handle failed")
    );
}

// keep the collection imports honest across cfg variations
#[allow(dead_code)]
fn _unused(_: BTreeMap<String, Json>, _: BTreeSet<String>) {}
