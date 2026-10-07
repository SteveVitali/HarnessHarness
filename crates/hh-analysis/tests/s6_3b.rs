//! s6_3b — the designed causal-attribution suite (S6.3b; R-2.9.7 6c;
//! AC-R-2.9.7-{1,2,3,5,6,7,8,9,11,12,14}; KA-I7-1/2/3/5/6/7/8/9/11/13/14).
//!
//! The executed-arm legs are *synthetic drivers*: the test plays the
//! role the boundary's Group W `counterfactual` executor plays — every
//! planned `ArmCell` becomes an `ArmOutcome` drawn from the closed
//! planted world by a seeded bit (the hermetic equivalent of the
//! KA-I7-7 "fake/recording gateway whose control decisions follow a
//! declared conditional distribution"). `attribute`'s fold is then the
//! code under test: estimand floors, the label ceiling, V-legs, the
//! locus, the mediation split, Shapley + antithetic + the coalition
//! cap, `unattributed_share`, report determinism and the hosted
//! admissibility ladder.

use hh_analysis::attribution::{
    self, AttributionDesign, AttributionError, AttributionHypothesis, ComponentTarget,
    CoupledSource, DESIGN_SCHEMA, TARGET_CAP,
};
use hh_eval::stats::PPM;
use hh_wire::json::Json;
use hh_wire::sha256::sha256_hex;

// ── synthetic driver ────────────────────────────────────────────────────────

/// A deterministic Bernoulli bit in ppm — `H(material)` below `p_ppm`.
fn bit(material: &str, p_ppm: i64) -> i64 {
    let h = sha256_hex(material.as_bytes());
    let v = u64::from_str_radix(&h[..16], 16).expect("hex") % (PPM as u64);
    if (v as i64) < p_ppm {
        PPM
    } else {
        0
    }
}

/// The planted world: `factual_p` when the arm stays factual,
/// `counterfactual_p` under the ablation, `direct_p` under M5's
/// mediation arm. `change_rate` marks the factual arm's action-changed
/// share; `deferred` pauses the counterfactual suffix (V6);
/// `consumed` names the post-fork sources (V10).
struct World {
    factual_p: i64,
    counterfactual_p: i64,
    direct_p: Option<i64>,
    change_rate: i64,
    deferred: bool,
    consumed: Vec<String>,
    validity: &'static str,
    /// `true` — the outcome is the rate verbatim (a unit-scale measure,
    /// not a Bernoulli draw) for exact planted splits.
    unit: bool,
}

impl World {
    fn pivotal(on: i64, off: i64) -> World {
        World {
            factual_p: on,
            counterfactual_p: off,
            direct_p: None,
            change_rate: PPM,
            deferred: false,
            consumed: vec!["environment_state".into(), "provider_sampling".into()],
            validity: "deterministic",
            unit: false,
        }
    }
}

/// Execute the plan: one `ArmOutcome` per planned cell, seeded by
/// `(seed, key)` — under `crn` the factual/counterfactual share the
/// replicate's exogenous draw (the cell's `seed` member already couples
/// them; the world consumes it).
fn execute(
    d: &AttributionDesign,
    plan: &[attribution::ArmCell],
    world: &World,
) -> Vec<attribution::ArmOutcome> {
    plan.iter()
        .map(|cell| {
            let p = match cell.role.as_str() {
                "factual" => world.factual_p,
                "direct" => world.direct_p.unwrap_or(world.counterfactual_p),
                _ => world.counterfactual_p,
            };
            let outcome = if cell.role != "factual" && world.deferred {
                None
            } else if world.unit {
                Some(p)
            } else {
                Some(bit(&format!("{}|{}", d.seed, cell.seed), p))
            };
            attribution::ArmOutcome {
                target: cell.target.clone(),
                fork_point: cell.fork_point,
                role: cell.role.clone(),
                replicate_index: cell.replicate_index,
                outcome,
                validity: world.validity.to_string(),
                change_rate_ppm: if cell.role == "factual" {
                    Some(world.change_rate)
                } else {
                    None
                },
                deferred_paused: cell.role != "factual" && world.deferred,
                consumed_sources: world.consumed.clone(),
                spend: 1,
            }
        })
        .collect()
}

