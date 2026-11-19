//! R6.48 — FP-RESULTS battery (manifest 314).
//!
//! AC-1 `AC-6.5-1`/REQ-XH-121 — the outcome seal opens exactly once;
//! the open act is a recorded firewall over every pre-existing
//! sealed consumer.
//! AC-2 REQ-XD-070/REQ-XD-071 — `answer-scan/1` over every scored
//! trial; the flagged-pass review handoff; a missing scan is
//! `unresolved`, never clean.
//! AC-3/AC-4 REQ-XD-005/REQ-XD-090/REQ-XD-030 — the
//! `hh-comparative-results/1` withheld packets keep the computed
//! packet's members verbatim with typed markers at uncomputable
//! leaves; unavailable Holm members stay in the family.
//! AC-5 REQ-XD-119/REQ-XD-120 — P3-L2 withheld, the L1-on-L2 audit's
//! W2-corpus guard, the L3 report's discordance listing.
//! AC-6 REQ-XH-020/REQ-XF-122 — typed markers never `null`; the
//! emitted records' sha256 sidecars recompute.
//! AC-7 — golden fixtures for `run{full}`, `fallback`,
//! `end_after_w2`, `re-scope{end-first-pass-early}` all validate.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators::validate_results_packet;
use hharness_runner::answer_scan::{FingerprintCorpus, ScanInput, ANSWER_SCAN_VERSION};
use hharness_runner::fp_results::*;
use hharness_runner::fp_seal::{SealClass, SealError};
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
    hharness_wire::json::parse(
        &std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

// ── AC-1 ─────────────────────────────────────────────────────────────

#[test]
fn ac1_seal_open_passes_masked_consumers() {
    let masked = hharness_wire::json::parse(
        r#"{"schema":"hh-pp0-ag-evidence/1","outcome":"withheld{sealed_until_results}",
            "ag":{"AG3":"pass"},"trial_ref":"ws1-pp0-2"}"#,
    )
    .unwrap();
    let cost_only = hharness_wire::json::parse(
        r#"{"schema":"hh-fp-trials/1","trials":[{"slot":"s1","cost_micros":42}]}"#,
    )
    .unwrap();
    let rec = seal_open_record(
        &[
            SealCheck {
                path: "fixture:pp0".into(),
                record: masked,
                class: SealClass::Pp0AgEvidence,
            },
            SealCheck {
                path: "fixture:trials".into(),
                record: cost_only,
                class: SealClass::AgentSlot,
            },
        ],
        &[(
            "docs/build/readouts/GATE-G29.md".into(),
            "masking — no first-pass outcome field is cited".into(),
        )],
    )
    .expect("masked consumers pass");
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(SEAL_OPEN_SCHEMA)
    );
    assert_eq!(rec.get("opens").and_then(Json::as_int), Some(1));
    assert_eq!(
        rec.get("opened_classes")
            .and_then(Json::as_arr)
            .map(|a| a.len()),
        Some(1)
    );
    let checks = rec
        .get("precedence_check")
        .and_then(|c| c.get("records"))
        .and_then(Json::as_arr)
        .expect("records member");
    assert_eq!(checks.len(), 2);
    assert!(checks
        .iter()
        .all(|c| c.get("check").and_then(Json::as_str) == Some("pass")));
}

#[test]
fn ac1_seal_open_refuses_an_early_outcome_read() {
    // a fixture record carrying an unmasked outcome field BEFORE the
    // open act — the firewall refuses the open (the AC-1 fail leg).
    let leaked = hharness_wire::json::parse(
        r#"{"schema":"hh-fp-trials/1","trials":[{"slot":"s1","verdict":"pass"}]}"#,
    )
    .unwrap();
    let err = seal_open_record(
        &[SealCheck {
            path: "fixture:leak".into(),
            record: leaked,
            class: SealClass::AgentSlot,
        }],
        &[],
    )
    .expect_err("an unmasked outcome member refuses the open");
    match err {
        SealError::OutcomeField { path } => assert!(path.contains("verdict"), "{path}"),
        other => panic!("expected OutcomeField, got {other:?}"),
    }
}

