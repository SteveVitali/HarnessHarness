//! R6.27 stage 2 — `answer-scan/1`, the request-fingerprint check and
//! the flagged-pass review-record schema (AC-4 `AC-6.9-1`/REQ-XD-070
//! absorbing REQ-XB-016/XC-319/XD-223; AC-5 `AC-5.3-8`/REQ-XC-313;
//! AC-6/REQ-XD-071 contributing).

use hharness_runner::admission::{self, ValidationContext};
use hharness_runner::answer_scan::*;
use hharness_wire::json::Json;

fn fp_corpus() -> FingerprintCorpus {
    let mut lines = std::collections::BTreeMap::new();
    lines.insert(
        "solution".to_string(),
        vec!["export ANSWER = solve the maze with Dijkstra then exit zero".to_string()],
    );
    lines.insert(
        "test".to_string(),
        vec!["assert output.strip() == expected_canonical_form_42".to_string()],
    );
    lines.insert(
        "readme".to_string(),
        vec!["this task asks you to repair the broken nginx reverse proxy".to_string()],
    );
    let mut shingles = std::collections::BTreeMap::new();
    shingles.insert("solution".to_string(), vec![]);
    FingerprintCorpus {
        task_id: "t0".into(),
        lines,
        shingles,
    }
}

fn base_input() -> ScanInput {
    ScanInput {
        slot_ref: "slot:fixture".into(),
        task_id: "t0".into(),
        received_requests: vec![
            (
                "{\"messages\":[{\"role\":\"user\",\"content\":\"fix the service\"}]}".into(),
                Some("litellm/1.103.5".into()),
                vec![
                    "authorization".into(),
                    "content-type".into(),
                    "user-agent".into(),
                ],
            ),
            (
                "{\"messages\":[{\"role\":\"user\",\"content\":\"still failing\"}]}".into(),
                Some("litellm/1.103.5".into()),
                vec![
                    "authorization".into(),
                    "content-type".into(),
                    "user-agent".into(),
                ],
            ),
        ],
        egress_rows: vec![],
        seal_present: true,
        native_paths: vec!["agent.log".into(), "run.sh".into()],
        transcript: "$ mini-swe-agent fix the service\n…ok".into(),
    }
}

fn expected_fp() -> RequestFingerprint {
    RequestFingerprint {
        user_agent: "litellm/1.103.5".into(),
        header_names: vec![
            "authorization".into(),
            "content-type".into(),
            "user-agent".into(),
        ],
    }
}

