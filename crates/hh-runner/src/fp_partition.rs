//! `fp_partition` — the W2 L1 corpus partition (manifest 303/R6.42
//! Stage 1; spec §7.4 + REQ-XH-114 primary; REQ-XD-106, REQ-XH-011,
//! REQ-XH-121 contributing).
//!
//! REQ-XH-114 partitions the first-pass L1 natural corpus — W2's
//! RECORD captures of the original mini-swe-agent on the 7 development
//! tasks of §10.1.2 — **by task** under a committed seed into dev /
//! gate-A / gate-B (3 / 2 / 2 tasks; 12 / 8 / 8 trajectories), before
//! H development uses the dev partition. REQ-XD-106 commits the gate
//! partitions by digest and binds every allowlist rerun to a version
//! bump on an unused gate partition. REQ-XH-121's
//! [`crate::fp_seal::dev_projection`] is the only corpus input H
//! development reads.
//!
//! The module owns the `hh-fp-partition/1` record and its checks:
//!
//! - [`build_partition`] draws the partition from the committed
//!   `hh-fp-prereg/1` packet's `task_lists.draws[fp_dev7]` — the
//!   successor task list. The draw is seeded (`sha256` of
//!   `seed|fp_dev7|<cell>|<slot>`, [`crate::schedule::seeded_key`]'s
//!   idiom — one home) and recomputes byte-identically from the
//!   committed seed.
//! - The freeze state is recorded, never hidden: `task_lists.state =
//!   "blocked"` is the honest input — every member carries its slot's
//!   freeze state (`blocked{un_reproofed_successor}`,
//!   `blocked{no_adjacent_donor}`) verbatim. The partition describes
//!   the intended layout; the freeze governs admission (R6.40's CORPUS
//!   leg `withheld{un_reproofed_successor:task_lists_state=blocked}`
//!   verbatim — the partition is over the drawn task list, not
//!   trajectory outputs, which do not exist).
//! - [`check_partition`] is the schema check: 3/2/2 tasks, 12/8/8
//!   trajectories, disjoint by task, digests recomputed, every member
//!   byte-verbatim against the cited packet's slots.
//! - [`check_dev_not_held`] refuses a partition member that also
//!   appears in the held-out manifest (`fp_held10` + the select
//!   commit's `held_out_draw`).
//! - [`check_gate_run`] is REQ-XD-106's version-bump predicate: an
//!   `hh-l1-canon/<v>` rule change after a recorded gate run without a
//!   version bump, or a rerun on a used gate partition, refuses typed.
//! - [`dev_projection_record`] applies R6.30's `dev_projection` to the
//!   dev partition — the sealed projection H development reads; the
//!   gate halves stay sealed (their digests alone bind the input).

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::fp_seal::{check_dev_projection, dev_projection};
use crate::json_ext::JsonExt;
use crate::schedule::seeded_key;

/// `hh-fp-partition/1` — the corpus partition record (committed
/// `docs/build/reports/2026-11-11-fp-partition.json`).
pub const PARTITION_SCHEMA: &str = "hh-fp-partition/1";
/// The declared commit instant — the records are byte-reproducible,
/// never clock-read (the R6.30/R6.33 idiom).
pub const RECORDED_AT: &str = "2026-11-11T00:00:00Z";
/// The committed partition seed. The R6.33 prereg declares no
/// corpus-partition seed — `schedule.seed` is W4's interleave seed —
/// so this seed is committed in the record itself before the draw it
/// produces (REQ-XH-114's "committed seed"; ticket 303's note).
pub const PARTITION_SEED: &str = "fp-partition-r6.42-2026-11-11";
/// The successor task list the partition draws.
pub const DEV_DRAW: &str = "fp_dev7";
/// The held-out draw — no development task may appear in it.
pub const HELD_DRAW: &str = "fp_held10";
/// Trajectories per task — 7 tasks × 4 = the 28-trajectory corpus
/// (REQ-XH-114: 12 / 8 / 8).
pub const TRAJECTORIES_PER_TASK: i64 = 4;
/// The partition names — a closed set.
pub const PARTITION_NAMES: [&str; 3] = ["dev", "gate_a", "gate_b"];
/// The declared split — dev / gate-A / gate-B task counts.
pub const PARTITION_SIZES: [usize; 3] = [3, 2, 2];
/// The gate partitions — `dev` never serves as a gate partition (X-07:
/// the dev partition is fitted on, never re-tested).
pub const GATE_PARTITIONS: [&str; 2] = ["gate_a", "gate_b"];

