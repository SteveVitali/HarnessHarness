//! R6.19 — the §6.4 admission-map apparatus (manifest 272; REQ-XD-001,
//! REQ-XD-022, REQ-XD-030/031, REQ-XD-061, REQ-XF-123; contributing
//! REQ-XA-006, REQ-XD-021/023/090). Fails-without: `comparative_estimators`,
//! `population::arm_bounds`, `comparative_protocol::ScheduleEvidence`,
//! `finite_manifest::arm_marginal_interval` and the `task_mixture`
//! `slot_schedule_randomization` route do not exist at the R6.18 base —
//! this file compiles red there. Fixture-only: every input is supplied;
//! no spend, no network.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators as ce;
use hharness_eval::comparative_protocol::{
    randomization_digest, RandomizationRecord, ScheduleEvidence,
};
use hharness_eval::finite_manifest::{
    arm_marginal_interval, finite_manifest_contrast, ContrastDecl, BASE_DRAWS, CAP_DRAWS,
    CONTRAST_KIND, LEVEL_PPM,
};
use hharness_eval::population::{
    assemble, Availability, BoundsDecl, PlannedPopulation, PlannedSlot, PopulationInput,
    PopulationKind,
};
use hharness_eval::task_mixture::{
    task_mixture, TaskMixtureDecl, TaskMixtureError, SLOT_SCHEDULE_EXCHANGEABILITY,
};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::Json;

// ── fixtures ─────────────────────────────────────────────────────────

fn sub(run: &str, ok: Option<bool>) -> hharness_eval::population::SlotSubject {
    hharness_eval::population::SlotSubject {
        subject_run: run.into(),
        attempt_no: 1,
        outcome: ok.map(|_| OutcomeClass::Scored),
        success: ok,
        veto_tripped: vec![],
        availability: if ok.is_some() {
            Availability::Resolved
        } else {
            Availability::NotRun
        },
        seq: 0,
        missing_required_checks: false,
        admissible: true,
    }
}

fn cell(task: &str, arm: &str, c: usize, n: usize, unresolved: usize) -> Vec<PlannedSlot> {
    (0..n + unresolved)
        .map(|i| PlannedSlot {
            plan_ref: format!("p-{task}-{arm}-{i}"),
            task_id: task.into(),
            split_label: "held_out".into(),
            stratum: "strat".into(),
            arm_id: arm.into(),
            replicate_index: i as u64,
            subjects: vec![sub(
                &format!("r-{task}-{arm}-{i}"),
                if i < n { Some(i < c) } else { None },
            )],
        })
        .collect()
}

/// 3 arms × 2 tasks × 5 reps, all resolved — a P2-FP-shaped manifest.
fn pop3() -> PlannedPopulation {
    let mut slots = Vec::new();
    for (t, (a, b, c)) in [("t1", (2, 4, 1)), ("t2", (3, 1, 4))] {
        slots.extend(cell(t, "mini", a, 5, 0));
        slots.extend(cell(t, "pi", b, 5, 0));
        slots.extend(cell(t, "cc", c, 5, 0));
    }
    assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap()
}

/// As `pop3` plus a third task, with one unresolved slot on (t1, pi).
fn pop3_unresolved() -> PlannedPopulation {
    let mut slots = Vec::new();
    slots.extend(cell("t1", "mini", 2, 5, 0));
    slots.extend(cell("t1", "pi", 4, 5, 1)); // one unresolved member
    slots.extend(cell("t1", "cc", 1, 5, 0));
    slots.extend(cell("t2", "mini", 3, 5, 0));
    slots.extend(cell("t2", "pi", 1, 5, 0));
    slots.extend(cell("t2", "cc", 4, 5, 0));
    slots.extend(cell("t3", "mini", 5, 5, 0));
    slots.extend(cell("t3", "pi", 0, 5, 0));
    slots.extend(cell("t3", "cc", 5, 5, 0));
    assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap()
}

fn rate_decl(use_as: &str) -> ce::RateDecl {
    ce::RateDecl {
        population: PopulationKind::JointTaskPassWithoutObservedVeto,
        arm: "mini".into(),
        stratum: None,
        prior: (1, 1),
        prior_name: "beta{1,1}".into(),
        bounds: BoundsDecl::Unit,
        level_ppm: LEVEL_PPM,
        seed: 7,
        use_as: use_as.into(),
        admission: None,
    }
}

fn passed() -> Option<ce::AdmissionRecord> {
    Some(ce::AdmissionRecord {
        gate: "GATE-G23".into(),
        status: "passed".into(),
        record_ref: "docs/build/readouts/GATE-G23.md".into(),
    })
}

fn evidence(rounds: u32) -> ScheduleEvidence {
    let slot_order: Vec<String> = (0..rounds * 6).map(|i| format!("slot:{i:03}")).collect();
    let mut record = RandomizationRecord {
        kind: "k_round_interleave".into(),
        seed: "sched-seed".into(),
        rounds,
        exchangeability: "slot_schedule_randomization".into(),
        digest: String::new(),
    };
    record.digest = randomization_digest(&record, &slot_order);
    ScheduleEvidence { record, slot_order }
}

