//! The runner's declared run-input records — `hh-runner-drive/1` (the
//! coordinator's drive packet) and `hh-runner-slot/1` (the per-attempt
//! worker packet). Inputs are declared, never absolute paths (S-2 /
//! REQ-XA-010): every path member is interpreted relative to the packet's
//! own directory or the declared roots.

use std::collections::BTreeMap;

use hharness_lab::experiment::ExperimentSpec;
use hharness_wire::json::Json;

use crate::json_ext::JsonExt;

use crate::disposition::{AdmissionReason, EndCause, InfraCause};
use crate::facts::SlotFacts;

/// `hh-runner-drive/1` schema member.
pub const DRIVE_SCHEMA: &str = "hh-runner-drive/1";
/// `hh-runner-slot/1` schema member.
pub const SLOT_PACKET_SCHEMA: &str = "hh-runner-slot/1";

/// Packet decode error.
#[derive(Debug, Clone, PartialEq)]
pub struct PacketError {
    /// What failed.
    pub detail: String,
}

impl std::fmt::Display for PacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runner packet: {}", self.detail)
    }
}

impl std::error::Error for PacketError {}

fn err(d: impl Into<String>) -> PacketError {
    PacketError { detail: d.into() }
}

fn need<'a>(m: &'a BTreeMap<String, Json>, k: &str) -> Result<&'a Json, PacketError> {
    m.get(k).ok_or_else(|| err(format!("{k} required")))
}

fn need_str(m: &BTreeMap<String, Json>, k: &str) -> Result<String, PacketError> {
    need(m, k)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| err(format!("{k} must be a string")))
}

/// A fixture slot recipe — the declared slot-fact producer for the
/// fixture executor (each variant is one §3.3 row or a kill-point hold).
#[derive(Debug, Clone, PartialEq)]
pub enum SlotRecipe {
    /// A valid reward: `reward_ppm` with `end_cause` (`resolved` iff
    /// `reward_ppm > 0`).
    Scored {
        /// ppm of 1.0.
        reward_ppm: u64,
        /// Agent-phase end cause.
        end_cause: EndCause,
        /// Declared model-call consumption charged to the slot.
        consumed_model_calls: i64,
    },
    /// The verifier stays unavailable after `max_grade_attempts`.
    VerifierUnavailable {
        /// The `<c>` spelling.
        cause: String,
    },
    /// An infrastructure ending (§3.3 row 3).
    Infra {
        /// The `infra:<c>` cause.
        cause: InfraCause,
    },
    /// Premature slot budget refusal — precedence over a valid reward
    /// (RD-17); `reward_ppm` present ⇒ a reward was also produced.
    PrematureBudgetRefusal {
        /// The reward the slot produced anyway (never scored).
        reward_ppm: Option<u64>,
    },
    /// Relay death after an agent-issued signal (REQ-XB-129).
    RelayKilledByAgent {
        /// The recorded reward.
        reward_ppm: u64,
    },
    /// An ST0 admission refusal (§3.3 row 6).
    Admission {
        /// The `admission:<r>` reason.
        reason: AdmissionReason,
    },
    /// Operator cancel (§3.3 row 7).
    Cancelled,
    /// Hold inside the agent phase for `hold_ms` — the kill-point
    /// recipe: a worker killed while holding never seals.
    HoldAgent {
        /// How long the agent phase holds (ms).
        hold_ms: u64,
    },
    /// Seal the record, then hold `hold_ms` before exit — the
    /// kill-point recipe: a worker killed while holding sealed but was
    /// never reaped for bind.
    HoldAfterSeal {
        /// How long to hold after the seal (ms).
        hold_ms: u64,
        /// The recorded reward.
        reward_ppm: u64,
    },
}

