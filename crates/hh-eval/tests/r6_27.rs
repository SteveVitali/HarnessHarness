//! R6.27 stage-tagged battery — D1-b (b) + D1-c (manifest 280).
//! AC-1 (AC-6.4-3 / REQ-XD-024): the comparative battery covers the
//! full grid plus the first-pass T = 10 cells at ≥2,000 datasets per
//! scenario; a stopped scenario can never produce an admitted rollup;
//! the seed rule reproduces a scenario byte-identically.
//! AC-2 (AC-6.4-2 / REQ-XD-023): the not-admitted refusal stays in
//! force until a GATE-G23 PASSED record exists.
//! AC-4/5/6 legs for `answer-scan/1` live in `crates/hh-runner/tests/r6_27.rs`.

use std::collections::BTreeSet;

use hharness_eval::comparative_battery::*;
use hharness_eval::comparative_estimators::{
    check_method_admission, AdmissionRecord, EstimatorError, UseClass,
};
use hharness_eval::sim_battery::{Cap, Status};

fn small_cap() -> Cap {
    Cap {
        max_datasets: 12,
        deadline: None,
    }
}

/// AC-1: the frozen table covers the mandated grid — every
/// (H, T, mixture, σ) crossing at the null, the power strip, the three
/// mechanisms at the declared base cell and the first-pass T = 10
/// cells at H ∈ {3, 2} × Δ ∈ {0, .09, .13, .20}; every cell declares
/// ≥2,000 datasets.
#[test]
fn ac1_grid_covers_the_mandate() {
    let cells = cb_scenarios();
    assert_eq!(cells.len(), 77);
    let ids: BTreeSet<String> = cells.iter().map(|s| s.id.clone()).collect();
    for h in [2u64, 4, 8] {
        for t in [20u64, 37, 48] {
            for mix in ["board", "easyband", "hard"] {
                for s in ["s00", "s75"] {
                    assert!(
                        ids.contains(&format!("h{h}-t{t}-{mix}-{s}-e+00")),
                        "h{h}-t{t}-{mix}-{s}-e+00"
                    );
                }
            }
            assert!(ids.contains(&format!("h{h}-t{t}-board-s75-e+10")));
        }
    }
    for m in [
        "informative-infra",
        "flagged-pass-veto",
        "shared-shock-rounds",
    ] {
        for e in ["e+00", "e+10"] {
            assert!(ids.contains(&format!("mech/{m}-h4-t37-board-s75-{e}")));
        }
    }
    for h in [3u64, 2] {
        for e in ["e+00", "e+09", "e+13", "e+20"] {
            assert!(ids.contains(&format!("fp/h{h}-t10-board-s75-{e}")));
        }
    }
    for s in &cells {
        assert!(s.datasets >= CB_DATASETS, "{} below the floor", s.id);
        assert_eq!(s.mechanism == CMechanism::Clean, !s.id.starts_with("mech/"));
    }
}

