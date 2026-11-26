//! `fp_p4c_fold` — the R7.6 (manifest 333) confirmation fold: the
//! committed P4-C records (the frozen `hh-p4-c-prereg/1`, the
//! `hh-p4-selection/1` packet it cites, and the executed
//! `hh-fp-window/1` r4 record whose `slot_results` member embeds the
//! 370-slot matrix verbatim) fold into the `hh-p4-c-confirmation/1`
//! packet — REQ-XD-215's fold evidence, REQ-XD-216's Holm-family
//! estimation, REQ-XD-219's typed dispositions, and the §8.8
//! two-statement claims surface (REQ-XD-222/REQ-XD-224 contributing).
//!
//! Counting rule (CC1 — the same rule the X1 fold declares): a slot's
//! verifier `reward_file` is the resolved member value
//! (`disposition: "recorded"` with a present reward). A slot whose
//! verifier carried no grade — `verifier_unavailable{…}` or a recorded
//! envelope with no reward — stays typed unresolved, never imputed.
//! The confirmatory estimator path (`comparative_estimand`,
//! `finite_manifest_contrast`, `confirmatory_family` under the GATE-G23
//! admission) refuses `UnresolvedCell` on those cells; the refusals
//! stand verbatim in the packet beside the all-planned bounds and the
//! labelled conditional secondaries. An unavailable member stays in
//! the Holm family at worst-case mass — never dropped (REQ-XD-216).
//!
//! Disposition ladder (REQ-XD-219): `confirmed` only when the member's
//! confirmatory estimate exists, its interval excludes 0 and the
//! Holm-adjusted p rejects at α = .05; the null template
//! "not confirmed; interval includes 0" when the executed estimate
//! covers 0; a typed `inconclusive{…}` when the confirmatory estimator
//! refused; `withheld{…}` verbatim when the window or selection
//! withheld; `unmet{not_executed}` when no confirmation ran. Equality
//! and equivalence wording is never emitted — `check_claims_text`
//! refuses it (AC-2's mutation battery exercises the guard).

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators::{
    comparative_estimand, confirmatory_family, EstimandDecl, FamilyDeclC,
    LABEL_CONDITIONAL_SECONDARY, PRIMARY_METRIC,
};
use hharness_eval::comparative_protocol::{
    randomization_digest, RandomizationRecord, ScheduleEvidence, EXCHANGEABILITY, SCHEDULE_KIND,
};
use hharness_eval::finite_manifest::{
    finite_manifest_contrast, ContrastDecl, ContrastError, LEVEL_PPM, REGISTERED_PRIOR,
    REGISTERED_PRIOR_NAME,
};
use hharness_eval::population::{
    assemble, Availability, BoundsDecl, PlannedSlot, PopulationInput, PopulationKind, SlotSubject,
};
use hharness_eval::stats::{bootstrap_paired, Interval, PPM};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::fp_w4::{g23_admission, not_computed, refused};
use crate::json_ext::JsonExt;

/// The confirmation packet's schema.
pub const CONFIRMATION_SCHEMA: &str = "hh-p4-c-confirmation/1";
/// The ticket that mints it.
pub const TICKET: &str = "333_R7.6";
/// The record date — the window's close date.
pub const RECORDED_AT: &str = "2026-11-26";
/// The frozen primary family id (the prereg's `P4-C1`).
pub const FAMILY_ID: &str = "P4-C1";
/// The exploratory family id (member-pair BH family — empty at m = 1).
pub const EXPLORATORY_FAMILY_ID: &str = "P4-C1-exploratory";
/// The declared 95% level.
pub const CONFIDENCE_PPM: i64 = 950_000;
/// Holm α = .05.
pub const ALPHA_PPM: i64 = 50_000;
/// The declared fold seed the family's sign-flip material mixes.
pub const FOLD_SEED: u64 = 20_261_126;
/// The held-out stratum/split spellings the population slots carry.
pub const STRATUM: &str = "c";
/// The split label.
pub const SPLIT: &str = "held_out";

/// The null template — verbatim, pre-registered (REQ-XD-219).
pub const NULL_DISPOSITION: &str = "not confirmed; interval includes 0";
/// The confirmed template lead.
pub const CONFIRMED_DISPOSITION: &str = "confirmed";
/// The unexecuted disposition.
pub const UNEXECUTED_DISPOSITION: &str = "unmet{not_executed}";