// ── errors ───────────────────────────────────────────────────────────

/// A typed partition refusal (X-02/X-07: mechanical, never silent).
#[derive(Debug, Clone, PartialEq)]
pub enum PartitionError {
    /// A schema or member-shape failure on strict decode.
    Schema {
        /// The verbatim detail.
        detail: String,
    },
    /// A task appears in two partitions (REQ-XD-106: partitions are
    /// by task).
    Overlap {
        /// The duplicated task.
        task: String,
    },
    /// A partition member also appears in the held-out manifest.
    HeldOut {
        /// The leaked task.
        task: String,
    },
    /// The partition does not recompute from the committed seed.
    Recompute {
        /// The verbatim detail.
        detail: String,
    },
    /// A gate run targeted a partition outside [`GATE_PARTITIONS`].
    GatePartition {
        /// The rejected spelling.
        spelling: String,
    },
    /// A gate run targeted an already-recorded gate partition
    /// (REQ-XD-106: reruns run on an unused gate partition).
    PartitionUsed {
        /// The used partition.
        partition: String,
    },
    /// An `hh-l1-canon` rule change after a recorded gate run without
    /// a version bump (REQ-XD-106).
    CanonUnbumped {
        /// The verbatim detail.
        detail: String,
    },
}

impl std::fmt::Display for PartitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionError::Schema { detail } => write!(f, "{PARTITION_SCHEMA}: {detail}"),
            PartitionError::Overlap { task } => write!(
                f,
                "{PARTITION_SCHEMA}: task {task} in two partitions — partitions are by task (REQ-XD-106)"
            ),
            PartitionError::HeldOut { task } => write!(
                f,
                "{PARTITION_SCHEMA}: partition member {task} is in the held-out manifest (REQ-XH-114)"
            ),
            PartitionError::Recompute { detail } => {
                write!(f, "{PARTITION_SCHEMA}: recompute mismatch — {detail}")
            }
            PartitionError::GatePartition { spelling } => write!(
                f,
                "refused{{gate_partition}}: {spelling} — a gate run targets gate_a|gate_b, never dev (REQ-XD-106)"
            ),
            PartitionError::PartitionUsed { partition } => write!(
                f,
                "refused{{partition_used}}: {partition} — a rerun requires an unused gate partition (REQ-XD-106)"
            ),
            PartitionError::CanonUnbumped { detail } => write!(
                f,
                "refused{{canon_unbumped}}: {detail} — an hh-l1-canon rule change after a recorded gate run requires a version bump (REQ-XD-106)"
            ),
        }
    }
}
impl std::error::Error for PartitionError {}

fn schema(detail: impl Into<String>) -> PartitionError {
    PartitionError::Schema {
        detail: detail.into(),
    }
}

// ── the inputs manifest ──────────────────────────────────────────────

/// The emitter's inputs manifest — `name=relpath` pairs; the reader
/// rule keeps the committed `docs/` relpaths at script level
/// (`scripts/fp-partition-inputs.txt`), never in the crate.
#[derive(Debug, Clone)]
pub struct PartitionInputs {
    /// The committed `hh-fp-prereg/1` packet (the task_lists source).
    pub packet: String,
    /// The committed `hh-fp-select/1` select commit (held_out_draw).
    pub select_commit: String,
    /// The committed `hh-fp-exclusion-record/1`.
    pub exclusion: String,
    /// The partition record path the emitter writes.
    pub partition: String,
    /// The partition hash sidecar path.
    pub partition_sha256: String,
    /// The sealed dev-projection record path the emitter writes.
    pub dev_projection: String,
    /// The dev-projection hash sidecar path.
    pub dev_projection_sha256: String,
}

impl PartitionInputs {
    /// The manifest names [`PartitionInputs::from_manifest`] accepts.
    pub const NAMES: [&str; 7] = [
        "packet",
        "select_commit",
        "exclusion",
        "partition",
        "partition_sha256",
        "dev_projection",
        "dev_projection_sha256",
    ];

