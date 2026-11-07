//! `hh-experiment-slot/1` — the one record per slot (REQ-XB-108; §3.3's
//! section list; design/31 §1.4 spellings carrying over from `hh-s4-slot/1`
//! where they exist). The codec lives here over `hharness-wire` canonical
//! JSON (contract Notes — a runner-crate record, not `hh-embed-schema`).
//!
//! `costs` holds references to `hh-slot-costs/1` only (`model` = the proxy
//! fold, `compute` = the window reference — both owned by R6.14) and never
//! duplicates them: the codec refuses any `costs` member other than
//! `{schema, ref}` references.
//!
//! `identity.slot_store.head_hash` is the slot run's head at the seal
//! point — the head immediately before the `lifecycle.run.finished` row
//! that binds the record lands. The post-record head is the coordinate the
//! coordinator binds into the subject run (`slot_store.head_hash` there);
//! both are durable-derivable and never recomputed.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_id;
use hharness_wire::json::Json;

use crate::json_ext::JsonExt;

use crate::disposition::{Disposition, SlotFlag, VocabularyError};

/// The record kind spelling.
pub const SLOT_RECORD_SCHEMA: &str = "hh-experiment-slot/1";

/// Strict-decode error (schema member or vocabulary refusal).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordError {
    /// What failed.
    pub detail: String,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{SLOT_RECORD_SCHEMA}: {}", self.detail)
    }
}

impl std::error::Error for RecordError {}

fn err(detail: impl Into<String>) -> RecordError {
    RecordError {
        detail: detail.into(),
    }
}

fn obj<'a>(j: &'a Json, member: &str) -> Result<&'a BTreeMap<String, Json>, RecordError> {
    j.get(member)
        .and_then(Json::as_obj)
        .ok_or_else(|| err(format!("{member} must be an object")))
}

fn str_member(m: &BTreeMap<String, Json>, name: &str) -> Result<String, RecordError> {
    m.get(name)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| err(format!("{name} must be a string")))
}

fn int_member(m: &BTreeMap<String, Json>, name: &str) -> Result<i64, RecordError> {
    m.get(name)
        .and_then(Json::as_int)
        .ok_or_else(|| err(format!("{name} must be an integer")))
}

fn no_unknown(
    m: &BTreeMap<String, Json>,
    allowed: &[&str],
    section: &str,
) -> Result<(), RecordError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(err(format!("{section}: unknown member {k}")));
        }
    }
    Ok(())
}

fn opt_str(m: &BTreeMap<String, Json>, name: &str) -> Option<String> {
    m.get(name).and_then(Json::as_str).map(str::to_string)
}

fn opt_int(m: &BTreeMap<String, Json>, name: &str) -> Option<i64> {
    m.get(name).and_then(Json::as_int)
}

/// `identity.slot_store{root_digest, run_id, head_hash}` — the slot store's
/// durable coordinate at the seal point.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotStoreRef {
    /// Digest of the slot store's persisted `store.tag` (`sha256:<hex>`).
    pub root_digest: String,
    /// The slot run id.
    pub run_id: String,
    /// The slot run's head hash at the seal point.
    pub head_hash: String,
}

/// The runner's identity stamps — `driver_commit` (REQ-XA-127) and the
/// binary digest (REQ-XB-101; every packet names both).
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerIdentity {
    /// The runner commit the packet names (`unrecorded` when unset).
    pub driver_commit: String,
    /// `sha256:<hex>` of the running `hh-runner` binary.
    pub binary_sha256: String,
}

impl RunnerIdentity {
    /// Detect from the environment: `HH_RUNNER_COMMIT` names the commit
    /// (records-in — the runner never shells out for it);
    /// `binary_sha256` hashes the running executable's bytes.
    pub fn detect() -> RunnerIdentity {
        let commit = std::env::var("HH_RUNNER_COMMIT").unwrap_or_else(|_| "unrecorded".to_string());
        let digest = std::env::current_exe()
            .and_then(|p| std::fs::read(&p).map(|b| (p, b)))
            .map(|(_, bytes)| format!("sha256:{}", hharness_wire::sha256::sha256_hex(&bytes)))
            .unwrap_or_else(|_| "unrecorded".to_string());
        RunnerIdentity {
            driver_commit: commit,
            binary_sha256: digest,
        }
    }