impl SlotRecipe {
    /// The slot facts this recipe produces when not killed mid-flight
    /// (`Hold*` recipes never produce facts — they are kill points).
    pub fn facts(&self) -> Option<SlotFacts> {
        use crate::facts::{Resolution, RewardFact};
        let mut f = SlotFacts::default();
        match self {
            SlotRecipe::Scored {
                reward_ppm,
                end_cause,
                ..
            } => {
                f.reward = Some(RewardFact {
                    resolution: if *reward_ppm > 0 {
                        Resolution::Resolved
                    } else {
                        Resolution::Unresolved
                    },
                    end_cause: *end_cause,
                    reward_ppm: Some(*reward_ppm),
                    flags: vec![],
                });
            }
            SlotRecipe::VerifierUnavailable { cause } => {
                f.verifier_unavailable = Some(cause.clone());
            }
            SlotRecipe::Infra { cause } => {
                f.infra = Some(cause.clone());
            }
            SlotRecipe::PrematureBudgetRefusal { reward_ppm } => {
                f.premature_budget_refusal = true;
                if let Some(ppm) = reward_ppm {
                    f.reward = Some(RewardFact {
                        resolution: Resolution::Resolved,
                        end_cause: EndCause::Exited(0),
                        reward_ppm: Some(*ppm),
                        flags: vec![],
                    });
                }
            }
            SlotRecipe::RelayKilledByAgent { reward_ppm } => {
                f.relay_killed_by_agent = true;
                f.reward = Some(RewardFact {
                    resolution: if *reward_ppm > 0 {
                        Resolution::Resolved
                    } else {
                        Resolution::Unresolved
                    },
                    end_cause: EndCause::Crashed,
                    reward_ppm: Some(*reward_ppm),
                    flags: vec![crate::disposition::SlotFlag::RelayKilledByAgent],
                });
            }
            SlotRecipe::Admission { reason } => {
                f.admission_refusal = Some(*reason);
            }
            SlotRecipe::Cancelled => {
                f.cancelled = Some(hharness_ontology::control::CancelledBy::Operator);
            }
            SlotRecipe::HoldAgent { .. } => return None,
            SlotRecipe::HoldAfterSeal { reward_ppm, .. } => {
                f.reward = Some(RewardFact {
                    resolution: if *reward_ppm > 0 {
                        Resolution::Resolved
                    } else {
                        Resolution::Unresolved
                    },
                    end_cause: crate::disposition::EndCause::Exited(0),
                    reward_ppm: Some(*reward_ppm),
                    flags: vec![],
                });
            }
        }
        Some(f)
    }

