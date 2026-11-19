//! R6.54a — the dated computed `hh-fp-l2-audit/1` addendum battery.
//!
//! REQ-XD-119's owed L1-on-L2 leg executed offline over the retained
//! receipt-bound W4 O recordings: the frozen H (`sha256:c451ed97…`)
//! drove `hh_l1_drive --mode obs_replay --mswea-pack` against a strict
//! `hh-model-proxy` replay endpoint (`hh-l1-canon/1`, sequence keying,
//! first-divergence-fails; zero provider access, zero metered spend).
//! The addendum computes `audit.state` with the typed per-slot
//! verdicts; the frozen packet and its sidecar stay append-only and
//! the recorded L2 verdict is preserved.

use std::collections::BTreeSet;

use hharness_runner::fp_results::L2_AUDIT_SCHEMA;
use hharness_runner::fp_w4::{audit_sample, w4_audit_computed_packet, W4AuditLeg, AUDIT_SEED};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

const REPO: &str = env!("CARGO_MANIFEST_DIR");
const ADDENDUM: &str = "docs/build/reports/2026-11-19-fp-w4-l2-audit-addendum.json";
const BASE_AUDIT: &str = "docs/build/reports/2026-11-19-fp-w4-l2-audit.json";
const FROZEN_AUDIT: &str = "docs/build/reports/2026-11-19-fp-w4-l2-audit-frozen.json";
const H_IDENTITY: &str = "sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9";

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

/// The W4 O-recording population the committed window record binds —
/// the same `slot_results` arm fold the addendum emitter runs.
fn w4_o_ids() -> BTreeSet<String> {
    let w = load_repo("docs/build/reports/2026-11-19-fp-w4-window.json");
    w.get("slot_results")
        .and_then(Json::as_obj)
        .expect("window slot_results")
        .iter()
        .filter(|(_, s)| s.get("arm").and_then(Json::as_str) == Some("mini-swe-agent"))
        .map(|(k, _)| k.clone())
        .collect()
}

fn w2_ids() -> BTreeSet<String> {
    (1..=28)
        .map(|i| format!("l10-ws1-corpus-{i:02}-a"))
        .chain((1..=28).map(|i| format!("l10-ws1-corpus-{i:02}-b")))
        .collect()
}

fn leg(id: &str, verdict: &str, recorded: i64, exact: i64) -> W4AuditLeg {
    W4AuditLeg {
        slot_id: id.to_string(),
        recorded_calls: recorded,
        calls_exact: exact,
        verdict: verdict.to_string(),
        first_divergence: None,
        terminal: "completed{succeeded/scored — the submit-marker boundary}".into(),
        h_run_id: "run-test".into(),
        evidence: vec![(
            format!("retained:l1-audit/legs/{id}/raws.json"),
            "sha256:x".into(),
        )],
    }
}

fn emit(legs: &[W4AuditLeg]) -> Result<Json, hharness_runner::fp_results::AuditError> {
    w4_audit_computed_packet(
        &w4_o_ids(),
        &w2_ids(),
        AUDIT_SEED,
        "2026-11-19-fp-l2-audit-frozen.json",
        "sha256:w",
        Json::Arr(vec![]),
        H_IDENTITY,
        "sha256:frozen",
        legs,
    )
}

// ── the committed addendum ───────────────────────────────────────────