    /// The canonical JSON (`{driver_commit, binary_sha256}`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("driver_commit", Json::str(&self.driver_commit)),
            ("binary_sha256", Json::str(&self.binary_sha256)),
        ])
    }
}

/// `phases[]` — one row per executed lifecycle phase (design §1.4
/// `phases[]`; §3.3 ST2/ST3/ST5 carry `setup|agent|verify`, the closed set
/// `hharness_env::driver::EnvDriver::set_phase` accepts).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotPhase {
    /// `setup | agent | verify` (closed set).
    pub phase: String,
    /// Wall time (ms).
    pub wall_ms: u64,
    /// The declared deadline seconds, when a deadline bound the phase.
    pub deadline_declared_sec: Option<u64>,
    /// The deadline multiplier (ppm of 1.0), when applied.
    pub deadline_multiplier_ppm: Option<u64>,
    /// The enforced deadline (ms), when enforced.
    pub deadline_enforced_ms: Option<u64>,
    /// The end cause (agent phase) or `completed`.
    pub end_cause: String,
    /// The exit code, when the phase ran a process.
    pub exit_code: Option<i64>,
    /// The dispatch outcome stamp (`observed:applied` family).
    pub dispatch_outcome: String,
}

impl SlotPhase {
    /// The closed phase set (the `set_phase` vocabulary).
    pub const PHASES: [&'static str; 3] = ["setup", "agent", "verify"];

    fn to_json(&self) -> Json {
        let mut deadline = BTreeMap::new();
        if let Some(s) = self.deadline_declared_sec {
            deadline.insert("declared_sec".into(), Json::Int(s as i64));
        }
        if let Some(m) = self.deadline_multiplier_ppm {
            deadline.insert("multiplier_ppm".into(), Json::Int(m as i64));
        }
        if let Some(e) = self.deadline_enforced_ms {
            deadline.insert("enforced_ms".into(), Json::Int(e as i64));
        }
        let mut m = vec![
            ("phase", Json::str(&self.phase)),
            ("wall_ms", Json::Int(self.wall_ms as i64)),
            ("deadline", Json::Obj(deadline)),
            ("end_cause", Json::str(&self.end_cause)),
            ("dispatch_outcome", Json::str(&self.dispatch_outcome)),
        ];
        if let Some(rc) = self.exit_code {
            m.push(("exit_code", Json::Int(rc)));
        }
        Json::obj(m)
    }

    fn from_json(j: &Json) -> Result<SlotPhase, RecordError> {
        let m = j
            .as_obj()
            .ok_or_else(|| err("phases[] member must be an object"))?;
        no_unknown(
            m,
            &[
                "phase",
                "wall_ms",
                "deadline",
                "end_cause",
                "exit_code",
                "dispatch_outcome",
            ],
            "phases[]",
        )?;
        let phase = str_member(m, "phase")?;
        if !Self::PHASES.contains(&phase.as_str()) {
            return Err(err(format!("phase {phase} is outside setup|agent|verify")));
        }
        let d = obj(j, "deadline")?;
        no_unknown(
            d,
            &["declared_sec", "multiplier_ppm", "enforced_ms"],
            "phases[].deadline",
        )?;
        Ok(SlotPhase {
            phase,
            wall_ms: int_member(m, "wall_ms")? as u64,
            deadline_declared_sec: opt_int(d, "declared_sec").map(|v| v as u64),
            deadline_multiplier_ppm: opt_int(d, "multiplier_ppm").map(|v| v as u64),
            deadline_enforced_ms: opt_int(d, "enforced_ms").map(|v| v as u64),
            end_cause: str_member(m, "end_cause")?,
            exit_code: opt_int(m, "exit_code"),
            dispatch_outcome: str_member(m, "dispatch_outcome")?,
        })
    }
}

/// A `costs` reference — `{schema: "hh-slot-costs/1", ref: <address>}` —
/// references only, never the folded value (REQ-XB-108).
#[derive(Debug, Clone, PartialEq)]
pub struct CostRef {
    /// The referenced `hh-slot-costs/1` record address.
    pub ref_: String,
}

impl CostRef {
    fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-slot-costs/1")),
            ("ref", Json::str(&self.ref_)),
        ])
    }

    fn from_json(j: &Json) -> Result<CostRef, RecordError> {
        let m = j
            .as_obj()
            .ok_or_else(|| err("costs.* must be a {schema, ref} reference"))?;
        no_unknown(m, &["schema", "ref"], "costs.*")?;
        if str_member(m, "schema")? != "hh-slot-costs/1" {
            return Err(err("costs.* schema must be hh-slot-costs/1"));
        }
        Ok(CostRef {
            ref_: str_member(m, "ref")?,
        })
    }
}

