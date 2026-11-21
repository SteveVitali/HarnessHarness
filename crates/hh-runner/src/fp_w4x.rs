//! The executed-W4x (W4 extension) fold — `hh-fp-w4x-outcomes/1` over
//! the ADR-0453 a4 `w4x` and a5 `w4xv` windows, plus the superseding
//! `hh-comparative-results/1` packets (P2-FP: pi + claude-code landed
//! beside mini-swe-agent; P4-FP: v5 landed on its corrected-seal
//! verifier evidence), `hh-fp-l2/1` and `hh-fp-l3/1`.
//!
//! Append-only: the W4 outcomes record, its `not_run{admission:AG3}`
//! pi/cc rows and v5's `recorded{127}` history stay byte-untouched —
//! this fold supersedes them only through dated references (R6.57,
//! ADR-0453). A `recorded{rc=127}` verifier exec is machinery absence,
//! never a task verdict; a process `rc=0` is not a pass — CTRF/reward
//! evidence determines the scored outcome. Pi/claude-code provider
//! usage objects were absent — the honest `0` attributed cost stays,
//! classed `unmetered_streaming_usage`, never laundered into a number.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators::{
    comparative_estimand, finite_manifest_rate, pass_rates, rerun_flip, swap_flip, EstimandDecl,
    FlipCell, RateDecl, ALPHA_PPM as FAMILY_ALPHA_PPM, LABEL_CONFIRMED_MIN,
    LABEL_EXPLORATORY_LEADER, LABEL_INTERNAL, LABEL_MODEL_SPEND, LABEL_SECONDARY_POP,
    PRIMARY_METRIC, REQUIRED_LABELS, REQUIRED_POPULATIONS, RESULTS_SCHEMA,
};
use hharness_eval::finite_manifest::{LEVEL_PPM, REGISTERED_PRIOR, REGISTERED_PRIOR_NAME};
use hharness_eval::l2_wire::{dimension_map_record, typed_unknowns, L2_SCHEMA};
use hharness_eval::population::{
    assemble, Availability, BoundsDecl, PlannedPopulation, PlannedSlot, PopulationInput,
    PopulationKind, SlotSubject,
};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::answer_scan::ANSWER_SCAN_VERSION;
use crate::fp_results::{marker, L3_SCHEMA};
use crate::fp_w4::{
    dist, g23_admission, not_computed, obj_dyn, refused, unavailable, w4_freeze_diff_errors,
    SlotOutcome,
};
use crate::json_ext::JsonExt;

/// The W4x outcomes schema.
pub const W4X_OUTCOMES_SCHEMA: &str = "hh-fp-w4x-outcomes/1";
/// The W4x flagged-review handoff schema (same wire as the W4 leg).
pub const W4X_FLAGGED_INPUT_SCHEMA: &str = "hh-flagged-review-input/1";
/// The a4 window record.
pub const W4X_WINDOW_PATH: &str = "docs/build/reports/2026-11-20-fp-w4x-window.json";
/// The a5 corrected-verifier window record.
pub const W4XV_WINDOW_PATH: &str = "docs/build/reports/2026-11-20-fp-w4xv-window.json";
/// The extension ruling.
pub const ADR_0453_PATH: &str = "docs/adr/ADR-0453-w4x-extension-unblock-rulings.md";
/// The committed W4 outcomes record this fold supersedes members of.
pub const W4_OUTCOMES_PATH: &str = "docs/build/reports/2026-11-19-fp-w4-outcomes.json";
/// The fold's mint timestamp (recorded, not clocked at emit).
pub const W4X_RECORDED_AT: &str = "2026-11-20T16:05:00Z";
/// The review row the w4x flagged passes route to.
pub const W4X_REVIEW_INPUT_TO: &str = "HUMAN-H20";
/// Its readout.
pub const W4X_REVIEW_READOUT: &str = "docs/build/readouts/HUMAN-H20.md";
/// The committed W4x scan path.
pub const W4X_SCAN_REL: &str = "docs/build/reports/2026-11-20-fp-w4x-answer-scan.json";
/// The committed flagged-review-input path.
pub const W4X_REVIEW_INPUT_REL: &str =
    "docs/build/reports/2026-11-20-fp-w4x-flagged-review-input.json";
/// The committed review record path (minted when HUMAN-H20 lands).
pub const W4X_REVIEW_REL: &str = "docs/build/reports/2026-11-20-fp-w4x-flagged-review.json";
/// The committed W4x analysis record path (minted with the review).
pub const W4X_ANALYSIS_REL: &str = "docs/build/reports/2026-11-20-fp-w4x-flag-analysis.json";
/// The seed the W4 fold's estimator members used (kept — the same
/// declared estimator seed material spans the executed tranche).
pub const W4X_EST_SEED: u64 = 0xD0E5;
const L2_EPSILON_PPM: i64 = 200_000;
const L2_PLANNED_N: i64 = 50;

fn jget<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    j.get(k)
}
fn jstr(j: &Json, k: &str) -> Option<String> {
    jget(j, k).and_then(Json::as_str).map(str::to_string)
}
fn jint(j: &Json, k: &str) -> Option<i64> {
    jget(j, k).and_then(Json::as_int)
}

// ── the w4x outcomes record ──────────────────────────────────────────

/// Per-slot folded facts — the scored verdict plus descriptive
/// counters. Every member that cannot be folded stays `None`.
#[derive(Debug, Clone, Default)]
pub struct W4xSlotFact {
    /// The scored verdict (`None` = no scored outcome — typed member).
    pub pass: Option<bool>,
    /// The scored-evidence member (CTRF summary or reward record).
    pub scored: Option<Json>,
    /// Folded steps.
    pub steps: Option<i64>,
    /// Folded tool calls.
    pub tool_calls: Option<i64>,
    /// Folded errors.
    pub errors: Option<i64>,
}

/// One outcomes slot row — shared shape for both windows' rows. The
/// `window` member names which executed window the row folds.
fn w4x_slot_row(key: &str, window: &str, s: &Json, f: Option<&W4xSlotFact>) -> (String, Json) {
    let mut row: Vec<(String, Json)> = vec![
        ("slot_id".into(), Json::str(key)),
        ("window".into(), Json::str(window)),
        ("arm".into(), s.get("arm").cloned().unwrap_or(Json::Null)),
        ("task".into(), s.get("task").cloned().unwrap_or(Json::Null)),
        (
            "replicate".into(),
            s.get("replicate").cloned().unwrap_or(Json::Null),
        ),
        ("rc".into(), s.get("rc").cloned().unwrap_or(Json::Null)),
        (
            "cost_micros".into(),
            s.get("cost_micros").cloned().unwrap_or(Json::Null),
        ),
        (
            "wall_ms".into(),
            s.get("wall_ms").cloned().unwrap_or(Json::Null),
        ),
    ];
    if let Some(v) = s.get("verifier") {
        row.push(("verifier".into(), v.clone()));
    }
    let pass = match f.and_then(|f| f.pass) {
        Some(p) => Json::Bool(p),
        None => Json::str("unavailable{no_scored_outcome}"),
    };
    row.push(("pass".into(), pass));
    row.push((
        "scored".into(),
        f.and_then(|f| f.scored.clone()).unwrap_or(Json::Null),
    ));
    for (m, v) in [
        ("steps", f.and_then(|f| f.steps)),
        ("tool_calls", f.and_then(|f| f.tool_calls)),
        ("errors", f.and_then(|f| f.errors)),
    ] {
        row.push((m.into(), v.map(Json::Int).unwrap_or(Json::Null)));
    }
    (key.to_string(), obj_dyn(row))
}

