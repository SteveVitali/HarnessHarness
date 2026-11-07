//! The slot worker — one process per in-flight slot attempt, sole writer
//! of its own slot store root (REQ-XB-104; §3.3 ST0–ST7). It writes phase
//! rows, the ST4 `submission_seal` proof, the retained grade attempts,
//! one `hh-experiment-slot/1` blob and the slot run's
//! `lifecycle.run.finished{stop}` — then exits. Recovery never lives here:
//! the coordinator derives it from durable records (REQ-XB-105).

use std::path::{Path, PathBuf};

use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

use crate::disposition::{Disposition, SlotFlag};
use crate::facts::map_slot_facts;
use crate::json_ext::JsonExt;
use crate::packet::{SlotPacket, SlotRecipe};
use crate::slot_record::{
    ExperimentSlot, GradeAttemptRef, SlotCosts, SlotIntegrity, SlotPhase, SlotVerdict,
};

/// The fixture verdict channel spelling (a fixture channel, labelled —
/// never `harbor_reward_file/1` which the live cell owns).
pub const FIXTURE_CHANNEL: &str = "hh-runner-fixture-verdict/1";
/// The fixture executor stamp.
pub const FIXTURE_EXECUTOR: &str = "hh-runner-fixture/1";
/// The worker's writer-lease holder label.
pub const WORKER_HOLDER: &str = "hh-runner-slot";

/// Worker outcome — the seal coordinate the coordinator binds.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerSeal {
    /// The slot run id.
    pub slot_run_id: String,
    /// The sealed record's content address.
    pub record_ref: String,
    /// The slot record's `slot_ref`.
    pub slot_ref: String,
    /// The mapped disposition.
    pub disposition: Disposition,
    /// The slot run's head hash at the seal point.
    pub seal_head_hash: String,
}

/// The worker error — typed, never fabricated.
#[derive(Debug)]
pub enum WorkerError {
    /// A store/durability failure.
    Ledger(hharness_ledger::errors::LedgerError),
    /// A record/schema failure.
    Record(String),
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::Ledger(e) => write!(f, "worker ledger: {e}"),
            WorkerError::Record(d) => write!(f, "worker record: {d}"),
        }
    }
}

impl std::error::Error for WorkerError {}

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
        surface_ids: std::collections::BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-runner/1", store.now_ms())),
        content_kind: None,
        payload,
    }
}

fn append(events: Vec<Event>, store: &mut Store, run_id: &str, lease: &Lease) {
    let mut parent = store.head_event_id(run_id).unwrap_or_default();
    let mut batch = events;
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, lease, batch).unwrap();
}

/// The slot run's deterministic id — `slot-<idp(run_plan_id, attempt_no)>`:
/// the `(plan, attempt)` pair is the idempotency key (a second worker for
/// the same attempt finds the run dir held and refuses — the
/// second-writer refusal).
pub fn slot_run_id(run_plan_id: &str, attempt_no: u32) -> String {
    format!(
        "slot-{}",
        hharness_identity::idp::idp_id(
            "hh.runner.slot_run",
            format!("{run_plan_id}:{attempt_no}").as_bytes()
        )
    )
}

fn slot_manifest(packet: &SlotPacket) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.extra.insert(
        "slot".into(),
        Json::obj([
            ("run_plan_id", Json::str(&packet.run_plan_id)),
            ("attempt_no", Json::Int(packet.attempt_no as i64)),
            ("subject_run_id", Json::str(&packet.subject_run_id)),
            (
                "helper_protocol",
                Json::str(hharness_helper::protocol::HELPER_PROTOCOL),
            ),
            ("executor", Json::str(FIXTURE_EXECUTOR)),
        ]),
    );
    m.lease_ttl.writer_ms = 300_000;
    m
}