/// A multi-target world — per-target planted effects keyed by
/// `target ref → (factual_p, counterfactual_p)`; absent targets hold at
/// `factual_p` (the AND-interaction / run-forward fixtures compose it).
fn execute_multi(
    d: &AttributionDesign,
    plan: &[attribution::ArmCell],
    factual_p: i64,
    effects: &std::collections::BTreeMap<String, i64>,
) -> Vec<attribution::ArmOutcome> {
    plan.iter()
        .map(|cell| {
            let p = match cell.role.as_str() {
                "factual" => factual_p,
                _ => *effects.get(&cell.target).unwrap_or(&factual_p),
            };
            attribution::ArmOutcome {
                target: cell.target.clone(),
                fork_point: cell.fork_point,
                role: cell.role.clone(),
                replicate_index: cell.replicate_index,
                outcome: Some(bit(&format!("{}|{}", d.seed, cell.seed), p)),
                validity: "deterministic".to_string(),
                change_rate_ppm: if cell.role == "factual" {
                    Some(PPM)
                } else {
                    None
                },
                deferred_paused: false,
                consumed_sources: vec![
                    "environment_state".to_string(),
                    "provider_sampling".to_string(),
                ],
                spend: 1,
            }
        })
        .collect()
}

fn target(kind: &str, r: &str) -> ComponentTarget {
    ComponentTarget {
        kind: kind.into(),
        ref_: r.into(),
    }
}

fn coupled() -> Vec<CoupledSource> {
    vec![
        CoupledSource {
            source: "environment_state".into(),
            coupling_agreement: true,
        },
        CoupledSource {
            source: "provider_sampling".into(),
            coupling_agreement: true,
        },
    ]
}

/// A TE_crn M2 design over `targets` — deterministic + crn, k per the
/// outcome floor, `run_start` fork (or `every_decision_point` when
/// `fps` is `Some`).
fn design_m2_crn(targets: Vec<ComponentTarget>, fps: Option<Vec<i64>>) -> AttributionDesign {
    AttributionDesign {
        design_ref: None,
        configuration_id: "cfg-1".into(),
        run_ids: vec![],
        task_ids: vec![],
        method: "M2".into(),
        targets,
        fork_policy: if fps.is_some() {
            "every_decision_point".into()
        } else {
            "run_start".into()
        },
        fork_kind: None,
        k: 4,
        replay_mode_requested: "deterministic".into(),
        noise_coupling: "crn".into(),
        coupling_assumption: "weak".into(),
        match_spec: Json::obj([("mode", Json::str("matched_total"))]),
        outcome: "task_success".into(),
        outcome_oracle_class: None,
        outcome_calibration_ref: None,
        budget_reserved: 0,
        seed: 42,
        claim_kind: "outcome".into(),
        pre_registration_ref: None,
        coupled_sources: coupled(),
        n_permutations: 0,
        samples_per_eval: 0,
        antithetic: false,
        m4_factor: None,
        m4_levels: vec![],
        equivalence_margin: None,
        delta_total: None,
        label: "confirmatory".into(),
        hosted: false,
        intervention_kind: None,
        granularity: None,
        interception: None,
    }
}

fn effect_point(report: &Json, target_ref: &str, fork_seq: i64) -> Option<i64> {
    report
        .get("effects")
        .and_then(|e| match e {
            Json::Arr(a) => Some(a),
            _ => None,
        })?
        .iter()
        .find(|e| {
            e.get("target")
                .and_then(|t| t.get("ref"))
                .and_then(Json::as_str)
                == Some(target_ref)
                && e.get("fork_seq").and_then(Json::as_int) == Some(fork_seq)
        })
        .and_then(|e| e.get("point"))
        .and_then(Json::as_int)
}

// ── KA-I7-1 — seed coupling recovers the planted effect ─────────────────────

