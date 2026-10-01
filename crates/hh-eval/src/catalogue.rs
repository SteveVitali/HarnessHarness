//! The Stage-3 metric catalogue + deterministic oracle declarations
//! (spec §5h.2 §2.1–2.3; R-2.9.2; S3.3).
//!
//! The catalogue is data: `SCORECARD_METRICS` is the complete set of
//! `MetricDeclaration`s the Stage-3 scorecard renders, and
//! `ORACLE_DECLARATIONS` the deterministic-oracle registry rows the
//! `hh-eval-oracle` binary implements. `check_catalogue` is the catalogue's
//! own conformance check — every declaration `validate()`s, every headline
//! metric admits only the C0 deterministic oracle classes, every C0 veto has
//! a declaration, and the catalogue carries a content-addressed
//! `metric_registry_version` (the scorecard names it).
//!
//! Adding a metric is a registry entry; adding a dimension is an ADR
//! (ADR-0045 D1). Every rate is `ppm` (`0..=1_000_000`); heavy-tailed units
//! (`tokens`, `money`, `ms`) never take `clt` (ADR-0158 — enforced by
//! `MetricDeclaration::validate`, exercised by `check_catalogue`).

use std::collections::BTreeSet;

use hh_ontology::compliance::{Detector, MetricDeclaration, MetricError};
use hh_ontology::eval::{
    ChargedTo, IntervalMethod, MediationChannel, MediationRequirement, MetricLevel,
    MetricValueType, OracleClass, OracleDeclaration, OracleError, OutcomeClassPolicy,
    ReplicateReducer, VerdictType,
};
use hh_ontology::lab::EnvironmentFamily;
use hh_ontology::participant::{Granularity, Observability, ParticipantClass};
use hh_ontology::{Dimension, Direction};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::compliance::names;
use crate::vetoes::veto_id;

/// The catalogue's deterministic oracle classes (the C0 headline set —
/// ADR-0047 D2).
fn c0_oracles() -> BTreeSet<OracleClass> {
    [
        OracleClass::Executable,
        OracleClass::EndState,
        OracleClass::OutputCheck,
        OracleClass::TracePredicate,
        OracleClass::ProtocolCheck,
    ]
    .into_iter()
    .collect()
}

fn all_classes() -> BTreeSet<ParticipantClass> {
    [ParticipantClass::Native, ParticipantClass::Hosted]
        .into_iter()
        .collect()
}

fn all_granularities() -> BTreeSet<Granularity> {
    [
        Granularity::ComponentLevel,
        Granularity::ConfigurationLevel,
        Granularity::ProductLevel,
    ]
    .into_iter()
    .collect()
}

fn det() -> BTreeSet<Detector> {
    [Detector::Deterministic].into_iter().collect()
}

/// The shared row base — `MetricDeclaration::default()` plus the Stage-3
/// catalogue's common members (both classes, every granularity, the C0
/// deterministic oracle set).
fn base() -> MetricDeclaration {
    MetricDeclaration {
        applies_to_classes: all_classes(),
        admissible_granularities: all_granularities(),
        oracle_classes_allowed: c0_oracles(),
        ..MetricDeclaration::default()
    }
}

/// A catalogue row for a veto metric (`veto: true`, `charged_to: instrument`
/// — a veto is never the subject's spend).
fn veto_metric(name: &str, dimension: Dimension) -> MetricDeclaration {
    MetricDeclaration {
        name: name.into(),
        dimension,
        level: MetricLevel::Run,
        value_type: MetricValueType::Bool,
        direction: Direction::Lower,
        unit: "ppm".into(),
        requires_observability: [Observability::Events].into_iter().collect(),
        applies_to_classes: all_classes(),
        applies_to_families: BTreeSet::new(),
        requires_capabilities: BTreeSet::new(),
        requires_mediation: MediationRequirement::Any,
        admissible_granularities: all_granularities(),
        detector_classes_allowed: det(),
        oracle_classes_allowed: [OracleClass::TracePredicate].into_iter().collect(),
        replicate_reducer: ReplicateReducer::Mean,
        interval_method: IntervalMethod::Wilson,
        outcome_class_policy: OutcomeClassPolicy::for_capability(),
        headline: false,
        veto: true,
        charged_to: ChargedTo::Instrument,
    }
}

