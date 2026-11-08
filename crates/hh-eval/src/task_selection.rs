//! `task_selection` — the `task-selection/1` materializer
//! (experiments spec §3.1 REQ-XB-122 absorbing REQ-XB-001; §6.2
//! REQ-XD-011 absorbing REQ-XB-017; §6.12 REQ-XH-012; §10.1.2's draw
//! shape; design/35 F4; manifest 271/R6.18 stage 1 SELECT).
//!
//! `materialize_selection` consumes the **supplied** declared-field
//! census — every value an external gated input, never chosen here —
//! and yields the deterministic `task-selection/1` record: the
//! exclusion echo, the seeded band × difficulty split that reserves the
//! development pool by largest remainder before any W2 access, the
//! declared seeded stratified draws with their pre-committed
//! within-cell replacement orders, and the TB 2.1 three-part pin
//! carried verbatim. Ranking and content-addressing run under
//! `idp_digest` domain `comparative_protocol.split` (REQ-XD-011).
//!
//! Strata come only from declared `task.toml` fields — the
//! agent-timeout band and `difficulty` (research/25 Appendix A at
//! `12b75656`); any other stratification spelling refuses. HF
//! saturation is echoed as a reporting covariate and is never a
//! selection input. An input manifest carrying an outcome field — a
//! verdict, reward, `ctrf`, verifier output or another phase's
//! held-out outcome — refuses typed (REQ-XH-012). `ref=latest`
//! refuses; the pin is three fields, always.
//!
//! Phase-2 confirmatory draws read only the held-out pool; development
//! draws are labelled `development` and carry no confirmatory claim.
//! Nothing here is an admitted estimator and no outcome field is ever
//! read (X-07). The real first-pass commit of these records is R6.30's.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::idp::idp_digest;
use hharness_wire::Json;

use crate::central_protocol::OUTCOME_BLIND_EXCLUSION_BASES;
use crate::population::MethodMetadata;

/// The materialization name (the S.1 `estimator` member).
pub const KIND: &str = "task_selection";
/// The durable payload version.
pub const VERSION: &str = "task-selection/1";
/// The ranking/addressing domain (REQ-XD-011).
pub const DOMAIN_SPLIT: &str = "comparative_protocol.split";
/// The development-pool reservation (§6.2 dev row; Q-X40 default 20).
pub const DEV_POOL_TASKS: u32 = 20;
/// The only admitted stratification — both components are declared
/// `task.toml` fields. Any other spelling refuses
/// [`SelectionError::NonDeclaredStratum`].
pub const STRATIFICATION: &str = "agent_timeout_band_x_difficulty";
/// The declared difficulty vocabulary (`[metadata].difficulty`).
pub const DIFFICULTIES: [&str; 3] = ["easy", "medium", "hard"];
/// The input-manifest members the declared strata derive from — each
/// must be named for the census to be admissible.
pub const REQUIRED_MANIFEST_FIELDS: [&str; 3] = ["task_id", "agent_timeout_sec", "difficulty"];
/// The outcome vocabulary — any input-manifest member carrying one of
/// these names, or any name containing `outcome` (which covers another
/// phase's `held_out_outcome` spellings), refuses (REQ-XH-012).
pub const OUTCOME_FIELD_NAMES: [&str; 8] = [
    "verdict",
    "reward",
    "ctrf",
    "verifier_output",
    "result",
    "score",
    "held_out_outcome",
    "outcome",
];
/// The band boundaries on declared `[agent].timeout_sec` seconds
/// (§6.2: A ≤ 900; B 1200–1800; C 2400–3600; D ≥ 7200 — open gaps take
/// the lower-adjacent band's ceiling).
pub const BAND_EDGES: [(&str, u64); 3] = [("A", 900), ("B", 1800), ("C", 3600)];

fn rank(material: &str) -> String {
    idp_digest(DOMAIN_SPLIT, material.as_bytes())
}

// ── declared inputs (records-in) ───────────────────────────────────

