//! `fp_prereg` — the first-pass pre-registration packet
//! (`hh-fp-prereg/1`, manifest 291/R6.33; spec §6.7, §10.1.2–§10.1.4,
//! §10.1.6, §8.1 V5; REQ-XH-111/113/115/116/117, REQ-XD-115,
//! REQ-XH-119/122/123/126, REQ-XH-127(c)).
//!
//! Three owned records plus the freeze machinery:
//!
//! - [`freeze_post_replacement`] — the post-replacement freeze: W1's
//!   typed exclusions (re-proof `o_fail`/flaky and `allowlist_gap{host}`
//!   verdicts, plus the SELECT-time download declarations) applied to
//!   R6.30's committed within-cell seeded orders. A member that was not
//!   re-proofed in W1 is never admitted: the slot refuses typed
//!   `blocked{un_reproofed_successor}` — it needs the contingency
//!   window (REQ-XH-116). A development-task exclusion that empties
//!   its cell draws from the adjacent cell's committed order and sets
//!   the disclosure (REQ-XH-127(c)); an emptied medium dev cell has no
//!   declared donor — `blocked{no_adjacent_donor}` (R6.29's recorded
//!   reading in `holds.rs`, CC1 — the mapping is not duplicated).
//! - `FpPreregPacket` — `hh-fp-prereg/1`: every §6.7 member applying
//!   to the first pass — the pin/split/seed/exclusion citations, the
//!   allowlist freeze, the arms, effort `high` (Q-X22), ε = ±.20 with
//!   the N = 50 P(`equivalent`) table (Q-X53), the declared schedule
//!   seed + the 5-round interleave with O/H pairs launched together,
//!   the arm priority (Q-X61 recorded default, `pending` labelled),
//!   the projection/wall-time/slice/per-arm-cap formulas (values at
//!   FP-E), the fallback trigger quoted verbatim from Q-X62 (`pending`),
//!   H fixed by rule (REQ-XH-114), the claims block, the trial-reuse
//!   declaration (REQ-XH-113), AR-33's departures cited, and the V5
//!   declaration as its own canonical-JSON sub-object with its own
//!   sha256 (Q-X56 text, the REQ-XH-115 label). The packet hash is
//!   sha256 over the canonical packet — committed beside it with a
//!   recompute command (`scripts/check-fp-prereg.sh`).
//! - [`w2_quote`] — the `comparative-quote/1` GATE-G26 presents:
//!   the W2 model lines at §10.1.5's binding values (UNVERIFIED
//!   planning inputs, labelled) and the five committed
//!   `hh-compute-quote/1` lines cited verbatim (priced by R6.14 only,
//!   C-11) — Σ ceilings ≤ 20 000 000 µ$ with every term. The quote
//!   reads no outcome field (X-07) and authorizes nothing (X-02).
//! - [`compute_readiness`] — `hh-fp-readiness/1`: the offline, $0
//!   recompute over landed records (this packet, HUMAN-H14's readout
//!   date, W1's verified harvest manifest) yielding the instant every
//!   FP-S precondition except the E2-T8 rung record is met — the
//!   `pending{human_h14_not_run}` state is committed now; the rung
//!   record is never an input (Q-X62's trigger starts from
//!   "otherwise ready", nothing pre-dated).
//!
//! Margin rule (REQ-XD-115): ε is explicit everywhere — constructing
//! an L2 packet or quote without an explicit ε refuses typed
//! (`no_default_margin`); the first-pass exception cites Q-X53 and
//! §10.1.3's N = 50 table.

use std::collections::BTreeMap;

use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuote, ComparativeQuoteDecl, ComputeLine, Envelope,
    MenuRowDecl,
};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;
use crate::reproof::PACKET_SCHEMA as REPROOF_SCHEMA;

/// `hh-fp-prereg/1` — the packet kind.
pub const PREREG_SCHEMA: &str = "hh-fp-prereg/1";
/// `hh-fp-readiness/1` — the FP-S readiness record kind.
pub const READINESS_SCHEMA: &str = "hh-fp-readiness/1";
/// The declared commit timestamp — the contract fixes it; the record
/// is byte-reproducible, never clock-read (R6.30's idiom).
pub const RECORDED_AT: &str = "2026-11-10T00:00:00Z";
/// The declared W4 schedule seed — outcome-blind at declaration; the
/// schedule recomputes from it (R6.16's `build_schedule`).
pub const SCHEDULE_SEED: &str = "fp-w4-schedule-r6.33-2026-11-10";

/// The committed-record relpaths the emitter reads, writes and cites.
/// The reader rule keeps `docs/` literals out of the crate: the paths
/// arrive at run time from the inputs manifest — newline
/// `name=relpath` pairs in `scripts/fp-prereg-inputs.txt`, named by
/// `HH_FP_INPUTS` (the emitter also honours
/// `<repo>/scripts/fp-prereg-inputs.txt` when the env var is unset).
/// The packet and readiness citations emit exactly the supplied
/// strings, so the manifest is the single source for the read path
/// and the cited path alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreregInputs {
    /// The `task-selection/1` record.
    pub selection: String,
    /// The `hh-fp-select/1` commit record.
    pub select_commit: String,
    /// The `hh-fp-exclusion-record/1` record.
    pub exclusion: String,
    /// The W1 held-out re-proof packet.
    pub reproof_held: String,
    /// The W1 held-out discordance register.
    pub reproof_held_register: String,
    /// The W1 development re-proof packet.
    pub reproof_dev: String,
    /// The W1 development discordance register.
    pub reproof_dev_register: String,
    /// The committed `hh-live-evidence/1` for the re-proof leg.
    pub reproof_evidence: String,
    /// The W1 verified harvest manifest (REQ-XB-220).
    pub w1_manifest: String,
    /// The HUMAN-H14 readout — existence-checked only while the
    /// readiness record stays pending.
    pub human_h14_readout: String,
    /// The packet path the emitter writes.
    pub packet: String,
    /// The sidecar carrying the packet hash + the recompute command.
    pub packet_sha256: String,
    /// The W2 `comparative-quote/1` path the emitter writes.
    pub w2_quote: String,
    /// The FP-S readiness record path the emitter writes.
    pub readiness: String,
}

impl PreregInputs {
    /// The manifest names [`PreregInputs::from_manifest`] accepts.
    pub const NAMES: [&str; 14] = [
        "selection",
        "select_commit",
        "exclusion",
        "reproof_held",
        "reproof_held_register",
        "reproof_dev",
        "reproof_dev_register",
        "reproof_evidence",
        "w1_manifest",
        "human_h14_readout",
        "packet",
        "packet_sha256",
        "w2_quote",
        "readiness",
    ];

