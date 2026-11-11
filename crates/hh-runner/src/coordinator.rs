//! The coordinator — the sole writer of the experiment store root; it
//! drives `ExperimentEngine` (`next`/`claim`/`launch`/`settle` under the
//! spec's `ReattemptPolicy`) and keeps no scheduler, retry counter or
//! authoritative store of its own (REQ-XB-102, CC1, K-2). Slot attempts
//! run in separate slot-worker processes, each the sole writer of its own
//! slot store root (REQ-XB-104). Recovery derives only from durable
//! records (REQ-XB-105): a launched-but-unsettled attempt binds its
//! worker's sealed record, or closes `inconclusive{infra:runner_crash}`
//! and replans within bounds. A paid agent phase is never resumed mid-run
//! — a dead attempt is replanned as a NEW attempt.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use hharness_experiment::docs::LabDocs;
use hharness_experiment::engine::{EngineContext, ExperimentEngine, LaunchOutcome, NextVerdict};
use hharness_experiment::errors::ExperimentError;
use hharness_lab::expand::{ArmConfiguration, ExpandError, ExpandTask};
use hharness_lab::experiment::{ArmSpec, ExperimentSpec};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::control::{InfraError, InfraErrorFamily, OutcomeClass, StopReason};
use hharness_ontology::lab::SplitLabel;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

use crate::continuation::FpArm;
use crate::disposition::InfraCause;
use crate::packet::{DrivePacket, SlotPacket, SlotRecipe};
use crate::slot_record::RunnerIdentity;
use crate::worker::{root_digest, slot_root_for, slot_run_id};

/// The coordinator's writer-lease holder label.
pub const COORDINATOR_HOLDER: &str = "hh-runner-coordinator";
/// The subject run's writer-lease holder.
pub const SUBJECT_HOLDER: &str = "hh-runner-subject";

/// Runner errors — typed, never fabricated.
#[derive(Debug)]
pub enum RunnerError {
    /// A store/durability failure.
    Ledger(hharness_ledger::errors::LedgerError),
    /// An engine failure.
    Engine(ExperimentError),
    /// A packet/io failure.
    Io(String),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerError::Ledger(e) => write!(f, "coordinator ledger: {e}"),
            RunnerError::Engine(e) => write!(f, "coordinator engine: {e:?}"),
            RunnerError::Io(d) => write!(f, "coordinator io: {d}"),
        }
    }
}

impl std::error::Error for RunnerError {}

fn io(detail: impl Into<String>) -> RunnerError {
    RunnerError::Io(detail.into())
}

/// The drive's declared roots and identity.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// The experiment store root — this process is its sole writer.
    pub experiment_root: PathBuf,
    /// Parent dir of per-attempt slot store roots (workers own theirs).
    pub slots_dir: PathBuf,
    /// Dir for worker packet files (declared run inputs).
    pub packet_dir: PathBuf,
    /// The runner's identity stamps.
    pub runner: RunnerIdentity,
    /// In-flight worker bound (≥ 1).
    pub max_parallel: usize,
}

/// A live worker process the coordinator reaps and binds — plus the
/// slot's declared deadline (the cut-off's in-flight set, R6.16).
pub struct WorkerProcess {
    /// The attempt's plan key.
    pub run_plan_id: String,
    /// The attempt ordinal.
    pub attempt_no: u32,
    /// The slot store root the worker owns.
    pub slot_root: PathBuf,
    /// The deterministic slot run id.
    pub slot_run: String,
    /// The subject run id in the experiment store.
    pub subject_run_id: String,
    /// The subject run's live writer lease (from `launch`).
    pub subject_lease: Lease,
    /// The child process.
    pub child: Child,
}

/// The drive report — what the coordinator settled (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct DriveReport {
    /// The experiment run id.
    pub run_id: String,
    /// `run_plan_id → outcome row` in settle order.
    pub settled: Vec<(String, Json)>,
    /// The close status string.
    pub close_status: String,
    /// The `hh-runner-window/1` record when the drive ran scheduled
    /// (R6.16) — the C inputs, packing rule, actual start order,
    /// withheld/unresolved sets, catalog snapshots and the harvest
    /// manifest, blob-deposited into the experiment store.
    pub window: Option<Json>,
}

/// Build the engine's resolver context from the declared packet (deposits
/// only — no absolute paths, no hidden state).
pub fn ctx_for(packet: &DrivePacket) -> EngineContext<'static> {
    let budgets = packet.budgets.clone();
    let tasks = packet.suite_tasks.clone();
    let cfgs = packet.arm_configs.clone();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| budgets.get(r).cloned())),
        suite_tasks: Some(Box::new(move |_spec: &ExperimentSpec| {
            Some(
                tasks
                    .iter()
                    .map(|(t, s)| ExpandTask {
                        task_id: t.clone(),
                        split_label: SplitLabel::parse(s).unwrap_or(SplitLabel::Dev),
                    })
                    .collect(),
            )
        })),
        arm_config: Some(Box::new(move |a: &ArmSpec| {
            cfgs.get(&a.arm_id)
                .map(|(cid, cv)| ArmConfiguration {
                    configuration_id: cid.clone(),
                    configuration_version_id: cv.clone(),
                    budget_ref: None,
                })
                .ok_or_else(|| ExpandError::Assembly {
                    arm: a.arm_id.clone(),
                    detail: "arm_configs deposit missing".to_string(),
                })
        })),
        artifact_sealed: Some(Box::new(|_| true)),
        min_replicates: 1,
        ..Default::default()
    }
}

/// Run `op` on a `bind_lease` engine bound to `lease`, then hand the
/// (possibly renewed) lease back — `bound()` renews inside the back half
/// of the TTL and only `take_binding` surfaces the new generation.
fn with_engine<'a, R>(
    store: &'a mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    run_id: &str,
    lease: &mut Lease,
    op: impl FnOnce(&mut ExperimentEngine<'a>) -> Result<R, RunnerError>,
) -> Result<R, RunnerError> {
    let mut eng = bound_engine(store, docs, packet, run_id, lease);
    let r = op(&mut eng);
    if let Some((_, l)) = eng.take_binding() {
        *lease = l;
    }
    r
}

