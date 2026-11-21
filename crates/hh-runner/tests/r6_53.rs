//! R6.53 — FP-W4-RESULTS battery (manifest 325b).
//!
//! The executed-W4 re-fold under ADR-0452: the committed outcomes
//! record, the answer scan + flagged-review handoff, and the five
//! superseding packets — each checked against the executed evidence's
//! honest dispositions. The frozen R6.48 withheld packets stay
//! append-only and byte-untouched.

use std::collections::BTreeMap;

use hharness_eval::l2_wire::L2_SCHEMA;
use hharness_ontology::control::OutcomeClass;
use hharness_runner::fp_results::{L2_AUDIT_SCHEMA, L3_SCHEMA};
use hharness_runner::fp_w4::*;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

const REPO: &str = env!("CARGO_MANIFEST_DIR");

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(REPO)
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn load_repo(rel: &str) -> Json {
    let text = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("read {rel}: {e}"));
    match hharness_wire::json::parse(&text) {
        Ok(j) => j,
        Err(_) => hharness_wire::json::parse_provider(&text)
            .unwrap_or_else(|e| panic!("parse {rel}: {e}")),
    }
}

fn sidecar_ok(rel_json: &str) -> bool {
    let sidecar = repo_root().join(rel_json.replace(".json", ".sha256"));
    let Ok(text) = std::fs::read_to_string(sidecar) else {
        return false;
    };
    let Some(first) = text.lines().next() else {
        return false;
    };
    let declared: &str = first.split_whitespace().next().unwrap_or("");
    let bytes = std::fs::read(repo_root().join(rel_json)).unwrap_or_default();
    declared == sha256_hex(&bytes)
}

const W4_PACKETS: [&str; 5] = [
    "docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json",
    "docs/build/reports/2026-11-19-fp-w4-results-p4-fp.json",
    "docs/build/reports/2026-11-19-fp-w4-results-p3-l2.json",
    "docs/build/reports/2026-11-19-fp-w4-l2-audit.json",
    "docs/build/reports/2026-11-19-fp-w4-l3.json",
];

// ── the committed outcomes fold ──────────────────────────────────────

