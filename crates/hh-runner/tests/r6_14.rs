//! R6.14 — hh-slot-costs/1 fold from a fixture phase proxy run (AC-4.5-3/REQ-XC-118).
use hharness_runner::slot_costs::*;
use hharness_wire::json::Json;

fn ev(c: &str, p: Json) -> (String, Json) {
    (c.to_string(), p)
}
fn ctx() -> SlotCostCtx {
    SlotCostCtx {
        run_id: "run-1".into(),
        slice_budget_id: "bud-slot-1".into(),
        provider: "ollama-cloud".into(),
        lane_ref: "lane-1".into(),
        model_id: "glm-5.3-flash".into(),
        effort_applied: "high".into(),
        settlement: Some(("settled".into(), "sp-recon-1".into())),
        compute: Some(ComputeRef {
            ref_: "cc-abc".into(),
            attributed_ms: 3600,
            provenance: "estimate".into(),
        }),
        currency: "USD".into(),
        compute_micros: 500_000,
        compute_currency: "USD".into(),
    }
}
#[test]
fn fold_reproduces_model_and_total() {
    let events = vec![
        ev(
            "control.budget.reserved",
            Json::obj([
                ("budget_id", Json::str("bud-slot-1")),
                ("holder", Json::str("mc:m1")),
                ("quantity", Json::obj([("spend", Json::Int(900_000))])),
                ("outcome", Json::str("held")),
            ]),
        ),
        ev(
            "model.call.requested",
            Json::obj([("model_call_id", Json::str("m1"))]),
        ),
        ev(
            "model.call.completed",
            Json::obj([
                ("model_call_id", Json::str("m1")),
                ("disposition", Json::str("forwarded")),
                (
                    "usage",
                    Json::obj([
                        ("input_total", Json::Int(2000)),
                        ("input_uncached", Json::Int(1800)),
                        ("cache_read", Json::Int(200)),
                        ("output_total", Json::Int(500)),
                        ("output_reasoning", Json::Int(50)),
                    ]),
                ),
            ]),
        ),
        ev(
            "measurement.cost.attributed",
            Json::obj([
                ("money", Json::obj([("micro_units", Json::Int(520))])),
                ("provenance", Json::str("estimated_from_pricing")),
                ("confidence", Json::str("exact")),
                ("coverage", Json::Int(1_000_000)),
                (
                    "attribution",
                    Json::obj([("participant_ref", Json::str("slot-1"))]),
                ),
            ]),
        ),
        ev(
            "model.call.failed",
            Json::obj([
                ("model_call_id", Json::str("m1")),
                ("disposition", Json::str("refused_budget")),
            ]),
        ),
    ];
    let doc = fold_slot_costs(&events, "slot-1", &ctx());
    validate_slot_costs(&doc).unwrap();
    let m = doc.get("model").unwrap();
    assert_eq!(
        m.get("calls").unwrap().get("dispatched").unwrap().as_int(),
        Some(1)
    );
    assert_eq!(
        m.get("calls").unwrap().get("completed").unwrap().as_int(),
        Some(1)
    );
    assert_eq!(
        m.get("calls")
            .unwrap()
            .get("proxy_refused")
            .unwrap()
            .as_int(),
        Some(1)
    );
    assert_eq!(
        m.get("tokens")
            .unwrap()
            .get("input_total")
            .unwrap()
            .as_int(),
        Some(2000)
    );
    let s = m.get("spend").unwrap();
    assert_eq!(
        s.get("money").unwrap().get("micro_units").unwrap().as_int(),
        Some(520)
    );
    assert_eq!(
        s.get("provenance").unwrap().as_str(),
        Some("estimated_from_pricing")
    );
    assert_eq!(
        m.get("reservation")
            .unwrap()
            .get("peak_reserved_micros")
            .unwrap()
            .as_int(),
        Some(900_000)
    );
    assert_eq!(
        m.get("settlement").unwrap().get("status").unwrap().as_str(),
        Some("settled")
    );
    assert_eq!(
        doc.get("compute").unwrap().get("ref").unwrap().as_str(),
        Some("cc-abc")
    );
    let t = doc
        .get("total")
        .expect("total present when currencies match");
    assert_eq!(
        t.get("money").unwrap().get("micro_units").unwrap().as_int(),
        Some(500_520)
    );
    // currency mismatch → no total member
    let mut c2 = ctx();
    c2.compute_currency = "EUR".into();
    let doc2 = fold_slot_costs(&events, "slot-1", &c2);
    assert!(doc2.get("total").is_none());
    validate_slot_costs(&doc2).unwrap();
}