/// `costs` — `model` (the proxy fold) and `compute` (the window
/// reference), references only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotCosts {
    /// `costs.model` — the proxy fold reference.
    pub model: Option<CostRef>,
    /// `costs.compute` — the window reference.
    pub compute: Option<CostRef>,
}

/// `verdict.grade_attempts[]` — one retained grade attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeAttemptRef {
    /// The attempt ordinal.
    pub attempt_no: u32,
    /// The `verification.oracle.invoked` event ref (`run_id:event_id`).
    pub invoked_ref: String,
    /// The `verification.oracle.verdict` event ref (`run_id:event_id`).
    pub verdict_ref: String,
}

/// The `verdict` section — the channel's reward record.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotVerdict {
    /// The verdict channel (`harbor_reward_file/1`, `tb10_pytest_log/1`,
    /// or the fixture channel `hh-runner-fixture-verdict/1`).
    pub channel: String,
    /// `files{name → {sha256, bytes, blob_ref?}}` — the reward-side bytes.
    pub files: BTreeMap<String, Json>,
    /// The raw reward document (verbatim).
    pub reward_raw: Option<Json>,
    /// The reward in ppm of 1.0, when the channel produced one.
    pub reward_ppm: Option<u64>,
    /// `success` when the channel classifies one.
    pub success: Option<bool>,
    /// `ctrf{tests, passed, failed, skipped, errors}`, when produced.
    pub ctrf: Option<Json>,
    /// The classification spelling.
    pub classification: String,
    /// Reserved flags (verbatim spellings, the closed set).
    pub flags: Vec<SlotFlag>,
    /// The retained grade attempts.
    pub grade_attempts: Vec<GradeAttemptRef>,
}

/// `integrity` — the seal/sentinel/observer/probe record (`hh-s4-slot/1`
/// spellings carried over).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotIntegrity {
    /// `sealed | …` — the seal state.
    pub seal: String,
    /// The seal proof ref (`run_id:event_id` of the
    /// `action.environment.snapshot{purpose: submission_seal}` row).
    pub seal_ref: Option<String>,
    /// `sentinel_intact`.
    pub sentinel_intact: Option<bool>,
    /// `observer_survived`.
    pub observer_survived: Option<bool>,
    /// The probe disposition (`ok:…` / `probe` spelling).
    pub probe: Option<String>,
}

/// The whole `hh-experiment-slot/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ExperimentSlot {
    /// `identity` — the full §3.3 member set.
    pub identity: Json,
    /// `task` — dataset pins, task tree digest, image ref, declared
    /// resources/timeouts.
    pub task: Json,
    /// `participant` — oracle|hosted, harness triple, config ref.
    pub participant: Json,
    /// `cell` — backend, applied limits, egress record.
    pub cell: Json,
    /// `phases[]`.
    pub phases: Vec<SlotPhase>,
    /// `verdict` (`None` before any verify leg — e.g. an ST0 refusal).
    pub verdict: Option<SlotVerdict>,
    /// `disposition.value` — the typed spelling.
    pub disposition: Disposition,
    /// `disposition.prior_disposition` (repairs carry it).
    pub prior_disposition: Option<String>,
    /// `disposition.repairs[{class, location, commit}]`.
    pub repairs: Vec<Json>,
    /// `integrity`.
    pub integrity: SlotIntegrity,
    /// `trajectory{atif_ref, atif_schema, proxy_transcript_ref}` members.
    pub trajectory: BTreeMap<String, Json>,
    /// `costs` — references only.
    pub costs: SlotCosts,
}