#[test]
fn ka_i7_1_crn_pair_isolates_the_planted_effect() {
    let d = design_m2_crn(vec![target("rule", "retrieval-ranking")], None);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    // Under crn the factual + counterfactual arms of replicate i share
    // the branch seed — the world's exogenous draw is the same, so the
    // residual diff is the planted effect, not the noise.
    // The degenerate conditional distribution — the shared exogenous
    // draw always lands on the same side of both thresholds, so the
    // only residual is the planted ablation (counterfactual − factual).
    let world = World::pivotal(1_000_000, 0);
    let outcomes = execute(&d, &plan, &world);
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report.get("attribution_label").and_then(Json::as_str),
        Some("causal_coupled")
    );
    assert_eq!(
        report.get("estimand").and_then(Json::as_str),
        Some("TE_crn")
    );
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("confirmatory")
    );
    // A beneficial target under `counterfactual − factual` (§5h.7's
    // contrast direction): removing it costs −1_000_000 ppm.
    let point = effect_point(&report, "retrieval-ranking", 0).expect("point");
    assert_eq!(point, -1_000_000, "the residual isolates the ablation");
    let interval = report
        .get("effects")
        .and_then(|e| match e {
            Json::Arr(a) => a.first().cloned(),
            _ => None,
        })
        .and_then(|e| e.get("interval").cloned())
        .expect("interval");
    assert!(
        interval.get("hi").and_then(Json::as_int).unwrap_or(0) < 0,
        "the planted effect's interval excludes 0: {interval:?}"
    );
    // The counterexample — an uncoupled source the design did not
    // declare with measured agreement refuses (V10, never silent).
    let mut bad = outcomes.clone();
    for o in &mut bad {
        o.consumed_sources = vec!["environment_state".into(), "sneaky_rng".into()];
    }
    assert_eq!(
        attribution::attribute(&d, &bad, &plan),
        Err(AttributionError::CouplingUnavailable {
            source: "sneaky_rng".into()
        })
    );
    // … and a `deterministic` design without `crn` cannot even plan a
    // coupled estimand (V1 — the label never rides a weaker claim).
    let uncoupled = AttributionDesign {
        noise_coupling: "none".into(),
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert_eq!(
        attribution::validate_design(&uncoupled),
        Err(AttributionError::EstimandBelowFloor {
            method: "M2".into(),
            floor: "noise_coupling = crn".into()
        })
    );
}

// ── KA-I7-2 — PolicyCollapsed refuses when the arm never changed ────────────

#[test]
fn ka_i7_2_policy_collapsed_refuses() {
    let d = design_m2_crn(vec![target("decision_point", "run-1:7")], None);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    // The factual arm never changed action at the fork — change_rate 0.
    let world = World {
        change_rate: 0,
        ..World::pivotal(900_000, 300_000)
    };
    let outcomes = execute(&d, &plan, &world);
    assert_eq!(
        attribution::attribute(&d, &outcomes, &plan),
        Err(AttributionError::PolicyCollapsed { seq: 0 })
    );
}

// ── KA-I7-3 — deferred pause reports n/a + deferred_paused ─────────────────

#[test]
fn ka_i7_3_deferred_pause_reports_process_not_outcome() {
    let d = design_m2_crn(vec![target("decision_point", "run-1:7")], None);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let world = World {
        deferred: true,
        ..World::pivotal(900_000, 300_000)
    };
    let outcomes = execute(&d, &plan, &world);
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    let e = match report.get("effects") {
        Some(Json::Arr(a)) => a[0].clone(),
        _ => panic!("effects"),
    };
    assert_eq!(
        e.get("deferred_paused").and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(true)
    );
    // The process effect survives (change_rate), the outcome is not_run.
    assert!(
        e.get("point").is_none(),
        "a paused branch carries no outcome point"
    );
    assert!(
        e.get("change_rate").and_then(Json::as_int).unwrap_or(0) > 0,
        "the process effect is still reported"
    );
}

// ── KA-I7-5 — locus: point_of_commitment vs earliest_direct ─────────────────

#[test]
fn ka_i7_5_locus_names_the_commitment_point() {
    // Two decision-point targets at seqs 7 and 12: t7's effect is real
    // but tiny (its interval includes 0 at this power); t12's is large.
    let d = design_m2_crn(
        vec![
            target("decision_point", "run-1:7"),
            target("decision_point", "run-1:12"),
        ],
        Some(vec![7, 12]),
    );
    let plan = attribution::attribution_plan(&d, &[7, 12]).expect("plan");
    let outcomes = plan
        .iter()
        .map(|cell| {
            let p = match (cell.role.as_str(), cell.target.as_str()) {
                ("factual", _) => 500_000,
                // t@12: big planted effect at its own seq only.
                (_, "run-1:12") if cell.fork_point == 12 => 950_000,
                // t@7: a small effect — point ≠ 0 (earliest_direct)
                // but the interval does not exclude 0.
                (_, "run-1:7") if cell.fork_point == 7 => 700_000,
                _ => 500_000,
            };
            attribution::ArmOutcome {
                target: cell.target.clone(),
                fork_point: cell.fork_point,
                role: cell.role.clone(),
                replicate_index: cell.replicate_index,
                outcome: Some(bit(&format!("{}|{}", d.seed, cell.seed), p)),
                validity: "deterministic".into(),
                change_rate_ppm: Some(PPM),
                deferred_paused: false,
                consumed_sources: coupled().iter().map(|c| c.source.clone()).collect(),
                spend: 1,
            }
        })
        .collect::<Vec<_>>();
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    let locus = report.get("locus").expect("locus member");
    assert_eq!(
        locus.get("rule").and_then(Json::as_str),
        Some("point_of_commitment")
    );
    assert_eq!(locus.get("seq").and_then(Json::as_int), Some(12));
    assert_eq!(locus.get("target").and_then(Json::as_str), Some("run-1:12"));
    // `earliest_direct` rides beside — the first seq with a non-zero
    // point — and disagreement is reported, never resolved.
    let ed = locus.get("earliest_direct").expect("earliest_direct");
    let ed_seq = ed.get("seq").and_then(Json::as_int);
    assert!(
        ed_seq == Some(7) || locus.get("disagreement").is_some(),
        "earliest_direct {ed_seq:?} vs point_of_commitment 12 — disagreement reported"
    );
}