/// A fold failure — a required member is absent or wrongly typed.
#[derive(Debug)]
pub enum FoldRefusal {
    /// A required record member is absent or mistyped.
    MissingMember(String),
    /// A slot id does not parse into the declared arm/task surface.
    BadSlotId(String),
    /// A record disagrees with a declared envelope member.
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

impl std::error::Error for FoldRefusal {}

fn round_div(a: i128, b: i128) -> i64 {
    if a >= 0 {
        ((a + b / 2) / b) as i64
    } else {
        -((-a + b / 2) / b) as i64
    }
}

/// `p4c_arm_task(slot_id)` — `p4c-<arm>-<task>-r<n>` → (arm, task).
pub fn p4c_arm_task(slot_id: &str) -> Result<(String, String), FoldRefusal> {
    let rest = slot_id
        .strip_prefix("p4c-")
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

/// One slot row read off the committed window record's `slot_results`.
#[derive(Debug, Clone)]
pub struct SlotRow {
    /// The slot id.
    pub slot_id: String,
    /// The arm.
    pub arm: String,
    /// The task.
    pub task: String,
    /// The replicate index.
    pub replicate: u64,
    /// The subject exit code.
    pub rc: i64,
    /// The slot's recorded cost (µ$).
    pub cost_micros: i64,
    /// Wall time (ms).
    pub wall_ms: i64,
    /// The verifier disposition, verbatim.
    pub disposition: String,
    /// The resolved member value — `None` when the slot carries no
    /// resolved reward (never imputed).
    pub success: Option<bool>,
    /// Why the slot is unresolved — `None` on a resolved member.
    pub unresolved_class: Option<String>,
}

/// `slot_rows(window)` — one `SlotRow` per `slot_results` entry. The
/// counting rule: `disposition: "recorded"` with a present
/// `reward_file` resolves the member ("1" = success); every other
/// disposition or a recorded envelope without a reward stays typed
/// unresolved, never imputed.
pub fn slot_rows(window: &Json) -> Result<Vec<SlotRow>, FoldRefusal> {
    let mut out = Vec::new();
    let results = match window.get("slot_results") {
        Some(Json::Obj(o)) => o,
        _ => {
            return Err(FoldRefusal::MissingMember(
                "window.slot_results object".into(),
            ))
        }
    };
    for (slot_id, s) in results {
        let (arm, task) = p4c_arm_task(slot_id)?;
        let verifier = s
            .get("verifier")
            .ok_or_else(|| FoldRefusal::MissingMember(format!("{slot_id}.verifier")))?;
        let disposition = verifier
            .get("disposition")
            .and_then(Json::as_str)
            .unwrap_or("recorded")
            .to_string();
        let reward = verifier.get("reward_file");
        let (success, unresolved_class) = if disposition == "recorded" && reward.is_some() {
            (
                Some(
                    reward
                        .and_then(|r| r.as_str().map(|s| s == "1").or(r.as_int().map(|i| i == 1)))
                        .unwrap_or(false),
                ),
                None,
            )
        } else {
            let cls = if disposition == "recorded" {
                "recorded{reward_absent}".to_string()
            } else {
                disposition.clone()
            };
            (None, Some(cls))
        };
        out.push(SlotRow {
            slot_id: slot_id.clone(),
            arm,
            task,
            replicate: s.get("replicate").and_then(Json::as_int).unwrap_or(0) as u64,
            rc: s.get("rc").and_then(Json::as_int).unwrap_or(0),
            cost_micros: s.get("cost_micros").and_then(Json::as_int).unwrap_or(0),
            wall_ms: s.get("wall_ms").and_then(Json::as_int).unwrap_or(0),
            disposition,
            success,
            unresolved_class,
        });
    }
    out.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));
    Ok(out)
}

/// The scheduled slot order — the committed `schedule_rows` members in
/// emitted order (the seeded shuffle's output order, recorded before
/// any slot ran).
pub fn slot_order(window: &Json) -> Result<Vec<String>, FoldRefusal> {
    let rows = window
        .get("schedule_rows")
        .and_then(Json::as_arr)
        .ok_or_else(|| FoldRefusal::MissingMember("window.schedule_rows".into()))?;
    let mut out = Vec::new();
    for row in rows {
        let members = row
            .get("members")
            .and_then(Json::as_arr)
            .ok_or_else(|| FoldRefusal::MissingMember("schedule_rows[].members".into()))?;
        for m in members {
            out.push(
                m.as_str()
                    .ok_or_else(|| FoldRefusal::MissingMember("schedule_rows member id".into()))?
                    .to_string(),
            );
        }
    }
    Ok(out)
}

/// The exchangeability evidence the family's sign-flip declares: the
/// `k_round_interleave` record over the committed seed + emitted slot
/// order (its digest recomputes from the committed `schedule_rows` —
/// the recorded-before-execution shuffle material, REQ-XD-021).
pub fn schedule_evidence(window: &Json) -> Result<ScheduleEvidence, FoldRefusal> {
    let seed = window
        .get("seed")
        .and_then(Json::as_str)
        .ok_or_else(|| FoldRefusal::MissingMember("window.seed".into()))?
        .to_string();
    let rounds = window
        .get("schedule_rows")
        .and_then(Json::as_arr)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("round").and_then(Json::as_int))
                .max()
                .unwrap_or(0) as u32
        })
        .unwrap_or(0);
    let order = slot_order(window)?;
    let mut record = RandomizationRecord {
        kind: SCHEDULE_KIND.to_string(),
        seed,
        rounds,
        exchangeability: EXCHANGEABILITY.to_string(),
        digest: String::new(),
    };
    record.digest = randomization_digest(&record, &order);
    Ok(ScheduleEvidence {
        record,
        slot_order: order,
    })
}

/// `assemble_population(rows)` — the reconciled `PlannedPopulation`
/// through `population::assemble` (the one counting rule — never a
/// reimplementation). A resolved slot binds a `scored` subject with
/// its recorded success; an unresolved slot binds an `oracle_failure`
/// subject — the verifier carried no grade, never scored, never
/// coerced.
pub fn assemble_population(
    rows: &[SlotRow],
) -> Result<hharness_eval::population::PlannedPopulation, FoldRefusal> {
    let mut slots = Vec::new();
    let mut required = BTreeSet::new();
    for r in rows {
        let (outcome, availability) = match r.success {
            Some(_) => (Some(OutcomeClass::Scored), Availability::Resolved),
            None => (Some(OutcomeClass::OracleFailure), Availability::Unavailable),
        };
        required.insert(r.slot_id.clone());
        slots.push(PlannedSlot {
            plan_ref: r.slot_id.clone(),
            task_id: r.task.clone(),
            split_label: SPLIT.into(),
            stratum: STRATUM.into(),
            arm_id: r.arm.clone(),
            replicate_index: r.replicate,
            subjects: vec![SlotSubject {
                subject_run: format!("{}-a1", r.slot_id),
                attempt_no: 1,
                outcome,
                success: r.success,
                veto_tripped: vec![],
                availability,
                seq: 0,
                missing_required_checks: false,
                admissible: true,
            }],
        });
    }
    assemble(PopulationInput {
        slots,
        required,
        pins: BTreeMap::new(),
    })
    .map_err(|e| FoldRefusal::EnvelopeMismatch(format!("assemble: {e}")))
}

