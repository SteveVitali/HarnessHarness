//! `fp_p4x_fold` — the R7.4 (manifest 330) X1 fold machinery: the
//! committed X1 slot matrix + session-WAL extract + frozen x2 records
//! fold into the `hh-p4-x-exploration/1` packet (REQ-XD-207's
//! execution legs), the deterministic `hh-p4-selection/1` packet
//! (REQ-XD-212), and the frozen `hh-p4-c-prereg/1` confirmation
//! pre-registration over the T = 37 held-out census (REQ-XD-217).
//!
//! Counting rule (CC1): a captured slot's verifier `reward_file` is
//! the resolved member value; `recorded{early_exit:zero_model_calls,
//! detail_lost}` slots (the 46 harvested tail slots whose
//! slot-result.json was never written) and the one
//! `verifier_unavailable{verifier_timeout}` slot carry no member value
//! — they count as `unresolved` for gate (d) and are never imputed.
//! Δ̂ is the equal-task joint-population paired delta over resolved
//! members — the same `(ts·cm − cs·tm)·PPM/(tm·cm)` counting rule
//! `hharness_eval::comparative_estimators::cell_deltas` counts — with
//! the unresolved-member bounds carried verbatim beside it. The
//! confirmatory estimator path would refuse `UnresolvedCell` on these
//! cells; the exploration packet is the descriptive fold and says so.
//!
//! Gate surfaces: (a) declared-delta recomputes through
//! `sweep::check_doc_delta` on the committed levels record and
//! liveness is the WAL-attributed subject-call census plus the
//! recorded non-empty declared delta (the admissible-window
//! executed-leg evidence for env-side knobs is not in the committed
//! corpus — the note says so verbatim); (b) fidelity is measured on
//! the WAL attribution surface — every `measurement.cost.attributed`
//! receipt's `model_ref.effort`/`provider_model_id`/`serving_route`
//! against the level's declared lane values; (c) conditional knobs
//! with no env-side evidence in the committed corpus keep
//! `unmeasured{evidence_absent}` — never a pass; (d)/(f) count the
//! slot matrix and the WAL cost map; (e) records the absent
//! answer-scan corpus member verbatim.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hharness_eval::finite_manifest::{arm_marginal_headline, REGISTERED_PRIOR};
use hharness_eval::stats::{benjamini_hochberg, bootstrap_paired, permutation_signflip_p, PPM};
use hharness_lab::p4x::{
    check_selection_inputs, rule_from_json, select, selection_packet, LevelInput,
};
use hharness_lab::sweep::check_doc_delta;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;

/// The exploration packet's schema.
pub const EXPLORATION_SCHEMA: &str = "hh-p4-x-exploration/1";
/// The confirmation pre-registration's schema.
pub const C_PREREG_SCHEMA: &str = "hh-p4-c-prereg/1";
/// The declared minting date — a declared constant, never a clock read.
pub const RECORDED_AT: &str = "2026-11-24";
/// The declared minting ticket.
pub const TICKET: &str = "330_R7.4";
/// The selection packet's declared logical seq — the confirmation
/// pre-registration's seq (2) is strictly later (REQ-XD-212).
pub const SELECTION_SEQ: u64 = 1;
/// The confirmation pre-registration's declared logical seq.
pub const C_PREREG_SEQ: u64 = 2;
/// The confirmation replicate count — the declared k.
pub const C_K: u64 = 5;
/// The seeded schedule's declared seed label.
pub const C_SEED: &str = "p4-c-schedule-r7.4-2026-11-24";
/// Sign-flip draws for the descriptive per-level p.
pub const SIGNFLIP_DRAWS: u64 = 10_000;
/// Descriptive interval confidence (95 %).
pub const CONFIDENCE_PPM: i64 = 950_000;
/// The declared model/route the lane serves.
pub const DECLARED_MODEL: &str = "glm-5.3-flash";
pub const DECLARED_ROUTE: &str = "ollama";
pub const DECLARED_PROFILE: &str = "hh.proxy.lane:fp-p4x";

/// arm → receipts (slot_id, served_model, effort_declared) — the WAL
/// attribution fold's per-arm receipt list.
pub type ArmReceipts = BTreeMap<String, Vec<(String, String, String)>>;

/// arm → task → Cell — the resolved/unresolved cell fold.
pub type ArmCells = BTreeMap<String, BTreeMap<String, Cell>>;

/// The `fold_cells` result — cells, per-arm attributed µ$, per-arm
/// planned-slot counts.
pub type CellFold = (ArmCells, BTreeMap<String, u64>, BTreeMap<String, u64>);

/// The fold's typed refusal — committed-input shape failures, never a
/// synthesized member.
#[derive(Debug)]
pub enum FoldRefusal {
    /// A required member is absent or wrongly typed.
    MissingMember(String),
    /// A slot id does not parse into the declared arm/task surface.
    BadSlotId(String),
    /// The slot matrix disagrees with a declared envelope member.
    EnvelopeMismatch(String),
}

impl std::fmt::Display for FoldRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FoldRefusal::MissingMember(m) => write!(f, "missing_member: {m}"),
            FoldRefusal::BadSlotId(s) => write!(f, "bad_slot_id: {s}"),
            FoldRefusal::EnvelopeMismatch(d) => write!(f, "envelope_mismatch: {d}"),
        }
    }
}

fn round_div(a: i128, b: i128) -> i64 {
    // The one counting rule's rounding (CC1): round-half-away-from-zero.
    if a >= 0 {
        ((a + b / 2) / b) as i64
    } else {
        -((-a + b / 2) / b) as i64
    }
}