#[test]
fn outcomes_fold_matches_executed_window() {
    let o = load_repo("docs/build/reports/2026-11-19-fp-w4-outcomes.json");
    assert_eq!(
        o.get("schema").and_then(Json::as_str),
        Some(W4_OUTCOMES_SCHEMA)
    );
    let arms = o.get("arms").and_then(Json::as_obj).expect("arms map");
    // mini-swe-agent: 50 settled, 50×rc=0, 29 verifier passes.
    let mini = &arms["mini-swe-agent"];
    assert_eq!(mini.get("planned").and_then(Json::as_int), Some(50));
    let c = mini
        .get("counts")
        .and_then(Json::as_obj)
        .expect("mini counts");
    assert_eq!(c.get("settled").and_then(Json::as_int), Some(50));
    assert_eq!(c.get("rc0").and_then(Json::as_int), Some(50));
    assert_eq!(c.get("passes").and_then(Json::as_int), Some(29));
    // hh_native: 50 settled — 46×rc=0 + 4×rc=1 recorded failures,
    // never withholds; 50 verifier execs recorded, no verdict.
    let nat = &arms["hh_native"];
    let c = nat
        .get("counts")
        .and_then(Json::as_obj)
        .expect("nat counts");
    assert_eq!(c.get("settled").and_then(Json::as_int), Some(50));
    assert_eq!(c.get("rc0").and_then(Json::as_int), Some(46));
    assert_eq!(c.get("rc1").and_then(Json::as_int), Some(4));
    assert_eq!(c.get("verifier:recorded").and_then(Json::as_int), Some(50));
    // v5: 20 settled verifier_unavailable + 30 dropped at the
    // makespan checkpoint.
    let v5 = &arms["v5"];
    let c = v5.get("counts").and_then(Json::as_obj).expect("v5 counts");
    assert_eq!(c.get("settled").and_then(Json::as_int), Some(20));
    assert_eq!(c.get("rc1").and_then(Json::as_int), Some(20));
    assert_eq!(v5.get("planned").and_then(Json::as_int), Some(50));
    // pi + claude-code: verbatim not_run{admission:AG3}.
    for arm in ["pi", "claude-code"] {
        assert_eq!(
            arms[arm].get("disposition").and_then(Json::as_str),
            Some("not_run{admission:AG3}"),
            "{arm} disposition"
        );
    }
    // per-slot rows: pass is Bool where scored, a typed string where
    // not — never null, never a fabricated score.
    let rows = slot_outcomes(&o);
    assert_eq!(rows.len(), 120);
    let slots = o.get("slots").and_then(Json::as_obj).expect("slots");
    for (sid, s) in slots {
        let arm = s.get("arm").and_then(Json::as_str).unwrap_or("");
        match s.get("pass") {
            Some(Json::Bool(_)) => assert_eq!(arm, "mini-swe-agent"),
            Some(Json::Str(p)) => assert!(p.starts_with("unavailable{"), "{sid}"),
            other => panic!("{sid}: pass must be Bool or typed string — {other:?}"),
        }
        assert_ne!(s.get("rc"), Some(&Json::Null));
    }
    // the derived v5 dropped set — 30 named ids, disjoint from the
    // settled 20.
    let dropped = o
        .get("v5_dropped")
        .and_then(|d| d.get("slot_ids"))
        .and_then(Json::as_arr)
        .expect("dropped list");
    assert_eq!(dropped.len(), 30);
    for d in dropped {
        let id = d.as_str().expect("dropped id");
        assert!(id.starts_with("w4-v5-"));
        assert!(!slots.contains_key(id), "{id} is settled, not dropped");
    }
    assert!(sidecar_ok(
        "docs/build/reports/2026-11-19-fp-w4-outcomes.json"
    ));
}

// ── the answer scan + review handoff ─────────────────────────────────

