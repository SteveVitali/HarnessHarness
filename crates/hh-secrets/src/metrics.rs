//! The four `secret_*` leak-metric declarations (§5g.3 §4; ADR-0059 D3) —
//! class-scoped `veto` metrics registered at Stage 1. Runtime emission (the
//! counters + the `requires_mediation` charge) is the §05h machinery's — the
//! declarations themselves are the Stage-1 contract.
//!
//! S1.22 (R-2.9.2) landed the **single complete `MetricDeclaration` form**
//! (ADR-0045 D1; CF-094 — the partial forms, including this module's earlier
//! `SecretMetric` wrapper, are superseded): each `secret_*` declaration *is* a
//! `MetricDeclaration` carrying `dimension = security`, `veto = true`,
//! `requires_mediation = mediated(egress)` (the credential broker),
//! `charged_to = instrument`, and the deterministic oracle classes the leak
//! scans read (`trace_predicate` over the ledger/model-io trace, `end_state`
//! for environment projections, `output_check` for report surfaces — spec
//! §5h.2 §2.3's veto-invariant oracle rule). A cell the observability can't
//! support renders `n/a{observability}` — never 0 (T-LCD-15; ADR-0059's
//! "never falsely zero" rule).

use std::collections::BTreeSet;

use hh_ontology::compliance::{Detector, MetricDeclaration};
use hh_ontology::eval::{
    ChargedTo, Dimension, Direction, MediationChannel, MediationRequirement, MetricLevel,
    MetricValueType, OracleClass,
};
use hh_ontology::participant::{Observability, ParticipantClass};

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

fn both_classes() -> BTreeSet<ParticipantClass> {
    [ParticipantClass::Native, ParticipantClass::Hosted]
        .into_iter()
        .collect()
}

fn deterministic_only() -> BTreeSet<Detector> {
    [Detector::Deterministic].into_iter().collect()
}

/// The shared `secret_*` declaration shape — a `security` veto metric at run
/// level, mediated by the broker, charged to the instrument.
fn leak_metric(
    name: &str,
    requires: &[Observability],
    oracles: &[OracleClass],
) -> MetricDeclaration {
    MetricDeclaration {
        name: name.into(),
        dimension: Dimension::Security,
        level: MetricLevel::Run,
        value_type: MetricValueType::Bool,
        direction: Direction::Lower,
        unit: "bool".into(),
        requires_observability: requires.iter().copied().collect(),
        applies_to_classes: both_classes(),
        requires_mediation: MediationRequirement::Mediated(MediationChannel::Egress),
        detector_classes_allowed: deterministic_only(),
        oracle_classes_allowed: oracles.iter().copied().collect(),
        veto: true,
        charged_to: ChargedTo::Instrument,
        ..MetricDeclaration::default()
    }
}

/// The four registered declarations (§5g.3 §4) as full `MetricDeclaration`s.
/// Every declaration passes `MetricDeclaration::validate`.
pub fn registered_metrics() -> Vec<MetricDeclaration> {
    vec![
        leak_metric(
            SECRET_VALUE_IN_LEDGER,
            // The ledger observability leg — spec §5g.3 §4's canonical row
            // declares `requires_observability: {ledger}` (the ledger is the
            // run's own record; every run carries it).
            &[Observability::Ledger],
            &[OracleClass::TracePredicate],
        ),
        leak_metric(
            SECRET_VALUE_IN_MODEL_CONTEXT,
            // Reading model context needs the model-IO surface.
            &[Observability::ModelIo],
            &[OracleClass::TracePredicate],
        ),
        leak_metric(
            SECRET_VALUE_IN_ENVIRONMENT,
            // The end-state projection (spec's `secret_leak_end_state` row —
            // `end_state` observability, `end_state` oracle class).
            &[Observability::EndState],
            &[OracleClass::EndState],
        ),
        leak_metric(
            SECRET_VALUE_IN_REPORT,
            &[Observability::Events],
            &[OracleClass::OutputCheck],
        ),
    ]
}