    /// The recipe's canonical JSON (`{kind, …}`).
    pub fn to_json(&self) -> Json {
        let kv = |k: &str, v: Json| Json::obj([("kind", Json::str(k)), ("v", v)]);
        match self {
            SlotRecipe::Scored {
                reward_ppm,
                end_cause,
                consumed_model_calls,
            } => kv(
                "scored",
                Json::obj([
                    ("reward_ppm", Json::Int(*reward_ppm as i64)),
                    ("end_cause", Json::str(end_cause.as_str())),
                    ("consumed_model_calls", Json::Int(*consumed_model_calls)),
                ]),
            ),
            SlotRecipe::VerifierUnavailable { cause } => kv(
                "verifier_unavailable",
                Json::obj([("cause", Json::str(cause))]),
            ),
            SlotRecipe::Infra { cause } => {
                kv("infra", Json::obj([("cause", Json::str(cause.as_str()))]))
            }
            SlotRecipe::PrematureBudgetRefusal { reward_ppm } => kv(
                "premature_budget_refusal",
                match reward_ppm {
                    Some(p) => Json::obj([("reward_ppm", Json::Int(*p as i64))]),
                    None => Json::obj([]),
                },
            ),
            SlotRecipe::RelayKilledByAgent { reward_ppm } => kv(
                "relay_killed_by_agent",
                Json::obj([("reward_ppm", Json::Int(*reward_ppm as i64))]),
            ),
            SlotRecipe::Admission { reason } => kv(
                "admission",
                Json::obj([("reason", Json::str(reason.as_str()))]),
            ),
            SlotRecipe::Cancelled => kv("cancelled", Json::obj([])),
            SlotRecipe::HoldAgent { hold_ms } => kv(
                "hold_agent",
                Json::obj([("hold_ms", Json::Int(*hold_ms as i64))]),
            ),
            SlotRecipe::HoldAfterSeal {
                hold_ms,
                reward_ppm,
            } => kv(
                "hold_after_seal",
                Json::obj([
                    ("hold_ms", Json::Int(*hold_ms as i64)),
                    ("reward_ppm", Json::Int(*reward_ppm as i64)),
                ]),
            ),
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SlotRecipe, PacketError> {
        let m = j.as_obj().ok_or_else(|| err("recipe must be an object"))?;
        let kind = need_str(m, "kind")?;
        let v = need(m, "v")?
            .as_obj()
            .ok_or_else(|| err("recipe.v must be an object"))?;
        let uint = |k: &str| -> Result<u64, PacketError> {
            need(v, k)?
                .as_int()
                .map(|i| i as u64)
                .ok_or_else(|| err(format!("recipe.v.{k} must be an integer")))
        };
        Ok(match kind.as_str() {
            "scored" => SlotRecipe::Scored {
                reward_ppm: uint("reward_ppm")?,
                end_cause: EndCause::parse(
                    need(v, "end_cause")?
                        .as_str()
                        .ok_or_else(|| err("end_cause must be a string"))?,
                )
                .ok_or_else(|| err("end_cause outside the closed set"))?,
                consumed_model_calls: uint("consumed_model_calls")? as i64,
            },
            "verifier_unavailable" => SlotRecipe::VerifierUnavailable {
                cause: need(v, "cause")?
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| err("cause must be a string"))?,
            },
            "infra" => SlotRecipe::Infra {
                cause: InfraCause::parse(
                    need(v, "cause")?
                        .as_str()
                        .ok_or_else(|| err("cause must be a string"))?,
                )
                .map_err(|e| err(e.to_string()))?,
            },
            "premature_budget_refusal" => SlotRecipe::PrematureBudgetRefusal {
                reward_ppm: v.get("reward_ppm").and_then(Json::as_int).map(|i| i as u64),
            },
            "relay_killed_by_agent" => SlotRecipe::RelayKilledByAgent {
                reward_ppm: uint("reward_ppm")?,
            },
            "admission" => SlotRecipe::Admission {
                reason: AdmissionReason::parse(
                    need(v, "reason")?
                        .as_str()
                        .ok_or_else(|| err("reason must be a string"))?,
                )
                .map_err(|e| err(e.to_string()))?,
            },
            "cancelled" => SlotRecipe::Cancelled,
            "hold_agent" => SlotRecipe::HoldAgent {
                hold_ms: uint("hold_ms")?,
            },
            "hold_after_seal" => SlotRecipe::HoldAfterSeal {
                hold_ms: uint("hold_ms")?,
                reward_ppm: uint("reward_ppm")?,
            },
            other => return Err(err(format!("recipe kind {other} unknown"))),
        })
    }
}

/// `hh-runner-slot/1` — the per-attempt worker packet. `slot_root` names
/// the worker's own store root (declared, relative to the drive's
/// `slots_dir`).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotPacket {
    /// The plan key.
    pub run_plan_id: String,
    /// The subject run id in the experiment store.
    pub subject_run_id: String,
    /// The attempt ordinal.
    pub attempt_no: u32,
    /// The experiment content address.
    pub experiment_id: String,
    /// `protocol_id` for `identity` (declared input).
    pub protocol_id: String,
    /// `selection_id` for `identity` (declared input).
    pub selection_id: String,
    /// The cell id.
    pub cell_id: String,
    /// The arm.
    pub arm_id: String,
    /// The task.
    pub task_id: String,
    /// The replicate within the cell.
    pub replicate_index: u32,
    /// The split label.
    pub split_label: String,
    /// The slot store root (the worker's sole-writer root).
    pub slot_root: String,
    /// The slot run id inside the slot store (deterministic).
    pub slot_run_id: String,
    /// Declared task facts stamped into `task`.
    pub task: BTreeMap<String, Json>,
    /// Declared participant facts stamped into `participant`.
    pub participant: BTreeMap<String, Json>,
    /// Declared cell facts stamped into `cell`.
    pub cell: BTreeMap<String, Json>,
    /// The declared slot recipe.
    pub recipe: SlotRecipe,
}

