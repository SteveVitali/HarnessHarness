//! `schedule` — the runner's declared window scheduling layer (manifest
//! 269/R6.16; RUNNER-CORE (c); spec §3.2/§3.3, §3.9 REQ-XB-128, §3.10
//! REQ-XB-214, §6.8 REQ-XD-060, §7.5 REQ-XD-111, §10.1 REQ-XH-125;
//! design/31 F17; rulings C-17/C-18).
//!
//! First-fit packing in the engine's recorded order under the declared
//! bounds and lookahead — packing changes start times, never assignment
//! (REQ-XB-121, absorbing REQ-XB-010). The C rule: `C = min(⌊(host
//! memory − reserve) / slot memory⌋, ⌊vCPU / slot cpus⌋, ⌈K/f⌉ for
//! agent phases)`; a trial's timeout clock never starts while C trials
//! run (REQ-XB-214). The two declared schedule kinds — seeded k-round
//! interleave and pair-coupled simultaneous launch in blocks — and
//! their composition, recomputed from the declared seed with a
//! catalog-identity snapshot at each block boundary (REQ-XH-017,
//! REQ-XD-111). The dead-man admission cut-off and the incremental
//! on-VM harvest manifest (REQ-XB-128); the same-window retry gate —
//! a retry scheduled after its window's end refuses and the slot stays
//! unresolved (REQ-XH-125); the experiment-label container sweep with a
//! teardown receipt per slot (REQ-XB-127).
//!
//! Every input here is declared (the drive packet's `window` member) or
//! read off durable plan facts — scheduling and cut-off read cost and
//! clock fields only, never outcomes (X-07). Reattempt arithmetic is
//! the engine's `ReattemptPolicy`; this module keeps no retry counter
//! (X-04/CC1, REQ-XH-016/REQ-XD-060).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;
use crate::packet::PacketError;

/// The window declaration's record spelling (drive packet `window`).
pub const WINDOW_SCHEMA: &str = "hh-runner-window/1";
/// The incremental harvest manifest's record spelling.
pub const HARVEST_SCHEMA: &str = "hh-runner-harvest/1";
/// The container label every experiment cell carries (REQ-XB-127).
pub const EXPERIMENT_LABEL: &str = "hh-experiment";
/// The slot-container name prefix — a teardown receipt binds its slot.
pub const SLOT_CONTAINER_PREFIX: &str = "hh-slot-";

fn werr(detail: impl Into<String>) -> PacketError {
    PacketError {
        detail: detail.into(),
    }
}

// ── the declared schedule kind (REQ-XH-017) ────────────────────────────

/// The declared schedule kind — `k_round_interleave` (seeded k-round
/// interleaving), `pair_coupled` (simultaneous O/H launch in blocks) or
/// `composed` (a k-round interleaving whose O/H pairs are coupled
/// units; REQ-XH-017's composition, C-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    /// Seeded k-round interleave — each unit is one plan.
    KRoundInterleave,
    /// Pair-coupled simultaneous launch in declared blocks — each unit
    /// is one (task, replicate) O/H pair.
    PairCoupled,
    /// The composition — coupled pairs interleaved across k rounds.
    Composed,
}

impl ScheduleKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduleKind::KRoundInterleave => "k_round_interleave",
            ScheduleKind::PairCoupled => "pair_coupled",
            ScheduleKind::Composed => "composed",
        }
    }
    /// Strict parse.
    pub fn parse(s: &str) -> Result<ScheduleKind, PacketError> {
        match s {
            "k_round_interleave" => Ok(ScheduleKind::KRoundInterleave),
            "pair_coupled" => Ok(ScheduleKind::PairCoupled),
            "composed" => Ok(ScheduleKind::Composed),
            other => Err(werr(format!(
                "schedule.kind {other} outside the declared set"
            ))),
        }
    }
}

/// The declared schedule member (`window.schedule`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleDecl {
    /// The kind.
    pub kind: ScheduleKind,
    /// k — the replicate rounds (`k_round_interleave`/`composed`).
    pub rounds: u32,
    /// The declared block count for `pair_coupled` (`composed`/`k_round`
    /// block on the round boundary; REQ-XD-111's snapshot rows are
    /// `blocks + 1`).
    pub blocks: u32,
    /// The order seed — the whole order recomputes from it.
    pub seed: String,
    /// The catalog identity re-probed at each block boundary
    /// (REQ-XD-111; the proxy's `catalog_boundary` op when supervised).
    pub catalog_id: String,
}

