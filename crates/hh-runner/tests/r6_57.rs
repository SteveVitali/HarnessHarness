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

// ── the post-review restored generation (HUMAN-H20 landed) ──────────

const REVIEW: &str = "docs/build/reports/2026-11-20-fp-w4x-flagged-review.json";
const P2_POST: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p2-fp-post-review.json";
const P4_POST: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p4-fp-post-review.json";
const L2_POST: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p3-l2-post-review.json";
const L3_POST: &str = "docs/build/reports/2026-11-20-fp-w4x-l3-post-review.json";
const P2_FROZEN: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p2-fp-frozen.json";
const P4_FROZEN: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p4-fp-frozen.json";
const L2_FROZEN: &str = "docs/build/reports/2026-11-20-fp-w4x-results-p3-l2-frozen.json";
const L3_FROZEN: &str = "docs/build/reports/2026-11-20-fp-w4x-l3-frozen.json";

#[test]
fn review_record_all_cleared_and_derived_disposition() {
    let review = load(&repo(), REVIEW);
    assert_eq!(
        review.get("schema").and_then(Json::as_str),
        Some(hharness_runner::fp_results::FLAGGED_REVIEW_SCHEMA)
    );
    // the all-cleared gate passes — every flagged record dispositioned
    // `cleared`, none confirmed.
    w4x_review_verified(&review).expect("HUMAN-H20's record is all-cleared");
    let cov = review.get("coverage").unwrap();
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(69));
    assert_eq!(cov.get("confirmed").and_then(Json::as_int), Some(0));
    assert_eq!(cov.get("cleared").and_then(Json::as_int), Some(69));
    let rows = review.get("records").and_then(Json::as_arr).unwrap();
    assert_eq!(rows.len(), 69);
    assert!(rows
        .iter()
        .all(|r| r.get("disposition").and_then(Json::as_str) == Some("cleared")));
    // the derived record-level disposition cites the readout where the
    // operator's verbatim lives — never invented text.
    assert_eq!(
        review.get("disposition").and_then(Json::as_str),
        Some(
            "69 of 69 cleared — operator mass ruling \
             (docs/build/readouts/HUMAN-H20.md verbatim)"
        )
    );
    // the restored veto set is empty — every flag cleared; a confirmed
    // or undispositioned flag would stay a veto.
    let scan = load(&repo(), SCAN);
    assert!(w4x_restored_vetoes(&scan, &review).is_empty());
    // a confirmed disposition keeps its veto — the restore is
    // disposition-driven, never a blanket clear.
    let mut mut_review = review.clone();
    if let Json::Obj(m) = &mut mut_review {
        if let Some(Json::Arr(rows)) = m.get_mut("records") {
            rows[0] = obj_for_confirmed(&rows[0]);
        }
    }
    let kept = w4x_restored_vetoes(&scan, &mut_review);
    assert_eq!(kept.len(), 1, "the one confirmed flag stays a veto");
    // and the all-cleared gate refuses the mutated record's coverage
    // honestly (coverage still reads 69 cleared — a forged record).
    assert!(w4x_review_verified(&mut_review).is_err());
}

/// `obj_for_confirmed(row)` — a copy of the review row with its
/// disposition mutated to `confirmed` (the refusal-path fixture).
fn obj_for_confirmed(row: &Json) -> Json {
    let mut m = match row {
        Json::Obj(m) => m.clone(),
        _ => panic!("review row is not an object"),
    };
    m.insert("disposition".to_string(), Json::str("confirmed"));
    Json::Obj(m)
}