/// The TB 2.1 three-part pin (REQ-XB-122 + REQ-XB-001's literal
/// predicate): dataset git commit, Hub dataset ref, per-task
/// `dataset.toml` digest. `latest` refuses typed.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetPin {
    /// The dataset git commit (hex).
    pub git_commit: String,
    /// The Hub dataset ref — `latest` refuses.
    pub hub_dataset_ref: String,
    /// `task_id → dataset.toml digest` — must cover every census task.
    /// Fixture digests are labelled stand-ins until R6.30 sources the
    /// real digests from the Harbor ingest; never invented.
    pub dataset_toml_digests: BTreeMap<String, String>,
}

/// One census row — declared `task.toml` fields only.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskRow {
    /// The task id.
    pub task_id: String,
    /// Declared `[agent].timeout_sec` (whole seconds).
    pub agent_timeout_sec: u64,
    /// Declared `[metadata].difficulty` — a member of
    /// [`DIFFICULTIES`].
    pub difficulty: String,
    /// HF saturation flag — a reporting covariate echoed in the
    /// record, never a selection input (REQ-XD-011).
    pub hf_saturated: bool,
}

/// One named, reasoned exclusion row (REQ-XD-010 contributing — the
/// outcome-blind bases are `central_protocol`'s registered set, CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionRow {
    /// The excluded task.
    pub task_id: String,
    /// The declared basis — a member of
    /// [`OUTCOME_BLIND_EXCLUSION_BASES`].
    pub basis: String,
}

/// Which pool a declared draw reads.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawPool {
    /// The held-out pool — the only pool a confirmatory purpose may
    /// read (REQ-XD-011).
    HeldOut,
    /// The development pool — development draws only; a confirmatory
    /// draw here refuses.
    Dev,
}

impl DrawPool {
    fn name(&self) -> &'static str {
        match self {
            DrawPool::HeldOut => "held_out",
            DrawPool::Dev => "dev",
        }
    }
}

/// A declared draw's purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Confirmatory — held-out pool only.
    Confirmatory,
    /// Development — either pool; never a confirmatory claim.
    Development,
}

impl Purpose {
    fn name(&self) -> &'static str {
        match self {
            Purpose::Confirmatory => "confirmatory",
            Purpose::Development => "development",
        }
    }
}

/// One declared stratified draw (§10.1.2's shape — e.g. the band-A
/// held-out 10 or the 7-task development corpus list).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawDecl {
    /// The draw's record name (`tr1`, `dev_l1`, …).
    pub name: String,
    /// The pool the draw reads.
    pub pool: DrawPool,
    /// Optional band restriction (`"A"`…`"D"`) — the §10.1.2 draws are
    /// band-local.
    pub band: Option<String>,
    /// The draw size.
    pub n: u32,
    /// The draw's declared purpose.
    pub purpose: Purpose,
}

/// The materialization input — records-in.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionDecl {
    /// The TB 2.1 three-part pin.
    pub pin: DatasetPin,
    /// The declared-field census (eligible tasks only — exclusions are
    /// named separately, never mixed in).
    pub census: Vec<TaskRow>,
    /// The named outcome-blind exclusions recorded before the split.
    pub exclusions: Vec<ExclusionRow>,
    /// The ref under which exclusions were recorded before any outcome
    /// (mandatory when `exclusions` is non-empty).
    pub exclusion_ref: String,
    /// The committed selection seed.
    pub seed: String,
    /// The admitted stratification — must equal [`STRATIFICATION`].
    pub stratification: String,
    /// The development-pool reservation (`0` disables — the split then
    /// puts every eligible task in held-out; [`DEV_POOL_TASKS`] is the
    /// §6.2 value).
    pub dev_pool_size: u32,
    /// `true` — the split record precedes any W2 access; `false`
    /// refuses [`SelectionError::LateSplit`].
    pub split_predates_access: bool,
    /// The names of every member the census input rows carry — the
    /// input manifest the outcome refusal checks (REQ-XH-012).
    pub input_manifest: Vec<String>,
    /// The declared draws materialized with the split.
    pub draws: Vec<DrawDecl>,
}

// ── the materialized record ────────────────────────────────────────

/// One band × difficulty cell of the split.
#[derive(Debug, Clone, PartialEq)]
pub struct CellSplit {
    /// The agent-timeout band (`A`…`D`).
    pub band: String,
    /// The difficulty.
    pub difficulty: String,
    /// Eligible census tasks in the cell.
    pub eligible: u32,
    /// The largest-remainder development quota.
    pub dev_quota: u32,
    /// The development members, in the cell's seeded order.
    pub dev: Vec<String>,
    /// The held-out members, in the cell's seeded order.
    pub held_out: Vec<String>,
    /// HF-saturated member count — reporting covariate only.
    pub hf_saturated: u32,
}

