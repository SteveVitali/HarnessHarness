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

// ── The spec-named §5g.3 §4 rows (S3.11b — the Stage-3 suite reads these
// spellings; the `secret_value_in_*` set above stays registered for the
// Stage-1/2 batteries — one declaration set, additive) ──────────────────────

/// `secret_leak` — the ledger-observability veto (`veto: true`,
/// `requires_observability: {ledger}`, `requires_mediation: mediated(egress)`,
/// `charged_to: instrument`): a `security.secret.leak_detected` row anywhere
/// in the run trips it (AC-R-2.8.3-3/-11 — the LT-03/LT-11 suite vetoes).
pub const SECRET_LEAK: &str = "secret_leak";
/// `secret_leak_model_io` — the model-io leg (`requires_observability:
/// {model_io}`).
pub const SECRET_LEAK_MODEL_IO: &str = "secret_leak_model_io";
/// `secret_leak_end_state` — the end-state leg (`requires_observability:
/// {end_state}`, `oracle_classes_allowed: {end_state}`).
pub const SECRET_LEAK_END_STATE: &str = "secret_leak_end_state";
/// `secret_detector_miss_rate` — the seeded-corpus detector quality fold
/// (`veto: false`, `level: suite` — LT-12).
pub const SECRET_DETECTOR_MISS_RATE: &str = "secret_detector_miss_rate";

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
        // ── The spec-named §5g.3 §4 rows (Stage 3 — S3.11b) ──────────────
        leak_metric(
            SECRET_LEAK,
            &[Observability::Ledger],
            &[OracleClass::TracePredicate],
        ),
        leak_metric(
            SECRET_LEAK_MODEL_IO,
            &[Observability::ModelIo],
            &[OracleClass::TracePredicate],
        ),
        leak_metric(
            SECRET_LEAK_END_STATE,
            &[Observability::EndState],
            &[OracleClass::EndState],
        ),
        // `secret_detector_miss_rate` — suite level, not a veto: it reports
        // detector quality on the seeded corpus, never a run verdict.
        MetricDeclaration {
            name: SECRET_DETECTOR_MISS_RATE.into(),
            dimension: Dimension::Security,
            level: MetricLevel::Suite,
            value_type: MetricValueType::Decimal,
            direction: Direction::Lower,
            unit: "ppm".into(),
            requires_observability: [Observability::Events].into_iter().collect(),
            applies_to_classes: both_classes(),
            requires_mediation: MediationRequirement::Mediated(MediationChannel::Egress),
            detector_classes_allowed: deterministic_only(),
            oracle_classes_allowed: [OracleClass::TracePredicate].into_iter().collect(),
            veto: false,
            charged_to: ChargedTo::Instrument,
            ..MetricDeclaration::default()
        },
    ]
}