#[test]
fn post_review_packets_carry_the_restored_fold() {
    for rel in [P2_POST, P4_POST] {
        let j = load(&repo(), rel);
        validate_w4x_results_packet(&j).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
        assert_eq!(
            j.get("supersession")
                .and_then(|s| s.get("basis"))
                .and_then(Json::as_str),
            Some(ADR_0453_PATH)
        );
        // no pending{flagged_review} / unavailable{pending_flagged_review}
        // marker survives in the restored generation.
        let text = j.to_canonical_string();
        assert!(
            !text.contains("pending{flagged_review}") && !text.contains("pending_flagged_review"),
            "{rel}: pending marker survives post-review"
        );
    }
    let p2 = load(&repo(), P2_POST);
    let hs = p2.get("harnesses").and_then(Json::as_obj).unwrap();
    // the restored fold: pi 31/50, claude-code 30/50, mini 29/50 —
    // the cleared flagged passes score as passes again.
    assert_eq!(
        hs["pi"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(620_000))
    );
    assert_eq!(
        hs["claude-code"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(600_000))
    );
    assert_eq!(
        hs["mini-swe-agent"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(580_000))
    );
    // the paired-cell Δ recomputes: pi +40000, claude-code +20000.
    let dpi = hs["pi"].get("delta_vs_ref").unwrap();
    assert_eq!(dpi.get("ppm"), Some(&Json::Int(40_000)));
    let dcc = hs["claude-code"].get("delta_vs_ref").unwrap();
    assert_eq!(dcc.get("ppm"), Some(&Json::Int(20_000)));
    // the sign-flip + Holm members land as ints — never fabricated,
    // recomputed over the restored population.
    assert!(dpi.get("p_raw_ppm").and_then(Json::as_int).is_some());
    assert!(dpi.get("p_holm_ppm").and_then(Json::as_int).is_some());
    assert!(dcc.get("p_raw_ppm").and_then(Json::as_int).is_some());
    assert!(dcc.get("p_holm_ppm").and_then(Json::as_int).is_some());
    // the Holm family members are computed — an unavailable member
    // stays at worst-case mass, never dropped.
    let fam = p2
        .get("family")
        .and_then(|f| f.get("members"))
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(fam.len(), 2);
    for m in fam {
        assert_eq!(m.get("status").and_then(Json::as_str), Some("computed"));
        assert!(m.get("raw_p_ppm").and_then(Json::as_int).is_some());
        assert!(m.get("adjusted_ppm").and_then(Json::as_int).is_some());
    }
    let expl = p2
        .get("exploratory_family")
        .and_then(|f| f.get("members"))
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(expl.len(), 1);
    assert_eq!(
        expl[0].get("status").and_then(Json::as_str),
        Some("computed")
    );
    // the leader decided: pi's restored θ is the argmax — every row
    // names the same leader_id and only pi rows itself the member.
    for arm in ["pi", "claude-code", "mini-swe-agent"] {
        let lg = hs[arm].get("leader_group").unwrap();
        assert_eq!(
            lg.get("leader_id").and_then(Json::as_str),
            Some("hh/pi@0.52.3"),
            "{arm} leader_id"
        );
        assert_eq!(
            lg.get("member"),
            Some(&Json::Bool(arm == "pi")),
            "{arm} leader member"
        );
    }
    // the integrity panels carry the honest per-arm disposition counts.
    let integ = |arm: &str| hs[arm].get("integrity").unwrap().clone();
    assert_eq!(
        integ("pi").get("flagged"),
        Some(&Json::Int(35)),
        "pi flagged slots"
    );
    assert_eq!(integ("pi").get("cleared"), Some(&Json::Int(35)));
    assert_eq!(integ("pi").get("confirmed"), Some(&Json::Int(0)));
    assert_eq!(integ("claude-code").get("flagged"), Some(&Json::Int(34)));
    assert_eq!(integ("claude-code").get("cleared"), Some(&Json::Int(34)));
    // mini's W4 cleared=32 (HUMAN-H19's record) is preserved.
    assert_eq!(integ("mini-swe-agent").get("cleared"), Some(&Json::Int(32)));

    // p4: v5's fold is unchanged by the review (no v5 flags) — θ 400000
    // stays; the Δ vs hh_native still refuses and its p-members stay
    // typed, never ints over a refusal.
    let p4 = load(&repo(), P4_POST);
    let hs4 = p4.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs4["v5"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(400_000))
    );
    let dv5 = hs4["v5"].get("delta_vs_ref").unwrap();
    assert!(dv5
        .get("ppm")
        .and_then(|p| p.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("UnresolvedCell"));
    for member in ["p_raw_ppm", "p_holm_ppm"] {
        assert_eq!(
            dv5.get(member)
                .and_then(|p| p.get("state"))
                .and_then(Json::as_str),
            Some("unavailable{contrast_refused}"),
            "v5 {member} stays typed over the refused contrast"
        );
    }
    // v5 is the leader over the single scored arm — honest label.
    let lg5 = hs4["v5"].get("leader_group").unwrap();
    assert_eq!(
        lg5.get("leader_id").and_then(Json::as_str),
        Some("hh/v5@corrected-seal")
    );
    assert_eq!(lg5.get("member"), Some(&Json::Bool(true)));
}

