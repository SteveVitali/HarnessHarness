//! `hh-bundle` S4.4 tests — `ReproReport{independent}` (§5h.3 §2's
//! record; R-2.9.3 C1/Stage-4 "`ReproReport` sidecar with
//! `independent`"): the spec's four-valued `verdict`, the declared
//! `reproducer`/`reproducer_instrument` members, and
//! `compute_independence`'s fail-closed distinctness rule (OQ-334
//! interim rule, ADR-0295). Every assertion fails if the members or the
//! computation are removed.

use hh_bundle::manifest::ReproLevel;
use hh_bundle::repro::{compute_independence, ReproOutcome, ReproReport};
use hh_wire::json::Json;

/// A declared `ProvenanceRecord` JSON — `signer` lands as an attestation
/// anchor (the one scheme a signing claim takes; CC1).
fn prov(signer: Option<&str>) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("origin".into(), Json::str("kernel:test"));
    if let Some(s) = signer {
        m.insert(
            "attestation".into(),
            Json::obj([
                ("kind", Json::str("verified")),
                ("subject_hash", Json::str("sha256:x")),
                ("anchor", Json::obj([("signer", Json::str(s))])),
                ("verified_by", Json::str("kernel:test")),
                ("verified_at", Json::Int(1)),
            ]),
        );
    }
    Json::Obj(m)
}

fn inst(installation_id: Option<&str>) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("version_id".into(), Json::str("idp:kernel-1"));
    if let Some(i) = installation_id {
        m.insert("installation_id".into(), Json::str(i));
    }
    Json::Obj(m)
}

#[test]
fn verdict_member_uses_the_spec_vocabulary() {
    let mk = |outcome, refusal: Option<&str>| {
        let mut r = ReproReport::refused("idp:b", ReproLevel::R0, "x");
        r.outcome = outcome;
        r.refusal = refusal.map(str::to_string);
        r.to_json()
    };
    // pass → reproduced; drift → not_reproduced;
    // nondeterministic|inconclusive → inconclusive; refused → n/a{reason}.
    assert_eq!(
        mk(ReproOutcome::Pass, None)
            .get("verdict")
            .and_then(Json::as_str),
        Some("reproduced")
    );
    assert_eq!(
        mk(ReproOutcome::Drift, None)
            .get("verdict")
            .and_then(Json::as_str),
        Some("not_reproduced")
    );
    assert_eq!(
        mk(ReproOutcome::Nondeterministic, None)
            .get("verdict")
            .and_then(Json::as_str),
        Some("inconclusive")
    );
    assert_eq!(
        mk(ReproOutcome::Inconclusive, None)
            .get("verdict")
            .and_then(Json::as_str),
        Some("inconclusive")
    );
    let na = mk(ReproOutcome::Refused, Some("EnvironmentUnavailable"));
    let v = na.get("verdict").unwrap();
    assert_eq!(v.get("kind").and_then(Json::as_str), Some("na"));
    assert_eq!(
        v.get("reason").and_then(Json::as_str),
        Some("EnvironmentUnavailable")
    );
}

#[test]
fn the_report_carries_reproducer_instrument_and_independent() {
    let mut r = ReproReport::refused("idp:b", ReproLevel::R0, "x");
    r.reproducer = prov(Some("lab-b"));
    r.reproducer_instrument = inst(Some("box-9"));
    r.independent = true;
    let j = r.to_json();
    assert_eq!(
        j.get("reproducer")
            .and_then(|p| p.get("attestation"))
            .and_then(|a| a.get("anchor"))
            .and_then(|a| a.get("signer"))
            .and_then(Json::as_str),
        Some("lab-b"),
        "{j:?}"
    );
    assert_eq!(
        j.get("reproducer_instrument")
            .and_then(|i| i.get("installation_id"))
            .and_then(Json::as_str),
        Some("box-9")
    );
    assert_eq!(j.get("independent"), Some(&Json::Bool(true)));
    // `report_id` covers the new members — resealing the mutated
    // report changes the id (the constructor sealed the pre-mutation
    // state) and sealing twice is stable.
    let mut r2 = r.clone();
    r2.seal();
    let id2 = r2.report_id.clone();
    assert_ne!(
        Some(id2.as_str()),
        j.get("report_id").and_then(Json::as_str),
        "the members must be inside the report's identity"
    );
    r2.seal();
    assert_eq!(
        r2.to_json().get("report_id").and_then(Json::as_str),
        Some(id2.as_str()),
        "sealing is deterministic over the same members"
    );
}

#[test]
fn independence_is_computed_fail_closed() {
    let prod_p = prov(Some("lab-a"));
    let prod_i = inst(Some("box-1"));

    // The positive arm — BOTH axes distinct.
    let (ind, basis) =
        compute_independence(&prod_p, &prod_i, &prov(Some("lab-b")), &inst(Some("box-2")));
    assert!(ind, "distinct signer+installation must be independent");
    assert_eq!(basis, "distinct_signer_and_installation");

    // Every failure arm computes false with the named basis — the flag
    // is never claimed by the caller.
    let cases: Vec<(Json, Json, &str)> = vec![
        (prov(Some("lab-a")), inst(Some("box-2")), "same_signer"),
        (
            prov(Some("lab-b")),
            inst(Some("box-1")),
            "same_installation",
        ),
        (
            prov(None),
            inst(Some("box-2")),
            "reproducer_signer_undeclared",
        ),
        (
            prov(Some("lab-b")),
            inst(None),
            "reproducer_installation_undeclared",
        ),
    ];
    for (rep_p, rep_i, want) in cases {
        let (ind, basis) = compute_independence(&prod_p, &prod_i, &rep_p, &rep_i);
        assert!(!ind, "{basis} must not be independent");
        assert_eq!(basis, want);
    }

    // Producer-side undeclared material fails closed too.
    let (ind, basis) = compute_independence(
        &prov(None),
        &prod_i,
        &prov(Some("lab-b")),
        &inst(Some("box-2")),
    );
    assert!(!ind);
    assert_eq!(basis, "producer_signer_undeclared");
    let (ind, basis) = compute_independence(
        &prod_p,
        &inst(None),
        &prov(Some("lab-b")),
        &inst(Some("box-2")),
    );
    assert!(!ind);
    assert_eq!(basis, "producer_installation_undeclared");
}