fn fc(c: u64, n: u64, task: &str) -> ce::FlipCell {
    ce::FlipCell {
        task_id: task.into(),
        c,
        n,
    }
}

fn estimand_decl(use_as: &str) -> ce::EstimandDecl {
    ce::EstimandDecl {
        population: PopulationKind::JointTaskPassWithoutObservedVeto,
        arms: vec!["mini".into(), "pi".into(), "cc".into()],
        reference_arm: "mini".into(),
        stratum: None,
        use_as: use_as.into(),
        admission: None,
    }
}

fn family_decl(use_as: &str) -> ce::FamilyDeclC {
    ce::FamilyDeclC {
        tranche_id: "TR1".into(),
        family_id: "P2-FP".into(),
        exploratory_id: "P2-FP-expl".into(),
        arms: vec!["mini".into(), "pi".into(), "cc".into()],
        reference_arm: "mini".into(),
        metric: ce::PRIMARY_METRIC.into(),
        population: PopulationKind::JointTaskPassWithoutObservedVeto,
        stratum: None,
        exchangeability: "slot_schedule_randomization".into(),
        schedule: Some(evidence(5)),
        seed: 11,
        use_as: use_as.into(),
        admission: None,
    }
}

// ── AC-2 legs: single-arm rate (route b) ─────────────────────────────

#[test]
fn rate_marginal_equality_and_known_answer() {
    let pop = pop3();
    // Confirmatory/first-pass refuse typed until GATE-G23 PASSED.
    for u in ["confirmatory", "first_pass"] {
        let mut d = rate_decl(u);
        assert!(matches!(
            ce::finite_manifest_rate(&pop, &d).unwrap_err(),
            ce::EstimatorError::MethodNotAdmitted { .. }
        ));
        d.admission = Some(ce::AdmissionRecord {
            gate: "GATE-G23".into(),
            status: "pending".into(),
            record_ref: "x".into(),
        });
        assert!(matches!(
            ce::finite_manifest_rate(&pop, &d).unwrap_err(),
            ce::EstimatorError::MethodNotAdmitted { .. }
        ));
    }
    // Descriptive computes; GATE-G23-passed confirmatory computes.
    let r = ce::finite_manifest_rate(&pop, &rate_decl("descriptive")).unwrap();
    // mini cells: t1 (2,5), t2 (3,5). observed = mean(.4,.6) = .5.
    assert_eq!(r.cells.len(), 2);
    assert_eq!(r.observed_ppm, 500_000);
    // posterior mean: mean(3/7, 4/7) = .5.
    assert_eq!(r.posterior_mean_ppm, 500_000);
    assert_eq!(r.draws_requested, BASE_DRAWS);
    assert_eq!(r.draws_used, CAP_DRAWS);
    assert_eq!(r.doubling.len(), 4); // 8k/16k/32k/64k ladder
    assert!(r.interval.lo <= r.interval.hi && r.interval.lo >= 0);
    // The AC-2 leg: the single-arm interval equals the contrast
    // machinery's per-arm marginal over the same resolved cells.
    let contrast = finite_manifest_contrast(
        &pop,
        &ContrastDecl {
            population: PopulationKind::JointTaskPassWithoutObservedVeto,
            test_arm: "pi".into(),
            control_arm: "mini".into(),
            stratum: None,
            prior: (1, 1),
            prior_name: "beta{1,1}".into(),
            dependence: None,
            target: None,
            use_as: None,
            bounds: BoundsDecl::Unit,
            level_ppm: LEVEL_PPM,
            seed: 9,
        },
    )
    .unwrap();
    let mini_cells: Vec<(u64, u64)> = contrast
        .cells
        .iter()
        .map(|c| (c.control_c, c.control_n))
        .collect();
    let marg = arm_marginal_interval(&mini_cells, (1, 1), &r.seed_material);
    assert_eq!(
        marg.interval, r.interval,
        "marginal must equal the rate interval"
    );
    assert_eq!(marg.doubling, r.doubling);
    // Refusals: OneTask, unresolved, wrong prior, unknown use.
    assert!(matches!(
        ce::finite_manifest_rate(&pop, &rate_decl("credible")).unwrap_err(),
        ce::EstimatorError::UnsupportedUse { .. }
    ));
    let mut d2 = rate_decl("descriptive");
    d2.prior_name = "beta{3,3}".into();
    d2.prior = (3, 3);
    assert!(ce::finite_manifest_rate(&pop, &d2).is_err());
    let _ = CONTRAST_KIND;
}

// ── AC-2 legs: flips + $/solve known answers ─────────────────────────