// ── KA-I7-6 — M5 mediation split + the exclusion ranking ───────────────────

#[test]
fn ka_i7_6_mediation_split_and_exclusion() {
    let d = AttributionDesign {
        method: "M5".into(),
        claim_kind: "outcome".into(),
        k: 4,
        ..design_m2_crn(vec![target("decision_point", "run-1:5")], None)
    };
    assert_eq!(attribution::validate_design(&d), Ok(()));
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    // Planted: TE_crn = +400k, DE = +250k ⇒ ME = +150k.
    let world = World {
        factual_p: 200_000,
        counterfactual_p: 600_000,
        direct_p: Some(450_000),
        change_rate: PPM,
        deferred: false,
        consumed: coupled().iter().map(|c| c.source.clone()).collect(),
        validity: "deterministic",
        unit: true,
    };
    let outcomes = execute(&d, &plan, &world);
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("exploratory"),
        "M5 stays exploratory until OQ-449 closes"
    );
    let e = match report.get("effects") {
        Some(Json::Arr(a)) => a[0].clone(),
        _ => panic!("effects"),
    };
    let de = e.get("DE").and_then(Json::as_int).expect("DE");
    let me = e.get("ME").and_then(Json::as_int).expect("ME");
    let te = e.get("TE_crn").and_then(Json::as_int).expect("TE_crn");
    assert_eq!(te, de + me, "the split is an identity");
    assert!(de > 0 && me > 0, "both planted legs recovered");

    // The degenerate case — opposite-signed DE/TE excludes the target
    // from the mediated-share ranking (inconclusive, never inert).
    let flipped = World {
        factual_p: 500_000,
        counterfactual_p: 100_000, // TE < 0
        direct_p: Some(800_000),   // DE > 0
        ..world
    };
    // unit-mode carries through `..world`.
    let outcomes = execute(&d, &plan, &flipped);
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    let e = match report.get("effects") {
        Some(Json::Arr(a)) => a[0].clone(),
        _ => panic!("effects"),
    };
    let de = e.get("DE").and_then(Json::as_int).expect("DE");
    let te = e.get("TE_crn").and_then(Json::as_int).expect("TE");
    assert!((de > 0 && te < 0) || (de < 0 && te > 0), "opposite-signed");
    assert_eq!(
        e.get("mediated_share_excluded").and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(true)
    );
    assert_eq!(
        e.get("verdict").and_then(Json::as_str),
        Some("inconclusive")
    );
    assert_eq!(
        report.get("exclusion_rate").and_then(Json::as_int),
        Some(1_000_000)
    );
}

// ── KA-I7-7 — the known-effect suite (fixture half) ─────────────────────────