/// `window{…}` — the declared window inputs (every member records-in:
/// no silent defaults — the contract's ambiguity note resolves
/// `declared_harvest_ms` to this required member).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowDecl {
    /// The window id (`W2`, `W4`, `Wc`).
    pub window_id: String,
    /// The window start on the runner's clock (ms).
    pub started_ms: u64,
    /// D — the dead-man stop (ms after `started_ms`; `None` = unbounded).
    pub dead_man_ms: Option<u64>,
    /// The declared harvest time (ms) inside the cut-off (§3.9; the
    /// declaration names it — never a silent default).
    pub declared_harvest_ms: u64,
    /// The largest slot deadline (agent + verifier, ms) a candidate
    /// slot carries into the in-flight set for the cut-off.
    pub slot_deadline_ms: u64,
    /// Host memory (MiB) the C rule packs against.
    pub host_memory_mb: u64,
    /// The declared memory reserve (MiB).
    pub memory_reserve_mb: u64,
    /// The vCPU budget.
    pub vcpu_budget: u64,
    /// Per-slot memory (MiB).
    pub slot_memory_mb: u64,
    /// Per-slot cpus.
    pub slot_cpus: u64,
    /// The window runs an agent phase (the ⌈K/f⌉ leg applies).
    pub agent_phase: bool,
    /// K — the model account's in-flight limit.
    pub model_in_flight_limit: u64,
    /// f — the measured or prior model-wait fraction (ppm of 1.0;
    /// `0` declares no model gate — ⌈K/f⌉ is unbounded then).
    pub model_wait_fraction_ppm: u64,
    /// The declared lookahead `w` — a later unit may start ahead of a
    /// waiting one by at most `w` positions.
    pub lookahead: u64,
    /// The declared schedule.
    pub schedule: ScheduleDecl,
}

fn u64_member(m: &BTreeMap<String, Json>, k: &str) -> Result<u64, PacketError> {
    m.get(k)
        .and_then(Json::as_int)
        .map(|v| v as u64)
        .ok_or_else(|| werr(format!("window.{k} must be an integer")))
}

