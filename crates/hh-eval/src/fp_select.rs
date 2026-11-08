//! `fp_select` — the first-pass SELECT commit (manifest 284/R6.30,
//! Stage 2; spec §10.1.2; REQ-XD-010, REQ-XD-012, REQ-XD-014,
//! REQ-XH-116, REQ-XH-111).
//!
//! Two committed records and the derivation that binds them to the
//! single [`crate::task_selection`] materializer (CC1 — no second
//! selection engine exists):
//!
//! - [`ExclusionRecord`] — `hh-fp-exclusion-record/1`: the
//!   outcome-blind exclusion record committed *before* the split.
//!   All 89 nominal TB 2.1 tasks with declared fields; the two QEMU
//!   tasks stay nominal `runner_infeasible` in every denominator
//!   (Q-X31); the five download-requiring tasks carry their first-pass
//!   disposition (`excluded_if_drawn_and_replaced`) and the
//!   `allowlist_extended` covariate (REQ-XD-014). Every exclusion is
//!   named and reasoned on a
//!   [`crate::central_protocol::OUTCOME_BLIND_EXCLUSION_BASES`] basis.
//! - [`FpSelectCommit`] — `hh-fp-select/1`: the commit binding the
//!   exclusion record and the `task-selection/1` record by sha256,
//!   carrying the declared seed, the resolved draws (held-out 10,
//!   development 7, the 3 smoke tasks), the within-cell successor
//!   heads, the adjacent-cell replacement orders (REQ-XH-127's
//!   dev-cell depletion rule), the drawn-exclusion replacement log and
//!   the W1 re-proof scope (10 drawn + one successor per difficulty
//!   cell at R = 3, 7 development tasks at R = 2).
//!
//! [`build_selection_decl`] derives the materializer input from the
//! exclusion record — the one derivation, so the commit, the tests and
//! FP-PREREG all recompute the same `selection_id` byte-identically
//! from the committed seed.

use std::collections::BTreeMap;

use hharness_wire::Json;

use crate::central_protocol::OUTCOME_BLIND_EXCLUSION_BASES;
use crate::task_selection::{
    materialize_selection, DatasetPin, DrawDecl, DrawPool, ExclusionRow, Purpose, SelectionDecl,
    SelectionError, StratifiedDraw, TaskRow, TaskSelection, DEV_POOL_TASKS, DIFFICULTIES,
    OUTCOME_FIELD_NAMES, STRATIFICATION,
};

/// `hh-fp-exclusion-record/1`.
pub const EXCLUSION_SCHEMA: &str = "hh-fp-exclusion-record/1";
/// `hh-fp-select/1`.
pub const SELECT_SCHEMA: &str = "hh-fp-select/1";

/// The nominal population (Q-X31: every nominal task in every
/// denominator).
pub const NOMINAL_TASKS: u32 = 89;
/// The TB 2.1 git pin (§10.1.2's three-part pin, verbatim).
pub const GIT_COMMIT: &str = "harbor-framework/terminal-bench-2-1@12b75656";
/// The Hub dataset ref (the spec's committed spelling, verbatim).
pub const HUB_DATASET_REF: &str = "terminal-bench/terminal-bench-2-1@sha256:7d7bdc1c";
/// The declared, outcome-blind first-pass selection seed — fixed at
/// contract time; no outcome field exists or was consulted.
pub const FP_SEED: &str = "fp-select-r6.30-2026-11-08";
/// The per-task `dataset.toml` digest marker — the real digests bind
/// at W1's observe-pass ingest (`hh-harbor-ingest/1`, R6.32) before
/// FP-PREREG freezes the post-replacement list. A declared `pending`
/// marker, never an invented digest (R6.17's `pending{first_build_W1}`
/// idiom).
pub const DIGEST_PENDING: &str = "pending{w1_observe_ingest}";

/// The held-out draw's name and size (§10.1.2: band-A 1/7/2).
pub const HELD_DRAW: (&str, u32) = ("fp_held10", 10);
/// The development draw's name and size (§10.1.2: band-A 1/4/2; the
/// smoke set's three tasks come from it).
pub const DEV_DRAW: (&str, u32) = ("fp_dev7", 7);