#[test]
fn ka_i7_7_known_effect_suite_recovers_pivots() {
    // World 1 — a single pivotal rule: the suite must name it and the
    // interval must exclude 0.
    let d = design_m2_crn(vec![target("rule", "pivotal")], None);
    let _plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let mut recovered = 0usize;
    let runs = 4usize;
    for r in 0..runs {
        let d_r = AttributionDesign {
            seed: 1000 + r as i64,
            ..d.clone()
        };
        let plan_r = attribution::attribution_plan(&d_r, &[]).expect("plan");
        let outcomes = execute(&d_r, &plan_r, &World::pivotal(1_000_000, 0));
        let report = attribution::attribute(&d_r, &outcomes, &plan_r).expect("attribute");
        let iv = match report.get("effects") {
            Some(Json::Arr(a)) => a[0].get("interval").cloned(),
            _ => None,
        };
        if let Some(i) = iv {
            let lo = i.get("lo").and_then(Json::as_int).unwrap_or(0);
            let hi = i.get("hi").and_then(Json::as_int).unwrap_or(0);
            if lo > 0 || hi < 0 {
                recovered += 1;
            }
        }
    }
    assert_eq!(
        recovered, runs,
        "the pivotal world must recover in every run ({recovered}/{runs})"
    );

    // World 2 — the AND-interaction: neither single target moves the
    // outcome alone; the pair does. M2's single-target effects are
    // ~0 (the honest single-target answer — the interaction rides M3).
    let d_and = design_m2_crn(vec![target("slot", "a"), target("slot", "b")], None);
    let plan = attribution::attribution_plan(&d_and, &[]).expect("plan");
    let outcomes = execute_multi(
        &d_and,
        &plan,
        500_000,
        &[(String::from("a"), 500_000), (String::from("b"), 500_000)]
            .into_iter()
            .collect(),
    );
    let report = attribution::attribute(&d_and, &outcomes, &plan).expect("attribute");
    for e in match report.get("effects") {
        Some(Json::Arr(a)) => a,
        _ => panic!(),
    } {
        let point = e.get("point").and_then(Json::as_int).unwrap_or(0).abs();
        assert!(
            point < 300_000,
            "a single member of an AND pair does not move the outcome alone"
        );
    }

    // World 3 — run-forward: the *earlier* inert point must NOT win the
    // locus against the later pivotal point.
    let d_rf = design_m2_crn(
        vec![
            target("decision_point", "r:3"),
            target("decision_point", "r:9"),
        ],
        Some(vec![3, 9]),
    );
    let plan = attribution::attribution_plan(&d_rf, &[3, 9]).expect("plan");
    let outcomes = plan
        .iter()
        .map(|cell| {
            let p = match (cell.role.as_str(), cell.target.as_str()) {
                ("factual", _) => 1_000_000,
                (_, "r:3") => 1_000_000,                 // inert
                (_, "r:9") if cell.fork_point == 9 => 0, // pivotal (cf − fact)
                _ => 1_000_000,
            };
            attribution::ArmOutcome {
                target: cell.target.clone(),
                fork_point: cell.fork_point,
                role: cell.role.clone(),
                replicate_index: cell.replicate_index,
                outcome: Some(bit(&format!("{}|{}", d_rf.seed, cell.seed), p)),
                validity: "deterministic".into(),
                change_rate_ppm: Some(PPM),
                deferred_paused: false,
                consumed_sources: vec![],
                spend: 1,
            }
        })
        .collect::<Vec<_>>();
    let report = attribution::attribute(&d_rf, &outcomes, &plan).expect("attribute");
    let locus = report.get("locus").expect("locus");
    assert_eq!(locus.get("seq").and_then(Json::as_int), Some(9));
    assert_eq!(locus.get("target").and_then(Json::as_str), Some("r:9"));
}

// ── KA-I7-8 — Shapley: additive recovery, antithetic, the cap ───────────────

#[test]
fn ka_i7_8_shapley_recovers_additive_and_enforces_v11() {
    let mut d = design_m2_crn(
        vec![
            target("slot", "a"),
            target("slot", "b"),
            target("slot", "c"),
        ],
        None,
    );
    d.method = "M3".into();
    d.n_permutations = 4;
    d.samples_per_eval = 4;
    d.antithetic = true;
    d.k = 4;
    assert_eq!(attribution::validate_design(&d), Ok(()));
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    // The additive world — v(S) = Σ w_t over the held coalition.
    let w: std::collections::BTreeMap<String, i64> = [
        ("a".to_string(), 200_000i64),
        ("b".to_string(), 300_000),
        ("c".to_string(), 50_000),
    ]
    .into_iter()
    .collect();
    let outcomes: Vec<attribution::ArmOutcome> = plan
        .iter()
        .map(|cell| {
            let v: i64 = 100_000 + cell.subset.iter().map(|t| w[t]).sum::<i64>();
            attribution::ArmOutcome {
                target: cell.target.clone(),
                fork_point: cell.fork_point,
                role: cell.role.clone(),
                replicate_index: cell.replicate_index,
                outcome: Some(v),
                validity: "deterministic".into(),
                change_rate_ppm: None,
                deferred_paused: false,
                consumed_sources: vec![],
                spend: 1,
            }
        })
        .collect();
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report.get("estimand").and_then(Json::as_str),
        Some("shapley")
    );
    let values = match report.get("shapley").and_then(|s| s.get("values")) {
        Some(Json::Arr(v)) => v.clone(),
        _ => panic!("shapley.values"),
    };
    for (name, want) in [("a", 200_000i64), ("b", 300_000), ("c", 50_000)] {
        let got = values
            .iter()
            .find(|v| v.get("target").and_then(Json::as_str) == Some(name))
            .and_then(|v| v.get("point"))
            .and_then(Json::as_int)
            .expect("φ");
        assert!(
            (got - want).abs() <= 1,
            "φ_{name}: want {want}, got {got} (additive world)"
        );
    }
    // Efficiency — Σφ = v(N) − v(∅) within the interval (V11).
    let eff = report
        .get("shapley")
        .and_then(|s| s.get("efficiency_check"))
        .and_then(Json::as_int)
        .expect("efficiency_check");
    assert_eq!(eff, 0, "the additive world balances exactly");
    // The refusal legs: no antithetic ⇒ refuse; 9 targets ⇒ the cap.
    let no_anti = AttributionDesign {
        antithetic: false,
        ..d.clone()
    };
    assert!(matches!(
        attribution::validate_design(&no_anti),
        Err(AttributionError::Schema(_))
    ));
    let too_many = AttributionDesign {
        targets: (0..TARGET_CAP + 1)
            .map(|i| target("slot", &format!("t{i}")))
            .collect(),
        ..d.clone()
    };
    assert_eq!(
        attribution::validate_design(&too_many),
        Err(AttributionError::TooManyTargets {
            n: TARGET_CAP + 1,
            cap: TARGET_CAP
        })
    );
}