fn bound_engine<'a>(
    store: &'a mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    run_id: &str,
    lease: &Lease,
) -> ExperimentEngine<'a> {
    ExperimentEngine::bind_lease(
        store,
        docs.clone(),
        ctx_for(packet),
        run_id.to_string(),
        lease.clone(),
    )
    .with_ttls(packet.writer_ttl_ms, packet.claim_ttl_ms)
}

fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-runner/1"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap_or_default(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-runner/1", store.now_ms())),
        content_kind: None,
        payload,
    }
}

fn append(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    mut batch: Vec<Event>,
) -> Result<(), RunnerError> {
    let mut parent = store.head_event_id(run_id).map_err(RunnerError::Ledger)?;
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store
        .append(run_id, lease, batch)
        .map_err(RunnerError::Ledger)?;
    Ok(())
}

/// `open_or_attach(store, docs, packet) → (experiment_id, run_id, lease)` —
/// register/expand are idempotent; the open-vs-attach choice reads the
/// durable index entry, never a runner-side flag. The returned writer
/// lease stays caller-owned across the whole drive (bind_lease engines
/// never re-acquire).
pub fn open_or_attach(
    store: &mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
) -> Result<(String, String, Lease), RunnerError> {
    let mut eng = ExperimentEngine::new(store, docs.clone(), ctx_for(packet))
        .with_holder(COORDINATOR_HOLDER)
        .with_ttls(packet.writer_ttl_ms, packet.claim_ttl_ms);
    let eid = eng.register(&packet.spec).map_err(RunnerError::Engine)?;
    eng.expand(&eid).map_err(RunnerError::Engine)?;
    let opened = docs
        .index_entry(&eid)
        .map_err(RunnerError::Engine)?
        .and_then(|e| e.run_id);
    let run_id = match opened {
        Some(_) => eng.attach(&eid).map_err(RunnerError::Engine)?,
        None => eng.open_experiment(&eid).map_err(RunnerError::Engine)?,
    };
    let (_, lease) = eng.take_binding().ok_or_else(|| io("no writer binding"))?;
    Ok((eid, run_id, lease))
}

/// Read a sealed slot attempt from its durable slot store: the slot run's
/// `lifecycle.run.finished` payload + the post-seal head hash. `None` when
/// the run was never sealed — the durable record decides, never a
/// file-existence skip (REQ-XB-105).
pub fn sealed_attempt(
    slot_root: &Path,
    slot_run: &str,
) -> Result<Option<(Json, String)>, RunnerError> {
    let store = Store::open(slot_root).map_err(RunnerError::Ledger)?;
    if !store.has_run(slot_run) {
        return Ok(None);
    }
    let events = store.events(slot_run).map_err(RunnerError::Ledger)?;
    let finished = events
        .iter()
        .find(|e| e.class == "lifecycle.run.finished" && e.payload.get("slot_record_ref").is_some())
        .cloned();
    Ok(finished.map(|f| (f.payload, f.hash)))
}

/// `bind_or_crash` — the one durable-derived close for an in-flight
/// attempt: a sealed record binds (stop_reason/outcome copied verbatim
/// into the subject finished row, the post-seal slot-store head named);
/// anything else closes `infrastructure_failure{runner_crash}` — never a
/// resume of a paid agent phase mid-run.
pub fn bind_or_crash(
    store: &mut Store,
    subject_run_id: &str,
    slot_root: &Path,
    slot_run: &str,
    writer_ttl_ms: u64,
) -> Result<(), RunnerError> {
    let writer = store
        .acquire_writer(SUBJECT_HOLDER, subject_run_id, writer_ttl_ms)
        .map_err(RunnerError::Ledger)?;
    bind_subject(store, subject_run_id, slot_root, slot_run, &writer)
}

/// `bind_subject` — [`bind_or_crash`] under a writer lease the caller
/// already holds (the `launch`-armed `subject_writer`; never re-acquired
/// while live — a re-acquire under a live lease is the same `WouldBlock`
/// fence a foreign holder sees).
pub fn bind_subject(
    store: &mut Store,
    subject_run_id: &str,
    slot_root: &Path,
    slot_run: &str,
    writer: &Lease,
) -> Result<(), RunnerError> {
    let sealed = sealed_attempt(slot_root, slot_run)?;
    // Mirror the slot run's declared consumption into the subject run's
    // own accounting (a verbatim fold of durable rows, X-04).
    let mut batch: Vec<Event> = Vec::new();
    if sealed.is_some() {
        let slot_store = Store::open(slot_root).map_err(RunnerError::Ledger)?;
        for e in slot_store
            .events(slot_run)
            .map_err(RunnerError::Ledger)?
            .iter()
        {
            if e.class != "control.budget.consumed" {
                continue;
            }
            if let (Some(d), Some(a)) = (
                e.payload.get("dimension").and_then(Json::as_str),
                e.payload.get("amount").and_then(Json::as_int),
            ) {
                batch.push(mint(
                    store,
                    subject_run_id,
                    "control.budget.consumed",
                    Json::obj([("dimension", Json::str(d)), ("amount", Json::Int(a))]),
                ));
            }
        }
    }
    let payload = match sealed {
        Some((slot_finished, head_hash)) => {
            let mut p = slot_finished;
            if let Json::Obj(ref mut m) = p {
                m.insert(
                    "slot_store".into(),
                    Json::obj([
                        ("root_digest", Json::str(root_digest(slot_root))),
                        ("run_id", Json::str(slot_run)),
                        ("head_hash", Json::str(head_hash)),
                    ]),
                );
            }
            p
        }
        None => Json::obj([
            (
                "stop_reason",
                StopReason::InfrastructureFailure {
                    error_class: InfraError {
                        family: InfraErrorFamily::Kernel,
                        class: InfraCause::RunnerCrash.as_str(),
                    },
                }
                .to_json(),
            ),
            (
                "outcome_class",
                Json::str(OutcomeClass::InfrastructureFailure.as_str()),
            ),
            ("oracle_failed", Json::Bool(false)),
            ("disposition", Json::str("inconclusive{infra:runner_crash}")),
            (
                "slot_store",
                Json::obj([
                    ("root_digest", Json::str(root_digest(slot_root))),
                    ("run_id", Json::str(slot_run)),
                    ("head_hash", Json::str("unsealed")),
                ]),
            ),
        ]),
    };
    batch.push(mint(
        store,
        subject_run_id,
        "lifecycle.run.finished",
        payload,
    ));
    append(store, subject_run_id, writer, batch)
}