    /// Parse the inputs manifest — `name=relpath` lines; `#` comments
    /// and blank lines are skipped. A malformed line, a duplicated or
    /// unknown name, and a missing required name each refuse typed.
    pub fn from_manifest(body: &str) -> Result<Self, PartitionError> {
        let mut m = BTreeMap::new();
        for (n, line) in body.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line.split_once('=').ok_or_else(|| {
                schema(format!(
                    "inputs manifest line {}: expected name=relpath",
                    n + 1
                ))
            })?;
            let (k, v) = (k.trim(), v.trim());
            if !Self::NAMES.contains(&k) {
                return Err(schema(format!(
                    "inputs manifest line {}: unknown name {k}",
                    n + 1
                )));
            }
            if v.is_empty() {
                return Err(schema(format!(
                    "inputs manifest line {}: empty relpath for {k}",
                    n + 1
                )));
            }
            if m.insert(k.to_string(), v.to_string()).is_some() {
                return Err(schema(format!(
                    "inputs manifest line {}: duplicate name {k}",
                    n + 1
                )));
            }
        }
        let get = |name: &str| -> Result<String, PartitionError> {
            m.get(name)
                .cloned()
                .ok_or_else(|| schema(format!("inputs manifest: required name {name} absent")))
        };
        Ok(Self {
            packet: get("packet")?,
            select_commit: get("select_commit")?,
            exclusion: get("exclusion")?,
            partition: get("partition")?,
            partition_sha256: get("partition_sha256")?,
            dev_projection: get("dev_projection")?,
            dev_projection_sha256: get("dev_projection_sha256")?,
        })
    }
}

// ── the draw ─────────────────────────────────────────────────────────

