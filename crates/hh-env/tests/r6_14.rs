//! R6.14 — evaluate_admitted_costs leg truth table (AC-4.5-2/REQ-XC-117).
use hharness_env::admitted_costs::*;
use hharness_env::preflight::LegVerdict;

fn pkt(
    exp: i64,
    adm: Option<&str>,
    cap: Option<i64>,
    st: SettlementStatus,
    cl: ComputeLeg,
) -> PacketCost {
    PacketCost {
        packet: "p".into(),
        exposure_micros: exp,
        admission_ref: adm.map(str::to_string),
        cap_micros: cap,
        settlement: st,
        compute: cl,
    }
}
fn facts(ps: Vec<PacketCost>) -> AdmittedCostFacts {
    AdmittedCostFacts {
        per_packet: ps,
        admissions: vec![Admission {
            ref_: "gate-fps".into(),
            cap_micros: 60_000_000,
            phase: "p1".into(),
        }],
        program_cap_micros: 100_000_000,
    }
}
#[test]
fn leg_truth_table() {
    let v = LegVerdict::Verified;
    // exposure 0 → ok; pre-schema compute folds unrecorded, never $0
    assert_eq!(
        evaluate_admitted_costs(&facts(vec![pkt(
            0,
            None,
            None,
            SettlementStatus::None,
            ComputeLeg::UnrecordedPreSchema
        )])),
        v
    );
    // admitted + within cap + settled → ok
    assert_eq!(
        evaluate_admitted_costs(&facts(vec![pkt(
            1_500_000,
            Some("gate-fps"),
            None,
            SettlementStatus::Settled,
            ComputeLeg::UnrecordedPreSchema
        )])),
        v
    );
    // unadmitted / over cap refuse
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(1, None, None, SettlementStatus::None, ComputeLeg::UnrecordedPreSchema)])),
        LegVerdict::Refused(r) if r.starts_with("unadmitted_spend")));
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(70_000_000, Some("gate-fps"), None, SettlementStatus::Settled,
            ComputeLeg::UnrecordedPreSchema)])),
        LegVerdict::Refused(r) if r.starts_with("over_cap")));
    // settlement pending → inconclusive; discrepant refuses unless accepted
    assert!(matches!(
        evaluate_admitted_costs(&facts(vec![pkt(
            1,
            Some("gate-fps"),
            None,
            SettlementStatus::Pending,
            ComputeLeg::UnrecordedPreSchema
        )])),
        LegVerdict::Inconclusive(_)
    ));
    assert!(matches!(
        evaluate_admitted_costs(&facts(vec![pkt(
            1,
            Some("gate-fps"),
            None,
            SettlementStatus::SettledDiscrepant { accepted_ref: None },
            ComputeLeg::UnrecordedPreSchema
        )])),
        LegVerdict::Refused(_)
    ));
    assert_eq!(
        evaluate_admitted_costs(&facts(vec![pkt(
            1,
            Some("gate-fps"),
            None,
            SettlementStatus::SettledDiscrepant {
                accepted_ref: Some("ops-note".into())
            },
            ComputeLeg::UnrecordedPreSchema
        )])),
        v
    );
    // compute leg predicates
    let win = |rr: Option<&str>, empty: bool, est: i64, ceil: i64, st: &str| ComputeWindow {
        window: "w2x".into(),
        record_ref: rr.map(str::to_string),
        post_state_empty: empty,
        estimate_micros: est,
        ceiling_micros: ceil,
        billed_status: st.into(),
    };
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(0, None, None, SettlementStatus::None,
            ComputeLeg::Recorded(vec![win(None, true, 1, 2, "pending")]))])),
        LegVerdict::Refused(r) if r.starts_with("compute_record_missing")));
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(0, None, None, SettlementStatus::None,
            ComputeLeg::Recorded(vec![win(Some("cc-1"), false, 1, 2, "pending")]))])),
        LegVerdict::Refused(r) if r.starts_with("post_state_nonempty")));
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(0, None, None, SettlementStatus::None,
            ComputeLeg::Recorded(vec![win(Some("cc-1"), true, 3, 2, "pending")]))])),
        LegVerdict::Refused(r) if r.starts_with("compute_over_authorization")));
    assert!(matches!(evaluate_admitted_costs(&facts(vec![
        pkt(0, None, None, SettlementStatus::None,
            ComputeLeg::Recorded(vec![win(Some("cc-1"), true, 1, 2, "pending")]))])),
        LegVerdict::Inconclusive(r) if r.starts_with("billing_pending")));
    assert_eq!(
        evaluate_admitted_costs(&facts(vec![pkt(
            0,
            None,
            None,
            SettlementStatus::None,
            ComputeLeg::Recorded(vec![win(Some("cc-1"), true, 1, 2, "within")])
        )])),
        v
    );
}