impl WindowDecl {
    /// Strict decode of the drive packet's `window` member.
    pub fn from_json(j: &Json) -> Result<WindowDecl, PacketError> {
        let m = j.as_obj().ok_or_else(|| werr("window must be an object"))?;
        for k in m.keys() {
            if ![
                "schema",
                "window_id",
                "started_ms",
                "dead_man_ms",
                "declared_harvest_ms",
                "slot_deadline_ms",
                "host_memory_mb",
                "memory_reserve_mb",
                "vcpu_budget",
                "slot_memory_mb",
                "slot_cpus",
                "agent_phase",
                "model_in_flight_limit",
                "model_wait_fraction_ppm",
                "lookahead",
                "schedule",
            ]
            .contains(&k.as_str())
            {
                return Err(werr(format!("window: unknown member {k}")));
            }
        }
        let sm = m
            .get("schedule")
            .and_then(JsonExt::as_obj)
            .ok_or_else(|| werr("window.schedule must be an object"))?;
        for k in sm.keys() {
            if !["kind", "rounds", "blocks", "seed", "catalog_id"].contains(&k.as_str()) {
                return Err(werr(format!("window.schedule: unknown member {k}")));
            }
        }
        let kind = ScheduleKind::parse(
            sm.get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| werr("window.schedule.kind required"))?,
        )?;
        let rounds = sm
            .get("rounds")
            .and_then(Json::as_int)
            .map(|v| v as u32)
            .unwrap_or(1)
            .max(1);
        let blocks = match sm.get("blocks").and_then(Json::as_int) {
            Some(b) => (b as u32).max(1),
            None if kind == ScheduleKind::PairCoupled => 1,
            None => rounds,
        };
        Ok(WindowDecl {
            window_id: m
                .get("window_id")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| werr("window.window_id required"))?,
            started_ms: u64_member(m, "started_ms")?,
            dead_man_ms: m
                .get("dead_man_ms")
                .and_then(Json::as_int)
                .map(|v| v as u64),
            declared_harvest_ms: u64_member(m, "declared_harvest_ms")?,
            slot_deadline_ms: u64_member(m, "slot_deadline_ms")?,
            host_memory_mb: u64_member(m, "host_memory_mb")?,
            memory_reserve_mb: u64_member(m, "memory_reserve_mb")?,
            vcpu_budget: u64_member(m, "vcpu_budget")?,
            slot_memory_mb: u64_member(m, "slot_memory_mb")?,
            slot_cpus: u64_member(m, "slot_cpus")?,
            agent_phase: m
                .get("agent_phase")
                .and_then(JsonExt::as_bool)
                .unwrap_or(false),
            model_in_flight_limit: u64_member(m, "model_in_flight_limit")?,
            model_wait_fraction_ppm: u64_member(m, "model_wait_fraction_ppm")?,
            lookahead: u64_member(m, "lookahead")?,
            schedule: ScheduleDecl {
                kind,
                rounds,
                blocks,
                seed: sm
                    .get("seed")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| werr("window.schedule.seed required"))?,
                catalog_id: sm
                    .get("catalog_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .unwrap_or_default(),
            },
        })
    }

    /// `C` — REQ-XB-214's admitted concurrency: `min(⌊(host−reserve)
    /// / slot_mem⌋, ⌊vcpu / slot_cpus⌋, ⌈K·10⁶/f_ppm⌉ for agent
    /// phases)`. At least 1 — a declared window admits its serial order
    /// even under degenerate bounds; 0 only when a declared leg is 0.
    pub fn concurrency_c(&self) -> u64 {
        let mem = (self.host_memory_mb.saturating_sub(self.memory_reserve_mb))
            / self.slot_memory_mb.max(1);
        let cpu = self.vcpu_budget / self.slot_cpus.max(1);
        let model = if self.agent_phase && self.model_wait_fraction_ppm > 0 {
            self.model_in_flight_limit
                .saturating_mul(1_000_000)
                .div_ceil(self.model_wait_fraction_ppm)
        } else {
            u64::MAX
        };
        mem.min(cpu).min(model).max(1)
    }

    /// The `c_inputs` record member — every term of the C rule lands in
    /// the window record (AC-3.10-2).
    pub fn c_inputs(&self) -> Json {
        Json::obj([
            ("host_memory_mb", Json::Int(self.host_memory_mb as i64)),
            (
                "memory_reserve_mb",
                Json::Int(self.memory_reserve_mb as i64),
            ),
            ("vcpu_budget", Json::Int(self.vcpu_budget as i64)),
            ("slot_memory_mb", Json::Int(self.slot_memory_mb as i64)),
            ("slot_cpus", Json::Int(self.slot_cpus as i64)),
            ("agent_phase", Json::Bool(self.agent_phase)),
            (
                "model_in_flight_limit",
                Json::Int(self.model_in_flight_limit as i64),
            ),
            (
                "model_wait_fraction_ppm",
                Json::Int(self.model_wait_fraction_ppm as i64),
            ),
            ("c", Json::Int(self.concurrency_c() as i64)),
        ])
    }

    /// `admit(now, in_flight_deadlines)` — the dead-man cut-off
    /// (REQ-XB-128): a slot may start only while
    /// `now + max(in_flight ∪ candidate deadlines) + declared_harvest ≤
    /// started + D`. `None` D admits always. The candidate's own
    /// deadline joins the set before the comparison — a slot that
    /// could not finish plus harvest is never admitted.
    pub fn admit(&self, now_ms: u64, mut in_flight_deadline_ms: Vec<u64>) -> bool {
        let Some(d) = self.dead_man_ms else {
            return true;
        };
        in_flight_deadline_ms.push(self.slot_deadline_ms);
        let largest = in_flight_deadline_ms.iter().copied().max().unwrap_or(0);
        now_ms
            .saturating_add(largest)
            .saturating_add(self.declared_harvest_ms)
            <= self.started_ms.saturating_add(d)
    }

    /// The cut-off instant — `started + D` (informational member of the
    /// window record; [`Self::admit`] is the decision).
    pub fn window_end_ms(&self) -> Option<u64> {
        self.dead_man_ms.map(|d| self.started_ms.saturating_add(d))
    }

    /// The window record doc — `hh-runner-window/1` (carries the C
    /// inputs, the packing rule and the declared schedule; the start
    /// order lands in `schedule_rows`/`start_order` appended at close).
    pub fn record_json(&self, c: u64) -> Json {
        Json::obj([
            ("schema", Json::str(WINDOW_SCHEMA)),
            ("window_id", Json::str(&self.window_id)),
            ("started_ms", Json::Int(self.started_ms as i64)),
            (
                "dead_man_ms",
                self.dead_man_ms
                    .map(|v| Json::Int(v as i64))
                    .unwrap_or(Json::Null),
            ),
            (
                "declared_harvest_ms",
                Json::Int(self.declared_harvest_ms as i64),
            ),
            ("slot_deadline_ms", Json::Int(self.slot_deadline_ms as i64)),
            ("c_inputs", self.c_inputs()),
            ("c", Json::Int(c as i64)),
            (
                "packing",
                Json::obj([
                    ("rule", Json::str("first_fit_recorded_order")),
                    ("lookahead", Json::Int(self.lookahead as i64)),
                ]),
            ),
            (
                "schedule",
                Json::obj([
                    ("kind", Json::str(self.schedule.kind.as_str())),
                    ("rounds", Json::Int(self.schedule.rounds as i64)),
                    ("blocks", Json::Int(self.schedule.blocks as i64)),
                    ("seed", Json::str(&self.schedule.seed)),
                    ("catalog_id", Json::str(&self.schedule.catalog_id)),
                ]),
            ),
        ])
    }
}