/// A nominal task's disposition in the exclusion record.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskDisposition {
    /// Eligible — enters the census.
    Eligible,
    /// Pre-registered outcome-blind infeasibility — nominal but out of
    /// the eligible census (the QEMU pair).
    RunnerInfeasible,
}

impl TaskDisposition {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskDisposition::Eligible => "eligible",
            TaskDisposition::RunnerInfeasible => "runner_infeasible",
        }
    }
}

/// One nominal census row — declared `task.toml` fields + disposition.
#[derive(Debug, Clone, PartialEq)]
pub struct NominalTask {
    /// The task id.
    pub task_id: String,
    /// Declared `[metadata].difficulty`.
    pub difficulty: String,
    /// Declared `[agent].timeout_sec`.
    pub agent_timeout_sec: u64,
    /// HF-saturation reporting covariate (never a selection input).
    pub hf_saturated: bool,
    /// The pre-split disposition.
    pub disposition: TaskDisposition,
    /// The recorded reason (`runner_infeasible` requires one).
    pub reason: Option<String>,
}

/// One download-requiring task's first-pass declaration (REQ-XD-014).
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadTask {
    /// The task id.
    pub task_id: String,
    /// The declared first-pass disposition —
    /// `excluded_if_drawn_and_replaced` (Q-X55 grants no extension).
    pub first_pass_disposition: String,
    /// The declared covariate — `allowlist_extended`.
    pub covariate: String,
    /// The recorded reason.
    pub reason: String,
}

/// One named exclusion row carried on the record.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedExclusion {
    /// The task id.
    pub task_id: String,
    /// The outcome-blind basis —
    /// [`crate::central_protocol::OUTCOME_BLIND_EXCLUSION_BASES`].
    pub basis: String,
    /// The recorded reason.
    pub reason: String,
}

/// The decoded `hh-fp-exclusion-record/1`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionRecord {
    /// The record timestamp (predates the split's).
    pub recorded_at: String,
    /// All 89 nominal tasks.
    pub tasks: Vec<NominalTask>,
    /// The named, reasoned exclusions.
    pub exclusions: Vec<RecordedExclusion>,
    /// The download-task declarations.
    pub download_tasks: Vec<DownloadTask>,
}

/// The closed refusal set.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectError {
    /// A required member absent.
    MissingKey {
        /// The member path.
        member: String,
    },
    /// A shape failure.
    Schema {
        /// The verbatim detail.
        detail: String,
    },
    /// The nominal population is not the declared 89.
    NominalMismatch {
        /// The declared vs. decoded count.
        have: usize,
    },
    /// An outcome field in the exclusion record — the record must be
    /// outcome-blind (REQ-XD-010).
    OutcomeField {
        /// The refused member name.
        member: String,
    },
    /// An exclusion outside the outcome-blind basis set.
    UnknownBasis {
        /// The rejected basis.
        basis: String,
    },
    /// An exclusion or download task not in the nominal population.
    UnknownTask {
        /// The rejected task id.
        task_id: String,
    },
    /// An unnamed/unreasoned exclusion or a `runner_infeasible` row
    /// without its reason.
    Unreasoned {
        /// The task id.
        task_id: String,
    },
    /// A task's disposition disagrees with the exclusion list, or a
    /// download declaration is malformed.
    DispositionMismatch {
        /// The verbatim detail.
        detail: String,
    },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::MissingKey { member } => {
                write!(f, "missing required member: {member}")
            }
            SelectError::Schema { detail } => write!(f, "{EXCLUSION_SCHEMA}: {detail}"),
            SelectError::NominalMismatch { have } => {
                write!(f, "nominal population is {have}, not {NOMINAL_TASKS}")
            }
            SelectError::OutcomeField { member } => {
                write!(
                    f,
                    "refused{{outcome_field}}: {member} — the exclusion record is outcome-blind"
                )
            }
            SelectError::UnknownBasis { basis } => {
                write!(f, "exclusion basis {basis} is not outcome-blind")
            }
            SelectError::UnknownTask { task_id } => {
                write!(f, "{task_id} is not a nominal TB 2.1 task")
            }
            SelectError::Unreasoned { task_id } => {
                write!(f, "{task_id} carries no recorded reason")
            }
            SelectError::DispositionMismatch { detail } => write!(f, "{detail}"),
        }
    }
}
impl std::error::Error for SelectError {}