// ── KA-I7-9 — unattributed_share ────────────────────────────────────────────

#[test]
fn ka_i7_9_unattributed_share_is_a_report_fact() {
    let mut d = design_m2_crn(vec![target("rule", "r")], None);
    d.delta_total = Some(500_000);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let outcomes = execute(&d, &plan, &World::pivotal(800_000, 500_000));
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    let u = report
        .get("unattributed_share")
        .and_then(Json::as_int)
        .expect("unattributed_share");
    let point = effect_point(&report, "r", 0).expect("point");
    assert_eq!(u, 500_000 - point, "unattributed = Δ_total − Σ effects");
}

// ── KA-I7-11 — report determinism ───────────────────────────────────────────

#[test]
fn ka_i7_11_identical_inputs_mint_identical_reports() {
    let d = design_m2_crn(vec![target("rule", "r")], None);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let outcomes = execute(&d, &plan, &World::pivotal(900_000, 400_000));
    let r1 = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    let r2 = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    assert_eq!(r1.to_canonical_string(), r2.to_canonical_string());
    assert_eq!(
        r1.get("report_id").and_then(Json::as_str),
        r2.get("report_id").and_then(Json::as_str)
    );
}

// ── KA-I7-13 — hosted admissibility ladder ──────────────────────────────────

#[test]
fn ka_i7_13_hosted_rows_carry_na_class() {
    // A hosted subject with component-level targets ⇒ ClassInadmissible.
    let hosted = AttributionDesign {
        hosted: true,
        ..design_m2_crn(vec![target("slot", "retriever")], None)
    };
    assert!(matches!(
        attribution::validate_design(&hosted),
        Err(AttributionError::ClassInadmissible { .. })
    ));
    // The exception — observation_substitution at configuration
    // granularity under model_io interception — is admissible.
    let exception = AttributionDesign {
        hosted: true,
        intervention_kind: Some("observation_substitution".into()),
        granularity: Some("configuration".into()),
        interception: Some("model_io".into()),
        ..design_m2_crn(vec![target("parameter", "retrieval.k")], None)
    };
    assert_eq!(attribution::validate_design(&exception), Ok(()));
    // attribution_quality on hosted ⇒ n/a{class}; without reports ⇒
    // n/a{not_run}; Δ's interval through 0 ⇒ n/a{estimator_undefined}.
    let q = attribution::attribution_quality(&Json::obj([("point", Json::Int(1))]), &[], true);
    assert_eq!(q.get("n/a").and_then(Json::as_str), Some("class"));
    let q = attribution::attribution_quality(&Json::obj([("point", Json::Int(1))]), &[], false);
    assert_eq!(q.get("n/a").and_then(Json::as_str), Some("not_run"));
    let zero_iv = Json::obj([
        ("point", Json::Int(100_000)),
        (
            "interval",
            Json::obj([("lo", Json::Int(-5)), ("hi", Json::Int(200_000))]),
        ),
    ]);
    let report = Json::obj([("effects", Json::Arr(vec![]))]);
    let q = attribution::attribution_quality(&zero_iv, &[report], false);
    assert_eq!(
        q.get("n/a").and_then(Json::as_str),
        Some("estimator_undefined")
    );
}