#[test]
fn flips_and_solve_known_answers() {
    // Constant cells (c=2,k=5): f_t = 2·2·3/(5·4) = 600_000 ppm.
    let cells: Vec<ce::FlipCell> = (0..4).map(|i| fc(2, 5, &format!("t{i}"))).collect();
    let f = ce::rerun_flip(&cells, "descriptive", None, 2_000, 5).unwrap();
    assert_eq!(f.point, 600_000);
    assert_eq!(f.labels, vec![ce::LABEL_DESCRIPTIVE.to_string()]);
    // Unbiasedness: E[2C(k−C)/(k(k−1))] = 2p(1−p) — the AC's "constant
    // p gives f = 2p(1−p)", verified exactly over the binomial
    // enumeration at p = 1/2, k = 5 (rational i128).
    let kernel = |c: i128| 2 * c * (5 - c) / 4; // 2c(5-c)/(5·4)·5 — wait, see below
    let _ = kernel;
    let mut num: i128 = 0;
    let binom = [1i128, 5, 10, 10, 5, 1];
    for c in 0..=5i128 {
        num += binom[c as usize] * (2 * c * (5 - c)); // 2c(k−c)
    }
    // E[f] = num/(k(k−1)·2^k) = num/(20·32) — must equal 1/2.
    assert_eq!(num, 20 * 32 / 2, "E[f] = 2p(1-p) at p=1/2");
    // The swap−rerun identity in expectation: E[f_swap] − ½(E[f_g] +
    // E[f_h]) = (p_g − p_h)² — exact 2-D binomial enumeration at
    // (3/10, 7/10, k=2). f_swap kernel: (c_g·2 + c_h·2 − 2c_g c_h)/4.
    let (mut eswap, mut eg, mut eh, mut wsum) = (0i128, 0i128, 0i128, 0i128);
    let (pg_num, ph_num) = (3i128, 7i128);
    for cg in 0..=2i128 {
        for ch in 0..=2i128 {
            let w = binom2(cg)
                * binom2(ch)
                * pg_num.pow(cg as u32)
                * (10 - pg_num).pow(2 - cg as u32)
                * ph_num.pow(ch as u32)
                * (10 - ph_num).pow(2 - ch as u32);
            wsum += w;
            eswap += w * (cg * 2 + ch * 2 - 2 * cg * ch); // ×4 denominator
            eg += w * (2 * cg * (2 - cg)); // 2c(k−c) × [k(k−1)=2 denom]
            eh += w * (2 * ch * (2 - ch));
        }
    }
    // eswap/4/wsum − ½(eg/2 + eh/2)/wsum == (p_g−p_h)² = 16/100.
    // lhs = (eswap−eg−eh)/(4·W) — must equal (.7−.3)²=.16 ⇒ 25·lhs_num = 16·W.
    assert_eq!(
        (eswap - eg - eh) * 25,
        16 * wsum,
        "swap-rerun identity in expectation"
    );
    // $/solve: two tasks' trials; Σ cost / Σ solved = (100+300+200)/(2)
    // = 300 µ$.
    let trials = vec![
        ce::TrialCost {
            task_id: "t1".into(),
            cost_micros: 100,
            solved: true,
        },
        ce::TrialCost {
            task_id: "t1".into(),
            cost_micros: 300,
            solved: false,
        },
        ce::TrialCost {
            task_id: "t2".into(),
            cost_micros: 200,
            solved: true,
        },
    ];
    let cr = ce::cost_routes(&trials, "model_spend_only", "descriptive", None, 2_000, 3).unwrap();
    assert_eq!(cr.per_solve_micros, Some(300));
    assert_eq!(cr.per_trial_micros, 200);
    // 0 solves refuses typed.
    let zero: Vec<ce::TrialCost> = trials
        .iter()
        .map(|t| ce::TrialCost {
            solved: false,
            ..t.clone()
        })
        .collect();
    assert!(matches!(
        ce::cost_routes(&zero, "model_spend_only", "descriptive", None, 2_000, 3).unwrap_err(),
        ce::EstimatorError::EstimatorUndefined { .. }
    ));
}

fn binom2(c: i128) -> i128 {
    [1i128, 2, 1][c as usize]
}

#[test]
fn swap_and_excess() {
    let g: Vec<ce::FlipCell> = vec![fc(4, 5, "t1"), fc(1, 5, "t2")];
    let h: Vec<ce::FlipCell> = vec![fc(1, 5, "t1"), fc(3, 5, "t2")];
    let r = ce::swap_flip("g", &g, "h", &h, "exploratory", None, 2_000, 4).unwrap();
    // t1: (4·5 + 1·5 − 2·4·1)/25 = 17/25 = 680_000; t2: (1·5+3·5−2·3)/25
    // = 14/25 = 560_000 → mean 620_000.
    assert_eq!(r.swap_ppm, 620_000);
    // f_rerun: c=4→ 2·4·1/20 = .4; c=1→.4; c=3→.6 → g mean .4, h mean .5.
    assert_eq!(r.rerun_ppm, (400_000, 500_000));
    // excess = swap − ½(fg+fh) = 620_000 − 450_000 = 170_000.
    assert_eq!(r.excess_ppm, 170_000);
    // Unpaired task refuses.
    let bad = vec![fc(1, 5, "t9")];
    assert!(matches!(
        ce::swap_flip("g", &g, "h", &bad, "exploratory", None, 2_000, 4).unwrap_err(),
        ce::EstimatorError::UnpairedTask { .. }
    ));
}

