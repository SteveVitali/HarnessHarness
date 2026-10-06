//! S4.16b — `grounding.summary_fidelity_judged` through the judged-detector
//! emission (R-2.4.2² C2; ADR-0077 d4): the declaration lives in
//! `hh_lab::exemplars` (the boundary companion's optional C1 metric —
//! `headline = false`, `charged_to = instrument`, `detector = judged`,
//! `oracle = judge`, native rows only). `emit_judged` is the deterministic
//! trace predicate over caller-supplied judge verdicts — the judge critic
//! itself is §05f/S4.16c; what lands here is that an admitted judge's
//! verdict lands on the declared row, instrument-charged, exploratory or
//! `n/a{no_detector}` exactly as the calibration/use pair dictates.

use hh_eval::judge::emit_judged;
use hh_eval::judge::JudgeContext;
use hh_lab::exemplars::summary_fidelity_judged;
use hh_ontology::compliance::{Detector, MetricDeclaration, NaReason};
use hh_ontology::eval::{
    ChargedTo, EvidenceKind, MetricValueKind, OracleClass, OracleDeclaration, VerdictType,
};
use hh_ontology::participant::Observability;
use hh_provenance::ProvenanceRecord;
use hh_verification::critics::IndependenceVector;
use hh_verification::vocab::{CalibrationStatus, CriticUse};

fn judge_oracle() -> OracleDeclaration {
    OracleDeclaration {
        oracle_id: "oracle/judge-fidelity".into(),
        class: OracleClass::Judge,
        deterministic: false,
        requires_observability: [Observability::ModelIo].into_iter().collect(),
        verdict_type: VerdictType::Graded,
        evidence_out: vec![EvidenceKind::ModelIo, EvidenceKind::Observation],
        charged_to: ChargedTo::Instrument,
        calibration_ref: Some("calib/judge-fidelity.v1".into()),
        provenance: ProvenanceRecord::kernel("hh-eval/s4_16b", 0),
    }
}

fn ctx(calibration: Option<CalibrationStatus>, use_: CriticUse) -> JudgeContext {
    JudgeContext {
        judge_snapshot: "snap:judge-model.v2".into(),
        judge_family: Some("family/judge".into()),
        beneficiary_snapshots: ["snap:subject-model.v1".to_string()].into_iter().collect(),
        beneficiary_families: ["family/subject".to_string()].into_iter().collect(),
        independence: IndependenceVector::kernel_independent(),
        calibration,
        use_,
    }
}

fn metric() -> MetricDeclaration {
    let m = summary_fidelity_judged();
    m.validate().expect("the declaration is well-formed");
    m
}

/// A caller-supplied judge verdict lands on the declared row — the
/// deterministic predicate stamps `detector = judged`,
/// `charged_to = instrument`, never headline.
#[test]
fn caller_supplied_verdict_emits_on_the_fidelity_row() {
    let m = metric();
    // The declaration admits exactly the judged detector/judge oracle path.
    assert_eq!(
        m.detector_classes_allowed,
        [Detector::Judged].into_iter().collect()
    );
    let e = emit_judged(
        &m,
        &judge_oracle(),
        &ctx(Some(CalibrationStatus::Active), CriticUse::Report),
        MetricValueKind::Decimal(870_000), // the judge's verdict, as data
        "run:boundary-applied",
        Some(900_000),
        Some("ev:judge-fidelity-1".into()),
    )
    .expect("an admitted judge emits on the declared row");
    assert_eq!(e.value.value, MetricValueKind::Decimal(870_000));
    assert_eq!(e.value.detector, Detector::Judged);
    assert_eq!(e.charged_to, ChargedTo::Instrument);
    assert!(!e.exploratory, "calibrated report use is not exploratory");
    // The declared row is never headline — the emission cannot promote it.
    assert!(!m.headline);
}

/// An uncalibrated gate use emits `n/a{no_detector}` — the caller's verdict
/// is discarded on the gating row (never smuggled in).
#[test]
fn uncalibrated_gate_emits_typed_na_not_the_verdict() {
    let e = emit_judged(
        &metric(),
        &judge_oracle(),
        &ctx(None, CriticUse::Gate),
        MetricValueKind::Decimal(870_000),
        "run:boundary-withheld",
        None,
        None,
    )
    .expect("uncalibrated gate use renders the typed n/a, not a refusal");
    assert_eq!(e.value.value, MetricValueKind::Na(NaReason::NoDetector));
    assert_eq!(e.value.detector, Detector::Judged);
    assert_eq!(e.charged_to, ChargedTo::Instrument);
}