fn schema(detail: impl Into<String>) -> SelectError {
    SelectError::Schema {
        detail: detail.into(),
    }
}

fn member<'a>(j: &'a Json, k: &str) -> Result<&'a Json, SelectError> {
    match j {
        Json::Obj(m) => m.get(k).ok_or(SelectError::MissingKey {
            member: k.to_string(),
        }),
        _ => Err(schema("record member must be an object")),
    }
}

fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, SelectError> {
    member(j, k)?
        .as_str()
        .ok_or_else(|| schema(format!("{k} must be a string")))
}

fn req_bool(j: &Json, k: &str) -> Result<bool, SelectError> {
    match member(j, k)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(schema(format!("{k} must be a bool"))),
    }
}

fn req_arr<'a>(j: &'a Json, k: &str) -> Result<&'a Vec<Json>, SelectError> {
    match member(j, k)? {
        Json::Arr(a) => Ok(a),
        _ => Err(schema(format!("{k} must be an array"))),
    }
}

fn req_u64(j: &Json, k: &str) -> Result<u64, SelectError> {
    match member(j, k)? {
        Json::Int(i) if *i >= 0 => Ok(*i as u64),
        _ => Err(schema(format!("{k} must be a non-negative integer"))),
    }
}

/// Scan for outcome members — the exclusion record must be
/// outcome-blind (REQ-XD-010's predicate, mechanical).
fn refuse_outcome_fields(j: &Json) -> Result<(), SelectError> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if OUTCOME_FIELD_NAMES.contains(&k.as_str()) {
                    return Err(SelectError::OutcomeField { member: k.clone() });
                }
                refuse_outcome_fields(v)?;
            }
            Ok(())
        }
        Json::Arr(a) => {
            for v in a {
                refuse_outcome_fields(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

impl ExclusionRecord {
    /// Strict decode + the REQ-XD-010/014 validations: the 89 nominal
    /// tasks, every exclusion named and reasoned on an outcome-blind
    /// basis, the QEMU pair consistent between the census rows and the
    /// exclusion list, the download declarations declared before
    /// selection with the `allowlist_extended` covariate.
    pub fn from_json(j: &Json) -> Result<ExclusionRecord, SelectError> {
        refuse_outcome_fields(j)?;
        let schema_v = req_str(j, "schema")?;
        if schema_v != EXCLUSION_SCHEMA {
            return Err(schema(format!(
                "schema {schema_v} is not {EXCLUSION_SCHEMA}"
            )));
        }
        let recorded_at = req_str(j, "recorded_at")?.to_string();

        let nominal = {
            let pop = member(j, "population")?;
            req_u64(pop, "nominal_tasks")? as usize
        };
        if nominal != NOMINAL_TASKS as usize {
            return Err(SelectError::NominalMismatch { have: nominal });
        }

        let mut tasks = Vec::new();
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for t in req_arr(j, "tasks")? {
            let task_id = req_str(t, "task_id")?.to_string();
            if seen.insert(task_id.clone(), ()).is_some() {
                return Err(schema(format!("task {task_id} twice")));
            }
            let difficulty = req_str(t, "difficulty")?.to_string();
            if !DIFFICULTIES.contains(&difficulty.as_str()) {
                return Err(schema(format!(
                    "task {task_id} difficulty {difficulty} is not declared"
                )));
            }
            let agent_timeout_sec = req_u64(t, "agent_timeout_sec")?;
            if agent_timeout_sec == 0 {
                return Err(schema(format!("task {task_id} has no declared timeout")));
            }
            let hf_saturated = req_bool(t, "hf_saturated")?;
            let disposition = match req_str(t, "disposition")? {
                "eligible" => TaskDisposition::Eligible,
                "runner_infeasible" => TaskDisposition::RunnerInfeasible,
                d => {
                    return Err(schema(format!(
                        "task {task_id} disposition {d} is undeclared"
                    )))
                }
            };
            let reason = t.get("reason").and_then(Json::as_str).map(str::to_string);
            if disposition == TaskDisposition::RunnerInfeasible && reason.is_none() {
                return Err(SelectError::Unreasoned {
                    task_id: task_id.clone(),
                });
            }
            tasks.push(NominalTask {
                task_id,
                difficulty,
                agent_timeout_sec,
                hf_saturated,
                disposition,
                reason,
            });
        }
        if tasks.len() != NOMINAL_TASKS as usize {
            return Err(SelectError::NominalMismatch { have: tasks.len() });
        }

        let mut exclusions = Vec::new();
        for x in req_arr(j, "exclusions")? {
            let task_id = req_str(x, "task_id")?.to_string();
            let basis = req_str(x, "basis")?.to_string();
            if !OUTCOME_BLIND_EXCLUSION_BASES.contains(&basis.as_str()) {
                return Err(SelectError::UnknownBasis { basis });
            }
            let reason = req_str(x, "reason")?.to_string();
            if reason.is_empty() {
                return Err(SelectError::Unreasoned {
                    task_id: task_id.clone(),
                });
            }
            let t = tasks.iter().find(|t| t.task_id == task_id).ok_or_else(|| {
                SelectError::UnknownTask {
                    task_id: task_id.clone(),
                }
            })?;
            if t.disposition != TaskDisposition::RunnerInfeasible {
                return Err(SelectError::DispositionMismatch {
                    detail: format!(
                        "exclusion {task_id} on basis {basis} but census row is {}",
                        t.disposition.as_str()
                    ),
                });
            }
            exclusions.push(RecordedExclusion {
                task_id,
                basis,
                reason,
            });
        }

        let mut download_tasks = Vec::new();
        for d in req_arr(j, "download_tasks")? {
            let task_id = req_str(d, "task_id")?.to_string();
            let disposition = req_str(d, "first_pass_disposition")?.to_string();
            if disposition != "excluded_if_drawn_and_replaced" {
                return Err(SelectError::DispositionMismatch {
                    detail: format!(
                        "download task {task_id} disposition {disposition} — Q-X55 grants no first-pass extension"
                    ),
                });
            }
            let covariate = req_str(d, "covariate")?.to_string();
            if covariate != "allowlist_extended" {
                return Err(SelectError::DispositionMismatch {
                    detail: format!(
                        "download task {task_id} covariate {covariate} is not allowlist_extended"
                    ),
                });
            }
            let reason = req_str(d, "reason")?.to_string();
            if reason.is_empty() {
                return Err(SelectError::Unreasoned {
                    task_id: task_id.clone(),
                });
            }
            let t = tasks.iter().find(|t| t.task_id == task_id).ok_or_else(|| {
                SelectError::UnknownTask {
                    task_id: task_id.clone(),
                }
            })?;
            if t.disposition != TaskDisposition::Eligible {
                return Err(SelectError::DispositionMismatch {
                    detail: format!("download task {task_id} is not an eligible census member"),
                });
            }
            download_tasks.push(DownloadTask {
                task_id,
                first_pass_disposition: disposition,
                covariate,
                reason,
            });
        }

        Ok(ExclusionRecord {
            recorded_at,
            tasks,
            exclusions,
            download_tasks,
        })
    }

    /// The eligible census (the materializer's rows).
    pub fn census(&self) -> Vec<TaskRow> {
        self.tasks
            .iter()
            .filter(|t| t.disposition == TaskDisposition::Eligible)
            .map(|t| TaskRow {
                task_id: t.task_id.clone(),
                agent_timeout_sec: t.agent_timeout_sec,
                difficulty: t.difficulty.clone(),
                hf_saturated: t.hf_saturated,
            })
            .collect()
    }
}

/// `build_selection_decl(record, exclusion_ref)` — the one derivation
/// from the committed exclusion record to the materializer's input:
/// the eligible census, the named exclusions, the pinned dataset, the
/// declared seed, the band-A held-out 10 and development-7 draws.
pub fn build_selection_decl(record: &ExclusionRecord, exclusion_ref: &str) -> SelectionDecl {
    SelectionDecl {
        pin: DatasetPin {
            git_commit: GIT_COMMIT.to_string(),
            hub_dataset_ref: HUB_DATASET_REF.to_string(),
            dataset_toml_digests: record
                .census()
                .iter()
                .map(|t| (t.task_id.clone(), DIGEST_PENDING.to_string()))
                .collect(),
        },
        census: record.census(),
        exclusions: record
            .exclusions
            .iter()
            .map(|x| ExclusionRow {
                task_id: x.task_id.clone(),
                basis: x.basis.clone(),
            })
            .collect(),
        exclusion_ref: exclusion_ref.to_string(),
        seed: FP_SEED.to_string(),
        stratification: STRATIFICATION.to_string(),
        dev_pool_size: DEV_POOL_TASKS,
        split_predates_access: true,
        input_manifest: ["task_id", "agent_timeout_sec", "difficulty", "hf_saturated"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        draws: vec![
            DrawDecl {
                name: HELD_DRAW.0.to_string(),
                pool: DrawPool::HeldOut,
                band: Some("A".to_string()),
                n: HELD_DRAW.1,
                purpose: Purpose::Confirmatory,
            },
            DrawDecl {
                name: DEV_DRAW.0.to_string(),
                pool: DrawPool::Dev,
                band: Some("A".to_string()),
                n: DEV_DRAW.1,
                purpose: Purpose::Development,
            },
        ],
    }
}

/// A consumed replacement — a drawn task excluded before commit and
/// the successor taken from its cell's committed seeded order.
#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    /// The excluded task.
    pub task_id: String,
    /// Its draw cell.
    pub cell: String,
    /// The recorded reason (the download-task declaration's reason or
    /// the re-proof allowlist gap spelling).
    pub reason: String,
    /// The successor taken from the cell's seeded order.
    pub replaced_by: String,
}

/// The resolved view of one draw after drawn-exclusions are consumed
/// from the within-cell committed orders.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDraw {
    /// The draw's record name.
    pub name: String,
    /// The final drawn set (post-replacement).
    pub drawn: Vec<String>,
    /// `cell → remaining replacement order` (the successor heads are
    /// each cell's first member).
    pub replacement_order: BTreeMap<String, Vec<String>>,
    /// The replacements consumed resolving the draw.
    pub replacements: Vec<Replacement>,
}

/// `resolve_draw(draw, excluded)` — apply REQ-XD-014's rule: a drawn
/// download task (or any member of `excluded`) is replaced by the next
/// task in its cell's committed seeded order, recursively (the next
/// member may itself be excluded).
pub fn resolve_draw(
    name: &str,
    draw: &StratifiedDraw,
    excluded: &BTreeMap<String, String>,
) -> ResolvedDraw {
    let mut cells = draw.cells.clone();
    let mut replacements = Vec::new();
    let mut drawn = Vec::new();
    for (cell, c) in cells.iter_mut() {
        let mut kept = Vec::new();
        for t in std::mem::take(&mut c.drawn) {
            let mut t = t;
            while let Some(reason) = excluded.get(&t) {
                // the successor may itself be excluded — keep
                // consuming the cell's committed order until an
                // eligible member lands or the order is exhausted
                let nx = c.replacement_order.first().cloned();
                match nx {
                    Some(n) => {
                        c.replacement_order.remove(0);
                        replacements.push(Replacement {
                            task_id: t.clone(),
                            cell: cell.clone(),
                            reason: reason.clone(),
                            replaced_by: n.clone(),
                        });
                        t = n;
                    }
                    // the cell's committed order is exhausted — the
                    // task stays dropped; REQ-XH-127's adjacent-cell
                    // order supplies further replacements
                    None => {
                        replacements.push(Replacement {
                            task_id: t.clone(),
                            cell: cell.clone(),
                            reason: reason.clone(),
                            replaced_by: String::new(),
                        });
                        break;
                    }
                }
            }
            if !excluded.contains_key(&t) {
                kept.push(t);
            }
        }
        c.drawn = kept.clone();
        drawn.extend(kept);
    }
    ResolvedDraw {
        name: name.to_string(),
        drawn,
        replacement_order: cells
            .into_iter()
            .map(|(c, d)| (c, d.replacement_order))
            .collect(),
        replacements,
    }
}

/// `derive_commit` — the derived members [`FpSelectCommit`] carries:
/// the resolved draws, the smoke set (one per difficulty from the dev
/// draw, the committed cells' first members), the successor heads, the
/// adjacent-cell orders for dev-cell depletion, and the W1 re-proof
/// scope (REQ-XH-116).
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedCommit {
    /// Resolved held-out draw.
    pub held: ResolvedDraw,
    /// Resolved development draw.
    pub dev: ResolvedDraw,
    /// `difficulty → smoke task` (one per difficulty from the dev
    /// draw).
    pub smoke: BTreeMap<String, String>,
    /// `difficulty → successor head` for the held-out re-proof scope.
    pub successors: BTreeMap<String, String>,
    /// `difficulty → adjacent-cell order` for dev-cell depletion
    /// (REQ-XH-127: easy|hard → medium's committed order).
    pub adjacent_orders: BTreeMap<String, Vec<String>>,
    /// The W1 re-proof scope: the 10 held-out tasks + one successor
    /// per difficulty cell + the 7 development tasks.
    pub reproof_scope: Vec<String>,
}

/// Compute the derived members from the materialized selection and the
/// exclusion declarations.
pub fn derive_commit(
    sel: &TaskSelection,
    record: &ExclusionRecord,
) -> Result<DerivedCommit, SelectError> {
    let excluded: BTreeMap<String, String> = record
        .download_tasks
        .iter()
        .map(|d| (d.task_id.clone(), d.reason.clone()))
        .collect();
    let difficulty: BTreeMap<String, String> = record
        .tasks
        .iter()
        .map(|t| (t.task_id.clone(), t.difficulty.clone()))
        .collect();
    let draw = |name| {
        sel.draws
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.draw.clone())
            .ok_or_else(|| schema(format!("draw {name} absent from the selection")))
    };
    let held = resolve_draw(HELD_DRAW.0, &draw(HELD_DRAW.0)?, &excluded);
    let dev = resolve_draw(DEV_DRAW.0, &draw(DEV_DRAW.0)?, &excluded);

    // smoke: one per difficulty — the first member of each dev-draw
    // cell's resolved drawn set (the committed order's head).
    let dev_draw = draw(DEV_DRAW.0)?;
    let mut smoke = BTreeMap::new();
    for (cell, c) in dev_draw.cells.iter() {
        // cell spellings are "<difficulty>" inside the band-A dev draw
        let diff = cell
            .rsplit([':', '-'])
            .next()
            .unwrap_or(cell);
        let resolved: Vec<String> = c
            .drawn
            .iter()
            .filter(|t| !excluded.contains_key(*t))
            .cloned()
            .collect();
        if let Some(t) = resolved
            .first()
            .or_else(|| dev.replacement_order.get(cell).and_then(|o| o.first()))
        {
            smoke.insert(diff.to_string(), t.clone());
        }
    }
    for d in DIFFICULTIES {
        if !smoke.contains_key(d) {
            return Err(schema(format!("dev draw supplies no {d} smoke task")));
        }
    }

    // successors: first member of each held-out cell's remaining
    // committed order (REQ-XH-116's pre-re-proofed successor).
    let held_draw = draw(HELD_DRAW.0)?;
    let mut successors = BTreeMap::new();
    for (cell, c) in held_draw.cells.iter() {
        let diff = cell
            .rsplit([':', '-'])
            .next()
            .unwrap_or(cell);
        let remaining = held
            .replacement_order
            .get(cell)
            .cloned()
            .unwrap_or(c.replacement_order.clone());
        let next = remaining
            .iter()
            .find(|t| !excluded.contains_key(*t))
            .cloned();
        if let Some(s) = next {
            successors.insert(diff.to_string(), s);
        }
    }
    for d in DIFFICULTIES {
        if !successors.contains_key(d) {
            return Err(schema(format!(
                "held-out draw supplies no {d} successor — the cell's committed order is exhausted"
            )));
        }
    }

    // adjacent orders (REQ-XH-127's dev-cell depletion): easy|hard
    // fall to the medium cell's committed order.
    let medium_order = dev_draw
        .cells
        .iter()
        .find(|(cell, _)| cell.ends_with("medium"))
        .map(|(_, c)| {
            c.drawn
                .iter()
                .chain(c.replacement_order.iter())
                .filter(|t| !dev.drawn.contains(*t))
                .cloned()
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let adjacent_orders: BTreeMap<String, Vec<String>> = ["easy", "hard"]
        .iter()
        .map(|d| (d.to_string(), medium_order.clone()))
        .collect();

    // reproof scope: 10 held-out + one successor per cell + 7 dev.
    let mut reproof_scope: Vec<String> = held.drawn.clone();
    for d in DIFFICULTIES {
        if let Some(s) = successors.get(d) {
            reproof_scope.push(s.clone());
        }
    }
    reproof_scope.extend(dev.drawn.iter().cloned());
    reproof_scope.sort();
    reproof_scope.dedup();

    let _ = difficulty;
    Ok(DerivedCommit {
        held,
        dev,
        smoke,
        successors,
        adjacent_orders,
        reproof_scope,
    })
}

/// The `hh-fp-select/1` commit record — the binding the PROV-W1
/// readout cites (hash + time).
#[derive(Debug, Clone, PartialEq)]
pub struct FpSelectCommit {
    /// `H(canonical(record minus select_id))` under `fp_select`.
    pub select_id: String,
    /// The record timestamp.
    pub recorded_at: String,
    /// The exclusion record's path + sha256.
    pub exclusion_ref: Json,
    /// The selection record's path + sha256 + `selection_id`.
    pub selection_ref: Json,
    /// The declared seed + its basis.
    pub seed: String,
    /// The derived members.
    pub derived: DerivedCommit,
}

const DOMAIN: &str = "fp_select";

impl FpSelectCommit {
    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let succ = Json::Obj(
            self.derived
                .successors
                .iter()
                .map(|(d, t)| (d.clone(), Json::str(t)))
                .collect(),
        );
        let smoke = Json::Obj(
            self.derived
                .smoke
                .iter()
                .map(|(d, t)| (d.clone(), Json::str(t)))
                .collect(),
        );
        let adjacent = Json::Obj(
            self.derived
                .adjacent_orders
                .iter()
                .map(|(d, o)| (d.clone(), Json::Arr(o.iter().map(Json::str).collect())))
                .collect(),
        );
        let draw_json = |d: &ResolvedDraw| {
            Json::obj([
                ("name", Json::str(&d.name)),
                ("drawn", Json::Arr(d.drawn.iter().map(Json::str).collect())),
                (
                    "replacements",
                    Json::Arr(
                        d.replacements
                            .iter()
                            .map(|r| {
                                Json::obj([
                                    ("task_id", Json::str(&r.task_id)),
                                    ("cell", Json::str(&r.cell)),
                                    ("reason", Json::str(&r.reason)),
                                    ("replaced_by", Json::str(&r.replaced_by)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ])
        };
        Json::obj([
            ("schema", Json::str(SELECT_SCHEMA)),
            ("recorded_at", Json::str(&self.recorded_at)),
            ("seed", Json::str(&self.seed)),
            (
                "seed_basis",
                Json::str(
                    "declared outcome-blind at contract time — no outcome field exists or was consulted",
                ),
            ),
            ("exclusion_record", self.exclusion_ref.clone()),
            ("selection", self.selection_ref.clone()),
            (
                "digests_status",
                Json::str(format!(
                    "{DIGEST_PENDING} — hh-harbor-ingest/1 per-task digests bind before FP-PREREG freezes the post-replacement list"
                )),
            ),
            ("held_out_draw", draw_json(&self.derived.held)),
            ("dev_draw", draw_json(&self.derived.dev)),
            ("smoke_tasks", smoke),
            ("successors", succ),
            ("adjacent_cell_orders", adjacent),
            (
                "reproof_scope",
                Json::obj([
                    ("held_out_r", Json::Int(3)),
                    ("dev_r", Json::Int(2)),
                    (
                        "tasks",
                        Json::Arr(
                            self.derived
                                .reproof_scope
                                .iter()
                                .map(Json::str)
                                .collect(),
                        ),
                    ),
                ]),
            ),
        ])
    }

    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("select_id");
        }
        self.select_id
            == hharness_identity::idp::idp_digest(DOMAIN, j.to_canonical_string().as_bytes())
    }

    /// Compute `select_id` for a freshly derived commit.
    pub fn with_id(mut self) -> Self {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("select_id");
        }
        self.select_id =
            hharness_identity::idp::idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
        self
    }

    /// The canonical JSON with `select_id` embedded.
    pub fn record_json(&self) -> Json {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.insert("select_id".to_string(), Json::str(&self.select_id));
        }
        j
    }
}

/// `materialize_fp` — the full pipeline the bin and the tests share:
/// decode the committed exclusion record, build the declaration and
/// materialize the `task-selection/1` record.
pub fn materialize_fp(
    record: &ExclusionRecord,
    exclusion_ref: &str,
) -> Result<TaskSelection, SelectionError> {
    materialize_selection(&build_selection_decl(record, exclusion_ref))
}