/// The task spellings a slot occupies — `member` (the admitted or
/// blocked-successor task) and `drawn` (the original draw). Disjoint
/// and held-out checks consult both: a partition is by task, and the
/// slot's task identity covers the drawn member and any successor the
/// freeze walk reached.
fn slot_tasks(slot: &Json) -> Vec<String> {
    let mut out = Vec::new();
    for k in ["member", "drawn"] {
        if let Some(t) = slot.get(k).and_then(Json::as_str) {
            if !out.iter().any(|s| s == t) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// `task_lists.draws[name].cells[].slots[]` in committed order.
fn draw_slots(task_lists: &Json, draw: &str) -> Result<Vec<(String, Json)>, PartitionError> {
    let draws = task_lists
        .get("draws")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| schema("task_lists.draws absent"))?;
    let d = draws
        .iter()
        .find(|d| d.get("name").and_then(Json::as_str) == Some(draw))
        .ok_or_else(|| schema(format!("task_lists.draws has no {draw}")))?;
    let cells = d
        .get("cells")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| schema(format!("draw {draw} cells absent")))?;
    let mut out = Vec::new();
    for c in cells {
        let cell = c
            .get("cell")
            .and_then(Json::as_str)
            .ok_or_else(|| schema(format!("draw {draw} cell name absent")))?
            .to_string();
        let slots = c
            .get("slots")
            .and_then(JsonExt::as_arr)
            .ok_or_else(|| schema(format!("draw {draw} cell {cell} slots absent")))?;
        for s in slots {
            out.push((cell.clone(), s.clone()));
        }
    }
    Ok(out)
}

/// The seeded member list: every `fp_dev7` slot verbatim plus `cell`
/// and its `partition_key`, in the seeded order the split consumes.
pub fn draw_members(task_lists: &Json, seed: &str) -> Result<Vec<Json>, PartitionError> {
    let slots = draw_slots(task_lists, DEV_DRAW)?;
    if slots.len() != PARTITION_SIZES.iter().sum() {
        return Err(schema(format!(
            "draw {DEV_DRAW} has {} slots — the corpus partition needs {}",
            slots.len(),
            PARTITION_SIZES.iter().sum::<usize>()
        )));
    }
    let mut keyed: Vec<(String, Json)> = Vec::with_capacity(slots.len());
    for (cell, slot) in &slots {
        let idx = match slot.get("slot") {
            Some(Json::Int(i)) => *i,
            _ => return Err(schema(format!("{DEV_DRAW} cell {cell} slot index absent"))),
        };
        let key = seeded_key(seed, &[DEV_DRAW, cell.as_str(), &idx.to_string()]);
        let mut m = slot.clone();
        if let Json::Obj(ref mut mm) = m {
            mm.insert("cell".to_string(), Json::str(cell.clone()));
            mm.insert("partition_key".to_string(), Json::str(&key));
        }
        keyed.push((key, m));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(keyed.into_iter().map(|(_, m)| m).collect())
}

/// `partition_object(name, members)` — `{members, name, tasks,
/// trajectories}` — the canonical content a partition digest binds.
fn partition_content(name: &str, members: &[Json]) -> Json {
    Json::obj([
        ("members", Json::Arr(members.to_vec())),
        ("name", Json::str(name)),
        ("tasks", Json::Int(members.len() as i64)),
        (
            "trajectories",
            Json::Int(members.len() as i64 * TRAJECTORIES_PER_TASK),
        ),
    ])
}

/// `partition_digest(name, members)` — `sha256:` over the canonical
/// bytes of [`partition_content`]. The corpus's partition input is
/// frozen by digest (REQ-XD-106).
pub fn partition_digest(name: &str, members: &[Json]) -> String {
    format!(
        "sha256:{}",
        sha256_hex(
            partition_content(name, members)
                .to_canonical_string()
                .as_bytes()
        )
    )
}

fn cite(path: &str, sha: &str) -> Json {
    Json::obj([("path", Json::str(path)), ("sha256", Json::str(sha))])
}

/// `build_partition(packet, inputs, packet_sha, select_commit_sha,
/// select_id, exclusion_sha)` — the `hh-fp-partition/1` record: the
/// seeded 3/2/2 by-task split of `task_lists.draws[fp_dev7]` with the
/// freeze state verbatim, per-partition digests, and the committed
/// citations (packet, select commit, exclusion record). Deterministic —
/// same packet + seed, byte-identical record.
pub fn build_partition(
    packet: &Json,
    inputs: &PartitionInputs,
    packet_sha: &str,
    select_commit_sha: &str,
    select_id: &str,
    exclusion_sha: &str,
) -> Result<Json, PartitionError> {
    if packet.get("schema").and_then(Json::as_str) != Some(crate::fp_prereg::PREREG_SCHEMA) {
        return Err(schema(format!(
            "source schema is not {}",
            crate::fp_prereg::PREREG_SCHEMA
        )));
    }
    let task_lists = packet
        .get("task_lists")
        .ok_or_else(|| schema("packet.task_lists absent"))?;
    let tl_state = task_lists
        .get("state")
        .and_then(Json::as_str)
        .ok_or_else(|| schema("packet.task_lists.state absent"))?;
    let pin = packet
        .get("pin")
        .cloned()
        .ok_or_else(|| schema("packet.pin absent"))?;
    let members = draw_members(task_lists, PARTITION_SEED)?;

    let mut partitions = Vec::new();
    let mut cursor = 0usize;
    for (name, size) in PARTITION_NAMES.iter().zip(PARTITION_SIZES.iter()) {
        let part_members: Vec<Json> = members[cursor..cursor + size].to_vec();
        cursor += size;
        let mut p = partition_content(name, &part_members);
        if let Json::Obj(ref mut m) = p {
            m.insert(
                "sha256".to_string(),
                Json::str(partition_digest(name, &part_members)),
            );
        }
        partitions.push(p);
    }

    let mut sc = cite(&inputs.select_commit, select_commit_sha);
    if let Json::Obj(ref mut m) = sc {
        m.insert("select_id".to_string(), Json::str(select_id));
    }
    Ok(Json::obj([
        ("schema", Json::str(PARTITION_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("seed", Json::str(PARTITION_SEED)),
        (
            "seed_basis",
            Json::str("committed before drawing — the R6.33 prereg declares no corpus-partition seed (schedule.seed is the W4 interleave); this seed is committed in this record, outcome-blind by construction: the task_lists carry freeze states, never outcome fields (REQ-XH-114)"),
        ),
        (
            "rule",
            Json::str("partitions are by task (REQ-XD-106): the 7 fp_dev7 slots in committed order, keyed sha256(seed|fp_dev7|cell|slot), sorted ascending; first 3 → dev, next 2 → gate_a, last 2 → gate_b — 3/2/2 tasks, 12/8/8 trajectories at 4 per task (REQ-XH-114)"),
        ),
        (
            "digest_rule",
            Json::str("sha256 over Json::to_canonical_string of each partition's {members, name, tasks, trajectories} — the corpus's partition input is frozen by digest (REQ-XD-106)"),
        ),
        (
            "source",
            Json::obj([
                ("prereg", cite(&inputs.packet, packet_sha)),
                ("select_commit", sc),
                ("exclusion_record", cite(&inputs.exclusion, exclusion_sha)),
                ("draw", Json::str(DEV_DRAW)),
                ("task_lists_state", Json::str(tl_state)),
                ("pin", pin),
                (
                    "corpus_status",
                    Json::str("withheld{un_reproofed_successor:task_lists_state=blocked} — the R6.40 CORPUS leg verbatim; the partition is over the drawn task list, not trajectory outputs (the corpus's execution records do not exist — never invented)"),
                ),
            ]),
        ),
        (
            "freeze_state",
            Json::str("blocked — recorded, never hidden: every member carries its slot's freeze state verbatim (blocked{un_reproofed_successor}, blocked{no_adjacent_donor}); the partition describes the intended layout while task_lists.state governs admission — W1's re-proof register recorded every drawn member excluded oracle_fails_on_O and the successors are un-re-proofed"),
        ),
        ("partitions", Json::Arr(partitions)),
        (
            "dev_projection",
            Json::obj([
                ("path", Json::str(&inputs.dev_projection)),
                ("schema", Json::str(crate::fp_seal::DEV_PROJECTION_SCHEMA)),
                (
                    "rule",
                    Json::str("REQ-XH-114 + REQ-XH-121 — the sealed dev projection (verdict, reward and verifier fields removed) is the only corpus input H development reads; the gate partitions stay sealed — their digests alone bind the corpus input"),
                ),
            ]),
        ),
        (
            "allowlist_rule",
            Json::str("REQ-XD-106 — an hh-l1-canon/<v> rule change after a recorded gate run requires a version bump and a rerun on an unused gate partition; check_gate_run refuses both violations typed"),
        ),
        (
            "gate_consumption",
            Json::str("the gate partitions are consumed by L1 gate runs only; a run's hh-l1-conformance/1 pass id is what unblocks the HH-native arm — the runner withholds a native-lane unit without an exact-H pass: withheld{l1_not_passed}, pair-atomic (R6.41's drive_window refusal, cited verbatim)"),
        ),
    ]))
}

/// `dev_projection_record(partition_record, partition_sha)` — the
/// sealed `hh-fp-dev-projection/1` document: R6.30's `dev_projection`
/// over the dev partition object (verdict, reward and verifier
/// members *removed*, never masked) plus the citations — the seed,
/// the source partition record and the two gate digests (a digest
/// binds, it does not leak; no gate-half member content appears).
/// The only corpus input H development reads (REQ-XH-114, REQ-XH-011).
pub fn dev_projection_record(
    partition_record: &Json,
    partition_path: &str,
    partition_sha: &str,
) -> Result<Json, PartitionError> {
    let parts = partition_record
        .get("partitions")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| schema("partitions absent"))?;
    let by_name = |n: &str| -> Result<&Json, PartitionError> {
        parts
            .iter()
            .find(|p| p.get("name").and_then(Json::as_str) == Some(n))
            .ok_or_else(|| schema(format!("partition {n} absent")))
    };
    let dev = by_name("dev")?;
    let seed = partition_record
        .get("seed")
        .and_then(Json::as_str)
        .ok_or_else(|| schema("seed absent"))?;
    let gate_digest = |n: &str| -> Result<String, PartitionError> {
        by_name(n)?
            .get("sha256")
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| schema(format!("partition {n} sha256 absent")))
    };
    let proj = dev_projection(dev);
    let record = proj
        .get("record")
        .cloned()
        .ok_or_else(|| schema("dev_projection record member absent"))?;
    check_dev_projection(&record)
        .map_err(|e| schema(format!("dev projection failed the schema check: {e}")))?;
    Ok(Json::obj([
        ("schema", Json::str(crate::fp_seal::DEV_PROJECTION_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("seed", Json::str(seed)),
        (
            "partition",
            Json::obj([
                ("path", Json::str(partition_path)),
                ("sha256", Json::str(partition_sha)),
                ("name", Json::str("dev")),
                (
                    "partition_sha256",
                    Json::str(
                        dev.get("sha256")
                            .and_then(Json::as_str)
                            .unwrap_or_default(),
                    ),
                ),
            ]),
        ),
        (
            "gate_digests",
            Json::obj([
                ("gate_a", Json::str(gate_digest("gate_a")?)),
                ("gate_b", Json::str(gate_digest("gate_b")?)),
            ]),
        ),
        (
            "boundary",
            Json::str("the only corpus input H development reads (REQ-XH-114, REQ-XH-011): the dev partition's members with verdict, reward and verifier members removed — no gate-half task content, no outcome field; check_dev_projection refuses a verdict member"),
        ),
        ("record", record),
    ]))
}

// ── the checks ───────────────────────────────────────────────────────

/// `check_partition(record, packet)` — the `hh-fp-partition/1` schema
/// check: exactly the three declared partitions at 3/2/2 tasks and
/// 12/8/8 trajectories; disjoint by task (no task in two partitions);
/// every member byte-verbatim against the cited packet's `fp_dev7`
/// slots (plus `cell`/`partition_key`); every partition's `sha256`
/// recomputes; `source.task_lists_state` matches the packet.
pub fn check_partition(record: &Json, packet: &Json) -> Result<(), PartitionError> {
    if record.get("schema").and_then(Json::as_str) != Some(PARTITION_SCHEMA) {
        return Err(schema(format!("schema is not {PARTITION_SCHEMA}")));
    }
    let task_lists = packet
        .get("task_lists")
        .ok_or_else(|| schema("packet.task_lists absent"))?;
    if record
        .get("source")
        .and_then(|s| s.get("task_lists_state"))
        .and_then(Json::as_str)
        != task_lists.get("state").and_then(Json::as_str)
    {
        return Err(schema("source.task_lists_state != packet.task_lists.state"));
    }
    let expected = draw_members(task_lists, PARTITION_SEED)?;
    let parts = record
        .get("partitions")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| schema("partitions absent"))?;
    if parts.len() != PARTITION_NAMES.len() {
        return Err(schema(format!(
            "partitions has {} members — needs {}",
            parts.len(),
            PARTITION_NAMES.len()
        )));
    }
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut cursor = 0usize;
    for (i, name) in PARTITION_NAMES.iter().enumerate() {
        let p = &parts[i];
        if p.get("name").and_then(Json::as_str) != Some(name) {
            return Err(schema(format!("partitions[{i}].name is not {name}")));
        }
        let members = p
            .get("members")
            .and_then(JsonExt::as_arr)
            .ok_or_else(|| schema(format!("partition {name} members absent")))?;
        if members.len() != PARTITION_SIZES[i] {
            return Err(schema(format!(
                "partition {name} has {} members — needs {}",
                members.len(),
                PARTITION_SIZES[i]
            )));
        }
        match p.get("tasks") {
            Some(Json::Int(n)) if *n == PARTITION_SIZES[i] as i64 => {}
            _ => {
                return Err(schema(format!(
                    "partition {name} tasks != {}",
                    PARTITION_SIZES[i]
                )))
            }
        }
        match p.get("trajectories") {
            Some(Json::Int(t)) if *t == PARTITION_SIZES[i] as i64 * TRAJECTORIES_PER_TASK => {}
            _ => {
                return Err(schema(format!(
                    "partition {name} trajectories != {}",
                    PARTITION_SIZES[i] as i64 * TRAJECTORIES_PER_TASK
                )))
            }
        }
        for m in members {
            // every member is the packet's slot verbatim + cell + key
            let want = &expected[cursor];
            if m != want {
                return Err(PartitionError::Recompute {
                    detail: format!("partition {name} member at draw position {cursor} is not the seeded packet slot verbatim"),
                });
            }
            cursor += 1;
            if m.get("state").and_then(Json::as_str).is_none() {
                return Err(schema(format!(
                    "partition {name} member lacks its freeze state verbatim"
                )));
            }
            for t in slot_tasks(m) {
                if let Some(other) = seen.insert(t.clone(), name) {
                    return Err(PartitionError::Overlap {
                        task: format!("{t} (in {other} and {name})"),
                    });
                }
            }
        }
        let want_digest = partition_digest(name, members);
        if p.get("sha256").and_then(Json::as_str) != Some(want_digest.as_str()) {
            return Err(PartitionError::Recompute {
                detail: format!("partition {name} sha256 does not recompute"),
            });
        }
    }
    if record.get("seed").and_then(Json::as_str) != Some(PARTITION_SEED) {
        return Err(schema("seed is not the committed seed"));
    }
    Ok(())
}

/// `check_dev_not_held(record, packet, select_commit)` — no partition
/// member's task (drawn or successor) appears in the held-out
/// manifest: the freeze's `fp_held10` draw members *and* the select
/// commit's `held_out_draw.drawn` (REQ-XH-114: "no development task is
/// in the held-out manifest").
pub fn check_dev_not_held(
    record: &Json,
    packet: &Json,
    select_commit: &Json,
) -> Result<(), PartitionError> {
    let task_lists = packet
        .get("task_lists")
        .ok_or_else(|| schema("packet.task_lists absent"))?;
    let mut held: BTreeSet<String> = BTreeSet::new();
    for (_cell, slot) in draw_slots(task_lists, HELD_DRAW)? {
        held.extend(slot_tasks(&slot));
    }
    if let Some(drawn) = select_commit
        .get("held_out_draw")
        .and_then(|d| d.get("drawn"))
        .and_then(JsonExt::as_arr)
    {
        held.extend(drawn.iter().filter_map(|t| t.as_str().map(str::to_string)));
    }
    let parts = record
        .get("partitions")
        .and_then(JsonExt::as_arr)
        .ok_or_else(|| schema("partitions absent"))?;
    for p in parts {
        let name = p.get("name").and_then(Json::as_str).unwrap_or("?");
        for m in p
            .get("members")
            .and_then(JsonExt::as_arr)
            .ok_or_else(|| schema(format!("partition {name} members absent")))?
        {
            for t in slot_tasks(m) {
                if held.contains(&t) {
                    return Err(PartitionError::HeldOut { task: t });
                }
            }
        }
    }
    Ok(())
}

// ── REQ-XD-106: the gate-run admission predicate ─────────────────────

/// One recorded or requested L1 gate run — the partition it consumed,
/// the `hh-l1-canon/<v>` version and the allowlist rules digest it ran
/// under.
#[derive(Debug, Clone, PartialEq)]
pub struct GateRun {
    /// The gate partition consumed (`gate_a` | `gate_b`).
    pub partition: String,
    /// The `hh-l1-canon/<v>` version label.
    pub canon_version: String,
    /// The allowlist rules digest the run canonicalized under.
    pub rules_digest: String,
}

/// `check_gate_run(recorded, requested)` — REQ-XD-106's admission
/// predicate. Refuses: a partition outside [`GATE_PARTITIONS`]
/// ([`PartitionError::GatePartition`] — `dev` never serves as a gate
/// partition); a rerun on an already-recorded partition
/// ([`PartitionError::PartitionUsed`]); an allowlist rule change after
/// a recorded gate run under an unchanged version
/// ([`PartitionError::CanonUnbumped`]). A rule change admits only on a
/// bumped `canon_version` over an unused partition.
pub fn check_gate_run(recorded: &[GateRun], requested: &GateRun) -> Result<(), PartitionError> {
    if !GATE_PARTITIONS.contains(&requested.partition.as_str()) {
        return Err(PartitionError::GatePartition {
            spelling: requested.partition.clone(),
        });
    }
    if let Some(_r) = recorded.iter().find(|r| r.partition == requested.partition) {
        return Err(PartitionError::PartitionUsed {
            partition: requested.partition.clone(),
        });
    }
    for r in recorded {
        if r.rules_digest != requested.rules_digest && r.canon_version == requested.canon_version {
            return Err(PartitionError::CanonUnbumped {
                detail: format!(
                    "rules_digest changed after {}'s recorded run under unchanged {}",
                    r.partition, r.canon_version
                ),
            });
        }
    }
    Ok(())
}
