//! R6.29 Stage-1 acceptance battery (manifest 283; FP runner rules):
//! AC-1 (REQ-XH-112) — outcome-field manifest refuses; round-2
//! checkpoint breach withholds V5 → Claude Code → Pi; protected-pair
//! breach withholds every remaining slot; a round-boundary fixture
//! withholds V5 before any HH-native slot; a wall-time shortfall
//! withholds in the same order. AC-2 (REQ-XH-117) — over-cap arm
//! withheld at slot admission inside the overshoot bound; a p̂/1.8
//! arm completes all 50 slots; the family retains the unavailable
//! member; the µ$ cap table emits. AC-3 (REQ-XD-040) — an over-cap
//! run stops without overshoot beyond in-flight trials; unrun slots
//! `withheld{budget}`; cost a censored lower bound. AC-4
//! (REQ-XH-126) — FP-E drop order under W2 measurement; boundary
//! shortfall same order; pair-atomic `withheld{window_deadline}`;
//! D_max releases Wc only when unauthorized and keeps the overflow
//! reservation. AC-5 (REQ-XD-042) — the four stop rules, partial
//! rows retained, no outcome trigger. USD $0 — fixture-verified.

use std::collections::BTreeMap;

use hharness_runner::continuation::{
    self, admit_slot, arm_cost_report, cap_record, cap_table, check_cost_only, d_max_ms,
    evaluate_stop, first_pass_family, overflow_reservation_micros, p_hat, run_checkpoint,
    stop_trigger_from_json, t_hat, w_p_ms, withhold_pair_deadline, AdmitVerdict, ArmPriority,
    CheckpointOutcome, ContinuationError, ContinuationInput, FpArm, OhPair, Scope, StopTrigger,
    TrialMeasure, W4Ceiling, WcTerm, WindowSum, WithholdAction, CAP_TABLE_SCHEMA,
    CHECKPOINT_SCHEMA, DROP_LABEL, FP_COMPUTE_CAP_MICROS, INPUT_SCHEMA, T_PRIOR_MS,
};
use hharness_runner::disposition::{Disposition, WithheldReason};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;

fn slots(arm: FpArm, n: usize, tag: &str) -> Vec<String> {
    (0..n)
        .map(|i| format!("{}-{}-{}", arm.as_str(), tag, i))
        .collect()
}

/// The §10.1.5 binding planning priors in integer µ$ (mini and both
/// HH-native arms $0.374; Pi $0.123; Claude Code $0.210).
fn priors() -> BTreeMap<FpArm, i64> {
    BTreeMap::from([
        (FpArm::OrigMini, 374_000),
        (FpArm::HhNative, 374_000),
        (FpArm::V5, 374_000),
        (FpArm::Pi, 123_000),
        (FpArm::ClaudeCode, 210_000),
    ])
}

/// A five-arm input: 50 remaining slots per arm, the recorded-default
/// priority, C = 4, s = $0.85, root $56 ($70 × (1 − .20)).
fn input(scope: Scope) -> ContinuationInput {
    ContinuationInput {
        scope,
        measured: BTreeMap::new(),
        remaining: FpArm::ALL
            .iter()
            .map(|a| (*a, slots(*a, 50, "r")))
            .collect(),
        consumed_by_arm: BTreeMap::new(),
        in_flight: BTreeMap::new(),
        consumed_micros: 0,
        orphaned_hi_micros: 0,
        root_cap_micros: 56_000_000,
        c: 4,
        slice_micros: 850_000,
        priors_micros: priors(),
        client_timeouts_ms: BTreeMap::new(),
        priority: ArmPriority::qx61_recorded_default(),
    }
}

fn measured(arm: FpArm, cost: i64, wall: i64, n: usize) -> Vec<TrialMeasure> {
    (0..n)
        .map(|i| TrialMeasure {
            slot: format!("{}-m{i}", arm.as_str()),
            cost_micros: cost,
            wall_ms: wall,
            queue_wait_ms: Some(10),
            latency_ms: Some(90),
            infra_failed: false,
        })
        .collect()
}