/// `w4x_outcomes_record(a4, a5, facts, planned, sources)` — the
/// `hh-fp-w4x-outcomes/1` fold. Slots key `w4x:<sid>` (a4) and
/// `w4xv:<sid>` (a5) — v5's a4 `recorded{127}` rows and its a5 scored
/// rows coexist as separate dated evidence; the a5 window is the
/// scored manifest of record for v5 (ADR-0453's corrected-seal
/// discharge), and the record's `supersession` member says so.
#[allow(clippy::too_many_arguments)] // the arity is the record's member set.
pub fn w4x_outcomes_record(
    w4x_window: &Json,
    w4xv_window: &Json,
    facts: &BTreeMap<String, W4xSlotFact>,
    planned_per_arm: &BTreeMap<String, i64>,
    sources: Vec<(String, String)>,
) -> Json {
    let mut rows: Vec<(String, Json)> = Vec::new();
    let mut arm_counts: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for (win_tag, win) in [("w4x", w4x_window), ("w4xv", w4xv_window)] {
        let sr = jget(win, "slot_results")
            .and_then(Json::as_obj)
            .cloned()
            .unwrap_or_default();
        for (sid, s) in &sr {
            let key = format!("{win_tag}:{sid}");
            let arm = jstr(s, "arm").unwrap_or_default();
            let f = facts.get(&key);
            rows.push(w4x_slot_row(&key, win_tag, s, f));
            let c = arm_counts.entry(arm).or_default();
            *c.entry(format!("{win_tag}_settled")).or_default() += 1;
            let rc = jint(s, "rc").unwrap_or(-1);
            *c.entry(format!("{win_tag}_rc{rc}")).or_default() += 1;
            if f.and_then(|f| f.pass) == Some(true) {
                *c.entry(format!("{win_tag}_passes")).or_default() += 1;
            }
            if let Some(v) = s.get("verifier") {
                let disp = v.get("disposition").and_then(Json::as_str).unwrap_or("");
                let vrc = jint(v, "returncode").unwrap_or(-1);
                *c.entry(format!("{win_tag}_verifier:{disp}{{{vrc}}}"))
                    .or_default() += 1;
            }
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut arms: Vec<(String, Json)> = Vec::new();
    for (arm, counts) in &arm_counts {
        arms.push((
            arm.clone(),
            Json::obj([
                (
                    "planned",
                    Json::Int(*planned_per_arm.get(arm).unwrap_or(&0)),
                ),
                (
                    "counts",
                    obj_dyn(counts.iter().map(|(k, v)| (k.clone(), Json::Int(*v)))),
                ),
            ]),
        ));
    }
    let src: Vec<Json> = sources
        .iter()
        .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
        .collect();
    Json::obj([
        ("schema", Json::str(W4X_OUTCOMES_SCHEMA)),
        (
            "run_id",
            w4x_window.get("run_id").cloned().unwrap_or(Json::Null),
        ),
        ("window", Json::str("W4x")),
        (
            "seed",
            w4x_window.get("seed").cloned().unwrap_or(Json::Null),
        ),
        (
            "windows",
            Json::obj([
                (
                    "w4x",
                    Json::obj([
                        ("path", Json::str(W4X_WINDOW_PATH)),
                        ("attempt", Json::str("a4")),
                        (
                            "window",
                            w4x_window.get("window").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "slots",
                            Json::Int(
                                jget(w4x_window, "slot_results")
                                    .and_then(Json::as_obj)
                                    .map(|m| m.len() as i64)
                                    .unwrap_or(0),
                            ),
                        ),
                        (
                            "withheld",
                            w4x_window
                                .get("withheld")
                                .cloned()
                                .unwrap_or(Json::Arr(vec![])),
                        ),
                        (
                            "not_run",
                            w4x_window.get("not_run").cloned().unwrap_or(Json::Null),
                        ),
                    ]),
                ),
                (
                    "w4xv",
                    Json::obj([
                        ("path", Json::str(W4XV_WINDOW_PATH)),
                        ("attempt", Json::str("a5")),
                        (
                            "window",
                            w4xv_window.get("window").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "slots",
                            Json::Int(
                                jget(w4xv_window, "slot_results")
                                    .and_then(Json::as_obj)
                                    .map(|m| m.len() as i64)
                                    .unwrap_or(0),
                            ),
                        ),
                        (
                            "withheld",
                            w4xv_window
                                .get("withheld")
                                .cloned()
                                .unwrap_or(Json::Arr(vec![])),
                        ),
                        (
                            "not_run",
                            w4xv_window.get("not_run").cloned().unwrap_or(Json::Null),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "checkpoint",
            w4x_window.get("checkpoint").cloned().unwrap_or(Json::Null),
        ),
        (
            "drop_order",
            w4x_window
                .get("drop_order")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
        ),
        (
            "supersession",
            Json::obj([
                ("basis", Json::str(ADR_0453_PATH)),
                ("amends", Json::str(W4_OUTCOMES_PATH)),
                (
                    "note",
                    Json::str(
                        "ADR-0453's W4 extension: pi/claude-code's W4 not_run{admission:AG3} \
                         rows and v5's W4 unavailable{verifier_unavailable} + w4x a4 \
                         recorded{127} rows stay byte-untouched append-only history; the \
                         dated a4/a5 executed evidence supersedes them here — v5's scored \
                         manifest is the w4xv window (the a4 127s were verifier-machinery \
                         absence, discharged by a5's corrected-seal commit_clone verifier)",
                    ),
                ),
            ]),
        ),
        ("arms", obj_dyn(arms)),
        ("slots", obj_dyn(rows)),
        ("sources", Json::Arr(src)),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ])
}

// ── populations ──────────────────────────────────────────────────────

/// One arm's resolved view inside a multi-arm population. `rows` are
/// committed `slot_outcomes` rows; `arm` filters them; `vetoes` maps
/// slot_ref → veto axis (unreviewed flags only — cleared flags carry
/// no veto).
pub fn w4x_population_for(
    slots: &mut Vec<PlannedSlot>,
    rows: &[SlotOutcome],
    arm: &str,
    split: &str,
    vetoes: &BTreeMap<String, Vec<String>>,
) {
    for r in rows.iter().filter(|r| r.arm == arm) {
        let veto = vetoes.get(&r.slot_id).cloned().unwrap_or_default();
        let (outcome, success, availability) = match r.pass {
            Some(p) => (Some(OutcomeClass::Scored), Some(p), Availability::Resolved),
            None => (
                Some(OutcomeClass::OracleFailure),
                None,
                Availability::Unavailable,
            ),
        };
        slots.push(PlannedSlot {
            plan_ref: r.slot_id.clone(),
            task_id: r.task.clone(),
            split_label: split.into(),
            stratum: "fp-w4".into(),
            arm_id: arm.into(),
            replicate_index: r.replicate as u64,
            subjects: vec![SlotSubject {
                subject_run: r.slot_id.clone(),
                attempt_no: 1,
                outcome,
                success,
                veto_tripped: veto,
                availability,
                seq: 0,
                missing_required_checks: false,
                admissible: true,
            }],
        });
    }
}

/// The multi-arm `PlannedPopulation` the Δ contrasts read: `arms` is
/// `(arm_id, rows, split_label, vetoes)` — the one `assemble` call
/// (the one counting rule, never reimplemented).
#[allow(clippy::type_complexity)] // the arm-decl tuple is the record's member set.
pub fn w4x_joint_population(
    arms: &[(&str, &[SlotOutcome], &str, &BTreeMap<String, Vec<String>>)],
) -> Result<PlannedPopulation, hharness_eval::population::PopulationError> {
    let mut slots = Vec::new();
    for (arm, rows, split, vetoes) in arms {
        w4x_population_for(&mut slots, rows, arm, split, vetoes);
    }
    assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
}

/// Per-task raw cells `(c, n)` for one arm's scored rows.
pub fn w4x_raw_cells(rows: &[SlotOutcome], arm: &str) -> Vec<FlipCell> {
    let mut m: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.arm == arm) {
        let c = m.entry(r.task.clone()).or_insert((0, 0));
        c.1 += 1;
        if r.pass == Some(true) {
            c.0 += 1;
        }
    }
    m.into_iter()
        .map(|(task_id, (c, n))| FlipCell {
            task_id,
            c: c as u64,
            n: n as u64,
        })
        .collect()
}

/// `w4x_review_dispositions(review)` — the committed
/// `hh-flagged-review/1` record's `slot_ref → disposition` map. Only
/// the operator-recorded dispositions restore a flag — an agent never
/// self-clears.
pub fn w4x_review_dispositions(review: &Json) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(Json::Arr(rows)) = review.get("records") {
        for r in rows {
            let sid = r.get("slot_ref").and_then(Json::as_str).unwrap_or("");
            let disp = r.get("disposition").and_then(Json::as_str).unwrap_or("");
            if !sid.is_empty() {
                out.insert(sid.to_string(), disp.to_string());
            }
        }
    }
    out
}

/// `w4x_restored_vetoes(scan, review)` — the post-review veto set: the
/// committed scan's flagged slots minus the review's `cleared`
/// dispositions. A `confirmed` flag and an undispositioned flag both
/// stay vetoes — the restore is disposition-driven, never a blanket
/// clear.
pub fn w4x_restored_vetoes(scan: &Json, review: &Json) -> BTreeMap<String, Vec<String>> {
    let disp = w4x_review_dispositions(review);
    let mut out = crate::fp_w4::scan_vetoes(scan);
    out.retain(|sid, _| disp.get(sid).map(String::as_str) != Some("cleared"));
    out
}

// ── the landed arm row (parameterized over the W4x landed set) ──────

/// The `delta_vs_ref` p-member posture — `Pending` is the committed
/// pre-review generation's `pending{flagged_review}` markers verbatim;
/// `Reviewed` is the post-review restored fold: the raw sign-flip p and
/// the Holm-adjudicated value land as computed ints (a `None` keeps a
/// typed `unavailable{paired_deltas}` marker — never a synthesized p).
/// A refused Δ keeps a typed p member in both generations.
#[derive(Debug, Clone)]
pub enum W4xDeltaP<'a> {
    /// The pre-review marker note (the committed generation verbatim).
    Pending(&'a str),
    /// The post-review computed values.
    Reviewed {
        /// The raw sign-flip p (ppm) when the paired deltas resolve.
        raw_ppm: Option<i64>,
        /// The Holm-adjusted p (ppm) when adjudicated.
        holm_ppm: Option<i64>,
    },
}

/// `w4x_delta(ref_arm, test_arm, pop)` — the `delta_vs_ref` member:
/// the paired-cell Δ via `comparative_estimand` (the same machinery,
/// never reimplemented). The Holm p-members stay typed pending the
/// flagged review in the `Pending` posture — the confirmatory read is
/// never claimed over an undispositioned-flag fold; `Reviewed` lands
/// the computed members the restored fold carries.
pub fn w4x_delta(
    pop: &PlannedPopulation,
    test_arm: &str,
    reference_arm: &str,
    p: &W4xDeltaP,
) -> Json {
    let post_pair = |raw_ppm: Option<i64>, holm_ppm: Option<i64>, refused: bool| {
        if refused {
            (
                marker(
                    "unavailable{contrast_refused}",
                    "the contrast refused — no sign-flip p is claimed",
                ),
                marker(
                    "unavailable{contrast_refused}",
                    "the contrast refused — no Holm-adjudicated p is claimed",
                ),
            )
        } else {
            let pv = |v: Option<i64>, member: &str| match v {
                Some(i) => Json::Int(i),
                None => marker(
                    "unavailable{paired_deltas}",
                    &format!("the paired deltas refused — no {member} is claimed"),
                ),
            };
            (
                pv(raw_ppm, "sign-flip p"),
                pv(holm_ppm, "Holm-adjudicated p"),
            )
        }
    };
    let (p_raw, p_holm, separated, label_suffix) = match p {
        W4xDeltaP::Pending(note) => (
            marker("pending{flagged_review}", note),
            marker("pending{flagged_review}", note),
            None,
            "the Holm-adjudicated p is pending{flagged_review}".to_string(),
        ),
        W4xDeltaP::Reviewed { raw_ppm, holm_ppm } => {
            let (r, h) = post_pair(*raw_ppm, *holm_ppm, false);
            // `separated` is the family machinery's rejection flag —
            // adjusted ≤ α = .05, the same rule `rejected_at_alpha`
            // applies.
            let sep = match holm_ppm {
                Some(adj) => Json::Bool(*adj <= FAMILY_ALPHA_PPM),
                None => marker(
                    "unavailable{holm_family}",
                    "the Holm-adjudicated p did not compute — no \
                     separated flag is claimed",
                ),
            };
            (
                r,
                h,
                Some(sep),
                "the Holm-adjudicated sign-flip p lands post-review — every flag \
                 cleared by the committed hh-flagged-review/1"
                    .to_string(),
            )
        }
    };
    let est = comparative_estimand(
        pop,
        &EstimandDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            arms: vec![reference_arm.to_string(), test_arm.to_string()],
            reference_arm: reference_arm.to_string(),
            stratum: None,
            use_as: "first_pass".into(),
            admission: Some(g23_admission()),
        },
    );
    match est {
        Ok(rep) => {
            let c = rep.contrasts.iter().find(|c| c.test_arm == test_arm);
            match c {
                Some(c) => {
                    let refused_delta = c.delta_ppm.is_none();
                    let ppm = match c.delta_ppm {
                        Some(d) => Json::Int(d),
                        None => refused(
                            &c.refusal
                                .clone()
                                .unwrap_or_else(|| "unavailable{contrast}".to_string()),
                        ),
                    };
                    let interval = match c.delta_ppm {
                        Some(_) => Json::obj([
                            ("lo_ppm", Json::Int(c.bounds.lo)),
                            ("hi_ppm", Json::Int(c.bounds.hi)),
                            ("kind", Json::str("paired_cell_all_planned_bound")),
                        ]),
                        None => refused(&c.refusal.clone().unwrap_or_default()),
                    };
                    // a refused Δ keeps its typed p members — the
                    // post-review generation never lands ints over a
                    // refusal.
                    let (p_raw, p_holm, separated, label_suffix) = if refused_delta {
                        match p {
                            W4xDeltaP::Pending(note) => (
                                marker("pending{flagged_review}", note),
                                marker("pending{flagged_review}", note),
                                None,
                                label_suffix,
                            ),
                            W4xDeltaP::Reviewed { .. } => {
                                let (r, h) = post_pair(None, None, true);
                                (
                                    r,
                                    h,
                                    Some(marker(
                                        "unavailable{contrast_refused}",
                                        "the contrast refused — no separated \
                                         flag is claimed",
                                    )),
                                    "the contrast refused — the p members stay typed".to_string(),
                                )
                            }
                        }
                    } else {
                        (p_raw, p_holm, separated, label_suffix)
                    };
                    let mut o = vec![
                        ("ppm", ppm),
                        ("interval", interval),
                        ("p_raw_ppm", p_raw),
                        ("p_holm_ppm", p_holm),
                    ];
                    if let Some(sep) = separated {
                        o.push(("separated", sep));
                    }
                    o.push((
                        "label",
                        Json::str(format!(
                            "paired-cell Δ({test_arm} − {reference_arm}) over the joint \
                             population — the interval is the all-planned bound; \
                             {label_suffix} — {LABEL_INTERNAL}"
                        )),
                    ));
                    if let Some(cond) = &c.conditional {
                        o.push((
                            "conditional",
                            Json::obj([
                                ("ppm", Json::Int(cond.value_ppm)),
                                (
                                    "tasks",
                                    Json::Arr(cond.tasks.iter().map(Json::str).collect()),
                                ),
                                (
                                    "excluded",
                                    Json::Arr(cond.excluded_tasks.iter().map(Json::str).collect()),
                                ),
                                ("label", Json::str(&cond.label)),
                            ]),
                        ));
                    }
                    Json::obj(o)
                }
                None => marker(
                    "unavailable{contrast_absent}",
                    "the estimand emitted no row",
                ),
            }
        }
        Err(e) => refused(&format!("{e:?}")),
    }
}

/// `w4x_swaps(arm, refs, cells_of)` — the `flips.swap_vs` +
/// `harness_excess` members via `swap_flip`; a ref with no scored
/// cells keeps the typed refusal verbatim.
pub fn w4x_swap_pair(
    arm: &str,
    cells: &[FlipCell],
    ref_arm: &str,
    ref_cells: &[FlipCell],
) -> (Json, Json) {
    if ref_cells.is_empty() {
        let reason = "unavailable{no_scored_outcome}";
        return (
            Json::obj([
                ("interval", refused(reason)),
                ("label", Json::str("deterministic resample pair-flip")),
            ]),
            Json::obj([
                ("ppm", refused(reason)),
                ("interval", refused(reason)),
                ("label", Json::str("harness excess vs the resampled pair")),
            ]),
        );
    }
    match swap_flip(
        arm,
        cells,
        ref_arm,
        ref_cells,
        "descriptive",
        None,
        4_000,
        W4X_EST_SEED,
    ) {
        Ok(rep) => {
            let iv = |i: Option<hharness_eval::stats::Interval>| match i {
                Some(i) => Json::obj([
                    ("lo_ppm", Json::Int(i.lo)),
                    ("hi_ppm", Json::Int(i.hi)),
                    ("kind", Json::str("task_bootstrap")),
                ]),
                None => not_computed(
                    "the descriptive task bootstrap is undefined over a degenerate \
                     per-task set",
                ),
            };
            (
                Json::obj([
                    ("ppm", Json::Int(rep.swap_ppm)),
                    ("interval", iv(rep.swap_interval)),
                    ("label", Json::str("deterministic resample pair-flip")),
                ]),
                Json::obj([
                    ("ppm", Json::Int(rep.excess_ppm)),
                    ("interval", iv(rep.excess_interval)),
                    ("label", Json::str("harness excess vs the resampled pair")),
                ]),
            )
        }
        Err(e) => {
            let reason = format!("{e:?}");
            (
                Json::obj([
                    ("interval", refused(&reason)),
                    ("label", Json::str("deterministic resample pair-flip")),
                ]),
                Json::obj([
                    ("ppm", refused(&reason)),
                    ("interval", refused(&reason)),
                    ("label", Json::str("harness excess vs the resampled pair")),
                ]),
            )
        }
    }
}

/// `w4x_arm_rate(pop, arm)` — the finite-manifest rate call every
/// landed row and the post-review leader argmax share (the one
/// estimator call, never a second decl).
pub fn w4x_arm_rate(
    pop: &PlannedPopulation,
    arm: &str,
) -> Result<hharness_eval::comparative_estimators::FiniteManifestRate, Vec<String>> {
    finite_manifest_rate(
        pop,
        &RateDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            arm: arm.to_string(),
            stratum: None,
            prior: REGISTERED_PRIOR,
            prior_name: REGISTERED_PRIOR_NAME.to_string(),
            bounds: BoundsDecl::Unit,
            level_ppm: LEVEL_PPM,
            seed: W4X_EST_SEED,
            use_as: "first_pass".into(),
            admission: Some(g23_admission()),
        },
    )
    .map_err(|e| vec![format!("finite_manifest_rate({arm}): {e:?}")])
}

/// The landed row's review posture — `Pending` is the committed
/// pre-review generation (the vetoed fold, the pending leader and the
/// pending integrity note); `Reviewed` is the post-review restored
/// generation — cleared flags score as passes, the leader decided
/// (argmax restored θ), the integrity note landing the committed
/// `hh-flagged-review/1` record.
#[derive(Debug, Clone)]
pub enum W4xReview<'a> {
    /// The pre-review generation — the note routes the review.
    Pending(&'a str),
    /// The post-review generation — `participant` is the decided
    /// leader's id, `member` marks this row's arm as the leader,
    /// `note` extends the exploratory label.
    Reviewed {
        /// The decided leader's participant id (argmax restored θ).
        participant: &'a str,
        /// `true` when this row's arm is the leader.
        member: bool,
        /// The trailing label note.
        note: &'a str,
    },
}

/// The landed W4x arm row — real cells, real (honest-zero) costs, the
/// joint population's veto-pending passes scored as failures. Unlike
/// the W4 landed row (mini only, Δ degenerate), this row computes the
/// Δ vs the reference and the swap diagnostics where the comparator
/// landed — the pending{{flagged_review}} p-members stay typed.
#[allow(clippy::too_many_arguments)] // the arity is the record's member set.
pub fn w4x_landed_arm_row(
    arm: &str,
    participant: &str,
    intended_participant: &str,
    pop: &PlannedPopulation,
    raw_cells: &[FlipCell],
    cost_micros: &[i64],
    cost_basis: &str,
    flagged: i64,
    confirmed: i64,
    cleared: i64,
    coverage_ppm: i64,
    delta: Json,
    swaps: &[(String, Json, Json)],
    review: &W4xReview,
) -> Result<Json, Vec<String>> {
    let post = matches!(review, W4xReview::Reviewed { .. });
    let rate = w4x_arm_rate(pop, arm)?;
    let (pass_all, pass_any) = pass_rates(raw_cells, 5, "descriptive", None, 4_000, W4X_EST_SEED)
        .map_err(|e| vec![format!("pass_rates({arm}): {e:?}")])?;
    let rerun = rerun_flip(raw_cells, "descriptive", None, 4_000, W4X_EST_SEED)
        .map_err(|e| vec![format!("rerun_flip({arm}): {e:?}")])?;
    let raw_ppm = {
        let s: i64 = raw_cells.iter().map(|c| c.c as i64).sum();
        let n: i64 = raw_cells.iter().map(|c| c.n as i64).sum();
        if n > 0 {
            s * 1_000_000 / n
        } else {
            0
        }
    };
    let total_cost: i64 = cost_micros.iter().sum();
    let solves: i64 = raw_cells.iter().map(|c| c.c as i64).sum();
    let mean_cost = if cost_micros.is_empty() {
        0
    } else {
        total_cost / cost_micros.len() as i64
    };
    let per_solve = if solves > 0 {
        Json::obj([
            ("micros", Json::Int(total_cost / solves)),
            (
                "interval",
                not_computed(
                    "the executed fold declares no interval route over per-solve \
                     cost — the point is total spend over solved trials",
                ),
            ),
            ("basis", Json::str("w4x_total_over_solves")),
        ])
    } else {
        marker("estimator_undefined", "0 solved — no per-solve numerator")
    };
    let mut flips_swap: Vec<(String, Json)> = Vec::new();
    let mut flips_excess: Vec<(String, Json)> = Vec::new();
    for (r, sw, ex) in swaps {
        flips_swap.push((r.clone(), sw.clone()));
        flips_excess.push((r.clone(), ex.clone()));
    }
    let fm_interval = Json::obj([
        ("lo_ppm", Json::Int(rate.interval.lo)),
        ("hi_ppm", Json::Int(rate.interval.hi)),
        ("kind", Json::str("fm")),
    ]);
    Ok(Json::obj([
        ("disposition", Json::str("executed")),
        (
            "theta",
            Json::obj([
                ("ppm", Json::Int(rate.observed_ppm)),
                ("lo_ppm", Json::Int(rate.interval.lo)),
                ("hi_ppm", Json::Int(rate.interval.hi)),
                ("interval", fm_interval.clone()),
                (
                    "bounds",
                    Json::obj([
                        ("lo_ppm", Json::Int(rate.bounds.lo)),
                        ("hi_ppm", Json::Int(rate.bounds.hi)),
                        ("kind", Json::str("all_planned")),
                    ]),
                ),
                ("seed_material", Json::str(&rate.seed_material)),
                (
                    "label",
                    Json::str(if post {
                        "finite-manifest rate over the 10-task tranche — joint \
                         population (cleared flags restored to passes by the \
                         committed hh-flagged-review/1 — HUMAN-H20's dispositions) \
                         — internal record — not outside-verified"
                            .to_string()
                    } else {
                        "finite-manifest rate over the 10-task tranche — joint \
                         population (veto-pending passes score as failures until \
                         review clears) — internal record — not outside-verified"
                            .to_string()
                    }),
                ),
            ]),
        ),
        (
            "secondary_populations",
            Json::obj([
                (
                    "raw",
                    Json::obj([
                        ("rate_ppm", Json::Int(raw_ppm)),
                        ("label", Json::str(LABEL_SECONDARY_POP)),
                    ]),
                ),
                (
                    "errors_zero",
                    Json::obj([
                        ("bound_lo_ppm", Json::Int(raw_ppm)),
                        ("bound_hi_ppm", Json::Int(raw_ppm)),
                        (
                            "label",
                            Json::str(format!(
                                "{LABEL_SECONDARY_POP} — errors=0 all-planned bound \
                                 (all 50 slots resolved — the bound is a point)"
                            )),
                        ),
                    ]),
                ),
            ]),
        ),
        ("delta_vs_ref", delta),
        (
            "leader_group",
            match review {
                W4xReview::Pending(note) => Json::obj([
                    ("member", Json::Bool(false)),
                    (
                        "leader_id",
                        Json::str("unavailable{pending_flagged_review}"),
                    ),
                    ("diff_interval", fm_interval),
                    (
                        "label",
                        Json::str(format!("{LABEL_EXPLORATORY_LEADER} — {note}")),
                    ),
                ]),
                W4xReview::Reviewed {
                    participant,
                    member,
                    note,
                } => Json::obj([
                    ("member", Json::Bool(*member)),
                    ("leader_id", Json::str(*participant)),
                    ("diff_interval", fm_interval),
                    (
                        "label",
                        Json::str(format!("{LABEL_EXPLORATORY_LEADER} — {note}")),
                    ),
                ]),
            },
        ),
        (
            "pass_at",
            Json::obj([
                ("k", Json::Int(5)),
                ("all_ppm", Json::Int(pass_all.point)),
                ("any_ppm", Json::Int(pass_any.point)),
                (
                    "label",
                    Json::str("pass^k / pass@k over the tranche's k=5 repeats"),
                ),
            ]),
        ),
        (
            "flips",
            Json::obj([
                (
                    "rerun",
                    Json::obj([
                        ("ppm", Json::Int(rerun.point)),
                        (
                            "interval",
                            match rerun.interval {
                                Some(iv) => Json::obj([
                                    ("lo_ppm", Json::Int(iv.lo)),
                                    ("hi_ppm", Json::Int(iv.hi)),
                                    ("kind", Json::str("task_bootstrap")),
                                ]),
                                None => not_computed(
                                    "the descriptive task bootstrap is undefined \
                                     over a degenerate per-task set",
                                ),
                            },
                        ),
                    ]),
                ),
                ("swap_vs", obj_dyn(flips_swap)),
                ("harness_excess", obj_dyn(flips_excess)),
                (
                    "label",
                    Json::str("descriptive flip diagnostics — ignores task clustering"),
                ),
            ]),
        ),
        (
            "cost",
            Json::obj([
                (
                    "per_trial",
                    Json::obj([
                        ("micros", Json::Int(mean_cost)),
                        (
                            "interval",
                            not_computed(
                                "the executed fold declares no interval route over \
                                 the slot costs — the point is the observed mean \
                                 over 50 settled slots",
                            ),
                        ),
                        ("basis", Json::str(cost_basis)),
                        (
                            "label",
                            Json::str(format!("descriptive; {LABEL_MODEL_SPEND}")),
                        ),
                    ]),
                ),
                ("per_solve", per_solve),
                ("provenance", Json::str("model_spend_only")),
            ]),
        ),
        (
            "counts",
            Json::obj([
                ("analysis_basis", Json::str("assignment")),
                (
                    "intended",
                    obj_dyn([(intended_participant.to_string(), Json::Int(50))]),
                ),
                (
                    "served",
                    obj_dyn([(participant.to_string(), Json::Int(50))]),
                ),
            ]),
        ),
        (
            "completion",
            Json::obj([
                ("planned", Json::Int(50)),
                ("first", Json::Int(50)),
                ("recovered", Json::Int(0)),
                ("unresolved", Json::Int(0)),
                ("withheld", Json::Int(0)),
            ]),
        ),
        (
            "integrity",
            Json::obj([
                ("flagged", Json::Int(flagged)),
                ("confirmed", Json::Int(confirmed)),
                ("cleared", Json::Int(cleared)),
                ("coverage_ppm", Json::Int(coverage_ppm)),
                ("detector", Json::str(ANSWER_SCAN_VERSION)),
                (
                    "label",
                    Json::str(if post {
                        format!(
                            "integrity panel — {LABEL_CONFIRMED_MIN}; cleared flags \
                             restore to passes — the committed hh-flagged-review/1 \
                             records {W4X_REVIEW_INPUT_TO}'s dispositions"
                        )
                    } else {
                        format!(
                            "integrity panel — {LABEL_CONFIRMED_MIN}; unreviewed flags \
                             stay vetoes — {W4X_REVIEW_INPUT_TO} routes them"
                        )
                    }),
                ),
            ]),
        ),
    ]))
}

// ── the superseding packets ──────────────────────────────────────────

/// `w4x_results_packet(...)` — the superseding
/// `hh-comparative-results/1` for the w4x fold — same member set as
/// the W4 packet plus the dated supersession under ADR-0453.
#[allow(clippy::too_many_arguments)]
pub fn w4x_results_packet(
    claim: &str,
    reference: &str,
    arms: Vec<(String, Json)>,
    family_members: Vec<Json>,
    expl_members: Vec<Json>,
    packet_disposition: &str,
    executed_evidence: &str,
    expected_precision: &str,
    supersedes_rel: &str,
    w4x_sha: &str,
    w4xv_sha: &str,
    sources: Json,
) -> Json {
    let labels: Vec<(String, Json)> = REQUIRED_LABELS
        .iter()
        .map(|(k, subs)| {
            (
                k.to_string(),
                Json::str(subs.join("; ") + " — " + crate::fp_results::FP_CLAIM_LABEL),
            )
        })
        .collect();
    Json::obj([
        ("schema", Json::str(RESULTS_SCHEMA)),
        ("packet_disposition", Json::str(packet_disposition)),
        ("claim", Json::str(claim)),
        (
            "tranche",
            Json::obj([
                ("id", Json::str("fp-w4x")),
                ("state", Json::str("executed{adr-0453}")),
                ("tasks", Json::Int(10)),
                ("k", Json::Int(5)),
            ]),
        ),
        (
            "family",
            Json::obj([
                ("id", Json::str(claim)),
                ("method", Json::str("holm")),
                ("alpha_ppm", Json::Int(FAMILY_ALPHA_PPM)),
                ("reference", Json::str(reference)),
                ("members", Json::Arr(family_members)),
            ]),
        ),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str(format!("{claim}-expl"))),
                ("method", Json::str("benjamini_hochberg")),
                ("members", Json::Arr(expl_members)),
            ]),
        ),
        ("metric", Json::str(PRIMARY_METRIC)),
        (
            "populations",
            Json::Arr(REQUIRED_POPULATIONS.iter().map(|s| Json::str(*s)).collect()),
        ),
        ("labels", obj_dyn(labels)),
        ("harnesses", obj_dyn(arms)),
        ("executed_evidence", Json::str(executed_evidence)),
        ("expected_precision", Json::str(expected_precision)),
        (
            "supersedes",
            Json::str(format!("docs/build/reports/{supersedes_rel}")),
        ),
        (
            "supersession",
            Json::obj([
                ("basis", Json::str(ADR_0453_PATH)),
                (
                    "note",
                    Json::str(
                        "ADR-0453's W4 extension executed the previously gated arms — the \
                         a4/a5 windows' evidence refolds this claim; the W4-era packet and \
                         every not_run/recorded{127} row stay append-only history, never \
                         rewritten",
                    ),
                ),
                (
                    "w4x_window",
                    Json::obj([
                        ("path", Json::str(W4X_WINDOW_PATH)),
                        ("sha256", Json::str(w4x_sha)),
                    ]),
                ),
                (
                    "w4xv_window",
                    Json::obj([
                        ("path", Json::str(W4XV_WINDOW_PATH)),
                        ("sha256", Json::str(w4xv_sha)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ])
}

/// `w4x_l2_packet(...)` — the `hh-fp-l2/1` superseding fold: the H
/// side still produced no scored outcome — the w4x extension changes
/// the O-side context, not the H-side absence, so admission stays
/// `unavailable{h_side_verdicts_absent}` verbatim.
#[allow(clippy::too_many_arguments)]
pub fn w4x_l2_packet(
    gl1_pass_id: &str,
    h_identity_digest: &str,
    corpus_digest: &str,
    expected_precision: &str,
    supersedes_rel: &str,
    w4x_sha: &str,
    w4xv_sha: &str,
    o_scored: i64,
    h_scored: i64,
    sources: Json,
) -> Json {
    let verdict = "unavailable{h_side_verdicts_absent}";
    let base = hharness_eval::l2_wire::l2_packet(Some(gl1_pass_id), h_identity_digest)
        .unwrap_or_else(|e| panic!("l2_packet skeleton: {e:?}"));
    Json::obj([
        ("schema", Json::str(L2_SCHEMA)),
        ("packet_disposition", Json::str(verdict)),
        ("claim", Json::str(hharness_eval::l2_wire::CLAIM_ID)),
        (
            "gl1",
            Json::obj([
                ("pass_id", Json::str(gl1_pass_id)),
                ("h_identity_digest", Json::str(h_identity_digest)),
                ("corpus_digest", Json::str(corpus_digest)),
                ("result", Json::str("pass")),
            ]),
        ),
        ("l2_packet", base),
        (
            "admission",
            Json::obj([
                ("state", Json::str("unavailable")),
                ("reason", Json::str(verdict)),
                ("kind", Json::str("parity_admission")),
                ("version", Json::str("parity-admission/1")),
                (
                    "expected",
                    Json::str(
                        "the 50 pairs launched coupled under ADR-0452; the H side \
                         produced no scored outcome (all 50 verifier execs recorded \
                         failures — 49×test.sh absent, 1×container improper). ADR-0453's \
                         w4x extension landed the previously gated arms but changed no \
                         H-side evidence — the parity-admission fold still has zero \
                         resolved pair members",
                    ),
                ),
            ]),
        ),
        (
            "kl_report",
            Json::obj([
                ("state", Json::str("unavailable")),
                ("reason", Json::str(verdict)),
                ("epsilon_ppm", Json::Int(L2_EPSILON_PPM)),
                ("planned_n", Json::Int(L2_PLANNED_N)),
                ("launched_n", Json::Int(50)),
                ("o_scored_n", Json::Int(o_scored)),
                ("h_scored_n", Json::Int(h_scored)),
                ("served_n", Json::Int(0)),
                ("rule", Json::str("L_Δ > −ε ∧ U_Δ < ε")),
                (
                    "expected_precision",
                    Json::str(
                        "the preregistration's pre-stated read at N=50, ε=±.20 is \
                         `inconclusive` — an admissible result; with zero scored H-side \
                         outcomes the inference itself is unavailable, never synthesized",
                    ),
                ),
            ]),
        ),
        (
            "conjunction",
            Json::obj([
                ("l1_gate_pass", Json::Bool(true)),
                ("admission", unavailable(verdict)),
                ("iut_conjunction_equivalent", unavailable(verdict)),
                (
                    "vetoes_empty",
                    not_computed("no scored pair — the veto layer is uninformative"),
                ),
            ]),
        ),
        (
            "veto_layer",
            Json::obj([
                ("tripped", Json::Arr(Vec::new())),
                (
                    "not_checked",
                    Json::Arr(vec![Json::obj([
                        ("veto_id", Json::str("l2_pairs")),
                        ("status", Json::str("not_checked")),
                        (
                            "reason",
                            Json::str(
                                "no scored H-side outcome — the veto layer over zero \
                                 resolved pairs is uninformative, not clean",
                            ),
                        ),
                    ])]),
                ),
            ]),
        ),
        ("verdict", Json::str(verdict)),
        ("epsilon_ppm", Json::Int(L2_EPSILON_PPM)),
        ("planned_n", Json::Int(L2_PLANNED_N)),
        ("served_n", Json::Int(0)),
        ("typed_unknowns", typed_unknowns()),
        ("dimension_map", dimension_map_record(L2_EPSILON_PPM)),
        (
            "label",
            Json::str(format!(
                "first-pass L2 packet — {verdict}; {}",
                "the first-pass read stays admissible-inconclusive"
            )),
        ),
        ("expected_precision", Json::str(expected_precision)),
        (
            "supersedes",
            Json::str(format!("docs/build/reports/{supersedes_rel}")),
        ),
        (
            "supersession",
            Json::obj([
                ("basis", Json::str(ADR_0453_PATH)),
                (
                    "note",
                    Json::str(
                        "the w4x extension's evidence refolds this claim's context; the \
                         H-side absence is unchanged W4 evidence — the frozen packet stays \
                         append-only history",
                    ),
                ),
                (
                    "w4x_window",
                    Json::obj([
                        ("path", Json::str(W4X_WINDOW_PATH)),
                        ("sha256", Json::str(w4x_sha)),
                    ]),
                ),
                (
                    "w4xv_window",
                    Json::obj([
                        ("path", Json::str(W4XV_WINDOW_PATH)),
                        ("sha256", Json::str(w4xv_sha)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ])
}

/// `w4x_l3_report(...)` — the `hh-fp-l3/1` superseding descriptive
/// report over the W4+W4x arm set. Same REQ-XD-120 contract: every
/// discordant pair listed, the H-side verdict absence typed.
pub fn w4x_l3_report(
    arm_rows: &[(String, Vec<SlotOutcome>)],
    supersedes_rel: &str,
    w4x_sha: &str,
    w4xv_sha: &str,
    sources: Json,
) -> Json {
    let arm_dist = |rows: &[SlotOutcome]| {
        let dist = |xs: &[i64]| dist(xs);
        Json::obj([
            ("served", Json::Int(rows.len() as i64)),
            (
                "steps",
                dist(&rows.iter().filter_map(|r| r.steps).collect::<Vec<_>>()),
            ),
            (
                "tool_calls",
                dist(&rows.iter().filter_map(|r| r.tool_calls).collect::<Vec<_>>()),
            ),
            (
                "errors",
                dist(&rows.iter().filter_map(|r| r.errors).collect::<Vec<_>>()),
            ),
            (
                "wall_ms",
                dist(&rows.iter().map(|r| r.wall_ms).collect::<Vec<_>>()),
            ),
            (
                "scored",
                Json::Int(rows.iter().filter(|r| r.pass.is_some()).count() as i64),
            ),
            (
                "passes",
                Json::Int(rows.iter().filter(|r| r.pass == Some(true)).count() as i64),
            ),
            (
                "label",
                Json::str("descriptive — per-arm distribution over settled slots"),
            ),
        ])
    };
    let verdict = "unavailable{h_side_verdicts_absent}";
    let per_arm: Vec<(String, Json)> = arm_rows
        .iter()
        .map(|(a, rs)| (a.clone(), arm_dist(rs)))
        .collect();
    Json::obj([
        ("schema", Json::str(L3_SCHEMA)),
        ("of", Json::str("P3-L2")),
        ("req", Json::str("REQ-XD-120")),
        (
            "population",
            Json::str(
                "the W4+W4x arm set — 250 settled slots across five arms; the L2 \
                 pair's H side stays unscored",
            ),
        ),
        ("planned_pairs", Json::Int(L2_PLANNED_N)),
        ("served_pairs", Json::Int(0)),
        ("per_arm", obj_dyn(per_arm)),
        (
            "discordant_pairs",
            Json::obj([
                ("listed", Json::Arr(Vec::new())),
                ("count", Json::Int(0)),
                ("evaluation", unavailable(verdict)),
                (
                    "note",
                    Json::str(
                        "every discordant pair is listed under REQ-XD-120 — outcome \
                         discordance needs both sides' verdicts and the H side has \
                         none, so the evaluation is unavailable, never summarized \
                         away",
                    ),
                ),
            ]),
        ),
        ("verdict", Json::str(verdict)),
        (
            "label",
            Json::str(
                "L3 descriptive report — distributions and discordance only; the \
                 schema carries no equivalence wording and none is asserted",
            ),
        ),
        (
            "supersedes",
            Json::str(format!("docs/build/reports/{supersedes_rel}")),
        ),
        (
            "supersession",
            Json::obj([
                ("basis", Json::str(ADR_0453_PATH)),
                (
                    "w4x_window",
                    Json::obj([
                        ("path", Json::str(W4X_WINDOW_PATH)),
                        ("sha256", Json::str(w4x_sha)),
                    ]),
                ),
                (
                    "w4xv_window",
                    Json::obj([
                        ("path", Json::str(W4XV_WINDOW_PATH)),
                        ("sha256", Json::str(w4xv_sha)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ])
}

// ── the W4x scan report + flagged-review input ───────────────────────

/// `w4x_scan_report(slots, not_scored, sources)` — the
/// `hh-fp-answer-scan-report/1` over the executed w4x/w4xv scored
/// trials (pi, claude-code, v5 — the verdict-bearing slots). Unscored
/// slots land on `not_scored` with their recorded reason.
pub fn w4x_scan_report(
    slots: &[crate::fp_results::SlotScan],
    not_scored: Vec<Json>,
    sources: Vec<(String, String)>,
) -> Result<Json, Vec<String>> {
    let base = crate::fp_results::scan_report(slots, sources)
        .map_err(|e| e.iter().map(|x| x.to_string()).collect::<Vec<_>>())?;
    let mut m = match base {
        Json::Obj(m) => m,
        _ => return Err(vec!["scan_report: not an object".into()]),
    };
    m.insert(
        "review_handoff".to_string(),
        Json::obj([
            ("schema", Json::str(W4X_FLAGGED_INPUT_SCHEMA)),
            ("input_to", Json::str(W4X_REVIEW_INPUT_TO)),
            ("readout", Json::str(W4X_REVIEW_READOUT)),
            ("disposition", Json::str("pending{human}")),
            (
                "note",
                Json::str(
                    "the review rows are HUMAN-H20's act — this handoff carries \
                     the flagged-pass list; until review lands, confirmed stays 0 \
                     and every flagged pass is veto-pending, never cleared; an \
                     agent never self-reviews",
                ),
            ),
        ]),
    );
    let cov_j = m.get("coverage").cloned().unwrap_or_else(|| Json::obj([]));
    if let Json::Obj(mut cov) = cov_j {
        cov.insert("not_scored".to_string(), Json::Arr(not_scored));
        m.insert("coverage".to_string(), Json::Obj(cov));
    }
    m.insert("ticket".to_string(), Json::str("R6.57"));
    Ok(Json::Obj(m))
}

/// `w4x_flagged_review_input(scan)` — the standalone
/// `hh-flagged-review-input/1` handoff routed to HUMAN-H20.
pub fn w4x_flagged_review_input(scan: &Json, sources: Vec<(String, String)>) -> Json {
    let rows = scan
        .get("flagged_pass_list")
        .and_then(|f| f.get("rows"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    let cov = scan.get("coverage").cloned().unwrap_or(Json::obj([]));
    let src: Vec<Json> = sources
        .iter()
        .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
        .collect();
    Json::obj([
        ("schema", Json::str(W4X_FLAGGED_INPUT_SCHEMA)),
        ("input_to", Json::str(W4X_REVIEW_INPUT_TO)),
        ("readout", Json::str(W4X_REVIEW_READOUT)),
        ("disposition", Json::str("pending{human}")),
        ("scan_ref", Json::str(W4X_SCAN_REL)),
        ("coverage", cov),
        ("flagged_pass_list", rows),
        (
            "note",
            Json::str(
                "each flagged pass is an observed veto until a recorded operator \
                 disposition lands — an agent never reviews, clears or confirms; \
                 the §10.3 'zero flags recorded trivially' path applies only when \
                 this list is empty",
            ),
        ),
        ("sources", Json::Arr(src)),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ])
}

// ── the W4x freeze ───────────────────────────────────────────────────

/// `w4x_freeze_packet(packet, scan, scan_sha, input_sha)` — the
/// trivial-flag case of the w4x freeze (§10.3's "zero flags recorded
/// trivially"). Any unreviewed flag refuses — `Err` carries the typed
/// reason (the `withheld{flagged_review}` state the run record
/// reports).
pub fn w4x_freeze_packet(
    packet: &Json,
    scan: &Json,
    scan_sha256: &str,
    input_sha256: &str,
) -> Result<Json, String> {
    let cov = scan.get("coverage").cloned().unwrap_or(Json::obj([]));
    let flagged = cov.get("flagged").and_then(Json::as_int).unwrap_or(-1);
    if flagged != 0 {
        return Err(format!(
            "withheld{{flagged_review}} — unreviewed flags: coverage.flagged \
             {flagged} != 0; the flagged review ({W4X_REVIEW_INPUT_TO}) is \
             pending{{human}}; every unreviewed flag stays a veto and the \
             freeze refuses"
        ));
    }
    let mut m: BTreeMap<String, Json> = match packet {
        Json::Obj(m) => m.clone(),
        _ => return Err("packet is not an object".into()),
    };
    let review = Json::obj([
        ("flagged", Json::Int(0)),
        ("confirmed", Json::Int(0)),
        ("cleared", Json::Int(0)),
        ("reviewed_flags", Json::Int(0)),
        ("detector", Json::str(ANSWER_SCAN_VERSION)),
        ("coverage_ppm", Json::Int(1_000_000)),
        ("label", Json::str(crate::fp_results::REVIEW_LABEL)),
        (
            "disposition",
            Json::str("zero flags recorded trivially — §10.3"),
        ),
        ("readout", Json::str(W4X_REVIEW_READOUT)),
    ]);
    let freeze = Json::obj([
        ("state", Json::str("frozen")),
        ("ticket", Json::str("R6.57")),
        ("frozen_at", Json::str(W4X_RECORDED_AT)),
        ("gate", Json::str("ADR-0453")),
        (
            "gate_disposition",
            Json::str(
                "executed-W4x supersession — ADR-0453's W4 extension over the a4/a5 \
                 windows",
            ),
        ),
        ("scan_ref", Json::str(W4X_SCAN_REL)),
        ("scan_sha256", Json::str(scan_sha256)),
        ("review_input_sha256", Json::str(input_sha256)),
    ]);
    m.insert("review".to_string(), review);
    m.insert("freeze".to_string(), freeze);
    Ok(Json::Obj(m))
}

/// `w4x_review_verified(review)` — the all-cleared gate over a
/// `hh-flagged-review/1` record: every flagged record dispositioned
/// `cleared`, none confirmed, `coverage` honest. `Ok(())` only when the
/// restored fold may mint — a confirmed or undispositioned flag keeps
/// its veto and the freeze/refold refuses.
pub fn w4x_review_verified(review: &Json) -> Result<(), String> {
    if review.get("schema").and_then(Json::as_str) != Some(crate::fp_results::FLAGGED_REVIEW_SCHEMA)
    {
        return Err("review record is not hh-flagged-review/1".into());
    }
    let cov = review.get("coverage").cloned().unwrap_or(Json::obj([]));
    let flagged = cov.get("flagged").and_then(Json::as_int).unwrap_or(-1);
    let confirmed = cov.get("confirmed").and_then(Json::as_int).unwrap_or(-1);
    let cleared = cov.get("cleared").and_then(Json::as_int).unwrap_or(-1);
    if flagged < 0 || confirmed != 0 || cleared != flagged {
        return Err(format!(
            "withheld{{flagged_review}} — review coverage {{{flagged} flagged, \
             {confirmed} confirmed, {cleared} cleared}} — a confirmed or \
             unreviewed flag stays a veto and the freeze refuses"
        ));
    }
    let rows = review
        .get("records")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default();
    if rows.len() as i64 != flagged {
        return Err(format!(
            "review records {} != coverage.flagged {flagged} — the freeze vetoes",
            rows.len()
        ));
    }
    for r in &rows {
        if r.get("disposition").and_then(Json::as_str) != Some("cleared") {
            return Err(format!(
                "review row {} is not cleared — a non-cleared disposition stays a \
                 veto and the freeze refuses",
                r.get("slot_ref").and_then(Json::as_str).unwrap_or("?")
            ));
        }
    }
    Ok(())
}

/// `w4x_freeze_reviewed_packet(packet, review, scan_sha, input_sha,
/// pre_review_path, restored)` — the reviewed case: the
/// `hh-flagged-review/1` record is verified all-cleared
/// ([`w4x_review_verified`]) then the packet gains `freeze` + `review`.
/// The packet it wraps is the post-review *restored* generation —
/// cleared flags score as passes and the Holm members land; `restored`
/// marks whether the fold's content recomputed (P2-FP/P4-FP) or is the
/// review-untouched rewrap (P3-L2/L3). The freeze member names the
/// pre-review packet it recomputes from — that record stays append-only
/// history, never rewritten.
pub fn w4x_freeze_reviewed_packet(
    packet: &Json,
    review: &Json,
    scan_sha256: &str,
    input_sha256: &str,
    pre_review_path: &str,
    restored: bool,
) -> Result<Json, String> {
    w4x_review_verified(review)?;
    let cov = review.get("coverage").cloned().unwrap_or(Json::obj([]));
    let flagged = cov.get("flagged").and_then(Json::as_int).unwrap_or(-1);
    let confirmed = cov.get("confirmed").and_then(Json::as_int).unwrap_or(-1);
    let cleared = cov.get("cleared").and_then(Json::as_int).unwrap_or(-1);
    let mut m: BTreeMap<String, Json> = match packet {
        Json::Obj(m) => m.clone(),
        _ => return Err("packet is not an object".into()),
    };
    let review_member = Json::obj([
        ("flagged", Json::Int(flagged)),
        ("confirmed", Json::Int(confirmed)),
        ("cleared", Json::Int(cleared)),
        ("reviewed_flags", Json::Int(flagged)),
        ("detector", Json::str(ANSWER_SCAN_VERSION)),
        ("coverage_ppm", Json::Int(1_000_000)),
        ("label", Json::str(crate::fp_results::REVIEW_LABEL)),
        (
            "disposition",
            Json::str(
                review
                    .get("disposition")
                    .and_then(Json::as_str)
                    .unwrap_or("cleared"),
            ),
        ),
        ("readout", Json::str(W4X_REVIEW_READOUT)),
        ("analysis_ref", Json::str(W4X_ANALYSIS_REL)),
    ]);
    let freeze = Json::obj([
        ("state", Json::str("frozen")),
        ("ticket", Json::str("R6.57")),
        ("frozen_at", Json::str(W4X_RECORDED_AT)),
        ("gate", Json::str("ADR-0453")),
        (
            "gate_disposition",
            Json::str("executed-W4x supersession — ADR-0453's W4 extension"),
        ),
        (
            "fold",
            Json::str(if restored {
                "post_review_restored"
            } else {
                "unchanged_by_review"
            }),
        ),
        (
            "fold_note",
            Json::str(if restored {
                format!(
                    "the frozen copy is the post-review restored generation — \
                     the review's cleared dispositions score the flagged passes \
                     as passes and the Holm members land; it recomputes from \
                     {pre_review_path}, which stays append-only history (its \
                     vetoed fold was the honest pending-generation read)"
                )
            } else {
                "the review's cleared dispositions touch no member of this \
                 fold — the frozen copy recomputes byte-identically from the \
                 pre-review record and adds only the freeze/review members"
                    .to_string()
            }),
        ),
        ("recomputed_from", Json::str(pre_review_path)),
        ("review_ref", Json::str(W4X_REVIEW_REL)),
        ("scan_ref", Json::str(W4X_SCAN_REL)),
        ("scan_sha256", Json::str(scan_sha256)),
        ("review_input_sha256", Json::str(input_sha256)),
    ]);
    m.insert("review".to_string(), review_member);
    m.insert("freeze".to_string(), freeze);
    if !m.contains_key("seal_open_ref") {
        m.insert(
            "seal_open_ref".to_string(),
            Json::str(crate::fp_results::SEAL_OPEN_REL),
        );
    }
    Ok(Json::Obj(m))
}

/// `w4x_flagged_review_record(scan, analysis, …)` — mint the
/// `hh-flagged-review/1` over HUMAN-H20's dispositioned rows. Same
/// shape as the W4 record (R6.54): every flagged slot dispositioned,
/// none confirmed, or the build refuses.
pub fn w4x_flagged_review_record(
    scan: &Json,
    analysis: &Json,
    scan_sha256: &str,
    analysis_sha256: &str,
    input_sha256: &str,
) -> Result<Json, String> {
    // the dispositioned rows read the analysis record's flags member
    // verbatim (same member set the W4 analysis record carries).
    let mut rows = Vec::new();
    if let Some(Json::Arr(flags)) = analysis.get("flags") {
        for f in flags {
            let sid = f
                .get("slot_ref")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let disp = f
                .get("disposition")
                .and_then(Json::as_str)
                .unwrap_or("cleared")
                .to_string();
            let mut r: Vec<(String, Json)> = vec![
                ("slot_ref".into(), Json::str(&sid)),
                ("disposition".into(), Json::str(&disp)),
            ];
            for m in ["reason", "verdict", "reviewer"] {
                if let Some(v) = f.get(m) {
                    r.push((m.into(), v.clone()));
                }
            }
            rows.push(obj_dyn(r));
        }
    }
    let cov = scan.get("coverage").cloned().unwrap_or(Json::obj([]));
    let flagged = cov.get("flagged").and_then(Json::as_int).unwrap_or(-1);
    if flagged < 0 {
        return Err("scan coverage.flagged: required".into());
    }
    if rows.len() as i64 != flagged {
        return Err(format!(
            "unreviewed flag: coverage.flagged {flagged} != dispositioned rows \
             {} — the freeze vetoes",
            rows.len()
        ));
    }
    // honest tallies — the cleared/confirmed counts derive from the
    // dispositioned rows, never asserted.
    let cleared = rows
        .iter()
        .filter(|r| r.get("disposition").and_then(Json::as_str) == Some("cleared"))
        .count() as i64;
    let confirmed = rows
        .iter()
        .filter(|r| r.get("disposition").and_then(Json::as_str) == Some("confirmed"))
        .count() as i64;
    let disposition = if cleared == flagged && confirmed == 0 {
        format!(
            "{cleared} of {flagged} cleared — operator mass ruling \
             ({W4X_REVIEW_READOUT} verbatim)"
        )
    } else {
        format!(
            "{cleared} of {flagged} cleared, {confirmed} confirmed — operator \
             ruling ({W4X_REVIEW_READOUT}); a confirmed or undispositioned \
             flag stays a veto"
        )
    };
    Ok(Json::obj([
        (
            "schema",
            Json::str(crate::fp_results::FLAGGED_REVIEW_SCHEMA),
        ),
        ("scan_ref", Json::str(W4X_SCAN_REL)),
        ("scan_sha256", Json::str(scan_sha256)),
        ("readout", Json::str(W4X_REVIEW_READOUT)),
        ("review_input_ref", Json::str(W4X_REVIEW_INPUT_REL)),
        ("review_input_sha256", Json::str(input_sha256)),
        ("analysis_ref", Json::str(W4X_ANALYSIS_REL)),
        ("analysis_sha256", Json::str(analysis_sha256)),
        (
            "detector",
            scan.get("detector").cloned().unwrap_or(Json::Null),
        ),
        (
            "coverage",
            Json::obj([
                ("records", cov.get("scanned").cloned().unwrap_or(Json::Null)),
                (
                    "scored_hits",
                    cov.get("scored_hits").cloned().unwrap_or(Json::Null),
                ),
                ("flagged", cov.get("flagged").cloned().unwrap_or(Json::Null)),
                ("confirmed", Json::Int(confirmed)),
                ("cleared", Json::Int(cleared)),
            ]),
        ),
        ("label", Json::str(crate::fp_results::REVIEW_LABEL)),
        // derived, never a literal: the record mints only after HUMAN-H20's
        // ruling lands, so the disposition summarizes the dispositioned rows
        // and cites the readout where the operator's verbatim lives.
        ("disposition", Json::str(&disposition)),
        ("records", Json::Arr(rows)),
        ("recorded_at", Json::str(W4X_RECORDED_AT)),
        ("ticket", Json::str("R6.57")),
    ]))
}

// ── the landed-packet validator ──────────────────────────────────────

/// `validate_w4x_results_packet(j)` — the superseding-form check for
/// the w4x fold: same member obligations as the W4 validator plus the
/// w4x supersession members; the `not_run{admission:AG3}` and
/// `verifier_unavailable` W4-era spellings may appear only inside the
/// typed-history members, never as a live landed disposition.
pub fn validate_w4x_results_packet(j: &Json) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    if j.get("schema").and_then(Json::as_str) != Some(RESULTS_SCHEMA) {
        errs.push("schema must be hh-comparative-results/1".into());
    }
    for m in ["supersedes", "packet_disposition", "claim"] {
        match j.get(m).and_then(Json::as_str) {
            Some(s) if !s.is_empty() => {}
            _ => errs.push(format!("{m}: non-empty string required")),
        }
    }
    match j.get("supersession") {
        Some(Json::Obj(s))
            if s.contains_key("basis")
                && s.contains_key("w4x_window")
                && s.contains_key("w4xv_window") => {}
        _ => errs.push("supersession: basis + w4x_window + w4xv_window members required".into()),
    }
    let expected = format!(
        "docs/build/reports/2026-11-19-fp-w4-results-{}-frozen.json",
        j.get("claim")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_lowercase()
    );
    if j.get("supersedes").and_then(Json::as_str) != Some(expected.as_str()) {
        errs.push("supersedes must name the frozen W4 executed packet for the claim".into());
    }
    match j.get("harnesses") {
        Some(Json::Obj(hs)) if hs.len() >= 2 => {
            for (arm, h) in hs {
                let p = format!("harnesses.{arm}");
                for m in [
                    "disposition",
                    "theta",
                    "secondary_populations",
                    "delta_vs_ref",
                    "leader_group",
                    "pass_at",
                    "flips",
                    "cost",
                    "counts",
                    "completion",
                    "integrity",
                ] {
                    if h.get(m).is_none() {
                        errs.push(format!("{p}.{m}: required member absent"));
                    }
                }
            }
        }
        _ => errs.push("harnesses: ≥2 arm rows required".into()),
    }
    let text = j.to_canonical_string();
    if text.contains("protected_pair_truncated{k<5,R<5}") {
        errs.push("the truncation refusal never reappears in an executed-W4x packet".into());
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// `w4x_freeze_diff_errors(orig, frozen)` — the member-wise diff rule:
/// every member of the committed packet stays byte-identical and the
/// freeze may add exactly `freeze`, `review` and `seal_open_ref`.
pub fn w4x_freeze_diff_errors(orig: &Json, frozen: &Json) -> Vec<String> {
    w4_freeze_diff_errors(orig, frozen)
}

/// `sha256_hex` re-export for the bin (kept — the record sidecar
/// digest idiom).
pub fn w4x_sha256_hex(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}
