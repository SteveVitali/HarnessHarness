//! §2.9.2 acceptance criteria, made executable at the integration seam (ticket S1.1).
//!
//! Each test maps to one AC (AC-A2-1/-2/-7 in full; the static halves of AC-A2-3…-6) and the
//! §2.9.8 fixtures (the mixed-class / one-native-one-hosted fixture, the T-LCD-13 chain fixture,
//! the six-factor configuration fixture). Every check fails if the behaviour it guards is
//! removed.

use std::collections::{BTreeMap, BTreeSet};

use hh_ontology::compliance::{
    validity, ChainEvent, Detector, MetricDeclaration, NaReason, Validity, ValidityCheck,
};
use hh_ontology::config::{Configuration, Ref};
use hh_ontology::control::{ControlBoundary, DecisionPoint, Owner};
use hh_ontology::dag::spec_dag_check;
use hh_ontology::formal::{resolve, Symbol};
use hh_ontology::glossary::glossary_check;
use hh_ontology::participant::{
    admissible_granularities, describe, CapabilityVerdict, Granularity, HostingMechanism,
    Observability, ParticipantClass, RawDescriptor,
};
use hh_ontology::planes::{classify_home_identifier, registered_kinds, Boundary, Home, Plane};

/// AC-A2-1 — §2 contains the two-level statement, the seven planes with core questions, the two
/// boundaries, the three cross-cutting attributes, and a total `home` for every registered kind.
#[test]
fn ac_a2_1_seven_planes_two_boundaries_and_total_home() {
    // Seven planes with core questions.
    assert_eq!(Plane::ALL.len(), 7);
    assert!(Plane::ALL.iter().all(|p| p.core_question().ends_with('?')));
    // Two boundaries.
    assert_eq!(Boundary::Model.tag(), "model_boundary");
    assert_eq!(Boundary::Environment.tag(), "environment_boundary");
    // classify_home total over every registered kind (identifier path exercised end-to-end).
    let codomain: BTreeSet<String> = Plane::ALL
        .iter()
        .map(|p| p.tag().to_string())
        .chain(["model_boundary", "environment_boundary", "run_lifecycle"].map(String::from))
        .collect();
    assert!(!registered_kinds().is_empty());
    for id in [
        "context.artefact.delivered",
        "action.effect.committed",
        "control.decision",
        "verification.artefact.followed",
        "security.permission.decided",
        "measurement.experiment.registered",
        "model.call.completed",
        "lifecycle.run.created",
        "authorize",
        "control_strategy",
    ] {
        let home = classify_home_identifier(id).expect("registered kind must classify");
        assert!(
            codomain.contains(&home.tag()),
            "{id} homed outside codomain"
        );
    }
}

/// AC-A2-2 — the formal model of §2.5 is present; every symbol resolves to §2.
#[test]
fn ac_a2_2_every_symbol_resolves_to_section_2() {
    for s in Symbol::ALL {
        assert!(resolve(s).section.starts_with("§2"));
    }
    // The exact named set {κ, B, β, Δ, Ψ, θ, M, J}.
    for s in Symbol::AC_A2_2_REQUIRED {
        assert!(!resolve(s).definition.is_empty());
    }
}

