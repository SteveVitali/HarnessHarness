//! R5.42 — per-stage readiness and full exposure quote (REQ-D-105/
//! 110/309, REQ-C-004/105/106; AC-1 positive construction, AC-2
//! typed negatives). Fails-without: `eval::exposure_quote` does not
//! exist at the R5.41 base — this file compiles red there.
//! Fixture-only: every input is a supplied external stand-in, never
//! a chosen actual pin/price/margin; USD stays $0 unless the fixture
//! declares a currency.

use std::collections::BTreeMap;

use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, EnvelopeQty, ExposureQuote, InputState, PrereqStatus,
    Prerequisite, QuoteDecl, QuoteError, ResourceLine, StageDecl, COMPONENTS, ENVELOPE_COMPONENTS,
    PINS, PREREQUISITES,
};
use hharness_wire::Json;

fn obj(j: &Json) -> &BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected object: {j:?}"),
    }
}
fn arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("expected array: {j:?}"),
    }
}

fn pins(state: InputState) -> Vec<BindingInput> {
    PINS.iter()
        .map(|name| BindingInput {
            name: name.to_string(),
            state: state.clone(),
        })
        .collect()
}

fn prereqs(status: PrereqStatus) -> Vec<Prerequisite> {
    PREREQUISITES
        .iter()
        .map(|name| Prerequisite {
            name: name.to_string(),
            status: status.clone(),
        })
        .collect()
}

/// Every worksheet component, priced (qty 0 priced is still quoted).
/// `provider_unknown_exposure` is bounded at an explicit zero.
fn worksheet(price: i64) -> Vec<ResourceLine> {
    COMPONENTS
        .iter()
        .map(|component| ResourceLine {
            component: component.to_string(),
            target: None,
            quantity: Some(if component == &"provider_unknown_exposure" {
                0
            } else {
                2
            }),
            unit_price_micros: Some(price),
            pool_from: None,
            top_up: false,
        })
        .collect()
}

fn envelope() -> Vec<EnvelopeQty> {
    ENVELOPE_COMPONENTS
        .iter()
        .map(|component| {
            let quantity = match *component {
                "calls_per_subject" => 50,
                "input_tokens_per_subject" => 500_000,
                "output_tokens_per_subject" => 50_000,
                "wall_minutes_per_subject" => 20,
                "verifier_minutes" => 10,
                "concurrent_subjects" => 2,
                "cores_per_subject" => 4,
                "ram_gib_per_subject" => 8,
                "cpu_core_hours" => 100,
                "storage_gib" => 40,
                "wall_days" => 10,
                _ => 0,
            };
            EnvelopeQty {
                component: component.to_string(),
                quantity,
            }
        })
        .collect()
}

fn endpoints(margin: Option<i64>) -> Vec<EndpointQuote> {
    vec![
        EndpointQuote {
            dimension: "capability".into(),
            margin_ppm: margin,
            assumed_margin: false,
        },
        EndpointQuote {
            dimension: "efficiency".into(),
            margin_ppm: margin,
            assumed_margin: false,
        },
    ]
}

fn study_stage(stage: &str, tasks: u32, repeats: u32) -> StageDecl {
    let mut resources = worksheet(100);
    if stage == "confirmation" {
        for l in &mut resources {
            if l.component == "subjects_first" {
                l.quantity = Some(2 * tasks as u64 * repeats as u64);
            }
        }
    }
    if stage == "parity" {
        resources.retain(|l| l.component != "subjects_first");
        resources.retain(|l| l.component != "reproduction");
        // Both runners are quoted — one priced target line each.
        for r in ["hh-runner", "original-runner"] {
            resources.push(ResourceLine {
                component: "provider_billed".into(),
                target: Some(r.into()),
                quantity: Some(1),
                unit_price_micros: Some(10),
                pool_from: None,
                top_up: false,
            });
        }
    }
    StageDecl {
        stage: stage.into(),
        tasks,
        repeats,
        declared_pairs: if stage == "parity" {
            Some(tasks as u64 * repeats as u64)
        } else {
            None
        },
        inputs: pins(InputState::Bound(format!("ref:{stage}"))),
        endpoints: if stage == "parity" {
            endpoints(Some(200_000))
        } else {
            vec![]
        },
        strata: vec!["s-low".into(), "s-high".into()],
        policies: if matches!(stage, "pilot" | "parity" | "confirmation") {
            vec!["A".into(), "B".into()]
        } else {
            vec!["A".into()]
        },
        runners: if stage == "parity" {
            vec!["hh-runner".into(), "original-runner".into()]
        } else {
            vec!["hh-runner".into()]
        },
        envelope: if matches!(stage, "pilot" | "confirmation") {
            envelope()
        } else {
            vec![]
        },
        resources,
        discordances: if stage == "parity" {
            vec![1, 3, 8, 38]
        } else {
            vec![]
        },
        cloud_fallback: false,
        outcome_conditioned: false,
        dispatch_requested: true,
    }
}

