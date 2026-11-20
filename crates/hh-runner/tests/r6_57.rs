//! R6.57 — the executed-W4x re-fold (manifest 325f): the committed
//! `hh-fp-w4x-outcomes/1` fold over the a4 `w4x` + a5 `w4xv` windows,
//! the answer-scan coverage (69 flagged — 34 flagged passes), the
//! `withheld{flagged_review}` freeze state, and byte-identical packet
//! re-emission from committed inputs only.

use std::path::{Path, PathBuf};

use hharness_runner::fp_w4::slot_outcomes;
use hharness_runner::fp_w4x::*;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn load(repo: &Path, rel: &str) -> Json {
    let text = std::fs::read_to_string(repo.join(rel)).expect(rel);
    match hharness_wire::json::parse(&text) {
        Ok(j) => j,
        Err(_) => panic!("{rel}: committed record must be canonical JSON"),
    }
}

const OUT: &str = "docs/build/reports/2026-11-20-fp-w4x-outcomes.json";
const SCAN: &str = "docs/build/reports/2026-11-20-fp-w4x-answer-scan.json";
const INPUT: &str = "docs/build/reports/2026-11-20-fp-w4x-flagged-review-input.json";
const P2: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p2-fp.json";
const P4: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p4-fp.json";
const L2: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p3-l2.json";
const L3: &str = "docs/build/reports/2026-11-20-fp-w4x-l3.json";

#[test]
fn outcomes_fold_counts() {
    let o = load(&repo(), OUT);
    assert_eq!(
        o.get("schema").and_then(Json::as_str),
        Some(W4X_OUTCOMES_SCHEMA)
    );
    let slots = o.get("slots").and_then(Json::as_obj).unwrap();
    // a4's 150 + a5's 50 — window-tagged keys (v5's a4 recorded{127}
    // rows coexist with their a5 scored rows; never rewritten).
    assert_eq!(slots.len(), 200);
    let rows = slot_outcomes(&o);
    let count = |arm: &str, tag: &str| {
        rows.iter()
            .filter(|r| r.arm == arm && r.slot_id.starts_with(tag))
            .count()
    };
    assert_eq!(count("pi", "w4x:"), 50);
    assert_eq!(count("claude-code", "w4x:"), 50);
    assert_eq!(count("v5", "w4x:"), 50);
    assert_eq!(count("v5", "w4xv:"), 50);
    let pass = |arm: &str, tag: &str| {
        rows.iter()
            .filter(|r| r.arm == arm && r.slot_id.starts_with(tag) && r.pass == Some(true))
            .count()
    };
    // R6.56's authoritative counts: pi 31, claude-code 30, v5 20 (a5).
    assert_eq!(pass("pi", "w4x:"), 31);
    assert_eq!(pass("claude-code", "w4x:"), 30);
    assert_eq!(pass("v5", "w4xv:"), 20);
    // a4 v5 rows carry no verdict — machinery absence, never a pass.
    assert_eq!(pass("v5", "w4x:"), 0);
}

#[test]
fn a4_v5_rows_keep_recorded_127_verbatim() {
    let o = load(&repo(), OUT);
    let slots = o.get("slots").and_then(Json::as_obj).unwrap();
    let a4_v5: Vec<&Json> = slots
        .values()
        .filter(|s| s.get("window").and_then(Json::as_str) == Some("w4x"))
        .filter(|s| s.get("arm").and_then(Json::as_str) == Some("v5"))
        .collect();
    assert_eq!(a4_v5.len(), 50);
    for s in &a4_v5 {
        let v = s.get("verifier").expect("a4 v5 verifier member");
        assert_eq!(v.get("returncode").and_then(Json::as_int), Some(127));
        assert_eq!(
            s.get("pass").and_then(Json::as_str),
            Some("unavailable{no_scored_outcome}")
        );
    }
    // the a5 rows carry real verdicts.
    let a5_v5_pass: usize = slots
        .values()
        .filter(|s| s.get("window").and_then(Json::as_str) == Some("w4xv"))
        .filter(|s| s.get("pass") == Some(&Json::Bool(true)))
        .count();
    assert_eq!(a5_v5_pass, 20);
}