#[test]
fn post_review_l2_l3_are_review_untouched() {
    // l2/l3 carry no flagged-review member — the post-review generation
    // is byte-identical to the committed pre-review record.
    for (post, pre) in [(L2_POST, L2), (L3_POST, L3)] {
        let a = std::fs::read(repo().join(post)).expect(post);
        let b = std::fs::read(repo().join(pre)).expect(pre);
        assert_eq!(a, b, "{post} must equal the review-untouched fold {pre}");
    }
}

#[test]
fn frozen_copies_wrap_the_post_review_generation() {
    let review = load(&repo(), REVIEW);
    for (post_rel, frozen_rel, restored) in [
        (P2_POST, P2_FROZEN, true),
        (P4_POST, P4_FROZEN, true),
        (L2_POST, L2_FROZEN, false),
        (L3_POST, L3_FROZEN, false),
    ] {
        let frozen = load(&repo(), frozen_rel);
        let post = load(&repo(), post_rel);
        // the member-wise diff rule — the frozen copy adds exactly
        // {freeze, review, seal_open_ref} over the post-review record.
        let errs = w4x_freeze_diff_errors(&post, &frozen);
        assert!(errs.is_empty(), "{frozen_rel}: {errs:?}");
        let fr = frozen.get("freeze").unwrap();
        assert_eq!(fr.get("state").and_then(Json::as_str), Some("frozen"));
        assert_eq!(
            fr.get("fold").and_then(Json::as_str),
            Some(if restored {
                "post_review_restored"
            } else {
                "unchanged_by_review"
            }),
            "{frozen_rel} fold"
        );
        // provenance names the append-only pre-review record.
        let pre_rel = post_rel.replace("-post-review", "");
        assert_eq!(
            fr.get("recomputed_from").and_then(Json::as_str),
            Some(pre_rel.as_str()),
            "{frozen_rel} recomputed_from"
        );
        // the review member lands the derived disposition verbatim.
        let rv = frozen.get("review").unwrap();
        assert_eq!(rv.get("cleared").and_then(Json::as_int), Some(69));
        assert_eq!(rv.get("confirmed").and_then(Json::as_int), Some(0));
        assert_eq!(
            rv.get("disposition").and_then(Json::as_str),
            review.get("disposition").and_then(Json::as_str)
        );
    }
    // the frozen p2 carries the restored θ — the freeze never wrapped
    // the vetoed fold.
    let fz = load(&repo(), P2_FROZEN);
    let hs = fz.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs["pi"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(620_000))
    );
    assert_eq!(
        hs["claude-code"].get("theta").unwrap().get("ppm"),
        Some(&Json::Int(600_000))
    );
    let text = fz.to_canonical_string();
    assert!(!text.contains("pending{flagged_review}"));
    assert!(!text.contains("pending_flagged_review"));
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
        REVIEW,
        P2_POST,
        P4_POST,
        L2_POST,
        L3_POST,
        P2_FROZEN,
        P4_FROZEN,
        L2_FROZEN,
        L3_FROZEN,
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