/// The complete declaration: all prerequisites satisfied, all pins
/// bound, all components priced, currency set, the five stages.
fn complete() -> QuoteDecl {
    QuoteDecl {
        currency: Some("USD".into()),
        prerequisites: prereqs(PrereqStatus::Satisfied("ev:p7".into())),
        stages: vec![
            study_stage("activation", 1, 0),
            study_stage("pilot", 0, 0),
            study_stage("parity", 30, 5),
            study_stage("confirmation", 30, 5),
            study_stage("replication", 0, 0),
        ],
    }
}

fn stage<'a>(q: &'a ExposureQuote, name: &str) -> &'a hharness_eval::exposure_quote::StagePacket {
    q.stages.iter().find(|s| s.stage == name).expect(name)
}

// ── AC-1: the complete packet ─────────────────────────────────────

#[test]
fn ac1_complete_packet_all_stages_ready() {
    let q = build_quote(&complete()).expect("complete decl");
    for s in &q.stages {
        assert_eq!(s.state, "ready_for_gate", "{}: {:?}", s.stage, s.blockers);
        assert!(s.blockers.is_empty());
    }
    assert_eq!(stage(&q, "activation").dispatch, "gate_pending:GATE-G12");
    assert_eq!(stage(&q, "pilot").dispatch, "gate_pending:GATE-G12");
    assert_eq!(stage(&q, "parity").dispatch, "gate_pending:GATE-G13");
    assert_eq!(stage(&q, "confirmation").dispatch, "gate_pending:GATE-G14");
    assert_eq!(
        stage(&q, "replication").dispatch,
        "gate_pending:unscheduled"
    );
    assert_eq!(q.currency.as_deref(), Some("USD"));
    assert!(q.total_micros > 0);
    assert!(q.check_id());
}

#[test]
fn ac1_deterministic_identity() {
    let a = build_quote(&complete()).unwrap();
    let b = build_quote(&complete()).unwrap();
    assert_eq!(a.quote_id, b.quote_id);
    assert_eq!(
        a.to_json().to_canonical_string(),
        b.to_json().to_canonical_string()
    );
}

#[test]
fn ac1_subjects_and_pairs_are_distinct_counts() {
    let q = build_quote(&complete()).unwrap();
    // 300 A/B subjects are not 300 H/O pairs: confirmation carries
    // ab_subjects=2·30·5=300; parity carries ho_pairs=30·5=150.
    assert_eq!(stage(&q, "confirmation").counts["ab_subjects"], 300);
    assert_eq!(stage(&q, "parity").counts["ho_pairs"], 150);
    assert!(!stage(&q, "parity").counts.contains_key("ab_subjects"));
    assert!(!stage(&q, "confirmation").counts.contains_key("ho_pairs"));
}

#[test]
fn ac1_precision_rows_reproduce_spec_diagnostics() {
    let q = build_quote(&complete()).unwrap();
    let p = stage(&q, "parity");
    assert_eq!(p.precision_state, "attainable");
    // §4.5: at η=.025, N=150, zero and equal-directional counts
    // {1,3,8,38} → half-widths {.024293,.042944,.065355,.100601,.190899}
    // (±1ppm outward rounding).
    let expected = [
        (0, 24_293),
        (1, 42_944),
        (3, 65_355),
        (8, 100_601),
        (38, 190_899),
    ];
    assert_eq!(p.precision.len(), expected.len());
    for (row, (c, hw)) in p.precision.iter().zip(expected) {
        assert_eq!(row.c, c);
        assert!(
            (row.half_width_ppm - hw).abs() <= 2,
            "c={c}: half-width {} vs spec {hw}",
            row.half_width_ppm
        );
        assert_eq!(row.half_width_ppm, row.hi_ppm - row.lo_ppm);
    }
    // A narrower margin turns the same scenarios infeasible.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.endpoints = endpoints(Some(50_000));
        }
    }
    let q2 = build_quote(&d).unwrap();
    let p2 = stage(&q2, "parity");
    assert_eq!(p2.precision_state, "infeasible");
    assert_eq!(p2.state, "infeasible");
    assert!(p2.blockers.contains(&"precision:infeasible".to_string()));
    assert_eq!(p2.dispatch, "refused:precision:infeasible");
}

