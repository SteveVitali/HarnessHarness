//! `fp_p4_refold` — the R7.9 (manifest 342) activation refold: the
//! DF-R7.1-2 unblocker's second half. The committed gen-3
//! `sweep-registry/1` folded V3/V4/V5 `unmeasured{evidence_absent}`
//! because no committed corpus measured the three conditional knobs.
//! This module re-folds the same `fold_activation` /
//! `forecast_entry` machinery (hh-lab `p4`, REQ-XD-208/209) over an
//! `hh-p4-activation-corpus/1` record — the metric-bearing activation
//! input the row names — so each level returns a measured verdict or
//! the same honest unmeasured shape when the corpus carries no
//! coverage for it.
//!
//! The corpus supplies counts only — never outcome data (the same
//! REQ-XD-205 input-manifest rule; the bin re-checks the corpus's
//! `selected_fields` before any value is read). The per-level metric
//! kind is the registry's own `activation_metric` declaration:
//!
//! - V3 `cmd-120` — `share_of_first_subjects`: subjects with ≥ 1
//!   `action.tool.completed` over the 30 s baseline bound.
//! - V4 `obs-2.5k` — `exact_counterfactual`: observations whose
//!   deterministic `json_head_tail@1` render differs under the level's
//!   2500/1250/1250 vs the baseline's 10000/5000/5000 (the corpus's
//!   `measure ≥ level.threshold` count — exact, recomputed, never
//!   sampled).
//! - V5 `rsn-drop` — `share_of_first_subjects` over *requests*
//!   (the registry's own fold spelling): upstream-bound requests
//!   carrying ≥ 1 non-empty reasoning member the drop knob exercises.
//!
//! A level whose corpus leg carries `total == 0` folds the
//! `unmeasured{evidence_absent}` shape verbatim — the corpus cannot
//! bear it and nothing is fabricated.

use hharness_lab::p4::{
    check_non_outcome_fields, fold_activation, forecast_entry, ActivationEvidence, ActivationMetric,
};
use hharness_wire::json::Json;

/// The refold record's schema.
pub const REFOLD_SCHEMA: &str = "hh-p4-activation-refold/1";
/// The corpus record schema the fold consumes.
pub const CORPUS_SCHEMA: &str = "hh-p4-activation-corpus/1";
/// The ticket that mints the refold.
pub const TICKET: &str = "342_R7.9";

/// The levels this refold covers, in registry order, with the
/// corpus-leg member names and the metric kind the registry declares.
pub const REFOLD_LEVELS: &[(&str, &str, &str, ActivationMetric)] = &[
    (
        "V3",
        "subjects",
        "share of first subjects with ≥ 1 command > 30 s",
        ActivationMetric::ShareOfFirstSubjects,
    ),
    (
        "V4",
        "observations",
        "exact counterfactual rendering difference",
        ActivationMetric::ExactCounterfactual,
    ),
    (
        "V5",
        "requests",
        "share of requests with a dropped non-empty reasoning member",
        ActivationMetric::ShareOfFirstSubjects,
    ),
];

/// The honest unmeasured fold — identical to the committed
/// `unmeasured{evidence_absent}` shape the R7.1 emitter mints.
fn unmeasured_fold(level_id: &str, metric: ActivationMetric, detail: &str) -> Json {
    Json::obj([
        ("level_id", Json::str(level_id)),
        ("metric", Json::str(metric.as_str())),
        ("activation_num", Json::Int(0)),
        ("activation_den", Json::Int(0)),
        ("activation_ppm", Json::Int(-1)),
        ("exact", Json::Bool(false)),
        ("note", Json::str(detail)),
    ])
}

