//! R7.6 — FP-P4C-CONFIRMATION battery (manifest 333).
//!
//! The confirmation fold over the committed P4-C records: the
//! deterministic recompute (AC-1), the Holm family retaining an
//! unavailable member (AC-1/REQ-XD-216), the disposition mutation
//! battery (AC-2/REQ-XD-219 — equality wording refused, withheld
//! typed, an unexecuted confirmation never null), and the §8.8 claims
//! leg over the emitted packet (AC-3 — the banned/conditional phrase
//! set, the two-statement form, selection-labelled estimates never
//! standing as claims).

use std::collections::BTreeSet;

use hharness_eval::comparative_estimators::{confirmatory_family, FamilyDeclC, PRIMARY_METRIC};
use hharness_eval::comparative_protocol::EXCHANGEABILITY;
use hharness_eval::population::PopulationKind;
use hharness_eval::stats::PPM;
use hharness_runner::fp_p4c_fold::*;
use hharness_runner::fp_w4::g23_admission;
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

const PREREG: &str = "docs/build/reports/2026-11-24-fp-p4c-prereg.json";
const SELECTION: &str = "docs/build/reports/2026-11-24-fp-p4x-selection.json";
const WINDOW: &str = "docs/build/reports/2026-11-26-fp-p4c-window-r4.json";
const EARLIER: [&str; 2] = [
    "docs/build/reports/2026-11-26-fp-p4c-window.json",
    "docs/build/reports/2026-11-26-fp-p4c-window-r2-attempt1-native_lane_hold.json",
];
const PACKET: &str = "docs/build/reports/2026-11-26-fp-p4c-confirmation.json";
const PACKET_SHA: &str = "docs/build/reports/2026-11-26-fp-p4c-confirmation.sha256";

fn manifest() -> Json {
    let mut entries = Vec::new();
    for rel in [PREREG, SELECTION, WINDOW].into_iter().chain(EARLIER) {
        let sha = format!(
            "sha256:{}",
            sha256_hex(&std::fs::read(repo_root().join(rel)).unwrap())
        );
        entries.push((rel, sha));
    }
    let refs: Vec<(&str, &str)> = entries.iter().map(|(r, s)| (*r, s.as_str())).collect();
    input_manifest(&refs)
}

fn folded() -> Json {
    let prereg = load_repo(PREREG);
    let selection = load_repo(SELECTION);
    let window = load_repo(WINDOW);
    let earlier: Vec<(&str, Json)> = EARLIER.iter().map(|r| (*r, load_repo(r))).collect();
    let earlier_refs: Vec<(&str, &Json)> = earlier.iter().map(|(r, j)| (*r, j)).collect();
    run_fold_c(
        &prereg,
        Some(&selection),
        Some(&window),
        &earlier_refs,
        &manifest(),
    )
    .unwrap_or_else(|e| panic!("fold: {e}"))
}