impl ExperimentSlot {
    /// The canonical record JSON (`{schema, identity, task, participant,
    /// cell, phases, verdict?, disposition, integrity, trajectory, costs}`).
    pub fn to_json(&self) -> Json {
        let mut disposition = vec![("value", self.disposition.to_json())];
        if let Some(p) = &self.prior_disposition {
            disposition.push(("prior_disposition", Json::str(p)));
        }
        if !self.repairs.is_empty() {
            disposition.push(("repairs", Json::Arr(self.repairs.clone())));
        }
        let mut integrity = vec![("seal", Json::str(&self.integrity.seal))];
        if let Some(r) = &self.integrity.seal_ref {
            integrity.push(("seal_ref", Json::str(r)));
        }
        if let Some(b) = self.integrity.sentinel_intact {
            integrity.push(("sentinel_intact", Json::Bool(b)));
        }
        if let Some(b) = self.integrity.observer_survived {
            integrity.push(("observer_survived", Json::Bool(b)));
        }
        if let Some(p) = &self.integrity.probe {
            integrity.push(("probe", Json::str(p)));
        }
        let mut costs = BTreeMap::new();
        if let Some(c) = &self.costs.model {
            costs.insert("model".into(), c.to_json());
        }
        if let Some(c) = &self.costs.compute {
            costs.insert("compute".into(), c.to_json());
        }
        let mut m = vec![
            ("schema", Json::str(SLOT_RECORD_SCHEMA)),
            ("identity", self.identity.clone()),
            ("task", self.task.clone()),
            ("participant", self.participant.clone()),
            ("cell", self.cell.clone()),
            (
                "phases",
                Json::Arr(self.phases.iter().map(SlotPhase::to_json).collect()),
            ),
            ("disposition", Json::obj(disposition)),
            ("integrity", Json::obj(integrity)),
            ("trajectory", Json::Obj(self.trajectory.clone())),
            ("costs", Json::Obj(costs)),
        ];
        if let Some(v) = &self.verdict {
            let mut vm = vec![
                ("channel", Json::str(&v.channel)),
                ("files", Json::Obj(v.files.clone())),
                ("classification", Json::str(&v.classification)),
                (
                    "flags",
                    Json::Arr(v.flags.iter().map(|f| Json::str(f.as_str())).collect()),
                ),
                (
                    "grade_attempts",
                    Json::Arr(
                        v.grade_attempts
                            .iter()
                            .map(|a| {
                                Json::obj([
                                    ("attempt_no", Json::Int(a.attempt_no as i64)),
                                    ("invoked_ref", Json::str(&a.invoked_ref)),
                                    ("verdict_ref", Json::str(&a.verdict_ref)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ];
            if let Some(r) = &v.reward_raw {
                vm.push(("reward_raw", r.clone()));
            }
            if let Some(p) = v.reward_ppm {
                vm.push(("reward_ppm", Json::Int(p as i64)));
            }
            if let Some(s) = v.success {
                vm.push(("success", Json::Bool(s)));
            }
            if let Some(c) = &v.ctrf {
                vm.push(("ctrf", c.clone()));
            }
            m.push(("verdict", Json::obj(vm)));
        }
        Json::obj(m)
    }

    /// Strict decode + validate — unknown members, out-of-set vocabulary
    /// and duplicated cost members all refuse.
    pub fn from_json(j: &Json) -> Result<ExperimentSlot, RecordError> {
        let m = j.as_obj().ok_or_else(|| err("record must be an object"))?;
        no_unknown(
            m,
            &[
                "schema",
                "identity",
                "task",
                "participant",
                "cell",
                "phases",
                "verdict",
                "disposition",
                "integrity",
                "trajectory",
                "costs",
            ],
            "record",
        )?;
        if str_member(m, "schema")? != SLOT_RECORD_SCHEMA {
            return Err(err("schema member is not hh-experiment-slot/1"));
        }
        // identity: require the §3.3 coordinate members.
        let id = obj(j, "identity")?;
        for k in [
            "experiment_id",
            "protocol_id",
            "run_plan_id",
            "cell_id",
            "arm_id",
            "task_id",
            "replicate_index",
            "first",
            "attempt_no",
            "subject_run_id",
            "slot_store",
            "runner",
        ] {
            if !id.contains_key(k) {
                return Err(err(format!("identity.{k} required")));
            }
        }
        let ss = obj(j.get("identity").unwrap(), "slot_store")?;
        no_unknown(
            ss,
            &["root_digest", "run_id", "head_hash"],
            "identity.slot_store",
        )?;
        let runner = obj(j.get("identity").unwrap(), "runner")?;
        no_unknown(
            runner,
            &["driver_commit", "binary_sha256"],
            "identity.runner",
        )?;
        let dm = obj(j, "disposition")?;
        no_unknown(
            dm,
            &["value", "prior_disposition", "repairs"],
            "disposition",
        )?;
        let disposition = Disposition::from_json(dm.get("value").unwrap_or(&Json::Null))
            .map_err(|e: VocabularyError| err(e.to_string()))?;
        let im = obj(j, "integrity")?;
        no_unknown(
            im,
            &[
                "seal",
                "seal_ref",
                "sentinel_intact",
                "observer_survived",
                "probe",
            ],
            "integrity",
        )?;
        let cm = obj(j, "costs")?;
        // References only — a value member is a duplicated fold, refused.
        for k in cm.keys() {
            if k != "model" && k != "compute" {
                return Err(err(format!("costs.{k} is not a fold reference")));
            }
        }
        let costs = SlotCosts {
            model: cm.get("model").map(CostRef::from_json).transpose()?,
            compute: cm.get("compute").map(CostRef::from_json).transpose()?,
        };
        let tm = obj(j, "trajectory")?;
        no_unknown(
            tm,
            &["atif_ref", "atif_schema", "proxy_transcript_ref"],
            "trajectory",
        )?;
        let verdict = match m.get("verdict") {
            None | Some(Json::Null) => None,
            Some(v) => {
                let vmo = v.as_obj().ok_or_else(|| err("verdict must be an object"))?;
                no_unknown(
                    vmo,
                    &[
                        "channel",
                        "files",
                        "reward_raw",
                        "reward_ppm",
                        "success",
                        "ctrf",
                        "classification",
                        "flags",
                        "grade_attempts",
                    ],
                    "verdict",
                )?;
                let flags = vmo
                    .get("flags")
                    .and_then(Json::as_arr)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|f| {
                        f.as_str()
                            .and_then(|s| SlotFlag::parse(s).ok())
                            .ok_or_else(|| err("verdict.flags member outside the closed set"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let grade_attempts = vmo
                    .get("grade_attempts")
                    .and_then(Json::as_arr)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|a| {
                        let am = a
                            .as_obj()
                            .ok_or_else(|| err("grade_attempts[] must be objects"))?;
                        no_unknown(
                            am,
                            &["attempt_no", "invoked_ref", "verdict_ref"],
                            "grade_attempts[]",
                        )?;
                        Ok(GradeAttemptRef {
                            attempt_no: int_member(am, "attempt_no")? as u32,
                            invoked_ref: str_member(am, "invoked_ref")?,
                            verdict_ref: str_member(am, "verdict_ref")?,
                        })
                    })
                    .collect::<Result<Vec<_>, RecordError>>()?;
                Some(SlotVerdict {
                    channel: str_member(vmo, "channel")?,
                    files: vmo
                        .get("files")
                        .and_then(Json::as_obj)
                        .cloned()
                        .unwrap_or_default(),
                    reward_raw: vmo.get("reward_raw").cloned(),
                    reward_ppm: opt_int(vmo, "reward_ppm").map(|v| v as u64),
                    success: vmo.get("success").and_then(Json::as_bool),
                    ctrf: vmo.get("ctrf").cloned(),
                    classification: str_member(vmo, "classification")?,
                    flags,
                    grade_attempts,
                })
            }
        };
        Ok(ExperimentSlot {
            identity: m.get("identity").unwrap().clone(),
            task: m.get("task").unwrap_or(&Json::obj([])).clone(),
            participant: m.get("participant").unwrap_or(&Json::obj([])).clone(),
            cell: m.get("cell").unwrap_or(&Json::obj([])).clone(),
            phases: m
                .get("phases")
                .and_then(Json::as_arr)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(SlotPhase::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            verdict,
            disposition,
            prior_disposition: opt_str(dm, "prior_disposition"),
            repairs: dm
                .get("repairs")
                .and_then(Json::as_arr)
                .cloned()
                .unwrap_or_default(),
            integrity: SlotIntegrity {
                seal: str_member(im, "seal")?,
                seal_ref: opt_str(im, "seal_ref"),
                sentinel_intact: im.get("sentinel_intact").and_then(Json::as_bool),
                observer_survived: im.get("observer_survived").and_then(Json::as_bool),
                probe: opt_str(im, "probe"),
            },
            trajectory: tm.clone(),
            costs,
        })
    }

    /// The content address of this record's canonical bytes (the
    /// `slot_ref` spelling `slot:<idp>` — `hh-s4-slot/1`'s carry-over).
    pub fn slot_ref(&self) -> String {
        format!(
            "slot:{}",
            idp_id(
                "hh.experiment_slot",
                self.to_json().to_canonical_string().as_bytes()
            )
        )
    }
}