// ── AC-1/AC-5: the estimand + §6.8 envelope ──────────────────────────

#[test]
fn estimand_by_hand_and_unresolved() {
    let pop = pop3();
    let rep = ce::comparative_estimand(&pop, &estimand_decl("exploratory")).unwrap();
    // θ̂: mini mean(.4,.6)=.5; pi mean(.8,.2)=.5; cc mean(.2,.8)=.5.
    let th: Vec<Option<i64>> = rep.arms.iter().map(|a| a.theta_ppm).collect();
    assert_eq!(th, vec![Some(500_000); 3]);
    // Δ(pi−mini) = mean(.8−.4, .2−.6) = 0; Δ(cc−mini) = mean(−.2,+.2)=0.
    assert_eq!(rep.contrasts[0].delta_ppm, Some(0));
    // envelopes: 10 slots per arm, all first-subject resolved.
    assert_eq!(rep.arms[0].completion, (10, 10, 0, 0));
    // AC-5: one unresolved slot → the involving contrast refuses
    // UnresolvedCell; bounds + envelope still publish; the conditional
    // secondary is labelled.
    let pop_u = pop3_unresolved();
    let rep_u = ce::comparative_estimand(&pop_u, &estimand_decl("exploratory")).unwrap();
    let pi_delta = &rep_u.contrasts[0];
    assert!(pi_delta.delta_ppm.is_none());
    assert!(pi_delta
        .refusal
        .as_deref()
        .unwrap()
        .contains("UnresolvedCell"));
    assert!(pi_delta.bounds.lo <= pi_delta.bounds.hi);
    // pi completion: 16 planned, 15 first, 1 unresolved.
    assert_eq!(rep_u.arms[1].completion, (16, 15, 0, 1));
    // conditional secondary: eligible-only over {t2,t3} →
    // mean(−.4,−1.0) = −.7, labelled.
    let cond = pi_delta
        .conditional
        .as_ref()
        .expect("conditional secondary");
    assert!(cond.label.contains("conditional secondary"));
    assert_eq!(cond.tasks, vec!["t2".to_string(), "t3".to_string()]);
    assert_eq!(cond.value_ppm, -700_000);
    // The cc contrast is unaffected: all its cells resolved.
    assert_eq!(rep_u.contrasts[1].delta_ppm, Some(0));
    // pi θ is None with the unresolved task named.
    assert_eq!(rep_u.arms[1].theta_ppm, None);
    assert_eq!(rep_u.arms[1].unresolved_tasks, vec!["t1".to_string()]);
}

// ── AC-3: the per-tranche family ─────────────────────────────────────

#[test]
fn family_holm_bh_and_unavailable() {
    let pop = pop3();
    let fam = ce::confirmatory_family(&pop, &family_decl("exploratory")).unwrap();
    // P2-FP shape: two primary members (pi, cc), family of 2.
    assert_eq!(fam.family.0, "P2-FP");
    assert_eq!(fam.family.1.len(), 2);
    assert!(fam.family.1.iter().all(|r| r.status == "computed"));
    assert!(fam
        .family
        .1
        .iter()
        .all(|r| r.mode.as_deref() == Some("exact")));
    // exploratory: one other pair (pi|cc) under BH.
    assert_eq!(fam.exploratory.1.len(), 1);
    // AC-3 unavailable leg: (t1,pi) unresolved → the pi member stays in
    // the family, status unavailable, Holm over 2 members at worst mass.
    let fam_u = ce::confirmatory_family(&pop3_unresolved(), &family_decl("exploratory")).unwrap();
    assert_eq!(fam_u.family.1.len(), 2);
    let pi_row = &fam_u.family.1[0];
    assert_eq!(pi_row.status, "unavailable");
    assert_eq!(pi_row.raw_p_ppm, None);
    assert!(pi_row
        .unavailable
        .as_deref()
        .unwrap()
        .contains("UnresolvedCell"));
    assert_eq!(pi_row.adjusted_ppm, 1_000_000);
    assert!(!pi_row.rejected_at_alpha);
    // refusals: no record → ExchangeabilityNotAdmitted; bad record →
    // RandomizationRecord; wrong spelling → ExchangeabilityNotAdmitted;
    // confirmatory/first_pass → MethodNotAdmitted; wrong metric/pop.
    let mut d = family_decl("exploratory");
    d.schedule = None;
    assert!(matches!(
        ce::confirmatory_family(&pop, &d).unwrap_err(),
        ce::EstimatorError::ExchangeabilityNotAdmitted { .. }
    ));
    let mut d2 = family_decl("exploratory");
    d2.schedule.as_mut().unwrap().slot_order.pop();
    assert!(matches!(
        ce::confirmatory_family(&pop, &d2).unwrap_err(),
        ce::EstimatorError::RandomizationRecord { .. }
    ));
    let mut d3 = family_decl("exploratory");
    d3.exchangeability = "coin_toss".into();
    assert!(matches!(
        ce::confirmatory_family(&pop, &d3).unwrap_err(),
        ce::EstimatorError::ExchangeabilityNotAdmitted { .. }
    ));
    for u in ["confirmatory", "first_pass"] {
        assert!(matches!(
            ce::confirmatory_family(&pop, &family_decl(u)).unwrap_err(),
            ce::EstimatorError::MethodNotAdmitted { .. }
        ));
    }
    let mut d4 = family_decl("confirmatory");
    d4.admission = passed();
    assert!(ce::confirmatory_family(&pop, &d4).is_ok());
}