/// One member declared by the frozen prereg.
#[derive(Debug, Clone)]
pub struct MemberDecl {
    /// The member id.
    pub member: String,
    /// The rank.
    pub rank: i64,
    /// The role verbatim.
    pub role: String,
    /// The factor verbatim (selected members).
    pub factor: Option<String>,
    /// The declared exploration Δ̂ (ppm) + its label (selected members).
    pub delta_ppm: Option<i64>,
    /// The exploration label verbatim.
    pub exploration_label: Option<String>,
}

/// `prereg_members(prereg)` — the prereg's member list verbatim;
/// `members[0]` is the reference arm.
pub fn prereg_members(prereg: &Json) -> Result<Vec<MemberDecl>, FoldRefusal> {
    let arr = prereg
        .get("members")
        .and_then(Json::as_arr)
        .ok_or_else(|| FoldRefusal::MissingMember("prereg.members".into()))?;
    let mut out = Vec::new();
    for m in arr {
        out.push(MemberDecl {
            member: m
                .get("member")
                .and_then(Json::as_str)
                .ok_or_else(|| FoldRefusal::MissingMember("prereg.members[].member".into()))?
                .to_string(),
            rank: m.get("rank").and_then(Json::as_int).unwrap_or(0),
            role: m
                .get("role")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            factor: m.get("factor").and_then(Json::as_str).map(str::to_string),
            delta_ppm: m.get("delta_ppm").and_then(Json::as_int),
            exploration_label: m
                .get("exploration_label")
                .and_then(Json::as_str)
                .map(str::to_string),
        });
    }
    out.sort_by_key(|m| m.rank);
    Ok(out)
}

/// The window's terminal disposition — `recorded` when every planned
/// slot carries a slot_result; else the typed stop/withheld state.
fn window_terminal(window: &Json, planned: i64) -> Json {
    let recorded = window
        .get("slot_results")
        .and_then(|s| s.as_obj().map(|o| o.len() as i64))
        .unwrap_or(0);
    if recorded >= planned && planned > 0 {
        return Json::str("recorded");
    }
    if let Some(w) = window.get("withheld").and_then(Json::as_arr) {
        if !w.is_empty() {
            let reason = w
                .iter()
                .filter_map(|x| x.get("disposition").and_then(Json::as_str))
                .next()
                .unwrap_or("withheld{unclassified}");
            return Json::obj([
                ("kind", Json::str("withheld")),
                ("reason", Json::str(reason)),
            ]);
        }
    }
    let stop = window
        .get("checkpoint")
        .and_then(|c| c.get("stop"))
        .and_then(Json::as_str)
        .unwrap_or("stop{unclassified}");
    Json::obj([
        ("kind", Json::str("stopped")),
        ("reason", Json::str(stop)),
        ("recorded", Json::Int(recorded)),
        ("planned", Json::Int(planned)),
    ])
}

/// Per-task paired deltas over the eligible tasks (both cells fully
/// resolved) plus the excluded-task list — the labelled conditional
/// secondary the confirmatory estimators publish beside a refused
/// primary (the same `(ts·cm − cs·tm)·PPM/(tm·cm)` counting rule
/// `cell_deltas` counts; a cell carrying an unresolved member is
/// excluded and named, never silently dropped).
fn eligible_deltas(
    rows: &[SlotRow],
    test: &str,
    control: &str,
) -> (Vec<(String, i64)>, Vec<String>) {
    let mut cells: BTreeMap<String, [u64; 6]> = BTreeMap::new(); // task -> [tm,ts,tu,cm,cs,cu]
    for r in rows {
        let e = cells.entry(r.task.clone()).or_insert([0; 6]);
        let off = if r.arm == test {
            0
        } else if r.arm == control {
            3
        } else {
            continue;
        };
        e[off] += 1;
        match r.success {
            Some(true) => e[off + 1] += 1,
            Some(false) => {}
            None => e[off + 2] += 1,
        }
    }
    let mut deltas = Vec::new();
    let mut excluded = Vec::new();
    for (t, [tm, ts, tu, cm, cs, cu]) in cells {
        if tu > 0 || cu > 0 {
            excluded.push(t);
            continue;
        }
        deltas.push((
            t,
            round_div(
                (i128::from(ts) * i128::from(cm) - i128::from(cs) * i128::from(tm))
                    * i128::from(PPM),
                i128::from(tm) * i128::from(cm),
            ),
        ));
    }
    (deltas, excluded)
}

/// `check_claims_text(text)` — the disposition/claim wording guard the
/// packet runs on its own emitted strings: equality and equivalence
/// wording is refused, never emitted (REQ-XD-219/AC-2). The mutation
/// battery drives the same guard.
pub fn check_claims_text(text: &str) -> Result<(), String> {
    let t = text.to_lowercase();
    for phrase in [
        "equivalent",
        "equal to the baseline",
        "equal to baseline",
        "no different",
        "statistically tied",
        "same as the baseline",
        "non-inferior",
        "noninferior",
        "parity with",
        "no worse",
        "indistinguishable",
        "tied with",
    ] {
        if t.contains(phrase) {
            return Err(format!(
                "equality/equivalence wording refused on a confirmation claim: {phrase:?} in {text:?}"
            ));
        }
    }
    Ok(())
}