/// Every string leaf of a Json document (the claims-check surface).
fn strings(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Str(s) => out.push(s.clone()),
        Json::Arr(a) => a.iter().for_each(|v| strings(v, out)),
        Json::Obj(o) => o.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

// ── AC-1 — deterministic recompute + the committed record ────────────

#[test]
fn ac1_fold_recomputes_byte_identical() {
    let committed = std::fs::read_to_string(repo_root().join(PACKET)).unwrap();
    let rebuilt = folded().to_canonical_string();
    assert_eq!(rebuilt, committed, "the fold recomputes byte-identically");
    let rebuilt2 = folded().to_canonical_string();
    assert_eq!(rebuilt2, committed, "the fold is deterministic");
}

#[test]
fn ac1_sidecar_and_schema() {
    let text = std::fs::read_to_string(repo_root().join(PACKET_SHA)).unwrap();
    let declared = text.split_whitespace().next().unwrap();
    let bytes = std::fs::read(repo_root().join(PACKET)).unwrap();
    assert_eq!(declared, sha256_hex(&bytes));
    let pkt = load_repo(PACKET);
    assert_eq!(
        pkt.get("schema").and_then(Json::as_str),
        Some(CONFIRMATION_SCHEMA)
    );
}

#[test]
fn ac1_committed_packet_shape() {
    let pkt = folded();
    assert_eq!(
        pkt.get("packet_disposition").and_then(Json::as_str),
        Some("executed — no selected member was confirmed different from the baseline at Holm α = .05 on 37 held-out tasks")
    );
    // 367/370 resolved; the 3 unresolved members stay typed.
    let pop = pkt.get("population").unwrap();
    assert_eq!(pop.get("slots").and_then(Json::as_int), Some(370));
    assert_eq!(pop.get("resolved").and_then(Json::as_int), Some(367));
    assert_eq!(pop.get("unresolved").and_then(Json::as_int), Some(3));
    assert_eq!(pop.get("complete").and_then(|v| v.as_bool()), Some(false));
    let um = pop
        .get("unresolved_members")
        .and_then(|v| v.as_arr())
        .unwrap();
    assert_eq!(um.len(), 3);
    // V1's member row — the confirmatory estimators refused typed;
    // the member stays in the Holm family at worst-case mass.
    let v1 = &pkt
        .get("estimand")
        .unwrap()
        .get("contrast")
        .unwrap()
        .get("members")
        .unwrap()
        .as_arr()
        .unwrap()[0];
    assert_eq!(
        v1.get("disposition").and_then(Json::as_str),
        Some("inconclusive{unresolved_cell}")
    );
    assert_eq!(
        v1.get("fm_interval")
            .unwrap()
            .get("state")
            .and_then(Json::as_str),
        Some("refused")
    );
    assert_eq!(
        v1.get("delta_ppm")
            .unwrap()
            .get("state")
            .and_then(Json::as_str),
        Some("refused")
    );
    assert_eq!(
        v1.get("adjusted_p_ppm").and_then(Json::as_int),
        Some(1_000_000)
    );
    assert_eq!(
        v1.get("holm_rejected_at_alpha").and_then(|v| v.as_bool()),
        Some(false)
    );
    // The all-planned bound and the labelled conditional bootstrap
    // still publish — both include 0.
    let b = v1.get("bound_ppm").unwrap();
    assert!(b.get("lo_ppm").and_then(Json::as_int).unwrap() <= 0);
    assert!(b.get("hi_ppm").and_then(Json::as_int).unwrap() >= 0);
    let boot = v1.get("task_bootstrap_interval").unwrap();
    assert_eq!(boot.get("eligible_tasks").and_then(Json::as_int), Some(34));
    assert_eq!(
        boot.get("excluded_tasks")
            .and_then(|v| v.as_arr())
            .unwrap()
            .len(),
        3
    );
    assert!(boot
        .get("label")
        .and_then(Json::as_str)
        .unwrap()
        .contains("conditional secondary"));
    // The family row stays in the family — unavailable, never dropped.
    let fam = pkt.get("family").unwrap();
    assert_eq!(fam.get("method").and_then(Json::as_str), Some("holm"));
    assert_eq!(fam.get("alpha_ppm").and_then(Json::as_int), Some(50_000));
    let rows = fam.get("members").and_then(|v| v.as_arr()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("status").and_then(Json::as_str),
        Some("unavailable")
    );
    assert!(rows[0]
        .get("unavailable")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("UnresolvedCell"));
    // The exchangeability record seals the emitted slot order.
    let exch = pkt.get("exchangeability").unwrap();
    assert_eq!(
        exch.get("exchangeability").and_then(Json::as_str),
        Some("slot_schedule_randomization")
    );
    assert_eq!(
        exch.get("kind").and_then(Json::as_str),
        Some("k_round_interleave")
    );
}

// ── AC-1 — the Holm fixture retains an unavailable member ────────────

#[test]
fn ac1_holm_retains_unavailable_member() {
    // A 3-arm fixture over 5 tasks at k = 5: V1 fully resolved vs the
    // baseline, V2 entirely unresolved — V2 stays in the family at
    // worst-case mass, never dropped (REQ-XD-216).
    let mut rows = Vec::new();
    for t in 0..5 {
        let task = format!("task-{t}");
        for r in 1..=5u64 {
            for (arm, resolved, ok) in [
                ("baseline", true, false),
                ("V1", true, true),
                ("V2", false, false),
            ] {
                rows.push(SlotRow {
                    slot_id: format!("p4c-{arm}-{task}-r{r}"),
                    arm: arm.into(),
                    task: task.clone(),
                    replicate: r,
                    rc: 0,
                    cost_micros: 0,
                    wall_ms: 0,
                    disposition: if resolved {
                        "recorded".into()
                    } else {
                        "verifier_unavailable{verifier_timeout}".into()
                    },
                    success: if resolved { Some(ok) } else { None },
                    unresolved_class: if resolved {
                        None
                    } else {
                        Some("verifier_unavailable{verifier_timeout}".into())
                    },
                });
            }
        }
    }
    let pop = assemble_population(&rows).expect("fixture population");
    let mut order = Vec::new();
    for r in &rows {
        order.push(r.slot_id.clone());
    }
    let mut record = hharness_eval::comparative_protocol::RandomizationRecord {
        kind: hharness_eval::comparative_protocol::SCHEDULE_KIND.into(),
        seed: "fixture".into(),
        rounds: 5,
        exchangeability: EXCHANGEABILITY.into(),
        digest: String::new(),
    };
    record.digest = hharness_eval::comparative_protocol::randomization_digest(&record, &order);
    let fam = confirmatory_family(
        &pop,
        &FamilyDeclC {
            tranche_id: "fixture".into(),
            family_id: FAMILY_ID.into(),
            exploratory_id: EXPLORATORY_FAMILY_ID.into(),
            arms: vec!["baseline".into(), "V1".into(), "V2".into()],
            reference_arm: "baseline".into(),
            metric: PRIMARY_METRIC.into(),
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            stratum: None,
            exchangeability: EXCHANGEABILITY.into(),
            schedule: Some(hharness_eval::comparative_protocol::ScheduleEvidence {
                record,
                slot_order: order,
            }),
            seed: FOLD_SEED,
            use_as: "confirmatory".into(),
            admission: Some(g23_admission()),
        },
    )
    .expect("family computes");
    let rows = &fam.family.1;
    assert_eq!(rows.len(), 2, "both members stay in the family");
    let v1 = rows.iter().find(|r| r.member == "V1").unwrap();
    let v2 = rows.iter().find(|r| r.member == "V2").unwrap();
    assert_eq!(v1.status, "computed");
    assert_eq!(v2.status, "unavailable");
    assert!(
        v2.unavailable.is_some(),
        "the refusal is a member, never a silence"
    );
    assert_eq!(v2.adjusted_ppm, PPM, "worst-case mass — never dropped");
}

// ── AC-2 — the disposition mutation battery ──────────────────────────

#[test]
fn ac2_equality_wording_refused() {
    // A null result is never equality/equivalence wording.
    for bad in [
        "not confirmed; V1 is equivalent to the baseline",
        "V1 equal to the baseline",
        "no different from baseline",
        "statistically tied with the baseline",
        "V1 is non-inferior",
        "parity with the baseline",
        "no worse than baseline",
        "tied with baseline",
    ] {
        assert!(
            check_claims_text(bad).is_err(),
            "equality wording must fail: {bad:?}"
        );
        assert!(
            check_disposition(bad).is_err(),
            "equality wording must fail as a disposition: {bad:?}"
        );
    }
    // The pre-registered forms pass.
    assert!(check_disposition(NULL_DISPOSITION).is_ok());
    assert!(check_disposition(CONFIRMED_DISPOSITION).is_ok());
    assert!(check_disposition("inconclusive{unresolved_cell}").is_ok());
    assert!(check_disposition("withheld{no_candidate}").is_ok());
    assert!(check_disposition("withheld{budget}").is_ok());
    assert!(check_disposition("withheld{infra}").is_ok());
    assert!(check_disposition(UNEXECUTED_DISPOSITION).is_ok());
    // A non-pre-registered shape refuses.
    assert!(check_disposition("null").is_err());
    assert!(check_disposition("done").is_err());
    assert!(check_disposition("no difference").is_err());
}

#[test]
fn ac2_withheld_and_unexecuted_are_typed() {
    let prereg = load_repo(PREREG);
    // A withheld prereg with no window keeps the verbatim withheld.
    let mut w = prereg.clone();
    if let Json::Obj(o) = &mut w {
        o.insert("disposition".into(), Json::str("withheld{no_candidate}"));
    }
    let pkt = run_fold_c(&w, None, None, &[], &manifest()).unwrap();
    assert_eq!(
        pkt.get("packet_disposition").and_then(Json::as_str),
        Some("withheld{no_candidate}")
    );
    // An unexecuted confirmation is unmet, never null.
    let pkt = run_fold_c(&prereg, None, None, &[], &manifest()).unwrap();
    let disp = pkt
        .get("packet_disposition")
        .and_then(Json::as_str)
        .unwrap();
    assert_eq!(disp, UNEXECUTED_DISPOSITION);
    assert_ne!(disp, NULL_DISPOSITION, "unexecuted is never null");
}

// ── AC-3 — the claims check over the packet ──────────────────────────

#[test]
fn ac3_claims_surface_is_clean() {
    let pkt = folded();
    let claims = pkt.get("claims").unwrap();
    assert_eq!(
        claims.get("form").and_then(Json::as_str),
        Some("two-statement (§8.8)")
    );
    // The two statements are the only "which performs best" rendering.
    for member in ["statement_exploratory", "statement_confirmatory"] {
        let s = claims.get(member).and_then(Json::as_str).unwrap();
        assert!(
            check_claims_text(s).is_ok(),
            "{member} carries banned wording"
        );
    }
    // Selection-labelled estimates never stand as claims — each row
    // carries the exploratory label verbatim.
    let est = claims.get("exploratory_estimates").unwrap();
    assert!(est
        .get("label")
        .and_then(Json::as_str)
        .unwrap()
        .contains("exploratory"));
    for m in est.get("members").and_then(|v| v.as_arr()).unwrap() {
        assert!(
            m.get("label")
                .and_then(Json::as_str)
                .unwrap()
                .contains("exploratory"),
            "a selection-labelled estimate keeps its label"
        );
    }
    // The mandatory labels are present verbatim.
    let labels = claims.get("labels").unwrap();
    for (member, needle) in [
        ("finite_manifest_scope", "finite-manifest"),
        (
            "ofat_caveat",
            "one-factor-at-a-time: effects are local to the baseline configuration; interactions not estimated",
        ),
        ("catalog_window", "catalog window"),
        ("baseline_label", "baseline"),
    ] {
        assert!(
            labels
                .get(member)
                .and_then(Json::as_str)
                .unwrap_or("")
                .contains(needle),
            "labels.{member} carries {needle:?}"
        );
    }
    // The cost surface is descriptive-only.
    assert!(claims
        .get("cost")
        .unwrap()
        .get("label")
        .and_then(Json::as_str)
        .unwrap()
        .contains("descriptive"));
    // The whole packet scans clean under the record-facing banned set
    // (`banned` + `record_banned`); `site_banned` applies to the
    // rendered site strings — the two statements are the site-bound
    // surface and carry none of it either.
    let lex = load_repo("scripts/claims-lexicon.json");
    let mut corpus = Vec::new();
    strings(&pkt, &mut corpus);
    let phrases = |key: &str| -> Vec<String> {
        lex.get(key)
            .and_then(|v| v.as_arr())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|e| {
                e.as_str()
                    .map(str::to_string)
                    .or_else(|| e.get("phrase").and_then(Json::as_str).map(str::to_string))
            })
            .collect()
    };
    let record_banned: Vec<String> = ["banned", "record_banned"]
        .iter()
        .flat_map(|k| phrases(k))
        .collect();
    let site_banned = phrases("site_banned");
    for s in &corpus {
        let low = s.to_lowercase();
        for b in &record_banned {
            assert!(
                !low.contains(&b.to_lowercase()),
                "packet carries banned phrase {b:?} in {s:?}"
            );
        }
    }
    for member in ["statement_exploratory", "statement_confirmatory"] {
        let s = claims.get(member).and_then(Json::as_str).unwrap();
        let low = s.to_lowercase();
        for b in site_banned.iter().chain(record_banned.iter()) {
            assert!(
                !low.contains(&b.to_lowercase()),
                "claim {member} carries banned phrase {b:?}: {s:?}"
            );
        }
    }
    // The conditional phrases never appear (no Holm p < .05 sentence
    // exists to admit them — V1 is inconclusive).
    for s in &corpus {
        let low = s.to_lowercase();
        for c in ["improves", "improvement", "significant", "equivalent"] {
            if low.contains(c) {
                // allowed only inside a documented label — assert the
                // surviving occurrences are the exploratory-label and
                // the family's published label strings, never a claim.
                assert!(
                    s.contains("interval of difference covers 0 is not equivalence evidence")
                        || s.contains("interactions not estimated")
                        || s.contains("never a claim")
                        || s.contains("not equivalence")
                        || s.contains("no confirmatory"),
                    "conditional phrase {c:?} outside its allow context: {s:?}"
                );
            }
        }
    }
    let _ = BTreeSet::<String>::new();
}