#[test]
fn ac1_worksheet_components_quoted_and_summed() {
    let q = build_quote(&complete()).unwrap();
    // Every required component priced 100µ × qty → nonzero totals.
    let conf = stage(&q, "confirmation");
    assert!(conf.exposure_micros > 0);
    assert_eq!(
        q.total_micros,
        q.stages.iter().map(|s| s.exposure_micros).sum::<i64>()
    );
    // The record carries the worksheet verbatim.
    let j = q.to_json();
    let stages = arr(&obj(&j)["stages"]);
    assert_eq!(stages.len(), 5);
    assert_eq!(obj(&j)["schema"], Json::str("exposure-quote/1"));
}

// ── AC-2: blocks and typed refusals ───────────────────────────────

#[test]
fn ac2_unset_currency_prices_zero_and_blocks_funding() {
    let mut d = complete();
    d.currency = None;
    let q = build_quote(&d).unwrap();
    assert_eq!(q.total_micros, 0);
    for s in &q.stages {
        assert_eq!(s.exposure_micros, 0);
        assert_eq!(s.state, "blocked");
        assert!(s.blockers.contains(&"funding:currency_unset".to_string()));
        // The dependent action alone is refused — the packet exists.
        assert!(s.dispatch.starts_with("refused:funding:"));
    }
}

#[test]
fn ac2_unresolved_inputs_block_only_dependent_action() {
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            for i in &mut s.inputs {
                match i.name.as_str() {
                    "endpoint_map" | "backend" | "rights" | "runner" => {
                        i.state = InputState::Unresolved;
                    }
                    "oracle" => i.state = InputState::Unknown,
                    _ => {}
                }
            }
        }
    }
    let q = build_quote(&d).unwrap();
    let p = stage(&q, "parity");
    assert_eq!(p.state, "blocked");
    for b in [
        "map:endpoint_map",
        "backend:backend",
        "rights:rights",
        "pins:runner",
        "pins:oracle",
    ] {
        assert!(p.blockers.contains(&b.to_string()), "missing {b}");
    }
    // Only the dependent action: other stages stay ready.
    assert_eq!(stage(&q, "confirmation").state, "ready_for_gate");
    assert_eq!(stage(&q, "pilot").state, "ready_for_gate");
    assert_eq!(p.dispatch, "refused:backend:backend");
}

#[test]
fn ac2_unpriced_and_unbounded_exposure_blocks() {
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "pilot" {
            for l in &mut s.resources {
                if l.component == "egress_gib" {
                    l.unit_price_micros = None;
                }
                if l.component == "provider_unknown_exposure" {
                    l.quantity = None; // unbounded — never a guess
                }
            }
        }
    }
    let q = build_quote(&d).unwrap();
    let p = stage(&q, "pilot");
    assert_eq!(p.state, "blocked");
    assert!(p
        .blockers
        .contains(&"exposure:unpriced:egress_gib".to_string()));
    assert!(p
        .blockers
        .contains(&"exposure:unbounded:provider_unknown_exposure".to_string()));
    assert_eq!(stage(&q, "confirmation").state, "ready_for_gate");
}

#[test]
fn ac2_missing_components_and_unset_bounds_block() {
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            s.resources
                .retain(|l| !matches!(l.component.as_str(), "backups" | "retention"));
            s.envelope.retain(|e| e.component != "wall_days");
        }
    }
    let q = build_quote(&d).unwrap();
    let c = stage(&q, "confirmation");
    assert_eq!(c.state, "blocked");
    assert!(c
        .blockers
        .contains(&"exposure:unquoted:backups".to_string()));
    assert!(c
        .blockers
        .contains(&"exposure:unquoted:retention".to_string()));
    assert!(c
        .blockers
        .contains(&"exposure:bound_unset:wall_days".to_string()));
}

#[test]
fn ac2_unmet_prerequisite_blocks_every_stage() {
    let mut d = complete();
    for p in &mut d.prerequisites {
        if p.name == "parity" {
            p.status = PrereqStatus::Unmet;
        }
        if p.name == "method" {
            p.status = PrereqStatus::Unknown;
        }
    }
    let q = build_quote(&d).unwrap();
    for s in &q.stages {
        assert_eq!(s.state, "blocked");
        assert!(s.blockers.contains(&"prerequisite:parity".to_string()));
        assert!(s.blockers.contains(&"prerequisite:method".to_string()));
    }
}