impl SlotPacket {
    /// The canonical packet JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(SLOT_PACKET_SCHEMA)),
            ("run_plan_id", Json::str(&self.run_plan_id)),
            ("subject_run_id", Json::str(&self.subject_run_id)),
            ("attempt_no", Json::Int(self.attempt_no as i64)),
            ("experiment_id", Json::str(&self.experiment_id)),
            ("protocol_id", Json::str(&self.protocol_id)),
            ("selection_id", Json::str(&self.selection_id)),
            ("cell_id", Json::str(&self.cell_id)),
            ("arm_id", Json::str(&self.arm_id)),
            ("task_id", Json::str(&self.task_id)),
            ("replicate_index", Json::Int(self.replicate_index as i64)),
            ("split_label", Json::str(&self.split_label)),
            ("slot_root", Json::str(&self.slot_root)),
            ("slot_run_id", Json::str(&self.slot_run_id)),
            ("task", Json::Obj(self.task.clone())),
            ("participant", Json::Obj(self.participant.clone())),
            ("cell", Json::Obj(self.cell.clone())),
            ("recipe", self.recipe.to_json()),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SlotPacket, PacketError> {
        let m = j.as_obj().ok_or_else(|| err("packet must be an object"))?;
        if need_str(m, "schema")? != SLOT_PACKET_SCHEMA {
            return Err(err("schema member is not hh-runner-slot/1"));
        }
        let u32_member = |k: &str| -> Result<u32, PacketError> {
            need(m, k)?
                .as_int()
                .map(|i| i as u32)
                .ok_or_else(|| err(format!("{k} must be an integer")))
        };
        Ok(SlotPacket {
            run_plan_id: need_str(m, "run_plan_id")?,
            subject_run_id: need_str(m, "subject_run_id")?,
            attempt_no: u32_member("attempt_no")?,
            experiment_id: need_str(m, "experiment_id")?,
            protocol_id: need_str(m, "protocol_id")?,
            selection_id: need_str(m, "selection_id")?,
            cell_id: need_str(m, "cell_id")?,
            arm_id: need_str(m, "arm_id")?,
            task_id: need_str(m, "task_id")?,
            replicate_index: u32_member("replicate_index")?,
            split_label: need_str(m, "split_label")?,
            slot_root: need_str(m, "slot_root")?,
            slot_run_id: need_str(m, "slot_run_id")?,
            task: need(m, "task")?.as_obj().cloned().unwrap_or_default(),
            participant: need(m, "participant")?
                .as_obj()
                .cloned()
                .unwrap_or_default(),
            cell: need(m, "cell")?.as_obj().cloned().unwrap_or_default(),
            recipe: SlotRecipe::from_json(need(m, "recipe")?)?,
        })
    }
}

/// `hh-runner-drive/1` — the coordinator's declared run input: the spec
/// document, the resolver deposits (`budgets`, `suite_tasks`,
/// `arm_configs`), the fixture recipe table and the topology.
#[derive(Debug, Clone, PartialEq)]
pub struct DrivePacket {
    /// The `ExperimentSpec` document (canonical).
    pub spec: ExperimentSpec,
    /// `budget ref → BudgetSpec` deposits for `EngineContext::resolve_budget`.
    pub budgets: BTreeMap<String, hharness_budget::spec::BudgetSpec>,
    /// The `ExpandTask` rows (`task_id`, `split_label`).
    pub suite_tasks: Vec<(String, String)>,
    /// `arm_id → {configuration_id, configuration_version_id}` deposits.
    pub arm_configs: BTreeMap<String, (String, String)>,
    /// The fixture recipe table: `task_id → recipe` (a task not listed
    /// runs the default `scored{resolved, exited{0}}` recipe).
    pub recipes: BTreeMap<String, SlotRecipe>,
    /// `protocol_id`/`selection_id`/`epoch` stamps for `identity`.
    pub protocol_id: String,
    /// `selection_id`.
    pub selection_id: String,
    /// `identity.epoch`.
    pub epoch: String,
    /// Process-level slot parallelism bound (≥ 1).
    pub max_parallel_slots: u32,
    /// Engine writer/claim lease TTLs (ms) — declared so kill-point runs
    /// can shorten the stale-holder window.
    pub writer_ttl_ms: u64,
    /// The claim TTL (ms).
    pub claim_ttl_ms: u64,
    /// Declared `costs.model`/`costs.compute` fold references + expected
    /// fold values for the fixture fold-recompute leg (`{ref, fold}`).
    pub costs: BTreeMap<String, Json>,
    /// `window{…}` — the declared window scheduling inputs (R6.16;
    /// `hh-runner-window/1`). Absent = the unscheduled R6.8 drive.
    pub window: Option<crate::schedule::WindowDecl>,
    /// The podman binary for the own-label sweep (`podman_bin` —
    /// declared, never PATH-defaulted in a scheduled window; R6.16
    /// REQ-XB-127). Absent = no container sweep.
    pub podman_bin: Option<String>,
}

