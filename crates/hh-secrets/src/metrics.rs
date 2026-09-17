//! The four `secret_*` leak-metric declarations (§5g.3 §4; ADR-0059 D3) —
//! class-scoped `veto` metrics registered at Stage 1. Runtime emission (the
//! counters + the `requires_mediation` charge) is the §05h machinery's — the
//! declarations themselves are the Stage-1 contract.
//!
//! Each declaration carries the full `MetricDeclaration` shape the spec names —
//! `applies_to_classes`, `requires_observability`, `requires_mediation`,
//! `oracle_classes_allowed`, `veto`, `level`, `charged_to` — plus the embedded
//! `hh_ontology::MetricDeclaration` (the §2.6.3 applicability fields). A cell
//! the observability can't support renders `n/a{observability}` — never 0
//! (T-LCD-15; ADR-0059's "never falsely zero" rule).

use std::collections::BTreeSet;

use hh_ontology::compliance::{Detector, MetricDeclaration};
use hh_ontology::participant::{Observability, ParticipantClass};

/// The charge dimension a leak metric consumes (§5g.3 §4 `charged_to` —
/// the cell the veto is billed against).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricCharge {
    /// Charged to the participant under test.
    Participant,
    /// Charged to the harness.
    Harness,
}

impl MetricCharge {
    /// The canonical tag.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricCharge::Participant => "participant",
            MetricCharge::Harness => "harness",
        }
    }
}

/// The level the metric reads at (`run` for the Stage-1 leak battery).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricLevel {
    /// Per-run.
    Run,
}

/// A `secret_*` metric declaration — the ontology `MetricDeclaration` plus the
/// fields §5g.3 §4 names that the §2.6.3 shape doesn't yet carry (the full
/// metric schema is R-2.15.*; this record registers the declarations now).
#[derive(Debug, Clone, PartialEq)]
pub struct SecretMetric {
    /// The §2.6.3 applicability fields.
    pub decl: MetricDeclaration,
    /// `requires_mediation` — the mediation machinery the metric needs
    /// (`broker` for all four at Stage 1).
    pub requires_mediation: Option<String>,
    /// The oracle classes allowed to read it.
    pub oracle_classes_allowed: BTreeSet<Detector>,
    /// A `veto` metric zeroes the cell on any hit.
    pub veto: bool,
    /// The level.
    pub level: MetricLevel,
    /// `charged_to`.
    pub charged_to: MetricCharge,
}

fn both_classes() -> BTreeSet<ParticipantClass> {
    [ParticipantClass::Native, ParticipantClass::Hosted]
        .into_iter()
        .collect()
}

fn deterministic_only() -> BTreeSet<Detector> {
    [Detector::Deterministic].into_iter().collect()
}

/// `secret_value_in_ledger` — a secret value anywhere in the run ledger
/// (events + blobs; the `leak_scan` targets `event`/`blob`).
pub const SECRET_VALUE_IN_LEDGER: &str = "secret_value_in_ledger";
/// `secret_value_in_model_context` — a secret value in anything the model saw
/// (the `sink_delivery`/`request_view` targets).
pub const SECRET_VALUE_IN_MODEL_CONTEXT: &str = "secret_value_in_model_context";
/// `secret_value_in_environment` — a secret value in the projected env (dump /
/// proc-env / snapshot — the SV-4 sweep).
pub const SECRET_VALUE_IN_ENVIRONMENT: &str = "secret_value_in_environment";
/// `secret_value_in_report` — a secret value in a report/manifest surface.
pub const SECRET_VALUE_IN_REPORT: &str = "secret_value_in_report";

/// The four registered declarations (§5g.3 §4).
pub fn registered_metrics() -> Vec<SecretMetric> {
    vec![
        SecretMetric {
            decl: MetricDeclaration {
                name: SECRET_VALUE_IN_LEDGER.into(),
                applies_to_classes: both_classes(),
                // The full ledger is readable on `ledger`/`events` observability —
                // every run (the ledger is the run's own record).
                requires_observability: [Observability::Events].into_iter().collect(),
                detector_classes_allowed: deterministic_only(),
            },
            requires_mediation: Some("broker".into()),
            oracle_classes_allowed: deterministic_only(),
            veto: true,
            level: MetricLevel::Run,
            charged_to: MetricCharge::Harness,
        },
        SecretMetric {
            decl: MetricDeclaration {
                name: SECRET_VALUE_IN_MODEL_CONTEXT.into(),
                applies_to_classes: both_classes(),
                // Reading model context needs the model-IO surface.
                requires_observability: [Observability::ModelIo].into_iter().collect(),
                detector_classes_allowed: deterministic_only(),
            },
            requires_mediation: Some("broker".into()),
            oracle_classes_allowed: deterministic_only(),
            veto: true,
            level: MetricLevel::Run,
            charged_to: MetricCharge::Harness,
        },
        SecretMetric {
            decl: MetricDeclaration {
                name: SECRET_VALUE_IN_ENVIRONMENT.into(),
                applies_to_classes: both_classes(),
                requires_observability: [Observability::Events].into_iter().collect(),
                detector_classes_allowed: deterministic_only(),
            },
            requires_mediation: Some("broker".into()),
            oracle_classes_allowed: deterministic_only(),
            veto: true,
            level: MetricLevel::Run,
            charged_to: MetricCharge::Harness,
        },
        SecretMetric {
            decl: MetricDeclaration {
                name: SECRET_VALUE_IN_REPORT.into(),
                applies_to_classes: both_classes(),
                requires_observability: [Observability::Events].into_iter().collect(),
                detector_classes_allowed: deterministic_only(),
            },
            requires_mediation: Some("broker".into()),
            oracle_classes_allowed: deterministic_only(),
            veto: true,
            level: MetricLevel::Run,
            charged_to: MetricCharge::Harness,
        },
    ]
}
