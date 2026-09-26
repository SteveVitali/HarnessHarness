//! The hosted-specific metric folds (spec §6.6 §2.4; ADR-0165 D8; S4.5a).
//!
//! The seven `applies_to_classes = {hosted}` declarations live in
//! [`crate::catalogue`]; this module is the fold that produces their values
//! from a hosted run's *projected* ledger rows — the native-class rows the
//! Hosting ABI projection (`proj_ABI`, `hh-hosting`) lifted with their
//! `mediation` stamp preserved in the payload (the `mediation` payload field
//! on every hosted row, §6.6 §3). `hh-eval` never depends on `hh-hosting`:
//! the fold reads ledger data, not hosting code (CC5/CC6 — the hosting tier
//! removes, these declarations simply produce `n/a{not_run}`).
//!
//! Every fold is a rate/count over *what the ledger says*. A value the run
//! did not produce is `None` — the caller renders the typed `n/a{reason}`
//! (never 0, never a proxy, never a coerced `unknown`).
//!
//! Fold definitions (owned here; ADR-0165 D8 names the metrics, the fold
//! semantics are this item's decision — recorded in the run ledger and the
//! S4.5a ADR):
//!
//! - `hosted_observation_completeness` — typed lifts / all hosted rows, ppm.
//!   A row the projection could not lift to a native class lands as a
//!   `lifecycle.hosted.native_record` leaf (CC3 — nothing silently drops);
//!   those leaves are the denominator's incomplete share.
//! - `mediation_coverage` — rows stamped `mediated` / stamped rows, ppm.
//!   `observed` and `unobserved` count against coverage; a run with no
//!   hosted rows yields `n/a`, never 0.
//! - `usage_report_agreement` — per-dimension min/max agreement between the
//!   `measured` and `participant_reported` `measurement.cost.attributed`
//!   rows, averaged in ppm (AC-R-2.10.6-9: both rows exist with distinct
//!   provenance; measured is primary). A run with no participant report —
//!   or no measured counterpart — yields `n/a{observability}`, never 0
//!   ("a participant reporting nothing yields NoPrice/unknown, never zero").
//! - `conformance_drift_count` — a count, not a rate: the number of
//!   `lifecycle.hosted.drift_observed` rows the run appended.
//! - `synthesized_terminal_rate` — terminal rows carrying
//!   `payload.synthesized = true` (I-1's `unobserved` synthesis) / all
//!   terminal rows, ppm.
//! - `observed_duplicate_action_rate` — `action.tool.*` rows beyond the
//!   first for one `call_id`/`tool_call_id`, or byte-identical payloads
//!   when no id rides the row / all action rows, ppm. Participant-reported
//!   tool events never enter the effect lifecycle — the duplicates are
//!   *observed*, never adjudicated.
//! - `observed_blast_radius` — a count: the number of distinct action
//!   coordinates the run touched (distinct tool names across `action.tool.*`
//!   rows plus distinct `host_norm`s across `security.egress.requested`
//!   rows). The native `blast_radius` metric is `n/a{class}` on hosted rows;
//!   this is the observational counterpart.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::Json;

use crate::facts::FactRow;
use crate::stats::PPM;

/// The terminal classes I-1 synthesizes `unobserved` when the participant's
/// stream ends mid-span (the same set `ensure_terminal` writes).
const TERMINAL_CLASSES: &[&str] = &[
    "lifecycle.turn.finished",
    "lifecycle.run.finished",
    "model.call.completed",
    "model.call.failed",
    "action.tool.completed",
    "action.tool.rejected",
    "action.tool.surface_rejected",
    "action.tool.call.refused",
];

/// The action classes the duplicate/blast folds read (participant-reported
/// tool rows lifted into the `action.tool.*` family).
const ACTION_CLASSES: &[&str] = &[
    "action.tool.completed",
    "action.tool.rejected",
    "action.tool.surface_rejected",
    "action.tool.call.refused",
    "action.tool.proposed",
];

/// `p.member` as a string slice.
fn ms<'a>(p: &'a Json, member: &str) -> Option<&'a str> {
    p.get(member).and_then(Json::as_str)
}

/// `p.member` as an int.
fn mi(p: &Json, member: &str) -> Option<i64> {
    p.get(member).and_then(Json::as_int)
}

