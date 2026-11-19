//! R6.53 — the executed-W4 re-fold (manifest 325b). ADR-0452 superseded
//! GATE-G30's `re-scope{end-first-pass-early}` for the W4 scope: the
//! protected pair (mini-swe-agent + HH-native) and V5 ran under the
//! standard protocol; Pi and Claude-Code stayed `not_run{admission:AG3}`
//! verbatim. This module folds the committed `hh-fp-window/1` W4 record
//! plus the retained harvest into superseding `hh-comparative-results/1`,
//! `hh-fp-l2/1`, `hh-fp-l2-audit/1` and `hh-fp-l3/1` packets — the frozen
//! R6.48/R6.49 withheld packets stay append-only history, never rewritten.
//!
//! Honest outcomes land verbatim: mini-swe-agent's 50 settled slots
//! carry real rewards (the retained `collect/reward.txt` fold); HH-native
//! settles 50 drives (46 `rc=0`, 4 recorded `rc=1` failures) with every
//! verifier exec recorded and no reward produced; V5 settles 20
//! doc-admission failures (`verifier_unavailable{env_closed_or_unmounted}`)
//! and drops 30 slots at the round-2 makespan checkpoint; Pi/CC are
//! `not_run{admission:AG3}`. `unavailable`/`inconclusive`/`not_run` are
//! landed outcomes — never fabricated scores, never silent drops.
//!
//! The score-bearing leaves that cannot compute carry typed markers in
//! the same absent-state vocabulary the computed packet admits
//! (`not_computed`/`refused`/… — REQ-XF-122); every packet carries a
//! `supersedes`/`supersession` member citing the frozen packet it
//! replaces, the W4 window record digest, and ADR-0452.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators::{
    finite_manifest_rate, pass_rates, rerun_flip, AdmissionRecord, FlipCell, RateDecl,
    LABEL_CONFIRMED_MIN, LABEL_EXPLORATORY_LEADER, LABEL_FIRST_PASS, LABEL_MODEL_SPEND,
    LABEL_SECONDARY_POP, PRIMARY_METRIC, REQUIRED_LABELS, REQUIRED_POPULATIONS, RESULTS_SCHEMA,
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
use crate::fp_results::{marker, FamilyMember, L2_AUDIT_SCHEMA, L3_SCHEMA, RECORDED_AT};
use crate::json_ext::JsonExt;

/// `hh-fp-w4-outcomes/1` — the committed fold of the executed W4 slot
/// evidence (one row per settled slot, plus the dropped/not_run sets).
pub const W4_OUTCOMES_SCHEMA: &str = "hh-fp-w4-outcomes/1";
/// `hh-flagged-review-input/1` — the REQ-XD-071 handoff to the human row.
pub const FLAGGED_INPUT_SCHEMA: &str = "hh-flagged-review-input/1";
/// The committed executed-W4 window record this fold reads.
pub const W4_WINDOW_PATH: &str = "docs/build/reports/2026-11-19-fp-w4-window.json";
/// The scope amendment carrying this fold's authority.
pub const ADR_0452_PATH: &str = "docs/adr/ADR-0452-w4-scope-amendment.md";
/// The verbatim not_run spelling for the admission-blocked arms.
pub const NOT_RUN_AG3: &str = "not_run{admission:AG3}";
/// The new human row the flagged-review handoff routes to.
pub const REVIEW_INPUT_TO: &str = "HUMAN-H19";
/// Its readout path.
pub const REVIEW_READOUT: &str = "docs/build/readouts/HUMAN-H19.md";
/// The declared estimator seed (the packets record the full seed
/// material verbatim — any fold recomputes byte-identically).
pub const W4_EST_SEED: u64 = 0xF4_53;
/// The seeded audit draw size: 10% of the 50 planned pairs → 5.
const AUDIT_SAMPLE_N: usize = 5;
/// The committed L1-on-L2 audit replay seed (the same audit lane
/// `fp-results` drives).
pub const AUDIT_SEED: u64 = 0xfeed;
const FAMILY_ALPHA_PPM: i64 = 50_000;
const L2_EPSILON_PPM: i64 = 200_000;
const L2_PLANNED_N: i64 = 50;

fn obj_dyn(pairs: impl IntoIterator<Item = (String, Json)>) -> Json {
    Json::Obj(pairs.into_iter().collect())
}

fn refused(reason: &str) -> Json {
    marker("refused", reason)
}

fn not_computed(reason: &str) -> Json {
    marker("not_computed", reason)
}

fn unavailable(reason: &str) -> Json {
    marker("not_applicable", reason)
}

/// The GATE-G23 method-admission record the `first_pass` estimator use
/// cites (REQ-XD-023's hook — the gate is PASSED).
pub fn g23_admission() -> AdmissionRecord {
    AdmissionRecord {
        gate: "GATE-G23".into(),
        status: "passed".into(),
        record_ref: "docs/build/readouts/GATE-G23.md".into(),
    }
}

// ── the committed outcomes record ────────────────────────────────────

