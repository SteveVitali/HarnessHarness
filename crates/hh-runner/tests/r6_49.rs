//! R6.49 — the flagged-review record + the first-pass freeze battery
//! (manifest 316).
//!
//! AC-6 (freeze) — every HUMAN-H16 disposition is applied verbatim; an
//! unreviewed flag remains a veto; frozen packets are byte-identical to
//! R6.48's except for the recorded `freeze` + `review` disposition
//! members. REQ-XD-071 consumed; §6.9's unreviewed-flag veto.

use hharness_runner::fp_results::*;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;

const REPO: &str = env!("CARGO_MANIFEST_DIR");

fn load_repo(rel: &str) -> Json {
    hharness_wire::json::parse(
        &std::fs::read_to_string(std::path::Path::new(REPO).join("../..").join(rel)).unwrap(),
    )
    .unwrap()
}

const SCAN: &str = "docs/build/reports/2026-11-19-fp-answer-scan.json";
const REVIEW: &str = "docs/build/reports/2026-11-19-fp-flagged-review.json";

/// The committed `hh-flagged-review/1` re-derives from the committed
/// scan: one "cleared" row per flagged slot, excerpt sha256s verbatim.
#[test]
fn flagged_review_covers_every_flag() {
    let scan = load_repo(SCAN);
    let review = load_repo(REVIEW);
    assert_eq!(
        review.get("schema").and_then(Json::as_str),
        Some(FLAGGED_REVIEW_SCHEMA)
    );
    let rows = review.get("records").and_then(Json::as_arr).unwrap();
    assert_eq!(rows.len(), 35);
    // every scan row with a non-empty hits list appears exactly once,
    // with the same excerpt digests in the same order.
    let mut expected = 0usize;
    for rec in scan.get("records").and_then(Json::as_arr).unwrap() {
        let hits = rec.get("hits").and_then(Json::as_arr).unwrap();
        if hits.is_empty() {
            continue;
        }
        expected += 1;
        let slot = rec.get("slot_ref").and_then(Json::as_str).unwrap();
        let row = rows
            .iter()
            .find(|r| r.get("slot_ref").and_then(Json::as_str) == Some(slot))
            .unwrap_or_else(|| panic!("flagged slot {slot} unreviewed"));
        assert_eq!(
            row.get("disposition").and_then(Json::as_str),
            Some("cleared")
        );
        let excerpts: Vec<&Json> = row
            .get("excerpt_sha256")
            .and_then(Json::as_arr)
            .unwrap()
            .iter()
            .collect();
        let want: Vec<&Json> = hits
            .iter()
            .filter_map(|h| h.get("excerpt_sha256"))
            .collect();
        assert_eq!(excerpts, want, "slot {slot} excerpts not verbatim");
    }
    assert_eq!(expected, 35);
    // coverage carries the operator's counts + the minimum-count label.
    let cov = review.get("coverage").unwrap();
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(35));
    assert_eq!(cov.get("confirmed").and_then(Json::as_int), Some(0));
    assert_eq!(cov.get("cleared").and_then(Json::as_int), Some(35));
    assert!(review
        .get("label")
        .and_then(Json::as_str)
        .unwrap()
        .contains("the confirmed count is a minimum"));
}

/// §6.9: a flagged slot without a disposition vetoes the freeze — the
/// record refuses when coverage.flagged exceeds the dispositioned rows.
#[test]
fn unreviewed_flag_vetoes_freeze() {
    let mut scan = load_repo(SCAN);
    // lie about coverage: claim one more flagged slot than the records
    // carry — the review must refuse, not paper over it.
    if let Json::Obj(m) = &mut scan {
        if let Some(Json::Obj(cov)) = m.get_mut("coverage") {
            cov.insert("flagged".into(), Json::Int(36));
        }
    }
    let err = flagged_review_record(
        &scan,
        "docs/build/readouts/HUMAN-H16.md",
        "sha256:x",
        "2026-11-19T00:00Z",
    )
    .unwrap_err();
    assert!(err.contains("unreviewed flag"), "{err}");
}