#[test]
fn ac1_seal_open_committed_records_pass() {
    // the real firewall leg: every committed consumer record passes
    // its class check under the open act.
    let checks = vec![
        SealCheck {
            path: "docs/build/reports/2026-11-19-fp-ws1-trials.json".into(),
            record: load_repo("docs/build/reports/2026-11-19-fp-ws1-trials.json"),
            class: SealClass::AgentSlot,
        },
        SealCheck {
            path: "docs/build/reports/2026-11-19-fp-ws1-pp0-evidence.json".into(),
            record: load_repo("docs/build/reports/2026-11-19-fp-ws1-pp0-evidence.json"),
            class: SealClass::Pp0AgEvidence,
        },
        SealCheck {
            path: "docs/build/reports/2026-11-19-fp-ws1-measurement.json".into(),
            record: load_repo("docs/build/reports/2026-11-19-fp-ws1-measurement.json"),
            class: SealClass::AgentSlot,
        },
        SealCheck {
            path: "docs/build/reports/2026-11-10-fp-prereg.json".into(),
            record: load_repo("docs/build/reports/2026-11-10-fp-prereg.json"),
            class: SealClass::AgentSlot,
        },
    ];
    seal_open_record(&checks, &[]).expect("every committed consumer passes its class check");
}

// ── AC-2 ─────────────────────────────────────────────────────────────

fn slot(slot: &str, pass: bool, mut input: ScanInput) -> SlotScan {
    input.slot_ref = slot.into();
    SlotScan {
        slot_ref: slot.into(),
        task_id: "t-fixture".into(),
        arm: "fixture-arm".into(),
        pass,
        kind: "subject".into(),
        input,
        corpus: Some(FingerprintCorpus {
            task_id: "t-fixture".into(),
            lines: BTreeMap::from([(
                "solution".into(),
                vec!["the canonical fixture answer line 0123456789".into()],
            )]),
            shingles: BTreeMap::new(),
        }),
        receipt_refs: vec!["sha256:deadbeef".into()],
        member_digests: vec![("seal".into(), "sha256:c0ffee".into())],
    }
}

fn base_input() -> ScanInput {
    ScanInput {
        slot_ref: "s".into(),
        task_id: "t-fixture".into(),
        received_requests: vec![(
            "{\"messages\":[{\"role\":\"user\",\"content\":\"fix it\"}]}".into(),
            None,
            vec![],
        )],
        egress_rows: vec![],
        seal_present: true,
        native_paths: vec!["agent.log".into()],
        transcript: "$ run it\n".into(),
    }
}