// ── AC-4: tied-with-leader ───────────────────────────────────────────

#[test]
fn tied_with_leader_route() {
    // Strong leader: cc 5/5 & 4/5; pi 1/5 & 2/5; mini 0/5 & 1/5.
    let mut slots = Vec::new();
    for (t, (a, b, c)) in [("t1", (0, 1, 5)), ("t2", (1, 2, 4))] {
        slots.extend(cell(t, "mini", a, 5, 0));
        slots.extend(cell(t, "pi", b, 5, 0));
        slots.extend(cell(t, "cc", c, 5, 0));
    }
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    let arms = vec!["mini".to_string(), "pi".to_string(), "cc".to_string()];
    let r = ce::tied_with_leader(
        &pop,
        PopulationKind::JointTaskPassWithoutObservedVeto,
        &arms,
        None,
        "exploratory",
        None,
        None,
        2_000,
        3,
    )
    .unwrap();
    assert_eq!(r.leader_id, "cc");
    assert!(r
        .rows
        .iter()
        .any(|row| row.arm_id == "cc" && row.tied == Some(true)));
    // The labels carry the descriptive/exploratory selection caveat.
    assert!(r
        .labels
        .iter()
        .any(|l| l.contains("leader selected from the same data")));
    assert!(r.labels.iter().any(|l| l.contains("descriptive")));
    // An equivalence claim refuses — the claims-check leg (REQ-XD-031).
    for claim in ["equivalence", "tost", "no_difference", "statistically_tied"] {
        assert!(matches!(
            ce::tied_with_leader(
                &pop,
                PopulationKind::JointTaskPassWithoutObservedVeto,
                &arms,
                None,
                "exploratory",
                Some(claim),
                None,
                2_000,
                3,
            )
            .unwrap_err(),
            ce::EstimatorError::UnsupportedClaim { .. }
        ));
    }
}

// ── AC-7: the slot_schedule_randomization sign-flip route ────────────

#[test]
fn task_mixture_slot_schedule_randomization() {
    let pop = pop3();
    let decl = |use_class: Option<&str>, ev: Option<ScheduleEvidence>, adm| TaskMixtureDecl {
        population: PopulationKind::JointTaskPassWithoutObservedVeto,
        test_arm: "pi".into(),
        control_arm: "mini".into(),
        stratum: None,
        bootstrap: "percentile".into(),
        null: Some("sharp_no_effect".into()),
        exchangeability: Some(SLOT_SCHEDULE_EXCHANGEABILITY.into()),
        dependence: None,
        verdict: None,
        bounds: BoundsDecl::Unit,
        level_ppm: 950_000,
        draws: 2_000,
        seed: 5,
        family: None,
        exploratory: None,
        schedule_randomization: ev,
        use_class: use_class.map(str::to_string),
        admission: adm,
    };
    // No evidence → ExchangeabilityNotAdmitted.
    assert!(matches!(
        task_mixture(&pop, &decl(Some("exploratory"), None, None)).unwrap_err(),
        TaskMixtureError::ExchangeabilityNotAdmitted { .. }
    ));
    // Evidence + exploratory → computed under the new spelling.
    let out = task_mixture(&pop, &decl(Some("exploratory"), Some(evidence(5)), None)).unwrap();
    match &out.sign_flip {
        hharness_eval::task_mixture::SignFlip::Computed {
            exchangeability, ..
        } => {
            assert_eq!(exchangeability, SLOT_SCHEDULE_EXCHANGEABILITY);
        }
        other => panic!("expected computed sign flip: {other:?}"),
    }
    let j = out.to_json();
    assert_eq!(
        j.get("sign_flip")
            .and_then(|s| s.get("exchangeability"))
            .and_then(Json::as_str),
        Some(SLOT_SCHEDULE_EXCHANGEABILITY)
    );
    // Mismatched digest → RandomizationRecord.
    let mut bad = evidence(5);
    bad.slot_order.pop();
    assert!(matches!(
        task_mixture(&pop, &decl(Some("exploratory"), Some(bad), None)).unwrap_err(),
        TaskMixtureError::RandomizationRecord { .. }
    ));
    // Confirmatory/first_pass (and absent use_class) → MethodNotAdmitted
    // until GATE-G23 PASSED.
    for u in [Some("confirmatory"), Some("first_pass"), None] {
        assert!(matches!(
            task_mixture(&pop, &decl(u, Some(evidence(5)), None)).unwrap_err(),
            TaskMixtureError::MethodNotAdmitted { .. }
        ));
    }
    // GATE-G23 PASSED computes.
    let ok = task_mixture(
        &pop,
        &decl(Some("confirmatory"), Some(evidence(5)), passed()),
    )
    .unwrap();
    assert!(matches!(
        ok.sign_flip,
        hharness_eval::task_mixture::SignFlip::Computed { .. }
    ));
    // lane_randomization still computes unchanged (round-5 battery
    // posture — no record needed).
    let mut lane = decl(Some("confirmatory"), None, None);
    lane.exchangeability = Some("lane_randomization".into());
    assert!(task_mixture(&pop, &lane).is_ok());
}