/// Every frozen packet: member-wise identical to R6.48's committed
/// packet except the two recorded disposition members (AC-6).
#[test]
fn frozen_packets_differ_only_in_disposition_members() {
    for base in [
        "2026-11-19-fp-results-p2-fp",
        "2026-11-19-fp-results-p4-fp",
        "2026-11-19-fp-results-p3-l2",
        "2026-11-19-fp-l2-audit",
        "2026-11-19-fp-l3",
    ] {
        let orig = load_repo(&format!("docs/build/reports/{base}.json"));
        let fz = load_repo(&format!("docs/build/reports/{base}-frozen.json"));
        let errs = freeze_diff_errors(&orig, &fz);
        assert!(errs.is_empty(), "{base}: {errs:?}");
        // the freeze member is the site's admission ticket (X-07).
        assert_eq!(
            fz.get("freeze")
                .and_then(|f| f.get("state"))
                .and_then(Json::as_str),
            Some("frozen"),
            "{base}"
        );
        assert_eq!(
            fz.get("freeze")
                .and_then(|f| f.get("gate_disposition"))
                .and_then(Json::as_str),
            Some("re-scope{end-first-pass-early}"),
            "{base}"
        );
        // the withheld packet keeps its typed disposition verbatim.
        if let Some(d) = orig.get("packet_disposition") {
            assert_eq!(fz.get("packet_disposition"), Some(d), "{base}");
        }
    }
}

/// The committed frozen bytes re-derive exactly (the canonical-string
/// equality is the byte-identity proof), and the review record
/// re-derives from the committed scan.
#[test]
fn freeze_is_deterministic_and_rederives() {
    let scan = load_repo(SCAN);
    let scan_sha = format!(
        "sha256:{}",
        hharness_wire::sha256::sha256_hex(
            &std::fs::read(std::path::Path::new(REPO).join("../..").join(SCAN)).unwrap()
        )
    );
    let want = load_repo(REVIEW);
    let got = flagged_review_record(
        &scan,
        "docs/build/readouts/HUMAN-H16.md",
        &scan_sha,
        "2026-11-19T00:00Z",
    )
    .unwrap();
    assert_eq!(got.to_canonical_string(), want.to_canonical_string());
    for base in ["2026-11-19-fp-results-p2-fp", "2026-11-19-fp-l3"] {
        let orig = load_repo(&format!("docs/build/reports/{base}.json"));
        let want = load_repo(&format!("docs/build/reports/{base}-frozen.json"));
        let got = freeze_packet(
            &orig,
            REVIEW,
            SCAN,
            &scan_sha,
            "re-scope{end-first-pass-early}",
            "2026-11-19T00:00Z",
        )
        .unwrap();
        assert_eq!(got.to_canonical_string(), want.to_canonical_string());
    }
}

/// A frozen record still carries the withheld packet's mandatory
/// members — the freeze adds, never removes (REQ-XH-020).
#[test]
fn frozen_packet_keeps_typed_withholds() {
    let fz = load_repo("docs/build/reports/2026-11-19-fp-results-p2-fp-frozen.json");
    assert_eq!(
        fz.get("packet_disposition").and_then(Json::as_str),
        Some("withheld{window_deadline}:protected_pair_truncated{k<5,R<5}")
    );
    let harnesses = fz.get("harnesses").unwrap();
    for (arm, want) in [
        ("mini-swe-agent", "withheld{window_deadline}"),
        ("pi", "withheld{admission:AG3}"),
        ("claude-code", "withheld{admission:AG3}"),
    ] {
        assert_eq!(
            harnesses
                .get(arm)
                .and_then(|h| h.get("disposition"))
                .and_then(Json::as_str),
            Some(want),
            "arm {arm}"
        );
    }
    // P4-FP: the HH-native arm + the v5 configuration arm.
    let fz4 = load_repo("docs/build/reports/2026-11-19-fp-results-p4-fp-frozen.json");
    let h4 = fz4.get("harnesses").unwrap();
    for (arm, want) in [
        ("hh_native", "withheld{window_deadline}"),
        ("v5", "withheld{budget}"),
    ] {
        assert_eq!(
            h4.get(arm)
                .and_then(|h| h.get("disposition"))
                .and_then(Json::as_str),
            Some(want),
            "arm {arm}"
        );
    }
}
