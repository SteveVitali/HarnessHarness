//! S4.15 — judge-oracle admission and the judged-detector emission
//! (AC-R-2.9.2-14; spec §5h.2 C2; ADR-0047 D3/(c)(ii), ADR-0115…0117):
//!
//! - a judge declared with the beneficiary's snapshot is refused
//!   `JudgeNotIndependent` (and `same_context` judges are refused
//!   outright);
//! - a judge without an active calibration renders `n/a{no_detector}` for
//!   gating and `exploratory` for report use;
//! - every judge call is `charged_to = instrument`;
//! - judged emissions carry `detector = judged` under a declaration that
//!   admits it — the deterministic and judged execution-alignment metrics
//!   are distinct catalogue rows, never merged;
//! - hosted rows project their `capability_vector` (the analysis
//!   projection's manifest.extra half is covered in hh-analysis; the row
//!   itself carries it into `applicability`).

use hh_eval::catalogue;
use hh_eval::judge::{admit_judge, emit_judged, JudgeAdmission, JudgeContext, JudgeError};
use hh_ontology::compliance::{Detector, MetricDeclaration, NaReason};
use hh_ontology::eval::{
    ChargedTo, EvidenceKind, MetricValueKind, OracleClass, OracleDeclaration, OracleError,
    VerdictType,
};
use hh_ontology::participant::Observability;
use hh_provenance::ProvenanceRecord;
use hh_verification::critics::IndependenceVector;
use hh_verification::vocab::{
    CalibrationStatus, CapabilityIndependence, ContextIndependence, CriticUse, Optimization,
    ProvenanceIndependence, SnapshotIndependence,
};

fn judge_oracle() -> OracleDeclaration {
    OracleDeclaration {
        oracle_id: "oracle/judge-alignment".into(),
        class: OracleClass::Judge,
        deterministic: false,
        requires_observability: [Observability::ModelIo].into_iter().collect(),
        verdict_type: VerdictType::Bool,
        evidence_out: vec![EvidenceKind::ModelIo, EvidenceKind::Observation],
        charged_to: ChargedTo::Instrument,
        calibration_ref: Some("calib/judge-alignment.v1".into()),
        provenance: ProvenanceRecord::kernel("hh-eval/s4_15", 0),
    }
}

fn judged_metric() -> MetricDeclaration {
    catalogue::metric("execution_alignment_failure_rate_judged")
        .expect("the judged execution-alignment declaration is in the catalogue")
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

// ── admission refusals ────────────────────────────────────────────────────

/// AC-R-2.9.2-14 (first half): a judge declared with the beneficiary's
/// snapshot is refused `JudgeNotIndependent` — for gate *and* report use.
#[test]
fn judge_sharing_beneficiary_snapshot_is_refused() {
    for use_ in [CriticUse::Gate, CriticUse::Report] {
        let mut c = ctx(Some(CalibrationStatus::Active), use_);
        c.judge_snapshot = "snap:subject-model.v1".into();
        assert_eq!(
            admit_judge(&judge_oracle(), &c),
            Err(JudgeError::JudgeNotIndependent {
                axis: "snapshot = same_snapshot".into()
            }),
            "{use_:?}: the beneficiary's own snapshot must refuse"
        );
    }
}

/// A `same_context` judge is not a critic — refused for every use.
#[test]
fn judge_sharing_context_is_refused() {
    for use_ in [CriticUse::Gate, CriticUse::Report] {
        let mut c = ctx(Some(CalibrationStatus::Active), use_);
        c.independence.context = ContextIndependence::SameContext;
        assert_eq!(
            admit_judge(&judge_oracle(), &c),
            Err(JudgeError::JudgeNotIndependent {
                axis: "context = same_context".into()
            })
        );
    }
}

/// A declared `different_family` over a derived `same_family` axis is an
/// inflated basis — refused, never recorded.
#[test]
fn declared_family_overclaim_is_refused() {
    let mut c = ctx(Some(CalibrationStatus::Active), CriticUse::Gate);
    c.judge_family = Some("family/subject".into()); // same family as the beneficiary
    c.independence.snapshot = SnapshotIndependence::DifferentFamily; // overclaim
    assert_eq!(
        admit_judge(&judge_oracle(), &c),
        Err(JudgeError::JudgeNotIndependent {
            axis: "declared snapshot axis overclaims the derived one".into()
        })
    );
}

/// The ADR-0116 D2 gate minimums — each weakened axis refuses
/// `IndependenceBelowMinimum`.
#[test]
fn gate_independence_minimums_bind() {
    type Mutation = Box<dyn Fn(&mut JudgeContext)>;
    let cases: Vec<(Mutation, &str)> = vec![
        (
            Box::new(|c: &mut JudgeContext| {
                c.independence.context = ContextIndependence::SharedTranscript;
            }),
            "context ≠ fresh",
        ),
        (
            Box::new(|c: &mut JudgeContext| {
                c.independence.provenance = ProvenanceIndependence::ModelAuthoredRubric;
            }),
            "provenance = model_authored_rubric",
        ),
        (
            Box::new(|c: &mut JudgeContext| {
                c.independence.capability = CapabilityIndependence::SharedMutable;
            }),
            "capability = shared_mutable",
        ),
        (
            Box::new(|c: &mut JudgeContext| {
                c.independence.optimization = Optimization::InLoopUnbounded;
            }),
            "optimization = in_loop_unbounded",
        ),
    ];
    for (weaken, axis) in cases {
        let mut c = ctx(Some(CalibrationStatus::Active), CriticUse::Gate);
        weaken(&mut c);
        assert_eq!(
            admit_judge(&judge_oracle(), &c),
            Err(JudgeError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: axis.to_string(),
            }),
            "axis {axis} must refuse for gate use"
        );
        // The same weakened vector is admissible for a calibrated report
        // use — the weaker basis is recorded on the verdict, never widened.
        let mut c = ctx(Some(CalibrationStatus::Active), CriticUse::Report);
        weaken(&mut c);
        assert_eq!(
            admit_judge(&judge_oracle(), &c),
            Ok(JudgeAdmission::Admitted { exploratory: false }),
            "axis {axis} must not refuse a calibrated report use"
        );
    }
}

