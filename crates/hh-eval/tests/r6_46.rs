//! R6.46 (manifest 310) — K7 L2 wiring: participant equality
//! (REQ-XD-112), typed unknowns + no CRN/seed coupling (REQ-XD-113),
//! the pre-data infra envelope (REQ-XD-114), the dimension map
//! (REQ-XD-116), the veto layer (REQ-XD-117), operational stops
//! (REQ-XD-118), the G-L1 precondition (REQ-XD-102) and the
//! REQ-XD-101 parity-claim fold.

use std::collections::BTreeMap;

use hharness_eval::directional_kl::ConjunctionOutcome;
use hharness_eval::l2_wire::*;
use hharness_eval::vetoes::VetoTrip;
use hharness_wire::Json;

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn ac7_xd112_unequal_pairs_become_uncertain() {
    let h = fields(&[
        ("model_ref", "m1"),
        ("provider_model_id", "glm-5.3-flash"),
        ("effort", "high"),
        ("task", "t1"),
        ("repeat", "0"),
        ("pair_id", "p1"),
        ("policy_set", "A,B"),
        ("seed_record_ref", "s1"),
        ("environment_ref", "e1"),
    ]);
    let o_same = h.clone();
    assert!(participant_equal(&h, &o_same).is_empty());
    let mut o_diff = h.clone();
    o_diff.insert("effort".into(), "low".into());
    assert_eq!(participant_equal(&h, &o_diff), vec!["effort".to_string()]);
}

#[test]
fn ac9_typed_unknowns_and_no_coupling() {
    let u = typed_unknowns();
    let kinds: Vec<&str> = match &u {
        Json::Arr(a) => a
            .iter()
            .filter_map(|e| e.get("kind").and_then(Json::as_str))
            .collect(),
        _ => panic!("not an array"),
    };
    assert_eq!(
        kinds,
        [
            "served_weights",
            "cache_state",
            "seeds",
            "determinism",
            "support_gate"
        ]
    );
    assert!(check_no_coupling_claim(Some("independent_pairs")).is_ok());
    assert!(check_no_coupling_claim(None).is_ok());
    assert!(matches!(
        check_no_coupling_claim(Some("seed_coupling")),
        Err(L2Err::CouplingClaim { .. })
    ));
}

#[test]
fn ac10_infra_envelope_uncertain_never_reattempt() {
    let env = InfraEnvelope::from_json(&Json::obj([
        (
            "classes",
            Json::Arr(vec![
                Json::str("infra:upstream_error"),
                Json::str("infra:timeout"),
            ]),
        ),
        ("declared_before_data_seq", Json::Int(7)),
    ]))
    .unwrap();
    assert_eq!(env.pair_status(None).unwrap(), PairStatus::Certain);
    assert_eq!(
        env.pair_status(Some("infra:upstream_error")).unwrap(),
        PairStatus::Uncertain {
            cause: "infra:upstream_error".into()
        }
    );
    // A class outside the pre-data envelope refuses — the envelope
    // was fixed before data; the pair still leaves the analysis set
    // (the caller marks it uncertain on this path too).
    assert!(env.pair_status(Some("infra:undeclared")).is_err());
}

#[test]
fn ac11_stop_marks_remaining_uncertain_n_unchanged() {
    assert_eq!(evaluate_stop(None).unwrap(), StopVerdict::Continue);
    match evaluate_stop(Some("infra_threshold")).unwrap() {
        StopVerdict::Stopped { trigger } => assert_eq!(trigger, "infra_threshold"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        evaluate_stop(Some("outcome_regressed")),
        Err(L2Err::OutcomeStop { .. })
    ));
    let p = stopped_packet(&["p3".into(), "p4".into()], 20);
    assert_eq!(p.get("planned_n").and_then(Json::as_int), Some(20));
    assert_eq!(p.get("planned_n_unchanged"), Some(&Json::Bool(true)));
    let n_unc = match p.get("uncertain_pairs") {
        Some(Json::Arr(a)) => a.len(),
        _ => panic!(),
    };
    assert_eq!(n_unc, 2);
}

#[test]
fn ac12_claim_fold_conjunction() {
    let conj: ConjunctionOutcome = (
        "claim-ref".into(),
        "equivalent".into(),
        vec![("capability".into(), "equivalent".into())],
        vec![],
    );
    // All true → admitted; the label carries ε and N.
    let c = parity_claim(true, true, Some(&conj), &[], 25_000, 150).unwrap();
    assert_eq!(c.get("verdict").and_then(Json::as_str), Some("admitted"));
    let label = c.get("label").and_then(Json::as_str).unwrap();
    assert!(label.contains("25000") && label.contains("150"));
    for banned in BANNED_LABEL_PHRASES {
        assert!(!label.to_lowercase().contains(banned));
    }
    // Each false conjunct withholds.
    for (l1, adm, conj, vetoed) in [
        (false, true, Some(&conj), false),
        (true, false, Some(&conj), false),
        (true, true, None, false),
        (true, true, Some(&conj), true),
    ] {
        let vetoes: Vec<VetoTrip> = if vetoed {
            vec![VetoTrip {
                veto_id: "evidence_tampered".into(),
                evidence: Json::Null,
            }]
        } else {
            vec![]
        };
        let c = parity_claim(l1, adm, conj, &vetoes, 25_000, 150).unwrap();
        assert_eq!(
            c.get("verdict").and_then(Json::as_str),
            Some("withheld{incomplete_conjunction}"),
            "l1={l1} adm={adm} vetoed={vetoed}"
        );
    }
    // An inconclusive conjunction withholds too.
    let incon: ConjunctionOutcome = ("claim-ref".into(), "inconclusive".into(), vec![], vec![]);
    let c = parity_claim(true, true, Some(&incon), &[], 25_000, 150).unwrap();
    assert_eq!(
        c.get("verdict").and_then(Json::as_str),
        Some("withheld{incomplete_conjunction}")
    );
}

#[test]
fn ac12_xd102_no_pair_before_gl1_pass() {
    assert!(matches!(
        l2_packet(None, "sha256:abc"),
        Err(L2Err::MissingGl1Pass)
    ));
    let p = l2_packet(
        Some("l1-pass:sha256:80bfafee35ac9dd669d80f786ee54deca9948049bcd8559dd7a6dee8d06dd8ef"),
        "sha256:c451ed97",
    )
    .unwrap();
    assert_eq!(p.get("schema").and_then(Json::as_str), Some(L2_SCHEMA));
    // The packet carries REQ-XD-113's unknowns and REQ-XD-116's map.
    assert!(p.get("unknowns").is_some());
    let map = p.get("dimension_map").unwrap();
    match map {
        Json::Arr(entries) => {
            assert_eq!(entries.len(), 7);
            let req: Vec<&str> = entries
                .iter()
                .filter(|e| e.get("status").and_then(Json::as_str) == Some("required"))
                .filter_map(|e| e.get("dimension").and_then(Json::as_str))
                .collect();
            assert_eq!(req, ["capability", "reliability"]);
        }
        _ => panic!("dimension_map not an array"),
    }
}