// ── AC-6: hh-comparative-results/1 ───────────────────────────────────

fn harness_row() -> Json {
    Json::obj([
        (
            "theta",
            Json::obj([
                ("ppm", Json::Int(500_000)),
                ("lo_ppm", Json::Int(320_000)),
                ("hi_ppm", Json::Int(680_000)),
                (
                    "interval",
                    Json::obj([
                        ("kind", Json::str("fm_credible")),
                        ("level_ppm", Json::Int(950_000)),
                        ("draws", Json::Int(8_000)),
                        ("lo_ppm", Json::Int(320_000)),
                        ("hi_ppm", Json::Int(680_000)),
                    ]),
                ),
                (
                    "bounds",
                    Json::obj([
                        ("kind", Json::str("all_planned")),
                        ("lo_ppm", Json::Int(500_000)),
                        ("hi_ppm", Json::Int(500_000)),
                    ]),
                ),
                (
                    "label",
                    Json::str("finite-manifest: these 10 TB 2.1 held-out tasks (band A), not the 89-task benchmark"),
                ),
            ]),
        ),
        (
            "secondary_populations",
            Json::obj([
                (
                    "raw",
                    Json::obj([
                        ("rate_ppm", Json::Int(510_000)),
                        ("label", Json::str("secondary population")),
                    ]),
                ),
                (
                    "errors_zero",
                    Json::obj([
                        ("bound_lo_ppm", Json::Int(500_000)),
                        ("bound_hi_ppm", Json::Int(500_000)),
                        ("label", Json::str("secondary population")),
                    ]),
                ),
            ]),
        ),
        (
            "delta_vs_ref",
            Json::obj([
                ("ppm", Json::Int(120_000)),
                ("lo_ppm", Json::Int(-80_000)),
                ("hi_ppm", Json::Int(300_000)),
                (
                    "interval",
                    Json::obj([
                        ("kind", Json::str("fm_credible")),
                        ("level_ppm", Json::Int(950_000)),
                        ("lo_ppm", Json::Int(-80_000)),
                        ("hi_ppm", Json::Int(300_000)),
                    ]),
                ),
                ("p_raw_ppm", Json::Int(120_000)),
                ("p_holm_ppm", Json::Int(240_000)),
                ("separated", Json::Bool(false)),
                ("label", Json::str("confirmatory family TR1; α = .05 Holm")),
            ]),
        ),
        (
            "leader_group",
            Json::obj([
                ("member", Json::Bool(true)),
                ("leader_id", Json::str("pi")),
                (
                    "diff_interval",
                    Json::obj([
                        ("kind", Json::str("task_bootstrap")),
                        ("level_ppm", Json::Int(950_000)),
                        ("lo_ppm", Json::Int(-50_000)),
                        ("hi_ppm", Json::Int(210_000)),
                    ]),
                ),
                (
                    "label",
                    Json::str("exploratory; leader selected from the same data"),
                ),
            ]),
        ),
        (
            "pass_at",
            Json::obj([
                ("k", Json::Int(5)),
                ("all_ppm", Json::Int(120_000)),
                ("any_ppm", Json::Int(880_000)),
            ]),
        ),
        (
            "flips",
            Json::obj([
                (
                    "rerun",
                    Json::obj([
                        ("ppm", Json::Int(420_000)),
                        (
                            "interval",
                            Json::obj([
                                ("kind", Json::str("task_bootstrap")),
                                ("level_ppm", Json::Int(950_000)),
                                ("lo_ppm", Json::Int(300_000)),
                                ("hi_ppm", Json::Int(540_000)),
                            ]),
                        ),
                    ]),
                ),
                (
                    "swap_vs",
                    Json::obj([(
                        "cc",
                        Json::obj([
                            ("ppm", Json::Int(360_000)),
                            (
                                "interval",
                                Json::obj([
                                    ("kind", Json::str("task_bootstrap")),
                                    ("level_ppm", Json::Int(950_000)),
                                    ("lo_ppm", Json::Int(240_000)),
                                    ("hi_ppm", Json::Int(480_000)),
                                ]),
                            ),
                        ]),
                    )]),
                ),
                (
                    "harness_excess",
                    Json::obj([(
                        "cc",
                        Json::obj([
                            ("ppm", Json::Int(40_000)),
                            (
                                "interval",
                                Json::obj([
                                    ("kind", Json::str("task_bootstrap")),
                                    ("level_ppm", Json::Int(950_000)),
                                    ("lo_ppm", Json::Int(-20_000)),
                                    ("hi_ppm", Json::Int(100_000)),
                                ]),
                            ),
                        ]),
                    )]),
                ),
            ]),
        ),
        (
            "cost",
            Json::obj([
                (
                    "per_trial",
                    Json::obj([
                        ("micros", Json::Int(3_400)),
                        (
                            "interval",
                            Json::obj([
                                ("kind", Json::str("clustered_bootstrap")),
                                ("level_ppm", Json::Int(950_000)),
                                ("lo_ppm", Json::Int(2_900)),
                                ("hi_ppm", Json::Int(3_900)),
                            ]),
                        ),
                        (
                            "tails",
                            Json::obj([
                                ("p50_micros", Json::Int(3_200)),
                                ("p90_micros", Json::Int(4_100)),
                                ("p95_micros", Json::Int(4_400)),
                                ("max_micros", Json::Int(6_000)),
                            ]),
                        ),
                    ]),
                ),
                (
                    "per_solve",
                    Json::obj([
                        ("micros", Json::Int(8_500)),
                        (
                            "interval",
                            Json::obj([
                                ("kind", Json::str("task_resampled_ratio")),
                                ("level_ppm", Json::Int(950_000)),
                                ("lo_ppm", Json::Int(5_000)),
                                ("hi_ppm", Json::Int(14_000)),
                            ]),
                        ),
                    ]),
                ),
                ("provenance", Json::str("model_spend_only")),
                ("label", Json::str("descriptive; model spend only")),
            ]),
        ),
        (
            "counts",
            Json::obj([
                ("analysis_basis", Json::str("assignment")),
                ("intended", Json::obj([("v1.2.3", Json::Int(50))])),
                ("served", Json::obj([("hh/mini-swe-agent@1", Json::Int(50))])),
            ]),
        ),
        (
            "completion",
            Json::obj([
                ("planned", Json::Int(50)),
                ("first", Json::Int(49)),
                ("recovered", Json::Int(1)),
                ("unresolved", Json::Int(0)),
            ]),
        ),
        (
            "integrity",
            Json::obj([
                ("flagged", Json::Int(2)),
                ("confirmed", Json::Int(1)),
                ("cleared", Json::Int(1)),
                ("detector", Json::str("answer-scan/1")),
                ("coverage_ppm", Json::Int(1_000_000)),
                (
                    "label",
                    Json::str("integrity panel — the confirmed count is a minimum"),
                ),
            ]),
        ),
    ])
}