fn phase_row(phase: &SlotPhase, executor: &str) -> Json {
    Json::obj([
        ("phase", Json::str(&phase.phase)),
        ("wall_ms", Json::Int(phase.wall_ms as i64)),
        ("end_cause", Json::str(&phase.end_cause)),
        ("dispatch_outcome", Json::str(&phase.dispatch_outcome)),
        ("executor", Json::str(executor)),
    ])
}

/// `run_slot_worker(packet, slot_root) → WorkerSeal` — the ST0–ST7 drive
/// over the packet's declared recipe. The worker owns `slot_root`
/// exclusively for the call's duration; a second worker on a held slot
/// run refuses through the store's own rules.
pub fn run_slot_worker(packet: &SlotPacket, slot_root: &Path) -> Result<WorkerSeal, WorkerError> {
    let mut store = Store::open(slot_root).map_err(WorkerError::Ledger)?;
    let (run_id, lease) = store
        .open_run_with_id(&packet.slot_run_id, slot_manifest(packet), WORKER_HOLDER)
        .map_err(WorkerError::Ledger)?;

    let recipe = &packet.recipe;
    // HoldAgent dies inside ST3 before any seal — the kill-point recipe.
    if let SlotRecipe::HoldAgent { hold_ms } = recipe {
        append(
            vec![mint(
                &store,
                &run_id,
                "action.environment.phase.changed",
                Json::obj([
                    ("phase", Json::str("agent")),
                    ("end_cause", Json::str("in_flight")),
                    ("dispatch_outcome", Json::str("observed:applied")),
                    ("executor", Json::str(FIXTURE_EXECUTOR)),
                ]),
            )],
            &mut store,
            &run_id,
            &lease,
        );
        std::thread::sleep(std::time::Duration::from_millis(*hold_ms));
        return Err(WorkerError::Record("hold elapsed without kill".into()));
    }

    // ST0 ADMIT — a refusal produces a withheld record, never a cell.
    let mut facts = recipe.facts().expect("hold recipes return before this");
    // AR-07 deadline admission (REQ-XB-118): a packet declaring
    // `task.deadlines` enforces task-declared agent/verifier deadlines ×
    // declared multipliers under the declared protocol ceiling through
    // the separate admitted path; malformed or above-ceiling
    // declarations refuse `admission:deadline_admission`. A packet
    // without the member keeps the fixture defaults byte-identically.
    struct DeclaredDeadlines {
        agent: (u64, u64, u64),
        verifier: (u64, u64, u64),
    }
    let deadlines: Option<DeclaredDeadlines> = packet.task.get("deadlines").and_then(|d| {
        let m = d.as_obj().cloned().unwrap_or_default();
        let u = |k: &str| -> u64 {
            m.get(k)
                .and_then(Json::as_int)
                .map(|v| v as u64)
                .unwrap_or(0)
        };
        let ppm = |k: &str| -> u64 {
            m.get(k)
                .and_then(Json::as_int)
                .map(|v| v as u64)
                .unwrap_or(hharness_env::task_deadline::CANONICAL_MULTIPLIER_PPM)
        };
        let ceiling = u("ceiling_sec");
        let (agent_sec, agent_ppm) = (u("agent_sec"), ppm("agent_multiplier_ppm"));
        let (verifier_sec, verifier_ppm) = (u("verifier_sec"), ppm("verifier_multiplier_ppm"));
        let e = hharness_env::task_deadline::task_declared_deadline_ms;
        match (
            e(agent_sec, agent_ppm, ceiling),
            e(verifier_sec, verifier_ppm, ceiling),
        ) {
            (Ok(a), Ok(v)) => Some(DeclaredDeadlines {
                agent: (agent_sec, agent_ppm, a),
                verifier: (verifier_sec, verifier_ppm, v),
            }),
            _ => None,
        }
    });
    if packet.task.get("deadlines").is_some() && deadlines.is_none() {
        // An ST0 refusal precedes any run — nothing the recipe would
        // have produced exists (no reward, no infra ending); only the
        // refusal fact stands.
        let mut refused = crate::facts::SlotFacts::default();
        refused.admission_refusal = Some(crate::disposition::AdmissionReason::DeadlineAdmission);
        facts = refused;
    }
    let outcome = map_slot_facts(&facts);

    let mut phases: Vec<SlotPhase> = Vec::new();
    let mut verdict: Option<SlotVerdict> = None;
    let mut seal_ref: Option<String> = None;
    let mut invoked_ref: Option<String> = None;
    let mut verdict_ref: Option<String> = None;

    if facts.admission_refusal.is_none() {
        // ST1 PROVISION — fixture provisions through the declared recipe;
        // an infra cause ending here is recorded, never hidden.
        let provision_ok = facts.infra.is_none();
        if !provision_ok {
            append(
                vec![mint(
                    &store,
                    &run_id,
                    "action.environment.failed",
                    Json::obj([
                        (
                            "detail",
                            Json::str(format!(
                                "infra:{}",
                                facts.infra.as_ref().map(|c| c.as_str()).unwrap_or_default()
                            )),
                        ),
                        ("executor", Json::str(FIXTURE_EXECUTOR)),
                    ]),
                )],
                &mut store,
                &run_id,
                &lease,
            );
        } else {
            append(
                vec![mint(
                    &store,
                    &run_id,
                    "action.environment.provisioned",
                    Json::obj([
                        ("executor", Json::str(FIXTURE_EXECUTOR)),
                        ("dispatch_outcome", Json::str("observed:applied")),
                    ]),
                )],
                &mut store,
                &run_id,
                &lease,
            );
            // ST2/ST3 phase rows (setup, agent) — fixture dispatch.
            for (phase, end_cause) in [
                ("setup", "completed"),
                (
                    "agent",
                    facts
                        .reward
                        .as_ref()
                        .map(|r| r.end_cause.as_str())
                        .unwrap_or_else(|| "completed".into())
                        .as_str(),
                ),
            ] {
                let (dsec, dppm, dms) = if phase == "agent" {
                    deadlines
                        .as_ref()
                        .map(|d| d.agent)
                        .unwrap_or((60, 1_000_000, 60_000))
                } else {
                    (60, 1_000_000, 60_000)
                };
                let ph = SlotPhase {
                    phase: phase.to_string(),
                    wall_ms: 1,
                    deadline_declared_sec: Some(dsec),
                    deadline_multiplier_ppm: Some(dppm),
                    deadline_enforced_ms: Some(dms),
                    end_cause: end_cause.to_string(),
                    exit_code: Some(0),
                    dispatch_outcome: "observed:applied".to_string(),
                };
                phases.push(ph.clone());
                append(
                    vec![mint(&store, &run_id, "action.environment.phase.changed", {
                        let mut p = phase_row(&ph, FIXTURE_EXECUTOR);
                        if let Some(sec) = ph.deadline_declared_sec {
                            if let Json::Obj(ref mut m) = p {
                                m.insert(
                                    "deadline".into(),
                                    Json::obj([
                                        ("declared_sec", Json::Int(sec as i64)),
                                        (
                                            "multiplier_ppm",
                                            Json::Int(
                                                ph.deadline_multiplier_ppm.unwrap_or(0) as i64
                                            ),
                                        ),
                                        (
                                            "enforced_ms",
                                            Json::Int(ph.deadline_enforced_ms.unwrap_or(0) as i64),
                                        ),
                                    ]),
                                );
                            }
                        }
                        p
                    })],
                    &mut store,
                    &run_id,
                    &lease,
                );
            }
            // ST4 SEAL — the submission_seal proof row (hh-bench's
            // registered class spellings, CC1).
            append(
                vec![mint(
                    &store,
                    &run_id,
                    hharness_bench::oracle::SEAL_CLASS,
                    Json::obj([
                        ("purpose", Json::str(hharness_bench::oracle::SEAL_PURPOSE)),
                        ("executor", Json::str(FIXTURE_EXECUTOR)),
                    ]),
                )],
                &mut store,
                &run_id,
                &lease,
            );
            seal_ref = Some(format!(
                "{}:{}",
                run_id,
                store.head_event_id(&run_id).map_err(WorkerError::Ledger)?
            ));
            // ST5 VERIFY — one grade attempt; `verifier_unavailable`
            // regrades exhaust the declared bound (≤ 2) and stay
            // unavailable.
            let max_attempts: u32 = if facts.verifier_unavailable.is_some() {
                2
            } else {
                1
            };
            for attempt_no in 1..=max_attempts {
                append(
                    vec![mint(
                        &store,
                        &run_id,
                        hharness_bench::oracle::ORACLE_INVOKED,
                        Json::obj([
                            ("subject_run", Json::str(&packet.subject_run_id)),
                            (
                                "submission_ref",
                                Json::str(seal_ref.as_deref().unwrap_or("")),
                            ),
                            ("oracle_ref", Json::str(FIXTURE_CHANNEL)),
                            ("attempt_no", Json::Int(attempt_no as i64)),
                        ]),
                    )],
                    &mut store,
                    &run_id,
                    &lease,
                );
                invoked_ref = Some(format!(
                    "{}:{}",
                    run_id,
                    store.head_event_id(&run_id).map_err(WorkerError::Ledger)?
                ));
                let unavailable = facts.verifier_unavailable.is_some();
                append(
                    vec![mint(
                        &store,
                        &run_id,
                        hharness_bench::oracle::ORACLE_VERDICT,
                        Json::obj([
                            ("attempt_no", Json::Int(attempt_no as i64)),
                            (
                                "grade_outcome",
                                Json::str(if unavailable {
                                    "oracle_failure"
                                } else {
                                    "decided"
                                }),
                            ),
                            ("channel", Json::str(FIXTURE_CHANNEL)),
                        ]),
                    )],
                    &mut store,
                    &run_id,
                    &lease,
                );
                verdict_ref = Some(format!(
                    "{}:{}",
                    run_id,
                    store.head_event_id(&run_id).map_err(WorkerError::Ledger)?
                ));
                if !unavailable {
                    break;
                }
            }
            let (vsec, vppm, vms) = deadlines
                .as_ref()
                .map(|d| d.verifier)
                .unwrap_or((30, 1_000_000, 30_000));
            phases.push(SlotPhase {
                phase: "verify".to_string(),
                wall_ms: 1,
                deadline_declared_sec: Some(vsec),
                deadline_multiplier_ppm: Some(vppm),
                deadline_enforced_ms: Some(vms),
                end_cause: if facts.verifier_unavailable.is_some() {
                    "unavailable".to_string()
                } else {
                    "completed".to_string()
                },
                exit_code: None,
                dispatch_outcome: "observed:applied".to_string(),
            });
            // The verdict section — `reward.json` bytes sealed in the
            // slot store's blob pool.
            if facts.reward.is_some() || facts.verifier_unavailable.is_some() {
                let reward_ppm = facts.reward.as_ref().and_then(|r| r.reward_ppm);
                let reward_raw =
                    reward_ppm.map(|ppm| Json::obj([("reward", Json::Int(ppm as i64))]));
                let reward_bytes = reward_raw
                    .as_ref()
                    .map(|r| r.to_canonical_string().into_bytes());
                let reward_addr = reward_bytes
                    .as_ref()
                    .map(|b| store.put_blob(b, "text/plain"))
                    .transpose()
                    .map_err(WorkerError::Ledger)?;
                let mut files = std::collections::BTreeMap::new();
                if let (Some(b), Some(a)) = (reward_bytes, reward_addr) {
                    files.insert(
                        "reward.json".to_string(),
                        Json::obj([
                            ("sha256", Json::str(hharness_wire::sha256::sha256_hex(&b))),
                            ("bytes", Json::Int(b.len() as i64)),
                            (
                                "blob_ref",
                                Json::str(format!("{}:{}", a.algorithm, a.digest)),
                            ),
                        ]),
                    );
                }
                let mut flags: Vec<SlotFlag> = facts
                    .reward
                    .as_ref()
                    .map(|r| r.flags.clone())
                    .unwrap_or_default();
                if facts.relay_killed_by_agent && !flags.contains(&SlotFlag::RelayKilledByAgent) {
                    flags.push(SlotFlag::RelayKilledByAgent);
                }
                verdict = Some(SlotVerdict {
                    channel: FIXTURE_CHANNEL.to_string(),
                    files,
                    reward_raw,
                    reward_ppm,
                    success: reward_ppm.map(|p| p > 0),
                    ctrf: None,
                    classification: if facts.verifier_unavailable.is_some() {
                        "verifier_unavailable".to_string()
                    } else {
                        "decided".to_string()
                    },
                    flags,
                    grade_attempts: match (invoked_ref, verdict_ref) {
                        (Some(i), Some(v)) => vec![GradeAttemptRef {
                            attempt_no: max_attempts,
                            invoked_ref: i,
                            verdict_ref: v,
                        }],
                        _ => vec![],
                    },
                });
            }
        }
    }

    // The slot's declared consumption — the worker's own accounting rows
    // (the coordinator mirrors them verbatim into the subject run).
    let consumed_calls = match recipe {
        SlotRecipe::Scored {
            consumed_model_calls,
            ..
        } => Some(*consumed_model_calls),
        _ => None,
    };
    if let Some(amount) = consumed_calls {
        append(
            vec![mint(
                &store,
                &run_id,
                "control.budget.consumed",
                Json::obj([
                    ("dimension", Json::str("model_calls")),
                    ("amount", Json::Int(amount)),
                ]),
            )],
            &mut store,
            &run_id,
            &lease,
        );
    }

    // ST6 RECORD — the hh-experiment-slot/1 blob + the bound finished row.
    let head_hash = store.head_event_id(&run_id).map_err(WorkerError::Ledger)?;
    let runner = crate::slot_record::RunnerIdentity::detect();
    // AR-06 cell echo (REQ-XB-107): a packet declaring `cell.cell_spec`
    // records the applied limits + the helper argv tail verbatim.
    let cell_members = {
        let mut c = packet.cell.clone();
        if let Some(spec_j) = packet.cell.get("cell_spec") {
            if let Ok(spec) = hharness_env::cell_spec::CellSpec::from_json(spec_j) {
                if !spec.is_empty() {
                    c.insert("applied_limits".to_string(), spec.to_json());
                    c.insert(
                        "helper_args".to_string(),
                        Json::Arr(spec.helper_args().into_iter().map(Json::str).collect()),
                    );
                }
            }
        }
        c
    };
    let identity = Json::obj([
        ("experiment_id", Json::str(&packet.experiment_id)),
        ("protocol_id", Json::str(&packet.protocol_id)),
        ("selection_id", Json::str(&packet.selection_id)),
        ("run_plan_id", Json::str(&packet.run_plan_id)),
        ("cell_id", Json::str(&packet.cell_id)),
        ("arm_id", Json::str(&packet.arm_id)),
        ("task_id", Json::str(&packet.task_id)),
        ("replicate_index", Json::Int(packet.replicate_index as i64)),
        ("first", Json::Bool(packet.attempt_no == 1)),
        ("attempt_no", Json::Int(packet.attempt_no as i64)),
        (
            "epoch",
            Json::str(
                packet
                    .task
                    .get("epoch")
                    .and_then(Json::as_str)
                    .unwrap_or("fixture"),
            ),
        ),
        ("subject_run_id", Json::str(&packet.subject_run_id)),
        (
            "slot_store",
            Json::obj([
                ("root_digest", Json::str(root_digest(slot_root))),
                ("run_id", Json::str(&run_id)),
                ("head_hash", Json::str(&head_hash)),
            ]),
        ),
        ("runner", runner.to_json()),
    ]);
    // REQ-XH-014 — the O-surface slots carry the program-wide Harbor
    // pin in `identity.harbor` (the packet's declared member).
    let identity = match (identity, &packet.harbor) {
        (Json::Obj(mut m), Some(pin)) => {
            m.insert(
                "harbor".to_string(),
                Json::obj([("pin", Json::str(pin.clone()))]),
            );
            Json::Obj(m)
        }
        (id, _) => id,
    };
    let record = ExperimentSlot {
        identity,
        task: Json::Obj(packet.task.clone()),
        participant: Json::Obj(packet.participant.clone()),
        cell: Json::Obj(cell_members),
        phases,
        verdict,
        disposition: outcome.disposition.clone(),
        prior_disposition: None,
        repairs: vec![],
        integrity: SlotIntegrity {
            seal: if seal_ref.is_some() {
                "sealed".to_string()
            } else {
                "n/a{no_cell}".to_string()
            },
            seal_ref,
            sentinel_intact: Some(true),
            observer_survived: Some(true),
            probe: Some("ok:undeterminable".to_string()),
        },
        trajectory: std::collections::BTreeMap::new(),
        costs: SlotCosts::default(),
    };
    let record_json = record.to_json();
    let record_bytes = record_json.to_canonical_string().into_bytes();
    let record_addr = store
        .put_blob(&record_bytes, "application/json")
        .map_err(WorkerError::Ledger)?;
    let record_ref = format!("{}:{}", record_addr.algorithm, record_addr.digest);

    let mut finished_payload = Json::obj([
        ("stop_reason", outcome.stop_reason.to_json()),
        ("outcome_class", Json::str(outcome.outcome_class.as_str())),
        ("oracle_failed", Json::Bool(outcome.oracle_failed)),
        ("disposition", Json::str(outcome.disposition.as_str())),
        ("slot_record_ref", Json::str(&record_ref)),
        ("slot_ref", Json::str(record.slot_ref())),
        ("executor", Json::str(FIXTURE_EXECUTOR)),
    ]);
    if let Some(end) = outcome.disposition.end_cause() {
        if let Json::Obj(ref mut m) = finished_payload {
            m.insert("end_cause".into(), Json::str(end.as_str()));
        }
    }
    let mut finished = mint(&store, &run_id, "lifecycle.run.finished", finished_payload);
    finished.refs = vec![record_addr];
    append(vec![finished], &mut store, &run_id, &lease);

    // HoldAfterSeal — sealed but killed before the coordinator reaps.
    if let SlotRecipe::HoldAfterSeal { hold_ms, .. } = recipe {
        let seal = WorkerSeal {
            slot_run_id: run_id,
            record_ref,
            slot_ref: record.slot_ref(),
            disposition: outcome.disposition.clone(),
            seal_head_hash: head_hash,
        };
        let _ = &seal;
        std::thread::sleep(std::time::Duration::from_millis(*hold_ms));
        return Ok(seal);
    }

    Ok(WorkerSeal {
        slot_run_id: run_id,
        record_ref,
        slot_ref: record.slot_ref(),
        disposition: outcome.disposition,
        seal_head_hash: head_hash,
    })
}

/// The slot store's persisted `store.tag` digest — `sha256:<hex>` over the
/// tag bytes; `unrecorded` when absent (the store creates it on open).
pub fn root_digest(slot_root: &Path) -> String {
    match std::fs::read_to_string(slot_root.join("store.tag")) {
        Ok(t) => format!(
            "sha256:{}",
            hharness_wire::sha256::sha256_hex(t.trim().as_bytes())
        ),
        Err(_) => "unrecorded".to_string(),
    }
}

/// PathBuf helper for callers that derive the slot root from a declared
/// base (never absolute-path minted: the base is a declared run input).
pub fn slot_root_for(slots_dir: &Path, run_plan_id: &str, attempt_no: u32) -> PathBuf {
    slots_dir.join(format!("{}-a{}", run_plan_id, attempt_no))
}