/// A judge whose `evidence_out` is only the beneficiary's claims —
/// `model_io`/`human_attestation` alone — emits no evidence.
#[test]
fn model_claim_only_evidence_is_refused() {
    for evidence_out in [
        vec![EvidenceKind::ModelIo],
        vec![EvidenceKind::HumanAttestation],
        vec![EvidenceKind::ModelIo, EvidenceKind::HumanAttestation],
        vec![],
    ] {
        let mut o = judge_oracle();
        o.evidence_out = evidence_out;
        assert_eq!(
            admit_judge(&o, &ctx(Some(CalibrationStatus::Active), CriticUse::Report)),
            Err(JudgeError::ModelClaimOnly)
        );
    }
}

/// Declaration-level invariants re-checked at the boundary: a
/// `deterministic` judge and a subject-charged judge refuse even when the
/// registry row skipped `validate`.
#[test]
fn declaration_invariants_rechecked() {
    let mut det = judge_oracle();
    det.deterministic = true;
    assert_eq!(
        admit_judge(
            &det,
            &ctx(Some(CalibrationStatus::Active), CriticUse::Report)
        ),
        Err(JudgeError::InvalidDeclaration(
            OracleError::JudgeDeterministic
        ))
    );
    let mut subj = judge_oracle();
    subj.charged_to = ChargedTo::Subject;
    assert_eq!(
        admit_judge(
            &subj,
            &ctx(Some(CalibrationStatus::Active), CriticUse::Report)
        ),
        Err(JudgeError::InvalidDeclaration(
            OracleError::JudgeNotInstrument
        ))
    );
    let mut other = judge_oracle();
    other.class = OracleClass::Executable;
    assert_eq!(
        admit_judge(
            &other,
            &ctx(Some(CalibrationStatus::Active), CriticUse::Report)
        ),
        Err(JudgeError::NotAJudge {
            class: "executable".into()
        })
    );
}

// ── emission ──────────────────────────────────────────────────────────────

/// AC-R-2.9.2-14 (second half): gate use without an active calibration
/// emits `n/a{no_detector}` — the judged verdict never lands on the row.
#[test]
fn uncalibrated_gate_emits_no_detector() {
    for cal in [None, Some(CalibrationStatus::Expired)] {
        let e = emit_judged(
            &judged_metric(),
            &judge_oracle(),
            &ctx(cal, CriticUse::Gate),
            MetricValueKind::Decimal(400_000),
            "run:r1",
            Some(900_000),
            Some("ev:judge-1".into()),
        )
        .expect("uncalibrated gate use is not refused — it renders the typed n/a");
        assert_eq!(e.value.value, MetricValueKind::Na(NaReason::NoDetector));
        assert_eq!(e.value.detector, Detector::Judged);
        assert_eq!(e.charged_to, ChargedTo::Instrument);
        assert!(!e.exploratory);
    }
}

