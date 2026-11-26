//! Executable coverage for manifest ticket 342 / R7.9 — the
//! DF-R7.1-2 activation refold (`fp_p4_refold`). The committed
//! gen-3 registry folded V3/V4/V5 `unmeasured{evidence_absent}`;
//! the refold must return a MEASURED verdict where the committed
//! `hh-p4-activation-corpus/1` bears the activation inputs, and the
//! honest unmeasured shape where it does not. Every test fails if
//! the behaviour it names is removed.

use hharness_runner::fp_p4_refold;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::{parse, Json};

/// The corpus shape the extractor emits — the activation legs plus
/// the non-outcome input manifest.
fn corpus(v3: (i64, i64), v4: (i64, i64), v5: (i64, i64)) -> Json {
    parse(&format!(
        r#"{{
            "schema": "hh-p4-activation-corpus/1",
            "input_manifest": {{
                "selected_fields": [
                    "proxy.usage.view.tokens.input.uncached",
                    "action.tool.completed.execution_ms",
                    "context.observation.recorded.model_view.ref",
                    "upstream_request.messages[].reasoning_content"
                ]
            }},
            "activation": {{
                "V3": {{"exercised": {}, "total": {}, "unit": "subjects"}},
                "V4": {{"exercised": {}, "total": {}, "unit": "observations"}},
                "V5": {{"exercised": {}, "total": {}, "unit": "requests"}}
            }}
        }}"#,
        v3.0, v3.1, v4.0, v4.1, v5.0, v5.1
    ))
    .unwrap()
}

fn level(rec: &Json, id: &str) -> Json {
    rec.get("levels")
        .and_then(|l| match l {
            Json::Arr(a) => a
                .iter()
                .find(|e| e.get("level_id").and_then(Json::as_str) == Some(id))
                .cloned(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no level {id}"))
}

#[test]
fn measured_corpus_folds_measured_verdicts() {
    // The W4x retained root's own counts (extracted 2026-11-26):
    // V3 4/49, V4 267/1441, V5 1354/5124 — all three measured.
    let c = corpus((4, 49), (267, 1441), (1354, 5124));
    let rec = fp_p4_refold::refold_record(&c, &[("corpus.json", "sha256:x")]).unwrap();
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some("hh-p4-activation-refold/1")
    );

    let v3 = level(&rec, "V3");
    let f = v3.get("fold").unwrap();
    assert_eq!(f.get("activation_num").and_then(Json::as_int), Some(4));
    assert_eq!(f.get("activation_den").and_then(Json::as_int), Some(49));
    assert_eq!(f.get("activation_ppm").and_then(Json::as_int), Some(81_632));
    assert_eq!(
        f.get("metric").and_then(Json::as_str),
        Some("share_of_first_subjects")
    );
    // A measured share below the 250 000-ppm floor withholds —
    // measured, never promoted.
    assert_eq!(
        v3.get("disposition").and_then(Json::as_str),
        Some("withheld{inert_predicted}")
    );

    let v4 = level(&rec, "V4");
    let f = v4.get("fold").unwrap();
    assert_eq!(
        f.get("metric").and_then(Json::as_str),
        Some("exact_counterfactual")
    );
    assert_eq!(
        f.get("activation_ppm").and_then(Json::as_int),
        Some(185_287)
    );
    assert_eq!(f.get("exact").and_then(Json::as_bool), Some(true));
    assert_eq!(
        v4.get("disposition").and_then(Json::as_str),
        Some("withheld{inert_predicted}")
    );

    let v5 = level(&rec, "V5");
    let f = v5.get("fold").unwrap();
    assert_eq!(
        f.get("activation_ppm").and_then(Json::as_int),
        Some(264_246)
    );
    assert_eq!(
        v5.get("disposition").and_then(Json::as_str),
        Some("admitted{forecast_ok}")
    );
}

#[test]
fn uncovered_level_stays_honest_unmeasured() {
    // A corpus leg with total 0 folds the committed unmeasured shape —
    // never a fabricated 0/0 pass.
    let c = corpus((0, 0), (267, 1441), (1354, 5124));
    let rec = fp_p4_refold::refold_record(&c, &[]).unwrap();
    let v3 = level(&rec, "V3");
    let f = v3.get("fold").unwrap();
    assert_eq!(f.get("activation_ppm").and_then(Json::as_int), Some(-1));
    assert_eq!(
        v3.get("disposition").and_then(Json::as_str),
        Some("unmeasured{evidence_absent}")
    );
    let v4 = level(&rec, "V4");
    assert_eq!(
        v4.get("disposition").and_then(Json::as_str),
        Some("withheld{inert_predicted}")
    );
}

#[test]
fn outcome_named_field_refuses_the_fold() {
    // REQ-XD-205/209 — a selected member naming an outcome field
    // refuses before any value is read.
    let c = parse(
        r#"{
            "schema": "hh-p4-activation-corpus/1",
            "input_manifest": {"selected_fields": ["proxy.usage.source", "reward"]},
            "activation": {}
        }"#,
    )
    .unwrap();
    let e = fp_p4_refold::refold_record(&c, &[]).unwrap_err();
    assert!(e.contains("refused"), "{e}");
}

#[test]
fn wrong_schema_refuses() {
    let c = parse(r#"{"schema":"other/1","input_manifest":{"selected_fields":[]}}"#).unwrap();
    assert!(fp_p4_refold::refold_record(&c, &[]).is_err());
}