/// AC-4 (AC-6.9-1; REQ-XD-070 + absorbed REQ-XB-016/XC-319/XD-223):
/// the seeded-leak battery is detected from receipts alone; clean
/// controls stay unflagged; a requested-only fetch is reported, never
/// scored; a missing scan is unresolved; a confirmed leak vetoes.
#[test]
fn ac4_seeded_leak_battery_detected() {
    // Planted solution excerpt in a model-received request body.
    let mut planted = base_input();
    planted.received_requests[0].0 = format!(
        "{{\"messages\":[{{\"content\":\"the file says export ANSWER = solve the maze   with Dijkstra then exit zero\"}}]}}"
    );
    let r = scan(&planted, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert_eq!(r.state, "flagged");
    assert!(r.hits.iter().any(|h| h.signal == "fingerprint:solution"));
    assert_eq!(r.detector, ANSWER_SCAN_VERSION);
    // Canary GUID.
    let mut canary = base_input();
    canary.received_requests[1].0 = format!("body with {CANARY_GUID} inside");
    let r = scan(&canary, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r.hits.iter().any(|h| h.signal == "canary"));
    // Repository signature (clone URL) in a received body.
    let mut repo = base_input();
    repo.received_requests[0].0 =
        "git clone https://github.com/harbor-framework/terminal-bench-2-1 x".into();
    let r = scan(&repo, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r.hits.iter().any(|h| h.signal == "repo_solution"));
    // Served repository fetch flags; a requested-only fetch reports.
    let mut eg = base_input();
    eg.egress_rows = vec![
        (
            "github.com/harbor-framework/terminal-bench-2-1".into(),
            false,
        ),
        ("pypi.org".into(), true),
    ];
    let r = scan(&eg, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert_eq!(r.state, "clean");
    assert_eq!(r.requested_only.len(), 1);
    eg.egress_rows[0].1 = true;
    let r = scan(&eg, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r
        .hits
        .iter()
        .any(|h| h.signal == "repo_solution" && h.scored));
    // Reward-file / verifier writes.
    let mut tamper = base_input();
    tamper
        .native_paths
        .push("\u{2f}logs\u{2f}verifier\u{2f}reward.json".into());
    let r = scan(&tamper, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r.hits.iter().any(|h| h.signal == "tamper:native"));
    let mut tt = base_input();
    tt.transcript = "$ echo pass > \u{2f}tests\u{2f}conftest.py".into();
    let r = scan(&tt, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r.hits.iter().any(|h| h.signal == "tamper:transcript"));
    // Clean control stays unflagged.
    let r = scan(&base_input(), Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert_eq!(r.state, "clean");
    assert!(r.hits.iter().all(|h| !h.scored));
    // Missing scan → unresolved; confirmed → veto; cleared → clean.
    assert_eq!(scored_state(false, true, false, false), "unresolved");
    assert_eq!(scored_state(true, true, false, true), "veto");
    assert_eq!(scored_state(true, true, true, false), "clean");
    assert_eq!(
        scored_state(true, true, false, false),
        "veto_pending_review"
    );
    // veto_for: cleared yields no observed-veto member.
    let flagged = scan(&planted, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(!veto_for(&flagged, false).is_empty());
    assert!(veto_for(&flagged, true).is_empty());
}

/// AC-5 (AC-5.3-8; REQ-XC-313): a fingerprint-mismatch receipt is
/// flagged; an `in_environment` admission without the residual
/// refuses; the committed admission records carry the fingerprint.
#[test]
fn ac5_request_fingerprint() {
    let mut m = base_input();
    m.received_requests[0].1 = Some("curl/8.5.0".into()); // wrong UA
    let r = scan(&m, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r
        .hits
        .iter()
        .any(|h| h.signal == "request_fingerprint_mismatch"));
    m.received_requests[0].1 = Some("litellm/1.103.5".into());
    m.received_requests[0].2 = vec!["authorization".into(), "content-type".into()];
    let r = scan(&m, Some(&fp_corpus()), Some(&expected_fp())).unwrap();
    assert!(r
        .hits
        .iter()
        .any(|h| h.signal == "request_fingerprint_mismatch"));
    // The committed admission records carry request_fingerprint and
    // the in_environment residual; stripping either refuses.
    for h in ["mini-swe-agent", "pi", "claude-code"] {
        let txt = std::fs::read_to_string(format!("tests/fixtures/r6_22/{h}.json")).unwrap();
        let rec = hharness_wire::json::parse(&txt).unwrap();
        admission::validate(&rec, &ValidationContext::first_pass())
            .unwrap_or_else(|e| panic!("{h}: {e}"));
        let fp = rec.get("request_fingerprint").expect("fingerprint member");
        assert!(fp.get("user_agent").and_then(Json::as_str).is_some());
        let mut stripped = rec.clone();
        if let Json::Obj(mm) = &mut stripped {
            mm.remove("request_fingerprint");
        }
        assert_eq!(
            admission::validate(&stripped, &ValidationContext::first_pass())
                .unwrap_err()
                .rule,
            "member_missing"
        );
        // in_environment without the residual declaration refuses.
        let mut nores = rec.clone();
        if let Json::Obj(mm) = &mut nores {
            if let Json::Obj(p) = mm.get_mut("posture").unwrap() {
                p.insert("residuals".into(), Json::Arr(vec![]));
            }
        }
        assert_eq!(
            admission::validate(&nores, &ValidationContext::first_pass())
                .unwrap_err()
                .rule,
            "posture_residual"
        );
    }
}

/// AC-6 (REQ-XD-071 contributing): the review-record schema validates
/// a complete fixture and refuses one missing reason or excerpt hash;
/// reports carry the counts, detector, coverage and the minimum label.
#[test]
fn ac6_review_record_schema() {
    let sha = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let good = Json::obj([
        ("schema", Json::str(REVIEW_SCHEMA)),
        ("slot_ref", Json::str("slot:fixture")),
        ("detector", Json::str(ANSWER_SCAN_VERSION)),
        ("harness_masked", Json::Bool(true)),
        (
            "flags",
            Json::Arr(vec![Json::obj([
                ("disposition", Json::str("confirmed")),
                ("reason", Json::str("solution excerpt in request body")),
                ("excerpt_sha256", Json::str(sha)),
            ])]),
        ),
    ]);
    assert!(validate_review_record(&good).is_ok());
    // Missing reason refuses.
    let mut bad = good.clone();
    if let Json::Obj(m) = &mut bad {
        if let Json::Arr(f) = m.get_mut("flags").unwrap() {
            if let Json::Obj(fm) = &mut f[0] {
                fm.remove("reason");
            }
        }
    }
    assert!(!validate_review_record(&bad).unwrap_err().is_empty());
    // Missing excerpt hash refuses.
    let mut bad2 = good.clone();
    if let Json::Obj(m) = &mut bad2 {
        if let Json::Arr(f) = m.get_mut("flags").unwrap() {
            if let Json::Obj(fm) = &mut f[0] {
                fm.remove("excerpt_sha256");
            }
        }
    }
    assert!(!validate_review_record(&bad2).unwrap_err().is_empty());
    // A raw harness member never appears; unmasked needs a basis.
    let mut h = good.clone();
    if let Json::Obj(m) = &mut h {
        m.insert("harness".into(), Json::str("mini-swe-agent"));
    }
    assert!(!validate_review_record(&h).unwrap_err().is_empty());
    let rep = flagged_report((10, 3), &[good]).unwrap();
    assert_eq!(
        rep.get("label").and_then(Json::as_str),
        Some(LABEL_CONFIRMED_MIN)
    );
    assert_eq!(rep.get("confirmed").and_then(Json::as_int), Some(1));
    assert_eq!(rep.get("flagged").and_then(Json::as_int), Some(3));
}