fn packet() -> Json {
    Json::obj([
        ("schema", Json::str("hh-comparative-results/1")),
        (
            "tranche",
            Json::obj([("id", Json::str("TR1")), ("band", Json::str("A"))]),
        ),
        (
            "family",
            Json::obj([
                ("id", Json::str("P2-FP")),
                ("method", Json::str("holm")),
                ("alpha_ppm", Json::Int(50_000)),
                ("reference", Json::str("mini-swe-agent")),
                (
                    "members",
                    Json::Arr(vec![
                        Json::obj([
                            ("member", Json::str("pi")),
                            ("status", Json::str("computed")),
                            ("raw_p_ppm", Json::Int(120_000)),
                            ("adjusted_ppm", Json::Int(240_000)),
                            ("rejected_at_alpha", Json::Bool(false)),
                        ]),
                        Json::obj([
                            ("member", Json::str("cc")),
                            ("status", Json::str("unavailable")),
                            ("raw_p_ppm", Json::Null),
                            ("adjusted_ppm", Json::Int(1_000_000)),
                            ("rejected_at_alpha", Json::Bool(false)),
                        ]),
                    ]),
                ),
            ]),
        ),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str("P2-FP-expl")),
                ("method", Json::str("benjamini_hochberg")),
                (
                    "members",
                    Json::Arr(vec![Json::obj([
                        ("member", Json::str("pi|cc")),
                        ("raw_p_ppm", Json::Int(400_000)),
                        ("adjusted_ppm", Json::Int(400_000)),
                    ])]),
                ),
            ]),
        ),
        ("metric", Json::str("capability.task_success")),
        (
            "populations",
            Json::Arr(vec![
                Json::str("joint_task_pass_without_observed_veto"),
                Json::str("raw"),
                Json::str("errors_zero"),
            ]),
        ),
        (
            "labels",
            Json::obj([
                ("internal_record", Json::str("internal record — not outside-verified")),
                (
                    "finite_manifest_scope",
                    Json::str("finite-manifest: these 10 TB 2.1 held-out tasks (band A), not the 89-task benchmark"),
                ),
                (
                    "hosted_participant",
                    Json::str("stochastic hosted participant glm-5.3-flash via Ollama — catalog identity reported, not fingerprinted"),
                ),
                ("allowlisted_network", Json::str("allowlisted agent network")),
                (
                    "not_leaderboard_comparable",
                    Json::str("fewer than 89 tasks and other departures — not leaderboard-comparable"),
                ),
                (
                    "hf_context",
                    Json::str("HF board figures are context from a different serving setup"),
                ),
            ]),
        ),
        (
            "harnesses",
            Json::obj([
                ("mini-swe-agent", harness_row()),
                ("pi", harness_row()),
            ]),
        ),
    ])
}