// ── KA-I7-14 — no causal label without the declared coupling ────────────────

#[test]
fn ka_i7_14_label_ceiling_and_widening_refusals() {
    // TE_marg (re_executed, uncoupled) can never confirm.
    let mut d = design_m2_crn(vec![target("rule", "r")], None);
    d.replay_mode_requested = "re_executed".into();
    d.noise_coupling = "none".into();
    d.k = 8;
    d.label = "confirmatory".into();
    d.coupled_sources = vec![];
    assert_eq!(attribution::validate_design(&d), Ok(()));
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let mut world = World::pivotal(900_000, 400_000);
    world.validity = "re_executed";
    let outcomes = execute(&d, &plan, &world);
    let report = attribution::attribute(&d, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("exploratory"),
        "TE_marg never supports confirmatory"
    );
    assert_eq!(
        report.get("attribution_label").and_then(Json::as_str),
        Some("causal_interventional")
    );
    // The widening/loosening intervention refuses (governance leg).
    let t = target("parameter", "budget.max");
    let change = Json::obj([("kind", Json::str("parameter_change"))]);
    let cls = Json::obj([("authority_delta", Json::str("widening"))]);
    assert_eq!(
        attribution::intervention_for(&t, &change, Some(&cls)),
        Err(AttributionError::AuthorityWidening)
    );
    let cls = Json::obj([("budget_delta", Json::str("loosening"))]);
    assert_eq!(
        attribution::intervention_for(&t, &change, Some(&cls)),
        Err(AttributionError::BudgetLoosening)
    );
    // `permission_change` narrows only.
    let widen = Json::obj([
        ("kind", Json::str("permission_change")),
        ("direction", Json::str("widening")),
    ]);
    assert_eq!(
        attribution::intervention_for(&t, &widen, None),
        Err(AttributionError::AuthorityWidening)
    );
    let narrow = Json::obj([
        ("kind", Json::str("permission_change")),
        ("direction", Json::str("narrowing")),
    ]);
    let rec = attribution::intervention_for(&t, &narrow, None).expect("narrowing ok");
    assert_eq!(
        rec.get("kind").and_then(Json::as_str),
        Some("permission_change")
    );
    // The InterventionRecord kinds decode through the ADR-0135 codec —
    // no new kernel kind (ADR-0199 D3).
    let kinds = [
        "slot_disable",
        "variant_swap",
        "parameter_change",
        "rule_removal",
        "procedure_edit",
        "leaf_ablation",
        "observation",
        "response",
        "decision",
        "budget",
        "model_swap",
    ];
    for k in kinds {
        let rec = attribution::intervention_for(&t, &Json::obj([("kind", Json::str(k))]), None)
            .unwrap_or_else(|e| panic!("{k}: {e}"));
        assert!(attribution::INTERVENTION_KINDS
            .contains(&rec.get("kind").and_then(Json::as_str).unwrap_or("")));
    }
}

// ── the V-leg and plan/estimate surface ─────────────────────────────────────