#[test]
fn ac2_margin_unresolved_blocks_precision() {
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.endpoints[0].margin_ppm = None;
        }
    }
    let q = build_quote(&d).unwrap();
    let p = stage(&q, "parity");
    assert_eq!(p.precision_state, "unresolved");
    assert_eq!(p.state, "blocked");
    assert!(p
        .blockers
        .contains(&"precision:margin_unresolved:capability".to_string()));
}

#[test]
fn ac2_forbidden_constructs_refuse() {
    // automatic top-up
    let mut d = complete();
    d.stages[1].resources[0].top_up = true;
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::AutomaticTopUp { .. })
    ));
    // pooled stage capacity
    let mut d = complete();
    d.stages[1].resources[0].pool_from = Some("confirmation".into());
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::PooledCapacity { .. })
    ));
    // guessed margin
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.endpoints[0].assumed_margin = true;
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::GuessedMargin { .. })
    ));
    // new cloud
    let mut d = complete();
    d.stages[1].cloud_fallback = true;
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::CloudFallback { .. })
    ));
    // favorable cohort shopping
    let mut d = complete();
    d.stages[3].outcome_conditioned = true;
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::FavorableShopping { .. })
    ));
}

#[test]
fn ac2_count_and_floor_refusals() {
    // R=4 ⇒ 2TR < 10T.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            s.repeats = 4;
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::SubjectFloor { t: 30, r: 4, .. })
    ));
    // declared pairs ≠ T·R.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.declared_pairs = Some(300); // the A/B-subject confusion
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::CountMismatch {
            declared: 300,
            computed: 150,
            ..
        })
    ));
    // a subjects line on the pairs stage conflates the counts.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.resources.push(ResourceLine {
                component: "subjects_first".into(),
                target: None,
                quantity: Some(300),
                unit_price_micros: Some(1),
                pool_from: None,
                top_up: false,
            });
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::CountConflation { .. })
    ));
    // a pairs count on a subject stage conflates too.
    let mut d = complete();
    d.stages[3].declared_pairs = Some(150);
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::CountConflation { .. })
    ));
}

#[test]
fn ac2_envelope_refusals() {
    // pilot above 60 first subjects.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "pilot" {
            for l in &mut s.resources {
                if l.component == "subjects_first" {
                    l.quantity = Some(61);
                }
            }
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::EnvelopeExceeded {
            declared: 61,
            ceiling: 60,
            ..
        })
    ));
    // confirmation tranche over 900 core-hours.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            for e in &mut s.envelope {
                if e.component == "cpu_core_hours" {
                    e.quantity = 901;
                }
            }
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::EnvelopeExceeded {
            declared: 901,
            ceiling: 900,
            ..
        })
    ));
    // a subjects_first line at 301 on confirmation hits its ceiling.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            for l in &mut s.resources {
                if l.component == "subjects_first" {
                    l.quantity = Some(301);
                }
            }
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::EnvelopeExceeded { ceiling: 300, .. })
            | Err(QuoteError::CountMismatch { .. })
    ));
}

#[test]
fn ac2_vocabulary_and_shape_refusals() {
    // unknown stage
    let mut d = complete();
    d.stages[0].stage = "collection".into();
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::StageVocabulary { .. })
    ));
    // duplicate stage
    let mut d = complete();
    d.stages.push(d.stages[0].clone());
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::DuplicateStage { .. })
    ));
    // unknown pin
    let mut d = complete();
    d.stages[0].inputs[0].name = "mood".into();
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::PinVocabulary { .. })
    ));
    // missing pin member
    let mut d = complete();
    d.stages[0].inputs.retain(|i| i.name != "drift");
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::MissingKey { .. })
    ));
    // duplicate pin
    let mut d = complete();
    let dup = d.stages[0].inputs[0].clone();
    d.stages[0].inputs.push(dup);
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::DuplicatePin { .. })
    ));
    // unknown component
    let mut d = complete();
    d.stages[1].resources[0].component = "vibes".into();
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::ComponentVocabulary { .. })
    ));
    // unknown endpoint dimension
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.endpoints[0].dimension = "vibes".into();
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::EndpointVocabulary { .. })
    ));
    // duplicated endpoint dimension — the echo map would silently
    // collapse it; refuse instead.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.endpoints.push(s.endpoints[0].clone());
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::EndpointVocabulary { .. })
    ));
    // replication may quote both runners and both policies.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "replication" {
            s.runners = vec!["hh-runner".into(), "original-runner".into()];
            s.policies = vec!["A".into(), "B".into()];
        }
    }
    let q = build_quote(&d).unwrap();
    assert_eq!(stage(&q, "replication").state, "ready_for_gate");
    // a margin-free endpoint on a non-parity stage is echoed, not a
    // precision obligation.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            s.endpoints = endpoints(None);
        }
    }
    let q = build_quote(&d).unwrap();
    assert_eq!(stage(&q, "confirmation").state, "ready_for_gate");
    // parity with one runner
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.runners = vec!["hh-runner".into()];
        }
    }
    assert!(matches!(build_quote(&d), Err(QuoteError::RunnerSet { .. })));
    // activation with tasks ≠ 1
    let mut d = complete();
    d.stages[0].tasks = 2;
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::ActivationShape { .. })
    ));
    // parity without both runners quoted by a target line
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.resources
                .retain(|l| l.target.as_deref() != Some("original-runner"));
        }
    }
    let q = build_quote(&d).unwrap();
    let p = stage(&q, "parity");
    assert!(p
        .blockers
        .contains(&"exposure:runner_unquoted:original-runner".to_string()));
}