#[test]
fn scan_coverage_and_review_handoff() {
    let s = load_repo("docs/build/reports/2026-11-19-fp-w4-answer-scan.json");
    assert_eq!(
        s.get("schema").and_then(Json::as_str),
        Some(hharness_runner::fp_results::SCAN_REPORT_SCHEMA)
    );
    let cov = s.get("coverage").and_then(Json::as_obj).expect("coverage");
    assert_eq!(cov.get("scanned").and_then(Json::as_int), Some(50));
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(32));
    // not_scored covers every non-scored W4 member: 50 hh_native +
    // 20 settled v5 + 30 derived dropped = 100.
    let ns = cov
        .get("not_scored")
        .and_then(Json::as_arr)
        .expect("not_scored rows");
    assert_eq!(ns.len(), 100);
    let flagged = s
        .get("records")
        .and_then(Json::as_arr)
        .map(|rs| {
            rs.iter()
                .filter(|r| r.get("state").and_then(Json::as_str) == Some("flagged"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(flagged, 32);
    let flagged_passes = s
        .get("records")
        .and_then(Json::as_arr)
        .map(|rs| {
            rs.iter()
                .filter(|r| {
                    r.get("state").and_then(Json::as_str) == Some("flagged")
                        && r.get("pass") == Some(&Json::Bool(true))
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(flagged_passes, 16);
    // the handoff routes to the new HUMAN row, pending — an agent
    // never reviews, clears or confirms.
    let inp = load_repo("docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json");
    assert_eq!(
        inp.get("schema").and_then(Json::as_str),
        Some("hh-flagged-review-input/1")
    );
    assert_eq!(
        inp.get("input_to").and_then(Json::as_str),
        Some("HUMAN-H19")
    );
    assert_eq!(
        inp.get("disposition").and_then(Json::as_str),
        Some("pending{human}")
    );
    let rows = inp
        .get("flagged_pass_list")
        .and_then(Json::as_arr)
        .expect("flagged pass list");
    assert_eq!(rows.len(), 16);
    assert!(sidecar_ok(
        "docs/build/reports/2026-11-19-fp-w4-answer-scan.json"
    ));
    assert!(sidecar_ok(
        "docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json"
    ));
}

// ── the superseding packets ──────────────────────────────────────────

#[test]
fn p2_packet_supersedes_with_executed_evidence() {
    let p = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json");
    validate_w4_results_packet(&p).expect("p2 validates");
    assert_eq!(
        p.get("supersedes").and_then(Json::as_str),
        Some("docs/build/reports/2026-11-19-fp-results-p2-fp-frozen.json")
    );
    assert_eq!(
        p.get("packet_disposition").and_then(Json::as_str),
        Some("not_run{admission:AG3}")
    );
    let hs = p
        .get("harnesses")
        .and_then(Json::as_obj)
        .expect("harnesses");
    // mini-swe-agent landed: a computed theta + interval, not a
    // typed marker.
    let mini = &hs["mini-swe-agent"];
    assert_eq!(
        mini.get("disposition").and_then(Json::as_str),
        Some("executed")
    );
    let theta = mini.get("theta").and_then(Json::as_obj).expect("theta");
    assert_eq!(theta.get("hi_ppm").and_then(Json::as_int), Some(409978));
    // pi + claude-code stay unavailable members with the verbatim
    // not_run disposition — equal prominence, never dropped.
    for arm in ["pi", "claude-code"] {
        let h = &hs[arm];
        assert_eq!(
            h.get("disposition").and_then(Json::as_str),
            Some("not_run{admission:AG3}")
        );
        let t = h.get("theta").and_then(Json::as_obj).expect("theta");
        let hi = t.get("hi_ppm").and_then(Json::as_obj).expect("hi marker");
        assert_eq!(hi.get("state").and_then(Json::as_str), Some("refused"));
    }
    let fam = p
        .get("family")
        .and_then(|f| f.get("members"))
        .and_then(Json::as_arr)
        .expect("family members");
    assert_eq!(fam.len(), 2);
    for m in fam {
        assert_eq!(m.get("status").and_then(Json::as_str), Some("unavailable"));
        assert_eq!(
            m.get("disposition").and_then(Json::as_str),
            Some("not_run{admission:AG3}")
        );
    }
    assert!(!p.to_canonical_string().contains("protected_pair_truncated"));
    assert!(sidecar_ok(
        "docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json"
    ));
}

#[test]
fn p4_packet_records_failures_and_unavailable() {
    let p = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p4-fp.json");
    validate_w4_results_packet(&p).expect("p4 validates");
    let hs = p
        .get("harnesses")
        .and_then(Json::as_obj)
        .expect("harnesses");
    let nat = &hs["hh_native"];
    assert_eq!(
        nat.get("disposition").and_then(Json::as_str),
        Some("unavailable{verifier_recorded_no_verdict}")
    );
    // all 50 slots executed and served — the 4 rc=1 land in the
    // completion counts, not a withhold.
    let comp = nat
        .get("completion")
        .and_then(Json::as_obj)
        .expect("completion");
    assert_eq!(comp.get("first").and_then(Json::as_int), Some(50));
    let v5 = &hs["v5"];
    assert_eq!(
        v5.get("disposition").and_then(Json::as_str),
        Some("unavailable{verifier_unavailable:env_closed_or_unmounted}")
    );
    let comp = v5
        .get("completion")
        .and_then(Json::as_obj)
        .expect("v5 completion");
    assert_eq!(comp.get("first").and_then(Json::as_int), Some(20));
    assert_eq!(
        comp.get("dropped_checkpoint").and_then(Json::as_int),
        Some(30)
    );
    assert!(sidecar_ok(
        "docs/build/reports/2026-11-19-fp-w4-results-p4-fp.json"
    ));
}

#[test]
fn l2_audit_l3_typed_unavailable() {
    let l2 = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p3-l2.json");
    assert_eq!(l2.get("schema").and_then(Json::as_str), Some(L2_SCHEMA));
    assert_eq!(
        l2.get("admission")
            .and_then(|a| a.get("reason"))
            .and_then(Json::as_str),
        Some("unavailable{h_side_verdicts_absent}")
    );
    assert_eq!(
        l2.get("conjunction").and_then(|c| c.get("l1_gate_pass")),
        Some(&Json::Bool(true))
    );
    let audit = load_repo("docs/build/reports/2026-11-19-fp-w4-l2-audit.json");
    assert_eq!(
        audit.get("schema").and_then(Json::as_str),
        Some(L2_AUDIT_SCHEMA)
    );
    // the replay leg stays an honest not_computed; the seeded sample
    // is drawn over W4-O ids only.
    assert_eq!(
        audit
            .get("audit")
            .and_then(|a| a.get("state"))
            .and_then(Json::as_str),
        Some("not_computed")
    );
    let sampled = audit
        .get("sampled")
        .and_then(Json::as_arr)
        .expect("sampled");
    for s in sampled {
        let id = s.as_str().expect("sampled id");
        assert!(
            id.starts_with("w4-mini-swe-agent-"),
            "{id} is not a W4 O recording"
        );
    }
    let l3 = load_repo("docs/build/reports/2026-11-19-fp-w4-l3.json");
    assert_eq!(l3.get("schema").and_then(Json::as_str), Some(L3_SCHEMA));
    assert_eq!(
        l3.get("discordant_pairs")
            .and_then(|d| d.get("evaluation"))
            .and_then(|e| e.get("state"))
            .and_then(Json::as_str),
        Some("not_applicable")
    );
    for p in W4_PACKETS {
        assert!(sidecar_ok(p), "{p} sidecar");
    }
}

// ── the freeze veto ──────────────────────────────────────────────────

#[test]
fn freeze_refuses_unreviewed_flags() {
    let scan = load_repo("docs/build/reports/2026-11-19-fp-w4-answer-scan.json");
    for p in W4_PACKETS {
        let pkt = load_repo(p);
        let err = w4_freeze_packet(&pkt, &scan, "sha256:x", "sha256:y")
            .expect_err("unreviewed flags refuse the freeze");
        assert!(err.contains("unreviewed"), "{p}: {err}");
    }
}

#[test]
fn freeze_mints_on_zero_flags() {
    let scan = hharness_wire::json::parse(
        r#"{"schema":"hh-fp-answer-scan-report/1","coverage":{"flagged":0},"records":[]}"#,
    )
    .unwrap();
    let pkt = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json");
    let frozen = w4_freeze_packet(&pkt, &scan, "sha256:scan", "sha256:input")
        .expect("zero flags freeze trivially");
    let fr = frozen
        .get("freeze")
        .and_then(Json::as_obj)
        .expect("freeze member");
    assert_eq!(fr.get("state").and_then(Json::as_str), Some("frozen"));
    assert_eq!(
        fr.get("scan_sha256").and_then(Json::as_str),
        Some("sha256:scan")
    );
    let rv = frozen
        .get("review")
        .and_then(Json::as_obj)
        .expect("review member");
    assert_eq!(rv.get("flagged").and_then(Json::as_int), Some(0));
}

// ── R6.54: the reviewed freeze over HUMAN-H19's dispositions ─────────

fn sha_repo(rel: &str) -> String {
    format!(
        "sha256:{}",
        sha256_hex(&std::fs::read(repo_root().join(rel)).expect("read"))
    )
}

fn w4_review() -> Json {
    let scan = load_repo(W4_SCAN_REL);
    let analysis = load_repo(W4_ANALYSIS_REL);
    w4_flagged_review_record(
        &scan,
        &analysis,
        &sha_repo(W4_SCAN_REL),
        &sha_repo(W4_ANALYSIS_REL),
        &sha_repo(W4_REVIEW_INPUT_REL),
    )
    .expect("the committed review mints over the committed inputs")
}

#[test]
fn review_record_covers_every_flag() {
    let review = w4_review();
    assert_eq!(
        review.get("schema").and_then(Json::as_str),
        Some("hh-flagged-review/1")
    );
    let cov = review
        .get("coverage")
        .and_then(Json::as_obj)
        .expect("coverage");
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(32));
    assert_eq!(cov.get("confirmed").and_then(Json::as_int), Some(0));
    assert_eq!(cov.get("cleared").and_then(Json::as_int), Some(32));
    assert_eq!(
        review.get("readout").and_then(Json::as_str),
        Some("docs/build/readouts/HUMAN-H19.md")
    );
    assert_eq!(
        review.get("analysis_ref").and_then(Json::as_str),
        Some(W4_ANALYSIS_REL)
    );
    let rows = review
        .get("records")
        .and_then(Json::as_arr)
        .expect("records");
    assert_eq!(rows.len(), 32, "one row per flagged record");
    for r in rows {
        assert_eq!(
            r.get("disposition").and_then(Json::as_str),
            Some("cleared"),
            "H19's mass ruling"
        );
        assert!(
            r.get("reason")
                .and_then(Json::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "every row carries the per-flag reason (REQ-XD-071)"
        );
        assert!(
            r.get("excerpt_sha256")
                .and_then(Json::as_arr)
                .map(|a| !a.is_empty())
                .unwrap_or(false),
            "every row carries excerpt hashes (REQ-XD-071)"
        );
    }
    // the minted record byte-equals the committed record.
    let committed = load_repo(W4_REVIEW_REL);
    assert_eq!(
        review.to_canonical_string(),
        committed.to_canonical_string(),
        "review re-mints byte-identically"
    );
    assert!(sidecar_ok(W4_REVIEW_REL), "review sidecar");
}

#[test]
fn review_missing_a_flag_refuses() {
    let mut review = w4_review();
    if let Json::Obj(m) = &mut review {
        if let Some(Json::Arr(rows)) = m.get_mut("records") {
            rows.pop();
        }
    }
    let pkt = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json");
    let err = w4_freeze_reviewed_packet(&pkt, &review, "sha256:s", "sha256:i")
        .expect_err("an uncovered flag refuses");
    assert!(err.contains("records"), "{err}");
}

#[test]
fn review_confirmed_flag_refuses() {
    let mut review = w4_review();
    if let Json::Obj(m) = &mut review {
        if let Some(Json::Obj(cov)) = m.get_mut("coverage") {
            cov.insert("confirmed".to_string(), Json::Int(1));
            cov.insert("cleared".to_string(), Json::Int(31));
        }
    }
    let pkt = load_repo("docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json");
    let err = w4_freeze_reviewed_packet(&pkt, &review, "sha256:s", "sha256:i")
        .expect_err("a confirmed flag stays a veto");
    assert!(err.contains("confirmed") || err.contains("veto"), "{err}");
}

#[test]
fn reviewed_freeze_byte_equals_committed() {
    let review = w4_review();
    for p in W4_PACKETS {
        let pkt = load_repo(p);
        let frozen = w4_freeze_reviewed_packet(
            &pkt,
            &review,
            &sha_repo(W4_SCAN_REL),
            &sha_repo(W4_REVIEW_INPUT_REL),
        )
        .unwrap_or_else(|e| panic!("freeze {p}: {e}"));
        let committed = load_repo(&p.replace(".json", "-frozen.json"));
        assert_eq!(
            frozen.to_canonical_string(),
            committed.to_canonical_string(),
            "{p} frozen copy re-mints byte-identically"
        );
        // the W4 member-wise rule: verbatim + {freeze, review,
        // seal_open_ref} (the REQ-XH-121 citation the committed fold
        // lacks — the fold of record stays byte-stable).
        let diffs = w4_freeze_diff_errors(&pkt, &frozen);
        assert!(diffs.is_empty(), "{p} member diff: {diffs:?}");
        assert_eq!(
            frozen.get("seal_open_ref").and_then(Json::as_str),
            Some("docs/build/reports/2026-11-19-fp-seal-open.json"),
            "{p} seal-open citation (REQ-XH-121)"
        );
        let fr = frozen
            .get("freeze")
            .and_then(Json::as_obj)
            .expect("freeze member");
        assert_eq!(fr.get("state").and_then(Json::as_str), Some("frozen"));
        assert_eq!(fr.get("gate").and_then(Json::as_str), Some("ADR-0452"));
        assert_eq!(
            fr.get("review_ref").and_then(Json::as_str),
            Some(W4_REVIEW_REL)
        );
        let rv = frozen
            .get("review")
            .and_then(Json::as_obj)
            .expect("review member");
        assert_eq!(rv.get("flagged").and_then(Json::as_int), Some(32));
        assert_eq!(rv.get("cleared").and_then(Json::as_int), Some(32));
        assert_eq!(
            rv.get("readout").and_then(Json::as_str),
            Some("docs/build/readouts/HUMAN-H19.md")
        );
        assert!(
            sidecar_ok(&p.replace(".json", "-frozen.json")),
            "{p} sidecar"
        );
    }
}

// ── the population fold refuses score fabrication ────────────────────

#[test]
fn unscored_slots_resolve_oracle_failure_never_score() {
    let rows = vec![
        SlotOutcome {
            slot_id: "s1".into(),
            arm: "a".into(),
            task: "t".into(),
            replicate: 1,
            rc: 0,
            cost_micros: 10,
            wall_ms: 5,
            pass: Some(true),
            steps: None,
            tool_calls: None,
            errors: None,
        },
        SlotOutcome {
            slot_id: "s2".into(),
            arm: "a".into(),
            task: "t".into(),
            replicate: 2,
            rc: 1,
            cost_micros: 10,
            wall_ms: 5,
            pass: None, // settled, verifier produced no verdict
            steps: None,
            tool_calls: None,
            errors: None,
        },
    ];
    let pop = planned_population(&rows, "a", &BTreeMap::new()).expect("assemble");
    // the scored slot carries its recorded verdict; the unscored
    // slot resolves oracle_failure/unavailable — never coerced into
    // a pass or a scored fail.
    let s1 = pop.slots.iter().find(|s| s.plan_ref == "s1").expect("s1");
    let s2 = pop.slots.iter().find(|s| s.plan_ref == "s2").expect("s2");
    assert_eq!(s1.success, Some(true));
    assert_eq!(s1.outcome, Some(OutcomeClass::Scored));
    assert_eq!(s2.success, None);
    assert_eq!(s2.outcome, Some(OutcomeClass::OracleFailure));
}

// ── the superseded records stay append-only ──────────────────────────

#[test]
fn frozen_r6_48_packets_untouched() {
    for r in [
        "docs/build/reports/2026-11-19-fp-results-p2-fp-frozen.json",
        "docs/build/reports/2026-11-19-fp-results-p4-fp-frozen.json",
        "docs/build/reports/2026-11-19-fp-results-p3-l2-frozen.json",
        "docs/build/reports/2026-11-19-fp-l2-audit-frozen.json",
        "docs/build/reports/2026-11-19-fp-l3-frozen.json",
    ] {
        assert!(sidecar_ok(r), "{r} sidecar still verifies");
        let j = load_repo(r);
        // the historical withheld basis is still legible in the
        // frozen record — the new packet supersedes, it never
        // rewrites.
        assert!(
            j.to_canonical_string().contains("withheld"),
            "{r} keeps its historical withheld record"
        );
    }
}
