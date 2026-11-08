// r6_20.rs — R6.20 (F2-c (a); manifest 273): the site fixture packet is
// a `hh-comparative-results/1` document (C-33, REQ-XF-123). The site
// consumes it read-only; this battery proves the committed fixture
// validates, carries its declared-fixture marker (the seal, X-07), and
// holds the acceptance shape: a 4-member observed-leader group, one
// Holm-separated harness and the three populations (REQ-XF-111/115).

use hharness_eval::comparative_estimators as ce;
use hharness_wire::json::{parse, Json};

fn fixture() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/site/fixtures/phase2-fp-results.json"
    );
    let text = std::fs::read_to_string(path).expect("fixture readable");
    parse(&text).expect("fixture parses")
}

#[test]
fn fixture_validates_as_results_packet() {
    let j = fixture();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some(ce::RESULTS_SCHEMA)
    );
    assert_eq!(
        ce::validate_results_packet(&j),
        Ok(()),
        "the committed site fixture must validate under \
         hh-comparative-results/1"
    );
}

#[test]
fn fixture_carries_the_seal_and_acceptance_shape() {
    let j = fixture();
    // The seal: the packet declares itself a presentation fixture.
    assert_eq!(j.get("fixture"), Some(&Json::Bool(true)));
    // Acceptance shape: 4-member observed-leader group, exactly one
    // Holm-separated harness, the three required populations.
    let hs = match j.get("harnesses") {
        Some(Json::Obj(hs)) => hs,
        _ => panic!("harnesses object required"),
    };
    let is_true = |v: Option<&Json>| matches!(v, Some(Json::Bool(true)));
    let members = hs
        .iter()
        .filter(|(_, h)| is_true(h.get("leader_group").and_then(|g| g.get("member"))))
        .count();
    assert_eq!(members, 4, "observed-leader group is 4-member");
    let separated = hs
        .iter()
        .filter(|(_, h)| is_true(h.get("delta_vs_ref").and_then(|d| d.get("separated"))))
        .count();
    assert_eq!(separated, 1, "exactly one Holm-separated harness");
    let pops: Vec<&str> = match j.get("populations") {
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).collect(),
        _ => panic!("populations array required"),
    };
    for want in ce::REQUIRED_POPULATIONS {
        assert!(pops.contains(want), "missing population {want}");
    }
}