fn wall(d_limit_ms: i64) -> continuation::WallInput {
    continuation::WallInput {
        d_limit_ms,
        cutoff_ms: 2_340_000, // ≈ 0.65 h (the W4 bound + harvest)
        overhead_ms: 4_500_000,
        load_ms: 900_000,
        pairs: Vec::new(),
    }
}

fn arm_order(actions: &[WithholdAction]) -> Vec<FpArm> {
    actions.iter().map(|a| a.arm).collect()
}

// ── AC-1 (REQ-XH-112) ────────────────────────────────────────────────

#[test]
fn ac1_outcome_field_in_manifest_refuses_typed() {
    let base = Json::obj([
        ("schema", Json::str(INPUT_SCHEMA)),
        ("scope", Json::str("w4_checkpoint")),
        ("after_round", Json::Int(2)),
        ("measured", Json::obj([])),
        ("remaining", Json::obj([])),
        ("consumed_by_arm", Json::obj([])),
        ("in_flight", Json::obj([])),
        ("consumed_micros", Json::Int(0)),
        ("orphaned_hi_micros", Json::Int(0)),
        ("root_cap_micros", Json::Int(56_000_000)),
        ("c", Json::Int(4)),
        ("slice_micros", Json::Int(850_000)),
        ("priors_micros", Json::obj([])),
        ("client_timeouts_ms", Json::obj([])),
        (
            "priority",
            Json::obj([
                (
                    "protected",
                    Json::Arr(vec![Json::str("mini-swe-agent"), Json::str("hh_native")]),
                ),
                (
                    "drop_order",
                    Json::Arr(vec![
                        Json::str("v5"),
                        Json::str("claude-code"),
                        Json::str("pi"),
                    ]),
                ),
            ]),
        ),
    ]);
    ContinuationInput::from_json(&base).unwrap();
    // A verdict member anywhere refuses — nested inside a measured row.
    let mut bad = base.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert(
            "measured".to_string(),
            Json::obj([(
                "pi",
                Json::Arr(vec![Json::obj([
                    ("slot", Json::str("s")),
                    ("cost_micros", Json::Int(1)),
                    ("wall_ms", Json::Int(1)),
                    ("verdict", Json::str("resolved")),
                ])]),
            )]),
        );
    }
    assert_eq!(
        ContinuationInput::from_json(&bad).unwrap_err(),
        ContinuationError::OutcomeField {
            path: "measured.pi[0].verdict".to_string()
        }
    );
    // An executed{…} disposition member refuses; an infra one passes.
    assert!(check_cost_only(&Json::obj([(
        "disposition",
        Json::str("executed{resolved}")
    )]))
    .is_err());
    assert!(check_cost_only(&Json::obj([(
        "disposition",
        Json::str("inconclusive{infra:environment_died}")
    )]))
    .is_ok());
}

#[test]
fn ac1_round2_checkpoint_withholds_in_priority_order() {
    let mut i = input(Scope::Checkpoint { after_round: 2 });
    // lhs = 72.75 M at planning priors; rhs = root − 0 − 0 − 3.4 M.
    // root = 41.4 M ⇒ rhs = 38 M: V5 (18.7) and Claude Code (10.5)
    // and Pi (6.15) must drop before the protected pair (37.4) fits.
    i.root_cap_micros = 41_400_000;
    let out = run_checkpoint(&i, None).unwrap();
    assert_eq!(
        arm_order(&out.withheld),
        vec![FpArm::V5, FpArm::ClaudeCode, FpArm::Pi]
    );
    for a in &out.withheld {
        assert_eq!(a.reason, WithheldReason::Budget);
        assert_eq!(a.slots.len(), 50);
        assert_eq!(a.label, DROP_LABEL);
    }
    // The protected pair continues — never dropped while an
    // unprotected arm remained.
    assert_eq!(out.continuing[&FpArm::OrigMini].len(), 50);
    assert_eq!(out.continuing[&FpArm::HhNative].len(), 50);
    assert!(out.inference_refusals.is_empty());
    assert!(out.cost_fit.0 <= out.cost_fit.1);
    // The emitted readout is cost-only.
    check_cost_only(&out.to_json()).unwrap();
    assert_eq!(
        out.to_json().get("schema").and_then(Json::as_str),
        Some(CHECKPOINT_SCHEMA)
    );
}