/// AC-1: the seed rule reproduces a scenario byte-identically —
/// dataset k of any cell regenerates the same slots, outcomes and
/// realized Δ.
#[test]
fn ac1_seed_rule_byte_identical() {
    let s = cb_scenarios()
        .into_iter()
        .find(|s| s.id == "h2-t20-board-s75-e+00")
        .unwrap();
    let p = materialize_for(&s);
    let a = cb_dataset(&s, &p, 7);
    let b = cb_dataset(&s, &p, 7);
    let slots = |d: &CDataset| {
        d.input
            .slots
            .iter()
            .map(|sl| {
                (
                    sl.plan_ref.clone(),
                    sl.arm_id.clone(),
                    sl.task_id.clone(),
                    sl.subjects[0].success,
                    sl.subjects[0].availability.clone(),
                    sl.subjects[0].veto_tripped.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(slots(&a), slots(&b));
    assert_eq!(a.true_delta_ppm, b.true_delta_ppm);
    assert_eq!(a.true_theta_ppm, b.true_theta_ppm);
    // And the protocol itself materializes byte-identically (the
    // randomization record recomputes).
    let p2 = materialize_for(&s);
    assert_eq!(p.protocol_id, p2.protocol_id);
    assert_eq!(p.randomization.digest, p2.randomization.digest);
}

/// AC-1: a stopped scenario cannot produce an admitted rollup —
/// `cb_rollup` is `incomplete:<ids>` whenever a cell is `Stopped`,
/// short of its declared count, or carries a generator failure.
#[test]
fn ac1_stopped_never_admits() {
    let mut rep = run_cb_scenario(
        &cb_scenarios()
            .into_iter()
            .find(|s| s.id == "fp/h3-t10-board-s75-e+00")
            .unwrap(),
        &small_cap(),
    );
    assert_eq!(rep.status, Status::Executed);
    assert_eq!(rep.datasets_run, 12);
    // A truncated run reads Stopped.
    let cap = Cap {
        max_datasets: 12,
        deadline: Some(std::time::Instant::now()),
    };
    let stopped = run_cb_scenario(
        &cb_scenarios()
            .into_iter()
            .find(|s| s.id == "fp/h3-t10-board-s75-e+00")
            .unwrap(),
        &cap,
    );
    assert_eq!(stopped.status, Status::Stopped);
    assert!(stopped.datasets_run <= 1);
    rep.status = Status::Stopped;
    let roll = cb_rollup(vec![rep.clone()]);
    assert!(!roll.is_complete());
    assert!(roll.rollup.starts_with("incomplete:fp/h3-t10"));
    rep.status = Status::Executed;
    rep.generator_failures = 1;
    assert!(!cb_rollup(vec![rep]).is_complete());
}

/// AC-1: one executed cell reports its quantities — availability at
/// 1.0, a diagnostic coverage member, the family members' verdict
/// rows, and the T = 10 cells carry `m_ge_7_ppm` and the median
/// bootstrap half-width.
#[test]
fn ac1_cell_records_the_quoted_quantities() {
    let cap = small_cap();
    let fp = run_cb_scenario(
        &cb_scenarios()
            .into_iter()
            .find(|s| s.id == "fp/h3-t10-board-s75-e+13")
            .unwrap(),
        &cap,
    );
    assert_eq!(fp.availability_ppm, 1_000_000);
    assert!(fp.m_ge_7_ppm.is_some(), "T=10 cell carries P(m >= 7)");
    assert!(fp.boot_halfwidth_median_ppm.is_some());
    assert_eq!(fp.member_verdicts.len(), 2, "H=3 → two primary members");
    assert!(fp.mc_error_ppm > 0);
    // A non-T=10 cell leaves the T=10 member absent.
    let big = run_cb_scenario(
        &cb_scenarios()
            .into_iter()
            .find(|s| s.id == "h2-t20-board-s00-e+00")
            .unwrap(),
        &cap,
    );
    assert!(big.m_ge_7_ppm.is_none());
    // The flagged-pass mechanism resolves with joint-population
    // vetoes recorded as facts.
    let mech = run_cb_scenario(
        &cb_scenarios()
            .into_iter()
            .find(|s| s.id == "mech/flagged-pass-veto-h4-t37-board-s75-e+00")
            .unwrap(),
        &cap,
    );
    assert_eq!(mech.status, Status::Executed);
}

/// AC-2 (AC-6.4-2): the not-admitted refusal stays in force — every
/// gated use refuses `MethodNotAdmitted` until a GATE-G23 PASSED
/// record exists; the record and gate entry precede first use.
#[test]
fn ac2_not_admitted_stays_in_force() {
    for u in [UseClass::Confirmatory, UseClass::FirstPass] {
        assert!(matches!(
            check_method_admission("finite_manifest_rate", u, None),
            Err(EstimatorError::MethodNotAdmitted { .. })
        ));
        // A record for the wrong gate / wrong status / empty ref
        // never admits.
        for (gate, status, rref) in [
            ("GATE-G22", "passed", "readouts/GATE-G22.md"),
            ("GATE-G23", "pending", "readouts/GATE-G23.md"),
            ("GATE-G23", "passed", ""),
        ] {
            let a = AdmissionRecord {
                gate: gate.into(),
                status: status.into(),
                record_ref: rref.into(),
            };
            assert!(matches!(
                check_method_admission("finite_manifest_rate", u, Some(&a)),
                Err(EstimatorError::MethodNotAdmitted { .. })
            ));
        }
    }
    // The PASSED record admits — and descriptive/exploratory uses
    // were never gated.
    let ok = AdmissionRecord {
        gate: "GATE-G23".into(),
        status: "passed".into(),
        record_ref: "docs/build/readouts/GATE-G23.md".into(),
    };
    assert!(check_method_admission("x", UseClass::FirstPass, Some(&ok)).is_ok());
    for u in [UseClass::Descriptive, UseClass::Exploratory] {
        assert!(check_method_admission("x", u, None).is_ok());
    }
}