/// `recover` — the restart pass: every launched-but-unsettled attempt is
/// closed from durable records only (bind sealed, else runner_crash →
/// engine replans). Live-but-stale claims expire through the engine's own
/// reconcile on the next `next`.
pub fn recover(
    store: &mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    cfg: &CoordinatorConfig,
    eid: &str,
    run_id: &str,
    lease: &mut Lease,
) -> Result<Vec<String>, RunnerError> {
    let inflight: Vec<(String, String, u32)> = {
        let view = with_engine(store, docs, packet, run_id, lease, |eng| {
            eng.project().map_err(RunnerError::Engine)
        })?;
        let _ = eid;
        view.plans
            .values()
            .filter_map(|ps| {
                ps.in_flight()
                    .map(|a| (ps.run_plan_id.clone(), a.run_id.clone(), a.attempt_no))
            })
            .collect()
    };
    let mut recovered = Vec::new();
    for (run_plan_id, subject_run_id, attempt_no) in inflight {
        let slot_root = slot_root_for(&cfg.slots_dir, &run_plan_id, attempt_no);
        bind_or_crash(
            store,
            &subject_run_id,
            &slot_root,
            &slot_run_id(&run_plan_id, attempt_no),
            packet.writer_ttl_ms,
        )?;
        with_engine(store, docs, packet, run_id, lease, |eng| {
            eng.settle(&run_plan_id).map_err(RunnerError::Engine)
        })?;
        recovered.push(run_plan_id);
    }
    Ok(recovered)
}

/// The packet for one attempt's worker (deterministic fields only).
#[allow(clippy::too_many_arguments)]
pub fn slot_packet(
    packet: &DrivePacket,
    cfg: &CoordinatorConfig,
    eid: &str,
    run_plan_id: &str,
    cell_id: &str,
    arm_id: &str,
    task_id: &str,
    replicate_index: u32,
    split_label: &str,
    launch: &LaunchOutcome,
    recipe: SlotRecipe,
) -> SlotPacket {
    SlotPacket {
        run_plan_id: run_plan_id.to_string(),
        subject_run_id: launch.run_id.clone(),
        attempt_no: launch.attempt_no,
        experiment_id: eid.to_string(),
        protocol_id: packet.protocol_id.clone(),
        selection_id: packet.selection_id.clone(),
        cell_id: cell_id.to_string(),
        arm_id: arm_id.to_string(),
        task_id: task_id.to_string(),
        replicate_index,
        split_label: split_label.to_string(),
        slot_root: slot_root_for(&cfg.slots_dir, run_plan_id, launch.attempt_no)
            .to_string_lossy()
            .into_owned(),
        slot_run_id: slot_run_id(run_plan_id, launch.attempt_no),
        task: {
            let mut m = BTreeMap::new();
            m.insert(
                "dataset".to_string(),
                Json::obj([
                    ("task_digest", Json::str(format!("fixture:{task_id}"))),
                    ("hub_ref", Json::str("fixture")),
                ]),
            );
            m.insert("epoch".to_string(), Json::str(&packet.epoch));
            m
        },
        participant: {
            let mut m = BTreeMap::new();
            m.insert(
                "participant_ref".to_string(),
                Json::str(format!("fixture:{arm_id}")),
            );
            // R6.41 (REQ-XE-126/REQ-XE-121; E2-T8 (b)) — a declared
            // HH-native arm stamps `kind: native`, the `topology` the
            // rung-conditional gate checked, and the REQ-XE-126 `hir`
            // identity set verbatim; every other arm stays the fixture
            // `oracle` kind (absent-not-null — byte-identical records).
            match packet
                .native_participants
                .get(arm_id)
                .filter(|_| FpArm::parse(arm_id).is_some_and(|a| a.native_lane()))
            {
                Some(decl) => {
                    m.insert("kind".to_string(), Json::str(crate::native::NATIVE_KIND));
                    m.insert("topology".to_string(), decl.topology.to_json());
                    m.insert("hir".to_string(), decl.hir.clone());
                }
                None => {
                    m.insert("kind".to_string(), Json::str("oracle"));
                }
            }
            m
        },
        cell: {
            let mut m = BTreeMap::new();
            m.insert(
                "egress".to_string(),
                Json::obj([
                    ("profile", Json::str("allowlist_mediated")),
                    ("cell_network", Json::str("none")),
                    ("policy_net_mode", Json::str("mediated")),
                ]),
            );
            m.insert(
                "backend".to_string(),
                Json::obj([("executor", Json::str(crate::worker::FIXTURE_EXECUTOR))]),
            );
            m
        },
        harbor: packet.harbor.clone(),
        recipe,
    }
}