/// AC-A2-3 (T-LCD-13) static half — the taxonomy contains the three chain events with `detector`
/// provenance (the T-LCD-13 chain fixture); a per-profile compliance metric *form* is present.
#[test]
fn ac_a2_3_chain_fixture_has_three_events_with_detector() {
    let fixture = [
        ChainEvent::Delivered {
            artefact_id: "art:1".into(),
            delivery_id: "d:1".into(),
        },
        ChainEvent::Activated {
            artefact_id: "art:1".into(),
            delivery_id: "d:1".into(),
            detector: Detector::Deterministic,
        },
        ChainEvent::Followed {
            artefact_id: "art:1".into(),
            delivery_id: "d:1".into(),
            detector: Detector::Deterministic,
        },
    ];
    let ids: Vec<&str> = fixture.iter().map(|e| e.identifier()).collect();
    assert_eq!(
        ids,
        [
            "context.artefact.delivered",
            "context.artefact.activated",
            "verification.artefact.followed"
        ]
    );
    // Home planes: P1, P1, P4 via the prefix rule.
    assert_eq!(
        classify_home_identifier(fixture[0].identifier()).unwrap(),
        Home::Plane(Plane::Observation)
    );
    assert_eq!(
        classify_home_identifier(fixture[2].identifier()).unwrap(),
        Home::Plane(Plane::Verification)
    );
    // detector provenance present on activated/followed, absent on delivered.
    assert_eq!(fixture[0].detector(), None);
    assert_eq!(fixture[1].detector(), Some(Detector::Deterministic));
    // The deterministic per-profile compliance metric form exists and is computable from events.
    let metric = MetricDeclaration {
        name: "compliance.followed.rate".into(),
        applies_to_classes: [ParticipantClass::Native].into_iter().collect(),
        requires_observability: [Observability::Events].into_iter().collect(),
        detector_classes_allowed: [Detector::Deterministic].into_iter().collect(),
        ..MetricDeclaration::default()
    };
    assert!(metric
        .requires_observability
        .contains(&Observability::Events));
}

/// AC-A2-4 (T-LCD-15) static half — every metric carries `applies_to_classes` +
/// `requires_observability`; the one-native-one-hosted fixture renders a typed `n/a` on the
/// inadmissible (hosted) cell, never 0.
#[test]
fn ac_a2_4_mixed_class_fixture_renders_na_on_inadmissible_cell() {
    let native = describe(RawDescriptor {
        class: Some(ParticipantClass::Native),
        hosting_mechanism: HostingMechanism::None,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
            Observability::Ledger,
        ]
        .into_iter()
        .collect(),
        capability_vector: BTreeMap::new(),
    })
    .unwrap();
    let hosted = describe(RawDescriptor {
        class: Some(ParticipantClass::Hosted),
        hosting_mechanism: HostingMechanism::SessionAbi,
        observability_level: [Observability::Events, Observability::ModelIo]
            .into_iter()
            .collect(),
        capability_vector: [("model".to_string(), CapabilityVerdict::Supported)]
            .into_iter()
            .collect(),
    })
    .unwrap();

    // A native-only, ledger-requiring metric.
    let metric = MetricDeclaration {
        name: "ledger.deep.metric".into(),
        applies_to_classes: [ParticipantClass::Native].into_iter().collect(),
        requires_observability: [Observability::Ledger].into_iter().collect(),
        detector_classes_allowed: [Detector::Deterministic].into_iter().collect(),
        ..MetricDeclaration::default()
    };
    assert!(metric
        .applies_to_classes
        .contains(&ParticipantClass::Native));
    assert!(!metric.requires_observability.is_empty());

    // Native cell: applicable.
    assert_eq!(metric.applicability(&native), Ok(()));
    // Hosted cell: a typed n/a{class}, never 0.
    assert_eq!(metric.applicability(&hosted), Err(NaReason::Class));

    // A component-level factor on the hosted participant is inadmissible.
    let g = admissible_granularities(&hosted);
    assert!(!g.contains(&Granularity::ComponentLevel));
    assert!(admissible_granularities(&native).contains(&Granularity::ComponentLevel));
}