#[test]
fn audit_addendum_binds_sample_and_state() {
    let add = load_repo(ADDENDUM);
    let base = load_repo(BASE_AUDIT);
    let frozen = load_repo(FROZEN_AUDIT);
    assert_eq!(
        add.get("schema").and_then(Json::as_str),
        Some(L2_AUDIT_SCHEMA)
    );
    assert_eq!(add.get("req").and_then(Json::as_str), Some("REQ-XD-119"));
    assert_eq!(
        add.get("audit")
            .and_then(|a| a.get("state"))
            .and_then(Json::as_str),
        Some("computed")
    );
    // seed + seeded sample re-derive byte-identically against the
    // frozen and base packets.
    assert_eq!(
        add.get("seed").and_then(Json::as_int),
        Some(AUDIT_SEED as i64)
    );
    assert_eq!(add.get("sampled"), base.get("sampled"));
    assert_eq!(add.get("sampled"), frozen.get("sampled"));
    assert_eq!(add.get("population_n").and_then(Json::as_int), Some(50));
    assert_eq!(add.get("sampled_n").and_then(Json::as_int), Some(5));
    // the frozen H identity + the binary-set citation.
    let audit = add.get("audit").expect("audit member");
    assert_eq!(
        audit.get("h_identity").and_then(Json::as_str),
        Some(H_IDENTITY)
    );
    assert_eq!(
        audit.get("h_binary_set").and_then(Json::as_str),
        Some("docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json")
    );
    assert_eq!(audit.get("first_divergence_fails"), Some(&Json::Bool(true)));
    // the supersedes/supersession members bind identically to the base
    // packet; `sources` is the base member verbatim; `amends` cites the
    // frozen packet's real sha256.
    assert_eq!(add.get("supersedes"), base.get("supersedes"));
    assert_eq!(add.get("supersession"), base.get("supersession"));
    assert_eq!(add.get("sources"), base.get("sources"));
    let amends = add.get("amends").expect("amends");
    assert_eq!(
        amends.get("path").and_then(Json::as_str),
        Some(FROZEN_AUDIT)
    );
    let frozen_bytes = std::fs::read(repo_root().join(FROZEN_AUDIT)).unwrap();
    assert_eq!(
        amends.get("sha256").and_then(Json::as_str),
        Some(format!("sha256:{}", sha256_hex(&frozen_bytes))).as_deref()
    );
    assert_eq!(add.get("ticket").and_then(Json::as_str), Some("R6.54a"));
    // sidecar verifies; the frozen packet's own sidecar is untouched.
    assert!(sidecar_ok(ADDENDUM), "addendum sidecar");
    assert!(sidecar_ok(FROZEN_AUDIT), "frozen sidecar");
    assert!(sidecar_ok(BASE_AUDIT), "base sidecar");
}

#[test]
fn audit_addendum_leg_rows_typed() {
    let add = load_repo(ADDENDUM);
    let legs = add
        .get("audit")
        .and_then(|a| a.get("legs"))
        .and_then(Json::as_arr)
        .expect("legs");
    assert_eq!(legs.len(), 5);
    let verdict = add.get("verdict").and_then(Json::as_str).unwrap_or("");
    assert!(verdict.starts_with("pass{"), "{verdict}");
    for l in legs {
        let v = l.get("verdict").and_then(Json::as_str).unwrap_or("");
        // every committed leg is pass|diverged|unavailable — never a
        // bare bool, never an untyped score.
        assert!(
            v == "pass" || v.starts_with("diverged{") || v.starts_with("unavailable{"),
            "{v}"
        );
        let recorded = l.get("recorded_calls").and_then(Json::as_int).unwrap_or(-1);
        let exact = l.get("calls_exact").and_then(Json::as_int).unwrap_or(-1);
        if v == "pass" {
            assert_eq!(exact, recorded, "pass leg must be fully exact");
            assert!(recorded > 0);
        }
        if v.starts_with("diverged{") {
            assert!(
                l.get("first_divergence").is_some(),
                "diverged leg must carry first-divergence detail"
            );
        }
        for e in l
            .get("evidence")
            .and_then(Json::as_arr)
            .cloned()
            .unwrap_or_default()
        {
            let p = e.get("path").and_then(Json::as_str).unwrap_or("");
            assert!(
                p.starts_with("retained:l1-audit/") || p.starts_with("docs/"),
                "{p}"
            );
            assert!(
                e.get("sha256")
                    .and_then(Json::as_str)
                    .map(|s| s.starts_with("sha256:"))
                    .unwrap_or(false),
                "{p} sha"
            );
        }
    }
}

// ── the emitter contract ─────────────────────────────────────────────