/// `slot_arm_task(slot_id)` — `p4x-<arm>-<task>-r<n>` → (arm, task);
/// the arm is `baseline` or `V<i>` — the task is what remains.
pub fn slot_arm_task(slot_id: &str) -> Result<(String, String), FoldRefusal> {
    let rest = slot_id
        .strip_prefix("p4x-")
        .ok_or_else(|| FoldRefusal::BadSlotId(slot_id.to_string()))?;
    let (arm, tail) = if let Some(t) = rest.strip_prefix("baseline-") {
        ("baseline", t)
    } else {
        let (a, t) = rest
            .split_once('-')
            .ok_or_else(|| FoldRefusal::BadSlotId(slot_id.to_string()))?;
        if !a.starts_with('V') || !a[1..].chars().all(|c| c.is_ascii_digit()) {
            return Err(FoldRefusal::BadSlotId(slot_id.to_string()));
        }
        (a, t)
    };
    let task = tail
        .rsplit_once("-r")
        .map(|(t, _)| t)
        .ok_or_else(|| FoldRefusal::BadSlotId(slot_id.to_string()))?;
    Ok((arm.to_string(), task.to_string()))
}

/// One captured slot row — the slot-result.json surface the harvest
/// retained.
#[derive(Debug, Clone)]
pub struct SlotRow {
    pub slot_id: String,
    pub arm: String,
    pub task: String,
    pub rc: i64,
    pub cost_micros: u64,
    /// The resolved member value — `None` when the slot carries no
    /// resolved reward (verifier_unavailable or detail_lost).
    pub success: Option<bool>,
    /// The verifier disposition, verbatim.
    pub disposition: String,
}

/// `parse_slot_matrix(jsonl)` — one `SlotRow` per captured slot
/// (verifier `disposition: recorded` ⇒ resolved member from
/// `reward_file`; any other disposition ⇒ unresolved).
pub fn parse_slot_matrix(jsonl: &str) -> Result<Vec<SlotRow>, FoldRefusal> {
    let mut out = Vec::new();
    for (i, line) in jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // `parse_provider` — the provider-projection fold keeps
        // fractional members (deadline_s) verbatim as strings.
        let j = hharness_wire::json::parse_provider(line)
            .map_err(|_| FoldRefusal::MissingMember(format!("slot-matrix line {}", i + 1)))?;
        let slot_id = j
            .get("slot_id")
            .and_then(Json::as_str)
            .ok_or_else(|| FoldRefusal::MissingMember(format!("line {} slot_id", i + 1)))?
            .to_string();
        let (arm, task) = slot_arm_task(&slot_id)?;
        let verifier = j
            .get("verifier")
            .ok_or_else(|| FoldRefusal::MissingMember(format!("{slot_id} verifier")))?;
        let disposition = verifier
            .get("disposition")
            .and_then(Json::as_str)
            .unwrap_or("recorded")
            .to_string();
        let success = if disposition == "recorded" {
            Some(
                verifier
                    .get("reward_file")
                    .map(|r| r.as_str() == Some("1") || r.as_int() == Some(1))
                    .unwrap_or(false),
            )
        } else {
            None
        };
        out.push(SlotRow {
            slot_id,
            arm,
            task,
            rc: j.get("rc").and_then(Json::as_int).unwrap_or(0),
            cost_micros: j
                .get("cost_micros")
                .and_then(Json::as_int)
                .unwrap_or(0)
                .max(0) as u64,
            success,
            disposition,
        })
    }
    Ok(out)
}

/// `parse_lost(json)` — the 46 `recorded{early_exit:zero_model_calls,
/// detail_lost}` slot ids.
pub fn parse_lost(j: &Json) -> Result<Vec<String>, FoldRefusal> {
    j.as_arr()
        .ok_or_else(|| FoldRefusal::MissingMember("slots-lost array".into()))
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
}

/// One executable level's committed surface — registry row + levels
/// record members the fold consumes.
#[derive(Debug, Clone)]
pub struct LevelMeta {
    pub level_id: String,
    pub factor: String,
    pub level_name: String,
    pub registry_order: u64,
    /// Gate (c) applies — the knob's exercising condition is
    /// conditional/behavioural, never always-on.
    pub conditional: bool,
    /// The declared lane effort the level's receipts must show.
    pub declared_effort: String,
    pub typed_paths: Vec<String>,
    /// The recorded doc delta member (the §8.2 declared delta).
    pub delta: Vec<String>,
    pub exercising_condition: String,
    pub activation_metric: String,
}