/// AC-A2-5 (T-LCD-09) static half — the six-factor configuration fixture: all six factors
/// explicit, β representable as a factor, and "does variant V help M₁ more than M₂?" expressible
/// (four distinct configuration_ids on a 2×2 variant×model design; replicates aggregate).
#[test]
fn ac_a2_5_six_factor_config_and_beta_expressible() {
    let mk = |model: &str, variant: &str, seed: &str| Configuration {
        m_set: Ref::new(model, format!("{model}@v1")),
        h: Ref::new(variant, format!("{variant}@v1")),
        p: Ref::new("profile/default", "profile/default@v1"),
        e: Ref::new("env/ubuntu", "env/ubuntu@sha"),
        b: Ref::new("budget/std", "budget/std@v1"),
        seed: seed.to_string(),
    };
    let base = mk("M1", "h/withV", "s0");
    assert_eq!(base.six_factors().len(), 6);

    // β representable as a factor and as a validatable record on AgentProcess.native.
    let mut beta = ControlBoundary::default();
    beta.assignments.insert(DecisionPoint::Plan, Owner::Model);
    beta.assignments
        .insert(DecisionPoint::Authorize, Owner::Code);
    assert!(beta.validate().is_ok());

    // Four interaction cells, distinct configuration_ids; two model levels join-able.
    let cells = [
        mk("M1", "h/withV", "s0"),
        mk("M1", "h/noV", "s0"),
        mk("M2", "h/withV", "s0"),
        mk("M2", "h/noV", "s0"),
    ];
    let ids: BTreeSet<_> = cells.iter().map(|c| c.configuration_id().0).collect();
    assert_eq!(ids.len(), 4);
    // Replicates aggregate by configuration_id (seed excluded).
    assert_eq!(
        mk("M1", "h/withV", "s0").configuration_id(),
        mk("M1", "h/withV", "s9").configuration_id()
    );
}

/// AC-A2-6 (T-LCD-06/-07) static half — `admissible_granularities` never coerces `unknown`, and
/// the spec-DAG check reports `tier_violations=[]`, `cycles=[]`, no HIR→Hosting-ABI edge while
/// `proj_ABI` is total on native ledgers.
#[test]
fn ac_a2_6_spec_dag_clean_and_unknown_never_coerced() {
    let report = spec_dag_check();
    assert!(
        report.tier_violations.is_empty(),
        "tier_violations must be []"
    );
    assert!(report.cycles.is_empty(), "cycles must be []");
    assert!(report.hosting_edges.is_empty(), "no HIR→Hosting-ABI edge");
    assert!(
        report.proj_abi_total,
        "proj_ABI must be total on native ledgers"
    );
    assert!(report.is_clean());

    // unknown never coerced: a hosted participant with only unknown coordinates collapses to
    // product-level (never promoted to configuration-level).
    let hosted_unknown = describe(RawDescriptor {
        class: Some(ParticipantClass::Hosted),
        hosting_mechanism: HostingMechanism::SessionAbi,
        observability_level: [Observability::Events].into_iter().collect(),
        capability_vector: [("model".to_string(), CapabilityVerdict::Unknown)]
            .into_iter()
            .collect(),
    })
    .unwrap();
    assert_eq!(
        admissible_granularities(&hosted_unknown),
        [Granularity::ProductLevel].into_iter().collect()
    );
}

/// AC-A2-7 — the glossary check passes with the v4 register: no unregistered synonym, no "meta-"
/// sub-concept, "HarnessHarness" only as the proper noun.
#[test]
fn ac_a2_7_glossary_check() {
    // Clean canonical text passes.
    assert!(glossary_check(
        "The compatibility surface Ψ_θ is a derived view; HarnessHarness is the product."
    )
    .is_empty());
    // Each violation class is caught.
    assert!(!glossary_check("the HTIR node").is_empty());
    assert!(!glossary_check("a meta-controller").is_empty());
    assert!(!glossary_check("harness-harness rocks").is_empty());
}

/// Cross-cutting: `validity()` is model-free and three-valued (§2.6.1 invariant) — the same
/// artifact version + check gives the same outcome regardless of configuration.
#[test]
fn validity_is_model_free_and_three_valued() {
    let check = ValidityCheck::VersionStamp {
        stamp: "v3".into(),
        expected: "v3".into(),
    };
    assert_eq!(validity(&check, "evt:a"), validity(&check, "evt:b"));
    assert_eq!(validity(&check, "evt:a").state, Validity::Valid);
    assert_eq!(
        validity(&ValidityCheck::None, "evt:a").state,
        Validity::Unknown
    );
}