#[test]
fn ac1_protected_pair_breach_withholds_everything() {
    let mut i = input(Scope::Checkpoint { after_round: 2 });
    // rhs below the pair's own projected cost (37.4 M): every
    // remaining slot is withheld{budget} and the three claims refuse.
    i.root_cap_micros = 39_000_000; // rhs = 35.6 M < 37.4 M pair cost
    let out = run_checkpoint(&i, None).unwrap();
    assert_eq!(out.withheld.len(), 5);
    assert!(out.continuing.values().all(|s| s.is_empty()));
    assert_eq!(out.inference_refusals.len(), 3);
    let claims: Vec<&str> = out.inference_refusals.iter().map(|r| r.claim).collect();
    assert_eq!(claims, vec!["P2-FP", "P3-L2", "P4-FP"]);
    assert!(out
        .inference_refusals
        .iter()
        .all(|r| r.reason.contains("protected_pair_truncated")));
}

#[test]
fn ac1_round_boundary_withholds_v5_before_hh_native() {
    // A later boundary (after round 3) with a smaller remaining set:
    // V5 drops before any HH-native slot is touched.
    let mut i = input(Scope::Checkpoint { after_round: 3 });
    i.remaining = FpArm::ALL
        .iter()
        .map(|a| (*a, slots(*a, 30, "r3")))
        .collect();
    i.root_cap_micros = 40_000_000; // rhs = 36.6 M
    let out = run_checkpoint(&i, None).unwrap();
    let v5_at = out.withheld.iter().position(|a| a.arm == FpArm::V5);
    let hh_at = out.withheld.iter().position(|a| a.arm == FpArm::HhNative);
    assert!(v5_at.is_some());
    match hh_at {
        Some(h) => assert!(v5_at.unwrap() < h),
        None => assert!(!out.continuing[&FpArm::HhNative].is_empty()),
    }
}

#[test]
fn ac1_wall_time_shortfall_withholds_in_the_same_order() {
    let mut i = input(Scope::Checkpoint { after_round: 2 });
    // The cost step must fit so the wall-time rule alone withholds:
    // root $80 leaves rhs = 76.6 M ≥ 72.75 M of projected cost.
    i.root_cap_micros = 80_000_000;
    // All-prior wall: t̂ = 1,040 s for every arm (native lane uses
    // mini's t̂ = the same prior). W_p(5 arms, C=4) = 5×55×1.04/4 h
    // in ms = 71.5 M + overhead + load = 76.9 M.
    let w = wall(62_340_000); // room = 60 000 000 (d_limit − cut-off)
    let out = run_checkpoint(&i, Some(&w)).unwrap();
    // 71.5 + 5.4 = 76.9 > 60 → drop V5 → 57.2 + 5.4 = 62.6 > 60 →
    // drop Claude Code → 42.9 + 5.4 = 48.3 ≤ 60 → stop.
    assert_eq!(arm_order(&out.withheld), vec![FpArm::V5, FpArm::ClaudeCode]);
    assert!(out
        .withheld
        .iter()
        .all(|a| a.reason == WithheldReason::WindowDeadline));
    let (wp, room) = out.wall_fit.unwrap();
    assert!(wp <= room);
}