// ── pass-2 self-review repairs ────────────────────────────────────

#[test]
fn ac2_infeasible_dominates_an_unresolved_margin() {
    // Severity is order-independent: an unresolved margin never
    // downgrades a proven infeasible endpoint. The reversed order
    // reported `unresolved` before the pass-2 repair.
    for order in [
        vec![("capability", Some(50_000)), ("efficiency", None)],
        vec![("efficiency", None), ("capability", Some(50_000))],
    ] {
        let mut d = complete();
        for s in &mut d.stages {
            if s.stage == "parity" {
                s.endpoints = order
                    .iter()
                    .map(|(dim, m)| EndpointQuote {
                        dimension: dim.to_string(),
                        margin_ppm: *m,
                        assumed_margin: false,
                    })
                    .collect();
            }
        }
        let q = build_quote(&d).unwrap();
        let p = stage(&q, "parity");
        assert_eq!(p.precision_state, "infeasible", "order {order:?}");
        assert_eq!(p.state, "infeasible");
        assert!(p.blockers.contains(&"precision:infeasible".to_string()));
        // The unresolved margin is still named, not hidden.
        assert!(p
            .blockers
            .contains(&"precision:margin_unresolved:efficiency".to_string()));
    }
}

#[test]
fn ac2_pathological_counts_stay_total() {
    // Totality: absurd declared counts never panic — the doubled
    // subject count saturates and the 2·T·R / c·1e6 products are
    // computed wide (u128). Every leg panicked before the repair.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "confirmation" {
            // N = T·R ≈ 1.2e19 — 2·N overflows u64.
            s.tasks = 4_000_000_000;
            s.repeats = 3_000_000_000;
            for l in &mut s.resources {
                if l.component == "subjects_first" {
                    l.quantity = Some(300); // ≤ ceiling → count check
                }
            }
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::CountMismatch {
            member: "subjects_first",
            declared: 300,
            computed: u64::MAX, // saturated, not wrapped
            ..
        })
    ));
    // Parity N > i64::MAX exceeds the diagnostic's domain — a typed
    // refusal, never a wrapped negative n into kl_directional_bound.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.tasks = 4_000_000_000;
            s.repeats = 3_000_000_000;
            s.declared_pairs = Some(12_000_000_000_000_000_000);
            s.discordances = vec![6_000_000_000_000_000_000];
        }
    }
    assert!(matches!(
        build_quote(&d),
        Err(QuoteError::ExposureMalformed { component, .. })
            if component == "declared_pairs"
    ));
    // Parity at a representable-but-huge N still computes (the
    // c·1e6 numerator needs u128); replication at u32::MAX² keeps
    // its saturated subject count and stays ready.
    let mut d = complete();
    for s in &mut d.stages {
        if s.stage == "parity" {
            s.tasks = 3_000_000_000;
            s.repeats = 3_000_000_000;
            s.declared_pairs = Some(9_000_000_000_000_000_000);
            s.discordances = vec![4_500_000_000_000_000_000];
        }
        if s.stage == "replication" {
            s.tasks = u32::MAX;
            s.repeats = u32::MAX;
        }
    }
    let q = build_quote(&d).expect("huge counts stay total");
    let p = stage(&q, "parity");
    assert_eq!(p.counts["ho_pairs"], 9_000_000_000_000_000_000);
    assert_eq!(p.precision[1].x_ppm, 500_000);
    assert_eq!(stage(&q, "replication").state, "ready_for_gate");
}