#[test]
fn ac2_scan_report_coverage_flagged_unresolved() {
    let mut flagged = base_input();
    flagged.received_requests.push((
        "the canonical fixture answer line 0123456789".into(),
        None,
        vec![],
    ));
    let incomplete = ScanInput {
        slot_ref: "s3".into(),
        task_id: "t".into(),
        received_requests: vec![],
        egress_rows: vec![],
        seal_present: false,
        native_paths: vec![],
        transcript: String::new(),
    };
    let report = scan_report(
        &[
            slot("s1", false, base_input()),
            slot("s2", true, flagged),
            slot("s3", false, incomplete),
        ],
        vec![("fixture".into(), "sha256:x".into())],
    )
    .expect("report emits");
    let cov = report.get("coverage").unwrap();
    assert_eq!(cov.get("scored_trials").and_then(Json::as_int), Some(3));
    assert_eq!(cov.get("scanned").and_then(Json::as_int), Some(2));
    assert_eq!(cov.get("unresolved").and_then(Json::as_int), Some(1));
    assert_eq!(cov.get("flagged").and_then(Json::as_int), Some(1));
    // the missing scan is unresolved, never clean.
    let un = report
        .get("unresolved")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .find(|u| u.get("slot_ref").and_then(Json::as_str) == Some("s3"))
        .expect("s3 unresolved");
    assert_eq!(un.get("state").and_then(Json::as_str), Some("unresolved"));
    // flagged pass on the list — masked identity, excerpt hashes only.
    let rows = report
        .get("flagged_pass_list")
        .and_then(|f| f.get("rows"))
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("harness_masked").and_then(Json::as_bool),
        Some(true)
    );
    assert!(rows[0]
        .get("excerpt_sha256s")
        .and_then(Json::as_arr)
        .map(|a| a.iter().all(|h| h
            .as_str()
            .map(|s| s.starts_with("sha256:"))
            .unwrap_or(false)))
        .unwrap_or(false));
    // the flagged pass is veto-pending (never cleared without review).
    let recs = report.get("records").and_then(Json::as_arr).unwrap();
    let s2 = recs
        .iter()
        .find(|r| r.get("slot_ref").and_then(Json::as_str) == Some("s2"))
        .unwrap();
    assert_eq!(
        s2.get("scored_state").and_then(Json::as_str),
        Some("veto_pending_review")
    );
    // REQ-XD-071 report members.
    let rr = report.get("review_report").unwrap();
    assert_eq!(
        rr.get("label").and_then(Json::as_str),
        Some("the confirmed count is a minimum")
    );
    assert_eq!(rr.get("confirmed").and_then(Json::as_int), Some(0));
    assert_eq!(
        report
            .get("review_handoff")
            .and_then(|h| h.get("input_to"))
            .and_then(Json::as_str),
        Some("HUMAN-H16")
    );
    assert_eq!(
        report.get("detector").and_then(Json::as_str),
        Some(ANSWER_SCAN_VERSION)
    );
    // no matched bytes anywhere — every hit member is a digest.
    let canon = report.to_canonical_string();
    assert!(
        !canon.contains("canonical fixture answer"),
        "matched bytes leak"
    );
}

// ── AC-3 / AC-4 / AC-6 ───────────────────────────────────────────────

fn fixture_sources() -> Json {
    Json::Arr(vec![Json::obj([
        ("path", Json::str("fixture")),
        ("sha256", Json::str("sha256:x")),
    ])])
}

#[test]
fn ac3_p2_fp_withheld_packet_validates_and_retains_family() {
    let p = withheld_results_packet(
        "P2-FP",
        "mini-swe-agent",
        &p2_arms(),
        &p2_family(),
        &p2_exploratory(),
        "expected",
        fixture_sources(),
    );
    validate_withheld_results_packet(&p)
        .unwrap_or_else(|e| panic!("withheld packet must validate: {e:?}"));
    // the computed validator refuses it — a withheld packet is never
    // mistaken for a computed result.
    assert!(validate_results_packet(&p).is_err());
    assert_eq!(
        p.get("packet_disposition").and_then(Json::as_str),
        Some(CLAIM_REFUSAL)
    );
    // REQ-XD-030 — the unavailable members stay in the Holm family
    // with their verbatim typed reasons.
    let members = p
        .get("family")
        .and_then(|f| f.get("members"))
        .and_then(Json::as_arr)
        .unwrap();
    let mut disps: BTreeMap<String, String> = BTreeMap::new();
    for m in members {
        disps.insert(
            m.get("member").and_then(Json::as_str).unwrap().to_string(),
            m.get("disposition")
                .and_then(Json::as_str)
                .unwrap()
                .to_string(),
        );
        assert_eq!(m.get("status").and_then(Json::as_str), Some("unavailable"));
    }
    assert_eq!(disps["pi"], "withheld{admission:AG3}");
    assert_eq!(disps["claude-code"], "withheld{admission:AG3}");
    // per-arm dispositions verbatim.
    let hs = p.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(hs.len(), 3);
    assert_eq!(
        hs["pi"].get("disposition").and_then(Json::as_str),
        Some("withheld{admission:AG3}")
    );
    assert_eq!(
        hs["mini-swe-agent"]
            .get("disposition")
            .and_then(Json::as_str),
        Some("withheld{window_deadline}")
    );
    // no silent-zero result — every uncomputable leaf is a typed marker
    // (the family's `raw_p_ppm: null` is the schema's own sanctioned
    // absent spelling; `disposition` carries the typed reason).
    let canon = p.to_canonical_string();
    assert!(!canon.contains("raw_p_ppm\":{"));
    assert!(is_typed_marker(hs["pi"].get("delta_vs_ref").unwrap()));
}