// ── plans, units, the seeded order ─────────────────────────────────────

/// The plan facts the schedule reads (the engine's recorded members —
/// `order_pos` is the recorded-order coordinate; assignment never
/// changes).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanFact {
    /// The plan key.
    pub run_plan_id: String,
    /// The cell id.
    pub cell_id: String,
    /// The arm.
    pub arm_id: String,
    /// The task.
    pub task_id: String,
    /// The replicate within the cell.
    pub replicate_index: u32,
    /// The split label (carried to the slot packet).
    pub split_label: String,
    /// The engine's recorded order position.
    pub order_pos: u64,
}

/// One packing unit — the members launch in one packing step on the
/// same VM and proxy (a coupled O/H pair) or singly (REQ-XH-017).
#[derive(Debug, Clone, PartialEq)]
pub struct PackingUnit {
    /// The unit's plan keys, in within-unit launch order.
    pub members: Vec<String>,
    /// The block the unit belongs to (round index for the round kinds;
    /// the declared block for `pair_coupled`).
    pub block: u32,
}

fn seeded_key(seed: &str, parts: &[&str]) -> String {
    sha256_hex(format!("{seed}|{}", parts.join("|")).as_bytes())
}

/// `build_schedule(plans, decl) → units` — the whole order recomputes
/// from the declared seed (C-18): plans group into rounds by
/// `replicate_index`; a `pair_coupled` unit is one (task, replicate)
/// O/H pair; `composed` couples pairs inside each round. Within-unit
/// member order is the engine's recorded order.
pub fn build_schedule(plans: &[PlanFact], decl: &ScheduleDecl) -> Vec<PackingUnit> {
    let mut by_order: Vec<&PlanFact> = plans.iter().collect();
    by_order.sort_by_key(|p| p.order_pos);
    let pair_units = |round_plans: &[&PlanFact], block: u32| -> Vec<PackingUnit> {
        let mut groups: BTreeMap<(String, u32), Vec<&PlanFact>> = BTreeMap::new();
        for p in round_plans {
            groups
                .entry((p.task_id.clone(), p.replicate_index))
                .or_default()
                .push(p);
        }
        let mut units: Vec<(String, PackingUnit)> = groups
            .into_iter()
            .map(|((t, r), mut ps)| {
                ps.sort_by_key(|p| p.order_pos);
                (
                    seeded_key(&decl.seed, &[&t, &r.to_string()]),
                    PackingUnit {
                        members: ps.iter().map(|p| p.run_plan_id.clone()).collect(),
                        block,
                    },
                )
            })
            .collect();
        units.sort_by(|a, b| a.0.cmp(&b.0));
        units.into_iter().map(|(_, u)| u).collect()
    };
    match decl.kind {
        ScheduleKind::PairCoupled => {
            let units = pair_units(&by_order, 0);
            // Declared blocks — contiguous chunks of the seeded order.
            let per = units.len().div_ceil(decl.blocks as usize).max(1);
            units
                .into_iter()
                .enumerate()
                .map(|(i, mut u)| {
                    u.block = (i / per) as u32;
                    u
                })
                .collect()
        }
        _ => {
            // k rounds by replicate index; `composed` couples within the
            // round, `k_round_interleave` leaves singleton units.
            let mut rounds: BTreeMap<u32, Vec<&PlanFact>> = BTreeMap::new();
            for p in by_order {
                rounds.entry(p.replicate_index).or_default().push(p);
            }
            let mut units = Vec::new();
            for (block, ps) in rounds {
                if decl.kind == ScheduleKind::Composed {
                    units.extend(pair_units(&ps, block));
                } else {
                    let mut keyed: Vec<(String, &PlanFact)> = ps
                        .iter()
                        .map(|p| (seeded_key(&decl.seed, &[&p.run_plan_id]), *p))
                        .collect();
                    keyed.sort_by(|a, b| a.0.cmp(&b.0));
                    units.extend(keyed.into_iter().map(|(_, p)| PackingUnit {
                        members: vec![p.run_plan_id.clone()],
                        block,
                    }));
                }
            }
            units
        }
    }
}