    /// Parse the inputs manifest — `name=relpath` lines; `#` comments
    /// and blank lines are skipped. A malformed line, a duplicated or
    /// unknown name, and a missing required name each refuse typed.
    pub fn from_manifest(body: &str) -> Result<Self, PreregError> {
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
        let get = |name: &str| -> Result<String, PreregError> {
            m.get(name)
                .cloned()
                .ok_or_else(|| schema(format!("inputs manifest: required name {name} absent")))
        };
        Ok(Self {
            selection: get("selection")?,
            select_commit: get("select_commit")?,
            exclusion: get("exclusion")?,
            reproof_held: get("reproof_held")?,
            reproof_held_register: get("reproof_held_register")?,
            reproof_dev: get("reproof_dev")?,
            reproof_dev_register: get("reproof_dev_register")?,
            reproof_evidence: get("reproof_evidence")?,
            w1_manifest: get("w1_manifest")?,
            human_h14_readout: get("human_h14_readout")?,
            packet: get("packet")?,
            packet_sha256: get("packet_sha256")?,
            w2_quote: get("w2_quote")?,
            readiness: get("readiness")?,
        })
    }
}

/// The harness admission records (R6.22's committed set — `crates/`
/// paths, cited and sha256-bound by the packet's arms table).
pub const ADMISSION_RECORDS: [&str; 3] = [
    "crates/hh-runner/tests/fixtures/r6_22/mini-swe-agent.json",
    "crates/hh-runner/tests/fixtures/r6_22/pi.json",
    "crates/hh-runner/tests/fixtures/r6_22/claude-code.json",
];
/// The five window declarations carrying each compute quote's
/// `authorization` block (`pq-` id + ceiling).
pub const WINDOW_DECL_PATHS: [&str; 5] = [
    "scripts/gce-backend/declarations/fp-w1.json",
    "scripts/gce-backend/declarations/fp-w2.json",
    "scripts/gce-backend/declarations/fp-w3.json",
    "scripts/gce-backend/declarations/fp-w4.json",
    "scripts/gce-backend/declarations/fp-wc.json",
];

/// Q-X56's ledger default, verbatim (Appendix B). `pending` — a
/// recorded default is never an operator answer (X-03).
pub const Q_X56_VERBATIM: &str =
    "First-pass optional phase-4 variant = V5 `rsn-drop` (spec v0.2 default)";
/// Q-X57's ledger default, verbatim.
pub const Q_X57_VERBATIM: &str = "First-pass phase 4 = one pre-registered single contrast, a scoped departure from Q-X15 explore→confirm (kept for later passes)";
/// Q-X61's ledger default, verbatim.
pub const Q_X61_VERBATIM: &str = "Original and HH-native mini-swe-agent protected from cost-based dropping; truncation order V5 → Claude Code → Pi";
/// Q-X62's ledger default, verbatim — the fallback trigger the packet
/// quotes in full (REQ-XH-123).
pub const Q_X62_VERBATIM: &str = "First-pass spend gate (FP-S) waits for the E2-T8 rung record; hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready (14 is a synthesis placeholder)";
/// The recorded default day count inside Q-X62's trigger (labelled
/// pending — the operator fixes the number at ratification/FP-S).
pub const Q_X62_TRIGGER_AFTER_DAYS: u64 = 14;
/// H's identity rule (REQ-XH-114, verbatim phrase).
pub const H_RULE: &str = "the version that passes G-L1";
/// The REQ-XH-119 first-pass label.
pub const FP_LABEL: &str = "first pass — capability demonstration on 10 held-out TB 2.1 tasks at k = 5; below the declared precision target; not leaderboard-comparable";
/// The REQ-XH-119 P2-FP/P4-FP power label.
pub const POWER_LABEL: &str =
    "pipeline demonstration; confirmatory test pre-stated as near-zero power";
/// The REQ-XH-122 scoped L2 label.
pub const L2_LABEL: &str =
    "first-pass L2 equivalent at a ±.20 margin (N = 50; gross-infidelity screen; not parity-verified)";
/// The REQ-XH-115 V5 label.
pub const V5_LABEL: &str =
    "single pre-registered contrast; no exploration; local to the baseline configuration";
/// The REQ-XH-113 dependence sentence.
pub const DEPENDENCE: &str =
    "the claims share trials and are dependent; no cross-claim familywise control is claimed";
/// The §10.1.4 cost-correlation label every drop carries.
pub const DROP_LABEL: &str = "cost-triggered; cost correlates with failure length";

/// A typed refusal — never a silent downgrade (X-03/X-07/honest refusal).
#[derive(Debug, Clone, PartialEq)]
pub enum PreregError {
    /// A required member absent or malformed.
    Schema { detail: String },
    /// A cited record's sha256 does not recompute (drift).
    Digest { member: String },
    /// An L2 packet or quote was built without an explicit ε —
    /// `no_default_margin` (REQ-XD-115; the first-pass exception cites
    /// Q-X53, never a default).
    MissingEpsilon,
    /// The packet or the readiness record encountered an outcome
    /// field (X-07/REQ-XH-121).
    OutcomeField { member: String },
}

impl std::fmt::Display for PreregError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreregError::Schema { detail } => write!(f, "{PREREG_SCHEMA}: {detail}"),
            PreregError::Digest { member } => {
                write!(f, "{PREREG_SCHEMA}: cited record drifted at {member}")
            }
            PreregError::MissingEpsilon => write!(
                f,
                "refused{{no_default_margin}}: an explicit ε is required (REQ-XD-115)"
            ),
            PreregError::OutcomeField { member } => {
                write!(f, "refused{{outcome_field}}: {member}")
            }
        }
    }
}
impl std::error::Error for PreregError {}

fn schema(detail: impl Into<String>) -> PreregError {
    PreregError::Schema {
        detail: detail.into(),
    }
}

/// The ε rule: explicit or typed refusal — there is no default margin
/// anywhere (REQ-XD-115; Q-X34). `ppm` is the one-sided margin in
/// parts per million (±.20 → `200_000`).
pub fn require_epsilon(ppm: Option<u64>) -> Result<u64, PreregError> {
    ppm.filter(|p| *p > 0).ok_or(PreregError::MissingEpsilon)
}