/// The Stage-3 scorecard metric catalogue (the `metric_registry` content —
/// `metric_registry_version` is its content address).
pub fn scorecard_metrics() -> Vec<MetricDeclaration> {
    let mut metrics = vec![
        // ── capability (headline) ────────────────────────────────────────
        MetricDeclaration {
            name: "task_success".into(),
            dimension: Dimension::Capability,
            level: MetricLevel::Task,
            value_type: MetricValueType::Decimal,
            direction: Direction::Higher,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            headline: true,
            ..MetricDeclaration {
                requires_observability: [Observability::EndState].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_capability(),
                ..base()
            }
        },
        // pass^k / pass@k over the same distribution — the replicate reducers
        // carry k (τ-bench pass^k; Chen pass@k).
        MetricDeclaration {
            name: "task_success_pass_k".into(),
            dimension: Dimension::Capability,
            level: MetricLevel::Task,
            value_type: MetricValueType::Decimal,
            unit: "ppm".into(),
            replicate_reducer: ReplicateReducer::PassK(2),
            interval_method: IntervalMethod::Wilson,
            headline: true,
            ..MetricDeclaration {
                requires_observability: [Observability::EndState].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_capability(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "task_success_pass_at".into(),
            dimension: Dimension::Capability,
            level: MetricLevel::Task,
            value_type: MetricValueType::Decimal,
            unit: "ppm".into(),
            replicate_reducer: ReplicateReducer::PassAt(4),
            interval_method: IntervalMethod::Wilson,
            headline: true,
            ..MetricDeclaration {
                requires_observability: [Observability::EndState].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_capability(),
                ..base()
            }
        },
        // ── reliability ──────────────────────────────────────────────────
        MetricDeclaration {
            name: "budget_exhausted_rate".into(),
            dimension: Dimension::Reliability,
            level: MetricLevel::Run,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..base()
        },
        MetricDeclaration {
            name: "infra_failure_rate".into(),
            dimension: Dimension::Reliability,
            level: MetricLevel::Run,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..base()
        },
        // ── efficiency (heavy-tailed units — clustered_clt, never clt) ───
        MetricDeclaration {
            name: "cost_tokens_total".into(),
            dimension: Dimension::Efficiency,
            level: MetricLevel::Run,
            unit: "tokens".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
            ..base()
        },
        MetricDeclaration {
            name: "cost_spend_total".into(),
            dimension: Dimension::Efficiency,
            level: MetricLevel::Run,
            unit: "money".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
            ..base()
        },
        MetricDeclaration {
            name: "wall_time_ms".into(),
            dimension: Dimension::Efficiency,
            level: MetricLevel::Run,
            unit: "ms".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
            ..base()
        },
        // ── compliance chain (ADR-0045 D4; requires {events}) ────────────
        MetricDeclaration {
            name: names::ARTEFACT_DELIVERY_RATE.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: names::ARTEFACT_ACTIVATION_RATE.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: names::ARTEFACT_FOLLOW_RATE.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: names::DETECTOR_CONFORMITY.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: names::INTERVENTION_RATE.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: names::ATTRIBUTION_COMPLETENESS.into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        // ── portability: dynamic opacity (n/a{class} for hosted — the
        //    declaration's applies_to_classes is native-only) ─────────────
        MetricDeclaration {
            name: names::OPACITY_DYNAMIC.into(),
            dimension: Dimension::Portability,
            unit: "ppm".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                applies_to_classes: [ParticipantClass::Native].into_iter().collect(),
                ..base()
            }
        },
        // ── §5c context/memory (S3.8 — the context_metrics fold's rows) ──
        // Heavy-tailed units take clustered_clt; rates are ppm/Wilson; every
        // row requires the events the fold reads (ADR-0045 D4).
        MetricDeclaration {
            name: "harness_overhead.tokens".into(),
            dimension: Dimension::Efficiency,
            unit: "tokens".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "harness_overhead.model_calls".into(),
            dimension: Dimension::Efficiency,
            unit: "calls".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "compaction.tokens_freed".into(),
            dimension: Dimension::Efficiency,
            unit: "tokens".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                outcome_class_policy: OutcomeClassPolicy::for_efficiency(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "reacquisition_count".into(),
            dimension: Dimension::Efficiency,
            unit: "count".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "repeated_action_count".into(),
            dimension: Dimension::Efficiency,
            unit: "count".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "recall_probe_hit_rate".into(),
            dimension: Dimension::Reliability,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Ledger].into_iter().collect(),
                ..base()
            }
        },
        // The memory compliance chain (AC-R-2.4.3-9's emitted rows).
        MetricDeclaration {
            name: "memory.delivered".into(),
            dimension: Dimension::Compliance,
            unit: "count".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.activated".into(),
            dimension: Dimension::Compliance,
            unit: "count".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.followed".into(),
            dimension: Dimension::Compliance,
            unit: "count".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.promoted".into(),
            dimension: Dimension::Compliance,
            unit: "count".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.withheld".into(),
            dimension: Dimension::Reliability,
            unit: "count".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.over_invalidation".into(),
            dimension: Dimension::Reliability,
            unit: "count".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.validity_rate".into(),
            dimension: Dimension::Reliability,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "memory.activated_given_delivered".into(),
            dimension: Dimension::Compliance,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "compaction.applied".into(),
            dimension: Dimension::Efficiency,
            unit: "count".into(),
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        MetricDeclaration {
            name: "compaction.model_calls".into(),
            dimension: Dimension::Efficiency,
            unit: "calls".into(),
            direction: Direction::Lower,
            interval_method: IntervalMethod::ClusteredClt,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
        // ── security (AgentDojo — §5h.4 process metric) ──────────────────
        MetricDeclaration {
            name: "injection_delivered_rate".into(),
            dimension: Dimension::Security,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                applies_to_families: [EnvironmentFamily::AdversarialSecurity]
                    .into_iter()
                    .collect(),
                requires_observability: [Observability::Events].into_iter().collect(),
                ..base()
            }
        },
    ];
    // The veto metrics (the C0/Stage-3 veto tier — §5h.2 §2.3 + §5h.4). Per-veto
    // applicability overrides (ADR-0165 D6–D8; §6.6 §2.4): a hosted row never
    // claims a security property the Lab did not mediate —
    // `veto.permission_violation` requires `mediated(effects)`; the
    // duplicate-effect/audit-completeness vetoes are `n/a{class}` on hosted
    // rows (participant-reported tool events never enter the effect lifecycle
    // and a hosted ledger carries no audit surface to complete).
    for (id, dim) in [
        (veto_id::DUPLICATE_EFFECT, Dimension::Compliance),
        (veto_id::AUDIT_COMPLETENESS, Dimension::Compliance),
        (veto_id::PERMISSION_VIOLATION, Dimension::Security),
        (veto_id::FALSE_COMPLETION, Dimension::Capability),
        (veto_id::ATTRIBUTION_COMPLETENESS, Dimension::Compliance),
        (veto_id::BENCHMARK_EGRESS, Dimension::Security),
        (veto_id::HELD_OUT_LEAK, Dimension::Security),
        (veto_id::UNCOMPENSATED_MUTATION, Dimension::Compliance),
        (veto_id::PARTIAL_CONSUMPTION, Dimension::Compliance),
        (veto_id::SHARED_VERIFIER_CONTAMINATION, Dimension::Security),
        (veto_id::GRADER_LOG_INCONSISTENT, Dimension::Reliability),
        (veto_id::SUITE_NOT_RUN, Dimension::Reliability),
        (veto_id::HELD_OUT_SKIPPED, Dimension::Reliability),
    ] {
        let mut m = veto_metric(&format!("veto.{id}"), dim);
        match id {
            veto_id::PERMISSION_VIOLATION => {
                m.requires_mediation = MediationRequirement::Mediated(MediationChannel::Effects);
            }
            veto_id::DUPLICATE_EFFECT | veto_id::AUDIT_COMPLETENESS => {
                m.applies_to_classes = [ParticipantClass::Native].into_iter().collect();
            }
            _ => {}
        }
        metrics.push(m);
    }
    // The hosted-specific declarations (§6.6 §2.4 — `applies_to_classes =
    // {hosted}`, non-veto). Schema-only at C0/Stage 3: the folds land with the
    // hosting service (S4.5a); a declared row renders `n/a{class}` on native
    // arms and is never coerced to a value a fold did not produce.
    for name in [
        "hosted_observation_completeness",
        "mediation_coverage",
        "usage_report_agreement",
        "conformance_drift_count",
        "synthesized_terminal_rate",
        "observed_duplicate_action_rate",
        "observed_blast_radius",
    ] {
        metrics.push(MetricDeclaration {
            name: name.into(),
            dimension: Dimension::Reliability,
            level: MetricLevel::Run,
            unit: "ppm".into(),
            interval_method: IntervalMethod::Wilson,
            ..MetricDeclaration {
                requires_observability: [Observability::Events].into_iter().collect(),
                applies_to_classes: [ParticipantClass::Hosted].into_iter().collect(),
                ..base()
            }
        });
    }
    metrics
}

/// The Stage-3 deterministic oracle declarations (the `hh-eval-oracle`
/// binary implements every row — executable/end_state/output_check/
/// trace_predicate/protocol_check; judged/human/teacher classes are never
/// declared deterministic).
pub fn oracle_declarations() -> Vec<OracleDeclaration> {
    let det_oracle =
        |oracle_id: &str, class: OracleClass, obs: &[Observability]| OracleDeclaration {
            oracle_id: oracle_id.into(),
            class,
            deterministic: true,
            requires_observability: obs.iter().copied().collect(),
            verdict_type: VerdictType::Bool,
            evidence_out: vec![
                hh_ontology::eval::EvidenceKind::EndState,
                hh_ontology::eval::EvidenceKind::LedgerRange,
            ],
            charged_to: ChargedTo::Instrument,
            calibration_ref: None,
            provenance: ProvenanceRecord::kernel("hh-eval/stage3-catalogue", 0),
        };
    vec![
        det_oracle(
            "oracle/executable",
            OracleClass::Executable,
            &[Observability::EndState],
        ),
        det_oracle(
            "oracle/end_state",
            OracleClass::EndState,
            &[Observability::EndState],
        ),
        det_oracle(
            "oracle/output_check",
            OracleClass::OutputCheck,
            &[Observability::Events],
        ),
        det_oracle(
            "oracle/trace_predicate",
            OracleClass::TracePredicate,
            &[Observability::Events, Observability::Ledger],
        ),
        det_oracle(
            "oracle/protocol_check",
            OracleClass::ProtocolCheck,
            &[Observability::Events],
        ),
    ]
}

/// A catalogue check finding (typed — never a bare string at the boundary).
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogueFinding {
    /// A declaration failed `validate`.
    InvalidMetric {
        /// The metric name.
        name: String,
        /// The failure.
        error: MetricError,
    },
    /// An oracle declaration failed `validate`.
    InvalidOracle {
        /// The oracle id.
        oracle_id: String,
        /// The failure.
        error: OracleError,
    },
    /// A `headline` metric admits a non-deterministic oracle class.
    HeadlineAdmitsNonDeterministic {
        /// The metric name.
        name: String,
    },
    /// A C0 veto id lacks a catalogue row.
    MissingVeto {
        /// The veto id.
        veto_id: String,
    },
    /// Two rows share a name.
    DuplicateName {
        /// The duplicated name.
        name: String,
    },
    /// A non-deterministic oracle was declared `deterministic` (the
    /// declaration-level tripwire — `validate` covers `judge`; this covers
    /// any class outside the C0 set).
    NonC0OracleDeterministic {
        /// The oracle id.
        oracle_id: String,
    },
}

/// The catalogue check report — `{metrics, oracles, findings,
/// metric_registry_version}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogueReport {
    /// The number of metric rows checked.
    pub metrics: usize,
    /// The number of oracle rows checked.
    pub oracles: usize,
    /// The findings (`[]` ⇒ the catalogue conforms).
    pub findings: Vec<CatalogueFinding>,
    /// The catalogue's content address (`idp/1` over canonical rows) — the
    /// `metric_registry_version` a scorecard names.
    pub metric_registry_version: String,
}

/// The C0 veto ids the catalogue must declare.
pub const C0_VETO_IDS: &[&str] = &[
    veto_id::DUPLICATE_EFFECT,
    veto_id::AUDIT_COMPLETENESS,
    veto_id::PERMISSION_VIOLATION,
    veto_id::FALSE_COMPLETION,
    veto_id::ATTRIBUTION_COMPLETENESS,
    veto_id::BENCHMARK_EGRESS,
    veto_id::HELD_OUT_LEAK,
    veto_id::UNCOMPENSATED_MUTATION,
    veto_id::PARTIAL_CONSUMPTION,
    veto_id::SHARED_VERIFIER_CONTAMINATION,
    veto_id::GRADER_LOG_INCONSISTENT,
    veto_id::SUITE_NOT_RUN,
    veto_id::HELD_OUT_SKIPPED,
];

/// The catalogue conformance check — pure, deterministic, reportable.
pub fn check_catalogue() -> CatalogueReport {
    let metrics = scorecard_metrics();
    let oracles = oracle_declarations();
    let mut findings = Vec::new();
    let mut names = BTreeSet::new();
    for m in &metrics {
        if !names.insert(m.name.clone()) {
            findings.push(CatalogueFinding::DuplicateName {
                name: m.name.clone(),
            });
        }
        if let Err(e) = m.validate() {
            findings.push(CatalogueFinding::InvalidMetric {
                name: m.name.clone(),
                error: e,
            });
        }
        if m.headline && m.oracle_classes_allowed.iter().any(|c| !c.is_c0_headline()) {
            findings.push(CatalogueFinding::HeadlineAdmitsNonDeterministic {
                name: m.name.clone(),
            });
        }
    }
    for id in C0_VETO_IDS {
        let veto_name = format!("veto.{id}");
        if !metrics.iter().any(|m| m.veto && m.name == veto_name) {
            findings.push(CatalogueFinding::MissingVeto {
                veto_id: (*id).to_string(),
            });
        }
    }
    for o in &oracles {
        if let Err(e) = o.validate() {
            findings.push(CatalogueFinding::InvalidOracle {
                oracle_id: o.oracle_id.clone(),
                error: e,
            });
        }
        if o.deterministic && !o.class.is_c0_headline() {
            findings.push(CatalogueFinding::NonC0OracleDeterministic {
                oracle_id: o.oracle_id.clone(),
            });
        }
    }
    let canonical = Json::Arr(
        metrics
            .iter()
            .map(|m| m.to_json())
            .chain(oracles.iter().map(|o| o.to_json()))
            .collect(),
    )
    .to_canonical_string();
    CatalogueReport {
        metrics: metrics.len(),
        oracles: oracles.len(),
        findings,
        metric_registry_version: hh_identity::idp_id("eval.metric_registry", canonical.as_bytes()),
    }
}

/// Look up a metric declaration by name.
pub fn metric(name: &str) -> Option<MetricDeclaration> {
    scorecard_metrics().into_iter().find(|m| m.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_conforms() {
        let r = check_catalogue();
        assert!(r.findings.is_empty(), "findings: {:?}", r.findings);
        assert!(r.metrics >= 15);
        assert_eq!(r.oracles, 5);
        assert!(r.metric_registry_version.starts_with("sha256:"));
    }

    #[test]
    fn registry_version_is_stable() {
        assert_eq!(
            check_catalogue().metric_registry_version,
            check_catalogue().metric_registry_version
        );
    }

    #[test]
    fn headline_metrics_admit_only_deterministic() {
        for m in scorecard_metrics().iter().filter(|m| m.headline) {
            assert!(m.oracle_classes_allowed.iter().all(|c| c.is_c0_headline()));
        }
    }
}