/// A row belongs to the hosted surface when it carries the `mediation`
/// payload stamp or lands in the `lifecycle.hosted.*` family (the stamps
/// the boundary writes on every hosted row, §6.6 §3/§6).
fn is_hosted_row(r: &FactRow) -> bool {
    r.class.starts_with("lifecycle.hosted.") || r.payload.get("mediation").is_some()
}

/// The fold's typed result — one member per declared metric, `Option` where
/// the run can fail to produce a value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HostedMetrics {
    /// Typed lifts / all hosted rows (ppm).
    pub hosted_observation_completeness: Option<i64>,
    /// `mediated` rows / stamped rows (ppm).
    pub mediation_coverage: Option<i64>,
    /// Mean per-dimension min/max agreement, measured vs reported (ppm).
    pub usage_report_agreement: Option<i64>,
    /// `lifecycle.hosted.drift_observed` rows (a count).
    pub conformance_drift_count: i64,
    /// Synthesized terminals / all terminals (ppm).
    pub synthesized_terminal_rate: Option<i64>,
    /// Duplicate action rows / action rows (ppm).
    pub observed_duplicate_action_rate: Option<i64>,
    /// Distinct action coordinates touched (a count).
    pub observed_blast_radius: Option<i64>,
}

/// `fold(rows)` — the one-pass deterministic computation over the run's
/// rows. `rows` is the same `FactRow` view [`LedgerFacts`] reads.
///
/// [`LedgerFacts`]: crate::facts::LedgerFacts
pub fn fold(rows: &[FactRow]) -> HostedMetrics {
    let mut m = HostedMetrics::default();
    let hosted: Vec<&FactRow> = rows.iter().filter(|r| is_hosted_row(r)).collect();
    if hosted.is_empty() {
        // Not a hosted run — every member stays `None`/`0` so the caller
        // renders `n/a{not_run}` rather than a vacuous perfect score.
        return m;
    }

    // ── hosted_observation_completeness ─────────────────────────────────
    let leaves = hosted
        .iter()
        .filter(|r| r.class == "lifecycle.hosted.native_record")
        .count() as i64;
    let total = hosted.len() as i64;
    m.hosted_observation_completeness = Some((total - leaves) * PPM / total);

    // ── mediation_coverage ──────────────────────────────────────────────
    let stamped: Vec<&&FactRow> = hosted
        .iter()
        .filter(|r| r.payload.get("mediation").is_some())
        .collect();
    if !stamped.is_empty() {
        let mediated = stamped
            .iter()
            .filter(|r| ms(&r.payload, "mediation") == Some("mediated"))
            .count() as i64;
        m.mediation_coverage = Some(mediated * PPM / stamped.len() as i64);
    }

    // ── usage_report_agreement ──────────────────────────────────────────
    // Group `measurement.cost.attributed` spend by dimension × provenance.
    // The projection stamps `provenance: participant_reported` on lifted
    // `usage.reported` rows; measured rows carry `measured`/`intercepted`
    // (or no member — the instrument's own accounting is primary).
    let mut measured: BTreeMap<String, i64> = BTreeMap::new();
    let mut reported: BTreeMap<String, i64> = BTreeMap::new();
    for r in &hosted {
        if r.class != "measurement.cost.attributed" {
            continue;
        }
        let usage = r.payload.get("usage").unwrap_or(&r.payload);
        let dimension = ms(usage, "dimension")
            .or_else(|| ms(&r.payload, "dimension"))
            .unwrap_or("spend")
            .to_string();
        let amount = mi(usage, "amount")
            .or_else(|| mi(usage, "micro_usd"))
            .or_else(|| mi(&r.payload, "amount"))
            .or_else(|| mi(&r.payload, "micro_usd"))
            .unwrap_or(0);
        match ms(&r.payload, "provenance") {
            Some("participant_reported") => {
                *reported.entry(dimension).or_insert(0) += amount;
            }
            _ => {
                *measured.entry(dimension).or_insert(0) += amount;
            }
        }
    }
    if !reported.is_empty() {
        // Agreement per dimension the *participant* reported: min/max when a
        // measured row exists; 0 when it does not (the report is unverified —
        // counted against agreement, never excused). Dimensions the Lab
        // measured but the participant stayed silent on count 0 as well —
        // silence is disagreement, never omission.
        let dims: BTreeSet<&String> = measured.keys().chain(reported.keys()).collect();
        let mut sum: i128 = 0;
        let mut n: i128 = 0;
        for d in dims {
            let a = measured.get(d).copied().unwrap_or(0);
            let b = reported.get(d).copied().unwrap_or(0);
            let lo = a.min(b) as i128;
            let hi = a.max(b) as i128;
            sum += if hi == 0 {
                PPM as i128
            } else {
                lo * PPM as i128 / hi
            };
            n += 1;
        }
        m.usage_report_agreement = Some((sum / n.max(1)) as i64);
    }

    // ── conformance_drift_count ─────────────────────────────────────────
    m.conformance_drift_count = hosted
        .iter()
        .filter(|r| r.class == "lifecycle.hosted.drift_observed")
        .count() as i64;

    // ── synthesized_terminal_rate ───────────────────────────────────────
    let terminals: Vec<&&FactRow> = hosted
        .iter()
        .filter(|r| TERMINAL_CLASSES.contains(&r.class.as_str()))
        .collect();
    if !terminals.is_empty() {
        let synthesized = terminals
            .iter()
            .filter(|r| r.payload.get("synthesized") == Some(&Json::Bool(true)))
            .count() as i64;
        m.synthesized_terminal_rate = Some(synthesized * PPM / terminals.len() as i64);
    }

    // ── observed_duplicate_action_rate ──────────────────────────────────
    let actions: Vec<&&FactRow> = hosted
        .iter()
        .filter(|r| ACTION_CLASSES.contains(&r.class.as_str()))
        .collect();
    if !actions.is_empty() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut duplicates = 0i64;
        for r in &actions {
            // The dedup key: the call id when the participant carries one,
            // else the class + canonical payload (two byte-identical action
            // reports are a duplicate; an id-less *distinct* action is not).
            let key = ms(&r.payload, "call_id")
                .or_else(|| ms(&r.payload, "tool_call_id"))
                .or_else(|| ms(&r.payload, "action_id"))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}|{}", r.class, r.payload.to_canonical_string()));
            if !seen.insert(key) {
                duplicates += 1;
            }
        }
        m.observed_duplicate_action_rate = Some(duplicates * PPM / actions.len() as i64);
    }

    // ── observed_blast_radius ───────────────────────────────────────────
    let mut coordinates: BTreeSet<String> = BTreeSet::new();
    for r in &hosted {
        if ACTION_CLASSES.contains(&r.class.as_str()) {
            if let Some(tool) = ms(&r.payload, "tool")
                .or_else(|| ms(&r.payload, "name"))
                .or_else(|| ms(&r.payload, "tool_name"))
            {
                coordinates.insert(format!("tool:{tool}"));
            }
        }
        if r.class == "security.egress.requested" {
            if let Some(host) = ms(&r.payload, "host_norm").or_else(|| ms(&r.payload, "host")) {
                coordinates.insert(format!("egress:{host}"));
            }
        }
    }
    if !coordinates.is_empty() {
        m.observed_blast_radius = Some(coordinates.len() as i64);
    }

    m
}