/// `structure_ok(computed, withheld, path, errs)` — equal structure
/// and prominence: every member a computed row carries exists in the
/// withheld row at the same position; a typed marker
/// (`{state, reason}`) at the withheld leaf discharges the subtree —
/// that is the REQ-XH-020 marker discipline, not a dropped member.
/// Dynamic-key maps (participant spellings, harness pair names) compare
/// by presence and non-emptiness.
fn structure_ok(c: &Json, w: &Json, path: &str, errs: &mut Vec<String>) {
    if is_typed_marker(w) {
        return;
    }
    if let (Json::Obj(cm), Json::Obj(wm)) = (c, w) {
        // member maps whose keys are declared values, not schema
        // members — presence + non-empty is the structure check.
        let dynamic = [
            "counts.intended",
            "counts.served",
            "flips.swap_vs",
            "flips.harness_excess",
            "labels",
            "harnesses",
        ];
        if dynamic.iter().any(|d| path.ends_with(d)) {
            if wm.is_empty() {
                errs.push(format!("{path}: dynamic member map empty"));
            }
            return;
        }
        for (k, cv) in cm {
            let q = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            match wm.get(k) {
                None => errs.push(format!("{q}: missing in the withheld row")),
                Some(wv) => structure_ok(cv, wv, &q, errs),
            }
        }
    }
}

#[test]
fn ac3_withheld_structure_matches_computed() {
    let computed_arm = {
        let g = golden_computed_packet();
        g.get("harnesses").and_then(Json::as_obj).unwrap()["fixture-arm"].clone()
    };
    let withheld = withheld_results_packet(
        "P2-FP",
        "mini-swe-agent",
        &p2_arms(),
        &p2_family(),
        &p2_exploratory(),
        "e",
        fixture_sources(),
    );
    let warm = withheld.get("harnesses").and_then(Json::as_obj).unwrap()["pi"].clone();
    let mut errs: Vec<String> = Vec::new();
    structure_ok(&computed_arm, &warm, "", &mut errs);
    assert!(errs.is_empty(), "withheld row drops members: {errs:?}");
    // and every required member is present verbatim.
    let shape_w = key_shape(&warm);
    for member in [
        "theta.ppm",
        "theta.interval",
        "delta_vs_ref",
        "leader_group.member",
        "pass_at.k",
        "flips.rerun",
        "cost.per_trial.micros",
        "counts.served",
        "completion.planned",
        "integrity.detector",
    ] {
        assert!(shape_w.contains(member), "withheld arm drops {member}");
    }
}

#[test]
fn ac4_p4_fp_withheld_packet() {
    let p = withheld_results_packet(
        "P4-FP",
        "hh_native",
        &p4_arms(),
        &p4_family(),
        &p4_exploratory(),
        "expected",
        fixture_sources(),
    );
    validate_withheld_results_packet(&p)
        .unwrap_or_else(|e| panic!("p4 withheld packet must validate: {e:?}"));
    let hs = p.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs["v5"].get("disposition").and_then(Json::as_str),
        Some("withheld{budget}")
    );
    assert_eq!(
        hs["hh_native"].get("disposition").and_then(Json::as_str),
        Some("withheld{window_deadline}")
    );
    // no favorable-effect claim is asserted — no computed delta.
    assert!(is_typed_marker(hs["v5"].get("delta_vs_ref").unwrap()));
}

// ── AC-5 ─────────────────────────────────────────────────────────────