/// One cell of a materialized draw — the drawn prefix and the
/// pre-committed seeded replacement order (REQ-XH-116's within-cell
/// order).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawCell {
    /// The drawn tasks (the cell's seeded order's first `quota`).
    pub drawn: Vec<String>,
    /// The remaining members in seeded order — the committed
    /// replacement order.
    pub replacement_order: Vec<String>,
}

/// A materialized stratified draw.
#[derive(Debug, Clone, PartialEq)]
pub struct StratifiedDraw {
    /// `cell → {drawn, replacement_order}`.
    pub cells: BTreeMap<String, DrawCell>,
    /// `cell → quota` the largest-remainder allocation assigned.
    pub quotas: BTreeMap<String, u32>,
}

impl StratifiedDraw {
    /// Every drawn task (all cells).
    pub fn drawn(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for c in self.cells.values() {
            v.extend(c.drawn.iter().cloned());
        }
        v
    }

    /// Consume the next task of `cell`'s committed replacement order —
    /// the §10.1.2 / REQ-XH-116 replacement step. `None` when the cell
    /// is absent or its order is exhausted.
    pub fn take_replacement(&mut self, cell: &str) -> Option<String> {
        let c = self.cells.get_mut(cell)?;
        if c.replacement_order.is_empty() {
            return None;
        }
        let next = c.replacement_order.remove(0);
        c.drawn.push(next.clone());
        Some(next)
    }
}

/// One materialized declared draw.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedDraw {
    /// The draw's record name.
    pub name: String,
    /// The pool it read (`held_out` | `dev`).
    pub pool: String,
    /// The band restriction (`None` = whole pool).
    pub band: Option<String>,
    /// The declared purpose.
    pub purpose: String,
    /// The draw.
    pub draw: StratifiedDraw,
}

/// The materialized `task-selection/1` record — `selection_id` is the
/// `idp_digest` commitment under `comparative_protocol.split`.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSelection {
    /// `H(canonical(record minus selection_id))` under
    /// [`DOMAIN_SPLIT`].
    pub selection_id: String,
    /// The TB 2.1 pin, carried verbatim on the record.
    pub pin: DatasetPin,
    /// The committed seed.
    pub seed: String,
    /// The stratification spelling (always [`STRATIFICATION`]).
    pub stratification: String,
    /// The recorded exclusion ref.
    pub exclusion_ref: String,
    /// The echoed exclusion rows.
    pub exclusions: Vec<ExclusionRow>,
    /// `true` — the split predates any W2 access.
    pub split_predates_access: bool,
    /// The split cells, sorted by (band, difficulty).
    pub cells: Vec<CellSplit>,
    /// The materialized declared draws.
    pub draws: Vec<MaterializedDraw>,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

/// The pool a task sits in inside a [`TaskSelection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    /// Development.
    Dev,
    /// Held-out.
    HeldOut,
}