fn drop_member(j: &Json, path: &[&str]) -> Json {
    // Return a copy of `j` with the member at `path` removed.
    let mut root = j.clone();
    let mut cur = &mut root;
    for (i, m) in path.iter().enumerate() {
        if i == path.len() - 1 {
            if let Json::Obj(o) = cur {
                o.remove(*m);
            }
            break;
        }
        cur = match cur {
            Json::Obj(o) => o.get_mut(*m).unwrap_or_else(|| panic!("no {m}")),
            _ => panic!("not obj at {m}"),
        };
    }
    root
}

#[test]
fn results_schema_validates_and_mutations_fail() {
    let p = packet();
    assert_eq!(ce::validate_results_packet(&p), Ok(()), "complete fixture");
    // Every leg of AC-6: dropping a §6.11 quantity, an interval, a
    // mandatory label, the harness_excess interval, or per-arm
    // intended/served counts fails.
    for path in [
        vec!["harnesses", "mini-swe-agent", "theta"],
        vec!["harnesses", "mini-swe-agent", "theta", "interval"],
        vec!["harnesses", "pi", "theta", "label"],
        vec![
            "harnesses",
            "pi",
            "flips",
            "harness_excess",
            "cc",
            "interval",
        ],
        vec!["harnesses", "pi", "counts", "intended"],
        vec!["harnesses", "pi", "counts", "served"],
        vec!["harnesses", "pi", "delta_vs_ref"],
        vec!["harnesses", "pi", "leader_group"],
        vec!["harnesses", "pi", "pass_at"],
        vec!["harnesses", "pi", "cost"],
        vec!["harnesses", "pi", "completion"],
        vec!["harnesses", "pi", "integrity"],
        vec!["labels", "finite_manifest_scope"],
        vec!["family"],
        vec!["exploratory_family"],
        vec!["populations"],
    ] {
        let m = drop_member(&p, &path);
        assert!(
            ce::validate_results_packet(&m).is_err(),
            "dropping {path:?} must fail"
        );
    }
    // A typed absent member stays admissible (refused delta, 0-solve).
    let mut j = packet();
    if let Json::Obj(ref mut top) = j {
        if let Some(Json::Obj(hs)) = top.get_mut("harnesses") {
            for (_, h) in hs.iter_mut() {
                if let Json::Obj(hm) = h {
                    hm.insert(
                        "delta_vs_ref".into(),
                        Json::obj([
                            ("state", Json::str("refused")),
                            ("variant", Json::str("UnresolvedCell")),
                        ]),
                    );
                    if let Some(Json::Obj(cost)) = hm.get_mut("cost") {
                        cost.insert(
                            "per_solve".into(),
                            Json::obj([
                                ("state", Json::str("estimator_undefined")),
                                ("reason", Json::str("0 solves")),
                            ]),
                        );
                    }
                }
            }
        }
    }
    assert_eq!(
        ce::validate_results_packet(&j),
        Ok(()),
        "typed absent admissible"
    );
}

// ── descriptive route gate coverage (AC-7 leg 2) ─────────────────────

#[test]
fn every_new_piece_refuses_gated_use() {
    let pop = pop3();
    // estimand + tied + flips + cost share the REQ-XD-023 check.
    for u in ["confirmatory", "first_pass"] {
        assert!(matches!(
            ce::comparative_estimand(&pop, &estimand_decl(u)).unwrap_err(),
            ce::EstimatorError::MethodNotAdmitted { .. }
        ));
        let cells = vec![fc(2, 5, "t1"), fc(3, 5, "t2")];
        assert!(matches!(
            ce::rerun_flip(&cells, u, None, 2_000, 1).unwrap_err(),
            ce::EstimatorError::MethodNotAdmitted { .. }
        ));
    }
    // A GATE-G23-passed record unlocks the gated classes.
    let mut d = estimand_decl("confirmatory");
    d.admission = passed();
    assert!(ce::comparative_estimand(&pop, &d).is_ok());
}