/// The first-pass margin declaration — the one operator-ruled
/// exception (Q-X53) citing §10.1.3's N = 50 table.
pub fn margin_decl() -> Result<Json, PreregError> {
    let ppm = require_epsilon(Some(200_000))?;
    Ok(Json::obj([
        ("epsilon_ppm", Json::Int(ppm as i64)),
        ("epsilon", Json::str("±.20")),
        ("decision", Json::str("Q-X53 (answered): \"±.20, expect inconclusive (Recommended)\"")),
        ("exception_basis", Json::str("REQ-XD-115's one operator-ruled first-pass exception — no default margin exists anywhere else")),
        (
            "n50_p_equivalent_table",
            Json::Arr(
                [
                    ("Δ = 0, q± = .117 (central prior)", 16_i64, 217_i64),
                    ("Δ = 0, q± = .14", 3, 86),
                    ("Δ = 0, q± = .05", 572, 885),
                    ("true Δ = +.10 (q+ = .167, q− = .067)", 11, 116),
                    ("true Δ = +.15 (q+ = .192, q− = .042)", 6, 51),
                ]
                .iter()
                .map(|(s, p20, p25)| {
                    Json::obj([
                        ("scenario", Json::str(*s)),
                        ("p_equivalent_at_eps_20_ppm", Json::Int(*p20)),
                        ("p_equivalent_at_eps_25_ppm_reference", Json::Int(*p25)),
                    ])
                })
                .collect(),
            ),
        ),
        (
            "table_note",
            Json::str("§10.1.3 P3-L2 at N = 50 — exact trinomial enumeration over (c+, c−) with the K_η bound, η = .025 and the strict verdict rule; O2 recomputation, reproduced at revision; per-mille P(equivalent) at ε = ±.20 (±.25 for reference)"),
        ),
        ("tost_half_width", Json::str(".90 TOST half-width ≈ .246 at q = .117")),
        ("expected", Json::str("inconclusive")),
    ]))
}

// ── the post-replacement freeze ─────────────────────────────────────

/// One reproof packet's `task → verdict` map (`admit` |
/// `exclude:<basis>` | `repair:<class>`).
pub fn reproof_verdicts(packet: &Json) -> Result<BTreeMap<String, String>, PreregError> {
    if packet.get("schema").and_then(Json::as_str) != Some(REPROOF_SCHEMA) {
        return Err(schema(format!("packet schema is not {REPROOF_SCHEMA}")));
    }
    let tasks = packet
        .get("tasks")
        .and_then(Json::as_obj)
        .ok_or_else(|| schema("tasks member absent"))?;
    let mut out = BTreeMap::new();
    for (task, row) in tasks {
        let verdict = row
            .get("verdict")
            .and_then(Json::as_str)
            .ok_or_else(|| schema(format!("tasks.{task}.verdict absent")))?;
        out.insert(task.clone(), verdict.to_string());
    }
    Ok(out)
}

/// Is `task` excluded under the supplied maps? Basis =
/// `reproof:<verdict>` | `download:<reason>` — outcome-blind only.
fn excluded(
    task: &str,
    verdicts: &BTreeMap<String, String>,
    downloads: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(v) = verdicts.get(task) {
        if v.starts_with("exclude:") {
            return Some(v.clone());
        }
        return None;
    }
    downloads.get(task).cloned()
}