/// One settled slot's folded facts — the committed outcomes record's
/// per-slot row (verifier/verdict members carried verbatim).
pub struct W4SlotFact {
    /// `collect/reward.txt` value where one exists (mini-swe-agent).
    pub reward: Option<bool>,
    /// The retained reward file's sha256 (cited, never copied).
    pub reward_sha256: Option<String>,
    /// Steps folded from the retained trajectory/events (descriptive).
    pub steps: Option<i64>,
    /// Tool calls folded the same way.
    pub tool_calls: Option<i64>,
    /// Non-zero/unknown tool observations folded the same way.
    pub errors: Option<i64>,
}

/// `w4_outcomes_record(window, facts, sources)` — the
/// `hh-fp-w4-outcomes/1` fold. `facts` is `slot_id → W4SlotFact`
/// (retained-folded members; the committed record carries them with
/// `retained:`-prefixed citations).
pub fn w4_outcomes_record(
    window: &Json,
    facts: &BTreeMap<String, W4SlotFact>,
    derived_v5_dropped: &[String],
    planned_per_arm: &BTreeMap<String, i64>,
    sources: Vec<(String, String)>,
) -> Json {
    let sr = window
        .get("slot_results")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let mut rows: Vec<(String, Json)> = Vec::new();
    let mut arm_counts: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for (sid, s) in &sr {
        let arm = s
            .get("arm")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let f = facts.get(sid);
        let reward_v = f.and_then(|f| f.reward);
        let mut row: Vec<(&str, Json)> = vec![
            ("slot_id", Json::str(sid)),
            ("arm", Json::str(&arm)),
            ("task", s.get("task").cloned().unwrap_or(Json::Null)),
            (
                "replicate",
                s.get("replicate").cloned().unwrap_or(Json::Null),
            ),
            ("rc", s.get("rc").cloned().unwrap_or(Json::Null)),
            (
                "cost_micros",
                s.get("cost_micros").cloned().unwrap_or(Json::Null),
            ),
            ("wall_ms", s.get("wall_ms").cloned().unwrap_or(Json::Null)),
        ];
        if let Some(v) = s.get("verifier") {
            row.push(("verifier", v.clone()));
        }
        match (reward_v, f.and_then(|f| f.reward_sha256.clone())) {
            (Some(r), sha) => row.push((
                "reward",
                Json::obj([
                    ("value", Json::Int(r as i64)),
                    ("file_sha256", sha.map(Json::str).unwrap_or(Json::Null)),
                ]),
            )),
            _ => row.push(("reward", Json::Null)),
        }
        row.push((
            "steps",
            f.and_then(|f| f.steps).map(Json::Int).unwrap_or(Json::Null),
        ));
        row.push((
            "tool_calls",
            f.and_then(|f| f.tool_calls)
                .map(Json::Int)
                .unwrap_or(Json::Null),
        ));
        row.push((
            "errors",
            f.and_then(|f| f.errors)
                .map(Json::Int)
                .unwrap_or(Json::Null),
        ));
        let pass = match reward_v {
            Some(r) => Json::Bool(r),
            None => Json::str("unavailable{no_scored_outcome}"),
        };
        row.push(("pass", pass));
        rows.push((sid.clone(), Json::obj(row)));
        let c = arm_counts.entry(arm).or_default();
        *c.entry("settled".into()).or_default() += 1;
        let rc = s.get("rc").and_then(Json::as_int).unwrap_or(-1);
        *c.entry(format!("rc{rc}")).or_default() += 1;
        if reward_v == Some(true) {
            *c.entry("passes".into()).or_default() += 1;
        }
        if s.get("verifier").is_some() {
            let disp = s
                .get("verifier")
                .and_then(|v| v.get("disposition"))
                .and_then(Json::as_str)
                .unwrap_or("");
            *c.entry(format!("verifier:{disp}")).or_default() += 1;
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
    for arm in ["pi", "claude-code"] {
        arms.push((
            arm.to_string(),
            Json::obj([
                (
                    "planned",
                    Json::Int(*planned_per_arm.get(arm).unwrap_or(&50)),
                ),
                (
                    "disposition",
                    Json::str(
                        window
                            .get("not_run")
                            .and_then(|n| n.get(arm))
                            .and_then(Json::as_str)
                            .unwrap_or(NOT_RUN_AG3),
                    ),
                ),
            ]),
        ));
    }
    let src: Vec<Json> = sources
        .iter()
        .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
        .collect();
    Json::obj([
        ("schema", Json::str(W4_OUTCOMES_SCHEMA)),
        (
            "run_id",
            window.get("run_id").cloned().unwrap_or(Json::Null),
        ),
        (
            "window",
            window.get("window").cloned().unwrap_or(Json::Null),
        ),
        ("seed", window.get("seed").cloned().unwrap_or(Json::Null)),
        (
            "checkpoint",
            window.get("checkpoint").cloned().unwrap_or(Json::Null),
        ),
        (
            "not_run",
            window.get("not_run").cloned().unwrap_or(Json::Null),
        ),
        (
            "withheld",
            window.get("withheld").cloned().unwrap_or(Json::Arr(vec![])),
        ),
        (
            "drop_order",
            window
                .get("drop_order")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
        ),
        (
            "v5_dropped",
            Json::obj([
                ("count", Json::Int(derived_v5_dropped.len() as i64)),
                (
                    "reason",
                    Json::str("dropped{makespan_checkpoint:w4-round-2}"),
                ),
                (
                    "slot_ids",
                    Json::Arr(derived_v5_dropped.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        ("arms", obj_dyn(arms)),
        ("slots", obj_dyn(rows)),
        ("sources", Json::Arr(src)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ])
}

// ── outcome parsing (the emit-packets leg reads committed records) ───

/// One committed outcomes row's parsed view.
#[derive(Debug, Clone)]
pub struct SlotOutcome {
    /// The slot id.
    pub slot_id: String,
    /// The arm.
    pub arm: String,
    /// The task.
    pub task: String,
    /// The replicate.
    pub replicate: i64,
    /// The process exit code.
    pub rc: i64,
    /// Recorded cost (µ$).
    pub cost_micros: i64,
    /// Recorded wall time.
    pub wall_ms: i64,
    /// The scored pass where one exists.
    pub pass: Option<bool>,
    /// Folded steps.
    pub steps: Option<i64>,
    /// Folded tool calls.
    pub tool_calls: Option<i64>,
    /// Folded errors.
    pub errors: Option<i64>,
}

/// `slot_outcomes(outcomes)` — parse the committed outcomes record's
/// `slots` member into rows.
pub fn slot_outcomes(outcomes: &Json) -> Vec<SlotOutcome> {
    let mut out = Vec::new();
    if let Some(Json::Obj(slots)) = outcomes.get("slots") {
        for (sid, s) in slots {
            let pass = match s.get("pass") {
                Some(Json::Bool(b)) => Some(*b),
                _ => None,
            };
            out.push(SlotOutcome {
                slot_id: sid.clone(),
                arm: s.get("arm").and_then(Json::as_str).unwrap_or("").into(),
                task: s.get("task").and_then(Json::as_str).unwrap_or("").into(),
                replicate: s.get("replicate").and_then(Json::as_int).unwrap_or(0),
                rc: s.get("rc").and_then(Json::as_int).unwrap_or(-1),
                cost_micros: s.get("cost_micros").and_then(Json::as_int).unwrap_or(0),
                wall_ms: s.get("wall_ms").and_then(Json::as_int).unwrap_or(0),
                pass,
                steps: s.get("steps").and_then(Json::as_int),
                tool_calls: s.get("tool_calls").and_then(Json::as_int),
                errors: s.get("errors").and_then(Json::as_int),
            });
        }
    }
    out.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));
    out
}

/// The committed scan record's `slot_ref → veto` map (a flagged record
/// contributes the observed veto axis; cleared flags only exist in a
/// review record — none does yet).
pub fn scan_vetoes(scan: &Json) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    if let Some(Json::Arr(recs)) = scan.get("records") {
        for r in recs {
            if r.get("state").and_then(Json::as_str) == Some("flagged") {
                if let Some(sid) = r.get("slot_ref").and_then(Json::as_str) {
                    out.insert(sid.to_string(), vec!["answer_scan".to_string()]);
                }
            }
        }
    }
    out
}

/// `planned_population(rows, arm, vetoes)` — the reconciled
/// `PlannedPopulation` through `population::assemble` (the one
/// counting rule — never a reimplementation). Each committed outcome
/// row binds one subject: a scored row resolves `scored` with its
/// recorded `success`; an unscored settled slot resolves
/// `oracle_failure` (the verifier produced no verdict — never scored,
/// never coerced).
pub fn planned_population(
    rows: &[SlotOutcome],
    arm: &str,
    vetoes: &BTreeMap<String, Vec<String>>,
) -> Result<PlannedPopulation, hharness_eval::population::PopulationError> {
    let mut slots = Vec::new();
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
            split_label: "w4".into(),
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
    assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
}

// ── arm rows ─────────────────────────────────────────────────────────

/// The typed (non-scored) arm row — disposition + served/completion
/// counts parameterize the same member set the withheld row carries.
pub fn typed_arm_row(
    participant: &str,
    intended_participant: &str,
    disposition: &str,
    planned: i64,
    served: i64,
    completion: Json,
    delta_reason: &str,
    swap_refs: &[String],
    is_ref: bool,
    measured_cost: Option<(i64, &str)>,
) -> Json {
    let finite_label =
        "finite-manifest rate over the 10-task tranche — internal record — not outside-verified";
    let served_m = obj_dyn([(participant.to_string(), Json::Int(served))]);
    let intended_m = obj_dyn([(intended_participant.to_string(), Json::Int(planned))]);
    let mut flips_swap: Vec<(String, Json)> = Vec::new();
    let mut flips_excess: Vec<(String, Json)> = Vec::new();
    for r in swap_refs {
        flips_swap.push((
            r.clone(),
            Json::obj([
                ("interval", refused(disposition)),
                ("label", Json::str("deterministic resample pair-flip")),
            ]),
        ));
        flips_excess.push((
            r.clone(),
            Json::obj([
                ("ppm", refused(disposition)),
                ("interval", refused(disposition)),
                ("label", Json::str("harness excess vs the resampled pair")),
            ]),
        ));
    }
    let per_trial = match measured_cost {
        Some((m, basis)) => Json::obj([
            ("micros", Json::Int(m)),
            (
                "interval",
                not_computed(
                    "the executed fold declares no interval route over the slot \
                     costs — the point is the observed mean over settled slots",
                ),
            ),
            ("basis", Json::str(basis)),
            (
                "label",
                Json::str(format!("descriptive; {LABEL_MODEL_SPEND}")),
            ),
        ]),
        None => Json::obj([
            ("micros", refused(disposition)),
            ("interval", refused(disposition)),
            ("basis", Json::str("no executed tranche")),
            (
                "label",
                Json::str(format!("descriptive; {LABEL_MODEL_SPEND}")),
            ),
        ]),
    };
    Json::obj([
        ("disposition", Json::str(disposition)),
        (
            "theta",
            Json::obj([
                ("ppm", refused(disposition)),
                ("lo_ppm", refused(disposition)),
                ("hi_ppm", refused(disposition)),
                ("interval", refused(disposition)),
                (
                    "bounds",
                    Json::obj([
                        ("lo_ppm", Json::Int(0)),
                        ("hi_ppm", Json::Int(1_000_000)),
                        ("kind", Json::str("all_planned")),
                    ]),
                ),
                ("label", Json::str(finite_label)),
            ]),
        ),
        (
            "secondary_populations",
            Json::obj([
                (
                    "raw",
                    Json::obj([
                        ("rate_ppm", refused(disposition)),
                        ("label", Json::str(LABEL_SECONDARY_POP)),
                    ]),
                ),
                (
                    "errors_zero",
                    Json::obj([
                        ("bound_lo_ppm", Json::Int(0)),
                        ("bound_hi_ppm", Json::Int(1_000_000)),
                        (
                            "label",
                            Json::str(format!(
                                "{LABEL_SECONDARY_POP} — errors=0 all-planned bound"
                            )),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "delta_vs_ref",
            if is_ref {
                Json::obj([
                    ("ppm", Json::Int(0)),
                    (
                        "interval",
                        Json::obj([
                            ("lo_ppm", Json::Int(0)),
                            ("hi_ppm", Json::Int(0)),
                            ("kind", Json::str("degenerate_reference")),
                        ]),
                    ),
                    ("p_raw_ppm", Json::Int(1_000_000)),
                    ("p_holm_ppm", Json::Int(1_000_000)),
                    ("label", Json::str("reference arm — Δ ≡ 0 by construction")),
                ])
            } else {
                refused(delta_reason)
            },
        ),
        (
            "leader_group",
            Json::obj([
                ("member", Json::Bool(false)),
                ("leader_id", Json::str("unavailable{single_scored_arm}")),
                ("diff_interval", refused(disposition)),
                ("label", Json::str(LABEL_EXPLORATORY_LEADER)),
            ]),
        ),
        (
            "pass_at",
            Json::obj([
                ("k", Json::Int(5)),
                ("all_ppm", refused(disposition)),
                ("any_ppm", refused(disposition)),
                (
                    "label",
                    Json::str("pass^k / pass@k over the tranche's k=5 repeats"),
                ),
            ]),
        ),
        (
            "flips",
            Json::obj([
                ("rerun", refused(disposition)),
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
                ("per_trial", per_trial),
                (
                    "per_solve",
                    marker(
                        "estimator_undefined",
                        "no scored solve — the per-solve route needs a scored \
                         numerator, never a fabricated one",
                    ),
                ),
                ("provenance", Json::str("model_spend_only")),
            ]),
        ),
        (
            "counts",
            Json::obj([
                ("analysis_basis", Json::str("assignment")),
                ("intended", intended_m),
                ("served", served_m),
            ]),
        ),
        ("completion", completion),
        (
            "integrity",
            Json::obj([
                ("flagged", Json::Int(0)),
                ("confirmed", Json::Int(0)),
                ("cleared", Json::Int(0)),
                ("coverage_ppm", Json::Int(0)),
                ("detector", Json::str(ANSWER_SCAN_VERSION)),
                (
                    "label",
                    Json::str(format!(
                        "integrity panel — {LABEL_CONFIRMED_MIN}; this arm produced \
                         no scored verdict, so it sits outside the scored-trial \
                         scan universe (the records' not_scored member carries the \
                         slot-level reason)"
                    )),
                ),
            ]),
        ),
    ])
}

/// The landed mini-swe-agent row — real cells, real costs, the joint
/// population's veto-pending passes scored as failures.
pub fn landed_arm_row(
    participant: &str,
    intended_participant: &str,
    pop: &PlannedPopulation,
    raw_cells: &[FlipCell],
    cost_micros: &[i64],
    flagged: i64,
    coverage_ppm: i64,
    swap_refs: &[String],
) -> Result<Json, Vec<String>> {
    let adm = g23_admission();
    let rate = finite_manifest_rate(
        pop,
        &RateDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            arm: "mini-swe-agent".into(),
            stratum: None,
            prior: REGISTERED_PRIOR,
            prior_name: REGISTERED_PRIOR_NAME.to_string(),
            bounds: BoundsDecl::Unit,
            level_ppm: LEVEL_PPM,
            seed: W4_EST_SEED,
            use_as: "first_pass".into(),
            admission: Some(adm.clone()),
        },
    )
    .map_err(|e| vec![format!("finite_manifest_rate: {e:?}")])?;
    let (pass_all, pass_any) = pass_rates(raw_cells, 5, "descriptive", None, 4_000, W4_EST_SEED)
        .map_err(|e| vec![format!("pass_rates: {e:?}")])?;
    let rerun = rerun_flip(raw_cells, "descriptive", None, 4_000, W4_EST_SEED)
        .map_err(|e| vec![format!("rerun_flip: {e:?}")])?;
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
            ("basis", Json::str("w4_total_over_solves")),
        ])
    } else {
        marker("estimator_undefined", "0 solved — no per-solve numerator")
    };
    let mut flips_swap: Vec<(String, Json)> = Vec::new();
    let mut flips_excess: Vec<(String, Json)> = Vec::new();
    for r in swap_refs {
        flips_swap.push((
            r.clone(),
            Json::obj([
                ("interval", refused(NOT_RUN_AG3)),
                ("label", Json::str("deterministic resample pair-flip")),
            ]),
        ));
        flips_excess.push((
            r.clone(),
            Json::obj([
                ("ppm", refused(NOT_RUN_AG3)),
                ("interval", refused(NOT_RUN_AG3)),
                ("label", Json::str("harness excess vs the resampled pair")),
            ]),
        ));
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
                    Json::str(
                        "finite-manifest rate over the 10-task tranche — joint \
                         population (veto-pending passes score as failures until \
                         review clears) — internal record — not outside-verified",
                    ),
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
        (
            "delta_vs_ref",
            Json::obj([
                ("ppm", Json::Int(0)),
                (
                    "interval",
                    Json::obj([
                        ("lo_ppm", Json::Int(0)),
                        ("hi_ppm", Json::Int(0)),
                        ("kind", Json::str("degenerate_reference")),
                    ]),
                ),
                ("p_raw_ppm", Json::Int(1_000_000)),
                ("p_holm_ppm", Json::Int(1_000_000)),
                ("label", Json::str("reference arm — Δ ≡ 0 by construction")),
            ]),
        ),
        (
            "leader_group",
            Json::obj([
                ("member", Json::Bool(true)),
                ("leader_id", Json::str(participant)),
                ("diff_interval", fm_interval),
                (
                    "label",
                    Json::str(format!(
                        "{LABEL_EXPLORATORY_LEADER} — leader over the single scored \
                         arm; the comparators are {NOT_RUN_AG3}"
                    )),
                ),
            ]),
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
                        ("basis", Json::str("w4_measured_mean")),
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
                ("confirmed", Json::Int(0)),
                ("cleared", Json::Int(0)),
                ("coverage_ppm", Json::Int(coverage_ppm)),
                ("detector", Json::str(ANSWER_SCAN_VERSION)),
                (
                    "label",
                    Json::str(format!(
                        "integrity panel — {LABEL_CONFIRMED_MIN}; unreviewed flags \
                         stay vetoes — {REVIEW_INPUT_TO} routes them"
                    )),
                ),
            ]),
        ),
    ]))
}

/// The superseding `hh-comparative-results/1` packet — same member set
/// as the withheld form plus `supersedes`/`supersession`.
pub fn w4_results_packet(
    claim: &str,
    reference: &str,
    arms: Vec<(String, Json)>,
    family_members: Vec<Json>,
    expl_members: Vec<Json>,
    packet_disposition: &str,
    executed_evidence: &str,
    expected_precision: &str,
    supersedes_rel: &str,
    w4_window_sha256: &str,
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
                ("id", Json::str("fp-w4")),
                ("state", Json::str("executed{adr-0452}")),
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
                ("basis", Json::str(ADR_0452_PATH)),
                (
                    "note",
                    Json::str(
                        "ADR-0452 superseded GATE-G30's re-scope{end-first-pass-early} \
                         for the W4 scope — the executed window's evidence refolds \
                         this claim; the frozen withheld packet stays append-only \
                         history, never rewritten",
                    ),
                ),
                (
                    "w4_window",
                    Json::obj([
                        ("path", Json::str(W4_WINDOW_PATH)),
                        ("sha256", Json::str(w4_window_sha256)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ])
}

/// A family member row for the superseding form (same shape as the
/// withheld family rows).
pub fn family_member(m: &FamilyMember) -> Json {
    crate::fp_results::family_row(m)
}

// ── the superseding L2 packet ────────────────────────────────────────

/// `w4_l2_packet(...)` — the `hh-fp-l2/1` superseding fold. The 50
/// pairs launched coupled; the O side (mini-swe-agent) scored, the H
/// side produced no verifier outcome — admission, TOST and the
/// conjunction land `unavailable{h_side_verdicts_absent}`.
pub fn w4_l2_packet(
    gl1_pass_id: &str,
    h_identity_digest: &str,
    corpus_digest: &str,
    expected_precision: &str,
    supersedes_rel: &str,
    w4_window_sha256: &str,
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
                         failures — 49×test.sh absent, 1×container improper), so \
                         the parity-admission fold has zero resolved pair members",
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
                         outcomes the inference itself is unavailable, never \
                         synthesized",
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
                "first-pass L2 packet — {verdict}; {LABEL_FIRST_PASS}"
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
                ("basis", Json::str(ADR_0452_PATH)),
                (
                    "note",
                    Json::str(
                        "the executed window's evidence refolds this claim; the \
                         frozen withheld packet stays append-only history",
                    ),
                ),
                (
                    "w4_window",
                    Json::obj([
                        ("path", Json::str(W4_WINDOW_PATH)),
                        ("sha256", Json::str(w4_window_sha256)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ])
}

// ── the superseding audit record ─────────────────────────────────────

/// Deterministic seeded sample: `ids` sorted by
/// `sha256("fp-w4-l2-audit|<seed>|<id>")` ascending, first `n`.
pub fn audit_sample(ids: &[String], seed: u64, n: usize) -> Vec<String> {
    let mut keyed: Vec<(String, String)> = ids
        .iter()
        .map(|i| {
            (
                sha256_hex(format!("fp-w4-l2-audit|{seed}|{i}").as_bytes()),
                i.clone(),
            )
        })
        .collect();
    keyed.sort();
    keyed.into_iter().take(n).map(|(_, i)| i).collect()
}

/// `w4_audit_packet(...)` — the `hh-fp-l2-audit/1` superseding fold.
/// The population is the 50 W4 O-side recordings (the mini-swe-agent
/// slots — W2-corpus ids still refuse at decode); the seeded sample is
/// drawn and recorded; the replay leg itself did not execute in this
/// ticket — `audit`/`verdict` carry the typed `not_computed` outcome.
pub fn w4_audit_packet(
    w4_o_ids: &BTreeSet<String>,
    w2_corpus_ids: &BTreeSet<String>,
    seed: u64,
    supersedes_rel: &str,
    w4_window_sha256: &str,
    sources: Json,
) -> Result<Json, crate::fp_results::AuditError> {
    let sample = audit_sample(
        &w4_o_ids.iter().cloned().collect::<Vec<_>>(),
        seed,
        AUDIT_SAMPLE_N,
    );
    for id in &sample {
        if w2_corpus_ids.contains(id) || !w4_o_ids.contains(id) {
            return Err(crate::fp_results::AuditError::NotW4O { id: id.clone() });
        }
    }
    let replay_reason = "o_recording_replay_not_executed — the L1.1 obs_replay \
        leg over receipt-built recordings was not wired in this ticket; the \
        population and seeded sample are recorded so the owed replay re-derives \
        them byte-identically";
    Ok(Json::obj([
        ("schema", Json::str(L2_AUDIT_SCHEMA)),
        ("audit_of", Json::str("P3-L2")),
        ("req", Json::str("REQ-XD-119")),
        ("seed", Json::Int(seed as i64)),
        (
            "population",
            Json::str("w4_o_recordings_only — every other recording class refuses"),
        ),
        ("population_n", Json::Int(w4_o_ids.len() as i64)),
        (
            "planned_sample",
            Json::str("10% of the 50 planned pairs → 5 O-side recordings"),
        ),
        ("sampled_n", Json::Int(sample.len() as i64)),
        (
            "sampled",
            Json::Arr(sample.iter().map(Json::str).collect()),
        ),
        (
            "sample_rule",
            Json::str(
                "sha256(fp-w4-l2-audit|<seed>|<slot_id>) ascending, first 5 — \
                 deterministic, recomputable",
            ),
        ),
        (
            "guard",
            Json::obj([
                (
                    "rule",
                    Json::str(
                        "a W2-corpus recording (or any non-W4-O id) in the sample refuses at decode",
                    ),
                ),
                ("result", Json::str("sample all-W4-O")),
            ]),
        ),
        ("audit", not_computed(replay_reason)),
        ("verdict", Json::str(format!("unavailable{{{replay_reason}}}"))),
        (
            "label",
            Json::str(
                "the audit is machinery evidence over executed W4 O recordings; the \
                 population is non-empty and the seeded sample is drawn — the replay \
                 leg stays an honest not_computed until the L1.1 obs_replay leg \
                 executes; never samples a W2 corpus recording",
            ),
        ),
        ("supersedes", Json::str(format!("docs/build/reports/{supersedes_rel}"))),
        (
            "supersession",
            Json::obj([
                ("basis", Json::str(ADR_0452_PATH)),
                (
                    "w4_window",
                    Json::obj([
                        ("path", Json::str(W4_WINDOW_PATH)),
                        ("sha256", Json::str(w4_window_sha256)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ]))
}

// ── the superseding L3 report ────────────────────────────────────────

fn dist(vals: &[i64]) -> Json {
    if vals.is_empty() {
        return unavailable("no folded values");
    }
    let mut v = vals.to_vec();
    v.sort();
    let med = if v.len() % 2 == 1 {
        v[v.len() / 2]
    } else {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2
    };
    Json::obj([
        ("n", Json::Int(v.len() as i64)),
        ("min", Json::Int(v[0])),
        ("median", Json::Int(med)),
        ("max", Json::Int(v[v.len() - 1])),
        ("kind", Json::str("observed_distribution")),
    ])
}

fn pair_diff(
    o: &[SlotOutcome],
    h: &[SlotOutcome],
    pick: impl Fn(&SlotOutcome) -> Option<i64>,
) -> Json {
    let mut diffs = Vec::new();
    let hmap: BTreeMap<(String, i64), &SlotOutcome> = h
        .iter()
        .map(|s| ((s.task.clone(), s.replicate), s))
        .collect();
    for o_s in o {
        if let Some(h_s) = hmap.get(&(o_s.task.clone(), o_s.replicate)) {
            if let (Some(a), Some(b)) = (pick(o_s), pick(h_s)) {
                diffs.push(a - b);
            }
        }
    }
    dist(&diffs)
}

/// `w4_l3_report(...)` — the `hh-fp-l3/1` superseding fold: per-arm and
/// per-pair descriptive distributions over the executed window; outcome
/// discordance is `unavailable` — the H side has no scored verdict.
pub fn w4_l3_report(
    o_rows: &[SlotOutcome],
    h_rows: &[SlotOutcome],
    supersedes_rel: &str,
    w4_window_sha256: &str,
    sources: Json,
) -> Json {
    let arm_dist = |rows: &[SlotOutcome]| {
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
                "label",
                Json::str("descriptive — per-arm distribution over settled slots"),
            ),
        ])
    };
    let verdict = "unavailable{h_side_verdicts_absent}";
    let uncertain: Vec<Json> = o_rows
        .iter()
        .map(|r| {
            Json::obj([
                ("pair", Json::str(format!("{}-r{}", r.task, r.replicate))),
                ("o_slot", Json::str(&r.slot_id)),
                ("class", Json::str("uncertain{h_side_verdict_absent}")),
            ])
        })
        .collect();
    Json::obj([
        ("schema", Json::str(L3_SCHEMA)),
        ("of", Json::str("P3-L2")),
        ("req", Json::str("REQ-XD-120")),
        (
            "population",
            Json::str("all 50 planned pairs; uncertain pairs as a separate class"),
        ),
        ("planned_pairs", Json::Int(L2_PLANNED_N)),
        ("launched_pairs", Json::Int(o_rows.len() as i64)),
        ("served_pairs", Json::Int(0)),
        (
            "per_arm",
            Json::obj([("o", arm_dist(o_rows)), ("h", arm_dist(h_rows))]),
        ),
        (
            "per_pair",
            Json::obj([
                (
                    "diffs",
                    Json::obj([
                        ("steps", pair_diff(o_rows, h_rows, |r| r.steps)),
                        ("tool_calls", pair_diff(o_rows, h_rows, |r| r.tool_calls)),
                        ("errors", pair_diff(o_rows, h_rows, |r| r.errors)),
                        ("wall_ms", pair_diff(o_rows, h_rows, |r| Some(r.wall_ms))),
                        ("outcome", unavailable(verdict)),
                    ]),
                ),
                ("uncertain_class", Json::Arr(uncertain)),
                (
                    "label",
                    Json::str("descriptive — the pair-difference distribution"),
                ),
            ]),
        ),
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
                ("basis", Json::str(ADR_0452_PATH)),
                (
                    "w4_window",
                    Json::obj([
                        ("path", Json::str(W4_WINDOW_PATH)),
                        ("sha256", Json::str(w4_window_sha256)),
                    ]),
                ),
            ]),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ])
}

// ── the W4 scan report + flagged-review input ────────────────────────

/// `w4_scan_report(slots, not_scored, sources)` — the
/// `hh-fp-answer-scan-report/1` fold over the executed W4 scored
/// trials (the mini-swe-agent slots — the only verdicts). Slots with
/// no scored outcome land on `not_scored` with their recorded reason.
pub fn w4_scan_report(
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
    // REQ-XD-071's handoff routes to this fold's human row — the new
    // HUMAN-H19 — and records the pending{human} disposition verbatim.
    m.insert(
        "review_handoff".to_string(),
        Json::obj([
            ("schema", Json::str(FLAGGED_INPUT_SCHEMA)),
            ("input_to", Json::str(REVIEW_INPUT_TO)),
            ("readout", Json::str(REVIEW_READOUT)),
            ("disposition", Json::str("pending{human}")),
            (
                "note",
                Json::str(
                    "the review rows are HUMAN-H19's act — this handoff carries \
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
    m.insert("ticket".to_string(), Json::str("R6.53"));
    Ok(Json::Obj(m))
}

/// `w4_flagged_review_input(scan)` — the standalone
/// `hh-flagged-review-input/1` handoff: the flagged-pass list (masked
/// identity + excerpt hashes) routed to HUMAN-H19 with
/// `pending{human}` disposition.
pub fn w4_flagged_review_input(scan: &Json, sources: Vec<(String, String)>) -> Json {
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
        ("schema", Json::str(FLAGGED_INPUT_SCHEMA)),
        ("input_to", Json::str(REVIEW_INPUT_TO)),
        ("readout", Json::str(REVIEW_READOUT)),
        ("disposition", Json::str("pending{human}")),
        (
            "scan_ref",
            Json::str("docs/build/reports/2026-11-19-fp-w4-answer-scan.json"),
        ),
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
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.53")),
    ])
}

// ── the W4 freeze ────────────────────────────────────────────────────

/// `w4_freeze_packet(packet, scan_sha, input_sha)` — mint the frozen
/// site-facing copy. The freeze veto (REQ-XH-121): any unreviewed or
/// confirmed flag refuses — `Err` carries the typed reason. With zero
/// flagged rows the review requirement is trivially met (§10.3's
/// "zero flags recorded trivially").
pub fn w4_freeze_packet(
    packet: &Json,
    scan: &Json,
    scan_sha256: &str,
    input_sha256: &str,
) -> Result<Json, String> {
    let cov = scan.get("coverage").cloned().unwrap_or(Json::obj([]));
    let flagged = cov.get("flagged").and_then(Json::as_int).unwrap_or(-1);
    if flagged != 0 {
        return Err(format!(
            "unreviewed flags — coverage.flagged {flagged} != 0; the flagged \
             review ({REVIEW_INPUT_TO}) is pending{{human}}; every unreviewed \
             flag stays a veto and the freeze refuses"
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
        ("label", Json::str(LABEL_CONFIRMED_MIN)),
        (
            "disposition",
            Json::str(
                "zero flags recorded — the review requirement is trivially met \
                 (§10.3 'zero flags recorded trivially'); no review rows exist \
                 to disposition",
            ),
        ),
        ("readout", Json::str(REVIEW_READOUT)),
        ("input_sha256", Json::str(input_sha256)),
    ]);
    let freeze = Json::obj([
        ("state", Json::str("frozen")),
        ("ticket", Json::str("R6.53")),
        ("frozen_at", Json::str(RECORDED_AT)),
        ("gate", Json::str("ADR-0452")),
        (
            "gate_disposition",
            Json::str(
                "executed-W4 supersession — ADR-0452 amends GATE-G30's \
                re-scope{end-first-pass-early} for the W4 scope",
            ),
        ),
        (
            "review_ref",
            Json::str("docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json"),
        ),
        (
            "scan_ref",
            Json::str("docs/build/reports/2026-11-19-fp-w4-answer-scan.json"),
        ),
        ("scan_sha256", Json::str(scan_sha256)),
    ]);
    m.insert("review".to_string(), review);
    m.insert("freeze".to_string(), freeze);
    Ok(Json::Obj(m))
}

// ── the landed-packet validator ──────────────────────────────────────

/// `validate_w4_results_packet(j)` — the superseding-form check: the
/// same shape obligations as the withheld form plus the supersession
/// members, and the honesty rules — the landed arm's `theta` carries
/// real ints; the not_run/unavailable arms keep typed markers; a
/// `withheld{window_deadline}:protected_pair_truncated` reason never
/// appears (k=5/R=5 executed).
pub fn validate_w4_results_packet(j: &Json) -> Result<(), Vec<String>> {
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
        Some(Json::Obj(s)) if s.contains_key("basis") && s.contains_key("w4_window") => {}
        _ => errs.push("supersession: basis + w4_window members required".into()),
    }
    let expected = format!(
        "docs/build/reports/2026-11-19-fp-results-{}-frozen.json",
        j.get("claim")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_lowercase()
    );
    if j.get("supersedes").and_then(Json::as_str) != Some(expected.as_str()) {
        errs.push("supersedes must name the frozen R6.48 packet for the claim".into());
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
    // the retired refusal spelling never reappears — the protected
    // pair reached k=5/R=5 under ADR-0452.
    let text = j.to_canonical_string();
    if text.contains("protected_pair_truncated{k<5,R<5}") {
        errs.push("the truncation refusal never reappears in an executed-W4 packet".into());
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}