#[test]
fn audit_computed_leg_coverage_is_exact() {
    let sample = audit_sample(&w4_o_ids().into_iter().collect::<Vec<_>>(), AUDIT_SEED, 5);
    // a leg outside the sample refuses — no substitution.
    let mut legs: Vec<W4AuditLeg> = sample.iter().map(|id| leg(id, "pass", 2, 2)).collect();
    legs.push(leg("w4-mini-swe-agent-headless-terminal-r1", "pass", 2, 2));
    match emit(&legs) {
        Err(hharness_runner::fp_results::AuditError::LegOutsideSample { .. }) => {}
        other => panic!("expected LegOutsideSample — {other:?}"),
    }
    // a missing leg refuses — an unserved slot folds upstream as
    // `unavailable{evidence}`, never dropped here.
    let legs: Vec<W4AuditLeg> = sample[1..].iter().map(|id| leg(id, "pass", 2, 2)).collect();
    match emit(&legs) {
        Err(hharness_runner::fp_results::AuditError::SampleMissingLeg { .. }) => {}
        other => panic!("expected SampleMissingLeg — {other:?}"),
    }
    // a W2-corpus id inside the declared W4-O population still refuses
    // at decode when it lands in the sample.
    let mut ids = w4_o_ids();
    let w2 = w2_ids().into_iter().next().unwrap();
    ids.insert(w2.clone());
    let sample2 = audit_sample(&ids.iter().cloned().collect::<Vec<_>>(), AUDIT_SEED, 5);
    if sample2.contains(&w2) {
        match w4_audit_computed_packet(
            &ids,
            &w2_ids(),
            AUDIT_SEED,
            "x",
            "sha256:w",
            Json::Arr(vec![]),
            H_IDENTITY,
            "sha256:f",
            &[],
        ) {
            Err(hharness_runner::fp_results::AuditError::NotW4O { .. }) => {}
            other => panic!("expected NotW4O — {other:?}"),
        }
    }
}

#[test]
fn audit_computed_verdict_composition() {
    let sample = audit_sample(&w4_o_ids().into_iter().collect::<Vec<_>>(), AUDIT_SEED, 5);
    // all pass → pass{N/N …}
    let legs: Vec<W4AuditLeg> = sample.iter().map(|id| leg(id, "pass", 4, 4)).collect();
    let pkt = emit(&legs).expect("all-pass emit");
    assert!(
        pkt.get("verdict")
            .and_then(Json::as_str)
            .unwrap_or("")
            .starts_with("pass{"),
        "all-pass verdict"
    );
    // first-divergence-fails — a diverged leg moves the top verdict to
    // the typed divergence and the leg row carries the detail.
    let mut legs: Vec<W4AuditLeg> = sample.iter().map(|id| leg(id, "pass", 4, 4)).collect();
    legs[0].verdict = "diverged{step=3, class=upstream-bound messages[5].content}".into();
    legs[0].first_divergence = Some(Json::obj([
        ("k", Json::Int(3)),
        ("class", Json::str("upstream-bound")),
        ("first_diff_path", Json::str("messages[5].content")),
    ]));
    legs[0].calls_exact = 3;
    let pkt = emit(&legs).expect("diverged emit");
    assert!(
        pkt.get("verdict")
            .and_then(Json::as_str)
            .unwrap_or("")
            .starts_with("diverged{"),
        "first-divergence-fails verdict"
    );
    let row = pkt
        .get("audit")
        .and_then(|a| a.get("legs"))
        .and_then(Json::as_arr)
        .expect("legs")
        .first()
        .cloned()
        .unwrap();
    assert!(row.get("first_divergence").is_some());
    // typed unavailability is honest, not a pass.
    let mut legs: Vec<W4AuditLeg> = sample.iter().map(|id| leg(id, "pass", 4, 4)).collect();
    legs[0].verdict = "unavailable{evidence:no_receipts}".into();
    legs[0].recorded_calls = 0;
    legs[0].calls_exact = 0;
    let pkt = emit(&legs).expect("unavailable emit");
    assert!(
        pkt.get("verdict")
            .and_then(Json::as_str)
            .unwrap_or("")
            .starts_with("unavailable{"),
        "unavailable verdict"
    );
}