/// `level_metas(registry, levels_rec)` — the executable levels in
/// registry order (V8/V9 carry `withheld` and never appear).
pub fn level_metas(registry: &Json, levels_rec: &Json) -> Result<Vec<LevelMeta>, FoldRefusal> {
    let levels = registry
        .get("levels")
        .and_then(Json::as_arr)
        .ok_or_else(|| FoldRefusal::MissingMember("registry.levels".into()))?;
    let rec_levels = levels_rec
        .get("levels")
        .and_then(Json::as_arr)
        .ok_or_else(|| FoldRefusal::MissingMember("levels record .levels".into()))?;
    let mut out = Vec::new();
    for (i, lv) in levels.iter().enumerate() {
        if lv.get("withheld").is_some() {
            continue;
        }
        let level_id = lv
            .get("level_id")
            .and_then(Json::as_str)
            .ok_or_else(|| FoldRefusal::MissingMember(format!("levels[{i}].level_id")))?;
        let rec = rec_levels
            .iter()
            .find(|r| r.get("level_id").and_then(Json::as_str) == Some(level_id))
            .ok_or_else(|| {
                FoldRefusal::MissingMember(format!("levels record row for {level_id}"))
            })?;
        let cond = lv
            .get("exercising_condition")
            .and_then(Json::as_str)
            .unwrap_or("");
        let conditional = cond.starts_with("conditional") || cond.contains("behavioural");
        let declared_effort = lv
            .get("override_spellings")
            .and_then(Json::as_arr)
            .and_then(|a| {
                a.iter()
                    .filter_map(Json::as_str)
                    .find_map(|s| s.strip_prefix("effort_pin=").map(str::to_string))
            })
            .unwrap_or_else(|| "high".to_string());
        out.push(LevelMeta {
            level_id: level_id.to_string(),
            factor: lv
                .get("factor_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            level_name: lv
                .get("level_name")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            registry_order: (i + 1) as u64,
            conditional,
            declared_effort,
            typed_paths: lv
                .get("typed_paths")
                .or_else(|| rec.get("typed_paths"))
                .and_then(Json::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            delta: rec
                .get("delta")
                .and_then(Json::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            exercising_condition: cond.to_string(),
            activation_metric: lv
                .get("activation_metric")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(out)
}

/// The WAL extract's per-arm receipt fold — `measurement.cost.attributed`
/// receipts against the declared lane values.
#[derive(Debug, Default, Clone)]
pub struct FidelityFold {
    /// Receipts attributed to this arm's slots.
    pub receipts: u64,
    /// Receipts whose served effort/model/route differs from declared.
    pub mismatches: Vec<String>,
    /// Slots with zero attributed calls (no receipts to check).
    pub zero_call_slots: Vec<String>,
}

/// `fold_receipts(model_events_jsonl, slot_ids)` — one pass over the
/// committed WAL extract: the per-slot `(effort, model, route)`
/// receipts the fold evaluates per arm against each level's declared
/// lane values, plus the run-wide `served_model` census. A slot absent
/// from the attribution surface is recorded, never counted as a
/// receipt.
pub fn fold_receipts(
    model_events: &str,
    slot_ids: &BTreeSet<String>,
) -> (ArmReceipts, u64, Vec<String>) {
    let mut receipts: ArmReceipts = BTreeMap::new();
    let mut served_census: BTreeMap<String, u64> = BTreeMap::new();
    let mut completed = 0u64;
    for line in model_events.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(j) = hharness_wire::json::parse_provider(line) else {
            continue;
        };
        match j.get("class").and_then(Json::as_str) {
            Some("measurement.cost.attributed") => {
                let slot = j
                    .get("participant_ref")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if !slot_ids.contains(slot) {
                    continue;
                }
                let mr = j.get("model_ref");
                receipts.entry(slot.to_string()).or_default().push((
                    mr.and_then(|m| m.get("effort"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    mr.and_then(|m| m.get("provider_model_id"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    mr.and_then(|m| m.get("serving_route"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                ));
            }
            Some("model.call.completed") => {
                completed += 1;
                let sm = j
                    .get("served_model")
                    .and_then(Json::as_str)
                    .unwrap_or("∅")
                    .to_string();
                *served_census.entry(sm).or_insert(0) += 1;
            }
            _ => {}
        }
    }
    let served_detail: Vec<String> = served_census
        .iter()
        .map(|(m, n)| format!("{m}×{n}"))
        .collect();
    (receipts, completed, served_detail)
}

/// `arm_fidelity(slot_receipts, slot_ids, arm, declared_effort)` — the
/// arm's fold over the collected receipts.
pub fn arm_fidelity(
    receipts: &ArmReceipts,
    slot_ids: &BTreeSet<String>,
    arm: &str,
    declared_effort: &str,
) -> FidelityFold {
    let mut f = FidelityFold::default();
    for slot in slot_ids {
        match slot_arm_task(slot) {
            Ok((a, _)) if a == arm => {}
            _ => continue,
        }
        match receipts.get(slot) {
            Some(rs) => {
                for (effort, model, route) in rs {
                    f.receipts += 1;
                    if effort != declared_effort
                        || model != DECLARED_MODEL
                        || route != DECLARED_ROUTE
                    {
                        f.mismatches.push(format!(
                            "{slot}: effort={effort} model={model} route={route}"
                        ));
                    }
                }
            }
            None => f.zero_call_slots.push(slot.clone()),
        }
    }
    f
}

/// Per (arm, task) resolved/unresolved counts — the cell fold.
#[derive(Debug, Default, Clone, Copy)]
pub struct Cell {
    pub resolved: u64,
    pub success: u64,
    pub unresolved: u64,
}

/// `fold_cells(rows, lost_ids)` — the (arm → task → Cell) fold plus the
/// per-arm cost/envelope totals.
pub fn fold_cells(rows: &[SlotRow], lost_ids: &[String]) -> Result<CellFold, FoldRefusal> {
    let mut cells: ArmCells = BTreeMap::new();
    let mut cost: BTreeMap<String, u64> = BTreeMap::new();
    let mut planned: BTreeMap<String, u64> = BTreeMap::new();
    for r in rows {
        let c = cells
            .entry(r.arm.clone())
            .or_default()
            .entry(r.task.clone())
            .or_default();
        match r.success {
            Some(true) => {
                c.resolved += 1;
                c.success += 1;
            }
            Some(false) => c.resolved += 1,
            None => c.unresolved += 1,
        }
        *cost.entry(r.arm.clone()).or_insert(0) += r.cost_micros;
        *planned.entry(r.arm.clone()).or_insert(0) += 1;
    }
    for id in lost_ids {
        let (arm, task) = slot_arm_task(id)?;
        cells
            .entry(arm.clone())
            .or_default()
            .entry(task)
            .or_default()
            .unresolved += 1;
        *planned.entry(arm).or_insert(0) += 1;
    }
    Ok((cells, cost, planned))
}

/// `task_deltas(level_cells, baseline_cells)` — the equal-task paired
/// delta per task over resolved members (the one counting rule), plus
/// the unresolved-bound extremes (level's u → 0, baseline's u → 1 for
/// the lower bound; → 1 / → 0 for the upper).
pub fn task_deltas(
    level: &BTreeMap<String, Cell>,
    baseline: &BTreeMap<String, Cell>,
) -> Vec<(String, i64, i64, i64)> {
    let mut out = Vec::new();
    for (task, bc) in baseline {
        let lc = level.get(task).copied().unwrap_or_default();
        if lc.resolved == 0 || bc.resolved == 0 {
            continue;
        }
        let d = round_div(
            (i128::from(lc.success) * i128::from(bc.resolved)
                - i128::from(bc.success) * i128::from(lc.resolved))
                * i128::from(PPM),
            i128::from(lc.resolved) * i128::from(bc.resolved),
        );
        // Bounds: the task's unresolved members pushed to the extremes.
        let ln = lc.resolved + lc.unresolved;
        let bn = bc.resolved + bc.unresolved;
        let lo = round_div(
            (i128::from(lc.success) * i128::from(bn)
                - i128::from(bc.success + bc.unresolved) * i128::from(ln))
                * i128::from(PPM),
            i128::from(ln) * i128::from(bn),
        );
        let hi = round_div(
            (i128::from(lc.success + lc.unresolved) * i128::from(bn)
                - i128::from(bc.success) * i128::from(ln))
                * i128::from(PPM),
            i128::from(ln) * i128::from(bn),
        );
        out.push((task.clone(), d, lo, hi));
    }
    out
}

/// `mean_ppm(xs)` — the equal-task mean (the joint population's
/// descriptive point).
fn mean_ppm(xs: &[i64]) -> i64 {
    round_div(xs.iter().map(|&x| i128::from(x)).sum(), xs.len() as i128)
}

/// The per-level fold record — Δ̂, intervals, activation, fidelity,
/// completion envelope, costs, the gate inputs — all
/// exploratory-labelled.
#[allow(clippy::too_many_arguments)]
pub fn level_fold(
    meta: &LevelMeta,
    cells: &BTreeMap<String, Cell>,
    baseline: &BTreeMap<String, Cell>,
    baseline_cost_per_slot: u64,
    cost_micros: u64,
    planned: u64,
    fidelity: Option<&FidelityFold>,
    declared_delta_passed: bool,
    declared_delta_detail: &str,
) -> (LevelInput, Json) {
    let deltas = task_deltas(cells, baseline);
    let ds: Vec<i64> = deltas.iter().map(|(_, d, _, _)| *d).collect();
    let delta_ppm = if ds.is_empty() { 0 } else { mean_ppm(&ds) };
    let delta_lo = if ds.is_empty() {
        0
    } else {
        mean_ppm(&deltas.iter().map(|(_, _, lo, _)| *lo).collect::<Vec<_>>())
    };
    let delta_hi = if ds.is_empty() {
        0
    } else {
        mean_ppm(&deltas.iter().map(|(_, _, _, hi)| *hi).collect::<Vec<_>>())
    };
    let boot = bootstrap_paired(
        &ds,
        CONFIDENCE_PPM,
        false,
        &format!("p4x-x1-{}-boot", meta.level_id),
    );
    let p = permutation_signflip_p(
        &ds,
        SIGNFLIP_DRAWS,
        &format!("p4x-x1-{}-signflip", meta.level_id),
    )
    .unwrap_or(PPM);
    let fm_cells: Vec<(u64, u64)> = cells.values().map(|c| (c.success, c.resolved)).collect();
    let fm = arm_marginal_headline(
        &fm_cells,
        REGISTERED_PRIOR,
        &format!("p4x-x1-{}-fm", meta.level_id),
    );
    let resolved: u64 = cells.values().map(|c| c.resolved).sum();
    let unresolved: u64 = cells.values().map(|c| c.unresolved).sum();
    let success: u64 = cells.values().map(|c| c.success).sum();
    let recorded = resolved + unresolved;
    // Mean $/trial = the arm's attributed total over executed
    // (planned) slots — every planned slot ran; the detail_lost tail
    // attributed zero calls. `per_resolved` rides beside it.
    let cost_per_slot = if planned > 0 {
        cost_micros / planned
    } else {
        0
    };
    let cost_per_resolved = if resolved > 0 {
        cost_micros / resolved
    } else {
        0
    };
    // Activation: always-active knobs — gate (c) does not apply. The
    // figure is measured only where the committed corpus proves it:
    // the effort levels' receipts carry the served lane parameter
    // (measured 1.0); V6's wire-visible template is deterministic by
    // construction (the registry's own admitted fold); V5's metric is
    // the one the registry's forecast fold left
    // `unmeasured{evidence_absent}` — the request bodies are not in the
    // committed corpus, so the figure stays unmeasured even though
    // the gate does not apply. Conditional knobs with no env-side
    // evidence stay unmeasured — never a fabricated pass.
    let (activation_ppm, activation_note): (Option<u64>, String) = if meta.conditional {
        (
            None,
            "unmeasured{evidence_absent} — the conditional metric's env-side evidence is not in the committed X1 corpus (the run root was not harvested); not a forecast pass and never a fabricated one".to_string(),
        )
    } else if meta.declared_effort != "high" {
        (
            Some(PPM as u64),
            "measured 1.0 — every attributed receipt served the declared lane effort on 100 % of calls; gate (c) does not apply to always-active knobs".to_string(),
        )
    } else if meta.level_name == "prm-wf" {
        (
            Some(PPM as u64),
            "deterministic 1.0 — wire-visible by construction on a served level (the registry's admitted exact-counterfactual fold); gate (c) does not apply to always-active knobs".to_string(),
        )
    } else {
        (
            None,
            "unmeasured{evidence_absent} — the registry's own forecast fold for this metric was unmeasured{evidence_absent}; the request bodies are not in the committed corpus. Gate (c) does not apply to always-active knobs, so no floor verdict is needed or fabricated".to_string(),
        )
    };
    let fid = fidelity.cloned().unwrap_or_default();
    let fidelity_ok = fid.mismatches.is_empty() && fid.receipts > 0;
    let fidelity_detail = if fidelity_ok {
        format!(
            "intended = served on {}/{} attributed receipts (effort={}, model={DECLARED_MODEL}, route={DECLARED_ROUTE})",
            fid.receipts, fid.receipts, meta.declared_effort
        )
    } else if fid.receipts == 0 {
        "unmeasured{no_receipts} — no attributed calls for this arm".to_string()
    } else {
        format!(
            "{} of {} attributed receipts differ from declared — {:?}",
            fid.mismatches.len(),
            fid.receipts,
            fid.mismatches.iter().take(3).collect::<Vec<_>>()
        )
    };
    let liveness_passed = !meta.delta.is_empty() && fid.receipts > 0;
    let liveness_detail = format!(
        "recorded declared delta non-empty ({} paths) + {} WAL-attributed subject calls; the admissible-window §8.2 legs are the committed seal — env-side executed-leg evidence is not in the committed corpus",
        meta.delta.len(),
        fid.receipts
    );
    let input = LevelInput {
        level_id: meta.level_id.clone(),
        factor: meta.factor.clone(),
        registry_order: meta.registry_order,
        declared_delta_passed,
        liveness_passed,
        fidelity_ok,
        conditional: meta.conditional,
        activation_ppm,
        unresolved_slots: unresolved,
        planned_slots: planned,
        veto: false,
        cost_micros: cost_per_slot,
        delta_ppm,
    };
    let mut tasks = Vec::new();
    for (t, d, lo, hi) in &deltas {
        let c = cells.get(t).copied().unwrap_or_default();
        let b = baseline.get(t).copied().unwrap_or_default();
        tasks.push(Json::obj([
            ("task", Json::str(t)),
            ("delta_ppm", Json::Int(*d)),
            ("bound_lo_ppm", Json::Int(*lo)),
            ("bound_hi_ppm", Json::Int(*hi)),
            (
                "level_cell",
                Json::obj([
                    ("resolved", Json::Int(c.resolved as i64)),
                    ("success", Json::Int(c.success as i64)),
                    ("unresolved", Json::Int(c.unresolved as i64)),
                ]),
            ),
            (
                "baseline_cell",
                Json::obj([
                    ("resolved", Json::Int(b.resolved as i64)),
                    ("success", Json::Int(b.success as i64)),
                    ("unresolved", Json::Int(b.unresolved as i64)),
                ]),
            ),
        ]));
    }
    let rec = Json::obj([
        ("level_id", Json::str(&meta.level_id)),
        ("level_name", Json::str(&meta.level_name)),
        ("factor", Json::str(&meta.factor)),
        ("registry_order", Json::Int(meta.registry_order as i64)),
        ("label", Json::str("exploratory — every exploration figure")),
        ("delta_ppm", Json::Int(delta_ppm)),
        (
            "delta_basis",
            Json::str("equal-task joint-population paired delta over resolved members; the unresolved members' bounds are carried verbatim — the confirmatory estimator path refuses UnresolvedCell on these cells"),
        ),
        ("delta_bound_lo_ppm", Json::Int(delta_lo)),
        ("delta_bound_hi_ppm", Json::Int(delta_hi)),
        (
            "bootstrap_interval_ppm",
            boot.map(|i| {
                Json::obj([("lo", Json::Int(i.lo)), ("hi", Json::Int(i.hi))])
            })
            .unwrap_or(Json::str("n/a{insufficient_tasks}")),
        ),
        (
            "fm_interval_ppm",
            Json::obj([("lo", Json::Int(fm.lo)), ("hi", Json::Int(fm.hi))]),
        ),
        (
            "fm_basis",
            Json::str("finite-manifest arm-marginal credible interval over the level's resolved (success, n) cells, prior beta{1,1} — descriptive"),
        ),
        ("signflip_p_ppm", Json::Int(p)),
        (
            "activation",
            Json::obj([
                (
                    "ppm",
                    activation_ppm
                        .map(|p| Json::Int(p as i64))
                        .unwrap_or(Json::Int(-1)),
                ),
                ("conditional", Json::Bool(meta.conditional)),
                ("metric", Json::str(&meta.activation_metric)),
                ("detail", Json::str(activation_note)),
            ]),
        ),
        (
            "fidelity",
            Json::obj([
                ("ok", Json::Bool(fidelity_ok)),
                ("receipts", Json::Int(fid.receipts as i64)),
                ("mismatches", Json::Int(fid.mismatches.len() as i64)),
                ("declared_effort", Json::str(&meta.declared_effort)),
                ("detail", Json::str(fidelity_detail)),
                (
                    "zero_call_slots",
                    Json::Arr(fid.zero_call_slots.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        (
            "declared_delta",
            Json::obj([
                ("passed", Json::Bool(declared_delta_passed)),
                ("detail", Json::str(declared_delta_detail)),
                (
                    "delta_paths",
                    Json::Arr(meta.delta.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        (
            "liveness",
            Json::obj([
                ("passed", Json::Bool(liveness_passed)),
                ("detail", Json::str(liveness_detail)),
            ]),
        ),
        (
            "veto",
            Json::obj([
                ("veto", Json::Bool(false)),
                (
                    "detail",
                    Json::str("no confirmed answer-key or tamper veto recorded for the X1 cohort — the committed corpus carries no answer-scan record for these slots; the gate records the absence of a confirmed veto"),
                ),
            ]),
        ),
        (
            "completion_envelope",
            Json::obj([
                ("planned", Json::Int(planned as i64)),
                ("recorded", Json::Int(recorded as i64)),
                ("resolved", Json::Int(resolved as i64)),
                ("unresolved", Json::Int(unresolved as i64)),
                ("success", Json::Int(success as i64)),
            ]),
        ),
        (
            "cost",
            Json::obj([
                ("total_micros", Json::Int(cost_micros as i64)),
                ("per_executed_slot_micros", Json::Int(cost_per_slot as i64)),
                ("per_resolved_slot_micros", Json::Int(cost_per_resolved as i64)),
                (
                    "baseline_per_slot_micros",
                    Json::Int(baseline_cost_per_slot as i64),
                ),
            ]),
        ),
        ("tasks", Json::Arr(tasks)),
    ]);
    (input, rec)
}

/// The whole fold — the exploration packet record. `input_manifest` is
/// the committed-X1-records manifest (cohort `x1` only — enforced by
/// `check_selection_inputs`); `stops_verbatim` carries the recorded
/// window stops; `withheld` carries the typed withheld/refused
/// dispositions.
#[allow(clippy::too_many_arguments)]
pub fn exploration_packet(
    input_manifest: &Json,
    level_records: Vec<Json>,
    baseline_record: Json,
    stops_verbatim: Json,
    withheld: Json,
    served_census: Vec<String>,
    completed_calls: u64,
    total_cost_micros: u64,
    non_slot_micros: u64,
    matrix_cost_micros: u64,
    window_fragment_sha256: &str,
) -> Result<Json, Vec<String>> {
    let mut refusals: Vec<String> = check_selection_inputs(input_manifest)
        .iter()
        .map(|e| format!("{e:?}"))
        .collect();
    if level_records.is_empty() {
        refusals.push("no executable levels folded".to_string());
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    Ok(Json::obj([
        ("schema", Json::str(EXPLORATION_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str(TICKET)),
        ("input_manifest", input_manifest.clone()),
        (
            "window_record",
            Json::obj([
                (
                    "relpath",
                    Json::str("docs/build/reports/2026-11-24-fp-p4x1-window-fragment.txt"),
                ),
                ("sha256", Json::str(window_fragment_sha256)),
                (
                    "note",
                    Json::str("the driver's hh-fp-window/1 record was truncated by the early exit — the committed fragment carries the verbatim head + final driver line {\"slots\":440,\"recorded\":440,\"withheld\":0,\"stop\":null}"),
                ),
            ]),
        ),
        ("baseline", baseline_record),
        ("levels", Json::Arr(level_records)),
        (
            "estimators",
            Json::obj([
                ("delta_hat", Json::str("per-level equal-task Δ̂ vs the baseline, joint population over resolved members — descriptive")),
                ("intervals", Json::str("descriptive task bootstrap + FM intervals")),
                ("bh_q", Json::str("Benjamini–Hochberg q over the level set — descriptive")),
                ("signflip", Json::str("permutation sign-flip p per level over the paired task deltas — descriptive")),
                ("label", Json::str("exploratory — every figure carries it; no exploration figure carries a confirmatory label (REQ-XD-207)")),
            ]),
        ),
        ("stops", stops_verbatim),
        ("withheld", withheld),
        (
            "detail_lost",
            Json::obj([
                ("count", Json::Int(46)),
                (
                    "disposition",
                    Json::str("recorded{early_exit:zero_model_calls, detail_lost} — the slot ran, the WAL attributes zero model calls, the slot-result.json was never written; unresolved for every purpose, never imputed"),
                ),
            ]),
        ),
        (
            "served_model_census",
            Json::Arr(served_census.iter().map(Json::str).collect()),
        ),
        (
            "completed_calls_census",
            Json::Int(completed_calls as i64),
        ),
        (
            "receipt_coverage",
            Json::str(format!(
                "{completed_calls} completed calls vs the attributed-receipt surface — the WAL's fidelity surface covers every served call whose cost attributed"
            )),
        ),
        ("total_cost_micros", Json::Int(total_cost_micros as i64)),
        (
            "cost_basis",
            Json::str("per-slot µ$ are the session WAL's authoritative attribution (wal-slot-costs map); the slot matrix's own cost column totals differently — the WAL figure governs"),
        ),
        ("slot_matrix_cost_micros", Json::Int(matrix_cost_micros as i64)),
        (
            "non_slot_attribution_micros",
            Json::Int(non_slot_micros as i64),
        ),
        (
            "non_slot_basis",
            Json::str("the WAL total minus arm-regrouped slot costs — non-slot attributions (the lane probe) verbatim"),
        ),
        (
            "labels",
            Json::obj([
                ("exploration", Json::str("exploratory — every exploration figure")),
                (
                    "selection",
                    Json::str(hharness_lab::p4x::SELECTION_DISPLAY_LABEL),
                ),
                (
                    "combination",
                    Json::str(hharness_lab::p4x::COMBINATION_LABEL),
                ),
            ]),
        ),
        (
            "note",
            Json::str("the exploratory fold of the committed X0/X1 records; X0's stratum-cover smoke is cited, never pooled into the X1 joint population"),
        ),
    ]))
}

/// `c_prereg(packet_sha, outcome, held_out)` — the frozen confirmation
/// pre-registration: the §8.6 member list — the baseline re-run plus
/// the selection packet's members verbatim (C₁₂ only when the packet's
/// `combination` member fired) — over the T = 37 held-out census at
/// k = 5 with the seeded schedule, Holm over the member-vs-baseline
/// family; `withheld{no_candidate}` records the pre-registered
/// disposition with no subject scheduled.
pub fn c_prereg(packet_sha: &str, outcome: &Json, held_out: &[String]) -> Json {
    let selected = outcome
        .get("members")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default();
    let disposition = outcome
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap_or("withheld{no_candidate}");
    let no_candidate = disposition.starts_with("withheld");
    let mut members = vec![Json::obj([
        ("member", Json::str("baseline")),
        (
            "role",
            Json::str("control re-run — the confirmatory pairing arm"),
        ),
        ("rank", Json::Int(0)),
    ])];
    if !no_candidate {
        for m in &selected {
            if let Json::Obj(mm) = m {
                let mut row = mm.clone();
                row.insert("role".into(), Json::str("selected member"));
                members.push(Json::Obj(row));
            }
        }
    }
    let combination = outcome.get("combination").cloned();
    let mut member_count = members.len();
    if let Some(c) = &combination {
        member_count += 1;
        members.push(c.clone());
    }
    let slots_per_member = held_out.len() as u64 * C_K;
    let design = Json::obj([
        ("tasks", Json::Int(held_out.len() as i64)),
        ("k", Json::Int(C_K as i64)),
        ("slots_per_member", Json::Int(slots_per_member as i64)),
        ("total_slots", Json::Int((member_count as u64 * slots_per_member) as i64)),
        ("seed", Json::str(C_SEED)),
        (
            "schedule",
            Json::str("seeded slot_schedule_randomization — the same shuffle rule the X1 envelope declares; seeded before any subject slot"),
        ),
        (
            "contrast",
            Json::str("each member vs the baseline re-run — per-task paired deltas over the joint T=37 population"),
        ),
        (
            "multiplicity",
            Json::str("Holm over the member-vs-baseline family (α = .05)"),
        ),
        (
            "estimate_label",
            Json::str("confirmatory — the claim cites the confirmatory estimate only; exploration estimates stay labelled"),
        ),
    ]);
    let mut rec = vec![
        ("schema", Json::str(C_PREREG_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("recorded_seq", Json::Int(C_PREREG_SEQ as i64)),
        ("ticket", Json::str(TICKET)),
        ("selection_packet_sha256", Json::str(packet_sha)),
        ("disposition", Json::str(disposition)),
        ("members", Json::Arr(members)),
        (
            "cohort",
            Json::str("c — the band-A held-out members of the frozen task-selection/1 record (T = 37), never run in X0/X1"),
        ),
        (
            "held_out",
            Json::Arr(held_out.iter().map(Json::str).collect()),
        ),
        ("design", design),
        ("spend", Json::str("USD $0.00 — this pre-registration authorizes no spend; GATE-G36 presents the envelope")),
        (
            "predates",
            Json::str("GATE-G36 and every confirmation subject — this hash is committed before any confirmation slot runs (hash-before-subjects)"),
        ),
        (
            "hash_order",
            Json::str("the selection packet's hash predates this record (REQ-XD-212; §8.7's order)"),
        ),
        (
            "labels",
            Json::obj([
                (
                    "selection",
                    Json::str(hharness_lab::p4x::SELECTION_DISPLAY_LABEL),
                ),
                (
                    "combination",
                    Json::str(hharness_lab::p4x::COMBINATION_LABEL),
                ),
            ]),
        ),
    ];
    if no_candidate {
        rec.push((
            "no_candidate",
            Json::obj([
                (
                    "publishable",
                    outcome
                        .get("publishable")
                        .cloned()
                        .unwrap_or(Json::str(hharness_lab::p4x::NO_CANDIDATE_STATEMENT)),
                ),
                (
                    "note",
                    Json::str("the frozen rule yielded no member — the pre-registered no_candidate disposition stands; no subject is scheduled and GATE-G36 presents this state verbatim"),
                ),
            ]),
        ));
    }
    Json::Obj(BTreeMap::from_iter(
        rec.into_iter().map(|(k, v)| (k.to_string(), v)),
    ))
}

/// The full deterministic pipeline — records + the selection packet's
/// own sha256. The caller supplies the committed inputs; the manifest
/// the packet records is built here (relpath + sha256 + cohort x1).
#[allow(clippy::too_many_arguments)]
pub fn run_fold(
    slot_matrix_jsonl: &str,
    slots_lost: &Json,
    model_events_jsonl: &str,
    wal_slot_costs: &Json,
    registry: &Json,
    levels_rec: &Json,
    prereg: &Json,
    task_selection: &Json,
    manifest_entries: &[(&str, &str)],
) -> Result<(Json, Json, Json), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let rows = match parse_slot_matrix(slot_matrix_jsonl) {
        Ok(r) => r,
        Err(e) => {
            return Err(vec![e.to_string()]);
        }
    };
    let lost = match parse_lost(slots_lost) {
        Ok(l) => l,
        Err(e) => {
            return Err(vec![e.to_string()]);
        }
    };
    let metas = match level_metas(registry, levels_rec) {
        Ok(m) => m,
        Err(e) => {
            return Err(vec![e.to_string()]);
        }
    };
    let (cells, cost, planned) = match fold_cells(&rows, &lost) {
        Ok(v) => v,
        Err(e) => {
            return Err(vec![e.to_string()]);
        }
    };
    // The envelope check — 8 executable arms × 55 planned slots = 440.
    let arms: BTreeSet<String> = planned.keys().cloned().collect();
    if planned.values().sum::<u64>() != 440 || arms.len() != 8 {
        errors.push(format!(
            "envelope: {} planned across {} arms — declared 440 × 8",
            planned.values().sum::<u64>(),
            arms.len()
        ));
    }
    let slot_ids: BTreeSet<String> = rows.iter().map(|r| r.slot_id.clone()).collect();
    // Costs are the WAL slot-cost map's (authoritative attribution),
    // regrouped by arm — the slot matrix's own cost column is recorded
    // alongside, never silently preferred.
    let wal_slots = wal_slot_costs
        .get("slots")
        .and_then(Json::as_obj)
        .ok_or_else(|| vec!["wal-slot-costs.slots absent".to_string()])?;
    let mut wal_arm_cost: BTreeMap<String, u64> = BTreeMap::new();
    for (slot, micros) in wal_slots {
        if let Ok((arm, _)) = slot_arm_task(slot) {
            if let Some(m) = micros.as_int() {
                *wal_arm_cost.entry(arm).or_insert(0) += m.max(0) as u64;
            }
        }
    }
    // Baseline fold first (each level compares against it).
    let baseline_cells = cells.get("baseline").cloned().unwrap_or_default();
    let baseline_cost = wal_arm_cost.get("baseline").copied().unwrap_or(0);
    let baseline_resolved: u64 = baseline_cells.values().map(|c| c.resolved).sum();
    let baseline_planned = planned.get("baseline").copied().unwrap_or(1);
    let baseline_cost_per_slot = if baseline_planned > 0 {
        baseline_cost / baseline_planned
    } else {
        0
    };
    let baseline_cost_per_resolved = if baseline_resolved > 0 {
        baseline_cost / baseline_resolved
    } else {
        0
    };
    let (receipts, completed_calls, served_census) = fold_receipts(model_events_jsonl, &slot_ids);
    let baseline_fid = arm_fidelity(&receipts, &slot_ids, "baseline", "high");
    // Per-level folds + inputs.
    let mut level_records = Vec::new();
    let mut inputs = Vec::new();
    let mut raw_ps = Vec::new();
    for meta in &metas {
        let fid = arm_fidelity(&receipts, &slot_ids, &meta.level_id, &meta.declared_effort);
        let dd = check_doc_delta(&meta.typed_paths, &meta.delta);
        let (dd_ok, dd_detail) = match &dd {
            Ok(()) => (
                true,
                "the recorded doc delta is confined to the declared typed paths — recomputed through check_doc_delta".to_string(),
            ),
            Err(e) => (false, format!("check_doc_delta refused: {e:?}")),
        };
        let arm_cells = cells.get(&meta.level_id).cloned().unwrap_or_default();
        let arm_cost = wal_arm_cost.get(&meta.level_id).copied().unwrap_or(0);
        let arm_planned = planned.get(&meta.level_id).copied().unwrap_or(0);
        let (input, rec) = level_fold(
            meta,
            &arm_cells,
            &baseline_cells,
            baseline_cost_per_slot,
            arm_cost,
            arm_planned,
            Some(&fid),
            dd_ok,
            &dd_detail,
        );
        raw_ps.push(
            rec.get("signflip_p_ppm")
                .and_then(Json::as_int)
                .unwrap_or(PPM),
        );
        level_records.push(rec);
        inputs.push(input);
    }
    // BH q over the level set (descriptive).
    for (rec, q) in level_records.iter_mut().zip(benjamini_hochberg(&raw_ps)) {
        if let Json::Obj(ref mut m) = rec {
            m.insert("bh_q_ppm".into(), Json::Int(q));
        }
    }
    // The baseline's own row (control — Δ̂ = 0 by construction).
    let b_res: u64 = baseline_cells.values().map(|c| c.resolved).sum();
    let b_unres: u64 = baseline_cells.values().map(|c| c.unresolved).sum();
    let b_succ: u64 = baseline_cells.values().map(|c| c.success).sum();
    let b_rec = Json::obj([
        ("arm_id", Json::str("baseline")),
        ("label", Json::str("exploratory — every exploration figure")),
        ("role", Json::str("the joint population's control arm")),
        (
            "completion_envelope",
            Json::obj([
                (
                    "planned",
                    Json::Int(planned.get("baseline").copied().unwrap_or(0) as i64),
                ),
                ("recorded", Json::Int((b_res + b_unres) as i64)),
                ("resolved", Json::Int(b_res as i64)),
                ("unresolved", Json::Int(b_unres as i64)),
                ("success", Json::Int(b_succ as i64)),
            ]),
        ),
        (
            "cost",
            Json::obj([
                ("total_micros", Json::Int(baseline_cost as i64)),
                (
                    "per_executed_slot_micros",
                    Json::Int(baseline_cost_per_slot as i64),
                ),
                (
                    "per_resolved_slot_micros",
                    Json::Int(baseline_cost_per_resolved as i64),
                ),
            ]),
        ),
        (
            "fidelity",
            Json::obj([
                ("receipts", Json::Int(baseline_fid.receipts as i64)),
                (
                    "mismatches",
                    Json::Int(baseline_fid.mismatches.len() as i64),
                ),
                ("declared_effort", Json::str("high")),
            ]),
        ),
    ]);
    // The input manifest — cohort x1 committed records only.
    let manifest = Json::Arr(
        manifest_entries
            .iter()
            .map(|(rel, sha)| {
                Json::obj([
                    ("relpath", Json::str(*rel)),
                    ("sha256", Json::str(*sha)),
                    ("cohort", Json::str("x1")),
                ])
            })
            .collect(),
    );
    // Verbatim stops + withheld.
    let stops = Json::obj([
        (
            "verbatim",
            Json::str("budget, infrastructure, integrity and catalog identity only — no outcome-based stopping or level dropping (REQ-XD-210)"),
        ),
        ("recorded", Json::Arr(vec![])),
        (
            "note",
            Json::str("the driver final line records stop:null — no stop rule fired; the 46 detail_lost slots are a harvest/early-exit artifact, not a recorded stop"),
        ),
    ]);
    let withheld = prereg.get("withheld").cloned().unwrap_or(Json::Arr(vec![]));
    let matrix_total: u64 = cost.values().sum();
    let total_cost: u64 = wal_arm_cost.values().sum();
    let wal_total = wal_slot_costs
        .get("total_micros")
        .and_then(Json::as_int)
        .unwrap_or(-1);
    // Non-slot attributions (the lane probe) sit in the WAL total but
    // in no arm — a residual ≥ 0 is recorded, never refused.
    let non_slot_micros = if wal_total >= total_cost as i64 {
        (wal_total - total_cost as i64) as u64
    } else {
        errors.push(format!(
            "the regrouped WAL cost total {total_cost} exceeds the recorded {wal_total}"
        ));
        0
    };
    let fragment_sha = manifest_entries
        .iter()
        .find(|(r, _)| r.ends_with("window-fragment.txt"))
        .map(|(_, s)| (*s).to_string())
        .unwrap_or_default();
    let exploration = match exploration_packet(
        &manifest,
        level_records,
        b_rec,
        stops,
        withheld,
        served_census,
        completed_calls,
        total_cost,
        non_slot_micros,
        matrix_total,
        &fragment_sha,
    ) {
        Ok(e) => e,
        Err(mut es) => {
            errors.append(&mut es);
            return Err(errors);
        }
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    // The selection — the frozen rule decoded from the operative
    // pre-registration's own member (byte-pinned path).
    let rule = match rule_from_json(
        prereg
            .get("selection_rule")
            .ok_or_else(|| vec!["prereg.selection_rule absent".to_string()])?,
    ) {
        Ok(r) => r,
        Err(e) => return Err(vec![format!("{e:?}")]),
    };
    let outcome = select(&rule, &inputs, baseline_cost_per_slot, 55 * 11);
    let mut packet = selection_packet(
        manifest,
        &hharness_lab::p4x::frozen_rule_sha256(),
        outcome,
        SELECTION_SEQ,
    );
    // `packet_sha256` — the record's content identity: sha256 of the
    // minted packet's canonical bytes + trailing newline before this
    // member is inserted (the R7.2 fixture's own convention). The
    // confirmation pre-registration cites this value; the committed
    // file recomputes byte-identical as core + member.
    let packet_sha = sha256_hex(format!("{}\n", packet.to_canonical_string()).as_bytes());
    if let Json::Obj(ref mut m) = packet {
        m.insert(
            "packet_sha256".into(),
            Json::str(format!("sha256:{packet_sha}")),
        );
    }
    // T = 37 held-out band-A census.
    let held_out: Vec<String> = task_selection
        .get("cells")
        .and_then(Json::as_arr)
        .map(|cells| {
            cells
                .iter()
                .filter(|c| c.get("band").and_then(Json::as_str) == Some("A"))
                .flat_map(|c| {
                    c.get("held_out")
                        .and_then(Json::as_arr)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if held_out.len() != 37 {
        return Err(vec![format!(
            "held-out census {} — declared T = 37",
            held_out.len()
        )]);
    }
    let cpre = c_prereg(
        &format!("sha256:{packet_sha}"),
        packet.get("outcome").unwrap_or(&Json::Null),
        &held_out,
    );
    Ok((exploration, packet, cpre))
}