#[test]
fn s6_3b_v_legs_and_rollout_estimate() {
    // Underpowered — k below the floor.
    let weak = AttributionDesign {
        k: 3,
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert_eq!(
        attribution::validate_design(&weak),
        Err(AttributionError::Underpowered { k: 3, required: 4 })
    );
    // The decision-sequence claim admits k = 1 under crn (OQ-323).
    let seq_claim = AttributionDesign {
        k: 1,
        claim_kind: "decision_sequence".into(),
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert_eq!(attribution::validate_design(&seq_claim), Ok(()));
    // Missing match.
    let mut j = design_m2_crn(vec![target("rule", "r")], None).to_json();
    if let Json::Obj(m) = &mut j {
        m.remove("match");
    }
    assert_eq!(
        AttributionDesign::from_json(&j),
        Err(AttributionError::MissingMatchSpec)
    );
    // UnmatchedBudget — a non-matched_total mode refuses for M2.
    let unmatched = AttributionDesign {
        match_spec: Json::obj([("mode", Json::str("matched_tokens"))]),
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert!(matches!(
        attribution::validate_design(&unmatched),
        Err(AttributionError::UnmatchedBudget { .. })
    ));
    // crn with no declared coupling ⇒ CouplingUnavailable at validate.
    let naked = AttributionDesign {
        coupled_sources: vec![],
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert!(matches!(
        attribution::validate_design(&naked),
        Err(AttributionError::CouplingUnavailable { .. })
    ));
    // A declared-but-unmeasured coupling ⇒ same refusal.
    let unmeasured = AttributionDesign {
        coupled_sources: vec![CoupledSource {
            source: "env".into(),
            coupling_agreement: false,
        }],
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert_eq!(
        attribution::validate_design(&unmeasured),
        Err(AttributionError::CouplingUnavailable {
            source: "env".into()
        })
    );
    // `declared_strong` forces run_start — the effective policy degrades
    // to the M1 form, recorded on the report's assumptions.
    let strong = AttributionDesign {
        coupling_assumption: "declared_strong".into(),
        fork_policy: "every_decision_point".into(),
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    assert_eq!(attribution::effective_fork_policy(&strong), "run_start");
    let plan = attribution::attribution_plan(&strong, &[3, 9]).expect("plan");
    assert!(plan.iter().all(|c| c.fork_point == 0));
    let outcomes = execute(&strong, &plan, &World::pivotal(900_000, 300_000));
    let report = attribution::attribute(&strong, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report
            .get("assumptions")
            .and_then(|a| a.get("fork_policy"))
            .and_then(Json::as_str),
        Some("run_start")
    );
    assert!(report
        .get("reasons")
        .and_then(|r| match r {
            Json::Arr(a) => a
                .iter()
                .any(|x| x.as_str() == Some("declared_strong_forces_run_start"))
                .then_some(()),
            _ => None,
        })
        .is_some());
    // An invalid branch ⇒ ReplayInvalid (never a weaker label).
    let d = design_m2_crn(vec![target("rule", "r")], None);
    let plan = attribution::attribution_plan(&d, &[]).expect("plan");
    let mut bad = execute(&d, &plan, &World::pivotal(900_000, 400_000));
    bad[0].validity = "invalid".into();
    assert!(matches!(
        attribution::attribute(&d, &bad, &plan),
        Err(AttributionError::ReplayInvalid { .. })
    ));
    // A degraded branch ⇒ the report's label is exploratory.
    let mut degraded = execute(&d, &plan, &World::pivotal(900_000, 400_000));
    degraded[0].validity = "degraded".into();
    let report = attribution::attribute(&d, &degraded, &plan).expect("attribute");
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("exploratory")
    );
    // estimate_rollouts — the scheduler's pre-reservation price.
    let mut m3 = design_m2_crn(vec![target("slot", "a"), target("slot", "b")], None);
    m3.method = "M3".into();
    m3.n_permutations = 6;
    m3.samples_per_eval = 4;
    m3.antithetic = true;
    assert_eq!(
        attribution::estimate_rollouts(&m3, 1),
        6 * 2 * (2 + 1) * 4,
        "n_permutations · antithetic · (|T|+1) · samples_per_eval"
    );
    let m2 = design_m2_crn(vec![target("slot", "a")], None);
    assert_eq!(attribution::estimate_rollouts(&m2, 5), (5 + 1) * 2 * 4);
    // Budget truncation — the plan caps at the reservation and the
    // report carries `budget_truncated` (I4, never an overspend).
    let capped = AttributionDesign {
        budget_reserved: 4,
        ..design_m2_crn(vec![target("rule", "r")], None)
    };
    let plan = attribution::attribution_plan(&capped, &[]).expect("plan");
    assert_eq!(plan.len(), 4);
    let outcomes = execute(&capped, &plan, &World::pivotal(900_000, 300_000));
    let report = attribution::attribute(&capped, &outcomes, &plan).expect("attribute");
    assert_eq!(
        report
            .get("budget")
            .and_then(|b| b.get("budget_truncated"))
            .and_then(|j| match j {
                Json::Bool(b) => Some(*b),
                _ => None,
            }),
        Some(true)
    );
    // The design codec round-trips.
    let j = design_m2_crn(vec![target("rule", "r")], None).to_json();
    assert_eq!(j.get("schema").and_then(Json::as_str), Some(DESIGN_SCHEMA));
    let back = AttributionDesign::from_json(&j).expect("decode");
    assert_eq!(back.method, "M2");
    // The M0 hypothesis is observational — never a value.
    let h = AttributionHypothesis::mint(
        vec![target("rule", "r")],
        "the retrieval rule looks responsible",
        "validator:judge-1",
        "judge",
    );
    let hj = h.to_json();
    assert_eq!(
        hj.get("label").and_then(Json::as_str),
        Some("observational")
    );
}