impl TaskSelection {
    /// Recompute the identity — `false` means the record drifted.
    pub fn check_id(&self) -> bool {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("selection_id");
        }
        self.selection_id == idp_digest(DOMAIN_SPLIT, j.to_canonical_string().as_bytes())
    }

    /// The pool `task_id` sits in (`None` = not in the selection).
    pub fn pool_of(&self, task_id: &str) -> Option<Pool> {
        for c in &self.cells {
            if c.dev.iter().any(|t| t == task_id) {
                return Some(Pool::Dev);
            }
            if c.held_out.iter().any(|t| t == task_id) {
                return Some(Pool::HeldOut);
            }
        }
        None
    }

    /// The held-out members (`Some(band)` restricts to one band), in
    /// cell order then seeded order.
    pub fn held_out(&self, band: Option<&str>) -> Vec<String> {
        self.cells
            .iter()
            .filter(|c| band.is_none_or(|b| c.band == b))
            .flat_map(|c| c.held_out.iter().cloned())
            .collect()
    }

    /// The development members (`Some(band)` restricts to one band).
    pub fn dev(&self, band: Option<&str>) -> Vec<String> {
        self.cells
            .iter()
            .filter(|c| band.is_none_or(|b| c.band == b))
            .flat_map(|c| c.dev.iter().cloned())
            .collect()
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("selection_id", Json::str(&self.selection_id)),
            (
                "pin",
                Json::obj([
                    ("git_commit", Json::str(&self.pin.git_commit)),
                    ("hub_dataset_ref", Json::str(&self.pin.hub_dataset_ref)),
                    (
                        "dataset_toml_digests",
                        Json::Obj(
                            self.pin
                                .dataset_toml_digests
                                .iter()
                                .map(|(t, d)| (t.clone(), Json::str(d)))
                                .collect(),
                        ),
                    ),
                ]),
            ),
            ("seed", Json::str(&self.seed)),
            ("stratification", Json::str(&self.stratification)),
            ("exclusion_ref", Json::str(&self.exclusion_ref)),
            (
                "exclusions",
                Json::Arr(
                    self.exclusions
                        .iter()
                        .map(|x| {
                            Json::obj([
                                ("task_id", Json::str(&x.task_id)),
                                ("basis", Json::str(&x.basis)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "split_predates_access",
                Json::Bool(self.split_predates_access),
            ),
            (
                "cells",
                Json::Arr(
                    self.cells
                        .iter()
                        .map(|c| {
                            Json::obj([
                                ("band", Json::str(&c.band)),
                                ("difficulty", Json::str(&c.difficulty)),
                                ("eligible", Json::Int(c.eligible as i64)),
                                ("dev_quota", Json::Int(c.dev_quota as i64)),
                                ("dev", Json::Arr(c.dev.iter().map(Json::str).collect())),
                                (
                                    "held_out",
                                    Json::Arr(c.held_out.iter().map(Json::str).collect()),
                                ),
                                ("hf_saturated", Json::Int(c.hf_saturated as i64)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "draws",
                Json::Arr(
                    self.draws
                        .iter()
                        .map(|d| {
                            Json::obj([
                                ("name", Json::str(&d.name)),
                                ("pool", Json::str(&d.pool)),
                                ("band", d.band.as_ref().map_or(Json::Null, Json::str)),
                                ("purpose", Json::str(&d.purpose)),
                                ("draw", draw_json(&d.draw)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

fn draw_json(d: &StratifiedDraw) -> Json {
    Json::obj([
        (
            "quotas",
            Json::Obj(
                d.quotas
                    .iter()
                    .map(|(c, q)| (c.clone(), Json::Int(*q as i64)))
                    .collect(),
            ),
        ),
        (
            "cells",
            Json::Obj(
                d.cells
                    .iter()
                    .map(|(cell, d)| {
                        (
                            cell.clone(),
                            Json::obj([
                                ("drawn", Json::Arr(d.drawn.iter().map(Json::str).collect())),
                                (
                                    "replacement_order",
                                    Json::Arr(d.replacement_order.iter().map(Json::str).collect()),
                                ),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

// ── the refusal set ────────────────────────────────────────────────

/// The closed refusal set — typed, never a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionError {
    /// A required member is absent/empty.
    MissingKey { member: String },
    /// A pin member is `latest`/unpinned (REQ-XB-001's predicate).
    UnpinnedRef { member: &'static str },
    /// The pin's `dataset_toml_digests` does not cover a census task.
    UndeclaredPinDigest { task_id: String },
    /// A stratification spelling outside [`STRATIFICATION`] — strata
    /// come from declared `task.toml` fields only (REQ-XB-122).
    NonDeclaredStratum { stratification: String },
    /// An outcome field — or another phase's held-out outcome — in the
    /// input manifest (REQ-XH-012).
    OutcomeField { field: String },
    /// A required manifest member is absent — the census does not
    /// declare the fields the admitted strata derive from.
    UndeclaredField { field: &'static str },
    /// An exclusion's basis is not outcome-blind.
    OutcomeSelectedExclusion { task_id: String, basis: String },
    /// Exclusions exist with no before-outcomes ref.
    UnpublishedExclusions,
    /// An excluded task is also eligible.
    ExclusionEligible { task_id: String },
    /// The same task appears twice in the census.
    ConflictingTask { task_id: String },
    /// A declared difficulty outside [`DIFFICULTIES`].
    UnknownDifficulty { task_id: String, difficulty: String },
    /// The split does not predate access.
    LateSplit,
    /// `dev_pool_size` exceeds the eligible census.
    DevPoolMismatch { requested: u32, eligible: u32 },
    /// A draw name appears twice.
    DuplicateDraw { name: String },
    /// A draw band is outside `A`…`D`.
    UnknownBand { draw: String, band: String },
    /// A confirmatory draw reads the development pool.
    PoolMismatch { draw: String },
    /// The pool cannot satisfy the draw size.
    InsufficientPool { draw: String, have: u32, need: u32 },
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use SelectionError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing required member: {member}"),
            E::UnpinnedRef { member } => write!(f, "pin member {member} is unpinned (`latest`)"),
            E::UndeclaredPinDigest { task_id } => {
                write!(f, "no dataset.toml digest pinned for {task_id}")
            }
            E::NonDeclaredStratum { stratification } => write!(
                f,
                "stratification {stratification} is not a declared-field stratum"
            ),
            E::OutcomeField { field } => {
                write!(f, "outcome field {field} in the selection input manifest")
            }
            E::UndeclaredField { field } => {
                write!(f, "declared field {field} absent from the input manifest")
            }
            E::OutcomeSelectedExclusion { task_id, basis } => {
                write!(f, "exclusion of {task_id} is outcome-selected: {basis}")
            }
            E::UnpublishedExclusions => write!(f, "exclusions lack a before-outcomes ref"),
            E::ExclusionEligible { task_id } => {
                write!(f, "excluded task {task_id} presented as eligible")
            }
            E::ConflictingTask { task_id } => write!(f, "task {task_id} presented twice"),
            E::UnknownDifficulty {
                task_id,
                difficulty,
            } => {
                write!(f, "task {task_id} difficulty {difficulty} is undeclared")
            }
            E::LateSplit => write!(f, "split does not predate access"),
            E::DevPoolMismatch {
                requested,
                eligible,
            } => write!(
                f,
                "dev pool {requested} exceeds the {eligible}-task eligible census"
            ),
            E::DuplicateDraw { name } => write!(f, "draw {name} declared twice"),
            E::UnknownBand { draw, band } => {
                write!(f, "draw {draw} band {band} outside A–D")
            }
            E::PoolMismatch { draw } => {
                write!(f, "confirmatory draw {draw} reads the development pool")
            }
            E::InsufficientPool { draw, have, need } => {
                write!(f, "draw {draw}: {have} pool tasks < {need} requested")
            }
        }
    }
}

impl std::error::Error for SelectionError {}

// ── shared deterministic machinery ─────────────────────────────────

/// The declared agent-timeout band (§6.2 boundaries).
pub fn band_of(agent_timeout_sec: u64) -> &'static str {
    for (band, ceiling) in BAND_EDGES {
        if agent_timeout_sec <= ceiling {
            return band;
        }
    }
    "D"
}

/// Largest-remainder allocation of `seats` over `cells` (exact integer
/// quotas; ties on the fractional remainder break by the seeded rank,
/// then the cell name — deterministic by construction). Returns
/// `cell → seats`.
pub fn largest_remainder(cells: &[(String, u32)], seats: u32, seed: &str) -> BTreeMap<String, u32> {
    let total: u64 = cells.iter().map(|(_, n)| *n as u64).sum();
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    if total == 0 || seats == 0 {
        return cells.iter().map(|(c, _)| (c.clone(), 0)).collect();
    }
    let mut assigned: u64 = 0;
    let mut rem: Vec<(u64, String)> = Vec::with_capacity(cells.len());
    for (cell, n) in cells {
        let q = (*n as u64) * (seats as u64) / total;
        out.insert(cell.clone(), q as u32);
        assigned += q;
        rem.push(((*n as u64) * (seats as u64) % total, cell.clone()));
    }
    let mut left = seats as u64 - assigned;
    rem.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(
                rank(&format!("{seed}|alloc|{}", a.1)).cmp(&rank(&format!("{seed}|alloc|{}", b.1))),
            )
            .then(a.1.cmp(&b.1))
    });
    // A remainder seat never exceeds the cell's size (a cell cannot
    // supply more members than it holds).
    let size_of: BTreeMap<&str, u32> = cells.iter().map(|(c, n)| (c.as_str(), *n)).collect();
    for (_, cell) in rem {
        if left == 0 {
            break;
        }
        if let Some(q) = out.get_mut(&cell) {
            if (*q as u64) < *size_of.get(cell.as_str()).unwrap_or(&0) as u64 {
                *q += 1;
                left -= 1;
            }
        }
    }
    out
}

/// The seeded within-cell order — `idp_digest` ranking under
/// [`DOMAIN_SPLIT`], `task_id` tie-break (the REQ-XD-011 rule shared
/// with `central_protocol`'s digest-ranked selection).
fn seeded_order<'a>(seed: &str, cell: &str, tasks: &[&'a TaskRow]) -> Vec<&'a TaskRow> {
    let mut v: Vec<&TaskRow> = tasks.to_vec();
    v.sort_by(|a, b| {
        rank(&format!("{seed}|cell|{cell}|{}", a.task_id))
            .cmp(&rank(&format!("{seed}|cell|{cell}|{}", b.task_id)))
            .then(a.task_id.cmp(&b.task_id))
    });
    v
}

/// One candidate of a stratified draw.
#[derive(Debug, Clone, Copy)]
pub struct DrawCandidate<'a> {
    /// The task id.
    pub task_id: &'a str,
    /// The cell the task draws under (e.g. its difficulty).
    pub cell: &'a str,
}

/// `stratified_draw(candidates, n, seed)` — the seeded
/// difficulty-stratified draw by largest remainder with the
/// pre-committed seeded within-cell replacement order (§10.1.2's
/// shape): each cell's seeded order supplies its drawn prefix; the
/// remainder is the committed replacement order a later re-proof or
/// exclusion consumes in order.
pub fn stratified_draw(
    candidates: &[DrawCandidate<'_>],
    n: u32,
    seed: &str,
) -> Result<StratifiedDraw, SelectionError> {
    if n == 0 {
        return Err(SelectionError::MissingKey {
            member: "n".to_string(),
        });
    }
    let mut by_cell: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for c in candidates {
        by_cell.entry(c.cell).or_default().push(c.task_id);
    }
    if (candidates.len() as u32) < n {
        return Err(SelectionError::InsufficientPool {
            draw: "stratified".to_string(),
            have: candidates.len() as u32,
            need: n,
        });
    }
    let sizes: Vec<(String, u32)> = by_cell
        .iter()
        .map(|(c, ts)| (c.to_string(), ts.len() as u32))
        .collect();
    let quotas = largest_remainder(&sizes, n, seed);
    let mut cells: BTreeMap<String, DrawCell> = BTreeMap::new();
    for (cell, ts) in &by_cell {
        let mut keyed: Vec<(String, &str)> = ts
            .iter()
            .map(|t| (rank(&format!("{seed}|draw|{cell}|{t}")), *t))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
        let order: Vec<String> = keyed.into_iter().map(|(_, t)| t.to_string()).collect();
        let q = (quotas.get(*cell).copied().unwrap_or(0) as usize).min(order.len());
        cells.insert(
            cell.to_string(),
            DrawCell {
                drawn: order[..q].to_vec(),
                replacement_order: order[q..].to_vec(),
            },
        );
    }
    Ok(StratifiedDraw { cells, quotas })
}

// ── the materializer ───────────────────────────────────────────────

/// `materialize_selection(decl)` — validate the supplied gated inputs
/// and emit the deterministic `task-selection/1` record. Pure and
/// total: same census, seed and pin ⇒ byte-identical record and
/// `selection_id`.
pub fn materialize_selection(decl: &SelectionDecl) -> Result<TaskSelection, SelectionError> {
    use SelectionError as E;

    // ── pin: three fields, `latest` refused ────────────────────────
    if decl.pin.git_commit.is_empty() {
        return Err(E::MissingKey {
            member: "pin.git_commit".to_string(),
        });
    }
    if decl.pin.hub_dataset_ref.is_empty() {
        return Err(E::MissingKey {
            member: "pin.hub_dataset_ref".to_string(),
        });
    }
    let unpinned = |v: &str| v == "latest" || v.ends_with(":latest") || v == "HEAD";
    if unpinned(&decl.pin.git_commit) {
        return Err(E::UnpinnedRef {
            member: "git_commit",
        });
    }
    if unpinned(&decl.pin.hub_dataset_ref) {
        return Err(E::UnpinnedRef {
            member: "hub_dataset_ref",
        });
    }
    for digest in decl.pin.dataset_toml_digests.values() {
        if digest.is_empty() || unpinned(digest) {
            return Err(E::UnpinnedRef {
                member: "dataset_toml_digests",
            });
        }
    }
    if decl.seed.is_empty() {
        return Err(E::MissingKey {
            member: "seed".to_string(),
        });
    }

    // ── stratification: declared task.toml fields only ─────────────
    if decl.stratification != STRATIFICATION {
        return Err(E::NonDeclaredStratum {
            stratification: decl.stratification.clone(),
        });
    }

    // ── input manifest: required declared fields + outcome refusal ──
    let manifest: BTreeSet<&str> = decl.input_manifest.iter().map(String::as_str).collect();
    for f in REQUIRED_MANIFEST_FIELDS {
        if !manifest.contains(f) {
            return Err(E::UndeclaredField { field: f });
        }
    }
    for f in &decl.input_manifest {
        if OUTCOME_FIELD_NAMES.contains(&f.as_str()) || f.contains("outcome") {
            return Err(E::OutcomeField { field: f.clone() });
        }
    }

    // ── exclusions: outcome-blind bases, recorded first ────────────
    for x in &decl.exclusions {
        if !OUTCOME_BLIND_EXCLUSION_BASES.contains(&x.basis.as_str()) {
            return Err(E::OutcomeSelectedExclusion {
                task_id: x.task_id.clone(),
                basis: x.basis.clone(),
            });
        }
    }
    if !decl.exclusions.is_empty() && decl.exclusion_ref.is_empty() {
        return Err(E::UnpublishedExclusions);
    }
    let excluded: BTreeSet<&str> = decl.exclusions.iter().map(|x| x.task_id.as_str()).collect();

    // ── census: unique ids, declared difficulties, pin coverage ────
    if decl.census.is_empty() {
        return Err(E::MissingKey {
            member: "census".to_string(),
        });
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for t in &decl.census {
        if t.task_id.is_empty() {
            return Err(E::MissingKey {
                member: "census[].task_id".to_string(),
            });
        }
        if excluded.contains(t.task_id.as_str()) {
            return Err(E::ExclusionEligible {
                task_id: t.task_id.clone(),
            });
        }
        if !seen.insert(t.task_id.as_str()) {
            return Err(E::ConflictingTask {
                task_id: t.task_id.clone(),
            });
        }
        if !DIFFICULTIES.contains(&t.difficulty.as_str()) {
            return Err(E::UnknownDifficulty {
                task_id: t.task_id.clone(),
                difficulty: t.difficulty.clone(),
            });
        }
        match decl.pin.dataset_toml_digests.get(&t.task_id) {
            Some(d) if !d.is_empty() => {}
            _ => {
                return Err(E::UndeclaredPinDigest {
                    task_id: t.task_id.clone(),
                })
            }
        }
    }
    if !decl.split_predates_access {
        return Err(E::LateSplit);
    }
    if decl.dev_pool_size > decl.census.len() as u32 {
        return Err(E::DevPoolMismatch {
            requested: decl.dev_pool_size,
            eligible: decl.census.len() as u32,
        });
    }

    // ── the seeded split: cells band × difficulty, dev by largest
    // remainder, dev members = the seeded order's prefix ────────────
    let mut by_cell: BTreeMap<(String, String), Vec<&TaskRow>> = BTreeMap::new();
    for t in &decl.census {
        by_cell
            .entry((
                band_of(t.agent_timeout_sec).to_string(),
                t.difficulty.clone(),
            ))
            .or_default()
            .push(t);
    }
    let sizes: Vec<(String, u32)> = by_cell
        .iter()
        .map(|((b, d), ts)| (format!("{b}|{d}"), ts.len() as u32))
        .collect();
    let quotas = largest_remainder(&sizes, decl.dev_pool_size, &decl.seed);
    let mut cells: Vec<CellSplit> = Vec::with_capacity(by_cell.len());
    for ((band, difficulty), tasks) in &by_cell {
        let key = format!("{band}|{difficulty}");
        let q = quotas.get(&key).copied().unwrap_or(0) as usize;
        let order = seeded_order(&decl.seed, &key, tasks);
        let q = q.min(order.len());
        cells.push(CellSplit {
            band: band.clone(),
            difficulty: difficulty.clone(),
            eligible: tasks.len() as u32,
            dev_quota: q as u32,
            dev: order[..q].iter().map(|t| t.task_id.clone()).collect(),
            held_out: order[q..].iter().map(|t| t.task_id.clone()).collect(),
            hf_saturated: tasks.iter().filter(|t| t.hf_saturated).count() as u32,
        });
    }

    // ── the declared draws ─────────────────────────────────────────
    let mut draw_names: BTreeSet<&str> = BTreeSet::new();
    let mut draws: Vec<MaterializedDraw> = Vec::with_capacity(decl.draws.len());
    for d in &decl.draws {
        if d.name.is_empty() || !draw_names.insert(d.name.as_str()) {
            return Err(E::DuplicateDraw {
                name: d.name.clone(),
            });
        }
        if let Some(b) = &d.band {
            if !["A", "B", "C", "D"].contains(&b.as_str()) {
                return Err(E::UnknownBand {
                    draw: d.name.clone(),
                    band: b.clone(),
                });
            }
        }
        if d.purpose == Purpose::Confirmatory && d.pool == DrawPool::Dev {
            return Err(E::PoolMismatch {
                draw: d.name.clone(),
            });
        }
        let mut candidates: Vec<DrawCandidate<'_>> = Vec::new();
        for c in &cells {
            if d.band.as_ref().is_some_and(|b| &c.band != b) {
                continue;
            }
            let members: &[String] = match d.pool {
                DrawPool::HeldOut => &c.held_out,
                DrawPool::Dev => &c.dev,
            };
            for t in members {
                candidates.push(DrawCandidate {
                    task_id: t.as_str(),
                    cell: c.difficulty.as_str(),
                });
            }
        }
        let draw = stratified_draw(&candidates, d.n, &decl.seed).map_err(|e| match e {
            E::InsufficientPool { have, need, .. } => E::InsufficientPool {
                draw: d.name.clone(),
                have,
                need,
            },
            other => other,
        })?;
        draws.push(MaterializedDraw {
            name: d.name.clone(),
            pool: d.pool.name().to_string(),
            band: d.band.clone(),
            purpose: d.purpose.name().to_string(),
            draw,
        });
    }

    let metadata = MethodMetadata {
        estimator: KIND.to_string(),
        version: VERSION.to_string(),
        target: "seeded declared-field split and stratified draw identities".into(),
        assumptions: vec![
            "strata from declared task.toml fields only (agent timeout band, difficulty)".into(),
            "outcome-blind exclusions recorded before the split; outcome fields refuse".into(),
            "largest-remainder pools; seeded idp_digest within-cell orders".into(),
            "HF saturation is a reporting covariate, never a selection input".into(),
        ],
        prior: None,
        keys: BTreeMap::from([
            (
                "dataset_git_commit".to_string(),
                decl.pin.git_commit.clone(),
            ),
            (
                "hub_dataset_ref".to_string(),
                decl.pin.hub_dataset_ref.clone(),
            ),
            ("selection_seed".to_string(), decl.seed.clone()),
        ]),
        stratum_membership: cells
            .iter()
            .map(|c| format!("{}|{}", c.band, c.difficulty))
            .collect(),
        seed: 0,
        draws_requested: 0,
        draws_used: 0,
        floors: vec![format!("dev_pool_tasks<={}", decl.dev_pool_size)],
        interval: ("descriptive".into(), None),
        refusal: None,
    };

    let mut out = TaskSelection {
        selection_id: String::new(),
        pin: decl.pin.clone(),
        seed: decl.seed.clone(),
        stratification: decl.stratification.clone(),
        exclusion_ref: decl.exclusion_ref.clone(),
        exclusions: decl.exclusions.clone(),
        split_predates_access: decl.split_predates_access,
        cells,
        draws,
        metadata,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("selection_id");
    }
    out.selection_id = idp_digest(DOMAIN_SPLIT, j.to_canonical_string().as_bytes());
    Ok(out)
}