/// The in-flight resource usage the packing bounds charge against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceUse {
    /// Σ in-flight memory (MiB).
    pub memory_mb: u64,
    /// Σ in-flight cpus.
    pub cpus: u64,
    /// Σ in-flight model-pool slots (agent-phase consumers).
    pub model_slots: u64,
}

/// The packing bounds (B3-declared: memory, CPU, the model pool).
#[derive(Debug, Clone, PartialEq)]
pub struct PackingBounds {
    /// The memory ceiling (MiB).
    pub memory_mb: u64,
    /// The cpu ceiling.
    pub cpus: u64,
    /// The model-pool concurrent limit.
    pub model_pool: u64,
}

/// `first_fit(candidates, in_flight, bounds, lookahead) → startable
/// indices` — first-fit in the candidates' order: a candidate starts
/// when its resources fit the remaining capacity and it is within
/// `lookahead` positions of the first candidate that did not fit.
/// Packing changes start times only — the candidates' order is the
/// engine's recorded order (REQ-XB-121; design/31 F17).
pub fn first_fit(
    candidates: &[ResourceUse],
    in_flight: &ResourceUse,
    bounds: &PackingBounds,
    lookahead: u64,
) -> Vec<usize> {
    let mut used = in_flight.clone();
    let mut out = Vec::new();
    let mut first_blocked: Option<usize> = None;
    for (i, c) in candidates.iter().enumerate() {
        if let Some(b) = first_blocked {
            if (i - b) as u64 > lookahead {
                break;
            }
        }
        let fits = used.memory_mb.saturating_add(c.memory_mb) <= bounds.memory_mb
            && used.cpus.saturating_add(c.cpus) <= bounds.cpus
            && used.model_slots.saturating_add(c.model_slots) <= bounds.model_pool;
        if fits && first_blocked.is_none_or(|b| (i - b) as u64 <= lookahead) {
            used.memory_mb = used.memory_mb.saturating_add(c.memory_mb);
            used.cpus = used.cpus.saturating_add(c.cpus);
            used.model_slots = used.model_slots.saturating_add(c.model_slots);
            out.push(i);
        } else {
            first_blocked.get_or_insert(i);
        }
    }
    out
}

// ── the incremental harvest manifest (REQ-XB-128) ─────────────────────

/// `hh-runner-harvest/1` — the on-VM incremental harvest manifest: one
/// `slots[]` row lands per settled slot (the dead-man stop strands no
/// settled slot — the manifest covers every settle that already
/// happened, never a predicted one).
#[derive(Debug, Clone, Default)]
pub struct HarvestManifest {
    /// The settled-slot rows, appended in settle order.
    pub slots: Vec<Json>,
}