impl DrivePacket {
    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<DrivePacket, PacketError> {
        let m = j.as_obj().ok_or_else(|| err("packet must be an object"))?;
        if need_str(m, "schema")? != DRIVE_SCHEMA {
            return Err(err("schema member is not hh-runner-drive/1"));
        }
        let spec = ExperimentSpec::from_json(need(m, "experiment")?)
            .map_err(|e| err(format!("experiment decode: {e:?}")))?;
        let mut budgets = BTreeMap::new();
        for (k, v) in need(m, "budgets")?.as_obj().cloned().unwrap_or_default() {
            budgets.insert(
                k,
                hharness_budget::spec::BudgetSpec::from_json(&v)
                    .ok_or_else(|| err("budgets decode failed"))?,
            );
        }
        let mut suite_tasks = Vec::new();
        for t in need(m, "suite_tasks")?
            .as_arr()
            .cloned()
            .unwrap_or_default()
        {
            let tm = t.as_obj().ok_or_else(|| err("suite_tasks[] objects"))?;
            suite_tasks.push((
                tm.get("task_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| err("suite_tasks[].task_id"))?,
                tm.get("split_label")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| err("suite_tasks[].split_label"))?,
            ));
        }
        let mut arm_configs = BTreeMap::new();
        for (k, v) in need(m, "arm_configs")?
            .as_obj()
            .cloned()
            .unwrap_or_default()
        {
            let vm = v.as_obj().ok_or_else(|| err("arm_configs values"))?;
            arm_configs.insert(
                k,
                (
                    vm.get("configuration_id")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| err("configuration_id"))?,
                    vm.get("configuration_version_id")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| err("configuration_version_id"))?,
                ),
            );
        }
        let mut recipes = BTreeMap::new();
        for (k, v) in need(m, "recipes")?.as_obj().cloned().unwrap_or_default() {
            recipes.insert(k, SlotRecipe::from_json(&v)?);
        }
        let u64_or = |k: &str, d: u64| -> Result<u64, PacketError> {
            Ok(m.get(k)
                .and_then(Json::as_int)
                .map(|i| i as u64)
                .unwrap_or(d))
        };
        Ok(DrivePacket {
            spec,
            budgets,
            suite_tasks,
            arm_configs,
            recipes,
            protocol_id: need_str(m, "protocol_id")?,
            selection_id: need_str(m, "selection_id")?,
            epoch: need_str(m, "epoch")?,
            max_parallel_slots: u64_or("max_parallel_slots", 1)?.max(1) as u32,
            writer_ttl_ms: u64_or("writer_ttl_ms", 60_000)?,
            claim_ttl_ms: u64_or("claim_ttl_ms", 30_000)?,
            costs: m
                .get("costs")
                .and_then(Json::as_obj)
                .cloned()
                .unwrap_or_default(),
            window: m
                .get("window")
                .map(crate::schedule::WindowDecl::from_json)
                .transpose()?,
            podman_bin: m
                .get("podman_bin")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }
}