/// Re-check the corpus record's `input_manifest.selected_fields` —
/// the same non-outcome gate the R7.1 emitter applies before any
/// value is read (REQ-XD-205/209).
pub fn check_corpus_manifest(corpus: &Json) -> Result<(), String> {
    let fields: Vec<String> = corpus
        .get("input_manifest")
        .and_then(|m| m.get("selected_fields"))
        .and_then(|f| match f {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|j| j.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| panic!("corpus record carries no input_manifest.selected_fields"));
    check_non_outcome_fields(fields.iter().map(String::as_str))
        .map_err(|e| format!("corpus input manifest refused: {e}"))
}

/// One level's `(exercised, total)` from the corpus's `activation`
/// member. `Ok(None)` = the leg carries no coverage — the fold stays
/// honest unmeasured. `Err` = the member is present but mistyped —
/// never silently zeroed.
fn level_evidence(corpus: &Json, level_id: &str, unit: &str) -> Result<Option<(u64, u64)>, String> {
    let leg = corpus.get("activation").and_then(|a| a.get(level_id));
    let Some(leg) = leg else {
        return Ok(None);
    };
    let exercised = leg
        .get("exercised")
        .and_then(Json::as_int)
        .ok_or_else(|| format!("activation.{level_id}.exercises missing (unit {unit})"))?;
    let total = leg
        .get("total")
        .and_then(Json::as_int)
        .ok_or_else(|| format!("activation.{level_id}.total missing (unit {unit})"))?;
    if exercised < 0 || total < 0 || exercised > total {
        return Err(format!(
            "activation.{level_id}: exercised {exercised} outside [0, total {total}]"
        ));
    }
    if total == 0 {
        return Ok(None);
    }
    Ok(Some((exercised as u64, total as u64)))
}

/// `refold_level(corpus, level_id, unit, metric)` — one level's
/// `forecast_entry` over the corpus's activation leg: measured when
/// the corpus covers it, the honest unmeasured shape otherwise.
pub fn refold_level(
    corpus: &Json,
    level_id: &str,
    unit: &str,
    metric: ActivationMetric,
) -> Result<Json, String> {
    let fold = match level_evidence(corpus, level_id, unit)? {
        Some((exercised, total)) => fold_activation(
            level_id,
            metric,
            ActivationEvidence { exercised, total },
        ),
        None => unmeasured_fold(
            level_id,
            metric,
            "unmeasured{evidence_absent} — the corpus carries no coverage for this level's activation leg; not a forecast pass",
        ),
    };
    Ok(forecast_entry(level_id, fold))
}

/// `refold_record(corpus, manifest_entries)` — the whole
/// `hh-p4-activation-refold/1` record: per-level folds + dispositions
/// and the input manifest's verbatim (relpath, sha256) entries.
/// `manifest_entries` is ordered `(relpath, sha256)` pairs — corpus
/// first, then the registry/level evidence the fold cites.
pub fn refold_record(corpus: &Json, manifest_entries: &[(&str, &str)]) -> Result<Json, String> {
    if corpus.get("schema").and_then(Json::as_str) != Some(CORPUS_SCHEMA) {
        return Err(format!("corpus record is not {CORPUS_SCHEMA}"));
    }
    check_corpus_manifest(corpus)?;
    let mut levels = Vec::new();
    for (level_id, unit, _decl, metric) in REFOLD_LEVELS {
        levels.push(refold_level(corpus, level_id, unit, *metric)?);
    }
    let manifest = Json::obj([
        (
            "rule",
            Json::str(
                "non-outcome fields only — the corpus's selected_fields are re-checked before any value is read",
            ),
        ),
        (
            "sources",
            Json::Arr(
                manifest_entries
                    .iter()
                    .map(|(p, s)| {
                        Json::obj([
                            ("path", Json::str(*p)),
                            ("sha256", Json::str(*s)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    Ok(Json::obj([
        ("schema", Json::str(REFOLD_SCHEMA)),
        ("ticket", Json::str(TICKET)),
        (
            "note",
            Json::str(
                "the DF-R7.1-2 refold: fold_activation + forecast_entry over the committed hh-p4-activation-corpus/1 activation legs — measured where the corpus bears evidence, honest unmeasured where it does not; a measured below-floor share folds withheld{inert_predicted}, never promoted",
            ),
        ),
        ("input_manifest", manifest),
        ("levels", Json::Arr(levels)),
    ]))
}