/// Spawn one slot-worker process on a packet file (process-level
/// concurrency — REQ-XB-104). The coordinator never writes the worker's
/// slot root.
pub fn spawn_worker(
    hh_runner: &Path,
    packet: &SlotPacket,
    packet_path: &Path,
    launch: &LaunchOutcome,
) -> Result<WorkerProcess, RunnerError> {
    std::fs::write(
        packet_path,
        packet.to_json().to_canonical_string().as_bytes(),
    )
    .map_err(|e| io(format!("write packet: {e}")))?;
    let child = Command::new(hh_runner)
        .arg("slot-worker")
        .arg("--packet")
        .arg(packet_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| io(format!("spawn hh-runner slot-worker: {e}")))?;
    Ok(WorkerProcess {
        run_plan_id: packet.run_plan_id.clone(),
        attempt_no: packet.attempt_no,
        slot_root: PathBuf::from(&packet.slot_root),
        slot_run: packet.slot_run_id.clone(),
        subject_run_id: launch.run_id.clone(),
        subject_lease: launch.subject_writer.clone(),
        child,
    })
}

/// Reap one attempt: durable-derived bind-or-crash, then engine settle —
/// recorded into `settled`.
#[allow(clippy::too_many_arguments)]
fn finish_attempt(
    store: &mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    eid: &str,
    run_id: &str,
    lease: &mut Lease,
    run_plan_id: &str,
    slot_root: &Path,
    slot_run: &str,
    subject_run_id: &str,
    subject_lease: &Lease,
    settled: &mut Vec<(String, Json)>,
) -> Result<(), RunnerError> {
    let _ = (eid, run_id);
    // The subject run id and its live writer lease come from the launch
    // record — bind under them (a re-acquire under the live lease would
    // read as the same WouldBlock fence a foreign holder sees). When the
    // armed lease has already expired on this clock — a worker that
    // outlived the manifest's writer TTL — the owned takeover is the
    // same fenced `acquire_writer` `bind_or_crash` uses (an expired
    // lease never binds; the takeover is audited, never bypassed).
    if subject_lease.expires_at_ms > store.now_ms() {
        bind_subject(store, subject_run_id, slot_root, slot_run, subject_lease)?;
    } else {
        bind_or_crash(
            store,
            subject_run_id,
            slot_root,
            slot_run,
            packet.writer_ttl_ms,
        )?;
    }
    let out = with_engine(store, docs, packet, run_id, lease, |eng| {
        eng.settle(run_plan_id).map_err(RunnerError::Engine)
    })?;
    settled.push((
        run_plan_id.to_string(),
        Json::obj([
            ("attempt_no", Json::Int(out.attempt_no as i64)),
            ("outcome_class", Json::str(out.outcome_class.as_str())),
            ("accepted", Json::Bool(out.accepted)),
            ("replanned", Json::Bool(out.replanned)),
            ("superseded", Json::Bool(out.superseded)),
            ("plan_final", Json::Bool(out.plan_final)),
        ]),
    ));
    Ok(())
}