#[test]
fn ac5_p3_l2_withheld_packet() {
    let p = l2_withheld_packet(
        "l1-pass:sha256:80bfafee35ac9dd669d80f786ee54deca9948049bcd8559dd7a6dee8d06dd8ef",
        "sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9",
        "sha256:072de1f36f3ddc6bd8387bfde992554e7c892b05f2e5d9590c30e221f028cb0d",
        "expected",
        fixture_sources(),
    );
    assert_eq!(p.get("verdict").and_then(Json::as_str), Some(CLAIM_REFUSAL));
    assert_eq!(
        p.get("packet_disposition").and_then(Json::as_str),
        Some(CLAIM_REFUSAL)
    );
    assert_eq!(
        p.get("admission")
            .and_then(|a| a.get("reason"))
            .and_then(Json::as_str),
        Some(CLAIM_REFUSAL)
    );
    assert_eq!(p.get("served_n").and_then(Json::as_int), Some(0));
    assert_eq!(p.get("planned_n").and_then(Json::as_int), Some(50));
    // the dimension map + typed unknowns are the machinery's own.
    let dims = p.get("dimension_map").and_then(Json::as_arr).unwrap();
    assert_eq!(dims.len(), 7);
    let inapp = dims
        .iter()
        .filter(|d| d.get("status").and_then(Json::as_str) == Some("inapplicable"))
        .count();
    assert_eq!(inapp, 5);
    let unk = p.get("typed_unknowns").and_then(Json::as_arr).unwrap();
    assert_eq!(unk.len(), 5);
    // no equivalence wording anywhere in the label.
    let canon = p.to_canonical_string().to_lowercase();
    for banned in [
        "equivalent",
        "noninferior",
        "non-inferior",
        "parity achieved",
    ] {
        assert!(
            !canon
                .replace("iut_conjunction_equivalent", "")
                .contains(banned),
            "banned phrase {banned} in a withheld L2 packet"
        );
    }
    // the pre-stated expected read is recorded, never claimed as executed.
    let kl = p.get("kl_report").unwrap();
    assert!(kl
        .get("expected_precision")
        .and_then(Json::as_str)
        .unwrap()
        .contains("inconclusive"));
    assert_eq!(kl.get("state").and_then(Json::as_str), Some("refused"));
}

#[test]
fn ac5_l2_audit_refuses_w2_corpus() {
    let w4: BTreeSet<String> = ["w4-o-1".to_string()].into_iter().collect();
    let w2: BTreeSet<String> = ["l10-ws1-corpus-01-a".to_string()].into_iter().collect();
    // a W2-corpus member in the sample refuses at decode.
    let err = l2_audit_packet(
        &["l10-ws1-corpus-01-a".to_string()],
        &w4,
        &w2,
        0,
        fixture_sources(),
    )
    .expect_err("a W2-corpus sample member refuses");
    match err {
        AuditError::NotW4O { id } => assert_eq!(id, "l10-ws1-corpus-01-a"),
    }
    // an undeclared id refuses too.
    assert!(l2_audit_packet(&["nope".into()], &w4, &w2, 0, fixture_sources()).is_err());
    // the empty sample (no W4 O recordings exist) withholds.
    let p = l2_audit_packet(&[], &BTreeSet::new(), &w2, 0, fixture_sources()).unwrap();
    assert_eq!(p.get("verdict").and_then(Json::as_str), Some(CLAIM_REFUSAL));
    assert_eq!(p.get("sampled_n").and_then(Json::as_int), Some(0));
    // a W4-O sample passes the guard.
    let ok = l2_audit_packet(&["w4-o-1".into()], &w4, &w2, 0, fixture_sources()).unwrap();
    assert_eq!(ok.get("verdict").and_then(Json::as_str), Some("executed"));
}

#[test]
fn ac5_l3_report_no_equivalence_language() {
    let p = l3_report(fixture_sources());
    assert_eq!(p.get("schema").and_then(Json::as_str), Some(L3_SCHEMA));
    assert_eq!(p.get("served_pairs").and_then(Json::as_int), Some(0));
    let disc = p.get("discordant_pairs").unwrap();
    assert_eq!(disc.get("count").and_then(Json::as_int), Some(0));
    let canon = p.to_canonical_string().to_lowercase();
    for banned in ["equivalent", "noninferior", "non-inferior"] {
        assert!(
            !canon.replace("no equivalence wording", "").contains(banned),
            "banned phrase {banned} in the L3 report"
        );
    }
}

// ── AC-6: emitted records' sidecars recompute ────────────────────────