/// The fold's canonical emission — `(metric, Option<value>)` rows in
/// declaration order. `None` is a genuine non-value: the caller renders the
/// typed `n/a{reason}` (a hosted metric on a run with no hosted rows renders
/// `n/a{not_run}` — the `fold` returns all-`None` there).
pub fn emit(m: &HostedMetrics) -> Vec<(&'static str, Option<i64>)> {
    vec![
        (
            "hosted_observation_completeness",
            m.hosted_observation_completeness,
        ),
        ("mediation_coverage", m.mediation_coverage),
        ("usage_report_agreement", m.usage_report_agreement),
        ("conformance_drift_count", Some(m.conformance_drift_count)),
        ("synthesized_terminal_rate", m.synthesized_terminal_rate),
        (
            "observed_duplicate_action_rate",
            m.observed_duplicate_action_rate,
        ),
        ("observed_blast_radius", m.observed_blast_radius),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_wire::Json;

    fn row(seq: u64, class: &str, payload: Json) -> FactRow {
        FactRow {
            seq,
            event_id: None,
            class: class.into(),
            payload,
        }
    }

    fn hosted(class: &str, payload: Json) -> FactRow {
        let mut p = payload;
        if let Json::Obj(ref mut m) = p {
            m.insert("mediation".into(), Json::str("mediated"));
        }
        row(0, class, p)
    }

    #[test]
    fn a_non_hosted_run_folds_to_all_none() {
        let rows = vec![row(1, "lifecycle.run.created", Json::obj([]))];
        let m = fold(&rows);
        assert_eq!(m, HostedMetrics::default());
    }

    #[test]
    fn observation_completeness_counts_native_record_leaves_against() {
        let rows = vec![
            hosted("lifecycle.turn.finished", Json::obj([])),
            hosted("lifecycle.turn.finished", Json::obj([])),
            hosted(
                "lifecycle.hosted.native_record",
                Json::obj([("kind", Json::str("_vendor"))]),
            ),
        ];
        let m = fold(&rows);
        // 2 typed / 3 total — the leaf is the incomplete share.
        assert_eq!(m.hosted_observation_completeness, Some(2 * PPM / 3));
    }

    #[test]
    fn mediation_coverage_is_the_mediated_share() {
        let mediated = hosted("lifecycle.turn.started", Json::obj([]));
        let mut observed = row(2, "lifecycle.turn.started", Json::obj([]));
        if let Json::Obj(ref mut p) = observed.payload {
            p.insert("mediation".into(), Json::str("observed"));
        }
        let m = fold(&[mediated, observed]);
        assert_eq!(m.mediation_coverage, Some(PPM / 2));
    }

    #[test]
    fn usage_agreement_is_none_without_a_participant_report() {
        let rows = vec![hosted(
            "measurement.cost.attributed",
            Json::obj([
                ("dimension", Json::str("spend")),
                ("amount", Json::Int(100)),
            ]),
        )];
        assert_eq!(fold(&rows).usage_report_agreement, None);
    }

    #[test]
    fn usage_agreement_averages_per_dimension_min_max() {
        let mut measured = hosted(
            "measurement.cost.attributed",
            Json::obj([
                ("dimension", Json::str("spend")),
                ("amount", Json::Int(100)),
            ]),
        );
        if let Json::Obj(ref mut p) = measured.payload {
            p.insert("provenance".into(), Json::str("measured"));
        }
        let mut reported = hosted(
            "measurement.cost.attributed",
            Json::obj([("dimension", Json::str("spend")), ("amount", Json::Int(50))]),
        );
        if let Json::Obj(ref mut p) = reported.payload {
            p.insert("provenance".into(), Json::str("participant_reported"));
        }
        // The unmeasured second reported dimension counts 0 — silence is
        // disagreement, never omission.
        let mut extra = hosted(
            "measurement.cost.attributed",
            Json::obj([
                ("dimension", Json::str("tokens.output")),
                ("amount", Json::Int(10)),
            ]),
        );
        if let Json::Obj(ref mut p) = extra.payload {
            p.insert("provenance".into(), Json::str("participant_reported"));
        }
        let m = fold(&[measured, reported, extra]);
        // (500_000 + 0) / 2
        assert_eq!(m.usage_report_agreement, Some(PPM / 4));
    }

    #[test]
    fn drift_count_and_synthesized_rate_read_the_marks() {
        let drift = row(1, "lifecycle.hosted.drift_observed", Json::obj([]));
        let synth = hosted(
            "lifecycle.turn.finished",
            Json::obj([("synthesized", Json::Bool(true))]),
        );
        let normal = hosted("lifecycle.turn.finished", Json::obj([]));
        let m = fold(&[drift, synth, normal]);
        assert_eq!(m.conformance_drift_count, 1);
        assert_eq!(m.synthesized_terminal_rate, Some(PPM / 2));
    }

    #[test]
    fn duplicate_actions_and_blast_radius_fold_deterministically() {
        let a = hosted(
            "action.tool.completed",
            Json::obj([("call_id", Json::str("c1")), ("tool", Json::str("fs.read"))]),
        );
        let dup = hosted(
            "action.tool.completed",
            Json::obj([("call_id", Json::str("c1")), ("tool", Json::str("fs.read"))]),
        );
        let other = hosted(
            "action.tool.completed",
            Json::obj([("call_id", Json::str("c2")), ("tool", Json::str("exec"))]),
        );
        let m = fold(&[a, dup, other]);
        assert_eq!(m.observed_duplicate_action_rate, Some(PPM / 3));
        assert_eq!(m.observed_blast_radius, Some(2));
    }

    #[test]
    fn emit_covers_the_seven_declarations_in_order() {
        let m = fold(&[hosted("lifecycle.turn.finished", Json::obj([]))]);
        let names: Vec<&str> = emit(&m).iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                "hosted_observation_completeness",
                "mediation_coverage",
                "usage_report_agreement",
                "conformance_drift_count",
                "synthesized_terminal_rate",
                "observed_duplicate_action_rate",
                "observed_blast_radius",
            ]
        );
    }
}