impl HarvestManifest {
    /// Record one settled slot `{run_plan_id, slot_run_id, attempt_no,
    /// outcome_class, teardown?}`.
    pub fn record(
        &mut self,
        run_plan_id: &str,
        slot_run_id: &str,
        attempt_no: u32,
        outcome_class: &str,
        teardown: Option<&Json>,
    ) {
        let mut row = vec![
            ("run_plan_id", Json::str(run_plan_id)),
            ("slot_run_id", Json::str(slot_run_id)),
            ("attempt_no", Json::Int(attempt_no as i64)),
            ("outcome_class", Json::str(outcome_class)),
        ];
        if let Some(t) = teardown {
            row.push(("teardown", t.clone()));
        }
        self.slots.push(Json::obj(row));
    }

    /// `covers(settled)` — the manifest names every settled slot id.
    pub fn covers(&self, settled: &[String]) -> bool {
        let have: Vec<&str> = self
            .slots
            .iter()
            .filter_map(|r| r.get("run_plan_id").and_then(Json::as_str))
            .collect();
        settled.iter().all(|s| have.contains(&s.as_str()))
    }

    /// The manifest document (written under the declared window root
    /// after each settle — incremental by rewrite of the durable list).
    pub fn to_json(&self, window_id: &str, torn_down_at_ms: Option<u64>) -> Json {
        Json::obj([
            ("schema", Json::str(HARVEST_SCHEMA)),
            ("window_id", Json::str(window_id)),
            ("slots", Json::Arr(self.slots.clone())),
            (
                "harvested_at_ms",
                torn_down_at_ms
                    .map(|v| Json::Int(v as i64))
                    .unwrap_or(Json::Null),
            ),
        ])
    }

    /// Write the manifest — the incremental on-VM artifact.
    pub fn persist(
        &self,
        path: &Path,
        window_id: &str,
        at_ms: Option<u64>,
    ) -> Result<(), PacketError> {
        std::fs::write(
            path,
            self.to_json(window_id, at_ms)
                .to_canonical_string()
                .as_bytes(),
        )
        .map_err(|e| werr(format!("harvest manifest write: {e}")))
    }
}

// ── the experiment-label sweep + teardown receipt (REQ-XB-127) ─────────

/// The own-label filter — `sweep_targets` keeps exactly the containers
/// whose `hh-experiment` label equals the window's experiment id; a
/// foreign or unlabelled container is never a target (AC-3.2-6).
pub fn sweep_targets(listed: &[(String, String)], experiment_id: &str) -> Vec<String> {
    listed
        .iter()
        .filter(|(_, label)| label == experiment_id)
        .map(|(id, _)| id.clone())
        .collect()
}

/// `podman ps -a --format` — every container's `{{.ID}}` and its
/// `hh-experiment` label value (the runner filters; the stub prints
/// all rows — the own-label rule is the runner's, never delegated).
pub fn sweep_list_argv() -> Vec<String> {
    vec![
        "ps".to_string(),
        "-a".to_string(),
        "--format".to_string(),
        "{{.ID}} {{index .Labels \"hh-experiment\"}}".to_string(),
    ]
}

/// The removal argv for one target.
pub fn sweep_rm_argv(container_id: &str) -> Vec<String> {
    vec!["rm".to_string(), "-f".to_string(), container_id.to_string()]
}

/// `sweep_experiment(podman, experiment_id) → teardown receipts` — list,
/// filter to the own label, `rm -f` each target, and record a
/// `hh-runner-teardown/1`-shaped receipt per removed container (the
/// slot id decodes from the `hh-slot-` name prefix when present).
pub fn sweep_experiment(podman: &Path, experiment_id: &str) -> Result<Vec<Json>, PacketError> {
    let out = Command::new(podman)
        .args(sweep_list_argv())
        .output()
        .map_err(|e| werr(format!("sweep list: {e}")))?;
    if !out.status.success() {
        return Err(werr(format!("sweep list: status {:?}", out.status.code())));
    }
    let listed: Vec<(String, String)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next()) {
                (Some(id), rest) => Some((id.to_string(), rest.unwrap_or("").to_string())),
                _ => None,
            }
        })
        .collect();
    let mut receipts = Vec::new();
    for id in sweep_targets(&listed, experiment_id) {
        let rm = Command::new(podman)
            .args(sweep_rm_argv(&id))
            .output()
            .map_err(|e| werr(format!("sweep rm: {e}")))?;
        receipts.push(Json::obj([
            ("container_id", Json::str(&id)),
            ("experiment_label", Json::str(experiment_id)),
            ("removed", Json::Bool(rm.status.success())),
        ]));
    }
    Ok(receipts)
}