#[test]
fn ac6_emitted_records_sidecars_verify() {
    for name in [
        "2026-11-19-fp-seal-open",
        "2026-11-19-fp-answer-scan",
        "2026-11-19-fp-results-p2-fp",
        "2026-11-19-fp-results-p4-fp",
        "2026-11-19-fp-results-p3-l2",
        "2026-11-19-fp-l2-audit",
        "2026-11-19-fp-l3",
    ] {
        let rel = format!("docs/build/reports/{name}.json");
        let side = repo_root().join(format!("docs/build/reports/{name}.sha256"));
        if !side.exists() {
            panic!("{rel} sidecar missing — run `fp-results emit` first");
        }
        let bytes = std::fs::read(repo_root().join(&rel)).unwrap();
        let want: String = String::from_utf8_lossy(&std::fs::read(&side).unwrap())
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        assert_eq!(sha256_hex(&bytes), want, "{rel} sha256 sidecar mismatch");
    }
}

#[test]
fn ac6_packets_deterministic() {
    let a = withheld_results_packet(
        "P2-FP",
        "mini-swe-agent",
        &p2_arms(),
        &p2_family(),
        &p2_exploratory(),
        "e",
        fixture_sources(),
    );
    let b = withheld_results_packet(
        "P2-FP",
        "mini-swe-agent",
        &p2_arms(),
        &p2_family(),
        &p2_exploratory(),
        "e",
        fixture_sources(),
    );
    assert_eq!(a.to_canonical_string(), b.to_canonical_string());
}

// ── AC-7: golden disposition branches ────────────────────────────────

#[test]
fn ac7_golden_branches_validate() {
    for branch in GOLDEN_BRANCHES {
        let set = golden_packet_set(branch).unwrap_or_else(|e| panic!("golden {branch}: {e}"));
        assert!(!set.is_empty(), "golden {branch} emits a packet set");
        for p in &set {
            match p.get("schema").and_then(Json::as_str) {
                Some("hh-comparative-results/1") => {
                    if p.get("packet_disposition").is_some() {
                        validate_withheld_results_packet(p)
                            .unwrap_or_else(|e| panic!("golden {branch} withheld packet: {e:?}"));
                    } else {
                        validate_results_packet(p)
                            .unwrap_or_else(|e| panic!("golden {branch} computed packet: {e:?}"));
                    }
                }
                other => panic!("golden {branch} unexpected schema {other:?}"),
            }
        }
    }
}

#[test]
fn ac7_golden_typed_reasons() {
    let fallback = golden_packet_set("fallback").unwrap();
    let p4 = &fallback[1];
    let hs = p4.get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs["v5"].get("disposition").and_then(Json::as_str),
        Some("withheld{l1_not_passed}")
    );
    let end = golden_packet_set("end_after_w2").unwrap();
    let hs2 = end[0].get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs2["mini-swe-agent"]
            .get("disposition")
            .and_then(Json::as_str),
        Some("withheld{admission:pp0_infra_hold}")
    );
    // the real disposition — the emitted spellings verbatim.
    let real = golden_packet_set("re-scope{end-first-pass-early}").unwrap();
    let hs3 = real[0].get("harnesses").and_then(Json::as_obj).unwrap();
    assert_eq!(
        hs3["pi"].get("disposition").and_then(Json::as_str),
        Some("withheld{admission:AG3}")
    );
}

#[test]
fn ac7_emitted_packets_validate() {
    // the committed records themselves — the emitted set re-validates.
    for (rel, claim) in [
        (
            "docs/build/reports/2026-11-19-fp-results-p2-fp.json",
            "P2-FP",
        ),
        (
            "docs/build/reports/2026-11-19-fp-results-p4-fp.json",
            "P4-FP",
        ),
    ] {
        let p = repo_root().join(rel);
        if !p.exists() {
            panic!("{rel} missing — run `fp-results emit` first");
        }
        let j = load_repo(rel);
        validate_withheld_results_packet(&j).unwrap_or_else(|e| panic!("emitted {claim}: {e:?}"));
        assert_eq!(j.get("claim").and_then(Json::as_str), Some(claim));
    }
}