/// `check_disposition(text)` — a disposition spelling is one of the
/// pre-registered forms; equality/equivalence wording refuses.
pub fn check_disposition(text: &str) -> Result<(), String> {
    check_claims_text(text)?;
    let ok = text == NULL_DISPOSITION
        || text == CONFIRMED_DISPOSITION
        || text == UNEXECUTED_DISPOSITION
        || text.starts_with("inconclusive{")
        || text.starts_with("withheld{")
        || text.starts_with("unmet{");
    if !ok {
        return Err(format!(
            "disposition is not a pre-registered form: {text:?}"
        ));
    }
    Ok(())
}

fn interval_json(i: &Interval, kind: &str) -> Json {
    Json::obj([
        ("lo_ppm", Json::Int(i.lo)),
        ("hi_ppm", Json::Int(i.hi)),
        ("kind", Json::str(kind)),
        ("level_ppm", Json::Int(CONFIDENCE_PPM)),
    ])
}

/// The member's disposition — the pre-registered ladder:
/// `confirmed` (interval excludes 0 ∧ adjusted p < α); the null
/// template when the executed estimate covers 0; typed
/// `inconclusive{…}` when the confirmatory estimator refused.
fn member_disposition(
    family_status: &str,
    unavailable_reason: &str,
    fm_interval: Option<&Interval>,
    bootstrap_interval: Option<&Interval>,
    rejected_at_alpha: bool,
) -> String {
    let confirmed = fm_interval.map(|i| i.lo > 0 || i.hi < 0).unwrap_or(false) && rejected_at_alpha;
    if confirmed {
        return CONFIRMED_DISPOSITION.to_string();
    }
    if family_status == "unavailable" || fm_interval.is_none() {
        let cls = if unavailable_reason.starts_with("UnresolvedCell") {
            "unresolved_cell"
        } else {
            "unavailable"
        };
        return format!("inconclusive{{{cls}}}");
    }
    let covers_zero = fm_interval.map(|i| i.lo <= 0 && i.hi >= 0).unwrap_or(false)
        && bootstrap_interval
            .map(|i| i.lo <= 0 && i.hi >= 0)
            .unwrap_or(true);
    if covers_zero && !rejected_at_alpha {
        return NULL_DISPOSITION.to_string();
    }
    "inconclusive{interval_excludes_zero_without_rejection}".to_string()
}