#[test]
fn ac1_p_hat_and_t_hat_formulas() {
    let mut i = input(Scope::FpE);
    // <10 measured → the binding prior.
    assert_eq!(p_hat(FpArm::Pi, &i), 123_000);
    assert_eq!(t_hat(FpArm::Pi, &i), T_PRIOR_MS);
    // 10 measured at cost c: p̂ = ceil(1.2 × 1.5 × c) = 1.8c.
    i.measured
        .insert(FpArm::Pi, measured(FpArm::Pi, 100_000, 0, 10));
    assert_eq!(p_hat(FpArm::Pi, &i), 180_000);
    // P90 above 1.5×mean binds: eight cheap + two outliers → the
    // nearest-rank P90 is the outlier.
    let mut skew = measured(FpArm::Pi, 100_000, 0, 8);
    skew.extend(measured(FpArm::Pi, 400_000, 0, 2));
    i.measured.insert(FpArm::Pi, skew);
    // mean = 160k → 1.5×mean = 240k < P90 = 400k → p̂ = 1.2×400k.
    assert_eq!(p_hat(FpArm::Pi, &i), 480_000);
    // t̂: ≥3 measured → 1.1 × mean; native lane uses mini's.
    i.measured
        .insert(FpArm::OrigMini, measured(FpArm::OrigMini, 0, 2_000_000, 3));
    assert_eq!(t_hat(FpArm::OrigMini, &i), 2_200_000);
    assert_eq!(t_hat(FpArm::HhNative, &i), 2_200_000);
    assert_eq!(t_hat(FpArm::V5, &i), 2_200_000);
    assert_eq!(t_hat(FpArm::ClaudeCode, &i), T_PRIOR_MS);
}

// ── AC-2 (REQ-XH-117) ────────────────────────────────────────────────

#[test]
fn ac2_over_cap_arm_withheld_at_admission() {
    let mut i = input(Scope::FpE);
    let planned: BTreeMap<FpArm, i64> = FpArm::ALL.iter().map(|a| (*a, 50)).collect();
    let caps = cap_table(&planned, &i);
    // The cap table is the formula: 50 × p̂_h, unprotected only.
    assert_eq!(caps[&FpArm::Pi], 50 * 123_000);
    assert_eq!(caps[&FpArm::ClaudeCode], 50 * 210_000);
    assert_eq!(caps[&FpArm::V5], 50 * 374_000);
    assert!(!caps.contains_key(&FpArm::OrigMini));
    assert!(!caps.contains_key(&FpArm::HhNative));
    let rec = cap_record(
        &planned,
        &caps,
        &BTreeMap::from_iter(FpArm::ALL.iter().map(|a| (*a, p_hat(*a, &i)))),
    );
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(CAP_TABLE_SCHEMA)
    );
    check_cost_only(&rec).unwrap();
    // Simulate Pi spending up to its cap, then one more slot.
    let cap = caps[&FpArm::Pi];
    i.consumed_by_arm.insert(FpArm::Pi, cap - 50_000);
    i.in_flight
        .insert(FpArm::Pi, vec![("pi-f0".to_string(), 123_000)]);
    match admit_slot(FpArm::Pi, &i, &caps) {
        AdmitVerdict::Capped {
            expected_micros,
            cap_micros,
        } => {
            // expected = consumed + max(p̂, in-flight consumed) + p̂
            assert_eq!(expected_micros, cap - 50_000 + 123_000 + 123_000);
            assert_eq!(cap_micros, cap);
        }
        other => panic!("expected Capped, got {other:?}"),
    }
    // The in-flight overshoot is bounded by in-flight × (s − p̂).
    assert_eq!(
        continuation::overshoot_bound_micros(1, i.slice_micros, p_hat(FpArm::Pi, &i)),
        850_000 - 123_000
    );
    // Protected arms are bounded by the root + checkpoint only.
    assert_eq!(
        admit_slot(FpArm::OrigMini, &i, &caps),
        AdmitVerdict::ProtectedByRoot
    );
    assert_eq!(
        admit_slot(FpArm::HhNative, &i, &caps),
        AdmitVerdict::ProtectedByRoot
    );
}

#[test]
fn ac2_cheap_arm_completes_all_fifty_slots() {
    let mut i = input(Scope::FpE);
    // Every measured trial at exactly p̂/1.8 (10 measured → p̂ =
    // 1.8 × cost): Pi at 100 000 µ$/trial → p̂ = 180 000.
    i.measured
        .insert(FpArm::Pi, measured(FpArm::Pi, 100_000, 0, 10));
    let planned: BTreeMap<FpArm, i64> = BTreeMap::from([(FpArm::Pi, 50)]);
    let caps = cap_table(&planned, &i);
    assert_eq!(caps[&FpArm::Pi], 50 * 180_000);
    let mut consumed = 0i64;
    for _ in 0..50 {
        i.consumed_by_arm.insert(FpArm::Pi, consumed);
        assert_eq!(admit_slot(FpArm::Pi, &i, &caps), AdmitVerdict::Admit);
        consumed += 100_000;
    }
    assert_eq!(consumed, 5_000_000); // ≤ cap 9 000 000
}