/// `drive(store, docs, packet, cfg, hh_runner)` — the whole coordinator
/// loop: resume from durable records, then dispatch
/// `next → claim → launch → worker → bind → settle` until the engine
/// reports `Done`, then `close`.
pub fn drive(
    store: &mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    cfg: &CoordinatorConfig,
    hh_runner: &Path,
) -> Result<DriveReport, RunnerError> {
    if packet.window.is_some() {
        return drive_window(store, docs, packet, cfg, hh_runner, &mut || {});
    }
    let (eid, run_id, mut lease) = open_or_attach(store, docs, packet)?;
    // Resume derives only from durable records: close every launched-but-
    // unsettled attempt before dispatching new work.
    recover(store, docs, packet, cfg, &eid, &run_id, &mut lease)?;
    let plan_facts: BTreeMap<String, (String, String, String, u32, String)> = {
        with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
            eng.project().map_err(RunnerError::Engine)
        })?
        .plans
        .iter()
        .map(|(id, ps)| {
            (
                id.clone(),
                (
                    ps.cell_id.clone(),
                    ps.arm_id.clone(),
                    ps.task_id.clone(),
                    ps.replicate_index,
                    ps.split_label.clone(),
                ),
            )
        })
        .collect()
    };
    let mut settled: Vec<(String, Json)> = Vec::new();
    let mut workers: Vec<WorkerProcess> = Vec::new();
    std::fs::create_dir_all(&cfg.packet_dir).map_err(|e| io(format!("mkdir: {e}")))?;
    loop {
        // Reap exited workers — the durable record decides bind vs crash.
        let mut i = 0;
        while i < workers.len() {
            match workers[i].child.try_wait() {
                Ok(Some(_status)) => {
                    let w = workers.remove(i);
                    finish_attempt(
                        store,
                        docs,
                        packet,
                        &eid,
                        &run_id,
                        &mut lease,
                        &w.run_plan_id,
                        &w.slot_root,
                        &w.slot_run,
                        &w.subject_run_id,
                        &w.subject_lease,
                        &mut settled,
                    )?;
                }
                Ok(None) => i += 1,
                Err(e) => return Err(io(format!("wait: {e}"))),
            }
        }
        let verdict = if workers.len() < cfg.max_parallel {
            with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
                eng.next().map_err(RunnerError::Engine)
            })?
        } else {
            NextVerdict::Wait
        };
        match verdict {
            NextVerdict::Plan { run_plan_id } => {
                let launch = with_engine(
                    store,
                    docs,
                    packet,
                    &run_id,
                    &mut lease,
                    |eng| -> Result<LaunchOutcome, RunnerError> {
                        let ticket = eng
                            .claim(&run_plan_id, COORDINATOR_HOLDER)
                            .map_err(RunnerError::Engine)?;
                        eng.launch(&ticket, SUBJECT_HOLDER)
                            .map_err(RunnerError::Engine)
                    },
                );
                let launch = match launch {
                    Ok(l) => l,
                    // A live claim lease on the plan fences a re-claim until
                    // its TTL lapses — the durable record decides when the
                    // dead/stale holder is reclaimable; the runner waits the
                    // fence out, never retries around it (K-2: no second
                    // claim bookkeeping).
                    Err(RunnerError::Engine(ExperimentError::WouldBlock { .. })) => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                let (cell_id, arm_id, task_id, rep, split) =
                    plan_facts.get(&run_plan_id).cloned().unwrap_or_else(|| {
                        (String::new(), String::new(), String::new(), 0, "dev".into())
                    });
                let recipe = packet
                    .recipes
                    .get(&task_id)
                    .cloned()
                    .unwrap_or(SlotRecipe::Scored {
                        reward_ppm: 1_000_000,
                        end_cause: crate::disposition::EndCause::Exited(0),
                        consumed_model_calls: 1,
                    });
                let sp = slot_packet(
                    packet,
                    cfg,
                    &eid,
                    &run_plan_id,
                    &cell_id,
                    &arm_id,
                    &task_id,
                    rep,
                    &split,
                    &launch,
                    recipe,
                );
                let packet_path = cfg.packet_dir.join(format!(
                    "slot-{}-a{}.json",
                    slot_run_id(&run_plan_id, launch.attempt_no),
                    launch.attempt_no
                ));
                workers.push(spawn_worker(hh_runner, &sp, &packet_path, &launch)?);
            }
            NextVerdict::Done | NextVerdict::BudgetExhausted if workers.is_empty() => break,
            NextVerdict::Done
            | NextVerdict::BudgetExhausted
            | NextVerdict::Wait
            | NextVerdict::Backoff { .. } => {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
    }
    let close_status = {
        let mut eng = bound_engine(store, docs, packet, &run_id, &lease);
        eng.close(false)
            .map_err(RunnerError::Engine)?
            .status
            .as_str()
            .to_string()
    };
    Ok(DriveReport {
        run_id,
        settled,
        close_status,
        window: None,
    })
}

// ── R6.16 Stage 1 — the scheduled window drive ─────────────────────────

/// `mark_unadmitted` — the dead-man/budget resolution rows: a
/// never-attempted open plan is `withheld{<reason>}`; an attempted one
/// (a gated retry) is `unresolved` (REQ-XH-125 — the slot keeps its
/// last attempt's inconclusive disposition; no retry is carried to a
/// later window). Returns `(withheld_rows, unresolved_rows, newly
/// closed ids)` — the caller lands them in the window record.
fn mark_unadmitted(
    units: &[crate::schedule::PackingUnit],
    closed: &BTreeSet<String>,
    attempted: &BTreeSet<String>,
    inflight: &BTreeSet<String>,
    reason: &str,
) -> (Vec<Json>, Vec<Json>, Vec<String>) {
    let mut w = Vec::new();
    let mut u = Vec::new();
    let mut newly = Vec::new();
    for unit in units {
        for id in &unit.members {
            if closed.contains(id) || inflight.contains(id.as_str()) {
                continue;
            }
            if attempted.contains(id) {
                u.push(Json::obj([
                    ("run_plan_id", Json::str(id)),
                    ("disposition", Json::str("unresolved")),
                    ("cause", Json::str(format!("retry gated by {reason}"))),
                ]));
            } else {
                w.push(Json::obj([
                    ("run_plan_id", Json::str(id)),
                    ("disposition", Json::str(format!("withheld{{{reason}}}"))),
                ]));
            }
            newly.push(id.clone());
        }
    }
    (w, u, newly)
}

/// `drive_window` — the declared-window drive (R6.16; REQ-XB-121/127/
/// 128/214, REQ-XH-016/017, REQ-XD-060/111, REQ-XH-125): the schedule
/// kinds order packing units (a coupled pair launches in one step,
/// never split); first-fit admits under the declared bounds +
/// lookahead and the C rule; the dead-man cut-off withholds what it
/// cannot admit — never-attempted plans `withheld{window_deadline}`,
/// an attempted replan gated past its window `unresolved`; every
/// settled slot lands in the incremental harvest manifest; the sweep
/// removes only own-labelled containers with a teardown receipt per
/// container. `step_hook` fires at the top of every loop turn — the
/// deterministic seam a simulated clock advances through (production
/// passes `&mut || {}`).
pub fn drive_window(
    store: &mut Store,
    docs: &LabDocs,
    packet: &DrivePacket,
    cfg: &CoordinatorConfig,
    hh_runner: &Path,
    step_hook: &mut dyn FnMut(),
) -> Result<DriveReport, RunnerError> {
    let window = packet
        .window
        .clone()
        .ok_or_else(|| io("drive_window requires a declared window"))?;
    let (eid, run_id, mut lease) = open_or_attach(store, docs, packet)?;
    recover(store, docs, packet, cfg, &eid, &run_id, &mut lease)?;
    // Stage 2 — the declared supervision: fold prior ledgers read-only
    // (a refusal stops the window before either process spawns), then
    // the proxy on its Json control channel and the mediator on its
    // ready handshake — each a separate sole-writer process.
    let mut sup = packet
        .supervision
        .as_ref()
        .map(|sd| {
            let pbin = crate::supervision::resolve_bin(
                sd.proxy.bin.as_deref(),
                crate::supervision::PROXY_BIN_ENV,
                crate::supervision::PROXY_BIN_NAME,
            )
            .map_err(|e| io(format!("proxy bin: {}", e.detail)))?;
            let mbin = sd
                .mediator
                .as_ref()
                .map(|md| {
                    crate::supervision::resolve_bin(
                        md.bin.as_deref(),
                        crate::supervision::MEDIATOR_BIN_ENV,
                        crate::supervision::MEDIATOR_BIN_NAME,
                    )
                    .map_err(|e| io(format!("mediator bin: {}", e.detail)))
                })
                .transpose()?;
            crate::supervision::Supervision::open(sd, &pbin, mbin.as_deref())
                .map_err(|e| io(format!("supervision open: {}", e.detail)))
        })
        .transpose()?;
    if let Some(s) = &mut sup {
        s.boundary("window_open");
    }
    let facts: Vec<crate::schedule::PlanFact> = {
        let view = with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
            eng.project().map_err(RunnerError::Engine)
        })?;
        let mut v: Vec<crate::schedule::PlanFact> = view
            .plans
            .values()
            .map(|ps| crate::schedule::PlanFact {
                run_plan_id: ps.run_plan_id.clone(),
                cell_id: ps.cell_id.clone(),
                arm_id: ps.arm_id.clone(),
                task_id: ps.task_id.clone(),
                replicate_index: ps.replicate_index,
                split_label: ps.split_label.clone(),
                order_pos: ps.order_pos,
            })
            .collect();
        v.sort_by_key(|p| p.order_pos);
        v
    };
    let fact = |id: &str| facts.iter().find(|f| f.run_plan_id == id);
    let units = crate::schedule::build_schedule(&facts, &window.schedule);
    let c = (cfg.max_parallel as u64).min(window.concurrency_c()).max(1) as usize;
    let bounds = crate::schedule::PackingBounds {
        memory_mb: window
            .host_memory_mb
            .saturating_sub(window.memory_reserve_mb),
        cpus: window.vcpu_budget,
        model_pool: window.model_in_flight_limit,
    };
    let slot_use = crate::schedule::ResourceUse {
        memory_mb: window.slot_memory_mb,
        cpus: window.slot_cpus,
        model_slots: u64::from(window.agent_phase),
    };
    let mut settled: Vec<(String, Json)> = Vec::new();
    let mut workers: Vec<WorkerProcess> = Vec::new();
    let mut closed: BTreeSet<String> = BTreeSet::new();
    let mut attempted: BTreeSet<String> = BTreeSet::new();
    let mut start_order: Vec<Json> = Vec::new();
    let mut withheld: Vec<Json> = Vec::new();
    let mut unresolved: Vec<Json> = Vec::new();
    let mut snapshots: Vec<Json> = vec![Json::obj([
        ("seq", Json::Int(0)),
        ("boundary", Json::str("window_open")),
        ("block", Json::Int(0)),
        ("catalog_id", Json::str(&window.schedule.catalog_id)),
    ])];
    let mut harvest = crate::schedule::HarvestManifest::default();
    let manifest_path = cfg.packet_dir.join("harvest-manifest.json");
    let mut open_in_block: BTreeMap<u32, u64> = BTreeMap::new();
    for u in &units {
        *open_in_block.entry(u.block).or_insert(0) += u.members.len() as u64;
    }
    let mut budget_stop = false;
    std::fs::create_dir_all(&cfg.packet_dir).map_err(|e| io(format!("mkdir: {e}")))?;
    let total_plans: usize = units.iter().map(|u| u.members.len()).sum();

    loop {
        step_hook();
        let now = store.now_ms();
        // Reap exited workers — the durable record decides bind vs crash.
        let mut i = 0;
        while i < workers.len() {
            match workers[i].child.try_wait() {
                Ok(Some(_status)) => {
                    let w = workers.remove(i);
                    finish_attempt(
                        store,
                        docs,
                        packet,
                        &eid,
                        &run_id,
                        &mut lease,
                        &w.run_plan_id,
                        &w.slot_root,
                        &w.slot_run,
                        &w.subject_run_id,
                        &w.subject_lease,
                        &mut settled,
                    )
                    .map_err(|e| io(format!("finish_attempt {}: {e:?}", w.run_plan_id)))?;
                    // Slot close — revoke the attempt's token
                    // (`security.credential.revoked`; X-05: no token
                    // outlives its slot, replanned or not).
                    if let Some(s) = &mut sup {
                        s.slot_close(&w.slot_run)
                            .map_err(|e| io(format!("slot_close: {}", e.detail)))?;
                    }
                    let row = &settled.last().map(|(_, r)| r.clone()).unwrap_or(Json::Null);
                    let replanned = row
                        .get("replanned")
                        .and_then(crate::json_ext::JsonExt::as_bool)
                        .unwrap_or(false);
                    harvest.record(
                        &w.run_plan_id,
                        &w.slot_run,
                        w.attempt_no,
                        row.get("outcome_class")
                            .and_then(Json::as_str)
                            .unwrap_or(""),
                        None,
                    );
                    let _ = harvest.persist(&manifest_path, &window.window_id, None);
                    if !replanned {
                        closed.insert(w.run_plan_id.clone());
                        if let Some(u) = units.iter().find(|u| u.members.contains(&w.run_plan_id)) {
                            let b = u.block;
                            if let Some(v) = open_in_block.get_mut(&b) {
                                *v = v.saturating_sub(1);
                                if *v == 0 {
                                    if let Some(s) = &mut sup {
                                        s.boundary(&format!("block_end:{b}"));
                                    }
                                    snapshots.push(Json::obj([
                                        ("seq", Json::Int(snapshots.len() as i64)),
                                        ("boundary", Json::str("block_end")),
                                        ("block", Json::Int(b as i64)),
                                        ("catalog_id", Json::str(&window.schedule.catalog_id)),
                                    ]));
                                }
                            }
                        }
                    }
                }
                Ok(None) => i += 1,
                Err(e) => return Err(io(format!("wait: {e}"))),
            }
        }
        if closed.len() >= total_plans && workers.is_empty() {
            break;
        }
        // Admission — first-fit over the units' schedule order. A unit
        // starts when every open member is engine-eligible now (a pair
        // is admitted together or withheld together, never split).
        if !budget_stop && (workers.len()) < c {
            let inflight: BTreeSet<String> =
                workers.iter().map(|w| w.run_plan_id.clone()).collect();
            let view = with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
                eng.project().map_err(RunnerError::Engine)
            })?;
            let mut cands: Vec<(usize, Vec<String>)> = Vec::new();
            for (ui, u) in units.iter().enumerate() {
                let open: Vec<String> = u
                    .members
                    .iter()
                    .filter(|id| !closed.contains(*id) && !inflight.contains(id.as_str()))
                    .cloned()
                    .collect();
                if open.is_empty() {
                    continue;
                }
                let all_eligible = open.iter().all(|id| {
                    view.plans
                        .get(id)
                        .map(|ps| ps.open_at(now))
                        .unwrap_or(false)
                });
                if all_eligible {
                    cands.push((ui, open));
                }
            }
            let uses: Vec<crate::schedule::ResourceUse> = cands
                .iter()
                .map(|(_, m)| crate::schedule::ResourceUse {
                    memory_mb: slot_use.memory_mb.saturating_mul(m.len() as u64),
                    cpus: slot_use.cpus.saturating_mul(m.len() as u64),
                    model_slots: slot_use.model_slots.saturating_mul(m.len() as u64),
                })
                .collect();
            let used = crate::schedule::ResourceUse {
                memory_mb: slot_use.memory_mb.saturating_mul(workers.len() as u64),
                cpus: slot_use.cpus.saturating_mul(workers.len() as u64),
                model_slots: slot_use.model_slots.saturating_mul(workers.len() as u64),
            };
            let picks = crate::schedule::first_fit(&uses, &used, &bounds, window.lookahead);
            for pi in picks {
                if workers.len() >= c {
                    break;
                }
                let (ui, members) = &cands[pi];
                // The dead-man cut-off (REQ-XB-128): in-flight slots
                // carry the declared per-slot deadline; the candidate
                // joins the set inside `admit`.
                let dl: Vec<u64> = workers.iter().map(|_| window.slot_deadline_ms).collect();
                if !window.admit(now, dl) {
                    let (w, u2, newly) =
                        mark_unadmitted(&units, &closed, &attempted, &inflight, "window_deadline");
                    withheld.extend(w);
                    unresolved.extend(u2);
                    closed.extend(newly);
                    break;
                }
                // R6.41 (REQ-XD-102; contributes REQ-XH-114/REQ-XD-220)
                // — the L1-conformance schedule gate: a packing unit
                // carrying a native-lane member (a paid L2 pair's H
                // side, or the HH-native arm itself) admits only when
                // the declared `hh-l1-conformance/1` pass id matches the
                // arm's exact H version (`participant.hir` →
                // `{version: definition_version_id, digest:
                // kernel_binary_sha256}` — any H change invalidates the
                // match) **and** its declared topology passes the
                // recorded C-05 rung (rung 3 without an admitted AR-04
                // amendment refuses the same way — X-01). The pair is
                // atomic: the whole unit is `withheld{l1_not_passed}`,
                // never split.
                let l1_blocked: Vec<String> = members
                    .iter()
                    .filter(|id| {
                        fact(id)
                            .and_then(|f| FpArm::parse(&f.arm_id))
                            .is_some_and(|a| a.native_lane())
                    })
                    .filter(|id| {
                        let arm = fact(id).map(|f| f.arm_id.clone()).unwrap_or_default();
                        let decl = packet.native_participants.get(&arm);
                        match (packet.l1_gate.as_ref(), decl) {
                            (Some(g), Some(d)) => {
                                d.topology.check().is_err()
                                    || !crate::native::l1_gate_match(g, &d.hir)
                            }
                            _ => true,
                        }
                    })
                    .cloned()
                    .collect();
                if !l1_blocked.is_empty() {
                    for id in members {
                        withheld.push(Json::obj([
                            ("run_plan_id", Json::str(id)),
                            (
                                "disposition",
                                Json::str(crate::native::l1_not_passed_disposition().as_str()),
                            ),
                            ("unit", Json::Int(*ui as i64)),
                        ]));
                        closed.insert(id.clone());
                    }
                    continue;
                }
                // One packing step — claim every member, then launch.
                let step: Result<Vec<(String, LaunchOutcome)>, RunnerError> =
                    with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
                        let mut tickets = Vec::new();
                        for id in members {
                            let t = eng
                                .claim(id, COORDINATOR_HOLDER)
                                .map_err(RunnerError::Engine)?;
                            tickets.push((id.clone(), t));
                        }
                        let mut ls = Vec::new();
                        for (id, t) in tickets {
                            let l = eng
                                .launch(&t, SUBJECT_HOLDER)
                                .map_err(RunnerError::Engine)?;
                            ls.push((id, l));
                        }
                        Ok(ls)
                    });
                let launches = match step {
                    Ok(ls) => ls,
                    Err(RunnerError::Engine(ExperimentError::InsufficientBudget { .. })) => {
                        budget_stop = true;
                        let (w, u2, newly) =
                            mark_unadmitted(&units, &closed, &attempted, &inflight, "budget");
                        withheld.extend(w);
                        unresolved.extend(u2);
                        closed.extend(newly);
                        break;
                    }
                    Err(RunnerError::Engine(
                        ExperimentError::WouldBlock { .. }
                        | ExperimentError::PoolExhausted { .. }
                        | ExperimentError::BadPlanState { .. },
                    )) => break, // blocked — try again next turn
                    Err(e) => return Err(e),
                };
                for (id, launch) in launches {
                    let f = fact(&id).cloned().unwrap_or(crate::schedule::PlanFact {
                        run_plan_id: id.clone(),
                        cell_id: String::new(),
                        arm_id: String::new(),
                        task_id: String::new(),
                        replicate_index: 0,
                        split_label: "dev".into(),
                        order_pos: 0,
                    });
                    let recipe =
                        packet
                            .recipes
                            .get(&f.task_id)
                            .cloned()
                            .unwrap_or(SlotRecipe::Scored {
                                reward_ppm: 1_000_000,
                                end_cause: crate::disposition::EndCause::Exited(0),
                                consumed_model_calls: 1,
                            });
                    let mut sp = slot_packet(
                        packet,
                        cfg,
                        &eid,
                        &id,
                        &f.cell_id,
                        &f.arm_id,
                        &f.task_id,
                        f.replicate_index,
                        &f.split_label,
                        &launch,
                        recipe,
                    );
                    sp.cell.insert(
                        "schedule".to_string(),
                        Json::obj([
                            ("unit", Json::Int(*ui as i64)),
                            ("block", Json::Int(units[*ui].block as i64)),
                            ("kind", Json::str(window.schedule.kind.as_str())),
                            ("start_order_pos", Json::Int(start_order.len() as i64)),
                        ]),
                    );
                    // Slot open — the fresh per-attempt proxy token and
                    // the phase-granted sockets land in the slot's cell
                    // record (the upstream credential never enters it).
                    let slot_run = slot_run_id(&id, launch.attempt_no);
                    if let Some(s) = &mut sup {
                        let stamp = s
                            .slot_open(&slot_run)
                            .map_err(|e| io(format!("slot_open: {}", e.detail)))?;
                        if let Some(env) = stamp.get("env") {
                            sp.cell.insert("env".to_string(), env.clone());
                        }
                        if let Some(grants) = stamp.get("grants") {
                            sp.cell.insert("grants".to_string(), grants.clone());
                        }
                    }
                    let packet_path = cfg
                        .packet_dir
                        .join(format!("slot-{slot_run}-a{}.json", launch.attempt_no));
                    workers.push(spawn_worker(hh_runner, &sp, &packet_path, &launch)?);
                    attempted.insert(id.clone());
                    start_order.push(Json::obj([
                        ("seq", Json::Int(start_order.len() as i64)),
                        ("run_plan_id", Json::str(&id)),
                        ("attempt_no", Json::Int(launch.attempt_no as i64)),
                        ("unit", Json::Int(*ui as i64)),
                        ("block", Json::Int(units[*ui].block as i64)),
                        ("in_flight_at_start", Json::Int((workers.len() - 1) as i64)),
                    ]));
                }
            }
        }
        let remaining_open = total_plans.saturating_sub(closed.len() + workers.len());
        if remaining_open == 0 && workers.is_empty() {
            break;
        }
        if workers.is_empty() && remaining_open > 0 {
            // Can anything still be admitted before its deadline? A
            // never-attempted plan is admissible while `admit(now)`;
            // an attempted replan is admissible at max(now, not_before)
            // — a retry scheduled past the cut-off is unresolved.
            let view = with_engine(store, docs, packet, &run_id, &mut lease, |eng| {
                eng.project().map_err(RunnerError::Engine)
            })?;
            let mut can_admit = false;
            'outer: for u in &units {
                for id in &u.members {
                    if closed.contains(id) {
                        continue;
                    }
                    let at = if attempted.contains(id) {
                        view.plans
                            .get(id)
                            .map(|p| p.not_before_ms.max(now))
                            .unwrap_or(now)
                    } else {
                        now
                    };
                    if window.admit(at, vec![window.slot_deadline_ms]) {
                        can_admit = true;
                        break 'outer;
                    }
                }
            }
            if !can_admit {
                let inflight: BTreeSet<String> =
                    workers.iter().map(|w| w.run_plan_id.clone()).collect();
                let (w, u2, newly) =
                    mark_unadmitted(&units, &closed, &attempted, &inflight, "window_deadline");
                withheld.extend(w);
                unresolved.extend(u2);
                closed.extend(newly);
                continue;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    // Own-label sweep + per-container teardown receipts (REQ-XB-127).
    let mut teardowns: Vec<Json> = Vec::new();
    if let Some(bin) = &packet.podman_bin {
        teardowns = crate::schedule::sweep_experiment(Path::new(bin), &eid)
            .map_err(|e| io(format!("sweep: {}", e.detail)))?;
    }
    let manifest_ref = {
        let at = Some(store.now_ms());
        let _ = harvest.persist(&manifest_path, &window.window_id, at);
        let doc = harvest.to_json(&window.window_id, at);
        let addr = store
            .put_blob(doc.to_canonical_string().as_bytes(), "application/json")
            .map_err(RunnerError::Ledger)?;
        format!("{}:{}", addr.algorithm, addr.digest)
    };
    let settled_ids: Vec<String> = settled.iter().map(|(id, _)| id.clone()).collect();
    let covers = harvest.covers(&settled_ids);
    let mut wrec = match window.record_json(c as u64) {
        Json::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    // Withheld/unresolved plans stay engine-open by construction — the
    // close is partial exactly then (the dossier records them).
    let partial = !withheld.is_empty() || !unresolved.is_empty();
    wrec.insert("start_order".into(), Json::Arr(start_order));
    wrec.insert("withheld".into(), Json::Arr(withheld));
    wrec.insert("unresolved".into(), Json::Arr(unresolved));
    wrec.insert("catalog_snapshots".into(), Json::Arr(snapshots));
    wrec.insert("teardown_receipts".into(), Json::Arr(teardowns));
    wrec.insert("harvest_manifest_ref".into(), Json::str(&manifest_ref));
    wrec.insert("harvest_manifest_covers".into(), Json::Bool(covers));
    // Window end — `stop` the proxy (catalog end-probe) and the
    // mediator; the finished proxy ledger is this window's harvested
    // record for the next window's carry-in fold.
    if let Some(s) = sup.take() {
        wrec.insert(
            "supervised".into(),
            Json::obj([
                ("proxy_run", Json::str(&s.proxy.run_id)),
                (
                    "proxy_root",
                    Json::str(s.proxy.store_root.display().to_string()),
                ),
                ("proxy_listen", Json::str(&s.proxy.listen)),
                (
                    "mediator_run",
                    s.mediator
                        .as_ref()
                        .map(|m| Json::str(&m.run_id))
                        .unwrap_or(Json::Null),
                ),
            ]),
        );
        s.close();
    }
    let window_json = Json::Obj(wrec);
    let _window_record_ref = {
        let addr = store
            .put_blob(
                window_json.to_canonical_string().as_bytes(),
                "application/json",
            )
            .map_err(RunnerError::Ledger)?;
        format!("{}:{}", addr.algorithm, addr.digest)
    };
    let close_status = {
        let mut eng = bound_engine(store, docs, packet, &run_id, &lease);
        eng.close(partial)
            .map_err(RunnerError::Engine)?
            .status
            .as_str()
            .to_string()
    };
    Ok(DriveReport {
        run_id,
        settled,
        close_status,
        window: Some(window_json),
    })
}