#[test]
fn scan_covers_every_scored_trial_and_flags() {
    let s = load(&repo(), SCAN);
    let cov = s.get("coverage").unwrap();
    assert_eq!(cov.get("scanned").and_then(Json::as_int), Some(150));
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(69));
    assert_eq!(cov.get("unresolved").and_then(Json::as_int), Some(0));
    // the a4 v5 rows land on not_scored with the typed 127 reason.
    let ns = cov.get("not_scored").and_then(Json::as_arr).unwrap();
    assert_eq!(ns.len(), 50);
    assert!(ns.iter().all(|r| r
        .get("reason")
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("verifier_recorded_127")));
    // 34 flagged passes are the veto set — the review input routes them.
    let input = load(&repo(), INPUT);
    let rows = input
        .get("flagged_pass_list")
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(rows.len(), 34);
    assert_eq!(
        input.get("input_to").and_then(Json::as_str),
        Some(W4X_REVIEW_INPUT_TO)
    );
    assert_eq!(
        input.get("disposition").and_then(Json::as_str),
        Some("pending{human}")
    );
}

#[test]
fn packets_validate_and_supersede() {
    for rel in [P2, P4] {
        let j = load(&repo(), rel);
        validate_w4x_results_packet(&j).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
        assert_eq!(
            j.get("supersession")
                .and_then(|s| s.get("basis"))
                .and_then(Json::as_str),
            Some(ADR_0453_PATH)
        );
    }
    // pi/claude-code land executed rows — the AG3 history stays only in
    // the supersession note, never as a live landed disposition.
    let p2 = load(&repo(), P2);
    let hs = p2.get("harnesses").and_then(Json::as_obj).unwrap();
    for arm in ["pi", "claude-code", "mini-swe-agent"] {
        assert_eq!(
            hs[arm].get("disposition").and_then(Json::as_str),
            Some("executed"),
            "{arm} must land executed"
        );
        let th = hs[arm].get("theta").unwrap();
        assert!(th.get("ppm").and_then(Json::as_int).is_some());
    }
    // theta over the vetoed fold: mini 29/50 cleared, pi 14/50 and
    // claude-code 13/50 with flagged passes scored as failures.
    assert_eq!(
        hs["mini-swe-agent"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(580_000))
    );
    assert_eq!(
        hs["pi"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(280_000))
    );
    assert_eq!(
        hs["claude-code"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(260_000))
    );
    // the computed Δ rows: pi −320k-(-20k bound), cc similar — the
    // paired-cell point lands, Holm members stay pending.
    let d = hs["pi"].get("delta_vs_ref").unwrap();
    assert_eq!(d.get("ppm"), Some(&Json::Int(-300_000)));
    assert_eq!(
        hs["claude-code"].get("delta_vs_ref").unwrap().get("ppm"),
        Some(&Json::Int(-320_000))
    );
    // the Holm family stays pending{flagged_review}, never fabricated.
    let fam = p2
        .get("family")
        .and_then(|f| f.get("members"))
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(fam.len(), 2);
    assert!(fam.iter().all(|m| m
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("pending{flagged_review}")));
    // p4: v5 landed 20/50; hh_native unchanged typed; Δ refused verbatim.
    let p4 = load(&repo(), P4);
    let hs4 = p4.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs4["v5"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(400_000))
    );
    assert_eq!(
        hs4["hh_native"].get("disposition").and_then(Json::as_str),
        Some("unavailable{verifier_recorded_no_verdict}")
    );
    let dv5 = hs4["v5"].get("delta_vs_ref").unwrap();
    assert!(dv5
        .get("ppm")
        .and_then(|p| p.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("UnresolvedCell"));
}

#[test]
fn freeze_withholds_over_unreviewed_flags() {
    let scan = load(&repo(), SCAN);
    for rel in [P2, P4, L2, L3] {
        let pkt = load(&repo(), rel);
        let err = w4x_freeze_packet(&pkt, &scan, "sha256:x", "sha256:y")
            .expect_err(&format!("{rel} must refuse the freeze"));
        assert!(err.contains("withheld{flagged_review}"), "{rel}: {err}");
    }
}

#[test]
fn committed_sidecars_verify() {
    for rel in [
        OUT,
        SCAN,
        INPUT,
        P2,
        P4,
        L2,
        L3,
        "docs/build/reports/2026-11-20-fp-w4x-admission-pi-executed.json",
        "docs/build/reports/2026-11-20-fp-w4x-admission-claude-code-executed.json",
    ] {
        let side = rel.replace(".json", ".sha256");
        let text = std::fs::read_to_string(repo().join(&side)).expect(&side);
        let want = text.split_whitespace().next().unwrap().to_string();
        let bytes = std::fs::read(repo().join(rel)).expect(rel);
        let got = hharness_wire::sha256::sha256_hex(&bytes);
        assert_eq!(want, got, "{rel} sidecar mismatch");
    }
}