/// `run_fold_c(prereg, selection, window, input_manifest)` — the
/// confirmation fold: every estimator consumes the committed records
/// through the one counting rule; every refusal/typed state stands.
/// `window` may be `None` (no confirmation ran — `unmet{not_executed}`).
pub fn run_fold_c(
    prereg: &Json,
    selection: Option<&Json>,
    window: Option<&Json>,
    earlier_windows: &[(&str, &Json)],
    input_manifest: &Json,
) -> Result<Json, FoldRefusal> {
    let members = prereg_members(prereg)?;
    let planned = prereg
        .get("design")
        .and_then(|d| d.get("total_slots"))
        .and_then(Json::as_int)
        .unwrap_or(0);
    let tasks = prereg
        .get("design")
        .and_then(|d| d.get("tasks"))
        .and_then(Json::as_int)
        .unwrap_or(0);
    let k = prereg
        .get("design")
        .and_then(|d| d.get("k"))
        .and_then(Json::as_int)
        .unwrap_or(0);
    let schedule_seed = prereg
        .get("design")
        .and_then(|d| d.get("seed"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let reference = members
        .first()
        .map(|m| m.member.clone())
        .unwrap_or_else(|| "baseline".to_string());
    let test_members: Vec<&MemberDecl> = members.iter().skip(1).collect();
    let arms: Vec<String> = members.iter().map(|m| m.member.clone()).collect();

    // ── the terminal-state leg — an unexecuted confirmation is unmet ──
    if window.is_none() {
        let withheld = prereg
            .get("disposition")
            .and_then(Json::as_str)
            .unwrap_or("");
        let disposition = if withheld.starts_with("withheld{") || withheld.starts_with("unmet{") {
            withheld.to_string()
        } else {
            UNEXECUTED_DISPOSITION.to_string()
        };
        check_disposition(&disposition).map_err(FoldRefusal::EnvelopeMismatch)?;
        return Ok(Json::obj([
            ("schema", Json::str(CONFIRMATION_SCHEMA)),
            ("ticket", Json::str(TICKET)),
            ("recorded_at", Json::str(RECORDED_AT)),
            ("packet_disposition", Json::str(&disposition)),
            ("prereg_disposition", Json::str(withheld)),
            (
                "note",
                Json::str(
                    "an unexecuted confirmation is unmet, never null — the prereg's \
                 typed disposition stands verbatim (REQ-XD-219)",
                ),
            ),
            ("input_manifest", input_manifest.clone()),
        ]));
    }
    let window = window.unwrap();

    // ── fold the slot matrix ─────────────────────────────────────────
    let rows = slot_rows(window)?;
    let pop = assemble_population(&rows)?;
    let ev = schedule_evidence(window)?;

    let resolved: Vec<&SlotRow> = rows.iter().filter(|r| r.success.is_some()).collect();
    let unresolved: Vec<&SlotRow> = rows.iter().filter(|r| r.success.is_none()).collect();

    // ── estimand — per-arm θ̂, completion envelopes, the contrast ─────
    let est = comparative_estimand(
        &pop,
        &EstimandDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            arms: arms.clone(),
            reference_arm: reference.clone(),
            stratum: None,
            use_as: "confirmatory".into(),
            admission: Some(g23_admission()),
        },
    )
    .map_err(|e| FoldRefusal::EnvelopeMismatch(format!("comparative_estimand: {e}")))?;

    // ── the family — Δ(h−ref) + sign-flip + Holm, unavailable kept ───
    let fam = confirmatory_family(
        &pop,
        &FamilyDeclC {
            tranche_id: "p4-c".into(),
            family_id: FAMILY_ID.into(),
            exploratory_id: EXPLORATORY_FAMILY_ID.into(),
            arms: arms.clone(),
            reference_arm: reference.clone(),
            metric: PRIMARY_METRIC.into(),
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            stratum: None,
            exchangeability: EXCHANGEABILITY.into(),
            schedule: Some(ev.clone()),
            seed: FOLD_SEED,
            use_as: "confirmatory".into(),
            admission: Some(g23_admission()),
        },
    )
    .map_err(|e| FoldRefusal::EnvelopeMismatch(format!("confirmatory_family: {e}")))?;

    // ── per-member estimate legs ─────────────────────────────────────
    let mut member_rows = Vec::new();
    let mut dispositions = Vec::new();
    for m in &test_members {
        let (eligible, excluded) = eligible_deltas(&rows, &m.member, &reference);
        // FM credible interval — the admitted estimator; a typed
        // refusal stands verbatim when a cell is unresolved.
        let fm = finite_manifest_contrast(
            &pop,
            &ContrastDecl {
                population: PopulationKind::JointTaskPassWithoutObservedVeto,
                test_arm: m.member.clone(),
                control_arm: reference.clone(),
                stratum: None,
                prior: REGISTERED_PRIOR,
                prior_name: REGISTERED_PRIOR_NAME.to_string(),
                dependence: None,
                target: None,
                use_as: Some("confirmatory".to_string()),
                bounds: BoundsDecl::Unit,
                level_ppm: LEVEL_PPM,
                seed: FOLD_SEED,
            },
        );
        let fm_interval = fm.as_ref().ok().map(|c| c.interval);
        let fm_json = match &fm {
            Ok(c) => interval_json(&c.interval, "credible"),
            Err(ContrastError::UnresolvedCell { task_id, arm_id, u }) => refused(&format!(
                "unresolved_cell: cell ({task_id}, {arm_id}) carries {u} unresolved member(s) — never imputed"
            )),
            Err(e) => refused(&format!("{e:?}")),
        };
        // Task bootstrap over the eligible tasks — the labelled
        // conditional secondary (3 unresolved-cell tasks excluded,
        // named verbatim).
        let delta_vec: Vec<i64> = eligible.iter().map(|(_, d)| *d).collect();
        let boot = bootstrap_paired(
            &delta_vec,
            CONFIDENCE_PPM,
            false,
            &format!("fp-p4c-fold|{}|{reference}", m.member),
        );
        let boot_json = match &boot {
            Some(i) => Json::obj([
                ("lo_ppm", Json::Int(i.lo)),
                ("hi_ppm", Json::Int(i.hi)),
                ("kind", Json::str("task_bootstrap")),
                ("level_ppm", Json::Int(CONFIDENCE_PPM)),
                ("label", Json::str(LABEL_CONDITIONAL_SECONDARY)),
                ("eligible_tasks", Json::Int(delta_vec.len() as i64)),
                (
                    "excluded_tasks",
                    Json::Arr(excluded.iter().map(Json::str).collect()),
                ),
            ]),
            None => not_computed("task bootstrap undefined over the eligible-task set"),
        };
        // The contrast row — Δ̂, all-planned bound, refusal verbatim.
        let ce = est.contrasts.iter().find(|c| c.test_arm == m.member);
        let (delta_json, bound_json) = match ce {
            Some(c) => {
                let d = match c.delta_ppm {
                    Some(d) => Json::Int(d),
                    None => refused(c.refusal.as_deref().unwrap_or("unavailable{contrast}")),
                };
                (
                    d,
                    Json::obj([
                        ("lo_ppm", Json::Int(c.bounds.lo)),
                        ("hi_ppm", Json::Int(c.bounds.hi)),
                        ("kind", Json::str("paired_cell_all_planned_bound")),
                    ]),
                )
            }
            None => (
                refused("unavailable{contrast_row_absent}"),
                refused("unavailable{contrast_row_absent}"),
            ),
        };
        // The family row — sign-flip p + Holm-adjusted, unavailable
        // retained verbatim.
        let fr = fam.family.1.iter().find(|r| r.member == m.member);
        let (fam_status, fam_unavailable, raw_p, adj_p, rejected, mode) = match fr {
            Some(r) => (
                r.status.clone(),
                r.unavailable.clone(),
                r.raw_p_ppm,
                r.adjusted_ppm,
                r.rejected_at_alpha,
                r.mode.clone(),
            ),
            None => (
                "unavailable".into(),
                Some("family row absent".into()),
                None,
                PPM,
                false,
                None,
            ),
        };
        let disposition = member_disposition(
            &fam_status,
            fam_unavailable.as_deref().unwrap_or(""),
            fm_interval.as_ref(),
            boot.as_ref(),
            rejected,
        );
        check_disposition(&disposition).map_err(FoldRefusal::EnvelopeMismatch)?;
        member_rows.push(Json::obj([
            ("member", Json::str(&m.member)),
            (
                "factor",
                m.factor.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
            ("delta_ppm", delta_json),
            ("bound_ppm", bound_json),
            ("fm_interval", fm_json),
            ("task_bootstrap_interval", boot_json),
            (
                "raw_p_ppm",
                raw_p.map(Json::Int).unwrap_or_else(|| {
                    refused(
                        &fam_unavailable
                            .clone()
                            .unwrap_or_else(|| "unavailable".into()),
                    )
                }),
            ),
            ("adjusted_p_ppm", Json::Int(adj_p)),
            ("holm_rejected_at_alpha", Json::Bool(rejected)),
            (
                "signflip_mode",
                mode.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
            ("status", Json::str(&fam_status)),
            (
                "unavailable",
                fam_unavailable
                    .as_ref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("disposition", Json::str(&disposition)),
        ]));
        dispositions.push(Json::obj([
            ("member", Json::str(&m.member)),
            ("disposition", Json::str(&disposition)),
        ]));
    }

    // ── per-arm rows (θ̂, envelopes, unresolved tasks named) ──────────
    let mut arm_rows = Vec::new();
    for a in &est.arms {
        arm_rows.push(Json::obj([
            ("arm", Json::str(&a.arm_id)),
            (
                "theta_ppm",
                a.theta_ppm.map(Json::Int).unwrap_or_else(|| {
                    refused(
                        "unresolved_cell: the equal-task rate needs the executed manifest complete",
                    )
                }),
            ),
            (
                "unresolved_tasks",
                Json::Arr(a.unresolved_tasks.iter().map(Json::str).collect()),
            ),
            (
                "bound_ppm",
                Json::obj([
                    ("lo_ppm", Json::Int(a.bounds.lo)),
                    ("hi_ppm", Json::Int(a.bounds.hi)),
                    ("kind", Json::str("all_planned_bound")),
                ]),
            ),
            (
                "completion_envelope",
                Json::obj([
                    ("planned", Json::Int(a.completion.0 as i64)),
                    ("first", Json::Int(a.completion.1 as i64)),
                    ("recovered", Json::Int(a.completion.2 as i64)),
                    ("unresolved", Json::Int(a.completion.3 as i64)),
                ]),
            ),
            (
                "conditional_secondary",
                a.conditional
                    .as_ref()
                    .map(|c| {
                        Json::obj([
                            ("label", Json::str(&c.label)),
                            ("value_ppm", Json::Int(c.value_ppm)),
                            ("tasks", Json::Int(c.tasks.len() as i64)),
                            (
                                "excluded_tasks",
                                Json::Arr(c.excluded_tasks.iter().map(Json::str).collect()),
                            ),
                        ])
                    })
                    .unwrap_or(Json::Null),
            ),
        ]));
    }

    // ── cost — descriptive only (REQ-XD-224; no admitted method) ─────
    let mut cost_rows = Vec::new();
    for a in &arms {
        let arm_rows2: Vec<&SlotRow> = rows.iter().filter(|r| &r.arm == a).collect();
        let total: i64 = arm_rows2.iter().map(|r| r.cost_micros).sum();
        let n_exec = arm_rows2.len() as i64;
        let n_res = arm_rows2.iter().filter(|r| r.success.is_some()).count() as i64;
        cost_rows.push(Json::obj([
            ("arm", Json::str(a)),
            ("total_cost_micros", Json::Int(total)),
            ("slots", Json::Int(n_exec)),
            ("resolved_slots", Json::Int(n_res)),
            (
                "per_executed_slot_micros",
                Json::Int(if n_exec > 0 { total / n_exec } else { 0 }),
            ),
            (
                "per_resolved_slot_micros",
                Json::Int(if n_res > 0 { total / n_res } else { 0 }),
            ),
        ]));
    }

    // ── the §8.8 claims surface — the two-statement form ─────────────
    let confirmed = dispositions
        .iter()
        .any(|d| d.get("disposition").and_then(Json::as_str) == Some(CONFIRMED_DISPOSITION));
    let expl_estimates: Vec<Json> = test_members
        .iter()
        .map(|m| {
            Json::obj([
                ("member", Json::str(&m.member)),
                (
                    "delta_ppm",
                    m.delta_ppm.map(Json::Int).unwrap_or(Json::Null),
                ),
                (
                    "label",
                    m.exploration_label
                        .as_ref()
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
                (
                    "factor",
                    m.factor.as_ref().map(Json::str).unwrap_or(Json::Null),
                ),
            ])
        })
        .collect();
    let expl_members: Vec<String> = test_members
        .iter()
        .map(|m| {
            format!(
                "{} ({:+.4}, {})",
                m.member,
                m.delta_ppm.unwrap_or(0) as f64 / 1_000_000.0,
                m.factor.clone().unwrap_or_default()
            )
        })
        .collect();
    let exploratory_statement = format!(
        "In exploration on 11 development tasks, the level(s) with the highest estimated \
         contrast vs the baseline were {}. These estimates are descriptive, selected from \
         the same data, and expected to be optimistic (expected optimism \u{2248} +.03 to +.10 \
         by design simulation).",
        expl_members.join(" and ")
    );
    let member_frag: Vec<String> = dispositions
        .iter()
        .map(|d| {
            format!(
                "{} — {}",
                d.get("member").and_then(Json::as_str).unwrap_or(""),
                d.get("disposition").and_then(Json::as_str).unwrap_or("")
            )
        })
        .collect();
    let confirmatory_statement = if confirmed {
        format!(
            "Confirmed on {} held-out tasks (family {FAMILY_ID}, Holm \u{3b1} = .05).",
            tasks
        )
    } else {
        format!(
            "Not confirmed on {tasks} held-out tasks: no selected member was confirmed \
             different from the baseline at Holm \u{3b1} = .05 (family {FAMILY_ID}). {}",
            member_frag.join("; ")
        )
    };
    check_claims_text(&exploratory_statement).map_err(FoldRefusal::EnvelopeMismatch)?;
    check_claims_text(&confirmatory_statement).map_err(FoldRefusal::EnvelopeMismatch)?;

    let claims = Json::obj([
        ("form", Json::str("two-statement (§8.8)")),
        ("statement_exploratory", Json::str(&exploratory_statement)),
        ("statement_confirmatory", Json::str(&confirmatory_statement)),
        (
            "exploratory_estimates",
            Json::obj([
                ("members", Json::Arr(expl_estimates)),
                (
                    "label",
                    Json::str("selection-labelled — exploratory estimates never stand as claims (REQ-XD-213)"),
                ),
            ]),
        ),
        (
            "labels",
            Json::obj([
                (
                    "baseline_label",
                    Json::str("baseline — the in-window baseline re-run (REQ-XD-215's fresh baseline; never L2/phase-2 reuse)"),
                ),
                (
                    "finite_manifest_scope",
                    Json::str("finite-manifest: these 37 TB 2.1 held-out tasks"),
                ),
                (
                    "ofat_caveat",
                    Json::str("one-factor-at-a-time: effects are local to the baseline configuration; interactions not estimated"),
                ),
                (
                    "catalog_window",
                    Json::str("catalog window: the frozen task-selection/1 band-A census (T = 37), the P4-C window of 2026-11-25/26"),
                ),
                (
                    "w_window",
                    Json::str("not_applicable{no_compaction_members} — no member sweeps a W-token context window"),
                ),
            ]),
        ),
        (
            "cost",
            Json::obj([
                ("rows", Json::Arr(cost_rows.clone())),
                (
                    "label",
                    Json::str("descriptive — per-level cost/$/solve/tokens/calls carry no confirmatory claim; no admitted method (Q-X44; REQ-XD-224)"),
                ),
                ("basis", Json::str("slot_results cost_micros verbatim — the window record's per-slot attribution")),
            ]),
        ),
    ]);

    // ── unresolved members — typed, verbatim, never imputed ──────────
    let unresolved_members: Vec<Json> = unresolved
        .iter()
        .map(|r| {
            Json::obj([
                ("slot_id", Json::str(&r.slot_id)),
                ("arm", Json::str(&r.arm)),
                ("task", Json::str(&r.task)),
                ("replicate", Json::Int(r.replicate as i64)),
                ("rc", Json::Int(r.rc)),
                ("disposition", Json::str(&r.disposition)),
                (
                    "unresolved_class",
                    Json::str(r.unresolved_class.as_deref().unwrap_or("unresolved")),
                ),
            ])
        })
        .collect();
    let rc_census: BTreeMap<String, i64> = {
        let mut c = BTreeMap::new();
        for r in &rows {
            *c.entry(format!("{}:{}", r.arm, r.rc)).or_insert(0) += 1;
        }
        c
    };

    // ── the terminal-state record (CAP7.1's input) ───────────────────
    let earlier: Vec<Json> = earlier_windows
        .iter()
        .map(|(rel, w)| {
            Json::obj([
                ("relpath", Json::str(*rel)),
                (
                    "run_id",
                    w.get("run_id")
                        .and_then(Json::as_str)
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
                (
                    "terminal",
                    if w.get("native_lane_hold").and_then(|v| v.as_bool()) == Some(true) {
                        Json::str("native_lane_hold")
                    } else {
                        w.get("checkpoint")
                            .and_then(|c| c.get("stop"))
                            .and_then(Json::as_str)
                            .map(Json::str)
                            .unwrap_or(Json::Null)
                    },
                ),
                (
                    "slots_recorded",
                    w.get("slot_results")
                        .and_then(|s| s.as_obj().map(|o| o.len() as i64))
                        .map(Json::Int)
                        .unwrap_or(Json::Int(0)),
                ),
                (
                    "slots_withheld",
                    w.get("withheld")
                        .and_then(|s| s.as_arr().map(|a| a.len() as i64))
                        .map(Json::Int)
                        .unwrap_or(Json::Int(0)),
                ),
            ])
        })
        .collect();
    let terminal_state = Json::obj([
        (
            "window_id",
            window
                .get("window")
                .and_then(Json::as_str)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "run_id",
            window
                .get("run_id")
                .and_then(Json::as_str)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        ("terminal_disposition", window_terminal(window, planned)),
        ("slots_planned", Json::Int(planned)),
        ("slots_recorded", Json::Int(rows.len() as i64)),
        (
            "lane_probe",
            window
                .get("lane_probe")
                .and_then(|p| p.get("disposition"))
                .and_then(Json::as_str)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        ("earlier_attempts", Json::Arr(earlier)),
    ]);

    // ── the packet ────────────────────────────────────────────────────
    let mut census = Vec::new();
    for (k_, v) in &rc_census {
        census.push(Json::obj([
            ("class", Json::str(k_)),
            ("count", Json::Int(*v)),
        ]));
    }
    let family_members: Vec<Json> = fam
        .family
        .1
        .iter()
        .map(|r| {
            Json::obj([
                ("member", Json::str(&r.member)),
                ("status", Json::str(&r.status)),
                (
                    "unavailable",
                    r.unavailable.as_ref().map(Json::str).unwrap_or(Json::Null),
                ),
                (
                    "delta_ppm",
                    r.delta_ppm.map(Json::Int).unwrap_or(Json::Null),
                ),
                (
                    "raw_p_ppm",
                    r.raw_p_ppm.map(Json::Int).unwrap_or(Json::Null),
                ),
                ("adjusted_p_ppm", Json::Int(r.adjusted_ppm)),
                ("rejected_at_alpha", Json::Bool(r.rejected_at_alpha)),
                ("mode", r.mode.as_ref().map(Json::str).unwrap_or(Json::Null)),
            ])
        })
        .collect();
    let exploratory_members: Vec<Json> = fam
        .exploratory
        .1
        .iter()
        .map(|r| {
            Json::obj([
                ("member", Json::str(&r.member)),
                ("status", Json::str(&r.status)),
                ("adjusted_p_ppm", Json::Int(r.adjusted_ppm)),
            ])
        })
        .collect();
    let unresolved_task_list: BTreeSet<String> =
        unresolved.iter().map(|r| r.task.clone()).collect();

    let packet = Json::obj([
        ("schema", Json::str(CONFIRMATION_SCHEMA)),
        ("ticket", Json::str(TICKET)),
        ("recorded_at", Json::str(RECORDED_AT)),
        (
            "packet_disposition",
            Json::str(if confirmed {
                "executed — a member confirmed different from the baseline at Holm α = .05"
            } else {
                "executed — no selected member was confirmed different from the baseline at Holm α = .05 on 37 held-out tasks"
            }),
        ),
        ("determinism", Json::str(
            "pure function of the committed P4-C records — byte-identical recompute (same inputs, same packet)",
        )),
        ("input_manifest", input_manifest.clone()),
        ("counting_rule", Json::str(
            "a slot's verifier reward_file is the resolved member value (disposition recorded + reward present); \
             every other disposition or a recorded envelope without a reward stays typed unresolved — never imputed (the X1 rule verbatim)",
        )),
        (
            "prereg",
            Json::obj([
                ("schema", Json::str("hh-p4-c-prereg/1")),
                ("disposition", prereg.get("disposition").cloned().unwrap_or(Json::Null)),
                ("selection_packet_sha256", prereg.get("selection_packet_sha256").cloned().unwrap_or(Json::Null)),
                ("held_out_tasks", Json::Int(tasks)),
                ("k", Json::Int(k)),
                ("seed", Json::str(&schedule_seed)),
            ]),
        ),
        ("terminal_state", terminal_state),
        (
            "population",
            Json::obj([
                ("kind", Json::str("joint_task_pass_without_observed_veto")),
                ("tasks", Json::Int(pop.n_tasks as i64)),
                ("slots", Json::Int(pop.n_slots as i64)),
                ("resolved", Json::Int(resolved.len() as i64)),
                ("unresolved", Json::Int(unresolved.len() as i64)),
                ("complete", Json::Bool(unresolved.is_empty())),
                (
                    "unresolved_members",
                    Json::Arr(unresolved_members),
                ),
                (
                    "unresolved_tasks",
                    Json::Arr(unresolved_task_list.iter().map(Json::str).collect()),
                ),
                ("rc_census", Json::Arr(census)),
            ]),
        ),
        ("estimand", Json::obj([
            ("arms", Json::Arr(arm_rows)),
            (
                "contrast",
                Json::obj([
                    ("direction", Json::str("test_minus_control")),
                    ("control_arm", Json::str(&reference)),
                    ("members", Json::Arr(member_rows)),
                ]),
            ),
            ("use_as", Json::str(&est.use_as)),
        ])),
        ("family", Json::obj([
            ("id", Json::str(FAMILY_ID)),
            ("method", Json::str("holm")),
            ("alpha_ppm", Json::Int(ALPHA_PPM)),
            ("members", Json::Arr(family_members)),
            ("labels", Json::Arr(fam.labels.iter().map(Json::str).collect())),
            (
                "note",
                Json::str("unavailable members stay in the family at worst-case mass — never silently dropped (REQ-XD-216)"),
            ),
        ])),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str(EXPLORATORY_FAMILY_ID)),
                ("method", Json::str("benjamini_hochberg")),
                ("members", Json::Arr(exploratory_members)),
            ]),
        ),
        ("dispositions", Json::Arr(dispositions)),
        ("claims", claims),
        (
            "exchangeability",
            Json::obj([
                ("kind", Json::str(&ev.record.kind)),
                ("seed", Json::str(&ev.record.seed)),
                ("rounds", Json::Int(ev.record.rounds as i64)),
                ("exchangeability", Json::str(&ev.record.exchangeability)),
                ("digest", Json::str(&ev.record.digest)),
                (
                    "slot_order_sha256",
                    Json::str(format!(
                        "sha256:{}",
                        sha256_hex(ev.slot_order.join("\n").as_bytes())
                    )),
                ),
                (
                    "note",
                    Json::str("the emitted slot order recomputes from the committed schedule_rows members in order; the digest seals that order — ScheduleEvidence::check passes inside confirmatory_family (REQ-XD-021)"),
                ),
            ]),
        ),
        (
            "selection_labelled_estimates",
            Json::obj([
                (
                    "members",
                    selection
                        .and_then(|s| s.get("outcome"))
                        .and_then(|o| o.get("members"))
                        .cloned()
                        .unwrap_or(Json::Arr(vec![])),
                ),
                (
                    "label",
                    Json::str("selected from the same data; estimate is exploratory — never a claim (REQ-XD-213)"),
                ),
            ]),
        ),
    ]);
    Ok(packet)
}

/// The input-manifest member — `{relpath, sha256}` per consumed record,
/// cohort `c` (the confirmation window's own evidence class).
pub fn input_manifest(entries: &[(&str, &str)]) -> Json {
    Json::Arr(
        entries
            .iter()
            .map(|(rel, sha)| {
                Json::obj([
                    ("cohort", Json::str("c")),
                    ("relpath", Json::str(*rel)),
                    ("sha256", Json::str(*sha)),
                ])
            })
            .collect(),
    )
}