/// `freeze_post_replacement(selection, select_commit, verdicts,
/// downloads) → freeze record` — apply W1's typed exclusions to the
/// committed within-cell orders (REQ-XH-116), with REQ-XH-127(c)'s
/// adjacent-cell dev replacement and its disclosure.
///
/// Slot states: `admitted{member}` (a re-proofed admit landed),
/// `blocked{un_reproofed_successor{member}}` (the next member was not
/// re-proofed in W1 — it needs Wc), `blocked{cell_order_exhausted}`
/// (held-out cells only — they have no declared donor),
/// `blocked{no_adjacent_donor}` (an emptied medium dev cell —
/// REQ-XH-127(c) maps easy|hard → medium only). The draw and the whole
/// freeze are `frozen` only when every slot admitted; otherwise the
/// state is `blocked` and the record carries the full walk — the
/// honest state, never a silently substituted list.
pub fn freeze_post_replacement(
    selection: &Json,
    select_commit: &Json,
    verdicts: &BTreeMap<String, String>,
    downloads: &BTreeMap<String, String>,
) -> Result<Json, PreregError> {
    let adjacent_orders: BTreeMap<String, Vec<String>> = select_commit
        .get("adjacent_cell_orders")
        .and_then(Json::as_obj)
        .ok_or_else(|| schema("select_commit.adjacent_cell_orders absent"))?
        .iter()
        .map(|(c, o)| {
            (
                c.clone(),
                o.as_arr()
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect();

    let draws = selection
        .get("draws")
        .and_then(Json::as_arr)
        .ok_or_else(|| schema("selection.draws absent"))?;
    let mut out_draws = Vec::new();
    let mut all_frozen = true;
    for d in draws {
        let name = d
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| schema("draw.name absent"))?;
        let is_dev = d.get("pool").and_then(Json::as_str) == Some("dev");
        let cells = d
            .get("draw")
            .and_then(|dd| dd.get("cells"))
            .and_then(Json::as_obj)
            .ok_or_else(|| schema(format!("draw {name} cells absent")))?;
        let mut out_cells = Vec::new();
        let mut draw_frozen = true;
        // The dev medium cell's committed order is one physical donor
        // pool: REQ-XH-127(c) adjacent draws (easy|hard → medium) and
        // the medium cell's own replacements share it — a member serves
        // exactly one slot.
        let mut donor_taken: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (cell, cj) in cells {
            let drawn: Vec<String> = cj
                .get("drawn")
                .and_then(Json::as_arr)
                .ok_or_else(|| schema(format!("draw {name} cell {cell} drawn absent")))?
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect();
            let mut order: Vec<String> = cj
                .get("replacement_order")
                .and_then(Json::as_arr)
                .ok_or_else(|| schema(format!("draw {name} cell {cell} replacement_order absent")))?
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect();
            let mut slots = Vec::new();
            let mut cell_frozen = true;
            let mut donor_note = false;
            for (i, task) in drawn.iter().enumerate() {
                let mut consumed: Vec<Json> = Vec::new();
                let mut disclosure = false;
                let state: Json;
                let mut t = task.clone();
                let first_basis = excluded(&t, verdicts, downloads);
                loop {
                    match excluded(&t, verdicts, downloads) {
                        None => {
                            state = Json::obj([
                                ("state", Json::str("admitted")),
                                ("member", Json::str(&t)),
                            ]);
                            break;
                        }
                        Some(_) => {
                            if order.is_empty() {
                                // REQ-XH-127(c): a dev easy|hard cell's
                                // emptied order falls to the medium
                                // dev cell's committed order — once,
                                // with the disclosure.
                                if is_dev && cell != "medium" && !donor_note {
                                    donor_note = true;
                                    disclosure = true;
                                    let committed: Vec<String> = adjacent_orders
                                        .get(cell.as_str())
                                        .cloned()
                                        .unwrap_or_default();
                                    order = committed
                                        .into_iter()
                                        .filter(|m| !drawn.contains(m) && !donor_taken.contains(m))
                                        .collect();
                                    continue;
                                }
                                state = Json::obj([(
                                    "state",
                                    Json::str(if is_dev && cell == "medium" {
                                        "blocked{no_adjacent_donor}"
                                    } else if is_dev {
                                        "blocked{adjacent_order_exhausted}"
                                    } else {
                                        "blocked{cell_order_exhausted}"
                                    }),
                                )]);
                                cell_frozen = false;
                                break;
                            }
                            let m = order.remove(0);
                            // Dev cells share the medium donor pool —
                            // a member already serving another slot
                            // is skipped, never double-assigned.
                            if is_dev && !donor_taken.insert(m.clone()) {
                                continue;
                            }
                            let basis = excluded(&m, verdicts, downloads);
                            consumed.push(Json::obj([
                                ("member", Json::str(&m)),
                                (
                                    "basis",
                                    Json::str(
                                        basis.clone().unwrap_or_else(|| "un_reproofed".to_string()),
                                    ),
                                ),
                            ]));
                            match basis {
                                // excluded itself — keep consuming the
                                // committed order (resolve_draw's rule).
                                Some(_) => {
                                    t = m;
                                }
                                // re-proofed but not excluded → admit
                                // (verdict "admit"); repair/machinery
                                // verdicts are never a member here.
                                None if verdicts.get(&m).is_some_and(|v| v == "admit") => {
                                    t = m;
                                }
                                // a member with no verdict either way —
                                // never silently admitted: it needs the
                                // contingency window (REQ-XH-116).
                                _ => {
                                    state = Json::obj([
                                        ("state", Json::str("blocked{un_reproofed_successor}")),
                                        ("member", Json::str(&m)),
                                        ("needs", Json::str("wc_reproof")),
                                    ]);
                                    cell_frozen = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                cell_frozen =
                    cell_frozen && state.get("state").and_then(Json::as_str) == Some("admitted");
                let mut slot = BTreeMap::new();
                slot.insert("slot".to_string(), Json::Int(i as i64));
                slot.insert("drawn".to_string(), Json::str(task));
                if let Some(b) = first_basis {
                    slot.insert("drawn_basis".to_string(), Json::str(&b));
                }
                if let Json::Obj(sm) = state {
                    slot.extend(sm);
                }
                slot.insert("consumed".to_string(), Json::Arr(consumed));
                if disclosure {
                    slot.insert(
                        "disclosure".to_string(),
                        Json::str("req_xh_127_c_adjacent_cell"),
                    );
                }
                slots.push(Json::Obj(slot));
            }
            out_cells.push(Json::obj([
                ("cell", Json::str(cell)),
                (
                    "state",
                    Json::str(if cell_frozen { "frozen" } else { "blocked" }),
                ),
                ("slots", Json::Arr(slots)),
            ]));
            draw_frozen &= cell_frozen;
        }
        all_frozen &= draw_frozen;
        out_draws.push(Json::obj([
            ("name", Json::str(name)),
            (
                "state",
                Json::str(if draw_frozen { "frozen" } else { "blocked" }),
            ),
            ("cells", Json::Arr(out_cells)),
        ]));
    }
    Ok(Json::obj([
        (
            "state",
            Json::str(if all_frozen { "frozen" } else { "blocked" }),
        ),
        (
            "rule",
            Json::str("REQ-XH-116: a task excluded by the re-proof or by Q-X55 is replaced by the next task in its difficulty cell's committed order; a successor beyond those re-proofed in W1 needs the contingency window"),
        ),
        (
            "adjacent_rule",
            Json::str("REQ-XH-127(c): a development-task exclusion that empties its difficulty cell is replaced from the adjacent cell's committed order (easy|hard → medium), re-proofed under REQ-XH-116, and disclosed; the medium dev cell has no declared donor"),
        ),
        ("draws", Json::Arr(out_draws)),
    ]))
}

// ── the packet ──────────────────────────────────────────────────────

/// The V5 declaration sub-object — one level of one registry factor,
/// Q-X56's recorded-default text verbatim with its `pending` label
/// (X-03). Canonical via `Json::to_canonical_string`; the packet
/// carries its sha256 (REQ-XH-115).
pub fn v5_declaration() -> Json {
    Json::obj([
        ("level_id", Json::str("V5")),
        ("level_name", Json::str("rsn-drop")),
        ("factor_id", Json::str("transcript_render.assistant_replay.reasoning")),
        ("parameters", Json::obj([
            ("baseline", Json::str("keep (as O)")),
            ("level", Json::str("drop")),
        ])),
        ("registry_row", Json::str("§8.1 V5 `rsn-drop` — always active after turn 1 (reasoning always present); cost ↓; dependency E2-T1")),
        ("q_x56", Json::obj([
            ("text", Json::str(Q_X56_VERBATIM)),
            ("status", Json::str("pending")),
            ("decided_at", Json::str("FP-S (pre-registration)")),
            ("alternative_recorded", Json::str("V4 `obs-2.5k`")),
        ])),
        ("q_x57", Json::obj([
            ("text", Json::str(Q_X57_VERBATIM)),
            ("status", Json::str("pending")),
            ("departures", Json::Arr(
                ["Q-X15 explore→confirm (kept for later passes)",
                 "REQ-XD-215 (baseline shared with the L2 H side)",
                 "REQ-XD-220 (concurrent with the L2 H side)"]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            )),
        ])),
        ("family", Json::str("family of one")),
        ("exploration", Json::str("none")),
        ("selection", Json::str("none")),
        ("arm", Json::str("HH-native + V5 `rsn-drop` (optional; lowest priority — included at FP-E only if the measured quote leaves room, Q-X54)")),
        ("label", Json::str(V5_LABEL)),
        ("label_req", Json::str("REQ-XH-115")),
        ("sealing", Json::str("R6.43 seals an equal level with a refusal test and records the registry hash before the first W4 V5 subject — the packet carries no registry hash (the registry postdates it)")),
    ])
}

/// The claims block — §10.1.3's four claims, each with its own
/// estimator, population, family, multiplicity and the expected
/// precision quoted before data (REQ-XH-111/119/122).
pub fn claims_block() -> Json {
    Json::Arr(vec![
        Json::obj([
            ("claim", Json::str("P2-FP")),
            ("estimand", Json::str("Δ(Pi − mini), Δ(Claude Code − mini): finite-manifest equal-task Δ, joint population")),
            ("estimator", Json::str("FM credible and task bootstrap; sharp-null sign flip under `slot_schedule_randomization` (admitted GATE-G23)")),
            ("population", Json::str("the frozen 10 held-out band-A tasks")),
            ("family", Json::str("Holm α = .05 over 2 members (an unavailable member is retained)")),
            ("multiplicity", Json::str("2 members")),
            ("expected_precision", Json::str("pipeline demonstration; confirmatory test pre-stated as near-zero power — P(m ≥ 7 non-tied) ≈ .20, P(raw p ≤ .025) at Δ ≈ 0/.09/.13/.20 ≈ .003/.018/.048/.146, median percentile-bootstrap half-width ≈ .14 (O2 recomputation; UNVERIFIED for real tasks)")),
            ("labels", Json::Arr([Json::str(POWER_LABEL)].into_iter().collect())),
        ]),
        Json::obj([
            ("claim", Json::str("P3-L1")),
            ("estimand", Json::str("L1 pass/fail on the frozen W2 corpus (gate-A partition; gate-B for one allowlist amendment)")),
            ("estimator", Json::str("deterministic — `hh-l1-conformance/1` first divergence fails")),
            ("population", Json::str("the 7 development tasks' 28 trajectories, partitioned 3/2/2 by task")),
            ("family", Json::str("gate")),
            ("multiplicity", Json::str("1")),
            ("expected_precision", Json::str("deterministic")),
            ("labels", Json::Arr(vec![])),
        ]),
        Json::obj([
            ("claim", Json::str("P3-L2")),
            ("estimand", Json::str("KL paired verdict on N = 50 simultaneously launched pairs, capability ∧ reliability IUT, ε = ±.20 (Q-X53)")),
            ("estimator", Json::str("TOST under the K_η bound, η = .025, strict verdict rule L_Δ > −ε ∧ U_Δ < ε")),
            ("population", Json::str("the 50 (task, replicate) O/H pairs in W4")),
            ("family", Json::str("selected_claims = 1")),
            ("multiplicity", Json::str("1")),
            ("expected_precision", Json::str(".90 TOST half-width ≈ .246 at q = .117; expected `inconclusive` — see margin.n50_p_equivalent_table")),
            ("labels", Json::Arr(vec![Json::str(L2_LABEL)])),
        ]),
        Json::obj([
            ("claim", Json::str("P4-FP")),
            ("estimand", Json::str("Δ(V5 − baseline), the P2-FP estimators")),
            ("estimator", Json::str("FM credible and task bootstrap; sharp-null sign flip under `slot_schedule_randomization`")),
            ("population", Json::str("the frozen 10 held-out band-A tasks")),
            ("family", Json::str("family of one")),
            ("multiplicity", Json::str("1")),
            ("expected_precision", Json::str("as P2-FP — near-zero power (the same T = 10 recomputation, UNVERIFIED for real tasks)")),
            ("labels", Json::Arr(vec![Json::str(POWER_LABEL), Json::str(V5_LABEL)])),
        ]),
    ])
}

/// The trial-reuse declaration — REQ-XH-113's three reuses, verbatim
/// scope, with the dependence sentence.
pub fn trial_reuse() -> Json {
    Json::obj([
        (
            "reuses",
            Json::Arr(
                [
                    "the W4 original mini-swe-agent trials serve as the phase-2 reference arm and the L2 O side",
                    "the W4 HH-native trials serve as the L2 H side and the phase-4 baseline",
                    "the W2 corpus serves L1 only",
                ]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            ),
        ),
        ("dependence", Json::str(DEPENDENCE)),
        (
            "no_other_reuse",
            Json::str("declared reuse is limited to the three above; a packet declaring any other reuse or a cross-claim familywise claim fails review/schema check"),
        ),
    ])
}

/// The arms table — §10.1.2's five arms with their refs and roles.
fn arms_block(shas: &[String; 3]) -> Json {
    Json::Arr(vec![
        Json::obj([
            ("arm", Json::str("mini-swe-agent 2.4.6 (original, hosted)")),
            ("surface", Json::str("HH cell + proxy (= phase-3 O, C-09)")),
            ("windows", Json::str("W2, W4")),
            ("role", Json::str("phase-2 reference; L1 RECORD corpus (W2, development tasks); L2 O side (W4)")),
            ("trials", Json::str("3 + 28 + 50")),
            ("admission_ref", Json::str(ADMISSION_RECORDS[0])),
            ("admission_sha256", Json::str(&shas[0])),
            ("priority", Json::str("protected (Q-X61 recorded default)")),
        ]),
        Json::obj([
            ("arm", Json::str("Pi")),
            ("surface", Json::str("HH cell + proxy")),
            ("windows", Json::str("W2 smoke, W4")),
            ("role", Json::str("contrast 1")),
            ("trials", Json::str("3 + ≤ 50")),
            ("admission_ref", Json::str(ADMISSION_RECORDS[1])),
            ("admission_sha256", Json::str(&shas[1])),
            ("priority", Json::str("drop order position 3 (Q-X61 recorded default, pending)")),
        ]),
        Json::obj([
            ("arm", Json::str("Claude Code (Q-X26 terms)")),
            ("surface", Json::str("HH cell + proxy (Anthropic messages)")),
            ("windows", Json::str("W2 smoke, W4")),
            ("role", Json::str("contrast 2")),
            ("trials", Json::str("3 + ≤ 50")),
            ("admission_ref", Json::str(ADMISSION_RECORDS[2])),
            ("admission_sha256", Json::str(&shas[2])),
            ("priority", Json::str("drop order position 2 (Q-X61 recorded default, pending)")),
        ]),
        Json::obj([
            ("arm", Json::str("mini-swe-agent HH-native (HIR)")),
            ("surface", Json::str("HH-native lane through the proxy")),
            ("windows", Json::str("W4")),
            ("role", Json::str("L2 H side; phase-4 baseline")),
            ("trials", Json::str("50")),
            ("admission_ref", Json::str("n/a — H is fixed by rule, not an admission record (REQ-XH-114)")),
            ("priority", Json::str("protected (Q-X61 recorded default)")),
        ]),
        Json::obj([
            ("arm", Json::str("HH-native + V5 `rsn-drop` (optional; level Q-X56 pending)")),
            ("surface", Json::str("HH-native lane")),
            ("windows", Json::str("W4")),
            ("role", Json::str("single contrast")),
            ("trials", Json::str("≤ 50")),
            ("admission_ref", Json::str("n/a — one level of the HH-native baseline (REQ-XH-115)")),
            ("priority", Json::str("lowest — drop order position 1; included at FP-E only if the quote leaves room (Q-X54, Q-X61 recorded default, pending)")),
        ]),
    ])
}

/// The formula block — projection, wall-time, slice and per-arm cap,
/// declared as formulas with values filling at FP-E (REQ-XH-117).
fn formulas_block() -> Json {
    Json::obj([
        ("cost_projection", Json::str("p̂_h = 1.2 × max(1.5 × mean_h, P90_h) over the arm's measured trials in the current scope (W2 for FP-E; W4 at each checkpoint) when at least 10 are measured, else the binding planning prior of §10.1.5; a step fits when Σ_h (remaining slots_h × p̂_h) ≤ root cap − consumed − Σ orphaned calls at bounded.hi − C × s")),
        ("wall_time", Json::str("t̂_h = 1.1 × the arm's measured mean wall time per trial (W2 for FP-E, W4 at each checkpoint) when at least 3 trials are measured, else the 1,040 s prior; the HH-native arms and V5 use original mini-swe-agent's t̂; W_p = Σ_h (remaining slots_h + ⌈0.1 × remaining slots_h⌉) × t̂_h / C + overhead + load; a step fits when W_p ≤ D − the REQ-XB-128 admission cut-off (≈ 0.65 h in W4); D ≤ D_max (the largest D keeping Σ compute ceilings ≤ $20; unauthorized Wc contributes only r × 1 h + S·r_disk·h_r)")),
        ("slice", Json::str("REQ-XC-126: the in-flight slice headroom is C × s reserved against the root only; an arm's in-flight overshoot of its cap is bounded by its in-flight slots × (s − p̂_h) and covered by the root")),
        ("per_arm_cap", Json::obj([
            ("formula", Json::str("cap_h = planned slots_h × p̂_h (integer µ$)")),
            ("admission", Json::str("an arm's slot is admitted on expected cost only when consumed_h + Σ over its in-flight slots of max(p̂_h, consumed-to-date) + p̂_h ≤ cap_h; otherwise the arm's remaining slots are withheld{budget} — no cell, never a scored refusal")),
            ("arms", Json::str("Pi, Claude Code, V5 (unprotected); the protected pair is bounded only by the root cap and the checkpoint rule (Q-X61 recorded default)")),
            ("values", Json::str("formula now — values fill at FP-E from cost fields only (REQ-XH-117)")),
        ])),
    ])
}

/// The FP-S readiness computation — `pending{human_h14_not_run}` while
/// the HUMAN row is absent; `ready{instant, trigger_instant}` when the
/// packet, the HUMAN row and the verified W1 manifest are all present.
/// `trigger_instant` = the readiness instant plus the recorded number
/// of days (Q-X62's recorded default, `days` read from the record).
/// The E2-T8 rung record is never an input. The `inputs[].record`
/// citations emit `paths`' supplied relpaths verbatim.
pub fn compute_readiness(
    paths: &PreregInputs,
    packet_recorded_at: &str,
    packet_state: &str,
    human_h14_recorded_at: Option<&str>,
    w1_manifest_verified_at: Option<&str>,
    trigger_after_days: u64,
) -> Json {
    let mut inputs = Vec::new();
    inputs.push(Json::obj([
        ("record", Json::str(&paths.packet)),
        ("precondition", Json::str("fp_prereg_packet_hashed")),
        ("present", Json::Bool(true)),
        ("recorded_at", Json::str(packet_recorded_at)),
        ("task_lists_state", Json::str(packet_state)),
    ]));
    inputs.push(Json::obj([
        ("record", Json::str(&paths.human_h14_readout)),
        ("precondition", Json::str("human_h14_ollama_hf_rows")),
        ("present", Json::Bool(human_h14_recorded_at.is_some())),
        (
            "recorded_at",
            human_h14_recorded_at.map_or(Json::Null, Json::str),
        ),
    ]));
    inputs.push(Json::obj([
        ("record", Json::str(&paths.w1_manifest)),
        ("precondition", Json::str("w1_verified_harvest_manifest")),
        ("present", Json::Bool(w1_manifest_verified_at.is_some())),
        (
            "recorded_at",
            w1_manifest_verified_at.map_or(Json::Null, Json::str),
        ),
    ]));
    let instants: Vec<&str> = [
        Some(packet_recorded_at),
        human_h14_recorded_at,
        w1_manifest_verified_at,
    ]
    .into_iter()
    .flatten()
    .collect();
    let all_present = human_h14_recorded_at.is_some() && w1_manifest_verified_at.is_some();
    let state = if all_present {
        let instant = instants.into_iter().max().unwrap_or(packet_recorded_at);
        Json::obj([
            ("state", Json::str("ready")),
            ("readiness_instant", Json::str(instant)),
            (
                "trigger_instant",
                Json::str(format!(
                    "{instant} + {trigger_after_days} days (Q-X62 recorded default, pending)"
                )),
            ),
            ("trigger_after_days", Json::Int(trigger_after_days as i64)),
        ])
    } else {
        let mut missing = Vec::new();
        if human_h14_recorded_at.is_none() {
            missing.push(Json::str("HUMAN-H14"));
        }
        if w1_manifest_verified_at.is_none() {
            missing.push(Json::str("w1_verified_harvest_manifest"));
        }
        Json::obj([
            (
                "state",
                Json::str(format!("pending{{{} not run}}", {
                    if human_h14_recorded_at.is_none() {
                        "HUMAN-H14"
                    } else {
                        "w1_manifest_verification"
                    }
                })),
            ),
            ("missing", Json::Arr(missing)),
        ])
    };
    let rec = vec![
        ("schema", Json::str(READINESS_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        (
            "purpose",
            Json::str("the instant every FP-S precondition except the E2-T8 rung record is met — the derived Q-X62 trigger instant; recomputed offline at $0 by GATE-G26, GATE-G29 and R6.46"),
        ),
        ("inputs", Json::Arr(inputs)),
        (
            "e2_t8_rung_record",
            Json::str("never an input — the rung record gates FP-S separately (Q-X62); once the trigger fires FP-S no longer waits for it"),
        ),
        (
            "q_x62_trigger",
            Json::obj([
                ("verbatim", Json::str(Q_X62_VERBATIM)),
                ("status", Json::str("pending")),
                ("trigger_after_days", Json::Int(trigger_after_days as i64)),
            ]),
        ),
        ("result", state),
        (
            "honesty",
            Json::str("committed in its pending{HUMAN-H14 not run} state — HUMAN-H14 has not run; nothing pre-dated"),
        ),
    ];
    Json::obj(rec)
}

/// The packet's citation of one committed record (path + sha256).
fn cite(path: &str, sha: &str) -> Json {
    Json::obj([("path", Json::str(path)), ("sha256", Json::str(sha))])
}

/// `build_packet(paths, ...)` — assemble `hh-fp-prereg/1`. All values
/// are declared constants, spec-verbatim text or cited records; the
/// citations emit `paths`' supplied relpaths verbatim (the reader rule
/// — no `docs/` literal lives in the crate), and the task lists are
/// the freeze record (never a silently substituted list).
pub fn build_packet(
    paths: &PreregInputs,
    freeze: Json,
    select_commit_sha: &str,
    selection_sha: &str,
    selection_id: &str,
    select_id: &str,
    exclusion_sha: &str,
    reproof_held_sha: &str,
    reproof_held_register_sha: &str,
    reproof_dev_sha: &str,
    reproof_dev_register_sha: &str,
    live_evidence_sha: &str,
    w1_manifest_sha: &str,
    freeze_digest: &str,
    admission_shas: &[String; 3],
) -> Json {
    Json::obj([
        ("schema", Json::str(PREREG_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        (
            "predates",
            Json::str("first_paid_subject — hashed before any paid first-pass subject (REQ-XH-111); checked again by R6.47's fleshing"),
        ),
        (
            "pin",
            Json::obj([
                ("git_commit", Json::str("harbor-framework/terminal-bench-2-1@12b75656")),
                ("hub_dataset_ref", Json::str("terminal-bench/terminal-bench-2-1@sha256:7d7bdc1c")),
                ("dataset_toml_digests", Json::str("pending{w1_observe_ingest} — hh-harbor-ingest/1 per-task digests bind before the post-replacement list freezes; never invented")),
            ]),
        ),
        ("split", {
            let mut c = cite(&paths.selection, selection_sha);
            if let Json::Obj(ref mut m) = c {
                m.insert("selection_id".into(), Json::str(selection_id));
                m.insert("seed".into(), Json::str("fp-select-r6.30-2026-11-08"));
                m.insert("seed_basis".into(), Json::str("declared outcome-blind at contract time — no outcome field exists or was consulted"));
            }
            c
        }),
        ("select_commit", {
            let mut c = cite(&paths.select_commit, select_commit_sha);
            if let Json::Obj(ref mut m) = c {
                m.insert("select_id".into(), Json::str(select_id));
            }
            c
        }),
        ("exclusion_record", cite(&paths.exclusion, exclusion_sha)),
        (
            "reproof",
            Json::obj([
                ("held_packet", cite(&paths.reproof_held, reproof_held_sha)),
                (
                    "held_register",
                    cite(&paths.reproof_held_register, reproof_held_register_sha),
                ),
                ("dev_packet", cite(&paths.reproof_dev, reproof_dev_sha)),
                (
                    "dev_register",
                    cite(&paths.reproof_dev_register, reproof_dev_register_sha),
                ),
                (
                    "live_evidence",
                    cite(&paths.reproof_evidence, live_evidence_sha),
                ),
                ("w1_manifest", cite(&paths.w1_manifest, w1_manifest_sha)),
                ("evidence_label", Json::str("live-executed")),
                ("honesty", Json::str("the committed register records every re-proofed task discordant `o_fail` — the O surface (Harbor oracle through the local podman environment) failed on all trials, harbor_exception digests recorded verbatim; this is executed evidence of O-surface machinery failure, not agreement data, and the freeze applies the contract's typed rules to it verbatim")),
            ]),
        ),
        (
            "allowlist_freeze",
            Json::obj([
                ("schema", Json::str("hh-egress-allowlist-freeze/1")),
                ("digest", Json::str(freeze_digest)),
                ("epoch", Json::Int(1)),
                ("protocol_id", Json::str("fp-w1-q-x55")),
                ("posture", Json::str("agent phase: model proxy + apt/PyPI/npm/crates only — no github.com (Q-X55); grading phase may reach the hosts its verifier needs")),
            ]),
        ),
        ("arms", arms_block(admission_shas)),
        ("reference_harness", Json::str("mini-swe-agent 2.4.6 (original, hosted)")),
        ("h_rule", Json::obj([
            ("rule", Json::str(H_RULE)),
            ("req", Json::str("REQ-XH-114 — H's identity is fixed by rule in the pre-registration and frozen by digest at G-L1, before any L2 subject; the HH-native arm runs only after G-L1 passes for the exact H version")),
        ])),
        ("effort", Json::obj([
            ("value", Json::str("high")),
            ("decision", Json::str("Q-X22 (answered) — enforced at the proxy, requested and applied values in every receipt (REQ-XD-082)")),
        ])),
        ("margin", margin_decl().unwrap_or(Json::Null)),
        (
            "schedule",
            Json::obj([
                ("kind", Json::str("composed")),
                ("seed", Json::str(SCHEDULE_SEED)),
                ("rounds", Json::Int(5)),
                ("k", Json::Int(5)),
                ("pair_launch", Json::str("each (task, replicate) O/H pair launched together (REQ-XD-111 kept; REQ-XH-017's two kinds composed; Q-X52)")),
                ("blocks", Json::str("the 5 rounds; a catalog snapshot at each round boundary")),
                ("window", Json::str("W4 — every arm in one seeded interleaved schedule")),
                ("recompute", Json::str("schedule::build_schedule over the engine's recorded plans — seeded_key sha256 of seed|task|replicate")),
            ]),
        ),
        ("task_lists", freeze),
        (
            "arm_priority",
            Json::obj([
                ("q_x61", Json::obj([
                    ("text", Json::str(Q_X61_VERBATIM)),
                    ("status", Json::str("pending")),
                    ("decided_at", Json::str("FP-S")),
                ])),
                ("protected", Json::Arr([Json::str("mini-swe-agent 2.4.6 (original, hosted)"), Json::str("mini-swe-agent HH-native (HIR)")].into_iter().collect())),
                ("drop_order", Json::Arr([Json::str("V5"), Json::str("Claude Code"), Json::str("Pi")].into_iter().collect())),
                ("protected_truncation", Json::str("a protected-pair truncation withholds P2-FP, P3-L2 and P4-FP primary inference with typed reasons (REQ-XH-112, REQ-XH-117)")),
                ("v5_inclusion", Json::str("V5 is included at FP-E only if the measured quote leaves room after every higher arm (Q-X54)")),
            ]),
        ),
        ("formulas", formulas_block()),
        (
            "fallback",
            Json::obj([
                ("q_x62", Json::obj([
                    ("text", Json::str(Q_X62_VERBATIM)),
                    ("status", Json::str("pending")),
                    ("recorded_at", Json::str("FP-S")),
                ])),
                ("rule", Json::str("if G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S, FP-E is held in its hosted-only form: W4 runs original mini-swe-agent, Pi and Claude Code alone (phase 2 only), and P3-L2 and P4-FP are withheld{l1_not_passed} and move to a later pass (REQ-XH-123)")),
                ("specifics", Json::Arr([
                    Json::str("FP-S is \"otherwise ready\" when every FP-S precondition except the E2-T8 rung record is met"),
                    Json::str("an E2-T8 record on C-05 rung 3 is not itself a trigger — it goes to the operator through REQ-XE-120's AR-04 amendment ADR, and if that delays G-L1, the 14-day trigger fires"),
                    Json::str("once the trigger fires, FP-S no longer waits for the rung record, which only the HH-native arms need"),
                ].into_iter().collect())),
            ]),
        ),
        ("claims", claims_block()),
        ("trial_reuse", trial_reuse()),
        (
            "stops",
            Json::obj([
                ("rules", Json::str("budget (the proxy root cap), infrastructure (PP0 ≥ 1 of 3 holds; > 20 % after ≥ 10 stops the arm; ≥ 3 stopped arms stops the pass), integrity (six events stop everything at once), calendar (the provider credit period end) — no outcome-based stopping (§6.6; REQ-XD-042)")),
                ("reattempt", Json::str("infra classes re-run within §6.8's bounds; L2 reads only the pre-assigned first attempt of each pair — zero replans (REQ-XH-016, REQ-XH-125)")),
                ("scan", Json::str("answer-scan/1 on every scored trial; a flagged pass is an observed veto pending the recorded flagged-pass review (REQ-XD-070/071)")),
            ]),
        ),
        (
            "timeouts",
            Json::str("task-declared deadlines (Q-X32; AR-07); the W4 agent-plus-verifier bound is 2,100 s"),
        ),
        (
            "labels",
            Json::Arr([
                Json::str(FP_LABEL),
                Json::str(POWER_LABEL),
                Json::str(L2_LABEL),
            ]
            .into_iter()
            .collect()),
        ),
        (
            "ar33_departures",
            Json::obj([
                ("adr", Json::str("ADR-0439 (proposed at R6.27; AR-33's departures admitted at GATE-G23, never adopted here — ids stamped verbatim)")),
                ("departures", Json::Arr([
                    Json::str("one interleaved window with coupled O/H pairs (Q-X52)"),
                    Json::str("the shared phase-4 baseline and its concurrency with L2 (REQ-XD-215, REQ-XD-220)"),
                    Json::str("the family-of-one P4 contrast"),
                    Json::str("a T = 10, H = 3 family below the §4.5 precision target"),
                    Json::str("ε = ±.20 with the scoped label (REQ-XH-122)"),
                    Json::str("the cost-only checkpoint and arm priority"),
                ].into_iter().collect())),
            ]),
        ),
        (
            "pooled_estimand",
            Json::str("none — each claim keeps its own estimator, population, family and multiplicity"),
        ),
    ])
}

/// The packet with its V5 sub-object sha embedded.
pub fn packet_with_v5(mut packet: Json) -> Json {
    let v5 = v5_declaration();
    let v5_sha = sha256_hex(v5.to_canonical_string().as_bytes());
    if let Json::Obj(ref mut m) = packet {
        m.insert("v5_declaration".to_string(), v5);
        m.insert("v5_declaration_sha256".to_string(), Json::str(&v5_sha));
    }
    packet
}

/// sha256 over the canonical packet's committed bytes (canonical +
/// trailing newline, the sibling-record convention) — the committed
/// packet hash; `shasum -a 256` on the committed file reproduces it.
pub fn packet_sha256(packet: &Json) -> String {
    sha256_hex(format!("{}\n", packet.to_canonical_string()).as_bytes())
}

// ── the W2 quote (comparative-quote/1 for GATE-G26) ─────────────────

/// §10.1.5's binding planning per-trial values (UNVERIFIED priors;
/// no-cache × 1.5 × 1.2 × 1.34), integer µ$.
pub const BINDING_MINI: i64 = 374_000;
/// Pi's binding per-trial.
pub const BINDING_PI: i64 = 123_000;
/// Claude Code's binding per-trial.
pub const BINDING_CC: i64 = 210_000;
/// PS0's declared ≤ $0.05 bound.
pub const PS0_BOUND: i64 = 50_000;
/// The W2 corpus tasks (the development list, committed order).
pub const CORPUS_TASKS: [&str; 7] = [
    "cobol-modernization",
    "torch-pipeline-parallelism",
    "polyglot-rust-c",
    "modernize-scientific-stack",
    "extract-elf",
    "openssl-selfsigned-cert",
    "sqlite-with-gcov",
];
/// The PP0 smoke tasks (one per difficulty).
pub const SMOKE_TASKS: [&str; 3] = [
    "cobol-modernization",
    "modernize-scientific-stack",
    "torch-pipeline-parallelism",
];

/// `w2_quote(compute)` — the `comparative-quote/1` GATE-G26 presents:
/// one row per W2 model block at §10.1.5's binding values, each arm
/// entry a planned slot line at the arm's per-trial binding price
/// (UNVERIFIED planning inputs — the measure_ref labels them), plus
/// the committed `hh-compute-quote/1` lines verbatim. `t = k = 1` —
/// each enumerated slot line counts once; n_trials is the real trial
/// count per block.
pub fn w2_quote(
    compute: Vec<ComputeLine>,
) -> Result<ComparativeQuote, hharness_eval::comparative_quote::ComparativeQuoteError> {
    let bind = |id: &str, micros: i64, what: &str| ArmMeasure {
        arm_id: id.to_string(),
        measure_ref: format!("experiments-spec §10.1.5 binding {what} (UNVERIFIED planning prior)"),
        mean_trial_micros: micros,
        p90_trial_micros: micros,
        model_occupied_ms: 0,
        queue_wait_ms: 0,
    };
    let mut pp0_arms = Vec::new();
    for (arm, micros) in [
        ("mini-swe-agent", BINDING_MINI),
        ("pi", BINDING_PI),
        ("claude-code", BINDING_CC),
    ] {
        for task in SMOKE_TASKS {
            pp0_arms.push(bind(
                &format!("{arm}:{task}"),
                micros,
                "$/trial at `high` for the 3-task × 1 × 3-arm PP0 smoke",
            ));
        }
    }
    let mut corpus_arms = Vec::new();
    for task in CORPUS_TASKS {
        for r in 0..4 {
            corpus_arms.push(bind(
                &format!("mini-swe-agent:{task}:r{r}"),
                BINDING_MINI,
                "$/trial at `high` for the 7-task × 4-rep L1 RECORD corpus",
            ));
        }
    }
    let decl = ComparativeQuoteDecl {
        t: 1,
        k: 1,
        concurrency_c: 4,
        rows: vec![
            MenuRowDecl {
                row_id: "w2-ps0".to_string(),
                arms: vec![bind(
                    "ps0-probe-bundle",
                    PS0_BOUND,
                    "≤ $0.05 bound — one call per dialect plus seed, `tool_choice` and `cache_control` probes",
                )],
                infra_allowance_ppm: 0,
                sim: None,
            },
            MenuRowDecl {
                row_id: "w2-pp0".to_string(),
                arms: pp0_arms,
                infra_allowance_ppm: 0,
                sim: None,
            },
            MenuRowDecl {
                row_id: "w2-corpus".to_string(),
                arms: corpus_arms,
                infra_allowance_ppm: 0,
                sim: None,
            },
        ],
        compute,
        envelope: Envelope::PreQuote,
    };
    build_quote(&decl)
}