/// …and report use without calibration emits `exploratory` — the verdict
/// lands as report-only evidence, never gate-admissible.
#[test]
fn uncalibrated_report_emits_exploratory() {
    let e = emit_judged(
        &judged_metric(),
        &judge_oracle(),
        &ctx(None, CriticUse::Report),
        MetricValueKind::Decimal(400_000),
        "run:r1",
        None,
        None,
    )
    .expect("report use admits the judged value");
    assert_eq!(e.value.value, MetricValueKind::Decimal(400_000));
    assert!(e.exploratory, "uncalibrated report ⇒ exploratory");
    assert_eq!(e.charged_to, ChargedTo::Instrument);
}

/// A calibrated gate use emits the judged verdict with the derived
/// independence summary recorded.
#[test]
fn calibrated_gate_emits_verdict() {
    let e = emit_judged(
        &judged_metric(),
        &judge_oracle(),
        &ctx(Some(CalibrationStatus::Active), CriticUse::Gate),
        MetricValueKind::Bool(true),
        "run:r1",
        None,
        None,
    )
    .expect("calibrated gate emits");
    assert_eq!(e.value.value, MetricValueKind::Bool(true));
    assert_eq!(e.value.oracle_ref, "oracle/judge-alignment");
    assert_eq!(
        e.calibration_ref.as_deref(),
        Some("calib/judge-alignment.v1")
    );
    assert!(e.independence_summary.contains("snapshot=different_family"));
}

/// A `same_family` derivation is recorded — never rendered
/// `different_family`.
#[test]
fn same_family_derivation_is_recorded() {
    let mut c = ctx(Some(CalibrationStatus::Active), CriticUse::Report);
    c.judge_family = Some("family/subject".into());
    c.independence.snapshot = SnapshotIndependence::DifferentSnapshotSameFamily;
    let e = emit_judged(
        &judged_metric(),
        &judge_oracle(),
        &c,
        MetricValueKind::Decimal(0),
        "run:r1",
        None,
        None,
    )
    .expect("same_family report admits");
    assert!(
        e.independence_summary
            .contains("snapshot=different_snapshot_same_family"),
        "{}",
        e.independence_summary
    );
}

/// A judged value never lands on a declaration that does not admit
/// `judged` detectors or `judge` oracles — the detector classes are
/// separate, never merged.
#[test]
fn judged_value_never_lands_on_deterministic_metric() {
    let det_metric = catalogue::metric("execution_alignment_failure_rate")
        .expect("the deterministic execution-alignment declaration");
    assert_eq!(
        emit_judged(
            &det_metric,
            &judge_oracle(),
            &ctx(Some(CalibrationStatus::Active), CriticUse::Report),
            MetricValueKind::Decimal(0),
            "run:r1",
            None,
            None,
        ),
        Err(JudgeError::DetectorNotAdmitted {
            metric: "execution_alignment_failure_rate".into()
        })
    );
    // And a judged-detector declaration that admits no judge oracle —
    // refused `OracleNotAdmitted`.
    let mut m = judged_metric().clone();
    m.oracle_classes_allowed = [OracleClass::Executable].into_iter().collect();
    assert!(matches!(
        emit_judged(
            &m,
            &judge_oracle(),
            &ctx(Some(CalibrationStatus::Active), CriticUse::Report),
            MetricValueKind::Decimal(0),
            "run:r1",
            None,
            None,
        ),
        Err(JudgeError::OracleNotAdmitted { .. })
    ));
}

/// The two execution-alignment declarations are distinct catalogue rows —
/// the deterministic fold over the ledger and the judged trajectory read
/// never merge (CF-483; ADR-0114).
#[test]
fn execution_alignment_detectors_are_distinct() {
    let det =
        catalogue::metric("execution_alignment_failure_rate").expect("deterministic declaration");
    let jud =
        catalogue::metric("execution_alignment_failure_rate_judged").expect("judged declaration");
    assert_ne!(det.name, jud.name);
    assert_eq!(
        det.detector_classes_allowed,
        [Detector::Deterministic].into_iter().collect()
    );
    assert_eq!(
        jud.detector_classes_allowed,
        [Detector::Judged].into_iter().collect()
    );
    // The deterministic fold is ledger-scoped: a hosted row (events-only
    // observability) renders `n/a{observability}` — the §5f.2-prescribed
    // reason; the class-conditional `end_state`-hosted/D5-only cell lands
    // with the C2 variant. The judged read is model_io-scoped and applies
    // to hosted rows the trajectory is intercepted on.
    assert!(det.requires_observability.contains(&Observability::Ledger));
    assert!(det
        .applies_to_classes
        .contains(&hh_ontology::participant::ParticipantClass::Hosted));
    assert!(jud.requires_observability.contains(&Observability::ModelIo));
    // The catalogue check stays clean with both rows.
    assert!(catalogue::check_catalogue().findings.is_empty());
}
