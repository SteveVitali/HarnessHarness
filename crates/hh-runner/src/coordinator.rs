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

use std::collections::BTreeMap;
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

/// A live worker process the coordinator reaps and binds.
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
            m.insert("kind".to_string(), Json::str("oracle"));
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
    // read as the same WouldBlock fence a foreign holder sees).
    bind_subject(store, subject_run_id, slot_root, slot_run, subject_lease)?;
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
    })
}