#[test]
fn ac2_family_retains_the_unavailable_member() {
    let dropped = BTreeMap::from([(FpArm::V5, WithheldReason::Budget)]);
    let fam = first_pass_family(&dropped);
    assert_eq!(fam.len(), 3);
    let v5 = fam.iter().find(|m| m.arm == FpArm::V5).unwrap();
    assert_eq!(v5.claim, "P4-FP");
    assert!(!v5.available);
    assert_eq!(v5.withheld.as_deref(), Some("withheld{budget}"));
    assert!(fam
        .iter()
        .filter(|m| m.arm != FpArm::V5)
        .all(|m| m.available));
}

// ── AC-3 (REQ-XD-040 + REQ-XC-211's literal predicate) ───────────────

#[test]
fn ac3_over_cap_run_stops_and_reports_a_censored_bound() {
    let mut i = input(Scope::FpE);
    let planned: BTreeMap<FpArm, i64> = BTreeMap::from([(FpArm::ClaudeCode, 50)]);
    let caps = cap_table(&planned, &i); // 50 × 210 000 = 10.5 M
                                        // Trials meter at 400 000 µ$ each (above p̂ = 210 000).
    let mut consumed = 0i64;
    let mut in_flight_actual = 0i64;
    let mut stopped_at = None;
    for k in 0..50 {
        i.consumed_by_arm.insert(FpArm::ClaudeCode, consumed);
        i.in_flight.insert(
            FpArm::ClaudeCode,
            vec![(format!("cc-f{k}"), in_flight_actual)],
        );
        match admit_slot(FpArm::ClaudeCode, &i, &caps) {
            AdmitVerdict::Admit => {
                consumed += 400_000; // the settled trial meters in
                in_flight_actual = 400_000;
            }
            AdmitVerdict::Capped { .. } => {
                stopped_at = Some(k);
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let stopped = stopped_at.expect("the run must cap");
    // Overshoot beyond the cap never exceeds the in-flight bound
    // in_flight × (s − p̂): one in-flight slot × (850 000 − 210 000).
    let metered_at_stop = consumed + in_flight_actual;
    let bound =
        caps[&FpArm::ClaudeCode] + continuation::overshoot_bound_micros(1, i.slice_micros, 210_000);
    assert!(metered_at_stop <= bound, "{metered_at_stop} > {bound}");
    // The unrun slots are withheld{budget}, never scored.
    let unrun: Vec<String> = slots(FpArm::ClaudeCode, 50, "r")
        .into_iter()
        .skip(stopped)
        .collect();
    let action = WithholdAction {
        arm: FpArm::ClaudeCode,
        slots: unrun.clone(),
        reason: WithheldReason::Budget,
        label: DROP_LABEL,
    };
    assert_eq!(unrun.len(), 50 - stopped);
    assert_eq!(
        action.to_json().get("disposition").and_then(Json::as_str),
        Some("withheld{budget}")
    );
    // Its cost is a censored lower bound (REQ-XC-211's predicate).
    assert_eq!(
        arm_cost_report(metered_at_stop, true)
            .get("basis")
            .and_then(Json::as_str),
        Some("censored_lower_bound")
    );
    assert_eq!(
        arm_cost_report(metered_at_stop, false)
            .get("basis")
            .and_then(Json::as_str),
        Some("complete")
    );
}

// ── AC-4 (REQ-XH-126) ────────────────────────────────────────────────

#[test]
fn ac4_fp_e_wall_time_drops_in_priority_order() {
    let mut i = input(Scope::FpE);
    // W2 measurement: original mini at 2,000 s/trial → t̂ = 2,200 s
    // for mini, hh_native and v5; Pi and Claude Code stay at the
    // 1,040 s prior (<3 measured).
    i.measured
        .insert(FpArm::OrigMini, measured(FpArm::OrigMini, 0, 2_000_000, 3));
    // The cost step fits (rhs = 76.6 M ≥ 72.75 M projected) so the
    // wall-time rule alone withholds.
    i.root_cap_micros = 80_000_000;
    // W_p(5) = (3×55×2.2M + 2×55×1.04M)/4 + 5.4M = 124.75M ms.
    // room 70 M ⇒ drops V5 (94.5M) → Claude Code (80.2M) → Pi
    // (65.9M) before the pair (65.9 ≤ 70) fits.
    let w = wall(72_340_000); // d_limit − cutoff = 70 000 000
    let out = run_checkpoint(&i, Some(&w)).unwrap();
    assert_eq!(
        arm_order(&out.withheld),
        vec![FpArm::V5, FpArm::ClaudeCode, FpArm::Pi]
    );
    assert!(out
        .withheld
        .iter()
        .all(|a| a.reason == WithheldReason::WindowDeadline));
    assert!(!out.continuing[&FpArm::OrigMini].is_empty());
    assert!(!out.continuing[&FpArm::HhNative].is_empty());
    let (wp, room) = out.wall_fit.unwrap();
    assert_eq!(wp, 65_900_000);
    assert_eq!(room, 70_000_000);
}

#[test]
fn ac4_round_boundary_wall_shortfall_same_order() {
    let mut i = input(Scope::Checkpoint { after_round: 4 });
    i.measured
        .insert(FpArm::OrigMini, measured(FpArm::OrigMini, 0, 2_000_000, 4));
    // 30 slots remain per arm: W_p = (3×33×2.2 + 2×33×1.04)M/4 + 5.4M
    // = (217.8 + 68.64)/4 + 5.4 = 77.01M ms. room 40 M ⇒ V5, CC, Pi
    // drop (pair 36.3 + 5.4 = 41.7 > 40 → protected breach).
    i.remaining = FpArm::ALL
        .iter()
        .map(|a| (*a, slots(*a, 30, "r4")))
        .collect();
    let w = wall(42_340_000); // room = 40 M
    let out = run_checkpoint(&i, Some(&w)).unwrap();
    assert_eq!(
        arm_order(&out.withheld),
        vec![
            FpArm::V5,
            FpArm::ClaudeCode,
            FpArm::Pi,
            FpArm::OrigMini,
            FpArm::HhNative
        ]
    );
    assert!(out
        .withheld
        .iter()
        .all(|a| a.reason == WithheldReason::WindowDeadline));
    assert_eq!(out.inference_refusals.len(), 3);
}

#[test]
fn ac4_pair_atomic_window_deadline() {
    let pair = OhPair {
        o_slot: "pair7-o".to_string(),
        h_slot: "pair7-h".to_string(),
    };
    let ds = withhold_pair_deadline(&pair);
    assert_eq!(ds.len(), 2);
    for (slot, d) in &ds {
        assert_eq!(
            *d,
            Disposition::Withheld {
                reason: WithheldReason::WindowDeadline
            },
            "{slot}"
        );
    }
    // A withheld set covering one side refuses pair closure.
    let half: BTreeMap<String, ()> = BTreeMap::from([("pair7-o".to_string(), ())]);
    assert!(continuation::check_pair_closure(&half, std::slice::from_ref(&pair)).is_err());
    let whole: BTreeMap<String, ()> =
        BTreeMap::from([("pair7-o".to_string(), ()), ("pair7-h".to_string(), ())]);
    assert!(continuation::check_pair_closure(&whole, &[pair]).is_ok());
}

#[test]
fn ac4_d_max_releases_wc_only_when_unauthorized() {
    // Completed windows at billed-where-reconciled else ceiling;
    // W4's fixed terms = r·restart + disk + egress.
    let completed = vec![
        WindowSum {
            window: "w1".to_string(),
            ceiling_micros: 5_050_000,
            billed_micros: Some(4_900_000), // reconciled → billed
        },
        WindowSum {
            window: "w2".to_string(),
            ceiling_micros: 2_410_000,
            billed_micros: None, // ceiling
        },
        WindowSum {
            window: "w3".to_string(),
            ceiling_micros: 2_410_000,
            billed_micros: None,
        },
    ];
    let w4 = W4Ceiling {
        hourly_micros: 314_140,
        fixed_micros: 1_000_000,
    };
    // Overflow reservation ≈ $0.56: r×1h + S·r_disk·h_r.
    let reservation = overflow_reservation_micros(314_140, 137, 300, 6);
    assert_eq!(reservation, 314_140 + 300 * 137 * 6);
    let unauthorized = WcTerm {
        authorized: false,
        ceiling_micros: 1_940_000,
        reservation_micros: reservation,
    };
    let d_unauth = d_max_ms(&completed, &w4, &unauthorized);
    // Σ others = 4.9 + 2.41 + 2.41 + 0.56074 = 10.28074 M; headroom =
    // 20 − 10.28074 − 1 = 8.71926 M µ$ → D = headroom/r hours.
    let expect =
        (FP_COMPUTE_CAP_MICROS - (4_900_000 + 2_410_000 + 2_410_000 + reservation) - 1_000_000)
            * 3_600_000
            / 314_140;
    assert_eq!(d_unauth, expect);
    // Authorized Wc counts its full ceiling — a strictly smaller D.
    let authorized = WcTerm {
        authorized: true,
        ..unauthorized.clone()
    };
    let d_auth = d_max_ms(&completed, &w4, &authorized);
    let expect_auth = (FP_COMPUTE_CAP_MICROS
        - (4_900_000 + 2_410_000 + 2_410_000 + authorized.ceiling_micros)
        - 1_000_000)
        * 3_600_000
        / 314_140;
    assert_eq!(d_auth, expect_auth);
    assert!(d_auth < d_unauth);
    // No headroom → D_max 0 → the fit test refuses, never negative.
    let mut tight = completed.clone();
    tight.push(WindowSum {
        window: "wx".to_string(),
        ceiling_micros: 30_000_000,
        billed_micros: None,
    });
    assert_eq!(d_max_ms(&tight, &w4, &unauthorized), 0);
}

#[test]
fn ac4_w_p_carries_retry_allowance_and_load() {
    let i = input(Scope::FpE);
    // 50 remaining per arm at the wall prior: (50 + ⌈0.1×50⌉) × t̂.
    let rem: BTreeMap<FpArm, Vec<String>> =
        BTreeMap::from([(FpArm::Pi, slots(FpArm::Pi, 50, "w"))]);
    let w = wall(0);
    let wp = w_p_ms(&rem, &i, &w);
    // (50 + 5) × 1 040 000 / 4 + 4.5M + 0.9M = 14.3M + 5.4M.
    assert_eq!(wp, 55 * 1_040_000 / 4 + 5_400_000);
}

// ── AC-5 (REQ-XD-042) ────────────────────────────────────────────────

#[test]
fn ac5_the_four_stop_rules_and_no_outcome_trigger() {
    // Budget: the root cap.
    let b = evaluate_stop(&StopTrigger::Budget {
        consumed_micros: 56_000_000,
        reserved_micros: 850_000,
        root_cap_micros: 56_000_000,
    })
    .unwrap();
    assert_eq!(b.scope, "pass");
    assert_eq!(b.trigger, "budget");
    assert!(b.partial_rows_retained);
    assert!(evaluate_stop(&StopTrigger::Budget {
        consumed_micros: 1_000,
        reserved_micros: 0,
        root_cap_micros: 56_000_000,
    })
    .is_none());
    // Infra: the PP0 leg (≥ 1 of 3) holds the arm; the window leg
    // (>20 % after ≥ 10) stops it; ≥ 3 stopped arms stops the pass.
    let pp0 = evaluate_stop(&StopTrigger::Infra {
        stage: "pp0".to_string(),
        trials: 3,
        infra_failed: 1,
        stopped_arms: 0,
        arm: FpArm::Pi,
    })
    .unwrap();
    assert_eq!(pp0.scope, "pi");
    assert_eq!(
        pp0.withheld,
        Some(WithheldReason::Admission(
            hharness_runner::disposition::AdmissionReason::Pp0InfraHold
        ))
    );
    assert!(evaluate_stop(&StopTrigger::Infra {
        stage: "window".to_string(),
        trials: 10,
        infra_failed: 3, // > 20 %
        stopped_arms: 0,
        arm: FpArm::Pi,
    })
    .is_some());
    assert!(evaluate_stop(&StopTrigger::Infra {
        stage: "window".to_string(),
        trials: 10,
        infra_failed: 2, // = 20 % — not over
        stopped_arms: 0,
        arm: FpArm::Pi,
    })
    .is_none());
    assert!(evaluate_stop(&StopTrigger::Infra {
        stage: "window".to_string(),
        trials: 9,
        infra_failed: 9,
        stopped_arms: 0,
        arm: FpArm::Pi,
    })
    .is_none());
    let three = evaluate_stop(&StopTrigger::Infra {
        stage: "window".to_string(),
        trials: 0,
        infra_failed: 0,
        stopped_arms: 3,
        arm: FpArm::Pi,
    })
    .unwrap();
    assert_eq!(three.scope, "pass");
    // Integrity: every event stops everything at once.
    for event in [
        continuation::IntegrityEvent::UnreceiptedCall,
        continuation::IntegrityEvent::EgressOutsideAllowlist,
        continuation::IntegrityEvent::CredentialExposure,
        continuation::IntegrityEvent::ImageDigestMismatch,
        continuation::IntegrityEvent::CatalogIdentityChange,
        continuation::IntegrityEvent::AnswerKeyMultiTrial,
    ] {
        let d = evaluate_stop(&StopTrigger::Integrity { event }).unwrap();
        assert_eq!(d.scope, "pass");
        assert!(d.partial_rows_retained);
    }
    // Calendar: the credit period end, unless the operator overrode.
    assert!(evaluate_stop(&StopTrigger::Calendar {
        credit_period_end_ms: 1_000,
        now_ms: 1_001,
        operator_overrode: false,
    })
    .is_some());
    assert!(evaluate_stop(&StopTrigger::Calendar {
        credit_period_end_ms: 1_000,
        now_ms: 1_001,
        operator_overrode: true,
    })
    .is_none());
    // No outcome-based trigger exists to configure.
    assert!(matches!(
        stop_trigger_from_json(&Json::obj([("kind", Json::str("outcome"))])),
        Err(ContinuationError::UnsupportedTrigger { .. })
    ));
    assert!(matches!(
        stop_trigger_from_json(&Json::obj([
            ("kind", Json::str("budget")),
            ("consumed_micros", Json::Int(1)),
            ("reserved_micros", Json::Int(1)),
            ("root_cap_micros", Json::Int(2)),
            ("verdict", Json::str("resolved")),
        ])),
        Err(ContinuationError::OutcomeField { .. })
    ));
    // The strict decode serves the four kinds.
    assert_eq!(
        stop_trigger_from_json(&Json::obj([
            ("kind", Json::str("calendar")),
            ("credit_period_end_ms", Json::Int(1)),
            ("now_ms", Json::Int(2)),
        ])),
        Ok(StopTrigger::Calendar {
            credit_period_end_ms: 1,
            now_ms: 2,
            operator_overrode: false,
        })
    );
}

#[test]
fn ac1_checkpoint_readout_carries_p99_cost_fields() {
    let mut i = input(Scope::Checkpoint { after_round: 2 });
    i.measured
        .insert(FpArm::Pi, measured(FpArm::Pi, 100_000, 0, 10));
    i.client_timeouts_ms =
        BTreeMap::from([("queue".to_string(), 120), ("wide".to_string(), 1_000)]);
    let out: CheckpointOutcome = run_checkpoint(&i, None).unwrap();
    // 10 samples of queue_wait+latency = 100 → p99 = 100.
    let q = out.p99.iter().find(|r| r.timeout == "queue").unwrap();
    assert_eq!(q.p99_ms, Some(100));
    assert_eq!(q.within, Some(true));
    let j = out.to_json();
    assert!(j.get("p99").and_then(Json::as_arr).is_some());
    check_cost_only(&j).unwrap();
}
